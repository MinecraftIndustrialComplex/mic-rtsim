// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 market sweeps of the scale bench: market count and edge count (SCALE.md §10;
//! economy research "Cost (ESTIMATE, to bench): per event O(log G + deg × G)").
//!
//! A grid of human and ribbit villages (`rtsim_world::synth::grid`) trades for a warm-up, then a
//! measured window. Cost per sim day is per event, so it grows with how many markets have events;
//! a world at rest costs nothing (the two-market test checks quiescence). The edge sweep varies
//! the edge rule's nearest-k with the market count fixed, so edges grow and so does the work per
//! event.

use std::time::Instant;

use rtsim_kernel::clock::Tick;
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};
use serde::{Deserialize, Serialize};

use super::scale::Measured;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketConfig {
    pub markets: u64,
    /// The edge rule's nearest-k.
    pub nearest: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
}

pub fn run(c: &MarketConfig) -> Measured {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut cfg = WorldConfig::new(c.seed);
    cfg.genesis = Genesis::Share(500);
    cfg.edge_rule.nearest = c.nearest as usize;
    let mut w = World::build(cfg, goods, book, synth::grid(c.markets as usize, c.seed));
    w.run_until(Tick::from_days(c.warmup_days));
    let before = w.stats;
    sys::reset_heap_peak();
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    w.run_until(Tick::from_days(c.warmup_days + c.days));
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    };
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let conservation = match w.check_conserved(w.now()) {
        Ok(()) => "ok".to_string(),
        Err(e) => e.to_string(),
    };
    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: w.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: w.stats.market_events - before.market_events,
        reads: w.stats.deliveries - before.deliveries,
        edges: Some(w.edges.len() as u64),
        conservation,
        failures,
        state_hash: Some(w.state_hash() as u32),
        ..Measured::default()
    }
}
