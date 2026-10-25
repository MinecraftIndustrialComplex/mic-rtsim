// SPDX-License-Identifier: AGPL-3.0-or-later
//! One district's glass state: the compile it holds, the regime it is in, and the checksum window
//! (GLASSBOX.md §5.3, G-B14; docs/industry-boundary.md §9.3).
//!
//! **Per sweep, O(stores × goods):** the interval since the last sweep is integrated under the
//! regime it was in (controllers switch between sweeps, so the prediction is the time average of
//! the per-interval regimes), the sample is added to the window's running sums, then the stores'
//! states and the controllers are read again; only a changed regime solves (and a regime seen
//! recently comes from the cache). A heater lit on the burn it had at the compile changes phase at
//! a known tick (it refuses fuel until its burn falls to the insertion threshold, then takes it, then
//! runs out): the interval is cut there and the rest integrated under the new regime.
//!
//! **The fit.** Per good, the measured series is the read stores' summed stock minus the district's
//! ports (`N = S - P`, as the learner); the predicted series is the cumulative L prediction. The
//! measured slope's standard error comes from the residual of measured minus predicted, so a regime
//! change inside the window does not inflate it, and the predicted value is the least-squares slope
//! of the cumulative prediction at the same sample times (in one regime, exactly `in_l - out_l`).
//!
//! **Judging units** (SECTIONS.md §3.4, §4.1 to §4.3; slice S2b). A compile cut into two judging
//! units or more ([`Units`]) keeps, beside the district's window, one window per unit: its rows
//! (one sorted `Vec` by unit and good), its own start, warm-up, verdicts and strikes. The units'
//! windows integrate the same solve, each on its own stores and its own bounds ([`units::bounds`],
//! choice SC56), and are sampled from the same sweep: a unit is measured on its stores less the
//! ports into its inventories (`ports_at`). A unit's window survives what does not touch it: a
//! sweep in which another part of the district did not run, an edit outside its box, and a
//! recompile that left it unchanged. The district's window stays the checksum over everything and
//! keeps today's rules; derates and demotions are still its own.
//!
//! **Counted stores** (SECTIONS.md §4.1, §4.2; slice S3). At a shared store between units whose
//! moves Java counts ([`Units::counted`]), a unit is measured on what its own movers put in less what
//! they took out (IND_COUNT_SAMPLE, cumulative since each pair's `since`), and on the store's
//! **residue**: its stock less the ports into it less every counted move there, as it changed since
//! the unit's window began. The model has no move at a counted store but its movers', so a residue is
//! a move it lacks, and every unit at the store fails on that good as the district's window does
//! (§4.3's judging group for one window). A unit's window opens only at a sweep whose count sample
//! holds each pair at its counted stores; a sweep without that loses it, and a pair whose `since`
//! moved (Java forgot its counters) starts it again from that sweep.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_kernel::clock::Tick;

use super::graph::Graph;
use super::q::Q;
use super::records::{
    CONTROL_RUN_WHEN_ON, CONTROL_STOP_WHEN_ON, GraphDomain, NodeKind, ProbeSet, model,
};
use super::report::{self, GoodReport, Report};
use super::sections::Sections;
use super::solve::{self, Regime, Solution, burn};
use super::units::{self, PAIR_IN, PAIR_OUT, UnitBound, Units};
use crate::district::Inventory;
use crate::ids::{BlockPos, DistrictId, Good, GoodClass, GoodVec, VecMap};
use crate::reconcile::GoodLookup;
use crate::tunables::GlassTunables;

/// The variance factor of a least-squares slope fitted to a cumulative compound Poisson count over
/// a window W: `Var(slope) = 6/5 × λ E[X²] / W` (∫∫ w(s) w(t) min(s, t) over the window, w(t) = t -
/// W/2, divided by (W³/12)²; checked in this module's tests).
pub const POISSON_SLOPE_FACTOR: f64 = 1.2;

/// A controller flag (params[2]): the switch's output was on at compile (docs/industry-boundary.md
/// §9.3, added in G1: choice GB15).
pub const CONTROLLER_ON_AT_COMPILE: i64 = 2;
/// A controller flag: the switch is inverted. For display only: its Control edges' `param` already
/// includes the inversion (§9.3, GB51), so "on" is the switch's state before inversion.
pub const CONTROLLER_INVERTED: i64 = 1;

/// Running sums for one good over a window.
#[derive(Clone, Debug, Default, PartialEq)]
struct Acc {
    /// Measured: Y = S - ports.
    sy: f64,
    sty: f64,
    /// Predicted cumulative (L) and its bounds.
    sp: f64,
    stp: f64,
    slo: f64,
    stlo: f64,
    shi: f64,
    sthi: f64,
    /// D = Y - P.
    sd: f64,
    std: f64,
    sdd: f64,
}

/// One good's cumulative prediction since the window started.
#[derive(Clone, Debug, Default, PartialEq)]
struct Cum {
    p: f64,
    lo: f64,
    hi: f64,
    lo_inf: bool,
    hi_inf: bool,
    /// ∫ in L, out L, in U, out U dt (out less what the stores could not give).
    il: f64,
    ol: f64,
    iu: f64,
    ou: f64,
    iu_inf: bool,
    ou_inf: bool,
    /// ∫ λ E[X²] dt.
    var: f64,
    batch: f64,
    class: u32,
    why: (u32, u32),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Window {
    start: Tick,
    last: Tick,
    n: u32,
    st: f64,
    stt: f64,
    goods: BTreeMap<Good, (Acc, Cum)>,
    /// Per read store and good: the predicted cumulative change, and (Σy, Σty, Σp, Σtp) of its
    /// sampled level and that prediction, for the per-store slopes the replay's residual is split
    /// by (G2-8).
    stores: BTreeMap<(u32, Good), (f64, [f64; 4])>,
}

/// One row of a judging unit's open window (SECTIONS.md §3.3): its good's running sums. The rows
/// of every unit are one `Vec` sorted by (unit, good), never a map per unit.
#[derive(Clone, Debug, Default, PartialEq)]
struct Row {
    unit: u16,
    good: Good,
    acc: Acc,
    cum: Cum,
}

/// A judging unit's open window: when it began and its sample times' sums (its goods are its rows
/// in [`Compiled`]). Its target length is the district's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct UnitWin {
    start: Tick,
    last: Tick,
    n: u32,
    st: f64,
    stt: f64,
}

/// The least capacity at which a unit's window saw one of its kinetic domains run at one speed
/// (SECTIONS.md §3.3): the solve's capacity there and that speed, as [`speed_key`] keeps it. One
/// per speed the domain ran at (a faster row may have more capacity and fail first), in
/// [`cap_order`]. The domain is its compile's index; a recompile that carries the unit maps it by
/// the unit's own nodes on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DomCap {
    pub domain: u32,
    /// The source whose own speed `speed` is, by position; None: `speed` is the reference's.
    pub source: Option<i64>,
    pub speed: f64,
    pub capacity: f64,
}

/// The order a unit's [`DomCap`]s are kept in: by domain, then speed.
fn cap_order(a: &DomCap, b: &DomCap) -> std::cmp::Ordering {
    (a.domain, a.source)
        .cmp(&(b.domain, b.source))
        .then(a.speed.total_cmp(&b.speed))
}

/// Domain `dom`'s reference speed `v` in a frame a recompile keeps (its reference may move with
/// Create's root, choice SC44): the own speed of its lowest source with a known ratio, by
/// position, else the reference's.
fn speed_key(dom: &GraphDomain, v: f64) -> (Option<i64>, f64) {
    match dom
        .sources
        .iter()
        .find_map(|s| s.ratio().filter(|&r| r != 0.0).map(|r| (s.pos, r)))
    {
        Some((pos, r)) => (Some(pos), v * r),
        None => (None, v),
    }
}

/// The reference speed of `dom` that [`speed_key`] kept as (`source`, `speed`), if it still holds
/// that source with a known ratio.
fn speed_from_key(dom: &GraphDomain, source: Option<i64>, speed: f64) -> Option<f64> {
    match source {
        None => Some(speed),
        Some(pos) => {
            let i = dom.sources.binary_search_by_key(&pos, |s| s.pos).ok()?;
            let r = dom.sources[i].ratio().filter(|&r| r != 0.0)?;
            Some(speed / r)
        }
    }
}

/// One judging unit's learning state (SECTIONS.md §3.3, §3.4): the district's window fields, per
/// unit. Saved by unit index: the units are a function of the compile and its sections, which the
/// page's reader recomputes and checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitState {
    win: Option<UnitWin>,
    /// Its window may open from here: the district's warm-up at the compile, and a fresh one after
    /// each discard of its own.
    pub ready_at: Tick,
    /// Its last concluded window (no per-store slopes: those are the district's).
    pub last: Option<WindowResult>,
    /// The start and length of its latest window that passed.
    pub passed: Option<(Tick, u64)>,
    /// An edit inside its box came after that pass: stale until a window passes again or a
    /// recompile finds it unchanged (§3.6).
    pub edited: bool,
    /// Failed windows in a row.
    pub strikes: u32,
    pub windows_completed: u32,
    /// The open window's least capacity run at, per domain that ran in it, by domain.
    caps: Vec<DomCap>,
    /// Its latest passed window's, for the domain check (§3.6).
    pub passed_caps: Vec<DomCap>,
    /// When a derate last fell on one of its nodes (choice SC61): a window of its that started
    /// before was judged against another model, so it lets the unit neither replay nor carry.
    pub derated_at: Option<Tick>,
    /// The goods its windows' rows held since the compile, kept as those rows go (a discard or a
    /// conclusion): the veto reads them (choice SC65). Sorted.
    pub touched: Vec<Good>,
    /// Its open window's pairs at its counted stores, by (mover, store) position, with each one's
    /// `since` at the window's first sample: a pair whose `since` moved starts the window again.
    /// By position, so a recompile that names the same pairs keeps the window (§4.2).
    pub count_since: Vec<PairSince>,
    /// Its counted stores' summed residue per good at its open window's first sample, sorted:
    /// what each later sample's residue is measured from.
    pub rbase: Vec<(Good, i64)>,
}

/// Which chunks ran through a sweep, for the units' liveness (SECTIONS.md §3.4).
pub enum Liveness<'a> {
    /// Every chunk of the district block-ticked throughout (IND_SWEEP_END's live flag).
    All,
    /// Some did not. A unit ran when each chunk it lies in is loaded, was reported ticking
    /// (IND_CHUNK_TICKING: Java reports no chunk that never ticked, so a district with none
    /// reported runs no unit, choice SC58) and did not stop since the last sweep sampled.
    Partial {
        loaded: &'a BTreeSet<crate::ids::ChunkPos>,
        dim: u32,
    },
}

/// One good's verdict in a completed window.
#[derive(Clone, Debug, PartialEq)]
pub struct GoodResult {
    pub good: Good,
    pub class: u32,
    pub why: (u32, u32),
    /// The window's average flows (NaN for a window with no length yet).
    pub in_l: f64,
    pub in_u: f64,
    pub out_l: f64,
    pub out_u: f64,
    pub predicted: f64,
    pub lo: f64,
    pub hi: f64,
    pub measured: f64,
    pub sigma: f64,
    pub q: f64,
    pub verdict: u32,
    pub samples: u32,
}

/// A completed window.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowResult {
    pub start: Tick,
    pub ticks: u64,
    pub verdict: u32,
    pub goods: Vec<GoodResult>,
    /// Per read store and good: (inventory, good, measured slope, predicted slope) per tick.
    pub stores: Vec<(u32, Good, f64, f64)>,
}

/// A probed buffer's first and latest total in the current probe set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeTrack {
    pub first_sweep: u32,
    pub first: i64,
    pub last_sweep: u32,
    pub last: i64,
}

/// The counters of one sweep's IND_COUNT_SAMPLE records (SECTIONS.md §4.2), for the pairs of the
/// current count set: each pair's `since`, and its cumulative moves in and out per good. Not
/// saved: a sweep's sample comes before its end, which reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CountTake {
    pub sweep: u32,
    /// (mover, store) to the game time its counters started at.
    pub since: BTreeMap<(u32, u32), i64>,
    /// (mover, store, good) to (moved in, moved out).
    pub moved: BTreeMap<(u32, u32, Good), (i64, i64)>,
    /// (mover, store) to how many entries its records said the pair has in all (the largest said).
    pub stated: BTreeMap<(u32, u32), u32>,
    /// (mover, store) to how many entries of it the records of the sweep held.
    pub got: BTreeMap<(u32, u32), u32>,
}

impl CountTake {
    /// The pair's `since`, when the sample holds the pair whole: at least one entry, and as many
    /// as it says the pair has. A pair split over records, one of them lost, is partial: its
    /// goods in the records that came would count, and those in the lost one read as not moved.
    pub fn since_of(&self, pair: (u32, u32)) -> Option<i64> {
        let since = *self.since.get(&pair)?;
        let stated = self.stated.get(&pair).copied().unwrap_or(0);
        let got = self.got.get(&pair).copied().unwrap_or(0);
        (stated >= 1 && got >= stated).then_some(since)
    }
}

/// What the district's glass did, for operators and tests (bounded: the last few events).
#[derive(Clone, Debug, PartialEq)]
pub enum GlassEvent {
    Derated { node: u32, eta: Q, good: Good },
    Probed { nodes: Vec<u32> },
    Demoted { model: u32, node: u32 },
}

