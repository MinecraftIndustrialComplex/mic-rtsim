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
//!
//! **Paged** (`factions-paged`, A16): the same run with the conflict's row tables (factions with
//! their feuds, sources, places) paged to a page store under a fixed small budget and saved at
//! Minecraft's autosave interval, and again whenever the rows are over the budget with every
//! resident page changed (`Conflict::needs_save`: the host's extra save). The phase 1 world under
//! the camps is not paged here and dominates the process heap, so the point reports the row
//! tables' resident bytes (bounded by the budget plus one step's changes), the engine's own
//! `heap_bytes` estimate, and the engine's live bytes measured by dropping it at the end
//! (`engine_live_bytes`, the counting allocator's difference).
//!
//! **Rail** (`rail-paged`, M7 review P3-4): Frostmaw dens, each with a line of its own run at a
//! fixed number of legs a day by one player, on the same paged row tables. A leg is a RAIL_PASS
//! through the den's circle and the TRAIN_ARRIVED that names its line, so the run is the rail
//! meter's: its counts, pushes and wakes, and the page loads they cost. The figures it reports
//! apart (`RailFigures`) are the wake pops a day, live and stale, and the pushes.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use rtsim_conflict::Conflict;
use rtsim_conflict::boundary::Obs;
use rtsim_conflict::engine::StationEnd;
use rtsim_conflict::synth::{self, RailDen, RailEvent, RailLine};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use serde::{Deserialize, Serialize};

use super::scale::{Measured, RailFigures};
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactionConfig {
    pub factions: u64,
    pub villages: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
    /// Page the row tables (A16); `None`: resident.
    #[serde(default)]
    pub paged: Option<FactionPaging>,
}

/// A paged faction run's budget and save interval.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactionPaging {
    pub budget_bytes: u64,
    pub save_every_ticks: u64,
}

/// The conflict's meta page (rtsim-jni's conflict page key).
const META: PageKey = PageKey::new(0x4300, 0);

/// Saves done in the measured window: CPU, wall and bytes.
#[derive(Default)]
struct Saves {
    n: u64,
    /// Saves the budget asked for (`Conflict::needs_save`), beyond the autosaves.
    pressure: u64,
    cpu_ns: u64,
    wall_ns: u64,
    bytes: u64,
    log_bytes: u64,
}

struct Pager {
    dir: PathBuf,
    store: Arc<RwLock<PageStore>>,
    every: u64,
    next: Tick,
    saves: Saves,
}

impl Pager {
    fn save(&mut self, k: &mut Conflict) {
        let t0 = Instant::now();
        let c0 = sys::thread_cpu_ns();
        let mut b = SaveBatch::new();
        let tickets = k.save_paged(&mut b, META);
        let st = self
            .store
            .write()
            .expect("store")
            .commit(b, 0)
            .expect("commit conflict pages");
        k.saved(tickets);
        k.enforce_budget();
        self.saves.n += 1;
        self.saves.wall_ns += t0.elapsed().as_nanos() as u64;
        if let (Some(a), Some(b)) = (c0, sys::thread_cpu_ns()) {
            self.saves.cpu_ns += b - a;
        }
        self.saves.bytes += st.segment_bytes;
        self.saves.log_bytes += st.log_bytes;
    }

    /// At a step boundary at `t`: the autosave when it is due, else the budget's save when every
    /// resident page is changed (PM-C10).
    fn after_step(&mut self, k: &mut Conflict, t: Tick) {
        if self.next == t {
            self.save(k);
            self.next = t + self.every;
        } else if k.needs_save() {
            self.save(k);
            self.saves.pressure += 1;
        }
    }
}

