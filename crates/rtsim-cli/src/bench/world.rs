// SPDX-License-Identifier: AGPL-3.0-or-later
//! The synthetic world of the scale bench (SCALE.md §10).
//!
//! - **Regions** are lazy cells of one model in a [`LazyTable`]: [`SynthRegion`] (a deterministic
//!   grain stock plus a herd whose births, ageing and deaths commit once per day slot) or a bare
//!   [`Stock`] (deterministic only, the closed-form case).
//! - **Latent individuals** are the herds' members: counts, never rows.
//! - **Events** are caravans: a departure takes grain from one region, and the arrival one to
//!   three days later gives it to another. Departures are evenly spaced through the day.
//! - **Observers** stand for players: each reads the 3×3 block of regions around it once a second
//!   and moves one region along every 1,000 ticks.
//! - **Pages** group regions by index; a region read or changed marks its page dirty.
//!
//! Every count a sweep does not vary stays at the baseline in `scale.rs`, and every rate is a
//! PLACEHOLDER chosen to keep the world busy and bounded, not to model anything.

use rtsim_kernel::clock::{TICKS_PER_DAY, TICKS_PER_SECOND, Tick};
use rtsim_kernel::codec::{Encode, Writer};
use rtsim_kernel::conserve::{Diffuser, transfer_up_to};
use rtsim_kernel::lazy::{LazyModel, LazyTable, RegionId};
use rtsim_kernel::page::{CommitStats, Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::{SlotRng, StreamKey, SystemId};
use rtsim_kernel::synth::{DyadicHerd, DyadicRegion, Herd, Stock, SynthRegion};
use rtsim_kernel::wheel::TimingWheel;
use serde::{Deserialize, Serialize};

/// Grain a caravan carries at most. PLACEHOLDER.
const CARGO: i64 = 50;
/// Grain a region produces per day, before per-region variation. PLACEHOLDER.
const PRODUCTION_PER_DAY: i64 = 100;
/// Ticks between observer reads: once a second.
const OBSERVE_EVERY: u64 = TICKS_PER_SECOND;
/// Ticks between observer moves.
const MOVE_EVERY: u64 = 1000;
const PAGE_TABLE: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    /// [`SynthRegion`]: random herd on day slots plus a deterministic stock.
    Herd,
    /// [`Stock`] only: deterministic, closed-form catch-up.
    Closed,
    /// [`DyadicRegion`]: a stock plus a herd whose randomness comes from dyadic counts, so it has
    /// no slots to commit (DECISIONS.md O17).
    Dyadic,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub model: ModelKind,
    pub regions: u64,
    /// Latent individuals across all herds. Ignored by the closed model.
    pub latent: u64,
    pub events_per_day: u64,
    pub observers: u64,
    pub regions_per_page: u64,
    pub warmup_days: u64,
    pub days: u64,
    pub seed: u64,
}

/// What a model needs to take part in the bench world.
pub trait BenchModel: LazyModel + Encode + Clone {
    fn make(index: u64, cfg: &WorldConfig, rng: &mut SlotRng) -> Self;
    fn grain(&mut self) -> &mut i64;
    fn grain_now(&self) -> i64;
    fn produced(&self) -> i64;
    /// (head count, born, died), or `None` without a herd.
    fn heads(&self) -> Option<(u64, u64, u64)>;
}

fn production(rng: &mut SlotRng) -> Diffuser {
    // 50% to 150% of the base rate.
    let per_day = PRODUCTION_PER_DAY / 2 + rng.below(PRODUCTION_PER_DAY as u64 + 1) as i64;
    Diffuser::per(per_day, TICKS_PER_DAY)
}

impl BenchModel for Stock {
    fn make(_index: u64, _cfg: &WorldConfig, rng: &mut SlotRng) -> Self {
        // No threshold: breakpoints are not what the sweeps vary.
        Stock::new(100, production(rng), i64::MAX)
    }
    fn grain(&mut self) -> &mut i64 {
        &mut self.amount
    }
    fn grain_now(&self) -> i64 {
        self.amount
    }
    fn produced(&self) -> i64 {
        self.produced
    }
    fn heads(&self) -> Option<(u64, u64, u64)> {
        None
    }
}

/// Region `index`'s share of the latent individuals, split young, adult, old.
fn cohorts(index: u64, cfg: &WorldConfig) -> [u64; 3] {
    let base = cfg.latent / cfg.regions;
    let n = base + u64::from(index < cfg.latent % cfg.regions);
    let young = n / 4;
    let old = n * 15 / 100;
    [young, n - young - old, old]
}

