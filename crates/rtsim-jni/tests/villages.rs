// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages renew over the real boundary (EQUILIBRIUM.md §2.4.3, slice S6;
//! docs/overnight-choices-equilibrium.md EQC150 to EQC152, EQC162, EQC169): the census records the
//! plugin writes reach the world through a filter that believes a lower count only when two
//! complete readings agree, a settlers' order leaves as a SPAWN_RESIDENT with the arrival flags,
//! and the bridge's answer settles the booking.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use rtsim_jni::engine::Engine;
use rtsim_jni::orders::Order;
use rtsim_jni::wire::{self, cmd, obs};

/// `engine::phase1::village` indexes.
const MEMBERS: usize = 0;
const K: usize = 1;
const SEEN: usize = 2;
const OWED: usize = 3;
const WAIT_DUE: usize = 4;
const PRESSURE_MILLI: usize = 8;
const SEEN_READING: usize = 9;
/// `engine::phase1::stat` indexes.
const P1_SETTLERS: usize = 20;
const P1_EMPTIED: usize = 23;
const P1_LAPSED: usize = 24;
const P1_CENSUSES: usize = 25;

const A: u64 = 0x0001_0000_0000_0021;
const PLAINS: &str = "minecraft:village_plains";
const DURABLE_WAIT: Duration = Duration::from_secs(60);

fn open(tag: &str) -> (std::path::PathBuf, Arc<Engine>, Java) {
    let dir = tmpdir(tag);
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = 0b1111;
    j.adopt(A, PLAINS, "human", 90, 0, 0);
    j.idle();
    (dir, e, j)
}

fn row(e: &Engine) -> [i64; 10] {
    e.phase1_village(A).expect("the village's row")
}

fn stat(e: &Engine, i: usize) -> u64 {
    e.phase1_stats()[i]
}

/// Saves at the next step and waits until the pages hold every observation applied.
fn save_durable(e: &Engine, j: &mut Java) {
    const APPLIED: usize = 4;
    const DURABLE: usize = 5;
    const SAVES_DONE: usize = 9;
    const SAVE_FAILURES: usize = 15;
    let deadline = Instant::now() + DURABLE_WAIT;
    loop {
        let before = e.stats_array();
        e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.idle();
        loop {
            let s = e.stats_array();
            if s[SAVES_DONE] != before[SAVES_DONE] || s[SAVE_FAILURES] != before[SAVE_FAILURES] {
                break;
            }
            assert!(Instant::now() < deadline, "the save never committed");
            std::thread::sleep(Duration::from_millis(2));
        }
        let s = e.stats_array();
        if s[DURABLE] >= s[APPLIED] {
            return;
        }
        assert!(Instant::now() < deadline, "the pages never became durable");
    }
}

/// Advances the world `ticks` in 3,000-tick quanta, Java answering whatever comes.
fn advance(e: &Engine, j: &mut Java, ticks: u64) {
    for _ in 0..ticks.div_ceil(3_000) {
        e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
        j.idle();
    }
}

