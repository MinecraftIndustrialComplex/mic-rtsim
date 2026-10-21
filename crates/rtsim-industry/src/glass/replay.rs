//! The glass replay (GLASSBOX.md §4, phase G2): while a compiled, eligible district is away, it
//! replays on its graph one regime at a time.
//!
//! **Regimes.** A regime is the graph's flags (§4.1): which stores hold which goods, which have
//! room, which controllers are on, which heaters burn, which cut goods ran dry. Its solve gives
//! every goods edge's L flow; the replay books those per store (`PoolKey::Store`) and across a
//! partial load's cut (`PoolKey::Stock`, I7, G-B6) as one all-or-nothing Box process. Each flag a
//! level can change has a watch: a bound on one pool with weight ±1, which the cluster checks
//! exactly, so the process stops the tick before the flag changes; the replay re-solves (the
//! regime cache answers most of these) and replaces the process in place (choices G2-4, G2-10).
//!
//! **Time.** Heater phases and latency (§4.4) are timers, the process's deadline. A simulated
//! node that would run faster than it did is held off for its downstream latency (the time its
//! output takes to reach a store), then runs: what it makes arrives no earlier than in Create,
//! and every flow stays a solve's, so goods are conserved (G2-6). A fall is at once.
//!
//! **Cycles.** A regime sequence that repeats exactly (same flags, same watched levels, zero
//! drift on every pool that decides a breakpoint) becomes one `Body::Cycle` over as many whole
//! periods as provably change nothing else (G-B9, G2-13): a controller loop costs one process per
//! stretch, not two breakpoints per period.
//!
//! **Fallback.** A regime with no fixed point, a process that stays blocked, or more than
//! `glass.breakpointsPerEventMax` breakpoints between outside events ends the glass replay
//! (§4.5, G2-7); the engine falls back to the black box or freezes.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::conserve::{Diffuser, FIXED_ONE};

use super::check::{Burn, Compiled, gates, holding_threshold};
use super::q::Q;
use super::records::NodeKind;
use super::report;
use super::solve::{Regime, Solution, burn};
use crate::cluster::{
    Cluster, CycleFire, CyclePiece, MemberSpec, PoolKey, ProcSpec, ProcSpecBody, Run, SpaceSpec,
    Stop, cycle_fired, cycle_period,
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
}

/// Counters for the report (since the replay started).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplayCounts {
    pub regimes: u32,
    pub breakpoints: u32,
    pub cycles: u32,
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
}

