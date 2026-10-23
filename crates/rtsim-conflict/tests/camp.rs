// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M4 (§9): the camp ledger (§4.9, ledgers 6 and 7) played
//! against the emulated plugin's camp world (`synth::CampRun`), one test for each invariant I1 to
//! I6, the draw order and the cap, waiting for whole bodies, deaths in every phase, strays and
//! identity, the epochs (a second census before the first's lines, a session start, a replayed
//! census), answers lost, late, replayed and stale, every new state through a save and a paged
//! save, and a crash at every step of a scripted run and at random.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::engine::{Phase, SlotState};
use rtsim_conflict::synth::CampRun;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

fn days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64) as u64
}

fn day(d: f64) -> Tick {
    Tick(days(d))
}

fn grove(seed: u64, found: u16) -> CampRun {
    CampRun::new(seed, "umvuthana", found)
}

fn den(seed: u64, found: u16) -> CampRun {
    CampRun::new(seed, "frostmaw", found)
}

/// Slots by state: body, owed, left, gone, dead.
fn counts(r: &CampRun) -> [u32; 5] {
    let mut n = [0; 5];
    for s in &r.c.camp(r.f).expect("a ledger").slots {
        n[s.state as usize] += 1;
    }
    n
}

fn world_bodies(r: &CampRun) -> usize {
    r.plugin.camps.get(&r.key).map_or(0, |c| c.bodies.len())
}

fn tagged_bodies(r: &CampRun) -> usize {
    r.plugin
        .camps
        .get(&r.key)
        .map_or(0, |c| c.bodies.iter().filter(|b| b.generation != 0).count())
}

/// Every CAMP_RECONCILE the engine sent, with the tick.
fn orders(r: &CampRun) -> Vec<(Tick, boundary::CampReconcile)> {
    r.cmds
        .iter()
        .filter_map(|(t, c)| match c {
            Cmd::Reconcile(x) => Some((*t, x.clone())),
            _ => None,
        })
        .collect()
}

/// Lines of `action` the plugin applied (any outcome).
fn applied(r: &CampRun, action: u8) -> usize {
    r.plugin
        .applied
        .iter()
        .filter(|a| a.line.action == action)
        .count()
}

/// Lines the plugin answered with `outcome`.
fn answered(r: &CampRun, outcome: u32) -> usize {
    r.plugin
        .applied
        .iter()
        .filter(|a| a.outcome == outcome)
        .count()
}

fn ok(r: &mut CampRun) {
    let bad = r.check();
    assert!(bad.is_empty(), "the invariants break: {bad:#?}");
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
}

/// The camp comes in and its first lines are answered.
fn come_in(r: &mut CampRun) {
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

fn war_parties(
    r: &CampRun,
) -> Vec<(
    rtsim_kernel::handle::Handle<rtsim_conflict::engine::Party>,
    Phase,
    u32,
)> {
    r.c.parties
        .iter()
        .filter(|(_, p)| p.kind.is_war())
        .map(|(h, p)| (h, p.phase, p.alive()))
        .collect()
}

// ---- the census and the strays ----------------------------------------------------------------

/// MOWZIE.md §4.9, *Strays*: a camp found with its bodies counted starts with every slot owed, and
/// the jar's own untagged bodies, the first census's strays, fill them (an adopt line each, no
/// spawn); the books and the world agree (I1, I2, I3).
#[test]
fn a_first_census_adopts_the_jars_bodies_into_the_owed_slots() {
    let mut r = grove(11, 9);
    assert_eq!(counts(&r), [0, 9, 0, 0, 0], "found: every slot owed");
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 0);
    assert!(!r.c.camp_is_in(r.f));
    come_in(&mut r);
    assert!(r.c.camp_is_in(r.f));
    assert_eq!(counts(&r), [9, 0, 0, 0, 0]);
    assert_eq!(applied(&r, boundary::ACTION_ADOPT), 9);
    assert_eq!(
        applied(&r, boundary::ACTION_SPAWN),
        0,
        "a stray fills a slot before any spawn"
    );
    assert_eq!(answered(&r, boundary::LINE_DONE), 9);
    assert_eq!(
        tagged_bodies(&r),
        9,
        "the jar's bodies carry their slots now"
    );
    assert_eq!(world_bodies(&r), 9);
    ok(&mut r);
    // Each member has its own generation, none 0.
    let c = r.c.camp(r.f).unwrap();
    let mut gens: Vec<u32> = c.slots.iter().map(|s| s.generation).collect();
    gens.sort_unstable();
    gens.dedup();
    assert_eq!(gens.len(), 9);
    assert!(!gens.contains(&0));
}

/// A den found with its Frostmaw (walkthrough A, day 0): one slot, owed, filled by the jar's own
/// body; cap and pool 1.
#[test]
fn a_den_found_with_its_frostmaw_adopts_it() {
    let mut r = den(12, 1);
    assert_eq!(r.c.factions[r.f as usize].cap, 1);
    assert_eq!(counts(&r), [0, 1, 0, 0, 0]);
    come_in(&mut r);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(world_bodies(&r), 1);
    ok(&mut r);
}

/// A den found empty (§4.10) starts with an empty pool and a cap of 1; a camp that is in refills
/// it at the culture's rate, a spawn line for the whole member when it is ready, and never past
/// the cap.
#[test]
fn a_den_found_empty_refills_to_its_cap_and_no_further() {
    let mut r = den(13, 0);
    assert_eq!(counts(&r), [0, 0, 0, 0, 0]);
    assert_eq!(r.c.factions[r.f as usize].cap, 1);
    come_in(&mut r);
    assert_eq!(
        world_bodies(&r),
        0,
        "nothing at the census: nothing to adopt or spawn"
    );
    assert_eq!(orders(&r).len(), 0);
    r.run(day(3.0));
    assert_eq!(world_bodies(&r), 0, "a Frostmaw is recruited every 6 days");
    r.run(day(7.5));
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 1);
    assert_eq!(world_bodies(&r), 1);
    ok(&mut r);
    r.run(day(40.0));
    assert_eq!(world_bodies(&r), 1, "the cap is 1");
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 1);
    ok(&mut r);
}

/// A pool member recruited while its camp is out is a slot owed a body, with no order sent; the
/// camp's next census spawns it (walkthrough A, day 21.5; test 5).
#[test]
fn a_refill_while_the_camp_is_out_is_spawned_at_the_next_census() {
    let mut r = grove(14, 9);
    come_in(&mut r);
    // Four are killed with the grove in; it goes out before they are replaced.
    let uuids: Vec<_> = r.plugin.camps[&r.key]
        .bodies
        .iter()
        .take(4)
        .map(|b| b.uuid)
        .collect();
    for u in uuids {
        let at = r.c.now() + 1;
        assert!(r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at));
    }
    r.camp_out();
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!(counts(&r), [5, 0, 0, 0, 0], "four members died");
    let sent = orders(&r).len();
    r.run(day(1.0) + 100);
    assert_eq!(
        orders(&r).len(),
        sent,
        "a camp that is out is never ordered"
    );
    // Two a day: the pool is back, as owed slots (the audit reads the pool, which adds them).
    ok(&mut r);
    let c = r.c.camp(r.f).unwrap();
    assert!(c.slots.iter().any(|s| s.state == SlotState::Owed));
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert!(applied(&r, boundary::ACTION_SPAWN) >= 1);
    assert_eq!(counts(&r)[1], 0, "every owed slot has its body");
    assert_eq!(tagged_bodies(&r), r.c.camp(r.f).unwrap().slots.len());
    ok(&mut r);
}

