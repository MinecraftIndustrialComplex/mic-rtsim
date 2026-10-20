// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every number the herds run on that is not per species. Each is a TUNABLE PLACEHOLDER unless its
//! comment cites a vanilla fact or a decision.

use rtsim_kernel::clock::TICKS_PER_DAY;
use serde::Serialize;

use super::route::Weights;
use super::species::{HerdSpecies, HerdSpeciesData};
use crate::calendar::Calendar;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HerdTunables {
    pub species: [HerdSpeciesData; HerdSpecies::COUNT],
    /// The ecology's calendar (P2 undecided: Serene Seasons' shape until then).
    pub calendar: Calendar,
    /// Share of a sub-season a herd may spend walking to its next target. PLACEHOLDER 0.6.
    pub travel_share: f64,
    /// A decision's flow field reaches at most this many cells from the herd. PLACEHOLDER 16
    /// (2,048 blocks): bounds a decision's cost.
    pub max_reach_cells: i32,
    /// Score lost per °C outside the comfort band. PLACEHOLDER 0.15.
    pub comfort_per_degree: f64,
    /// Score lost per 1,000 block-equivalents of route cost. PLACEHOLDER 0.08.
    pub travel_per_kblock: f64,
    /// Score lost near a cell another herd settled on this sub-season. PLACEHOLDER 0.4.
    pub claim_penalty: f64,
    /// Forage over demand saturates at this in a cell's score. PLACEHOLDER 1.5.
    pub food_cap: f64,
    /// Share of the forage above the floor a herd can eat in its footprint in one sub-season.
    /// PLACEHOLDER 0.8.
    pub graze_share: f64,
    /// Below this share of demand met, a herd starves (deaths at the species' starve hazard,
    /// scaled by the shortfall). PLACEHOLDER 0.6.
    pub starve_below: f64,
    /// Body condition moves this share of the way toward what grazing met, each sub-season.
    /// PLACEHOLDER 0.5.
    pub condition_rate: f64,
    /// Decisions are staggered by a hash of the herd over this many ticks after a sub-season
    /// begins (PLAN.md §4.3: nothing spikes). PLACEHOLDER a quarter day.
    pub stagger_ticks: u64,
    /// A player's interest area for herds: roughly the entity-ticking distance, blocks.
    /// PLACEHOLDER 160 (as the dragons').
    pub view_radius: u32,
    /// How far the footprint must reach into the area before the group materializes, blocks:
    /// members appear inside the area's edge, not on it. PLACEHOLDER 24.
    pub depth: u32,
    /// The group is taken back once the footprint is this far beyond the area. PLACEHOLDER 64
    /// (demotion waits a little so edge-walking does not thrash, PLAN.md §4.2).
    pub hysteresis: u32,
    /// Puppets in one herd's group at most. PLACEHOLDER 32 (PLAN.md §4.1's per-player cap per
    /// body kind).
    pub group_cap: u32,
    /// Herd puppets per player at most, all herds together. PLACEHOLDER 48.
    pub player_cap: u32,
    /// How often an embodied group is looked at (steering, leaving view), ticks. PLACEHOLDER 600.
    pub check_ticks: u64,
    /// Seen members a herd remembers at most; the least recently seen are forgotten first.
    /// PLACEHOLDER 1,024.
    pub seen_cap: usize,
    /// Extra cost of entering a cell per obstacle report at full failure. PLACEHOLDER 25.
    pub obstacle_step: f64,
    /// The most extra cost a cell can carry. PLACEHOLDER 400.
    pub obstacle_max: f64,
    /// Obstacle cost halves over this many ticks. PLACEHOLDER two sim years (192 days).
    pub obstacle_half_life_ticks: u64,
    /// A sheared sheep stays sheared this long, ticks: vanilla regrows wool when a sheep eats
    /// grass, which an adult tries one tick in 1,000 (`EatBlockGoal`, verified), so this is a
    /// PLACEHOLDER of a few minutes, 2,400.
    pub sheared_ticks: u64,
    /// A predator takes at most this share of a herd in one hunt, thousandths. PLACEHOLDER 10.
    pub predation_share_milli: u32,
    /// How close (blocks, beyond the footprint) a predator must be to the herd at the end of its
    /// hunt. PLACEHOLDER 256.
    pub predation_reach: u32,
    /// A predator that ate from a herd follows it this many times its usual range. PLACEHOLDER 3.
    pub follow_factor: f64,
    /// ... for this long after it last ate from it. PLACEHOLDER three sub-seasons (24 days).
    pub follow_ticks: u64,
    /// Followers a herd keeps at most. PLACEHOLDER 4.
    pub followers_cap: usize,
}

impl Default for HerdTunables {
    fn default() -> Self {
        HerdTunables {
            species: HerdSpecies::ALL.map(HerdSpeciesData::of),
            calendar: Calendar::default(),
            travel_share: 0.6,
            max_reach_cells: 16,
            comfort_per_degree: 0.15,
            travel_per_kblock: 0.08,
            claim_penalty: 0.4,
            food_cap: 1.5,
            graze_share: 0.8,
            starve_below: 0.6,
            condition_rate: 0.5,
            stagger_ticks: TICKS_PER_DAY / 4,
            view_radius: 160,
            depth: 24,
            hysteresis: 64,
            group_cap: 32,
            player_cap: 48,
            check_ticks: 600,
            seen_cap: 1_024,
            obstacle_step: 25.0,
            obstacle_max: 400.0,
            obstacle_half_life_ticks: 192 * TICKS_PER_DAY,
            sheared_ticks: 2_400,
            predation_share_milli: 10,
            predation_reach: 256,
            follow_factor: 3.0,
            follow_ticks: 24 * TICKS_PER_DAY,
            followers_cap: 4,
        }
    }
}

impl HerdTunables {
    pub fn species(&self, s: HerdSpecies) -> &HerdSpeciesData {
        &self.species[s.index()]
    }

    pub fn weights(&self) -> Weights {
        Weights {
            travel_share: self.travel_share,
            max_reach_cells: self.max_reach_cells,
            comfort_per_degree: self.comfort_per_degree,
            travel_per_kblock: self.travel_per_kblock,
            claim_penalty: self.claim_penalty,
            food_cap: self.food_cap,
            obstacle_half_life: self.obstacle_half_life_ticks,
        }
    }
}
