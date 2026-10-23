// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box (GLASSBOX.md, C28; Ben's G-B1 to G-B19): compiled Create factories.
//!
//! Phase G1 (G-B18, §11.3) is read only: the Java compiler sends each district's graph
//! ([`records`], [`codec`]); the engine stores it ([`store`], [`graph`]), solves it for the
//! measured-behaviour floor L ([`solve`]) and the LP ceiling U per good ([`lp`]), in exact
//! rationals ([`q`]), and checks it against the boundary measurement for one window ([`check`],
//! G-B14). Nothing it computes changes the world: the replay still runs the C19 black box.
//!
//! **Paged (G2-9).** Each district's glass state is a page of its own ([`page`]): the compile, its
//! passed window and the glass replay's regime state, so a restart keeps the glass replay instead
//! of dropping the district to the black box. Java numbers each session's compiles from 1: after
//! a session start the first END that compiles the same factory keeps the compile held (G2-81).
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
pub mod page;
pub mod q;
pub mod records;
pub mod replay;
pub mod report;
pub mod sections;
pub mod solve;
pub mod store;
#[cfg(test)]
mod tests;

use rtsim_kernel::clock::Tick;
use serde::Serialize;

use crate::ids::{DistrictId, GoodVec, VecMap};
use crate::reconcile::GoodLookup;
use crate::tunables::GlassTunables;
use check::{Compiled, DistrictGlass, View};
use records::{GlassObs, ProbeSet};
use report::{Report, discard};
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
    /// A new session's first compile of the same factory: the compile held is kept under its
    /// number (G2-81).
    pub compiles_kept: u64,
    pub solves: u64,
    pub windows_completed: u64,
    pub windows_failed: u64,
    pub probe_sets_sent: u64,
    pub demotions: u64,
    pub probe_samples: u64,
    /// Glass replays started, and those that fell back to the black box (§4.5).
    pub replays: u64,
    pub fallbacks: u64,
}

/// The glass box's state across districts.
#[derive(Debug, Default)]
pub struct Glass {
    /// Glass records received.
    pub records: u64,
    /// One page each (G2-80), written only when changed since the last save (ID1): every `&mut`
    /// to an entry marks it.
    pub districts: crate::tracked::Tracked<DistrictId, DistrictGlass>,
    pub stats: GlassStats,
    /// Districts whose glass page the restore dropped (`page::PageRead::Dropped`: the format
    /// before sections, or sections their compile no longer cuts into), for the host's one line.
    /// Not saved.
    pub dropped_on_restore: Vec<(DistrictId, &'static str)>,
}

/// An empty view, for a compile whose district the engine does not know yet.
fn empty_view<'a>(
    inv: &'a VecMap<u32, crate::district::Inventory>,
    ports: &'a GoodVec,
) -> View<'a> {
    View {
        revision: u32::MAX,
        inventories: inv,
        ports,
    }
}

/// Does a new session's compile describe the factory the held compile does (G2-81)? The same
/// END (fingerprint, the test Java itself uses for "the compile still holds", G-B3; revision,
/// frame, window and counts) but for its number, a resend flag and the ticks it spanned, and the
/// same records but for the compile number and what is the factory's state at the compile rather
/// than its build ([`state_slots`]; a domain's level, live stress and live capacity).
/// A node's `params` that are its state at the compile, not its build: a heater's burn left and
/// heat level, a controller's output and the level it read. The rest (thresholds, burn per fuel,
/// a tank's size, a source's moments) are its build.
pub(crate) fn state_slots(kind: records::NodeKind) -> &'static [usize] {
    match kind {
        records::NodeKind::Heater => &[0, 1],
        records::NodeKind::Controller => &[2, 3],
        _ => &[],
    }
}

