// SPDX-License-Identifier: AGPL-3.0-or-later
//! The faction-count sweep of the scale bench (SCALE.md §10; PLAN.md §10.3): factions with one
//! village per camp (a fixed density), nobody watching. The contract is a flat cost per faction: a faction costs its
//! events (a feud's next rung or fall is one booked event, a plunder is a muster, a march, a raid
//! and a homecoming), never a tick, so the conflict engine's own CPU per sim day divided by the
//! faction count must not grow with how many there are, and its memory grows by rows.
//!
//! Each point builds `rtsim_conflict::synth::many`: goblins, bandits and raiders in turn on a grid
//! between the villages, each with a smog source inside its territory (the settlers') and a
//! player's logging in it, so every faction carries two feuds and plunders what it hates. Over the
//! measured window the world is stepped to each conflict event apart, so the per-faction figure is
//! the conflict's own CPU (its decisions, legs, and the market transfers its raids cause).

use std::time::Instant;

use rtsim_conflict::synth;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use serde::{Deserialize, Serialize};

use super::scale::Measured;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactionConfig {
    pub factions: u64,
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

pub fn run(c: &FactionConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let (mut w, mut k) = synth::many(c.villages as usize, c.factions as usize, c.seed);
    k.run_until(&mut w, &mut NoInfrastructure, warm);
    let _ = k.take_commands();
    let before = k.stats;
    sys::reset_heap_peak();
    let wall0 = Instant::now();
    let mut ns = 0.0;
    let mut have_cpu = true;
    let mut t = warm;
    while t < end {
        let next = k.next_due().unwrap_or(end).clamp(t + 1, end);
        w.run_until(next);
        let c0 = sys::thread_cpu_ns();
        k.run_until(&mut w, &mut NoInfrastructure, next);
        match cpu_since(c0) {
            Some(n) => ns += n,
            None => have_cpu = false,
        }
        let _ = k.take_commands();
        t = next;
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let conservation = match k.check(&w) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let st = k.stats;
    if st.materialized > 0 {
        failures.push("a party materialized with nobody watching".into());
    }
    let n = c.factions.max(1) as f64;
    let cpu = have_cpu.then_some(ns);
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|c| c / (days * n)),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: k.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: c.factions,
        caravans: k.parties.len() as u64,
        conservation,
        failures,
        state_hash: Some(k.state_hash() as u32),
        save_full_bytes: Some(k.save().len() as u64),
        ..Measured::default()
    }
}
