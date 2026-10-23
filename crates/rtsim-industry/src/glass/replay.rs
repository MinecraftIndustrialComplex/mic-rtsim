//! The glass replay (GLASSBOX.md §4, phase G2): while a compiled, eligible district is away, it
//! replays on its graph one regime at a time.
//!
//! **Regimes.** A regime is the graph's flags (§4.1): which stores hold which goods, which have
//! room, which controllers are on, which heaters burn, which cut goods ran dry or filled their live
//! receivers' room (choice G2-66). Its solve gives
//! every goods edge's L flow; the replay books those per store (`PoolKey::Store`) and across a
//! partial load's cut (`PoolKey::Stock`, I7, G-B6) as one all-or-nothing Box process. Each flag a
//! level can change has a watch: a bound on one pool with weight ±1, which the cluster checks
//! exactly, so the process stops the tick before the flag changes; the replay re-solves (the
//! regime cache answers most of these) and replaces the process in place (choices G2-4, G2-10).
//!
//! **Time.** Heater phases and latency (§4.4) are timers, the process's deadline. A simulated
//! node whose potential rises (its flow in the regime with nothing held) is held off for its
//! downstream latency (the time its output takes to reach a store; a loop counts whole), then
//! runs: what it makes arrives no earlier than in Create, and every flow stays a solve's, so
//! goods are conserved (G2-6). A fall is at once.
//!
//! **Cycles.** A regime sequence that repeats exactly (same flags, same watched levels, zero
//! drift on every pool that decides a breakpoint) becomes one `Body::Cycle` over as many whole
//! periods as provably change nothing else (G-B9, G2-13): a controller loop costs one process per
//! stretch, not two breakpoints per period.
//!
//! **Parts.** A graph of pieces that share nothing but stores they only draw from (a source
//! chest feeding two independent loops) replays as parts, each its own process with its own
//! regimes and cycles, so two loops of unequal periods each find their cycle (choice G2-22). A
//! shared store (a hub) is in every part it feeds; the cluster keeps it safe between them (a
//! cycle's steepest drain is its envelope there). A cut or a residual keeps the district whole.
//!
//! **Units** (SECTIONS.md §3.5, SEC-1 slice S2c). With two judging units or more, only the units
//! that may replay run: every other unit's simulated machines are gated off, as controllers gate
//! nodes, so its stores are pools only replaying neighbours touch, and a gated boiler stops every
//! simulated node on its domain. Parts are solve units, not freeze units.
//!
//! **Fallback.** A regime with no fixed point, a process that stays blocked, or more than
//! `glass.breakpointsPerEventMax` breakpoints between outside events (§4.5, G2-7) freezes the units
//! it names: those whose stores and controllers fired most since the last outside event (a stop
//! or an oscillation), or whose flags flipped most in the regime that did not settle. Their nodes
//! are gated off and every part they gate is rebuilt at the fallback's tick, keeping what was
//! booked; a unit freezes once per replay. A fallback that names none, or names every unit
//! replaying in its part, ends that part's replay (its units freeze); only when no unit replays
//! does the glass replay end, and the engine falls back to the black box or freezes.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::conserve::{Diffuser, FIXED_ONE};

use super::check::{Burn, Compiled, gates, holding_threshold};
use super::q::Q;
use super::records::NodeKind;
use super::report;
use super::sections::NONE;
use super::solve::{Regime, Solution, burn};
use crate::cluster::{
    Cluster, CycleFire, CyclePiece, MemberSpec, PoolKey, ProcSpec, ProcSpecBody, Run, SpaceSpec,
    Stop, bound_index, bound_part, bound_tag, cycle_fired, cycle_period,
};
use crate::ids::{DistrictId, Good, GoodClass, GoodVec};
use crate::reconcile::GoodLookup;
use crate::tunables::GlassTunables;

/// Transitions handled in one call before the replay gives up on a breakpoint it cannot leave.
pub const MAX_STEPS: u32 = 32;
/// Marks kept for cycle detection.
const MARKS_KEPT: usize = 128;
/// Candidate periods tried per mark.
const CYCLE_TRIES: usize = 4;
/// Units of margin per pool when a cycle runs without its bounds (diffuser rounding).
const CYCLE_MARGIN: i64 = 2;
/// The longest stretch one closed-form cycle may cover.
const CYCLE_TICKS_MAX: u64 = 1 << 40;
/// A net rate below this (units per tick) is rounding, not flow: 1 unit per 16.7M ticks.
const NET_EPS: f64 = 1.0 / (1u64 << 24) as f64;
const U: i128 = FIXED_ONE as i128;

/// Why a glass replay stopped and the district fell back (§4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// The regime's flags found no fixed point.
    Unsettled,
    /// More breakpoints than `glass.breakpointsPerEventMax` since the last outside event.
    Oscillating,
    /// A breakpoint the replay could not leave: its process stays blocked.
    Stuck,
    /// A stop the replay does not model.
    Unexpected,
}

impl Fallback {
    pub fn words(self) -> &'static str {
        match self {
            Fallback::Unsettled => "the regime has no fixed point",
            Fallback::Oscillating => "oscillating: too many breakpoints between events",
            Fallback::Stuck => "a breakpoint the replay could not leave",
            Fallback::Unexpected => "a stop the glass replay does not model",
        }
    }
}

/// A store inventory's declared capacity, from IND_INVENTORY.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreDecl {
    pub item_slots: u32,
    pub fluid_capacity: i64,
}

/// What the engine knows of the district when a glass replay starts.
#[derive(Clone, Debug, Default)]
pub struct Setup {
    pub district: DistrictId,
    /// Per graph node: it runs in the world now (its chunk is loaded and block-ticking).
    pub live: Vec<bool>,
    /// The inventories the replay covers (not loaded, or loaded without block ticking).
    pub covered: BTreeMap<u32, StoreDecl>,
    /// Every inventory's effective contents, covered or not.
    pub contents: BTreeMap<u32, GoodVec>,
    /// What the district owes per good (virtual stock), for the cut's supply.
    pub owed: GoodVec,
    /// The declared capacity of every inventory the replay does not cover, for the room a cut
    /// from the simulated side may fill (choice G2-66).
    pub live_decls: BTreeMap<u32, StoreDecl>,
}

/// Counters for the report (since the replay started).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplayCounts {
    pub regimes: u32,
    pub breakpoints: u32,
    pub cycles: u32,
    /// Fallbacks that froze units and rebuilt the replay (SECTIONS.md §3.5).
    pub freezes: u32,
}

#[derive(Clone, Debug)]
struct StoreInfo {
    fluid: bool,
    /// Item slots or millibuckets; 0 unknown (no room bound).
    cap: i64,
    nodes: Vec<u32>,
}

/// What a bound watches (its tag is its index + 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Watch {
    /// A store's room, shared out over the goods arriving in it.
    Room { inv: u32, key: PoolKey },
    /// A good flagged empty refilling to its holding threshold.
    Refill { key: PoolKey },
    /// A full store's good draining a stack.
    Frees { inv: u32, key: PoolKey },
    /// A controller's observed level moving toward its threshold.
    Switch { node: u32, key: PoolKey },
    /// A cut good that ran dry coming back.
    Undry { good: Good },
    /// A good the simulated side sends across the cut filling its live receivers' room.
    Seal { good: Good },
}

impl Watch {
    fn key(&self, d: DistrictId) -> PoolKey {
        match *self {
            Watch::Room { key, .. }
            | Watch::Refill { key }
            | Watch::Frees { key, .. }
            | Watch::Switch { key, .. } => key,
            Watch::Undry { good } | Watch::Seal { good } => PoolKey::Stock(d, good),
        }
    }
}

/// Per node, every goods edge's flow under a solve (the larger of in and out), for the latency
/// rule's "runs faster than it did".
fn node_flows(graph: &super::graph::Graph, sol: &Solution) -> Vec<f64> {
    let n = graph.nodes.len();
    let (mut fin, mut fout) = (vec![0.0f64; n], vec![0.0f64; n]);
    for (e, ed) in graph.edges.iter().enumerate() {
        if !super::graph::carries_goods(ed.kind) {
            continue;
        }
        let f = sol.edge_flow.get(e).copied().unwrap_or(0.0);
        fout[ed.from as usize] += f;
        fin[ed.to as usize] += f;
    }
    fin.iter().zip(&fout).map(|(a, b)| a.max(*b)).collect()
}

/// Everything a breakpoint can change, for a mark (and its restore mid-cycle).
#[derive(Clone, Debug, PartialEq)]
struct State {
    empty: BTreeMap<(u32, Good), i64>,
    full: BTreeMap<u32, BTreeMap<Good, i64>>,
    ctrl_on: Vec<bool>,
    dry: BTreeSet<Good>,
    /// Cut goods whose live receivers' room is spent: their simulated senders are off (G2-66).
    sealed: BTreeSet<Good>,
    /// The latency rule: nodes held off until the tick their rise comes into force.
    hold: BTreeMap<u32, Tick>,
}

/// The replay at one breakpoint, for cycle detection.
#[derive(Clone, Debug)]
struct Mark {
    at: Tick,
    sig: u64,
    levels: BTreeMap<PoolKey, i64>,
    nets: BTreeMap<PoolKey, i64>,
    state: State,
    /// Pools a watch other than a room share looks at.
    watched: BTreeSet<PoolKey>,
    /// The pool whose crossing caused this breakpoint (None: a timer or an outside event).
    fired: Option<PoolKey>,
    /// A room share fired (a re-split at a time no period repeats).
    room: bool,
    /// A heater's phase changed here.
    heater: bool,
    /// The time shift applied here: the pool that fired, and the carry it landed on.
    fire: Option<(PoolKey, u32)>,
    fed: Vec<f64>,
}

/// A closed-form cycle running: its pieces' marks (first to last piece, then the repeat).
#[derive(Clone, Debug)]
struct CycleRun {
    start: Tick,
    period: u64,
    marks: Vec<Mark>,
    /// Per heater: burn spent per period.
    spend: Vec<f64>,
}

/// One part of a district's glass replay: a connected piece of the graph with its own process
/// (its id), regimes and cycles (choice G2-22). A district with one part is the whole graph.
#[derive(Clone, Debug)]
struct Part {
    /// Its process id in the cluster; its bounds' tags carry it ([`bound_tag`]).
    id: u16,
    /// Per node: in this part (a hub store is in every part it feeds).
    mine: Vec<bool>,
    /// Hub inventories this part shares: how many parts draw on each.
    hubs: BTreeMap<u32, u32>,
    d: DistrictId,
    compile: u32,
    live: Vec<bool>,
    /// Per node: simulated (a store: its inventory covered; else not live).
    sim: Vec<bool>,
    /// Per node: a simulated store's inventory (its pools).
    inv_of: Vec<Option<u32>>,
    stores: BTreeMap<u32, StoreInfo>,
    /// Contents of inventories the replay does not cover (live, constant here), for controllers.
    fixed: BTreeMap<u32, GoodVec>,
    /// Goods edges across the cut: (edge, from simulated).
    cut: Vec<(u32, bool)>,
    /// Per live store inventory a cut edge from the simulated side lands in: its capacity in item
    /// slots and in millibuckets (0 unknown), for the room the carry may fill (choice G2-66).
    live_caps: BTreeMap<u32, (i64, i64)>,
    /// `Stock` levels at the start (the live side's supply), the base of each good's seal.
    stock0: BTreeMap<Good, i64>,
    st: State,
    heaters: Vec<Burn>,
    heat_at: Tick,
    /// Per heater: the share of its burn its fuel pays for in the current regime.
    fed: Vec<f64>,
    /// Per node: its flow in the regime that runs (the latency rule's baseline).
    flow: Vec<f64>,
    /// The nets that run (32.32).
    nets: BTreeMap<PoolKey, i64>,
    watches: Vec<(Watch, SpaceSpec)>,
    /// This breakpoint's time shift (choice G2-12): the pool that fired and the carry the shift
    /// landed it on. The regime that ended moves back by under a tick, every flow together, so a
    /// repeated regime sequence repeats exactly and nothing runs ahead of the control.
    fire: Option<(PoolKey, u32)>,
    /// When the running process was installed: a breakpoint at that same tick shifts nothing.
    built_at: Tick,
    /// The residual node (G-B13): per pool, 32.32 per tick; runs while coupled.
    residual: BTreeMap<PoolKey, i64>,
    coupled: bool,
    learned_sources: u32,
    marks: Vec<Mark>,
    cycle: Option<CycleRun>,
    /// Breakpoints since the last outside event or cycle.
    steps: u32,
    counts: ReplayCounts,
    /// Per node: its judging unit, [`NONE`] for none (and for every node with one unit or none).
    unit_of: Vec<u16>,
    /// Each inventory a unit holds, and that unit.
    unit_inv: BTreeMap<u32, u16>,
    /// Per node: gated off because its unit does not replay, or its domain's driver's does not.
    gated: Vec<bool>,
    /// Per unit: stops and watches of its stores and controllers that fired since the last outside
    /// event or rebuild, which an oscillation or a stuck stop names.
    hits: BTreeMap<u16, u32>,
    /// Per unit: flags of its stores and controllers that flipped in the last regime build, which
    /// a regime with no fixed point names.
    flips: BTreeMap<u16, u32>,
}

