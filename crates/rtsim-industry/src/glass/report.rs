// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the glass box reports per district (G1, read only): the solve's bounds per good and the
//! checksum's verdict against one window of live sweeps (GLASSBOX.md §3.5, §5.3; the meaning of each
//! number is docs/industry-boundary.md §9.3). Operators read it through `/rtsim district` and the
//! server log, GameTests through the JNI inspection call; nothing in the world shows it (I18).

use crate::ids::BlockPos;

/// How a good's prediction is judged (docs/industry-boundary.md §9.3).
pub mod class {
    /// Every rule on its paths is exact: judged against `in_l - out_l`.
    pub const EXACT: u32 = 0;
    /// A bounded rule, node or unconverged round on its paths: judged against [L, U].
    pub const BOUNDED: u32 = 1;
    /// A black box or a port on its paths: reported, never judged.
    pub const UNBOUNDED: u32 = 2;
}

/// Why a good's class is what it is (per good in the report, with the node or edge it came from):
/// the first reason found at the worst taint on the good's paths (GB104).
pub mod why {
    pub const NONE: u32 = 0;
    /// A node's BOUNDED or CALIBRATED flag (Java's).
    pub const NODE_FLAG: u32 = 1;
    /// A bounded rule (randomize, synchronize, merge, side insert) splitting over several sides.
    pub const RULE: u32 = 2;
    /// A cycle.
    pub const CYCLE: u32 = 3;
    pub const BLACK_BOX: u32 = 4;
    pub const PORT: u32 = 5;
    /// A store the sampler does not read, passing goods on.
    pub const UNREAD_STORE: u32 = 6;
    /// An edge's BOUNDED flag (Java's). At: the edge.
    pub const EDGE_FLAG: u32 = 7;
    /// A LINE edge (a depot worked by two stations, GB37). At: the edge.
    pub const LINE: u32 = 8;
    /// A DROP edge onto a collector (not a sink). At: the edge.
    pub const DROP: u32 = 9;
    /// The rounds stopped before the fixed point. At: the edge still settling, or none.
    pub const UNCONVERGED: u32 = 10;
    /// An exact good whose U is infinite.
    pub const U_INFINITE: u32 = 11;
    /// An unlimited flow arriving (an unlimited source into an unlimited sink).
    pub const UNLIMITED_FLOW: u32 = 12;
    /// An edge's capacity shared by two goods flowing on it.
    pub const SHARED_EDGE: u32 = 13;
    /// A machine short of inputs for several flows.
    pub const SHORT_INPUTS: u32 = 14;
    /// A node's capacity shared by several flows (not a mover).
    pub const SHARED_CAPACITY: u32 = 15;
    /// A machine time-sharing several recipes.
    pub const TIME_SHARE: u32 = 16;
    /// An input good shared by several recipes of one machine.
    pub const SHARED_INPUT: u32 = 17;
    /// A boiler heated by a heater at part duty.
    pub const PART_DUTY: u32 = 18;
    /// A priority node with two out-edges of one order (GB103).
    pub const PRIORITY_TIE: u32 = 19;
    /// A part of a shared kinetic network that is not coupled (SEC-1 S5-2): its network is
    /// incomplete or spans too many districts, so it runs at its compile speed.
    pub const SHARED_NETWORK: u32 = 20;
}

/// Why a district's open window was thrown away or a sweep not counted (`Report` slots 24 to 32).
/// The first two are skips: the sweep is not sampled and the window goes on.
pub mod discard {
    pub const NONE: u32 = 0;
    /// IND_SWEEP_END with no samples buffered for it (skip).
    pub const NO_SWEEP: u32 = 1;
    /// IND_SWEEP_END whose record count did not match what arrived (skip).
    pub const DROPPED: u32 = 2;
    /// A sweep that was not live (some chunk did not block-tick throughout).
    pub const NOT_LIVE: u32 = 3;
    /// IND_EDIT: a player changed the district.
    pub const EDIT: u32 = 4;
    /// The district's inventory revision is not the compile's.
    pub const REVISION: u32 = 5;
    /// A new compile replaced the one whose window this was.
    pub const RECOMPILE: u32 = 6;
    /// A session start.
    pub const SESSION: u32 = 7;
    /// A station's call that moved cargo no one measured (IND_DISCARD `Other` from a train,
    /// SECTIONS.md §3.6): the units its last ports named, and the district's window.
    pub const STATION: u32 = 8;
    /// Reasons, for array sizes.
    pub const COUNT: usize = 9;
}

