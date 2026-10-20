// SPDX-License-Identifier: AGPL-3.0-or-later
//! A storehouse market (C13, PLAN.md §8.4) as a kernel lazy model, and the economic rules that
//! read it.
//!
//! # As a lazy model
//!
//! A market has no randomness ([`LazyModel::slot_period`] is `None`), so it commits no slots and
//! the O5 slot cost does not apply. Between events its rates are constant, so every line follows
//! its closed form ([`crate::line`]); settling over any span costs O(goods) whatever the span.
//! [`LazyModel::next_breakpoint`] is the first quantum boundary at which any line changes level
//! (0, the target, twice the target), changes production regime when other lines use its output,
//! or is one step from leaving its outflow uncovered. Those are the moments the market's outputs
//! change in kind (SCALE.md §2), so the market emits news and reallocates only there.
//!
//! # Economic rules
//!
//! - [`Market::recompute`]: effective production capacity (an empty input stops its recipe) and
//!   consumption (households plus inputs to the market's own recipes), at every event.
//! - [`Market::exportable`] and [`Market::absorbable`]: what flows may take away and what the
//!   market advertises it can take in, as rates per quantum (PLAN.md §8.4 "Flows").
//! - Cash: a market refuses its own cash above twice its treasury target and any foreign cash
//!   above twice its holding target ([`Market::refuses`]).
//! - Player trades through the treasury (C14, C23): [`Market::quote_buy`], [`Market::quote_sell`],
//!   [`Market::offer`] and the barter quote at anchor ratios for a market that refuses cash.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{Encode, Writer};
use rtsim_kernel::lazy::LazyModel;
use rtsim_settle::Culture;

use crate::goods::GoodId;
use crate::line::{Level, Line, UNLIMITED, band};
use crate::price::PriceCurve;
use crate::tunables::Ratio;
use crate::units::{Amount, MILLI, PRICE_ONE, Price, Quantum};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarketKind {
    /// A village or haven: stock, treasury, production (C13).
    Storehouse,
    /// A player base (C26): no stock or treasury of its own; only the prices players traded at.
    Base,
}

/// One recipe input as the market's own coupling sees it: `per` thousandths of `good` for every
/// `yield_` thousandths of output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Input {
    pub good: GoodId,
    pub per: Amount,
    pub yield_: Amount,
}

/// What a market knows about one good besides its line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GoodState {
    pub curve: PriceCurve,
    /// Household consumption per quantum: `pop × need`.
    pub household: Amount,
    /// Production capacity per quantum from the chosen recipe, before input shortages.
    pub nominal_cap: Amount,
    /// Inputs of the chosen recipe (its own output excluded: seed grain is netted out of the
    /// yield at build time).
    pub inputs: Vec<Input>,
    /// Whether another of this market's recipes uses this good, so a change in its production
    /// changes their consumption: such lines also break at production changes.
    pub used_here: bool,
    /// A currency line (the treasury or an exchange line): never exported or advertised as a
    /// good; money moves only as payment.
    pub currency: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Market {
    pub key: u64,
    pub kind: MarketKind,
    pub culture: Option<Culture>,
    /// The home currency (the treasury is its line). `None` for player bases.
    pub currency: Option<GoodId>,
    pub quantum: Quantum,
    /// `T` in quanta for this market, with the derived lower bound applied.
    pub t_quanta: u64,
    pub lines: Vec<Line>,
    pub goods: Vec<GoodState>,
    /// Goods in an order where every good comes before the inputs of its recipe, so one pass of
    /// [`Market::recompute`] sets consumption from up-to-date production.
    pub order: Vec<GoodId>,
    /// For a player base: the last price players traded each good at, and when (C26: the base's
    /// prices are the prices players trade at there).
    pub base_prices: Vec<Option<(Price, Tick)>>,
    /// The tunables the market decides with ([`crate::tunables::Rules`]).
    pub rules: crate::tunables::Rules,
    /// What the market owes per quantum in its own coin for what arrives, before the coverage
    /// rule cuts it to what it can pay: set by the flows at every event (overnight choice E52).
    pub owed: Amount,
    /// Whether the last event cut the treasury's payments to its income (overnight choice E50):
    /// a cut treasury stays cut until it is back at its target, not just inside its band.
    pub cut: bool,
}

