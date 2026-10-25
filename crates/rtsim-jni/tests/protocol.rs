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

/// How long a test waits for its save to become durable before it gives up. A wall clock, wide
/// on purpose: the step pool and the IO lane share the machine with every other test and agent.
const DURABLE_WAIT: Duration = Duration::from_secs(60);

/// Saves at the next step and waits until the pages on disk hold every observation the engine has
/// applied (`durable_obs_seq` has caught up with `stats_array()[4]`), driving the fake Java side
/// while it waits. Panics after `DURABLE_WAIT` instead of hanging, saying what it waited for.
///
/// One order, one `idle` and a poll are not enough to wait on. The step that takes the save holds
/// the observations applied up to it; a later step can apply more (Java's answer to a command the
/// save step wrote), and then no save is coming to cover them. So each round orders a save,
/// idles, waits for that save's commit (it lands on the IO lane by itself), and goes again while
/// the pages still trail the engine. In a quiet world that is one round, as it always was.
fn save_durable(e: &Engine, j: &mut Java) {
    const APPLIED: usize = 4;
    const DURABLE: usize = 5;
    const SAVES_DONE: usize = 9;
    const SAVE_FAILURES: usize = 15;
    let deadline = Instant::now() + DURABLE_WAIT;
    let give_up = |e: &Engine, rounds: u32| -> ! {
        let s = e.stats_array();
        panic!(
            "the pages never became durable: waited {DURABLE_WAIT:?} for durable_obs_seq to reach \
             the observations the engine applied; durable_obs_seq {}, applied through {} \
             ({rounds} save rounds, {} saves done, {} failed)",
            s[DURABLE], s[APPLIED], s[SAVES_DONE], s[SAVE_FAILURES]
        );
    };
    let mut rounds = 0;
    loop {
        let before = e.stats_array();
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        rounds += 1;
        // The commit runs on the IO lane and lands (or fails) without Java.
        loop {
            let s = e.stats_array();
            if s[SAVES_DONE] != before[SAVES_DONE] || s[SAVE_FAILURES] != before[SAVE_FAILURES] {
                break;
            }
            if Instant::now() >= deadline {
                give_up(e, rounds);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let s = e.stats_array();
        if s[DURABLE] >= s[APPLIED] {
            return;
        }
        if Instant::now() >= deadline {
            give_up(e, rounds);
        }
    }
}

/// Runs Java in lockstep with the engine until `done`: one game tick a step (`Java::flush` hands
/// over, waits for that step and applies its commands), at most `max_ticks` of game time. For a
/// wait whose outcome counts game time: `Java::run_until` ticks on the wall clock, so under load
/// the engine falls behind, one step takes in hundreds of game ticks, and a clock the test races
/// (wool regrowing, a player's presence going stale) runs out between two of the engine's answers.
fn flush_until(j: &mut Java, what: &str, max_ticks: i64, mut done: impl FnMut(&mut Java) -> bool) {
    let start = j.game_time;
    while !done(j) {
        assert!(
            j.game_time - start < max_ticks,
            "timed out waiting for {what}: not within {max_ticks} game ticks in lockstep"
        );
        j.flush();
    }
}

/// `save_durable` covers what the save step's own commands bring back: an engine transfer is
/// written in the step that takes the save, Java's answer is applied one step after it, and the
/// pages must hold that answer too. (One order and one wait on `durable_obs_seq` never ends here.)
#[test]
fn a_durable_wait_covers_a_reply_applied_after_the_save_step() {
    let dir = tmpdir("durable-reply");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.idle();
    e.order(vec![transfer_order(4242, 3)]).unwrap();
    save_durable(&e, &mut j);
    let s = e.stats_array();
    assert!(
        s[4] >= 1,
        "Java's answer to the transfer was applied: {s:?}"
    );
    assert!(s[5] >= s[4], "and the pages hold it: {s:?}");
    assert!(e.inspect().op_applied(4242), "the transfer was applied");
    e.close(true, Duration::from_secs(5)).unwrap();
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
    // The world saved with its journal at 11 (the session's JOURNAL_GEN, then ten transfers);
    // ten more arrive in the same hand-over.
    let through = j.next_obs - 1;
    assert_eq!(through, 11);
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
        Some(12),
        "trimmed to the acknowledgement"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// RP-49 (fixes4 item 6): a save Java asks for while the engine lags its journal by more than
/// a save interval is taken. Each round Java journals more than one hand-over carries and asks
/// for a save through its journal's end, as every world save does, and hands over once: the
/// engine falls further behind each round, and every request after the first lies past it. The
/// pages must stop at the first request's world save once the engine reaches it. A request that
/// replaced the pending one pushed the target past the engine each round: no save was taken,
/// and the journal the next start replays grew without bound.
#[test]
fn a_save_asked_for_while_the_engine_lags_is_taken() {
    const SAVES_DONE: usize = 9;
    const ROUNDS: u64 = 4;
    let dir = tmpdir("save-lag");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.idle();
    // A hand-over of MIN_CAPACITY carries less than its bytes' worth of transfers (about
    // 1,000): two buffers' worth a round.
    let per_round = (2 * wire::MIN_CAPACITY / obs::transfer::SIZE) as u64;
    let saves = e.stats_array()[SAVES_DONE];
    let mut first = 0;
    for round in 0..ROUNDS {
        for i in 0..per_round {
            j.transfer(wire::JAVA_OP_BIT | (round << 32) | i, 1, 2, 1);
        }
        let through = j.next_obs - 1;
        if first == 0 {
            first = through;
        }
        e.request_save(through);
        j.flush();
    }
    let applied = e.stats_array()[4];
    assert!(
        applied > first && applied < j.next_obs - 1,
        "the engine passed the first request and lags the last: applied {applied}, first \
         {first}, journal end {}",
        j.next_obs - 1
    );
    let deadline = Instant::now() + DURABLE_WAIT;
    while e.durable_obs_seq() == 0 {
        assert!(
            Instant::now() < deadline,
            "no save taken: {} saves done, durable {}, applied {applied}, first request {first}",
            e.stats_array()[SAVES_DONE] - saves,
            e.durable_obs_seq()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        e.durable_obs_seq(),
        first,
        "the pages stop at the first request's world save"
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
    use std::sync::atomic::Ordering::Acquire;
    let dir = tmpdir("fail");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.idle();
    e.order(vec![Order::Fail]).unwrap();
    j.run_until("the FAILED flag", |j| j.failed_seen);
    assert!(e.protocol_error().is_some_and(|m| m.contains("panicked")));
    assert_eq!(
        e.stats.window_grew.load(Acquire),
        1,
        "the failure logs that the replay window grows (RP-49)"
    );
    // The bridge is never left waiting: steps keep publishing.
    let before = j.submitted;
    j.idle();
    assert!(j.submitted > before);
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A save whose write fails partway poisons the page store; the store reopens in place from its
/// files and the engine runs on (docs/choices-leg-purge.md LP10): the failed save is never
/// acknowledged, the next one is, and the world restarts from those pages. A torn log record
/// leaves the store at the save before; a failed cleanup, at the failed save itself.
#[test]
fn a_save_failing_partway_costs_that_save_not_the_engine() {
    use rtsim_kernel::page::Fault;
    use std::sync::atomic::Ordering::Acquire;
    for fault in [Fault::TornLog, Fault::BeforeCleanup] {
        let dir = tmpdir(&format!("poisoned-{fault:?}"));
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        for i in 0..5 {
            j.transfer(wire::JAVA_OP_BIT | (100 + i), 1, 2, 1);
        }
        j.idle();
        j.idle();
        let first = j.next_obs - 1;
        e.inject_store_fault(Some(fault));
        e.request_save(first);
        j.run_until("the failed save", |j| j.e.stats_array()[15] >= 1);
        assert_eq!(
            e.durable_obs_seq(),
            0,
            "{fault:?}: a failed save is not acknowledged"
        );
        assert!(!j.acks.contains(&first), "{fault:?}");
        for i in 0..5 {
            j.transfer(wire::JAVA_OP_BIT | (200 + i), 1, 2, 1);
        }
        j.idle();
        j.idle();
        assert!(
            e.protocol_error().is_none(),
            "{fault:?}: {:?}",
            e.protocol_error()
        );
        let world = j.world_save();
        let through = j.next_obs - 1;
        // The failed commit counts its failure (what the wait above saw) a moment before it
        // reopens the store and hands its tickets back, and a save the step builds in that
        // moment is stale (PF14, `paging::Job::stale`): refused, counted as a failure too, and
        // the save after it writes the pages. The test cannot tell which side of that moment its
        // next save falls on, so a refused save is asked for again, at most twice more (as
        // industry.rs's `industry_save_tries`).
        let mut refused = 0;
        loop {
            let failed = e.stats_array()[15];
            e.request_save(through);
            j.run_until("the next save, made or refused", |j| {
                j.e.durable_obs_seq() >= through || j.e.stats_array()[15] != failed
            });
            if e.durable_obs_seq() >= through {
                break;
            }
            refused += 1;
            assert!(
                refused < 3,
                "{fault:?}: {refused} saves after the failed one refused too (failed saves {})",
                e.stats_array()[15]
            );
        }
        j.run_until("its acknowledgement", |j| j.acks.contains(&through));
        assert_eq!(
            e.stats_array()[15],
            1 + refused,
            "{fault:?}: one failure, and {refused} stale save(s) refused after it"
        );
        assert_eq!(
            e.stats.window_grew.load(Acquire),
            1 + refused,
            "{fault:?}: each save asked for and not made logs that the replay window grows (RP-49)"
        );
        assert!(
            e.protocol_error().is_none(),
            "{fault:?}: {:?}",
            e.protocol_error()
        );
        e.close(false, Duration::from_secs(5)).unwrap();
        let e2 = engine(&dir, wire::MIN_CAPACITY);
        let mut j2 = Java::open(e2.clone(), world);
        j2.idle();
        assert!(
            e2.protocol_error().is_none(),
            "{fault:?}: {:?}",
            e2.protocol_error()
        );
        assert_eq!(
            e2.durable_obs_seq(),
            through,
            "{fault:?}: the pages restart at the save"
        );
        e2.close(true, Duration::from_secs(5)).unwrap();
    }
}

// ---- phase 1 (docs/boundary.md §4) ----------------------------------------------------------------

const EMERALD: &str = "minecraft:emerald";

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
    save_durable(&e, &mut j);
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

// ---- the works purse (SINK.md) ---------------------------------------------------------------------

/// One work kind of a test catalogue, with one variant.
struct TestWork<'a> {
    id: &'a str,
    /// 0 lamp, 1 decor, 2 golem.
    group: u32,
    culture: &'a str,
    /// The village structure it belongs to, "" for every village of the culture.
    style: &'a str,
    /// (cap kind, n): 1 per residents, 2 fixed.
    cap: (u32, u32),
    /// A template (with its processor list, "" none) or a body.
    template: Option<(&'a str, &'a str)>,
    body: Option<&'a str>,
    blocks: u32,
    items: &'a [(&'a str, u32)],
}

/// The works' records as Java writes them (a trait: the fake Java is rtsim-host's).
trait Works {
    fn work_catalog(&mut self, kinds: &[TestWork<'_>]);
    fn census(&mut self, village: u64, residents: u32, golems: u32);
}

impl Works for Java {
    /// A WORK_CATALOG record (latest wins) in the open buffer, after the DICTs it uses.
    fn work_catalog(&mut self, kinds: &[TestWork<'_>]) {
        use obs::work_catalog as k;
        let mut ids = Vec::new();
        let mut size = k::DEFS;
        for w in kinds {
            let style = if w.style.is_empty() {
                0
            } else {
                self.dict_id(wire::DICT_STRUCTURE, w.style)
            };
            let (what, processors) = match (w.template, w.body) {
                (Some((t, p)), _) => (
                    self.dict_id(wire::DICT_TEMPLATE, t),
                    if p.is_empty() {
                        0
                    } else {
                        self.dict_id(wire::DICT_PROCESSORS, p)
                    },
                ),
                (None, Some(b)) => (self.dict_id(wire::DICT_BODY, b), 0),
                (None, None) => panic!("a work places something"),
            };
            let items: Vec<(u32, u32)> = w
                .items
                .iter()
                .map(|(i, n)| (self.dict_id(wire::DICT_ITEM, i), *n))
                .collect();
            ids.push((
                self.dict_id(wire::DICT_WORK, w.id),
                self.dict_id(wire::DICT_CULTURE, w.culture),
                style,
                what,
                processors,
                items,
            ));
            size += k::DEF + k::VARIANT + w.items.len() * k::ITEM;
        }
        assert!(self.preamble(), "room for the dictionary");
        let r = self
            .reserve(obs::WORK_CATALOG, size, kinds.len() as u32)
            .expect("room for WORK_CATALOG");
        wire::put_u32(r, k::N, kinds.len() as u32);
        let mut at = k::DEFS;
        for (w, (id, culture, style, what, processors, items)) in kinds.iter().zip(ids) {
            wire::put_u32(r, at + k::D_ID, id);
            wire::put_u32(r, at + k::D_GROUP, w.group);
            wire::put_u32(r, at + k::D_CULTURE, culture);
            wire::put_u32(r, at + k::D_STYLE, style);
            wire::put_u32(r, at + k::D_CAP, w.cap.0);
            wire::put_u32(r, at + k::D_CAP_N, w.cap.1);
            wire::put_u32(r, at + k::D_VARIANTS, 1);
            at += k::DEF;
            let place = if w.template.is_some() {
                k::PLACE_TEMPLATE
            } else {
                k::PLACE_BODY
            };
            wire::put_u32(r, at + k::V_PLACE, place);
            wire::put_u32(r, at + k::V_WHAT, what);
            wire::put_u32(r, at + k::V_PROCESSORS, processors);
            wire::put_u32(r, at + k::V_BLOCKS, w.blocks);
            wire::put_u32(r, at + k::V_ITEMS, items.len() as u32);
            at += k::VARIANT;
            for (item, n) in items {
                wire::put_u32(r, at, item);
                wire::put_u32(r, at + 4, n);
                at += k::ITEM;
            }
        }
    }

    /// A VILLAGE_CENSUS (latest wins) of `residents` and `golems`.
    fn census(&mut self, village: u64, residents: u32, golems: u32) {
        use obs::village_census as k;
        let dim = self.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        assert!(self.preamble(), "room for the dictionary");
        let r = self
            .reserve(obs::VILLAGE_CENSUS, k::ENTRIES, 0)
            .expect("room for VILLAGE_CENSUS");
        wire::put_u64(r, k::VILLAGE, village);
        wire::put_u32(r, k::DIM, dim);
        wire::put_u32(r, k::RESIDENTS, residents);
        wire::put_u32(r, k::GOLEMS, golems);
    }
}

const PLAINS_LAMP: &str = "minecraft:village/plains/plains_lamp_1";
const IRON_GOLEM: &str = "minecraft:iron_golem";
/// `phase1::stat` indexes: works bought, placed, refused.
const P1_WORKS_BOUGHT: usize = 16;
const P1_WORKS_PLACED: usize = 17;
const P1_WORKS_REFUSED: usize = 18;

fn test_works() -> [TestWork<'static>; 2] {
    [
        TestWork {
            id: "mic_rtsim:plains_lamp",
            group: 0,
            culture: "human",
            style: "minecraft:village_plains",
            cap: (1, 3),
            template: Some((PLAINS_LAMP, "")),
            body: None,
            blocks: 13,
            items: &[
                ("minecraft:oak_fence", 4),
                ("minecraft:white_wool", 4),
                ("minecraft:torch", 4),
                ("minecraft:cobblestone", 1),
            ],
        },
        TestWork {
            id: "mic_rtsim:iron_golem",
            group: 2,
            culture: "human",
            style: "",
            cap: (1, 5),
            template: None,
            body: Some(IRON_GOLEM),
            blocks: 1,
            items: &[
                ("minecraft:iron_ingot", 36),
                ("minecraft:carved_pumpkin", 1),
            ],
        },
    ]
}

/// The PLACE_HOUSE and SPAWN_RESIDENT commands Java applied for `village`.
fn works_sent(j: &Java, village: u64) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let houses = j
        .p1_of(cmd::PLACE_HOUSE)
        .into_iter()
        .filter(|h| wire::u64_at(h, cmd::place_house::VILLAGE) == village)
        .collect();
    let bodies = j
        .p1_of(cmd::SPAWN_RESIDENT)
        .into_iter()
        .filter(|h| wire::u64_at(h, cmd::spawn_resident::VILLAGE) == village)
        .collect();
    (houses, bodies)
}

/// The works purse over the real boundary (SINK.md): the catalogue from WORK_CATALOG, a granted
/// purse buying the village type's own lamps up to the cap (PLACE_HOUSE with SEARCH from the
/// meeting point, no resident queued for them), a raid buying golems first (SPAWN_RESIDENT
/// AT_VILLAGE), the census's golems, a village with no free plot marking the kind full with the
/// coin spent (MS-11), and after a crash the pages' input log rebuilding the same world.
#[test]
fn phase1_works_are_bought_placed_refused_and_replayed_across_a_crash() {
    let dir = tmpdir("works");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = 0b1111;
    let (a, b) = (0x0001_0000_0000_0001u64, 0x0001_0000_0000_0002);
    j.adopt(a, "minecraft:village_plains", "human", 90, 0, 0);
    j.adopt(b, "minecraft:village_plains", "human", 90, 400, 0);
    j.idle();
    j.idle();
    j.work_catalog(&test_works());
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(
        p1_stat(&e, P1_WORKS_BOUGHT),
        0,
        "a young village buys nothing"
    );

    // No free plot near B: its first lamp is refused, the kind is full, the coin stays spent.
    j.no_free_plot = true;
    e.order(vec![Order::WorksGrant {
        village: b,
        amount: 5_000_000,
    }])
    .unwrap();
    j.idle();
    j.idle();
    let (houses, _) = works_sent(&j, b);
    assert_eq!(houses.len(), 1, "one lamp tried, then the kind is full");
    let vb = e.phase1_works(b).expect("B's works");
    assert_eq!((vb[0], vb[1]), (1, 1), "one bought, one kind full: {vb:?}");
    assert!(
        vb[5] > 0 && vb[2] == 5_000_000 - vb[5],
        "spent, not refunded: {vb:?}"
    );
    assert_eq!(p1_stat(&e, P1_WORKS_REFUSED), 1);
    j.no_free_plot = false;

    // A's purse buys its own type's lamps, up to one per 3 residents (30 from 90 pieces).
    e.order(vec![Order::WorksGrant {
        village: a,
        amount: 5_000_000,
    }])
    .unwrap();
    // One work in flight (MSC13): each lamp is bought once the last one is answered.
    j.run_until("ten lamps", |j| works_sent(j, a).0.len() >= 10);
    j.idle();
    j.idle();
    let (houses, bodies) = works_sent(&j, a);
    eprintln!("A: {} lamps, view {:?}", houses.len(), e.phase1_works(a));
    assert_eq!(houses.len(), 10, "lamps to the cap");
    for h in &houses {
        use cmd::place_house as k;
        assert_eq!(
            j.cmd_name(wire::DICT_TEMPLATE, wire::u32_at(h, k::TEMPLATE)),
            PLAINS_LAMP
        );
        assert_eq!(
            wire::u16_at(h, k::FLAGS),
            k::FLAG_SEARCH | k::FLAG_Y_FROM_VILLAGE
        );
        assert_eq!(wire::u32_at(h, k::PROCESSORS), 0, "no processor list");
    }
    assert!(
        bodies.is_empty(),
        "no golem without a threat, no resident for a lamp"
    );
    assert_eq!(p1_stat(&e, P1_WORKS_PLACED), 10);

    // A raid: golems first, at the meeting point.
    e.order(vec![Order::WorksRaid {
        village: a,
        tribute: false,
    }])
    .unwrap();
    j.run_until("six golems", |j| works_sent(j, a).1.len() >= 6);
    j.idle();
    j.idle();
    let (_, bodies) = works_sent(&j, a);
    assert_eq!(bodies.len(), 6, "golems to one per 5 residents");
    for g in &bodies {
        use cmd::spawn_resident as k;
        assert_eq!(
            j.cmd_name(wire::DICT_BODY, wire::u32_at(g, k::BODY)),
            IRON_GOLEM
        );
        assert_eq!(wire::u32_at(g, k::FLAGS), k::FLAG_AT_VILLAGE);
    }
    j.census(a, 30, 6);
    j.idle();
    let va = e.phase1_works(a).expect("A's works");
    assert_eq!((va[0], va[1], va[3]), (16, 0, 6), "{va:?}");

    // More coin for B buys nothing: its only kind is full.
    e.order(vec![Order::WorksGrant {
        village: b,
        amount: 1_000_000,
    }])
    .unwrap();
    j.idle();
    j.idle();
    assert_eq!(
        works_sent(&j, b).0.len(),
        1,
        "a full kind is never tried again"
    );
    assert_eq!(p1_stat(&e, P1_WORKS_BOUGHT), 17);

    // A save, then a crash: the same world, works and all.
    save_durable(&e, &mut j);
    let saved = j.world_save();
    let _ = j.crash();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = 0b1111;
    let (_, replay_ok, _) = e.phase1_state();
    assert!(replay_ok, "the replayed world is the saved one");
    let back = e.phase1_works(a).expect("A's works after the crash");
    let keep = |v: [i64; 6]| [v[0], v[1], v[3], v[4], v[5]];
    assert_eq!(keep(back), keep(va), "bought, full, golems, tribute, spent");
    assert_eq!(e.phase1_works(b).map(|v| (v[0], v[1])), Some((1, 1)));
    j.idle();
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
    save_durable(&e, &mut j);
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

// ---- TALK (docs/boundary.md §13) -------------------------------------------------------------------

mod talk {
    use super::*;
    use rtsim_jni::talk_wire::{Talked, talked};

    const RIBBIT_VILLAGE: u64 = 0x0001_0000_0000_00B1;

    /// A session with one ribbit village adopted.
    fn session(dir: &std::path::PathBuf, save: WorldSave) -> (std::sync::Arc<Engine>, Java) {
        let e = engine(dir, 256 * 1024);
        let fresh = save.journal.is_empty() && save.next_obs <= 1;
        let mut j = Java::open(e.clone(), save);
        j.module_mask = 0b1111; // knowledge, settlements, trade, travel
        if fresh {
            j.adopt(RIBBIT_VILLAGE, "ribbits:village", "ribbit", 12, 0, 0);
        }
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        (e, j)
    }

    /// The value the village's latest SENTIMENT command lists for the player, if it lists them.
    fn pushed_feeling(j: &Java) -> Option<i32> {
        use cmd::sentiment as k;
        let rec = j
            .p1_of(cmd::SENTIMENT)
            .into_iter()
            .rfind(|b| wire::u64_at(b, k::HOLDER) == RIBBIT_VILLAGE)?;
        (0..wire::u32_at(&rec, wire::R_AUX) as usize)
            .map(|i| k::ENTRIES + i * k::ENTRY)
            .find(|&e| (wire::u64_at(&rec, e), wire::u64_at(&rec, e + 8)) == PLAYER)
            .map(|e| wire::i32_at(&rec, e + 16))
    }

    /// A talk credits its village once: a TALKED reported twice under one op id (a double report)
    /// is applied once, one the pages include is skipped on replay, one they lack is applied once on
    /// replay, and the sentiment Java's ribbits read (the SENTIMENT command) and the answer to a
    /// TALK_ASK both say so.
    #[test]
    fn a_talk_credits_its_village_once_live_and_across_a_crash() {
        let dir = tmpdir("talked");
        let (e, mut j) = session(&dir, WorldSave::fresh());
        assert_eq!(p1_stat(&e, 0), 1, "the ribbit village joined");
        assert_eq!(j.talk_sentiment(1, RIBBIT_VILLAGE, PLAYER), Some(0));
        assert_eq!(pushed_feeling(&j), None);

        // Live: one talk, reported twice under the same op id.
        let op1 = wire::JAVA_OP_BIT | 0x7a1;
        j.talked(RIBBIT_VILLAGE, PLAYER, op1);
        j.talked(RIBBIT_VILLAGE, PLAYER, op1);
        j.idle();
        assert_eq!(e.talk_stats().talked, 1, "the same op id applied once");
        assert_eq!(e.talk_stats().talked_refused, 0);
        assert_eq!(j.talk_sentiment(2, RIBBIT_VILLAGE, PLAYER), Some(3));
        assert_eq!(
            pushed_feeling(&j),
            Some(3),
            "the village's feeling went to Java for its ribbits to read"
        );

        // Pages that include the first talk.
        save_durable(&e, &mut j);

        // After that save: a talk with another ribbit (a new op id), reported twice.
        let op2 = wire::JAVA_OP_BIT | 0x7a2;
        j.talked(RIBBIT_VILLAGE, PLAYER, op2);
        j.talked(RIBBIT_VILLAGE, PLAYER, op2);
        j.idle();
        assert_eq!(e.talk_stats().talked, 2);
        assert_eq!(j.talk_sentiment(3, RIBBIT_VILLAGE, PLAYER), Some(6));
        let saved = j.world_save();
        let held = saved
            .journal
            .iter()
            .filter(|(_, r)| wire::u16_at(r, wire::R_KIND) == talk_kind())
            .count();
        assert!(
            held >= 2,
            "the journal still holds the talks the pages lack ({held})"
        );
        let _ = j.crash();

        // After the crash the journal replays: the first talk is in the pages (skipped by
        // sequence), the second is applied once and its repeat is a duplicate op.
        let (e, mut j) = session(&dir, saved);
        assert_eq!(
            e.talk_stats().talked,
            1,
            "only the talk the pages lack was applied again, once"
        );
        assert_eq!(
            j.talk_sentiment(1, RIBBIT_VILLAGE, PLAYER),
            Some(6),
            "+3 from the pages and +3 from the replay, not more"
        );

        // And the same op id once more, from a Java that does not know it was applied.
        j.talked(RIBBIT_VILLAGE, PLAYER, op2);
        j.idle();
        assert_eq!(j.talk_sentiment(2, RIBBIT_VILLAGE, PLAYER), Some(6));
        assert_eq!(e.talk_stats().talked, 1);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    fn talk_kind() -> u16 {
        rtsim_jni::talk_wire::TALKED
    }

    /// A TALKED the engine cannot use is refused and counted, never a protocol error, and the
    /// ones after it still apply.
    #[test]
    fn a_malformed_talked_is_refused_and_counted_and_the_next_one_still_applies() {
        let dir = tmpdir("talked-bad");
        let (e, mut j) = session(&dir, WorldSave::fresh());
        let good = Talked {
            obs_seq: 0,
            game_time: 0,
            op_id: wire::JAVA_OP_BIT | 0x7b1,
            village: RIBBIT_VILLAGE,
            player: PLAYER,
            body: 0,
        };
        // Truncated by one 8-byte word, and one word too long.
        let mut short = good.encode();
        short.truncate(talked::SIZE - 8);
        wire::put_u16(&mut short, wire::R_SIZE, (talked::SIZE - 8) as u16);
        j.journal_raw(&short);
        let mut long = good.encode();
        long.extend_from_slice(&[0; 8]);
        wire::put_u16(&mut long, wire::R_SIZE, (talked::SIZE + 8) as u16);
        j.journal_raw(&long);
        // A village nobody adopted, and nobody as the player.
        // (Each with an op id of its own: a record with an op id seen before is a duplicate, which
        // is skipped before it can be refused.)
        j.journal_raw(
            &Talked {
                village: 0xDEAD,
                op_id: good.op_id + 1,
                ..good
            }
            .encode(),
        );
        let mut nil = Talked {
            op_id: good.op_id + 2,
            ..good
        }
        .encode();
        wire::put_u64(&mut nil, talked::PLAYER_MOST, 0);
        wire::put_u64(&mut nil, talked::PLAYER_LEAST, 0);
        j.journal_raw(&nil);
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let st = e.talk_stats();
        assert_eq!((st.talked, st.talked_refused), (0, 4));
        assert_eq!(j.talk_sentiment(1, RIBBIT_VILLAGE, PLAYER), Some(0));
        assert_eq!(pushed_feeling(&j), None);

        j.journal_raw(&good.encode());
        j.idle();
        assert_eq!(e.talk_stats().talked, 1);
        assert_eq!(j.talk_sentiment(2, RIBBIT_VILLAGE, PLAYER), Some(3));
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

// ---- dragons (docs/boundary.md §6) ----------------------------------------------------------------

mod dragons {
    use super::*;
    use std::collections::BTreeSet;

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
    /// a player at the lair materializing it again, a natural fire spawn discarded while field
    /// flights run; then a save, a hoard robbery and a spawn-egg dragon after it, and a crash:
    /// the pages read back to the saved state hash, and the journal's replay books the robbery
    /// once and adopts the egg dragon once.
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

        // A natural fire dragon (In Control's): with field flights on, the tunables' default
        // until an OIL_CONFIG, it is discarded, since the flights replace it (OGB-6, OGC66); it
        // is never adopted and never joins the sim.
        j.dragon_obs(&untagged((0xD1, 0x2), SpawnKind::Natural, 0, dim));
        j.idle();
        j.idle();
        let answers: Vec<_> = j
            .iaf
            .iter()
            .filter_map(|(_, c)| match c {
                D::Discard(x) if x.entity == (0xD1, 0x2) => Some(("discard", x.reason)),
                D::Adopt(x) if x.entity == (0xD1, 0x2) => Some(("adopt", 0)),
                _ => None,
            })
            .collect();
        assert_eq!(answers, vec![("discard", 1)], "the natural dragon is discarded");
        assert_eq!(summary(&e).0, 1, "and never joins the sim");

        // A save: pages with all of that.
        save_durable(&e, &mut j);

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

    /// The transfers Java applied, by op id.
    fn transfer_ops(j: &Java) -> BTreeSet<u64> {
        j.commands
            .iter()
            .filter(|(_, kind, _)| *kind == cmd::TRANSFER_TO_CONTAINER)
            .map(|(_, _, op)| *op)
            .collect()
    }

    /// Dragon tribute over the real boundary (SINK.md §4.5, §4.6): TRIBUTE_START makes a granted
    /// village the lair's tributary and it pays at once, but the coin waits for the lair's chest;
    /// LAIR_CHEST sends it as TRANSFER_TO_CONTAINER stacks; a FAILED stack comes back and waits
    /// for the chest to be reported again; stacks Java never answered are in the outbox, so after
    /// a crash they go again under the same op ids and land once. The village's risk counts its
    /// golems' fear.
    #[test]
    fn tribute_lands_in_the_lairs_chest_once_across_a_failure_and_a_crash() {
        let dir = tmpdir("tribute");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let village = 0x0001_0000_0000_0001u64;
        j.adopt(village, "minecraft:village_plains", "human", 90, 2900, 3000);
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
        j.idle();
        j.idle();
        j.work_catalog(&test_works());
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let risk = e.dragons_village_risk(village);
        assert!(risk >= 0, "a known village has a risk: {risk}");
        assert_eq!(e.dragons_village_risk(0xDEAD), -1, "an unknown village");

        e.order(vec![
            Order::WorksGrant {
                village,
                amount: 2_000_000,
            },
            Order::TributeStart {
                village,
                lair: LAIR,
            },
        ])
        .unwrap();
        j.idle();
        j.idle();
        let [paid, placed, coins, placing] = e.dragons_tribute();
        assert_eq!(paid, 1, "the raid's tribute is paid at once");
        assert!(coins > 64, "more than a stack waits for the chest: {coins}");
        assert_eq!((placed, placing), (0, 0), "no chest yet");
        assert!(transfer_ops(&j).is_empty(), "nothing sent without a chest");
        let tribute = e.phase1_works(village).expect("works")[4];
        assert_eq!(
            tribute,
            coins * 1000,
            "the village's books: its tribute paid"
        );

        // The plugin finds the chest, but the chunk is gone again: every stack fails and waits.
        let chest = [HOME.0 + 2, HOME.1 - 4, HOME.2 + 1];
        let lair_chest = O::LairChest(db::LairChest {
            lair: LAIR,
            dimension: dim,
            pos: (chest[0], chest[1], chest[2]),
        });
        j.fail_transfers = true;
        j.dragon_obs(&lair_chest);
        j.idle();
        j.idle();
        let failed = transfer_ops(&j);
        let stacks = (coins + 63) / 64;
        assert_eq!(failed.len() as i64, stacks, "whole stacks of at most 64");
        assert_eq!(e.dragons_tribute(), [1, 0, coins, 0], "all back, waiting");
        assert!(j.containers.is_empty());
        let has_chest = |e: &Engine| {
            e.with_dragons(|d, _| d.lair_of_key(LAIR).and_then(|l| d.chest(l)).is_some())
        };
        assert!(!has_chest(&e), "a failed stack forgets the chest");
        assert!(
            j.cmd_dict
                .iter()
                .any(|((k, _), n)| *k == wire::DICT_ITEM && n == "minecraft:emerald"),
            "the coin is the village's emeralds"
        );

        // Reported again, while Minecraft holds the commands (a world save that is lost): the
        // stacks are new ops, in the outbox at the save.
        j.fail_transfers = false;
        j.hold_commands = true;
        j.dragon_obs(&lair_chest);
        j.idle();
        j.idle();
        let held: BTreeSet<u64> = j.held_ops.iter().copied().collect();
        assert_eq!(held.len() as i64, stacks);
        assert!(
            held.is_disjoint(&failed),
            "a failed stack goes again as a new op"
        );
        assert_eq!(e.dragons_tribute(), [1, 0, 0, stacks]);
        save_durable(&e, &mut j);
        let saved = j.world_save();
        let _ = j.crash();

        // After the crash the outbox sends them again, under the same op ids, and they land.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(e.dragons_restore_check(), (None, false));
        assert_eq!(e.dragons_tribute(), [1, 0, 0, stacks]);
        j.idle();
        j.idle();
        assert_eq!(transfer_ops(&j), held, "the held stacks, by their op ids");
        assert_eq!(e.dragons_tribute(), [1, coins, 0, 0], "every coin placed");
        assert_eq!(j.containers.get(&chest).copied(), Some(coins as i32));
        let (ledger, world) = e.with_dragons(|d, w| {
            (
                d.ledger.tribute.values().sum::<i128>(),
                w.works_totals().1.iter().map(|(_, a)| *a).sum::<i128>(),
            )
        });
        assert_eq!(
            ledger,
            i128::from(coins) * 1000,
            "the books hold every coin"
        );
        assert_eq!(world, ledger, "the world's tribute is the dragons'");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The oil gate through the native engine and the fake plugin (docs/boundary.md §6.8): the
    /// plugin's switches and an oil field open the gate; a player near the field draws a field
    /// flight (a hostile fire dragon on a full reserve, flags 8 and 16) and, the field having no
    /// fire lair in reach, one PLACE_LAIR in Java's dimension id; the body's breaths drain its
    /// reserve; and across a crash the journal's replay books the breaths after the save once and
    /// asks for no second lair.
    #[test]
    fn the_oil_gate_draws_a_field_flight_places_a_lair_and_replays_breaths_once() {
        let dir = tmpdir("dragons-oil");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.dragon_obs(&O::OilConfig(db::OilConfig {
            flags: db::OIL_FIELD_FLIGHTS | db::OIL_PLACE_LAIRS,
        }));
        // A 3 x 3 field at chunk (200, 200): blocks 3200 to 3247.
        let chunks: Vec<db::OilChunkRec> = (0..9)
            .map(|i| db::OilChunkRec {
                cx: 200 + i % 3,
                cz: 200 + i / 3,
                amount: 40_000,
                flags: 0,
            })
            .collect();
        j.dragon_obs(&O::OilChunks(db::OilChunks {
            dimension: dim,
            source: db::OIL_SOURCE_CDG,
            chunks,
        }));
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (s, _, _) = e.dragon_stats();
        assert_eq!((s.oil_chunks, s.fields), (9, 1), "{s:?}");

        // A player on the field, above y 15: the field flights roll and draw.
        j.players = vec![(PLAYER, dim, [3224.0, 70.0, 3224.0])];
        let drawn = |j: &Java| {
            j.iaf.iter().find_map(|(_, c)| match c {
                D::Materialize(m) if m.flags & db::MAT_OIL != 0 => Some(*m),
                _ => None,
            })
        };
        // In lockstep. A roll draws only while the player's last report is fresh (within
        // `presence_fresh_ticks`, 200), and a step runs the dragons to its time before it reads
        // the step's players. A step that takes in more than 200 game ticks (Java ticking ahead
        // of a loaded engine) finds a roll late in it stale: the rolls stop, start again from the
        // next report with a new wait, and the 12,000 ticks ran out under load.
        let start = j.game_time;
        while drawn(&j).is_none() {
            assert!(
                j.game_time - start < 12_000,
                "no field flight drawn: {:?}",
                e.dragon_stats()
            );
            j.flush();
        }
        j.idle();
        let m = drawn(&j).unwrap();
        assert_eq!(m.species, Species::Fire);
        assert!(
            m.flags & db::MAT_HOSTILE != 0,
            "a visitor is hostile: {m:?}"
        );
        assert_eq!(m.target_player, PLAYER, "{m:?}");
        let capacity = e.with_dragons(|d, _| d.tun.oil.capacity);
        assert_eq!(m.oil, capacity, "a drawn dragon arrives full");
        assert_eq!(m.dimension, dim, "in Java's dimension id");
        let places: Vec<db::PlaceLair> = j
            .iaf
            .iter()
            .filter_map(|(_, c)| match c {
                D::PlaceLair(p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(places.len(), 1, "one lair placed for the field: {places:?}");
        let p = places[0];
        assert_eq!(
            (p.dimension, p.species, p.kind),
            (dim, Species::Fire, LairKind::Roost)
        );
        assert!(
            (3150..3300).contains(&p.x) && (3150..3300).contains(&p.z),
            "near the field: {p:?}"
        );
        let (s, _, _) = e.dragon_stats();
        assert!(s.field_draws >= 1 && s.lairs_placed == 1, "{s:?}");

        // Its breaths drain the reserve (one point each).
        let level = |e: &Engine| {
            e.with_dragons(|d, _| {
                let h = rtsim_ecology::dragon::dragon_of(m.agent)?;
                d.dragons.get(h).map(|x| x.oil_level(&d.tun.oil, d.now()))
            })
        };
        let full = level(&e).expect("the drawn dragon is in the sim");
        j.dragon_obs(&O::Breaths(db::Breaths {
            agent: m.agent,
            epoch: m.epoch,
            count: 30,
        }));
        j.idle();
        let after = level(&e).unwrap();
        assert!(
            (full - 31..=full - 30).contains(&after),
            "30 breaths: {full} to {after}"
        );

        // A save, then 20 more breaths journaled after it, and a crash.
        save_durable(&e, &mut j);
        j.dragon_obs(&O::Breaths(db::Breaths {
            agent: m.agent,
            epoch: m.epoch,
            count: 20,
        }));
        j.idle();
        let before = level(&e).unwrap();
        assert!(before <= after - 20, "{after} to {before}");
        assert_eq!(e.dragon_stats().0.visitors_left, 0);
        let breaths = e.dragon_stats().0.breaths;
        assert_eq!(breaths, 50);
        let saved = j.world_save();
        let _ = j.crash();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(
            e.dragons_restore_check(),
            (None, false),
            "the dragons read back to their saved state hash"
        );
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        // The breaths after the save are booked once (a second replay would make 70), and the
        // body the crash took ended the visit: a visitor carrying nothing leaves the sim (OGC11).
        let (s, _, _) = e.dragon_stats();
        assert_eq!(
            s.breaths, breaths,
            "the breaths after the save, once: {s:?}"
        );
        assert!(level(&e).is_none() && s.visitors_left >= 1, "{s:?}");
        let (s, _, _) = e.dragon_stats();
        assert_eq!(s.lairs_placed, 1, "the lair is not asked for again: {s:?}");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The far view's FAR_LEG (docs/boundary.md §12, DHT27): every dragon at a session start,
    /// then one record whenever what it does changes, named in the session's dictionary and timed
    /// in game time.
    #[test]
    fn far_legs_follow_a_dragon_from_its_body_to_its_lair() {
        use rtsim_ecology::far::{FAR_KIND_DRAGON, FarState};
        let dir = tmpdir("dragons-far");
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
        let last = |j: &Java| *j.far.iter().rev().find(|r| r.agent == agent).expect("a far leg");
        let body = last(&j);
        assert_eq!(body.state, FarState::Body, "an adopted dragon is a body in the game");
        assert_eq!(body.kind, FAR_KIND_DRAGON);
        assert_eq!(
            j.cmd_dict.get(&(wire::DICT_BODY, body.body)).map(String::as_str),
            Some("iceandfire:fire_dragon"),
            "its species in the engine's dictionary"
        );
        assert_eq!(
            j.cmd_dict.get(&(wire::DICT_DIMENSION, body.dimension)).map(String::as_str),
            Some(OVERWORLD)
        );
        assert!((1..=5).contains(&body.stage));
        assert_eq!(body.age_days, 110, "its age in whole days, as the body reported it (DH-7)");

        // Captured, it is abstract again: at its lair, or on a leg in game time.
        let seen = j.far.len();
        j.dragon_obs(&captured(agent, epoch, dim, 210.0));
        j.idle();
        let after = last(&j);
        assert!(j.far.len() > seen, "a change sends a record");
        assert_ne!(after.state, FarState::Body, "{after:?}");
        assert_eq!(after.age_days, 110, "the body's age, a day not yet passed");
        assert!(
            after.t0 <= j.game_time && after.t0 >= j.game_time - 200,
            "times are game time: {} at {}",
            after.t0,
            j.game_time
        );
        if after.state == FarState::Roost {
            assert_eq!((after.x0, after.z0), (HOME.0, HOME.2), "it rests at its lair");
        }

        // Quiet steps send nothing.
        let quiet = j.far.len();
        j.idle();
        j.idle();
        assert_eq!(j.far.len(), quiet, "no change, no record");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
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

    /// A lair's own dragon, adopted (embodied) and saved, then killed by a player. The session
    /// ends in a crash (the journal holds the death the pages lack, and the next session replays
    /// it) or, for the control, in a clean close (the pages hold it). The dragons after the
    /// restart and its replay.
    fn dragons_after_a_death(tag: &str, crash: bool) -> usize {
        let dir = tmpdir(tag);
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
        let first = adoptions(&j);
        assert_eq!(first.len(), 1, "the lair's own dragon is adopted");
        let (agent, epoch) = (first[0].agent, first[0].epoch);
        save_durable(&e, &mut j);
        assert_eq!(summary(&e).0, 1, "one dragon at the save");

        j.dragon_obs(&O::Died(db::Died {
            agent,
            epoch,
            killer_kind: AttackerKind::Player,
            killer: PLAYER,
            dimension: dim,
            pos: (f64::from(HOME.0), f64::from(HOME.1), f64::from(HOME.2)),
        }));
        j.idle();
        j.idle();
        assert_eq!(
            summary(&e).0,
            0,
            "the session that went on: the dragon is dead"
        );
        let saved = if crash {
            let saved = j.crash();
            assert!(
                saved
                    .journal
                    .iter()
                    .any(|(_, r)| wire::u16_at(r, wire::R_KIND) == db::DRAGON_DIED),
                "the journal holds the death the pages lack"
            );
            saved
        } else {
            let saved = j.world_save();
            e.close(true, Duration::from_secs(5)).unwrap();
            saved
        };

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.idle();
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let alive = summary(&e).0;
        e.close(true, Duration::from_secs(5)).unwrap();
        alive
    }

    /// docs/replay-gate.md §5.1 (RP-70): the bodies the pages hold embodied stay embodied through
    /// the replay, so the journal's DRAGON_DIED of the last session's body finds it. Today (the
    /// base): the session start made every dragon abstract first, the death found nothing, and
    /// the dead dragon was back (1 alive, 0 expected).
    #[test]
    fn a_dragon_killed_after_a_save_stays_dead_after_a_crash() {
        assert_eq!(
            dragons_after_a_death("dragon-died-crash", true),
            0,
            "the replayed death counts: the dragon stays dead"
        );
    }

    /// The control: without the crash the pages hold the death.
    #[test]
    fn a_dragon_killed_after_a_save_stays_dead_after_a_clean_restart() {
        assert_eq!(dragons_after_a_death("dragon-died-clean", false), 0);
    }

    // ---- a test's order waits for the replay end (docs/replay-gate.md §4.5, RP-74) ------------

    /// The replay ends this engine ran.
    fn replays_ended(e: &Engine) -> u64 {
        e.stats
            .replays_ended
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Hands Java's whole journal over.
    fn hand_over_all(j: &mut Java) {
        let last = j.world_save().journal.back().map_or(0, |(s, _)| *s);
        for _ in 0..64 {
            j.idle();
            if j.sent_obs >= last {
                break;
            }
        }
        assert!(j.sent_obs >= last, "the whole journal handed over");
        assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    }

    /// A restart whose replay takes several hand-overs: a lair saved, then 8,000 wounds to a body
    /// that never was (about 64 bytes each, so a minimum-size buffer takes the replay in eight or
    /// more), the session ended in a crash. Returns the next session's engine and Java after its
    /// first hand-over, the replay still under way.
    fn a_replay_under_way(tag: &str) -> (std::sync::Arc<Engine>, Java) {
        let dir = tmpdir(tag);
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
        j.idle();
        j.idle();
        save_durable(&e, &mut j);
        let first = j.world_save().next_obs;
        for _ in 0..8_000 {
            j.dragon_obs(&O::Hurt(db::Hurt {
                agent: 0,
                epoch: 0,
                attacker_kind: AttackerKind::Player,
                attacker: PLAYER,
                hits: 1,
                amount: 1.0,
                health_after: 1.0,
            }));
        }
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();
        let bytes = saved
            .journal
            .iter()
            .filter(|(s, _)| *s >= first)
            .map(|(_, r)| r.len())
            .sum::<usize>();
        assert!(
            bytes > 6 * wire::MIN_CAPACITY,
            "the replay's records: {bytes} bytes"
        );
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.idle();
        assert_eq!(
            replays_ended(&e),
            0,
            "the replay runs past its first hand-over"
        );
        (e, j)
    }

    /// A STEER of a body the engine never had, as a test would order it.
    fn a_steer_order(agent: u64) -> Order {
        let mut record = Vec::new();
        db::encode_cmd(
            0,
            &D::Steer(db::Steer {
                agent,
                epoch: 1,
                flags: 1,
                target: (1.0, 90.0, -4.0),
            }),
            &db::SpeciesDict::default(),
            &mut record,
        );
        Order::DragonCommand {
            names: vec![(wire::DICT_BODY, 1, Species::Fire.entity_id().to_string())],
            record,
        }
    }

    fn steered(j: &Java, agent: u64) -> bool {
        j.iaf
            .iter()
            .any(|(_, c)| matches!(c, D::Steer(s) if s.agent == agent))
    }

    /// RP-74 (the audit of the routes around the gate): a test's dragon command joins the
    /// dragons' queue around their gate, as a conflict command (RP-66) and a herd command
    /// (RP-85) do, so one ordered while the replay runs waits for the replay end with the orders
    /// that run the clock.
    #[test]
    fn a_dragon_command_ordered_while_replaying_waits_for_the_replay_end() {
        let (e, mut j) = a_replay_under_way("dragon-rp74-order");
        let agent = 0x44 << 56 | 0x74;
        e.order(vec![a_steer_order(agent)]).unwrap();
        j.idle();
        assert_eq!(replays_ended(&e), 0, "still replaying");
        assert!(!steered(&j, agent), "not written while the replay runs");
        hand_over_all(&mut j);
        j.idle();
        assert_eq!(replays_ended(&e), 1, "the replay ended");
        assert!(steered(&j, agent), "written once it has ended");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Control: no replay, the order is written at the first step that takes it.
    #[test]
    fn a_dragon_command_ordered_without_a_replay_is_written_at_once() {
        let dir = tmpdir("dragon-rp74-order-live");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        j.idle();
        let agent = 0x44 << 56 | 0x74;
        e.order(vec![a_steer_order(agent)]).unwrap();
        j.idle();
        assert!(steered(&j, agent), "written at once");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// RP-74: NEST_DAY books the nests' next watch from the engine's own clock, so taken while
    /// the replay runs it would watch the replayed past at its new cadence. It waits too.
    #[test]
    fn a_nest_day_ordered_while_replaying_waits_for_the_replay_end() {
        let (e, mut j) = a_replay_under_way("dragon-rp74-nestday");
        let was = e.with_dragons(|d, _| d.nest_day_ticks());
        assert_ne!(was, 1_234, "the tunable's own day");
        e.order(vec![Order::NestDay(1_234)]).unwrap();
        j.idle();
        assert_eq!(replays_ended(&e), 0, "still replaying");
        assert_eq!(
            e.with_dragons(|d, _| d.nest_day_ticks()),
            was,
            "not set while the replay runs"
        );
        hand_over_all(&mut j);
        j.idle();
        assert_eq!(replays_ended(&e), 1, "the replay ended");
        assert_eq!(
            e.with_dragons(|d, _| d.nest_day_ticks()),
            1_234,
            "set once it has ended"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Control: no replay, the nests' day is set at once.
    #[test]
    fn a_nest_day_ordered_without_a_replay_is_set_at_once() {
        let dir = tmpdir("dragon-rp74-nestday-live");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        j.idle();
        e.order(vec![Order::NestDay(1_234)]).unwrap();
        j.idle();
        assert_eq!(e.with_dragons(|d, _| d.nest_day_ticks()), 1_234);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The dragons' clock, in ticks.
    fn dragons_now(e: &Engine) -> u64 {
        e.with_dragons(|d, _| d.now().0)
    }

    /// A test's phase 1 advance, far past anything the replayed journal holds.
    const ADVANCE: u64 = 5_000_000;

    /// RP-74: a phase 1 advance raises the offset that every record's game time takes, so taken
    /// while the replay runs it would shift the replayed records still to come, the dragons'
    /// among them, years into the future. It waits for the replay end too.
    #[test]
    fn a_phase1_advance_ordered_while_replaying_waits_for_the_replay_end() {
        let (e, mut j) = a_replay_under_way("dragon-rp74-advance");
        e.order(vec![Order::Phase1Advance(ADVANCE)]).unwrap();
        j.idle();
        assert_eq!(replays_ended(&e), 0, "still replaying");
        assert!(
            dragons_now(&e) < ADVANCE,
            "the replayed records are not shifted: the dragons are at {}",
            dragons_now(&e)
        );
        hand_over_all(&mut j);
        j.idle();
        assert_eq!(replays_ended(&e), 1, "the replay ended");
        assert!(
            dragons_now(&e) >= ADVANCE,
            "applied once it has ended: the dragons are at {}",
            dragons_now(&e)
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Control: no replay, the advance is taken at the first step.
    #[test]
    fn a_phase1_advance_ordered_without_a_replay_is_taken_at_once() {
        let dir = tmpdir("dragon-rp74-advance-live");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        j.idle();
        assert!(dragons_now(&e) < ADVANCE);
        e.order(vec![Order::Phase1Advance(ADVANCE)]).unwrap();
        j.idle();
        // A record of the dragons' own, which takes the clock the offset gives it.
        j.dragon_obs(&O::Hurt(db::Hurt {
            agent: 0,
            epoch: 0,
            attacker_kind: AttackerKind::Player,
            attacker: PLAYER,
            hits: 1,
            amount: 1.0,
            health_after: 1.0,
        }));
        j.idle();
        assert!(
            dragons_now(&e) >= ADVANCE,
            "the dragons are at {}",
            dragons_now(&e)
        );
        e.close(true, Duration::from_secs(5)).unwrap();
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
        save_durable(e, j);
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
        // In lockstep: "most of its voyage ahead" is the engine's clock, and the fight and the loot
        // below land at Java's. Ticking on the wall clock (`run_until`) let Java run ahead of a
        // loaded engine, so the loot landed after the merchant had docked and sold its hold: no
        // loot booked.
        let t0 = j.game_time;
        flush_until(&mut j, "a merchant with a hold in view", 12_000, |j| {
            laden(j, &j.e.clone()).is_some()
        });
        let ticks = j.game_time - t0;
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

    /// A trade port's state through the native engine: whether the ships hold it closed, and
    /// whether the world holds each of its lanes closed.
    fn port_shut(e: &Engine, d: u32) -> (bool, Vec<bool>) {
        e.with_ships(|s, w| {
            let lanes = s
                .dock_lanes(d)
                .iter()
                .map(|&l| w.lane_closed(s.lanes[l as usize].edge))
                .collect();
            (s.port_closed(d), lanes)
        })
    }

    /// PC101 (C25, U10): switching the ships off lifts every port closure, so no lane of the
    /// world stays closed by a module that is not running. A closed port (closed by hand, as its
    /// check would close it) opens with its lanes at the switch-off, and closes again once the
    /// ships are back on. A session that starts with the ships off lifts the closure its pages
    /// hold and writes them once; switched on in the next session they read back clean, the port
    /// open.
    #[test]
    fn switching_the_ships_off_lifts_their_port_closures() {
        let dir = tmpdir("ships-lift");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        coast(&mut j);
        let d = e
            .with_ships(|s, w| (0..s.docks.len() as u32).find(|&d| s.is_trade_port(w, d)))
            .expect("a trade port");
        let n = e.with_ships(|s, _| s.dock_lanes(d).len());
        assert!(n > 0, "the port has lanes");
        let (shut, open) = ((true, vec![true; n]), (false, vec![false; n]));
        assert_eq!(port_shut(&e, d), open);
        assert!(e.with_ships_mut(|s, w| s.close_port(w, d)));
        assert_eq!(port_shut(&e, d), shut);
        j.idle();
        assert_eq!(port_shut(&e, d), shut, "closed while the ships run");

        // Switched off: the port and its lanes open.
        j.modules(0b1111);
        j.idle();
        assert_eq!(port_shut(&e, d), open, "lifted at the switch-off");
        e.with_ships(|s, w| s.check(w)).expect("the ships check");
        assert_eq!(e.with_ships(|s, _| s.stats.reopenings), 1);

        // On again: it can close again; then the session ends with it closed.
        j.modules(MODULES);
        j.idle();
        assert_eq!(port_shut(&e, d), open);
        assert!(e.with_ships_mut(|s, w| s.close_port(w, d)));
        j.idle();
        assert_eq!(port_shut(&e, d), shut, "closed again");
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        let closed_page = page(&dir).expect("a ships page");

        // A session off from its start: the closure the pages hold is lifted, and written.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        j.idle();
        assert_eq!(port_shut(&e, d), open, "lifted at the session's start");
        e.with_ships(|s, w| s.check(w)).expect("the ships check");
        assert_eq!(
            e.with_ships(|s, _| (s.stats.closures, s.stats.reopenings)),
            (2, 2)
        );
        save(&e, &mut j);
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();
        assert_ne!(page(&dir), Some(closed_page), "the lift is written");

        // On in the next session: the pages read back clean, the port open in the ships and in
        // the world.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.idle();
        assert_eq!(e.ships_restore_check(), (None, false));
        assert_eq!(port_shut(&e, d), open, "open after the restore");
        e.with_ships(|s, w| s.check(w)).expect("the ships check");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// PC103 (C25, U10): a long switch-off, then the switch-on's catch-up, through the native
    /// engine. Thirty days on (a haven on the coast, so the pirates hunt), thirty days off, then
    /// on again: the ships replay what they missed up to the world's present at the switch-on,
    /// and decide no port in that time. After a day on, no port check has been decided since the
    /// switch-off (each missed one comes again a check interval after the switch-on), no port
    /// holds a closure, blockade or open spell dated before the switch-on and every booked check
    /// is after it; the ships check and the protocol holds.
    #[test]
    fn switch_on_catch_up_after_a_long_off() {
        let dir = tmpdir("ships-catchup");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        coast(&mut j);
        let days = |j: &mut Java, n: u64| {
            for _ in 0..n * 20 {
                j.game_time += 1199;
                j.tick();
            }
            j.idle();
        };
        days(&mut j, 30);
        let (before, _, _, _) = e.ship_stats();
        j.modules(0b1111);
        j.idle();
        let off = e.with_ships(|s, _| s.now());
        days(&mut j, 30);
        let on = e.with_ships(|_, w| w.now());
        assert!(
            on.0 >= off.0 + 29 * rtsim_kernel::clock::TICKS_PER_DAY,
            "the world ran on alone"
        );
        assert_eq!(e.with_ships(|s, _| s.now()), off, "the ships stood still");
        j.modules(MODULES);
        j.idle();
        days(&mut j, 1);
        let (after, _, _, _) = e.ship_stats();
        assert!(after.events > before.events, "nothing replayed");
        assert_eq!(
            after.port_checks, before.port_checks,
            "a port check decided in the missed time"
        );
        let (now, ports) = e.with_ships(|s, _| (s.now(), s.ports.clone()));
        assert!(now >= on, "the ships caught up: {now:?} < {on:?}");
        for (d, p) in &ports {
            for t in [p.closed, p.blockaded, p.reopened].into_iter().flatten() {
                assert!(t >= on, "dock {d}: dated in the missed time: {p:?}");
            }
            assert!(p.check.is_some_and(|t| t >= on), "dock {d}: {p:?}");
        }
        e.with_ships(|s, w| s.check(w)).expect("the ships check");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        eprintln!(
            "off at {off:?}, on at {on:?}, now {now:?}; events {} -> {}, port checks {} -> {}, \
             {} ports with a state",
            before.events,
            after.events,
            before.port_checks,
            after.port_checks,
            ports.len()
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// U7, PC103: a phase 1 input from a latest-state record runs the modules to its time first,
    /// so the world never runs ahead of the ships in normal running and nothing there is taken for
    /// missed while the ships were off. A hand-over that spans several ticks (the engine's last
    /// step not yet published, so Java waited to submit) carries a census that changes a village's
    /// golems: the port check due inside it is decided at its time, as in the same hand-over
    /// without the census, and the next is booked a check interval after it.
    #[test]
    fn a_census_in_a_long_hand_over_leaves_the_ships_their_port_check() {
        let mut arms = Vec::new();
        for with_census in [false, true] {
            let dir = tmpdir(if with_census {
                "ships-census-on"
            } else {
                "ships-census-off"
            });
            let e = engine(&dir, 256 * 1024);
            let mut j = Java::open(e.clone(), WorldSave::fresh());
            j.module_mask = MODULES;
            coast(&mut j);
            // A quarter of a day a hand-over until a port has a check booked well ahead.
            let next_check = |e: &Engine| {
                e.with_ships(|s, _| {
                    let now = s.now();
                    s.ports
                        .iter()
                        .filter_map(|(d, p)| p.check.map(|t| (t, *d)))
                        .filter(|(t, _)| t.0 > now.0 + 10)
                        .min()
                })
            };
            let mut due = None;
            for _ in 0..4 * 60 {
                j.game_time += rtsim_kernel::clock::TICKS_PER_DAY as i64 / 4;
                j.flush();
                due = next_check(&e);
                if due.is_some() {
                    break;
                }
            }
            let (t, d) = due.expect("a port check is booked");
            // The world's tick for a game time (the test offset, 0 here).
            let offset = e.with_ships(|_, w| w.now().0) as i64 - (j.game_time - 1);
            // Hand-overs to two ticks before the check, then one of three ticks across it.
            j.game_time = t.0 as i64 - offset - 2;
            j.flush();
            let before = e.ship_stats().0.port_checks;
            j.game_time = t.0 as i64 - offset + 1;
            if with_census {
                j.census(VILLAGES[0].0, 0, 1);
            }
            j.flush();
            let (checks, check, now) = e.with_ships(|s, w| {
                (
                    s.stats.port_checks,
                    s.ports.get(&d).and_then(|p| p.check),
                    (s.now(), w.now()),
                )
            });
            assert_eq!(now.0, now.1, "census {with_census}: one clock");
            assert!(
                checks > before,
                "census {with_census}: the check due at {t:?} was not decided ({check:?})"
            );
            e.with_ships(|s, w| s.check(w)).expect("the ships check");
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            arms.push((t, d, checks - before, check));
            e.close(true, Duration::from_secs(5)).unwrap();
        }
        assert_eq!(arms[0], arms[1], "a census changes no port check");
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
        save_durable(e, j);
    }

    /// Founds `n` herds of 20 sheep, 64 blocks apart along a line, in one order unit.
    fn found_many(e: &Engine, j: &mut Java, n: u64) {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let unit = (0..n)
            .map(|i| Order::HerdFound {
                name: format!("Testdown flock {i}"),
                species: "minecraft:sheep".into(),
                dimension: dim,
                x: AT.0 + 64 * i as i32,
                z: AT.1,
                count: 20,
                seed: i + 1,
            })
            .collect();
        e.order(unit).unwrap();
        j.idle();
        assert_eq!(e.herd_agents().len() as u64, n, "every herd founded");
    }

    /// A save at the next step, waited for: the raw bytes it wrote.
    fn save_bytes(e: &Engine, j: &mut Java) -> u64 {
        use std::sync::atomic::Ordering::Acquire;
        let done = e.stats.saves_done.load(Acquire);
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        let deadline = Instant::now() + Duration::from_secs(20);
        while e.stats.saves_done.load(Acquire) == done {
            assert!(Instant::now() < deadline, "the save never committed");
            std::thread::sleep(Duration::from_millis(2));
        }
        e.stats.last_save_bytes.load(Acquire)
    }

    /// The herds page through the engine's store (docs/choices-paging-followups.md PF12): a
    /// save with nothing changed since the last one writes what changed, not every herd. At 32
    /// herds and at 128 (no player near them, a few ticks between the two saves), the second
    /// save writes at most 4 KB more. It failed while the link saved the herds as one page.
    #[test]
    fn a_save_with_nothing_changed_is_flat_in_the_herd_count() {
        let quiet = |n: u64| {
            let dir = tmpdir("herds-quiet");
            let e = engine(&dir, 256 * 1024);
            let mut j = Java::open(e.clone(), WorldSave::fresh());
            j.module_mask = MODULES;
            found_many(&e, &mut j, n);
            save_bytes(&e, &mut j);
            let bytes = save_bytes(&e, &mut j);
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            e.close(true, Duration::from_secs(5)).unwrap();
            bytes
        };
        let (small, large) = (quiet(32), quiet(128));
        eprintln!("a quiet save: {small} B at 32 herds, {large} B at 128");
        assert!(
            large <= small + 4096,
            "a quiet save wrote {small} B at 32 herds, {large} B at 128"
        );
    }

    /// Herds under a row budget a fraction of their rows (docs/choices-paging-followups.md
    /// PF15): saved pages leave at the step boundary, a step whose resident rows all changed
    /// since the last save takes a save of its own (a pending generation), and after a crash the
    /// start rolls those back and reads the herds back at the last save's state hash, paging
    /// their rows in from the store as they are needed.
    #[test]
    fn herds_over_their_row_budget_page_out_and_read_back_after_a_crash() {
        use std::sync::atomic::Ordering::Acquire;
        let dir = tmpdir("herds-budget");
        let budget = 4 * 1024;
        let e = engine_with(&dir, 256 * 1024, |c| c.row_budget = budget);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        found_many(&e, &mut j, 128);
        save_bytes(&e, &mut j);
        // The founding dirtied every row; the herds move on for a few days, no save asked for.
        let asked = e.stats.saves_done.load(Acquire);
        let mut peak = 0;
        for _ in 0..80 {
            j.game_time += 1_200;
            j.idle();
            peak = peak.max(e.with_herds(|h, _| h.rows_resident_bytes()));
        }
        let (resident, total) = e.with_herds(|h, _| (h.rows_resident_bytes(), h.rows_budget()));
        let pressure = e.stats.pressure_saves.load(Acquire);
        eprintln!(
            "herds under {total} B: rows {resident} B at the end, {peak} B at peak, {pressure} saves for the budget"
        );
        assert_eq!(e.stats.saves_done.load(Acquire), asked, "no save asked for");
        assert!(pressure > 0, "the budget took saves of its own");
        assert!(
            resident <= total,
            "the rows left down to their budget: {resident} B against {total} B"
        );
        let herds = e.herd_agents();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let saved = j.crash();
        {
            let store = PageStore::open(&dir, rtsim_kernel::page::Durability::NoSync).unwrap();
            assert_ne!(
                store.current(),
                store.confirmed(),
                "the budget's saves are pending generations"
            );
        }
        let e = engine_with(&dir, 256 * 1024, |c| c.row_budget = budget);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        assert_eq!(
            e.herds_restore_check(),
            (None, false),
            "the herds read back to the last save's state hash"
        );
        assert_eq!(e.herd_agents(), herds, "every herd paged back in");
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let _ = j.crash();
        let store = PageStore::open(&dir, rtsim_kernel::page::Durability::NoSync).unwrap();
        assert_eq!(
            store.current(),
            store.confirmed(),
            "the start rolled the pending generations back"
        );
    }

    /// RP-49 for the herds (fixes4 item 7), the largest paged module: 128 herds under a 4 KiB
    /// row budget, a save, then `steps` steps of 1,200 ticks with the herds moving on, 50
    /// transfers journaled a step, no save asked for, and a crash. Returns the live session's
    /// resident row peak, the same session's with a budget it never reaches (every herd row
    /// resident: the unpaged footprint), and the replay's peak after the crash, under the budget.
    fn the_replays_herd_peak_against_the_live_sessions(name: &str, steps: u64) -> [usize; 3] {
        use std::sync::atomic::Ordering::Acquire;
        const BUDGET: usize = 4 * 1024;
        let live = |dir: &std::path::PathBuf, budget: usize| {
            let e = engine_with(dir, 256 * 1024, |c| c.row_budget = budget);
            let mut j = Java::open(e.clone(), WorldSave::fresh());
            j.module_mask = MODULES;
            found_many(&e, &mut j, 128);
            save(&e, &mut j);
            let durable = e.durable_obs_seq();
            let mut peak = 0;
            for i in 0..steps * 50 {
                j.transfer(wire::JAVA_OP_BIT | (i + 1), 1, 2, 1);
                if (i + 1) % 50 == 0 {
                    j.game_time += 1_200;
                    j.idle();
                    peak = peak.max(e.with_herds(|h, _| h.rows_resident_bytes()));
                }
            }
            assert_eq!(e.durable_obs_seq(), durable, "no save asked for");
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            let pressure = e.stats.pressure_saves.load(Acquire);
            let gt = j.game_time;
            (peak, pressure, gt, j.crash())
        };
        let (unpaged, pressure, _, _) = live(&tmpdir(&format!("{name}-unpaged")), 1 << 30);
        assert_eq!(pressure, 0, "a budget the session never reaches");
        let dir = tmpdir(name);
        let (peak, pressure, gt, saved) = live(&dir, BUDGET);
        assert!(
            pressure > 0,
            "the live session's budget took saves of its own"
        );

        // Not in TEST mode: TEST mode reads every page back for its state hash at the start and
        // at each save (`herds.rs`, `paging.rs`), which this peak would count as the replay's.
        let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
            c.row_budget = BUDGET;
            c.test = false;
        });
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        let mut replay = 0;
        for hand_overs in 0.. {
            assert!(hand_overs < 256, "the replay never ended");
            if e.stats.replays_ended.load(Acquire) > 0 {
                break;
            }
            j.flush();
            replay = replay.max(e.with_herds(|h, _| h.rows_resident_bytes()));
        }
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
        [peak, unpaged, replay]
    }

    /// RP-49 for the herds (fixes4 item 7, RP-55): the replay's resident herd rows are the pages
    /// the crashed session touched since its last durable save, bounded by the unpaged footprint.
    /// No page is taken while replaying (RP-42), so they grow with the window: two steps hold
    /// about half of it here, eighty nearly all. The live peaks are printed, not asserted: they
    /// ride on when the IO lane's saves come back.
    #[test]
    fn a_replays_herd_rows_are_the_rows_touched_since_the_last_durable_save() {
        let short = the_replays_herd_peak_against_the_live_sessions("herds-rp49-short", 2);
        let long = the_replays_herd_peak_against_the_live_sessions("herds-rp49-long", 80);
        eprintln!("herd row bytes [live, unpaged, replay], 2 steps: {short:?}; 80 steps: {long:?}");
        for [live, unpaged, replay] in [short, long] {
            assert!(
                replay <= unpaged,
                "the replay holds no more than the unpaged footprint: {live} {unpaged} {replay}"
            );
        }
        assert!(
            short[2] < short[1],
            "a short window holds the pages it touched, not every page: {short:?}"
        );
        assert!(long[2] > short[2], "{short:?} {long:?}");
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
        // From here to the second group, Java runs in lockstep with the engine: the sheared sheep
        // is still sheared only while its wool has not regrown (`sheared_ticks` of game time).
        let shorn_at = j.game_time;
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
        flush_until(&mut j, "the group captured", 1_000, |j| {
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
        flush_until(&mut j, "the group again", 1_000, |j| {
            materializations(j).len() >= 2
        });
        j.idle();
        let regrow = e.with_herds(|h, _| h.tun.sheared_ticks) as i64;
        assert!(
            j.game_time - shorn_at < regrow,
            "the group came back {} game ticks after the shearing, past the {regrow} its wool takes to regrow",
            j.game_time - shorn_at
        );
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
    use rtsim_conflict::culture::PartyKind;
    use rtsim_conflict::engine::{Party, Phase, Target};
    use rtsim_conflict::{Step, synth};
    use rtsim_jni::engine::{CONFLICT_TABLE, MODULE_CONFLICT, MODULE_INDUSTRY};
    use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
    use rtsim_kernel::handle::Handle;
    use rtsim_kernel::page::{PageKey, PageStore};
    use rtsim_settle::Pos;

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
            home_bodies: 0,
            census: 0,
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
        save_durable(e, j);
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

    /// The Mowzie commands (MOWZIE.md §2) through CONFLICT_COMMAND: BAND_MARCH's dimension,
    /// culture (dictionary kind 5) and unit bodies, CAMP_RECONCILE's line bodies and AMBUSH_SPOT's
    /// dimension go out in the engine's dictionary, the rest as given.
    #[test]
    fn the_mowzie_commands_go_out_in_the_engines_dictionary() {
        let dir = tmpdir("conflict-order-mowzie");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.idle();
        let (warrior, culture) = (41, 7);
        let agent = 0x43 << 56 | 0xD0D;
        let march = C::March(cb::BandMarch {
            agent,
            revision: 2,
            dimension: dim,
            faction: CAMP,
            culture,
            kind: 3,
            phase: cb::MARCH_OUT,
            flags: cb::MARCH_CHIEF,
            partner: 0,
            muster_end: 5_000,
            units: vec![(warrior, 6)],
            waypoints: vec![(10, 20, 6_000), (300, -40, 9_000)],
        });
        let reconcile = C::Reconcile(cb::CampReconcile {
            faction: CAMP,
            epoch: (1 << 63) | 9,
            op: 0x43 << 56 | 0x55,
            flags: 0,
            throne: (0.0, 0.0, 0.0),
            direction: 0.0,
            lines: vec![
                cb::ReconcileLine {
                    slot: 1,
                    generation: 1,
                    body: warrior,
                    action: cb::ACTION_SPAWN,
                    flags: 0,
                    a: 0,
                    b: 0,
                },
                cb::ReconcileLine {
                    slot: 0,
                    generation: 0,
                    body: 0,
                    action: cb::ACTION_REMOVE,
                    flags: 0,
                    a: 0x1234,
                    b: 0x5678,
                },
            ],
        });
        let spot = C::AmbushSpot(cb::AmbushSpot {
            agent,
            epoch: 1,
            flags: 0,
            dimension: dim,
            faction: CAMP,
            track: (1.0, 64.0, 2.0),
            spot: (5.0, 64.0, 6.0),
            reach: 8.0,
        });
        let names = vec![
            (
                wire::DICT_BODY,
                warrior,
                "mic_rtsim:conflict/goblin/warrior".to_string(),
            ),
            (
                wire::DICT_CULTURE,
                culture,
                "mic_rtsim:conflict/goblin".to_string(),
            ),
        ];
        for cmd in [&march, &reconcile, &spot] {
            let mut record = Vec::new();
            cb::encode_cmd(0, cmd, &mut record);
            e.order(vec![Order::ConflictCommand {
                names: names.clone(),
                record,
            }])
            .unwrap();
        }
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let got = cmds(&j)
            .find_map(|c| match c {
                C::March(m) => Some(m.clone()),
                _ => None,
            })
            .expect("the ordered BAND_MARCH");
        assert_eq!(
            java_name(&j, wire::DICT_CULTURE, got.culture).as_deref(),
            Some("mic_rtsim:conflict/goblin")
        );
        assert_eq!(
            java_name(&j, wire::DICT_BODY, got.units[0].0).as_deref(),
            Some("mic_rtsim:conflict/goblin/warrior")
        );
        assert_eq!(got.dimension, dim);
        assert_eq!(
            (got.muster_end, got.waypoints.clone(), got.partner),
            (5_000, vec![(10, 20, 6_000), (300, -40, 9_000)], 0),
            "a test's order is in game time already"
        );
        let got = cmds(&j)
            .find_map(|c| match c {
                C::Reconcile(r) => Some(r.clone()),
                _ => None,
            })
            .expect("the ordered CAMP_RECONCILE");
        assert_eq!(
            java_name(&j, wire::DICT_BODY, got.lines[0].body).as_deref(),
            Some("mic_rtsim:conflict/goblin/warrior")
        );
        assert_eq!(
            (got.lines[1].body, got.lines[1].a, got.lines[1].b),
            (0, 0x1234, 0x5678),
            "a line with no body keeps 0"
        );
        let got = cmds(&j)
            .find_map(|c| match c {
                C::AmbushSpot(a) => Some(*a),
                _ => None,
            })
            .expect("the ordered AMBUSH_SPOT");
        assert_eq!((got.dimension, got.reach), (dim, 8.0));
        let (_, link) = e.conflict_stats();
        assert_eq!(link.commands_written, 3);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A conflict session with the camp founded, its territory back, the fake plugin answering
    /// as `PLAYER` and the player standing in the territory.
    fn camp(name: &str) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java) {
        camp_with(name, MODULES)
    }

    /// [`camp`] with the module switches `mask`.
    fn camp_with(name: &str, mask: u32) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java) {
        let dir = tmpdir(name);
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = mask;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let base = [f64::from(BASE.0) + 0.5, 64.0, f64::from(BASE.1) + 0.5];
        j.players = vec![(PLAYER, dim, base)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        let _ = e.conflict_log(true);
        j.idle();
        found(&mut j, CAMP_AT);
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        (dir, e, j)
    }

    /// One TALK_ASK in the open buffer, a step, and the answer: its record's size and its facts.
    fn ask_holder(
        j: &mut Java,
        id: u64,
        kind: u32,
        holder: u64,
        player: (u64, u64),
        flags: u32,
    ) -> Option<(usize, rtsim_jni::talk_wire::Facts)> {
        use rtsim_jni::talk_wire::{Ask, Facts, encode_asks};
        let dimension = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let rec = encode_asks(&[Ask {
            id,
            player,
            holder_kind: kind,
            dimension,
            holder,
            x: CAMP_AT.0,
            z: CAMP_AT.1,
            flags,
        }]);
        if j.preamble() {
            assert!(j.copy(&rec), "room for a TALK_ASK");
        }
        j.idle();
        j.talk_facts.iter().find_map(|b| {
            Facts::decode(b)
                .ok()
                .filter(|(_, f)| f.ask_id == id)
                .map(|(_, f)| (b.len(), f))
        })
    }

    /// TALK slice 4 (docs/boundary.md §13.2, §13.3): a camp is asked about by its faction key
    /// (holder kind 3) and answers, over the real protocol, with its chief's news and the 48-byte
    /// faction block after the news. A stranger finds the camp at rung 0 with no signal; after a
    /// killing the block carries the feud as the engine's own view has it; the chief's board takes
    /// a TEST post, and the news comes back told from the camp's side.
    #[test]
    fn a_camp_is_asked_about_and_answers_with_its_chiefs_news_and_the_faction_block() {
        use rtsim_jni::orders::{TalkActor, TalkReportCmd};
        use rtsim_jni::talk_wire::{ask, facts, news, reference};
        let (_dir, e, mut j) = camp("conflict-talk-camp");
        let both = ask::NEWS | ask::FACTION;
        let kind = ask::holder::FACTION;

        // A stranger: the camp is known, nobody has any quarrel, no news.
        let (len, f) = ask_holder(&mut j, 1, kind, CAMP, PLAYER2, both).expect("answered");
        assert_eq!(f.flags, facts::HOLDER | facts::FACTION);
        assert!(f.news.is_empty());
        assert_eq!(len, facts::record_size(0, true), "48 + 48");
        let b = f.faction.expect("the block");
        assert_eq!(
            (b.rung, b.signal, b.grievance),
            (0, facts::faction::NO_SIGNAL, 0.0)
        );
        assert_eq!(
            b.chief,
            rtsim_jni::talk_wire::Ref::default(),
            "M9's, never set yet"
        );
        assert_eq!(b.mood & facts::faction::mood::YOU_KILLED_CHIEF, 0);
        assert_eq!(e.talk_stats().with_faction, 1);

        // Without the flag there is no block: the record is what it was before slice 4.
        let (len, f) = ask_holder(&mut j, 2, kind, CAMP, PLAYER2, ask::NEWS).unwrap();
        assert_eq!((f.flags, f.faction, len), (facts::HOLDER, None, 48));

        // Not a camp: a village asked by the camp's key, and a camp nobody founded.
        let (_, f) = ask_holder(&mut j, 3, ask::holder::VILLAGE, CAMP, PLAYER2, both).unwrap();
        assert_eq!((f.flags, f.faction), (0, None));
        let (_, f) = ask_holder(&mut j, 4, kind, CAMP + 1, PLAYER2, both).unwrap();
        assert_eq!((f.flags, f.faction), (0, None));

        // A killing at the camp: the block carries the feud as the engine's view has it.
        signal(&mut j, 4, 60, PLAYER);
        j.idle();
        let view = feud(&e, PLAYER).expect("a feud with the killer");
        let (_, f) = ask_holder(&mut j, 5, kind, CAMP, PLAYER, both).unwrap();
        let b = f.faction.unwrap();
        assert_eq!(b.signal, 4, "killing, FACTION_NOTICE's code");
        assert_eq!(b.rung, view.step as u8);
        assert!(
            (f64::from(b.grievance) - view.grievance).abs() <= 0.02 * view.grievance,
            "{} against {}",
            b.grievance,
            view.grievance
        );
        // Another player is still a stranger to it.
        let (_, f) = ask_holder(&mut j, 6, kind, CAMP, PLAYER2, both).unwrap();
        assert_eq!(f.faction.unwrap().rung, 0);

        // The chief's board takes a TEST post, and the camp tells it as its own.
        e.order(vec![Order::TalkReport(TalkReportCmd {
            village: CAMP,
            kind: rtsim_knowledge::ReportKind::Raid as u16,
            severity: 3,
            flags: 0,
            who: TalkActor {
                class: reference::class::CAMP,
                a: CAMP,
                b: 0,
            },
            whom: TalkActor {
                class: reference::class::UNKNOWN,
                a: 0,
                b: 0,
            },
            x: CAMP_AT.0,
            z: CAMP_AT.1,
            ago: 1_000,
        })])
        .unwrap();
        j.idle();
        assert_eq!(e.talk_stats().posted, 1);
        // The board also holds the camp's own Feud against the killer, recorded at the rung the
        // killing reached (TK26): two stories, each told as the camp's own.
        let (len, f) = ask_holder(&mut j, 7, kind, CAMP, PLAYER2, both).unwrap();
        assert_eq!(f.news.len(), 2, "the post and the feud");
        assert_eq!(len, facts::record_size(2, true), "48 + 2 x 120 + 48");
        let of = |k: rtsim_knowledge::ReportKind| {
            f.news
                .iter()
                .find(|n| n.kind == k as u16)
                .unwrap_or_else(|| panic!("a {k:?} in the news"))
        };
        let n = of(rtsim_knowledge::ReportKind::Raid);
        assert_eq!(n.talk_flags & (news::BY_US | news::TO_US), news::BY_US);
        assert_eq!((n.who.class, n.who.a), (reference::class::CAMP, CAMP));
        let n = of(rtsim_knowledge::ReportKind::Feud);
        assert_eq!(n.talk_flags & (news::BY_US | news::TO_US), news::BY_US);
        assert_eq!((n.who.class, n.who.a), (reference::class::CAMP, CAMP));
        assert_eq!(n.severity, view.step as u8, "the rung");
        assert!(f.faction.is_some());
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// DIFFICULTY, journaled at the present game time (MZB-17).
    fn difficulty(j: &mut Java, difficulty: u32) {
        let seq = j.conflict_obs(&O::Difficulty(cb::Difficulty { difficulty }));
        assert!(seq > 0, "journaled");
        j.idle();
    }

    /// War bands, as BAND_MATERIALIZE puts them on the arc around the players.
    fn war_bands(j: &Java) -> usize {
        cmds(j)
            .filter(|c| matches!(c, C::Materialize(m) if m.flags & cb::BAND_HOSTILE != 0))
            .count()
    }

    /// MOWZIE.md §4.12 (ledger 14): DIFFICULTY is journaled (MZB-17), applied at its game time
    /// as `Conflict::ingest` applies it; an unchanged value changes nothing. On Peaceful no war
    /// party and no envoy leaves however far the grievance climbs (MZB-132): the notice still
    /// comes and the feud holds at it; back on Normal the climb resumes: the envoy, the refused
    /// demand, the scouts and a raid. A DIFFICULTY sent while the module is off is ignored, as
    /// every journaled record is (U10).
    #[test]
    fn on_peaceful_no_war_party_leaves_and_on_normal_the_raid_follows() {
        let (_dir, e, mut j) = camp("conflict-peaceful");
        j.conflict_plugin.answer = synth::Answer::Refuse;
        j.game_time += 5000;
        let gt = j.game_time;
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert!(
            e.with_conflict(|c, _| c.peaceful),
            "the engine is on Peaceful"
        );
        let since = e.with_conflict(|c, _| c.peaceful_since);
        assert_eq!(
            since,
            rtsim_kernel::clock::Tick(gt as u64),
            "applied at its game time"
        );
        // Again, unchanged, later: nothing changes, the span still starts at the first.
        j.game_time += 500;
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert_eq!(e.with_conflict(|c, _| c.peaceful_since), since);
        signal(&mut j, 4, 60, PLAYER);
        j.idle();
        let f = feud(&e, PLAYER).expect("a feud with the killer");
        assert!(f.grievance > 400.0, "far past the raid's rung: {f:?}");
        let start = j.game_time;
        advance_until(&mut j, 1_000, 9 * DAY, "eight days of Peaceful", |j| {
            j.game_time >= start + 8 * DAY
        });
        let (st, link) = e.conflict_stats();
        let war: u64 = [PartyKind::Raid, PartyKind::Invasion, PartyKind::Siege]
            .iter()
            .map(|k| st.parties[k.index()])
            .sum();
        assert_eq!(war, 0, "no war party on Peaceful: {st:?}");
        assert_eq!(war_bands(&j), 0, "no war band in view");
        assert_eq!(
            st.parties[PartyKind::Envoy.index()],
            0,
            "no envoy on Peaceful: {st:?}"
        );
        assert!(!cmds(&j).any(|c| matches!(c, C::Envoy(_))), "none in view");
        let f = feud(&e, PLAYER).unwrap();
        assert_eq!(f.step, Step::Notice, "the feud holds at its notice: {f:?}");
        assert!(f.demand.is_none(), "and asks nothing: {f:?}");
        assert_eq!(link.ignored, 0, "{link:?}");

        // Normal: the climb resumes: the envoy comes, the demand is refused, scouts go, and a
        // raid comes to the base.
        difficulty(&mut j, cb::DIFFICULTY_NORMAL);
        assert!(!e.with_conflict(|c, _| c.peaceful));
        advance_until(&mut j, 1_000, 4 * DAY, "the demand refused", |j| {
            cmds(j).any(|c| matches!(c, C::Settled(s) if s.outcome == cb::SETTLED_REFUSED))
        });
        assert!(
            e.conflict_stats().0.parties[PartyKind::Envoy.index()] >= 1,
            "the envoy came"
        );
        advance_until(&mut j, 1_000, 12 * DAY, "a war band at the base", |j| {
            war_bands(j) > 0
        });
        assert!(
            e.conflict_stats().0.parties[PartyKind::Scouts.index()] >= 1,
            "the scouts went first"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();

        // With the module off, DIFFICULTY is ignored like every journaled record (U10): the
        // plugin sends it again at the next session start. Switched on, the next one applies.
        let dir = tmpdir("conflict-peaceful-off");
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = 0b1111;
        j.idle();
        let (_, before) = e.conflict_stats();
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        let (_, link) = e.conflict_stats();
        assert_eq!(link.ignored, before.ignored + 1, "ignored while off");
        assert!(!e.with_conflict(|c, _| c.peaceful), "not applied while off");
        j.modules(MODULES);
        j.idle();
        assert!(!e.with_conflict(|c, _| c.peaceful), "nor when switched on");
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert!(e.with_conflict(|c, _| c.peaceful), "the next one applies");
        // The retired latest-wins id is read as nothing.
        let mut b = vec![0u8; 16];
        b[0..2].copy_from_slice(&0x0093u16.to_le_bytes());
        b[2..4].copy_from_slice(&16u16.to_le_bytes());
        assert!(cb::decode_obs(&b).is_err(), "0x0093 is retired");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-132 (item 13): Peaceful begins with an envoy in view. It walks off (RECALL_PASSING) and
    /// is held where it is: no demand comes due past its deadline, nothing settles, no envoy is
    /// sent again, however long Peaceful lasts. Back on Normal the deadline d1 is d0 plus the
    /// span, the envoy is in view again at once carrying d1, and the demand comes due not before
    /// d1.
    #[test]
    fn an_envoy_in_view_when_peaceful_begins_walks_off_and_its_deadline_moves_by_the_span() {
        let (_dir, e, mut j) = camp("conflict-peaceful-envoy");
        let envoy = envoy_in_view(&mut j);
        let d0 = envoy.deadline;
        let demand = |e: &Engine| feud(e, PLAYER).and_then(|f| f.demand).map(|d| d.3 as i64);
        assert_eq!(
            demand(&e),
            Some(d0),
            "the command carries the demand's deadline"
        );
        let held = |e: &Engine| {
            e.with_conflict(|c, _| {
                c.parties
                    .iter()
                    .find(|(_, x)| x.kind == PartyKind::Envoy)
                    .map(|(_, x)| (x.phase, x.embodied))
            })
        };
        assert_eq!(held(&e), Some((Phase::Out, true)), "in view, on its road");
        // The killing goes on: a demand cut at its deadline would not be an expired one.
        signal(&mut j, 2, 5, PLAYER);
        j.idle();

        let from = j.conflict.len();
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        let since = e.with_conflict(|c, _| c.peaceful_since);
        let walks_off: Vec<u32> = j.conflict[from..]
            .iter()
            .filter_map(|(_, c)| match c {
                C::Recall(r) if r.agent == envoy.agent => Some(r.reason),
                _ => None,
            })
            .collect();
        assert_eq!(walks_off, vec![cb::RECALL_PASSING], "the envoy walks off");
        j.idle();
        assert_eq!(held(&e), Some((Phase::Out, false)), "abstract, on its road");

        // Two days past the deadline it never reached, the player in view all along.
        let limit = d0 - j.game_time + 3 * DAY;
        advance_until(&mut j, 1_000, limit, "past the deadline", |j| {
            j.game_time >= d0 + 2 * DAY
        });
        assert!(
            !j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Envoy(_) | C::Settled(_))),
            "no envoy sent again and nothing settled on Peaceful"
        );
        assert_eq!(
            held(&e),
            Some((Phase::Waiting, false)),
            "arrived and held at its target"
        );
        assert_eq!(demand(&e), Some(d0), "the deadline is unmoved on Peaceful");
        assert_eq!(feud(&e, PLAYER).unwrap().step, Step::Envoy);
        assert_eq!(e.conflict_stats().0.demands, 1);

        // Normal: d1 = d0 + the span.
        let from = j.conflict.len();
        let at = rtsim_kernel::clock::Tick(j.game_time as u64);
        difficulty(&mut j, cb::DIFFICULTY_NORMAL);
        let d1 = d0 + at.since(since) as i64;
        assert!(
            d1 > j.game_time + DAY,
            "the moved deadline is days off: {d1}"
        );
        assert_eq!(demand(&e), Some(d1), "moved by the span");
        // The envoy is in view again, carrying d1, before the old deadline's second passing.
        advance_until(&mut j, 100, 4_000, "the envoy in view again", |j| {
            j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Envoy(_)))
        });
        let again = j.conflict[from..]
            .iter()
            .find_map(|(_, c)| match c {
                C::Envoy(x) => Some(x.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(again.deadline, d1, "the next envoy command carries d1");
        assert_eq!(again.agent, envoy.agent, "the same envoy");
        assert!(again.epoch > envoy.epoch, "a new embodiment");
        // The demand comes due not before d1.
        let limit = d1 - j.game_time + DAY;
        advance_until(&mut j, 1_000, limit, "the demand due", |j| {
            let due = j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Settled(s) if s.outcome == cb::SETTLED_EXPIRED));
            assert!(
                !due || j.game_time >= d1,
                "expired before d1 ({d1}): {}",
                j.game_time
            );
            due
        });
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MOWZIE.md §4.12 (ledger 14): a notice nobody played (no NOTICE_SHOWN) is kept in the feud's
    /// row and sent again after its faction's TERRITORY at each session start, a crash's
    /// included; NOTICE_SHOWN for it (journaled) ends that, and the next session sends none.
    #[test]
    fn an_unplayed_notice_is_resent_at_each_session_start_until_shown() {
        let (dir, e, mut j) = camp("conflict-notice");
        j.conflict_plugin.play_notices = false;
        signal(&mut j, 2, 300, PLAYER);
        j.idle();
        let notice = cmds(&j)
            .find_map(|c| match c {
                C::Notice(n) => Some(*n),
                _ => None,
            })
            .expect("FACTION_NOTICE");
        assert_eq!((notice.faction, notice.player), (CAMP, PLAYER));
        j.game_time += 100;
        j.conflict_flush();
        j.idle();
        assert_eq!(e.with_conflict(|c, _| c.kept_notices()), vec![notice]);
        save(&e, &mut j);
        let gt = j.game_time;
        let saved = j.world_save();
        let _ = j.crash();

        // The crash restart: the territory, then the kept notice.
        let restart = |saved: WorldSave, gt: i64| {
            let e = engine(&dir, 256 * 1024);
            let mut j = Java::open(e.clone(), saved);
            j.module_mask = MODULES;
            j.game_time = gt;
            j.conflict_plugin.play_notices = false;
            assert_eq!(e.conflict_restore_check(), (None, false));
            j.idle();
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            (e, j)
        };
        let (e, mut j) = restart(saved, gt);
        let order: Vec<&str> = cmds(&j)
            .filter(|c| matches!(c, C::Territory(_) | C::Notice(_)))
            .map(|c| c.name())
            .collect();
        assert_eq!(
            order,
            vec!["TERRITORY", "FACTION_NOTICE"],
            "the territory, then the kept notice"
        );
        let again = cmds(&j)
            .find_map(|c| match c {
                C::Notice(n) => Some(*n),
                _ => None,
            })
            .unwrap();
        assert_eq!(again, notice, "the same notice");

        // A NOTICE_SHOWN for another serial or a stranger changes nothing; the right one ends it.
        j.conflict_obs(&O::NoticeShown(cb::NoticeShown {
            faction: CAMP,
            serial: notice.serial + 1,
            player: PLAYER,
        }));
        j.conflict_obs(&O::NoticeShown(cb::NoticeShown {
            faction: CAMP,
            serial: notice.serial,
            player: PLAYER2,
        }));
        j.idle();
        assert_eq!(e.with_conflict(|c, _| c.kept_notices().len()), 1);
        j.conflict_obs(&O::NoticeShown(cb::NoticeShown {
            faction: CAMP,
            serial: notice.serial,
            player: PLAYER,
        }));
        j.idle();
        assert!(e.with_conflict(|c, _| c.kept_notices().is_empty()));
        assert_eq!(e.with_conflict(|c, _| c.stats.notices_shown), 1);

        // A crash before the pages hold the NOTICE_SHOWN (MZB-31): the pages still keep the
        // notice, the journal's replay ends it, and only then are kept notices sent, so none is.
        let gt = j.game_time;
        let saved = j.crash();
        let (e, mut j) = restart(saved, gt);
        assert!(cmds(&j).any(|c| matches!(c, C::Territory(t) if t.faction == CAMP)));
        assert!(
            !cmds(&j).any(|c| matches!(c, C::Notice(_))),
            "a notice shown before the crash is not sent after the replay"
        );
        assert!(e.with_conflict(|c, _| c.kept_notices().is_empty()));
        assert_eq!(
            e.with_conflict(|c, _| c.stats.notices_shown),
            1,
            "the replay ended it, once"
        );
        save(&e, &mut j);
        let gt = j.game_time;
        let saved = j.world_save();
        e.close(true, Duration::from_secs(5)).unwrap();

        // The next session: the territory, no notice.
        let (e, j) = restart(saved, gt);
        assert!(cmds(&j).any(|c| matches!(c, C::Territory(t) if t.faction == CAMP)));
        assert!(
            !cmds(&j).any(|c| matches!(c, C::Notice(_))),
            "a shown notice is not sent again"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MOWZIE.md §9: a record whose engine half comes with a later slice (the chief's, the rail's,
    /// the tribute's; the camp's came with M4) is decoded, counted as deferred and dropped; one with
    /// an op id spends it, so the same record again is not counted twice.
    #[test]
    fn later_slices_records_are_counted_and_dropped() {
        let (_dir, e, mut j) = camp("conflict-deferred");
        let casualties = e.with_conflict(|c, _| c.stats.casualties);
        let paid = O::ChiefPaid(cb::ChiefPaid {
            faction: CAMP,
            slot: 3,
            generation: 1,
            flags: 0,
            player: PLAYER,
            op: (1 << 63) | 0xDEAD,
            items: Vec::new(),
        });
        j.conflict_obs(&paid);
        j.conflict_obs(&paid);
        j.conflict_obs(&O::ChiefState(cb::ChiefState {
            faction: CAMP,
            flags: 1,
            health: 120.0,
        }));
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (_, link) = e.conflict_stats();
        assert_eq!((link.deferred, link.ignored), (2, 0), "{link:?}");
        assert!(
            e.with_conflict(|c, _| c.op_applied((1 << 63) | 0xDEAD)),
            "the engine spent the op id, as Conflict::ingest does (MZB-23)"
        );
        assert_eq!(
            e.with_conflict(|c, _| c.stats.casualties),
            casualties,
            "nothing reached the conflict"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    // ---- M4: the camp reconciler through the journal ----------------------------------------------

    /// The Umvuthana's warrior, as Java names it.
    const WARRIOR: &str = "mic_rtsim:conflict/umvuthana/warrior";

    /// A grove (FACTION_FOUND with the Umvuthana's structure) with `strays` of its own bodies standing
    /// in it, as the jar places them, in the fake plugin's camp world; the camp is out.
    fn grove(name: &str, strays: u16) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java) {
        grove_with(name, strays, false)
    }

    /// [`grove`], with the pages saved before the camp is found when `pages_first`.
    fn grove_with(
        name: &str,
        strays: u16,
        pages_first: bool,
    ) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java) {
        let dir = tmpdir(name);
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let base = [f64::from(BASE.0) + 0.5, 64.0, f64::from(BASE.1) + 0.5];
        j.players = vec![(PLAYER, dim, base)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        let _ = e.conflict_log(true);
        j.idle();
        let structure = j.dict_id(wire::DICT_STRUCTURE, "mowziesmobs:umvuthana_grove");
        let body = j.dict_id(wire::DICT_BODY, WARRIOR);
        for _ in 0..strays {
            j.conflict_plugin.camp_stray(CAMP, body);
        }
        if pages_first {
            save(&e, &mut j);
        }
        j.conflict_obs(&O::FactionFound(cb::FactionFound {
            faction: CAMP,
            flags: 0,
            dimension: dim,
            structure,
            pos: (CAMP_AT.0, 64, CAMP_AT.1),
            home_bodies: strays,
            census: 0,
        }));
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        (dir, e, j)
    }

    /// The camp's slots by state (body, owed, left, gone, dead).
    fn slots(e: &Engine) -> [u32; 5] {
        e.with_conflict(|c, _| {
            let mut n = [0; 5];
            if let Some(f) = c.faction_of_key(CAMP)
                && let Some(camp) = c.camp(f)
            {
                for s in &camp.slots {
                    n[s.state as usize] += 1;
                }
            }
            n
        })
    }

    fn reconciles(j: &Java) -> Vec<cb::CampReconcile> {
        cmds(j)
            .filter_map(|c| match c {
                C::Reconcile(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }

    /// The camp comes in (its census journaled at the present game time).
    fn camp_in(j: &mut Java) -> u64 {
        let at = rtsim_kernel::clock::Tick(j.game_time.max(0) as u64);
        let epoch = j.conflict_plugin.camp_in(CAMP, at);
        j.conflict_flush();
        j.idle();
        epoch
    }

    /// Waits (the answers journaled as the plugin sends them) until `done`.
    fn camp_until(j: &mut Java, what: &str, mut done: impl FnMut(&mut Java) -> bool) {
        advance_until(j, 50, 2 * DAY, what, |j| done(j));
    }

    /// MOWZIE.md §4.9 through the journal: a census (CAMP_CENSUS) is journaled, the engine orders
    /// (CAMP_RECONCILE, with the engine's body ids in Java's) an adoption for each stray, the plugin
    /// applies them and answers (CAMP_BODIES, journaled), and the slots are booked home, body; the
    /// same answer twice (a replay) books once.
    #[test]
    fn a_census_is_reconciled_and_its_answer_booked_through_the_journal() {
        let (_dir, e, mut j) = grove("camp-census", 3);
        assert_eq!(
            slots(&e),
            [0, 3, 0, 0, 0],
            "found with its bodies counted: owed"
        );
        let body = j.java_id(wire::DICT_BODY, WARRIOR).expect("named");
        j.conflict_plugin.replay_answers = true;
        let epoch = camp_in(&mut j);
        camp_until(&mut j, "the adoptions answered", |_| {
            slots(&e) == [3, 0, 0, 0, 0]
        });
        let orders = reconciles(&j);
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].epoch, epoch);
        assert_eq!(orders[0].lines.len(), 3);
        assert!(
            orders[0]
                .lines
                .iter()
                .all(|l| l.action == cb::ACTION_ADOPT && l.body == body),
            "adopt lines in Java's body id: {orders:?}"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let (camps, answers, deaths) = e.with_conflict(|c, _| {
            (
                c.stats.camp_censuses,
                c.stats.camp_answers,
                c.stats.camp_deaths,
            )
        });
        assert_eq!(
            (camps, answers, deaths),
            (1, 3, 0),
            "the replayed answer booked once"
        );
        assert!(e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())));
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A body killed at the camp is CAMP_DEATH, journaled under its own sequence: the member is gone
    /// from its slot and the pool, once, and a second copy of the record (a replay's) books nothing,
    /// because the slot's generation is retired at the first (the death keeps no op id).
    #[test]
    fn a_death_is_booked_through_the_journal_once() {
        let (_dir, e, mut j) = grove("camp-death", 3);
        camp_in(&mut j);
        camp_until(&mut j, "the adoptions answered", |_| {
            slots(&e) == [3, 0, 0, 0, 0]
        });
        let b = j.conflict_plugin.camps[&CAMP].bodies[0];
        let at = rtsim_kernel::clock::Tick(j.game_time.max(0) as u64);
        assert!(j.conflict_plugin.camp_kill(CAMP, b.uuid, cb::BY_PLAYER, at));
        let record = j.conflict_plugin.drop_queued();
        assert_eq!(record.len(), 1);
        j.conflict_obs(&record[0]);
        j.conflict_obs(&record[0]);
        j.idle();
        assert_eq!(slots(&e), [2, 0, 0, 0, 0]);
        assert_eq!(
            e.with_conflict(|c, _| c.stats.camp_deaths),
            1,
            "once: the second copy finds the slot's generation retired"
        );
        let (pool, cap) = e.with_conflict(|c, _| {
            let f = &c.factions[c.faction_of_key(CAMP).unwrap() as usize];
            (f.pool.floor() as u32, f.cap)
        });
        assert_eq!((pool, cap), (2, 3));
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// One op's answer comes as several records (a holder's, journaled again, and held lines answered
    /// later): each (op, line) is booked once through the journal, a line twice (a holder's record
    /// again) booked once, and the camp's records keep no op id (the engine acknowledges by
    /// sequence).
    #[test]
    fn an_answer_in_two_records_books_every_line_and_a_repeated_line_once() {
        let (_dir, e, mut j) = grove("camp-split", 3);
        j.conflict_plugin.answer_ticks = 100_000;
        camp_in(&mut j);
        assert_eq!(slots(&e), [0, 3, 0, 0, 0]);
        let record = j
            .conflict_plugin
            .queued()
            .into_iter()
            .find_map(|o| match o {
                O::CampBodies(b) => Some(b),
                _ => None,
            })
            .expect("the answer, on its way");
        let _ = j.conflict_plugin.drop_queued();
        assert_eq!(record.lines.len(), 3);
        assert!(record.lines.iter().all(|l| l.action == cb::ACTION_ADOPT));
        let part = |lines: &[cb::LineAnswer]| {
            O::CampBodies(cb::CampBodies {
                lines: lines.to_vec(),
                ..record.clone()
            })
        };
        j.conflict_obs(&part(&record.lines[..1]));
        j.conflict_obs(&part(&record.lines[1..]));
        j.conflict_obs(&part(&record.lines[..1]));
        j.idle();
        assert_eq!(slots(&e), [3, 0, 0, 0, 0], "all three lines booked");
        assert_eq!(e.with_conflict(|c, _| c.stats.camp_answers), 3, "once each");
        assert!(
            !e.with_conflict(|c, _| c.op_applied(record.op)),
            "no op id kept"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A world save that makes an answer's record durable lets the camp forget it (MZB-177): the
    /// ledger keeps (op, line) with the record's sequence, a copy before the save is skipped, and one
    /// after it is booked again, by its slot's state (here the adopted body stays one body).
    #[test]
    fn a_durable_world_save_lets_the_camp_forget_the_answers_it_covers() {
        let (_dir, e, mut j) = grove("camp-forget", 3);
        j.conflict_plugin.answer_ticks = 100_000;
        camp_in(&mut j);
        let record = j
            .conflict_plugin
            .queued()
            .into_iter()
            .find_map(|o| match o {
                O::CampBodies(b) => Some(b),
                _ => None,
            })
            .expect("the answer, on its way");
        let _ = j.conflict_plugin.drop_queued();
        let part = |lines: &[cb::LineAnswer]| {
            O::CampBodies(cb::CampBodies {
                lines: lines.to_vec(),
                ..record.clone()
            })
        };
        let kept = || {
            e.with_conflict(|c, _| {
                c.camp(c.faction_of_key(CAMP).unwrap())
                    .unwrap()
                    .answered
                    .iter()
                    .map(|a| (a.line, a.seq))
                    .collect::<Vec<_>>()
            })
        };
        let first = j.conflict_obs(&part(&record.lines[..2]));
        let copy = j.conflict_obs(&part(&record.lines[..1]));
        j.idle();
        assert_eq!(slots(&e), [2, 1, 0, 0, 0]);
        assert_eq!(
            e.with_conflict(|c, _| c.stats.camp_answers),
            2,
            "the copy skipped"
        );
        assert_eq!(
            kept(),
            vec![(record.lines[0].line, copy), (record.lines[1].line, first)],
            "kept with the record's sequence, a copy's included"
        );
        // A save durable through the first record only: the copy keeps line 0.
        let durable = |j: &mut Java, through: u64| {
            let Some(r) = j.reserve(obs::WORLD_SAVED, obs::world_saved::SIZE, 0) else {
                panic!()
            };
            wire::put_u64(r, obs::world_saved::DURABLE_OBS_SEQ, through);
            j.idle();
        };
        durable(&mut j, first);
        j.conflict_obs(&part(&record.lines[2..]));
        j.idle();
        assert_eq!(slots(&e), [3, 0, 0, 0, 0]);
        assert_eq!(
            kept().iter().map(|k| k.0).collect::<Vec<_>>(),
            vec![record.lines[0].line, record.lines[2].line],
            "line 1 (first record only) forgotten, line 0 kept by its copy"
        );
        // A save past the copy: the next answer forgets it too, and its copy books again.
        let now = j.next_obs - 1;
        durable(&mut j, now);
        let again = j.conflict_obs(&part(&record.lines[..1]));
        j.idle();
        assert_eq!(kept(), vec![(record.lines[0].line, again)]);
        assert_eq!(
            e.with_conflict(|c, _| c.stats.camp_answers),
            4,
            "the copy after the save is booked again, by state"
        );
        assert_eq!(slots(&e), [3, 0, 0, 0, 0], "nothing moves");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A camp's record carried on a holder comes back as a REASSERT (docs/boundary.md section 3.2,
    /// MZB-652), and the engine judges the copy by the generation that journaled it, not by the
    /// record's (op, line): a copy of a record it applied is dropped even where the camp has
    /// forgotten the line; a copy from a generation it never heard is booked once, under the copy's
    /// own sequence; and a copy journaled in the last session and replayed stays inside the replay
    /// window, so the replayed census after it still only marks its camp out.
    #[test]
    fn a_camp_record_a_holder_journals_again_is_judged_by_its_generation() {
        use std::sync::atomic::Ordering;
        /// A generation no engine heard: the one a crash lost with its journal.
        const EARLIER_GEN: u64 = 0x0E41_11E4_0000_0001;
        let copies = |e: &Engine| {
            (
                e.stats.copies_applied.load(Ordering::Acquire),
                e.stats.copies_dropped.load(Ordering::Acquire),
            )
        };
        let answers = |e: &Engine| e.with_conflict(|c, _| c.stats.camp_answers);
        let kept = |e: &Engine| {
            e.with_conflict(|c, _| {
                c.camp(c.faction_of_key(CAMP).unwrap())
                    .unwrap()
                    .answered
                    .iter()
                    .map(|a| (a.line, a.seq))
                    .collect::<Vec<_>>()
            })
        };
        // The record as a holder kept it: one op's answer, split in lines, journaled by a session
        // the engine never heard (its sequence is that session's).
        let held = |j: &Java, o: &O, seq: u64| {
            let mut b = Vec::new();
            cb::encode_obs(0, j.game_time, o, &mut b);
            wire::put_u64(&mut b, obs::OBS_SEQ, seq);
            b
        };
        let answer = |j: &mut Java| {
            let record = j
                .conflict_plugin
                .queued()
                .into_iter()
                .find_map(|o| match o {
                    O::CampBodies(b) => Some(b),
                    _ => None,
                })
                .expect("the answer, on its way");
            let _ = j.conflict_plugin.drop_queued();
            record
        };
        let part = |record: &cb::CampBodies, lines: &[cb::LineAnswer]| {
            O::CampBodies(cb::CampBodies {
                lines: lines.to_vec(),
                ..record.clone()
            })
        };

        // A dropped copy books nothing, and an unknown one is booked once.
        let (_dir, e, mut j) = grove("camp-copies", 3);
        j.conflict_plugin.answer_ticks = 100_000;
        camp_in(&mut j);
        let record = answer(&mut j);
        assert_eq!(record.lines.len(), 3);
        let first = j.conflict_obs(&part(&record, &record.lines[..1]));
        let original = j.journaled(first).unwrap();
        let nonce = j.nonce;
        j.idle();
        assert_eq!((answers(&e), slots(&e)), (1, [1, 2, 0, 0, 0]));
        // A save durable through it, and the camp's next answer forgets the line (MZB-177): a plain
        // resend of the line would book it again, by state (the test above).
        let Some(r) = j.reserve(obs::WORLD_SAVED, obs::world_saved::SIZE, 0) else {
            panic!()
        };
        wire::put_u64(r, obs::world_saved::DURABLE_OBS_SEQ, first);
        j.idle();
        j.conflict_obs(&part(&record, &record.lines[2..]));
        j.idle();
        assert_eq!((answers(&e), slots(&e)), (2, [2, 1, 0, 0, 0]));
        assert_eq!(
            kept(&e).iter().map(|k| k.0).collect::<Vec<_>>(),
            vec![record.lines[2].line],
            "line 0 forgotten"
        );
        let before = kept(&e);
        j.reassert(nonce, &original);
        j.idle();
        assert_eq!(copies(&e), (0, 1), "a copy of the open generation's record");
        assert_eq!(
            answers(&e),
            2,
            "dropped: nothing booked, though the line is forgotten"
        );
        assert_eq!(slots(&e), [2, 1, 0, 0, 0]);
        assert_eq!(kept(&e), before);

        // A record of an earlier generation, whose original the crash lost: the copy is all of it.
        let lost = held(&j, &part(&record, &record.lines[1..2]), 7);
        let copy = j.reassert(EARLIER_GEN, &lost);
        j.idle();
        assert_eq!(copies(&e), (1, 1));
        assert_eq!((answers(&e), slots(&e)), (3, [3, 0, 0, 0, 0]));
        assert_eq!(
            kept(&e).iter().find(|k| k.0 == record.lines[1].line),
            Some(&(record.lines[1].line, copy)),
            "booked under the copy's sequence, not the record's own (7)"
        );
        j.reassert(EARLIER_GEN, &lost);
        j.idle();
        assert_eq!(copies(&e), (1, 2), "the second copy of it is dropped");
        assert_eq!((answers(&e), slots(&e)), (3, [3, 0, 0, 0, 0]));
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();

        // A copy the last session journaled, replayed: inside the replay window.
        let (dir, e, mut j) = grove("camp-copies-replay", 3);
        j.conflict_plugin.answer_ticks = 100_000;
        save(&e, &mut j);
        camp_in(&mut j);
        let record = answer(&mut j);
        let lost = held(&j, &part(&record, &record.lines[..1]), 7);
        j.reassert(EARLIER_GEN, &lost);
        // A later census, journaled after the copy: live, it would order a reconcile.
        let at = rtsim_kernel::clock::Tick(j.game_time.max(0) as u64);
        j.conflict_plugin.camp_out(CAMP, at);
        j.conflict_flush();
        j.idle();
        camp_in(&mut j);
        assert_eq!(
            copies(&e),
            (1, 0),
            "applied in the session that journaled it"
        );
        let gt = j.game_time;
        let saved = j.world_save();
        let mut plugin =
            std::mem::replace(&mut j.conflict_plugin, synth::FakeConflict::new(PLAYER));
        let _ = plugin.drop_queued();
        for camp in plugin.camps.values_mut() {
            camp.epoch = 0;
        }
        let _ = j.crash();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.conflict_plugin = plugin;
        assert_eq!(e.conflict_restore_check(), (None, false));
        assert_eq!(slots(&e), [0, 3, 0, 0, 0], "the pages predate the census");
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(copies(&e), (1, 0), "the replay applied the copy, once");
        assert_eq!(
            slots(&e),
            [1, 2, 0, 0, 0],
            "the copy's line, booked by state"
        );
        assert_eq!(
            reconciles(&j).len(),
            0,
            "the census after the copy was replayed, not live"
        );
        assert!(
            !e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())),
            "its camp is out"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// CAMP_OUT ends the epoch: the lines in flight are void, and a late answer for them is booked by
    /// the world fact it reports, not dropped, and a CAMP_OUT for another epoch ends nothing.
    #[test]
    fn a_camp_out_voids_the_lines_and_a_late_answer_is_booked_by_state() {
        let (_dir, e, mut j) = grove("camp-out", 3);
        j.conflict_plugin.answer_ticks = 5_000;
        let epoch = camp_in(&mut j);
        assert_eq!(slots(&e), [0, 3, 0, 0, 0], "the answers are on their way");
        let in_flight =
            || e.with_conflict(|c, _| c.camp(c.faction_of_key(CAMP).unwrap()).unwrap().in_flight());
        assert_eq!(in_flight(), 3);
        // A CAMP_OUT for an older epoch: nothing.
        j.conflict_obs(&O::CampOut(cb::CampOut {
            faction: CAMP,
            epoch: epoch + 1,
        }));
        j.idle();
        assert_eq!(in_flight(), 3);
        let at = rtsim_kernel::clock::Tick(j.game_time.max(0) as u64);
        j.conflict_plugin.camp_out(CAMP, at);
        j.conflict_flush();
        j.idle();
        assert_eq!(in_flight(), 0, "void");
        assert!(!e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())));
        camp_until(&mut j, "the late answers", |_| slots(&e) == [3, 0, 0, 0, 0]);
        assert_eq!(reconciles(&j).len(), 1, "nothing was ordered again");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A crash with lines in flight (MOWZIE.md §4.9's crash table): the pages keep the lines, the
    /// restart voids them and marks every camp out; the journal's census, replayed, only marks the
    /// camp out and orders nothing; the camp's next census finds the adopted bodies (the plugin's
    /// world applied them, the answers were lost) and books them by state, with no second order.
    #[test]
    fn a_crash_with_lines_in_flight_is_repaired_by_the_next_census() {
        let (dir, e, mut j) = grove("camp-crash", 3);
        j.conflict_plugin.answer_ticks = 100_000;
        camp_in(&mut j);
        assert_eq!(slots(&e), [0, 3, 0, 0, 0]);
        assert_eq!(reconciles(&j).len(), 1);
        save(&e, &mut j);
        let gt = j.game_time;
        let saved = j.world_save();
        // The plugin's world keeps what it applied; the answers die with the session.
        let mut plugin =
            std::mem::replace(&mut j.conflict_plugin, synth::FakeConflict::new(PLAYER));
        let _ = plugin.drop_queued();
        for camp in plugin.camps.values_mut() {
            camp.epoch = 0;
        }
        let _ = j.crash();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        plugin.answer_ticks = 0;
        j.conflict_plugin = plugin;
        assert_eq!(e.conflict_restore_check(), (None, false));
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert!(
            !e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())),
            "out"
        );
        assert_eq!(
            e.with_conflict(|c, _| c.camp(c.faction_of_key(CAMP).unwrap()).unwrap().in_flight()),
            0,
            "void"
        );
        assert_eq!(
            reconciles(&j).len(),
            0,
            "the replayed census ordered nothing"
        );
        assert_eq!(slots(&e), [0, 3, 0, 0, 0]);
        // The camp comes in again: the bodies the lost lines adopted are found.
        camp_in(&mut j);
        assert_eq!(slots(&e), [3, 0, 0, 0, 0], "found, booked by state");
        assert_eq!(reconciles(&j).len(), 0, "no second order for them");
        assert_eq!(j.conflict_plugin.camps[&CAMP].bodies.len(), 3);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The crash table's journal rows through the real replay: the pages predate the census, the
    /// journal holds the census and the answers; after the crash the replayed census only marks its
    /// camp out (it orders nothing: its bodies are not the world as it is now), the replayed answers
    /// are booked by state, and the camp's next census finds the books and the world agree.
    #[test]
    fn a_replayed_census_orders_nothing_and_its_answers_are_booked_by_state() {
        let (dir, e, mut j) = grove("camp-replay", 3);
        save(&e, &mut j);
        camp_in(&mut j);
        camp_until(&mut j, "the adoptions answered", |_| {
            slots(&e) == [3, 0, 0, 0, 0]
        });
        assert_eq!(reconciles(&j).len(), 1);
        // What a holder of the answers keeps: its generation and the records as journaled.
        let (nonce, answers): (u64, Vec<Vec<u8>>) = (
            j.nonce,
            j.journal
                .iter()
                .filter(|(_, r)| wire::u16_at(r, wire::R_KIND) == cb::CAMP_BODIES)
                .map(|(_, r)| r.clone())
                .collect(),
        );
        assert!(!answers.is_empty());
        let gt = j.game_time;
        let saved = j.world_save();
        let mut plugin =
            std::mem::replace(&mut j.conflict_plugin, synth::FakeConflict::new(PLAYER));
        for camp in plugin.camps.values_mut() {
            camp.epoch = 0;
        }
        let _ = j.crash();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.conflict_plugin = plugin;
        assert_eq!(e.conflict_restore_check(), (None, false));
        assert_eq!(slots(&e), [0, 3, 0, 0, 0], "the pages predate the census");
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(
            reconciles(&j).len(),
            0,
            "the replayed census ordered nothing"
        );
        assert!(
            !e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())),
            "out"
        );
        assert_eq!(
            slots(&e),
            [3, 0, 0, 0, 0],
            "the replayed answers, booked by state"
        );
        // RP-69: nothing the gate never lets through was decided in the replay; and a live
        // holder's copy (REASSERT) of each answer, for the replayed census's epoch, books nothing
        // again.
        let never = |e: &Engine| e.with_conflict(|c, _| c.replay_gate_counts().never);
        assert_eq!(never(&e), 0);
        let booked = e.with_conflict(|c, _| c.stats.camp_answers);
        for r in &answers {
            j.reassert(nonce, r);
        }
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(slots(&e), [3, 0, 0, 0, 0]);
        assert_eq!(
            e.with_conflict(|c, _| c.stats.camp_answers),
            booked,
            "the copies"
        );
        assert_eq!(reconciles(&j).len(), 0);
        camp_in(&mut j);
        assert_eq!(slots(&e), [3, 0, 0, 0, 0]);
        assert_eq!(reconciles(&j).len(), 0, "nothing to order");
        assert_eq!(j.conflict_plugin.camps[&CAMP].bodies.len(), 3);
        assert_eq!(never(&e), 0);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// RP-69 (the M5 merge check's item 5): the conflict's replay begun late. The pages predate
    /// the camp, so the conflict does not run at SESSION_START (conflict.rs `reset_if_due`); the
    /// replayed FACTION_FOUND starts it and opens the window, and the camp's census replayed after
    /// it only marks the camp out, as in a replay begun at the session start.
    #[test]
    fn a_replay_begun_by_a_replayed_faction_only_marks_its_census_out() {
        let (dir, e, mut j) = grove_with("camp-replay-late", 3, true);
        camp_in(&mut j);
        camp_until(&mut j, "the adoptions answered", |_| {
            slots(&e) == [3, 0, 0, 0, 0]
        });
        assert_eq!(reconciles(&j).len(), 1);
        let gt = j.game_time;
        let saved = j.world_save();
        let mut plugin =
            std::mem::replace(&mut j.conflict_plugin, synth::FakeConflict::new(PLAYER));
        for camp in plugin.camps.values_mut() {
            camp.epoch = 0;
        }
        let _ = j.crash();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.conflict_plugin = plugin;
        assert!(
            e.with_conflict(|c, _| c.faction_of_key(CAMP).is_none()),
            "the pages predate the camp"
        );
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        assert_eq!(
            reconciles(&j).len(),
            0,
            "the replayed census ordered nothing"
        );
        assert!(
            !e.with_conflict(|c, _| c.camp_is_in(c.faction_of_key(CAMP).unwrap())),
            "out"
        );
        assert_eq!(
            slots(&e),
            [3, 0, 0, 0, 0],
            "the replayed answers, booked by state"
        );
        assert_eq!(e.with_conflict(|c, _| c.replay_gate_counts().never), 0);
        camp_in(&mut j);
        assert_eq!(slots(&e), [3, 0, 0, 0, 0]);
        assert_eq!(reconciles(&j).len(), 0, "nothing to order");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Runs the camp's feud with `PLAYER` to its envoy, embodied in view.
    fn envoy_in_view(j: &mut Java) -> cb::EnvoyMaterialize {
        signal(j, 2, 300, PLAYER);
        advance_until(j, 600, 2 * DAY, "an envoy in view", |j| {
            cmds(j).any(|c| matches!(c, C::Envoy(_)))
        });
        let envoy = cmds(j)
            .find_map(|c| match c {
                C::Envoy(x) => Some(x.clone()),
                _ => None,
            })
            .unwrap();
        j.game_time += 20;
        j.conflict_flush();
        j.idle();
        envoy
    }

    fn wipe(j: &mut Java, envoy: &cb::EnvoyMaterialize, by: u32, killer: (u64, u64)) {
        for &(body, count) in &envoy.members {
            j.conflict_obs(&O::Casualties(cb::BandCasualties {
                agent: envoy.agent,
                epoch: envoy.epoch,
                by,
                body,
                count,
                killer,
            }));
        }
        j.idle();
    }

    /// MOWZIE.md ledger 14: an envoy wiped out by no player (a mob) withdraws its
    /// demand: DEMAND_SETTLED outcome 5, the feud stays on Envoy with no climb. Killed by a
    /// player, the demand stays open.
    #[test]
    fn an_envoy_wiped_out_by_a_mob_withdraws_its_demand() {
        let (_dir, e, mut j) = camp("conflict-withdrawn");
        let envoy = envoy_in_view(&mut j);
        assert!(feud(&e, PLAYER).unwrap().demand.is_some());
        wipe(&mut j, &envoy, cb::BY_MOB, (0, 0));
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let settled: Vec<cb::DemandSettled> = cmds(&j)
            .filter_map(|c| match c {
                C::Settled(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(settled.len(), 1, "{settled:?}");
        assert_eq!(
            (settled[0].outcome, settled[0].faction, settled[0].player),
            (cb::SETTLED_WITHDRAWN, CAMP, PLAYER)
        );
        let f = feud(&e, PLAYER).unwrap();
        assert!(f.demand.is_none(), "withdrawn: {f:?}");
        assert_eq!(f.step, Step::Envoy, "the feud stays on Envoy");
        assert!(
            !cmds(&j).any(|c| matches!(c, C::Recall(r) if r.agent == envoy.agent)),
            "nobody is left to recall (MZB-32)"
        );
        let (st, _) = e.conflict_stats();
        assert_eq!((st.withdrawn, st.expired), (1, 0));
        e.close(true, Duration::from_secs(5)).unwrap();

        let (_dir, e, mut j) = camp("conflict-envoy-killed");
        let envoy = envoy_in_view(&mut j);
        wipe(&mut j, &envoy, cb::BY_PLAYER, PLAYER);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert!(
            !cmds(&j).any(|c| matches!(c, C::Settled(s) if s.outcome == cb::SETTLED_WITHDRAWN)),
            "a player's killing withdraws nothing"
        );
        assert!(
            feud(&e, PLAYER).unwrap().demand.is_some(),
            "the demand stands"
        );
        assert_eq!(e.conflict_stats().0.withdrawn, 0);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Orders a war party of `kind` from the camp at `BASE` for `PLAYER`'s feud, now, the feud on
    /// `step` (made first by a small signal).
    fn order(e: &Engine, j: &mut Java, kind: PartyKind, step: Step) -> Handle<Party> {
        order_to(e, j, kind, step, BASE)
    }

    /// [`order`] toward a place of its own.
    fn order_to(
        e: &Engine,
        j: &mut Java,
        kind: PartyKind,
        step: Step,
        to: (i32, i32),
    ) -> Handle<Party> {
        signal(j, 2, 30, PLAYER);
        j.idle();
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD) as u16;
        let at = rtsim_kernel::clock::Tick(j.game_time.max(0) as u64);
        let p = e.with_conflict_mut(|c, w| {
            let a = c.player(w, PLAYER);
            let at = at.max(c.now());
            let target = Target::Pos(Pos::new(dim, to.0, to.1));
            let p = c
                .order_party(w, CAMP, kind, target, Some(a), at)
                .expect("ordered");
            let f = c.faction_of_key(CAMP).expect("the camp");
            c.feud_mut(f, a).expect("the feud").step = step;
            p
        });
        j.idle();
        p
    }

    /// Puts `PLAYER`'s feud on `step` now, still served by `p`, with a grievance over every
    /// threshold so that no step falls by decay within the test (the small signal's would).
    fn on_step(e: &Engine, p: Handle<Party>, step: Step) {
        e.with_conflict_mut(|c, w| {
            let a = c.player(w, PLAYER);
            let f = c.faction_of_key(CAMP).expect("the camp");
            let now = c.now();
            let x = c.feud_mut(f, a).expect("the feud");
            assert_eq!(x.party, Some(p), "the party serves the feud");
            x.step = step;
            x.g = rtsim_conflict::Grievance::new(now);
            x.g.level[rtsim_conflict::SignalKind::Killing.index()] = 400.0;
        });
    }

    fn party(e: &Engine, p: Handle<Party>) -> Option<(Phase, bool)> {
        e.with_conflict(|c, _| c.parties.get(p).map(|x| (x.phase, x.embodied)))
    }

    fn recalls(j: &Java, from: usize, p: Handle<Party>) -> Vec<u32> {
        j.conflict[from..]
            .iter()
            .filter_map(|(_, c)| match c {
                C::Recall(r) if r.agent == Party::agent(p) => Some(r.reason),
                _ => None,
            })
            .collect()
    }

    /// MZB-15 (review): Peaceful stops a raid already out on the road, embodied in view. It goes
    /// home with RECALL_SETTLED, its feud a rung back, and no body of it spawns again.
    #[test]
    fn peaceful_turns_back_an_embodied_raid_on_the_road() {
        let (_dir, e, mut j) = camp("conflict-peaceful-out");
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        let from = j.conflict.len();
        let held = e.conflict_stats().0.peace_held;
        on_step(&e, p, Step::Raid);
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert_eq!(party(&e, p).map(|x| x.0), Some(Phase::Home), "turned back");
        assert_eq!(recalls(&j, from, p), vec![cb::RECALL_SETTLED]);
        assert_eq!(e.conflict_stats().0.peace_held, held + 1);
        assert_eq!(
            feud(&e, PLAYER).unwrap().step,
            Step::Scouts,
            "a rung below the raid it never made"
        );
        let from = j.conflict.len();
        j.game_time += 20;
        advance_until(&mut j, 600, 2 * DAY, "home", |j| party(&j.e, p).is_none());
        assert!(
            !j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Materialize(m) if m.agent == Party::agent(p))),
            "no body of it spawns on Peaceful"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-15 (review): Peaceful ends a watched fight with RECALL_SETTLED; the feud keeps its
    /// step (the raid came).
    #[test]
    fn peaceful_ends_a_watched_fight() {
        let (_dir, e, mut j) = camp("conflict-peaceful-fight");
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        advance_until(&mut j, 100, 3 * DAY, "the raid fighting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Fighting)
        });
        let from = j.conflict.len();
        let held = e.conflict_stats().0.peace_held;
        on_step(&e, p, Step::Raid);
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert_eq!(
            party(&e, p).map(|x| x.0),
            Some(Phase::Home),
            "the fight ended"
        );
        assert_eq!(recalls(&j, from, p), vec![cb::RECALL_SETTLED]);
        assert_eq!(e.conflict_stats().0.peace_held, held + 1);
        assert_eq!(feud(&e, PLAYER).unwrap().step, Step::Raid);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-15 (review): Peaceful sends home a siege waiting at a place nobody watches, with no
    /// recall (nothing of it is in Minecraft); the feud keeps its step.
    #[test]
    fn peaceful_sends_home_a_waiting_siege() {
        let (_dir, e, mut j) = camp("conflict-peaceful-siege");
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.players = vec![(PLAYER, dim, [20_000.5, 64.0, 20_000.5])];
        j.idle();
        let p = order(&e, &mut j, PartyKind::Siege, Step::Siege);
        advance_until(&mut j, 600, 4 * DAY, "the siege waiting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Waiting)
        });
        let from = j.conflict.len();
        let held = e.conflict_stats().0.peace_held;
        on_step(&e, p, Step::Siege);
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        assert_eq!(party(&e, p).map(|x| x.0), Some(Phase::Home), "sent home");
        assert!(recalls(&j, from, p).is_empty(), "nothing to recall");
        assert_eq!(e.conflict_stats().0.peace_held, held + 1);
        assert_eq!(feud(&e, PLAYER).unwrap().step, Step::Siege);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The party's alive count, its epoch, whether it is in view, whether the engine still
    /// waits for a walk-off's count, and its bodies in Minecraft by the engine's books.
    fn books(e: &Engine, p: Handle<Party>) -> (u32, u32, bool, bool, u32) {
        e.with_conflict(|c, _| {
            let x = &c.parties[p];
            (
                x.alive(),
                x.epoch,
                x.embodied,
                !x.recalled.is_empty(),
                x.spawned.iter().map(|s| s.1).sum(),
            )
        })
    }

    /// MZB-131 (item 12): a band passing by is recalled (RECALL_PASSING) and the plugin walks its
    /// bodies off; a player ahead sees it again before the despawn, at the next epoch. Two of the
    /// old epoch's members die on the walk, so its BAND_DEMATERIALIZED, which comes while the
    /// new epoch is in view, counts them: the engine had dropped it for its old epoch and never
    /// cleared the recall, so the dead came back, and a later recall (the player gone stale) took
    /// the whole band's count with it.
    #[test]
    fn a_band_seen_again_while_it_walks_off_still_counts_the_walks_dead_once() {
        let (_dir, e, mut j) = camp("conflict-walk-off");
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        // The walk off outlasts the band's next sighting.
        j.conflict_plugin.leave_ticks = 3_000;
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        let epoch1 = books(&e, p).1;
        assert_eq!(books(&e, p).4, n, "all of it in Minecraft");

        // It leaves the player's view: RECALL_PASSING, and two die on the way out.
        let from = j.conflict.len();
        j.conflict_plugin.walk_off_deaths = 2;
        advance_until(&mut j, 100, 3 * DAY, "RECALL_PASSING", |j| {
            recalls(j, from, p).contains(&cb::RECALL_PASSING)
        });
        j.conflict_plugin.walk_off_deaths = 0;
        let recalled_at = j.game_time;
        assert!(!books(&e, p).2, "recalled: out of view");
        assert!(books(&e, p).3, "its epoch's walk-off to count");

        // A player ahead on its road sees it again, a new epoch, with the walk-off in flight.
        let ahead = e.with_conflict(|c, _| c.party_pos(p, c.now() + 3_000));
        j.players = vec![(
            PLAYER,
            dim,
            [f64::from(ahead.0) + 0.5, 64.0, f64::from(ahead.1) + 0.5],
        )];
        advance_until(&mut j, 100, 2_000, "in view again at a new epoch", |j| {
            let b = books(&j.e, p);
            b.2 && b.1 > epoch1
        });
        assert!(
            j.game_time < recalled_at + 3_000,
            "seen again before the old epoch's despawn came"
        );
        assert_eq!(books(&e, p).0, n, "the walk's dead are not known yet");
        assert_eq!(books(&e, p).4, n, "spawned from the count before");

        // The old epoch's BAND_DEMATERIALIZED: the dead are counted, the new bodies clamped.
        advance_until(&mut j, 100, 4_000, "past the old walk-off", |j| {
            j.game_time >= recalled_at + 3_000 + 200
        });
        let (alive, epoch2, embodied, recalled, spawned) = books(&e, p);
        assert_eq!(alive, n - 2, "the two that died on the walk are dead");
        assert!(
            embodied && epoch2 > epoch1,
            "still in view at the new epoch"
        );
        assert!(!recalled, "the old epoch's walk-off is counted");
        assert!(spawned <= alive, "its bodies are clamped to who is alive");

        // The player goes stale: the second recall counts nothing it did not lose.
        let second = j.conflict.len();
        j.players.clear();
        advance_until(&mut j, 100, 2_000, "the second recall", |j| {
            !recalls(j, second, p).is_empty()
        });
        assert_eq!(recalls(&j, second, p), vec![cb::RECALL_PLAYER_LEFT]);
        let second_at = j.game_time;
        advance_until(&mut j, 100, 4_000, "past the second walk-off", |j| {
            j.game_time >= second_at + 3_000 + 200
        });
        let (alive, _, embodied, recalled, _) = books(&e, p);
        assert_eq!(alive, n - 2, "no one else is lost");
        assert!(!embodied);
        assert!(!recalled, "the second walk-off is counted too");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-132 (review): a crash in the middle of Peaceful, the envoy held where it arrived, and
    /// the world leaving Peaceful from the engine the pages loaded: the demand's deadline moves by
    /// the whole span (the part before the crash and the part after), the envoy comes back
    /// carrying it, and the demand comes due there and not before.
    #[test]
    fn a_crash_in_the_middle_of_peaceful_then_leaving_it_moves_the_deadline_by_the_whole_span() {
        let (dir, e, mut j) = camp("conflict-peaceful-crash");
        let envoy = envoy_in_view(&mut j);
        let d0 = envoy.deadline;
        // The killing goes on: a demand cut at its deadline would not be an expired one.
        signal(&mut j, 2, 5, PLAYER);
        j.idle();
        difficulty(&mut j, cb::DIFFICULTY_PEACEFUL);
        let since = e.with_conflict(|c, _| c.peaceful_since);
        advance_until(&mut j, 1_000, 4 * DAY, "past the deadline", |j| {
            j.game_time >= d0 + DAY
        });
        let state = |e: &Engine| {
            e.with_conflict(|c, _| {
                c.parties
                    .iter()
                    .find(|(_, x)| x.kind == PartyKind::Envoy)
                    .map(|(_, x)| (x.phase, x.embodied))
            })
        };
        assert_eq!(state(&e), Some((Phase::Waiting, false)), "held, abstract");
        save(&e, &mut j);
        j.game_time += DAY / 2;
        let gt = j.game_time;
        let saved = j.crash();

        let (e, mut j) = reopen(&dir, saved, gt);
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.players = vec![(
            PLAYER,
            dim,
            [f64::from(BASE.0) + 0.5, 64.0, f64::from(BASE.1) + 0.5],
        )];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        assert!(e.with_conflict(|c, _| c.peaceful), "still Peaceful");
        assert_eq!(e.with_conflict(|c, _| c.peaceful_since), since);
        assert_eq!(state(&e), Some((Phase::Waiting, false)), "held, abstract");
        let demand = |e: &Engine| feud(e, PLAYER).and_then(|f| f.demand).map(|d| d.3 as i64);
        assert_eq!(demand(&e), Some(d0), "unmoved by the restart");
        // The killing goes on.
        signal(&mut j, 2, 5, PLAYER);
        j.idle();

        let from = j.conflict.len();
        let at = rtsim_kernel::clock::Tick(j.game_time as u64);
        difficulty(&mut j, cb::DIFFICULTY_NORMAL);
        let d1 = d0 + at.since(since) as i64;
        assert_eq!(demand(&e), Some(d1), "moved by the whole span");
        advance_until(&mut j, 100, 4_000, "the envoy in view again", |j| {
            j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Envoy(_)))
        });
        let again = j.conflict[from..]
            .iter()
            .find_map(|(_, c)| match c {
                C::Envoy(x) => Some(x.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(again.deadline, d1, "it carries the moved deadline");
        assert_eq!(again.agent, envoy.agent, "the same envoy");
        let limit = d1 - j.game_time + DAY;
        advance_until(&mut j, 1_000, limit, "the demand due", |j| {
            let due = j.conflict[from..]
                .iter()
                .any(|(_, c)| matches!(c, C::Settled(s) if s.outcome == cb::SETTLED_EXPIRED));
            assert!(
                !due || j.game_time >= d1,
                "expired before d1 ({d1}): {}",
                j.game_time
            );
            due
        });
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The conflict engine from the pages in `dir`, as the next session opens it over `saved`.
    fn reopen(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
    ) -> (std::sync::Arc<Engine>, Java) {
        reopen_with(dir, saved, gt, 256 * 1024)
    }

    /// [`reopen`] with an observation buffer of `capacity` bytes: a small one spreads a long
    /// journal's replay over several hand-overs.
    fn reopen_with(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        capacity: usize,
    ) -> (std::sync::Arc<Engine>, Java) {
        reopen_mask(dir, saved, gt, capacity, MODULES)
    }

    /// [`reopen_with`] with the module switches `mask`.
    fn reopen_mask(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        capacity: usize,
        mask: u32,
    ) -> (std::sync::Arc<Engine>, Java) {
        let e = engine(dir, capacity);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = mask;
        j.game_time = gt;
        assert_eq!(e.conflict_restore_check(), (None, false));
        // Java hands its journal over while it fits: until it has sent every record.
        let last = j.world_save().journal.back().map_or(0, |(s, _)| *s);
        for _ in 0..64 {
            j.idle();
            if j.sent_obs >= last {
                break;
            }
        }
        assert!(j.sent_obs >= last, "the whole journal handed over");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        (e, j)
    }

    /// MZB-138 (review item 2, older than item 12): a raid in view at a save, recalled after it
    /// (the player left) with two members dying on the walk-off, its BAND_DEMATERIALIZED
    /// journaled, and then a crash. The restart replays the despawn: the session start used to
    /// clear the recalled books before the replay, so the despawn was stale and the two dead
    /// came back. Now the bodies the pages held in Minecraft are kept for the replay, the despawn
    /// counts the dead, and nothing is left kept once the replay has caught up.
    #[test]
    fn a_walk_offs_dead_are_counted_again_by_the_replay_after_a_crash() {
        let (dir, e, mut j) = camp("conflict-walk-off-crash");
        j.conflict_plugin.leave_ticks = 1_500;
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        assert_eq!(books(&e, p).4, n, "all of it in Minecraft at the save");
        save(&e, &mut j);

        // The player leaves: it is recalled and two die on the walk off; the despawn is
        // journaled, no page holds it.
        let from = j.conflict.len();
        j.conflict_plugin.walk_off_deaths = 2;
        j.players.clear();
        advance_until(&mut j, 100, 2 * DAY, "the recall", |j| {
            !recalls(j, from, p).is_empty()
        });
        let recalled_at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the despawn journaled", |j| {
            j.game_time >= recalled_at + 1_500 + 200
        });
        assert_eq!(books(&e, p).0, n - 2, "the session that went on");
        assert!(!books(&e, p).3, "its walk-off counted");
        let gt = j.game_time;
        let saved = j.crash();

        let (e, j) = reopen(&dir, saved, gt);
        let (alive, _, embodied, recalled, spawned) = books(&e, p);
        assert_eq!(alive, n - 2, "the replay counts the walk off's dead");
        assert!(!embodied && spawned == 0, "no body survived the crash");
        assert!(!recalled, "nothing kept once the replay has caught up");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-138: a raid in view at a save, fought after it (the scripted player kills half of it)
    /// and then a crash with no despawn journaled: the replayed casualties are counted once, the
    /// members who died stay dead, and the rest count as alive as they always did.
    #[test]
    fn casualties_journaled_after_a_save_of_a_band_in_view_are_counted_by_the_replay() {
        let (dir, e, mut j) = camp("conflict-casualties-crash");
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.5,
            after: 1_500,
            kill_chief: false,
            only_at_base: false,
        });
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        assert_eq!(books(&e, p).0, n, "no one has fallen yet");
        save(&e, &mut j);
        let at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= at + 1_500 + 200
        });
        let lost = n - books(&e, p).0;
        assert!(lost >= 2, "the fight cost members: {lost}");
        let gt = j.game_time;
        let saved = j.crash();

        let (e, j) = reopen(&dir, saved, gt);
        let (alive, _, embodied, recalled, _) = books(&e, p);
        assert_eq!(alive, n - lost, "the replay counts each death once");
        assert!(!embodied);
        assert!(!recalled, "nothing kept once the replay has caught up");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// MZB-141 (re-review P2-1): the replay of the walk-off test, but with 1,500 records between
    /// the recall and the despawn on an engine whose buffers hold about a thousand of them, so
    /// the despawn comes in a later hand-over than the session start. The conflict module used to
    /// end the replay (`replay_done`, which clears the bodies kept for it) after the first
    /// hand-over, and the despawn found nothing to count: the two dead came back.
    #[test]
    fn a_replay_over_several_hand_overs_still_counts_the_walk_offs_dead() {
        let (dir, e, mut j) = camp("conflict-walk-off-long-replay");
        j.conflict_plugin.leave_ticks = 1_500;
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        save(&e, &mut j);

        let from = j.conflict.len();
        j.conflict_plugin.walk_off_deaths = 2;
        j.players.clear();
        advance_until(&mut j, 100, 2 * DAY, "the recall", |j| {
            !recalls(j, from, p).is_empty()
        });
        // Filler a replay must read through: casualties of a band the engine never had (64 bytes
        // each, so about a thousand fill a minimum-size buffer), journaled before the despawn.
        let first = j.world_save().next_obs;
        for _ in 0..1_500 {
            j.conflict_obs(&O::Casualties(cb::BandCasualties {
                agent: 0,
                epoch: 0,
                by: cb::BY_MOB,
                body: 0,
                count: 1,
                killer: (0, 0),
            }));
        }
        let recalled_at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the despawn journaled", |j| {
            j.game_time >= recalled_at + 1_500 + 200
        });
        assert_eq!(books(&e, p).0, n - 2, "the session that went on");
        let gt = j.game_time;
        let saved = j.crash();
        let bytes = saved
            .journal
            .iter()
            .filter(|(s, _)| *s >= first)
            .map(|(_, r)| r.len())
            .sum::<usize>();
        assert!(
            bytes > wire::MIN_CAPACITY,
            "the replay needs several hand-overs: {bytes} bytes"
        );

        let (e, j) = reopen_with(&dir, saved, gt, wire::MIN_CAPACITY);
        let (alive, _, embodied, recalled, spawned) = books(&e, p);
        assert_eq!(
            alive,
            n - 2,
            "the despawn in a later hand-over still counts the dead"
        );
        assert!(!embodied && spawned == 0, "no body survived the crash");
        assert!(!recalled, "nothing kept once the replay has caught up");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    // ---- the replay gate (docs/replay-gate.md, fix/replay-push) ----------------------------

    /// A buffer that holds a whole long replay in one hand-over.
    const ONE_HAND_OVER: usize = 1024 * 1024;

    /// Filler a replay must read through: `count` casualties of a band the engine never had (64
    /// bytes each, so about a thousand fill a minimum-size buffer). 4,000 span four hand-overs.
    fn fillers(j: &mut Java, count: usize) {
        for _ in 0..count {
            j.conflict_obs(&O::Casualties(cb::BandCasualties {
                agent: 0,
                epoch: 0,
                by: cb::BY_MOB,
                body: 0,
                count: 1,
                killer: (0, 0),
            }));
        }
    }

    /// The camp's pool of warriors at home, and when it was last changed; and the pool at a
    /// fixed time after every test's events (its closed form, so a band pooled earlier or later
    /// gives the same number there).
    fn pool(e: &Engine) -> (f64, u64, f64) {
        e.with_conflict(|c, _| {
            let f = c.faction_of_key(CAMP).expect("the camp");
            let x = &c.factions[f as usize];
            (x.pool, x.pool_at.0, c.pool_at(f, Tick(60_000)))
        })
    }

    /// The replay ends this engine ran, and the last one's hand-overs and records.
    fn replay_counts(e: &Engine) -> (u64, u64, u64) {
        use std::sync::atomic::Ordering::Acquire;
        (
            e.stats.replays_ended.load(Acquire),
            e.stats.replay_handovers.load(Acquire),
            e.stats.replay_records.load(Acquire),
        )
    }

    /// RP-24 (the review's amendment c): a watched fight kept in the pages (Fighting at the save),
    /// its casualties journaled behind 4,000 fillers, and its end, its walk home and its arrival
    /// (`home()`, which pools the party) all after them, inside the replayed window, on a short
    /// route home (100 blocks). The session that goes on pools what is left of the band; after a
    /// crash the replay must leave the same pool. `capacity`: the next session's buffer (`None`,
    /// no crash); `mask`: the module switches of both sessions.
    fn a_fight_kept_in_the_pages_across_a_replay(name: &str, capacity: Option<usize>, mask: u32) {
        a_fight_kept_in_the_pages_with(name, capacity, mask, None);
    }

    /// The village a works grant during the replay is for: far from the camp, the raid and the
    /// players, so that it changes nothing they do.
    const GRANTED: u64 = 0x0001_0000_0000_00A1;

    /// The dragons' lair a TRIBUTE_START during the replay names: far from the camp too.
    const TRIBUTE_LAIR: u64 = 0x0000_0EEE_0000_0EEE;

    /// [`a_fight_kept_in_the_pages_across_a_replay`], with `given` (a test's order that runs the
    /// clock to the hand-over's time, RP-33) given in the replay's first hand-over, so that the
    /// engine takes it at the next step while it still replays. A works grant's village is
    /// adopted before the save; a works raid's and a tribute's is given works and a purse there
    /// too, and a tribute's lair is found. The order's effect is there once the replay has ended.
    fn a_fight_kept_in_the_pages_with(
        name: &str,
        capacity: Option<usize>,
        mask: u32,
        given: Option<Order>,
    ) {
        let (dir, e, mut j) = camp_with(name, mask);
        if let Some(
            Order::WorksGrant { village, .. }
            | Order::WorksRaid { village, .. }
            | Order::TributeStart { village, .. },
        ) = &given
        {
            j.adopt(
                *village,
                "minecraft:village_plains",
                "human",
                90,
                -4_000,
                -4_000,
            );
            j.idle();
        }
        if let Some(Order::WorksRaid { village, .. } | Order::TributeStart { village, .. }) = &given
        {
            // Works and a purse: a raid has golems to buy, a tribute coin to pay. The catalog is
            // read before the grant is taken (a step takes its orders first).
            j.work_catalog(&test_works());
            j.idle();
            e.order(vec![Order::WorksGrant {
                village: *village,
                amount: 1_000_000,
            }])
            .unwrap();
            j.idle();
        }
        if let Some(Order::TributeStart { lair, .. }) = &given {
            use rtsim_ecology::boundary as db;
            let home = (-6_000, 100, -6_000);
            let dimension = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
            j.dragon_obs(&db::Obs::LairFound(db::LairFound {
                lair: *lair,
                late: false,
                dimension,
                species: rtsim_ecology::species::Species::Fire,
                kind: rtsim_ecology::dragon::LairKind::Roost,
                min: (home.0 - 30, 60, home.2 - 30),
                max: (home.0 + 30, 140, home.2 + 30),
                home,
            }));
            j.idle();
        }
        // The player sees the band leave the camp too: only the fight's waves are fought. A
        // quarter falls, short of a rout: the fight ends sooner, not at once.
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.25,
            after: 1_500,
            kill_chief: false,
            only_at_base: true,
        });
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 8, "a band with members to lose: {n}");
        // A pool well below its cap, so that it shows the band coming home.
        e.with_conflict_mut(|c, _| {
            let f = c.faction_of_key(CAMP).expect("the camp") as usize;
            let now = c.now();
            c.factions[f].pool = 10.0;
            c.factions[f].pool_at = now;
        });
        advance_until(&mut j, 100, 3 * DAY, "the raid fighting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Fighting)
        });
        let fight_at = j.game_time;
        assert_eq!(books(&e, p).0, n, "no one has fallen yet");
        save(&e, &mut j);
        let first = j.world_save().next_obs;
        fillers(&mut j, 4_000);
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= fight_at + 1_500 + 200
        });
        let lost = n - books(&e, p).0;
        assert!(lost >= 2, "the fight cost members: {lost}");
        assert_eq!(
            party(&e, p).map(|x| x.0),
            Some(Phase::Fighting),
            "not routed"
        );
        let casualties_at = j.game_time;
        advance_until(&mut j, 100, 2 * DAY, "the band home", |j| {
            party(&j.e, p).is_none()
        });
        assert!(
            j.game_time > casualties_at + 1_000,
            "the fight's end, the walk and the arrival come after the casualties"
        );
        // Past where the fight would have ended without its losses, and the walk after it.
        let home_at = j.game_time;
        advance_until(
            &mut j,
            100,
            DAY,
            "past the unshortened fight's walk home",
            |j| j.game_time >= home_at + 2_000,
        );
        let gt = j.game_time;
        let live = pool(&e);
        let cap = e.with_conflict(|c, _| {
            let f = c.faction_of_key(CAMP).expect("the camp") as usize;
            f64::from(c.cultures.get(c.factions[f].culture).def.pool_cap)
        });
        assert!(
            live.2 + 1.0 < cap,
            "the pool is not at its cap, so it shows the band: {live:?} of {cap}"
        );
        let Some(capacity) = capacity else {
            j.idle();
            assert_eq!(pool(&e), live, "the session that went on");
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            e.close(true, Duration::from_secs(5)).unwrap();
            return;
        };
        let saved = j.crash();
        let bytes = saved
            .journal
            .iter()
            .filter(|(s, _)| *s >= first)
            .map(|(_, r)| r.len())
            .sum::<usize>();
        assert!(
            bytes > 3 * wire::MIN_CAPACITY && bytes < capacity.max(5 * wire::MIN_CAPACITY),
            "the replay's records: {bytes} bytes"
        );

        let (e, j) = match &given {
            None => reopen_mask(&dir, saved, gt, capacity, mask),
            Some(order) => {
                let e = engine(&dir, capacity);
                let mut j = Java::open(e.clone(), saved);
                j.module_mask = mask;
                j.game_time = gt;
                assert_eq!(e.conflict_restore_check(), (None, false));
                j.idle();
                assert_eq!(
                    replay_counts(&e).0,
                    0,
                    "the replay runs on past its first hand-over"
                );
                let mut order = order.clone();
                match &mut order {
                    // The village's culture buys works, so that the grant has a purse to go to.
                    Order::WorksGrant { .. } => j.work_catalog(&test_works()),
                    Order::HerdFound { dimension, .. } => {
                        *dimension = j.dict_id(wire::DICT_DIMENSION, OVERWORLD)
                    }
                    _ => {}
                }
                e.order(vec![order]).unwrap();
                hand_over_all(&mut j);
                j.idle();
                (e, j)
            }
        };
        let (ended, hand_overs, _) = replay_counts(&e);
        assert_eq!(ended, 1, "the replay ended");
        if capacity == wire::MIN_CAPACITY {
            assert!(hand_overs >= 3, "a replay of {hand_overs} hand-overs");
        } else {
            assert_eq!(hand_overs, 1, "a replay of one hand-over");
        }
        assert!(party(&e, p).is_none(), "the band is home");
        let after = pool(&e);
        assert_eq!(
            after.2, live.2,
            "the pool after the replay counts the band as the session that went on did ({n} out, \
             {lost} lost): {after:?}, live {live:?}"
        );
        assert_eq!(
            after, live,
            "and the band came home when it did in the session that went on"
        );
        // The order itself is taken once the replay has ended.
        match given {
            Some(Order::WorksGrant { village, amount }) => {
                let v = e.phase1_works(village).expect("the granted village");
                assert_eq!(
                    v[2] + v[5],
                    amount,
                    "the grant in its purse or spent: {v:?}"
                );
            }
            Some(Order::HerdFound { .. }) => {
                assert_eq!(e.herd_agents().len(), 1, "a herd founded")
            }
            Some(Order::WorksRaid { village, .. }) => {
                let threat = e.with_dragons(|_, w| {
                    w.node_of_key(village)
                        .and_then(|n| w.works_of(n))
                        .and_then(|s| s.threat_at)
                });
                assert!(threat.is_some(), "the village raided");
            }
            Some(Order::TributeStart { .. }) => {
                assert_eq!(e.dragons_tribute()[0], 1, "the tribute paid")
            }
            _ => {}
        }
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_fight_kept_in_the_pages_counts_its_casualties_in_a_long_replay() {
        let long = Some(wire::MIN_CAPACITY);
        a_fight_kept_in_the_pages_across_a_replay("conflict-rp24-long", long, MODULES);
    }

    /// The same with industry on: its clock is not held (docs/replay-gate.md §3.6), and the
    /// conflict's still is.
    #[test]
    fn a_fight_kept_in_the_pages_counts_its_casualties_in_a_long_replay_with_industry_on() {
        let long = Some(wire::MIN_CAPACITY);
        let mask = MODULES | MODULE_INDUSTRY;
        a_fight_kept_in_the_pages_across_a_replay("conflict-rp24-long-industry", long, mask);
    }

    /// RP-33 (the fix round's item 3): a works grant given while the replay runs. Applied at
    /// once it would run the phase 1 world to the hand-over's time, Java's present, and the next
    /// replayed record's clamp (`max(record, conflict.now, phase1.world.now)`) would carry the
    /// conflict there: the fight's end, the walk and `home()` before the casualties, which then
    /// find the band pooled.
    #[test]
    fn a_works_grant_while_replaying_undoes_no_casualty() {
        let order = Order::WorksGrant {
            village: GRANTED,
            amount: 1_000_000,
        };
        let long = Some(wire::MIN_CAPACITY);
        a_fight_kept_in_the_pages_with("conflict-rp33-grant", long, MODULES, Some(order));
    }

    /// The same with a HERD_FOUND, which runs the shared clock to the hand-over's time itself
    /// (`shared::run_to`), the herds on.
    #[test]
    fn a_herd_found_while_replaying_undoes_no_casualty() {
        let order = Order::HerdFound {
            name: "Replayed flock".into(),
            species: "minecraft:sheep".into(),
            dimension: 0,
            x: -4_000,
            z: -4_000,
            count: 6,
            seed: 0,
        };
        let long = Some(wire::MIN_CAPACITY);
        let mask = MODULES | rtsim_jni::engine::MODULE_HERDS;
        a_fight_kept_in_the_pages_with("conflict-rp33-herd", long, mask, Some(order));
    }

    /// The same with a works raid (the re-review's item 5), which runs the phase 1 world to the
    /// hand-over's time as a grant does.
    #[test]
    fn a_works_raid_while_replaying_undoes_no_casualty() {
        let order = Order::WorksRaid {
            village: GRANTED,
            tribute: false,
        };
        let long = Some(wire::MIN_CAPACITY);
        a_fight_kept_in_the_pages_with("conflict-rp33-raid", long, MODULES, Some(order));
    }

    /// The same with a TRIBUTE_START (the re-review's item 5), which runs the dragons and the
    /// phase 1 world to the hand-over's time, the dragons on.
    #[test]
    fn a_tribute_start_while_replaying_undoes_no_casualty() {
        let order = Order::TributeStart {
            village: GRANTED,
            lair: TRIBUTE_LAIR,
        };
        let long = Some(wire::MIN_CAPACITY);
        let mask = MODULES | rtsim_jni::engine::MODULE_HERDS;
        a_fight_kept_in_the_pages_with("conflict-rp33-tribute", long, mask, Some(order));
    }

    /// RP-66 (the M5 merge check's item 2): a test's conflict command joins the conflict's queue
    /// around its gate, so one ordered while the replay runs waits for the replay end as RP-33's
    /// orders do. A BAND_MATERIALIZE, a "never" kind, is not written while the replay runs, and
    /// is once it has ended.
    #[test]
    fn a_conflict_command_ordered_while_replaying_waits_for_the_replay_end() {
        let (dir, e, mut j) = camp("conflict-rp66-order");
        save(&e, &mut j);
        let first = j.world_save().next_obs;
        // Eight hand-overs: an idle takes two.
        fillers(&mut j, 8_000);
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();
        let bytes = saved
            .journal
            .iter()
            .filter(|(s, _)| *s >= first)
            .map(|(_, r)| r.len())
            .sum::<usize>();
        assert!(
            bytes > 6 * wire::MIN_CAPACITY,
            "the replay's records: {bytes} bytes"
        );
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.idle();
        assert_eq!(
            replay_counts(&e).0,
            0,
            "the replay runs past its first hand-over"
        );
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let (warrior, agent) = (41, 0x43 << 56 | 0x66);
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
        let name = "mic_rtsim:conflict/goblin/warrior".to_string();
        e.order(vec![Order::ConflictCommand {
            names: vec![(wire::DICT_BODY, warrior, name)],
            record,
        }])
        .unwrap();
        let written =
            |j: &Java| cmds(j).any(|c| matches!(c, C::Materialize(m) if m.agent == agent));
        j.idle();
        assert_eq!(replay_counts(&e).0, 0, "still replaying");
        assert!(!written(&j), "not written while the replay runs");
        hand_over_all(&mut j);
        j.idle();
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        assert!(written(&j), "written once it has ended");
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// A second case: the same replay in one hand-over. It does not isolate RP-24: before
    /// df53fe38 it failed too, on RP-26's anchor (the arm's leave of the players' areas). The
    /// control is the no-crash variant below.
    #[test]
    fn a_fight_kept_in_the_pages_counts_its_casualties_in_a_one_hand_over_replay() {
        let one = Some(ONE_HAND_OVER);
        a_fight_kept_in_the_pages_across_a_replay("conflict-rp24-one", one, MODULES);
    }

    /// Control: no crash, the session that goes on.
    #[test]
    fn a_fight_kept_in_the_pages_pools_what_is_left_without_a_crash() {
        a_fight_kept_in_the_pages_across_a_replay("conflict-rp24-live", None, MODULES);
    }

    /// RP-22 (the review's amendment a): an invasion of 26 of one body kind, seen leaving the
    /// camp after the save, the quota's 16 placed; `killed` of them die in view (BAND_CASUALTIES),
    /// then the player leaves, the band is recalled and Java despawns it with 16 - `killed`
    /// survivors. Returns the band's alive count: in the session that went on (`crash` false),
    /// or after a crash and the replay of those records. The honour must place what Java was
    /// commanded (16), not every alive member (26): Java's despawn counts only the placed.
    fn an_invasion_seen_after_the_save(name: &str, killed: u32, crash: bool) -> u32 {
        let (dir, e, mut j) = camp(name);
        if killed > 0 {
            // Of one stack of 16: floor(16 * 0.2) = 3, 200 ticks after it spawns.
            j.conflict_plugin.fight = Some(synth::Fight {
                kill_share: 0.2,
                after: 200,
                kill_chief: false,
                only_at_base: false,
            });
        }
        // Far, so that it is seen leaving the camp and walks on, not fought.
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Invasion, Step::Invasion, far);
        on_step(&e, p, Step::Invasion);
        e.with_conflict_mut(|c, _| {
            let x = &mut c.parties[p];
            let u = x.units[0].0;
            x.units = vec![(u, 26)];
            x.start_units = 26;
        });
        assert!(!books(&e, p).2, "not seen yet");
        save(&e, &mut j);
        advance_until(&mut j, 100, 3 * DAY, "the invasion out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        assert_eq!(books(&e, p).4, 16, "the quota placed");
        if killed > 0 {
            advance_until(&mut j, 100, 3_000, "the casualties", |j| {
                books(&j.e, p).0 < 26
            });
            assert_eq!(books(&e, p).0, 26 - killed);
        }
        let from = j.conflict.len();
        j.players.clear();
        advance_until(&mut j, 100, 2 * DAY, "the recall", |j| {
            !recalls(j, from, p).is_empty()
        });
        advance_until(&mut j, 100, 3_000, "the despawn", |j| !books(&j.e, p).3);
        let live = books(&e, p);
        assert!(!live.2 && live.4 == 0, "despawned: {live:?}");
        if !crash {
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            e.close(true, Duration::from_secs(5)).unwrap();
            return live.0;
        }
        let gt = j.game_time;
        let saved = j.crash();
        let (e, j) = reopen(&dir, saved, gt);
        let (alive, _, embodied, recalled, spawned) = books(&e, p);
        assert!(
            !embodied && !recalled && spawned == 0,
            "abstract after the replay"
        );
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        alive
    }

    #[test]
    fn an_invasion_seen_after_the_save_is_counted_as_placed_after_a_crash() {
        assert_eq!(
            an_invasion_seen_after_the_save("conflict-rp22", 0, true),
            26
        );
    }

    #[test]
    fn an_invasion_seen_after_the_save_counts_its_dead_once_after_a_crash() {
        assert_eq!(
            an_invasion_seen_after_the_save("conflict-rp22-dead", 3, true),
            23
        );
    }

    /// Control: no crash.
    #[test]
    fn an_invasion_seen_and_despawned_without_a_crash() {
        assert_eq!(
            an_invasion_seen_after_the_save("conflict-rp22-live", 0, false),
            26
        );
        assert_eq!(
            an_invasion_seen_after_the_save("conflict-rp22-live-dead", 3, false),
            23
        );
    }

    /// RP-26 (the review's item): a watched fight kept in the pages, its casualties journaled
    /// after the save, then 4,000 fillers. After the crash the arm changes no body (conflict
    /// hears no `player_left` there): in the replay's hand-overs that carry the casualties the
    /// band is still embodied at its epoch, fighting, with its kept stack, and the casualties
    /// count off that stack, as in the session that went on; no RECALL goes in any buffer.
    /// `crash`: false is the control (the same books, read at the same point).
    fn a_watched_band_kept_across_the_arm(name: &str, crash: bool) {
        let (dir, e, mut j) = camp(name);
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.25,
            after: 1_500,
            kill_chief: false,
            only_at_base: true,
        });
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 8, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid fighting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Fighting)
        });
        let fight_at = j.game_time;
        let at_save = books(&e, p);
        assert!(
            at_save.2 && at_save.4 > 0,
            "in view at the save: {at_save:?}"
        );
        save(&e, &mut j);
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= fight_at + 1_500 + 200
        });
        let live = books(&e, p);
        assert!(n - live.0 >= 2, "the fight cost members: {live:?}");
        assert_eq!(
            party(&e, p),
            Some((Phase::Fighting, true)),
            "still fighting"
        );
        assert!(
            live.4 < at_save.4,
            "off its stack: {live:?}, at the save {at_save:?}"
        );
        fillers(&mut j, 4_000);
        if !crash {
            j.idle();
            assert_eq!(books(&e, p), live, "the session that went on");
            assert!(recalls(&j, 0, p).is_empty(), "no recall");
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            e.close(true, Duration::from_secs(5)).unwrap();
            return;
        }
        let gt = j.game_time;
        let saved = j.crash();

        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        // Hand over until the casualties are counted, the replay still running.
        let mut seen = Vec::new();
        for _ in 0..8 {
            j.flush();
            let b = books(&e, p);
            seen.push((b, party(&e, p)));
            if b.0 == live.0 {
                break;
            }
        }
        assert!(
            e.with_conflict(|c, _| c.replaying()),
            "the fillers are still to come: {seen:?}"
        );
        assert_eq!(
            (books(&e, p), party(&e, p)),
            (live, Some((Phase::Fighting, true))),
            "the kept band, its casualties off its kept stack: {seen:?}"
        );
        assert!(recalls(&j, 0, p).is_empty(), "no recall in any buffer");
        let last = j.world_save().journal.back().map_or(0, |(s, _)| *s);
        for _ in 0..64 {
            j.idle();
            if j.sent_obs >= last {
                break;
            }
        }
        assert!(!e.with_conflict(|c, _| c.replaying()), "the replay ended");
        assert_eq!(books(&e, p).0, live.0, "the replay counted each death once");
        assert!(recalls(&j, 0, p).is_empty(), "no recall after it either");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_watched_band_kept_in_the_pages_stays_embodied_across_the_arm() {
        a_watched_band_kept_across_the_arm("conflict-rp26", true);
    }

    /// Control: no crash.
    #[test]
    fn a_watched_band_keeps_its_books_without_a_crash() {
        a_watched_band_kept_across_the_arm("conflict-rp26-live", false);
    }

    /// §9.1 test 1 (the scratch audit's band first seen after a save): a raid in view only after
    /// the save, so it is materialized at a new epoch, fought and half killed, then a crash. The
    /// replay honours its BAND_MATERIALIZED (RP-6), so the casualties of that epoch find the band
    /// (12 alive before RP-6: none of them counted). Returns its alive count after the crash and
    /// the replay, or in the session that went on (`crash` false).
    fn a_band_first_seen_after_the_save(name: &str, crash: bool) -> u32 {
        let (dir, e, mut j) = camp(name);
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.5,
            after: 1_500,
            kill_chief: false,
            only_at_base: false,
        });
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        assert_eq!(party(&e, p).map(|x| x.1), Some(false), "not in view yet");
        save(&e, &mut j);
        let epoch_at_save = books(&e, p).1;
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        let at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= at + 1_500 + 200
        });
        let live = books(&e, p);
        let lost = n - live.0;
        assert!(lost >= 2, "the fight cost members: {lost}");
        assert!(live.1 > epoch_at_save, "seen after the save at a new epoch");
        if !crash {
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            e.close(true, Duration::from_secs(5)).unwrap();
            return live.0;
        }
        let gt = j.game_time;
        let saved = j.crash();
        let (e, j) = reopen(&dir, saved, gt);
        let (alive, epoch, embodied, recalled, _) = books(&e, p);
        assert_eq!(alive, n - lost, "the replay counts each death once");
        assert_eq!(epoch, live.1, "at the epoch the replay honoured");
        assert!(!embodied && !recalled, "abstract after the replay");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        alive
    }

    #[test]
    fn a_band_first_seen_after_the_save_is_counted_after_a_crash() {
        assert_eq!(
            a_band_first_seen_after_the_save("conflict-seen-after-save", true),
            6
        );
    }

    /// Control: no crash.
    #[test]
    fn a_band_first_seen_after_the_save_is_counted_without_a_crash() {
        assert_eq!(
            a_band_first_seen_after_the_save("conflict-seen-after-save-live", false),
            6
        );
    }

    /// A raid out and in view at a save, half of it killed after the save behind `count` fillers,
    /// then a crash (scratch audit :5586): what the crash saved, the band, its size and losses,
    /// the epochs Java was commanded for it, and where it stood at the save and at the crash.
    struct InView {
        dir: std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        p: Handle<Party>,
        n: u32,
        lost: u32,
        epochs: Vec<u32>,
        at_save: (i32, i32),
        at_crash: (i32, i32),
    }

    /// See [`InView`]. `resave`: a save after the casualties too, right before the crash, so that
    /// nothing is left to replay.
    fn a_band_in_view_at_the_save(name: &str, mask: u32, count: usize, resave: bool) -> InView {
        let (dir, e, mut j) = camp_with(name, mask);
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.5,
            after: 1_500,
            kill_chief: false,
            only_at_base: false,
        });
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        save(&e, &mut j);
        let epoch_at_save = books(&e, p).1;
        let at_save = e.with_conflict(|c, _| c.party_pos(p, c.now()));
        let first = j.world_save().next_obs;
        fillers(&mut j, count);
        let at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= at + 1_500 + 200
        });
        let lost = n - books(&e, p).0;
        assert!(lost >= 2, "the fight cost members: {lost}");
        assert_eq!(
            books(&e, p).1,
            epoch_at_save,
            "one epoch in the old session"
        );
        if resave {
            save(&e, &mut j);
        }
        let epochs = materialized_epochs(&j, 0, p);
        let gt = j.game_time;
        let at_crash = e.with_conflict(|c, _| c.party_pos(p, c.now()));
        let saved = j.crash();
        let bytes = saved
            .journal
            .iter()
            .filter(|(s, _)| *s >= first)
            .map(|(_, r)| r.len())
            .sum::<usize>();
        if count >= 4_000 && !resave {
            assert!(
                bytes > 3 * wire::MIN_CAPACITY,
                "the replay needs several hand-overs: {bytes} bytes"
            );
        }
        InView {
            dir,
            saved,
            gt,
            p,
            n,
            lost,
            epochs,
            at_save,
            at_crash,
        }
    }

    /// The epochs of the BAND_MATERIALIZEs Java was commanded for `p` from `from` on, each once,
    /// in order.
    fn materialized_epochs(j: &Java, from: usize, p: Handle<Party>) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        for (_, c) in &j.conflict[from..] {
            if let C::Materialize(m) = c
                && m.agent == Party::agent(p)
                && out.last() != Some(&m.epoch)
            {
                out.push(m.epoch);
            }
        }
        out
    }

    /// The next session over `saved`, a player standing at `at`, its journal not handed over.
    fn reopen_at(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        at: (i32, i32),
        capacity: usize,
        mask: u32,
    ) -> (std::sync::Arc<Engine>, Java) {
        let e = engine(dir, capacity);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = mask;
        j.game_time = gt;
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let pos = [f64::from(at.0) + 0.5, 64.0, f64::from(at.1) + 0.5];
        j.players = vec![(PLAYER, dim, pos)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        (e, j)
    }

    /// Hands Java's whole journal over.
    fn hand_over_all(j: &mut Java) {
        let last = j.world_save().journal.back().map_or(0, |(s, _)| *s);
        for _ in 0..64 {
            j.idle();
            if j.sent_obs >= last {
                break;
            }
        }
        assert!(j.sent_obs >= last, "the whole journal handed over");
        assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    }

    /// §9.1 test 2 (scratch audit :5586): the band in view at the save, its casualties replayed
    /// over the hand-overs a `capacity` buffer takes, with a player standing at the band. The
    /// replay never embodies it again (RP-11), so the casualties' epoch stays its own and each
    /// death counts once.
    fn a_long_replay_at_the_band(name: &str, capacity: usize, mask: u32) {
        let v = a_band_in_view_at_the_save(name, mask, 4_000, false);
        let (e, mut j) = reopen_at(&v.dir, v.saved.clone(), v.gt, v.at_crash, capacity, mask);
        hand_over_all(&mut j);
        let (ended, hand_overs, _) = replay_counts(&e);
        assert_eq!(ended, 1, "the replay ended");
        if capacity == wire::MIN_CAPACITY {
            assert!(hand_overs >= 3, "a replay of {hand_overs} hand-overs");
        } else {
            assert_eq!(hand_overs, 1, "a replay of one hand-over");
        }
        assert_eq!(
            books(&e, v.p).0,
            v.n - v.lost,
            "the replay counts each death once ({} out, {} lost)",
            v.n,
            v.lost
        );
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_long_replay_counts_every_casualty_against_its_band() {
        a_long_replay_at_the_band("conflict-long-replay", wire::MIN_CAPACITY, MODULES);
    }

    /// The same with industry on (its clock is not held, docs/replay-gate.md §3.6).
    #[test]
    fn a_long_replay_counts_every_casualty_against_its_band_with_industry_on() {
        let mask = MODULES | MODULE_INDUSTRY;
        a_long_replay_at_the_band("conflict-long-replay-industry", wire::MIN_CAPACITY, mask);
    }

    /// Control: the same replay in one hand-over.
    #[test]
    fn a_one_hand_over_replay_counts_every_casualty_against_its_band() {
        a_long_replay_at_the_band("conflict-long-replay-one", ONE_HAND_OVER, MODULES);
    }

    /// §9.1 test 3: the long replay of test 2, hand-over by hand-over. Until the replay end no
    /// BAND_MATERIALIZE, ENVOY_MATERIALIZE or FACTION_NOTICE goes in any command buffer; after it
    /// the band materializes where it is now (seen on the march: the arc's centre is the band's
    /// place), at an epoch above every one Java was commanded. `resave`, the control: a save right
    /// before the crash, nothing to replay, and the band materializes as soon as the session
    /// starts (in the second hand-over, as below).
    fn a_band_materialized_after_the_replay(name: &str, resave: bool) {
        let v = a_band_in_view_at_the_save(name, MODULES, 4_000, resave);
        let (e, mut j) = reopen_at(
            &v.dir,
            v.saved.clone(),
            v.gt,
            v.at_crash,
            wire::MIN_CAPACITY,
            MODULES,
        );
        let embodies = |c: &C| matches!(c, C::Materialize(_) | C::Envoy(_) | C::Notice(_));
        let mut hand_overs = 0;
        while replay_counts(&e).0 == 0 {
            assert!(hand_overs < 64, "the replay never ended");
            j.flush();
            hand_overs += 1;
            if replay_counts(&e).0 == 0 {
                let early: Vec<&str> = j
                    .conflict
                    .iter()
                    .filter(|(_, c)| embodies(c))
                    .map(|(_, c)| c.name())
                    .collect();
                assert!(
                    early.is_empty(),
                    "hand-over {hand_overs}, replaying: {early:?}"
                );
            }
        }
        if resave {
            assert_eq!(
                hand_overs, 1,
                "nothing to replay: it ends in the first hand-over"
            );
        } else {
            assert!(hand_overs >= 3, "a replay of {hand_overs} hand-overs");
        }
        // The step books the players' areas after the conflict's run (`shared::simulate` comes
        // after `conflict::simulate`), so the sighting found in the hand-over that ends the replay
        // runs in the next one, as a live player's coming into view does.
        let ended_at = hand_overs;
        while materialized_epochs(&j, 0, v.p).is_empty() {
            assert!(
                hand_overs < ended_at + 8,
                "never materialized after the replay"
            );
            j.flush();
            hand_overs += 1;
        }
        assert_eq!(
            hand_overs,
            ended_at + 1,
            "materialized in the next hand-over"
        );
        let (m, live) = (
            j.conflict
                .iter()
                .find_map(|(_, c)| match c {
                    C::Materialize(m) if m.agent == Party::agent(v.p) => Some(m.clone()),
                    _ => None,
                })
                .expect("the materialize"),
            e.with_conflict(|c, _| c.party_pos(v.p, c.now())),
        );
        let old = v
            .epochs
            .iter()
            .copied()
            .max()
            .expect("materialized last session");
        assert!(
            m.epoch > old,
            "a new epoch: {} after {:?}",
            m.epoch,
            v.epochs
        );
        let off = |a: (f64, f64), b: (i32, i32)| (a.0 - f64::from(b.0)).hypot(a.1 - f64::from(b.1));
        let moved = off((f64::from(v.at_save.0), f64::from(v.at_save.1)), live);
        assert!(moved > 16.0, "the band moved since the save: {moved}");
        assert!(
            off(m.arc_centre, live) < 4.0,
            "from where the band is now, {live:?}: {:?} (at the save {:?})",
            m.arc_centre,
            v.at_save
        );
        assert_eq!(
            books(&e, v.p).0,
            v.n - v.lost,
            "and each death counted once"
        );
        hand_over_all(&mut j);
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn no_band_materializes_while_a_long_replay_runs_with_a_player_near() {
        a_band_materialized_after_the_replay("conflict-no-materialize", false);
    }

    /// Control: nothing to replay.
    #[test]
    fn a_band_materializes_at_the_session_start_with_nothing_to_replay() {
        a_band_materialized_after_the_replay("conflict-no-materialize-resaved", true);
    }

    /// §9.1 test 4 (RP-7, RP-30): a raid not in view at the save (epoch E), seen after it at
    /// E + 1 and its BAND_MATERIALIZED journaled, then a crash. The replay honours E + 1 (RP-6),
    /// and the next session's sighting takes E + 2: E + 1 named the last session's bodies. Returns
    /// the epochs Java was commanded for the band, as offsets from E, in order. `crash` false, the
    /// control: the session goes on, the player leaves (a recall and its despawn) and comes back
    /// to the band.
    fn epochs_of_a_band_seen_after_the_save(name: &str, crash: bool) -> Vec<u32> {
        let (dir, e, mut j) = camp(name);
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        assert_eq!(party(&e, p).map(|x| x.1), Some(false), "not in view yet");
        save(&e, &mut j);
        let at_save = books(&e, p).1;
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        let seen_at = j.game_time;
        advance_until(&mut j, 100, 1_000, "its BAND_MATERIALIZED journaled", |j| {
            j.game_time >= seen_at + 200
        });
        let mut epochs = materialized_epochs(&j, 0, p);
        let watch = |j: &mut Java, at: (i32, i32)| {
            let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
            let pos = [f64::from(at.0) + 0.5, 64.0, f64::from(at.1) + 0.5];
            j.players = vec![(PLAYER, dim, pos)];
        };
        let (e, mut j) = if crash {
            let gt = j.game_time;
            let saved = j.crash();
            let (e, mut j) = reopen(&dir, saved, gt);
            j.conflict_plugin = synth::FakeConflict::new(PLAYER);
            let b = books(&e, p);
            assert!(
                b.1 == at_save + 1 && !b.2,
                "the replay honoured E + 1, abstract after it: {b:?}"
            );
            // The player comes back where the band is now.
            let band = e.with_conflict(|c, _| c.party_pos(p, c.now()));
            watch(&mut j, band);
            (e, j)
        } else {
            let from = j.conflict.len();
            j.players.clear();
            advance_until(&mut j, 100, 2 * DAY, "the recall", |j| {
                !recalls(j, from, p).is_empty()
            });
            advance_until(&mut j, 100, 3_000, "the despawn", |j| {
                let b = books(&j.e, p);
                !b.2 && !b.3
            });
            let band = e.with_conflict(|c, _| c.party_pos(p, c.now()));
            watch(&mut j, band);
            (e, j)
        };
        let from = j.conflict.len();
        advance_until(&mut j, 20, 2_000, "seen again", |j| {
            !materialized_epochs(j, from, p).is_empty()
        });
        epochs.extend(materialized_epochs(&j, from, p));
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        epochs.iter().map(|x| x - at_save).collect()
    }

    #[test]
    fn a_band_epoch_decided_after_the_save_is_never_decided_again() {
        let epochs = epochs_of_a_band_seen_after_the_save("conflict-epoch-floor", true);
        assert_eq!(epochs, vec![1, 2]);
    }

    /// Control: no crash.
    #[test]
    fn a_band_seen_twice_without_a_crash_takes_two_epochs() {
        let epochs = epochs_of_a_band_seen_after_the_save("conflict-epoch-floor-live", false);
        assert_eq!(epochs, vec![1, 2]);
    }

    /// §9.1 test 7 (U10, docs/replay-gate.md §3.1): the conflict switched off at the session start
    /// and on in the middle of the session. Its reset begins its replay, and with the session's
    /// replay over, that replay ends at once. `long`, the control: switched on while the session's
    /// replay still runs (4,000 fillers after the save), its replay ends with the session's.
    fn the_conflict_switched_on_mid_session(name: &str, long: bool) {
        let (dir, e, mut j) = camp(name);
        save(&e, &mut j);
        if long {
            fillers(&mut j, 4_000);
        }
        let gt = j.game_time;
        let saved = j.crash();
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES & !MODULE_CONFLICT;
        j.game_time = gt;
        j.flush();
        let conflict_replaying = |e: &Engine| e.with_conflict(|c, _| c.replaying());
        assert_eq!(
            replay_counts(&e).0,
            u64::from(!long),
            "the session's replay end"
        );
        assert!(
            !conflict_replaying(&e),
            "switched off: its replay has not begun"
        );
        j.modules(MODULES);
        j.flush();
        assert_eq!(
            conflict_replaying(&e),
            long,
            "switched on: its replay ends with the session's"
        );
        hand_over_all(&mut j);
        assert_eq!(replay_counts(&e).0, 1, "one replay end a session");
        assert!(!conflict_replaying(&e), "the conflict's replay ended");
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_module_switched_on_mid_session_ends_its_replay_at_once() {
        the_conflict_switched_on_mid_session("conflict-switched-on", false);
    }

    /// Control: switched on while the session's replay runs.
    #[test]
    fn a_module_switched_on_while_the_replay_runs_ends_it_with_the_session() {
        the_conflict_switched_on_mid_session("conflict-switched-on-replaying", true);
    }

    /// §9.1 test 7 (docs/replay-gate.md §2.1, §2.2, RP-29): where the replay ends and the
    /// hand-overs it counts. A camp saved, `count` fillers journaled after the save, and a crash;
    /// `trim`: Java's journal trimmed past the pages (emptied, its next sequence kept), so that it
    /// starts at or past `session_first`. Returns the replay counts after the next session's first
    /// hand-over; by then the replay has ended, and the conflict's with it.
    fn replay_counts_after(name: &str, count: usize, trim: bool) -> (u64, u64, u64) {
        let (dir, e, mut j) = camp(name);
        save(&e, &mut j);
        fillers(&mut j, count);
        let gt = j.game_time;
        let mut saved = j.crash();
        if trim {
            saved.journal.clear();
        }
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.flush();
        let counts = replay_counts(&e);
        assert!(
            !e.with_conflict(|c, _| c.replaying()),
            "the conflict's replay ended"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        counts
    }

    #[test]
    fn a_session_with_nothing_to_replay_ends_its_replay_at_the_session_start() {
        assert_eq!(
            replay_counts_after("conflict-replay-nothing", 0, false),
            (1, 0, 0)
        );
    }

    /// RP-29: without the journal check the replay waited for the first live record.
    #[test]
    fn a_journal_starting_past_the_session_first_ends_the_replay_at_the_session_start() {
        assert_eq!(
            replay_counts_after("conflict-replay-trimmed", 10, true),
            (1, 0, 0)
        );
    }

    /// Control: the same ten records kept, a replay of one hand-over.
    #[test]
    fn a_short_replay_ends_in_its_first_hand_over() {
        let (ended, hand_overs, records) = replay_counts_after("conflict-replay-short", 10, false);
        assert_eq!((ended, hand_overs), (1, 1));
        assert!(records >= 10, "the fillers replayed: {records}");
    }

    // ---- §2.5: the tests that need both S4p and RP (docs/replay-gate.md §2.5, RP-43) ---------

    /// What a holder keeps of a band's casualties: the generation that journaled them and the
    /// records as journaled.
    struct Kept {
        nonce: u64,
        records: Vec<Vec<u8>>,
    }

    /// The BAND_CASUALTIES Java journaled for `p` from sequence `first` on.
    fn casualties_kept(j: &Java, first: u64, p: Handle<Party>) -> Kept {
        let records = j
            .journal
            .iter()
            .filter(|(s, r)| *s >= first && wire::u16_at(r, wire::R_KIND) == cb::BAND_CASUALTIES)
            .filter(|(_, r)| {
                matches!(cb::decode_obs(r), Ok((_, O::Casualties(c), _))
                    if c.agent == Party::agent(p))
            })
            .map(|(_, r)| r.clone())
            .collect();
        Kept {
            nonce: j.nonce,
            records,
        }
    }

    /// A holder's copies (REASSERT) of each kept record, under generation `nonce`, handed over.
    fn reassert_all(j: &mut Java, k: &Kept, nonce: u64) {
        for r in &k.records {
            j.reassert(nonce, r);
        }
        j.idle();
        assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    }

    /// Copies applied and dropped (S4p's counts).
    fn copies(e: &Engine) -> (u64, u64) {
        use std::sync::atomic::Ordering::Acquire;
        (
            e.stats.copies_applied.load(Acquire),
            e.stats.copies_dropped.load(Acquire),
        )
    }

    struct Fought {
        dir: std::path::PathBuf,
        e: std::sync::Arc<Engine>,
        j: Java,
        p: Handle<Party>,
        n: u32,
        lost: u32,
        kept: Kept,
    }

    /// [`a_fight_and_a_crash`]'s first session, up to its crash: the world saves taken with the
    /// pages (`before`) and at the crash, and the game time then.
    struct Crashed {
        dir: std::path::PathBuf,
        p: Handle<Party>,
        n: u32,
        lost: u32,
        kept: Kept,
        before: WorldSave,
        crashed: WorldSave,
        gt: i64,
    }

    /// A raid in view at a save (the pages, and a world save beside them), its fight's casualties
    /// journaled after it, then a crash.
    fn a_fight_then_a_crash(name: &str) -> Crashed {
        let (dir, e, mut j) = camp(name);
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.5,
            after: 1_500,
            kill_chief: false,
            only_at_base: false,
        });
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        let n = books(&e, p).0;
        assert!(n >= 4, "a band with members to lose: {n}");
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        save(&e, &mut j);
        let before = j.world_save();
        let at = j.game_time;
        advance_until(&mut j, 100, 3_000, "the casualties journaled", |j| {
            j.game_time >= at + 1_500 + 200
        });
        let lost = n - books(&e, p).0;
        assert!(lost >= 2, "the fight cost members: {lost}");
        let kept = casualties_kept(&j, before.next_obs, p);
        assert!(!kept.records.is_empty(), "its casualties journaled");
        let gt = j.game_time;
        let crashed = j.crash();
        Crashed {
            dir,
            p,
            n,
            lost,
            kept,
            before,
            crashed,
            gt,
        }
    }

    /// [`a_fight_then_a_crash`] and the next session. `lost_tail`: it starts from the world save
    /// taken with the pages, so the crash lost the casualties with the journal's tail and only a
    /// holder's copy is left of them; else from the world save at the crash, whose replay counts
    /// them. Returns the next session, its replay over.
    fn a_fight_and_a_crash(name: &str, lost_tail: bool) -> Fought {
        let Crashed {
            dir,
            p,
            n,
            lost,
            kept,
            before,
            crashed,
            gt,
        } = a_fight_then_a_crash(name);
        let (e, j) = reopen(&dir, if lost_tail { before } else { crashed }, gt);
        let (alive, _, embodied, recalled, _) = books(&e, p);
        let want = if lost_tail { n } else { n - lost };
        assert_eq!(
            alive, want,
            "lost with the tail ({lost_tail}), else replayed"
        );
        assert!(!embodied && !recalled, "abstract after the replay");
        Fought {
            dir,
            e,
            j,
            p,
            n,
            lost,
            kept,
        }
    }

    /// §2.5 test 1 (SEC-1 fact 3, case 1; RP-9): a live copy of casualties a crash lost with the
    /// journal's tail counts once, against the band by identity (its bodies are gone); a second
    /// copy of the same records is dropped and counts nothing.
    #[test]
    fn a_live_copy_of_casualties_a_crash_lost_counts_once() {
        let Fought {
            e,
            mut j,
            p,
            n,
            lost,
            kept,
            ..
        } = a_fight_and_a_crash("conflict-copy-live", true);
        let k = kept.records.len() as u64;
        reassert_all(&mut j, &kept, kept.nonce);
        assert_eq!(books(&e, p).0, n - lost, "the copy counts the deaths");
        assert_eq!(copies(&e), (k, 0));
        reassert_all(&mut j, &kept, kept.nonce);
        assert_eq!(books(&e, p).0, n - lost, "the second copy counts nothing");
        assert_eq!(copies(&e), (k, k));
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// Control: the casualties in the replayed tail and a copy of them after it: counted once, by
    /// the original; the copy is not new.
    #[test]
    fn a_copy_of_casualties_the_replay_counted_counts_nothing() {
        let Fought {
            e,
            mut j,
            p,
            n,
            lost,
            kept,
            ..
        } = a_fight_and_a_crash("conflict-copy-replayed-original", false);
        let k = kept.records.len() as u64;
        reassert_all(&mut j, &kept, kept.nonce);
        assert_eq!(books(&e, p).0, n - lost, "counted once, by the original");
        assert_eq!(copies(&e), (0, k));
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// §2.5 test 2 (fact 3, case 2): test 1's live copy journaled after a save of the session
    /// that applied it, then a crash. The next session replays the copy: it is dispatched while
    /// replaying (counted among the replay's records, every record after the pages), and the
    /// band's units fall once.
    #[test]
    fn a_copy_journaled_after_the_pages_is_counted_by_the_replay() {
        let (e, j, ..) = a_copy_replayed("conflict-copy-in-the-replay");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// §2.5 test 2's sessions, its checks made. Returns the session that replayed the copy, the
    /// band, its alive units and the records copied.
    fn a_copy_replayed(name: &str) -> (std::sync::Arc<Engine>, Java, Handle<Party>, u32, Kept) {
        let Fought {
            dir,
            e,
            mut j,
            p,
            n,
            lost,
            kept,
        } = a_fight_and_a_crash(name, true);
        save(&e, &mut j);
        let pages = e.durable_obs_seq();
        let k = kept.records.len() as u64;
        reassert_all(&mut j, &kept, kept.nonce);
        assert_eq!(books(&e, p).0, n - lost, "counted live");
        let gt = j.game_time;
        let saved = j.crash();
        let tail = saved
            .journal
            .iter()
            .filter(|(s, r)| *s > pages && wire::u16_at(r, wire::R_KIND) != obs::JOURNAL_GEN)
            .count() as u64;
        let copies_in_tail = saved
            .journal
            .iter()
            .filter(|(s, r)| *s > pages && wire::u16_at(r, wire::R_KIND) == obs::REASSERT)
            .count() as u64;
        assert_eq!(copies_in_tail, k, "the copies are after the pages");
        let (e, j) = reopen(&dir, saved, gt);
        assert_eq!(books(&e, p).0, n - lost, "the replay counts the copy once");
        assert_eq!(copies(&e), (k, 0), "applied from the replay");
        let (ended, _, records) = replay_counts(&e);
        assert_eq!(
            (ended, records),
            (1, tail),
            "every record after the pages replayed, the copies among them"
        );
        (e, j, p, n - lost, kept)
    }

    /// SEC-1's meeting-point review, item 7: a second live copy after a replayed copy. §2.5 test
    /// 2's copy is replayed and counted once; then the holder loads again after the replay end
    /// and journals the same copies live. The replay recorded them (`admit_copy`), so they are
    /// not new: dropped, counting nothing.
    #[test]
    fn a_second_live_copy_after_a_replayed_copy_counts_nothing() {
        let (e, mut j, p, alive, kept) = a_copy_replayed("conflict-copy-replayed-then-live");
        let k = kept.records.len() as u64;
        reassert_all(&mut j, &kept, kept.nonce);
        assert_eq!(books(&e, p).0, alive, "the live copy counts nothing");
        assert_eq!(copies(&e), (k, k), "dropped: the replay applied it");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// §2.5 test 3 (RP-8, RP-7): a raid not in view at a save (epoch E), seen after it (its
    /// BAND_MATERIALIZED at E + 1 journaled), then a crash that loses the journal's tail with it,
    /// so the next session decided nothing past E. A holder's copy of the BAND_MATERIALIZED,
    /// `replayed`: applied in that session, then a crash, and replayed in the one after; else
    /// applied live. It embodies nothing, and the band's next sighting is above it. Returns the
    /// epochs Java was commanded after the copy, as offsets from E.
    fn a_copy_of_a_band_materialized(name: &str, replayed: bool) -> Vec<u32> {
        let (dir, e, mut j) = camp(name);
        let far = (CAMP_AT.0 + 3_000, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, far);
        on_step(&e, p, Step::Raid);
        assert_eq!(party(&e, p).map(|x| x.1), Some(false), "not in view yet");
        save(&e, &mut j);
        let before = j.world_save();
        let at_save = books(&e, p).1;
        advance_until(&mut j, 100, 3 * DAY, "the raid out and in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        let seen_at = j.game_time;
        advance_until(&mut j, 100, 1_000, "its BAND_MATERIALIZED journaled", |j| {
            j.game_time >= seen_at + 200
        });
        let rec = j
            .journal
            .iter()
            .filter(|(s, r)| {
                *s >= before.next_obs && wire::u16_at(r, wire::R_KIND) == cb::BAND_MATERIALIZED
            })
            .map(|(_, r)| r.clone())
            .next()
            .expect("its BAND_MATERIALIZED");
        assert_eq!(materialized_epochs(&j, 0, p), vec![at_save + 1]);
        let nonce = j.nonce;
        let gt = j.game_time;
        let _ = j.crash();

        let (mut e, mut j) = reopen(&dir, before, gt);
        assert_eq!(
            books(&e, p).1,
            at_save,
            "the crash lost E + 1 with the tail"
        );
        j.reassert(nonce, &rec);
        j.idle();
        if replayed {
            let gt = j.game_time;
            let saved = j.crash();
            (e, j) = reopen(&dir, saved, gt);
        }
        assert_eq!(copies(&e), (1, 0), "the copy applied");
        let b = books(&e, p);
        assert!(!b.2 && b.4 == 0, "the copy embodies nothing: {b:?}");
        assert!(
            materialized_epochs(&j, 0, p).is_empty(),
            "nothing commanded"
        );
        // The player comes to where the band is now.
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        let band = e.with_conflict(|c, _| c.party_pos(p, c.now()));
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let pos = [f64::from(band.0) + 0.5, 64.0, f64::from(band.1) + 0.5];
        j.players = vec![(PLAYER, dim, pos)];
        advance_until(&mut j, 20, 2_000, "seen again", |j| {
            !materialized_epochs(j, 0, p).is_empty()
        });
        let epochs = materialized_epochs(&j, 0, p);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        epochs.iter().map(|x| x - at_save).collect()
    }

    #[test]
    fn a_live_copy_of_a_band_materialized_embodies_nothing_and_raises_the_epoch() {
        assert_eq!(
            a_copy_of_a_band_materialized("conflict-copy-materialized-live", false),
            vec![2]
        );
    }

    #[test]
    fn a_replayed_copy_of_a_band_materialized_embodies_nothing_and_raises_the_epoch() {
        assert_eq!(
            a_copy_of_a_band_materialized("conflict-copy-materialized-replayed", true),
            vec![2]
        );
    }

    /// §2.5 test 5 (RP-21): two live copies of the lost casualties under generation `nonce`.
    /// Returns the members the copies took and the fight's dead, the conflict's refusals, S4p's
    /// copy counts, and the records in one copy.
    fn two_copies_of_lost_casualties(name: &str, nonce: u64) -> ((u32, u32), u64, (u64, u64), u64) {
        let Fought {
            e,
            mut j,
            p,
            n,
            lost,
            kept,
            ..
        } = a_fight_and_a_crash(name, true);
        reassert_all(&mut j, &kept, nonce);
        reassert_all(&mut j, &kept, nonce);
        let took = n - books(&e, p).0;
        let (_, link) = e.conflict_stats();
        let k = kept.records.len() as u64;
        let out = ((took, lost), link.copies_refused, copies(&e), k);
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        out
    }

    /// Generation 0: neither copy counts (a death really lost: a member lives on), each refused.
    #[test]
    fn copies_of_generation_0_casualties_are_refused() {
        let ((took, _), refused, copies, k) =
            two_copies_of_lost_casualties("conflict-copy-gen0", 0);
        assert_eq!(took, 0, "neither copy counts");
        assert_eq!(refused, 2 * k, "both refused");
        assert_eq!(copies, (2 * k, 0), "S4p finds each new");
    }

    /// Control: a named generation no page holds: the first copy counts once, the second is
    /// dropped.
    #[test]
    fn copies_of_casualties_of_an_unknown_generation_count_once() {
        let ((took, lost), refused, copies, k) =
            two_copies_of_lost_casualties("conflict-copy-unknown-gen", 0x7E57_0000_0000_0001);
        assert_eq!(took, lost, "the first copy counts the deaths once");
        assert_eq!(refused, 0);
        assert_eq!(copies, (k, k), "the second dropped");
    }

    // ---- The fix round: no save while replaying (docs/replay-gate.md §3.2, RP-42) -----------

    /// The camp's observed fights (`end_fight` pushes one; `wait_over` none).
    fn observed(e: &Engine) -> Vec<(f64, f64)> {
        e.with_conflict(|c, _| {
            let f = c.faction_of_key(CAMP).expect("the camp") as usize;
            c.factions[f].observed.clone()
        })
    }

    /// Opens the next session over `saved`, `players` in the world, with Java asking before its
    /// first hand-over for a save through `session_first - 1`, the last replayed record: the
    /// save is due at the session's first record, its JOURNAL_GEN. Hands the journal over and
    /// waits for that save to land. Returns the session and its `session_first`.
    fn reopen_saving_at_the_first_live_record(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        players: Vec<((u64, u64), u32, [f64; 3])>,
    ) -> (std::sync::Arc<Engine>, Java, u64) {
        const SAVES_DONE: usize = 9;
        let e = engine(dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.players = players;
        let first = j.next_obs - 1;
        let back = j
            .journal
            .back()
            .map(|(s, r)| (*s, wire::u16_at(r, wire::R_KIND)));
        assert_eq!(
            back,
            Some((first, obs::JOURNAL_GEN)),
            "the session's first record"
        );
        assert!(e.durable_obs_seq() < first - 1, "a tail to replay");
        let saves = e.stats_array()[SAVES_DONE];
        e.request_save(first - 1);
        hand_over_all(&mut j);
        let deadline = Instant::now() + DURABLE_WAIT;
        while e.durable_obs_seq() < first - 1 {
            assert!(Instant::now() < deadline, "the save never landed");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            e.durable_obs_seq(),
            first - 1,
            "the pages end at the last replayed record"
        );
        assert_eq!(e.stats_array()[SAVES_DONE], saves + 1, "one save");
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        (e, j, first)
    }

    /// A restart from the pages taken at the last session's first record (`first`): their state
    /// before the first hand-over, then the session with the journal handed over.
    fn reopen_from_the_first_live_records_pages(
        dir: &std::path::PathBuf,
        saved: WorldSave,
        gt: i64,
        first: u64,
        p: Handle<Party>,
    ) -> (Option<(Phase, bool)>, std::sync::Arc<Engine>, Java) {
        let e = engine(dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        assert_eq!(e.durable_obs_seq(), first - 1, "the pages of that save");
        let paged = party(&e, p);
        hand_over_all(&mut j);
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        (paged, e, j)
    }

    /// RP-42: the budget's own save waits for the replay end too. A conflict under a small row
    /// budget, 200 camps found after its last save and 4,000 fillers behind them, then a crash;
    /// the next session replays them over several hand-overs. The camps' rows are new, so every
    /// resident row page is dirty and over the budget from the first hand-over on: no save is
    /// taken until the replay end, and the resident peak passes the budget by the replay's rows
    /// until then; after it, a budget save comes and the rows go back under their budget.
    #[test]
    fn a_long_replay_takes_no_budget_save_before_its_end() {
        use std::sync::atomic::Ordering::Acquire;
        let budget = 4 * 1024;
        let dir = tmpdir("conflict-rp42-budget");
        let e = engine_with(&dir, 256 * 1024, |c| c.row_budget = budget);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        j.idle();
        save(&e, &mut j);
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
        for i in 0..200 {
            j.conflict_obs(&O::FactionFound(cb::FactionFound {
                faction: CAMP + 1 + i,
                flags: 0,
                dimension: dim,
                structure,
                pos: (CAMP_AT.0 + 400 * i as i32, 64, CAMP_AT.1),
                home_bodies: 0,
                census: 0,
            }));
        }
        fillers(&mut j, 4_000);
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();

        let e = engine_with(&dir, wire::MIN_CAPACITY, |c| c.row_budget = budget);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        let (pressure, saves) = (
            e.stats.pressure_saves.load(Acquire),
            e.stats.saves_done.load(Acquire),
        );
        let total = e.with_conflict(|c, _| c.budget());
        let (mut peak, mut hand_overs) = (0, 0);
        while replay_counts(&e).0 == 0 {
            assert!(hand_overs < 64, "the replay never ended");
            j.flush();
            hand_overs += 1;
            peak = peak.max(e.with_conflict(|c, _| c.resident_bytes()));
            if replay_counts(&e).0 == 0 {
                assert_eq!(
                    e.stats.pressure_saves.load(Acquire),
                    pressure,
                    "no budget save while replaying (hand-over {hand_overs})"
                );
            }
        }
        assert!(hand_overs >= 3, "a replay of {hand_overs} hand-overs");
        assert_eq!(
            e.stats.saves_done.load(Acquire),
            saves,
            "no save while replaying"
        );
        assert!(
            peak > total,
            "over the budget while replaying: {peak} B of {total} B"
        );
        for _ in 0..8 {
            j.idle();
        }
        let resident = e.with_conflict(|c, _| c.resident_bytes());
        let after = e.stats.pressure_saves.load(Acquire);
        eprintln!(
            "a replay of {hand_overs} hand-overs under {total} B: rows {peak} B at peak while \
             replaying, {resident} B after; {} budget saves after the replay end",
            after - pressure
        );
        assert!(after > pressure, "the budget saved after the replay end");
        assert!(
            resident <= total,
            "the rows back under their budget: {resident} B of {total} B"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// The conflict's resident row bytes in [`the_replays_peak_against_the_live_sessions`]: the
    /// live session's peak under the budget, the same session's with a budget it never reaches
    /// (every row it touched since its last durable save resident), and the replay's after a
    /// crash, under the budget.
    #[derive(Debug)]
    struct Peaks {
        live: usize,
        touched: usize,
        replay: usize,
    }

    /// RP-49 (the re-review's item 2): a conflict under a 4 KiB row budget, a save, then `rounds`
    /// hand-overs of 20 camps found and 400 fillers each, no save asked for, and a crash. The
    /// live session's budget takes saves of its own, pending generations that the crash rolls
    /// back (PF15), so the replay starts from the save before the first round and, taking no
    /// page until its end (RP-42), holds every row the replayed records touch.
    fn the_replays_peak_against_the_live_sessions(name: &str, rounds: u64) -> Peaks {
        use std::sync::atomic::Ordering::Acquire;
        const BUDGET: usize = 4 * 1024;
        const PER: u64 = 20;
        let live = |dir: &std::path::PathBuf, budget: usize| {
            let e = engine_with(dir, 256 * 1024, |c| c.row_budget = budget);
            let mut j = Java::open(e.clone(), WorldSave::fresh());
            j.module_mask = MODULES;
            j.idle();
            save(&e, &mut j);
            let durable = e.durable_obs_seq();
            let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
            let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
            let mut peak = 0;
            for i in 0..rounds * PER {
                j.conflict_obs(&O::FactionFound(cb::FactionFound {
                    faction: CAMP + 1 + i,
                    flags: 0,
                    dimension: dim,
                    structure,
                    pos: (CAMP_AT.0 + 400 * i as i32, 64, CAMP_AT.1),
                    home_bodies: 0,
                    census: 0,
                }));
                if (i + 1) % PER == 0 {
                    fillers(&mut j, 400);
                    j.game_time += 100;
                    j.idle();
                    peak = peak.max(e.with_conflict(|c, _| c.resident_bytes()));
                }
            }
            assert_eq!(e.durable_obs_seq(), durable, "no save asked for");
            assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
            let pressure = e.stats.pressure_saves.load(Acquire);
            let gt = j.game_time;
            (peak, pressure, gt, j.crash())
        };
        let (touched, pressure, _, _) = live(&tmpdir(&format!("{name}-touched")), 1 << 30);
        assert_eq!(pressure, 0, "a budget the session never reaches");
        let dir = tmpdir(name);
        let (peak, pressure, gt, saved) = live(&dir, BUDGET);
        assert!(
            pressure > 0,
            "the live session's budget took saves of its own"
        );

        let e = engine_with(&dir, wire::MIN_CAPACITY, |c| c.row_budget = BUDGET);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        let mut replay = 0;
        for hand_overs in 0.. {
            assert!(hand_overs < 256, "the replay never ended");
            let replaying = replay_counts(&e).0 == 0;
            if !replaying {
                break;
            }
            j.flush();
            replay = replay.max(e.with_conflict(|c, _| c.resident_bytes()));
        }
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        e.close(true, Duration::from_secs(5)).unwrap();
        Peaks {
            live: peak,
            touched,
            replay,
        }
    }

    /// RP-49: the replay's resident peak is bounded by the rows the crashed session touched
    /// since its last durable save, not by its live peak. Under the budget the live session
    /// stays near its budget and a step, while the replay holds the whole window, so the replay
    /// passes the live peak by the window's rows less the budget: twice the window, twice the
    /// excess and more, not a constant (reported to the build lead as a finding; the window is
    /// Java's save cadence).
    #[test]
    fn a_replays_resident_peak_is_the_rows_touched_since_the_last_durable_save() {
        let short = the_replays_peak_against_the_live_sessions("conflict-rp49-short", 5);
        let long = the_replays_peak_against_the_live_sessions("conflict-rp49-long", 10);
        eprintln!("resident row bytes, 100 camps: {short:?}; 200 camps: {long:?}");
        for p in [&short, &long] {
            assert!(
                p.replay <= p.touched,
                "the replay holds no more than the window's rows: {p:?}"
            );
            assert!(
                p.live < p.touched,
                "the live session under its budget: {p:?}"
            );
        }
        let (a, b) = (short.replay - short.live, long.replay - long.live);
        assert!(
            b > a + (long.touched - short.touched) / 2,
            "the replay's excess over the live peak grows with the window: {a} B, then {b} B"
        );
    }

    /// RP-42: a clean close while the replay runs takes no page either: the pages stay as they
    /// were, and the next start replays the whole tail again.
    #[test]
    fn a_close_while_replaying_keeps_the_pages() {
        let (dir, e, mut j) = camp("conflict-rp42-close");
        save(&e, &mut j);
        let pages = e.durable_obs_seq();
        fillers(&mut j, 4_000);
        j.idle();
        let gt = j.game_time;
        let tail = j.journal.iter().filter(|(s, _)| *s > pages).count() as u64;
        let saved = j.crash();

        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved.clone());
        j.module_mask = MODULES;
        j.game_time = gt;
        j.flush();
        assert_eq!(replay_counts(&e).0, 0, "still replaying");
        assert!(e.stats_array()[4] > pages, "some of the tail replayed");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();

        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        assert_eq!(e.durable_obs_seq(), pages, "the pages as they were");
        hand_over_all(&mut j);
        assert_eq!(
            replay_counts(&e),
            (1, 1, tail),
            "the whole tail replayed again"
        );
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// RP-42 (the fix round's item 1): a fight kept in the pages (Fighting at the save), a crash,
    /// and a next session whose Java asks for a save through its last replayed record, taken at
    /// its first record. The pages must hold the state after the replay end: the fight Waiting,
    /// abstract. The session goes on (no player: the band waits out its fight, `wait_over`, and
    /// walks home), journals past that and crashes. The restart from those pages must end where
    /// the session that went on did. With the save before the replay end the pages kept the
    /// fight Fighting and embodied, and the restart ended it with `end_fight`, an observed fight
    /// the session that went on never had.
    #[test]
    fn a_save_at_the_first_live_record_keeps_a_kept_fight_waiting() {
        let (dir, e, mut j) = camp("conflict-rp42-fight");
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.25,
            after: 1_500,
            kill_chief: false,
            only_at_base: true,
        });
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        on_step(&e, p, Step::Raid);
        advance_until(&mut j, 100, 3 * DAY, "the raid fighting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Fighting)
        });
        save(&e, &mut j);
        fillers(&mut j, 20);
        j.game_time += 300;
        j.idle();
        assert_eq!(
            party(&e, p),
            Some((Phase::Fighting, true)),
            "fighting at the crash"
        );
        let observed_before = observed(&e);
        let gt = j.game_time;
        let saved = j.crash();

        // The next session: the player has gone.
        let (e, mut j, first) = reopen_saving_at_the_first_live_record(&dir, saved, gt, vec![]);
        assert_eq!(
            party(&e, p),
            Some((Phase::Waiting, false)),
            "abstract after the replay"
        );
        advance_until(&mut j, 100, 2 * DAY, "the band home", |j| {
            party(&j.e, p).is_none()
        });
        fillers(&mut j, 20);
        j.game_time += 600;
        j.idle();
        let live = (observed(&e), pool(&e));
        assert_eq!(
            live.0, observed_before,
            "a Waiting fight ends as no observed fight"
        );
        let gt = j.game_time;
        let saved = j.crash();

        let (paged, e, j) = reopen_from_the_first_live_records_pages(&dir, saved, gt, first, p);
        assert_eq!(
            paged,
            Some((Phase::Waiting, false)),
            "the pages hold the fight as the replay end left it"
        );
        assert!(party(&e, p).is_none(), "the band is home");
        assert_eq!(
            (observed(&e), pool(&e)),
            live,
            "the restart ends where the session that went on did"
        );
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// RP-42: a raid that reaches its target, unwatched, inside the replayed window is held until
    /// the replay end (§6.1), and Java asks for a save through the last replayed record, taken
    /// at the session's first record. The session goes on: the held arrival runs as live (no one
    /// watches: resolved, and the band walks home) while the player stands at the target, then
    /// a crash. The restart from those pages holds the arrival again while it replays (the
    /// pages kept its poll) and runs it after its replay end: the band is never read as
    /// fighting at its target, and its books end as in the session that went on. A band going
    /// home is never seen (`seen`), so no record of a sighting can follow a held raid's
    /// arrival to be read as its fight.
    #[test]
    fn a_save_at_the_first_live_record_reads_no_held_raid_as_a_fight() {
        let (dir, e, mut j) = camp("conflict-rp42-held");
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        // The player leaves, and its last report goes stale: no one sees the band leave.
        j.players.clear();
        j.game_time += 2_500;
        j.idle();
        let target = (CAMP_AT.0 + 600, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, target);
        on_step(&e, p, Step::Raid);
        advance_until(&mut j, 100, 3 * DAY, "the raid on its way", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Out)
        });
        assert_eq!(
            party(&e, p),
            Some((Phase::Out, false)),
            "abstract on its way"
        );
        save(&e, &mut j);
        advance_until(
            &mut j,
            100,
            3 * DAY,
            "the raid arrived and going home",
            |j| party(&j.e, p).is_some_and(|x| x.0 == Phase::Home),
        );
        fillers(&mut j, 20);
        j.game_time += 300;
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();

        let at_target = [f64::from(target.0) + 0.5, 64.0, f64::from(target.1) + 0.5];
        let players = vec![(PLAYER, dim, at_target)];
        let (e, mut j, first) = reopen_saving_at_the_first_live_record(&dir, saved, gt, players);
        let live_arrival = e.conflict_stats().0.raids_watched;
        advance_until(&mut j, 20, DAY, "the held arrival run as live", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Home)
        });
        fillers(&mut j, 20);
        j.game_time += 300;
        j.idle();
        let watched = e.conflict_stats().0.raids_watched;
        assert_eq!(
            watched, live_arrival,
            "the held arrival resolved, unwatched"
        );
        let live = books(&e, p).0;
        let gt = j.game_time;
        let saved = j.crash();

        let (paged, e, mut j) = reopen_from_the_first_live_records_pages(&dir, saved, gt, first, p);
        assert!(
            paged.is_some_and(|x| x.0 == Phase::Out),
            "the pages hold the held arrival on its last segment: {paged:?}"
        );
        advance_until(&mut j, 20, DAY, "the arrival run after the replay", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Home)
        });
        assert_eq!(e.conflict_stats().0.raids_watched, watched, "no fight read");
        assert_eq!(
            books(&e, p).0,
            live,
            "the band's books as in the session that went on"
        );
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// SEC-1's meeting-point review, item 7: a save due at a REASSERT (§2.2 trigger 1, §3.2,
    /// RP-42). On main a session's first live record is its JOURNAL_GEN, so the REASSERT here is
    /// the first record past the save Java asks for, right after the JOURNAL_GEN in the session's
    /// first hand-over. S0's casualties are lost with the journal's tail (the next session opens
    /// on the world save taken with the pages); S1's holder journals its copies before the first
    /// hand-over, and Java asks for a save through the JOURNAL_GEN. The save is taken before the
    /// first copy is admitted: the pages end at the JOURNAL_GEN, without the copies, which then
    /// count once. A crash, and S2 replays the copies from those pages: counted once, not twice
    /// and not lost.
    #[test]
    fn a_save_due_at_a_reassert_takes_the_pages_before_the_copy() {
        const SAVES_DONE: usize = 9;
        let Crashed {
            dir,
            p,
            n,
            lost,
            kept,
            before,
            gt,
            ..
        } = a_fight_then_a_crash("conflict-save-at-a-copy");
        let k = kept.records.len() as u64;
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), before);
        j.module_mask = MODULES;
        j.game_time = gt;
        let first = j.next_obs - 1;
        assert_eq!(
            j.journal
                .back()
                .map(|(s, r)| (*s, wire::u16_at(r, wire::R_KIND))),
            Some((first, obs::JOURNAL_GEN)),
            "the session's first record"
        );
        let saves = e.stats_array()[SAVES_DONE];
        e.request_save(first);
        reassert_all(&mut j, &kept, kept.nonce);
        let deadline = Instant::now() + DURABLE_WAIT;
        while e.durable_obs_seq() < first {
            assert!(Instant::now() < deadline, "the save never landed");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            e.durable_obs_seq(),
            first,
            "the pages end at the JOURNAL_GEN, before the copies"
        );
        assert_eq!(e.stats_array()[SAVES_DONE], saves + 1, "one save");
        assert_eq!(books(&e, p).0, n - lost, "the copies count once, live");
        assert_eq!(copies(&e), (k, 0));
        let gt = j.game_time;
        let saved = j.crash();

        let (e, j) = reopen(&dir, saved, gt);
        assert_eq!(e.durable_obs_seq(), first, "the pages of that save");
        assert_eq!(
            books(&e, p).0,
            n - lost,
            "the replay counts the copies once"
        );
        assert_eq!(copies(&e), (k, 0), "applied from the replay");
        assert_eq!(replay_counts(&e), (1, 1, k), "the copies, replayed");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    /// SEC-1's meeting-point review, item 7: a raid's arrival in the replayed window, held
    /// (§6.1, RP-23), and its replayed fight. S0: the player at the target sees the band leave
    /// the camp; a save while it is on its way; its arrival is a watched fight (a
    /// BAND_MATERIALIZED at the next epoch), a quarter of the wave falls (BAND_CASUALTIES); a
    /// crash. S1 replays both records from the pages taken before the arrival: the arrival is
    /// held, the BAND_MATERIALIZED (inside the hold's slack, RP-48) is honoured as its fight, the
    /// casualties count against that epoch's stack, and nothing is resolved abstractly. The
    /// band's books, its epoch and the watched fights are S0's, and the fight is Waiting,
    /// abstract, at the replay end.
    #[test]
    fn a_held_arrivals_replayed_fight_counts_its_casualties_once() {
        let resolved = |e: &Engine| {
            e.conflict_log(true)
                .iter()
                .filter(|l| l.what == "finds nothing to raid")
                .count()
        };
        let (dir, e, mut j) = camp("conflict-held-fight");
        j.conflict_plugin.fight = Some(synth::Fight {
            kill_share: 0.25,
            after: 1_500,
            kill_chief: false,
            only_at_base: true,
        });
        let p = order(&e, &mut j, PartyKind::Raid, Step::Raid);
        on_step(&e, p, Step::Raid);
        advance_until(&mut j, 20, DAY, "the raid on its way, in view", |j| {
            party(&j.e, p) == Some((Phase::Out, true))
        });
        save(&e, &mut j);
        let durable = e.durable_obs_seq();
        let (n, at_save) = (books(&e, p).0, books(&e, p).1);
        let watched = e.conflict_stats().0.raids_watched;
        let from = j.conflict.len();
        advance_until(&mut j, 20, DAY, "the raid fighting", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Fighting)
        });
        advance_until(&mut j, 100, DAY, "its casualties journaled", |j| {
            books(&j.e, p).0 < n
        });
        fillers(&mut j, 20);
        j.game_time += 300;
        j.idle();
        assert_eq!(materialized_epochs(&j, from, p), vec![at_save + 1]);
        let replayed = |kind: u16| {
            j.journal
                .iter()
                .filter(|(s, r)| *s > durable && wire::u16_at(r, wire::R_KIND) == kind)
                .count()
        };
        let (m, c) = (
            replayed(cb::BAND_MATERIALIZED),
            replayed(cb::BAND_CASUALTIES),
        );
        assert!(
            m >= 1 && c > 20,
            "the fight's records after the pages: {m}, {c}"
        );
        let live = books(&e, p);
        assert!(live.0 < n && live.1 == at_save + 1, "{live:?}");
        assert_eq!(party(&e, p), Some((Phase::Fighting, true)));
        assert_eq!(e.conflict_stats().0.raids_watched, watched + 1);
        assert_eq!(resolved(&e), 0, "S0 resolved nothing");
        let gt = j.game_time;
        let saved = j.crash();

        let (e, j) = reopen(&dir, saved, gt);
        assert_eq!(
            e.durable_obs_seq(),
            durable,
            "the pages from before the arrival"
        );
        assert_eq!(resolved(&e), 0, "the held arrival resolved nothing");
        assert_eq!(
            e.conflict_stats().0.raids_watched,
            watched + 1,
            "the replayed BAND_MATERIALIZED honoured as the arrival's fight"
        );
        let b = books(&e, p);
        assert_eq!(
            (b.0, b.1),
            (live.0, live.1),
            "the casualties counted once, at the fight's epoch"
        );
        assert_eq!(
            party(&e, p),
            Some((Phase::Waiting, false)),
            "the fight Waiting, abstract, at the replay end"
        );
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    // ---- The fix round: a held arrival across two crashes (§6.1) ------------------------------

    /// The fix round's item 8 (§6.1): a session's held arrival stays held across a second
    /// session that ends before its replay end, by a crash or by a clean close (`clean`, the
    /// re-review's item 3: `close(true)`, the path RP-42 changed). S0: a raid, unwatched,
    /// reaches its target after the save, resolves (it finds nothing to raid) and walks home;
    /// 4,000 fillers; a crash. S1 replays at MIN_CAPACITY, past the arrival (held, nothing
    /// resolved), journals records of its own and ends before its replay end. It takes no page
    /// while replaying (RP-42), so S2 opens on the same pages and replays S0's tail and S1's
    /// records whole: the arrival is held again and resolved once, after S2's replay end (inside
    /// the hold's slack, RP-48), its books as in S0. What is lost is time: the arrival resolves
    /// after S2's replay end, not at its tick in S0.
    fn a_held_arrival_across_a_second_session(name: &str, clean: bool) {
        use std::sync::atomic::Ordering::Acquire;
        let resolved = |e: &Engine| {
            e.conflict_log(true)
                .iter()
                .filter(|l| l.what == "finds nothing to raid")
                .count()
        };
        let (dir, e, mut j) = camp(name);
        j.players.clear();
        j.game_time += 2_500;
        j.idle();
        let target = (CAMP_AT.0 + 600, CAMP_AT.1);
        let p = order_to(&e, &mut j, PartyKind::Raid, Step::Raid, target);
        on_step(&e, p, Step::Raid);
        advance_until(&mut j, 100, 3 * DAY, "the raid on its way", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Out)
        });
        save(&e, &mut j);
        let pages = e.durable_obs_seq();
        let arrival = e.with_conflict(|c, _| *c.parties[p].times.last().unwrap());
        let _ = resolved(&e);
        advance_until(
            &mut j,
            100,
            3 * DAY,
            "the raid arrived and going home",
            |j| party(&j.e, p).is_some_and(|x| x.0 == Phase::Home),
        );
        assert_eq!(resolved(&e), 1, "S0 resolves the arrival");
        let live = books(&e, p).0;
        fillers(&mut j, 4_000);
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();

        // S1: replays past the arrival, journals 20 records of its own, and crashes or closes.
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        let _ = e.conflict_log(true);
        j.flush();
        assert_eq!(replay_counts(&e).0, 0, "S1 still replaying");
        assert!(
            e.with_conflict(|c, _| c.now()) > arrival,
            "S1's replay past the arrival"
        );
        assert_eq!(party(&e, p), Some((Phase::Out, false)), "held in S1");
        assert_eq!(resolved(&e), 0, "S1 resolves nothing");
        fillers(&mut j, 20);
        assert_eq!(
            e.stats.window_grew.load(Acquire),
            0,
            "S0 saved, so S1's window is S0's tail alone (RP-49)"
        );
        let gt = j.game_time;
        let saved = if clean {
            let saved = j.world_save();
            drop(j);
            e.close(true, Duration::from_secs(5)).unwrap();
            saved
        } else {
            j.crash()
        };
        let tail = saved.journal.iter().filter(|(s, _)| *s > pages).count() as u64;

        // S2: the same pages, S0's tail and S1's records replayed whole.
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        assert_eq!(e.durable_obs_seq(), pages, "S1 took no page");
        let _ = e.conflict_log(true);
        hand_over_all(&mut j);
        // All but S1's JOURNAL_GEN, which is never dispatched.
        assert_eq!(replay_counts(&e), (1, 1, tail - 1), "S0's and S1's records");
        assert_eq!(
            e.stats.window_grew.load(Acquire),
            1,
            "S1 ended with no save, so S2's window holds its records too: logged (RP-49)"
        );
        advance_until(&mut j, 20, DAY, "the arrival run after S2's replay", |j| {
            party(&j.e, p).is_some_and(|x| x.0 == Phase::Home)
        });
        assert_eq!(resolved(&e), 1, "resolved once, after S2's replay end");
        assert_eq!(books(&e, p).0, live, "the band's books as in S0");
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
    }

    #[test]
    fn a_held_arrival_across_a_second_crash_before_the_replay_end_is_resolved_once() {
        a_held_arrival_across_a_second_session("conflict-held-two-crashes", false);
    }

    /// The re-review's item 3: S1 closes cleanly while replaying. With RP-42's guard in `close`
    /// off, S1 saves its pages mid-replay and S2 opens on them: durable through observation
    /// 1,026, a record S1 replayed, not S0's save at 3; S2 then replays 2,997 records, not
    /// 4,020. The arrival is still resolved once with S0's books, by `held_since`'s inference
    /// from pages saved mid-replay, kept as a defence (§6.1, RP-40).
    #[test]
    fn a_held_arrival_across_a_clean_close_before_the_replay_end_is_resolved_once() {
        a_held_arrival_across_a_second_session("conflict-held-a-clean-close", true);
    }

    // ---- The fix round: phase 1 shows nothing while replaying (RP-44) ------------------------

    /// The villages of [`phase1_across_a_replay`]: a player stands in A, on its roads to B and C.
    const VILLAGES: [(u64, &str, i32, i32); 3] = [
        (0x0001_0000_0000_00B1, "minecraft:village_plains", 0, 0),
        (0x0001_0000_0000_00B2, "minecraft:village_taiga", 400, 0),
        (0x0001_0000_0000_00B3, "minecraft:village_desert", 0, 400),
    ];

    /// What phase 1 sent around a replay ([`phase1_across_a_replay`]): (MARKET, SPAWN_GROUP)
    /// commands while the replay ran and in the hand-overs after its end, and its hand-overs.
    struct Phase1Replay {
        during: (usize, usize),
        after: (usize, usize),
        hand_overs: u64,
    }

    /// RP-44 (the fix round's item 4): three villages, a goblin camp far off (so the conflict runs
    /// and the world is held while replaying, docs/replay-gate.md §3.6), and a player standing in
    /// village A, on its roads; days on until a caravan is materialized near it, then a save,
    /// 4,000 fillers and a crash. The next session hands the journal over at `capacity`, one
    /// hand-over at a time, the player where it was.
    fn phase1_across_a_replay(name: &str, capacity: usize) -> Phase1Replay {
        let dir = tmpdir(name);
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
        for (key, structure, x, z) in VILLAGES {
            j.adopt(key, structure, "human", 90, x, z);
        }
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let at = [10.0, 64.0, 0.0];
        j.players = vec![(PLAYER, dim, at)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        j.idle();
        found(&mut j, (6_000, 6_000));
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let counts = |j: &Java| (j.p1_of(cmd::MARKET).len(), j.p1_of(cmd::SPAWN_GROUP).len());
        assert!(counts(&j).0 > 0, "the player in A has its market");
        let mut quanta = 0;
        while counts(&j).1 == 0 {
            assert!(
                quanta < 240,
                "no caravan near the player: {:?}",
                e.phase1_stats()
            );
            e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
            // Game time on too: phase 1 looks for what players see every 20 ticks of it.
            j.game_time += 100;
            j.idle();
            quanta += 1;
        }
        save(&e, &mut j);
        fillers(&mut j, 4_000);
        let gt = j.game_time;
        let saved = j.crash();

        let e = engine(&dir, capacity);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = MODULES;
        j.game_time = gt;
        j.players = vec![(PLAYER, dim, at)];
        j.conflict_plugin = synth::FakeConflict::new(PLAYER);
        let mut during = (0, 0);
        for _ in 0..64 {
            j.flush();
            if replay_counts(&e).0 > 0 {
                break;
            }
            during = counts(&j);
        }
        let (ended, hand_overs, _) = replay_counts(&e);
        assert_eq!(ended, 1, "the replay ended");
        j.idle();
        let now = counts(&j);
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        drop(j);
        e.close(true, Duration::from_secs(5)).unwrap();
        Phase1Replay {
            during,
            after: (now.0 - during.0, now.1 - during.1),
            hand_overs,
        }
    }

    /// A replay of several hand-overs: no market and no caravan while it runs, both once it has
    /// ended.
    #[test]
    fn phase1_shows_nothing_while_replaying() {
        let r = phase1_across_a_replay("phase1-replay-long", wire::MIN_CAPACITY);
        assert!(r.hand_overs >= 3, "a replay of {} hand-overs", r.hand_overs);
        assert_eq!(r.during, (0, 0), "(markets, caravans) sent while replaying");
        assert!(
            r.after.0 > 0 && r.after.1 > 0,
            "the market and the caravan after the replay end: {:?}",
            r.after
        );
    }

    /// Control: the same replay in one hand-over, which ends before phase 1 runs.
    #[test]
    fn phase1_shows_its_market_and_caravan_after_a_one_hand_over_replay() {
        let r = phase1_across_a_replay("phase1-replay-one", ONE_HAND_OVER);
        assert_eq!(r.hand_overs, 1, "a replay of one hand-over");
        assert_eq!(r.during, (0, 0));
        assert!(
            r.after.0 > 0 && r.after.1 > 0,
            "the market and the caravan in its first hand-over: {:?}",
            r.after
        );
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
        save_durable(e, j);
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
