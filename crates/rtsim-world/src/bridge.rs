// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the bridge reports, applied exactly as it happened in Minecraft (docs/boundary.md §4.1).
//!
//! [`World::trade`] and [`World::caravan_trade`] quote a trade themselves. In game the bridge has
//! already moved the items and coins by its own copy of the price math, from the last snapshot the
//! engine sent, and reports what was really paid and received (TRADE: "the counts are what was
//! really paid and received, reputation included"). These calls book those counts, clamped only
//! where the sim's store holds less than the bridge moved (a snapshot the engine had moved past),
//! with the same reports, witnesses and news as the quoting calls. The other calls here are the
//! events the quoting API has no shape for: a robbery of named goods (CARGO_CHANGED), an assault on
//! a caravan (HURT), and a report at a village about one of its residents.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::{ActorId, Place, Report, ReportId, ReportKind, SEVERITY_UNIT};
use rtsim_trade::{Amount, GoodId, MILLI, PRICE_ONE, Price, Quote, Refusal};

use crate::caravans::Attack;
use crate::observe::Side;
use crate::world::{CaravanId, Cause, World};

/// What one talk with a village's ribbit adds to the village's sentiment toward the player (TK2): a
/// first talk of the day per village and player, the cap kept on the player (TK60). TUNABLE (PLACEHOLDER), in this one place. Weak feelings fade
/// in hours, so a talk lifts a player a little for the day and never makes a friend alone.
pub const TALK_CREDIT: i16 = 3;

/// What an exact trade booked, against what the bridge reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Booked {
    pub quote: Quote,
    /// The bridge moved more than the sim's store held: the difference was not booked.
    pub clamped: bool,
}

impl World {
    /// A resident's trade at node `i` as the bridge reports it: `items` whole items of `g` and
    /// `coins` whole coins of the market's currency, the other way. The player bought (`Buy`) or
    /// sold (`Sell`). A fair trade is a report the market witnesses, and news (as
    /// [`World::trade`]).
    #[allow(clippy::too_many_arguments)]
    pub fn trade_exact(
        &mut self,
        i: usize,
        player: ActorId,
        g: GoodId,
        items: i64,
        coins: i64,
        side: Side,
        at: Tick,
    ) -> Result<Booked, Refusal> {
        if items <= 0 || coins < 0 {
            return Err(Refusal::Nothing);
        }
        self.run_until(at);
        self.release(i, at);
        let region = self.nodes[i].region;
        let result = self
            .markets
            .apply(region, at, |m, _| {
                if !m.is_storehouse() {
                    return Err(Refusal::NotAStorehouse);
                }
                let Some(cur) = m.currency else {
                    return Err(Refusal::NotAStorehouse);
                };
                if cur == g {
                    return Err(Refusal::Nothing);
                }
                let q = match side {
                    Side::Buy => Quote {
                        items: items.min(m.stock(g) / MILLI),
                        coins,
                    },
                    Side::Sell => Quote {
                        items,
                        coins: coins.min(m.stock(cur) / MILLI),
                    },
                };
                let clamped = q.items != items || q.coins != coins;
                if q.items <= 0 {
                    return Err(Refusal::NoStock);
                }
                match side {
                    Side::Buy => m.apply_buy(g, q),
                    Side::Sell => m.apply_sell(g, q),
                }
                Ok(Booked { quote: q, clamped })
            })
            .expect("every node has a market");
        if result.is_ok() {
            self.stats.player_trades += 1;
            let actor = self.nodes[i].actor;
            let place = self.place_of(i);
            let id = self.knowledge.record(Report {
                who: player,
                whom: actor,
                kind: ReportKind::Trade,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place,
            });
            self.knowledge.witness(actor, id, at);
            self.touch_sentiment(i);
            self.flush_gossip();
            self.market_event(i, at, Cause::Player);
        }
        result
    }

