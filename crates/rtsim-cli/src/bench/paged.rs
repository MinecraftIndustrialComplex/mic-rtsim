// SPDX-License-Identifier: AGPL-3.0-or-later
//! The scale bench's world on paged regions (A16, SCALE.md §8): the same regions, caravans and
//! observers as [`super::world::World`], but the regions live in a [`PagedTable`] over a page store
//! on disk, with a fixed memory budget, saves every [`PagedSpec::save_every`] ticks (Minecraft's
//! autosave interval), prefetch of caravan destinations and read-ahead along the observers' path.
//!
//! The world is built page by page with periodic commits, so no point of the run ever holds the
//! whole world in memory: what the bench reports as resident memory is what a server with that
//! budget would hold.

use std::time::Instant;

use rtsim_kernel::clock::{TICKS_PER_DAY, TICKS_PER_SECOND, Tick};
use rtsim_kernel::conserve::transfer_up_to;
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_kernel::paging::{PagedModel, PagedTable, PagingConfig, PagingStats};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_kernel::wheel::TimingWheel;
use serde::{Deserialize, Serialize};

use super::world::{BenchModel, WorldConfig};

/// Grain a caravan carries at most. As in `world.rs`. PLACEHOLDER.
const CARGO: i64 = 50;
const OBSERVE_EVERY: u64 = TICKS_PER_SECOND;
const MOVE_EVERY: u64 = 1000;
const PAGE_TABLE: u16 = 1;
const META_TABLE: u16 = 2;
/// Pages per commit while building. PLACEHOLDER, only bounds the build's memory.
const BUILD_BATCH_PAGES: u64 = 256;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PagedSpec {
    pub world: WorldConfig,
    pub budget_bytes: u64,
    /// Ticks between saves: Minecraft's autosave is every 6,000 ticks (5 minutes).
    pub save_every: u64,
}

#[derive(Clone, Copy, Debug)]
enum Traffic {
    Depart(u64),
    Arrive { to: u64, amount: i64 },
}

struct Observer {
    x: u64,
    y: u64,
}

/// What the save path did over a run.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct SaveTotals {
    pub saves: u64,
    pub wall_ns: u64,
    /// Thread CPU time inside saves: encoding and compressing pages, writing, syncing.
    pub cpu_ns: u64,
    pub pages: u64,
    pub segment_bytes: u64,
    pub log_bytes: u64,
    pub checkpoint_bytes: u64,
    pub checkpoints: u64,
}

pub struct PagedWorld<M: BenchModel + PagedModel> {
    pub cfg: WorldConfig,
    pub table: PagedTable<M>,
    pub store: PageStore,
    pub dir: std::path::PathBuf,
    traffic: TimingWheel<Traffic>,
    traffic_key: StreamKey,
    in_transit: i64,
    initial_grain: i64,
    initial_heads: u64,
    side: u64,
    observers: Vec<Observer>,
    save_every: u64,
    pub saves: SaveTotals,
    pub now: Tick,
    pub departures: u64,
    pub arrivals: u64,
}

fn io<T>(r: rtsim_kernel::page::Result<T>) -> T {
    r.unwrap_or_else(|e| panic!("page store: {e}"))
}

impl<M: BenchModel + PagedModel> PagedWorld<M> {
    /// Builds the world into a fresh store under `dir`, page by page, never holding more than the
    /// budget plus one build batch.
    pub fn build(spec: &PagedSpec, dir: std::path::PathBuf) -> PagedWorld<M> {
        let cfg = spec.world.clone();
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = io(PageStore::open(&dir, Durability::NoSync));
        let paging = PagingConfig {
            regions_per_page: cfg.regions_per_page,
            budget_bytes: spec.budget_bytes as usize,
            ..PagingConfig::new(PAGE_TABLE, META_TABLE)
        };
        let now = Tick::ZERO;
        let mut table = PagedTable::new(cfg.seed, SystemId::named("bench.region"), now, paging);
        let setup = StreamKey::new(cfg.seed, SystemId::named("bench.setup"), 0);
        let (mut initial_grain, mut initial_heads) = (0, 0);
        for i in 0..cfg.regions {
            let mut rng = setup.slot(i);
            let m = M::make(i, &cfg, &mut rng);
            initial_grain += m.grain_now();
            initial_heads += m.heads().map_or(0, |h| h.0);
            io(table.insert(&mut store, i, m, now));
            if (i + 1) % (cfg.regions_per_page * BUILD_BATCH_PAGES) == 0 {
                let mut b = SaveBatch::new();
                let t = table.save_dirty(&mut b);
                io(store.commit(b, 0));
                table.saved(t);
                table.enforce_budget();
            }
        }
        let mut b = SaveBatch::new();
        let t = table.save_dirty(&mut b);
        table.save_meta(&mut b);
        io(store.commit(b, 0));
        table.saved(t);
        table.enforce_budget();
        // From here on, saves are real ones.
        store.set_durability(Durability::Fsync);
        io(store.checkpoint_now());
        let side = (cfg.regions as f64).sqrt().ceil() as u64;
        let observers = (0..cfg.observers)
            .map(|o| Observer {
                x: (o * 7919) % side,
                y: (o * 104_729) % side,
            })
            .collect();
        let mut traffic = TimingWheel::new(now);
        if cfg.events_per_day > 0 {
            traffic.schedule(now, Traffic::Depart(0));
        }
        table.reset_stats();
        PagedWorld {
            traffic_key: StreamKey::new(cfg.seed, SystemId::named("bench.traffic"), 0),
            cfg,
            table,
            store,
            dir,
            traffic,
            in_transit: 0,
            initial_grain,
            initial_heads,
            side,
            observers,
            save_every: spec.save_every.max(1),
            saves: SaveTotals::default(),
            now,
            departures: 0,
            arrivals: 0,
        }
    }