/// Why a player trade is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// The market does not hold that much.
    NoStock,
    /// The treasury cannot pay (C14: "a poor village cannot pay much").
    NoMoney,
    /// The treasury is above twice its target: cash refused, barter only (PLAN.md §8.4).
    CashRefused,
    /// Above twice its target the market takes no more of the good.
    NotWanted,
    /// A player base has no storehouse (C26).
    NotAStorehouse,
    /// A zero or negative quantity.
    Nothing,
}

/// Which way an offer goes, from the market's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OfferSide {
    /// The market (villager, ribbit merchant) sells the good for coins.
    MarketSells,
    /// The market buys the good for coins.
    MarketBuys,
}

/// A vanilla-shaped offer: `items` of the good for `coins` of the market's currency, `uses` times
/// before it shows crossed out (PLAN.md §8.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Offer {
    pub items: i64,
    pub coins: i64,
    pub uses: u32,
}

/// A priced player trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Quote {
    pub items: i64,
    pub coins: i64,
}

/// No markup.
pub const FAIR: Ratio = Ratio { num: 1, den: 1 };

/// How much worse a disliked actor's prices are, as a pair of factors for buying (at least 1) and
/// selling (at most 1). Never better than fair for anyone, so sentiment cannot be farmed into a
/// money pump (overnight choice E7). `worst` is the markup at sentiment -128. PLACEHOLDER (P18).
pub fn markup_for(sentiment: i8, worst: Ratio) -> (Ratio, Ratio) {
    if sentiment >= 0 {
        return (FAIR, FAIR);
    }
    let s = i64::from(sentiment).unsigned_abs() as i64;
    let den = 128 * worst.den;
    (
        Ratio::new(den + s * worst.num, den),
        Ratio::new((den - s * worst.num).max(0), den),
    )
}

impl Market {
    pub fn good(&self, g: GoodId) -> &GoodState {
        &self.goods[g.index()]
    }

    pub fn line(&self, g: GoodId) -> &Line {
        &self.lines[g.index()]
    }

    pub fn line_mut(&mut self, g: GoodId) -> &mut Line {
        &mut self.lines[g.index()]
    }

    pub fn stock(&self, g: GoodId) -> Amount {
        self.lines[g.index()].stock
    }

    /// The treasury in the home currency, thousandths.
    pub fn treasury(&self) -> Amount {
        self.currency.map_or(0, |c| self.stock(c))
    }

    pub fn is_storehouse(&self) -> bool {
        self.kind == MarketKind::Storehouse
    }

    /// `p(S)` now: for an item in home coins, for a foreign currency the exchange rate in home
    /// coins per foreign coin, for the home currency one coin.
    pub fn price(&self, g: GoodId) -> Price {
        if !self.is_storehouse() {
            return self.base_prices[g.index()].map_or(0, |p| p.0);
        }
        self.goods[g.index()].curve.price_at(self.stock(g))
    }

    pub fn anchor(&self, g: GoodId) -> Price {
        self.goods[g.index()].curve.anchor
    }

    /// Whether the market takes payment in currency `cur` now: refused above twice its target.
    pub fn refuses(&self, cur: GoodId) -> bool {
        self.lines[cur.index()].level() == Level::Glut
    }

    /// Whether the treasury refuses cash (PLAN.md §8.4: above `2 Mbar`, barter at anchor ratios).
    pub fn cash_refused(&self) -> bool {
        self.currency.is_some_and(|c| self.refuses(c))
    }

    /// Whether the market can pay for imports: it holds money, or money is coming in.
    pub fn can_pay(&self) -> bool {
        let Some(c) = self.currency else {
            return false;
        };
        let l = &self.lines[c.index()];
        l.stock > 0 || l.rates.inflow + l.rates.cap > 0
    }

    /// Production per step under the current rates (the first piece of the line's plan).
    pub fn production_now(&self, g: GoodId) -> Amount {
        self.lines[g.index()]
            .plan()
            .segs()
            .first()
            .map_or(0, |s| s.prod)
    }