type Levels = BTreeMap<PoolKey, i64>;

/// A flag of inventory `inv` flipped while a regime settles: a flip of the unit holding it.
fn flip_inv(flips: &mut BTreeMap<u16, u32>, unit_inv: &BTreeMap<u32, u16>, inv: u32) {
    if let Some(&u) = unit_inv.get(&inv) {
        *flips.entry(u).or_insert(0) += 1;
    }
}

/// 32.32 of an exact rate: rises round down, falls round away from zero (never above).
fn to_fixed(q: Q) -> i64 {
    if q.is_inf() {
        return i64::MAX / 4;
    }
    if q.to_f64().abs() < NET_EPS {
        return 0;
    }
    let (mut n, mut d) = (i128::from(q.numer()) << 32, i128::from(q.denom()));
    if d < 0 {
        (n, d) = (-n, -d);
    }
    // Floor: a rise rounds down, a fall away from zero.
    let v = n.div_euclid(d);
    i64::try_from(v).unwrap_or(if n < 0 { i64::MIN / 4 } else { i64::MAX / 4 })
}

/// The most a pool moves in one tick at `net`: a diffuser's carry is below a unit, so one tick
/// emits at most `ceil(|net|)` (a fall's front-loaded first tick included).
fn reach(net: i64) -> i64 {
    i64::try_from((i128::from(net).abs() + U - 1) / U).unwrap_or(i64::MAX / 4)
}

/// A controller's lookahead per pool: a tick's move and a unit more, so a share it is given
/// (at least this much) always runs a tick before its bound fires.
fn ctrl_reach(net: i64) -> i64 {
    reach(net) + 1
}

/// Splits `total` by weights: floors, the rest to the heaviest.
fn split(total: i64, w: &[f64]) -> Vec<i64> {
    let sum: f64 = w.iter().sum();
    if w.is_empty() || total <= 0 || sum <= 0.0 {
        return vec![0; w.len()];
    }
    let mut s: Vec<i64> = w
        .iter()
        .map(|&x| ((total as f64) * x / sum).floor().max(0.0) as i64)
        .collect();
    let used: i64 = s.iter().sum();
    let heavy = (0..w.len())
        .max_by(|&a, &b| w[a].total_cmp(&w[b]).then(b.cmp(&a)))
        .expect("nonempty");
    s[heavy] += total - used;
    s
}

fn mix(h: &mut u64, x: u64) {
    for b in x.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn mix_key(h: &mut u64, k: PoolKey) {
    match k {
        PoolKey::Stock(d, g) => {
            mix(h, 1);
            mix(h, d.0);
            mix(h, u64::from(g.0));
        }
        PoolKey::Mail(d, i, a, g) => {
            mix(h, 2);
            mix(h, d.0);
            mix(h, u64::from(i));
            mix(h, u64::from(a));
            mix(h, u64::from(g.0));
        }
        PoolKey::Store(d, i, g) => {
            mix(h, 3);
            mix(h, d.0);
            mix(h, u64::from(i));
            mix(h, u64::from(g.0));
        }
    }
}

/// A heater's burn spent over `dt` ticks at `fed`, landing exactly on each phase boundary.
fn advance_burn(b: &mut Burn, fed: f64, mut dt: f64) {
    for _ in 0..4 {
        if dt <= 0.0 {
            return;
        }
        match b.until(fed) {
            Some(u) if u <= dt => {
                match b.phase() {
                    burn::REFUSING => b.left = b.thr,
                    burn::TAKING => b.left = 0.0,
                    _ => return,
                }
                dt -= u.max(0.0);
            }
            _ => {
                b.left = (b.left - b.spend(fed) * dt).max(0.0);
                return;
            }
        }
    }
}

fn level(lv: &Levels, k: PoolKey) -> i64 {
    lv.get(&k).copied().unwrap_or(0)
}

/// The district's pools in a cluster, by key.
pub fn levels_of(c: &Cluster, d: DistrictId) -> BTreeMap<PoolKey, i64> {
    c.pools()
        .iter()
        .filter(|p| p.key.district() == d)
        .map(|p| (p.key, p.level))
        .collect()
}

/// One district's glass replay: its parts, each its own process (choice G2-22).
#[derive(Clone, Debug)]
pub struct Replay {
    pub started: Tick,
    parts: Vec<Part>,
    covered: BTreeSet<u32>,
    /// Per judging unit (the compile's at the start; empty with one unit or none): it may replay
    /// (SECTIONS.md §3.4).
    replays: Vec<bool>,
    /// Per judging unit: a fallback froze it (§3.5).
    frozen: Vec<bool>,
    /// Fallbacks that froze units.
    freezes: u32,
}

/// Per node: gated off in a replay whose units replay as `on` says (SECTIONS.md §3.5): a
/// simulated node other than a store whose unit does not, and every simulated node on a domain a
/// gated node drives (a boiler of a unit that does not replay stops its network).
fn gate_of(comp: &Compiled, sim: &[bool], unit_of: &[u16], on: &[bool]) -> Vec<bool> {
    let g = &comp.graph;
    let n = g.nodes.len();
    let runs = |v: usize| sim[v] && g.nodes[v].kind != NodeKind::Store;
    let mut gate: Vec<bool> = (0..n)
        .map(|v| {
            runs(v) && unit_of[v] != NONE && !on.get(unit_of[v] as usize).copied().unwrap_or(false)
        })
        .collect();
    let stopped: BTreeSet<u32> = (0..n)
        .filter(|&v| gate[v])
        .filter_map(|v| g.drives[v])
        .collect();
    if !stopped.is_empty() {
        for (v, x) in gate.iter_mut().enumerate() {
            if runs(v) && g.nodes[v].domain.is_some_and(|d| stopped.contains(&d)) {
                *x = true;
            }
        }
    }
    gate
}

impl Replay {
    /// Starts a replay: its member (pools at the engine's levels, each part's first regime's
    /// process and bounds) for a new cluster. With judging units, only those that may replay run;
    /// a fallback in a first regime freezes the units it names and the start is tried again
    /// without them (SECTIONS.md §3.5).
    pub fn start(
        comp: &mut Compiled,
        setup: Setup,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
        at: Tick,
    ) -> Result<(Replay, MemberSpec), Fallback> {
        let mut frozen = vec![false; comp.unit_state.len()];
        let mut freezes = 0;
        loop {
            match Self::try_start(comp, &setup, goods, tun, at, &frozen) {
                Ok((mut rp, spec)) => {
                    rp.freezes = freezes;
                    return Ok((rp, spec));
                }
                Err(e) => {
                    let (i, f, mut rp) = *e;
                    if !rp.freeze_for(i, f, comp) {
                        return Err(f);
                    }
                    frozen = rp.frozen;
                    freezes += 1;
                }
            }
        }
    }

    /// One try at a start with the units in `frozen` gated off: the replay, or which part fell
    /// back and how, with the replay as far as it got.
    #[allow(clippy::type_complexity)]
    fn try_start(
        comp: &mut Compiled,
        setup: &Setup,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
        at: Tick,
        frozen: &[bool],
    ) -> Result<(Replay, MemberSpec), Box<(usize, Fallback, Replay)>> {
        let d = setup.district;
        let (mut whole, levels) = Part::new(comp, setup, goods, tun, at);
        if !comp.unit_state.is_empty() {
            whole.unit_of = comp.units.of_node.clone();
            whole.unit_inv = comp.units.of_inv.iter().map(|(&i, &u)| (i, u)).collect();
        }
        let replays = comp.units_replaying();
        let on: Vec<bool> = replays.iter().zip(frozen).map(|(&r, &f)| r && !f).collect();
        whole.gated = gate_of(comp, &whole.sim, &whole.unit_of, &on);
        whole.residual(comp, tun, &on);
        let mut parts = Vec::new();
        // A part runs apart only where nothing couples it to another: no cut (the live side's
        // supply is one pool) and no residual (learned from the whole).
        if whole.cut.is_empty() && whole.residual.is_empty() {
            let (mines, hubs) = partition(comp, &whole);
            if mines.len() > 1 {
                for (i, mine) in mines.into_iter().enumerate() {
                    parts.push(whole.split(i as u16, mine, hubs.clone()));
                }
            }
        }
        if parts.is_empty() {
            parts.push(whole);
        }
        comp.cache_parts = parts.len();
        let mut rp = Replay {
            started: at,
            parts,
            covered: setup.covered.keys().copied().collect(),
            replays,
            frozen: frozen.to_vec(),
            freezes: 0,
        };
        let mut procs = Vec::with_capacity(rp.parts.len());
        let mut bounds = Vec::new();
        for i in 0..rp.parts.len() {
            match rp.parts[i].build(comp, &levels, at, None, false, goods, tun) {
                Ok((proc, b)) => {
                    procs.push(proc);
                    bounds.extend(b);
                }
                Err(f) => return Err(Box::new((i, f, rp))),
            }
        }
        let spec = MemberSpec {
            district: d,
            pools: levels.iter().map(|(&k, &a)| (k, a, None)).collect(),
            procs,
            space: None,
            bounds,
        };
        Ok((rp, spec))
    }

    /// Per unit: replaying now (may replay, not frozen).
    fn on(&self) -> Vec<bool> {
        self.replays
            .iter()
            .zip(&self.frozen)
            .map(|(&r, &f)| r && !f)
            .collect()
    }

    /// The units that replay in part `i`: on, with a simulated node other than a store there that
    /// runs.
    fn replaying_in(&self, i: usize, comp: &Compiled) -> BTreeSet<u16> {
        let on = self.on();
        let p = &self.parts[i];
        (0..comp.graph.nodes.len())
            .filter(|&v| {
                p.mine[v] && p.sim[v] && comp.graph.nodes[v].kind != NodeKind::Store && !p.gated[v]
            })
            .map(|v| p.unit_of[v])
            .filter(|&u| u != NONE && on.get(u as usize).copied().unwrap_or(false))
            .collect()
    }

    /// Part `i` fell back with `f`: freezes the units it names among those replaying in it, or all
    /// of them when it names none or every one (SECTIONS.md §3.5). False when nothing was left to
    /// freeze there or no unit replays after: the replay ends.
    fn freeze_for(&mut self, i: usize, f: Fallback, comp: &Compiled) -> bool {
        let cand = self.replaying_in(i, comp);
        if cand.is_empty() {
            return false;
        }
        let empty = BTreeMap::new();
        let score = match f {
            Fallback::Oscillating | Fallback::Stuck => &self.parts[i].hits,
            Fallback::Unsettled => &self.parts[i].flips,
            Fallback::Unexpected => &empty,
        };
        let top = cand.iter().filter_map(|u| score.get(u)).copied().max();
        let named: Vec<u16> = match top {
            Some(m) if m > 0 => cand
                .iter()
                .copied()
                .filter(|u| score.get(u) == Some(&m))
                .collect(),
            _ => cand.iter().copied().collect(),
        };
        for u in named {
            self.frozen[u as usize] = true;
        }
        self.freezes += 1;
        self.some_unit_runs(comp)
    }

    /// Under the gates the units replaying now would set: does a simulated node of some unit
    /// still run anywhere?
    fn some_unit_runs(&self, comp: &Compiled) -> bool {
        let p = &self.parts[0];
        let gate = gate_of(comp, &p.sim, &p.unit_of, &self.on());
        (0..comp.graph.nodes.len()).any(|v| {
            p.sim[v]
                && comp.graph.nodes[v].kind != NodeKind::Store
                && p.unit_of[v] != NONE
                && !gate[v]
        })
    }

    /// Sets every part's gates, and a whole replay's residual, from the units replaying now;
    /// returns the parts whose own nodes' gates or whose residual changed (another part's nodes
    /// never run in a part's process). A replay split into parts started with no residual (a
    /// residual keeps the district one part), and units only go off while it runs, so it never
    /// gets one: a part's own stores would hand it shares weighed for the whole.
    fn set_gates(&mut self, comp: &Compiled, tun: &GlassTunables) -> Vec<usize> {
        let on = self.on();
        let p = &self.parts[0];
        let gate = gate_of(comp, &p.sim, &p.unit_of, &on);
        let whole = self.parts.len() == 1;
        let mut changed = Vec::new();
        for (i, p) in self.parts.iter_mut().enumerate() {
            let mut moved = false;
            if whole {
                let before = std::mem::take(&mut p.residual);
                p.residual(comp, tun, &on);
                moved = p.residual != before;
            }
            if moved || (0..gate.len()).any(|v| p.mine[v] && p.gated[v] != gate[v]) {
                changed.push(i);
            }
            p.gated.clone_from(&gate);
        }
        changed
    }

    /// Part `i` fell back with `f` at `at`: the units it names freeze and every part they gate is
    /// rebuilt now, from the levels as they are, keeping every booking made so far (the cluster
    /// replaces a process from a tick on). A part that falls back in its rebuild freezes the units
    /// it names in turn. Err when no unit is left to replay.
    #[allow(clippy::too_many_arguments)]
    fn recover(
        &mut self,
        i: usize,
        f: Fallback,
        comp: &mut Compiled,
        c: &mut Cluster,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        let (mut i, mut f) = (i, f);
        let mut pending: BTreeSet<usize> = BTreeSet::new();
        loop {
            if !self.freeze_for(i, f, comp) {
                return Err(f);
            }
            pending.extend(self.set_gates(comp, tun));
            pending.insert(i);
            let mut failed = None;
            while let Some(j) = pending.pop_first() {
                if let Err(e) = self.parts[j].run(comp, c, None, at, goods, tun) {
                    failed = Some((j, e));
                    break;
                }
            }
            match failed {
                None => return Ok(()),
                Some((j, e)) => {
                    i = j;
                    f = e;
                }
            }
        }
    }

    /// Per unit: it may replay and no fallback froze it; empty with one unit or none.
    pub fn units_on(&self) -> Vec<bool> {
        self.on()
    }

    /// Per unit: a fallback froze it.
    pub fn frozen(&self) -> &[bool] {
        &self.frozen
    }

    /// Fallbacks that froze units since the start.
    pub fn freezes(&self) -> u32 {
        self.freezes
    }

    /// Per node: gated off because its unit does not replay, or its domain's driver's does not.
    pub fn gated(&self) -> &[bool] {
        &self.parts[0].gated
    }

    /// Does the replay gate its units as `comp` would start one now: the same compile, the same
    /// units, the same ones allowed? A unit that went stale, failed, or was cut anew needs a new
    /// start, and so does a new compile that kept every unit: a replay of the old one stops at its
    /// next step (`glass_step`), so going on with it would stall its bounds and timers.
    pub fn same_units(&self, comp: &Compiled) -> bool {
        if self.compile() != comp.graph.end.compile {
            return false;
        }
        let p = &self.parts[0];
        let unit_of: &[u16] = if comp.unit_state.is_empty() {
            &[]
        } else {
            &comp.units.of_node
        };
        let same_nodes = if unit_of.is_empty() {
            p.unit_of.iter().all(|&u| u == NONE)
        } else {
            p.unit_of == unit_of
        };
        same_nodes && self.replays == comp.units_replaying()
    }

    pub fn compile(&self) -> u32 {
        self.parts[0].compile
    }

    /// The compile held was kept under a new session's number (choice G2-81): the replay goes on,
    /// as that number's.
    pub fn renumber(&mut self, compile: u32) {
        for p in &mut self.parts {
            p.compile = compile;
        }
    }

    /// Does a restored replay index a graph of `n` nodes (a page check, G2-80)?
    pub fn fits(&self, n: usize) -> bool {
        let nu = self.replays.len();
        !self.parts.is_empty()
            && self.frozen.len() == nu
            && self.parts.iter().all(|p| {
                [
                    p.mine.len(),
                    p.live.len(),
                    p.sim.len(),
                    p.inv_of.len(),
                    p.flow.len(),
                    p.st.ctrl_on.len(),
                    p.unit_of.len(),
                    p.gated.len(),
                ]
                .iter()
                .all(|&l| l == n)
                    && p.fed.len() == p.heaters.len()
                    && p.heaters.iter().all(|b| (b.node as usize) < n)
                    && p.unit_of.iter().all(|&u| u == NONE || (u as usize) < nu)
                    && p.unit_inv.values().all(|&u| (u as usize) < nu)
            })
    }

    /// Per node: it ran in the world when this replay started.
    pub fn live(&self) -> &[bool] {
        &self.parts[0].live
    }

    /// Goods edges between live and simulated nodes.
    pub fn cut_edges(&self) -> u32 {
        self.parts[0].cut.len() as u32
    }

    /// Goods the residual node adds as learned sources (I9).
    pub fn learned_sources(&self) -> u32 {
        self.parts[0].learned_sources
    }

    /// Does the replay book inventory `inv` per store?
    pub fn covers(&self, inv: u32) -> bool {
        self.covered.contains(&inv)
    }

    /// Is a closed-form cycle running in some part?
    pub fn in_cycle(&self) -> bool {
        self.parts.iter().any(|p| p.cycle.is_some())
    }

    /// The parts, each its own process (ids 0, 1, ...).
    pub fn parts(&self) -> u16 {
        self.parts.len() as u16
    }

    /// Counters since the replay started, over its parts.
    pub fn counts(&self) -> ReplayCounts {
        let mut c = ReplayCounts {
            freezes: self.freezes,
            ..ReplayCounts::default()
        };
        for p in &self.parts {
            c.regimes += p.counts.regimes;
            c.breakpoints += p.counts.breakpoints;
            c.cycles += p.counts.cycles;
        }
        c
    }

    /// Process `id` stopped (a breakpoint of that part): change its regime and replace its
    /// process. A stop another part's change already lifted is nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn on_stop(
        &mut self,
        comp: &mut Compiled,
        c: &mut Cluster,
        id: u16,
        why: Stop,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        let Some(p) = self.parts.get_mut(id as usize) else {
            return Err(Fallback::Unexpected);
        };
        if c.run_of(p.d, id) != Some(Run::Stopped(why)) {
            return Ok(());
        }
        match p.run(comp, c, Some(why), at, goods, tun) {
            Ok(()) => Ok(()),
            Err(f) => self.recover(id as usize, f, comp, c, at, goods, tun),
        }
    }

    /// Something outside the replay changed its pools or the district (an import, an export, a
    /// correction): imports land in the stores that feed the graph (choice G2-14), and every part
    /// rebuilds its regime from the levels as they are.
    pub fn refresh(
        &mut self,
        comp: &mut Compiled,
        c: &mut Cluster,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        for p in &self.parts {
            p.absorb_imports(comp, c, at);
        }
        for i in 0..self.parts.len() {
            if let Err(f) = self.parts[i].run(comp, c, None, at, goods, tun) {
                self.recover(i, f, comp, c, at, goods, tun)?;
            }
        }
        Ok(())
    }
}

