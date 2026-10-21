// SPDX-License-Identifier: AGPL-3.0-or-later
//! A compile's graph, checked and indexed once when it is accepted (docs/industry-boundary.md §9.3):
//! adjacency in compressed rows, the strongly connected components (Tarjan, GLASSBOX.md §3.2 step
//! 1), the order the passes run in, and the longest path latency (§4.4). Everything here is fixed
//! for the compile; what changes per regime lives in `solve.rs`.

use super::q::Q;
use super::records::{
    EdgeKind, GraphDomain, GraphEdge, GraphEnd, GraphNode, GraphRecipe, NodeKind, RateEntry,
    edge_flags, node_flags, rate_flags, term_flags,
};
use crate::ids::Good;

/// Why an assembled compile was refused although its counts matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    EdgeEnd(u32),
    Recipe(u32),
    Domain(u32),
    Member(u32),
    Level(u32),
}

/// Compressed rows: `idx[off[v]..off[v + 1]]` are node `v`'s items.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    pub off: Vec<u32>,
    pub idx: Vec<u32>,
}

impl Rows {
    fn build(n: usize, pairs: &mut [(u32, u64, u32)]) -> Rows {
        // (node, sort key, item)
        pairs.sort_unstable();
        let mut off = vec![0u32; n + 1];
        for &(v, _, _) in pairs.iter() {
            off[v as usize + 1] += 1;
        }
        for i in 0..n {
            off[i + 1] += off[i];
        }
        Rows {
            off,
            idx: pairs.iter().map(|p| p.2).collect(),
        }
    }

    pub fn of(&self, v: u32) -> &[u32] {
        &self.idx[self.off[v as usize] as usize..self.off[v as usize + 1] as usize]
    }
}

/// Whether an edge carries goods (heat, control and observe edges do not).
pub fn carries_goods(k: EdgeKind) -> bool {
    !matches!(k, EdgeKind::Heat | EdgeKind::Control | EdgeKind::Observe)
}

/// One compile, ready to solve.
#[derive(Clone, Debug)]
pub struct Graph {
    pub end: GraphEnd,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub recipes: Vec<GraphRecipe>,
    pub domains: Vec<GraphDomain>,
    /// Goods edges out of and into each node, by `order` then index.
    pub out_g: Rows,
    pub in_g: Rows,
    /// HEAT edges into each node (from heaters and boiler sources of heat).
    pub heat_in: Rows,
    /// CONTROL edges into each node.
    pub control_in: Rows,
    /// OBSERVE edges into each controller.
    pub observe_in: Rows,
    /// Each node's recipes (distinct, ascending), from its rate entries.
    pub node_recipes: Rows,
    /// The domain each boiler drives.
    pub drives: Vec<Option<u32>>,
    /// The passes' order: strongly connected components in topological order, each in the reverse
    /// postorder of a depth-first search inside it.
    pub order: Vec<u32>,
    /// Goods edges inside a component that go back in [`Graph::order`]: L carries nothing on them
    /// (a return loop carries nothing while downstream has room, §3.4); the LP keeps them.
    pub back: Vec<bool>,
    /// Nodes in a component of more than one node, or with an edge to themselves.
    pub cyclic: Vec<bool>,
    pub cycles: u32,
    /// The longest path latency in ticks: edge latencies plus first cycles (§4.4).
    pub longest_latency: u64,
    /// Nodes that couple inputs or regimes: machines with a recipe of two inputs or more, nodes
    /// with two goods in-edges or more, and boilers that drive a domain (`glass.solveRoundsMax`).
    pub multi_input: u32,
}

/// A rate entry's capacity: units (or crafts) per tick.
pub fn entry_rate(e: &RateEntry) -> Q {
    Q::per_ticks(e.batch, e.ticks_num, e.ticks_den)
}

/// The rational amount of a recipe term; a zero or negative denominator counts as nothing.
pub fn term_amount(num: i64, den: i64) -> Q {
    if den <= 0 || num <= 0 {
        Q::ZERO
    } else {
        Q::new(num, den)
    }
}

