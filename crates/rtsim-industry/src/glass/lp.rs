// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ceiling U (GLASSBOX.md §3.5, G-B15): the most each good could flow into and out of the read
//! stores with every routing choice free. Nothing Create can do beats it.
//!
//! Two bounds, both valid, and U is the smaller (then never below L):
//! - **The linear relaxation:** one backward and one forward pass where every junction may send its
//!   whole supply down every branch and every recipe may use the whole machine. O(slots); loose
//!   where routing matters. Goods through a cycle get `+∞` here.
//! - **The LP** (microlp 0.6.0, choice GB12): per good and direction, the maximum over the cone of
//!   nodes that can carry it (ancestors of the stores' in-edges, or descendants of their out-edges).
//!   Edges leaving the cone are free (a relaxation: still a bound). Cones above
//!   `glass.lpVarsMax` variables are left to the linear bound.
//!
//! microlp solves in f64: its optimum is raised by `glass.lpMarginRel` and rounded up.

use std::collections::BTreeMap;

use microlp::{ComparisonOp, LinearExpr, OptimizationDirection, Problem, Variable};

use super::graph::term_amount;
use super::q::Q;
use super::records::{NodeKind, node_flags, term_flags};
use super::solve::{Solver, UNBOUNDED, burn};

/// One LP constraint before it is handed to microlp: terms, sense, right-hand side.
type Row = (Vec<(u32, f64)>, ComparisonOp, f64);

/// The bounds per dense good.
pub struct Upper {
    pub in_u: Vec<Q>,
    pub out_u: Vec<Q>,
    pub in_lp: Vec<Option<f64>>,
    pub out_lp: Vec<Option<f64>>,
    pub in_lin: Vec<Q>,
    pub out_lin: Vec<Q>,
    pub lp_solves: u32,
    /// LPs not run: more than `glass.lpVarsMax` variables (the linear bound stands).
    pub lp_skipped: u32,
    /// LPs not needed: the linear bound already equals L.
    pub lp_tight: u32,
}

/// The U-side capacities: every domain level a node could run at in the regime, heaters at the
/// most duty they could have.
pub(crate) struct UCaps {
    /// Per domain: the levels it could run at (L's level, and for a boiler-driven domain every
    /// level its boiler could reach with its heat and tank).
    levels: Vec<Vec<u32>>,
    /// Per node: a heater's most duty.
    pub duty: Vec<Q>,
}

impl UCaps {
    fn new(s: &Solver<'_>) -> UCaps {
        let g = s.g;
        let n = g.nodes.len();
        let mut levels: Vec<Vec<u32>> = s
            .dom_level
            .iter()
            .map(|l| l.iter().copied().collect())
            .collect();
        // Heaters that could be lit at all count toward a boiler's heat.
        let maybe: Vec<Q> = (0..n as u32)
            .map(|v| {
                if g.node(v).kind == NodeKind::Heater && !s.rg.gate_off[v as usize] {
                    Q::ONE
                } else {
                    Q::ZERO
                }
            })
            .collect();
        for (b, d) in g.drives.iter().enumerate() {
            let Some(d) = *d else { continue };
            let max = s.boiler_level(b as u32, Q::INF, &maybe, true);
            for (i, l) in g.domains[d as usize].levels.iter().enumerate() {
                if i64::from(l.selector) <= max && s.running(d, i as u32).is_some() {
                    levels[d as usize].push(i as u32);
                }
            }
            levels[d as usize].sort_unstable();
            levels[d as usize].dedup();
        }
        UCaps {
            levels,
            duty: vec![Q::ONE; n],
        }
    }

    fn level_sets(&self, s: &Solver<'_>, v: u32) -> Vec<Option<u32>> {
        match s.g.node(v).domain {
            None => vec![Some(0)],
            Some(d) => {
                let ls = &self.levels[d as usize];
                if ls.is_empty() {
                    vec![None]
                } else {
                    ls.iter().map(|&l| Some(l)).collect()
                }
            }
        }
    }

    /// The node's shared capacity, the most over its possible levels.
    fn shared(&self, s: &Solver<'_>, v: u32) -> Q {
        let mut best = Q::ZERO;
        for l in self.level_sets(s, v) {
            best = best.max(s.shared_cap_at(v, l));
        }
        best
    }

