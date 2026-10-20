// SPDX-License-Identifier: AGPL-3.0-or-later
//! Settlements (PLAN.md §8.2): the places markets live, as rows.
//!
//! - [`Settlement`]: one village or player base, with a stable key, a position, a culture and the
//!   population the bridge's census reports. Villages join when their structure start generates
//!   (C22); bases are abstract market nodes (C26) with no culture of their own.
//! - [`Settlements`]: the table, plus a grid index so the edge rule does not scan every pair.
//! - [`EdgeRule`] and [`Settlements::edges_for`]: which settlements are linked by flow edges. The
//!   starting rule is Proposal P18; this crate takes PLAN.md §17's recommendation (Veloren's rule:
//!   each settlement links to its 4 nearest within 400 chunks), recorded as overnight choice E2.
//!
//! Nothing here knows about goods or money: `rtsim-trade` keys its markets by settlement.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::{Arena, Handle};

mod persist;

/// The two phase 1 cultures (C6, C9). Each has its own currency in `rtsim-trade`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Culture {
    /// Vanilla villages: emeralds.
    Human = 0,
    /// Ribbits villages: amethyst.
    Ribbit = 1,
}

impl Culture {
    pub const ALL: [Culture; 2] = [Culture::Human, Culture::Ribbit];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Culture::Human => "human",
            Culture::Ribbit => "ribbit",
        }
    }
}

/// What kind of market node a settlement is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    /// An adopted village of a culture: a storehouse market (C13).
    Village(Culture),
    /// A player base (C26): caravans stop, trade with the players present and carry its prices
    /// away as news. No treasury, stock or production of its own (PLAN.md §8.2). What counts as a
    /// base is Proposal P30; the engine only needs the node.
    PlayerBase { owner: u64 },
    /// A predator haven (C24): where raiders take what they steal, so stolen goods stay in the
    /// world's totals. Its culture decides what money its market keeps.
    Haven(Culture),
}

impl NodeKind {
    /// The culture whose currency the node's market keeps; `None` for player bases.
    pub const fn culture(self) -> Option<Culture> {
        match self {
            NodeKind::Village(c) | NodeKind::Haven(c) => Some(c),
            NodeKind::PlayerBase { .. } => None,
        }
    }

    pub const fn is_storehouse(self) -> bool {
        matches!(self, NodeKind::Village(_) | NodeKind::Haven(_))
    }
}

/// A position in block coordinates within a frame (a Minecraft dimension for phase 1, PLAN.md
/// §5.2). Height does not matter to the edge rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub frame: u16,
    pub x: i32,
    pub z: i32,
}

impl Pos {
    pub const fn new(frame: u16, x: i32, z: i32) -> Self {
        Self { frame, x, z }
    }

    /// Squared distance in blocks, `None` across frames: frames are only linked by strategic
    /// edges, never by the distance rule.
    pub fn dist_sq(self, other: Pos) -> Option<u64> {
        if self.frame != other.frame {
            return None;
        }
        let dx = i64::from(self.x) - i64::from(other.x);
        let dz = i64::from(self.z) - i64::from(other.z);
        Some((dx * dx + dz * dz) as u64)
    }

    /// Straight-line distance in whole blocks, rounded down.
    pub fn dist(self, other: Pos) -> Option<u64> {
        self.dist_sq(other).map(isqrt)
    }
}

/// Integer square root, rounded down. Integer so edge lengths never depend on float rounding.
pub fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = (n as f64).sqrt() as u64;
    // Fix the float estimate up or down to the exact floor.
    while x.checked_mul(x).is_none_or(|sq| sq > n) {
        x -= 1;
    }
    while (x + 1).checked_mul(x + 1).is_some_and(|sq| sq <= n) {
        x += 1;
    }
    x
}

/// One village, base or haven.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    /// Stable across saves: for a village the packed chunk position of its structure start, for
    /// a base the bridge's id. Keys RNG streams and market news, so it never changes.
    pub key: u64,
    pub kind: NodeKind,
    pub pos: Pos,
    /// Residents counted by the census (C2: residents stay vanilla entities). Population is
    /// fixed in phase 1 (economy research); P17 would let it step at a breakpoint.
    pub pop: u32,
    /// When it joined the sim (C22: when its structure start generated).
    pub joined: Tick,
}