/// Bench scratch folders live under the target folder (on disk, not a tmpfs), as
/// `bench::scale`'s.
fn scratch_dir(tag: &str) -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("{tag}-{}", std::process::id()))
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
    let mut pager = c.paged.map(|p| {
        let dir = scratch_dir("factions");
        let _ = std::fs::remove_dir_all(&dir);
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).expect("open scratch page store"),
        ));
        k.attach_store(store.clone(), p.budget_bytes as usize);
        let mut pg = Pager {
            dir,
            store,
            every: p.save_every_ticks.max(1),
            next: Tick(p.save_every_ticks.max(1)),
            saves: Saves::default(),
        };
        // The first save writes every row; then Minecraft's saves, with their fsyncs.
        pg.save(&mut k);
        pg.store
            .write()
            .expect("store")
            .set_durability(Durability::Fsync);
        pg
    });
    let mut t = Tick::ZERO;
    let step = |t: Tick, to: Tick, k: &Conflict, pager: &Option<Pager>| {
        let next = k.next_due().unwrap_or(to).clamp(t + 1, to);
        pager.as_ref().map_or(next, |p| next.min(p.next))
    };
    // Warm up as measured, saves included, so the window starts from a steady paged state.
    while t < warm {
        let next = step(t, warm, &k, &pager);
        k.run_until(&mut w, &mut NoInfrastructure, next);
        if let Some(p) = pager.as_mut() {
            p.after_step(&mut k, next);
        }
        t = next;
    }
    let _ = k.take_commands();
    let before = k.stats;
    k.reset_row_stats();
    if let Some(p) = pager.as_mut() {
        p.saves = Saves::default();
    }
    sys::reset_heap_peak();
    let wall0 = Instant::now();
    let mut ns = 0.0;
    let mut have_cpu = true;
    while t < end {
        let next = step(t, end, &k, &pager);
        w.run_until(next);
        let c0 = sys::thread_cpu_ns();
        k.run_until(&mut w, &mut NoInfrastructure, next);
        match cpu_since(c0) {
            Some(n) => ns += n,
            None => have_cpu = false,
        }
        let _ = k.take_commands();
        if let Some(p) = pager.as_mut() {
            p.after_step(&mut k, next);
        }
        t = next;
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let rows = k.row_stats();
    let mut failures = Vec::new();
    let resident = k.resident_bytes() as u64;
    if let Some(p) = pager.as_mut() {
        // At a step boundary after a save the budget holds: nothing is dirty.
        p.save(&mut k);
        p.saves.n -= 1;
        if k.resident_bytes() > k.budget() {
            failures.push(format!(
                "resident {} bytes over the budget {} after a save",
                k.resident_bytes(),
                k.budget()
            ));
        }
    }
    let conservation = match k.check(&w) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let st = k.stats;
    if st.materialized > 0 {
        failures.push("a party materialized with nobody watching".into());
    }
    let n = c.factions.max(1) as f64;
    let cpu = have_cpu.then_some(ns);
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    // Whole-engine passes (they load every page, evicted again after): outside the window.
    let state_hash = Some(k.state_hash() as u32);
    let save_full = Some(k.save().len() as u64);
    k.enforce_budget();
    let engine_heap = k.heap_bytes() as u64;
    let parties = k.parties.len() as u64;
    // The engine's own live bytes: what the allocator gets back when it goes.
    let live0 = sys::heap_live();
    drop(k);
    let engine_live = live0.saturating_sub(sys::heap_live()) as u64;
    let paged = pager.map(|p| {
        let idx = p.store.read().expect("store").heap_bytes() as u64;
        let _ = std::fs::remove_dir_all(&p.dir);
        (p.saves, idx)
    });
    let per_save =
        |f: &dyn Fn(&Saves) -> u64| paged.as_ref().map(|(s, _)| f(s) as f64 / s.n.max(1) as f64);
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|c| c / (days * n)),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: engine_heap,
        engine_live_bytes: Some(engine_live),
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: c.factions,
        caravans: parties,
        conservation,
        failures,
        state_hash,
        save_full_bytes: save_full,
        resident_bytes: c.paged.map(|_| resident),
        resident_peak_bytes: c.paged.map(|_| rows.peak_resident_bytes),
        budget_bytes: c.paged.map(|p| p.budget_bytes),
        store_index_bytes: paged.as_ref().map(|p| p.1),
        loads_per_day: c.paged.map(|_| rows.loads as f64 / days),
        evictions_per_day: c.paged.map(|_| rows.evictions as f64 / days),
        over_budget_checks: c.paged.map(|_| rows.over_budget),
        save_ms_per_day: paged.as_ref().map(|(s, _)| s.wall_ns as f64 / 1e6 / days),
        save_cpu_ms_per_day: paged.as_ref().map(|(s, _)| s.cpu_ns as f64 / 1e6 / days),
        save_bytes_per_save: per_save(&|s| s.bytes),
        save_log_bytes_per_save: per_save(&|s| s.log_bytes),
        saves_per_day: paged.as_ref().map(|(s, _)| s.n as f64 / days),
        pressure_saves_per_day: paged.as_ref().map(|(s, _)| s.pressure as f64 / days),
        ..Measured::default()
    }
}

