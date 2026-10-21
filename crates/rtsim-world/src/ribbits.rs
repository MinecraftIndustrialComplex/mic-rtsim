// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ribbit behaviours the sim drives (C7, PLAN.md §8.8): sorcerers who remember, village growth
//! from YUNG's house templates, and touring bands. Merchant caravans are ordinary caravans.
//!
//! - **Sorcerers** ([`World::sentiment_snapshot`], [`World::sorcerer_includes`]): the bridge's
//!   mixin on `RibbitApplyBuffGoal` filters the players near a sorcerer by its village's sentiment
//!   snapshot (the SENTIMENT command of docs/boundary.md §4.2): at or below the dislike threshold a
//!   player is left out. Snapshots are pushed as commands when a ribbit village's feelings may
//!   have changed, and can be pulled at any time.
//! - **Growth** (P17's recommendation): when a ribbit village's stocks and treasury stay at or
//!   above their targets for a while, it books one house (a PLACE_HOUSE command, placed by the
//!   bridge when the chunk next loads) and its population steps up at that moment, so the market
//!   stays piecewise linear: only its targets move.
//! - **Touring bands** (P16, Ben: all three options and player settlements): a band of ribbits
//!   with instruments tours ribbit villages, the human villages on the border (those with a ribbit
//!   neighbour) and the player settlements (P30's bases) whose owners are in good standing with
//!   its home. It stays a while at each stop to play, which raises the host's opinion of the
//!   band's home village, and carries news from stop to stop. Its whereabouts are a closed form of
//!   time ([`World::band_view`]); each stay is one event ([`World::band_plays`]), and the circuit
//!   is charted again at each stay at home, so a base marked or a village joined is on the next
//!   circuit (overnight choices PR8 to PR10).

use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_knowledge::{ActorId, BoardCursor};
use rtsim_settle::{Culture, NodeKind};
use rtsim_trade::line::Level;
use rtsim_travel::caravan::Body;

use crate::world::{Cause, Command, Ev, World};

/// Op ids derived from identity: 55 bits of a hash of `(identity, slot)`, never 0 (bits 55 to 63
/// clear: the top bit is the bridge's, bits 56 to 62 name an engine allocator, 0 for the core).
pub const SYS_OP: SystemId = SystemId::named("world.op");

/// An engine op id for the command whose identity is `(identity, slot)` (docs/boundary.md §2.2):
/// the same decision re-derived after a crash gets the same id, a different one never does.
pub fn identity_op(identity: u64, slot: u64) -> u64 {
    let h = StreamKey::new(identity, SYS_OP, slot).block(0, 0)[0] & ((1 << 55) - 1);
    if h == 0 { 1 } else { h }
}

/// Growth's RNG system: which template, which seed.
pub const SYS_GROWTH: SystemId = SystemId::named("ribbits.growth");
/// Bands' RNG system: their seeds.
pub const SYS_BAND: SystemId = SystemId::named("ribbits.band");

/// A village's feelings toward players, for the sorcerers' filter and the ribbit merchants'
/// prices (the SENTIMENT command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentimentSnapshot {
    /// The village's key (its structure start).
    pub village: u64,
    /// Players at or below this are left out of a sorcerer's buffs.
    pub dislike_at_or_below: i8,
    /// Players at or above this are friends (the bridge may favour them).
    pub friend_at_or_above: i8,
    pub at: Tick,
    /// Every player the village feels anything about: (UUID most, UUID least, sentiment).
    pub entries: Vec<(u64, u64, i8)>,
}

/// Book a house in a ribbit village (P17): the PLACE_HOUSE command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceHouse {
    /// The engine's operation id (top bit clear, O10).
    pub op: u64,
    pub village: u64,
    pub frame: u16,
    /// Which of the 14 templates under `data/ribbits/structure/houses`, from the seed.
    pub template: u32,
    /// Where to start searching for a free plot: the village's meeting point (the bridge searches
    /// rings around it, PLACE_HOUSE's SEARCH flag, with the height from the village).
    pub x: i32,
    pub z: i32,
    /// How far out to search, in blocks. PLACEHOLDER: 48.
    pub radius: u32,
    /// Quarter turns, from the seed.
    pub rotation: u8,
    pub seed: u64,
}

