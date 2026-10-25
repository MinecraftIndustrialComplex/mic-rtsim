// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fake Java side, on the real host: mic_native is started in this process, the engine's steps
//! run on its background pool and its saves on its IO lane, exactly as in game. [`Java`] is a Rust
//! copy of the bridge's `Handover` and `BoundarySession` rules: it writes observations into the open
//! buffer, publishes with release, submits, polls the command buffers with acquire, applies commands
//! (skipping by sequence, then by op id) and trims its journal on acknowledgements. With an industry
//! world ([`Java::mc`], rtsim-industry's fake world in remote mode) it also plays the industry module
//! of docs/industry-boundary.md.
//!
//! Lifted out of rtsim-jni's tests (which use it from here) for the headless host (VIEWER.md §12):
//! there it plays a server where nobody is online, answering every command as Java does when it
//! cannot act ([`Policy::Refuse`]).
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rtsim_industry::cmd::Cmd;
use rtsim_industry::fake::{FakeWorld, Outgoing};
use rtsim_industry::obs::Timed;
use rtsim_industry::wire as iw;
use rtsim_jni::buffers::Buf;
use rtsim_jni::engine::{Config, Engine, Exec};
use rtsim_jni::orders::Order;
use rtsim_jni::wire::{self, RecordWriter, Records, cmd, obs};
use rtsim_kernel::page::Durability;

/// docs/industry-boundary.md §2.2.
pub const DICT_GOOD: u32 = 0x10;
pub const DICT_NAME: u32 = 0x11;
pub const DICT_BLOCK_STATE: u32 = 0x12;

/// Starts mic_native's host in this process (two foreground and two background threads) and
/// registers the engine as its in-process plugin, once; later calls do nothing.
pub fn host() {
    host_with(2, 2);
}

/// [`host`] with the pools' sizes; the first call in the process decides them.
pub fn host_with(fg_threads: usize, bg_threads: usize) {
    static HOST: OnceLock<()> = OnceLock::new();
    HOST.get_or_init(|| {
        mic_native_host::start(mic_native_host::Config {
            fg_threads,
            bg_threads,
        })
        .expect("start the host");
        let table = mic_native_host::register_in_process("mic_rtsim").expect("register");
        // SAFETY: the table the host just made for this in-process plugin.
        unsafe { mic_native_rt::init(table) }.expect("init the runtime");
    });
}

pub fn tmpdir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "rtsim-jni-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

pub fn engine(dir: &PathBuf, capacity: usize) -> Arc<Engine> {
    engine_with(dir, capacity, |_| {})
}

/// [`engine`] with the configuration adjusted first.
pub fn engine_with(dir: &PathBuf, capacity: usize, f: impl FnOnce(&mut Config)) -> Arc<Engine> {
    host();
    let mut cfg = Config::new(dir);
    cfg.capacity = capacity;
    cfg.test = true;
    cfg.durability = Durability::NoSync;
    cfg.exec = Exec::Host;
    f(&mut cfg);
    Engine::open(cfg).expect("open")
}

pub const OVERWORLD: &str = "minecraft:overworld";
pub const DIAMOND: &str = "minecraft:diamond";
pub const CARAVAN: &str = "mic_rtsim:human_caravan";

/// What [`Java`] does with the engine's commands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Plays them, as the tests' world does: transfers land in containers, groups materialize,
    /// modules' commands go to their fake plugins.
    #[default]
    Play,
    /// Answers each as Java does when it cannot act (the headless host, VIEWER.md §12.1: a server
    /// where nobody is online and nothing is loaded): see [`Java::refuse`]. Nothing is recorded
    /// per command, so a ten-year soak holds no growing list.
    Refuse,
}

/// SPAWN_REJECTED (and the modules' analogues) reason 1: the position is not ticking
/// (docs/boundary.md §2.3).
pub const NOT_TICKING: u32 = 1;

/// The Java side: one session of the bridge's boundary, as `BoundarySession` and `Handover` run it.
pub struct Java {
    pub e: Arc<Engine>,
    // the journal (saved with the "world")
    pub journal: VecDeque<(u64, Vec<u8>)>,
    /// The newest acknowledged JOURNAL_GEN, kept past the trim and sent first at a session start
    /// (Java's `Journal.pinnedGen`, docs/boundary.md §3.2).
    pub pinned_gen: Option<(u64, Vec<u8>)>,
    pub next_obs: u64,
    pub last_applied_cmd: u64,
    pub dict: Vec<(u32, String)>,
    pub applied_cmd_ops: HashSet<u64>,
    // per session
    /// This session's generation (docs/boundary.md §3.2): its JOURNAL_GEN's nonce, which the
    /// records a holder carries are stamped with.
    pub nonce: u64,
    pub pages_obs: u64,
    pub journal_first_at_start: u64,
    pub world_next_at_start: u64,
    pub replay_up_to: u64,
    pub submitted: u64,
    pub next_cmd: u64,
    pub cursor: usize,
    pub count: u32,
    pub open_flags: u32,
    pub open_highest: u64,
    pub sent_obs: u64,
    pub dict_sent: usize,
    pub session_start_pending: bool,
    pub pending_ack: u64,
    pub tick_marked: i64,
    pub game_time: i64,
    pub cmd_dict: HashMap<(u32, u32), String>,
    // what Minecraft did
    pub containers: HashMap<[i32; 3], i32>,
    pub commands: Vec<(u64, u16, u64)>,
    pub acks: Vec<u64>,
    pub stale_commands: u64,
    /// Read and hand back command buffers without applying anything (a world save that will be
    /// lost: whatever Minecraft did is rolled back with it).
    pub hold_commands: bool,
    /// The op ids of transfers read while holding.
    pub held_ops: Vec<u64>,
    /// Leave acknowledgements unapplied (the journal untrimmed): the world save a test takes next
    /// is one the bridge wrote before it read the acknowledgement.
    pub hold_acks: bool,
    /// The FAILED flag was seen on a command buffer.
    pub failed_seen: bool,
    /// SESSION_START's module mask (the bridge's `Module` bits).
    pub module_mask: u32,
    /// Phase 1 commands applied, in order: (kind, record bytes).
    pub p1: Vec<(u16, Vec<u8>)>,
    /// TALK_FACTS records applied, in order (their records).
    pub talk_facts: Vec<Vec<u8>>,
    /// Answer every new PLACE_HOUSE NOT_FREE (no plot), as HouseBuilder does when its search
    /// finds none.
    pub no_free_plot: bool,
    /// Answer every new TRANSFER_TO_CONTAINER FAILED (the container's chunk not loaded, or no
    /// room), as CommandDispatcher does.
    pub fail_transfers: bool,
    /// Players reported every tick: (UUID, dimension id, position).
    pub players: Vec<((u64, u64), u32, [f64; 3])>,
    /// The industry world this Java plays (rtsim-industry's fake world in remote mode), if any.
    pub mc: Option<FakeWorld>,
    /// The cmdSeq of the industry command that carried each engine op id (for COMMAND_RESULT).
    pub ind_cmd_seq: HashMap<u64, u64>,
    /// Industry commands applied (not stale): (hand-over, cmdSeq, command as decoded, dimension
    /// in the engine's dictionary).
    pub ind_commands: Vec<(u64, u64, Cmd)>,
    /// Industry commands read while holding: (cmdSeq, command).
    pub held_ind: Vec<(u64, Cmd)>,
    /// Answer the next IND_CARRIAGE_DELTA failed (its train gone), after asking for pages that
    /// stop right before that answer; then `failed_carriage` is (cmdSeq, op id, through).
    pub fail_next_carriage: bool,
    pub failed_carriage: Option<(u64, u64, u64)>,
    /// Dragon commands applied, as the Ice and Fire plugin would (docs/boundary.md §6.5):
    /// (cmdSeq, command decoded with the reference codec, its dimension in Java's dictionary).
    pub iaf: Vec<(u64, rtsim_ecology::boundary::Cmd)>,
    /// The far view's FAR_LEG records as they arrived (docs/boundary.md §12), names still the
    /// engine's dictionary ids (`cmd_dict`).
    pub far: Vec<rtsim_ecology::far::Decoded>,
    /// Ship commands applied, as a ships plugin would (docs/boundary.md §7.5): (cmdSeq, command
    /// decoded with the reference codec, its body, item and dimension ids in Java's dictionary).
    pub ships: Vec<(u64, rtsim_ships::boundary::Cmd)>,
    /// Herd commands applied, as the core's vanilla herd adapter would (docs/boundary.md §8.5):
    /// (cmdSeq, command decoded with the reference codec, its dimension in Java's dictionary).
    pub herds: Vec<(u64, rtsim_ecology::herds::boundary::Cmd)>,
    /// The vanilla herd adapter this Java plays: rtsim-ecology's fake plugin
    /// (`herds::synth::FakeHerds`), which answers the commands and plays kills, shearing, leads.
    pub herd_plugin: rtsim_ecology::herds::synth::FakeHerds,
    /// Conflict commands applied, as a conflict plugin would (docs/boundary.md §9.5): (cmdSeq,
    /// command decoded with the reference codec, its body, item and dimension ids in Java's
    /// dictionary).
    pub conflict: Vec<(u64, rtsim_conflict::boundary::Cmd)>,
    /// The conflict plugin this Java plays: rtsim-conflict's fake plugin (`synth::FakeConflict`),
    /// which answers envoys and fights bands as its script says.
    pub conflict_plugin: rtsim_conflict::synth::FakeConflict,
    /// HABITAT_KNOWN commands applied (docs/boundary.md §10.5): (cmdSeq, record, its dimension
    /// as Java's dictionary names it, or None for END).
    pub known: Vec<(u64, rtsim_space::KnownRecord, Option<String>)>,
    /// Play the commands or refuse them ([`Policy`]).
    pub policy: Policy,
    /// Under [`Policy::Refuse`]: commands answered, by kind (latest-wins ones included).
    pub refused: BTreeMap<u16, u64>,
    /// Under [`Policy::Refuse`]: module commands the reference codecs could not decode (skipped,
    /// unanswered).
    pub undecoded: u64,
}

