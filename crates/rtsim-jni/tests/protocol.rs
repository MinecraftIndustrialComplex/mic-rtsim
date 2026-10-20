// SPDX-License-Identifier: AGPL-3.0-or-later
//! The buffer protocol of docs/boundary.md against a fake Java side, on the real host: mic_native
//! is started in this process, the engine's steps run on its background pool and its saves on its
//! IO lane, exactly as in game. The fake Java side ([`Java`]) is a Rust copy of the bridge's
//! `Handover` and `BoundarySession` rules: it writes observations into the open buffer, publishes
//! with release, submits, polls the command buffers with acquire, applies commands (skipping by
//! sequence, then by op id) and trims its journal on acknowledgements.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rtsim_jni::buffers::Buf;
use rtsim_jni::engine::{Config, Engine, Exec};
use rtsim_jni::orders::{ItemOrder, MemberOrder, Order};
use rtsim_jni::wire::{self, RecordWriter, Records, cmd, obs};
use rtsim_kernel::page::Durability;

fn host() {
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

fn tmpdir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "rtsim-jni-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn engine(dir: &PathBuf, capacity: usize) -> Arc<Engine> {
    host();
    let mut cfg = Config::new(dir);
    cfg.capacity = capacity;
    cfg.test = true;
    cfg.durability = Durability::NoSync;
    cfg.exec = Exec::Host;
    Engine::open(cfg).expect("open")
}

const OVERWORLD: &str = "minecraft:overworld";
const DIAMOND: &str = "minecraft:diamond";
const CARAVAN: &str = "mic_rtsim:human_caravan";

/// The Java side: one session of the bridge's boundary, as `BoundarySession` and `Handover` run it.
struct Java {
    e: Arc<Engine>,
    // the journal (saved with the "world")
    journal: VecDeque<(u64, Vec<u8>)>,
    next_obs: u64,
    last_applied_cmd: u64,
    dict: Vec<(u32, String)>,
    applied_cmd_ops: HashSet<u64>,
    // per session
    pages_obs: u64,
    journal_first_at_start: u64,
    world_next_at_start: u64,
    replay_up_to: u64,
    submitted: u64,
    next_cmd: u64,
    cursor: usize,
    count: u32,
    open_flags: u32,
    open_highest: u64,
    sent_obs: u64,
    dict_sent: usize,
    session_start_pending: bool,
    pending_ack: u64,
    tick_marked: i64,
    game_time: i64,
    cmd_dict: HashMap<(u32, u32), String>,
    // what Minecraft did
    containers: HashMap<[i32; 3], i32>,
    commands: Vec<(u64, u16, u64)>,
    acks: Vec<u64>,
    stale_commands: u64,
    /// Read and hand back command buffers without applying anything (a world save that will be
    /// lost: whatever Minecraft did is rolled back with it).
    hold_commands: bool,
}

/// What a "world save" keeps of the Java side: the journal SavedData.
#[derive(Clone)]
struct WorldSave {
    journal: VecDeque<(u64, Vec<u8>)>,
    next_obs: u64,
    last_applied_cmd: u64,
    dict: Vec<(u32, String)>,
}

impl WorldSave {
    fn fresh() -> Self {
        WorldSave {
            journal: VecDeque::new(),
            next_obs: 1,
            last_applied_cmd: 0,
            dict: Vec::new(),
        }
    }
}