    /// A recipe's capacity with heat, the most over its possible levels.
    fn recipe(&self, s: &Solver<'_>, v: u32, r: u32) -> Q {
        let mut best = Q::ZERO;
        for l in self.level_sets(s, v) {
            best = best.max(s.recipe_cap_at(v, r, l));
        }
        let heat = s.g.recipes[r as usize].heat;
        best.mul_up(s.heat_factor_with(v, heat, &self.duty))
    }
}

/// The linear relaxation: per slot, an upper bound on its flow.
fn linear(s: &Solver<'_>, caps: &mut UCaps) -> Vec<Q> {
    let g = s.g;
    let ns = s.f.len();
    let mut acc = vec![Q::INF; ns];
    let mut fu = vec![Q::ZERO; ns];
    let gn = s.goods.len();
    let mut a = vec![Q::ZERO; gn];
    let mut seen = vec![false; gn];
    let mut list: Vec<u32> = Vec::new();
    let sum_out = |acc: &[Q], v: u32, gi: u32| -> Q {
        let mut t = Q::ZERO;
        for &e in g.out_g.of(v) {
            if let Some(sl) = s.slot_of(e, gi) {
                t = t.add_up(acc[sl]);
            }
        }
        t
    };
    // Backward: acceptance, routing free.
    for i in (0..g.order.len()).rev() {
        let v = g.order[i];
        list.clear();
        for &e in g.in_g.of(v) {
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl];
                if !seen[gi as usize] {
                    seen[gi as usize] = true;
                    list.push(gi);
                }
            }
        }
        match s.kind(v) {
            NodeKind::Store => {
                let room = s.rg.room[v as usize];
                for &gi in &list {
                    a[gi as usize] = if room { Q::INF } else { sum_out(&acc, v, gi) };
                }
            }
            NodeKind::Buffer | NodeKind::Mover | NodeKind::Junction => {
                let c = caps.shared(s, v);
                for &gi in &list {
                    a[gi as usize] = sum_out(&acc, v, gi).min(c);
                }
            }
            NodeKind::Sink => {
                let c = caps.shared(s, v);
                for &gi in &list {
                    a[gi as usize] = c;
                }
            }
            NodeKind::Heater => {
                let on = !s.rg.gate_off[v as usize] && s.rg.burning[v as usize] != burn::REFUSING;
                for &gi in &list {
                    let burn = s.burn_ticks(v, s.goods[gi as usize]);
                    a[gi as usize] = if on && burn > 0 {
                        Q::new(1, burn)
                    } else {
                        Q::ZERO
                    };
                }
            }
            NodeKind::Machine => {
                let node = g.node(v);
                let blocks = node.flags & node_flags::ANY_OUTPUT_BLOCKS != 0;
                let pass = node.flags & node_flags::PASS_THROUGH != 0;
                for &gi in &list {
                    a[gi as usize] = Q::ZERO;
                }
                for &r in g.node_recipes.of(v) {
                    let rec = &g.recipes[r as usize];
                    let mut c = caps.recipe(s, v, r);
                    if blocks {
                        for t in &rec.outputs {
                            let amt = term_amount(t.num, t.den);
                            if amt.is_zero() {
                                continue;
                            }
                            let oa = s.gidx(t.good).map_or(Q::ZERO, |gi| sum_out(&acc, v, gi));
                            c = c.min(oa.mul_up(Q::ONE).div(amt).add_up(Q::ZERO));
                        }
                    }
                    for t in &rec.inputs {
                        if t.flags & term_flags::CATALYST != 0 {
                            continue;
                        }
                        if let Some(gi) = s.gidx(t.good)
                            && seen[gi as usize]
                        {
                            a[gi as usize] =
                                a[gi as usize].add_up(c.mul_up(term_amount(t.num, t.den)));
                        }
                    }
                }
                if pass {
                    for &gi in &list {
                        if !taken(s, v, gi) {
                            a[gi as usize] = sum_out(&acc, v, gi);
                        }
                    }
                }
            }
            // Boilers void; black boxes and ports take anything (their goods are unbounded).
            NodeKind::Boiler | NodeKind::BlackBox | NodeKind::Port => {
                for &gi in &list {
                    a[gi as usize] = Q::INF;
                }
            }
            _ => {
                for &gi in &list {
                    a[gi as usize] = Q::ZERO;
                }
            }
        }
        for &e in g.in_g.of(v) {
            let cap = g.edge_cap(e);
            for sl in s.slots_of(e) {
                acc[sl] = cap.min(a[s.slot_good[sl] as usize]);
            }
        }
        for &gi in &list {
            seen[gi as usize] = false;
        }
    }
    // Forward: supply, every branch may take the whole of it.
    let mut inn = vec![Q::ZERO; gn];
    let mut out = vec![Q::ZERO; gn];
    for &v in &g.order {
        list.clear();
        for &e in g.in_g.of(v) {
            if g.back[e as usize] {
                continue;
            }
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl];
                if !seen[gi as usize] {
                    seen[gi as usize] = true;
                    inn[gi as usize] = Q::ZERO;
                    list.push(gi);
                }
                inn[gi as usize] = inn[gi as usize].add_up(fu[sl]);
            }
        }
        let kind = s.kind(v);
        let node = g.node(v);
        // Out per good (default: nothing).
        let mut outs: Vec<(u32, Q)> = Vec::new();
        match kind {
            NodeKind::Store => {
                for &e in g.out_g.of(v) {
                    for sl in s.slots_of(e) {
                        let gi = s.slot_good[sl];
                        let sup = if s.rg.held[v as usize]
                            .binary_search(&s.goods[gi as usize])
                            .is_ok()
                        {
                            Q::INF
                        } else if seen[gi as usize] {
                            inn[gi as usize]
                        } else {
                            Q::ZERO
                        };
                        outs.push((gi, sup));
                    }
                }
            }
            NodeKind::Buffer | NodeKind::Mover | NodeKind::Junction => {
                let c = caps.shared(s, v);
                for &gi in &list {
                    outs.push((gi, inn[gi as usize].min(c)));
                }
            }
            NodeKind::Heater => {
                let mut load = Q::ZERO;
                for &gi in &list {
                    let burn = s.burn_ticks(v, s.goods[gi as usize]);
                    if burn > 0 {
                        load = load.add_up(inn[gi as usize].mul_up(Q::int(burn)));
                    }
                }
                let creative = node.params[2] <= 0 && g.node_recipes.of(v).is_empty();
                caps.duty[v as usize] = if s.rg.gate_off[v as usize] {
                    Q::ZERO
                } else if creative || s.rg.burning[v as usize] != burn::OUT || g.cyclic[v as usize]
                {
                    Q::ONE
                } else {
                    load.min(Q::ONE)
                };
            }
            NodeKind::Machine | NodeKind::Source => {
                let blocks = node.flags & node_flags::ANY_OUTPUT_BLOCKS != 0
                    || node.kind == NodeKind::Source;
                let pass = node.flags & node_flags::PASS_THROUGH != 0;
                for &gi in &list {
                    out[gi as usize] = Q::ZERO;
                }
                let mut touched: Vec<u32> = Vec::new();
                let recipes = g.node_recipes.of(v);
                if recipes.is_empty() && node.kind == NodeKind::Source {
                    let c = caps.shared(s, v);
                    for &e in g.out_g.of(v) {
                        for sl in s.slots_of(e) {
                            outs.push((s.slot_good[sl], c));
                        }
                    }
                }
                for &r in recipes {
                    let rec = &g.recipes[r as usize];
                    let mut c = caps.recipe(s, v, r);
                    for t in &rec.inputs {
                        if t.flags & term_flags::CATALYST != 0 {
                            continue;
                        }
                        let amt = term_amount(t.num, t.den);
                        if amt.is_zero() {
                            continue;
                        }
                        let have = s.gidx(t.good).map_or(Q::ZERO, |gi| {
                            if seen[gi as usize] {
                                inn[gi as usize]
                            } else {
                                Q::ZERO
                            }
                        });
                        c = c.min(div_up(have, amt));
                    }
                    if blocks {
                        for t in &rec.outputs {
                            let amt = term_amount(t.num, t.den);
                            if amt.is_zero() {
                                continue;
                            }
                            let oa = s.gidx(t.good).map_or(Q::ZERO, |gi| sum_out(&acc, v, gi));
                            c = c.min(div_up(oa, amt));
                        }
                    }
                    for t in &rec.outputs {
                        if let Some(gi) = s.gidx(t.good) {
                            if !touched.contains(&gi) {
                                touched.push(gi);
                                if !seen[gi as usize] {
                                    out[gi as usize] = Q::ZERO;
                                }
                            }
                            out[gi as usize] =
                                out[gi as usize].add_up(c.mul_up(term_amount(t.num, t.den)));
                        }
                    }
                }
                if pass {
                    for &gi in &list {
                        if !taken(s, v, gi) {
                            if !touched.contains(&gi) {
                                touched.push(gi);
                            }
                            out[gi as usize] = out[gi as usize].add_up(inn[gi as usize]);
                        }
                    }
                }
                for gi in touched {
                    outs.push((gi, out[gi as usize]));
                }
            }
            NodeKind::BlackBox | NodeKind::Port => {
                for &e in g.out_g.of(v) {
                    for sl in s.slots_of(e) {
                        outs.push((s.slot_good[sl], Q::INF));
                    }
                }
            }
            _ => {}
        }
        for (gi, sup) in outs {
            for &e in g.out_g.of(v) {
                if let Some(sl) = s.slot_of(e, gi) {
                    fu[sl] = acc[sl].min(sup);
                }
            }
        }
        for &gi in &list {
            seen[gi as usize] = false;
        }
    }
    fu
}

