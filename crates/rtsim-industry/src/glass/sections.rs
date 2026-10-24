// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sections: a district's compile cut at its stores (SECTIONS.md §3.1 to §3.3, SEC-1 slice S2).
//!
//! **The cut.** Union along every edge whose ends are both not cuts; a cut is a `STORE` node or a
//! virtual node (the void sink, which has no position of its own: choice SC48). Kinetic domains do
//! not join (Q1 a). Each union class is a piece.
//!
//! **The fold** (§3.2). A piece is *working* when it holds a machine, heater, boiler, sink or source,
//! drops into a virtual sink, or is a residual group that touches a store; *transfer* when it only
//! moves goods between stores; *idle* when it touches no store (and holds no working node). A
//! transfer piece folds, round by round, into a section it reaches through one store, downstream
//! first, then upstream, the working piece with the lowest position first; transfer pieces that
//! reach none merge across the stores they share (a storage system). Idle nodes belong to no
//! section. Past `glass.sectionsMax`, the two smallest sections that share a store merge, else two
//! that share a domain, else the two smallest, until it holds. Every tie goes by position, then by
//! a hash of the nodes' records, never by Java's indices.
//!
//! **Stores.** A store with goods edges to one section is that section's (interior); to several it is
//! shared and in no section's `section_of`; to none (idle movers only, or nothing) it is in no
//! section. Observe edges do not make a store a section's (choice SC49): a controller reading a vault
//! another line fills reads a level, it moves nothing there.
//!
//! **Judging groups** (§4.3). Sections at a shared store whose moves are not counted are judged on one
//! row. A shared store is **counted** (slice S3, §4.2) when it is an item store and every goods edge
//! at it runs to a mover Java counts at ([`COUNTED_MODELS`]): its sections join no group there, each
//! judged on what its own movers put in and took out. Past `glass.countedStoresMax` per district, the
//! lowest positions first, the rest are not ([`Sections::compute_counted`], choice SC78).
//!
//! **Fingerprints** (§3.3). FNV-1a 64 over the section's records with positions in place of Java's
//! indices, each record encoded alone and the encodings sorted, so the same factory gives the same
//! fingerprint in any node order. State slots are left out (`state_slots`). A kinetic level is a
//! node's own speed there (its ratio times the level's speed), which no choice of Create's root moves.
//!
//! Everything here is a function of the compile and `glass.sectionsMax`: it is recomputed at every
//! accepted compile and on restore, never saved but for the fingerprints the page checks (§5).

use std::collections::{BTreeMap, BTreeSet};

use super::graph::{Graph, carries_goods};
use super::records::{DomainLevel, GraphNode, NodeKind, domain_flags, model};
use crate::ids::BlockPos;

/// No section: idle nodes, shared stores and stores no section touches.
pub const NONE: u16 = u16::MAX;

/// The models whose moves at a store Java counts (SECTIONS.md §4.2): chutes and smart chutes
/// (their transfers), funnels (`InvManipulationBehaviour`), arms (`ArmInteractionPoint`), a
/// basin's outputs, a frogport's push and pull and a packager's extraction and unpacking. Hoppers,
/// belts, tunnels, depots, pumps and pipes are not counted.
pub const COUNTED_MODELS: [u32; 8] = [
    model::CHUTE,
    model::FUNNEL_ANDESITE,
    model::FUNNEL_BRASS,
    model::ARM,
    model::SMART_CHUTE,
    model::PACKAGER,
    model::FROGPORT,
    model::BASIN,
];

/// One section of a compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Its nodes, stores not included, ascending.
    pub nodes: Vec<u32>,
    /// The stores its nodes have goods edges to or from (interior and shared), ascending.
    pub stores: Vec<u32>,
    /// A working section; else a storage system (transfer pieces that reach no working section).
    pub working: bool,
    /// Its lowest node position (virtual nodes have none), for ties and names.
    pub low: Option<BlockPos>,
    /// The bounding box of its nodes' and stores' positions (a residual group's extent included),
    /// grown by one block: an edit inside it stales the section (§3.6).
    pub bbox: Option<(BlockPos, BlockPos)>,
    /// Domains its nodes are members of or drive, ascending.
    pub domains: Vec<u32>,
    pub fingerprint: u64,
}

impl Section {
    /// Whether `p` lies in the section's node box.
    pub fn holds(&self, p: BlockPos) -> bool {
        self.bbox.is_some_and(|(lo, hi)| {
            lo.x <= p.x && p.x <= hi.x && lo.y <= p.y && p.y <= hi.y && lo.z <= p.z && p.z <= hi.z
        })
    }
}

/// A compile's sections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sections {
    /// Per node: its section; [`NONE`] for idle nodes, virtual nodes, shared stores and stores no
    /// section touches.
    pub section_of: Vec<u16>,
    /// Sections by their lowest node position.
    pub list: Vec<Section>,
    /// Stores shared by sections: (store node, its sections ascending), by store node.
    pub shared: Vec<(u32, Vec<u16>)>,
    /// Per section: its judging group, named by the group's lowest section.
    pub group_of: Vec<u16>,
    /// Merges `glass.sectionsMax` made.
    pub merged: u32,
    /// The shared stores whose moves are counted at their movers (§4.2), ascending.
    pub counted: Vec<u32>,
}

/// Can moves at store `h` be counted at its movers (§4.2): an item store with an inventory, with
/// at least one goods edge, each to a node of [`COUNTED_MODELS`] (`edges`: those at `h`).
fn countable(g: &Graph, h: u32, edges: &[u32]) -> bool {
    let n = g.node(h);
    if n.kind != NodeKind::Store || n.model != model::STORE_ITEMS || n.inventory.is_none() {
        return false;
    }
    let mut any = false;
    for &e in edges {
        let ed = g.edge(e);
        if !carries_goods(ed.kind) {
            continue;
        }
        let far = if ed.from == h { ed.to } else { ed.from };
        if far == h || !COUNTED_MODELS.contains(&g.node(far).model) {
            return false;
        }
        any = true;
    }
    any
}

/// The void sink: goods leave the world at no place of their own.
pub fn is_virtual(n: &GraphNode) -> bool {
    n.kind == NodeKind::Sink && n.model == model::SINK_VOID
}

fn is_cut(n: &GraphNode) -> bool {
    n.kind == NodeKind::Store || is_virtual(n)
}

fn working_kind(k: NodeKind) -> bool {
    matches!(
        k,
        NodeKind::Machine | NodeKind::Heater | NodeKind::Boiler | NodeKind::Sink | NodeKind::Source
    )
}

/// The positions a node occupies: its own, and for a residual group the extent Java sends in its
/// params (`dx, dy, dz` from its lowest corner, GraphBuilder's residual groups). None for a
/// virtual node.
pub fn extent(n: &GraphNode) -> Option<(BlockPos, BlockPos)> {
    if is_virtual(n) {
        return None;
    }
    let p = n.pos;
    if n.kind == NodeKind::BlackBox {
        let d = |i: usize| n.params[i].clamp(0, 1 << 20) as i32;
        let hi = BlockPos::new(
            p.x.saturating_add(d(0)),
            p.y.saturating_add(d(1)),
            p.z.saturating_add(d(2)),
        );
        return Some((p, hi));
    }
    Some((p, p))
}

pub(super) struct Uf(Vec<u32>);

impl Uf {
    pub(super) fn new(n: usize) -> Uf {
        Uf((0..n as u32).collect())
    }
    pub(super) fn find(&mut self, mut x: u32) -> u32 {
        while self.0[x as usize] != x {
            let p = self.0[x as usize];
            self.0[x as usize] = self.0[p as usize];
            x = p;
        }
        x
    }
    /// Joins the two classes; the lower root stays.
    pub(super) fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            let (lo, hi) = (a.min(b), a.max(b));
            self.0[hi as usize] = lo;
        }
    }
}