#[test]
fn village_pop_follows_census_down() {
    let (dir, e, mut j) = open("village-census-down");
    let guess = row(&e)[MEMBERS];
    assert!(guess > 8, "the adoption guessed {guess}");
    assert_eq!(row(&e)[SEEN], 0);
    // The first complete census is believed at once.
    j.village_census(A, 8, 12, true);
    j.idle();
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[K], r[SEEN], r[SEEN_READING]), (8, 12, 1, 8));
    assert_eq!(stat(&e, P1_CENSUSES), 1);
    // A different complete reading is held: one reading is not enough to lower the count.
    j.village_census(A, 3, 12, true);
    j.idle();
    assert_eq!(row(&e)[MEMBERS], 8);
    assert_eq!(stat(&e, P1_CENSUSES), 1, "held, not logged");
    // The second equal reading lowers it.
    j.village_census(A, 3, 12, true);
    j.idle();
    assert_eq!((row(&e)[MEMBERS], row(&e)[SEEN_READING]), (3, 3));
    assert_eq!(stat(&e, P1_CENSUSES), 2);
    // An incomplete census never lowers it.
    j.village_census(A, 1, 12, false);
    j.idle();
    assert_eq!(row(&e)[MEMBERS], 3);
    // It may raise it (a part-loaded village counts low, never high).
    j.village_census(A, 5, 12, false);
    j.idle();
    assert_eq!(row(&e)[MEMBERS], 5);

    // Heartbeats of an unchanged village are throttled to one per 6,000 ticks.
    for _ in 0..2 {
        j.village_census(A, 5, 12, true);
        j.idle();
    }
    let base = stat(&e, P1_CENSUSES);
    for _ in 0..50 {
        j.game_time += 100;
        j.village_census(A, 5, 12, true);
        j.idle();
    }
    assert!(stat(&e, P1_CENSUSES) <= base + 1, "5,000 ticks of heartbeats: one at most");
    j.game_time += 2_000;
    j.village_census(A, 5, 12, true);
    j.idle();
    j.village_census(A, 5, 12, true);
    j.idle();
    assert!(stat(&e, P1_CENSUSES) > base, "after 6,000 ticks the heartbeat is logged");

    // The world after a restart equals the one that kept running.
    let before = row(&e);
    save_durable(&e, &mut j);
    let saved = j.world_save();
    let _ = j.crash();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = 0b1111;
    let (_, ok, _) = e.phase1_state();
    assert!(ok, "the replayed world is the saved one (its state hash is the save's)");
    j.idle();
    assert_eq!(row(&e)[MEMBERS], before[MEMBERS]);
    assert_eq!(row(&e)[SEEN_READING], before[SEEN_READING]);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// Nobody online (the host's [`Policy::Refuse`]): Java holds a house and a resident order, as the real
/// bridge holds one for chunks that are not loaded (`HouseBuilder` waits for its plot's chunks,
/// `ResidentSpawner` for its spawn chunk), and answers neither FAILED. An answer would reach the
/// engine at the next hand-over and lapse the booking, and the engine acts on both answers.
#[test]
fn nobody_online_holds_house_and_resident_orders() {
    let (_dir, e, mut j) = open("village-held");
    j.policy = Policy::Refuse;
    let before = j.journal.len();
    let mut house = vec![0u8; cmd::place_house::SIZE];
    wire::put_u64(&mut house, cmd::place_house::OP_ID, 0x1234);
    j.refuse(cmd::PLACE_HOUSE, 1, &house);
    let mut order = vec![0u8; cmd::spawn_resident::SIZE];
    wire::put_u64(&mut order, cmd::spawn_resident::OP_ID, 0x5678);
    j.refuse(cmd::SPAWN_RESIDENT, 2, &order);
    assert_eq!(j.journal.len(), before, "neither order is answered");
    assert!(j.refused.is_empty(), "nor counted as refused: {:?}", j.refused);
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn census_flapping_never_reaches_the_world() {
    let (_dir, e, mut j) = open("village-census-flap");
    j.village_census(A, 6, 12, true);
    j.idle();
    assert_eq!(row(&e)[MEMBERS], 6);
    let logged = stat(&e, P1_CENSUSES);
    // A villager across the reach's edge: the count flaps between readings and never reads the
    // same twice in a row except the world's own value.
    for i in 0..20 {
        let seen = if i % 2 == 0 { 5 } else { 6 };
        j.village_census(A, seen, 12, true);
        j.idle();
        assert_eq!(row(&e)[MEMBERS], 6, "reading {i}: {seen}");
    }
    // Readings that differ from each other and from the world's never agree either.
    for (i, seen) in [5, 4, 5, 3, 2, 4].into_iter().enumerate() {
        j.village_census(A, seen, 12, true);
        j.idle();
        assert_eq!(row(&e)[MEMBERS], 6, "reading {i}: {seen}");
    }
    assert_eq!(stat(&e, P1_CENSUSES), logged, "none of them reached the log");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn spawn_resident_flags_round_trip() {
    let (_dir, e, mut j) = open("village-settlers");
    // Seen alive, then wiped (two equal readings of none): the settlers' wait starts.
    j.village_census(A, 6, 12, true);
    j.idle();
    for _ in 0..2 {
        j.village_census(A, 0, 12, true);
        j.idle();
    }
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[SEEN]), (0, 1));
    assert!(r[WAIT_DUE] >= 0, "the wait is running: {r:?}");
    assert_eq!(stat(&e, P1_EMPTIED), 1);
    // 16 days later a pair comes: a SPAWN_RESIDENT each, at the village, an arrival, walking in.
    advance(&e, &mut j, 16 * 24_000 + 3_000);
    let sent = j.p1_of(cmd::SPAWN_RESIDENT);
    assert_eq!(sent.len(), 2, "a pair: {:?}", e.phase1_stats());
    use cmd::spawn_resident as k;
    let want = k::FLAG_AT_VILLAGE | k::FLAG_ARRIVAL | k::FLAG_WALK_IN;
    for s in &sent {
        assert_eq!(wire::u32_at(s, k::FLAGS), want);
        assert_eq!(wire::u64_at(s, k::VILLAGE), A);
        assert_eq!(j.cmd_name(wire::DICT_BODY, wire::u32_at(s, k::BODY)), "minecraft:villager");
        let (x, z) = (wire::f64_at(s, k::X), wire::f64_at(s, k::Z));
        assert!((x.hypot(z) - 192.0).abs() < 2.0, "192 blocks out: {x} {z}");
    }
    assert_eq!(stat(&e, P1_SETTLERS), 2);
    // Java placed both (the fake answers APPLIED): the bookings are settled.
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED]), (2, 0), "{r:?}");
    assert_eq!(stat(&e, P1_LAPSED), 0);

    // A village with no free bed: the arrival is refused NOT_FREE, the booking lapses and a new
    // wait runs.
    j.no_free_bed = true;
    for _ in 0..2 {
        j.village_census(A, 2, 12, true);
        j.idle();
    }
    for _ in 0..2 {
        j.village_census(A, 0, 12, true);
        j.idle();
    }
    assert_eq!(row(&e)[MEMBERS], 0);
    advance(&e, &mut j, 16 * 24_000 + 3_000);
    assert_eq!(j.p1_of(cmd::SPAWN_RESIDENT).len(), 4, "a second pair was ordered");
    assert_eq!(stat(&e, P1_LAPSED), 2, "and refused");
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED]), (0, 0), "{r:?}");
    assert!(r[WAIT_DUE] >= 0, "short again: a new wait runs");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// Three villages of a dozen with the plugin's census over the real boundary: the rig counts the