/// Rail dens: `dens` Frostmaw dens, each with its own short line run at `legs_per_day` legs a day
/// by one player (M7 review P3-4). Paged as the faction sweep is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RailConfig {
    pub dens: u64,
    pub legs_per_day: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
    #[serde(default)]
    pub paged: Option<FactionPaging>,
}

/// The sampler's jitter on a train's departure, as `sim mowzie` runs it (MZB-330).
const RAIL_JITTER: f64 = 0.4;

/// A line's ends are this far either side of its den, along z: inside the next den's gap (544
/// blocks at the least), so no line crosses another den's circle (200 blocks).
const RAIL_HALF_BLOCKS: f64 = 300.0;

fn rail_end(id: u64, x: f64, z: f64) -> StationEnd {
    StationEnd {
        id,
        dim: 0,
        x: x as i32,
        z: z as i32,
        district: Some(id + 1000),
    }
}

/// A rail run's pieces, stepped together by [`Rig::drive`].
struct Rig<I: Iterator<Item = (Tick, RailEvent)>> {
    w: rtsim_world::World,
    k: Conflict,
    pager: Option<Pager>,
    events: std::iter::Peekable<I>,
    t: Tick,
    /// The engine's thread CPU so far (events, steps; saves apart), and whether it was readable.
    ns: f64,
    have_cpu: bool,
}

impl<I: Iterator<Item = (Tick, RailEvent)>> Rig<I> {
    /// Runs the engine to `to`: every event before it at its own tick, the pager's saves at
    /// theirs.
    fn drive(&mut self, to: Tick) {
        let infra = &mut NoInfrastructure;
        while self.t < to {
            let mut at = self.events.peek().map_or(to, |e| e.0.min(to)).max(self.t);
            if let Some(p) = self.pager.as_ref() {
                at = at.min(p.next.max(self.t));
            }
            self.w.run_until(at);
            let c0 = sys::thread_cpu_ns();
            self.k.run_until(&mut self.w, infra, at);
            while self.events.peek().is_some_and(|e| e.0 == at && at < to) {
                let (_, ev) = self.events.next().expect("peeked");
                match ev {
                    RailEvent::Pass(p) => self.k.ingest(&mut self.w, infra, &Obs::RailPass(p), at),
                    RailEvent::Arrive(call) => self.k.rail_call(&mut self.w, &call, at),
                }
            }
            match cpu_since(c0) {
                Some(n) => self.ns += n,
                None => self.have_cpu = false,
            }
            let _ = self.k.take_commands();
            if let Some(p) = self.pager.as_mut() {
                p.after_step(&mut self.k, at);
            }
            self.t = at;
        }
    }
}