pub type SettlementId = Handle<Settlement>;

/// Edges are linked by a rule over distance. P18 is open; the defaults are PLAN.md §17's
/// recommendation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeRule {
    /// Each node links to this many nearest others (P18 recommendation: 4, Veloren's rule).
    pub nearest: usize,
    /// No link longer than this, in blocks (P18 recommendation: 400 chunks = 6,400 blocks).
    pub max_blocks: u64,
    /// Path length over straight-line length, in thousandths. PLACEHOLDER (P18 "path
    /// inefficiency"): 1,000 means straight roads until a phase 0 GameTest measures real paths.
    pub path_factor_milli: u64,
}

impl EdgeRule {
    /// PLAN.md §17, P18's recommendation.
    pub const P18: EdgeRule = EdgeRule {
        nearest: 4,
        max_blocks: 400 * 16,
        path_factor_milli: 1_000,
    };
}

impl Default for EdgeRule {
    fn default() -> Self {
        Self::P18
    }
}

/// A flow edge between two settlements, before any flow exists on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeSpec {
    /// The endpoint with the smaller key comes first, so an edge has one spelling.
    pub a: SettlementId,
    pub b: SettlementId,
    /// Path length in blocks (straight line times the path factor).
    pub length_blocks: u64,
    /// The endpoints keep different currencies: a border edge (PLAN.md §8.4 exchange rates).
    pub border: bool,
}

/// The settlement table with a coarse grid for nearest-neighbour searches.
#[derive(Default)]
pub struct Settlements {
    rows: Arena<Settlement>,
    by_key: BTreeMap<u64, SettlementId>,
    /// (frame, cell x, cell z) to the settlements in that cell.
    grid: BTreeMap<(u16, i32, i32), Vec<SettlementId>>,
}

/// Grid cell edge in blocks. A vanilla village candidate cell is 544 blocks and a Ribbits one 400
/// (PLAN.md §8.2), so a 512-block cell (one region, PLAN.md §5.2) holds about one village.
const CELL: i32 = 512;

fn cell_of(p: Pos) -> (u16, i32, i32) {
    (p.frame, p.x.div_euclid(CELL), p.z.div_euclid(CELL))
}

