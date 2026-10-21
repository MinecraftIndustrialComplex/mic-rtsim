// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 market sweeps of the scale bench: market count and edge count (SCALE.md §10;
//! economy research "Cost (ESTIMATE, to bench): per event O(log G + deg × G)").
//!
//! A grid of human and ribbit villages (`rtsim_world::synth::grid`) trades for a warm-up, then a
//! measured window. Cost per sim day is per event, so it grows with how many markets have events;
//! a world at rest costs nothing (the two-market test checks quiescence). The edge sweep varies
//! the edge rule's nearest-k with the market count fixed, so edges grow and so does the work per
//! event.
//!
//! **Paged** (`paged`, docs/choices-paging-modules.md): the market rows page through a store
//! under a budget, the world stepped tick by tick, autosaved every `save_every_ticks` and saved
//! again whenever every resident market page is changed (`World::needs_save`).

use std::sync::{Arc, RwLock};
use std::time::Instant;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
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
    /// Page the market rows under a budget.
    #[serde(default)]
    pub paged: Option<super::herds::HerdsPaging>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Saves {
    saves: u64,
    pressure: u64,
    cpu_ns: u64,
    bytes: u64,
}

struct Paged {
    store: Arc<RwLock<PageStore>>,
    dir: std::path::PathBuf,
    save_every: u64,
    saves: Saves,
}

impl Paged {
    fn save(&mut self, w: &mut World, pressure: bool) {
        let c0 = sys::thread_cpu_ns();
        let mut b = SaveBatch::new();
        let t = w.save_paged(&mut b);
        let st = self
            .store
            .write()
            .expect("store lock")
            .commit(b, 0)
            .unwrap_or_else(|e| panic!("page store: {e}"));
        w.saved(t);
        w.enforce_budget();
        if let (Some(a), Some(b)) = (c0, sys::thread_cpu_ns()) {
            self.saves.cpu_ns += b - a;
        }
        self.saves.saves += 1;
        self.saves.pressure += u64::from(pressure);
        self.saves.bytes += st.segment_bytes + st.log_bytes;
    }

    fn run(&mut self, w: &mut World, end: Tick) {
        let mut t = w.now();
        while t < end {
            t += 1;
            w.run_until(t);
            if t.0.is_multiple_of(self.save_every) {
                self.save(w, false);
            } else if w.needs_save() {
                self.save(w, true);
            }
        }
    }
}

pub fn run(c: &MarketConfig) -> Measured {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut cfg = WorldConfig::new(c.seed);
    cfg.genesis = Genesis::Share(500);
    cfg.edge_rule.nearest = c.nearest as usize;
    let mut w = World::build(cfg, goods, book, synth::grid(c.markets as usize, c.seed));
    let mut paged = c.paged.as_ref().map(|p| {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("markets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store =
            PageStore::open(&dir, Durability::NoSync).unwrap_or_else(|e| panic!("page store: {e}"));
        let store = Arc::new(RwLock::new(store));
        w.attach_store(store.clone(), p.budget_bytes as usize);
        let mut pg = Paged {
            store,
            dir,
            save_every: p.save_every_ticks.max(1),
            saves: Saves::default(),
        };
        pg.save(&mut w, false);
        pg
    });
    let warm = Tick::from_days(c.warmup_days);
    match &mut paged {
        Some(p) => p.run(&mut w, warm),
        None => w.run_until(warm),
    }
    let before = w.stats;
    let saves0 = paged.as_ref().map(|p| p.saves).unwrap_or_default();
    w.markets.rows_mut().reset_stats();
    sys::reset_heap_peak();
    let (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
    let end = Tick::from_days(c.warmup_days + c.days);
    match &mut paged {
        Some(p) => p.run(&mut w, end),
        None => w.run_until(end),
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let saves = paged.as_ref().map(|p| p.saves).unwrap_or_default();
    let save_cpu = (saves.cpu_ns - saves0.cpu_ns) as f64;
    let cpu = match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some(((b - a) as f64 - save_cpu).max(0.0)),
        _ => None,
    };
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let rows = w.markets.rows().stats();
    let resident = w.markets.rows().resident_bytes() as u64;
    let engine_heap_bytes = w.heap_bytes() as u64;
    let n_saves = (saves.saves - saves0.saves).max(1) as f64;
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
    if let Some(p) = &c.paged {
        if let Some(pg) = &mut paged {
            pg.save(&mut w, false);
        }
        if w.markets.rows().resident_bytes() as u64 > p.budget_bytes {
            failures.push(format!(
                "resident {} bytes over the budget {} after a save",
                w.markets.rows().resident_bytes(),
                p.budget_bytes
            ));
        }
    }
    let state_hash = Some(w.state_hash() as u32);
    let pm = c.paged.as_ref().map(|p| (p, saves, saves0));
    if let Some(pg) = paged {
        let dir = pg.dir.clone();
        drop(pg);
        let _ = std::fs::remove_dir_all(dir);
    }
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes,
        resident_bytes: pm.map(|_| resident),
        resident_peak_bytes: pm.map(|_| rows.peak_resident_bytes),
        budget_bytes: pm.map(|(p, _, _)| p.budget_bytes),
        loads_per_day: pm.map(|_| rows.loads as f64 / days),
        evictions_per_day: pm.map(|_| rows.evictions as f64 / days),
        over_budget_checks: pm.map(|_| rows.over_budget),
        save_cpu_ms_per_day: pm.map(|_| save_cpu / 1e6 / days),
        save_bytes_per_save: pm.map(|(_, s, s0)| (s.bytes - s0.bytes) as f64 / n_saves),
        saves_per_day: pm.map(|(_, s, s0)| (s.saves - s0.saves) as f64 / days),
        pressure_saves_per_day: pm.map(|(_, s, s0)| (s.pressure - s0.pressure) as f64 / days),
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: w.stats.market_events - before.market_events,
        reads: w.stats.deliveries - before.deliveries,
        edges: Some(w.edges.len() as u64),
        conservation,
        failures,
        state_hash,
        ..Measured::default()
    }
}