/// Spawn a resident for a placed house (the SPAWN_RESIDENT command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnResident {
    pub op: u64,
    pub village: u64,
    /// The PLACE_HOUSE op the resident is homed to.
    pub house: u64,
    pub body: Body,
}

/// A village's growth state (P17).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Growth {
    /// Since when its stocks and treasury have been at or above their targets.
    pub since: Option<Tick>,
    /// Bumped whenever `since` changes: a booked growth check with an older stamp is stale.
    pub stamp: u32,
    /// Houses booked so far.
    pub houses: u32,
}

/// A touring band (P16): a fixed circuit of ribbit villages from its home.
#[derive(Clone, Debug)]
pub struct Band {
    pub home: usize,
    pub seed: u64,
    /// Stops in order, starting at home; the tour goes round and round.
    pub route: Vec<usize>,
    /// Quanta walking from `route[i]` to the next stop.
    pub walks: Vec<u64>,
    /// Quanta it stays at each stop, playing.
    pub stay: u64,
    /// The quantum boundary its first stay at home began.
    pub start: u64,
    pub members: u8,
    /// Where each stop's noticeboard was last read for the band's packets.
    pub cursors: Vec<BoardCursor>,
    /// The packet waiting at each stop for the band's next departure: (departure boundary, id).
    pub pending: Vec<Option<(u64, u32)>>,
    /// Snapshots each stop received after this were relayed already.
    pub relayed: Vec<Tick>,
    /// Bumped whenever the circuit is charted again: a booked stay with an older stamp is stale.
    pub stamp: u32,
}

impl Band {
    /// Quanta for one whole circuit.
    pub fn period(&self) -> u64 {
        self.walks.iter().map(|w| w + self.stay).sum::<u64>().max(1)
    }

    /// Quanta from the start of a circuit to the start of the stay at stop `i`.
    fn offset(&self, i: usize) -> u64 {
        (0..i).map(|j| self.stay + self.walks[j]).sum()
    }

    /// The first boundary at or after `b` at which the band's stay at stop `i` begins.
    pub fn stay_start(&self, i: usize, b: u64) -> u64 {
        let p = self.period();
        let first = self.start + self.offset(i);
        if b <= first {
            return first;
        }
        first + (b - first).div_ceil(p) * p
    }

    /// The first boundary at or after `b` at which the band leaves stop `i`.
    pub fn next_departure(&self, i: usize, b: u64) -> u64 {
        let p = self.period();
        let first = self.start + self.offset(i) + self.stay;
        if b <= first {
            return first;
        }
        first + (b - first).div_ceil(p) * p
    }

    /// The bodies of the group: a master and its musicians (C25: one puppet group).
    pub fn bodies(&self) -> Vec<Body> {
        let mut v = vec![Body::RibbitMaster];
        v.extend(std::iter::repeat_n(
            Body::RibbitMusician,
            usize::from(self.members.saturating_sub(1)),
        ));
        v
    }
}

/// Where a band is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandView {
    pub band: usize,
    pub seed: u64,
    pub home: usize,
    pub bodies: Vec<Body>,
    pub frame: u16,
    pub x: i64,
    pub z: i64,
    /// Staying at this node (playing) until the given tick.
    pub staying: Option<(usize, Tick)>,
    /// Walking: (from, to, thousandths of the way).
    pub walking: Option<(usize, usize, u32)>,
}

impl World {
    // ---- Sorcerers ----

    /// The village's feelings toward players now, with the sorcerers' thresholds (P18
    /// PLACEHOLDERs), for the SENTIMENT command. `None` for a node that keeps no feelings.
    pub fn sentiment_snapshot(&self, node: usize) -> Option<SentimentSnapshot> {
        let n = self.nodes.get(node)?;
        let actor = n.actor;
        if !self.knowledge.is_holder(actor) {
            return None;
        }
        let entries = self
            .knowledge
            .sentiment_snapshot(actor, self.now)
            .into_iter()
            .filter(|(a, _)| self.knowledge.actors().is_player(*a))
            .map(|(a, s)| {
                let (most, least) = self.player_uuid_of(a);
                (most, least, s)
            })
            .collect();
        Some(SentimentSnapshot {
            village: n.key,
            dislike_at_or_below: self.cfg.sorcerer_dislike_at_or_below,
            friend_at_or_above: self.cfg.sorcerer_friend_at_or_above,
            at: self.now,
            entries,
        })
    }

