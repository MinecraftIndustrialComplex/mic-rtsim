// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ship-count sweep of the scale bench (SCALE.md §10; PLAN.md §10.1): merchant ships per
//! service, on a fixed ocean grid of ports, nobody watching. The contract is that an unobserved
//! merchant ship costs nothing: its position is a closed form of the clock, it books no voyage and
//! schedules no event, so the ships engine's CPU per sim day does not grow with how many there
//! are (only the pirates' few events a hunt remain), and its memory grows by one row per ship.
//!
//! Each point builds `rtsim_ships::synth::ocean_grid` with the service cap set to the swept value
//! (headways as short as the platform spacing allows), runs a warm-up, then times the ships'
//! handlers on their own clock with the world stepped apart, as the dragon sweep does.

use std::time::Instant;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_ships::{ShipTunables, synth};
use serde::{Deserialize, Serialize};

use super::scale::Measured;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShipsConfig {
    pub ships_per_service: u64,
    /// Ports on a side of the grid.
    pub side: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
}

fn cpu_since(cpu0: Option<u64>) -> Option<f64> {
    match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    }
}

pub fn run(c: &ShipsConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let tun = ShipTunables {
        max_ships_per_service: c.ships_per_service.max(1) as u32,
        headway_ticks: 1,
        ..ShipTunables::default()
    };
    let (mut w, mut s, _mask) = synth::ocean_grid(c.side as usize, c.seed, tun);
    let merchants = s
        .ships
        .iter()
        .filter(|(_, x)| x.service.is_some())
        .count()
        .max(1);
    s.run_until(&mut w, &mut NoInfrastructure, warm);
    let before = s.stats;
    sys::reset_heap_peak();
    let wall0 = Instant::now();
    let mut ships_ns = 0.0;
    let mut have_cpu = true;
    let mut t = warm;
    while t < end {
        let next = s.next_due().unwrap_or(end).clamp(t + 1, end);
        w.run_until(next);
        let c0 = sys::thread_cpu_ns();
        s.run_until(&mut w, &mut NoInfrastructure, next);
        match cpu_since(c0) {
            Some(ns) => ships_ns += ns,
            None => have_cpu = false,
        }
        t = next;
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let conservation = match s.check(&w) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let st = s.stats;
    if st.materialized > 0 || st.voyages_booked > 0 {
        failures.push("a ship materialized or a voyage was booked with nobody watching".into());
    }
    let pirate_kinds = ["decide", "reach_lane", "bored", "home", "rebuild"];
    if let Some((k, _)) = s
        .pending_by_kind()
        .into_iter()
        .find(|(k, _)| !pirate_kinds.contains(k))
    {
        failures.push(format!("a {k} event pending with nobody watching"));
    }
    let cpu = have_cpu.then_some(ships_ns);
    Measured {
        // The ships engine's own CPU per sim day (the world stepped apart).
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|c| c / (days * merchants as f64)),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: s.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: merchants as u64,
        caravans: merchants as u64,
        conservation,
        failures,
        state_hash: Some(s.state_hash() as u32),
        save_full_bytes: Some(s.save().len() as u64),
        ..Measured::default()
    }
}
