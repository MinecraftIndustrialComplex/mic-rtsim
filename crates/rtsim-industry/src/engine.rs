// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine: observations in, commands out (INDUSTRY.md; C15 to C19).
//!
//! # Time
//!
//! Everything runs on game ticks (I4: nothing passes while the server is stopped). Observations
//! are handled in the order given, and each first advances the engine to its time: every replay
//! breakpoint and natural-pressure wake due by then fires first, in time order, which is the order
//! the kernel's `LazyTable` requires (DECISIONS.md O18). An observation stamped before the engine's
//! time (replayed from the journal after a restart, or late) is taken as happening now.
//!
//! # Cost (INDUSTRY.md §15, SCALE.md §0)
//!
//! - **Replay:** O(breakpoints + arrivals + attacks), never elapsed time or district count. A
//!   district nobody reads and nothing reaches costs nothing per tick.
//! - **Learning:** O(samples), only for loaded districts.
//! - **Reconcile:** O(deltas), once per load.
//! - **Threats:** O(attacks): natural pressure books one wake per attack, not one per night.
//!
//! The `bench industry` sweep checks the district-count slope.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::journal::OpId;
use rtsim_kernel::lazy::{LazyTable, RegionId};
use rtsim_kernel::rng::SystemId;
use serde::Serialize;

use crate::cluster::{Cluster, ClusterEvent, ClusterView, PoolKey, Run, Stop};
use crate::cmd::{
    BlockDestroy, CarriageDelta, CarriageReason, ChannelDelta, ChunkReconcile, Cmd, DepositDelta,
    IndustryOps, InvDelta, LedgerLine, WritePackage,
};
use crate::deposit::{self, Writeback};
use crate::district::{
    AttackReport, DEPOSIT_INVENTORY, District, Inventory, Model, PendingChunk, Phase, Presence,
    ReconcileReport, SimSpan, SweepBuf,
};
use crate::ids::{
    BlockPos, ChannelId, ChunkPos, DistrictId, Good, GoodClass, GoodInfo, GoodVec, StationId,
    TrainId,
};
use crate::learn::{LinkKey, Sample};
use crate::obs::{
    Carriage, CarriageResult, Conditions, Direction, LoadState, Obs, PackageEvent, PortKind,
    ReconcileResult, StationDecl, Timed, TrainCall, TrainDeparture,
};
use crate::reconcile::{self, GoodLookup};
use crate::threat::{
    self, Leg, LegIndex, Threat, ThreatClass, ThreatSource, ThreatTarget, next_pressure_attack,
};
use crate::tunables::Tunables;

/// The replay table's system id (the replay draws nothing; the id only names the table).
const REPLAY: SystemId = SystemId::named("industry.replay");

/// The tick of each game day natural attacks land on. TUNABLE, PLACEHOLDER 18,000 (`dayTime`
/// midnight; gameTime and dayTime drift apart when players sleep, INDUSTRY.md §0).
pub const NIGHT_ATTACK_TICK: u64 = 18_000;

/// One natural mob's strength in fight units. TUNABLE, PLACEHOLDER 1.0.
pub const NATURAL_STRENGTH: f64 = 1.0;

/// Real-time hour in ticks at 20 per second: the ledger's "per hour".
pub const TICKS_PER_HOUR: u64 = 72_000;

/// Phase I-A shows what districts would have made and changes nothing (I26); I-B on applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Mode {
    ReadOnly,
    Apply,
}

/// The ledger's states (docs/industry-boundary.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LedgerState {
    Live = 0,
    Learning = 1,
    Running = 2,
    Stale = 3,
    NotLearnable = 4,
    Suspended = 5,
    Damaged = 6,
    OutOfSeason = 7,
}

/// What a district would have made while unloaded, and why it stands as it does (I-A).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LedgerReport {
    pub district: DistrictId,
    pub state: LedgerState,
    pub reason: Option<String>,
    /// Learned rates, units per real hour.
    pub rates_per_hour: Vec<(Good, f64)>,
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub since: Option<Tick>,
    /// Net change per good since the replay began: what it made (positive) and used (negative).
    pub made_since: GoodVec,
    pub running: Option<bool>,
    pub stop: Option<String>,
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub next_breakpoint: Option<Tick>,
    /// What happens then.
    pub next: Option<crate::cluster::Forecast>,
    pub learning_ticks_to_go: Option<u64>,
    pub last_reconcile: Option<ReconcileReport>,
    pub attacks: Vec<AttackReport>,
}

/// Station-to-station traffic seen, for forecasts and threat bookings (INDUSTRY.md §8.8).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LineEdge {
    pub trips: u64,
    pub cargo: GoodVec,
    pub travel_ticks: u64,
    #[serde(serialize_with = "crate::ser::tick")]
    pub first: Tick,
    #[serde(serialize_with = "crate::ser::tick")]
    pub last: Tick,
}

/// Counters for the CLI, tests and the bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub observations: u64,
    pub models_adopted: u64,
    pub sims_started: u64,
    pub reconciles: u64,
    pub chunk_commands: u64,
    pub duplicate_results: u64,
    pub trims: u64,
    pub exchanges: u64,
    pub attacks: u64,
    pub attacks_skipped: u64,
    pub packages_written: u64,
    pub breakpoints: u64,
    pub cluster_settles: u64,
}

/// World-level books: what the engine made, used, and moved out of the player's stores. With the
/// fake world's own books they close the end-to-end conservation check.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Books {
    pub minted: GoodVec,
    pub burned: GoodVec,
    pub stolen: GoodVec,
    pub dropped: GoodVec,
    /// Trim removals that came up short: outputs the world kept (docs §4.2).
    pub untracked: GoodVec,
    /// Inputs a removal could not take because a machine took them first (I8).
    pub shortfall: GoodVec,
    /// Outputs not delivered, or taken back, because of shortfalls.
    pub trimmed: GoodVec,
}

/// One line of the engine's own log, for the CLI and tests. Capped (TUNABLE, PLACEHOLDER 20,000
/// lines); the oldest go first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EngineEvent {
    pub tick: u64,
    pub district: u64,
    pub what: String,
}

const LOG_CAP: usize = 20_000;

#[derive(Clone, Debug, PartialEq)]
struct Station {
    decl: StationDecl,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ChannelState {
    known: GoodVec,
    capacity: i64,
}

#[derive(Clone, Debug, PartialEq)]
struct PendingExchange {
    district: DistrictId,
    train: TrainId,
    /// (carriage, good, delta into the carriage) asked.
    entries: Vec<(u32, Good, i64)>,
    /// Loot from a train leg (I24) rather than a station exchange.
    loot: bool,
}

pub struct Engine {
    pub tun: Tunables,
    pub mode: Mode,
    world_seed: u64,
    goods: BTreeMap<Good, GoodInfo>,
    districts: BTreeMap<DistrictId, District>,
    clusters: LazyTable<Cluster>,
    cluster_of: BTreeMap<DistrictId, RegionId<Cluster>>,
    next_cluster: u64,
    stations: BTreeMap<StationId, Station>,
    lines: BTreeMap<(StationId, StationId), LineEdge>,
    last_departure: BTreeMap<TrainId, (StationId, Tick, GoodVec)>,
    arrivals: BTreeMap<(TrainId, StationId), Vec<Carriage>>,
    legs: LegIndex,
    legs_swept: u64,
    channels: BTreeMap<ChannelId, ChannelState>,
    /// Simulated districts linked to each channel, so coupling never scans every district.
    channel_sims: BTreeMap<ChannelId, BTreeSet<DistrictId>>,
    /// Power sinks of each source (IND_POWER_LINK).
    sinks_of: BTreeMap<DistrictId, BTreeSet<DistrictId>>,
    conditions: Option<Conditions>,
    pressure: BTreeMap<(Tick, u64), (DistrictId, ThreatClass, u32, u32)>,
    pressure_seq: u64,
    exchanges: BTreeMap<rtsim_kernel::journal::OpId, PendingExchange>,
    /// Per train and threat class: no attack lands before this tick (I25).
    train_cooldown: BTreeMap<TrainId, [Tick; ThreatClass::COUNT]>,
    ops: IndustryOps,
    out: Vec<Cmd>,
    now: Tick,
    pub stats: Stats,
    pub books: Books,
    pub log: std::collections::VecDeque<EngineEvent>,
    /// Keep [`Engine::log`]: off by default (formatting lines costs every breakpoint); the fake
    /// world and the CLI turn it on.
    pub log_enabled: bool,
}

impl Engine {
    pub fn new(world_seed: u64, tun: Tunables, mode: Mode, now: Tick) -> Self {
        Self {
            tun,
            mode,
            world_seed,
            goods: BTreeMap::new(),
            districts: BTreeMap::new(),
            clusters: LazyTable::new(world_seed, REPLAY, now),
            cluster_of: BTreeMap::new(),
            next_cluster: 1,
            stations: BTreeMap::new(),
            lines: BTreeMap::new(),
            last_departure: BTreeMap::new(),
            arrivals: BTreeMap::new(),
            legs: LegIndex::default(),
            legs_swept: 0,
            channels: BTreeMap::new(),
            channel_sims: BTreeMap::new(),
            sinks_of: BTreeMap::new(),
            conditions: None,
            pressure: BTreeMap::new(),
            pressure_seq: 0,
            exchanges: BTreeMap::new(),
            train_cooldown: BTreeMap::new(),
            ops: IndustryOps::default(),
            out: Vec::new(),
            now,
            stats: Stats::default(),
            books: Books::default(),
            log: std::collections::VecDeque::new(),
            log_enabled: false,
        }
    }

    fn note(&mut self, d: DistrictId, at: Tick, what: String) {
        if !self.log_enabled {
            return;
        }
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(EngineEvent {
            tick: at.0,
            district: d.0,
            what,
        });
    }

    fn good_name(&self, g: Good) -> String {
        self.goods
            .get(&g)
            .map_or_else(|| format!("good {}", g.0), |i| i.name.clone())
    }

    fn goods_words(&self, v: &GoodVec) -> String {
        let parts: Vec<String> = v
            .iter()
            .map(|(g, n)| format!("{n:+} {}", self.good_name(g)))
            .collect();
        if parts.is_empty() {
            "nothing".to_string()
        } else {
            parts.join(", ")
        }
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    /// A GOOD dictionary entry from Java (docs/industry-boundary.md §2.2).
    pub fn define_good(&mut self, g: Good, info: GoodInfo) {
        self.goods.insert(g, info);
    }

    pub fn goods(&self) -> &BTreeMap<Good, GoodInfo> {
        &self.goods
    }

    pub fn district(&self, d: DistrictId) -> Option<&District> {
        self.districts.get(&d)
    }

    pub fn districts(&self) -> impl Iterator<Item = &District> {
        self.districts.values()
    }

    pub fn lines(&self) -> &BTreeMap<(StationId, StationId), LineEdge> {
        &self.lines
    }

    pub fn legs(&self) -> &LegIndex {
        &self.legs
    }

    pub fn cluster_count(&self) -> usize {
        self.clusters.len()
    }

    pub fn heap_bytes(&self) -> usize {
        self.clusters.heap_bytes()
            + self
                .clusters
                .iter()
                .map(|(_, r)| r.cell().model().heap_bytes())
                .sum::<usize>()
    }

    /// Commands for the Java side, in order.
    pub fn drain_commands(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.out)
    }