/// A window's verdict for one good or for the whole district.
pub mod verdict {
    /// No completed window yet (or the good was not judged).
    pub const NONE: u32 = 0;
    pub const PASS: u32 = 1;
    pub const FAIL: u32 = 2;
}

/// One good's bounds and its checksum. Rates are per tick (items in units, fluids in mB). With a
/// window (the last completed one, else the one in progress) every number is that window's, so they
/// agree with each other and with the verdict; before any, the current solve's.
#[derive(Clone, Debug, PartialEq)]
pub struct GoodReport {
    /// The good's id, as the district's records carry it.
    pub good: u32,
    /// [`class`] (the worst over the window's regimes).
    pub class: u32,
    /// Flow into stores: L (the fixed point) and U (the LP maximum), averaged over the window's
    /// regimes. `f64::INFINITY` when unbounded.
    pub in_l: f64,
    pub in_u: f64,
    /// Flow out of stores, likewise (less what a store could not give: it held too little).
    pub out_l: f64,
    pub out_u: f64,
    /// The predicted net change of the district's sampled stock: the slope of the integrated L
    /// prediction over the window's samples (`in_l - out_l` in a single regime).
    pub predicted: f64,
    /// The measured slope of the sampled stock over the window; `f64::NAN` before any window.
    pub measured: f64,
    /// The slope's standard error, including a stochastic source's model variance.
    pub sigma: f64,
    /// The batch quantization over the window, `b / W`.
    pub q: f64,
    /// [`verdict`].
    pub verdict: u32,
    /// Sweeps in the window.
    pub samples: u32,
    /// The band a bounded good is judged in (before the tolerance): slopes of the integrated
    /// `in_l - out_u` and `in_u - out_l`, so `lo <= predicted <= hi`.
    pub lo: f64,
    pub hi: f64,
    /// [`why`] the class is what it is, and the node (or edge, for the edge reasons) it came from;
    /// `u32::MAX` (reported -1) for none.
    pub why: u32,
    pub why_at: u32,
}

/// A district's glass state for operators and tests.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Report {
    /// The compile the solve used.
    pub compile: u32,
    /// Coupling rounds the solve took (§3.2 step 4), and whether it converged before
    /// `glass.solveRoundsMax`.
    pub rounds: u32,
    pub converged: bool,
    pub nodes: u32,
    pub edges: u32,
    /// Wall time of the last solve, in microseconds (bench and operator use only; never in state).
    pub solve_micros: f64,
    /// The window the goods below come from: the last completed one if any, else the one in progress.
    pub window_start: u64,
    pub window_ticks_done: u64,
    pub window_ticks_target: u64,
    pub windows_completed: u32,
    /// The last completed window's district verdict ([`verdict`]): PASS only if every judged good passed.
    pub verdict: u32,
    /// Failed windows in a row (demotion after `glass.mismatchStrikes`).
    pub strikes: u32,
    /// Solves run for this compile (a regime change solves again, unless its regime was cached).
    pub solves: u32,
    /// Node kinds (model codes) demoted to measured in this district (G-B14).
    pub demoted: u32,
    /// The compile's longest path latency in ticks (part of the warm-up before a window).
    pub latency: u64,
    /// The tick the compile was accepted.
    pub accepted_at: u64,
    /// The earliest tick the next window may open (after the warm-up and any discard).
    pub ready_at: u64,
    /// Live sweeps this compile's checksum took, and those sampled into a window.
    pub sweeps: u32,
    pub sweeps_sampled: u32,
    /// Heaters still lit on the burn they had at the compile, and the tick the next of them changes
    /// phase (None: none will).
    pub heaters_burning: u32,
    pub next_breakpoint: Option<u64>,
    /// The inventory revision the compile names, and the district's at its last sweep.
    pub revision: u32,
    pub district_revision: Option<u32>,
    /// The latest discard or skip: (tick, [`discard`] reason).
    pub last_discard: Option<(u64, u32)>,
    /// Discards and skips by [`discard`] reason (index 0 unused), since the district's glass began.
    pub discards: [u64; discard::COUNT],
    /// The district's replay while unloaded (G2).
    pub replay: ReplayReport,
    pub goods: Vec<GoodReport>,
    /// The judging units, by index; empty with one unit or none (SECTIONS.md §3.4, choice SC74).
    pub units: Vec<UnitReport>,
    /// The compile's sections, by index (SECTIONS.md §8.1, choice SC165). With one unit or none
    /// their state is the district's own window and replay.
    pub sections: Vec<SectionReport>,
    /// The stores shared by sections, by store node (SECTIONS.md §4.2, §8.1).
    pub shared: Vec<SharedReport>,
}

