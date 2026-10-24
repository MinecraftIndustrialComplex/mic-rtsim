// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase 1 behind the boundary (docs/boundary.md §4, §5): the phase 1 world (`rtsim-world`:
//! settle, trade, travel and knowledge composed) fed by the bridge's phase 1 observations, and the
//! commands it decides written into the command buffers.
//!
//! The step calls this module at its observe, simulate, write and save points, beside the core
//! records (`engine.rs`) and the industry engine:
//!
//! - **Observations** ([`journaled`], [`observe_state`], [`after_core`]): VILLAGE_ADOPTED joins a
//!   village; TRADE, CARGO_CHANGED, HURT, RESCUE, BASE and HOUSE_PLACED become inputs of the world,
//!   applied once (the kernel's `Replay` filters sequences and the op ids of TRADE and
//!   CARGO_CHANGED); TAKEN and DIED of a phase 1 puppet take or kill its member; the census and
//!   gossip records are latest state. Everything that changes the world goes through the input log
//!   ([`log`]), which is what the pages keep (choice V6).
//! - **Simulate** ([`simulate`]): the world runs to the hand-over's game time; its commands
//!   (vanilla gossip, sentiment, houses, residents) are queued; caravans and bands near players are
//!   materialized, steered and despawned (C25 groups, C21 cargo in the chests); markets near
//!   players, or just traded at, get a MARKET snapshot.
//! - **Commands** ([`write_resends`], [`write_queue`]): MARKET, MERCHANT_OFFERS, WRITE_GOSSIP,
//!   SENTIMENT, PLACE_HOUSE, SPAWN_RESIDENT and the puppet commands. Those the bridge answers by op
//!   id are kept until it does and resent after a restart, in sequence order with the core's.
//! - **Pages** ([`Link::load`], [`snapshot`]; choices S1 to S6 in
//!   docs/overnight-choices-persist.md, PF17 in docs/choices-paging-followups.md): a snapshot of
//!   the world and the inputs since it (table 0x5000, in chunks of 1,024 after the meta page). The
//!   world pages through the engine's store: its meta page at table 0x5001 id 0, its market rows
//!   in table 0x5010 (`rtsim_world`), the link's part of the snapshot (the village index) in
//!   pieces after the world's meta page. A start opens the world's meta page and replays only
//!   that short tail, the market rows read as they are needed; a save writes the meta page and
//!   the tail's chunks that changed, and a new snapshot (the world's changed pages) when the tail
//!   is long or old, or the world's rows are over their budget ([`wants_snapshot`]). Pages this
//!   build cannot read follow [`when_pages_unreadable`].
//!
//! A test's scripted village ([`directive`], `ScriptVillage`) is left alone entirely: its
//! observations are not applied and no command of the engine's own names it.

mod catalog;
mod cmds;
mod view;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Reader, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::page::{PageError, PageKey, PageStore, SaveBatch};
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_kernel::rows::PageSource;
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::{Amount, GoodId, MILLI, OfferSide};
use rtsim_travel::caravan::{Body, GroupKind};
use rtsim_world::bases::CaravanRef;
use rtsim_world::works::{self, WorkCap, WorkDef, WorkGroup, WorkPlace, WorkVariant};
use rtsim_world::{CaravanId, Command, Visible, World, WorldConfig, identity_op};

use super::{State, log_info, log_warn, shared};
use crate::orders::{
    GossipCmd, ItemOrder, MarketCmd, MarketGood, MemberOrder, OfferCmd, OffersCmd, Order,
    PlaceHouseCmd, SentimentCmd, SpawnResidentCmd,
};
use crate::talk_wire::{TALKED, Talked, talked as talked_at};
use crate::wire::{self, Rec, RecordWriter, cmd, obs};

pub(super) use cmds::P1Cmd;
use rtsim_world::inputs as log;
use rtsim_world::inputs::{Input, Logged, Uuid};

/// The phase 1 pages: table 0x5000 ('P'; table 1 is the core's and the industry link's key, the
/// industry engine's are 0x4900 to 0x4904) holds the meta page and the tail's chunks; table
/// 0x5001 the snapshot: the world's meta page (id 0, PF12) and the link's part in pieces after
/// it. The world's market rows are its own table, 0x5010.
pub const TABLE: u16 = 0x5000;
pub const SNAPSHOT_TABLE: u16 = 0x5001;
pub const KEY_META: PageKey = PageKey::new(TABLE, 0);
pub const WORLD_META: PageKey = PageKey::new(SNAPSHOT_TABLE, 0);
/// Inputs per log page.
pub const CHUNK: usize = 1024;
/// Snapshot bytes per page. PLACEHOLDER (S2): a large world's snapshot is several MiB.
pub const PIECE: usize = 1 << 20;
/// A save takes a new snapshot once the tail holds this many inputs (S3, PLACEHOLDER: one chunk).
pub const TAIL_MAX_INPUTS: usize = CHUNK;
/// ... or once the snapshot is this many world ticks old (S3, PLACEHOLDER: one world day): a
/// start replays the world's time since its snapshot, inputs or not.
pub const TAIL_MAX_TICKS: u64 = 24_000;
pub const META_MAGIC: u32 = u32::from_le_bytes(*b"RTP1");
/// Version 2: a snapshot and a tail (S1). Version 3: the snapshot's world writes its market rows
/// as a row table (`World::save` RTW2, docs/choices-paging-modules.md). Version 4: the world pages
/// through the store (`World::save_paged`, PF17), the snapshot pieces hold the link's part only,
/// and the state hash is written in TEST mode only (0 otherwise, PF16). Before release a version
/// this build does not read is dropped (S5, [`when_pages_unreadable`]); there is no migration.
pub const META_VERSION: u16 = 4;
pub const SNAP_MAGIC: u32 = u32::from_le_bytes(*b"RTPS");

fn chunk_key(k: usize) -> PageKey {
    PageKey::new(TABLE, 1 + k as u64)
}

fn piece_key(k: usize) -> PageKey {
    PageKey::new(SNAPSHOT_TABLE, 1 + k as u64)
}

/// The phase 1 world's configuration, goods and recipes: what [`fresh_world`] builds with and a
/// snapshot is read back with.
fn world_parts() -> (
    WorldConfig,
    rtsim_trade::Goods,
    rtsim_trade::labour::RecipeBook,
) {
    let c = catalog::Catalog::new();
    (WorldConfig::new(WORLD_SEED), c.goods, c.book)
}

/// Java's module bits (`config/Module.java` ordinals) phase 1 runs under: knowledge,
/// settlements, trade and travel. Phase 1 runs only with all four on: its world composes them, and
/// trade or travel off restores vanilla's wandering traders (C25), which the sim's caravans must
/// not double (choice PR13).
pub const MODULES_PHASE1: u32 = 0b1111;

/// Whether a module mask runs phase 1.
pub fn phase1_on(mask: u32) -> bool {
    mask & MODULES_PHASE1 == MODULES_PHASE1
}

/// The phase 1 world's seed. PLACEHOLDER (V5): the bridge does not send the Minecraft world's.
pub const WORLD_SEED: u64 = 0x6d69_635f_7031;

/// Phase 1 agent ids: bit 62 over 55 bits of the caravan's or band's identity. Test agents sit
/// below 2^40, `/rtsim stub` ones at 2^40 and up.
pub const AGENT_P1: u64 = 1 << 62;

// ---- tunables: every one a PLACEHOLDER (choice V8) ----------------------------------------------

/// Game ticks between passes over players for caravans and bands to materialize, steer, despawn.
const INTEREST_TICKS: i64 = 20;
/// A traveller this close to a player (blocks) materializes: inside the entity-ticking area of a
/// default view distance, so its chunks tick.
const SPAWN_RADIUS: u32 = 96;
/// A puppet farther than this from every player is captured back.
const DESPAWN_RADIUS: f64 = 144.0;
/// Groups materialized per player, and in all.
const MAX_PER_PLAYER: usize = 2;
const MAX_EMBODIED: usize = 16;
/// Game ticks between steering commands for one group.
const STEER_TICKS: i64 = 100;
const STEER_SPEED: f32 = 0.6;
/// Game ticks between market passes; a village this close to a player gets MARKET snapshots.
const MARKET_TICKS: i64 = 100;
const MARKET_RADIUS: f64 = 192.0;
/// The market pass's cell edge, blocks.
const MARKET_CELL: i64 = 256;
/// A player not reported for this many game ticks is gone.
const PLAYER_STALE: i64 = 200;
/// Epochs skipped at every start: more than a session hands out between two saves.
const EPOCH_STRIDE: u32 = 1 << 20;
/// World ticks before a group whose spawn was rejected is tried again.
const BACKOFF_TICKS: u64 = 6_000;
/// Vanilla drops a gossip entry below 2 (boundary.md §4.3): smaller amounts are sent as 2.
const GOSSIP_MIN: i32 = 2;
/// Price levels a MARKET carries besides the fair one (G31, Ben: "Inside the sim quote"): a
/// player whose standing with the village (vanilla's reputation with that villager, or a ribbit
/// village's sentiment) is at or below a threshold pays that level's markup (rtsim-trade
/// `markup_for` at the threshold). Never better than fair (E7). TUNABLE, PLACEHOLDER: -25 to
/// -100 by 25 (vanilla's iron golems turn on a player at -100).
pub const PRICE_LEVELS: [i32; 4] = [-25, -50, -75, -100];
/// Bundles per MARKET ladder: trades a player can make in a row (a shift-click) before the next
/// MARKET. PLACEHOLDER: 8.
pub const LADDER_DEPTH: usize = 8;
/// A caravan this close to an end of its road (blocks) stands at that market: at a border market
/// it exchanges coin there (P21, choice PR11).
const AT_MARKET_RADIUS: f64 = 96.0;

