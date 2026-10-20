// SPDX-License-Identifier: AGPL-3.0-or-later
//! What happens at a market when something reaches it, and how news leaves it.
//!
//! **A market event** (a breakpoint, a flow's arrivals changing, news arriving, a player trade):
//! 1. inflows are re-read from every pipeline arriving at the market, at the next boundary;
//! 2. capacities and consumption are recomputed ([`Market::recompute`]);
//! 3. the exportable rate of each good is allocated to edges by news margin, best first
//!    ([`rtsim_travel::alloc`]); raided roads send their share to the haven;
//! 4. what arrives is paid for at its quoted price, in the exporter's currency when the market
//!    holds it, else in its own (gold points form at border markets, PLAN.md §8.4);
//! 5. outflow that the next step could not cover is cut to what is sustainable, so no stock and no
//!    treasury ever goes negative (no debt);
//! 6. a snapshot is taken, and if it differs in kind from the last one sent, or the event was a
//!    breakpoint, news leaves on every edge with the next caravan.
//!
//! Every pipeline change books a wake at its destination for the boundary where its arrivals
//! change, so the destination's constant inflow is always the pipelines' true arrivals: that is the
//! invariant conservation rests on.

use std::collections::BTreeMap;
use std::sync::Arc;

use rtsim_kernel::clock::Tick;
use rtsim_trade::build::{MARGIN_SLACK, transport_price};
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::line::Level;
use rtsim_trade::{Amount, GoodId, GoodKind, Market, PRICE_ONE, Price};
use rtsim_travel::alloc::{Candidate, allocate};
use rtsim_travel::edge::{DirRef, caravan_period, split_for_raid};
use rtsim_travel::news::Snapshot;
use rtsim_travel::pipeline::Pipeline;

use crate::world::{Ev, Node, PacketData, World};

/// A pipeline changed: its destination must wake before the first changed arrival.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Change {
    pub dest: usize,
    pub first: u64,
    pub delay: u64,
}

/// A price in `from`'s currency, in `m`'s coins: through `m`'s exchange line when the currencies
/// differ. `None` when `m` has no rate for it.
pub(crate) fn to_home(m: &Market, from: Option<GoodId>, price: Price) -> Option<Price> {
    let from = from?;
    if Some(from) == m.currency {
        return Some(price);
    }
    let rate = m.price(from);
    (rate > 0).then(|| mul_div_floor(price, rate, PRICE_ONE))
}

