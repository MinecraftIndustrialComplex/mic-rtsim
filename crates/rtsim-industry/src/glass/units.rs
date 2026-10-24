// SPDX-License-Identifier: AGPL-3.0-or-later
//! Judging units: a compile's sections as the checksum judges them (SECTIONS.md §3.4, §4.1 to §4.3;
//! SEC-1 slice S2b).
//!
//! A unit is a judging group (§4.3): the sections at a shared store whose moves are not counted are
//! judged on one row, with that store interior. A unit's own stores give `Y(u,g)` their summed stock
//! less the ports into them, and `P(u,g)` the solve's predictions on them (`Solution.per_store`), as
//! the district's are over every read store.
//!
//! **Counted stores** (§4.1, §4.2, slice S3): a shared store whose moves Java counts at its movers
//! (`Sections::counted`) and whose sections fall in two units or more is no unit's. Each unit at it
//! is judged on what its own movers put in and took out there (IND_COUNT_SAMPLE), and on the
//! store's edges to its movers in the solve ([`bounds`]). The pairs Java counts are every goods edge
//! between such a store and its mover ([`Units::pairs`], IND_COUNT_SET).
//!
//! **A station's ports** (§4.4, choice SC75): the inventories one station's interfaces reach
//! (IND_STATION_PORTS) take its train cargo, which cannot be split between them, so the units
//! holding them are one unit, judged on one row with those stores interior.
//!
//! **One unit or none** judges nothing of its own: the district window is that unit's window (§3.4,
//! "today's verdict for districts with one section"), so a district of one line holds no extra rows.
//!
//! **A unit's bounds** ([`bounds`], choice SC56): L and the class are its own store edges' (the solve's
//! class of the component past each edge, which never crosses a store; at a counted store, the edges
//! to its own movers); U is the district's less
//! what every other store takes at least, `U(u) = U(d) - (L(d) - L(u))`, never below `L(u)`;
//! infinite when the district's is, or when the unit's own edges for the good are unbounded (past a
//! black box or a port, by the solve's class). An exact unit is judged on L alone, so the
//! district's infinite U makes it bounded only when the district's own L was exact
//! (`why::U_INFINITE`): another unit's black box does not loosen it. Its variance is its own
//! stochastic sources' and its own machines' rolls, netted by the share of what they made that
//! reaches its stores; its batch is the largest on its own store edges.
//!
//! Everything here is derived from the compile, its sections and its solve: never saved.

use std::collections::BTreeMap;

use super::graph::Graph;
use super::report::{class, why};
use super::sections::{NONE, Sections, Uf, extent, is_virtual};
use super::solve::Solution;
use crate::ids::{BlockPos, Good, VecMap};

/// Chunks a residual group's extent may span before its unit counts as never live on a partial
/// sweep (a bound on the liveness test's work; Java's groups lie inside one district).
const EXTENT_CHUNKS_MAX: i64 = 256;

