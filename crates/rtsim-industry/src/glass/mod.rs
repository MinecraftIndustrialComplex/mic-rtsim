// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box (GLASSBOX.md, C28; Ben's G-B1 to G-B19): compiled Create factories.
//!
//! Phase G1 (G-B18, §11.3) is read only: the Java compiler sends each district's graph
//! ([`records`], [`codec`]); the engine stores it ([`store`], [`graph`]), solves it for the
//! measured-behaviour floor L ([`solve`]) and the LP ceiling U per good ([`lp`]), in exact
//! rationals ([`q`]), and checks it against the boundary measurement for one window ([`check`],
//! G-B14). Nothing it computes changes the world: the replay still runs the C19 black box.
//!
//! **Not persisted (GB4).** Java resends the last compile periodically, so a restarted engine
//! has its graph again within `glass.resendTicks`; windows restart after a session start anyway.
//!
//! **Per tick: nothing.** A compile solves when it arrives; a sweep integrates its interval and
//! adds one sample (O(stores × goods)), and solves again only when the regime it reads (stores'
//! held goods and room, controllers) changed and is not in the small cache.
//!
//! Choices made building it are numbered GB# in docs/choices-glassbox-g1.md.

pub mod check;
pub mod codec;
pub mod graph;
pub mod lp;
pub mod q;
pub mod records;
pub mod report;
pub mod solve;
pub mod store;

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use serde::Serialize;

use crate::ids::{DistrictId, GoodVec, VecMap};
use crate::reconcile::GoodLookup;
use crate::tunables::GlassTunables;
use check::{Compiled, DistrictGlass, View};
use records::{GlassObs, ProbeSet};
use report::Report;
use store::EndOutcome;

/// Counters across districts, for operators and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GlassStats {
    pub compiles_accepted: u64,
    pub resends_ignored: u64,
    pub compiles_dropped: u64,
    pub compiles_stale: u64,
    pub not_compilable: u64,
    /// Complete compiles whose indices did not hold together (`graph::Invalid`): dropped.
    pub compiles_invalid: u64,
    pub solves: u64,
    pub windows_completed: u64,
    pub windows_failed: u64,
    pub probe_sets_sent: u64,
    pub demotions: u64,
    pub probe_samples: u64,
}

/// The glass box's state across districts.
#[derive(Debug, Default)]
pub struct Glass {
    /// Glass records received.
    pub records: u64,
    pub districts: BTreeMap<DistrictId, DistrictGlass>,
    pub stats: GlassStats,
}

/// An empty view, for a compile whose district the engine does not know yet.
fn empty_view<'a>(inv: &'a VecMap<u32, crate::district::Inventory>, ports: &'a GoodVec) -> View<'a> {
    View {
        revision: u32::MAX,
        inventories: inv,
        ports,
    }
}

