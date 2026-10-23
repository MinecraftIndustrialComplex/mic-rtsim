// SPDX-License-Identifier: AGPL-3.0-or-later
//! Predators with no haven (PLAN.md §9: "dragons destroy; they have no haven"), as seen from the
//! world's books. Raiders take to a haven's market (C24, [`World::set_predation`]); what a dragon
//! takes leaves the world, counted as destroyed in the conservation sum (§8.4), so the totals
//! still balance. Where it went (eaten, burnt or carried to a hoard) is the predator module's own
//! ledger (`rtsim-ecology`).
//!
//! - [`World::take_stock`] and [`World::take_stocks`]: a raid on a village's stock, such as its
//!   livestock (P12's recommendation: village livestock counted as market stock).
//! - [`World::prey_on_traveller`]: an attack on a caravan sampled from a flow. The caravan
//!   becomes an individual row (SCALE.md §3: "a sampled traveller becomes an individual row when
//!   something happens to it"), loses a share of its cargo, and goes on with the rest.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::{Amount, GoodId};
use rtsim_travel::caravan::CaravanKey;

use crate::world::{CaravanId, World};

impl World {
    /// Takes up to `amount` thousandths of `g` from node `i`'s stock at `at`, booked as destroyed
    /// in the market's line. The market then re-reads its flows and sends news, as after a trade:
    /// the price moved with the stock. Returns what was taken (0 for a node with no storehouse).
    pub fn take_stock(&mut self, i: usize, g: GoodId, amount: Amount, at: Tick) -> Amount {
        self.take_stocks(i, &BTreeMap::from([(g, amount)]), at)
            .get(&g)
            .copied()
            .unwrap_or(0)
    }

    /// [`World::take_stock`] for several goods at once, with one release, one market update and
    /// one piece of news (as [`World::stock_to_held`]). Returns what was taken, per good taken.
    pub fn take_stocks(
        &mut self,
        i: usize,
        goods: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) -> BTreeMap<GoodId, Amount> {
        self.run_until(at);
        if i >= self.nodes.len() || goods.values().all(|a| *a <= 0) {
            return BTreeMap::new();
        }
        self.release(i, at);
        let region = self.nodes[i].region;
        let taken = self
            .markets
            .apply(region, at, |m, _| {
                let mut out = BTreeMap::new();
                if !m.is_storehouse() {
                    return out;
                }
                for (&g, &a) in goods {
                    if a <= 0 || g.index() >= m.lines.len() {
                        continue;
                    }
                    let l = m.line_mut(g);
                    let t = a.min(l.stock).max(0);
                    if t > 0 {
                        l.stock -= t;
                        l.book.destroyed += t;
                        out.insert(g, t);
                    }
                }
                out
            })
            .unwrap_or_default();
        if !taken.is_empty() {
            self.market_event(i, at, crate::world::Cause::Predator);
        }
        taken
    }

    /// A predator with no haven falls on the traveller `key` on the road now: it becomes an
    /// individual caravan row ([`World::promote`]), and `share_milli` thousandths of each good it
    /// carries are destroyed. Returns the caravan and what was destroyed, or `None` when no such
    /// caravan is on the road now.
    pub fn prey_on_traveller(
        &mut self,
        key: CaravanKey,
        share_milli: i64,
    ) -> Option<(CaravanId, BTreeMap<GoodId, Amount>)> {
        let id = self.promote(key)?;
        let cargo = self.caravans.get(id)?.cargo();
        let mut taken = BTreeMap::new();
        for (g, a) in cargo {
            let t = mul_div_floor(a, share_milli.clamp(0, 1000), 1000);
            let got = self.caravans.get_mut(id)?.take(g, t);
            if got > 0 {
                *self.outside.destroyed.entry(g).or_default() += i128::from(got);
                taken.insert(g, got);
            }
        }
        Some((id, taken))
    }

    /// Everything the world's books count as destroyed so far: caravan cargo and market stock,
    /// per good, at `at`.
    pub fn destroyed_total(&self, at: Tick) -> BTreeMap<GoodId, i128> {
        let mut out = self.outside.destroyed.clone();
        for i in 0..self.nodes.len() {
            let m = self.market_at(i, at);
            for (gi, l) in m.lines.iter().enumerate() {
                if l.book.destroyed != 0 {
                    *out.entry(GoodId(gi as u16)).or_default() += i128::from(l.book.destroyed);
                }
            }
        }
        out.retain(|_, v| *v != 0);
        out
    }
}