    /// Sets effective capacities and consumption from the current stocks and rates. Called at
    /// every event after inflows and outflows change, so the rates the closed form runs on are
    /// consistent until the next one.
    pub fn recompute(&mut self) {
        if !self.is_storehouse() {
            return;
        }
        let n = self.lines.len();
        let mut uses = vec![0 as Amount; n];
        for &g in &self.order.clone() {
            let i = g.index();
            // Consumption: households plus what this market's recipes take, from goods earlier in
            // the order (their production is already set).
            self.lines[i].rates.cons = self.goods[i].household + uses[i];
            // Capacity: an input with an empty store limits its recipe to what flows in. With the
            // band rule, an input below its target's band limits it to what flows in, so a starved
            // input holds still instead of flickering between empty and a few thousandths, and
            // the limit depends on rates alone, not on a stock that moves between events
            // (overnight choice E44).
            let mut cap = self.goods[i].nominal_cap;
            // Hysteresis (overnight choice E51): a recipe already held back by an input stays
            // held back until the input is back at its target, not just inside its band.
            let was_limited = self.lines[i].rates.cap < self.goods[i].nominal_cap;
            for inp in &self.goods[i].inputs {
                let l = &self.lines[inp.good.index()];
                let short = if self.rules.exporter_band {
                    l.stock < l.target - band(l.target)
                        || (self.rules.input_hysteresis && was_limited && l.stock < l.target)
                } else {
                    l.stock <= 0
                };
                if short {
                    let supply = (l.rates.inflow + l.rates.cap - l.rates.outflow).max(0);
                    let limit = (i128::from(supply) * i128::from(inp.yield_)
                        / i128::from(inp.per.max(1))) as Amount;
                    cap = cap.min(limit);
                }
            }
            self.lines[i].rates.cap = cap.max(0);
            // What sinks above the ceiling (overnight choice E43): a little plenty for goods,
            // upkeep for money.
            let step = self.news_step(g);
            let money_sink = self.rules.money_sink;
            let slack_steps = self.rules.goods_slack_steps.max(0);
            let l = &mut self.lines[i];
            if self.goods[i].currency {
                if money_sink {
                    l.ceiling = l.target.saturating_mul(2);
                    l.rates.slack = UNLIMITED;
                } else {
                    l.ceiling = l.target;
                    l.rates.slack = 0;
                }
            } else {
                l.ceiling = l.target;
                l.rates.slack = slack_steps * step;
            }
            let prod = self.production_now(g);
            for inp in &self.goods[i].inputs {
                let need = (i128::from(prod) * i128::from(inp.per) / i128::from(inp.yield_.max(1)))
                    as Amount;
                uses[inp.good.index()] += need;
            }
        }
    }

    /// The news step of good `g`: what the line turns over per quantum at its target (the target
    /// over `T`, which is `max(c, y)` from the market's recipes and households) over the news
    /// resolution's divisor, at least an item a day (overnight choice E17). It comes from the
    /// target, not from the consumption of the moment, so the grid advertisements are rounded to
    /// stays put when a recipe's output changes (overnight choice E45).
    pub fn news_step(&self, g: GoodId) -> Amount {
        let l = &self.lines[g.index()];
        let turnover = if self.rules.exporter_band {
            l.target / self.t_quanta.max(1) as i64
        } else {
            l.rates.cons
        };
        (turnover / self.rules.news_resolution_div.max(1))
            .max(self.quantum.per_day(self.rules.step_floor_milli_per_day))
            .max(1)
    }

    /// The total outflow per quantum the line can sustain, for flows to take: capacity plus
    /// inflow minus consumption, plus a drain of the surplus over `T` above the target (PLAN.md
    /// §8.4; overnight choice E8). Nothing below the target's band; within the band below the
    /// target, the drain is negative, so the market sends a little less and refills (overnight
    /// choice E45). With [`crate::tunables::Rules::exporter_band`] off, nothing below the target
    /// itself, as round 1 did.
    pub fn exportable(&self, g: GoodId) -> Amount {
        if !self.is_storehouse() {
            return 0;
        }
        let l = &self.lines[g.index()];
        if self.goods[g.index()].currency {
            return 0;
        }
        let floor = if self.rules.exporter_band {
            l.target - band(l.target)
        } else {
            l.target
        };
        if l.stock < floor || l.target <= 0 {
            return 0;
        }
        let x = self.exportable_raw(g);
        if self.rules.exporter_band && !self.rules.hysteresis {
            // Rounded down to the news step, so the stock moving a little between events does not
            // move every flow by a thousandth (overnight choice E45). What is not sent stays in
            // the production regime's slack.
            let step = self.news_step(g);
            x / step * step
        } else {
            x
        }
    }

