// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships' replay gate on the real host (docs/replay-gate.md §9.5, RP-90 to RP-99 and RP-120 to
//! RP-122): the fake Java side's journal replayed after a crash, with the ships' module on. The
//! engine-level tests are rtsim-ships' `tests/replay.rs`; these are the ones that need the host:
//! the journaled dispatch, the copies (REASSERT), the hand-overs of a long replay and the orders.

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
    coast_in(j, false);
}

/// [`coast`], the haven reported before the ports when `haven_first`: the record that first puts
/// the ships in the world is then the HAVEN_FOUND, whose pirates the engine allocates at once.
fn coast_in(j: &mut Java, haven_first: bool) {
    for (v, structure, x, z) in VILLAGES {
        j.adopt(v, structure, "human", 90, x, z);
    }
    j.idle();
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let haven = |j: &mut Java| {
        j.ship_obs(&O::HavenFound(sb::HavenFound {
            haven: HAVEN,
            media: 1,
            dimension: dim,
            structure: 0,
            dock: (200, 63, 200),
        }));
    };
    if haven_first {
        haven(j);
    }
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
    if !haven_first {
        haven(j);
    }
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
    watched: bool,
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
    let mid = e.with_ships(|s, _| {
        let p = &s.lanes[lane].path;
        p.at(p.length() / 2)
    });
    if watched {
        // A player already standing there at the save: the pages hold its interest and the
        // sightings it booked.
        stand(&mut j, mid);
        j.idle();
        j.idle();
    }
    save_durable(&e, &mut j);
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
    let (dir, e, mut j, mid) = scene("ships-replay-damaged", 256 * 1024, false);
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
    let (dir, e, mut j, mid) = scene("ships-replay-clean", 256 * 1024, false);
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
    let (dir, e, mut j, mid) = scene("ships-replay-epoch", 256 * 1024, false);
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
/// at the save; the journal behind it is 8,000 records over ten minutes of game time, eight
/// hand-overs on a minimum-size buffer, so the replay's clock runs the ships' events for ten
/// minutes. No ship command is written before the replay ends, the gate saw no "never" kind, and
/// the engine says it held decisions back (`deferred`); once the replay has ended, the ships in
/// view are materialized (the control: the same player and lane see ships).
///
/// This one passes on the base too, and is a regression guard, not the gate's proof: the ships
/// have no player until the replay ends (`shared::simulate` routes none while replaying, RP-11),
/// so no sighting reaches them through the host. The early returns (RP-94) are proved where a
/// player can be registered during the replay, in rtsim-ships' `tests/replay.rs`
/// (`no_ship_embodies_while_the_replay_runs`).
#[test]
fn no_ship_is_materialized_while_a_long_replay_runs() {
    let (dir, e, mut j, mid) = scene("ships-replay-quiet", wire::MIN_CAPACITY, true);
    let first = j.world_save().next_obs;
    for _ in 0..8 {
        j.game_time += 1_500;
        fillers(&mut j, 1_000);
    }
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
    // Every hand-over of the replay: no ship command so far.
    let mut handed_over = 0;
    while replay_counts(&e).0 == 0 {
        assert!(
            j.ships.is_empty(),
            "a ship command while the replay runs, after {handed_over} idles: {:?}",
            j.ships.iter().map(|(_, c)| c.kind()).collect::<Vec<_>>()
        );
        assert!(handed_over < 64, "the replay never ended");
        j.idle();
        handed_over += 1;
    }
    assert!(handed_over >= 3, "a replay of several hand-overs");
    assert_eq!(replay_counts(&e).0, 1, "the replay ended");
    let gate = e.ship_gate_counts();
    eprintln!("quiet replay: {gate:?}, {} commands after", j.ships.len());
    assert_eq!(gate.never, 0, "no decision that embodies got by: {gate:?}");
    assert!(
        gate.deferred > 0,
        "the replay held decisions back: {gate:?}"
    );
    // The control: once the replay has ended the player sees the lane's ships.
    j.run_until("a ship in view after the replay", |j| {
        !materializations(j).is_empty()
    });
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
    let (dir, e, mut j, _) = scene(name, wire::MIN_CAPACITY, false);
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

// ---- the session's bound and the fold before the drop (RP-120, RP-122) -------------------------

/// A ship's (embodied, epoch).
fn body(e: &Engine, agent: u64) -> (bool, u32) {
    e.with_ships(|s, _| {
        let x = s.ship(Ship::id_of_agent(agent).unwrap()).unwrap();
        (x.embodied, x.epoch)
    })
}

/// RP-122: an epoch a replayed record named is raised as a floor before anything drops the record,
/// the module being off for the session among the reasons. A merchant's body ends after the save
/// (Java's DEMATERIALIZED); the crash; the next session starts with the ships off and replays the
/// journal (dropped), and the ships are switched on after the replay end. The ship's next body is
/// above the epoch the journal named, as it is when the ships were on through the replay (the
/// control, RP-57).
#[test]
fn an_epoch_named_while_the_ships_were_off_is_not_reused_once_they_are_on() {
    for off in [false, true] {
        let name = if off {
            "ships-replay-fold-off"
        } else {
            "ships-replay-fold-on"
        };
        let (dir, e, mut j, mid) = scene(name, 256 * 1024, false);
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
        let highest = materializations(&j)
            .iter()
            .filter(|n| n.agent == m.agent)
            .map(|n| n.epoch)
            .max()
            .unwrap();
        let saved = j.world_save();
        let gt = j.game_time;
        let _ = j.crash();

        let (e, mut j) = reopen(&dir, saved, gt, 256 * 1024);
        if off {
            j.module_mask = MODULES & !MODULE_SHIPS;
        }
        j.idle();
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        if off {
            // The ships come on after the replay end.
            j.modules(MODULES);
            j.idle();
            j.idle();
        }
        let next = e.with_ships(|s, _| s.next_epoch(Ship::id_of_agent(m.agent).unwrap()));
        assert!(
            next > highest,
            "ships off {off}: the next body is numbered {next}, and the last session spent {highest}"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

/// RP-122: the same for a latest-wins state record (`observe_state`, not journaled), which the
/// plugin's first hand-over of the new session may carry while the replay runs: a SHIP_STATE
/// names a body of the earlier session, whose epoch is spent whether or not the ships are on. The
/// next body of the ship, once they are, is above it. Control: the ships on through the replay.
#[test]
fn an_epoch_a_state_record_named_while_the_ships_were_off_is_not_reused() {
    for off in [false, true] {
        let name = if off {
            "ships-replay-state-off"
        } else {
            "ships-replay-state-on"
        };
        let (dir, e, mut j, mid) = scene(name, wire::MIN_CAPACITY, false);
        stand(&mut j, mid);
        j.run_until("a crewed merchant in view", |j| {
            laden(j, &j.e.clone()).is_some()
        });
        let m = laden(&j, &e).expect("found");
        let highest = materializations(&j)
            .iter()
            .filter(|n| n.agent == m.agent)
            .map(|n| n.epoch)
            .max()
            .unwrap();
        let named = highest + 5;
        // A long journal behind the save: the replay spans several hand-overs, and the state
        // record comes in the first.
        fillers(&mut j, 4_000);
        let gt = j.game_time;
        let saved = j.crash();
        drop(e);

        let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
        if off {
            j.module_mask = MODULES & !MODULE_SHIPS;
        }
        // The first hand-over opens the session and starts the replay.
        j.idle();
        assert_eq!(
            replay_counts(&e).0,
            0,
            "the replay runs past its first hand-over"
        );
        let mut b = Vec::new();
        sb::encode_obs(
            0,
            j.game_time,
            &O::State(sb::State {
                agent: m.agent,
                epoch: named,
                flags: 0,
                dimension: 0,
                hull_milli: 1000,
                crew: 3,
                pos: (f64::from(mid.0), 63.0, f64::from(mid.1)),
            }),
            &mut b,
        );
        assert!(j.copy(&b), "room for the state record");
        j.idle();
        assert_eq!(
            replay_counts(&e).0,
            0,
            "the replay runs past its second hand-over"
        );
        let mut rounds = 0;
        while replay_counts(&e).0 == 0 {
            assert!(rounds < 64, "the replay never ended");
            j.idle();
            rounds += 1;
        }
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        if off {
            // The ships come on after the replay end.
            j.modules(MODULES);
            j.idle();
            j.idle();
        }
        let next = e.with_ships(|s, _| s.next_epoch(Ship::id_of_agent(m.agent).unwrap()));
        assert!(
            next > named,
            "ships off {off}: the next body is numbered {next}, and the state record spent {named}"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

/// The agent id of the first ship a found coast allocates (the haven's pirate), learned on a
/// scratch engine.
fn first_ships_agent() -> u64 {
    let dir = tmpdir("ships-replay-first-agent");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    coast(&mut j);
    let agent = e.with_ships(|s, _| Ship::agent(s.ships.iter().next().expect("a ship").0));
    e.close(true, Duration::from_secs(5)).unwrap();
    agent
}

/// RP-122: the fold is keyed on the session's replay, not on the ships' own: a record journaled
/// before the ships have any dock or haven (the engine is not running yet, so its replay has not
/// begun) names an epoch all the same, and the ship that is the record's, found afterwards, has
/// it as a floor. Control: the same record on pages that hold the coast (the replay begins at
/// the session start).
#[test]
fn an_epoch_named_before_the_ships_had_a_dock_is_not_reused() {
    for coast_first in [true, false] {
        let name = if coast_first {
            "ships-replay-floor-docks"
        } else {
            "ships-replay-floor-nodock"
        };
        let (dir, e, mut j, agent) = if coast_first {
            let (dir, e, j, _) = scene(name, wire::MIN_CAPACITY, false);
            let agent = e.with_ships(|s, _| Ship::agent(s.ships.iter().next().expect("a ship").0));
            (dir, e, j, agent)
        } else {
            let dir = tmpdir(name);
            let e = engine(&dir, wire::MIN_CAPACITY);
            let mut j = Java::open(e.clone(), WorldSave::fresh());
            j.module_mask = MODULES;
            j.idle();
            (dir, e, j, first_ships_agent())
        };
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        j.ship_obs(&O::Materialized(sb::Materialized {
            agent,
            epoch: 7,
            dimension: dim,
            entity: (0x11, 0x22),
        }));
        j.idle();
        let gt = j.game_time;
        let saved = j.crash();
        drop(e);

        let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
        j.idle();
        j.idle();
        j.idle();
        assert_eq!(replay_counts(&e).0, 1, "the replay ended");
        if !coast_first {
            // The coast is found after the replay end: the ships are in the world now.
            coast(&mut j);
            j.idle();
        }
        let next = e.with_ships(|s, _| s.next_epoch(Ship::id_of_agent(agent).unwrap()));
        assert_eq!(
            next, 8,
            "docks first {coast_first}: above the epoch 7 the journal named"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

/// A session on pages that hold an empty ships engine (saved before any dock): the coast is
/// found (the haven first, or last), the haven's pirate (handle 0 when the haven is first, the
/// replay's own either way) is materialized at epoch 3 between 4,000 fillers and 4,000 more, then
/// a crash. With `switch` the ships are switched off and on again after the replay's first
/// hand-over (a MODULES pair), which begins their replay again. Returns whether the pirate was
/// ever seen embodied between hand-overs, and its (embodied, epoch) and next epoch after the
/// replay end.
fn pirate_on_empty_pages(switch: bool, haven_first: bool) -> (bool, (bool, u32), u32) {
    let name = format!(
        "ships-replay-{}-{}",
        if switch { "switch" } else { "empty-pages" },
        if haven_first {
            "haven-first"
        } else {
            "ports-first"
        }
    );
    let dir = tmpdir(&name);
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    j.idle();
    save_durable(&e, &mut j);
    coast_in(&mut j, haven_first);
    let agent = e.with_ships(|s, _| Ship::agent(s.havens[0].ships[0]));
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    fillers(&mut j, 4_000);
    j.ship_obs(&O::Materialized(sb::Materialized {
        agent,
        epoch: 3,
        dimension: dim,
        entity: (0x11, 0x22),
    }));
    fillers(&mut j, 4_000);
    j.idle();
    let gt = j.game_time;
    let saved = j.crash();
    drop(e);

    let (e, mut j) = reopen(&dir, saved, gt, wire::MIN_CAPACITY);
    j.idle();
    assert_eq!(
        replay_counts(&e).0,
        0,
        "the replay runs past its first hand-over"
    );
    let mut seen = false;
    if switch {
        j.modules(0);
        j.modules(MODULES);
    }
    let mut rounds = 0;
    while replay_counts(&e).0 == 0 {
        assert!(rounds < 64, "the replay never ended");
        j.idle();
        rounds += 1;
        if replay_counts(&e).0 == 0 {
            seen |= body(&e, agent).0;
        }
    }
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let end = body(&e, agent);
    let next = e.with_ships(|s, _| s.next_epoch(Ship::id_of_agent(agent).unwrap()));
    e.close(true, Duration::from_secs(5)).unwrap();
    (seen, end, next)
}

/// RP-120: the ships the engine holds when the session begins are the only ones a replayed record
/// finds, and the first record that activates the engine (with the haven first, the HAVEN_FOUND,
/// whose pirate takes handle 0) is ingested before the replay begins. A SHIP_MATERIALIZED for
/// that handle is a floor only: the pirate is never seen embodied, and its epoch is the floor at
/// the replay end. With the ports first the engine is running before the haven's record and the
/// replay begins in time on either code (the order the session that went on happened to take).
/// The control (`a_replayed_materialization_is_honoured_on_the_pages_ship`) is the pages' ship,
/// honoured.
#[test]
fn a_replayed_materialization_is_a_floor_for_the_first_activating_havens_pirate() {
    for haven_first in [true, false] {
        let (seen, end, next) = pirate_on_empty_pages(false, haven_first);
        assert!(
            !seen,
            "haven first {haven_first}: the replay's pirate was embodied by a record that named a live ship"
        );
        assert_eq!(
            end,
            (false, 3),
            "haven first {haven_first}: the floor folds at the replay end"
        );
        assert_eq!(next, 4, "haven first {haven_first}");
    }
}

/// RP-120: the switch off and on inside the replay (a MODULES pair) begins the ships' replay
/// again, when the arena holds the replay's own ships too; the bound stays the session's.
#[test]
fn a_switch_off_and_on_inside_the_replay_does_not_widen_the_honour() {
    for haven_first in [true, false] {
        let (seen, end, next) = pirate_on_empty_pages(true, haven_first);
        assert!(
            !seen,
            "haven first {haven_first}: the replay's pirate was embodied by a record that named a live ship"
        );
        assert_eq!(
            end,
            (false, 3),
            "haven first {haven_first}: the floor folds at the replay end"
        );
        assert_eq!(next, 4, "haven first {haven_first}");
    }
}

/// RP-97 (review P3.4): a copy (REASSERT) of a HAVEN_FOUND joins no haven node to the world,
/// since `Ships::ingest_copy` only folds the epoch: a node no haven of the engine stands behind
/// would be a market with no pirates. The control: the session's own HAVEN_FOUND, the same
/// bytes, joins it.
#[test]
fn a_copy_of_a_haven_found_joins_no_node_to_the_world() {
    for copy in [false, true] {
        let dir = tmpdir(if copy {
            "ships-haven-copy"
        } else {
            "ships-haven-own"
        });
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), WorldSave::fresh());
        j.module_mask = MODULES;
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
        let o = O::HavenFound(sb::HavenFound {
            haven: HAVEN,
            media: 1,
            dimension: dim,
            structure: 0,
            dock: (200, 63, 200),
        });
        if copy {
            let mut b = Vec::new();
            sb::encode_obs(0, j.game_time, &o, &mut b);
            wire::put_u64(&mut b, wire::obs::OBS_SEQ, 7);
            j.reassert(EARLIER_GEN, &b);
        } else {
            j.ship_obs(&o);
        }
        j.idle();
        j.idle();
        assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
        let joined = e.with_ships(|_, w| w.node_of_key(HAVEN).is_some());
        assert_eq!(
            joined, !copy,
            "copy {copy}: the haven node is in the world only when the record was the session's"
        );
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}
