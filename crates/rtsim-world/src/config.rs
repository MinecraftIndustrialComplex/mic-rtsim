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
    /// Checks conservation after every event and panics on a break. For tests and debugging: it
    /// costs a pass over the whole world per event.
    pub check_every_event: bool,
    /// Keeps whole pipeline histories (no trimming) so [`crate::World::audit`] can sum them. For
    /// debugging only: memory grows with time.
    pub keep_history: bool,
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
            news_cap: 32,
            relay_max: 8,
            markup_worst: Ratio::new(1, 2),
            genesis: Genesis::AtTarget,
            trader_lifespan_days: 120,
            check_every_event: false,
            keep_history: false,
        }
    }
}
