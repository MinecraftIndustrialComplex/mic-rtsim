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
