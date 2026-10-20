// SPDX-License-Identifier: AGPL-3.0-or-later
//! Caravans near players: sampling travellers from flows, making one an individual when something
//! happens to it, and what happens to it after (PLAN.md §8.6, §8.7, §8.9 D, E, I; C21, C24, C25).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::{ActorId, ActorKind, HolderKind, Place, Report, ReportKind, SEVERITY_UNIT};
use rtsim_knowledge::{Temperament, Trait, witnesses};
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::{Amount, GoodId, PRICE_ONE};
use rtsim_travel::caravan::{Caravan, CaravanKey, GroupKind, Leg, Stage, Traveller, load};
use rtsim_travel::legs::{Owner, Path};

use crate::world::{CaravanId, Ev, World};

/// The result of an attack on an individual caravan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attack {
    /// What the player took, thousandths.
    pub taken: BTreeMap<GoodId, Amount>,
    pub outcome: CaravanOutcome,
    /// The report the caravan (and any settlement in sight) now carries.
    pub report: rtsim_knowledge::ReportId,
}

/// What happened to an individual caravan, for the caller's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaravanOutcome {
    /// It went on to where it was going.
    Continued,
    /// It turned back home.
    TurnedBack,
    /// Nobody is left to lead it: the group broke up.
    Dissolved,
}

impl World {
    /// The travellers on the road in one direction of one edge now (SCALE.md §3): derived from
    /// the flow, never stored.
    pub fn travellers(&self, edge: u32, dir: u8) -> Vec<Traveller> {
        let q = self.cfg.trade.quantum;
        let b = q.index(self.now);
        let e = &self.edges[edge as usize];
        let d = &e.dirs[dir as usize];
        let lo = (b + 1).saturating_sub(e.d);
        let group = GroupKind::of(self.nodes[d.from].kind.culture());
        d.lattice
            .departures(lo, b)
            .into_iter()
            .map(|dep| {
                let key = CaravanKey {
                    edge,
                    dir,
                    departure: dep,
                };
                let (w0, _) = d.lattice.window(dep);
                let from = w0.max(lo);
                let mut cargo = BTreeMap::new();
                for (g, p) in d.goods.iter().chain(d.pay.iter()) {
                    let a = p.departed_between(from, dep);
                    if a > 0 {
                        *cargo.entry(*g).or_default() += a as Amount;
                    }
                }
                let departed = q.at(dep);
                let arrives = q.at(dep + e.d);
                let span = (arrives.0 - departed.0).max(1);
                Traveller {
                    key,
                    seed: key.seed(self.cfg.seed, e.seed),
                    from: d.from,
                    to: d.to,
                    group,
                    cargo,
                    progress_milli: ((self.now.0.saturating_sub(departed.0)).min(span) * 1000
                        / span) as u32,
                    departed,
                    arrives,
                }
            })
            .collect()
    }

    /// Registers a player as an actor (idempotent).
    pub fn player(&mut self, uuid_hash: u64) -> ActorId {
        self.knowledge
            .actors_mut()
            .register(ActorKind::Player, uuid_hash)
    }

    fn place_of(&self, node: usize) -> Place {
        let p = self.nodes[node].pos;
        Place::new(p.frame, p.x, 64, p.z)
    }

    /// Where an individual caravan is now, on its leg.
    pub fn caravan_place(&self, c: &Caravan, at: Tick) -> Place {
        let (a, b) = (self.nodes[c.leg.from].pos, self.nodes[c.leg.to].pos);
        let f = i64::from(c.progress_milli(at));
        let lerp =
            |x: i32, y: i32| (i64::from(x) + (i64::from(y) - i64::from(x)) * f / 1000) as i32;
        Place::new(a.frame, lerp(a.x, b.x), 64, lerp(a.z, b.z))
    }

    fn path_of(&self, leg: &Leg) -> Path {
        let (a, b) = (self.nodes[leg.from].pos, self.nodes[leg.to].pos);
        Path {
            frame: a.frame,
            x0: a.x,
            z0: a.z,
            x1: b.x,
            z1: b.z,
            t0: leg.start,
            t1: leg.end,
            radius: self.cfg.knowledge.witness_radius,
        }
    }

