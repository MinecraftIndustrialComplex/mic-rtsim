// SPDX-License-Identifier: AGPL-3.0-or-later
//! The counter-based RNG (PLAN.md §4.7, SCALE.md §4).
//!
//! **Algorithm:** Philox4x64-10 from Salmon, Moraes, Dror and Shaw, "Parallel Random Numbers: As
//! Easy as 1, 2, 3", SC '11 (doi:10.1145/2063384.2063405). It maps a 256-bit counter and a 128-bit
//! key to 256 random bits with no state, so any draw can be computed directly from its
//! coordinates. That is what lets a lazy region commit slot 917's draws without having drawn slots
//! 0 to 916, and lets a thread count never change a result. The known-answer tests below are
//! Random123's vectors, reproduced with numpy 2.4.4's `Philox` bit generator (the same algorithm)
//! on 2026-10-20.
//!
//! **Keying:** the key is (world seed, system); the counter is (entity, slot, block, 0). A stream is
//! therefore one (world seed, system, entity, slot) and holds 2^64 blocks of four `u64`s.
//!
//! **Floats:** distribution code uses `f64` `ln`, `sqrt` and `log1p`. These are deterministic on one
//! platform and build, which is what the determinism gate checks (same machine, any thread count).
//! Across platforms the last bit of `ln` may differ; committed results are stored state, never
//! recomputed, so a save moved between machines stays consistent.

/// Philox4x64 round multipliers (Random123 `PHILOX_M4x64_0`, `PHILOX_M4x64_1`).
const M0: u64 = 0xD2E7_470E_E14C_6C93;
const M1: u64 = 0xCA5A_8263_9512_1157;
/// Key schedule Weyl constants (Random123 `PHILOX_W64_0`, `PHILOX_W64_1`: the golden ratio and
/// sqrt(3) - 1, as 64-bit fractions).
const W0: u64 = 0x9E37_79B9_7F4A_7C15;
const W1: u64 = 0xBB67_AE85_84CA_A73B;

#[inline(always)]
fn mulhilo(a: u64, b: u64) -> (u64, u64) {
    let p = u128::from(a) * u128::from(b);
    ((p >> 64) as u64, p as u64)
}

/// One Philox4x64-10 block.
#[inline]
pub fn philox4x64_10(ctr: [u64; 4], key: [u64; 2]) -> [u64; 4] {
    let mut c = ctr;
    let mut k = key;
    for round in 0..10 {
        if round > 0 {
            k[0] = k[0].wrapping_add(W0);
            k[1] = k[1].wrapping_add(W1);
        }
        let (hi0, lo0) = mulhilo(M0, c[0]);
        let (hi1, lo1) = mulhilo(M1, c[2]);
        c = [hi1 ^ c[1] ^ k[0], lo1, hi0 ^ c[3] ^ k[1], lo0];
    }
    c
}

/// FNV-1a, 64-bit (Fowler, Noll and Vo; offset basis and prime from the FNV specification,
/// draft-eastlake-fnv). `const`, so system ids are computed at compile time and never change
/// between builds.
pub const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

/// Which system draws: one per random process (herd births, caravan departures, ...).
///
/// Named ids hash their name, so adding a system never shifts another system's numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemId(pub u64);

impl SystemId {
    pub const fn named(name: &str) -> SystemId {
        SystemId(fnv1a64(name.as_bytes()))
    }
}

/// A stream's coordinates without the slot: bind a slot with [`StreamKey::slot`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamKey {
    pub world_seed: u64,
    pub system: SystemId,
    pub entity: u64,
}

impl StreamKey {
    pub const fn new(world_seed: u64, system: SystemId, entity: u64) -> Self {
        Self {
            world_seed,
            system,
            entity,
        }
    }

    pub const fn slot(self, slot: u64) -> SlotRng {
        SlotRng::new(self, slot)
    }

    /// Block `n` of `slot`, without a stream object.
    #[inline]
    pub fn block(self, slot: u64, n: u64) -> [u64; 4] {
        philox4x64_10([self.entity, slot, n, 0], [self.world_seed, self.system.0])
    }
}

/// The draws of one (world seed, system, entity, slot), in order. Two `SlotRng`s with the same
/// coordinates produce the same numbers, whenever and wherever they are created.
#[derive(Clone, Debug)]
pub struct SlotRng {
    key: StreamKey,
    slot: u64,
    next_block: u64,
    buf: [u64; 4],
    pos: u8,
}

impl SlotRng {
    pub const fn new(key: StreamKey, slot: u64) -> Self {
        Self {
            key,
            slot,
            next_block: 0,
            buf: [0; 4],
            pos: 4,
        }
    }