fn same_factory(held: &graph::Graph, c: &store::Complete) -> bool {
    use records::end_flags::RESEND;
    let (a, b) = (&held.end, &c.end);
    let head = |x: records::GraphHead, y: records::GraphHead| {
        x.district == y.district && x.index == y.index
    };
    let end = records::GraphEnd {
        compile: b.compile,
        flags: b.flags,
        compile_ticks: b.compile_ticks,
        ..*a
    } == *b
        && a.flags & !RESEND == b.flags & !RESEND;
    let node = |x: &records::GraphNode, y: &records::GraphNode| {
        let mut params = x.params;
        for &i in state_slots(x.kind) {
            params[i] = y.params[i];
        }
        head(x.head, y.head)
            && records::GraphNode {
                head: y.head,
                params,
                ..x.clone()
            } == *y
    };
    let edge = |x: &records::GraphEdge, y: &records::GraphEdge| {
        head(x.head, y.head)
            && records::GraphEdge {
                head: y.head,
                ..x.clone()
            } == *y
    };
    let recipe = |x: &records::GraphRecipe, y: &records::GraphRecipe| {
        head(x.head, y.head)
            && records::GraphRecipe {
                head: y.head,
                ..x.clone()
            } == *y
    };
    let domain = |x: &records::GraphDomain, y: &records::GraphDomain| {
        head(x.head, y.head)
            && x.network == y.network
            && x.driver == y.driver
            && x.flags == y.flags
            && x.levels == y.levels
            && x.members == y.members
    };
    fn all<T>(x: &[T], y: &[T], f: impl Fn(&T, &T) -> bool) -> bool {
        x.len() == y.len() && x.iter().zip(y).all(|(x, y)| f(x, y))
    }
    end && all(&held.nodes, &c.nodes, node)
        && all(&held.edges, &c.edges, edge)
        && all(&held.recipes, &c.recipes, recipe)
        && all(&held.domains, &c.domains, domain)
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
        let dg = self.districts.get_or_insert_with(d, DistrictGlass::default);
        // After a session start, the held compile's number is the last session's: Java's new
        // numbers are neither older nor a resend of it (G2-81).
        let held = dg
            .compiled
            .as_ref()
            .filter(|_| !dg.numbers_reset)
            .map(|c| c.graph.end.compile);
        match g {
            GlassObs::Node(n) => dg.asm.node(n.clone(), held),
            GlassObs::Edge(e) => dg.asm.edge(e.clone(), held),
            GlassObs::Recipe(r) => dg.asm.recipe(r.clone(), held),
            GlassObs::Domain(x) => dg.asm.domain(x.clone(), held),
            GlassObs::End(end) => {
                let (outcome, complete) = dg.asm.end(end, held);
                if matches!(outcome, EndOutcome::Complete | EndOutcome::NotCompilable) {
                    let reset = std::mem::take(&mut dg.numbers_reset);
                    if let (true, Some(c), Some(comp)) =
                        (reset, complete.as_ref(), dg.compiled.as_mut())
                        && same_factory(&comp.graph, c)
                    {
                        // The same factory as the compile held: it stays, with its passed window
                        // and its replay, under the new session's number (Java names probes and
                        // compiles by it). Java forgot the probe set.
                        comp.graph.end.compile = c.end.compile;
                        comp.probe_set.clear();
                        comp.probes.clear();
                        if let Some(rp) = dg.replay.as_mut() {
                            rp.renumber(c.end.compile);
                        }
                        self.stats.compiles_kept += 1;
                        return;
                    }
                }
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
                                // The old compile's window (open or warming up) goes with it.
                                if dg.compiled.is_some() {
                                    dg.note(at, discard::RECOMPILE);
                                }
                                dg.compiled = Some(comp);
                                dg.not_compilable = None;
                                // A new compile gets its own chance at the glass replay (§4.5).
                                dg.fallback = None;
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
        let revision = dg.compiled.as_ref()?.graph.end.revision;
        if view.revision != revision {
            // The inventory indices name another revision's list: nothing to measure (GB24).
            dg.note(at, discard::REVISION);
            let c = dg.compiled.as_mut()?;
            c.district_revision = Some(view.revision);
            c.discard(at, tun);
            return None;
        }
        let c = dg.compiled.as_mut()?;
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

    /// A sweep that does not count, or a change that ends the window, for a [`discard`] reason. A
    /// sweep without its samples (NO_SWEEP, DROPPED) is a skip: nothing is sampled and the window
    /// goes on, integrating across the gap (GB101). The rest discard the open window, and the next
    /// starts after a fresh warm-up.
    pub fn discard(&mut self, at: Tick, district: DistrictId, reason: u32, tun: &GlassTunables) {
        let Some(dg) = self.districts.get_mut(&district) else {
            return;
        };
        if dg.compiled.is_none() {
            return;
        }
        dg.note(at, reason);
        if matches!(reason, discard::NO_SWEEP | discard::DROPPED) {
            return;
        }
        if let Some(c) = dg.compiled.as_mut() {
            c.discard(at, tun);
            // An edit: the compile no longer matches the blocks until a window passes again or
            // Java recompiles (G-B3, G-B4); no glass replay meanwhile.
            if reason == discard::EDIT {
                c.edited = true;
            }
        }
    }

    /// A session start: every open window loses the samples that were in flight.
    pub fn session_start(&mut self, at: Tick, tun: &GlassTunables) {
        for dg in self.districts.values_mut() {
            if dg.compiled.is_some() {
                dg.note(at, discard::SESSION);
            }
            if let Some(c) = dg.compiled.as_mut() {
                c.discard(at, tun);
            }
            dg.asm = store::Assembler::default();
            dg.ticking.clear();
            dg.numbers_reset = dg.compiled.is_some();
        }
    }

    /// The district is gone.
    pub fn remove(&mut self, district: DistrictId) {
        self.districts.remove(&district);
    }

    /// The district's glass report (`report.rs`), or None if it has no complete compile.
    pub fn report(&self, district: DistrictId) -> Option<Report> {
        let dg = self.districts.get(&district)?;
        dg.compiled.as_ref().map(|c| c.report(dg))
    }
}
