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
pub mod units;

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use serde::Serialize;

use crate::ids::{BlockPos, ChunkPos, DistrictId, GoodVec, VecMap};
use crate::reconcile::GoodLookup;
use crate::tunables::GlassTunables;
use check::{Compiled, DistrictGlass, Liveness, View};
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

/// This run's count traffic (SECTIONS.md §4.2), for operators and tests. Not saved (the globals
/// page keeps [`GlassStats`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CountStats {
    /// IND_COUNT_SAMPLEs read for the compile held.
    pub samples: u64,
    /// IND_COUNT_SETs sent.
    pub sets_sent: u64,
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
    pub count_stats: CountStats,
    /// Districts whose glass page the restore dropped (`page::PageRead::Dropped`: the format
    /// before sections, or sections their compile no longer cuts into), for the host's one line.
    /// Not saved.
    pub dropped_on_restore: Vec<(DistrictId, &'static str)>,
    /// Per district with active units: its units' extra rows (SECTIONS.md §7.2), as of its last
    /// sweep or restore, and their sum, for the row caps. Not saved: recounted on restore.
    extra: BTreeMap<DistrictId, usize>,
    extra_total: usize,
    /// Per district: the count set Java holds, as (compile, pairs) of the last IND_COUNT_SET sent
    /// with pairs (SECTIONS.md §4.2); none for a district Java counts nothing in. Not saved: a
    /// restore is a new session, and Java counts nothing until told.
    count_held: BTreeMap<DistrictId, (u32, Vec<(u32, u32)>)>,
    /// Districts whose units may have changed since their count set was checked.
    count_dirty: BTreeSet<DistrictId>,
}

/// The compile of a held entry Java's count set is not known for (a resend END): the next check
/// sends the set, even an empty one.
const COUNT_UNKNOWN: u32 = u32::MAX;

