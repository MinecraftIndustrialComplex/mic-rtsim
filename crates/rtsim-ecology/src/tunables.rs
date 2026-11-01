// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every number the dragon module runs on that is not per species. Each is a TUNABLE
//! PLACEHOLDER unless its comment cites the pack's config, a verified Ice and Fire fact, or a
//! decision.

use rtsim_kernel::clock::TICKS_PER_DAY;
use rtsim_kernel::renewal::RenewalRule;
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
    /// More fear of a village's defenders per iron golem its census counts, fullness points,
    /// added to `village_risk` before boldness bends it (SINK.md §4.6). PLACEHOLDER 5.
    pub golem_risk: i32,
    /// The share of its works purse a tributary village pays its raider's lair each season,
    /// thousandths (SINK.md §4.6, MS-5). PLACEHOLDER 250: a quarter.
    pub tribute_share_milli: i64,
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
    /// Days a roost must stand empty before a young dragon claims it when no player has
    /// pressed it (the respawn rule, P13; EQUILIBRIUM.md §2.4.1's base). TUNABLE, 20.
    pub respawn_delay_days: u64,
    /// The longest a roost waits, however hard it was farmed (§2.4.1's cap). TUNABLE, 192.
    pub respawn_cap_days: u64,
    /// The half-life of a lair's pressure, days (§2.2's H, one game year). TUNABLE, 96.
    pub respawn_half_life_days: u32,
    /// Maintenance (§2.4.1, the dragon food gate, EQC531): a dragon older than
    /// `starve_leave_age_days` whose lair's fed share falls below this leaves (starve-leave),
    /// and a lair whose fed share is at or below it hatches nothing. TUNABLE, 0.35.
    pub starve_leave: f32,
    pub starve_leave_age_days: u64,
    /// A newcomer settles only where the hunting ground would feed it at least this share of
    /// its need, its share of every weight counted (§2.4.1's arrive-food, EQC533), else the lair
    /// checks again after `arrive_recheck_days`. TUNABLE, 0.5 and 24.
    pub arrive_food: f32,
    pub arrive_recheck_days: u64,
    /// The fed share's moving-average time constant, days. TUNABLE, 30.
    pub fed_tau_days: f32,
    /// What the arrival gate reads for a lair (§2.4.1 and §3.4, EQC533): the share of its need
    /// one more dragon would catch there, `W / (W + gate_half)`. W sums `gate_wild` (roads and
    /// stray prey), `gate_village` for each village in the nine regions around the lair and
    /// `(N - R)^2 / K` for each herd in them, each divided by one more than the occupied lairs
    /// whose nine regions hold it. TUNABLE, 150 (a village alone reads 0.47, short of
    /// `arrive_food`; the model's was 300 with no sharing), 30 and 200 (the model's).
    pub gate_village: f64,
    pub gate_wild: f64,
    pub gate_half: f64,
    /// A young dragon that has found no lair it may claim this many days after its dispersal
    /// age leaves the sim, off the map (EQC534): by then its lair's next clutch is due, so a
    /// lair holds its breeder (or pair) and at most one young. TUNABLE, 10 (it is then 60 days
    /// old, stage 3, Ice and Fire's teen).
    pub emigrate_after_days: u64,
    /// The oil row's weight a field (EQC561), for a species that drinks oil (`uses_oil`): shared
    /// by the mouths whose nine regions hold the field like a village's weight, it draws
    /// newcomers and keeps a dragon from starving out, and breeds none (births read the catch).
    /// At the gate's margin a mouth needs 200, so a field holds `oil_weight / 200` dragons in its
    /// nine regions on oil alone: 800 holds the 4.5 lairs of the real density's nine regions when
    /// the food holds one. TUNABLE.
    pub oil_weight: f64,
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
    /// Fire mobs exactly as without the mod (OILGATE.md OG-1, P13), except a fire one while
    /// field flights run, which is discarded whatever this says: the flights replace In
    /// Control's desert fire dragons (OGB-6, docs/choices-oilgate.md OGC66). TUNABLE, default
    /// false (OGC47).
    pub adopt_natural_dragons: bool,
    /// Home bodies a lair keeps at most: a pair's (EQUILIBRIUM.md S5, EQC106, H2). A third
    /// occupant stays a puppet. PLACEHOLDER 2.
    pub home_bodies_max: usize,
    /// Censuses that must miss a home body before it is written off, and the game days that must
    /// have passed since the first miss (the camps' numbers, EQC114). PLACEHOLDER 3 and 2.
    pub miss_writeoff: u8,
    pub miss_span_days: u64,
    /// A settled puppet this close to its lair horizontally, and within `hand_back_height`
    /// vertically, is handed back as a home body (EQC110). PLACEHOLDER 24 and 16 blocks.
    pub hand_back_blocks: f64,
    pub hand_back_height: f64,
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
            golem_risk: 5,
            tribute_share_milli: 250,
            grudge_below: -40,
            grudge_weight: 60,
            grudge_cooldown_ticks: 3 * TICKS_PER_DAY,
            raid_districts: 2,
            wound_unit_milli: 250,
            wound_shortens_milli: 3_000,
            raid_wound_milli: 400,
            respawn_delay_days: 20,
            respawn_cap_days: 192,
            respawn_half_life_days: 96,
            starve_leave: 0.35,
            starve_leave_age_days: 60,
            arrive_food: 0.5,
            arrive_recheck_days: 24,
            fed_tau_days: 30.0,
            gate_village: 150.0,
            gate_wild: 30.0,
            gate_half: 200.0,
            emigrate_after_days: 10,
            oil_weight: 800.0,
            respawn_age_days: 30,
            respawn_from_blocks: 3_000,
            view_radius: 160,
            lair_feelings: 4,
            home_half_life_ticks: 3 * TICKS_PER_DAY,
            home_regions: 8,
            victims_kept: 4,
            herd_risk: 2,
            adopt_natural_dragons: false,
            home_bodies_max: 2,
            miss_writeoff: 3,
            miss_span_days: 2,
            hand_back_blocks: 24.0,
            hand_back_height: 16.0,
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

    /// The lairs' renewal rule (EQUILIBRIUM.md §2.4.1: base 20, cap 192, H 96).
    pub fn renewal(&self) -> RenewalRule {
        RenewalRule {
            base_days: self.respawn_delay_days as f32,
            cap_days: self.respawn_cap_days as f32,
            half_life_days: self.respawn_half_life_days as f32,
        }
    }
}
