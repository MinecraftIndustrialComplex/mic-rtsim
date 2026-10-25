// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine side of the boundary (docs/boundary.md §1.2): one session of buffers, steps and
//! pages.
//!
//! - [`Engine::start`] loads the pages and zeroes the buffers' sequence words.
//! - [`Engine::submit`] queues step h on mic_native's background pool and returns at once.
//! - A step reads observation buffer h & 1 (acquire), stores `consumed = h` (release), applies the
//!   records (journaled ones through the kernel's `Replay`), saves if a save is due, writes command
//!   buffer h & 1 and stores `published = h` (release).
//! - A save snapshots the pages at a step boundary on the step thread and writes them on the
//!   host's IO lane; the acknowledgement (`ackedObsSeq`) only ever names pages already on disk.
//!   The paged modules write only their pages changed since the last save, and a module over its
//!   row budget gets an extra save at a step boundary, kept as a pending generation that a crash
//!   rolls back (`paging`, docs/choices-paging-followups.md PF12 to PF21).
//!
//! At most one step runs at a time (the bridge submits h only once step h - 1 published). The
//! state lock is taken by the step and, outside any step, by `start` and `close`: no game thread
//! ever waits on a lock a running step holds.

use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mic_native_rt::snapshot::SnapshotCell;
use mic_native_rt::{self as rt, Pool};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::journal::{GEN_BLOCK, ObsSeq, OpId, Outbox, Reconcile, Replay, Verdict};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};

use crate::buffers::Buf;
use crate::orders::Order;
use crate::wire::{self, Rec, RecordWriter, Records, cmd, obs};
use crate::world::{Item, Member, World};

/// The industry engine behind the boundary: everything industry lives there, and the step calls
/// it at its observe, simulate, write and save points (docs/industry-boundary.md).
mod industry;
pub use industry::{
    IndustryStats, KEY_LINK as KEY_INDUSTRY_LINK, MODULE_INDUSTRY, TABLES as INDUSTRY_TABLES,
    glass_notes_offline, glass_offline, glass_replay_offline,
};
/// Phase 1 behind the boundary: the phase 1 world (settle, trade, travel, knowledge), its
/// observations, commands and pages (docs/boundary.md §4, §5).
mod phase1;
pub use phase1::{STATS as PHASE1_STATS, TABLE as PHASE1_TABLE};
/// Phase 2's dragons behind the boundary: `rtsim-ecology` fed by the Ice and Fire plugin's
/// records, in the phase 1 world (docs/boundary.md §6).
mod dragons;
pub use dragons::{MODULE_ECOLOGY, TABLE as DRAGONS_TABLE};
/// The ships behind the boundary: `rtsim-ships` fed by a ships plugin's records, in the phase 1
/// world (docs/boundary.md §7).
mod ships;
pub use ships::{MODULE_SHIPS, TABLE as SHIPS_TABLE};
/// The herds behind the boundary: rtsim-ecology's migrating herds fed by the core's vanilla herd
/// adapter, in the phase 1 world (docs/boundary.md §8).
mod herds;
pub use herds::{HERD_INFO, MODULE_HERDS, TABLE as HERDS_TABLE};
/// The conflict behind the boundary: rtsim-conflict's factions, feuds and war bands fed by a
/// conflict plugin's records and the other modules, in the phase 1 world (docs/boundary.md §9).
mod conflict;
pub use conflict::{ConflictLinkStats, MODULE_CONFLICT, TABLE as CONFLICT_TABLE};
// The world model (docs/boundary.md §10; choices X2-# in docs/overnight-choices-worldmodel.md).
mod worldmodel;
pub use worldmodel::{
    HABITAT_INFO, TABLE_META as WORLD_MODEL_TABLE, TABLE_REGIONS as WORLD_MODEL_REGIONS,
};
/// What the modules in the phase 1 world share: one interest area per player, one clock.
mod shared;
pub use shared::PLAYER_STALE;

/// TALK's asks and facts (TALK_ASK, TALK_FACTS; docs/boundary.md §13, TALK.md §9).
mod talk;
pub use talk::Stats as TalkStats;
// Every level's topology (TOPOLOGY records; docs/boundary.md §11).
mod topology;
// The paged modules' saves, tickets and budgets (PF12 to PF21).
mod paging;
pub use paging::DEFAULT_ROW_BUDGET;
/// The event archive (VIEWER.md §10): the modules' tapes, flushed with each save.
mod archive;
pub use archive::{DIR as ARCHIVE_DIR, KEY as ARCHIVE_KEY, dir as archive_dir};
/// The view tap (VIEWER.md §5.2): the layers as parts in the step's slack while a viewer holds
/// the lease, committed into the view store on the IO lane.
pub mod tap;
/// The page layout rtsim-view mirrors in its `keys.rs` (it cannot link this crate); its test
/// (`crates/rtsim-view/tests/harness.rs`) compares every value here with its own.
pub mod mirrored {
    pub use super::archive::KEY as ARCHIVE_SERIAL;
    pub use super::conflict::KEY_CONFLICT;
    pub use super::dragons::KEY_DRAGONS;
    pub use super::herds::KEY_HERDS;
    pub use super::paging::{HASH_MAGIC, hash_key};
    pub use super::ships::KEY_SHIPS;
    pub use super::worldmodel::KEY_META as WORLD_MODEL_META;
    /// Phase 1's pages.
    pub mod phase1 {
        pub use super::super::phase1::{
            CHUNK, KEY_META, META_MAGIC, META_VERSION, PIECE, SNAP_MAGIC, SNAPSHOT_TABLE, TABLE,
            WORLD_META, WORLD_SEED,
        };
    }
}

/// Record bytes per buffer when Java asks for the default. PLACEHOLDER (the stub's 256 KiB): the
/// engine is to size buffers from the per-player puppet caps and per-tick quotas (PLAN.md §4.5).
pub const DEFAULT_CAPACITY: usize = 256 * 1024;

/// The engine's pages: table 1 is this crate's core state.
pub const KEY_META: PageKey = PageKey::new(1, 0);
pub const KEY_WORLD: PageKey = PageKey::new(1, 1);
const META_MAGIC: u32 = u32::from_le_bytes(*b"RTJM");
const META_VERSION: u16 = 2;
/// The journal's closed generations (docs/boundary.md §3.2), [`GEN_BLOCK`] a page from id 0:
/// written once, when a block gains one.
pub const GEN_TABLE: u16 = 2;
/// The generation the engine judges a session's records in when its bridge sends no JOURNAL_GEN:
/// this bit with the session's first sequence.
const UNNAMED_GEN: u64 = 1 << 63;

/// [`Engine::window_grows`]'s reasons: a save Java asked for that failed, and an engine failure.
const NOT_MADE: &str = "a save Java asked for was not made";
const STOPPED: &str =
    "the engine stopped simulating and takes no save for the rest of this session";

fn gen_key(k: usize) -> PageKey {
    PageKey::new(GEN_TABLE, k as u64)
}

/// `nOpen` flags.
pub const FLAG_TEST: u32 = 1;
pub const FLAG_NO_FSYNC: u32 = 2;
/// The industry engine runs read-only (phase I-A, F25): ledger lines only, nothing applied.
pub const FLAG_INDUSTRY_READ_ONLY: u32 = 4;

/// Where steps and saves run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exec {
    /// mic_native's background pool for steps, its IO lane for saves (the real thing).
    Host,
    /// On the calling thread, at once (unit tests without a host).
    Inline,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub pages_dir: PathBuf,
    /// Record bytes per buffer.
    pub capacity: usize,
    /// Keep the received log and the inspection snapshot (GameTests, `/rtsim`).
    pub test: bool,
    pub durability: Durability,
    pub exec: Exec,
    /// The industry engine in read-only mode (F25).
    pub industry_read_only: bool,
    /// The world seed of a new industry engine (tests); None draws one (W5).
    pub industry_seed: Option<u64>,
    /// Row bytes each paged module (herds, dragons, conflict, ships, the phase 1 world's
    /// markets) keeps resident (PF19).
    pub row_budget: usize,
    /// Record events in the archive under the pages folder (VIEWER.md §10; on by default).
    pub archive: bool,
}

impl Config {
    pub fn new(pages_dir: impl Into<PathBuf>) -> Self {
        Config {
            pages_dir: pages_dir.into(),
            capacity: DEFAULT_CAPACITY,
            test: false,
            durability: Durability::Fsync,
            exec: Exec::Host,
            industry_read_only: false,
            industry_seed: None,
            row_budget: DEFAULT_ROW_BUDGET,
            archive: true,
        }
    }
}

/// A transfer into a Minecraft container: the outbox's command (it is resent after a restart).
#[derive(Clone, Debug, PartialEq)]
pub struct TransferCmd {
    pub dimension: String,
    pub item: String,
    pub pos: [i32; 3],
    pub count: i32,
}

impl Encode for TransferCmd {
    fn encode(&self, w: &mut Writer) {
        w.bytes(self.dimension.as_bytes());
        w.bytes(self.item.as_bytes());
        self.pos.iter().for_each(|v| w.var_i64(i64::from(*v)));
        w.var_i64(i64::from(self.count));
    }
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

fn str_of(b: &[u8]) -> codec::Result<String> {
    String::from_utf8(b.to_vec()).map_err(|e| CodecError::Invalid(format!("utf-8: {e}")))
}

impl Decode for TransferCmd {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let dimension = str_of(r.bytes()?)?;
        let item = str_of(r.bytes()?)?;
        let pos = [
            i32_of(r.var_i64()?)?,
            i32_of(r.var_i64()?)?,
            i32_of(r.var_i64()?)?,
        ];
        let count = i32_of(r.var_i64()?)?;
        Ok(TransferCmd {
            dimension,
            item,
            pos,
            count,
        })
    }
}

fn encode_meta(replay: &Replay, outbox: &Outbox<TransferCmd>) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u32(META_MAGIC);
    w.u16(META_VERSION);
    replay.encode(&mut w);
    outbox.encode(&mut w);
    w.into_bytes()
}

fn decode_meta(b: &[u8]) -> codec::Result<(Replay, Outbox<TransferCmd>)> {
    let mut r = Reader::new(b);
    if r.u32()? != META_MAGIC {
        return Err(CodecError::Invalid("not an rtsim-jni meta page".into()));
    }
    let v = r.u16()?;
    if v != META_VERSION {
        return Err(CodecError::Invalid(format!(
            "meta page version {v}, this build reads {META_VERSION}"
        )));
    }
    let replay = Replay::decode(&mut r)?;
    let outbox = Outbox::decode(&mut r)?;
    r.finish()?;
    Ok((replay, outbox))
}

/// One observation record the engine read (TEST mode), for `nDrainReceived`.
#[derive(Clone, Debug)]
pub struct Received {
    pub handover: u64,
    pub obs_seq: u64,
    pub skipped: bool,
    pub bytes: Vec<u8>,
}

/// What a test can ask without a lock (published after every step in TEST mode).
#[derive(Clone, Debug, Default)]
pub struct Inspect {
    pub ledger: std::collections::BTreeMap<u64, i64>,
    pub done_ops: std::collections::BTreeSet<u64>,
    pub replay: Replay,
    pub agents: std::collections::BTreeMap<u64, crate::world::Agent>,
    pub counters: crate::world::WorldCounters,
    /// Every phase 1 village's feelings toward players: village to (UUID most, least, sentiment).
    pub phase1_sentiments: std::collections::BTreeMap<u64, Vec<(u64, u64, i8)>>,
    /// The phase 1 world's villages and bases.
    pub phase1_villages: std::collections::BTreeSet<u64>,
    /// Every phase 1 village's works (`phase1::works_view`).
    pub phase1_works: std::collections::BTreeMap<u64, [i64; phase1::WORKS_VIEW]>,
    /// Every frame's topology as the TOPOLOGY records left it, and the records refused.
    pub topologies: rtsim_space::SharedTopologies,
    pub topology_refused: u64,
}

impl Inspect {
    pub fn op_applied(&self, op: u64) -> bool {
        self.replay.knows_op(OpId(op)) || self.done_ops.contains(&op)
    }
}

