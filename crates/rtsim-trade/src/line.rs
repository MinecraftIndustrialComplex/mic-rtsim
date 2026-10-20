// SPDX-License-Identifier: AGPL-3.0-or-later
//! One good at one market: its stock, its rates and the law that moves it.
//!
//! # The law, per quantum
//!
//! At every quantum boundary, with rates constant since the last event:
//!
//! ```text
//! pre  = S + inflow - outflow - consumption
//! prod = clamp(target - pre, 0, capacity)        // the three production regimes in one line
//! sink = clamp(pre - ceiling, 0, slack)          // what is spent or spoils above the ceiling
//! S'   = pre + prod - sink, and if S' < 0 consumption falls short by -S' and S' = 0
//! ```
//!
//! `prod` is PLAN.md §8.4's three regimes: below the target the market produces at capacity, at
//! the target it produces exactly what keeps it there (matching outflow), above it nothing. Money
//! follows the same law with the treasury target `Mbar` and the mint capacity, so "a market mines
//! while `M < Mbar`" holds and a market at `Mbar` mints exactly what it pays out, with no chatter
//! (overnight choice E6). Outflow is never cut by the law: the market's owner must keep outflow
//! covered, and [`Line::next_change`] reports the boundary before a step that would not be.
//!
//! `sink` is the mirror of production (overnight choice E43): above its ceiling a line loses up
//! to `slack` a quantum. For goods the ceiling is the target and the slack a small share of the
//! line's own consumption (a village with plenty eats a little more and lets a little spoil), so a
//! small oversupply leaves the stock exactly at the target instead of creeping away from it; for
//! money the ceiling is twice the target and the slack unlimited (coin beyond it is spent on
//! upkeep), so a surplus village's treasury stops growing. What sinks is ledgered
//! ([`Book::sunk`]), so conservation stays exact.
//!
//! # The closed form
//!
//! With constant rates the trajectory is at most a handful of linear pieces (fall to the target,
//! one step onto it, sit there; or climb at capacity, step onto the target, rise to the ceiling,
//! sit there sinking; or drain to zero and sit empty). [`Line::plan`] lists them, so
//! [`Line::advance`] over any number of quanta and [`Line::next_change`] cost O(1), whatever the
//! elapsed time (A15).

use crate::units::Amount;

/// Rates per quantum, constant between events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rates {
    /// Production capacity (for the home currency: mint capacity).
    pub cap: Amount,
    /// Consumption: households plus inputs to other lines' recipes.
    pub cons: Amount,
    /// Arrivals from pipelines (flows), summed.
    pub inflow: Amount,
    /// Departures into pipelines (flows and payments), summed.
    pub outflow: Amount,
    /// Most the line loses per quantum above its ceiling ([`Line::ceiling`]): spoilage and plenty
    /// for goods, upkeep for money ([`UNLIMITED`]). Zero: nothing sinks.
    pub slack: Amount,
}

/// A slack with no limit: everything above the ceiling sinks at the next boundary.
pub const UNLIMITED: Amount = 1 << 60;

/// A cumulative count in thousandths. An `i64` holds 9.2 × 10^15 whole items: a market moving a
/// million items a day takes 25,000 years to reach it, so the books are half the size they were as
/// `i128` (overnight choice E58); the ledger still sums them in `i128`.
pub type Count = i64;

/// Cumulative accounting, for the conservation ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Book {
    pub produced: Count,
    pub consumed: Count,
    pub arrived: Count,
    pub departed: Count,
    /// From players (sales to the market, deposits).
    pub player_in: Count,
    /// To players (purchases from the market, loot).
    pub player_out: Count,
    /// Put into the world at its creation (genesis endowment), ledgered.
    pub genesis: Count,
    /// Destroyed (a dragon eats the cargo).
    pub destroyed: Count,
    /// Lost above the ceiling: spoilage and plenty for goods, upkeep for money (overnight choice
    /// E43).
    pub sunk: Count,
    /// Moved in or out by lump transfers the engine makes itself (an individual caravan selling
    /// its cargo, a robbed caravan's stock going to a haven): two-sided, so these net to zero
    /// across the world.
    pub lump_in: Count,
    pub lump_out: Count,
}

