// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every number the dragon module runs on that is not per species. Each is a TUNABLE
//! PLACEHOLDER unless its comment cites the pack's config, a verified Ice and Fire fact, or a
//! decision.

use rtsim_kernel::clock::TICKS_PER_DAY;
use serde::Serialize;

use crate::calendar::{Calendar, Season};
use crate::oil::OilTunables;
use crate::prey::PreyTable;
use crate::species::{Species, SpeciesData};

/// The kinds of prey a hungry dragon weighs (P12's recommendation, plus C16's infrastructure).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PreyKind {
    /// Caravans on a road: a hazard on the flow.
    Road,
    /// A village's livestock, counted as its market stock.
    Village,
    /// A player's home, only when a grudge sends it there: districts there are raided through
    /// the industry threat interface (C16 to C18).
    Grudge,
    /// A booked train leg a hungry dragon's flight crosses (INDUSTRY.md §13.3, I24).
    Train,
    /// A wild herd (`herds`, PLAN.md §10.2; P12's phase 2 note on a minimal wild-herd model).
    Herd,
}

impl PreyKind {
    pub const ALL: [PreyKind; 5] = [
        PreyKind::Road,
        PreyKind::Village,
        PreyKind::Grudge,
        PreyKind::Train,
        PreyKind::Herd,
    ];
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            PreyKind::Road => "road",
            PreyKind::Village => "village",
            PreyKind::Grudge => "grudge",
            PreyKind::Train => "train",
            PreyKind::Herd => "herd",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DragonTunables {
    pub species: [SpeciesData; Species::COUNT],
    pub calendar: Calendar,
    pub prey: PreyTable,
    /// Fullness is Ice and Fire's `Hunger` NBT: clamped to 0 to 100, higher is fuller, and it
    /// falls by one every `hungerTickRate` ticks while above 0 (verified, `IafDragonLogic`).
    pub full: i32,
    /// Ticks per fullness point lost: the pack's Ice and Fire `hungerTickRate` (3,000, verified in
    /// `iaf-common.json`).
    pub hunger_tick_rate: u64,
    /// A roosting dragon goes hunting when its fullness falls to this. Ice and Fire's own target
    /// goal hunts below 90 (verified, `DragonAITargetGoal`); the sim waits longer so a dragon hunts
    /// every few days, not every few hours. TUNABLE, PLACEHOLDER 60.
    pub hunt_below: i32,
    /// Hunts one bout may chain before the dragon goes home, however hungry. PLACEHOLDER 2.
    pub bout_hunts: u8,
    /// The least a dragon rests at its lair between bouts, ticks. PLACEHOLDER 12,000 (half a
    /// day): a dragon that ate little does not hunt again at once.
    pub rest_ticks: u64,
    /// An embodied dragon this hungry leaves to hunt (C24: hunger brings the departure
    /// forward). PLACEHOLDER 20.
    pub starving: i32,
    /// Season weights per prey kind, percent (indexed [season][kind]). The rule they encode is a
    /// PLACEHOLDER: in winter herds are penned near villages and easy to take, so villages weigh
    /// more; in summer they graze wide, so roads and wild herds weigh more.
    pub season_percent: [[i32; PreyKind::COUNT]; 4],
    /// Utility lost per block of flight to a target, in fullness points per 1,000 blocks.
    /// PLACEHOLDER 4.
    pub distance_cost_per_kblock: i32,
    /// A village's defenders (iron golems, guards): utility lost before temperament, in fullness
    /// points. Bold dragons discount it (C25). PLACEHOLDER 15.
    pub village_risk: i32,
    /// Sentiment at or below which a dragon holds a grudge (rtsim-knowledge's scale, -127 to
    /// 127). PLACEHOLDER -40.
    pub grudge_below: i8,
    /// A grudge's pull, in fullness points, scaled by how deep the grudge is. PLACEHOLDER 60.
    pub grudge_weight: i32,
    /// After going for a grudge (a raid, or a visit to a watched home), a dragon hunts for food
    /// for this long before the grudge pulls again; a starving dragon always eats first. TUNABLE,
    /// PLACEHOLDER 3 days.
    pub grudge_cooldown_ticks: u64,
    /// Districts one grudge raid attacks at most. PLACEHOLDER 2.
    pub raid_districts: usize,
    /// A wound's share of max health that doubles a report's severity (rtsim-knowledge's unit).
    /// PLACEHOLDER: a quarter of max health.
    pub wound_unit_milli: i64,
    /// How much a wound shortens the rest of a visit: remaining × (1 − k × damage / max health),
    /// k in thousandths. PLACEHOLDER 3,000 (a third of its health lost ends the visit).
    pub wound_shortens_milli: i64,
    /// Health a dragon loses in an abstract raid per share of its passes stopped by defences, as a
    /// share of max health, thousandths. PLACEHOLDER 400.
    pub raid_wound_milli: i64,
    /// Hunts in a row that found nothing before the dragon moves to another roost or ranges far
    /// (a migration). PLACEHOLDER 3.
    pub fruitless_before_move: u8,
    /// Days a roost must stand empty before a young dragon claims it (the respawn rule, P13).
    /// TUNABLE, PLACEHOLDER 20.
    pub respawn_delay_days: u64,
    /// Age in days of a dragon the respawn rule brings in. PLACEHOLDER 30 (stage 2: it grows into
    /// the roost).
    pub respawn_age_days: u64,
    /// How far away a respawned dragon comes from, blocks. PLACEHOLDER 3,000.
    pub respawn_from_blocks: u32,
    /// A player's interest area: what the bridge can materialize a dragon into, blocks. Roughly
    /// the entity-ticking distance; PLACEHOLDER 160.
    pub view_radius: u32,
    /// Strongest feelings a newcomer takes over from a lair it claims (C24: the lair is the
    /// heir of last resort). PLACEHOLDER 4, as rtsim-knowledge's `heir_sentiments`.
    pub lair_feelings: usize,
    /// P26's home counter: the half-life of time spent in a region, ticks. PLACEHOLDER 3 days.
    pub home_half_life_ticks: u64,
    /// Regions a player's home counter keeps at most. PLACEHOLDER 8.
    pub home_regions: usize,
    /// Settlements a dragon remembers preying on, for the thanks a slayer gets. PLACEHOLDER 4.
    pub victims_kept: usize,
    /// Utility lost to a wild herd's risk (a kick, a horn), fullness points. PLACEHOLDER 2.
    pub herd_risk: i32,
    /// Adopt (and so, per R18, discard) untagged dragons that spawned naturally or from a
    /// spawner. False leaves them alone: not adopted, not discarded, not tracked, plain Ice and
    /// Fire mobs exactly as without the mod, so In Control's desert fire dragons stay the oil
    /// progression gate (P13: In Control stays untouched until the sim version is measured to
    /// be at least as dangerous; OILGATE.md OG-1). TUNABLE, default false: set true only after
    /// the oil gate (OILGATE.md OG-10) measures at least as dangerous as In Control.
    pub adopt_natural_dragons: bool,
    /// The oil gate (OILGATE.md §4): the reserve, refuelling, field flights, sensing.
    pub oil: OilTunables,
}

impl Default for DragonTunables {
    fn default() -> Self {
        let pct = |road: i32, village: i32, herd: i32| [road, village, 100, 100, herd];
        let mut season_percent = [[100; PreyKind::COUNT]; 4];
        season_percent[Season::Spring.index()] = pct(100, 80, 110);
        season_percent[Season::Summer.index()] = pct(140, 50, 130);
        season_percent[Season::Autumn.index()] = pct(130, 60, 110);
        season_percent[Season::Winter.index()] = pct(50, 180, 70);
        DragonTunables {
            species: [
                SpeciesData::of(Species::Fire),
                SpeciesData::of(Species::Ice),
                SpeciesData::of(Species::Lightning),
            ],
            calendar: Calendar::default(),
            prey: PreyTable::default(),
            full: 100,
            hunger_tick_rate: 3_000,
            hunt_below: 60,
            bout_hunts: 2,
            rest_ticks: 12_000,
            starving: 20,
            season_percent,
            distance_cost_per_kblock: 4,
            village_risk: 15,
            grudge_below: -40,
            grudge_weight: 60,
            grudge_cooldown_ticks: 3 * TICKS_PER_DAY,
            raid_districts: 2,
            wound_unit_milli: 250,
            wound_shortens_milli: 3_000,
            raid_wound_milli: 400,
            fruitless_before_move: 3,
            respawn_delay_days: 20,
            respawn_age_days: 30,
            respawn_from_blocks: 3_000,
            view_radius: 160,
            lair_feelings: 4,
            home_half_life_ticks: 3 * TICKS_PER_DAY,
            home_regions: 8,
            victims_kept: 4,
            herd_risk: 2,
            adopt_natural_dragons: false,
            oil: OilTunables::default(),
        }
    }
}

impl DragonTunables {
    pub fn species(&self, s: Species) -> &SpeciesData {
        &self.species[s.index()]
    }

    pub fn season_weight(&self, season: Season, kind: PreyKind) -> i32 {
        self.season_percent[season.index()][kind.index()]
    }
}