impl Graph {
    /// Checks every index and builds the graph. Records arrive indexed `0..count` (the store
    /// checked the counts).
    pub fn build(
        end: GraphEnd,
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
        recipes: Vec<GraphRecipe>,
        domains: Vec<GraphDomain>,
    ) -> Result<Graph, Invalid> {
        let n = nodes.len();
        let nn = n as u32;
        for (i, e) in edges.iter().enumerate() {
            if e.from >= nn || e.to >= nn {
                return Err(Invalid::EdgeEnd(i as u32));
            }
        }
        let mut recipe_pairs = Vec::new();
        for (v, node) in nodes.iter().enumerate() {
            if let Some(d) = node.domain
                && d as usize >= domains.len()
            {
                return Err(Invalid::Domain(v as u32));
            }
            for r in &node.rates {
                if let Some(ri) = r.recipe {
                    if ri as usize >= recipes.len() {
                        return Err(Invalid::Recipe(v as u32));
                    }
                    if r.flags & rate_flags::FIRST_CYCLE == 0 {
                        recipe_pairs.push((v as u32, u64::from(ri), ri));
                    }
                }
            }
        }
        recipe_pairs.dedup();
        let mut drives = vec![None; n];
        for (di, d) in domains.iter().enumerate() {
            for m in &d.members {
                if let Some(v) = m.node
                    && v >= nn
                {
                    return Err(Invalid::Member(di as u32));
                }
            }
            if !d.levels.is_empty() && d.current as usize >= d.levels.len() {
                return Err(Invalid::Level(di as u32));
            }
            if let Some(b) = d.driver {
                if b >= nn {
                    return Err(Invalid::Member(di as u32));
                }
                if nodes[b as usize].kind == NodeKind::Boiler {
                    drives[b as usize] = Some(di as u32);
                }
            }
        }
        let key = |e: &GraphEdge, i: usize| (u64::from(e.order) << 32) | i as u64;
        let mut outs = Vec::new();
        let mut ins = Vec::new();
        let mut heat = Vec::new();
        let mut control = Vec::new();
        let mut observe = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            let k = key(e, i);
            match e.kind {
                EdgeKind::Heat => heat.push((e.to, k, i as u32)),
                EdgeKind::Control => control.push((e.to, k, i as u32)),
                EdgeKind::Observe => observe.push((e.to, k, i as u32)),
                _ => {
                    outs.push((e.from, k, i as u32));
                    ins.push((e.to, k, i as u32));
                }
            }
        }
        let out_g = Rows::build(n, &mut outs);
        let in_g = Rows::build(n, &mut ins);
        let heat_in = Rows::build(n, &mut heat);
        let control_in = Rows::build(n, &mut control);
        let observe_in = Rows::build(n, &mut observe);
        let node_recipes = Rows::build(n, &mut recipe_pairs);