/// The parts of a district's graph that run apart (choice G2-22): per part, its nodes; and the
/// hub inventories, with how many parts draw on each. A hub is a covered store that only
/// supplies (no goods edge into it, no other edge touching it, no kinetic domain): a source its
/// parts draw on independently, which the cluster keeps safe between them. Anything else that
/// touches joins: every edge, a kinetic domain and the boiler driving it, one inventory's nodes.
/// A part has a simulated machine; what is in none (an idle store, a live piece) goes to the
/// first.
fn partition(comp: &Compiled, whole: &Part) -> (Vec<Vec<bool>>, BTreeMap<u32, u32>) {
    let graph = &comp.graph;
    let n = graph.nodes.len();
    let mut hub_inv: BTreeSet<u32> = whole.stores.keys().copied().collect();
    for ed in &graph.edges {
        let goods = super::graph::carries_goods(ed.kind);
        if let Some(inv) = whole.inv_of[ed.to as usize] {
            hub_inv.remove(&inv);
        }
        if !goods && let Some(inv) = whole.inv_of[ed.from as usize] {
            hub_inv.remove(&inv);
        }
    }
    for v in 0..n {
        if let Some(inv) = whole.inv_of[v]
            && graph.node(v as u32).domain.is_some()
        {
            hub_inv.remove(&inv);
        }
    }
    let hub = |v: u32| whole.inv_of[v as usize].is_some_and(|i| hub_inv.contains(&i));
    let mut uf: Vec<u32> = (0..n as u32).collect();
    fn find(uf: &mut [u32], mut x: u32) -> u32 {
        while uf[x as usize] != x {
            let up = uf[uf[x as usize] as usize];
            uf[x as usize] = up;
            x = up;
        }
        x
    }
    fn join(uf: &mut [u32], a: u32, b: u32) {
        let (ra, rb) = (find(uf, a), find(uf, b));
        if ra != rb {
            uf[ra.max(rb) as usize] = ra.min(rb);
        }
    }
    for ed in &graph.edges {
        if !hub(ed.from) && !hub(ed.to) {
            join(&mut uf, ed.from, ed.to);
        }
    }
    let mut domain_first: BTreeMap<u32, u32> = BTreeMap::new();
    for v in 0..n as u32 {
        let node = graph.node(v);
        for dm in node.domain.into_iter().chain(graph.drives[v as usize]) {
            let f = *domain_first.entry(dm).or_insert(v);
            join(&mut uf, f, v);
        }
    }
    for (inv, st) in &whole.stores {
        if hub_inv.contains(inv) {
            continue;
        }
        for w in st.nodes.windows(2) {
            join(&mut uf, w[0], w[1]);
        }
    }
    // Parts in order of their first simulated machine.
    let mut part_of_root: BTreeMap<u32, usize> = BTreeMap::new();
    for v in 0..n as u32 {
        if whole.sim[v as usize] && graph.node(v).kind != NodeKind::Store && !hub(v) {
            let r = find(&mut uf, v);
            let next = part_of_root.len();
            part_of_root.entry(r).or_insert(next);
        }
    }
    let parts = part_of_root.len().max(1);
    let mut mines = vec![vec![false; n]; parts];
    for v in 0..n as u32 {
        if hub(v) {
            continue;
        }
        let r = find(&mut uf, v);
        let p = part_of_root.get(&r).copied().unwrap_or(0);
        mines[p][v as usize] = true;
    }
    // A hub is in every part it supplies.
    let mut sharers: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for ed in &graph.edges {
        if !hub(ed.from) || hub(ed.to) {
            continue;
        }
        let r = find(&mut uf, ed.to);
        let p = part_of_root.get(&r).copied().unwrap_or(0);
        mines[p][ed.from as usize] = true;
        if let Some(inv) = whole.inv_of[ed.from as usize] {
            sharers.entry(inv).or_default().insert(p);
        }
    }
    for v in 0..n {
        if hub(v as u32) && !mines.iter().any(|m| m[v]) {
            mines[0][v] = true;
        }
    }
    let hubs = sharers
        .into_iter()
        .filter(|(_, ps)| ps.len() > 1)
        .map(|(inv, ps)| (inv, ps.len() as u32))
        .collect();
    (mines, hubs)
}