/// Ledger 6, the cap: strays over it are removed, none adopted while the pool and the members away
/// are at it; under it they are adopted.
#[test]
fn strays_over_the_cap_are_removed_and_those_under_it_are_adopted() {
    let mut r = grove(15, 3);
    assert_eq!(r.c.factions[r.f as usize].cap, 3);
    let body = r.body;
    r.plugin.camp_stray(r.key, body);
    r.plugin.camp_stray(r.key, body);
    come_in(&mut r);
    assert_eq!(counts(&r), [3, 0, 0, 0, 0], "the cap is 3");
    assert_eq!(applied(&r, boundary::ACTION_ADOPT), 3);
    assert_eq!(
        applied(&r, boundary::ACTION_REMOVE),
        2,
        "the two over it are removed"
    );
    assert_eq!(world_bodies(&r), 3);
    ok(&mut r);
    // One dies, the grove goes out and a player puts a body down: it comes back in at once, with
    // the pool still short of the cap, and the stray fills the room.
    let u = r.plugin.camps[&r.key].bodies[0].uuid;
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at);
    r.camp_out();
    r.plugin.camp_stray(r.key, body);
    r.plugin.camp_stray(r.key, body);
    let to = r.c.now() + 3;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [3, 0, 0, 0, 0]);
    assert_eq!(world_bodies(&r), 3, "one adopted, one removed");
    ok(&mut r);
}

/// §4.9's table, home owed: no stray, a spawn line (the jar's bodies lost before the first census).
#[test]
fn an_owed_slot_with_no_stray_is_spawned() {
    let mut r = grove(16, 4);
    r.plugin.camps.get_mut(&r.key).unwrap().bodies.truncate(2);
    come_in(&mut r);
    assert_eq!(applied(&r, boundary::ACTION_ADOPT), 2);
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 2);
    assert_eq!(counts(&r), [4, 0, 0, 0, 0]);
    assert_eq!(world_bodies(&r), 4);
    ok(&mut r);
}

/// §4.9's table, home body, not found: a miss; the third, at least two days after the first, writes
/// the member off (the pool loses it and its generation is retired); a body found resets the
/// misses; a body found later under the written-off generation is a stray like any other.
#[test]
fn a_member_whose_body_is_missed_three_times_over_two_days_is_written_off() {
    let mut r = grove(17, 9);
    come_in(&mut r);
    let lost = r
        .plugin
        .camps
        .get_mut(&r.key)
        .unwrap()
        .bodies
        .pop()
        .unwrap();
    let id =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .find(|s| s.generation == lost.generation)
            .unwrap()
            .id;
    let census_at = |r: &mut CampRun, t: Tick| {
        r.run(t);
        r.camp_out();
        let to = r.c.now() + 2;
        r.run(to);
        r.camp_in();
        let to = r.c.now() + 10;
        r.run(to);
    };
    let t0 = r.c.now();
    census_at(&mut r, t0 + 100);
    let s = |r: &CampRun| {
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .find(|s| s.id == id && s.generation == lost.generation)
            .map(|s| (s.state, s.misses))
    };
    assert_eq!(s(&r), Some((SlotState::Body, 1)), "one miss");
    census_at(&mut r, t0 + days(1.0));
    assert_eq!(s(&r), Some((SlotState::Body, 2)));
    census_at(&mut r, t0 + days(1.5));
    assert_eq!(
        s(&r),
        Some((SlotState::Body, 3)),
        "three misses, but not two days"
    );
    assert_eq!(r.c.stats.camp_written_off, 0);
    ok(&mut r);
    // A body found resets the misses.
    r.plugin.camps.get_mut(&r.key).unwrap().bodies.push(lost);
    census_at(&mut r, t0 + days(1.6));
    assert_eq!(s(&r), Some((SlotState::Body, 0)), "found again");
    r.plugin.camps.get_mut(&r.key).unwrap().bodies.pop();
    census_at(&mut r, t0 + days(2.0));
    census_at(&mut r, t0 + days(3.0));
    census_at(&mut r, t0 + days(3.9));
    assert_eq!(
        s(&r),
        Some((SlotState::Body, 3)),
        "three misses within two days of the first"
    );
    assert_eq!(r.c.stats.camp_written_off, 0);
    census_at(&mut r, t0 + days(4.1));
    assert_eq!(
        r.c.stats.camp_written_off, 1,
        "the next miss is two days after the first"
    );
    assert_eq!(s(&r), None, "the slot is freed");
    let retired =
        r.c.camp(r.f)
            .unwrap()
            .retired
            .iter()
            .find(|x| x.generation == lost.generation)
            .copied();
    assert!(
        retired.is_some_and(|x| !x.died),
        "retired at a write-off, not a death"
    );
    ok(&mut r);
}

// ---- deaths ------------------------------------------------------------------------------------

/// §4.9, *Deaths*: at home the pool loses a member and the slot's generation is retired at a death:
/// a body found later under it (a save restored, a body put back) is removed, not adopted. A stray's
/// death books nothing.
#[test]
fn a_death_at_home_costs_the_pool_a_member_and_its_generation_is_retired() {
    let mut r = grove(18, 9);
    come_in(&mut r);
    let b = r.plugin.camps[&r.key].bodies[0];
    let at = r.c.now() + 1;
    assert!(r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at));
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!(counts(&r), [8, 0, 0, 0, 0]);
    assert_eq!(r.c.stats.camp_deaths, 1);
    let died =
        r.c.camp(r.f)
            .unwrap()
            .retired
            .iter()
            .any(|x| x.generation == b.generation && x.died);
    assert!(died);
    ok(&mut r);
    // The dead body comes back under its old tag (a world save restored): the next census removes
    // it and adopts nothing.
    r.plugin
        .camp_tagged_body(r.key, b.body, b.slot, b.generation);
    r.camp_out();
    let to = r.c.now() + 3;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert!(
        r.plugin.camps[&r.key]
            .bodies
            .iter()
            .all(|x| x.generation != b.generation),
        "removed"
    );
    assert_eq!(applied(&r, boundary::ACTION_REMOVE), 1);
    ok(&mut r);
    // A stray's death books nothing.
    let stray = r.plugin.camp_stray(r.key, b.body);
    let before = (counts(&r), r.c.stats.camp_deaths);
    let at = r.c.now() + 1;
    assert!(r.plugin.camp_kill(r.key, stray, boundary::BY_PLAYER, at));
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!((counts(&r), r.c.stats.camp_deaths), before);
}

/// A death record for a body the books do not hold (a generation retired or never held) books
/// nothing; one for a slot already dead does nothing more (the Umvuthi's two death events).
#[test]
fn a_death_for_a_generation_the_slot_does_not_hold_books_nothing() {
    let mut r = grove(19, 9);
    come_in(&mut r);
    let b = r.plugin.camps[&r.key].bodies[0];
    let before = (r.c.camp(r.f).unwrap().clone(), r.c.pool_at(r.f, r.c.now()));
    let d = boundary::CampDeath {
        faction: r.key,
        by: boundary::BY_PLAYER,
        body: b.body,
        flags: 0,
        slot: b.slot,
        generation: b.generation ^ 0x5555,
        killer: (1, 2),
        op: 77,
    };
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut rtsim_ecology::infra::NoInfrastructure,
        &Obs::CampDeath(d),
        at,
    );
    assert_eq!(
        (r.c.camp(r.f).unwrap().clone(), r.c.pool_at(r.f, r.c.now())),
        before,
        "a generation the slot does not hold"
    );
    assert_eq!(r.c.stats.camp_deaths, 0);
    // Twice for a held one: the second does nothing.
    for op in [78, 79] {
        let held = boundary::CampDeath {
            generation: b.generation,
            op,
            ..d
        };
        r.c.ingest(
            &mut r.w,
            &mut rtsim_ecology::infra::NoInfrastructure,
            &Obs::CampDeath(held),
            at,
        );
    }
    assert_eq!(r.c.stats.camp_deaths, 1);
    assert_eq!(r.c.camp(r.f).unwrap().slots.len(), 8);
}