/// The state of a section, as `/rtsim district sections` names it (choice SC166).
pub mod section_state {
    /// No pass yet, or nothing to say: the window is still being taken.
    pub const LEARNING: u32 = 0;
    /// A window passed and nothing since changed it, but it does not replay now.
    pub const PASSED: u32 = 1;
    /// Its last concluded window failed.
    pub const FAILED: u32 = 2;
    /// An edit inside its box came after its pass.
    pub const STALE: u32 = 3;
    /// A fallback froze it in the current replay.
    pub const FROZEN: u32 = 4;
    /// The district is replaying in glass and this section is part of it.
    pub const REPLAYING: u32 = 5;
}

/// One section in a [`Report`] (SECTIONS.md §8.1, choice SC165).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SectionReport {
    /// Its judging unit; None with one unit or none (the district's window judges).
    pub unit: Option<u32>,
    /// Its judging group, named by the group's lowest section.
    pub group: u32,
    pub nodes: u32,
    /// The inventories of the stores it touches, interior and shared: (inventory, shared with
    /// another section); None for a store with no inventory (a tank the sampler does not read).
    pub stores: Vec<(Option<u32>, bool)>,
    pub domains: u32,
    /// A working section (a machine, heater, boiler, sink or source), else a storage system.
    pub working: bool,
    /// Its node box, grown by one block (§3.3).
    pub bbox: Option<(BlockPos, BlockPos)>,
    /// Its lowest node position, which names it.
    pub low: Option<BlockPos>,
    pub fingerprint: u64,
    /// [`section_state`].
    pub state: u32,
    pub passed: bool,
    pub stale: bool,
    pub may_replay: bool,
    pub frozen: bool,
    /// Its last concluded window's verdict ([`verdict`]), strikes in a row, windows completed and
    /// the open window's ticks so far; of its unit, else of the district.
    pub verdict: u32,
    pub strikes: u32,
    pub windows_completed: u32,
    pub window_ticks: u64,
    /// The failed good of the last window that missed its band by the most (choice SC167).
    pub worst: Option<WorstGood>,
}

/// A failed good of a window, for [`SectionReport::worst`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorstGood {
    pub good: u32,
    pub measured: f64,
    pub predicted: f64,
    /// How far the measured slope lies beyond the good's band (an exact good's band is its
    /// prediction), per tick, before the tolerance.
    pub excess: f64,
}

/// One store shared by sections in a [`Report`] (SECTIONS.md §4.2, §8.1, choice SC168).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SharedReport {
    /// Its node in the compile.
    pub node: u32,
    pub inventory: Option<u32>,
    pub pos: Option<BlockPos>,
    /// The sections that touch it, ascending.
    pub sections: Vec<u32>,
    /// Its moves are counted at its movers (it lies between units); else a unit holds it.
    pub counted: bool,
    /// The (mover, store) pairs Java counts at it.
    pub pairs: u32,
    /// The latest count sample covered every pair at it.
    pub counted_now: bool,
    /// The sweep of Java's latest count sample (-1 none).
    pub sample_sweep: Option<u32>,
    /// Its residue per good since it was first counted under the current counters: its stock less
    /// the ports into it and every counted move there, as it changed. Goods with none are absent.
    pub residue: Vec<(u32, i64)>,
}

/// A district's compile as a saved page shows it (SECTIONS.md §8.3, slice S7): the sections and the
/// stores they share, as the live report builds them. A save holds no live count sample, so its
/// shared stores read as not counted this sweep and have no residue. The viewer draws this.
#[derive(Clone, Debug, PartialEq)]
pub struct DistrictSections {
    pub sections: Vec<SectionReport>,
    pub shared: Vec<SharedReport>,
    /// Its judging units (0 or 1: the district's own window judges).
    pub units: u32,
    /// The compile's number.
    pub compile: u32,
}

impl DistrictSections {
    /// The sections of `dg`'s compile; None before its first complete compile.
    pub fn of(dg: &super::check::DistrictGlass) -> Option<DistrictSections> {
        let c = dg.compiled.as_ref()?;
        Some(DistrictSections {
            sections: c.section_reports(dg),
            shared: c.shared_reports(),
            units: c.unit_state.len() as u32,
            compile: c.graph.end.compile,
        })
    }
}