impl Part {
    /// The whole district as one part, at the engine's levels, its flags read from them.
    fn new(
        comp: &Compiled,
        setup: &Setup,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
        at: Tick,
    ) -> (Part, Levels) {
        let graph = &comp.graph;
        let d = setup.district;
        let n = graph.nodes.len();
        let mut live = setup.live.clone();
        live.resize(n, false);
        let mut sim = vec![false; n];
        let mut inv_of = vec![None; n];
        let mut stores: BTreeMap<u32, StoreInfo> = BTreeMap::new();
        for v in 0..n as u32 {
            let node = graph.node(v);
            if node.kind == NodeKind::Store {
                match node.inventory {
                    Some(inv) => {
                        if let Some(decl) = setup.covered.get(&inv) {
                            sim[v as usize] = true;
                            inv_of[v as usize] = Some(inv);
                            let fluid = graph.is_fluid(v);
                            let e = stores.entry(inv).or_insert(StoreInfo {
                                fluid,
                                cap: 0,
                                nodes: Vec::new(),
                            });
                            e.fluid |= fluid;
                            e.nodes.push(v);
                            if e.cap <= 0 {
                                e.cap = if node.capacity > 0 {
                                    node.capacity
                                } else if e.fluid {
                                    decl.fluid_capacity
                                } else {
                                    i64::from(decl.item_slots)
                                };
                            }
                        }
                    }
                    None => sim[v as usize] = !live[v as usize],
                }
            } else {
                sim[v as usize] = !live[v as usize];
            }
        }
        let mut cut = Vec::new();
        for (e, ed) in graph.edges.iter().enumerate() {
            if !super::graph::carries_goods(ed.kind) {
                continue;
            }
            let (a, b) = (sim[ed.from as usize], sim[ed.to as usize]);
            if a != b {
                cut.push((e as u32, a));
            }
        }
        // The live stores the simulated side sends into, with what they can hold.
        let mut live_caps: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
        for &(e, from_sim) in &cut {
            let v = graph.edge(e).to;
            let node = graph.node(v);
            let (true, NodeKind::Store, Some(inv)) = (from_sim, node.kind, node.inventory) else {
                continue;
            };
            let c = live_caps.entry(inv).or_insert((0, 0));
            if graph.is_fluid(v) {
                c.1 = c.1.max(node.capacity);
            } else {
                c.0 = c.0.max(node.capacity);
            }
        }
        for (inv, c) in &mut live_caps {
            if let Some(decl) = setup.live_decls.get(inv) {
                if c.0 <= 0 {
                    c.0 = i64::from(decl.item_slots);
                }
                if c.1 <= 0 {
                    c.1 = decl.fluid_capacity;
                }
            }
        }
        let fixed: BTreeMap<u32, GoodVec> = setup
            .contents
            .iter()
            .filter(|(i, _)| !setup.covered.contains_key(i))
            .map(|(&i, c)| (i, c.clone()))
            .collect();
        // Pools: every covered store's goods; the cut's supply when goods cross it.
        let mut levels: Levels = BTreeMap::new();
        for &inv in stores.keys() {
            if let Some(c) = setup.contents.get(&inv) {
                for (g, a) in c.iter() {
                    if a > 0 {
                        levels.insert(PoolKey::Store(d, inv, g), a);
                    }
                }
            }
        }
        if cut.iter().any(|&(_, from_sim)| !from_sim) {
            let mut supply = GoodVec::new();
            for c in fixed.values() {
                supply.merge(c);
            }
            for (g, a) in setup.owed.iter() {
                if a > 0 {
                    supply.add(g, a);
                }
            }
            for (g, a) in supply.iter() {
                if a > 0 {
                    levels.insert(PoolKey::Stock(d, g), a);
                }
            }
        }
        let stock0: BTreeMap<Good, i64> = levels
            .iter()
            .filter_map(|(k, &a)| match *k {
                PoolKey::Stock(_, g) => Some((g, a)),
                _ => None,
            })
            .collect();
        // Heaters spend from where the model left them to now, on the compile's last regime.
        let mut heaters: Vec<Burn> = comp.heaters().to_vec();
        let dt = at.0.saturating_sub(comp.model_at().0) as f64;
        for b in &mut heaters {
            let f = comp.sol.fed.get(b.node as usize).copied().unwrap_or(0.0);
            advance_burn(b, f, dt);
        }
        let fed = heaters
            .iter()
            .map(|b| comp.sol.fed.get(b.node as usize).copied().unwrap_or(0.0))
            .collect();
        let mut rp = Part {
            id: 0,
            mine: vec![true; n],
            hubs: BTreeMap::new(),
            d,
            compile: graph.end.compile,
            live,
            sim,
            inv_of,
            stores,
            fixed,
            cut,
            live_caps,
            stock0,
            st: State {
                empty: BTreeMap::new(),
                full: BTreeMap::new(),
                ctrl_on: comp.ctrl_on.clone(),
                dry: BTreeSet::new(),
                sealed: BTreeSet::new(),
                hold: BTreeMap::new(),
            },
            heaters,
            heat_at: at,
            fed,
            flow: vec![0.0; n],
            nets: BTreeMap::new(),
            watches: Vec::new(),
            fire: None,
            built_at: at,
            residual: BTreeMap::new(),
            coupled: false,
            learned_sources: 0,
            marks: Vec::new(),
            cycle: None,
            steps: 0,
            counts: ReplayCounts::default(),
            unit_of: vec![NONE; n],
            unit_inv: BTreeMap::new(),
            gated: vec![false; n],
            hits: BTreeMap::new(),
            flips: BTreeMap::new(),
        };
        // A store below its holding threshold passes on what it receives (GB100), as the
        // compile's own regime read it.
        let mut empty = BTreeMap::new();
        for (&k, &a) in &levels {
            if let PoolKey::Store(_, inv, g) = k
                && a < rp.hold(comp, inv, g, goods, tun)
            {
                empty.insert((inv, g), 0);
            }
        }
        rp.st.empty = empty;
        (rp, levels)
    }

    /// This part of the whole: the nodes in `mine`, the stores they touch, their heaters; the
    /// flags of its own stores only.
    fn split(&self, id: u16, mine: Vec<bool>, hubs: BTreeMap<u32, u32>) -> Part {
        let mut p = self.clone();
        p.id = id;
        p.stores
            .retain(|_, st| st.nodes.iter().any(|&v| mine[v as usize]));
        p.hubs = hubs
            .into_iter()
            .filter(|(inv, _)| p.stores.contains_key(inv))
            .collect();
        let keep: Vec<bool> = p.heaters.iter().map(|b| mine[b.node as usize]).collect();
        let mut j = 0;
        p.heaters.retain(|_| {
            j += 1;
            keep[j - 1]
        });
        let mut j = 0;
        p.fed.retain(|_| {
            j += 1;
            keep[j - 1]
        });
        let stores = &p.stores;
        p.st.empty.retain(|&(inv, _), _| stores.contains_key(&inv));
        p.mine = mine;
        p
    }

    /// The unit holding inventory `inv`, if one does.
    fn unit_of_inv(&self, inv: u32) -> Option<u16> {
        self.unit_inv.get(&inv).copied()
    }