    /// What flows may take before rounding: capacity plus inflow minus consumption plus the
    /// signed drain of the stock toward the target over `T`, at least zero. Not the band rule:
    /// see [`Market::exportable`].
    pub fn exportable_raw(&self, g: GoodId) -> Amount {
        let l = &self.lines[g.index()];
        let r = l.rates;
        let base = (r.cap + r.inflow - r.cons).max(0);
        (base + (l.stock - l.target) / self.t_quanta as i64).max(0)
    }

    /// What the market advertises it can take in per quantum, signed: need minus inbound, where
    /// need is consumption plus outflow minus own capacity plus a refill of the gap to the target
    /// over `T` (negative above the target, so a glut asks its suppliers for less). Rounded toward
    /// zero to the news resolution, which is the dead band that lets a closed world go quiet.
    /// Nothing above twice the target or while the market can pay for nothing: then it asks for
    /// everything it receives to stop (overnight choice E8).
    pub fn absorbable(&self, g: GoodId) -> Amount {
        if !self.is_storehouse() || self.goods[g.index()].currency {
            return 0;
        }
        let l = &self.lines[g.index()];
        let r = l.rates;
        if l.level() == Level::Glut || !self.can_pay() {
            return -r.inflow;
        }
        let refill = (l.target - l.stock) / self.t_quanta as i64;
        let need = r.cons + r.outflow - r.cap + refill;
        let raw = need - r.inflow;
        let step = self.news_step(g);
        let q = if self.rules.absorb_round_up {
            // Up: what rounding leaves is a small oversupply, which the slack absorbs at the
            // target (overnight choice E43).
            -((-raw).div_euclid(step)) * step
        } else {
            raw / step * step
        };
        // Asking to cut more than arrives means nothing to anyone, so it is not news: a producer
        // that imports nothing advertises zero, however its own exports move (overnight choice
        // E47).
        let q = if self.insolvent() { q.min(0) } else { q };
        if self.rules.clamp_to_inflow {
            q.max(-r.inflow)
        } else {
            q
        }
    }

    /// Whether the market cannot pay for what reaches it (overnight choice E52): its treasury is
    /// below its band and it owes more than its mint and its takings bring in. Such a market asks
    /// for no more of anything, so a broke village imports what it can pay for instead of growing
    /// debts its suppliers never see.
    pub fn insolvent(&self) -> bool {
        let Some(c) = self.currency else {
            return false;
        };
        let l = &self.lines[c.index()];
        let income = l.rates.inflow + l.rates.cap - l.rates.cons;
        self.rules.budget && l.stock < l.target - band(l.target) && self.owed > income.max(0)
    }

    /// The advertised rate with hysteresis (overnight choice E51). The hysteresis is on what the
    /// market asks to receive in all (`prev_total`: what it last advertised plus what was then
    /// arriving), not on the advertised change: while what it needs stays within one news step of
    /// that total, it keeps asking for the same total, and the advertised change is that total
    /// minus what arrives now. Holding the change itself would be wrong once the inflow moves: a
    /// standing "send 41 less" would be read again after the cut and cut a second time.
    pub fn absorbable_held(&self, g: GoodId, prev_total: Option<Amount>) -> Amount {
        let q = self.absorbable(g);
        let (Some(p), true) = (prev_total, self.rules.hysteresis) else {
            return q;
        };
        if !self.is_storehouse() || self.goods[g.index()].currency {
            return q;
        }
        let l = &self.lines[g.index()];
        let r = l.rates;
        if l.level() == Level::Glut || !self.can_pay() {
            return q;
        }
        let refill = (l.target - l.stock) / self.t_quanta as i64;
        let need = r.cons + r.outflow - r.cap + refill;
        let step = self.news_step(g);
        if (need - p).abs() >= step {
            return q;
        }
        // The same total as before: the change is what is missing from it now, rounded as
        // `absorbable` rounds.
        let raw = p - r.inflow;
        let a = if self.rules.absorb_round_up {
            -((-raw).div_euclid(step)) * step
        } else {
            raw / step * step
        };
        let a = if self.insolvent() { a.min(0) } else { a };
        if self.rules.clamp_to_inflow {
            a.max(-r.inflow)
        } else {
            a
        }
    }