pub fn run_rail(c: &RailConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let (mut w, mut k, dens) = synth::many_dens(c.dens as usize, c.dens as usize, c.seed);
    let owner = synth::PLAYER;
    k.player(&mut w, owner);
    let lines: Vec<RailLine> = dens
        .iter()
        .enumerate()
        .map(|(i, (_, p))| {
            let (x, z) = (f64::from(p.x), f64::from(p.z));
            let (north, south) = (z - RAIL_HALF_BLOCKS, z + RAIL_HALF_BLOCKS);
            RailLine::steady(
                rail_end(2 * i as u64 + 1, x, north),
                rail_end(2 * i as u64 + 2, x, south),
                vec![(x, north), (x, south)],
                owner,
                c.legs_per_day as f64,
            )
        })
        .collect();
    let circles: Vec<RailDen> = dens
        .iter()
        .map(|&(key, p)| RailDen {
            key,
            x: p.x,
            z: p.z,
            radius: 200,
            since: Tick::ZERO,
        })
        .collect();
    let mut events = synth::rail_events(&lines, &circles, c.seed, RAIL_JITTER, end);
    events.sort_by_key(|e| e.0);
    let events = events.into_iter().peekable();
    let pager = c.paged.map(|p| {
        let dir = scratch_dir("rail");
        let _ = std::fs::remove_dir_all(&dir);
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).expect("open scratch page store"),
        ));
        k.attach_store(store.clone(), p.budget_bytes as usize);
        let mut pg = Pager {
            dir,
            store,
            every: p.save_every_ticks.max(1),
            next: Tick(p.save_every_ticks.max(1)),
            saves: Saves::default(),
        };
        pg.save(&mut k);
        pg.store
            .write()
            .expect("store")
            .set_durability(Durability::Fsync);
        pg
    });
    let mut rig = Rig {
        w,
        k,
        pager,
        events,
        t: Tick::ZERO,
        ns: 0.0,
        have_cpu: true,
    };
    rig.drive(warm);
    rig.ns = 0.0;
    rig.have_cpu = true;
    let before = rig.k.stats;
    rig.k.reset_row_stats();
    if let Some(p) = rig.pager.as_mut() {
        p.saves = Saves::default();
    }
    sys::reset_heap_peak();
    let wall0 = Instant::now();
    rig.drive(end);
    let Rig {
        w,
        mut k,
        mut pager,
        ns,
        have_cpu,
        ..
    } = rig;
    let wall = wall0.elapsed().as_nanos() as f64;
    let days = c.days as f64;
    let (rss, rss_peak) = sys::rss();
    let rows = k.row_stats();
    let mut failures = Vec::new();
    let resident = k.resident_bytes() as u64;
    if let Some(p) = pager.as_mut() {
        p.save(&mut k);
        p.saves.n -= 1;
        if k.resident_bytes() > k.budget() {
            failures.push(format!(
                "resident {} bytes over the budget {} after a save",
                k.resident_bytes(),
                k.budget()
            ));
        }
    }
    let conservation = match k.check(&w) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let st = k.stats;
    if st.materialized > 0 {
        failures.push("a party materialized with nobody watching".into());
    }
    let n = c.dens.max(1) as f64;
    let cpu = have_cpu.then_some(ns);
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let state_hash = Some(k.state_hash() as u32);
    let save_full = Some(k.save().len() as u64);
    k.enforce_budget();
    let engine_heap = k.heap_bytes() as u64;
    let parties = k.parties.len() as u64;
    let live0 = sys::heap_live();
    drop(k);
    let engine_live = live0.saturating_sub(sys::heap_live()) as u64;
    let paged = pager.map(|p| {
        let idx = p.store.read().expect("store").heap_bytes() as u64;
        let _ = std::fs::remove_dir_all(&p.dir);
        (p.saves, idx)
    });
    let per = |a: u64, b: u64| (a - b) as f64 / days;
    let rail = RailFigures {
        legs_per_day: per(st.rail_calls, before.rail_calls),
        passes_per_day: per(st.rail_passes, before.rail_passes),
        wakes_per_day: per(st.rail_wakes, before.rail_wakes),
        stale_per_day: per(st.rail_stale, before.rail_stale),
        pushes_per_day: per(st.rail_pushes, before.rail_pushes),
    };
    let per_save =
        |f: &dyn Fn(&Saves) -> u64| paged.as_ref().map(|(s, _)| f(s) as f64 / s.n.max(1) as f64);
    Measured {
        cpu_ns_per_day: cpu.map(|c| c / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu.map(|c| c / (days * n)),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: engine_heap,
        engine_live_bytes: Some(engine_live),
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: c.dens,
        caravans: parties,
        conservation,
        failures,
        state_hash,
        save_full_bytes: save_full,
        resident_bytes: c.paged.map(|_| resident),
        resident_peak_bytes: c.paged.map(|_| rows.peak_resident_bytes),
        budget_bytes: c.paged.map(|p| p.budget_bytes),
        store_index_bytes: paged.as_ref().map(|p| p.1),
        loads_per_day: c.paged.map(|_| rows.loads as f64 / days),
        evictions_per_day: c.paged.map(|_| rows.evictions as f64 / days),
        over_budget_checks: c.paged.map(|_| rows.over_budget),
        save_ms_per_day: paged.as_ref().map(|(s, _)| s.wall_ns as f64 / 1e6 / days),
        save_cpu_ms_per_day: paged.as_ref().map(|(s, _)| s.cpu_ns as f64 / 1e6 / days),
        save_bytes_per_save: per_save(&|s| s.bytes),
        save_log_bytes_per_save: per_save(&|s| s.log_bytes),
        saves_per_day: paged.as_ref().map(|(s, _)| s.n as f64 / days),
        pressure_saves_per_day: paged.as_ref().map(|(s, _)| s.pressure as f64 / days),
        rail: Some(rail),
        ..Measured::default()
    }
}
