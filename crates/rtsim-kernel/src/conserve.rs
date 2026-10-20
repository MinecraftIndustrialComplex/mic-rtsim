// SPDX-License-Identifier: AGPL-3.0-or-later
//! Integer conservation (SCALE.md §9, C13): counts, goods and money are integers, every transfer is
//! two-sided, and fractional rates carry their remainder forward so totals stay exact over any time
//! span.
//!
//! - [`Totals`]: a holder's conserved quantities, as (kind, amount) pairs.
//! - [`Ledger`]: what the world's totals must be, given every mint and burn. Debug builds check
//!   observed totals against it.
//! - [`transfer`]: the one way to move a quantity between holders.
//! - [`Diffuser`]: a rate in 32.32 fixed point that emits whole units and keeps the fraction.
//!   Fixed point, not `f64`, because then emitting over `a` ticks and then `b` ticks gives exactly
//!   what emitting over `a + b` gives, which a lazy catch-up needs (SCALE.md §4).

use std::collections::BTreeMap;
use std::fmt;

/// What is conserved: an item, a currency, a head count of one species. Opaque to the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuantityKind(pub u32);

/// Conserved quantities held by one thing (or summed over many), sorted by kind, no zero entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    entries: Vec<(QuantityKind, i128)>,
}

impl Totals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, kind: QuantityKind, amount: i128) {
        if amount == 0 {
            return;
        }
        match self.entries.binary_search_by_key(&kind, |e| e.0) {
            Ok(i) => {
                self.entries[i].1 += amount;
                if self.entries[i].1 == 0 {
                    self.entries.remove(i);
                }
            }
            Err(i) => self.entries.insert(i, (kind, amount)),
        }
    }

    pub fn get(&self, kind: QuantityKind) -> i128 {
        self.entries
            .binary_search_by_key(&kind, |e| e.0)
            .map_or(0, |i| self.entries[i].1)
    }

    pub fn merge(&mut self, other: &Totals) {
        for &(k, a) in &other.entries {
            self.add(k, a);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (QuantityKind, i128)> + '_ {
        self.entries.iter().copied()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Anything that holds conserved quantities reports them here, so property tests and debug
/// checks can sum a world.
pub trait Conserved {
    fn add_totals(&self, out: &mut Totals);

    fn totals(&self) -> Totals {
        let mut t = Totals::new();
        self.add_totals(&mut t);
        t
    }
}

/// The book of the world: totals change only by recorded mints (production, births, wages) and
/// burns (consumption, deaths, decay).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    expected: BTreeMap<QuantityKind, i128>,
    minted: BTreeMap<QuantityKind, i128>,
    burned: BTreeMap<QuantityKind, i128>,
}

/// Observed totals disagree with the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imbalance {
    pub kind: QuantityKind,
    pub expected: i128,
    pub observed: i128,
}

impl fmt::Display for Imbalance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "conservation broken for {:?}: ledger expects {}, the world holds {}",
            self.kind, self.expected, self.observed
        )
    }
}

impl std::error::Error for Imbalance {}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mint(&mut self, kind: QuantityKind, amount: i128) {
        debug_assert!(amount >= 0, "mint of {amount}");
        *self.expected.entry(kind).or_default() += amount;
        *self.minted.entry(kind).or_default() += amount;
    }

    pub fn burn(&mut self, kind: QuantityKind, amount: i128) {
        debug_assert!(amount >= 0, "burn of {amount}");
        *self.expected.entry(kind).or_default() -= amount;
        *self.burned.entry(kind).or_default() += amount;
    }

    /// A signed change: positive mints, negative burns.
    pub fn record(&mut self, kind: QuantityKind, delta: i128) {
        if delta >= 0 {
            self.mint(kind, delta);
        } else {
            self.burn(kind, -delta);
        }
    }

    pub fn expected(&self, kind: QuantityKind) -> i128 {
        self.expected.get(&kind).copied().unwrap_or(0)
    }

    pub fn minted(&self, kind: QuantityKind) -> i128 {
        self.minted.get(&kind).copied().unwrap_or(0)
    }

    pub fn burned(&self, kind: QuantityKind) -> i128 {
        self.burned.get(&kind).copied().unwrap_or(0)
    }

    /// Checks every kind the ledger or the observation mentions.
    pub fn check(&self, observed: &Totals) -> Result<(), Imbalance> {
        for (&kind, &expected) in &self.expected {
            let got = observed.get(kind);
            if got != expected {
                return Err(Imbalance {
                    kind,
                    expected,
                    observed: got,
                });
            }
        }
        for (kind, got) in observed.iter() {
            if !self.expected.contains_key(&kind) {
                return Err(Imbalance {
                    kind,
                    expected: 0,
                    observed: got,
                });
            }
        }
        Ok(())
    }

    /// [`Ledger::check`] in debug builds; free in release builds.
    #[track_caller]
    pub fn debug_check(&self, observed: &Totals) {
        if cfg!(debug_assertions)
            && let Err(e) = self.check(observed)
        {
            panic!("{e}");
        }
    }
}