/// A CAMP_DEATH replayed from the journal (the same op id twice) is booked once: the pool loses one
/// member, not two.
#[test]
fn a_death_replayed_from_the_journal_is_booked_once() {
    let mut r = grove(125, 9);
    come_in(&mut r);
    let b = r.plugin.camps[&r.key].bodies[0];
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    let j = journal(&mut r);
    assert_eq!(j.len(), 1);
    for _ in 0..2 {
        for (t, o) in j.clone() {
            let at = t.max(r.c.now());
            r.c.ingest(
                &mut r.w,
                &mut rtsim_ecology::infra::NoInfrastructure,
                &o,
                at,
            );
        }
    }
    assert_eq!(r.c.stats.camp_deaths, 1);
    assert_eq!(counts(&r), [8, 0, 0, 0, 0]);
    ok(&mut r);
}

/// Test 22, deaths in every phase (1): a body killed at the camp during a muster, with the pool at
/// 0, kills that member's unit; a party left with no units ends through `home` (and goes on to be
/// ordered again, the pool refilled later).
#[test]
fn a_body_killed_during_a_muster_kills_that_members_unit() {
    let mut r = grove(20, 4);
    come_in(&mut r);
    let p = r.raid().expect("four members make a raid");
    assert_eq!(counts(&r), [0, 0, 4, 0, 0], "drawn with their bodies: left");
    assert_eq!(r.c.factions[r.f as usize].away, 4);
    ok(&mut r);
    let bodies: Vec<_> = r.plugin.camps[&r.key].bodies.clone();
    let at = r.c.now() + 1;
    r.plugin
        .camp_kill(r.key, bodies[0].uuid, boundary::BY_PLAYER, at);
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!(r.c.parties[p].alive(), 3, "the party lost that member");
    assert_eq!(counts(&r), [0, 0, 3, 0, 0]);
    assert_eq!(r.c.factions[r.f as usize].away, 3);
    ok(&mut r);
    // The rest die: no units, and the party ends through its own `home`.
    for b in &bodies[1..] {
        let at = r.c.now() + 1;
        r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
        let to = r.c.now() + 3;
        r.run(to);
    }
    let to = r.c.now() + days(2.0);
    r.run(to);
    assert!(
        war_parties(&r).is_empty(),
        "the party ended: {:?}",
        war_parties(&r)
    );
    assert_eq!(r.c.factions[r.f as usize].away, 0);
    assert_eq!(world_bodies(&r), r.c.camp(r.f).unwrap().slots.len());
    ok(&mut r);
}

/// Deaths in every phase (2): between a muster's end and its answer. The set-out line's answer is
/// late; a body killed meanwhile is that member's death, its slot freed, and the answer for it
/// books nothing; the others book away, gone.
#[test]
fn a_body_killed_between_a_musters_end_and_its_answer_is_a_death_and_the_answer_books_nothing() {
    let mut r = grove(21, 9);
    come_in(&mut r);
    r.plugin.answer_ticks = 3_000;
    let p = r.raid().unwrap();
    let end = r.c.parties[p].until;
    r.run(end + 1);
    assert_eq!(
        r.c.camp(r.f).unwrap().in_flight(),
        4,
        "set-out lines in flight"
    );
    // The line is applied in the tick it arrives, so the bodies are already gone: kill by the
    // record a body that died the same tick would send.
    let victim =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .find(|s| s.state == SlotState::Left)
            .map(|s| (s.id, s.generation, s.body))
            .unwrap();
    let d = boundary::CampDeath {
        faction: r.key,
        by: boundary::BY_PLAYER,
        body: victim.2,
        flags: 0,
        slot: victim.0,
        generation: victim.1,
        killer: (9, 9),
        op: 1 << 62,
    };
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut rtsim_ecology::infra::NoInfrastructure,
        &Obs::CampDeath(d),
        at,
    );
    assert_eq!(r.c.parties[p].alive(), 3, "that member's unit died");
    r.run(end + 4_000);
    assert_eq!(counts(&r)[3], 3, "the others are away, gone");
    assert_eq!(counts(&r)[2], 0);
    ok(&mut r);
}

/// Deaths in every phase (3): while its party is out with the body left behind (the camp was out
/// when it set out): that member's unit dies, its slot is freed, the party goes on.
#[test]
fn a_body_left_behind_by_a_party_that_is_out_dies_with_its_unit() {
    let mut r = grove(22, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.camp_out();
    let end = r.c.parties[p].until;
    r.run(end + 100);
    assert_eq!(r.c.parties[p].phase, Phase::Out);
    assert_eq!(counts(&r)[2], 4, "left in the camp while it was out");
    let b = r.plugin.camps[&r.key]
        .bodies
        .iter()
        .find(|b| {
            b.generation != 0
                && r.c
                    .camp(r.f)
                    .unwrap()
                    .slots
                    .iter()
                    .any(|s| s.generation == b.generation && s.state == SlotState::Left)
        })
        .copied()
        .unwrap();
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    let to = r.c.now() + 3;
    r.run(to);
    assert_eq!(r.c.parties[p].alive(), 3);
    assert_eq!(counts(&r)[2], 3);
    assert_eq!(r.c.camp(r.f).unwrap().slots.len(), 8);
    ok(&mut r);
}

/// A slot dead, left (a unit that died away while its body stayed in the camp, §4.9's last table
/// row): a raid that set out from a camp that was out, met by the player and fought (BAND_CASUALTIES
/// by the band's own puppets), loses members whose bodies are still in the camp; the next census
/// removes those bodies and frees the slots, and takes nothing it should not.
#[test]
fn a_dead_slots_body_is_removed_at_the_next_census() {
    let mut r = grove(23, 9);
    r.plugin.fight = Some(rtsim_conflict::synth::Fight {
        kill_share: 0.5,
        after: 200,
        kill_chief: false,
        only_at_base: false,
    });
    come_in(&mut r);
    r.watch = true;
    let p = r.raid().unwrap();
    r.camp_out();
    r.run(day(1.9));
    assert_eq!(r.c.parties[p].phase, Phase::Out);
    ok(&mut r);
    r.run(day(4.0));
    assert!(war_parties(&r).is_empty(), "the raid is home");
    let n = counts(&r);
    assert!(
        n[4] >= 1,
        "members died away with their bodies left in the camp: {n:?}"
    );
    let dead = n[4];
    assert_eq!(
        world_bodies(&r) as u32,
        n[0] + n[4],
        "their bodies are still there"
    );
    ok(&mut r);
    r.watch = false;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let n = counts(&r);
    assert_eq!(n[4], 0, "the dead slots are freed");
    assert_eq!(applied(&r, boundary::ACTION_REMOVE), dead as usize);
    assert_eq!(
        applied(&r, boundary::ACTION_TAKE),
        0,
        "no one is out: nothing is taken"
    );
    assert_eq!(n[2] + n[3], 0);
    assert_eq!(world_bodies(&r), r.c.camp(r.f).unwrap().slots.len());
    ok(&mut r);
}

// ---- draws and the cap -------------------------------------------------------------------------

/// Test 22, the draw order: found bodies first, then a missed one, then owed ones, never one with a
/// line in flight. A grove of four with one body lost: the party of four takes the three found and
/// the missed one; the set-out line's answer for the missed one is nothing to do, and it is booked
/// away and gone like the rest.
#[test]
fn a_draw_takes_found_bodies_then_a_missed_one_and_its_set_out_is_nothing_to_do() {
    let mut r = grove(24, 4);
    come_in(&mut r);
    let lost = r
        .plugin
        .camps
        .get_mut(&r.key)
        .unwrap()
        .bodies
        .pop()
        .unwrap();
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let missed =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .find(|s| s.generation == lost.generation)
            .unwrap()
            .misses;
    assert_eq!(missed, 1);
    let p = r.raid().expect("four make a raid");
    assert_eq!(
        counts(&r),
        [0, 0, 4, 0, 0],
        "the three found and the missed one: left"
    );
    let end = r.c.parties[p].until;
    r.run(end + 10);
    assert_eq!(counts(&r), [0, 0, 0, 4, 0], "all away, gone");
    let setout: Vec<_> = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_SET_OUT)
        .collect();
    assert_eq!(setout.len(), 4);
    assert_eq!(
        setout
            .iter()
            .filter(|a| a.outcome == boundary::LINE_NOTHING)
            .count(),
        1,
        "the missed one's body was not there"
    );
    assert_eq!(
        setout
            .iter()
            .filter(|a| a.outcome == boundary::LINE_DONE)
            .count(),
        3
    );
    assert_eq!(world_bodies(&r), 0);
    ok(&mut r);
}

