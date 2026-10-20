// SPDX-License-Identifier: AGPL-3.0-or-later
//! Player bases as market nodes, and caravans trading with players (C26, P30's recommendation;
//! PLAN.md §8.2, §8.5, walkthrough J).
//!
//! - **What a base is** (P30's recommendation): a place a player spends much time. The bridge keeps
//!   the decaying dwell counter per 128-block cell and reports BASE MARKED or UNMARKED per (base,
//!   player) (docs/boundary.md §4.1); [`World::mark_base`] makes the base a node the first time and
//!   [`World::unmark_base`] retires it as a stop when no player marks it any more. A base is not a
//!   storehouse: no stock, treasury or production; its prices are the prices players trade at.
//! - **Stops**: a marked base near a road becomes a stop on it, so the travellers sampled on that
//!   road say they stop there ([`rtsim_travel::Traveller::stops`]); the bridge walks them through.
//! - **Trading**: a caravan trades only through its own offers ([`World::caravan_offers`]), with
//!   the players present: it sells its cargo at its destination's prices as of the news it carries
//!   and buys what its home market lacks at its home's prices, up to the coin it carries, in its
//!   culture's currency (PLAN.md §8.5 "Sim caravans"). A trade is a consequence, so the caravan
//!   becomes an individual row; its price at a base becomes the base's news.

use std::collections::BTreeSet;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::{ActorId, Place, Report, ReportKind, SEVERITY_UNIT};
use rtsim_settle::{NodeKind, Pos};
use rtsim_trade::market::{FAIR, markup_for};
use rtsim_trade::tunables::Ratio;
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::{Amount, GoodId, MILLI, OfferSide, PRICE_ONE, Price, Quote, Refusal};
use rtsim_travel::caravan::CaravanKey;
use rtsim_travel::roads::dist2_to_segment;

use crate::flows::to_home;
use crate::observe::Side;
use crate::world::{CaravanId, Cause, NodeData, World};

/// Who marks a base (P30).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BaseState {
    pub players: BTreeSet<ActorId>,
}

/// A caravan to trade with: a traveller sampled from a flow, or an individual row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaravanRef {
    Sampled(CaravanKey),
    Row(CaravanId),
}

/// One of a caravan's offers, vanilla-shaped (the MERCHANT_OFFERS command): `items` of the good
/// for `coins` of `currency`, `uses` times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaravanOffer {
    pub good: GoodId,
    /// From the caravan's side: it sells (the player buys) or it buys.
    pub side: OfferSide,
    pub items: i64,
    pub coins: i64,
    pub currency: GoodId,
    pub uses: u32,
}

/// The integer bundle for a price in millionths of a coin per item: one item for whole coins when
/// an item is worth a coin or more, else whole items for one coin, rounded against the player.
fn bundle(price: Price, caravan_sells: bool) -> Option<(i64, i64)> {
    if price <= 0 {
        return None;
    }
    Some(if price >= PRICE_ONE {
        let coins = if caravan_sells {
            (price + PRICE_ONE - 1) / PRICE_ONE
        } else {
            price / PRICE_ONE
        };
        (1, coins.max(1))
    } else {
        let items = if caravan_sells {
            PRICE_ONE / price
        } else {
            (PRICE_ONE + price - 1) / price
        };
        (items.max(1), 1)
    })
}

impl World {
    /// A player's dwell counter marked a base (P30): the base joins as a node the first time, and
    /// becomes a stop on the roads that pass near it.
    pub fn mark_base(&mut self, key: u64, pos: Pos, player: ActorId, at: Tick) -> usize {
        let node = match self.node_of_key(key) {
            Some(i) => i,
            None => {
                let owner = self.knowledge.actors().external(player).unwrap_or(0);
                self.join(
                    NodeData {
                        key,
                        kind: NodeKind::PlayerBase { owner },
                        pos,
                        pop: 0,
                        resources: Default::default(),
                    },
                    at,
                )
            }
        };
        let st = self.bases.entry(node).or_default();
        let was_empty = st.players.is_empty();
        st.players.insert(player);
        if was_empty {
            self.add_stops(node);
        }
        node
    }

    /// A player's dwell counter fell off (UNMARKED). A base no player marks stops being a stop;
    /// its node stays (news about it may still be on the roads).
    pub fn unmark_base(&mut self, key: u64, player: ActorId) {
        let Some(node) = self.node_of_key(key) else {
            return;
        };
        let Some(st) = self.bases.get_mut(&node) else {
            return;
        };
        st.players.remove(&player);
        if st.players.is_empty() {
            for e in &mut self.edges {
                e.stops.retain(|&s| s != node);
            }
        }
    }