/// A section while it is built: pieces' nodes, stores, kind.
#[derive(Clone, Debug)]
struct Draft {
    nodes: Vec<u32>,
    stores: BTreeSet<u32>,
    domains: BTreeSet<u32>,
    working: bool,
    low: Option<BlockPos>,
    /// Its nodes' record hashes summed ([`Ctx::canon`]): the tie after the position.
    canon: u64,
}

/// What the cut and the fingerprints read per node, built once per compute.
struct Ctx {
    /// Per node: the edges with an end at it, ascending.
    edges_at: Vec<Vec<u32>>,
    /// Per node: its ratio on its own domain (its member record's), if it is listed there.
    ratio_of: Vec<Option<(i32, u32)>>,
    /// Per recipe: its contents hashed.
    recipe_hash: Vec<u64>,
    /// Per node: its record hashed (its build at its position), for ties.
    hash: Vec<u64>,
}

impl Ctx {
    fn new(g: &Graph) -> Ctx {
        let n = g.nodes.len();
        let mut edges_at = vec![Vec::new(); n];
        for (i, e) in g.edges.iter().enumerate() {
            edges_at[e.from as usize].push(i as u32);
            if e.to != e.from {
                edges_at[e.to as usize].push(i as u32);
            }
        }
        let mut ratio_of = vec![None; n];
        for (di, d) in g.domains.iter().enumerate() {
            for m in &d.members {
                if let Some(v) = m.node.filter(|&v| (v as usize) < n)
                    && g.nodes[v as usize].domain == Some(di as u32)
                {
                    ratio_of[v as usize].get_or_insert((m.ratio_num, m.ratio_den));
                }
            }
        }
        let recipe_hash = (0..g.recipes.len() as u32)
            .map(|r| recipe_hash(g, r))
            .collect();
        let mut c = Ctx {
            edges_at,
            ratio_of,
            recipe_hash,
            hash: Vec::new(),
        };
        c.hash = (0..n as u32)
            .map(|v| rtsim_kernel::rng::fnv1a64(&node_record(g, &c, v, 0)))
            .collect();
        c
    }

    /// A set of nodes' record hashes summed: the same for the same nodes in any order or numbering.
    fn canon(&self, nodes: &[u32]) -> u64 {
        nodes
            .iter()
            .fold(0u64, |a, &v| a.wrapping_add(self.hash[v as usize]))
    }
}

fn low_of(g: &Graph, nodes: &[u32]) -> Option<BlockPos> {
    nodes
        .iter()
        .filter_map(|&v| extent(g.node(v)).map(|e| e.0))
        .min()
}

/// Orders by the lowest position, a section with none last.
fn pos_key(p: Option<BlockPos>) -> (bool, BlockPos) {
    (p.is_none(), p.unwrap_or_default())
}

impl Sections {
    /// The sections of `g`, with every shared store uncounted (slice S2).
    pub fn compute(g: &Graph, max: usize) -> Sections {
        Sections::compute_with(g, max, |_| false)
    }

    /// The sections of `g`; `counted(store)` says a shared store's moves are counted per section
    /// (S3), so it joins no judging group.
    pub fn compute_with(g: &Graph, max: usize, counted: impl Fn(u32) -> bool) -> Sections {
        Sections::compute_picking(g, max, |_, shared| {
            shared.iter().copied().filter(|&h| counted(h)).collect()
        })
    }

    /// The sections of `g` with the shared stores its movers can be counted at counted (§4.2), at
    /// most `counted_max` of them (`glass.countedStoresMax`), the lowest positions first: what a
    /// compile is cut into. A graph on a moving frame (`end.frame != 0`) counts no store: Java neither
    /// takes a count set for one nor samples it (`num.frame != 0`), so a counted store there could never
    /// be covered, and its units would be discarded at every sweep instead of judged with the store
    /// shared (choice SC122).
    pub fn compute_counted(g: &Graph, max: usize, counted_max: usize) -> Sections {
        Sections::compute_picking(g, max, |ctx, shared| {
            if g.end.frame != 0 {
                return Vec::new();
            }
            let mut c: Vec<(BlockPos, u32)> = shared
                .iter()
                .filter(|&&h| countable(g, h, &ctx.edges_at[h as usize]))
                .map(|&h| (g.node(h).pos, h))
                .collect();
            c.sort_unstable();
            c.truncate(counted_max);
            c.into_iter().map(|(_, h)| h).collect()
        })
    }

    /// The sections of `g` cut into at most `max` sections, below `base_max` (`glass.sectionsMax`;
    /// a row cap's cut, choice SC57): counted are the stores [`Sections::compute_counted`] counts at
    /// `base_max` that this cut still shares, so a row cap never counts a store the full cut did not
    /// (choice SC78).
    pub fn cut_counted(g: &Graph, max: usize, base_max: usize, counted_max: usize) -> Sections {
        let base = Sections::compute_counted(g, base_max, counted_max);
        let clamp = |m: usize| m.clamp(1, usize::from(NONE) - 1);
        if clamp(max) >= clamp(base_max) {
            return base;
        }
        Sections::cut_with(g, max, &base.counted)
    }

    /// The sections of `g` cut into at most `max`, counted the shared stores of `base` (ascending).
    pub fn cut_with(g: &Graph, max: usize, base: &[u32]) -> Sections {
        Sections::compute_with(g, max, |h| base.binary_search(&h).is_ok())
    }