/// Events kept per district.
const EVENTS_KEPT: usize = 16;

/// A heater lit on the burn it had at the compile, as the model spends it.
#[derive(Clone, Debug, PartialEq)]
pub struct Burn {
    pub node: u32,
    /// Burn ticks left.
    pub left: f64,
    /// The insertion threshold (`params[3]`): above it the heater refuses fuel.
    pub thr: f64,
}

impl UnitState {
    /// Its window is open.
    pub fn window_open(&self) -> bool {
        self.win.is_some()
    }
}

impl Burn {
    pub fn phase(&self) -> u8 {
        if self.left > self.thr {
            burn::REFUSING
        } else if self.left > 0.0 {
            burn::TAKING
        } else {
            burn::OUT
        }
    }

    /// Burn spent per tick: all of it while refusing, less what its fuel pays for while taking.
    pub fn spend(&self, fed: f64) -> f64 {
        match self.phase() {
            burn::REFUSING => 1.0,
            burn::TAKING => (1.0 - fed).max(0.0),
            _ => 0.0,
        }
    }

    /// Ticks until its phase changes at that spend, if it will.
    pub fn until(&self, fed: f64) -> Option<f64> {
        let r = self.spend(fed);
        match self.phase() {
            burn::REFUSING => Some(self.left - self.thr),
            burn::TAKING if r > 1e-12 => Some(self.left / r),
            _ => None,
        }
    }
}

/// The compile a district holds and everything computed from it.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub graph: Graph,
    pub accepted_at: Tick,
    pub window_target: u64,
    pub ready_at: Tick,
    /// Per node: a controller's switch state (before inversion).
    pub ctrl_on: Vec<bool>,
    pub regime: Regime,
    pub key: u64,
    pub sol: Solution,
    /// Recent solves by regime key, newest last.
    cache: VecDeque<(u64, Solution)>,
    /// The glass replay's parts (choice G2-22): the cache keeps `glass.regimeCache` per part.
    pub cache_parts: usize,
    pub solves: u32,
    pub cache_hits: u32,
    pub solve_micros: f64,
    win: Option<Window>,
    pub last: Option<WindowResult>,
    /// The latest window that passed (G-B4: the compile may replay), and whether an IND_EDIT came
    /// after it (the compile no longer matches the blocks until Java recompiles: no replay).
    pub passed: Option<WindowResult>,
    pub edited: bool,
    /// The goods the district's last concluded window failed on, carried from the compile before
    /// until this one's first district window concludes: a recompile does not lift the veto on a
    /// unit (SECTIONS.md §3.4, choice SC66). Sorted; [`Compiled::district_failing`] reads it.
    pub veto: Vec<Good>,
    /// When the last derate fell (choice SC61): a unit's window that started before it was
    /// judged against another model and names no node to derate. Carried by a recompile.
    pub derated_at: Option<Tick>,
    pub windows_completed: u32,
    pub strikes: u32,
    /// Per (inventory, good): its stock as of the last sweep, spent and refilled as the model
    /// integrates, so an interval never predicts more drain than the store held (§4.1's "input
    /// store empties").
    stock_est: BTreeMap<(u32, Good), f64>,
    last_sweep: Option<Tick>,
    /// Heaters lit on their compile-time burn, and the tick the model has reached.
    heaters: Vec<Burn>,
    model_at: Tick,
    /// Live sweeps taken, and those sampled into a window.
    pub sweeps: u32,
    pub sweeps_sampled: u32,
    /// The district's inventory revision at the last sweep.
    pub district_revision: Option<u32>,
    pub probe_set: Vec<u32>,
    pub probes: BTreeMap<u32, ProbeTrack>,
    pub events: VecDeque<GlassEvent>,
    /// The glass replay's regimes, breakpoints and oscillation fallbacks since the compile.
    pub replay_regimes: u32,
    pub replay_breakpoints: u32,
    pub replay_oscillations: u32,
    /// The compile cut into sections (SECTIONS.md §3): recomputed from the graph at the accept and
    /// on restore (`page::read_district_page`), never saved but for the fingerprints the page checks.
    pub sections: Sections,
    /// The sections' judging units (§4.3, `units.rs`): recomputed with the sections, never saved.
    pub units: Units,
    /// The station port sets the units join (`Units::build`, choice SC75): the district's
    /// (`DistrictGlass::joins`) when they number this compile's revision's inventories, else none.
    pub joins: Vec<Vec<u32>>,
    /// Each unit's bounds per good under the current solve (choice SC56), by (unit, good):
    /// recomputed at every solve and on restore, never saved. Empty unless the units are active.
    pub ubounds: Vec<UnitBound>,
    /// Per unit: its learning state. Empty unless the units are [`Units::active`]: one unit's
    /// window would be the district's.
    pub unit_state: Vec<UnitState>,
    /// The units' open windows' rows, sorted by (unit, good).
    rows: Vec<Row>,
    /// Per inventory a unit holds or a counted store between units holds: the ports into it since
    /// the compile (cumulative, positive in), what a unit's measured series and a counted store's
    /// residue subtract (§4.4). Kept across a recompile of the same revision, with the windows it
    /// carries.
    pub ports_at: VecMap<u32, GoodVec>,
    /// The latest sweep's IND_COUNT_SAMPLE (§4.2). Not saved.
    pub counts: Option<CountTake>,
}

/// One district's glass state.
#[derive(Clone, Debug, Default)]
pub struct DistrictGlass {
    /// Discards and skips by `report::discard` reason, and the latest (tick, reason).
    pub discards: [u64; report::discard::COUNT],
    pub last_discard: Option<(Tick, u32)>,
    pub asm: super::store::Assembler,
    pub compiled: Option<Compiled>,
    /// The latest not-compilable END: (compile, reason).
    pub not_compilable: Option<(u32, u32)>,
    /// Chunks of the district that block-tick (IND_CHUNK_TICKING), for G2's partial loads.
    pub ticking: BTreeSet<crate::ids::ChunkPos>,
    /// Model codes demoted to measured here (G-B14), kept across compiles.
    pub demoted: BTreeSet<u32>,
    /// The glass replay running while the district is away (G2), if any.
    pub replay: Option<super::replay::Replay>,
    /// What replays the district now ([`report::replay_model`]) and since when.
    pub replay_model: u32,
    pub replay_since: Option<Tick>,
    /// Why this compile's glass replay fell back (cleared by a new compile or a live district).
    pub fallback: Option<super::replay::Fallback>,
    /// A session started since the compile was accepted: Java numbers each session's compiles
    /// from 1 (`GlassCompiler.State`), so the next END is judged by what it compiled, not by its
    /// number. The same factory keeps the compile held, its passed window and its replay
    /// (choice G2-81).
    pub numbers_reset: bool,
    /// The sections a compile of this district is cut into at most, below `glass.sectionsMax`,
    /// once its extra rows passed a row cap (SECTIONS.md §7.2): kept across compiles (choice SC57).
    pub sections_cap: Option<u32>,
    /// Chunks that stopped running (unloaded, or stopped block-ticking) since the last sweep that
    /// sampled: a unit lying in one did not run throughout that sweep (§3.4).
    pub stopped: BTreeSet<crate::ids::ChunkPos>,
    /// The district went away ([`super::Glass::away`]) and is not back yet ([`super::Glass::back`]):
    /// no unit runs (choice SC59).
    pub away: bool,
    /// The inventory sets the district's stations' ports name (IND_STATION_PORTS), each sorted,
    /// and the inventory revision they number: what its compiles' units join (choice SC75).
    pub joins: Option<(u32, Vec<Vec<u32>>)>,
}

/// A counted pair's mover and store positions, and the game time its counters started at (§4.2).
pub type PairSince = (BlockPos, BlockPos, i64);

/// What a sweep sees of the district.
pub struct View<'a> {
    pub revision: u32,
    /// The sweep's number: the count sample it reads is this sweep's (SECTIONS.md §4.2).
    pub sweep: u32,
    pub inventories: &'a VecMap<u32, Inventory>,
    pub ports: &'a GoodVec,
}

/// The nodes the controllers stop, from their switch states (`ctrl_on`, before inversion).
pub(crate) fn gates(graph: &Graph, ctrl_on: &[bool], gate_off: &mut [bool]) {
    for g in gate_off.iter_mut() {
        *g = false;
    }
    for v in 0..graph.nodes.len() as u32 {
        for &e in graph.control_in.of(v) {
            let ed = graph.edge(e);
            // The param already includes the inversion (GB51).
            let signal = ctrl_on[ed.from as usize];
            let off = match ed.param {
                CONTROL_STOP_WHEN_ON => signal,
                CONTROL_RUN_WHEN_ON => !signal,
                _ => false,
            };
            if off {
                gate_off[v as usize] = true;
            }
        }
    }
}

/// What a store must hold of a good to supply it. A good the graph also brings into the store
/// needs `glass.holdingStacks` stacks (below, the store passes on what it receives: a chest a funnel
/// keeps nearly empty is not a source); a good nothing brings in needs one unit (GB100).
pub(crate) fn holding_threshold(goods: &dyn GoodLookup, g: Good, stacks: i64, fed: bool) -> i64 {
    if !fed {
        return 1;
    }
    match goods.class(g) {
        GoodClass::Item => goods.max_stack(g) * stacks,
        GoodClass::Fluid => 1000 * stacks,
        _ => stacks.max(1),
    }
}

/// Least-squares slope from sums.
fn slope(n: f64, st: f64, stt: f64, sy: f64, sty: f64) -> f64 {
    let d = n * stt - st * st;
    if d <= 0.0 {
        f64::NAN
    } else {
        (n * sty - st * sy) / d
    }
}

/// Judges a window's goods by the check's rules (G-B14): the district's window and each unit's
/// alike. `samples`, `st` and `stt` are its sample count and the sums of its sample times (from
/// its start), `w` its length in ticks. Returns each good's result and whether one failed.
fn judge_goods<'a>(
    samples: u32,
    st: f64,
    stt: f64,
    w: f64,
    goods: impl Iterator<Item = (Good, &'a Acc, &'a Cum)>,
    z: f64,
) -> (Vec<GoodResult>, bool) {
    let n = f64::from(samples);
    let mut out = Vec::new();
    let mut any_fail = false;
    for (g, acc, cum) in goods {
        let measured = slope(n, st, stt, acc.sy, acc.sty);
        let predicted = slope(n, st, stt, acc.sp, acc.stp);
        let lo = if cum.lo_inf {
            f64::NEG_INFINITY
        } else {
            slope(n, st, stt, acc.slo, acc.stlo)
        };
        let hi = if cum.hi_inf {
            f64::INFINITY
        } else {
            slope(n, st, stt, acc.shi, acc.sthi)
        };
        // The slope's standard error, from the residual of measured minus predicted.
        let sxx = stt - st * st / n;
        let sxy = acc.std - st * acc.sd / n;
        let syy = acc.sdd - acc.sd * acc.sd / n;
        let se = if samples > 2 && sxx > 0.0 {
            ((syy - sxy * sxy / sxx).max(0.0) / (n - 2.0) / sxx).sqrt()
        } else {
            0.0
        };
        let model = if w > 0.0 {
            (POISSON_SLOPE_FACTOR * (cum.var / w) / w).sqrt()
        } else {
            0.0
        };
        let sigma = se.max(model);
        let q = if w > 0.0 { cum.batch.max(1.0) / w } else { 0.0 };
        let avg = |x: f64, inf: bool| {
            if inf {
                f64::INFINITY
            } else if w > 0.0 {
                x / w
            } else {
                f64::NAN
            }
        };
        let tol = z * sigma + q;
        let verdict =
            if samples < 3 || cum.class == report::class::UNBOUNDED || !measured.is_finite() {
                report::verdict::NONE
            } else if cum.class == report::class::EXACT {
                if (measured - predicted).abs() <= tol {
                    report::verdict::PASS
                } else {
                    report::verdict::FAIL
                }
            } else if lo - tol <= measured && measured <= hi + tol {
                report::verdict::PASS
            } else {
                report::verdict::FAIL
            };
        any_fail |= verdict == report::verdict::FAIL;
        out.push(GoodResult {
            good: g,
            class: cum.class,
            why: cum.why,
            in_l: avg(cum.il, false),
            in_u: avg(cum.iu, cum.iu_inf),
            out_l: avg(cum.ol, false),
            out_u: avg(cum.ou, cum.ou_inf),
            predicted,
            lo,
            hi,
            measured,
            sigma,
            q,
            verdict,
            samples,
        });
    }
    (out, any_fail)
}

/// One flow piece of `dt` ticks into a good's cumulative prediction: L in and out, U in and out
/// (infinite ones flag it), what the stores could not give (`c`, at most 0: their drain clamp),
/// the variance rate, the batch and the class with its why.
#[allow(clippy::too_many_arguments)]
fn add_flow(
    cum: &mut Cum,
    dt: f64,
    c: f64,
    (il, ol, iu, ou): (f64, f64, f64, f64),
    var_rate: f64,
    batch: f64,
    class: u32,
    why: (u32, u32),
) {
    let dp = (il - ol) * dt + c;
    cum.p += dp;
    cum.il += il * dt;
    cum.ol += ol * dt - c;
    if iu.is_finite() {
        cum.iu += iu * dt;
    } else {
        cum.iu_inf = true;
    }
    if ou.is_finite() {
        cum.ou += ou * dt - c;
    } else {
        cum.ou_inf = true;
    }
    let out_spread = ou - ol;
    let in_spread = iu - il;
    if out_spread.is_finite() {
        cum.lo += dp - out_spread * dt;
    } else {
        cum.lo_inf = true;
    }
    if in_spread.is_finite() {
        cum.hi += dp + in_spread * dt;
    } else {
        cum.hi_inf = true;
    }
    cum.var += var_rate * dt;
    cum.batch = cum.batch.max(batch);
    if class > cum.class || (class == cum.class && cum.why.0 == report::why::NONE) {
        cum.class = class;
        cum.why = why;
    }
}

