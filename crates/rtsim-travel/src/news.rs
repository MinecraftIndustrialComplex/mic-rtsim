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
    /// How many neighbours could answer the advertisement (those supplying the good now and those
    /// whose news says they have it to spare), at least 1. Each answers its share of an increase,
    /// so several suppliers do not all fill the same shortage (overnight choice E46).
    pub suppliers: u16,
    /// A currency line (the treasury or an exchange line).
    pub currency: bool,
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
        Self::of_after(market, m, at, None)
    }

    /// A snapshot whose advertised rates hold those of `prev` (the last one sent) where they
    /// have not moved by a news step (overnight choice E51).
    pub fn of_after(market: usize, m: &Market, at: Tick, prev: Option<&Snapshot>) -> Snapshot {
        let goods = (0..m.lines.len())
            .map(|i| {
                let g = GoodId(i as u16);
                let l = m.line(g);
                GoodNews {
                    price: m.price(g),
                    anchor: m.anchor(g),
                    stock: l.stock,
                    target: l.target,
                    absorbable: m
                        .absorbable_held(g, prev.map(|p| p.good(g).absorbable + p.inflow_total(g))),
                    level: l.level(),
                    suppliers: 1,
                    currency: m.good(g).currency,
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

    /// Everything reaching the market of `g` when the snapshot was taken.
    pub fn inflow_total(&self, g: GoodId) -> Amount {
        self.inflows.iter().filter(|x| x.1 == g).map(|x| x.2).sum()
    }

    /// What `source` should send of `g` by this news: what it was sending plus its share of the
    /// advertised change. An increase is split evenly among the possible suppliers; a cut falls
    /// on the current suppliers in proportion to what each sends (overnight choice E46). With
    /// `split` off, every supplier answers the whole advertisement, as round 1 did.
    pub fn want_from(&self, source: usize, g: GoodId, split: bool) -> Amount {
        let own = self.inflow_from(source, g);
        let gn = self.good(g);
        let adv = gn.absorbable;
        if !split {
            return (own + adv).max(0);
        }
        let change = if adv >= 0 {
            let n = i64::from(gn.suppliers.max(1));
            (adv + n - 1) / n
        } else {
            let total = self.inflow_total(g);
            if total > 0 {
                (i128::from(adv) * i128::from(own) / i128::from(total)) as Amount
            } else {
                adv
            }
        };
        (own + change).max(0)
    }

    pub fn good(&self, g: GoodId) -> &GoodNews {
        &self.goods[g.index()]
    }

    /// Whether the decisions it drives would differ from `other`'s: levels, absorbable rates,
    /// whether it can pay or refuses cash, and which foreign currencies it holds. Price alone,
    /// which moves with every unit of stock, does not count (overnight choice E11).
    ///
    /// For a currency line only what a payer decides by counts: whether the market holds any
    /// (it is paid in its own coin if it does) and whether it refuses it. A treasury drifting
    /// between levels changes no decision, so it is not news (overnight choice E47).
    pub fn differs_in_kind(&self, other: &Snapshot) -> bool {
        self.can_pay != other.can_pay
            || self.refuses_cash != other.refuses_cash
            || self.goods.iter().zip(&other.goods).any(|(a, b)| {
                if a.currency {
                    (a.stock > 0) != (b.stock > 0)
                        || (a.level == Level::Glut) != (b.level == Level::Glut)
                } else {
                    a.level != b.level || a.absorbable != b.absorbable
                }
            })
    }
}

/// A market's news: the freshest snapshot of each market it has heard of. Capped; the stalest
/// entry goes first, and a market never forgets its own neighbours' news for a stranger's
/// (`pinned`).
#[derive(Clone, Debug, Default)]
pub struct NewsRing {
    pub(crate) cap: usize,
    /// Source market to (snapshot, when it arrived here).
    pub(crate) entries: BTreeMap<usize, (Arc<Snapshot>, Tick)>,
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
