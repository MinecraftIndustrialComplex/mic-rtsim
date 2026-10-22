// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exact rationals for the solve (GLASSBOX.md §3.7): i64 over i64 with i128 intermediates,
//! reduced by gcd, plus one positive infinity for "unlimited".
//!
//! **Rounding.** Every operation is exact while its result fits. When a reduced result does not
//! fit in i64 over i64, it is rounded **down** to the best rational below it whose denominator is
//! at most 2^31 (continued-fraction convergents and semiconvergents), so a floor stays a floor
//! (the L solve, §3.5). [`Q::ceil_f64`], [`Q::add_up`] and [`Q::mul_up`] round up instead, for
//! the ceiling U.
//! Flows are never negative; signed values are supported for differences only.

use std::cmp::Ordering;

/// The largest denominator a rounded result keeps.
const MAX_DEN: i128 = 1 << 31;
/// Integer parts beyond this are kept as integers (their fraction dropped, floor-wise).
const MAX_INT_PART: i128 = 1 << 31;

/// A rational `n / d` with `d > 0` in lowest terms, or `+∞` (`d == 0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Q {
    n: i64,
    d: i64,
}

impl Default for Q {
    fn default() -> Self {
        Q::ZERO
    }
}

/// The rounding direction of a result that does not fit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Round {
    Down,
    Up,
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return a << shift;
        }
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// The best rational `p / q <= r / d` with `q <= maxq`, for `0 <= r < d`: the last even
/// convergent of `r / d`'s continued fraction that fits, or the largest semiconvergent after it.
fn lower_approx(r: i128, d: i128, maxq: i128) -> (i128, i128) {
    if r == 0 {
        return (0, 1);
    }
    let (mut h0, mut k0, mut h1, mut k1) = (0i128, 1i128, 1i128, 0i128);
    let (mut num, mut den) = (r, d);
    let mut best = (0i128, 1i128);
    let mut i = 0u32;
    loop {
        let a = num / den;
        let rem = num - a * den;
        let fits = if k1 == 0 {
            k0 <= maxq
        } else {
            a <= (maxq - k0) / k1
        };
        if !fits {
            if i.is_multiple_of(2) && k1 != 0 {
                let t = (maxq - k0) / k1;
                if t >= 1 {
                    best = (t * h1 + h0, t * k1 + k0);
                }
            }
            return best;
        }
        let (h2, k2) = (a * h1 + h0, a * k1 + k0);
        if rem == 0 {
            return (h2, k2);
        }
        if i.is_multiple_of(2) {
            best = (h2, k2);
        }
        h0 = h1;
        k0 = k1;
        h1 = h2;
        k1 = k2;
        num = den;
        den = rem;
        i += 1;
    }
}

// The arithmetic is named methods, not `std::ops`: every operation rounds a named way on
// overflow (down unless `_up`), which an operator would hide.
#[allow(clippy::should_implement_trait)]
impl Q {
    pub const ZERO: Q = Q { n: 0, d: 1 };
    pub const ONE: Q = Q { n: 1, d: 1 };
    pub const INF: Q = Q { n: 1, d: 0 };

    /// `n / d`, reduced; `d == 0` is a programming error unless `n > 0` (then `+∞`).
    pub fn new(n: i64, d: i64) -> Q {
        if d == 0 {
            assert!(n > 0, "rational with zero denominator");
            return Q::INF;
        }
        Q::from_i128(i128::from(n), i128::from(d), Round::Down)
    }

    pub const fn int(n: i64) -> Q {
        Q { n, d: 1 }
    }

    /// `n / d` from unsigned wire fields: `0 / 0` is 0 and `n / 0` is `+∞`.
    pub fn ratio_u64(n: u64, d: u64) -> Q {
        if d == 0 {
            return if n == 0 { Q::ZERO } else { Q::INF };
        }
        Q::from_i128(i128::from(n), i128::from(d), Round::Down)
    }

    /// `batch` units every `ticks_num / ticks_den` ticks, as units per tick: `batch × den / num`.
    /// `ticks_num == 0` is unlimited; `ticks_den == 0` is never (0).
    pub fn per_ticks(batch: u32, ticks_num: u32, ticks_den: u32) -> Q {
        if ticks_num == 0 {
            return Q::INF;
        }
        if ticks_den == 0 {
            return Q::ZERO;
        }
        Q::ratio_u64(
            u64::from(batch) * u64::from(ticks_den),
            u64::from(ticks_num),
        )
    }

