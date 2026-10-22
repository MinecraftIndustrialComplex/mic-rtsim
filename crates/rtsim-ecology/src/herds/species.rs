// SPDX-License-Identifier: AGPL-3.0-or-later
//! Herd species and their data (PLAN.md §10.2). Bodies are vanilla animals for now (D4: nothing in
//! the core depends on a pack mod; Naturalist is a later body candidate, P8). Every number here
//! that is not cited as a vanilla 1.21.1 fact is a TUNABLE PLACEHOLDER, data per species.
//!
//! **Vanilla facts, checked in the 1.21.1 sources (NeoForm's decompiled `EntityType`, `Sheep`,
//! `Horse`, `Goat`, `AgeableMob`):**
//! - Entity types `minecraft:sheep` (box 0.9 × 1.3), `minecraft:cow` (0.9 × 1.4), `minecraft:pig`
//!   (0.9 × 0.9), `minecraft:horse` (1.3964844 × 1.6), `minecraft:goat` (0.9 × 1.3), all
//!   `MobCategory.CREATURE` with `clientTrackingRange(10)`.
//! - A natural sheep's colour (`Sheep.getRandomSheepColor`): `nextInt(100)` below 5 black, below
//!   10 gray, below 15 light gray, below 18 brown, else white, except one in 500 of those pink.
//!   NBT `Color` (a byte, `DyeColor` id: white 0, pink 6, gray 7, light gray 8, brown 12, black
//!   15) and `Sheared` (a boolean). A sheared sheep's wool comes back when it eats grass
//!   (`Sheep.ate`), and `EatBlockGoal` tries one tick in 1,000 for an adult.
//! - A horse's `Variant` int is `variant | markings << 8`; a spawned horse draws both uniformly
//!   (`Util.getRandom` over 7 colours and 5 markings) and its health, speed and jump from its own
//!   random.
//! - A goat screams with chance 0.02 at spawn (`IsScreamingGoat`); `HasLeftHorn` and
//!   `HasRightHorn` are booleans, true at spawn.
//! - Cows and pigs have no variants in 1.21.1.
//! - `AgeableMob`'s `Age` NBT is negative for a baby, starting at -24,000 ticks.
//!
//! Ice and Fire's food points for a living prey are `round(width × height × 10)` of its box
//! (`FoodUtils.getFoodPoints`, verified with the dragons, docs/overnight-choices-dragons.md), so a
//! dragon's fullness from each animal follows from the boxes above.

use serde::Serialize;

/// The vanilla herd animals (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum HerdSpecies {
    Sheep,
    Cow,
    Pig,
    Horse,
    Goat,
}

impl HerdSpecies {
    pub const ALL: [HerdSpecies; 5] = [
        HerdSpecies::Sheep,
        HerdSpecies::Cow,
        HerdSpecies::Pig,
        HerdSpecies::Horse,
        HerdSpecies::Goat,
    ];
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: u32) -> Option<HerdSpecies> {
        HerdSpecies::ALL.get(i as usize).copied()
    }

    /// The vanilla entity type id (verified in `EntityType`).
    pub const fn entity_id(self) -> &'static str {
        match self {
            HerdSpecies::Sheep => "minecraft:sheep",
            HerdSpecies::Cow => "minecraft:cow",
            HerdSpecies::Pig => "minecraft:pig",
            HerdSpecies::Horse => "minecraft:horse",
            HerdSpecies::Goat => "minecraft:goat",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            HerdSpecies::Sheep => "sheep",
            HerdSpecies::Cow => "cow",
            HerdSpecies::Pig => "pig",
            HerdSpecies::Horse => "horse",
            HerdSpecies::Goat => "goat",
        }
    }

    /// The entity's box, width × height (verified in `EntityType`).
    pub const fn body_box(self) -> (f32, f32) {
        match self {
            HerdSpecies::Sheep => (0.9, 1.3),
            HerdSpecies::Cow => (0.9, 1.4),
            HerdSpecies::Pig => (0.9, 0.9),
            HerdSpecies::Horse => (1.396_484_4, 1.6),
            HerdSpecies::Goat => (0.9, 1.3),
        }
    }

    /// Ice and Fire's food points for one adult: `round(width × height × 10)` (verified rule).
    pub fn food_points(self) -> i32 {
        let (w, h) = self.body_box();
        (w * h * 10.0).round() as i32
    }
}