/// One judging unit in a [`Report`] (choice SC74).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct UnitReport {
    /// Its sections.
    pub sections: u32,
    /// Its nodes (stores it holds included).
    pub nodes: u32,
    /// The box its sections' node boxes span (each grown by one block); None when none has one.
    pub bbox: Option<(BlockPos, BlockPos)>,
    /// Its open window's ticks so far (0: none open).
    pub window_ticks: u64,
    pub windows_completed: u32,
    /// Its last concluded window's verdict ([`verdict`]).
    pub verdict: u32,
    /// Failed windows in a row.
    pub strikes: u32,
    /// A window of it passed on this compile.
    pub passed: bool,
    /// An edit inside its box came after that pass (§3.6).
    pub stale: bool,
    /// It may replay now (`Compiled::unit_may_replay`).
    pub may_replay: bool,
    /// A fallback froze it in the current replay.
    pub frozen: bool,
    /// Its goods start unbounded (a black box or a port among its nodes).
    pub unbounded: bool,
    /// Its window may open from this tick.
    pub ready_at: u64,
}

impl UnitReport {
    /// The flag bits in slot 13 of [`Report::to_f64s`]'s unit tail.
    pub const PASSED: u32 = 1;
    pub const STALE: u32 = 2;
    pub const MAY_REPLAY: u32 = 4;
    pub const FROZEN: u32 = 8;
    pub const UNBOUNDED: u32 = 16;

    fn flags(&self) -> u32 {
        (if self.passed { Self::PASSED } else { 0 })
            | (if self.stale { Self::STALE } else { 0 })
            | (if self.may_replay { Self::MAY_REPLAY } else { 0 })
            | (if self.frozen { Self::FROZEN } else { 0 })
            | (if self.unbounded { Self::UNBOUNDED } else { 0 })
    }
}

/// Which model replays a district while it is unloaded (`ReplayReport::model`).
pub mod replay_model {
    /// Not replaying: live, frozen, or never unloaded since its last load.
    pub const NONE: u32 = 0;
    /// The learned boundary rates (C19's black box).
    pub const BLACK_BOX: u32 = 1;
    /// The compiled graph (G2).
    pub const GLASS: u32 = 2;
}

/// The district's replay while unloaded (GLASSBOX.md §4, G2): header slots 33 to 40.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ReplayReport {
    /// [`replay_model`]: what replays the district now.
    pub model: u32,
    /// The tick its current replay started (None: not replaying).
    pub since: Option<u64>,
    /// Regimes the glass replay entered since the compile (each solve or cache hit at a breakpoint).
    pub regimes: u32,
    /// Breakpoints the glass replay handled since the compile.
    pub breakpoints: u32,
    /// Regimes that fell back to the black box as oscillating (§4.5) since the compile.
    pub oscillations: u32,
    /// Cut edges between live and simulated nodes in the current replay (I7, G-B6).
    pub cut_edges: u32,
    /// The compile may replay: a window of it passed (G-B4).
    pub eligible: bool,
    /// Goods the residual node replays as learned sources (§5.1, §5.3).
    pub learned_sources: u32,
}

/// Values per good in [`Report::to_f64s`].
pub const GOOD_STRIDE: usize = 16;
/// Values before the first good in [`Report::to_f64s`].
pub const HEADER: usize = 42;
/// Values per unit in [`Report::to_f64s`]'s tail.
pub const UNIT_STRIDE: usize = 15;
/// Values per section in [`Report::to_f64s`]'s sections part.
pub const SECTION_STRIDE: usize = 27;
/// Values per (section, store) entry of the sections part's store list.
pub const SECTION_STORE_STRIDE: usize = 3;
/// Values per shared store in [`Report::to_f64s`]'s shared-stores part.
pub const SHARED_STRIDE: usize = 12;
/// Values per (shared store, section) link of the shared-stores part.
pub const SHARED_LINK_STRIDE: usize = 2;
/// Values per (shared store, good) residue of the shared-stores part.
pub const SHARED_RESIDUE_STRIDE: usize = 3;

impl SectionReport {
    /// The flag bits in slot 6 of a section.
    pub const PASSED: u32 = 1;
    pub const STALE: u32 = 2;
    pub const MAY_REPLAY: u32 = 4;
    pub const FROZEN: u32 = 8;
    pub const WORKING: u32 = 16;
    pub const BOX: u32 = 32;
    pub const LOW: u32 = 64;

    fn flags(&self) -> u32 {
        (if self.passed { Self::PASSED } else { 0 })
            | (if self.stale { Self::STALE } else { 0 })
            | (if self.may_replay { Self::MAY_REPLAY } else { 0 })
            | (if self.frozen { Self::FROZEN } else { 0 })
            | (if self.working { Self::WORKING } else { 0 })
            | (if self.bbox.is_some() { Self::BOX } else { 0 })
            | (if self.low.is_some() { Self::LOW } else { 0 })
    }
}

