// SPDX-License-Identifier: AGPL-3.0-or-later
//! The who-is-alive census (EQUILIBRIUM.md §4, slice S1): one row per kind of site, counted the same
//! way by every engine, so `rtsim-cli sim census`, the host soak and the later slices' tests read one
//! format.
//!
//! - **A site** is whatever renews: a dragon's lair, an oil nest, a herd's range, a pirate haven, a
//!   camp, a village. Each is in exactly one of SITE_STATE's four states (§2.2): broken (the site
//!   itself is gone or cannot renew, as a nest's broken heart), else empty (no members), else short
//!   (fewer members than its K), else full. A kind with no K yet counts its sites full or empty.
//! - **Members** are what the site holds now; **capacity** is the sum of the sites' K.
//! - **Mean P** is the mean over the kind's sites of the shared rule's pressure now
//!   ([`crate::renewal::Pressure::now`]); a kind with no pressure yet reads 0.
//! - **Renewals** are cumulative in the engine's counters; [`CensusLog`] prints each line's count
//!   since the kind's previous line.
//!
//! The text format is fixed-width with one line per kind per count, kinds in the engines' order, so
//! two runs diff line by line.

/// One kind's sites, counted at one time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KindCensus {
    /// `module.site`, as `dragon.lair` or `herd.range`.
    pub kind: &'static str,
    pub sites: u32,
    pub members: u64,
    /// The sites' K, summed (0 for a kind with no K yet).
    pub capacity: u64,
    pub short: u32,
    pub empty: u32,
    pub broken: u32,
    /// The sites' pressure now, summed ([`KindCensus::mean_p`]).
    pub p_sum: f64,
    /// Renewal events since the engine started (an arrival, a calving, a refit, a founding).
    pub renewals: u64,
}

impl KindCensus {
    pub fn new(kind: &'static str) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// Counts one site: its members, its K (0: none), whether it is broken, its pressure now.
    pub fn site(&mut self, members: u64, k: u64, broken: bool, p: f32) {
        self.sites += 1;
        self.members += members;
        self.capacity += k;
        self.p_sum += f64::from(p.max(0.0));
        if broken {
            self.broken += 1;
        } else if members == 0 {
            self.empty += 1;
        } else if members < k {
            self.short += 1;
        }
    }

    /// Sites neither broken, empty nor short.
    pub fn full(&self) -> u32 {
        self.sites - self.short - self.empty - self.broken
    }

    pub fn mean_p(&self) -> f64 {
        if self.sites == 0 {
            0.0
        } else {
            self.p_sum / f64::from(self.sites)
        }
    }

    /// Members over capacity (0 with no capacity).
    pub fn fill(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            self.members as f64 / self.capacity as f64
        }
    }
}

/// Prints census lines, remembering each kind's renewals so a line shows the count since the last.
#[derive(Clone, Debug, Default)]
pub struct CensusLog {
    last: Vec<(&'static str, u64)>,
}

impl CensusLog {
    /// The column heads, aligned with [`CensusLog::line`].
    pub const HEADER: &'static str =
        "    day kind            sites  members capacity  short  empty broken  mean_p renewals";

    pub fn new() -> Self {
        Self::default()
    }

    /// One kind's line at `day`; its renewals are those since this kind's previous line.
    pub fn line(&mut self, day: u64, c: &KindCensus) -> String {
        let renewals = match self.last.iter_mut().find(|(k, _)| *k == c.kind) {
            Some((_, last)) => {
                let n = c.renewals.saturating_sub(*last);
                *last = c.renewals;
                n
            }
            None => {
                self.last.push((c.kind, c.renewals));
                c.renewals
            }
        };
        format!(
            "{day:>7} {:<15} {:>5} {:>8} {:>8} {:>6} {:>6} {:>6} {:>7.3} {renewals:>8}",
            c.kind,
            c.sites,
            c.members,
            c.capacity,
            c.short,
            c.empty,
            c.broken,
            c.mean_p(),
        )
    }

    /// Every kind's line at `day`, in the order given.
    pub fn lines(&mut self, day: u64, cs: &[KindCensus]) -> Vec<String> {
        cs.iter().map(|c| self.line(day, c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_fall_in_one_state_each() {
        let mut c = KindCensus::new("oil.nest");
        c.site(2, 2, false, 0.0);
        c.site(1, 2, false, 0.5);
        c.site(0, 2, false, 1.0);
        c.site(2, 2, true, 0.0);
        c.site(3, 2, false, 0.0);
        assert_eq!(
            (c.sites, c.short, c.empty, c.broken, c.full()),
            (5, 1, 1, 1, 2)
        );
        assert_eq!((c.members, c.capacity), (8, 10));
        assert!((c.mean_p() - 0.3).abs() < 1e-9);
        // No K: full or empty.
        let mut v = KindCensus::new("village.human");
        v.site(7, 0, false, 0.0);
        v.site(0, 0, false, 0.0);
        assert_eq!((v.short, v.empty, v.full()), (0, 1, 1));
    }

    #[test]
    fn lines_count_renewals_since_the_last() {
        let mut log = CensusLog::new();
        let mut c = KindCensus::new("dragon.lair");
        c.site(1, 1, false, 0.0);
        c.renewals = 5;
        let a = log.line(24, &c);
        c.renewals = 7;
        let b = log.line(48, &c);
        assert_eq!(a.len(), CensusLog::HEADER.len());
        assert!(a.ends_with("       5"), "{a}");
        assert!(b.ends_with("       2"), "{b}");
        assert!(b.starts_with("     48 dragon.lair"), "{b}");
    }
}