    /// The unit a stop names: the unit of the store it emptied, or of the store or controller its
    /// watch looks at. A cut's supply, a seal, a timer and the rest name none.
    fn unit_of_stop(&self, w: Stop) -> Option<u16> {
        match w {
            Stop::InputEmpty(PoolKey::Store(_, inv, _)) => self.unit_of_inv(inv),
            Stop::Bound(_, tag) if bound_part(tag) == self.id => {
                let (watch, _) = self
                    .watches
                    .get((bound_index(tag) as usize).wrapping_sub(1))?;
                match *watch {
                    Watch::Room { inv, .. } | Watch::Frees { inv, .. } => self.unit_of_inv(inv),
                    Watch::Refill {
                        key: PoolKey::Store(_, inv, _),
                    } => self.unit_of_inv(inv),
                    Watch::Switch { node, .. } => self
                        .unit_of
                        .get(node as usize)
                        .copied()
                        .filter(|&u| u != NONE),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn run(
        &mut self,
        comp: &mut Compiled,
        c: &mut Cluster,
        why: Option<Stop>,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        let mut why = why;
        if why.is_none() {
            self.leave_cycle(at);
            self.steps = 0;
            self.marks.clear();
            self.hits.clear();
        } else if self.cycle.is_some()
            && let Some(w) = why
            && !matches!(w, Stop::Timer | Stop::Disabled)
        {
            // A hub's envelope stopped the cycle (another part drains the hub too, §4.2): leave it
            // where it is and step on. A cycle that ran a whole period made progress.
            if self.leave_cycle(at) >= 1 {
                self.steps = 0;
            }
            self.marks.clear();
        }
        for _ in 0..MAX_STEPS {
            self.fire = None;
            if let Some(w) = why
                && at > self.built_at
            {
                let crossing = match w {
                    Stop::InputEmpty(k) => Some((k, false)),
                    Stop::Bound(_, tag) if bound_part(tag) == self.id => self
                        .watches
                        .get((bound_index(tag) as usize).wrapping_sub(1))
                        .filter(|(watch, _)| !matches!(watch, Watch::Room { .. }))
                        .map(|(watch, spec)| {
                            let rising = spec.0.first().is_some_and(|&(_, w)| w > 0);
                            (watch.key(self.d), rising)
                        }),
                    _ => None,
                };
                if let Some((k, rising)) = crossing
                    && let Some(canon) = c.shift_back(self.d, self.id, k, rising)
                {
                    self.fire = Some((k, canon));
                }
            }
            let lv = levels_of(c, self.d);
            let mut fired = None;
            let mut room = false;
            let mut heater = false;
            if let Some(w) = why {
                self.counts.breakpoints += 1;
                comp.replay_breakpoints += 1;
                // A hold coming due is not an oscillation: a node holds at most once a rise of its
                // potential, and a rise needs another breakpoint or an outside event.
                let release = matches!(w, Stop::Timer)
                    && self.cycle.is_none()
                    && self.st.hold.values().any(|&t| t <= at);
                if !release {
                    self.steps += 1;
                }
                // Counted first: the stop past the limit names its unit too.
                if let Some(u) = self.unit_of_stop(w) {
                    *self.hits.entry(u).or_insert(0) += 1;
                }
                if self.steps > tun.breakpoints_per_event_max {
                    return Err(Fallback::Oscillating);
                }
                match w {
                    Stop::InputEmpty(k @ PoolKey::Store(_, inv, g)) => {
                        self.st.empty.insert((inv, g), level(&lv, k));
                        fired = Some(k);
                    }
                    Stop::InputEmpty(k @ PoolKey::Stock(_, g)) => {
                        self.st.dry.insert(g);
                        fired = Some(k);
                    }
                    Stop::Bound(_, tag) => {
                        let Some(&(watch, _)) = (bound_part(tag) == self.id)
                            .then(|| {
                                self.watches
                                    .get((bound_index(tag) as usize).wrapping_sub(1))
                            })
                            .flatten()
                        else {
                            return Err(Fallback::Unexpected);
                        };
                        fired = Some(watch.key(self.d));
                        match watch {
                            Watch::Room { .. } => room = true,
                            Watch::Refill { key } => {
                                if let PoolKey::Store(_, inv, g) = key {
                                    self.st.empty.remove(&(inv, g));
                                }
                            }
                            Watch::Frees { inv, .. } => {
                                self.st.full.remove(&inv);
                            }
                            Watch::Switch { .. } => {}
                            Watch::Undry { good } => {
                                self.st.dry.remove(&good);
                            }
                            Watch::Seal { good } => {
                                self.st.sealed.insert(good);
                            }
                        }
                    }
                    Stop::Timer => {
                        if self.cycle.is_some() {
                            self.end_cycle(at);
                        } else {
                            heater = self.advance_heaters(at);
                        }
                    }
                    Stop::Disabled => return Ok(()),
                    _ => return Err(Fallback::Unexpected),
                }
            }
            if !heater {
                self.advance_heaters(at);
            }
            let (proc, bounds) = self.build(comp, &lv, at, fired, room || heater, goods, tun)?;
            let mut proc = proc;
            let mut bounds = bounds;
            if heater && let Some(m) = self.marks.last_mut() {
                m.heater = true;
            }
            let carries = c.carries_of_proc(self.d, self.id);
            if let Some((cp, deadline)) = self.detect(at, &carries, goods) {
                proc = cp;
                proc.deadline = Some(deadline);
                bounds = Vec::new();
            }
            c.replace_proc(self.d, proc, bounds, at);
            self.built_at = at;
            match c.run_of(self.d, self.id) {
                Some(Run::Running) | None => return Ok(()),
                Some(Run::Stopped(Stop::Waiting)) => return Err(Fallback::Stuck),
                Some(Run::Stopped(w)) => {
                    // Blocked at once: the next transition, at the same tick.
                    why = Some(w);
                }
            }
        }
        Err(Fallback::Stuck)
    }

    /// Imports (a train's unload, a give-back, an injection) land in the district's `Stock` pool,
    /// which no process reads. With no cut, each good moves into the covered store that feeds the
    /// graph with it (one with room first, then the lowest inventory index), so the machines see
    /// it; the store's change is written back to that store at the reconcile. A good no covered
    /// store feeds waits in `Stock` for the reconcile, as does every import across a partial
    /// load's cut, where `Stock` is the live side's supply (choice G2-14).
    fn absorb_imports(&self, comp: &Compiled, c: &mut Cluster, at: Tick) {
        if !self.cut.is_empty() {
            return;
        }
        let d = self.d;
        let waiting: Vec<(Good, i64)> = levels_of(c, d)
            .iter()
            .filter_map(|(k, &a)| match *k {
                PoolKey::Stock(_, g) if a > 0 => Some((g, a)),
                _ => None,
            })
            .collect();
        for (g, a) in waiting {
            let Some(inv) = self.landing(comp, g) else {
                continue;
            };
            let took = -c.add_to_pool(PoolKey::Stock(d, g), -a, false, at);
            if took > 0 {
                c.add_to_pool(PoolKey::Store(d, inv, g), took, false, at);
            }
        }
    }

    /// The covered store an import of `g` lands in: one that feeds the graph with it.
    fn landing(&self, comp: &Compiled, g: Good) -> Option<u32> {
        let graph = &comp.graph;
        let feeds = |inv: &u32| {
            self.stores[inv]
                .nodes
                .iter()
                .any(|&v| graph.out_g.of(v).iter().any(|&e| graph.allows(e, g)))
        };
        let mut cands = self.stores.keys().filter(|i| feeds(i));
        let first = cands.clone().next().copied();
        cands
            .find(|i| !self.st.full.contains_key(i))
            .copied()
            .or(first)
    }

    /// What a store must hold of a good to supply it (the compile's holding rule).
    fn hold(
        &self,
        comp: &Compiled,
        inv: u32,
        g: Good,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> i64 {
        let graph = &comp.graph;
        let fed = self.stores.get(&inv).is_some_and(|s| {
            s.nodes
                .iter()
                .any(|&v| graph.in_g.of(v).iter().any(|&e| graph.allows(e, g)))
        });
        holding_threshold(goods, g, tun.holding_stacks, fed).max(1)
    }

    /// The regime the flags say, for the solve.
    fn regime(&self, comp: &Compiled, lv: &Levels) -> Regime {
        let graph = &comp.graph;
        let mut rg = comp.regime.clone();
        for v in 0..graph.nodes.len() {
            if graph.node(v as u32).kind != NodeKind::Store {
                continue;
            }
            if let Some(inv) = self.inv_of[v] {
                rg.read[v] = true;
                rg.held[v] = lv
                    .range(
                        PoolKey::Store(self.d, inv, Good(0))
                            ..=PoolKey::Store(self.d, inv, Good(u32::MAX)),
                    )
                    .filter(|&(k, &a)| a > 0 && !self.st.empty.contains_key(&(inv, k.good())))
                    .map(|(k, _)| k.good())
                    .collect();
                rg.room[v] = !self.st.full.contains_key(&inv);
            } else if !self.st.dry.is_empty() {
                rg.held[v].retain(|g| !self.st.dry.contains(g));
            }
        }
        gates(graph, &self.st.ctrl_on, &mut rg.gate_off);
        // A unit that does not replay runs nothing (SECTIONS.md §3.5).
        for (v, &x) in self.gated.iter().enumerate() {
            if x {
                rg.gate_off[v] = true;
            }
        }
        // Another part's nodes run in its own process.
        for (v, &m) in self.mine.iter().enumerate() {
            if !m && self.sim[v] && graph.node(v as u32).kind != NodeKind::Store {
                rg.gate_off[v] = true;
            }
        }
        // A dry cut good stops the live node that would send it.
        if !self.st.dry.is_empty() {
            for &(e, from_sim) in &self.cut {
                if from_sim {
                    continue;
                }
                let ed = graph.edge(e);
                if graph.node(ed.from).kind != NodeKind::Store
                    && self.st.dry.iter().any(|&g| graph.allows(e, g))
                {
                    rg.gate_off[ed.from as usize] = true;
                }
            }
        }
        // A sealed cut good stops what would carry more of it into the live side (choice G2-66):
        // the simulated node that sends it, or the live machine a simulated store feeds.
        if !self.st.sealed.is_empty() {
            for &(e, from_sim) in &self.cut {
                if !from_sim || !self.st.sealed.iter().any(|&g| graph.allows(e, g)) {
                    continue;
                }
                let ed = graph.edge(e);
                if graph.node(ed.from).kind != NodeKind::Store {
                    rg.gate_off[ed.from as usize] = true;
                } else if graph.node(ed.to).kind != NodeKind::Store {
                    rg.gate_off[ed.to as usize] = true;
                } else {
                    rg.room[ed.to as usize] = false;
                }
            }
        }
        for b in &self.heaters {
            rg.burning[b.node as usize] = b.phase();
        }
        rg
    }

    /// The solve's flows booked per pool, exact, and as 32.32 nets with the residual.
    fn book(&self, comp: &Compiled, sol: &Solution, coupled: bool) -> BTreeMap<PoolKey, i64> {
        let graph = &comp.graph;
        let d = self.d;
        let mut q: BTreeMap<PoolKey, Q> = BTreeMap::new();
        let mut add = |k: PoolKey, f: Q, sign: bool| {
            let e = q.entry(k).or_insert(Q::ZERO);
            *e = if sign { e.add(f) } else { e.sub(f) };
        };
        for &(e, g, f) in &sol.edge_goods {
            let ed = graph.edge(e);
            let (a, b) = (ed.from as usize, ed.to as usize);
            // Another part's edges are its own to book.
            if (!self.sim[a] && !self.sim[b]) || !self.mine[a] || !self.mine[b] {
                continue;
            }
            if let Some(inv) = self.inv_of[a] {
                add(PoolKey::Store(d, inv, g), f, false);
            }
            if let Some(inv) = self.inv_of[b] {
                add(PoolKey::Store(d, inv, g), f, true);
            }
            if self.sim[a] != self.sim[b] {
                add(PoolKey::Stock(d, g), f, self.sim[a]);
            }
        }
        let mut out: BTreeMap<PoolKey, i64> = q
            .into_iter()
            .map(|(k, v)| (k, to_fixed(v)))
            .filter(|&(_, v)| v != 0)
            .collect();
        if coupled {
            for (&k, &r) in &self.residual {
                *out.entry(k).or_default() += r;
            }
            out.retain(|_, v| *v != 0);
        }
        out
    }

    /// The latency rule (§4.4, choice G2-6): a simulated node with a downstream latency whose
    /// potential (its flow in this regime with nothing held) is above its potential in the
    /// regime before is held off until its output could have arrived. Holding it off is a solve
    /// of its own (the node gated), so every flow stays recipe-consistent, and what it makes
    /// arrives no earlier than in Create. Potentials do not depend on holds, so a hold coming due
    /// never makes another node hold (a machine fed by two held paths, or a loop, would chain
    /// holds forever). True if a node was newly held.
    fn hold_rises(&mut self, comp: &Compiled, potential: &[f64], at: Tick) -> bool {
        let graph = &comp.graph;
        let mut added = false;
        for (v, &f) in potential.iter().enumerate() {
            let lat = graph.down_latency.get(v).copied().unwrap_or(0);
            let id = v as u32;
            if lat == 0 || !self.sim[v] || !self.mine[v] || graph.node(id).kind == NodeKind::Store {
                continue;
            }
            let old = self.flow.get(v).copied().unwrap_or(0.0);
            if f <= old * (1.0 + 1e-9) + 1e-12 {
                continue;
            }
            // A further rise while held waits from now; only a new hold changes the solve.
            match self.st.hold.entry(id) {
                std::collections::btree_map::Entry::Occupied(mut o) => {
                    let t = o.get_mut();
                    *t = (*t).max(at + lat);
                }
                std::collections::btree_map::Entry::Vacant(o) => {
                    o.insert(at + lat);
                    added = true;
                }
            }
        }
        added
    }

    /// The residual node from the passing window (G-B13, §5.1): per good, what the model misses,
    /// shaded by the checksum's z, split over the covered stores by their own slopes. A store of a
    /// unit that is not on (`on`, per unit) takes no share: its unit does not run, and its stores
    /// are pools only replaying neighbours touch (SECTIONS.md §3.5, choice SC68). Shares are
    /// weighed over every store, so a gated unit's share is dropped, not given to the others.
    fn residual(&mut self, comp: &Compiled, tun: &GlassTunables, on: &[bool]) {
        self.residual.clear();
        self.learned_sources = 0;
        let Some(win) = comp.passed.as_ref() else {
            return;
        };
        let unit_inv = &self.unit_inv;
        let gated_store = |inv: u32| {
            unit_inv
                .get(&inv)
                .is_some_and(|&u| !on.get(u as usize).copied().unwrap_or(false))
        };
        let mut residual: BTreeMap<PoolKey, i64> = BTreeMap::new();
        let mut learned = 0;
        let d = self.d;
        let z = tun.checksum_z;
        for gr in &win.goods {
            if gr.class == report::class::EXACT
                || !gr.measured.is_finite()
                || !gr.predicted.is_finite()
            {
                continue;
            }
            let m = gr.measured;
            let s = m.signum() * (m.abs() - z * gr.sigma.max(0.0)).max(0.0);
            // A bounded good inside [L, U] is the model's, calibrated (§5.3), never a residual:
            // goods are tied to each other by recipes, and a residual on one alone books it from
            // nothing (or loses it) when its partners' windows say otherwise. Only a production
            // above U is something unmodelled producing: its excess over U is a learned source
            // (I9, choice G2-57). The unbounded goods (a black box's) take measured minus
            // predicted, the residual node of G-B13.
            let r = if gr.class == report::class::BOUNDED {
                let hi = gr.hi.max(gr.lo);
                if !hi.is_finite() || s <= hi || s <= 0.0 {
                    continue;
                }
                s - hi
            } else {
                s - gr.predicted
            };
            if r.abs() < NET_EPS {
                continue;
            }
            let mine: Vec<(u32, f64, f64)> = win
                .stores
                .iter()
                .filter(|s| s.1 == gr.good && self.stores.contains_key(&s.0))
                .map(|s| (s.0, s.2, s.3))
                .collect();
            let mut cands: Vec<(u32, f64)> = mine
                .iter()
                .filter(|&&(_, ms, ps)| (ms - ps).signum() == r.signum() && (ms - ps).abs() > 0.0)
                .map(|&(i, ms, ps)| (i, (ms - ps).abs()))
                .collect();
            if cands.is_empty() && r < 0.0 {
                cands = mine
                    .iter()
                    .filter(|&&(_, _, ps)| ps > 0.0)
                    .map(|&(i, _, ps)| (i, ps))
                    .collect();
            }
            let total: f64 = cands.iter().map(|c| c.1).sum();
            if cands.is_empty() || total <= 0.0 {
                continue;
            }
            let mut booked = false;
            for (i, w) in cands {
                if gated_store(i) {
                    continue;
                }
                let x = r * w / total;
                // Floor: a rise rounds down, a fall away from zero (never above).
                let f = (x * FIXED_ONE as f64).floor() as i64;
                if f != 0 {
                    *residual.entry(PoolKey::Store(d, i, gr.good)).or_default() += f;
                    booked = true;
                }
            }
            if booked && r > 0.0 && s > 0.0 {
                learned += 1;
            }
        }
        self.residual = residual;
        self.learned_sources = learned;
    }

    /// The residual runs while the factory is as the window saw it: every store it drains holds
    /// the good, every store it fills has room.
    fn coupling(&self, lv: &Levels) -> bool {
        !self.residual.is_empty()
            && self.residual.iter().all(|(&k, &r)| match k {
                PoolKey::Store(_, inv, g) if r < 0 => {
                    level(lv, k) > 0 && !self.st.empty.contains_key(&(inv, g))
                }
                PoolKey::Store(_, inv, _) => !self.st.full.contains_key(&inv),
                _ => true,
            })
    }

    /// Room shares of a store over the goods arriving in it: (pool, units it may still take).
    fn shares(
        &self,
        inv: u32,
        lv: &Levels,
        arriving: &[(PoolKey, i64)],
        goods: &dyn GoodLookup,
    ) -> Vec<(PoolKey, i64)> {
        let st = &self.stores[&inv];
        let d = self.d;
        let mine =
            || lv.range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)));
        let mut out = Vec::with_capacity(arriving.len());
        if st.cap <= 0 {
            return arriving.iter().map(|&(k, _)| (k, i64::MAX / 4)).collect();
        }
        let kind = if st.fluid {
            GoodClass::Fluid
        } else {
            GoodClass::Item
        };
        type Shares = Vec<(PoolKey, i64)>;
        let (fits, other): (Shares, Shares) = arriving
            .iter()
            .partition(|&&(k, _)| goods.class(k.good()) == kind);
        for (k, _) in other {
            out.push((k, i64::MAX / 4));
        }
        if fits.is_empty() {
            return out;
        }
        if st.fluid {
            let used: i64 = mine()
                .filter(|(k, _)| goods.class(k.good()) == GoodClass::Fluid)
                .map(|(_, &a)| a.max(0))
                .sum();
            let free = (st.cap - used).max(0);
            let w: Vec<f64> = fits.iter().map(|&(_, r)| r as f64).collect();
            for (j, s) in split(free, &w).into_iter().enumerate() {
                out.push((fits[j].0, s));
            }
        } else {
            let used: i64 = mine()
                .filter(|(k, _)| goods.class(k.good()) == GoodClass::Item)
                .map(|(k, &a)| {
                    let ms = goods.max_stack(k.good()).max(1);
                    (a.max(0) + ms - 1) / ms
                })
                .sum();
            let free = (st.cap - used).max(0);
            let w: Vec<f64> = fits
                .iter()
                .map(|&(k, r)| r as f64 / goods.max_stack(k.good()).max(1) as f64)
                .collect();
            // A good with no part-filled stack takes a slot of its own with its first unit, so
            // each has one first, the heaviest first while they last; the rest split by rate. By
            // rate alone a rare byproduct's share floors to no slot and the store reads full at
            // once (choice G2-26).
            let mut base = vec![0i64; fits.len()];
            let mut left = free;
            let mut order: Vec<usize> = (0..fits.len()).collect();
            order.sort_by(|&a, &b| w[b].total_cmp(&w[a]).then(a.cmp(&b)));
            for j in order {
                if left == 0 {
                    break;
                }
                let k = fits[j].0;
                let ms = goods.max_stack(k.good()).max(1);
                if level(lv, k).max(0) % ms == 0 {
                    base[j] = 1;
                    left -= 1;
                }
            }
            for (j, s) in split(left, &w).into_iter().enumerate() {
                let k = fits[j].0;
                let ms = goods.max_stack(k.good()).max(1);
                let a = level(lv, k).max(0);
                let partial = if a % ms == 0 { 0 } else { ms - a % ms };
                out.push((k, partial + (base[j] + s) * ms));
            }
        }
        out
    }

    /// A controller's observed level: (sum, pools with their multiplicity: a store observed
    /// through two edges counts twice).
    fn observed(&self, comp: &Compiled, v: u32, lv: &Levels) -> Option<(i64, Vec<(PoolKey, i64)>)> {
        let graph = &comp.graph;
        let d = self.d;
        let mut sum: Option<i64> = None;
        let mut pools: BTreeMap<PoolKey, i64> = BTreeMap::new();
        for &e in graph.observe_in.of(v) {
            let ed = graph.edge(e);
            let from = graph.node(ed.from);
            if from.kind != NodeKind::Store {
                continue;
            }
            let Some(inv) = from.inventory else {
                continue;
            };
            let range = PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX));
            if self.stores.contains_key(&inv) {
                let s: i64 = lv
                    .range(range.clone())
                    .filter(|(k, _)| graph.allows(e, k.good()))
                    .map(|(_, &a)| a)
                    .sum();
                sum = Some(sum.unwrap_or(0) + s);
                let keys: BTreeSet<PoolKey> = lv
                    .range(range.clone())
                    .map(|(k, _)| *k)
                    .chain(self.nets.range(range).map(|(k, _)| *k))
                    .filter(|k| graph.allows(e, k.good()))
                    .collect();
                for k in keys {
                    *pools.entry(k).or_insert(0) += 1;
                }
            } else if let Some(c) = self.fixed.get(&inv) {
                let s: i64 = c
                    .iter()
                    .filter(|&(g, _)| graph.allows(e, g))
                    .map(|(_, a)| a)
                    .sum();
                sum = Some(sum.unwrap_or(0) + s);
            } else {
                sum = Some(sum.unwrap_or(0));
            }
        }
        sum.map(|s| (s, pools.into_iter().collect()))
    }