fn opt(x: Option<u64>) -> f64 {
    x.map_or(-1.0, |v| v as f64)
}

fn idx(x: u32) -> f64 {
    if x == u32::MAX { -1.0 } else { f64::from(x) }
}

impl Report {
    /// The flat form the JNI inspection call returns (`NativeEngine.glassReport`); -1 is "none":
    ///
    /// - header (42): 0 compile, 1 rounds, 2 converged (0 or 1), 3 nodes, 4 edges, 5 solve µs,
    ///   6 window start tick, 7 window ticks done, 8 window ticks target, 9 windows completed,
    ///   10 verdict, 11 strikes, 12 goods, 13 solves, 14 demoted kinds, 15 latency ticks,
    ///   16 accepted at (tick), 17 ready at (tick), 18 sweeps taken, 19 sweeps sampled,
    ///   20 heaters burning, 21 next burn breakpoint (tick), 22 compile revision, 23 district
    ///   revision at the last sweep, 24 last discard tick, 25 last discard reason ([`discard`]),
    ///   26 to 32 discards and skips by reason 1 to 7 (no sweep, dropped records, not live, edit,
    ///   revision, recompile, session start), 33 replay model ([`replay_model`]), 34 replay since
    ///   (tick), 35 regimes entered, 36 breakpoints handled, 37 oscillation fallbacks, 38 cut
    ///   edges, 39 eligible (0 or 1), 40 learned sources, 41 discards for a station's cargo
    ///   ([`discard::STATION`]);
    /// - then per good (16): 0 good, 1 class, 2 in L, 3 in U, 4 out L, 5 out U, 6 predicted,
    ///   7 measured, 8 sigma, 9 q, 10 verdict, 11 samples, 12 lo, 13 hi, 14 why ([`why`]),
    ///   15 why at (a node, or an edge for the edge reasons);
    /// - then the units' count, and per unit ([`UNIT_STRIDE`], choice SC74): 0 sections, 1 nodes,
    ///   2 has a box (0 or 1), 3 to 5 the box's low corner, 6 to 8 its high corner, 9 open window
    ///   ticks, 10 windows completed, 11 last verdict, 12 strikes, 13 flags ([`UnitReport`]'s bits:
    ///   passed, stale, may replay, frozen, unbounded), 14 ready at (tick);
    /// - then the sections' count, and per section ([`SECTION_STRIDE`], choice SC165): 0 unit (-1
    ///   none), 1 judging group, 2 nodes, 3 stores, 4 stores shared with another section, 5
    ///   domains, 6 flags ([`SectionReport`]'s bits: passed, stale, may replay, frozen, working,
    ///   has a box, has a low position), 7 state ([`section_state`]), 8 to 10 the box's low
    ///   corner, 11 to 13 its high corner, 14 to 16 the lowest node position, 17 and 18 the
    ///   fingerprint's high and low 32 bits, 19 last verdict, 20 strikes, 21 windows completed,
    ///   22 open window ticks, 23 worst good (-1 none), 24 its measured slope, 25 its predicted,
    ///   26 its excess beyond the band;
    /// - then the store list: its count, and per entry ([`SECTION_STORE_STRIDE`]) 0 section,
    ///   1 inventory (-1 none), 2 shared (0 or 1), the entries of one section in its store order;
    /// - then the shared stores' count, and per store ([`SHARED_STRIDE`], choice SC168): 0 store
    ///   node, 1 inventory (-1 none), 2 has a position, 3 to 5 the position, 6 sections, 7 counted
    ///   between units (0 or 1), 8 pairs, 9 counted in the latest sweep (0 or 1), 10 that sweep (-1
    ///   none), 11 goods with a residue;
    /// - then the links' count and per link ([`SHARED_LINK_STRIDE`]) 0 store (index in the list),
    ///   1 section;
    /// - then the residues' count and per residue ([`SHARED_RESIDUE_STRIDE`]) 0 store (index in the
    ///   list), 1 good, 2 amount.
    pub fn to_f64s(&self) -> Vec<f64> {
        let mut v = Vec::with_capacity(HEADER + GOOD_STRIDE * self.goods.len());
        v.extend_from_slice(&[
            f64::from(self.compile),
            f64::from(self.rounds),
            if self.converged { 1.0 } else { 0.0 },
            f64::from(self.nodes),
            f64::from(self.edges),
            self.solve_micros,
            self.window_start as f64,
            self.window_ticks_done as f64,
            self.window_ticks_target as f64,
            f64::from(self.windows_completed),
            f64::from(self.verdict),
            f64::from(self.strikes),
            self.goods.len() as f64,
            f64::from(self.solves),
            f64::from(self.demoted),
            self.latency as f64,
            self.accepted_at as f64,
            self.ready_at as f64,
            f64::from(self.sweeps),
            f64::from(self.sweeps_sampled),
            f64::from(self.heaters_burning),
            opt(self.next_breakpoint),
            f64::from(self.revision),
            opt(self.district_revision.map(u64::from)),
            opt(self.last_discard.map(|d| d.0)),
            self.last_discard.map_or(0.0, |d| f64::from(d.1)),
        ]);
        v.extend(
            self.discards[1..discard::STATION as usize]
                .iter()
                .map(|&c| c as f64),
        );
        let r = &self.replay;
        v.extend_from_slice(&[
            f64::from(r.model),
            opt(r.since),
            f64::from(r.regimes),
            f64::from(r.breakpoints),
            f64::from(r.oscillations),
            f64::from(r.cut_edges),
            if r.eligible { 1.0 } else { 0.0 },
            f64::from(r.learned_sources),
            self.discards[discard::STATION as usize] as f64,
        ]);
        debug_assert_eq!(v.len(), HEADER);
        for g in &self.goods {
            v.extend_from_slice(&[
                f64::from(g.good),
                f64::from(g.class),
                g.in_l,
                g.in_u,
                g.out_l,
                g.out_u,
                g.predicted,
                g.measured,
                g.sigma,
                g.q,
                f64::from(g.verdict),
                f64::from(g.samples),
                g.lo,
                g.hi,
                f64::from(g.why),
                idx(g.why_at),
            ]);
        }
        v.push(self.units.len() as f64);
        for u in &self.units {
            let (lo, hi) = u.bbox.unwrap_or_default();
            v.extend_from_slice(&[
                f64::from(u.sections),
                f64::from(u.nodes),
                if u.bbox.is_some() { 1.0 } else { 0.0 },
                f64::from(lo.x),
                f64::from(lo.y),
                f64::from(lo.z),
                f64::from(hi.x),
                f64::from(hi.y),
                f64::from(hi.z),
                u.window_ticks as f64,
                f64::from(u.windows_completed),
                f64::from(u.verdict),
                f64::from(u.strikes),
                f64::from(u.flags()),
                u.ready_at as f64,
            ]);
        }
        self.put_sections(&mut v);
        self.put_shared(&mut v);
        v
    }