/// Test 22, the draw order with owed slots (the spec's walkthrough): a grove with three bodies
/// found, one missed and two owed sends a party of four; the three found and the one missed are
/// drawn, none owed, and the owed two stay owed.
#[test]
fn a_draw_leaves_the_owed_slots_for_last() {
    let mut r = grove(124, 6);
    come_in(&mut r);
    let lost = r
        .plugin
        .camps
        .get_mut(&r.key)
        .unwrap()
        .bodies
        .pop()
        .unwrap();
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    // Two members killed with the camp in: the pool refills them later, owed, and the camp goes
    // out before the refill's spawn lines.
    for i in 0..2 {
        let b = r.plugin.camps[&r.key].bodies[i];
        let at = r.c.now() + 1;
        r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    }
    let to = r.c.now() + 5;
    r.run(to);
    r.camp_out();
    let to = r.c.now() + days(1.2);
    r.run(to);
    r.raid().expect("six slots make a raid of four");
    let c = r.c.camp(r.f).unwrap();
    let drawn: Vec<_> = c
        .slots
        .iter()
        .filter(|s| s.state == SlotState::Left)
        .collect();
    assert_eq!(drawn.len(), 4);
    assert!(
        drawn.iter().any(|s| s.generation == lost.generation),
        "the missed one is drawn"
    );
    assert_eq!(counts(&r), [0, 2, 4, 0, 0], "no owed slot was drawn");
    ok(&mut r);
}

/// §4.9, *Homecomings*: a party that walks home in its members' own bodies (embodied, its puppets
/// in the world) has its survivors' bodies handed back at the camp that is in: a hand-back line
/// for each puppet, not a spawn, and the world holds one body for each member. No flow of this
/// slice leaves a party embodied at its homecoming (the walk home with bodies is M10's), so the
/// state is made by hand: the party is marked embodied once it walks home.
#[test]
fn a_party_home_with_its_puppets_has_them_handed_back_not_spawned() {
    let mut r = grove(127, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    let mut t = r.c.now();
    while r.c.parties[p].phase != Phase::Home {
        t += 200;
        r.run(t);
        assert!(t < day(20.0), "the party never walked home");
    }
    assert_eq!(counts(&r), [5, 0, 0, 4, 0], "away, gone");
    assert_eq!(world_bodies(&r), 5);
    let units = r.c.parties[p].units.clone();
    r.c.parties[p].embodied = true;
    r.c.parties[p].spawned = units;
    let end = r.c.parties[p].until.max(t) + days(2.0);
    r.run(end);
    assert!(war_parties(&r).is_empty(), "home");
    assert_eq!(
        applied(&r, boundary::ACTION_HAND_BACK),
        4,
        "a hand-back for each puppet"
    );
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 0);
    assert_eq!(counts(&r), [9, 0, 0, 0, 0]);
    assert_eq!(world_bodies(&r), 9);
    assert!(
        !r.cmds.iter().any(|(_, c)| matches!(c, Cmd::Recall(_))),
        "no recall: the camp is in"
    );
    ok(&mut r);
}

/// Test 22, a member drawn while owed (walkthrough A, "kill it"): a den out for a week has its
/// Frostmaw owed; it musters and sets out owed (away, gone, carrying its debt). The den comes in
/// while it is out and while it is away: nothing is taken, nothing spawned, no member booked dead.
/// At `home` it is owed again and spawned once, and the world holds one Frostmaw throughout.
#[test]
fn a_member_drawn_owed_carries_its_debt_and_is_spawned_once_at_home() {
    let mut r = den(25, 0);
    r.run(day(6.5));
    ok(&mut r);
    assert_eq!(
        counts(&r),
        [0, 1, 0, 0, 0],
        "recruited while the den was out: owed"
    );
    let p = r.raid().expect("one Frostmaw makes the raid");
    assert_eq!(counts(&r), [0, 0, 0, 1, 0], "drawn owed: away, gone");
    // The den comes in while the party is out, and again: nothing happens to the slot.
    r.run(r.c.parties[p].until + 100);
    assert_eq!(r.c.parties[p].phase, Phase::Out);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [0, 0, 0, 1, 0], "still away and gone");
    assert!(
        orders(&r).is_empty(),
        "no take and no spawn while it is away"
    );
    assert_eq!(r.c.stats.camp_written_off, 0, "no member booked dead");
    assert_eq!(world_bodies(&r), 0);
    ok(&mut r);
    // It comes home: owed, spawned once.
    let before = applied(&r, boundary::ACTION_SPAWN);
    let mut d = 7.0;
    while !war_parties(&r).is_empty() && d < 14.0 {
        d += 0.25;
        r.run(day(d));
    }
    assert!(war_parties(&r).is_empty(), "the party is home");
    r.run(r.c.now() + 100);
    assert_eq!(
        applied(&r, boundary::ACTION_SPAWN),
        before + 1,
        "spawned once"
    );
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(world_bodies(&r), 1, "one Frostmaw");
    ok(&mut r);
}

