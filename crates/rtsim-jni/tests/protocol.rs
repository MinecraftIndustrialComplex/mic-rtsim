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
    fn robbed(&mut self, agent: u64, epoch: u32, member: u32, items: &[(&str, i32)], op: u64) -> u64 {
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
    let (a, b, c) = (0x0001_0000_0000_0001u64, 0x0001_0000_0000_0002, 0x0001_0000_0000_0003);
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
        .find(|s| wire::u64_at(&s, cmd::spawn_group::AGENT) == agent)
        .unwrap();
    assert_eq!(
        j.cmd_name(wire::DICT_BODY, wire::u32_at(&spawn, cmd::spawn_group::GROUP_KIND)),
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
        let (_, item, n) = cargo.iter().find(|(_, i, _)| i != EMERALD).unwrap_or(&cargo[0]);
        (item.clone(), (*n).min(5))
    };
    let member = cargo.iter().find(|(_, i, _)| *i == robbed_item).unwrap().0;
    j.robbed(agent, 1, member, &[(&robbed_item, robbed_n)], wire::JAVA_OP_BIT | 0x7a12);
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
    eprintln!("start with a phase 1 replay of {inputs} inputs: {:?}", t0.elapsed());
    let (_, replay_ok, _) = e.phase1_state();
    assert!(replay_ok, "the replayed world is the saved one");
    j.idle();
    assert_eq!(p1_stat(&e, 0), 3, "the villages are back");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}