/// What a "world save" keeps of the Java side: the journal SavedData.
#[derive(Clone)]
pub struct WorldSave {
    pub journal: VecDeque<(u64, Vec<u8>)>,
    pub pinned_gen: Option<(u64, Vec<u8>)>,
    pub next_obs: u64,
    pub last_applied_cmd: u64,
    pub dict: Vec<(u32, String)>,
}

impl WorldSave {
    pub fn fresh() -> Self {
        WorldSave {
            journal: VecDeque::new(),
            pinned_gen: None,
            next_obs: 1,
            last_applied_cmd: 0,
            dict: Vec::new(),
        }
    }
}

impl Java {
    pub fn open(e: Arc<Engine>, save: WorldSave) -> Java {
        Java::try_open(e, save).expect("start")
    }

    /// [`Java::open`], returning the engine's start error instead of panicking (the host).
    pub fn try_open(e: Arc<Engine>, save: WorldSave) -> Result<Java, String> {
        Java::try_open_as(e, save, session_gen())
    }

    /// [`Java::try_open`] as the session `nonce` (never 0). The host, which carries no holders
    /// and so never journals a copy, names its session after the world's save so that two runs
    /// of one world write the same pages; every other session is random, as the bridge's are.
    pub fn try_open_as(e: Arc<Engine>, save: WorldSave, nonce: u64) -> Result<Java, String> {
        assert_ne!(nonce, 0, "generation 0 is a record from before generations");
        let (pages_obs, next_cmd_seq) = e.start(save.last_applied_cmd)?;
        if next_cmd_seq <= save.last_applied_cmd {
            return Err(format!(
                "the engine's nextCmdSeq {next_cmd_seq} is not above the journal's lastAppliedCmdSeq {}",
                save.last_applied_cmd
            ));
        }
        for i in 0..4 {
            let b = e.buffer(i).unwrap();
            assert_eq!(b.published().load(Ordering::Acquire), 0, "fresh buffers");
            assert_eq!(b.consumed().load(Ordering::Acquire), 0, "fresh buffers");
            assert_eq!(b.base() as usize % 64, 0, "64-aligned");
        }
        let journal_first = save.journal.front().map_or(0, |(s, _)| *s);
        let world_next = save.next_obs;
        let next_obs = save.next_obs.max(pages_obs + 1);
        let mut applied_cmd_ops = HashSet::new();
        for (_, r) in &save.journal {
            if let Some(op) = applied_op(r) {
                applied_cmd_ops.insert(op);
            }
        }
        let mut j = Java {
            e,
            journal: save.journal,
            pinned_gen: save.pinned_gen,
            next_obs,
            last_applied_cmd: save.last_applied_cmd,
            dict: save.dict,
            applied_cmd_ops,
            nonce,
            pages_obs,
            journal_first_at_start: journal_first,
            world_next_at_start: world_next,
            replay_up_to: next_obs - 1,
            submitted: 0,
            next_cmd: 1,
            cursor: 0,
            count: 0,
            open_flags: 0,
            open_highest: 0,
            sent_obs: 0,
            dict_sent: 0,
            session_start_pending: true,
            pending_ack: 0,
            tick_marked: i64::MIN,
            game_time: 1000,
            cmd_dict: HashMap::new(),
            containers: HashMap::new(),
            commands: Vec::new(),
            acks: Vec::new(),
            stale_commands: 0,
            hold_commands: false,
            held_ops: Vec::new(),
            hold_acks: false,
            failed_seen: false,
            module_mask: 0,
            p1: Vec::new(),
            talk_facts: Vec::new(),
            no_free_plot: false,
            fail_transfers: false,
            players: Vec::new(),
            mc: None,
            ind_cmd_seq: HashMap::new(),
            ind_commands: Vec::new(),
            held_ind: Vec::new(),
            fail_next_carriage: false,
            failed_carriage: None,
            iaf: Vec::new(),
            far: Vec::new(),
            ships: Vec::new(),
            herds: Vec::new(),
            herd_plugin: rtsim_ecology::herds::synth::FakeHerds::default(),
            conflict: Vec::new(),
            conflict_plugin: rtsim_conflict::synth::FakeConflict::new((0, 0)),
            known: Vec::new(),
            policy: Policy::Play,
            refused: BTreeMap::new(),
            undecoded: 0,
        };
        // The session's first record names its generation, as BoundarySession's constructor does.
        let g = j.nonce;
        j.journal(obs::JOURNAL_GEN, obs::journal_gen::SIZE, 0, |r| {
            wire::put_u64(r, obs::journal_gen::GEN, g)
        });
        Ok(j)
    }

    pub fn world_save(&self) -> WorldSave {
        WorldSave {
            journal: self.journal.clone(),
            pinned_gen: self.pinned_gen.clone(),
            next_obs: self.next_obs,
            last_applied_cmd: self.last_applied_cmd,
            dict: self.dict.clone(),
        }
    }

    pub fn obs_buf(&self, i: u64) -> &Buf {
        self.e.obs_buffer(i as usize)
    }

    pub fn cmd_buf(&self, i: u64) -> &Buf {
        self.e.cmd_buffer(i as usize)
    }

    pub fn open_index(&self) -> u64 {
        self.submitted + 1
    }

    /// Reserves a record in the open buffer (zero-filled); None when full.
    pub fn reserve(&mut self, kind: u16, size: usize, aux: u32) -> Option<&mut [u8]> {
        let b = self.e.obs_buffer(self.open_index() as usize);
        // SAFETY: the open buffer is Java's until it publishes it (the step that read it last has
        // stored consumed, checked at the previous submit).
        let area = unsafe { b.area_mut() };
        let size = wire::align8(size);
        if self.cursor + size > area.len() {
            return None;
        }
        let at = self.cursor;
        let mut w = RecordWriter::new(&mut area[at..]);
        w.reserve(kind, size, aux)?;
        self.cursor += size;
        self.count += 1;
        Some(&mut area[at..at + size])
    }

    pub fn copy(&mut self, rec: &[u8]) -> bool {
        match self.reserve(
            wire::u16_at(rec, wire::R_KIND),
            rec.len(),
            wire::u32_at(rec, wire::R_AUX),
        ) {
            Some(r) => {
                r.copy_from_slice(rec);
                true
            }
            None => false,
        }
    }

    pub fn dict_id(&mut self, kind: u32, name: &str) -> u32 {
        if let Some(i) = self.dict.iter().position(|(k, n)| *k == kind && n == name) {
            return i as u32 + 1;
        }
        self.dict.push((kind, name.to_string()));
        self.dict.len() as u32
    }