/// `nPhase1Stats` indexes (NativeEngine's P1_* constants).
pub mod stat {
    pub const VILLAGES: usize = 0;
    pub const EMBODIED: usize = 1;
    pub const RESIDENT_TRADES: usize = 2;
    pub const CARAVAN_TRADES: usize = 3;
    pub const TRADES_REFUSED: usize = 4;
    pub const ROBBERIES: usize = 5;
    pub const REPORTS: usize = 6;
    pub const GOSSIP_SENT: usize = 7;
    pub const MARKETS_SENT: usize = 8;
    pub const OFFERS_SENT: usize = 9;
    pub const HOUSES_SENT: usize = 10;
    pub const RESIDENTS_SENT: usize = 11;
    pub const SENTIMENTS_SENT: usize = 12;
    pub const INPUTS: usize = 13;
    pub const SPAWNS_SENT: usize = 14;
    pub const WORLD_TICK: usize = 15;
    /// Works the world bought from village purses (SINK.md), this session's world.
    pub const WORKS_BOUGHT: usize = 16;
    /// Bought works the bridge answered placed (APPLIED or DUPLICATE).
    pub const WORKS_PLACED: usize = 17;
    /// Bought works the bridge answered NOT_FREE or FAILED: the kind is full (MS-11).
    pub const WORKS_REFUSED: usize = 18;
    /// Tributes villages paid to dragons' lairs (MS-5).
    pub const TRIBUTES: usize = 19;
}
pub const STATS: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PuppetOf {
    /// A caravan row, by its handle's bits.
    Caravan(u64),
    Band(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PState {
    Spawning,
    Embodied,
    Despawning,
}

#[derive(Clone, Debug)]
struct Puppet {
    epoch: u32,
    of: PuppetOf,
    state: PState,
    frame: u16,
    pos: [f64; 3],
    /// A test's group: never steered or despawned by distance.
    pinned: bool,
    /// The leg end the caravan row had at spawn: when the sim's caravan moves on (arrives,
    /// turns back), the puppet is captured back.
    leg_end: u64,
    last_steer: i64,
    /// The market its offers were made at (P21: a border market adds coin exchange).
    offers_at: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct Seen {
    frame: u16,
    pos: [f64; 3],
    at: i64,
}

/// A test's P1_SPAWN_NEAR, done at the next simulate.
#[derive(Clone, Debug)]
struct SpawnNear {
    agent: u64,
    epoch: u32,
    dimension: String,
    pos: [f64; 3],
    radius: u32,
    search: [f64; 2],
}

/// The phase 1 state the step keeps (`State::phase1`).
pub(super) struct Link {
    pub(super) world: World,
    cat: catalog::Catalog,
    /// Java's village and base ids to world nodes.
    nodes: BTreeMap<u64, usize>,
    /// Village and base ids by (frame, cell) of `MARKET_CELL` blocks: the market pass looks up
    /// the cells around players, never every village (SCALE.md §0).
    cells: BTreeMap<(u16, i64, i64), Vec<u64>>,
    /// Villages a test scripts (saved: their adoption may replay in a later session).
    scripted: BTreeSet<u64>,
    /// The inputs since the snapshot (the tail; S1).
    log: Vec<Logged>,
    /// What is on disk: the snapshot generation and how many full tail chunks of it, packed as
    /// `gen << 32 | chunks`. Only a commit that succeeded moves it ([`Saved::done`]): a save whose
    /// pages never reached the disk must not count.
    durable: Arc<AtomicU64>,
    /// The generation of the snapshot `log` follows: 0 is the fresh world, which needs no pages.
    snap_gen: u32,
    /// The snapshot's pieces and bytes (0 for the fresh world), and its world tick.
    snap_pieces: usize,
    snap_len: usize,
    snap_at: u64,
    /// Inputs before the snapshot, for the INPUTS counter.
    base_inputs: u64,
    /// The world changed outside the input log since the snapshot (the dragons, which share it):
    /// the tail alone no longer rebuilds it, so the next save takes a snapshot (S4).
    tainted: bool,
    /// World ticks ahead of game time (a test's P1_ADVANCE).
    pub(super) offset: u64,
    /// Commands the bridge answers by op id, kept until it does: (sequence, op, command).
    outbox: Vec<(u64, u64, P1Cmd)>,
    resend: VecDeque<(u64, u64, P1Cmd)>,
    queue: VecDeque<P1Cmd>,
    puppets: BTreeMap<u64, Puppet>,
    /// Which caravan row each phase 1 agent is (agent to the row's handle bits), saved with the
    /// pages: a record about a puppet can arrive in a later session (replayed from the journal
    /// after a crash, or carried by a chunk, docs/boundary.md §3.1), when `puppets` no longer has
    /// it. Rows that are gone are dropped at each save.
    agents: BTreeMap<u64, u64>,
    backoff: BTreeMap<u64, u64>,
    next_epoch: u32,
    players: BTreeMap<Uuid, Seen>,
    markets_sent: BTreeMap<u64, u64>,
    market_dirty: BTreeSet<u64>,
    censused: BTreeSet<u64>,
    last_interest: i64,
    last_market: i64,
    spawn_near: Vec<SpawnNear>,
    /// U10 (Ben: "Ignore everything"): with phase 1 switched off, its records are admitted by
    /// `Replay` and then ignored, its world does not run, nothing new is decided or materialized,
    /// and on again it catches up at once ([`set_enabled`]).
    enabled: bool,
    /// Records ignored while phase 1 was switched off (U10).
    pub(super) ignored: u64,
    pub(super) stats: [u64; STATS],
    /// Captured cargo that did not match the sim's row at a despawn (logged, not reconciled).
    pub(super) capture_mismatches: u64,
    /// The last load replayed the log to a state other than the one saved (`state_hash`): the
    /// world is still the replayed one, but the build changed what the inputs mean.
    pub(super) replay_diverged: Option<(u64, u64)>,
    /// The last load dropped pages it could not read ([`when_pages_unreadable`]).
    pub(super) dropped_pages: bool,
    /// The phase 1 tables hold pages this build does not use: `start` deletes them before the
    /// fresh world could read one (`paging::Pager::purge`).
    pub(super) purge: bool,
    /// The world came back from pages (rather than starting fresh).
    restored: bool,
}

fn fresh_world() -> (World, catalog::Catalog) {
    let cat = catalog::Catalog::new();
    let (cfg, goods, book) = world_parts();
    let world = World::build(cfg, goods, book, Vec::new());
    (world, cat)
}

/// **The policy for phase 1 pages this build cannot read** (S5; open on Ben's sheet, its
/// recommendation taken): drop them and start a fresh phase 1 world, with a loud log, as the
/// industry link does with its own (W23). The core's pages and the rest of the engine start as
/// usual; the next save replaces the dropped pages. Before release there is no migration (Ben's
/// rule), so a page another build wrote is simply not this build's.
///
/// To make an unreadable page stop the whole engine instead (the behaviour before), return
/// `Err(why)` here: `start` then fails with it.
fn when_pages_unreadable(
    why: &str,
    source: &Arc<dyn PageSource>,
    budget: usize,
) -> Result<Link, String> {
    super::log_error(&format!(
        "PHASE 1 PAGES DROPPED: {why}. This build cannot read them (another build's format, or a damaged page); \
         the phase 1 world starts afresh (villages join again as their chunks load) and the next save replaces them"
    ));
    let mut l = Link::fresh(source, budget);
    l.dropped_pages = true;
    l.purge = true;
    Ok(l)
}

impl Link {
    pub(super) fn empty() -> Link {
        let (world, cat) = fresh_world();
        Link {
            world,
            cat,
            nodes: BTreeMap::new(),
            cells: BTreeMap::new(),
            scripted: BTreeSet::new(),
            log: Vec::new(),
            durable: Arc::new(AtomicU64::new(0)),
            snap_gen: 0,
            snap_pieces: 0,
            snap_len: 0,
            snap_at: 0,
            base_inputs: 0,
            tainted: false,
            offset: 0,
            outbox: Vec::new(),
            resend: VecDeque::new(),
            queue: VecDeque::new(),
            puppets: BTreeMap::new(),
            agents: BTreeMap::new(),
            backoff: BTreeMap::new(),
            next_epoch: 1,
            players: BTreeMap::new(),
            markets_sent: BTreeMap::new(),
            market_dirty: BTreeSet::new(),
            censused: BTreeSet::new(),
            last_interest: i64::MIN,
            last_market: i64::MIN,
            spawn_near: Vec::new(),
            enabled: true,
            ignored: 0,
            stats: [0; STATS],
            capture_mismatches: 0,
            replay_diverged: None,
            dropped_pages: false,
            purge: false,
            restored: false,
        }
    }

    /// A fresh world paging its market rows through the engine's store under `budget` bytes.
    fn fresh(source: &Arc<dyn PageSource>, budget: usize) -> Link {
        let mut l = Link::empty();
        l.world.attach_store(Arc::clone(source), budget);
        l
    }

    /// Loads the phase 1 pages: the world from its snapshot (its meta page; the market rows are
    /// read as they are needed, under `budget` bytes), then the tail of inputs since it replayed.
    /// `test`: check the replayed world against the state hash the save wrote (PF16). Pages this
    /// build cannot read go to [`when_pages_unreadable`].
    pub(super) fn load(
        source: &Arc<dyn PageSource>,
        budget: usize,
        test: bool,
    ) -> Result<Link, String> {
        match Link::read_pages(source, budget, test) {
            Ok(l) => Ok(l),
            Err(why) => when_pages_unreadable(&why, source, budget),
        }
    }

    fn read_pages(source: &Arc<dyn PageSource>, budget: usize, test: bool) -> Result<Link, String> {
        let mut link = Link::fresh(source, budget);
        let Some(meta) = source
            .read(KEY_META)
            .map_err(|e| format!("cannot read the phase 1 meta page: {e}"))?
        else {
            if super::paging::holds_pages(&**source, super::paging::tables(TABLE)) {
                return Err("phase 1 pages without their meta page".into());
            }
            return Ok(link);
        };
        let m = decode_meta(&meta).map_err(|e| format!("the phase 1 meta page: {e}"))?;
        link.restored = true;
        let t0 = std::time::Instant::now();
        if m.snap_pieces > 0 {
            let mut blob = Vec::with_capacity(m.snap_len);
            for k in 0..m.snap_pieces {
                let b = source
                    .read(piece_key(k))
                    .map_err(|e| format!("cannot read phase 1 snapshot page {k}: {e}"))?
                    .ok_or_else(|| format!("phase 1 snapshot page {k} is missing"))?;
                blob.extend_from_slice(&b);
            }
            if blob.len() != m.snap_len {
                return Err(format!(
                    "the phase 1 snapshot holds {} bytes, its meta page says {}",
                    blob.len(),
                    m.snap_len
                ));
            }
            let (nodes, censused) =
                decode_snapshot(&blob).map_err(|e| format!("the phase 1 snapshot: {e}"))?;
            let (cfg, goods, book) = world_parts();
            let world = World::open_paged(Arc::clone(source), budget, WORLD_META, cfg, goods, book)
                .map_err(|e| format!("the phase 1 snapshot's world: {e}"))?;
            if world.now().0 != m.snap_at {
                return Err(format!(
                    "the phase 1 snapshot is at world tick {}, its meta page says {}",
                    world.now().0,
                    m.snap_at
                ));
            }
            if let Some((k, i)) = nodes
                .iter()
                .find(|(k, i)| world.nodes.get(**i).is_none_or(|n| n.key != **k))
            {
                return Err(format!(
                    "the phase 1 snapshot's village {k:#x} is node {i}, which the world does not have"
                ));
            }
            link.world = world;
            link.nodes = nodes;
            link.censused = censused;
        } else if super::paging::holds_pages(&**source, SNAPSHOT_TABLE..=TABLE + 0x1f) {
            return Err("phase 1 world pages without a snapshot".into());
        }
        let t_snap = t0.elapsed();
        let chunks = m.log_len.div_ceil(CHUNK);
        let mut log = Vec::with_capacity(m.log_len);
        for k in 0..chunks {
            let b = source
                .read(chunk_key(k))
                .map_err(|e| format!("cannot read phase 1 log page {k}: {e}"))?
                .ok_or_else(|| format!("phase 1 log page {k} is missing"))?;
            let mut r = Reader::new(&b);
            while !r.is_empty() {
                log.push(Logged::decode(&mut r).map_err(|e| format!("phase 1 log page {k}: {e}"))?);
            }
        }
        if log.len() != m.log_len {
            return Err(format!(
                "the phase 1 log holds {} inputs, its meta page says {}",
                log.len(),
                m.log_len
            ));
        }
        for l in &log {
            log::apply(&mut link.world, &mut link.nodes, &l.input, l.at);
        }
        link.world.run_until(Tick(m.now));
        // Commands the replay re-decided were decided before the save: those still unanswered
        // are in the kept outbox, the rest (latest wins) are decided again this session.
        let _ = link.world.take_commands();
        // The hash reads every market row: TEST mode only (PF16), where the save wrote one.
        let hash = (test && m.hash != 0).then(|| link.world.state_hash());
        if let Some(hash) = hash.filter(|h| *h != m.hash) {
            link.replay_diverged = Some((m.hash, hash));
            log_warn(&format!(
                "the phase 1 world replayed to state {hash:016x}, the save had {:016x}: this build reads the inputs differently (the replayed world stands)",
                m.hash
            ));
        }
        log_info(&format!(
            "phase 1 world restored to world tick {}: snapshot of {} bytes at tick {} read in {} ms, then {} inputs replayed, {} ms in all: {} nodes, {} caravans{}",
            m.now,
            m.snap_len,
            m.snap_at,
            t_snap.as_millis(),
            log.len(),
            t0.elapsed().as_millis(),
            link.world.nodes.len(),
            link.world.caravans.len(),
            hash.map_or(String::new(), |h| format!(", state {h:016x}"))
        ));
        link.snap_gen = 1;
        link.snap_pieces = m.snap_pieces;
        link.snap_len = m.snap_len;
        link.snap_at = m.snap_at;
        link.base_inputs = m.base_inputs;
        link.durable
            .store(pack(link.snap_gen, log.len() / CHUNK), Ordering::Release);
        // Villages whose first census is in the tail (the snapshot carried the earlier ones;
        // choice V9: later ones only raise).
        link.censused
            .extend(log.iter().filter_map(|l| match l.input {
                Input::Population { key, .. } => Some(key),
                _ => None,
            }));
        link.log = log;
        let keys: Vec<u64> = link.nodes.keys().copied().collect();
        for key in keys {
            link.index_node(key);
        }
        link.offset = m.offset;
        // Epochs handed out after the save were lost with the crash, and a record of that
        // session may still be replayed: start well past them (choice V26).
        link.next_epoch = m.next_epoch.max(1).wrapping_add(EPOCH_STRIDE).max(1);
        link.scripted = m.scripted;
        link.outbox = m.outbox;
        link.agents = m.agents;
        // Kept commands decided before the save but not yet written: the replay above decided
        // them again and threw them away, so they go out from here (their op ids are their
        // identity: one Java already applied is answered DUPLICATE).
        link.queue = m.queued.into();
        link.stats[stat::INPUTS] = link.base_inputs + link.log.len() as u64;
        link.stats[stat::VILLAGES] = link.nodes.len() as u64;
        Ok(link)
    }

    /// Whether the world came back from its pages: the dragons' page is only good with the world
    /// it was saved with.
    pub(super) fn world_restored(&self) -> bool {
        self.restored && !self.dropped_pages
    }

    /// The world changed outside the input log (the dragons, which share it): the next save takes
    /// a snapshot (S4).
    pub(super) fn taint(&mut self) {
        self.tainted = true;
    }

    /// After `start`: kept commands Java already applied (at or below its `lastAppliedCmdSeq`)
    /// leave; the rest are resent first, in sequence order.
    pub(super) fn resume(&mut self, last_applied_cmd: u64) {
        self.outbox.retain(|(seq, _, _)| *seq > last_applied_cmd);
        self.outbox.sort_by_key(|e| e.0);
        self.resend = self.outbox.iter().cloned().collect();
    }

    /// The world tick for a game time (the test's offset added).
    fn wt(&self, game_time: i64) -> u64 {
        (game_time.max(0) as u64).saturating_add(self.offset)
    }

    /// Applies an input at a world tick no earlier than the world's, and logs it.
    fn input(&mut self, input: Input, at: u64) -> log::Applied {
        let at = at.max(self.world.now().0);
        let out = log::apply(&mut self.world, &mut self.nodes, &input, at);
        if let (Input::Join { key, .. } | Input::MarkBase { key, .. }, Some(_)) = (&input, out.node)
        {
            self.index_node(*key);
        }
        self.log.push(Logged { at, input });
        self.stats[stat::INPUTS] = self.base_inputs + self.log.len() as u64;
        out
    }

    /// Puts a village or base into the market pass's cells (once).
    fn index_node(&mut self, key: u64) {
        let Some(&i) = self.nodes.get(&key) else {
            return;
        };
        let p = self.world.nodes[i].pos;
        let cell = (
            p.frame,
            i64::from(p.x).div_euclid(MARKET_CELL),
            i64::from(p.z).div_euclid(MARKET_CELL),
        );
        let v = self.cells.entry(cell).or_default();
        if !v.contains(&key) {
            v.push(key);
        }
    }

    fn java_dim(st_world: &crate::world::World, frame: u16) -> Option<String> {
        st_world
            .java_dict
            .get(&(wire::DICT_DIMENSION, u32::from(frame)))
            .cloned()
    }

    /// Queues a command; a latest-wins one replaces an older one still queued for its target.
    fn push(&mut self, c: P1Cmd) {
        self.queue.retain(|q| !c.supersedes(q));
        self.queue.push_back(c);
    }
}

// ---- the meta page and the snapshot ---------------------------------------------------------------

struct Meta {
    now: u64,
    hash: u64,
    offset: u64,
    next_epoch: u32,
    log_len: usize,
    snap_pieces: usize,
    snap_len: usize,
    snap_at: u64,
    base_inputs: u64,
    scripted: BTreeSet<u64>,
    outbox: Vec<(u64, u64, P1Cmd)>,
    agents: BTreeMap<u64, u64>,
    queued: Vec<P1Cmd>,
}

/// The meta page, written at every save: where the world is (tick and hash, for the replay
/// check), where its snapshot and tail are, and the link's own state: scripted villages, the
/// kept outbox, the caravan identity map (review 3, P2) and the kept commands still queued
/// (review 3, P1).
/// The world's state hash is a whole pass over its rows, so only a TEST engine writes it (`test`;
/// 0 otherwise, PF16).
fn encode_meta(l: &Link, test: bool) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u32(META_MAGIC);
    w.u16(META_VERSION);
    w.u64(l.world.now().0);
    w.u64(if test { l.world.state_hash() } else { 0 });
    w.u64(l.offset);
    w.u32(l.next_epoch);
    w.var_u64(l.log.len() as u64);
    w.var_u64(l.snap_pieces as u64);
    w.var_u64(l.snap_len as u64);
    w.u64(l.snap_at);
    w.u64(l.base_inputs);
    w.var_u64(l.scripted.len() as u64);
    l.scripted.iter().for_each(|v| w.u64(*v));
    w.var_u64(l.outbox.len() as u64);
    for (seq, op, c) in &l.outbox {
        w.u64(*seq);
        w.u64(*op);
        cmds::encode_kept(&mut w, c);
    }
    w.var_u64(l.agents.len() as u64);
    for (a, bits) in &l.agents {
        w.u64(*a);
        w.u64(*bits);
    }
    let queued: Vec<&P1Cmd> = l.queue.iter().filter(|c| c.kept_op().is_some()).collect();
    w.var_u64(queued.len() as u64);
    for c in queued {
        cmds::encode_kept(&mut w, c);
    }
    w.into_bytes()
}

fn decode_meta(b: &[u8]) -> codec::Result<Meta> {
    let mut r = Reader::new(b);
    if r.u32()? != META_MAGIC {
        return Err(CodecError::Invalid("not a phase 1 meta page".into()));
    }
    let v = r.u16()?;
    if v != META_VERSION {
        return Err(CodecError::Invalid(format!(
            "phase 1 meta page version {v}, this build reads {META_VERSION}"
        )));
    }
    let now = r.u64()?;
    let hash = r.u64()?;
    let offset = r.u64()?;
    let next_epoch = r.u32()?;
    let log_len = r.var_u64()? as usize;
    let snap_pieces = r.var_u64()? as usize;
    let snap_len = r.var_u64()? as usize;
    let snap_at = r.u64()?;
    let base_inputs = r.u64()?;
    if snap_len.div_ceil(PIECE) != snap_pieces {
        return Err(CodecError::Invalid(format!(
            "a snapshot of {snap_len} bytes in {snap_pieces} pages"
        )));
    }
    let mut scripted = BTreeSet::new();
    for _ in 0..r.len_prefix(8)? {
        scripted.insert(r.u64()?);
    }
    let mut outbox = Vec::new();
    for _ in 0..r.len_prefix(17)? {
        let seq = r.u64()?;
        let op = r.u64()?;
        outbox.push((seq, op, cmds::decode_kept(&mut r)?));
    }
    let mut agents = BTreeMap::new();
    for _ in 0..r.len_prefix(16)? {
        let a = r.u64()?;
        agents.insert(a, r.u64()?);
    }
    let mut queued = Vec::new();
    for _ in 0..r.len_prefix(1)? {
        queued.push(cmds::decode_kept(&mut r)?);
    }
    r.finish()?;
    Ok(Meta {
        now,
        hash,
        offset,
        next_epoch,
        log_len,
        snap_pieces,
        snap_len,
        snap_at,
        base_inputs,
        scripted,
        outbox,
        agents,
        queued,
    })
}

/// The link's part of the snapshot: its village index and censused villages as of the snapshot.
/// The world's part is its own pages (`World::save_paged`, the meta page at [`WORLD_META`]).
/// Everything else of the link is in the meta page, written every save.
fn encode_snapshot(l: &Link) -> Vec<u8> {
    let mut w = Writer::with_capacity(64 + l.nodes.len() * 24 + l.censused.len() * 8);
    w.u32(SNAP_MAGIC);
    w.var_u64(l.nodes.len() as u64);
    for (k, i) in &l.nodes {
        w.u64(*k);
        w.var_u64(*i as u64);
    }
    w.var_u64(l.censused.len() as u64);
    l.censused.iter().for_each(|k| w.u64(*k));
    w.into_bytes()
}

type Restored = (BTreeMap<u64, usize>, BTreeSet<u64>);

fn decode_snapshot(b: &[u8]) -> codec::Result<Restored> {
    let mut r = Reader::new(b);
    if r.u32()? != SNAP_MAGIC {
        return Err(CodecError::Invalid("not a phase 1 snapshot".into()));
    }
    let mut nodes = BTreeMap::new();
    for _ in 0..r.len_prefix(9)? {
        let k = r.u64()?;
        nodes.insert(k, r.var_u64()? as usize);
    }
    let mut censused = BTreeSet::new();
    for _ in 0..r.len_prefix(8)? {
        censused.insert(r.u64()?);
    }
    r.finish()?;
    Ok((nodes, censused))
}

fn pack(generation: u32, chunks: usize) -> u64 {
    u64::from(generation) << 32 | chunks.min(u32::MAX as usize) as u64
}

fn unpack(v: u64) -> (u32, usize) {
    ((v >> 32) as u32, (v & 0xffff_ffff) as usize)
}

/// What a commit that wrote a phase 1 save reports when it succeeded, and which phase 1 pages
/// that save leaves live (the rest are deleted with it).
pub(super) struct Saved {
    durable: Arc<AtomicU64>,
    generation: u32,
    chunks: usize,
    live: Vec<PageKey>,
}

impl Saved {
    /// The save's pages are on disk: its snapshot and its full tail chunks never need writing
    /// again.
    pub(super) fn done(&self) {
        self.durable
            .fetch_max(pack(self.generation, self.chunks), Ordering::AcqRel);
    }
}

/// The phase 1 pages in the store that a save leaves behind: an older snapshot's pieces and tail
/// chunks, and pages a build could not read ([`when_pages_unreadable`]). The world's market
/// table is the world's own (`World::save_paged` deletes what it drops). Reads only these two
/// tables' index blocks.
pub(super) fn deletions(store: &PageStore, saved: &Saved) -> Result<Vec<PageKey>, PageError> {
    let live: BTreeSet<PageKey> = saved.live.iter().copied().collect();
    let mut out = Vec::new();
    for table in [TABLE, SNAPSHOT_TABLE] {
        for id in store.table_ids(table)? {
            let k = PageKey::new(table, id);
            if !live.contains(&k) {
                out.push(k);
            }
        }
    }
    Ok(out)
}

/// Whether this save takes a new snapshot (S3, S4): the one the tail follows never reached the
/// disk (a failed or unfinished commit), the world changed outside the log, the tail is long or
/// old, or the world's market rows are over their budget with every resident page changed
/// (PF17: the world's pages are written only with a snapshot, so only a snapshot frees them).
fn wants_snapshot(l: &Link) -> bool {
    let (generation, _) = unpack(l.durable.load(Ordering::Acquire));
    generation != l.snap_gen
        || l.tainted
        || l.log.len() >= TAIL_MAX_INPUTS
        || l.world.now().0.saturating_sub(l.snap_at) >= TAIL_MAX_TICKS
        || l.world.needs_save()
}

/// The pages a save writes, into `batch`: the meta page, and either a new snapshot (the world's
/// meta page and its market pages changed since the last snapshot, and the link's part; the tail
/// starts over) or every tail chunk not yet on disk in full. The commit calls [`Saved::done`]
/// once they are, and the step hands the world's ticket back ([`world_done`]). `test`: the meta
/// page carries the world's state hash (PF16).
pub(super) fn snapshot(
    st: &mut State,
    batch: &mut SaveBatch,
    test: bool,
) -> (Saved, Option<SaveTicket>) {
    // Commands the world decided and the link has not taken yet (a save between an input and
    // the next take, e.g. after a census) go to the queue now, so the kept ones are in the meta
    // page and the snapshot's own outbox is empty: a start discards what its replay re-decides.
    take_world_commands(st);
    let l = &mut st.phase1;
    let caravans = &l.world.caravans;
    l.agents
        .retain(|_, bits| caravans.get(Handle::from_bits(*bits)).is_some());
    let mut ticket = None;
    if wants_snapshot(l) {
        let blob = encode_snapshot(l);
        ticket = Some(l.world.save_paged(batch, WORLD_META));
        l.snap_gen = l.snap_gen.wrapping_add(1).max(1);
        l.snap_len = blob.len();
        l.snap_pieces = blob.len().div_ceil(PIECE);
        l.snap_at = l.world.now().0;
        l.base_inputs += l.log.len() as u64;
        l.log.clear();
        l.tainted = false;
        for (k, piece) in blob.chunks(PIECE).enumerate() {
            batch.put(piece_key(k), piece);
        }
    } else {
        let (_, clean) = unpack(l.durable.load(Ordering::Acquire));
        let chunks = l.log.len().div_ceil(CHUNK);
        for k in clean..chunks {
            let mut w = Writer::with_capacity(CHUNK * 24);
            for e in &l.log[k * CHUNK..((k + 1) * CHUNK).min(l.log.len())] {
                e.encode(&mut w);
            }
            batch.put(chunk_key(k), w.as_bytes());
        }
    }
    batch.put(KEY_META, &encode_meta(l, test));
    let live = [KEY_META, WORLD_META]
        .into_iter()
        .chain((0..l.log.len().div_ceil(CHUNK)).map(chunk_key))
        .chain((0..l.snap_pieces).map(piece_key))
        .collect();
    let saved = Saved {
        durable: Arc::clone(&l.durable),
        generation: l.snap_gen,
        chunks: l.log.len() / CHUNK,
        live,
    };
    (saved, ticket)
}

/// The world's ticket back from the IO lane: its pages are on disk (`ok`), or the commit failed
/// and the next snapshot writes them again (the failed snapshot's generation never became
/// durable, so the next save takes one, [`wants_snapshot`]).
pub(super) fn world_done(st: &mut State, t: Option<SaveTicket>, ok: bool) {
    if let Some(t) = t {
        if ok {
            st.phase1.world.saved(t);
        } else {
            st.phase1.world.save_failed(t);
        }
    }
}

/// The phase 1 world's tick for a game time (the test offset added): the clock the dragons,
/// who share the world, run on too.
pub(super) fn world_tick(st: &State, game_time: i64) -> u64 {
    st.phase1.wt(game_time)
}

/// The phase 1 world's good for a Minecraft item id, if the catalogue trades it.
pub(super) fn catalog_good(st: &State, item: &str) -> Option<GoodId> {
    st.phase1.cat.good(item)
}

/// The Minecraft item id of a phase 1 world's good, if the catalogue has it.
pub(super) fn catalog_item(st: &State, g: GoodId) -> Option<&'static str> {
    st.phase1.cat.try_item(g)
}

// ---- hooks: session and latest-wins records -------------------------------------------------------

/// SESSION_START: puppets never outlive a session, and Java forgot the latest-wins snapshots.
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    let l = &mut st.phase1;
    l.puppets.clear();
    l.players.clear();
    l.markets_sent.clear();
    l.spawn_near.clear();
    l.queue.retain(|c| c.kept_op().is_some());
    l.enabled = phase1_on(module_mask);
    l.last_interest = i64::MIN;
    l.last_market = i64::MIN;
}

