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

use rtsim_archive::kinds::{self, Kind};
use rtsim_archive::{At, Severity, Tape};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::journal::OpId;
use rtsim_kernel::lazy::RegionId;
use rtsim_kernel::rng::SystemId;
use serde::Serialize;

use crate::cluster::{Cluster, ClusterEvent, ClusterView, PoolKey, Run, Stop};
use crate::cmd::{
    BlockDestroy, CarriageDelta, CarriageReason, ChunkReconcile, Cmd, DepositDelta, IndustryOps,
    InvDelta, WritePackage,
};
use crate::deposit::{self, Writeback};
use crate::district::{
    AttackReport, DEPOSIT_INVENTORY, District, Inventory, Model, PendingChunk, Phase, Presence,
    ReconcileReport, SimSpan, SweepBuf,
};
use crate::ids::{
    BlockPos, ChunkPos, DistrictId, Good, GoodClass, GoodInfo, GoodVec, StationId, TrainId,
};
use crate::learn::{LinkKey, Sample};
use crate::obs::{
    BlockFlags, Carriage, CarriageResult, Conditions, Direction, EditKind, LoadState, NotLearnable,
    Obs, PackageEvent, ReconcileResult, StationDecl, Timed, TrainCall, TrainDeparture,
    UnwatchedStop,
};
use crate::reconcile::{self, GoodLookup};
use crate::threat::{
    self, Leg, LegIndex, Threat, ThreatClass, ThreatSource, ThreatTarget, next_pressure_attack,
};
use crate::tracked::{Tracked, TrackedTable};
use crate::tunables::Tunables;

/// How a district replays while away (GLASSBOX.md §4, G2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlassMode {
    /// On its compiled graph, one regime at a time.
    Glass,
    /// Frozen: a compile that has not passed a window yet, or one edited since (G-B4, G2-2).
    Freeze,
    /// By the black box as before (no compile, a compile it does not model yet, or a fallback).
    Black,
}

/// The replay table's system id (the replay draws nothing; the id only names the table).
const REPLAY: SystemId = SystemId::named("industry.replay");

/// The tick of each game day natural attacks land on. TUNABLE, PLACEHOLDER 18,000 (`dayTime`
/// midnight; gameTime and dayTime drift apart when players sleep, INDUSTRY.md §0).
pub const NIGHT_ATTACK_TICK: u64 = 18_000;

/// The magnitudes of an archive "district changed" row: why the learned district went stale.
const STALE_REVISION: i64 = 1;
const STALE_EDIT: i64 = 2;

/// One natural mob's strength in fight units. TUNABLE, PLACEHOLDER 1.0.
pub const NATURAL_STRENGTH: f64 = 1.0;

/// Real-time hour in ticks at 20 per second: the ledger's "per hour".
pub const TICKS_PER_HOUR: u64 = 72_000;

/// Where a district stands, for tests and the CLI ([`Engine::inspect`]). The codes are the retired
/// IND_LEDGER's (docs/industry-boundary.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DistrictState {
    Live = 0,
    Learning = 1,
    Running = 2,
    Stale = 3,
    NotLearnable = 4,
    Suspended = 5,
    Damaged = 6,
    OutOfSeason = 7,
}

/// A district as the engine holds it, for tests, the CLI and an operator's log ([`Engine::inspect`]).
/// **Never sent to a player** (I18, Ben's correction: players see only what their own built
/// readouts measure). The "District ledger" query (IND_LEDGER_REQUEST and IND_LEDGER)
/// is gone from the wire.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Inspection {
    pub district: DistrictId,
    pub state: DistrictState,
    pub reason: Option<String>,
    /// Learned rates, units per real hour.
    pub rates_per_hour: Vec<(Good, f64)>,
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub since: Option<Tick>,
    /// Net change per good since the replay began: what it made (positive) and used (negative).
    pub made_since: GoodVec,
    pub running: Option<bool>,
    pub stop: Option<String>,
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

/// A timed wake of the engine's own ([`Engine::advance_to`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wake {
    /// An approach hold ends, unless the district's stamp moved on.
    Hold(DistrictId, u32),
    /// Level-wide pollution a replay makes is pushed to the level (I14), unless the replay that
    /// booked it (by its start) has ended.
    Pollution(DistrictId, Tick),
}

/// What a reconcile round covers ([`Engine::reconcile_round`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Round {
    /// Every inventory; commands to loaded chunks now, the rest at their loads.
    Full,
    /// Every inventory; every chunk's command now (a player is on the way).
    Ahead,
    /// Only these (loaded) chunks leave the replay; the rest of its change stays virtual.
    Part(BTreeSet<ChunkPos>),
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
    /// Reconciles sent ahead of arrival (IND_APPROACH, the stale-tick answer).
    pub reconciles_ahead: u64,
    /// Level-wide pollution pushed to the level while its district was unloaded (I14).
    pub pollution_pushes: u64,
    /// Loaded calls by when their cargo was read (choice SG30): as the train approached, at its
    /// departure from its previous district station, or at `arriveAt` (late: the station's
    /// interfaces may have moved goods already).
    pub arrivals_approach: u64,
    pub arrivals_departure: u64,
    pub arrivals_late: u64,
    /// Simulated calls at a station with an exchange rule but no known ports: nothing moved
    /// (choice SG33).
    pub exchanges_portless: u64,
    /// Calls at a station outside every district (IND_TRAIN_UNWATCHED, choice SG50): each ended
    /// the train's leg. Not saved (choice SG58): a restore starts it at 0.
    pub unwatched_stops: u64,
    /// Arrivals at the station the train's leg began at, with no unwatched stop between: no edge
    /// (choice SG51). Not saved (choice SG58): a restore starts it at 0.
    pub self_edges_refused: u64,
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

#[derive(Clone, Debug, PartialEq)]
struct PendingExchange {
    district: DistrictId,
    train: TrainId,
    /// (carriage, good, delta into the carriage) asked.
    entries: Vec<(u32, Good, i64)>,
    /// Loot from a train leg (I24) rather than a station exchange.
    loot: bool,
    /// A station exchange's bookings on the station's port inventories (choice SG33).
    ports: Vec<PortBooking>,
}

/// One booking of a simulated station call: (inventory, good, delta into the inventory, booked
/// on its glass store pool).
type PortBooking = (u32, Good, i64, bool);

/// A fraction as an archive magnitude: thousandths.
fn per_mille(f: f64) -> i64 {
    (f * 1000.0).round() as i64
}

/// Where a district's rows go at `at`: its centre, in its dimension.
fn place_of(topo: &rtsim_space::Topologies, d: &District, at: Tick) -> Option<At> {
    let (x, z) = d.centre(topo)?;
    Some(At::new(at.0, d.dim, x, z))
}

/// Items in `v` by absolute amount, for an archive magnitude: fluids, energy and deposits are not
/// items. A good the dictionary does not know counts as one.
fn item_count(goods: &BTreeMap<Good, GoodInfo>, v: &GoodVec) -> i64 {
    v.iter()
        .filter(|(g, _)| goods.get(g).is_none_or(|i| i.class == GoodClass::Item))
        .fold(0i64, |a, (_, n)| a.saturating_add(n.saturating_abs()))
}

/// `v` as a short line for an archive payload: `64 iron ingot, 12 coal`.
fn counts(goods: &BTreeMap<Good, GoodInfo>, v: &GoodVec) -> String {
    let parts: Vec<String> = v
        .iter()
        .map(|(g, n)| {
            let name = goods
                .get(&g)
                .map_or_else(|| format!("good {}", g.0), |i| i.name.clone());
            format!("{n} {name}")
        })
        .collect();
    if parts.is_empty() {
        "nothing".to_string()
    } else {
        parts.join(", ")
    }
}

pub struct Engine {
    pub tun: Tunables,
    world_seed: u64,
    goods: BTreeMap<Good, GoodInfo>,
    /// One page each (F36), written only when changed since the last save (ID1).
    districts: Tracked<DistrictId, District>,
    clusters: TrackedTable<Cluster>,
    cluster_of: BTreeMap<DistrictId, RegionId<Cluster>>,
    next_cluster: u64,
    stations: BTreeMap<StationId, Station>,
    lines: BTreeMap<(StationId, StationId), LineEdge>,
    last_departure: BTreeMap<TrainId, (StationId, Tick, GoodVec)>,
    arrivals: BTreeMap<(TrainId, StationId), Vec<Carriage>>,
    legs: LegIndex,
    legs_swept: u64,
    /// Power sinks of each source (IND_POWER_LINK).
    sinks_of: BTreeMap<DistrictId, BTreeSet<DistrictId>>,
    conditions: Option<Conditions>,
    pressure: BTreeMap<(Tick, u64), (DistrictId, ThreatClass, u32, u32)>,
    pressure_seq: u64,
    /// Approach holds ending and live pollution pushes, by time (lazily cancelled: a hold by the
    /// district's stamp, a push by its replay span, as O6).
    wakes: BTreeMap<(Tick, u64), Wake>,
    wake_seq: u64,
    /// Districts whose replay coverage changed this tick (I7), brought up to date once the
    /// tick's records are in.
    dirty: BTreeSet<DistrictId>,
    /// The Java side reports chunk unloads one by one (IND_CHUNK_UNLOADED); until it does, a
    /// district with no chunk ticking counts as wholly unloaded.
    per_chunk_unloads: bool,
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
    /// The glass box (GLASSBOX.md): compiled graphs, solves, checksums and glass replays. Paged,
    /// one page per district (`glass::page`, G2-9); its counters go in the globals.
    pub glass: crate::glass::Glass,
    /// Every frame's topology (docs/choices-topology.md TP16): districts are local, so only a
    /// train leg's booking reads it, going the short way across a seam. Not saved.
    pub topo: rtsim_space::SharedTopologies,
    /// Op ids of the chunk reconciles sent since the session started. Java holds a command for an
    /// unloaded chunk in memory only, so a restart loses what it held: the next approach sends
    /// again whatever is still pending and was not sent in this session (docs/choices-repair.md
    /// R4). Not saved: a restore starts a session.
    sent_this_session: BTreeSet<OpId>,
    /// The event archive's recorder (VIEWER.md §10.1): off until the host turns it on; not state.
    pub tape: Tape,
    /// Districts declared while the tape was on that have no chunks yet: their "found" row waits
    /// for the first chunks, which give it a place. Not saved.
    undetected: BTreeSet<DistrictId>,
    /// Changes whenever a district comes, goes or changes its chunks (the oil gate's
    /// `DistrictIndex` rebuilds on it, choices-oilgate.md OGC50). Drawn from a process-wide
    /// sequence, so no two engines, a restored one included, ever share a value. Not saved.
    districts_rev: u64,
    /// The page check (ID5): every page's hash as of the last save built, kept only in a debug
    /// build or with `RTSIM_VERIFY_PAGES` set. Not saved.
    page_check: Option<Box<PageCheck>>,
}

/// One save's industry pages ([`Engine::save_changed`], ID2): the globals page and every page
/// changed since the save before, and the pages of districts and clusters gone since then.
#[derive(Debug)]
pub struct ChangedPages {
    pub put: Vec<(rtsim_kernel::page::PageKey, Vec<u8>)>,
    pub delete: Vec<rtsim_kernel::page::PageKey>,
    pub ticket: SaveTicket,
}

/// The pages one save wrote or deleted: a failed save hands it back ([`Engine::save_failed`]),
/// and the next save writes them again (ID3).
#[derive(Debug, Default)]
pub struct SaveTicket {
    districts: Vec<DistrictId>,
    glass: Vec<DistrictId>,
    clusters: Vec<(u64, RegionId<Cluster>)>,
    legs: Vec<u64>,
}

impl SaveTicket {
    /// Pages the save wrote or deleted, the globals page not counted.
    pub fn len(&self) -> usize {
        self.districts.len() + self.glass.len() + self.clusters.len() + self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// What the page check found (ID5), over every save the engine built.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageCheck {
    hashes: BTreeMap<rtsim_kernel::page::PageKey, u64>,
    /// Saves checked.
    pub saves: u64,
    /// Pages a save wrote or deleted, the globals page not counted.
    pub written: u64,
    /// Pages whose bytes changed since the save before but that no mark covered: a missed mark,
    /// so a page the store would have kept stale. Always 0; a debug build panics on one.
    pub unmarked: u64,
    pub first_unmarked: Option<rtsim_kernel::page::PageKey>,
    /// Marked pages whose bytes had not changed: what the conservative marks cost.
    pub unchanged: u64,
}

/// Whether engines keep the page check (ID5): a debug build, or `RTSIM_VERIFY_PAGES` set and not
/// `0`. Read once per process.
pub fn page_check_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        cfg!(debug_assertions) || std::env::var_os("RTSIM_VERIFY_PAGES").is_some_and(|v| v != "0")
    })
}