    pub fn numer(self) -> i64 {
        self.n
    }

    pub fn denom(self) -> i64 {
        self.d
    }

    pub fn is_inf(self) -> bool {
        self.d == 0
    }

    pub fn is_zero(self) -> bool {
        self.n == 0 && self.d != 0
    }

    pub fn is_pos(self) -> bool {
        self.n > 0
    }

    pub fn to_f64(self) -> f64 {
        if self.d == 0 {
            f64::INFINITY
        } else {
            self.n as f64 / self.d as f64
        }
    }

    fn from_i128(mut n: i128, mut d: i128, round: Round) -> Q {
        debug_assert!(d != 0);
        if d < 0 {
            n = -n;
            d = -d;
        }
        if n == 0 {
            return Q::ZERO;
        }
        let g = gcd_u128(n.unsigned_abs(), d as u128) as i128;
        if g > 1 {
            n /= g;
            d /= g;
        }
        if let (Ok(n64), Ok(d64)) = (i64::try_from(n), i64::try_from(d)) {
            return Q { n: n64, d: d64 };
        }
        match round {
            Round::Down => Q::approx_floor(n, d),
            Round::Up => {
                let f = Q::approx_floor(-n, d);
                Q { n: -f.n, d: f.d }
            }
        }
    }

    /// The best rational at or below `n / d` (d > 0, reduced, too large for i64) with a
    /// denominator of at most 2^31.
    fn approx_floor(n: i128, d: i128) -> Q {
        let a = n.div_euclid(d);
        let r = n.rem_euclid(d);
        if a >= MAX_INT_PART {
            return Q {
                n: i64::try_from(a).unwrap_or(i64::MAX),
                d: 1,
            };
        }
        if a < -MAX_INT_PART {
            return Q {
                n: i64::try_from(a).unwrap_or(i64::MIN + 1),
                d: 1,
            };
        }
        let (p, q) = lower_approx(r, d, MAX_DEN);
        let num = a * q + p;
        let g = gcd_u128(num.unsigned_abs(), q as u128).max(1) as i128;
        Q {
            n: (num / g) as i64,
            d: (q / g) as i64,
        }
    }

    fn reduce64(n: i64, d: i64) -> Q {
        if n == 0 {
            return Q::ZERO;
        }
        let g = gcd_u64(n.unsigned_abs(), d as u64) as i64;
        Q { n: n / g, d: d / g }
    }

    fn add_r(self, o: Q, round: Round) -> Q {
        if self.d == 0 || o.d == 0 {
            return Q::INF;
        }
        if self.d == o.d {
            if let Some(n) = self.n.checked_add(o.n) {
                return Q::reduce64(n, self.d);
            }
        } else if let (Some(a), Some(b), Some(d)) = (
            self.n.checked_mul(o.d),
            o.n.checked_mul(self.d),
            self.d.checked_mul(o.d),
        ) && let Some(n) = a.checked_add(b)
        {
            return Q::reduce64(n, d);
        }
        let n = i128::from(self.n) * i128::from(o.d) + i128::from(o.n) * i128::from(self.d);
        let d = i128::from(self.d) * i128::from(o.d);
        Q::from_i128(n, d, round)
    }

    /// `self + o`, rounded down if it does not fit. `∞ + x = ∞`.
    pub fn add(self, o: Q) -> Q {
        self.add_r(o, Round::Down)
    }

    /// `self + o`, rounded up if it does not fit.
    pub fn add_up(self, o: Q) -> Q {
        self.add_r(o, Round::Up)
    }

    /// `self - o` for finite values, rounded down.
    pub fn sub(self, o: Q) -> Q {
        assert!(!o.is_inf(), "subtracting infinity");
        if self.is_inf() {
            return Q::INF;
        }
        self.add(Q { n: -o.n, d: o.d })
    }

    /// `max(0, self - o)`: `∞ - x = ∞` for any x (a remaining unlimited supply), `x - ∞ = 0`.
    pub fn sub_sat(self, o: Q) -> Q {
        if self.is_inf() {
            return Q::INF;
        }
        if o.is_inf() {
            return Q::ZERO;
        }
        let r = self.add(Q { n: -o.n, d: o.d });
        if r.n < 0 { Q::ZERO } else { r }
    }