        let mut g = Graph {
            end,
            nodes,
            edges,
            recipes,
            domains,
            out_g,
            in_g,
            heat_in,
            control_in,
            observe_in,
            node_recipes,
            drives,
            order: Vec::new(),
            back: Vec::new(),
            cyclic: vec![false; n],
            cycles: 0,
            longest_latency: 0,
            multi_input: 0,
        };
        g.order_and_cycles();
        g.latency();
        g.multi_input = g.count_multi_input();
        Ok(g)
    }

    /// Tarjan's components (iterative), then each component's internal order and back edges.
    fn order_and_cycles(&mut self) {
        let n = self.nodes.len();
        // Successor lists including heat edges (heater -> heated).
        let mut succ_pairs: Vec<(u32, u64, u32)> = Vec::new();
        for (i, e) in self.edges.iter().enumerate() {
            if carries_goods(e.kind) || e.kind == EdgeKind::Heat {
                succ_pairs.push((e.from, i as u64, i as u32));
            }
        }
        let succ_e = Rows::build(n, &mut succ_pairs);
        let next = |e: u32| self.edges[e as usize].to;

        const UNSEEN: u32 = u32::MAX;
        let mut index = vec![UNSEEN; n];
        let mut low = vec![0u32; n];
        let mut on_stack = vec![false; n];
        let mut stack: Vec<u32> = Vec::new();
        let mut comps: Vec<Vec<u32>> = Vec::new();
        let mut counter = 0u32;
        // Call stack of (node, next successor position).
        let mut call: Vec<(u32, u32)> = Vec::new();
        for root in 0..n as u32 {
            if index[root as usize] != UNSEEN {
                continue;
            }
            call.push((root, 0));
            index[root as usize] = counter;
            low[root as usize] = counter;
            counter += 1;
            stack.push(root);
            on_stack[root as usize] = true;
            while let Some(&mut (v, ref mut pos)) = call.last_mut() {
                let row = succ_e.of(v);
                if (*pos as usize) < row.len() {
                    let w = next(row[*pos as usize]);
                    *pos += 1;
                    if index[w as usize] == UNSEEN {
                        index[w as usize] = counter;
                        low[w as usize] = counter;
                        counter += 1;
                        stack.push(w);
                        on_stack[w as usize] = true;
                        call.push((w, 0));
                    } else if on_stack[w as usize] {
                        low[v as usize] = low[v as usize].min(index[w as usize]);
                    }
                } else {
                    call.pop();
                    if let Some(&(u, _)) = call.last() {
                        low[u as usize] = low[u as usize].min(low[v as usize]);
                    }
                    if low[v as usize] == index[v as usize] {
                        let mut comp = Vec::new();
                        loop {
                            let w = stack.pop().expect("on stack");
                            on_stack[w as usize] = false;
                            comp.push(w);
                            if w == v {
                                break;
                            }
                        }
                        comps.push(comp);
                    }
                }
            }
        }
        // Tarjan emits components in reverse topological order.
        comps.reverse();
        let mut comp_of = vec![0u32; n];
        for (ci, c) in comps.iter().enumerate() {
            for &v in c {
                comp_of[v as usize] = ci as u32;
            }
        }
        let mut order = Vec::with_capacity(n);
        let mut pos = vec![0u32; n];
        let mut seen = vec![false; n];
        for c in &comps {
            let self_loop = c.len() == 1 && succ_e.of(c[0]).iter().any(|&e| next(e) == c[0]);
            if c.len() == 1 {
                if self_loop {
                    self.cyclic[c[0] as usize] = true;
                    self.cycles += 1;
                }
                pos[c[0] as usize] = order.len() as u32;
                order.push(c[0]);
                continue;
            }
            self.cycles += 1;
            let ci = comp_of[c[0] as usize];
            // Enter at the lowest-indexed node fed from outside the component, else the lowest.
            let mut members = c.clone();
            members.sort_unstable();
            let entry = members
                .iter()
                .copied()
                .find(|&v| {
                    self.in_g
                        .of(v)
                        .iter()
                        .any(|&e| comp_of[self.edges[e as usize].from as usize] != ci)
                })
                .unwrap_or(members[0]);
            // Depth-first postorder inside the component, from the entry, then the rest.
            let mut post: Vec<u32> = Vec::with_capacity(c.len());
            let mut dfs: Vec<(u32, u32)> = Vec::new();
            for start in std::iter::once(entry).chain(members.iter().copied()) {
                if seen[start as usize] {
                    continue;
                }
                seen[start as usize] = true;
                dfs.push((start, 0));
                while let Some(&mut (v, ref mut p)) = dfs.last_mut() {
                    let row = succ_e.of(v);
                    if (*p as usize) < row.len() {
                        let w = next(row[*p as usize]);
                        *p += 1;
                        if comp_of[w as usize] == ci && !seen[w as usize] {
                            seen[w as usize] = true;
                            dfs.push((w, 0));
                        }
                    } else {
                        post.push(v);
                        dfs.pop();
                    }
                }
            }
            for &v in post.iter().rev() {
                self.cyclic[v as usize] = true;
                pos[v as usize] = order.len() as u32;
                order.push(v);
            }
        }
        self.back = self
            .edges
            .iter()
            .map(|e| {
                carries_goods(e.kind)
                    && comp_of[e.from as usize] == comp_of[e.to as usize]
                    && pos[e.to as usize] <= pos[e.from as usize]
            })
            .collect();
        // A heat edge inside a component is not cut: the heater's duty is then the previous
        // round's (`solve.rs`).
        self.order = order;
    }

    /// The longest path latency over the order (back edges dropped).
    fn latency(&mut self) {
        let n = self.nodes.len();
        let mut lat = vec![0u64; n];
        let mut best = 0u64;
        for &v in &self.order {
            let first = self.nodes[v as usize]
                .rates
                .iter()
                .filter(|r| r.flags & rate_flags::FIRST_CYCLE != 0 && r.ticks_den != 0)
                .map(|r| u64::from(r.ticks_num).div_ceil(u64::from(r.ticks_den)))
                .max()
                .unwrap_or(0);
            let here = lat[v as usize] + first;
            best = best.max(here);
            for &e in self.out_g.of(v) {
                if self.back[e as usize] {
                    continue;
                }
                let ed = &self.edges[e as usize];
                let t = here + u64::from(ed.latency);
                let w = ed.to as usize;
                if t > lat[w] {
                    lat[w] = t;
                }
            }
        }
        self.longest_latency = best;
    }

    fn count_multi_input(&self) -> u32 {
        let mut count = 0;
        for (v, node) in self.nodes.iter().enumerate() {
            let v = v as u32;
            let multi_recipe = node.kind == NodeKind::Machine
                && self.node_recipes.of(v).iter().any(|&r| {
                    self.recipes[r as usize]
                        .inputs
                        .iter()
                        .filter(|t| t.flags & term_flags::CATALYST == 0)
                        .count()
                        >= 2
                });
            let multi_in = self.in_g.of(v).len() >= 2;
            let driver = self.drives[v as usize].is_some();
            if multi_recipe || multi_in || driver {
                count += 1;
            }
        }
        count
    }

    pub fn node(&self, v: u32) -> &GraphNode {
        &self.nodes[v as usize]
    }

    pub fn edge(&self, e: u32) -> &GraphEdge {
        &self.edges[e as usize]
    }

    /// Whether `e` lets `g` through (an empty allow list allows everything).
    pub fn allows(&self, e: u32, g: Good) -> bool {
        let ed = &self.edges[e as usize];
        let listed = ed.filter.contains(&g);
        if ed.flags & edge_flags::DENY != 0 {
            !listed
        } else {
            ed.filter.is_empty() || listed
        }
    }

    /// The edge's own capacity in units per tick (`+∞` when unlimited).
    pub fn edge_cap(&self, e: u32) -> Q {
        match self.edges[e as usize].capacity {
            None => Q::INF,
            Some((num, den)) => Q::ratio_u64(u64::from(num), u64::from(den)),
        }
    }

    /// Whether the node holds fluids (its stock is counted in mB).
    pub fn is_fluid(&self, v: u32) -> bool {
        self.nodes[v as usize].flags & node_flags::FLUID != 0
    }

    /// Whether the node has any rate entry that is not a first cycle (else it is unlimited).
    pub fn has_rates(&self, v: u32) -> bool {
        self.nodes[v as usize]
            .rates
            .iter()
            .any(|r| r.flags & rate_flags::FIRST_CYCLE == 0)
    }

    /// The node's capacity at `level` (the domain level index; 0 outside a domain): the shared
    /// entry (SHARED, or no recipe) and each recipe's own entry. Several entries for the same key
    /// keep the fastest. `None` where there is no such entry.
    pub fn caps_at(&self, v: u32, level: u32) -> (Option<Q>, impl Iterator<Item = (u32, Q)> + '_) {
        let rates = &self.nodes[v as usize].rates;
        let mut shared: Option<Q> = None;
        for r in rates {
            if r.flags & rate_flags::FIRST_CYCLE != 0 || r.level != level {
                continue;
            }
            if r.flags & rate_flags::SHARED != 0 || r.recipe.is_none() {
                let c = entry_rate(r);
                shared = Some(shared.map_or(c, |s| s.max(c)));
            }
        }
        let per = rates.iter().filter(move |r| {
            r.flags & rate_flags::FIRST_CYCLE == 0
                && r.level == level
                && r.flags & rate_flags::SHARED == 0
                && r.recipe.is_some()
        });
        (
            shared,
            per.map(|r| (r.recipe.expect("filtered"), entry_rate(r))),
        )
    }

    /// The largest batch the node moves or crafts in one operation, in units of `g` (at least 1):
    /// the checksum's quantization (§5.3, q = b / W).
    pub fn batch_units(&self, v: u32, g: Good) -> f64 {
        let node = &self.nodes[v as usize];
        let mut b: f64 = 1.0;
        for r in &node.rates {
            if r.flags & rate_flags::FIRST_CYCLE != 0 {
                continue;
            }
            match r.recipe {
                Some(ri) => {
                    let rec = &self.recipes[ri as usize];
                    for t in rec.inputs.iter().chain(rec.outputs.iter()) {
                        if t.good == g && t.den > 0 {
                            b = b.max((f64::from(r.batch) * t.num as f64 / t.den as f64).ceil());
                        }
                    }
                }
                None => b = b.max(f64::from(r.batch)),
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glass::records::{GraphHead, NONE};
    use crate::ids::{BlockPos, DistrictId};

    fn node(i: u32) -> GraphNode {
        GraphNode {
            head: GraphHead {
                district: DistrictId(1),
                compile: 1,
                index: i,
            },
            kind: NodeKind::Junction,
            model: 0,
            domain: None,
            inventory: None,
            flags: 0,
            pos: BlockPos { x: 0, y: 0, z: 0 },
            rule: 0,
            capacity: 0,
            params: [0; 4],
            rates: Vec::new(),
        }
    }

    fn edge(i: u32, from: u32, to: u32, latency: u32) -> GraphEdge {
        GraphEdge {
            head: GraphHead {
                district: DistrictId(1),
                compile: 1,
                index: i,
            },
            kind: EdgeKind::Belt,
            from,
            to,
            order: 0,
            flags: 0,
            capacity: None,
            latency,
            param: 0,
            filter: Vec::new(),
        }
    }

    fn end(nodes: u32, edges: u32) -> GraphEnd {
        GraphEnd {
            district: DistrictId(1),
            compile: 1,
            flags: 1,
            window: 0,
            records: nodes + edges,
            nodes,
            edges,
            recipes: 0,
            domains: 0,
            reason: 0,
            fingerprint: 0,
            frame: 0,
            revision: 1,
            compile_ticks: 0,
        }
    }

    #[test]
    fn a_loop_is_one_component_with_one_back_edge() {
        // 0 -> 1 -> 2 -> 1 (return), 2 -> 3
        let nodes = (0..4).map(node).collect();
        let edges = vec![
            edge(0, 0, 1, 5),
            edge(1, 1, 2, 7),
            edge(2, 2, 1, 3),
            edge(3, 2, 3, 1),
        ];
        let g = Graph::build(end(4, 4), nodes, edges, vec![], vec![]).expect("valid");
        assert_eq!(g.order, vec![0, 1, 2, 3]);
        assert_eq!(g.back, vec![false, false, true, false]);
        assert_eq!(g.cyclic, vec![false, true, true, false]);
        assert_eq!(g.cycles, 1);
        assert_eq!(g.longest_latency, 13);
        let _ = NONE;
    }

    #[test]
    fn bad_indices_are_refused() {
        let nodes = (0..2).map(node).collect();
        let edges = vec![edge(0, 0, 5, 0)];
        assert_eq!(
            Graph::build(end(2, 1), nodes, edges, vec![], vec![]).err(),
            Some(Invalid::EdgeEnd(0))
        );
    }
}
