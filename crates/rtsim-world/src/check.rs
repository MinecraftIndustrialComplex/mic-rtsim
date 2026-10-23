// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conservation and state hashes (PLAN.md §8.4 "Conservation", §4.7).
//!
//! Per good and per currency: everything in stores, everything on the road (flows, raids on their
//! way to havens) and everything individual caravans carry equals genesis + produced − consumed +
//! player in − player out − destroyed. Lump transfers between caravans and stores (a caravan
//! selling its cargo) and cuts from flows into caravans are internal: they move a quantity
//! between two holders the sum already counts.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{Encode, Writer};
use rtsim_kernel::conserve::{Imbalance, Ledger, QuantityKind, Totals};
use rtsim_kernel::rng::fnv1a64;
use rtsim_trade::GoodId;

use crate::world::World;

fn kind(g: GoodId) -> QuantityKind {
    QuantityKind(u32::from(g.0))
}

impl World {
    /// What the world holds at `at`, per good, and what its books say it should.
    pub fn conservation(&self, at: Tick) -> (Totals, Ledger) {
        let mut seen = Totals::new();
        let mut book = Ledger::new();
        for i in 0..self.nodes.len() {
            let m = self.market_at(i, at);
            let ws = self.works.get(&i);
            for (gi, l) in m.lines.iter().enumerate() {
                let g = GoodId(gi as u16);
                let k = kind(g);
                seen.add(k, i128::from(l.stock));
                let b = l.book;
                let w = i128::from;
                book.mint(k, w(b.genesis) + w(b.produced) + w(b.player_in));
                book.burn(k, w(b.consumed) + w(b.player_out) + w(b.destroyed));
                if m.goods[gi].currency {
                    // Money above the ceiling is in the works purse (a holder) until it is spent
                    // on works or paid as tribute (burns), SINK.md §4.5.
                    let (spent, tribute, granted) =
                        ws.map_or((0, 0, 0), |s| (s.spent(g), s.tribute(g), s.granted(g)));
                    seen.add(k, w(b.sunk) + w(granted) - w(spent) - w(tribute));
                    book.mint(k, w(granted));
                    book.burn(k, w(spent) + w(tribute));
                } else {
                    book.burn(k, w(b.sunk));
                }
            }
        }
        let b = self.cfg.trade.quantum.index(at);
        let mut transit: BTreeMap<GoodId, i128> = BTreeMap::new();
        for e in &self.edges {
            for d in &e.dirs {
                d.in_transit(b, &mut transit);
            }
        }
        for (g, a) in transit {
            seen.add(kind(g), a);
        }
        for (_, c) in self.caravans.iter() {
            for (g, a) in c.holdings() {
                seen.add(kind(g), i128::from(a));
            }
        }
        for (g, a) in &self.outside.player_in {
            book.mint(kind(*g), *a);
        }
        for (g, a) in &self.outside.player_out {
            book.burn(kind(*g), *a);
        }
        for (g, a) in &self.outside.destroyed {
            book.burn(kind(*g), *a);
        }
        for (g, a) in &self.outside.held {
            seen.add(kind(*g), *a);
        }
        (seen, book)
    }

    /// Debugging aid (needs `keep_history`): for every market and good, what its books say
    /// departed and arrived against what its pipelines say, up to `at`. Returns the mismatches as
    /// (node, good, booked departed, piped departed, booked arrived, piped arrived).
    pub fn audit(&self, at: Tick) -> Vec<(usize, GoodId, i128, i128, i128, i128)> {
        let b = self.cfg.trade.quantum.index(at);
        let mut out = Vec::new();
        for i in 0..self.nodes.len() {
            let m = self.market_at(i, at);
            let mut dep: BTreeMap<GoodId, i128> = BTreeMap::new();
            let mut arr: BTreeMap<GoodId, i128> = BTreeMap::new();
            for d in &self.nodes[i].out {
                let e = &self.edges[d.edge as usize];
                let o = &e.dirs[d.dir as usize];
                for (g, p) in o.goods.iter().chain(o.pay.iter()) {
                    *dep.entry(*g).or_default() +=
                        (0..=b).map(|q| i128::from(p.rate_at(q))).sum::<i128>();
                }
                for (g, p) in &o.raid {
                    *dep.entry(*g).or_default() +=
                        (0..=b).map(|q| i128::from(p.rate_at(q))).sum::<i128>();
                }
                let n = &e.dirs[1 - d.dir as usize];
                for (g, p) in n.goods.iter().chain(n.pay.iter()) {
                    *arr.entry(*g).or_default() +=
                        (0..=b).map(|q| i128::from(p.arrival_at(q))).sum::<i128>();
                }
            }
            for r in &self.nodes[i].raids_in {
                for (g, p) in &self.edges[r.edge as usize].dirs[r.dir as usize].raid {
                    *arr.entry(*g).or_default() +=
                        (0..=b).map(|q| i128::from(p.arrival_at(q))).sum::<i128>();
                }
            }
            for (gi, l) in m.lines.iter().enumerate() {
                let g = GoodId(gi as u16);
                let (pd, pa) = (
                    dep.get(&g).copied().unwrap_or(0),
                    arr.get(&g).copied().unwrap_or(0),
                );
                let (bd, ba) = (i128::from(l.book.departed), i128::from(l.book.arrived));
                if pd != bd || pa != ba {
                    out.push((i, g, bd, pd, ba, pa));
                }
            }
        }
        out
    }