/// Counters, readable from any thread (`nStats`). Indexes are the Java side's.
#[derive(Default)]
pub struct Stats {
    pub steps_started: AtomicU64,
    pub steps_done: AtomicU64,
    pub stale_skipped: AtomicU64,
    pub op_duplicates: AtomicU64,
    pub applied_through: AtomicU64,
    pub acked_sent: AtomicU64,
    pub next_cmd_seq: AtomicU64,
    pub orders_pending: AtomicU64,
    pub saves_done: AtomicU64,
    pub last_step_nanos: AtomicU64,
    pub agents: AtomicU64,
    pub embodied: AtomicU64,
    pub gaps: AtomicU64,
    pub commands_written: AtomicU64,
    pub save_failures: AtomicU64,
    pub steps_submitted: AtomicU64,
    /// Phase 1's counters (`nPhase1Stats`, phase1::stat).
    pub phase1: [AtomicU64; phase1::STATS],
    /// Raw bytes the last committed save wrote (its batch's pages, a budget's save included).
    /// Not in Java's array.
    pub last_save_bytes: AtomicU64,
    /// Saves taken for the paged modules' budgets and committed (pending generations, PF15;
    /// not in `saves_done`, which counts the saves Java asked for). Not in Java's array.
    pub pressure_saves: AtomicU64,
    /// Event archive rows written, rows it dropped since it began (its index's count, every
    /// cause), and its open, flush and compaction errors (none fail a save). Not in Java's array.
    pub archive_rows: AtomicU64,
    pub archive_dropped: AtomicU64,
    pub archive_failures: AtomicU64,
    /// The serial of the last save the archive wrote, and of the generation a restart would
    /// select (`archive.rs`). Not in Java's array.
    pub archive_serial: AtomicU64,
    pub archive_confirmed: AtomicU64,
    /// Rows and bytes waiting in the archive's buffer for the next save. Not in Java's array.
    pub archive_pending_rows: AtomicU64,
    pub archive_pending_bytes: AtomicU64,
    /// The view tap's counters (`tap.rs`). Not in Java's array.
    pub tap: tap::Counters,
    /// The session's journal replay (docs/replay-gate.md §3.2): the replay ends this engine ran
    /// (one a session), and the last replay's hand-overs and records applied while replaying
    /// (`dispatch` says which). Not in Java's array.
    pub replays_ended: AtomicU64,
    pub replay_handovers: AtomicU64,
    pub replay_records: AtomicU64,
    /// Copies of carried records (REASSERT, docs/boundary.md §3.2) applied, and dropped as already
    /// in the engine's state (not new, or new with an op id it heard: RP-50). Not in Java's array.
    pub copies_applied: AtomicU64,
    pub copies_dropped: AtomicU64,
    /// Lines logged that the replay window grows past one save interval
    /// ([`Engine::window_grows`], RP-49): a save Java asked for that was not made, a replayed
    /// session that ended with no such save, an engine that stopped simulating. Not in Java's
    /// array.
    pub window_grew: AtomicU64,
}

pub const STATS_LEN: usize = 16;

struct State {
    replay: Replay,
    outbox: Outbox<TransferCmd>,
    world: World,
    orders_rx: Receiver<Vec<Order>>,
    received_tx: Option<Sender<Received>>,
    /// Order units not yet written, oldest first.
    units: VecDeque<Vec<Order>>,
    /// Order units taken while the session's replay runs that hold an order which runs the clock
    /// to the hand-over's time or a conflict command (RP-66), and every unit after one
    /// (docs/replay-gate.md §3.6, RP-33): taken at the first step after the replay end, oldest
    /// first. Not saved; a SESSION_START drops them (the re-review's item 5).
    held_units: VecDeque<Vec<Order>>,
    /// Outbox commands to resend at the start of the session, before anything new.
    resend: VecDeque<(u64, OpId, TransferCmd)>,
    /// The engine's dictionary for this session: (kind, name) to id, ids per kind from 1.
    cmd_dict: HashMap<(u32, String), u32>,
    cmd_dict_next: HashMap<u32, u32>,
    dict_sent: HashSet<(u32, u32)>,
    /// The first observation sequence of this session; anything below is a replay.
    session_first: u64,
    /// The session's journal replay is running (docs/replay-gate.md §1.1, RP-1): set at
    /// SESSION_START and cleared once a session, by the first of the three triggers to run
    /// `shared::replay_end`: `replay_end_before` (a record whose sequence is at or past
    /// `session_first`), `replay_end_if_due` (the step's top, `applied_through + 1 >=
    /// session_first`) and `replay_end_at_session_start` (the arm's end, Java's journal start
    /// counted, RP-29). It follows `applied_through + 1 < session_first` without being it: RP-29
    /// can clear it first. Not saved.
    replaying: bool,
    /// The generation nonce of the copy (REASSERT) being dispatched, `None` otherwise (RP-8).
    /// Set by `reasserted()` around its dispatch (on the copy's Apply, RP-50), cleared after.
    applying_copy: Option<u64>,
    /// How far the shared clock may run while replaying (§3.6, RP-24): the world's present at
    /// SESSION_START, raised by each replayed record's world tick (`shared::catch_up`). Not saved.
    replay_clock: Tick,
    /// The replay's hand-overs (counted at each step's entry while replaying) and its records
    /// applied while replaying (`dispatch`), for the replay end's log line and the stats. Not
    /// saved.
    replay_handovers: u64,
    replay_records: u64,
    /// The replay ends run, one a session.
    replays_ended: u64,
    /// Whether this session's own records started with its JOURNAL_GEN, or one was made up.
    session_gen_seen: bool,
    /// The closed generations the pages hold (written in [`GEN_TABLE`]), and the paged saves'
    /// failures seen when they were last written: a failure since means write them all again.
    gens_written: usize,
    gens_failures: u64,
    pages_obs_seq: u64,
    last_applied_cmd: u64,
    /// A test's rewound command sequence (`REWIND_CMD_SEQ`): the next commands take sequences
    /// from here until it catches up with the outbox's counter.
    rewind: Option<u64>,
    game_time: i64,
    /// Where the op ids of ordered transfers come from (`random_op`).
    op_salt: std::collections::hash_map::RandomState,
    op_counter: u64,
    /// The industry engine and its commands (`industry.rs`).
    industry: industry::Link,
    /// The phase 1 world and its commands (`phase1.rs`).
    phase1: phase1::Link,
    /// The dragons and their commands (`dragons.rs`).
    dragons: dragons::Link,
    /// The ships and their commands (`ships.rs`).
    ships: ships::Link,
    /// The herds and their commands (`herds.rs`).
    herds: herds::Link,
    /// The conflict and its commands (`conflict.rs`).
    conflict: conflict::Link,
    /// The world model (`worldmodel.rs`): the habitat cells every module reads.
    worldmodel: worldmodel::Link,
    /// Players' interest areas and the modules' clock (`shared.rs`).
    shared: shared::Link,
    /// Every level's topology (`topology.rs`), handed to the modules.
    topology: topology::Link,
    /// TALK's open asks (`talk.rs`).
    talk: talk::Link,
    /// The paged modules' saves on their way back from the IO lane (`paging.rs`).
    paging: paging::Channel,
    /// The event archive's pending rows and dictionary (`archive.rs`), and whether this session
    /// records.
    archive: rtsim_archive::Recorder,
    archive_on: bool,
    /// The serial of the last save that took the recorder's rows (`archive::KEY`).
    archive_serial: u64,
    /// The view tap's cursor and mailbox (`tap.rs`).
    tap: tap::Tap,
}

impl State {
    fn new(orders_rx: Receiver<Vec<Order>>, received_tx: Option<Sender<Received>>) -> Self {
        let wm = worldmodel::Link::empty();
        State {
            replay: Replay::new(ObsSeq(0)),
            outbox: Outbox::new(),
            world: World::default(),
            orders_rx,
            received_tx,
            units: VecDeque::new(),
            held_units: VecDeque::new(),
            resend: VecDeque::new(),
            cmd_dict: HashMap::new(),
            cmd_dict_next: HashMap::new(),
            dict_sent: HashSet::new(),
            session_first: 1,
            replaying: false,
            applying_copy: None,
            replay_clock: Tick::ZERO,
            replay_handovers: 0,
            replay_records: 0,
            replays_ended: 0,
            session_gen_seen: false,
            gens_written: 0,
            gens_failures: 0,
            pages_obs_seq: 0,
            last_applied_cmd: 0,
            rewind: None,
            game_time: 0,
            op_salt: std::collections::hash_map::RandomState::new(),
            op_counter: 0,
            industry: industry::Link::empty(),
            phase1: phase1::Link::empty(),
            dragons: dragons::Link::empty(),
            ships: ships::Link::empty(),
            herds: herds::Link::empty(&wm.world),
            conflict: conflict::Link::empty(),
            worldmodel: wm,
            shared: shared::Link::default(),
            topology: topology::Link::default(),
            talk: talk::Link::default(),
            paging: paging::Channel::new(),
            archive: rtsim_archive::Recorder::default(),
            archive_on: false,
            archive_serial: 0,
            tap: tap::Tap::default(),
        }
    }

    /// A rewound sequence, while a test's `REWIND_CMD_SEQ` is behind the outbox's counter.
    fn rewound_seq(&mut self) -> Option<u64> {
        let r = self.rewind?;
        if r < self.outbox.next_seq() {
            self.rewind = Some(r + 1);
            return Some(r);
        }
        self.rewind = None;
        None
    }

    fn session_seq(&mut self) -> u64 {
        match self.rewound_seq() {
            Some(r) => r,
            None => self.outbox.take_seq(),
        }
    }

    /// An op id for a transfer an order asks for without one: 55 random bits, never 0. Never a
    /// counter restored from the pages: after a crash such a counter hands the ids it gave out
    /// since the last save to *different* transfers, and Java, still holding the first one's
    /// COMMAND_RESULT, would answer the new one DUPLICATE and drop it. Bits 55 to 63 stay clear:
    /// the top bit is Java's, bits 56 to 62 name other engine allocators (industry uses 0x49).
    /// A transfer the sim decides must instead derive its id from its own identity, so that
    /// re-deriving it after a crash gives the same id (docs/boundary.md §2.2).
    fn random_op(&mut self) -> OpId {
        use std::hash::{BuildHasher, Hasher};
        loop {
            self.op_counter += 1;
            let mut h = self.op_salt.build_hasher();
            h.write_u64(self.op_counter);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            );
            let op = h.finish() & ((1 << 55) - 1);
            if op != 0 {
                return OpId(op);
            }
        }
    }

    /// The dictionary id of a name, sent as a DICT command first if this session has not sent it.
    fn dict_id(&mut self, w: &mut RecordWriter<'_>, kind: u32, name: &str) -> u32 {
        let id = match self.cmd_dict.get(&(kind, name.to_string())) {
            Some(id) => *id,
            None => {
                let next = self.cmd_dict_next.entry(kind).or_insert(1);
                let id = *next;
                *next += 1;
                self.cmd_dict.insert((kind, name.to_string()), id);
                id
            }
        };
        if !self.dict_sent.contains(&(kind, id)) {
            let bytes = name.as_bytes();
            let r = w
                .reserve(cmd::DICT, cmd::dict::size(bytes.len()), kind)
                .expect("the unit's size was checked, its DICT records included");
            wire::put_u32(r, cmd::dict::ID, id);
            wire::put_u32(r, cmd::dict::NAME_LEN, bytes.len() as u32);
            r[cmd::dict::NAME..cmd::dict::NAME + bytes.len()].copy_from_slice(bytes);
            self.dict_sent.insert((kind, id));
        }
        id
    }

    /// Bytes of the DICT records `names` still need this session (each name once).
    fn dict_bytes(&self, names: &[(u32, &str)]) -> usize {
        let mut seen: HashSet<(u32, &str)> = HashSet::new();
        let mut total = 0;
        for &(kind, name) in names {
            if !seen.insert((kind, name)) {
                continue;
            }
            let sent = self
                .cmd_dict
                .get(&(kind, name.to_string()))
                .is_some_and(|id| self.dict_sent.contains(&(kind, *id)));
            if !sent {
                total += cmd::dict::size(name.len());
            }
        }
        total
    }
}

fn names_of(order: &Order, out: &mut Vec<(u32, String)>) {
    match order {
        Order::SpawnGroup {
            group_kind,
            dimension,
            members,
            ..
        } => {
            out.push((wire::DICT_BODY, group_kind.clone()));
            out.push((wire::DICT_DIMENSION, dimension.clone()));
            for m in members {
                for it in &m.items {
                    out.push((wire::DICT_ITEM, it.item.clone()));
                }
            }
        }
        Order::TransferToContainer {
            dimension, item, ..
        } => {
            out.push((wire::DICT_DIMENSION, dimension.clone()));
            out.push((wire::DICT_ITEM, item.clone()));
        }
        other => phase1::scripted_names(other, out),
    }
}

