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

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_kernel::clock::Tick;

use super::graph::Graph;
use super::q::Q;
use super::records::{CONTROL_RUN_WHEN_ON, CONTROL_STOP_WHEN_ON, NodeKind, ProbeSet, model};
use super::report::{self, GoodReport, Report};
use super::solve::{self, Regime, Solution, burn};
use crate::district::Inventory;
use crate::ids::{DistrictId, Good, GoodClass, GoodVec, VecMap};
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
}

/// What a sweep sees of the district.
pub struct View<'a> {
    pub revision: u32,
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

impl Compiled {
    pub fn new(graph: Graph, at: Tick, tun: &GlassTunables, demoted: &BTreeSet<u32>) -> Compiled {
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

    /// The first solve, at acceptance, from what the engine knows of the inventories.
    pub fn start(&mut self, view: &View<'_>, goods: &dyn GoodLookup, tun: &GlassTunables) {
        self.read_state(view, goods, tun);
        self.resolve(tun, true);
    }

    /// Discards the open window; the next starts after a fresh warm-up.
    pub fn discard(&mut self, at: Tick, tun: &GlassTunables) {
        self.win = None;
        self.last_sweep = None;
        self.stock_est.clear();
        self.ready_at = self
            .ready_at
            .max(Tick(at.0 + tun.warmup_ticks + self.graph.longest_latency));
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
        let integrating = self.win.is_some() && self.last_sweep.is_some();
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

    /// Integrates `dt` ticks of the current regime into the open window.
    fn integrate(&mut self, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        let Some(win) = self.win.as_mut() else {
            return;
        };
        // What the stores could not give (they held too little), per good.
        let mut corr: BTreeMap<Good, f64> = BTreeMap::new();
        for &(inv, g, net) in &self.sol.per_store {
            let est = self.stock_est.entry((inv, g)).or_insert(0.0);
            let raw = net * dt;
            let clamped = raw.max(-*est);
            *est += clamped;
            win.stores.entry((inv, g)).or_default().0 += clamped;
            if clamped != raw {
                *corr.entry(g).or_insert(0.0) += clamped - raw;
            }
        }
        for gb in &self.sol.goods {
            let (_, cum) = win.goods.entry(gb.good).or_default();
            let c = corr.get(&gb.good).copied().unwrap_or(0.0);
            let (il, ol) = (gb.in_l.to_f64(), gb.out_l.to_f64());
            let (iu, ou) = (gb.in_u.to_f64(), gb.out_u.to_f64());
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
            cum.var += gb.var_rate * dt;
            cum.batch = cum.batch.max(gb.batch);
            if gb.class > cum.class || (gb.class == cum.class && cum.why.0 == report::why::NONE) {
                cum.class = gb.class;
                cum.why = gb.why;
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
        for (&g, &a) in stock {
            win.goods.entry(g).or_insert_with(|| {
                (
                    Acc::default(),
                    Cum {
                        class: if unbounded {
                            report::class::UNBOUNDED
                        } else {
                            report::class::EXACT
                        },
                        batch: 1.0,
                        ..Cum::default()
                    },
                )
            });
            let _ = a;
        }
        for (g, _) in ports.iter() {
            win.goods.entry(g).or_default();
        }
        for (g, (acc, cum)) in win.goods.iter_mut() {
            let y = stock.get(g).copied().unwrap_or(0) as f64 - ports.get(*g) as f64;
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
    }

    /// Judges the window.
    fn judge(&self, win: &Window, z: f64) -> WindowResult {
        let n = f64::from(win.n);
        let w = win.last.0.saturating_sub(win.start.0) as f64;
        let mut goods = Vec::new();
        let mut any_fail = false;
        for (g, (acc, cum)) in &win.goods {
            let measured = slope(n, win.st, win.stt, acc.sy, acc.sty);
            let predicted = slope(n, win.st, win.stt, acc.sp, acc.stp);
            let lo = if cum.lo_inf {
                f64::NEG_INFINITY
            } else {
                slope(n, win.st, win.stt, acc.slo, acc.stlo)
            };
            let hi = if cum.hi_inf {
                f64::INFINITY
            } else {
                slope(n, win.st, win.stt, acc.shi, acc.sthi)
            };
            // The slope's standard error, from the residual of measured minus predicted.
            let sxx = win.stt - win.st * win.st / n;
            let sxy = acc.std - win.st * acc.sd / n;
            let syy = acc.sdd - acc.sd * acc.sd / n;
            let se = if win.n > 2 && sxx > 0.0 {
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
                if win.n < 3 || cum.class == report::class::UNBOUNDED || !measured.is_finite() {
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
            goods.push(GoodResult {
                good: *g,
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
                samples: win.n,
            });
        }
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

    /// The node limiting a good's flow on the read stores (§5.3): from the stores' edges carrying it
    /// (in-edges for a produced good, out-edges for a consumed one), the nearest node with flow whose
    /// own capacity binds; else the nearest machine; else the nearest node.
    pub fn limiting(&self, good: Good, produced: bool) -> Option<u32> {
        let g = &self.graph;
        let mut queue: Vec<u32> = Vec::new();
        let mut seen = vec![false; g.nodes.len()];
        for &(e, gd, flow, into) in &self.sol.store_edges {
            if gd != good || into != produced || flow <= 0.0 {
                continue;
            }
            let ed = g.edge(e);
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
    /// it, and after `glass.mismatchStrikes` failed windows demote its kind (G-B14). Returns the
    /// probe set to send and a newly demoted model, if any.
    fn act(&mut self, res: &WindowResult, tun: &GlassTunables) -> (Option<Vec<u32>>, Option<u32>) {
        if res.verdict != report::verdict::FAIL {
            self.strikes = 0;
            return (None, None);
        }
        self.strikes += 1;
        // The most optimistic failing good: the model predicts more throughput than measured.
        let mut worst: Option<(f64, Good, bool)> = None;
        for r in &res.goods {
            if r.verdict != report::verdict::FAIL || r.predicted == 0.0 || !r.predicted.is_finite()
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
        let Some((eta, good, produced)) = worst else {
            return (None, None);
        };
        let Some(v) = self.limiting(good, produced) else {
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
        let mut out = (None, None);
        self.sweeps += 1;
        self.district_revision = Some(view.revision);
        self.advance(at, tun);
        let stock = self.stock(view);
        let per = self.store_levels(view);
        let unbounded = self.sol.has_unbounded;
        if self.win.is_some() {
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
                out = self.act(&res, tun);
                if res.verdict == report::verdict::PASS {
                    self.passed = Some(res.clone());
                    self.edited = false;
                }
                self.last = Some(res);
                // The next window starts here, with this sample.
                self.win = Some(Window {
                    start: at,
                    ..Window::default()
                });
                self.read_state(view, goods, tun);
                self.resolve(tun, false);
                self.sample(at, &stock, &per, view.ports, unbounded);
                self.set_stock(view);
                self.last_sweep = Some(at);
                return out;
            }
        }
        self.read_state(view, goods, tun);
        self.resolve(tun, false);
        if self.win.is_none() && at >= self.ready_at {
            self.win = Some(Window {
                start: at,
                ..Window::default()
            });
            self.sample(at, &stock, &per, view.ports, self.sol.has_unbounded);
            self.sweeps_sampled += 1;
        }
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
                eligible: self.eligible(),
                learned_sources: dg.replay.as_ref().map_or(0, |r| r.learned_sources()),
            },
            goods: goods.into_values().collect(),
        }
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