/// real residents of each, answers arrivals as the bridge does (one more resident each), and sends
/// every village's census twice per quantum (the second equal reading is the one that lowers).
struct Rig {
    dir: std::path::PathBuf,
    e: Arc<Engine>,
    j: Java,
    real: std::collections::BTreeMap<u64, u32>,
    /// The first day each village was ordered settlers (days after `t0`), and how many bodies.
    first: std::collections::BTreeMap<u64, f64>,
    sent: std::collections::BTreeMap<u64, u32>,
    seen: usize,
    t0: i64,
}

const B: u64 = A + 1;
const C: u64 = A + 2;
const DAY: i64 = 24_000;

impl Rig {
    fn census(&mut self) {
        for _ in 0..2 {
            for (&v, &n) in &self.real.clone() {
                self.j.village_census(v, n, 12, true);
                self.j.idle();
            }
        }
    }

    /// Runs until `days` after the wipe, a 3,000-tick quantum at a time.
    fn run_to(&mut self, days: f64) {
        while ((self.e.clocks()[2] - self.t0) as f64) < days * DAY as f64 {
            advance(&self.e, &mut self.j, 3_000);
            let day = (self.e.clocks()[2] - self.t0) as f64 / DAY as f64;
            let orders = self.j.p1_of(cmd::SPAWN_RESIDENT);
            for o in &orders[self.seen..] {
                let v = wire::u64_at(o, cmd::spawn_resident::VILLAGE);
                *self.sent.entry(v).or_default() += 1;
                self.first.entry(v).or_insert(day);
                *self.real.get_mut(&v).unwrap() += 1;
            }
            self.seen = orders.len();
            self.census();
        }
    }