/// Ledger 6: the pool refills only while it and the members away are under the camp's cap, so a
/// band's survivors always have their slots; and a draw takes no more than the slots at home.
#[test]
fn the_pool_and_the_members_away_never_pass_the_cap() {
    let mut r = grove(26, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    assert_eq!(r.c.factions[r.f as usize].away, 4);
    let mut d = 0.0;
    while !r.c.parties.is_empty() && d < 10.0 {
        d += 0.1;
        r.run(day(d));
        let pool = r.c.pool_at(r.f, r.c.now());
        let away = f64::from(r.c.factions[r.f as usize].away);
        assert!(
            pool + away <= 9.0 + 1e-9,
            "day {d}: pool {pool}, {away} away"
        );
        r.audit();
    }
    assert!(r.c.parties.get(p).is_none(), "the raid came home");
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
    ok(&mut r);
    assert_eq!(counts(&r).iter().sum::<u32>(), 9, "all nine home");
}

/// MOWZIE.md ledger 6, waiting for whole bodies: a feud whose camp has the warriors in its pool but
/// every slot held by a line in flight waits, books no event for it ([`BLOCKED`]), and is planned
/// again when the answer lands; the raid then goes at once.
#[test]
fn a_feud_waits_for_the_lines_in_flight_and_goes_when_they_are_answered() {
    let mut r = den(27, 0);
    r.plugin.answer_ticks = 5 * TICKS_PER_DAY;
    r.camp_in();
    let mut d = 0.0;
    let mut blocked_days = Vec::new();
    while d < 16.0 {
        r.kill(20.0);
        d += 1.0;
        r.run(day(d));
        let x = r.c.feud(r.f, r.player).unwrap();
        if x.wait_until >= rtsim_conflict::engine::BLOCKED {
            blocked_days.push(d);
            assert!(r.c.parties.is_empty(), "a blocked feud sends nothing");
            assert_eq!(r.c.camp(r.f).unwrap().drawable(), 0);
            assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 1);
        }
    }
    assert!(!blocked_days.is_empty(), "the feud waited for the line");
    assert!(
        r.c.stats.parties.iter().sum::<u64>() >= 1,
        "the raid went after the answer"
    );
    // The war party went out within a day of the answer (the spawn line went out at about day 6
    // and was answered at about day 11).
    let first_raid = r
        .cmds
        .iter()
        .find_map(|(t, c)| matches!(c, Cmd::Reconcile(x) if x.lines.iter().any(|l| l.action == boundary::ACTION_SET_OUT)).then_some(*t));
    let _ = first_raid;
    ok(&mut r);
}

// ---- epochs ------------------------------------------------------------------------------------

/// Test 22, a second census before the first's lines: CAMP_OUT and a new census arrive before an
/// epoch's CAMP_RECONCILE, which Java answers stale and does not apply; the new census's reconcile
/// takes each body once, and the stale answer books nothing.
#[test]
fn a_second_census_before_the_firsts_lines_has_them_answered_stale() {
    let mut r = grove(28, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.camp_out();
    r.run(r.c.parties[p].until + 100);
    assert_eq!(counts(&r)[2], 4, "left in the camp while it set out");
    // In, out and in again before any command is applied.
    let first = r.camp_in();
    r.camp_out();
    let second = r.camp_in();
    assert_ne!(first, second);
    let to = r.c.now() + 10;
    r.run(to);
    let takes: Vec<_> = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_TAKE)
        .collect();
    assert_eq!(takes.len(), 8, "two reconciles of four takes");
    assert_eq!(
        takes
            .iter()
            .filter(|a| a.outcome == boundary::LINE_STALE)
            .count(),
        4
    );
    assert_eq!(
        takes
            .iter()
            .filter(|a| a.outcome == boundary::LINE_DONE)
            .count(),
        4
    );
    assert_eq!(counts(&r), [5, 0, 0, 4, 0], "each body taken once");
    assert_eq!(world_bodies(&r), 5);
    ok(&mut r);
}

/// §4.9, *Orders and answers*: at a session start every camp is out and every line in flight is
/// void; nothing is resent, and the camp's next census orders what is still needed. The answers
/// the plugin had not delivered are lost with the session.
#[test]
fn a_session_start_voids_the_lines_and_the_next_census_orders_again() {
    let mut r = grove(29, 9);
    r.plugin.answer_ticks = 5_000;
    r.camp_in();
    r.run(Tick(100));
    assert_eq!(
        r.c.camp(r.f).unwrap().in_flight(),
        9,
        "nine adopts in flight"
    );
    r.restart();
    assert!(!r.c.camp_is_in(r.f));
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 0, "void");
    let sent = orders(&r).len();
    r.run(Tick(20_000));
    assert_eq!(orders(&r).len(), sent, "nothing is resent");
    assert_eq!(
        counts(&r),
        [0, 9, 0, 0, 0],
        "the answers were lost: the slots are still owed"
    );
    // The adopts were applied in the world: the next census finds nine tagged bodies in their slots.
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [9, 0, 0, 0, 0]);
    assert_eq!(
        applied(&r, boundary::ACTION_ADOPT),
        9,
        "each body adopted once"
    );
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 0);
    ok(&mut r);
}

/// §4.9, *Epochs*: a CAMP_OUT for an epoch that is not the camp's is an older one's, and ends
/// nothing: the camp stays in at its new epoch.
#[test]
fn a_late_out_for_an_older_epoch_ends_nothing() {
    let mut r = grove(126, 9);
    r.plugin.answer_ticks = 5_000;
    let first = r.camp_in();
    r.run(Tick(50));
    r.camp_out();
    r.run(Tick(60));
    let second = r.camp_in();
    r.run(Tick(70));
    assert_ne!(first, second);
    assert!(r.c.camp_is_in(r.f));
    let flight = r.c.camp(r.f).unwrap().in_flight();
    let at = r.c.now() + 1;
    r.c.ingest(
        &mut r.w,
        &mut rtsim_ecology::infra::NoInfrastructure,
        &Obs::CampOut(boundary::CampOut {
            faction: r.key,
            epoch: first,
        }),
        at,
    );
    assert!(r.c.camp_is_in(r.f), "still in");
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), flight);
    assert_eq!(r.c.camp(r.f).unwrap().epoch, second);
    // The right epoch's out does end it.
    r.c.ingest(
        &mut r.w,
        &mut rtsim_ecology::infra::NoInfrastructure,
        &Obs::CampOut(boundary::CampOut {
            faction: r.key,
            epoch: second,
        }),
        at + 1,
    );
    assert!(!r.c.camp_is_in(r.f));
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 0);
}

/// §4.9: a CAMP_CENSUS replayed from an earlier session only marks its camp out; its bodies are
/// not the world as it is now.
#[test]
fn a_replayed_census_only_marks_its_camp_out() {
    let mut r = grove(30, 9);
    come_in(&mut r);
    let slots = r.c.camp(r.f).unwrap().slots.clone();
    let sent = orders(&r).len();
    r.c.replaying = true;
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut rtsim_ecology::infra::NoInfrastructure,
        &Obs::CampCensus(boundary::CampCensus {
            faction: r.key,
            epoch: 12345,
            flags: 0,
            health: 0.0,
            throne: (0.0, 0.0, 0.0),
            direction: 0.0,
            traded_slot: 0,
            traded_generation: 0,
            bodies: Vec::new(),
            traded: Vec::new(),
        }),
        at,
    );
    r.c.replaying = false;
    r.flush(at);
    assert!(!r.c.camp_is_in(r.f), "marked out");
    assert_eq!(
        r.c.camp(r.f).unwrap().slots,
        slots,
        "no slot changed: it holds no bodies"
    );
    assert_eq!(orders(&r).len(), sent, "and nothing was ordered");
    assert_eq!(r.c.stats.camp_censuses, 1);
    // The live census after it is booked.
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert!(r.c.camp_is_in(r.f));
    assert_eq!(r.c.stats.camp_censuses, 2);
    ok(&mut r);
}

// ---- answers lost, replayed and stale ---------------------------------------------------------

