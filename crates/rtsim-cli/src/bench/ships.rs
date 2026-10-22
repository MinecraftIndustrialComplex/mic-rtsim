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
//!
//! **The grid sweeps** (`ships-grid`, `ships-paged`; A16, docs/choices-paging-modules.md PM-S):
//! the port grid grows (49 to 6,400 ports at sixteen ships a service, about 400 to 51,000 ships)
//! with a player touring the 7 x 7 docks around the haven, one an hour, and the fake plugin
//! answering, so ships are seen, embodied and released: the same play at every size, the rest of
//! the world unwatched. `ships-paged` runs the same points with the ship rows and lane threats
//! paged to a page store under a fixed budget, saved every autosave interval and whenever every
//! resident page has changed (`Ships::needs_save`, PM7): its row tables' resident bytes are held
//! to the budget whatever the ship count, while the network (docks, lanes, services: the flow
//! graph, resident) grows with the ports and is reported apart (`network_bytes`). A roaming
//! player (`ShipsConfig::roam`, one-off runs) visits every dock instead.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_settle::Pos;
use rtsim_ships::synth::FakeShips;
use rtsim_ships::{ShipTunables, Ships, synth};
use serde::{Deserialize, Serialize};

use super::scale::{Measured, Metric, Sweep, Vary, contract};
use super::world::ModelKind;
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShipsConfig {
    pub ships_per_service: u64,
    /// Ports on a side of the grid.
    pub side: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
    /// A player touring the docks, one an hour, with the fake plugin answering (the grid sweeps).
    #[serde(default)]
    pub observer: bool,
    /// The player roams every dock of the grid instead of the 7 x 7 docks around the haven: a
    /// fresh dock most hours, so what it costs grows with the grid until it never comes back
    /// within a save interval (one-off measurements; the sweeps tour the fixed block).
    #[serde(default)]
    pub roam: bool,
    /// The ships' row tables paged to a page store (A16).
    #[serde(default)]
    pub paged: Option<PagedShips>,
}

/// A paged point: the row tables' budget and the ticks between saves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PagedShips {
    pub budget_bytes: u64,
    pub save_every: u64,
}

/// Ships per service in the grid sweeps: the ship sweep's top point.
const GRID_SHIPS_PER_SERVICE: u64 = 16;

/// The paged sweep's budget for both row tables. PLACEHOLDER 64 KiB: about 200 resident ship
/// rows, a tenth of the smallest grid's ships, so every point pages (A16, P24).
const PAGED_BUDGET_BYTES: u64 = 64 << 10;

/// Where the paged sweep keeps the ships' meta page.
const META: PageKey = PageKey::new(0x5300, 0);

/// The grid sweeps: ports in the grid (a square), the same points resident and paged.
pub fn grid_sweeps(quick: bool) -> Vec<Sweep> {
    // Sides 7 to 80: about 400 to 51,000 ships. The smallest grid's ship table (about 100 KB)
    // is already as big as what the touring player dirties between two saves, so every point
    // pages and the resident bytes read flat; a smaller grid would hold its whole table, less.
    let values = if quick {
        vec![49, 784, 6_400]
    } else {
        vec![49, 196, 784, 3_136, 6_400]
    };
    let cpu = |paged: bool| {
        contract(
            Metric::Cpu,
            0.2,
            0.0,
            &format!(
                "the ships engine's own CPU per sim day (the world and the fake plugin stepped apart{}) with one player touring the docks: what the player sees is bounded by its view, the haven's reach is fixed, and unobserved ships cost nothing, so it does not grow with the ports. Declared 0.2, not flat: a bigger world's indexes cost log factors",
                if paged { ", saves excluded" } else { "" }
            ),
        )
    };
    let heap = contract(
        Metric::Heap,
        1.1,
        1.0,
        "the live heap holds the phase 1 world (about 17 KB a village) and the port network, both resident and linear in the ports",
    );
    vec![
        Sweep {
            name: "ships-grid".into(),
            model: ModelKind::Closed,
            vary: Vary::ShipPorts,
            regions: None,
            paged: false,
            values: values.clone(),
            contracts: vec![
                cpu(false),
                heap.clone(),
                contract(
                    Metric::Save,
                    1.1,
                    1.0,
                    "the one-blob save writes every ship row and the network",
                ),
            ],
        },
        Sweep {
            name: "ships-paged".into(),
            model: ModelKind::Closed,
            vary: Vary::ShipPorts,
            regions: None,
            paged: true,
            values,
            contracts: vec![
                contract(
                    Metric::Resident,
                    0.1,
                    0.0,
                    "the ship rows' and lane threats' resident pages are held to the budget whatever the ship count (A16): an unobserved ship's page is evicted once saved and never read again",
                ),
                cpu(true),
                heap,
                contract(
                    Metric::SaveIncr,
                    0.15,
                    0.0,
                    "a periodic save writes the pages the player and the pirates dirtied and a meta page without the network (written once, after it changed), not the ships; the store's compaction of the first full save's segment lands in some windows (up to 3x one save)",
                ),
            ],
        },
    ]
}