    /// Whether a base is marked by any player.
    pub fn base_active(&self, node: usize) -> bool {
        self.bases.get(&node).is_some_and(|b| !b.players.is_empty())
    }

    fn add_stops(&mut self, node: usize) {
        let p = self.nodes[node].pos;
        let r = i64::from(self.cfg.base_stop_blocks);
        let here = (i64::from(p.x), i64::from(p.z));
        for e in self.roads.near(p.frame, here.0, here.1, r) {
            let (a, b) = (self.edges[e as usize].a, self.edges[e as usize].b);
            if a == node || b == node {
                continue;
            }
            let (pa, pb) = (self.nodes[a].pos, self.nodes[b].pos);
            if pa.frame != p.frame {
                continue;
            }
            let d2 = dist2_to_segment(
                here,
                (i64::from(pa.x), i64::from(pa.z)),
                (i64::from(pb.x), i64::from(pb.z)),
            );
            if d2 <= (r * r) as f64 && !self.edges[e as usize].stops.contains(&node) {
                self.edges[e as usize].stops.push(node);
            }
        }
    }

    /// The caravan's offers to `player` now (MERCHANT_OFFERS): it sells its cargo at its
    /// destination's prices as of the news it carries, and buys what its home market lacks at its
    /// home's prices, up to the coin it carries (PLAN.md §8.5). A player its home dislikes pays
    /// more and is paid less (E7: never better than fair).
    pub fn caravan_offers(&self, c: CaravanRef, player: ActorId) -> Vec<CaravanOffer> {
        let Some((from, to, cargo)) = self.caravan_parts(c) else {
            return Vec::new();
        };
        let Some(cur) = self.nodes[from].currency else {
            return Vec::new();
        };
        let (up, down) = self.caravan_markup(from, player);
        let home = self.market(from);
        let mine = self.nodes[from].last_sent.clone();
        // The prices it sells at: where it is going, as of the news it carries (its home's own
        // when it is walking home).
        let news = if to == from {
            mine.clone()
        } else {
            self.nodes[from].news.get(to).cloned()
        };
        let cash = cargo.get(&cur).copied().unwrap_or(0);
        let mut out = Vec::new();
        for g in self.goods.ids() {
            if self.goods.is_currency(g) {
                continue;
            }
            // Sell what it carries, at the destination's price in our coin.
            let have = cargo.get(&g).copied().unwrap_or(0);
            if have >= MILLI
                && let Some(snap) = &news
                && let Some(p) = to_home(&home, snap.currency, snap.good(g).price)
                && let Some((items, coins)) = bundle(mul_div_floor(p, up.num, up.den), true)
            {
                let uses = (have / (items * MILLI)).min(i64::from(u32::MAX)) as u32;
                if uses > 0 {
                    out.push(CaravanOffer {
                        good: g,
                        side: OfferSide::MarketSells,
                        items,
                        coins,
                        currency: cur,
                        uses,
                    });
                }
            }
            // Buy what home lacks, at home's price, with the coin it carries.
            if let Some(m) = &mine {
                let gn = m.good(g);
                if gn.level <= rtsim_trade::Level::Below
                    && gn.target > 0
                    && let Some((items, coins)) =
                        bundle(mul_div_floor(gn.price, down.num, down.den), false)
                {
                    let uses = (cash / (coins * MILLI)).min(i64::from(u32::MAX)) as u32;
                    if uses > 0 {
                        out.push(CaravanOffer {
                            good: g,
                            side: OfferSide::MarketBuys,
                            items,
                            coins,
                            currency: cur,
                            uses,
                        });
                    }
                }
            }
        }
        out
    }

    /// The markup a caravan of `home` applies to `player`: its home's feelings (PLAN.md §8.7).
    fn caravan_markup(&self, home: usize, player: ActorId) -> (Ratio, Ratio) {
        let s = self
            .knowledge
            .sentiment(self.nodes[home].actor, player, self.now);
        if s >= 0 {
            (FAIR, FAIR)
        } else {
            markup_for(s, self.cfg.markup_worst)
        }
    }