    /// A crash before the next save: the engine comes back from the pages of the last save (the
    /// test's `save_durable`) and replays what Java's journal holds past them, as it does after a
    /// real crash. A census is latest state and is not in the journal, and the test's clock
    /// (`Phase1Advance`) is not either, so the world is back at the save's day and runs the days
    /// since again, booking what it booked before under the same op ids.
    fn crash(self) -> Rig {
        let Rig { dir, j, real, first, sent, t0, .. } = self;
        let saved = j.world_save();
        let _ = j.crash();
        let e = engine(&dir, 256 * 1024);
        let mut j = Java::open(e.clone(), saved);
        j.module_mask = 0b1111;
        j.idle();
        let (_, ok, _) = e.phase1_state();
        assert!(ok, "the replayed world is the one the journal leads to");
        Rig { dir, e, j, real, first, sent, seen: 0, t0 }
    }
}

/// A player wipes A, a siege wipes B, C is left alone. B's pair is ordered 16 days after the census
/// saw it empty, A's 32 (a full wipe at K 12 is pressure 1.00), C gets none, and a crash and
/// replay in the middle of the waits changes none of it.
#[test]
fn villages_renew_end_to_end() {
    let dir = tmpdir("villages-e2e");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = 0b1111;
    for (v, x) in [(A, 0), (B, 600), (C, 1_200)] {
        j.adopt(v, PLAINS, "human", 90, x, 0);
    }
    j.idle();
    let real = [(A, 12), (B, 12), (C, 12)].into_iter().collect();
    let mut rig = Rig {
        dir,
        e,
        j,
        real,
        first: Default::default(),
        sent: Default::default(),
        seen: 0,
        t0: 0,
    };
    rig.census();
    for v in [A, B, C] {
        assert_eq!(rig.e.phase1_village(v).unwrap()[MEMBERS], 12);
    }
    for _ in 0..12 {
        rig.j.resident_killed(A, true);
        rig.j.resident_killed(B, false);
    }
    rig.j.idle();
    rig.real.insert(A, 0);
    rig.real.insert(B, 0);
    rig.census();
    rig.t0 = rig.e.clocks()[2];
    for v in [A, B] {
        let r = rig.e.phase1_village(v).unwrap();
        assert_eq!((r[MEMBERS], r[WAIT_DUE] >= 0), (0, true), "village {v:x}: {r:?}");
    }
    let (pa, pb) = (
        rig.e.phase1_village(A).unwrap()[PRESSURE_MILLI],
        rig.e.phase1_village(B).unwrap()[PRESSURE_MILLI],
    );
    assert!((pa - 1_000).abs() <= 2, "a player's twelve kills at K 12: {pa}");
    assert_eq!(pb, 0, "a siege's add nothing");

    rig.run_to(5.0);
    save_durable(&rig.e, &mut rig.j);
    rig.run_to(10.0);
    assert!(rig.sent.is_empty(), "nothing before the shortest wait: {:?}", rig.sent);
    let clock = rig.e.clocks()[2];
    let mut rig = rig.crash();
    assert!(rig.e.clocks()[2] < clock - 2 * DAY, "rolled back to the save's day: {} of {clock}", rig.e.clocks()[2]);
    for v in [A, B] {
        let r = rig.e.phase1_village(v).unwrap();
        assert_eq!((r[MEMBERS], r[WAIT_DUE] >= 0), (0, true), "village {v:x} after the replay: {r:?}");
    }
    rig.run_to(15.0);
    assert!(rig.sent.is_empty(), "nothing at day 15: {:?}", rig.sent);
    rig.run_to(18.0);
    assert_eq!(rig.sent.get(&B), Some(&2), "B's pair, 16 days on: {:?}", rig.sent);
    assert!(!rig.sent.contains_key(&A), "A is still waiting: {:?}", rig.sent);
    let b_day = rig.first[&B];
    assert!((15.9..17.0).contains(&b_day), "B's pair came on day {b_day}");
    rig.run_to(31.0);
    assert!(!rig.sent.contains_key(&A), "A waits 32 days: {:?}", rig.sent);
    rig.run_to(34.0);
    assert_eq!(rig.sent.get(&A), Some(&2), "A's pair at 32: {:?}", rig.sent);
    let a_day = rig.first[&A];
    assert!((31.9..33.5).contains(&a_day), "A's pair came on day {a_day}");
    assert!(!rig.sent.contains_key(&C), "C was never short");
    for (v, n) in [(A, 2), (B, 2), (C, 12)] {
        let r = rig.e.phase1_village(v).unwrap();
        assert_eq!(r[MEMBERS], n, "village {v:x}: {r:?}");
        assert_eq!(r[OWED], 0);
    }
    assert!(rig.e.protocol_error().is_none(), "{:?}", rig.e.protocol_error());
    rig.e.close(true, Duration::from_secs(5)).unwrap();
}

