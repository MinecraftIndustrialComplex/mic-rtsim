// SPDX-License-Identifier: AGPL-3.0-or-later
//! What TALK reads of phase 1 (docs/boundary.md §13): read-only views of the link's private
//! tables, so the link keeps its fields private and `talk.rs` asks questions instead of reaching
//! in. Nothing here changes the link.

use std::collections::BTreeSet;

use rtsim_space::topology::Topologies;

use super::{Link, MARKET_CELL};

/// The goods a village's market is shortest and fullest of, for TALK's mood.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::engine) struct Extremes {
    /// The scarcest good's item and its stock per target, thousandths.
    pub scarcest: Option<(String, u32)>,
    /// The most glutted good's item and its stock per target, thousandths.
    pub glutted: Option<(String, u32)>,
}

/// One end of a market's stock: the stock per target (thousandths) and the good, if any good has a
/// target.
type Extreme = Option<(u32, rtsim_trade::GoodId)>;

/// The scarcest and the most glutted of goods with their stock per target (thousandths), in good
/// order: the first of equals wins. One good with a target is both the shortest and the fullest,
/// and saying so twice would have the talker mourn and boast of the same thing, so the glutted is
/// left out then (TK46).
fn pick_extremes(
    ratios: impl IntoIterator<Item = (rtsim_trade::GoodId, u32)>,
) -> (Extreme, Extreme) {
    let mut scarcest: Extreme = None;
    let mut glutted: Extreme = None;
    for (g, ratio) in ratios {
        if scarcest.is_none_or(|(r, _)| ratio < r) {
            scarcest = Some((ratio, g));
        }
        if glutted.is_none_or(|(r, _)| ratio > r) {
            glutted = Some((ratio, g));
        }
    }
    if scarcest.map(|(_, g)| g) == glutted.map(|(_, g)| g) {
        glutted = None;
    }
    (scarcest, glutted)
}

impl Link {
    /// Whether phase 1 runs (U10 switches it off).
    pub(in crate::engine) fn is_on(&self) -> bool {
        self.enabled
    }

    /// The world node of a village or base by Java's id.
    pub(in crate::engine) fn node_of(&self, key: u64) -> Option<usize> {
        self.nodes.get(&key).copied()
    }

    /// The caravan row (its handle bits) a phase 1 agent is, if any.
    pub(in crate::engine) fn caravan_bits(&self, agent: u64) -> Option<u64> {
        super::caravan_of(self, agent)
    }