#[allow(clippy::too_many_arguments)]
fn step(
    m: &mut Market,
    node_index: usize,
    node: &Node,
    nodes: &[Node],
    edges: &mut [rtsim_travel::Edge],
    goods: &rtsim_trade::Goods,
    cfg: &crate::config::WorldConfig,
    l_mint: f64,
    nb: u64,
    changes: &mut Vec<Change>,
) {
    for l in m.lines.iter_mut() {
        l.rates.inflow = 0;
        l.rates.outflow = 0;
    }
    if !m.is_storehouse() {
        return;
    }
    // 1. Inflows.
    for d in &node.out {
        let din = &edges[d.edge as usize].dirs[1 - d.dir as usize];
        for (g, p) in din.goods.iter().chain(din.pay.iter()) {
            m.line_mut(*g).rates.inflow += p.arrival_at(nb);
        }
    }
    for r in &node.raids_in {
        for (g, p) in &edges[r.edge as usize].dirs[r.dir as usize].raid {
            m.line_mut(*g).rates.inflow += p.arrival_at(nb);
        }
    }
    m.recompute();
    let q = cfg.trade.quantum;

    // 3. Exports.
    for g in goods.ids() {
        if goods.get(g).kind != GoodKind::Item {
            continue;
        }
        let exportable = m.exportable(g);
        let mine = m.price(g);
        let mut cands = Vec::with_capacity(node.out.len());
        for d in &node.out {
            let e = &edges[d.edge as usize];
            let dir = &e.dirs[d.dir as usize];
            let j = dir.to;
            let had = dir.goods.get(&g).is_some_and(|p| p.current_rate() > 0);
            let Some(snap) = node.news.get(j).filter(|s| !s.is_base) else {
                if had {
                    cands.push(Candidate {
                        dir: *d,
                        margin: -1,
                        want: 0,
                        quote: 0,
                    });
                }
                continue;
            };
            let gn = snap.good(g);
            // The destination pays in our currency if it holds some, else in its own.
            let pay_cur = match (m.currency, snap.currency) {
                (Some(mc), Some(sc)) if mc != sc && snap.good(mc).stock > 0 => Some(mc),
                (_, sc) => sc,
            };
            // Cash we refuse can still buy our goods by barter, if the destination has a
            // surplus of something we need (PLAN.md §8.4: above twice its target a market refuses
            // cash and barters at anchor ratios).
            let barter = barter_good_for(m, snap, goods).is_some();
            let refused = pay_cur.is_none_or(|c| m.refuses(c)) && !barter;
            let tau = transport_price(goods, &cfg.trade, g, e.d, l_mint);
            // The destination's bid: its price, but never below its anchor. Above its target it
            // asks for less through its advertised rate, so the flow eases off instead of
            // stopping at a margin cliff and starting again when it drains (the bang-bang limit
            // cycle the two-market test measured; overnight choice E18).
            let bid = gn.price.max(gn.anchor);
            // A destination with no cash can still pay in goods.
            let margin = match (to_home(m, snap.currency, bid), refused, snap.can_pay || barter) {
                (Some(v), false, true) => v - mine - tau + MARGIN_SLACK,
                _ => -1,
            };
            let own = snap.inflow_from(node_index, g);
            if margin < 0 && !had {
                continue;
            }
            cands.push(Candidate {
                dir: *d,
                margin,
                want: (own + gn.absorbable).max(0),
                quote: gn.price,
            });
        }
        let mut total = 0;
        for gr in allocate(exportable, cands) {
            let e = &mut edges[gr.dir.edge as usize];
            let delay = e.d;
            let dir = &mut e.dirs[gr.dir.dir as usize];
            let (to_dest, to_raid) = split_for_raid(gr.rate, dir.predation);
            let p = dir
                .goods
                .entry(g)
                .or_insert_with(|| Pipeline::new(delay));
            if p.set(nb, to_dest, gr.quote) {
                changes.push(Change {
                    dest: dir.to,
                    first: nb,
                    delay,
                });
            }
            if let Some(pr) = dir.predation {
                let rp = dir
                    .raid
                    .entry(g)
                    .or_insert_with(|| Pipeline::new(pr.delay));
                if rp.set(nb, to_raid, 0) {
                    changes.push(Change {
                        dest: pr.haven,
                        first: nb,
                        delay: pr.delay,
                    });
                }
            }
            total += gr.rate;
        }
        m.line_mut(g).rates.outflow = total;
    }

    // 4. Payments for what arrives at the next boundary.
    let home = m.currency;
    let mut pay: BTreeMap<(DirRef, GoodId), Amount> = BTreeMap::new();
    for d in &node.out {
        let e = &edges[d.edge as usize];
        let din = &e.dirs[1 - d.dir as usize];
        let mut value: i128 = 0;
        for p in din.goods.values() {
            let a = p.arrival_at(nb);
            if a > 0 {
                value += i128::from(a) * i128::from(p.arrival_quote(nb)) / i128::from(PRICE_ONE);
            }
        }
        if value <= 0 {
            continue;
        }
        let value = value as Amount;
        let theirs = nodes[din.from].currency;
        let (cur, amount) = match (theirs, home) {
            (Some(tc), Some(hc)) if tc != hc && m.stock(tc) > 0 && m.price(tc) > 0 => {
                (tc, mul_div_floor(value, PRICE_ONE, m.price(tc)))
            }
            (_, Some(hc)) => (hc, value),
            _ => continue,
        };
        // The exporter refuses that cash: pay in a good it needs and we have to spare, at our
        // anchor ratio (barter at anchor ratios, PLAN.md §8.4).
        // So is cash we do not have: a broke market pays in goods too.
        let refused = node.news.get(din.from).is_some_and(|s| s.good(cur).level == Level::Glut)
            || m.stock(cur) < amount;
        let (cur, amount) = match node.news.get(din.from).filter(|_| refused) {
            Some(snap) => match barter_good_for_payer(m, snap, goods) {
                Some(h) if m.anchor(h) > 0 => {
                    // value is in home coins: convert at the anchor of h.
                    let v_home = if cur == home.unwrap_or(cur) {
                        value
                    } else {
                        mul_div_floor(amount, m.price(cur), PRICE_ONE)
                    };
                    (h, mul_div_floor(v_home, PRICE_ONE, m.anchor(h)))
                }
                _ => (cur, amount),
            },
            None => (cur, amount),
        };
        *pay.entry((*d, cur)).or_default() += amount;
    }
    set_payments(m, node, edges, &pay, nb, changes);

    // 5. Coverage: no step may take out more than there is.
    for _ in 0..2 {
        let mut cut = false;
        for gi in 0..m.lines.len() {
            let l = m.lines[gi];
            if l.rates.outflow == 0 || l.next_change().uncovered != Some(1) {
                continue;
            }
            cut = true;
            let g = GoodId(gi as u16);
            let allowed = (l.rates.inflow + l.rates.cap - l.rates.cons).max(0);
            let total = l.rates.outflow;
            let mut sum = 0;
            for d in &node.out {
                let e = &mut edges[d.edge as usize];
                let delay = e.d;
                let dir = &mut e.dirs[d.dir as usize];
                let to = dir.to;
                let pred = dir.predation;
                // Goods to the destination (plus their raid share) and payments, scaled alike.
                let raid = dir.raid.get(&g).map_or(0, |r| r.current_rate());
                if let Some(p) = dir.goods.get_mut(&g) {
                    let was = p.current_rate() + raid;
                    let now = mul_div_floor(was, allowed, total.max(1));
                    let (to_dest, to_raid) = split_for_raid(now, pred);
                    let quote = p.current_quote();
                    if p.set(nb, to_dest, quote) {
                        changes.push(Change {
                            dest: to,
                            first: nb,
                            delay,
                        });
                    }
                    if let Some(pr) = pred
                        && let Some(rp) = dir.raid.get_mut(&g)
                        && rp.set(nb, to_raid, 0)
                    {
                        changes.push(Change {
                            dest: pr.haven,
                            first: nb,
                            delay: pr.delay,
                        });
                    }
                    sum += now;
                }
                if let Some(p) = dir.pay.get_mut(&g) {
                    let now = mul_div_floor(p.current_rate(), allowed, total.max(1));
                    if p.set(nb, now, 0) {
                        changes.push(Change {
                            dest: to,
                            first: nb,
                            delay,
                        });
                    }
                    sum += now;
                }
            }
            m.lines[gi].rates.outflow = sum;
        }
        if !cut {
            break;
        }
    }
    m.recompute();

    // Caravan lattices follow the volume; old segments are forgotten.
    let slots = cfg.trade.caravan_slots;
    let idle = q.days(cfg.scout_days);
    for d in &node.out {
        let e = &mut edges[d.edge as usize];
        let ed = e.d;
        let dir = &mut e.dirs[d.dir as usize];
        let period = caravan_period(dir.volume_slot_milli(goods), slots, ed, idle);
        dir.lattice.set(nb, period);
        if cfg.keep_history {
            continue;
        }
        let b = nb.saturating_sub(1);
        for p in dir.goods.values_mut().chain(dir.pay.values_mut()).chain(dir.raid.values_mut()) {
            p.trim(b);
        }
        dir.lattice.trim(b.saturating_sub(2 * ed + idle));
    }
}

