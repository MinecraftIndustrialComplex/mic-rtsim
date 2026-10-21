// SPDX-License-Identifier: AGPL-3.0-or-later
//! One district's glass state: the compile it holds, the regime it is in, and the checksum window
//! (GLASSBOX.md §5.3, G-B14; docs/industry-boundary.md §9.3).
//!
//! **Per sweep, O(stores × goods):** the interval since the last sweep is integrated under the
//! regime it was in (controllers switch between sweeps, so the prediction is the time average of
//! the per-interval regimes), the sample is added to the window's running sums, then the stores'
//! states and the controllers are read again; only a changed regime solves (and a regime seen
//! recently comes from the cache).
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
use super::solve::{self, Regime, Solution};
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
/// A controller flag: the output is inverted.
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
    /// ∫ λ E[X²] dt.
    var: f64,
    batch: f64,
    class: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Window {
    start: Tick,
    last: Tick,
    n: u32,
    st: f64,
    stt: f64,
    goods: BTreeMap<Good, (Acc, Cum)>,
}

/// One good's verdict in a completed window.
#[derive(Clone, Debug, PartialEq)]
pub struct GoodResult {
    pub good: Good,
    pub class: u32,
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
    pub solves: u32,
    pub cache_hits: u32,
    pub solve_micros: f64,
    win: Option<Window>,
    pub last: Option<WindowResult>,
    pub windows_completed: u32,
    pub strikes: u32,
    /// Per (inventory, good) with a negative L net rate: its stock at the last sweep, so an interval
    /// never predicts more drain than the store held (§4.1's "input store empties").
    drains: Vec<(u32, Good, f64, f64)>,
    last_sweep: Option<Tick>,
    pub probe_set: Vec<u32>,
    pub probes: BTreeMap<u32, ProbeTrack>,
    pub events: VecDeque<GlassEvent>,
}

/// One district's glass state.
#[derive(Clone, Debug, Default)]
pub struct DistrictGlass {
    pub asm: super::store::Assembler,
    pub compiled: Option<Compiled>,
    /// The latest not-compilable END: (compile, reason).
    pub not_compilable: Option<(u32, u32)>,
    /// Chunks of the district that block-tick (IND_CHUNK_TICKING), for G2's partial loads.
    pub ticking: BTreeSet<crate::ids::ChunkPos>,
    /// Model codes demoted to measured here (G-B14), kept across compiles.
    pub demoted: BTreeSet<u32>,
}

/// What a sweep sees of the district.
pub struct View<'a> {
    pub revision: u32,
    pub inventories: &'a VecMap<u32, Inventory>,
    pub ports: &'a GoodVec,
}

