// SPDX-License-Identifier: AGPL-3.0-or-later
//! The price curve and the player trade integral (PLAN.md §8.4, §8.5).
//!
//! `p(S) = p* (1 + r (1 - S / Sbar))` on `[0, 2 Sbar]`, and the floor `p* (1 - r)` above it. Price
//! is a pure function of stock: nothing integrates, so nothing drifts.
//!
//! **The trade integral.** A player who buys `q` from stock `S` pays `C = ∫_{S-q}^{S} p dS`, and a
//! player who sells is paid `∫_{S}^{S+q} p dS`. Both are exact rationals in `i128` (the curve is
//! linear, so the integral is a quadratic in the stock levels). The buyer pays the ceiling in whole
//! coins and the seller gets the floor. Because the integral is additive over adjacent intervals,
//! any sequence of trades that returns the goods returns the stock to where it was, and the
//! rounding can only take money from the player: no loop of trades makes money (property test in
//! this module and in `tests/`).

use crate::tunables::Ratio;
use crate::units::{Amount, MILLI, PRICE_ONE, Price};

/// Largest stock the exact integral supports: about 1.1 billion items. A market near it is a bug
/// upstream (targets are days of consumption).
pub const MAX_STOCK: Amount = 1 << 40;
/// Largest anchor price: about 4,295 coins per item.
pub const MAX_ANCHOR: Price = 1 << 32;
/// Largest numerator or denominator of `r`, so the integral's `i128` products cannot overflow.
pub const MAX_R_TERM: i64 = 1 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PriceCurve {
    /// `p*`, millionths of a coin per item.
    pub anchor: Price,
    /// `r`, the half-width of the band as a share of `p*`.
    pub r: Ratio,
    /// `Sbar`, in thousandths of an item. Never below 1, so the curve is always defined.
    pub target: Amount,
}

/// An exact non-negative rational value in millionths of a coin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value {
    pub num: i128,
    pub den: i128,
}

impl Value {
    /// Whole coins, rounded up: what a buyer pays.
    pub fn coins_ceil(self) -> i64 {
        let d = self.den * i128::from(PRICE_ONE);
        i64::try_from((self.num + d - 1).div_euclid(d)).expect("price overflow")
    }

    /// Whole coins, rounded down: what a seller gets.
    pub fn coins_floor(self) -> i64 {
        let d = self.den * i128::from(PRICE_ONE);
        i64::try_from(self.num.div_euclid(d)).expect("price overflow")
    }

    /// The value scaled by `num / den` (a markup or markdown).
    pub fn scaled(self, by: Ratio) -> Value {
        Value {
            num: self.num * i128::from(by.num),
            den: self.den * i128::from(by.den),
        }
    }

    pub fn as_f64_coins(self) -> f64 {
        self.num as f64 / self.den as f64 / PRICE_ONE as f64
    }
}

impl PriceCurve {
    pub fn new(anchor: Price, r: Ratio, target: Amount) -> Self {
        assert!(
            (0..=MAX_ANCHOR).contains(&anchor),
            "anchor {anchor} out of range"
        );
        assert!(
            r.num >= 0 && r.num < r.den && r.den <= MAX_R_TERM,
            "r = {}/{} must be in [0, 1) with a small denominator",
            r.num,
            r.den
        );
        PriceCurve {
            anchor,
            r,
            target: target.clamp(1, MAX_STOCK),
        }
    }

    /// A price that does not move with stock: a market's own currency is worth one coin.
    pub fn flat(price: Price) -> Self {
        Self::new(price, Ratio::new(0, 1), 1)
    }

    /// `p(S)`, rounded down to a millionth of a coin.
    pub fn price_at(&self, stock: Amount) -> Price {
        let (n, d) = (i128::from(self.r.num), i128::from(self.r.den));
        let t = i128::from(self.target);
        let s = i128::from(stock.clamp(0, 2 * self.target));
        let p = i128::from(self.anchor) * ((d + n) * t - n * s) / (d * t);
        p as Price
    }

    /// The top of the band, at an empty store.
    pub fn ceiling(&self) -> Price {
        self.price_at(0)
    }

