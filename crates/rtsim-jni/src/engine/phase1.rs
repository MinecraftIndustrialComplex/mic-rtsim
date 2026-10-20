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
//! - **Pages** ([`Link::load`], [`snapshot`]): table 0x5000 (choice V7): a meta page, then the input log
//!   in chunks of 1,024 inputs; a save writes the meta page and the chunks that changed.
//!
//! A test's scripted village ([`directive`], `ScriptVillage`) is left alone entirely: its
//! observations are not applied and no command of the engine's own names it.

mod catalog;
mod cmds;
mod log;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Reader, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::{Amount, GoodId, MILLI, OfferSide};
use rtsim_travel::caravan::{Body, GroupKind};
use rtsim_world::bases::CaravanRef;
use rtsim_world::{CaravanId, Command, Visible, World, WorldConfig, identity_op};

use super::{State, log_info, log_warn};
use crate::orders::{
    GossipCmd, ItemOrder, MarketCmd, MarketGood, MemberOrder, OfferCmd, OffersCmd, Order,
    PlaceHouseCmd, SentimentCmd, SpawnResidentCmd,
};
use crate::wire::{self, Rec, RecordWriter, cmd, obs};

pub(super) use cmds::P1Cmd;
use log::{Input, Logged, Uuid};

/// The phase 1 pages: table 0x5000 ('P'; table 1 is the core's and the industry link's key, the
/// industry engine's are 0x4900 to 0x4902).
pub const TABLE: u16 = 0x5000;
pub const KEY_META: PageKey = PageKey::new(TABLE, 0);
/// Inputs per log page.
pub const CHUNK: usize = 1024;
const META_MAGIC: u32 = u32::from_le_bytes(*b"RTP1");
const META_VERSION: u16 = 1;

fn chunk_key(k: usize) -> PageKey {
    PageKey::new(TABLE, 1 + k as u64)
}

/// Java's module bits (`config/Module.java` ordinals) phase 1 runs under: knowledge,
/// settlements, trade and travel. With all four off it decides nothing.
pub const MODULES_PHASE1: u32 = 0b1111;

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
}
pub const STATS: usize = 16;

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
    log: Vec<Logged>,
    /// Log chunks below this are in pages on disk and never change. Only a commit that succeeded
    /// moves it ([`Saved::done`]): a save whose pages never reached the disk must not count.
    clean_chunks: Arc<AtomicUsize>,
    /// World ticks ahead of game time (a test's P1_ADVANCE).
    offset: u64,
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
    enabled: bool,
    pub(super) stats: [u64; STATS],
    /// Captured cargo that did not match the sim's row at a despawn (logged, not reconciled).
    pub(super) capture_mismatches: u64,
    /// The last load replayed the log to a state other than the one saved (`state_hash`): the
    /// world is still the replayed one, but the build changed what the inputs mean.
    pub(super) replay_diverged: Option<(u64, u64)>,
}