/// A test's order that runs the shared clock to the hand-over's time when taken (RP-33): the
/// works grants and raids (`phase1::directive`, the phase 1 world), TRIBUTE_START and HERD_FOUND.
fn runs_the_clock(o: &Order) -> bool {
    matches!(
        o,
        Order::WorksGrant { .. }
            | Order::WorksRaid { .. }
            | Order::TributeStart { .. }
            | Order::HerdFound { .. }
    )
}

/// A test's order that waits for the replay end, with every unit after it (RP-33): one that runs
/// the clock, a conflict command (RP-66), which joins the conflict's queue around its gate
/// (docs/replay-gate.md §4.4), and a herd command (RP-85), which joins the herds' queue around
/// theirs (§4.6): neither must go while the replay runs.
fn waits_for_the_replay_end(o: &Order) -> bool {
    runs_the_clock(o) || matches!(o, Order::ConflictCommand { .. } | Order::HerdCommand { .. })
}

fn spawn_size(members: &[crate::orders::MemberOrder]) -> usize {
    cmd::spawn_group::MEMBERS
        + members
            .iter()
            .map(|m| cmd::spawn_group::member::size(m.items.len()))
            .sum::<usize>()
}

/// Bytes of command records an order writes (DICT records not included).
fn command_bytes(order: &Order) -> usize {
    match order {
        Order::SpawnGroup { members, .. } => wire::align8(spawn_size(members)),
        Order::DespawnGroup { .. } => cmd::despawn_group::SIZE,
        Order::SteerGroup { .. } => cmd::steer_group::SIZE,
        Order::TransferToContainer { .. } => cmd::transfer_to_container::SIZE,
        Order::SaveAtNextStep
        | Order::RewindCmdSeq(_)
        | Order::Fail
        | Order::IndustryThreat(_)
        | Order::IndustryCommand { .. }
        | Order::DragonCommand { .. }
        | Order::ShipCommand { .. }
        | Order::HerdCommand { .. }
        | Order::HerdFound { .. }
        | Order::ConflictCommand { .. } => 0,
        other => phase1::scripted_bytes(other),
    }
}

thread_local! {
    static LAST_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// The calling thread's last error message (JNI `nLastError`).
pub fn last_error() -> String {
    LAST_ERROR.with(|e| e.borrow().clone())
}

pub fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = msg.into());
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn log_info(msg: &str) {
    rt::info(&format!("[mic_rtsim] {msg}"));
}

fn log_warn(msg: &str) {
    rt::warn(&format!("[mic_rtsim] {msg}"));
}

fn log_error(msg: &str) {
    rt::error(&format!("[mic_rtsim] {msg}"));
}

/// A write that failed partway poisons the store, which then refuses every read and write: the
/// next page miss would stop the engine. The store reopens in place from its files, under the
/// lock the failed write held, so no reader sees it poisoned (docs/choices-leg-purge.md LP10).
/// What the files hold is durable; the save that failed counts as failed whether or not its
/// record made it, so `durable` stays and Java keeps its journal, and its modules write their
/// pages again at the next save (PF14). False if the store is still poisoned: the next save
/// tries again.
fn recover_store(store: &mut PageStore) -> bool {
    match store.recover() {
        Ok(false) => true,
        Ok(true) => {
            log_warn(&format!(
                "the page store reopened from its files after a failed write; it holds generation {} (observations through {}); the next save writes the pages again",
                store.save_seq(),
                store.obs_seq()
            ));
            true
        }
        Err(e) => {
            log_error(&format!(
                "the page store cannot reopen after a failed write: {e}; the next save tries again"
            ));
            false
        }
    }
}

/// One session of the engine.
pub struct Engine {
    /// Observation buffers 0 and 1, then command buffers 0 and 1.
    /// Shared with Java's buffer holds (`hold_buffers`): Java's direct `ByteBuffer`s point into
    /// this memory, so it is freed only once the engine and every Java buffer are gone.
    bufs: Arc<[Buf; 4]>,
    cfg: Config,
    state: Mutex<State>,
    /// Shared with the paged modules, which read their pages through it (`paging::Store`).
    store: paging::Store,
    /// The event archive, written on the IO lane (`archive.rs`); `None` when off or unopened.
    archive: Mutex<Option<rtsim_archive::Archive>>,
    /// TEST: the next commit stops after the archive's flush, as a process killed before its page
    /// commit would ([`Engine::crash_before_next_page_commit`]).
    crash_at_page_commit: AtomicBool,
    /// Saves that failed or were not made (`paging::Job`): a save built before the step saw one
    /// of them is not made either (PF14).
    io_failures: Arc<AtomicU64>,
    orders_tx: Mutex<Sender<Vec<Order>>>,
    received_rx: Mutex<Option<Receiver<Received>>>,
    pub stats: Stats,
    /// `through + 1` of the oldest pending requested save ([`Engine::request_save`]); 0 when
    /// none is pending.
    save_through: AtomicU64,
    save_at_step_end: AtomicBool,
    /// The observation sequence the pages on disk include.
    durable: AtomicU64,
    started: AtomicBool,
    closing: AtomicBool,
    /// A crash: queued saves write nothing.
    dead: AtomicBool,
    failed: AtomicBool,
    protocol_error: SnapshotCell<Option<String>>,
    inspect: SnapshotCell<Inspect>,
}

impl Engine {
    pub fn open(cfg: Config) -> Result<Arc<Engine>, String> {
        if cfg.capacity < wire::MIN_CAPACITY || !cfg.capacity.is_multiple_of(wire::RECORD_ALIGN) {
            return Err(format!(
                "buffer capacity {} must be a multiple of 8 and at least {}",
                cfg.capacity,
                wire::MIN_CAPACITY
            ));
        }
        let (orders_tx, orders_rx) = channel();
        let (received_tx, received_rx) = if cfg.test {
            let (t, r) = channel();
            (Some(t), Some(r))
        } else {
            (None, None)
        };
        let bufs = [
            Buf::new(cfg.capacity),
            Buf::new(cfg.capacity),
            Buf::new(cfg.capacity),
            Buf::new(cfg.capacity),
        ];
        Ok(Arc::new(Engine {
            bufs: Arc::new(bufs),
            cfg,
            state: Mutex::new(State::new(orders_rx, received_tx)),
            store: Arc::new(Mutex::new(None)),
            archive: Mutex::new(None),
            crash_at_page_commit: AtomicBool::new(false),
            io_failures: Arc::new(AtomicU64::new(0)),
            orders_tx: Mutex::new(orders_tx),
            received_rx: Mutex::new(received_rx),
            stats: Stats::default(),
            save_through: AtomicU64::new(0),
            save_at_step_end: AtomicBool::new(false),
            durable: AtomicU64::new(0),
            started: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            dead: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            protocol_error: SnapshotCell::new(None),
            inspect: SnapshotCell::new(Inspect::default()),
        }))
    }

    /// A strong reference to the buffers for one Java `ByteBuffer`, as a raw pointer; Java gives it
    /// back with [`Engine::release_buffers`] when that buffer is unreachable (a `Cleaner`).
    pub fn hold_buffers(&self) -> usize {
        Arc::into_raw(Arc::clone(&self.bufs)) as usize
    }