/// A life stage of a member, from its age (never stepped).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Cohort {
    Young,
    Adult,
    Old,
}

impl Cohort {
    pub const ALL: [Cohort; 3] = [Cohort::Young, Cohort::Adult, Cohort::Old];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Cohort::Young => "young",
            Cohort::Adult => "adult",
            Cohort::Old => "old",
        }
    }
}

/// Everything per species. Every number is a TUNABLE PLACEHOLDER unless its comment cites a
/// vanilla fact.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HerdSpeciesData {
    pub species: HerdSpecies,
    /// Diet: forage eaten per animal per day, in forage units (one unit feeds a sheep for a day).
    /// PLACEHOLDER.
    pub intake_per_day: f64,
    /// Comfort band, °C: the herd seeks cells whose temperature this sub-season lies inside it.
    /// PLACEHOLDER.
    pub comfort: (f32, f32),
    /// Herd sizes the synthetic worlds found herds with, (least, most). PLACEHOLDER; the core
    /// takes any size.
    pub herd_size: (u32, u32),
    /// Migration speed, blocks per day of travel. PLACEHOLDER.
    pub speed_blocks_per_day: f64,
    /// Sub-seasons of the year (0 early spring to 11 late winter) in which young are born; births
    /// come once a year, at the first of them. PLACEHOLDER (sheep lamb in early spring).
    pub breeding_sub_seasons: Vec<u8>,
    /// Age at which a young animal is an adult, sim days. PLACEHOLDER: one sim year of Serene
    /// Seasons' default calendar (96 days). Vanilla babies grow up in 24,000 ticks; the sim's
    /// cohorts are its own clock.
    pub maturity_days: u32,
    /// Age at which an adult is old, sim days. PLACEHOLDER.
    pub old_days: u32,
    /// Lifespan, sim days: every member dies at this age at the latest. Vanilla animals never die
    /// of age; the sim gives them one so herds turn over. PLACEHOLDER.
    pub lifespan_days: u32,
    /// Chance of dying within a sim year, per cohort (young, adult, old), at a constant hazard.
    /// PLACEHOLDER.
    pub annual_mortality: [f64; 3],
    /// Young born per adult per year at full condition (both sexes counted). PLACEHOLDER.
    pub fertility: f64,
    /// Ground per animal when the herd grazes, square blocks: sets the footprint's radius.
    /// PLACEHOLDER.
    pub area_per_animal: f64,
    /// Chance of starving within a sub-season at no forage at all, scaled by the shortfall.
    /// PLACEHOLDER.
    pub starve_hazard: f64,
    /// How much predators prefer each cohort, relative weights. PLACEHOLDER: the young and the
    /// old are easier to take.
    pub predation_weight: [u32; 3],
}

/// Days in a sim year of Serene Seasons' default calendar (four 24-day seasons, P2), which the
/// placeholder ages below are written in.
const YEAR_DAYS: u32 = 96;

impl HerdSpeciesData {
    pub fn of(s: HerdSpecies) -> HerdSpeciesData {
        let base = HerdSpeciesData {
            species: s,
            intake_per_day: 1.0,
            comfort: (-5.0, 24.0),
            herd_size: (500, 12_000),
            speed_blocks_per_day: 400.0,
            breeding_sub_seasons: vec![0],
            maturity_days: YEAR_DAYS,
            old_days: 7 * YEAR_DAYS,
            lifespan_days: 10 * YEAR_DAYS,
            annual_mortality: [0.25, 0.06, 0.25],
            fertility: 0.55,
            area_per_animal: 150.0,
            starve_hazard: 0.2,
            predation_weight: [3, 1, 2],
        };
        match s {
            HerdSpecies::Sheep => base,
            HerdSpecies::Cow => HerdSpeciesData {
                intake_per_day: 4.0,
                comfort: (-8.0, 26.0),
                herd_size: (200, 6_000),
                speed_blocks_per_day: 300.0,
                breeding_sub_seasons: vec![1],
                old_days: 10 * YEAR_DAYS,
                lifespan_days: 15 * YEAR_DAYS,
                annual_mortality: [0.18, 0.04, 0.2],
                fertility: 0.4,
                area_per_animal: 700.0,
                ..base
            },
            HerdSpecies::Pig => HerdSpeciesData {
                intake_per_day: 1.5,
                comfort: (0.0, 30.0),
                herd_size: (50, 2_000),
                speed_blocks_per_day: 250.0,
                breeding_sub_seasons: vec![0, 6],
                old_days: 8 * YEAR_DAYS,
                lifespan_days: 12 * YEAR_DAYS,
                annual_mortality: [0.35, 0.08, 0.3],
                fertility: 1.2,
                area_per_animal: 200.0,
                ..base
            },
            HerdSpecies::Horse => HerdSpeciesData {
                intake_per_day: 5.0,
                comfort: (-10.0, 28.0),
                herd_size: (50, 3_000),
                speed_blocks_per_day: 900.0,
                breeding_sub_seasons: vec![2],
                old_days: 18 * YEAR_DAYS,
                lifespan_days: 25 * YEAR_DAYS,
                annual_mortality: [0.15, 0.03, 0.15],
                fertility: 0.3,
                area_per_animal: 900.0,
                ..base
            },
            HerdSpecies::Goat => HerdSpeciesData {
                intake_per_day: 1.0,
                comfort: (-15.0, 18.0),
                herd_size: (50, 4_000),
                speed_blocks_per_day: 350.0,
                breeding_sub_seasons: vec![1],
                old_days: 9 * YEAR_DAYS,
                lifespan_days: 12 * YEAR_DAYS,
                annual_mortality: [0.25, 0.06, 0.25],
                fertility: 0.7,
                area_per_animal: 150.0,
                ..base
            },
        }
    }