    /// What flows may take with hysteresis (overnight choice E51): a market sending `current`
    /// keeps sending it while what it can spare stays within one news step of it, so a stock
    /// moving a little between events does not move every flow it sends.
    pub fn exportable_held(&self, g: GoodId, current: Amount) -> Amount {
        let x = self.exportable(g);
        if !self.rules.hysteresis || current <= 0 || x <= 0 {
            return x;
        }
        if (self.exportable_raw(g) - current).abs() < self.news_step(g) {
            current
        } else {
            x
        }
    }

    // ---- Player trades through the treasury (C14, C23) ----

    /// What a player pays, in whole coins, to buy `items` from the market. `markup` is at least
    /// fair ([`markup_for`]).
    pub fn quote_buy(&self, g: GoodId, items: i64, markup: Ratio) -> Result<Quote, Refusal> {
        if !self.is_storehouse() {
            return Err(Refusal::NotAStorehouse);
        }
        if items <= 0 {
            return Err(Refusal::Nothing);
        }
        if self.cash_refused() {
            return Err(Refusal::CashRefused);
        }
        let q = items * MILLI;
        let s = self.stock(g);
        if q > s {
            return Err(Refusal::NoStock);
        }
        let v = self.goods[g.index()].curve.buy_value(s, q).scaled(markup);
        Ok(Quote {
            items,
            coins: v.coins_ceil(),
        })
    }

    /// What the market pays, in whole coins, for `items` a player sells. `markdown` is at most
    /// fair. Refused when the treasury cannot pay or the store is past twice its target.
    pub fn quote_sell(&self, g: GoodId, items: i64, markdown: Ratio) -> Result<Quote, Refusal> {
        if !self.is_storehouse() {
            return Err(Refusal::NotAStorehouse);
        }
        if items <= 0 {
            return Err(Refusal::Nothing);
        }
        if Some(g) == self.currency {
            return Err(Refusal::NotWanted);
        }
        if self.lines[g.index()].level() == Level::Glut {
            return Err(Refusal::NotWanted);
        }
        let v = self.goods[g.index()]
            .curve
            .sell_value(self.stock(g), items * MILLI)
            .scaled(markdown);
        let coins = v.coins_floor();
        if coins * MILLI > self.treasury() {
            return Err(Refusal::NoMoney);
        }
        Ok(Quote { items, coins })
    }

    /// Applies a player's purchase that [`Market::quote_buy`] priced.
    pub fn apply_buy(&mut self, g: GoodId, q: Quote) {
        let cur = self.currency.expect("a storehouse has a currency");
        let (a, c) = (q.items * MILLI, q.coins * MILLI);
        let l = &mut self.lines[g.index()];
        assert!(l.stock >= a, "player bought more than the stock");
        l.stock -= a;
        l.book.player_out += i128::from(a);
        let m = &mut self.lines[cur.index()];
        m.stock += c;
        m.book.player_in += i128::from(c);
    }

    /// Applies a player's sale that [`Market::quote_sell`] priced.
    pub fn apply_sell(&mut self, g: GoodId, q: Quote) {
        let cur = self.currency.expect("a storehouse has a currency");
        let (a, c) = (q.items * MILLI, q.coins * MILLI);
        let m = &mut self.lines[cur.index()];
        assert!(m.stock >= c, "the treasury paid more than it held");
        m.stock -= c;
        m.book.player_out += i128::from(c);
        let l = &mut self.lines[g.index()];
        l.stock += a;
        l.book.player_in += i128::from(a);
    }

