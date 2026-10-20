// SPDX-License-Identifier: AGPL-3.0-or-later
//! A herd's population as year-classes whose deaths are dyadic counts (DECISIONS.md O5's
//! recommendation; the kernel's `dyadic` module; PLAN.md §4.4).
//!
//! **Year-classes.** Every birth event makes one class: the members born that day, with member
//! indices `[first, first + born)` from the herd's monotonic next index (PLAN.md §4.4: a new member
//! never reuses an old identity). A member's age is the class's age, so its cohort (young, adult,
//! old) is a function of time, never a stored transition: the same ewe is a lamb, then a ewe, then
//! old, and never young again. The young/adult/old counts the rest of the module reads are sums
//! over classes.
//!
//! **Deaths are dyadic counts.** Within a cohort, members die independently at the species' fixed
//! daily hazard: randomness that does not depend on state, which is exactly what
//! [`DyadicCounts`] carries. A class's deaths from its anchor day to any day walk one tree per
//! cohort it passes through (at most three), each at most 2L binomial draws, so a read after one
//! day and a read after ten thousand cost the same (O5, flat catch-up). Nothing is committed per
//! slot: a read in the middle of nowhere sees the same numbers as an engine that stepped every
//! day.
//!
//! **What depends on state** (births in proportion to adults, starvation in proportion to the
//! shortfall, predators and players taking members) happens only at events: the yearly breeding
//! event, the sub-season decision and the takings. Each re-anchors the classes it touches at that
//! day: the deaths so far are committed to the ledger, the count drops by what was taken, and a
//! fresh tree (a new generation in its key) starts. Events happen at the same times however often
//! anything is read, so the composition rule holds.
//!
//! **Day slots.** Deaths of day d are counted from day d + 1 on. A re-anchor at any tick of day D
//! commits days before D and starts the new trees at D, so no day is lost or counted twice.

use rtsim_kernel::dyadic::{DyadicCounts, Split};
use rtsim_kernel::rng::{StreamKey, SystemId};

use super::species::{Cohort, HerdSpeciesData};

pub const SYS_DEATH: SystemId = SystemId::named("herds.death");

/// One year-class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// The first member index.
    pub first: u64,
    /// Members born into it: indices `[first, first + born)`.
    pub born: u32,
    /// The absolute day of its birth (may be before day 0 for founders).
    pub birth_day: i64,
    /// Deaths are counted from this day.
    pub anchor_day: i64,
    /// Alive at the start of `anchor_day`.
    pub alive: u32,
    /// Re-anchors so far: keys the trees, so every anchor draws afresh.
    pub generation: u32,
}

/// Work done by a read: tree levels walked, an upper bound on binomial draws / 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub segments: u32,
    pub levels: u32,
}

fn levels_for(len: i64) -> u32 {
    let len = len.max(2) as u64;
    (64 - (len - 1).leading_zeros()).clamp(1, 62)
}

impl Class {
    pub fn new(first: u64, born: u32, birth_day: i64, anchor_day: i64, alive: u32) -> Self {
        Class {
            first,
            born,
            birth_day,
            anchor_day,
            alive,
            generation: 0,
        }
    }

    pub fn end(&self) -> u64 {
        self.first + u64::from(self.born)
    }

    pub fn contains(&self, index: u64) -> bool {
        index >= self.first && index < self.end()
    }

    pub fn age_on(&self, day: i64) -> i64 {
        day - self.birth_day
    }

    pub fn cohort_on(&self, sp: &HerdSpeciesData, day: i64) -> Option<Cohort> {
        sp.cohort_at(self.age_on(day))
    }

    fn tree(&self, herd_seed: u64, cohort: Cohort, len: i64) -> DyadicCounts {
        let entity = StreamKey::new(herd_seed, SYS_DEATH, self.first).block(
            (u64::from(self.generation) << 8) | cohort.index() as u64,
            0,
        )[0];
        DyadicCounts::new(
            StreamKey::new(herd_seed, SYS_DEATH, entity),
            levels_for(len),
        )
    }

    /// Deaths in `[anchor_day, day)`, and the work the read took. Pure.
    pub fn deaths_until(
        &self,
        sp: &HerdSpeciesData,
        year_days: u32,
        herd_seed: u64,
        day: i64,
    ) -> (u32, Work) {
        let mut work = Work::default();
        let mut n = u64::from(self.alive);
        let mut acc = 0u64;
        let mut d = self.anchor_day;
        while n > 0 && d <= day {
            let Some(c) = self.cohort_on(sp, d) else {
                // The lifespan is reached on day d: every survivor dies that day.
                acc += n;
                break;
            };
            if d == day {
                break;
            }
            let seg_end = self.birth_day + sp.cohort_end_days(c);
            let len = seg_end - d;
            let tree = self.tree(herd_seed, c, len);
            let p = sp.daily_hazard(c, year_days);
            let total = tree.hazard_total(n, p);
            work.segments += 1;
            work.levels += levels_for(len);
            let upto = (day.min(seg_end) - d) as u64;
            let k = tree.prefix(total, Split::Hazard(p), upto);
            acc += k;
            if day < seg_end {
                break;
            }
            n -= k;
            d = seg_end;
        }
        (acc.min(u64::from(self.alive)) as u32, work)
    }

