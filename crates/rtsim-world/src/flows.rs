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
use rtsim_trade::line::Level;
use rtsim_trade::units::mul_div_floor;
use rtsim_trade::{Amount, GoodId, GoodKind, Market, PRICE_ONE, Price};
use rtsim_travel::alloc::{Candidate, allocate_with};
use rtsim_travel::edge::{DirRef, caravan_period, split_for_raid};
use rtsim_travel::news::Snapshot;
use rtsim_travel::pipeline::Pipeline;

use crate::world::{Cause, Ev, Node, PacketData, TraceRow, Via, World};

/// A pipeline changed: its destination must wake before the first changed arrival.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Change {
    pub dest: usize,
    pub first: u64,
    pub delay: u64,
    /// For the trace: the good, which pipeline (goods, raid, payment), and the rates before and
    /// after.
    pub good: GoodId,
    pub kind: PipeKind,
    pub old: Amount,
    pub new: Amount,
}

/// Which pipeline of a direction changed (for the trace).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PipeKind {
    Goods,
    Raid,
    Pay,
    /// Cut by the coverage rule (E20).
    Cover,
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

/// What a trader expects to lose per whole item sent on a ships' lane that pirates hunt: the
/// raided share of the destination's bid `v` (the destination pays only for what arrives), so a
/// hunted lane's margin falls and the trade goes another way or not at all (PLAN.md §10.1, "fewer
/// caravans take it"; overnight choice U9). Roads are left as they were: phase 1's raided roads
/// keep their own rule. Off with [`crate::WorldConfig::price_lane_hazard`].
fn lane_loss(
    e: &rtsim_travel::Edge,
    dir: &rtsim_travel::edge::Dir,
    v: Price,
    cfg: &crate::config::WorldConfig,
) -> Price {
    match dir.predation {
        Some(p) if cfg.price_lane_hazard && e.medium != rtsim_travel::Medium::Road && v > 0 => {
            mul_div_floor(v, p.share_milli.clamp(0, 1000), 1000)
        }
        _ => 0,
    }
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
    // Pipelines may be set more than once in a step (an allocation, then the coverage cut);
    // only what differs at the end is a change the destination must wake for.
    let before = pipe_states(node, edges);
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

    // 3. Exports, in recipe order: a good before the inputs of its recipe, so an input's
    // consumption already counts what its outputs' exports make the recipe use. (Round 1 went in
    // id order, so a bakery exported wheat as if it baked only for itself, then baked for its
    // bread exports too, fell below target and stopped: a two-quantum cycle.)
    for g in m.order.clone() {
        if goods.get(g).kind != GoodKind::Item {
            continue;
        }
        if m.goods[g.index()].used_here {
            m.recompute();
        }
        let current: Amount = node
            .out
            .iter()
            .map(|d| {
                let dir = &edges[d.edge as usize].dirs[d.dir as usize];
                dir.goods.get(&g).map_or(0, |p| p.current_rate())
                    + dir.raid.get(&g).map_or(0, |p| p.current_rate())
            })
            .sum();
        let exportable = m.exportable_held(g, current);
        // The ask: our price, but never above our anchor inside the band (overnight choice E45).
        let mine = if m.rules.ask_at_anchor {
            m.price(g).min(m.anchor(g))
        } else {
            m.price(g)
        };
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
            let barter = m.rules.barter_in_flows && barter_good_for(m, snap, goods).is_some();
            let refused = pay_cur.is_none_or(|c| m.refuses(c)) && !barter;
            let tau = transport_price(goods, &cfg.trade, g, e.d, l_mint);
            // The destination's bid: its price, but never below its anchor. Above its target it
            // asks for less through its advertised rate, so the flow eases off instead of
            // stopping at a margin cliff and starting again when it drains (the bang-bang limit
            // cycle the two-market test measured; overnight choice E18).
            let mut bid = gn.price.max(gn.anchor);
            // Hysteresis (overnight choice E51): a supplier already sending keeps sending while
            // the destination is anywhere in its target's band, as if it stood at the band's
            // bottom. Otherwise a dearer second supplier is called in by a shortage, ends it,
            // loses the margin the shortage paid for, stops, and the shortage comes back.
            if had && m.rules.hysteresis && gn.level >= Level::At {
                let r = cfg.trade.r;
                let floor_price = gn.anchor
                    + mul_div_floor(gn.anchor, r.num, r.den * rtsim_trade::line::BAND_DIV);
                bid = bid.max(floor_price);
            }
            // A destination with no cash can still pay in goods.
            let margin = match (
                to_home(m, snap.currency, bid),
                refused,
                snap.can_pay || barter,
            ) {
                (Some(v), false, true) => v - lane_loss(e, dir, v, cfg) - mine - tau + MARGIN_SLACK,
                _ => -1,
            };
            if margin < 0 && !had {
                continue;
            }
            // The price agreed: the destination's bid. Within its band and above it that is its
            // anchor, so a stock wandering inside the band does not re-price every flow into it
            // (overnight choice E45).
            let quote = if m.rules.ask_at_anchor && gn.level >= Level::At {
                gn.anchor
            } else {
                gn.price
            };
            let mut want = snap.want_from(node_index, g, m.rules.split_among_suppliers);
            // A ships' lane carries at most its holds' capacity (rtsim-ships, overnight choice
            // T4); roads have no cap.
            if let Some(room) = dir.room_for(goods, g, e.cap_slot_milli) {
                want = want.min(room);
            }
            cands.push(Candidate {
                dir: *d,
                margin,
                want,
                quote,
            });
        }
        share_destinations(&mut cands, edges);
        let wants: Vec<(DirRef, Amount)> = cands.iter().map(|c| (c.dir, c.want)).collect();
        let mut total = 0;
        for gr in allocate_with(exportable, cands, m.rules.ration) {
            let want = wants.iter().find(|w| w.0 == gr.dir).map_or(0, |w| w.1);
            let e = &mut edges[gr.dir.edge as usize];
            let delay = e.lag();
            let dir = &mut e.dirs[gr.dir.dir as usize];
            // A small move of what we can spare keeps the current rate and quote; what the
            // destination asked for is followed exactly, so its corrections are never blocked
            // (overnight choice E48).
            let was = dir.goods.get(&g).map_or(0, |p| p.current_rate())
                + dir.raid.get(&g).map_or(0, |p| p.current_rate());
            // A rate above what we can spare is kept only while we hold a surplus, so stickiness
            // never drains a store below its target.
            let surplus = m.line(g).stock >= m.line(g).target;
            let rate = if m.rules.sticky_goods && gr.rate < want && (was <= gr.rate || surplus) {
                m.rules.sticky(was, gr.rate)
            } else {
                gr.rate
            };
            let quote = if rate == was && rate > 0 {
                dir.goods.get(&g).map_or(gr.quote, |p| p.current_quote())
            } else {
                gr.quote
            };
            let gr = rtsim_travel::alloc::Grant {
                dir: gr.dir,
                rate,
                quote,
            };
            let (to_dest, to_raid) = split_for_raid(gr.rate, dir.predation);
            let p = dir.goods.entry(g).or_insert_with(|| Pipeline::new(delay));
            let old = p.current_rate();
            if p.set(nb, to_dest, gr.quote) {
                changes.push(Change {
                    dest: dir.to,
                    first: nb,
                    delay,
                    good: g,
                    kind: PipeKind::Goods,
                    old,
                    new: to_dest,
                });
            }
            if let Some(pr) = dir.predation {
                let rp = dir.raid.entry(g).or_insert_with(|| Pipeline::new(pr.delay));
                let old = rp.current_rate();
                if rp.set(nb, to_raid, 0) {
                    changes.push(Change {
                        dest: pr.haven,
                        first: nb,
                        delay: pr.delay,
                        good: g,
                        kind: PipeKind::Raid,
                        old,
                        new: to_raid,
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
    // What each foreign coin can pay out per quantum: what arrives of it plus its stock spent
    // over `T` (review fix, overnight choice E19 revised): a payer pays in the exporter's coin
    // as far as that lasts and the rest in its own, never all or nothing.
    let mut fx_left: BTreeMap<GoodId, Amount> = BTreeMap::new();
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
        if !m.rules.barter_in_flows {
            let Some(hc) = home else { continue };
            let mut home_value = value;
            if let Some(tc) = theirs.filter(|&tc| tc != hc)
                && m.price(tc) > 0
            {
                let left = fx_left.entry(tc).or_insert_with(|| {
                    // Its stock spent over `T`, rounded down to the line's news step, so a stock
                    // moving a little does not move every payment (E64).
                    let l = m.line(tc);
                    let step = m.news_step(tc);
                    l.rates.inflow + l.stock.max(0) / m.t_quanta.max(1) as i64 / step * step
                });
                let full = mul_div_floor(value, PRICE_ONE, m.price(tc));
                let in_tc = full.min(*left);
                let covered = mul_div_floor(in_tc, m.price(tc), PRICE_ONE);
                if in_tc > 0 && covered > 0 {
                    *left -= in_tc;
                    *pay.entry((*d, tc)).or_default() += in_tc;
                    home_value = (value - covered).max(0);
                }
            }
            if home_value > 0 {
                *pay.entry((*d, hc)).or_default() += home_value;
            }
            continue;
        }
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
        let refused = m.rules.barter_in_flows
            && (node
                .news
                .get(din.from)
                .is_some_and(|s| s.good(cur).level == Level::Glut)
                || m.stock(cur) < amount);
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
    m.owed = home.map_or(0, |h| {
        pay.iter()
            .filter(|((_, c), _)| *c == h)
            .map(|(_, a)| *a)
            .sum()
    });
    set_payments(m, node, edges, &pay, nb, changes);

    // 5. Coverage: no step may take out more than there is. A treasury below its band that pays
    // out more than comes in pays only what comes in (overnight choice E50): it passes its income
    // on instead of draining to nothing and flickering between paying in full and being cut.
    for _ in 0..2 {
        let mut cut = false;
        for gi in 0..m.lines.len() {
            let l = m.lines[gi];
            let income = l.rates.inflow + l.rates.cap - l.rates.cons;
            // With stickiness on, a payment a little above income is let stand until the store
            // is out (the uncovered rule below), so small moves of income do not ripple on.
            let over = if m.rules.sticky_div > 0 || m.rules.pay_band_div > 0 {
                let div = m.rules.sticky_div.max(m.rules.pay_band_div);
                let tol = (income.max(0) / div).max(STICKY_MIN);
                l.rates.outflow > income.max(0) + tol
            } else {
                l.rates.outflow > income.max(0)
            };
            let below = l.stock < l.target - rtsim_trade::line::band(l.target)
                || (m.rules.hysteresis
                    && m.cut
                    && Some(GoodId(gi as u16)) == m.currency
                    && l.stock < l.target);
            // Only the home treasury: a foreign coin line pays what it holds and receives by the
            // split above, never cut to its income and held there (review fix).
            let insolvent =
                m.rules.exporter_band && Some(GoodId(gi as u16)) == m.currency && below && over;
            if l.rates.outflow == 0 || !(insolvent || l.next_change().uncovered == Some(1)) {
                continue;
            }
            cut = true;
            let g = GoodId(gi as u16);
            let allowed = income.max(0).min(l.rates.outflow);
            let total = l.rates.outflow;
            // Every pipeline of the good leaving here, goods (with their raid share) and
            // payments, scaled alike; what floor rounding leaves goes to the biggest, so exactly
            // `allowed` leaves and a cut treasury holds still (overnight choice E50).
            let mut entries: Vec<(DirRef, bool, Amount)> = Vec::new();
            for d in &node.out {
                let dir = &edges[d.edge as usize].dirs[d.dir as usize];
                let raid = dir.raid.get(&g).map_or(0, |r| r.current_rate());
                if let Some(p) = dir.goods.get(&g) {
                    entries.push((*d, false, p.current_rate() + raid));
                }
                if let Some(p) = dir.pay.get(&g) {
                    entries.push((*d, true, p.current_rate()));
                }
            }
            let mut now: Vec<Amount> = entries
                .iter()
                .map(|e| mul_div_floor(e.2, allowed, total.max(1)))
                .collect();
            let left = allowed - now.iter().sum::<Amount>();
            if left > 0
                && let Some(k) =
                    (0..entries.len()).max_by_key(|&k| (entries[k].2, std::cmp::Reverse(k)))
            {
                now[k] += left;
            }
            let mut sum = 0;
            for (k, &(d, is_pay, _)) in entries.iter().enumerate() {
                let dir = &mut edges[d.edge as usize].dirs[d.dir as usize];
                if is_pay {
                    if let Some(p) = dir.pay.get_mut(&g) {
                        p.set(nb, now[k], 0);
                    }
                } else {
                    let pred = dir.predation;
                    let (to_dest, to_raid) = split_for_raid(now[k], pred);
                    if let Some(p) = dir.goods.get_mut(&g) {
                        let quote = p.current_quote();
                        p.set(nb, to_dest, quote);
                    }
                    if pred.is_some()
                        && let Some(rp) = dir.raid.get_mut(&g)
                    {
                        rp.set(nb, to_raid, 0);
                    }
                }
                sum += now[k];
            }
            m.lines[gi].rates.outflow = sum;
        }
        if !cut {
            break;
        }
    }
    m.recompute();
    m.cut = m.currency.is_some_and(|c| m.line(c).rates.outflow < m.owed);

    changes.clear();
    let after = pipe_states(node, edges);
    net_changes(&before, &after, edges, nb, changes);

    // Caravan lattices follow the volume: a caravan leaves when the dock has filled it or its
    // oldest load has waited the longest wait (overnight choice E53). Old segments are forgotten.
    let slots = match node.kind.culture() {
        Some(rtsim_settle::Culture::Ribbit) => cfg.trade.ribbit_caravan_slots,
        _ => cfg.trade.caravan_slots,
    };
    for d in &node.out {
        let e = &mut edges[d.edge as usize];
        let ed = e.lag();
        let dock = e.dock.max(1);
        let idle = q.days(cfg.scout_days).min(dock);
        let dir = &mut e.dirs[d.dir as usize];
        let period = caravan_period(dir.volume_slot_milli(goods), slots, dock, idle);
        dir.lattice.set(nb, period);
        if cfg.keep_history {
            continue;
        }
        let b = nb.saturating_sub(1);
        for p in dir
            .goods
            .values_mut()
            .chain(dir.pay.values_mut())
            .chain(dir.raid.values_mut())
        {
            p.trim(b);
        }
        // A pipeline with nothing on the road and nothing coming is forgotten: most of a settled
        // world's pipelines are (overnight choice E58). It comes back empty when a flow does.
        dir.goods.retain(|_, p| !p.is_idle(b));
        dir.pay.retain(|_, p| !p.is_idle(b));
        dir.raid.retain(|_, p| !p.is_idle(b));
        dir.lattice.trim(b.saturating_sub(2 * ed + idle));
    }
}

/// A good the destination of `snap` could barter with us: it has a surplus (at or above its
/// target) and we can still store it (not past twice our target). Goods we need come first, then
/// the one we value most; a cash-refusing market takes goods it can trade on, so a structural
/// surplus keeps goods moving instead of starving its neighbours (overnight choice E21).
/// Two edges from this market to one destination (a road and a ships' lane, overnight choice T5)
/// share what the destination wants instead of each sending all of it: the better margin first
/// (ties by direction), each up to its own want (a lane's is capped by its holds). One edge per
/// destination, as every phase 1 world has, is left exactly as it was.
fn share_destinations(cands: &mut [Candidate], edges: &[rtsim_travel::Edge]) {
    if cands.len() < 2 {
        return;
    }
    let to = |c: &Candidate| edges[c.dir.edge as usize].dirs[c.dir.dir as usize].to;
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by(|&a, &b| {
        to(&cands[a])
            .cmp(&to(&cands[b]))
            .then(cands[b].margin.cmp(&cands[a].margin))
            .then(cands[a].dir.cmp(&cands[b].dir))
    });
    let mut k = 0;
    while k < order.len() {
        let dest = to(&cands[order[k]]);
        let mut end = k + 1;
        while end < order.len() && to(&cands[order[end]]) == dest {
            end += 1;
        }
        if end - k > 1 {
            // What the destination wants from us is the same on every edge (its news): the
            // largest want stands for it.
            let mut left = order[k..end]
                .iter()
                .map(|&i| cands[i].want)
                .max()
                .unwrap_or(0);
            for &i in &order[k..end] {
                let c = &mut cands[i];
                if c.margin < 0 {
                    continue;
                }
                let take = c.want.min(left).max(0);
                c.want = take;
                left -= take;
            }
        }
        k = end;
    }
}

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
fn barter_good_for_payer(
    m: &Market,
    snap: &Snapshot,
    goods: &rtsim_trade::Goods,
) -> Option<GoodId> {
    goods
        .ids()
        .filter(|&h| {
            goods.get(h).kind == GoodKind::Item
                && m.line(h).stock >= m.line(h).target
                && m.line(h).target > 0
                && snap.good(h).level != Level::Glut
        })
        .max_by_key(|&h| {
            (
                snap.good(h).absorbable,
                snap.good(h).price,
                std::cmp::Reverse(h),
            )
        })
}

/// The smallest change of a rate that stickiness lets through, thousandths: a payment moving by a
/// thousandth or two between two broke markets is not worth a wake (overnight choice E48).
const STICKY_MIN: Amount = 4;

/// One pipeline leaving a node, as (direction, pipeline, good) with its current (rate, quote).
type PipeState = ((DirRef, PipeKind, GoodId), (Amount, Price));

/// The current rate and quote of every pipeline leaving `node`, sorted.
fn pipe_states(node: &Node, edges: &[rtsim_travel::Edge]) -> Vec<PipeState> {
    let mut v = Vec::new();
    for d in &node.out {
        let dir = &edges[d.edge as usize].dirs[d.dir as usize];
        for (kind, map) in [
            (PipeKind::Goods, &dir.goods),
            (PipeKind::Raid, &dir.raid),
            (PipeKind::Pay, &dir.pay),
        ] {
            for (g, p) in map {
                let r = p.current_rate();
                // A stopped pipeline's quote does not matter.
                let q = if r == 0 { 0 } else { p.current_quote() };
                v.push(((*d, kind, *g), (r, q)));
            }
        }
    }
    v.sort_unstable_by_key(|x| x.0);
    v
}

/// The pipelines whose rate or quote differs between `before` and `after`.
fn net_changes(
    before: &[PipeState],
    after: &[PipeState],
    edges: &[rtsim_travel::Edge],
    nb: u64,
    changes: &mut Vec<Change>,
) {
    let old_of = |k: &(DirRef, PipeKind, GoodId)| {
        before
            .binary_search_by_key(k, |x| x.0)
            .map_or((0, 0), |i| before[i].1)
    };
    for (k, now) in after {
        let old = old_of(k);
        if old == *now {
            continue;
        }
        let (d, kind, g) = *k;
        let e = &edges[d.edge as usize];
        let dir = &e.dirs[d.dir as usize];
        let (dest, delay) = match (kind, dir.predation) {
            (PipeKind::Raid, Some(pr)) => (pr.haven, pr.delay),
            _ => (dir.to, e.lag()),
        };
        changes.push(Change {
            dest,
            first: nb,
            delay,
            good: g,
            kind,
            old: old.0,
            new: now.0,
        });
    }
}

/// Counts, per good, the neighbours that could answer the market's advertisement: those supplying
/// it now and those whose news says they have it to spare (overnight choice E46).
fn count_suppliers(s: &mut Snapshot, node: &Node) {
    let inflows = std::mem::take(&mut s.inflows);
    for (gi, gn) in s.goods.iter_mut().enumerate() {
        let g = GoodId(gi as u16);
        let n = node
            .neighbours
            .iter()
            .filter(|&&j| {
                inflows
                    .iter()
                    .any(|x| x.0 as usize == j && x.1 == g && x.2 > 0)
                    || node
                        .news
                        .get(j)
                        .is_some_and(|sn| !sn.is_base && sn.good(g).level >= Level::At)
            })
            .count();
        gn.suppliers = n.clamp(1, usize::from(u16::MAX)) as u16;
    }
    s.inflows = inflows;
}

/// What reaches the market at boundary `nb`, per (source, good), sorted.
fn inflows_by_source(
    node: &Node,
    edges: &[rtsim_travel::Edge],
    nb: u64,
) -> Vec<(u32, GoodId, Amount)> {
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
        let delay = e.lag();
        let dir = &mut e.dirs[d.dir as usize];
        let to = dir.to;
        let curs: std::collections::BTreeSet<GoodId> = dir
            .pay
            .keys()
            .copied()
            .chain(
                pay.range((*d, GoodId(0))..=(*d, GoodId(u16::MAX)))
                    .map(|(k, _)| k.1),
            )
            .collect();
        for cur in curs {
            let p = dir.pay.entry(cur).or_insert_with(|| Pipeline::new(delay));
            let old = p.current_rate();
            let rate = m.rules.pay_held(
                old,
                m.rules
                    .sticky(old, pay.get(&(*d, cur)).copied().unwrap_or(0)),
            );
            if p.set(nb, rate, 0) {
                changes.push(Change {
                    dest: to,
                    first: nb,
                    delay,
                    good: cur,
                    kind: PipeKind::Pay,
                    old,
                    new: rate,
                });
            }
            m.line_mut(cur).rates.outflow += rate;
        }
    }
}

impl World {
    /// Something reached market `i` at `at`; `breakpoint` when it is one of the market's own.
    pub(crate) fn market_event(&mut self, i: usize, at: Tick, cause: Cause) {
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
                step(
                    m,
                    i,
                    node,
                    nodes,
                    edges,
                    goods,
                    cfg,
                    l_mint,
                    nb,
                    &mut changes,
                );
                let mut s = Snapshot::of_after(i, m, at, node.last_sent.as_deref());
                s.inflows = inflows_by_source(node, edges, nb);
                count_suppliers(&mut s, node);
                s
            })
            .expect("every node has a market");
        let changed = changes.len() as u32;
        let flows: Vec<(usize, GoodId, PipeKind, Amount, Amount)> = if self.trace.is_some() {
            changes
                .iter()
                .map(|c| (c.dest, c.good, c.kind, c.old, c.new))
                .collect()
        } else {
            Vec::new()
        };
        for c in changes {
            self.stats.segments += 1;
            self.schedule(q.at(c.first + c.delay - 1), Ev::Wake(c.dest));
        }
        let snap = Arc::new(snap);
        let differs = self.nodes[i]
            .last_sent
            .as_ref()
            .is_none_or(|l| snap.differs_in_kind(l));
        let news = cause.forces_news() || differs;
        if let Some(trace) = self.trace.as_mut() {
            let (diffs, cash_changed) = match self.nodes[i].last_sent.as_ref() {
                Some(l) => (
                    snap.goods
                        .iter()
                        .zip(&l.goods)
                        .enumerate()
                        .filter(|(_, (a, b))| a.level != b.level || a.absorbable != b.absorbable)
                        .map(|(g, (a, b))| {
                            (
                                GoodId(g as u16),
                                b.level,
                                a.level,
                                b.absorbable,
                                a.absorbable,
                            )
                        })
                        .collect(),
                    l.can_pay != snap.can_pay || l.refuses_cash != snap.refuses_cash,
                ),
                None => (Vec::new(), true),
            };
            trace.push(TraceRow {
                at,
                node: i,
                cause,
                changed,
                flows,
                news,
                diffs,
                cash_changed,
            });
        }
        if news {
            self.emit_snapshot(i, at, snap);
        }
        self.growth_check(i, at);
        if self.cfg.check_every_event {
            self.assert_conserved(at);
        }
    }

    /// Sends market `i`'s news on every edge with the next caravan. `force` sends even when
    /// nothing changed in kind (at build, on joining).
    pub(crate) fn emit(&mut self, i: usize, at: Tick, force: bool) {
        let m = self.market_at(i, at);
        let nb = self.cfg.trade.quantum.index(at) + 1;
        let mut s = Snapshot::of_after(i, &m, at, self.nodes[i].last_sent.as_deref());
        s.inflows = inflows_by_source(&self.nodes[i], &self.edges, nb);
        count_suppliers(&mut s, &self.nodes[i]);
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
                            via: Via::Road(d),
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
        // Touring bands that stop here carry the same news on to their next stop (P16).
        for (b, stop) in self.nodes[i].bands.clone() {
            let (b, stop) = (b as usize, stop as usize);
            let band = &mut self.bands[b];
            let n = band.route.len();
            let to = band.route[(stop + 1) % n];
            let departs = band.next_departure(stop, nb);
            let walk = band.walks[stop];
            let reports = self.knowledge.packet_since(actor, &mut band.cursors[stop]);
            let mut relays = self.nodes[i].news.received_after(band.relayed[stop], to);
            relays.truncate(self.cfg.relay_max);
            band.relayed[stop] = at;
            let pending = band.pending[stop].filter(|&(dep, _)| dep >= nb);
            match pending.and_then(|(_, pid)| self.packets.get_mut(&pid)) {
                Some(p) => {
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
                            via: Via::Band {
                                band: b as u32,
                                stop: stop as u32,
                            },
                            from: i,
                            to,
                            departs,
                            own: own.clone(),
                            relays,
                            reports,
                        },
                    );
                    self.bands[b].pending[stop] = Some((departs, pid));
                    self.stats.packets_sent += 1;
                    self.stats.band_packets += 1;
                    self.schedule(q.at(departs + walk), Ev::Deliver(pid));
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
        if learned > 0 {
            self.touch_sentiment(to);
        }
        self.flush_gossip();
        self.market_event(to, at, Cause::Delivery);
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