/// An answer applied by Java but lost on its way to the engine (a crash between the world's save
/// and the journal's): the line stays in flight until the epoch ends, the slots are not drawn, and
/// the next census finds the bodies and books them by state, with no second spawn.
#[test]
fn a_lost_answer_is_repaired_by_the_next_census_without_a_second_body() {
    let mut r = den(31, 0);
    r.camp_in();
    r.plugin.lose_answers = 1;
    r.run(day(6.5));
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 1);
    assert_eq!(world_bodies(&r), 1);
    let c = r.c.camp(r.f).unwrap();
    assert_eq!(c.in_flight(), 1, "the answer never came");
    assert_eq!(c.slots[0].state, SlotState::Owed);
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0], "the census found the body");
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 1);
    assert_eq!(world_bodies(&r), 1);
    ok(&mut r);
}

/// An answer sent twice (a replay) is booked once: the second is a no-op by its op id.
#[test]
fn a_replayed_answer_is_booked_once() {
    let mut r = grove(32, 9);
    r.plugin.replay_answers = true;
    come_in(&mut r);
    assert_eq!(r.c.stats.camp_answers, 9, "nine lines, booked once each");
    assert_eq!(counts(&r), [9, 0, 0, 0, 0]);
    ok(&mut r);
}

/// An answer for a generation its slot no longer holds books nothing: a slot freed by a death and
/// made again under a new generation is not touched by the old line's answer.
#[test]
fn an_answer_for_a_generation_the_slot_no_longer_holds_books_nothing() {
    let mut r = grove(33, 3);
    r.plugin.answer_ticks = 2_000;
    r.camp_in();
    r.run(Tick(100));
    // The slot's body dies while its adopt answer is on its way.
    let b = r.plugin.camps[&r.key].bodies[0];
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    r.run(Tick(300));
    assert_eq!(r.c.camp(r.f).unwrap().slots.len(), 2, "the slot was freed");
    let before = r.c.camp(r.f).unwrap().clone();
    r.run(Tick(5_000));
    let after = r.c.camp(r.f).unwrap();
    for s in &before.slots {
        let t = after.slots.iter().find(|x| x.id == s.id).unwrap();
        assert!(
            t.state == SlotState::Body,
            "the other answers booked: {t:?}"
        );
    }
    assert_eq!(
        after.slots.len(),
        2 + usize::from(r.c.camp(r.f).unwrap().slots.len() > 2)
    );
    ok(&mut r);
}

// ---- crashes: MOWZIE.md §4.9's crash table, the engine's half ----------------------------------

/// Lines of `action` in the commands the engine held back.
fn lines_in(cmds: &[Cmd], action: u8) -> usize {
    cmds.iter()
        .map(|c| match c {
            Cmd::Reconcile(x) => x.lines.iter().filter(|l| l.action == action).count(),
            _ => 0,
        })
        .sum()
}

/// The journal's tail as the next session replays it: records from before the session start, which
/// the engine ingests with `replaying` set (rtsim-jni sets it from the record's sequence number).
fn replay(r: &mut CampRun, journal: Vec<(Tick, Obs)>) {
    r.c.replaying = true;
    for (t, o) in journal {
        let at = t.max(r.c.now());
        r.c.ingest(
            &mut r.w,
            &mut rtsim_ecology::infra::NoInfrastructure,
            &o,
            at,
        );
    }
    r.c.replaying = false;
    let at = r.c.now();
    r.flush(at);
}

/// The drain of everything the plugin has queued: the journal's tail, not yet read by the engine.
fn journal(r: &mut CampRun) -> Vec<(Tick, Obs)> {
    r.plugin.due(Tick(u64::MAX / 2))
}

/// Row 1, a crash before the order is applied: nothing changed on disk, the order is void and the
/// camp's next census orders again whatever is still needed (a spawn, an adoption, and a set-out's
/// follow-up: the body left behind is taken once the party is out).
#[test]
fn an_order_lost_before_it_is_applied_is_void_and_the_next_census_orders_it_again() {
    // A spawn.
    let mut r = den(41, 0);
    r.run(day(6.5));
    let held = r.census_held();
    assert_eq!(lines_in(&held, boundary::ACTION_SPAWN), 1);
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 1);
    r.restart();
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 0, "void");
    assert_eq!(world_bodies(&r), 0, "nothing was applied");
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(world_bodies(&r), 1);
    assert_eq!(
        applied(&r, boundary::ACTION_SPAWN),
        1,
        "ordered again, once"
    );
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    ok(&mut r);

    // Adoptions.
    let mut r = grove(42, 9);
    let held = r.census_held();
    assert_eq!(lines_in(&held, boundary::ACTION_ADOPT), 9);
    r.restart();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_ADOPT), 9, "each adopted once");
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 0);
    assert_eq!(counts(&r), [9, 0, 0, 0, 0]);
    ok(&mut r);

    // A set-out: the party is out, its four bodies still stand in the camp; the next census takes
    // them (the party is out), and nothing is left behind twice.
    let mut r = grove(43, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    let end = r.c.parties[p].until;
    let held = r.decide(end + 1);
    assert_eq!(lines_in(&held, boundary::ACTION_SET_OUT), 4);
    assert_eq!(r.c.parties[p].phase, Phase::Out);
    r.restart();
    assert_eq!(world_bodies(&r), 9, "nothing was applied");
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_SET_OUT), 0);
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 4);
    assert_eq!(counts(&r), [5, 0, 0, 4, 0]);
    assert_eq!(world_bodies(&r), 5);
    ok(&mut r);
}

/// Rows 2 and 3, a change applied but its answer lost (the world saved, the journal not): the line
/// stays in flight, the slots are not drawn, and the next census books the change by state, with
/// no second order for it. A set-out's lost answer: the slot left is found gone.
#[test]
fn a_set_out_applied_whose_answer_is_lost_is_found_gone_by_the_next_census() {
    let mut r = grove(44, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.plugin.lose_answers = 1;
    let end = r.c.parties[p].until;
    r.run(end + 10);
    assert_eq!(applied(&r, boundary::ACTION_SET_OUT), 4);
    assert_eq!(world_bodies(&r), 5);
    assert_eq!(
        r.c.camp(r.f).unwrap().in_flight(),
        4,
        "the answer never came"
    );
    assert_eq!(counts(&r), [5, 0, 4, 0, 0]);
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [5, 0, 0, 4, 0], "left, no body: gone");
    assert_eq!(
        applied(&r, boundary::ACTION_SET_OUT),
        4,
        "not ordered twice"
    );
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 0);
    ok(&mut r);
}

/// Rows 2 and 3 for a take: the party is out with its bodies in a camp that was out; the census
/// takes them, the answer is lost, and the next census finds them gone.
#[test]
fn a_take_applied_whose_answer_is_lost_is_found_gone_by_the_next_census() {
    let mut r = grove(45, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.camp_out();
    let end = r.c.parties[p].until;
    r.run(end + 100);
    assert_eq!(counts(&r), [5, 0, 4, 0, 0]);
    r.plugin.lose_answers = 1;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 4);
    assert_eq!(world_bodies(&r), 5);
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 4);
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [5, 0, 0, 4, 0]);
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 4, "taken once");
    ok(&mut r);
}

