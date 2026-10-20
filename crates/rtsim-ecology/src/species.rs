// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragon species and their data (PLAN.md §9). Ice and Fire CE 2.1-beta.1 supplies the bodies;
//! everything here that is not an Ice and Fire fact cited in a comment is a TUNABLE placeholder,
//! data per species, waiting on a soak or on Ben.

use rtsim_kernel::clock::TICKS_PER_DAY;
use serde::Serialize;

/// The three Ice and Fire dragons (`DragonBaseEntity` subclasses).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Species {
    Fire,
    Ice,
    Lightning,
}

impl Species {
    pub const ALL: [Species; 3] = [Species::Fire, Species::Ice, Species::Lightning];
    pub const COUNT: usize = 3;

    pub const fn index(self) -> usize {
        self as usize
    }

    /// The entity type id (verified: `iceandfire:fire_dragon`, `ice_dragon`, `lightning_dragon`).
    pub const fn entity_id(self) -> &'static str {
        match self {
            Species::Fire => "iceandfire:fire_dragon",
            Species::Ice => "iceandfire:ice_dragon",
            Species::Lightning => "iceandfire:lightning_dragon",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Species::Fire => "fire",
            Species::Ice => "ice",
            Species::Lightning => "lightning",
        }
    }

    pub fn from_index(i: u32) -> Option<Species> {
        Species::ALL.get(i as usize).copied()
    }
}

/// Ice and Fire's `Gender` NBT, a boolean that is true for a male (verified in
/// `DragonBaseEntity.addAdditionalSaveData`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Gender {
    Female,
    Male,
}

/// A life stage, 1 to 5 as Ice and Fire numbers them (`getDragonStage`: 25, 50, 75 and 100 days
/// begin stages 2 to 5, verified). Derived from age when read, never stepped (PLAN.md §5.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Stage(pub u8);

impl Stage {
    pub fn index(self) -> usize {
        usize::from(self.0.clamp(1, 5) - 1)
    }

    /// Hunts on its own: Ice and Fire dragons pick prey up from stage 2 (`doHurtTarget` needs
    /// `getDragonStage() > 1`, verified).
    pub fn hunts(self) -> bool {
        self.0 >= 2
    }

    /// Can breed: Ice and Fire's `canFallInLove` needs `isMature()`, stage 4 or more (verified).
    pub fn is_mature(self) -> bool {
        self.0 >= 4
    }
}

/// Everything per species. Every number is a TUNABLE PLACEHOLDER unless its comment cites a
/// verified fact or the pack's config.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpeciesData {
    /// In-game days at which stages 2, 3, 4 and 5 begin: 25, 50, 75 and 100, Ice and Fire's
    /// `getDragonStage` (verified).
    pub stage_days: [u64; 4],
    /// Sim lifespan in days (C24: one death event). Ice and Fire has no death of age (verified:
    /// no age-based death path in `IafDragonLogic`, and `getAgeInDays` stops at 128 days for a wild
    /// dragon), so the sim gives them a span so lineages turn over. TUNABLE, PLACEHOLDER 400.
    pub lifespan_days: u64,
    /// The absolute Y a dragon flies at while unloaded (veloren research §4: forced altitude, so
    /// it can never pop in underground). The plugin raises it above the terrain when it
    /// materializes. PLACEHOLDER 180.
    pub cruise_y: i32,
    /// Flight speed while unloaded, blocks per tick. PLACEHOLDER 0.5 (10 blocks a second).
    pub speed: f64,
    /// Hunting radius around the lair, by stage. PLACEHOLDER.
    pub territory: [u32; 5],
    /// How far a grudge sends it, blocks. PLACEHOLDER.
    pub grudge_range: u32,
    /// How close a target must be to be seen and fallen on, blocks: the pack's Ice and Fire
    /// `targetSearchLength` (128, from the pack config).
    pub reach: u32,
    /// How long a raid on a village lasts before it goes home, ticks. PLACEHOLDER 1,500.
    pub dwell_ticks: u64,
    /// How long it patrols a road, ticks: it falls on the caravans on the road when the patrol
    /// ends. PLACEHOLDER 6,000.
    pub patrol_ticks: u64,
    /// How long it stays when it materializes to harass a place before it gets bored (C24),
    /// ticks. PLACEHOLDER 6,000 (five real minutes).
    pub visit_ticks: u64,
    /// Chance per caravan passing within reach while it hunts that it is attacked, thousandths.
    /// PLACEHOLDER.
    pub catch_milli: i64,
    /// Share of an attacked caravan's cargo lost (eaten, burnt or carried off), thousandths.
    /// PLACEHOLDER.
    pub caravan_share_milli: i64,
    /// Share of a village's livestock taken in one raid at most, thousandths. PLACEHOLDER.
    pub village_share_milli: i64,
    /// Fullness points it can eat in one hunt at most. PLACEHOLDER 100 (a full belly).
    pub appetite: i32,
    /// The pack's Ice and Fire `dragon.maxHealth` (500, verified in `iaf-common.json`). A dragon's
    /// max health follows Ice and Fire's `refreshDirtyAttributes` rule: from 4% of it at day 0 up
    /// to all of it at day 125 (verified), see [`SpeciesData::max_health_at`].
    pub max_health: f32,
    /// Health regained per day while it roosts. PLACEHOLDER: a fifth of the maximum.
    pub regen_per_day_milli: i64,
    /// A raid on infrastructure: attack passes (the threat's count) and strength per pass, by
    /// stage (INDUSTRY.md §13.2: a flying threat). PLACEHOLDER.
    pub passes: [u32; 5],
    pub strength_per_pass: [f64; 5],
    /// Days between clutches for a mated pair in a lair (M1's recommendation). PLACEHOLDER 60.
    pub breed_days: u64,
    /// Days after hatching at which a young dragon leaves its parents' lair to claim a roost of
    /// its own. PLACEHOLDER 50 (it is then stage 3, a teen in Ice and Fire's terms).
    pub disperse_days: u64,
}