    /// The sections of `g`; `pick(ctx, shared)` names the shared stores (ascending) that are
    /// counted.
    fn compute_picking(g: &Graph, max: usize, pick: impl Fn(&Ctx, &[u32]) -> Vec<u32>) -> Sections {
        let n = g.nodes.len();
        // `section_of` is a u16 with NONE kept apart.
        let max = max.clamp(1, usize::from(NONE) - 1);
        let ctx = Ctx::new(g);
        let cut: Vec<bool> = g.nodes.iter().map(is_cut).collect();
        let node_domains = node_domains(g);
        // The cut: every edge between two nodes that are not cuts joins them.
        let mut uf = Uf::new(n);
        for e in &g.edges {
            if !cut[e.from as usize] && !cut[e.to as usize] {
                uf.join(e.from, e.to);
            }
        }
        // Pieces, numbered by their lowest node index (only for building: the order of the result
        // is by position).
        let mut piece_of = vec![u32::MAX; n];
        let mut pieces: Vec<Draft> = Vec::new();
        let mut root_piece: BTreeMap<u32, u32> = BTreeMap::new();
        for v in 0..n as u32 {
            if cut[v as usize] {
                continue;
            }
            let r = uf.find(v);
            let p = *root_piece.entry(r).or_insert_with(|| {
                pieces.push(Draft {
                    nodes: Vec::new(),
                    stores: BTreeSet::new(),
                    domains: BTreeSet::new(),
                    working: false,
                    low: None,
                    canon: 0,
                });
                pieces.len() as u32 - 1
            });
            piece_of[v as usize] = p;
            pieces[p as usize].nodes.push(v);
        }
        let mut residual = vec![false; pieces.len()];
        // Who gives into and who takes from each store, by piece.
        let mut givers: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut takers: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for e in &g.edges {
            let (f, t) = (e.from as usize, e.to as usize);
            if !carries_goods(e.kind) {
                continue;
            }
            let (fs, ts) = (
                g.nodes[f].kind == NodeKind::Store,
                g.nodes[t].kind == NodeKind::Store,
            );
            if !cut[f] && ts {
                let p = piece_of[f];
                pieces[p as usize].stores.insert(e.to);
                givers.entry(e.to).or_default().insert(p);
            }
            if fs && !cut[t] {
                let p = piece_of[t];
                pieces[p as usize].stores.insert(e.from);
                takers.entry(e.from).or_default().insert(p);
            }
            // Dropping into the void is work of its own (SC48).
            if !cut[f] && is_virtual(&g.nodes[t]) {
                pieces[piece_of[f] as usize].working = true;
            }
        }
        for (pi, p) in pieces.iter_mut().enumerate() {
            for &v in &p.nodes {
                let k = g.nodes[v as usize].kind;
                p.working |= working_kind(k);
                residual[pi] |= k == NodeKind::BlackBox;
            }
            // A residual group between stores is an unknown machine, kept apart (§3.2 step 1).
            p.working |= residual[pi] && !p.stores.is_empty();
            p.low = low_of(g, &p.nodes);
            p.canon = ctx.canon(&p.nodes);
        }
        // The fold: transfer pieces into the section they reach through one store, round by round.
        const UNSET: u32 = u32::MAX;
        let mut owner: Vec<u32> = pieces
            .iter()
            .enumerate()
            .map(|(i, p)| if p.working { i as u32 } else { UNSET })
            .collect();
        let transfer: Vec<u32> = (0..pieces.len() as u32)
            .filter(|&i| !pieces[i as usize].working && !pieces[i as usize].stores.is_empty())
            .collect();
        loop {
            let mut moves: Vec<(u32, u32)> = Vec::new();
            for &t in &transfer {
                if owner[t as usize] != UNSET {
                    continue;
                }
                let pick = |from: &BTreeMap<u32, BTreeSet<u32>>,
                            to: &BTreeMap<u32, BTreeSet<u32>>| {
                    let mut best: Option<u32> = None;
                    for &h in &pieces[t as usize].stores {
                        if !from.get(&h).is_some_and(|s| s.contains(&t)) {
                            continue;
                        }
                        for &w in to.get(&h).into_iter().flatten() {
                            let o = owner[w as usize];
                            if o == UNSET || w == t {
                                continue;
                            }
                            // the working piece's own lowest position, then its records
                            let rank = |x: u32| {
                                let p = &pieces[x as usize];
                                (pos_key(p.low), p.canon, x)
                            };
                            let better = best.is_none_or(|b| rank(o) < rank(b));
                            if better {
                                best = Some(o);
                            }
                        }
                    }
                    best
                };
                // Downstream first: it gives into a store a section takes from.
                if let Some(o) = pick(&givers, &takers).or_else(|| pick(&takers, &givers)) {
                    moves.push((t, o));
                }
            }
            if moves.is_empty() {
                break;
            }
            for (t, o) in moves {
                owner[t as usize] = o;
            }
        }
        // Transfer pieces that reach no working section: storage systems, merged across the stores
        // they share.
        let mut st = Uf::new(pieces.len());
        let mut by_store: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for &t in &transfer {
            if owner[t as usize] == UNSET {
                for &h in &pieces[t as usize].stores {
                    by_store.entry(h).or_default().push(t);
                }
            }
        }
        for ps in by_store.values() {
            for w in ps.windows(2) {
                st.join(w[0], w[1]);
            }
        }
        for &t in &transfer {
            if owner[t as usize] == UNSET {
                owner[t as usize] = st.find(t);
            }
        }
        // Drafts by owner.
        let mut drafts: BTreeMap<u32, Draft> = BTreeMap::new();
        for (i, p) in pieces.iter().enumerate() {
            let o = owner[i];
            if o == UNSET {
                continue; // idle
            }
            let d = drafts.entry(o).or_insert_with(|| Draft {
                nodes: Vec::new(),
                stores: BTreeSet::new(),
                domains: BTreeSet::new(),
                working: pieces[o as usize].working,
                low: None,
                canon: 0,
            });
            d.nodes.extend_from_slice(&p.nodes);
            d.stores.extend(p.stores.iter().copied());
            for &v in &p.nodes {
                d.domains.extend(node_domains[v as usize].iter().copied());
            }
        }
        let mut drafts: Vec<Draft> = drafts.into_values().collect();
        for d in drafts.iter_mut() {
            d.nodes.sort_unstable();
            d.low = low_of(g, &d.nodes);
            d.canon = ctx.canon(&d.nodes);
        }
        // The cap (§3.2 step 5).
        let mut merged = 0u32;
        while drafts.len() > max {
            let (a, b) = pick_merge(&drafts);
            let bd = drafts.remove(b);
            let ad = &mut drafts[a];
            ad.nodes.extend(bd.nodes);
            ad.nodes.sort_unstable();
            ad.stores.extend(bd.stores);
            ad.domains.extend(bd.domains);
            ad.working |= bd.working;
            ad.low = low_of(g, &ad.nodes);
            ad.canon = ad.canon.wrapping_add(bd.canon);
            merged += 1;
        }
        let mut list: Vec<Section> = drafts
            .iter()
            .map(|d| {
                let stores: Vec<u32> = d.stores.iter().copied().collect();
                let doms: Vec<u32> = d.domains.iter().copied().collect();
                Section {
                    bbox: bbox_of(g, &d.nodes, &stores),
                    fingerprint: fingerprint_in(g, &ctx, &d.nodes, &stores, &doms),
                    nodes: d.nodes.clone(),
                    stores,
                    working: d.working,
                    low: d.low,
                    domains: doms,
                }
            })
            .collect();
        list.sort_by(|a, b| {
            (pos_key(a.low), a.fingerprint, a.nodes.first()).cmp(&(
                pos_key(b.low),
                b.fingerprint,
                b.nodes.first(),
            ))
        });
        let mut section_of = vec![NONE; n];
        let mut touching: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
        for (si, s) in list.iter().enumerate() {
            for &v in &s.nodes {
                section_of[v as usize] = si as u16;
            }
            for &h in &s.stores {
                touching.entry(h).or_default().push(si as u16);
            }
        }
        let mut shared = Vec::new();
        for (h, ss) in touching {
            if ss.len() == 1 {
                section_of[h as usize] = ss[0];
                continue;
            }
            shared.push((h, ss));
        }
        let ids: Vec<u32> = shared.iter().map(|(h, _)| *h).collect();
        let mut counted = pick(&ctx, &ids);
        counted.sort_unstable();
        counted.dedup();
        let mut gu = Uf::new(list.len());
        for (h, ss) in &shared {
            if counted.binary_search(h).is_err() {
                for w in ss.windows(2) {
                    gu.join(u32::from(w[0]), u32::from(w[1]));
                }
            }
        }
        let group_of = (0..list.len() as u32).map(|s| gu.find(s) as u16).collect();
        Sections {
            section_of,
            list,
            shared,
            group_of,
            merged,
            counted,
        }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The judging groups: each group's sections ascending, by the group's lowest section.
    pub fn groups(&self) -> Vec<Vec<u16>> {
        let mut by: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for (s, &gr) in self.group_of.iter().enumerate() {
            by.entry(gr).or_default().push(s as u16);
        }
        by.into_values().collect()
    }

    /// The sections whose node box holds `p`.
    pub fn at(&self, p: BlockPos) -> Vec<u16> {
        (0..self.list.len())
            .filter(|&s| self.list[s].holds(p))
            .map(|s| s as u16)
            .collect()
    }

    /// The fingerprints, in section order (what the page keeps, §5).
    pub fn fingerprints(&self) -> Vec<u64> {
        self.list.iter().map(|s| s.fingerprint).collect()
    }

    /// What a page keeps to check that its restore cuts as it did (§5): the fingerprints, then,
    /// with counted stores, a hash of their node indices (the page saves the graph they index).
    pub fn cut_keys(&self) -> Vec<u64> {
        let mut k = self.fingerprints();
        if !self.counted.is_empty() {
            let b: Vec<u8> = self.counted.iter().flat_map(|h| h.to_le_bytes()).collect();
            k.push(rtsim_kernel::rng::fnv1a64(&b));
        }
        k
    }
}

/// The pair to merge next (indices into `drafts`, `a < b`). Drafts rank by their nodes, then their
/// lowest position, then their records ([`Ctx::canon`]). In each group of drafts that share a store
/// the two smallest are its pair, and the group whose pair is smallest together (by summed nodes,
/// then the lower and the higher position, then the records) gives it; else a group that shares a
/// domain; else the two smallest of all. Linear in the drafts' stores and domains.
fn pick_merge(drafts: &[Draft]) -> (usize, usize) {
    let rank = |i: usize| {
        (
            drafts[i].nodes.len(),
            pos_key(drafts[i].low),
            drafts[i].canon,
            i,
        )
    };
    let two = |it: &mut dyn Iterator<Item = usize>| -> Option<(usize, usize)> {
        let (mut a, mut b): (Option<usize>, Option<usize>) = (None, None);
        for i in it {
            if a.is_none_or(|x| rank(i) < rank(x)) {
                b = a;
                a = Some(i);
            } else if b.is_none_or(|y| rank(i) < rank(y)) {
                b = Some(i);
            }
        }
        Some((a?, b?))
    };
    let key = |(a, b): (usize, usize)| {
        let (ra, rb) = (rank(a), rank(b));
        (
            ra.0 + rb.0,
            ra.1.min(rb.1),
            ra.1.max(rb.1),
            ra.2.min(rb.2),
            ra.2.max(rb.2),
            a.min(b),
            a.max(b),
        )
    };
    let best_of = |by: &BTreeMap<u32, Vec<usize>>| {
        by.values()
            .filter_map(|ds| two(&mut ds.iter().copied()))
            .min_by_key(|&p| key(p))
    };
    let mut by_store: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut by_domain: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, d) in drafts.iter().enumerate() {
        for &h in &d.stores {
            by_store.entry(h).or_default().push(i);
        }
        for &x in &d.domains {
            by_domain.entry(x).or_default().push(i);
        }
    }
    let (a, b) = best_of(&by_store)
        .or_else(|| best_of(&by_domain))
        .or_else(|| two(&mut (0..drafts.len())))
        .expect("at least two drafts");
    (a.min(b), a.max(b))
}