/// A compile's judging units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Units {
    /// Per section: its unit.
    pub of_section: Vec<u16>,
    /// Per node: its unit (a store's when its sections are one unit's); [`NONE`] for idle nodes,
    /// the void sink and stores no unit holds.
    pub of_node: Vec<u16>,
    /// Per unit: its sections, ascending.
    pub sections: Vec<Vec<u16>>,
    /// Per unit: the inventories of its stores, ascending, each once.
    pub stores: Vec<Vec<u32>>,
    /// Each inventory a unit's node holds or claims, and that unit.
    pub of_inv: VecMap<u32, u16>,
    /// Per unit: a black box or a port among its nodes (its goods start unbounded).
    pub unbounded: Vec<bool>,
    /// Per unit: the chunks (x, z) its nodes and stores lie in, ascending; None when they cannot
    /// be listed (a moving frame, or an extent past [`EXTENT_CHUNKS_MAX`]).
    pub chunks: Vec<Option<Vec<(i32, i32)>>>,
    /// Per unit: what a recompile matches it by, its sections' fingerprints ascending.
    pub keys: Vec<Vec<u64>>,
    /// The counted stores between units (§4.2), ascending: a counted store whose sections all
    /// fall in one unit (joined at another store, or by a station's ports) is that unit's.
    pub counted: Vec<u32>,
    /// Every (mover, store) pair Java counts: a goods edge between a store of [`Units::counted`]
    /// and another node, by its far node, ascending, each once.
    pub pairs: Vec<(u32, u32)>,
    /// Per pair of [`Units::pairs`]: the directions of Java's counts it takes ([`PAIR_IN`],
    /// [`PAIR_OUT`]). Java keys its counters by the two blocks' positions, so the pairs of movers
    /// at one block (a packager's packing and unpacking nodes) get the same counts: each
    /// direction goes to the lowest of them with an edge that way, else the lowest (choice SC94).
    pub pair_dirs: Vec<u8>,
    /// Per unit: the stores of [`Units::counted`] one of its nodes has a pair at, ascending.
    pub counted_of: Vec<Vec<u32>>,
    /// Per unit: every pair at those stores, its own and the other units', ascending: what its
    /// window needs counted in a sweep (the others' make the stores' residue).
    pub pairs_of: Vec<Vec<(u32, u32)>>,
    /// Each counted store's inventory, and the store.
    pub counted_inv: VecMap<u32, u32>,
}

/// A pair takes Java's counts into the store ([`Units::pair_dirs`]).
pub const PAIR_IN: u8 = 1;
/// A pair takes Java's counts out of the store.
pub const PAIR_OUT: u8 = 2;

/// [`Units::pair_dirs`] of `pairs` (ascending): per (mover block, store), each direction to the
/// lowest pair with an edge that way (into the store: mover to store), else to the lowest.
fn pair_dirs(g: &Graph, pairs: &[(u32, u32)]) -> Vec<u8> {
    let mut dirs = vec![0u8; pairs.len()];
    let mut at: BTreeMap<(BlockPos, u32), Vec<usize>> = BTreeMap::new();
    for (i, &(m, h)) in pairs.iter().enumerate() {
        at.entry((g.node(m).pos, h)).or_default().push(i);
    }
    for idx in at.values() {
        let has = |i: usize, dir: u8| {
            let (m, h) = pairs[i];
            if dir == PAIR_IN {
                g.in_g.of(h).iter().any(|&e| g.edge(e).from == m)
            } else {
                g.out_g.of(h).iter().any(|&e| g.edge(e).to == m)
            }
        };
        for dir in [PAIR_IN, PAIR_OUT] {
            let i = idx.iter().copied().find(|&i| has(i, dir)).unwrap_or(idx[0]);
            dirs[i] |= dir;
        }
    }
    dirs
}