impl SpeciesData {
    pub fn of(s: Species) -> SpeciesData {
        let base = SpeciesData {
            stage_days: [25, 50, 75, 100],
            lifespan_days: 400,
            cruise_y: 180,
            speed: 0.5,
            territory: [0, 600, 1_500, 2_000, 2_500],
            grudge_range: 6_000,
            reach: 128,
            dwell_ticks: 1_500,
            patrol_ticks: 6_000,
            visit_ticks: 6_000,
            catch_milli: 800,
            caravan_share_milli: 700,
            village_share_milli: 100,
            appetite: 100,
            max_health: 500.0,
            regen_per_day_milli: 200,
            passes: [0, 1, 2, 3, 4],
            strength_per_pass: [0.0, 1.0, 2.0, 3.0, 4.0],
            breed_days: 60,
            disperse_days: 50,
        };
        match s {
            // No species differences are claimed as Ice and Fire facts; these only make two
            // species of the scenario behave differently. PLACEHOLDER.
            Species::Fire => base,
            Species::Ice => SpeciesData {
                speed: 0.45,
                visit_ticks: 4_000,
                ..base
            },
            Species::Lightning => SpeciesData {
                speed: 0.6,
                catch_milli: 900,
                ..base
            },
        }
    }

    /// The stage at an age in ticks.
    pub fn stage(&self, age_ticks: u64) -> Stage {
        let days = age_ticks / TICKS_PER_DAY;
        let s = 1 + self.stage_days.iter().filter(|&&d| days >= d).count();
        Stage(s as u8)
    }

    /// Ice and Fire's max health at an age: `round(min + (max - min) / 125 × min(days, 125))`
    /// with `min = 0.04 × max` (verified, `DragonBaseEntity.refreshDirtyAttributes`).
    pub fn max_health_at(&self, age_ticks: u64) -> f32 {
        let days = (age_ticks / TICKS_PER_DAY).min(125) as f32;
        let min = self.max_health * 0.04;
        (min + (self.max_health - min) / 125.0 * days).round()
    }

    pub fn lifespan_ticks(&self) -> u64 {
        self.lifespan_days * TICKS_PER_DAY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_follow_age() {
        let d = SpeciesData::of(Species::Fire);
        assert_eq!(d.stage(0), Stage(1));
        assert_eq!(d.stage(24 * TICKS_PER_DAY), Stage(1));
        assert_eq!(d.stage(25 * TICKS_PER_DAY), Stage(2));
        assert_eq!(d.stage(80 * TICKS_PER_DAY), Stage(4));
        assert_eq!(d.stage(10_000 * TICKS_PER_DAY), Stage(5));
        assert!(!Stage(1).hunts() && Stage(2).hunts());
        assert!(!Stage(3).is_mature() && Stage(4).is_mature());
        assert_eq!(d.max_health_at(0), 20.0);
        assert_eq!(d.max_health_at(125 * TICKS_PER_DAY), 500.0);
        assert_eq!(d.max_health_at(400 * TICKS_PER_DAY), 500.0);
    }
}
