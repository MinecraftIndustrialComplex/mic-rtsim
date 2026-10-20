// SPDX-License-Identifier: AGPL-3.0-or-later
//! The buffer protocol of docs/boundary.md against a fake Java side, on the real host: mic_native
//! is started in this process, the engine's steps run on its background pool and its saves on its
//! IO lane, exactly as in game. The fake Java side (`common::Java`) is a Rust copy of the bridge's
//! `Handover` and `BoundarySession` rules: it writes observations into the open buffer, publishes
//! with release, submits, polls the command buffers with acquire, applies commands (skipping by
//! sequence, then by op id) and trims its journal on acknowledgements.

mod common;

use std::time::{Duration, Instant};

use common::*;
use rtsim_jni::engine::Engine;
use rtsim_jni::orders::{ItemOrder, MemberOrder, Order};
use rtsim_jni::wire::{self, cmd, obs};

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
    // A save that includes everything, then a crash before the acknowledgement trims the journal
    // (held: the save's own step may already carry it when its commit is quick).
    j.idle();
    j.hold_acks = true;
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.run_until("pages on disk", |j| j.e.durable_obs_seq() > 0);
    let saved = j.world_save();
    assert!(
        !saved.journal.is_empty(),
        "the journal still holds the transfer (acks {:?}, durable {}, next obs {}, sent {}, pages at start {})",
        j.acks,
        e.durable_obs_seq(),
        j.next_obs,
        j.sent_obs,
        j.pages_obs
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
    let written_op = *j.held_ops.first().expect("the transfer was written");
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
        resent.2, written_op,
        "the same op id, kept across the restart"
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

/// Review finding 1: Java's live lastAppliedCmdSeq is ahead of the world save the snapshot pairs
/// with, so trimming the outbox by it lost a transfer the world save never applied.
#[test]
fn a_transfer_applied_after_a_world_save_is_resent_after_a_crash() {
    let dir = tmpdir("adv-outbox");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    // Something journaled, so the world save has a non-zero through.
    j.transfer(wire::JAVA_OP_BIT | 7, 1, 2, 1);
    j.idle();
    // The engine decides a transfer; step h writes it.
    e.order(vec![transfer_order(0, 3)]).unwrap();
    let written_before = e.stats_array()[14];
    // Hand over until the step that writes it has published, but do not apply it yet.
    loop {
        // tick without applying: submit only
        j.preamble();
        if j.try_submit() {
            break;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while e.stats_array()[1] < j.submitted {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_micros(100));
    }
    assert!(
        e.stats_array()[14] > written_before,
        "the transfer was written"
    );
    // World save S: the journal is serialized now (lastApplied does not include the transfer).
    let s = j.world_save();
    let through = j.next_obs - 1;
    e.request_save(through);
    eprintln!(
        "world save S: through {through}, lastApplied {}",
        s.last_applied_cmd
    );
    // Same tick, ServerTickEvent.Post: Java applies the transfer, journals COMMAND_RESULT, submits.
    let saves_before = e.stats_array()[9];
    j.run_until("the engine's save for S", |j| {
        j.e.stats_array()[9] > saves_before
    });
    eprintln!(
        "engine pages durable through {}, Java lastApplied now {}",
        e.durable_obs_seq(),
        j.last_applied_cmd
    );
    assert_eq!(
        j.containers.values().sum::<i32>(),
        3,
        "applied in session 1"
    );
    // Crash before the next world save: the world rolls back to S, the engine to its pages.
    e.close(false, Duration::from_secs(5)).unwrap();
    let e2 = engine(&dir, wire::MIN_CAPACITY);
    let mut j2 = Java::open(e2.clone(), s);
    for _ in 0..5 {
        j2.idle();
    }
    eprintln!(
        "after restart: containers {:?}, commands {:?}",
        j2.containers, j2.commands
    );
    assert_eq!(
        j2.containers.values().sum::<i32>(),
        3,
        "the world save S never applied the transfer, so it must be resent"
    );
}

/// Review finding 2: op ids from a counter restored from the pages were reused after a crash for
/// a different transfer, which Java answered DUPLICATE and never applied.
#[test]
fn a_new_transfer_after_a_crash_never_reuses_an_op_id() {
    let dir = tmpdir("adv-opid");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    e.order(vec![transfer_order(0, 3)]).unwrap();
    j.idle();
    j.idle();
    assert_eq!(j.containers.values().sum::<i32>(), 3);
    let first_op = j
        .commands
        .iter()
        .find(|c| c.1 == cmd::TRANSFER_TO_CONTAINER)
        .unwrap()
        .2;
    // The world saves after applying it (journal holds COMMAND_RESULT(applied)); the engine had
    // no save since. Crash.
    let s = j.world_save();
    e.close(false, Duration::from_secs(5)).unwrap();
    let e2 = engine(&dir, wire::MIN_CAPACITY);
    let mut j2 = Java::open(e2.clone(), s);
    // A different transfer, decided after the restart.
    e2.order(vec![Order::TransferToContainer {
        op: 0,
        dimension: OVERWORLD.into(),
        item: "minecraft:emerald".into(),
        pos: [9, 64, 9],
        count: 5,
    }])
    .unwrap();
    for _ in 0..4 {
        j2.idle();
    }
    let second_op = j2
        .commands
        .iter()
        .find(|c| c.1 == cmd::TRANSFER_TO_CONTAINER)
        .map(|c| c.2);
    eprintln!(
        "first op {first_op}, second op {second_op:?}, containers {:?}",
        j2.containers
    );
    assert_eq!(
        j2.containers.get(&[9, 64, 9]).copied().unwrap_or(0),
        5,
        "the new transfer must be applied"
    );
}

#[test]
fn an_engine_failure_publishes_empty_buffers_flagged_failed() {
    let dir = tmpdir("fail");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.idle();
    e.order(vec![Order::Fail]).unwrap();
    j.run_until("the FAILED flag", |j| j.failed_seen);
    assert!(e.protocol_error().is_some_and(|m| m.contains("panicked")));
    // The bridge is never left waiting: steps keep publishing.
    let before = j.submitted;
    j.idle();
    assert!(j.submitted > before);
    e.close(true, Duration::from_secs(5)).unwrap();
}

// ---- phase 1 (docs/boundary.md §4) ----------------------------------------------------------------

const PLAYER: (u64, u64) = (0x1111_2222_3333_4444, 0x5555_6666_7777_8888);
const EMERALD: &str = "minecraft:emerald";

impl Java {
    /// Journals a VILLAGE_ADOPTED for a village whose meeting point is (x, z).
    fn adopt(&mut self, village: u64, structure: &str, culture: &str, pieces: u32, x: i32, z: i32) {
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
    fn cmd_name(&self, kind: u32, id: u32) -> String {
        self.cmd_dict.get(&(kind, id)).cloned().unwrap_or_default()
    }

    fn p1_of(&self, kind: u16) -> Vec<Vec<u8>> {
        self.p1
            .iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, b)| b.clone())
            .collect()
    }

    /// A TRADE with a puppet merchant: the player paid `cost` and got `result`.
    #[allow(clippy::too_many_arguments)]
    fn trade_with(
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
    fn robbed(
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

fn p1_stat(e: &Engine, i: usize) -> u64 {
    e.phase1_stats()[i]
}

/// Phase 1 end to end over the real boundary: villages adopted, a market snapshot for the one a
/// player stands in, a caravan materialized from the engine's own flows with its cargo in the
/// chests and its offers, a trade and a robbery booked once, the news carried to other villages
/// (WRITE_GOSSIP for a human village, sentiment), and after a crash the world rebuilt from the
/// pages' input log to the same state.
#[test]
fn phase1_caravans_trade_are_robbed_and_the_news_travels_across_a_crash() {
    let dir = tmpdir("phase1");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = 0b1111; // knowledge, settlements, trade, travel
    let (a, b, c) = (
        0x0001_0000_0000_0001u64,
        0x0001_0000_0000_0002,
        0x0001_0000_0000_0003,
    );
    j.adopt(a, "minecraft:village_plains", "human", 90, 0, 0);
    j.adopt(b, "minecraft:village_taiga", "human", 90, 400, 0);
    j.adopt(c, "minecraft:village_desert", "human", 90, 0, 400);
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.players = vec![(PLAYER, dim, [10.0, 64.0, 0.0])];
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(p1_stat(&e, 0), 3, "three villages joined");
    // The player stands in A: its market snapshot came, in emeralds.
    let markets = j.p1_of(cmd::MARKET);
    let am = markets
        .iter()
        .find(|m| wire::u64_at(m, cmd::market::VILLAGE) == a)
        .expect("a MARKET for the village the player stands in");
    assert_eq!(
        j.cmd_name(wire::DICT_ITEM, wire::u32_at(am, cmd::market::CURRENCY)),
        EMERALD
    );
    assert!(wire::i64_at(am, cmd::market::TREASURY) > 0);
    assert!(
        !markets
            .iter()
            .any(|m| wire::u64_at(m, cmd::market::VILLAGE) == b),
        "no snapshot for a village no player is near"
    );

    // Days on: news has crossed the roads and caravans with wares walk them (a caravan leaves
    // when its dock fills or after two days, so a road may be empty for a while).
    let agent = 0xCA4A_u64;
    let spawned = |j: &Java| {
        j.p1_of(cmd::SPAWN_GROUP)
            .iter()
            .any(|s| wire::u64_at(s, cmd::spawn_group::AGENT) == agent)
    };
    let mut days = 0;
    while !spawned(&j) {
        assert!(
            days < 240,
            "no caravan with wares on any road in 30 days: stats {:?}, log {}",
            e.phase1_stats(),
            mic_native_host::drain_log()
        );
        e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
        j.idle();
        days += 1;
        e.order(vec![Order::Phase1SpawnNear {
            agent,
            epoch: 1,
            dimension: OVERWORLD.into(),
            pos: [10.5, 64.0, 0.5],
            radius: 2_000,
            search: [10.5, 0.5],
        }])
        .unwrap();
        j.idle();
    }
    eprintln!("a caravan with wares after {} quanta", days * 3);
    j.idle();
    let spawn = j
        .p1_of(cmd::SPAWN_GROUP)
        .into_iter()
        .find(|s| wire::u64_at(s, cmd::spawn_group::AGENT) == agent)
        .unwrap();
    assert_eq!(
        j.cmd_name(
            wire::DICT_BODY,
            wire::u32_at(&spawn, cmd::spawn_group::GROUP_KIND)
        ),
        CARAVAN
    );
    // Its cargo, chest by chest (C21): (member, item, count).
    let mut cargo: Vec<(u32, String, i32)> = Vec::new();
    let mut at = cmd::spawn_group::MEMBERS;
    for _ in 0..wire::u32_at(&spawn, wire::R_AUX) {
        use cmd::spawn_group::member as mb;
        let idx = wire::u32_at(&spawn, at + mb::INDEX);
        let n = wire::u32_at(&spawn, at + mb::ITEM_COUNT) as usize;
        for i in 0..n {
            let ia = at + mb::ITEMS + i * obs::item::SIZE;
            cargo.push((
                idx,
                j.cmd_name(wire::DICT_ITEM, wire::u32_at(&spawn, ia + obs::item::ITEM)),
                wire::i32_at(&spawn, ia + obs::item::COUNT),
            ));
        }
        at += mb::size(n);
    }
    eprintln!("the caravan's cargo: {cargo:?}");
    assert!(!cargo.is_empty(), "a caravan off a flow carries something");
    assert_eq!(p1_stat(&e, 1), 1, "embodied");
    let offers = j.p1_of(cmd::MERCHANT_OFFERS);
    let mine: Vec<&Vec<u8>> = offers
        .iter()
        .filter(|o| wire::u64_at(o, cmd::merchant_offers::AGENT) == agent)
        .collect();
    assert_eq!(mine.len(), 1, "its offers came with it");
    let o = mine[0];
    let n_offers = wire::u32_at(o, wire::R_AUX) as usize;
    eprintln!("{n_offers} offers");

    // A trade off its first selling offer, if it has one, then a robbery of a stack of its cargo.
    let mut trades = 0;
    for i in 0..n_offers {
        use cmd::merchant_offers as k;
        let p = k::OFFERS + i * k::OFFER;
        if wire::u32_at(o, p + k::O_FLAGS) != k::O_FLAG_SELLS {
            continue;
        }
        let cost = j.cmd_name(wire::DICT_ITEM, wire::u32_at(o, p + k::O_COST_A_ITEM));
        let result = j.cmd_name(wire::DICT_ITEM, wire::u32_at(o, p + k::O_RESULT_ITEM));
        let (cn, rn) = (
            wire::i32_at(o, p + k::O_COST_A_COUNT),
            wire::i32_at(o, p + k::O_RESULT_COUNT),
        );
        let op = wire::JAVA_OP_BIT | 0x7a11;
        j.trade_with(
            agent,
            1,
            obs::trade::MERCHANT_SELLS,
            (&cost, cn),
            (&result, rn),
            op,
        );
        // The same trade again under the same op id (a chunk-carried resend): applied once.
        j.trade_with(
            agent,
            1,
            obs::trade::MERCHANT_SELLS,
            (&cost, cn),
            (&result, rn),
            op,
        );
        trades = 1;
        break;
    }
    let (robbed_item, robbed_n) = {
        let (_, item, n) = cargo
            .iter()
            .find(|(_, i, _)| i != EMERALD)
            .unwrap_or(&cargo[0]);
        (item.clone(), (*n).min(5))
    };
    let member = cargo.iter().find(|(_, i, _)| *i == robbed_item).unwrap().0;
    j.robbed(
        agent,
        1,
        member,
        &[(&robbed_item, robbed_n)],
        wire::JAVA_OP_BIT | 0x7a12,
    );
    j.idle();
    assert_eq!(p1_stat(&e, 3), trades, "the caravan trade booked once");
    assert_eq!(p1_stat(&e, 5), 1, "one robbery");
    let before = e.phase1_state();

    // News travels: a few days later some village other than the caravan's own feels the
    // robbery, and a human village is told to write it into its villagers' gossip.
    e.order(vec![Order::Phase1Advance(4 * 24_000)]).unwrap();
    j.idle();
    j.idle();
    let feel: Vec<(u64, i32)> = [a, b, c]
        .iter()
        .filter_map(|&v| e.phase1_sentiment(v, PLAYER).map(|s| (v, s)))
        .collect();
    eprintln!("sentiment toward the robber: {feel:?}");
    assert!(
        feel.iter().any(|(_, s)| *s < 0),
        "a village still holds the robbery against the player: {feel:?}"
    );
    let gossip = j.p1_of(cmd::WRITE_GOSSIP);
    for g in &gossip {
        use cmd::write_gossip as k;
        eprintln!(
            "WRITE_GOSSIP village {:#x} player {:#x} gossip {} amount {} report {}",
            wire::u64_at(g, k::VILLAGE),
            wire::u64_at(g, k::PLAYER_MOST),
            wire::u32_at(g, k::GOSSIP),
            wire::i32_at(g, k::AMOUNT),
            wire::u32_at(g, k::REPORT)
        );
    }
    eprintln!("phase 1 stats {:?}", e.phase1_stats());
    let told: std::collections::BTreeSet<u64> = gossip
        .iter()
        .filter(|g| {
            wire::u64_at(g, cmd::write_gossip::PLAYER_MOST) == PLAYER.0
                && wire::u32_at(g, cmd::write_gossip::REPORT) == 3
        })
        .map(|g| wire::u64_at(g, cmd::write_gossip::VILLAGE))
        .collect();
    assert!(
        told.len() >= 2,
        "the robbery was written into more than one village's gossip: {told:?}"
    );
    assert!(
        gossip
            .iter()
            .all(|g| wire::i32_at(g, cmd::write_gossip::AMOUNT) >= 2),
        "never an amount vanilla drops"
    );
    assert!(p1_stat(&e, 7) >= 1, "gossip sent");

    // A save, then a crash: the pages' input log rebuilds the same world.
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    while e.durable_obs_seq() < e.stats_array()[4] {
        std::thread::sleep(Duration::from_millis(2));
    }
    let (hash, _, _) = e.phase1_state();
    assert_ne!(hash, before.0, "the world moved on");
    let saved = j.world_save();
    let inputs = p1_stat(&e, 13);
    let _ = j.crash();
    let e = engine(&dir, 256 * 1024);
    let t0 = Instant::now();
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = 0b1111;
    eprintln!(
        "start with a phase 1 replay of {inputs} inputs: {:?}",
        t0.elapsed()
    );
    let (_, replay_ok, _) = e.phase1_state();
    assert!(replay_ok, "the replayed world is the saved one");
    j.idle();
    assert_eq!(p1_stat(&e, 0), 3, "the villages are back");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// Three villages, a player in the first, and days on until P1_SPAWN_NEAR materializes a caravan
/// with wares as `agent` (epoch 1). Returns its SPAWN_GROUP's cargo as (member, item, count).
fn phase1_caravan(e: &std::sync::Arc<Engine>, j: &mut Java, agent: u64) -> Vec<(u32, String, i32)> {
    j.module_mask = 0b1111;
    j.adopt(1, "minecraft:village_plains", "human", 90, 0, 0);
    j.adopt(2, "minecraft:village_taiga", "human", 90, 400, 0);
    j.adopt(3, "minecraft:village_desert", "human", 90, 0, 400);
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.players = vec![(PLAYER, dim, [10.0, 64.0, 0.0])];
    j.idle();
    let spawned = |j: &Java| {
        j.p1_of(cmd::SPAWN_GROUP)
            .iter()
            .any(|s| wire::u64_at(s, cmd::spawn_group::AGENT) == agent)
    };
    let mut quanta = 0;
    while !spawned(j) {
        assert!(
            quanta < 240,
            "no caravan with wares: {:?}",
            e.phase1_stats()
        );
        e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
        j.idle();
        quanta += 1;
        e.order(vec![Order::Phase1SpawnNear {
            agent,
            epoch: 1,
            dimension: OVERWORLD.into(),
            pos: [10.5, 64.0, 0.5],
            radius: 2_000,
            search: [10.5, 0.5],
        }])
        .unwrap();
        j.idle();
    }
    j.idle();
    let spawn = j
        .p1_of(cmd::SPAWN_GROUP)
        .into_iter()
        .find(|s| wire::u64_at(s, cmd::spawn_group::AGENT) == agent)
        .unwrap();
    let mut cargo = Vec::new();
    let mut at = cmd::spawn_group::MEMBERS;
    for _ in 0..wire::u32_at(&spawn, wire::R_AUX) {
        use cmd::spawn_group::member as mb;
        let idx = wire::u32_at(&spawn, at + mb::INDEX);
        let n = wire::u32_at(&spawn, at + mb::ITEM_COUNT) as usize;
        for i in 0..n {
            let ia = at + mb::ITEMS + i * obs::item::SIZE;
            cargo.push((
                idx,
                j.cmd_name(wire::DICT_ITEM, wire::u32_at(&spawn, ia + obs::item::ITEM)),
                wire::i32_at(&spawn, ia + obs::item::COUNT),
            ));
        }
        at += mb::size(n);
    }
    cargo
}

/// Review 3, P2: a puppet's robbery the pages do not include yet stays in Java's journal and is
/// replayed after a crash, into a session where the puppet no longer exists. It must still be
/// booked to the caravan: the player has the goods, and a caravan that keeps them too has
/// created them. Before the fix the engine looked the caravan up in this session's puppets only.
#[test]
fn phase1_puppet_records_replayed_after_a_crash_are_still_booked() {
    let dir = tmpdir("p1replay");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let agent = 0xCA4B_u64;
    let cargo = phase1_caravan(&e, &mut j, agent);
    assert!(!cargo.is_empty());
    // Pages with the caravan in them.
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    while e.durable_obs_seq() < e.stats_array()[4] {
        std::thread::sleep(Duration::from_millis(2));
    }
    // A robbery after that save, booked live.
    let (member, item, n) = cargo
        .iter()
        .find(|(_, i, _)| i != EMERALD)
        .cloned()
        .unwrap_or_else(|| cargo[0].clone());
    j.robbed(
        agent,
        1,
        member,
        &[(&item, n.min(3))],
        wire::JAVA_OP_BIT | 0x7b01,
    );
    j.idle();
    assert_eq!(p1_stat(&e, 5), 1, "the robbery booked live");
    // The world saves (its journal holds the robbery: the pages do not include it), then the
    // server dies.
    let saved = j.world_save();
    assert!(
        saved
            .journal
            .iter()
            .any(|(_, r)| wire::u16_at(r, wire::R_KIND) == obs::CARGO_CHANGED),
        "the journal still holds the robbery"
    );
    let _ = j.crash();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = 0b1111;
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(
        p1_stat(&e, 5),
        1,
        "the replayed robbery is booked to the caravan the puppet was"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

// ---- dragons (docs/boundary.md §6) ----------------------------------------------------------------

mod dragons {
    use super::*;
    use rtsim_ecology::boundary::{self as db, AttackerKind, Cmd as D, Obs as O, SpawnKind};
    use rtsim_ecology::dragon::LairKind;
    use rtsim_ecology::species::{Gender, Species};

    const LAIR: u64 = 0x0000_0BB8_0000_0BB8;
    const HOME: (i32, i32, i32) = (3000, 100, 3000);
    const EGG: (u64, u64) = (0xE66, 0xE66);

    /// Every module on: knowledge, settlements, trade, travel and ecology.
    const MODULES: u32 = 0b1_1111;

    fn untagged(entity: (u64, u64), spawn: SpawnKind, lair: u64, dim: u32) -> O {
        O::Untagged(db::Untagged {
            entity,
            spawn,
            species: Species::Fire,
            dimension: dim,
            flags: if lair != 0 { db::UNTAGGED_HAS_HOME } else { 0 },
            age_ticks: 110 * 24_000,
            hunger: 60,
            gender: Gender::Female,
            variant: 1,
            home: if lair != 0 { HOME } else { (0, 0, 0) },
            lair,
            pos: (f64::from(HOME.0), f64::from(HOME.1), f64::from(HOME.2)),
        })
    }

    fn captured(agent: u64, epoch: u32, dim: u32, health: f32) -> O {
        O::Dematerialized(db::Dematerialized {
            agent,
            epoch,
            reason: 3,
            dimension: dim,
            health,
            max_health: 500.0,
            hunger: 55,
            age_ticks: 110 * 24_000,
            pos: (f64::from(HOME.0), f64::from(HOME.1), f64::from(HOME.2)),
        })
    }

    fn adoptions(j: &Java) -> Vec<db::Adopt> {
        j.iaf
            .iter()
            .filter_map(|(_, c)| match c {
                D::Adopt(a) => Some(*a),
                _ => None,
            })
            .collect()
    }

    /// What must be the same after a crash and the journal's replay: the dragons, their health
    /// (whole points: regeneration is anchored at whenever it was last read), their feelings
    /// toward the player, what was robbed, and the counters.
    #[allow(clippy::type_complexity)]
    fn summary(e: &Engine) -> (usize, Vec<(u64, i64, i8)>, i128, u64, u64) {
        e.with_dragons(|d, w| {
            let p = w.knowledge.actors().find(
                rtsim_knowledge::ActorKind::Player,
                rtsim_ecology::engine::uuid_hash(PLAYER.0, PLAYER.1),
            );
            let mut rows: Vec<(u64, i64, i8)> = d
                .dragons
                .iter()
                .map(|(h, x)| {
                    (
                        h.to_bits(),
                        x.health.round() as i64,
                        p.map_or(0, |p| w.knowledge.sentiment(x.actor, p, d.now())),
                    )
                })
                .collect();
            rows.sort();
            (
                d.dragons.len(),
                rows,
                d.ledger.robbed.values().sum(),
                d.stats.hoards_robbed,
                d.stats.discarded,
            )
        })
    }

    /// The dragons through the native engine and a fake Ice and Fire plugin: a lair and its
    /// structure dragon adopted, a player's wound (a grudge), the body captured with its health,
    /// a player at the lair materializing it again, a natural spawn discarded; then a save, a hoard
    /// robbery and a spawn-egg dragon after it, and a crash: the pages read back to the saved
    /// state hash, and the journal's replay books the robbery once and adopts the egg dragon once.
    #[test]
    fn dragons_are_adopted_hurt_captured_materialized_and_replayed_across_a_crash() {
        let dir = tmpdir("dragons");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.dragon_obs(&O::LairFound(db::LairFound {
            lair: LAIR,
            late: false,
            dimension: dim,
            species: Species::Fire,
            kind: LairKind::Roost,
            min: (HOME.0 - 30, 60, HOME.2 - 30),
            max: (HOME.0 + 30, 140, HOME.2 + 30),
            home: HOME,
        }));
        j.dragon_obs(&untagged((0xD1, 0x1), SpawnKind::Structure, LAIR, dim));
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let first = adoptions(&j);
        assert_eq!(first.len(), 1, "the lair's own dragon is adopted");
        let (agent, epoch) = (first[0].agent, first[0].epoch);
        assert_eq!(agent >> 56, 0x44, "a dragon's agent id (R13)");
        assert_eq!(first[0].home, HOME);
        assert_eq!(e.with_dragons(|d, _| d.lairs[0].occupants.len()), 1);

        // A player wounds it: a grudge.
        j.dragon_obs(&O::Hurt(db::Hurt {
            agent,
            epoch,
            attacker_kind: AttackerKind::Player,
            attacker: PLAYER,
            hits: 3,
            amount: 140.0,
            health_after: 210.0,
        }));
        j.idle();
        let (grudge, threshold) = e.with_dragons(|d, w| {
            let (h, x) = d.dragons.iter().next().unwrap();
            let p = w.knowledge.actors().find(
                rtsim_knowledge::ActorKind::Player,
                rtsim_ecology::engine::uuid_hash(PLAYER.0, PLAYER.1),
            );
            let _ = h;
            (
                p.map(|p| w.knowledge.sentiment(x.actor, p, d.now())),
                d.tun.grudge_below,
            )
        });
        let grudge = grudge.expect("the player is known");
        assert!(grudge <= threshold, "a wound is a grudge ({grudge})");

        // The body leaves tracking: captured with its health.
        j.dragon_obs(&captured(agent, epoch, dim, 210.0));
        j.idle();
        let health = e.with_dragons(|d, _| d.dragons.iter().next().unwrap().1.health);
        assert_eq!(health, 210.0, "the capture's health is the dragon's");

        // A player walks up to the lair: the engine materializes the dragon there (a new epoch),
        // the plugin answers, and the player goes.
        j.players = vec![(PLAYER, dim, [3010.0, 70.0, 3005.0])];
        let materialized = |j: &Java| {
            j.iaf.iter().find_map(|(_, c)| match c {
                D::Materialize(m) if m.agent == agent => Some(*m),
                _ => None,
            })
        };
        let mut ticks = 0;
        while materialized(&j).is_none() {
            assert!(
                ticks < 4_000,
                "no materialization at the lair: {:?}",
                e.dragon_stats()
            );
            j.tick();
            ticks += 1;
            std::thread::sleep(Duration::from_micros(100));
        }
        j.idle();
        let m = materialized(&j).unwrap();
        assert!(m.epoch > epoch, "a new epoch for every materialization");
        j.players.clear();
        j.dragon_obs(&captured(agent, m.epoch, dim, 230.0));
        j.idle();

        // A natural spawn: discarded (the sim owns the population, P13), answered by
        // COMMAND_RESULT.
        j.dragon_obs(&untagged((0xD1, 0x2), SpawnKind::Natural, 0, dim));
        j.idle();
        j.idle();
        let discards: Vec<_> = j
            .iaf
            .iter()
            .filter(|(_, c)| matches!(c, D::Discard(x) if x.entity == (0xD1, 0x2)))
            .collect();
        assert_eq!(discards.len(), 1, "the natural dragon is discarded");
        assert_eq!(summary(&e).0, 1, "and never joins the sim");

        // A save: pages with all of that.
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        while e.durable_obs_seq() < e.stats_array()[4] {
            std::thread::sleep(Duration::from_millis(2));
        }

        // After the save: a hoard robbery and a spawn-egg dragon, journaled; the world saves,
        // then the server dies.
        let item = j.dict_id(wire::DICT_ITEM, EMERALD);
        j.dragon_obs(&O::HoardRobbed(db::HoardRobbed {
            lair: LAIR,
            op: wire::JAVA_OP_BIT | 0xD00D,
            player: PLAYER,
            items: vec![(item, 12)],
        }));
        j.dragon_obs(&untagged(EGG, SpawnKind::SpawnEgg, 0, dim));
        j.idle();
        j.idle();
        let egg = adoptions(&j)
            .into_iter()
            .find(|a| a.entity == EGG)
            .expect("the egg dragon is adopted");
        j.dragon_obs(&captured(egg.agent, egg.epoch, dim, 300.0));
        j.idle();
        let before = summary(&e);
        assert_eq!(before.0, 2, "two dragons");
        assert_eq!(before.3, 1, "one robbery");
        let saved = j.world_save();
        assert!(
            saved
                .journal
                .iter()
                .any(|(_, r)| wire::u16_at(r, wire::R_KIND) == db::HOARD_ROBBED),
            "the journal holds what the pages lack"
        );
        let _ = j.crash();

        // The restart: the pages read back to the state they were saved with, and the journal's
        // replay brings the rest once.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(
            e.dragons_restore_check(),
            (None, false),
            "the dragons read back to their saved state hash"
        );
        let (_, replay_ok, _) = e.phase1_state();
        assert!(replay_ok, "the world they live in too");
        assert_eq!(summary(&e).0, 1, "the pages predate the egg dragon");
        j.idle();
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let after = summary(&e);
        assert_eq!(after.0, 2, "the egg dragon adopted once, never twice");
        assert_eq!(after.2, before.2, "the robbery booked once");
        assert_eq!(after.3, before.3);
        assert_eq!(
            after.1, before.1,
            "every dragon's health and grudge as before the crash"
        );
        // The replayed robbery again under its op id (a chunk-carried resend): nothing more.
        j.dragon_obs(&O::HoardRobbed(db::HoardRobbed {
            lair: LAIR,
            op: wire::JAVA_OP_BIT | 0xD00D,
            player: PLAYER,
            items: vec![(item, 12)],
        }));
        j.idle();
        assert_eq!(summary(&e).3, 1, "a robbery's op id is applied once");
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();

        // A clean restart reads them back again.
        let e = engine(&dir, 256 * 1024);
        let _j = Java::open(e.clone(), saved);
        assert_eq!(e.dragons_restore_check(), (None, false));
        assert_eq!(summary(&e).0, after.0);
        assert_eq!(summary(&e).2, after.2);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// C25 (U10): with the ecology module switched off mid-session the dragons ignore their
    /// records (a hoard robbery), decide nothing (a player at the lair sees no dragon) and keep
    /// the page they had at the switch; switched on again they run on and the player sees the
    /// dragon.
    #[test]
    fn switched_off_dragons_ignore_their_records_and_keep_their_page() {
        use rtsim_jni::engine::DRAGONS_TABLE;
        use rtsim_kernel::page::{PageKey, PageStore};
        let page = |dir: &std::path::PathBuf| {
            let mut store =
                PageStore::open(dir, rtsim_kernel::page::Durability::NoSync).expect("open");
            store.get(PageKey::new(DRAGONS_TABLE, 0)).expect("read")
        };
        let dir = tmpdir("dragons-off");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.dragon_obs(&O::LairFound(db::LairFound {
            lair: LAIR,
            late: false,
            dimension: dim,
            species: Species::Fire,
            kind: LairKind::Roost,
            min: (HOME.0 - 30, 60, HOME.2 - 30),
            max: (HOME.0 + 30, 140, HOME.2 + 30),
            home: HOME,
        }));
        // A well-fed dragon: it stays at rest in its lair for days (R7), so a player there sees
        // it whenever the dragons run.
        let mut fed = untagged((0xD2, 0x1), SpawnKind::Structure, LAIR, dim);
        if let O::Untagged(u) = &mut fed {
            u.hunger = 100;
        }
        j.dragon_obs(&fed);
        j.idle();
        j.idle();
        let a = adoptions(&j)[0];
        let mut back = captured(a.agent, a.epoch, dim, 400.0);
        if let O::Dematerialized(d) = &mut back {
            d.hunger = 100;
        }
        j.dragon_obs(&back);
        j.idle();

        // Off: a robbery is ignored, a player at the lair sees nothing.
        j.modules(0b1111);
        j.idle();
        let item = j.dict_id(wire::DICT_ITEM, EMERALD);
        j.dragon_obs(&O::HoardRobbed(db::HoardRobbed {
            lair: LAIR,
            op: wire::JAVA_OP_BIT | 0xD0FF,
            player: PLAYER,
            items: vec![(item, 3)],
        }));
        j.players = vec![(PLAYER, dim, [3010.0, 70.0, 3005.0])];
        let commands = j.iaf.len();
        for _ in 0..600 {
            j.tick();
        }
        j.idle();
        assert!(e.dragons_ignored() >= 1, "the robbery was ignored");
        assert_eq!(e.dragon_stats().0.hoards_robbed, 0);
        assert_eq!(
            j.iaf.len(),
            commands,
            "no dragon command while switched off"
        );
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        let at_switch = page(&dir).expect("a dragons page");

        // A session that starts with it off keeps that page exactly.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        j.players = vec![(PLAYER, dim, [3010.0, 70.0, 3005.0])];
        for _ in 0..300 {
            j.tick();
        }
        j.idle();
        assert!(j.iaf.is_empty());

        // On again, mid-session: the player at the lair sees the dragon.
        j.modules(MODULES);
        j.run_until("a materialization after the switch", |j| {
            j.iaf
                .iter()
                .any(|(_, c)| matches!(c, D::Materialize(m) if m.agent == a.agent))
        });
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let saved = j.world_save();
        let _ = j.crash();
        assert_eq!(
            page(&dir),
            Some(at_switch.clone()),
            "the page kept while switched off"
        );
        let e = engine(&dir, 256 * 1024);
        let _j = Java::open(e.clone(), saved);
        assert_eq!(e.dragons_restore_check(), (None, false));
        e.close(false, Duration::from_secs(5)).unwrap();
    }
}

// ---- ships (docs/boundary.md §7) ------------------------------------------------------------------

mod ships {
    use super::*;
    use rtsim_jni::engine::{MODULE_SHIPS, SHIPS_TABLE};
    use rtsim_kernel::page::{PageKey, PageStore};
    use rtsim_ships::boundary::{self as sb, By, Cmd as S, Obs as O};
    use rtsim_ships::engine::Ship;

    /// Knowledge, settlements, trade, travel and ships.
    const MODULES: u32 = 0b1111 | MODULE_SHIPS;
    const HAVEN: u64 = 0x0000_0000_0000_0042;
    const VILLAGES: [(u64, &str, i32, i32); 3] = [
        (0x0001_0000_0000_0011, "minecraft:village_plains", 0, 0),
        (0x0001_0000_0000_0012, "minecraft:village_taiga", 400, 0),
        (0x0001_0000_0000_0013, "minecraft:village_desert", 0, 400),
    ];

    /// Three villages with sea ports and a haven between them, reported by the plugin; the
    /// network is built at the next simulate point.
    fn coast(j: &mut Java) {
        for (v, structure, x, z) in VILLAGES {
            j.adopt(v, structure, "human", 90, x, z);
        }
        j.idle();
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        for (v, _, x, z) in VILLAGES {
            j.ship_obs(&O::PortFound(sb::PortFound {
                village: v,
                medium: rtsim_travel::Medium::Sea,
                dimension: dim,
                structure: 0,
                dock: (x + 8, 63, z + 8),
                berths: 4,
            }));
        }
        j.ship_obs(&O::HavenFound(sb::HavenFound {
            haven: HAVEN,
            media: 1,
            dimension: dim,
            structure: 0,
            dock: (200, 63, 200),
        }));
        j.idle();
        j.idle();
    }

    /// The materializations the plugin applied: (cmdSeq, record).
    fn materializations(j: &Java) -> Vec<sb::Materialize> {
        j.ships
            .iter()
            .filter_map(|(_, c)| match c {
                S::Materialize(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    /// Goods a lane's origins send on it now, both directions (thousandths a quantum).
    fn lane_flow(e: &Engine, l: usize) -> i64 {
        e.with_ships(|s, w| {
            w.edges[s.lanes[l].edge as usize]
                .dirs
                .iter()
                .map(|d| d.goods.values().map(|p| p.current_rate()).sum::<i64>())
                .sum()
        })
    }

    /// What must be the same after a crash and the journal's replay: the network, what players
    /// took, the counters, and every ship's crew, hull and hold (embodiment and epochs are per
    /// session).
    #[allow(clippy::type_complexity)]
    fn summary(e: &Engine) -> (usize, usize, i128, u64, u64, Vec<(u64, u32, u32, i64)>) {
        e.with_ships(|s, _| {
            let mut rows: Vec<(u64, u32, u32, i64)> = s
                .ships
                .iter()
                .map(|(id, x)| {
                    (
                        Ship::agent(id),
                        x.crew,
                        x.hull_milli,
                        x.cargo.values().sum::<i64>(),
                    )
                })
                .collect();
            rows.sort();
            (
                s.docks.len(),
                s.lanes.len(),
                s.ledger.taken_by_players.values().sum(),
                s.stats.loot_events,
                s.stats.fights_observed,
                rows,
            )
        })
    }

    fn page(dir: &std::path::PathBuf) -> Option<Vec<u8>> {
        let mut store = PageStore::open(dir, rtsim_kernel::page::Durability::NoSync).expect("open");
        store.get(PageKey::new(SHIPS_TABLE, 0)).expect("read")
    }

    fn save(e: &Engine, j: &mut Java) {
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        while e.durable_obs_seq() < e.stats_array()[4] {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The ships through the native engine and a fake ships plugin: ports and a haven reported,
    /// the network built, a merchant entering a player's view materialized with its crew and hold
    /// (DICT records first, Java's names), a fight reported and booked, a save; then the player
    /// loots the hold (applied once, the same op id again changes nothing) and the server dies.
    /// The pages read back to their saved state hash, and the journal's replay books the loot
    /// once, leaving every ship as before the crash.
    #[test]
    fn ships_materialize_fight_are_looted_once_and_replay_across_a_crash() {
        let dir = tmpdir("ships");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        coast(&mut j);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (docks, lanes, services, havens) = e.with_ships(|s, _| {
            (
                s.docks.len(),
                s.lanes.len(),
                s.services.len(),
                s.havens.len(),
            )
        });
        assert_eq!((docks, havens), (3, 1), "three ports and a haven");
        assert!(
            lanes >= 2 && services >= 1,
            "a network: {lanes} lanes, {services} services"
        );

        // Days on, until a lane carries goods.
        let mut rounds = 0;
        let lane = loop {
            if let Some(l) = (0..lanes).find(|&l| lane_flow(&e, l) > 0) {
                break l;
            }
            assert!(rounds < 400, "no goods on any lane: {:?}", e.ship_stats());
            e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
            j.idle();
            rounds += 1;
        };
        // A save: what follows reaches the next session only through the journal.
        save(&e, &mut j);

        // The player stands at the lane's middle. A merchant with goods in its hold enters the
        // view, with most of its voyage still ahead.
        let mid = e.with_ships(|s, _| {
            let p = &s.lanes[lane].path;
            p.at(p.length() / 2)
        });
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.players = vec![(PLAYER, dim, [f64::from(mid.0), 64.0, f64::from(mid.1)])];
        let laden = |j: &Java, e: &Engine| {
            let now = e.with_ships(|_, w| w.now().0 as i64);
            materializations(j).into_iter().find(|m| {
                m.role == sb::Role::Merchant && !m.cargo.is_empty() && m.times[3] > now + 400
            })
        };
        let mut ticks = 0;
        j.run_until("a merchant with a hold in view", |j| {
            ticks += 1;
            laden(j, &j.e.clone()).is_some()
        });
        let m = laden(&j, &e).expect("found");
        assert_eq!(m.agent >> 56, 0x53, "a ship's agent id");
        assert_eq!(m.dimension, dim, "the dimension in Java's dictionary");
        // The members: the ship first, then its crew (docs/boundary.md §7.4).
        let names: Vec<&str> = m
            .members
            .iter()
            .map(|x| j.dict[x.body as usize - 1].1.as_str())
            .collect();
        assert_eq!(j.dict[m.body as usize - 1].1, "mic_rtsim:ship/sea_merchant");
        assert_eq!(names[0], "mic_rtsim:ship/sea_merchant");
        assert!(
            names[1..].iter().all(|n| *n == "mic_rtsim:crew/sailor"),
            "the crew: {names:?}"
        );
        assert_eq!(
            m.members.len() as u32,
            m.crew + 1,
            "the ship and its crew, one group"
        );
        let (shared, seen) = (e.ship_stats().0.interest_bookings, e.ship_stats().0.seen);
        assert!(shared > 0 && seen > 0, "the player's shared area saw it");

        // At once (the hold is the voyage's): the plugin reports a fight it played (the pirates
        // broke off; both sides' wounds are the engine's) and the player loots the hold.
        let pirate = e.with_ships(|s, _| Ship::agent(s.havens[0].ships[0]));
        j.ship_obs(&O::Fight(sb::Fight {
            attacker: pirate,
            defender: m.agent,
            outcome: 3,
            attacker_crew: (8, 6),
            defender_crew: (m.crew, m.crew - 1),
            attacker_hull_milli: 700,
            defender_hull_milli: 850,
        }));
        let items: Vec<sb::Item> = m
            .cargo
            .iter()
            .map(|it| sb::Item {
                item: it.item,
                count: (it.count / 2).max(1),
            })
            .collect();
        let loot = O::CargoLooted(sb::CargoLooted {
            agent: m.agent,
            epoch: m.epoch,
            by: By::Player,
            player: PLAYER,
            ship: 0,
            op: wire::JAVA_OP_BIT | 0x5A11,
            items,
        });
        j.ship_obs(&loot);
        j.idle();
        let row = |e: &Engine, agent: u64| {
            e.with_ships(|s, _| {
                let x = s.ship(Ship::id_of_agent(agent).unwrap()).unwrap();
                (x.crew, x.hull_milli)
            })
        };
        assert_eq!(e.ship_stats().0.fights_observed, 1);
        assert_eq!(row(&e, m.agent), (m.crew - 1, 850), "the merchant's wounds");
        assert_eq!(row(&e, pirate), (6, 700), "the pirates' wounds");
        let before = summary(&e);
        eprintln!(
            "ships exchange: {rounds} advances of 3,000 ticks, then {ticks} ticks to a laden merchant; \
             {} docks, {} lanes, taken {} thousandths, stats {:?}",
            before.0,
            before.1,
            before.2,
            e.ship_stats()
        );
        assert_eq!(
            before.3,
            1,
            "one loot: {:?}; materialized phase {} times {:?} cargo {:?}; now {:?}",
            e.ship_stats(),
            m.phase,
            m.times,
            m.cargo,
            e.with_ships(|s, w| {
                let x = s.ship(Ship::id_of_agent(m.agent).unwrap()).unwrap();
                (w.now(), x.epoch, x.promoted, x.cargo.clone())
            })
        );
        assert!(before.2 > 0, "the player took goods from the hold");
        // The same loot again under its op id (a chunk-carried resend): nothing more.
        j.ship_obs(&loot);
        j.idle();
        assert_eq!(summary(&e), before, "a loot's op id is applied once");
        let saved = j.world_save();
        assert!(
            saved
                .journal
                .iter()
                .any(|(_, r)| wire::u16_at(r, wire::R_KIND) == sb::CARGO_LOOTED),
            "the journal holds what the pages lack"
        );
        let _ = j.crash();

        // The restart: the pages read back to their saved state, and the replay brings the rest
        // once.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(
            e.ships_restore_check(),
            (None, false),
            "the ships read back to their saved state hash"
        );
        let (_, replay_ok, _) = e.phase1_state();
        assert!(replay_ok, "the world they live in too");
        assert_eq!(summary(&e).3, 0, "the pages predate the loot");
        j.idle();
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(
            summary(&e),
            before,
            "the loot booked once, every ship as before the crash"
        );
        j.ship_obs(&loot);
        j.idle();
        assert_eq!(summary(&e), before, "and its op id still applies once");
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();

        // A clean restart reads them back again.
        let e = engine(&dir, 256 * 1024);
        let _j = Java::open(e.clone(), saved);
        assert_eq!(e.ships_restore_check(), (None, false));
        assert_eq!(summary(&e), before);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// C25 (U10): with the ships module switched off they ignore their records, write nothing
    /// and keep the page they had; switched on again they run on.
    #[test]
    fn switched_off_ships_ignore_their_records_and_keep_their_page() {
        let dir = tmpdir("ships-off");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        coast(&mut j);
        assert_eq!(e.with_ships(|s, _| s.docks.len()), 3);
        save(&e, &mut j);
        j.modules(0b1111);
        j.idle();
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        let at_switch = page(&dir).expect("a ships page");

        // Off from the start of a session: a new port is ignored, a player by a lane sees no
        // ship, and the page stays as it was.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.adopt(
            0x0001_0000_0000_0014,
            "minecraft:village_savanna",
            "human",
            90,
            400,
            400,
        );
        j.idle();
        j.ship_obs(&O::PortFound(sb::PortFound {
            village: 0x0001_0000_0000_0014,
            medium: rtsim_travel::Medium::Sea,
            dimension: dim,
            structure: 0,
            dock: (408, 63, 408),
            berths: 4,
        }));
        let mid = e.with_ships(|s, _| {
            let p = &s.lanes[0].path;
            p.at(p.length() / 2)
        });
        j.players = vec![(PLAYER, dim, [f64::from(mid.0), 64.0, f64::from(mid.1)])];
        for _ in 0..3_000 {
            j.tick();
        }
        j.idle();
        let (stats, _, ignored, written) = e.ship_stats();
        assert!(ignored >= 1, "the port was ignored ({ignored})");
        assert_eq!(e.with_ships(|s, _| s.docks.len()), 3, "no fourth dock");
        assert_eq!(
            (written, stats.interest_bookings),
            (0, 0),
            "nothing seen, nothing written"
        );
        assert!(j.ships.is_empty(), "no ship command reached Java");
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_eq!(
            page(&dir),
            Some(at_switch.clone()),
            "the page kept as it was"
        );

        // On again: they run, and the player by the lane sees ships.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.players = vec![(PLAYER, dim, [f64::from(mid.0), 64.0, f64::from(mid.1)])];
        assert_eq!(e.ships_restore_check(), (None, false));
        j.run_until("the ships seeing the player again", |j| {
            j.e.ship_stats().0.interest_bookings > 0
        });
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_ne!(
            page(&dir),
            Some(at_switch),
            "running again, the page moves on"
        );
    }

    /// One interest area per player across the modules (U2): with a dragon at rest by the lane
    /// and the ships sailing it, the player reported every 20 ticks has one leg in the shared
    /// index, and both the dragon and a ship enter the player's view through it.
    #[test]
    fn dragons_and_ships_see_a_player_through_one_area() {
        use rtsim_ecology::boundary::{self as db, Cmd as D, Obs as DO, SpawnKind};
        use rtsim_ecology::dragon::LairKind;
        use rtsim_ecology::species::{Gender, Species};
        let dir = tmpdir("shared-area");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES | rtsim_jni::engine::MODULE_ECOLOGY;
        coast(&mut j);
        let mid = e.with_ships(|s, _| {
            let p = &s.lanes[0].path;
            p.at(p.length() / 2)
        });
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let lair: u64 = 0x0000_0000_0000_0D0D;
        let home = (mid.0 + 40, 90, mid.1 + 40);
        j.dragon_obs(&DO::LairFound(db::LairFound {
            lair,
            late: false,
            dimension: dim,
            species: Species::Fire,
            kind: LairKind::Roost,
            min: (home.0 - 30, 60, home.2 - 30),
            max: (home.0 + 30, 140, home.2 + 30),
            home,
        }));
        j.dragon_obs(&DO::Untagged(db::Untagged {
            entity: (0xD3, 0x1),
            spawn: SpawnKind::Structure,
            species: Species::Fire,
            dimension: dim,
            flags: db::UNTAGGED_HAS_HOME,
            age_ticks: 110 * 24_000,
            hunger: 90,
            gender: Gender::Female,
            variant: 0,
            home,
            lair,
            pos: (f64::from(home.0), f64::from(home.1), f64::from(home.2)),
        }));
        j.idle();
        j.idle();
        let adopted = j
            .iaf
            .iter()
            .find_map(|(_, c)| match c {
                D::Adopt(a) => Some(*a),
                _ => None,
            })
            .expect("the lair's dragon adopted");
        j.dragon_obs(&DO::Dematerialized(db::Dematerialized {
            agent: adopted.agent,
            epoch: adopted.epoch,
            reason: 3,
            dimension: dim,
            health: 400.0,
            max_health: 500.0,
            hunger: 90,
            age_ticks: 110 * 24_000,
            pos: (f64::from(home.0), f64::from(home.1), f64::from(home.2)),
        }));
        j.idle();
        let player_legs = |e: &Engine| {
            e.with_ships(|_, w| {
                w.legs
                    .iter()
                    .filter(|(_, r)| r.owner.0 & rtsim_world::interest::PLAYER_OWNER != 0)
                    .count()
            })
        };
        j.players = vec![(PLAYER, dim, [f64::from(mid.0), 64.0, f64::from(mid.1)])];
        let dragon_seen = |j: &Java| {
            j.iaf
                .iter()
                .any(|(_, c)| matches!(c, D::Materialize(m) if m.agent == adopted.agent))
        };
        let ship_seen = |j: &Java| !materializations(j).is_empty();
        let mut ticks = 0;
        j.run_until("the dragon and a ship in view", |j| {
            ticks += 1;
            if ticks % 100 == 0 {
                assert!(player_legs(&j.e) <= 1, "one area for the player");
            }
            dragon_seen(j) && ship_seen(j)
        });
        j.idle();
        assert_eq!(player_legs(&e), 1, "one area for the player");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (d, _, _) = e.dragon_stats();
        assert_eq!(d.seen, 1, "the dragon seen through one meeting: {d:?}");
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

mod herds {
    use super::*;
    use rtsim_ecology::herds::boundary::{self as hb, Cmd as H, MEMBER_SHEARED, Obs as O};
    use rtsim_ecology::herds::species::Cohort;
    use rtsim_jni::engine::{HERDS_TABLE, MODULE_HERDS};
    use rtsim_kernel::page::{PageKey, PageStore};

    /// The core's four phase 1 modules and ECOLOGY, which the herds run under (Y2).
    const MODULES: u32 = 0b1111 | MODULE_HERDS;
    const AT: (i32, i32) = (1_000, 1_000);

    fn found(e: &Engine, j: &mut Java, count: u64) -> u64 {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        e.order(vec![Order::HerdFound {
            name: "Testdown flock".into(),
            species: "minecraft:sheep".into(),
            dimension: dim,
            x: AT.0,
            z: AT.1,
            count,
            seed: 0,
        }])
        .unwrap();
        j.idle();
        let agents = e.herd_agents();
        assert_eq!(agents.len(), 1, "one herd founded");
        agents[0]
    }

    fn materializations(j: &Java) -> Vec<hb::Materialize> {
        j.herds
            .iter()
            .filter_map(|(_, c)| match c {
                H::Materialize(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    fn info(e: &Engine, agent: u64) -> [u64; rtsim_jni::engine::HERD_INFO] {
        e.herd_info(agent).expect("a herd")
    }

    /// (alive, killed, taken, lost, dead-set size): what the herd remembers of what players did.
    fn summary(e: &Engine, agent: u64) -> (u64, u64, u64, u64, u64) {
        let i = info(e, agent);
        (i[1], i[2], i[3], i[4], i[7])
    }

    fn save(e: &Engine, j: &mut Java) {
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        while e.durable_obs_seq() < e.stats_array()[4] {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The herds through the native engine and a fake vanilla herd adapter (rtsim-ecology's
    /// `FakeHerds`): a herd founded by the test order, a player beside it seeing a group of its
    /// sheep materialize (the engine's species, colours, ages and positions, DICT records first),
    /// a kill, a shearing and a lead (each booked once, by op id), an obstacle (the herd replans),
    /// the player leaving (the group captured with each member's state), a save, the player back
    /// (the same sheep, the sheared one still sheared, no dead one), then a kill after the save
    /// and a crash: the pages read back to their saved state hash and the journal's replay brings
    /// the herd to the state it had before the crash, the kill booked once.
    #[test]
    fn herds_materialize_are_hunted_shorn_led_blocked_and_replay_across_a_crash() {
        let dir = tmpdir("herds");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let agent = found(&e, &mut j, 200);
        assert_eq!(agent >> 56, 0x48, "a herd's agent id (namespace 0x48)");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let alive0 = info(&e, agent)[1];
        assert_eq!(alive0, 200, "200 sheep founded");

        // A player beside the herd: the group materializes.
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let pos = [f64::from(AT.0) + 20.0, 64.0, f64::from(AT.1) - 10.0];
        j.players = vec![(PLAYER, dim, pos)];
        j.run_until("a herd group in view", |j| !materializations(j).is_empty());
        j.idle();
        let m = materializations(&j).remove(0);
        assert_eq!(m.agent, agent);
        assert_eq!(m.species, rtsim_ecology::herds::HerdSpecies::Sheep);
        assert_eq!(m.dimension, dim, "the dimension in Java's dictionary");
        assert_eq!(
            m.flags & hb::MAT_SURFACE,
            hb::MAT_SURFACE,
            "members stand on the surface"
        );
        assert_eq!(
            m.members.len(),
            32,
            "the group cap (every sheep's footprint is in view)"
        );
        for b in &m.members {
            assert!(
                [0, 6, 7, 8, 12, 15].contains(&b.variant),
                "a natural sheep colour: {}",
                b.variant
            );
            assert_eq!(b.vanilla_age < 0, b.cohort == Cohort::Young, "{b:?}");
            let d = ((f64::from(b.x) - pos[0]).powi(2) + (f64::from(b.z) - pos[2]).powi(2)).sqrt();
            assert!(d <= 160.0, "inside the player's area: {d}");
        }
        let distinct: std::collections::BTreeSet<u64> =
            m.members.iter().map(|b| b.member).collect();
        assert_eq!(distinct.len(), m.members.len(), "distinct members");
        let i = info(&e, agent);
        assert_eq!((i[5], i[6]), (u64::from(m.epoch), 32), "embodied");

        // The player kills one, shears one and leads one away.
        let before = info(&e, agent)[1];
        let killed = j.herd_plugin.kill(agent, 1, PLAYER);
        let sheared = j.herd_plugin.shear(agent, 1, PLAYER);
        let led = j.herd_plugin.lead(agent, PLAYER).expect("one led away");
        assert_eq!((killed.len(), sheared.len()), (1, 1));
        j.herd_flush();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (alive, k, t, lost, dead) = summary(&e, agent);
        assert_eq!(
            (k, t, lost, dead),
            (1, 1, 0, 2),
            "a kill and a taking, each once"
        );
        assert_eq!(alive, before - 2, "the herd's count drops by the two");
        assert_eq!(e.herd_stats().0.sheared, 1, "the shearing booked");
        // The kill again under its op id (a carried record journaled again): nothing more.
        let kill = j
            .journal
            .iter()
            .find(|(_, r)| wire::u16_at(r, wire::R_KIND) == hb::HERD_KILLED)
            .map(|(_, r)| r.clone());
        if let Some(mut r) = kill {
            let seq = j.next_obs;
            j.next_obs += 1;
            wire::put_u64(&mut r, obs::OBS_SEQ, seq);
            j.journal.push_back((seq, r));
            j.idle();
        } else {
            // Already acknowledged and trimmed: send the same kill anew under the same op id.
            j.herd_obs(&O::Killed(hb::Killed {
                agent,
                epoch: m.epoch,
                member: killed[0],
                by: hb::KillerKind::Player,
                killer: PLAYER,
                op: (1 << 63) | 1,
            }));
            j.idle();
        }
        assert_eq!(summary(&e, agent).1, 1, "a kill's op id is applied once");

        // A fence: the group keeps failing to pass toward its target. The herd replans.
        let replans = e.herd_stats().0.replans;
        j.herd_obs(&O::Obstacle(hb::Obstacle {
            agent,
            epoch: m.epoch,
            failures: 20,
            present: 29,
            stuck: (AT.0 + 30, AT.1),
            target: (AT.0 + 400, AT.1),
        }));
        j.idle();
        let (st, _, _, _) = e.herd_stats();
        assert_eq!(st.obstacles, 1, "the obstacle booked");
        assert_eq!(
            st.replans,
            replans + 1,
            "and the herd replanned from where it is stuck"
        );
        assert!(
            e.with_herds(|h, _| !h.obstacles.cells.is_empty()),
            "the blocked cell costs more"
        );

        // The player leaves: the group is captured, each member with its state.
        j.players.clear();
        j.run_until("the group captured", |j| {
            j.herds
                .iter()
                .any(|(_, c)| matches!(c, H::Despawn(d) if d.agent == agent && d.epoch == m.epoch))
        });
        j.idle();
        assert_eq!(info(&e, agent)[5], 0, "no group embodied");
        let despawn = j
            .herds
            .iter()
            .find_map(|(_, c)| match c {
                H::Despawn(d) => Some(*d),
                _ => None,
            })
            .unwrap();
        assert_eq!(despawn.reason, 2, "the player left");
        save(&e, &mut j);

        // The player is back: the same sheep, the sheared one still sheared, no dead one.
        j.players = vec![(PLAYER, dim, pos)];
        j.run_until("the group again", |j| materializations(j).len() >= 2);
        j.idle();
        let m2 = materializations(&j).remove(1);
        assert!(m2.epoch > m.epoch, "a new epoch for every materialization");
        let first: std::collections::BTreeMap<u64, u32> =
            m.members.iter().map(|b| (b.member, b.variant)).collect();
        let gone = [killed[0], led];
        for b in &m2.members {
            assert!(
                !gone.contains(&b.member),
                "a dead or taken sheep never comes back"
            );
            if let Some(v) = first.get(&b.member) {
                assert_eq!(*v, b.variant, "the same sheep has the same colour");
            }
            assert_eq!(
                b.flags & MEMBER_SHEARED != 0,
                b.member == sheared[0],
                "only the sheared sheep is sheared"
            );
        }
        let again = m2
            .members
            .iter()
            .filter(|b| first.contains_key(&b.member))
            .count();
        assert_eq!(
            again, 30,
            "every sheep seen and still alive comes back first"
        );
        eprintln!(
            "herds exchange: group of {} (epoch {}), then {} (epoch {}); killed {:?}, led {led}, \
             sheared {:?}; summary {:?}; stats {:?}",
            m.members.len(),
            m.epoch,
            m2.members.len(),
            m2.epoch,
            killed,
            sheared,
            summary(&e, agent),
            e.herd_stats()
        );

        // After the save: a kill, journaled; the world saves, then the server dies.
        let k2 = j.herd_plugin.kill(agent, 1, PLAYER);
        j.herd_flush();
        j.idle();
        let before_crash = summary(&e, agent);
        assert_eq!(before_crash.1, 2);
        let saved = j.world_save();
        let op2 = saved
            .journal
            .iter()
            .rev()
            .find(|(_, r)| wire::u16_at(r, wire::R_KIND) == hb::HERD_KILLED)
            .map(|(_, r)| wire::u64_at(r, 64))
            .expect("the journal holds what the pages lack");
        let _ = j.crash();

        // The restart: the pages read back to their saved state, the replay brings the rest once.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(
            e.herds_restore_check(),
            (None, false),
            "the herds read back to their saved state hash"
        );
        let (_, replay_ok, _) = e.phase1_state();
        assert!(replay_ok, "the world they live in too");
        assert_eq!(summary(&e, agent).1, 1, "the pages predate the second kill");
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let after = summary(&e, agent);
        assert_eq!(
            (after.1, after.2, after.3, after.4),
            (
                before_crash.1,
                before_crash.2,
                before_crash.3,
                before_crash.4
            ),
            "the kill booked once, the herd as before the crash"
        );
        assert_eq!(info(&e, agent)[5], 0, "no group outlives a session");
        // The second kill again under its op id: nothing more.
        j.herd_obs(&O::Killed(hb::Killed {
            agent,
            epoch: m2.epoch,
            member: k2[0],
            by: hb::KillerKind::Player,
            killer: PLAYER,
            op: op2,
        }));
        j.idle();
        assert_eq!(summary(&e, agent).1, 2, "and its op id still applies once");
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();

        // A clean restart reads them back again.
        let e = engine(&dir, 256 * 1024);
        let _j = Java::open(e.clone(), saved);
        assert_eq!(e.herds_restore_check(), (None, false));
        assert_eq!(summary(&e, agent).1, 2);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// C25 (U10): with the ecology module switched off the herds ignore their records (an
    /// obstacle), decide nothing (a player by the herd sees no group) and keep the page they
    /// had; switched on again, the player sees the herd.
    #[test]
    fn switched_off_herds_ignore_their_records_and_keep_their_page() {
        let page = |dir: &std::path::PathBuf| {
            let mut store =
                PageStore::open(dir, rtsim_kernel::page::Durability::NoSync).expect("open");
            store.get(PageKey::new(HERDS_TABLE, 0)).expect("read")
        };
        let dir = tmpdir("herds-off");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let agent = found(&e, &mut j, 60);
        save(&e, &mut j);
        j.modules(0b1111);
        j.idle();
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        let at_switch = page(&dir).expect("a herds page");

        // Off from the start of a session: a record is ignored, a player by the herd sees
        // nothing, and the page stays as it was.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.herd_obs(&O::Obstacle(hb::Obstacle {
            agent,
            epoch: 1,
            failures: 3,
            present: 3,
            stuck: AT,
            target: (AT.0 + 300, AT.1),
        }));
        j.players = vec![(PLAYER, dim, [f64::from(AT.0), 64.0, f64::from(AT.1)])];
        for _ in 0..600 {
            j.tick();
        }
        j.idle();
        let (stats, _, ignored, written) = e.herd_stats();
        assert!(ignored >= 1, "the obstacle was ignored ({ignored})");
        assert_eq!(
            (written, stats.seen),
            (0, 0),
            "nothing seen, nothing written"
        );
        assert!(j.herds.is_empty(), "no herd command reached Java");
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_eq!(
            page(&dir),
            Some(at_switch.clone()),
            "the page kept as it was"
        );

        // On again: they run, and the player sees the herd.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.players = vec![(PLAYER, dim, [f64::from(AT.0), 64.0, f64::from(AT.1)])];
        assert_eq!(e.herds_restore_check(), (None, false));
        j.run_until("the herd in view again", |j| {
            !materializations(j).is_empty()
        });
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_ne!(
            page(&dir),
            Some(at_switch),
            "running again, the page moves on"
        );
    }
}

/// The conflict engine through the native link (docs/boundary.md §9, choices C2-1 to C2-17 in
/// docs/overnight-choices-conflictjni.md), against a fake conflict plugin
/// (`rtsim_conflict::synth::FakeConflict`) behind the fake Java side.
mod conflict {
    use super::*;
    use rtsim_conflict::boundary::{self as cb, Cmd as C, Obs as O};
    use rtsim_conflict::{Step, synth};
    use rtsim_jni::engine::{CONFLICT_TABLE, MODULE_CONFLICT};
    use rtsim_kernel::clock::TICKS_PER_DAY;
    use rtsim_kernel::page::{PageKey, PageStore};

    /// The core's four phase 1 modules and CONFLICT (bit 5).
    const MODULES: u32 = 0b1111 | MODULE_CONFLICT;
    const CAMP: u64 = 0xC0B1_0001;
    const CAMP_AT: (i32, i32) = (900, 1000);
    /// Where the players stand: inside the goblins' territory, the camp in view.
    const BASE: (i32, i32) = (880, 900);
    const PLAYER2: (u64, u64) = (0x2222_0000_0000_0002, 0x9999_0000_0000_0009);
    const DAY: i64 = TICKS_PER_DAY as i64;

    /// Moves game time on in steps of `step` ticks until `done`, journaling the fake plugin's
    /// answers at each step; fails after `limit` ticks.
    fn advance_until(
        j: &mut Java,
        step: i64,
        limit: i64,
        what: &str,
        mut done: impl FnMut(&mut Java) -> bool,
    ) {
        let end = j.game_time + limit;
        loop {
            j.conflict_flush();
            j.idle();
            if done(j) {
                return;
            }
            if j.game_time >= end {
                for l in j.e.conflict_log(true) {
                    eprintln!("  {:7.2} {:9} {}", l.day, l.kind, l.what);
                }
                for (seq, c) in &j.conflict {
                    eprintln!("  cmd {seq}: {}", c.name());
                }
            }
            assert!(
                j.game_time < end,
                "{what}: not by game time {} (day {:.2})",
                j.game_time,
                j.game_time as f64 / DAY as f64
            );
            j.game_time += step;
        }
    }

    fn actor(e: &Engine, uuid: (u64, u64)) -> Option<rtsim_knowledge::ActorId> {
        e.with_conflict(|_, w| {
            w.players
                .iter()
                .find(|(_, p)| p.uuid == uuid)
                .map(|(a, _)| *a)
        })
    }

    fn feud(e: &Engine, uuid: (u64, u64)) -> Option<rtsim_conflict::FeudView> {
        let a = actor(e, uuid)?;
        e.with_conflict(|c, _| c.feud_view(CAMP, a))
    }

    fn found(j: &mut Java, at: (i32, i32)) {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
        j.conflict_obs(&O::FactionFound(cb::FactionFound {
            faction: CAMP,
            flags: 0,
            dimension: dim,
            structure,
            pos: (at.0, 64, at.1),
        }));
    }

    fn signal(j: &mut Java, kind: u32, count: u32, player: (u64, u64)) {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.conflict_obs(&O::Signal(cb::TerritorySignal {
            faction: 0,
            kind,
            dimension: dim,
            count,
            player,
            pos: (BASE.0, 64, BASE.1),
        }));
    }

    fn cmds<'a>(j: &'a Java) -> impl Iterator<Item = &'a C> + 'a {
        j.conflict.iter().map(|(_, c)| c)
    }

    fn java_name(j: &Java, kind: u32, id: u32) -> Option<String> {
        j.dict
            .get((id as usize).wrapping_sub(1))
            .filter(|(k, _)| *k == kind)
            .map(|(_, n)| n.clone())
    }

    fn save(e: &Engine, j: &mut Java) {
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        while e.durable_obs_seq() < e.stats_array()[4] {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// A goblin camp reported (FACTION_FOUND) joins the world as a haven and its territory goes
    /// back (TERRITORY); a player's logging there (TERRITORY_SIGNAL) raises grievance against
    /// that player (P14); the envoy comes into the player's view and is paid (TRIBUTE_PAID, once
    /// by its op id); a second player's killing climbs the ladder to a refused demand, scouts, a
    /// raid and an invasion, which the raid director sends in waves on an arc around the watching
    /// players; casualties and the war chief's death (CHIEF_KILLED, once by its op id) rout it and
    /// the heir takes over. Then a save, more records and a crash: the pages read back to their
    /// state hash and the journal's tail is applied once; a clean restart reads back the same
    /// state hash.
    #[test]
    fn factions_take_notice_send_envoys_take_tribute_once_and_raid_in_waves_across_a_crash() {
        let dir = tmpdir("conflict");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let base = [f64::from(BASE.0) + 0.5, 64.0, f64::from(BASE.1) + 0.5];
        j.players = vec![(PLAYER, dim, base), (PLAYER2, dim, base)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        j.conflict_plugin.answer = synth::Answer::Pay;
        let _ = e.conflict_log(true);
        j.idle();

        // The camp: a haven node in the world, the territory back to Java.
        found(&mut j, CAMP_AT);
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (factions, haven) =
            e.with_conflict(|c, w| (c.factions.len(), w.node_of_key(CAMP).is_some()));
        assert_eq!(
            (factions, haven),
            (1, true),
            "the camp is a faction and a haven"
        );
        let territory = cmds(&j)
            .find_map(|c| match c {
                C::Territory(t) => Some(*t),
                _ => None,
            })
            .expect("TERRITORY");
        assert_eq!(
            (territory.faction, territory.dimension, territory.radius),
            (CAMP, dim, 400),
            "the goblins' territory in Java's dimension id"
        );
        assert_eq!((territory.x, territory.z), CAMP_AT);

        // A territory signal: 300 logs broken by the first player.
        signal(&mut j, 2, 300, PLAYER);
        j.idle();
        let f = feud(&e, PLAYER).expect("a feud with the logger");
        assert!(
            (f.grievance - 45.0).abs() < 0.5,
            "300 logs at 0.15 each: {f:?}"
        );
        assert!(f.step >= Step::Notice, "{f:?}");
        assert!(
            cmds(&j).any(|c| matches!(c, C::Notice(n) if n.player == PLAYER && n.dimension == dim)),
            "FACTION_NOTICE to the player"
        );

        // The envoy comes into view and is paid, once.
        advance_until(&mut j, 600, 2 * DAY, "an envoy in view", |j| {
            cmds(j).any(|c| matches!(c, C::Envoy(_)))
        });
        let envoy = cmds(&j)
            .find_map(|c| match c {
                C::Envoy(x) => Some(x.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(envoy.agent >> 56, 0x43, "a party's agent (namespace 0x43)");
        assert_eq!(
            (envoy.faction, envoy.player, envoy.dimension),
            (CAMP, PLAYER, dim)
        );
        assert_eq!(
            java_name(&j, wire::DICT_ITEM, envoy.tribute_item).as_deref(),
            Some("minecraft:emerald"),
            "the tribute in Java's item ids"
        );
        assert!(envoy.tribute_count > 0, "{envoy:?}");
        let bodies: Vec<String> = envoy
            .members
            .iter()
            .filter_map(|m| java_name(&j, wire::DICT_BODY, m.0))
            .collect();
        assert!(
            bodies.contains(&"mic_rtsim:conflict/goblin/emissary".to_string()),
            "the envoy's bodies in Java's ids: {bodies:?}"
        );
        assert!(
            envoy.deadline > j.game_time && envoy.deadline < j.game_time + 4 * DAY,
            "the deadline in game time: {} at {}",
            envoy.deadline,
            j.game_time
        );
        advance_until(&mut j, 100, DAY, "the demand settled", |j| {
            cmds(j).any(|c| matches!(c, C::Settled(s) if s.outcome == cb::SETTLED_PAID))
        });
        let paid = |e: &Engine| {
            e.with_conflict(|c, _| {
                (
                    c.stats.tributes,
                    c.ledger.tribute.values().copied().sum::<i128>(),
                )
            })
        };
        let (tributes, emeralds) = paid(&e);
        assert_eq!(tributes, 1, "one tribute");
        assert_eq!(
            emeralds,
            i128::from(envoy.tribute_count) * 1_000,
            "the haven took what was paid"
        );
        // The same TRIBUTE_PAID again (a record carried by a chunk, journaled again): nothing more.
        let tribute = j
            .journal
            .iter()
            .find(|(_, r)| wire::u16_at(r, wire::R_KIND) == cb::TRIBUTE_PAID)
            .map(|(_, r)| r.clone());
        let tribute = match tribute {
            Some(r) => r,
            None => {
                let mut b = Vec::new();
                let op = (1 << 63) | 0x7777;
                cb::encode_obs(
                    0,
                    j.game_time,
                    &O::TributePaid(cb::TributePaid {
                        faction: CAMP,
                        agent: envoy.agent,
                        epoch: envoy.epoch,
                        player: PLAYER,
                        op,
                        items: vec![cb::Item {
                            item: envoy.tribute_item,
                            count: envoy.tribute_count,
                        }],
                    }),
                    &mut b,
                );
                b
            }
        };
        let mut again = tribute.clone();
        let seq = j.next_obs;
        j.next_obs += 1;
        wire::put_u64(&mut again, obs::OBS_SEQ, seq);
        j.journal.push_back((seq, again));
        j.idle();
        assert_eq!(
            paid(&e),
            (tributes, emeralds),
            "a tribute's op id applies once"
        );
        let f = feud(&e, PLAYER).unwrap();
        assert_eq!(f.step, Step::Notice, "paid: back to notice ({f:?})");

        // The second player kills members in the territory: refused demand, scouts, a raid.
        j.conflict_plugin.answer = synth::Answer::Refuse;
        signal(&mut j, 4, 60, PLAYER2);
        j.idle();
        let f2 = feud(&e, PLAYER2).expect("a feud with the killer");
        assert!(f2.grievance > 400.0, "{f2:?}");
        advance_until(&mut j, 1_000, 3 * DAY, "the demand refused", |j| {
            cmds(j).any(|c| matches!(c, C::Settled(s) if s.player == PLAYER2))
        });
        // Nobody fights the raid: it waits at the base, gets bored and goes home.
        advance_until(&mut j, 1_000, 6 * DAY, "a war band at the base", |j| {
            cmds(j).any(|c| matches!(c, C::Materialize(m) if m.purpose == 3 && m.arc_radius > 0))
        });
        let raid = cmds(&j)
            .find_map(|c| match c {
                C::Materialize(m) if m.purpose == 3 && m.arc_radius > 0 => Some(m.clone()),
                _ => None,
            })
            .unwrap();
        eprintln!(
            "the raid at day {:.2}: {raid:?}",
            j.game_time as f64 / DAY as f64
        );
        assert_eq!(raid.arc_radius, 112, "the director's arc, 112 blocks out");
        assert!(
            (raid.arc_centre.0 - base[0]).abs() < 2.0 && (raid.arc_centre.1 - base[2]).abs() < 2.0,
            "around the watching player: {:?}",
            raid.arc_centre
        );
        assert!(raid.flags & cb::BAND_HOSTILE != 0);

        // The invasion: more than the spawn quota, so it comes in waves; the players kill four in
        // ten of each wave, then its war chief.
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.4,
            after: 200,
            kill_chief: false,
            only_at_base: true,
        });
        let waves_of = |j: &Java, agent: u64| -> Vec<cb::BandMaterialize> {
            cmds(j)
                .filter_map(|c| match c {
                    C::Materialize(m) if m.agent == agent && m.arc_radius > 0 => Some(m.clone()),
                    _ => None,
                })
                .collect()
        };
        advance_until(&mut j, 1_000, 12 * DAY, "an invasion at the base", |j| {
            cmds(j).any(|c| matches!(c, C::Materialize(m) if m.purpose == 4 && m.arc_radius > 0))
        });
        let inv = cmds(&j)
            .find_map(|c| match c {
                C::Materialize(m) if m.purpose == 4 && m.arc_radius > 0 => Some(m.clone()),
                _ => None,
            })
            .unwrap();
        advance_until(&mut j, 100, DAY / 4, "a second wave", |j| {
            waves_of(j, inv.agent).len() >= 2
        });
        let waves = waves_of(&j, inv.agent);
        let first: u32 = waves[0].members.iter().map(|s| s.count).sum();
        eprintln!(
            "the invasion at day {:.2}: {} waves, the first of {first}",
            j.game_time as f64 / DAY as f64,
            waves.len()
        );
        assert!(first <= 16, "a wave within the spawn quota: {first}");
        assert_eq!(waves[0].wave, 0);
        assert!(
            waves[1].wave >= 1 && waves[1].epoch == waves[0].epoch,
            "{waves:?}"
        );
        assert!(
            waves
                .iter()
                .all(|w| w.arc_radius == 112 && (w.half_width_deg - 30.0).abs() < 1e-3),
            "on the arc"
        );
        assert!(
            waves[0].flags & cb::BAND_CHIEF != 0,
            "the war chief leads the invasion"
        );
        let (casualties, chiefs) =
            e.with_conflict(|c, _| (c.stats.casualties, c.stats.chiefs_killed));
        assert!(casualties > 0, "casualties booked");
        let chief_before = e.with_conflict(|c, _| c.factions[0].chief);
        let ck = O::ChiefKilled(cb::ChiefKilled {
            agent: inv.agent,
            epoch: inv.epoch,
            by: cb::BY_PLAYER,
            killer: PLAYER2,
            op: (1 << 63) | 0xC41E_F000,
        });
        j.conflict_obs(&ck);
        j.idle();
        let (chiefs_now, successions, chief_now) = e.with_conflict(|c, _| {
            (
                c.stats.chiefs_killed,
                c.stats.successions,
                c.factions[0].chief,
            )
        });
        assert_eq!(chiefs_now, chiefs + 1, "the war chief killed");
        assert!(
            successions >= 1 && chief_now != chief_before,
            "the heir takes over"
        );
        assert!(
            cmds(&j).any(|c| matches!(c, C::Recall(r) if r.agent == inv.agent && r.reason == cb::RECALL_ROUTED)),
            "the band routs"
        );
        // The same death again under its op id: nothing more.
        j.conflict_obs(&ck);
        j.idle();
        assert_eq!(
            e.with_conflict(|c, _| c.stats.chiefs_killed),
            chiefs_now,
            "a chief's death applies once"
        );
        advance_until(&mut j, 200, DAY / 4, "the survivors dematerialized", |j| {
            j.conflict_plugin.embodied() == 0
        });
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (st, link) = e.conflict_stats();
        eprintln!("conflict exchange: {st:?}; link {link:?}");
        assert!(st.waves >= 2 && st.raids_watched >= 1, "{st:?}");
        assert_eq!(j.stale_commands, 0);

        // A save; then a gift (journaled), the world saves, and the server dies.
        save(&e, &mut j);
        let gifts = e.with_conflict(|c, _| c.stats.gifts);
        let emerald = j.dict_id(wire::DICT_ITEM, "minecraft:emerald");
        j.conflict_obs(&O::GiftGiven(cb::GiftGiven {
            faction: CAMP,
            player: PLAYER,
            op: (1 << 63) | 0x61F7,
            items: vec![cb::Item {
                item: emerald,
                count: 5,
            }],
        }));
        j.idle();
        assert_eq!(e.with_conflict(|c, _| c.stats.gifts), gifts + 1);
        let gt = j.game_time;
        let saved = j.world_save();
        let _ = j.crash();

        // The restart: the pages read back to their state hash, the journal's tail once.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        assert_eq!(
            e.conflict_restore_check(),
            (None, false),
            "the conflict reads back to its saved state hash"
        );
        let (_, replay_ok, _) = e.phase1_state();
        assert!(replay_ok, "the world it lives in too");
        assert_eq!(
            e.with_conflict(|c, _| c.stats.gifts),
            gifts,
            "the pages predate the gift"
        );
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(
            e.with_conflict(|c, _| (c.stats.gifts, c.stats.tributes, c.stats.chiefs_killed)),
            (gifts + 1, tributes, chiefs_now),
            "the gift applied once, the rest as before"
        );
        assert!(
            cmds(&j).any(|c| matches!(c, C::Territory(t) if t.faction == CAMP)),
            "TERRITORY again at the session start"
        );
        // A clean close and restart: the same state hash.
        let saved = j.world_save();
        let hash = e.with_conflict(|c, _| c.state_hash());
        e.close(true, Duration::from_secs(5)).unwrap();
        let e = engine(&dir, 256 * 1024);
        let _j = Java::open(e.clone(), saved);
        assert_eq!(e.conflict_restore_check(), (None, false));
        assert_eq!(
            e.with_conflict(|c, _| c.state_hash()),
            hash,
            "a restore gives the same state hash"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
        let page = {
            let mut store =
                PageStore::open(&dir, rtsim_kernel::page::Durability::NoSync).expect("open");
            store.get(PageKey::new(CONFLICT_TABLE, 0)).expect("read")
        };
        assert!(page.is_some(), "the conflict's page in table 0x4300");
    }

    /// C25 (U10, C2-6): with the conflict module switched off, the conflict ignores its records
    /// (a territory signal), decides and writes nothing, and keeps the page it had; switched on
    /// again, it runs on and the territory goes out again.
    #[test]
    fn switched_off_conflict_ignores_its_records_and_keeps_its_page() {
        let page = |dir: &std::path::PathBuf| {
            let mut store =
                PageStore::open(dir, rtsim_kernel::page::Durability::NoSync).expect("open");
            store.get(PageKey::new(CONFLICT_TABLE, 0)).expect("read")
        };
        let dir = tmpdir("conflict-off");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        found(&mut j, CAMP_AT);
        j.idle();
        signal(&mut j, 2, 100, PLAYER);
        j.idle();
        save(&e, &mut j);
        j.modules(0b1111);
        j.idle();
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        let at_switch = page(&dir).expect("a conflict page");

        // Off from the start of a session: a signal is ignored, days pass, nothing is written.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.players = vec![(PLAYER, dim, [f64::from(BASE.0), 64.0, f64::from(BASE.1)])];
        signal(&mut j, 2, 300, PLAYER);
        advance_until(&mut j, 2_000, 3 * DAY, "three days", |j| {
            j.game_time > 3 * DAY
        });
        let (_, link) = e.conflict_stats();
        assert!(link.ignored >= 1, "the signal was ignored ({link:?})");
        assert_eq!(link.commands_written, 0, "nothing written");
        assert!(j.conflict.is_empty(), "no conflict command reached Java");
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_eq!(
            page(&dir),
            Some(at_switch.clone()),
            "the page kept as it was"
        );

        // On again: it runs, and the plugin hears the territory again.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(e.conflict_restore_check(), (None, false));
        j.idle();
        assert!(
            cmds(&j)
                .any(|c| matches!(c, C::Territory(t) if t.faction == CAMP && t.dimension == dim)),
            "TERRITORY again"
        );
        signal(&mut j, 2, 100, PLAYER);
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_ne!(
            page(&dir),
            Some(at_switch),
            "running again, the page moves on"
        );
    }

    /// CONFLICT_COMMAND (order 29, TEST mode, C2-15): a command in Java's ids and game time goes
    /// out as the conflict's own, its body names in the engine's dictionary first; the conflict
    /// knows nothing of its agent, so Java's answer is ignored.
    #[test]
    fn a_conflict_command_order_goes_out_in_the_engines_dictionary() {
        let dir = tmpdir("conflict-order");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.idle();
        let warrior = 41;
        let agent = 0x43 << 56 | 0xABC;
        let cmd = C::Materialize(cb::BandMaterialize {
            agent,
            epoch: 3,
            dimension: dim,
            faction: CAMP,
            purpose: 3,
            flags: cb::BAND_HOSTILE,
            wave: 0,
            arc_centre: (10.0, 20.0),
            bearing_deg: 90.0,
            arc_radius: 112,
            half_width_deg: 30.0,
            target: (10.0, 64.0, 20.0),
            target_player: PLAYER,
            speed: 0.02,
            members: vec![cb::Stack {
                body: warrior,
                count: 5,
                flags: 0,
            }],
        });
        let mut record = Vec::new();
        cb::encode_cmd(0, &cmd, &mut record);
        e.order(vec![Order::ConflictCommand {
            names: vec![(
                wire::DICT_BODY,
                warrior,
                "mic_rtsim:conflict/goblin/warrior".into(),
            )],
            record,
        }])
        .unwrap();
        j.idle();
        let got = cmds(&j)
            .find_map(|c| match c {
                C::Materialize(m) if m.agent == agent => Some(m.clone()),
                _ => None,
            })
            .expect("the ordered BAND_MATERIALIZE");
        assert_eq!(got.dimension, dim);
        assert_eq!(
            java_name(&j, wire::DICT_BODY, got.members[0].body).as_deref(),
            Some("mic_rtsim:conflict/goblin/warrior"),
            "the body in the engine's dictionary, as Java maps it"
        );
        assert_eq!((got.arc_radius, got.members[0].count), (112, 5));
        j.conflict_flush();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (st, link) = e.conflict_stats();
        assert_eq!(st.materialized, 0, "the conflict itself decided nothing");
        assert_eq!(link.commands_written, 1);
        // Outside TEST mode the order is refused.
        let dir2 = tmpdir("conflict-order-live");
        let live = engine_with(&dir2, 256 * 1024, |c| c.test = false);
        assert!(
            live.order(vec![Order::ConflictCommand {
                names: Vec::new(),
                record: {
                    let mut r = Vec::new();
                    cb::encode_cmd(0, &cmd, &mut r);
                    r
                },
            }])
            .is_err()
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

/// The world model (docs/boundary.md §10): the core's habitat sampler reports cells, the engine
/// keeps them, founds a herd for a suitable region and lists what it holds at every session
/// start (HABITAT_KNOWN), so the bridge resends only what changed or what a crash lost.
mod worldmodel {
    use super::*;
    use rtsim_jni::engine::{HABITAT_INFO, MODULE_HERDS};
    use rtsim_space::HabitatRecord;
    use rtsim_space::habitat::KNOWN_END;
    use rtsim_space::synth::{Painter, land, sea};

    const MODULES: u32 = 0b1111 | MODULE_HERDS;

    fn save(e: &Engine, j: &mut Java) {
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        while e.durable_obs_seq() < e.stats_array()[4] {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Region (0, 0): sea west of x = `shore`, plains east of it.
    fn region(p: &mut Painter, shore: i32) -> Vec<HabitatRecord> {
        let mut out = Vec::new();
        for cz in 0..8 {
            for cx in 0..8 {
                out.push(p.record(
                    0,
                    cx,
                    cz,
                    &|x, _| {
                        if x < shore {
                            sea()
                        } else {
                            land("minecraft:plains")
                        }
                    },
                    &[],
                ));
            }
        }
        out
    }

    fn send(j: &mut Java, p: &Painter, recs: &[HabitatRecord]) {
        for r in recs {
            while !j.habitat(r, &|i| p.biome_name(i).to_string(), &|i| {
                p.body_name(i).to_string()
            }) {
                j.tick();
            }
        }
        j.idle();
    }

    fn known_end(j: &mut Java) -> bool {
        j.known.iter().any(|k| k.1.flags & KNOWN_END != 0)
    }

    fn info(e: &Engine, j: &mut Java, x: i32, z: i32) -> [i64; HABITAT_INFO] {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        e.habitat_info(dim, x, z)
    }

    #[test]
    fn habitat_cells_found_a_herd_on_land_and_are_listed_again_after_a_restart() {
        let dir = tmpdir("worldmodel");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        j.run_until("the engine's list", known_end);
        assert_eq!(j.known.len(), 1, "a fresh engine holds nothing: END alone");

        // The sampler reports a region: sea in its west quarter, plains elsewhere.
        let mut p = Painter::default();
        let recs = region(&mut p, 128);
        send(&mut j, &p, &recs);
        j.run_until("a herd founded", |j| info(&e, j, 300, 300)[9] != 0);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let i = info(&e, &mut j, 300, 300);
        assert_eq!((i[0], i[1], i[2]), (1, 0xFFFF, 0), "a known land cell");
        assert_eq!((i[7], i[8]), (64, 1), "64 cells in one region");
        let agent = i[9] as u64;
        assert_eq!(agent >> 56, 0x48, "a herd's agent");
        assert_eq!(i[10], 1, "the region is decided");
        assert_eq!(info(&e, &mut j, 10, 10)[2], 0xFFFF, "the west is sea");
        let (pos, species) = e.with_herds(|h, _| {
            let id = rtsim_ecology::herds::herd::herd_of(agent).unwrap();
            (h.position(id, h.now()), h.herd(id).unwrap().species)
        });
        assert!(pos.0 >= 128, "founded on land: {pos:?}");
        assert!(
            e.with_world_model(|w| w
                .area(
                    0,
                    pos.0 as i32,
                    pos.1 as i32,
                    pos.0 as i32 + 1,
                    pos.1 as i32 + 1
                )
                .water)
                == 0
        );
        eprintln!("world model: a herd of {species:?} founded at {pos:?}");

        // The same cells again: nothing changes, and no second herd.
        send(&mut j, &p, &recs);
        j.idle();
        let unchanged = e.with_world_model(|w| w.sampled_ref().unwrap().stats.unchanged);
        assert_eq!(unchanged, 64, "latest wins: the same cells change nothing");
        assert_eq!(e.herd_agents().len(), 1, "one herd per region");
        save(&e, &mut j);
        let save1 = j.world_save();

        // After the save: the sea floods a quarter more of the region. The server dies.
        let flooded = region(&mut p, 256);
        send(&mut j, &p, &flooded);
        j.idle();
        assert_eq!(info(&e, &mut j, 200, 10)[2], 0xFFFF, "flooded in memory");
        let _ = j.crash();

        // The restart: the pages read back to their state hash, and the engine lists what they
        // hold: the cells as saved, so the bridge sees the flooded cells differ and resends them.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), save1);
        j.module_mask = MODULES;
        assert_eq!(e.world_model_restore_check(), (None, false));
        j.run_until("the engine's list again", known_end);
        let lists: Vec<_> = j
            .known
            .iter()
            .filter(|k| k.1.flags & KNOWN_END == 0)
            .collect();
        assert_eq!(lists.len(), 1, "one region");
        let (_, k, dim) = lists[0];
        assert_eq!(
            dim.as_deref(),
            Some(OVERWORLD),
            "the dimension through the engine's dictionary"
        );
        assert_eq!((k.region, k.cells.len()), ((0, 0), 64));
        for (idx, fp) in &k.cells {
            let (cx, cz) = (i32::from(*idx) % 8, i32::from(*idx) / 8);
            let sent = recs.iter().find(|r| r.cell == (cx, cz)).unwrap();
            assert_eq!(
                *fp, sent.fingerprint,
                "the saved fingerprint of cell ({cx}, {cz})"
            );
            let flood = flooded.iter().find(|r| r.cell == (cx, cz)).unwrap();
            assert_eq!(
                *fp != flood.fingerprint,
                (2..4).contains(&cx),
                "exactly the flooded cells differ, so only they are sent again"
            );
        }
        assert_eq!(
            e.herd_agents(),
            vec![agent],
            "the herd came back, not a second one"
        );
        let i = info(&e, &mut j, 300, 300);
        assert_eq!((i[7], i[9], i[10]), (64, agent as i64, 1));
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    }
}