fn div_up(a: Q, b: Q) -> Q {
    if b.is_zero() {
        return if a.is_pos() { Q::INF } else { Q::ZERO };
    }
    if a.is_inf() {
        return Q::INF;
    }
    a.mul_up(Q::new(b.denom(), b.numer()))
}

fn taken(s: &Solver<'_>, v: u32, gi: u32) -> bool {
    let good = s.goods[gi as usize];
    s.g.node_recipes.of(v).iter().any(|&r| {
        s.g.recipes[r as usize]
            .inputs
            .iter()
            .any(|t| t.good == good)
    })
}

/// The cone of one good and direction: which nodes get constraints. `into`: ancestors of the read
/// stores' in-edges carrying the good, stopping at stores that hold everything they send on; else
/// descendants of their out-edges, stopping at stores with room.
fn cone(
    s: &Solver<'_>,
    gi: u32,
    into: bool,
    mark: &mut [bool],
    seen: &mut [bool],
    list: &mut Vec<u32>,
) {
    let g = s.g;
    list.clear();
    let mut queue: Vec<u32> = Vec::new();
    let add = |v: u32, seen: &mut [bool], queue: &mut Vec<u32>| {
        if !seen[v as usize] {
            seen[v as usize] = true;
            queue.push(v);
        }
    };
    let holds = |v: u32, gk: u32| {
        s.rg.held[v as usize]
            .binary_search(&s.goods[gk as usize])
            .is_ok()
    };
    for v in 0..g.nodes.len() as u32 {
        if s.kind(v) != NodeKind::Store {
            continue;
        }
        if into {
            for &e in g.in_g.of(v) {
                if s.slot_of(e, gi).is_some() {
                    add(g.edge(e).from, seen, &mut queue);
                }
            }
        } else {
            for &e in g.out_g.of(v) {
                if s.slot_of(e, gi).is_some() {
                    add(g.edge(e).to, seen, &mut queue);
                    // A store that does not hold the good passes on what it receives.
                    if !holds(v, gi) {
                        add(v, seen, &mut queue);
                    }
                }
            }
        }
    }
    let mut i = 0;
    while i < queue.len() {
        let v = queue[i];
        i += 1;
        let store = s.kind(v) == NodeKind::Store;
        if into {
            if store {
                let passes = g
                    .out_g
                    .of(v)
                    .iter()
                    .any(|&e| s.slots_of(e).any(|sl| !holds(v, s.slot_good[sl])));
                if !passes {
                    // Holds all it sends: a free supply, no constraint, nothing above it.
                    continue;
                }
            }
            mark[v as usize] = true;
            list.push(v);
            for &e in g.in_g.of(v) {
                add(g.edge(e).from, seen, &mut queue);
            }
            for &e in g.heat_in.of(v) {
                add(g.edge(e).from, seen, &mut queue);
            }
        } else {
            if store && s.rg.room[v as usize] && holds(v, gi) {
                continue;
            }
            if store && s.rg.room[v as usize] {
                // A store with room takes anything: a free sink, unless it is the source store
                // passing the good on (added above), which keeps its constraint.
                let source_side = g.out_g.of(v).iter().any(|&e| s.slot_of(e, gi).is_some());
                if !source_side {
                    continue;
                }
            }
            mark[v as usize] = true;
            list.push(v);
            for &e in g.out_g.of(v) {
                add(g.edge(e).to, seen, &mut queue);
            }
        }
    }
    for &v in &queue {
        seen[v as usize] = false;
    }
}