    /// Draws taken so far, in `u64`s. Lets tests and the profiler count draws.
    pub fn draws(&self) -> u64 {
        self.next_block * 4 - (4 - u64::from(self.pos))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        if self.pos == 4 {
            self.buf = self.key.block(self.slot, self.next_block);
            self.next_block += 1;
            self.pos = 0;
        }
        let x = self.buf[self.pos as usize];
        self.pos += 1;
        x
    }

    /// Uniform in [0, 1), 53 bits.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in (0, 1), never 0 or 1: safe to take the log of, and the log is never 0.
    #[inline]
    pub fn next_f64_open(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in [0, n), unbiased. Lemire, "Fast Random Integer Generation in an Interval", ACM
    /// TOMACS 29(1), 2019: one multiply, and a division only on the rare rejection path.
    #[inline]
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        let mut m = u128::from(self.next_u64()) * u128::from(n);
        if (m as u64) < n {
            let threshold = n.wrapping_neg() % n;
            while (m as u64) < threshold {
                m = u128::from(self.next_u64()) * u128::from(n);
            }
        }
        (m >> 64) as u64
    }

    #[inline]
    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// Binomial(n, p) with expected cost independent of `n`: inversion by geometric gaps while
    /// n·min(p, 1-p) < 10, otherwise Hörmann's BTRS transformed rejection ("The generation of
    /// binomial random variates", J. Statist. Comput. Simul. 46, 1993), in the form TensorFlow's
    /// `random_binomial_op` uses. This is the single draw that lets deaths at a constant hazard
    /// over n days cost O(1) (SCALE.md §4).
    pub fn binomial(&mut self, n: u64, p: f64) -> u64 {
        assert!((0.0..=1.0).contains(&p), "binomial p = {p}");
        assert!(
            n <= 1 << 53,
            "binomial n = {n} exceeds f64's exact integers"
        );
        if n == 0 || p == 0.0 {
            return 0;
        }
        if p == 1.0 {
            return n;
        }
        if p > 0.5 {
            return n - self.binomial(n, 1.0 - p);
        }
        let nf = n as f64;
        if nf * p < 10.0 {
            self.binomial_inversion(nf, p)
        } else {
            self.binomial_btrs(nf, p)
        }
    }

    /// Counts geometric gaps until they pass `n`; expected n·p + 1 draws.
    fn binomial_inversion(&mut self, n: f64, p: f64) -> u64 {
        let log_q = (-p).ln_1p();
        let mut sum = 0.0;
        let mut k = 0u64;
        loop {
            // A geometric gap is at least one slot; u < 1 keeps ln(u) < 0.
            let u = self.next_f64_open();
            sum += (u.ln() / log_q).ceil().max(1.0);
            if sum > n {
                return k;
            }
            k += 1;
        }
    }

    fn binomial_btrs(&mut self, n: f64, p: f64) -> u64 {
        let spq = (n * p * (1.0 - p)).sqrt();
        let b = 1.15 + 2.53 * spq;
        let a = -0.0873 + 0.0248 * b + 0.01 * p;
        let c = n * p + 0.5;
        let v_r = 0.92 - 4.2 / b;
        let r = p / (1.0 - p);
        let alpha = (2.83 + 5.1 / b) * spq;
        let m = ((n + 1.0) * p).floor();
        loop {
            let u = self.next_f64() - 0.5;
            let mut v = self.next_f64();
            let us = 0.5 - u.abs();
            let k = ((2.0 * a / us + b) * u + c).floor();
            if k < 0.0 || k > n {
                continue;
            }
            if us >= 0.07 && v <= v_r {
                // Inside the box, where the hat lies under the density: accept at once.
                return k as u64;
            }
            v = (v * alpha / (a / (us * us) + b)).ln();
            // ln f(k) - ln f(m) by Stirling. TensorFlow's form, (n + 1)·ln((n - m + 1)/(n - k + 1))
            // and kin, takes logs of ratios within 1e-14 of 1 and multiplies their rounding by n:
            // at n near 2^53 the bound is off by a whole unit and the draws are biased. Here every
            // log of a near-1 ratio is ln_1p of an exact integer difference, and the remaining
            // (k - m)·ln(...) term is small near the mode.
            let bound = (m + 0.5) * ((m - k) / (k + 1.0)).ln_1p()
                + (n - m + 0.5) * ((k - m) / (n - k + 1.0)).ln_1p()
                + (k - m) * (r * (n - k + 1.0) / (k + 1.0)).ln()
                + stirling_tail(m)
                + stirling_tail(n - m)
                - stirling_tail(k)
                - stirling_tail(n - k);
            if v <= bound {
                return k as u64;
            }
        }
    }
}