impl Java {
    fn open(e: Arc<Engine>, save: WorldSave) -> Java {
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
        }
    }

    fn world_save(&self) -> WorldSave {
        WorldSave {
            journal: self.journal.clone(),
            next_obs: self.next_obs,
            last_applied_cmd: self.last_applied_cmd,
            dict: self.dict.clone(),
        }
    }

    fn obs_buf(&self, i: u64) -> &Buf {
        self.e.obs_buffer(i as usize)
    }

    fn cmd_buf(&self, i: u64) -> &Buf {
        self.e.cmd_buffer(i as usize)
    }

    fn open_index(&self) -> u64 {
        self.submitted + 1
    }

    /// Reserves a record in the open buffer (zero-filled); None when full.
    fn reserve(&mut self, kind: u16, size: usize, aux: u32) -> Option<&mut [u8]> {
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

    fn copy(&mut self, rec: &[u8]) -> bool {
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

    fn dict_id(&mut self, kind: u32, name: &str) -> u32 {
        if let Some(i) = self.dict.iter().position(|(k, n)| *k == kind && n == name) {
            return i as u32 + 1;
        }
        self.dict.push((kind, name.to_string()));
        self.dict.len() as u32
    }

    /// Journals a record: `body` fills it from offset 24.
    fn journal(&mut self, kind: u16, size: usize, aux: u32, body: impl FnOnce(&mut [u8])) -> u64 {
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

    fn transfer(&mut self, op: u64, from: u64, to: u64, amount: i64) -> u64 {
        self.journal(obs::TRANSFER, obs::transfer::SIZE, 1, |r| {
            wire::put_u64(r, obs::transfer::OP_ID, op);
            wire::put_u64(r, obs::transfer::FROM, from);
            wire::put_u64(r, obs::transfer::TO, to);
            wire::put_i64(r, obs::transfer::AMOUNT, amount);
        })
    }

    fn preamble(&mut self) -> bool {
        if self.session_start_pending {
            let (gt, la, jf, wn) = (
                self.game_time,
                self.last_applied_cmd,
                self.journal_first_at_start,
                self.world_next_at_start,
            );
            let Some(r) = self.reserve(obs::SESSION_START, obs::session_start::SIZE, 0) else {
                return false;
            };
            wire::put_i64(r, obs::session_start::GAME_TIME, gt);
            wire::put_u64(r, obs::session_start::LAST_APPLIED_CMD_SEQ, la);
            wire::put_u64(r, obs::session_start::JOURNAL_FIRST_OBS_SEQ, jf);
            wire::put_u64(r, obs::session_start::WORLD_NEXT_OBS_SEQ, wn);
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
        }
        true
    }

    /// The end of a server tick: apply finished commands, hand over, submit if allowed.
    fn tick(&mut self) -> bool {
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

    fn try_submit(&mut self) -> bool {
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

    fn apply_commands(&mut self) {
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
                    continue;
                }
                if seq <= self.last_applied_cmd {
                    self.stale_commands += 1;
                    continue;
                }
                let op = if rec.kind == cmd::TRANSFER_TO_CONTAINER {
                    wire::u64_at(rec.bytes, cmd::transfer_to_container::OP_ID)
                } else {
                    0
                };
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
            if self.pending_ack > 0 {
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
    fn run_until(&mut self, what: &str, mut done: impl FnMut(&mut Java) -> bool) {
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
    fn flush(&mut self) {
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
    fn idle(&mut self) {
        self.flush();
        self.flush();
    }

    fn received(&self) -> Vec<rtsim_jni::engine::Received> {
        self.e.drain_received()
    }

    fn crash(self) -> WorldSave {
        let save = self.world_save();
        self.e.close(false, Duration::from_secs(5)).expect("close");
        save
    }
}

fn ledger(e: &Engine, store: u64) -> i64 {
    e.inspect().ledger.get(&store).copied().unwrap_or(0)
}

fn transfer_order(op: u64, count: i32) -> Order {
    Order::TransferToContainer {
        op,
        dimension: OVERWORLD.into(),
        item: DIAMOND.into(),
        pos: [5, 64, 5],
        count,
    }
}

#[test]
fn hand_overs_alternate_buffers_and_never_overlap() {
    let dir = tmpdir("alternate");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    // Spin as fast as possible: every tick submits once the previous step published, so steps
    // run back to back on pool threads while this thread writes the other buffer.
    let mut submits = 0;
    let t0 = Instant::now();
    while submits < 5000 {
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "stuck at {submits} submits: stats {:?}, error {:?}, log {}",
            j.e.stats_array(),
            j.e.protocol_error(),
            mic_native_host::drain_log()
        );
        if j.tick() {
            submits += 1;
            if submits % 7 == 0 {
                j.transfer(wire::JAVA_OP_BIT | submits, 1, 2, 1);
            }
        }
    }
    j.idle();
    let stats = e.stats_array();
    assert_eq!(stats[1], j.submitted, "every submitted step published");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(stats[13], 0, "no gaps");
    // Every journaled transfer applied once: 5000 / 7 ops, distinct.
    let expect = (1..=5000u64).filter(|s| s % 7 == 0).count() as i64;
    assert_eq!(ledger(&e, 2), expect);
    assert_eq!(ledger(&e, 1), -expect, "two-sided");
    eprintln!("5000 hand-overs in {:?}", t0.elapsed());
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn orders_become_commands_dict_first_sequences_rising_and_overflow_waits() {
    let dir = tmpdir("orders");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    // 300 units of 300 items each: about 4.8 KiB a unit, so a 64 KiB buffer takes 13 or so a step.
    let items: Vec<ItemOrder> = (0..300)
        .map(|i| ItemOrder {
            slot: i,
            count: 1,
            item: format!("minecraft:item_{}", i % 7),
        })
        .collect();
    for a in 0..300u64 {
        e.order(vec![Order::SpawnGroup {
            agent: 100 + a,
            epoch: 1,
            group_kind: CARAVAN.into(),
            dimension: OVERWORLD.into(),
            pos: [0.5, 64.0, 0.5],
            members: vec![MemberOrder {
                index: 0,
                health: 0.0,
                body_a: -1,
                body_b: -1,
                items: items.clone(),
            }],
        }])
        .unwrap();
    }
    let mut more_seen = false;
    j.run_until("every spawn applied", |j| {
        let k = j.next_cmd.saturating_sub(1);
        if k > 0 && j.cmd_buf(k).get_u32(wire::H_FLAGS) & wire::CMD_FLAG_MORE != 0 {
            more_seen = true;
        }
        j.commands
            .iter()
            .filter(|c| c.1 == cmd::SPAWN_GROUP)
            .count()
            == 300
    });
    assert!(more_seen, "a full buffer sets MORE and the rest waits");
    let seqs: Vec<u64> = j.commands.iter().map(|c| c.0).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] <= w[1]),
        "commands come in hand-over order"
    );
    assert_eq!(e.stats_array()[8], 0, "no order pending");
    assert_eq!(
        j.stale_commands, 0,
        "no command arrived at or below an applied sequence"
    );
    // The MATERIALIZED answers embodied every agent.
    j.idle();
    j.idle();
    assert_eq!(e.stats_array()[12], 300, "300 agents embodied");
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn transfers_apply_exactly_once_across_crashes_in_both_directions() {
    let dir = tmpdir("crash");
    // Session 1: a Java transfer and an engine transfer; the world saves; the server dies before
    // the engine saved anything.
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let java_op = wire::JAVA_OP_BIT | 0x5eed;
    j.transfer(java_op, 1, 7, 5);
    e.order(vec![transfer_order(4242, 3)]).unwrap();
    j.run_until("session 1", |j| {
        ledger(&j.e, 7) == 5
            && j.containers.values().sum::<i32>() == 3
            && j.e.inspect().op_applied(4242)
    });
    let saved = j.world_save();
    let _ = j.crash();

    // Session 2: the engine is back at no pages and re-derives the same op under a new sequence.
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), saved);
    e.order(vec![transfer_order(4242, 3)]).unwrap();
    j.run_until("the duplicate answer", |j| {
        j.journal.iter().any(|(_, r)| {
            wire::u16_at(r, 0) == obs::COMMAND_RESULT
                && wire::u32_at(r, wire::R_AUX) == obs::command_result::DUPLICATE_OP
        }) && ledger(&j.e, 7) == 5
    });
    assert_eq!(
        j.containers.values().sum::<i32>(),
        0,
        "this session's Minecraft never applied it again"
    );
    // A save that includes everything, then a crash before the acknowledgement trims the journal.
    j.idle();
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.run_until("pages on disk", |j| j.e.durable_obs_seq() > 0);
    let saved = j.world_save();
    assert!(
        !saved.journal.is_empty(),
        "the journal still holds the transfer"
    );
    let _ = j.crash();

    // Session 3: pages include the transfer, the replay is skipped, the same op id again is refused.
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), saved);
    assert!(j.pages_obs > 0);
    j.run_until("the replay", |j| j.e.stats_array()[2] > 0);
    assert_eq!(
        ledger(&e, 7),
        5,
        "a replay the pages include changes nothing"
    );
    j.transfer(java_op, 1, 7, 5);
    j.run_until("the duplicate op", |j| j.e.stats_array()[3] == 1);
    assert_eq!(ledger(&e, 7), 5, "the same op id twice applies once");
    let r = j.received();
    assert!(
        r.iter().any(|r| r.skipped),
        "replayed records were skipped by sequence"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn the_outbox_is_resent_after_a_restart_with_the_same_sequence_and_op() {
    let dir = tmpdir("resend");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    // A Java whose world save is lost: it hands command buffers back but its effects roll back.
    j.hold_commands = true;
    e.order(vec![transfer_order(0, 2)]).unwrap();
    j.run_until("written", |j| j.e.stats_array()[14] >= 1);
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    let before = j.e.stats_array()[9];
    j.run_until("saved", |j| j.e.stats_array()[9] > before);
    let save = WorldSave::fresh();
    let _ = j.crash();
    // Whatever Java applied of it was in a world save we threw away; the engine resends.
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), save);
    j.run_until("the resend", |j| {
        j.commands.iter().any(|c| c.1 == cmd::TRANSFER_TO_CONTAINER)
    });
    let resent = j
        .commands
        .iter()
        .find(|c| c.1 == cmd::TRANSFER_TO_CONTAINER)
        .unwrap();
    assert_eq!(
        resent.2, 1,
        "the engine's first op id, kept across the restart"
    );
    assert_eq!(
        resent.2 & wire::JAVA_OP_BIT,
        0,
        "an engine op id has the top bit clear"
    );
    assert_eq!(j.containers.values().sum::<i32>(), 2);
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn acknowledgements_name_only_pages_on_disk_and_stop_at_the_world_save() {
    let dir = tmpdir("ack");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    for i in 0..10 {
        j.transfer(wire::JAVA_OP_BIT | (100 + i), 1, 2, 1);
    }
    j.idle();
    j.idle();
    // The world saved with its journal at 10; ten more arrive in the same hand-over.
    let through = j.next_obs - 1;
    assert_eq!(through, 10);
    e.request_save(through);
    for i in 0..10 {
        j.transfer(wire::JAVA_OP_BIT | (200 + i), 1, 2, 1);
    }
    j.run_until("pages on disk", |j| j.e.durable_obs_seq() > 0);
    assert_eq!(
        e.durable_obs_seq(),
        through,
        "the pages stop exactly at the world save's journal"
    );
    j.run_until("the acknowledgement", |j| j.acks.contains(&through));
    assert!(
        j.acks.iter().all(|a| *a <= e.durable_obs_seq()),
        "never an ack beyond the disk"
    );
    assert_eq!(
        j.journal.front().map(|(s, _)| *s),
        Some(11),
        "trimmed to the acknowledgement"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn repeats_are_no_ops_and_a_session_start_disembodies() {
    let dir = tmpdir("repeat");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    e.order(vec![Order::SpawnGroup {
        agent: 77,
        epoch: 1,
        group_kind: CARAVAN.into(),
        dimension: OVERWORLD.into(),
        pos: [0.0; 3],
        members: (0..3)
            .map(|i| MemberOrder {
                index: i,
                health: 0.0,
                body_a: -1,
                body_b: -1,
                items: vec![],
            })
            .collect(),
    }])
    .unwrap();
    j.run_until("embodied", |j| j.e.stats_array()[12] == 1);
    // The same TAKEN twice (a chunk saved ahead of the journal reasserts it: same op, new sequence),
    // the same COMMAND_RESULT twice, a DIED, then the DIED again.
    let op = wire::JAVA_OP_BIT | 99;
    for _ in 0..2 {
        j.journal(obs::TAKEN, obs::taken::SIZE, 1, |r| {
            wire::put_u64(r, obs::taken::AGENT, 77);
            wire::put_u32(r, obs::taken::EPOCH, 1);
            wire::put_u32(r, obs::taken::MEMBER, 1);
            wire::put_u64(r, obs::taken::OP_ID, op);
        });
    }
    for _ in 0..2 {
        j.journal(obs::DIED, obs::died::SIZE, 2, |r| {
            wire::put_u64(r, obs::died::AGENT, 77);
            wire::put_u32(r, obs::died::EPOCH, 1);
            wire::put_u64(r, obs::died::OP_ID, wire::JAVA_OP_BIT | 98);
        });
    }
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none());
    // Forget the ops (a durable world save past them), then the TAKEN once more: by state.
    let durable = j.next_obs - 1;
    {
        let Some(r) = j.reserve(obs::WORLD_SAVED, obs::world_saved::SIZE, 0) else {
            panic!()
        };
        wire::put_u64(r, obs::world_saved::DURABLE_OBS_SEQ, durable);
    }
    j.idle();
    assert!(
        !e.inspect().replay.knows_op(rtsim_kernel::journal::OpId(op)),
        "forgotten"
    );
    j.journal(obs::TAKEN, obs::taken::SIZE, 1, |r| {
        wire::put_u64(r, obs::taken::AGENT, 77);
        wire::put_u32(r, obs::taken::EPOCH, 1);
        wire::put_u32(r, obs::taken::MEMBER, 1);
        wire::put_u64(r, obs::taken::OP_ID, op);
    });
    j.idle();
    j.idle();
    let i = e.inspect();
    let a = &i.agents[&77];
    assert!(
        matches!(a.members[1].gone, Some(rtsim_jni::world::Gone::Taken { how: 1, op: o }) if o == op)
    );
    assert!(matches!(
        a.members[2].gone,
        Some(rtsim_jni::world::Gone::Died { .. })
    ));
    assert!(a.members[0].gone.is_none());
    assert_eq!(
        i.counters.repeated_events, 1,
        "the third TAKEN, after its op was forgotten, was refused by state"
    );
    // A new session: the agent is abstract, and its old MATERIALIZED replayed never embodies it.
    let save = j.world_save();
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.run_until("saved", |j| j.e.durable_obs_seq() > 0);
    let save = WorldSave {
        journal: save.journal.clone(),
        ..save
    };
    let _ = j.crash();
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), save);
    j.idle();
    j.idle();
    assert_eq!(
        e.stats_array()[12],
        0,
        "nothing embodied after a session start"
    );
    assert_eq!(e.stats_array()[11], 1, "the agent is still known");
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_journal_trimmed_past_the_pages_is_a_counted_gap_not_a_stall() {
    let dir = tmpdir("gap");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut save = WorldSave::fresh();
    // A world whose journal starts at 40 while the engine's pages are empty.
    save.next_obs = 40;
    let mut j = Java::open(e.clone(), save);
    j.transfer(wire::JAVA_OP_BIT | 1, 1, 3, 4);
    j.transfer(wire::JAVA_OP_BIT | 2, 1, 3, 4);
    j.run_until("applied", |j| ledger(&j.e, 3) == 8);
    assert_eq!(e.stats_array()[13], 1, "one gap");
    e.close(true, Duration::from_secs(5)).unwrap();
}