    /// Barter at anchor ratios, for a market that refuses cash: how many whole `want` the market
    /// gives for `give_items` of `give`, rounded down. Anchors do not move with stock, so a
    /// barter loop returns at most what went in.
    pub fn quote_barter(&self, give: GoodId, give_items: i64, want: GoodId) -> i64 {
        let (pg, pw) = (self.anchor(give), self.anchor(want));
        if pw <= 0 || give_items <= 0 {
            return 0;
        }
        let n = (i128::from(give_items) * i128::from(pg) / i128::from(pw)) as i64;
        n.min(self.stock(want) / MILLI)
    }

    /// Applies a barter quoted by [`Market::quote_barter`].
    pub fn apply_barter(&mut self, give: GoodId, give_items: i64, want: GoodId, want_items: i64) {
        let (a, b) = (give_items * MILLI, want_items * MILLI);
        let w = &mut self.lines[want.index()];
        assert!(w.stock >= b);
        w.stock -= b;
        w.book.player_out += i128::from(b);
        let gl = &mut self.lines[give.index()];
        gl.stock += a;
        gl.book.player_in += i128::from(a);
    }

    /// The integer bundle closest to the market's price (PLAN.md §8.5): one item for whole coins
    /// when an item is worth a coin or more, else whole items for one coin. `uses` follow the
    /// stock for what the market sells and the treasury for what it pays (C14). `None` when
    /// refused.
    pub fn offer(&self, g: GoodId, side: OfferSide, markup: Ratio) -> Option<Offer> {
        if !self.is_storehouse() || Some(g) == self.currency {
            return None;
        }
        let s = self.stock(g);
        let curve = &self.goods[g.index()].curve;
        match side {
            OfferSide::MarketSells => {
                if self.cash_refused() || s < MILLI {
                    return None;
                }
                let one = curve.buy_value(s, MILLI).scaled(markup).coins_ceil();
                let (items, coins) = if curve.price_at(s) >= PRICE_ONE || one > 1 {
                    (1, one.max(1))
                } else {
                    // Most items one coin buys, by bisection on the monotone cost.
                    let (mut lo, mut hi) = (1i64, s / MILLI);
                    while lo < hi {
                        let mid = lo + (hi - lo + 1) / 2;
                        let c = curve.buy_value(s, mid * MILLI).scaled(markup).coins_ceil();
                        if c <= 1 { lo = mid } else { hi = mid - 1 }
                    }
                    (lo, 1)
                };
                let uses = (s / (items * MILLI)).min(i64::from(u32::MAX)) as u32;
                (uses > 0).then_some(Offer { items, coins, uses })
            }
            OfferSide::MarketBuys => {
                if self.lines[g.index()].level() == Level::Glut {
                    return None;
                }
                let m = self.treasury();
                let one = curve.sell_value(s, MILLI).scaled(markup).coins_floor();
                let (items, coins) = if one >= 1 {
                    (1, one)
                } else {
                    // Fewest items one coin is paid for.
                    let mut n = 1i64;
                    while curve.sell_value(s, n * MILLI).scaled(markup).coins_floor() < 1 {
                        n *= 2;
                        if n > 1 << 20 {
                            return None;
                        }
                    }
                    let (mut lo, mut hi) = (n / 2 + 1, n);
                    while lo < hi {
                        let mid = lo + (hi - lo) / 2;
                        if curve
                            .sell_value(s, mid * MILLI)
                            .scaled(markup)
                            .coins_floor()
                            >= 1
                        {
                            hi = mid
                        } else {
                            lo = mid + 1
                        }
                    }
                    (lo, 1)
                };
                let uses = (m / (coins * MILLI)).min(i64::from(u32::MAX)) as u32;
                (uses > 0).then_some(Offer { items, coins, uses })
            }
        }
    }

    /// Records a trade players made at a player base (C26): its price there becomes the base's
    /// news.
    pub fn record_base_trade(&mut self, g: GoodId, price: Price, at: Tick) {
        self.base_prices[g.index()] = Some((price, at));
    }

