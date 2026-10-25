// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships' replay gate on the real host (docs/replay-gate.md §9.5, RP-90 to RP-99): the fake
//! Java side's journal replayed after a crash, with the ships' module on. The engine-level tests
//! are rtsim-ships' `tests/replay.rs`; these are the ones that need the host: the journaled
//! dispatch, the copies (REASSERT), the hand-overs of a long replay and the orders.

mod common;

use std::time::{Duration, Instant};

use common::*;
use rtsim_jni::engine::{Engine, MODULE_SHIPS};
use rtsim_jni::orders::Order;
use rtsim_jni::wire;
use rtsim_ships::boundary::{self as sb, By, Cmd as S, Obs as O};
use rtsim_ships::engine::{Ship, ShipId};

/// Knowledge, settlements, trade, travel and ships.
const MODULES: u32 = 0b1111 | MODULE_SHIPS;
const HAVEN: u64 = 0x0000_0000_0000_0042;
const VILLAGES: [(u64, &str, i32, i32); 3] = [
    (0x0001_0000_0000_0011, "minecraft:village_plains", 0, 0),
    (0x0001_0000_0000_0012, "minecraft:village_taiga", 400, 0),
    (0x0001_0000_0000_0013, "minecraft:village_desert", 0, 400),
];

/// How long a test waits for its save to become durable before it gives up.
const DURABLE_WAIT: Duration = Duration::from_secs(60);

/// Saves at the next step and waits until the pages on disk hold every observation the engine has
/// applied (protocol.rs's `save_durable`, which this copies).
fn save_durable(e: &Engine, j: &mut Java) {
    const APPLIED: usize = 4;
    const DURABLE: usize = 5;
    const SAVES_DONE: usize = 9;
    const SAVE_FAILURES: usize = 15;
    let deadline = Instant::now() + DURABLE_WAIT;
    let give_up = |e: &Engine, rounds: u32| -> ! {
        let s = e.stats_array();
        panic!(
            "the pages never became durable: waited {DURABLE_WAIT:?}; durable_obs_seq {}, applied \
             through {} ({rounds} save rounds, {} saves done, {} failed)",
            s[DURABLE], s[APPLIED], s[SAVES_DONE], s[SAVE_FAILURES]
        );
    };
    let mut rounds = 0;
    loop {
        let before = e.stats_array();
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        rounds += 1;
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

/// Three villages with sea ports and a haven between them, reported by the plugin; the network is
/// built at the next simulate point.
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

/// The materializations the plugin applied: the records.
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

/// A coast with goods on a lane, saved: the engine, its Java, its directory, and the lane's
/// midpoint (where a player sees the lane's ships).
fn scene(
    name: &str,
    capacity: usize,
) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java, (i32, i32)) {
    let dir = tmpdir(name);
    let e = engine(&dir, capacity);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    coast(&mut j);
    let lanes = e.with_ships(|s, _| s.lanes.len());
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
    save_durable(&e, &mut j);
    let mid = e.with_ships(|s, _| {
        let p = &s.lanes[lane].path;
        p.at(p.length() / 2)
    });
    (dir, e, j, mid)
}

/// A player standing at `at` in the overworld.
fn stand(j: &mut Java, at: (i32, i32)) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.players = vec![(PLAYER, dim, [f64::from(at.0), 64.0, f64::from(at.1)])];
}

/// The next session over `saved`: an engine of `capacity` bytes, its ships on, game time
/// `gt`.
fn reopen(
    dir: &std::path::PathBuf,
    saved: WorldSave,
    gt: i64,
    capacity: usize,
) -> (std::sync::Arc<Engine>, Java) {
    let e = engine(dir, capacity);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = MODULES;
    j.game_time = gt;
    (e, j)
}

/// A crewed merchant with a long voyage ahead that the plugin has had materialized: the record.
fn laden(j: &Java, e: &Engine) -> Option<sb::Materialize> {
    let now = e.with_ships(|_, w| w.now().0 as i64);
    materializations(j)
        .into_iter()
        .find(|m| m.role == sb::Role::Merchant && m.crew >= 2 && m.times[3] > now + 400)
}