    /// A session start (the bridge's SESSION_START): open learning windows are lost with the
    /// latest-wins samples that were in flight, so they are discarded (F6). Every district is
    /// unloaded until its chunks report in.
    pub fn session_start(&mut self, at: Tick) {
        self.advance_to(at);
        let ids: Vec<DistrictId> = self.districts.keys().copied().collect();
        for d in ids {
            let dist = self.districts.get_mut(&d).expect("listed");
            dist.learner.discard();
            dist.sweep = None;
            dist.loaded_chunks.clear();
            if dist.presence != Presence::Unloaded {
                self.set_presence(d, Presence::Unloaded, at);
            }
        }
    }

    // ---- time -----------------------------------------------------------------------------

    /// Fires every breakpoint and pressure wake due by `to`, in time order.
    pub fn advance_to(&mut self, to: Tick) {
        if to < self.now {
            return;
        }
        loop {
            let next_p = self
                .pressure
                .keys()
                .next()
                .map(|k| k.0)
                .filter(|&t| t <= to);
            let limit = next_p.unwrap_or(to);
            while let Some((at, h)) = self.clusters.next_due(limit) {
                self.stats.breakpoints += 1;
                self.after_cluster(h, at);
            }
            match next_p {
                Some(t) => {
                    self.now = self.now.max(t);
                    let (key, wake) = self.pressure.pop_first().expect("peeked");
                    self.pressure_wake(key.0, wake);
                }
                None => break,
            }
        }
        self.now = to;
    }

    /// Handles one observation.
    pub fn ingest(&mut self, t: &Timed) {
        let at = t.at.max(self.now);
        self.advance_to(at);
        self.stats.observations += 1;
        match &t.obs {
            Obs::StockSample {
                district,
                inventory,
                sweep,
                goods,
            } => {
                let dist = self.district_mut(*district);
                let buf = dist.sweep.get_or_insert_with(SweepBuf::default);
                if buf.sweep != *sweep {
                    *buf = SweepBuf {
                        sweep: *sweep,
                        ..SweepBuf::default()
                    };
                }
                buf.records += 1;
                buf.contents.entry(*inventory).or_default().merge(goods);
                buf.read_at.entry(*inventory).or_insert(at);
            }
            Obs::DepositSample(s) => {
                let dist = self.district_mut(s.district);
                let buf = dist.sweep.get_or_insert_with(SweepBuf::default);
                if buf.sweep != s.sweep {
                    *buf = SweepBuf {
                        sweep: s.sweep,
                        ..SweepBuf::default()
                    };
                }
                buf.records += 1;
                buf.deposits.push(s.clone());
            }
            Obs::SweepEnd(e) => self.sweep_end(at, e),
            Obs::ChannelSample {
                channel,
                capacity,
                goods,
            } => {
                let c = self.channels.entry(*channel).or_default();
                c.known = goods.clone();
                c.capacity = *capacity;
            }
            Obs::LedgerRequest { district } => {
                let line = self.ledger_line(*district, at);
                self.out.push(Cmd::Ledger(line));
            }
            Obs::Approach {
                district,
                distance,
                eta_ticks,
                closing,
            } => {
                // Data only (W16): the stale-tick race's policy is not decided.
                if let Some(dist) = self.districts.get_mut(district) {
                    dist.approach = Some(crate::district::Approach {
                        at,
                        distance: *distance,
                        eta_ticks: *eta_ticks,
                        closing: *closing,
                    });
                }
            }
            Obs::Watched { district, watched } => {
                let dist = self.district_mut(*district);
                let w = *watched && dist.presence != Presence::Unloaded;
                if dist.watched != w {
                    dist.watched = w;
                    self.schedule_pressure(*district, at);
                }
            }
            Obs::District(decl) => {
                let dist = self.district_mut(decl.district);
                if dist.revision != decl.revision {
                    if dist.revision != 0 && dist.phase == Phase::Learned {
                        dist.phase = Phase::Stale;
                    }
                    dist.revision = decl.revision;
                    dist.chunks.clear();
                    dist.inventories.clear();
                    dist.learner.discard();
                }
                dist.dim = decl.dim;
                dist.name = decl.name;
                dist.not_learnable = decl.not_learnable;
                if let Some(n) = decl.not_learnable {
                    dist.phase = Phase::NotLearnable(n.code());
                } else if matches!(dist.phase, Phase::NotLearnable(_)) {
                    dist.phase = Phase::Learning;
                }
            }
            Obs::DistrictChunks {
                district,
                revision,
                chunks,
                ..
            } => {
                let dist = self.district_mut(*district);
                if dist.revision == *revision {
                    dist.chunks.extend(chunks.iter().copied());
                }
            }
            Obs::Inventory(decl) => {
                let dist = self.district_mut(decl.district);
                if dist.revision == decl.revision {
                    let dim = dist.dim;
                    dist.inventories.insert(
                        decl.index,
                        Inventory {
                            decl: decl.clone(),
                            chunk: decl.pos.chunk(dim),
                            known: GoodVec::new(),
                            known_at: Tick::ZERO,
                            pending: GoodVec::new(),
                            received: BTreeSet::new(),
                            doomed: false,
                            destroyed: false,
                        },
                    );
                }
            }
            Obs::DistrictRemoved { district } => {
                if self.districts.contains_key(district) {
                    self.stop_sim(*district, at);
                    self.districts.remove(district);
                }
            }
            Obs::Edit { district, .. } => {
                // Every IND_EDIT is a player's (§3.3): a learned district goes stale (I5).
                let dist = self.district_mut(*district);
                dist.learner.discard();
                if dist.phase == Phase::Learned {
                    dist.phase = Phase::Stale;
                }
            }
            Obs::Discard { district, .. } => self.district_mut(*district).learner.discard(),
            Obs::LoadState {
                district, state, ..
            } => {
                let p = match state {
                    LoadState::Unloaded => Presence::Unloaded,
                    LoadState::Partial => Presence::Partial,
                    LoadState::Live => Presence::Live,
                };
                self.district_mut(*district);
                self.set_presence(*district, p, at);
            }
            Obs::ChunkLoaded {
                district,
                chunk,
                applied_epoch,
            } => {
                self.district_mut(*district).loaded_chunks.insert(*chunk);
                // The chunk's attachment says an epoch is applied whose result never came (a
                // crash between the chunk's save and the journal's): it was applied as asked
                // (review finding 13). A late real result is then a duplicate.
                let done: Vec<ChunkReconcile> = self
                    .districts
                    .get(district)
                    .and_then(|x| x.pending.get(chunk))
                    .into_iter()
                    .flatten()
                    .filter(|p| p.cmd.epoch <= *applied_epoch)
                    .map(|p| p.cmd.clone())
                    .collect();
                for c in done {
                    let r = ReconcileResult {
                        op: c.op,
                        district: c.district,
                        epoch: c.epoch,
                        chunk: c.chunk,
                        entries: c
                            .deltas
                            .iter()
                            .map(|x| crate::obs::ResultEntry {
                                inventory: x.inventory,
                                good: x.good,
                                requested: x.delta,
                                applied: x.delta,
                            })
                            .collect(),
                        destroyed: c.destroy.iter().map(|b| b.pos).collect(),
                        blocks_skipped: 0,
                    };
                    self.reconcile_result(at, &r);
                }
                self.send_pending(*district, *chunk, *applied_epoch);
            }
            Obs::Conditions(c) => self.conditions = Some(*c),
            Obs::PowerLink { sink, source } => {
                let old = std::mem::replace(&mut self.district_mut(*sink).power_source, *source);
                if let Some(o) = old
                    && let Some(set) = self.sinks_of.get_mut(&o)
                {
                    set.remove(sink);
                }
                if let Some(s) = source {
                    self.sinks_of.entry(*s).or_default().insert(*sink);
                }
            }
            Obs::Port(p) => self.port(at, p),
            Obs::TrainArrived(c) => self.train_arrived(at, c),
            Obs::TrainDeparted(d) => self.train_departed(at, d),
            Obs::Station(s) => {
                if let Some(d) = s.district {
                    self.district_mut(d).stations.insert(s.station);
                }
                self.stations.insert(s.station, Station { decl: s.clone() });
            }
            Obs::Package(p) => self.package(at, p),
            Obs::ReconcileResult(r) => self.reconcile_result(at, r),
            Obs::CarriageResult(r) => self.carriage_result(at, r),
            Obs::SpawnSample(s) => {
                let dist = self.district_mut(s.district);
                dist.defence.pressure.add(s);
                self.schedule_pressure(s.district, at);
            }
            Obs::Fight(f) => self.district_mut(f.district).defence.add_fight(f),
            Obs::Survey(s) => {
                let dist = self.district_mut(s.district);
                dist.defence.set_survey(s);
                if s.revision != dist.revision {
                    dist.defence.exposed.clear();
                }
                self.schedule_pressure(s.district, at);
            }
            Obs::SurveyBlocks {
                district,
                part,
                blocks,
                ..
            } => {
                let dist = self.district_mut(*district);
                if *part == 0 {
                    dist.defence.exposed.clear();
                }
                dist.defence
                    .exposed
                    .extend(blocks.iter().map(threat::ExposedBlockRow::from));
            }
        }
    }

    fn district_mut(&mut self, d: DistrictId) -> &mut District {
        self.districts.entry(d).or_insert_with(|| District::new(d))
    }

    /// Adopts a model as if a steady window had produced it (tests and the bench; the real path is
    /// [`Engine::ingest`] with live sweeps). Scaffolding.
    pub fn adopt_model(
        &mut self,
        d: DistrictId,
        learned: crate::learn::Learned,
        targets: Vec<crate::learn::LearnedTarget>,
    ) {
        let season = self.season_at(self.now);
        let dist = self.district_mut(d);
        let batches = dist.batches.clone();
        dist.models.insert(
            season,
            Model::from_learned(learned, targets, season, batches),
        );
        dist.phase = Phase::Learned;
        self.stats.models_adopted += 1;
    }

    /// An import into a simulated district's stock at the engine's time, as an emulated station
    /// unload would make (the bench drives restarts with it). Scaffolding. Returns what was added,
    /// or `None` if the district is not replaying.
    pub fn inject(&mut self, d: DistrictId, g: Good, n: i64) -> Option<i64> {
        let h = *self.cluster_of.get(&d)?;
        let at = self.now;
        let got = self.clusters.apply(h, at, |c, at| {
            c.add_to_pool(PoolKey::Stock(d, g), n, false, at)
        });
        self.after_cluster(h, at);
        got
    }

    // ---- seasons --------------------------------------------------------------------------

    /// The season at `t` (I12), 0 without a season mod.
    pub fn season_at(&self, t: Tick) -> u32 {
        match self.conditions {
            Some(c) if c.season_length > 0 && c.season_count > 0 => {
                let since = t.0.saturating_sub(c.season_origin.0);
                ((since / c.season_length) % u64::from(c.season_count)) as u32
            }
            _ => 0,
        }
    }

    fn next_season_boundary(&self, t: Tick) -> Option<Tick> {
        let c = self.conditions?;
        if c.season_length == 0 || c.season_count <= 1 {
            return None;
        }
        let since = t.0.saturating_sub(c.season_origin.0);
        Some(Tick(
            c.season_origin.0 + (since / c.season_length + 1) * c.season_length,
        ))
    }

    // ---- sweeps and learning --------------------------------------------------------------