    /// Journals a record: `body` fills it from offset 24.
    pub fn journal(
        &mut self,
        kind: u16,
        size: usize,
        aux: u32,
        body: impl FnOnce(&mut [u8]),
    ) -> u64 {
        let len = wire::align8(size);
        let mut r = vec![0u8; len];
        wire::put_u16(&mut r, wire::R_KIND, kind);
        wire::put_u16(&mut r, wire::R_SIZE, len as u16);
        wire::put_u32(&mut r, wire::R_AUX, aux);
        wire::put_i64(&mut r, obs::GAME_TIME, self.game_time);
        body(&mut r);
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut r, obs::OBS_SEQ, seq);
        if kind == obs::COMMAND_RESULT && aux == obs::command_result::APPLIED {
            self.applied_cmd_ops
                .insert(wire::u64_at(&r, obs::command_result::OP_ID));
        }
        self.journal.push_back((seq, r));
        seq
    }

    /// Journals a copy of a record a holder carried (REASSERT, docs/boundary.md §3.2): the
    /// record's bytes as first journaled, after `nonce`, the generation they were journaled in.
    /// Returns the copy's sequence.
    pub fn reassert(&mut self, nonce: u64, original: &[u8]) -> u64 {
        let len = obs::reassert::INNER + original.len();
        let mut r = vec![0u8; len];
        wire::put_u16(&mut r, wire::R_KIND, obs::REASSERT);
        wire::put_u16(&mut r, wire::R_SIZE, len as u16);
        wire::put_i64(
            &mut r,
            obs::GAME_TIME,
            wire::i64_at(original, obs::GAME_TIME),
        );
        wire::put_u64(&mut r, obs::reassert::GEN, nonce);
        r[obs::reassert::INNER..].copy_from_slice(original);
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut r, obs::OBS_SEQ, seq);
        if let Some(op) = applied_op(&r) {
            self.applied_cmd_ops.insert(op);
        }
        self.journal.push_back((seq, r));
        seq
    }

    /// The journal's record at `seq`, as a holder would have kept it.
    pub fn journaled(&self, seq: u64) -> Option<Vec<u8>> {
        self.journal
            .iter()
            .find(|(s, _)| *s == seq)
            .map(|(_, r)| r.clone())
    }

    pub fn transfer(&mut self, op: u64, from: u64, to: u64, amount: i64) -> u64 {
        self.journal(obs::TRANSFER, obs::transfer::SIZE, 1, |r| {
            wire::put_u64(r, obs::transfer::OP_ID, op);
            wire::put_u64(r, obs::transfer::FROM, from);
            wire::put_u64(r, obs::transfer::TO, to);
            wire::put_i64(r, obs::transfer::AMOUNT, amount);
        })
    }

    pub fn preamble(&mut self) -> bool {
        if self.session_start_pending {
            let (gt, la, jf, wn, mask) = (
                self.game_time,
                self.last_applied_cmd,
                self.journal_first_at_start,
                self.world_next_at_start,
                self.module_mask,
            );
            let Some(r) = self.reserve(obs::SESSION_START, obs::session_start::SIZE, 0) else {
                return false;
            };
            wire::put_i64(r, obs::session_start::GAME_TIME, gt);
            wire::put_u64(r, obs::session_start::LAST_APPLIED_CMD_SEQ, la);
            wire::put_u64(r, obs::session_start::JOURNAL_FIRST_OBS_SEQ, jf);
            wire::put_u64(r, obs::session_start::WORLD_NEXT_OBS_SEQ, wn);
            wire::put_u32(r, obs::session_start::MODULE_MASK, mask);
            self.open_flags |= wire::OBS_FLAG_SESSION_START;
            self.session_start_pending = false;
        }
        while self.dict_sent < self.dict.len() {
            let (kind, name) = self.dict[self.dict_sent].clone();
            let id = self.dict_sent as u32 + 1;
            let Some(r) = self.reserve(obs::DICT, obs::dict::NAME + name.len(), kind) else {
                return false;
            };
            wire::put_u32(r, obs::dict::ID, id);
            wire::put_u32(r, obs::dict::NAME_LEN, name.len() as u32);
            r[obs::dict::NAME..obs::dict::NAME + name.len()].copy_from_slice(name.as_bytes());
            self.dict_sent += 1;
        }
        if self.tick_marked != self.game_time {
            let gt = self.game_time;
            let Some(r) = self.reserve(obs::TICK, obs::tick::SIZE, 0) else {
                return false;
            };
            wire::put_i64(r, obs::tick::GAME_TIME, gt);
            self.tick_marked = gt;
            for (uuid, dim, pos) in self.players.clone() {
                let Some(r) = self.reserve(obs::PLAYER, obs::player::SIZE, dim) else {
                    return false;
                };
                wire::put_u64(r, obs::player::UUID_MOST, uuid.0);
                wire::put_u64(r, obs::player::UUID_LEAST, uuid.1);
                wire::put_f64(r, obs::player::X, pos[0]);
                wire::put_f64(r, obs::player::Y, pos[1]);
                wire::put_f64(r, obs::player::Z, pos[2]);
            }
        }
        true
    }

    /// The server config's module switches changed (C25): a MODULES record in the open buffer,
    /// and the mask a later session start sends.
    pub fn modules(&mut self, mask: u32) {
        self.module_mask = mask;
        let r = self
            .reserve(obs::MODULES, obs::modules::SIZE, 0)
            .expect("room for MODULES");
        wire::put_u32(r, obs::modules::MASK, mask);
    }

    /// The end of a server tick: apply finished commands, hand over, submit if allowed.
    pub fn tick(&mut self) -> bool {
        self.apply_commands();
        let submitted = self.hand_over();
        self.game_time += 1;
        submitted
    }

    /// Fills the open buffer (the preamble, then the journal entries not yet sent, flagged REPLAY
    /// up to the session's start) and submits it if the protocol allows: returns whether it did.
    pub fn hand_over(&mut self) -> bool {
        let mut submitted = false;
        if self.preamble() {
            let entries: Vec<(u64, Vec<u8>)> = self
                .pinned_gen
                .iter()
                .chain(&self.journal)
                .filter(|(s, _)| *s > self.sent_obs)
                .cloned()
                .collect();
            for (s, r) in entries {
                if !self.copy(&r) {
                    break;
                }
                if self.sent_obs < self.replay_up_to {
                    self.open_flags |= wire::OBS_FLAG_REPLAY;
                }
                self.sent_obs = s;
                self.open_highest = s;
            }
        }
        if self.try_submit() {
            submitted = true;
        }
        submitted
    }

    /// Whether the last hand-over's step has published its commands (the headless host's
    /// lockstep waits for this, then applies them and hands over again).
    pub fn last_step_published(&self) -> bool {
        self.submitted == 0
            || self
                .cmd_buf(self.submitted)
                .published()
                .load(Ordering::Acquire)
                == self.submitted
    }

    pub fn try_submit(&mut self) -> bool {
        let h = self.submitted + 1;
        if self.submitted > 0
            && self
                .cmd_buf(self.submitted)
                .published()
                .load(Ordering::Acquire)
                != self.submitted
        {
            return false;
        }
        if self.next_cmd + 1 < h {
            return false;
        }
        let o = self.obs_buf(h);
        o.put_u32(wire::H_USED, self.cursor as u32);
        o.put_u32(wire::H_COUNT, self.count);
        o.put_u32(wire::H_FLAGS, self.open_flags);
        o.put_u64(wire::H_WORD_A, self.last_applied_cmd);
        o.put_i64(wire::H_WORD_B, self.game_time);
        o.put_u64(wire::H_WORD_C, self.open_highest);
        o.put_u64(wire::H_RESERVED, 0);
        o.published().store(h, Ordering::Release);
        self.submitted = h;
        self.e.submit(h).expect("submit");
        let next = self.obs_buf(h + 1);
        let consumed = next.consumed().load(Ordering::Acquire);
        assert_eq!(
            consumed,
            h - 1,
            "observation buffer {} reused before step {} consumed it",
            (h + 1) & 1,
            h - 1
        );
        self.cursor = 0;
        self.count = 0;
        self.open_flags = 0;
        self.open_highest = 0;
        true
    }

    pub fn apply_commands(&mut self) {
        while self.next_cmd <= self.submitted {
            let k = self.next_cmd;
            let e = Arc::clone(&self.e);
            let c = e.cmd_buffer(k as usize);
            if c.published().load(Ordering::Acquire) != k {
                return;
            }
            let used = c.get_u32(wire::H_USED) as usize;
            assert!(used <= c.capacity(), "command used beyond capacity");
            let ack = c.get_u64(wire::H_WORD_A);
            if c.get_u32(wire::H_FLAGS) & wire::CMD_FLAG_FAILED != 0 {
                self.failed_seen = true;
            }
            let highest = c.get_u64(wire::H_WORD_C);
            self.pending_ack = ack.min(self.sent_obs.max(self.pages_obs));
            // SAFETY: the step published k; it does not touch this buffer until Java stores
            // consumed = k (and submits k + 2).
            let area = unsafe { c.area(used) }.to_vec();
            let mut max_seq = 0;
            let mut recs = Records::new(&area);
            for rec in &mut recs {
                assert!(rec.kind >= 0x8000, "a command kind");
                let seq = wire::u64_at(rec.bytes, cmd::CMD_SEQ);
                if rec.kind == cmd::DICT {
                    assert_eq!(
                        seq, 0,
                        "a DICT carries no sequence, so it never moves lastAppliedCmdSeq"
                    );
                    let id = wire::u32_at(rec.bytes, cmd::dict::ID);
                    let len = wire::u32_at(rec.bytes, cmd::dict::NAME_LEN) as usize;
                    let name = String::from_utf8(
                        rec.bytes[cmd::dict::NAME..cmd::dict::NAME + len].to_vec(),
                    )
                    .unwrap();
                    self.cmd_dict.insert((rec.aux, id), name);
                    continue;
                }
                max_seq = max_seq.max(seq);
                if self.hold_commands {
                    if rec.kind == cmd::TRANSFER_TO_CONTAINER {
                        self.held_ops
                            .push(wire::u64_at(rec.bytes, cmd::transfer_to_container::OP_ID));
                    }
                    if (0x8400..=0x84FF).contains(&rec.kind)
                        && let Ok(Some((_, c))) = iw::decode_cmd(rec.bytes)
                    {
                        self.held_ind.push((seq, c));
                    }
                    continue;
                }
                if seq <= self.last_applied_cmd {
                    self.stale_commands += 1;
                    continue;
                }
                if self.policy == Policy::Refuse {
                    self.refuse(rec.kind, seq, rec.bytes);
                    self.last_applied_cmd = seq;
                    continue;
                }
                let op = match rec.kind {
                    cmd::TRANSFER_TO_CONTAINER => {
                        wire::u64_at(rec.bytes, cmd::transfer_to_container::OP_ID)
                    }
                    cmd::WRITE_GOSSIP => wire::u64_at(rec.bytes, cmd::write_gossip::OP_ID),
                    cmd::PLACE_HOUSE => wire::u64_at(rec.bytes, cmd::place_house::OP_ID),
                    cmd::SPAWN_RESIDENT => wire::u64_at(rec.bytes, cmd::spawn_resident::OP_ID),
                    _ => 0,
                };
                if (cmd::MARKET..=cmd::SPAWN_RESIDENT).contains(&rec.kind)
                    || rec.kind == cmd::SPAWN_GROUP
                    || rec.kind == cmd::DESPAWN_GROUP
                    || rec.kind == cmd::STEER_GROUP
                {
                    self.p1.push((rec.kind, rec.bytes.to_vec()));
                }
                if rec.kind == rtsim_jni::talk_wire::TALK_FACTS {
                    self.talk_facts.push(rec.bytes.to_vec());
                }
                self.commands.push((k, rec.kind, op));
                match rec.kind {
                    cmd::TRANSFER_TO_CONTAINER => {
                        let b = rec.bytes;
                        for (kind, off) in [
                            (wire::DICT_DIMENSION, cmd::transfer_to_container::DIM),
                            (wire::DICT_ITEM, cmd::transfer_to_container::ITEM),
                        ] {
                            assert!(
                                self.cmd_dict.contains_key(&(kind, wire::u32_at(b, off))),
                                "a DICT comes before the record that uses it"
                            );
                        }
                        let status = if self.applied_cmd_ops.contains(&op) {
                            obs::command_result::DUPLICATE_OP
                        } else if self.fail_transfers {
                            obs::command_result::FAILED
                        } else {
                            let pos = [
                                wire::i32_at(b, cmd::transfer_to_container::X),
                                wire::i32_at(b, cmd::transfer_to_container::Y),
                                wire::i32_at(b, cmd::transfer_to_container::Z),
                            ];
                            *self.containers.entry(pos).or_insert(0) +=
                                wire::i32_at(b, cmd::transfer_to_container::COUNT);
                            obs::command_result::APPLIED
                        };
                        self.journal(
                            obs::COMMAND_RESULT,
                            obs::command_result::SIZE,
                            status,
                            |r| {
                                wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                                wire::put_u64(r, obs::command_result::OP_ID, op);
                            },
                        );
                    }
                    cmd::SPAWN_GROUP => {
                        let b = rec.bytes;
                        assert!(self.cmd_dict.contains_key(&(
                            wire::DICT_BODY,
                            wire::u32_at(b, cmd::spawn_group::GROUP_KIND)
                        )));
                        let agent = wire::u64_at(b, cmd::spawn_group::AGENT);
                        let epoch = wire::u32_at(b, cmd::spawn_group::EPOCH);
                        let dim = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
                        self.journal(obs::MATERIALIZED, obs::materialized::SIZE, rec.aux, |r| {
                            wire::put_u64(r, obs::materialized::AGENT, agent);
                            wire::put_u32(r, obs::materialized::EPOCH, epoch);
                            wire::put_u32(r, obs::materialized::DIM, dim);
                        });
                    }
                    k if (0x8400..=0x84FF).contains(&k) => self.industry_command(k, seq, rec.bytes),
                    k if (0x8500..=0x85FF).contains(&k) => self.dragon_command(seq, rec.bytes),
                    k if (0x8600..=0x86FF).contains(&k) => self.ship_command(seq, rec.bytes),
                    k if (0x8700..=0x87FF).contains(&k) => self.herd_command(seq, rec.bytes),
                    k if (0x8800..=0x88FF).contains(&k) => self.conflict_command(seq, rec.bytes),
                    k if (0x8900..=0x89FF).contains(&k) => self.habitat_command(seq, rec.bytes),
                    k if (0x8C00..=0x8CFF).contains(&k) => {
                        let r = rtsim_ecology::far::decode(rec.bytes).expect("a FAR_LEG decodes");
                        assert_eq!(r.seq, seq);
                        self.far.push(r);
                    }
                    cmd::DESPAWN_GROUP => {
                        let b = rec.bytes;
                        let agent = wire::u64_at(b, cmd::despawn_group::AGENT);
                        let epoch = wire::u32_at(b, cmd::despawn_group::EPOCH);
                        self.journal(
                            obs::DEMATERIALIZED,
                            obs::dematerialized::SIZE,
                            obs::dematerialized::COMMAND,
                            |r| {
                                wire::put_u64(r, obs::dematerialized::AGENT, agent);
                                wire::put_u32(r, obs::dematerialized::EPOCH, epoch);
                            },
                        );
                    }
                    cmd::WRITE_GOSSIP | cmd::PLACE_HOUSE | cmd::SPAWN_RESIDENT => {
                        let status = if self.applied_cmd_ops.contains(&op) {
                            obs::command_result::DUPLICATE_OP
                        } else if rec.kind == cmd::PLACE_HOUSE && self.no_free_plot {
                            obs::command_result::NOT_FREE
                        } else {
                            if rec.kind == cmd::PLACE_HOUSE {
                                let village = wire::u64_at(rec.bytes, cmd::place_house::VILLAGE);
                                self.journal(obs::HOUSE_PLACED, obs::house_placed::SIZE, 0, |r| {
                                    wire::put_u64(r, obs::house_placed::OP_ID, op);
                                    wire::put_u64(r, obs::house_placed::VILLAGE, village);
                                });
                            }
                            obs::command_result::APPLIED
                        };
                        self.journal(
                            obs::COMMAND_RESULT,
                            obs::command_result::SIZE,
                            status,
                            |r| {
                                wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                                wire::put_u64(r, obs::command_result::OP_ID, op);
                            },
                        );
                    }
                    _ => {}
                }
                self.last_applied_cmd = seq;
            }
            assert!(recs.error().is_none(), "{:?}", recs.error());
            assert_eq!(
                max_seq, highest,
                "wordC is the highest cmdSeq in the buffer"
            );
            c.consumed().store(k, Ordering::Release);
            if self.pending_ack > 0 && !self.hold_acks {
                let a = self.pending_ack;
                while self.journal.front().is_some_and(|(s, _)| *s <= a) {
                    let e = self.journal.pop_front().unwrap();
                    if wire::u16_at(&e.1, wire::R_KIND) == obs::JOURNAL_GEN {
                        self.pinned_gen = Some(e);
                    }
                }
                if self.policy == Policy::Play {
                    self.acks.push(a);
                }
                self.pending_ack = 0;
            }
            self.next_cmd += 1;
        }
    }

    /// One command answered as Java answers it when it cannot act (VIEWER.md §12.1,
    /// docs/boundary.md §2.3): nothing is loaded and nobody is online, so
    /// - every spawn is refused as not ticking ([`NOT_TICKING`]): SPAWN_GROUP, and the modules'
    ///   materializations and adoptions, with their own SPAWN_REJECTED kinds;
    /// - every despawn, departure or recall answers not embodied (reason 6) with nothing captured,
    ///   as the bridge and its plugins answer one for a body they do not have;
    /// - every command that carries an op id is answered COMMAND_RESULT failed (3): transfers,
    ///   gossip, houses, residents, industry commands, ship cargo, and a dragon discard (op 0);
    /// - latest-wins commands (markets, offers, sentiment, steering, timetables, territories,
    ///   notices, habitat) need no answer, nor do the dragons' lair placements and oil surveys,
    ///   which wait for chunks to load.
    pub fn refuse(&mut self, kind: u16, seq: u64, b: &[u8]) {
        use rtsim_conflict::boundary as cb;
        use rtsim_ecology::boundary as db;
        use rtsim_ecology::herds::boundary as hb;
        use rtsim_ships::boundary as sb;
        *self.refused.entry(kind).or_default() += 1;
        let failed = |j: &mut Java, op: u64| {
            j.journal(
                obs::COMMAND_RESULT,
                obs::command_result::SIZE,
                obs::command_result::FAILED,
                |r| {
                    wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                    wire::put_u64(r, obs::command_result::OP_ID, op);
                },
            );
        };
        match kind {
            cmd::TRANSFER_TO_CONTAINER => {
                failed(self, wire::u64_at(b, cmd::transfer_to_container::OP_ID))
            }
            cmd::WRITE_GOSSIP => failed(self, wire::u64_at(b, cmd::write_gossip::OP_ID)),
            cmd::PLACE_HOUSE => failed(self, wire::u64_at(b, cmd::place_house::OP_ID)),
            cmd::SPAWN_RESIDENT => failed(self, wire::u64_at(b, cmd::spawn_resident::OP_ID)),
            cmd::SPAWN_GROUP => {
                let agent = wire::u64_at(b, cmd::spawn_group::AGENT);
                let epoch = wire::u32_at(b, cmd::spawn_group::EPOCH);
                self.journal(
                    obs::SPAWN_REJECTED,
                    obs::spawn_rejected::SIZE,
                    NOT_TICKING,
                    |r| {
                        wire::put_u64(r, obs::spawn_rejected::AGENT, agent);
                        wire::put_u32(r, obs::spawn_rejected::EPOCH, epoch);
                        wire::put_u64(r, obs::spawn_rejected::CMD_SEQ, seq);
                    },
                );
            }
            cmd::DESPAWN_GROUP => {
                let agent = wire::u64_at(b, cmd::despawn_group::AGENT);
                let epoch = wire::u32_at(b, cmd::despawn_group::EPOCH);
                self.journal(
                    obs::DEMATERIALIZED,
                    obs::dematerialized::SIZE,
                    obs::dematerialized::NOT_EMBODIED,
                    |r| {
                        wire::put_u64(r, obs::dematerialized::AGENT, agent);
                        wire::put_u32(r, obs::dematerialized::EPOCH, epoch);
                    },
                );
            }
            k if (0x8400..=0x84FF).contains(&k) => match iw::decode_cmd(b) {
                Ok(Some((_, c))) => {
                    if let Some(op) = c.op() {
                        failed(self, op.0);
                    }
                }
                _ => self.undecoded += 1,
            },
            k if (0x8500..=0x85FF).contains(&k) => {
                let dict = self.engine_species();
                match db::decode_cmd(b, &dict) {
                    Ok((_, _, c)) => match c {
                        db::Cmd::Materialize(db::Materialize { agent, epoch, .. })
                        | db::Cmd::Adopt(db::Adopt { agent, epoch, .. }) => {
                            self.dragon_obs(&db::Obs::SpawnRejected(db::SpawnRejected {
                                agent,
                                epoch,
                                reason: NOT_TICKING,
                                cmd_seq: seq,
                            }));
                        }
                        db::Cmd::Depart(d) => {
                            self.dragon_obs(&db::Obs::Dematerialized(db::Dematerialized {
                                agent: d.agent,
                                epoch: d.epoch,
                                reason: obs::dematerialized::NOT_EMBODIED,
                                dimension: 0,
                                health: 0.0,
                                max_health: 0.0,
                                hunger: 0,
                                age_ticks: 0,
                                pos: (0.0, 0.0, 0.0),
                            }));
                        }
                        db::Cmd::Discard(_) => failed(self, 0),
                        // They wait for chunks to load (OILGATE.md §4.1, §4.6), which here never
                        // happens: no lair, no OIL_CHUNKS, no nest's chamber.
                        db::Cmd::PlaceLair(_) | db::Cmd::OilSurvey(_) | db::Cmd::PlaceNest(_) => {}
                        // A nest's worms and sorties need its chamber, which never formed.
                        db::Cmd::NestRenew(_) | db::Cmd::NestSortie(_) => {}
                        db::Cmd::Steer(_) => {}
                    },
                    Err(_) => self.undecoded += 1,
                }
            }
            k if (0x8600..=0x86FF).contains(&k) => match sb::decode_cmd(b) {
                Ok((_, _, c)) => match c {
                    sb::Cmd::Materialize(m) => {
                        self.ship_obs(&sb::Obs::SpawnRejected(sb::SpawnRejected {
                            agent: m.agent,
                            epoch: m.epoch,
                            reason: NOT_TICKING,
                            cmd_seq: seq,
                        }));
                    }
                    sb::Cmd::Depart(sb::Depart { agent, epoch, .. })
                    | sb::Cmd::Despawn(sb::Despawn { agent, epoch, .. }) => {
                        self.ship_obs(&sb::Obs::Dematerialized(sb::Dematerialized {
                            agent,
                            epoch,
                            reason: obs::dematerialized::NOT_EMBODIED,
                            dimension: 0,
                            hull_milli: 0,
                            crew: 0,
                            pos: (0.0, 0.0, 0.0),
                        }));
                    }
                    sb::Cmd::SetCargo(sc) => failed(self, sc.op),
                    sb::Cmd::Timetable(_) | sb::Cmd::Steer(_) => {}
                },
                Err(_) => self.undecoded += 1,
            },
            k if (0x8700..=0x87FF).contains(&k) => {
                let dict = self.engine_herd_species();
                match hb::decode_cmd(b, &dict) {
                    Ok((_, _, c)) => match c {
                        hb::Cmd::Materialize(m) => {
                            self.herd_obs(&hb::Obs::SpawnRejected(hb::SpawnRejected {
                                agent: m.agent,
                                epoch: m.epoch,
                                reason: NOT_TICKING,
                                cmd_seq: seq,
                            }));
                        }
                        hb::Cmd::Despawn(d) => {
                            self.herd_obs(&hb::Obs::Dematerialized(hb::Dematerialized {
                                agent: d.agent,
                                epoch: d.epoch,
                                reason: obs::dematerialized::NOT_EMBODIED,
                                members: Vec::new(),
                            }));
                        }
                        hb::Cmd::Steer(_) => {}
                    },
                    Err(_) => self.undecoded += 1,
                }
            }
            k if (0x8800..=0x88FF).contains(&k) => match cb::decode_cmd(b) {
                Ok((_, c, _)) => match c {
                    cb::Cmd::Envoy(e) => {
                        self.conflict_obs(&cb::Obs::SpawnRejected(cb::BandSpawnRejected {
                            agent: e.agent,
                            epoch: e.epoch,
                            wave: 0,
                            reason: NOT_TICKING,
                            cmd_seq: seq,
                        }));
                    }
                    cb::Cmd::Materialize(m) => {
                        self.conflict_obs(&cb::Obs::SpawnRejected(cb::BandSpawnRejected {
                            agent: m.agent,
                            epoch: m.epoch,
                            wave: u32::from(m.wave),
                            reason: NOT_TICKING,
                            cmd_seq: seq,
                        }));
                    }
                    cb::Cmd::Recall(r) => {
                        self.conflict_obs(&cb::Obs::Dematerialized(cb::BandDematerialized {
                            agent: r.agent,
                            epoch: r.epoch,
                            reason: obs::dematerialized::NOT_EMBODIED,
                            survivors: Vec::new(),
                        }));
                    }
                    cb::Cmd::Territory(_)
                    | cb::Cmd::Notice(_)
                    | cb::Cmd::Steer(_)
                    | cb::Cmd::Settled(_)
                    | cb::Cmd::AmbushSpot(_)
                    | cb::Cmd::Reconcile(_)
                    | cb::Cmd::TrainRobbed(_)
                    | cb::Cmd::ChiefBody(_)
                    | cb::Cmd::March(_) => {}
                },
                Err(_) => self.undecoded += 1,
            },
            // MARKET, MERCHANT_OFFERS, SENTIMENT, STEER_GROUP and HABITAT_KNOWN (0x89xx): latest
            // wins, nothing to answer.
            _ => {}
        }
    }

    /// The dragons' species as the engine's dictionary numbers them (its DICT commands so far).
    fn engine_species(&self) -> rtsim_ecology::boundary::SpeciesDict {
        use rtsim_ecology::species::Species;
        let mut dict = rtsim_ecology::boundary::SpeciesDict {
            body: [0; 3],
            structure: [[0; 2]; 3],
        };
        for (i, s) in Species::ALL.iter().enumerate() {
            for (&(kind, id), name) in &self.cmd_dict {
                if kind == wire::DICT_BODY && name == s.entity_id() {
                    dict.body[i] = id;
                }
                for (j, k) in [
                    rtsim_ecology::dragon::LairKind::Roost,
                    rtsim_ecology::dragon::LairKind::Cave,
                ]
                .iter()
                .enumerate()
                {
                    if kind == wire::DICT_STRUCTURE
                        && *name == rtsim_ecology::boundary::SpeciesDict::structure_name(*s, *k)
                    {
                        dict.structure[i][j] = id;
                    }
                }
            }
        }
        dict
    }

    /// The herd species as the engine's dictionary numbers them.
    fn engine_herd_species(&self) -> rtsim_ecology::herds::boundary::HerdDict {
        use rtsim_ecology::herds::HerdSpecies;
        let mut dict = rtsim_ecology::herds::boundary::HerdDict { body: [0; 5] };
        for s in HerdSpecies::ALL {
            for (&(kind, id), name) in &self.cmd_dict {
                if kind == wire::DICT_BODY && name == s.entity_id() {
                    dict.body[s.index()] = id;
                }
            }
        }
        dict
    }

    /// Ticks until `done`, with a short sleep between ticks (a server tick is 50 ms; this is faster).
    pub fn run_until(&mut self, what: &str, mut done: impl FnMut(&mut Java) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.tick();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    /// Hands over everything open, waits for that step without submitting another, and applies
    /// its commands.
    pub fn flush(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.tick() {
            assert!(Instant::now() < deadline, "no submit possible");
            std::thread::sleep(Duration::from_micros(100));
        }
        while self.e.stats_array()[1] < self.submitted {
            assert!(
                Instant::now() < deadline,
                "step {} never finished",
                self.submitted
            );
            std::thread::sleep(Duration::from_micros(100));
        }
        self.apply_commands();
        assert_eq!(
            self.next_cmd,
            self.submitted + 1,
            "every finished step's commands applied"
        );
    }

    /// Two flushes: what Java journaled reaches the engine, and what the engine answered comes back.
    pub fn idle(&mut self) {
        self.flush();
        self.flush();
    }

    // ---- the industry module ---------------------------------------------------------------

    /// Java's dictionary id of a name (the harness numbers every kind in one sequence).
    pub fn java_id(&self, kind: u32, name: &str) -> Option<u32> {
        self.dict
            .iter()
            .position(|(k, n)| *k == kind && n == name)
            .map(|i| i as u32 + 1)
    }

    fn java_name(&self, id: u32) -> Option<&(u32, String)> {
        self.dict.get((id as usize).wrapping_sub(1))
    }

    /// The engine's GOOD, NAME and BLOCK_STATE ids are Java's own (F2): each must have come in a
    /// DICT command, with Java's name, before the command that uses it.
    fn check_mirrored(&self, kind: u32, id: u32) {
        let engine = self.cmd_dict.get(&(kind, id));
        assert!(
            engine.is_some(),
            "a DICT ({kind:#x}, {id}) comes before its first use"
        );
        let java = self.java_name(id);
        assert_eq!(
            java.map(|(k, n)| (*k, n.as_str())),
            Some((kind, engine.unwrap().as_str())),
            "the engine's entry ({kind:#x}, {id}) mirrors Java's"
        );
    }

    /// The engine's dimension id, as Java's: by name (Java's `engineLevel`).
    fn java_dim(&self, engine_dim: u32) -> u32 {
        let name = self
            .cmd_dict
            .get(&(wire::DICT_DIMENSION, engine_dim))
            .expect("a dimension's DICT comes before its first use");
        self.java_id(wire::DICT_DIMENSION, name)
            .expect("a dimension Java named")
    }

    /// One industry command: decoded with the reference codec, its dictionary ids checked and
    /// translated, handed to the world, which applies it at its next tick.
    fn industry_command(&mut self, kind: u16, seq: u64, bytes: &[u8]) {
        let (s, mut c) = iw::decode_cmd(bytes)
            .expect("an industry command decodes")
            .unwrap_or_else(|| panic!("industry kind {kind:#06x} is one the codec reads"));
        assert_eq!(s, seq);
        if let Some(op) = c.op() {
            assert_eq!(
                op.0 >> 56,
                0x49,
                "an industry op id carries namespace 0x49: {:#x}",
                op.0
            );
            self.ind_cmd_seq.insert(op.0, seq);
        }
        match &mut c {
            Cmd::ChunkReconcile(r) => {
                r.chunk.dim = self.java_dim(r.chunk.dim);
                r.deltas
                    .iter()
                    .for_each(|d| self.check_mirrored(DICT_GOOD, d.good.0));
                r.destroy
                    .iter()
                    .for_each(|d| self.check_mirrored(DICT_BLOCK_STATE, d.expected_state));
            }
            Cmd::DepositDelta(d) => {
                d.chunk.dim = self.java_dim(d.chunk.dim);
                self.check_mirrored(DICT_GOOD, d.good.0);
            }
            Cmd::CarriageDelta(d) => d
                .entries
                .iter()
                .for_each(|e| self.check_mirrored(DICT_GOOD, e.1.0)),
            Cmd::WritePackage(p) => p
                .goods
                .goods()
                .for_each(|g| self.check_mirrored(DICT_GOOD, g.0)),
            Cmd::WriteChain(p) => p
                .goods
                .goods()
                .for_each(|g| self.check_mirrored(DICT_GOOD, g.0)),
            Cmd::ProbeSet(_) | Cmd::CountSet(_) | Cmd::HeldDraw(_) => {}
        }
        if self.fail_next_carriage
            && let Cmd::CarriageDelta(cd) = &c
        {
            self.fail_next_carriage = false;
            let through = self.next_obs - 1;
            self.e.request_save(through);
            self.ind_commands.push((self.next_cmd, seq, c.clone()));
            self.failed_carriage = Some((seq, cd.op.0, through));
            self.industry_result(cd.op.0, obs::command_result::FAILED);
            return;
        }
        self.ind_commands.push((self.next_cmd, seq, c.clone()));
        if let Some(mc) = &mut self.mc {
            mc.deliver(c);
        }
    }

    /// One industry observation from the world: journaled (a new sequence) or into the open
    /// buffer (latest-wins; dropped when full, as the bridge does).
    pub fn industry_obs(&mut self, t: &Timed) {
        let mut buf = Vec::new();
        if t.obs.is_journaled() {
            let seq = self.next_obs;
            self.next_obs += 1;
            iw::encode_obs(seq, t, &mut buf).expect("an observation encodes");
            self.journal.push_back((seq, buf));
        } else {
            iw::encode_obs(0, t, &mut buf).expect("an observation encodes");
            if self.preamble() {
                let _ = self.copy(&buf);
            }
        }
    }

    /// Java's own COMMAND_RESULT for an industry op id.
    fn industry_result(&mut self, op: u64, status: u32) {
        let seq = self.ind_cmd_seq.get(&op).copied().unwrap_or(0);
        self.journal(
            obs::COMMAND_RESULT,
            obs::command_result::SIZE,
            status,
            |r| {
                wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                wire::put_u64(r, obs::command_result::OP_ID, op);
            },
        );
    }

    /// One server tick of the industry world in lockstep with the engine: the world ticks
    /// (applying the commands delivered last tick, one tick late as in game), its observations
    /// are journaled or written, the hand-over is submitted, and the step's commands come back
    /// before the next tick.
    pub fn mc_tick(&mut self) {
        let mc = self.mc.as_mut().expect("an industry world");
        mc.tick();
        let t = mc.t;
        let out = std::mem::take(&mut mc.outgoing);
        let threats = std::mem::take(&mut mc.threats_out);
        self.game_time = t.0 as i64;
        for th in threats {
            self.e
                .order(vec![Order::IndustryThreat(th)])
                .expect("a threat order");
        }
        for o in out {
            match o {
                Outgoing::Obs(t) => self.industry_obs(&t),
                Outgoing::CommandResult { op, status } => self.industry_result(op.0, status),
            }
        }
        self.lockstep();
    }

    /// Hands over (it must submit: the last step finished and its commands were applied), waits
    /// for the step and applies its commands.
    pub fn lockstep(&mut self) {
        let gt = self.game_time;
        assert!(self.tick(), "a lockstep tick always submits");
        self.game_time = gt;
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.e.stats_array()[1] < self.submitted {
            assert!(
                Instant::now() < deadline,
                "step {} never finished",
                self.submitted
            );
            std::hint::spin_loop();
            std::thread::yield_now();
        }
        self.apply_commands();
        assert_eq!(self.next_cmd, self.submitted + 1);
    }

    /// Runs the industry world until its time reaches `t`.
    pub fn mc_run_until(&mut self, t: rtsim_kernel::clock::Tick) {
        while self.mc.as_ref().expect("an industry world").t < t {
            self.mc_tick();
        }
    }

    pub fn mc(&self) -> &FakeWorld {
        self.mc.as_ref().expect("an industry world")
    }

    pub fn mc_mut(&mut self) -> &mut FakeWorld {
        self.mc.as_mut().expect("an industry world")
    }

    pub fn received(&self) -> Vec<rtsim_jni::engine::Received> {
        self.e.drain_received()
    }

    pub fn crash(self) -> WorldSave {
        let save = self.world_save();
        self.e.close(false, Duration::from_secs(5)).expect("close");
        save
    }

    // ---- the Ice and Fire plugin (docs/boundary.md §6) -----------------------------------------

    /// The species and lair structures as Java's dictionary numbers them (defined on first use).
    pub fn java_species(&mut self) -> rtsim_ecology::boundary::SpeciesDict {
        use rtsim_ecology::boundary::SpeciesDict;
        use rtsim_ecology::dragon::LairKind;
        use rtsim_ecology::species::Species;
        let mut d = SpeciesDict {
            body: [0; 3],
            structure: [[0; 2]; 3],
        };
        for (i, s) in Species::ALL.iter().enumerate() {
            d.body[i] = self.dict_id(wire::DICT_BODY, s.entity_id());
            for (k, kind) in [LairKind::Roost, LairKind::Cave].iter().enumerate() {
                d.structure[i][k] = self.dict_id(
                    wire::DICT_STRUCTURE,
                    &SpeciesDict::structure_name(*s, *kind),
                );
            }
        }
        d
    }

    /// A dragon observation, encoded with the reference codec in Java's dictionary: journaled
    /// (a new sequence) for kinds 0x0500 to 0x05FF, otherwise into the open buffer (latest-wins,
    /// 0x0060 to 0x006F). Returns the sequence (0 for latest-wins).
    pub fn dragon_obs(&mut self, o: &rtsim_ecology::boundary::Obs) -> u64 {
        let dict = self.java_species();
        let mut b = Vec::new();
        rtsim_ecology::boundary::encode_obs(0, self.game_time, o, &dict, &mut b);
        let kind = wire::u16_at(&b, wire::R_KIND);
        if !obs::is_journaled(kind) {
            if self.preamble() {
                assert!(self.copy(&b), "room for a latest-wins dragon record");
            }
            return 0;
        }
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut b, obs::OBS_SEQ, seq);
        self.journal.push_back((seq, b));
        seq
    }

    /// One dragon command, as the plugin applies it: a materialization or an adoption answers
    /// DRAGON_MATERIALIZED (the body is there), a discard COMMAND_RESULT; steering and departures
    /// are recorded (the test captures a departing dragon itself, with DRAGON_DEMATERIALIZED).
    fn dragon_command(&mut self, seq: u64, bytes: &[u8]) {
        use rtsim_ecology::boundary::{self as db, Cmd as D};
        let dict = self.engine_species();
        let (_, s, mut c) = db::decode_cmd(bytes, &dict).expect("a dragon command decodes");
        assert_eq!(s, seq);
        let overworld = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        match &mut c {
            D::Materialize(m) => {
                m.dimension = self.java_dim(m.dimension);
                let (agent, epoch, dimension) = (m.agent, m.epoch, m.dimension);
                self.dragon_obs(&db::Obs::Materialized(db::Materialized {
                    agent,
                    epoch,
                    dimension,
                    entity: (0xD0D0, agent),
                }));
            }
            D::Adopt(a) => {
                let (agent, epoch, entity) = (a.agent, a.epoch, a.entity);
                self.dragon_obs(&db::Obs::Materialized(db::Materialized {
                    agent,
                    epoch,
                    dimension: overworld,
                    entity,
                }));
            }
            D::Discard(_) => {
                self.journal(
                    obs::COMMAND_RESULT,
                    obs::command_result::SIZE,
                    obs::command_result::APPLIED,
                    |r| {
                        wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                    },
                );
            }
            // The oil gate's asks (a lair placed, a survey): recorded, in Java's dimension ids.
            D::PlaceLair(p) => p.dimension = self.java_dim(p.dimension),
            D::OilSurvey(o) => o.dimension = self.java_dim(o.dimension),
            // The oil nests' asks (OILGATE.md §4.6): recorded; the test answers them itself.
            D::PlaceNest(p) => p.dimension = self.java_dim(p.dimension),
            D::NestRenew(r) => r.dimension = self.java_dim(r.dimension),
            D::NestSortie(o) => o.dimension = self.java_dim(o.dimension),
            D::Steer(_) | D::Depart(_) => {}
        }
        self.iaf.push((seq, c));
    }

    // ---- a ships plugin (docs/boundary.md §7; none exists yet, P8) ------------------------------

    /// Journals a ship observation (a kind of 0x0600 to 0x06FF), encoded with the reference codec;
    /// its ids are Java's dictionary's. Also the record of what a real plugin would send.
    pub fn ship_obs(&mut self, o: &rtsim_ships::boundary::Obs) -> u64 {
        let mut b = Vec::new();
        rtsim_ships::boundary::encode_obs(0, self.game_time, o, &mut b);
        let kind = wire::u16_at(&b, wire::R_KIND);
        assert!(obs::is_journaled(kind), "a journaled ship kind");
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut b, obs::OBS_SEQ, seq);
        self.journal.push_back((seq, b));
        seq
    }

    /// The Java id of a name the engine sent in its dictionary (defined in Java's on first use,
    /// as a plugin maps an engine name to its registry and the registry to Java's own id).
    fn java_of_engine(&mut self, kind: u32, engine_id: u32) -> u32 {
        let name = self
            .cmd_dict
            .get(&(kind, engine_id))
            .unwrap_or_else(|| panic!("a DICT ({kind}, {engine_id}) comes before its first use"))
            .clone();
        self.dict_id(kind, &name)
    }

    /// One ship command, as a plugin applies it: a materialization answers SHIP_MATERIALIZED (the
    /// ship and its crew are there), a despawn SHIP_DEMATERIALIZED; the rest are recorded (the
    /// test plays fights, loot and captures itself). Ids are translated to Java's.
    fn ship_command(&mut self, seq: u64, bytes: &[u8]) {
        use rtsim_ships::boundary::{self as sb, Cmd as S};
        let (_, s, mut c) = sb::decode_cmd(bytes).expect("a ship command decodes");
        assert_eq!(s, seq);
        match &mut c {
            S::Materialize(m) => {
                m.dimension = self.java_dim(m.dimension);
                m.body = self.java_of_engine(wire::DICT_BODY, m.body);
                for mb in &mut m.members {
                    mb.body = self.java_of_engine(wire::DICT_BODY, mb.body);
                }
                for it in &mut m.cargo {
                    it.item = self.java_of_engine(wire::DICT_ITEM, it.item);
                }
                let (agent, epoch, dimension) = (m.agent, m.epoch, m.dimension);
                self.ship_obs(&sb::Obs::Materialized(sb::Materialized {
                    agent,
                    epoch,
                    dimension,
                    entity: (0x5A5A, agent),
                }));
            }
            S::SetCargo(sc) => {
                assert_eq!(
                    sc.op >> 56,
                    0x53,
                    "an engine SHIP_CARGO op id (allocator 0x53)"
                );
                for it in &mut sc.items {
                    it.item = self.java_of_engine(wire::DICT_ITEM, it.item);
                }
                // The hold is set exactly, once per op id (docs/boundary.md §7.5).
                let op = sc.op;
                let status = if self.applied_cmd_ops.contains(&op) {
                    obs::command_result::DUPLICATE_OP
                } else {
                    obs::command_result::APPLIED
                };
                self.journal(
                    obs::COMMAND_RESULT,
                    obs::command_result::SIZE,
                    status,
                    |r| {
                        wire::put_u64(r, obs::command_result::CMD_SEQ, seq);
                        wire::put_u64(r, obs::command_result::OP_ID, op);
                    },
                );
            }
            S::Despawn(d) => {
                let (agent, epoch) = (d.agent, d.epoch);
                // The body as it was materialized (the test reports wounds itself).
                let (crew, hull) = self
                    .ships
                    .iter()
                    .rev()
                    .find_map(|(_, c)| match c {
                        S::Materialize(m) if m.agent == agent && m.epoch == epoch => {
                            Some((m.crew, m.hull_milli))
                        }
                        _ => None,
                    })
                    .unwrap_or((0, 0));
                self.ship_obs(&sb::Obs::Dematerialized(sb::Dematerialized {
                    agent,
                    epoch,
                    reason: 1,
                    dimension: 0,
                    hull_milli: hull,
                    crew,
                    pos: (0.0, 63.0, 0.0),
                }));
            }
            S::Timetable(_) | S::Steer(_) | S::Depart(_) => {}
        }
        self.ships.push((seq, c));
    }

    // ---- the habitat sampler (docs/boundary.md §10) ----------------------------------------------

    /// Writes a HABITAT record (latest wins) into the open buffer, as the core's sampler does: the
    /// record's biome and body ids are `names`' (a painter's fake dictionary), translated to
    /// Java's own here (DICT records go out first, in the preamble).
    pub fn habitat(
        &mut self,
        r: &rtsim_space::HabitatRecord,
        biome: &dyn Fn(u32) -> String,
        body: &dyn Fn(u32) -> String,
    ) -> bool {
        let mut r = r.clone();
        let map =
            |j: &mut Java, kind: u32, n: String| if n.is_empty() { 0 } else { j.dict_id(kind, &n) };
        r.biome = map(self, rtsim_space::habitat::DICT_BIOME, biome(r.biome));
        r.land_biome = map(self, rtsim_space::habitat::DICT_BIOME, biome(r.land_biome));
        for c in &mut r.creatures {
            c.0 = map(self, wire::DICT_BODY, body(c.0));
        }
        r.dimension = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        if !self.preamble() {
            return false;
        }
        let mut b = Vec::new();
        r.encode(&mut b);
        self.copy(&b)
    }

    /// HABITAT_KNOWN, as the core's sampler takes it: what the engine holds, by cell.
    fn habitat_command(&mut self, seq: u64, bytes: &[u8]) {
        let (_, k) = rtsim_space::KnownRecord::decode(bytes).expect("HABITAT_KNOWN decodes");
        let dim = (k.flags & rtsim_space::habitat::KNOWN_END == 0).then(|| {
            self.cmd_dict
                .get(&(wire::DICT_DIMENSION, k.dimension))
                .cloned()
                .expect("a DICT names the dimension before HABITAT_KNOWN uses it")
        });
        self.known.push((seq, k, dim));
    }

    // ---- the vanilla herd adapter (docs/boundary.md §8) ------------------------------------------

    /// Journals a herd observation (a kind of 0x0700 to 0x07FF), encoded with the reference codec.
    pub fn herd_obs(&mut self, o: &rtsim_ecology::herds::boundary::Obs) -> u64 {
        let mut b = Vec::new();
        rtsim_ecology::herds::boundary::encode_obs(0, self.game_time, o, &mut b);
        let kind = wire::u16_at(&b, wire::R_KIND);
        assert!(obs::is_journaled(kind), "a journaled herd kind");
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut b, obs::OBS_SEQ, seq);
        self.journal.push_back((seq, b));
        seq
    }

    /// Journals everything the fake herd plugin has to say (its answers, and the kills, shearing
    /// and leads a test played on it).
    pub fn herd_flush(&mut self) {
        for o in self.herd_plugin.take_obs() {
            self.herd_obs(&o);
        }
    }

    /// One herd command, as the vanilla adapter applies it: the species and the dimension are
    /// the engine's dictionary's (a DICT before first use), translated to Java's; the fake plugin
    /// answers HERD_MATERIALIZED and HERD_DEMATERIALIZED (with each member's capture).
    fn herd_command(&mut self, seq: u64, bytes: &[u8]) {
        use rtsim_ecology::herds::boundary::{self as hb, Cmd as H};
        let dict = self.engine_herd_species();
        let (_, s, mut c) = hb::decode_cmd(bytes, &dict).expect("a herd command decodes");
        assert_eq!(s, seq);
        if let H::Materialize(m) = &mut c {
            m.dimension = self.java_dim(m.dimension);
        }
        let at = rtsim_kernel::clock::Tick(self.game_time.max(0) as u64);
        self.herd_plugin.apply(vec![c.clone()], at);
        self.herds.push((seq, c));
        self.herd_flush();
    }

    // ---- a conflict plugin (docs/boundary.md §9; none exists yet, P8) ---------------------------

    /// A conflict observation, encoded with the reference codec in Java's ids: journaled (a new
    /// sequence, at the current game time) for kinds 0x0800 to 0x08FF, otherwise into the open
    /// buffer (latest-wins). Returns the sequence (0 for latest-wins).
    pub fn conflict_obs(&mut self, o: &rtsim_conflict::boundary::Obs) -> u64 {
        let mut b = Vec::new();
        rtsim_conflict::boundary::encode_obs(0, self.game_time, o, &mut b);
        let kind = wire::u16_at(&b, wire::R_KIND);
        if !obs::is_journaled(kind) {
            if self.preamble() {
                assert!(self.copy(&b), "room for a latest-wins conflict record");
            }
            return 0;
        }
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut b, obs::OBS_SEQ, seq);
        self.journal.push_back((seq, b));
        seq
    }

    /// Journals what the fake conflict plugin has to say by now (its answers, casualties and
    /// recalls, each some ticks after the command that caused it).
    pub fn conflict_flush(&mut self) {
        let now = rtsim_kernel::clock::Tick(self.game_time.max(0) as u64);
        for (_, o) in self.conflict_plugin.due(now) {
            self.conflict_obs(&o);
        }
    }

    /// One conflict command, as a plugin applies it: its dimension, body and item ids are the
    /// engine's dictionary's (a DICT before first use), translated to Java's; the fake plugin
    /// answers it (BAND_MATERIALIZED, a tribute or a refusal, casualties, BAND_DEMATERIALIZED).
    fn conflict_command(&mut self, seq: u64, bytes: &[u8]) {
        use rtsim_conflict::boundary::{self as cb, Cmd as C};
        let (s, mut c, _) = cb::decode_cmd(bytes).expect("a conflict command decodes");
        assert_eq!(s, seq);
        match &mut c {
            C::Territory(t) => t.dimension = self.java_dim(t.dimension),
            C::Notice(n) => n.dimension = self.java_dim(n.dimension),
            C::Envoy(e) => {
                e.dimension = self.java_dim(e.dimension);
                for m in &mut e.members {
                    m.0 = self.java_of_engine(wire::DICT_BODY, m.0);
                }
                if e.tribute_item != 0 {
                    e.tribute_item = self.java_of_engine(wire::DICT_ITEM, e.tribute_item);
                }
            }
            C::Materialize(m) => {
                m.dimension = self.java_dim(m.dimension);
                for st in &mut m.members {
                    st.body = self.java_of_engine(wire::DICT_BODY, st.body);
                }
            }
            C::AmbushSpot(a) => a.dimension = self.java_dim(a.dimension),
            C::Reconcile(r) => {
                for l in r.lines.iter_mut().filter(|l| l.body != 0) {
                    l.body = self.java_of_engine(wire::DICT_BODY, l.body);
                }
            }
            C::March(m) => {
                m.dimension = self.java_dim(m.dimension);
                if m.culture != 0 {
                    m.culture = self.java_of_engine(wire::DICT_CULTURE, m.culture);
                }
                for u in &mut m.units {
                    u.0 = self.java_of_engine(wire::DICT_BODY, u.0);
                }
            }
            C::Steer(_) | C::Recall(_) | C::Settled(_) | C::TrainRobbed(_) | C::ChiefBody(_) => {}
        }
        let at = rtsim_kernel::clock::Tick(self.game_time.max(0) as u64);
        self.conflict_plugin.apply(vec![c.clone()], at);
        self.conflict.push((seq, c));
    }
}

