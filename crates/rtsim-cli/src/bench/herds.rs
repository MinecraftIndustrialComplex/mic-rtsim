// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herd sweeps of the scale bench (SCALE.md §10; PLAN.md §10.2): herd count, and members per
//! herd, in the synthetic valley (`rtsim_ecology::herds::synth::bench_herds`), nobody watching.
//!
//! **Contracts.** A herd costs a decision per sub-season (a bounded flow field), so the herds
//! engine's CPU per herd-day must not grow with how many herds there are; and members are latent
//! (deaths are dyadic counts, births one draw a year), so its CPU per sim day must not grow with
//! the members per herd, and neither must its memory. The one term that grows with members is a
//! big herd's grazing footprint, which is capped (`HerdSpeciesData::footprint_radius`), so it is
//! bounded, not flat; the members sweep measures whether it shows.
//!
//! Each point builds the herds, runs a warm-up, then times the herds engine alone over the
//! measured days (no world: herds need only the leg index), and reads every herd's counts after
//! a long unread gap to time the catch-up.

use std::time::Instant;

use rtsim_ecology::herds::synth;
use rtsim_kernel::clock::Tick;
use rtsim_travel::legs::LegIndex;
use serde::{Deserialize, Serialize};

use super::scale::Measured;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HerdsConfig {
    pub herds: u64,
    pub members: u64,
    pub warmup_days: u64,
    pub days: u64,
    /// Days left unread before the catch-up read.
    pub gap_days: u64,
    pub seed: u64,
}

fn cpu_since(cpu0: Option<u64>) -> Option<f64> {
    match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    }
}

pub fn run(c: &HerdsConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let mut legs = LegIndex::new(1_000);
    let mut h = synth::bench_herds(c.seed, c.herds as usize, c.members, Tick::ZERO);
    h.run_until(&mut legs, warm);
    let before = h.stats;
    sys::reset_heap_peak();
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    h.run_until(&mut legs, end);
    let cpu = cpu_since(cpu0);
    let wall = wall0.elapsed().as_nanos() as f64;
    // Catch-up: every herd's cohorts read after a long unread gap (pure reads, dyadic counts).
    let ids: Vec<_> = h.herds.iter().map(|(id, _)| id).collect();
    let later = end + c.gap_days * rtsim_kernel::clock::TICKS_PER_DAY;
    let reads = 64usize.max(ids.len());
    let c1 = sys::thread_cpu_ns();
    let mut sink = 0u64;
    for k in 0..reads {
        let id = ids[k % ids.len()];
        sink = sink.wrapping_add(h.cohorts(id, later + k as u64).iter().sum::<u64>());
    }
    let read_cpu = cpu_since(c1);
    std::hint::black_box(sink);
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let conservation = match h.check(end) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    let mut failures = Vec::new();
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    if h.stats.materialized > 0 || h.stats.clips > 0 {
        failures.push("a herd materialized or booked a leg with nobody watching".into());
    }
    let st = h.stats;
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|x| x / (days * c.herds.max(1) as f64)),
        catch_up_cpu_ns_per_read: read_cpu.map(|x| x / reads as f64),
        heap_live_bytes: sys::heap_live() as u64,
        heap_peak_bytes: sys::heap_peak() as u64,
        engine_heap_bytes: h.heap_bytes() as u64,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: st.decisions - before.decisions,
        conservation,
        failures,
        state_hash: Some(h.state_hash() as u32),
        save_full_bytes: Some(h.save().len() as u64),
        ..Measured::default()
    }
}