/// A ship's crew, hull and epoch.
fn row(e: &Engine, agent: u64) -> (u32, u32, u32) {
    e.with_ships(|s, _| {
        let x = s.ship(Ship::id_of_agent(agent).unwrap()).unwrap();
        (x.crew, x.hull_milli, x.epoch)
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

/// Filler a replay must read through: `count` damage reports of a ship the engine never had, which
/// change nothing (about 56 bytes each, so about a thousand fill a minimum-size buffer). 8,000
/// span eight hand-overs.
fn fillers(j: &mut Java, count: usize) {
    for _ in 0..count {
        j.ship_obs(&O::Damaged(sb::Damaged {
            agent: 0,
            epoch: 0,
            by: By::Other,
            hull_milli: 1000,
            crew_lost: 0,
            player: (0, 0),
            ship: 0,
        }));
    }
}

/// §9.5 test 1 (scratch audit :2625; today live (3, 800, 1), after the crash (4, 1000, 0)): a
/// merchant materialized after a save (a new epoch), damaged in view (one crew lost, hull 800),
/// then a crash. The replay must honour the materialization, or the ship keeps the save's epoch
/// and the replayed damage is dropped as a stale report: the dead sailor and the hull come back.
/// Controls: the session that goes on, and a clean stop and restart.
#[test]
fn damage_to_a_ship_seen_after_a_save_is_counted_after_a_crash() {
    let (dir, e, mut j, mid) = scene("ships-replay-damaged", 256 * 1024);
    stand(&mut j, mid);
    j.run_until("a crewed merchant in view", |j| {
        laden(j, &j.e.clone()).is_some()
    });
    let m = laden(&j, &e).expect("found");
    j.ship_obs(&O::Damaged(sb::Damaged {
        agent: m.agent,
        epoch: m.epoch,
        by: By::Player,
        hull_milli: 800,
        crew_lost: 1,
        player: PLAYER,
        ship: 0,
    }));
    j.idle();
    let live = row(&e, m.agent);
    assert_eq!(
        live,
        (m.crew - 1, 800, m.epoch),
        "the damage in the session that went on"
    );
    let saved = j.world_save();
    let gt = j.game_time;
    let _ = j.crash();

    let (e, mut j) = reopen(&dir, saved, gt, 256 * 1024);
    j.idle();
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(replay_counts(&e).0, 1, "the replay ended");
    assert_eq!(
        row(&e, m.agent),
        live,
        "the replayed damage counts, on the epoch the replay honoured"
    );
    let gate = e.ship_gate_counts();
    assert_eq!(gate.never, 0, "nothing of a kind that embodies: {gate:?}");
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The control: the same scene stopped cleanly (the pages hold the ship as it was), restarted.
#[test]
fn damage_to_a_ship_seen_after_a_save_is_counted_after_a_clean_stop() {
    let (dir, e, mut j, mid) = scene("ships-replay-clean", 256 * 1024);
    stand(&mut j, mid);
    j.run_until("a crewed merchant in view", |j| {
        laden(j, &j.e.clone()).is_some()
    });
    let m = laden(&j, &e).expect("found");
    j.ship_obs(&O::Damaged(sb::Damaged {
        agent: m.agent,
        epoch: m.epoch,
        by: By::Player,
        hull_milli: 800,
        crew_lost: 1,
        player: PLAYER,
        ship: 0,
    }));
    j.idle();
    let live = row(&e, m.agent);
    assert_eq!((live.0, live.1), (m.crew - 1, 800));
    save_durable(&e, &mut j);
    let saved = j.world_save();
    let gt = j.game_time;
    e.close(true, Duration::from_secs(5)).unwrap();

    let (e, mut j) = reopen(&dir, saved, gt, 256 * 1024);
    j.idle();
    j.idle();
    let after = row(&e, m.agent);
    assert_eq!(
        (after.0, after.1),
        (live.0, live.1),
        "the pages hold the damage"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// §9.5 test 2, on the host: an epoch the last session's journal named is not spent again. A
/// merchant is materialized and its body ends (Java's DEMATERIALIZED) after the save; after the
/// crash the same ship's next materialization is above both. Without the honour and the floor the
/// replayed records name an epoch the ship does not hold, are dropped, and the next body is
/// numbered again from the save's epoch: Java would take it for the body that ended.
#[test]
fn an_epoch_the_last_session_ended_is_not_reused_after_a_crash() {
    let (dir, e, mut j, mid) = scene("ships-replay-epoch", 256 * 1024);
    stand(&mut j, mid);
    j.run_until("a crewed merchant in view", |j| {
        laden(j, &j.e.clone()).is_some()
    });
    let m = laden(&j, &e).expect("found");
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.ship_obs(&O::Dematerialized(sb::Dematerialized {
        agent: m.agent,
        epoch: m.epoch,
        reason: 1,
        dimension: dim,
        hull_milli: m.hull_milli,
        crew: m.crew,
        pos: (f64::from(mid.0), 63.0, f64::from(mid.1)),
    }));
    j.idle();
    // Every epoch the session spent on this ship.
    let spent = |j: &Java| {
        materializations(j)
            .iter()
            .filter(|n| n.agent == m.agent)
            .map(|n| n.epoch)
            .max()
            .unwrap()
    };
    let highest = spent(&j);
    let saved = j.world_save();
    let gt = j.game_time;
    let _ = j.crash();

    let (e, mut j) = reopen(&dir, saved, gt, 256 * 1024);
    stand(&mut j, mid);
    j.run_until("the ship in view again", |j| {
        materializations(j).iter().any(|n| n.agent == m.agent)
    });
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let next = spent(&j);
    assert!(
        next > highest,
        "the ship's next body is numbered {next}, and the last session spent {highest}"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// §9.5 test 3, on the host: nothing embodies while a long replay runs. A player stands by a lane
/// with ships in view; the journal behind the save is 8,000 records, eight hand-overs on a
/// minimum-size buffer. No ship command is written before the replay ends; once it has, the
/// ships in view are materialized. (The control is the second half: the same player and the
/// same lane see ships.)
#[test]
fn no_ship_is_materialized_while_a_long_replay_runs() {
    let (dir, e, mut j, mid) = scene("ships-replay-quiet", wire::MIN_CAPACITY);
    let first = j.world_save().next_obs;
    fillers(&mut j, 8_000);
    j.idle();
    let gt = j.game_time;
    let saved = j.crash();
    let bytes: usize = saved
        .journal
        .iter()
        .filter(|(s, _)| *s >= first)
        .map(|(_, r)| r.len())
        .sum();
    assert!(
        bytes > 6 * wire::MIN_CAPACITY,
        "the replay's records: {bytes} bytes"
    );
    drop(e);

    let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
    stand(&mut j, mid);
    j.idle();
    assert_eq!(
        replay_counts(&e).0,
        0,
        "the replay runs past its first hand-over"
    );
    for _ in 0..3 {
        j.idle();
        if replay_counts(&e).0 > 0 {
            break;
        }
        assert!(
            materializations(&j).is_empty() && j.ships.is_empty(),
            "no ship command while the replay runs: {:?}",
            j.ships.len()
        );
    }
    let gate = e.ship_gate_counts();
    assert_eq!(gate.never, 0, "no decision that embodies got by: {gate:?}");
    // The control: once the replay has ended the player sees the lane's ships.
    hand_over_all(&mut j);
    j.run_until("a ship in view after the replay", |j| {
        !materializations(j).is_empty()
    });
    assert_eq!(replay_counts(&e).0, 1, "the replay ended");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A despawn of a ship nobody holds: the smallest ship command, which names nothing in the
/// dictionaries.
fn despawn(agent: u64) -> Vec<u8> {
    let mut record = Vec::new();
    sb::encode_cmd(
        0,
        &S::Despawn(sb::Despawn {
            agent,
            epoch: 1,
            reason: 2,
        }),
        &mut record,
    );
    record
}

/// RP-96 (as RP-66 for the conflict): a test's ship command joins the ships' queue around their
/// gate, so one ordered while the replay runs waits for the replay end as the conflict's does.
/// A SHIP_DESPAWN is not written while the replay runs, and is once it has ended.
#[test]
fn a_ship_command_ordered_while_replaying_waits_for_the_replay_end() {
    let dir = tmpdir("ships-replay-order");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    coast(&mut j);
    save_durable(&e, &mut j);
    let first = j.world_save().next_obs;
    // Eight hand-overs: an idle takes two.
    fillers(&mut j, 8_000);
    j.idle();
    let gt = j.game_time;
    let saved = j.crash();
    let bytes: usize = saved
        .journal
        .iter()
        .filter(|(s, _)| *s >= first)
        .map(|(_, r)| r.len())
        .sum();
    assert!(
        bytes > 6 * wire::MIN_CAPACITY,
        "the replay's records: {bytes} bytes"
    );
    let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
    j.idle();
    assert_eq!(
        replay_counts(&e).0,
        0,
        "the replay runs past its first hand-over"
    );
    let agent = Ship::agent(ShipId::from_bits(0x77));
    e.order(vec![Order::ShipCommand {
        names: Vec::new(),
        record: despawn(agent),
    }])
    .unwrap();
    let written = |j: &Java| {
        j.ships
            .iter()
            .any(|(_, c)| matches!(c, S::Despawn(d) if d.agent == agent))
    };
    j.idle();
    assert_eq!(replay_counts(&e).0, 0, "still replaying");
    assert!(!written(&j), "not written while the replay runs");
    hand_over_all(&mut j);
    j.idle();
    assert_eq!(replay_counts(&e).0, 1, "the replay ended");
    assert!(written(&j), "written once it has ended");
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The control: no replay (a fresh world), the same order is written at once.
#[test]
fn a_ship_command_ordered_without_a_replay_is_written_at_once() {
    let dir = tmpdir("ships-replay-order-live");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    coast(&mut j);
    j.idle();
    let agent = Ship::agent(ShipId::from_bits(0x77));
    e.order(vec![Order::ShipCommand {
        names: Vec::new(),
        record: despawn(agent),
    }])
    .unwrap();
    j.idle();
    j.idle();
    assert!(
        j.ships
            .iter()
            .any(|(_, c)| matches!(c, S::Despawn(d) if d.agent == agent)),
        "written at once"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A generation no engine heard: the one a crash lost with its journal.
const EARLIER_GEN: u64 = 0x0E41_11E4_0000_0001;

/// A ship at rest, and a SHIP_MATERIALIZED of an epoch five above its own, journaled first and
/// followed by 8,000 fillers: so that its replay is read in the first hand-over and the engine
/// is seen mid-replay. `copy`: the record is a holder's copy (REASSERT) of a record the crash
/// lost, else the session's own. Returns the ship's agent, its epoch before, and what the engine
/// holds of it after the first hand-over and after the replay end: (embodied, epoch).
fn a_replayed_materialization(copy: bool) -> (u64, u32, (bool, u32), (bool, u32), u32) {
    let name = if copy {
        "ships-replay-copy"
    } else {
        "ships-replay-honour"
    };
    let (dir, e, mut j, _) = scene(name, wire::MIN_CAPACITY);
    let (agent, before) = e.with_ships(|s, _| {
        let (id, x) = s
            .ships
            .iter()
            .find(|(_, x)| x.is_active() && !x.embodied)
            .expect("a ship at rest");
        (Ship::agent(id), x.epoch)
    });
    let spent = before + 5;
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let o = O::Materialized(sb::Materialized {
        agent,
        epoch: spent,
        dimension: dim,
        entity: (0x11, 0x22),
    });
    if copy {
        let mut b = Vec::new();
        sb::encode_obs(0, j.game_time, &o, &mut b);
        wire::put_u64(&mut b, wire::obs::OBS_SEQ, 7);
        j.reassert(EARLIER_GEN, &b);
    } else {
        j.ship_obs(&o);
    }
    fillers(&mut j, 8_000);
    j.idle();
    let gt = j.game_time;
    let saved = j.crash();

    let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
    j.idle();
    assert_eq!(
        replay_counts(&e).0,
        0,
        "the replay runs past its first hand-over"
    );
    let held = |e: &Engine| {
        e.with_ships(|s, _| {
            let x = s.ship(Ship::id_of_agent(agent).unwrap()).unwrap();
            (x.embodied, x.epoch)
        })
    };
    let mid = held(&e);
    hand_over_all(&mut j);
    j.idle();
    assert_eq!(replay_counts(&e).0, 1, "the replay ended");
    let end = held(&e);
    let next = e.with_ships(|s, _| s.next_epoch(Ship::id_of_agent(agent).unwrap()));
    assert_eq!(e.ship_gate_counts().never, 0);
    e.close(true, Duration::from_secs(5)).unwrap();
    (agent, before, mid, end, next)
}

/// RP-97 (a copy is applied by identity): a holder's copy of a SHIP_MATERIALIZED embodies
/// nothing; the engine only raises the ship's epoch floor, which the replay end folds into the
/// ship's epoch. The host routes a REASSERT to the ships' `ingest_copy`; routed to `ingest` it
/// would be honoured like the session's own record (the control), and the ship would be seen
/// embodied in a body the crash took.
#[test]
fn a_copy_of_a_materialization_embodies_nothing_and_raises_the_floor() {
    let (_, before, mid, end, next) = a_replayed_materialization(true);
    assert_eq!(
        mid,
        (false, before),
        "mid-replay: not embodied, its epoch unchanged"
    );
    assert_eq!(end, (false, before + 5), "the replay end folds the floor");
    assert_eq!(next, before + 6, "and the next body is above it");
}

/// The control: the session's own record, replayed, is honoured (RP-6).
#[test]
fn a_replayed_materialization_is_honoured_on_the_pages_ship() {
    let (_, before, mid, end, next) = a_replayed_materialization(false);
    assert_eq!(
        mid,
        (true, before + 5),
        "mid-replay: embodied at the record's epoch"
    );
    assert_eq!(
        end,
        (false, before + 5),
        "abstract at the replay end, the epoch kept"
    );
    assert_eq!(next, before + 6);
}