    /// Brings the flags to a fixed point at these levels, then builds the process and bounds.
    #[allow(clippy::too_many_arguments)]
    fn build(
        &mut self,
        comp: &mut Compiled,
        lv: &Levels,
        at: Tick,
        fired: Option<PoolKey>,
        room_or_heater: bool,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(ProcSpec, Vec<(u32, SpaceSpec)>), Fallback> {
        let d = self.d;
        let n_ctrl = comp
            .graph
            .nodes
            .iter()
            .filter(|nd| nd.kind == NodeKind::Controller)
            .count();
        // Holds that came due: those nodes run now. Their potential did not change, so they are
        // not held again.
        self.st.hold.retain(|_, t| *t > at);
        self.flips.clear();
        let cap = 2 * (lv.len() + self.stores.len() + n_ctrl) + 8 + comp.graph.nodes.len();
        let mut settled = false;
        let mut sol = Solution::default();
        let mut potential = Vec::new();
        // The solves this regime change may run past the regime cache (choice G2-61): each is
        // bounded (`glass.solveWorkMax`), and so is their number.
        let solves0 = comp.solves;
        for _ in 0..cap {
            if comp.solves.saturating_sub(solves0) >= tun.build_solves_max {
                return Err(Fallback::Unsettled);
            }
            self.coupled = self.coupling(lv);
            let rg = self.regime(comp, lv);
            sol = if self.st.hold.is_empty() {
                comp.solve_regime(&rg, tun)
            } else {
                let mut held = rg.clone();
                for &v in self.st.hold.keys() {
                    held.gate_off[v as usize] = true;
                }
                comp.solve_regime(&held, tun)
            };
            self.nets = self.book(comp, &sol, self.coupled);
            if self.settle_once(comp, lv, goods, tun) {
                continue;
            }
            // Settled: what this regime runs with nothing held. A node whose potential rose waits
            // its path latency; the start too, since a belt may have been empty at the unload
            // (G-B2's ceiling).
            potential = if self.st.hold.is_empty() {
                node_flows(&comp.graph, &sol)
            } else {
                let free = comp.solve_regime(&rg, tun);
                node_flows(&comp.graph, &free)
            };
            if self.hold_rises(comp, &potential, at) {
                continue;
            }
            settled = true;
            break;
        }
        // A solve that did not settle books flows that do not balance at its nodes (a loop whose
        // only exit is full grows round by round): goods would leave the stores and arrive
        // nowhere (choice G2-27).
        if !settled || !sol.converged {
            return Err(Fallback::Unsettled);
        }
        self.counts.regimes += 1;
        comp.replay_regimes += 1;
        self.flow = potential;
        self.fed = self
            .heaters
            .iter()
            .map(|b| sol.fed.get(b.node as usize).copied().unwrap_or(0.0))
            .collect();
        // Watches.
        let mut watches: Vec<(Watch, SpaceSpec)> = Vec::new();
        let nets = self.nets.clone();
        let net = |k: PoolKey| nets.get(&k).copied().unwrap_or(0);
        for &inv in self.stores.keys() {
            if self.st.full.contains_key(&inv) {
                continue;
            }
            let arriving: Vec<(PoolKey, i64)> = nets
                .range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)))
                .filter(|&(_, &r)| r > 0)
                .map(|(&k, &r)| (k, r))
                .collect();
            if arriving.is_empty() {
                continue;
            }
            for (k, s) in self.shares(inv, lv, &arriving, goods) {
                if s >= i64::MAX / 8 {
                    continue;
                }
                watches.push((
                    Watch::Room { inv, key: k },
                    (vec![(k, 1)], 0, level(lv, k) + s),
                ));
            }
        }
        for (&(inv, g), &l0) in &self.st.empty {
            let k = PoolKey::Store(d, inv, g);
            if net(k) > 0 {
                let h = self.hold(comp, inv, g, goods, tun);
                watches.push((Watch::Refill { key: k }, (vec![(k, 1)], 0, l0 + h - 1)));
            }
        }
        for (&inv, flv) in &self.st.full {
            for (&k, &r) in
                nets.range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)))
            {
                if r >= 0 {
                    continue;
                }
                let g = k.good();
                let step = self.step_of(inv, g, goods);
                let f = flv.get(&g).copied().unwrap_or_else(|| level(lv, k));
                watches.push((
                    Watch::Frees { inv, key: k },
                    (vec![(k, -1)], 0, step - 1 - f),
                ));
            }
        }
        for v in 0..comp.graph.nodes.len() as u32 {
            let node = comp.graph.node(v);
            if node.kind != NodeKind::Controller || !self.mine[v as usize] {
                continue;
            }
            let Some((sum, pools)) = self.observed(comp, v, lv) else {
                continue;
            };
            let on = self.st.ctrl_on[v as usize];
            let (dist, sign) = if on {
                (sum - node.params[1], -1)
            } else {
                (node.params[0] - sum, 1)
            };
            let moving: Vec<(PoolKey, i64, i64)> = pools
                .iter()
                .filter(|&&(k, _)| net(k) * sign > 0)
                .map(|&(k, m)| (k, m, net(k).abs()))
                .collect();
            if moving.is_empty() || dist <= 0 {
                continue;
            }
            // Each pool first gets a tick's move (the settle switched the controller if the
            // distance were less), the rest by rate, so no share fires before a tick runs.
            let base: Vec<i64> = moving.iter().map(|&(_, m, r)| m * ctrl_reach(r)).collect();
            let rest = (dist - base.iter().sum::<i64>()).max(0);
            let w: Vec<f64> = moving
                .iter()
                .map(|&(_, m, r)| (r as f64) * m as f64)
                .collect();
            for (j, s) in split(rest, &w).into_iter().enumerate() {
                let s = s + base[j];
                let (k, m, _) = moving[j];
                let room = (s + m - 1) / m - 1;
                let l = level(lv, k);
                let spec = if sign > 0 {
                    (vec![(k, 1)], 0, l + room)
                } else {
                    (vec![(k, -1)], 0, room - l)
                };
                watches.push((Watch::Switch { node: v, key: k }, spec));
            }
        }
        for &g in &self.st.dry {
            let k = PoolKey::Stock(d, g);
            if net(k) > 0 {
                let h = holding_threshold(goods, g, tun.holding_stacks, true).max(1);
                watches.push((Watch::Undry { good: g }, (vec![(k, 1)], 0, h - 1)));
            }
        }
        for (&k, &r) in &nets {
            let PoolKey::Stock(_, g) = k else {
                continue;
            };
            if r <= 0 || self.st.sealed.contains(&g) {
                continue;
            }
            if let Some(top) = self.seal_at(comp, g, goods) {
                watches.push((Watch::Seal { good: g }, (vec![(k, 1)], 0, top)));
            }
        }
        self.watches = watches;
        // The deadline: the next held node's rise or heater phase.
        let mut deadline: Option<Tick> = None;
        let mut soon = |t: Tick| {
            if t > at {
                deadline = Some(deadline.map_or(t, |x: Tick| x.min(t)));
            }
        };
        for &t in self.st.hold.values() {
            soon(t);
        }
        for (h, b) in self.heaters.iter().enumerate() {
            if let Some(u) = b.until(self.fed[h]) {
                soon(at + (u.ceil().max(1.0) as u64));
            }
        }
        let proc = ProcSpec {
            id: self.id,
            body: ProcSpecBody::Box(
                nets.iter()
                    .filter(|&(_, &r)| r != 0)
                    .map(|(&k, &r)| (k, Diffuser::from_fixed(r)))
                    .collect(),
            ),
            targets: Vec::new(),
            requires: None,
            latency: 0,
            enabled: true,
            deadline,
            min_run: 1,
        };
        let bounds: Vec<(u32, SpaceSpec)> = self
            .watches
            .iter()
            .enumerate()
            .map(|(i, (_, s))| (bound_tag(self.id, i as u32 + 1), s.clone()))
            .collect();
        self.mark(comp, lv, at, fired, room_or_heater);
        Ok((proc, bounds))
    }

    /// A store's unit of freed room: a stack for items, a bucket for fluids.
    fn step_of(&self, inv: u32, g: Good, goods: &dyn GoodLookup) -> i64 {
        if self.stores.get(&inv).is_some_and(|s| s.fluid) || goods.class(g) == GoodClass::Fluid {
            1000
        } else {
            goods.max_stack(g).max(1)
        }
    }

    /// One pass over the flags at these levels and nets; true if any changed. Each test looks one
    /// tick ahead (a bound the process could not run a tick under is a crossing now).
    fn settle_once(
        &mut self,
        comp: &Compiled,
        lv: &Levels,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> bool {
        let d = self.d;
        let net = |k: PoolKey| self.nets.get(&k).copied().unwrap_or(0);
        let mut changed = false;
        // (a) Refills: a good flagged empty that reached its holding threshold, or one that holds
        // stock while nothing arrives (choice G2-19): a chest a funnel kept nearly empty is not a
        // source while goods pass through it, but what is left in it drains once they stop. Not
        // a hub's (choice G2-22): its other parts drain what is left, which this part cannot see.
        let mut unflag = Vec::new();
        for (&(inv, g), &l0) in &self.st.empty {
            let k = PoolKey::Store(d, inv, g);
            let r = net(k);
            let ahead = if r > 0 { reach(r) } else { 0 };
            if level(lv, k) + ahead >= l0 + self.hold(comp, inv, g, goods, tun)
                || (r <= 0 && level(lv, k) >= 1 && !self.hubs.contains_key(&inv))
            {
                unflag.push((inv, g));
            }
        }
        for x in unflag {
            self.st.empty.remove(&x);
            flip_inv(&mut self.flips, &self.unit_inv, x.0);
            changed = true;
        }
        // (b) Empties: a drained pool that cannot run a tick of its drain. A pool nothing drains
        // (an output store at 0) is not flagged: no watch restarts its carry (choice G2-12).
        let keys: Vec<PoolKey> = lv.keys().chain(self.nets.keys()).copied().collect();
        for k in keys {
            match k {
                PoolKey::Store(_, inv, g) => {
                    if self.st.empty.contains_key(&(inv, g)) || !self.stores.contains_key(&inv) {
                        continue;
                    }
                    let l = level(lv, k);
                    let r = net(k);
                    if r < 0 && l < reach(r) {
                        self.st.empty.insert((inv, g), l.max(0));
                        flip_inv(&mut self.flips, &self.unit_inv, inv);
                        changed = true;
                    }
                }
                PoolKey::Stock(_, g) => {
                    let r = net(k);
                    if r < 0 && !self.st.dry.contains(&g) && level(lv, k) < reach(r) {
                        self.st.dry.insert(g);
                        changed = true;
                    }
                }
                PoolKey::Mail(..) => {}
            }
        }
        // (c) Full: a store an arriving good has no room in for a tick.
        let mut newly_full = Vec::new();
        for &inv in self.stores.keys() {
            if self.st.full.contains_key(&inv) {
                continue;
            }
            let arriving: Vec<(PoolKey, i64)> = self
                .nets
                .range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)))
                .filter(|&(_, &r)| r > 0)
                .map(|(&k, &r)| (k, r))
                .collect();
            if arriving.is_empty() {
                continue;
            }
            let sh = self.shares(inv, lv, &arriving, goods);
            if sh.iter().any(|&(k, s)| s < reach(net(k))) {
                newly_full.push(inv);
            }
        }
        for inv in newly_full {
            let flv: BTreeMap<Good, i64> = lv
                .range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)))
                .map(|(k, &a)| (k.good(), a))
                .collect();
            self.st.full.insert(inv, flv);
            flip_inv(&mut self.flips, &self.unit_inv, inv);
            changed = true;
        }
        // (d) Frees: a full store whose good drained a stack (a bucket).
        let mut freed = Vec::new();
        for (&inv, flv) in &self.st.full {
            let gone = lv
                .range(PoolKey::Store(d, inv, Good(0))..=PoolKey::Store(d, inv, Good(u32::MAX)))
                .any(|(&k, &a)| {
                    let r = net(k);
                    let ahead = if r < 0 { reach(r) } else { 0 };
                    let f = flv.get(&k.good()).copied().unwrap_or(a);
                    a - ahead <= f - self.step_of(inv, k.good(), goods)
                })
                || flv.iter().any(|(&g, &f)| {
                    !lv.contains_key(&PoolKey::Store(d, inv, g)) && f >= self.step_of(inv, g, goods)
                });
            if gone {
                freed.push(inv);
            }
        }
        for inv in freed {
            self.st.full.remove(&inv);
            flip_inv(&mut self.flips, &self.unit_inv, inv);
            changed = true;
        }
        // (e) Controllers: hysteresis on the observed level, a tick ahead.
        for v in 0..comp.graph.nodes.len() as u32 {
            let node = comp.graph.node(v);
            if node.kind != NodeKind::Controller || !self.mine[v as usize] {
                continue;
            }
            let Some((sum, pools)) = self.observed(comp, v, lv) else {
                continue;
            };
            let on = self.st.ctrl_on[v as usize];
            let (dist, sign) = if on {
                (sum - node.params[1], -1)
            } else {
                (node.params[0] - sum, 1)
            };
            let ahead: i64 = pools
                .iter()
                .filter(|&&(k, _)| net(k) * sign > 0)
                .map(|&(k, m)| m * ctrl_reach(net(k)))
                .sum();
            if dist <= 0 || (ahead > 0 && dist <= ahead) {
                self.st.ctrl_on[v as usize] = !on;
                if let Some(&u) = self.unit_of.get(v as usize).filter(|&&u| u != NONE) {
                    *self.flips.entry(u).or_insert(0) += 1;
                }
                changed = true;
            }
        }
        // (f) Dry cut goods back at a holding threshold.
        let mut undry = Vec::new();
        for &g in &self.st.dry {
            let k = PoolKey::Stock(d, g);
            let r = net(k);
            let ahead = if r > 0 { reach(r) } else { 0 };
            if level(lv, k) + ahead >= holding_threshold(goods, g, tun.holding_stacks, true).max(1)
            {
                undry.push(g);
            }
        }
        for g in undry {
            self.st.dry.remove(&g);
            changed = true;
        }
        // (g) Seals: a cut good the simulated side sends that cannot run a tick more into its live
        // receivers' room (choice G2-66). A seal holds to the end of the replay.
        let mut seal = Vec::new();
        for (&k, &r) in &self.nets {
            let PoolKey::Stock(_, g) = k else {
                continue;
            };
            if r <= 0 || self.st.sealed.contains(&g) {
                continue;
            }
            if let Some(top) = self.seal_at(comp, g, goods)
                && top - level(lv, k) < reach(r)
            {
                seal.push(g);
            }
        }
        for g in seal {
            self.st.sealed.insert(g);
            changed = true;
        }
        changed
    }

    /// The `Stock` level a good the simulated side sends across the cut may reach (choice G2-66):
    /// its start level plus the room its live receivers had at the start. A store's room is its
    /// free slots (or millibuckets) for the good; a machine, belt or other node holds nothing a
    /// reload could place. None: no cut edge from the simulated side lets the good through, or a
    /// receiving store's capacity is unknown (no bound, as a covered store of unknown capacity).
    fn seal_at(&self, comp: &Compiled, g: Good, goods: &dyn GoodLookup) -> Option<i64> {
        let graph = &comp.graph;
        let mut any = false;
        let mut seen = BTreeSet::new();
        let mut room = 0i64;
        for &(e, from_sim) in &self.cut {
            if !from_sim || !graph.allows(e, g) {
                continue;
            }
            any = true;
            let node = graph.node(graph.edge(e).to);
            if node.kind != NodeKind::Store {
                continue;
            }
            let inv = node.inventory?;
            if !seen.insert(inv) {
                continue;
            }
            let &(slots, mb) = self.live_caps.get(&inv)?;
            let none = GoodVec::new();
            let c = self.fixed.get(&inv).unwrap_or(&none);
            room += if goods.class(g) == GoodClass::Fluid {
                if mb <= 0 {
                    return None;
                }
                let used: i64 = c
                    .iter()
                    .filter(|&(h, _)| goods.class(h) == GoodClass::Fluid)
                    .map(|(_, a)| a.max(0))
                    .sum();
                (mb - used).max(0)
            } else {
                if slots <= 0 {
                    return None;
                }
                let used: i64 = c
                    .iter()
                    .filter(|&(h, _)| goods.class(h) == GoodClass::Item)
                    .map(|(h, a)| {
                        let ms = goods.max_stack(h).max(1);
                        (a.max(0) + ms - 1) / ms
                    })
                    .sum();
                let ms = goods.max_stack(g).max(1);
                let a = c.get(g).max(0);
                let partial = if a % ms == 0 { 0 } else { ms - a % ms };
                (slots - used).max(0) * ms + partial
            };
        }
        let base = self.stock0.get(&g).copied().unwrap_or(0);
        any.then(|| base.saturating_add(room))
    }

    /// Spends the heaters' burn to `at`; true if a phase changed.
    fn advance_heaters(&mut self, at: Tick) -> bool {
        if at <= self.heat_at {
            return false;
        }
        let dt = (at.0 - self.heat_at.0) as f64;
        let mut changed = false;
        for (h, b) in self.heaters.iter_mut().enumerate() {
            let before = b.phase();
            advance_burn(b, self.fed[h], dt);
            changed |= b.phase() != before;
        }
        self.heat_at = at;
        changed
    }

    // ---- cycles (G-B9) --------------------------------------------------------------------

    fn sig(&self, lv: &Levels, at: Tick) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (&k, &r) in &self.nets {
            mix_key(&mut h, k);
            mix(&mut h, r as u64);
        }
        mix(&mut h, 0xA1);
        for (&(i, g), &l0) in &self.st.empty {
            mix(&mut h, u64::from(i));
            mix(&mut h, u64::from(g.0));
            mix(&mut h, l0 as u64);
        }
        mix(&mut h, 0xA2);
        for (&i, f) in &self.st.full {
            mix(&mut h, u64::from(i));
            for (&g, &a) in f {
                mix(&mut h, u64::from(g.0));
                mix(&mut h, a as u64);
            }
        }
        mix(&mut h, 0xA3);
        for &on in &self.st.ctrl_on {
            mix(&mut h, u64::from(on));
        }
        for g in &self.st.dry {
            mix(&mut h, u64::from(g.0));
        }
        mix(&mut h, 0xA6);
        for g in &self.st.sealed {
            mix(&mut h, u64::from(g.0));
        }
        mix(&mut h, 0xA4);
        for (w, _) in &self.watches {
            if matches!(w, Watch::Room { .. }) {
                continue;
            }
            let k = w.key(self.d);
            mix_key(&mut h, k);
            mix(&mut h, level(lv, k) as u64);
        }
        mix(&mut h, 0xA5);
        for b in &self.heaters {
            mix(&mut h, u64::from(b.phase()));
        }
        mix(&mut h, u64::from(self.coupled));
        for (&v, &t) in &self.st.hold {
            mix(&mut h, u64::from(v));
            mix(&mut h, t.0.saturating_sub(at.0));
        }
        h
    }

    fn mark(
        &mut self,
        _comp: &Compiled,
        lv: &Levels,
        at: Tick,
        fired: Option<PoolKey>,
        room: bool,
    ) {
        let sig = self.sig(lv, at);
        let watched = self
            .watches
            .iter()
            .filter(|(w, _)| !matches!(w, Watch::Room { .. }))
            .map(|(w, _)| w.key(self.d))
            .collect();
        if self.marks.len() >= MARKS_KEPT {
            self.marks.remove(0);
        }
        self.marks.push(Mark {
            at,
            sig,
            levels: lv.clone(),
            nets: self.nets.clone(),
            state: self.st.clone(),
            watched,
            fired,
            room,
            heater: false,
            fire: self.fire,
            fed: self.fed.clone(),
        });
    }

    /// Looks for a period ending at the newest mark; installs it as a cycle when one fits.
    fn detect(
        &mut self,
        at: Tick,
        carries: &BTreeMap<PoolKey, u32>,
        goods: &dyn GoodLookup,
    ) -> Option<(ProcSpec, Tick)> {
        let j = self.marks.len().checked_sub(1)?;
        let mut tries = 0;
        for i in (0..j).rev() {
            if self.marks[i].sig != self.marks[j].sig {
                continue;
            }
            tries += 1;
            if tries > CYCLE_TRIES {
                break;
            }
            if let Some(found) = self.try_cycle(i, j, at, carries, goods) {
                return Some(found);
            }
        }
        None
    }

    fn try_cycle(
        &mut self,
        i: usize,
        j: usize,
        at: Tick,
        carries: &BTreeMap<PoolKey, u32>,
        goods: &dyn GoodLookup,
    ) -> Option<(ProcSpec, Tick)> {
        let (mi, mj) = (&self.marks[i], &self.marks[j]);
        let period = mj.at.0.checked_sub(mi.at.0)?;
        if period == 0 || mi.fire != mj.fire || mj.fire != self.fire {
            return None;
        }
        let ms = &self.marks[i..=j];
        // Every breakpoint inside must repeat: no room re-split, no heater phase.
        if ms[1..].iter().any(|m| m.room || m.heater) {
            return None;
        }
        let keys: BTreeSet<PoolKey> = ms
            .iter()
            .flat_map(|m| m.levels.keys().chain(m.nets.keys()))
            .copied()
            .collect();
        let drift = |k: PoolKey| level(&mj.levels, k) - level(&mi.levels, k);
        // Every pool that decides a breakpoint returns to where it was.
        for m in ms {
            for &k in m.watched.iter().chain(m.fired.iter()) {
                if drift(k) != 0 {
                    return None;
                }
            }
        }
        // Heaters: the burn per period, and how many periods keep every phase.
        let mut spend = vec![0.0; self.heaters.len()];
        for w in ms.windows(2) {
            let len = (w[1].at.0 - w[0].at.0) as f64;
            for (h, b) in self.heaters.iter().enumerate() {
                spend[h] += b.spend(w[0].fed.get(h).copied().unwrap_or(0.0)) * len;
            }
        }
        let mut n_max: u64 = CYCLE_TICKS_MAX / period;
        for (h, b) in self.heaters.iter().enumerate() {
            if spend[h] <= 0.0 {
                continue;
            }
            let room = match b.phase() {
                burn::REFUSING => b.left - b.thr,
                burn::TAKING => b.left,
                _ => continue,
            };
            let n = (room / spend[h]).floor() - 1.0;
            if n < 2.0 {
                return None;
            }
            n_max = n_max.min(n as u64);
        }
        // The cycle's pools and pieces. A piece of no length (two breakpoints at one tick) passes
        // its restarts to the next.
        let pools: Vec<PoolKey> = keys
            .iter()
            .copied()
            .filter(|k| {
                ms[..ms.len() - 1]
                    .iter()
                    .any(|m| m.nets.get(k).is_some_and(|&r| r != 0))
            })
            .collect();
        if pools.is_empty() {
            return None;
        }
        let index: BTreeMap<PoolKey, usize> =
            pools.iter().enumerate().map(|(j, &k)| (k, j)).collect();
        let mut pieces: Vec<CyclePiece> = Vec::new();
        let mut pending: Vec<CycleFire> = Vec::new();
        for w in ms.windows(2) {
            if let Some((k, canon)) = w[0].fire {
                let &j = index.get(&k)?;
                pending.push(CycleFire {
                    pool: j as u16,
                    canon,
                });
            }
            let len = w[1].at.0 - w[0].at.0;
            if len == 0 {
                continue;
            }
            pieces.push(CyclePiece {
                len,
                rates: pools
                    .iter()
                    .map(|k| w[0].nets.get(k).copied().unwrap_or(0))
                    .collect(),
                fires: std::mem::take(&mut pending),
            });
        }
        // Breakpoints at the period's last instant come before the first piece's.
        let first = pieces.first_mut()?;
        if !pending.is_empty() {
            pending.append(&mut first.fires);
            first.fires = pending;
        }
        // The cycle from the carries as they are: every fired pool must come back to its carry
        // after one period (so every period is the first one), and every pool a breakpoint
        // decides must come back to its level; a decided pool no breakpoint shifts must come back
        // to its carry too.
        let m = pools.len();
        let c0: Vec<u32> = pools
            .iter()
            .map(|k| carries.get(k).copied().unwrap_or(0))
            .collect();
        let (e1, c1) = cycle_period(&pieces, &c0);
        let fired = cycle_fired(&pieces, m);
        if (0..m).any(|j| fired[j] && c1[j] != c0[j]) {
            return None;
        }
        let decides: BTreeSet<PoolKey> = ms
            .iter()
            .flat_map(|m| m.watched.iter().chain(m.fired.iter()))
            .copied()
            .collect();
        for (j, k) in pools.iter().enumerate() {
            if decides.contains(k) && (e1[j] != 0 || (!fired[j] && c1[j] != c0[j])) {
                return None;
            }
        }
        // Per pool over n periods: n times a period's move (exact; a unit either side covers the
        // ticks inside a period).
        let change = |k: PoolKey, n: u64| -> (i64, i64) {
            let Some(&j) = index.get(&k) else {
                return (0, 0);
            };
            let n128 = i128::from(n);
            let x = if fired[j] {
                e1[j].saturating_mul(n128)
            } else {
                let mv = e1[j] * U + i128::from(c1[j]) - i128::from(c0[j]);
                (i128::from(c0[j]) + mv.saturating_mul(n128)).div_euclid(U)
            };
            (
                i64::try_from(x - 1).unwrap_or(i64::MIN / 4),
                i64::try_from(x + 1).unwrap_or(i64::MAX / 4),
            )
        };
        let tick_max: i64 = ms
            .iter()
            .flat_map(|m| m.nets.values())
            .map(|&r| reach(r))
            .max()
            .unwrap_or(1);
        // A fired pool's path repeats exactly every period (its carry comes back), so the period
        // it was found from bounds it; the others need a margin for their drift.
        let exact = |k: PoolKey| index.get(&k).is_some_and(|&j| fired[j]);
        let check = |n: u64| -> bool {
            for &k in &keys {
                if exact(k) {
                    continue;
                }
                let mut lo = ms.iter().map(|m| level(&m.levels, k)).min().unwrap_or(0);
                // A hub's other parts drain it too: this cycle may count on its share.
                if let PoolKey::Store(_, inv, _) = k
                    && let Some(&n) = self.hubs.get(&inv)
                {
                    lo /= i64::from(n.max(1));
                }
                let drains = ms.iter().any(|m| m.nets.get(&k).is_some_and(|&r| r < 0));
                if drains && lo + change(k, n).0.min(0) < CYCLE_MARGIN {
                    return false;
                }
            }
            for (&inv, st) in &self.stores {
                if st.cap <= 0 {
                    continue;
                }
                let mine: Vec<PoolKey> = keys
                    .iter()
                    .copied()
                    .filter(|k| matches!(k, PoolKey::Store(_, i, _) if *i == inv))
                    .collect();
                if !mine.iter().any(|&k| change(k, n).1 > 1) {
                    continue;
                }
                let hi = |k: PoolKey| {
                    let top = ms.iter().map(|m| level(&m.levels, k)).max().unwrap_or(0);
                    if exact(k) {
                        top
                    } else {
                        top + change(k, n).1.max(0) + CYCLE_MARGIN + tick_max
                    }
                };
                let ok = if st.fluid {
                    mine.iter()
                        .filter(|k| goods.class(k.good()) == GoodClass::Fluid)
                        .map(|&k| hi(k))
                        .sum::<i64>()
                        <= st.cap
                } else {
                    mine.iter()
                        .filter(|k| goods.class(k.good()) == GoodClass::Item)
                        .map(|&k| {
                            let s = goods.max_stack(k.good()).max(1);
                            (hi(k) + s - 1) / s
                        })
                        .sum::<i64>()
                        <= st.cap
                };
                if !ok {
                    return false;
                }
            }
            true
        };
        if n_max < 2 || !check(2) {
            return None;
        }
        let (mut lo, mut hi) = (2u64, n_max);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if check(mid) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let n = lo;
        let run = CycleRun {
            start: at,
            period,
            marks: ms.to_vec(),
            spend,
        };
        let proc = ProcSpec {
            id: self.id,
            body: ProcSpecBody::Cycle { pools, pieces },
            targets: Vec::new(),
            requires: None,
            latency: 0,
            enabled: true,
            deadline: None,
            min_run: 1,
        };
        self.cycle = Some(run);
        self.counts.cycles += 1;
        self.watches.clear();
        Some((proc, at + n * period))
    }

    /// The cycle ended at its deadline: whole periods ran; the state is its first mark's.
    fn end_cycle(&mut self, at: Tick) {
        let Some(run) = self.cycle.take() else {
            return;
        };
        let periods = (at.0 - run.start.0) / run.period;
        for (h, b) in self.heaters.iter_mut().enumerate() {
            let floor = if b.phase() == burn::REFUSING {
                b.thr
            } else {
                0.0
            };
            b.left = (b.left - run.spend[h] * periods as f64).max(floor);
        }
        self.heat_at = at;
        let first = &run.marks[0];
        let shift = at.0 - first.at.0;
        self.restore(&first.state.clone(), shift, at);
        self.steps = 0;
        self.marks.clear();
    }

    /// Leaves a running cycle mid-way (an outside event, a hub's envelope): the state of the
    /// piece it is in. Returns the whole periods it ran.
    fn leave_cycle(&mut self, at: Tick) -> u64 {
        let Some(run) = self.cycle.take() else {
            return 0;
        };
        let el = at.0 - run.start.0;
        let periods = el / run.period;
        let mut off = el % run.period;
        let mut k = 0;
        let ms = &run.marks;
        while k + 2 < ms.len() {
            let len = ms[k + 1].at.0 - ms[k].at.0;
            if off < len {
                break;
            }
            off -= len;
            k += 1;
        }
        for (h, b) in self.heaters.iter_mut().enumerate() {
            let mut spent = run.spend[h] * periods as f64;
            for w in ms[..=k].windows(2) {
                let f = w[0].fed.get(h).copied().unwrap_or(0.0);
                spent += b.spend(f) * (w[1].at.0 - w[0].at.0) as f64;
            }
            spent += b.spend(ms[k].fed.get(h).copied().unwrap_or(0.0)) * off as f64;
            let floor = if b.phase() == burn::REFUSING {
                b.thr
            } else {
                0.0
            };
            b.left = (b.left - spent).max(floor);
        }
        self.heat_at = at;
        let m = &ms[k];
        // Mark k's moment, now: `off` ticks into its piece.
        let shift = at.0 - off - m.at.0;
        let s = m.state.clone();
        self.restore(&s, shift, at);
        periods
    }

    /// The flags of a mark, its held nodes' rises moved forward by `shift` ticks.
    fn restore(&mut self, s: &State, shift: u64, _at: Tick) {
        let mut st = s.clone();
        for t in st.hold.values_mut() {
            *t += shift;
        }
        self.st = st;
    }
}