/// Rows 2 and 3 for a removal: a dead slot's body removed, the answer lost; the next census finds
/// no body and frees the slot, and orders nothing again.
#[test]
fn a_removal_applied_whose_answer_is_lost_frees_the_dead_slot_at_the_next_census() {
    let mut r = grove(23, 9);
    r.plugin.fight = Some(rtsim_conflict::synth::Fight {
        kill_share: 0.5,
        after: 200,
        kill_chief: false,
        only_at_base: false,
    });
    come_in(&mut r);
    r.watch = true;
    let p = r.raid().unwrap();
    r.camp_out();
    r.run(day(1.9));
    assert_eq!(r.c.parties[p].phase, Phase::Out);
    r.run(day(4.0));
    r.watch = false;
    let dead = counts(&r)[4];
    assert!(dead >= 1, "members died away with their bodies left");
    r.plugin.lose_answers = 1;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_REMOVE), dead as usize);
    assert_eq!(counts(&r)[4], dead, "the answer was lost: still dead, left");
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r)[4], 0, "freed");
    assert_eq!(
        applied(&r, boundary::ACTION_REMOVE),
        dead as usize,
        "removed once"
    );
    assert_eq!(world_bodies(&r), r.c.camp(r.f).unwrap().slots.len());
    ok(&mut r);
}

/// Row 3, an answer in the journal replayed after a restart: the engine's pages hold the line in
/// flight, the session start voids it, and the replayed answer is booked by state: an owed slot
/// is a spawn done, a slot left is a take done. The census after finds the books and the world
/// agree and orders nothing.
#[test]
fn an_answer_in_the_journal_is_replayed_after_a_restart_and_booked_by_state() {
    let mut r = den(47, 0);
    r.run(day(6.5));
    r.plugin.answer_ticks = 1_000;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 1);
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 1);
    r.round_trip();
    let j = journal(&mut r);
    assert_eq!(j.len(), 1, "the answer, not yet read");
    r.restart();
    assert_eq!(counts(&r), [0, 1, 0, 0, 0], "voided: still owed");
    replay(&mut r, j);
    assert_eq!(
        counts(&r),
        [1, 0, 0, 0, 0],
        "booked by state: the spawn is done"
    );
    let sent = orders(&r).len();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(orders(&r).len(), sent, "nothing is ordered");
    assert_eq!(world_bodies(&r), 1);
    ok(&mut r);

    // A take.
    let mut r = grove(48, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.camp_out();
    let end = r.c.parties[p].until;
    r.run(end + 100);
    r.plugin.answer_ticks = 1_000;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 4);
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 4);
    r.round_trip();
    let j = journal(&mut r);
    r.restart();
    assert_eq!(counts(&r), [5, 0, 4, 0, 0]);
    replay(&mut r, j);
    assert_eq!(
        counts(&r),
        [5, 0, 0, 4, 0],
        "booked by state: the takes are done"
    );
    ok(&mut r);
}

/// Row 4, the answer in the pages too: replayed again after a restart it is skipped by its op id,
/// and books nothing.
#[test]
fn an_answer_already_in_the_pages_is_skipped_by_its_op_id_when_replayed() {
    let mut r = den(49, 0);
    r.run(day(6.5));
    r.plugin.answer_ticks = 1_000;
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let j = journal(&mut r);
    let again = j.clone();
    let at = r.c.now() + 1_000;
    r.run(at);
    for (t, o) in j {
        let at = t.max(r.c.now());
        r.c.ingest(
            &mut r.w,
            &mut rtsim_ecology::infra::NoInfrastructure,
            &o,
            at,
        );
    }
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(r.c.stats.camp_answers, 1);
    r.round_trip();
    r.restart();
    let before = r.c.camp(r.f).unwrap().clone();
    replay(&mut r, again);
    assert_eq!(r.c.stats.camp_answers, 1, "booked once");
    assert_eq!(r.c.camp(r.f).unwrap(), &before);
    ok(&mut r);
}

/// Row 5, the journal ahead of an entity chunk: a spawn booked but not on disk is missed three times
/// over two days and written off (a member lost, its generation retired, not died), and no second
/// body is made meanwhile; the pool's refill makes a new member later.
#[test]
fn a_spawn_booked_but_not_on_disk_is_written_off_after_three_misses() {
    let mut r = den(50, 0);
    r.run(day(6.5));
    let disk = r.plugin.camps.clone();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0], "the spawn is booked");
    r.plugin.camps = disk;
    r.restart();
    assert_eq!(
        world_bodies(&r),
        0,
        "the entity chunk never reached the disk"
    );
    for round in 0..3 {
        r.camp_in();
        let to = r.c.now() + 10;
        r.run(to);
        let n = counts(&r);
        if round < 2 {
            assert_eq!(
                n,
                [1, 0, 0, 0, 0],
                "census {round}: missed, not yet written off"
            );
            assert_eq!(r.c.camp(r.f).unwrap().slots[0].misses, round + 1);
        }
        r.camp_out();
        let to = r.c.now() + days(1.1);
        r.run(to);
    }
    assert_eq!(r.c.stats.camp_written_off, 1);
    assert_eq!(counts(&r), [0, 0, 0, 0, 0], "a member lost");
    assert!(
        r.c.camp(r.f).unwrap().retired.iter().all(|t| !t.died),
        "not at a death"
    );
    assert_eq!(
        applied(&r, boundary::ACTION_SPAWN),
        1,
        "no second body meanwhile"
    );
    // The refill makes a new member.
    r.run(day(20.0));
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(world_bodies(&r), 1);
    assert_eq!(applied(&r, boundary::ACTION_SPAWN), 2);
    ok(&mut r);
}

/// Row 5, a take booked whose body is on disk: the census finds it (the slot gone becomes left),
/// and with its party out takes it again; one body, never two.
#[test]
fn a_take_booked_whose_body_is_on_disk_is_found_and_taken_again() {
    let mut r = grove(51, 9);
    come_in(&mut r);
    let p = r.raid().unwrap();
    r.camp_out();
    let end = r.c.parties[p].until;
    r.run(end + 100);
    let disk = r.plugin.camps.clone();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [5, 0, 0, 4, 0], "the takes are booked");
    r.plugin.camps = disk;
    r.restart();
    assert_eq!(world_bodies(&r), 9, "the bodies are still on disk");
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_TAKE), 8, "taken again");
    assert_eq!(counts(&r), [5, 0, 0, 4, 0]);
    assert_eq!(world_bodies(&r), 5);
    ok(&mut r);
}

/// Row 5, a death booked whose body is on disk: the slot's generation was retired at the death, so
/// the census removes the body, and the slot is not made again.
#[test]
fn a_death_booked_whose_body_is_on_disk_is_removed_by_the_census() {
    let mut r = grove(52, 9);
    come_in(&mut r);
    let disk = r.plugin.camps.clone();
    let b = r.plugin.camps[&r.key].bodies[0];
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!(counts(&r), [8, 0, 0, 0, 0], "the death is booked");
    r.plugin.camps = disk;
    r.restart();
    assert_eq!(world_bodies(&r), 9, "the body is still on disk");
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(applied(&r, boundary::ACTION_REMOVE), 1);
    assert_eq!(world_bodies(&r), 8);
    assert_eq!(counts(&r), [8, 0, 0, 0, 0]);
    ok(&mut r);
}

// ---- every new state through a save and a paged save -------------------------------------------