/// Builds and solves one LP over the cone (`mark`, `list`), maximizing the flow of `gi` on the
/// read stores' in-edges (`into`) or out-edges. None when it is too big or microlp fails; `+∞` when
/// unbounded.
fn lp(
    s: &Solver<'_>,
    caps: &UCaps,
    gi: u32,
    into: bool,
    mark: &[bool],
    list: &[u32],
    var_of: &mut [u32],
) -> Option<f64> {
    let g = s.g;
    const NO: u32 = u32::MAX;
    let is_store = |v: u32| s.kind(v) == NodeKind::Store;
    // Edges with an end in the part (objective edges with no end in the cone are summed by the
    // caller, [`free_objective`]).
    let mut edges: Vec<u32> = Vec::new();
    for &v in list {
        edges.extend_from_slice(g.in_g.of(v));
        edges.extend_from_slice(g.out_g.of(v));
    }
    edges.sort_unstable();
    edges.dedup();
    let mut nvars = 0usize;
    for &e in &edges {
        nvars += s.slots_of(e).len();
    }
    for &v in list {
        nvars += g.node_recipes.of(v).len();
    }
    if nvars > s.tun.lp_vars_max {
        return None;
    }
    // Slot variables: (objective coefficient, bounds).
    let mut defs: Vec<(f64, (f64, f64))> = Vec::with_capacity(nvars);
    let mut used_slots: Vec<usize> = Vec::new();
    for &e in &edges {
        let ed = g.edge(e);
        let objective = if into {
            is_store(ed.to)
        } else {
            is_store(ed.from)
        };
        let dead = matches!(s.kind(ed.to), NodeKind::BlackBox | NodeKind::Port)
            || matches!(s.kind(ed.from), NodeKind::BlackBox | NodeKind::Port);
        for sl in s.slots_of(e) {
            let obj = if objective && s.slot_good[sl] == gi {
                1.0
            } else {
                0.0
            };
            var_of[sl] = defs.len() as u32;
            used_slots.push(sl);
            defs.push((
                obj,
                if dead {
                    (0.0, 0.0)
                } else {
                    (0.0, f64::INFINITY)
                },
            ));
        }
    }
    let mut cons: Vec<Row> = Vec::new();
    let mut terms: BTreeMap<u32, f64> = BTreeMap::new();
    let push = |cons: &mut Vec<Row>, terms: &mut BTreeMap<u32, f64>, op: ComparisonOp, rhs: f64| {
        terms.retain(|_, c| *c != 0.0);
        if !terms.is_empty() {
            cons.push((terms.iter().map(|(&i, &c)| (i, c)).collect(), op, rhs));
        }
        terms.clear();
    };
    let net = |terms: &mut BTreeMap<u32, f64>, ins: &[u32], outs: &[u32], si: f64, so: f64| {
        for &x in ins {
            *terms.entry(x).or_insert(0.0) += si;
        }
        for &x in outs {
            *terms.entry(x).or_insert(0.0) += so;
        }
    };
    // Edge capacities.
    for &e in &edges {
        let cap = g.edge_cap(e);
        if cap.is_inf() {
            continue;
        }
        for sl in s.slots_of(e) {
            terms.insert(var_of[sl], 1.0);
        }
        push(&mut cons, &mut terms, ComparisonOp::Le, cap.to_f64());
    }
    for &v in list {
        debug_assert!(mark[v as usize]);
        let node = g.node(v);
        // Per good: in and out slot variables.
        let mut per: BTreeMap<u32, (Vec<u32>, Vec<u32>)> = BTreeMap::new();
        for &e in g.in_g.of(v) {
            for sl in s.slots_of(e) {
                per.entry(s.slot_good[sl]).or_default().0.push(var_of[sl]);
            }
        }
        for &e in g.out_g.of(v) {
            for sl in s.slots_of(e) {
                per.entry(s.slot_good[sl]).or_default().1.push(var_of[sl]);
            }
        }
        match s.kind(v) {
            NodeKind::Store => {
                for (&gk, (ins, outs)) in &per {
                    let held = s.rg.held[v as usize]
                        .binary_search(&s.goods[gk as usize])
                        .is_ok();
                    if !held {
                        net(&mut terms, ins, outs, -1.0, 1.0);
                        push(&mut cons, &mut terms, ComparisonOp::Le, 0.0);
                    }
                    if !s.rg.room[v as usize] {
                        net(&mut terms, ins, outs, 1.0, -1.0);
                        push(&mut cons, &mut terms, ComparisonOp::Le, 0.0);
                    }
                }
            }
            NodeKind::Buffer | NodeKind::Mover | NodeKind::Junction | NodeKind::Sink => {
                if s.kind(v) != NodeKind::Sink {
                    for (ins, outs) in per.values() {
                        net(&mut terms, ins, outs, 1.0, -1.0);
                        push(&mut cons, &mut terms, ComparisonOp::Eq, 0.0);
                    }
                }
                let c = caps.shared(s, v);
                if !c.is_inf() {
                    for (ins, _) in per.values() {
                        net(&mut terms, ins, &[], 1.0, 0.0);
                    }
                    push(&mut cons, &mut terms, ComparisonOp::Le, c.to_f64());
                }
            }
            NodeKind::Heater => {
                let mut load: BTreeMap<u32, f64> = BTreeMap::new();
                for (&gk, (ins, _)) in &per {
                    let burn = s.burn_ticks(v, s.goods[gk as usize]);
                    let takes =
                        !s.rg.gate_off[v as usize] && s.rg.burning[v as usize] != burn::REFUSING;
                    if burn > 0 && takes {
                        net(&mut load, ins, &[], burn as f64, 0.0);
                    } else {
                        net(&mut terms, ins, &[], 1.0, 0.0);
                        push(&mut cons, &mut terms, ComparisonOp::Le, 0.0);
                    }
                }
                push(&mut cons, &mut load, ComparisonOp::Le, 1.0);
            }
            NodeKind::Boiler => {}
            NodeKind::Machine | NodeKind::Source => {
                let source = node.kind == NodeKind::Source;
                let blocks = node.flags & node_flags::ANY_OUTPUT_BLOCKS != 0;
                let pass = node.flags & node_flags::PASS_THROUGH != 0;
                let recipes = g.node_recipes.of(v);
                if recipes.is_empty() {
                    if source {
                        let c = caps.shared(s, v);
                        if !c.is_inf() {
                            for (_, outs) in per.values() {
                                net(&mut terms, &[], outs, 0.0, 1.0);
                            }
                            push(&mut cons, &mut terms, ComparisonOp::Le, c.to_f64());
                        }
                    } else {
                        // A machine without recipes passes nothing.
                        for (ins, outs) in per.values() {
                            net(&mut terms, ins, outs, 1.0, 1.0);
                        }
                        push(&mut cons, &mut terms, ComparisonOp::Le, 0.0);
                    }
                    continue;
                }
                let rv: Vec<u32> = recipes
                    .iter()
                    .map(|_| {
                        defs.push((0.0, (0.0, f64::INFINITY)));
                        defs.len() as u32 - 1
                    })
                    .collect();
                // Time share: Σ c_r / cap_r ≤ 1.
                for (k, &r) in recipes.iter().enumerate() {
                    let c = caps.recipe(s, v, r);
                    if c.is_zero() {
                        defs[rv[k] as usize].1 = (0.0, 0.0);
                    } else if !c.is_inf() {
                        terms.insert(rv[k], 1.0 / c.to_f64());
                    }
                }
                push(&mut cons, &mut terms, ComparisonOp::Le, 1.0);
                // A shared capacity over recipe capacities.
                if let Some(l) = s.dom_level_of(v) {
                    let (shared, per_r) = g.caps_at(v, l);
                    if let Some(c) = shared
                        && per_r.count() > 0
                    {
                        let c = caps.shared(s, v).max(c);
                        if !c.is_inf() {
                            for &x in &rv {
                                terms.insert(x, 1.0);
                            }
                            push(&mut cons, &mut terms, ComparisonOp::Le, c.to_f64());
                        }
                    }
                }
                let mut goods: Vec<u32> = per.keys().copied().collect();
                for &r in recipes {
                    let rec = &g.recipes[r as usize];
                    for t in rec.inputs.iter().chain(rec.outputs.iter()) {
                        if let Some(x) = s.gidx(t.good) {
                            goods.push(x);
                        }
                    }
                }
                goods.sort_unstable();
                goods.dedup();
                for gk in goods {
                    let good = s.goods[gk as usize];
                    let (ins, outs) = per.get(&gk).cloned().unwrap_or_default();
                    let pv = (pass && !taken(s, v, gk) && !ins.is_empty()).then(|| {
                        defs.push((0.0, (0.0, f64::INFINITY)));
                        defs.len() as u32 - 1
                    });
                    // Inputs: arrivals = recipe use (+ pass-through).
                    if !source {
                        net(&mut terms, &ins, &[], 1.0, 0.0);
                        for (k, &r) in recipes.iter().enumerate() {
                            for t in &g.recipes[r as usize].inputs {
                                if t.good == good && t.flags & term_flags::CATALYST == 0 {
                                    *terms.entry(rv[k]).or_insert(0.0) -=
                                        term_amount(t.num, t.den).to_f64();
                                }
                            }
                        }
                        if let Some(p) = pv {
                            *terms.entry(p).or_insert(0.0) -= 1.0;
                        }
                        push(&mut cons, &mut terms, ComparisonOp::Eq, 0.0);
                    }
                    // Outputs: pushed = (or ≤) made (+ pass-through).
                    net(&mut terms, &[], &outs, 0.0, 1.0);
                    for (k, &r) in recipes.iter().enumerate() {
                        for t in &g.recipes[r as usize].outputs {
                            if t.good == good {
                                *terms.entry(rv[k]).or_insert(0.0) -=
                                    term_amount(t.num, t.den).to_f64();
                            }
                        }
                    }
                    if let Some(p) = pv {
                        *terms.entry(p).or_insert(0.0) -= 1.0;
                    }
                    let op = if blocks && !source {
                        ComparisonOp::Eq
                    } else {
                        ComparisonOp::Le
                    };
                    push(&mut cons, &mut terms, op, 0.0);
                }
            }
            _ => {
                // Black boxes, ports, controllers: nothing flows (their edges are fixed at 0).
            }
        }
    }
    for sl in used_slots {
        var_of[sl] = NO;
    }
    if defs.len() > s.tun.lp_vars_max {
        return None;
    }
    let mut p = Problem::new(OptimizationDirection::Maximize);
    let vars: Vec<Variable> = defs.iter().map(|(o, b)| p.add_var(*o, *b)).collect();
    for (t, op, rhs) in cons {
        let expr: LinearExpr = t.iter().map(|&(i, c)| (vars[i as usize], c)).collect();
        p.add_constraint(expr, op, rhs);
    }
    match p.solve() {
        Ok(outcome) => match outcome.into_solution() {
            Ok(sol) => Some(sol.objective()),
            Err(_) => None,
        },
        Err(microlp::Error::Unbounded) => Some(f64::INFINITY),
        Err(_) => None,
    }
}