/// A good the destination of `snap` could barter with us: it has a surplus (at or above its
/// target) and we can still store it (not past twice our target). Goods we need come first, then
/// the one we value most; a cash-refusing market takes goods it can trade on, so a structural
/// surplus keeps goods moving instead of starving its neighbours (overnight choice E21).
fn barter_good_for(m: &Market, snap: &Snapshot, goods: &rtsim_trade::Goods) -> Option<GoodId> {
    goods
        .ids()
        .filter(|&h| {
            goods.get(h).kind == GoodKind::Item
                && snap.good(h).level >= Level::At
                && m.line(h).level() != Level::Glut
                && m.anchor(h) > 0
        })
        .max_by_key(|&h| (m.absorbable(h) > 0, m.price(h), std::cmp::Reverse(h)))
}

/// A good we (the payer) can pay the exporter of `snap` in: we are at or above target in it and
/// it can still store it. The one it asks for most, then the one it values most.
fn barter_good_for_payer(m: &Market, snap: &Snapshot, goods: &rtsim_trade::Goods) -> Option<GoodId> {
    goods
        .ids()
        .filter(|&h| {
            goods.get(h).kind == GoodKind::Item
                && m.line(h).stock >= m.line(h).target
                && m.line(h).target > 0
                && snap.good(h).level != Level::Glut
        })
        .max_by_key(|&h| (snap.good(h).absorbable, snap.good(h).price, std::cmp::Reverse(h)))
}