    /// Makes the traveller `key` an individual row (SCALE.md §3): its consignments still on the
    /// road are cut out of the flow and become its cargo, so totals stay exact; its destination is
    /// woken where its arrivals change. `None` if no such caravan is on the road now.
    pub fn promote(&mut self, key: CaravanKey) -> Option<CaravanId> {
        let at = self.now;
        let q = self.cfg.trade.quantum;
        let b = q.index(at);
        let e = self.edges.get(key.edge as usize)?;
        let ed = e.d;
        let edge_seed = e.seed;
        let d = &e.dirs[key.dir as usize];
        let dep = key.departure;
        if !(dep <= b && b < dep + ed && d.lattice.is_departure(dep)) {
            return None;
        }
        let (from, to) = (d.from, d.to);
        let (w0, _) = d.lattice.window(dep);
        let lo = w0.max((b + 1).saturating_sub(ed));
        let dir = &mut self.edges[key.edge as usize].dirs[key.dir as usize];
        let mut cargo: BTreeMap<GoodId, Amount> = BTreeMap::new();
        let mut quotes = BTreeMap::new();
        for (g, p) in dir.goods.iter_mut().chain(dir.pay.iter_mut()) {
            let a = p.cut(lo, dep);
            if a > 0 {
                *cargo.entry(*g).or_default() += a;
                quotes.insert(*g, p.quote_at(dep));
            }
        }
        // The destination's arrivals change where the hole starts and ends. The start can be the
        // very next boundary, in which case the destination must re-read its inflows now.
        for first in [lo, dep + 1] {
            let wake = q.at(first + ed - 1).max(at);
            self.schedule(wake, Ev::Wake(to));
        }
        self.stats.promotions += 1;
        let seed = key.seed(self.cfg.seed, edge_seed);
        let group = GroupKind::of(self.nodes[from].kind.culture());
        let carriers = group
            .bodies()
            .iter()
            .filter(|b| GroupKind::carries(**b))
            .count()
            .max(1) as u32;
        let members = load(
            group,
            &cargo,
            &self.goods,
            self.cfg.trade.caravan_slots / carriers,
        );
        let holder = self.knowledge.actors_mut().register(ActorKind::Group, seed);
        self.knowledge.add_holder(holder, HolderKind::Caravan);
        let leg = Leg {
            from,
            to,
            start: q.at(dep),
            end: q.at(dep + ed),
            generation: 0,
        };
        let c = Caravan {
            key,
            seed,
            home: from,
            group,
            members,
            stage: Stage::Outbound,
            leg,
            holder,
            notable: None,
            quotes,
        };
        let path = self.path_of(&leg);
        let id = self.caravans.insert(c);
        let stamp = self.legs.book(path, Owner(id.to_bits()));
        let c = self.caravans.get_mut(id).expect("just inserted");
        c.leg.generation = stamp.generation;
        self.leg_ids.insert(id, stamp.leg);
        self.schedule(
            leg.end,
            Ev::Arrive {
                caravan: id,
                generation: stamp.generation,
            },
        );
        Some(id)
    }

    fn report(
        &mut self,
        who: ActorId,
        c: CaravanId,
        kind: ReportKind,
        at: Tick,
    ) -> Option<rtsim_knowledge::ReportId> {
        let car = self.caravans.get(c)?;
        let place = self.caravan_place(car, at);
        let whom = self.nodes[car.home].actor;
        let holder = car.holder;
        let id = self.knowledge.record(Report {
            who,
            whom,
            kind,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place,
        });
        // The caravan saw it; so does any settlement close enough (PLAN.md §5.6: a settlement's
        // awareness of events in it).
        self.knowledge.witness(holder, id, at);
        let near: Vec<(ActorId, Place)> = (0..self.nodes.len())
            .map(|i| (self.nodes[i].actor, self.place_of(i)))
            .collect();
        let seen = witnesses(place, self.cfg.knowledge.witness_radius, near);
        for h in seen {
            self.knowledge.witness(h, id, at);
        }
        // The trader saw it too, as a notable: it remembers, and feels it, so its grudge can
        // pass to its heir (C24).
        let notable = self.trader_notable(c, at);
        self.knowledge.witness(notable, id, at);
        self.knowledge.remember(notable, id, SEVERITY_UNIT);
        self.flush_gossip();
        Some(id)
    }

    /// The caravan's trader as a notable (PLAN.md §4.4: a member becomes notable when something
    /// makes it matter), its heir its home market (C24).
    fn trader_notable(&mut self, c: CaravanId, at: Tick) -> ActorId {
        if let Some(n) = self.caravans.get(c).and_then(|x| x.notable) {
            return n;
        }
        let (seed, home) = {
            let car = self.caravans.get(c).expect("live caravan");
            (car.seed, self.nodes[car.home].actor)
        };
        let life = self.cfg.trader_lifespan_days * rtsim_kernel::clock::TICKS_PER_DAY;
        let n = self.knowledge.create_notable(seed, home, home, at, life);
        self.caravans.get_mut(c).expect("live").notable = Some(n);
        n
    }