    fn mul_r(self, o: Q, round: Round) -> Q {
        if self.is_zero() || o.is_zero() {
            return Q::ZERO;
        }
        if self.d == 0 || o.d == 0 {
            return if (self.n < 0) ^ (o.n < 0) {
                Q::ZERO
            } else {
                Q::INF
            };
        }
        if let (Some(n), Some(d)) = (self.n.checked_mul(o.n), self.d.checked_mul(o.d)) {
            return Q::reduce64(n, d);
        }
        Q::from_i128(
            i128::from(self.n) * i128::from(o.n),
            i128::from(self.d) * i128::from(o.d),
            round,
        )
    }

    /// `self × o`, rounded down. `∞ × 0 = 0`, `∞ × x = ∞` for x > 0.
    pub fn mul(self, o: Q) -> Q {
        self.mul_r(o, Round::Down)
    }

    /// `self × o`, rounded up.
    pub fn mul_up(self, o: Q) -> Q {
        self.mul_r(o, Round::Up)
    }

    /// `self / o`, rounded down. `x / 0 = ∞` for x > 0 (and 0 for x = 0), `x / ∞ = 0`,
    /// `∞ / x = ∞` for finite x > 0.
    pub fn div(self, o: Q) -> Q {
        if o.is_zero() {
            return if self.n > 0 { Q::INF } else { Q::ZERO };
        }
        if o.is_inf() {
            return if self.is_inf() { Q::INF } else { Q::ZERO };
        }
        if self.is_inf() {
            return if o.n > 0 { Q::INF } else { Q::ZERO };
        }
        self.mul_r(
            Q {
                n: if o.n < 0 { -o.d } else { o.d },
                d: o.n.abs(),
            },
            Round::Down,
        )
    }

    pub fn min(self, o: Q) -> Q {
        if o < self { o } else { self }
    }

    pub fn max(self, o: Q) -> Q {
        if o > self { o } else { self }
    }

    /// The smallest rational with denominator 2^24 at or above `x`, for the ceiling U (G-B15).
    /// Non-finite or huge values are `+∞`.
    pub fn ceil_f64(x: f64) -> Q {
        if !x.is_finite() || x > 1e12 {
            return Q::INF;
        }
        if x == 0.0 {
            return Q::ZERO;
        }
        Q::from_i128((x * 16_777_216.0).ceil() as i128, 1 << 24, Round::Up)
    }

    /// The largest rational with denominator 2^20 at or below `x` (a derating factor).
    pub fn floor_f64(x: f64) -> Q {
        if x.is_nan() || x <= 0.0 {
            return Q::ZERO;
        }
        if !x.is_finite() || x > 1e12 {
            return Q::INF;
        }
        Q::from_i128((x * 1_048_576.0).floor() as i128, 1 << 20, Round::Down)
    }
}

impl Ord for Q {
    fn cmp(&self, o: &Q) -> Ordering {
        match (self.d == 0, o.d == 0) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                if self.d == o.d {
                    self.n.cmp(&o.n)
                } else {
                    (i128::from(self.n) * i128::from(o.d))
                        .cmp(&(i128::from(o.n) * i128::from(self.d)))
                }
            }
        }
    }
}