/// Per node: the domains it is a member of or drives, ascending.
fn node_domains(g: &Graph) -> Vec<Vec<u32>> {
    let mut out: Vec<Vec<u32>> = vec![Vec::new(); g.nodes.len()];
    for (v, n) in g.nodes.iter().enumerate() {
        out[v].extend(n.domain);
        out[v].extend(g.drives[v]);
    }
    for (di, d) in g.domains.iter().enumerate() {
        for m in &d.members {
            if let Some(v) = m.node.filter(|&v| (v as usize) < out.len()) {
                out[v as usize].push(di as u32);
            }
        }
    }
    for d in out.iter_mut() {
        d.sort_unstable();
        d.dedup();
    }
    out
}

fn bbox_of(g: &Graph, nodes: &[u32], stores: &[u32]) -> Option<(BlockPos, BlockPos)> {
    let mut b: Option<(BlockPos, BlockPos)> = None;
    for &v in nodes.iter().chain(stores) {
        let Some((lo, hi)) = extent(g.node(v)) else {
            continue;
        };
        b = Some(match b {
            None => (lo, hi),
            Some((a, z)) => (
                BlockPos::new(a.x.min(lo.x), a.y.min(lo.y), a.z.min(lo.z)),
                BlockPos::new(z.x.max(hi.x), z.y.max(hi.y), z.z.max(hi.z)),
            ),
        });
    }
    b.map(|(lo, hi)| {
        (
            BlockPos::new(
                lo.x.saturating_sub(1),
                lo.y.saturating_sub(1),
                lo.z.saturating_sub(1),
            ),
            BlockPos::new(
                hi.x.saturating_add(1),
                hi.y.saturating_add(1),
                hi.z.saturating_add(1),
            ),
        )
    })
}

// ---- fingerprints (§3.3) ---------------------------------------------------------------------

struct Enc(Vec<u8>);