/// What reaches the market at boundary `nb`, per (source, good), sorted.
fn inflows_by_source(node: &Node, edges: &[rtsim_travel::Edge], nb: u64) -> Vec<(u32, GoodId, Amount)> {
    let mut v = Vec::new();
    for d in &node.out {
        let din = &edges[d.edge as usize].dirs[1 - d.dir as usize];
        for (g, p) in &din.goods {
            let a = p.arrival_at(nb);
            if a != 0 {
                v.push((din.from as u32, *g, a));
            }
        }
    }
    v.sort_unstable();
    v
}

/// Sets every payment pipeline leaving the market from the totals per (direction, currency):
/// those not in `pay` stop. The market's currency lines take the sums as outflow.
fn set_payments(
    m: &mut Market,
    node: &Node,
    edges: &mut [rtsim_travel::Edge],
    pay: &BTreeMap<(DirRef, GoodId), Amount>,
    nb: u64,
    changes: &mut Vec<Change>,
) {
    for d in &node.out {
        let e = &mut edges[d.edge as usize];
        let delay = e.d;
        let dir = &mut e.dirs[d.dir as usize];
        let to = dir.to;
        let curs: std::collections::BTreeSet<GoodId> = dir
            .pay
            .keys()
            .copied()
            .chain(pay.range((*d, GoodId(0))..=(*d, GoodId(u16::MAX))).map(|(k, _)| k.1))
            .collect();
        for cur in curs {
            let rate = pay.get(&(*d, cur)).copied().unwrap_or(0);
            let p = dir.pay.entry(cur).or_insert_with(|| Pipeline::new(delay));
            if p.set(nb, rate, 0) {
                changes.push(Change {
                    dest: to,
                    first: nb,
                    delay,
                });
            }
            m.line_mut(cur).rates.outflow += rate;
        }
    }
}

impl World {
    /// Something reached market `i` at `at`; `breakpoint` when it is one of the market's own.
    pub(crate) fn market_event(&mut self, i: usize, at: Tick, breakpoint: bool) {
        self.stats.market_events += 1;
        let q = self.cfg.trade.quantum;
        let nb = q.index(at) + 1;
        let l_mint = self.nodes[i]
            .kind
            .culture()
            .map_or(f64::INFINITY, |c| self.mint_cost[c.index()]);
        let mut changes = Vec::new();
        let World {
            markets,
            nodes,
            edges,
            goods,
            cfg,
            ..
        } = self;
        let node = &nodes[i];
        let snap = markets
            .apply(node.region, at, |m, at| {
                step(m, i, node, nodes, edges, goods, cfg, l_mint, nb, &mut changes);
                let mut s = Snapshot::of(i, m, at);
                s.inflows = inflows_by_source(node, edges, nb);
                s
            })
            .expect("every node has a market");
        for c in changes {
            self.stats.segments += 1;
            self.schedule(q.at(c.first + c.delay - 1), Ev::Wake(c.dest));
        }
        let snap = Arc::new(snap);
        let differs = self.nodes[i]
            .last_sent
            .as_ref()
            .is_none_or(|l| snap.differs_in_kind(l));
        if breakpoint || differs {
            self.emit_snapshot(i, at, snap);
        }
        if self.cfg.check_every_event {
            self.assert_conserved(at);
        }
    }

