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
        let mut scarcest: Option<(u32, rtsim_trade::GoodId)> = None;
        let mut glutted: Option<(u32, rtsim_trade::GoodId)> = None;
        for g in self.cat.goods.ids() {
            if self.cat.goods.is_currency(g) {
                continue;
            }
            let target = m.goods[g.index()].curve.target;
            if target <= 0 {
                continue;
            }
            let ratio = (i128::from(m.stock(g).max(0)) * 1000 / i128::from(target))
                .clamp(0, i128::from(u32::MAX)) as u32;
            if scarcest.is_none_or(|(r, _)| ratio < r) {
                scarcest = Some((ratio, g));
            }
            if glutted.is_none_or(|(r, _)| ratio > r) {
                glutted = Some((ratio, g));
            }
        }
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

    /// Switches phase 1 off or on for TALK's tests (U10).
    #[cfg(test)]
    pub(in crate::engine) fn set_on_for_test(&mut self, on: bool) {
        self.enabled = on;
    }
}