    /// (origin, destination, cargo) of a caravan.
    fn caravan_parts(
        &self,
        c: CaravanRef,
    ) -> Option<(usize, usize, std::collections::BTreeMap<GoodId, Amount>)> {
        match c {
            CaravanRef::Sampled(k) => self.traveller(k).map(|t| (t.from, t.to, t.cargo)),
            // Where it is walking now: its destination outbound, its home on the way back.
            CaravanRef::Row(id) => self
                .caravans
                .get(id)
                .map(|car| (car.home, car.leg.to, car.cargo())),
        }
    }

    /// A player trades `bundles` of one of the caravan's offers (the TRADE observation for a
    /// puppet merchant). The caravan becomes an individual row (a trade is a consequence,
    /// SCALE.md §3); goods and coin cross to and from the player; the trade is a fair-trade
    /// report its home will hear; at a player base, its price becomes the base's news (C26).
    pub fn caravan_trade(
        &mut self,
        c: CaravanRef,
        at_node: Option<usize>,
        player: ActorId,
        good: GoodId,
        side: Side,
        bundles: u32,
    ) -> Result<(CaravanId, Quote), Refusal> {
        if bundles == 0 {
            return Err(Refusal::Nothing);
        }
        let want = match side {
            Side::Buy => OfferSide::MarketSells,
            Side::Sell => OfferSide::MarketBuys,
        };
        let offer = self
            .caravan_offers(c, player)
            .into_iter()
            .find(|o| o.good == good && o.side == want)
            .ok_or(Refusal::NoStock)?;
        if offer.uses < bundles {
            return Err(match side {
                Side::Buy => Refusal::NoStock,
                Side::Sell => Refusal::NoMoney,
            });
        }
        let id = match c {
            CaravanRef::Row(id) => id,
            CaravanRef::Sampled(k) => self.promote(k).ok_or(Refusal::NoStock)?,
        };
        let items = offer.items * i64::from(bundles);
        let coins = offer.coins * i64::from(bundles);
        let (cur, at) = (offer.currency, self.now);
        let car = self.caravans.get_mut(id).ok_or(Refusal::NoStock)?;
        match side {
            Side::Buy => {
                let got = car.take(good, items * MILLI);
                debug_assert_eq!(got, items * MILLI);
                car.give(cur, coins * MILLI);
                *self.outside.player_out.entry(good).or_default() += i128::from(got);
                *self.outside.player_in.entry(cur).or_default() += i128::from(coins * MILLI);
            }
            Side::Sell => {
                let paid = car.take(cur, coins * MILLI);
                debug_assert_eq!(paid, coins * MILLI);
                car.give(good, items * MILLI);
                *self.outside.player_out.entry(cur).or_default() += i128::from(paid);
                *self.outside.player_in.entry(good).or_default() += i128::from(items * MILLI);
            }
        }
        self.stats.player_trades += 1;
        // A fair trade: the caravan saw it, and carries it home (PLAN.md §8.7).
        let (home, holder, place) = {
            let car = self.caravans.get(id).expect("live");
            (car.home, car.holder, self.caravan_place(car, at))
        };
        let rid = self.knowledge.record(Report {
            who: player,
            whom: self.nodes[home].actor,
            kind: ReportKind::Trade,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place: Place::new(place.frame, place.x, place.y, place.z),
        });
        self.knowledge.witness(holder, rid, at);
        self.flush_gossip();
        // At a base, the price players traded at is the base's news (C26), in emeralds at mint
        // parity whatever the caravan's coin, so a base's prices compare.
        if let Some(b) = at_node.filter(|&b| !self.nodes[b].kind.is_storehouse()) {
            let unit = coins * PRICE_ONE / items.max(1);
            let parity = match self.nodes[home].kind.culture() {
                Some(c) if c != rtsim_settle::Culture::Human => {
                    self.mint_cost[c.index()] / self.mint_cost[0]
                }
                _ => 1.0,
            };
            let price = (unit as f64 * parity).round() as Price;
            let region = self.nodes[b].region;
            self.markets
                .apply(region, at, |m, at| m.record_base_trade(good, price, at));
            self.market_event(b, at, Cause::Player);
        }
        Ok((id, Quote { items, coins }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_round_against_the_player() {
        // 2.5 coins an item: the player pays 3, is paid 2.
        assert_eq!(bundle(2_500_000, true), Some((1, 3)));
        assert_eq!(bundle(2_500_000, false), Some((1, 2)));
        // 0.3 coins: 3 items for a coin when buying from the caravan, 4 when selling to it.
        assert_eq!(bundle(300_000, true), Some((3, 1)));
        assert_eq!(bundle(300_000, false), Some((4, 1)));
        assert_eq!(bundle(0, true), None);
    }
}
