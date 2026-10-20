// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragon sweep of the scale bench (SCALE.md §10; PLAN.md §9): dragon count, with the world
//! fixed and nobody watching. The contract is a flat cost per unobserved dragon: what the dragons
//! add to a sim day (their own events, and the market events their raids cause) divided by their
//! number must not grow with how many there are.
//!
//! Each point runs a village grid (`rtsim_ecology::synth::grid`) with one dragon per lair spread
//! over its extent (`synth::bench_lairs`). Over the measured window the world is stepped to each
//! dragon event on its own clock and the dragons' handlers on theirs, so the per-dragon figure is
//! the dragons' own CPU (their decisions, legs, and the market events their raids cause), not
//! the world's own trade, which a subtraction of two runs would bury in noise at small counts.

use std::time::Instant;

use rtsim_ecology::engine::Dragons;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::Tick;
use serde::{Deserialize, Serialize};

use super::scale::Measured;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DragonConfig {
    pub dragons: u64,
    pub villages: u64,
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

pub fn run(c: &DragonConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let mut w = synth::grid(c.villages as usize, c.seed);
    let mut d = Dragons::new(c.seed, DragonTunables::default(), Tick::ZERO);
    let lairs = synth::bench_lairs(&w, c.dragons as usize, 1, c.seed);
    synth::populate(&mut w, &mut d, &lairs, Tick::ZERO);
    d.run_until(&mut w, &mut NoInfrastructure, warm);
    let before = d.stats;
    sys::reset_heap_peak();
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    let mut dragon_ns = 0.0;
    let mut have_cpu = cpu0.is_some();
    let mut t = warm;
    while t < end {
        let next = d.next_due().unwrap_or(end).clamp(t + 1, end);
        w.run_until(next);
        let c0 = sys::thread_cpu_ns();
        d.run_until(&mut w, &mut NoInfrastructure, next);
        match cpu_since(c0) {
            Some(ns) => dragon_ns += ns,
            None => have_cpu = false,
        }
        t = next;
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = cpu_since(cpu0);
    let days = c.days as f64;
    let n = c.dragons.max(1) as f64;
    let added = have_cpu.then_some(dragon_ns);
    let (rss, rss_peak) = sys::rss();
    let conservation = match d.check(&w, None) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    if d.stats.materialized > 0 {
        failures.push("a dragon materialized with nobody watching".into());
    }
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: added.map(|a| a / (days * n)),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: d.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: d.stats.events - before.events,
        reads: d.stats.decisions - before.decisions,
        conservation,
        failures,
        state_hash: Some(d.state_hash() as u32),
        ..Measured::default()
    }
}