/// The source holds less than the transfer asks for. Nothing moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insufficient {
    pub wanted: i64,
    pub held: i64,
}

/// Moves `amount` from one holder's balance to another's: both sides or neither.
#[inline]
pub fn transfer(from: &mut i64, to: &mut i64, amount: i64) -> Result<(), Insufficient> {
    debug_assert!(amount >= 0, "negative transfer {amount}");
    if *from < amount {
        return Err(Insufficient {
            wanted: amount,
            held: *from,
        });
    }
    *from -= amount;
    *to = to.checked_add(amount).expect("balance overflow");
    Ok(())
}

/// Moves up to `amount`, as much as the source holds; returns what moved.
#[inline]
pub fn transfer_up_to(from: &mut i64, to: &mut i64, amount: i64) -> i64 {
    let moved = amount.min(*from).max(0);
    *from -= moved;
    *to += moved;
    moved
}

/// One unit in 32.32 fixed point.
pub const FIXED_ONE: i64 = 1 << 32;

/// A rate that produces whole units, carrying the fractional remainder (error diffusion).
///
/// The carry is always in `[0, 2^32)`, and `emit` is `floor((carry + rate·dt) / 2^32)` with the
/// remainder kept, so `emit(a) + emit(b) == emit(a + b)` exactly for any split: integer floor
/// division composes. A float accumulator would not, and lazy and eager catch-up would drift apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Diffuser {
    /// Units per tick, 32.32 fixed point. May be negative (a drain).
    rate: i64,
    /// Fraction carried forward, in 1/2^32 units.
    carry: u32,
}

impl Diffuser {
    pub const fn from_fixed(rate_per_tick: i64) -> Self {
        Self {
            rate: rate_per_tick,
            carry: 0,
        }
    }

    /// A rate with a carry restored from a page.
    pub const fn with_carry(rate_per_tick: i64, carry: u32) -> Self {
        Self {
            rate: rate_per_tick,
            carry,
        }
    }

    /// `units` every `ticks`, rounded to the nearest 2^-32 per tick. The rounding is the only
    /// inexact step, and it happens once, here.
    pub fn per(units: i64, ticks: u64) -> Self {
        assert!(ticks > 0);
        let num = i128::from(units) << 32;
        let den = i128::from(ticks);
        let q = (num + num.signum() * den / 2) / den;
        Self::from_fixed(i64::try_from(q).expect("rate out of range"))
    }

    pub fn from_f64_per_tick(rate: f64) -> Self {
        Self::from_fixed((rate * FIXED_ONE as f64).round() as i64)
    }

    pub const fn rate_fixed(&self) -> i64 {
        self.rate
    }

    pub const fn carry(&self) -> u32 {
        self.carry
    }

    /// Changes the rate and keeps the carry, so no fraction is lost at a breakpoint.
    pub fn set_rate_fixed(&mut self, rate: i64) {
        self.rate = rate;
    }

    /// Whole units produced over `dt` ticks; the fraction stays for next time.
    #[inline]
    pub fn emit(&mut self, dt: u64) -> i64 {
        let total = i128::from(self.carry) + i128::from(self.rate) * i128::from(dt);
        self.carry = total.rem_euclid(1 << 32) as u32;
        i64::try_from(total.div_euclid(1 << 32)).expect("emission overflow")
    }

    /// What [`Diffuser::emit`] would return, without changing anything: for read-time views.
    #[inline]
    pub fn peek(&self, dt: u64) -> i64 {
        let total = i128::from(self.carry) + i128::from(self.rate) * i128::from(dt);
        i64::try_from(total.div_euclid(1 << 32)).expect("emission overflow")
    }