    /// Whether a sorcerer of node `node`'s village buffs `player` (PLAN.md §8.8): everyone but a
    /// player the village dislikes, at or below the threshold.
    pub fn sorcerer_includes(&self, node: usize, player: ActorId) -> bool {
        let actor = self.nodes[node].actor;
        self.knowledge.sentiment(actor, player, self.now) > self.cfg.sorcerer_dislike_at_or_below
    }

    /// The sorcerer filter over a list of players near a sorcerer: who stays in.
    pub fn sorcerer_filter(&self, node: usize, players: &[ActorId]) -> Vec<bool> {
        players
            .iter()
            .map(|&p| self.sorcerer_includes(node, p))
            .collect()
    }

    /// A player's UUID halves; (0, hash) for one registered by hash only.
    pub fn player_uuid_of(&self, a: ActorId) -> (u64, u64) {
        self.players
            .get(&a)
            .map(|p| p.uuid)
            .unwrap_or((0, self.knowledge.actors().external(a).unwrap_or(0)))
    }

    /// A node's feelings may have changed: a ribbit village's snapshot goes out with the next
    /// commands.
    pub(crate) fn touch_sentiment(&mut self, node: usize) {
        if matches!(self.nodes[node].kind, NodeKind::Village(Culture::Ribbit)) {
            self.sentiment_dirty.insert(node);
        }
    }

    /// The same, for a knowledge holder that may be a node.
    pub(crate) fn touch_sentiment_actor(&mut self, actor: ActorId) {
        if let Some(i) = self.nodes.iter().position(|n| n.actor == actor) {
            self.touch_sentiment(i);
        }
    }

    pub(crate) fn flush_sentiments(&mut self) {
        for i in std::mem::take(&mut self.sentiment_dirty) {
            if let Some(s) = self.sentiment_snapshot(i) {
                self.outbox.push(Command::Sentiment(s));
            }
        }
    }

    // ---- Growth (P17) ----

    /// Whether a ribbit village's stocks of what its people need and its treasury are all at or
    /// above their targets' bands now (P17). A needed good the village has no way to get (none
    /// in store, none made, none coming) does not hold growth back: ribbits far from any iron
    /// still grow (overnight choice E56).
    pub fn prosperous(&self, node: usize) -> bool {
        let m = self.market(node);
        if !m.is_storehouse() {
            return false;
        }
        let goods_ok = (0..m.lines.len()).all(|gi| {
            let g = rtsim_trade::GoodId(gi as u16);
            let l = m.line(g);
            let out_of_reach = l.level() == Level::Empty && l.rates.cap <= 0 && l.rates.inflow <= 0;
            m.goods[gi].currency
                || m.goods[gi].household <= 0
                || l.level() >= Level::At
                || out_of_reach
        });
        let cash_ok = m.currency.is_some_and(|c| m.line(c).level() >= Level::At);
        goods_ok && cash_ok
    }

    /// After a market event at `node`: start or stop the growth clock (P17).
    pub(crate) fn growth_check(&mut self, node: usize, at: Tick) {
        if !matches!(self.nodes[node].kind, NodeKind::Village(Culture::Ribbit)) || !self.cfg.growth
        {
            return;
        }
        let ok = self.prosperous(node);
        let g = self.growth.entry(node).or_default();
        if g.houses >= self.cfg.growth_max_houses {
            return;
        }
        match (ok, g.since) {
            (true, None) => {
                g.since = Some(at);
                g.stamp += 1;
                let stamp = g.stamp;
                let when = at + self.cfg.growth_after_days * rtsim_kernel::clock::TICKS_PER_DAY;
                self.schedule(when, Ev::Grow { node, stamp });
            }
            (false, Some(_)) => {
                g.since = None;
                g.stamp += 1;
            }
            _ => {}
        }
    }