/// The switch changed (MODULES; U10, Ben: "Ignore everything"). **Off:** every phase 1 puppet is
/// captured back, one last write, so no caravan trades or is robbed in game while the sim ignores
/// it; what was queued for this session (MARKET, MERCHANT_OFFERS, SENTIMENT, spawns, steering)
/// is dropped. Commands the bridge answers by op id (gossip, houses, residents) decided before the
/// switch wait unsequenced until it is on again ([`write_queue_off`]); resends of ones already
/// written still go out with their sequences, and answers are still taken: that is the protocol
/// finishing, not the module running (choice PR13). **On:** the world runs to now at the next
/// simulate (its missed events at once) and materializes again.
fn set_enabled(st: &mut State, on: bool) {
    let l = &mut st.phase1;
    let was = l.enabled;
    l.enabled = on;
    if was && !on {
        l.queue
            .retain(|c| c.kept_op().is_some() || matches!(c, P1Cmd::Despawn { .. }));
        let mut despawn = Vec::new();
        for (&agent, p) in l.puppets.iter_mut() {
            match p.state {
                PState::Embodied | PState::Spawning => {
                    p.state = PState::Despawning;
                    despawn.push((agent, p.epoch));
                }
                PState::Despawning => {}
            }
        }
        for (agent, epoch) in despawn {
            l.push(P1Cmd::Despawn {
                agent,
                epoch,
                reason: obs::dematerialized::COMMAND,
            });
        }
        l.spawn_near.clear();
        l.market_dirty.clear();
    }
    if on && !was {
        l.last_interest = i64::MIN;
        l.last_market = i64::MIN;
        l.markets_sent.clear();
    }
}

pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            set_enabled(st, phase1_on(wire::u32_at(b, obs::modules::MASK)));
        }
        obs::PLAYER if rec.size() >= obs::player::SIZE => {
            let key = (
                wire::u64_at(b, obs::player::UUID_MOST),
                wire::u64_at(b, obs::player::UUID_LEAST),
            );
            let frame = rec.aux as u16;
            // Canonical at the door (topology choice TP4).
            let (x, z) = st.topology.shared.of(frame).canonical_point(
                wire::f64_at(b, obs::player::X),
                wire::f64_at(b, obs::player::Z),
            );
            let seen = Seen {
                frame,
                pos: [x, wire::f64_at(b, obs::player::Y), z],
                at: st.game_time,
            };
            st.phase1.players.insert(key, seen);
        }
        obs::PUPPET_STATE if rec.size() >= obs::puppet_state::SIZE => {
            let agent = wire::u64_at(b, obs::puppet_state::AGENT);
            let epoch = wire::u32_at(b, obs::puppet_state::EPOCH);
            let topo = st.topology.shared.clone();
            if rec.aux == 0
                && let Some(p) = st.phase1.puppets.get_mut(&agent)
                && p.epoch == epoch
            {
                // A puppet the frameworks folded across a seam is where it is (TP4, TP11): a
                // short step, never a teleport.
                let (x, z) = topo.of(p.frame).canonical_point(
                    wire::f64_at(b, obs::puppet_state::X),
                    wire::f64_at(b, obs::puppet_state::Z),
                );
                p.pos = [x, wire::f64_at(b, obs::puppet_state::Y), z];
            }
        }
        obs::VILLAGE_CENSUS if rec.size() >= obs::village_census::ENTRIES => {
            if !st.phase1.enabled {
                st.phase1.ignored += 1;
                return;
            }
            let key = wire::u64_at(b, obs::village_census::VILLAGE);
            let residents = wire::u32_at(b, obs::village_census::RESIDENTS);
            let golems = wire::u32_at(b, obs::village_census::GOLEMS);
            census(st, key, residents, golems);
        }
        obs::WORK_CATALOG if rec.size() >= obs::work_catalog::DEFS => {
            let Some(defs) = work_catalog(st, rec) else {
                log_warn("phase 1: a malformed WORK_CATALOG was ignored");
                return;
            };
            if defs != st.phase1.world.works_catalog {
                // Taken while switched off too, at the frozen clock (as HOUSE_PLACED, PR13): the
                // catalogue is sent once a session.
                if st.phase1.enabled {
                    input_now(st, Input::Works { defs });
                } else {
                    let at = st.phase1.world.now().0;
                    st.phase1.input(Input::Works { defs }, at);
                }
                take_world_commands(st);
            }
        }
        // VILLAGE_GOSSIP is vanilla's own record of players; the engine reads it as latest
        // state and acts on none of it yet (the knowledge crate keeps its own).
        _ => {}
    }
}

/// A census: the first one sets the village's population (its adoption guessed it from its
/// pieces); later ones only raise it, since growth books residents before they exist (P17) and a
/// census of a part-loaded village counts low (choice V9). Its golems are the latest count
/// (SINK.md MS-8: defenders against dragons), logged when they change.
fn census(st: &mut State, key: u64, residents: u32, golems: u32) {
    let l = &mut st.phase1;
    if l.scripted.contains(&key) {
        return;
    }
    let Some(&i) = l.nodes.get(&key) else {
        return;
    };
    if l.world.golems(i) != golems {
        input_now(st, Input::Golems { key, golems });
    }
    if residents == 0 {
        return;
    }
    let l = &mut st.phase1;
    let pop = l.world.nodes[i].local.pop;
    // The first census is logged even when it changes nothing: the log is what tells a later
    // session which villages had theirs.
    let first = l.censused.insert(key);
    if first || residents > pop {
        input_now(
            st,
            Input::Population {
                key,
                pop: residents,
            },
        );
    }
}

/// An input from a latest-state record (a census, the works catalogue), at the hand-over's game
/// time. The modules that share the world run to it first (U7, as [`shared::catch_up`] does for a
/// journaled record): the input runs the world to its time, and a module whose events in a
/// hand-over of several ticks had not run yet would meet a world already past them, which the
/// ships take for time they missed while switched off (PC103) and the world for a change before
/// its anchor.
///
/// While replaying with a module that shares the world, the input is held at the replay clock
/// as the step's run is (docs/replay-gate.md §3.6): at the hand-over's time it would run the
/// world, and with it every module, past the replayed records still to come.
fn input_now(st: &mut State, input: Input) -> log::Applied {
    let at = shared::hand_over_target(st);
    shared::run_to(st, at);
    st.phase1.input(input, at.0)
}

/// The works catalogue a WORK_CATALOG record holds (docs/boundary.md §4.1, SINK.md §4.2), its
/// names through Java's dictionary, sorted and deduplicated as the world keeps it. `None` when the
/// record is malformed. A kind of a culture the phase 1 world does not trade in, or naming what
/// the dictionary does not know, is left out (logged); an item none of the world's goods is
/// priced as material labour (MSC3).
fn work_catalog(st: &State, rec: &Rec<'_>) -> Option<Vec<WorkDef>> {
    use obs::work_catalog as k;
    let b = rec.bytes;
    let size = rec.size();
    let name = |kind: u32, id: u32| st.world.java_dict.get(&(kind, id)).cloned();
    let n = wire::u32_at(b, k::N) as usize;
    let mut at = k::DEFS;
    let mut defs = Vec::new();
    for _ in 0..n {
        if at + k::DEF > size {
            return None;
        }
        let id = name(wire::DICT_WORK, wire::u32_at(b, at + k::D_ID));
        let group = WorkGroup::from_u32(wire::u32_at(b, at + k::D_GROUP));
        let culture = name(wire::DICT_CULTURE, wire::u32_at(b, at + k::D_CULTURE))
            .and_then(|c| catalog::culture(&c));
        let style = match wire::u32_at(b, at + k::D_STYLE) {
            0 => Some(0),
            s => name(wire::DICT_STRUCTURE, s).map(|s| works::style_of(&s)),
        };
        let cap_n = wire::u32_at(b, at + k::D_CAP_N);
        let cap = match wire::u32_at(b, at + k::D_CAP) {
            k::CAP_PER_RESIDENTS => Some(WorkCap::PerResidents(cap_n.max(1))),
            k::CAP_FIXED => Some(WorkCap::Fixed(cap_n)),
            _ => None,
        };
        let nv = wire::u32_at(b, at + k::D_VARIANTS) as usize;
        at += k::DEF;
        let mut variants = Vec::new();
        let mut known = true;
        for _ in 0..nv {
            if at + k::VARIANT > size {
                return None;
            }
            let what = wire::u32_at(b, at + k::V_WHAT);
            let place = match wire::u32_at(b, at + k::V_PLACE) {
                k::PLACE_TEMPLATE => name(wire::DICT_TEMPLATE, what).map(|template| {
                    let processors = match wire::u32_at(b, at + k::V_PROCESSORS) {
                        0 => Some(String::new()),
                        p => name(wire::DICT_PROCESSORS, p),
                    };
                    processors.map(|processors| WorkPlace::Template {
                        template,
                        processors,
                    })
                }),
                k::PLACE_BODY => {
                    name(wire::DICT_BODY, what).map(|body| Some(WorkPlace::Spawn { body }))
                }
                _ => None,
            }
            .flatten();
            let blocks = i64::from(wire::u32_at(b, at + k::V_BLOCKS));
            let ni = wire::u32_at(b, at + k::V_ITEMS) as usize;
            at += k::VARIANT;
            if at + ni * k::ITEM > size {
                return None;
            }
            let mut goods: BTreeMap<GoodId, i64> = BTreeMap::new();
            let mut other_items = 0i64;
            for j in 0..ni {
                let item = wire::u32_at(b, at + j * k::ITEM);
                let count = i64::from(wire::u32_at(b, at + j * k::ITEM + 4));
                match name(wire::DICT_ITEM, item).and_then(|i| st.phase1.cat.good(&i)) {
                    Some(g) => *goods.entry(g).or_default() += count,
                    None => other_items += count,
                }
            }
            at += ni * k::ITEM;
            match place {
                Some(place) => variants.push(WorkVariant {
                    place,
                    goods: goods.into_iter().collect(),
                    other_items,
                    blocks,
                }),
                None => known = false,
            }
        }
        match (id, group, culture, style, cap) {
            (Some(id), Some(group), Some(culture), Some(style), Some(cap))
                if known && !variants.is_empty() =>
            {
                defs.push(WorkDef {
                    id,
                    group,
                    culture,
                    style,
                    cap,
                    variants,
                })
            }
            (id, ..) => log_warn(&format!(
                "phase 1: work kind {} left out of the catalogue (an unknown name, culture or cap)",
                id.as_deref().unwrap_or("?")
            )),
        }
    }
    defs.sort_by(|a, b| a.id.cmp(&b.id));
    defs.dedup_by(|a, b| a.id == b.id);
    Some(defs)
}

// ---- hooks: journaled records ---------------------------------------------------------------------

/// The journaled kinds this module applies (the core's are seen after the core in
/// [`after_core`]).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    (obs::VILLAGE_ADOPTED..=obs::HOUSE_PLACED).contains(&rec.kind) || rec.kind == TALKED
}

/// Op ids the kernel's `Replay` applies once: a trade's and a robbery's transfer, and a talk's
/// credit (a record of any other size has none: it is refused when applied).
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    let b = rec.bytes;
    match rec.kind {
        TALKED if rec.size() == talked_at::SIZE => wire::u64_at(b, talked_at::OP_ID),
        obs::TRADE if rec.size() >= obs::trade::SIZE => wire::u64_at(b, obs::trade::OP_ID),
        obs::CARGO_CHANGED if rec.size() >= obs::cargo_changed::ITEMS => {
            wire::u64_at(b, obs::cargo_changed::OP_ID)
        }
        _ => 0,
    }
}

fn uuid_at(b: &[u8], at: usize) -> Uuid {
    (wire::u64_at(b, at), wire::u64_at(b, at + 8))
}

fn item_name(st: &State, id: u32) -> Option<String> {
    st.world.java_dict.get(&(wire::DICT_ITEM, id)).cloned()
}

/// A phase 1 record `Replay` admitted. Switched off, phase 1 ignores it (U10), except HOUSE_PLACED:
/// the answer to a house it booked while on, taken at its frozen clock (the world does not run) so
/// the house's resident comes once it is on again (choice PR13).
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    if !st.phase1.enabled && rec.kind != obs::HOUSE_PLACED {
        st.phase1.ignored += 1;
        return;
    }
    let b = rec.bytes;
    let at = if st.phase1.enabled {
        st.phase1.wt(wire::i64_at(b, obs::GAME_TIME))
    } else {
        st.phase1.world.now().0
    };
    match rec.kind {
        obs::VILLAGE_ADOPTED if rec.size() >= obs::village_adopted::SIZE => adopted(st, rec, at),
        obs::TRADE if rec.size() >= obs::trade::SIZE => trade(st, rec, at),
        obs::HURT if rec.size() >= obs::hurt::SIZE => hurt(st, rec, at),
        obs::RESCUE if rec.size() >= obs::rescue::SIZE => rescue(st, rec, at),
        TALKED => talked(st, rec, at),
        obs::CARGO_CHANGED if rec.size() >= obs::cargo_changed::ITEMS => cargo(st, rec, at),
        obs::BASE if rec.size() >= obs::base::SIZE => {
            let key = wire::u64_at(b, obs::base::BASE);
            let player = uuid_at(b, obs::base::PLAYER_MOST);
            let input = if rec.aux == obs::base::MARKED {
                {
                    let frame = wire::u32_at(b, obs::base::DIM) as u16;
                    let (x, z) = st.topology.shared.of(frame).canonical_i32(
                        wire::f64_at(b, obs::base::X).floor() as i32,
                        wire::f64_at(b, obs::base::Z).floor() as i32,
                    );
                    Input::MarkBase {
                        key,
                        frame,
                        x,
                        z,
                        player,
                    }
                }
            } else {
                Input::UnmarkBase { key, player }
            };
            st.phase1.input(input, at);
            take_world_commands(st);
        }
        obs::HOUSE_PLACED if rec.size() >= obs::house_placed::SIZE => {
            let op = wire::u64_at(b, obs::house_placed::OP_ID);
            let village = wire::u64_at(b, obs::house_placed::VILLAGE);
            if st.phase1.scripted.contains(&village) || !st.phase1.nodes.contains_key(&village) {
                return;
            }
            // A work placed (SINK.md): its COMMAND_RESULT, right after, is what the world takes;
            // a work has no resident.
            if st.phase1.world.is_work_op(op) {
                return;
            }
            st.phase1.input(Input::HousePlaced { op }, at);
            take_world_commands(st);
        }
        _ => {}
    }
}