// ---- phase 1 observations the tests journal ------------------------------------------------------

/// The player the phase 1 helpers name.
pub const PLAYER: (u64, u64) = (0x1111_2222_3333_4444, 0x5555_6666_7777_8888);

impl Java {
    /// Journals a VILLAGE_ADOPTED for a village whose meeting point is (x, z).
    pub fn adopt(
        &mut self,
        village: u64,
        structure: &str,
        culture: &str,
        pieces: u32,
        x: i32,
        z: i32,
    ) {
        let dim = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let st = self.dict_id(wire::DICT_STRUCTURE, structure);
        let cu = self.dict_id(wire::DICT_CULTURE, culture);
        use obs::village_adopted as k;
        self.journal(obs::VILLAGE_ADOPTED, k::SIZE, 0, |r| {
            wire::put_u64(r, k::VILLAGE, village);
            wire::put_u32(r, k::DIM, dim);
            wire::put_u32(r, k::STRUCTURE, st);
            wire::put_u32(r, k::CULTURE, cu);
            wire::put_u32(r, k::PIECES, pieces);
            wire::put_i32(r, k::MIN_X, x - 20);
            wire::put_i32(r, k::MIN_Y, 60);
            wire::put_i32(r, k::MIN_Z, z - 20);
            wire::put_i32(r, k::MAX_X, x + 20);
            wire::put_i32(r, k::MAX_Y, 80);
            wire::put_i32(r, k::MAX_Z, z + 20);
            wire::put_i32(r, k::MEET_X, x);
            wire::put_i32(r, k::MEET_Y, 64);
            wire::put_i32(r, k::MEET_Z, z);
        });
    }