    fn departure_time(&self, k: u64) -> Tick {
        Tick(
            (u128::from(k) * u128::from(TICKS_PER_DAY) / u128::from(self.cfg.events_per_day))
                as u64,
        )
    }

    fn handle(&mut self, at: Tick, ev: Traffic) {
        match ev {
            Traffic::Depart(k) => {
                self.traffic
                    .schedule(self.departure_time(k + 1).max(at), Traffic::Depart(k + 1));
                let mut rng = self.traffic_key.slot(k);
                let from = rng.below(self.cfg.regions);
                let to = rng.below(self.cfg.regions);
                let travel = TICKS_PER_DAY + rng.below(2 * TICKS_PER_DAY);
                let taken = io(self.table.apply(&mut self.store, from, at, |m, _| {
                    let mut cargo = 0;
                    transfer_up_to(m.grain(), &mut cargo, CARGO)
                }))
                .expect("live region");
                self.in_transit += taken;
                self.departures += 1;
                self.traffic
                    .schedule(at + travel, Traffic::Arrive { to, amount: taken });
                // The arrival is known now: its page comes in ahead of it (SCALE.md §8).
                self.table.prefetch_at(to, at + travel);
            }
            Traffic::Arrive { to, amount } => {
                io(self
                    .table
                    .apply(&mut self.store, to, at, |m, _| *m.grain() += amount));
                self.in_transit -= amount;
                self.arrivals += 1;
            }
        }
    }

    fn observe(&mut self, at: Tick) {
        for o in 0..self.observers.len() {
            let (cx, cy) = (self.observers[o].x, self.observers[o].y);
            for dy in 0..3 {
                for dx in 0..3 {
                    let x = (cx + dx + self.side - 1) % self.side;
                    let y = (cy + dy + self.side - 1) % self.side;
                    let i = y * self.side + x;
                    if i < self.cfg.regions {
                        std::hint::black_box(io(self.table.read(&mut self.store, i, at)));
                    }
                }
            }
            if at.0.is_multiple_of(MOVE_EVERY) {
                self.observers[o].x = (self.observers[o].x + 1) % self.side;
                // Read-ahead along the path: the column two ahead of the new centre.
                let ahead = (self.observers[o].x + 2) % self.side;
                let keys: Vec<u64> = (0..3)
                    .map(|dy| ((cy + dy + self.side - 1) % self.side) * self.side + ahead)
                    .filter(|&k| k < self.cfg.regions)
                    .collect();
                self.table.read_ahead(keys);
            }
        }
    }

    /// One save, as at Minecraft's autosave: dirty pages and the table's meta, fsynced.
    pub fn save(&mut self) {
        let t0 = Instant::now();
        let c0 = crate::sys::thread_cpu_ns();
        let mut b = SaveBatch::new();
        let ticket = self.table.save_dirty(&mut b);
        self.table.save_meta(&mut b);
        let st = io(self.store.commit(b, 0));
        self.table.saved(ticket);
        self.saves.saves += 1;
        self.saves.wall_ns += t0.elapsed().as_nanos() as u64;
        if let (Some(a), Some(b)) = (c0, crate::sys::thread_cpu_ns()) {
            self.saves.cpu_ns += b - a;
        }
        self.saves.pages += st.pages_written;
        self.saves.segment_bytes += st.segment_bytes;
        self.saves.log_bytes += st.log_bytes;
        self.saves.checkpoint_bytes += st.checkpoint_bytes;
        self.saves.checkpoints += u64::from(st.checkpoint_bytes > 0);
    }