/// A grid sweep's point: `ports` (a square) at sixteen ships a service, with the touring player.
pub fn grid_config(ports: u64, paged: bool, days: u64, seed: u64, save_every: u64) -> ShipsConfig {
    ShipsConfig {
        ships_per_service: GRID_SHIPS_PER_SERVICE,
        side: (ports as f64).sqrt().round().max(1.0) as u64,
        warmup_days: 2,
        days,
        seed,
        observer: true,
        roam: false,
        paged: paged.then_some(PagedShips {
            budget_bytes: PAGED_BUDGET_BYTES,
            save_every,
        }),
    }
}

fn cpu_since(cpu0: Option<u64>) -> Option<f64> {
    match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    }
}

pub fn run(c: &ShipsConfig) -> Measured {
    if c.observer || c.paged.is_some() {
        return run_grid(c);
    }
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
    let pirate_kinds = ["decide", "reach_lane", "bored", "home", "refit"];
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

/// The player of the grid sweeps.
const TOURIST: (u64, u64) = (0x6121, 0xB);

/// Docks on a side of the block the player tours: the smallest grid's side, so the smallest grid
/// is all block and every grid gives the player the same docks around the same haven.
const TOUR_SIDE: i32 = 7;

/// The docks the player visits, in order (a stride coprime to the grid sizes reaches them all):
/// the `TOUR_SIDE` block around the haven (the grid's middle), or every dock when roaming.
fn tour_stops(s: &Ships, side: u64, roam: bool) -> Vec<(i32, i32)> {
    let mid = (side / 2) as i32;
    let near = |d: &rtsim_ships::network::Dock| {
        // ocean_grid's ports sit 1,200 blocks apart, nudged by less than 200.
        let (gx, gz) = (d.x.div_euclid(1_200), d.z.div_euclid(1_200));
        roam || ((gx - mid).abs() <= TOUR_SIDE / 2 && (gz - mid).abs() <= TOUR_SIDE / 2)
    };
    let stops: Vec<(i32, i32)> = s
        .docks
        .iter()
        .filter(|d| near(d))
        .map(|d| (d.x, d.z))
        .collect();
    assert!(!stops.is_empty(), "the tour has docks");
    stops
}

/// Ticks per step of the grid sweeps' loop (the fake plugin's resolution).
const STEP: u64 = 100;

/// A page store in the bench's scratch folder (on disk, beside the binary).
struct Pager {
    store: Arc<RwLock<PageStore>>,
    dir: PathBuf,
    save_every: u64,
    saves: u64,
    /// Saves the budget asked for (`Ships::needs_save`), beyond the autosaves.
    pressure: u64,
    cpu_ns: u64,
    bytes: u64,
    pages: u64,
    compacted: u64,
}

impl Pager {
    fn open(c: &PagedShips) -> Pager {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("ships-paged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = PageStore::open(&dir, Durability::Fsync).expect("open the scratch page store");
        Pager {
            store: Arc::new(RwLock::new(store)),
            dir,
            save_every: c.save_every.max(STEP),
            saves: 0,
            pressure: 0,
            cpu_ns: 0,
            bytes: 0,
            pages: 0,
            compacted: 0,
        }
    }

    /// One paged save as the game makes it: the batch, the commit, the tickets back, the budget.
    fn save(&mut self, s: &mut Ships, pressure: bool) {
        let c0 = sys::thread_cpu_ns();
        let mut b = SaveBatch::new();
        let t = s.save_paged(&mut b, META);
        self.saves += 1;
        self.pressure += u64::from(pressure);
        let st = self
            .store
            .write()
            .expect("the store lock")
            .commit(b, self.saves)
            .expect("a commit to the scratch store");
        s.saved(t);
        s.enforce_budget();
        if let (Some(a), Some(b)) = (c0, sys::thread_cpu_ns()) {
            self.cpu_ns += b - a;
        }
        self.bytes += st.segment_bytes + st.log_bytes;
        self.pages += st.pages_written;
        self.compacted += st.pages_compacted;
    }
}

impl Drop for Pager {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The grid with its touring player, stepped to `to`: the ships' own calls are timed (`cpu`),
/// the world and the fake plugin run apart, and the paged engine saves every interval.
#[allow(clippy::too_many_arguments)]
fn tour(
    w: &mut rtsim_world::World,
    s: &mut Ships,
    fake: &mut FakeShips,
    mask: &rtsim_ships::water::AllWater,
    stops: &[(i32, i32)],
    pager: &mut Option<Pager>,
    t: &mut Tick,
    to: Tick,
    cpu: &mut Option<f64>,
) {
    let timed = |cpu: &mut Option<f64>, f: &mut dyn FnMut()| {
        let c0 = sys::thread_cpu_ns();
        f();
        match (c0, sys::thread_cpu_ns(), cpu.as_mut()) {
            (Some(a), Some(b), Some(c)) => *c += (b - a) as f64,
            _ => *cpu = None,
        }
    };
    while *t < to {
        if t.0.is_multiple_of(1_000) {
            let hour = (t.0 / 1_000) as usize;
            let (x, z) = stops[hour * 11 % stops.len()];
            fake.set_player(TOURIST, f64::from(x), f64::from(z));
            let at = *t;
            timed(cpu, &mut || {
                s.observe_player(w, TOURIST, Pos::new(0, x, z), at);
            });
        }
        let boundary = Tick((t.0 / STEP + 1) * STEP).min(to);
        // The world runs apart, never past the ships' next event (a lower bound).
        let next = s.next_due().unwrap_or(boundary).clamp(*t + 1, boundary);
        w.run_until(next);
        timed(cpu, &mut || s.run_until(w, &mut NoInfrastructure, next));
        *t = next;
        if next == boundary {
            let cmds = s.take_commands();
            let mut obs = fake.apply(&cmds, next);
            obs.extend(fake.step(next));
            timed(cpu, &mut || {
                for o in &obs {
                    s.ingest(w, o, mask, next);
                }
                s.enforce_budget();
            });
        }
        // The autosave, or the budget's save when every resident page has changed (PM7).
        if let Some(p) = pager.as_mut() {
            if next == boundary && next.0.is_multiple_of(p.save_every) {
                p.save(s, false);
            } else if s.needs_save() {
                p.save(s, true);
            }
        }
    }
}

fn run_grid(c: &ShipsConfig) -> Measured {
    let (warm, end) = (
        Tick::from_days(c.warmup_days),
        Tick::from_days(c.warmup_days + c.days),
    );
    let tun = ShipTunables {
        max_ships_per_service: c.ships_per_service.max(1) as u32,
        headway_ticks: 1,
        ..ShipTunables::default()
    };
    let (mut w, mut s, mask) = synth::ocean_grid(c.side as usize, c.seed, tun);
    // Ships per service from the services' key ranges: no pass over the rows.
    let merchants = (0..s.services.len() as u32)
        .map(|v| s.service_ships(v).count())
        .sum::<usize>()
        .max(1);
    let lanes = s.lanes.len() as u64;
    // The network weighed by the allocator (a copy holds its contents, not spare capacity).
    let h0 = sys::heap_live();
    let copy = s.clone_network();
    let network_copy = sys::heap_live().saturating_sub(h0);
    drop(copy);
    let stops = tour_stops(&s, c.side, c.roam);
    let mut pager = c.paged.as_ref().map(Pager::open);
    if let Some(p) = pager.as_mut() {
        s.attach_store(
            p.store.clone(),
            c.paged.as_ref().map_or(0, |p| p.budget_bytes) as usize,
        );
        // The first save writes every row; then the budget evicts them.
        p.save(&mut s, false);
    }
    let mut fake = FakeShips::new(f64::from(s.tun.view_radius));
    let mut t = Tick::ZERO;
    let mut none = None;
    tour(
        &mut w, &mut s, &mut fake, &mask, &stops, &mut pager, &mut t, warm, &mut none,
    );
    let before = s.stats;
    s.reset_row_stats();
    let saves0 = pager
        .as_ref()
        .map(|p| (p.saves, p.cpu_ns, p.bytes, p.pressure));
    sys::reset_heap_peak();
    sys::trim_heap();
    sys::reset_rss_peak();
    let wall0 = Instant::now();
    let mut cpu = sys::thread_cpu_ns().map(|_| 0.0);
    tour(
        &mut w, &mut s, &mut fake, &mask, &stops, &mut pager, &mut t, end, &mut cpu,
    );
    let wall = wall0.elapsed().as_nanos() as f64;
    let days = c.days as f64;
    // Measured before the whole-table passes below (the check and the hash load every page).
    let heap_live = sys::heap_live() as u64;
    let heap_peak = sys::heap_peak() as u64;
    let (rss, rss_peak) = sys::rss();
    let rows = s.row_stats();
    let resident = s.rows_resident_bytes() as u64;
    let engine = s.heap_bytes() as u64;
    let network = s.network_bytes() as u64;
    let st = s.stats;
    let mut failures = Vec::new();
    let mut save_counts = None;
    let (save_cpu, save_bytes) = match (&mut pager, saves0) {
        (Some(p), Some((n0, c0, b0, p0))) => {
            let n = (p.saves - n0).max(1) as f64;
            let r = ((p.cpu_ns - c0) as f64, (p.bytes - b0) as f64 / n);
            save_counts = Some(((p.saves - n0) as f64, (p.pressure - p0) as f64));
            // At a step boundary after a save, the budget holds: nothing is dirty.
            p.save(&mut s, false);
            // A save with nothing changed writes the meta page alone (the resident state).
            let b1 = p.bytes;
            p.save(&mut s, false);
            eprintln!(
                "  the meta page alone: {} bytes stored ({} havens, {} navies, {} pending events)",
                p.bytes - b1,
                s.havens.len(),
                s.navies.len(),
                s.pending_events()
            );
            let budget = c.paged.as_ref().map_or(0, |p| p.budget_bytes);
            if s.rows_resident_bytes() as u64 > budget {
                failures.push(format!(
                    "rows resident {} bytes over the budget {budget} after a save",
                    s.rows_resident_bytes()
                ));
            }
            (Some(r.0), Some(r.1))
        }
        _ => (None, None),
    };
    if st.seen == before.seen {
        failures.push("the touring player saw no ship in the window".into());
    }
    eprintln!(
        "ships grid side {}: {merchants} ships, {lanes} lanes, {} services, network {network} B estimated ({:?}), {network_copy} B copied; engine {engine} B; rows resident {resident} B, peak {} B; {} loads, {} evictions",
        c.side,
        s.services.len(),
        s.network_breakdown(),
        rows.peak_resident_bytes,
        rows.loads,
        rows.evictions,
    );
    if let Some(p) = &pager {
        eprintln!(
            "  {} saves ({} for the budget): {} pages written, {} compacted, {} bytes",
            p.saves, p.pressure, p.pages, p.compacted, p.bytes
        );
    }
    let conservation = match s.check(&w) {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    if conservation != "ok" {
        failures.push(format!("conservation: {conservation}"));
    }
    let paged = c.paged.is_some();
    // Saves run outside the timed calls: `cpu` is the engine's own work, saves apart.
    let cpu_ns = cpu;
    Measured {
        cpu_ns_per_day: cpu_ns.map(|x| x / days),
        wall_ns_per_day: Some(wall / days),
        cpu_ns_per_unit_day: cpu_ns.map(|x| x / (days * merchants as f64)),
        heap_live_bytes: heap_live,
        heap_peak_bytes: heap_peak,
        engine_heap_bytes: engine,
        rss_bytes: rss,
        rss_peak_bytes: rss_peak,
        events: st.events - before.events,
        reads: merchants as u64,
        caravans: merchants as u64,
        edges: Some(lanes),
        conservation,
        failures,
        state_hash: Some(s.state_hash() as u32),
        save_full_bytes: Some(s.save().len() as u64),
        resident_bytes: paged.then_some(resident),
        resident_peak_bytes: paged.then_some(rows.peak_resident_bytes),
        budget_bytes: c.paged.as_ref().map(|p| p.budget_bytes),
        store_index_bytes: pager
            .as_ref()
            .map(|p| p.store.read().expect("the store lock").heap_bytes() as u64),
        loads_per_day: paged.then_some(rows.loads as f64 / days),
        evictions_per_day: paged.then_some(rows.evictions as f64 / days),
        over_budget_checks: paged.then_some(rows.over_budget),
        save_cpu_ms_per_day: save_cpu.map(|c| c / 1e6 / days),
        save_bytes_per_save: save_bytes,
        network_bytes: Some(network),
        saves_per_day: save_counts.map(|(n, _)| n / days),
        pressure_saves_per_day: save_counts.map(|(_, p)| p / days),
        ..Measured::default()
    }
}