    /// Gives back a hold from [`Engine::hold_buffers`].
    ///
    /// # Safety
    /// `token` came from `hold_buffers` and is released once.
    pub unsafe fn release_buffers(token: usize) {
        if token != 0 {
            // SAFETY: the caller's contract.
            drop(unsafe { Arc::from_raw(token as *const [Buf; 4]) });
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Buffer `which`: 0 and 1 observations, 2 and 3 commands.
    pub fn buffer(&self, which: usize) -> Option<&Buf> {
        self.bufs.get(which)
    }

    pub fn obs_buffer(&self, i: usize) -> &Buf {
        &self.bufs[i & 1]
    }

    pub fn cmd_buffer(&self, i: usize) -> &Buf {
        &self.bufs[2 + (i & 1)]
    }

    pub fn durable_obs_seq(&self) -> u64 {
        self.durable.load(Ordering::Acquire)
    }

    pub fn protocol_error(&self) -> Option<String> {
        (*self.protocol_error.load_full()).clone()
    }

    pub fn inspect(&self) -> Arc<Inspect> {
        self.inspect.load_full()
    }

    pub fn describe(&self) -> String {
        format!(
            "native engine (rtsim-jni {}, mic_native background pool; pages {}; {} steps, {} agents, {} embodied)",
            env!("CARGO_PKG_VERSION"),
            self.cfg.pages_dir.display(),
            self.stats.steps_done.load(Ordering::Relaxed),
            self.stats.agents.load(Ordering::Relaxed),
            self.stats.embodied.load(Ordering::Relaxed),
        )
    }

    /// The counters, in the Java side's order (`NativeEngine` STATS indexes).
    pub fn stats_array(&self) -> [u64; STATS_LEN] {
        let s = &self.stats;
        let l = |a: &AtomicU64| a.load(Ordering::Acquire);
        [
            l(&s.steps_started),
            l(&s.steps_done),
            l(&s.stale_skipped),
            l(&s.op_duplicates),
            l(&s.applied_through),
            self.durable_obs_seq(),
            l(&s.acked_sent),
            l(&s.next_cmd_seq),
            l(&s.orders_pending),
            l(&s.saves_done),
            l(&s.last_step_nanos),
            l(&s.agents),
            l(&s.embodied),
            l(&s.gaps),
            l(&s.commands_written),
            l(&s.save_failures),
        ]
    }

    /// The census (EQUILIBRIUM.md S1) of every module this session runs, each at its own now, in a
    /// fixed order: villages, dragons and nests, herds, ships, conflict. Between steps only, as the
    /// `with_*` reads.
    pub fn census(&self) -> Vec<rtsim_kernel::census::KindCensus> {
        let st = lock(&self.state);
        let mut out = st.phase1.world.census();
        if st.dragons.enabled {
            out.extend(st.dragons.engine.census());
        }
        if st.herds.enabled {
            out.extend(st.herds.engine.census());
        }
        if st.ships.enabled {
            out.extend(st.ships.engine.census());
        }
        if st.conflict.enabled {
            out.extend(st.conflict.engine.census());
        }
        out
    }

    /// TEST mode: the view lease read now and every layer encoded and committed into the view
    /// store at once, waited for (`tap::flush`). Errors when no lease is live.
    pub fn view_tap_flush(self: &Arc<Self>) -> Result<(), String> {
        if !self.cfg.test {
            return Err("view_tap_flush needs an engine opened in TEST mode".into());
        }
        tap::flush(self)
    }

    /// Makes the page store's next writes fail at `fault` (tests only: a failed save, then the
    /// store's recovery).
    #[doc(hidden)]
    pub fn inject_store_fault(&self, fault: Option<rtsim_kernel::page::Fault>) {
        if let Some(s) = lock(&self.store).as_mut() {
            s.inject_fault(fault);
        }
    }

    /// TEST mode: the next save's commit writes its events, then stops before the page commit as
    /// a process killed there would; the engine saves nothing after it (a crash, `close(false)`,
    /// comes next).
    pub fn crash_before_next_page_commit(&self) {
        if self.cfg.test {
            self.crash_at_page_commit.store(true, Ordering::Release);
        }
    }

    /// The phase 1 world's determinism hash, and whether the last load replayed its log to the
    /// state the save had. Takes the state lock: tests and `/rtsim` only, never a game tick.
    pub fn phase1_state(&self) -> (u64, bool, u64) {
        let st = lock(&self.state);
        (
            st.phase1.world.state_hash(),
            st.phase1.replay_diverged.is_none(),
            st.phase1.capture_mismatches,
        )
    }

    /// Phase 1's counters as of the last step (`phase1::stat` indexes).
    pub fn phase1_stats(&self) -> [u64; PHASE1_STATS] {
        std::array::from_fn(|i| self.stats.phase1[i].load(Ordering::Acquire))
    }

    /// A phase 1 village's sentiment toward a player as of the last step (TEST mode): `None`
    /// when the engine knows no such village (or keeps no inspection).
    pub fn phase1_sentiment(&self, village: u64, player: (u64, u64)) -> Option<i32> {
        if !self.cfg.test {
            return None;
        }
        let i = self.inspect();
        if !i.phase1_villages.contains(&village) {
            return None;
        }
        Some(
            i.phase1_sentiments
                .get(&village)
                .and_then(|v| v.iter().find(|e| (e.0, e.1) == player))
                .map_or(0, |e| i32::from(e.2)),
        )
    }

    /// A phase 1 village's works as of the last step (TEST mode; `phase1::works`): `None` when
    /// the engine knows no such village or it has no works state.
    pub fn phase1_works(&self, village: u64) -> Option<[i64; phase1::WORKS_VIEW]> {
        if !self.cfg.test {
            return None;
        }
        self.inspect().phase1_works.get(&village).copied()
    }

    fn set_protocol_error(&self, msg: String) {
        log_error(&format!("engine protocol error: {msg}"));
        self.protocol_error.publish(Some(msg));
    }

    /// Logs that the replay window grows (docs/replay-gate.md §3.2, RP-49): the journal past
    /// the last durable save, which the next start replays taking no page, holding every row it
    /// touches. Normally it is one vanilla autosave interval at most; `why` says what made it
    /// longer.
    fn window_grows(&self, why: &str) {
        self.stats.window_grew.fetch_add(1, Ordering::AcqRel);
        log_warn(&format!(
            "the replay window grows: {why}; a start now replays the journal past observation {}, every row it touches resident until its end",
            self.durable.load(Ordering::Acquire)
        ));
    }

    // ---- start ----------------------------------------------------------------------------------

    /// Loads the pages and zeroes the buffers. Returns `(pagesObsSeq, nextCmdSeq)`.
    pub fn start(&self, last_applied_cmd: u64) -> Result<(u64, u64), String> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err("start called twice".into());
        }
        let mut store = PageStore::open(&self.cfg.pages_dir, self.cfg.durability).map_err(|e| {
            format!(
                "cannot open the engine pages in {}: {e}",
                self.cfg.pages_dir.display()
            )
        })?;
        // A save taken for a module's budget past the last save Java asked for (a pending
        // generation, PF15) never had the journal trimmed behind it: the journal replays it.
        if store
            .rollback()
            .map_err(|e| format!("cannot roll back the engine pages' pending saves: {e}"))?
        {
            log_info(
                "rolled back the pages of saves taken for the modules' budgets since the last save; the journal replays them",
            );
        }
        let meta = store
            .get(KEY_META)
            .map_err(|e| format!("cannot read the engine's meta page: {e}"))?;
        let world = store
            .get(KEY_WORLD)
            .map_err(|e| format!("cannot read the engine's world page: {e}"))?;
        let (mut replay, outbox) = match meta {
            Some(b) => decode_meta(&b).map_err(|e| format!("the engine's meta page: {e}"))?,
            None => (Replay::new(ObsSeq(0)), Outbox::new()),
        };
        load_gens(&mut store, &mut replay)?;
        let world = match world {
            Some(b) => {
                World::from_bytes(&b).map_err(|e| format!("the engine's world page: {e}"))?
            }
            None => World::default(),
        };
        let mut link = industry::Link::load(&mut store, &self.cfg)?;
        if link.fresh {
            // A new industry engine's seed keys its op ids (W3): on disk before any command it
            // decides, or a crash before the first save would derive them again under another (W20).
            let mut batch = SaveBatch::new();
            let pages = link.pages();
            let stale = industry::deletions(&store, &pages)
                .map_err(|e| format!("cannot read the industry pages' index: {e}"))?;
            for k in stale {
                batch.delete(k);
            }
            for (k, b) in &pages {
                batch.put(*k, b);
            }
            let at = store.obs_seq();
            store
                .commit(batch, at)
                .map_err(|e| format!("cannot write the new industry engine's pages: {e}"))?;
            link.fresh = false;
        }
        let wlink = worldmodel::Link::load(&mut store);
        let pages_obs = store.obs_seq();
        // The archive cuts what lies above the generation the pages are at before anything is
        // written (§10.5).
        let serial = archive::load_serial(&mut store);
        let archive = if self.cfg.archive {
            archive::open(&self.cfg.pages_dir, serial, &self.stats)
        } else {
            None
        };
        if replay.applied_through().0 != pages_obs {
            log_warn(&format!(
                "the pages' manifest says observation {pages_obs}, their replay state {}",
                replay.applied_through().0
            ));
        }
        // The paged modules read their pages through the shared store from here on.
        *lock(&self.store) = Some(store);
        let source: Arc<dyn rtsim_kernel::rows::PageSource> = self.store.clone();
        let (budget, test) = (self.cfg.row_budget, self.cfg.test);
        let p1link = phase1::Link::load(&source, budget, test)?;
        let restored = p1link.world_restored();
        let dlink = dragons::Link::load(&source, budget, test, restored)?;
        let slink = ships::Link::load(&source, budget, test, restored)?;
        let hlink = herds::Link::load(&source, budget, test, restored, &wlink.world)?;
        let clink = conflict::Link::load(&source, budget, test, restored)?;
        // Pages a module dropped go before anything runs: a fresh table would read them as its
        // own (PF18).
        let purge: Vec<std::ops::RangeInclusive<u16>> = [
            (p1link.purge, phase1::TABLE),
            (dlink.pager.purge, dragons::TABLE),
            (slink.pager.purge, ships::TABLE),
            (hlink.pager.purge, herds::TABLE),
            (clink.pager.purge, conflict::TABLE),
        ]
        .into_iter()
        .filter(|(p, _)| *p)
        .map(|(_, t)| paging::tables(t))
        .collect();
        if !purge.is_empty() {
            let mut guard = lock(&self.store);
            let store = guard.as_mut().expect("the store opened above");
            let mut batch = SaveBatch::new();
            for tables in &purge {
                for k in paging::purge_keys(store, tables)
                    .map_err(|e| format!("cannot list the dropped pages: {e}"))?
                {
                    batch.delete(k);
                }
            }
            if !batch.is_empty() {
                let at = store.obs_seq();
                store
                    .commit(batch, at)
                    .map_err(|e| format!("cannot delete the dropped pages: {e}"))?;
            }
        }

        let mut st = lock(&self.state);
        st.gens_written = replay.closed_len();
        st.gens_failures = st.paging.failures_seen;
        st.replay = replay;
        st.outbox = outbox;
        st.world = world;
        st.outbox.resume_after(last_applied_cmd);
        st.outbox.ack_through(last_applied_cmd);
        let resend: VecDeque<_> = st.outbox.after(last_applied_cmd).cloned().collect();
        st.resend = resend;
        st.last_applied_cmd = last_applied_cmd;
        st.industry = link;
        st.industry.resume(last_applied_cmd);
        st.phase1 = p1link;
        st.phase1.resume(last_applied_cmd);
        st.dragons = dlink;
        st.ships = slink;
        st.herds = hlink;
        st.conflict = clink;
        st.worldmodel = wlink;
        st.pages_obs_seq = pages_obs;
        st.session_first = pages_obs + 1;
        // Every module has loaded (phase 1 replayed its input log): their tapes go on now.
        if let Some(a) = archive {
            archive::start(&mut st, &a, serial);
            *lock(&self.archive) = Some(a);
        }
        let next_cmd = st.outbox.next_seq();
        self.durable.store(pages_obs, Ordering::Release);
        for b in self.bufs.iter() {
            b.reset_words();
            for off in [wire::H_USED, wire::H_COUNT, wire::H_FLAGS] {
                b.put_u32(off, 0);
            }
            for off in [
                wire::H_WORD_A,
                wire::H_WORD_B,
                wire::H_WORD_C,
                wire::H_RESERVED,
            ] {
                b.put_u64(off, 0);
            }
        }
        self.publish_stats(&st);
        if self.cfg.test {
            self.publish_inspect(&st);
        }
        log_info(&format!(
            "engine started: pages include observation {pages_obs}, {} agents, {} outbox commands to resend, next command {next_cmd}",
            st.world.agents.len(),
            st.resend.len()
        ));
        Ok((pages_obs, next_cmd))
    }

    // ---- calls from Java ------------------------------------------------------------------------

    /// Queues step `h` and returns at once.
    pub fn submit(self: &Arc<Self>, h: u64) -> Result<(), String> {
        if self.closing.load(Ordering::Acquire) {
            return Err("the engine is closed".into());
        }
        if !self.started.load(Ordering::Acquire) {
            return Err("submit before start".into());
        }
        self.stats.steps_submitted.fetch_add(1, Ordering::AcqRel);
        let this = Arc::clone(self);
        match self.cfg.exec {
            Exec::Host => rt::spawn(Pool::Background, move || this.step(h)).map_err(|e| {
                self.stats.steps_submitted.fetch_sub(1, Ordering::AcqRel);
                format!("mic_native refused the step: {e}")
            }),
            Exec::Inline => {
                this.step(h);
                Ok(())
            }
        }
    }