/// A pair ordered and never answered (Java down between placing the bodies and saying so), then a
/// crash before the engine's next save: the engine is rolled back to its pages, reads the wiped
/// village again and books the same pair under the same op ids, so the answers Java gives when it
/// is back settle them, once; an answer for an op the world no longer holds is no answer.
#[test]
fn a_settlers_order_survives_a_crash_between_its_order_and_its_result() {
    let (dir, e, mut j) = open("village-settlers-crash");
    j.hold_arrivals = true;
    j.village_census(A, 6, 12, true);
    j.idle();
    // The pages hold the village seen alive, and no more.
    save_durable(&e, &mut j);
    for _ in 0..2 {
        j.village_census(A, 0, 12, true);
        j.idle();
    }
    advance(&e, &mut j, 16 * 24_000 + 3_000);
    use cmd::spawn_resident as k;
    let ops: Vec<u64> = j.p1_of(cmd::SPAWN_RESIDENT).iter().map(|s| wire::u64_at(s, k::OP_ID)).collect();
    assert_eq!(ops.len(), 2, "a pair was ordered: {:?}", e.phase1_stats());
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED]), (2, 2), "booked, not answered: {r:?}");
    // The crash comes before the next save: the engine is back at the pages (the wiped censuses, the
    // order and the booking are lost with them: a census is latest state, never in the journal) and
    // Java's registry is gone with it. Java reads the empty village again, as its census does every
    // 100 ticks, and the order comes again.
    let saved = j.world_save();
    let _ = j.crash();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = 0b1111;
    j.hold_arrivals = true;
    j.idle();
    let (_, ok, _) = e.phase1_state();
    assert!(ok, "the replay led to the state the journal does");
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED], r[SEEN]), (6, 0, 1), "rolled back to the pages: {r:?}");
    assert!(r[WAIT_DUE] < 0, "no wait runs at the pages: {r:?}");
    for _ in 0..2 {
        j.village_census(A, 0, 12, true);
        j.idle();
    }
    assert!(row(&e)[WAIT_DUE] >= 0, "the wiped village is read again and the wait runs again: {:?}", row(&e));
    advance(&e, &mut j, 16 * 24_000 + 3_000);
    let again: Vec<u64> = j.p1_of(cmd::SPAWN_RESIDENT).iter().map(|s| wire::u64_at(s, k::OP_ID)).collect();
    assert_eq!(again, ops, "a rolled back engine books the same pair under the same op ids");
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED]), (2, 2), "booked again, not answered: {r:?}");
    // Java is back and answers what its registry held: both placed, one of them told twice.
    let answer = |j: &mut Java, op: u64, status: u32| {
        j.journal(obs::COMMAND_RESULT, obs::command_result::SIZE, status, |r| {
            wire::put_u64(r, obs::command_result::OP_ID, op);
        });
        j.idle();
    };
    answer(&mut j, ops[0], obs::command_result::APPLIED);
    assert_eq!(row(&e)[OWED], 1);
    answer(&mut j, ops[0], obs::command_result::DUPLICATE_OP);
    assert_eq!(row(&e)[OWED], 1, "an op is answered once");
    answer(&mut j, ops[1], obs::command_result::APPLIED);
    let r = row(&e);
    assert_eq!((r[MEMBERS], r[OWED]), (2, 0), "settled: {r:?}");
    assert_eq!(stat(&e, P1_LAPSED), 0);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}