    /// The cohort at an age in days, or `None` past the lifespan.
    pub fn cohort_at(&self, age_days: i64) -> Option<Cohort> {
        if age_days < 0 || age_days >= i64::from(self.lifespan_days) {
            return None;
        }
        Some(if age_days < i64::from(self.maturity_days) {
            Cohort::Young
        } else if age_days < i64::from(self.old_days) {
            Cohort::Adult
        } else {
            Cohort::Old
        })
    }

    /// The age in days at which the cohort `c` ends (the next one begins, or death).
    pub fn cohort_end_days(&self, c: Cohort) -> i64 {
        i64::from(match c {
            Cohort::Young => self.maturity_days,
            Cohort::Adult => self.old_days,
            Cohort::Old => self.lifespan_days,
        })
    }

    /// The per-day hazard of a cohort, from its annual mortality: 1 − (1 − m)^(1 / year days).
    pub fn daily_hazard(&self, c: Cohort, year_days: u32) -> f64 {
        let m = self.annual_mortality[c.index()].clamp(0.0, 0.999_999);
        -((1.0 - m).ln() / f64::from(year_days.max(1))).exp_m1()
    }

    /// The radius of the ground a herd of `count` covers, blocks (PLACEHOLDER clamps).
    pub fn footprint_radius(&self, count: u64) -> u32 {
        let r = (count as f64 * self.area_per_animal / std::f64::consts::PI).sqrt();
        r.clamp(16.0, 1_536.0) as u32
    }

    /// Beverton-Holt's M over K (EQUILIBRIUM.md §3.2), from this species' own ages and
    /// mortality: with births `f x A / (1 + A/M)` a year from the adults A, the herd's
    /// post-calving steady state is K. Over a year of age at a time from birth, `l_n` is the
    /// expected years alive per newborn and `l_a` those as an adult (old animals do not breed);
    /// the steady state has `A* = M (f l_a - 1)` and a post-calving total of `A* l_n / l_a`, so
    /// `M = K l_a / (l_n (f l_a - 1))`. `None` when the species cannot replace itself at full
    /// condition (`f l_a <= 1`): no density term then.
    pub fn bh_m_per_k(&self, year_days: u32) -> Option<f64> {
        let yd = i64::from(year_days.max(1));
        let (mut l_n, mut l_a, mut s) = (0.0f64, 0.0f64, 1.0f64);
        let mut age = 0i64;
        while let Some(c) = self.cohort_at(age) {
            l_n += s;
            if c == Cohort::Adult {
                l_a += s;
            }
            s *= 1.0 - self.annual_mortality[c.index()];
            age += yd;
        }
        let r = self.fertility * l_a - 1.0;
        (r > 0.0 && l_n > 0.0).then(|| l_a / (l_n * r))
    }

    pub fn breeds_in(&self, sub_in_year: u32) -> bool {
        self.breeding_sub_seasons
            .first()
            .is_some_and(|&b| u32::from(b) == sub_in_year)
    }
}