    /// The growth clock ran out: book a house if the village is still prosperous.
    pub(crate) fn grow(&mut self, node: usize, stamp: u32, at: Tick) {
        let Some(g) = self.growth.get(&node).copied() else {
            return;
        };
        if g.stamp != stamp || g.since.is_none() {
            return;
        }
        if !self.prosperous(node) {
            let g = self.growth.entry(node).or_default();
            g.since = None;
            g.stamp += 1;
            return;
        }
        let houses = g.houses + 1;
        let key = self.nodes[node].key;
        let seed = StreamKey::new(self.cfg.seed, SYS_GROWTH, key).block(u64::from(houses), 0)[0];
        // The op id is the house's identity (this village's nth house), never a counter: an engine
        // a crash rolled back books the same house under the same id, and the bridge answers it
        // DUPLICATE; a counter would hand that id to a different command (docs/boundary.md §2.2).
        let op = identity_op(seed, 0);
        let pos = self.nodes[node].pos;
        self.outbox.push(Command::PlaceHouse(PlaceHouse {
            op,
            village: key,
            frame: pos.frame,
            template: (seed % u64::from(self.cfg.house_templates.max(1))) as u32,
            x: pos.x,
            z: pos.z,
            radius: 48,
            rotation: ((seed >> 32) % 4) as u8,
            seed,
        }));
        self.houses_pending.insert(op, node);
        // The population steps up now (P17), so the market stays piecewise linear: only its
        // targets move.
        let pop = self.nodes[node].local.pop + self.cfg.growth_pop_per_house;
        self.set_population(node, pop, at);
        let g = self.growth.entry(node).or_default();
        g.houses = houses;
        g.since = None;
        g.stamp += 1;
        self.stats.houses += 1;
        self.market_event(node, at, Cause::Growth);
    }

    /// Sets a node's population: the settlement, the market's households and targets.
    pub fn set_population(&mut self, node: usize, pop: u32, at: Tick) {
        self.nodes[node].local.pop = pop;
        let sid = self.nodes[node].settlement;
        if let Some(s) = self.settlements.get_mut(sid) {
            s.pop = pop;
        }
        let l_mint = self.nodes[node]
            .kind
            .culture()
            .map_or(f64::INFINITY, |c| self.mint_cost[c.index()]);
        self.release(node, at);
        let region = self.nodes[node].region;
        let (goods, tun) = (&self.goods, &self.cfg.trade);
        self.markets.apply(region, at, |m, _| {
            rtsim_trade::build::repopulate(m, goods, tun, pop, l_mint)
        });
    }

    /// The bridge placed a booked house (HOUSE_PLACED): its residents may move in. Returns the
    /// SPAWN_RESIDENT commands it queued, one per resident the house brought (the population
    /// already counts them).
    pub fn house_placed(&mut self, op: u64) -> usize {
        let Some(node) = self.houses_pending.remove(&op) else {
            return 0;
        };
        let village = self.nodes[node].key;
        for k in 0..self.cfg.growth_pop_per_house {
            // The resident's identity: its house and its place in it.
            let rop = identity_op(op, u64::from(k) + 1);
            self.outbox.push(Command::SpawnResident(SpawnResident {
                op: rop,
                village,
                house: op,
                body: Body::RibbitResident,
            }));
        }
        self.cfg.growth_pop_per_house as usize
    }

    /// A fresh engine operation id (top bit clear, O10), deterministic. Commands the bridge
    /// answers by op id take [`identity_op`] instead (docs/boundary.md §2.2).
    #[allow(dead_code)]
    pub(crate) fn next_op(&mut self) -> u64 {
        self.op_counter += 1;
        self.op_counter & !(1 << 63)
    }

    // ---- Bands (P16) ----

    /// Gives `home` a touring band if it is a ribbit village big enough with somewhere to tour
    /// (P16): the circuit visits the nearest stops a band may visit ([`World::band_may_visit`]),
    /// up to the configured number, then comes home.
    pub(crate) fn maybe_band(&mut self, home: usize, at: Tick) {
        if !matches!(self.nodes[home].kind, NodeKind::Village(Culture::Ribbit))
            || self.nodes[home].local.pop < self.cfg.band_min_pop
            || self.bands.iter().any(|b| b.home == home)
        {
            return;
        }
        let route = self.band_route(home, at);
        if route.len() < 2 {
            return;
        }
        let q = self.cfg.trade.quantum;
        let walks = self.band_walks(&route);
        let key = self.nodes[home].key;
        let seed = StreamKey::new(self.cfg.seed, SYS_BAND, key).block(0, 0)[0];
        let n = route.len();
        let start = q.index(at) + 1;
        self.bands.push(Band {
            home,
            seed,
            route,
            walks,
            stay: self.cfg.band_stay_quanta.max(1),
            start,
            members: self.cfg.band_members.max(1),
            cursors: vec![BoardCursor::default(); n],
            pending: vec![None; n],
            relayed: vec![at; n],
            stamp: 0,
        });
        let b = self.bands.len() - 1;
        self.index_band(b);
        self.book_stay(b, 0, start);
    }