/// The objective edges of a good with neither end in the cone (a store handing straight to a
/// store): each carries at most its capacity.
fn free_objective(s: &Solver<'_>, gi: u32, into: bool, mark: &[bool]) -> f64 {
    let g = s.g;
    let mut total = 0.0;
    for v in 0..g.nodes.len() as u32 {
        if s.kind(v) != NodeKind::Store {
            continue;
        }
        let row = if into { g.in_g.of(v) } else { g.out_g.of(v) };
        for &e in row {
            let ed = g.edge(e);
            if s.slot_of(e, gi).is_none() || mark[ed.from as usize] || mark[ed.to as usize] {
                continue;
            }
            if matches!(s.kind(ed.to), NodeKind::BlackBox | NodeKind::Port)
                || matches!(s.kind(ed.from), NodeKind::BlackBox | NodeKind::Port)
            {
                continue;
            }
            let cap = g.edge_cap(e);
            total += if cap.is_inf() {
                f64::INFINITY
            } else {
                cap.to_f64()
            };
        }
    }
    total
}

fn find(p: &mut [u32], mut x: u32) -> u32 {
    while p[x as usize] != x {
        let up = p[p[x as usize] as usize];
        p[x as usize] = up;
        x = up;
    }
    x
}

/// The LP of one good and direction, one per connected part of its cone (goods and heat edges
/// inside the cone join nodes; stores outside it are free and join nothing), summed. Independent
/// lines of a district solve as small LPs, so the cost stays linear in their number. None when a
/// part is too big or microlp fails.
#[allow(clippy::too_many_arguments)]
fn lp_parts(
    s: &Solver<'_>,
    caps: &UCaps,
    gi: u32,
    into: bool,
    mark: &mut [bool],
    list: &mut [u32],
    var_of: &mut [u32],
    parent: &mut [u32],
) -> Option<f64> {
    let g = s.g;
    let mut total = free_objective(s, gi, into, mark);
    for &v in list.iter() {
        parent[v as usize] = v;
    }
    for &v in list.iter() {
        for &e in g.in_g.of(v).iter().chain(g.heat_in.of(v)) {
            let u = g.edge(e).from;
            if mark[u as usize] {
                let (a, b) = (find(parent, u), find(parent, v));
                if a != b {
                    parent[a.max(b) as usize] = a.min(b);
                }
            }
        }
    }
    let mut keyed: Vec<(u32, u32)> = list.iter().map(|&v| (find(parent, v), v)).collect();
    keyed.sort_unstable();
    for &v in list.iter() {
        mark[v as usize] = false;
    }
    let mut out = Some(0.0);
    let mut i = 0;
    while i < keyed.len() {
        let root = keyed[i].0;
        let mut j = i;
        while j < keyed.len() && keyed[j].0 == root {
            j += 1;
        }
        let part: Vec<u32> = keyed[i..j].iter().map(|&(_, v)| v).collect();
        for &v in &part {
            mark[v as usize] = true;
        }
        let r = lp(s, caps, gi, into, mark, &part, var_of);
        for &v in &part {
            mark[v as usize] = false;
        }
        match r {
            None => {
                out = None;
                break;
            }
            Some(x) => total += x,
        }
        i = j;
    }
    // The caller unmarks the cone: mark it again as it was.
    for &v in list.iter() {
        mark[v as usize] = true;
    }
    out.map(|_| total)
}