/// What a stock level means to news and decisions.
///
/// The levels are bands around the target, not points (overnight choice E44): `At` is the band
/// `target ± target / BAND_DIV`. A stock wandering a little around its target is not news and
/// changes no decision; only leaving the band is a breakpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Out, or nearly: at most a quarter of the band (overnight choice E44), so a starved store
    /// flickering between nothing and a few thousandths is not news.
    Empty,
    /// Below the target's band.
    Below,
    /// Within the target's band.
    At,
    /// Above the band, at most twice the target.
    Above,
    /// More than twice the target: absorbs nothing, and for money, refuses cash.
    Glut,
}

/// The half-width of the target's band is the target over this. PLACEHOLDER (overnight choice
/// E44): an eighth, the same share as the news resolution.
pub const BAND_DIV: Amount = 8;

/// The half-width of `target`'s band.
pub const fn band(target: Amount) -> Amount {
    if target <= 0 { 0 } else { target / BAND_DIV }
}

/// The top of the `Empty` level: a quarter of the band.
pub const fn empty_edge(target: Amount) -> Amount {
    band(target) / 4
}

pub fn level_of(stock: Amount, target: Amount) -> Level {
    let w = band(target);
    if stock <= empty_edge(target) {
        Level::Empty
    } else if stock < target - w {
        Level::Below
    } else if stock <= target + w {
        Level::At
    } else if stock <= target.saturating_mul(2) {
        Level::Above
    } else {
        Level::Glut
    }
}

/// One linear piece of a trajectory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg {
    /// Steps in the piece; `u64::MAX` for forever.
    pub len: u64,
    /// Stock change per step.
    pub delta: Amount,
    /// Production per step.
    pub prod: Amount,
    /// Consumption shortfall per step (the stock is empty).
    pub short: Amount,
    /// Lost above the ceiling per step.
    pub sink: Amount,
}

pub const FOREVER: u64 = u64::MAX;

/// The pieces of a trajectory, in order; the last one is forever.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub start: Amount,
    segs: [Seg; 10],
    n: usize,
}

impl Plan {
    pub fn segs(&self) -> &[Seg] {
        &self.segs[..self.n]
    }
}

/// When a line's trajectory next matters, in steps (quantum boundaries) after now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// The first step after which the stock's [`Level`] differs from now.
    pub level: Option<u64>,
    /// The first step that would leave outflow uncovered (a shortfall bigger than consumption).
    pub uncovered: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Line {
    pub stock: Amount,
    pub target: Amount,
    /// Where sinking starts, at least the target (overnight choice E43).
    pub ceiling: Amount,
    pub rates: Rates,
    pub book: Book,
}

fn ceil_div(a: i128, b: i128) -> i128 {
    debug_assert!(b > 0);
    (a + b - 1).div_euclid(b)
}

impl Line {
    pub fn new(stock: Amount, target: Amount) -> Self {
        Line {
            stock,
            target,
            ceiling: target,
            ..Line::default()
        }
    }

    pub fn level(&self) -> Level {
        level_of(self.stock, self.target)
    }

    /// Net change per step without production: inflow - outflow - consumption.
    pub fn net_without_production(&self) -> Amount {
        self.rates.inflow - self.rates.outflow - self.rates.cons
    }