    /// The engine dictionary's name for an id the commands used.
    pub fn cmd_name(&self, kind: u32, id: u32) -> String {
        self.cmd_dict.get(&(kind, id)).cloned().unwrap_or_default()
    }

    /// The phase 1 commands of one kind this Java received (their records).
    pub fn p1_of(&self, kind: u16) -> Vec<Vec<u8>> {
        self.p1
            .iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, b)| b.clone())
            .collect()
    }

    /// A TRADE with a puppet merchant: the player paid `cost` and got `result`.
    #[allow(clippy::too_many_arguments)]
    pub fn trade_with(
        &mut self,
        agent: u64,
        epoch: u32,
        side: u32,
        cost: (&str, i32),
        result: (&str, i32),
        op: u64,
    ) -> u64 {
        let a = self.dict_id(wire::DICT_ITEM, cost.0);
        let r_ = self.dict_id(wire::DICT_ITEM, result.0);
        use obs::trade as k;
        self.journal(obs::TRADE, k::SIZE, side, |r| {
            wire::put_u64(r, k::OP_ID, op);
            wire::put_u64(r, k::AGENT, agent);
            wire::put_u32(r, k::EPOCH, epoch);
            wire::put_u64(r, k::PLAYER_MOST, PLAYER.0);
            wire::put_u64(r, k::PLAYER_LEAST, PLAYER.1);
            wire::put_u32(r, k::COST_A_ITEM, a);
            wire::put_i32(r, k::COST_A_COUNT, cost.1);
            wire::put_u32(r, k::RESULT_ITEM, r_);
            wire::put_i32(r, k::RESULT_COUNT, result.1);
        })
    }

    /// A CARGO_CHANGED: `items` taken out of member `member`'s cargo by the player.
    pub fn robbed(
        &mut self,
        agent: u64,
        epoch: u32,
        member: u32,
        items: &[(&str, i32)],
        op: u64,
    ) -> u64 {
        let ids: Vec<u32> = items
            .iter()
            .map(|(n, _)| self.dict_id(wire::DICT_ITEM, n))
            .collect();
        use obs::cargo_changed as k;
        self.journal(obs::CARGO_CHANGED, k::size(items.len()), member, |r| {
            wire::put_u64(r, k::OP_ID, op);
            wire::put_u64(r, k::AGENT, agent);
            wire::put_u32(r, k::EPOCH, epoch);
            wire::put_u32(r, k::FLAGS, k::FLAG_BY_PROXIMITY);
            wire::put_u64(r, k::PLAYER_MOST, PLAYER.0);
            wire::put_u64(r, k::PLAYER_LEAST, PLAYER.1);
            wire::put_u32(r, k::ITEM_COUNT, items.len() as u32);
            for (i, ((_, n), id)) in items.iter().zip(&ids).enumerate() {
                let it = k::ITEMS + i * obs::item::SIZE;
                wire::put_u32(r, it + obs::item::SLOT, u32::MAX);
                wire::put_u32(r, it + obs::item::ITEM, *id);
                wire::put_i32(r, it + obs::item::COUNT, -*n);
            }
        })
    }
}

