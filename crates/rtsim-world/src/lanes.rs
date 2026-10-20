// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lanes: edges that timetabled ships travel instead of caravans (`rtsim-ships`, C5; overnight
//! choices T3 to T8 in docs/overnight-choices-ships.md).
//!
//! A lane is an ordinary edge of the world, so merchant ships carry flows of goods between port
//! markets exactly as caravans do on roads (A13): the same allocation by news margin, pipelines,
//! quotes, payments, news and conservation. What differs is set by the ships engine:
//! - its travel time and dock wait come from the ships' timetable, not walking speed;
//! - it carries at most the ships' holds per quantum ([`rtsim_travel::Edge::cap_slot_milli`]);
//! - it is not in the road index, so no caravan is ever sampled on it.
//!
//! The rest of this module is what the ships engine needs from the world's books:
//! - [`World::cut_lane_window`]: a ship becomes an individual row (SCALE.md §3): the
//!   consignments it carries are cut out of the flow (goods, payments and the raided share) and
//!   counted as [`crate::world::Outside::held`] until the row gives them back;
//! - [`World::land_held`], [`World::held_to_players`], [`World::destroy_held`]: where held goods
//!   go (a market's stock, a player, the sea floor);
//! - [`World::stock_from_players`]: goods stolen from a player's district enter the world at a
//!   haven's market;
//! - [`World::set_lane_hazard`]: pirates on a lane (C24): a share of its flow sails to their
//!   haven's market, re-allocated at once.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::{Amount, GoodId, PRICE_ONE, Price};
use rtsim_travel::edge::{DirRef, Edge, Medium, Predation};

use crate::world::{Cause, Ev, World};

/// A lane to add, from the ships engine's network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneSpec {
    pub a: usize,
    pub b: usize,
    pub medium: Medium,
    /// Length along the lane's path, blocks.
    pub length_blocks: u64,
    /// Quanta from a ship's departure to its arrival, at least 1.
    pub d_quanta: u64,
    /// The longest a consignment waits at the dock for a ship, quanta: the longest gap between
    /// departures in the timetable.
    pub dock_quanta: u64,
    /// The most a direction carries per quantum, thousandths of a slot.
    pub cap_slot_milli: i64,
}

/// What a ship took out of a flow when it became an individual row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cut {
    /// Goods and coins, thousandths.
    pub goods: BTreeMap<GoodId, Amount>,
    /// The quote each good left with, in the payer's currency.
    pub quotes: BTreeMap<GoodId, Price>,
}

impl World {
    /// Adds a lane between nodes `a` and `b` and sends both ends' news along it so trade can
    /// start. Returns the new edge's index. A lane between two cultures opens the exchange lines
    /// a border edge needs, as [`World::join`] does.
    pub fn add_lane(&mut self, spec: LaneSpec, at: Tick) -> u32 {
        self.run_until(at);
        let e = self.edges.len() as u32;
        let (ka, kb) = (self.nodes[spec.a].key, self.nodes[spec.b].key);
        let seed = rtsim_kernel::rng::StreamKey::new(
            self.cfg.seed,
            rtsim_kernel::rng::SystemId::named("ships.lane"),
            u64::from(spec.medium as u8),
        )
        .block(ka.min(kb), ka.max(kb))[0];
        let (ca, cb) = (
            self.nodes[spec.a].kind.culture(),
            self.nodes[spec.b].kind.culture(),
        );
        let border = ca.is_some() && cb.is_some() && ca != cb;
        let idle = spec.dock_quanta.max(1);
        let mut edge = Edge::new(
            spec.a,
            spec.b,
            spec.length_blocks,
            spec.d_quanta,
            spec.dock_quanta,
            seed,
            border,
            idle,
        );
        edge.medium = spec.medium;
        edge.cap_slot_milli = spec.cap_slot_milli;
        self.edges.push(edge);
        self.nodes[spec.a].out.push(DirRef { edge: e, dir: 0 });
        self.nodes[spec.b].out.push(DirRef { edge: e, dir: 1 });
        if !self.nodes[spec.a].neighbours.contains(&spec.b) {
            self.nodes[spec.a].neighbours.push(spec.b);
        }
        if !self.nodes[spec.b].neighbours.contains(&spec.a) {
            self.nodes[spec.b].neighbours.push(spec.a);
        }
        if border {
            self.open_exchange(spec.a, spec.b, spec.d_quanta, at);
            self.open_exchange(spec.b, spec.a, spec.d_quanta, at);
        }
        self.emit(spec.a, at, true);
        self.emit(spec.b, at, true);
        e
    }

