// SPDX-License-Identifier: AGPL-3.0-or-later
//! The solve (GLASSBOX.md §3; docs/industry-boundary.md §9.3): Create's back-pressure fixed point,
//! the floor L, in exact rationals, then the classes and the ceiling U (`lp.rs`).
//!
//! **One round** is a backward pass (each edge's acceptance per good, in reverse order) and a
//! forward pass (each node runs at `min(supply by recipe ratio, capacity, acceptance)` and splits
//! by its rule). After it, an edge whose head used less than arrived gets its acceptance lowered to
//! what was used (coupling, §3.2 step 4), and a boiler whose level moved changes its domain's level.
//! Acceptances only fall. A back edge of a cycle (its head comes first in the order) brings its
//! tail's push of the round before, and a head whose back-edge supply still changes is not throttled
//! (GB110). The rounds stop when nothing changed, or at `glass.solveRoundsMax`, when every good is
//! bounded.
//!
//! **Cost:** O(rounds × slots) rational operations, a slot being one good on one edge. The passes
//! allocate nothing; a solve allocates its slot arrays once.

use std::collections::{BTreeMap, BTreeSet};

use super::graph::{Graph, carries_goods, term_amount};
use super::q::Q;
use super::records::{
    EdgeKind, NodeKind, edge_flags, heat as heat_level, model, node_flags, recipe_type, rule,
    term_flags,
};
use super::report::why;
use crate::ids::Good;
use crate::tunables::GlassTunables;

/// The variance of one roll of a chance output whose expectation is `e`: a Bernoulli for `e <= 1`,
/// else `ceil(e)` units each at `e / ceil(e)` (a binomial), as `ProcessingOutput.rollOutput` rolls a
/// count at a chance (GB111).
pub fn chance_variance(e: f64) -> f64 {
    if e.is_nan() || e <= 0.0 || e.is_infinite() {
        return 0.0;
    }
    if e <= 1.0 {
        e * (1.0 - e)
    } else {
        let k = e.ceil();
        let p = e / k;
        k * p * (1.0 - p)
    }
}

/// GB72 (GLASSBOX.md §3.3, changed by Ben 2026-10-21): belt side insertion as Create has it.
/// `BeltInventory.canInsertAtFromSide` (`isBlocking`) stops an insertion only while an earlier
/// insertion at the same segment from the same side is within a block of it, never for the items
/// passing, so at a belt segment the insertions go first, sharing the segment's capacity among
/// themselves, and the through traffic (in-edges flagged `THROUGH`) takes what is left. `false`
/// restores the earlier rule: the main line first, the side inputs sharing the rest.
pub const BELT_INSERTIONS_FIRST: bool = true;

/// GB72 (GLASSBOX.md §3.4, changed by Ben 2026-10-21): a sequenced-assembly loop fed faster than
/// its machines finish items stalls ([`Solver::shut_overfed_loops`]). `false` restores
/// settling at the machines' capacity.
pub const OVERFED_LOOPS_STALL: bool = true;

/// Taint levels, ordered: a good takes the worst on its paths (`report::class`).
pub const EXACT: u8 = 0;
pub const BOUNDED: u8 = 1;
pub const UNBOUNDED: u8 = 2;

/// A heater's burn phase in a regime (`Regime::burning`): what is left of the burn it had at the
/// compile (`params[0]`), as Create's `BlazeBurnerBlockEntity` spends it.
pub mod burn {
    /// No burn left: its duty is the fuel it takes (§9.3).
    pub const OUT: u8 = 0;
    /// Lit, and above the insertion threshold (`params[3]`): it refuses fuel (`tryUpdateFuel`).
    pub const REFUSING: u8 = 1;
    /// Lit, at or below the threshold: it takes fuel up to `1 / burnTicks` items per tick.
    pub const TAKING: u8 = 2;
}

/// What the solve takes from the latest sweep and the engine's own state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Regime {
    /// Per node: the goods a store holds (`check.rs`'s holding rule, GB13 and GB100), ascending.
    pub held: Vec<Vec<Good>>,
    /// Per node: a store with room.
    pub room: Vec<bool>,
    /// Per node: a store whose inventory the sampler reads (its flows are measured).
    pub read: Vec<bool>,
    /// Per node: stopped by a controller (G-B8).
    pub gate_off: Vec<bool>,
    /// Derating factors from failed windows (G-B14), by node.
    pub derate: BTreeMap<u32, Q>,
    /// Model codes demoted to measured (black boxes) in this district.
    pub demoted: BTreeSet<u32>,
    /// Per node: a heater's [`burn`] phase (the engine tracks its remaining burn between solves).
    pub burning: Vec<u8>,
}

impl Regime {
    pub fn new(n: usize) -> Regime {
        Regime {
            held: vec![Vec::new(); n],
            room: vec![true; n],
            read: vec![false; n],
            gate_off: vec![false; n],
            derate: BTreeMap::new(),
            demoted: BTreeSet::new(),
            burning: vec![burn::OUT; n],
        }
    }

    /// A 64-bit key of everything a solve reads (FNV-1a), for the regime cache.
    pub fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |x: u64| {
            for b in x.to_le_bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for (v, held) in self.held.iter().enumerate() {
            mix(v as u64);
            mix(u64::from(self.room[v])
                | u64::from(self.read[v]) << 1
                | u64::from(self.gate_off[v]) << 2
                | u64::from(self.burning.get(v).copied().unwrap_or(burn::OUT)) << 3);
            for g in held {
                mix(u64::from(g.0));
            }
        }
        for (v, q) in &self.derate {
            mix(u64::from(*v));
            mix(q.numer() as u64);
            mix(q.denom() as u64);
        }
        for m in &self.demoted {
            mix(u64::from(*m) | 1 << 40);
        }
        h
    }
}

/// One good's bounds on the district's read stores (rates per tick).
#[derive(Clone, Debug, PartialEq)]
pub struct GoodBound {
    pub good: Good,
    pub class: u32,
    pub in_l: Q,
    pub out_l: Q,
    pub in_u: Q,
    pub out_u: Q,
    /// The LP's own optima before the guards (None: not run), for tests and the bench.
    pub in_u_lp: Option<f64>,
    pub out_u_lp: Option<f64>,
    /// The linear relaxation bound.
    pub in_u_lin: Q,
    pub out_u_lin: Q,
    /// The checksum's model variance per tick: `λ × E[X²]` of stochastic sources emitting it, plus
    /// the roll variance of chance outputs reaching the read stores (GB111).
    pub var_rate: f64,
    /// The largest batch into plus out of the read stores, in units (q = batch / W).
    pub batch: f64,
    /// Why the class is what it is: a `report::why` code and its node or edge (u32::MAX: none).
    pub why: (u32, u32),
}

/// A solve's result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solution {
    /// Goods on the read stores' edges, ascending.
    pub goods: Vec<GoodBound>,
    /// Per (inventory, good): the predicted net rate into that read store under L, per tick.
    pub per_store: Vec<(u32, Good, f64)>,
    pub rounds: u32,
    pub converged: bool,
    /// Per node: its own capacity bound the flow (a limiting-node candidate, §5.3).
    pub saturated: Vec<bool>,
    /// Per edge: the flow L used, all goods (f64), for localization.
    pub edge_flow: Vec<f64>,
    /// Read-store edges with their good and L flow: (edge, good, flow, into the store).
    pub store_edges: Vec<(u32, Good, f64, bool)>,
    /// Every goods edge's exact L flow per good, nonzero only, by edge then good: what the glass
    /// replay books per store and across a partial load's cut (G2).
    pub edge_goods: Vec<(u32, Good, Q)>,
    /// Per domain: the running level index under L (None: stopped or overstressed).
    pub dom_level: Vec<Option<u32>>,
    /// Per node: a heater's duty under L.
    pub duty: Vec<Q>,
    /// Per node: the share of a heater's burn its fuel under L pays for, `min(1, Σ fuel × burnTicks)`
    /// (its remaining burn falls by `1 - fed` per tick while lit).
    pub fed: Vec<f64>,
    /// Graph-wide: a black box or port (or a demoted node) exists.
    pub has_unbounded: bool,
    /// LPs run, LPs skipped as too big (`glass.lpVarsMax`), LPs not needed (linear bound = L).
    pub lp_solves: u32,
    pub lp_skipped: u32,
    pub lp_tight: u32,
}

/// A per-good scratch array that clears only what it touched.
#[derive(Clone, Debug, Default)]
struct Scratch {
    v: Vec<Q>,
    on: Vec<bool>,
    list: Vec<u32>,
}

impl Scratch {
    fn new(n: usize) -> Scratch {
        Scratch {
            v: vec![Q::ZERO; n],
            on: vec![false; n],
            list: Vec::new(),
        }
    }

    fn get(&self, g: u32) -> Q {
        if self.on[g as usize] {
            self.v[g as usize]
        } else {
            Q::ZERO
        }
    }

    fn has(&self, g: u32) -> bool {
        self.on[g as usize]
    }

    fn set(&mut self, g: u32, q: Q) {
        if !self.on[g as usize] {
            self.on[g as usize] = true;
            self.list.push(g);
        }
        self.v[g as usize] = q;
    }

    fn add(&mut self, g: u32, q: Q) {
        let cur = self.get(g);
        self.set(g, cur.add(q));
    }

    fn clear(&mut self) {
        for &g in &self.list {
            self.on[g as usize] = false;
        }
        self.list.clear();
    }
}

/// One side of a split: an out-edge's slot for the good, its acceptance, whether it is a
/// filtered side, and what it is given.
#[derive(Clone, Copy, Debug)]
struct Side {
    slot: usize,
    acc: Q,
    filtered: bool,
    give: Q,
}

/// What a node's rule does with supply over its sides (GLASSBOX.md §3.3).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Priority,
    Fair,
    Forced,
}

fn mode_of(r: u32) -> Mode {
    match r {
        rule::SPLIT
        | rule::ROUND_ROBIN
        | rule::RANDOMIZE
        | rule::SYNCHRONIZE
        | rule::MERGE
        | rule::SIDE_INSERT => Mode::Fair,
        rule::FORCED_SPLIT | rule::FORCED_ROUND_ROBIN => Mode::Forced,
        _ => Mode::Priority,
    }
}

/// Rules whose split is a model choice, not Create's exact behaviour (§3.3, "bounded").
fn rule_bounded(r: u32) -> bool {
    matches!(
        r,
        rule::RANDOMIZE | rule::SYNCHRONIZE | rule::MERGE | rule::SIDE_INSERT
    )
}

/// The acceptance of a group of sides under a mode.
fn group_accept(mode: Mode, sides: &[Side]) -> Q {
    if sides.is_empty() {
        return Q::ZERO;
    }
    match mode {
        Mode::Forced => {
            let m = sides.iter().map(|s| s.acc).min().expect("non-empty");
            m.mul(Q::int(sides.len() as i64))
        }
        _ => sides.iter().fold(Q::ZERO, |a, s| a.add(s.acc)),
    }
}

