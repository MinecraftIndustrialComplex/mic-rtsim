// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flat by century (EQUILIBRIUM.md §4's S11 row; docs/overnight-choices-equilibrium.md EQC438 and
//! EQC447): one rule that the who-is-alive soak, the long run and the CLI read.
//!
//! A series `x_1 .. x_n` (a kind's fill by century, a page table's bytes) has a **drift**
//! `D = (n - 1) x` its Theil-Sen slope, the median over every pair `i < j` of
//! `(x_j - x_i) / (j - i)`, and a Mann-Kendall `S` with the ties correction and a two-sided `p` from
//! the normal approximation with the continuity correction. It **fails** if and only if
//! `|D| > delta` and `p < alpha`: a drift both material and significant. A series whose values are
//! all tied is flat. Both measures are rank-based, so one odd value moves neither, and no
//! distribution is assumed.
//!
//! The normal approximation was checked against the exact distribution at the soak's 26 values with
//! heavy ties (EQC471): it is never anticonservative at 0.001 there, so no permutation p is needed.
//!
//! A series of fewer than three values is an error, never a pass.

/// Why a series could not be judged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrendError {
    /// Fewer than three values: no rule can call that flat.
    TooShort { n: usize },
    /// A value that is not a finite number.
    NotFinite { at: usize },
}

impl std::fmt::Display for TrendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrendError::TooShort { n } => write!(f, "a series of {n} values is too short to judge"),
            TrendError::NotFinite { at } => write!(f, "value {at} of the series is not finite"),
        }
    }
}

impl std::error::Error for TrendError {}

/// One series judged by the flat rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flat {
    /// The Theil-Sen slope times the span, `n - 1` steps.
    pub drift: f64,
    /// Mann-Kendall's `S`.
    pub s: i64,
    /// The two-sided p of `S` (1 for a series with no order at all).
    pub p: f64,
    /// False only when `|drift| > delta` and `p < alpha`.
    pub ok: bool,
}

fn check(xs: &[f64]) -> Result<(), TrendError> {
    if xs.len() < 3 {
        return Err(TrendError::TooShort { n: xs.len() });
    }
    match xs.iter().position(|x| !x.is_finite()) {
        Some(at) => Err(TrendError::NotFinite { at }),
        None => Ok(()),
    }
}

/// The Theil-Sen slope: the median over every pair `i < j` of `(x_j - x_i) / (j - i)`, in units of
/// the series per step.
pub fn theil_sen(xs: &[f64]) -> Result<f64, TrendError> {
    check(xs)?;
    let n = xs.len();
    let mut slopes = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in i + 1..n {
            slopes.push((xs[j] - xs[i]) / (j - i) as f64);
        }
    }
    slopes.sort_by(f64::total_cmp);
    let m = slopes.len();
    Ok(if m % 2 == 1 {
        slopes[m / 2]
    } else {
        (slopes[m / 2 - 1] + slopes[m / 2]) / 2.0
    })
}