fn fresh_world() -> (World, catalog::Catalog) {
    let cat = catalog::Catalog::new();
    let c2 = catalog::Catalog::new();
    let world = World::build(WorldConfig::new(WORLD_SEED), c2.goods, c2.book, Vec::new());
    (world, cat)
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
            clean_chunks: Arc::new(AtomicUsize::new(0)),
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
            stats: [0; STATS],
            capture_mismatches: 0,
            replay_diverged: None,
        }
    }

    /// Loads the phase 1 pages and rebuilds the world by replaying its input log.
    pub(super) fn load(store: &mut PageStore) -> Result<Link, String> {
        let mut link = Link::empty();
        let Some(meta) = store
            .get(KEY_META)
            .map_err(|e| format!("cannot read the phase 1 meta page: {e}"))?
        else {
            return Ok(link);
        };
        let m = decode_meta(&meta).map_err(|e| format!("the phase 1 meta page: {e}"))?;
        let chunks = m.log_len.div_ceil(CHUNK);
        let mut log = Vec::with_capacity(m.log_len);
        for k in 0..chunks {
            let b = store
                .get(chunk_key(k))
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
        let t0 = std::time::Instant::now();
        for l in &log {
            log::apply(&mut link.world, &mut link.nodes, &l.input, l.at);
        }
        link.world.run_until(Tick(m.now));
        // Commands the replay re-decided were decided before the save: those still unanswered
        // are in the kept outbox, the rest (latest wins) are decided again this session.
        let _ = link.world.take_commands();
        let hash = link.world.state_hash();
        if hash != m.hash {
            link.replay_diverged = Some((m.hash, hash));
            log_warn(&format!(
                "the phase 1 world replayed to state {hash:016x}, the save had {:016x}: this build reads the inputs differently (the replayed world stands)",
                m.hash
            ));
        }
        log_info(&format!(
            "phase 1 world rebuilt from {} inputs to world tick {} in {} ms: {} nodes, {} caravans, state {:016x}",
            log.len(),
            m.now,
            t0.elapsed().as_millis(),
            link.world.nodes.len(),
            link.world.caravans.len(),
            link.world.state_hash()
        ));
        link.clean_chunks
            .store(log.len() / CHUNK, Ordering::Release);
        // Villages whose first census is in the log (choice V9: later ones only raise).
        link.censused = log
            .iter()
            .filter_map(|l| match l.input {
                Input::Population { key, .. } => Some(key),
                _ => None,
            })
            .collect();
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
        link.stats[stat::INPUTS] = link.log.len() as u64;
        Ok(link)
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
        self.stats[stat::INPUTS] = self.log.len() as u64;
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

// ---- the meta page -------------------------------------------------------------------------------

struct Meta {
    now: u64,
    hash: u64,
    offset: u64,
    next_epoch: u32,
    log_len: usize,
    scripted: BTreeSet<u64>,
    outbox: Vec<(u64, u64, P1Cmd)>,
    agents: BTreeMap<u64, u64>,
    queued: Vec<P1Cmd>,
}

fn encode_meta(l: &Link) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u32(META_MAGIC);
    w.u16(META_VERSION);
    w.u64(l.world.now().0);
    w.u64(l.world.state_hash());
    w.u64(l.offset);
    w.u32(l.next_epoch);
    w.var_u64(l.log.len() as u64);
    w.var_u64(l.scripted.len() as u64);
    l.scripted.iter().for_each(|v| w.u64(*v));
    w.var_u64(l.outbox.len() as u64);
    for (seq, op, c) in &l.outbox {
        w.u64(*seq);
        w.u64(*op);
        cmds::encode_kept(&mut w, c);
    }
    // Added by review 3 at the end, so a meta page written before reads as having none.
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
    let mut scripted = BTreeSet::new();
    for _ in 0..r.var_u64()? {
        scripted.insert(r.u64()?);
    }
    let mut outbox = Vec::new();
    for _ in 0..r.var_u64()? {
        let seq = r.u64()?;
        let op = r.u64()?;
        outbox.push((seq, op, cmds::decode_kept(&mut r)?));
    }
    let mut agents = BTreeMap::new();
    let mut queued = Vec::new();
    if !r.is_empty() {
        for _ in 0..r.var_u64()? {
            let a = r.u64()?;
            agents.insert(a, r.u64()?);
        }
        for _ in 0..r.var_u64()? {
            queued.push(cmds::decode_kept(&mut r)?);
        }
    }
    r.finish()?;
    Ok(Meta {
        now,
        hash,
        offset,
        next_epoch,
        log_len,
        scripted,
        outbox,
        agents,
        queued,
    })
}

/// What a commit that wrote a phase 1 snapshot reports when it succeeded.
pub(super) struct Saved {
    clean: Arc<AtomicUsize>,
    chunks: usize,
}

impl Saved {
    /// The snapshot's pages are on disk: its full log chunks never need writing again.
    pub(super) fn done(&self) {
        self.clean.fetch_max(self.chunks, Ordering::AcqRel);
    }
}

/// The pages a save writes: the meta page, and every log chunk not yet on disk in full. The
/// commit calls [`Saved::done`] once they are.
pub(super) fn snapshot(st: &mut State) -> (Vec<(PageKey, Vec<u8>)>, Saved) {
    let l = &mut st.phase1;
    let caravans = &l.world.caravans;
    l.agents
        .retain(|_, bits| caravans.get(Handle::from_bits(*bits)).is_some());
    let mut out = vec![(KEY_META, encode_meta(l))];
    let chunks = l.log.len().div_ceil(CHUNK);
    for k in l.clean_chunks.load(Ordering::Acquire)..chunks {
        let mut w = Writer::with_capacity(CHUNK * 24);
        for e in &l.log[k * CHUNK..((k + 1) * CHUNK).min(l.log.len())] {
            e.encode(&mut w);
        }
        out.push((chunk_key(k), w.into_bytes()));
    }
    let saved = Saved {
        clean: Arc::clone(&l.clean_chunks),
        chunks: l.log.len() / CHUNK,
    };
    (out, saved)
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
    l.enabled = module_mask & MODULES_PHASE1 != 0;
    l.last_interest = i64::MIN;
    l.last_market = i64::MIN;
}

pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            st.phase1.enabled = wire::u32_at(b, obs::modules::MASK) & MODULES_PHASE1 != 0;
        }
        obs::PLAYER if rec.size() >= obs::player::SIZE => {
            let key = (
                wire::u64_at(b, obs::player::UUID_MOST),
                wire::u64_at(b, obs::player::UUID_LEAST),
            );
            let seen = Seen {
                frame: rec.aux as u16,
                pos: [
                    wire::f64_at(b, obs::player::X),
                    wire::f64_at(b, obs::player::Y),
                    wire::f64_at(b, obs::player::Z),
                ],
                at: st.game_time,
            };
            st.phase1.players.insert(key, seen);
        }
        obs::PUPPET_STATE if rec.size() >= obs::puppet_state::SIZE => {
            let agent = wire::u64_at(b, obs::puppet_state::AGENT);
            let epoch = wire::u32_at(b, obs::puppet_state::EPOCH);
            if rec.aux == 0
                && let Some(p) = st.phase1.puppets.get_mut(&agent)
                && p.epoch == epoch
            {
                p.pos = [
                    wire::f64_at(b, obs::puppet_state::X),
                    wire::f64_at(b, obs::puppet_state::Y),
                    wire::f64_at(b, obs::puppet_state::Z),
                ];
            }
        }
        obs::VILLAGE_CENSUS if rec.size() >= obs::village_census::ENTRIES => {
            let key = wire::u64_at(b, obs::village_census::VILLAGE);
            let residents = wire::u32_at(b, obs::village_census::RESIDENTS);
            census(st, key, residents);
        }
        // VILLAGE_GOSSIP is vanilla's own record of players; the engine reads it as latest
        // state and acts on none of it yet (the knowledge crate keeps its own).
        _ => {}
    }
}

