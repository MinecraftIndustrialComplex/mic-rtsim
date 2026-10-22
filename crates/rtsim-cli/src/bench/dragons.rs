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
//!
//! **Paged** (A16; docs/choices-paging-modules.md, Dragons): with a budget, the dragons' and
//! lairs' rows page to a page store in a scratch folder under that budget, with a save (meta page
//! and dirty row pages, fsynced) every `save_every` ticks and the budget enforced every
//! [`STEP_TICKS`], as a host would at its step boundaries. The run reports the rows' resident
//! bytes at their peak (between saves, when the rows changed since the last save are pinned, at
//! most the budget plus what one step changes, because a step boundary over the budget saves)
//! and after the last save, the saves a day (and of them the budget's), and the whole heap, whose
//! slope is what stays resident (the wheel, the lair indexes, and the phase 1 world under the
//! dragons).

use std::sync::{Arc, RwLock};
use std::time::Instant;

use rtsim_ecology::boundary::{self, Obs, OilChunkRec, OilChunks};
use rtsim_ecology::engine::Dragons;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_world::World;
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
    /// Paged runs: the row tables' budget in bytes (`None`: resident rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_bytes: Option<u64>,
    /// Paged runs: ticks between saves (Minecraft's autosave: 6,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_every: Option<u64>,
    /// The oil gate on (OILGATE.md): fields over the grid from the start, so fire dragons burn,
    /// wake low and fly to drink.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub oil: bool,
}

/// Oil fields of 3 by 3 chunks over `w`'s extent, one per eight villages, reported as Deep Time's
/// traps at the start (the gate opens with them).
fn add_oil(w: &mut World, d: &mut Dragons, seed: u64) {
    let (mut x0, mut x1, mut z0, mut z1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for n in &w.nodes {
        x0 = x0.min(n.pos.x);
        x1 = x1.max(n.pos.x);
        z0 = z0.min(n.pos.z);
        z1 = z1.max(n.pos.z);
    }
    let key = StreamKey::new(seed, SystemId::named("bench.oil"), 0);
    let mut chunks = Vec::new();
    for k in 0..(w.nodes.len() / 8).max(1) as u64 {
        let mut r = key.slot(k);
        let cx = (x0 + r.below((x1 - x0).max(1) as u64) as i32) >> 4;
        let cz = (z0 + r.below((z1 - z0).max(1) as u64) as i32) >> 4;
        for j in 0..9 {
            chunks.push(OilChunkRec {
                cx: cx + j % 3,
                cz: cz + j / 3,
                amount: 1,
                flags: 0,
            });
        }
    }
    for part in chunks.chunks(boundary::OIL_CHUNKS_MAX) {
        let o = Obs::OilChunks(OilChunks {
            dimension: 0,
            source: boundary::OIL_SOURCE_DEEP_TIME,
            chunks: part.to_vec(),
        });
        d.ingest(w, &mut NoInfrastructure, &o, Tick::ZERO);
    }
}

/// The paged sweeps' world: two dragons (lairs) per village, so density stays at the fixed-grid
/// sweep's top point (512 dragons on 256 villages, about two lairs per region, Ice and Fire's own
/// spacing) as the count grows.
pub fn grown_villages(dragons: u64) -> u64 {
    (dragons / 2).max(256)
}

/// The paged sweeps' budget for both row tables. PLACEHOLDER 256 KiB: a few hundred dragons'
/// rows, so every point pages.
pub const PAGED_BUDGET: u64 = 256 << 10;

/// The step a host enforces the budget at: a second of game time. PLACEHOLDER 20 ticks.
pub const STEP_TICKS: u64 = 20;

/// Where a paged host would put the dragons' meta page (rtsim-jni's dragons page today).
const META: PageKey = PageKey::new(0x4400, 0);

fn cpu_since(cpu0: Option<u64>) -> Option<f64> {
    match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    }
}