impl Enc {
    fn tag(t: u8) -> Enc {
        Enc(vec![t])
    }
    fn u8(&mut self, x: u8) {
        self.0.push(x);
    }
    fn u32(&mut self, x: u32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn i32(&mut self, x: i32) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn u64(&mut self, x: u64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn i64(&mut self, x: i64) {
        self.0.extend_from_slice(&x.to_le_bytes());
    }
    fn pos(&mut self, p: BlockPos) {
        self.i32(p.x);
        self.i32(p.y);
        self.i32(p.z);
    }
}

/// A node as an edge's end: its kind and model, and its position unless it is virtual.
fn end(e: &mut Enc, n: &GraphNode) {
    e.u32(super::codec::node_code(n.kind));
    e.u32(n.model);
    if is_virtual(n) {
        e.u8(0);
    } else {
        e.u8(1);
        e.pos(n.pos);
    }
}

/// A recipe's contents (the record but its header), hashed.
fn recipe_hash(g: &Graph, r: u32) -> u64 {
    let rc = &g.recipes[r as usize];
    let mut e = Enc::tag(0);
    e.u32(rc.recipe_type);
    e.u32(rc.heat);
    e.u32(rc.duration);
    e.u32(rc.name);
    for (side, terms) in [(1u8, &rc.inputs), (2u8, &rc.outputs)] {
        e.u8(side);
        e.u32(terms.len() as u32);
        for t in terms.iter() {
            e.u32(t.good.0);
            e.i64(t.num);
            e.i64(t.den);
            e.u32(t.flags);
        }
    }
    rtsim_kernel::rng::fnv1a64(&e.0)
}

/// A node's speed at a level: its ratio times the level's speed, in lowest terms.
fn own_speed(ratio: (i32, u32), l: &DomainLevel) -> (i64, u64) {
    let n = i64::from(ratio.0) * i64::from(l.speed_num);
    let d = u64::from(ratio.1) * u64::from(l.speed_den);
    let (mut a, mut b) = (n.unsigned_abs(), d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    if a == 0 {
        return (0, 0);
    }
    (n / a as i64, d / a)
}

/// A node's build (every field but its index, its inventory index and its state slots), its rate
/// entries' recipes by contents and levels by selector and the node's own speed there.
fn node_record(g: &Graph, ctx: &Ctx, v: u32, tag: u8) -> Vec<u8> {
    let n = g.node(v);
    let mut e = Enc::tag(tag);
    end(&mut e, n);
    e.u32(n.flags);
    e.u32(n.rule);
    e.i64(n.capacity);
    e.u8(u8::from(n.domain.is_some()));
    let mut params = n.params;
    for &i in super::state_slots(n.kind) {
        params[i] = 0;
    }
    for p in params {
        e.i64(p);
    }
    let dom = n.domain.and_then(|d| g.domains.get(d as usize));
    let mut rates: Vec<Vec<u8>> = n
        .rates
        .iter()
        .map(|r| {
            let mut x = Enc::tag(0);
            match dom.and_then(|d| d.levels.get(r.level as usize)) {
                Some(l) => {
                    x.u8(1);
                    x.u32(l.selector);
                    match ctx.ratio_of[v as usize] {
                        Some(ratio) => {
                            let (a, b) = own_speed(ratio, l);
                            x.u8(1);
                            x.i64(a);
                            x.u64(b);
                        }
                        None => {
                            x.u8(0);
                            x.i32(l.speed_num);
                            x.u32(l.speed_den);
                        }
                    }
                }
                None => {
                    x.u8(0);
                    x.u32(r.level);
                }
            }
            x.u64(r.recipe.map_or(0, |ri| {
                ctx.recipe_hash
                    .get(ri as usize)
                    .copied()
                    .unwrap_or_default()
            }));
            x.u32(r.batch);
            x.u32(r.ticks_num);
            x.u32(r.ticks_den);
            x.u32(r.flags);
            x.0
        })
        .collect();
    rates.sort_unstable();
    e.u32(rates.len() as u32);
    for r in rates {
        e.0.extend_from_slice(&r);
    }
    e.0
}

fn edge_record(g: &Graph, i: usize) -> Vec<u8> {
    let ed = &g.edges[i];
    let mut e = Enc::tag(2);
    e.u32(super::codec::edge_code(ed.kind));
    end(&mut e, g.node(ed.from));
    end(&mut e, g.node(ed.to));
    e.u32(ed.order);
    e.u32(ed.flags);
    match ed.capacity {
        None => e.u8(0),
        Some((a, b)) => {
            e.u8(1);
            e.u32(a);
            e.u32(b);
        }
    }
    e.u32(ed.latency);
    e.i64(ed.param);
    let mut f: Vec<u32> = ed.filter.iter().map(|g| g.0).collect();
    f.sort_unstable();
    e.u32(f.len() as u32);
    for x in f {
        e.u32(x);
    }
    e.0
}

/// A domain as the section sees it: its stress setting, whether a boiler drives it and its speed
/// rows' count (not the network's capacity, sources or other members, so a machine added elsewhere
/// on it leaves the section alone; the domain check weighs those, §3.6), and the section's own
/// members, each with its speed at each row (its ratio times the row's speed: Create's root, which
/// loaded chunks choose, moves both and not their product).
fn domain_record(g: &Graph, d: u32, nodes: &[u32]) -> Vec<u8> {
    let dm = &g.domains[d as usize];
    let mut e = Enc::tag(3);
    e.u32(dm.flags & domain_flags::STRESS_DISABLED);
    e.u8(u8::from(dm.driven()));
    e.u32(dm.levels.len() as u32);
    let mut own: Vec<Vec<u8>> = dm
        .members
        .iter()
        .filter(|m| m.node.is_some_and(|v| nodes.binary_search(&v).is_ok()))
        .map(|m| {
            let mut x = Enc::tag(0);
            end(&mut x, g.node(m.node.expect("filtered")));
            x.u32(m.count);
            x.u64(m.impact.to_bits());
            let mut speeds: Vec<(u32, i64, u64)> = dm
                .levels
                .iter()
                .map(|l| {
                    let (a, b) = own_speed((m.ratio_num, m.ratio_den), l);
                    (l.selector, a, b)
                })
                .collect();
            speeds.sort_unstable();
            x.u32(speeds.len() as u32);
            for (sel, a, b) in speeds {
                x.u32(sel);
                x.i64(a);
                x.u64(b);
            }
            x.0
        })
        .collect();
    own.sort_unstable();
    e.u32(own.len() as u32);
    for m in own {
        e.0.extend_from_slice(&m);
    }
    e.0
}

/// The section's fingerprint: its nodes, the stores it touches, the edges with an end at one of its
/// nodes, and its view of each domain it touches, each record encoded alone, sorted, then hashed.
pub fn fingerprint(g: &Graph, nodes: &[u32], stores: &[u32], domains: &[u32]) -> u64 {
    fingerprint_in(g, &Ctx::new(g), nodes, stores, domains)
}

fn fingerprint_in(g: &Graph, ctx: &Ctx, nodes: &[u32], stores: &[u32], domains: &[u32]) -> u64 {
    let mut recs: Vec<Vec<u8>> = Vec::new();
    for &v in nodes {
        recs.push(node_record(g, ctx, v, 0));
    }
    for &h in stores {
        recs.push(node_record(g, ctx, h, 1));
    }
    let mut edges: Vec<u32> = nodes
        .iter()
        .flat_map(|&v| ctx.edges_at[v as usize].iter().copied())
        .collect();
    edges.sort_unstable();
    edges.dedup();
    for i in edges {
        recs.push(edge_record(g, i as usize));
    }
    for &d in domains {
        recs.push(domain_record(g, d, nodes));
    }
    recs.sort_unstable();
    let mut all = Vec::new();
    for r in recs {
        all.extend_from_slice(&(r.len() as u32).to_le_bytes());
        all.extend_from_slice(&r);
    }
    rtsim_kernel::rng::fnv1a64(&all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glass::records::{
        DomainLevel, DomainSource, EdgeKind, GraphDomain, GraphEdge, GraphEnd, GraphHead,
        GraphRecipe, Member, RateEntry, Term,
    };
    use crate::ids::{DistrictId, Good};

    /// A hand-built graph: nodes by (kind, model, x), edges by (from, to, kind).
    struct B {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
        recipes: Vec<GraphRecipe>,
        domains: Vec<GraphDomain>,
    }

    fn head(i: usize) -> GraphHead {
        GraphHead {
            district: DistrictId(1),
            compile: 1,
            index: i as u32,
        }
    }

    impl B {
        fn new() -> B {
            B {
                nodes: Vec::new(),
                edges: Vec::new(),
                recipes: Vec::new(),
                domains: Vec::new(),
            }
        }
        fn node(&mut self, kind: NodeKind, m: u32, x: i32) -> u32 {
            let i = self.nodes.len();
            self.nodes.push(GraphNode {
                head: head(i),
                kind,
                model: m,
                domain: None,
                inventory: (kind == NodeKind::Store).then_some(i as u32),
                flags: 0,
                pos: BlockPos::new(x, 64, 0),
                rule: 0,
                capacity: 0,
                params: [0; 4],
                rates: Vec::new(),
            });
            i as u32
        }
        fn store(&mut self, x: i32) -> u32 {
            self.node(NodeKind::Store, model::STORE_ITEMS, x)
        }
        fn mover(&mut self, x: i32) -> u32 {
            self.node(NodeKind::Mover, model::FUNNEL_ANDESITE, x)
        }
        fn machine(&mut self, x: i32) -> u32 {
            self.node(NodeKind::Machine, model::PRESS, x)
        }
        fn edge(&mut self, from: u32, to: u32, kind: EdgeKind) {
            let i = self.edges.len();
            self.edges.push(GraphEdge {
                head: head(i),
                kind,
                from,
                to,
                order: 0,
                flags: 0,
                capacity: None,
                latency: 0,
                param: 0,
                filter: Vec::new(),
            });
        }
        fn link(&mut self, path: &[u32]) {
            for w in path.windows(2) {
                self.edge(w[0], w[1], EdgeKind::Adjacency);
            }
        }
        fn graph(&self) -> Graph {
            let end = GraphEnd {
                district: DistrictId(1),
                compile: 1,
                flags: 1,
                window: 0,
                records: 0,
                nodes: self.nodes.len() as u32,
                edges: self.edges.len() as u32,
                recipes: self.recipes.len() as u32,
                domains: self.domains.len() as u32,
                reason: 0,
                fingerprint: 0,
                frame: 0,
                revision: 1,
                compile_ticks: 0,
            };
            Graph::build(
                end,
                self.nodes.clone(),
                self.edges.clone(),
                self.recipes.clone(),
                self.domains.clone(),
            )
            .expect("valid")
        }
        /// The same graph with its records in another order (Java's order is not canonical).
        fn shuffled(&self, seed: u64) -> Graph {
            let n = self.nodes.len();
            let mut perm: Vec<u32> = (0..n as u32).collect();
            let mut s = seed | 1;
            for i in (1..n).rev() {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                perm.swap(i, (s % (i as u64 + 1)) as usize);
            }
            // perm[new] = old
            let mut new_of = vec![0u32; n];
            for (new, &old) in perm.iter().enumerate() {
                new_of[old as usize] = new as u32;
            }
            let mut b = B::new();
            b.recipes = self.recipes.clone();
            for &old in &perm {
                let mut nd = self.nodes[old as usize].clone();
                nd.head = head(b.nodes.len());
                b.nodes.push(nd);
            }
            let mut edges = self.edges.clone();
            edges.reverse();
            for (i, mut e) in edges.into_iter().enumerate() {
                e.head = head(i);
                e.from = new_of[e.from as usize];
                e.to = new_of[e.to as usize];
                b.edges.push(e);
            }
            for d in &self.domains {
                let mut d = d.clone();
                for m in d.members.iter_mut() {
                    m.node = m.node.map(|v| new_of[v as usize]);
                }
                for src in d.sources.iter_mut() {
                    src.driver = src.driver.map(|v| new_of[v as usize]);
                }
                d.members.reverse();
                b.domains.push(d);
            }
            b.graph()
        }
    }

    /// Two lines on a vault: A fills it (funnel from a press), B draws from it (funnel to a press).
    fn vault_between_two_lines() -> (B, [u32; 7]) {
        let mut b = B::new();
        let src = b.store(0);
        let f1 = b.mover(1);
        let m1 = b.machine(2);
        let f2 = b.mover(3);
        let vault = b.store(4);
        let f3 = b.mover(5);
        let m2 = b.machine(6);
        b.link(&[src, f1, m1, f2, vault, f3, m2]);
        (b, [src, f1, m1, f2, vault, f3, m2])
    }

    #[test]
    fn a_vault_between_two_lines_cuts_them_and_shares_the_vault() {
        let (b, [src, f1, m1, f2, vault, f3, m2]) = vault_between_two_lines();
        let s = Sections::compute(&b.graph(), 64);
        assert_eq!(s.len(), 2);
        assert_eq!(s.list[0].nodes, vec![f1, m1, f2]);
        assert_eq!(s.list[1].nodes, vec![f3, m2]);
        assert_eq!(
            s.section_of[src as usize], 0,
            "the source store is interior to line A"
        );
        assert_eq!(s.section_of[vault as usize], NONE);
        assert_eq!(s.shared, vec![(vault, vec![0, 1])]);
        // S2 counts no mover: the shared vault joins the two lines' judging group.
        assert_eq!(s.group_of, vec![0, 0]);
        // Counted (S3), the two are judged apart.
        let c = Sections::compute_with(&b.graph(), 64, |h| h == vault);
        assert_eq!(c.group_of, vec![0, 1]);
        assert!(s.list.iter().all(|x| x.working));
        let _ = m2;
    }

    /// Counted stores (§4.2): a vault between lines whose every goods edge runs to a mover Java
    /// counts at; past `glass.countedStoresMax` the lowest positions first; a row cap's cut counts
    /// only stores the full cut counts and still shares (choice SC78).
    #[test]
    fn the_counted_stores_are_those_counted_movers_reach_up_to_the_cap() {
        let (b, [_, _, _, _, vault, f3, _]) = vault_between_two_lines();
        let s = Sections::compute_counted(&b.graph(), 64, 256);
        assert_eq!(s.counted, vec![vault]);
        assert_eq!(s.group_of, vec![0, 1]);
        // A belt at the vault: Java does not count it, and the vault joins the two lines.
        let mut belt = vault_between_two_lines().0;
        belt.nodes[f3 as usize].model = model::BELT;
        let s = Sections::compute_counted(&belt.graph(), 64, 256);
        assert!(s.counted.is_empty());
        assert_eq!(s.group_of, vec![0, 0]);
        // Three lines on two vaults: with room for one, the lower is counted.
        let mut b = B::new();
        let src = b.store(0);
        let f1 = b.mover(1);
        let m1 = b.machine(2);
        let f2 = b.mover(3);
        let va = b.store(4);
        let f3 = b.mover(5);
        let m2 = b.machine(6);
        let f4 = b.mover(7);
        let vb = b.store(8);
        let f5 = b.mover(9);
        let m3 = b.machine(10);
        b.link(&[src, f1, m1, f2, va, f3, m2, f4, vb, f5, m3]);
        let g = b.graph();
        assert_eq!(Sections::compute_counted(&g, 64, 256).counted, vec![va, vb]);
        let one = Sections::compute_counted(&g, 64, 1);
        assert_eq!(one.counted, vec![va]);
        assert_eq!(one.group_of, vec![0, 1, 1]);
        assert!(Sections::compute_counted(&g, 64, 0).counted.is_empty());
        // Cut to two sections, one vault is interior; the other is still counted.
        let cut = Sections::cut_counted(&g, 2, 64, 256);
        assert_eq!(cut.len(), 2);
        assert_eq!(cut.counted.len(), 1);
        assert!(cut.shared.iter().any(|(h, _)| *h == cut.counted[0]));
        // The keys name the counted stores: the same sections counted otherwise differ.
        assert_ne!(
            Sections::compute_counted(&g, 64, 256).cut_keys(),
            Sections::compute_counted(&g, 64, 1).cut_keys()
        );
    }

    /// A graph on a moving frame has no counted store (choice SC122): Java never samples one, so the
    /// store stays shared and the lines at it are one judging group, as in S2; `cut_counted` agrees.
    #[test]
    fn a_moving_frame_counts_no_store() {
        let (b, [_, _, _, _, vault, _, _]) = vault_between_two_lines();
        let mut g = b.graph();
        assert_eq!(Sections::compute_counted(&g, 64, 256).counted, vec![vault]);
        g.end.frame = 7;
        let s = Sections::compute_counted(&g, 64, 256);
        assert!(s.counted.is_empty());
        assert_eq!(s.group_of, vec![0, 0]);
        let cut = Sections::cut_counted(&g, 2, 64, 256);
        assert!(cut.counted.is_empty());
        assert_eq!(s.cut_keys(), Sections::compute(&g, 64).cut_keys());
    }

    #[test]
    fn a_belt_into_a_machine_is_one_section() {
        let mut b = B::new();
        let a = b.store(0);
        let f = b.mover(1);
        let press = b.machine(2);
        let saw = b.node(NodeKind::Machine, model::SAW, 3);
        let out = b.store(4);
        b.link(&[a, f, press]);
        b.edge(press, saw, EdgeKind::Belt);
        b.link(&[saw, out]);
        let s = Sections::compute(&b.graph(), 64);
        assert_eq!(s.len(), 1);
        assert_eq!(s.list[0].nodes, vec![f, press, saw]);
        assert_eq!(s.section_of[a as usize], 0);
        assert_eq!(s.section_of[out as usize], 0);
        assert!(s.shared.is_empty());
    }

    #[test]
    fn a_transfer_chain_folds_downstream_then_upstream() {
        // in -> f0 -> v1 -> f1 -> v2 -> f2 -> m (a machine) -> out; and a lone funnel f3 that only
        // takes from `out` into `tail`.
        let mut b = B::new();
        let input = b.store(0);
        let f0 = b.mover(1);
        let v1 = b.store(2);
        let f1 = b.mover(3);
        let v2 = b.store(4);
        let f2 = b.mover(5);
        let m = b.machine(6);
        let out = b.store(7);
        let f3 = b.mover(8);
        let tail = b.store(9);
        b.link(&[input, f0, v1, f1, v2, f2, m, out, f3, tail]);
        let s = Sections::compute(&b.graph(), 64);
        // f1 folds downstream into the machine's section through v2, then f0 through v1; f3 reaches
        // no section downstream and folds upstream through `out`.
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(s.list[0].nodes, vec![f0, f1, f2, m, f3]);
        for h in [input, v1, v2, out, tail] {
            assert_eq!(s.section_of[h as usize], 0);
        }
    }

    #[test]
    fn a_transfer_piece_between_two_lines_goes_downstream() {
        // line A -> va -> f (transfer) -> vb -> line B: f folds into B (downstream).
        let mut b = B::new();
        let a0 = b.store(0);
        let ma = b.machine(1);
        let va = b.store(2);
        let f = b.mover(3);
        let vb = b.store(4);
        let mb = b.machine(5);
        let b0 = b.store(6);
        b.link(&[a0, ma, va, f, vb, mb, b0]);
        let s = Sections::compute(&b.graph(), 64);
        assert_eq!(s.len(), 2);
        assert_eq!(s.list[1].nodes, vec![f, mb]);
        assert_eq!(s.section_of[vb as usize], 1, "vb is interior to B");
        assert_eq!(s.shared, vec![(va, vec![0, 1])]);
    }

    #[test]
    fn a_storage_system_is_one_section_and_idle_movers_none() {
        // Vaults sorted by funnels, no machine: one storage section. A funnel into nothing and a
        // belt between two movers with no store are idle.
        let mut b = B::new();
        let v1 = b.store(0);
        let f1 = b.mover(1);
        let v2 = b.store(2);
        let f2 = b.mover(3);
        let v3 = b.store(4);
        b.link(&[v1, f1, v2, f2, v3]);
        let i1 = b.mover(10);
        let i2 = b.mover(11);
        b.link(&[i1, i2]);
        let s = Sections::compute(&b.graph(), 64);
        assert_eq!(s.len(), 1);
        assert!(!s.list[0].working);
        assert_eq!(s.list[0].nodes, vec![f1, f2]);
        assert_eq!(s.section_of[i1 as usize], NONE);
        assert_eq!(s.section_of[i2 as usize], NONE);
        for h in [v1, v2, v3] {
            assert_eq!(s.section_of[h as usize], 0);
        }
    }

    #[test]
    fn residual_groups_and_the_void_sink() {
        let mut b = B::new();
        // A residual group between two stores is working, apart from a press line on the same
        // source store.
        let a = b.store(0);
        let r = b.node(NodeKind::BlackBox, model::BLACK_BOX, 1);
        b.nodes[r as usize].params = [2, 0, 0, 0];
        let c = b.store(4);
        b.link(&[a, r, c]);
        let f = b.mover(10);
        let m = b.machine(11);
        b.link(&[a, f, m]);
        // A residual touching nothing is idle.
        let lone = b.node(NodeKind::BlackBox, model::BLACK_BOX, 20);
        // Two lines dropping into the one void sink stay apart.
        let void = b.node(NodeKind::Sink, model::SINK_VOID, 0);
        let d1 = b.mover(30);
        let d2 = b.mover(40);
        let s1 = b.store(29);
        let s2 = b.store(41);
        b.link(&[s1, d1, void]);
        b.link(&[s2, d2, void]);
        let s = Sections::compute(&b.graph(), 64);
        let sec = |v: u32| s.section_of[v as usize];
        assert_ne!(sec(r), sec(m));
        assert_eq!(sec(lone), NONE);
        assert_eq!(sec(void), NONE);
        assert_ne!(sec(d1), sec(d2));
        assert_ne!(sec(d1), NONE);
        // The residual's box covers its extent, grown by one block.
        let rb = s.list[sec(r) as usize].bbox.expect("box");
        assert_eq!((rb.0.x, rb.1.x), (-1, 5), "a at 0, the group to 3, c at 4");
        assert!(s.list[sec(r) as usize].holds(BlockPos::new(3, 64, 0)));
        assert!(!s.list[sec(r) as usize].holds(BlockPos::new(3, 66, 0)));
    }

    #[test]
    fn heat_and_control_join_observe_does_not() {
        let mut b = B::new();
        let fuel = b.store(0);
        let feed = b.mover(1);
        let heater = b.node(NodeKind::Heater, model::BLAZE_BURNER, 2);
        let m = b.machine(3);
        let out = b.store(4);
        b.link(&[fuel, feed, heater]);
        b.edge(heater, m, EdgeKind::Heat);
        b.link(&[m, out]);
        // A switch reads `out` and stops a funnel of another line.
        let sw = b.node(NodeKind::Controller, model::THRESHOLD_SWITCH, 10);
        let other = b.store(11);
        let g2 = b.mover(12);
        let m2 = b.machine(13);
        b.link(&[other, g2, m2]);
        b.edge(out, sw, EdgeKind::Observe);
        b.edge(sw, g2, EdgeKind::Control);
        let s = Sections::compute(&b.graph(), 64);
        let sec = |v: u32| s.section_of[v as usize];
        assert_eq!(sec(heater), sec(m), "heat joins");
        assert_eq!(sec(feed), sec(m));
        assert_eq!(sec(sw), sec(g2), "control joins");
        assert_ne!(sec(sw), sec(m), "observe does not");
        assert_eq!(sec(out), sec(m), "an observed store is not shared");
        assert!(s.shared.is_empty());
    }

    fn domain(b: &mut B, members: &[(u32, i32)], driver: Option<u32>) -> u32 {
        let i = b.domains.len();
        for &(v, _) in members {
            b.nodes[v as usize].domain = Some(i as u32);
        }
        b.domains.push(GraphDomain {
            head: head(i),
            network: 99,
            current: Some(0),
            flags: 0,
            live_stress: 0.0,
            live_capacity: 0.0,
            levels: vec![DomainLevel::speed_row(64)],
            sources: vec![match driver {
                Some(v) => DomainSource::driven(v, 0, &[(0, 64, 1024.0)], 0),
                None => DomainSource::fixed(0, 64, 1024.0),
            }],
            foreign: 0,
            members: members
                .iter()
                .map(|&(v, r)| Member {
                    node: Some(v),
                    count: 1,
                    ratio_num: r,
                    ratio_den: 1,
                    impact: 4.0,
                })
                .collect(),
        });
        i as u32
    }

    #[test]
    fn a_shared_domain_does_not_join_and_its_fingerprint_is_the_sections_own() {
        // A boiler section and two lines on its domain.
        let mut b = B::new();
        let water = b.store(0);
        let pump = b.node(NodeKind::Mover, model::PUMP, 1);
        let boiler = b.node(NodeKind::Boiler, model::BOILER, 2);
        b.link(&[water, pump, boiler]);
        let (ca, cb) = (b.store(10), b.store(13));
        let crush = b.node(NodeKind::Machine, model::CRUSHER, 11);
        let fa = b.mover(12);
        b.link(&[ca, crush, fa, cb]);
        let (pa, pb) = (b.store(20), b.store(23));
        let press = b.machine(21);
        let fb = b.mover(22);
        b.link(&[pa, press, fb, pb]);
        let d = domain(&mut b, &[(crush, 1), (press, 2)], Some(boiler));
        let s = Sections::compute(&b.graph(), 64);
        assert_eq!(s.len(), 3);
        let sec = |v: u32| s.section_of[v as usize] as usize;
        assert_eq!(s.list[sec(crush)].domains, vec![d]);
        assert_eq!(s.list[sec(boiler)].domains, vec![d], "the boiler drives it");
        // A machine added elsewhere on the domain changes no other section's fingerprint.
        let before = s.fingerprints();
        let (qa, qb) = (b.store(30), b.store(32));
        let mill = b.node(NodeKind::Machine, model::MILLSTONE, 31);
        b.link(&[qa, mill, qb]);
        b.nodes[mill as usize].domain = Some(d);
        b.domains[d as usize].members.push(Member {
            node: Some(mill),
            count: 1,
            ratio_num: 1,
            ratio_den: 1,
            impact: 8.0,
        });
        let t = Sections::compute(&b.graph(), 64);
        assert_eq!(t.len(), 4);
        for (i, f) in before.iter().enumerate() {
            assert_eq!(t.list[i].fingerprint, *f, "section {i}");
        }
        // The press's own ratio is its build.
        b.domains[d as usize].members[1].ratio_num = 4;
        let u = Sections::compute(&b.graph(), 64);
        let changed: Vec<usize> = (0..4)
            .filter(|&i| u.list[i].fingerprint != t.list[i].fingerprint)
            .collect();
        assert_eq!(changed, vec![sec(press)]);
    }

    #[test]
    fn fingerprints_ignore_node_order_and_state_slots() {
        let (mut b, _) = vault_between_two_lines();
        let heater = b.node(NodeKind::Heater, model::BLAZE_BURNER, 7);
        b.nodes[heater as usize].params = [500, 1, 1600, 400];
        b.edge(heater, 6, EdgeKind::Heat);
        b.recipes.push(GraphRecipe {
            head: head(0),
            recipe_type: 1,
            heat: 0,
            inputs: vec![Term {
                good: Good(1),
                num: 1,
                den: 1,
                flags: 0,
            }],
            outputs: vec![Term {
                good: Good(2),
                num: 1,
                den: 1,
                flags: 0,
            }],
            duration: 100,
            name: 0,
        });
        b.nodes[2].rates.push(RateEntry {
            level: 0,
            recipe: Some(0),
            batch: 1,
            ticks_num: 20,
            ticks_den: 1,
            flags: 0,
        });
        let base = Sections::compute(&b.graph(), 64);
        for seed in 1..20 {
            let s = Sections::compute(&b.shuffled(seed), 64);
            assert_eq!(s.fingerprints(), base.fingerprints(), "seed {seed}");
            assert_eq!(s.group_of, base.group_of);
            let sizes: Vec<usize> = s.list.iter().map(|x| x.nodes.len()).collect();
            let base_sizes: Vec<usize> = base.list.iter().map(|x| x.nodes.len()).collect();
            assert_eq!(sizes, base_sizes);
        }
        // A heater's burn left and level are state.
        b.nodes[heater as usize].params[0] = 20;
        b.nodes[heater as usize].params[1] = 2;
        assert_eq!(
            Sections::compute(&b.graph(), 64).fingerprints(),
            base.fingerprints()
        );
        // Its insertion threshold is build: only its own section changes.
        b.nodes[heater as usize].params[3] = 300;
        let t = Sections::compute(&b.graph(), 64);
        assert_eq!(t.list[0].fingerprint, base.list[0].fingerprint);
        assert_ne!(t.list[1].fingerprint, base.list[1].fingerprint);
        // A recipe's contents are build.
        b.recipes[0].outputs[0].num = 2;
        let u = Sections::compute(&b.graph(), 64);
        assert_ne!(u.list[0].fingerprint, t.list[0].fingerprint);
        assert_eq!(u.list[1].fingerprint, t.list[1].fingerprint);
    }

    #[test]
    fn ties_follow_the_records_not_java_order() {
        // Every node at one position: the fold (f reaches a press and a saw through v, downstream)
        // and the cap break their ties by the nodes' records, so any node order cuts the same.
        let mut b = B::new();
        let s0 = b.store(0);
        let f = b.mover(0);
        let v = b.store(0);
        let m1 = b.machine(0);
        let m2 = b.node(NodeKind::Machine, model::SAW, 0);
        let (o1, o2) = (b.store(0), b.store(0));
        b.link(&[s0, f, v, m1, o1]);
        b.link(&[v, m2, o2]);
        for m in [model::MILLSTONE, model::CRUSHER, model::MILLSTONE] {
            let (a, c) = (b.store(0), b.store(0));
            let x = b.node(NodeKind::Machine, m, 0);
            b.link(&[a, x, c]);
        }
        for max in [64, 4, 3, 2, 1] {
            let base = Sections::compute(&b.graph(), max);
            for seed in 1..30 {
                let s = Sections::compute(&b.shuffled(seed), max);
                assert_eq!(
                    s.fingerprints(),
                    base.fingerprints(),
                    "max {max}, seed {seed}"
                );
                assert_eq!(s.group_of, base.group_of, "max {max}, seed {seed}");
                assert_eq!(s.merged, base.merged);
            }
        }
    }

    #[test]
    fn a_root_change_moves_no_fingerprint_and_a_boilers_heat_is_state() {
        let mut b = B::new();
        let water = b.store(0);
        let pump = b.node(NodeKind::Mover, model::PUMP, 1);
        let boiler = b.node(NodeKind::Boiler, model::BOILER, 2);
        b.link(&[water, pump, boiler]);
        b.nodes[boiler as usize].params = [8, 1, 6, 300];
        let (pa, pb) = (b.store(20), b.store(23));
        let press = b.machine(21);
        let fb = b.mover(22);
        b.link(&[pa, press, fb, pb]);
        b.nodes[press as usize].rates.push(RateEntry {
            level: 0,
            recipe: None,
            batch: 1,
            ticks_num: 20,
            ticks_den: 1,
            flags: 0,
        });
        let d = domain(&mut b, &[(press, 2)], Some(boiler));
        let base = Sections::compute(&b.graph(), 64).fingerprints();
        // Another root: every ratio doubles and the level's speed halves.
        b.domains[d as usize].members[0].ratio_num = 4;
        b.domains[d as usize].levels[0].speed_num = 32;
        assert_eq!(Sections::compute(&b.graph(), 64).fingerprints(), base);
        // The speed itself is build.
        b.domains[d as usize].levels[0].speed_num = 64;
        assert_ne!(Sections::compute(&b.graph(), 64).fingerprints(), base);
        b.domains[d as usize].levels[0].speed_num = 32;
        // A boiler's active heat and water supply are state; its tank is build.
        b.nodes[boiler as usize].params[2] = 18;
        b.nodes[boiler as usize].params[3] = 1200;
        assert_eq!(Sections::compute(&b.graph(), 64).fingerprints(), base);
        b.nodes[boiler as usize].params[0] = 16;
        assert_ne!(Sections::compute(&b.graph(), 64).fingerprints(), base);
    }

    #[test]
    fn the_cap_merges_by_store_then_domain_then_size() {
        // Four independent press lines; two share a vault, two share a domain.
        let mut b = B::new();
        let mut lines = Vec::new();
        for i in 0..4 {
            let x = i * 10;
            let (a, c) = (b.store(x), b.store(x + 3));
            let m = b.machine(x + 1);
            let f = b.mover(x + 2);
            b.link(&[a, m, f, c]);
            lines.push((a, m, f, c));
        }
        // Lines 0 and 1 share a vault: line 1 also draws from line 0's output.
        let g1 = b.mover(15);
        b.link(&[lines[0].3, g1, lines[1].1]);
        domain(&mut b, &[(lines[2].1, 1), (lines[3].1, 1)], None);
        let all = Sections::compute(&b.graph(), 64);
        assert_eq!(all.len(), 4);
        assert_eq!(all.merged, 0);
        let three = Sections::compute(&b.graph(), 3);
        assert_eq!(three.len(), 3);
        let sec = |s: &Sections, v: u32| s.section_of[v as usize];
        assert_eq!(
            sec(&three, lines[0].1),
            sec(&three, lines[1].1),
            "the store first"
        );
        let two = Sections::compute(&b.graph(), 2);
        assert_eq!(
            sec(&two, lines[2].1),
            sec(&two, lines[3].1),
            "then the domain"
        );
        let one = Sections::compute(&b.graph(), 1);
        assert_eq!(one.len(), 1);
        assert_eq!(one.merged, 3);
        assert!(one.shared.is_empty(), "merged sections share nothing");
    }
}