/// A census: the first one sets the village's population (its adoption guessed it from its
/// pieces); later ones only raise it, since growth books residents before they exist (P17) and a
/// census of a part-loaded village counts low (choice V9).
fn census(st: &mut State, key: u64, residents: u32) {
    let l = &mut st.phase1;
    if l.scripted.contains(&key) || residents == 0 {
        return;
    }
    let Some(&i) = l.nodes.get(&key) else {
        return;
    };
    let pop = l.world.nodes[i].local.pop;
    // The first census is logged even when it changes nothing: the log is what tells a later
    // session which villages had theirs.
    let first = l.censused.insert(key);
    if first || residents > pop {
        let at = l.wt(st.game_time);
        l.input(
            Input::Population {
                key,
                pop: residents,
            },
            at,
        );
    }
}

// ---- hooks: journaled records ---------------------------------------------------------------------

/// The journaled kinds this module applies (the core's are seen after the core in
/// [`after_core`]).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    (obs::VILLAGE_ADOPTED..=obs::HOUSE_PLACED).contains(&rec.kind)
}

/// Op ids the kernel's `Replay` applies once: a trade's and a robbery's transfer.
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    let b = rec.bytes;
    match rec.kind {
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

/// A phase 1 record `Replay` admitted.
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    let at = st.phase1.wt(wire::i64_at(b, obs::GAME_TIME));
    match rec.kind {
        obs::VILLAGE_ADOPTED if rec.size() >= obs::village_adopted::SIZE => adopted(st, rec, at),
        obs::TRADE if rec.size() >= obs::trade::SIZE => trade(st, rec, at),
        obs::HURT if rec.size() >= obs::hurt::SIZE => hurt(st, rec, at),
        obs::RESCUE if rec.size() >= obs::rescue::SIZE => rescue(st, rec, at),
        obs::CARGO_CHANGED if rec.size() >= obs::cargo_changed::ITEMS => cargo(st, rec, at),
        obs::BASE if rec.size() >= obs::base::SIZE => {
            let key = wire::u64_at(b, obs::base::BASE);
            let player = uuid_at(b, obs::base::PLAYER_MOST);
            let input = if rec.aux == obs::base::MARKED {
                Input::MarkBase {
                    key,
                    frame: wire::u32_at(b, obs::base::DIM) as u16,
                    x: wire::f64_at(b, obs::base::X).floor() as i32,
                    z: wire::f64_at(b, obs::base::Z).floor() as i32,
                    player,
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
            let at = l.world.now().0;
            l.input(
                Input::Member {
                    caravan: bits,
                    member,
                    player,
                    killed,
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
    let target = st.phase1.wt(st.game_time);
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
        };
        st.phase1.push(cmd);
    }
}

fn dist2(a: [f64; 3], x: f64, z: f64) -> f64 {
    (a[0] - x).powi(2) + (a[2] - z).powi(2)
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
    let offers = offers_of(l, id, home_actor);
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
        },
    );
    true
}

/// A caravan's offers as MERCHANT_OFFERS (PLAN.md §8.5): priced fair (MERCHANT_OFFERS is not
/// per player; choice V10), uses as the sim allows.
fn offers_of(l: &Link, id: CaravanId, as_actor: rtsim_knowledge::ActorId) -> Vec<OfferCmd> {
    let mut out = Vec::new();
    for o in l.world.caravan_offers(CaravanRef::Row(id), as_actor) {
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
            let seen = l.players.values().any(|s| {
                s.frame == p.frame && dist2(s.pos, p.pos[0], p.pos[2]) <= DESPAWN_RADIUS.powi(2)
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
            if moved_on || !seen {
                p.state = PState::Despawning;
                despawn.push((agent, p.epoch));
            } else if let Some((x, z)) = target
                && (p.last_steer == i64::MIN || gt - p.last_steer >= STEER_TICKS)
            {
                p.last_steer = gt;
                steer.push((
                    agent,
                    p.epoch,
                    [f64::from(x) + 0.5, p.pos[1], f64::from(z) + 0.5],
                ));
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
                    && dist2(s.pos, p.pos[0], p.pos[2]) <= f64::from(SPAWN_RADIUS).powi(2)
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
            let (x, z) = v.pos();
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
    for s in l.players.values() {
        let (x, z) = (s.pos[0].floor() as i64, s.pos[2].floor() as i64);
        for cx in (x - r).div_euclid(MARKET_CELL)..=(x + r).div_euclid(MARKET_CELL) {
            for cz in (z - r).div_euclid(MARKET_CELL)..=(z + r).div_euclid(MARKET_CELL) {
                if let Some(v) = l.cells.get(&(s.frame, cx, cz)) {
                    candidates.extend(v.iter().copied());
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
                    && dist2(s.pos, f64::from(n.pos.x), f64::from(n.pos.z)) <= MARKET_RADIUS.powi(2)
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
    let mut highest = 0;
    while let Some(c) = st.phase1.queue.front().cloned() {
        if !st.phase1.enabled && c.kept_op().is_none() && !matches!(c, P1Cmd::Despawn { .. }) {
            // Phase 1 is off: nothing new materializes and no snapshot goes out; kept commands
            // and captures still do.
            st.phase1.queue.pop_front();
            forget_unsent_spawn(st, &c);
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::page::{Durability, SaveBatch};

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

    /// Saves `l`'s meta page and loads it back as a restarted engine would.
    fn reload(l: &Link, tag: &str) -> Link {
        let dir = tmpdir(tag);
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(KEY_META, &encode_meta(l));
        store.commit(b, 1).unwrap();
        let back = Link::load(&mut store).unwrap();
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

    fn commit(store: &mut PageStore, (pages, saved): (Vec<(PageKey, Vec<u8>)>, Saved)) {
        let mut b = SaveBatch::new();
        for (k, v) in &pages {
            b.put(*k, v);
        }
        let at = store.obs_seq() + 1;
        store.commit(b, at).unwrap();
        saved.done();
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
        census(&mut st, 5, 25);
        census(&mut st, 5, 28);
        let i = st.phase1.nodes[&5];
        assert_eq!(st.phase1.world.nodes[i].local.pop, 28);
        let dir = tmpdir("census");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        commit(&mut store, snapshot(&mut st));
        let mut st2 = state();
        st2.phase1 = Link::load(&mut store).unwrap();
        census(&mut st2, 5, 20);
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
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        push(&mut st, 1_500);
        commit(&mut store, snapshot(&mut st));
        push(&mut st, 700);
        let _lost = snapshot(&mut st); // the IO lane failed to write these
        push(&mut st, 100);
        commit(&mut store, snapshot(&mut st));
        let back = Link::load(&mut store).expect("the pages load");
        assert_eq!(back.log.len(), 2_300);
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
        let saved = encode_meta(&l);
        // Epochs 5 to 8 go out after the save, then the server dies.
        let dir = tmpdir("epochs");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        commit(
            &mut store,
            (
                vec![(KEY_META, saved)],
                Saved {
                    clean: Arc::new(AtomicUsize::new(0)),
                    chunks: 0,
                },
            ),
        );
        let back = Link::load(&mut store).unwrap();
        assert!(
            back.next_epoch > 8,
            "the restart hands out new epochs: next {}",
            back.next_epoch
        );
        let _ = std::fs::remove_dir_all(&dir);
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

    /// Review 3, P2: which caravan a phase 1 agent is outlives the session, so a record about it
    /// replayed after a restart (a robbery, a trade, a member killed) still finds the caravan.
    #[test]
    fn a_puppets_caravan_is_known_after_a_restart() {
        let mut l = Link::empty();
        l.agents.insert(0xCA4A, 0x0000_0003_0000_0007);
        let back = reload(&l, "agents");
        assert_eq!(caravan_of(&back, 0xCA4A), Some(0x0000_0003_0000_0007));
    }
}