impl Glass {
    /// One glass record. `view` is the district as the engine knows it (None before its
    /// IND_DISTRICT).
    pub fn ingest(
        &mut self,
        at: Tick,
        g: &GlassObs,
        view: Option<&View<'_>>,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) {
        self.records += 1;
        let d = g.district();
        let dg = self.districts.entry(d).or_default();
        let held = dg.compiled.as_ref().map(|c| c.graph.end.compile);
        match g {
            GlassObs::Node(n) => dg.asm.node(n.clone(), held),
            GlassObs::Edge(e) => dg.asm.edge(e.clone(), held),
            GlassObs::Recipe(r) => dg.asm.recipe(r.clone(), held),
            GlassObs::Domain(x) => dg.asm.domain(x.clone(), held),
            GlassObs::End(end) => {
                let (outcome, complete) = dg.asm.end(end, held);
                match outcome {
                    EndOutcome::Complete => {
                        let c = complete.expect("complete");
                        match graph::Graph::build(c.end, c.nodes, c.edges, c.recipes, c.domains) {
                            Ok(graph) => {
                                let mut comp = Compiled::new(graph, at, tun, &dg.demoted);
                                let none_inv = VecMap::new();
                                let none_ports = GoodVec::new();
                                let empty = empty_view(&none_inv, &none_ports);
                                comp.start(view.unwrap_or(&empty), goods, tun);
                                self.stats.solves += u64::from(comp.solves);
                                dg.compiled = Some(comp);
                                dg.not_compilable = None;
                                self.stats.compiles_accepted += 1;
                            }
                            Err(_) => self.stats.compiles_invalid += 1,
                        }
                    }
                    EndOutcome::ResendIgnored => self.stats.resends_ignored += 1,
                    EndOutcome::Dropped => self.stats.compiles_dropped += 1,
                    EndOutcome::Stale => self.stats.compiles_stale += 1,
                    EndOutcome::NotCompilable => {
                        // The C19 box stays the district's only model; an older compile is no
                        // longer the district (GLASSBOX.md §1.4).
                        dg.compiled = None;
                        dg.not_compilable = Some((end.compile, end.reason));
                        self.stats.not_compilable += 1;
                    }
                }
            }
            GlassObs::ProbeSample(p) => {
                if let Some(c) = dg.compiled.as_mut()
                    && c.graph.end.compile == p.compile
                {
                    c.probe_sample(p.sweep, p.node, &p.goods);
                    self.stats.probe_samples += 1;
                }
            }
            GlassObs::ChunkTicking(t) => {
                if t.ticking {
                    dg.ticking.insert(t.chunk);
                } else {
                    dg.ticking.remove(&t.chunk);
                }
            }
        }
    }

    /// A live sweep of `district`, after its inventories were updated (creative and overstressed
    /// sweeps count: the compile models both). Returns the probe set to send, if any.
    pub fn sweep(
        &mut self,
        at: Tick,
        district: DistrictId,
        view: &View<'_>,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Option<ProbeSet> {
        let dg = self.districts.get_mut(&district)?;
        let c = dg.compiled.as_mut()?;
        if view.revision != c.graph.end.revision {
            // The inventory indices name another revision's list: nothing to measure (GB24).
            c.discard(at, tun);
            return None;
        }
        let solves = c.solves;
        let windows = c.windows_completed;
        let (probes, demoted) = c.sweep(at, view, goods, tun);
        self.stats.solves += u64::from(c.solves - solves);
        if c.windows_completed > windows {
            self.stats.windows_completed += 1;
            if c.last
                .as_ref()
                .is_some_and(|r| r.verdict == report::verdict::FAIL)
            {
                self.stats.windows_failed += 1;
            }
        }
        if let Some(m) = demoted {
            dg.demoted.insert(m);
            self.stats.demotions += 1;
        }
        let nodes = probes?;
        self.stats.probe_sets_sent += 1;
        dg.probe_cmd(district, nodes)
    }

    /// A sweep that does not count (not live, or dropped): the open window is discarded.
    pub fn discard(&mut self, at: Tick, district: DistrictId, tun: &GlassTunables) {
        if let Some(c) = self
            .districts
            .get_mut(&district)
            .and_then(|d| d.compiled.as_mut())
        {
            c.discard(at, tun);
        }
    }

    /// A session start: every open window loses the samples that were in flight.
    pub fn session_start(&mut self, at: Tick, tun: &GlassTunables) {
        for dg in self.districts.values_mut() {
            if let Some(c) = dg.compiled.as_mut() {
                c.discard(at, tun);
            }
            dg.asm = store::Assembler::default();
            dg.ticking.clear();
        }
    }

    /// The district is gone.
    pub fn remove(&mut self, district: DistrictId) {
        self.districts.remove(&district);
    }

    /// The district's glass report (`report.rs`), or None if it has no complete compile.
    pub fn report(&self, district: DistrictId) -> Option<Report> {
        let dg = self.districts.get(&district)?;
        dg.compiled.as_ref().map(|c| c.report(dg.demoted.len()))
    }
}