    /// The bottom of the band, at twice the target and above.
    pub fn floor(&self) -> Price {
        self.price_at(2 * self.target)
    }

    /// `∫_a^b p(S) dS` for stock levels `0 <= a <= b`, in millionths of a coin, exact.
    pub fn integral(&self, a: Amount, b: Amount) -> Value {
        assert!(
            0 <= a && a <= b && b <= MAX_STOCK,
            "integral over [{a}, {b}]"
        );
        let (n, d) = (i128::from(self.r.num), i128::from(self.r.den));
        let t = i128::from(self.target);
        let two_t = 2 * t;
        let (a, b) = (i128::from(a), i128::from(b));
        // The sloped part, [a, min(b, 2t)].
        let (a1, b1) = (a.min(two_t), b.min(two_t));
        let (d1, s1) = (b1 - a1, b1 + a1);
        // The floor part, [max(a, 2t), b].
        let d2 = (b - a.max(two_t)).max(0);
        let sloped = 2 * (d + n) * t * d1 - n * d1 * s1;
        let flat = 2 * t * (d - n) * d2;
        let num = i128::from(self.anchor)
            .checked_mul(sloped + flat)
            .expect("trade integral overflow");
        Value {
            num,
            den: 2 * d * t * i128::from(MILLI),
        }
    }

    /// What a buyer of `q` thousandths pays from stock `s`, before rounding.
    pub fn buy_value(&self, s: Amount, q: Amount) -> Value {
        assert!(q >= 0 && q <= s, "buying {q} from a stock of {s}");
        self.integral(s - q, s)
    }

    /// What a seller of `q` thousandths is paid into stock `s`, before rounding.
    pub fn sell_value(&self, s: Amount, q: Amount) -> Value {
        assert!(q >= 0 && s >= 0);
        self.integral(s, s + q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn curve() -> PriceCurve {
        PriceCurve::new(50_000, Ratio::new(3, 4), 40 * MILLI)
    }

    #[test]
    fn band_ends() {
        let c = curve();
        assert_eq!(c.price_at(0), 87_500);
        assert_eq!(c.price_at(40 * MILLI), 50_000);
        assert_eq!(c.price_at(80 * MILLI), 12_500);
        assert_eq!(c.price_at(1_000 * MILLI), 12_500);
    }

    #[test]
    fn integral_of_the_whole_band_is_the_trapezoid() {
        let c = curve();
        // 80 items from 0.0875 to 0.0125 coins: mean 0.05 coins, 4 coins.
        let v = c.integral(0, 80 * MILLI);
        assert_eq!(v.coins_floor(), 4);
        assert_eq!(v.coins_ceil(), 4);
        // Above twice the target the price is flat at the floor: 8 items at 0.0125 = 0.1 coins.
        let w = c.integral(80 * MILLI, 88 * MILLI);
        assert_eq!(w.num * 10, w.den * i128::from(PRICE_ONE));
    }

    proptest! {
        /// Additivity: the integral over [a, c] is the sum over [a, b] and [b, c], exactly.
        #[test]
        fn integral_is_additive(a in 0i64..200_000, x in 0i64..200_000, y in 0i64..200_000, anchor in 1i64..10_000_000, t in 1i64..100_000) {
            let c = PriceCurve::new(anchor, Ratio::new(3, 4), t);
            let (b, e) = (a + x, a + x + y);
            let whole = c.integral(a, e);
            let l = c.integral(a, b);
            let r = c.integral(b, e);
            // Same denominator, so numerators add.
            prop_assert_eq!(l.den, whole.den);
            prop_assert_eq!(l.num + r.num, whole.num);
        }

        /// A buy then a sell of the same amount never pays the player.
        #[test]
        fn round_trip_never_pays(s in 0i64..300_000, q in 0i64..100_000, anchor in 1i64..100_000_000, t in 1i64..100_000) {
            let c = PriceCurve::new(anchor, Ratio::new(3, 4), t);
            let q = q.min(s);
            let paid = c.buy_value(s, q).coins_ceil();
            let got = c.sell_value(s - q, q).coins_floor();
            prop_assert!(got <= paid);
        }
    }
}