    /// The keys of the villages and bases whose market cell lies within `r` blocks (Chebyshev,
    /// seams crossed) of a block, in key order. A cell that is near is not a village that is:
    /// the caller measures.
    pub(in crate::engine) fn keys_around(
        &self,
        topo: &Topologies,
        frame: u16,
        x: i64,
        z: i64,
        r: i64,
    ) -> BTreeSet<u64> {
        let mut out = BTreeSet::new();
        for part in topo.of(frame).rects_near(x, z, r) {
            let (x0, z0, x1, z1) = part.grid_cells(MARKET_CELL);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    if let Some(v) = self.cells.get(&(frame, cx, cz)) {
                        out.extend(v.iter().copied());
                    }
                }
            }
        }
        out
    }

    /// The scarcest and most glutted goods of village node `node`'s market, by stock per target
    /// (thousandths); goods with no target and currencies are left out, ties go to the lower good
    /// id. `None` for a node with no storehouse market.
    pub(in crate::engine) fn market_extremes(&self, node: usize) -> Option<Extremes> {
        let n = self.world.nodes.get(node)?;
        if !n.kind.is_storehouse() {
            return None;
        }
        let m = self.world.try_market(node)?;
        let ratios = self.cat.goods.ids().filter_map(|g| {
            if self.cat.goods.is_currency(g) {
                return None;
            }
            let target = m.goods[g.index()].curve.target;
            if target <= 0 {
                return None;
            }
            let ratio = (i128::from(m.stock(g).max(0)) * 1000 / i128::from(target))
                .clamp(0, i128::from(u32::MAX)) as u32;
            Some((g, ratio))
        });
        let (scarcest, glutted) = pick_extremes(ratios);
        Some(Extremes {
            scarcest: scarcest.map(|(r, g)| (self.cat.item(g).to_string(), r)),
            glutted: glutted.map(|(r, g)| (self.cat.item(g).to_string(), r)),
        })
    }

    /// A scripted village for TALK's tests: joins at the world's time with no resources.
    #[cfg(test)]
    pub(in crate::engine) fn join_for_test(
        &mut self,
        key: u64,
        ribbit: bool,
        frame: u16,
        x: i32,
        z: i32,
    ) {
        let at = self.world.now().0;
        self.input(
            rtsim_world::inputs::Input::Join {
                key,
                culture: u8::from(ribbit),
                frame,
                x,
                z,
                pop: 20,
                resources: Vec::new(),
            },
            at,
        );
    }

    /// For TALK's tests: applies an input at the world's time, as an observation would.
    #[cfg(test)]
    pub(in crate::engine) fn input_for_test(&mut self, input: rtsim_world::inputs::Input) {
        let at = self.world.now().0;
        self.input(input, at);
    }

    /// For TALK's tests: marks a village as a test's scripted one.
    #[cfg(test)]
    pub(in crate::engine) fn script_for_test(&mut self, key: u64) {
        self.scripted.insert(key);
    }

    /// For TALK's tests: whether the input log holds a TALKED.
    #[cfg(test)]
    pub(in crate::engine) fn logged_talked_for_test(&self) -> bool {
        self.log
            .iter()
            .any(|l| matches!(l.input, rtsim_world::inputs::Input::Talked { .. }))
    }

    /// A fresh link whose world keeps at most `cap` reports, for TALK's tests of a full table:
    /// call it before anything joins.
    #[cfg(test)]
    pub(in crate::engine) fn small_report_table_for_test(&mut self, cap: u32) {
        let (mut cfg, goods, book) = super::world_parts();
        cfg.knowledge.report_cap = cap;
        self.world = rtsim_world::World::build(cfg, goods, book, Vec::new());
    }

    /// Forgets that the world changed outside the input log, as a snapshot does.
    #[cfg(test)]
    pub(in crate::engine) fn untaint_for_test(&mut self) {
        self.tainted = false;
    }

    /// Whether the world changed outside the input log since the last snapshot.
    #[cfg(test)]
    pub(in crate::engine) fn tainted_for_test(&self) -> bool {
        self.tainted
    }

    /// Switches phase 1 off or on for TALK's tests (U10).
    #[cfg(test)]
    pub(in crate::engine) fn set_on_for_test(&mut self, on: bool) {
        self.enabled = on;
    }
}

#[cfg(test)]
mod tests {
    use rtsim_trade::GoodId;

    use super::pick_extremes;

    #[test]
    fn the_extremes_of_one_good_are_its_scarcity_alone() {
        let (s, g) = pick_extremes([(GoodId(4), 700)]);
        assert_eq!(s, Some((700, GoodId(4))));
        assert_eq!(g, None, "not also the most glutted");
        assert_eq!(pick_extremes([]), (None, None));
    }

    #[test]
    fn goods_at_the_same_stock_leave_none_glutted() {
        // Equals: the first of them wins both, so it is the same good and only scarce.
        let (s, g) = pick_extremes([(GoodId(1), 500), (GoodId(2), 500)]);
        assert_eq!(s, Some((500, GoodId(1))));
        assert_eq!(g, None, "the first of equals is both: left out");
    }

    #[test]
    fn the_scarcest_and_the_most_glutted_of_several_goods() {
        let (s, g) = pick_extremes([(GoodId(1), 900), (GoodId(2), 100), (GoodId(3), 2_400)]);
        assert_eq!(s, Some((100, GoodId(2))));
        assert_eq!(g, Some((2_400, GoodId(3))));
    }
}