    pub fn check_conserved(&self, at: Tick) -> Result<(), Imbalance> {
        let (seen, book) = self.conservation(at);
        book.check(&seen)?;
        // No purse ever pays more than it holds.
        for (&i, s) in &self.works {
            let m = self.market_at(i, at);
            for &(g, _) in s.spent.iter().chain(&s.tribute) {
                let Some(l) = m.lines.get(g.index()) else {
                    continue;
                };
                let purse = s.purse(g, l);
                if purse < 0 {
                    return Err(Imbalance {
                        kind: kind(g),
                        expected: 0,
                        observed: i128::from(purse),
                    });
                }
            }
        }
        Ok(())
    }

    /// A closed lane offers no new goods (choice PC22): once `close_lane` has returned, every
    /// closed edge's goods and raid pipelines run at rate 0 in both directions, whatever the
    /// news says, so what is on the road is all that is left to arrive. Cheap: one pass over the
    /// edges. Not part of the per-event check, because between the two ends' events of a closure
    /// the far end still sends.
    pub fn check_lanes(&self) -> Result<(), String> {
        for (i, e) in self.edges.iter().enumerate() {
            if !e.closed {
                continue;
            }
            if e.medium == rtsim_travel::Medium::Road {
                return Err(format!("edge {i} is a closed road"));
            }
            for (k, d) in e.dirs.iter().enumerate() {
                for (g, p) in d.goods.iter().chain(d.raid.iter()) {
                    if p.current_rate() != 0 {
                        return Err(format!(
                            "closed edge {i} direction {k} still sends good {} at {}",
                            g.0,
                            p.current_rate()
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn assert_conserved(&self, at: Tick) {
        if let Err(e) = self.check_conserved(at) {
            panic!("{e} at {at:?} ({:?})", self.stats);
        }
    }

    /// A hash of the whole state at the world's time, independent of who read what when: markets
    /// are hashed as projected to now, not as last stored.
    pub fn state_hash(&self) -> u64 {
        let mut w = Writer::new();
        let now = self.now;
        w.u64(now.0);
        for i in 0..self.nodes.len() {
            self.market_at(i, now).encode(&mut w);
            let n = &self.nodes[i];
            w.u64(n.news.len() as u64);
            for (src, s) in n.news.iter() {
                w.u64(*src as u64);
                w.u64(s.at.0);
            }
        }
        for e in &self.edges {
            w.bool(e.closed);
            for d in &e.dirs {
                w.u64(d.lattice.current_period());
                for (g, p) in d.pipelines() {
                    w.u16(g.0);
                    for s in p.segs() {
                        w.u64(s.start);
                        w.i64(s.rate);
                        w.i64(s.quote);
                    }
                    for h in p.holes() {
                        w.u64(h.from);
                        w.u64(h.to);
                    }
                }
            }
        }
        for (h, c) in self.caravans.iter() {
            w.u64(h.to_bits());
            w.u64(c.seed);
            w.u64(c.leg.end.0);
            w.u32(c.leg.generation);
            for (g, a) in c.holdings() {
                w.u16(g.0);
                w.i64(a);
            }
        }
        self.knowledge.encode(&mut w);
        // Round 2's state: growth clocks, houses waiting to be placed, bases' markers, bands'
        // waiting packets and players' positions.
        for (n, g) in &self.growth {
            w.u64(*n as u64);
            w.u32(g.houses);
            w.u32(g.stamp);
            w.u64(g.since.map_or(u64::MAX, |t| t.0));
        }
        for (op, n) in &self.houses_pending {
            w.u64(*op);
            w.u64(*n as u64);
        }
        for (n, b) in &self.bases {
            w.u64(*n as u64);
            for p in &b.players {
                w.u64(p.to_bits());
            }
        }
        for b in &self.bands {
            for p in &b.pending {
                w.u64(p.map_or(u64::MAX, |x| x.0));
            }
        }
        for (a, p) in &self.players {
            w.u64(a.to_bits());
            if let Some((pos, t)) = p.pos {
                w.u64(t.0);
                w.u32(pos.x as u32);
                w.u32(pos.z as u32);
            }
        }
        w.u64(self.op_counter);
        for (k, id) in &self.promoted {
            w.u32(k.edge);
            w.u8(k.dir);
            w.u64(k.departure);
            w.u64(id.to_bits());
        }
        // The money sink's state (SINK.md): catalogue, purses' outflows, pointers, bookings.
        for d in &self.works_catalog {
            w.u64(d.key());
            w.u32(d.variants.len() as u32);
        }
        for (n, s) in &self.works {
            w.u64(*n as u64);
            w.u64(s.style);
            for (g, a) in s.spent.iter().chain(&s.tribute).chain(&s.granted) {
                w.u16(g.0);
                w.i64(*a);
            }
            for (k, c) in &s.bought {
                w.u64(*k);
                w.u32(*c);
            }
            for k in &s.full {
                w.u64(*k);
            }
            w.u32(s.nth);
            w.u32(s.stamp);
            w.u64(s.due.map_or(u64::MAX, |t| t.0));
            w.u64(s.threat_at.map_or(u64::MAX, |t| t.0));
            w.bool(s.tributary);
            w.u32(s.golems);
            w.u32(s.in_flight);
            match s.next {
                crate::works::Next::Unknown => w.u8(0),
                crate::works::Next::Nothing => w.u8(1),
                crate::works::Next::Work {
                    def,
                    variant,
                    price,
                } => {
                    w.u8(2);
                    w.u32(def);
                    w.u16(variant);
                    w.i64(price);
                }
            }
        }
        for (op, (n, k)) in &self.works_pending {
            w.u64(*op);
            w.u64(*n as u64);
            w.u64(*k);
        }
        for map in [
            &self.outside.player_in,
            &self.outside.player_out,
            &self.outside.destroyed,
            &self.outside.held,
        ] {
            for (g, a) in map {
                w.u16(g.0);
                w.raw(&a.to_le_bytes());
            }
        }
        fnv1a64(w.as_bytes())
    }
}
