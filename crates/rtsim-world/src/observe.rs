// SPDX-License-Identifier: AGPL-3.0-or-later
//! Player trades and the market snapshot the bridge prices offers from (PLAN.md §8.5; C14, C23,
//! C26).
//!
//! In game the bridge prices a villager's or ribbit merchant's offer on the server thread from the
//! last snapshot the engine sent ([`World::offers`]), lets vanilla run the trade, and reports it
//! with an operation id; the engine applies it at its next step ([`World::trade`]). Here the same
//! two calls run headless.

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::{ActorId, HolderKind, Report, ReportKind, SEVERITY_UNIT};
use rtsim_trade::market::{FAIR, markup_for};
use rtsim_trade::tunables::Ratio;
use rtsim_trade::{GoodId, Offer, OfferSide, Price, Quote, Refusal};

use crate::world::World;

/// Which way a player trade goes, from the player's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The player buys from the market.
    Buy,
    /// The player sells to the market.
    Sell,
}

impl World {
    /// The factors a player's prices are bent by at node `i`. Vanilla villages apply reputation
    /// themselves through gossip (C20), so the sim's quote stays fair there; ribbit villages have
    /// no gossip, so a disliked player's prices rise through the sim's own quote (PLAN.md §8.5,
    /// §8.7). Never better than fair (`markup_for`).
    pub fn markup(&self, i: usize, player: ActorId) -> (Ratio, Ratio) {
        let actor = self.nodes[i].actor;
        match self.knowledge.holder_kind(actor) {
            Some(HolderKind::RibbitVillage) => {
                let s = self.knowledge.sentiment(actor, player, self.now);
                markup_for(s, self.cfg.markup_worst)
            }
            _ => (FAIR, FAIR),
        }
    }

    /// A quote for a player trade at node `i` now, without trading.
    pub fn quote(
        &self,
        i: usize,
        player: ActorId,
        g: GoodId,
        items: i64,
        side: Side,
    ) -> Result<Quote, Refusal> {
        let m = self.market_at(i, self.now);
        let (up, down) = self.markup(i, player);
        match side {
            Side::Buy => m.quote_buy(g, items, up),
            Side::Sell => m.quote_sell(g, items, down),
        }
    }

    /// A player trade at node `i` at `at`, through the treasury (C14, C23): priced by the exact
    /// integral, the buyer paying the ceiling and the seller getting the floor. A fair trade is a
    /// positive report the market witnesses (PLAN.md §8.7).
    pub fn trade(
        &mut self,
        i: usize,
        player: ActorId,
        g: GoodId,
        items: i64,
        side: Side,
        at: Tick,
    ) -> Result<Quote, Refusal> {
        self.run_until(at);
        let (up, down) = self.markup(i, player);
        self.release(i, at);
        let region = self.nodes[i].region;
        let result = self
            .markets
            .apply(region, at, |m, _| {
                let q = match side {
                    Side::Buy => m.quote_buy(g, items, up)?,
                    Side::Sell => m.quote_sell(g, items, down)?,
                };
                match side {
                    Side::Buy => m.apply_buy(g, q),
                    Side::Sell => m.apply_sell(g, q),
                }
                Ok(q)
            })
            .expect("every node has a market");
        if result.is_ok() {
            self.stats.player_trades += 1;
            let actor = self.nodes[i].actor;
            let p = self.nodes[i].pos;
            let id = self.knowledge.record(Report {
                who: player,
                whom: actor,
                kind: ReportKind::Trade,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: rtsim_knowledge::Place::new(p.frame, p.x, 64, p.z),
            });
            self.knowledge.witness(actor, id, at);
            self.touch_sentiment(i);
            self.flush_gossip();
            // A trade moves the stock, and with it the price: news, even with no breakpoint.
            self.market_event(i, at, crate::world::Cause::Player);
        }
        result
    }

    /// The offers a villager or ribbit merchant at node `i` would show `player` now, for every
    /// good the market deals in (PLAN.md §8.5: uses follow stock and treasury).
    pub fn offers(&self, i: usize, player: ActorId) -> Vec<(GoodId, OfferSide, Offer)> {
        let m = self.market_at(i, self.now);
        let (up, down) = self.markup(i, player);
        let mut out = Vec::new();
        for g in self.goods.ids() {
            if self.goods.is_currency(g) {
                continue;
            }
            if let Some(o) = m.offer(g, OfferSide::MarketSells, up) {
                out.push((g, OfferSide::MarketSells, o));
            }
            if let Some(o) = m.offer(g, OfferSide::MarketBuys, down) {
                out.push((g, OfferSide::MarketBuys, o));
            }
        }
        out
    }

    /// Players traded `g` at `price` at a player base (C26): the base's price, carried away as
    /// news by the next caravans.
    pub fn base_trade(&mut self, i: usize, g: GoodId, price: Price, at: Tick) {
        self.run_until(at);
        self.release(i, at);
        let region = self.nodes[i].region;
        self.markets
            .apply(region, at, |m, at| m.record_base_trade(g, price, at))
            .expect("every node has a market");
        self.market_event(i, at, crate::world::Cause::Player);
    }
}
