// SPDX-License-Identifier: AGPL-3.0-or-later
//! Raiders that carry stock off on foot (C24, PLAN.md §10.3): a war band takes goods from a
//! village's (or a rival haven's) market and holds them while it marches home, where
//! [`World::land_held`] stocks them at its haven. Both moves are internal to the conservation sum
//! (a market's stock becomes goods held by another module's row, `Outside::held`, then a haven's
//! stock), so the totals stay exact.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_trade::{Amount, GoodId};

use crate::world::{Cause, World};

impl World {
    /// Takes up to `goods` (thousandths) from node `i`'s stock at `at` into goods held by a row
    /// outside the markets (a war band's loot), booked as a lump out of the market. The market
    /// then re-reads its flows and sends news, as after a predator. Returns what was taken
    /// (nothing at a node with no storehouse).
    pub fn stock_to_held(
        &mut self,
        i: usize,
        goods: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) -> BTreeMap<GoodId, Amount> {
        self.run_until(at);
        let mut out = BTreeMap::new();
        if i >= self.nodes.len() {
            return out;
        }
        let region = self.nodes[i].region;
        self.markets.apply(region, at, |m, _| {
            if !m.is_storehouse() {
                return;
            }
            for (&g, &a) in goods {
                if a <= 0 || g.index() >= m.lines.len() {
                    continue;
                }
                let l = m.line_mut(g);
                let t = a.min(l.stock).max(0);
                if t > 0 {
                    l.stock -= t;
                    l.book.lump_out += t;
                    out.insert(g, t);
                }
            }
        });
        for (g, a) in &out {
            *self.outside.held.entry(*g).or_default() += i128::from(*a);
        }
        if !out.is_empty() {
            self.market_event(i, at, Cause::Predator);
        }
        out
    }
}