    /// Whether a band from `home` may stop at node `j` now (P16): a ribbit village; a human
    /// village on the border (one with a ribbit neighbour); or a player settlement (an active
    /// base, P30) whose owners are all in good standing with the band's home (sentiment at or
    /// above `band_standing_at_or_above`). Havens never.
    pub fn band_may_visit(&self, home: usize, j: usize, at: Tick) -> bool {
        let ribbit = |k: usize| matches!(self.nodes[k].kind, NodeKind::Village(Culture::Ribbit));
        match self.nodes[j].kind {
            NodeKind::Village(Culture::Ribbit) => true,
            NodeKind::Village(Culture::Human) => {
                self.nodes[j].neighbours.iter().any(|&k| ribbit(k))
            }
            NodeKind::PlayerBase { .. } => {
                let holder = self.nodes[home].actor;
                self.bases.get(&j).is_some_and(|st| {
                    !st.players.is_empty()
                        && st.players.iter().all(|&p| {
                            self.knowledge.sentiment(holder, p, at)
                                >= self.cfg.band_standing_at_or_above
                        })
                })
            }
            NodeKind::Haven(_) => false,
        }
    }

    /// The circuit a band from `home` would tour now: from each stop, the nearest neighbour it
    /// may visit and has not visited yet.
    fn band_route(&self, home: usize, at: Tick) -> Vec<usize> {
        let mut route = vec![home];
        while route.len() <= self.cfg.band_stops {
            let here = *route.last().expect("home");
            let next = self.nodes[here]
                .neighbours
                .iter()
                .copied()
                .filter(|&j| !route.contains(&j) && self.band_may_visit(home, j, at))
                .min_by_key(|&j| {
                    (
                        self.pos_dist(self.nodes[here].pos, self.nodes[j].pos),
                        self.nodes[j].key,
                    )
                });
            match next {
                Some(j) => route.push(j),
                None => break,
            }
        }
        route
    }

    /// Quanta walking from each stop of `route` to the next (the last back home).
    fn band_walks(&self, route: &[usize]) -> Vec<u64> {
        let q = self.cfg.trade.quantum;
        let speed = self.cfg.caravan_blocks_per_tick;
        (0..route.len())
            .map(|i| {
                let (a, b) = (route[i], route[(i + 1) % route.len()]);
                let dist = self
                    .pos_dist(self.nodes[a].pos, self.nodes[b].pos)
                    .unwrap_or(0);
                q.covering((dist as f64 / speed).ceil() as u64)
            })
            .collect()
    }

    /// Lists band `b`'s stops on their nodes (the news a stop sends rides the band).
    fn index_band(&mut self, b: usize) {
        for i in 0..self.bands[b].route.len() {
            let node = self.bands[b].route[i];
            self.nodes[node].bands.push((b as u32, i as u32));
        }
    }

    fn unindex_band(&mut self, b: usize) {
        for i in 0..self.bands[b].route.len() {
            let node = self.bands[b].route[i];
            self.nodes[node].bands.retain(|&(x, _)| x as usize != b);
        }
    }

    /// Books band `b`'s next stay at stop `i`, at or after boundary `from`.
    fn book_stay(&mut self, b: usize, i: usize, from: u64) {
        let band = &self.bands[b];
        let at = self.cfg.trade.quantum.at(band.stay_start(i, from));
        let stamp = band.stamp;
        self.schedule(
            at,
            Ev::BandPlays {
                band: b as u32,
                stop: i as u32,
                stamp,
            },
        );
    }