impl BenchModel for SynthRegion {
    fn make(index: u64, cfg: &WorldConfig, rng: &mut SlotRng) -> Self {
        SynthRegion {
            stock: Stock::new(100, production(rng), i64::MAX),
            herd: Herd::new(rng.next_u64(), cohorts(index, cfg)),
        }
    }
    fn grain(&mut self) -> &mut i64 {
        &mut self.stock.amount
    }
    fn grain_now(&self) -> i64 {
        self.stock.amount
    }
    fn produced(&self) -> i64 {
        self.stock.produced
    }
    fn heads(&self) -> Option<(u64, u64, u64)> {
        Some((self.herd.total(), self.herd.born, self.herd.died))
    }
}

impl BenchModel for DyadicRegion {
    fn make(index: u64, cfg: &WorldConfig, rng: &mut SlotRng) -> Self {
        DyadicRegion {
            stock: Stock::new(100, production(rng), i64::MAX),
            herd: DyadicHerd::new(cfg.seed, index, 0, cohorts(index, cfg)),
            day: 0,
        }
    }
    fn grain(&mut self) -> &mut i64 {
        &mut self.stock.amount
    }
    fn grain_now(&self) -> i64 {
        self.stock.amount
    }
    fn produced(&self) -> i64 {
        self.stock.produced
    }
    fn heads(&self) -> Option<(u64, u64, u64)> {
        // At the anchor's day: deaths are "died", immigrants are "born".
        let (deaths, immigrants) = self.herd.changes(self.day);
        let total = self.herd.counts(self.day).iter().sum();
        Some((total, immigrants, deaths.iter().sum()))
    }
}

#[derive(Clone, Copy, Debug)]
enum Traffic {
    Depart(u64),
    Arrive { to: u32, amount: i64 },
}

struct Observer {
    x: u64,
    y: u64,
}

pub struct World<M: BenchModel> {
    pub cfg: WorldConfig,
    pub table: LazyTable<M>,
    ids: Vec<RegionId<M>>,
    traffic: TimingWheel<Traffic>,
    traffic_key: StreamKey,
    in_transit: i64,
    initial_grain: i64,
    initial_heads: u64,
    side: u64,
    observers: Vec<Observer>,
    dirty: Vec<u64>,
    pub now: Tick,
    pub departures: u64,
    pub arrivals: u64,
}

impl<M: BenchModel> World<M> {
    pub fn build(cfg: WorldConfig) -> World<M> {
        let now = Tick::ZERO;
        let mut table = LazyTable::new(cfg.seed, SystemId::named("bench.region"), now);
        let setup = StreamKey::new(cfg.seed, SystemId::named("bench.setup"), 0);
        let mut ids = Vec::with_capacity(cfg.regions as usize);
        let (mut initial_grain, mut initial_heads) = (0, 0);
        for i in 0..cfg.regions {
            let mut rng = setup.slot(i);
            let m = M::make(i, &cfg, &mut rng);
            initial_grain += m.grain_now();
            initial_heads += m.heads().map_or(0, |h| h.0);
            ids.push(table.insert(i, m, now));
        }
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
        let pages = cfg.regions.div_ceil(cfg.regions_per_page);
        World {
            traffic_key: StreamKey::new(cfg.seed, SystemId::named("bench.traffic"), 0),
            dirty: vec![0; pages.div_ceil(64) as usize],
            cfg,
            table,
            ids,
            traffic,
            in_transit: 0,
            initial_grain,
            initial_heads,
            side,
            observers,
            now,
            departures: 0,
            arrivals: 0,
        }
    }

    fn touch(&mut self, index: u64) {
        let page = index / self.cfg.regions_per_page;
        self.dirty[(page / 64) as usize] |= 1 << (page % 64);
    }