    fn put_sections(&self, v: &mut Vec<f64>) {
        v.push(self.sections.len() as f64);
        for s in &self.sections {
            let (lo, hi) = s.bbox.unwrap_or_default();
            let low = s.low.unwrap_or_default();
            let w = s.worst;
            v.extend_from_slice(&[
                s.unit.map_or(-1.0, f64::from),
                f64::from(s.group),
                f64::from(s.nodes),
                s.stores.len() as f64,
                s.stores.iter().filter(|x| x.1).count() as f64,
                f64::from(s.domains),
                f64::from(s.flags()),
                f64::from(s.state),
                f64::from(lo.x),
                f64::from(lo.y),
                f64::from(lo.z),
                f64::from(hi.x),
                f64::from(hi.y),
                f64::from(hi.z),
                f64::from(low.x),
                f64::from(low.y),
                f64::from(low.z),
                (s.fingerprint >> 32) as f64,
                (s.fingerprint & 0xFFFF_FFFF) as f64,
                f64::from(s.verdict),
                f64::from(s.strikes),
                f64::from(s.windows_completed),
                s.window_ticks as f64,
                w.map_or(-1.0, |w| f64::from(w.good)),
                w.map_or(0.0, |w| w.measured),
                w.map_or(0.0, |w| w.predicted),
                w.map_or(0.0, |w| w.excess),
            ]);
        }
        v.push(self.sections.iter().map(|s| s.stores.len()).sum::<usize>() as f64);
        for (i, s) in self.sections.iter().enumerate() {
            for &(inv, shared) in &s.stores {
                v.extend_from_slice(&[
                    i as f64,
                    inv.map_or(-1.0, f64::from),
                    if shared { 1.0 } else { 0.0 },
                ]);
            }
        }
    }