    /// The trajectory under the current rates.
    pub fn plan(&self) -> Plan {
        let mut plan = Plan {
            start: self.stock,
            segs: [Seg {
                len: 0,
                delta: 0,
                prod: 0,
                short: 0,
                sink: 0,
            }; 10],
            n: 0,
        };
        let push = |p: &mut Plan, len: u64, delta: i128, prod: i128, short: i128, sink: i128| {
            assert!(p.n < p.segs.len(), "line plan too long: {self:?}");
            let to = |x: i128| Amount::try_from(x).expect("line amount overflow");
            p.segs[p.n] = Seg {
                len,
                delta: to(delta),
                prod: to(prod),
                short: to(short),
                sink: to(sink),
            };
            p.n += 1;
        };
        let t = i128::from(self.target);
        let hi = i128::from(self.ceiling.max(self.target));
        let cap = i128::from(self.rates.cap.max(0));
        let slack = i128::from(self.rates.slack.max(0));
        let a = i128::from(self.net_without_production());
        let mut s = i128::from(self.stock.max(0));
        let steps = |x: i128| u64::try_from(x).expect("negative step count");
        loop {
            let pre = s + a;
            if pre > hi {
                // Above the ceiling: nothing is produced, and up to the slack sinks.
                if pre - slack >= hi {
                    let d = a - slack;
                    if d >= 0 {
                        push(&mut plan, FOREVER, d, 0, 0, slack);
                        break;
                    }
                    // Falling with the whole slack sinking: every step that stays at or above the
                    // ceiling. At least one, since s + d >= hi.
                    let count = (s - hi) / (-d);
                    push(&mut plan, steps(count), d, 0, 0, slack);
                    s += count * d;
                    continue;
                }
                // One step onto the ceiling, sinking just the excess.
                push(&mut plan, 1, hi - s, 0, 0, pre - hi);
                s = hi;
                if a >= 0 && a <= slack {
                    // The upper fixed point: the sink matches the net gain.
                    push(&mut plan, FOREVER, 0, 0, 0, a);
                    break;
                }
                continue;
            }
            if pre >= t {
                // Between the target and the ceiling: nothing is produced or sunk.
                if a == 0 {
                    push(&mut plan, FOREVER, 0, 0, 0, 0);
                    break;
                }
                let count = if a > 0 { (hi - s) / a } else { (s - t) / (-a) };
                push(&mut plan, steps(count), a, 0, 0, 0);
                s += count * a;
                continue;
            }
            if pre >= t - cap {
                // One step onto the target, producing just enough.
                push(&mut plan, 1, t - s, t - pre, 0, 0);
                s = t;
                if a <= 0 && a >= -cap {
                    // The fixed point: production matches the net drain.
                    push(&mut plan, FOREVER, 0, -a, 0, 0);
                    break;
                }
                continue;
            }
            // Below: production at capacity.
            let d = a + cap;
            if pre + cap < 0 {
                // This step empties the store; consumption (and, if it is not enough, outflow)
                // falls short.
                push(&mut plan, 1, -s, cap, -(pre + cap), 0);
                s = 0;
                if d < 0 {
                    push(&mut plan, FOREVER, 0, cap, -d, 0);
                    break;
                }
                continue;
            }
            if d > 0 {
                // Climbing toward the target: every step that stays below the onto-target band.
                let count = ceil_div(t - cap - a - s, d);
                push(&mut plan, steps(count), d, cap, 0, 0);
                s += count * d;
                continue;
            }
            if d == 0 {
                push(&mut plan, FOREVER, 0, cap, 0, 0);
                break;
            }
            // Draining at capacity: every step that leaves the stock non-negative.
            let count = s / (-d);
            push(&mut plan, steps(count), d, cap, 0, 0);
            s += count * d;
        }
        plan
    }

    /// Books `n` quantum boundaries under the current rates.
    pub fn advance(&mut self, n: u64) {
        if n == 0 {
            return;
        }
        let plan = self.plan();
        let mut left = n;
        let r = self.rates;
        for seg in plan.segs() {
            let k = seg.len.min(left);
            let k128 = i128::from(k);
            self.stock = Amount::try_from(i128::from(self.stock) + i128::from(seg.delta) * k128)
                .expect("stock overflow");
            let count =
                |rate: Amount| Count::try_from(i128::from(rate) * k128).expect("book overflow");
            self.book.produced += count(seg.prod);
            self.book.sunk += count(seg.sink);
            self.book.consumed += count(r.cons - seg.short);
            self.book.arrived += count(r.inflow);
            self.book.departed += count(r.outflow);
            left -= k;
            if left == 0 {
                break;
            }
        }
        debug_assert_eq!(left, 0, "plan ended before {n} steps");
        debug_assert!(self.stock >= 0);
    }

    /// The stock after `n` more boundaries, without changing anything.
    pub fn stock_after(&self, n: u64) -> Amount {
        let mut l = *self;
        l.advance(n);
        l.stock
    }