    /// A save request (never blocks): pages stop exactly where the world save's journal did.
    /// The oldest pending request stays (RP-49): one asked for while the engine has not reached
    /// the pending one is dropped, and the next world save asks again. Replacing it moved the
    /// target past an engine that lags the journal by more than a save interval at every world
    /// save, so no save was ever taken.
    pub fn request_save(&self, through: u64) {
        let want = through.saturating_add(1);
        let _ = self
            .save_through
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending == 0 || want < pending).then_some(want)
            });
    }

    /// Queues one unit of orders (decoded from `nOrder`).
    pub fn order(&self, unit: Vec<Order>) -> Result<(), String> {
        let mut unit = unit;
        if !self.cfg.test && unit.iter().any(Order::test_only) {
            return Err(
                "a FAIL, P1_SPAWN_NEAR, P1_ADVANCE, DRAGON_COMMAND, SHIP_COMMAND, HERD_COMMAND, HERD_FOUND, CONFLICT_COMMAND or TALK_REPORT order needs an engine opened in TEST mode"
                    .into(),
            );
        }
        if unit.iter().any(|o| matches!(o, Order::SaveAtNextStep)) {
            self.save_at_step_end.store(true, Ordering::Release);
            unit.retain(|o| !matches!(o, Order::SaveAtNextStep));
        }
        if unit.is_empty() {
            return Ok(());
        }
        self.stats.orders_pending.fetch_add(1, Ordering::AcqRel);
        lock(&self.orders_tx).send(unit).map_err(|_| {
            self.stats.orders_pending.fetch_sub(1, Ordering::AcqRel);
            "the engine is gone".to_string()
        })
    }

    /// The received log since the last call (TEST mode).
    pub fn drain_received(&self) -> Vec<Received> {
        match lock(&self.received_rx).as_ref() {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        }
    }

    /// Ends the session. `save`: a final save through the IO lane, waited for; otherwise queued
    /// saves are dropped, as a crash would (a save already writing finishes, so the folder is
    /// never left to two stores at once). Waits up to `timeout` for a step in flight.
    pub fn close(self: &Arc<Self>, save: bool, timeout: Duration) -> Result<(), String> {
        if self.closing.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let deadline = Instant::now() + timeout;
        while self.stats.steps_done.load(Ordering::Acquire)
            < self.stats.steps_submitted.load(Ordering::Acquire)
        {
            if Instant::now() > deadline {
                // The step finishes on its own (it holds its own Arc), but no save of it may land:
                // the folder may be reopened at once (a singleplayer world loaded again).
                self.dead.store(true, Ordering::Release);
                let _ = self.run_io_waiting(|| {});
                *lock(&self.store) = None;
                return Err(format!(
                    "a step was still running after {} ms; it finishes on its own, pages not saved",
                    timeout.as_millis()
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut result = Ok(());
        let save =
            save && self.started.load(Ordering::Acquire) && !self.failed.load(Ordering::Acquire);
        // No page is taken while the session's replay runs (docs/replay-gate.md §3.2, RP-42): the
        // pages stay as they are, and the next start replays what they lack from Java's journal.
        let replaying = save && lock(&self.state).replaying;
        if replaying {
            log_info("closed while the session's replay runs: the pages stay as they were");
        }
        if save && !replaying {
            let save = self.take_save(&mut lock(&self.state), false);
            let this = Arc::clone(self);
            result = self.run_io_waiting(move || this.commit(save));
        } else {
            self.dead.store(true, Ordering::Release);
            let _ = self.run_io_waiting(|| {});
        }
        self.dead.store(true, Ordering::Release);
        *lock(&self.store) = None;
        result
    }

    fn run_io_waiting(&self, f: impl FnOnce() + Send + 'static) -> Result<(), String> {
        match self.cfg.exec {
            Exec::Host => {
                let h = rt::spawn_with_handle(Pool::Io, f)
                    .map_err(|e| format!("the IO lane refused the job: {e}"))?;
                h.wait().map_err(|e| format!("the IO job failed: {e}"))
            }
            Exec::Inline => {
                f();
                Ok(())
            }
        }
    }

    /// Runs `f` on the IO lane. False if the lane refused it (a save failure).
    fn spawn_io(&self, f: impl FnOnce() + Send + 'static) -> bool {
        match self.cfg.exec {
            Exec::Host => {
                if let Err(e) = rt::io(f) {
                    self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
                    log_error(&format!("the IO lane refused a save: {e}"));
                    return false;
                }
                true
            }
            Exec::Inline => {
                f();
                true
            }
        }
    }

    // ---- the step -------------------------------------------------------------------------------

    fn step(self: &Arc<Self>, h: u64) {
        let _z = rt::zone!("rtsim.step");
        let t0 = Instant::now();
        self.stats.steps_started.fetch_add(1, Ordering::AcqRel);
        if self.failed.load(Ordering::Acquire) {
            self.publish_empty(h);
        } else {
            match catch_unwind(AssertUnwindSafe(|| self.step_inner(h))) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    self.failed.store(true, Ordering::Release);
                    self.set_protocol_error(format!(
                        "step {h}: {e}; the engine stops simulating for this session"
                    ));
                    self.window_grows(STOPPED);
                    self.publish_empty(h);
                }
                Err(p) => {
                    let msg = p
                        .downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| p.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "a panic".into());
                    self.failed.store(true, Ordering::Release);
                    self.set_protocol_error(format!(
                        "step {h} panicked: {msg}; the engine stops simulating for this session"
                    ));
                    self.window_grows(STOPPED);
                    self.publish_empty(h);
                }
            }
        }
        let nanos = t0.elapsed().as_nanos() as u64;
        self.stats.last_step_nanos.store(nanos, Ordering::Release);
        self.stats.steps_done.fetch_add(1, Ordering::AcqRel);
        // The view tap, in the slack after the step: not step time, and gone the moment step
        // h + 1 arrives (`tap.rs`).
        if !self.failed.load(Ordering::Acquire) {
            tap::after_step(self, h, nanos);
        }
    }

    /// After a failure: hand the buffers back empty with the FAILED flag, so the bridge is never
    /// left waiting and knows to stop this session (docs/boundary.md §1.1). Writes the command
    /// buffer only when the protocol gives it to the engine (Java consumed step h - 2's).
    fn publish_empty(&self, h: u64) {
        let o = self.obs_buffer(h as usize);
        if o.published().load(Ordering::Acquire) == h {
            o.consumed().store(h, Ordering::Release);
        }
        let c = self.cmd_buffer(h as usize);
        if c.published().load(Ordering::Acquire) == h {
            return;
        }
        if h > 2 && c.consumed().load(Ordering::Acquire) != h - 2 {
            return; // Java still reads it: a broken protocol, nothing safe to write
        }
        c.put_u32(wire::H_USED, 0);
        c.put_u32(wire::H_COUNT, 0);
        c.put_u32(wire::H_FLAGS, wire::CMD_FLAG_FAILED);
        c.put_u64(wire::H_WORD_A, self.durable_obs_seq());
        c.put_i64(wire::H_WORD_B, 0);
        c.put_u64(wire::H_WORD_C, 0);
        c.put_u64(wire::H_RESERVED, 0);
        c.published().store(h, Ordering::Release);
    }

    fn step_inner(self: &Arc<Self>, h: u64) -> Result<(), String> {
        // Step h - 1 published its commands before it let go of the state, so step h can start
        // while h - 1 is still in its last lines: wait for them (microseconds), lending this pool
        // thread's slot meanwhile. Never a game thread: they take this lock only outside steps.
        let mut st = rt::blocking(|| lock(&self.state));
        // A hand-over of the session's replay (docs/replay-gate.md §2.2).
        if st.replaying {
            st.replay_handovers += 1;
        }
        // The paged modules' saves that came back since the last step (PF13).
        paging::drain(&mut st);
        // 1. Observations: published (acquire), read, consumed (release).
        let o = self.obs_buffer(h as usize);
        let published = o.published().load(Ordering::Acquire);
        if published != h {
            return Err(format!(
                "observation buffer {} holds hand-over {published}, not {h}",
                h & 1
            ));
        }
        // A test's order that runs the clock to the hand-over's time, Java's present, waits for
        // the replay end, and so does every unit after it, in order (docs/replay-gate.md §3.6,
        // RP-33): taken now it would carry the modules past the replayed records still to come.
        // A conflict command waits too, as it bypasses the gate (RP-66), and a herd command
        // (RP-85).
        if !st.replaying {
            for unit in std::mem::take(&mut st.held_units) {
                self.take_unit(&mut st, unit);
            }
        }
        for unit in st.orders_rx.try_iter().collect::<Vec<_>>() {
            if st.replaying
                && (!st.held_units.is_empty() || unit.iter().any(waits_for_the_replay_end))
            {
                st.held_units.push_back(unit);
            } else {
                self.take_unit(&mut st, unit);
            }
        }
        self.read_observations(&mut st, h, o);
        o.consumed().store(h, Ordering::Release);

        // 2. Simulate: the industry engine and the phase 1 world run to the hand-over's game time
        // and decide; orders stand in for the other decisions. 3. A save due at this boundary.
        // Every module measures through the levels' topologies (a module a load or a switch
        // replaced gets them here).
        topology::sync(&mut st);
        // A replay whose last record was this hand-over's last ends before anything runs
        // (docs/replay-gate.md §2.2, trigger 2).
        shared::replay_end_if_due(&mut st);
        industry::simulate(&mut st);
        dragons::simulate(&mut st);
        ships::simulate(&mut st);
        worldmodel::simulate(&mut st);
        herds::simulate(&mut st);
        conflict::simulate(&mut st);
        shared::simulate(&mut st);
        phase1::simulate(&mut st);
        // No page is taken while the session's replay runs (docs/replay-gate.md §3.2, RP-42): a
        // save asked for waits for the replay end, and so does the budget's, so the resident rows
        // may pass their budget by what the replay's hand-overs change until then: at most the
        // rows the crashed session touched since its last durable save (RP-49).
        let req = self.save_through.load(Ordering::Acquire);
        if !st.replaying {
            if self.save_at_step_end.swap(false, Ordering::AcqRel) {
                self.save_now(&mut st, false);
            } else if req != 0 && st.replay.applied_through().0 >= req - 1 {
                let _ =
                    self.save_through
                        .compare_exchange(req, 0, Ordering::AcqRel, Ordering::Acquire);
                self.save_now(&mut st, false);
            }
        }
        // The paged modules' budgets, at this step boundary: saved rows leave; a module whose
        // resident rows all changed since the last save gets a save of its own, one at a time
        // (PF15).
        paging::drain(&mut st);
        if paging::enforce_budget(&mut st) && st.paging.in_flight == 0 && !st.replaying {
            self.save_now(&mut st, true);
        }
        archive::absorb(&mut st, &self.stats);

        // 4. Commands, into C[h & 1] once Java has finished with step h - 2's.
        let c = self.cmd_buffer(h as usize);
        let consumed = c.consumed().load(Ordering::Acquire);
        if h > 2 && consumed != h - 2 {
            return Err(format!(
                "command buffer {} still held by Java (consumed {consumed}, expected {})",
                h & 1,
                h - 2
            ));
        }
        // SAFETY: Java finished with this buffer (consumed == h - 2) and reads it only after the
        // release store of `published = h` below.
        let area = unsafe { c.area_mut() };
        let mut w = RecordWriter::new(area);
        let (highest, more) = self.write_commands(&mut st, &mut w);
        let (used, count) = (w.used(), w.count());
        let acked = self.durable_obs_seq();
        c.put_u32(wire::H_USED, used as u32);
        c.put_u32(wire::H_COUNT, count);
        c.put_u32(wire::H_FLAGS, if more { wire::CMD_FLAG_MORE } else { 0 });
        c.put_u64(wire::H_WORD_A, acked);
        c.put_i64(wire::H_WORD_B, st.game_time);
        c.put_u64(wire::H_WORD_C, highest);
        c.put_u64(wire::H_RESERVED, 0);
        self.stats.acked_sent.store(acked, Ordering::Release);
        self.stats
            .commands_written
            .fetch_add(u64::from(count), Ordering::AcqRel);
        self.publish_stats(&st);
        if self.cfg.test {
            self.publish_inspect(&st);
        }
        // 5. Publish (release): the last thing the step does with the buffers.
        c.published().store(h, Ordering::Release);
        Ok(())
    }

    fn publish_stats(&self, st: &State) {
        let s = &self.stats;
        s.applied_through
            .store(st.replay.applied_through().0, Ordering::Release);
        s.next_cmd_seq
            .store(st.outbox.next_seq(), Ordering::Release);
        s.agents
            .store(st.world.agents.len() as u64, Ordering::Release);
        s.embodied
            .store(st.world.embodied() as u64, Ordering::Release);
        for (a, v) in s.phase1.iter().zip(st.phase1.stats) {
            a.store(v, Ordering::Release);
        }
        s.replays_ended.store(st.replays_ended, Ordering::Release);
        s.replay_handovers
            .store(st.replay_handovers, Ordering::Release);
        s.replay_records.store(st.replay_records, Ordering::Release);
    }

    fn publish_inspect(&self, st: &State) {
        self.inspect.publish(Inspect {
            ledger: st.world.ledger.clone(),
            done_ops: st.world.done_ops.clone(),
            replay: st.replay.clone(),
            agents: st.world.agents.clone(),
            counters: st.world.counters,
            phase1_sentiments: phase1::sentiments(st),
            phase1_villages: phase1::villages(st),
            phase1_works: phase1::works_view(st),
            topologies: st.topology.shared.clone(),
            topology_refused: st.topology.refused,
        });
    }

    fn take_unit(&self, st: &mut State, unit: Vec<Order>) {
        let mut kept = Vec::with_capacity(unit.len());
        for o in unit {
            match o {
                Order::SaveAtNextStep => self.save_at_step_end.store(true, Ordering::Release),
                Order::IndustryThreat(t) => industry::threat(st, &t),
                Order::IndustryCommand { names, record } => {
                    industry::order_command(st, &names, &record)
                }
                Order::DragonCommand { names, record } => {
                    dragons::order_command(st, &names, &record)
                }
                Order::ShipCommand { names, record } => ships::order_command(st, &names, &record),
                Order::HerdCommand { names, record } => herds::order_command(st, &names, &record),
                Order::HerdFound {
                    name,
                    species,
                    dimension,
                    x,
                    z,
                    count,
                    seed,
                } => herds::order_found(st, &name, &species, dimension, x, z, count, seed),
                Order::ConflictCommand { names, record } => {
                    conflict::order_command(st, &names, &record)
                }
                Order::TributeStart { village, lair } => dragons::tribute_start(st, village, lair),
                Order::NestDay(ticks) => dragons::nest_day(st, ticks),
                Order::TalkReport(c) => talk::post(st, &c),
                d if d.is_directive() => phase1::directive(st, d),
                // A rewind stays in the unit: it takes effect where it stands, when written.
                other => kept.push(other),
            }
        }
        if kept.is_empty() {
            self.stats.orders_pending.fetch_sub(1, Ordering::AcqRel);
        } else {
            st.units.push_back(kept);
        }
    }

    fn read_observations(self: &Arc<Self>, st: &mut State, h: u64, o: &Buf) {
        let mut used = o.get_u32(wire::H_USED) as usize;
        if used > o.capacity() {
            self.set_protocol_error(format!(
                "step {h}: observation buffer used={used} beyond its capacity"
            ));
            used = 0;
        }
        let last_applied = o.get_u64(wire::H_WORD_A);
        st.game_time = o.get_i64(wire::H_WORD_B);
        // Java's live lastAppliedCmdSeq can be ahead of the world save the next snapshot pairs
        // with, so it never trims the outbox (review finding 1); COMMAND_RESULTs do (`journaled`).
        if last_applied > st.last_applied_cmd {
            st.last_applied_cmd = last_applied;
        }
        // SAFETY: Java published hand-over h into this buffer and does not touch it again before
        // the release store of `consumed = h`, which follows this read.
        let area = unsafe { o.area(used) };
        let mut recs = Records::new(area);
        let mut ticks = 0;
        for rec in &mut recs {
            ticks += u32::from(rec.kind == obs::TICK);
            self.observe(st, h, &rec);
        }
        st.tap.ticks(ticks);
        if let Some(e) = recs.error() {
            self.set_protocol_error(format!("step {h}: {e}; the rest of the buffer was skipped"));
        }
    }

    fn log_received(st: &State, h: u64, rec: &Rec<'_>, obs_seq: u64, skipped: bool) {
        if let Some(tx) = &st.received_tx
            && rec.kind != obs::TICK
        {
            let _ = tx.send(Received {
                handover: h,
                obs_seq,
                skipped,
                bytes: rec.bytes.to_vec(),
            });
        }
    }

    fn observe(self: &Arc<Self>, st: &mut State, h: u64, rec: &Rec<'_>) {
        let b = rec.bytes;
        match rec.kind {
            obs::SESSION_START if rec.size() >= obs::session_start::SIZE => {
                let last_applied = wire::u64_at(b, obs::session_start::LAST_APPLIED_CMD_SEQ);
                let journal_first = wire::u64_at(b, obs::session_start::JOURNAL_FIRST_OBS_SEQ);
                let world_next = wire::u64_at(b, obs::session_start::WORLD_NEXT_OBS_SEQ);
                // The session's replay starts before any module resets, so that a reset inside
                // the arm never reads the last session's `false` (docs/replay-gate.md §2.1, RP-1).
                // The copy bit's reset is a defence (RP-8): `reasserted()` clears it after each
                // dispatch, and a step that panics inside one fails the engine for good. The clock
                // is held at the world's present until the replayed records move it (§3.6).
                st.replaying = true;
                st.applying_copy = None;
                st.replay_clock = st.phase1.world.now();
                st.replay_handovers = 0;
                st.replay_records = 0;
                // A test's order units held for the last session's replay end (RP-33) were that
                // session's: none waits for this one's.
                let held = std::mem::take(&mut st.held_units).len() as u64;
                if held > 0 {
                    self.stats.orders_pending.fetch_sub(held, Ordering::AcqRel);
                    log_warn(&format!(
                        "SESSION_START drops {held} test order units held for the last session's replay end"
                    ));
                }
                st.world.session_start();
                st.world.module_mask = wire::u32_at(b, obs::session_start::MODULE_MASK);
                st.world.game_time = wire::i64_at(b, obs::session_start::GAME_TIME);
                let (game_time, mask) = (st.world.game_time, st.world.module_mask);
                industry::session_start(st, game_time, mask);
                let mask = st.world.module_mask;
                phase1::session_start(st, mask);
                dragons::session_start(st, mask);
                ships::session_start(st, mask);
                herds::session_start(st, mask);
                conflict::session_start(st, mask);
                worldmodel::session_start(st);
                shared::session_start(st);
                talk::session_start(st);
                if last_applied > st.last_applied_cmd {
                    st.last_applied_cmd = last_applied;
                }
                // An empty journal is sent as 0; the kernel reads it as journal_first == world_next.
                let jf = if journal_first == 0 {
                    world_next
                } else {
                    journal_first
                };
                match st.replay.reconcile(ObsSeq(jf), ObsSeq(world_next)) {
                    Reconcile::InSync => {}
                    Reconcile::EngineAhead { pages, world_next } => log_warn(&format!(
                        "the engine's pages include observation {} but the world save only reaches {} (P29, open): \
                         the replay skips what the pages have",
                        pages.0,
                        world_next.0.saturating_sub(1)
                    )),
                    Reconcile::JournalTrimmedPastPages {
                        pages,
                        journal_first,
                    } => log_warn(&format!(
                        "the journal starts at observation {} but the pages only include {}: observations {} to {} are lost",
                        journal_first.0,
                        pages.0,
                        pages.0 + 1,
                        journal_first.0 - 1
                    )),
                }
                st.session_first = world_next.max(st.pages_obs_seq + 1);
                st.session_gen_seen = false;
                // Nothing to replay, the pages' or Java's journal's (RP-29): the replay ends here.
                // Else this hand-over is its first.
                shared::replay_end_at_session_start(st, jf);
                if st.replaying {
                    st.replay_handovers = 1;
                }
                Self::log_received(st, h, rec, 0, false);
            }
            k if obs::is_journaled(k) => self.journaled(st, h, rec),
            obs::WORLD_SAVED if rec.size() >= obs::world_saved::SIZE => {
                let durable = wire::u64_at(b, obs::world_saved::DURABLE_OBS_SEQ);
                st.replay.forget_ops_through(ObsSeq(durable));
                // The camp ledgers forget the answers of those records as they next book one
                // (MZB-177), from the conflict's replay end on (RP-68).
                st.conflict.engine.world_saved(durable);
                Self::log_received(st, h, rec, 0, false);
            }
            _ => {
                st.world.observe_state(rec);
                topology::observe_state(st, rec);
                industry::observe_state(st, rec);
                phase1::observe_state(st, rec);
                dragons::observe_state(st, rec);
                ships::observe_state(st, rec);
                herds::observe_state(st, rec);
                conflict::observe_state(st, rec);
                worldmodel::observe_state(st, rec);
                shared::observe_state(st, rec);
                talk::observe_state(st, rec);
                Self::log_received(st, h, rec, 0, false);
            }
        }
    }

    fn journaled(self: &Arc<Self>, st: &mut State, h: u64, rec: &Rec<'_>) {
        let b = rec.bytes;
        if rec.size() < obs::BODY {
            self.set_protocol_error(format!(
                "a journaled record of kind {:#06x} is only {} bytes",
                rec.kind,
                rec.size()
            ));
            return;
        }
        let seq = wire::u64_at(b, obs::OBS_SEQ);
        // The first record of this session ends the replay before it is admitted
        // (docs/replay-gate.md §2.2, trigger 1), and before a save it is due for: no page is
        // taken while replaying (RP-42).
        shared::replay_end_before(st, seq);
        // A save for a world save whose journal ended at `through` is taken right before the first
        // journaled record past it: the pages never get ahead of that world save. Java's journal
        // ends at or past the session's JOURNAL_GEN, so one due below `session_first` is older
        // than the session: it waits for the replay end.
        let req = self.save_through.load(Ordering::Acquire);
        if req != 0 && seq > req - 1 && !st.replaying {
            let _ = self
                .save_through
                .compare_exchange(req, 0, Ordering::AcqRel, Ordering::Acquire);
            self.save_now(st, false);
        }
        if rec.kind == obs::JOURNAL_GEN {
            self.journal_gen(st, h, rec, seq);
            return;
        }
        if seq >= st.session_first && !st.session_gen_seen {
            // A bridge that names no generation: this session's records still must not extend
            // the last one's, whose sequences a crash may hand out again (docs/boundary.md §3.2).
            st.session_gen_seen = true;
            let nonce = UNNAMED_GEN | st.session_first;
            log_warn(&format!(
                "this session's first record ({seq}) is not a JOURNAL_GEN: its records are judged as generation {nonce:#x}"
            ));
            st.replay.begin_gen(nonce, ObsSeq(seq - 1));
        }
        if rec.kind == obs::REASSERT {
            self.reasserted(st, h, rec, seq);
            return;
        }
        let op = op_of(rec);
        let verdict = self.admitted(st, seq, |r| r.admit(ObsSeq(seq), op));
        let skipped = verdict == Verdict::AlreadyApplied;
        Self::log_received(st, h, rec, seq, skipped);
        self.dispatch(st, rec, seq, verdict);
    }

    /// `admit` (a [`Replay`] admission of `seq`), going on past a gap: what the gap held is lost
    /// (a journal trimmed past the pages).
    fn admitted(
        &self,
        st: &mut State,
        seq: u64,
        admit: impl Fn(&mut Replay) -> Verdict,
    ) -> Verdict {
        let verdict = admit(&mut st.replay);
        let Verdict::Gap { expected, got } = verdict else {
            return verdict;
        };
        self.stats.gaps.fetch_add(1, Ordering::AcqRel);
        log_warn(&format!(
            "observations {} to {} never arrived (a journal trimmed past the pages); going on from {}",
            expected.0,
            got.0 - 1,
            got.0
        ));
        st.replay.skip_to(ObsSeq(seq));
        admit(&mut st.replay)
    }

    /// JOURNAL_GEN: a session's generation opens at its sequence (docs/boundary.md §3.2).
    fn journal_gen(&self, st: &mut State, h: u64, rec: &Rec<'_>, seq: u64) {
        if rec.size() < obs::journal_gen::SIZE {
            self.set_protocol_error(format!("a JOURNAL_GEN is only {} bytes", rec.size()));
            return;
        }
        if seq >= st.session_first {
            st.session_gen_seen = true;
        }
        let nonce = wire::u64_at(rec.bytes, obs::journal_gen::GEN);
        let verdict = self.admitted(st, seq, |r| r.admit_gen(ObsSeq(seq), nonce));
        Self::log_received(st, h, rec, seq, verdict == Verdict::AlreadyApplied);
        if seq < st.session_first && verdict == Verdict::Apply {
            // An earlier session's start past the pages: it ended with no save the pages took
            // (a crash, or a close, before its replay end or its first save), so this replay
            // holds its records too (§6.1's "More than one crash").
            self.window_grows(&format!(
                "the replayed journal holds the start of a session that ended with no save (generation {nonce:#x}, observation {seq})"
            ));
        }
        match verdict {
            Verdict::AlreadyApplied => {
                self.stats.stale_skipped.fetch_add(1, Ordering::AcqRel);
            }
            Verdict::Gap { .. } => {
                self.set_protocol_error(format!(
                    "observation {seq} still a gap after skipping to it"
                ));
            }
            _ => {}
        }
    }

    /// REASSERT: a copy of a record a chunk or entity carried, judged by the generation it was
    /// first journaled in (docs/boundary.md §3.2). Applied as the record itself under the copy's sequence when it
    /// is new; otherwise answered as a record whose op id was already heard.
    fn reasserted(self: &Arc<Self>, st: &mut State, h: u64, rec: &Rec<'_>, seq: u64) {
        let b = rec.bytes;
        let Some(inner) = reasserted_record(b) else {
            self.set_protocol_error(format!(
                "a REASSERT of {} bytes holds no journaled record",
                rec.size()
            ));
            return;
        };
        let verdict = self.admitted(st, seq, |r| r.admit(ObsSeq(seq), None));
        let nonce = wire::u64_at(b, obs::reassert::GEN);
        let of = ObsSeq(wire::u64_at(inner, obs::OBS_SEQ));
        let mut copy = inner.to_vec();
        wire::put_u64(&mut copy, obs::OBS_SEQ, seq);
        let r = Rec {
            kind: wire::u16_at(&copy, wire::R_KIND),
            aux: wire::u32_at(&copy, wire::R_AUX),
            bytes: &copy,
        };
        match verdict {
            Verdict::Apply => {}
            Verdict::AlreadyApplied => {
                // The pages include this copy: it was judged when it first came.
                Self::log_received(st, h, rec, seq, true);
                self.stats.stale_skipped.fetch_add(1, Ordering::AcqRel);
                st.world.refused_journaled(&r);
                return;
            }
            Verdict::DuplicateOp | Verdict::Gap { .. } => {
                self.set_protocol_error(format!(
                    "observation {seq} (a REASSERT) not admitted: {verdict:?}"
                ));
                return;
            }
        }
        let new = st.replay.copy_is_new(nonce, of);
        Self::log_received(st, h, rec, seq, !new);
        if !new {
            self.stats.copies_dropped.fetch_add(1, Ordering::AcqRel);
            Self::answered_again(st, &r);
            return;
        }
        let op = op_of(&r);
        let verdict = st.replay.admit_copy(nonce, of, ObsSeq(seq), op);
        if verdict == Verdict::Apply {
            self.stats.copies_applied.fetch_add(1, Ordering::AcqRel);
            // The copy bit (docs/replay-gate.md §2.3, RP-8): the modules' `journaled` read it,
            // on the apply path only.
            st.applying_copy = Some(nonce);
        } else {
            // Its op id was heard: answered again, applying nothing (RP-50).
            self.stats.copies_dropped.fetch_add(1, Ordering::AcqRel);
        }
        self.dispatch(st, &r, seq, verdict);
        st.applying_copy = None;
    }

    /// A journaled record admitted by sequence, to the module that claims it.
    fn dispatch(&self, st: &mut State, rec: &Rec<'_>, seq: u64, verdict: Verdict) {
        let b = rec.bytes;
        debug_assert!(
            st.applying_copy.is_none() || verdict == Verdict::Apply,
            "observation {seq}: the copy bit on a {verdict:?} (docs/replay-gate.md §2.3)"
        );
        // The records applied while replaying (docs/replay-gate.md §3.2): not the JOURNAL_GENs
        // (never dispatched), the stale records the pages hold, the copies dropped as not new or
        // the op-id duplicates. A copy of generation 0 that conflict refuses counts: it is
        // dispatched as Apply.
        if st.replaying && verdict == Verdict::Apply {
            st.replay_records += 1;
        }
        match verdict {
            Verdict::Apply if industry::claims(rec) => industry::journaled(st, rec),
            Verdict::Apply if dragons::claims(rec) => dragons::journaled(st, rec),
            Verdict::Apply if ships::claims(rec) => ships::journaled(st, rec),
            Verdict::Apply if herds::claims(rec) => herds::journaled(st, rec),
            Verdict::Apply if conflict::claims(rec) => conflict::journaled(st, rec),
            Verdict::Apply if phase1::claims(rec) => {
                shared::catch_up(st, rec);
                phase1::journaled(st, rec)
            }
            Verdict::Apply => {
                let replayed = seq < st.session_first;
                if rec.kind == obs::COMMAND_RESULT && rec.size() >= obs::command_result::SIZE {
                    // Done in Minecraft (applied, a duplicate, or failed): the transfer leaves
                    // the outbox at this observation, so a snapshot here keeps exactly what the
                    // world save here lacks.
                    let op = OpId(wire::u64_at(b, obs::command_result::OP_ID));
                    st.outbox.complete(op);
                    st.resend.retain(|(_, o, _)| *o != op);
                }
                st.world.apply_journaled(rec, replayed);
                shared::catch_up(st, rec);
                phase1::after_core(st, rec);
                dragons::after_core(st, rec);
            }
            Verdict::AlreadyApplied => {
                self.stats.stale_skipped.fetch_add(1, Ordering::AcqRel);
                st.world.refused_journaled(rec);
            }
            Verdict::DuplicateOp => {
                if rec.kind == obs::TRANSFER {
                    self.stats.op_duplicates.fetch_add(1, Ordering::AcqRel);
                }
                Self::answered_again(st, rec);
            }
            Verdict::Gap { .. } => {
                self.set_protocol_error(format!(
                    "observation {seq} still a gap after skipping to it"
                ));
            }
        }
    }

    /// A record already in the engine's state: an answer for an op it heard is still an answer.
    fn answered_again(st: &mut State, rec: &Rec<'_>) {
        if industry::claims(rec) {
            industry::answered_again(st, rec);
        }
        dragons::answered_again(st, rec);
        st.world.refused_journaled(rec);
    }

    /// Snapshots the pages at this step boundary and writes them on the IO lane. `pending`: a
    /// save for the paged modules' budgets, past the last save Java asked for (PF15).
    fn save_now(self: &Arc<Self>, st: &mut State, pending: bool) {
        let save = self.take_save(st, pending);
        let this = Arc::clone(self);
        if !self.spawn_io(move || this.commit(save)) && !pending {
            self.window_grows(NOT_MADE);
        }
    }

    /// One save's pages, taken at this step boundary on the step thread.
    fn take_save(&self, st: &mut State, pending: bool) -> Save {
        let _z = rt::zone!("rtsim.snapshot");
        // Results that came back first: a failed save's pages go into this one (PF14).
        paging::drain(st);
        let test = self.cfg.test;
        let obs_seq = st.replay.applied_through().0;
        // The events up to the state this save holds (§10.3).
        let events = archive::take(st, &self.stats);
        let meta = encode_meta(&st.replay, &st.outbox);
        let world = st.world.to_bytes();
        let ind = industry::save(st);
        let mut whole = ind.put;
        let gone = ind.delete;
        let mut batch = SaveBatch::new();
        let mut tickets = paging::Tickets {
            industry: Some(ind.ticket),
            ..paging::Tickets::default()
        };
        // The dragons, the ships, the herds and the conflict page into the batch (PF13): their
        // meta pages and the pages changed since the last save.
        let (t, dragons) = dragons::save(st, &mut batch, test);
        tickets.dragons = t;
        let (t, ships) = ships::save(st, &mut batch, test);
        tickets.ships = t;
        let (t, herds) = herds::save(st, &mut batch, test);
        tickets.herds = t;
        let (t, conflict) = conflict::save(st, &mut batch, test);
        tickets.conflict = t;
        let failures = self.stats.save_failures.load(Ordering::Acquire);
        whole.extend(worldmodel::snapshot(st, failures));
        whole.extend(gen_blocks(st));
        let (p1, t) = phase1::snapshot(st, &mut batch, test);
        tickets.world = t;
        st.paging.in_flight += 1;
        let job = paging::Job::new(
            tickets,
            st.paging.tx.clone(),
            Arc::clone(&self.io_failures),
            st.paging.failures_seen,
        );
        Save {
            meta,
            world,
            whole,
            gone,
            batch,
            live: [
                (dragons::TABLE, dragons),
                (ships::TABLE, ships),
                (herds::TABLE, herds),
                (conflict::TABLE, conflict),
            ],
            p1,
            job,
            obs_seq,
            pending,
            events,
        }
    }

    fn commit(&self, save: Save) {
        let _z = rt::zone!("rtsim.save");
        let Save {
            meta,
            world,
            whole,
            gone,
            mut batch,
            live,
            p1,
            job,
            obs_seq,
            pending,
            events,
        } = save;
        // From here on, a return before `job.succeeded()` reports the save failed (the job's
        // drop): its modules write their pages again.
        if self.dead.load(Ordering::Acquire) {
            return;
        }
        let mut guard = lock(&self.store);
        let Some(store) = guard.as_mut() else { return };
        let stale = job.stale();
        // The events go first, stale save or not: the chunks are tagged with this save's serial,
        // and a restart at an older generation cuts them (§10.3, §10.5).
        let serial = events.as_ref().map(|ev| ev.serial);
        if let Some(ev) = events {
            archive::flush(&self.archive, ev, obs_seq, &self.stats);
        }
        if self.cfg.test && self.crash_at_page_commit.swap(false, Ordering::AcqRel) {
            self.dead.store(true, Ordering::Release);
            return;
        }
        // A save Java asked for that is not made: the window grows (RP-49). A budget's save
        // is a pending generation, which a crash rolls back anyway.
        let lost = || {
            if !pending {
                self.window_grows(NOT_MADE);
            }
        };
        // A store poisoned by a failed write reopens from its files before this save writes to it
        // (LP10); the events above went first either way.
        if !recover_store(store) {
            self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
            lost();
            return;
        }
        if stale {
            // Built before the step saw a save fail: its paged modules wrote only what changed
            // since that save, which never reached the disk (PF14).
            self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
            log_warn(
                "a save taken before an earlier one failed is not made; the next save writes the pages again",
            );
            lost();
            return;
        }
        batch.put(KEY_META, &meta);
        batch.put(KEY_WORLD, &world);
        if let Some(s) = serial {
            batch.put(archive::KEY, &s.to_le_bytes());
        }
        // The industry engine's deletions are its own marks (no index read); the paged modules'
        // and phase 1's read only their tables' index blocks.
        let deletions = (|| -> Result<Vec<PageKey>, rtsim_kernel::page::PageError> {
            let mut out = gone;
            for (table, l) in &live {
                out.extend(l.deletions(store, *table)?);
            }
            out.extend(phase1::deletions(store, &p1)?);
            Ok(out)
        })();
        let deletions = match deletions {
            Ok(d) => d,
            Err(e) => {
                self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
                log_error(&format!(
                    "saving the engine pages failed: the page index: {e}"
                ));
                lost();
                return;
            }
        };
        for k in deletions {
            batch.delete(k);
        }
        for (k, b) in &whole {
            batch.put(*k, b);
        }
        let bytes = batch.raw_bytes() as u64;
        let r = if pending {
            store.commit_pending(batch, obs_seq)
        } else {
            store.commit(batch, obs_seq)
        };
        match r {
            Ok(_) => {
                self.stats.last_save_bytes.store(bytes, Ordering::Release);
                p1.done();
                if pending {
                    // Past the last save Java asked for: the journal is not trimmed behind it.
                    self.stats.pressure_saves.fetch_add(1, Ordering::AcqRel);
                } else {
                    self.durable.fetch_max(obs_seq, Ordering::AcqRel);
                    self.stats.saves_done.fetch_add(1, Ordering::AcqRel);
                    if let Some(s) = serial {
                        self.stats.archive_confirmed.store(s, Ordering::Release);
                    }
                }
                drop(guard);
                job.succeeded();
                let confirmed = self.stats.archive_confirmed.load(Ordering::Acquire);
                archive::compact(&self.archive, confirmed, &self.stats);
            }
            Err(e) => {
                self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
                log_error(&format!("saving the engine pages failed: {e}"));
                recover_store(store);
                lost();
            }
        }
    }

    // ---- commands -------------------------------------------------------------------------------

    /// Writes the resend queue, then whole order units while they fit. Returns the highest
    /// command sequence written and whether anything is left for a later step.
    fn write_commands(&self, st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
        let mut highest = 0;
        loop {
            // The industry link's and phase 1's kept commands go out merged with the core's in
            // sequence order: Java skips a command at or below the last it applied.
            let core = st.resend.front().map(|r| r.0);
            if core.is_none()
                && phase1::next_resend(st).is_none()
                && industry::next_resend(st).is_none()
            {
                break;
            }
            let (h, full) = industry::write_resends(st, w, lowest(core, phase1::next_resend(st)));
            highest = highest.max(h);
            if full {
                return (highest, true);
            }
            let (h, full) = phase1::write_resends(st, w, lowest(core, industry::next_resend(st)));
            highest = highest.max(h);
            if full {
                return (highest, true);
            }
            let Some((seq, op, t)) = st.resend.front().cloned() else {
                continue; // the others wrote theirs; the loop ends when all three are empty
            };
            let need = st.dict_bytes(&[
                (wire::DICT_DIMENSION, &t.dimension),
                (wire::DICT_ITEM, &t.item),
            ]) + cmd::transfer_to_container::SIZE;
            let name_too_big = [&t.dimension, &t.item]
                .iter()
                .any(|n| cmd::dict::size(n.len()) > wire::MAX_RECORD);
            if name_too_big || need > w.capacity() {
                // Can never be written; it would hold everything after it back forever.
                log_error(&format!(
                    "a resent transfer (op {}) can never fit a command buffer; dropped",
                    op.0
                ));
                st.resend.pop_front();
                continue;
            }
            if need > w.remaining() {
                return (highest, true);
            }
            write_transfer(st, w, seq, op, &t);
            highest = highest.max(seq);
            st.resend.pop_front();
        }
        while let Some(unit) = st.units.front() {
            let mut names = Vec::new();
            unit.iter().for_each(|o| names_of(o, &mut names));
            let refs: Vec<(u32, &str)> = names.iter().map(|(k, n)| (*k, n.as_str())).collect();
            let commands: usize = unit.iter().map(command_bytes).sum();
            let need = st.dict_bytes(&refs) + commands;
            let too_big = unit.iter().any(|o| command_bytes(o) > wire::MAX_RECORD)
                || names
                    .iter()
                    .any(|(_, n)| cmd::dict::size(n.len()) > wire::MAX_RECORD)
                || need > w.capacity();
            if too_big {
                log_error(&format!(
                    "an order unit of {need} bytes can never fit a command buffer; dropped"
                ));
                st.units.pop_front();
                self.stats.orders_pending.fetch_sub(1, Ordering::AcqRel);
                continue;
            }
            if need > w.remaining() {
                return (highest, true);
            }
            let unit = st.units.pop_front().expect("front exists");
            for order in unit {
                if let Some(seq) = write_order(st, w, order) {
                    highest = highest.max(seq);
                }
            }
            self.stats.orders_pending.fetch_sub(1, Ordering::AcqRel);
        }
        let (h, more) = industry::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = phase1::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = dragons::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = ships::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = herds::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = conflict::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = worldmodel::write_queue(st, w);
        highest = highest.max(h);
        if more {
            return (highest, true);
        }
        let (h, more) = talk::write_queue(st, w);
        (highest.max(h), more)
    }
}