impl Units {
    /// The units of `g` cut into `s`, the groups holding the inventories of each of `joins` (a
    /// station's ports, SC75) joined into one.
    pub fn build(g: &Graph, s: &Sections, joins: &[Vec<u32>]) -> Units {
        let groups = join_groups(g, s, s.groups(), joins);
        let nu = groups.len();
        let mut of_section = vec![NONE; s.len()];
        for (u, gr) in groups.iter().enumerate() {
            for &x in gr {
                of_section[x as usize] = u as u16;
            }
        }
        let mut of_node = vec![NONE; g.nodes.len()];
        for (si, sec) in s.list.iter().enumerate() {
            let u = of_section[si];
            for &v in &sec.nodes {
                of_node[v as usize] = u;
            }
        }
        // A store is the unit's when every section touching it is in that unit: in S2 always.
        let mut store_units: BTreeMap<u32, Option<u16>> = BTreeMap::new();
        for (si, sec) in s.list.iter().enumerate() {
            let u = of_section[si];
            for &h in &sec.stores {
                store_units
                    .entry(h)
                    .and_modify(|x| {
                        if *x != Some(u) {
                            *x = None;
                        }
                    })
                    .or_insert(Some(u));
            }
        }
        for (&h, &u) in &store_units {
            if let Some(u) = u {
                of_node[h as usize] = u;
            }
        }
        // Inventories: a node holding one names its unit; two units on one inventory, none.
        let mut inv: BTreeMap<u32, Option<u16>> = BTreeMap::new();
        for (v, n) in g.nodes.iter().enumerate() {
            let (Some(i), u) = (n.inventory, of_node[v]) else {
                continue;
            };
            if u == NONE {
                continue;
            }
            inv.entry(i)
                .and_modify(|x| {
                    if *x != Some(u) {
                        *x = None;
                    }
                })
                .or_insert(Some(u));
        }
        let mut of_inv = VecMap::new();
        for (&i, &u) in &inv {
            if let Some(u) = u {
                of_inv.insert(i, u);
            }
        }
        let mut stores = vec![Vec::new(); nu];
        for (&h, &u) in &store_units {
            let Some(u) = u else { continue };
            if let Some(i) = g.node(h).inventory
                && of_inv.get(&i) == Some(&u)
            {
                stores[u as usize].push(i);
            }
        }
        for st in stores.iter_mut() {
            st.sort_unstable();
            st.dedup();
        }
        let mut unbounded = vec![false; nu];
        for (v, n) in g.nodes.iter().enumerate() {
            let u = of_node[v];
            if u != NONE
                && matches!(
                    n.kind,
                    super::records::NodeKind::BlackBox | super::records::NodeKind::Port
                )
            {
                unbounded[u as usize] = true;
            }
        }
        let moving = g.end.frame != 0;
        let mut chunks: Vec<Option<Vec<(i32, i32)>>> = vec![(!moving).then(Vec::new); nu];
        for (v, n) in g.nodes.iter().enumerate() {
            let u = of_node[v];
            if u == NONE || is_virtual(n) {
                continue;
            }
            let Some(list) = chunks[u as usize].as_mut() else {
                continue;
            };
            let Some((lo, hi)) = extent(n) else { continue };
            let (x0, x1, z0, z1) = (lo.x >> 4, hi.x >> 4, lo.z >> 4, hi.z >> 4);
            if (i64::from(x1) - i64::from(x0) + 1) * (i64::from(z1) - i64::from(z0) + 1)
                > EXTENT_CHUNKS_MAX
            {
                chunks[u as usize] = None;
                continue;
            }
            for x in x0..=x1 {
                for z in z0..=z1 {
                    list.push((x, z));
                }
            }
        }
        for c in chunks.iter_mut().flatten() {
            c.sort_unstable();
            c.dedup();
        }
        let keys = groups
            .iter()
            .map(|gr| {
                let mut k: Vec<u64> = gr.iter().map(|&x| s.list[x as usize].fingerprint).collect();
                k.sort_unstable();
                k
            })
            .collect();
        // Counted stores whose sections are two units' or more (the others are a unit's own).
        let counted: Vec<u32> = s
            .counted
            .iter()
            .copied()
            .filter(|h| store_units.get(h).is_some_and(|u| u.is_none()))
            .collect();
        let mut pairs = Vec::new();
        let mut counted_of = vec![Vec::new(); nu];
        let mut counted_inv = VecMap::new();
        for &h in &counted {
            let far = g.out_g.of(h).iter().map(|&e| g.edge(e).to);
            let near = g.in_g.of(h).iter().map(|&e| g.edge(e).from);
            for m in far.chain(near).filter(|&m| m != h) {
                pairs.push((m, h));
                let u = of_node[m as usize];
                if u != NONE {
                    counted_of[u as usize].push(h);
                }
            }
            if let Some(i) = g.node(h).inventory
                && counted_inv.get(&i).is_none()
            {
                counted_inv.insert(i, h);
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        let pair_dirs = pair_dirs(g, &pairs);
        for c in counted_of.iter_mut() {
            c.sort_unstable();
            c.dedup();
        }
        let pairs_of = counted_of
            .iter()
            .map(|hs| {
                pairs
                    .iter()
                    .copied()
                    .filter(|(_, h)| hs.binary_search(h).is_ok())
                    .collect()
            })
            .collect();
        Units {
            of_section,
            of_node,
            sections: groups,
            stores,
            of_inv,
            unbounded,
            chunks,
            keys,
            counted,
            pairs,
            pair_dirs,
            counted_of,
            pairs_of,
            counted_inv,
        }
    }

    /// The counted store whose inventory is `i`, if any (§4.2).
    pub fn counted_store_of_inv(&self, i: u32) -> Option<u32> {
        self.counted_inv.get(&i).copied()
    }

    pub fn len(&self) -> usize {
        self.sections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Two units or more: each keeps its own window. With one or none the district window judges.
    pub fn active(&self) -> bool {
        self.sections.len() >= 2
    }

    /// The unit of inventory `i`, if a unit holds it.
    pub fn unit_of_inv(&self, i: u32) -> Option<u16> {
        self.of_inv.get(&i).copied()
    }

    /// Does unit `u` run in the world, by `runs(chunk x, z)`? A unit whose chunks cannot be listed
    /// never does on a partial sweep.
    pub fn runs(&self, u: u16, runs: &dyn Fn(i32, i32) -> bool) -> bool {
        self.chunks[u as usize]
            .as_ref()
            .is_some_and(|c| c.iter().all(|&(x, z)| runs(x, z)))
    }

    /// The units whose sections' node boxes hold `p` (§3.6).
    pub fn at(&self, s: &Sections, p: BlockPos) -> Vec<u16> {
        let mut out: Vec<u16> = s
            .at(p)
            .into_iter()
            .map(|x| self.of_section[x as usize])
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// One unit's bound for one good under a solve (choice SC56).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitBound {
    pub unit: u16,
    pub good: Good,
    pub class: u32,
    pub why: (u32, u32),
    pub in_l: f64,
    pub out_l: f64,
    pub in_u: f64,
    pub out_u: f64,
    /// `λ E[X²]` per tick of its own sources and rolls.
    pub var: f64,
    /// The largest batch into plus out of its stores.
    pub batch: f64,
}

/// `groups` (each ascending, ordered by its lowest section) with those holding an inventory of one
/// of `joins` joined: a section holds an inventory when one of its nodes or stores does.
fn join_groups(
    g: &Graph,
    s: &Sections,
    groups: Vec<Vec<u16>>,
    joins: &[Vec<u32>],
) -> Vec<Vec<u16>> {
    if joins.is_empty() || groups.len() < 2 {
        return groups;
    }
    let mut group_of = vec![0u32; s.len()];
    for (k, gr) in groups.iter().enumerate() {
        for &x in gr {
            group_of[x as usize] = k as u32;
        }
    }
    let mut holders: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (si, sec) in s.list.iter().enumerate() {
        for &v in sec.nodes.iter().chain(sec.stores.iter()) {
            if let Some(i) = g.node(v).inventory {
                holders.entry(i).or_default().push(group_of[si]);
            }
        }
    }
    let mut uf = Uf::new(groups.len());
    let mut joined = false;
    for set in joins {
        let mut first: Option<u32> = None;
        for gr in set.iter().filter_map(|i| holders.get(i)).flatten() {
            match first {
                None => first = Some(*gr),
                Some(f) if uf.find(f) != uf.find(*gr) => {
                    uf.join(f, *gr);
                    joined = true;
                }
                Some(_) => {}
            }
        }
    }
    if !joined {
        return groups;
    }
    let mut by: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    for (k, gr) in groups.into_iter().enumerate() {
        by.entry(uf.find(k as u32)).or_default().extend(gr);
    }
    let mut out: Vec<Vec<u16>> = by
        .into_values()
        .map(|mut gr| {
            gr.sort_unstable();
            gr
        })
        .collect();
    out.sort_unstable_by_key(|gr| gr[0]);
    out
}

/// Every unit's bounds under `sol`, by (unit, good); empty unless the units are [`Units::active`].
pub fn bounds(g: &Graph, units: &Units, sol: &Solution) -> Vec<UnitBound> {
    if !units.active() {
        return Vec::new();
    }
    struct Agg {
        il: f64,
        ol: f64,
        class: u32,
        why: (u32, u32),
        b_in: f64,
        b_out: f64,
        roll: f64,
        made: f64,
        var: f64,
    }
    let mut agg: BTreeMap<(u16, Good), Agg> = BTreeMap::new();
    let fresh = || Agg {
        il: 0.0,
        ol: 0.0,
        class: class::EXACT,
        why: (why::NONE, u32::MAX),
        b_in: 1.0,
        b_out: 1.0,
        roll: 0.0,
        made: 0.0,
        var: 0.0,
    };
    for se in &sol.store_edges {
        let ed = g.edge(se.edge);
        let (store, far) = if se.into {
            (ed.to, ed.from)
        } else {
            (ed.from, ed.to)
        };
        // At a counted store between units, the edge is its mover's unit's: what that unit's
        // movers put in and take out there is its own (§4.1).
        let mut u = units.of_node[store as usize];
        if u == NONE && units.counted.binary_search(&store).is_ok() {
            u = units.of_node[far as usize];
        }
        if u == NONE {
            continue;
        }
        let a = agg.entry((u, se.good)).or_insert_with(fresh);
        let b = g.batch_units(far, se.good);
        if se.into {
            a.il += se.flow;
            a.b_in = a.b_in.max(b);
        } else {
            a.ol += se.flow;
            a.b_out = a.b_out.max(b);
        }
        if se.class > a.class {
            a.class = se.class;
            a.why = se.why;
        }
    }
    for &(v, good, x) in &sol.src_var {
        let u = units.of_node[v as usize];
        if let Some(a) = agg.get_mut(&(u, good)) {
            a.var += x;
        }
    }
    for &(v, good, roll, made) in &sol.roll_var {
        let u = units.of_node[v as usize];
        if let Some(a) = agg.get_mut(&(u, good)) {
            a.roll += roll;
            a.made += made;
        }
    }
    let district: BTreeMap<Good, &super::solve::GoodBound> =
        sol.goods.iter().map(|b| (b.good, b)).collect();
    agg.into_iter()
        .map(|((unit, good), a)| {
            let d = district.get(&good);
            let (dil, dol, diu, dou) = d.map_or((0.0, 0.0, f64::INFINITY, f64::INFINITY), |d| {
                (
                    d.in_l.to_f64(),
                    d.out_l.to_f64(),
                    d.in_u.to_f64(),
                    d.out_u.to_f64(),
                )
            });
            let ceil = |du: f64, dl: f64, l: f64| {
                if a.class == class::UNBOUNDED || !du.is_finite() {
                    f64::INFINITY
                } else {
                    (du - (dl - l)).max(l)
                }
            };
            let in_u = ceil(diu, dil, a.il);
            let out_u = ceil(dou, dol, a.ol);
            let (mut cls, mut wy) = (a.class, a.why);
            if cls == class::EXACT
                && (in_u.is_infinite() || out_u.is_infinite())
                && d.is_some_and(|d| d.why.0 == why::U_INFINITE)
            {
                cls = class::BOUNDED;
                wy = (why::U_INFINITE, u32::MAX);
            }
            let mut var = a.var;
            if a.roll > 0.0 && a.made > 0.0 {
                var += (a.il / a.made).clamp(0.0, 1.0) * a.roll;
            }
            UnitBound {
                unit,
                good,
                class: cls,
                why: wy,
                in_l: a.il,
                out_l: a.ol,
                in_u,
                out_u,
                var,
                batch: a.b_in + a.b_out,
            }
        })
        .collect()
}