    /// Cuts the consignments that left on direction `dir` of edge `edge` at boundaries
    /// `lo..=hi` out of its goods, payment and raid pipelines, as far as they are still on their
    /// way at the world's time, and counts them as held. Wakes the destination and the haven
    /// where their arrivals change. `None` for a bad edge or direction.
    pub fn cut_lane_window(&mut self, edge: u32, dir: u8, lo: u64, hi: u64) -> Option<Cut> {
        let at = self.now;
        let q = self.cfg.trade.quantum;
        let b = q.index(at);
        let e = self.edges.get(edge as usize)?;
        let lag = e.lag();
        let d = e.dirs.get(usize::from(dir))?;
        let to = d.to;
        let raid = d.predation;
        let mut cut = Cut::default();
        if hi < lo {
            return Some(cut);
        }
        let lo_flow = lo.max((b + 1).saturating_sub(lag));
        let dirm = &mut self.edges[edge as usize].dirs[usize::from(dir)];
        if lo_flow <= hi {
            for (g, p) in dirm.goods.iter_mut().chain(dirm.pay.iter_mut()) {
                if overlaps_hole(p, lo_flow, hi) {
                    continue;
                }
                let a = p.cut(lo_flow, hi);
                if a > 0 {
                    *cut.goods.entry(*g).or_default() += a;
                    cut.quotes.insert(*g, p.quote_at(hi));
                }
            }
        }
        let mut raid_wakes = Vec::new();
        if let Some(pr) = raid {
            let lo_raid = lo.max((b + 1).saturating_sub(pr.delay));
            if lo_raid <= hi {
                for (g, p) in dirm.raid.iter_mut() {
                    if overlaps_hole(p, lo_raid, hi) {
                        continue;
                    }
                    let a = p.cut(lo_raid, hi);
                    if a > 0 {
                        *cut.goods.entry(*g).or_default() += a;
                        // A raided share was never quoted: the destination's quote stands.
                        cut.quotes.entry(*g).or_insert(0);
                    }
                }
                raid_wakes.push((pr.haven, lo_raid, pr.delay));
            }
        }
        for first in [lo_flow, hi + 1] {
            let wake = q.at(first + lag - 1).max(at);
            self.schedule(wake, Ev::Wake(to));
        }
        for (haven, lo_raid, delay) in raid_wakes {
            for first in [lo_raid, hi + 1] {
                let wake = q.at(first + delay - 1).max(at);
                self.schedule(wake, Ev::Wake(haven));
            }
        }
        for (g, a) in &cut.goods {
            *self.outside.held.entry(*g).or_default() += i128::from(*a);
        }
        Some(cut)
    }

    fn take_held(&mut self, g: GoodId, a: Amount) -> Amount {
        let h = self.outside.held.entry(g).or_default();
        let t = i128::from(a.max(0)).min(*h).max(0);
        *h -= t;
        if *h == 0 {
            self.outside.held.remove(&g);
        }
        t as Amount
    }

    /// A ship row lands what it holds at node `i`: goods are sold to the market at their quotes
    /// as far as its treasury pays (C14, as a caravan row sells), and the payment is credited to
    /// node `paid_to`'s treasury at once (overnight choice T7); coins and what the treasury
    /// cannot pay for are stocked all the same. Returns what was stocked.
    pub fn land_held(
        &mut self,
        i: usize,
        paid_to: Option<usize>,
        goods: &BTreeMap<GoodId, Amount>,
        quotes: &BTreeMap<GoodId, Price>,
        at: Tick,
    ) -> BTreeMap<GoodId, Amount> {
        self.run_until(at);
        let mut stocked = BTreeMap::new();
        if i >= self.nodes.len() {
            return stocked;
        }
        let mut take: Vec<(GoodId, Amount)> = Vec::new();
        for (&g, &a) in goods {
            let t = self.take_held(g, a);
            if t > 0 {
                take.push((g, t));
            }
        }
        let payee = paid_to.filter(|&j| {
            j != i && j < self.nodes.len() && self.nodes[j].kind.is_storehouse()
        });
        let region = self.nodes[i].region;
        let gd = &self.goods;
        let mut paid: Option<(GoodId, Amount)> = None;
        let storehouse = self
            .markets
            .apply(region, at, |m, _| {
                if !m.is_storehouse() {
                    return false;
                }
                let mut value: Amount = 0;
                for &(g, a) in &take {
                    if g.index() >= m.lines.len() {
                        continue;
                    }
                    let l = m.line_mut(g);
                    l.stock += a;
                    l.book.lump_in += a;
                    if !gd.is_currency(g) {
                        let quote = quotes.get(&g).copied().unwrap_or(0).max(0);
                        value += mul_div_floor(a, quote, PRICE_ONE);
                    }
                }
                if let (Some(cur), true) = (m.currency, payee.is_some()) {
                    let pay = value.min(m.stock(cur)).max(0);
                    if pay > 0 {
                        let t = m.line_mut(cur);
                        t.stock -= pay;
                        t.book.lump_out += pay;
                        paid = Some((cur, pay));
                    }
                }
                true
            })
            .unwrap_or(false);
        if !storehouse {
            // A base keeps no stock: the goods stay held (the caller keeps them aboard).
            for (g, a) in take {
                *self.outside.held.entry(g).or_default() += i128::from(a);
            }
            return stocked;
        }
        for (g, a) in take {
            stocked.insert(g, a);
        }
        if let (Some((cur, pay)), Some(j)) = (paid, payee) {
            let region = self.nodes[j].region;
            self.markets.apply(region, at, |m, _| {
                let l = m.line_mut(cur);
                l.stock += pay;
                l.book.lump_in += pay;
            });
            self.market_event(j, at, Cause::Lane);
        }
        self.market_event(i, at, Cause::Lane);
        stocked
    }