    /// A puppet caravan's trade as the bridge reports it (TRADE with an agent): the player bought
    /// (`Buy`: goods out of the cargo, coin into the purse) or sold (`Sell`: coin out of the purse,
    /// goods into it), in whole items and coins of `currency`. As [`World::caravan_trade`]: a fair
    /// trade the caravan carries home, and at a player base the base's price.
    #[allow(clippy::too_many_arguments)]
    pub fn caravan_trade_exact(
        &mut self,
        c: CaravanId,
        at_node: Option<usize>,
        player: ActorId,
        good: GoodId,
        currency: GoodId,
        side: Side,
        items: i64,
        coins: i64,
    ) -> Result<Booked, Refusal> {
        if items <= 0 || coins < 0 || good == currency {
            return Err(Refusal::Nothing);
        }
        let at = self.now;
        let car = self.caravans.get_mut(c).ok_or(Refusal::NoStock)?;
        let (booked_items, booked_coins) = match side {
            // In game the chests hold the flow's cargo and the purse together: what the player
            // took came from either, the flow's cargo first.
            Side::Buy => {
                let mut got = car.take(good, items * MILLI);
                if got < items * MILLI {
                    got += car.take_purse(good, items * MILLI - got);
                }
                car.give_purse(currency, coins * MILLI);
                *self.outside.player_out.entry(good).or_default() += i128::from(got);
                *self.outside.player_in.entry(currency).or_default() += i128::from(coins * MILLI);
                (got, coins * MILLI)
            }
            Side::Sell => {
                let mut paid = car.take_purse(currency, coins * MILLI);
                if paid < coins * MILLI {
                    paid += car.take(currency, coins * MILLI - paid);
                }
                car.give_purse(good, items * MILLI);
                *self.outside.player_out.entry(currency).or_default() += i128::from(paid);
                *self.outside.player_in.entry(good).or_default() += i128::from(items * MILLI);
                (items * MILLI, paid)
            }
        };
        let quote = Quote {
            items: booked_items / MILLI,
            coins: booked_coins / MILLI,
        };
        let clamped = booked_items != items * MILLI || booked_coins != coins * MILLI;
        self.stats.player_trades += 1;
        let (home, holder, place) = {
            let car = self.caravans.get(c).expect("live");
            (car.home, car.holder, self.caravan_place(car, at))
        };
        let rid = self.knowledge.record(Report {
            who: player,
            whom: self.nodes[home].actor,
            kind: ReportKind::Trade,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place,
        });
        self.knowledge.witness(holder, rid, at);
        self.flush_gossip();
        if let Some(b) = at_node.filter(|&b| !self.nodes[b].kind.is_storehouse())
            && items > 0
            && !self.goods.is_currency(good)
        {
            let unit = coins * PRICE_ONE / items;
            let parity = match self.nodes[home].kind.culture() {
                Some(cu) if cu != rtsim_settle::Culture::Human => {
                    self.mint_cost[cu.index()] / self.mint_cost[0]
                }
                _ => 1.0,
            };
            let price = (unit as f64 * parity).round() as Price;
            self.release(b, at);
            let region = self.nodes[b].region;
            self.markets
                .apply(region, at, |m, at| m.record_base_trade(good, price, at));
            self.market_event(b, at, Cause::Player);
        }
        Ok(Booked { quote, clamped })
    }

    /// Goods taken out of an individual caravan (CARGO_CHANGED, negative deltas): they leave the
    /// sim to the player. With a player known it is a robbery the caravan (and any settlement in
    /// sight) reports, and the trader goes on or turns back (as [`World::rob`]); with nobody known
    /// the goods just leave. Returns what was taken and, with a player, the report.
    pub fn rob_goods(
        &mut self,
        c: CaravanId,
        player: Option<ActorId>,
        goods: &BTreeMap<GoodId, Amount>,
    ) -> Option<(BTreeMap<GoodId, Amount>, Option<Attack>)> {
        let at = self.now;
        let mut taken: BTreeMap<GoodId, Amount> = BTreeMap::new();
        {
            let car = self.caravans.get_mut(c)?;
            for (&g, &a) in goods {
                if a <= 0 {
                    continue;
                }
                let mut got = car.take(g, a);
                if got < a {
                    got += car.take_purse(g, a - got);
                }
                if got > 0 {
                    *taken.entry(g).or_default() += got;
                }
            }
        }
        for (&g, &a) in &taken {
            *self.outside.player_out.entry(g).or_default() += i128::from(a);
        }
        let attack = match player {
            Some(p) => {
                let report = self.report(p, c, ReportKind::Robbery, at)?;
                let outcome = self.decide_after_attack(c, at);
                Some(Attack {
                    taken: taken.clone(),
                    outcome,
                    report,
                })
            }
            None => None,
        };
        Some((taken, attack))
    }