    /// Steps tick by tick to `end`: breakpoints, traffic, observers, then prefetch, saves and the
    /// budget at the step boundary.
    pub fn run_to(&mut self, end: Tick) {
        while self.now < end {
            self.now += 1;
            let now = self.now;
            while let Some((at, key)) = io(self.table.next_due(&mut self.store, now)) {
                io(self.table.apply(&mut self.store, key, at, |_, _| ()));
            }
            while let Some((at, ev)) = self.traffic.pop_until(now) {
                self.handle(at, ev);
            }
            if now.0.is_multiple_of(OBSERVE_EVERY) {
                self.observe(now);
            }
            io(self.table.prefetch(&mut self.store, now));
            if now.0.is_multiple_of(self.save_every) {
                self.save();
            }
            self.table.enforce_budget();
        }
    }

    pub fn stats(&self) -> PagingStats {
        self.table.stats()
    }

    /// Grain and head counts against the ledger, over the stored states. Loads every page (a
    /// pass over the world), outside the timed window, keeping to the budget as it goes.
    pub fn conservation(&mut self) -> Result<(), String> {
        let (mut grain, mut produced) = (0i64, 0i64);
        let (mut heads, mut born, mut died) = (0u64, 0u64, 0u64);
        let mut err = None;
        io(self.table.for_each(&mut self.store, |key, cell, _| {
            let m = cell.model();
            if let Err(e) = m.invariants() {
                err.get_or_insert(format!("region {key}: {e}"));
            }
            grain += m.grain_now();
            produced += m.produced();
            if let Some((h, b, d)) = m.heads() {
                heads += h;
                born += b;
                died += d;
            }
        }));
        if let Some(e) = err {
            return Err(e);
        }
        if grain + self.in_transit != self.initial_grain + produced {
            return Err(format!(
                "grain: held {grain} + in transit {} != initial {} + produced {produced}",
                self.in_transit, self.initial_grain
            ));
        }
        if heads + died != self.initial_heads + born {
            return Err(format!(
                "heads: {heads} + died {died} != initial {} + born {born}",
                self.initial_heads
            ));
        }
        Ok(())
    }

    /// Every region caught up to now and hashed: equal for equal worlds, whatever the budget.
    pub fn state_hash(&mut self) -> u32 {
        io(self.table.state_hash(&mut self.store, self.now))
    }

    pub fn heap_bytes(&self) -> usize {
        self.table.heap_bytes() + self.traffic.heap_bytes() + self.store.heap_bytes()
    }
}

impl<M: BenchModel + PagedModel> Drop for PagedWorld<M> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::world::ModelKind;
    use rtsim_kernel::synth::{DyadicRegion, Stock, SynthRegion};

    fn spec(model: ModelKind, budget: u64) -> PagedSpec {
        PagedSpec {
            world: WorldConfig {
                model,
                regions: 600,
                latent: 60_000,
                events_per_day: 300,
                observers: 2,
                regions_per_page: 8,
                warmup_days: 1,
                days: 2,
                seed: 11,
            },
            budget_bytes: budget,
            save_every: 6_000,
        }
    }

    fn run<M: BenchModel + PagedModel>(model: ModelKind, budget: u64) -> (u32, PagingStats) {
        let dir = std::env::temp_dir().join(format!(
            "rtsim-paged-world-{model:?}-{budget}-{}",
            std::process::id()
        ));
        let mut w: PagedWorld<M> = PagedWorld::build(&spec(model, budget), dir);
        w.run_to(Tick::from_days(3));
        w.conservation().unwrap();
        let st = w.stats();
        (w.state_hash(), st)
    }

    /// Paging changes nothing but memory: the same world at a tiny budget and an unbounded one
    /// ends in the same state, and conserves grain and heads.
    #[test]
    fn paging_changes_nothing_but_memory() {
        let (a, sa) = run::<SynthRegion>(ModelKind::Herd, u64::MAX);
        let (b, sb) = run::<SynthRegion>(ModelKind::Herd, 30_000);
        assert_eq!(sa.evictions, 0);
        assert!(sb.evictions > 0 && sb.loads > 0, "{sb:?}");
        assert_eq!(a, b);
        let (a, _) = run::<Stock>(ModelKind::Closed, u64::MAX);
        let (b, _) = run::<Stock>(ModelKind::Closed, 30_000);
        assert_eq!(a, b);
        let (a, _) = run::<DyadicRegion>(ModelKind::Dyadic, u64::MAX);
        let (b, sb) = run::<DyadicRegion>(ModelKind::Dyadic, 30_000);
        assert_eq!(a, b);
        assert!(sb.prefetches > 0, "{sb:?}");
    }
}