impl Settlements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Adds a settlement. A key seen before is a bug in the caller (a structure start joins once).
    pub fn insert(&mut self, s: Settlement) -> SettlementId {
        assert!(
            !self.by_key.contains_key(&s.key),
            "settlement key {} joined twice",
            s.key
        );
        let (key, pos) = (s.key, s.pos);
        let id = self.rows.insert(s);
        self.by_key.insert(key, id);
        self.grid.entry(cell_of(pos)).or_default().push(id);
        id
    }

    pub fn remove(&mut self, id: SettlementId) -> Option<Settlement> {
        let s = self.rows.remove(id)?;
        self.by_key.remove(&s.key);
        if let Some(v) = self.grid.get_mut(&cell_of(s.pos)) {
            v.retain(|&x| x != id);
        }
        Some(s)
    }

    /// A row the table itself just found: a stale handle here is a bug in this file.
    fn row(&self, id: SettlementId) -> &Settlement {
        self.rows
            .get(id)
            .expect("settlement handle from the table's own index")
    }

    pub fn get(&self, id: SettlementId) -> Option<&Settlement> {
        self.rows.get(id)
    }

    pub fn get_mut(&mut self, id: SettlementId) -> Option<&mut Settlement> {
        self.rows.get_mut(id)
    }

    pub fn find(&self, key: u64) -> Option<SettlementId> {
        self.by_key.get(&key).copied()
    }

    /// In key order, so every caller sees the same order whatever the insertion history.
    pub fn iter(&self) -> impl Iterator<Item = (SettlementId, &Settlement)> {
        self.by_key.values().map(|&id| (id, self.row(id)))
    }

    /// The `n` nearest others to `id` within `max_blocks`, nearest first, ties by key. Searches
    /// grid rings outward and stops once the ring is beyond the n-th distance found, so the cost
    /// is the settlements nearby, not the table.
    pub fn nearest(&self, id: SettlementId, n: usize, max_blocks: u64) -> Vec<(u64, SettlementId)> {
        let Some(me) = self.rows.get(id) else {
            return Vec::new();
        };
        let (frame, cx, cz) = cell_of(me.pos);
        let max_sq = max_blocks.saturating_mul(max_blocks);
        let max_ring = (max_blocks / CELL as u64) as i32 + 1;
        let mut found: Vec<(u64, u64, SettlementId)> = Vec::new();
        for ring in 0..=max_ring {
            // Every point outside ring r-1 is at least (r-1)·CELL blocks away.
            if found.len() >= n && ring > 0 {
                let reach = (ring as u64 - 1) * CELL as u64;
                let nth = found[n - 1].0;
                if reach.saturating_mul(reach) > nth {
                    break;
                }
            }
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs() != ring && dz.abs() != ring {
                        continue;
                    }
                    let Some(cell) = self.grid.get(&(frame, cx + dx, cz + dz)) else {
                        continue;
                    };
                    for &other in cell {
                        if other == id {
                            continue;
                        }
                        let o = self.row(other);
                        if let Some(d2) = me.pos.dist_sq(o.pos)
                            && d2 <= max_sq
                        {
                            found.push((d2, o.key, other));
                        }
                    }
                }
            }
            found.sort_unstable_by_key(|&(d2, key, _)| (d2, key));
        }
        found.truncate(n);
        found
            .into_iter()
            .map(|(d2, _, id)| (isqrt(d2), id))
            .collect()
    }

    fn spec(&self, x: SettlementId, y: SettlementId, rule: &EdgeRule) -> Option<EdgeSpec> {
        let (sx, sy) = (self.rows.get(x)?, self.rows.get(y)?);
        let straight = sx.pos.dist(sy.pos)?;
        let (a, b) = if sx.key <= sy.key { (x, y) } else { (y, x) };
        let border = match (sx.kind.culture(), sy.kind.culture()) {
            (Some(p), Some(q)) => p != q,
            _ => false,
        };
        Some(EdgeSpec {
            a,
            b,
            // At least one block, so a travel time is never zero.
            length_blocks: (straight * rule.path_factor_milli / 1000).max(1),
            border,
        })
    }

    /// The edges the rule gives the whole table: the union of every node's nearest set, so a
    /// link exists if either end counts the other among its nearest. Sorted by endpoint keys.
    pub fn edges(&self, rule: &EdgeRule) -> Vec<EdgeSpec> {
        let mut out: BTreeMap<(u64, u64), EdgeSpec> = BTreeMap::new();
        for (id, _) in self.iter() {
            for (_, other) in self.nearest(id, rule.nearest, rule.max_blocks) {
                if let Some(e) = self.spec(id, other, rule) {
                    let k = (self.row(e.a).key, self.row(e.b).key);
                    out.entry(k).or_insert(e);
                }
            }
        }
        out.into_values().collect()
    }

    /// The edges a newly joined settlement adds (C22: villages join one at a time as their
    /// structure starts generate): its own nearest, plus each settlement that now counts it among
    /// its nearest. Existing edges are never dropped when a nearer village appears, so flows
    /// already on a road are not cut by exploration (overnight choice E2).
    pub fn edges_for(&self, id: SettlementId, rule: &EdgeRule) -> Vec<EdgeSpec> {
        let mut out: BTreeMap<(u64, u64), EdgeSpec> = BTreeMap::new();
        let mut add = |x: SettlementId, y: SettlementId| {
            if let Some(e) = self.spec(x, y, rule) {
                out.entry((self.row(e.a).key, self.row(e.b).key))
                    .or_insert(e);
            }
        };
        let mine = self.nearest(id, rule.nearest, rule.max_blocks);
        for &(_, other) in &mine {
            add(id, other);
        }
        // Who might now count the newcomer among their nearest: only settlements within reach.
        for (_, other) in self.nearest(id, usize::MAX, rule.max_blocks) {
            if self
                .nearest(other, rule.nearest, rule.max_blocks)
                .iter()
                .any(|&(_, x)| x == id)
            {
                add(id, other);
            }
        }
        out.into_values().collect()
    }

    pub fn heap_bytes(&self) -> usize {
        self.rows.heap_bytes()
            + self.by_key.len() * 32
            + self
                .grid
                .values()
                .map(|v| v.capacity() * 6 + 48)
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn village(key: u64, x: i32, z: i32, c: Culture) -> Settlement {
        Settlement {
            key,
            kind: NodeKind::Village(c),
            pos: Pos::new(0, x, z),
            pop: 10,
            joined: Tick::ZERO,
        }
    }

    #[test]
    fn isqrt_is_exact() {
        for n in [0u64, 1, 2, 3, 4, 15, 16, 17, 1 << 40, u64::MAX] {
            let r = isqrt(n);
            assert!(r * r <= n);
            assert!((r + 1).checked_mul(r + 1).is_none_or(|s| s > n));
        }
    }

    #[test]
    fn nearest_respects_count_and_range() {
        let mut s = Settlements::new();
        let a = s.insert(village(1, 0, 0, Culture::Human));
        for i in 0..10 {
            s.insert(village(10 + i, 600 * (i as i32 + 1), 0, Culture::Human));
        }
        let near = s.nearest(a, 4, 6_400);
        assert_eq!(near.len(), 4);
        assert_eq!(near[0].0, 600);
        assert_eq!(near[3].0, 2_400);
        // Only five lie within 3,000 blocks.
        assert_eq!(s.nearest(a, 100, 3_000).len(), 5);
    }

    #[test]
    fn border_edges_join_cultures() {
        let mut s = Settlements::new();
        s.insert(village(1, 0, 0, Culture::Human));
        s.insert(village(2, 500, 0, Culture::Ribbit));
        s.insert(village(3, 0, 500, Culture::Human));
        let e = s.edges(&EdgeRule::P18);
        assert_eq!(e.len(), 3);
        assert_eq!(e.iter().filter(|e| e.border).count(), 2);
    }

    #[test]
    fn frames_never_link_by_distance() {
        let mut s = Settlements::new();
        s.insert(village(1, 0, 0, Culture::Human));
        let mut far = village(2, 10, 0, Culture::Human);
        far.pos.frame = 1;
        s.insert(far);
        assert!(s.edges(&EdgeRule::P18).is_empty());
    }

    proptest! {
        /// The grid search finds exactly what a scan of every pair finds.
        #[test]
        fn nearest_matches_brute_force(pts in prop::collection::vec((-5000i32..5000, -5000i32..5000), 2..60), n in 1usize..6, range in 100u64..8000) {
            let mut s = Settlements::new();
            let ids: Vec<_> = pts.iter().enumerate().map(|(i, &(x, z))| s.insert(village(i as u64, x, z, Culture::Human))).collect();
            for &id in &ids {
                let me = s.get(id).unwrap().pos;
                let mut brute: Vec<(u64, u64, SettlementId)> = ids.iter().filter(|&&o| o != id).filter_map(|&o| {
                    let st = s.get(o).unwrap();
                    let d2 = me.dist_sq(st.pos)?;
                    (d2 <= range * range).then_some((d2, st.key, o))
                }).collect();
                brute.sort_unstable_by_key(|&(d2, k, _)| (d2, k));
                brute.truncate(n);
                let got: Vec<SettlementId> = s.nearest(id, n, range).into_iter().map(|x| x.1).collect();
                let want: Vec<SettlementId> = brute.into_iter().map(|x| x.2).collect();
                prop_assert_eq!(got, want);
            }
        }

        /// Joining settlements one at a time with `edges_for` gives a superset of the batch rule
        /// on the final table minus nothing it should have: every batch edge whose newer end saw
        /// the older end among its nearest at join time exists.
        #[test]
        fn incremental_edges_cover_each_join(pts in prop::collection::vec((-3000i32..3000, -3000i32..3000), 2..30)) {
            let mut s = Settlements::new();
            let rule = EdgeRule::P18;
            let mut have: BTreeMap<(u64, u64), EdgeSpec> = BTreeMap::new();
            for (i, &(x, z)) in pts.iter().enumerate() {
                let id = s.insert(village(i as u64, x, z, Culture::Human));
                for e in s.edges_for(id, &rule) {
                    have.insert((s.get(e.a).unwrap().key, s.get(e.b).unwrap().key), e);
                }
                // After each join, the new node has every edge of its own nearest set.
                for (_, o) in s.nearest(id, rule.nearest, rule.max_blocks) {
                    let (ka, kb) = (s.get(id).unwrap().key, s.get(o).unwrap().key);
                    let k = (ka.min(kb), ka.max(kb));
                    prop_assert!(have.contains_key(&k));
                }
            }
        }
    }
}