    fn sweep_end(&mut self, at: Tick, e: &crate::obs::SweepEnd) {
        let season = self.season_at(at);
        // Split borrows: no clone of the GOOD dictionary per sweep (review finding 16).
        let tun = &self.tun.learn;
        let goods = &self.goods;
        let dist = self
            .districts
            .entry(e.district)
            .or_insert_with(|| District::new(e.district));
        let Some(buf) = dist.sweep.take() else {
            return;
        };
        if buf.sweep != e.sweep || buf.records != e.records {
            // A latest-wins record was dropped: the whole sweep goes (F5).
            return;
        }
        let read_at = buf.read_at;
        for (idx, contents) in buf.contents {
            if let Some(inv) = dist.inventories.get_mut(&idx) {
                // Sampled again: the block was rebuilt.
                inv.destroyed = false;
                if e.live {
                    for (g, c) in contents.iter() {
                        if c > inv.known.get(g) {
                            inv.received.insert(g);
                        }
                    }
                }
                inv.known = contents;
                inv.known_at = read_at.get(&idx).copied().unwrap_or(at);
            }
        }
        for s in buf.deposits {
            dist.deposits.insert(s.good, s);
        }
        if !e.live || e.creative || e.overstressed {
            if e.creative || e.overstressed {
                dist.learner.discard();
            }
            return;
        }
        if matches!(dist.phase, Phase::NotLearnable(_)) {
            return;
        }
        let stock = dist.effective_stock();
        let links = dist.cumulative_links();
        let space_free = space_free(dist, goods);
        let current = dist.models.get(&season).map(|m| m.learned.clone());
        let ports = dist.ports.clone();
        if let Some(learned) = dist.learner.push(
            Sample {
                at,
                stock: &stock,
                ports: &ports,
                links: &links,
                space_free,
            },
            current.as_ref(),
            tun,
        ) {
            let targets = dist.learner.targets(&learned, tun);
            let batches = dist.batches.clone();
            let was = dist.phase;
            let old_goods: Vec<Good> = current
                .as_ref()
                .map_or(Vec::new(), |m| m.rates.iter().map(|r| r.0).collect());
            dist.models.insert(
                season,
                Model::from_learned(learned, targets, season, batches),
            );
            dist.phase = Phase::Learned;
            let m = &dist.models[&season];
            let new_goods: Vec<Good> = m.learned.rates.iter().map(|r| r.0).collect();
            let changed = was != Phase::Learned || old_goods != new_goods;
            let rates: Vec<String> = m
                .learned
                .rates
                .iter()
                .map(|&(g, r)| {
                    format!(
                        "{:+.0}/h {}",
                        r * TICKS_PER_HOUR as f64,
                        goods.get(&g).map_or("?", |i| i.name.as_str())
                    )
                })
                .collect();
            let what = format!(
                "learned (window {} ticks, confidence {:.2}): {}",
                m.learned.window_ticks,
                m.learned.confidence,
                if rates.is_empty() {
                    "idle".to_string()
                } else {
                    rates.join(", ")
                }
            );
            self.stats.models_adopted += 1;
            if changed {
                self.note(e.district, at, what);
            }
        } else if let Some(m) = current {
            // Targets keep improving between windows.
            let targets = dist.learner.targets(&m, tun);
            if let Some(model) = dist.models.get_mut(&season)
                && !targets.is_empty()
            {
                model.targets = targets;
            }
        }
    }

    // ---- presence -------------------------------------------------------------------------

    fn set_presence(&mut self, d: DistrictId, p: Presence, at: Tick) {
        let old = self.districts[&d].presence;
        if old == p {
            return;
        }
        self.districts.get_mut(&d).expect("exists").presence = p;
        match (old, p) {
            (Presence::Unloaded, _) => {
                // The first chunk ticks: catch up, stop, reconcile (I7).
                let dist = self.districts.get_mut(&d).expect("exists");
                dist.pressure_stamp += 1;
                self.reconcile_on_load(d, at);
                // Live now: its power sinks may run (F21), whether or not it was replaying.
                self.refresh_power(d, at);
            }
            (_, Presence::Unloaded) => {
                let dist = self.districts.get_mut(&d).expect("exists");
                dist.watched = false;
                // An unloaded gap must not sit inside a learning window.
                dist.learner.discard();
                dist.loaded_chunks.clear();
                self.start_sim(d, at);
                self.schedule_pressure(d, at);
            }
            _ => {}
        }
    }

    /// A learned, fresh district starts its replay from its effective stock (I5, I6).
    fn start_sim(&mut self, d: DistrictId, at: Tick) {
        let season = self.season_at(at);
        let dist = &self.districts[&d];
        if dist.freeze_reason(season).is_some() || dist.sim.is_some() {
            return;
        }
        let model = dist.models[&season].clone();
        let base = dist.effective_stock();
        // Only the channels this model links (review finding 16).
        let channels: BTreeMap<ChannelId, (GoodVec, i64)> = model
            .links
            .iter()
            .filter_map(|(k, _)| match k {
                LinkKey::Channel(c, _) => self
                    .channels
                    .get(c)
                    .map(|s| (*c, (s.known.clone(), s.capacity))),
                LinkKey::Mail { .. } => None,
            })
            .collect();
        let mut spec = dist.member_spec(&model, &base, &channels, &self.goods, &self.tun);
        // A power source that is live is assumed running (F21); one that is unloaded and not
        // simulated keeps the requirement, which nothing in the cluster meets, so the sink waits.
        if let Some(src) = dist.power_source
            && self
                .districts
                .get(&src)
                .is_some_and(|s| s.presence != Presence::Unloaded)
        {
            for p in &mut spec.procs {
                p.requires = None;
            }
        }
        // Districts sharing a channel or a power line join one cluster (I13).
        let mut linked: BTreeSet<DistrictId> = BTreeSet::new();
        let my_channels: BTreeSet<ChannelId> = model
            .links
            .iter()
            .filter_map(|(k, _)| match k {
                LinkKey::Channel(c, _) => Some(*c),
                LinkKey::Mail { .. } => None,
            })
            .collect();
        for c in &my_channels {
            if let Some(set) = self.channel_sims.get(c) {
                linked.extend(set.iter().copied().filter(|&o| o != d));
            }
        }
        if let Some(src) = dist.power_source
            && self.cluster_of.contains_key(&src)
        {
            linked.insert(src);
        }
        if let Some(sinks) = self.sinks_of.get(&d) {
            linked.extend(
                sinks
                    .iter()
                    .copied()
                    .filter(|o| self.cluster_of.contains_key(o)),
            );
        }
        let mut handles: Vec<RegionId<Cluster>> = linked
            .iter()
            .filter_map(|o| self.cluster_of.get(o).copied())
            .collect();
        handles.sort_by_key(|h| self.clusters.get(*h).map(|r| r.key()));
        handles.dedup();
        let h = match handles.first().copied() {
            None => {
                let mut c = Cluster::new(self.tun.replay.restart_min_run_ticks);
                c.add_member(spec, at);
                let key = self.next_cluster;
                self.next_cluster += 1;
                self.clusters.insert(key, c, at)
            }
            Some(first) => {
                for &other in &handles[1..] {
                    self.clusters.apply(other, at, |_, _| ());
                    let (_, lazy) = self.clusters.remove(other).expect("live cluster");
                    let (model, anchor) = lazy.into_parts();
                    debug_assert_eq!(anchor, at);
                    for m in model.members() {
                        self.cluster_of.insert(m, first);
                    }
                    self.clusters.apply(first, at, |c, at| c.absorb(model, at));
                }
                self.clusters
                    .apply(first, at, |c, at| c.add_member(spec, at));
                first
            }
        };
        let boundary = self.next_season_boundary(at);
        self.clusters
            .apply(h, at, |c, _| c.set_season_boundary(boundary));
        self.cluster_of.insert(d, h);
        for c in &my_channels {
            self.channel_sims.entry(*c).or_default().insert(d);
        }
        // Sinks already replaying on this district's power now follow it (I13).
        self.refresh_power(d, at);
        let dist = self.districts.get_mut(&d).expect("exists");
        dist.sim = Some(SimSpan {
            started: at,
            base,
            season,
        });
        self.stats.sims_started += 1;
        self.note(d, at, "replay starts".to_string());
        self.after_cluster(h, at);
    }