// ---- the glass page (page.rs, choice G2-80) -------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    persist_unit_enum!(
        Fallback,
        [
            Fallback::Unsettled,
            Fallback::Oscillating,
            Fallback::Stuck,
            Fallback::Unexpected,
        ]
    );
    persist_struct!(ReplayCounts {
        regimes,
        breakpoints,
        cycles,
        freezes
    });
    persist_struct!(StoreInfo { fluid, cap, nodes });

    impl Persist for Watch {
        fn put(&self, w: &mut Writer) {
            match *self {
                Watch::Room { inv, key } => {
                    w.u8(0);
                    inv.put(w);
                    key.put(w);
                }
                Watch::Refill { key } => {
                    w.u8(1);
                    key.put(w);
                }
                Watch::Frees { inv, key } => {
                    w.u8(2);
                    inv.put(w);
                    key.put(w);
                }
                Watch::Switch { node, key } => {
                    w.u8(3);
                    node.put(w);
                    key.put(w);
                }
                Watch::Undry { good } => {
                    w.u8(4);
                    good.put(w);
                }
                Watch::Seal { good } => {
                    w.u8(5);
                    good.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Watch::Room {
                    inv: Persist::get(r)?,
                    key: Persist::get(r)?,
                },
                1 => Watch::Refill {
                    key: Persist::get(r)?,
                },
                2 => Watch::Frees {
                    inv: Persist::get(r)?,
                    key: Persist::get(r)?,
                },
                3 => Watch::Switch {
                    node: Persist::get(r)?,
                    key: Persist::get(r)?,
                },
                4 => Watch::Undry {
                    good: Persist::get(r)?,
                },
                5 => Watch::Seal {
                    good: Persist::get(r)?,
                },
                _ => return invalid("glass watch"),
            })
        }
    }

    persist_struct!(State {
        empty,
        full,
        ctrl_on,
        dry,
        sealed,
        hold
    });
    persist_struct!(Mark {
        at,
        sig,
        levels,
        nets,
        state,
        watched,
        fired,
        room,
        heater,
        fire,
        fed
    });
    persist_struct!(CycleRun {
        start,
        period,
        marks,
        spend
    });
    persist_struct!(Part {
        id,
        mine,
        hubs,
        d,
        compile,
        live,
        sim,
        inv_of,
        stores,
        fixed,
        cut,
        live_caps,
        stock0,
        st,
        heaters,
        heat_at,
        fed,
        flow,
        nets,
        watches,
        fire,
        built_at,
        residual,
        coupled,
        learned_sources,
        marks,
        cycle,
        steps,
        counts,
        unit_of,
        unit_inv,
        gated,
        hits,
        flips
    });
    /// A replay has at least one part ([`Replay::compile`] reads the first).
    impl Persist for Replay {
        fn put(&self, w: &mut Writer) {
            self.started.put(w);
            self.parts.put(w);
            self.covered.put(w);
            self.replays.put(w);
            self.frozen.put(w);
            self.freezes.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            let rp = Replay {
                started: Persist::get(r)?,
                parts: Persist::get(r)?,
                covered: Persist::get(r)?,
                replays: Persist::get(r)?,
                frozen: Persist::get(r)?,
                freezes: Persist::get(r)?,
            };
            if rp.parts.is_empty() {
                return invalid("glass replay without parts");
            }
            Ok(rp)
        }
    }
}