fn adopted(st: &mut State, rec: &Rec<'_>, at: u64) {
    use obs::village_adopted as k;
    let b = rec.bytes;
    let key = wire::u64_at(b, k::VILLAGE);
    if st.phase1.scripted.contains(&key) || st.phase1.nodes.contains_key(&key) {
        return; // a test's village, or one seen again after a crash (C22: a no-op)
    }
    let culture_name = st
        .world
        .java_dict
        .get(&(wire::DICT_CULTURE, wire::u32_at(b, k::CULTURE)))
        .cloned()
        .unwrap_or_default();
    let Some(culture) = catalog::culture(&culture_name) else {
        return; // a culture the phase 1 world does not trade in
    };
    let structure = st
        .world
        .java_dict
        .get(&(wire::DICT_STRUCTURE, wire::u32_at(b, k::STRUCTURE)))
        .cloned()
        .unwrap_or_default();
    let resources = catalog::resources(&structure, culture)
        .into_iter()
        .map(|(r, a)| (r.0, a))
        .collect();
    let input = Input::Join {
        key,
        culture: u8::from(culture == Culture::Ribbit),
        frame: wire::u32_at(b, k::DIM) as u16,
        x: wire::i32_at(b, k::MEET_X),
        z: wire::i32_at(b, k::MEET_Z),
        pop: catalog::population(wire::u32_at(b, k::PIECES), culture),
        resources,
    };
    st.phase1.input(input, at);
    // Its village type: which of the catalogue's styled works it buys (SINK.md §4.2).
    let style = works::style_of(&structure);
    if style != 0 && st.phase1.nodes.contains_key(&key) {
        st.phase1.input(Input::WorkStyle { key, style }, at);
    }
    st.phase1.stats[stat::VILLAGES] = st.phase1.nodes.len() as u64;
    take_world_commands(st);
}

/// The caravan row a phase 1 agent is: this session's puppet, else what the pages remember (a
/// record replayed or carried into a later session).
fn caravan_of(l: &Link, agent: u64) -> Option<u64> {
    match l.puppets.get(&agent).map(|p| p.of) {
        Some(PuppetOf::Caravan(bits)) => Some(bits),
        Some(PuppetOf::Band(_)) => None,
        None => l.agents.get(&agent).copied(),
    }
}

fn trade(st: &mut State, rec: &Rec<'_>, at: u64) {
    use obs::trade as k;
    let b = rec.bytes;
    let sells = match rec.aux {
        k::MERCHANT_SELLS => true,
        k::MERCHANT_BUYS => false,
        _ => {
            st.phase1.stats[stat::TRADES_REFUSED] += 1;
            return;
        }
    };
    let node = wire::u64_at(b, k::NODE);
    let agent = wire::u64_at(b, k::AGENT);
    let player = uuid_at(b, k::PLAYER_MOST);
    let names = (
        item_name(st, wire::u32_at(b, k::COST_A_ITEM)),
        item_name(st, wire::u32_at(b, k::RESULT_ITEM)),
    );
    let (a_n, r_n) = (
        i64::from(wire::i32_at(b, k::COST_A_COUNT)),
        i64::from(wire::i32_at(b, k::RESULT_COUNT)),
    );
    let l = &mut st.phase1;
    let (Some(a), Some(r)) = names else {
        l.stats[stat::TRADES_REFUSED] += 1;
        return;
    };
    let ((good, items), (cur, coins)) = if sells {
        ((r, r_n), (a, a_n))
    } else {
        ((a, a_n), (r, r_n))
    };
    let (Some(g), Some(c)) = (l.cat.good(&good), l.cat.good(&cur)) else {
        l.stats[stat::TRADES_REFUSED] += 1;
        return;
    };
    if wire::u32_at(b, k::COST_B_ITEM) != 0 || !l.cat.goods.is_currency(c) {
        l.stats[stat::TRADES_REFUSED] += 1;
        return;
    }
    if agent == 0 {
        if l.scripted.contains(&node) || !l.nodes.contains_key(&node) {
            return;
        }
        let out = l.input(
            Input::Trade {
                key: node,
                player,
                good: g.0,
                items,
                coins,
                buy: sells,
            },
            at,
        );
        if out.refused {
            l.stats[stat::TRADES_REFUSED] += 1;
        } else {
            l.stats[stat::RESIDENT_TRADES] += 1;
        }
        l.market_dirty.insert(node);
    } else {
        let Some(bits) = caravan_of(l, agent) else {
            return;
        };
        let at_node = if l.scripted.contains(&node) { 0 } else { node };
        let out = l.input(
            Input::CaravanTrade {
                caravan: bits,
                at_node,
                player,
                good: g.0,
                currency: c.0,
                buy: sells,
                items,
                coins,
            },
            at,
        );
        if out.refused {
            l.stats[stat::TRADES_REFUSED] += 1;
        } else {
            l.stats[stat::CARAVAN_TRADES] += 1;
        }
    }
    take_world_commands(st);
}

fn hurt(st: &mut State, rec: &Rec<'_>, at: u64) {
    use obs::hurt as k;
    let b = rec.bytes;
    if wire::u32_at(b, k::ATTACKER_KIND) != k::ATTACKER_PLAYER {
        return;
    }
    let player = uuid_at(b, k::ATTACKER_MOST);
    let l = &mut st.phase1;
    let input = match rec.aux {
        k::PUPPET_HURT => {
            let Some(bits) = caravan_of(l, wire::u64_at(b, k::AGENT)) else {
                return;
            };
            Input::CaravanReport {
                caravan: bits,
                player,
                rescue: false,
            }
        }
        k::RESIDENT_HURT | k::RESIDENT_KILLED => {
            let node = wire::u64_at(b, k::NODE);
            if l.scripted.contains(&node) || !l.nodes.contains_key(&node) {
                return;
            }
            // Flagged VANILLA_GOSSIP: vanilla wrote its own gossip. The report still counts for
            // the sim's sentiment and news; rtsim-knowledge never writes back what vanilla saw
            // first-hand (E36).
            Input::VillageReport {
                key: node,
                player,
                kind: if rec.aux == k::RESIDENT_KILLED { 8 } else { 7 },
            }
        }
        _ => return,
    };
    if !l.input(input, at).refused {
        l.stats[stat::REPORTS] += 1;
    }
    take_world_commands(st);
}

/// A TALKED `Replay` admitted (docs/boundary.md §13.7): the village credits the player's talk with
/// one of its ribbits. A record that does not decode, names a village the world does not know (or
/// a base), or is by nobody is refused and counted, never applied. A scripted village is skipped
/// like every other kind's (its sentiment is the test's own).
fn talked(st: &mut State, rec: &Rec<'_>, at: u64) {
    let t = match Talked::decode(rec.bytes) {
        Ok(t) => t,
        Err(e) => {
            return super::talk::refuse_talked(st, &format!("a TALKED does not decode ({e})"));
        }
    };
    let l = &st.phase1;
    if l.scripted.contains(&t.village) {
        return;
    }
    let is_village = l
        .nodes
        .get(&t.village)
        .and_then(|&i| l.world.nodes.get(i))
        .is_some_and(|n| matches!(n.kind, NodeKind::Village(_)));
    if !is_village {
        return super::talk::refuse_talked(
            st,
            &format!(
                "a TALKED names village {} the world does not know",
                t.village
            ),
        );
    }
    let out = st.phase1.input(
        Input::Talked {
            key: t.village,
            player: t.player,
        },
        at,
    );
    if out.refused {
        return super::talk::refuse_talked(st, "a TALKED the world refused");
    }
    st.talk.stats.talked += 1;
    take_world_commands(st);
}

fn rescue(st: &mut State, rec: &Rec<'_>, at: u64) {
    use obs::rescue as k;
    let b = rec.bytes;
    let player = uuid_at(b, k::RESCUER_MOST);
    let l = &mut st.phase1;
    let input = match rec.aux {
        k::PUPPET => {
            let Some(bits) = caravan_of(l, wire::u64_at(b, k::AGENT)) else {
                return;
            };
            Input::CaravanReport {
                caravan: bits,
                player,
                rescue: true,
            }
        }
        k::RESIDENT => {
            let node = wire::u64_at(b, k::NODE);
            if l.scripted.contains(&node) || !l.nodes.contains_key(&node) {
                return;
            }
            Input::VillageReport {
                key: node,
                player,
                kind: 6,
            }
        }
        _ => return,
    };
    if !l.input(input, at).refused {
        l.stats[stat::REPORTS] += 1;
    }
    take_world_commands(st);
}

fn cargo(st: &mut State, rec: &Rec<'_>, at: u64) {
    use obs::cargo_changed as k;
    let b = rec.bytes;
    let n = wire::u32_at(b, k::ITEM_COUNT) as usize;
    if k::size(n) > rec.size() {
        return;
    }
    let agent = wire::u64_at(b, k::AGENT);
    let player = uuid_at(b, k::PLAYER_MOST);
    let mut out: BTreeMap<u16, Amount> = BTreeMap::new();
    let mut inn: BTreeMap<u16, Amount> = BTreeMap::new();
    for i in 0..n {
        let it = k::ITEMS + i * obs::item::SIZE;
        let Some(name) = item_name(st, wire::u32_at(b, it + obs::item::ITEM)) else {
            continue;
        };
        let Some(g) = st.phase1.cat.good(&name) else {
            continue; // not a good the sim trades: it was never in its books
        };
        let delta = i64::from(wire::i32_at(b, it + obs::item::COUNT)) * MILLI;
        if delta < 0 {
            *out.entry(g.0).or_default() -= delta;
        } else if delta > 0 {
            *inn.entry(g.0).or_default() += delta;
        }
    }
    let l = &mut st.phase1;
    let Some(bits) = caravan_of(l, agent) else {
        return;
    };
    if !out.is_empty() {
        let refused = l
            .input(
                Input::Rob {
                    caravan: bits,
                    player,
                    goods: out.into_iter().collect(),
                },
                at,
            )
            .refused;
        if !refused && player != (0, 0) {
            l.stats[stat::ROBBERIES] += 1;
        }
    }
    if !inn.is_empty() {
        l.input(
            Input::CargoIn {
                caravan: bits,
                goods: inn.into_iter().collect(),
            },
            at,
        );
    }
    take_world_commands(st);
}

/// A core record about a phase 1 puppet or command, after the core world applied it.
pub(super) fn after_core(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::MATERIALIZED if rec.size() >= obs::materialized::SIZE => {
            let agent = wire::u64_at(b, obs::materialized::AGENT);
            let epoch = wire::u32_at(b, obs::materialized::EPOCH);
            if let Some(p) = st.phase1.puppets.get_mut(&agent)
                && p.epoch == epoch
                && p.state == PState::Spawning
            {
                p.state = PState::Embodied;
            }
        }
        obs::SPAWN_REJECTED if rec.size() >= obs::spawn_rejected::SIZE => {
            let agent = wire::u64_at(b, obs::spawn_rejected::AGENT);
            let epoch = wire::u32_at(b, obs::spawn_rejected::EPOCH);
            let l = &mut st.phase1;
            if l.puppets.get(&agent).is_some_and(|p| p.epoch == epoch) {
                l.puppets.remove(&agent);
                let until = l.world.now().0 + BACKOFF_TICKS;
                l.backoff.insert(agent, until);
            }
        }
        obs::DEMATERIALIZED if rec.size() >= obs::dematerialized::SIZE => {
            let agent = wire::u64_at(b, obs::dematerialized::AGENT);
            let epoch = wire::u32_at(b, obs::dematerialized::EPOCH);
            if st
                .phase1
                .puppets
                .get(&agent)
                .is_some_and(|p| p.epoch == epoch)
            {
                let p = st.phase1.puppets.remove(&agent).expect("checked");
                check_capture(st, agent, &p);
            }
        }
        obs::TAKEN | obs::DIED => {
            if !st.phase1.enabled {
                // U10: ignored, as every phase 1 record is while it is off.
                if rec.size() >= obs::taken::SIZE.min(obs::died::SIZE)
                    && caravan_of(&st.phase1, wire::u64_at(b, obs::taken::AGENT)).is_some()
                {
                    st.phase1.ignored += 1;
                }
                return;
            }
            let (agent, epoch, member, player, killed) = if rec.kind == obs::TAKEN {
                if rec.size() < obs::taken::SIZE {
                    return;
                }
                (
                    wire::u64_at(b, obs::taken::AGENT),
                    wire::u32_at(b, obs::taken::EPOCH),
                    wire::u32_at(b, obs::taken::MEMBER),
                    uuid_at(b, obs::taken::PLAYER_MOST),
                    false,
                )
            } else {
                if rec.size() < obs::died::SIZE {
                    return;
                }
                (
                    wire::u64_at(b, obs::died::AGENT),
                    wire::u32_at(b, obs::died::EPOCH),
                    rec.aux,
                    uuid_at(b, obs::died::KILLER_MOST),
                    true,
                )
            };
            let l = &mut st.phase1;
            // Any epoch: a member taken or killed in an earlier materialization (a record
            // replayed after a crash, or carried by a dropped item) was still this caravan's. A
            // repeat is a no-op by op id, then by state (the member is already gone).
            let _ = epoch;
            let Some(bits) = caravan_of(l, agent) else {
                return;
            };
            // Review P3: exactly what its chest or pack held (the MEMBER_STATE right before this
            // record) leaves the sim, never its planned cargo. Items the sim does not trade were
            // never in its books.
            let mut held: BTreeMap<u16, Amount> = BTreeMap::new();
            for it in st.world.captured_items(agent, member).unwrap_or(&[]) {
                if let Some(g) = l.cat.good(&it.item)
                    && it.count > 0
                {
                    *held.entry(g.0).or_default() += i64::from(it.count) * MILLI;
                }
            }
            let at = l.world.now().0;
            l.input(
                Input::MemberHeld {
                    caravan: bits,
                    member,
                    player,
                    killed,
                    held: held.into_iter().collect(),
                },
                at,
            );
            if player != (0, 0) {
                l.stats[stat::REPORTS] += 1;
            }
            take_world_commands(st);
        }
        obs::COMMAND_RESULT if rec.size() >= obs::command_result::SIZE => {
            let op = wire::u64_at(b, obs::command_result::OP_ID);
            let l = &mut st.phase1;
            l.outbox.retain(|(_, o, _)| *o != op);
            l.resend.retain(|(_, o, _)| *o != op);
            // A kept command queued again after a restart that Java applied before the crash.
            l.queue.retain(|c| c.kept_op() != Some(op));
            // A bought work's answer (SINK.md MS-11): placed, or its kind is full. Taken while
            // switched off too, at the frozen clock, as HOUSE_PLACED is (PR13).
            if l.world.is_work_op(op) {
                use obs::command_result as r;
                let placed = rec.aux == r::APPLIED || rec.aux == r::DUPLICATE_OP;
                let at = if l.enabled {
                    l.wt(wire::i64_at(b, obs::GAME_TIME))
                } else {
                    l.world.now().0
                };
                l.input(Input::WorkResult { op, placed }, at);
                if placed {
                    l.stats[stat::WORKS_PLACED] += 1;
                } else {
                    l.stats[stat::WORKS_REFUSED] += 1;
                }
                take_world_commands(st);
            }
        }
        _ => {}
    }
}

/// What a despawned caravan's members carried, against the sim's row (whole items, cargo and
/// purse). A difference no TRADE or CARGO_CHANGED explained is counted and logged, not booked.
fn check_capture(st: &mut State, agent: u64, p: &Puppet) {
    let PuppetOf::Caravan(bits) = p.of else {
        return;
    };
    let Some(a) = st.world.agents.get(&agent) else {
        return;
    };
    let mut seen: BTreeMap<GoodId, i64> = BTreeMap::new();
    for m in a.members.iter().filter(|m| m.gone.is_none()) {
        for it in &m.items {
            if let Some(g) = st.phase1.cat.good(&it.item) {
                *seen.entry(g).or_default() += i64::from(it.count);
            }
        }
    }
    let l = &mut st.phase1;
    let Some(car) = l.world.caravans.get(Handle::from_bits(bits)) else {
        return;
    };
    let mut want: BTreeMap<GoodId, i64> = BTreeMap::new();
    for (g, a) in car.holdings() {
        if a >= MILLI {
            want.insert(g, a / MILLI);
        }
    }
    if seen != want {
        l.capture_mismatches += 1;
        log_warn(&format!(
            "phase 1 caravan agent {agent:#x} came back with {seen:?}, the sim's row holds {want:?} (not reconciled)"
        ));
    }
}

// ---- directives (orders applied when the engine takes them) -------------------------------------

pub(super) fn directive(st: &mut State, order: Order) {
    let l = &mut st.phase1;
    match order {
        Order::ScriptVillage(v) => {
            l.scripted.insert(v);
        }
        Order::Phase1Advance(t) => {
            l.offset = l.offset.saturating_add(t);
        }
        Order::WorksGrant { village, amount } => {
            // Switched off the world's clock is frozen (U10): a test's raid or grant waits for
            // nothing and is dropped.
            if l.enabled && l.nodes.contains_key(&village) {
                let at = l.wt(st.game_time);
                l.input(
                    Input::Grant {
                        key: village,
                        amount,
                    },
                    at,
                );
                take_world_commands(st);
            }
        }
        Order::WorksRaid { village, tribute } => {
            // Switched off the world's clock is frozen (U10): a test's raid or grant waits for
            // nothing and is dropped.
            if l.enabled && l.nodes.contains_key(&village) {
                let at = l.wt(st.game_time);
                l.input(
                    Input::Raided {
                        key: village,
                        tribute,
                    },
                    at,
                );
                take_world_commands(st);
            }
        }
        Order::Phase1SpawnNear {
            agent,
            epoch,
            dimension,
            pos,
            radius,
            search,
        } => l.spawn_near.push(SpawnNear {
            agent,
            epoch,
            dimension,
            pos,
            radius,
            search,
        }),
        _ => {}
    }
}

// ---- simulate --------------------------------------------------------------------------------------