    /// A player robs an individual caravan: takes `share_milli` thousandths of everything it
    /// carries (PLAN.md §8.9 D). The loot leaves the world to the player. The trader then goes on
    /// or turns back home, as its temperament decides (C25).
    pub fn rob(&mut self, c: CaravanId, player: ActorId, share_milli: i64) -> Option<Attack> {
        let at = self.now;
        let cargo = self.caravans.get(c)?.cargo();
        let mut taken = BTreeMap::new();
        for (g, a) in cargo {
            let t = mul_div_floor(a, share_milli.clamp(0, 1000), 1000);
            let got = self.caravans.get_mut(c)?.take(g, t);
            if got > 0 {
                taken.insert(g, got);
                *self.outside.player_out.entry(g).or_default() += i128::from(got);
            }
        }
        let report = self.report(player, c, ReportKind::Robbery, at)?;
        let outcome = self.decide_after_attack(c, at);
        Some(Attack {
            taken,
            outcome,
            report,
        })
    }

    /// A player kills mobs attacking a caravan (C24: rescues count). The caravan carries the
    /// rescue home as a positive report.
    pub fn rescue(&mut self, c: CaravanId, player: ActorId) -> Option<rtsim_knowledge::ReportId> {
        let at = self.now;
        self.report(player, c, ReportKind::Rescue, at)
    }

    /// A player leashes or picks up a member (A10): it leaves the sim with what it carries, and
    /// the caravan reports a theft. The rest of the group carries on as one unit (C25).
    pub fn take_member(&mut self, c: CaravanId, member: usize, player: ActorId) -> Option<Attack> {
        let at = self.now;
        let car = self.caravans.get_mut(c)?;
        let m = car.members.get_mut(member)?;
        if !m.present {
            return None;
        }
        m.present = false;
        let mut taken = BTreeMap::new();
        for (g, a) in std::mem::take(&mut m.cargo) {
            *self.outside.player_out.entry(g).or_default() += i128::from(a);
            taken.insert(g, a);
        }
        let report = self.report(player, c, ReportKind::Theft, at)?;
        Some(Attack {
            taken,
            outcome: self.decide_after_attack(c, at),
            report,
        })
    }

    /// A player kills a member. Its cargo drops (C21) and leaves the sim. If it was the leader
    /// the trader's grudge passes to its home market (C24: heirs inherit grudges), and the group
    /// breaks up.
    pub fn kill_member(&mut self, c: CaravanId, member: usize, player: ActorId) -> Option<Attack> {
        let at = self.now;
        let car = self.caravans.get_mut(c)?;
        let m = car.members.get_mut(member)?;
        if !m.alive || !m.present {
            return None;
        }
        m.alive = false;
        m.present = false;
        let leader = matches!(
            m.body,
            rtsim_travel::Body::Trader | rtsim_travel::Body::RibbitMerchant
        );
        let mut taken = BTreeMap::new();
        for (g, a) in std::mem::take(&mut m.cargo) {
            *self.outside.player_out.entry(g).or_default() += i128::from(a);
            taken.insert(g, a);
        }
        let report = self.report(player, c, ReportKind::Killing, at)?;
        if leader {
            if let Some(n) = self.caravans.get(c).and_then(|x| x.notable) {
                self.knowledge.die(n, at);
            }
            self.flush_gossip();
        }
        Some(Attack {
            taken,
            outcome: self.decide_after_attack(c, at),
            report,
        })
    }

    /// After an attack: with no leader the group breaks up (what is left leaves the sim with the
    /// animals); otherwise a bold trader goes on and a timid one turns back home (C25: the
    /// temperament derived from its seed weighs the choice; the threshold is the middle of the
    /// trait's range).
    fn decide_after_attack(&mut self, c: CaravanId, at: Tick) -> CaravanOutcome {
        let Some(car) = self.caravans.get(c) else {
            return CaravanOutcome::Dissolved;
        };
        if !car.has_leader() {
            self.dissolve(c, false);
            return CaravanOutcome::Dissolved;
        }
        if car.stage == Stage::Returning {
            return CaravanOutcome::Continued;
        }
        let bold = Temperament::from_seed(car.seed).get(Trait::Bold);
        if bold >= 128 {
            return CaravanOutcome::Continued;
        }
        // Turn back: walk home from here, as long as it has walked so far.
        let walked = at.0.saturating_sub(car.leg.start.0);
        let home = car.home;
        let here = car.leg.to;
        let leg = Leg {
            from: here,
            to: home,
            start: at,
            end: at + walked.max(1),
            generation: 0,
        };
        self.replan(c, leg, Stage::Returning);
        CaravanOutcome::TurnedBack
    }