    /// When the level next changes, and when outflow would first go uncovered.
    pub fn next_change(&self) -> Change {
        let plan = self.plan();
        let start_level = self.level();
        let t = self.target;
        let mut out = Change::default();
        let mut done: u64 = 0;
        let mut s = self.stock;
        for seg in plan.segs() {
            if out.uncovered.is_none() && seg.short > self.rates.cons {
                out.uncovered = Some(done + 1);
            }
            if out.level.is_none()
                && let Some(j) = first_level_change(s, seg.delta, seg.len, t, start_level)
            {
                out.level = Some(done.saturating_add(j));
            }
            if out.level.is_some() && out.uncovered.is_some() {
                break;
            }
            if seg.len == FOREVER {
                break;
            }
            s += seg.delta * seg.len as i64;
            done += seg.len;
        }
        out
    }
}

impl Line {
    /// The first step after which whether the stock is above `threshold` differs from now: for a
    /// currency line, whose only level that changes a decision is twice its target (cash refused,
    /// overnight choice E47).
    pub fn next_crossing(&self, threshold: Amount) -> Option<u64> {
        let plan = self.plan();
        let above = self.stock > threshold;
        let (th, mut s) = (i128::from(threshold), i128::from(self.stock));
        let mut done: u64 = 0;
        for seg in plan.segs() {
            let d = i128::from(seg.delta);
            let j = if above && d < 0 {
                Some(ceil_div(s - th, -d))
            } else if !above && d > 0 {
                Some(ceil_div(th + 1 - s, d))
            } else {
                None
            };
            if let Some(j) = j.and_then(|j| u64::try_from(j.max(1)).ok())
                && j <= seg.len
            {
                return Some(done.saturating_add(j));
            }
            if seg.len == FOREVER {
                return None;
            }
            s += d * i128::from(seg.len);
            done += seg.len;
        }
        None
    }
}