    /// Band `b` begins its stay at stop `stop` (P16). Away from home it plays, and the host thinks
    /// better of the band's home village (`band_performance_delta`, a direct change of the
    /// host's feeling, not a report: it is not news). A player settlement whose owners fell out
    /// of good standing since the circuit was charted gets no performance. At home the next
    /// circuit is charted from who may be visited now; a changed circuit starts here.
    pub(crate) fn band_plays(&mut self, b: usize, stop: usize, stamp: u32, at: Tick) {
        let Some(band) = self.bands.get(b) else {
            return;
        };
        if band.stamp != stamp || stop >= band.route.len() {
            return;
        }
        let nb = self.cfg.trade.quantum.index(at);
        let (home, host) = (band.home, band.route[stop]);
        if stop == 0 {
            let route = self.band_route(home, at);
            // A circuit that would shrink to home alone keeps its old stops: the band walks on,
            // and plays only where it still may.
            if route.len() >= 2 && route != self.bands[b].route {
                self.rechart_band(b, route, nb, at);
            }
        } else if self.band_may_visit(home, host, at) {
            let (h, a) = (self.nodes[host].actor, self.nodes[home].actor);
            self.knowledge
                .adjust(h, a, self.cfg.band_performance_delta, at);
            self.stats.band_plays += 1;
        }
        // After a new chart this is stop 1 of the new circuit.
        let next = (stop + 1) % self.bands[b].route.len();
        self.book_stay(b, next, nb);
    }

    /// Band `b` tours `route` from now on, its first stay at home beginning at boundary `nb`.
    /// What each stop that stays on the circuit knew of the band (its board cursor, the packet
    /// waiting for the band, what was relayed) carries over.
    fn rechart_band(&mut self, b: usize, route: Vec<usize>, nb: u64, at: Tick) {
        self.unindex_band(b);
        let walks = self.band_walks(&route);
        let band = &mut self.bands[b];
        let mut cursors = Vec::with_capacity(route.len());
        let mut pending = Vec::with_capacity(route.len());
        let mut relayed = Vec::with_capacity(route.len());
        for node in &route {
            match band.route.iter().position(|x| x == node) {
                Some(i) => {
                    cursors.push(band.cursors[i]);
                    pending.push(band.pending[i]);
                    relayed.push(band.relayed[i]);
                }
                None => {
                    cursors.push(BoardCursor::default());
                    pending.push(None);
                    relayed.push(at);
                }
            }
        }
        band.route = route;
        band.walks = walks;
        band.cursors = cursors;
        band.pending = pending;
        band.relayed = relayed;
        band.start = nb;
        band.stamp = band.stamp.wrapping_add(1);
        self.index_band(b);
    }

    /// Where band `b` is at `at`: a closed form of its circuit.
    pub fn band_view(&self, b: usize, at: Tick) -> BandView {
        let band = &self.bands[b];
        let ql = self.cfg.trade.quantum.len();
        let p_ticks = band.period() * ql;
        let start = band.start * ql;
        let t = at.0.max(start);
        let mut phi = (t - start) % p_ticks;
        let n = band.route.len();
        let pos = |i: usize| self.nodes[band.route[i % n]].pos;
        let mut view = BandView {
            band: b,
            seed: band.seed,
            home: band.home,
            bodies: band.bodies(),
            frame: pos(0).frame,
            x: i64::from(pos(0).x),
            z: i64::from(pos(0).z),
            staying: None,
            walking: None,
        };
        for i in 0..n {
            let stay = band.stay * ql;
            if phi < stay {
                let p = pos(i);
                view.x = i64::from(p.x);
                view.z = i64::from(p.z);
                view.staying = Some((band.route[i], Tick(at.0 + (stay - phi))));
                return view;
            }
            phi -= stay;
            let walk = band.walks[i] * ql;
            if phi < walk {
                let (a, c) = (pos(i), pos(i + 1));
                let f = phi as i64 * 1000 / walk.max(1) as i64;
                // On the road the short way, canonical (TP21).
                (view.x, view.z) = self.road_point(a, c, f);
                view.walking = Some((band.route[i], band.route[(i + 1) % n], f as u32));
                return view;
            }
            phi -= walk;
        }
        view
    }

    /// The bands staying at `node` now: the bridge gathers them around their master and plays
    /// (`RibbitPlayMusicGoal`).
    pub fn bands_at(&self, node: usize) -> Vec<BandView> {
        (0..self.bands.len())
            .map(|b| self.band_view(b, self.now))
            .filter(|v| v.staying.is_some_and(|s| s.0 == node))
            .collect()
    }
}