fn page_hash(b: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// The next value of [`Engine::districts_rev`]; 0 is never handed out.
fn next_districts_rev() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Engine {
    /// Hands the engine every frame's topology (TP6).
    pub fn set_topologies(&mut self, t: rtsim_space::SharedTopologies) {
        self.topo = t;
    }

    pub fn new(world_seed: u64, tun: Tunables, now: Tick) -> Self {
        Self {
            tun,
            world_seed,
            goods: BTreeMap::new(),
            districts: Tracked::default(),
            clusters: TrackedTable::new(world_seed, REPLAY, now),
            cluster_of: BTreeMap::new(),
            next_cluster: 1,
            stations: BTreeMap::new(),
            lines: BTreeMap::new(),
            last_departure: BTreeMap::new(),
            arrivals: BTreeMap::new(),
            legs: LegIndex::default(),
            legs_swept: 0,
            sinks_of: BTreeMap::new(),
            conditions: None,
            pressure: BTreeMap::new(),
            pressure_seq: 0,
            wakes: BTreeMap::new(),
            wake_seq: 0,
            dirty: BTreeSet::new(),
            per_chunk_unloads: false,
            exchanges: BTreeMap::new(),
            train_cooldown: BTreeMap::new(),
            ops: IndustryOps::default(),
            out: Vec::new(),
            now,
            stats: Stats::default(),
            books: Books::default(),
            log: std::collections::VecDeque::new(),
            log_enabled: false,
            glass: crate::glass::Glass::default(),
            topo: rtsim_space::topology::all_flat(),
            sent_this_session: BTreeSet::new(),
            tape: Tape::new(),
            undetected: BTreeSet::new(),
            districts_rev: next_districts_rev(),
            page_check: page_check_on().then(Box::default),
        }
    }

    /// A value that changes whenever a district comes, goes or changes its chunks, and differs
    /// between engines: a cache of district chunks is current while it matches.
    pub fn districts_rev(&self) -> u64 {
        self.districts_rev
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

    // ---- the archive (VIEWER.md §10.1) -------------------------------------------------------

    /// Where `d`'s rows go: its centre at `at`. `None` while the tape is off, or the district has
    /// no chunks yet.
    fn place(&self, d: DistrictId, at: Tick) -> Option<At> {
        if !self.tape.is_on() {
            return None;
        }
        place_of(&self.topo, self.districts.get(&d)?, at)
    }

    /// A district's event at its centre, with no object.
    fn district_event(&mut self, kind: Kind, d: DistrictId, at: Tick, magnitude: i64) {
        if let Some(p) = self.place(d, at) {
            self.tape.event(kind, p, u128::from(d.0), 0, magnitude);
        }
    }

    /// A district's first chunks: its "found" row, at the centre of what has arrived, and its
    /// "not learnable" row if it was declared so (the declaration came before it had a place).
    /// Every later declaration of the same district is not a first sight.
    fn detected(&mut self, d: DistrictId, at: Tick) {
        if !self.undetected.contains(&d) {
            return;
        }
        let Some(p) = self.place(d, at) else {
            return;
        };
        self.undetected.remove(&d);
        let dist = &self.districts[&d];
        let (chunks, phase) = (dist.chunks.len() as i64, dist.phase);
        let id = u128::from(d.0);
        self.tape.event(kinds::INDUSTRY_DETECTED, p, id, 0, chunks);
        if let Phase::NotLearnable(code) = phase {
            self.tape
                .event(kinds::INDUSTRY_UNLEARNABLE, p, id, 0, i64::from(code));
        }
    }

    /// Where a train is at `at` on its booked `leg`: along the straight line between its stations,
    /// the short way across a seam, as the leg was booked. `None` while the tape is off or the
    /// leg's departure station is unknown.
    fn leg_spot(&self, leg: &Leg, at: Tick) -> Option<At> {
        if !self.tape.is_on() {
            return None;
        }
        let from = &self.stations.get(&leg.from)?.decl;
        let to = leg
            .to
            .and_then(|s| self.stations.get(&s))
            .map_or(from, |s| &s.decl);
        let topo = self.topo.of(from.dim as u16);
        let a = topo.canonical(i64::from(from.pos.x), i64::from(from.pos.z));
        let b = topo.toward(a, (i64::from(to.pos.x), i64::from(to.pos.z)));
        let span = leg.eta.0.saturating_sub(leg.depart.0).max(1) as f64;
        let f = (at.0.saturating_sub(leg.depart.0) as f64 / span).clamp(0.0, 1.0);
        let x = a.0 + ((b.0 - a.0) as f64 * f).round() as i64;
        let z = a.1 + ((b.1 - a.1) as f64 * f).round() as i64;
        let (x, z) = topo.canonical(x, z);
        Some(At::new(at.0, from.dim, x as i32, z as i32))
    }

    /// Where a train is at `at` for the archive: on its booked leg, else at the station it last
    /// left. `None` while the tape is off or nothing is known of it.
    fn train_spot(&self, train: TrainId, at: Tick) -> Option<At> {
        if !self.tape.is_on() {
            return None;
        }
        if let Some(id) = self.legs.leg_of(train, at)
            && let Some(leg) = self.legs.get(id)
        {
            return self.leg_spot(leg, at);
        }
        let (station, ..) = self.last_departure.get(&train)?;
        let d = &self.stations.get(station)?.decl;
        Some(At::new(at.0, d.dim, d.pos.x, d.pos.z))
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

    /// The districts with ids in `ids`, in id order (the viewer's tap encodes a range at a time).
    pub fn districts_in(&self, ids: std::ops::Range<u64>) -> impl Iterator<Item = &District> {
        self.districts
            .range(DistrictId(ids.start)..DistrictId(ids.end))
            .map(|(_, d)| d)
    }

    /// Whether district `d` replays (is in a replay cluster), without building its member set.
    pub fn replays(&self, d: DistrictId) -> bool {
        self.cluster_of.contains_key(&d)
    }

    /// The station declarations with ids in `ids`, in id order.
    pub fn stations_in(&self, ids: std::ops::Range<u64>) -> impl Iterator<Item = &StationDecl> {
        self.stations
            .range(StationId(ids.start)..StationId(ids.end))
            .map(|(_, s)| &s.decl)
    }

    /// One station's declaration.
    pub fn station_decl(&self, id: StationId) -> Option<&StationDecl> {
        self.stations.get(&id).map(|s| &s.decl)
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
        self.sent_this_session.clear();
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
        self.glass.session_start(at, &self.tun.glass);
    }

    // ---- time -----------------------------------------------------------------------------

    /// Fires every breakpoint, pressure wake and approach hold due by `to`, in time order, after
    /// the replay coverage of every district whose chunks changed is brought up to date (I7).
    /// The step calls this once the hand-over's records are in.
    pub fn advance_to(&mut self, to: Tick) {
        let now = self.now;
        self.flush_dirty(now);
        self.fire_until(to);
    }

    fn fire_until(&mut self, to: Tick) {
        if to < self.now {
            return;
        }
        loop {
            let next_p = self.pressure.keys().next().map(|k| k.0);
            let next_h = self.wakes.keys().next().map(|k| k.0);
            let next = next_p.into_iter().chain(next_h).min().filter(|&t| t <= to);
            let limit = next.unwrap_or(to);
            while let Some((at, h)) = self.clusters.next_due(limit) {
                self.stats.breakpoints += 1;
                self.after_cluster(h, at);
            }
            match next {
                Some(t) => {
                    self.now = self.now.max(t);
                    // On a tie a hold ends (or pollution is pushed) before an attack lands.
                    if next_h == Some(t) {
                        let (key, wake) = self.wakes.pop_first().expect("peeked");
                        match wake {
                            Wake::Hold(d, stamp) => self.hold_wake(key.0, (d, stamp)),
                            Wake::Pollution(d, started) => self.pollution_wake(key.0, d, started),
                        }
                    } else {
                        let (key, wake) = self.pressure.pop_first().expect("peeked");
                        self.pressure_wake(key.0, wake);
                    }
                    // A wake can change coverage; it settles at its own tick.
                    let now = self.now;
                    self.flush_dirty(now);
                }
                None => break,
            }
        }
        self.now = to;
    }

    /// Handles one observation.
    pub fn ingest(&mut self, t: &Timed) {
        let at = t.at.max(self.now);
        // Coverage changed by this tick's records settles before the next tick's (I7).
        if at > self.now {
            let now = self.now;
            self.flush_dirty(now);
        }
        self.fire_until(at);
        self.stats.observations += 1;
        match &t.obs {
            Obs::Glass(g) => self.glass_obs(at, g),
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
            Obs::Approach {
                district,
                distance,
                eta_ticks,
                closing,
            } => {
                if let Some(dist) = self.districts.get_mut(district) {
                    dist.approach = Some(crate::district::Approach {
                        at,
                        distance: *distance,
                        eta_ticks: *eta_ticks,
                        closing: *closing,
                    });
                    // Ben's answer to the stale-tick race: reconcile ahead of arrival.
                    if *closing && *distance <= self.tun.replay.approach_distance_blocks {
                        self.approach(*district, at);
                    }
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
                // A new revision clears the chunks; the dimension may change.
                self.districts_rev = next_districts_rev();
                let dist = self
                    .districts
                    .get_or_insert_with(decl.district, || District::new(decl.district));
                // The archive's place for what this declaration changes: taken before a new
                // revision clears the chunks.
                let spot = self
                    .tape
                    .is_on()
                    .then(|| place_of(&self.topo, dist, at))
                    .flatten();
                let mut stale = false;
                if dist.revision != decl.revision {
                    if dist.revision != 0 && dist.phase == Phase::Learned {
                        dist.phase = Phase::Stale;
                        stale = true;
                    }
                    // A district seen for the first time has no chunks yet: it is recorded when
                    // they arrive.
                    if dist.revision == 0 && self.tape.is_on() {
                        self.undetected.insert(decl.district);
                    }
                    dist.revision = decl.revision;
                    dist.chunks.clear();
                    dist.inventories.clear();
                    // A new revision numbers its inventories afresh: the stations' ports come
                    // again from their next loaded calls (choice SG32).
                    for rule in dist.exchange.values_mut() {
                        rule.ports.clear();
                    }
                    dist.learner.discard();
                    dist.learner.coupling.reset();
                }
                dist.dim = decl.dim;
                dist.name = decl.name;
                // I9 (Ben: "Everything"): an unloaded district replays everything, spawn-fed mob
                // farms included, so code 6 no longer freezes a district. An older Java side may
                // still send it; it is read as learnable.
                let not_learnable = decl
                    .not_learnable
                    .filter(|n| *n != NotLearnable::SpawnFedFarm);
                dist.not_learnable = not_learnable;
                let mut unlearnable = None;
                if let Some(n) = not_learnable {
                    if dist.phase != Phase::NotLearnable(n.code()) {
                        unlearnable = Some(n.code());
                    }
                    dist.phase = Phase::NotLearnable(n.code());
                } else if matches!(dist.phase, Phase::NotLearnable(_)) {
                    dist.phase = Phase::Learning;
                }
                if let Some(p) = spot {
                    let id = u128::from(decl.district.0);
                    if stale {
                        self.tape
                            .event(kinds::INDUSTRY_STALE, p, id, 0, STALE_REVISION);
                    }
                    if let Some(code) = unlearnable {
                        self.tape
                            .event(kinds::INDUSTRY_UNLEARNABLE, p, id, 0, i64::from(code));
                    }
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
                    self.districts_rev = next_districts_rev();
                    self.detected(*district, at);
                }
            }
            Obs::Inventory(decl) => {
                let dist = self.district_mut(decl.district);
                // The same declaration again (a session start, the module switched on, U10) keeps
                // what the engine knows of the inventory, its pending and held deltas included.
                let same = dist
                    .inventories
                    .get(&decl.index)
                    .is_some_and(|i| i.decl == *decl);
                if dist.revision == decl.revision && !same {
                    let dim = dist.dim;
                    dist.inventories.insert(
                        decl.index,
                        Inventory {
                            decl: decl.clone(),
                            chunk: decl.pos.chunk(dim),
                            known: GoodVec::new(),
                            known_at: Tick::ZERO,
                            pending: GoodVec::new(),
                            held: GoodVec::new(),
                            received: BTreeSet::new(),
                            doomed: false,
                            destroyed: false,
                        },
                    );
                }
            }
            Obs::DistrictRemoved { district } => {
                self.undetected.remove(district);
                if self.districts.contains_key(district) {
                    self.stop_sim(*district, at);
                    self.districts.remove(district);
                    self.districts_rev = next_districts_rev();
                }
                self.glass.remove(*district);
            }
            Obs::Edit {
                district,
                kind,
                player,
                pos,
            } => {
                // Every IND_EDIT is a player's (§3.3): a learned district goes stale (I5), and
                // what its starvation episodes showed may no longer hold (I6). A replay of its
                // unloaded part stops (I7: stale freezes).
                let dist = self.district_mut(*district);
                // A block placed or broken, or a machine configured, makes the player one of its
                // builders (the oil gate's dragons name them, choices-oilgate.md OGC54).
                if *kind != EditKind::Command {
                    dist.note_editor(*player, at);
                }
                dist.learner.discard();
                dist.learner.coupling.reset();
                let went_stale = dist.phase == Phase::Learned;
                if went_stale {
                    dist.phase = Phase::Stale;
                }
                // The compile no longer matches the blocks until Java recompiles (G-B3); of its
                // judging units, those whose box holds the edit (all, for a command's).
                self.glass.edit(
                    at,
                    *district,
                    *pos,
                    *kind == EditKind::Command,
                    &self.tun.glass,
                );
                self.mark(*district);
                if went_stale {
                    self.district_event(kinds::INDUSTRY_STALE, *district, at, STALE_EDIT);
                }
            }
            Obs::Discard {
                district,
                reason,
                station,
            } => {
                self.district_mut(*district).learner.discard();
                if let (crate::obs::DiscardReason::Other, Some(s)) = (reason, station) {
                    // A station's call moved cargo no one measured (SECTIONS.md §3.6): the units
                    // its last ports named lose their windows, every unit when it named none.
                    let ports = self
                        .district_mut(*district)
                        .exchange
                        .get(s)
                        .map(|r| r.ports.clone())
                        .unwrap_or_default();
                    self.glass
                        .station_discard(at, *district, &ports, &self.tun.glass);
                }
            }
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
            Obs::ChunkUnloaded { district, chunk } => {
                self.per_chunk_unloads = true;
                self.district_mut(*district).loaded_chunks.remove(chunk);
                self.glass.chunk_stopped(*district, *chunk);
                self.mark(*district);
            }
            Obs::ChunkLoaded {
                district,
                chunk,
                applied_epoch,
            } => {
                self.district_mut(*district).loaded_chunks.insert(*chunk);
                // It leaves the replay once the tick's records are in (I7).
                self.mark(*district);
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
            Obs::TrainUnwatched(u) => self.train_unwatched(u),
            Obs::Station(s) => {
                if let Some(d) = s.district {
                    self.district_mut(d).stations.insert(s.station);
                }
                self.stations.insert(s.station, Station { decl: s.clone() });
            }
            Obs::Package(p) => self.package(at, p),
            Obs::StationPorts(p) => {
                let mut inventories = p.inventories.clone();
                inventories.sort_unstable();
                inventories.dedup();
                let dist = self.district_mut(p.district);
                dist.stations.insert(p.station);
                dist.exchange.entry(p.station).or_default().ports = inventories;
                // Each station's ports take its cargo as one: their units join (choice SC75).
                let revision = dist.revision;
                let sets = dist.exchange.values().map(|r| r.ports.clone()).collect();
                self.glass
                    .station_ports(at, p.district, revision, sets, &self.tun.glass);
            }
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
                dist.defence.keep_chunks(&dist.chunks);
                self.schedule_pressure(s.district, at);
            }
            Obs::SurveyBlocks {
                district,
                part,
                chunk,
                partial,
                blocks,
                ..
            } => {
                let dist = self.district_mut(*district);
                dist.defence.survey_blocks(*chunk, *part, *partial, blocks);
                dist.defence.keep_chunks(&dist.chunks);
            }
        }
        // A compile, a sweep's cut or a station's ports changed a district's units: Java counts
        // the pairs at their counted stores (SECTIONS.md §4.2).
        for set in self.glass.take_count_sets() {
            self.out.push(Cmd::CountSet(set));
        }
    }

    fn district_mut(&mut self, d: DistrictId) -> &mut District {
        if !self.districts.contains_key(&d) {
            self.districts_rev = next_districts_rev();
        }
        self.districts.get_or_insert_with(d, || District::new(d))
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
        let confidence = per_mille(learned.confidence);
        let dist = self.district_mut(d);
        let batches = dist.batches.clone();
        dist.models.insert(
            season,
            Model::from_learned(learned, targets, season, batches),
        );
        let was = std::mem::replace(&mut dist.phase, Phase::Learned);
        self.stats.models_adopted += 1;
        if was != Phase::Learned {
            let now = self.now;
            self.district_event(kinds::INDUSTRY_LEARNED, d, now, confidence);
        }
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
        self.glass_touch(d, at);
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

    /// A glass-box record (GLASSBOX.md §1.4): the compiled graph, probe samples, chunk ticking.
    /// Read only in G1: nothing here changes the replay.
    fn glass_obs(&mut self, at: Tick, g: &crate::glass::records::GlassObs) {
        let view = self
            .districts
            .get(&g.district())
            .map(|d| crate::glass::check::View {
                revision: d.revision,
                sweep: d.last_sweep,
                inventories: &d.inventories,
                ports: &d.ports,
            });
        self.glass
            .ingest(at, g, view.as_ref(), &self.goods, &self.tun.glass);
        // A new compile or a chunk's ticking changes how the district replays (G2).
        use crate::glass::records::GlassObs;
        if matches!(g, GlassObs::End(_) | GlassObs::ChunkTicking(_)) {
            self.mark(g.district());
        }
    }

    fn sweep_end(&mut self, at: Tick, e: &crate::obs::SweepEnd) {
        let season = self.season_at(at);
        // Split borrows: no clone of the GOOD dictionary per sweep (review finding 16).
        let tun = &self.tun.learn;
        let goods = &self.goods;
        let dist = self
            .districts
            .get_or_insert_with(e.district, || District::new(e.district));
        use crate::glass::report::discard;
        let Some(buf) = dist.sweep.take() else {
            // A sweep that sampled nothing still happened: what an older sweep sampled is gone
            // (an oil well whose pumpjack was broken, choices-oilgate.md OGC71).
            if e.records == 0 {
                dist.last_sweep = e.sweep;
            }
            self.glass
                .discard(at, e.district, discard::NO_SWEEP, &self.tun.glass);
            return;
        };
        if buf.sweep != e.sweep || buf.records != e.records {
            // A latest-wins record was dropped: the whole sweep goes (F5).
            self.glass
                .discard(at, e.district, discard::DROPPED, &self.tun.glass);
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
        dist.last_sweep = e.sweep;
        for s in buf.deposits {
            dist.deposits.insert(s.good, s);
        }
        // The glass checksum (GLASSBOX.md §5.3): every live sweep counts, creative and
        // overstressed ones too (the compile models both). A sweep that was not live discards
        // the district's window (`NOT_LIVE`) and samples the judging units whose chunks all ran
        // (SECTIONS.md §3.4).
        {
            let view = crate::glass::check::View {
                revision: dist.revision,
                sweep: e.sweep,
                inventories: &dist.inventories,
                ports: &dist.ports,
            };
            let live = if e.live {
                crate::glass::check::Liveness::All
            } else {
                crate::glass::check::Liveness::Partial {
                    loaded: &dist.loaded_chunks,
                    dim: dist.dim,
                }
            };
            if let Some(p) = self
                .glass
                .sweep(at, e.district, &view, &live, goods, &self.tun.glass)
            {
                self.out.push(Cmd::ProbeSet(p));
            }
            // A unit of a partly loaded district that failed or was cut anew while the rest
            // replays: the replay starts again over the units that may (SECTIONS.md §3.5).
            if self.glass.districts.get(&e.district).is_some_and(
                |g| matches!((&g.replay, &g.compiled), (Some(r), Some(c)) if !r.same_units(c)),
            ) {
                self.dirty.insert(e.district);
            }
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
            let confidence = per_mille(m.learned.confidence);
            self.stats.models_adopted += 1;
            if changed {
                self.note(e.district, at, what);
            }
            if was != Phase::Learned {
                self.district_event(kinds::INDUSTRY_LEARNED, e.district, at, confidence);
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

    // ---- presence and coverage (I7) --------------------------------------------------------

    /// IND_LOAD_STATE: the district's ticking chunks crossed none or all of them.
    ///
    /// - **Live** (every chunk ticks): the replay stops and everything it did is reconciled (a
    ///   full round), as before.
    /// - **Partial** or **Unloaded**: the replay covers the chunks that are not loaded (I7, Ben's
    ///   "simulate the rest"). Which chunks those are comes from IND_CHUNK_LOADED and
    ///   IND_CHUNK_UNLOADED; the coverage is brought up to date once the tick's records are in
    ///   ([`Engine::mark`]). A Java side that never sends IND_CHUNK_UNLOADED is read the old way:
    ///   with no chunk ticking, every chunk counts as unloaded.
    fn set_presence(&mut self, d: DistrictId, p: Presence, at: Tick) {
        let old = self.districts[&d].presence;
        if old == p {
            return;
        }
        let per_chunk = self.per_chunk_unloads;
        let dist = self.districts.get_mut(&d).expect("exists");
        dist.presence = p;
        // A window must not span a stretch the district did not run whole.
        if old == Presence::Live {
            dist.learner.discard();
        }
        if old == Presence::Unloaded {
            dist.pressure_stamp += 1;
        }
        match p {
            Presence::Live => {
                // A fallback holds until the district is live again (§4.5).
                if let Some(dg) = self.glass.districts.get_mut(&d) {
                    dg.fallback = None;
                }
                if old == Presence::Unloaded {
                    self.glass.back(at, d, &self.tun.glass);
                }
                self.reconcile_round(d, at, Round::Full);
            }
            Presence::Partial => {
                if old == Presence::Unloaded {
                    self.glass.back(at, d, &self.tun.glass);
                }
                self.mark(d);
            }
            Presence::Unloaded => {
                dist.watched = false;
                dist.learner.discard();
                if !per_chunk {
                    dist.loaded_chunks.clear();
                }
                self.mark(d);
                self.schedule_pressure(d, at);
                // No judging unit's window spans the stretch away (choice SC59).
                self.glass.away(at, d, &self.tun.glass);
            }
        }
        // Live or not, its power sinks follow it (F21).
        self.refresh_power(d, at);
    }

    /// The district's coverage changed this tick: brought up to date once the tick's records are
    /// in (a district whose chunks all load in one tick reconciles in one round, not one per
    /// chunk).
    fn mark(&mut self, d: DistrictId) {
        self.dirty.insert(d);
    }

    /// Brings every marked district's replay to the chunks that are unloaded now.
    fn flush_dirty(&mut self, at: Tick) {
        while let Some(d) = self.dirty.pop_first() {
            self.sync_coverage(d, at);
        }
    }

    /// Makes district `d`'s replay cover exactly its unloaded inventories (I7), or nothing when it
    /// is live, frozen (I5), or held for a player on the way (the stale-tick answer).
    ///
    /// - Inventories whose chunks loaded leave the replay in a **partial round**: the replay stops,
    ///   its change is planned over the inventories it covered, and only the loaded chunks' deltas
    ///   go to Java now, each chunk on its own epoch. The rest of the change stays virtual (owed)
    ///   and rides into the replay that goes on for the chunks still unloaded.
    /// - Inventories whose chunks unloaded join it: the replay stops, its change is carried as
    ///   virtual stock, and it starts again over the larger part from what the engine knows they
    ///   hold.
    fn sync_coverage(&mut self, d: DistrictId, at: Tick) {
        let season = self.season_at(at);
        let Some(dist) = self.districts.get(&d) else {
            return;
        };
        if dist.presence == Presence::Live {
            return;
        }
        let held = dist.hold_until.is_some_and(|h| h > at);
        let mode = self.glass_mode(d);
        let want = match mode {
            _ if held => BTreeSet::new(),
            GlassMode::Glass => self.glass_covered(d),
            GlassMode::Freeze => BTreeSet::new(),
            GlassMode::Black if dist.freeze_reason(season).is_some() => BTreeSet::new(),
            GlassMode::Black => dist.unloaded_inventories(),
        };
        if let Some(span) = &dist.sim {
            let same_kind = span.glass == (mode == GlassMode::Glass);
            // A glass replay goes on while its nodes run where they did and its judging units
            // are the ones that may replay now (SECTIONS.md §3.5).
            let same_live = !span.glass
                || self.glass.districts.get(&d).is_some_and(|g| {
                    matches!((&g.replay, &g.compiled), (Some(r), Some(c))
                        if r.live() == self.glass_live(d).as_slice() && r.same_units(c))
                });
            if span.invs == want && same_kind && same_live {
                return;
            }
            let leaving: BTreeSet<ChunkPos> = span
                .invs
                .difference(&want)
                .filter_map(|i| dist.inventories.get(i))
                .map(|i| i.chunk)
                .filter(|c| dist.loaded_chunks.contains(c))
                .collect();
            if leaving.is_empty() {
                self.carry_sim(d, at);
            } else {
                self.reconcile_round(d, at, Round::Part(leaving));
            }
        }
        if !want.is_empty() {
            self.start_sim(d, at);
        }
    }

    /// A learned, fresh district starts its replay over its unloaded inventories, from what the
    /// engine knows they hold (I5, I6, I7). Nothing starts while it is live, frozen, held for a
    /// player on the way, or when no process could run in the part that is unloaded.
    fn start_sim(&mut self, d: DistrictId, at: Tick) {
        match self.glass_mode(d) {
            GlassMode::Glass => return self.start_glass(d, at),
            GlassMode::Freeze => return,
            GlassMode::Black => {}
        }
        let season = self.season_at(at);
        let dist = &self.districts[&d];
        if dist.freeze_reason(season).is_some()
            || dist.sim.is_some()
            || dist.presence == Presence::Live
            || dist.hold_until.is_some_and(|h| h > at)
        {
            return;
        }
        let part = dist.unloaded_inventories();
        if part.is_empty() {
            return;
        }
        let model = dist.models[&season].clone();
        let base = dist.stock_of(&part);
        let Some(mut spec) = dist.member_spec(&model, &base, &part, &self.goods, &self.tun) else {
            return;
        };
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
        // Districts joined by a PowerGrid line join one cluster (I13). An Ender Transmission
        // channel never joins two districts: its endpoints are one district's (Ben's correction).
        let mut linked: BTreeSet<DistrictId> = BTreeSet::new();
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
        let groups = spec
            .procs
            .iter()
            .filter(|p| p.id < crate::district::LINK_PROC_BASE)
            .count();
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
        // Sinks already replaying on this district's power now follow it (I13).
        self.refresh_power(d, at);
        // Level-wide pollution it makes reaches the level while it is away (I14).
        let pollutes = model.rates.iter().any(|&(g, r)| {
            r.rate_fixed() > 0 && self.is_live(g) && self.districts[&d].deposits.contains_key(&g)
        });
        let dist = self.districts.get_mut(&d).expect("exists");
        let whole = dist.is_whole(&part);
        let covered = part.len();
        dist.sim = Some(SimSpan {
            started: at,
            base,
            season,
            invs: part,
            glass: false,
            folded: GoodVec::new(),
        });
        self.stats.sims_started += 1;
        if let Some(dg) = self.glass.districts.get_mut(&d) {
            dg.replay_model = crate::glass::report::replay_model::BLACK_BOX;
            dg.replay_since = Some(at);
        }
        if pollutes {
            self.wake_seq += 1;
            let at_next = at + self.tun.replay.live_pollution_ticks.max(1);
            self.wakes
                .insert((at_next, self.wake_seq), Wake::Pollution(d, at));
        }
        let what = if whole {
            format!("replay starts ({groups} process(es))")
        } else {
            format!(
                "replay starts over the unloaded part ({covered} inventories, {groups} process(es))"
            )
        };
        self.note(d, at, what);
        self.after_cluster(h, at);
    }

    /// Takes a district out of its cluster at `at`; returns its pools' levels (stock and mail).
    fn stop_sim(
        &mut self,
        d: DistrictId,
        at: Tick,
    ) -> Option<(SimSpan, Vec<crate::cluster::Pool>)> {
        let h = *self.cluster_of.get(&d)?;
        self.cluster_of.remove(&d);
        if let Some(dg) = self.glass.districts.get_mut(&d) {
            dg.replay = None;
            dg.replay_model = crate::glass::report::replay_model::NONE;
            dg.replay_since = None;
        }
        let out = self.clusters.apply(h, at, |c, at| c.remove_member(d, at))?;
        // Book what the replay made and used.
        let mut minted = GoodVec::new();
        let mut burned = GoodVec::new();
        for p in &out.pools {
            minted.add(p.key.good(), p.books.minted);
            burned.add(p.key.good(), p.books.burned);
        }
        self.books.minted.merge(&minted);
        self.books.burned.merge(&burned);
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
        let Some((events, settles)) = self.clusters.apply(h, at, |c, _| {
            let s = c.settles;
            c.settles = 0;
            (c.take_events(), s)
        }) else {
            return;
        };
        self.stats.cluster_settles += settles;
        let mut season_change = false;
        let mut glass_stops: Vec<(DistrictId, u16, Stop)> = Vec::new();
        for e in events {
            match e {
                ClusterEvent::Stopped { at, owner, id, why }
                    if id < crate::district::LINK_PROC_BASE =>
                {
                    // A glass replay's parts are its processes 0, 1, ... (choice G2-22).
                    if why != Stop::Disabled
                        && self
                            .glass
                            .districts
                            .get(&owner)
                            .and_then(|g| g.replay.as_ref())
                            .is_some_and(|r| id < r.parts())
                    {
                        glass_stops.push((owner, id, why));
                    }
                    if let Some(dist) = self.districts.get_mut(&owner) {
                        dist.last_run = Some((at, Some(why)));
                    }
                    if self.log_enabled && why != Stop::Disabled {
                        let words = stop_words(why, &self.goods);
                        let which = if id > 0 {
                            format!(" (process {id})")
                        } else {
                            String::new()
                        };
                        self.note(owner, at, format!("breakpoint{which}: {words}"));
                    }
                }
                ClusterEvent::Started { at, owner, id } if id < crate::district::LINK_PROC_BASE => {
                    if let Some(dist) = self.districts.get_mut(&owner) {
                        dist.last_run = Some((at, None));
                    }
                    if self.log_enabled {
                        let which = if id > 0 {
                            format!(" (process {id})")
                        } else {
                            String::new()
                        };
                        self.note(owner, at, format!("running{which}"));
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
        for (d, id, why) in glass_stops {
            self.glass_step(h, d, Some((id, why)), at);
        }
        if season_change {
            self.season_boundary(h, at);
        }
    }

    // ---- the glass replay (GLASSBOX.md §4, G2) ----------------------------------------------

    /// How district `d` replays while away. A compiled district replays on its graph once a
    /// window of the compile passed (G-B4), and freezes before that; with judging units, once one
    /// unit may replay (SECTIONS.md §3.5: the others are gated off). What the glass replay does
    /// not model yet (deposits, cross-district links, PowerGrid coupling, damage) keeps the black
    /// box (choice G2-16), as does a compile whose glass replay fell back with no unit left to
    /// replay (§4.5).
    fn glass_mode(&self, d: DistrictId) -> GlassMode {
        let Some(dg) = self.glass.districts.get(&d) else {
            return GlassMode::Black;
        };
        let Some(comp) = dg.compiled.as_ref() else {
            return GlassMode::Black;
        };
        let Some(dist) = self.districts.get(&d) else {
            return GlassMode::Black;
        };
        if dist.phase == Phase::Damaged
            || !dist.deposits.is_empty()
            || !dist.links.is_empty()
            || dist.power_source.is_some()
            || self.sinks_of.get(&d).is_some_and(|s| !s.is_empty())
            || dg.fallback.is_some()
        {
            return GlassMode::Black;
        }
        if comp.replayable() {
            GlassMode::Glass
        } else {
            GlassMode::Freeze
        }
    }

    /// Does chunk `c` of the district run in the world (loaded and block-ticking)? A Java side
    /// that sends no IND_CHUNK_TICKING is read as "every loaded chunk ticks" (choice G2-17).
    fn chunk_runs(&self, d: DistrictId, c: ChunkPos) -> bool {
        let Some(dist) = self.districts.get(&d) else {
            return false;
        };
        let ticking = self.glass.districts.get(&d).map(|g| &g.ticking);
        dist.loaded_chunks.contains(&c) && ticking.is_none_or(|t| t.is_empty() || t.contains(&c))
    }

    /// The inventories a glass replay covers: every one whose chunk does not run (I7, §6.1).
    fn glass_covered(&self, d: DistrictId) -> BTreeSet<u32> {
        let dist = &self.districts[&d];
        dist.inventories
            .values()
            .filter(|i| !i.destroyed && !self.chunk_runs(d, i.chunk))
            .map(|i| i.decl.index)
            .collect()
    }

    /// Per graph node: it runs in the world. A compile on a moving frame (a Sable ship, frame
    /// not 0) has no world positions: every node is simulated (choice G2-18).
    fn glass_live(&self, d: DistrictId) -> Vec<bool> {
        let Some(comp) = self
            .glass
            .districts
            .get(&d)
            .and_then(|g| g.compiled.as_ref())
        else {
            return Vec::new();
        };
        let dim = self.districts.get(&d).map_or(0, |x| x.dim);
        if comp.graph.end.frame != 0 {
            return vec![false; comp.graph.nodes.len()];
        }
        comp.graph
            .nodes
            .iter()
            .map(|n| self.chunk_runs(d, n.pos.chunk(dim)))
            .collect()
    }

    /// Starts a glass replay over the district's covered inventories, from what the engine knows
    /// they hold. It has its own cluster (G2-16 keeps linked districts on the black box).
    fn start_glass(&mut self, d: DistrictId, at: Tick) {
        use crate::glass::replay::{Replay, Setup, StoreDecl};
        let season = self.season_at(at);
        let Some(dist) = self.districts.get(&d) else {
            return;
        };
        if dist.sim.is_some()
            || dist.presence == Presence::Live
            || dist.hold_until.is_some_and(|h| h > at)
        {
            return;
        }
        let part = self.glass_covered(d);
        if part.is_empty() {
            return;
        }
        let mut setup = Setup {
            district: d,
            live: self.glass_live(d),
            covered: BTreeMap::new(),
            contents: BTreeMap::new(),
            owed: dist.owed.clone(),
            live_decls: BTreeMap::new(),
        };
        let mut base = GoodVec::new();
        for inv in dist.inventories.values().filter(|i| !i.destroyed) {
            let c = inv.effective();
            if part.contains(&inv.decl.index) {
                setup.covered.insert(
                    inv.decl.index,
                    StoreDecl {
                        item_slots: inv.decl.item_slots,
                        fluid_capacity: inv.decl.fluid_capacity,
                    },
                );
                base.merge(&c);
            } else {
                setup.live_decls.insert(
                    inv.decl.index,
                    StoreDecl {
                        item_slots: inv.decl.item_slots,
                        fluid_capacity: inv.decl.fluid_capacity,
                    },
                );
            }
            setup.contents.insert(inv.decl.index, c);
        }
        let Engine {
            glass, goods, tun, ..
        } = self;
        let Some(dg) = glass.districts.get_mut(&d) else {
            return;
        };
        let Some(comp) = dg.compiled.as_mut() else {
            return;
        };
        let (rp, spec) = match Replay::start(comp, setup, goods, &tun.glass, at) {
            Ok(x) => x,
            Err(f) => {
                if f == crate::glass::replay::Fallback::Oscillating {
                    comp.replay_oscillations += 1;
                }
                dg.fallback = Some(f);
                self.glass.stats.fallbacks += 1;
                self.note(d, at, format!("glass replay falls back: {}", f.words()));
                self.start_sim(d, at);
                return;
            }
        };
        let cut = rp.cut_edges();
        let parts = rp.parts();
        let frozen = rp.frozen().iter().filter(|&&f| f).count();
        dg.replay = Some(rp);
        dg.replay_model = crate::glass::report::replay_model::GLASS;
        dg.replay_since = Some(at);
        let mut c = Cluster::new(self.tun.replay.restart_min_run_ticks);
        c.add_member(spec, at);
        let key = self.next_cluster;
        self.next_cluster += 1;
        let h = self.clusters.insert(key, c, at);
        self.cluster_of.insert(d, h);
        let dist = self.districts.get_mut(&d).expect("exists");
        let whole = dist.is_whole(&part);
        let covered = part.len();
        dist.sim = Some(SimSpan {
            started: at,
            base,
            season,
            invs: part,
            glass: true,
            folded: GoodVec::new(),
        });
        self.stats.sims_started += 1;
        self.glass.stats.replays += 1;
        let mut what = if whole {
            "glass replay starts".to_string()
        } else {
            format!("glass replay starts over {covered} inventories ({cut} cut edges)")
        };
        if frozen > 0 {
            what.push_str(&format!(", {frozen} units frozen"));
        }
        self.note(d, at, what);
        self.after_cluster(h, at);
        // A first regime blocked at once stops silently: take it as its breakpoint.
        for id in 0..parts {
            if let Some(Run::Stopped(w)) = self
                .clusters
                .get(h)
                .and_then(|r| r.cell().model().run_of(d, id))
                && w != Stop::Disabled
            {
                self.glass_step(h, d, Some((id, w)), at);
            }
        }
    }

    /// The glass replay of `d` at a breakpoint of its part `why.0` (`why.1`), or after an outside
    /// event changed its pools (None): the next regime, or a fallback.
    fn glass_step(
        &mut self,
        h: RegionId<Cluster>,
        d: DistrictId,
        why: Option<(u16, Stop)>,
        at: Tick,
    ) {
        if self.cluster_of.get(&d) != Some(&h) {
            return;
        }
        let Engine {
            clusters,
            glass,
            goods,
            tun,
            dirty,
            ..
        } = self;
        let Some(dg) = glass.districts.get_mut(&d) else {
            return;
        };
        let (Some(rp), Some(comp)) = (dg.replay.as_mut(), dg.compiled.as_mut()) else {
            return;
        };
        if rp.compile() != comp.graph.end.compile {
            // A new compile came in: the coverage pass stops this replay.
            dirty.insert(d);
            return;
        }
        let fz = rp.freezes();
        let r = clusters.apply(h, at, |c, at| match why {
            Some((id, w)) => rp.on_stop(comp, c, id, w, at, goods, &tun.glass),
            None => rp.refresh(comp, c, at, goods, &tun.glass),
        });
        let frozen = rp.frozen().iter().filter(|&&f| f).count();
        let froze = rp.freezes() > fz;
        match r {
            Some(Err(f)) => self.glass_fallback(d, f, at),
            _ => {
                if froze {
                    self.note(
                        d,
                        at,
                        format!("glass replay freezes units: {frozen} frozen, the rest go on"),
                    );
                }
                self.after_cluster(h, at);
            }
        }
    }

    /// The glass replay cannot go on (§4.5): what it did is carried, and the district replays by
    /// the black box until the next compile or until it is live again.
    fn glass_fallback(&mut self, d: DistrictId, f: crate::glass::replay::Fallback, at: Tick) {
        if let Some(dg) = self.glass.districts.get_mut(&d) {
            dg.fallback = Some(f);
            if f == crate::glass::replay::Fallback::Oscillating
                && let Some(c) = dg.compiled.as_mut()
            {
                c.replay_oscillations += 1;
            }
        }
        self.glass.stats.fallbacks += 1;
        self.note(d, at, format!("glass replay falls back: {}", f.words()));
        self.sync_coverage(d, at);
    }

    /// An outside event changed a glass replay's pools (an import, an export, a theft).
    fn glass_touch(&mut self, d: DistrictId, at: Tick) {
        if !self
            .glass
            .districts
            .get(&d)
            .is_some_and(|g| g.replay.is_some())
        {
            return;
        }
        if let Some(&h) = self.cluster_of.get(&d) {
            self.glass_step(h, d, None, at);
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
                Some(src)
            } else {
                None
            };
            self.clusters
                .apply(h, at, |c, at| c.set_requires(s, req, at));
            self.after_cluster(h, at);
        }
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

    /// Stops a replay and carries the change so far as virtual stock (owed), so the reconcile at
    /// the next load still sees the whole change since the first unload.
    fn carry_sim(&mut self, d: DistrictId, at: Tick) {
        let Some((span, pools)) = self.stop_sim(d, at) else {
            return;
        };
        let live: Vec<Good> = pools
            .iter()
            .map(|p| p.key.good())
            .filter(|g| self.is_live(*g))
            .collect();
        let dist = self.districts.get_mut(&d).expect("member");
        for p in &pools {
            dist.minted_carried.add(p.key.good(), p.books.minted);
            match p.key {
                // A glass replay's district pool holds only events (imports waiting, G2-14).
                PoolKey::Stock(_, g) if span.glass => dist.owed.add(g, p.level - p.books.initial),
                // Level pollution pushed live (I14) is in the world (and in `remaining`) already.
                PoolKey::Stock(_, g) if live.contains(&g) => dist.owed.add(g, p.level - p.pushed),
                PoolKey::Stock(_, g) => dist.owed.add(g, p.level - span.base.get(g)),
                PoolKey::Mail(_, _, _, g) => dist.owed.add(g, p.level - p.pushed),
                // A store's change stays with that store (G2-3), counted in its contents.
                PoolKey::Store(_, inv, g) => {
                    let change = p.level - p.books.initial;
                    match dist.inventories.get_mut(&inv) {
                        Some(i) if !i.destroyed => i.held.add(g, change),
                        _ => dist.owed.add(g, change),
                    }
                }
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
    }

    /// Stops a replay and starts it again from where it stood, with a member rebuilt from the
    /// district as it is now (a new season's model, a destroyed inventory's capacity gone, new
    /// coupling evidence).
    fn restart_sim(&mut self, d: DistrictId, at: Tick) {
        if !self.cluster_of.contains_key(&d) {
            return;
        }
        self.carry_sim(d, at);
        self.start_sim(d, at);
    }

    // ---- reconcile (I7, I8) ---------------------------------------------------------------

    /// One reconcile round (I7, I8): the replay stops and what it did, with what the district
    /// owes, becomes per-inventory deltas.
    ///
    /// - [`Round::Full`]: over every live inventory; commands for every chunk, sent now to the
    ///   chunks that are loaded and at each other chunk's IND_CHUNK_LOADED (F27).
    /// - [`Round::Ahead`]: the same, but every chunk's command is sent now: a player is on the
    ///   way (IND_APPROACH), and Java holds each until its chunk loads and applies it at the
    ///   chunk's load, before any machine there ticks (the stale-tick answer).
    /// - [`Round::Part`]: planned over the inventories the replay covered; only the given chunks'
    ///   deltas (and deposits and destroyed blocks there) go to Java, each chunk on its own epoch;
    ///   the rest stays virtual in `owed` for the replay that goes on (I7).
    fn reconcile_round(&mut self, d: DistrictId, at: Tick, round: Round) {
        let stopped = self.stop_sim(d, at);
        let live: BTreeSet<Good> = stopped
            .iter()
            .flat_map(|(_, pools)| pools.iter().map(|p| p.key.good()))
            .filter(|g| self.is_live(*g))
            .collect();
        let dist = self.districts.get_mut(&d).expect("exists");
        let mut net = GoodVec::new();
        // A glass replay's `Stock` change (the cut's carry both ways, imports): owed, so it is
        // planned with the district's owed and lands on the live side where it crossed (G2-11).
        let mut stock = GoodVec::new();
        let mut deposits = GoodVec::new();
        let mut span_minted = std::mem::take(&mut dist.minted_carried);
        let since = stopped.as_ref().map_or(at, |s| s.0.started);
        let all: BTreeSet<u32> = dist
            .inventories
            .values()
            .filter(|i| !i.destroyed)
            .map(|i| i.decl.index)
            .collect();
        // The replay's change happened in the inventories it covered: it is planned there.
        let covered: BTreeSet<u32> = stopped
            .as_ref()
            .map_or_else(|| all.clone(), |(span, _)| span.invs.clone());
        // A glass replay's per-store changes land in their own stores (G2-3).
        let mut direct: BTreeMap<(u32, Good), i64> = BTreeMap::new();
        if let Some((span, pools)) = stopped {
            for p in &pools {
                span_minted.add(p.key.good(), p.books.minted);
                match p.key {
                    PoolKey::Store(_, inv, g) => {
                        let change = p.level - p.books.initial;
                        if dist.inventories.get(&inv).is_some_and(|i| !i.destroyed) {
                            *direct.entry((inv, g)).or_default() += change;
                        } else {
                            net.add(g, change);
                        }
                    }
                    PoolKey::Stock(_, g) if span.glass => stock.add(g, p.level - p.books.initial),
                    PoolKey::Stock(_, g) => {
                        // Level pollution pushed live (I14) is in the world already.
                        let change = if live.contains(&g) {
                            p.level - p.pushed
                        } else {
                            p.level - span.base.get(g)
                        };
                        if self.goods.get(&g).map(|i| i.class) == Some(GoodClass::Deposit) {
                            deposits.add(g, change);
                        } else {
                            net.add(g, change);
                        }
                    }
                    // Goods packed but not yet a whole package go back to stock.
                    PoolKey::Mail(_, _, _, g) => net.add(g, p.level - p.pushed),
                }
            }
        }
        deposits.merge(&std::mem::take(&mut dist.deposit_owed));
        let leaving = match &round {
            Round::Part(chunks) => Some(chunks),
            Round::Full | Round::Ahead => None,
        };
        let goes = |c: &ChunkPos| leaving.is_none_or(|l| l.contains(c));
        let states = |dist: &District, set: &BTreeSet<u32>| -> Vec<reconcile::InvState> {
            dist.inventories
                .values()
                .filter(|i| !i.destroyed && set.contains(&i.decl.index))
                .map(|i| i.state())
                .collect()
        };
        // 1. The replay's change, over what it covered. 2. What the district owes (unplaced
        // changes, theft while frozen), with what step 1 could not place: over everything in a
        // full round, over the covered part in a partial one (it stays virtual there).
        let owed = std::mem::take(&mut dist.owed);
        // Direct store changes first: the plan below sees the stores as they left them.
        for (&(i, g), &n) in &direct {
            if let Some(inv) = dist.inventories.get_mut(&i) {
                inv.held.add(g, n);
            }
        }
        let plan1 = reconcile::plan(&states(dist, &covered), &net, &self.goods);
        for (&(i, g), &n) in &direct {
            if let Some(inv) = dist.inventories.get_mut(&i) {
                inv.held.add(g, -n);
            }
        }
        let mut deltas = direct;
        for p in &plan1.deltas {
            *deltas.entry((p.inventory, p.good)).or_default() += p.delta;
        }
        let mut rest = plan1.unplaced.clone();
        rest.merge(&owed);
        rest.merge(&stock);
        if !rest.is_empty() {
            // Step 2 sees the inventories as step 1 left them.
            for (&(i, g), &n) in &deltas {
                if let Some(inv) = dist.inventories.get_mut(&i) {
                    inv.held.add(g, n);
                }
            }
            let scope2 = if leaving.is_some() { &covered } else { &all };
            let plan2 = reconcile::plan(&states(dist, scope2), &rest, &self.goods);
            for (&(i, g), &n) in &deltas {
                if let Some(inv) = dist.inventories.get_mut(&i) {
                    inv.held.add(g, -n);
                }
            }
            for p in &plan2.deltas {
                *deltas.entry((p.inventory, p.good)).or_default() += p.delta;
            }
            rest = plan2.unplaced;
        }
        // What does not go to Java now stays virtual: unplaced changes as owed, and (in a partial
        // round) each staying inventory's deltas as its `held`, counted in its contents, so the
        // replay that goes on starts from them and the next round sends them.
        dist.owed = rest.clone();
        for ((i, g), n) in deltas {
            dist.inventories
                .get_mut(&i)
                .expect("planned inventory")
                .held
                .add(g, n);
        }
        let mut by_chunk: BTreeMap<ChunkPos, Vec<InvDelta>> = BTreeMap::new();
        let mut sent_net = GoodVec::new();
        for inv in dist.inventories.values_mut() {
            if inv.held.is_empty() || !goes(&inv.chunk) {
                continue;
            }
            let held = std::mem::take(&mut inv.held);
            for (g, n) in held.iter() {
                inv.pending.add(g, n);
                sent_net.add(g, n);
                by_chunk.entry(inv.chunk).or_default().push(InvDelta {
                    inventory: inv.decl.index,
                    pos: inv.decl.pos,
                    good: g,
                    delta: n,
                });
            }
        }
        // Deposits (I11): with the chunk, or at once; a partial round writes back only the
        // deposits in the chunks leaving.
        let mut immediate: Vec<Cmd> = Vec::new();
        for (g, delta) in deposits.iter() {
            let Some(s) = dist.deposits.get_mut(&g) else {
                continue;
            };
            if !goes(&s.chunk) {
                dist.deposit_owed.add(g, delta);
                continue;
            }
            s.remaining += delta;
            sent_net.add(g, delta);
            match deposit::writeback(s.plugin, self.goods.get(&g)) {
                Writeback::WithChunk => by_chunk.entry(s.chunk).or_default().push(InvDelta {
                    inventory: DEPOSIT_INVENTORY,
                    pos: BlockPos::new(s.chunk.x * 16, 0, s.chunk.z * 16),
                    good: g,
                    delta,
                }),
                Writeback::Immediate | Writeback::Live => {
                    immediate.push(Cmd::DepositDelta(DepositDelta {
                        op: OpId(0),
                        plugin: s.plugin,
                        chunk: s.chunk,
                        good: g,
                        delta,
                    }))
                }
            }
        }
        let mut destroys: BTreeMap<ChunkPos, Vec<BlockDestroy>> = BTreeMap::new();
        let pending_chunks: Vec<ChunkPos> = dist.destroy_pending.keys().copied().collect();
        for c in pending_chunks {
            if goes(&c)
                && let Some(v) = dist.destroy_pending.remove(&c)
            {
                destroys.insert(c, v);
            }
        }
        if by_chunk.is_empty() && destroys.is_empty() && immediate.is_empty() {
            // Nothing for Java this round; what the replay made rides on as virtual stock.
            dist.minted_carried = span_minted;
            return;
        }
        self.stats.reconciles += 1;
        dist.epoch += 1;
        let epoch = dist.epoch;
        let mut report = ReconcileReport {
            epoch,
            at,
            since,
            net: sent_net,
            unplaced: rest,
            ..ReconcileReport::default()
        };
        for mut c in immediate {
            c.set_op(self.ops.derive(self.world_seed, self.now, &c));
            self.out.push(c);
        }
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
        // A partial round leaves part of that span virtual, so the cap rides on with it too.
        if leaving.is_some() {
            dist.minted_carried = span_minted.clone();
        }
        dist.round_added.insert(epoch, span_minted);
        // Keep the books of the last few rounds only (trims refer to recent ones).
        while dist.round_added.len() > 8 {
            dist.round_added.pop_first();
        }
        let what = format!(
            "reconcile epoch {epoch}{}: {} in {} chunk(s), {} deltas{}{}",
            match round {
                Round::Full => "",
                Round::Ahead => " (ahead of arrival)",
                Round::Part(_) => " (part loaded)",
            },
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
        if self.tape.is_on() {
            let items = item_count(&self.goods, &report.net);
            self.district_event(kinds::INDUSTRY_RECONCILE, d, at, items);
        }
        let dist = self.districts.get_mut(&d).expect("exists");
        dist.push_report(report);
        let send: Vec<ChunkPos> = if round == Round::Ahead {
            dist.pending.keys().copied().collect()
        } else {
            dist.loaded_chunks.iter().copied().collect()
        };
        self.note(d, at, what);
        for c in send {
            self.send_pending_from(d, c, epoch);
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
                    self.sent_this_session.insert(p.cmd.op);
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
            self.glass
                .port_one(r.district, dist.revision, e.inventory, e.good, e.applied);
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
            let revision = dist.revision;
            for inv in dist.inventories.values_mut() {
                if inv.doomed && r.destroyed.contains(&inv.decl.pos) {
                    inv.doomed = false;
                    inv.destroyed = true;
                    let spill = inv.effective();
                    self.books.dropped.merge(&spill);
                    dist.ports.subtract(&spill);
                    let out: GoodVec = spill.iter().map(|(g, a)| (g, -a)).collect();
                    self.glass.port(r.district, revision, inv.decl.index, &out);
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
                self.sent_this_session.insert(p.cmd.op);
                self.out.push(Cmd::ChunkReconcile(p.cmd.clone()));
            }
        }
    }

    // ---- ports, trains, packages ----------------------------------------------------------

    /// Goods that crossed the boundary while loaded (§3.3). Ender Transmission channels cross no
    /// boundary: their endpoints are one district's (I13 as corrected), so they have no port kind.
    fn port(&mut self, at: Tick, p: &crate::obs::PortTransfer) {
        let dist = self.district_mut(p.district);
        dist.ports.merge(&p.goods);
        let revision = dist.revision;
        if let Some(inv) = p.inventory {
            self.glass.port(p.district, revision, inv, &p.goods);
        } else if p.kind == crate::obs::PortKind::TrainCargo {
            // The train's whole change at the station it stands at, a loaded call (§4.4): mapped
            // through that station's ports, which Java sends before the cargo (choice SC75).
            let train = TrainId(p.counterpart);
            let station = self
                .arrivals
                .range((train, StationId(0))..=(train, StationId(u64::MAX)))
                .map(|(&(_, s), _)| s)
                .next();
            let ports = station
                .and_then(|s| self.district_mut(p.district).exchange.get(&s))
                .map(|r| r.ports.clone())
                .unwrap_or_default();
            self.glass
                .cargo(at, p.district, revision, &ports, &p.goods, &self.tun.glass);
        }
    }

    /// A train's call outside every district (IND_TRAIN_UNWATCHED, choice SG50): the leg it began at
    /// its last district station ends here. The next district station it reaches is no edge from
    /// that one, and never an edge from that station to itself.
    fn train_unwatched(&mut self, u: &UnwatchedStop) {
        self.last_departure.remove(&u.train);
        self.stats.unwatched_stops += 1;
    }

    /// One trip along the edge `(from, c.station)`: `from` is where the train's leg began at `depart`.
    fn line_edge(
        &mut self,
        at: Tick,
        c: &TrainCall,
        from: StationId,
        depart: Tick,
        cargo: GoodVec,
    ) {
        let e = self.lines.entry((from, c.station)).or_default();
        let first = e.trips == 0;
        if first {
            e.first = at;
        }
        e.trips += 1;
        e.cargo.merge(&cargo);
        e.travel_ticks += at.since(depart);
        e.last = at;
        if first && self.tape.is_on() {
            // At the station it arrived at, or where it left from if that one was not declared.
            let station = self
                .stations
                .get(&c.station)
                .or_else(|| self.stations.get(&from));
            if let Some(s) = station {
                let p = At::new(at.0, s.decl.dim, s.decl.pos.x, s.decl.pos.z);
                let items = item_count(&self.goods, &cargo);
                self.tape.event(
                    kinds::INDUSTRY_LINE,
                    p,
                    u128::from(from.0),
                    u128::from(c.station.0),
                    items,
                );
            }
        }
    }

    fn train_arrived(&mut self, at: Tick, c: &TrainCall) {
        // Line edges (§8.8). A leg that ends where it began is no edge (choice SG51): the train went
        // out and back through stops nothing watched, so its travel time is a round trip's.
        if let Some((from, depart, cargo)) = self.last_departure.remove(&c.train) {
            if from == c.station {
                self.stats.self_edges_refused += 1;
            } else {
                self.line_edge(at, c, from, depart, cargo);
            }
        }
        if !c.simulated {
            match c.cargo_from {
                crate::obs::CargoFrom::Approach => self.stats.arrivals_approach += 1,
                crate::obs::CargoFrom::Departure => self.stats.arrivals_departure += 1,
                crate::obs::CargoFrom::Arrival => self.stats.arrivals_late += 1,
            }
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
        let Some(span) = dist.sim.as_ref() else {
            return;
        };
        let d = c.district;
        let glass = span.glass;
        if !rule
            .ports
            .iter()
            .filter_map(|i| dist.inventories.get(i))
            .any(|i| !i.destroyed && !i.doomed && span.invs.contains(&i.decl.index))
        {
            if rule.unload.is_empty() && rule.load.is_empty() {
                return;
            }
            self.stats.exchanges_portless += 1;
            if self.log_enabled {
                let what = format!(
                    "station exchange with train {} skipped: the station's ports are not known",
                    c.train.0
                );
                self.note(d, at, what);
            }
            return;
        }
        // A black-box replay's change so far lands in its inventories first, as a reconcile
        // would place it, so the ports hold what the replay made (choice SG35).
        if !glass {
            let of: BTreeSet<Good> = rule.unload.union(&rule.load).copied().collect();
            self.fold_replay(h, d, at, &of);
        }
        // The station's own port inventories, still standing and in the replay (choice SG33):
        // never the district's whole stock, which its factories draw on.
        let dist = &self.districts[&d];
        let Some(span) = dist.sim.as_ref() else {
            return;
        };
        let ports: Vec<reconcile::InvState> = rule
            .ports
            .iter()
            .filter_map(|i| dist.inventories.get(i))
            .filter(|i| !i.destroyed && !i.doomed && span.invs.contains(&i.decl.index))
            .map(Inventory::state)
            .collect();
        let carriages = c.carriages.clone();
        let goods = &self.goods;
        let booked = self.clusters.apply(h, at, |cl, at| {
            exchange_on_ports(cl, at, d, glass, &rule, &ports, &carriages, goods)
        });
        let Some((entries, bookings)) = booked else {
            return;
        };
        // Bookings outside a glass store pool are the inventories' own: held, counted in their
        // contents and sent at the reconcile. A black-box span's base moves with its stock pool,
        // so the reconcile's net leaves them out.
        if let Some(dist) = self.districts.get_mut(&d) {
            for &(inv, g, n, store) in &bookings {
                if store {
                    continue;
                }
                if let Some(i) = dist.inventories.get_mut(&inv) {
                    i.held.add(g, n);
                }
                if !glass && let Some(span) = dist.sim.as_mut() {
                    span.base.add(g, n);
                }
            }
        }
        self.after_cluster(h, at);
        self.glass_touch(d, at);
        if entries.is_empty() {
            return;
        }
        self.stats.exchanges += 1;
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
                ports: bookings,
            },
        );
        self.out.push(cmd);
    }

    /// Java answered an industry command failed (COMMAND_RESULT failed, docs/industry-boundary.md
    /// §4.3): a station exchange or a train's loot moved nothing, so it is taken back as a result
    /// with nothing applied. Returns whether the engine was waiting on the op. Deposit and
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
        if !ex.ports.is_empty() {
            self.unbook_ports(at, &ex, &fix);
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
            self.glass_touch(d, at);
            if let Some(short) = short
                && let Some(dist) = self.districts.get_mut(&d)
            {
                dist.owed.merge(&short);
            }
        } else if let Some(dist) = self.districts.get_mut(&d) {
            dist.owed.merge(&fix);
        }
    }

    /// Places a black-box replay's change so far in `of` over the inventories it covers, by the
    /// reconcile's own plan, as their `held` (choice SG35), and moves the span's base with it. A
    /// station call then reads the ports as the replay left them: without it, a port's `held` from
    /// earlier calls grows without bound while the replay's change waits for the reconcile, and
    /// the two never meet inside the port's room. What the plan cannot place stays in the net.
    fn fold_replay(&mut self, h: RegionId<Cluster>, d: DistrictId, at: Tick, of: &BTreeSet<Good>) {
        let keys: Vec<Good> = of
            .iter()
            .copied()
            .filter(|g| {
                matches!(
                    self.goods.get(g).map(|i| i.class),
                    Some(GoodClass::Item | GoodClass::Fluid)
                ) && !self.is_live(*g)
            })
            .collect();
        if keys.is_empty() {
            return;
        }
        let Some(levels) = self.clusters.apply(h, at, |cl, _| {
            keys.iter()
                .filter_map(|&g| cl.level(PoolKey::Stock(d, g)).map(|l| (g, l)))
                .collect::<Vec<_>>()
        }) else {
            return;
        };
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        let Some(span) = dist.sim.as_ref() else {
            return;
        };
        let mut net = GoodVec::new();
        for (g, l) in levels {
            net.add(g, l - span.base.get(g));
        }
        if net.is_empty() {
            return;
        }
        let covered: Vec<reconcile::InvState> = dist
            .inventories
            .values()
            .filter(|i| !i.destroyed && span.invs.contains(&i.decl.index))
            .map(Inventory::state)
            .collect();
        let plan = reconcile::plan(&covered, &net, &self.goods);
        let mut placed = GoodVec::new();
        for p in &plan.deltas {
            if let Some(i) = dist.inventories.get_mut(&p.inventory) {
                i.held.add(p.good, p.delta);
                placed.add(p.good, p.delta);
            }
        }
        if let Some(span) = dist.sim.as_mut() {
            span.base.merge(&placed);
            span.folded.merge(&placed);
        }
    }

    /// Takes back what a station exchange booked and the carriage did not take (choice SG33),
    /// on the same port inventories, newest booking first. `fix` is into the district.
    fn unbook_ports(&mut self, at: Tick, ex: &PendingExchange, fix: &GoodVec) {
        let d = ex.district;
        let mut owed = GoodVec::new();
        let mut back: Vec<PortBooking> = Vec::new();
        for (g, f) in fix.iter() {
            let mut left = f;
            for &(inv, bg, n, store) in ex.ports.iter().rev() {
                if bg != g || left == 0 || n.signum() == left.signum() {
                    continue;
                }
                let r = if left > 0 { left.min(-n) } else { left.max(-n) };
                back.push((inv, g, r, store));
                left -= r;
            }
            owed.add(g, left);
        }
        let glass = self
            .districts
            .get(&d)
            .and_then(|x| x.sim.as_ref())
            .map(|s| s.glass);
        match (self.cluster_of.get(&d).copied(), glass) {
            (Some(h), Some(glass)) => {
                let done = self.clusters.apply(h, at, |cl, at| {
                    back.iter()
                        .map(|&(inv, g, r, store)| {
                            if store && cl.pool(PoolKey::Store(d, inv, g)).is_some() {
                                (
                                    cl.add_to_pool(PoolKey::Store(d, inv, g), r, false, at),
                                    true,
                                )
                            } else if glass {
                                (r, false)
                            } else {
                                (cl.add_to_pool(PoolKey::Stock(d, g), r, false, at), false)
                            }
                        })
                        .collect::<Vec<_>>()
                });
                self.after_cluster(h, at);
                self.glass_touch(d, at);
                let done = done.unwrap_or_default();
                if let Some(dist) = self.districts.get_mut(&d) {
                    for (k, &(inv, g, r, _)) in back.iter().enumerate() {
                        let (got, store) = done.get(k).copied().unwrap_or((0, false));
                        owed.add(g, r - got);
                        if store {
                            continue;
                        }
                        if let Some(i) = dist.inventories.get_mut(&inv) {
                            i.held.add(g, got);
                        }
                        if !glass && let Some(span) = dist.sim.as_mut() {
                            span.base.add(g, got);
                        }
                    }
                }
            }
            // The replay is over: the next reconcile places it.
            _ => {
                for &(_, g, r, _) in &back {
                    owed.add(g, r);
                }
            }
        }
        if !owed.is_empty()
            && let Some(dist) = self.districts.get_mut(&d)
        {
            dist.owed.merge(&owed);
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
                    rule.learn_amount(g, n);
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
            let topo = self.topo.clone();
            self.legs.book_on(
                topo.of(from.dim as u16),
                Leg {
                    train: dep.train,
                    from: dep.station,
                    to: dep.destination,
                    depart: at,
                    eta: at + dep.eta_ticks.clamp(1, threat::LEG_MAX_TICKS),
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
        // Into or out of a judging unit's store: a port for its window, loaded or not (an away
        // unit's window is gone already).
        if let Some(revision) = self.districts.get(&d).map(|x| x.revision) {
            let signed: GoodVec = p.goods.iter().map(|(g, n)| (g, sign * n)).collect();
            self.glass.port(d, revision, p.inventory, &signed);
        }
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
        let glass_span = dist.sim.as_ref().is_some_and(|s| s.glass);
        let h = self.cluster_of[&d];
        let goods = p.goods.clone();
        let out = p.direction == Direction::Out;
        let inv = p.inventory;
        let address = p.address;
        // A glass replay books the world's own change on the store it landed in, as already
        // written (choice G2-14): its reconcile writes each store's change, not a stock's.
        if glass_span {
            let covered = self
                .glass
                .districts
                .get(&d)
                .and_then(|g| g.replay.as_ref())
                .is_some_and(|r| r.covers(inv));
            self.clusters.apply(h, at, |c, at| {
                for (g, n) in goods.iter() {
                    let key = if covered {
                        PoolKey::Store(d, inv, g)
                    } else {
                        PoolKey::Stock(d, g)
                    };
                    c.rebase(key, sign * n, at);
                }
            });
            self.after_cluster(h, at);
            self.glass_touch(d, at);
            return;
        }
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
        self.glass_touch(d, at);
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
        // Natural mobs never burrow: the surveyed classes only.
        for c in ThreatClass::SURVEYED_ALL {
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

    // ---- reconcile ahead of arrival (the stale-tick race) ----------------------------------

    /// A player is closing in on district `d` (IND_APPROACH). Ben's answer to the stale-tick race
    /// (INDUSTRY.md §19 risk 3): its reconcile goes out before its chunks tick. The replay stops
    /// now and every chunk's command is sent at once; Java holds each until its chunk loads and
    /// applies it at the chunk's load, before any machine there ticks, so no machine runs on
    /// stock the replay already counted. The replay does not start again while the player keeps
    /// coming (each closing record extends the hold), and goes on from the reconciled state if
    /// the player turns away. Never better than reality: the district makes nothing in between.
    fn approach(&mut self, d: DistrictId, at: Tick) {
        let until = at + self.tun.replay.approach_hold_ticks.max(1);
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        let was_held = dist.hold_until.is_some_and(|h| h > at);
        dist.hold_until = Some(until);
        dist.hold_stamp = dist.hold_stamp.wrapping_add(1);
        let stamp = dist.hold_stamp;
        let live = dist.presence == Presence::Live;
        self.wake_seq += 1;
        self.wakes
            .insert((until, self.wake_seq), Wake::Hold(d, stamp));
        if live {
            return;
        }
        if !was_held {
            self.stats.reconciles_ahead += 1;
            self.note(
                d,
                at,
                "a player is on the way: reconcile ahead of arrival".to_string(),
            );
            self.reconcile_round(d, at, Round::Ahead);
        }
        // Commands of earlier rounds still waiting for their chunks go now too, and on every
        // closing record those sent before a restart (Java held them in memory only, R4).
        let chunks: Vec<ChunkPos> = self
            .districts
            .get(&d)
            .map(|x| x.pending.keys().copied().collect())
            .unwrap_or_default();
        for c in chunks {
            self.send_unsent(d, c);
        }
    }

    /// Whether good `g` is level-wide pollution, applied live while its district is unloaded (I14).
    fn is_live(&self, g: Good) -> bool {
        self.goods.get(&g).is_some_and(|i| {
            deposit::writeback(crate::obs::DepositPluginId::DestroyCrude, Some(i))
                == Writeback::Live
        })
    }

    /// Pushes the level-wide pollution district `d`'s replay made since the last push to the level
    /// (I14: "level-wide types apply live while unloaded"), as IND_DEPOSIT_DELTA, which Java
    /// applies to the level at once, its chunk loaded or not. A stale wake (the replay that booked
    /// it has ended) does nothing.
    fn pollution_wake(&mut self, at: Tick, d: DistrictId, started: Tick) {
        let Some(dist) = self.districts.get(&d) else {
            return;
        };
        if dist.sim.as_ref().is_none_or(|s| s.started != started) {
            return;
        }
        let Some(&h) = self.cluster_of.get(&d) else {
            return;
        };
        let live: Vec<(Good, crate::obs::DepositPluginId, ChunkPos)> = dist
            .deposits
            .values()
            .filter(|s| self.is_live(s.good))
            .map(|s| (s.good, s.plugin, s.chunk))
            .collect();
        let pushed = self
            .clusters
            .apply(h, at, |c, _| {
                live.iter()
                    .map(|&(g, _, _)| c.take_unpushed(PoolKey::Stock(d, g)))
                    .collect::<Vec<i64>>()
            })
            .unwrap_or_default();
        self.after_cluster(h, at);
        for (&(g, plugin, chunk), &delta) in live.iter().zip(&pushed) {
            if delta == 0 {
                continue;
            }
            if let Some(s) = self
                .districts
                .get_mut(&d)
                .and_then(|x| x.deposits.get_mut(&g))
            {
                s.remaining += delta;
            }
            let mut c = Cmd::DepositDelta(DepositDelta {
                op: OpId(0),
                plugin,
                chunk,
                good: g,
                delta,
            });
            c.set_op(self.ops.derive(self.world_seed, at, &c));
            self.out.push(c);
            self.stats.pollution_pushes += 1;
        }
        self.wake_seq += 1;
        let next = at + self.tun.replay.live_pollution_ticks.max(1);
        self.wakes
            .insert((next, self.wake_seq), Wake::Pollution(d, started));
    }

    /// An approach hold ended with no newer closing record: the player did not come, and the
    /// replay goes on over whatever is still unloaded, from the reconciled state.
    fn hold_wake(&mut self, at: Tick, (d, stamp): (DistrictId, u32)) {
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        if dist.hold_stamp != stamp {
            return;
        }
        dist.hold_until = None;
        if dist.presence != Presence::Live {
            self.note(
                d,
                at,
                "the player did not come: the replay goes on".to_string(),
            );
        }
        self.sync_coverage(d, at);
    }

    /// Every command for `chunk` not sent in this session (never, or before a restart).
    fn send_unsent(&mut self, d: DistrictId, chunk: ChunkPos) {
        let Some(dist) = self.districts.get_mut(&d) else {
            return;
        };
        if let Some(list) = dist.pending.get_mut(&chunk) {
            for p in list.iter_mut() {
                if !self.sent_this_session.insert(p.cmd.op) {
                    continue;
                }
                p.sent = true;
                self.stats.chunk_commands += 1;
                self.out.push(Cmd::ChunkReconcile(p.cmd.clone()));
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
        self.attack_district(&t, &[], 0, false);
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
        self.threat_drinking(t, &[], 0);
    }

    /// An attack that also drinks (OILGATE.md §4.3, OG-5): on a district, before anything else,
    /// the survivors take the `fuels` goods in order, up to `room` units in all (scaled by the
    /// surviving share), from the district's boundary stock. What they take is theft like any
    /// other: it leaves the stock and enters `books.stolen` and the attack's report.
    pub fn threat_drinking(&mut self, t: &Threat, fuels: &[Good], room: i64) {
        self.threat_with(t, fuels, room, false);
    }

    /// An attack whose attackers the players also meet in person (P28 as Ben decided it, "bounded
    /// damage too"; choices-oilgate.md OGC56): a watched district resolves abstractly as an
    /// unwatched one does, blocks and theft under the same caps and cooldown, while the real
    /// fight goes on. Otherwise [`Engine::threat_drinking`].
    pub fn threat_in_sight(&mut self, t: &Threat, fuels: &[Good], room: i64) {
        self.threat_with(t, fuels, room, true);
    }

    fn threat_with(&mut self, t: &Threat, fuels: &[Good], room: i64, in_sight: bool) {
        let at = t.at.max(self.now);
        self.advance_to(at);
        let t = Threat { at, ..*t };
        match t.target {
            ThreatTarget::District(_) => self.attack_district(&t, fuels, room, in_sight),
            ThreatTarget::Train(train) => self.attack_train(&t, train),
        }
    }

    fn attack_district(&mut self, t: &Threat, fuels: &[Good], room: i64, in_sight: bool) {
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
        // chunks are loaded (C18); so does a watched one whose attackers come in person too
        // (`in_sight`, OGC56).
        let watched = dist.watched;
        if watched && !in_sight {
            report.skipped = Some("watched");
        } else if at < dist.defence.cooldown_until[t.class.index()] {
            report.skipped = Some("cooldown");
        }
        if let Some(why) = report.skipped {
            self.stats.attacks_skipped += 1;
            let what = format!("{:?} {:?} x{} skipped: {why}", t.source, t.class, t.count);
            self.note(d, at, what);
            if let Some(p) = self.place(d, at) {
                // Nothing happened: a minor row with no object, as an attack that took nothing.
                self.tape.event_as(
                    kinds::INDUSTRY_ATTACK,
                    Severity::Minor,
                    p,
                    u128::from(d.0),
                    0,
                    0,
                );
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
        let mut dmg = threat::damage(t, &outcome, &items, &dist.defence, &tun);
        // Drinking (OG-5): fuel first, up to the surviving share of the room, all of a good's stock
        // if the room allows (choices-oilgate.md OGC8: not held to the item theft cap).
        if dmg.survivors > 0 && room > 0 {
            let mut left =
                (i128::from(room) * i128::from(dmg.survivors) / i128::from(t.count.max(1))) as i64;
            for &g in fuels {
                if left <= 0 {
                    break;
                }
                let n = (stock.get(g) - dmg.stolen.get(g)).max(0).min(left);
                if n > 0 {
                    dmg.stolen.add(g, n);
                    left -= n;
                }
            }
        }
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
                if !dmg.machine_lost {
                    self.glass_touch(d, at);
                }
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
            }
        }
        // Chunk-loaded with nobody near: what lands in loaded chunks is applied at once, as a
        // reconcile round now (§13.1); what lands in unloaded ones waits for their loads. While
        // part of the district is replayed (I7), the round takes only the loaded chunks and the
        // replay goes on for the rest.
        let dist = self.districts.get_mut(&d).expect("exists");
        if dist.presence == Presence::Live {
            if !dist.destroy_pending.is_empty() || !dist.owed.is_empty() {
                self.reconcile_round(d, at, Round::Full);
            }
        } else if !dist.loaded_chunks.is_empty() {
            let mut hit: BTreeSet<ChunkPos> = dist
                .destroy_pending
                .keys()
                .filter(|c| dist.loaded_chunks.contains(c))
                .copied()
                .collect();
            if h.is_none() && !take.is_empty() {
                hit.extend(dist.loaded_chunks.iter().copied());
            }
            if !hit.is_empty() {
                self.reconcile_round(d, at, Round::Part(hit));
                self.start_sim(d, at);
            }
        }
        let what = format!(
            "{:?} {:?} x{}{}: survived {:.2}{}{}, stole {}, destroyed {}{}",
            t.source,
            t.class,
            t.count,
            if watched {
                " (watched, a real fight too)"
            } else {
                ""
            },
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
        if let Some(p) = self.place(d, at) {
            // The threat's id is a hash of the attack, not an entity: no object. An attack that
            // took nothing (the defenders held, or there was nothing to carry) is minor.
            let stolen = item_count(&self.goods, &dmg.stolen);
            let blocks = dmg.destroyed.len();
            let (source, class, count, survivors) = (t.source, t.class, t.count, dmg.survivors);
            let severity = if stolen > 0 || blocks > 0 {
                Severity::Major
            } else {
                Severity::Minor
            };
            let goods = &self.goods;
            self.tape
                .event_as(
                    kinds::INDUSTRY_ATTACK,
                    severity,
                    p,
                    u128::from(d.0),
                    0,
                    stolen,
                )
                .text(|| {
                    format!(
                        "{source:?} {class:?} x{count}: stole {}, destroyed {blocks} blocks, \
                         {survivors} survived",
                        counts(goods, &dmg.stolen)
                    )
                });
            if dmg.machine_lost {
                let machines = dmg
                    .destroyed
                    .iter()
                    .filter(|b| b.flags & BlockFlags::MACHINE != 0)
                    .count();
                self.tape
                    .event(
                        kinds::INDUSTRY_DAMAGED,
                        p,
                        u128::from(d.0),
                        0,
                        machines as i64,
                    )
                    .text(|| format!("{machines} machine blocks lost of {blocks} destroyed"));
            }
        }
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
            if let Some(p) = self.train_spot(train, t.at) {
                self.tape.event_as(
                    kinds::INDUSTRY_ATTACK,
                    Severity::Minor,
                    p,
                    u128::from(train.0),
                    0,
                    0,
                );
            }
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
        if loot.is_empty() {
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
        if let Some(p) = self.leg_spot(&leg, t.at) {
            let items = item_count(&self.goods, &loot);
            let (source, class, count) = (t.source, t.class, t.count);
            let goods = &self.goods;
            self.tape
                .event(kinds::INDUSTRY_ATTACK, p, u128::from(train.0), 0, items)
                .text(|| {
                    format!(
                        "train ambushed ({source:?} {class:?} x{count}): looted {}",
                        counts(goods, &loot)
                    )
                });
        }
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
                ports: Vec::new(),
            },
        );
        self.out.push(cmd);
    }

    // ---- inspection (tests, the CLI; never a player, I18) -------------------------------------

    /// Where a district stands and what its replay has done so far. A read: it projects the replay
    /// without committing anything (the kernel's read rule) and never fires what is due. For tests,
    /// the CLI and an operator's log only: under I18 no player sees simulated state, so nothing
    /// here reaches the Java side.
    pub fn inspect(&mut self, d: DistrictId, at: Tick) -> Option<Inspection> {
        let at = at.min(self.now);
        self.districts.get(&d)?;
        let view = self
            .cluster_of
            .get(&d)
            .and_then(|&h| self.clusters.read(h, at));
        self.inspection(d, at, view)
    }

    /// [`Engine::inspect`] through `&self`, for a viewer (VIEWER.md §5.3): the replay is peeked,
    /// so `touched` and the statistics stay as they are. The cluster model has no slot period,
    /// so the answer is the same as `inspect`'s.
    pub fn inspect_peek(&self, d: DistrictId, at: Tick) -> Option<Inspection> {
        let at = at.min(self.now);
        self.districts.get(&d)?;
        let view = self
            .cluster_of
            .get(&d)
            .and_then(|&h| self.clusters.peek(h, at))
            .map(|p| p.view);
        self.inspection(d, at, view)
    }

    fn inspection(&self, d: DistrictId, at: Tick, view: Option<ClusterView>) -> Option<Inspection> {
        let season = self.season_at(at);
        let dist = self.districts.get(&d)?;
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
            (Presence::Live, _) => (DistrictState::Live, None),
            (Presence::Partial, _) => (DistrictState::Suspended, None),
            (_, Phase::Damaged) => (DistrictState::Damaged, dist.freeze_reason(season)),
            (_, Phase::Learning) => (DistrictState::Learning, Some("learning")),
            (_, Phase::Stale) => (DistrictState::Stale, dist.freeze_reason(season)),
            (_, Phase::NotLearnable(_)) => {
                (DistrictState::NotLearnable, dist.freeze_reason(season))
            }
            (_, Phase::Learned) if view.is_none() => {
                (DistrictState::OutOfSeason, dist.freeze_reason(season))
            }
            (_, Phase::Learned) => (DistrictState::Running, None),
        };
        let mut made_since = GoodVec::new();
        let mut running = None;
        let mut stop = None;
        if let (Some(v), Some(span)) = (&view, &dist.sim) {
            let now = stock_of(v, d);
            made_since = now.minus(&span.base);
            made_since.merge(&span.folded);
            let runs: Vec<Run> = v
                .runs
                .iter()
                .filter(|r| r.0 == d && r.1 < crate::district::LINK_PROC_BASE)
                .map(|r| r.2)
                .collect();
            running = (!runs.is_empty()).then(|| runs.contains(&Run::Running));
            if let Some(Run::Stopped(why)) = runs.iter().find(|r| **r != Run::Running) {
                stop = Some(stop_words(*why, &self.goods));
            }
        }
        Some(Inspection {
            district: d,
            state,
            reason: reason.map(str::to_string),
            rates_per_hour,
            since: dist.sim.as_ref().map(|s| s.started),
            made_since,
            running,
            stop,
            learning_ticks_to_go: if dist.phase == Phase::Learning {
                dist.learner.ticks_to_go(at, &self.tun.learn)
            } else {
                None
            },
            last_reconcile: dist.reports.back().cloned(),
            attacks: dist.attacks.iter().cloned().collect(),
        })
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

    /// [`Engine::sim_stock`] through `&self`, for a viewer: peeks the replay (VIEWER.md §5.3).
    pub fn stock_peek(&self, d: DistrictId) -> Option<GoodVec> {
        let h = *self.cluster_of.get(&d)?;
        let v = self.clusters.peek(h, self.now)?.view;
        Some(stock_of(&v, d))
    }

    /// Every station the engine knows, with where it is (a viewer's map; VIEWER.md §7).
    pub fn stations(&self) -> impl Iterator<Item = &StationDecl> {
        self.stations.values().map(|s| &s.decl)
    }

    /// The districts that share a replay cluster with `d` (`d` included), if it is simulated.
    pub fn cluster_members(&self, d: DistrictId) -> Option<BTreeSet<DistrictId>> {
        let h = *self.cluster_of.get(&d)?;
        Some(self.clusters.get(h)?.cell().model().members())
    }

    /// The district's replay run state, if simulated: running if any of its processes runs.
    pub fn sim_run(&self, d: DistrictId) -> Option<Run> {
        let h = *self.cluster_of.get(&d)?;
        self.clusters.get(h)?.cell().model().box_run(d)
    }

    /// Each of the district's replay processes (I6) with its goods and run state, if simulated.
    pub fn sim_processes(&self, d: DistrictId) -> Vec<(u16, Vec<Good>, Run)> {
        let Some(h) = self.cluster_of.get(&d) else {
            return Vec::new();
        };
        let Some(r) = self.clusters.get(*h) else {
            return Vec::new();
        };
        let c = r.cell().model();
        c.procs()
            .iter()
            .filter(|p| p.owner == d && p.id < crate::district::LINK_PROC_BASE)
            .map(|p| {
                let goods = match &p.body {
                    crate::cluster::Body::Box { terms } => terms
                        .iter()
                        .map(|t| c.pools()[t.pool as usize].key.good())
                        .collect(),
                    crate::cluster::Body::Link { .. } => Vec::new(),
                    crate::cluster::Body::Cycle(cy) => cy
                        .pools
                        .iter()
                        .map(|&i| c.pools()[i as usize].key.good())
                        .collect(),
                };
                (p.id, goods, p.run)
            })
            .collect()
    }

    /// Every replay process of a simulated district at the engine's time, through `&self`, for a
    /// viewer (VIEWER.md §5.3): the replay is peeked, never settled. Box processes come first (ids
    /// from 0), then links (a packager filling a postbox, ids from `LINK_PROC_BASE`). `None` when
    /// the district is not replaying.
    pub fn processes_peek(&self, d: DistrictId) -> Option<Vec<ProcessPeek>> {
        let h = *self.cluster_of.get(&d)?;
        let view = self.clusters.peek(h, self.now)?.view;
        let c = self.clusters.get(h)?.cell().model();
        let key = |i: u16| c.pools()[i as usize].key;
        Some(
            c.procs()
                .iter()
                .filter(|p| p.owner == d)
                .map(|p| {
                    let (goods, fills) = match &p.body {
                        crate::cluster::Body::Box { terms } => {
                            (terms.iter().map(|t| key(t.pool).good()).collect(), None)
                        }
                        crate::cluster::Body::Link { from, to, .. } => {
                            (vec![key(*from).good()], Some(key(*to)))
                        }
                        // A glass replay's closed-form cycle (G2-13): every pool its pieces move.
                        crate::cluster::Body::Cycle(cy) => {
                            (cy.pools.iter().map(|&i| key(i).good()).collect(), None)
                        }
                    };
                    let run = view.run_of(d, p.id).unwrap_or(p.run);
                    ProcessPeek {
                        id: p.id,
                        goods,
                        fills,
                        running: run == Run::Running,
                        stop: match run {
                            Run::Running => None,
                            Run::Stopped(why) => Some(stop_words(why, &self.goods)),
                        },
                        requires: p.requires,
                    }
                })
                .collect(),
        )
    }
}

/// One replay process as a viewer sees it ([`Engine::processes_peek`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProcessPeek {
    pub id: u16,
    /// The goods it touches: a box process's inputs and outputs, a link's one good.
    pub goods: Vec<Good>,
    /// For a link, the pool it fills (a postbox's mail); `None` for a box process.
    pub fills: Option<PoolKey>,
    pub running: bool,
    /// Why it is stopped, in words; `None` while it runs.
    pub stop: Option<String>,
    /// The district whose box processes must all run for this one to run (a PowerGrid source,
    /// I13).
    pub requires: Option<DistrictId>,
}

fn stock_of(v: &ClusterView, d: DistrictId) -> GoodVec {
    v.levels
        .iter()
        .filter_map(|&(k, l)| match k {
            PoolKey::Stock(o, g) | PoolKey::Store(o, _, g) if o == d => Some((g, l)),
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

/// A simulated station call on the station's own port inventories (choices SG33, SG34): unloads
/// go into the ports' room, the ports that received the good while learning first; loads come out
/// of what the ports hold, at most the learned amount per call. A black-box span moves the
/// district's stock pool as well (its processes see the goods come and go); a glass span moves a
/// port's store pool where it has one for the good, and otherwise only the port's own contents.
/// A black-box caller has folded the replay's change so far into the ports first (SG35), so
/// their contents are what the replay made. Returns the carriage entries (carriage, good, delta into the carriage) and the bookings (inventory, good,
/// delta into it, on its store pool).
#[allow(clippy::too_many_arguments)]
fn exchange_on_ports(
    cl: &mut Cluster,
    at: Tick,
    d: DistrictId,
    glass: bool,
    rule: &crate::district::ExchangeRule,
    ports: &[reconcile::InvState],
    carriages: &[Carriage],
    goods: &BTreeMap<Good, GoodInfo>,
) -> (Vec<(u32, Good, i64)>, Vec<PortBooking>) {
    let mut entries = Vec::new();
    let mut bookings = Vec::new();
    let mut cargo: Vec<GoodVec> = carriages.iter().map(|c| c.goods.clone()).collect();
    // The ports' contents as this call leaves them.
    let mut contents: Vec<GoodVec> = ports.iter().map(|p| p.contents.clone()).collect();
    // A glass span's store pool of the port and good, if it has one: its level is the port's.
    let store = |cl: &Cluster, inv: u32, g: Good| {
        if glass {
            cl.level(PoolKey::Store(d, inv, g))
        } else {
            None
        }
    };
    let book = |cl: &mut Cluster, inv: u32, g: Good, n: i64, on_store: bool| -> i64 {
        if on_store {
            cl.add_to_pool(PoolKey::Store(d, inv, g), n, false, at)
        } else if glass {
            n
        } else {
            cl.add_to_pool(PoolKey::Stock(d, g), n, false, at)
        }
    };
    for &g in &rule.unload {
        let carried: i64 = cargo.iter().map(|v| v.get(g).max(0)).sum();
        if carried <= 0 {
            continue;
        }
        let receivers = ports.iter().any(|p| p.received.contains(&g));
        let mut placed = 0i64;
        for (k, p) in ports.iter().enumerate() {
            let want = carried - placed;
            if want <= 0 {
                break;
            }
            if receivers && !p.received.contains(&g) {
                continue;
            }
            let level = store(cl, p.index, g);
            let mut now = contents[k].clone();
            if let Some(l) = level {
                now.set(g, l);
            }
            let n = want.min(reconcile::room_for(p, &now, g, goods));
            if n <= 0 {
                continue;
            }
            let got = book(cl, p.index, g, n, level.is_some());
            if got > 0 {
                contents[k].add(g, got);
                bookings.push((p.index, g, got, level.is_some()));
                placed += got;
            }
        }
        let mut left = placed;
        for (ci, car) in carriages.iter().enumerate() {
            let n = cargo[ci].get(g).max(0).min(left);
            if n > 0 {
                cargo[ci].add(g, -n);
                entries.push((car.index, g, -n));
                left -= n;
            }
        }
    }
    for &g in &rule.load {
        let room: i64 = carriages
            .iter()
            .enumerate()
            .map(|(ci, car)| carriage_room(car, &cargo[ci], g, goods))
            .sum();
        let mut want = rule.load_cap(g).min(room);
        let mut taken = 0i64;
        for (k, p) in ports.iter().enumerate() {
            if want <= 0 {
                break;
            }
            let level = store(cl, p.index, g);
            let have = level.unwrap_or_else(|| contents[k].get(g));
            let n = want.min(have);
            if n <= 0 {
                continue;
            }
            let got = -book(cl, p.index, g, -n, level.is_some());
            if got > 0 {
                contents[k].add(g, -got);
                bookings.push((p.index, g, -got, level.is_some()));
                taken += got;
                want -= got;
            }
        }
        let mut left = taken;
        for (ci, car) in carriages.iter().enumerate() {
            let n = carriage_room(car, &cargo[ci], g, goods).min(left);
            if n > 0 {
                cargo[ci].add(g, n);
                entries.push((car.index, g, n));
                left -= n;
            }
        }
    }
    (entries, bookings)
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
        Stop::Bound(_, tag) => format!("stopped: store bound {tag}"),
        Stop::Timer => "stopped: regime timer".to_string(),
    }
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};
    use rtsim_kernel::lazy::Lazy;
    use rtsim_kernel::page::PageKey;

    use super::*;
    use crate::persist::{
        CLUSTER_TABLE, DISTRICT_TABLE, GLASS_TABLE, GLOBAL_TABLE, LEG_TABLE, Persist, from_bytes,
        persist_struct, to_bytes,
    };

    persist_struct!(LineEdge {
        trips,
        cargo,
        travel_ticks,
        first,
        last
    });
    // The counters a run keeps for its own readout (`unwatched_stops`, `self_edges_refused`, choice
    // SG58) are not in the page: adding them would change the globals layout, and a save written
    // before them would no longer read. A restore starts them at 0, like the session's own counts
    // in the host.
    impl Persist for Stats {
        fn put(&self, w: &mut Writer) {
            self.observations.put(w);
            self.models_adopted.put(w);
            self.sims_started.put(w);
            self.reconciles.put(w);
            self.chunk_commands.put(w);
            self.duplicate_results.put(w);
            self.trims.put(w);
            self.exchanges.put(w);
            self.attacks.put(w);
            self.attacks_skipped.put(w);
            self.packages_written.put(w);
            self.breakpoints.put(w);
            self.cluster_settles.put(w);
            self.reconciles_ahead.put(w);
            self.pollution_pushes.put(w);
            self.arrivals_approach.put(w);
            self.arrivals_departure.put(w);
            self.arrivals_late.put(w);
            self.exchanges_portless.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Self {
                observations: Persist::get(r)?,
                models_adopted: Persist::get(r)?,
                sims_started: Persist::get(r)?,
                reconciles: Persist::get(r)?,
                chunk_commands: Persist::get(r)?,
                duplicate_results: Persist::get(r)?,
                trims: Persist::get(r)?,
                exchanges: Persist::get(r)?,
                attacks: Persist::get(r)?,
                attacks_skipped: Persist::get(r)?,
                packages_written: Persist::get(r)?,
                breakpoints: Persist::get(r)?,
                cluster_settles: Persist::get(r)?,
                reconciles_ahead: Persist::get(r)?,
                pollution_pushes: Persist::get(r)?,
                arrivals_approach: Persist::get(r)?,
                arrivals_departure: Persist::get(r)?,
                arrivals_late: Persist::get(r)?,
                exchanges_portless: Persist::get(r)?,
                unwatched_stops: 0,
                self_edges_refused: 0,
            })
        }
    }
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
    impl Persist for Wake {
        fn put(&self, w: &mut Writer) {
            match *self {
                Wake::Hold(d, stamp) => {
                    w.u8(0);
                    d.put(w);
                    stamp.put(w);
                }
                Wake::Pollution(d, started) => {
                    w.u8(1);
                    d.put(w);
                    started.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Wake::Hold(Persist::get(r)?, Persist::get(r)?),
                1 => Wake::Pollution(Persist::get(r)?, Persist::get(r)?),
                _ => return Err(CodecError::Invalid("industry wake".into())),
            })
        }
    }
    persist_struct!(PendingExchange {
        district,
        train,
        entries,
        loot,
        ports
    });

    /// The globals page starts with this, so a page of another format fails by name, not by whatever
    /// field its bytes happen to decode as (ID10).
    pub const GLOBAL_MAGIC: u32 = u32::from_le_bytes(*b"IGB1");

    fn globals(e: &Engine, w: &mut Writer) {
        w.u32(GLOBAL_MAGIC);
        e.world_seed.put(w);
        e.goods.put(w);
        e.next_cluster.put(w);
        e.stations.put(w);
        e.lines.put(w);
        e.last_departure.put(w);
        e.arrivals.put(w);
        e.legs.next_id().put(w);
        e.legs_swept.put(w);
        e.sinks_of.put(w);
        e.conditions.put(w);
        e.pressure.put(w);
        e.pressure_seq.put(w);
        e.wakes.put(w);
        e.wake_seq.put(w);
        e.dirty.put(w);
        e.per_chunk_unloads.put(w);
        e.exchanges.put(w);
        e.train_cooldown.put(w);
        e.ops.put(w);
        e.out.put(w);
        e.now.put(w);
        e.stats.put(w);
        e.books.put(w);
        e.log_enabled.put(w);
        e.glass.records.put(w);
        e.glass.stats.put(w);
    }

    fn key48(id: u64, what: &str) -> u64 {
        assert!(
            id < 1 << 48,
            "{what} id {id} does not fit a page key (48 bits)"
        );
        id
    }

    fn globals_page(e: &Engine) -> (PageKey, Vec<u8>) {
        let mut w = Writer::new();
        globals(e, &mut w);
        (PageKey::new(GLOBAL_TABLE, 0), w.into_bytes())
    }

    fn district_key(id: DistrictId) -> PageKey {
        PageKey::new(DISTRICT_TABLE, key48(id.0, "district"))
    }

    fn glass_key(id: DistrictId) -> PageKey {
        PageKey::new(GLASS_TABLE, key48(id.0, "district"))
    }

    fn leg_key(page: u64) -> PageKey {
        PageKey::new(LEG_TABLE, key48(page, "leg page"))
    }

    fn cluster_page(region: &rtsim_kernel::lazy::Region<Cluster>) -> (PageKey, Vec<u8>) {
        let mut w = Writer::new();
        region.cell().model().put(&mut w);
        region.cell().anchor().put(&mut w);
        region.wake().put(&mut w);
        (
            PageKey::new(CLUSTER_TABLE, key48(region.key(), "cluster")),
            w.into_bytes(),
        )
    }

    impl Engine {
        /// Every page of the engine's state (A16): globals, one per district, one per replay
        /// cluster with its booked wake, one per district's glass state (G2-9). District ids
        /// must fit 48 bits (F36). A save writes [`Engine::save_changed`]'s pages instead; this
        /// is the whole state, for a fresh store, tests and state hashes.
        pub fn snapshot(&self) -> Vec<(PageKey, Vec<u8>)> {
            let mut out = Vec::with_capacity(
                1 + self.districts.len()
                    + self.clusters.len()
                    + self.glass.districts.len()
                    + self.legs.len() / 128,
            );
            out.push(globals_page(self));
            for (id, d) in self.districts.iter() {
                out.push((district_key(*id), to_bytes(d)));
            }
            for (_, region) in self.clusters.iter() {
                out.push(cluster_page(region));
            }
            for (id, dg) in self.glass.districts.iter() {
                out.push((glass_key(*id), crate::glass::page::district_page(dg)));
            }
            for (page, bytes) in self.legs.pages() {
                out.push((leg_key(page), bytes));
            }
            out
        }

        /// One save's pages (ID2): the globals page, every district, cluster and glass page
        /// changed since the last save, and the pages of those gone since then to delete. The
        /// engine counts them saved from here on; a failed save hands its ticket back to
        /// [`Engine::save_failed`]. Call between steps. With the page check on (ID5), every page
        /// is hashed and a page that changed without a mark is counted (a debug build panics).
        pub fn save_changed(&mut self) -> ChangedPages {
            let mut put = vec![globals_page(self)];
            let mut delete = Vec::new();
            let mut ticket = SaveTicket::default();
            for id in self.districts.take_changed() {
                match self.districts.get(&id) {
                    Some(d) => put.push((district_key(id), to_bytes(d))),
                    None => delete.push(district_key(id)),
                }
                ticket.districts.push(id);
            }
            for (key, h) in self.clusters.take_changed() {
                match self.clusters.live(key, h) {
                    Some(region) => put.push(cluster_page(region)),
                    None => delete.push(PageKey::new(CLUSTER_TABLE, key48(key, "cluster"))),
                }
                ticket.clusters.push((key, h));
            }
            for id in self.glass.districts.take_changed() {
                match self.glass.districts.get(&id) {
                    Some(dg) => put.push((glass_key(id), crate::glass::page::district_page(dg))),
                    None => delete.push(glass_key(id)),
                }
                ticket.glass.push(id);
            }
            for page in self.legs.take_changed() {
                match self.legs.page_bytes(page) {
                    Some(b) => put.push((leg_key(page), b)),
                    None => delete.push(leg_key(page)),
                }
                ticket.legs.push(page);
            }
            if self.page_check.is_some() {
                self.check_pages(&put, &delete);
            }
            ChangedPages {
                put,
                delete,
                ticket,
            }
        }

        /// A save whose pages go nowhere, for the page check (ID5): the fake world's and the
        /// tests' saves, between steps. With the check on, it checks the marks since the save
        /// before and panics on a page that changed without one; with it off, it does nothing.
        pub fn check_save(&mut self) {
            if self.page_check.is_none() {
                return;
            }
            self.save_changed();
            let c = self.page_check.as_deref().expect("on");
            assert!(
                c.unmarked == 0,
                "industry page {:#x} changed since the last save without a mark ({} such pages)",
                c.first_unmarked.map_or(0, |k| k.0),
                c.unmarked
            );
        }

        /// A save that did not reach the disk: its pages are written again by the next (ID3).
        pub fn save_failed(&mut self, t: SaveTicket) {
            for id in t.districts {
                self.districts.mark(id);
            }
            for id in t.glass {
                self.glass.districts.mark(id);
            }
            for (key, h) in t.clusters {
                self.clusters.mark(key, h);
            }
            for page in t.legs {
                self.legs.mark(page);
            }
        }

        /// Pages changed since the last save, waiting for the next one (the globals page not
        /// counted).
        pub fn pages_changed(&self) -> usize {
            self.districts.changed().len()
                + self.clusters.changed().len()
                + self.glass.districts.changed().len()
                + self.legs.changed().len()
        }

        /// What the page check found, if it is on (ID5).
        pub fn page_check(&self) -> Option<&PageCheck> {
            self.page_check.as_deref()
        }

        /// Turns the page check on (from the next save, against this state) or off.
        pub fn set_page_check(&mut self, on: bool) {
            self.page_check = on.then(|| {
                Box::new(PageCheck {
                    hashes: self
                        .snapshot()
                        .into_iter()
                        .map(|(k, b)| (k, page_hash(&b)))
                        .collect(),
                    ..PageCheck::default()
                })
            });
        }

        /// The page check of one save (ID5): every page whose bytes differ from the save before
        /// (or that came or went) must be among those it wrote or deleted.
        fn check_pages(&mut self, put: &[(PageKey, Vec<u8>)], delete: &[PageKey]) {
            let now: BTreeMap<PageKey, u64> = self
                .snapshot()
                .into_iter()
                .map(|(k, b)| (k, page_hash(&b)))
                .collect();
            let check = self.page_check.as_mut().expect("on");
            let covered: BTreeSet<PageKey> = put
                .iter()
                .map(|(k, _)| *k)
                .chain(delete.iter().copied())
                .collect();
            let keys: BTreeSet<PageKey> = now.keys().chain(check.hashes.keys()).copied().collect();
            for k in keys {
                if now.get(&k) != check.hashes.get(&k) && !covered.contains(&k) {
                    check.unmarked += 1;
                    check.first_unmarked.get_or_insert(k);
                }
            }
            for k in &covered {
                if k.table() != GLOBAL_TABLE {
                    check.written += 1;
                    if now.get(k) == check.hashes.get(k) {
                        check.unchanged += 1;
                    }
                }
            }
            check.saves += 1;
            check.hashes = now;
            debug_assert!(
                check.unmarked == 0,
                "industry page {:#x} changed since the last save without a mark ({} such pages)",
                check.first_unmarked.map_or(0, |k| k.0),
                check.unmarked
            );
        }

        /// The engine a [`Engine::snapshot`] describes. Tunables are configuration, not state.
        pub fn restore(
            pages: impl IntoIterator<Item = (PageKey, Vec<u8>)>,
            tun: Tunables,
        ) -> Result<Engine> {
            let mut global = None;
            let mut districts = BTreeMap::new();
            let mut clusters = Vec::new();
            let mut glass = BTreeMap::new();
            let mut glass_dropped = Vec::new();
            let mut leg_pages = Vec::new();
            // The page check's start (ID5): the pages as given, so a restored engine that would
            // save other bytes than it was restored from fails the check at its first save.
            let mut hashes = page_check_on().then(BTreeMap::new);
            for (k, bytes) in pages {
                if let Some(h) = hashes.as_mut()
                    && matches!(
                        k.table(),
                        GLOBAL_TABLE | DISTRICT_TABLE | CLUSTER_TABLE | GLASS_TABLE | LEG_TABLE
                    )
                {
                    h.insert(k, page_hash(&bytes));
                }
                match k.table() {
                    GLOBAL_TABLE => global = Some(bytes),
                    DISTRICT_TABLE => {
                        let d: District = from_bytes(&bytes)?;
                        // A save writes and deletes a district's page at its own key only (ID2):
                        // one elsewhere would never be replaced or deleted.
                        if d.id.0 != k.id() {
                            return Err(CodecError::Invalid(format!(
                                "district {} in the page of district {}",
                                d.id.0,
                                k.id()
                            )));
                        }
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
                    GLASS_TABLE => {
                        match crate::glass::page::read_district_page(
                            &bytes,
                            tun.glass.sections_max,
                            tun.glass.counted_stores_max,
                        )? {
                            crate::glass::page::PageRead::Kept(dg) => {
                                glass.insert(DistrictId(k.id()), *dg);
                            }
                            crate::glass::page::PageRead::Dropped(why) => {
                                glass_dropped.push((DistrictId(k.id()), why));
                            }
                        }
                    }
                    LEG_TABLE => leg_pages.push((k.id(), bytes)),
                    _ => {}
                }
            }
            let g = global.ok_or_else(|| CodecError::Invalid("no globals page".into()))?;
            let mut r = Reader::new(&g);
            let magic = r.u32()?;
            if magic != GLOBAL_MAGIC {
                return Err(CodecError::Invalid(format!(
                    "globals page magic {magic:#010x}, not {GLOBAL_MAGIC:#010x}: an older format"
                )));
            }
            let world_seed = u64::get(&mut r)?;
            let mut e = Engine::new(world_seed, tun, Tick::ZERO);
            e.goods = Persist::get(&mut r)?;
            e.next_cluster = Persist::get(&mut r)?;
            e.stations = Persist::get(&mut r)?;
            e.lines = Persist::get(&mut r)?;
            e.last_departure = Persist::get(&mut r)?;
            e.arrivals = Persist::get(&mut r)?;
            let legs_next = u64::get(&mut r)?;
            e.legs_swept = Persist::get(&mut r)?;
            e.sinks_of = Persist::get(&mut r)?;
            e.conditions = Persist::get(&mut r)?;
            e.pressure = Persist::get(&mut r)?;
            e.pressure_seq = Persist::get(&mut r)?;
            e.wakes = Persist::get(&mut r)?;
            e.wake_seq = Persist::get(&mut r)?;
            e.dirty = Persist::get(&mut r)?;
            e.per_chunk_unloads = Persist::get(&mut r)?;
            e.exchanges = Persist::get(&mut r)?;
            e.train_cooldown = Persist::get(&mut r)?;
            e.ops = Persist::get(&mut r)?;
            e.out = Persist::get(&mut r)?;
            e.now = Persist::get(&mut r)?;
            e.stats = Persist::get(&mut r)?;
            e.books = Persist::get(&mut r)?;
            e.log_enabled = Persist::get(&mut r)?;
            e.glass.records = Persist::get(&mut r)?;
            e.glass.stats = Persist::get(&mut r)?;
            r.finish()?;
            e.legs = LegIndex::from_pages(legs_next, leg_pages)?;
            e.glass.districts = Tracked::restored(glass);
            e.glass.recount_rows();
            // A dropped glass page is deleted at the next save; the district learns again.
            for &(id, _) in &glass_dropped {
                e.glass.districts.mark(id);
            }
            e.glass.dropped_on_restore = glass_dropped;
            e.districts = Tracked::restored(districts);
            e.clusters = TrackedTable::new(world_seed, REPLAY, e.now);
            clusters.sort_by_key(|c| c.0);
            for (key, model, anchor, wake) in clusters {
                let members = model.members();
                let h = e.clusters.restore(key, Lazy::new(model, anchor), wake);
                for m in members {
                    e.cluster_of.insert(m, h);
                }
            }
            // A glass span whose replay came without its glass page (rtsim-view's inspect restores
            // one district alone) goes on by the black box (or freezes) from the next step, its
            // stores' changes carried. With its page it goes on exactly (G2-9).
            let unreplayed: Vec<DistrictId> = e
                .districts
                .values()
                .filter(|d| d.sim.as_ref().is_some_and(|s| s.glass))
                .filter(|d| {
                    e.glass
                        .districts
                        .get(&d.id)
                        .is_none_or(|g| g.replay.is_none() || g.compiled.is_none())
                })
                .map(|d| d.id)
                .collect();
            e.dirty.extend(unreplayed);
            if let (Some(c), Some(hashes)) = (e.page_check.as_mut(), hashes) {
                c.hashes = hashes;
            }
            Ok(e)
        }
    }
}