// ---- identity rolls: what a member looks like, from its seed --------------------------------

/// A member's look, as vanilla would roll it, from the member's identity seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Look {
    /// A sheep's `Color` (DyeColor id).
    Sheep { color: u8 },
    /// A horse's `Variant` int: colour | markings << 8.
    Horse { variant: u32 },
    /// A goat's `IsScreamingGoat` (horns as spawned: both).
    Goat { screaming: bool },
    /// Cows and pigs have no variants in 1.21.1.
    Plain,
}

impl Look {
    /// The value sent as the member's variant (docs/boundary.md §8): the sheep's dye id, the
    /// horse's `Variant`, 1 for a screaming goat, else 0.
    pub fn variant(self) -> u32 {
        match self {
            Look::Sheep { color } => u32::from(color),
            Look::Horse { variant } => variant,
            Look::Goat { screaming } => u32::from(screaming),
            Look::Plain => 0,
        }
    }
}

/// Uniform in [0, n) from 64 bits (Lemire's multiply, no rejection: the bias is below 2^-50 for
/// these n).
fn below(x: u64, n: u64) -> u64 {
    ((u128::from(x) * u128::from(n)) >> 64) as u64
}

/// The look of a member with identity seed `seed`, with vanilla's own odds.
pub fn look(species: HerdSpecies, seed: u64) -> Look {
    let a = seed;
    let b = seed.rotate_left(29) ^ 0x9E37_79B9_7F4A_7C15;
    match species {
        HerdSpecies::Sheep => {
            // Sheep.getRandomSheepColor (verified): 5 black, 5 gray, 5 light gray, 3 brown, the
            // rest white but one in 500 of those pink.
            let i = below(a, 100);
            let color = if i < 5 {
                15
            } else if i < 10 {
                7
            } else if i < 15 {
                8
            } else if i < 18 {
                12
            } else if below(b, 500) == 0 {
                6
            } else {
                0
            };
            Look::Sheep { color }
        }
        HerdSpecies::Horse => {
            let colour = below(a, 7) as u32;
            let markings = below(b, 5) as u32;
            Look::Horse {
                variant: colour | (markings << 8),
            }
        }
        HerdSpecies::Goat => Look::Goat {
            screaming: below(a, 1_000_000) < 20_000,
        },
        HerdSpecies::Cow | HerdSpecies::Pig => Look::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_points_follow_ice_and_fire_rule() {
        assert_eq!(HerdSpecies::Sheep.food_points(), 12);
        assert_eq!(HerdSpecies::Cow.food_points(), 13);
        assert_eq!(HerdSpecies::Pig.food_points(), 8);
        assert_eq!(HerdSpecies::Horse.food_points(), 22);
        assert_eq!(HerdSpecies::Goat.food_points(), 12);
    }

    #[test]
    fn cohorts_follow_age() {
        let d = HerdSpeciesData::of(HerdSpecies::Sheep);
        assert_eq!(d.cohort_at(0), Some(Cohort::Young));
        assert_eq!(d.cohort_at(95), Some(Cohort::Young));
        assert_eq!(d.cohort_at(96), Some(Cohort::Adult));
        assert_eq!(d.cohort_at(7 * 96), Some(Cohort::Old));
        assert_eq!(d.cohort_at(10 * 96), None);
        let p = d.daily_hazard(Cohort::Adult, 96);
        assert!(((1.0 - p).powi(96) - 0.94).abs() < 1e-9);
    }

    #[test]
    fn sheep_colours_have_vanilla_odds() {
        let n = 200_000u64;
        let mut counts = [0u64; 16];
        for i in 0..n {
            let seed = rtsim_kernel::rng::StreamKey::new(
                1,
                rtsim_kernel::rng::SystemId::named("test.look"),
                i,
            )
            .block(0, 0)[0];
            if let Look::Sheep { color } = look(HerdSpecies::Sheep, seed) {
                counts[usize::from(color)] += 1;
            }
        }
        let share = |c: usize| counts[c] as f64 / n as f64;
        assert!((share(15) - 0.05).abs() < 0.003, "black {}", share(15));
        assert!((share(12) - 0.03).abs() < 0.003, "brown {}", share(12));
        assert!((share(0) - 0.82 * 499.0 / 500.0).abs() < 0.005);
        assert!(share(6) > 0.0 && share(6) < 0.004, "pink {}", share(6));
    }
}
