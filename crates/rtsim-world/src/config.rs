// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world's settings. Every number is a PLACEHOLDER unless its comment cites a decision
//! or a derivation; the trade tunables are `rtsim_trade::Tunables` (P9), the knowledge ones
//! `rtsim_knowledge::Tunables` (P18).

use rtsim_settle::EdgeRule;
use rtsim_trade::build::Genesis;
use rtsim_trade::tunables::Ratio;

#[derive(Clone, Debug)]
pub struct WorldConfig {
    pub seed: u64,
    pub trade: rtsim_trade::Tunables,
    pub knowledge: rtsim_knowledge::Tunables,
    /// P18's recommended rule (overnight choice E2).
    pub edge_rule: EdgeRule,
    /// Caravan walking speed in blocks per tick. PLACEHOLDER: PLAN.md §8.4 says it is measured in
    /// a phase 0 GameTest of a real caravan walking, not guessed. 0.1 blocks a tick (2 a second)
    /// stands in until then.
    pub caravan_blocks_per_tick: f64,
    /// Days between scouts on a road with no flow, which carry news and nothing else. PLACEHOLDER
    /// (P18; overnight choice E10).
    pub scout_days: u64,
    /// Snapshots a market keeps in its news ring. PLACEHOLDER; neighbours are never evicted.
    pub news_cap: usize,
    /// Relayed snapshots a packet carries at most. PLACEHOLDER.
    pub relay_max: usize,
    /// How much worse a fully disliked player's prices are (`markup_for`). PLACEHOLDER (P18).
    pub markup_worst: Ratio,
    /// How the world starts.
    pub genesis: Genesis,
    /// A trader's lifespan once it becomes a notable, in days. PLACEHOLDER (P18).
    pub trader_lifespan_days: u64,
    /// Checks conservation after every event and panics on a break (PLAN.md §8.4: "asserted per
    /// event in debug"). On by default in debug builds, off in release: it costs a pass over the
    /// whole world per event.
    pub check_every_event: bool,
    /// Keeps whole pipeline histories (no trimming) so [`crate::World::audit`] can sum them. For
    /// debugging only: memory grows with time.
    pub keep_history: bool,
    /// A player's interest radius in blocks: what [`crate::World::visible_to`] looks around, and
    /// the radius its area is booked with in the leg index. PLACEHOLDER: 160 blocks, vanilla's
    /// default simulation distance of 10 chunks (PLAN.md §6.1: travellers appear at the edge of
    /// the entity-ticking area).
    pub view_blocks: u32,
    /// Sorcerers leave out a player at or below this sentiment (PLAN.md §8.8). PLACEHOLDER (P18):
    /// -25, one assault or a robbery heard as news.
    pub sorcerer_dislike_at_or_below: i8,
    /// A player at or above this is a friend of the village. PLACEHOLDER (P18): 25.
    pub sorcerer_friend_at_or_above: i8,
    /// Ribbit villages grow (P17).
    pub growth: bool,
    /// Days a ribbit village's stocks and treasury must stay at or above target before it books a
    /// house. PLACEHOLDER (P17, P18): 10.
    pub growth_after_days: u64,
    /// Residents a grown house brings. PLACEHOLDER (P17: "a new ribbit in it"): 1.
    pub growth_pop_per_house: u32,
    /// Most houses a village grows. PLACEHOLDER: 8, so growth is bounded (SCALE.md §6).
    pub growth_max_houses: u32,
    /// House templates to choose from: the 14 under `data/ribbits/structure/houses` (PLAN.md §8.8).
    pub house_templates: u32,
    /// Smallest ribbit village with a touring band (P16). PLACEHOLDER: 8 residents.
    pub band_min_pop: u32,
    /// Other villages a band's circuit visits. PLACEHOLDER: 3.
    pub band_stops: usize,
    /// Quanta a band stays at each stop, playing. PLACEHOLDER (P18's stay window): 6 hours.
    pub band_stay_quanta: u64,
    /// Ribbits in a band, its master included. PLACEHOLDER: 3.
    pub band_members: u8,
    /// A player base within this many blocks of a road becomes a stop on it (P30's
    /// recommendation). PLACEHOLDER: 256.
    pub base_stop_blocks: u32,
}

impl WorldConfig {
    pub fn new(seed: u64) -> Self {
        WorldConfig {
            seed,
            trade: rtsim_trade::Tunables::default(),
            knowledge: rtsim_knowledge::Tunables::default(),
            edge_rule: EdgeRule::P18,
            caravan_blocks_per_tick: 0.1,
            scout_days: 1,
            news_cap: 16,
            relay_max: 8,
            markup_worst: Ratio::new(1, 2),
            genesis: Genesis::AtTarget,
            trader_lifespan_days: 120,
            check_every_event: cfg!(debug_assertions),
            keep_history: false,
            view_blocks: 160,
            sorcerer_dislike_at_or_below: -25,
            sorcerer_friend_at_or_above: 25,
            growth: true,
            growth_after_days: 10,
            growth_pop_per_house: 1,
            growth_max_houses: 8,
            house_templates: 14,
            band_min_pop: 8,
            band_stops: 3,
            band_stay_quanta: 6,
            band_members: 3,
            base_stop_blocks: 256,
        }
    }
}