    /// Takes a district out of its cluster at `at`; returns its pools' levels (stock and mail).
    fn stop_sim(
        &mut self,
        d: DistrictId,
        at: Tick,
    ) -> Option<(SimSpan, Vec<crate::cluster::Pool>)> {
        let h = *self.cluster_of.get(&d)?;
        if let Some(season) = self
            .districts
            .get(&d)
            .and_then(|x| x.sim.as_ref())
            .map(|s| s.season)
        {
            let channels: Vec<ChannelId> = self.districts[&d]
                .models
                .get(&season)
                .map(|m| {
                    m.links
                        .iter()
                        .filter_map(|(k, _)| match k {
                            LinkKey::Channel(c, _) => Some(*c),
                            LinkKey::Mail { .. } => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            for c in channels {
                if let Some(set) = self.channel_sims.get_mut(&c) {
                    set.remove(&d);
                }
            }
        }
        self.cluster_of.remove(&d);
        let out = self.clusters.apply(h, at, |c, at| c.remove_member(d, at))?;
        // Book what the replay made and used (the apply phases; read-only changes nothing).
        let mut minted = GoodVec::new();
        let mut burned = GoodVec::new();
        for p in &out.pools {
            minted.add(p.key.good(), p.books.minted);
            burned.add(p.key.good(), p.books.burned);
            // A channel pool leaving with its last user takes unpushed changes with it: push them
            // now, or the world's buffer never sees them (review finding 1).
            if let PoolKey::Channel(ch, g) = p.key {
                self.push_channel(ch, g, p.level - p.pushed, at);
            }
        }
        if self.mode == Mode::Apply {
            self.books.minted.merge(&minted);
            self.books.burned.merge(&burned);
        }
        self.after_cluster(h, at);
        if self
            .clusters
            .get(h)
            .is_some_and(|r| r.cell().model().is_empty())
        {
            self.clusters.remove(h);
        }
        // Sinks on this district's power follow what it is now (F21).
        self.refresh_power(d, at);
        let dist = self.districts.get_mut(&d)?;
        let span = dist.sim.take()?;
        dist.made_total.merge(&minted);
        dist.made_total.subtract(&burned);
        Some((span, out.pools))
    }

    /// Events and pushes after anything touched cluster `h` at `at`.
    fn after_cluster(&mut self, h: RegionId<Cluster>, at: Tick) {
        let Some((events, pushes, settles)) = self.clusters.apply(h, at, |c, _| {
            let s = c.settles;
            c.settles = 0;
            (c.take_events(), c.take_unpushed(), s)
        }) else {
            return;
        };
        self.stats.cluster_settles += settles;
        for (key, delta) in pushes {
            if let PoolKey::Channel(ch, g) = key {
                self.push_channel(ch, g, delta, at);
            }
        }
        let mut season_change = false;
        for e in events {
            match e {
                ClusterEvent::Stopped {
                    at,
                    owner,
                    id: 0,
                    why,
                } => {
                    if let Some(dist) = self.districts.get_mut(&owner) {
                        dist.last_run = Some((at, Some(why)));
                    }
                    if self.log_enabled && why != Stop::Disabled {
                        let words = stop_words(why, &self.goods);
                        self.note(owner, at, format!("breakpoint: {words}"));
                    }
                }
                ClusterEvent::Started { at, owner, id: 0 } => {
                    if let Some(dist) = self.districts.get_mut(&owner) {
                        dist.last_run = Some((at, None));
                    }
                    if self.log_enabled {
                        self.note(owner, at, "running".to_string());
                    }
                }
                ClusterEvent::PackagesReady {
                    at,
                    pool: PoolKey::Mail(d, inv, address, g),
                    count,
                    batch,
                } => self.write_packages(d, inv, address, g, count, batch, at),
                ClusterEvent::SeasonBoundary { .. } => season_change = true,
                _ => {}
            }
        }
        if season_change {
            self.season_boundary(h, at);
        }
    }

    /// Sets what each replaying sink of `src` requires (I13, F21): the source's box when it
    /// replays in the same cluster; nothing when it is live (assumed running); otherwise an
    /// unmet requirement, so the sink waits for power.
    fn refresh_power(&mut self, src: DistrictId, at: Tick) {
        let sinks: Vec<DistrictId> = self
            .sinks_of
            .get(&src)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        let live = self
            .districts
            .get(&src)
            .is_some_and(|s| s.presence != Presence::Unloaded);
        let src_h = self.cluster_of.get(&src).copied();
        for s in sinks {
            let Some(&h) = self.cluster_of.get(&s) else {
                continue;
            };
            let req = if src_h == Some(h) || !live {
                Some((src, 0))
            } else {
                None
            };
            self.clusters
                .apply(h, at, |c, at| c.set_requires(s, 0, req, at));
            self.after_cluster(h, at);
        }
    }

    /// A channel buffer change the replay made, sent to the world (I13). Read-only mode sends
    /// nothing, so the buffer the engine knows stays what the world has.
    fn push_channel(&mut self, ch: ChannelId, g: Good, delta: i64, at: Tick) {
        if delta == 0 || self.mode != Mode::Apply {
            return;
        }
        let mut c = Cmd::ChannelDelta(ChannelDelta {
            op: OpId(0),
            channel: ch,
            goods: GoodVec::from_pairs([(g, delta)]),
        });
        // The event's own tick, not the engine's: a breakpoint fires inside `advance_to` before
        // `now` moves, so `now` would tie the id to where hand-overs fell.
        c.set_op(self.ops.derive(self.world_seed, at, &c));
        self.out.push(c);
        self.channels.entry(ch).or_default().known.add(g, delta);
    }

    #[allow(clippy::too_many_arguments)]
    fn write_packages(
        &mut self,
        d: DistrictId,
        inv: u32,
        address: u32,
        g: Good,
        count: i64,
        batch: i64,
        at: Tick,
    ) {
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        let pos = dist
            .inventories
            .get(&inv)
            .map_or(BlockPos::default(), |i| i.decl.pos);
        // The packages are physically in the offline buffer now. They came out of the mail pool,
        // not out of stock, so the replay's base does not move: the stock pool already fell by
        // what was packed, and the reconcile takes that from the vaults.
        if let Some(i) = dist.inventories.get_mut(&inv) {
            i.known.add(g, count * batch);
            i.known_at = i.known_at.max(at);
        }
        self.stats.packages_written += count as u64;
        if self.mode == Mode::Apply {
            let mut c = Cmd::WritePackage(WritePackage {
                op: OpId(0),
                district: d,
                inventory: inv,
                pos,
                address,
                count: count as u32,
                goods: GoodVec::from_pairs([(g, batch)]),
            });
            c.set_op(self.ops.derive(self.world_seed, at, &c));
            self.out.push(c);
        }
    }

    /// At a season boundary, members whose season model is missing freeze; the others swap models
    /// (I12). Implemented as a reconcile-free restart: stop, then start from the replay's levels.
    fn season_boundary(&mut self, h: RegionId<Cluster>, at: Tick) {
        let members: Vec<DistrictId> = self
            .clusters
            .get(h)
            .map(|r| r.cell().model().members().into_iter().collect())
            .unwrap_or_default();
        for d in members {
            self.restart_sim(d, at);
            if !self.cluster_of.contains_key(&d) {
                self.note(d, at, "frozen: no model for the new season".to_string());
            }
        }
    }

    /// Stops a replay and starts it again from where it stood, with a member rebuilt from the
    /// district as it is now (a new season's model, a destroyed inventory's capacity gone). The
    /// change so far is carried as virtual stock (owed), so the reconcile at the next load still
    /// sees the whole change since the first unload.
    fn restart_sim(&mut self, d: DistrictId, at: Tick) {
        let Some((span, pools)) = self.stop_sim(d, at) else {
            return;
        };
        let dist = self.districts.get_mut(&d).expect("member");
        for p in &pools {
            dist.minted_carried.add(p.key.good(), p.books.minted);
            match p.key {
                PoolKey::Stock(_, g) => dist.owed.add(g, p.level - span.base.get(g)),
                PoolKey::Mail(_, _, _, g) => dist.owed.add(g, p.level - p.pushed),
                PoolKey::Channel(..) => {}
            }
        }
        // Deposits are read back from their pools, not carried as owed.
        let deposit_goods: Vec<Good> = dist.deposits.keys().copied().collect();
        for g in deposit_goods {
            let o = dist.owed.get(g);
            if o != 0 {
                dist.owed.add(g, -o);
                dist.deposit_owed.add(g, o);
            }
        }
        self.start_sim(d, at);
    }

    // ---- reconcile (I7, I8) ---------------------------------------------------------------

    fn reconcile_on_load(&mut self, d: DistrictId, at: Tick) {
        let stopped = self.stop_sim(d, at);
        let dist = self.districts.get_mut(&d).expect("exists");
        let mut net = GoodVec::new();
        let mut deposits = GoodVec::new();
        let mut span_minted = std::mem::take(&mut dist.minted_carried);
        let since = stopped.as_ref().map_or(at, |s| s.0.started);
        if let Some((span, pools)) = stopped {
            for p in &pools {
                span_minted.add(p.key.good(), p.books.minted);
                match p.key {
                    PoolKey::Stock(_, g) => {
                        let change = p.level - span.base.get(g);
                        if self.goods.get(&g).map(|i| i.class) == Some(GoodClass::Deposit) {
                            deposits.add(g, change);
                        } else {
                            net.add(g, change);
                        }
                    }
                    // Goods packed but not yet a whole package go back to stock.
                    PoolKey::Mail(_, _, _, g) => net.add(g, p.level - p.pushed),
                    PoolKey::Channel(..) => {}
                }
            }
        }
        net.merge(&dist.owed);
        deposits.merge(&std::mem::take(&mut dist.deposit_owed));
        let had_owed = !dist.owed.is_empty();
        if net.is_empty() && deposits.is_empty() && dist.destroy_pending.is_empty() && !had_owed {
            return;
        }
        self.stats.reconciles += 1;
        let read_only = self.mode == Mode::ReadOnly;
        let invs: Vec<reconcile::InvState> = dist
            .inventories
            .values()
            .filter(|i| !i.destroyed)
            .map(|i| i.state())
            .collect();
        let plan = reconcile::plan(&invs, &net, &self.goods);
        let mut report = ReconcileReport {
            epoch: dist.epoch + 1,
            at,
            since,
            net: net.clone(),
            unplaced: plan.unplaced.clone(),
            read_only,
            ..ReconcileReport::default()
        };
        if read_only {
            // I-A: the vaults stay untouched; the sim's stock is dropped with the span.
            report.epoch = dist.epoch;
            dist.owed = GoodVec::new();
            let what = format!("would have made: {}", self.goods_words(&report.net));
            let dist = self.districts.get_mut(&d).expect("exists");
            dist.push_report(report);
            self.note(d, at, what);
            return;
        }
        dist.epoch += 1;
        let epoch = dist.epoch;
        dist.owed = plan.unplaced.clone();
        let mut by_chunk: BTreeMap<ChunkPos, Vec<InvDelta>> = BTreeMap::new();
        let mut added = GoodVec::new();
        for p in &plan.deltas {
            let inv = dist
                .inventories
                .get_mut(&p.inventory)
                .expect("planned inventory");
            inv.pending.add(p.good, p.delta);
            if p.delta > 0 {
                added.add(p.good, p.delta);
            }
            by_chunk.entry(inv.chunk).or_default().push(InvDelta {
                inventory: p.inventory,
                pos: inv.decl.pos,
                good: p.good,
                delta: p.delta,
            });
        }
        // Deposits (I11): with the chunk, or at once.
        for (g, delta) in deposits.iter() {
            let Some(s) = dist.deposits.get_mut(&g) else {
                continue;
            };
            s.remaining += delta;
            match deposit::plugin(s.plugin).writeback() {
                Writeback::WithChunk => by_chunk.entry(s.chunk).or_default().push(InvDelta {
                    inventory: DEPOSIT_INVENTORY,
                    pos: BlockPos::new(s.chunk.x * 16, 0, s.chunk.z * 16),
                    good: g,
                    delta,
                }),
                Writeback::Immediate => {
                    let mut c = Cmd::DepositDelta(DepositDelta {
                        op: OpId(0),
                        plugin: s.plugin,
                        chunk: s.chunk,
                        good: g,
                        delta,
                    });
                    c.set_op(self.ops.derive(self.world_seed, self.now, &c));
                    self.out.push(c);
                }
            }
        }
        let mut destroys = std::mem::take(&mut dist.destroy_pending);
        let chunks: BTreeSet<ChunkPos> = by_chunk.keys().chain(destroys.keys()).copied().collect();
        for chunk in chunks {
            let mut deltas = by_chunk.remove(&chunk).unwrap_or_default();
            // Removals first (I8).
            deltas.sort_by_key(|x| (x.delta > 0, x.inventory, x.good));
            let destroy = destroys.remove(&chunk).unwrap_or_default();
            report.destroyed += destroy.len() as u32;
            report.deltas += deltas.len() as u32;
            report.chunks += 1;
            let mut cmd = ChunkReconcile {
                op: OpId(0),
                district: d,
                epoch,
                chunk,
                deltas,
                destroy,
            };
            cmd.op = self
                .ops
                .derive(self.world_seed, self.now, &Cmd::ChunkReconcile(cmd.clone()));
            dist.pending_ops.insert(cmd.op, chunk);
            dist.pending.entry(chunk).or_default().push(PendingChunk {
                cmd,
                trim: false,
                sent: false,
            });
        }
        // Trims are capped by what the replay made in the span, not by this round's additions:
        // outputs already shipped are taken back from whatever holds them, or booked untracked.
        let _ = added;
        dist.round_added.insert(epoch, span_minted);
        // Keep the books of the last few rounds only (trims refer to recent ones).
        while dist.round_added.len() > 8 {
            dist.round_added.pop_first();
        }
        let what = format!(
            "reconcile epoch {epoch}: {} in {} chunk(s), {} deltas{}{}",
            self.goods_words(&report.net),
            report.chunks,
            report.deltas,
            if report.unplaced.is_empty() {
                String::new()
            } else {
                format!(", unplaced {}", self.goods_words(&report.unplaced))
            },
            if report.destroyed > 0 {
                format!(", {} blocks to destroy", report.destroyed)
            } else {
                String::new()
            }
        );
        let dist = self.districts.get_mut(&d).expect("exists");
        dist.push_report(report);
        let loaded: Vec<ChunkPos> = dist.loaded_chunks.iter().copied().collect();
        self.note(d, at, what);
        for c in loaded {
            self.send_pending(d, c, 0);
        }
    }

    fn send_pending(&mut self, d: DistrictId, chunk: ChunkPos, applied_epoch: u32) {
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        if let Some(list) = dist.pending.get_mut(&chunk) {
            for p in list.iter_mut() {
                if p.cmd.epoch > applied_epoch {
                    p.sent = true;
                    self.stats.chunk_commands += 1;
                    self.out.push(Cmd::ChunkReconcile(p.cmd.clone()));
                }
            }
        }
    }

    fn reconcile_result(&mut self, at: Tick, r: &ReconcileResult) {
        let season = self.season_at(at);
        let Some(dist) = self.districts.get_mut(&r.district) else {
            return;
        };
        let Some(chunk) = dist.pending_ops.remove(&r.op) else {
            // Already applied: a replayed or repeated result changes nothing (A9).
            self.stats.duplicate_results += 1;
            return;
        };
        let list = dist.pending.get_mut(&chunk).expect("pending chunk");
        let pos = list
            .iter()
            .position(|p| p.cmd.op == r.op)
            .expect("pending op");
        let done = list.remove(pos);
        if list.is_empty() {
            dist.pending.remove(&chunk);
        }
        let mut shortfall = GoodVec::new();
        let mut unfit = GoodVec::new();
        for e in &r.entries {
            if e.inventory == DEPOSIT_INVENTORY {
                // The plugin took what was there; a machine may have drilled first.
                if let Some(s) = dist.deposits.get_mut(&e.good) {
                    s.remaining += e.applied - e.requested;
                }
                dist.ports.add(e.good, e.applied);
                if e.requested < 0 && e.applied > e.requested {
                    shortfall.add(e.good, e.applied - e.requested);
                }
                continue;
            }
            if let Some(inv) = dist.inventories.get_mut(&e.inventory) {
                inv.pending.add(e.good, -e.requested);
                if at > inv.known_at {
                    inv.known.add(e.good, e.applied);
                }
            }
            // Applied amounts cross the boundary: learning must never read them as production.
            dist.ports.add(e.good, e.applied);
            if e.requested < 0 && e.applied > e.requested {
                shortfall.add(e.good, e.applied - e.requested);
            }
            if e.requested > 0 && e.applied < e.requested {
                unfit.add(e.good, e.requested - e.applied);
            }
        }
        dist.owed.merge(&unfit);
        // Blocks broke: the doomed inventories at those positions are gone, and what they held
        // dropped. A block skipped (changed since) keeps its inventory (review finding 7).
        if !r.destroyed.is_empty() {
            for inv in dist.inventories.values_mut() {
                if inv.doomed && r.destroyed.contains(&inv.decl.pos) {
                    inv.doomed = false;
                    inv.destroyed = true;
                    let spill = inv.effective();
                    self.books.dropped.merge(&spill);
                    dist.ports.subtract(&spill);
                    inv.known = GoodVec::new();
                    inv.pending = GoodVec::new();
                }
            }
            // The district changed under the learner: that window is not a steady one.
            dist.learner.discard();
        }
        // Doomed blocks this command skipped are not doomed any more.
        for b in &done.cmd.destroy {
            if !r.destroyed.contains(&b.pos)
                && let Some(inv) = dist
                    .inventories
                    .values_mut()
                    .find(|i| i.doomed && i.decl.pos == b.pos)
            {
                inv.doomed = false;
            }
        }
        if done.trim {
            for e in &r.entries {
                if e.requested < 0 {
                    self.books.trimmed.add(e.good, -e.applied);
                }
            }
            // The trimmed outputs had already moved on: book it, visibly (docs §4.2).
            dist.untracked.merge(&shortfall);
            self.books.untracked.merge(&shortfall);
            if let Some(rep) = dist.reports.back_mut() {
                rep.untracked.merge(&shortfall);
            }
            return;
        }
        if shortfall.is_empty() {
            return;
        }
        self.books.shortfall.merge(&shortfall);
        let epoch = done.cmd.epoch;
        let rates = dist
            .models
            .get(&season)
            .or_else(|| dist.models.values().next())
            .map(|m| m.rates.clone())
            .unwrap_or_default();
        let added = dist.round_added.get(&epoch).cloned().unwrap_or_default();
        // Every result of an epoch adds to its shortfall; trims are for the largest need over
        // the inputs, so only the increase is trimmed now.
        let (short_e, trimmed_e) = dist.round_short.entry(epoch).or_default();
        short_e.merge(&shortfall);
        let want = reconcile::trims(short_e, &rates, &added);
        let mut trims = GoodVec::new();
        for (g, n) in want.iter() {
            let more = n - trimmed_e.get(g);
            if more > 0 {
                trims.add(g, more);
            }
        }
        trimmed_e.merge(&trims);
        while dist.round_short.len() > 8 {
            dist.round_short.pop_first();
        }
        if let Some(rep) = dist.reports.iter_mut().rev().find(|x| x.epoch == epoch) {
            rep.shortfalls.merge(&shortfall);
            rep.trims.merge(&trims);
        }
        if trims.is_empty() {
            return;
        }
        self.stats.trims += 1;
        let what = format!(
            "shortfall {} trims {}",
            self.goods_words(&shortfall),
            self.goods_words(&trims)
        );
        self.note(r.district, at, what);
        self.apply_trims(r.district, epoch, &trims, at);
    }

    /// Trims outputs (I8): first from additions of `epoch` not yet applied, then by a new epoch of
    /// removals from whatever holds them.
    fn apply_trims(&mut self, d: DistrictId, epoch: u32, trims: &GoodVec, at: Tick) {
        let dist = self.districts.get_mut(&d).expect("exists");
        let mut left = trims.clone();
        for list in dist.pending.values_mut() {
            for p in list
                .iter_mut()
                .filter(|p| p.cmd.epoch == epoch && !p.trim && !p.sent)
            {
                for delta in p.cmd.deltas.iter_mut().filter(|x| x.delta > 0) {
                    let want = left.get(delta.good);
                    if want <= 0 {
                        continue;
                    }
                    let cut = want.min(delta.delta);
                    delta.delta -= cut;
                    left.add(delta.good, -cut);
                    self.books.trimmed.add(delta.good, cut);
                    if let Some(inv) = dist.inventories.get_mut(&delta.inventory) {
                        inv.pending.add(delta.good, -cut);
                    }
                }
                p.cmd.deltas.retain(|x| x.delta != 0);
            }
        }
        if left.is_empty() {
            return;
        }
        let invs: Vec<reconcile::InvState> = dist
            .inventories
            .values()
            .filter(|i| !i.destroyed)
            .map(|i| i.state())
            .collect();
        let neg: GoodVec = left.iter().map(|(g, n)| (g, -n)).collect();
        let plan = reconcile::plan(&invs, &neg, &self.goods);
        // What nobody holds any more is an untracked mint, booked at once.
        let gone: GoodVec = plan.unplaced.iter().map(|(g, n)| (g, -n)).collect();
        dist.untracked.merge(&gone);
        self.books.untracked.merge(&gone);
        if plan.deltas.is_empty() {
            return;
        }
        dist.epoch += 1;
        let new_epoch = dist.epoch;
        let mut by_chunk: BTreeMap<ChunkPos, Vec<InvDelta>> = BTreeMap::new();
        for p in &plan.deltas {
            let inv = dist.inventories.get_mut(&p.inventory).expect("planned");
            inv.pending.add(p.good, p.delta);
            by_chunk.entry(inv.chunk).or_default().push(InvDelta {
                inventory: p.inventory,
                pos: inv.decl.pos,
                good: p.good,
                delta: p.delta,
            });
        }
        let _ = at;
        for (chunk, deltas) in by_chunk {
            let mut cmd = ChunkReconcile {
                op: OpId(0),
                district: d,
                epoch: new_epoch,
                chunk,
                deltas,
                destroy: Vec::new(),
            };
            cmd.op = self
                .ops
                .derive(self.world_seed, self.now, &Cmd::ChunkReconcile(cmd.clone()));
            dist.pending_ops.insert(cmd.op, chunk);
            dist.pending.entry(chunk).or_default().push(PendingChunk {
                cmd,
                trim: true,
                sent: false,
            });
        }
        let loaded: Vec<ChunkPos> = dist.loaded_chunks.iter().copied().collect();
        for c in loaded {
            self.send_pending_from(d, c, new_epoch);
        }
    }

    fn send_pending_from(&mut self, d: DistrictId, chunk: ChunkPos, epoch: u32) {
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        if let Some(list) = dist.pending.get_mut(&chunk) {
            for p in list.iter_mut().filter(|p| p.cmd.epoch == epoch) {
                p.sent = true;
                self.stats.chunk_commands += 1;
                self.out.push(Cmd::ChunkReconcile(p.cmd.clone()));
            }
        }
    }

    // ---- ports, trains, packages ----------------------------------------------------------

    fn port(&mut self, at: Tick, p: &crate::obs::PortTransfer) {
        let dist = self.district_mut(p.district);
        dist.ports.merge(&p.goods);
        if p.kind == PortKind::Channel {
            let ch = ChannelId(p.counterpart);
            for (g, n) in p.goods.iter() {
                *dist.links.entry(LinkKey::Channel(ch, g)).or_default() += n;
            }
            // A live member's real transfer changes the shared buffer the replay holds.
            let holders: Vec<RegionId<Cluster>> = self
                .channel_sims
                .get(&ch)
                .into_iter()
                .flatten()
                .filter_map(|o| self.cluster_of.get(o).copied())
                .collect();
            for h in holders {
                let has = self.clusters.get(h).is_some_and(|r| {
                    p.goods
                        .goods()
                        .any(|g| r.cell().model().pool(PoolKey::Channel(ch, g)).is_some())
                });
                if has {
                    let goods = p.goods.clone();
                    self.clusters.apply(h, at, |c, at| {
                        for (g, n) in goods.iter() {
                            c.add_to_pool(PoolKey::Channel(ch, g), -n, true, at);
                        }
                    });
                    self.after_cluster(h, at);
                    break;
                }
            }
            for (g, n) in p.goods.iter() {
                self.channels.entry(ch).or_default().known.add(g, -n);
            }
        }
    }

    fn train_arrived(&mut self, at: Tick, c: &TrainCall) {
        // Line edges (§8.8).
        if let Some((from, depart, cargo)) = self.last_departure.remove(&c.train) {
            let e = self.lines.entry((from, c.station)).or_default();
            if e.trips == 0 {
                e.first = at;
            }
            e.trips += 1;
            e.cargo.merge(&cargo);
            e.travel_ticks += at.since(depart);
            e.last = at;
        }
        if !c.simulated {
            self.arrivals
                .insert((c.train, c.station), c.carriages.clone());
            return;
        }
        let Some(dist) = self.districts.get(&c.district) else {
            return;
        };
        let Some(rule) = dist.exchange.get(&c.station).cloned() else {
            return;
        };
        let Some(&h) = self.cluster_of.get(&c.district) else {
            // Frozen: interfaces do nothing while unloaded, as in vanilla.
            return;
        };
        let d = c.district;
        let carriages = c.carriages.clone();
        // A Portable Storage Interface unloads only into room: the receivers' room at the last
        // sample, less what the replay has already added (F19).
        let mut room: BTreeMap<Good, (i64, i64)> = BTreeMap::new();
        for &g in &rule.unload {
            let mut r = 0i64;
            let mut any = false;
            for inv in dist
                .inventories
                .values()
                .filter(|i| !i.destroyed && i.received.contains(&g))
            {
                any = true;
                r += reconcile::room_for(&inv.state(), &inv.effective(), g, &self.goods);
            }
            if !any {
                for inv in dist.inventories.values().filter(|i| !i.destroyed) {
                    r += reconcile::room_for(&inv.state(), &inv.effective(), g, &self.goods);
                }
            }
            let base = dist.sim.as_ref().map_or(0, |s| s.base.get(g));
            room.insert(g, (r, base));
        }
        let goods = &self.goods;
        let entries = self.clusters.apply(h, at, |cl, at| {
            let mut entries = Vec::new();
            for car in &carriages {
                let mut cargo = car.goods.clone();
                for &g in &rule.unload {
                    let (r, base) = room[&g];
                    let level = cl.level(PoolKey::Stock(d, g)).unwrap_or(0);
                    let free = (r - (level - base).max(0)).max(0);
                    let n = cargo.get(g).min(free);
                    if n > 0 {
                        let got = cl.add_to_pool(PoolKey::Stock(d, g), n, false, at);
                        cargo.add(g, -got);
                        entries.push((car.index, g, -got));
                    }
                }
                for &g in &rule.load {
                    let have = cl.level(PoolKey::Stock(d, g)).unwrap_or(0);
                    let room = carriage_room(car, &cargo, g, goods);
                    let n = have.min(room);
                    if n > 0 {
                        let got = -cl.add_to_pool(PoolKey::Stock(d, g), -n, false, at);
                        cargo.add(g, got);
                        entries.push((car.index, g, got));
                    }
                }
            }
            entries
        });
        let Some(entries) = entries else {
            return;
        };
        self.after_cluster(h, at);
        if entries.is_empty() {
            return;
        }
        self.stats.exchanges += 1;
        if self.mode == Mode::ReadOnly {
            // I-A: the exchange is the ledger's estimate only; the train keeps its cargo.
            return;
        }
        if self.log_enabled {
            let mut moved = GoodVec::new();
            for &(_, g, n) in &entries {
                moved.add(g, -n);
            }
            let what = format!(
                "station exchange with train {}: {}",
                c.train.0,
                self.goods_words(&moved)
            );
            self.note(d, at, what);
        }
        let mut cmd = Cmd::CarriageDelta(CarriageDelta {
            op: OpId(0),
            train: c.train,
            reason: CarriageReason::Exchange,
            entries: entries.clone(),
        });
        let op = self.ops.derive(self.world_seed, self.now, &cmd);
        cmd.set_op(op);
        self.exchanges.insert(
            op,
            PendingExchange {
                district: d,
                train: c.train,
                entries,
                loot: false,
            },
        );
        self.out.push(cmd);
    }

    /// Java answered an industry command failed (COMMAND_RESULT failed, docs/industry-boundary.md
    /// §4.3): a station exchange or a train's loot moved nothing, so it is taken back as a result
    /// with nothing applied. Returns whether the engine was waiting on the op. Channel, deposit and
    /// package commands have no correction yet.
    pub fn command_failed(&mut self, at: Tick, op: rtsim_kernel::journal::OpId) -> bool {
        let Some(ex) = self.exchanges.get(&op) else {
            return false;
        };
        let r = CarriageResult {
            op,
            train: ex.train,
            entries: ex
                .entries
                .iter()
                .map(
                    |&(carriage, good, requested)| crate::obs::CarriageResultEntry {
                        carriage,
                        good,
                        requested,
                        applied: 0,
                    },
                )
                .collect(),
        };
        self.ingest(&Timed {
            at,
            obs: Obs::CarriageResult(r),
        });
        true
    }

    fn carriage_result(&mut self, at: Tick, r: &CarriageResult) {
        let Some(ex) = self.exchanges.remove(&r.op) else {
            return;
        };
        if ex.loot {
            // Loot the carriage no longer had was never stolen (review finding 8).
            for e in &r.entries {
                let unstolen = e.applied - e.requested;
                if e.requested < 0 && unstolen > 0 {
                    self.books.stolen.add(e.good, -unstolen);
                }
            }
            return;
        }
        // What the district gave or got differs from what was asked: correct its stock.
        let mut fix = GoodVec::new();
        for e in &r.entries {
            // Into the carriage: the district's change is the negative.
            fix.add(e.good, e.requested - e.applied);
        }
        if fix.is_empty() {
            return;
        }
        let d = ex.district;
        if let Some(&h) = self.cluster_of.get(&d) {
            let f = fix.clone();
            // A give-back the replay's stock cannot cover is owed to the next reconcile.
            let short = self.clusters.apply(h, at, |c, at| {
                let mut short = GoodVec::new();
                for (g, n) in f.iter() {
                    let got = c.add_to_pool(PoolKey::Stock(d, g), n, false, at);
                    short.add(g, n - got);
                }
                short
            });
            self.after_cluster(h, at);
            if let Some(short) = short
                && let Some(dist) = self.districts.get_mut(&d)
            {
                dist.owed.merge(&short);
            }
        } else if let Some(dist) = self.districts.get_mut(&d) {
            dist.owed.merge(&fix);
        }
    }

    fn train_departed(&mut self, at: Tick, dep: &TrainDeparture) {
        let cargo: GoodVec = {
            let mut v = GoodVec::new();
            for c in &dep.carriages {
                v.merge(&c.goods);
            }
            v
        };
        // Learn the station's exchange from a loaded call (I15).
        if let Some(arr) = self.arrivals.remove(&(dep.train, dep.station)) {
            let mut before = GoodVec::new();
            for c in &arr {
                before.merge(&c.goods);
            }
            let diff = before.minus(&cargo);
            if !diff.is_empty() {
                let rule = self
                    .district_mut(dep.district)
                    .exchange
                    .entry(dep.station)
                    .or_default();
                for (g, n) in diff.iter() {
                    if n > 0 {
                        rule.unload.insert(g);
                        rule.load.remove(&g);
                    } else {
                        rule.load.insert(g);
                        rule.unload.remove(&g);
                    }
                }
                rule.observed += 1;
            }
        }
        self.last_departure
            .insert(dep.train, (dep.station, at, cargo));
        // Book the leg (A15).
        let from = self.stations.get(&dep.station).map(|s| s.decl.clone());
        let to = dep
            .destination
            .and_then(|s| self.stations.get(&s))
            .map(|s| s.decl.clone());
        if let Some(from) = from {
            let to_pos = to.as_ref().map_or(from.pos, |t| t.pos);
            self.legs.book(
                Leg {
                    train: dep.train,
                    from: dep.station,
                    to: dep.destination,
                    depart: at,
                    eta: at + dep.eta_ticks.max(1),
                    cargo: dep
                        .carriages
                        .iter()
                        .map(|c| (c.index, c.goods.clone()))
                        .collect(),
                },
                from.dim,
                (from.pos.x, from.pos.z),
                (to_pos.x, to_pos.z),
            );
        }
        // Legs are kept for a day after they end (TUNABLE, PLACEHOLDER), swept once a day.
        if at.day() > self.legs_swept {
            self.legs_swept = at.day();
            self.legs.expire(Tick(at.0.saturating_sub(TICKS_PER_DAY)));
        }
    }

    fn package(&mut self, at: Tick, p: &PackageEvent) {
        let d = p.district;
        let simulated = self.cluster_of.contains_key(&d);
        let sign = if p.direction == Direction::In { 1 } else { -1 };
        let dist = self.district_mut(d);
        if !simulated {
            // Loaded: an ordinary port. Packages leaving are the packager's learned rate.
            let goods: GoodVec = p.goods.iter().map(|(g, n)| (g, sign * n)).collect();
            dist.ports.merge(&goods);
            if p.direction == Direction::Out {
                for (g, n) in p.goods.iter() {
                    *dist
                        .links
                        .entry(LinkKey::Mail {
                            inventory: p.inventory,
                            address: p.address,
                            good: g,
                        })
                        .or_default() -= n;
                }
                if p.packages > 0 {
                    let per: GoodVec = p
                        .goods
                        .iter()
                        .map(|(g, n)| (g, n / i64::from(p.packages)))
                        .collect();
                    dist.batches.insert((p.inventory, p.address), per);
                }
            }
            return;
        }
        // Unloaded: the offline buffer changed for real. Stock and base move together (§8.2).
        if let Some(inv) = dist.inventories.get_mut(&p.inventory) {
            for (g, n) in p.goods.iter() {
                inv.known.add(g, (sign * n).max(-inv.known.get(g)));
            }
        }
        let h = self.cluster_of[&d];
        let goods = p.goods.clone();
        let out = p.direction == Direction::Out;
        let inv = p.inventory;
        let address = p.address;
        let moved = self.clusters.apply(h, at, |c, at| {
            let mut moved = GoodVec::new();
            for (g, n) in goods.iter() {
                if out {
                    // Engine-written packages first, then stock that was already there.
                    let mail = PoolKey::Mail(d, inv, address, g);
                    let from_mail = c.pool(mail).map_or(0, |m| m.pushed.min(m.level)).min(n);
                    if from_mail > 0 {
                        c.add_to_pool(mail, -from_mail, true, at);
                    }
                    let rest = n - from_mail;
                    if rest > 0 {
                        let got = -c.add_to_pool(PoolKey::Stock(d, g), -rest, false, at);
                        moved.add(g, -got);
                    }
                } else {
                    c.add_to_pool(PoolKey::Stock(d, g), n, false, at);
                    moved.add(g, n);
                }
            }
            moved
        });
        if let Some(moved) = moved
            && let Some(span) = self.districts.get_mut(&d).and_then(|x| x.sim.as_mut())
        {
            span.base.merge(&moved);
        }
        self.after_cluster(h, at);
    }

    // ---- threats --------------------------------------------------------------------------

    fn schedule_pressure(&mut self, d: DistrictId, at: Tick) {
        let tun = self.tun.threat.clone();
        let seed = self.world_seed;
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        dist.pressure_stamp += 1;
        // Natural mobs need a player near to spawn (vanilla); unattended, the sim adds them (I20).
        if dist.watched {
            return;
        }
        let Some(survey) = dist.defence.survey else {
            return;
        };
        let stamp = dist.pressure_stamp;
        for c in ThreatClass::ALL {
            let mean = dist
                .defence
                .pressure
                .per_day(c, survey.dark_spawnable, &tun);
            let from = at.max(dist.defence.cooldown_until[c.index()]);
            let mut day = from.day();
            if from.tick_of_day() > NIGHT_ATTACK_TICK {
                day += 1;
            }
            if let Some((night, count)) = next_pressure_attack(seed, d, c, mean, day) {
                let when = Tick(night * TICKS_PER_DAY + NIGHT_ATTACK_TICK);
                self.pressure_seq += 1;
                self.pressure
                    .insert((when, self.pressure_seq), (d, c, count, stamp));
            }
        }
    }

    fn pressure_wake(
        &mut self,
        at: Tick,
        (d, c, count, stamp): (DistrictId, ThreatClass, u32, u32),
    ) {
        let live = self
            .districts
            .get(&d)
            .is_some_and(|x| x.pressure_stamp == stamp && !x.watched);
        if !live {
            return;
        }
        let t = Threat {
            id: (d.0 << 8) ^ at.0,
            source: ThreatSource::Natural,
            class: c,
            count,
            strength: f64::from(count) * NATURAL_STRENGTH,
            target: ThreatTarget::District(d),
            loads: None,
            at,
        };
        self.attack_district(&t);
        // The next one, for this class only (the stamp is shared, so reschedule all classes).
        self.schedule_pressure(d, at + 1);
    }

    /// The trains a threat leg meets (A15, INDUSTRY.md §13.3): legs booked in the regions it
    /// crosses during `[from, to]`, one lookup per region, never a scan. The caller (a dragon or
    /// raid module) turns each into a [`Threat`] on [`ThreatTarget::Train`].
    pub fn encounters(
        &self,
        dim: u32,
        regions: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainId> {
        let mut out: Vec<TrainId> = Vec::new();
        for &r in regions {
            for id in self.legs.in_region(dim, r, from, to) {
                if let Some(l) = self.legs.get(id)
                    && !out.contains(&l.train)
                {
                    out.push(l.train);
                }
            }
        }
        out
    }

    /// An attack from another system (I19). Advances to its time first.
    pub fn threat(&mut self, t: &Threat) {
        let at = t.at.max(self.now);
        self.advance_to(at);
        let t = Threat { at, ..*t };
        match t.target {
            ThreatTarget::District(_) => self.attack_district(&t),
            ThreatTarget::Train(train) => self.attack_train(&t, train),
        }
    }

    fn attack_district(&mut self, t: &Threat) {
        let ThreatTarget::District(d) = t.target else {
            return;
        };
        let at = t.at;
        let tun = self.tun.threat.clone();
        let Some(dist) = self.districts.get(&d) else {
            return;
        };
        let mut report = AttackReport {
            at,
            source: t.source,
            class: t.class,
            count: t.count,
            outcome: None,
            survivors: 0,
            stolen: GoodVec::new(),
            destroyed: 0,
            machine_lost: false,
            skipped: None,
        };
        // Watched fights are real (§13.1). Unwatched ones resolve abstractly, whether or not the
        // chunks are loaded (C18).
        if dist.watched {
            report.skipped = Some("watched");
        } else if at < dist.defence.cooldown_until[t.class.index()] {
            report.skipped = Some("cooldown");
        }
        if report.skipped.is_some() || self.mode == Mode::ReadOnly {
            self.stats.attacks_skipped += 1;
            let what = format!(
                "{:?} {:?} x{} skipped: {}",
                t.source,
                t.class,
                t.count,
                report.skipped.unwrap_or("read-only phase")
            );
            self.note(d, at, what);
            if self.mode == Mode::ReadOnly && report.skipped.is_none() {
                report.skipped = Some("read-only phase");
            }
            self.district_mut(d).push_attack(report);
            return;
        }
        self.stats.attacks += 1;
        let h = self.cluster_of.get(&d).copied();
        let stock = match h {
            Some(h) => {
                let view = self.clusters.read(h, at).expect("live cluster");
                stock_of(&view, d)
            }
            None => dist.effective_stock(),
        };
        let ammo = dist
            .defence
            .survey
            .and_then(|s| s.ammo_good)
            .map_or(0, |g| stock.get(g));
        let outcome = threat::resolve(t, &dist.defence, ammo, &tun);
        // Raiders carry items; fluids, energy and deposits stay (review finding 5).
        let items: GoodVec = stock
            .iter()
            .filter(|(g, _)| self.goods.get(g).is_none_or(|i| i.class == GoodClass::Item))
            .collect();
        let dmg = threat::damage(t, &outcome, &items, &dist.defence, &tun);
        report.outcome = Some(outcome);
        report.survivors = dmg.survivors;
        report.stolen = dmg.stolen.clone();
        report.destroyed = dmg.destroyed.len() as u32;
        report.machine_lost = dmg.machine_lost;
        // Ammunition the turrets fired is used up, never more than is left after the theft.
        let mut fired = GoodVec::new();
        if let Some(g) = dist.defence.survey.and_then(|s| s.ammo_good) {
            fired.add(
                g,
                outcome
                    .ammo_used
                    .min(stock.get(g) - dmg.stolen.get(g))
                    .max(0),
            );
        }
        let mut take = dmg.stolen.clone();
        take.merge(&fired);
        self.books.burned.merge(&fired);
        let dim = dist.dim;
        let dist = self.districts.get_mut(&d).expect("exists");
        let mut doomed_any = false;
        for b in &dmg.destroyed {
            let pos = BlockPos::new(b.x, b.y, b.z);
            dist.destroy_pending
                .entry(pos.chunk(dim))
                .or_default()
                .push(BlockDestroy {
                    pos,
                    expected_state: b.state,
                });
            if let Some(i) = b.inventory
                && let Some(inv) = dist.inventories.get_mut(&i)
                && !inv.destroyed
                && !inv.doomed
            {
                inv.doomed = true;
                doomed_any = true;
            }
        }
        self.books.stolen.merge(&dmg.stolen);
        match h {
            Some(h) => {
                self.clusters.apply(h, at, |c, at| {
                    for (g, n) in take.iter() {
                        c.add_to_pool(PoolKey::Stock(d, g), -n, false, at);
                    }
                });
                if dmg.machine_lost {
                    self.clusters
                        .apply(h, at, |c, at| c.set_enabled(d, false, at));
                }
                self.after_cluster(h, at);
                // A doomed inventory takes its room with it: rebuild the member (F20).
                if doomed_any {
                    if dmg.machine_lost {
                        self.districts.get_mut(&d).expect("exists").phase = Phase::Damaged;
                    }
                    self.restart_sim(d, at);
                }
            }
            None => {
                let dist = self.districts.get_mut(&d).expect("exists");
                for (g, n) in take.iter() {
                    dist.owed.add(g, -n);
                }
                // Chunk-loaded with nobody near: applied at once, as a reconcile round now
                // (§13.1); unloaded: at the next load.
                if dist.presence != Presence::Unloaded {
                    self.reconcile_on_load(d, at);
                }
            }
        }
        let what = format!(
            "{:?} {:?} x{}: survived {:.2}{}{}, stole {}, destroyed {}{}",
            t.source,
            t.class,
            t.count,
            outcome.survived,
            if outcome.untested {
                " (untested defences)"
            } else {
                ""
            },
            outcome
                .floor
                .map_or(String::new(), |f| format!(" (floor {f:.2})")),
            self.goods_words(&dmg.stolen),
            dmg.destroyed.len(),
            if dmg.machine_lost {
                ", machine lost: stopped"
            } else {
                ""
            }
        );
        self.note(d, at, what);
        let dist = self.districts.get_mut(&d).expect("exists");
        if dmg.machine_lost {
            dist.phase = Phase::Damaged;
        }
        dist.defence.cooldown_until[t.class.index()] = at + tun.cooldown_ticks[t.class.index()];
        dist.push_attack(report);
    }

    fn attack_train(&mut self, t: &Threat, train: TrainId) {
        // Trains get the same per-class pacing as districts (I25).
        let cd = self
            .train_cooldown
            .entry(train)
            .or_insert([Tick::ZERO; ThreatClass::COUNT]);
        if t.at < cd[t.class.index()] {
            self.stats.attacks_skipped += 1;
            return;
        }
        let Some(id) = self.legs.leg_of(train, t.at) else {
            return;
        };
        let leg = self.legs.get(id).expect("found").clone();
        let mut cargo = GoodVec::new();
        for c in &leg.cargo {
            cargo.merge(&c.1);
        }
        let loot = threat::loot(t, &cargo, &self.tun.threat);
        if loot.is_empty() || self.mode == Mode::ReadOnly {
            return;
        }
        self.stats.attacks += 1;
        // Taken carriage by carriage, in order.
        let mut entries = Vec::new();
        let mut left = loot.clone();
        for (idx, goods) in &leg.cargo {
            for (g, n) in goods.iter() {
                let take = n.min(left.get(g));
                if take > 0 {
                    entries.push((*idx, g, -take));
                    left.add(g, -take);
                    self.legs.take_cargo(id, *idx, g, take);
                }
            }
        }
        self.books.stolen.merge(&loot);
        self.train_cooldown
            .entry(train)
            .or_insert([Tick::ZERO; ThreatClass::COUNT])[t.class.index()] =
            t.at + self.tun.threat.cooldown_ticks[t.class.index()];
        let what = format!(
            "train {} ambushed ({:?} x{}): looted {}",
            train.0,
            t.class,
            t.count,
            self.goods_words(&loot)
        );
        self.note(DistrictId(0), t.at, what);
        let mut cmd = Cmd::CarriageDelta(CarriageDelta {
            op: OpId(0),
            train,
            reason: CarriageReason::Loot,
            entries: entries.clone(),
        });
        let op = self.ops.derive(self.world_seed, self.now, &cmd);
        cmd.set_op(op);
        self.exchanges.insert(
            op,
            PendingExchange {
                district: DistrictId(0),
                train,
                entries,
                loot: true,
            },
        );
        self.out.push(cmd);
    }

    // ---- the ledger (I-A, I18) ------------------------------------------------------------

    /// What a district would have made while unloaded, and where it stands. Read-only: it
    /// projects the replay without committing anything (the kernel's read rule).
    pub fn ledger(&mut self, d: DistrictId, at: Tick) -> Option<LedgerReport> {
        // A read reports as of the engine's time: it never fires what is due (review finding 12).
        let at = at.min(self.now);
        let season = self.season_at(at);
        let dist = self.districts.get(&d)?;
        let view = self
            .cluster_of
            .get(&d)
            .and_then(|&h| self.clusters.read(h, at));
        let model = dist
            .models
            .get(&season)
            .or_else(|| dist.models.values().next_back());
        let rates_per_hour = model
            .map(|m| {
                m.learned
                    .rates
                    .iter()
                    .map(|&(g, r)| (g, r * TICKS_PER_HOUR as f64))
                    .collect()
            })
            .unwrap_or_default();
        let (state, reason) = match (dist.presence, dist.phase) {
            (Presence::Live, _) => (LedgerState::Live, None),
            (Presence::Partial, _) => (LedgerState::Suspended, None),
            (_, Phase::Damaged) => (LedgerState::Damaged, dist.freeze_reason(season)),
            (_, Phase::Learning) => (LedgerState::Learning, Some("learning")),
            (_, Phase::Stale) => (LedgerState::Stale, dist.freeze_reason(season)),
            (_, Phase::NotLearnable(_)) => (LedgerState::NotLearnable, dist.freeze_reason(season)),
            (_, Phase::Learned) if view.is_none() => {
                (LedgerState::OutOfSeason, dist.freeze_reason(season))
            }
            (_, Phase::Learned) => (LedgerState::Running, None),
        };
        let mut made_since = GoodVec::new();
        let mut running = None;
        let mut stop = None;
        if let (Some(v), Some(span)) = (&view, &dist.sim) {
            let now = stock_of(v, d);
            made_since = now.minus(&span.base);
            let run = v.run_of(d, 0);
            running = run.map(|r| r == Run::Running);
            if let Some(Run::Stopped(why)) = run {
                stop = Some(stop_words(why, &self.goods));
            }
        }
        let forecast = self
            .cluster_of
            .get(&d)
            .and_then(|&h| self.clusters.get(h))
            .and_then(|r| r.cell().model().forecast(r.cell().anchor(), d));
        let next_breakpoint = forecast.map(|f| f.0);
        Some(LedgerReport {
            district: d,
            state,
            reason: reason.map(str::to_string),
            rates_per_hour,
            since: dist.sim.as_ref().map(|s| s.started),
            made_since,
            running,
            stop,
            next_breakpoint,
            next: forecast.map(|f| f.1),
            learning_ticks_to_go: if dist.phase == Phase::Learning {
                dist.learner.ticks_to_go(at, &self.tun.learn)
            } else {
                None
            },
            last_reconcile: dist.reports.back().cloned(),
            attacks: dist.attacks.iter().cloned().collect(),
        })
    }

    fn ledger_line(&mut self, d: DistrictId, at: Tick) -> LedgerLine {
        let Some(r) = self.ledger(d, at) else {
            return LedgerLine {
                district: d,
                state: LedgerState::Learning as u32,
                freeze: 0,
                next_breakpoint: None,
                next_kind: 0,
                next_good: None,
                since: at,
                goods: Vec::new(),
            };
        };
        let freeze = self
            .districts
            .get(&d)
            .and_then(|x| x.not_learnable)
            .map_or(0, |n| n.code());
        let goods = r
            .rates_per_hour
            .iter()
            .map(|&(g, ph)| (g, ph.round() as i64, r.made_since.get(g)))
            .collect();
        let (mut next_kind, next_good) = r.next.map_or((0, None), |f| f.code());
        if next_kind == 1
            && next_good.is_some_and(|g| {
                self.goods
                    .get(&g)
                    .is_some_and(|i| i.class == GoodClass::Deposit)
            })
        {
            next_kind = 4;
        }
        LedgerLine {
            district: d,
            state: r.state as u32,
            freeze,
            next_breakpoint: r.next_breakpoint,
            next_kind,
            next_good,
            since: r.since.unwrap_or(at),
            goods,
        }
    }

    /// Every live cluster's books close (tests and the CLI).
    pub fn check_books(&self) -> Result<(), String> {
        for (_, r) in self.clusters.iter() {
            r.cell().model().check_books()?;
        }
        Ok(())
    }

    /// The replay's current stock of a simulated district, without committing (a read).
    pub fn sim_stock(&mut self, d: DistrictId) -> Option<GoodVec> {
        let h = *self.cluster_of.get(&d)?;
        let now = self.now;
        let v = self.clusters.read(h, now)?;
        Some(stock_of(&v, d))
    }

    /// A channel buffer's level of a good: the replay's (a read, committing nothing) while a
    /// simulated district links it, else the last sample plus pushed changes.
    pub fn channel_level(&mut self, ch: ChannelId, g: Good) -> i64 {
        let now = self.now;
        let holder = self
            .channel_sims
            .get(&ch)
            .into_iter()
            .flatten()
            .find_map(|d| self.cluster_of.get(d).copied());
        if let Some(h) = holder
            && let Some(v) = self.clusters.read(h, now)
        {
            return v.level(PoolKey::Channel(ch, g));
        }
        self.channels.get(&ch).map_or(0, |c| c.known.get(g))
    }

    /// The district's replay run state, if simulated.
    pub fn sim_run(&self, d: DistrictId) -> Option<Run> {
        let h = *self.cluster_of.get(&d)?;
        self.clusters.get(h)?.cell().model().run_of(d, 0)
    }
}

fn stock_of(v: &ClusterView, d: DistrictId) -> GoodVec {
    v.levels
        .iter()
        .filter_map(|&(k, l)| match k {
            PoolKey::Stock(o, g) if o == d => Some((g, l)),
            _ => None,
        })
        .collect()
}

fn space_free(dist: &District, goods: &impl GoodLookup) -> bool {
    dist.inventories.values().any(|i| {
        i.decl.kind.has(crate::obs::InvKind::ITEMS) && {
            let used: i64 = i
                .known
                .iter()
                .map(|(g, c)| (c + goods.max_stack(g) - 1) / goods.max_stack(g))
                .sum();
            used < i64::from(i.decl.item_slots)
        }
    })
}

fn carriage_room(car: &Carriage, cargo: &GoodVec, g: Good, goods: &impl GoodLookup) -> i64 {
    match goods.class(g) {
        GoodClass::Item => {
            let used: i64 = cargo
                .iter()
                .filter(|(h, _)| goods.class(*h) == GoodClass::Item)
                .map(|(h, c)| (c + goods.max_stack(h) - 1) / goods.max_stack(h))
                .sum();
            let ms = goods.max_stack(g);
            let held = cargo.get(g);
            let partial = if held > 0 { (ms - held % ms) % ms } else { 0 };
            (i64::from(car.item_slots) - used).max(0) * ms + partial
        }
        GoodClass::Fluid => {
            let held: i64 = cargo
                .iter()
                .filter(|(h, _)| goods.class(*h) == GoodClass::Fluid)
                .map(|(_, c)| c)
                .sum();
            (car.fluid_capacity - held).max(0)
        }
        _ => 0,
    }
}

fn stop_words(why: Stop, goods: &BTreeMap<Good, GoodInfo>) -> String {
    let name = |k: PoolKey| {
        goods
            .get(&k.good())
            .map_or_else(|| format!("good {}", k.good().0), |i| i.name.clone())
    };
    match why {
        Stop::Disabled => "stopped: frozen".to_string(),
        Stop::InputEmpty(k) => {
            if goods.get(&k.good()).map(|i| i.class) == Some(GoodClass::Deposit) {
                format!("stopped: deposit exhausted ({})", name(k))
            } else {
                format!("stopped: no {}", name(k))
            }
        }
        Stop::OutputFull(k) => format!("stopped: {} full", name(k)),
        Stop::SpaceFull(_) => "stopped: output space full".to_string(),
        Stop::Target(k) => format!("stopped: target reached ({})", name(k)),
        Stop::SourceStopped(src) => format!("stopped: power source {} stopped", src.0),
        Stop::Waiting => "waiting to restart".to_string(),
    }
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};
    use rtsim_kernel::lazy::Lazy;
    use rtsim_kernel::page::PageKey;

    use super::*;
    use crate::persist::{
        CLUSTER_TABLE, DISTRICT_TABLE, GLOBAL_TABLE, Persist, from_bytes, persist_struct,
        persist_unit_enum, to_bytes,
    };

    persist_unit_enum!(Mode, [Mode::ReadOnly, Mode::Apply]);
    persist_struct!(LineEdge {
        trips,
        cargo,
        travel_ticks,
        first,
        last
    });
    persist_struct!(Stats {
        observations,
        models_adopted,
        sims_started,
        reconciles,
        chunk_commands,
        duplicate_results,
        trims,
        exchanges,
        attacks,
        attacks_skipped,
        packages_written,
        breakpoints,
        cluster_settles,
    });
    persist_struct!(Books {
        minted,
        burned,
        stolen,
        dropped,
        untracked,
        shortfall,
        trimmed
    });
    persist_struct!(Station { decl });
    persist_struct!(ChannelState { known, capacity });
    persist_struct!(PendingExchange {
        district,
        train,
        entries,
        loot
    });

    fn globals(e: &Engine, w: &mut Writer) {
        e.mode.put(w);
        e.world_seed.put(w);
        e.goods.put(w);
        e.next_cluster.put(w);
        e.stations.put(w);
        e.lines.put(w);
        e.last_departure.put(w);
        e.arrivals.put(w);
        e.legs.put(w);
        e.legs_swept.put(w);
        e.channels.put(w);
        e.channel_sims.put(w);
        e.sinks_of.put(w);
        e.conditions.put(w);
        e.pressure.put(w);
        e.pressure_seq.put(w);
        e.exchanges.put(w);
        e.train_cooldown.put(w);
        e.ops.put(w);
        e.out.put(w);
        e.now.put(w);
        e.stats.put(w);
        e.books.put(w);
        e.log_enabled.put(w);
    }

    fn key48(id: u64, what: &str) -> u64 {
        assert!(
            id < 1 << 48,
            "{what} id {id} does not fit a page key (48 bits)"
        );
        id
    }

    impl Engine {
        /// Every page of the engine's state (A16): globals, one per district, one per replay
        /// cluster with its booked wake. Call between steps; the host puts them in its save batch.
        /// District ids must fit 48 bits (F36).
        pub fn snapshot(&self) -> Vec<(PageKey, Vec<u8>)> {
            let mut out = Vec::with_capacity(1 + self.districts.len() + self.clusters.len());
            let mut w = Writer::new();
            globals(self, &mut w);
            out.push((PageKey::new(GLOBAL_TABLE, 0), w.into_bytes()));
            for (id, d) in &self.districts {
                out.push((
                    PageKey::new(DISTRICT_TABLE, key48(id.0, "district")),
                    to_bytes(d),
                ));
            }
            for (_, region) in self.clusters.iter() {
                let mut w = Writer::new();
                region.cell().model().put(&mut w);
                region.cell().anchor().put(&mut w);
                region.wake().put(&mut w);
                out.push((
                    PageKey::new(CLUSTER_TABLE, key48(region.key(), "cluster")),
                    w.into_bytes(),
                ));
            }
            out
        }

        /// The engine a [`Engine::snapshot`] describes. Tunables are configuration, not state.
        pub fn restore(
            pages: impl IntoIterator<Item = (PageKey, Vec<u8>)>,
            tun: Tunables,
        ) -> Result<Engine> {
            let mut global = None;
            let mut districts = BTreeMap::new();
            let mut clusters = Vec::new();
            for (k, bytes) in pages {
                match k.table() {
                    GLOBAL_TABLE => global = Some(bytes),
                    DISTRICT_TABLE => {
                        let d: District = from_bytes(&bytes)?;
                        districts.insert(d.id, d);
                    }
                    CLUSTER_TABLE => {
                        let mut r = Reader::new(&bytes);
                        let model = Cluster::get(&mut r)?;
                        let anchor = <Tick as Persist>::get(&mut r)?;
                        let wake = Option::<Tick>::get(&mut r)?;
                        r.finish()?;
                        clusters.push((k.id(), model, anchor, wake));
                    }
                    _ => {}
                }
            }
            let g = global.ok_or_else(|| CodecError::Invalid("no globals page".into()))?;
            let mut r = Reader::new(&g);
            let mode = Mode::get(&mut r)?;
            let world_seed = u64::get(&mut r)?;
            let mut e = Engine::new(world_seed, tun, mode, Tick::ZERO);
            e.goods = Persist::get(&mut r)?;
            e.next_cluster = Persist::get(&mut r)?;
            e.stations = Persist::get(&mut r)?;
            e.lines = Persist::get(&mut r)?;
            e.last_departure = Persist::get(&mut r)?;
            e.arrivals = Persist::get(&mut r)?;
            e.legs = Persist::get(&mut r)?;
            e.legs_swept = Persist::get(&mut r)?;
            e.channels = Persist::get(&mut r)?;
            e.channel_sims = Persist::get(&mut r)?;
            e.sinks_of = Persist::get(&mut r)?;
            e.conditions = Persist::get(&mut r)?;
            e.pressure = Persist::get(&mut r)?;
            e.pressure_seq = Persist::get(&mut r)?;
            e.exchanges = Persist::get(&mut r)?;
            e.train_cooldown = Persist::get(&mut r)?;
            e.ops = Persist::get(&mut r)?;
            e.out = Persist::get(&mut r)?;
            e.now = Persist::get(&mut r)?;
            e.stats = Persist::get(&mut r)?;
            e.books = Persist::get(&mut r)?;
            e.log_enabled = Persist::get(&mut r)?;
            r.finish()?;
            e.districts = districts;
            e.clusters = LazyTable::new(world_seed, REPLAY, e.now);
            clusters.sort_by_key(|c| c.0);
            for (key, model, anchor, wake) in clusters {
                let members = model.members();
                let h = e.clusters.restore(key, Lazy::new(model, anchor), wake);
                for m in members {
                    e.cluster_of.insert(m, h);
                }
            }
            Ok(e)
        }
    }
}