/// The lower of two next sequences, `None` standing for none at all.
/// One save on its way from the step thread to the IO lane: the core's pages, the pages written
/// outside the batch (the industry engine's changed ones and the world model's), and the paged
/// modules' batch with their tickets (`job`, sent back whatever happens, the industry engine's
/// included) and what each leaves live in its own table.
struct Save {
    meta: Vec<u8>,
    world: Vec<u8>,
    whole: Vec<(PageKey, Vec<u8>)>,
    /// The industry pages to delete: districts and replay clusters gone since the last save.
    gone: Vec<PageKey>,
    batch: SaveBatch,
    live: [(u16, paging::Live); 4],
    p1: phase1::Saved,
    job: paging::Job,
    obs_seq: u64,
    pending: bool,
    /// The events up to this snapshot, when the session records (`archive.rs`).
    events: Option<archive::Taken>,
}

fn lowest(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) => x,
        (None, y) => y,
    }
}

/// The record a REASSERT holds: a whole journaled record, neither a copy nor a generation.
fn reasserted_record(b: &[u8]) -> Option<&[u8]> {
    let inner = b.get(obs::reassert::INNER..)?;
    if inner.len() < obs::BODY {
        return None;
    }
    let kind = wire::u16_at(inner, wire::R_KIND);
    let size = wire::u16_at(inner, wire::R_SIZE) as usize;
    let fits = size >= obs::BODY && size <= inner.len() && size.is_multiple_of(wire::RECORD_ALIGN);
    let journaled = obs::is_journaled(kind) && kind != obs::REASSERT && kind != obs::JOURNAL_GEN;
    (fits && journaled).then(|| &inner[..size])
}