/// An empty view, for a compile whose district the engine does not know yet.
fn empty_view<'a>(
    inv: &'a VecMap<u32, crate::district::Inventory>,
    ports: &'a GoodVec,
) -> View<'a> {
    View {
        revision: u32::MAX,
        sweep: 0,
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
/// heat level, a boiler's active heat and water supply (the solve takes its level from its heat
/// edges and its water inflow, SC52), a controller's output and the level it read. The rest
/// (thresholds, burn per fuel, a tank's size, a boiler's engines, a source's moments) are its build.
pub(crate) fn state_slots(kind: records::NodeKind) -> &'static [usize] {
    match kind {
        records::NodeKind::Heater => &[0, 1],
        records::NodeKind::Boiler => &[2, 3],
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
                        // and the last session's count sample, whose sweep numbers it may collide with.
                        comp.counts = None;
                        if let Some(rp) = dg.replay.as_mut() {
                            rp.renumber(c.end.compile);
                        }
                        self.stats.compiles_kept += 1;
                        // Java's count set went with the session: the next check sends it anew.
                        self.count_dirty.insert(d);
                        return;
                    }
                }
                match outcome {
                    EndOutcome::Complete => {
                        let c = complete.expect("complete");
                        match graph::Graph::build(c.end, c.nodes, c.edges, c.recipes, c.domains) {
                            Ok(graph) => {
                                let max = dg.sections_cap.map_or(tun.sections_max, |c| {
                                    (c as usize).min(tun.sections_max)
                                });
                                let joins = match &dg.joins {
                                    Some((rev, sets)) if *rev == graph.end.revision => sets.clone(),
                                    _ => Vec::new(),
                                };
                                let mut comp =
                                    Compiled::new_cut(graph, at, tun, &dg.demoted, max, &joins);
                                let none_inv = VecMap::new();
                                let none_ports = GoodVec::new();
                                let empty = empty_view(&none_inv, &none_ports);
                                comp.start(view.unwrap_or(&empty), goods, tun);
                                self.stats.solves += u64::from(comp.solves);
                                // The old compile's district window (open or warming up) goes
                                // with it; its units the new compile has unchanged go on (§3.6),
                                // when the engine knows the district: the new compile's first
                                // solve reads its stores.
                                if let Some(old) = dg.compiled.as_mut() {
                                    // Nor does the district's veto go with it (choice SC66).
                                    comp.veto = old.district_failing();
                                    if view.is_some() {
                                        let solves = old.solves;
                                        comp.carry_from(old, at, tun);
                                        self.stats.solves += u64::from(old.solves - solves);
                                    }
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
                    EndOutcome::ResendIgnored => {
                        // Java may no longer hold the count set it was sent: send it again.
                        self.stats.resends_ignored += 1;
                        // Only a district whose set Java may have held (one never sent is not sent an empty one).
                        if self.count_held.contains_key(&d) {
                            self.count_held.insert(d, (COUNT_UNKNOWN, Vec::new()));
                        }
                    }
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
                self.recount(d, tun);
            }
            GlassObs::ProbeSample(p) => {
                if let Some(c) = dg.compiled.as_mut()
                    && c.graph.end.compile == p.compile
                {
                    c.probe_sample(p.sweep, p.node, &p.goods);
                    self.stats.probe_samples += 1;
                }
            }
            GlassObs::CountSample(s) => {
                // Of another compile (one Java sent before the set it counts for now): its node
                // numbers name other blocks, and the units it covers are not covered.
                if let Some(c) = dg.compiled.as_mut().filter(|_| !dg.numbers_reset)
                    && c.graph.end.compile == s.compile
                {
                    c.count_sample(s.sweep, &s.counts);
                    self.count_stats.samples += 1;
                }
            }
            GlassObs::ChunkTicking(t) => {
                if t.ticking {
                    dg.ticking.insert(t.chunk);
                } else {
                    dg.ticking.remove(&t.chunk);
                    dg.stopped.insert(t.chunk);
                }
            }
        }
    }

    /// A sweep of `district`, after its inventories were updated (creative and overstressed
    /// sweeps count: the compile models both). `live` says which chunks ran through it: a live
    /// sweep (every chunk did) samples the district's window and every unit's; a partial one
    /// discards the district's window (`NOT_LIVE`) and samples the units whose chunks all ran
    /// (SECTIONS.md §3.4), discarding the others'. Returns the probe set to send, if any.
    pub fn sweep(
        &mut self,
        at: Tick,
        district: DistrictId,
        view: &View<'_>,
        live: &Liveness<'_>,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Option<ProbeSet> {
        let out = self.sweep_inner(at, district, view, live, goods, tun);
        if let Some(dg) = self.districts.get_mut(&district) {
            dg.stopped.clear();
        }
        self.recount(district, tun);
        self.enforce_caps(at, district, tun);
        out
    }

    fn sweep_inner(
        &mut self,
        at: Tick,
        district: DistrictId,
        view: &View<'_>,
        live: &Liveness<'_>,
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
        let c = dg.compiled.as_ref()?;
        let nu = c.unit_state.len();
        let (ticking, stopped) = (&dg.ticking, &dg.stopped);
        // While the district is away no unit runs, whatever a sweep still in flight says; its return
        // starts each one's warm-up afresh ([`Glass::back`], choice SC59).
        let units_run = |runs: &dyn Fn(i32, i32) -> bool| -> Vec<bool> {
            if dg.away {
                return vec![false; nu];
            }
            (0..nu).map(|u| c.units.runs(u as u16, runs)).collect()
        };
        let (district_live, unit_live) = match live {
            // Every chunk ran through the sweep, but one may have stopped since the last sweep
            // that sampled, during a sweep that was dropped (choice SC67): its units did not run
            // throughout. Live has no dimension; the district's chunks lie in one.
            Liveness::All if stopped.is_empty() && !dg.away => (true, vec![true; nu]),
            Liveness::All => {
                let xz: BTreeSet<(i32, i32)> = stopped.iter().map(|c| (c.x, c.z)).collect();
                (true, units_run(&|x, z| !xz.contains(&(x, z))))
            }
            // A unit runs only in chunks Java reported ticking (choice SC58): Java reports no
            // chunk that never ticked, so an empty set says nothing ran, whatever G2-17 reads for
            // the district's window.
            Liveness::Partial { loaded, dim } => {
                let runs = |x: i32, z: i32| {
                    let cp = ChunkPos { dim: *dim, x, z };
                    loaded.contains(&cp) && ticking.contains(&cp) && !stopped.contains(&cp)
                };
                (false, units_run(&runs))
            }
        };
        if !district_live {
            // Today's rule for the district's window (`NOT_LIVE`); with no unit that ran, the
            // sweep is today's discard of everything.
            dg.note(at, discard::NOT_LIVE);
            let c = dg.compiled.as_mut()?;
            if !unit_live.iter().any(|&r| r) {
                c.discard(at, tun);
                return None;
            }
            c.discard_district(at, tun);
        }
        let c = dg.compiled.as_mut()?;
        let solves = c.solves;
        let windows = c.windows_completed;
        let (probes, demoted) = c.sweep_with(at, view, goods, tun, district_live, &unit_live);
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
    /// sweep without its samples (NO_SWEEP, DROPPED) is a skip: nothing is sampled and the windows
    /// go on, integrating across the gap (GB101). The rest discard every open window, the
    /// district's and each unit's, and the next start after a fresh warm-up; an `EDIT` with no
    /// place stales every unit ([`Glass::edit`] for one with a place).
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
            if reason == discard::EDIT {
                c.edit(at, None, tun);
            } else {
                c.discard(at, tun);
            }
        }
        self.recount(district, tun);
    }

    /// An `IND_EDIT` at `pos` (SECTIONS.md §3.6): the district's window goes and the compile no
    /// longer matches the blocks until a window passes again or Java recompiles (G-B3, G-B4; no
    /// glass replay meanwhile). Of its units, those whose box holds `pos` go stale and lose their
    /// windows; every unit does for a `whole` edit (a command's).
    pub fn edit(
        &mut self,
        at: Tick,
        district: DistrictId,
        pos: BlockPos,
        whole: bool,
        tun: &GlassTunables,
    ) {
        let Some(dg) = self.districts.get_mut(&district) else {
            return;
        };
        if dg.compiled.is_none() {
            return;
        }
        dg.note(at, discard::EDIT);
        if let Some(c) = dg.compiled.as_mut() {
            c.edit(at, (!whole).then_some(pos), tun);
        }
        self.recount(district, tun);
    }

    /// Goods ported into (positive) or out of inventory `inv` of the district's revision
    /// `revision`: kept per inventory a unit holds, which its measured series subtracts (§4.4).
    pub fn port(&mut self, district: DistrictId, revision: u32, inv: u32, goods: &GoodVec) {
        let wanted = self
            .districts
            .get(&district)
            .and_then(|dg| dg.compiled.as_ref())
            .is_some_and(|c| {
                c.units.active()
                    && c.graph.end.revision == revision
                    && (c.units.unit_of_inv(inv).is_some()
                        || c.units.counted_store_of_inv(inv).is_some())
            });
        if !wanted {
            return;
        }
        let Some(c) = self
            .districts
            .get_mut(&district)
            .and_then(|dg| dg.compiled.as_mut())
        else {
            return;
        };
        let mut v = c.ports_at.get(&inv).cloned().unwrap_or_default();
        v.merge(goods);
        c.ports_at.insert(inv, v);
    }

    /// The inventory sets the district's stations' ports name now (IND_STATION_PORTS; each
    /// sorted, empty ones left out), numbering the inventories of `revision` (SECTIONS.md §4.4,
    /// choice SC75): a station's cargo cannot be split between its ports, so the units holding
    /// one station's ports join into one. Kept for the district's next compiles; its compile of
    /// that revision regroups its units now, each unit that stays as it was keeping its window.
    pub fn station_ports(
        &mut self,
        at: Tick,
        district: DistrictId,
        revision: u32,
        mut sets: Vec<Vec<u32>>,
        tun: &GlassTunables,
    ) {
        sets.retain(|s| s.len() > 1);
        sets.sort_unstable();
        sets.dedup();
        let dg = self
            .districts
            .get_or_insert_with(district, DistrictGlass::default);
        let joins = Some((revision, sets));
        if dg.joins == joins {
            return;
        }
        dg.joins = joins;
        let Some(c) = dg.compiled.as_mut() else {
            return;
        };
        if c.graph.end.revision != revision {
            return;
        }
        let sets = dg.joins.as_ref().map(|j| j.1.clone()).unwrap_or_default();
        if c.regroup(&sets, at, tun) {
            self.recount(district, tun);
        }
    }

    /// A train's cargo at a station whose last ports named `inventories` (SECTIONS.md §4.4, choice
    /// SC75): with judging units, cargo whose named inventories all lie in one unit is that unit's
    /// port, booked at the first of them; cargo naming inventories of several units, or of a unit
    /// and of none, cannot be split between them, and those units lose their open windows (S3's
    /// judging groups by residue would judge them together). Cargo naming no inventory, or none a
    /// unit holds, is the district's alone, as before.
    pub fn cargo(
        &mut self,
        at: Tick,
        district: DistrictId,
        revision: u32,
        inventories: &[u32],
        goods: &GoodVec,
        tun: &GlassTunables,
    ) {
        if goods.is_empty() || inventories.is_empty() {
            return;
        }
        let Some(c) = self
            .districts
            .get(&district)
            .and_then(|dg| dg.compiled.as_ref())
        else {
            return;
        };
        if !c.units.active() || c.graph.end.revision != revision {
            return;
        }
        let of: Vec<Option<u16>> = inventories
            .iter()
            .map(|&i| c.units.unit_of_inv(i))
            .collect();
        let mut hit: Vec<u16> = of.iter().flatten().copied().collect();
        hit.sort_unstable();
        hit.dedup();
        if hit.is_empty() {
            return;
        }
        if hit.len() == 1 && of.iter().all(Option::is_some) {
            let inv = inventories[0];
            self.port(district, revision, inv, goods);
            return;
        }
        let Some(dg) = self.districts.get_mut(&district) else {
            return;
        };
        dg.note(at, discard::STATION);
        if let Some(c) = dg.compiled.as_mut() {
            for u in hit {
                c.discard_unit(u, at, tun);
            }
        }
        self.recount(district, tun);
    }

    /// A station's `Other` discard (SECTIONS.md §3.6): the district's window, and the units holding
    /// the inventories its last ports named; every unit when it named none.
    pub fn station_discard(
        &mut self,
        at: Tick,
        district: DistrictId,
        inventories: &[u32],
        tun: &GlassTunables,
    ) {
        let Some(dg) = self.districts.get_mut(&district) else {
            return;
        };
        let Some(c) = dg.compiled.as_mut() else {
            return;
        };
        let mut hit: Vec<u16> = inventories
            .iter()
            .filter_map(|&i| c.units.unit_of_inv(i))
            .collect();
        hit.sort_unstable();
        hit.dedup();
        if inventories.is_empty() {
            c.discard(at, tun);
        } else {
            for u in hit {
                c.discard_unit(u, at, tun);
            }
            c.discard_district(at, tun);
        }
        dg.note(at, discard::STATION);
        self.recount(district, tun);
    }

    /// One good ported into (positive) or out of an inventory: [`Glass::port`] for one good.
    pub fn port_one(
        &mut self,
        district: DistrictId,
        revision: u32,
        inv: u32,
        good: crate::ids::Good,
        amount: i64,
    ) {
        if amount == 0 {
            return;
        }
        let mut v = GoodVec::new();
        v.add(good, amount);
        self.port(district, revision, inv, &v);
    }

    /// Chunk `chunk` of the district unloaded: a unit lying in it did not run through the sweep
    /// in progress (§3.4).
    pub fn chunk_stopped(&mut self, district: DistrictId, chunk: ChunkPos) {
        let wanted = self
            .districts
            .get(&district)
            .and_then(|dg| dg.compiled.as_ref())
            .is_some_and(|c| c.units.active());
        if let Some(dg) = self.districts.get_mut(&district).filter(|_| wanted) {
            dg.stopped.insert(chunk);
        }
    }

    /// The district went away (unloaded): no unit window spans the stretch it did not run, whether
    /// or not Java sent each chunk's unload, and no unit runs until it is back ([`Glass::back`]),
    /// whatever a sweep still in flight says (choice SC59).
    pub fn away(&mut self, at: Tick, district: DistrictId, tun: &GlassTunables) {
        let wanted = self
            .districts
            .get(&district)
            .and_then(|dg| dg.compiled.as_ref())
            .is_some_and(|c| c.units.active());
        if !wanted {
            return;
        }
        if let Some(dg) = self.districts.get_mut(&district) {
            dg.away = true;
            if let Some(c) = dg.compiled.as_mut() {
                for u in 0..c.unit_state.len() as u16 {
                    c.discard_unit(u, at, tun);
                }
            }
        }
        self.recount(district, tun);
    }

    /// The district is back (loaded again after [`Glass::away`]): each unit warms up afresh from
    /// here (choice SC59).
    pub fn back(&mut self, at: Tick, district: DistrictId, tun: &GlassTunables) {
        let Some(dg) = self.districts.get_mut(&district).filter(|dg| dg.away) else {
            return;
        };
        dg.away = false;
        if let Some(c) = dg.compiled.as_mut() {
            for u in 0..c.unit_state.len() as u16 {
                c.discard_unit(u, at, tun);
            }
        }
        self.recount(district, tun);
    }

    /// Recounts the district's extra rows into the row caps' totals.
    fn recount(&mut self, district: DistrictId, _tun: &GlassTunables) {
        let now = self
            .districts
            .get(&district)
            .and_then(|dg| dg.compiled.as_ref())
            .map_or(0, |c| c.extra_rows());
        // Every change to a district's units comes through here.
        self.count_dirty.insert(district);
        let was = if now == 0 {
            self.extra.remove(&district).unwrap_or(0)
        } else {
            self.extra.insert(district, now).unwrap_or(0)
        };
        self.extra_total = self.extra_total - was + now;
    }

    /// Recounts every district's extra rows (after a restore).
    pub fn recount_rows(&mut self) {
        self.extra.clear();
        self.extra_total = 0;
        for (&d, dg) in self.districts.iter() {
            let n = dg.compiled.as_ref().map_or(0, |c| c.extra_rows());
            if n > 0 {
                self.extra.insert(d, n);
                self.extra_total += n;
            }
        }
    }

    /// Every district's units' extra rows, summed (SECTIONS.md §7.2).
    pub fn extra_rows(&self) -> usize {
        self.extra_total
    }

    /// The row caps (SECTIONS.md §7.2): a district past `glass.sectionRowsMax` extra rows is cut
    /// into fewer sections until it holds no more; while the world is past
    /// `glass.sectionRowsWorldMax`, the district with the most (the lowest id of equals) is cut
    /// down by the excess. A cut holds for the district's later compiles (choice SC57).
    fn enforce_caps(&mut self, at: Tick, district: DistrictId, tun: &GlassTunables) {
        if self.extra.get(&district).copied().unwrap_or(0) > tun.section_rows_max {
            self.cap(at, district, tun.section_rows_max, tun);
        }
        while self.extra_total > tun.section_rows_world_max {
            let Some((&d, &n)) = self
                .extra
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            else {
                break;
            };
            let limit = n.saturating_sub(self.extra_total - tun.section_rows_world_max);
            if !self.cap(at, d, limit, tun) {
                break;
            }
        }
    }

    /// Cuts the district into the most sections below its count whose rows stay within `limit`
    /// extra rows, and keeps the cut. False when it holds one section already.
    fn cap(&mut self, at: Tick, district: DistrictId, limit: usize, tun: &GlassTunables) -> bool {
        let Some(dg) = self.districts.get_mut(&district) else {
            return false;
        };
        let Some(c) = dg.compiled.as_mut() else {
            return false;
        };
        let have = c.sections.list.len();
        if have <= 1 {
            return false;
        }
        let n = c.sections_for_rows(limit, have, tun);
        c.recut(n, at, tun);
        // Not a discard (`report::discard` is Java's report layout): the units it keeps keep
        // their windows, and the CLI shows the cap.
        dg.sections_cap = Some(n as u32);
        self.recount(district, tun);
        true
    }

    /// A session start: every open window loses the samples that were in flight.
    pub fn session_start(&mut self, at: Tick, tun: &GlassTunables) {
        for dg in self.districts.values_mut() {
            if dg.compiled.is_some() {
                dg.note(at, discard::SESSION);
            }
            if let Some(c) = dg.compiled.as_mut() {
                c.discard(at, tun);
                // A new session's sweep numbers start over: the last session's sample is not theirs.
                c.counts = None;
            }
            dg.asm = store::Assembler::default();
            dg.ticking.clear();
            dg.stopped.clear();
            // Every unit warms up afresh from the discard.
            dg.away = false;
            dg.numbers_reset = dg.compiled.is_some();
        }
        // The windows' rows went with them.
        self.recount_rows();
        // A new session's Java counts nothing until its compiles' sets come.
        self.count_held.clear();
    }

    /// The district is gone.
    pub fn remove(&mut self, district: DistrictId) {
        self.districts.remove(&district);
        self.count_held.remove(&district);
        self.count_dirty.remove(&district);
        if let Some(n) = self.extra.remove(&district) {
            self.extra_total -= n;
        }
    }

    /// The IND_COUNT_SETs to send (SECTIONS.md §4.2): for each district whose units changed, the
    /// pairs (mover, store) at its counted stores between units, when they differ from the set
    /// Java holds or name another compile; an empty set when Java holds one and the district has
    /// none now (no compile, one unit, a compile of the last session's numbers).
    pub fn take_count_sets(&mut self) -> Vec<records::CountSet> {
        let mut out = Vec::new();
        for d in std::mem::take(&mut self.count_dirty) {
            let want = self
                .districts
                .get(&d)
                .filter(|dg| !dg.numbers_reset)
                .and_then(|dg| dg.compiled.as_ref())
                .filter(|c| c.units.active() && !c.units.pairs.is_empty())
                .map(|c| (c.graph.end.compile, &c.units.pairs));
            let held = self.count_held.get(&d);
            match (want, held) {
                (None, None) => continue,
                (Some((compile, pairs)), Some((h, hp))) if *h == compile && hp == pairs => continue,
                _ => {}
            }
            let set = match want {
                Some((compile, pairs)) => records::CountSet {
                    district: d,
                    compile,
                    pairs: pairs.clone(),
                },
                None => records::CountSet {
                    district: d,
                    compile: self
                        .districts
                        .get(&d)
                        .and_then(|dg| dg.compiled.as_ref())
                        .map_or(0, |c| c.graph.end.compile),
                    pairs: Vec::new(),
                },
            };
            if set.pairs.is_empty() {
                self.count_held.remove(&d);
            } else {
                self.count_held.insert(d, (set.compile, set.pairs.clone()));
            }
            self.count_stats.sets_sent += 1;
            out.push(set);
        }
        out
    }

    /// The district's glass report (`report.rs`), or None if it has no complete compile.
    pub fn report(&self, district: DistrictId) -> Option<Report> {
        let dg = self.districts.get(&district)?;
        dg.compiled.as_ref().map(|c| c.report(dg))
    }
}
