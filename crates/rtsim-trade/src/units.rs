// SPDX-License-Identifier: AGPL-3.0-or-later
//! Units. Conserved quantities are integers (SCALE.md §9); these are the integers.
//!
//! - **Amounts** of goods and money are thousandths of an item or coin ([`MILLI`] per whole one).
//!   A village eating 20 loaves a day eats 833 thousandths an hour, and a caravan flow of 2.5
//!   emeralds a day is exact. Players and the bridge only ever move whole items, so a whole item is
//!   1,000 amounts at the boundary (overnight choice E3).
//! - **Prices** are millionths of a coin per whole item ([`PRICE_ONE`] per coin), so a wheat at a
//!   twentieth of an emerald is 50,000. Prices are not conserved; they are a function of stock.
//! - **Rates** are amounts per accrual quantum ([`Quantum`]): stocks change only at quantum
//!   boundaries, by whole amounts, which is what makes every market's trajectory exact integer
//!   arithmetic and its catch-up a closed form.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

/// Thousandths of an item or a coin: the unit of every stock, flow and treasury.
pub type Amount = i64;

/// Amounts per whole item or coin.
pub const MILLI: Amount = 1_000;

/// Millionths of a coin per whole item.
pub type Price = i64;

/// One coin per item.
pub const PRICE_ONE: Price = 1_000_000;

/// The accrual quantum: stocks move at its boundaries, aligned to tick 0 so a boundary is the
/// same moment for every reader (as the kernel's slots are).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Quantum {
    ticks: u64,
}

impl Quantum {
    /// PLACEHOLDER (overnight choice E4): one in-game hour. Short enough that a caravan leg
    /// (about a fifth of a day between neighbouring villages at walking pace, ESTIMATE until the
    /// phase 0 GameTest measures it) spans several quanta; long enough that a day's need is a
    /// sizeable integer per quantum.
    pub const HOUR: Quantum = Quantum { ticks: 1_000 };

    pub const fn ticks(ticks: u64) -> Self {
        assert!(ticks > 0, "a quantum must be at least one tick");
        Quantum { ticks }
    }

    pub const fn len(self) -> u64 {
        self.ticks
    }

    /// The last boundary at or before `t`: the boundaries `(0, t]` have all been booked.
    pub const fn index(self, t: Tick) -> u64 {
        t.0 / self.ticks
    }

    /// The tick of boundary `b`.
    pub const fn at(self, b: u64) -> Tick {
        Tick(b * self.ticks)
    }

    /// Boundaries booked in `(from, to]`.
    pub const fn between(self, from: Tick, to: Tick) -> u64 {
        if to.0 <= from.0 {
            0
        } else {
            to.0 / self.ticks - from.0 / self.ticks
        }
    }

    /// A rate per day as amounts per quantum, rounded down. The remainder is lost once, at data
    /// compile time, not per step: a day's 20,000 at one-hour quanta is 833 per quantum, 19,992 a
    /// day.
    pub const fn per_day(self, amount_per_day: Amount) -> Amount {
        amount_per_day * self.ticks as i64 / TICKS_PER_DAY as i64
    }

    /// Quanta per day, at least 1.
    pub const fn per_day_count(self) -> u64 {
        let n = TICKS_PER_DAY / self.ticks;
        if n == 0 { 1 } else { n }
    }

    /// Whole quanta in `days` days, at least 1.
    pub const fn days(self, days: u64) -> u64 {
        let n = days * TICKS_PER_DAY / self.ticks;
        if n == 0 { 1 } else { n }
    }

    /// Quanta covering `ticks` ticks, rounded up and at least 1: a travel time.
    pub const fn covering(self, ticks: u64) -> u64 {
        let n = ticks.div_ceil(self.ticks);
        if n == 0 { 1 } else { n }
    }
}

/// `a × b / c` in `i128`, rounded toward negative infinity. For rate and price products that
/// overflow `i64` in the middle.
pub fn mul_div_floor(a: i64, b: i64, c: i64) -> i64 {
    assert!(c != 0);
    let n = i128::from(a) * i128::from(b);
    let q = n.div_euclid(i128::from(c));
    i64::try_from(q).expect("mul_div_floor overflow")
}

/// The value in coins (thousandths) of `amount` thousandths of an item at `price`, rounded down.
pub fn value_floor(amount: Amount, price: Price) -> Amount {
    mul_div_floor(amount, price, PRICE_ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantum_boundaries() {
        let q = Quantum::HOUR;
        assert_eq!(q.between(Tick(0), Tick(999)), 0);
        assert_eq!(q.between(Tick(0), Tick(1000)), 1);
        assert_eq!(q.between(Tick(1000), Tick(1000)), 0);
        assert_eq!(q.between(Tick(999), Tick(2500)), 2);
        assert_eq!(q.per_day(20_000), 833);
        assert_eq!(q.per_day_count(), 24);
        assert_eq!(q.covering(1), 1);
        assert_eq!(q.covering(1001), 2);
    }

    #[test]
    fn values() {
        // 2.5 items at a twentieth of a coin: 0.125 coins = 125 thousandths.
        assert_eq!(value_floor(2_500, 50_000), 125);
        assert_eq!(mul_div_floor(-7, 1, 2), -4);
    }
}