    fn put_shared(&self, v: &mut Vec<f64>) {
        v.push(self.shared.len() as f64);
        for h in &self.shared {
            let p = h.pos.unwrap_or_default();
            v.extend_from_slice(&[
                h.node as f64,
                h.inventory.map_or(-1.0, f64::from),
                if h.pos.is_some() { 1.0 } else { 0.0 },
                f64::from(p.x),
                f64::from(p.y),
                f64::from(p.z),
                h.sections.len() as f64,
                if h.counted { 1.0 } else { 0.0 },
                f64::from(h.pairs),
                if h.counted_now { 1.0 } else { 0.0 },
                h.sample_sweep.map_or(-1.0, f64::from),
                h.residue.len() as f64,
            ]);
        }
        v.push(self.shared.iter().map(|h| h.sections.len()).sum::<usize>() as f64);
        for (i, h) in self.shared.iter().enumerate() {
            for &s in &h.sections {
                v.extend_from_slice(&[i as f64, f64::from(s)]);
            }
        }
        v.push(self.shared.iter().map(|h| h.residue.len()).sum::<usize>() as f64);
        for (i, h) in self.shared.iter().enumerate() {
            for &(g, x) in &h.residue {
                v.extend_from_slice(&[i as f64, f64::from(g), x as f64]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flat_form_has_the_documented_layout() {
        let r = Report {
            compile: 3,
            rounds: 2,
            converged: true,
            solves: 4,
            demoted: 1,
            latency: 40,
            goods: vec![GoodReport {
                good: 7,
                class: class::BOUNDED,
                in_l: 0.0625,
                in_u: 0.1175,
                out_l: 0.0,
                out_u: 0.0,
                predicted: 0.0625,
                measured: 0.07,
                sigma: 0.001,
                q: 0.0005,
                verdict: verdict::PASS,
                samples: 120,
                lo: 0.0625,
                hi: 0.1175,
                why: why::RULE,
                why_at: 5,
            }],
            accepted_at: 100,
            ready_at: 1300,
            sweeps: 9,
            sweeps_sampled: 7,
            heaters_burning: 2,
            next_breakpoint: Some(2000),
            revision: 4,
            district_revision: None,
            last_discard: Some((900, discard::DROPPED)),
            discards: [0, 1, 2, 3, 4, 5, 6, 7, 8],
            replay: ReplayReport {
                model: replay_model::GLASS,
                since: Some(5000),
                regimes: 3,
                breakpoints: 9,
                oscillations: 1,
                cut_edges: 2,
                eligible: true,
                learned_sources: 4,
            },
            units: vec![UnitReport {
                sections: 2,
                nodes: 9,
                bbox: Some((
                    BlockPos { x: -3, y: 60, z: 4 },
                    BlockPos { x: 5, y: 64, z: 9 },
                )),
                window_ticks: 300,
                windows_completed: 1,
                verdict: verdict::PASS,
                strikes: 0,
                passed: true,
                stale: false,
                may_replay: true,
                frozen: false,
                unbounded: true,
                ready_at: 700,
            }],
            sections: vec![
                SectionReport {
                    unit: Some(0),
                    group: 0,
                    nodes: 9,
                    stores: vec![(Some(4), true), (None, false)],
                    domains: 1,
                    working: true,
                    bbox: Some((
                        BlockPos { x: -3, y: 60, z: 4 },
                        BlockPos { x: 5, y: 64, z: 9 },
                    )),
                    low: Some(BlockPos { x: -2, y: 61, z: 5 }),
                    fingerprint: 0xDEAD_BEEF_0123_4567,
                    state: section_state::REPLAYING,
                    passed: true,
                    stale: false,
                    may_replay: true,
                    frozen: false,
                    verdict: verdict::PASS,
                    strikes: 0,
                    windows_completed: 1,
                    window_ticks: 300,
                    worst: None,
                },
                SectionReport {
                    unit: None,
                    group: 0,
                    nodes: 2,
                    stores: vec![(Some(4), true)],
                    state: section_state::FAILED,
                    verdict: verdict::FAIL,
                    strikes: 2,
                    worst: Some(WorstGood {
                        good: 7,
                        measured: 0.05,
                        predicted: 0.1,
                        excess: 0.05,
                    }),
                    ..SectionReport::default()
                },
            ],
            shared: vec![SharedReport {
                node: 12,
                inventory: Some(4),
                pos: Some(BlockPos { x: 1, y: 62, z: 3 }),
                sections: vec![0, 1],
                counted: true,
                pairs: 2,
                counted_now: true,
                sample_sweep: Some(41),
                residue: vec![(7, -68)],
            }],
            ..Report::default()
        };
        let v = r.to_f64s();
        let tail = HEADER + GOOD_STRIDE + 1 + UNIT_STRIDE;
        assert_eq!(
            v.len(),
            tail + 1
                + 2 * SECTION_STRIDE
                + 1
                + 3 * SECTION_STORE_STRIDE
                + 1
                + SHARED_STRIDE
                + 1
                + 2 * SHARED_LINK_STRIDE
                + 1
                + SHARED_RESIDUE_STRIDE
        );
        assert_eq!(v[0], 3.0);
        assert_eq!(v[2], 1.0);
        assert_eq!(v[12], 1.0);
        assert_eq!(&v[13..16], &[4.0, 1.0, 40.0]);
        assert_eq!(
            &v[16..26],
            &[100.0, 1300.0, 9.0, 7.0, 2.0, 2000.0, 4.0, -1.0, 900.0, 2.0]
        );
        assert_eq!(&v[26..33], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(
            &v[33..42],
            &[2.0, 5000.0, 3.0, 9.0, 1.0, 2.0, 1.0, 4.0, 8.0]
        );
        assert_eq!(Report::default().to_f64s()[34], -1.0);
        assert_eq!(v[HEADER], 7.0);
        assert_eq!(v[HEADER + 1], 1.0);
        assert_eq!(v[HEADER + 3], 0.1175);
        assert_eq!(v[HEADER + 11], 120.0);
        assert_eq!(&v[HEADER + 12..HEADER + 16], &[0.0625, 0.1175, 2.0, 5.0]);
        let t = HEADER + GOOD_STRIDE;
        assert_eq!(v[t], 1.0);
        assert_eq!(
            &v[t + 1..t + 1 + UNIT_STRIDE],
            &[
                2.0, 9.0, 1.0, -3.0, 60.0, 4.0, 5.0, 64.0, 9.0, 300.0, 1.0, 1.0, 0.0, 21.0, 700.0
            ]
        );
        assert_eq!(Report::default().to_f64s().len(), HEADER + 1 + 2 + 3);
        // The sections part: a count, 27 values per section, then the store list.
        assert_eq!(v[tail], 2.0);
        let a = &v[tail + 1..tail + 1 + SECTION_STRIDE];
        assert_eq!(
            &a[..8],
            &[
                0.0,
                0.0,
                9.0,
                2.0,
                1.0,
                1.0,
                1.0 + 16.0 + 32.0 + 64.0 + 4.0,
                5.0
            ]
        );
        assert_eq!(
            &a[8..17],
            &[-3.0, 60.0, 4.0, 5.0, 64.0, 9.0, -2.0, 61.0, 5.0]
        );
        assert_eq!(a[17], f64::from(0xDEAD_BEEF_u32));
        assert_eq!(a[18], f64::from(0x0123_4567_u32));
        assert_eq!(&a[19..23], &[1.0, 0.0, 1.0, 300.0]);
        assert_eq!(&a[23..27], &[-1.0, 0.0, 0.0, 0.0]);
        let b = &v[tail + 1 + SECTION_STRIDE..tail + 1 + 2 * SECTION_STRIDE];
        assert_eq!(&b[..8], &[-1.0, 0.0, 2.0, 1.0, 1.0, 0.0, 0.0, 2.0]);
        assert_eq!(&b[19..27], &[2.0, 2.0, 0.0, 0.0, 7.0, 0.05, 0.1, 0.05]);
        let l = tail + 1 + 2 * SECTION_STRIDE;
        assert_eq!(v[l], 3.0);
        assert_eq!(
            &v[l + 1..l + 1 + 3 * SECTION_STORE_STRIDE],
            &[0.0, 4.0, 1.0, 0.0, -1.0, 0.0, 1.0, 4.0, 1.0]
        );
        // The shared part: stores, links, residues.
        let h = l + 1 + 3 * SECTION_STORE_STRIDE;
        assert_eq!(v[h], 1.0);
        assert_eq!(
            &v[h + 1..h + 1 + SHARED_STRIDE],
            &[
                12.0, 4.0, 1.0, 1.0, 62.0, 3.0, 2.0, 1.0, 2.0, 1.0, 41.0, 1.0
            ]
        );
        let k = h + 1 + SHARED_STRIDE;
        assert_eq!(v[k], 2.0);
        assert_eq!(&v[k + 1..k + 5], &[0.0, 0.0, 0.0, 1.0]);
        let q = k + 1 + 2 * SHARED_LINK_STRIDE;
        assert_eq!(&v[q..], &[1.0, 0.0, 7.0, -68.0]);
    }
}