    /// Held goods a player took (looted from a ship's hold): they leave the world to the player.
    pub fn held_to_players(&mut self, goods: &BTreeMap<GoodId, Amount>) -> BTreeMap<GoodId, Amount> {
        let mut out = BTreeMap::new();
        for (&g, &a) in goods {
            let t = self.take_held(g, a);
            if t > 0 {
                *self.outside.player_out.entry(g).or_default() += i128::from(t);
                out.insert(g, t);
            }
        }
        out
    }

    /// Held goods lost with a ship (sunk with nobody to take them): destroyed in the world's
    /// books.
    pub fn destroy_held(&mut self, goods: &BTreeMap<GoodId, Amount>) -> BTreeMap<GoodId, Amount> {
        let mut out = BTreeMap::new();
        for (&g, &a) in goods {
            let t = self.take_held(g, a);
            if t > 0 {
                *self.outside.destroyed.entry(g).or_default() += i128::from(t);
                out.insert(g, t);
            }
        }
        out
    }

    /// Goods a player put aboard a ship row (a player's cargo, or loot a player gave back): they
    /// enter the world from the player's side and are held.
    pub fn held_from_players(&mut self, goods: &BTreeMap<GoodId, Amount>) {
        for (&g, &a) in goods {
            if a > 0 {
                *self.outside.player_in.entry(g).or_default() += i128::from(a);
                *self.outside.held.entry(g).or_default() += i128::from(a);
            }
        }
    }

    /// Goods taken from a player's district (the industry engine booked them stolen) enter the
    /// world at node `i`'s market, as players' goods coming in (C24: stolen goods go home and are
    /// sold there). Returns what was stocked; nothing at a node with no storehouse.
    pub fn stock_from_players(
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
                l.stock += a;
                l.book.player_in += a;
                out.insert(g, a);
            }
        });
        if !out.is_empty() {
            self.market_event(i, at, Cause::Lane);
        }
        out
    }

    /// Pirates from the haven at node `haven` take `share_milli` thousandths of everything leaving
    /// on direction `dir` of lane `edge` (C24), arriving there `delay` quanta after it left; 0
    /// stops them. The origin's flows are re-allocated at `at`, so the change applies from the
    /// next boundary. A direction keeps one haven while anything raided from it is still on its
    /// way: a different haven is refused (false) until the raids in transit have arrived.
    pub fn set_lane_hazard(
        &mut self,
        edge: u32,
        dir: u8,
        haven: usize,
        share_milli: i64,
        delay: u64,
        at: Tick,
    ) -> bool {
        self.run_until(at);
        let Some(e) = self.edges.get(edge as usize) else {
            return false;
        };
        let Some(d) = e.dirs.get(usize::from(dir)) else {
            return false;
        };
        if haven >= self.nodes.len() {
            return false;
        }
        let b = self.cfg.trade.quantum.index(at);
        let from = d.from;
        let dref = DirRef { edge, dir };
        if let Some(old) = d.predation
            && old.haven != haven
        {
            let idle = old.share_milli == 0 && d.raid.values().all(|p| p.is_idle(b + 1));
            if !idle {
                return false;
            }
            self.nodes[old.haven].raids_in.retain(|r| *r != dref);
            self.edges[edge as usize].dirs[usize::from(dir)].raid = rtsim_travel::VecMap::new();
        }
        let d = &mut self.edges[edge as usize].dirs[usize::from(dir)];
        let delay = delay.max(1);
        let unchanged = d
            .predation
            .is_some_and(|p| p.haven == haven && p.share_milli == share_milli);
        // A raid pipeline's delay is fixed when it is made; a later delay applies to a fresh one.
        let delay = d.raid.values().next().map_or(delay, |p| p.delay);
        d.predation = Some(Predation {
            haven,
            share_milli: share_milli.clamp(0, 1000),
            delay,
        });
        if !self.nodes[haven].raids_in.contains(&dref) {
            self.nodes[haven].raids_in.push(dref);
        }
        if !unchanged {
            self.market_event(from, at, Cause::Lane);
        }
        true
    }

    /// The current hazard on a lane direction: (haven, share in thousandths), if any.
    pub fn lane_hazard(&self, edge: u32, dir: u8) -> Option<(usize, i64)> {
        let d = self.edges.get(edge as usize)?.dirs.get(usize::from(dir))?;
        d.predation.map(|p| (p.haven, p.share_milli))
    }
}

/// Whether consignments `lo..=hi` meet a hole already cut (a ship's window is cut once).
fn overlaps_hole(p: &rtsim_travel::Pipeline, lo: u64, hi: u64) -> bool {
    p.holes().iter().any(|h| h.from <= hi && lo <= h.to)
}