    /// Sends market `i`'s news on every edge with the next caravan. `force` sends even when
    /// nothing changed in kind (at build, on joining).
    pub(crate) fn emit(&mut self, i: usize, at: Tick, force: bool) {
        let m = self.market_at(i, at);
        let nb = self.cfg.trade.quantum.index(at) + 1;
        let mut s = Snapshot::of(i, &m, at);
        s.inflows = inflows_by_source(&self.nodes[i], &self.edges, nb);
        let snap = Arc::new(s);
        let differs = self.nodes[i]
            .last_sent
            .as_ref()
            .is_none_or(|l| snap.differs_in_kind(l));
        if force || differs {
            self.emit_snapshot(i, at, snap);
        } else {
            // Gossip still goes, with the last snapshot.
            let last = self.nodes[i].last_sent.clone();
            self.emit_packets(i, at, last);
        }
    }

    pub(crate) fn emit_snapshot(&mut self, i: usize, at: Tick, snap: Arc<Snapshot>) {
        self.nodes[i].last_sent = Some(snap.clone());
        self.emit_packets(i, at, Some(snap));
    }

    /// Packs news and gossip for the next caravan on each of `i`'s edges, merging with a packet
    /// already waiting for that caravan.
    fn emit_packets(&mut self, i: usize, at: Tick, own: Option<Arc<Snapshot>>) {
        let q = self.cfg.trade.quantum;
        let nb = q.index(at) + 1;
        let actor = self.nodes[i].actor;
        let outs = self.nodes[i].out.clone();
        for d in outs {
            let e = &mut self.edges[d.edge as usize];
            let ed = e.d;
            let dir = &mut e.dirs[d.dir as usize];
            let to = dir.to;
            let departs = dir.lattice.next_departure(nb);
            let reports = self.knowledge.packet_since(actor, &mut dir.board_cursor);
            let mut relays = self.nodes[i].news.received_after(dir.relayed_through, to);
            relays.truncate(self.cfg.relay_max);
            dir.relayed_through = at;
            let pending = dir.pending.filter(|&(dep, _)| dep >= nb);
            match pending.and_then(|(_, pid)| self.packets.get_mut(&pid).map(|p| (pid, p))) {
                Some((_, p)) => {
                    if own.is_some() {
                        p.own = own.clone();
                    }
                    p.relays.extend(relays);
                    p.reports.merge(&reports);
                }
                None => {
                    if own.is_none() && reports.is_empty() && relays.is_empty() {
                        continue;
                    }
                    let pid = self.next_packet;
                    self.next_packet += 1;
                    self.packets.insert(
                        pid,
                        PacketData {
                            dir: d,
                            from: i,
                            to,
                            departs,
                            own: own.clone(),
                            relays,
                            reports,
                        },
                    );
                    dir.pending = Some((departs, pid));
                    self.stats.packets_sent += 1;
                    let when = q.at(departs + ed);
                    self.schedule(when, Ev::Deliver(pid));
                }
            }
        }
    }

    /// A packet reaches its destination: news goes into the ring, gossip onto the board, and the
    /// market acts on what it now knows.
    pub(crate) fn deliver(&mut self, pid: u32, at: Tick) {
        let Some(p) = self.packets.remove(&pid) else {
            return;
        };
        self.stats.deliveries += 1;
        let to = p.to;
        let pinned = self.nodes[to].neighbours.clone();
        for s in p.own.iter().chain(p.relays.iter()) {
            if s.market != to {
                self.nodes[to].news.put(s.clone(), at, &pinned);
            }
        }
        let learned = self.knowledge.deliver(self.nodes[to].actor, &p.reports, at);
        self.flush_gossip();
        self.market_event(to, at, false);
        if learned > 0 {
            // Gossip moves on with the next caravans: at the speed of trade.
            self.emit(to, at, false);
        }
    }

    /// The market at node `i` projected to `at`.
    pub(crate) fn market_at(&self, i: usize, at: Tick) -> Market {
        self.markets
            .get(self.nodes[i].region)
            .expect("every node has a market")
            .cell()
            .view_without_catch_up(at)
    }
}