// ---- TALK (docs/boundary.md §13) ----------------------------------------------------------------

impl Java {
    /// Journals a TALKED (what a ribbit's credited talk becomes): village `village`, the player,
    /// the ribbit's body id 0 (unknown), op id `op`. Returns its observation sequence.
    pub fn talked(&mut self, village: u64, player: (u64, u64), op: u64) -> u64 {
        let t = rtsim_jni::talk_wire::Talked {
            obs_seq: 0,
            game_time: self.game_time,
            op_id: op,
            village,
            player,
            body: 0,
        };
        self.journal_raw(&t.encode())
    }

    /// Journals a whole record the caller encoded (its sequence and game time are set here, like
    /// [`Java::journal`]'s): for the malformed records a test sends.
    pub fn journal_raw(&mut self, rec: &[u8]) -> u64 {
        let mut r = rec.to_vec();
        let seq = self.next_obs;
        self.next_obs += 1;
        wire::put_u64(&mut r, obs::OBS_SEQ, seq);
        wire::put_i64(&mut r, obs::GAME_TIME, self.game_time);
        self.journal.push_back((seq, r));
        seq
    }

    /// Asks the engine about `village` for `player` (one TALK_ASK in the open buffer, no news) and
    /// returns the answer's sentiment, or None when the engine did not answer.
    pub fn talk_sentiment(&mut self, id: u64, village: u64, player: (u64, u64)) -> Option<i32> {
        use rtsim_jni::talk_wire::{Ask, Facts, ask, encode_asks};
        let dimension = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let rec = encode_asks(&[Ask {
            id,
            player,
            holder_kind: ask::holder::VILLAGE,
            dimension,
            holder: village,
            x: 0,
            z: 0,
            flags: 0,
        }]);
        if self.preamble() {
            assert!(self.copy(&rec), "room for a TALK_ASK");
        }
        self.idle();
        self.talk_facts
            .iter()
            .filter_map(|b| Facts::decode(b).ok())
            .find(|(_, f)| f.ask_id == id)
            .map(|(_, f)| f.sentiment)
    }
}

