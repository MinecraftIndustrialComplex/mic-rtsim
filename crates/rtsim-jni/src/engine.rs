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
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::journal::{ObsSeq, OpId, Outbox, Reconcile, Replay, Verdict};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};

use crate::buffers::Buf;
use crate::orders::Order;
use crate::wire::{self, Rec, RecordWriter, Records, cmd, obs};
use crate::world::{Item, Member, World};

/// The industry engine behind the boundary: everything industry lives there, and the step calls
/// it at its observe, simulate, write and save points (docs/industry-boundary.md).
mod industry;
pub use industry::{IndustryStats, KEY_LINK as KEY_INDUSTRY_LINK, MODULE_INDUSTRY};
/// Phase 1 behind the boundary: the phase 1 world (settle, trade, travel, knowledge), its
/// observations, commands and pages (docs/boundary.md §4, §5).
mod phase1;
pub use phase1::{STATS as PHASE1_STATS, TABLE as PHASE1_TABLE};

/// Record bytes per buffer when Java asks for the default. PLACEHOLDER (the stub's 256 KiB): the
/// engine is to size buffers from the per-player puppet caps and per-tick quotas (PLAN.md §4.5).
pub const DEFAULT_CAPACITY: usize = 256 * 1024;

/// The engine's pages: table 1 is this crate's core state.
pub const KEY_META: PageKey = PageKey::new(1, 0);
pub const KEY_WORLD: PageKey = PageKey::new(1, 1);
const META_MAGIC: u32 = u32::from_le_bytes(*b"RTJM");
const META_VERSION: u16 = 1;

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
    /// Outbox commands to resend at the start of the session, before anything new.
    resend: VecDeque<(u64, OpId, TransferCmd)>,
    /// The engine's dictionary for this session: (kind, name) to id, ids per kind from 1.
    cmd_dict: HashMap<(u32, String), u32>,
    cmd_dict_next: HashMap<u32, u32>,
    dict_sent: HashSet<(u32, u32)>,
    /// The first observation sequence of this session; anything below is a replay.
    session_first: u64,
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
}