/// The row of (`unit`, `good`) in rows sorted by both, inserted (with `fresh`) if missing.
fn row_mut(rows: &mut Vec<Row>, unit: u16, good: Good, fresh: impl FnOnce() -> Cum) -> &mut Row {
    let i = match rows.binary_search_by(|r| (r.unit, r.good).cmp(&(unit, good))) {
        Ok(i) => i,
        Err(i) => {
            rows.insert(
                i,
                Row {
                    unit,
                    good,
                    acc: Acc::default(),
                    cum: fresh(),
                },
            );
            i
        }
    };
    &mut rows[i]
}

/// The domains unit `u`'s sections are members of or drive, ascending.
fn unit_domains(s: &Sections, units: &Units, u: u16) -> Vec<u32> {
    let mut out: Vec<u32> = units
        .sections
        .get(u as usize)
        .into_iter()
        .flatten()
        .flat_map(|&x| s.list[x as usize].domains.iter().copied())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Per domain unit `u` is on: its lowest node position there and whether that node drives it
/// rather than turns with it, which a recompile finds the domain again by (a domain's index is
/// its compile's; Create's network id changes with its root). A domain whose anchor another
/// domain of the unit shares is left out: it cannot be found again.
fn domain_anchors(
    g: &Graph,
    s: &Sections,
    units: &Units,
    u: u16,
) -> BTreeMap<u32, (BlockPos, bool)> {
    let mut best: BTreeMap<u32, (BlockPos, bool)> = BTreeMap::new();
    for &x in units.sections.get(u as usize).into_iter().flatten() {
        for &v in &s.list[x as usize].nodes {
            if super::sections::is_virtual(g.node(v)) {
                continue;
            }
            let pos = g.node(v).pos;
            let roles = g
                .node(v)
                .domain
                .map(|d| (d, false))
                .into_iter()
                .chain(g.drives[v as usize].map(|d| (d, true)));
            for (d, drives) in roles {
                let e = best.entry(d).or_insert((pos, drives));
                if (pos, drives) < *e {
                    *e = (pos, drives);
                }
            }
        }
    }
    let mut seen: BTreeMap<(BlockPos, bool), u32> = BTreeMap::new();
    for a in best.values() {
        *seen.entry(*a).or_insert(0) += 1;
    }
    best.retain(|_, a| seen[a] == 1);
    best
}

/// Would domain `c.domain` of `g` stop at the speed its unit's passed window ran at least
/// capacity on (§3.6's domain check): Create's stress at that speed above that capacity. A domain
/// with stress disabled never does; one that no longer holds the source the speed was kept by
/// does (it cannot be checked).
fn over_capacity(g: &Graph, c: &DomCap) -> bool {
    let Some(dom) = g.domains.get(c.domain as usize) else {
        return true;
    };
    if dom.flags & super::records::domain_flags::STRESS_DISABLED != 0 {
        return false;
    }
    let Some(speed) = speed_from_key(dom, c.source, c.speed) else {
        return true;
    };
    c.capacity < super::solve::domain_stress(dom, speed)
}

/// The span of `unit`'s rows.
fn rows_of(rows: &[Row], unit: u16) -> std::ops::Range<usize> {
    rows.partition_point(|r| r.unit < unit)..rows.partition_point(|r| r.unit <= unit)
}

/// Of the failing goods (or `only` that good), the one the model is most optimistic on: it
/// predicts more throughput than measured. Its η (measured over predicted, in 0 to 1), the good,
/// and whether it is produced.
/// Adds the goods of `rows` to a unit's sorted `touched`.
fn keep_touched(touched: &mut Vec<Good>, rows: &[Row]) {
    for r in rows {
        if let Err(i) = touched.binary_search(&r.good) {
            touched.insert(i, r.good);
        }
    }
}

fn most_optimistic(goods: &[GoodResult], only: Option<Good>) -> Option<(f64, Good, bool)> {
    let mut worst: Option<(f64, Good, bool)> = None;
    for r in goods {
        if r.verdict != report::verdict::FAIL
            || r.predicted == 0.0
            || !r.predicted.is_finite()
            || only.is_some_and(|g| g != r.good)
        {
            continue;
        }
        let s = r.predicted.signum();
        if s * r.measured >= s * r.predicted {
            continue;
        }
        let eta = (r.measured / r.predicted).clamp(0.0, 1.0);
        if worst.is_none_or(|w| eta < w.0) {
            worst = Some((eta, r.good, r.predicted > 0.0));
        }
    }
    worst
}

/// The cumulative prediction a good sampled before any flow starts with: exact, or unbounded on
/// a unit or district holding a black box or a port; a batch of one.
fn sampled_cum(unbounded: bool) -> Cum {
    Cum {
        class: if unbounded {
            report::class::UNBOUNDED
        } else {
            report::class::EXACT
        },
        batch: 1.0,
        ..Cum::default()
    }
}

/// Every unit's fresh state (none unless the units are active), ready from `ready`.
fn fresh_units(units: &Units, ready: Tick) -> Vec<UnitState> {
    if !units.active() {
        return Vec::new();
    }
    vec![
        UnitState {
            ready_at: ready,
            ..UnitState::default()
        };
        units.len()
    ]
}

/// One sample of a good at `t` ticks into its window: measured `y`, the cumulative prediction
/// and its bounds as integrated so far.
fn add_sample(acc: &mut Acc, cum: &Cum, t: f64, y: f64) {
    acc.sy += y;
    acc.sty += t * y;
    acc.sp += cum.p;
    acc.stp += t * cum.p;
    acc.slo += cum.lo;
    acc.stlo += t * cum.lo;
    acc.shi += cum.hi;
    acc.sthi += t * cum.hi;
    let d = y - cum.p;
    acc.sd += d;
    acc.std += t * d;
    acc.sdd += d * d;
}

impl Compiled {
    pub fn new(graph: Graph, at: Tick, tun: &GlassTunables, demoted: &BTreeSet<u32>) -> Compiled {
        Compiled::new_cut(graph, at, tun, demoted, tun.sections_max, &[])
    }

    /// [`Compiled::new`], cut into at most `max` sections (`glass.sectionsMax`, or a district's
    /// row cap below it).
    pub fn new_cut(
        graph: Graph,
        at: Tick,
        tun: &GlassTunables,
        demoted: &BTreeSet<u32>,
        max: usize,
        joins: &[Vec<u32>],
    ) -> Compiled {
        let n = graph.nodes.len();
        let window_target = if graph.end.window == 0 {
            tun.checksum_window_min
        } else {
            u64::from(graph.end.window)
        };
        let ready_at = Tick(at.0 + tun.warmup_ticks + graph.longest_latency);
        let ctrl_on = graph
            .nodes
            .iter()
            .map(|nd| {
                nd.kind == NodeKind::Controller && {
                    let (on_above, off_below, flags, level) =
                        (nd.params[0], nd.params[1], nd.params[2], nd.params[3]);
                    if level >= on_above {
                        true
                    } else if level <= off_below {
                        false
                    } else {
                        flags & CONTROLLER_ON_AT_COMPILE != 0
                    }
                }
            })
            .collect();
        let sections = Sections::cut_counted(&graph, max, tun.sections_max, tun.counted_stores_max);
        let units = Units::build(&graph, &sections, joins);
        let mut regime = Regime::new(n);
        regime.demoted = demoted.clone();
        // Heaters lit at the compile spend their burn from here (§9.3); a creative one never does.
        let mut heaters = Vec::new();
        for (v, nd) in graph.nodes.iter().enumerate() {
            let creative = nd.params[2] <= 0 && graph.node_recipes.of(v as u32).is_empty();
            if nd.kind != NodeKind::Heater || creative || nd.params[0] <= 0 {
                continue;
            }
            let b = Burn {
                node: v as u32,
                left: nd.params[0] as f64,
                thr: nd.params[3].max(0) as f64,
            };
            regime.burning[v] = b.phase();
            heaters.push(b);
        }
        Compiled {
            graph,
            accepted_at: at,
            window_target,
            ready_at,
            ctrl_on,
            regime,
            key: 0,
            sol: Solution::default(),
            cache: VecDeque::new(),
            cache_parts: 1,
            solves: 0,
            cache_hits: 0,
            solve_micros: 0.0,
            win: None,
            last: None,
            passed: None,
            edited: false,
            veto: Vec::new(),
            derated_at: None,
            windows_completed: 0,
            strikes: 0,
            stock_est: BTreeMap::new(),
            last_sweep: None,
            heaters,
            model_at: at,
            sweeps: 0,
            sweeps_sampled: 0,
            district_revision: None,
            probe_set: Vec::new(),
            probes: BTreeMap::new(),
            events: VecDeque::new(),
            replay_regimes: 0,
            replay_breakpoints: 0,
            replay_oscillations: 0,
            unit_state: fresh_units(&units, ready_at),
            sections,
            units,
            joins: joins.to_vec(),
            ubounds: Vec::new(),
            rows: Vec::new(),
            ports_at: VecMap::new(),
            counts: None,
        }
    }

    /// Cuts the compile into at most `max` sections (a district's row cap, SECTIONS.md §7.2),
    /// keeping each unit the cut leaves as it was (by its sections' fingerprints) with its window
    /// and verdicts; a merged unit starts learning after a fresh warm-up from `at`.
    pub fn recut(&mut self, max: usize, at: Tick, tun: &GlassTunables) {
        let sections =
            Sections::cut_counted(&self.graph, max, tun.sections_max, tun.counted_stores_max);
        let units = Units::build(&self.graph, &sections, &self.joins);
        let ready = self
            .ready_at
            .max(Tick(at.0 + tun.warmup_ticks + self.graph.longest_latency));
        let old_units = std::mem::replace(&mut self.units, units);
        let old_state = std::mem::replace(&mut self.unit_state, fresh_units(&self.units, ready));
        let old_rows = std::mem::take(&mut self.rows);
        self.sections = sections;
        self.adopt(&old_units, old_state, old_rows, false);
        self.ubounds = units::bounds(&self.graph, &self.units, &self.sol);
        self.forget_idle_state();
    }

    /// Joins the units anew by `joins` (a station's ports changed, choice SC75), keeping each unit
    /// that stays as it was, as [`Compiled::recut`] does. False when the units are unchanged.
    pub fn regroup(&mut self, joins: &[Vec<u32>], at: Tick, tun: &GlassTunables) -> bool {
        if self.joins == joins {
            return false;
        }
        self.joins = joins.to_vec();
        let units = Units::build(&self.graph, &self.sections, &self.joins);
        if units == self.units {
            return false;
        }
        let ready = self
            .ready_at
            .max(Tick(at.0 + tun.warmup_ticks + self.graph.longest_latency));
        let old_units = std::mem::replace(&mut self.units, units);
        let old_state = std::mem::replace(&mut self.unit_state, fresh_units(&self.units, ready));
        let old_rows = std::mem::take(&mut self.rows);
        self.adopt(&old_units, old_state, old_rows, false);
        self.ubounds = units::bounds(&self.graph, &self.units, &self.sol);
        self.forget_idle_state();
        true
    }

    /// Takes over each unit of an older cut (`old_units`, with its states and rows) that this cut
    /// has unchanged: the same sections by fingerprint, each key once on both sides. A recompile
    /// (`recompiled`) also clears its edit: the compile found it as it was (§3.6).
    fn adopt(
        &mut self,
        old_units: &Units,
        mut old_state: Vec<UnitState>,
        old_rows: Vec<Row>,
        recompiled: bool,
    ) -> BTreeMap<u16, u16> {
        if !self.units.active() || old_state.len() != old_units.len() {
            return BTreeMap::new();
        }
        let once = |keys: &[Vec<u64>]| {
            let mut n: BTreeMap<&[u64], (u32, u16)> = BTreeMap::new();
            for (u, k) in keys.iter().enumerate() {
                let e = n.entry(k.as_slice()).or_insert((0, u as u16));
                e.0 += 1;
            }
            n.into_iter()
                .filter(|(_, (c, _))| *c == 1)
                .map(|(k, (_, u))| (k.to_vec(), u))
                .collect::<BTreeMap<Vec<u64>, u16>>()
        };
        let old = once(&old_units.keys);
        let new = once(&self.units.keys);
        let mut to: BTreeMap<u16, u16> = BTreeMap::new();
        for (k, &u) in &new {
            if let Some(&o) = old.get(k) {
                to.insert(o, u);
            }
        }
        // A derate does not outlive its compile: a unit with one on its nodes was judged against a
        // model the new compile does not have, and starts afresh (choice SC61).
        if recompiled {
            to.retain(|&o, _| old_state[o as usize].derated_at.is_none());
        }
        for (&o, &u) in &to {
            let mut st = std::mem::take(&mut old_state[o as usize]);
            if recompiled {
                st.edited = false;
            }
            self.unit_state[u as usize] = st;
        }
        let mut rows: Vec<Row> = old_rows
            .into_iter()
            .filter_map(|mut r| {
                let &u = to.get(&r.unit)?;
                r.unit = u;
                Some(r)
            })
            .collect();
        rows.sort_by_key(|r| (r.unit, r.good));
        self.rows = rows;
        to
    }

    /// With every unit window closed, the interval clock and the drain clamp go, as the district's
    /// discard has always cleared them.
    fn forget_idle_state(&mut self) {
        if self.win.is_none() && self.unit_state.iter().all(|s| s.win.is_none()) {
            self.last_sweep = None;
            self.stock_est.clear();
        }
    }

    /// A recompile of the same revision (G-B3, SECTIONS.md §3.6): each unit the new compile has
    /// unchanged keeps its passed window, strikes and open window, which the old compile's model
    /// integrates up to `at` first; the ports into its inventories go on from where they were.
    pub fn carry_from(&mut self, old: &mut Compiled, at: Tick, tun: &GlassTunables) {
        if old.graph.end.revision != self.graph.end.revision {
            return;
        }
        old.advance(at, tun);
        self.ports_at = std::mem::take(&mut old.ports_at);
        self.derated_at = old.derated_at;
        let old_state = std::mem::take(&mut old.unit_state);
        let old_rows = std::mem::take(&mut old.rows);
        let to = self.adopt(&old.units, old_state, old_rows, true);
        self.check_domains(old, &to, at, tun);
        if self.unit_state.iter().any(|s| s.win.is_some()) {
            self.last_sweep = old.last_sweep;
            self.stock_est = std::mem::take(&mut old.stock_est);
        }
    }

    /// The units a recompile carried (`to`, old unit to new): their domains found again by their
    /// own nodes, then the domain check (SECTIONS.md §3.6). A unit whose passed window ran a domain
    /// at a capacity below the new compile's stress there, or a domain the new compile does not
    /// hold, goes stale, as an edit in its box makes it: it stops replaying and learns again.
    fn check_domains(
        &mut self,
        old: &Compiled,
        to: &BTreeMap<u16, u16>,
        at: Tick,
        tun: &GlassTunables,
    ) {
        for (&o, &u) in to {
            let from = domain_anchors(&old.graph, &old.sections, &old.units, o);
            let back: BTreeMap<(BlockPos, bool), u32> =
                domain_anchors(&self.graph, &self.sections, &self.units, u)
                    .into_iter()
                    .map(|(d, a)| (a, d))
                    .collect();
            let map = |c: &DomCap| {
                let d = *back.get(from.get(&c.domain)?)?;
                Some(DomCap { domain: d, ..*c })
            };
            let st = &mut self.unit_state[u as usize];
            let open: Option<Vec<DomCap>> = st.caps.iter().map(map).collect();
            let passed: Option<Vec<DomCap>> = st.passed_caps.iter().map(map).collect();
            let mut stale = false;
            match open {
                Some(mut c) => {
                    c.sort_by(cap_order);
                    st.caps = c;
                }
                None => stale = true,
            }
            match passed {
                Some(mut c) => {
                    c.sort_by(cap_order);
                    stale |= st.passed.is_some() && c.iter().any(|x| over_capacity(&self.graph, x));
                    st.passed_caps = c;
                }
                None => stale = true,
            }
            if stale {
                st.edited = true;
                st.passed_caps.clear();
                self.discard_unit(u, at, tun);
            }
        }
    }

    fn event(&mut self, e: GlassEvent) {
        if self.events.len() >= EVENTS_KEPT {
            self.events.pop_front();
        }
        self.events.push_back(e);
    }

    /// The contents of a store node's inventory, when the sampler reads it in this revision.
    fn contents<'v>(&self, v: u32, view: &View<'v>) -> Option<&'v GoodVec> {
        let node = self.graph.node(v);
        if node.kind != NodeKind::Store || view.revision != self.graph.end.revision {
            return None;
        }
        view.inventories.get(&node.inventory?).map(|i| &i.known)
    }

    /// Reads the stores and controllers into the regime (no solve).
    fn read_state(&mut self, view: &View<'_>, goods: &dyn GoodLookup, tun: &GlassTunables) {
        let n = self.graph.nodes.len();
        for v in 0..n as u32 {
            let node = self.graph.node(v);
            if node.kind != NodeKind::Store {
                continue;
            }
            let Some(c) = self.contents(v, view) else {
                self.regime.read[v as usize] = false;
                self.regime.held[v as usize].clear();
                self.regime.room[v as usize] = true;
                continue;
            };
            let decl = &view.inventories[&node.inventory.expect("read")].decl;
            self.regime.read[v as usize] = true;
            let graph = &self.graph;
            let fed = |g: Good| graph.in_g.of(v).iter().any(|&e| graph.allows(e, g));
            let held: Vec<Good> = c
                .iter()
                .filter(|&(g, a)| a >= holding_threshold(goods, g, tun.holding_stacks, fed(g)))
                .map(|(g, _)| g)
                .collect();
            self.regime.held[v as usize] = held;
            let fluid = self.graph.is_fluid(v);
            self.regime.room[v as usize] = if fluid {
                let cap = if node.capacity > 0 {
                    node.capacity
                } else {
                    decl.fluid_capacity
                };
                let used: i64 = c
                    .iter()
                    .filter(|&(g, _)| goods.class(g) == GoodClass::Fluid)
                    .map(|(_, a)| a)
                    .sum();
                cap <= 0 || cap - used >= 1000
            } else {
                let cap = if node.capacity > 0 {
                    node.capacity
                } else {
                    i64::from(decl.item_slots)
                };
                let used: i64 = c
                    .iter()
                    .filter(|&(g, _)| goods.class(g) == GoodClass::Item)
                    .map(|(g, a)| (a + goods.max_stack(g) - 1) / goods.max_stack(g))
                    .sum();
                cap <= 0 || used < cap
            };
        }
        // Controllers: hysteresis on the observed stock (G-B8, evaluated per sweep in G1).
        for v in 0..n as u32 {
            let node = self.graph.node(v);
            if node.kind != NodeKind::Controller {
                continue;
            }
            let mut level: Option<i64> = None;
            for &e in self.graph.observe_in.of(v) {
                let ed = self.graph.edge(e);
                let Some(c) = self.contents(ed.from, view) else {
                    continue;
                };
                let sum: i64 = c
                    .iter()
                    .filter(|&(g, _)| self.graph.allows(e, g))
                    .map(|(_, a)| a)
                    .sum();
                level = Some(level.unwrap_or(0) + sum);
            }
            if let Some(l) = level {
                if l >= node.params[0] {
                    self.ctrl_on[v as usize] = true;
                } else if l <= node.params[1] {
                    self.ctrl_on[v as usize] = false;
                }
            }
        }
        gates(&self.graph, &self.ctrl_on, &mut self.regime.gate_off);
    }

    /// Solves the current regime if its key changed (or from the cache).
    fn resolve(&mut self, tun: &GlassTunables, force: bool) {
        let key = self.regime.key();
        if !force && key == self.key && self.solves > 0 {
            return;
        }
        self.key = key;
        if let Some(i) = self.cache.iter().position(|(k, _)| *k == key) {
            let (k, sol) = self.cache.remove(i).expect("found");
            self.sol = sol.clone();
            self.cache.push_back((k, sol));
            self.cache_hits += 1;
            self.ubounds = units::bounds(&self.graph, &self.units, &self.sol);
            return;
        }
        let t0 = std::time::Instant::now();
        let sol = solve::solve(&self.graph, &self.regime, tun);
        self.solve_micros = t0.elapsed().as_secs_f64() * 1e6;
        self.solves += 1;
        self.sol = sol.clone();
        if tun.regime_cache > 0 {
            while self.cache.len() >= tun.regime_cache * self.cache_parts.max(1) {
                self.cache.pop_front();
            }
            self.cache.push_back((key, sol));
        }
        self.ubounds = units::bounds(&self.graph, &self.units, &self.sol);
    }

    /// Solves any regime through the regime cache (the glass replay's regimes, G2): a cached
    /// regime costs a lookup. The compile's own regime and solution are untouched.
    pub fn solve_regime(&mut self, rg: &Regime, tun: &GlassTunables) -> Solution {
        let key = rg.key();
        if let Some(i) = self.cache.iter().position(|(k, _)| *k == key) {
            let (k, sol) = self.cache.remove(i).expect("found");
            self.cache.push_back((k, sol.clone()));
            self.cache_hits += 1;
            return sol;
        }
        let t0 = std::time::Instant::now();
        let sol = solve::solve(&self.graph, rg, tun);
        self.solve_micros = t0.elapsed().as_secs_f64() * 1e6;
        self.solves += 1;
        if tun.regime_cache > 0 {
            while self.cache.len() >= tun.regime_cache * self.cache_parts.max(1) {
                self.cache.pop_front();
            }
            self.cache.push_back((key, sol.clone()));
        }
        sol
    }

    /// Heaters lit on their compile-time burn, as the model has spent them to [`Compiled::model_at`].
    pub fn heaters(&self) -> &[Burn] {
        &self.heaters
    }

    /// The tick the model's heaters have reached.
    pub fn model_at(&self) -> Tick {
        self.model_at
    }

    /// May the district replay on this compile (G-B4): a window of it passed, and no edit since.
    pub fn eligible(&self) -> bool {
        self.passed.is_some() && !self.edited
    }

    /// May the district replay at all (SECTIONS.md §3.5): with judging units, while one of them may
    /// ([`Compiled::unit_may_replay`]); with one unit or none, by the district's rule.
    pub fn replayable(&self) -> bool {
        if self.unit_state.is_empty() {
            return self.eligible();
        }
        (0..self.unit_state.len() as u16).any(|u| self.unit_may_replay(u))
    }

    /// Per unit: may it replay now ([`Compiled::unit_may_replay`]); empty with one unit or none.
    pub fn units_replaying(&self) -> Vec<bool> {
        (0..self.unit_state.len() as u16)
            .map(|u| self.unit_may_replay(u))
            .collect()
    }

    /// The first solve, at acceptance, from what the engine knows of the inventories.
    pub fn start(&mut self, view: &View<'_>, goods: &dyn GoodLookup, tun: &GlassTunables) {
        self.read_state(view, goods, tun);
        self.resolve(tun, true);
    }

    /// Discards every open window, the district's and each unit's; the next start after a fresh
    /// warm-up.
    pub fn discard(&mut self, at: Tick, tun: &GlassTunables) {
        for u in 0..self.unit_state.len() as u16 {
            self.discard_unit(u, at, tun);
        }
        self.discard_district(at, tun);
    }

    /// Discards the district's open window alone (the units keep theirs); the next starts after a
    /// fresh warm-up.
    pub fn discard_district(&mut self, at: Tick, tun: &GlassTunables) {
        self.win = None;
        self.ready_at = self
            .ready_at
            .max(Tick(at.0 + tun.warmup_ticks + self.graph.longest_latency));
        self.forget_idle_state();
    }

    /// Discards unit `u`'s open window and its rows; its next starts after a fresh warm-up.
    pub fn discard_unit(&mut self, u: u16, at: Tick, tun: &GlassTunables) {
        let latency = self.graph.longest_latency;
        let Some(st) = self.unit_state.get_mut(u as usize) else {
            return;
        };
        st.ready_at = st.ready_at.max(Tick(at.0 + tun.warmup_ticks + latency));
        self.restart_unit(u);
    }

    /// Closes unit `u`'s open window unjudged, with its rows: a new one may open at once (a pair
    /// whose counters started again, §4.2).
    fn restart_unit(&mut self, u: u16) {
        let Some(st) = self.unit_state.get_mut(u as usize) else {
            return;
        };
        st.win = None;
        st.caps.clear();
        st.count_since.clear();
        st.rbase.clear();
        let span = rows_of(&self.rows, u);
        keep_touched(&mut st.touched, &self.rows[span.clone()]);
        self.rows.drain(span);
    }

    /// Per unit: its pairs' `since` by position in this sweep's count sample, sorted, or None when
    /// the sample lacks one of them (§4.2); empty for a unit with no counted store.
    fn count_cover(&self, sweep: u32) -> Vec<Option<Vec<PairSince>>> {
        let take = self.counts.as_ref().filter(|t| t.sweep == sweep);
        self.units
            .pairs_of
            .iter()
            .map(|ps| {
                let mut out = Vec::with_capacity(ps.len());
                for &(m, h) in ps {
                    let since = take?.since_of((m, h))?;
                    out.push((self.graph.node(m).pos, self.graph.node(h).pos, since));
                }
                out.sort_unstable();
                Some(out)
            })
            .collect()
    }

    /// IND_COUNT_SAMPLE of sweep `sweep` for this compile: the counters of the pairs of the count
    /// set it names; several records of one sweep add up, and a later sweep's replaces them. A pair
    /// whose entries are fewer than its records say ([`Count::entries`]) gives no cover.
    pub fn count_sample(&mut self, sweep: u32, counts: &[super::records::Count]) {
        if self.counts.as_ref().is_none_or(|t| t.sweep != sweep) {
            self.counts = Some(CountTake {
                sweep,
                ..CountTake::default()
            });
        }
        let units = &self.units;
        let t = self.counts.as_mut().expect("set");
        for c in counts {
            let Ok(i) = units.pairs.binary_search(&(c.mover, c.store)) else {
                continue;
            };
            t.since.insert((c.mover, c.store), c.since);
            *t.got.entry((c.mover, c.store)).or_insert(0) += 1;
            let said = t.stated.entry((c.mover, c.store)).or_insert(0);
            *said = (*said).max(c.entries);
            // Each count once, though Java names it under every pair of movers at one block
            let dirs = units.pair_dirs[i];
            let moved_in = if dirs & PAIR_IN != 0 { c.moved_in } else { 0 };
            let moved_out = if dirs & PAIR_OUT != 0 { c.moved_out } else { 0 };
            if let Some(g) = c.good {
                t.moved.insert((c.mover, c.store, g), (moved_in, moved_out));
            }
        }
    }

    /// An edit (SECTIONS.md §3.6): the district's window goes, as it covers every section, and the
    /// compile no longer matches the blocks until a window passes again or Java recompiles (G-B3,
    /// G-B4). Of the units, those whose sections' node boxes hold `pos` go stale and lose their
    /// windows; every unit does for an edit with no place (a command) or on a moving frame, whose
    /// positions are not the world's.
    pub fn edit(&mut self, at: Tick, pos: Option<BlockPos>, tun: &GlassTunables) {
        let hit: Vec<u16> = match pos {
            Some(p) if self.graph.end.frame == 0 => self.units.at(&self.sections, p),
            _ => (0..self.unit_state.len() as u16).collect(),
        };
        for u in hit {
            if let Some(st) = self.unit_state.get_mut(u as usize) {
                st.edited = true;
                self.discard_unit(u, at, tun);
            }
        }
        self.edited = true;
        self.discard_district(at, tun);
    }

    /// Each read store's level per good, each inventory once.
    fn store_levels(&self, view: &View<'_>) -> BTreeMap<(u32, Good), i64> {
        let mut out = BTreeMap::new();
        for v in 0..self.graph.nodes.len() as u32 {
            let Some(inv) = self.graph.node(v).inventory else {
                continue;
            };
            let Some(c) = self.contents(v, view) else {
                continue;
            };
            for (g, a) in c.iter() {
                out.insert((inv, g), a);
            }
        }
        out
    }

    /// The district's measured stock: the read stores' inventories, each counted once.
    fn stock(&self, view: &View<'_>) -> BTreeMap<Good, i64> {
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        let mut s: BTreeMap<Good, i64> = BTreeMap::new();
        for v in 0..self.graph.nodes.len() as u32 {
            let node = self.graph.node(v);
            if node.kind != NodeKind::Store {
                continue;
            }
            let Some(inv) = node.inventory else { continue };
            let Some(c) = self.contents(v, view) else {
                continue;
            };
            if !seen.insert(inv) {
                continue;
            }
            for (g, a) in c.iter() {
                *s.entry(g).or_insert(0) += a;
            }
        }
        s
    }

    /// Advances the model from where it was to `at`: heaters spend their compile-time burn, a heater
    /// changing phase cuts the interval and solves again (cached regimes cost nothing), and the open
    /// window integrates each piece under the regime it was in.
    fn advance(&mut self, at: Tick, tun: &GlassTunables) {
        let end = at.0 as f64;
        let mut t = self.model_at.0 as f64;
        let integrating = (self.win.is_some() || self.unit_state.iter().any(|s| s.win.is_some()))
            && self.last_sweep.is_some();
        // Each heater changes phase at most twice: the loop ends.
        let mut pieces = 0usize;
        while t < end && pieces <= 2 * self.heaters.len() + 1 {
            pieces += 1;
            let mut dt = end - t;
            let mut cut = false;
            for h in &self.heaters {
                let fed = self.sol.fed.get(h.node as usize).copied().unwrap_or(0.0);
                if let Some(d) = h.until(fed)
                    && d < dt
                {
                    dt = d.max(0.0);
                    cut = true;
                }
            }
            if integrating {
                self.integrate(dt);
            }
            let mut changed = false;
            for h in self.heaters.iter_mut() {
                let fed = self.sol.fed.get(h.node as usize).copied().unwrap_or(0.0);
                let before = h.phase();
                let until = h.until(fed);
                h.left -= h.spend(fed) * dt;
                // Land exactly on a phase boundary this piece was cut at.
                if before == burn::REFUSING && until.is_some_and(|u| u <= dt + 1e-9) {
                    h.left = h.thr;
                } else if before == burn::TAKING && until.is_some_and(|u| u <= dt + 1e-9) {
                    h.left = 0.0;
                }
                let after = h.phase();
                if after != before {
                    self.regime.burning[h.node as usize] = after;
                    changed = true;
                }
            }
            t += dt;
            if changed {
                self.resolve(tun, false);
            } else if !cut {
                break;
            }
        }
        self.model_at = at;
    }

    /// The tick the next heater changes phase, if one will.
    pub fn next_breakpoint(&self) -> Option<u64> {
        let mut best: Option<f64> = None;
        for h in &self.heaters {
            let fed = self.sol.fed.get(h.node as usize).copied().unwrap_or(0.0);
            if let Some(d) = h.until(fed) {
                best = Some(best.map_or(d, |b: f64| b.min(d)));
            }
        }
        best.map(|d| self.model_at.0 + d.max(0.0).ceil() as u64)
    }

    /// Heaters still lit on their compile-time burn.
    pub fn heaters_burning(&self) -> u32 {
        self.heaters
            .iter()
            .filter(|h| h.phase() != burn::OUT)
            .count() as u32
    }

    /// Integrates `dt` ticks of the current regime into the open windows: the district's on its
    /// goods' bounds, each open unit's on its own (choice SC56). The stores' drain clamp is shared:
    /// what a store could not give is short in the district's good and in its unit's.
    fn integrate(&mut self, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        let units_open = self.unit_state.iter().any(|s| s.win.is_some());
        if self.win.is_none() && !units_open {
            return;
        }
        // What the stores could not give (they held too little), per good and per unit's good.
        let mut corr: BTreeMap<Good, f64> = BTreeMap::new();
        let mut ucorr: BTreeMap<(u16, Good), f64> = BTreeMap::new();
        // At a counted store between units: (store, good, short), shared after the loop.
        let mut short: Vec<(u32, Good, f64)> = Vec::new();
        for &(inv, g, net) in &self.sol.per_store {
            let est = self.stock_est.entry((inv, g)).or_insert(0.0);
            let raw = net * dt;
            let clamped = raw.max(-*est);
            *est += clamped;
            if let Some(win) = self.win.as_mut() {
                win.stores.entry((inv, g)).or_default().0 += clamped;
            }
            if clamped != raw {
                *corr.entry(g).or_insert(0.0) += clamped - raw;
                if units_open {
                    if let Some(u) = self.units.unit_of_inv(inv) {
                        *ucorr.entry((u, g)).or_insert(0.0) += clamped - raw;
                    } else if let Some(h) = self.units.counted_store_of_inv(inv) {
                        short.push((h, g, clamped - raw));
                    }
                }
            }
        }
        // What a counted store could not give is short in its takers' units, each by its share of
        // the predicted take there (§4.1: a unit's own edges at the store are its flows).
        for (h, g, c) in short {
            let mut takers: BTreeMap<u16, f64> = BTreeMap::new();
            for se in &self.sol.store_edges {
                let ed = self.graph.edge(se.edge);
                if se.into || se.good != g || ed.from != h || se.flow <= 0.0 {
                    continue;
                }
                let u = self.units.of_node[ed.to as usize];
                if u != super::sections::NONE {
                    *takers.entry(u).or_insert(0.0) += se.flow;
                }
            }
            let total: f64 = takers.values().sum();
            if total > 0.0 {
                for (u, f) in takers {
                    *ucorr.entry((u, g)).or_insert(0.0) += c * f / total;
                }
            }
        }
        if let Some(win) = self.win.as_mut() {
            for gb in &self.sol.goods {
                let (_, cum) = win.goods.entry(gb.good).or_default();
                let c = corr.get(&gb.good).copied().unwrap_or(0.0);
                let flows = (
                    gb.in_l.to_f64(),
                    gb.out_l.to_f64(),
                    gb.in_u.to_f64(),
                    gb.out_u.to_f64(),
                );
                add_flow(cum, dt, c, flows, gb.var_rate, gb.batch, gb.class, gb.why);
            }
        }
        if units_open {
            for ub in &self.ubounds {
                if self.unit_state[ub.unit as usize].win.is_none() {
                    continue;
                }
                let c = ucorr.get(&(ub.unit, ub.good)).copied().unwrap_or(0.0);
                let row = row_mut(&mut self.rows, ub.unit, ub.good, Cum::default);
                let flows = (ub.in_l, ub.out_l, ub.in_u, ub.out_u);
                add_flow(
                    &mut row.cum,
                    dt,
                    c,
                    flows,
                    ub.var,
                    ub.batch,
                    ub.class,
                    ub.why,
                );
            }
        }
    }

    /// Adds the sample at `at` to the open window.
    fn sample(
        &mut self,
        at: Tick,
        stock: &BTreeMap<Good, i64>,
        per: &BTreeMap<(u32, Good), i64>,
        ports: &GoodVec,
        unbounded: bool,
    ) {
        let Some(win) = self.win.as_mut() else { return };
        let t = at.0.saturating_sub(win.start.0) as f64;
        win.n += 1;
        win.st += t;
        win.stt += t * t;
        win.last = at;
        for &k in per.keys() {
            win.stores.entry(k).or_default();
        }
        for (k, (p, acc)) in win.stores.iter_mut() {
            let y = per.get(k).copied().unwrap_or(0) as f64;
            acc[0] += y;
            acc[1] += t * y;
            acc[2] += *p;
            acc[3] += t * *p;
        }
        for &g in stock.keys() {
            win.goods
                .entry(g)
                .or_insert_with(|| (Acc::default(), sampled_cum(unbounded)));
        }
        for (g, _) in ports.iter() {
            win.goods.entry(g).or_default();
        }
        for (g, (acc, cum)) in win.goods.iter_mut() {
            let y = stock.get(g).copied().unwrap_or(0) as f64 - ports.get(*g) as f64;
            add_sample(acc, cum, t, y);
        }
    }

    /// Adds the sample at `at` to the open windows of the units `take` names: each unit's stores'
    /// stock less the ports into its inventories, per good (SECTIONS.md §4.1, the units' stores
    /// interior), plus at its counted stores what its own movers moved in less what they moved
    /// out and the stores' residue since its window's first sample (`cover`: the pairs' `since`
    /// per unit, [`Compiled::count_cover`], kept at that first sample).
    fn sample_units(
        &mut self,
        at: Tick,
        view: &View<'_>,
        take: &[bool],
        cover: &[Option<Vec<PairSince>>],
    ) {
        if !take.iter().any(|&x| x) || view.revision != self.graph.end.revision {
            return;
        }
        let mut y: BTreeMap<(u16, Good), i64> = BTreeMap::new();
        for (u, stores) in self.units.stores.iter().enumerate() {
            if !take[u] {
                continue;
            }
            for inv in stores {
                if let Some(i) = view.inventories.get(inv) {
                    for (g, a) in i.known.iter() {
                        *y.entry((u as u16, g)).or_insert(0) += a;
                    }
                }
            }
        }
        for (inv, &u) in self.units.of_inv.iter() {
            if !take[u as usize] {
                continue;
            }
            if let Some(p) = self.ports_at.get(inv) {
                for (g, a) in p.iter() {
                    *y.entry((u, g)).or_insert(0) -= a;
                }
            }
        }
        self.sample_counted(view, take, cover, &mut y);
        // A unit holding a node of a model demoted here (G-B14) is unbounded as one holding a
        // black box is: the solve treats the node as one.
        let mut unbounded = self.units.unbounded.clone();
        if !self.regime.demoted.is_empty() {
            for (v, n) in self.graph.nodes.iter().enumerate() {
                let u = self.units.of_node[v];
                if u != super::sections::NONE && self.regime.demoted.contains(&n.model) {
                    unbounded[u as usize] = true;
                }
            }
        }
        for &(u, g) in y.keys() {
            let ub = unbounded[u as usize];
            row_mut(&mut self.rows, u, g, || sampled_cum(ub));
        }
        let mut times = vec![0.0; take.len()];
        for (u, st) in self.unit_state.iter_mut().enumerate() {
            let Some(w) = st.win.as_mut().filter(|_| take[u]) else {
                continue;
            };
            let t = at.0.saturating_sub(w.start.0) as f64;
            w.n += 1;
            w.st += t;
            w.stt += t * t;
            w.last = at;
            times[u] = t;
        }
        // The least capacity each unit's domains ran at, per speed, in this sample's regime (§3.3).
        for (u, &took) in take.iter().enumerate() {
            if !took || self.unit_state[u].win.is_none() {
                continue;
            }
            for d in unit_domains(&self.sections, &self.units, u as u16) {
                let Some(Some(l)) = self.sol.dom_level.get(d as usize) else {
                    continue;
                };
                let dom = &self.graph.domains[d as usize];
                let Some(lv) = dom.levels.get(*l as usize) else {
                    continue;
                };
                let capacity = self.sol.dom_cap.get(d as usize).copied().unwrap_or(0.0);
                let (source, speed) = speed_key(dom, lv.speed());
                let cap = DomCap {
                    domain: d,
                    source,
                    speed,
                    capacity,
                };
                let caps = &mut self.unit_state[u].caps;
                match caps.binary_search_by(|c| cap_order(c, &cap)) {
                    Ok(i) if caps[i].capacity <= capacity => {}
                    Ok(i) => caps[i] = cap,
                    Err(i) => caps.insert(i, cap),
                }
            }
        }
        for r in self.rows.iter_mut() {
            let u = r.unit as usize;
            if !take[u] || self.unit_state[u].win.is_none() {
                continue;
            }
            let v = y.get(&(r.unit, r.good)).copied().unwrap_or(0) as f64;
            add_sample(&mut r.acc, &r.cum, times[u], v);
        }
    }

    /// The counted stores' part of the samples of the units `take` names (§4.1), into `y`: what a
    /// unit's own movers moved in less what they moved out at its counted stores, and those stores'
    /// summed residue (stock less ports less every counted move there) less its value at the unit's
    /// window's first sample. That first sample keeps the residue as the window's base, and the
    /// pairs' `since` (`cover`).
    fn sample_counted(
        &mut self,
        view: &View<'_>,
        take: &[bool],
        cover: &[Option<Vec<PairSince>>],
        y: &mut BTreeMap<(u16, Good), i64>,
    ) {
        let units = &self.units;
        let wanted = |u: usize| {
            take.get(u).copied().unwrap_or(false)
                && units.counted_of.get(u).is_some_and(|c| !c.is_empty())
        };
        if !(0..units.len()).any(wanted) {
            return;
        }
        let Some(t) = self.counts.as_ref().filter(|t| t.sweep == view.sweep) else {
            return;
        };
        let mut r: BTreeMap<(u32, Good), i64> = BTreeMap::new();
        for (&(m, h, g), &(moved_in, moved_out)) in &t.moved {
            let net = moved_in.saturating_sub(moved_out);
            let e = r.entry((h, g)).or_insert(0);
            *e = e.saturating_sub(net);
            let u = units.of_node[m as usize];
            if u != super::sections::NONE && wanted(u as usize) {
                let e = y.entry((u, g)).or_insert(0);
                *e = e.saturating_add(net);
            }
        }
        for &h in &units.counted {
            let Some(inv) = self.graph.node(h).inventory else {
                continue;
            };
            if let Some(i) = view.inventories.get(&inv) {
                for (g, a) in i.known.iter() {
                    let e = r.entry((h, g)).or_insert(0);
                    *e = e.saturating_add(a);
                }
            }
            if let Some(p) = self.ports_at.get(&inv) {
                for (g, a) in p.iter() {
                    let e = r.entry((h, g)).or_insert(0);
                    *e = e.saturating_sub(a);
                }
            }
        }
        for u in 0..units.len() {
            if !wanted(u) {
                continue;
            }
            let mut sum: BTreeMap<Good, i64> = BTreeMap::new();
            for &h in &units.counted_of[u] {
                for (&(_, g), &v) in r.range((h, Good(0))..=(h, Good(u32::MAX))) {
                    let e = sum.entry(g).or_insert(0);
                    *e = e.saturating_add(v);
                }
            }
            let st = &mut self.unit_state[u];
            if st.win.is_some_and(|w| w.n == 0) {
                st.rbase = sum.into_iter().collect();
                st.count_since = cover.get(u).cloned().flatten().unwrap_or_default();
                continue;
            }
            for &(g, _) in &st.rbase {
                sum.entry(g).or_insert(0);
            }
            for (g, v) in sum {
                let base = st
                    .rbase
                    .binary_search_by_key(&g, |&(x, _)| x)
                    .map_or(0, |i| st.rbase[i].1);
                let e = v.saturating_sub(base);
                // Inserted only when not zero: a row that starts later than the window had a
                // residue of zero before, as its absent samples say.
                if e != 0 {
                    let x = y.entry((u as u16, g)).or_insert(0);
                    *x = x.saturating_add(e);
                }
            }
        }
    }

    /// Judges the window.
    fn judge(&self, win: &Window, z: f64) -> WindowResult {
        let n = f64::from(win.n);
        let w = win.last.0.saturating_sub(win.start.0) as f64;
        let (goods, any_fail) = judge_goods(
            win.n,
            win.st,
            win.stt,
            w,
            win.goods.iter().map(|(&g, (acc, cum))| (g, acc, cum)),
            z,
        );
        let stores = win
            .stores
            .iter()
            .map(|(&(inv, g), (_, a))| {
                (
                    inv,
                    g,
                    slope(n, win.st, win.stt, a[0], a[1]),
                    slope(n, win.st, win.stt, a[2], a[3]),
                )
            })
            .collect();
        WindowResult {
            stores,
            start: win.start,
            ticks: w as u64,
            verdict: if any_fail {
                report::verdict::FAIL
            } else {
                report::verdict::PASS
            },
            goods,
        }
    }

    /// Judges unit `u`'s open window on its rows, by the district's rules (§3.4).
    fn judge_unit(&self, u: u16, z: f64) -> Option<WindowResult> {
        let w = self.unit_state.get(u as usize)?.win?;
        let ticks = w.last.0.saturating_sub(w.start.0);
        let rows = &self.rows[rows_of(&self.rows, u)];
        let (goods, any_fail) = judge_goods(
            w.n,
            w.st,
            w.stt,
            ticks as f64,
            rows.iter().map(|r| (r.good, &r.acc, &r.cum)),
            z,
        );
        Some(WindowResult {
            start: w.start,
            ticks,
            verdict: if any_fail {
                report::verdict::FAIL
            } else {
                report::verdict::PASS
            },
            goods,
            stores: Vec::new(),
        })
    }

    /// Concludes each unit window that reached the district's target length with three samples:
    /// judged, its verdicts kept, and a new one opened at `at`. Returns which units opened one.
    fn conclude_units(&mut self, at: Tick, z: f64) -> Vec<bool> {
        let nu = self.unit_state.len();
        let mut opened = vec![false; nu];
        let due: Vec<usize> = (0..nu)
            .filter(|&u| {
                self.unit_state[u].win.is_some_and(|w| {
                    at.0.saturating_sub(w.start.0) >= self.window_target && w.n >= 3
                })
            })
            .collect();
        for u in due {
            let res = self.judge_unit(u as u16, z).expect("open");
            let span = rows_of(&self.rows, u as u16);
            let st = &mut self.unit_state[u];
            keep_touched(&mut st.touched, &self.rows[span.clone()]);
            self.rows.drain(span);
            st.windows_completed += 1;
            if res.verdict == report::verdict::PASS {
                st.passed = Some((res.start, res.ticks));
                st.passed_caps = std::mem::take(&mut st.caps);
                st.edited = false;
                st.strikes = 0;
            } else {
                st.strikes += 1;
                st.caps.clear();
            }
            st.last = Some(res);
            st.win = Some(UnitWin {
                start: at,
                ..UnitWin::default()
            });
            opened[u] = true;
        }
        opened
    }

    /// The node limiting a good's flow on the read stores (§5.3): from the stores' edges carrying it
    /// (in-edges for a produced good, out-edges for a consumed one), the nearest node with flow whose
    /// own capacity binds; else the nearest machine; else the nearest node.
    pub fn limiting(&self, good: Good, produced: bool) -> Option<u32> {
        self.limiting_in(good, produced, None)
    }

    /// [`Compiled::limiting`] from the store edges of unit `unit` alone, or of every store (None).
    /// The search stops at stores, and a store's sections are its unit's, so it stays in the unit.
    fn limiting_in(&self, good: Good, produced: bool, unit: Option<u16>) -> Option<u32> {
        let g = &self.graph;
        let mut queue: Vec<u32> = Vec::new();
        let mut seen = vec![false; g.nodes.len()];
        for se in &self.sol.store_edges {
            let (e, into) = (se.edge, se.into);
            if se.good != good || into != produced || se.flow <= 0.0 {
                continue;
            }
            let ed = g.edge(e);
            let store = if into { ed.to } else { ed.from };
            if unit.is_some_and(|u| self.units.of_node[store as usize] != u) {
                continue;
            }
            let v = if into { ed.from } else { ed.to };
            if !seen[v as usize] {
                seen[v as usize] = true;
                queue.push(v);
            }
        }
        let mut i = 0;
        let mut machine = None;
        while i < queue.len() {
            let v = queue[i];
            i += 1;
            let node = g.node(v);
            if node.kind == NodeKind::Store {
                continue;
            }
            if self.sol.saturated.get(v as usize).copied().unwrap_or(false) {
                return Some(v);
            }
            if machine.is_none() && node.kind == NodeKind::Machine {
                machine = Some(v);
            }
            let row = if produced {
                g.in_g.of(v)
            } else {
                g.out_g.of(v)
            };
            for &e in row {
                if self.sol.edge_flow.get(e as usize).copied().unwrap_or(0.0) <= 0.0 {
                    continue;
                }
                let ed = g.edge(e);
                let w = if produced { ed.from } else { ed.to };
                if !seen[w as usize] {
                    seen[w as usize] = true;
                    queue.push(w);
                }
            }
        }
        machine.or(queue
            .first()
            .copied()
            .filter(|&v| g.node(v).kind != NodeKind::Store))
    }

    /// The buffers (and machines, whose basins and depots are part of them) upstream of a node,
    /// nearest first, up to `max`: what IND_PROBE_SET asks Java to sample.
    fn probe_nodes(&self, v: u32, max: usize) -> Vec<u32> {
        let g = &self.graph;
        let mut out = Vec::new();
        let mut seen = vec![false; g.nodes.len()];
        let mut queue = vec![v];
        seen[v as usize] = true;
        let mut i = 0;
        while i < queue.len() && out.len() < max {
            let u = queue[i];
            i += 1;
            let k = g.node(u).kind;
            if matches!(k, NodeKind::Buffer | NodeKind::Machine) {
                out.push(u);
            }
            if k == NodeKind::Store {
                continue;
            }
            for &e in g.in_g.of(u) {
                let w = g.edge(e).from;
                if !seen[w as usize] {
                    seen[w as usize] = true;
                    queue.push(w);
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// On a failed window: derate the limiting node of the most optimistic good, probe upstream of
    /// it, and after `glass.mismatchStrikes` failed windows demote its kind (G-B14). With the units
    /// active, the search starts in the unit whose own last window failed worst on that good, and
    /// the derate is that window's (choice SC61); with none, over the district. Returns the probe
    /// set to send and a newly demoted model, if any.
    fn act(
        &mut self,
        at: Tick,
        res: &WindowResult,
        tun: &GlassTunables,
    ) -> (Option<Vec<u32>>, Option<u32>) {
        if res.verdict != report::verdict::FAIL {
            self.strikes = 0;
            return (None, None);
        }
        self.strikes += 1;
        let Some((eta, good, produced)) = most_optimistic(&res.goods, None) else {
            return (None, None);
        };
        // A unit's window that started before the last derate was judged against another model:
        // it names no node (choice SC61).
        let since = self.derated_at.unwrap_or(Tick::ZERO);
        let local = self
            .unit_state
            .iter()
            .enumerate()
            .filter_map(|(u, st)| {
                let r = st.last.as_ref().filter(|r| r.start >= since)?;
                let (e, _, p) = most_optimistic(&r.goods, Some(good))?;
                Some((e, u as u16, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .and_then(|(e, u, p)| Some((self.limiting_in(good, p, Some(u))?, e)));
        let Some((v, eta)) = local.or_else(|| Some((self.limiting(good, produced)?, eta))) else {
            return (None, None);
        };
        // η is a measurement: a relative 1e-9 up keeps a float slope of 0.4999999999 at 1/2.
        let eta_q = Q::floor_f64(eta * (1.0 + 1e-9)).min(Q::ONE);
        let d = self
            .regime
            .derate
            .get(&v)
            .copied()
            .unwrap_or(Q::ONE)
            .mul(eta_q);
        self.regime.derate.insert(v, d);
        self.derated_at = Some(at);
        if let Some(&u) = self.units.of_node.get(v as usize)
            && let Some(st) = self.unit_state.get_mut(u as usize)
        {
            st.derated_at = Some(at);
        }
        self.event(GlassEvent::Derated {
            node: v,
            eta: eta_q,
            good,
        });
        let probes = self.probe_nodes(v, tun.probe_nodes_max);
        let mut demote = None;
        if self.strikes >= tun.mismatch_strikes {
            let m = self.graph.node(v).model;
            if !matches!(
                m,
                model::STORE_ITEMS | model::STORE_FLUID | model::STORE_ENERGY
            ) {
                demote = Some(m);
                self.regime.demoted.insert(m);
                self.event(GlassEvent::Demoted { model: m, node: v });
            }
            self.strikes = 0;
        }
        let probes = (!probes.is_empty() && probes != self.probe_set).then_some(probes);
        if let Some(p) = &probes {
            self.probe_set = p.clone();
            self.probes.clear();
            self.event(GlassEvent::Probed { nodes: p.clone() });
        }
        (probes, demote)
    }

    /// One live sweep. Returns a probe set to send and a newly demoted model, if any.
    pub fn sweep(
        &mut self,
        at: Tick,
        view: &View<'_>,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> (Option<Vec<u32>>, Option<u32>) {
        let live = vec![true; self.unit_state.len()];
        self.sweep_with(at, view, goods, tun, true, &live)
    }

    /// One sweep: `district_live` when every chunk ran throughout it (the district's window then
    /// samples), and per unit whether its chunks did (`unit_live`, SECTIONS.md §3.4). A unit whose
    /// chunks did not run loses its open window; the others sample as the district does. Returns a
    /// probe set to send and a newly demoted model, if any.
    pub fn sweep_with(
        &mut self,
        at: Tick,
        view: &View<'_>,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
        district_live: bool,
        unit_live: &[bool],
    ) -> (Option<Vec<u32>>, Option<u32>) {
        let mut out = (None, None);
        if district_live {
            self.sweeps += 1;
        }
        self.district_revision = Some(view.revision);
        self.advance(at, tun);
        // The units first: each open one that ran is sampled, the others lose their windows.
        let nu = self.unit_state.len();
        let ran = |u: usize| unit_live.get(u).copied().unwrap_or(false);
        // Warming up or not: a unit that did not run warms up afresh once it runs again, as one
        // does whose counted stores this sweep's count sample does not cover (§4.2). A pair whose
        // counters started again starts its unit's window again.
        let cover = self.count_cover(view.sweep);
        let covered = |u: usize| cover.get(u).is_some_and(|c| c.is_some());
        for u in 0..nu {
            match cover.get(u) {
                Some(Some(since)) if ran(u) => {
                    let st = &self.unit_state[u];
                    if st.win.is_some() && *since != st.count_since {
                        self.restart_unit(u as u16);
                    }
                }
                _ => self.discard_unit(u as u16, at, tun),
            }
        }
        let take: Vec<bool> = (0..nu)
            .map(|u| ran(u) && self.unit_state[u].win.is_some())
            .collect();
        self.sample_units(at, view, &take, &cover);
        let mut first = self.conclude_units(at, tun.checksum_z);
        let stock = self.stock(view);
        let per = self.store_levels(view);
        let unbounded = self.sol.has_unbounded;
        let mut restarted = false;
        if district_live && self.win.is_some() {
            self.sample(at, &stock, &per, view.ports, unbounded);
            self.sweeps_sampled += 1;
            let done = self
                .win
                .as_ref()
                .is_some_and(|w| at.0.saturating_sub(w.start.0) >= self.window_target && w.n >= 3);
            if done {
                let win = self.win.take().expect("open");
                let res = self.judge(&win, tun.checksum_z);
                self.windows_completed += 1;
                out = self.act(at, &res, tun);
                if res.verdict == report::verdict::PASS {
                    self.passed = Some(res.clone());
                    self.edited = false;
                }
                self.last = Some(res);
                self.veto.clear();
                // The next window starts here, with this sample.
                self.win = Some(Window {
                    start: at,
                    ..Window::default()
                });
                restarted = true;
            }
        }
        self.read_state(view, goods, tun);
        self.resolve(tun, false);
        if restarted {
            self.sample(at, &stock, &per, view.ports, unbounded);
        } else if district_live && self.win.is_none() && at >= self.ready_at {
            self.win = Some(Window {
                start: at,
                ..Window::default()
            });
            self.sample(at, &stock, &per, view.ports, self.sol.has_unbounded);
            self.sweeps_sampled += 1;
        }
        // Units that concluded start their next window here, with this sample; units past their
        // warm-up whose chunks ran open one.
        for (u, f) in first.iter_mut().enumerate() {
            let st = &mut self.unit_state[u];
            if st.win.is_none() && ran(u) && covered(u) && at >= st.ready_at {
                st.win = Some(UnitWin {
                    start: at,
                    ..UnitWin::default()
                });
                st.caps.clear();
                *f = true;
            }
        }
        self.sample_units(at, view, &first, &cover);
        self.set_stock(view);
        self.last_sweep = Some(at);
        out
    }

    /// The read stores' stock per (inventory, good) at this sweep, for the drain clamp.
    fn set_stock(&mut self, view: &View<'_>) {
        self.stock_est.clear();
        for v in 0..self.graph.nodes.len() as u32 {
            let Some(inv) = self.graph.node(v).inventory else {
                continue;
            };
            let Some(c) = self.contents(v, view) else {
                continue;
            };
            for (g, a) in c.iter() {
                self.stock_est.insert((inv, g), a.max(0) as f64);
            }
        }
    }

    /// IND_PROBE_SAMPLE for a node of the current probe set.
    pub fn probe_sample(&mut self, sweep: u32, node: u32, goods: &GoodVec) {
        if !self.probe_set.contains(&node) {
            return;
        }
        let total: i64 = goods.iter().map(|(_, a)| a).sum();
        self.probes
            .entry(node)
            .and_modify(|p| {
                p.last_sweep = sweep;
                p.last = total;
            })
            .or_insert(ProbeTrack {
                first_sweep: sweep,
                first: total,
                last_sweep: sweep,
                last: total,
            });
    }

    /// The district's report (`report.rs`): with a window (the last completed one, else the one in
    /// progress), its goods as that window integrated and judged them; before any, the current
    /// solve's.
    pub fn report(&self, dg: &DistrictGlass) -> Report {
        let sol = &self.sol;
        let partial;
        let res: Option<&WindowResult> = match (&self.last, &self.win) {
            (Some(r), _) => Some(r),
            (None, Some(w)) if w.n > 0 => {
                partial = self.judge_partial(w);
                Some(&partial)
            }
            _ => None,
        };
        let mut goods: BTreeMap<Good, GoodReport> = BTreeMap::new();
        for gb in &sol.goods {
            let (il, iu) = (gb.in_l.to_f64(), gb.in_u.to_f64());
            let (ol, ou) = (gb.out_l.to_f64(), gb.out_u.to_f64());
            goods.insert(
                gb.good,
                GoodReport {
                    good: gb.good.0,
                    class: gb.class,
                    in_l: il,
                    in_u: iu,
                    out_l: ol,
                    out_u: ou,
                    predicted: il - ol,
                    measured: f64::NAN,
                    sigma: 0.0,
                    q: 0.0,
                    verdict: report::verdict::NONE,
                    samples: 0,
                    lo: il - ou,
                    hi: iu - ol,
                    why: gb.why.0,
                    why_at: gb.why.1,
                },
            );
        }
        if let Some(r) = res {
            for gr in &r.goods {
                let e = goods.entry(gr.good).or_insert_with(|| GoodReport {
                    good: gr.good.0,
                    class: gr.class,
                    in_l: 0.0,
                    in_u: 0.0,
                    out_l: 0.0,
                    out_u: 0.0,
                    predicted: 0.0,
                    measured: f64::NAN,
                    sigma: 0.0,
                    q: 0.0,
                    verdict: report::verdict::NONE,
                    samples: 0,
                    lo: 0.0,
                    hi: 0.0,
                    why: report::why::NONE,
                    why_at: u32::MAX,
                });
                // A window with length: every number is the window's, so they agree.
                if r.ticks > 0 {
                    e.class = gr.class;
                    (e.why, e.why_at) = gr.why;
                    e.in_l = gr.in_l;
                    e.in_u = gr.in_u;
                    e.out_l = gr.out_l;
                    e.out_u = gr.out_u;
                    if gr.predicted.is_finite() {
                        e.predicted = gr.predicted;
                    }
                    if !gr.lo.is_nan() {
                        e.lo = gr.lo;
                    }
                    if !gr.hi.is_nan() {
                        e.hi = gr.hi;
                    }
                }
                e.measured = gr.measured;
                e.sigma = gr.sigma;
                e.q = gr.q;
                e.verdict = gr.verdict;
                e.samples = gr.samples;
            }
        }
        let (window_start, done) = match (&self.last, &self.win) {
            (Some(r), _) => (r.start.0, r.ticks),
            (None, Some(w)) => (w.start.0, w.last.0.saturating_sub(w.start.0)),
            _ => (0, 0),
        };
        Report {
            compile: self.graph.end.compile,
            rounds: sol.rounds,
            converged: sol.converged,
            nodes: self.graph.nodes.len() as u32,
            edges: self.graph.edges.len() as u32,
            solve_micros: self.solve_micros,
            window_start,
            window_ticks_done: done,
            window_ticks_target: self.window_target,
            windows_completed: self.windows_completed,
            verdict: self
                .last
                .as_ref()
                .map_or(report::verdict::NONE, |r| r.verdict),
            strikes: self.strikes,
            solves: self.solves,
            demoted: dg.demoted.len() as u32,
            latency: self.graph.longest_latency,
            accepted_at: self.accepted_at.0,
            ready_at: self.ready_at.0,
            sweeps: self.sweeps,
            sweeps_sampled: self.sweeps_sampled,
            heaters_burning: self.heaters_burning(),
            next_breakpoint: self.next_breakpoint(),
            revision: self.graph.end.revision,
            district_revision: self.district_revision,
            last_discard: dg.last_discard.map(|(t, r)| (t.0, r)),
            discards: dg.discards,
            replay: report::ReplayReport {
                model: dg.replay_model,
                since: dg.replay_since.map(|t| t.0),
                regimes: self.replay_regimes,
                breakpoints: self.replay_breakpoints,
                oscillations: self.replay_oscillations,
                cut_edges: dg.replay.as_ref().map_or(0, |r| r.cut_edges()),
                eligible: self.replayable(),
                learned_sources: dg.replay.as_ref().map_or(0, |r| r.learned_sources()),
            },
            goods: goods.into_values().collect(),
            units: self.unit_reports(dg),
        }
    }

    /// Each judging unit as the report shows it (choice SC74); empty with one unit or none.
    fn unit_reports(&self, dg: &DistrictGlass) -> Vec<report::UnitReport> {
        let frozen = dg.replay.as_ref().map(|r| r.frozen());
        (0..self.unit_state.len())
            .map(|u| {
                let st = &self.unit_state[u];
                let secs = self.units.sections.get(u).map_or(&[][..], |v| &v[..]);
                let mut bbox: Option<(BlockPos, BlockPos)> = None;
                for &s in secs {
                    let Some((lo, hi)) = self.sections.list.get(s as usize).and_then(|x| x.bbox)
                    else {
                        continue;
                    };
                    bbox = Some(match bbox {
                        None => (lo, hi),
                        Some((a, b)) => (
                            BlockPos {
                                x: a.x.min(lo.x),
                                y: a.y.min(lo.y),
                                z: a.z.min(lo.z),
                            },
                            BlockPos {
                                x: b.x.max(hi.x),
                                y: b.y.max(hi.y),
                                z: b.z.max(hi.z),
                            },
                        ),
                    });
                }
                report::UnitReport {
                    sections: secs.len() as u32,
                    nodes: self
                        .units
                        .of_node
                        .iter()
                        .filter(|&&x| x as usize == u)
                        .count() as u32,
                    bbox,
                    window_ticks: st
                        .win
                        .as_ref()
                        .map_or(0, |w| w.last.0.saturating_sub(w.start.0)),
                    windows_completed: st.windows_completed,
                    verdict: st
                        .last
                        .as_ref()
                        .map_or(report::verdict::NONE, |r| r.verdict),
                    strikes: st.strikes,
                    passed: st.passed.is_some(),
                    stale: st.edited,
                    may_replay: self.unit_may_replay(u as u16),
                    frozen: frozen.is_some_and(|f| f.get(u).copied().unwrap_or(false)),
                    unbounded: self.units.unbounded.get(u).copied().unwrap_or(false),
                    ready_at: st.ready_at.0,
                }
            })
            .collect()
    }

    /// The window in progress, judged as it stands (its verdicts are reported as NONE).
    fn judge_partial(&self, w: &Window) -> WindowResult {
        let mut r = self.judge(w, 0.0);
        for g in r.goods.iter_mut() {
            g.verdict = report::verdict::NONE;
        }
        r.verdict = report::verdict::NONE;
        r
    }

    /// The window in progress, if any: (start, samples).
    pub fn window(&self) -> Option<(Tick, u32)> {
        self.win.as_ref().map(|w| (w.start, w.n))
    }

    /// Unit `u`'s window in progress, if any: (start, samples).
    pub fn unit_window(&self, u: u16) -> Option<(Tick, u32)> {
        self.unit_state
            .get(u as usize)?
            .win
            .as_ref()
            .map(|w| (w.start, w.n))
    }

    /// Unit `u`'s window in progress, judged as it stands (its verdicts reported as NONE).
    pub fn unit_partial(&self, u: u16) -> Option<WindowResult> {
        let mut r = self.judge_unit(u, 0.0)?;
        for g in r.goods.iter_mut() {
            g.verdict = report::verdict::NONE;
        }
        r.verdict = report::verdict::NONE;
        Some(r)
    }

    /// The units' open rows.
    pub fn rows_len(&self) -> usize {
        self.rows.len()
    }

    /// The units' extra rows (SECTIONS.md §7.2): their (unit, good) rows, open or in their last
    /// window, beyond one per good, which one section per district would hold.
    pub fn extra_rows(&self) -> usize {
        if self.unit_state.is_empty() {
            return 0;
        }
        let mut pairs: BTreeSet<(u16, Good)> = self.rows.iter().map(|r| (r.unit, r.good)).collect();
        for (u, st) in self.unit_state.iter().enumerate() {
            if let Some(l) = &st.last {
                pairs.extend(l.goods.iter().map(|g| (u as u16, g.good)));
            }
        }
        let goods: BTreeSet<Good> = pairs.iter().map(|&(_, g)| g).collect();
        pairs.len() - goods.len()
    }

    /// The units' goods (open or in their last window), per unit.
    fn unit_goods(&self) -> Vec<BTreeSet<Good>> {
        let mut out = vec![BTreeSet::new(); self.unit_state.len()];
        for r in &self.rows {
            out[r.unit as usize].insert(r.good);
        }
        for (u, st) in self.unit_state.iter().enumerate() {
            if let Some(l) = &st.last {
                out[u].extend(l.goods.iter().map(|g| g.good));
            }
        }
        out
    }

    /// The largest section count at most `below - 1` whose cut would hold at most `limit` extra
    /// rows, counting each unit of it as the goods of the units it merges (a lower count only
    /// merges more: `Sections::compute`'s merges are a prefix of a lower count's). 1 always does.
    /// Counted are the stores the full cut counts (`tun`'s `glass.sectionsMax` and
    /// `glass.countedStoresMax`, [`Sections::cut_counted`]).
    pub fn sections_for_rows(&self, limit: usize, below: usize, tun: &GlassTunables) -> usize {
        let goods = self.unit_goods();
        let base = Sections::compute_counted(&self.graph, tun.sections_max, tun.counted_stores_max)
            .counted;
        let extra_at = |max: usize| -> usize {
            let s = Sections::cut_with(&self.graph, max, &base);
            let u = Units::build(&self.graph, &s, &self.joins);
            if !u.active() {
                return 0;
            }
            let mut merged = vec![BTreeSet::new(); u.len()];
            for (old, gs) in goods.iter().enumerate() {
                // An old unit lies in one new unit: its first section's first node names it.
                let node = self.units.sections[old]
                    .first()
                    .and_then(|&x| self.sections.list[x as usize].nodes.first());
                let Some(&v) = node else { continue };
                let nu = u.of_node[v as usize];
                if nu != super::sections::NONE {
                    merged[nu as usize].extend(gs.iter().copied());
                }
            }
            let all: BTreeSet<Good> = merged.iter().flatten().copied().collect();
            merged.iter().map(|m| m.len()).sum::<usize>() - all.len()
        };
        let (mut lo, mut hi) = (1usize, below.saturating_sub(1).max(1));
        if extra_at(hi) <= limit {
            return hi;
        }
        // extra_at(lo) is 0; find the largest count that holds.
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if extra_at(mid) <= limit {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// On restore (`page::read_district_page`, its sections recomputed): the judging units and
    /// their bounds from the sections and the solve, and a check that the saved unit state is
    /// theirs. False when it is not (an invalid page).
    pub(crate) fn restore_units(&mut self) -> bool {
        self.units = Units::build(&self.graph, &self.sections, &self.joins);
        self.ubounds = units::bounds(&self.graph, &self.units, &self.sol);
        let nu = if self.units.active() {
            self.units.len()
        } else {
            0
        };
        if self.unit_state.len() != nu {
            return false;
        }
        let sorted = self
            .rows
            .windows(2)
            .all(|p| (p[0].unit, p[0].good) < (p[1].unit, p[1].good));
        let nd = self.graph.domains.len() as u32;
        let caps_fit = |c: &[DomCap]| {
            c.iter().all(|x| x.domain < nd) && c.windows(2).all(|p| cap_order(&p[0], &p[1]).is_lt())
        };
        sorted
            && self.rows.iter().all(|r| {
                self.unit_state
                    .get(r.unit as usize)
                    .is_some_and(|s| s.win.is_some())
            })
            && self.unit_state.iter().all(|s| {
                caps_fit(&s.caps)
                    && caps_fit(&s.passed_caps)
                    && (s.win.is_some() || s.caps.is_empty())
            })
    }

    /// The goods the district's last concluded window failed on: this compile's, else those the
    /// compile before carried over (`veto`).
    pub fn district_failing(&self) -> Vec<Good> {
        match &self.last {
            Some(r) => r
                .goods
                .iter()
                .filter(|g| g.verdict == report::verdict::FAIL)
                .map(|g| g.good)
                .collect(),
            None => self.veto.clone(),
        }
    }

    /// May unit `u` replay (SECTIONS.md §3.4)? Its own last window passed on at least one good
    /// (a window with nothing judged proves nothing, choice SC66) and no edit staled it since, and
    /// the district's last concluded window failed on none of the goods the unit touches: those of
    /// its last window, its open rows and its bounds under the current solve. With the units not
    /// active, the district's rule (G-B4, [`Compiled::eligible`]).
    pub fn unit_may_replay(&self, u: u16) -> bool {
        if self.unit_state.is_empty() {
            return self.eligible();
        }
        let Some(st) = self.unit_state.get(u as usize) else {
            return false;
        };
        let Some(own) = st
            .last
            .as_ref()
            .filter(|r| r.verdict == report::verdict::PASS)
        else {
            return false;
        };
        if st.edited || st.derated_at.is_some_and(|d| own.start < d) {
            return false;
        }
        // A window of unbounded goods alone (a black box, a port, a model demoted here), or of none
        // (a unit with no store, as a boiler its pumps fill from a source, which only drives a
        // network; or one whose goods none of its stores measured in the window, as an idle line),
        // judged nothing: such a unit leans on the district's pass, as a district alone does (G-B4,
        // choice SC66). Gated, a unit with no store would stop every unit on the network it drives.
        let judged = own.goods.iter().any(|g| g.verdict == report::verdict::PASS);
        let leans = own
            .goods
            .iter()
            .all(|g| g.class == report::class::UNBOUNDED)
            && self.eligible();
        if !judged && !leans {
            return false;
        }
        let failing = self.district_failing();
        if failing.is_empty() {
            return true;
        }
        let touched = own
            .goods
            .iter()
            .map(|g| g.good)
            .chain(st.touched.iter().copied())
            .chain(self.rows[rows_of(&self.rows, u)].iter().map(|r| r.good))
            .chain(self.ubounds.iter().filter(|b| b.unit == u).map(|b| b.good));
        !touched.into_iter().any(|g| failing.contains(&g))
    }

    /// May section `s` replay: its unit's rule ([`Compiled::unit_may_replay`]); an idle node's
    /// section (none) never does.
    pub fn section_may_replay(&self, s: u16) -> bool {
        match self.units.of_section.get(s as usize) {
            Some(&u) if u != super::sections::NONE => self.unit_may_replay(u),
            _ => false,
        }
    }
}

impl DistrictGlass {
    /// Counts a discard or skip.
    pub fn note(&mut self, at: Tick, reason: u32) {
        if let Some(c) = self.discards.get_mut(reason as usize) {
            *c += 1;
        }
        self.last_discard = Some((at, reason));
    }

    /// The probe set command for a district, if one is due.
    pub fn probe_cmd(&self, district: DistrictId, nodes: Vec<u32>) -> Option<ProbeSet> {
        let c = self.compiled.as_ref()?;
        Some(ProbeSet {
            district,
            compile: c.graph.end.compile,
            nodes,
        })
    }
}

// ---- the glass page (page.rs, choice G2-80) -------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct};

    persist_struct!(Acc {
        sy,
        sty,
        sp,
        stp,
        slo,
        stlo,
        shi,
        sthi,
        sd,
        std,
        sdd
    });
    persist_struct!(Cum {
        p,
        lo,
        hi,
        lo_inf,
        hi_inf,
        il,
        ol,
        iu,
        ou,
        iu_inf,
        ou_inf,
        var,
        batch,
        class,
        why
    });
    persist_struct!(Window {
        start,
        last,
        n,
        st,
        stt,
        goods,
        stores
    });
    persist_struct!(GoodResult {
        good,
        class,
        why,
        in_l,
        in_u,
        out_l,
        out_u,
        predicted,
        lo,
        hi,
        measured,
        sigma,
        q,
        verdict,
        samples
    });
    persist_struct!(WindowResult {
        start,
        ticks,
        verdict,
        goods,
        stores
    });
    persist_struct!(ProbeTrack {
        first_sweep,
        first,
        last_sweep,
        last
    });
    persist_struct!(Burn { node, left, thr });
    persist_struct!(Row {
        unit,
        good,
        acc,
        cum
    });
    persist_struct!(UnitWin {
        start,
        last,
        n,
        st,
        stt
    });
    persist_struct!(DomCap {
        domain,
        source,
        speed,
        capacity
    });
    persist_struct!(UnitState {
        win,
        ready_at,
        last,
        passed,
        edited,
        strikes,
        windows_completed,
        caps,
        passed_caps,
        derated_at,
        touched,
        count_since,
        rbase
    });

    impl Persist for GlassEvent {
        fn put(&self, w: &mut Writer) {
            match self {
                GlassEvent::Derated { node, eta, good } => {
                    w.u8(0);
                    node.put(w);
                    eta.put(w);
                    good.put(w);
                }
                GlassEvent::Probed { nodes } => {
                    w.u8(1);
                    nodes.put(w);
                }
                GlassEvent::Demoted { model, node } => {
                    w.u8(2);
                    model.put(w);
                    node.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => GlassEvent::Derated {
                    node: Persist::get(r)?,
                    eta: Persist::get(r)?,
                    good: Persist::get(r)?,
                },
                1 => GlassEvent::Probed {
                    nodes: Persist::get(r)?,
                },
                2 => GlassEvent::Demoted {
                    model: Persist::get(r)?,
                    node: Persist::get(r)?,
                },
                _ => return invalid("glass event"),
            })
        }
    }

    /// Every field but `solve_micros` (wall-clock time, restored as 0), `sections` and `units`
    /// (recomputed by the page's reader, which checks them against the fingerprints the page keeps
    /// and the unit state against the units), `ubounds` (recomputed from the solve) and `counts` (a
    /// sweep's sample is read at its end): the regime cache too, as a regime it answers is not a
    /// solve, and the replay's settle budget counts solves (G2-61).
    impl Persist for Compiled {
        fn put(&self, w: &mut Writer) {
            self.graph.put(w);
            self.accepted_at.put(w);
            self.window_target.put(w);
            self.ready_at.put(w);
            self.ctrl_on.put(w);
            self.regime.put(w);
            self.key.put(w);
            self.sol.put(w);
            self.cache.put(w);
            self.cache_parts.put(w);
            self.solves.put(w);
            self.cache_hits.put(w);
            self.win.put(w);
            self.last.put(w);
            self.passed.put(w);
            self.edited.put(w);
            self.veto.put(w);
            self.derated_at.put(w);
            self.windows_completed.put(w);
            self.strikes.put(w);
            self.stock_est.put(w);
            self.last_sweep.put(w);
            self.heaters.put(w);
            self.model_at.put(w);
            self.sweeps.put(w);
            self.sweeps_sampled.put(w);
            self.district_revision.put(w);
            self.probe_set.put(w);
            self.probes.put(w);
            self.events.put(w);
            self.replay_regimes.put(w);
            self.replay_breakpoints.put(w);
            self.replay_oscillations.put(w);
            self.unit_state.put(w);
            self.rows.put(w);
            self.ports_at.put(w);
            self.joins.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            let c = Compiled {
                graph: Persist::get(r)?,
                accepted_at: Persist::get(r)?,
                window_target: Persist::get(r)?,
                ready_at: Persist::get(r)?,
                ctrl_on: Persist::get(r)?,
                regime: Persist::get(r)?,
                key: Persist::get(r)?,
                sol: Persist::get(r)?,
                cache: Persist::get(r)?,
                cache_parts: Persist::get(r)?,
                solves: Persist::get(r)?,
                cache_hits: Persist::get(r)?,
                solve_micros: 0.0,
                win: Persist::get(r)?,
                last: Persist::get(r)?,
                passed: Persist::get(r)?,
                edited: Persist::get(r)?,
                veto: Persist::get(r)?,
                derated_at: Persist::get(r)?,
                windows_completed: Persist::get(r)?,
                strikes: Persist::get(r)?,
                stock_est: Persist::get(r)?,
                last_sweep: Persist::get(r)?,
                heaters: Persist::get(r)?,
                model_at: Persist::get(r)?,
                sweeps: Persist::get(r)?,
                sweeps_sampled: Persist::get(r)?,
                district_revision: Persist::get(r)?,
                probe_set: Persist::get(r)?,
                probes: Persist::get(r)?,
                events: Persist::get(r)?,
                replay_regimes: Persist::get(r)?,
                replay_breakpoints: Persist::get(r)?,
                replay_oscillations: Persist::get(r)?,
                unit_state: Persist::get(r)?,
                rows: Persist::get(r)?,
                ports_at: Persist::get(r)?,
                joins: Persist::get(r)?,
                sections: Sections::default(),
                units: Units::default(),
                ubounds: Vec::new(),
                counts: None,
            };
            // What the solve and the window index by node must cover the graph's nodes.
            let n = c.graph.nodes.len();
            let rg = &c.regime;
            let per_node = [
                c.ctrl_on.len(),
                rg.held.len(),
                rg.room.len(),
                rg.read.len(),
                rg.gate_off.len(),
                rg.burning.len(),
            ];
            if per_node.iter().any(|&l| l != n) || c.heaters.iter().any(|b| b.node as usize >= n) {
                return invalid("glass compile state of another graph");
            }
            Ok(c)
        }
    }
}
