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
//!
//! **Paged** (`paged`, docs/choices-paging-modules.md): the herd rows, forage cells and region
//! index page through a store under a budget, stepped tick by tick as the game does, saved every
//! `save_every_ticks` (Minecraft's autosave) and whenever the rows are over the budget with every
//! resident page changed (`Herds::needs_save`: the host's extra save). The sweep measures the
//! resident row bytes against the world's growth, and the saves that keeping to the budget costs.

use std::sync::{Arc, RwLock};
use std::time::Instant;

use rtsim_ecology::herds::{Herds, synth};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
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
    /// Widen the valley with the herds (`synth::wide_valley`): a world that grows with them.
    #[serde(default)]
    pub wide: bool,
    /// Page the rows under a budget.
    #[serde(default)]
    pub paged: Option<HerdsPaging>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HerdsPaging {
    pub budget_bytes: u64,
    pub save_every_ticks: u64,
}

/// Saves of a paged run.
#[derive(Clone, Copy, Debug, Default)]
struct Saves {
    saves: u64,
    /// Saves the budget asked for (`Herds::needs_save`), beyond the autosaves.
    pressure: u64,
    cpu_ns: u64,
    bytes: u64,
    pages: u64,
}

struct Paged {
    store: Arc<RwLock<PageStore>>,
    dir: std::path::PathBuf,
    save_every: u64,
    saves: Saves,
}

impl Paged {
    fn save(&mut self, h: &mut Herds, pressure: bool) {
        let c0 = sys::thread_cpu_ns();
        let mut b = SaveBatch::new();
        let t = h.save_paged(&mut b, PageKey::new(0x4800, 0));
        let st = self
            .store
            .write()
            .expect("store lock")
            .commit(b, 0)
            .unwrap_or_else(|e| panic!("page store: {e}"));
        h.saved(t);
        h.enforce_budget();
        if let (Some(a), Some(b)) = (c0, sys::thread_cpu_ns()) {
            self.saves.cpu_ns += b - a;
        }
        self.saves.saves += 1;
        self.saves.pressure += u64::from(pressure);
        self.saves.bytes += st.segment_bytes + st.log_bytes;
        self.saves.pages += st.pages_written;
    }

    /// Tick by tick to `end`: the herds' events, then the autosave or the budget's save.
    fn run(&mut self, h: &mut Herds, legs: &mut LegIndex, end: Tick) {
        let mut t = h.now();
        while t < end {
            t += 1;
            h.run_until(legs, t);
            if t.0.is_multiple_of(self.save_every) {
                self.save(h, false);
            } else if h.needs_save() {
                self.save(h, true);
            }
        }
    }
}

fn scratch_dir() -> std::path::PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("herds-{}", std::process::id()))
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
    let valley = if c.wide {
        synth::wide_valley(c.herds as usize)
    } else {
        synth::Valley::default()
    };
    let mut h = synth::bench_herds_in(c.seed, c.herds as usize, c.members, Tick::ZERO, valley);
    let mut paged = c.paged.as_ref().map(|p| {
        let dir = scratch_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let store =
            PageStore::open(&dir, Durability::NoSync).unwrap_or_else(|e| panic!("page store: {e}"));
        let store = Arc::new(RwLock::new(store));
        h.attach_store(store.clone(), p.budget_bytes as usize);
        let mut pg = Paged {
            store,
            dir,
            save_every: p.save_every_ticks.max(1),
            saves: Saves::default(),
        };
        pg.save(&mut h, false);
        pg
    });
    match &mut paged {
        Some(p) => p.run(&mut h, &mut legs, warm),
        None => h.run_until(&mut legs, warm),
    }
    let before = h.stats;
    let saves0 = paged.as_ref().map(|p| p.saves).unwrap_or_default();
    h.reset_row_stats();
    sys::reset_heap_peak();
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    match &mut paged {
        Some(p) => p.run(&mut h, &mut legs, end),
        None => h.run_until(&mut legs, end),
    }
    let cpu = cpu_since(cpu0);
    let wall = wall0.elapsed().as_nanos() as f64;
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let rows = h.row_stats();
    let resident = h.rows_resident_bytes() as u64;
    let saves = paged.as_ref().map(|p| p.saves).unwrap_or_default();
    let save_cpu = (saves.cpu_ns - saves0.cpu_ns) as f64;
    let n_saves = (saves.saves - saves0.saves).max(1) as f64;
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
    if let Some(p) = &c.paged {
        // At a step boundary after a save, the budget holds: nothing is dirty.
        if let Some(pg) = &mut paged {
            pg.save(&mut h, false);
        }
        if h.rows_resident_bytes() as u64 > p.budget_bytes {
            failures.push(format!(
                "resident {} bytes over the budget {} after a save",
                h.rows_resident_bytes(),
                p.budget_bytes
            ));
        }
    }
    let st = h.stats;
    // Saves exclude their own CPU from the engine's.
    let cpu = cpu.map(|x| (x - save_cpu).max(0.0));
    // Before the hash and the full save, which load every page.
    let engine_heap_bytes = h.heap_bytes() as u64;
    if std::env::var_os("RTSIM_HEAP_PARTS").is_some() {
        eprintln!("heap parts: {:?}", h.heap_parts());
    }
    let state_hash = Some(h.state_hash() as u32);
    let save_full_bytes = Some(h.save().len() as u64);
    let paged_m = c.paged.as_ref().map(|p| (p, saves, saves0));
    if let Some(pg) = paged {
        let dir = pg.dir.clone();
        drop(pg);
        let _ = std::fs::remove_dir_all(dir);
    }
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|x| x / (days * c.herds.max(1) as f64)),
        catch_up_cpu_ns_per_read: read_cpu.map(|x| x / reads as f64),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: st.decisions - before.decisions,
        conservation,
        failures,
        state_hash,
        save_full_bytes,
        resident_bytes: paged_m.map(|_| resident),
        resident_peak_bytes: paged_m.map(|_| rows.peak_resident_bytes),
        budget_bytes: paged_m.map(|(p, _, _)| p.budget_bytes),
        loads_per_day: paged_m.map(|_| rows.loads as f64 / days),
        evictions_per_day: paged_m.map(|_| rows.evictions as f64 / days),
        over_budget_checks: paged_m.map(|_| rows.over_budget),
        save_cpu_ms_per_day: paged_m.map(|_| save_cpu / 1e6 / days),
        save_bytes_per_save: paged_m.map(|(_, s, s0)| (s.bytes - s0.bytes) as f64 / n_saves),
        saves_per_day: paged_m.map(|(_, s, s0)| (s.saves - s0.saves) as f64 / days),
        pressure_saves_per_day: paged_m.map(|(_, s, s0)| (s.pressure - s0.pressure) as f64 / days),
        ..Measured::default()
    }
}