fn holding_threshold(goods: &dyn GoodLookup, g: Good, stacks: i64) -> i64 {
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
            solves: 0,
            cache_hits: 0,
            solve_micros: 0.0,
            win: None,
            last: None,
            windows_completed: 0,
            strikes: 0,
            drains: Vec::new(),
            last_sweep: None,
            probe_set: Vec::new(),
            probes: BTreeMap::new(),
            events: VecDeque::new(),
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
            let held: Vec<Good> = c
                .iter()
                .filter(|&(g, a)| a >= holding_threshold(goods, g, tun.holding_stacks).max(1))
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
        for g in self.regime.gate_off.iter_mut() {
            *g = false;
        }
        for v in 0..n as u32 {
            for &e in self.graph.control_in.of(v) {
                let ed = self.graph.edge(e);
                let c = ed.from as usize;
                let signal =
                    self.ctrl_on[c] ^ (self.graph.nodes[c].params[2] & CONTROLLER_INVERTED != 0);
                let off = match ed.param {
                    CONTROL_STOP_WHEN_ON => signal,
                    CONTROL_RUN_WHEN_ON => !signal,
                    _ => false,
                };
                if off {
                    self.regime.gate_off[v as usize] = true;
                }
            }
        }
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
            if self.cache.len() >= tun.regime_cache {
                self.cache.pop_front();
            }
            self.cache.push_back((key, sol));
        }
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
        self.drains.clear();
        self.ready_at = self
            .ready_at
            .max(Tick(at.0 + tun.warmup_ticks + self.graph.longest_latency));
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

    /// Integrates the interval since the last sweep under the regime it was in.
    fn integrate(&mut self, at: Tick) {
        let (Some(win), Some(prev)) = (self.win.as_mut(), self.last_sweep) else {
            return;
        };
        let dt = at.0.saturating_sub(prev.0) as f64;
        if dt <= 0.0 {
            return;
        }
        // Clamp corrections per good: a store never drains below empty.
        let mut corr: BTreeMap<Good, f64> = BTreeMap::new();
        for &(_, g, net, stock) in &self.drains {
            let raw = net * dt;
            let clamped = raw.max(-stock);
            *corr.entry(g).or_insert(0.0) += clamped - raw;
        }
        for gb in &self.sol.goods {
            let (_, cum) = win.goods.entry(gb.good).or_default();
            let net = gb.in_l.to_f64() - gb.out_l.to_f64();
            let dp = net * dt + corr.get(&gb.good).copied().unwrap_or(0.0);
            cum.p += dp;
            let out_spread = gb.out_u.to_f64() - gb.out_l.to_f64();
            let in_spread = gb.in_u.to_f64() - gb.in_l.to_f64();
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
            cum.class = cum.class.max(gb.class);
        }
    }

    /// Adds the sample at `at` to the open window.
    fn sample(&mut self, at: Tick, stock: &BTreeMap<Good, i64>, ports: &GoodVec, unbounded: bool) {
        let Some(win) = self.win.as_mut() else { return };
        let t = at.0.saturating_sub(win.start.0) as f64;
        win.n += 1;
        win.st += t;
        win.stt += t * t;
        win.last = at;
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
        WindowResult {
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
        self.integrate(at);
        let stock = self.stock(view);
        let unbounded = self.sol.has_unbounded;
        if self.win.is_some() {
            self.sample(at, &stock, view.ports, unbounded);
            let done = self
                .win
                .as_ref()
                .is_some_and(|w| at.0.saturating_sub(w.start.0) >= self.window_target && w.n >= 3);
            if done {
                let win = self.win.take().expect("open");
                let res = self.judge(&win, tun.checksum_z);
                self.windows_completed += 1;
                out = self.act(&res, tun);
                self.last = Some(res);
                // The next window starts here, with this sample.
                self.win = Some(Window {
                    start: at,
                    ..Window::default()
                });
                self.read_state(view, goods, tun);
                self.resolve(tun, false);
                self.sample(at, &stock, view.ports, unbounded);
                self.set_drains(view);
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
            self.sample(at, &stock, view.ports, self.sol.has_unbounded);
        }
        self.set_drains(view);
        self.last_sweep = Some(at);
        out
    }

    fn set_drains(&mut self, view: &View<'_>) {
        self.drains.clear();
        for &(inv, g, net) in &self.sol.per_store {
            if net >= 0.0 {
                continue;
            }
            let stock = view
                .inventories
                .get(&inv)
                .map_or(0, |i| i.known.get(g))
                .max(0) as f64;
            self.drains.push((inv, g, net, stock));
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

    /// The district's report (`report.rs`).
    pub fn report(&self, demoted: usize) -> Report {
        let sol = &self.sol;
        // Window goods: the last completed window, else the one in progress.
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
            goods.insert(
                gb.good,
                GoodReport {
                    good: gb.good.0,
                    class: gb.class,
                    in_l: gb.in_l.to_f64(),
                    in_u: gb.in_u.to_f64(),
                    out_l: gb.out_l.to_f64(),
                    out_u: gb.out_u.to_f64(),
                    predicted: gb.in_l.to_f64() - gb.out_l.to_f64(),
                    measured: f64::NAN,
                    sigma: 0.0,
                    q: 0.0,
                    verdict: report::verdict::NONE,
                    samples: 0,
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
                });
                if gr.predicted.is_finite() {
                    e.predicted = gr.predicted;
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
            demoted: demoted as u32,
            latency: self.graph.longest_latency,
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
