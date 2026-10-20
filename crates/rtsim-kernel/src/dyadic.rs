// SPDX-License-Identifier: AGPL-3.0-or-later
//! Random counts over any interval of slots, consistent for every reader, in O(log horizon) draws
//! whatever the interval's length (DECISIONS.md O5 and O17).
//!
//! # The problem
//!
//! SCALE.md §4's slot rule commits random processes once per slot, so a region left alone for n
//! slots pays n commits when it is read. That keeps the composition rule but makes catch-up linear
//! in elapsed time, which SCALE.md §10 forbids. One binomial draw over the whole gap would be O(1),
//! but its result would depend on where the reads fell.
//!
//! # The construction
//!
//! Fix a horizon of 2^L slots and a total N of points in it (a cohort's deaths, a Poisson stream's
//! arrivals). Every node of the dyadic tree over the horizon splits its points between its two
//! halves with one binomial draw, keyed by the node's position and by nothing else. Every node's
//! count is therefore a pure function of the stream key, so the count over `[a, b)` is the same for
//! every reader in every order: the composition rule holds by construction. Computing a prefix
//! count walks one root-to-leaf path, so any interval costs at most 2L draws, and fewer when the
//! counts reach zero early.
//!
//! This is valid when points are independent given the total: a Poisson process with a deterministic
//! rate ([`Split::Uniform`] for a constant rate), or a cohort whose members die independently at a
//! deterministic per-slot hazard ([`Split::Hazard`]). Then "a point in this node falls in its left
//! half" has a probability that depends only on the node, and the binomial split is exact, not an
//! approximation. A process whose rate depends on its own state (births in proportion to the adults
//! alive) is not of this kind; it must be restructured into such pieces or keep per-slot commits.

use crate::rng::{StreamKey, hazard_over};

/// How a node's points divide between its halves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Split {
    /// A constant rate: each point is equally likely to be in either half.
    Uniform,
    /// Each member dies in a slot with probability `p`, given it was alive at the slot's start. A
    /// death in a node falls in the left half with probability
    /// (1 - (1-p)^half) / (1 - (1-p)^size), whatever the node's start.
    Hazard(f64),
}

impl Split {
    fn left_fraction(self, half: u64, size: u64) -> f64 {
        match self {
            Split::Uniform => 0.5,
            Split::Hazard(p) => {
                let whole = hazard_over(p, size);
                if whole <= 0.0 {
                    0.5
                } else {
                    (hazard_over(p, half) / whole).clamp(0.0, 1.0)
                }
            }
        }
    }
}

/// A dyadic tree of counts over `2^levels` slots, keyed by a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DyadicCounts {
    key: StreamKey,
    levels: u32,
}

/// Slot 0 of the stream holds root draws; tree nodes use heap numbering from 1.
const ROOT_DRAW: u64 = 0;

impl DyadicCounts {
    pub fn new(key: StreamKey, levels: u32) -> Self {
        assert!((1..=62).contains(&levels), "levels = {levels}");
        Self { key, levels }
    }

    pub fn horizon(&self) -> u64 {
        1 << self.levels
    }

    /// The number of a cohort of `n` that die within the horizon at per-slot hazard `p`: the root
    /// total for [`Split::Hazard`]. Drawn once, from the stream's root slot.
    pub fn hazard_total(&self, n: u64, p: f64) -> u64 {
        self.key
            .slot(ROOT_DRAW)
            .binomial(n, hazard_over(p, self.horizon()))
    }

    /// Points of a Poisson process at `rate` per slot within the horizon: the root total for
    /// [`Split::Uniform`]. Drawn as Binomial(2^52, mean / 2^52), which is Poisson to within
    /// rounding at these sizes, so no second sampler is needed.
    pub fn poisson_total(&self, rate_per_slot: f64) -> u64 {
        const N: u64 = 1 << 52;
        let mean = rate_per_slot * self.horizon() as f64;
        assert!(
            mean >= 0.0 && mean < N as f64 / 2.0,
            "mean {mean} out of range"
        );
        self.key.slot(ROOT_DRAW).binomial(N, mean / N as f64)
    }

