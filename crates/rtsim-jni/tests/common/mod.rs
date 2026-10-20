// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fake Java side of rtsim-jni's tests, on the real host: mic_native is started in this
//! process, the engine's steps run on its background pool and its saves on its IO lane, exactly as
//! in game. [`Java`] is a Rust copy of the bridge's `Handover` and `BoundarySession` rules: it
//! writes observations into the open buffer, publishes with release, submits, polls the command
//! buffers with acquire, applies commands (skipping by sequence, then by op id) and trims its
//! journal on acknowledgements. With an industry world ([`Java::mc`], rtsim-industry's fake world
//! in remote mode) it also plays the industry module of docs/industry-boundary.md.
#![allow(dead_code)]
use std::collections::{HashMap, HashSet, VecDeque};
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

pub fn host() {
    static HOST: OnceLock<()> = OnceLock::new();
    HOST.get_or_init(|| {
        mic_native_host::start(mic_native_host::Config {
            fg_threads: 2,
            bg_threads: 2,
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

/// The Java side: one session of the bridge's boundary, as `BoundarySession` and `Handover` run it.
pub struct Java {
    pub e: Arc<Engine>,
    // the journal (saved with the "world")
    pub journal: VecDeque<(u64, Vec<u8>)>,
    pub next_obs: u64,
    pub last_applied_cmd: u64,
    pub dict: Vec<(u32, String)>,
    pub applied_cmd_ops: HashSet<u64>,
    // per session
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
    /// Ship commands applied, as a ships plugin would (docs/boundary.md §7.5): (cmdSeq, command
    /// decoded with the reference codec, its body, item and dimension ids in Java's dictionary).
    pub ships: Vec<(u64, rtsim_ships::boundary::Cmd)>,
    /// Herd commands applied, as the core's vanilla herd adapter would (docs/boundary.md §8.5):
    /// (cmdSeq, command decoded with the reference codec, its dimension in Java's dictionary).
    pub herds: Vec<(u64, rtsim_ecology::herds::boundary::Cmd)>,
    /// The vanilla herd adapter this Java plays: rtsim-ecology's fake plugin
    /// (`herds::synth::FakeHerds`), which answers the commands and plays kills, shearing, leads.
    pub herd_plugin: rtsim_ecology::herds::synth::FakeHerds,
    /// HABITAT_KNOWN commands applied (docs/boundary.md §10.5): (cmdSeq, record, its dimension
    /// as Java's dictionary names it, or None for END).
    pub known: Vec<(u64, rtsim_space::KnownRecord, Option<String>)>,
}

/// What a "world save" keeps of the Java side: the journal SavedData.
#[derive(Clone)]
pub struct WorldSave {
    pub journal: VecDeque<(u64, Vec<u8>)>,
    pub next_obs: u64,
    pub last_applied_cmd: u64,
    pub dict: Vec<(u32, String)>,
}

impl WorldSave {
    pub fn fresh() -> Self {
        WorldSave {
            journal: VecDeque::new(),
            next_obs: 1,
            last_applied_cmd: 0,
            dict: Vec::new(),
        }
    }
}

impl Java {
    pub fn open(e: Arc<Engine>, save: WorldSave) -> Java {
        let (pages_obs, next_cmd_seq) = e.start(save.last_applied_cmd).expect("start");
        assert!(
            next_cmd_seq > save.last_applied_cmd,
            "nextCmdSeq must be above lastAppliedCmdSeq"
        );
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
            if wire::u16_at(r, wire::R_KIND) == obs::COMMAND_RESULT
                && wire::u32_at(r, wire::R_AUX) == obs::command_result::APPLIED
            {
                applied_cmd_ops.insert(wire::u64_at(r, obs::command_result::OP_ID));
            }
        }
        Java {
            e,
            journal: save.journal,
            next_obs,
            last_applied_cmd: save.last_applied_cmd,
            dict: save.dict,
            applied_cmd_ops,
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
            players: Vec::new(),
            mc: None,
            ind_cmd_seq: HashMap::new(),
            ind_commands: Vec::new(),
            held_ind: Vec::new(),
            fail_next_carriage: false,
            failed_carriage: None,
            iaf: Vec::new(),
            ships: Vec::new(),
            herds: Vec::new(),
            herd_plugin: rtsim_ecology::herds::synth::FakeHerds::default(),
            known: Vec::new(),
        }
    }

    pub fn world_save(&self) -> WorldSave {
        WorldSave {
            journal: self.journal.clone(),
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
        let mut submitted = false;
        if self.preamble() {
            let entries: Vec<(u64, Vec<u8>)> = self
                .journal
                .iter()
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
        self.game_time += 1;
        submitted
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
                    k if (0x8900..=0x89FF).contains(&k) => self.habitat_command(seq, rec.bytes),
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
                    self.journal.pop_front();
                }
                self.acks.push(a);
                self.pending_ack = 0;
            }
            self.next_cmd += 1;
        }
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
            Cmd::ChannelDelta(d) => d
                .goods
                .goods()
                .for_each(|g| self.check_mirrored(DICT_GOOD, g.0)),
            Cmd::WritePackage(p) => p
                .goods
                .goods()
                .for_each(|g| self.check_mirrored(DICT_GOOD, g.0)),
            Cmd::Ledger(l) => l
                .goods
                .iter()
                .for_each(|g| self.check_mirrored(DICT_GOOD, g.0.0)),
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

    /// Journals a dragon observation (a kind of 0x0500 to 0x05FF), encoded with the reference
    /// codec in Java's dictionary.
    pub fn dragon_obs(&mut self, o: &rtsim_ecology::boundary::Obs) -> u64 {
        let dict = self.java_species();
        let mut b = Vec::new();
        rtsim_ecology::boundary::encode_obs(0, self.game_time, o, &dict, &mut b);
        let kind = wire::u16_at(&b, wire::R_KIND);
        assert!(obs::is_journaled(kind), "a journaled dragon kind");
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
        use rtsim_ecology::boundary::{self as db, Cmd as D, SpeciesDict};
        use rtsim_ecology::species::Species;
        let mut dict = SpeciesDict {
            body: [0; 3],
            structure: [[0; 2]; 3],
        };
        for (i, s) in Species::ALL.iter().enumerate() {
            for (&(kind, id), name) in &self.cmd_dict {
                if kind == wire::DICT_BODY && name == s.entity_id() {
                    dict.body[i] = id;
                }
            }
        }
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
        let map = |j: &mut Java, kind: u32, n: String| if n.is_empty() { 0 } else { j.dict_id(kind, &n) };
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
        use rtsim_ecology::herds::HerdSpecies;
        use rtsim_ecology::herds::boundary::{self as hb, Cmd as H, HerdDict};
        let mut dict = HerdDict { body: [0; 5] };
        for s in HerdSpecies::ALL {
            for (&(kind, id), name) in &self.cmd_dict {
                if kind == wire::DICT_BODY && name == s.entity_id() {
                    dict.body[s.index()] = id;
                }
            }
        }
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
}