/// Runs the world to the hand-over's game time, then materializes, steers and despawns puppets
/// and refreshes the markets near players.
pub(super) fn simulate(st: &mut State) {
    if !st.phase1.enabled {
        // U10: switched off, it runs nothing. (The dragons, ships, herds and conflict live in
        // its world and run its clock while they run; its own link decides nothing.)
        st.phase1.spawn_near.clear();
        st.phase1.stats[stat::WORLD_TICK] = st.phase1.world.now().0;
        return;
    }
    // Held at the replay clock while replaying with a module that shares the world (§3.6).
    let target = shared::hand_over_target(st).0;
    if target > st.phase1.world.now().0 {
        st.phase1.world.run_until(Tick(target));
    }
    take_world_commands(st);
    let near: Vec<SpawnNear> = std::mem::take(&mut st.phase1.spawn_near);
    for s in near {
        spawn_near(st, s);
    }
    let gt = st.game_time;
    if st.phase1.last_interest == i64::MIN || gt - st.phase1.last_interest >= INTEREST_TICKS {
        st.phase1.last_interest = gt;
        interest(st);
    }
    if !st.phase1.market_dirty.is_empty()
        || st.phase1.last_market == i64::MIN
        || gt - st.phase1.last_market >= MARKET_TICKS
    {
        st.phase1.last_market = gt;
        markets(st);
    }
    let l = &mut st.phase1;
    l.stats[stat::VILLAGES] = l.nodes.len() as u64;
    l.stats[stat::EMBODIED] = l
        .puppets
        .values()
        .filter(|p| p.state == PState::Embodied)
        .count() as u64;
    l.stats[stat::WORLD_TICK] = l.world.now().0;
    l.stats[stat::WORKS_BOUGHT] = l.world.stats.works;
    l.stats[stat::TRIBUTES] = l.world.stats.tributes;
}

/// Turns the world's commands into records. Scripted villages get none.
fn take_world_commands(st: &mut State) {
    let commands = st.phase1.world.take_commands();
    if commands.is_empty() {
        return;
    }
    for c in commands {
        let cmd = match c {
            Command::VanillaGossip(g) => {
                let l = &st.phase1;
                // A village's actor is registered under its key: a lookup, not a scan.
                let Some(i) = l
                    .world
                    .knowledge
                    .actors()
                    .external(g.village)
                    .and_then(|k| l.nodes.get(&k).copied())
                    .filter(|&i| l.world.nodes[i].actor == g.village)
                else {
                    continue;
                };
                let village = l.world.nodes[i].key;
                if l.scripted.contains(&village) {
                    continue;
                }
                let player = l.world.player_uuid_of(g.player);
                let kind = g.kind as u32;
                let when = g.at.0;
                // The write's identity: which village hears what about whom, done when. The same
                // decision re-derived after a crash gets the same id (docs/boundary.md §2.2).
                let identity = StreamKey::new(village, SystemId::named("phase1.gossip"), player.0)
                    .block(player.1, when ^ (u64::from(kind) << 56))[0];
                P1Cmd::Gossip(GossipCmd {
                    op: identity_op(identity, g.gossip as u64),
                    village,
                    player,
                    gossip: g.gossip as u32,
                    amount: g.amount.max(GOSSIP_MIN),
                    report: kind,
                    // rtsim-knowledge never asks for what vanilla records first-hand itself (E36),
                    // so the bridge's VANILLA_KNOWS refusal cannot come from these.
                    flags: if g.first_hand {
                        cmd::write_gossip::FLAG_FIRST_HAND
                    } else {
                        0
                    },
                })
            }
            Command::Sentiment(s) => {
                if st.phase1.scripted.contains(&s.village) {
                    continue;
                }
                P1Cmd::Sentiment(SentimentCmd {
                    holder: s.village,
                    dislike: i32::from(s.dislike_at_or_below),
                    friend: i32::from(s.friend_at_or_above),
                    flags: cmd::sentiment::FLAG_REPLACE,
                    entries: s
                        .entries
                        .iter()
                        .map(|&(m, l, v)| (m, l, i32::from(v)))
                        .collect(),
                })
            }
            Command::PlaceHouse(h) => {
                if st.phase1.scripted.contains(&h.village) {
                    continue;
                }
                let Some(dimension) = Link::java_dim(&st.world, h.frame) else {
                    log_warn(&format!(
                        "phase 1: no dimension for frame {} (house {:#x} dropped)",
                        h.frame, h.op
                    ));
                    continue;
                };
                let template = catalog::RIBBIT_HOUSES
                    [h.template as usize % catalog::RIBBIT_HOUSES.len()]
                .to_string();
                P1Cmd::PlaceHouse(PlaceHouseCmd {
                    op: h.op,
                    village: h.village,
                    dimension,
                    template,
                    processors: catalog::RIBBIT_PROCESSORS.to_string(),
                    pos: [h.x, 0, h.z],
                    rotation: u32::from(h.rotation),
                    mirror: 0,
                    flags: u32::from(
                        cmd::place_house::FLAG_SEARCH | cmd::place_house::FLAG_Y_FROM_VILLAGE,
                    ),
                    radius: h.radius,
                    seed: h.seed,
                    y_offset: 0,
                })
            }
            Command::SpawnResident(r) => {
                let l = &st.phase1;
                if l.scripted.contains(&r.village) {
                    continue;
                }
                let Some(&i) = l.nodes.get(&r.village) else {
                    continue;
                };
                let Some(dimension) = Link::java_dim(&st.world, l.world.nodes[i].pos.frame) else {
                    continue;
                };
                P1Cmd::SpawnResident(SpawnResidentCmd {
                    op: r.op,
                    village: r.village,
                    dimension,
                    body: catalog::RIBBIT_BODY.to_string(),
                    profession: String::new(),
                    flags: cmd::spawn_resident::FLAG_AT_HOUSE,
                    pos: [0.0; 3],
                    house: r.house,
                })
            }
            // A bought work (SINK.md): its template on a free plot near the meeting point, as a
            // house is placed, or its body at the meeting point. Kept by op like a house.
            Command::PlaceWork(p) => {
                if st.phase1.scripted.contains(&p.village) {
                    continue;
                }
                let Some(dimension) = Link::java_dim(&st.world, p.frame) else {
                    log_warn(&format!(
                        "phase 1: no dimension for frame {} (work {:#x} dropped)",
                        p.frame, p.op
                    ));
                    continue;
                };
                P1Cmd::PlaceHouse(PlaceHouseCmd {
                    op: p.op,
                    village: p.village,
                    dimension,
                    template: p.template,
                    processors: p.processors,
                    pos: [p.x, 0, p.z],
                    rotation: u32::from(p.rotation),
                    mirror: 0,
                    flags: u32::from(
                        cmd::place_house::FLAG_SEARCH | cmd::place_house::FLAG_Y_FROM_VILLAGE,
                    ),
                    radius: p.radius,
                    seed: p.seed,
                    y_offset: 0,
                })
            }
            Command::SpawnWork(s) => {
                let l = &st.phase1;
                if l.scripted.contains(&s.village) {
                    continue;
                }
                let Some(&i) = l.nodes.get(&s.village) else {
                    continue;
                };
                let Some(dimension) = Link::java_dim(&st.world, l.world.nodes[i].pos.frame) else {
                    continue;
                };
                P1Cmd::SpawnResident(SpawnResidentCmd {
                    op: s.op,
                    village: s.village,
                    dimension,
                    body: s.body,
                    profession: String::new(),
                    flags: cmd::spawn_resident::FLAG_AT_VILLAGE,
                    pos: [0.0; 3],
                    house: 0,
                })
            }
        };
        st.phase1.push(cmd);
    }
}

fn dist2(a: [f64; 3], x: f64, z: f64) -> f64 {
    (a[0] - x).powi(2) + (a[2] - z).powi(2)
}

/// [`dist2`] through every seam (topology choice TP2): the plane distance on a flat frame.
fn tdist2(t: &rtsim_space::Topology, a: [f64; 3], x: f64, z: f64) -> f64 {
    if t.is_flat() {
        return dist2(a, x, z);
    }
    t.dist_sq(
        (a[0].floor() as i64, a[2].floor() as i64),
        (x.floor() as i64, z.floor() as i64),
    ) as f64
}

fn caravan_agent(id: CaravanId) -> u64 {
    AGENT_P1 | identity_op(id.to_bits(), 0x5031)
}

fn band_agent(seed: u64) -> u64 {
    AGENT_P1 | identity_op(seed, 0x5032)
}

/// The cargo of `goods` as chest stacks in `slots` slots (whole items; the rest stays in the sim).
fn stacks(
    cat: &catalog::Catalog,
    goods: &BTreeMap<GoodId, Amount>,
    slots: u32,
    next: &mut u32,
) -> Vec<ItemOrder> {
    let mut out = Vec::new();
    for (&g, &a) in goods {
        let mut whole = a / MILLI;
        let stack = i64::from(cat.stack(g));
        while whole > 0 && *next < slots {
            let n = whole.min(stack);
            out.push(ItemOrder {
                slot: *next,
                count: n as i32,
                item: cat.item(g).to_string(),
            });
            *next += 1;
            whole -= n;
        }
    }
    out
}

/// Queues the spawn of caravan row `id` (and its offers). Returns whether it was queued.
#[allow(clippy::too_many_arguments)]
fn spawn_caravan(
    st: &mut State,
    id: CaravanId,
    frame: u16,
    pos: [f64; 3],
    flags: u32,
    agent_epoch: Option<(u64, u32)>,
    pinned: bool,
) -> bool {
    let Some(dimension) = Link::java_dim(&st.world, frame) else {
        return false;
    };
    let l = &mut st.phase1;
    let Some(car) = l.world.caravans.get(id) else {
        return false;
    };
    let (agent, epoch) = agent_epoch.unwrap_or_else(|| {
        let e = l.next_epoch;
        (caravan_agent(id), e)
    });
    if l.puppets.contains_key(&agent) || l.backoff.contains_key(&agent) {
        return false;
    }
    if agent_epoch.is_none() {
        l.next_epoch = l.next_epoch.wrapping_add(1).max(1);
    }
    let (group_kind, slots) = match car.group {
        GroupKind::HumanCaravan => (catalog::HUMAN_CARAVAN, catalog::LLAMA_SLOTS),
        GroupKind::RibbitCaravan => (catalog::RIBBIT_MERCHANT, catalog::PACK_SLOTS),
    };
    // The purse rides with the last carrier: in game the chests hold it with the cargo (C21).
    let last_carrier = car
        .members
        .iter()
        .rposition(|m| m.present && GroupKind::carries(m.body));
    let mut members = Vec::new();
    for (i, m) in car.members.iter().enumerate() {
        let mut next = 0;
        let mut items = Vec::new();
        if m.present && GroupKind::carries(m.body) {
            items = stacks(&l.cat, &m.cargo, slots, &mut next);
            if Some(i) == last_carrier {
                items.extend(stacks(&l.cat, &car.purse, slots, &mut next));
            }
        }
        members.push(MemberOrder {
            index: i as u32,
            health: if m.alive && m.present { 0.0 } else { -1.0 },
            body_a: -1,
            body_b: if m.body == Body::Llama {
                catalog::LLAMA_STRENGTH
            } else {
                -1
            },
            items,
        });
    }
    if members.iter().any(|m| m.health < 0.0) {
        // A group with a member gone cannot be spawned whole (the bridge spawns every member of
        // its kind): it stays in the sim.
        return false;
    }
    let leg_end = car.leg.end.0;
    let home_actor = l.world.nodes[car.home].actor;
    let at_market = market_at(l, id, frame, pos);
    let offers = offers_of(l, id, home_actor, at_market);
    l.push(P1Cmd::Spawn {
        agent,
        epoch,
        group_kind: group_kind.to_string(),
        dimension,
        pos,
        flags,
        members,
    });
    l.push(P1Cmd::Offers(OffersCmd {
        agent,
        epoch,
        member: 0,
        offers,
    }));
    l.agents.insert(agent, id.to_bits());
    l.puppets.insert(
        agent,
        Puppet {
            epoch,
            of: PuppetOf::Caravan(id.to_bits()),
            state: PState::Spawning,
            frame,
            pos,
            pinned,
            leg_end,
            last_steer: i64::MIN,
            offers_at: at_market,
        },
    );
    true
}

/// The end of caravan `id`'s road its puppet stands at (within `AT_MARKET_RADIUS`), if any.
fn market_at(l: &Link, id: CaravanId, frame: u16, pos: [f64; 3]) -> Option<usize> {
    let car = l.world.caravans.get(id)?;
    let t = l.world.topo(frame);
    [car.leg.to, car.leg.from].into_iter().find(|&n| {
        let p = l.world.nodes[n].pos;
        p.frame == frame
            && tdist2(t, pos, f64::from(p.x), f64::from(p.z)) <= AT_MARKET_RADIUS.powi(2)
    })
}

/// A caravan's offers as MERCHANT_OFFERS (PLAN.md §8.5): priced fair (MERCHANT_OFFERS is not
/// per player; choice V10), uses as the sim allows; standing at a border market, its coin
/// exchange too (P21).
fn offers_of(
    l: &Link,
    id: CaravanId,
    as_actor: rtsim_knowledge::ActorId,
    at: Option<usize>,
) -> Vec<OfferCmd> {
    let mut out = Vec::new();
    for o in l.world.caravan_offers_at(CaravanRef::Row(id), as_actor, at) {
        let (good, cur) = (l.cat.item(o.good), l.cat.item(o.currency));
        let (gs, cs) = (
            i64::from(l.cat.stack(o.good)),
            i64::from(l.cat.stack(o.currency)),
        );
        if o.items <= 0 || o.coins <= 0 || o.items > gs || o.coins > cs {
            continue;
        }
        let uses = i32::try_from(o.uses).unwrap_or(i32::MAX);
        out.push(match o.side {
            OfferSide::MarketSells => OfferCmd {
                cost_a: cur.to_string(),
                cost_a_count: o.coins as i32,
                cost_b: String::new(),
                cost_b_count: 0,
                result: good.to_string(),
                result_count: o.items as i32,
                max_uses: uses,
                flags: cmd::merchant_offers::O_FLAG_SELLS,
            },
            OfferSide::MarketBuys => OfferCmd {
                cost_a: good.to_string(),
                cost_a_count: o.items as i32,
                cost_b: String::new(),
                cost_b_count: 0,
                result: cur.to_string(),
                result_count: o.coins as i32,
                max_uses: uses,
                flags: cmd::merchant_offers::O_FLAG_BUYS,
            },
        });
    }
    out
}

fn spawn_band(st: &mut State, band: usize, frame: u16, pos: [f64; 3]) -> bool {
    let Some(dimension) = Link::java_dim(&st.world, frame) else {
        return false;
    };
    let l = &mut st.phase1;
    let Some(b) = l.world.bands.get(band) else {
        return false;
    };
    let agent = band_agent(b.seed);
    if l.puppets.contains_key(&agent) {
        return false;
    }
    let epoch = l.next_epoch;
    l.next_epoch = l.next_epoch.wrapping_add(1).max(1);
    let members = (0..catalog::RIBBIT_BAND_MEMBERS)
        .map(|i| MemberOrder {
            index: i,
            health: 0.0,
            body_a: -1,
            body_b: -1,
            items: Vec::new(),
        })
        .collect();
    l.push(P1Cmd::Spawn {
        agent,
        epoch,
        group_kind: catalog::RIBBIT_BAND.to_string(),
        dimension,
        pos,
        flags: cmd::spawn_group::FLAG_SURFACE,
        members,
    });
    l.puppets.insert(
        agent,
        Puppet {
            epoch,
            of: PuppetOf::Band(band),
            state: PState::Spawning,
            frame,
            pos,
            pinned: false,
            leg_end: u64::MAX,
            last_steer: i64::MIN,
            offers_at: None,
        },
    );
    true
}

/// A test's P1_SPAWN_NEAR: the caravan with wares nearest the search point within the radius,
/// materialized at the given place.
fn spawn_near(st: &mut State, s: SpawnNear) {
    let Some(frame) = st
        .world
        .java_dict
        .iter()
        .find(|((k, _), n)| *k == wire::DICT_DIMENSION && **n == s.dimension)
        .map(|((_, id), _)| *id as u16)
    else {
        log_warn(&format!(
            "phase 1: P1_SPAWN_NEAR in {}, a dimension Java never named",
            s.dimension
        ));
        return;
    };
    let pos = Pos::new(
        frame,
        s.search[0].floor() as i32,
        s.search[1].floor() as i32,
    );
    let visible = st.phase1.world.visible(pos, s.radius);
    let carries = |c: &BTreeMap<GoodId, Amount>| c.values().any(|&a| a >= MILLI);
    for v in visible {
        let id = match v {
            Visible::Traveller { traveller, .. } => {
                // A test wants a caravan with wares: scouts carrying only news are passed over.
                if !carries(&traveller.cargo) {
                    continue;
                }
                let k = traveller.key;
                let at = st.phase1.world.now().0;
                let out = st.phase1.input(
                    Input::Promote {
                        edge: k.edge,
                        dir: k.dir,
                        departure: k.departure,
                    },
                    at,
                );
                match out.caravan {
                    Some(id) => id,
                    None => continue,
                }
            }
            Visible::Caravan { id, .. } => {
                if !st
                    .phase1
                    .world
                    .caravans
                    .get(id)
                    .is_some_and(|c| carries(&c.cargo()))
                {
                    continue;
                }
                id
            }
            Visible::Band { .. } => continue,
        };
        if spawn_caravan(st, id, frame, s.pos, 0, Some((s.agent, s.epoch)), true) {
            return;
        }
    }
    log_warn(&format!(
        "phase 1: P1_SPAWN_NEAR found no caravan with wares within {} blocks of ({}, {})",
        s.radius, s.search[0], s.search[1]
    ));
}

