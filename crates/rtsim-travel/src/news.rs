// SPDX-License-Identifier: AGPL-3.0-or-later
//! Prices travel only as news (C13, PLAN.md §8.4; Gintis: stale private news stabilizes, a live
//! global board destabilizes). A [`Snapshot`] is what a market looked like at one moment; it is
//! immutable and shared, so relaying it costs a pointer. Each market keeps the freshest snapshot of
//! each market it has heard of in a capped [`NewsRing`]; allocation reads only that ring.

use std::collections::BTreeMap;
use std::sync::Arc;

use rtsim_kernel::clock::Tick;
use rtsim_settle::Culture;
use rtsim_trade::line::Level;
use rtsim_trade::{Amount, GoodId, Market, Price};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GoodNews {
    pub price: Price,
    /// `p*`, the price at the target.
    pub anchor: Price,
    pub stock: Amount,
    pub target: Amount,
    /// Thousandths per quantum the market asked for on top of what reaches it now (negative: it
    /// asks for less).
    pub absorbable: Amount,
    pub level: Level,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// The market's index in its world.
    pub market: usize,
    pub key: u64,
    pub at: Tick,
    pub culture: Option<Culture>,
    pub currency: Option<GoodId>,
    pub is_base: bool,
    pub can_pay: bool,
    pub refuses_cash: bool,
    pub goods: Vec<GoodNews>,
    /// What reached the market per (source market, good) when the snapshot was taken, sorted. An
    /// exporter adds its own share to the advertised change to know what to send, since the
    /// advertisement is net of everything arriving, its own flow included.
    pub inflows: Vec<(u32, GoodId, Amount)>,
}

impl Snapshot {
    pub fn of(market: usize, m: &Market, at: Tick) -> Snapshot {
        let goods = (0..m.lines.len())
            .map(|i| {
                let g = GoodId(i as u16);
                let l = m.line(g);
                GoodNews {
                    price: m.price(g),
                    anchor: m.anchor(g),
                    stock: l.stock,
                    target: l.target,
                    absorbable: m.absorbable(g),
                    level: l.level(),
                }
            })
            .collect();
        Snapshot {
            market,
            key: m.key,
            at,
            culture: m.culture,
            currency: m.currency,
            is_base: !m.is_storehouse(),
            can_pay: m.can_pay(),
            refuses_cash: m.cash_refused(),
            goods,
            inflows: Vec::new(),
        }
    }

    /// What `source` was landing here of `g` when the snapshot was taken.
    pub fn inflow_from(&self, source: usize, g: GoodId) -> Amount {
        self.inflows
            .binary_search_by_key(&(source as u32, g), |x| (x.0, x.1))
            .map_or(0, |i| self.inflows[i].2)
    }

    pub fn good(&self, g: GoodId) -> &GoodNews {
        &self.goods[g.index()]
    }

    /// Whether the decisions it drives would differ from `other`'s: levels, absorbable rates,
    /// whether it can pay or refuses cash, and which foreign currencies it holds. Price alone,
    /// which moves with every unit of stock, does not count (overnight choice E11).
    pub fn differs_in_kind(&self, other: &Snapshot) -> bool {
        self.can_pay != other.can_pay
            || self.refuses_cash != other.refuses_cash
            || self
                .goods
                .iter()
                .zip(&other.goods)
                .any(|(a, b)| a.level != b.level || a.absorbable != b.absorbable)
    }
}

/// A market's news: the freshest snapshot of each market it has heard of. Capped; the stalest
/// entry goes first, and a market never forgets its own neighbours' news for a stranger's
/// (`pinned`).
#[derive(Clone, Debug, Default)]
pub struct NewsRing {
    cap: usize,
    /// Source market to (snapshot, when it arrived here).
    entries: BTreeMap<usize, (Arc<Snapshot>, Tick)>,
}

impl NewsRing {
    pub fn new(cap: usize) -> Self {
        NewsRing {
            cap: cap.max(1),
            entries: BTreeMap::new(),
        }
    }

    pub fn get(&self, market: usize) -> Option<&Arc<Snapshot>> {
        self.entries.get(&market).map(|e| &e.0)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Keeps `snap` if it is newer than what the ring holds for its market. Returns whether it
    /// was kept. `pinned` lists markets never evicted (the holder's neighbours).
    pub fn put(&mut self, snap: Arc<Snapshot>, received: Tick, pinned: &[usize]) -> bool {
        if let Some((old, _)) = self.entries.get(&snap.market)
            && old.at >= snap.at
        {
            return false;
        }
        self.entries.insert(snap.market, (snap, received));
        while self.entries.len() > self.cap {
            let victim = self
                .entries
                .iter()
                .filter(|(m, _)| !pinned.contains(m))
                .min_by_key(|(m, e)| (e.0.at, **m))
                .map(|(m, _)| *m);
            match victim {
                Some(v) => {
                    self.entries.remove(&v);
                }
                None => break,
            }
        }
        true
    }

    /// Snapshots received after `since`, excluding `except`, oldest first: what to relay.
    pub fn received_after(&self, since: Tick, except: usize) -> Vec<Arc<Snapshot>> {
        let mut v: Vec<(Tick, usize, Arc<Snapshot>)> = self
            .entries
            .iter()
            .filter(|(m, e)| e.1 > since && **m != except)
            .map(|(m, e)| (e.1, *m, e.0.clone()))
            .collect();
        v.sort_by_key(|x| (x.0, x.1));
        v.into_iter().map(|x| x.2).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&usize, &Arc<Snapshot>)> {
        self.entries.iter().map(|(m, e)| (m, &e.0))
    }

    pub fn heap_bytes(&self) -> usize {
        self.entries.len() * 64
    }
}