impl Watch {
    fn key(&self, d: DistrictId) -> PoolKey {
        match *self {
            Watch::Room { key, .. }
            | Watch::Refill { key }
            | Watch::Frees { key, .. }
            | Watch::Switch { key, .. } => key,
            Watch::Undry { good } => PoolKey::Stock(d, good),
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

/// One district's glass replay.
#[derive(Clone, Debug)]
pub struct Replay {
    d: DistrictId,
    compile: u32,
    pub started: Tick,
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
    pub counts: ReplayCounts,
}

type Levels = BTreeMap<PoolKey, i64>;

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

impl Replay {
    /// Starts a replay: its member (pools at the engine's levels, the first regime's process and
    /// bounds) for a new cluster.
    pub fn start(
        comp: &mut Compiled,
        setup: Setup,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
        at: Tick,
    ) -> Result<(Replay, MemberSpec), Fallback> {
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
        let mut rp = Replay {
            d,
            compile: graph.end.compile,
            started: at,
            live,
            sim,
            inv_of,
            stores,
            fixed,
            cut,
            st: State {
                empty: BTreeMap::new(),
                full: BTreeMap::new(),
                ctrl_on: comp.ctrl_on.clone(),
                dry: BTreeSet::new(),
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
        rp.residual(comp, tun);
        let (procs, bounds) = rp.build(comp, &levels, at, None, false, goods, tun)?;
        let spec = MemberSpec {
            district: d,
            pools: levels.iter().map(|(&k, &a)| (k, a, None)).collect(),
            procs: vec![procs],
            space: None,
            bounds,
        };
        Ok((rp, spec))
    }

    pub fn compile(&self) -> u32 {
        self.compile
    }

    /// Per node: it ran in the world when this replay started.
    pub fn live(&self) -> &[bool] {
        &self.live
    }

    /// Goods edges between live and simulated nodes.
    pub fn cut_edges(&self) -> u32 {
        self.cut.len() as u32
    }

    /// Goods the residual node adds as learned sources (I9).
    pub fn learned_sources(&self) -> u32 {
        self.learned_sources
    }

    /// Is a closed-form cycle running?
    pub fn in_cycle(&self) -> bool {
        self.cycle.is_some()
    }

    /// The replay's process stopped (a breakpoint): change the regime and replace the process.
    #[allow(clippy::too_many_arguments)]
    pub fn on_stop(
        &mut self,
        comp: &mut Compiled,
        c: &mut Cluster,
        why: Stop,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        self.run(comp, c, Some(why), at, goods, tun)
    }

    /// Something outside the replay changed its pools or the district (an import, an export, a
    /// correction): rebuild the regime from the levels as they are.
    pub fn refresh(
        &mut self,
        comp: &mut Compiled,
        c: &mut Cluster,
        at: Tick,
        goods: &dyn GoodLookup,
        tun: &GlassTunables,
    ) -> Result<(), Fallback> {
        self.run(comp, c, None, at, goods, tun)
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
        }
        for _ in 0..MAX_STEPS {
            self.fire = None;
            if let Some(w) = why
                && at > self.built_at
            {
                let crossing = match w {
                    Stop::InputEmpty(k) => Some((k, false)),
                    Stop::Bound(_, tag) => self
                        .watches
                        .get((tag as usize).wrapping_sub(1))
                        .filter(|(watch, _)| !matches!(watch, Watch::Room { .. }))
                        .map(|(watch, spec)| {
                            let rising = spec.0.first().is_some_and(|&(_, w)| w > 0);
                            (watch.key(self.d), rising)
                        }),
                    _ => None,
                };
                if let Some((k, rising)) = crossing
                    && let Some(canon) = c.shift_back(self.d, k, rising)
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
                self.steps += 1;
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
                        let Some(&(watch, _)) = self.watches.get(tag as usize - 1) else {
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
            let carries = c.carries_of(self.d);
            if let Some((cp, deadline)) = self.detect(at, &carries, goods) {
                proc = cp;
                proc.deadline = Some(deadline);
                bounds = Vec::new();
            }
            c.replace_procs(self.d, vec![proc], bounds, at);
            self.built_at = at;
            match c.run_of(self.d, 0) {
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
            if !self.sim[a] && !self.sim[b] {
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

    /// The latency rule (§4.4, choice G2-6): a simulated node with a downstream latency that runs
    /// faster under `sol` than it did is held off until its output could have arrived. Holding it
    /// off is a solve of its own (the node gated), so every flow stays recipe-consistent, and what
    /// it makes arrives no earlier than in Create. True if a node was newly held.
    fn hold_rises(
        &mut self,
        comp: &Compiled,
        sol: &Solution,
        released: &BTreeSet<u32>,
        at: Tick,
    ) -> bool {
        let graph = &comp.graph;
        let now = node_flows(graph, sol);
        let mut added = false;
        for (v, &f) in now.iter().enumerate() {
            let lat = graph.down_latency.get(v).copied().unwrap_or(0);
            let id = v as u32;
            if lat == 0
                || !self.sim[v]
                || graph.node(id).kind == NodeKind::Store
                || released.contains(&id)
                || self.st.hold.contains_key(&id)
            {
                continue;
            }
            let old = self.flow.get(v).copied().unwrap_or(0.0);
            if f > old * (1.0 + 1e-9) + 1e-12 {
                self.st.hold.insert(id, at + lat);
                added = true;
            }
        }
        added
    }

    /// The residual node from the passing window (G-B13, §5.1): per good, what the model misses,
    /// shaded by the checksum's z, split over the covered stores by their own slopes.
    fn residual(&mut self, comp: &Compiled, tun: &GlassTunables) {
        self.residual.clear();
        self.learned_sources = 0;
        let Some(win) = comp.passed.as_ref() else {
            return;
        };
        let d = self.d;
        let z = tun.checksum_z;
        for gr in &win.goods {
            if gr.class == report::class::EXACT
                || !gr.measured.is_finite()
                || !gr.predicted.is_finite()
            {
                continue;
            }
            let mut m = gr.measured;
            if gr.class == report::class::BOUNDED && gr.lo.is_finite() && gr.hi.is_finite() {
                m = m.clamp(gr.lo.min(gr.hi), gr.hi.max(gr.lo));
            }
            let s = m.signum() * (m.abs() - z * gr.sigma.max(0.0)).max(0.0);
            let r = s - gr.predicted;
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
            for (i, w) in cands {
                let x = r * w / total;
                // Floor: a rise rounds down, a fall away from zero (never above).
                let f = (x * FIXED_ONE as f64).floor() as i64;
                if f != 0 {
                    *self
                        .residual
                        .entry(PoolKey::Store(d, i, gr.good))
                        .or_default() += f;
                }
            }
            if r > 0.0 && s > 0.0 {
                self.learned_sources += 1;
            }
        }
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
            for (j, s) in split(free, &w).into_iter().enumerate() {
                let k = fits[j].0;
                let ms = goods.max_stack(k.good()).max(1);
                let a = level(lv, k).max(0);
                let partial = if a % ms == 0 { 0 } else { ms - a % ms };
                out.push((k, partial + s * ms));
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
        // Holds that came due: those nodes rise now.
        let released: BTreeSet<u32> = self
            .st
            .hold
            .iter()
            .filter(|&(_, &t)| t <= at)
            .map(|(&v, _)| v)
            .collect();
        self.st.hold.retain(|_, t| *t > at);
        let cap = 2 * (lv.len() + self.stores.len() + n_ctrl) + 8 + comp.graph.nodes.len();
        let mut settled = false;
        let mut sol = Solution::default();
        for _ in 0..cap {
            self.coupled = self.coupling(lv);
            let mut rg = self.regime(comp, lv);
            for &v in self.st.hold.keys() {
                rg.gate_off[v as usize] = true;
            }
            sol = comp.solve_regime(&rg, tun);
            // The start too: a belt may have been empty at the unload (G-B2's ceiling).
            if self.hold_rises(comp, &sol, &released, at) {
                continue;
            }
            self.nets = self.book(comp, &sol, self.coupled);
            if !self.settle_once(comp, lv, goods, tun) {
                settled = true;
                break;
            }
        }
        if !settled {
            return Err(Fallback::Unsettled);
        }
        self.counts.regimes += 1;
        comp.replay_regimes += 1;
        self.flow = node_flows(&comp.graph, &sol);
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
            if node.kind != NodeKind::Controller {
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
            id: 0,
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
            .map(|(i, (_, s))| (i as u32 + 1, s.clone()))
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
        // source while goods pass through it, but what is left in it drains once they stop.
        let mut unflag = Vec::new();
        for (&(inv, g), &l0) in &self.st.empty {
            let k = PoolKey::Store(d, inv, g);
            let r = net(k);
            let ahead = if r > 0 { reach(r) } else { 0 };
            if level(lv, k) + ahead >= l0 + self.hold(comp, inv, g, goods, tun)
                || (r <= 0 && level(lv, k) >= 1)
            {
                unflag.push((inv, g));
            }
        }
        for x in unflag {
            self.st.empty.remove(&x);
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
            changed = true;
        }
        // (e) Controllers: hysteresis on the observed level, a tick ahead.
        for v in 0..comp.graph.nodes.len() as u32 {
            let node = comp.graph.node(v);
            if node.kind != NodeKind::Controller {
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
        changed
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
                let lo = ms.iter().map(|m| level(&m.levels, k)).min().unwrap_or(0);
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
            id: 0,
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

    /// Leaves a running cycle mid-way (an outside event): the state of the piece it is in.
    fn leave_cycle(&mut self, at: Tick) {
        let Some(run) = self.cycle.take() else {
            return;
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