/// The blocks of closed generations a save writes: those that gained one since the pages' last,
/// or every block after a paged save failed (it may have held one).
fn gen_blocks(st: &mut State) -> Vec<(PageKey, Vec<u8>)> {
    let closed = st.replay.closed_len();
    let failed = st.paging.failures_seen != st.gens_failures;
    if closed == st.gens_written && !failed {
        return Vec::new();
    }
    let from = if failed {
        0
    } else {
        st.gens_written / GEN_BLOCK
    };
    let out = (from..closed.div_ceil(GEN_BLOCK))
        .map(|k| (gen_key(k), st.replay.closed_block(k)))
        .collect();
    st.gens_written = closed;
    st.gens_failures = st.paging.failures_seen;
    out
}

/// Reads the closed generations the meta page counts.
fn load_gens(store: &mut PageStore, replay: &mut Replay) -> Result<(), String> {
    let n = replay.closed_expected();
    for k in 0..n.div_ceil(GEN_BLOCK) {
        let b = store
            .get(gen_key(k))
            .map_err(|e| format!("cannot read the journal's generations, block {k}: {e}"))?
            .ok_or_else(|| {
                format!("the journal's generations: block {k} of {n} generations is missing")
            })?;
        replay
            .load_closed_block(&b)
            .map_err(|e| format!("the journal's generations, block {k}: {e}"))?;
    }
    if replay.closed_len() != n {
        return Err(format!(
            "the journal's generations: the meta page counts {n}, the blocks hold {}",
            replay.closed_len()
        ));
    }
    Ok(())
}

