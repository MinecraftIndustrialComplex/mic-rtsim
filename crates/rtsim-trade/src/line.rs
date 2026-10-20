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
//! S'   = pre + prod, and if S' < 0 consumption falls short by -S' and S' = 0
//! ```
//!
//! `prod` is PLAN.md §8.4's three regimes: below the target the market produces at capacity, at
//! the target it produces exactly what keeps it there (matching outflow), above it nothing. Money
//! follows the same law with the treasury target `Mbar` and the mint capacity, so "a market mines
//! while `M < Mbar`" holds and a market at `Mbar` mints exactly what it pays out, with no chatter
//! (overnight choice E6). Outflow is never cut by the law: the market's owner must keep outflow
//! covered, and [`Line::next_change`] reports the boundary before a step that would not be.
//!
//! # The closed form
//!
//! With constant rates the trajectory is at most a handful of linear pieces (fall to the target,
//! one step onto it, sit there; or climb at capacity, step onto the target, rise above it; or drain
//! to zero and sit empty). [`Line::plan`] lists them, so [`Line::advance`] over any number of
//! quanta and [`Line::next_change`] cost O(1), whatever the elapsed time (A15).

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
}

/// Cumulative accounting, for the conservation ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Book {
    pub produced: i128,
    pub consumed: i128,
    pub arrived: i128,
    pub departed: i128,
    /// From players (sales to the market, deposits).
    pub player_in: i128,
    /// To players (purchases from the market, loot).
    pub player_out: i128,
    /// Put into the world at its creation (genesis endowment), ledgered.
    pub genesis: i128,
    /// Destroyed (a dragon eats the cargo).
    pub destroyed: i128,
    /// Moved in or out by lump transfers the engine makes itself (an individual caravan selling
    /// its cargo, a robbed caravan's stock going to a haven): two-sided, so these net to zero
    /// across the world.
    pub lump_in: i128,
    pub lump_out: i128,
}

/// What a stock level means to news and decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Empty,
    Below,
    At,
    /// Above the target, at most twice it.
    Above,
    /// More than twice the target: absorbs nothing, and for money, refuses cash.
    Glut,
}

pub fn level_of(stock: Amount, target: Amount) -> Level {
    if stock <= 0 {
        Level::Empty
    } else if stock < target {
        Level::Below
    } else if stock == target {
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
}

pub const FOREVER: u64 = u64::MAX;

/// The pieces of a trajectory, in order; the last one is forever.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub start: Amount,
    segs: [Seg; 8],
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
            }; 8],
            n: 0,
        };
        let push = |p: &mut Plan, len: u64, delta: Amount, prod: Amount, short: Amount| {
            assert!(p.n < p.segs.len(), "line plan too long: {self:?}");
            p.segs[p.n] = Seg {
                len,
                delta,
                prod,
                short,
            };
            p.n += 1;
        };
        let t = i128::from(self.target);
        let cap = i128::from(self.rates.cap.max(0));
        let a = i128::from(self.net_without_production());
        let mut s = i128::from(self.stock.max(0));
        let to = |x: i128| Amount::try_from(x).expect("line amount overflow");
        let steps = |x: i128| u64::try_from(x).expect("negative step count");
        loop {
            let pre = s + a;
            if pre >= t {
                // Above (or onto) the target: nothing is produced.
                if a >= 0 {
                    push(&mut plan, FOREVER, to(a), 0, 0);
                    break;
                }
                // Falling: every step whose result stays at or above the target. At least one,
                // since pre >= t means s - t >= -a.
                let count = (s - t) / (-a);
                push(&mut plan, steps(count), to(a), 0, 0);
                s += count * a;
                continue;
            }
            if pre >= t - cap {
                // One step onto the target, producing just enough.
                push(&mut plan, 1, to(t - s), to(t - pre), 0);
                s = t;
                if a <= 0 && a >= -cap {
                    // The fixed point: production matches the net drain.
                    push(&mut plan, FOREVER, 0, to(-a), 0);
                    break;
                }
                continue;
            }
            // Below: production at capacity.
            let d = a + cap;
            if pre + cap < 0 {
                // This step empties the store; consumption (and, if it is not enough, outflow)
                // falls short.
                push(&mut plan, 1, to(-s), to(cap), to(-(pre + cap)));
                s = 0;
                if d < 0 {
                    push(&mut plan, FOREVER, 0, to(cap), to(-d));
                    break;
                }
                continue;
            }
            if d > 0 {
                // Climbing toward the target: every step that stays below the onto-target band.
                let count = ceil_div(t - cap - a - s, d);
                push(&mut plan, steps(count), to(d), to(cap), 0);
                s += count * d;
                continue;
            }
            if d == 0 {
                push(&mut plan, FOREVER, 0, to(cap), 0);
                break;
            }
            // Draining at capacity: every step that leaves the stock non-negative.
            let count = s / (-d);
            push(&mut plan, steps(count), to(d), to(cap), 0);
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
            self.book.produced += i128::from(seg.prod) * k128;
            self.book.consumed += i128::from(r.cons - seg.short) * k128;
            self.book.arrived += i128::from(r.inflow) * k128;
            self.book.departed += i128::from(r.outflow) * k128;
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
    let (s, d, t) = (i128::from(s), i128::from(delta), i128::from(t));
    // Smallest j with s + j·d crossing the threshold that ends `from`'s band in d's direction.
    let j = if d > 0 {
        match from {
            Level::Empty | Level::At => Some(1),
            Level::Below => Some(ceil_div(t - s, d)),
            Level::Above => Some(ceil_div(2 * t + 1 - s, d)),
            Level::Glut => None,
        }
    } else {
        let e = -d;
        match from {
            Level::Glut => Some(ceil_div(s - 2 * t, e)),
            Level::Above => Some(ceil_div(s - t, e)),
            Level::At => Some(1),
            Level::Below => Some(ceil_div(s, e)),
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
        let mut post = pre + prod;
        let mut short = 0;
        if post < 0 {
            short = -post;
            post = 0;
        }
        l.book.produced += i128::from(prod);
        l.book.consumed += i128::from(r.cons - short);
        l.book.arrived += i128::from(r.inflow);
        l.book.departed += i128::from(r.outflow);
        l.stock = post;
        short
    }

    fn line(stock: i64, target: i64, cap: i64, cons: i64, inflow: i64, outflow: i64) -> Line {
        Line {
            stock,
            target,
            rates: Rates {
                cap,
                cons,
                inflow,
                outflow,
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
    fn breakpoints() {
        let l = line(0, 100, 30, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(1)); // Empty to Below at once.
        let l = line(20, 100, 30, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(4)); // 40, 60, 80, then 100.
        let l = line(150, 100, 0, 10, 0, 0);
        assert_eq!(l.next_change().level, Some(5));
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
        fn closed_form_matches_steps(stock in 0i64..10_000, target in 0i64..5_000, cap in 0i64..500, cons in 0i64..500, inflow in 0i64..500, outflow in 0i64..500, n in 0u64..200, cut in 0u64..200) {
            let base = line(stock, target, cap, cons, inflow, outflow);
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