/// Allocates `supply` over a group of sides; returns what is left.
fn group_split(mode: Mode, supply: Q, sides: &mut [Side], order_buf: &mut Vec<usize>) -> Q {
    if sides.is_empty() || supply.is_zero() {
        return supply;
    }
    match mode {
        Mode::Priority => {
            let mut rem = supply;
            for s in sides.iter_mut() {
                let give = rem.min(s.acc);
                s.give = give;
                rem = rem.sub_sat(give);
            }
            rem
        }
        Mode::Forced => {
            let k = Q::int(sides.len() as i64);
            let m = sides.iter().map(|s| s.acc).min().expect("non-empty");
            let each = supply.div(k).min(m);
            for s in sides.iter_mut() {
                s.give = each;
            }
            supply.sub_sat(each.mul(k))
        }
        Mode::Fair => {
            // Water-filling: smallest acceptance first, each takes an equal share of what is left.
            order_buf.clear();
            order_buf.extend(0..sides.len());
            order_buf.sort_by(|&a, &b| sides[a].acc.cmp(&sides[b].acc).then(a.cmp(&b)));
            let mut rem = supply;
            let mut k = sides.len() as i64;
            for &i in order_buf.iter() {
                let share = rem.div(Q::int(k));
                let give = share.min(sides[i].acc);
                sides[i].give = give;
                rem = rem.sub_sat(give);
                k -= 1;
            }
            rem
        }
    }
}

/// A rule over all sides: filtered sides first, then the rest (`distributionTargets`).
fn rule_accept(r: u32, sides: &[Side]) -> Q {
    let mode = mode_of(r);
    let nf = sides.iter().filter(|s| s.filtered).count();
    if nf == 0 || nf == sides.len() {
        return group_accept(mode, sides);
    }
    let mut a = Vec::with_capacity(nf);
    let mut b = Vec::with_capacity(sides.len() - nf);
    for s in sides {
        if s.filtered { a.push(*s) } else { b.push(*s) }
    }
    group_accept(mode, &a).add(group_accept(mode, &b))
}

fn rule_split(r: u32, supply: Q, sides: &mut [Side], order_buf: &mut Vec<usize>) {
    let mode = mode_of(r);
    for s in sides.iter_mut() {
        s.give = Q::ZERO;
    }
    // Filtered sides sort first (stable: the edge order within each group).
    sides.sort_by_key(|s| !s.filtered);
    let nf = sides.iter().filter(|s| s.filtered).count();
    let (a, b) = sides.split_at_mut(nf);
    let rem = group_split(mode, supply, a, order_buf);
    group_split(mode, rem, b, order_buf);
}

/// The solver's working state for one solve.
pub struct Solver<'a> {
    pub g: &'a Graph,
    pub rg: &'a Regime,
    pub tun: &'a GlassTunables,
    /// Dense goods.
    pub goods: Vec<Good>,
    words: usize,
    /// Per node, a bitset of the goods that can be there.
    reach: Vec<u64>,
    /// Per goods edge: its slots `slot_off[e]..slot_off[e + 1]`, each a dense good, ascending.
    pub slot_off: Vec<u32>,
    pub slot_good: Vec<u32>,
    pub f: Vec<Q>,
    pub acc: Vec<Q>,
    pub ovr: Vec<Q>,
    pub used: Vec<Q>,
    /// Per slot on a back edge (GB12): what arrived at its head this round, the tail's push of the
    /// round before (GB110).
    back_in: Vec<Q>,
    pub dom_level: Vec<Option<u32>>,
    dom_next: Vec<Option<u32>>,
    pub duty: Vec<Q>,
    duty_assumed: Vec<Q>,
    pub saturated: Vec<bool>,
    /// Per node: the worst rule on it (taint), set by the passes and the graph.
    pub taint: Vec<u8>,
    /// Per node: the `report::why` code of its taint.
    pub taint_why: Vec<u32>,
    /// Per node: a heater's fed share (`Solution::fed`).
    pub fed: Vec<Q>,
    /// Per node: crafts per tick it ran under L (sources, for the stochastic variance).
    pub crafts: Vec<Q>,
    /// Per node: the variance per tick of its chance outputs under L, by good (GB111).
    pub chance_var: Vec<Vec<(u32, f64)>>,
    s_in: Scratch,
    s_out: Scratch,
    s_acc: Scratch,
    s_push: Scratch,
    sides: Vec<Side>,
    obuf: Vec<usize>,
    rbuf: Vec<Q>,
    rbuf2: Vec<Q>,
    takes: Vec<(usize, Q)>,
    /// A belt merge's in-edges in priority order (GB72).
    in_order: Vec<u32>,
    /// GB72's switches, from [`BELT_INSERTIONS_FIRST`] and [`OVERFED_LOOPS_STALL`] (tests flip
    /// them).
    pub(crate) insertions_first: bool,
    pub(crate) loops_stall: bool,
    /// Nodes taken as sinks: an over-fed loop's probe (what its feed brings when the loop takes
    /// everything). Empty in a solve.
    as_sink: Vec<bool>,
}