/// The first step `j` in `1..=len` at which `level_of(s + j·delta)` differs from `from`.
fn first_level_change(s: Amount, delta: Amount, len: u64, t: Amount, from: Level) -> Option<u64> {
    if len == 0 {
        return None;
    }
    // The level after the first step may already differ (a one-step piece onto the target).
    let after1 = s + delta;
    if level_of(after1, t) != from {
        return Some(1);
    }
    if delta == 0 {
        return None;
    }
    let w = i128::from(band(t));
    let z = i128::from(empty_edge(t));
    let (s, d, t) = (i128::from(s), i128::from(delta), i128::from(t));
    // Smallest j with s + j·d crossing the edge that ends `from`'s band in d's direction.
    let j = if d > 0 {
        match from {
            Level::Empty => Some(ceil_div(z + 1 - s, d)),
            Level::Below => Some(ceil_div(t - w - s, d)),
            Level::At => Some(ceil_div(t + w + 1 - s, d)),
            Level::Above => Some(ceil_div(2 * t + 1 - s, d)),
            Level::Glut => None,
        }
    } else {
        let e = -d;
        match from {
            Level::Glut => Some(ceil_div(s - 2 * t, e)),
            Level::Above => Some(ceil_div(s - t - w, e)),
            Level::At => Some((s - t + w).div_euclid(e) + 1),
            Level::Below => Some(ceil_div(s - z, e)),
            Level::Empty => None,
        }
    }?;
    let j = u64::try_from(j.max(1)).ok()?;
    (j <= len).then_some(j)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The per-quantum law, one step at a time: the reference the closed form must match.
    fn step(l: &mut Line) -> Amount {
        let r = l.rates;
        let pre = l.stock + r.inflow - r.outflow - r.cons;
        let prod = (l.target - pre).clamp(0, r.cap.max(0));
        let sink = (pre - l.ceiling.max(l.target)).clamp(0, r.slack.max(0));
        l.book.sunk += sink;
        let mut post = pre + prod - sink;
        let mut short = 0;
        if post < 0 {
            short = -post;
            post = 0;
        }
        l.book.produced += prod;
        l.book.consumed += r.cons - short;
        l.book.arrived += r.inflow;
        l.book.departed += r.outflow;
        l.stock = post;
        short
    }

    fn line(stock: i64, target: i64, cap: i64, cons: i64, inflow: i64, outflow: i64) -> Line {
        Line {
            stock,
            target,
            ceiling: target,
            rates: Rates {
                cap,
                cons,
                inflow,
                outflow,
                slack: 0,
            },
            book: Book::default(),
        }
    }

    #[test]
    fn regimes() {
        // Below: produce at capacity, then step onto the target and match.
        let mut l = line(0, 100, 30, 10, 0, 0);
        l.advance(4);
        assert_eq!(l.stock, 80);
        l.advance(1);
        assert_eq!(l.stock, 100);
        l.advance(10);
        assert_eq!(l.stock, 100);
        assert_eq!(l.book.produced, 4 * 30 + 30 + 10 * 10);
        // Above with a glut inflow: no production, the stock rises.
        let mut g = line(100, 100, 30, 10, 25, 0);
        g.advance(4);
        assert_eq!(g.stock, 160);
        assert_eq!(g.book.produced, 0);
        // Not enough capacity: drains to zero, then consumption falls short.
        let mut e = line(50, 100, 5, 20, 0, 0);
        e.advance(10);
        assert_eq!(e.stock, 0);
        assert_eq!(e.book.consumed, 50 + 10 * 5);
    }

    #[test]
    fn sinks_above_the_ceiling() {
        // A small oversupply at the target sinks: the stock stays put.
        let mut l = line(100, 100, 30, 10, 14, 0);
        l.rates.slack = 5;
        l.advance(10);
        assert_eq!(l.stock, 100);
        assert_eq!(l.book.sunk, 40);
        // More than the slack: the stock rises by the difference.
        let mut l = line(100, 100, 30, 10, 20, 0);
        l.rates.slack = 5;
        l.advance(4);
        assert_eq!(l.stock, 120);
        // Money: unlimited slack above twice the target holds the treasury at the ceiling.
        let mut m = line(150, 100, 0, 0, 40, 0);
        m.ceiling = 200;
        m.rates.slack = UNLIMITED;
        m.advance(10);
        assert_eq!(m.stock, 200);
        assert_eq!(m.book.sunk, 150 + 400 - 200);
        assert_eq!(m.level(), Level::Above);
    }

    #[test]
    fn breakpoints() {
        let l = line(0, 100, 30, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(1)); // Empty to Below at once.
        let l = line(20, 100, 30, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(4)); // 40, 60, 80, then 100.
        // Down into the target's band (88 to 112) at 110.
        let l = line(150, 100, 0, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(4));
        // Outflow bigger than everything coming in: uncovered once the stock is gone.
        let l = line(35, 100, 0, 0, 0, 10);
        let c = l.next_change();
        assert_eq!(c.level, Some(4)); // 25, 15, 5, then 0 would need 5 more.
        assert_eq!(c.uncovered, Some(4));
    }

    proptest! {
        /// The closed form matches the step-by-step law exactly, stock and books, for any
        /// number of steps and any split of them.
        #[test]
        fn closed_form_matches_steps(stock in 0i64..10_000, target in 0i64..5_000, cap in 0i64..500, cons in 0i64..500, inflow in 0i64..500, outflow in 0i64..500, n in 0u64..200, cut in 0u64..200, slack in prop_oneof![Just(0i64), 0i64..300, Just(UNLIMITED)], above in prop_oneof![Just(0i64), 0i64..5_000]) {
            let mut base = line(stock, target, cap, cons, inflow, outflow);
            base.rates.slack = slack;
            base.ceiling = target + above;
            let mut stepped = base;
            let mut uncovered_at = None;
            for i in 0..n {
                let short = step(&mut stepped);
                if short > cons && uncovered_at.is_none() {
                    uncovered_at = Some(i + 1);
                }
            }
            let mut whole = base;
            whole.advance(n);
            prop_assert_eq!(whole, stepped);
            let cut = cut.min(n);
            let mut split = base;
            split.advance(cut);
            split.advance(n - cut);
            prop_assert_eq!(split, stepped);
            // next_change agrees with stepping.
            let c = base.next_change();
            if let Some(u) = uncovered_at {
                prop_assert_eq!(c.uncovered, Some(u));
            }
            let mut walk = base;
            let lv = base.level();
            let mut first = None;
            for i in 1..=n {
                step(&mut walk);
                if walk.level() != lv {
                    first = Some(i);
                    break;
                }
            }
            if let Some(f) = first {
                prop_assert_eq!(c.level, Some(f));
            } else if let Some(l) = c.level {
                prop_assert!(l > n);
            }
        }
    }
}