    /// Settles every line over the boundaries in `(from, to]`.
    fn advance_to(&mut self, from: Tick, to: Tick) {
        let n = self.quantum.between(from, to);
        if n == 0 {
            return;
        }
        for l in &mut self.lines {
            l.advance(n);
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<Line>()
            + self
                .goods
                .iter()
                .map(|g| std::mem::size_of::<GoodState>() + g.inputs.capacity() * 24)
                .sum::<usize>()
            + self.order.capacity() * 2
            + self.base_prices.capacity() * 24
    }
}

impl LazyModel for Market {
    /// The market as it stands at the read time: a projected copy.
    type View = Market;

    fn slot_period(&self) -> Option<rtsim_kernel::clock::SlotPeriod> {
        None
    }

    fn settle(&mut self, from: Tick, to: Tick) {
        self.advance_to(from, to);
    }

    fn commit_slot(&mut self, _slot: u64, _rng: &mut rtsim_kernel::rng::SlotRng) {
        unreachable!("markets have no randomness, so no slots")
    }

    fn view(&self, anchor: Tick, at: Tick) -> Market {
        let mut m = self.clone();
        m.advance_to(anchor, at);
        m
    }

    fn next_breakpoint(&self, anchor: Tick) -> Option<Tick> {
        if !self.is_storehouse() {
            return None;
        }
        let first = self.quantum.index(anchor) + 1;
        let mut best: Option<u64> = None;
        let mut take = |b: u64| best = Some(best.map_or(b, |x| x.min(b)));
        for (i, l) in self.lines.iter().enumerate() {
            let c = l.next_change();
            if self.goods[i].currency && self.rules.exporter_band {
                // A treasury drifting between levels decides little: crossing twice its target
                // (cash refused) and the bottom of its band (it pays only what comes in, E50) do
                // (overnight choice E47).
                for th in [l.target.saturating_mul(2), l.target - band(l.target) - 1] {
                    if let Some(k) = l.next_crossing(th) {
                        take(first + k - 1);
                    }
                }
                if self.cut
                    && self.rules.hysteresis
                    && l.stock < l.target
                    && let Some(k) = l.next_crossing(l.target - 1)
                {
                    take(first + k - 1);
                }
            } else if let Some(k) = c.level {
                take(first + k - 1);
            }
            if let Some(k) = c.uncovered {
                // The boundary before the uncovered step, so the owner can cut outflow in time.
                // Step 1 uncovered happens at exactly that breakpoint (the table reschedules
                // before the owner acts on it), and the owner then cuts outflow; booking the next
                // boundary keeps it looked at either way.
                take(first + k.max(2) - 2);
            }
            if self.goods[i].used_here && self.rules.input_hysteresis && l.stock < l.target {
                // An input back at its target lets a held-back recipe run in full (E51).
                if let Some(k) = l.next_crossing(l.target - 1) {
                    take(first + k - 1);
                }
            }
            if self.goods[i].used_here {
                // A change of production regime changes the inputs it eats.
                let plan = l.plan();
                let segs = plan.segs();
                let mut done = 0u64;
                for w in segs.windows(2) {
                    done = done.saturating_add(w[0].len);
                    if w[0].prod != w[1].prod {
                        take(first + done);
                        break;
                    }
                }
            }
        }
        best.map(|b| self.quantum.at(b))
    }
}

impl Encode for Market {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.key);
        w.u8(matches!(self.kind, MarketKind::Base) as u8);
        w.u64(self.t_quanta);
        w.i64(self.owed);
        w.u8(self.cut as u8);
        for l in &self.lines {
            w.i64(l.stock);
            w.i64(l.target);
            w.i64(l.rates.cap);
            w.i64(l.rates.cons);
            w.i64(l.rates.inflow);
            w.i64(l.rates.outflow);
            for v in [
                l.book.produced,
                l.book.consumed,
                l.book.arrived,
                l.book.departed,
                l.book.player_in,
                l.book.player_out,
                l.book.genesis,
                l.book.destroyed,
                l.book.sunk,
                l.book.lump_in,
                l.book.lump_out,
            ] {
                w.raw(&v.to_le_bytes());
            }
        }
        for g in &self.goods {
            w.i64(g.curve.anchor);
            w.i64(g.curve.target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::Rates;
    use crate::tunables::Tunables;
    use rtsim_kernel::lazy::Lazy;
    use rtsim_kernel::rng::{StreamKey, SystemId};

    /// Two goods: wheat (index 0) and the human currency (index 1).
    pub(crate) fn tiny(stock: Amount, treasury: Amount) -> Market {
        let t = Tunables::default();
        let wheat_target = 40 * MILLI;
        let mut wheat = Line::new(stock, wheat_target);
        wheat.rates = Rates {
            cap: t.quantum.per_day(30 * MILLI),
            cons: 0,
            inflow: 0,
            outflow: 0,
            slack: 0,
        };
        let money = Line::new(treasury, 20 * MILLI);
        Market {
            key: 1,
            kind: MarketKind::Storehouse,
            culture: Some(Culture::Human),
            currency: Some(GoodId(1)),
            quantum: t.quantum,
            t_quanta: 48,
            lines: vec![wheat, money],
            goods: vec![
                GoodState {
                    curve: PriceCurve::new(50_000, t.r, wheat_target),
                    household: t.quantum.per_day(20 * MILLI),
                    nominal_cap: t.quantum.per_day(30 * MILLI),
                    inputs: vec![],
                    used_here: false,
                    currency: false,
                },
                GoodState {
                    curve: PriceCurve::flat(PRICE_ONE),
                    household: 0,
                    nominal_cap: 0,
                    inputs: vec![],
                    used_here: false,
                    currency: true,
                },
            ],
            order: vec![GoodId(0), GoodId(1)],
            base_prices: vec![None, None],
            rules: crate::tunables::Rules::default(),
            owed: 0,
            cut: false,
        }
    }

    #[test]
    fn treasury_limits_what_the_market_pays() {
        let m = tiny(40 * MILLI, 2 * MILLI);
        // 40 wheat from the target to twice it are worth 1.25 coins, and each one past that the
        // floor of 0.0125: 200 wheat are worth 3.25 coins, more than the 2 in the treasury.
        assert_eq!(m.quote_sell(GoodId(0), 100, FAIR).unwrap().coins, 2);
        assert_eq!(m.quote_sell(GoodId(0), 200, FAIR), Err(Refusal::NoMoney));
        let q = m.quote_sell(GoodId(0), 20, FAIR).unwrap();
        assert!(q.coins <= 2);
        let o = m.offer(GoodId(0), OfferSide::MarketBuys, FAIR).unwrap();
        assert_eq!(o.coins, 1);
        assert_eq!(o.uses, 2);
    }

    #[test]
    fn cash_refused_above_twice_target() {
        let m = tiny(40 * MILLI, 41 * MILLI);
        assert!(m.cash_refused());
        assert_eq!(m.quote_buy(GoodId(0), 1, FAIR), Err(Refusal::CashRefused));
        assert!(m.quote_barter(GoodId(1), 1, GoodId(0)) > 0);
    }

    #[test]
    fn markup_is_never_better_than_fair() {
        let (b, s) = markup_for(-128, Ratio::new(1, 2));
        assert_eq!(b, Ratio::new(256 + 128, 256));
        assert_eq!(s, Ratio::new(256 - 128, 256));
        assert_eq!(markup_for(100, Ratio::new(1, 2)), (FAIR, FAIR));
    }

    #[test]
    fn breakpoints_are_level_changes() {
        let mut m = tiny(0, 20 * MILLI);
        m.recompute();
        // Empty (up to a quarter of the band, 1,250) to below at the third boundary: 417, 834,
        // 1,251.
        assert_eq!(m.next_breakpoint(Tick(0)), Some(Tick(3000)));
        let mut m = tiny(20 * MILLI, 20 * MILLI);
        m.recompute();
        // 1,250 - 833 = 417 a quantum reaches the target's band (35,000) in 36 quanta.
        let b = m.next_breakpoint(Tick(0)).unwrap();
        let lazy = Lazy::new(m.clone(), Tick(0));
        let mut l2 = lazy.clone();
        let key = StreamKey::new(0, SystemId::named("t"), 0);
        l2.apply(b, key, |_, _| ());
        assert_eq!(l2.model().line(GoodId(0)).level(), Level::At);
        let mut l3 = lazy;
        l3.apply(Tick(b.0 - 1000), key, |_, _| ());
        assert_eq!(l3.model().line(GoodId(0)).level(), Level::Below);
    }
}
