// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the glass box reports per district (G1, read only): the solve's bounds per good and the
//! checksum's verdict against one window of live sweeps (GLASSBOX.md §3.5, §5.3; the meaning of each
//! number is docs/industry-boundary.md §9.3). Operators read it through `/rtsim district` and the
//! server log, GameTests through the JNI inspection call; nothing in the world shows it (I18).

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
    /// Reasons, for array sizes.
    pub const COUNT: usize = 8;
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
pub const HEADER: usize = 41;

fn opt(x: Option<u64>) -> f64 {
    x.map_or(-1.0, |v| v as f64)
}

fn idx(x: u32) -> f64 {
    if x == u32::MAX { -1.0 } else { f64::from(x) }
}

impl Report {
    /// The flat form the JNI inspection call returns (`NativeEngine.glassReport`); -1 is "none":
    ///
    /// - header (41): 0 compile, 1 rounds, 2 converged (0 or 1), 3 nodes, 4 edges, 5 solve µs,
    ///   6 window start tick, 7 window ticks done, 8 window ticks target, 9 windows completed,
    ///   10 verdict, 11 strikes, 12 goods, 13 solves, 14 demoted kinds, 15 latency ticks,
    ///   16 accepted at (tick), 17 ready at (tick), 18 sweeps taken, 19 sweeps sampled,
    ///   20 heaters burning, 21 next burn breakpoint (tick), 22 compile revision, 23 district
    ///   revision at the last sweep, 24 last discard tick, 25 last discard reason ([`discard`]),
    ///   26 to 32 discards and skips by reason 1 to 7 (no sweep, dropped records, not live, edit,
    ///   revision, recompile, session start), 33 replay model ([`replay_model`]), 34 replay since
    ///   (tick), 35 regimes entered, 36 breakpoints handled, 37 oscillation fallbacks, 38 cut
    ///   edges, 39 eligible (0 or 1), 40 learned sources;
    /// - then per good (16): 0 good, 1 class, 2 in L, 3 in U, 4 out L, 5 out U, 6 predicted,
    ///   7 measured, 8 sigma, 9 q, 10 verdict, 11 samples, 12 lo, 13 hi, 14 why ([`why`]),
    ///   15 why at (a node, or an edge for the edge reasons).
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
        v.extend(self.discards[1..].iter().map(|&c| c as f64));
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
        v
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
            discards: [0, 1, 2, 3, 4, 5, 6, 7],
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
            ..Report::default()
        };
        let v = r.to_f64s();
        assert_eq!(v.len(), HEADER + GOOD_STRIDE);
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
            &v[33..41],
            &[2.0, 5000.0, 3.0, 9.0, 1.0, 2.0, 1.0, 4.0]
        );
        assert_eq!(Report::default().to_f64s()[34], -1.0);
        assert_eq!(v[HEADER], 7.0);
        assert_eq!(v[HEADER + 1], 1.0);
        assert_eq!(v[HEADER + 3], 0.1175);
        assert_eq!(v[HEADER + 11], 120.0);
        assert_eq!(&v[HEADER + 12..HEADER + 16], &[0.0625, 0.1175, 2.0, 5.0]);
    }
}