    /// Ticks until the emitted total first reaches `units` (> 0) for a positive rate; `None` if it
    /// never does. This is how a stock computes its breakpoint in O(1).
    pub fn ticks_until(&self, units: i64) -> Option<u64> {
        if units <= 0 {
            return Some(0);
        }
        if self.rate <= 0 {
            return None;
        }
        // Smallest dt with carry + rate·dt >= units·2^32.
        let need = (i128::from(units) << 32) - i128::from(self.carry);
        let rate = i128::from(self.rate);
        u64::try_from((need + rate - 1) / rate).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GRAIN: QuantityKind = QuantityKind(1);
    const COIN: QuantityKind = QuantityKind(2);

    #[test]
    fn totals_add_and_cancel() {
        let mut t = Totals::new();
        t.add(COIN, 5);
        t.add(GRAIN, 3);
        t.add(COIN, -5);
        assert_eq!(t.get(COIN), 0);
        assert_eq!(t.iter().collect::<Vec<_>>(), vec![(GRAIN, 3)]);
    }

    #[test]
    fn ledger_catches_a_leak() {
        let mut l = Ledger::new();
        l.mint(GRAIN, 10);
        l.burn(GRAIN, 4);
        let mut world = Totals::new();
        world.add(GRAIN, 6);
        assert!(l.check(&world).is_ok());
        world.add(GRAIN, 1);
        assert_eq!(l.check(&world).unwrap_err().observed, 7);
        let mut stray = Totals::new();
        stray.add(GRAIN, 6);
        stray.add(COIN, 1);
        assert!(l.check(&stray).is_err());
    }

    #[test]
    fn transfer_is_all_or_nothing() {
        let (mut a, mut b) = (5i64, 0i64);
        assert!(transfer(&mut a, &mut b, 6).is_err());
        assert_eq!((a, b), (5, 0));
        transfer(&mut a, &mut b, 5).unwrap();
        assert_eq!((a, b), (0, 5));
        assert_eq!(transfer_up_to(&mut b, &mut a, 9), 5);
    }

    #[test]
    fn diffuser_rates_are_exact_over_a_period() {
        // 7 units a day: exactly 7 per 24,000 ticks, however the day is cut.
        let mut d = Diffuser::per(7, 24_000);
        let mut sum = 0;
        for _ in 0..24_000 {
            sum += d.emit(1);
        }
        assert!((6..=7).contains(&sum));
        // Rounding the rate to 2^-32 per tick costs at most 24e6 / 2^33 = 0.003 units over 1,000
        // days, so the total is 7,000 or one short of it.
        let mut d = Diffuser::per(7, 24_000);
        assert!((6_999..=7_000).contains(&d.emit(24_000 * 1000)));
    }

    #[test]
    fn ticks_until_matches_emission() {
        let d = Diffuser::per(3, 1000);
        let n = d.ticks_until(10).unwrap();
        assert!(d.peek(n) >= 10);
        assert!(d.peek(n - 1) < 10);
        assert_eq!(Diffuser::per(-3, 1000).ticks_until(1), None);
    }

    proptest! {
        /// emit composes exactly: any partition of a span emits what the whole span emits, and the
        /// carry ends the same.
        #[test]
        fn diffuser_composes(rate in -(1i64 << 40)..(1i64 << 40), cuts in prop::collection::vec(0u64..1_000_000, 1..20)) {
            let mut split = Diffuser::from_fixed(rate);
            let mut sum = 0i64;
            for &c in &cuts {
                sum += split.emit(c);
            }
            let mut whole = Diffuser::from_fixed(rate);
            let total = whole.emit(cuts.iter().sum());
            prop_assert_eq!(sum, total);
            prop_assert_eq!(split, whole);
        }

        /// Random transfers between holders never change the total.
        #[test]
        fn transfers_conserve(start in prop::collection::vec(0i64..1000, 2..10), moves in prop::collection::vec((any::<u8>(), any::<u8>(), 0i64..500), 0..200)) {
            let mut h = start.clone();
            let n = h.len();
            for (a, b, amt) in moves {
                let (a, b) = (a as usize % n, b as usize % n);
                if a == b { continue; }
                let (x, y) = if a < b {
                    let (l, r) = h.split_at_mut(b);
                    (&mut l[a], &mut r[0])
                } else {
                    let (l, r) = h.split_at_mut(a);
                    (&mut r[0], &mut l[b])
                };
                let _ = transfer(x, y, amt);
                prop_assert!(h.iter().all(|&v| v >= 0));
            }
            prop_assert_eq!(h.iter().sum::<i64>(), start.iter().sum::<i64>());
        }
    }
}