/// Materializes what players can see, steers what is embodied, captures what no player sees.
fn interest(st: &mut State) {
    let gt = st.game_time;
    let wnow = st.phase1.world.now().0;
    let topo = st.topology.shared.clone();
    st.phase1.players.retain(|_, s| gt - s.at <= PLAYER_STALE);
    st.phase1.backoff.retain(|_, until| *until > wnow);

    // Capture or steer what is embodied.
    let mut despawn: Vec<(u64, u32)> = Vec::new();
    let mut steer: Vec<(u64, u32, [f64; 3])> = Vec::new();
    {
        let l = &mut st.phase1;
        for (&agent, p) in l.puppets.iter_mut() {
            if p.state != PState::Embodied || p.pinned {
                continue;
            }
            let t = topo.of(p.frame);
            let seen = l.players.values().any(|s| {
                s.frame == p.frame && tdist2(t, s.pos, p.pos[0], p.pos[2]) <= DESPAWN_RADIUS.powi(2)
            });
            let (moved_on, target) = match p.of {
                PuppetOf::Caravan(bits) => match l.world.caravans.get(Handle::from_bits(bits)) {
                    None => (true, None),
                    Some(c) => {
                        let to = l.world.nodes[c.leg.to].pos;
                        (
                            c.leg.end.0 != p.leg_end || wnow >= c.leg.end.0,
                            Some((to.x, to.z)),
                        )
                    }
                },
                PuppetOf::Band(b) => {
                    let v = l.world.band_view(b, Tick(wnow));
                    let node = v.staying.map(|s| s.0).or(v.walking.map(|w| w.1));
                    (
                        false,
                        node.map(|n| (l.world.nodes[n].pos.x, l.world.nodes[n].pos.z)),
                    )
                }
            };
            // The target's image nearest the puppet (TP11); one past an edge_stop seam means
            // the group goes before any body would cross it (TP12).
            let here = (p.pos[0].floor() as i64, p.pos[2].floor() as i64);
            let target = target.map(|(x, z)| t.embodied_toward(here, (i64::from(x), i64::from(z))));
            let blocked = matches!(target, Some(None));
            if moved_on || !seen || blocked {
                p.state = PState::Despawning;
                despawn.push((agent, p.epoch));
            } else if let Some(Some((x, z))) = target
                && (p.last_steer == i64::MIN || gt - p.last_steer >= STEER_TICKS)
            {
                p.last_steer = gt;
                steer.push((agent, p.epoch, [x as f64 + 0.5, p.pos[1], z as f64 + 0.5]));
            }
        }
        for (agent, epoch) in despawn {
            l.push(P1Cmd::Despawn {
                agent,
                epoch,
                reason: obs::dematerialized::COMMAND,
            });
        }
        for (agent, epoch, pos) in steer {
            l.push(P1Cmd::Steer {
                agent,
                epoch,
                speed: STEER_SPEED,
                pos,
            });
        }
        // A caravan that walked into or out of a market's reach gets its offers again: at a
        // border market they include coin exchange (P21).
        let mut reoffer: Vec<(u64, u32, CaravanId, Option<usize>)> = Vec::new();
        for (&agent, p) in &l.puppets {
            if p.state != PState::Embodied {
                continue;
            }
            if let PuppetOf::Caravan(bits) = p.of {
                let id: CaravanId = Handle::from_bits(bits);
                let now_at = market_at(l, id, p.frame, p.pos);
                if now_at != p.offers_at {
                    reoffer.push((agent, p.epoch, id, now_at));
                }
            }
        }
        for (agent, epoch, id, at) in reoffer {
            let Some(car) = l.world.caravans.get(id) else {
                continue;
            };
            let home_actor = l.world.nodes[car.home].actor;
            let offers = offers_of(l, id, home_actor, at);
            if let Some(p) = l.puppets.get_mut(&agent) {
                p.offers_at = at;
            }
            l.push(P1Cmd::Offers(OffersCmd {
                agent,
                epoch,
                member: 0,
                offers,
            }));
        }
    }

    if !st.phase1.enabled {
        return;
    }
    // Materialize what players can see, nearest first.
    let players: Vec<Seen> = st.phase1.players.values().copied().collect();
    for s in players {
        let live = st.phase1.puppets.len();
        if live >= MAX_EMBODIED {
            break;
        }
        let near_mine = st
            .phase1
            .puppets
            .values()
            .filter(|p| {
                p.frame == s.frame
                    && tdist2(topo.of(s.frame), s.pos, p.pos[0], p.pos[2])
                        <= f64::from(SPAWN_RADIUS).powi(2)
            })
            .count();
        if near_mine >= MAX_PER_PLAYER {
            continue;
        }
        let pos = Pos::new(s.frame, s.pos[0].floor() as i32, s.pos[2].floor() as i32);
        let visible = st.phase1.world.visible(pos, SPAWN_RADIUS);
        let mut spawned = 0;
        for v in visible {
            if near_mine + spawned >= MAX_PER_PLAYER {
                break;
            }
            // The image on the player's side (TP11); nothing across an edge_stop seam (TP12).
            let Some((x, z)) = topo
                .of(s.frame)
                .embodied_toward((pos.x.into(), pos.z.into()), v.pos())
            else {
                continue;
            };
            let at = [x as f64 + 0.5, s.pos[1], z as f64 + 0.5];
            let ok = match v {
                Visible::Traveller { traveller, .. } => {
                    let k = traveller.key;
                    let wnow = st.phase1.world.now().0;
                    let out = st.phase1.input(
                        Input::Promote {
                            edge: k.edge,
                            dir: k.dir,
                            departure: k.departure,
                        },
                        wnow,
                    );
                    match out.caravan {
                        Some(id) => spawn_caravan(
                            st,
                            id,
                            s.frame,
                            at,
                            cmd::spawn_group::FLAG_SURFACE,
                            None,
                            false,
                        ),
                        None => false,
                    }
                }
                Visible::Caravan { id, .. } => spawn_caravan(
                    st,
                    id,
                    s.frame,
                    at,
                    cmd::spawn_group::FLAG_SURFACE,
                    None,
                    false,
                ),
                Visible::Band { band, .. } => {
                    let seed = st.phase1.world.bands[band].seed;
                    if st.phase1.backoff.contains_key(&band_agent(seed)) {
                        continue;
                    }
                    spawn_band(st, band, s.frame, at)
                }
            };
            if ok {
                spawned += 1;
            }
        }
    }
}

/// MARKET snapshots for the villages near a player or just traded at, when they changed.
fn markets(st: &mut State) {
    let as_of = st.replay.applied_through().0;
    let l = &mut st.phase1;
    let worst = l.world.cfg.markup_worst;
    let mut out = Vec::new();
    // The villages just traded at, and those in the cells around a player.
    let mut candidates: BTreeSet<u64> = l.market_dirty.clone();
    let r = MARKET_RADIUS.ceil() as i64;
    let topo = l.world.legs.topologies().clone();
    for s in l.players.values() {
        let (x, z) = (s.pos[0].floor() as i64, s.pos[2].floor() as i64);
        // Across seams (TP8): the square folded onto canonical cells.
        for part in topo.of(s.frame).rects_near(x, z, r) {
            let (x0, z0, x1, z1) = part.grid_cells(MARKET_CELL);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    if let Some(v) = l.cells.get(&(s.frame, cx, cz)) {
                        candidates.extend(v.iter().copied());
                    }
                }
            }
        }
    }
    for key in candidates {
        let Some(&i) = l.nodes.get(&key) else {
            continue;
        };
        if l.scripted.contains(&key) {
            continue;
        }
        let n = &l.world.nodes[i];
        if !n.kind.is_storehouse() || matches!(n.kind, NodeKind::Haven(_)) {
            continue;
        }
        let near = l.market_dirty.contains(&key)
            || l.players.values().any(|s| {
                s.frame == n.pos.frame
                    && tdist2(
                        topo.of(s.frame),
                        s.pos,
                        f64::from(n.pos.x),
                        f64::from(n.pos.z),
                    ) <= MARKET_RADIUS.powi(2)
            });
        if !near {
            continue;
        }
        let m = l.world.market(i);
        let Some(cur) = m.currency else {
            continue;
        };
        let goods: Vec<MarketGood> = l
            .cat
            .goods
            .ids()
            .filter(|&g| !l.cat.goods.is_currency(g))
            .map(|g| {
                let c = &m.goods[g.index()].curve;
                MarketGood {
                    item: l.cat.item(g).to_string(),
                    stock: m.stock(g),
                    target: c.target,
                    anchor: c.anchor,
                    r_num: c.r.num as i32,
                    r_den: c.r.den as i32,
                }
            })
            .collect();
        let mc = MarketCmd {
            village: key,
            as_of,
            currency: l.cat.item(cur).to_string(),
            treasury: m.stock(cur),
            treasury_target: m.line(cur).target,
            worst_num: worst.num as i32,
            worst_den: worst.den as i32,
            goods,
        };
        let mut w = Writer::new();
        w.i64(mc.treasury);
        w.i64(mc.treasury_target);
        for g in &mc.goods {
            w.i64(g.stock);
            w.i64(g.target);
            w.i64(g.anchor);
        }
        let fp = StreamKey::new(key, SystemId::named("phase1.market"), 0)
            .block(fnv1a(w.as_bytes()), 0)[0];
        if l.markets_sent.get(&key) != Some(&fp) || l.market_dirty.contains(&key) {
            l.markets_sent.insert(key, fp);
            out.push(mc);
        }
    }
    l.market_dirty.clear();
    for mc in out {
        l.push(P1Cmd::Market(mc));
    }
}

/// A 64-bit FNV-1a of a byte string (a fingerprint, not a random draw).
fn fnv1a(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---- writing commands ------------------------------------------------------------------------------

/// The bytes `c` needs in a command buffer, its DICT records included.
fn need(st: &State, c: &P1Cmd) -> (usize, bool) {
    let mut names = Vec::new();
    c.names(&mut names);
    let refs: Vec<(u32, &str)> = names.iter().map(|(k, n)| (*k, n.as_str())).collect();
    let too_big = c.bytes() > wire::MAX_RECORD
        || names
            .iter()
            .any(|(_, n)| cmd::dict::size(n.len()) > wire::MAX_RECORD);
    (st.dict_bytes(&refs) + c.bytes(), too_big)
}

enum One {
    Written(u64),
    Full,
    Never,
}

/// Writes one command, under `seq` (a resend) or a new sequence.
fn write_one(st: &mut State, w: &mut RecordWriter<'_>, c: &P1Cmd, seq: Option<u64>) -> One {
    let (n, too_big) = need(st, c);
    if too_big || n > w.capacity() {
        return One::Never;
    }
    if n > w.remaining() {
        return One::Full;
    }
    // DICT records go first, then the sequence is taken, so a buffer's sequences rise.
    let mut names = Vec::new();
    c.names(&mut names);
    for (k, name) in &names {
        st.dict_id(w, *k, name);
    }
    let seq = match seq {
        Some(s) => s,
        None => st.session_seq(),
    };
    cmds::write(w, c, seq, &mut |w, k, name| st.dict_id(w, k, name));
    // The core world keeps its books of every agent (embodied counts, captures).
    match c {
        P1Cmd::Spawn {
            agent,
            epoch,
            group_kind,
            dimension,
            pos,
            members,
            ..
        } => {
            let world_members: Vec<crate::world::Member> = members
                .iter()
                .map(|m| crate::world::Member {
                    index: m.index,
                    body_kind: String::new(),
                    health: m.health,
                    max_health: 0.0,
                    body_a: m.body_a,
                    body_b: m.body_b,
                    flags: 0,
                    pos: *pos,
                    items: m
                        .items
                        .iter()
                        .map(|it| crate::world::Item {
                            slot: it.slot,
                            item: it.item.clone(),
                            count: it.count,
                        })
                        .collect(),
                    gone: None,
                })
                .collect();
            st.world
                .spawning(*agent, *epoch, group_kind, dimension, *pos, &world_members);
            st.phase1.stats[stat::SPAWNS_SENT] += 1;
        }
        P1Cmd::Despawn { agent, epoch, .. } => st.world.despawning(*agent, *epoch),
        P1Cmd::Market(_) => st.phase1.stats[stat::MARKETS_SENT] += 1,
        P1Cmd::Offers(_) => st.phase1.stats[stat::OFFERS_SENT] += 1,
        P1Cmd::Gossip(_) => st.phase1.stats[stat::GOSSIP_SENT] += 1,
        P1Cmd::Sentiment(_) => st.phase1.stats[stat::SENTIMENTS_SENT] += 1,
        P1Cmd::PlaceHouse(_) => st.phase1.stats[stat::HOUSES_SENT] += 1,
        P1Cmd::SpawnResident(_) => st.phase1.stats[stat::RESIDENTS_SENT] += 1,
        P1Cmd::Steer { .. } => {}
    }
    One::Written(seq)
}

/// The sequence of the next kept command to resend, if any.
pub(super) fn next_resend(st: &State) -> Option<u64> {
    st.phase1.resend.front().map(|r| r.0)
}

/// Resends kept commands with sequences at or below `through` (all with `None`). Returns the
/// highest sequence written and whether the buffer filled.
pub(super) fn write_resends(
    st: &mut State,
    w: &mut RecordWriter<'_>,
    through: Option<u64>,
) -> (u64, bool) {
    let mut highest = 0;
    while let Some((seq, op, c)) = st.phase1.resend.front().cloned() {
        if through.is_some_and(|t| seq > t) {
            break;
        }
        match write_one(st, w, &c, Some(seq)) {
            One::Written(s) => {
                highest = highest.max(s);
                st.phase1.resend.pop_front();
            }
            One::Full => return (highest, true),
            One::Never => {
                st.phase1.resend.pop_front();
                st.phase1.outbox.retain(|(_, o, _)| *o != op);
                super::log_error(&format!(
                    "a resent {} (op {op:#x}) can never fit a command buffer; dropped",
                    c.kind_name()
                ));
            }
        }
    }
    (highest, false)
}

/// Writes the queue while it fits; kept commands join the outbox under the sequence they got.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    if !st.phase1.enabled {
        return write_queue_off(st, w);
    }
    let mut highest = 0;
    while let Some(c) = st.phase1.queue.front().cloned() {
        match write_one(st, w, &c, None) {
            One::Written(seq) => {
                highest = highest.max(seq);
                st.phase1.queue.pop_front();
                if let Some(op) = c.kept_op() {
                    st.phase1.outbox.push((seq, op, c));
                }
            }
            One::Full => return (highest, true),
            One::Never => {
                st.phase1.queue.pop_front();
                forget_unsent_spawn(st, &c);
                super::log_error(&format!(
                    "a phase 1 {} can never fit a command buffer; dropped",
                    c.kind_name()
                ));
            }
        }
    }
    (highest, false)
}

/// The queue with phase 1 switched off (U10, choice PR13): only the captures go out. Commands the
/// bridge answers by op id wait, unsequenced, until it is on again (a sequence is taken only when
/// written, so holding them never puts one below what Java already applied); the rest of this
/// session's commands are dropped.
fn write_queue_off(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    let mut kept = VecDeque::new();
    while let Some(c) = st.phase1.queue.pop_front() {
        if c.kept_op().is_some() {
            kept.push_back(c);
            continue;
        }
        if !matches!(c, P1Cmd::Despawn { .. }) {
            forget_unsent_spawn(st, &c);
            continue;
        }
        match write_one(st, w, &c, None) {
            One::Written(seq) => highest = highest.max(seq),
            One::Full => {
                st.phase1.queue.push_front(c);
                kept.extend(st.phase1.queue.drain(..));
                st.phase1.queue = kept;
                return (highest, true);
            }
            One::Never => {}
        }
    }
    st.phase1.queue = kept;
    (highest, false)
}

/// A SPAWN_GROUP dropped unwritten: its puppet would wait as spawning for the whole session (no
/// answer ever comes), blocking that group and taking a place under the cap.
fn forget_unsent_spawn(st: &mut State, c: &P1Cmd) {
    if let P1Cmd::Spawn { agent, epoch, .. } = c
        && st
            .phase1
            .puppets
            .get(agent)
            .is_some_and(|p| p.epoch == *epoch && p.state == PState::Spawning)
    {
        st.phase1.puppets.remove(agent);
    }
}

/// A test's scripted phase 1 order as a command.
pub(super) fn scripted(order: &Order) -> Option<P1Cmd> {
    Some(match order {
        Order::Market(m) => P1Cmd::Market(m.clone()),
        Order::MerchantOffers(o) => P1Cmd::Offers(o.clone()),
        Order::WriteGossip(g) => P1Cmd::Gossip(g.clone()),
        Order::Sentiment(s) => P1Cmd::Sentiment(s.clone()),
        Order::PlaceHouse(h) => P1Cmd::PlaceHouse(h.clone()),
        Order::SpawnResident(r) => P1Cmd::SpawnResident(r.clone()),
        _ => return None,
    })
}

/// Bytes a scripted phase 1 order writes (DICT records not included), and its names.
pub(super) fn scripted_bytes(order: &Order) -> usize {
    scripted(order).map_or(0, |c| c.bytes())
}

pub(super) fn scripted_names(order: &Order, out: &mut Vec<(u32, String)>) {
    if let Some(c) = scripted(order) {
        c.names(out);
    }
}

/// Writes a scripted phase 1 order (its unit was checked to fit). Kept ones join the outbox.
pub(super) fn write_scripted(
    st: &mut State,
    w: &mut RecordWriter<'_>,
    order: &Order,
) -> Option<u64> {
    let c = scripted(order)?;
    match write_one(st, w, &c, None) {
        One::Written(seq) => {
            if let Some(op) = c.kept_op() {
                st.phase1.outbox.push((seq, op, c));
            }
            Some(seq)
        }
        One::Full | One::Never => None,
    }
}

// ---- inspection (TEST mode) ------------------------------------------------------------------------

/// Every village's feelings toward players (TEST mode's inspection): village id to (UUID most,
/// least, sentiment).
pub(super) fn sentiments(st: &State) -> BTreeMap<u64, Vec<(u64, u64, i8)>> {
    let l = &st.phase1;
    let mut out = BTreeMap::new();
    for (&key, &i) in &l.nodes {
        if let Some(s) = l.world.sentiment_snapshot(i)
            && !s.entries.is_empty()
        {
            out.insert(key, s.entries);
        }
    }
    out
}

/// The villages and bases the phase 1 world knows (TEST mode's inspection).
pub(super) fn villages(st: &State) -> BTreeSet<u64> {
    st.phase1.nodes.keys().copied().collect()
}

/// Every village's works (TEST mode's inspection, SINK.md), indexed by `WORKS_*`: works bought,
/// kinds full, the purse now, golems the census counts, coin paid as tribute and coin spent on
/// works (thousandths, every currency summed).
pub(super) fn works_view(st: &State) -> BTreeMap<u64, [i64; WORKS_VIEW]> {
    let l = &st.phase1;
    let now = l.world.now();
    let sum = |v: &[(GoodId, Amount)]| v.iter().map(|&(_, a)| a).sum::<i64>();
    let mut out = BTreeMap::new();
    for (&key, &i) in &l.nodes {
        let Some(w) = l.world.works_of(i) else {
            continue;
        };
        out.insert(
            key,
            [
                i64::from(w.nth),
                w.full.len() as i64,
                sum(&l.world.works_purse(i, now)),
                i64::from(w.golems),
                sum(&w.tribute),
                sum(&w.spent),
            ],
        );
    }
    out
}