    /// Alive at the start of `day`.
    pub fn alive_on(&self, sp: &HerdSpeciesData, year_days: u32, herd_seed: u64, day: i64) -> u32 {
        if day <= self.anchor_day {
            return self.alive;
        }
        self.alive - self.deaths_until(sp, year_days, herd_seed, day).0
    }

    /// Re-anchors at `day`: returns the deaths committed (days before `day`).
    pub fn reanchor(
        &mut self,
        sp: &HerdSpeciesData,
        year_days: u32,
        herd_seed: u64,
        day: i64,
    ) -> u32 {
        if day <= self.anchor_day {
            return 0;
        }
        let (dead, _) = self.deaths_until(sp, year_days, herd_seed, day);
        self.alive -= dead;
        self.anchor_day = day;
        self.generation = self.generation.wrapping_add(1);
        dead
    }
}

/// Counts per cohort on `day`, summed over classes.
pub fn cohort_counts(
    classes: &[Class],
    sp: &HerdSpeciesData,
    year_days: u32,
    herd_seed: u64,
    day: i64,
) -> [u64; 3] {
    let mut out = [0u64; 3];
    for c in classes {
        if let Some(k) = c.cohort_on(sp, day) {
            out[k.index()] += u64::from(c.alive_on(sp, year_days, herd_seed, day));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herds::species::HerdSpecies;
    use proptest::prelude::*;

    fn sheep() -> HerdSpeciesData {
        HerdSpeciesData::of(HerdSpecies::Sheep)
    }

    proptest! {
        /// Pure reads never change a class, counts never rise with time, and nobody outlives the
        /// lifespan.
        #[test]
        fn reads_are_pure_and_monotone(seed in any::<u64>(), n in 0u32..20_000, birth in -2000i64..0, d1 in 0i64..2000, d2 in 0i64..2000) {
            let sp = sheep();
            let c = Class::new(0, n, birth, 0, n);
            let (a, b) = (d1.min(d2), d1.max(d2));
            let xa = c.alive_on(&sp, 96, seed, a);
            let xb = c.alive_on(&sp, 96, seed, b);
            prop_assert!(xb <= xa);
            // A pure read in between leaves the class as it was.
            let before = c.clone();
            let _ = c.alive_on(&sp, 96, seed, a);
            prop_assert_eq!(&before, &c);
            // Past the lifespan nobody is left.
            let end = (birth + i64::from(sp.lifespan_days)).max(1);
            prop_assert_eq!(c.alive_on(&sp, 96, seed, end), 0);
        }

        /// Committing at an event and reading on gives counts no greater than the start and
        /// consistent with the ledger: alive(anchor) + committed deaths = the start.
        #[test]
        fn reanchor_conserves(seed in any::<u64>(), n in 1u32..5_000, d1 in 1i64..800, d2 in 1i64..800) {
            let sp = sheep();
            let mut c = Class::new(7, n, -100, 0, n);
            let died = c.reanchor(&sp, 96, seed, d1);
            prop_assert_eq!(c.alive + died, n);
            let later = c.alive_on(&sp, 96, seed, d1 + d2);
            prop_assert!(later <= c.alive);
        }
    }

    /// The work of a read does not grow with the days since the anchor (O5): at most three
    /// segments of at most 2^L days each.
    #[test]
    fn catch_up_work_is_flat_in_elapsed_time() {
        let sp = sheep();
        let c = Class::new(0, 10_000, 0, 0, 10_000);
        let mut worst = 0;
        for days in [1i64, 10, 100, 300, 600, 900, 959] {
            let (_, w) = c.deaths_until(&sp, 96, 42, days);
            assert!(w.segments <= 3);
            worst = worst.max(w.levels);
        }
        assert!(worst <= 3 * 10, "levels {worst}");
    }

    /// Survival over a year matches the hazard: mean over many classes.
    #[test]
    fn survival_matches_the_hazard() {
        let sp = sheep();
        let n = 1_000u32;
        let k = 400u64;
        let mut sum = 0.0;
        for s in 0..k {
            let c = Class::new(0, n, -200, 0, n);
            sum += f64::from(c.alive_on(&sp, 96, s, 96));
        }
        let mean = sum / k as f64 / f64::from(n);
        assert!((mean - 0.94).abs() < 0.005, "adult survival {mean}");
    }
}