/// A session's generation nonce: random, never 0.
fn session_gen() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(1);
    h.finish() | 1
}

/// A session's nonce named after the world's save (the host's, [`Java::try_open_as`]): the same
/// save gives the same nonce, and any record or session since gives another. Never 0.
pub fn nonce_of(save: &WorldSave) -> u64 {
    let mut x = save.next_obs ^ save.last_applied_cmd.rotate_left(32) ^ 0x5254_4a47_0000_0000;
    for (s, r) in &save.journal {
        x = (x ^ s ^ (r.len() as u64).rotate_left(17)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    x ^= x >> 31;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    (x ^ (x >> 29)) | 1
}

/// The engine op a journaled record says Java applied: a COMMAND_RESULT APPLIED's, a copy's too.
fn applied_op(r: &[u8]) -> Option<u64> {
    let r = if wire::u16_at(r, wire::R_KIND) == obs::REASSERT {
        r.get(obs::reassert::INNER..)?
    } else {
        r
    };
    (r.len() >= obs::command_result::SIZE
        && wire::u16_at(r, wire::R_KIND) == obs::COMMAND_RESULT
        && wire::u32_at(r, wire::R_AUX) == obs::command_result::APPLIED)
        .then(|| wire::u64_at(r, obs::command_result::OP_ID))
}