/// Mann-Kendall's `S` (the sum over pairs `i < j` of the sign of `x_j - x_i`) and its two-sided p:
/// the variance `(n(n-1)(2n+5) - sum over tie groups of t(t-1)(2t+5)) / 18`, `Z = (S - sign S) /
/// sqrt(var)`, `p = erfc(|Z| / sqrt 2)`. A series with `S` 0 or no untied pair has p 1.
pub fn mann_kendall(xs: &[f64]) -> Result<(i64, f64), TrendError> {
    check(xs)?;
    let n = xs.len();
    let mut s = 0i64;
    for i in 0..n {
        for j in i + 1..n {
            s += match xs[j].total_cmp(&xs[i]) {
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
            };
        }
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let term = |t: f64| t * (t - 1.0) * (2.0 * t + 5.0);
    let mut ties = 0.0;
    let mut run = 1usize;
    for k in 1..=n {
        if k < n && sorted[k] == sorted[k - 1] {
            run += 1;
        } else {
            ties += term(run as f64);
            run = 1;
        }
    }
    let var = (term(n as f64) - ties) / 18.0;
    if s == 0 || var <= 0.0 {
        return Ok((s, 1.0));
    }
    let z = (s.abs() - 1) as f64 / var.sqrt();
    Ok((s, erfc(z / std::f64::consts::SQRT_2).min(1.0)))
}

/// The flat rule (EQC438): `ok` unless the drift is material (`|D| > delta`) and significant
/// (`p < alpha`).
pub fn flat(xs: &[f64], delta: f64, alpha: f64) -> Result<Flat, TrendError> {
    let slope = theil_sen(xs)?;
    let (s, p) = mann_kendall(xs)?;
    let drift = slope * (xs.len() - 1) as f64;
    Ok(Flat {
        drift,
        s,
        p,
        ok: !(drift.abs() > delta && p < alpha),
    })
}

/// The complementary error function, with fractional error below 1.2e-7 everywhere (the Chebyshev
/// fit of Numerical Recipes' `erfcc`): ample for a p read against 0.001.
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let poly = -z * z - 1.265_512_23
        + t * (1.000_023_68
            + t * (0.374_091_96
                + t * (0.096_784_18
                    + t * (-0.186_288_06
                        + t * (0.278_868_07
                            + t * (-1.135_203_98
                                + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77))))))));
    let r = t * poly.exp();
    if x >= 0.0 { r } else { 2.0 - r }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{StreamKey, SystemId};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// Seeded noise, uniform on [-1, 1).
    fn noise(seed: u64, n: usize) -> Vec<f64> {
        let mut r = StreamKey::new(seed, SystemId::named("census.trend.test"), 0).slot(0);
        (0..n).map(|_| 2.0 * r.next_f64() - 1.0).collect()
    }

    #[test]
    fn theil_sen_and_mann_kendall_match_a_worked_example() {
        // Worked by hand: [1, 3, 2, 5, 4] has eight rising pairs and two falling, so S = 6, the
        // variance 5 x 4 x 15 / 18 = 16.67, Z = 5 / 4.082 = 1.2247 and p = 0.2207; its ten pair
        // slopes sorted are -1, -1, 1/3, 1/2, 3/4, 1, 1, 4/3, 2, 3, median 0.875.
        let a = [1.0, 3.0, 2.0, 5.0, 4.0];
        assert_eq!(mann_kendall(&a).unwrap().0, 6);
        assert!(close(mann_kendall(&a).unwrap().1, 0.220_671, 1e-5));
        assert!(close(theil_sen(&a).unwrap(), 0.875, 1e-12));
        // With a tie: [1, 2, 2, 3]: S = 5, the variance (4 x 3 x 13 - 2 x 1 x 9) / 18 = 7.667,
        // p = 0.1486; slopes 0, 1/2, 1/2, 2/3, 1, 1, median 7/12.
        let b = [1.0, 2.0, 2.0, 3.0];
        let (s, p) = mann_kendall(&b).unwrap();
        assert_eq!(s, 5);
        assert!(close(p, 0.148_562, 1e-5));
        assert!(close(theil_sen(&b).unwrap(), 7.0 / 12.0, 1e-12));
        // Ten values with one tie, as scipy's `theilslopes` and a hand Mann-Kendall read them:
        // S 34, the variance 124, p 0.003042, slope 0.8.
        let c = [10.0, 12.0, 11.0, 14.0, 13.0, 13.0, 16.0, 15.0, 18.0, 17.0];
        let (s, p) = mann_kendall(&c).unwrap();
        assert_eq!(s, 34);
        assert!(close(p, 0.003_041_7, 1e-6));
        assert!(close(theil_sen(&c).unwrap(), 0.8, 1e-12));
        let f = flat(&c, 1.0, 0.001).unwrap();
        assert!(close(f.drift, 7.2, 1e-9));
        assert!(f.ok, "significant at 0.01 but not at 0.001: {f:?}");
    }

    #[test]
    fn a_flat_noisy_series_is_flat() {
        // 26 centuries of a fill near 0.78 with noise of 0.01 against the herds' delta of 0.02.
        for seed in 1..=20 {
            let xs: Vec<f64> = noise(seed, 26).iter().map(|e| 0.78 + 0.01 * e).collect();
            let f = flat(&xs, 0.02, 0.001).unwrap();
            assert!(f.ok, "seed {seed}: {f:?}");
        }
    }

    #[test]
    fn a_slow_drift_is_not_flat() {
        // A rise of twice delta over the span, under noise of a quarter delta: material and
        // significant in every seed.
        let delta = 0.02;
        for seed in 1..=20 {
            let xs: Vec<f64> = noise(seed, 26)
                .iter()
                .enumerate()
                .map(|(i, e)| 0.78 + 2.0 * delta * i as f64 / 25.0 + 0.25 * delta * e)
                .collect();
            let f = flat(&xs, delta, 0.001).unwrap();
            assert!(!f.ok, "seed {seed}: {f:?}");
            assert!(f.drift > delta && f.p < 1e-4, "seed {seed}: {f:?}");
        }
        // The same drift, but under delta: significant, not material, so flat.
        let xs: Vec<f64> = (0..26).map(|i| 0.78 + 0.01 * i as f64 / 25.0).collect();
        let f = flat(&xs, delta, 0.001).unwrap();
        assert!(f.p < 1e-6 && f.ok, "{f:?}");
    }

    #[test]
    fn a_constant_series_is_flat() {
        let xs = [1.0; 26];
        let f = flat(&xs, 0.0, 0.001).unwrap();
        assert_eq!((f.s, f.p, f.drift, f.ok), (0, 1.0, 0.0, true));
    }

    #[test]
    fn one_odd_value_does_not_fail() {
        // A navy's assault: one century far below the rest, early, late or in the middle.
        for at in [0, 1, 12, 24, 25] {
            let mut xs: Vec<f64> = noise(at as u64 + 1, 26)
                .iter()
                .map(|e| 0.9 + 0.005 * e)
                .collect();
            xs[at] = 0.3;
            let f = flat(&xs, 0.078, 0.001).unwrap();
            assert!(f.ok, "odd value at {at}: {f:?}");
            assert!(f.drift.abs() < 0.05, "odd value at {at}: {f:?}");
        }
    }

    #[test]
    fn too_short_a_series_is_an_error() {
        for n in 0..3 {
            let xs = vec![1.0; n];
            assert_eq!(flat(&xs, 1.0, 0.001), Err(TrendError::TooShort { n }));
            assert!(theil_sen(&xs).is_err() && mann_kendall(&xs).is_err());
        }
        assert_eq!(
            flat(&[1.0, f64::NAN, 2.0], 1.0, 0.001),
            Err(TrendError::NotFinite { at: 1 })
        );
    }

    /// UNVERIFIED 15 (EQC471): at the soak's 26 values, a series with heavy ties (two levels: a kind
    /// that is full but for a few centuries) never reads p < 0.001 by the normal approximation when
    /// its exact p is 0.001 or more. The exact law of S for k low values among 26 is the
    /// Mann-Whitney count of (low, high) pairs in order, counted by recursion over the last value.
    #[test]
    fn the_normal_p_is_never_anticonservative_under_heavy_ties() {
        const N: usize = 26;
        // counts[k][m][u]: arrangements of k lows and m highs with u lows before a high.
        let mut counts = vec![vec![Vec::<f64>::new(); N + 1]; N + 1];
        for k in 0..=N {
            for m in 0..=N - k {
                counts[k][m] = if k == 0 || m == 0 {
                    vec![1.0]
                } else {
                    let mut c = vec![0.0; k * m + 1];
                    // The last value high: every low is before it.
                    for (u, x) in counts[k][m - 1].iter().enumerate() {
                        c[u + k] += x;
                    }
                    // The last value low: it is before no high.
                    for (u, x) in counts[k - 1][m].iter().enumerate() {
                        c[u] += x;
                    }
                    c
                };
            }
        }
        let mut conservative = 0;
        for (k, row) in counts.iter().enumerate().take(N / 2 + 1).skip(1) {
            let m = N - k;
            let law = &row[m];
            let total: f64 = law.iter().sum();
            for u in 0..=k * m {
                let s = 2 * u as i64 - (k * m) as i64;
                if s == 0 {
                    continue;
                }
                let exact: f64 = law
                    .iter()
                    .enumerate()
                    .filter(|(uu, _)| (2 * *uu as i64 - (k * m) as i64).abs() >= s.abs())
                    .map(|(_, x)| x)
                    .sum::<f64>()
                    / total;
                // A series with this S: the lows at the positions the count names, built greedily.
                let xs = series_with(k, m, u);
                let (s2, p) = mann_kendall(&xs).unwrap();
                assert_eq!(s2, s);
                assert!(
                    !(p < 0.001 && exact >= 0.001),
                    "k {k}, S {s}: normal {p}, exact {exact}"
                );
                if p >= 0.001 && exact < 0.001 {
                    conservative += 1;
                }
            }
        }
        // It errs the other way only: the study found 76 such values of S.
        assert_eq!(conservative, 76);
    }

    /// k lows (0.99) and m highs (1.0) with exactly u (low, high) pairs in order.
    fn series_with(k: usize, m: usize, mut u: usize) -> Vec<f64> {
        let mut xs = Vec::with_capacity(k + m);
        let (mut lows, mut highs) = (k, m);
        while lows + highs > 0 {
            // A low placed now is before every high still to come.
            if lows > 0 && u >= highs && (u - highs) <= (lows - 1) * highs {
                xs.push(0.99);
                u -= highs;
                lows -= 1;
            } else {
                xs.push(1.0);
                highs -= 1;
            }
        }
        assert_eq!(u, 0);
        xs
    }
}