impl<'a> Solver<'a> {
    pub fn new(g: &'a Graph, rg: &'a Regime, tun: &'a GlassTunables) -> Solver<'a> {
        let n = g.nodes.len();
        // The goods: every recipe term, every filter, every held good.
        let mut goods: Vec<Good> = Vec::new();
        for r in &g.recipes {
            goods.extend(r.inputs.iter().chain(r.outputs.iter()).map(|t| t.good));
        }
        for e in &g.edges {
            goods.extend(e.filter.iter().copied());
        }
        for h in &rg.held {
            goods.extend(h.iter().copied());
        }
        goods.sort_unstable();
        goods.dedup();
        let words = goods.len().div_ceil(64).max(1);
        let gn = goods.len();
        let mut s = Solver {
            g,
            rg,
            tun,
            goods,
            words,
            reach: vec![0u64; n * words],
            slot_off: Vec::new(),
            slot_good: Vec::new(),
            f: Vec::new(),
            acc: Vec::new(),
            ovr: Vec::new(),
            used: Vec::new(),
            back_in: Vec::new(),
            dom_level: Vec::new(),
            dom_next: Vec::new(),
            duty: vec![Q::ZERO; n],
            duty_assumed: vec![Q::ONE; n],
            saturated: vec![false; n],
            taint: vec![EXACT; n],
            taint_why: vec![why::NONE; n],
            fed: vec![Q::ZERO; n],
            crafts: vec![Q::ZERO; n],
            chance_var: vec![Vec::new(); n],
            s_in: Scratch::new(gn),
            s_out: Scratch::new(gn),
            s_acc: Scratch::new(gn),
            s_push: Scratch::new(gn),
            sides: Vec::new(),
            obuf: Vec::new(),
            rbuf: Vec::new(),
            rbuf2: Vec::new(),
            takes: Vec::new(),
            in_order: Vec::new(),
            insertions_first: BELT_INSERTIONS_FIRST,
            loops_stall: OVERFED_LOOPS_STALL,
            as_sink: Vec::new(),
        };
        s.reachability();
        s.slots();
        s.dom_level = (0..g.domains.len() as u32)
            .map(|d| s.running(d, g.domains[d as usize].current))
            .collect();
        s.dom_next = s.dom_level.clone();
        s
    }

    pub fn gidx(&self, g: Good) -> Option<u32> {
        self.goods.binary_search(&g).ok().map(|i| i as u32)
    }

    /// The node's kind as the solve treats it: a demoted model is a black box (G-B14), and a store
    /// the sampler does not read is a buffer (its stock is unknown, so it only passes goods on).
    pub fn kind(&self, v: u32) -> NodeKind {
        if self.as_sink.get(v as usize) == Some(&true) {
            return NodeKind::Sink;
        }
        let n = self.g.node(v);
        match n.kind {
            NodeKind::Store => {
                if self.rg.read[v as usize] {
                    NodeKind::Store
                } else {
                    NodeKind::Buffer
                }
            }
            k => {
                if self.rg.demoted.contains(&n.model) {
                    NodeKind::BlackBox
                } else {
                    k
                }
            }
        }
    }

    fn held(&self, v: u32, gi: u32) -> bool {
        self.rg.held[v as usize]
            .binary_search(&self.goods[gi as usize])
            .is_ok()
    }

    // ---- reachability and slots -----------------------------------------------------------

    fn bit(&self, v: u32, gi: u32) -> bool {
        self.reach[v as usize * self.words + gi as usize / 64] >> (gi % 64) & 1 == 1
    }

    fn set_bit(&mut self, v: u32, gi: u32) -> bool {
        let w = &mut self.reach[v as usize * self.words + gi as usize / 64];
        let m = 1u64 << (gi % 64);
        let was = *w & m != 0;
        *w |= m;
        !was
    }

    /// The goods that can be at each node, to a fixed point (machines make their recipes' outputs
    /// once every input can arrive).
    fn reachability(&mut self) {
        let g = self.g;
        let n = g.nodes.len() as u32;
        // Seeds.
        for v in 0..n {
            match self.kind(v) {
                NodeKind::Store => {
                    let held = self.rg.held[v as usize].clone();
                    for good in held {
                        if let Some(gi) = self.gidx(good) {
                            self.set_bit(v, gi);
                        }
                    }
                }
                NodeKind::Source | NodeKind::BlackBox | NodeKind::Port => {
                    let recipes: Vec<u32> = g.node_recipes.of(v).to_vec();
                    for r in recipes {
                        for t in g.recipes[r as usize].outputs.clone() {
                            if let Some(gi) = self.gidx(t.good) {
                                self.set_bit(v, gi);
                            }
                        }
                    }
                    for &e in g.out_g.of(v) {
                        let ed = g.edge(e);
                        if ed.flags & edge_flags::DENY == 0 {
                            for good in ed.filter.clone() {
                                if let Some(gi) = self.gidx(good) {
                                    self.set_bit(v, gi);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let mut in_goods = vec![0u64; self.words];
        loop {
            let mut changed = false;
            for &v in &g.order {
                // The goods arriving at v.
                in_goods.iter_mut().for_each(|w| *w = 0);
                for &e in g.in_g.of(v) {
                    let from = g.edge(e).from;
                    for gi in 0..self.goods.len() as u32 {
                        if self.bit(from, gi) && g.allows(e, self.goods[gi as usize]) {
                            in_goods[gi as usize / 64] |= 1 << (gi % 64);
                        }
                    }
                }
                let has_in = |gi: u32| in_goods[gi as usize / 64] >> (gi % 64) & 1 == 1;
                match self.kind(v) {
                    NodeKind::Machine => {
                        let node = g.node(v);
                        let pass = node.flags & node_flags::PASS_THROUGH != 0;
                        let mut taken = Vec::new();
                        for &r in g.node_recipes.of(v) {
                            let rec = &g.recipes[r as usize];
                            let ok = rec
                                .inputs
                                .iter()
                                .filter(|t| t.flags & term_flags::CATALYST == 0)
                                .all(|t| self.gidx(t.good).is_some_and(has_in));
                            for t in &rec.inputs {
                                taken.push(t.good);
                            }
                            if ok {
                                for t in rec.outputs.clone() {
                                    if let Some(gi) = self.gidx(t.good) {
                                        changed |= self.set_bit(v, gi);
                                    }
                                }
                            }
                        }
                        if pass {
                            for gi in 0..self.goods.len() as u32 {
                                if has_in(gi) && !taken.contains(&self.goods[gi as usize]) {
                                    changed |= self.set_bit(v, gi);
                                }
                            }
                        }
                    }
                    NodeKind::Store
                    | NodeKind::Buffer
                    | NodeKind::Mover
                    | NodeKind::Junction
                    | NodeKind::BlackBox
                    | NodeKind::Port => {
                        for gi in 0..self.goods.len() as u32 {
                            if has_in(gi) {
                                changed |= self.set_bit(v, gi);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn slots(&mut self) {
        let g = self.g;
        let mut off = Vec::with_capacity(g.edges.len() + 1);
        let mut sg = Vec::new();
        off.push(0u32);
        for (e, ed) in g.edges.iter().enumerate() {
            if carries_goods(ed.kind) {
                for gi in 0..self.goods.len() as u32 {
                    if self.bit(ed.from, gi) && g.allows(e as u32, self.goods[gi as usize]) {
                        sg.push(gi);
                    }
                }
            }
            off.push(sg.len() as u32);
        }
        let s = sg.len();
        self.slot_off = off;
        self.slot_good = sg;
        self.f = vec![Q::ZERO; s];
        self.acc = vec![Q::ZERO; s];
        self.ovr = vec![Q::INF; s];
        self.used = vec![Q::ZERO; s];
        self.back_in = vec![Q::ZERO; s];
        // A back edge's tail reads its acceptance before the head sets it (reverse order): it
        // starts at the edge's capacity, and coupling lowers it (GB110).
        for e in 0..g.edges.len() as u32 {
            if g.back[e as usize] {
                let cap = g.edge_cap(e);
                for sl in self.slots_of(e) {
                    self.acc[sl] = cap;
                }
            }
        }
    }

    pub fn slots_of(&self, e: u32) -> std::ops::Range<usize> {
        self.slot_off[e as usize] as usize..self.slot_off[e as usize + 1] as usize
    }

    pub fn slot_of(&self, e: u32, gi: u32) -> Option<usize> {
        let r = self.slots_of(e);
        let base = r.start;
        self.slot_good[r].binary_search(&gi).ok().map(|i| base + i)
    }

    // ---- capacities ---------------------------------------------------------------------------

    /// A domain's running level at `level`, or None when it is stopped or overstressed (§9.3: Create's
    /// floating-point sum of `count × |ratio × speed| × impact` against the level's capacity).
    pub fn running(&self, d: u32, level: u32) -> Option<u32> {
        let dom = &self.g.domains[d as usize];
        let lv = dom.levels.get(level as usize)?;
        if dom.flags & super::records::domain_flags::STRESS_DISABLED != 0 {
            return Some(level);
        }
        let speed = f64::from(lv.speed_num) / f64::from(lv.speed_den.max(1));
        let mut stress = 0.0f64;
        for m in &dom.members {
            let ratio = f64::from(m.ratio_num) / f64::from(m.ratio_den.max(1));
            stress += f64::from(m.count) * (ratio * speed).abs() * m.impact;
        }
        if lv.capacity < stress {
            None
        } else {
            Some(level)
        }
    }

    /// The node's level index (its domain's, 0 outside one) and whether it runs at all.
    fn level(&self, v: u32, dl: &[Option<u32>]) -> Option<u32> {
        match self.g.node(v).domain {
            None => Some(0),
            Some(d) => dl[d as usize],
        }
    }

    /// The factor on the node's capacities: 0 when gated off or its domain stopped, else its
    /// derating factor.
    fn scale(&self, v: u32, dl: &[Option<u32>]) -> Option<Q> {
        if self.rg.gate_off[v as usize] {
            return None;
        }
        self.level(v, dl)?;
        Some(self.rg.derate.get(&v).copied().unwrap_or(Q::ONE))
    }

    /// The node's level under L, if it runs.
    pub fn dom_level_of(&self, v: u32) -> Option<u32> {
        self.level(v, &self.dom_level)
    }

    /// The node's shared capacity at the level of `dl`: unlimited without entries; 0 when it has
    /// entries but none at this level (or is stopped).
    pub fn shared_cap_with(&self, v: u32, dl: &[Option<u32>]) -> Q {
        if self.scale(v, dl).is_none() {
            return Q::ZERO;
        }
        self.shared_cap_at(v, self.level(v, dl))
    }

    /// The factor on the node's capacities apart from its domain: 0 when gated off, else its
    /// derating factor.
    fn own_scale(&self, v: u32) -> Option<Q> {
        if self.rg.gate_off[v as usize] {
            return None;
        }
        Some(self.rg.derate.get(&v).copied().unwrap_or(Q::ONE))
    }

    /// The node's shared capacity at an explicit level (None: stopped).
    pub fn shared_cap_at(&self, v: u32, level: Option<u32>) -> Q {
        let (Some(sc), Some(level)) = (self.own_scale(v), level) else {
            return Q::ZERO;
        };
        if !self.g.has_rates(v) {
            return Q::INF;
        }
        let (shared, mut per) = self.g.caps_at(v, level);
        match shared {
            Some(c) => c.mul(sc),
            None => {
                if per.next().is_some() {
                    Q::INF
                } else {
                    Q::ZERO
                }
            }
        }
    }

    /// A recipe's capacity at the node, crafts per tick, before heat.
    pub fn recipe_cap_with(&self, v: u32, r: u32, dl: &[Option<u32>]) -> Q {
        if self.scale(v, dl).is_none() {
            return Q::ZERO;
        }
        self.recipe_cap_at(v, r, self.level(v, dl))
    }

    /// A recipe's capacity at an explicit level (None: stopped), before heat.
    pub fn recipe_cap_at(&self, v: u32, r: u32, level: Option<u32>) -> Q {
        let (Some(sc), Some(level)) = (self.own_scale(v), level) else {
            return Q::ZERO;
        };
        if !self.g.has_rates(v) {
            return Q::INF;
        }
        let (shared, per) = self.g.caps_at(v, level);
        let mut own: Option<Q> = None;
        for (ri, c) in per {
            if ri == r {
                own = Some(own.map_or(c, |o| o.max(c)));
            }
        }
        own.or(shared).unwrap_or(Q::ZERO).mul(sc)
    }

    /// The heat factor of a recipe needing `need` at machine `v`: the best duty of a heater whose
    /// level is high enough (§9.3); 1 for recipes needing none, 0 with no such heater.
    pub fn heat_factor_with(&self, v: u32, need: u32, duty: &[Q]) -> Q {
        if need == heat_level::NONE {
            return Q::ONE;
        }
        let mut best = Q::ZERO;
        for &e in self.g.heat_in.of(v) {
            let ed = self.g.edge(e);
            let h = ed.from;
            let hn = self.g.node(h);
            let level = if ed.param > 0 { ed.param } else { hn.params[1] };
            if level < i64::from(need) {
                continue;
            }
            let d = if hn.kind == NodeKind::Heater {
                duty[h as usize]
            } else {
                Q::ONE
            };
            best = best.max(d);
        }
        best
    }

    /// Raises a node's taint, keeping the first reason at the worst level.
    fn taint_node(&mut self, v: u32, t: u8, w: u32) {
        let i = v as usize;
        if t > self.taint[i] {
            self.taint[i] = t;
            self.taint_why[i] = w;
        }
    }

    /// A heater's burn phase in this regime.
    fn burning(&self, v: u32) -> u8 {
        self.rg
            .burning
            .get(v as usize)
            .copied()
            .unwrap_or(burn::OUT)
    }

    /// A heater's burn ticks per fuel item for a good: its fuel recipe's duration, else
    /// `params[2]`. 0 is a creative burner (never runs out, takes no fuel).
    pub fn burn_ticks(&self, v: u32, good: Good) -> i64 {
        let node = self.g.node(v);
        for &r in self.g.node_recipes.of(v) {
            let rec = &self.g.recipes[r as usize];
            if rec.inputs.iter().any(|t| t.good == good) && rec.duration > 0 {
                return i64::from(rec.duration);
            }
        }
        node.params[2]
    }

    fn filtered(&self, e: u32) -> bool {
        self.g.edge(e).flags & edge_flags::FILTERED_SIDE != 0
    }

    /// The node's out-edge sides for a good, with their current acceptance.
    fn collect_sides(&mut self, v: u32, gi: u32) {
        self.sides.clear();
        for &e in self.g.out_g.of(v) {
            if let Some(s) = self.slot_of(e, gi) {
                self.sides.push(Side {
                    slot: s,
                    acc: self.acc[s],
                    filtered: self.filtered(e),
                    give: Q::ZERO,
                });
            }
        }
    }

    /// What the node's out-edges accept of a good under its rule.
    fn out_accept(&mut self, v: u32, gi: u32) -> Q {
        let r = self.g.node(v).rule;
        if r == rule::ANDESITE_SPLIT {
            return self.andesite(v, gi, None);
        }
        self.collect_sides(v, gi);
        rule_accept(r, &self.sides)
    }

    /// The andesite tunnel (§3.3): per passing entry of `n` items, one to each open side and the
    /// rest on along the main line (the first out-edge); a refusing side stops the stack. With
    /// `supply`, writes the split into `f` and returns what was pushed; without, returns the
    /// acceptance.
    fn andesite(&mut self, v: u32, gi: u32, supply: Option<Q>) -> Q {
        self.collect_sides(v, gi);
        let n = self
            .g
            .in_g
            .of(v)
            .first()
            .map_or(1, |&e| self.g.edge(e).param.max(1));
        if self.sides.is_empty() {
            return Q::ZERO;
        }
        let main = self.sides[0];
        let others = &self.sides[1..];
        let k = (others.len() as i64).min(n);
        if n <= 1 || k == 0 {
            if let Some(s) = supply {
                let give = s.min(main.acc);
                self.f[main.slot] = give;
                return give;
            }
            return main.acc;
        }
        let nq = Q::int(n);
        let main_frac = Q::new(n - k, n);
        // Acceptance: each of the first k sides takes 1/n of the supply.
        let mut a = Q::INF;
        for s in others.iter().take(k as usize) {
            a = a.min(s.acc.mul(nq));
        }
        if n > k {
            a = a.min(main.acc.div(main_frac));
        }
        match supply {
            None => a,
            Some(s) => {
                let total = s.min(a);
                let side = total.div(nq);
                let sides: Vec<usize> = others.iter().take(k as usize).map(|s| s.slot).collect();
                for slot in sides {
                    self.f[slot] = side;
                }
                let m = if n > k { total.mul(main_frac) } else { Q::ZERO };
                self.f[main.slot] = m;
                side.mul(Q::int(k)).add(m)
            }
        }
    }

    /// Splits `supply` of a good over the node's out-edges by its rule, writing `f`; returns the
    /// amount pushed.
    fn push(&mut self, v: u32, gi: u32, supply: Q) -> Q {
        let r = self.g.node(v).rule;
        if r == rule::ANDESITE_SPLIT {
            return self.andesite(v, gi, Some(supply));
        }
        self.collect_sides(v, gi);
        let mut sides = std::mem::take(&mut self.sides);
        rule_split(r, supply, &mut sides, &mut self.obuf);
        let mut pushed = Q::ZERO;
        for s in &sides {
            self.f[s.slot] = s.give;
            pushed = pushed.add(s.give);
        }
        if sides.len() > 1 && rule_bounded(r) {
            self.taint_node(v, BOUNDED, why::RULE);
        }
        self.sides = sides;
        pushed
    }

    /// After the node's pushes: each out-edge's own capacity shared by its goods (proportional,
    /// bounded when it binds with two goods or more), and infinite pushes (no limit modelled) cut
    /// to 0 and marked unbounded. Fills `s_push` with what was pushed per good.
    fn settle_pushes(&mut self, v: u32) {
        let g = self.g;
        self.s_push.clear();
        for &e in g.out_g.of(v) {
            let r = self.slots_of(e);
            for s in r.clone() {
                if self.f[s].is_inf() {
                    self.f[s] = Q::ZERO;
                    self.taint_node(v, UNBOUNDED, why::UNLIMITED_FLOW);
                    self.taint_node(g.edge(e).to, UNBOUNDED, why::UNLIMITED_FLOW);
                }
            }
            let cap = g.edge_cap(e);
            if !cap.is_inf() {
                let total = r.clone().fold(Q::ZERO, |a, s| a.add(self.f[s]));
                if total > cap {
                    let k = cap.div(total);
                    let mut flowing = 0;
                    for s in r.clone() {
                        if self.f[s].is_pos() {
                            flowing += 1;
                        }
                        self.f[s] = self.f[s].mul(k);
                    }
                    if flowing > 1 {
                        self.taint_node(v, BOUNDED, why::SHARED_EDGE);
                    }
                }
            }
            for s in r {
                let gi = self.slot_good[s];
                self.s_push.add(gi, self.f[s]);
            }
        }
    }

    // ---- the passes ----------------------------------------------------------------------------

    /// One backward pass: every in-edge's acceptance per good.
    fn backward(&mut self) {
        let g = self.g;
        let dl = self.dom_level.clone();
        for i in (0..g.order.len()).rev() {
            let v = g.order[i];
            self.s_acc.clear();
            // The goods arriving at v.
            for &e in g.in_g.of(v) {
                for s in self.slots_of(e) {
                    let gi = self.slot_good[s];
                    if !self.s_acc.has(gi) {
                        self.s_acc.set(gi, Q::ZERO);
                    }
                }
            }
            let list = std::mem::take(&mut self.s_acc.list);
            match self.kind(v) {
                NodeKind::Store => {
                    let room = self.rg.room[v as usize];
                    for &gi in &list {
                        let a = if room {
                            Q::INF
                        } else {
                            let mut sum = Q::ZERO;
                            for &e in g.out_g.of(v) {
                                if let Some(s) = self.slot_of(e, gi) {
                                    sum = sum.add(self.acc[s]);
                                }
                            }
                            sum
                        };
                        self.s_acc.v[gi as usize] = a;
                    }
                }
                NodeKind::Buffer | NodeKind::Mover | NodeKind::Junction => {
                    let c = self.shared_cap_with(v, &dl);
                    for &gi in &list {
                        let a = self.out_accept(v, gi).min(c);
                        self.s_acc.v[gi as usize] = a;
                    }
                }
                NodeKind::Sink => {
                    let c = self.shared_cap_with(v, &dl);
                    for &gi in &list {
                        self.s_acc.v[gi as usize] = c;
                    }
                }
                NodeKind::Boiler => {
                    for &gi in &list {
                        self.s_acc.v[gi as usize] = Q::INF;
                    }
                }
                NodeKind::Heater => {
                    let on = self.scale(v, &dl).is_some();
                    let refusing = self.burning(v) == burn::REFUSING;
                    for &gi in &list {
                        let burn = self.burn_ticks(v, self.goods[gi as usize]);
                        let a = if on && !refusing && burn > 0 && self.is_fuel(v, gi) {
                            Q::new(1, burn)
                        } else {
                            Q::ZERO
                        };
                        self.s_acc.v[gi as usize] = a;
                    }
                }
                NodeKind::Machine => {
                    self.machine_accept(v, &list, &dl);
                }
                _ => {
                    for &gi in &list {
                        self.s_acc.v[gi as usize] = Q::ZERO;
                    }
                }
            }
            self.s_acc.list = list;
            for &e in g.in_g.of(v) {
                let cap = g.edge_cap(e);
                for s in self.slots_of(e) {
                    self.acc[s] = cap.min(self.s_acc.get(self.slot_good[s])).min(self.ovr[s]);
                }
            }
        }
        self.s_acc.clear();
    }

    /// Whether a good is fuel for a heater: an input of one of its fuel recipes, or anything when
    /// it has none.
    fn is_fuel(&self, v: u32, gi: u32) -> bool {
        let recipes = self.g.node_recipes.of(v);
        if recipes.is_empty() {
            return true;
        }
        let good = self.goods[gi as usize];
        recipes.iter().any(|&r| {
            self.g.recipes[r as usize]
                .inputs
                .iter()
                .any(|t| t.good == good)
        })
    }

    /// Each recipe's crafts per tick allowed by capacity, heat and (with ANY_OUTPUT_BLOCKS) its
    /// outputs' acceptance, into `rbuf` (one per recipe of the node).
    fn recipe_limits(&mut self, v: u32, dl: &[Option<u32>], duty: &[Q], outputs: bool) {
        let g = self.g;
        let node = g.node(v);
        let blocks = node.flags & node_flags::ANY_OUTPUT_BLOCKS != 0;
        let recipes = g.node_recipes.of(v);
        self.rbuf.clear();
        for &r in recipes {
            let rec = &g.recipes[r as usize];
            let mut c = self
                .recipe_cap_with(v, r, dl)
                .mul(self.heat_factor_with(v, rec.heat, duty));
            if outputs && (blocks || node.kind == NodeKind::Source) {
                for t in &rec.outputs {
                    let amt = term_amount(t.num, t.den);
                    if amt.is_zero() {
                        continue;
                    }
                    let Some(gi) = self.gidx(t.good) else {
                        c = Q::ZERO;
                        continue;
                    };
                    let oa = self.out_accept(v, gi);
                    c = c.min(oa.div(amt));
                }
            }
            self.rbuf.push(c);
        }
    }

    fn machine_accept(&mut self, v: u32, list: &[u32], dl: &[Option<u32>]) {
        let g = self.g;
        let duty = std::mem::take(&mut self.duty_assumed);
        self.recipe_limits(v, dl, &duty, true);
        self.duty_assumed = duty;
        let pass = g.node(v).flags & node_flags::PASS_THROUGH != 0;
        let recipes = g.node_recipes.of(v);
        for &gi in list {
            let good = self.goods[gi as usize];
            let mut a = Q::ZERO;
            let mut taken = false;
            for (k, &r) in recipes.iter().enumerate() {
                for t in &g.recipes[r as usize].inputs {
                    if t.good != good {
                        continue;
                    }
                    taken = true;
                    if t.flags & term_flags::CATALYST == 0 {
                        a = a.add(term_amount(t.num, t.den).mul(self.rbuf[k]));
                    }
                }
            }
            if !taken && pass {
                a = self.out_accept(v, gi);
            }
            self.s_acc.v[gi as usize] = a;
        }
    }

    /// One forward pass.
    fn forward(&mut self) {
        let g = self.g;
        let dl = self.dom_level.clone();
        self.dom_next = self.dom_level.clone();
        for i in 0..g.order.len() {
            let v = g.order[i];
            self.saturated[v as usize] = false;
            self.s_in.clear();
            for &e in g.in_g.of(v) {
                let back = g.back[e as usize];
                for s in self.slots_of(e) {
                    // A back edge brings what its tail pushed in the round before (GB110).
                    if back {
                        self.back_in[s] = self.f[s];
                    }
                    self.s_in.add(self.slot_good[s], self.f[s]);
                }
            }
            match self.kind(v) {
                NodeKind::Store => self.fwd_store(v),
                NodeKind::Buffer | NodeKind::Mover | NodeKind::Junction | NodeKind::Sink => {
                    self.fwd_conserve(v, &dl)
                }
                NodeKind::Machine | NodeKind::Source => self.fwd_machine(v, &dl),
                NodeKind::Heater => self.fwd_heater(v, &dl),
                NodeKind::Boiler => self.fwd_boiler(v),
                _ => {
                    // Black boxes, ports, controllers: L takes nothing and makes nothing.
                    for &e in g.in_g.of(v) {
                        for s in self.slots_of(e) {
                            self.used[s] = Q::ZERO;
                        }
                    }
                    for &e in g.out_g.of(v) {
                        for s in self.slots_of(e) {
                            self.f[s] = Q::ZERO;
                        }
                    }
                }
            }
        }
    }

    /// Allocates consumption per good to the in-edges, proportionally to arrivals; bounded when a
    /// shortfall falls on two in-edges or more.
    fn use_proportional(&mut self, v: u32, cons: impl Fn(&Self, u32) -> Q) {
        let g = self.g;
        let list = self.s_in.list.clone();
        for gi in list {
            let arrived = self.s_in.get(gi);
            let c = cons(self, gi).min(arrived);
            let short = c < arrived;
            let mut flowing = 0;
            for &e in g.in_g.of(v) {
                if let Some(s) = self.slot_of(e, gi) {
                    if self.f[s].is_pos() {
                        flowing += 1;
                    }
                    self.used[s] = if !short {
                        self.f[s]
                    } else if arrived.is_zero() {
                        Q::ZERO
                    } else {
                        self.f[s].mul(c).div(arrived)
                    };
                }
            }
            if short && flowing > 1 {
                self.taint_node(v, BOUNDED, why::SHORT_INPUTS);
            }
        }
    }

    fn fwd_store(&mut self, v: u32) {
        let g = self.g;
        // Supply: unlimited of what it holds; else what it receives.
        self.s_out.clear();
        for &e in g.out_g.of(v) {
            for s in self.slots_of(e) {
                self.s_out.set(self.slot_good[s], Q::ZERO);
            }
        }
        let outs = self.s_out.list.clone();
        for gi in outs {
            let supply = if self.held(v, gi) {
                Q::INF
            } else {
                self.s_in.get(gi)
            };
            self.push(v, gi, supply);
        }
        self.settle_pushes(v);
        let room = self.rg.room[v as usize];
        self.use_proportional(v, |s, gi| if room { Q::INF } else { s.s_push.get(gi) });
    }

    /// A belt segment where insertions meet the through traffic (GB72): a BELT mover with a
    /// `THROUGH` in-edge and another in-edge. Writes its in-edges into `order` in two priority
    /// groups and returns where the second starts: the insertions, then the through traffic
    /// ([`BELT_INSERTIONS_FIRST`]), or the other way round with the switch off. None for any
    /// other node, whose in-edges keep their order.
    fn belt_merge(&self, v: u32, kind: NodeKind, order: &mut Vec<u32>) -> Option<usize> {
        let g = self.g;
        if kind != NodeKind::Mover || g.node(v).model != model::BELT {
            return None;
        }
        let ins = g.in_g.of(v);
        let through = |e: u32| g.edge(e).flags & edge_flags::THROUGH != 0;
        let n = ins.iter().filter(|&&e| through(e)).count();
        if n == 0 || n == ins.len() {
            return None;
        }
        let first_through = !self.insertions_first;
        order.clear();
        order.extend(ins.iter().copied().filter(|&e| through(e) == first_through));
        let split = order.len();
        order.extend(ins.iter().copied().filter(|&e| through(e) != first_through));
        Some(split)
    }

    /// Buffers, movers, junctions and sinks: conserve each good under a shared capacity, taking
    /// in-edges by priority group (a mover's in-edges in order, side insertion's main line
    /// first, a belt merge's insertions then its through traffic, else one proportional group).
    fn fwd_conserve(&mut self, v: u32, dl: &[Option<u32>]) {
        let g = self.g;
        let kind = self.kind(v);
        let node = g.node(v);
        let cap = self.shared_cap_with(v, dl);
        let sink = kind == NodeKind::Sink;
        // Per-good acceptance downstream.
        self.s_acc.clear();
        if !sink {
            let list = self.s_in.list.clone();
            for gi in list {
                let a = self.out_accept(v, gi);
                self.s_acc.set(gi, a);
            }
        }
        let allowed = |s: &Self, gi: u32| if sink { Q::INF } else { s.s_acc.get(gi) };
        // Priority groups of in-edges.
        let mut order = std::mem::take(&mut self.in_order);
        let merge = self.belt_merge(v, kind, &mut order);
        let ins: &[u32] = if merge.is_some() {
            &order
        } else {
            g.in_g.of(v)
        };
        let mut takes = std::mem::take(&mut self.takes);
        takes.clear();
        let mut remaining = cap;
        let mut i = 0;
        let mut binding = false;
        let mut edges_flowing = 0;
        while i < ins.len() {
            let j = match (merge, kind, node.rule) {
                // A belt merge: two groups, each shared in proportion to supply (GB72).
                (Some(k), _, _) => {
                    if i < k {
                        k
                    } else {
                        ins.len()
                    }
                }
                (None, NodeKind::Mover, _) => i + 1,
                (None, _, rule::SIDE_INSERT) => {
                    let o = g.edge(ins[i]).order;
                    let mut j = i + 1;
                    if o == 0 {
                        while j < ins.len() && g.edge(ins[j]).order == 0 {
                            j += 1;
                        }
                    } else {
                        j = ins.len();
                    }
                    j
                }
                _ => ins.len(),
            };
            let group = &ins[i..j];
            let start = takes.len();
            for &e in group {
                for s in self.slots_of(e) {
                    takes.push((s, self.f[s]));
                }
            }
            // Per good within the group: no more than is still accepted.
            let mut k = start;
            while k < takes.len() {
                let gi = self.slot_good[takes[k].0];
                let total = takes[start..]
                    .iter()
                    .filter(|t| self.slot_good[t.0] == gi)
                    .fold(Q::ZERO, |a, t| a.add(t.1));
                let al = allowed(self, gi);
                if total > al {
                    binding = true;
                    let fr = al.div(total);
                    for t in takes[start..].iter_mut() {
                        if self.slot_good[t.0] == gi {
                            t.1 = t.1.mul(fr);
                        }
                    }
                }
                k += 1;
            }
            let total = takes[start..].iter().fold(Q::ZERO, |a, t| a.add(t.1));
            if total > remaining {
                binding = true;
                let fr = remaining.div(total);
                for t in takes[start..].iter_mut() {
                    t.1 = t.1.mul(fr);
                }
                self.saturated[v as usize] = true;
            }
            let total = takes[start..].iter().fold(Q::ZERO, |a, t| a.add(t.1));
            remaining = remaining.sub_sat(total);
            if !sink {
                for t in &takes[start..] {
                    let gi = self.slot_good[t.0];
                    let a = self.s_acc.get(gi).sub_sat(t.1);
                    self.s_acc.set(gi, a);
                }
            }
            let flowing = takes[start..].iter().filter(|t| t.1.is_pos()).count();
            if binding && flowing > 1 && kind != NodeKind::Mover {
                self.taint_node(v, BOUNDED, why::SHARED_CAPACITY);
            }
            if merge.is_some() {
                let mut k = start;
                for &e in group {
                    let n = self.slots_of(e).len();
                    if takes[k..k + n].iter().any(|t| t.1.is_pos()) {
                        edges_flowing += 1;
                    }
                    k += n;
                }
            }
            i = j;
        }
        // A belt merge that binds with two in-edges flowing: which gets the gaps depends on the
        // ticks of each insertion and of the through traffic, not modelled (GB72).
        if merge.is_some() && binding && edges_flowing > 1 {
            self.taint_node(v, BOUNDED, why::SHARED_CAPACITY);
        }
        if !cap.is_inf() && remaining.is_zero() {
            self.saturated[v as usize] = true;
        }
        // Out per good, pushed by the rule.
        self.s_out.clear();
        for t in &takes {
            self.s_out.add(self.slot_good[t.0], t.1);
        }
        if !sink {
            let outs = self.s_out.list.clone();
            for gi in outs {
                let o = self.s_out.get(gi);
                self.push(v, gi, o);
            }
            self.settle_pushes(v);
        }
        // What was used: the takes, cut where less was pushed.
        for e in ins {
            for s in self.slots_of(*e) {
                self.used[s] = Q::ZERO;
            }
        }
        for t in &takes {
            let gi = self.slot_good[t.0];
            let out = self.s_out.get(gi);
            let u = if sink || out.is_zero() {
                t.1
            } else {
                let p = self.s_push.get(gi);
                if p >= out { t.1 } else { t.1.mul(p).div(out) }
            };
            self.used[t.0] = u;
        }
        self.takes = takes;
        self.in_order = order;
    }

    /// Machines and sources: crafts per recipe, time-shared, limited by inputs, capacity, heat and
    /// (ANY_OUTPUT_BLOCKS) outputs; PASS_THROUGH goods go on unchanged.
    fn fwd_machine(&mut self, v: u32, dl: &[Option<u32>]) {
        // No clone of the duties per machine: they are only read here.
        let duty = std::mem::take(&mut self.duty);
        self.fwd_machine_with(v, dl, &duty);
        self.duty = duty;
    }

    fn fwd_machine_with(&mut self, v: u32, dl: &[Option<u32>], duty: &[Q]) {
        let g = self.g;
        let node = g.node(v);
        let source = node.kind == NodeKind::Source;
        let blocks = node.flags & node_flags::ANY_OUTPUT_BLOCKS != 0 || source;
        let pass = node.flags & node_flags::PASS_THROUGH != 0;
        let recipes = g.node_recipes.of(v);
        // A source without recipes emits its out-edges' goods as accepted, under its capacity.
        if recipes.is_empty() && source {
            let cap = self.shared_cap_with(v, dl);
            self.s_out.clear();
            for &e in g.out_g.of(v) {
                for s in self.slots_of(e) {
                    self.s_out.set(self.slot_good[s], Q::ZERO);
                }
            }
            let outs = self.s_out.list.clone();
            let mut rem = cap;
            for gi in outs {
                let a = self.out_accept(v, gi).min(rem);
                let p = self.push(v, gi, a);
                rem = rem.sub_sat(p);
            }
            if !cap.is_inf() && rem.is_zero() {
                self.saturated[v as usize] = true;
            }
            self.settle_pushes(v);
            self.crafts[v as usize] = cap.sub_sat(rem);
            return;
        }
        // Demand per recipe: capacity, heat, outputs, then inputs.
        self.recipe_limits(v, dl, duty, blocks);
        let caps: Vec<Q> = recipes
            .iter()
            .map(|&r| {
                self.recipe_cap_with(v, r, dl).mul(self.heat_factor_with(
                    v,
                    g.recipes[r as usize].heat,
                    duty,
                ))
            })
            .collect();
        let mut c = std::mem::take(&mut self.rbuf2);
        c.clear();
        for (k, &r) in recipes.iter().enumerate() {
            let mut d = self.rbuf[k];
            for t in &g.recipes[r as usize].inputs {
                if t.flags & term_flags::CATALYST != 0 {
                    continue;
                }
                let amt = term_amount(t.num, t.den);
                if amt.is_zero() {
                    continue;
                }
                let have = self.gidx(t.good).map_or(Q::ZERO, |gi| self.s_in.get(gi));
                d = d.min(have.div(amt));
            }
            c.push(d);
        }
        // Time sharing: Σ c_r / cap_r ≤ 1, water-filled in time.
        let mut share_sum = Q::ZERO;
        for (k, &d) in c.iter().enumerate() {
            if !caps[k].is_inf() && caps[k].is_pos() {
                share_sum = share_sum.add(d.div(caps[k]));
            }
        }
        if share_sum > Q::ONE {
            self.saturated[v as usize] = true;
            let mut shares: Vec<(Q, usize)> = c
                .iter()
                .enumerate()
                .filter(|(k, _)| !caps[*k].is_inf() && caps[*k].is_pos())
                .map(|(k, &d)| (d.div(caps[k]), k))
                .collect();
            shares.sort();
            let mut rem = Q::ONE;
            let mut left = shares.len() as i64;
            for (s, k) in shares {
                let t = s.min(rem.div(Q::int(left)));
                c[k] = t.mul(caps[k]);
                rem = rem.sub_sat(t);
                left -= 1;
            }
            if recipes.len() > 1 {
                self.taint_node(v, BOUNDED, why::TIME_SHARE);
            }
        } else if share_sum == Q::ONE {
            self.saturated[v as usize] = true;
        }
        // Inputs shared between recipes.
        self.share_inputs(v, &mut c);
        // A shared capacity on top of recipe capacities.
        let (shared, per) = match self.level(v, dl) {
            Some(l) => {
                let (s, p) = g.caps_at(v, l);
                (s, p.count())
            }
            None => (None, 0),
        };
        if let Some(sc) = shared
            && per > 0
        {
            let sc = sc.mul(self.scale(v, dl).unwrap_or(Q::ZERO));
            let total = c.iter().fold(Q::ZERO, |a, x| a.add(*x));
            if total > sc {
                let fr = sc.div(total);
                for x in c.iter_mut() {
                    *x = x.mul(fr);
                }
                self.saturated[v as usize] = true;
            }
        }
        // Outputs, pass-through, push; with blocking outputs, cut crafts to what was pushed and
        // push again.
        for attempt in 0..2 {
            self.s_out.clear();
            for (k, &r) in recipes.iter().enumerate() {
                for t in &g.recipes[r as usize].outputs {
                    if let Some(gi) = self.gidx(t.good) {
                        self.s_out.add(gi, c[k].mul(term_amount(t.num, t.den)));
                    }
                }
            }
            if pass {
                let list = self.s_in.list.clone();
                for gi in list {
                    if !self.taken(v, gi) {
                        let a = self.s_in.get(gi);
                        self.s_out.add(gi, a);
                    }
                }
            }
            // Clear the node's out slots, then push each good.
            for &e in g.out_g.of(v) {
                for s in self.slots_of(e) {
                    self.f[s] = Q::ZERO;
                }
            }
            let outs = self.s_out.list.clone();
            for gi in outs {
                let o = self.s_out.get(gi);
                self.push(v, gi, o);
            }
            self.settle_pushes(v);
            if attempt == 1 || !blocks {
                break;
            }
            // Any output short: scale the recipes making it.
            let mut cut = false;
            for (k, &r) in recipes.iter().enumerate() {
                let mut fr = Q::ONE;
                for t in &g.recipes[r as usize].outputs {
                    let Some(gi) = self.gidx(t.good) else {
                        continue;
                    };
                    let o = self.s_out.get(gi);
                    let p = self.s_push.get(gi);
                    if p < o && o.is_pos() {
                        fr = fr.min(p.div(o));
                    }
                }
                if fr < Q::ONE {
                    c[k] = c[k].mul(fr);
                    cut = true;
                }
            }
            if !cut {
                break;
            }
        }
        // Consumption.
        let mut total = Q::ZERO;
        for x in &c {
            total = total.add(*x);
        }
        self.crafts[v as usize] = total;
        // Create rolls every chance output of every craft (`ProcessingOutput.rollOutput`; a
        // sequenced assembly's pool picks one entry a sequence): a random walk the checksum allows
        // for (GB111). A stochastic source's own moment already covers its output.
        let mut cv = std::mem::take(&mut self.chance_var[v as usize]);
        cv.clear();
        if node.flags & node_flags::STOCHASTIC == 0 {
            for (k, &r) in recipes.iter().enumerate() {
                if !c[k].is_pos() || c[k].is_inf() {
                    continue;
                }
                for t in &g.recipes[r as usize].outputs {
                    if t.flags & term_flags::CHANCE == 0 {
                        continue;
                    }
                    if let Some(gi) = self.gidx(t.good) {
                        let var = chance_variance(term_amount(t.num, t.den).to_f64());
                        cv.push((gi, c[k].to_f64() * var));
                    }
                }
            }
        }
        self.chance_var[v as usize] = cv;
        let cons_of = |s: &Self, gi: u32| -> Q {
            let good = s.goods[gi as usize];
            let mut need = Q::ZERO;
            let mut taken = false;
            for (k, &r) in recipes.iter().enumerate() {
                for t in &s.g.recipes[r as usize].inputs {
                    if t.good == good {
                        taken = true;
                        if t.flags & term_flags::CATALYST == 0 {
                            need = need.add(c[k].mul(term_amount(t.num, t.den)));
                        }
                    }
                }
            }
            if !taken && pass {
                need = s.s_push.get(gi);
            }
            need
        };
        self.use_proportional(v, cons_of);
        self.rbuf2 = c;
    }

    /// Whether one of the machine's recipes takes the good (as an input or a catalyst).
    fn taken(&self, v: u32, gi: u32) -> bool {
        let good = self.goods[gi as usize];
        self.g.node_recipes.of(v).iter().any(|&r| {
            self.g.recipes[r as usize]
                .inputs
                .iter()
                .any(|t| t.good == good)
        })
    }

    /// Scales recipes down where their inputs together need more of a good than arrived.
    fn share_inputs(&mut self, v: u32, c: &mut [Q]) {
        let g = self.g;
        let recipes = g.node_recipes.of(v);
        if recipes.len() < 2 {
            return;
        }
        let list = self.s_in.list.clone();
        for gi in list {
            let good = self.goods[gi as usize];
            let mut need = Q::ZERO;
            let mut users = 0;
            for (k, &r) in recipes.iter().enumerate() {
                for t in &g.recipes[r as usize].inputs {
                    if t.good == good && t.flags & term_flags::CATALYST == 0 {
                        need = need.add(c[k].mul(term_amount(t.num, t.den)));
                        if c[k].is_pos() {
                            users += 1;
                        }
                    }
                }
            }
            let have = self.s_in.get(gi);
            if need > have && need.is_pos() {
                let fr = have.div(need);
                for (k, &r) in recipes.iter().enumerate() {
                    if g.recipes[r as usize]
                        .inputs
                        .iter()
                        .any(|t| t.good == good && t.flags & term_flags::CATALYST == 0)
                    {
                        c[k] = c[k].mul(fr);
                    }
                }
                if users > 1 {
                    self.taint_node(v, BOUNDED, why::SHARED_INPUT);
                }
            }
        }
    }

    fn fwd_heater(&mut self, v: u32, dl: &[Option<u32>]) {
        let on = self.scale(v, dl).is_some();
        let creative = self.g.node(v).params[2] <= 0 && self.g.node_recipes.of(v).is_empty();
        let list = self.s_in.list.clone();
        // Fuel per good, time-shared: Σ fuel × burn ≤ 1.
        let mut load = Q::ZERO;
        for &gi in &list {
            let burn = self.burn_ticks(v, self.goods[gi as usize]);
            if burn > 0 && self.is_fuel(v, gi) {
                load = load.add(self.s_in.get(gi).mul(Q::int(burn)));
            }
        }
        let fr = if load > Q::ONE {
            Q::ONE.div(load)
        } else {
            Q::ONE
        };
        // Lit on the burn it had at the compile, whatever fuel it takes (§9.3).
        let duty = if !on {
            Q::ZERO
        } else if creative || self.burning(v) != burn::OUT {
            Q::ONE
        } else {
            load.min(Q::ONE)
        };
        self.fed[v as usize] = if on { load.min(Q::ONE) } else { Q::ZERO };
        self.duty[v as usize] = duty;
        self.saturated[v as usize] = duty == Q::ONE;
        self.use_proportional(v, |s, gi| {
            let burn = s.burn_ticks(v, s.goods[gi as usize]);
            if on && burn > 0 && s.is_fuel(v, gi) {
                s.s_in.get(gi).mul(fr)
            } else {
                Q::ZERO
            }
        });
    }

    /// A boiler voids what it is offered and drives its domain (§9.3, GLASSBOX.md §2.5).
    fn fwd_boiler(&mut self, v: u32) {
        let g = self.g;
        let mut w = Q::ZERO;
        for &gi in &self.s_in.list {
            w = w.add(self.s_in.get(gi));
        }
        self.use_proportional(v, |_, _| Q::INF);
        let Some(d) = g.drives[v as usize] else {
            return;
        };
        let duty = std::mem::take(&mut self.duty);
        let level = self.boiler_level(v, w, &duty, false);
        self.duty = duty;
        self.dom_next[d as usize] = self.select_level(d, level);
        // Heaters at part duty make the boiler's goods bounded (§9.3).
        for &e in g.heat_in.of(v) {
            let h = g.edge(e).from as usize;
            if g.nodes[h].kind == NodeKind::Heater && self.duty[h].is_pos() && self.duty[h] < Q::ONE
            {
                self.taint_node(v, BOUNDED, why::PART_DUTY);
            }
        }
    }

    /// `min(activeHeat, min(18, ceil(w) / 10), min(18, tankBlocks / 4))`, activeHeat summing the
    /// Heat edges of lit heaters (L: at duty 1; `upper`: any duty above 0).
    pub fn boiler_level(&self, v: u32, water: Q, duty: &[Q], upper: bool) -> i64 {
        let g = self.g;
        let mut heat = 0i64;
        for &e in g.heat_in.of(v) {
            let ed = g.edge(e);
            let h = ed.from as usize;
            let lit = if g.nodes[h].kind == NodeKind::Heater {
                if upper {
                    duty[h].is_pos()
                } else {
                    duty[h] >= Q::ONE
                }
            } else {
                true
            };
            if lit {
                heat += ed.param.max(1);
            }
        }
        let water_level = if water.is_inf() {
            18
        } else {
            let n = water.numer();
            let d = water.denom();
            let ceil = if n <= 0 { 0 } else { (n + d - 1) / d };
            (ceil / 10).min(18)
        };
        let tank = g.node(v).params[0];
        let size_level = if tank > 0 { (tank / 4).min(18) } else { 18 };
        heat.min(water_level).min(size_level).max(0)
    }

    /// The domain's level for a driver value: the largest selector at or below it, if it runs.
    pub fn select_level(&self, d: u32, value: i64) -> Option<u32> {
        let dom = &self.g.domains[d as usize];
        let mut best: Option<(u32, u32)> = None;
        for (i, l) in dom.levels.iter().enumerate() {
            if i64::from(l.selector) <= value && best.is_none_or(|b| l.selector > b.0) {
                best = Some((l.selector, i as u32));
            }
        }
        self.running(d, best?.1)
    }

    /// Runs the rounds to the fixed point (or the cap).
    pub fn run(&mut self) -> (u32, bool) {
        let max = self.tun.rounds_per_multi_input * self.g.multi_input + self.tun.rounds_base;
        let max = max.max(1);
        let mut rounds = 0;
        let mut converged = false;
        // Heaters start lit unless gated (the duty the first backward pass assumes), and so does
        // the first forward pass: a machine before its heater in the order (a loop through the
        // heat edge) reads the round before's duty, and a duty of 0 there would cap its inputs
        // at nothing for good (choice G2-24). L only comes down from there.
        for v in 0..self.g.nodes.len() as u32 {
            if self.g.node(v).kind == NodeKind::Heater && self.rg.gate_off[v as usize] {
                self.duty_assumed[v as usize] = Q::ZERO;
            }
        }
        self.duty.clone_from(&self.duty_assumed);
        let mut warming = vec![false; self.g.nodes.len()];
        let mut limit = max;
        // A probe (`as_sink`) never looks for over-fed loops itself.
        let mut stall_checked = !self.loops_stall || !self.as_sink.is_empty();
        while rounds < limit {
            rounds += 1;
            self.backward();
            self.forward();
            let mut changed = false;
            // A back edge's head used what arrived this round: its tail's push of the round
            // before. While that push still changes, the head is warming up (in the first round
            // nothing has come round yet): it may have run short for want of it, so its in-edges
            // are not throttled this round, and another round runs (GB110).
            warming.iter_mut().for_each(|w| *w = false);
            for e in 0..self.g.edges.len() as u32 {
                if !self.g.back[e as usize] {
                    continue;
                }
                if self.slots_of(e).any(|s| self.f[s] != self.back_in[s]) {
                    warming[self.g.edge(e).to as usize] = true;
                    changed = true;
                }
            }
            for e in 0..self.g.edges.len() as u32 {
                let ed = self.g.edge(e);
                if warming[ed.to as usize] {
                    continue;
                }
                let back = self.g.back[e as usize];
                for s in self.slots_of(e) {
                    let arrived = if back { self.back_in[s] } else { self.f[s] };
                    if self.used[s] < arrived && self.used[s] < self.ovr[s] {
                        self.ovr[s] = self.used[s];
                        changed = true;
                    }
                }
            }
            if self.dom_next != self.dom_level {
                self.dom_level = self.dom_next.clone();
                changed = true;
            }
            self.duty_assumed.clone_from(&self.duty);
            if !changed {
                // Settled: shut the over-fed sequenced-assembly loops once (GB72), and run more
                // rounds (a fresh budget) to carry their stall up- and downstream.
                if !stall_checked {
                    stall_checked = true;
                    if self.shut_overfed_loops() {
                        limit = rounds + max;
                        continue;
                    }
                }
                converged = true;
                break;
            }
        }
        (rounds, converged)
    }

    /// GB72 (GLASSBOX.md §3.4): shuts the over-fed sequenced-assembly loops of a settled L and
    /// returns whether it shut any.
    ///
    /// A loop is a goods component holding a SEQUENCED machine (the collapsed line, §9.3), with no
    /// store in it. Create's belt insertions never wait for the items passing (§3.3), so a loop's
    /// feed is what the outside would bring if the loop took everything: a probe solve with the
    /// component as a sink gives it, per good, on the edges into the component. The machine works
    /// at most `μ` of the sequence's base input a tick (its capacity, in base input per craft).
    /// With the machine running flat out (saturated, so no other input is short: a deployer
    /// without its part lets the item pass) and a feed `f > μ` of a base input, the loop's
    /// population grows by `f - μ` a tick until every belt slot is full; then no item moves, and
    /// a stopped item is never processed (`BeltInventory.handleBeltProcessingAndCheckIfRemoved`
    /// returns on `noMovement`) or taken by a funnel (`BeltFunnelInteractionHandler` needs the
    /// item to cross it): a jam for good. Every goods edge into the component is shut
    /// (acceptance 0). `f = μ` runs.
    fn shut_overfed_loops(&mut self) -> bool {
        let g = self.g;
        let n = g.nodes.len();
        let mut shut = false;
        let mut seen = vec![false; n];
        for h in 0..n as u32 {
            if seen[h as usize]
                || !g.cyclic[h as usize]
                || g.node(h).model != model::SEQUENCED
                || self.kind(h) != NodeKind::Machine
                || !self.saturated[h as usize]
            {
                continue;
            }
            let comp = self.goods_component(h);
            for (v, &c) in comp.iter().enumerate() {
                seen[v] |= c;
            }
            if (0..n).any(|v| comp[v] && g.nodes[v].kind == NodeKind::Store) {
                continue;
            }
            let drain = self.sequence_drain(h);
            if drain.iter().all(|&(_, mu)| mu.is_inf()) {
                continue;
            }
            let Some(feed) = self.loop_feed(&comp) else {
                continue;
            };
            if !drain.iter().any(|&(gi, mu)| feed[gi as usize] > mu) {
                continue;
            }
            for (e, ed) in g.edges.iter().enumerate() {
                if carries_goods(ed.kind) && comp[ed.to as usize] {
                    for s in self.slots_of(e as u32) {
                        self.ovr[s] = Q::ZERO;
                    }
                }
            }
            shut = true;
        }
        shut
    }

    /// The nodes on a goods cycle with `h`: reachable from it and reaching it by goods edges.
    fn goods_component(&self, h: u32) -> Vec<bool> {
        let g = self.g;
        let n = g.nodes.len();
        let walk = |forward: bool| {
            let mut mark = vec![false; n];
            let mut stack = vec![h];
            mark[h as usize] = true;
            while let Some(v) = stack.pop() {
                let row = if forward { g.out_g.of(v) } else { g.in_g.of(v) };
                for &e in row {
                    let ed = g.edge(e);
                    let w = if forward { ed.to } else { ed.from };
                    if !mark[w as usize] {
                        mark[w as usize] = true;
                        stack.push(w);
                    }
                }
            }
            mark
        };
        let ahead = walk(true);
        let behind = walk(false);
        ahead.iter().zip(&behind).map(|(a, b)| *a && *b).collect()
    }

    /// What a SEQUENCED machine works of each base input (its sequences' first input) a tick at
    /// its capacity under L, by dense good.
    fn sequence_drain(&self, h: u32) -> Vec<(u32, Q)> {
        let g = self.g;
        let mut out: Vec<(u32, Q)> = Vec::new();
        for &r in g.node_recipes.of(h) {
            let rec = &g.recipes[r as usize];
            if rec.recipe_type != recipe_type::SEQUENCED {
                continue;
            }
            let Some(t) = rec.inputs.first() else {
                continue;
            };
            let Some(gi) = self.gidx(t.good) else {
                continue;
            };
            let cap = self
                .recipe_cap_with(h, r, &self.dom_level)
                .mul(self.heat_factor_with(h, rec.heat, &self.duty));
            let mu = cap.mul(term_amount(t.num, t.den));
            match out.iter_mut().find(|x| x.0 == gi) {
                Some(x) => x.1 = x.1.add(mu),
                None => out.push((gi, mu)),
            }
        }
        out
    }

    /// The probe: per dense good, what the edges into `comp` carry when `comp` takes everything
    /// (each of its nodes a sink, under its own capacity). None if the probe did not settle.
    fn loop_feed(&self, comp: &[bool]) -> Option<Vec<Q>> {
        let g = self.g;
        let mut p = Solver::new(g, self.rg, self.tun);
        p.insertions_first = self.insertions_first;
        p.as_sink = comp.to_vec();
        let (_, converged) = p.run();
        if !converged {
            return None;
        }
        let mut feed = vec![Q::ZERO; p.goods.len()];
        for (e, ed) in g.edges.iter().enumerate() {
            if !carries_goods(ed.kind) || !comp[ed.to as usize] || comp[ed.from as usize] {
                continue;
            }
            for s in p.slots_of(e as u32) {
                let gi = p.slot_good[s] as usize;
                feed[gi] = feed[gi].add(p.used[s]);
            }
        }
        Some(feed)
    }
}

/// Union-find for the classes.
pub(crate) struct Uf {
    p: Vec<u32>,
}

impl Uf {
    pub(crate) fn new(n: usize) -> Uf {
        Uf {
            p: (0..n as u32).collect(),
        }
    }

    pub(crate) fn find(&mut self, mut x: u32) -> u32 {
        while self.p[x as usize] != x {
            let gp = self.p[self.p[x as usize] as usize];
            self.p[x as usize] = gp;
            x = gp;
        }
        x
    }

    pub(crate) fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            self.p[hi as usize] = lo;
        }
    }
}

/// Classes: taint spreads through everything a flow couples (GLASSBOX.md §3.5), stopped by read
/// stores that hold their goods and have room (they decouple their in-side from their out-side).
/// Returns per element (node, or `n + store` for a store's out-side) its component's taint, the
/// union-find, per component whether it holds a cycle, and per element why (the first reason found
/// at the component's worst taint: a `report::why` code and its node or edge).
pub(crate) type Classes = (Vec<u8>, Uf, Vec<bool>, Vec<(u32, u32)>);

pub(crate) fn classes(s: &Solver<'_>, converged: bool) -> Classes {
    let g = s.g;
    let n = g.nodes.len();
    let mut uf = Uf::new(2 * n);
    let out_side = |v: u32| -> u32 {
        if s.kind(v) == NodeKind::Store {
            n as u32 + v
        } else {
            v
        }
    };
    for ed in &g.edges {
        match ed.kind {
            EdgeKind::Heat => uf.union(ed.from, ed.to),
            EdgeKind::Control | EdgeKind::Observe => {}
            _ => uf.union(out_side(ed.from), ed.to),
        }
    }
    for (d, dom) in g.domains.iter().enumerate() {
        let Some(b) = dom.driver else { continue };
        if g.drives[b as usize] != Some(d as u32) {
            continue;
        }
        for m in &dom.members {
            if let Some(v) = m.node {
                uf.union(b, v);
            }
        }
        for (v, node) in g.nodes.iter().enumerate() {
            if node.domain == Some(d as u32) {
                uf.union(b, v as u32);
            }
        }
    }
    for v in 0..n as u32 {
        if s.kind(v) != NodeKind::Store {
            continue;
        }
        let full = !s.rg.room[v as usize];
        let passes = g
            .out_g
            .of(v)
            .iter()
            .any(|&e| s.slots_of(e).any(|sl| !s.held(v, s.slot_good[sl])));
        if full || passes {
            uf.union(v, n as u32 + v);
        }
    }
    let mut taint = vec![EXACT; 2 * n];
    let mut reason = vec![(why::NONE, u32::MAX); 2 * n];
    let mut cyc = vec![false; 2 * n];
    // The first reason at the worst taint wins.
    let put = |uf: &mut Uf,
               taint: &mut Vec<u8>,
               reason: &mut Vec<(u32, u32)>,
               x: u32,
               t: u8,
               w: (u32, u32)| {
        let r = uf.find(x) as usize;
        if t > taint[r] {
            taint[r] = t;
            reason[r] = w;
        }
    };
    for v in 0..n as u32 {
        let node = g.node(v);
        let (mut t, mut w) = (s.taint[v as usize], s.taint_why[v as usize]);
        let mut raise = |level: u8, code: u32| {
            if level > t {
                t = level;
                w = code;
            }
        };
        if node.flags & (node_flags::BOUNDED | node_flags::CALIBRATED) != 0 {
            raise(BOUNDED, why::NODE_FLAG);
        }
        if rule_bounded(node.rule) {
            raise(BOUNDED, why::RULE);
        }
        if g.cyclic[v as usize] {
            raise(BOUNDED, why::CYCLE);
            let r = uf.find(v) as usize;
            cyc[r] = true;
        }
        match s.kind(v) {
            NodeKind::BlackBox => raise(UNBOUNDED, why::BLACK_BOX),
            NodeKind::Port => raise(UNBOUNDED, why::PORT),
            NodeKind::Buffer if node.kind == NodeKind::Store && !g.out_g.of(v).is_empty() => {
                raise(BOUNDED, why::UNREAD_STORE);
            }
            _ => {}
        }
        if priority_tie(s, v) {
            raise(BOUNDED, why::PRIORITY_TIE);
        }
        if t > EXACT {
            put(&mut uf, &mut taint, &mut reason, out_side(v), t, (w, v));
        }
    }
    for (e, ed) in g.edges.iter().enumerate() {
        if !carries_goods(ed.kind) {
            continue;
        }
        let (mut t, mut w) = (EXACT, why::NONE);
        if ed.flags & edge_flags::BOUNDED != 0 {
            (t, w) = (BOUNDED, why::EDGE_FLAG);
        }
        // A depot's line through two stations, and an item dropped onto a collector, are honest
        // about their order or timing only on average (GB37, GB102); one dropped to the floor is
        // simply lost at the dropper's rate.
        if ed.kind == EdgeKind::Line && t < BOUNDED {
            (t, w) = (BOUNDED, why::LINE);
        }
        if ed.kind == EdgeKind::Drop && s.kind(ed.to) != NodeKind::Sink && t < BOUNDED {
            (t, w) = (BOUNDED, why::DROP);
        }
        if !converged && t < BOUNDED {
            for sl in s.slots_of(e as u32) {
                if s.used[sl] < s.f[sl] {
                    (t, w) = (BOUNDED, why::UNCONVERGED);
                }
            }
        }
        if t > EXACT {
            put(&mut uf, &mut taint, &mut reason, ed.to, t, (w, e as u32));
        }
    }
    // Spread to roots.
    let mut out = vec![EXACT; 2 * n];
    let mut outw = vec![(why::NONE, u32::MAX); 2 * n];
    let mut cycles = vec![false; 2 * n];
    for x in 0..2 * n as u32 {
        let r = uf.find(x) as usize;
        if taint[x as usize] > out[r] {
            out[r] = taint[x as usize];
            outw[r] = reason[x as usize];
        }
        cycles[r] |= cyc[x as usize];
    }
    for x in 0..2 * n as u32 {
        let r = uf.find(x) as usize;
        out[x as usize] = out[r];
        outw[x as usize] = outw[r];
        cycles[x as usize] = cycles[r];
    }
    if !converged {
        for (t, w) in out.iter_mut().zip(outw.iter_mut()) {
            if *t < BOUNDED {
                *t = BOUNDED;
                *w = (why::UNCONVERGED, u32::MAX);
            }
        }
    }
    (out, uf, cycles, outw)
}

/// A priority node (rule NONE or PREFER_FIRST) with two goods out-edges of one `order` that can
/// carry a good in common: which goes first is the edge index, not Create (an arm's pull ranks the
/// edge for the arm, the node's own output for the node: a race). Bounded (GB103).
fn priority_tie(s: &Solver<'_>, v: u32) -> bool {
    let g = s.g;
    if s.kind(v) == NodeKind::Store || !matches!(g.node(v).rule, rule::NONE | rule::PREFER_FIRST) {
        return false;
    }
    let out = g.out_g.of(v);
    for (i, &a) in out.iter().enumerate() {
        for &b in &out[i + 1..] {
            if g.edge(a).order != g.edge(b).order {
                continue;
            }
            let shared = s
                .slots_of(a)
                .any(|sa| s.slots_of(b).any(|sb| s.slot_good[sa] == s.slot_good[sb]));
            if shared {
                return true;
            }
        }
    }
    false
}

/// The district's solve: L, classes, then U (`lp.rs`).
pub fn solve(g: &Graph, rg: &Regime, tun: &GlassTunables) -> Solution {
    solve_with(g, rg, tun, BELT_INSERTIONS_FIRST, OVERFED_LOOPS_STALL)
}

/// [`solve`] with GB72's switches given (tests).
pub(crate) fn solve_with(
    g: &Graph,
    rg: &Regime,
    tun: &GlassTunables,
    insertions_first: bool,
    loops_stall: bool,
) -> Solution {
    let mut s = Solver::new(g, rg, tun);
    s.insertions_first = insertions_first;
    s.loops_stall = loops_stall;
    let (rounds, converged) = s.run();
    let (comp_taint, _uf, comp_cycle, comp_why) = classes(&s, converged);
    let n = g.nodes.len();
    let out_side = |v: u32| -> usize {
        if s.kind(v) == NodeKind::Store {
            n + v as usize
        } else {
            v as usize
        }
    };
    // Aggregates over read stores.
    let gn = s.goods.len();
    let mut in_l = vec![Q::ZERO; gn];
    let mut out_l = vec![Q::ZERO; gn];
    let mut cls = vec![EXACT; gn];
    let mut cls_why = vec![(why::NONE, u32::MAX); gn];
    let mut raise = |gi: usize, x: usize, cls: &mut Vec<u8>| {
        if comp_taint[x] > cls[gi] {
            cls[gi] = comp_taint[x];
            cls_why[gi] = comp_why[x];
        }
    };
    let mut on_store = vec![false; gn];
    let mut cyc_good = vec![false; gn];
    let mut b_in = vec![1.0f64; gn];
    let mut b_out = vec![1.0f64; gn];
    let mut per_store: BTreeMap<(u32, Good), Q> = BTreeMap::new();
    let mut per_store_out: BTreeMap<(u32, Good), Q> = BTreeMap::new();
    let mut store_edges = Vec::new();
    for v in 0..n as u32 {
        if s.kind(v) != NodeKind::Store {
            continue;
        }
        let inv = g.node(v).inventory.unwrap_or(u32::MAX);
        for &e in g.in_g.of(v) {
            let from = g.edge(e).from;
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl] as usize;
                let x = s.used[sl];
                in_l[gi] = in_l[gi].add(x);
                on_store[gi] = true;
                raise(gi, out_side(from), &mut cls);
                cyc_good[gi] |= comp_cycle[out_side(from)];
                b_in[gi] = b_in[gi].max(g.batch_units(from, s.goods[gi]));
                let k = (inv, s.goods[gi]);
                per_store.insert(k, per_store.get(&k).copied().unwrap_or(Q::ZERO).add(x));
                store_edges.push((e, s.goods[gi], x.to_f64(), true));
            }
        }
        for &e in g.out_g.of(v) {
            let to = g.edge(e).to;
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl] as usize;
                let x = s.used[sl];
                out_l[gi] = out_l[gi].add(x);
                on_store[gi] = true;
                raise(gi, to as usize, &mut cls);
                cyc_good[gi] |= comp_cycle[to as usize];
                b_out[gi] = b_out[gi].max(g.batch_units(to, s.goods[gi]));
                let k = (inv, s.goods[gi]);
                per_store_out.insert(k, per_store_out.get(&k).copied().unwrap_or(Q::ZERO).add(x));
                store_edges.push((e, s.goods[gi], x.to_f64(), false));
            }
        }
    }
    // Stochastic sources: λ E[X²] per good they emit.
    let mut var_rate = vec![0.0f64; gn];
    for v in 0..n as u32 {
        let node = g.node(v);
        if node.kind != NodeKind::Source
            || node.flags & node_flags::STOCHASTIC == 0
            || node.params[3] <= 0
        {
            continue;
        }
        let ex2 = node.params[2] as f64 / node.params[3] as f64;
        let lambda = s.crafts[v as usize].to_f64();
        let mut emitted: Vec<u32> = Vec::new();
        for &r in g.node_recipes.of(v) {
            for t in &g.recipes[r as usize].outputs {
                if let Some(gi) = s.gidx(t.good) {
                    emitted.push(gi);
                }
            }
        }
        if emitted.is_empty() {
            for &e in g.out_g.of(v) {
                emitted.extend(s.slots_of(e).map(|sl| s.slot_good[sl]));
            }
        }
        emitted.sort_unstable();
        emitted.dedup();
        for gi in emitted {
            if lambda.is_finite() {
                var_rate[gi as usize] += lambda * ex2;
            }
        }
    }
    // Chance outputs: each machine's roll variance, netted by the share of the good's production
    // that reaches the read stores under L (a randomly thinned walk keeps that share of its
    // variance; a good made and consumed upstream adds none) (GB111).
    let mut made = vec![Q::ZERO; gn];
    let mut roll = vec![0.0f64; gn];
    for v in 0..n as u32 {
        if !matches!(s.kind(v), NodeKind::Machine | NodeKind::Source) {
            continue;
        }
        for &(gi, x) in &s.chance_var[v as usize] {
            roll[gi as usize] += x;
        }
        if s.chance_var[v as usize].is_empty() {
            continue;
        }
        // What the node made of each good, chance or not.
        for &e in g.out_g.of(v) {
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl] as usize;
                made[gi] = made[gi].add(s.used[sl]);
            }
        }
    }
    for gi in 0..gn {
        if roll[gi] > 0.0 && made[gi].is_pos() {
            let share = (in_l[gi].to_f64() / made[gi].to_f64()).clamp(0.0, 1.0);
            var_rate[gi] += share * roll[gi];
        }
    }
    let has_unbounded =
        (0..n as u32).any(|v| matches!(s.kind(v), NodeKind::BlackBox | NodeKind::Port));
    // U.
    let mut sol = Solution {
        goods: Vec::new(),
        per_store: Vec::new(),
        rounds,
        converged,
        saturated: s.saturated.clone(),
        edge_flow: (0..g.edges.len() as u32)
            .map(|e| s.slots_of(e).map(|sl| s.used[sl].to_f64()).sum())
            .collect(),
        store_edges,
        edge_goods: (0..g.edges.len() as u32)
            .flat_map(|e| {
                let s = &s;
                s.slots_of(e)
                    .filter(|&sl| !s.used[sl].is_zero())
                    .map(move |sl| (e, s.goods[s.slot_good[sl] as usize], s.used[sl]))
            })
            .collect(),
        dom_level: s.dom_level.clone(),
        duty: s.duty.clone(),
        fed: s.fed.iter().map(|q| q.to_f64()).collect(),
        has_unbounded,
        lp_solves: 0,
        lp_skipped: 0,
        lp_tight: 0,
    };
    let ub = super::lp::upper(&s, (&in_l, &out_l), &cyc_good, &on_store, &cls);
    sol.lp_solves = ub.lp_solves;
    sol.lp_skipped = ub.lp_skipped;
    sol.lp_tight = ub.lp_tight;
    for gi in 0..gn {
        if !on_store[gi] {
            continue;
        }
        let class = u32::from(cls[gi]);
        let (mut in_u, mut out_u) = (ub.in_u[gi], ub.out_u[gi]);
        if class == u32::from(UNBOUNDED) {
            in_u = Q::INF;
            out_u = Q::INF;
        }
        in_u = in_u.max(in_l[gi]);
        out_u = out_u.max(out_l[gi]);
        let (class, why) = if class == u32::from(EXACT) && (in_u.is_inf() || out_u.is_inf()) {
            (u32::from(BOUNDED), (why::U_INFINITE, u32::MAX))
        } else {
            (class, cls_why[gi])
        };
        sol.goods.push(GoodBound {
            good: s.goods[gi],
            class,
            why,
            in_l: in_l[gi],
            out_l: out_l[gi],
            in_u,
            out_u,
            in_u_lp: ub.in_lp[gi],
            out_u_lp: ub.out_lp[gi],
            in_u_lin: ub.in_lin[gi],
            out_u_lin: ub.out_lin[gi],
            var_rate: var_rate[gi],
            batch: b_in[gi] + b_out[gi],
        });
    }
    let mut keys: BTreeSet<(u32, Good)> = per_store.keys().copied().collect();
    keys.extend(per_store_out.keys().copied());
    for k in keys {
        let i = per_store.get(&k).copied().unwrap_or(Q::ZERO).to_f64();
        let o = per_store_out.get(&k).copied().unwrap_or(Q::ZERO).to_f64();
        if k.0 != u32::MAX {
            sol.per_store.push((k.0, k.1, i - o));
        }
    }
    sol
}