/// The remainder of Stirling's series: ln(k!) - [(k + 1/2) ln(k + 1) - (k + 1) + ln(2π)/2]. Exact
/// (by summing logs) below 10, the series to 1/k^5 above, where it is accurate to about 1e-10.
fn stirling_tail(k: f64) -> f64 {
    if k < 10.0 {
        let k = k as u32;
        let mut ln_fact = 0.0;
        for i in 2..=k {
            ln_fact += f64::from(i).ln();
        }
        let kf = f64::from(k);
        return ln_fact
            - ((kf + 0.5) * (kf + 1.0).ln() - (kf + 1.0) + 0.5 * std::f64::consts::TAU.ln());
    }
    let kp1 = k + 1.0;
    let kp1sq = kp1 * kp1;
    (1.0 / 12.0 - (1.0 / 360.0 - 1.0 / 1260.0 / kp1sq) / kp1sq) / kp1
}

/// The probability that a constant per-tick hazard `p` fires at least once in `n` ticks:
/// 1 - (1 - p)^n, computed without cancellation (PLAN.md §4.3).
pub fn hazard_over(p: f64, n: u64) -> f64 {
    if p >= 1.0 {
        return if n > 0 { 1.0 } else { 0.0 };
    }
    -((n as f64) * (-p).ln_1p()).exp_m1()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Random123's philox4x64_10 known answers, reproduced with numpy 2.4.4 (see module docs).
    #[test]
    fn philox_known_answers() {
        assert_eq!(
            philox4x64_10([0; 4], [0; 2]),
            [
                0x16554d9eca36314c,
                0xdb20fe9d672d0fdc,
                0xd7e772cee186176b,
                0x7e68b68aec7ba23b
            ]
        );
        assert_eq!(
            philox4x64_10([u64::MAX; 4], [u64::MAX; 2]),
            [
                0x87b092c3013fe90b,
                0x438c3c67be8d0224,
                0x9cc7d7c69cd777b6,
                0xa09caebf594f0ba0
            ]
        );
        assert_eq!(
            philox4x64_10(
                [
                    0x243f6a8885a308d3,
                    0x13198a2e03707344,
                    0xa4093822299f31d0,
                    0x082efa98ec4e6c89
                ],
                [0x452821e638d01377, 0xbe5466cf34e90c6c]
            ),
            [
                0xa528f45403e61d95,
                0x38c72dbd566e9788,
                0xa5a1610e72fd18b5,
                0x57bd43b5e52b7fe6
            ]
        );
        assert_eq!(
            philox4x64_10([1, 2, 3, 4], [5, 6]),
            [
                0xa39b5519339fe354,
                0xaceb1228efc25196,
                0xa0a2e3c25aa5f4fc,
                0x08d0cfa9332720df
            ]
        );
    }

    #[test]
    fn fnv_known_answers() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn streams_are_pure_functions_of_their_coordinates() {
        let k = StreamKey::new(42, SystemId::named("test"), 7);
        let a: Vec<u64> = {
            let mut r = k.slot(3);
            (0..9).map(|_| r.next_u64()).collect()
        };
        let mut r = k.slot(3);
        let b: Vec<u64> = (0..9).map(|_| r.next_u64()).collect();
        assert_eq!(a, b);
        assert_eq!(r.draws(), 9);
        let mut other = k.slot(4);
        assert_ne!(a[0], other.next_u64());
        assert_eq!(a[4], k.block(3, 1)[0]);
    }

    #[test]
    fn below_is_in_range_and_roughly_uniform() {
        let mut r = StreamKey::new(1, SystemId(2), 3).slot(0);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[r.below(6) as usize] += 1;
        }
        for c in counts {
            assert!((9_400..=10_600).contains(&c), "{counts:?}");
        }
    }

    fn ln_choose(n: u64, k: u64) -> f64 {
        let lf = |x: u64| (2..=x).map(|i| (i as f64).ln()).sum::<f64>();
        lf(n) - lf(k) - lf(n - k)
    }

    /// Pearson chi-square of `samples` draws against the exact pmf, over bins with expected
    /// count >= 5 (the tails pooled). Returns (statistic, degrees of freedom).
    fn chi_square(n: u64, p: f64, samples: u32, seed: u64) -> (f64, usize) {
        let mut r = StreamKey::new(seed, SystemId::named("chi"), n).slot(0);
        let mut hist = vec![0u32; n as usize + 1];
        for _ in 0..samples {
            hist[r.binomial(n, p) as usize] += 1;
        }
        let pmf: Vec<f64> = (0..=n)
            .map(|k| (ln_choose(n, k) + k as f64 * p.ln() + (n - k) as f64 * (1.0 - p).ln()).exp())
            .collect();
        let s = f64::from(samples);
        let (mut stat, mut bins) = (0.0, 0usize);
        let (mut pool_obs, mut pool_exp) = (0.0, 0.0);
        for k in 0..=n as usize {
            let e = pmf[k] * s;
            if e >= 5.0 {
                stat += (f64::from(hist[k]) - e).powi(2) / e;
                bins += 1;
            } else {
                pool_obs += f64::from(hist[k]);
                pool_exp += e;
            }
        }
        if pool_exp > 0.0 {
            stat += (pool_obs - pool_exp).powi(2) / pool_exp;
            bins += 1;
        }
        (stat, bins - 1)
    }

    /// Both branches (inversion below n·p = 10, BTRS above) match the exact distribution. The
    /// streams are fixed, so this is deterministic, not flaky; the bound is a loose 0.001-level
    /// critical value (df + 3.1·sqrt(2·df) + 10).
    #[test]
    fn binomial_matches_the_exact_pmf() {
        for &(n, p) in &[
            (20u64, 0.2f64),
            (100, 0.05),
            (100, 0.3),
            (1000, 0.5),
            (500, 0.9),
            (5000, 0.01),
        ] {
            let (stat, df) = chi_square(n, p, 200_000, 9);
            let bound = df as f64 + 3.1 * (2.0 * df as f64).sqrt() + 10.0;
            assert!(
                stat < bound,
                "n={n} p={p}: chi2={stat:.1} df={df} bound={bound:.1}"
            );
        }
    }

    /// Chi-square against the exact pmf at n = 2^53 - 1, where the TensorFlow form of the BTRS
    /// bound is off by a factor of e (review of 2026-10-20). Small means keep the support short;
    /// the pmf is summed from logs of the first k factors.
    #[test]
    fn binomial_is_exact_at_the_largest_n() {
        let n = (1u64 << 53) - 1;
        for (mean, seed) in [(12.0f64, 1u64), (30.0, 2)] {
            let p = mean / n as f64;
            let mut r = StreamKey::new(seed, SystemId::named("chi.big"), 0).slot(0);
            let samples = 400_000u32;
            let top = (mean * 4.0) as usize;
            let mut hist = vec![0u32; top + 1];
            for _ in 0..samples {
                let k = r.binomial(n, p) as usize;
                hist[k.min(top)] += 1;
            }
            let ln_pmf = |k: u64| {
                let mut lc = 0.0;
                for i in 0..k {
                    lc += ((n - i) as f64).ln() - ((i + 1) as f64).ln();
                }
                lc + k as f64 * p.ln() + (n - k) as f64 * (-p).ln_1p()
            };
            // Dense bins (expected count >= 5) on their own; everything else, including the
            // overflow bin at `top`, pooled into one.
            let s = f64::from(samples);
            let (mut stat, mut bins) = (0.0, 0usize);
            let (mut dense_e, mut dense_o) = (0.0, 0.0);
            for (k, &count) in hist.iter().enumerate().take(top) {
                let e = ln_pmf(k as u64).exp() * s;
                if e >= 5.0 {
                    let o = f64::from(count);
                    stat += (o - e).powi(2) / e;
                    bins += 1;
                    dense_e += e;
                    dense_o += o;
                }
            }
            let (pooled_e, pooled_o) = (s - dense_e, s - dense_o);
            stat += (pooled_o - pooled_e).powi(2) / pooled_e;
            let df = bins as f64;
            let bound = df + 3.1 * (2.0 * df).sqrt() + 10.0;
            assert!(
                stat < bound,
                "mean {mean}: chi2 {stat:.1}, df {df}, bound {bound:.1}"
            );
        }
    }

    #[test]
    fn binomial_moments_at_scale() {
        // n far beyond any table: mean and variance must hold, and cost must not grow with n.
        let n = 1u64 << 40;
        let p = 0.013;
        let mut r = StreamKey::new(5, SystemId::named("big"), 0).slot(0);
        let k = 20_000;
        let xs: Vec<f64> = (0..k).map(|_| r.binomial(n, p) as f64).collect();
        let mean = xs.iter().sum::<f64>() / k as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (k - 1) as f64;
        let (em, ev) = (n as f64 * p, n as f64 * p * (1.0 - p));
        assert!(
            (mean - em).abs() < 5.0 * (ev / k as f64).sqrt(),
            "mean {mean} vs {em}"
        );
        assert!((var / ev - 1.0).abs() < 0.05, "var {var} vs {ev}");
        assert!(
            r.draws() < 4 * k as u64,
            "BTRS used {} draws for {k} samples",
            r.draws()
        );
    }

    #[test]
    fn hazard_over_matches_the_direct_formula() {
        assert_eq!(hazard_over(0.0, 100), 0.0);
        assert_eq!(hazard_over(1.0, 3), 1.0);
        let direct = 1.0 - (1.0f64 - 0.01).powi(50);
        assert!((hazard_over(0.01, 50) - direct).abs() < 1e-14);
    }
}