/// The length of a village's works view ([`works_view`]).
pub const WORKS_VIEW: usize = 6;

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::page::Durability;
    use std::sync::Mutex;

    use super::super::paging::Store;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-p1-unit-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A page store shared as the engine shares it (`paging::Store`).
    fn open(dir: &std::path::Path) -> Store {
        Arc::new(Mutex::new(Some(
            PageStore::open(dir, Durability::NoSync).unwrap(),
        )))
    }

    fn with<R>(s: &Store, f: impl FnOnce(&mut PageStore) -> R) -> R {
        f(s.lock().unwrap().as_mut().unwrap())
    }

    /// The phase 1 pages loaded as a restarted TEST engine loads them (no budget).
    fn load(s: &Store) -> Result<Link, String> {
        let source: Arc<dyn PageSource> = s.clone();
        Link::load(&source, usize::MAX, true)
    }

    /// Saves `l`'s meta page and loads it back as a restarted engine would.
    fn reload(l: &Link, tag: &str) -> Link {
        let dir = tmpdir(tag);
        let s = open(&dir);
        let mut b = SaveBatch::new();
        b.put(KEY_META, &encode_meta(l, true));
        with(&s, |store| store.commit(b, 1).unwrap());
        let back = load(&s).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        back
    }

    /// Review 3, P1: a save is taken at a step boundary, after `simulate` queued this step's
    /// decisions and before `write_commands` wrote them. A kept command (WRITE_GOSSIP,
    /// PLACE_HOUSE, SPAWN_RESIDENT) still in the queue was in neither the saved outbox nor, after a
    /// crash, anywhere else: the replay re-decides it and throws it away. It must come back.
    #[test]
    fn a_kept_command_still_queued_at_a_save_comes_back_after_a_crash() {
        let mut l = Link::empty();
        l.push(P1Cmd::Gossip(GossipCmd {
            op: 0x77,
            village: 1,
            player: (2, 3),
            gossip: 2,
            amount: 2,
            report: 3,
            flags: 0,
        }));
        // A latest-wins command is decided again after a restart: it need not come back.
        l.push(P1Cmd::Despawn {
            agent: 9,
            epoch: 1,
            reason: 1,
        });
        let mut back = reload(&l, "queued");
        back.resume(0);
        let ops: Vec<u64> = back
            .queue
            .iter()
            .filter_map(P1Cmd::kept_op)
            .chain(back.resend.iter().map(|r| r.1))
            .collect();
        assert_eq!(
            ops,
            vec![0x77],
            "the queued WRITE_GOSSIP survives the restart"
        );
        assert!(
            back.queue.iter().all(|c| c.kept_op().is_some()),
            "only kept commands come back"
        );
    }

    fn state() -> State {
        let (_tx, rx) = std::sync::mpsc::channel();
        State::new(rx, None)
    }

    /// Takes a save and commits it as the engine's `commit` does (stale phase 1 pages deleted
    /// with it), then hands the world its ticket back as the next step does.
    fn commit(s: &Store, st: &mut State) {
        let mut b = SaveBatch::new();
        let (saved, ticket) = snapshot(st, &mut b, true);
        with(s, |store| {
            for k in deletions(store, &saved).unwrap() {
                b.delete(k);
            }
            let at = store.obs_seq() + 1;
            store.commit(b, at).unwrap();
        });
        saved.done();
        world_done(st, ticket, true);
    }

    /// Takes a save whose commit fails on the IO lane.
    fn lose(st: &mut State) {
        let mut b = SaveBatch::new();
        let (_saved, ticket) = snapshot(st, &mut b, true);
        world_done(st, ticket, false);
    }

    /// Review 3, P4: the first census sets a village's population and later ones only raise it
    /// (choice V9), because growth books residents before they exist and a part-loaded village
    /// counts low. Which villages had their first census was forgotten at every restart, so the
    /// first census after one lowered the population again.
    #[test]
    fn a_census_after_a_restart_never_lowers_a_population() {
        let mut st = state();
        st.phase1.input(
            Input::Join {
                key: 5,
                culture: 0,
                frame: 0,
                x: 0,
                z: 0,
                pop: 30,
                resources: Vec::new(),
            },
            0,
        );
        census(&mut st, 5, 25, 0);
        census(&mut st, 5, 28, 0);
        let i = st.phase1.nodes[&5];
        assert_eq!(st.phase1.world.nodes[i].local.pop, 28);
        let dir = tmpdir("census");
        let s = open(&dir);
        commit(&s, &mut st);
        let mut st2 = state();
        st2.phase1 = load(&s).unwrap();
        census(&mut st2, 5, 20, 0);
        assert_eq!(
            st2.phase1.world.nodes[i].local.pop, 28,
            "a low census after the restart left the population alone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review 3, P5: a save whose pages never reached the disk (the IO lane's commit failed) must
    /// not count its log chunks as written, or the next save leaves a chunk stale and every later
    /// start fails ("the phase 1 log holds ... inputs, its meta page says ...").
    #[test]
    fn a_failed_save_never_leaves_a_gap_in_the_log() {
        let mut st = state();
        let push = |st: &mut State, n: usize| {
            for _ in 0..n {
                st.phase1.log.push(Logged {
                    at: 0,
                    input: Input::Population { key: 1, pop: 5 },
                });
            }
        };
        let dir = tmpdir("gap");
        let s = open(&dir);
        // Short tails (below a snapshot's threshold): the chunks themselves are what a failed
        // save must not count as written.
        push(&mut st, 600);
        commit(&s, &mut st);
        push(&mut st, 300);
        lose(&mut st); // the IO lane failed to write these
        push(&mut st, 50);
        commit(&s, &mut st);
        let back = load(&s).expect("the pages load");
        assert!(!back.dropped_pages);
        assert_eq!(back.log.len(), 950);
        // A long tail: the save takes a snapshot, and one that never reached the disk is taken
        // again by the next save rather than followed by a tail the disk cannot rebuild.
        push(&mut st, 1_200);
        lose(&mut st); // a snapshot the IO lane failed to write
        push(&mut st, 10);
        commit(&s, &mut st);
        let back = load(&s).expect("the pages load");
        assert!(!back.dropped_pages);
        assert_eq!(back.stats[stat::INPUTS], 2_160);
        assert_eq!(back.world.state_hash(), st.phase1.world.state_hash());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review 3, P7: with phase 1 switched off, a queued SPAWN_GROUP is dropped unwritten; its
    /// puppet must go with it, or it waits as spawning for the whole session, blocking that
    /// caravan and taking a place under the cap.
    #[test]
    fn a_spawn_dropped_while_phase1_is_off_leaves_no_puppet() {
        let mut st = state();
        st.phase1.enabled = false;
        st.phase1.push(P1Cmd::Spawn {
            agent: 0xA,
            epoch: 1,
            group_kind: "x".into(),
            dimension: "minecraft:overworld".into(),
            pos: [0.0; 3],
            flags: 0,
            members: Vec::new(),
        });
        st.phase1.puppets.insert(
            0xA,
            Puppet {
                epoch: 1,
                of: PuppetOf::Caravan(7),
                state: PState::Spawning,
                frame: 1,
                pos: [0.0; 3],
                pinned: false,
                leg_end: 0,
                last_steer: i64::MIN,
                offers_at: None,
            },
        );
        let mut buf = vec![0u8; 4096];
        let mut w = RecordWriter::new(&mut buf);
        write_queue(&mut st, &mut w);
        assert!(st.phase1.queue.is_empty());
        assert!(
            !st.phase1.puppets.contains_key(&0xA),
            "no puppet waits for a spawn never sent"
        );
    }

    /// Review 3, P6: epochs are new for every materialization, across sessions too (choice V26).
    /// The counter in the pages is the save's; epochs handed out after it came again after a
    /// crash, so a record of the lost session could match a new puppet.
    #[test]
    fn epochs_after_a_restart_are_new() {
        let mut l = Link::empty();
        l.next_epoch = 5;
        // Epochs 5 to 8 go out after the save, then the server dies.
        let back = reload(&l, "epochs");
        assert!(
            back.next_epoch > 8,
            "the restart hands out new epochs: next {}",
            back.next_epoch
        );
    }

    /// Review 3, P1 (follow-up): a kept command queued again after a restart leaves the queue
    /// when its COMMAND_RESULT comes (replayed from the journal: Java applied it before the
    /// crash). Written later, after the engine acknowledged that result and Java forgot the op
    /// id, Java would apply it a second time (a second house).
    #[test]
    fn a_queued_kept_command_leaves_when_its_result_comes() {
        let mut st = state();
        st.phase1.push(P1Cmd::Gossip(GossipCmd {
            op: 0x99,
            village: 1,
            player: (2, 3),
            gossip: 2,
            amount: 2,
            report: 3,
            flags: 0,
        }));
        let mut b = vec![0u8; obs::command_result::SIZE];
        wire::put_u16(&mut b, 0, obs::COMMAND_RESULT);
        wire::put_u16(&mut b, 2, obs::command_result::SIZE as u16);
        wire::put_u32(&mut b, 4, obs::command_result::APPLIED);
        wire::put_u64(&mut b, obs::command_result::OP_ID, 0x99);
        let rec = Rec {
            kind: obs::COMMAND_RESULT,
            aux: obs::command_result::APPLIED,
            bytes: &b,
        };
        after_core(&mut st, &rec);
        assert!(
            st.phase1.queue.iter().all(|c| c.kept_op() != Some(0x99)),
            "the answered command is not sent again"
        );
    }

    fn record(kind: u16, size: usize, aux: u32, fill: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut b = vec![0u8; size];
        wire::put_u16(&mut b, 0, kind);
        wire::put_u16(&mut b, 2, size as u16);
        wire::put_u32(&mut b, 4, aux);
        fill(&mut b);
        b
    }

    fn modules(st: &mut State, mask: u32) {
        let b = record(obs::MODULES, obs::modules::SIZE, 0, |b| {
            wire::put_u32(b, obs::modules::MASK, mask)
        });
        observe_state(
            st,
            &Rec {
                kind: obs::MODULES,
                aux: 0,
                bytes: &b,
            },
        );
    }

    /// U10 (Ben: "Ignore everything"): switched off, phase 1 captures its puppets back, ignores
    /// its records (a village adopted, a census), and its world does not run; on again it runs
    /// to now at once. Phase 1 runs only with all four of its modules on (choice PR13).
    #[test]
    fn a_switched_off_phase1_ignores_everything_and_catches_up() {
        let mut st = state();
        st.world
            .java_dict
            .insert((wire::DICT_CULTURE, 1), "human".into());
        st.world
            .java_dict
            .insert((wire::DICT_STRUCTURE, 1), "minecraft:village_plains".into());
        let adopt = |key: u64, game_time: i64| {
            record(obs::VILLAGE_ADOPTED, obs::village_adopted::SIZE, 0, |b| {
                wire::put_u64(b, obs::OBS_SEQ, 1);
                wire::put_i64(b, obs::GAME_TIME, game_time);
                wire::put_u64(b, obs::village_adopted::VILLAGE, key);
                wire::put_u32(b, obs::village_adopted::DIM, 1);
                wire::put_u32(b, obs::village_adopted::STRUCTURE, 1);
                wire::put_u32(b, obs::village_adopted::CULTURE, 1);
                wire::put_u32(b, obs::village_adopted::PIECES, 30);
                wire::put_i32(b, obs::village_adopted::MEET_X, (key as i32) * 300);
            })
        };
        fn rec(b: &[u8]) -> Rec<'_> {
            Rec {
                kind: obs::VILLAGE_ADOPTED,
                aux: 0,
                bytes: b,
            }
        }
        let first = adopt(1, 100);
        journaled(&mut st, &rec(&first));
        assert_eq!(st.phase1.nodes.len(), 1);
        st.game_time = 1_000;
        simulate(&mut st);
        assert_eq!(st.phase1.world.now().0, 1_000);
        st.phase1.puppets.insert(
            0xA,
            Puppet {
                epoch: 3,
                of: PuppetOf::Caravan(7),
                state: PState::Embodied,
                frame: 1,
                pos: [0.0; 3],
                pinned: false,
                leg_end: 0,
                last_steer: i64::MIN,
                offers_at: None,
            },
        );
        st.phase1.push(P1Cmd::Steer {
            agent: 0xA,
            epoch: 3,
            speed: 1.0,
            pos: [0.0; 3],
        });
        // Travel off: phase 1 is off.
        modules(&mut st, 0b0111);
        assert!(!st.phase1.enabled);
        assert!(
            st.phase1.queue.iter().all(|c| matches!(
                c,
                P1Cmd::Despawn {
                    agent: 0xA,
                    epoch: 3,
                    ..
                }
            )),
            "the puppet is captured back and nothing else of this session goes out"
        );
        assert_eq!(st.phase1.queue.len(), 1);
        // A command the bridge answers by op id, decided before the switch, waits unsequenced;
        // only the capture goes out.
        let gossip = P1Cmd::Gossip(GossipCmd {
            op: 0x77,
            village: 1,
            player: (2, 3),
            gossip: 2,
            amount: 2,
            report: 3,
            flags: 0,
        });
        st.phase1.push(gossip.clone());
        let mut buf = vec![0u8; 4096];
        {
            let mut w = RecordWriter::new(&mut buf);
            write_queue(&mut st, &mut w);
            assert_eq!(w.count(), 1, "only the capture is written");
        }
        assert_eq!(st.phase1.queue.len(), 1);
        assert!(st.phase1.outbox.is_empty());
        let second = adopt(2, 2_000);
        journaled(&mut st, &rec(&second));
        assert_eq!(st.phase1.nodes.len(), 1, "a record while off is ignored");
        assert_eq!(st.phase1.ignored, 1);
        st.game_time = 50_000;
        simulate(&mut st);
        assert_eq!(st.phase1.world.now().0, 1_000, "its world does not run");
        // On again: it runs to now at once.
        modules(&mut st, 0b1111);
        assert!(st.phase1.enabled);
        simulate(&mut st);
        assert_eq!(st.phase1.world.now().0, 50_000);
        let mut w = RecordWriter::new(&mut buf);
        write_queue(&mut st, &mut w);
        assert!(
            st.phase1.outbox.iter().any(|(_, op, _)| *op == 0x77),
            "the held command goes out once phase 1 is on"
        );
    }

    /// Review P3 (Ben: "Book what was really there"): a caravan member killed books exactly the
    /// items its MEMBER_STATE reported, not its planned cargo; the rest stays with the caravan.
    #[test]
    fn a_killed_member_books_what_its_member_state_held() {
        let mut st = state();
        let human = |key: u64, x: i32| Input::Join {
            key,
            culture: 0,
            frame: 1,
            x,
            z: 0,
            pop: 20,
            resources: catalog::resources("minecraft:village_plains", Culture::Human)
                .into_iter()
                .map(|(r, a)| (r.0, a))
                .collect(),
        };
        st.phase1.input(human(1, 0), 0);
        st.phase1.input(
            Input::Join {
                key: 2,
                culture: 0,
                frame: 1,
                x: 700,
                z: 0,
                pop: 18,
                resources: catalog::resources("minecraft:village_snowy", Culture::Human)
                    .into_iter()
                    .map(|(r, a)| (r.0, a))
                    .collect(),
            },
            0,
        );
        // A caravan with goods in a llama, promoted.
        let mut t = 0u64;
        let (id, member, g) = loop {
            t += 1_000;
            assert!(t < 200 * 24_000, "no loaded caravan");
            st.phase1.world.run_until(Tick(t));
            let w = &st.phase1.world;
            let key = (0..w.edges.len() as u32)
                .flat_map(|e| (0..2u8).map(move |d| (e, d)))
                .flat_map(|(e, d)| w.travellers(e, d))
                .find(|tr| {
                    tr.cargo
                        .iter()
                        .any(|(g, a)| !w.goods.is_currency(*g) && *a >= 2 * MILLI)
                })
                .map(|tr| tr.key);
            let Some(k) = key else { continue };
            let out = st.phase1.input(
                Input::Promote {
                    edge: k.edge,
                    dir: k.dir,
                    departure: k.departure,
                },
                t,
            );
            let id = out.caravan.expect("promoted");
            let car = st.phase1.world.caravans.get(id).unwrap();
            let found = car.members.iter().enumerate().find_map(|(i, m)| {
                m.cargo
                    .iter()
                    .find(|(g, a)| !st.phase1.world.goods.is_currency(**g) && **a >= MILLI)
                    .map(|(g, _)| (i, *g))
            });
            if let Some((m, g)) = found {
                break (id, m, g);
            }
        };
        let agent = AGENT_P1 | 0xBEEF;
        st.phase1.agents.insert(agent, id.to_bits());
        st.world
            .java_dict
            .insert((wire::DICT_ITEM, 5), st.phase1.cat.item(g).to_string());
        let before = st.phase1.world.caravans.get(id).unwrap().holdings();
        let out_before = st.phase1.world.outside.player_out.get(&g).copied();
        // Its chest held one of the good (its plan held more, or spread it over two llamas).
        let ms = record(
            obs::MEMBER_STATE,
            obs::member_state::size(1),
            member as u32,
            |b| {
                wire::put_u64(b, obs::OBS_SEQ, 7);
                wire::put_u64(b, obs::member_state::AGENT, agent);
                wire::put_u32(b, obs::member_state::EPOCH, 9);
                wire::put_u32(b, obs::member_state::ITEM_COUNT, 1);
                wire::put_u32(b, obs::member_state::ITEMS + obs::item::ITEM, 5);
                wire::put_i32(b, obs::member_state::ITEMS + obs::item::COUNT, 1);
            },
        );
        let died = record(obs::DIED, obs::died::SIZE, member as u32, |b| {
            wire::put_u64(b, obs::OBS_SEQ, 8);
            wire::put_u64(b, obs::died::AGENT, agent);
            wire::put_u32(b, obs::died::EPOCH, 9);
            wire::put_u64(b, obs::died::KILLER_MOST, 0x77);
            wire::put_u64(b, obs::died::KILLER_LEAST, 0x88);
            wire::put_u64(b, obs::died::OP_ID, 1 << 63 | 5);
        });
        let ms_rec = Rec {
            kind: obs::MEMBER_STATE,
            aux: member as u32,
            bytes: &ms,
        };
        let died_rec = Rec {
            kind: obs::DIED,
            aux: member as u32,
            bytes: &died,
        };
        st.world.apply_journaled(&ms_rec, false);
        st.world.apply_journaled(&died_rec, false);
        after_core(&mut st, &died_rec);
        let w = &st.phase1.world;
        assert_eq!(
            w.outside.player_out.get(&g).copied().unwrap_or(0) - out_before.unwrap_or(0),
            i128::from(MILLI),
            "exactly the one item its chest held left the sim"
        );
        if let Some(car) = w.caravans.get(id) {
            let after = car.holdings();
            let want = before.get(&g).copied().unwrap_or(0) - MILLI;
            assert_eq!(after.get(&g).copied().unwrap_or(0), want);
        }
        assert!(matches!(
            st.phase1.log.last().map(|l| &l.input),
            Some(Input::MemberHeld { held, killed: true, .. }) if held == &vec![(g.0, MILLI)]
        ));
        w.check_conserved(w.now()).unwrap();
    }

    /// P21 (Ben: "At border markets"): a caravan standing at a border market its road touches gets
    /// coin exchange in its MERCHANT_OFFERS, emeralds for amethyst and back; away from it, none.
    #[test]
    fn a_caravan_at_a_border_market_offers_coin_exchange() {
        let mut st = state();
        let join = |key: u64, ribbit: bool, x: i32| Input::Join {
            key,
            culture: u8::from(ribbit),
            frame: 1,
            x,
            z: 0,
            pop: 16,
            resources: catalog::resources(
                "minecraft:village_plains",
                if ribbit {
                    Culture::Ribbit
                } else {
                    Culture::Human
                },
            )
            .into_iter()
            .map(|(r, a)| (r.0, a))
            .collect(),
        };
        st.phase1.input(join(1, false, 0), 0);
        st.phase1.input(join(2, true, 600), 0);
        let (human, ribbit) = (st.phase1.nodes[&1], st.phase1.nodes[&2]);
        // A caravan on the border road, promoted.
        let mut t = 0u64;
        let id = loop {
            t += 1_000;
            assert!(t < 100 * 24_000, "no caravan on the border road");
            st.phase1.world.run_until(Tick(t));
            let w = &st.phase1.world;
            let k = w.nodes[human]
                .out
                .iter()
                .flat_map(|d| w.travellers(d.edge, d.dir))
                .map(|tr| tr.key)
                .next();
            let Some(k) = k else { continue };
            if let Some(id) = st
                .phase1
                .input(
                    Input::Promote {
                        edge: k.edge,
                        dir: k.dir,
                        departure: k.departure,
                    },
                    t,
                )
                .caravan
            {
                break id;
            }
        };
        let (emerald, amethyst) = (
            st.phase1.cat.good("minecraft:emerald").unwrap(),
            st.phase1.cat.good("minecraft:amethyst_shard").unwrap(),
        );
        // It carries both coins (as after trading both ways).
        {
            let car = st.phase1.world.caravans.get_mut(id).unwrap();
            car.give_purse(emerald, 10 * MILLI);
            car.give_purse(amethyst, 100 * MILLI);
        }
        let home = st.phase1.world.nodes[human].actor;
        let p = st.phase1.world.nodes[human].pos;
        let at_human = market_at(
            &st.phase1,
            id,
            p.frame,
            [f64::from(p.x) + 10.0, 64.0, f64::from(p.z)],
        );
        assert_eq!(at_human, Some(human));
        let offers = offers_of(&st.phase1, id, home, at_human);
        let exchange: Vec<&OfferCmd> = offers
            .iter()
            .filter(|o| {
                [o.cost_a.as_str(), o.result.as_str()]
                    .iter()
                    .all(|n| *n == "minecraft:emerald" || *n == "minecraft:amethyst_shard")
            })
            .collect();
        assert_eq!(exchange.len(), 2, "both ways: {offers:?}");
        assert!(
            exchange
                .iter()
                .any(|o| o.result == "minecraft:amethyst_shard"
                    && o.flags == cmd::merchant_offers::O_FLAG_SELLS)
        );
        assert!(
            exchange
                .iter()
                .any(|o| o.cost_a == "minecraft:amethyst_shard"
                    && o.flags == cmd::merchant_offers::O_FLAG_BUYS)
        );
        // Far from both ends: no exchange.
        let far = market_at(
            &st.phase1,
            id,
            p.frame,
            [f64::from(p.x) + 300.0, 64.0, 900.0],
        );
        assert_eq!(far, None);
        let _ = ribbit;
        assert!(offers_of(&st.phase1, id, home, None).iter().all(|o| {
            !(o.cost_a == "minecraft:emerald" && o.result == "minecraft:amethyst_shard")
        }));
    }

    /// Review 3, P2: which caravan a phase 1 agent is outlives the session, so a record about it
    /// replayed after a restart (a robbery, a trade, a member killed) still finds the caravan.
    #[test]
    fn a_puppets_caravan_is_known_after_a_restart() {
        let mut l = Link::empty();
        l.agents.insert(0xCA4A, 0x0000_0003_0000_0007);
        let back = reload(&l, "agents");
        assert_eq!(caravan_of(&back, 0xCA4A), Some(0x0000_0003_0000_0007));
    }

    /// S5: phase 1 pages this build cannot read (another build's format, a damaged page, a
    /// missing piece) are dropped with a loud log and the world starts afresh; the engine starts,
    /// and the next save deletes them.
    #[test]
    fn unreadable_phase1_pages_are_dropped_and_replaced() {
        let dir = tmpdir("unreadable");
        let s = open(&dir);
        let mut b = SaveBatch::new();
        let mut meta = encode_meta(&Link::empty(), true);
        meta[4] = 9; // a version this build does not read
        b.put(KEY_META, &meta);
        b.put(chunk_key(0), b"stale");
        b.put(piece_key(3), b"stale");
        with(&s, |store| store.commit(b, 1).unwrap());
        let l = load(&s).expect("the policy drops them, the start goes on");
        assert!(l.dropped_pages);
        assert!(l.purge, "start deletes them before the fresh world runs");
        assert!(l.nodes.is_empty());
        let mut st = state();
        st.phase1 = l;
        commit(&s, &mut st);
        assert_eq!(
            with(&s, |store| store
                .keys()
                .filter(|k| k.table() == TABLE || k.table() == SNAPSHOT_TABLE)
                .collect::<Vec<_>>()),
            vec![KEY_META],
            "the next save replaced them"
        );
        assert!(!load(&s).unwrap().dropped_pages);
        // A snapshot piece missing or cut short is the same case.
        let mut st = state();
        st.phase1.input(
            Input::Join {
                key: 5,
                culture: 0,
                frame: 0,
                x: 0,
                z: 0,
                pop: 12,
                resources: vec![(0, 60_000)],
            },
            0,
        );
        st.phase1.world.run_until(Tick(TAIL_MAX_TICKS));
        commit(&s, &mut st);
        assert!(
            with(&s, |store| store.contains(piece_key(0))
                && store.contains(WORLD_META)),
            "a snapshot was taken"
        );
        let mut b = SaveBatch::new();
        b.put(piece_key(0), b"short");
        with(&s, |store| store.commit(b, store.obs_seq()).unwrap());
        assert!(load(&s).unwrap().dropped_pages);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One op of a phase 1 input script, applied to a link the way the step applies the
    /// bridge's records (`Link::input`) and runs the world (`simulate`).
    #[derive(Clone, Debug)]
    enum P1Op {
        Join {
            x: i32,
            z: i32,
            ribbit: bool,
            plains: bool,
        },
        Census {
            village: usize,
            pop: u32,
        },
        Trade {
            village: usize,
            good: u16,
            items: i64,
            buy: bool,
            player: u8,
        },
        Promote {
            village: usize,
        },
        CaravanTrade {
            good: u16,
            items: i64,
            buy: bool,
            player: u8,
        },
        Rob {
            good: u16,
            amount: i64,
            player: u8,
        },
        Member {
            member: u32,
            killed: bool,
            player: u8,
        },
        CaravanReport {
            rescue: bool,
            player: u8,
        },
        Report {
            village: usize,
            kind: u8,
            player: u8,
        },
        Base {
            x: i32,
            z: i32,
            player: u8,
            mark: bool,
        },
        Advance {
            ticks: u64,
        },
        Save,
        /// A save whose commit fails on the IO lane.
        SaveLost,
    }

    fn p1_op() -> impl proptest::strategy::Strategy<Value = P1Op> {
        use proptest::prelude::*;
        prop_oneof![
            3 => (-3000i32..3000, -3000i32..3000, any::<bool>(), any::<bool>())
                .prop_map(|(x, z, ribbit, plains)| P1Op::Join { x, z, ribbit, plains }),
            1 => (0usize..20, 1u32..40).prop_map(|(village, pop)| P1Op::Census { village, pop }),
            3 => (0usize..20, 0u16..9, 1i64..40, any::<bool>(), 1u8..4)
                .prop_map(|(village, good, items, buy, player)| P1Op::Trade { village, good, items, buy, player }),
            3 => (0usize..20).prop_map(|village| P1Op::Promote { village }),
            1 => (0u16..9, 1i64..20, any::<bool>(), 1u8..4)
                .prop_map(|(good, items, buy, player)| P1Op::CaravanTrade { good, items, buy, player }),
            1 => (0u16..9, -8000i64..-1, 0u8..4).prop_map(|(good, amount, player)| P1Op::Rob { good, amount, player }),
            1 => (0u32..4, any::<bool>(), 0u8..4).prop_map(|(member, killed, player)| P1Op::Member { member, killed, player }),
            1 => (any::<bool>(), 1u8..4).prop_map(|(rescue, player)| P1Op::CaravanReport { rescue, player }),
            1 => (0usize..20, 1u8..9, 1u8..4).prop_map(|(village, kind, player)| P1Op::Report { village, kind, player }),
            1 => (-3000i32..3000, -3000i32..3000, 1u8..4, any::<bool>())
                .prop_map(|(x, z, player, mark)| P1Op::Base { x, z, player, mark }),
            3 => (1u64..60_000).prop_map(|ticks| P1Op::Advance { ticks }),
            2 => Just(P1Op::Save),
            1 => Just(P1Op::SaveLost),
        ]
    }

    fn player_of(p: u8) -> Uuid {
        if p == 0 {
            (0, 0)
        } else {
            (u64::from(p) << 8, u64::from(p) * 7919)
        }
    }

    /// The first caravan row, by handle: the same in two worlds in the same state.
    fn first_caravan(l: &Link) -> Option<u64> {
        l.world.caravans.iter().next().map(|(h, _)| h.to_bits())
    }

    /// Applies one op at world tick `*t` (advanced by the op), as the step would.
    fn apply_op(st: &mut State, store: &Store, o: &P1Op, t: &mut u64) {
        *t += 200;
        let at = *t;
        let l = &mut st.phase1;
        let village = |l: &Link, i: usize| l.nodes.keys().nth(i % l.nodes.len().max(1)).copied();
        match *o {
            P1Op::Join {
                x,
                z,
                ribbit,
                plains,
            } => {
                let key = 0x1000 + l.nodes.len() as u64;
                let c = if ribbit {
                    Culture::Ribbit
                } else {
                    Culture::Human
                };
                let structure = if plains {
                    "minecraft:village_plains"
                } else {
                    "minecraft:village_taiga"
                };
                let resources = catalog::resources(structure, c)
                    .into_iter()
                    .map(|(r, a)| (r.0, a))
                    .collect();
                l.input(
                    Input::Join {
                        key,
                        culture: u8::from(ribbit),
                        frame: 0,
                        x,
                        z,
                        pop: 12,
                        resources,
                    },
                    at,
                );
            }
            P1Op::Census { village: v, pop } => {
                if let Some(key) = village(l, v) {
                    let _ = l;
                    census(st, key, pop, 0);
                }
            }
            P1Op::Trade {
                village: v,
                good,
                items,
                buy,
                player,
            } => {
                if let Some(key) = village(l, v) {
                    let coins = (items / 4).max(1);
                    l.input(
                        Input::Trade {
                            key,
                            player: player_of(player),
                            good,
                            items,
                            coins,
                            buy,
                        },
                        at,
                    );
                }
            }
            P1Op::Promote { village: v } => {
                l.world.run_until(Tick(at));
                if let Some(key) = village(l, v) {
                    let i = l.nodes[&key];
                    let k = l.world.nodes[i].out.iter().find_map(|d| {
                        l.world
                            .travellers(d.edge, d.dir)
                            .into_iter()
                            .find(|x| !x.cargo.is_empty())
                            .map(|x| x.key)
                    });
                    if let Some(k) = k {
                        l.input(
                            Input::Promote {
                                edge: k.edge,
                                dir: k.dir,
                                departure: k.departure,
                            },
                            at,
                        );
                    }
                }
            }
            P1Op::CaravanTrade {
                good,
                items,
                buy,
                player,
            } => {
                if let Some(caravan) = first_caravan(l) {
                    let coins = (items / 4).max(1);
                    l.input(
                        Input::CaravanTrade {
                            caravan,
                            at_node: 0,
                            player: player_of(player),
                            good,
                            currency: 7,
                            buy,
                            items,
                            coins,
                        },
                        at,
                    );
                }
            }
            P1Op::Rob {
                good,
                amount,
                player,
            } => {
                if let Some(caravan) = first_caravan(l) {
                    l.input(
                        Input::Rob {
                            caravan,
                            player: player_of(player),
                            goods: vec![(good, amount)],
                        },
                        at,
                    );
                }
            }
            P1Op::Member {
                member,
                killed,
                player,
            } => {
                if let Some(caravan) = first_caravan(l) {
                    l.input(
                        Input::Member {
                            caravan,
                            member,
                            player: player_of(player),
                            killed,
                        },
                        at,
                    );
                }
            }
            P1Op::CaravanReport { rescue, player } => {
                if let Some(caravan) = first_caravan(l) {
                    l.input(
                        Input::CaravanReport {
                            caravan,
                            player: player_of(player),
                            rescue,
                        },
                        at,
                    );
                }
            }
            P1Op::Report {
                village: v,
                kind,
                player,
            } => {
                if let Some(key) = village(l, v) {
                    l.input(
                        Input::VillageReport {
                            key,
                            player: player_of(player),
                            kind,
                        },
                        at,
                    );
                }
            }
            P1Op::Base { x, z, player, mark } => {
                let key = 1 << 62 | (x.rem_euclid(64) as u64) << 24 | z.rem_euclid(64) as u64;
                let input = if mark {
                    Input::MarkBase {
                        key,
                        frame: 0,
                        x,
                        z,
                        player: player_of(player),
                    }
                } else {
                    Input::UnmarkBase {
                        key,
                        player: player_of(player),
                    }
                };
                l.input(input, at);
            }
            P1Op::Advance { ticks } => {
                *t += ticks;
                l.world.run_until(Tick(*t));
                let _ = l.world.take_commands();
            }
            P1Op::Save => commit(store, st),
            P1Op::SaveLost => lose(st),
        }
        // The step takes the world's commands after inputs and every simulate (here: every
        // other op, so some saves find commands not yet taken).
        if t.is_multiple_of(400) {
            take_world_commands(st);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// The phase 1 world restored from its pages (a snapshot and the tail since it) at any
        /// point of any input script, saves that failed included, has the state hash of an
        /// engine that never stopped, and both go on identically.
        #[test]
        fn a_restored_phase1_world_matches_one_that_never_stopped(
            ops in proptest::collection::vec(p1_op(), 1..60),
            restart in 0usize..60,
        ) {
            let (da, db) = (tmpdir("prop-a"), tmpdir("prop-b"));
            let (sa, sb) = (open(&da), open(&db));
            let (mut a, mut b) = (state(), state());
            let (mut ta, mut tb) = (0u64, 0u64);
            for (i, o) in ops.iter().enumerate() {
                if i == restart {
                    // B saves (or not: then it restarts from its last save, so it saves first)
                    // and starts again from its pages, as after a crash right after that save.
                    commit(&sb, &mut b);
                    let mut fresh = state();
                    fresh.phase1 = load(&sb).expect("B's pages load");
                    proptest::prop_assert!(!fresh.phase1.dropped_pages);
                    proptest::prop_assert!(fresh.phase1.replay_diverged.is_none());
                    proptest::prop_assert_eq!(fresh.phase1.world.state_hash(), b.phase1.world.state_hash());
                    proptest::prop_assert_eq!(&fresh.phase1.nodes, &b.phase1.nodes);
                    proptest::prop_assert_eq!(&fresh.phase1.censused, &b.phase1.censused);
                    b = fresh;
                    // A takes the same save, so both take snapshots at the same points after.
                    commit(&sa, &mut a);
                }
                apply_op(&mut a, &sa, o, &mut ta);
                apply_op(&mut b, &sb, o, &mut tb);
                proptest::prop_assert_eq!(a.phase1.world.state_hash(), b.phase1.world.state_hash(), "after op {} ({:?})", i, o);
            }
            let end = ta + 3 * 24_000;
            a.phase1.world.run_until(Tick(end));
            b.phase1.world.run_until(Tick(end));
            proptest::prop_assert_eq!(a.phase1.world.save(), b.phase1.world.save());
            // And a last restart of each from its own pages reads the same world back.
            commit(&sa, &mut a);
            commit(&sb, &mut b);
            let (ra, rb) = (load(&sa).unwrap(), load(&sb).unwrap());
            proptest::prop_assert_eq!(ra.world.state_hash(), a.phase1.world.state_hash());
            proptest::prop_assert_eq!(rb.world.state_hash(), b.phase1.world.state_hash());
            proptest::prop_assert_eq!(ra.stats[stat::INPUTS], a.phase1.stats[stat::INPUTS]);
            let _ = std::fs::remove_dir_all(&da);
            let _ = std::fs::remove_dir_all(&db);
        }
    }
}
