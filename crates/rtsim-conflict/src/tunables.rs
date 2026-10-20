// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine's culture-independent numbers. Every one is a TUNABLE PLACEHOLDER unless
//! its comment cites a fact or a decision; the culture-dependent ones are data
//! ([`crate::culture`]).

use serde::Serialize;

use crate::culture::SignalKind;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConflictTunables {
    /// Detection radius of a party's leg in the leg index, blocks. PLACEHOLDER 16.
    pub party_radius: u32,
    /// A player's interest radius when this engine books its own areas (the CLI's and tests'
    /// `observe_player`; the native engine shares one area per player, choice U2). PLACEHOLDER
    /// 160 blocks, vanilla's default simulation distance (PLAN.md §6.1).
    pub view_radius: u32,
    /// The raid director's spawn arc: its distance from the watching player, blocks. PLACEHOLDER
    /// 112: inside a 128-block entity-ticking edge so a wave spawns ticking (PLAN.md §6.1's "edge
    /// of the entity-ticking area").
    pub spawn_edge_blocks: u32,
    /// Half the arc's width, degrees. PLACEHOLDER 30.
    pub arc_half_width_deg: f32,
    /// Most members of one band in Minecraft at once (the spawn quota). PLACEHOLDER 16.
    pub spawn_quota: u32,
    /// Ticks between waves while a watched raid lasts. PLACEHOLDER 1,200 (a minute).
    pub wave_interval_ticks: u64,
    /// A watched raid's waves stop after this long and the survivors withdraw. PLACEHOLDER 6,000.
    pub watched_fight_ticks: u64,
    /// A village's defence per resident, in fight strength (the golems and guards a census
    /// implies). PLACEHOLDER 0.25.
    pub village_defence_per_pop: f64,
    /// A haven's defence per resident. PLACEHOLDER 1.0.
    pub haven_defence_per_pop: f64,
    /// An observed fight counts as comparable within this strength factor (as INDUSTRY.md I22's).
    /// PLACEHOLDER 2.0.
    pub comparable_strength_factor: f64,
    /// A destroyed block counts as this many items stolen for vengeance. PLACEHOLDER 4.
    pub block_value_items: f64,
    /// Vengeance takes away at most this share of a feud's grievance. PLACEHOLDER 0.5.
    pub vengeance_cap_share: f64,
    /// A feud at rung none, with no inflow, below this is forgotten. PLACEHOLDER 0.5.
    pub forget_below: f64,
    /// Chance in thousandths that a war band ambushes a booked train leg its march crosses.
    /// PLACEHOLDER 500.
    pub ambush_milli: u32,
    /// Rungs a feud drops when its war chief is killed. PLACEHOLDER 2.
    pub chief_rout_steps: u8,
    /// An envoy attacked counts as this many members killed, and the demand is refused at once.
    /// PLACEHOLDER 2.
    pub envoy_insult_kills: f64,
    /// A chief's sentiment toward an actor its faction raids, per raid (rtsim-knowledge's scale).
    /// PLACEHOLDER -40.
    pub raid_grudge: i16,
    /// An heir's inherited sentiment toward a player becomes this much grievance per point below
    /// zero when it takes over. PLACEHOLDER 0.5.
    pub heir_grudge_factor: f64,
    /// Villages within this many blocks that hate a faction hear when a player breaks one of its
    /// bands (a rescue report, C24). PLACEHOLDER 2,000.
    pub rescue_news_blocks: u32,
    /// Destroy pollution types to signals and the units per pollution point (I14). The type ids
    /// are Destroy's registry keys, UNVERIFIED (INDUSTRY.md names smog and radioactivity as chunk
    /// types, greenhouse, ozone depletion and acid rain as level types).
    pub pollution: Vec<(String, SignalKind, f64)>,
    /// Faction spatial index cell, blocks. PLACEHOLDER 1,024.
    pub cell_blocks: i32,
    /// Ticks a recalled band has to leave before the plugin despawns it. PLACEHOLDER 1,200.
    pub recall_timeout_ticks: u32,
    /// A player position older than this does not count as watching. PLACEHOLDER 2,400 (PLAYER
    /// records come every 20 ticks; scripted players report every 1,200).
    pub player_stale_ticks: u64,
    /// Of the attackers an unwatched district raid did not leave standing (walls, defenders), the
    /// share that falls; the rest straggle home. PLACEHOLDER 0.5.
    pub abstract_loss_share: f64,
}

impl Default for ConflictTunables {
    fn default() -> Self {
        ConflictTunables {
            party_radius: 16,
            view_radius: 160,
            spawn_edge_blocks: 112,
            arc_half_width_deg: 30.0,
            spawn_quota: 16,
            wave_interval_ticks: 1_200,
            watched_fight_ticks: 6_000,
            village_defence_per_pop: 0.25,
            haven_defence_per_pop: 1.0,
            comparable_strength_factor: 2.0,
            block_value_items: 4.0,
            vengeance_cap_share: 0.5,
            forget_below: 0.5,
            ambush_milli: 500,
            chief_rout_steps: 2,
            envoy_insult_kills: 2.0,
            raid_grudge: -40,
            heir_grudge_factor: 0.5,
            rescue_news_blocks: 2_000,
            pollution: vec![
                ("destroy:smog".into(), SignalKind::Smog, 1.0),
                ("destroy:radioactivity".into(), SignalKind::Radiation, 1.0),
                ("destroy:greenhouse".into(), SignalKind::Smog, 0.5),
                ("destroy:ozone_depletion".into(), SignalKind::Smog, 0.5),
                ("destroy:acid_rain".into(), SignalKind::Smog, 0.5),
            ],
            cell_blocks: 1_024,
            recall_timeout_ticks: 1_200,
            player_stale_ticks: 2_400,
            abstract_loss_share: 0.5,
        }
    }
}

impl ConflictTunables {
    /// The signal and scale a Destroy pollution type maps to.
    pub fn pollution_signal(&self, name: &str) -> Option<(SignalKind, f64)> {
        self.pollution
            .iter()
            .find(|p| p.0 == name)
            .map(|p| (p.1, p.2))
    }
}