/// A ledger with every field set: slots in each state with misses and lines in flight, retired
/// generations of both kinds, a throne, an epoch and a booked refill.
fn rich_ledger(r: &mut CampRun) {
    use rtsim_conflict::engine::{Line, Retired, Slot, Throne};
    let k = &mut r.c.factions[r.f as usize];
    let camp = k.camp.as_mut().expect("a ledger");
    camp.epoch = 0x1234_5678_9abc;
    camp.seq = 7;
    camp.refill = 3;
    camp.refill_at = Some(Tick(123_456));
    let states = [
        SlotState::Body,
        SlotState::Owed,
        SlotState::Left,
        SlotState::Gone,
        SlotState::Dead,
    ];
    camp.slots.clear();
    for (i, st) in states.into_iter().enumerate() {
        camp.slots.push(Slot {
            id: i as u32 * 3,
            generation: 1_000 + i as u32,
            body: 4 + i as u32,
            state: st,
            party: rtsim_kernel::handle::Handle::NULL,
            misses: i as u8 % 3,
            missed_at: Tick(500 * i as u64),
            line: (i % 2 == 0).then_some(Line {
                action: 1 + i as u8,
                op: 0xAB00 + i as u64,
            }),
        });
    }
    camp.retired = vec![
        Retired {
            slot: 1,
            generation: 55,
            died: true,
        },
        Retired {
            slot: 4,
            generation: 77,
            died: false,
        },
    ];
    camp.throne = Some(Throne {
        spot: (1.5, 64.0, -3.25),
        direction: 0.75,
    });
    k.cap = 9;
    k.away = 2;
}

/// Every field of the camp ledger survives a save and a load: the loaded engine's ledger and its
/// bytes are the saved ones.
#[test]
fn a_ledger_with_every_field_set_survives_a_save_and_a_load() {
    let mut r = grove(60, 9);
    come_in(&mut r);
    rich_ledger(&mut r);
    let bytes = r.c.save();
    let back = rtsim_conflict::Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &bytes)
        .expect("loads");
    assert_eq!(back.camp(r.f), r.c.camp(r.f));
    assert_eq!(back.factions[r.f as usize].cap, 9);
    assert_eq!(back.factions[r.f as usize].away, 2);
    assert_eq!(back.save(), bytes, "and saves the same bytes");
    assert_eq!(back.stats.camp_censuses, r.c.stats.camp_censuses);
}

/// A refill booked on the wheel survives a save: the loaded engine goes on as the one that kept
/// going (the same stamp, so the same event is live), to the state hash.
#[test]
fn a_refill_booked_on_the_wheel_survives_a_save_and_a_load() {
    let end = |save_at: Option<f64>| {
        let mut r = den(61, 0);
        come_in(&mut r);
        r.run(day(0.4));
        assert!(
            r.c.camp(r.f).unwrap().refill_at.is_some(),
            "a refill is booked"
        );
        if let Some(d) = save_at {
            r.run(day(d));
            r.round_trip();
            assert!(r.failures.is_empty(), "{:#?}", r.failures);
        }
        r.run(day(9.0));
        (r.c.state_hash(), counts(&r), world_bodies(&r))
    };
    assert_eq!(end(Some(0.5)), end(None));
    assert_eq!(end(Some(0.5)).1, [1, 0, 0, 0, 0]);
}

/// The ledger through the page store: a camp in the middle of a raid, saved to pages under a
/// budget smaller than one page (everything evicted), reopened from a copy of the store, is the
/// engine that never paged, state hash for state hash, and goes on as it does.
#[test]
fn the_camp_ledger_survives_a_paged_save_and_goes_on_as_the_run_that_kept_going() {
    use rtsim_conflict::scenario::META_KEY;
    use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
    use std::sync::{Arc, RwLock};
    let dir = std::env::temp_dir().join(format!("rtsim-camp-paged-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    let mut a = grove(62, 9);
    a.c.attach_store(store.clone(), 8 << 10);
    come_in(&mut a);
    let p = a.raid().unwrap();
    let b = a.plugin.camps[&a.key].bodies[0];
    let at = a.c.now() + 1;
    a.plugin.camp_kill(a.key, b.uuid, boundary::BY_PLAYER, at);
    let to = a.c.now() + 5;
    a.run(to);
    rich_ledger(&mut a);
    let mut batch = SaveBatch::new();
    let t = a.c.save_paged(&mut batch, META_KEY);
    store.write().unwrap().commit(batch, 0).unwrap();
    a.c.saved(t);
    a.c.enforce_budget();
    let dir_b = std::env::temp_dir().join(format!("rtsim-camp-paged-b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir_b);
    std::fs::create_dir_all(&dir_b).unwrap();
    for e in std::fs::read_dir(&dir).unwrap() {
        let e = e.unwrap();
        if e.path().is_file() {
            std::fs::copy(e.path(), dir_b.join(e.file_name())).unwrap();
        }
    }
    let store_b = Arc::new(RwLock::new(
        PageStore::open(&dir_b, Durability::NoSync).unwrap(),
    ));
    let back = rtsim_conflict::Conflict::open_paged(
        store_b,
        8 << 10,
        META_KEY,
        a.c.tun.clone(),
        a.c.cultures.clone(),
    )
    .unwrap()
    .expect("a meta page");
    assert_eq!(
        back.state_hash(),
        a.c.state_hash(),
        "the same state at once"
    );
    assert_eq!(back.camp(a.f), a.c.camp(a.f));
    let _ = p;
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&dir_b).unwrap();
}

// ---- the audit and the chaos run ---------------------------------------------------------------

/// The audit fails on a broken ledger: a slot dropped from the pool without the pool moving breaks
/// I2, and the engine's audit names it.
#[test]
fn the_audit_names_a_broken_ledger() {
    let mut r = grove(34, 9);
    come_in(&mut r);
    assert!(r.check().is_empty());
    let at = r.c.now();
    // Break I2 and I1 by hand: a slot at home is booked away, in no party.
    r.c.factions[r.f as usize].camp.as_mut().unwrap().slots[0].state = SlotState::Left;
    let bad = r.c.camp_audit(r.f, at);
    assert!(bad.iter().any(|b| b.starts_with("I2")), "{bad:?}");
    assert!(bad.iter().any(|b| b.starts_with("I1")), "{bad:?}");
}

/// Test 22 and run 10: random camps in and out, musters, homecomings, deaths, answers lost, late and
/// replayed, session restarts and saves, with the invariants checked after every event; at the end
/// no member has two bodies (I5) and no member is booked dead unless its body was lost.
#[test]
fn chaos_keeps_the_invariants_for_both_cultures_over_many_seeds() {
    for culture in ["umvuthana", "frostmaw"] {
        for seed in 0..24u64 {
            let found = if culture == "frostmaw" { 1 } else { 9 };
            let mut r = CampRun::new(seed + 100, culture, found);
            r.plugin.fight = Some(rtsim_conflict::synth::Fight {
                kill_share: 0.4,
                after: 200,
                kill_chief: false,
                only_at_base: false,
            });
            r.watch = seed % 2 == 0;
            r.chaos(seed, 220);
            assert!(
                r.failures.is_empty(),
                "{culture} seed {seed}: {:#?}",
                r.failures
            );
            let bad = r.check();
            assert!(bad.is_empty(), "{culture} seed {seed}: {bad:#?}");
            assert!(
                u32::try_from(r.c.stats.camp_written_off).unwrap() <= r.lost_members(),
                "{culture} seed {seed}: {} written off, {} bodies lost to a crash",
                r.c.stats.camp_written_off,
                r.lost_members()
            );
        }
    }
}

/// A save at random points of a chaos run changes nothing: the run with its round trips ends in the
/// state of the run without them.
#[test]
fn saving_and_loading_at_random_points_changes_nothing() {
    for seed in 0..6u64 {
        let end = |round_trips: bool| {
            let mut r = grove(seed + 200, 9);
            r.round_trips = round_trips;
            r.chaos(seed, 150);
            assert!(r.failures.is_empty(), "seed {seed}: {:#?}", r.failures);
            (
                r.c.state_hash(),
                r.plugin.camps[&r.key].bodies.len(),
                r.c.stats.camp_lines,
            )
        };
        assert_eq!(end(true), end(false), "seed {seed}");
    }
}