/// U per dense good, for the goods on read-store edges whose class is not unbounded.
pub fn upper(
    s: &Solver<'_>,
    (in_l, out_l): (&[Q], &[Q]),
    cyc_good: &[bool],
    on_store: &[bool],
    cls: &[u8],
) -> Upper {
    let g = s.g;
    let gn = s.goods.len();
    let mut caps = UCaps::new(s);
    let fu = linear(s, &mut caps);
    let mut in_lin = vec![Q::ZERO; gn];
    let mut out_lin = vec![Q::ZERO; gn];
    for v in 0..g.nodes.len() as u32 {
        if s.kind(v) != NodeKind::Store {
            continue;
        }
        for &e in g.in_g.of(v) {
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl] as usize;
                in_lin[gi] = in_lin[gi].add_up(fu[sl]);
            }
        }
        for &e in g.out_g.of(v) {
            for sl in s.slots_of(e) {
                let gi = s.slot_good[sl] as usize;
                out_lin[gi] = out_lin[gi].add_up(fu[sl]);
            }
        }
    }
    for gi in 0..gn {
        if cyc_good[gi] {
            in_lin[gi] = Q::INF;
            out_lin[gi] = Q::INF;
        }
    }
    let mut up = Upper {
        in_u: in_lin.clone(),
        out_u: out_lin.clone(),
        in_lp: vec![None; gn],
        out_lp: vec![None; gn],
        in_lin,
        out_lin,
        lp_solves: 0,
        lp_skipped: 0,
        lp_tight: 0,
    };
    let mut mark = vec![false; g.nodes.len()];
    let mut seen = vec![false; g.nodes.len()];
    let mut list = Vec::new();
    let mut var_of = vec![u32::MAX; s.f.len()];
    let mut parent = vec![0u32; g.nodes.len()];
    let margin = s.tun.lp_margin_rel;
    for gi in 0..gn as u32 {
        if !on_store[gi as usize] || cls[gi as usize] == UNBOUNDED {
            continue;
        }
        for into in [true, false] {
            // The linear bound already meets L: U is L, the LP cannot change it (U = max(L,
            // min(LP, linear))). Most goods of a factory without routing slack end here.
            let (lin, l) = if into {
                (up.in_lin[gi as usize], in_l[gi as usize])
            } else {
                (up.out_lin[gi as usize], out_l[gi as usize])
            };
            if lin <= l {
                up.lp_tight += 1;
                continue;
            }
            cone(s, gi, into, &mut mark, &mut seen, &mut list);
            let r = lp_parts(
                s,
                &caps,
                gi,
                into,
                &mut mark,
                &mut list,
                &mut var_of,
                &mut parent,
            );
            for &v in &list {
                mark[v as usize] = false;
            }
            match r {
                None => up.lp_skipped += 1,
                Some(x) => {
                    up.lp_solves += 1;
                    let raised = if x.is_finite() {
                        x.max(0.0) * (1.0 + margin) + 1e-9
                    } else {
                        x
                    };
                    let q = Q::ceil_f64(raised);
                    let slot = if into {
                        &mut up.in_u[gi as usize]
                    } else {
                        &mut up.out_u[gi as usize]
                    };
                    *slot = (*slot).min(q);
                    if into {
                        up.in_lp[gi as usize] = Some(x);
                    } else {
                        up.out_lp[gi as usize] = Some(x);
                    }
                }
            }
        }
    }
    up
}