pub fn run(c: &DragonConfig) -> Measured {
    if let Some(budget) = c.budget_bytes {
        return run_paged(c, budget as usize, c.save_every.unwrap_or(6_000).max(1));
    }
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let mut w = synth::grid(c.villages as usize, c.seed);
    let mut d = Dragons::new(c.seed, DragonTunables::default(), Tick::ZERO);
    let lairs = synth::bench_lairs(&w, c.dragons as usize, 1, c.seed);
    synth::populate(&mut w, &mut d, &lairs, Tick::ZERO);
    if c.oil {
        add_oil(&mut w, &mut d, c.seed);
    }
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
    // The rows' heap is measured at the step boundary (`enforce_budget`; resident, it evicts
    // nothing).
    d.enforce_budget();
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

#[derive(Default)]
struct Saves {
    saves: u64,
    /// Saves the budget asked for (`Dragons::needs_save`), beyond the autosaves.
    pressure: u64,
    cpu_ns: u64,
    wall_ns: u64,
    bytes: u64,
    log_bytes: u64,
    pages: u64,
    checkpoints: u64,
}

/// One save as a paged host makes it at the autosave, or sooner when the budget asks (`pressure`):
/// the meta and the dirty rows in a batch, the commit (fsynced), the tickets back, the budget.
fn save(d: &mut Dragons, store: &RwLock<PageStore>, s: &mut Saves, pressure: bool) {
    let (c0, w0) = (sys::thread_cpu_ns(), Instant::now());
    let mut b = SaveBatch::new();
    let tickets = d.save_paged(&mut b, META);
    let st = store
        .write()
        .expect("the store")
        .commit(b, 0)
        .unwrap_or_else(|e| panic!("dragons bench: a save failed: {e}"));
    d.saved(tickets);
    d.enforce_budget();
    s.saves += 1;
    s.pressure += u64::from(pressure);
    s.wall_ns += w0.elapsed().as_nanos() as u64;
    if let Some(ns) = cpu_since(c0) {
        s.cpu_ns += ns as u64;
    }
    s.bytes += st.segment_bytes + st.log_bytes;
    s.log_bytes += st.log_bytes;
    s.pages += st.pages_written + st.pages_deleted;
    s.checkpoints += u64::from(st.checkpoint_bytes > 0);
}

/// What the stepping loop measured: the dragons' own CPU and the row bytes' peak.
#[derive(Default)]
struct Window {
    dragon_ns: f64,
    have_cpu: bool,
    peak: usize,
}

/// Runs to `end` as a paged host would: the world and the dragons stepped event by event, the
/// budget at every step boundary, a save at every `save_every`, and an extra save at a boundary
/// where the rows changed since the last save fill the budget (`Dragons::needs_save`, PM7).
fn step_paged(
    w: &mut World,
    d: &mut Dragons,
    store: &RwLock<PageStore>,
    (from, end): (Tick, Tick),
    save_every: u64,
    saves: &mut Saves,
    win: &mut Window,
) {
    let mut t = from;
    while t < end {
        let boundary = Tick((t.0 / STEP_TICKS + 1) * STEP_TICKS);
        let next = d.next_due().unwrap_or(end).clamp(t + 1, end).min(boundary);
        w.run_until(next);
        let c0 = sys::thread_cpu_ns();
        d.run_until(w, &mut NoInfrastructure, next);
        let at_boundary = next.0.is_multiple_of(STEP_TICKS);
        if at_boundary {
            // Resident bytes only grow between boundaries: their peak is here, before the check.
            win.peak = win.peak.max(d.resident_row_bytes());
            d.enforce_budget();
        }
        match cpu_since(c0) {
            Some(ns) => win.dragon_ns += ns,
            None => win.have_cpu = false,
        }
        if next.0.is_multiple_of(save_every) {
            save(d, store, saves, false);
        } else if at_boundary && d.needs_save() {
            save(d, store, saves, true);
        }
        t = next;
    }
}

fn run_paged(c: &DragonConfig, budget: usize, save_every: u64) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let mut w = synth::grid(c.villages as usize, c.seed);
    let mut d = Dragons::new(c.seed, DragonTunables::default(), Tick::ZERO);
    let lairs = synth::bench_lairs(&w, c.dragons as usize, 1, c.seed);
    synth::populate(&mut w, &mut d, &lairs, Tick::ZERO);
    if c.oil {
        add_oil(&mut w, &mut d, c.seed);
    }
    let dir = super::scale::scratch_dir("dragons-paged");
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync)
            .unwrap_or_else(|e| panic!("dragons bench: the page store: {e}")),
    ));
    d.attach_store(store.clone(), budget);
    let mut saves = Saves::default();
    // The first save writes every row (the world as built), then saves are real ones.
    save(&mut d, &store, &mut saves, false);
    store
        .write()
        .expect("the store")
        .set_durability(Durability::Fsync);
    let mut win = Window {
        have_cpu: true,
        ..Window::default()
    };
    step_paged(
        &mut w,
        &mut d,
        &store,
        (Tick::ZERO, warm),
        save_every,
        &mut saves,
        &mut win,
    );
    let before = d.stats;
    saves = Saves::default();
    d.reset_paging_stats();
    sys::trim_heap();
    sys::reset_heap_peak();
    let mut win = Window {
        have_cpu: true,
        ..Window::default()
    };
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    step_paged(
        &mut w,
        &mut d,
        &store,
        (warm, end),
        save_every,
        &mut saves,
        &mut win,
    );
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = cpu_since(cpu0);
    // At a step boundary after a save the budget holds: nothing is dirty.
    if !end.0.is_multiple_of(save_every) {
        save(&mut d, &store, &mut saves, false);
    }
    let resident = d.resident_row_bytes();
    let ps = d.paging_stats();
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let (rss, rss_peak) = sys::rss();
    let engine = d.heap_bytes() as u64;
    let store_index = store.read().expect("the store").heap_bytes() as u64;
    let disk = store.read().expect("the store").disk_bytes();
    let days = c.days as f64;
    let n = c.dragons.max(1) as f64;
    let mut failures = Vec::new();
    if resident > budget {
        failures.push(format!(
            "resident rows {resident} bytes over the budget {budget} after a save"
        ));
    }
    if d.stats.materialized > 0 {
        failures.push("a dragon materialized with nobody watching".into());
    }
    let events = d.stats.events - before.events;
    let decisions = d.stats.decisions - before.decisions;
    // The checks and the hash read every row page: outside the window, after the heap reading.
    let conservation = match d.check(&w, None) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let state_hash = Some(d.state_hash() as u32);
    drop(d);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
    let save_cpu = saves.cpu_ns as f64;
    let n_saves = saves.saves.max(1) as f64;
    Measured {
        cpu_ns_per_day: cpu.map(|c| (c - save_cpu).max(0.0) / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: win.have_cpu.then(|| win.dragon_ns / (days * n)),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: engine,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        save_full_bytes: Some(disk),
        events,
        reads: decisions,
        conservation,
        failures,
        state_hash,
        resident_bytes: Some(resident as u64),
        resident_peak_bytes: Some(win.peak as u64),
        budget_bytes: Some(budget as u64),
        store_index_bytes: Some(store_index),
        loads_per_day: Some(ps.loads() as f64 / days),
        evictions_per_day: Some(ps.evictions() as f64 / days),
        over_budget_checks: Some(ps.over_budget()),
        save_ms_per_day: Some(saves.wall_ns as f64 / 1e6 / days),
        save_cpu_ms_per_day: Some(save_cpu / 1e6 / days),
        save_bytes_per_save: Some(saves.bytes as f64 / n_saves),
        save_log_bytes_per_save: Some(saves.log_bytes as f64 / n_saves),
        save_incremental_pages: Some(saves.pages / saves.saves.max(1)),
        checkpoints: Some(saves.checkpoints),
        saves_per_day: Some(saves.saves as f64 / days),
        pressure_saves_per_day: Some(saves.pressure as f64 / days),
        ..Measured::default()
    }
}