    fn departure_time(&self, k: u64) -> Tick {
        // Evenly spaced: departure k at floor(k · day / E).
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
                let taken = self
                    .table
                    .apply(self.ids[from as usize], at, |m, _| {
                        let mut cargo = 0;
                        transfer_up_to(m.grain(), &mut cargo, CARGO)
                    })
                    .expect("live region");
                self.touch(from);
                self.in_transit += taken;
                self.departures += 1;
                self.traffic.schedule(
                    at + travel,
                    Traffic::Arrive {
                        to: to as u32,
                        amount: taken,
                    },
                );
            }
            Traffic::Arrive { to, amount } => {
                self.table
                    .apply(self.ids[to as usize], at, |m, _| *m.grain() += amount);
                self.touch(u64::from(to));
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
                        std::hint::black_box(self.table.read(self.ids[i as usize], at));
                        self.touch(i);
                    }
                }
            }
            if at.0.is_multiple_of(MOVE_EVERY) {
                self.observers[o].x = (self.observers[o].x + 1) % self.side;
            }
        }
    }

    /// Steps tick by tick to `end`, as the engine's step job would at 20 Hz.
    pub fn run_to(&mut self, end: Tick) {
        while self.now < end {
            self.now += 1;
            let now = self.now;
            while let Some((at, ev)) = self.traffic.pop_until(now) {
                self.handle(at, ev);
            }
            while let Some((at, h)) = self.table.next_due(now) {
                // No synthetic model books breakpoints; a real module would act here.
                self.table.apply(h, at, |_, _| ());
            }
            if now.0.is_multiple_of(OBSERVE_EVERY) {
                self.observe(now);
            }
        }
    }

    /// Checks that grain and head counts are conserved. Costs a pass over every region, so it
    /// runs outside the timed window.
    pub fn conservation(&self) -> Result<(), String> {
        let (mut grain, mut produced) = (0i64, 0i64);
        let (mut heads, mut born, mut died) = (0u64, 0u64, 0u64);
        for (_, r) in self.table.iter() {
            let m = r.cell().model();
            grain += m.grain_now();
            produced += m.produced();
            if let Some((h, b, d)) = m.heads() {
                heads += h;
                born += b;
                died += d;
            }
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

    fn page_bytes(&self, page: u64) -> Vec<u8> {
        let lo = page * self.cfg.regions_per_page;
        let hi = (lo + self.cfg.regions_per_page).min(self.cfg.regions);
        let mut w = Writer::with_capacity(64 * (hi - lo) as usize);
        for i in lo..hi {
            let r = self.table.get(self.ids[i as usize]).expect("live region");
            w.var_u64(r.cell().anchor().0);
            r.cell().model().encode(&mut w);
        }
        w.into_bytes()
    }

    /// Writes every page. Returns the commit's stats.
    pub fn save_full(&mut self, store: &mut PageStore) -> CommitStats {
        let mut batch = SaveBatch::new();
        for p in 0..self.cfg.regions.div_ceil(self.cfg.regions_per_page) {
            batch.put(PageKey::new(PAGE_TABLE, p), &self.page_bytes(p));
        }
        self.dirty.iter_mut().for_each(|w| *w = 0);
        store.commit(batch, 0).expect("full save")
    }

    /// Writes the pages touched since the last save.
    pub fn save_dirty(&mut self, store: &mut PageStore) -> (u64, CommitStats) {
        let mut batch = SaveBatch::new();
        let mut pages = 0;
        for w in 0..self.dirty.len() {
            let mut bits = self.dirty[w];
            while bits != 0 {
                let p = w as u64 * 64 + u64::from(bits.trailing_zeros());
                bits &= bits - 1;
                batch.put(PageKey::new(PAGE_TABLE, p), &self.page_bytes(p));
                pages += 1;
            }
            self.dirty[w] = 0;
        }
        (pages, store.commit(batch, 0).expect("incremental save"))
    }

    /// A CRC32 over every page: equal for equal worlds, for reproducibility checks.
    pub fn state_hash(&self) -> u32 {
        let mut h = crc32fast::Hasher::new();
        for p in 0..self.cfg.regions.div_ceil(self.cfg.regions_per_page) {
            h.update(&self.page_bytes(p));
        }
        h.finalize()
    }

    pub fn heap_bytes(&self) -> usize {
        self.table.heap_bytes()
            + self.traffic.heap_bytes()
            + self.ids.capacity() * std::mem::size_of::<RegionId<M>>()
            + self.dirty.capacity() * 8
    }
}

/// Opens a page store in a fresh scratch folder.
pub fn scratch_store(tag: &str) -> (std::path::PathBuf, PageStore) {
    let dir = std::env::temp_dir().join(format!("rtsim-bench-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = PageStore::open(&dir, Durability::Fsync).expect("open scratch page store");
    (dir, store)
}