    /// Points in slots `[0, t)`, given `total` points in the whole horizon.
    pub fn prefix(&self, total: u64, split: Split, t: u64) -> u64 {
        let horizon = self.horizon();
        if t >= horizon {
            return total;
        }
        // Invariant: `acc` counts the points in [0, lo); `n` the points in [lo, lo + size); and
        // t lies in [lo, lo + size).
        let (mut node, mut lo, mut size, mut n, mut acc) = (1u64, 0u64, horizon, total, 0u64);
        while t > lo && n > 0 {
            let half = size / 2;
            let left = self
                .key
                .slot(node)
                .binomial(n, split.left_fraction(half, size));
            if t >= lo + half {
                acc += left;
                n -= left;
                lo += half;
                node = 2 * node + 1;
            } else {
                n = left;
                node *= 2;
            }
            size = half;
        }
        acc
    }

    /// Points in slots `[a, b)`.
    pub fn count(&self, total: u64, split: Split, a: u64, b: u64) -> u64 {
        if b <= a {
            return 0;
        }
        self.prefix(total, split, b) - self.prefix(total, split, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SystemId;
    use proptest::prelude::*;

    fn tree(entity: u64, levels: u32) -> DyadicCounts {
        DyadicCounts::new(
            StreamKey::new(11, SystemId::named("dyadic.test"), entity),
            levels,
        )
    }

    proptest! {
        /// Leaf counts sum to the total, prefixes are monotone, and any interval equals the sum of
        /// any split of it.
        #[test]
        fn counts_are_consistent(entity in any::<u64>(), total in 0u64..5000, p in 0.0f64..0.2, hazard in any::<bool>(), cuts in prop::collection::vec(0u64..64, 1..8)) {
            let d = tree(entity, 6);
            let split = if hazard { Split::Hazard(p) } else { Split::Uniform };
            let leaves: u64 = (0..64).map(|i| d.count(total, split, i, i + 1)).sum();
            prop_assert_eq!(leaves, total);
            let mut prev = 0;
            for t in 0..=64 {
                let x = d.prefix(total, split, t);
                prop_assert!(x >= prev);
                prev = x;
            }
            let mut cuts = cuts;
            cuts.push(0);
            cuts.push(64);
            cuts.sort_unstable();
            let pieces: u64 = cuts.windows(2).map(|w| d.count(total, split, w[0], w[1])).sum();
            prop_assert_eq!(pieces, total);
        }
    }

    /// Deaths in [0, t) from a cohort of n follow Binomial(n, 1 - (1-p)^t), as per-slot simulation
    /// would give: mean and variance over many independent cohorts.
    #[test]
    fn hazard_counts_have_the_right_distribution() {
        let (n, p, t) = (1000u64, 0.01f64, 70u64);
        let expect_p = hazard_over(p, t);
        let k = 4000u32;
        let xs: Vec<f64> = (0..k)
            .map(|e| {
                let d = tree(u64::from(e), 20);
                let total = d.hazard_total(n, p);
                d.prefix(total, Split::Hazard(p), t) as f64
            })
            .collect();
        let mean = xs.iter().sum::<f64>() / f64::from(k);
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / f64::from(k - 1);
        let (em, ev) = (n as f64 * expect_p, n as f64 * expect_p * (1.0 - expect_p));
        assert!(
            (mean - em).abs() < 5.0 * (ev / f64::from(k)).sqrt(),
            "mean {mean} vs {em}"
        );
        assert!((var / ev - 1.0).abs() < 0.1, "var {var} vs {ev}");
    }

    /// Poisson arrivals over [a, b) have mean and variance rate × (b - a).
    #[test]
    fn poisson_counts_have_the_right_distribution() {
        let (rate, a, b) = (0.37f64, 1000u64, 1500u64);
        let k = 4000u32;
        let xs: Vec<f64> = (0..k)
            .map(|e| {
                let d = tree(u64::from(e) + 1_000_000, 24);
                let total = d.poisson_total(rate);
                d.count(total, Split::Uniform, a, b) as f64
            })
            .collect();
        let mean = xs.iter().sum::<f64>() / f64::from(k);
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / f64::from(k - 1);
        let m = rate * (b - a) as f64;
        assert!(
            (mean - m).abs() < 5.0 * (m / f64::from(k)).sqrt(),
            "mean {mean} vs {m}"
        );
        assert!((var / m - 1.0).abs() < 0.1, "var {var} vs {m}");
    }
}
