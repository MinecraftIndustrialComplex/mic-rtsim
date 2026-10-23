// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every number the ships engine uses. Each is a TUNABLE PLACEHOLDER unless its comment cites a
//! fact or a decision: no body mod is chosen yet (P8), so nothing here is measured in game.
//! Veloren figures (research/veloren-rtsim-2026-10-19.md §4) are cited where one was the model;
//! they are Veloren's, not Minecraft facts.

use rtsim_kernel::renewal::RenewalRule;
use rtsim_travel::Medium;

/// What differs between sea and sky ships.
#[derive(Clone, Debug, PartialEq)]
pub struct MediumTunables {
    /// Cruise speed, blocks per tick. PLACEHOLDER: sky 1.0 (20 blocks a second; Veloren's airship
    /// is nominally 23), sea 0.4.
    pub speed: f64,
    /// Departure and approach speed as a share of cruise speed. PLACEHOLDER 0.5.
    pub slow_share: f64,
    /// Blocks a ship covers leaving a dock (ascent for sky ships) and arriving (descent), each at
    /// most a third of the lane. PLACEHOLDER 64.
    pub harbour_blocks: u64,
    /// The longest lane, blocks along its path. PLACEHOLDER: sky 4,000, sea 6,000.
    pub max_lane_blocks: u64,
    /// A sea lane may be at most this many times the straight distance (thousandths).
    /// PLACEHOLDER 2,500 (2.5 times).
    pub max_stretch_milli: u64,
    /// A merchant ship's hold, chest slots. PLACEHOLDER: sky 27 (a chest), sea 54 (a double
    /// chest).
    pub hold_slots: u32,
    /// Merchant crew besides the ship. PLACEHOLDER: sky 3, sea 4.
    pub merchant_crew: u32,
    /// Pirate crew. PLACEHOLDER: sky 5, sea 6.
    pub pirate_crew: u32,
    /// Navy crew. PLACEHOLDER 6.
    pub navy_crew: u32,
    /// Body kinds (P15's dictionary names; the plugin maps them to a mod's entities, P8).
    pub merchant_body: &'static str,
    pub pirate_body: &'static str,
    pub navy_body: &'static str,
    pub merchant_crew_body: &'static str,
    pub pirate_crew_body: &'static str,
    pub navy_crew_body: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShipTunables {
    pub sea: MediumTunables,
    pub sky: MediumTunables,
    /// Ticks a ship stays at each dock. PLACEHOLDER 1,200 (Veloren: 60 seconds docked).
    pub dock_ticks: u64,
    /// Ticks between ships of one service, aimed for. PLACEHOLDER 4,800 (Veloren: 240 seconds
    /// apart).
    pub headway_ticks: u64,
    /// Ships one service runs at most. PLACEHOLDER 6.
    pub max_ships_per_service: u32,
    /// Platforms per dock. PLACEHOLDER 4 (Veloren: 4 platforms, so at most 4 routes a dock).
    pub berths: u32,
    /// Lanes a dock keeps at most, shortest first: with one loop through a dock per two lanes and
    /// two services per loop, this bounds services per dock by the berths. PLACEHOLDER 4.
    pub max_lanes_per_dock: usize,
    /// A loop is merged with a neighbour while both stay under this length, blocks.
    /// PLACEHOLDER 12,000.
    pub loop_target_blocks: u64,
    /// Sky cruise heights by service index (Veloren flies 400, 475, 550 and 625 by route index;
    /// these fit under the overworld's build limit of 320). PLACEHOLDER.
    pub cruise_y: [i32; 4],
    /// Sea ships' Y: vanilla's overworld sea level is 63.
    pub sea_level: i32,
    /// A sea port's dock snaps to water within this many mask cells. PLACEHOLDER 6.
    pub port_snap_cells: i32,
    /// Cells a lane search may expand. PLACEHOLDER 200,000.
    pub max_search_cells: usize,
    /// A player's interest radius, blocks: ships enter view at its edge. PLACEHOLDER 160 (vanilla's
    /// default simulation distance of 10 chunks, as `rtsim-world`'s).
    pub view_radius: u32,
    /// The radius ships' legs are booked with. PLACEHOLDER 24.
    pub body_radius: u32,

    // ---- Pirates (C24, C25) ----
    /// Rest at the haven between hunts, ticks. PLACEHOLDER one day.
    pub pirate_rest_ticks: u64,
    /// A hunt's length before the pirates get bored (C24), ticks. PLACEHOLDER three days.
    pub pirate_hunt_ticks: u64,
    /// Lanes a haven's pirates hunt lie within this many blocks of it. PLACEHOLDER 5,000.
    pub hunt_radius_blocks: i64,
    /// Hazard per unit of pirate strength on a lane, thousandths of each consignment.
    /// PLACEHOLDER 20 (a six-crew ship takes 12%).
    pub share_per_strength_milli: i64,
    /// The most a lane loses to pirates, thousandths. PLACEHOLDER 400.
    pub max_share_milli: i64,
    /// Fighting strength per crew member. PLACEHOLDER: pirates 1.0, merchants 0.5, navies 1.5.
    pub pirate_strength: f64,
    pub merchant_strength: f64,
    pub navy_strength: f64,
    /// Lanchester's effectiveness of each side (as `rtsim-industry`'s threat tunables).
    /// PLACEHOLDER 1.0 each.
    pub attacker_effectiveness: f64,
    pub defender_effectiveness: f64,
    /// A real fight vouches for an abstract one within this factor of its strength (as I22's).
    /// PLACEHOLDER 2.
    pub comparable_factor: f64,
    /// Real fights kept per class pair. PLACEHOLDER 16.
    pub observed_kept: usize,
    /// Of a beaten ship's cargo, the share pirates carry off when they win (thousandths), scaled by
    /// their surviving share. PLACEHOLDER 600.
    pub loot_share_milli: i64,
    /// Pirates who keep this share of their strength capture the ship. PLACEHOLDER 0.5.
    pub capture_at: f64,
    /// Of a captured merchant crew, the share that joins the pirates (M2's crew conversion).
    /// PLACEHOLDER 500 thousandths.
    pub crew_conversion_milli: u32,
    /// A beaten crew surrenders with this share still standing, so a capture leaves men to
    /// convert (the square law alone fights to the last). PLACEHOLDER 500 thousandths.
    pub surrender_share_milli: u32,
    /// A pirate's hold, chest slots: a full hold ends the hunt. PLACEHOLDER 27.
    pub pirate_hold_slots: u32,
    /// Ticks after a ship is seen before its pirates strike (they come into view). PLACEHOLDER 400.
    pub raid_delay_ticks: u64,
    /// A captain's lifespan (C24: notables age and die on one booked event). PLACEHOLDER 200 days.
    pub captain_lifespan_ticks: u64,
    /// Districts within this many blocks of a hunted lane's middle may be raided from it (C16,
    /// through the industry threat interface). PLACEHOLDER 384.
    pub coast_raid_blocks: i64,
    /// Chance in thousandths, at the middle temperament, that a hunt starts with a raid on such a
    /// district or a booked player leg on the lane. PLACEHOLDER 250, bent by Aggressive.
    pub coast_raid_milli: u32,

    // ---- Losses and respawns (unlike Veloren, killed crews come back) ----
    /// A lost merchant or navy ship is replaced this long after, at its home dock. PLACEHOLDER two
    /// days.
    pub respawn_ticks: u64,
    /// A haven refits an empty slot (EQUILIBRIUM.md §2.4.6, the shared rule of §2.2) after
    /// `min(cap, base x 2^P)` days, once its crew pool can man the ship. P is the haven's pressure:
    /// each pirate ship a player sinks or captures adds 1/slots. TUNABLE: base 4 (T27's rebuild
    /// delay), cap 96, half-life 96 (§2.4.6).
    pub refit: RenewalRule,
    /// A haven's crew pool holds at most this many crew (§2.4.6: three sea crews). TUNABLE 18.
    pub crew_pool_cap: u32,
    /// Hires (§2.4.6): drifters at the haven's market join its pool at this many a day toward the
    /// cap, logistic (`r (1 - pool/cap)`), read in closed form. Not slowed by pressure: the refit
    /// carries it. TUNABLE 0.25.
    pub hires_per_day: f64,
    /// What a pirate weighs a hunt with (§2.4.6 (b)): a lane is worth hunting when its win chance
    /// `p = A² / (A² + D²)`, against the defence it expects there (a merchant, the lane's patrols
    /// and escorts), makes `p x loot - (1 - p) x crew` positive. TUNABLE: the model's 3 and 6, a
    /// win chance above 2/3.
    pub hunt_loot_value: f64,
    pub hunt_crew_value: f64,

    // ---- Havens that rise and fall (EQUILIBRIUM.md §2.4.6a, slice S8b) ----
    /// Rules 1 to 5 of §2.4.6a (the toll, starving, laying up, shutting, refounding) are on. Off,
    /// the havens behave exactly as S8 left them, so a measurement can pair each seed with the S8
    /// behaviour. TUNABLE: on.
    pub lifecycle: bool,
    /// A haven is fed while one of its pirates began a hunt within this many days (§2.4.6a rule 2,
    /// PC2); starving, it hires nobody and its pool deserts at the hire rate. TUNABLE 30 (six
    /// rest-and-decide cycles: a lane escorted for a week does not starve a haven).
    pub starve_days: f64,
    /// A shut haven's site takes a new band after `min(cap, base x 2^P)` days, P its pressure
    /// (§2.4.6a rule 5, PC5, the camps' rule of §2.4.5); a look that finds a player in view of the
    /// dock or no lane worth hunting looks again `base` days later. TUNABLE: base 48, cap 384,
    /// half-life 96.
    pub refound: RenewalRule,
    /// Navies: ships per patrol. PLACEHOLDER 2.
    pub navy_ships: u32,
    /// Lanes a patrol covers: those within this many lane hops of its base. PLACEHOLDER 2.
    pub patrol_hops: u32,
}

impl ShipTunables {
    pub fn medium(&self, m: Medium) -> &MediumTunables {
        match m {
            Medium::Sky => &self.sky,
            _ => &self.sea,
        }
    }
}

impl Default for ShipTunables {
    fn default() -> Self {
        ShipTunables {
            sea: MediumTunables {
                speed: 0.4,
                slow_share: 0.5,
                harbour_blocks: 64,
                max_lane_blocks: 6_000,
                max_stretch_milli: 2_500,
                hold_slots: 54,
                merchant_crew: 4,
                pirate_crew: 6,
                navy_crew: 6,
                merchant_body: "mic_rtsim:ship/sea_merchant",
                pirate_body: "mic_rtsim:ship/sea_pirate",
                navy_body: "mic_rtsim:ship/sea_navy",
                merchant_crew_body: "mic_rtsim:crew/sailor",
                pirate_crew_body: "mic_rtsim:crew/pirate",
                navy_crew_body: "mic_rtsim:crew/marine",
            },
            sky: MediumTunables {
                speed: 1.0,
                slow_share: 0.5,
                harbour_blocks: 64,
                max_lane_blocks: 4_000,
                max_stretch_milli: 1_000,
                hold_slots: 27,
                merchant_crew: 3,
                pirate_crew: 5,
                navy_crew: 6,
                merchant_body: "mic_rtsim:ship/sky_merchant",
                pirate_body: "mic_rtsim:ship/sky_pirate",
                navy_body: "mic_rtsim:ship/sky_navy",
                merchant_crew_body: "mic_rtsim:crew/aeronaut",
                pirate_crew_body: "mic_rtsim:crew/sky_pirate",
                navy_crew_body: "mic_rtsim:crew/marine",
            },
            dock_ticks: 1_200,
            headway_ticks: 4_800,
            max_ships_per_service: 6,
            berths: 4,
            max_lanes_per_dock: 4,
            loop_target_blocks: 12_000,
            cruise_y: [200, 216, 232, 248],
            sea_level: 63,
            port_snap_cells: 6,
            max_search_cells: 200_000,
            view_radius: 160,
            body_radius: 24,
            pirate_rest_ticks: 24_000,
            pirate_hunt_ticks: 72_000,
            hunt_radius_blocks: 5_000,
            share_per_strength_milli: 20,
            max_share_milli: 400,
            pirate_strength: 1.0,
            merchant_strength: 0.5,
            navy_strength: 1.5,
            attacker_effectiveness: 1.0,
            defender_effectiveness: 1.0,
            comparable_factor: 2.0,
            observed_kept: 16,
            loot_share_milli: 600,
            capture_at: 0.5,
            crew_conversion_milli: 500,
            surrender_share_milli: 500,
            pirate_hold_slots: 27,
            raid_delay_ticks: 400,
            captain_lifespan_ticks: 200 * 24_000,
            coast_raid_blocks: 384,
            coast_raid_milli: 250,
            respawn_ticks: 2 * 24_000,
            refit: RenewalRule {
                base_days: 4.0,
                cap_days: 96.0,
                half_life_days: 96.0,
            },
            crew_pool_cap: 18,
            hires_per_day: 0.25,
            hunt_loot_value: 3.0,
            hunt_crew_value: 6.0,
            lifecycle: true,
            starve_days: 30.0,
            refound: RenewalRule {
                base_days: 48.0,
                cap_days: 384.0,
                half_life_days: 96.0,
            },
            navy_ships: 2,
            patrol_hops: 2,
        }
    }
}