    /// Goods put into an individual caravan's cargo by something other than the bridge
    /// (CARGO_CHANGED, positive deltas): they enter the sim from the player's side.
    pub fn cargo_in(&mut self, c: CaravanId, goods: &BTreeMap<GoodId, Amount>) -> bool {
        let Some(car) = self.caravans.get_mut(c) else {
            return false;
        };
        for (&g, &a) in goods {
            if a > 0 {
                car.give(g, a);
                *self.outside.player_in.entry(g).or_default() += i128::from(a);
            }
        }
        true
    }

    /// A player hurt a member of an individual caravan (HURT, PLAN.md §8.7): an assault the
    /// caravan and any settlement in sight report.
    pub fn assault_caravan(&mut self, c: CaravanId, player: ActorId) -> Option<ReportId> {
        let at = self.now;
        self.report(player, c, ReportKind::Assault, at)
    }

    /// Something a player did to or for a resident of node `i` (a resident hurt, killed, or
    /// rescued from a mob): a report the village witnesses first-hand. Vanilla villages skip
    /// writing what vanilla records itself (rtsim-knowledge, E36); news of it travels.
    pub fn village_report(
        &mut self,
        i: usize,
        player: ActorId,
        kind: ReportKind,
        at: Tick,
    ) -> ReportId {
        self.run_until(at);
        let actor = self.nodes[i].actor;
        let place: Place = self.place_of(i);
        let id = self.knowledge.record(Report {
            who: player,
            whom: actor,
            kind,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place,
        });
        self.knowledge.witness(actor, id, at);
        self.touch_sentiment(i);
        self.flush_gossip();
        // A village that saw it tells the next travellers.
        self.emit(i, at, false);
        id
    }

    /// A player talked with one of the village's ribbits (TALKED): the village's sentiment toward
    /// them rises by [`TALK_CREDIT`]. Not a report: nothing happened that news could carry.
    pub fn village_talked(&mut self, i: usize, player: ActorId, at: Tick) {
        self.run_until(at);
        let actor = self.nodes[i].actor;
        self.knowledge.adjust(actor, player, TALK_CREDIT, at);
        self.touch_sentiment(i);
    }

    /// A member left the caravan with nobody to blame (a mob killed it, another mod removed it, a
    /// creature taken by no player): its cargo leaves the sim with it, no report is made, and a
    /// group left with no leader breaks up (C25). `alive`: it was taken, not killed.
    pub fn lose_member(
        &mut self,
        c: CaravanId,
        member: usize,
        alive: bool,
    ) -> Option<crate::caravans::CaravanOutcome> {
        self.lose_member_holding(c, member, alive, None)
    }

    /// [`World::lose_member`] with what the member really held (Review P3).
    pub fn lose_member_holding(
        &mut self,
        c: CaravanId,
        member: usize,
        alive: bool,
        held: Option<&BTreeMap<GoodId, Amount>>,
    ) -> Option<crate::caravans::CaravanOutcome> {
        let at = self.now;
        let m = self.caravans.get(c)?.members.get(member)?;
        if !m.present {
            return None;
        }
        self.member_leaves(c, member, held, alive);
        Some(self.decide_after_attack(c, at))
    }

    /// The node whose knowledge actor is `actor`, if any.
    pub fn node_of_actor(&self, actor: ActorId) -> Option<usize> {
        self.nodes.iter().position(|n| n.actor == actor)
    }
}
