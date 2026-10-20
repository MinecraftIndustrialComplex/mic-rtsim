// SPDX-License-Identifier: AGPL-3.0-or-later
//! The world-model provider trait (PLAN.md §5.1's "world-model provider trait (sampled vanilla,
//! Deep Time reader)"): what modules may ask about the ground, water and climate.
//!
//! **Queries, never storage** (SCALE.md §2): a module asks about a block rectangle and the
//! provider answers from whatever it holds (the vanilla sampler's 64-block cells today, Deep
//! Time's baked tiles later, PLAN.md §11). So a Deep Time plugin replaces the sampler without a
//! module changing a line: it implements [`WorldModel`] and is handed to the modules instead.
//!
//! **Unknown ground** (X2-5): with lazy regions (SCALE.md §4) most of the world was never sampled.
//! Every answer says how much of the rectangle is known ([`Area::points`] against
//! [`Area::expected`]); modules treat unknown ground as impassable: herds never plan into it,
//! sea lanes never sail it, founding waits for it.

use crate::habitat::Habitat;

/// What a provider knows of a block rectangle. Point counts are the provider's samples whose
/// column lies inside the rectangle; the vanilla sampler has one per chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    /// Sample points the rectangle holds when everything is known.
    pub expected: u32,
    /// Sample points known.
    pub points: u32,
    pub land: u32,
    pub water: u32,
    pub ice: u32,
    pub fluid: u32,
    /// Lowest and highest top block over the known cells it touches (0 when none).
    pub min_y: i32,
    pub max_y: i32,
    /// Mean top block over the known points.
    pub mean_y: f32,
    /// Mean of the cells' year-round temperature, °C ([`WorldModel::season_offset`] adds the
    /// season); NaN when nothing is known.
    pub base_celsius: f32,
    /// Mean vegetation index (0 bare to 1 grassland) over the known land points; 0 with none.
    pub vegetation: f32,
    /// Mean downfall over the known points (vanilla's 0 to 1).
    pub downfall: f32,
}

impl Area {
    /// Every point of the rectangle is known.
    pub fn fully_known(&self) -> bool {
        self.expected > 0 && self.points >= self.expected
    }

    pub fn known_share(&self) -> f64 {
        if self.expected == 0 {
            0.0
        } else {
            f64::from(self.points) / f64::from(self.expected)
        }
    }

    pub fn water_share(&self) -> f64 {
        if self.points == 0 {
            0.0
        } else {
            f64::from(self.water) / f64::from(self.points)
        }
    }

    pub fn land_share(&self) -> f64 {
        if self.points == 0 {
            0.0
        } else {
            f64::from(self.land) / f64::from(self.points)
        }
    }

    /// The rectangle is known and all open water: what a sea lane may cross.
    pub fn all_water(&self) -> bool {
        self.fully_known() && self.water == self.points
    }
}

/// A creature kind's place on the ground of a rectangle: the input to founding herds (X2-12).
#[derive(Clone, Debug, PartialEq)]
pub struct Creatures {
    /// The body kind, an entity id (`minecraft:sheep`).
    pub body: String,
    /// Habitat in cells: the sum over known cells of the kind's spawn share × the cell's land
    /// share. A plains cell of all land gives a sheep about 0.26 (12 of vanilla plains' 46
    /// creature weight).
    pub habitat_cells: f64,
    /// Animals of the kind vanilla spawned there at chunk generation.
    pub generated: u32,
}

/// What every module may ask about the world. Implementations answer from what they hold and
/// never block; the same state always gives the same answer.
pub trait WorldModel: Send + Sync {
    /// A short name for logs: "vanilla sampler", "Deep Time".
    fn name(&self) -> &str;

    /// Rises whenever something a query could answer changes; a module caches against it.
    fn revision(&self) -> u64;

    /// Whether the provider knows anything of `frame` at all. A frame it has never heard of
    /// (a dimension nobody sampled, a test world fed by orders) lets a module keep its old,
    /// model-free behaviour there (X2-6).
    fn knows_frame(&self, frame: u16) -> bool;

    /// The ground of the block rectangle `[x0, x1) × [z0, z1)` in `frame` (Java's dimension id).
    fn area(&self, frame: u16, x0: i32, z0: i32, x1: i32, z1: i32) -> Area;

    /// °C the season adds to a base temperature in sub-season `sub` of the year (0 early spring
    /// to 11 late winter, the herds' calendar). 0 in a world without seasons.
    fn season_offset(&self, frame: u16, sub: u32) -> f32;

    /// The creature kinds the rectangle's ground supports, by body kind name order.
    fn creatures(&self, frame: u16, x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<Creatures>;

    /// The habitat cell holding block (x, z), for providers made of the vanilla sampler's cells
    /// (inspection and tests); `None` when unknown or the provider has no such cells.
    fn habitat_at(&self, frame: u16, x: i32, z: i32) -> Option<Habitat>;

    /// The vanilla sampler behind this provider, for the bridge's HABITAT records; a provider
    /// that is not fed by them answers `None`.
    fn sampled(&mut self) -> Option<&mut crate::sampled::SampledWorld> {
        None
    }

    /// Read-only view of [`WorldModel::sampled`].
    fn sampled_ref(&self) -> Option<&crate::sampled::SampledWorld> {
        None
    }
}

/// Vanilla's unitless biome temperature in °C, by Project Atmosphere's mapping as mic-climate
/// 0.2.0 uses it (`BiomeSeasonFallback`: -0.5 to 2.0 stretched over -20 to +56 °C), so the sim
/// and the pack's climate read one scale (X2-8). Plains (0.8) is 19.52 °C, a desert (2.0) 56 °C,
/// snowy plains (0.0) -4.8 °C.
pub fn vanilla_celsius(t: f32) -> f32 {
    -20.0 + 30.4 * (t.clamp(-0.5, 2.0) + 0.5)
}

/// mic-climate 0.2.0's default season offsets, °C (`ClimateConfig.defaultSeasonOffset`: spring
/// -2, summer +4, autumn -1, winter -8), by season of a four-season year. TUNABLE.
pub const SEASON_OFFSETS: [f32; 4] = [-2.0, 4.0, -1.0, -8.0];

/// The offset of season `season` of `count` (a four-season or twelve-sub-season calendar).
pub fn season_offset_of(season: u8, count: u8) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let s = usize::from(season) * 4 / usize::from(count).max(1);
    SEASON_OFFSETS[s.min(3)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_scale_matches_mic_climate() {
        assert!((vanilla_celsius(0.8) - 19.52).abs() < 1e-3);
        assert!((vanilla_celsius(2.0) - 56.0).abs() < 1e-3);
        assert!((vanilla_celsius(-0.7) + 20.0).abs() < 1e-3);
        assert_eq!(season_offset_of(0, 0), 0.0);
        assert_eq!(season_offset_of(3, 4), -8.0);
        assert_eq!(season_offset_of(4, 12), 4.0);
    }
}