    fn replan(&mut self, c: CaravanId, mut leg: Leg, stage: Stage) {
        let mut path = self.path_of(&leg);
        if stage == Stage::Returning && leg.start.0 > 0 {
            // The return starts where the caravan is, not at the far market.
            let car = self.caravans.get(c).expect("live");
            let p = self.caravan_place(car, leg.start);
            path.x0 = p.x;
            path.z0 = p.z;
        }
        let stamp = match self.leg_ids.get(&c) {
            Some(&id) => self.legs.replan(id, path),
            None => None,
        }
        .unwrap_or_else(|| self.legs.book(path, Owner(c.to_bits())));
        self.leg_ids.insert(c, stamp.leg);
        leg.generation = stamp.generation;
        let car = self.caravans.get_mut(c).expect("live");
        car.leg = leg;
        car.stage = stage;
        self.schedule(
            leg.end,
            Ev::Arrive {
                caravan: c,
                generation: stamp.generation,
            },
        );
    }

    /// Removes a caravan row. `home` when it reached home (its cargo was deposited); otherwise
    /// what it still carries leaves the sim with its animals.
    fn dissolve(&mut self, c: CaravanId, home: bool) {
        let Some(car) = self.caravans.remove(c) else {
            return;
        };
        if !home {
            for (g, a) in car.cargo() {
                *self.outside.player_out.entry(g).or_default() += i128::from(a);
            }
        }
        if let Some(id) = self.leg_ids.remove(&c) {
            self.legs.cancel(id);
        }
        self.knowledge.remove_holder(car.holder);
    }

    /// An individual caravan reaches the end of a leg.
    pub(crate) fn caravan_arrives(&mut self, c: CaravanId, generation: u32, at: Tick) {
        let Some(car) = self.caravans.get(c) else {
            return;
        };
        if car.leg.generation != generation {
            return;
        }
        self.stats.caravan_arrivals += 1;
        let node = car.leg.to;
        let stage = car.stage;
        let home = car.home;
        let holder = car.holder;
        // What it saw, it tells (gossip moves with travellers).
        let mut cursor = rtsim_knowledge::BoardCursor::default();
        let packet = self.knowledge.packet_since(holder, &mut cursor);
        let learned = self.knowledge.deliver(self.nodes[node].actor, &packet, at);
        self.flush_gossip();
        let car = self.caravans.get(c).expect("checked").clone();
        let region = self.nodes[node].region;
        let mut sold: BTreeMap<GoodId, Amount> = BTreeMap::new();
        let mut paid: BTreeMap<GoodId, Amount> = BTreeMap::new();
        let goods = &self.goods;
        self.markets.apply(region, at, |m, _| {
            let cargo = car.cargo();
            for (&g, &a) in &cargo {
                if a <= 0 {
                    continue;
                }
                let currency = goods.is_currency(g);
                let deposit_all = stage == Stage::Returning || node == home || currency;
                if deposit_all || !m.is_storehouse() {
                    if m.is_storehouse() {
                        let l = m.line_mut(g);
                        l.stock += a;
                        l.book.lump_in += i128::from(a);
                        sold.insert(g, a);
                    }
                    continue;
                }
                // Sell at the quote it left with, as far as the treasury pays (C14).
                let quote = car.quotes.get(&g).copied().unwrap_or(0).max(0);
                let Some(cur) = m.currency else { continue };
                let treasury = m.stock(cur);
                let value = mul_div_floor(a, quote, PRICE_ONE);
                let sell = if value <= treasury || value == 0 {
                    a
                } else {
                    mul_div_floor(a, treasury, value)
                };
                let pay = mul_div_floor(sell, quote, PRICE_ONE).min(treasury);
                if sell > 0 {
                    let l = m.line_mut(g);
                    l.stock += sell;
                    l.book.lump_in += i128::from(sell);
                    sold.insert(g, sell);
                }
                if pay > 0 {
                    let t = m.line_mut(cur);
                    t.stock -= pay;
                    t.book.lump_out += i128::from(pay);
                    *paid.entry(cur).or_default() += pay;
                }
            }
        });
        {
            let car = self.caravans.get_mut(c).expect("live");
            for (g, a) in &sold {
                car.take(*g, *a);
            }
            for (g, a) in &paid {
                car.give(*g, *a);
            }
        }
        let done = stage == Stage::Returning || node == home;
        if done {
            self.dissolve(c, true);
        } else {
            let walk = car.leg.end.0.saturating_sub(car.leg.start.0).max(1);
            let leg = Leg {
                from: node,
                to: home,
                start: at,
                end: at + walk,
                generation: 0,
            };
            self.replan(c, leg, Stage::Returning);
        }
        self.market_event(node, at, false);
        if learned > 0 {
            self.emit(node, at, false);
        }
    }
}
