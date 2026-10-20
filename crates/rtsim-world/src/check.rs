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
            for (gi, l) in m.lines.iter().enumerate() {
                let k = kind(GoodId(gi as u16));
                seen.add(k, i128::from(l.stock));
                let b = l.book;
                let w = i128::from;
                book.mint(k, w(b.genesis) + w(b.produced) + w(b.player_in));
                book.burn(
                    k,
                    w(b.consumed) + w(b.player_out) + w(b.destroyed) + w(b.sunk),
                );
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
            for (g, a) in c.cargo() {
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
        book.check(&seen)
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
            for (g, a) in c.cargo() {
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
        for map in [
            &self.outside.player_in,
            &self.outside.player_out,
            &self.outside.destroyed,
        ] {
            for (g, a) in map {
                w.u16(g.0);
                w.raw(&a.to_le_bytes());
            }
        }
        fnv1a64(w.as_bytes())
    }
}