impl PartialOrd for Q {
    fn partial_cmp(&self, o: &Q) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// In a glass page (G2-9): numerator and denominator as written; a page with a negative
/// denominator, or an infinity other than `1 / 0`, is refused.
impl crate::persist::Persist for Q {
    fn put(&self, w: &mut rtsim_kernel::codec::Writer) {
        w.var_i64(self.n);
        w.var_i64(self.d);
    }
    fn get(r: &mut rtsim_kernel::codec::Reader<'_>) -> rtsim_kernel::codec::Result<Self> {
        let (n, d) = (r.var_i64()?, r.var_i64()?);
        if d > 0 || (d == 0 && n == 1) {
            Ok(Q { n, d })
        } else {
            crate::persist::invalid("rational")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn exact_while_it_fits() {
        let a = Q::new(2, 17);
        let b = Q::new(1, 10);
        assert_eq!(a.sub(b), Q::new(3, 170));
        assert_eq!(a.mul(Q::int(1200)), Q::new(2400, 17));
        assert_eq!(Q::new(6, 4), Q::new(3, 2));
        assert_eq!(Q::new(1, 32).add(Q::new(1, 32)), Q::new(1, 16));
        assert_eq!(Q::per_ticks(2, 17, 1), Q::new(2, 17));
        assert_eq!(Q::per_ticks(1, 0, 1), Q::INF);
        assert_eq!(Q::per_ticks(1, 5, 0), Q::ZERO);
        assert!(Q::INF > Q::int(i64::MAX));
        assert_eq!(Q::INF.min(Q::ONE), Q::ONE);
        assert_eq!(Q::INF.mul(Q::ZERO), Q::ZERO);
        assert_eq!(Q::ONE.div(Q::INF), Q::ZERO);
        assert_eq!(Q::INF.sub_sat(Q::ONE), Q::INF);
        assert_eq!(Q::ONE.sub_sat(Q::INF), Q::ZERO);
        assert_eq!(Q::new(1, 3).sub_sat(Q::new(1, 2)), Q::ZERO);
    }

    #[test]
    fn an_overflowing_denominator_rounds_down_to_a_smaller_one() {
        // Two large coprime denominators: the exact sum's denominator is about 2^122.
        let p1 = 2_305_843_009_213_693_951i64; // 2^61 - 1 (prime)
        let p2 = 2_147_483_647i64 * 1_000_003; // a different large odd number
        let a = Q::new(1, p1);
        let b = Q::new(1, p2);
        let s = a.add(b);
        assert!(s.denom() <= 1 << 31, "denominator {} kept small", s.denom());
        // A floor: never above the exact value, and close to it.
        let exact = 1.0 / p1 as f64 + 1.0 / p2 as f64;
        assert!(s.to_f64() <= exact * (1.0 + 1e-12));
        let up = a.add_up(b);
        assert!(up >= s);
        assert!(up.to_f64() >= exact * (1.0 - 1e-12));
        // A product that overflows: (p1 - 2)/p1 × (p2 - 2)/p2.
        let x = Q::new(p1 - 2, p1).mul(Q::new(p2 - 2, p2));
        assert!(x.denom() <= 1 << 31);
        assert!(x < Q::ONE);
        assert!(x.to_f64() > 0.999_999);
    }

    #[test]
    fn floats_round_the_named_way() {
        let u = Q::ceil_f64(2.0 / 17.0);
        assert!(u >= Q::new(2, 17));
        assert!(u.to_f64() - 2.0 / 17.0 < 1e-6);
        let f = Q::floor_f64(0.75);
        assert_eq!(f, Q::new(3, 4));
        assert_eq!(Q::ceil_f64(f64::INFINITY), Q::INF);
    }

    proptest! {
        /// Whatever the operands, a rounded sum or product is never above the exact value, and the
        /// result is in lowest terms with a positive denominator.
        #[test]
        fn rounding_is_always_down(
            an in 1i64..i64::MAX, ad in 1i64..i64::MAX, bn in 1i64..i64::MAX, bd in 1i64..i64::MAX,
        ) {
            let a = Q::new(an, ad);
            let b = Q::new(bn, bd);
            for (r, exact_n, exact_d) in [
                (a.add(b),
                 i128::from(a.numer()) * i128::from(b.denom()) + i128::from(b.numer()) * i128::from(a.denom()),
                 i128::from(a.denom()) * i128::from(b.denom())),
                (a.mul(b),
                 i128::from(a.numer()) * i128::from(b.numer()),
                 i128::from(a.denom()) * i128::from(b.denom())),
            ] {
                prop_assert!(r.denom() > 0);
                // r <= exact  <=>  r.n × exact_d <= exact_n × r.d, compared without overflow by
                // dividing first: both sides positive.
                let lhs = (r.numer() as f64) / (r.denom() as f64);
                let rhs = (exact_n as f64) / (exact_d as f64);
                prop_assert!(lhs <= rhs * (1.0 + 1e-9), "{lhs} > {rhs}");
                prop_assert!(lhs >= rhs * (1.0 - 1e-6) - 1e-9, "{lhs} far below {rhs}");
            }
        }
    }
}