fn op_of(rec: &Rec<'_>) -> Option<OpId> {
    let b = rec.bytes;
    let raw = match rec.kind {
        obs::TRANSFER if rec.size() >= obs::transfer::SIZE => wire::u64_at(b, obs::transfer::OP_ID),
        obs::TAKEN if rec.size() >= obs::taken::SIZE => wire::u64_at(b, obs::taken::OP_ID),
        obs::DIED if rec.size() >= obs::died::SIZE => wire::u64_at(b, obs::died::OP_ID),
        obs::COMMAND_RESULT
            if rec.size() >= obs::command_result::SIZE
                && (rec.aux == obs::command_result::APPLIED
                    || rec.aux == obs::command_result::DUPLICATE_OP) =>
        {
            wire::u64_at(b, obs::command_result::OP_ID)
        }
        _ => [
            industry::op_of(rec),
            dragons::op_of(rec),
            ships::op_of(rec),
            herds::op_of(rec),
            conflict::op_of(rec),
            phase1::op_of(rec),
        ]
        .into_iter()
        .find(|&op| op != 0)
        .unwrap_or(0),
    };
    (raw != 0).then_some(OpId(raw))
}

fn write_transfer(st: &mut State, w: &mut RecordWriter<'_>, seq: u64, op: OpId, t: &TransferCmd) {
    let dim = st.dict_id(w, wire::DICT_DIMENSION, &t.dimension);
    let item = st.dict_id(w, wire::DICT_ITEM, &t.item);
    let r = w
        .reserve(
            cmd::TRANSFER_TO_CONTAINER,
            cmd::transfer_to_container::SIZE,
            0,
        )
        .expect("checked to fit");
    wire::put_u64(r, cmd::CMD_SEQ, seq);
    wire::put_u64(r, cmd::transfer_to_container::OP_ID, op.0);
    wire::put_u32(r, cmd::transfer_to_container::DIM, dim);
    wire::put_u32(r, cmd::transfer_to_container::ITEM, item);
    wire::put_i32(r, cmd::transfer_to_container::X, t.pos[0]);
    wire::put_i32(r, cmd::transfer_to_container::Y, t.pos[1]);
    wire::put_i32(r, cmd::transfer_to_container::Z, t.pos[2]);
    wire::put_i32(r, cmd::transfer_to_container::COUNT, t.count);
}

/// Writes one order as commands (its DICT records first). Returns the command's sequence.
fn write_order(st: &mut State, w: &mut RecordWriter<'_>, order: Order) -> Option<u64> {
    match order {
        Order::SpawnGroup {
            agent,
            epoch,
            group_kind,
            dimension,
            pos,
            members,
        } => {
            let kind = st.dict_id(w, wire::DICT_BODY, &group_kind);
            let dim = st.dict_id(w, wire::DICT_DIMENSION, &dimension);
            let item_ids: Vec<Vec<u32>> = members
                .iter()
                .map(|m| {
                    m.items
                        .iter()
                        .map(|it| st.dict_id(w, wire::DICT_ITEM, &it.item))
                        .collect()
                })
                .collect();
            let seq = st.session_seq();
            let r = w
                .reserve(cmd::SPAWN_GROUP, spawn_size(&members), members.len() as u32)
                .expect("checked to fit");
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, cmd::spawn_group::AGENT, agent);
            wire::put_u32(r, cmd::spawn_group::EPOCH, epoch);
            wire::put_u32(r, cmd::spawn_group::GROUP_KIND, kind);
            wire::put_u32(r, cmd::spawn_group::DIM, dim);
            wire::put_f64(r, cmd::spawn_group::X, pos[0]);
            wire::put_f64(r, cmd::spawn_group::Y, pos[1]);
            wire::put_f64(r, cmd::spawn_group::Z, pos[2]);
            let mut at = cmd::spawn_group::MEMBERS;
            for (m, ids) in members.iter().zip(&item_ids) {
                use cmd::spawn_group::member as mb;
                wire::put_u32(r, at + mb::INDEX, m.index);
                wire::put_u32(r, at + mb::ITEM_COUNT, m.items.len() as u32);
                wire::put_f32(r, at + mb::HEALTH, m.health);
                wire::put_i32(r, at + mb::BODY_A, m.body_a);
                wire::put_i32(r, at + mb::BODY_B, m.body_b);
                for (i, (it, id)) in m.items.iter().zip(ids).enumerate() {
                    let ia = at + mb::ITEMS + i * obs::item::SIZE;
                    wire::put_u32(r, ia + obs::item::SLOT, it.slot);
                    wire::put_u32(r, ia + obs::item::ITEM, *id);
                    wire::put_i32(r, ia + obs::item::COUNT, it.count);
                }
                at += mb::size(m.items.len());
            }
            let world_members: Vec<Member> = members
                .iter()
                .map(|m| Member {
                    index: m.index,
                    body_kind: String::new(),
                    health: m.health,
                    max_health: 0.0,
                    body_a: m.body_a,
                    body_b: m.body_b,
                    flags: 0,
                    pos,
                    items: m
                        .items
                        .iter()
                        .map(|it| Item {
                            slot: it.slot,
                            item: it.item.clone(),
                            count: it.count,
                        })
                        .collect(),
                    gone: None,
                })
                .collect();
            st.world
                .spawning(agent, epoch, &group_kind, &dimension, pos, &world_members);
            Some(seq)
        }
        Order::DespawnGroup {
            agent,
            epoch,
            reason,
        } => {
            let seq = st.session_seq();
            let r = w
                .reserve(cmd::DESPAWN_GROUP, cmd::despawn_group::SIZE, 0)
                .expect("checked to fit");
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, cmd::despawn_group::AGENT, agent);
            wire::put_u32(r, cmd::despawn_group::EPOCH, epoch);
            wire::put_u32(r, cmd::despawn_group::REASON, reason);
            st.world.despawning(agent, epoch);
            Some(seq)
        }
        Order::SteerGroup {
            agent,
            epoch,
            speed,
            pos,
        } => {
            let seq = st.session_seq();
            let r = w
                .reserve(cmd::STEER_GROUP, cmd::steer_group::SIZE, 0)
                .expect("checked to fit");
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, cmd::steer_group::AGENT, agent);
            wire::put_u32(r, cmd::steer_group::EPOCH, epoch);
            wire::put_f32(r, cmd::steer_group::SPEED, speed);
            wire::put_f64(r, cmd::steer_group::X, pos[0]);
            wire::put_f64(r, cmd::steer_group::Y, pos[1]);
            wire::put_f64(r, cmd::steer_group::Z, pos[2]);
            Some(seq)
        }
        Order::TransferToContainer {
            op,
            dimension,
            item,
            pos,
            count,
        } => {
            let op = if op == 0 { st.random_op() } else { OpId(op) };
            let t = TransferCmd {
                dimension,
                item,
                pos,
                count,
            };
            // The DICT records go before the command, and the sequence is taken only now, so the
            // buffer's sequences rise in buffer order.
            st.dict_id(w, wire::DICT_DIMENSION, &t.dimension);
            st.dict_id(w, wire::DICT_ITEM, &t.item);
            // A rewound transfer (a test's broken engine) is written but never kept for resend.
            let seq = match st.rewound_seq() {
                Some(r) => r,
                None => st.outbox.push_with_op(t.clone(), op),
            };
            write_transfer(st, w, seq, op, &t);
            Some(seq)
        }
        Order::RewindCmdSeq(next) => {
            st.rewind = Some(next);
            None
        }
        Order::Fail => panic!("a FAIL order (tests): the engine fails on purpose"),
        Order::SaveAtNextStep
        | Order::IndustryThreat(_)
        | Order::IndustryCommand { .. }
        | Order::DragonCommand { .. }
        | Order::ShipCommand { .. }
        | Order::HerdCommand { .. }
        | Order::HerdFound { .. }
        | Order::ConflictCommand { .. } => None,
        other => phase1::write_scripted(st, w, &other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_round_trips() {
        let mut replay = Replay::new(ObsSeq(4));
        replay.admit(ObsSeq(5), Some(OpId(wire::JAVA_OP_BIT | 3)));
        // Generations: more closed ones than a block holds, and an open one with a copy.
        for g in 0..GEN_BLOCK as u64 + 3 {
            let at = replay.applied_through().0 + 1;
            replay.admit_gen(ObsSeq(at), 100 + g);
            replay.admit(ObsSeq(at + 1), None);
        }
        let at = replay.applied_through().0 + 1;
        replay.admit(ObsSeq(at), None);
        replay.admit_copy(7, ObsSeq(3), ObsSeq(at), None);
        let mut outbox: Outbox<TransferCmd> = Outbox::new();
        outbox.push_with_op(
            TransferCmd {
                dimension: "d".into(),
                item: "i".into(),
                pos: [1, -2, 3],
                count: 64,
            },
            OpId(4242),
        );
        let (mut r, o) = decode_meta(&encode_meta(&replay, &outbox)).unwrap();
        assert_eq!(r.closed_expected(), GEN_BLOCK + 2);
        for k in 0..r.closed_expected().div_ceil(GEN_BLOCK) {
            r.load_closed_block(&replay.closed_block(k)).unwrap();
        }
        assert_eq!(r, replay);
        assert_eq!(o, outbox);
        assert!(!r.copy_is_new(7, ObsSeq(3)), "the open generation's copies");
        assert!(
            !r.copy_is_new(100, ObsSeq(6)),
            "the first closed generation"
        );
        assert!(!r.copy_is_new(100, ObsSeq(7)), "its last");
        assert!(r.copy_is_new(100, ObsSeq(8)), "past its last");
    }

    #[test]
    fn a_save_writes_the_generation_blocks_that_changed_and_all_after_a_failure() {
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut st = State::new(rx, None);
        assert!(gen_blocks(&mut st).is_empty(), "none closed");
        // The first opens a generation and each later one closes the one before: a block and one.
        for g in 0..GEN_BLOCK as u64 + 2 {
            let at = st.replay.applied_through().0 + 1;
            st.replay.admit_gen(ObsSeq(at), 1 + g);
        }
        let keys = |v: Vec<(PageKey, Vec<u8>)>| v.into_iter().map(|(k, _)| k).collect::<Vec<_>>();
        assert_eq!(keys(gen_blocks(&mut st)), vec![gen_key(0), gen_key(1)]);
        assert!(gen_blocks(&mut st).is_empty(), "nothing new");
        let at = st.replay.applied_through().0 + 1;
        st.replay.admit_gen(ObsSeq(at), 9999);
        assert_eq!(
            keys(gen_blocks(&mut st)),
            vec![gen_key(1)],
            "only the block that grew"
        );
        st.paging.failures_seen += 1;
        assert_eq!(
            keys(gen_blocks(&mut st)),
            vec![gen_key(0), gen_key(1)],
            "a failed save may have held any of them"
        );
    }
}