impl State {
    fn new(orders_rx: Receiver<Vec<Order>>, received_tx: Option<Sender<Received>>) -> Self {
        State {
            replay: Replay::new(ObsSeq(0)),
            outbox: Outbox::new(),
            world: World::default(),
            orders_rx,
            received_tx,
            units: VecDeque::new(),
            resend: VecDeque::new(),
            cmd_dict: HashMap::new(),
            cmd_dict_next: HashMap::new(),
            dict_sent: HashSet::new(),
            session_first: 1,
            pages_obs_seq: 0,
            last_applied_cmd: 0,
            rewind: None,
            game_time: 0,
            op_salt: std::collections::hash_map::RandomState::new(),
            op_counter: 0,
            industry: industry::Link::empty(),
            phase1: phase1::Link::empty(),
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
        | Order::IndustryCommand { .. } => 0,
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

/// One session of the engine.
pub struct Engine {
    /// Observation buffers 0 and 1, then command buffers 0 and 1.
    /// Shared with Java's buffer holds (`hold_buffers`): Java's direct `ByteBuffer`s point into
    /// this memory, so it is freed only once the engine and every Java buffer are gone.
    bufs: Arc<[Buf; 4]>,
    cfg: Config,
    state: Mutex<State>,
    store: Mutex<Option<PageStore>>,
    orders_tx: Mutex<Sender<Vec<Order>>>,
    received_rx: Mutex<Option<Receiver<Received>>>,
    pub stats: Stats,
    /// `through + 1` of a requested save; 0 when none is pending.
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
            store: Mutex::new(None),
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

    fn set_protocol_error(&self, msg: String) {
        log_error(&format!("engine protocol error: {msg}"));
        self.protocol_error.publish(Some(msg));
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
        let meta = store
            .get(KEY_META)
            .map_err(|e| format!("cannot read the engine's meta page: {e}"))?;
        let world = store
            .get(KEY_WORLD)
            .map_err(|e| format!("cannot read the engine's world page: {e}"))?;
        let (replay, outbox) = match meta {
            Some(b) => decode_meta(&b).map_err(|e| format!("the engine's meta page: {e}"))?,
            None => (Replay::new(ObsSeq(0)), Outbox::new()),
        };
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
            for k in industry::deletions(&store, &pages) {
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
        let p1link = phase1::Link::load(&mut store)?;
        let pages_obs = store.obs_seq();
        if replay.applied_through().0 != pages_obs {
            log_warn(&format!(
                "the pages' manifest says observation {pages_obs}, their replay state {}",
                replay.applied_through().0
            ));
        }
        *lock(&self.store) = Some(store);

        let mut st = lock(&self.state);
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
        st.pages_obs_seq = pages_obs;
        st.session_first = pages_obs + 1;
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
    pub fn request_save(&self, through: u64) {
        self.save_through
            .store(through.saturating_add(1), Ordering::Release);
    }

    /// Queues one unit of orders (decoded from `nOrder`).
    pub fn order(&self, unit: Vec<Order>) -> Result<(), String> {
        let mut unit = unit;
        if !self.cfg.test && unit.iter().any(Order::test_only) {
            return Err("a FAIL, P1_SPAWN_NEAR or P1_ADVANCE order needs an engine opened in TEST mode".into());
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
        if save && self.started.load(Ordering::Acquire) && !self.failed.load(Ordering::Acquire) {
            let (meta, world, industry, extra, obs_seq) = {
                let mut st = lock(&self.state);
                (
                    encode_meta(&st.replay, &st.outbox),
                    st.world.to_bytes(),
                    industry::snapshot(&st),
                    phase1::snapshot(&mut st),
                    st.replay.applied_through().0,
                )
            };
            let this = Arc::clone(self);
            result = self.run_io_waiting(move || this.commit(meta, world, industry, extra, obs_seq));
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

    fn spawn_io(&self, f: impl FnOnce() + Send + 'static) {
        match self.cfg.exec {
            Exec::Host => {
                if let Err(e) = rt::io(f) {
                    self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
                    log_error(&format!("the IO lane refused a save: {e}"));
                }
            }
            Exec::Inline => f(),
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
                    self.publish_empty(h);
                }
            }
        }
        self.stats
            .last_step_nanos
            .store(t0.elapsed().as_nanos() as u64, Ordering::Release);
        self.stats.steps_done.fetch_add(1, Ordering::AcqRel);
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
        // 1. Observations: published (acquire), read, consumed (release).
        let o = self.obs_buffer(h as usize);
        let published = o.published().load(Ordering::Acquire);
        if published != h {
            return Err(format!(
                "observation buffer {} holds hand-over {published}, not {h}",
                h & 1
            ));
        }
        for unit in st.orders_rx.try_iter().collect::<Vec<_>>() {
            self.take_unit(&mut st, unit);
        }
        self.read_observations(&mut st, h, o);
        o.consumed().store(h, Ordering::Release);

        // 2. Simulate: the industry engine and the phase 1 world run to the hand-over's game time
        // and decide; orders stand in for the other decisions. 3. A save due at this boundary.
        industry::simulate(&mut st);
        phase1::simulate(&mut st);
        let req = self.save_through.load(Ordering::Acquire);
        if self.save_at_step_end.swap(false, Ordering::AcqRel) {
            self.save_now(&mut st);
        } else if req != 0 && st.replay.applied_through().0 >= req - 1 {
            let _ = self
                .save_through
                .compare_exchange(req, 0, Ordering::AcqRel, Ordering::Acquire);
            self.save_now(&mut st);
        }

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
        for rec in &mut recs {
            self.observe(st, h, &rec);
        }
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
                st.world.session_start();
                st.world.module_mask = wire::u32_at(b, obs::session_start::MODULE_MASK);
                st.world.game_time = wire::i64_at(b, obs::session_start::GAME_TIME);
                let (game_time, mask) = (st.world.game_time, st.world.module_mask);
                industry::session_start(st, game_time, mask);
                let mask = st.world.module_mask;
                phase1::session_start(st, mask);
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
                Self::log_received(st, h, rec, 0, false);
            }
            k if obs::is_journaled(k) => self.journaled(st, h, rec),
            obs::WORLD_SAVED if rec.size() >= obs::world_saved::SIZE => {
                let durable = wire::u64_at(b, obs::world_saved::DURABLE_OBS_SEQ);
                st.replay.forget_ops_through(ObsSeq(durable));
                Self::log_received(st, h, rec, 0, false);
            }
            _ => {
                st.world.observe_state(rec);
                industry::observe_state(st, rec);
                phase1::observe_state(st, rec);
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
        // A save for a world save whose journal ended at `through` is taken right before the first
        // journaled record past it: the pages never get ahead of that world save.
        let req = self.save_through.load(Ordering::Acquire);
        if req != 0 && seq > req - 1 {
            let _ = self
                .save_through
                .compare_exchange(req, 0, Ordering::AcqRel, Ordering::Acquire);
            self.save_now(st);
        }
        let op = op_of(rec);
        let mut verdict = st.replay.admit(ObsSeq(seq), op);
        if let Verdict::Gap { expected, got } = verdict {
            self.stats.gaps.fetch_add(1, Ordering::AcqRel);
            log_warn(&format!(
                "observations {} to {} never arrived (a journal trimmed past the pages); going on from {}",
                expected.0,
                got.0 - 1,
                got.0
            ));
            st.replay.skip_to(got);
            verdict = st.replay.admit(ObsSeq(seq), op);
        }
        let skipped = verdict == Verdict::AlreadyApplied;
        Self::log_received(st, h, rec, seq, skipped);
        match verdict {
            Verdict::Apply if industry::claims(rec) => industry::journaled(st, rec),
            Verdict::Apply if phase1::claims(rec) => phase1::journaled(st, rec),
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
                phase1::after_core(st, rec);
            }
            Verdict::AlreadyApplied => {
                self.stats.stale_skipped.fetch_add(1, Ordering::AcqRel);
                st.world.refused_journaled(rec);
            }
            Verdict::DuplicateOp => {
                if rec.kind == obs::TRANSFER {
                    self.stats.op_duplicates.fetch_add(1, Ordering::AcqRel);
                }
                if industry::claims(rec) {
                    industry::answered_again(st, rec);
                }
                st.world.refused_journaled(rec);
            }
            Verdict::Gap { .. } => {
                self.set_protocol_error(format!(
                    "observation {seq} still a gap after skipping to it"
                ));
            }
        }
    }

    /// Snapshots the pages at this step boundary and writes them on the IO lane.
    fn save_now(self: &Arc<Self>, st: &mut State) {
        let _z = rt::zone!("rtsim.snapshot");
        let obs_seq = st.replay.applied_through().0;
        let meta = encode_meta(&st.replay, &st.outbox);
        let world = st.world.to_bytes();
        let industry = industry::snapshot(st);
        let extra = phase1::snapshot(st);
        let this = Arc::clone(self);
        self.spawn_io(move || this.commit(meta, world, industry, extra, obs_seq));
    }

    fn commit(
        &self,
        meta: Vec<u8>,
        world: Vec<u8>,
        industry: Vec<(PageKey, Vec<u8>)>,
        extra: Vec<(PageKey, Vec<u8>)>,
        obs_seq: u64,
    ) {
        let _z = rt::zone!("rtsim.save");
        if self.dead.load(Ordering::Acquire) {
            return;
        }
        let mut guard = lock(&self.store);
        let Some(store) = guard.as_mut() else { return };
        let mut batch = SaveBatch::new();
        batch.put(KEY_META, &meta);
        batch.put(KEY_WORLD, &world);
        for k in industry::deletions(store, &industry) {
            batch.delete(k);
        }
        for (k, b) in &extra {
            batch.put(*k, b);
        }
        for (k, b) in &industry {
            batch.put(*k, b);
        }
        match store.commit(batch, obs_seq) {
            Ok(_) => {
                self.durable.fetch_max(obs_seq, Ordering::AcqRel);
                self.stats.saves_done.fetch_add(1, Ordering::AcqRel);
            }
            Err(e) => {
                self.stats.save_failures.fetch_add(1, Ordering::AcqRel);
                log_error(&format!("saving the engine pages failed: {e}"));
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
        (highest.max(h), more)
    }
}

/// The lower of two next sequences, `None` standing for none at all.
fn lowest(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) => x,
        (None, y) => y,
    }
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
        _ => match industry::op_of(rec) {
            0 => phase1::op_of(rec),
            op => op,
        },
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
        Order::SaveAtNextStep | Order::IndustryThreat(_) | Order::IndustryCommand { .. } => None,
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
        let (r, o) = decode_meta(&encode_meta(&replay, &outbox)).unwrap();
        assert_eq!(r, replay);
        assert_eq!(o, outbox);
    }
}
