// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M9a (§4.10): the Umvuthana grove's chief and its heir,
//! the tribute at the grove's price and the chest it lands in, owed hunts and plunder of
//! livestock. Played against the emulated plugin (`synth::CampRun`) on the shipped cultures.

use std::collections::BTreeMap;

use rtsim_conflict::Step;
use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{Cultures, PartyKind, SignalKind};
use rtsim_conflict::engine::{
    BLOCKED, CHIEF_SLOT, Conflict, HUNT_RADIUS, Party, Phase, Signal, SlotState, Target, body_id,
    item_of,
};
use rtsim_conflict::synth::{self, CAMP_POS, CampRun, REFINERY_POS};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, RaidOutcome};
use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::renewal::LossCause;
use rtsim_settle::NodeKind;
use rtsim_trade::{Amount, GoodId, MILLI};

fn days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round() as u64
}

fn grove(seed: u64, found: u16) -> CampRun {
    CampRun::new(seed, "umvuthana", found)
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

/// The chief's slot: its state and generation.
fn chief(r: &CampRun) -> Option<(SlotState, u32)> {
    r.c.camp(r.f)
        .and_then(|c| c.chief())
        .map(|s| (s.state, s.generation))
}

/// Where the heir is due.
fn heir_due(r: &CampRun) -> Tick {
    r.c.camp(r.f).and_then(|c| c.heir).expect("an heir").due
}

/// The bodies of the chief's slot in the camp's world.
fn chief_bodies(r: &CampRun) -> Vec<synth::CampBody> {
    r.plugin.camps[&r.key]
        .bodies
        .iter()
        .filter(|b| b.slot == CHIEF_SLOT && b.generation != 0)
        .copied()
        .collect()
}

/// The warriors standing in the camp's world (the chief is not among them).
fn warriors(r: &CampRun) -> usize {
    r.plugin.camps[&r.key]
        .bodies
        .iter()
        .filter(|b| b.slot != CHIEF_SLOT || b.generation == 0)
        .count()
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

fn lines_of(r: &CampRun, action: u8) -> usize {
    orders(r)
        .iter()
        .flat_map(|(_, x)| x.lines.iter())
        .filter(|l| l.action == action)
        .count()
}

/// The camp comes in and waits out its first heir: a seated Umvuthi, its generation.
fn seat_chief(r: &mut CampRun) -> u32 {
    come_in(r);
    let due = heir_due(r);
    let predicted =
        r.c.camp(r.f)
            .and_then(|c| c.heir_generation(r.key))
            .expect("an heir waits");
    r.run(due + 100);
    assert_eq!(chief(r), Some((SlotState::Body, predicted)), "seated");
    predicted
}

// ---- the heir -----------------------------------------------------------------------------------

/// MZB-405, MZB-408, MZB-409: a grove found without its Umvuthi waits the cap's wait (192 days)
/// for an heir, orders nothing before it, and then sends one CAMP_RECONCILE of two lines, a MASK of
/// a warrior at home and a SEAT_HEIR of the new chief, in the generation the books derived while
/// it waited. The answer seats the chief's slot: a Body with that generation, the warrior gone,
/// the wait over.
#[test]
fn a_grove_found_without_its_umvuthi_seats_an_heir_when_the_cap_wait_ends() {
    let mut r = grove(5, 9);
    assert_eq!(chief(&r), None, "found without a chief");
    assert_eq!(
        heir_due(&r),
        Tick(1 + days(192.0)),
        "the cap's wait from the finding"
    );
    come_in(&mut r);
    r.run(Tick(days(191.0)));
    assert_eq!(
        lines_of(&r, boundary::ACTION_MASK),
        0,
        "nothing before the wait is over"
    );
    let key = r.key;
    let generation =
        r.c.camp(r.f)
            .unwrap()
            .heir_generation(key)
            .expect("an heir");
    assert_eq!(
        r.c.camp(r.f).unwrap().heir_generation(key),
        Some(generation),
        "the generation does not move while the heir waits"
    );
    r.run(heir_due(&r) + 100);
    let pairs: Vec<_> = orders(&r)
        .into_iter()
        .filter(|(_, x)| {
            x.lines
                .iter()
                .any(|l| l.action == boundary::ACTION_SEAT_HEIR)
        })
        .collect();
    assert_eq!(pairs.len(), 1, "one reconcile seats the heir");
    let lines = &pairs[0].1.lines;
    assert_eq!(lines.len(), 2, "the pair");
    assert_eq!(lines[0].action, boundary::ACTION_MASK);
    assert_eq!(
        (lines[0].slot, lines[0].generation),
        (0, 0),
        "a MASK names no slot"
    );
    assert_eq!(lines[1].action, boundary::ACTION_SEAT_HEIR);
    assert_eq!(
        (lines[1].slot, lines[1].generation),
        (CHIEF_SLOT, generation)
    );
    assert_eq!(chief(&r), Some((SlotState::Body, generation)));
    assert!(r.c.camp(r.f).unwrap().heir.is_none(), "the wait is over");
    let bodies = chief_bodies(&r);
    assert_eq!(bodies.len(), 1, "one chief body");
    assert_eq!(bodies[0].generation, generation);
    assert!(bodies[0].chief);
    assert_eq!(warriors(&r), 8, "the masked warrior is gone");
    let slots = r.c.camp(r.f).unwrap().slots.len();
    assert_eq!(slots, 9, "eight warriors and the chief");
    ok(&mut r);
    r.round_trip();
    ok(&mut r);
}

/// MZB-409 (lost answer): the pair applied in the world and its answer lost, the books still
/// wait on it; the next census finds the chief body of the generation the books derived, seats it
/// and takes the masked warrior's slot off the books, and no second pair is sent.
#[test]
fn a_lost_answer_to_the_heirs_pair_is_mended_by_the_census() {
    let mut r = grove(6, 9);
    come_in(&mut r);
    let generation = r.c.camp(r.f).unwrap().heir_generation(r.key).unwrap();
    // The camp has been recruiting and sending its members out all the while: the answer to lose
    // is the pair's, the next one after the wait is nearly over.
    r.run(Tick(heir_due(&r).0 - 10));
    let before = warriors(&r);
    r.plugin.lose_answers = 1;
    r.run(heir_due(&r) + 100);
    assert_eq!(chief_bodies(&r).len(), 1, "the world has its chief");
    assert_eq!(warriors(&r), before - 1, "and one warrior less");
    assert_eq!(chief(&r), None, "the books do not know yet");
    assert_eq!(
        r.c.camp(r.f).unwrap().in_flight(),
        2,
        "the pair is in flight"
    );
    r.camp_out();
    r.run(r.c.now() + 5);
    r.camp_in();
    r.run(r.c.now() + 100);
    assert_eq!(chief(&r), Some((SlotState::Body, generation)));
    assert_eq!(lines_of(&r, boundary::ACTION_MASK), 1, "no second pair");
    assert_eq!(chief_bodies(&r).len(), 1);
    assert_eq!(warriors(&r), before - 1);
    ok(&mut r);
}

/// MZB-406, MZB-409: a player kills the heir after the world seated it and before the books heard
/// (the pair's answer is lost; a holder keeps its record for the next camp-in). The death is
/// booked as a chief's, so the pressure steps and the next heir is due after the longer wait; the
/// record that lands late seats no one, and the books never hold a chief whose body is gone.
#[test]
fn an_heir_killed_before_its_seat_is_answered_is_booked_dead_and_its_late_record_seats_no_one() {
    let mut r = grove(8, 9);
    r.plugin.holders_again = true;
    come_in(&mut r);
    let generation = r.c.camp(r.f).unwrap().heir_generation(r.key).unwrap();
    r.run(Tick(heir_due(&r).0 - 10));
    r.plugin.lose_answers = 1;
    r.run(heir_due(&r) + 100);
    assert_eq!(chief_bodies(&r).len(), 1, "the world has its chief");
    assert_eq!(chief(&r), None, "the books do not know yet");
    let body = chief_bodies(&r)[0];
    let at = r.c.now() + 1;
    assert!(
        r.plugin
            .camp_kill(r.key, body.uuid, boundary::BY_PLAYER, at)
    );
    r.run(at + 5);
    assert_eq!(chief(&r), None, "no chief, dead before it was heard");
    let wait = (heir_due(&r).0 - at.0) as f64 / TICKS_PER_DAY as f64;
    assert!(
        (26.0..31.0).contains(&wait),
        "one player's loss doubles the wait of 6 x 2^1.25 days: {wait}"
    );
    assert_ne!(
        r.c.camp(r.f).unwrap().heir_generation(r.key),
        Some(generation),
        "the next heir has another generation"
    );
    // The record the holder kept lands at the next camp-in.
    r.camp_out();
    r.run(r.c.now() + 5);
    r.camp_in();
    r.run(r.c.now() + 100);
    assert_eq!(chief(&r), None, "the late record seats no one");
    assert!(chief_bodies(&r).is_empty());
    r.run(heir_due(&r) + 100);
    let (state, second) = chief(&r).expect("the next heir sits");
    assert_eq!(state, SlotState::Body);
    assert_ne!(second, generation);
    r.settle();
    assert_eq!(r.c.stats.camp_written_off, 0, "nothing was written off");
    ok(&mut r);
}

fn home_warriors(r: &CampRun) -> usize {
    r.c.camp(r.f)
        .unwrap()
        .slots
        .iter()
        .filter(|s| s.id != CHIEF_SLOT && s.state == SlotState::Body && s.line.is_none())
        .count()
}

/// MOWZIE.md 4.10: a grove whose warriors are all dead at the due tick, and that then stays in,
/// gets its heir once one is home, not only at its next census (review P2-1).
#[test]
fn an_heir_due_while_no_warrior_is_home_comes_once_one_is() {
    let mut r = grove(5, 9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - 50));
    let uuids: Vec<_> = r.plugin.camps[&r.key]
        .bodies
        .iter()
        .filter(|b| b.generation != 0 && !b.chief)
        .map(|b| b.uuid)
        .collect();
    let at = r.c.now() + 1;
    for u in uuids {
        assert!(r.plugin.camp_kill(r.key, u, boundary::BY_MOB, at));
    }
    r.run(due + 100);
    assert_eq!(chief(&r), None, "no warrior home at the due tick: no pair");
    assert_eq!(home_warriors(&r), 0);
    r.run(due + days(60.0));
    assert!(home_warriors(&r) > 0, "warriors came home");
    assert!(
        chief(&r).is_some(),
        "the heir comes once a warrior is home: no SEAT_HEIR sent"
    );
    assert_eq!(lines_of(&r, boundary::ACTION_SEAT_HEIR), 1, "one pair");
    ok(&mut r);
}

/// With no warrior at home to mask (every one outside the footprint) Java answers the pair STALE.
/// The pair goes out once, not once for each answer: the next is an hour on (`HEIR_RETRY`), and
/// the grove that stays in is ordered it again once a warrior is home. (Review P2-1; the bodies
/// were `near` a player before review P3-7: Java takes the farthest warrior all the same.)
#[test]
fn an_heir_pair_answered_stale_is_ordered_again_while_the_grove_stays_in() {
    let mut r = grove(6, 9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - 50));
    for b in &mut r.plugin.camp_world(r.key).bodies {
        b.elsewhere = true;
    }
    r.run(due + 100);
    assert_eq!(chief(&r), None, "the pair went out and was answered stale");
    // One pair, not one for each answer: the next is an hour on.
    assert_eq!(
        lines_of(&r, boundary::ACTION_SEAT_HEIR),
        1,
        "the pair went out"
    );
    for b in &mut r.plugin.camp_world(r.key).bodies {
        b.elsewhere = false;
    }
    r.run(due + days(30.0));
    assert!(
        chief(&r).is_some(),
        "the heir is ordered again while the grove stays in"
    );
    ok(&mut r);
}

/// A grove of nine whose plunder drive is off, so the only band that musters is the one a test orders.
fn quiet_grove(seed: u64) -> CampRun {
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", 9)
}

/// A raid ordered `before` ticks ahead of the heir's due tick: its muster is still going at the due
/// tick. The band, and the camp's warriors it has drawn (Left, no line yet).
fn raid_across_the_due_tick(r: &mut CampRun, before: u64) -> (Handle<Party>, usize) {
    let due = heir_due(r);
    r.run(Tick(due.0 - before));
    let p = r.raid().expect("a raid");
    assert_eq!(r.c.parties[p].phase, Phase::Mustering, "the band musters");
    let drawn =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .filter(|s| s.state == SlotState::Left && s.line.is_none() && s.party == p)
            .count();
    assert!(
        drawn > 1,
        "the band drew {drawn} warriors, left with no line"
    );
    (p, drawn)
}

/// MZB-612, review P2-5: an heir due while a band of the camp is mustering is not ordered. The
/// band's warriors are left in the books but their bodies still stand at the camp with no line, so
/// Java could mask one of them. The pair goes out at the muster's end, just after the band's
/// set-out lines, and it is the same tick the band leaves.
#[test]
fn an_heir_due_during_a_muster_waits_for_its_end_and_is_then_ordered() {
    let mut r = quiet_grove(8);
    come_in(&mut r);
    let due = heir_due(&r);
    let (p, drawn) = raid_across_the_due_tick(&mut r, days(0.5));
    let until = r.c.parties[p].until;
    assert!(until > due + 100, "the muster outlasts the due tick");
    r.run(due + 100);
    assert_eq!(r.c.parties[p].phase, Phase::Mustering, "still mustering");
    assert_eq!(
        lines_of(&r, boundary::ACTION_SEAT_HEIR),
        0,
        "no pair is ordered while the band musters"
    );
    assert_eq!(lines_of(&r, boundary::ACTION_MASK), 0);
    assert_eq!(chief(&r), None, "no chief yet");
    // The muster ends: the set-out lines, and the pair after them in the same tick.
    r.run(until + 200);
    let sets: Vec<Tick> = orders(&r)
        .iter()
        .filter(|(_, x)| x.lines.iter().any(|l| l.action == boundary::ACTION_SET_OUT))
        .map(|(t, _)| *t)
        .collect();
    let pair: Vec<usize> = orders(&r)
        .iter()
        .enumerate()
        .filter(|(_, (_, x))| {
            x.lines
                .iter()
                .any(|l| l.action == boundary::ACTION_SEAT_HEIR)
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        sets,
        vec![until],
        "the band is set out at the muster's end, and no other band is"
    );
    assert_eq!(pair.len(), 1, "one pair");
    let (when, _) = orders(&r)[pair[0]].clone();
    assert_eq!(
        when, until,
        "the pair goes out at the muster's end, not at the next census"
    );
    assert!(
        orders(&r)[..pair[0]]
            .iter()
            .any(|(_, x)| x.lines.iter().any(|l| l.action == boundary::ACTION_SET_OUT)),
        "after the band's set-out lines"
    );
    assert_eq!(
        chief(&r).map(|c| c.0),
        Some(SlotState::Body),
        "the heir is seated"
    );
    assert_eq!(
        units(&r, p),
        drawn as u32,
        "the band lost no warrior to the MASK"
    );
    ok(&mut r);
}

/// MZB-612, review P2-5: no warrior of a band that is mustering is masked, whenever in the muster
/// the heir falls due. Over a sweep of seeds and offsets the band keeps every warrior it drew (a
/// MASK that named one is the band's loss: a casualty), and the books hold.
#[test]
fn no_warrior_of_a_mustering_band_is_masked() {
    for seed in 1..=6u64 {
        for before in [0.05, 0.3, 0.6, 0.95] {
            let mut r = quiet_grove(seed);
            come_in(&mut r);
            let (p, drawn) = raid_across_the_due_tick(&mut r, days(before));
            // A player stands near every warrior left at home, so a MASK prefers the band's: the
            // pair that goes out during the muster would mask one of them.
            let band: Vec<u32> =
                r.c.camp(r.f)
                    .unwrap()
                    .slots
                    .iter()
                    .filter(|s| s.party == p && s.state == SlotState::Left)
                    .map(|s| s.id)
                    .collect();
            for b in &mut r.plugin.camp_world(r.key).bodies {
                b.near = !band.contains(&b.slot);
            }
            let casualties = r.c.stats.casualties;
            let until = r.c.parties[p].until;
            // Just after the muster: the band has not met anyone, so a unit lost is a MASK's.
            r.run(until + 200);
            assert_eq!(
                r.c.stats.casualties, casualties,
                "seed {seed}, offset {before}: a mustering warrior was masked"
            );
            assert_eq!(
                units(&r, p),
                drawn as u32,
                "seed {seed}, offset {before}: the band lost a unit"
            );
            assert_eq!(
                lines_of(&r, boundary::ACTION_SEAT_HEIR),
                1,
                "seed {seed}, offset {before}: one pair, after the muster"
            );
            assert!(chief(&r).is_some(), "seed {seed}, offset {before}: seated");
            ok(&mut r);
        }
    }
}

/// The grove's SEAT_HEIR orders: the tick each was sent.
fn pair_ticks(r: &CampRun) -> Vec<Tick> {
    orders(r)
        .iter()
        .filter(|(_, x)| {
            x.lines
                .iter()
                .any(|l| l.action == boundary::ACTION_SEAT_HEIR)
        })
        .map(|(t, _)| *t)
        .collect()
}

/// A grove of a hundred or so whose plunder drive is fast (a plunder every few hours when it can field one):
/// enough warriors that bands could muster one after another for days. The seed does not change a run of this
/// path (six seeds of one grove were one run, and the first form of the test below said so six times), so the
/// seed sets the grove: its size, 102 to 132, and the drive's mean, 0.03 to 0.08 of a day.
fn busy_grove(seed: u64) -> CampRun {
    let size = 96 + 6 * seed as u16;
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = Some(0.02 + 0.01 * seed as f64);
        k.def.pool_cap = size as u32;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", size)
}

/// MZB-616: musters can follow one another with no cap, and since MZB-612 an heir waits while a band
/// of its camp musters. While an heir is due and its pair is not in flight, no new war party of the
/// camp musters, so the heir waits at most the muster under way at its due tick. A plunder drive
/// that tries every few hours and a raid ordered every tenth of a day, from before the due tick on,
/// seat the heir within one muster of due, and nothing musters between the due tick and the pair.
#[test]
fn musters_back_to_back_seat_the_heir_within_one_muster_of_due() {
    for seed in 1..=6u64 {
        let mut r = busy_grove(seed);
        come_in(&mut r);
        let due = heir_due(&r);
        let muster = days(1.0);
        let mut started_after_due = 0;
        // The sampling grid starts a different part of a step in for each seed.
        let mut t = Tick(due.0 - days(1.5) + days(0.1) * (seed - 1) / 6);
        let end = due + days(3.0);
        while t < end {
            // The count is read before the step: the plunder drive starts its parties inside it, and a count taken
            // after it would see only the raid ordered below.
            let before = r.c.stats.parties.iter().sum::<u64>();
            r.run(t);
            let _ = r.raid();
            let after = r.c.stats.parties.iter().sum::<u64>();
            // A band that musters at or after the due tick, with the pair not yet ordered.
            if t >= due && pair_ticks(&r).first().is_none_or(|p| *p > t) {
                started_after_due += after - before;
            }
            t += days(0.1);
        }
        assert_eq!(
            started_after_due, 0,
            "seed {seed}: a war party mustered while the heir was due and unordered"
        );
        let pair = pair_ticks(&r);
        assert_eq!(pair.len(), 1, "seed {seed}: one pair");
        assert!(
            pair[0] >= due && pair[0] <= due + muster,
            "seed {seed}: the pair went at {} (due {}), more than one muster after",
            pair[0].0,
            due.0
        );
        assert_eq!(
            chief(&r).map(|c| c.0),
            Some(SlotState::Body),
            "seed {seed}: seated"
        );
        ok(&mut r);
    }
}

/// MZB-616, the control: the plunder drive off and no band mustering, the pair goes out at the due
/// tick as before, and the rule delays nothing.
#[test]
fn with_no_muster_under_way_the_heir_is_ordered_at_its_due_tick() {
    for seed in 1..=4u64 {
        let mut r = quiet_grove(seed);
        come_in(&mut r);
        let due = heir_due(&r);
        r.run(due + 100);
        let pair = pair_ticks(&r);
        assert_eq!(pair.len(), 1, "seed {seed}");
        assert!(
            pair[0] <= due + 10,
            "seed {seed}: ordered at {} for {}",
            pair[0].0,
            due.0
        );
        assert_eq!(chief(&r).map(|c| c.0), Some(SlotState::Body), "seed {seed}");
        // After the pair, a band musters again.
        assert!(
            r.raid().is_some(),
            "seed {seed}: the rule lifts with the pair"
        );
        ok(&mut r);
    }
}

/// MZB-616: a feud that climbs to a raid while the heir is due and a muster is under way waits
/// ([`BLOCKED`], no event booked), is planned again by the pair's order at the muster's end, and
/// musters after the pair, not before it.
#[test]
fn a_feud_that_climbs_while_the_heir_waits_musters_after_the_pair() {
    let mut r = quiet_grove(9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - days(0.5)));
    let p = r.raid().expect("a raid");
    let until = r.c.parties[p].until;
    assert!(until > due + days(0.4), "the muster outlasts the due tick");
    // A feud at the Scouts rung, whose dwell ends 0.4 day after the due tick, with the grievance
    // well above the Raid rung's.
    let who = r.player;
    let s = Tick(due.0 - days(0.1));
    r.run(s);
    r.c.signal(
        &mut r.w,
        Signal {
            faction: Some(r.key),
            kind: SignalKind::Killing,
            amount: 12.0,
            actor: who,
            pos: CAMP_POS,
            district: None,
        },
        s + 1,
    );
    {
        let x = r.c.feud_mut(r.f, who).expect("a feud");
        x.step = Step::Scouts;
        x.since = Tick(s.0 + 1 + days(0.4) - days(1.0));
    }
    let before = r.c.stats.parties.iter().sum::<u64>();
    // After the dwell, with the muster still under way and the heir due.
    r.run(due + days(0.45));
    assert_eq!(r.c.parties[p].phase, Phase::Mustering);
    assert!(
        r.c.feud(r.f, who).unwrap().wait_until >= BLOCKED,
        "the feud waits for the heir"
    );
    assert_eq!(
        r.c.stats.parties.iter().sum::<u64>(),
        before,
        "no band mustered before the pair"
    );
    assert!(pair_ticks(&r).is_empty(), "the pair waits for the muster");
    // The muster ends: the pair, then the feud's raid.
    r.run(until + 300);
    let pair = pair_ticks(&r);
    assert_eq!(pair.len(), 1);
    assert_eq!(pair[0], until, "the pair goes at the muster's end");
    assert_eq!(chief(&r).map(|c| c.0), Some(SlotState::Body));
    assert!(
        r.c.stats.parties.iter().sum::<u64>() > before,
        "the feud's raid mustered once the pair was ordered"
    );
    assert!(
        r.c.feud(r.f, who).unwrap().wait_until < BLOCKED,
        "and the feud no longer waits"
    );
    ok(&mut r);
}

/// Review P3-7: Java takes the warrior farthest from every player, so with a player near all but
/// one it is that one, and with a player near every warrior it takes one all the same, never a
/// STALE answer for the pair.
#[test]
fn an_heir_takes_the_warrior_farthest_from_the_players_and_one_near_a_player_if_none_is_far() {
    let ids = |r: &CampRun| -> Vec<(u32, u32)> {
        r.plugin.camps[&r.key]
            .bodies
            .iter()
            .filter(|b| b.slot != CHIEF_SLOT)
            .map(|b| (b.slot, b.generation))
            .collect()
    };
    for all_near in [false, true] {
        let mut r = grove(6, 9);
        come_in(&mut r);
        let due = heir_due(&r);
        r.run(Tick(due.0 - 50));
        let before = ids(&r);
        let far = before[0];
        for b in r.plugin.camp_world(r.key).bodies.iter_mut().skip(1) {
            b.near = true;
        }
        if all_near {
            r.plugin.camp_world(r.key).bodies[0].near = true;
        }
        r.run(due + 100);
        assert!(
            chief(&r).is_some(),
            "the pair was done (all near: {all_near})"
        );
        assert_eq!(chief_bodies(&r).len(), 1, "the heir stands");
        let after = ids(&r);
        assert_eq!(after.len(), before.len() - 1, "one warrior was taken");
        assert_eq!(
            !after.contains(&far),
            !all_near,
            "the warrior no player stood near goes first (all near: {all_near})"
        );
        ok(&mut r);
    }
}

/// Review P3-7, MZB-433's `holdSpawn`: with a player near the throne the heir's pair waits, the
/// books keep it in flight and the world changes nothing, until the player leaves.
#[test]
fn the_heirs_pair_waits_while_a_player_is_at_the_throne_and_goes_when_the_player_leaves() {
    let mut r = grove(6, 9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - 50));
    let at = r.c.now();
    r.plugin.watch_throne(r.key, true, at);
    let standing = warriors(&r);
    r.run(due + 3_000);
    assert_eq!(
        lines_of(&r, boundary::ACTION_SEAT_HEIR),
        1,
        "the pair went out"
    );
    assert_eq!(chief(&r), None, "and waits");
    assert!(chief_bodies(&r).is_empty());
    assert_eq!(warriors(&r), standing, "no warrior was taken");
    assert!(
        r.c.camp(r.f).unwrap().in_flight() > 0,
        "the books keep it in flight"
    );
    ok(&mut r);
    let at = r.c.now();
    r.plugin.watch_throne(r.key, false, at);
    r.run(at + 20);
    assert!(chief(&r).is_some(), "the player left, the pair was done");
    assert_eq!(warriors(&r), standing - 1);
    assert_eq!(r.c.camp(r.f).unwrap().in_flight(), 0);
    ok(&mut r);
}

/// The same with the player staying: the pair goes `HOLD_MAX` ticks after it arrived, not before.
#[test]
fn the_heirs_pair_goes_after_hold_max_with_the_player_still_at_the_throne() {
    let mut r = grove(6, 9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - 50));
    let at = r.c.now();
    r.plugin.watch_throne(r.key, true, at);
    r.run(due + (synth::HOLD_MAX - 5));
    assert_eq!(chief(&r), None, "still waiting");
    r.run(due + synth::HOLD_MAX + 20);
    assert!(chief(&r).is_some(), "HOLD_MAX is up");
    assert_eq!(
        lines_of(&r, boundary::ACTION_SEAT_HEIR),
        1,
        "one pair, ordered once"
    );
    ok(&mut r);
}

/// A camp that goes out while its pair waits ends the epoch: both lines are answered STALE and the
/// world is unchanged. The grove is ordered the pair again when it is back and the player gone.
#[test]
fn a_pair_held_at_the_throne_is_answered_stale_when_the_camp_goes_out() {
    let mut r = grove(6, 9);
    come_in(&mut r);
    let due = heir_due(&r);
    r.run(Tick(due.0 - 50));
    let at = r.c.now();
    r.plugin.watch_throne(r.key, true, at);
    let standing = warriors(&r);
    r.run(due + 100);
    assert_eq!(chief(&r), None, "waiting");
    r.camp_out();
    r.run(r.c.now() + 10);
    let stale = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.outcome == boundary::LINE_STALE)
        .count();
    assert_eq!(stale, 2, "both lines were answered stale");
    assert_eq!(warriors(&r), standing);
    assert!(chief_bodies(&r).is_empty());
    let at = r.c.now();
    r.plugin.watch_throne(r.key, false, at);
    r.camp_in();
    r.run(r.c.now() + 200);
    assert!(chief(&r).is_some(), "ordered again once it is back");
    ok(&mut r);
}

/// The same, with the camp out when the heir is killed: the CAMP_OUT voided the pair's markers, so
/// the books have no line in flight to tell them the body is the heir's. A body tagged with the
/// waiting heir's generation proves the world did the seat. (Found by run 10's chaos once review
/// P2-1 and P3-1 moved its trajectories: a chief seated over a body already dead, written off.)
#[test]
fn an_heir_killed_while_the_camp_is_out_is_booked_dead_all_the_same() {
    let mut r = grove(8, 9);
    r.plugin.holders_again = true;
    come_in(&mut r);
    let generation = r.c.camp(r.f).unwrap().heir_generation(r.key).unwrap();
    r.run(Tick(heir_due(&r).0 - 10));
    r.plugin.lose_answers = 1;
    r.run(heir_due(&r) + 100);
    assert_eq!(chief_bodies(&r).len(), 1, "the world has its chief");
    assert_eq!(chief(&r), None, "the books do not know yet");
    r.camp_out();
    r.run(r.c.now() + 5);
    let body = chief_bodies(&r)[0];
    let at = r.c.now() + 1;
    assert!(
        r.plugin
            .camp_kill(r.key, body.uuid, boundary::BY_PLAYER, at)
    );
    r.run(at + 5);
    assert_eq!(chief(&r), None, "dead before it was heard");
    assert_ne!(
        r.c.camp(r.f).unwrap().heir_generation(r.key),
        Some(generation),
        "the next heir has another generation"
    );
    r.camp_in();
    r.run(r.c.now() + 100);
    assert_eq!(chief(&r), None, "the late record seats no one");
    r.run(heir_due(&r) + 100);
    let (state, second) = chief(&r).expect("the next heir sits");
    assert_eq!(state, SlotState::Body);
    assert_ne!(second, generation);
    r.settle();
    assert_eq!(r.c.stats.camp_written_off, 0, "nothing was written off");
    ok(&mut r);
}

/// MZB-406, MZB-407: a player's kill of the chief steps the camp's pressure by one, so its heir
/// is due after twice the wait of a death no player caused; the players the chief had traded with
/// are forgotten; the heir that comes has another generation. (The pressure is what is left of the
/// cap's, found without a chief and decayed over the 192 days the first heir took: 1.25.)
#[test]
fn a_chiefs_death_sets_the_next_heir_by_who_caused_it() {
    let mut waits = Vec::new();
    for by in [boundary::BY_PLAYER, boundary::BY_MOB] {
        let mut r = grove(7, 9);
        let first = seat_chief(&mut r);
        // The chief remembers a player who paid it.
        let at = r.c.now();
        r.c.ingest(
            &mut r.w,
            &mut *r.infra,
            &Obs::ChiefPaid(boundary::ChiefPaid {
                faction: r.key,
                slot: CHIEF_SLOT,
                generation: first,
                flags: 0,
                player: synth::PLAYER,
                op: 0x77,
                items: Vec::new(),
            }),
            at,
        );
        assert_eq!(r.c.camp(r.f).unwrap().traded, vec![synth::PLAYER]);
        let body = chief_bodies(&r)[0];
        let at = r.c.now() + 1;
        assert!(r.plugin.camp_kill(r.key, body.uuid, by, at));
        r.run(at + 5);
        assert_eq!(chief(&r), None, "the slot is freed");
        assert!(r.c.camp(r.f).unwrap().traded.is_empty(), "its memory goes");
        waits.push((heir_due(&r).0 - at.0) as f64 / TICKS_PER_DAY as f64);
        r.run(heir_due(&r) + 100);
        let (state, second) = chief(&r).expect("the heir sits");
        assert_eq!(state, SlotState::Body);
        assert_ne!(second, first, "another generation");
        ok(&mut r);
    }
    assert!(
        (waits[0] / waits[1] - 2.0).abs() < 0.05,
        "one player's loss doubles the wait: {waits:?}"
    );
    assert!(
        (12.0..16.0).contains(&waits[1]),
        "6 x 2^1.25 days: {waits:?}"
    );
}

/// MZB-413: the Umvuthi leads an invasion and no raid (`chief_leads`): drawn into the party, its
/// slot leaves with it; a raid leaves it at home. Whatever the party meets, the chief ends back in
/// the camp's books (or its heir seated) and the invariants hold.
#[test]
fn the_chief_leads_an_invasion_and_stays_home_from_a_raid() {
    let mut r = grove(8, 9);
    seat_chief(&mut r);
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, REFINERY_POS);
    r.c.order_party(&mut r.w, r.key, PartyKind::Raid, target, None, at)
        .expect("a raid");
    r.flush(at);
    assert_eq!(
        chief(&r).map(|c| c.0),
        Some(SlotState::Body),
        "a raid is led by a warrior"
    );
    r.settle();
    assert_eq!(chief(&r).map(|c| c.0), Some(SlotState::Body));
    let at = r.c.now();
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    assert_eq!(
        chief(&r).map(|c| c.0),
        Some(SlotState::Left),
        "the chief leads"
    );
    let in_party: Vec<u32> =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .filter(|s| s.party == p)
            .map(|s| s.id)
            .collect();
    assert!(in_party.contains(&CHIEF_SLOT), "{in_party:?}");
    r.settle();
    assert_eq!(chief(&r).map(|c| c.0), Some(SlotState::Body), "home again");
    ok(&mut r);
}

/// An Invasion led by the seated Umvuthi: its party and the chief's generation.
fn led_invasion(r: &mut CampRun) -> (Handle<Party>, u32) {
    let a = seat_chief(r);
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, REFINERY_POS);
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, target, None, at)
            .expect("an invasion");
    r.flush(at);
    assert_eq!(
        chief(r).map(|c| c.0),
        Some(SlotState::Left),
        "the chief leads"
    );
    (p, a)
}

/// MZB-412, review P3-1: the Umvuthi that leads a band and is killed away (CHIEF_KILLED, as M3's
/// Java sends it for a chief puppet) is the camp's chief slot dying away: the slot goes Dead, the
/// pressure steps once for a player's kill and the heir is due after the longer wait, and the band
/// comes home without the chief.
#[test]
fn a_chief_killed_leading_a_band_is_the_slot_dying_away_and_the_band_comes_home_without_it() {
    let mut waits = Vec::new();
    for by in [boundary::BY_PLAYER, boundary::BY_MOB] {
        let mut r = grove(8, 9);
        let (p, a) = led_invasion(&mut r);
        let at = r.c.now() + 1;
        r.c.chief_killed_by(&mut r.w, p, None, by, None, at);
        assert_eq!(chief(&r), Some((SlotState::Dead, a)), "the slot is dead");
        let camp = r.c.camp(r.f).unwrap();
        assert!(camp.heir.is_some(), "an heir waits");
        waits.push((heir_due(&r).0 - at.0) as f64 / TICKS_PER_DAY as f64);
        r.settle();
        assert_ne!(
            chief(&r).map(|c| c.0),
            Some(SlotState::Body),
            "the band comes home without the chief"
        );
        assert!(chief_bodies(&r).is_empty(), "and no chief body stands");
        // A band that musters at the due tick holds the pair to its muster's end (MZB-612): a day.
        r.run(heir_due(&r) + days(1.5));
        let (state, second) = chief(&r).expect("the heir sits");
        assert_eq!(state, SlotState::Body);
        assert_ne!(second, a, "another generation");
        ok(&mut r);
    }
    assert!(
        waits[0] > waits[1] * 1.5,
        "one player's loss doubles the wait: {waits:?}"
    );
}

/// A grove whose Umvuthi leads an Invasion with a player near its body, so that its set-out (a take)
/// is held; the chief falls away, the next census answers the held set-out stale, reports the body
/// that still stands and draws the engine's REMOVE of it, which the player holds too. The grove
/// (quiet) and the chief's generation, dead with its heir waiting.
fn dead_chiefs_body_with_a_held_removal() -> (CampRun, u32) {
    let mut r = quiet_grove(8);
    let (p, a) = led_invasion(&mut r);
    // A player stands near the chief's body from the start: its set-out (a take) is held.
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, false, now);
    let until = r.c.parties[p].until;
    r.run(until + 50);
    let chief_lines = |r: &CampRun, action: u8| {
        r.plugin
            .applied
            .iter()
            .filter(|l| l.line.action == action && l.line.slot == CHIEF_SLOT)
            .map(|l| l.outcome)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        chief_lines(&r, boundary::ACTION_SET_OUT),
        Vec::<u8>::new(),
        "the chief's set-out waits on the player: no answer, no limit"
    );
    assert_eq!(chief_bodies(&r).len(), 1, "its body stands");
    // The chief falls away. The next census reports its body, after answering the held set-out
    // stale; the engine orders the removal of the body of a slot it booked dead.
    let at = r.c.now() + 1;
    r.c.chief_killed_by(&mut r.w, p, None, boundary::BY_PLAYER, None, at);
    r.run(at + 5);
    assert_eq!(chief(&r), Some((SlotState::Dead, a)), "the slot is dead");
    r.camp_in();
    let to = r.c.now() + 20;
    r.run(to);
    assert_eq!(
        chief_lines(&r, boundary::ACTION_SET_OUT),
        vec![boundary::LINE_STALE],
        "the census answered the held set-out stale"
    );
    assert_eq!(
        lines_of(&r, boundary::ACTION_REMOVE),
        1,
        "the engine orders the dead chief's body removed"
    );
    assert_eq!(
        chief_lines(&r, boundary::ACTION_REMOVE),
        Vec::<u8>::new(),
        "held: not applied, not answered"
    );
    assert_eq!(chief_bodies(&r).len(), 1, "the body is still reported");
    assert_eq!(chief(&r), Some((SlotState::Dead, a)), "and its slot dead");
    (r, a)
}

/// Emulator divergence (c), MZB-613: Java holds a chief's TAKE, REMOVE or SET_OUT while a player is
/// near its body, with no time limit, and answers it stale at each census or the camp's out; the
/// body stays in the world, so the next census reports it. A heir seated over that body (the pair's
/// L_A names its generation) is not held by the player's nearness, only by a fight, and the removal
/// held on the body is answered stale in the same batch as the pair's DONE, before it. The books
/// end with the new chief alone.
#[test]
fn a_removal_held_on_the_dead_chiefs_body_is_answered_stale_in_the_batch_of_the_pair_that_sits_over_it()
 {
    let (mut r, a) = dead_chiefs_body_with_a_held_removal();
    // The heir is due: the pair sits over the body, the removal is stale first.
    let applied = r.plugin.applied.len();
    r.run(heir_due(&r) + 100);
    let seat = r.plugin.applied[applied..]
        .iter()
        .find(|l| l.line.action == boundary::ACTION_SEAT_HEIR)
        .expect("the pair was applied")
        .at;
    let batch: Vec<(u8, u8)> = r.plugin.applied[applied..]
        .iter()
        .filter(|l| l.at == seat)
        .map(|l| (l.line.action, l.outcome))
        .collect();
    assert_eq!(
        batch,
        vec![
            (boundary::ACTION_REMOVE, boundary::LINE_STALE),
            (boundary::ACTION_MASK, boundary::LINE_DONE),
            (boundary::ACTION_SEAT_HEIR, boundary::LINE_DONE),
        ],
        "the removal's STALE, then the pair's two DONEs, in one tick"
    );
    let (state, b) = chief(&r).expect("the heir sits");
    assert_eq!(state, SlotState::Body);
    assert_ne!(b, a, "another generation");
    let bodies = chief_bodies(&r);
    assert_eq!(bodies.len(), 1, "the old body is gone, the new one stands");
    assert_eq!(bodies[0].generation, b);
    ok(&mut r);
}

/// Emulator divergence (c): a pair seated over a chief body in a fight waits on the fight, with no
/// limit (`HOLD_MAX` does not apply), and goes when the fight is over, a player near the throne or
/// the body notwithstanding. Another chief body of the camp (a second body, another generation)
/// stays when the heir sits (review P2-3: only the L_A generation's body is replaced).
#[test]
fn a_pair_over_a_chief_in_a_fight_waits_on_the_fight_alone_and_leaves_a_second_body_standing() {
    let (mut r, a) = dead_chiefs_body_with_a_held_removal();
    let pairs = lines_of(&r, boundary::ACTION_SEAT_HEIR);
    // A second chief body of another generation stands too.
    let second = r.plugin.camp_tagged_body(r.key, 1, CHIEF_SLOT, 777);
    for body in &mut r.plugin.camp_world(r.key).bodies {
        if body.uuid == second {
            body.chief = true;
        }
    }
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, true, now);
    r.plugin.watch_throne(r.key, true, now);
    r.run(heir_due(&r) + 3 * synth::HOLD_MAX);
    assert_eq!(
        lines_of(&r, boundary::ACTION_SEAT_HEIR),
        pairs + 1,
        "the pair went out"
    );
    assert_eq!(
        chief(&r),
        Some((SlotState::Dead, a)),
        "and waits on the fight, past HOLD_MAX"
    );
    assert_eq!(chief_bodies(&r).len(), 2, "both old bodies stand");
    // The fight ends; the players still stand near the body and the throne, which hold neither
    // the pair over an old body nor (for the throne) anything else now.
    let now = r.c.now();
    r.plugin.hold_chief(r.key, true, false, now);
    r.run(now + 10);
    let (state, b) = chief(&r).expect("the heir sits");
    assert_eq!(state, SlotState::Body);
    assert_ne!(b, a);
    let mut gens: Vec<u32> = chief_bodies(&r).iter().map(|c| c.generation).collect();
    gens.sort_unstable();
    let mut want = vec![777, b];
    want.sort_unstable();
    assert_eq!(
        gens, want,
        "the old chief's body was replaced, the second stays"
    );
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
}

/// MZB-295, MZB-412: a crash between the CHIEF_KILLED and the DEMATERIALIZED does not bring the
/// chief home: the slot is dead before the band's hand-back, saved so, and the replay finds no
/// chief to return.
#[test]
fn a_crash_after_the_chiefs_death_away_does_not_bring_it_home() {
    let mut r = grove(8, 9);
    let (p, a) = led_invasion(&mut r);
    let at = r.c.now() + 1;
    r.c.chief_killed_by(&mut r.w, p, None, boundary::BY_PLAYER, None, at);
    r.round_trip();
    assert_eq!(chief(&r), Some((SlotState::Dead, a)), "saved dead");
    r.restart();
    r.settle();
    assert_ne!(chief(&r).map(|c| c.0), Some(SlotState::Body));
    assert!(chief_bodies(&r).is_empty());
    ok(&mut r);
}

/// Review P3-1 (lesson 1, a copy booked by state once its answer is forgotten): a SEAT_HEIR answer
/// for chief A, journaled again after a save made the original durable, lands after A died away.
/// It does not raise A again, and the heir that waits still waits.
#[test]
fn a_late_seat_copy_does_not_raise_a_chief_that_died_away() {
    let mut r = grove(8, 9);
    r.c.obs_seq = 10;
    let (p, a) = led_invasion(&mut r);
    let (op, line) = seat_line(&r);
    let at = r.c.now() + 1;
    assert!(r.c.chief_died_away(r.f, LossCause::Player, at));
    let _ = p;
    assert_eq!(chief(&r), Some((SlotState::Dead, a)));
    assert!(r.c.camp(r.f).unwrap().heir.is_some(), "an heir waits");
    let stamp = r.c.camp(r.f).unwrap().heir_stamp;
    seat_copy(&mut r, op, line, a);
    assert_eq!(chief(&r), Some((SlotState::Dead, a)), "A is not raised");
    let camp = r.c.camp(r.f).unwrap();
    assert!(camp.heir.is_some(), "the heir still waits");
    assert_eq!(camp.heir_stamp, stamp, "and its booking stands");
}

/// A copy for an old generation reaching a live chief of a newer generation leaves its trade
/// memory and the heir's stamp alone (review P3-1).
#[test]
fn a_late_seat_copy_for_an_old_generation_leaves_the_live_chief_alone() {
    let mut r = grove(9, 9);
    r.c.obs_seq = 10;
    let a = seat_chief(&mut r);
    let (op, line) = seat_line(&r);
    // A dies at the camp; the next heir sits.
    let body = chief_bodies(&r)[0];
    let at = r.c.now() + 1;
    assert!(r.plugin.camp_kill(r.key, body.uuid, boundary::BY_MOB, at));
    r.run(at + 5);
    r.run(heir_due(&r) + 100);
    let (state, b) = chief(&r).expect("the next heir sits");
    assert_eq!((state, b == a), (SlotState::Body, false));
    // B remembers a player who paid it.
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot: CHIEF_SLOT,
            generation: b,
            flags: 0,
            player: synth::PLAYER,
            op: 0x78,
            items: Vec::new(),
        }),
        at,
    );
    assert_eq!(r.c.camp(r.f).unwrap().traded, vec![synth::PLAYER]);
    let stamp = r.c.camp(r.f).unwrap().heir_stamp;
    seat_copy(&mut r, op, line, a);
    assert_eq!(chief(&r), Some((SlotState::Body, b)), "B stands");
    let camp = r.c.camp(r.f).unwrap();
    assert_eq!(camp.traded, vec![synth::PLAYER], "its memory stands");
    assert_eq!(camp.heir_stamp, stamp, "and the heir's stamp");
}

/// A MASK answer naming `slot` and `generation`, as a plugin that masked that warrior sends it.
fn mask_answer(r: &mut CampRun, slot: u32, generation: u32) {
    let epoch = r.c.camp(r.f).unwrap().seen;
    let answer = boundary::CampBodies {
        faction: r.key,
        epoch,
        op: 0x3A5C_0000_0000_0001,
        lines: vec![boundary::LineAnswer {
            line: 0,
            slot,
            generation,
            action: boundary::ACTION_MASK,
            outcome: boundary::LINE_DONE,
        }],
    };
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut *r.infra, &Obs::CampBodies(answer), at);
}

/// The warriors a party holds, by its units.
fn units(r: &CampRun, p: Handle<Party>) -> u32 {
    r.c.parties[p].units.iter().map(|u| u.1).sum()
}

/// MZB-445, review P3-5: a MASK answered for a warrior the books have set out (a raid mustered
/// between the pair's order and its answer) is that band's loss, as one of its members dying at
/// the camp is: the slot is freed, the band loses the unit and comes home one short, and the
/// grove's pool and slots still add up. Before, the slot went Gone and the band kept the unit: the
/// grove ended one warrior over.
#[test]
fn a_mask_that_names_a_warrior_that_set_out_is_the_bands_loss() {
    let mut r = grove(8, 9);
    come_in(&mut r);
    let p = r.raid().expect("a raid");
    let before = units(&r, p);
    assert!(before > 1, "a band of {before}");
    let away = r.c.factions[r.f as usize].away;
    let casualties = r.c.stats.casualties;
    let (slot, generation) =
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .find(|s| s.party == p && s.state == SlotState::Left && s.id != CHIEF_SLOT)
            .map(|s| (s.id, s.generation))
            .expect("a warrior set out");
    mask_answer(&mut r, slot, generation);
    let held = |r: &CampRun| {
        r.c.camp(r.f)
            .unwrap()
            .slots
            .iter()
            .any(|s| s.id == slot && s.generation == generation)
    };
    assert!(!held(&r), "the slot is freed");
    assert_eq!(units(&r, p), before - 1, "the band lost the unit");
    assert_eq!(
        r.c.factions[r.f as usize].away,
        away - 1,
        "and so did the count away"
    );
    assert_eq!(r.c.stats.casualties, casualties + 1);
    // The same answer again, a holder's copy, books nothing more.
    mask_answer(&mut r, slot, generation);
    assert_eq!(units(&r, p), before - 1, "a copy loses no second unit");
    r.settle();
    assert!(r.c.parties.get(p).is_none(), "the band came home");
    assert!(!held(&r));
    ok(&mut r);
}

/// The first SEAT_HEIR the engine sent: its op and line.
fn seat_line(r: &CampRun) -> (u64, u32) {
    orders(r)
        .iter()
        .find_map(|(_, x)| {
            x.lines
                .iter()
                .position(|l| l.action == boundary::ACTION_SEAT_HEIR)
                .map(|i| (x.op, i as u32))
        })
        .expect("the pair")
}

/// A holder's copy of the SEAT_HEIR answer for `generation`, after a save made the original
/// durable (so the engine has forgotten the answer: a `CampRun`'s own are kept to the end of the
/// epoch, `obs_seq` 0).
fn seat_copy(r: &mut CampRun, op: u64, line: u32, generation: u32) {
    r.c.durable_obs_seq = 20;
    r.c.obs_seq = 30;
    let epoch = r.c.camp(r.f).unwrap().seen;
    let copy = boundary::CampBodies {
        faction: r.key,
        epoch,
        op,
        lines: vec![boundary::LineAnswer {
            line,
            slot: CHIEF_SLOT,
            generation,
            action: boundary::ACTION_SEAT_HEIR,
            outcome: boundary::LINE_DONE,
        }],
    };
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut *r.infra, &Obs::CampBodies(copy), at);
}

/// MZB-405: a census that finds an untagged Umvuthi in a camp found without one adopts it as the
/// chief (an ADOPT line on the chief's slot), and the heir's wait is over; the warrior slot its
/// body had counted for is owed again and spawned.
#[test]
fn a_census_that_finds_an_untagged_umvuthi_adopts_it_as_the_chief() {
    let mut r = grove(9, 9);
    let umvuthi = body_id(&r.c.cultures, "mic_rtsim:conflict/umvuthana/umvuthi");
    {
        let b = r.plugin.camp_world(r.key).bodies.first_mut().unwrap();
        b.body = umvuthi;
        b.chief = true;
    }
    assert!(
        r.c.camp(r.f).unwrap().heir.is_some_and(|h| h.found_empty),
        "waiting for an heir"
    );
    come_in(&mut r);
    assert!(r.c.camp(r.f).unwrap().heir.is_none(), "the wait is over");
    assert!(matches!(chief(&r), Some((SlotState::Body, g)) if g != 0));
    let adopted: Vec<_> = orders(&r)
        .into_iter()
        .flat_map(|(_, x)| x.lines)
        .filter(|l| l.action == boundary::ACTION_ADOPT && l.slot == CHIEF_SLOT)
        .collect();
    assert_eq!(adopted.len(), 1, "the chief's slot adopts the body");
    assert_eq!(chief_bodies(&r).len(), 1);
    assert_eq!(warriors(&r), 9, "eight warriors found and one spawned");
    ok(&mut r);
}

// ---- tribute, trade and the chest ---------------------------------------------------------------

/// The item dictionary of a good by name: its engine item id, named for the conflict.
fn item(r: &mut CampRun, name: &str) -> (GoodId, u32) {
    let g = r.w.goods.find(name).expect("a good of the synthetic world");
    let id = item_of(g);
    r.c.define(2, id, name);
    (g, id)
}

fn grievance(r: &CampRun) -> f64 {
    r.c.feud_view(r.key, r.player).map_or(0.0, |v| v.grievance)
}

/// A player's payment to an Umvuthi body, as the plugin reports it.
fn paid(r: &mut CampRun, slot: u32, generation: u32, flags: u32, op: u64, items: &[(u32, i32)]) {
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot,
            generation,
            flags,
            player: synth::PLAYER,
            op,
            items: items
                .iter()
                .map(|&(item, count)| boundary::Item { item, count })
                .collect(),
        }),
        at,
    );
}

/// The relief a player's payment brings (the grievance it takes off a killing's, which is large
/// enough that no payment below clears it), in a fresh run.
fn relief(pay: impl FnOnce(&mut CampRun)) -> f64 {
    let mut r = grove(10, 9);
    r.kill(40.0);
    let g0 = grievance(&r);
    pay(&mut r);
    let g1 = grievance(&r);
    assert!(
        g1 > 0.0,
        "a payment that clears the feud proves nothing ({g0})"
    );
    g0 - g1
}

/// MZB-416: gold nuggets are the grove's tribute and the haven's market has no price for them: they
/// are worth what the grove asks per coin (9), so 81 nuggets soften a feud as 9 coins do. A trade
/// at the grove (TRADED, MZB-417) is worth a tenth of that: 810 nuggets are 9 coins' relief.
#[test]
fn unpriced_tribute_is_worth_the_groves_price_and_a_trade_a_tenth() {
    let nuggets = relief(|r| {
        let (_, id) = item(r, "gold_nugget");
        paid(r, 0, 0, boundary::PAID_STRAY, 0x31, &[(id, 81)]);
    });
    let coins = relief(|r| {
        let (_, id) = item(r, "emerald");
        paid(r, 0, 0, boundary::PAID_STRAY, 0x32, &[(id, 9)]);
    });
    assert!(nuggets > 1.0, "a gift softens the feud: {nuggets}");
    assert!(
        (nuggets - coins).abs() < 1e-9,
        "81 nuggets {nuggets} are 9 coins {coins}"
    );
    let traded = relief(|r| {
        let at = r.c.now();
        let t = Obs::Traded(boundary::Traded {
            faction: r.key,
            player: synth::PLAYER,
            nuggets: 810,
            op: 0x33,
        });
        r.c.ingest(&mut r.w, &mut *r.infra, &t, at);
    });
    assert!(
        (traded - coins).abs() < 1e-9,
        "a trade of 810 nuggets is worth 9 coins' relief at a tenth: {traded} against {coins}"
    );
}

/// MZB-418, MZB-420: a payment to the live Umvuthi (its slot and generation, Body) joins its trade
/// memory, once, sorted; a payment to a stray, to another generation, to a slot that is not the
/// chief's, or to no chief at all does not; the memory is at most `TRADED_CAP`.
#[test]
fn only_a_payment_to_the_live_chief_is_remembered() {
    let mut r = grove(11, 9);
    let (_, id) = item(&mut r, "gold_nugget");
    come_in(&mut r);
    let memory = |r: &CampRun| r.c.camp(r.f).unwrap().traded.clone();
    // No chief yet: tribute and no memory.
    paid(&mut r, CHIEF_SLOT, 5, 0, 0x41, &[(id, 9)]);
    assert!(memory(&r).is_empty(), "no chief seated");
    let generation = seat_chief(&mut r);
    paid(
        &mut r,
        CHIEF_SLOT,
        generation.wrapping_add(1),
        0,
        0x42,
        &[(id, 9)],
    );
    assert!(memory(&r).is_empty(), "another generation");
    paid(
        &mut r,
        CHIEF_SLOT,
        generation,
        boundary::PAID_STRAY,
        0x43,
        &[(id, 9)],
    );
    assert!(memory(&r).is_empty(), "a stray Umvuthi");
    paid(&mut r, 0, 1, 0, 0x44, &[(id, 9)]);
    assert!(memory(&r).is_empty(), "a warrior's slot");
    paid(&mut r, CHIEF_SLOT, generation, 0, 0x45, &[(id, 9)]);
    assert_eq!(memory(&r), vec![synth::PLAYER]);
    paid(&mut r, CHIEF_SLOT, generation, 0, 0x46, &[(id, 9)]);
    assert_eq!(memory(&r), vec![synth::PLAYER], "once");
    // The memory is bounded.
    let mut camp = r.c.camp(r.f).unwrap().clone();
    camp.traded.clear();
    for n in 0..(rtsim_conflict::engine::TRADED_CAP as u64 + 5) {
        camp.remember((n, n));
    }
    assert_eq!(camp.traded.len(), rtsim_conflict::engine::TRADED_CAP);
    assert!(camp.traded.windows(2).all(|w| w[0] < w[1]), "sorted");
    assert!(camp.remember((0, 0)), "one already in is still in");
    assert!(!camp.remember((u64::MAX, 0)), "a full memory takes no more");
    ok(&mut r);
}

/// MZB-420, MZB-421: the live chief's census lists the players it remembers; the engine unions
/// that with its own memory, and when the body lacks some of what the engine holds it writes them
/// (CHIEF_BODY). A census of another generation's chief says nothing about this one.
#[test]
fn the_censuss_trade_memory_is_unioned_and_a_short_body_is_written_to() {
    let mut r = grove(12, 9);
    let (_, id) = item(&mut r, "gold_nugget");
    let generation = seat_chief(&mut r);
    paid(&mut r, CHIEF_SLOT, generation, 0, 0x51, &[(id, 9)]);
    let census = |r: &mut CampRun, slot: u32, generation: u32, traded: Vec<boundary::Uuid>| {
        let at = r.c.now() + 1;
        r.plugin.camp_in(r.key, at);
        let mut q = r.plugin.drop_queued();
        let Some(Obs::CampCensus(c)) = q.first_mut() else {
            panic!("a census");
        };
        c.traded_slot = slot;
        c.traded_generation = generation;
        c.traded = traded;
        let o = q.remove(0);
        r.c.ingest(&mut r.w, &mut *r.infra, &o, at);
        r.c.take_commands()
    };
    let other = (0xAAAA, 0xBBBB);
    // The wrong generation: its list is not this chief's.
    let cmds = census(&mut r, CHIEF_SLOT, generation.wrapping_add(1), vec![other]);
    assert_eq!(r.c.camp(r.f).unwrap().traded, vec![synth::PLAYER]);
    assert!(!cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))));
    // The chief's own: a player the engine did not know joins, and the body lacks the engine's.
    let cmds = census(&mut r, CHIEF_SLOT, generation, vec![other]);
    let mut want = vec![synth::PLAYER, other];
    want.sort_unstable();
    assert_eq!(r.c.camp(r.f).unwrap().traded, want);
    let written: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::ChiefBody(b) => Some(b.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(written.len(), 1, "the body is written to: {cmds:?}");
    // The body with the whole list is not written to.
    let cmds = census(&mut r, CHIEF_SLOT, generation, want.clone());
    assert!(
        !cmds.iter().any(|c| matches!(c, Cmd::ChiefBody(_))),
        "{cmds:?}"
    );
}

/// The grove's tribute (567 nuggets, one stack at most 64 at a time): a run with the camp in and
/// `pay` done.
fn tribute_run(seed: u64) -> (CampRun, GoodId) {
    let mut r = grove(seed, 9);
    let (nugget, id) = item(&mut r, "gold_nugget");
    come_in(&mut r);
    paid(&mut r, 0, 0, boundary::PAID_STRAY, 0x61, &[(id, 567)]);
    (r, nugget)
}

fn owed(r: &CampRun) -> i64 {
    r.c.camp(r.f)
        .unwrap()
        .owed_tribute
        .iter()
        .map(|e| e.1)
        .sum::<i64>()
        / MILLI
}

fn chest(r: &mut CampRun, free: u32) {
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::HavenChest(boundary::HavenChest {
            faction: r.key,
            dimension: u32::from(synth::CAMP_POS.frame),
            pos: (synth::CAMP_POS.x + 3, 64, synth::CAMP_POS.z + 2),
            free,
        }),
        at,
    );
}

fn stock(r: &CampRun, g: GoodId) -> Amount {
    r.w.market(r.c.factions[r.f as usize].node).stock(g)
}

/// MZB-423: tribute booked is owed to the chest (567 nuggets); with no chest known nothing is
/// sent; once the plugin names one the stacks go one at a time, each after the last one's answer,
/// whole stacks of at most 64, until every nugget has left the grove's books; the chest has a slot
/// less for each stack.
#[test]
fn tribute_goes_to_the_chest_in_stacks_one_after_another() {
    let (mut r, nugget) = tribute_run(20);
    let held = stock(&r, nugget);
    assert!(
        held >= 567 * MILLI,
        "the nuggets are stocked at the haven: {held}"
    );
    assert_eq!(owed(&r), 567);
    assert_eq!(r.c.placings().count(), 0, "no chest, nothing sent");
    chest(&mut r, 27);
    let mut amounts = Vec::new();
    let mut ops = Vec::new();
    loop {
        let Some((op, p)) = r.c.placings().next() else {
            break;
        };
        assert_eq!(r.c.placings().count(), 1, "one at a time");
        assert_eq!(p.good, u32::from(nugget.0));
        assert_eq!(p.dimension, u32::from(synth::CAMP_POS.frame));
        assert_eq!(p.pos, (synth::CAMP_POS.x + 3, 64, synth::CAMP_POS.z + 2));
        amounts.push(p.amount / MILLI);
        ops.push(op);
        let at = r.c.now();
        assert!(r.c.tribute_placed(&mut r.w, op, at));
    }
    assert_eq!(amounts, [64, 64, 64, 64, 64, 64, 64, 64, 55]);
    let distinct: std::collections::BTreeSet<u64> = ops.iter().copied().collect();
    assert_eq!(distinct.len(), 9, "each stack has its own op");
    assert_eq!(owed(&r), 0);
    assert_eq!(r.c.camp(r.f).unwrap().stacks, 9);
    assert_eq!(r.c.camp(r.f).unwrap().chest.map(|c| c.free), Some(27 - 9));
    assert_eq!(r.c.stats.tribute_stacks, 9);
    assert_eq!(
        stock(&r, nugget),
        held - 567 * MILLI,
        "they have left the books"
    );
    ok(&mut r);
}

/// MZB-423 (failure and duplicates): a stack the plugin could not put down goes back to the
/// grove's stock, the tribute is still owed and the chest is forgotten until it is named again,
/// when the next stack goes under a new op. An answer for an op that is not in flight (a second
/// answer to the same stack, an unknown op) changes nothing.
#[test]
fn a_failed_stack_goes_back_and_a_duplicate_answer_changes_nothing() {
    let (mut r, nugget) = tribute_run(21);
    let held = stock(&r, nugget);
    chest(&mut r, 27);
    let (op1, p1) = r.c.placings().next().expect("a stack");
    assert_eq!(stock(&r, nugget), held - 64 * MILLI, "booked as leaving");
    let at = r.c.now();
    assert!(r.c.tribute_failed(&mut r.w, op1, at));
    assert_eq!(stock(&r, nugget), held, "back");
    assert_eq!(owed(&r), 567);
    assert!(
        r.c.camp(r.f).unwrap().chest.is_none(),
        "the chest is forgotten"
    );
    assert_eq!(r.c.placings().count(), 0);
    assert!(!r.c.tribute_failed(&mut r.w, op1, at), "answered once");
    assert!(
        !r.c.tribute_placed(&mut r.w, op1, at),
        "and not placed after"
    );
    assert!(
        !r.c.tribute_placed(&mut r.w, 0xDEAD, at),
        "an op of no stack"
    );
    chest(&mut r, 27);
    let (op2, p2) = r.c.placings().next().expect("the chest again");
    assert_ne!(op1, op2, "a new op");
    assert_eq!(p1.amount, p2.amount);
    assert!(r.c.tribute_placed(&mut r.w, op2, at));
    assert!(!r.c.tribute_placed(&mut r.w, op2, at), "a duplicate answer");
    assert_eq!(owed(&r), 567 - 64, "lowered once");
    ok(&mut r);
}

/// MZB-423 (saves): a stack in flight is saved with its op, dimension and place, the chest is not
/// (it is the plugin's word, latest wins); after a load the stack's answer lowers what is owed, and
/// the next stack goes only when the chest is named again, under an op the saved counter makes new.
#[test]
fn a_stack_in_flight_survives_a_save_and_the_chest_does_not() {
    let (mut r, _) = tribute_run(22);
    chest(&mut r, 27);
    let before: Vec<_> = r.c.placings().collect();
    assert_eq!(before.len(), 1);
    r.round_trip();
    assert_eq!(r.c.placings().collect::<Vec<_>>(), before);
    assert!(
        r.c.camp(r.f).unwrap().chest.is_none(),
        "the chest is not saved"
    );
    assert_eq!(owed(&r), 567);
    let at = r.c.now();
    assert!(r.c.tribute_placed(&mut r.w, before[0].0, at));
    assert_eq!(owed(&r), 567 - 64);
    assert_eq!(r.c.placings().count(), 0, "no chest, no next stack");
    chest(&mut r, 27);
    let (op, _) = r.c.placings().next().expect("the next stack");
    assert_ne!(op, before[0].0);
    ok(&mut r);
}

/// MZB-446: only the tribute good is owed to the chest, since another item's stack may exceed its
/// limit; other gifts are stocked and not placed. A camp that is out places nothing, and one that
/// goes out forgets its chest but not the stack in flight.
#[test]
fn only_the_tribute_good_is_owed_and_a_camp_that_is_out_places_nothing() {
    let (mut r, nugget) = tribute_run(23);
    let (emerald, id) = item(&mut r, "emerald");
    paid(&mut r, 0, 0, boundary::PAID_STRAY, 0x62, &[(id, 40)]);
    let camp = r.c.camp(r.f).unwrap();
    assert_eq!(camp.owed_tribute.len(), 1, "{:?}", camp.owed_tribute);
    assert_eq!(camp.owed_tribute[0].0, u32::from(nugget.0));
    assert!(stock(&r, emerald) >= 40 * MILLI, "the emeralds are stocked");
    chest(&mut r, 27);
    let (op, _) = r.c.placings().next().expect("a stack");
    r.camp_out();
    r.run(r.c.now() + 5);
    assert!(
        r.c.camp(r.f).unwrap().chest.is_none(),
        "forgotten at CAMP_OUT"
    );
    assert_eq!(r.c.placings().count(), 1, "the stack in flight stays");
    let at = r.c.now();
    assert!(r.c.tribute_placed(&mut r.w, op, at));
    assert_eq!(
        r.c.placings().count(),
        0,
        "nothing goes while the camp is out"
    );
    chest(&mut r, 27);
    assert_eq!(
        r.c.placings().count(),
        0,
        "a chest named to a camp that is out"
    );
    ok(&mut r);
}

/// Two runs of one grove, equal at the save a crash falls back to: the camp is in, the gold nugget
/// is defined and nothing is owed. The first plays the session, the second the restart.
fn twin_groves(seed: u64) -> (CampRun, CampRun, u32) {
    let mut a = grove(seed, 9);
    let mut b = grove(seed, 9);
    let (_, id) = item(&mut a, "gold_nugget");
    item(&mut b, "gold_nugget");
    come_in(&mut a);
    come_in(&mut b);
    assert_eq!(a.c.state_hash(), b.c.state_hash(), "equal at the save");
    (a, b, id)
}

/// A stray Umvuthi's payment of `n` nuggets, as the journal holds it.
fn pay_record(key: u64, id: u32, op: u64, n: i32) -> Obs {
    Obs::ChiefPaid(boundary::ChiefPaid {
        faction: key,
        slot: 0,
        generation: 0,
        flags: boundary::PAID_STRAY,
        player: synth::PLAYER,
        op,
        items: vec![boundary::Item { item: id, count: n }],
    })
}

/// The chest record, as the journal holds it.
fn chest_record(key: u64, free: u32) -> Obs {
    Obs::HavenChest(boundary::HavenChest {
        faction: key,
        dimension: u32::from(synth::CAMP_POS.frame),
        pos: (synth::CAMP_POS.x + 3, 64, synth::CAMP_POS.z + 2),
        free,
    })
}

/// A session played: the records at their game times, the world run to each first.
fn play(r: &mut CampRun, tail: &[(Tick, Obs)]) {
    for (t, o) in tail {
        r.run(*t);
        r.c.ingest(&mut r.w, &mut *r.infra, o, *t);
    }
}

/// The restart after a crash: the session starts, the journal's tail since the save is replayed
/// into it as the host does it (each record at its game time), the replay is over, and the camp
/// comes in again.
fn restart_and_replay(r: &mut CampRun, tail: &[(Tick, Obs)]) {
    r.restart_to_replay();
    play(r, tail);
    r.c.replay_done();
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

/// MZB-446 (review P3-4): a chest the plugin named while the camp was out holds its stack until the
/// camp is in, and the census that brings it in places it: the tribute does not wait for another
/// chest record or payment.
#[test]
fn the_census_places_the_tribute_a_chest_named_while_the_camp_was_out_waited_for() {
    let (mut r, _) = tribute_run(26);
    r.camp_out();
    let to = r.c.now() + 5;
    r.run(to);
    chest(&mut r, 27);
    assert_eq!(
        r.c.camp(r.f).unwrap().chest.map(|c| c.free),
        Some(27),
        "the chest is kept"
    );
    assert_eq!(r.c.placings().count(), 0, "the camp is out");
    assert_eq!(owed(&r), 567);
    r.camp_in();
    let to = r.c.now() + 5;
    r.run(to);
    let (_, p) = r.c.placings().next().expect("placed at the census");
    assert_eq!(p.amount, 64 * MILLI);
    assert_eq!(
        r.c.camp(r.f).unwrap().chest.map(|c| c.free),
        Some(26),
        "a slot less for the stack"
    );
    ok(&mut r);
}

/// RP-130 (the M9 merge): Java never copies a HAVEN_CHEST (no op id, no holder carries one), so a
/// copy, were one sent, names no chest, where the same record live names it.
#[test]
fn a_copy_of_a_haven_chest_names_no_chest() {
    let (mut r, _) = tribute_run(26);
    r.camp_out();
    let to = r.c.now() + 5;
    r.run(to);
    let (at, key) = (r.c.now(), r.key);
    r.c.ingest_copy(&mut r.w, &mut *r.infra, &chest_record(key, 27), at);
    assert!(r.c.camp(r.f).unwrap().chest.is_none(), "a copy is dropped");
    // The control: the same record live.
    chest(&mut r, 27);
    assert_eq!(r.c.camp(r.f).unwrap().chest.map(|c| c.free), Some(27));
    ok(&mut r);
}

/// MZB-446 (review P3-3): HAVEN_CHEST is journaled, so a crash that loses the tail of the session
/// (a payment, the chest, the stack the engine decided at the chest) replays both records, and
/// the stack decided again is the same op with the same amount: the plugin's DUPLICATE_OP answer
/// for the stack it put down before the crash books what it put down.
#[test]
fn a_stack_decided_again_after_a_crash_is_the_same_op_and_amount() {
    let (mut a, mut b, id) = twin_groves(24);
    let key = a.key;
    let t1 = a.c.now() + 100;
    let t2 = t1 + 50;
    let tail = [
        (t1, pay_record(key, id, 0x6101, 30)),
        (t2, chest_record(key, 27)),
    ];
    // The session: the stack goes at the chest.
    play(&mut a, &tail);
    let (op, p) = a.c.placings().next().expect("a stack");
    assert_eq!(p.amount, 30 * MILLI);
    // The crash: the save before the payment, the two records replayed. During the replay the camp
    // is out, so nothing is placed, but the chest is known again.
    b.restart_to_replay();
    play(&mut b, &tail);
    b.c.replay_done();
    assert_eq!(b.c.placings().count(), 0, "the camp is out in the replay");
    assert_eq!(owed(&b), 30);
    assert_eq!(
        b.c.camp(b.f).unwrap().chest.map(|c| c.free),
        Some(27),
        "the replayed record names the chest"
    );
    // The camp comes in: its census places the stack, to the chest the replay named.
    b.camp_in();
    let to = b.c.now() + 10;
    b.run(to);
    let (again, q) = b.c.placings().next().expect("the stack again");
    assert_eq!((again, q.amount, q.good), (op, p.amount, p.good));
    let at = b.c.now();
    assert!(
        b.c.tribute_placed(&mut b.w, again, at),
        "DUPLICATE_OP is an answer"
    );
    assert_eq!(owed(&b), 0, "it books what the plugin put down");
    ok(&mut b);
}

/// MZB-446 (review P3-3): a stack decided again from more than it was decided from (a second
/// payment, journaled after the stack went, replays before the camp comes in) is another decision,
/// so it is another op: the plugin puts it down, and is never told that the 30 nuggets it put down
/// before the crash were 60 (docs/boundary.md §2.2). What is left is the first stack's 30 in the
/// chest too many, a gift to the player and never a loss.
#[test]
fn a_stack_decided_again_from_more_is_another_op() {
    let (mut a, mut b, id) = twin_groves(25);
    let key = a.key;
    let t1 = a.c.now() + 100;
    let tail = [
        (t1, pay_record(key, id, 0x6102, 30)),
        (t1 + 50, chest_record(key, 27)),
        (t1 + 80, pay_record(key, id, 0x6103, 30)),
    ];
    play(&mut a, &tail);
    let (op, p) = a.c.placings().next().expect("a stack");
    assert_eq!(p.amount, 30 * MILLI, "decided before the second payment");
    assert_eq!(owed(&a), 60);
    restart_and_replay(&mut b, &tail);
    let (again, q) = b.c.placings().next().expect("the stack again");
    assert_eq!(q.amount, 60 * MILLI, "the replay owes both");
    assert_ne!(again, op, "another decision, another op");
    let at = b.c.now();
    assert!(b.c.tribute_placed(&mut b.w, again, at));
    assert_eq!(owed(&b), 0);
    ok(&mut b);
}

/// A record of the journal's tail for the tribute's crash tests: an observation, or Java's answer
/// to a stack (the COMMAND_RESULT rtsim-jni's `after_core` hands to `tribute_placed` or
/// `tribute_failed`).
enum TributeRec {
    Obs(Obs),
    Placed(u64),
    Failed(u64),
}

/// The session that goes on: each record at its game time, the world run to it first, and each
/// answer to the stack in flight. Returns the tail as the journal holds it.
fn play_tribute(
    r: &mut CampRun,
    steps: &[(Tick, Option<Obs>, Option<bool>)],
) -> Vec<(Tick, TributeRec)> {
    let mut tail = Vec::new();
    for (t, o, answer) in steps {
        r.run(*t);
        if let Some(o) = o {
            r.c.ingest(&mut r.w, &mut *r.infra, o, *t);
            tail.push((*t, TributeRec::Obs(o.clone())));
        }
        if let Some(placed) = answer {
            let (op, _) = r.c.placings().next().expect("a stack in flight to answer");
            if *placed {
                assert!(r.c.tribute_placed(&mut r.w, op, *t));
                tail.push((*t, TributeRec::Placed(op)));
            } else {
                assert!(r.c.tribute_failed(&mut r.w, op, *t));
                tail.push((*t, TributeRec::Failed(op)));
            }
        }
    }
    tail
}

/// The restart after a crash with the answers in the tail: the session starts, the tail is
/// replayed as the host replays it (each record at its game time, each answer through the
/// engine's own entry), and the replay is over. The camp is still out.
fn replay_tribute(r: &mut CampRun, tail: &[(Tick, TributeRec)]) {
    r.restart_to_replay();
    for (t, rec) in tail {
        r.run(*t);
        match rec {
            TributeRec::Obs(o) => r.c.ingest(&mut r.w, &mut *r.infra, o, *t),
            TributeRec::Placed(op) => {
                r.c.tribute_placed(&mut r.w, *op, *t);
            }
            TributeRec::Failed(op) => {
                r.c.tribute_failed(&mut r.w, *op, *t);
            }
        }
    }
    r.c.replay_done();
}

/// RP-134 (the M9 check, P3-1): a stack decided and answered APPLIED after the last save is booked
/// from its replayed answer, so the census after the replay decides the next stack, not this one
/// again from a stock that has moved. The session names the chest and takes 30 nuggets (a stack of
/// 30, answered), then 40 more (a stack of 40, in flight at the crash). The replay finds no
/// placing for the first answer and books it by its op; the census then decides the stack of 40 at
/// the next index, the op the session sent: Java answers it DUPLICATE_OP if it put it down, and
/// puts it down if not. Without the booking the census decides one stack of 64 from the 70 owed,
/// an op Java never saw, and the 30 go twice.
#[test]
fn a_stack_answered_after_the_save_is_booked_from_its_replayed_answer() {
    let (mut a, mut b, id) = twin_groves(28);
    let key = a.key;
    let t1 = a.c.now() + 100;
    let tail = play_tribute(
        &mut a,
        &[
            (t1, Some(chest_record(key, 27)), None),
            (t1 + 50, Some(pay_record(key, id, 0x6301, 30)), None),
            (t1 + 60, None, Some(true)),
            (t1 + 100, Some(pay_record(key, id, 0x6302, 40)), None),
        ],
    );
    let (op2, p2) = a.c.placings().next().expect("the second stack in flight");
    assert_eq!(p2.amount, 40 * MILLI);
    assert_eq!(a.c.camp(a.f).unwrap().stacks, 2);
    let TributeRec::Placed(op1) = tail[2].1 else {
        panic!("the first stack's answer")
    };
    // Live, an answer to an op no placing holds is no tribute's.
    let at = a.c.now();
    assert!(
        !a.c.tribute_placed(&mut a.w, op1, at),
        "live, a spent op books nothing"
    );
    replay_tribute(&mut b, &tail);
    assert_eq!(b.c.placings().count(), 0, "the camp is out in the replay");
    assert_eq!(owed(&b), 40, "the first stack is booked from its answer");
    assert_eq!(b.c.camp(b.f).unwrap().stacks, 1, "the count passes it");
    assert_eq!(b.c.stats.tribute_stacks, a.c.stats.tribute_stacks);
    b.camp_in();
    let to = b.c.now() + 10;
    b.run(to);
    let (again, q) = b.c.placings().next().expect("the next stack");
    assert_eq!(
        (again, q.amount),
        (op2, p2.amount),
        "the stack the session sent, not the first again"
    );
    let at = b.c.now();
    assert!(
        b.c.tribute_placed(&mut b.w, again, at),
        "DUPLICATE_OP is an answer"
    );
    assert_eq!(owed(&b), 0);
    assert_eq!(b.c.placings().count(), 0, "nothing goes twice");
    ok(&mut b);
}

/// RP-134: a replayed FAILED for a stack decided after the save passes its index and forgets the
/// chest, as the session did. The session's first stack of 30 fails, the plugin names the chest
/// again, the stack goes again at the next index and is answered APPLIED. The replay books both
/// answers, so the census places nothing: without them it decides the first index again, an op Java
/// answered FAILED and so puts down, on top of the second.
#[test]
fn a_replayed_failure_of_a_stack_after_the_save_passes_its_index() {
    let (mut a, mut b, id) = twin_groves(29);
    let key = a.key;
    let t1 = a.c.now() + 100;
    let tail = play_tribute(
        &mut a,
        &[
            (t1, Some(chest_record(key, 27)), None),
            (t1 + 50, Some(pay_record(key, id, 0x6401, 30)), None),
            (t1 + 60, None, Some(false)),
            (t1 + 100, Some(chest_record(key, 27)), None),
            (t1 + 110, None, Some(true)),
        ],
    );
    assert_eq!(owed(&a), 0);
    assert_eq!(a.c.camp(a.f).unwrap().stacks, 2);
    replay_tribute(&mut b, &tail);
    assert_eq!(owed(&b), 0, "the second stack is booked from its answer");
    assert_eq!(b.c.camp(b.f).unwrap().stacks, 2, "both indexes passed");
    b.camp_in();
    let to = b.c.now() + 10;
    b.run(to);
    assert_eq!(b.c.placings().count(), 0, "nothing goes again");
    ok(&mut b);
}

/// MZB-446 (amended at the re-check, and by RP-134): a crash after several answered stacks. A
/// session names the chest and then takes three payments (30, 50, 40), each stack put down and
/// answered before the next payment: 120 nuggets are in the chest and nothing is owed. The pages
/// are not saved after them, and then the server dies. The replay books the three payments and
/// the chest, and each stack from its replayed answer (RP-134), so the census places nothing: the
/// 120 are in the chest once, the books are right, and nothing is lost. Before RP-134 the replay
/// found no placing for the answers, the census decided stacks of 64 and 56 from the 120 owed, ops
/// the plugin never saw, and every stack answered since the save went twice.
#[test]
fn a_crash_after_several_answered_stacks_books_them_and_gifts_nothing() {
    let (mut a, mut b, id) = twin_groves(27);
    let key = a.key;
    let t1 = a.c.now() + 100;
    let tail = play_tribute(
        &mut a,
        &[
            (t1, Some(chest_record(key, 27)), None),
            (t1 + 50, Some(pay_record(key, id, 0x6201, 30)), Some(true)),
            (t1 + 100, Some(pay_record(key, id, 0x6202, 50)), Some(true)),
            (t1 + 150, Some(pay_record(key, id, 0x6203, 40)), Some(true)),
        ],
    );
    let answered = tail
        .iter()
        .filter(|e| matches!(e.1, TributeRec::Placed(_)))
        .count();
    assert_eq!(answered, 3, "a stack for each payment");
    assert_eq!(owed(&a), 0);
    replay_tribute(&mut b, &tail);
    assert_eq!(owed(&b), 0, "the books are right");
    assert_eq!(b.c.camp(b.f).unwrap().stacks, 3);
    b.camp_in();
    let to = b.c.now() + 10;
    b.run(to);
    assert_eq!(b.c.placings().count(), 0, "no stack goes twice");
    assert_eq!(b.c.stats.tribute_stacks, a.c.stats.tribute_stacks);
    ok(&mut b);
}

/// Review P3-6: the stack in flight is resident, so the engine's memory counts it.
#[test]
fn a_stack_in_flight_is_counted_in_the_engines_memory() {
    let (mut r, _) = tribute_run(40);
    assert_eq!(r.c.memory()["tribute stacks"], 0, "no chest, no stack");
    chest(&mut r, 27);
    assert_eq!(r.c.placings().count(), 1);
    assert!(r.c.memory()["tribute stacks"] > 0, "the stack is counted");
    let (op, _) = r.c.placings().next().unwrap();
    let at = r.c.now();
    assert!(r.c.tribute_placed(&mut r.w, op, at));
}

// ---- owed hunts ---------------------------------------------------------------------------------

/// Walls that hold no one back: every raid on a district is survived whole.
struct Open;

impl Infrastructure for Open {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        Some(RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: false,
            stolen: GoodVec::new(),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }
}

/// The hunts the grove owes: (district, count, op).
fn hunts(r: &CampRun) -> Vec<(u64, u32, u64)> {
    r.c.factions[r.f as usize]
        .hunts()
        .iter()
        .map(|h| (h.district.0, h.count, h.op))
        .collect()
}

/// Every OWED_HUNT the engine sent.
fn hunt_cmds(r: &CampRun) -> Vec<boundary::OwedHunt> {
    r.cmds
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::OwedHunt(h) => Some(*h),
            _ => None,
        })
        .collect()
}

fn owe(r: &mut CampRun, district: u64, count: u32) {
    let at = r.c.now();
    r.c.owe_hunt(
        r.f,
        rtsim_industry::ids::DistrictId(district),
        REFINERY_POS,
        count,
        at,
    );
    r.flush(at);
}

fn applied(r: &mut CampRun, district: u64, op: u64, killed: u32) {
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::HuntApplied(boundary::HuntApplied {
            faction: r.key,
            district,
            op,
            killed,
        }),
        at,
    );
    r.flush(at);
}

/// MZB-424, MZB-425: a raid on a district nobody watches, survived, leaves the hunts its
/// survivors stand for (the raptor's 2 a head) owed in that district, as an OWED_HUNT the plugin
/// answers with HUNT_APPLIED whatever it killed; the answer spends it.
#[test]
fn a_raid_on_an_unwatched_district_leaves_its_raptors_hunts_owed() {
    let mut r = grove(30, 9);
    r.infra = Box::new(Open);
    come_in(&mut r);
    r.raid().expect("a raid: three warriors and a raptor");
    r.run(Tick(days(4.0)));
    assert_eq!(r.c.stats.raids_district, 1);
    let owed = hunts(&r);
    assert_eq!(owed.len(), 1, "{owed:?}");
    assert_eq!(
        (owed[0].0, owed[0].1),
        (synth::REFINERY.0, 2),
        "one raptor, two animals"
    );
    let sent = hunt_cmds(&r);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].district, sent[0].count, sent[0].op),
        (synth::REFINERY.0, 2, owed[0].2)
    );
    assert_eq!(sent[0].centre, (REFINERY_POS.x, REFINERY_POS.z));
    assert_eq!(sent[0].radius, HUNT_RADIUS);
    assert_eq!(sent[0].dimension, u32::from(REFINERY_POS.frame));
    assert_eq!(r.plugin.hunts.len(), 1, "the plugin has it");
    // The plugin kills one of the two (no more to be found) and says so.
    let at = r.c.now() + 1;
    r.plugin.apply_hunts(1, at);
    r.run(at + 5);
    assert!(hunts(&r).is_empty(), "spent, whatever it killed");
    assert_eq!(r.c.stats.hunts_killed, 1);
    ok(&mut r);
}

/// MZB-449: a hunt owed again in a district before the plugin answered adds to the one in flight
/// up to the culture's `hunt_cap` (8) and replaces it with a new op, keeping the replaced op and
/// `rest`, what the second raid owes on its own; the replaced op's late answer leaves the hunt at
/// `rest` (the re-check: not the capped sum less the 6, which would owe the second raid 2) and
/// sends it again; another district has its own hunt.
#[test]
fn a_hunt_owed_again_sums_to_the_cap_with_a_new_op_and_the_old_ones_late_answer_leaves_the_second_raids_own_count()
 {
    let mut r = grove(31, 9);
    owe(&mut r, 5, 6);
    let first = hunts(&r)[0];
    assert_eq!((first.0, first.1), (5, 6));
    owe(&mut r, 5, 6);
    let second = hunts(&r);
    assert_eq!(second.len(), 1);
    assert_eq!((second[0].0, second[0].1), (5, 8), "capped at 8");
    assert_ne!(second[0].2, first.2, "a new op");
    owe(&mut r, 6, 3);
    assert_eq!(hunts(&r).len(), 2, "another district, another hunt");
    let sent: Vec<_> = hunt_cmds(&r)
        .iter()
        .map(|h| (h.district, h.count, h.replaced, h.rest))
        .collect();
    assert_eq!(
        sent,
        [(5, 6, 0, 0), (5, 8, first.2, 6), (6, 3, 0, 0)],
        "the replacement names the op it replaced and the 6 the second raid owes alone"
    );
    // The first op's answer is late: the 6 it owed are not owed again, and the second raid keeps
    // its own 6 (the cap clipped the sum, not the raid).
    applied(&mut r, 5, first.2, 6);
    assert_eq!(
        hunts(&r).iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(),
        [(5, 6), (6, 3)],
        "the second raid's own 6, and the other district untouched"
    );
    assert_eq!(r.c.stats.hunts_killed, 6);
    let again = hunt_cmds(&r).pop().expect("the rest is sent again");
    assert_eq!(
        (
            again.district,
            again.count,
            again.op,
            again.replaced,
            again.rest
        ),
        (5, 6, second[0].2, 0, 0),
        "the held hunt, under its op, with what is left and no replaced op"
    );
    // A copy of that answer finds the replaced op gone: nothing comes off twice.
    applied(&mut r, 5, first.2, 6);
    assert_eq!(hunts(&r).iter().map(|h| h.1).collect::<Vec<_>>(), [6, 3]);
    assert_eq!(r.c.stats.hunts_killed, 6);
    // The second's clears its district only, once.
    applied(&mut r, 5, second[0].2, 6);
    assert_eq!(hunts(&r).iter().map(|h| h.0).collect::<Vec<_>>(), [6]);
    assert_eq!(r.c.stats.hunts_killed, 12);
    applied(&mut r, 5, second[0].2, 6);
    assert_eq!(r.c.stats.hunts_killed, 12, "once");
    ok(&mut r);
}

/// The animals a plugin would kill for the hunts it keeps, each as many as it was told.
fn pending_kills(r: &CampRun) -> u32 {
    r.plugin.hunts.iter().map(|h| h.count).sum()
}

/// MZB-449, review P3-8: the plugin applies the first hunt (5) before it reads the second raid's
/// (3, so 8 in one hunt); the first's HUNT_APPLIED is still on its way when the second is owed.
/// Before, the 8 were paid on top of the 5. Now the late answer takes the 5 off the hunt in flight
/// and sends it again with 3, which the plugin keeps in place of the 8: 5 and 3 animals die, the
/// 8 that were owed. The replaced op survives a save.
#[test]
fn a_hunt_applied_while_a_second_raid_replaces_it_is_not_paid_twice() {
    let mut r = grove(34, 9);
    owe(&mut r, 5, 5);
    let op1 = hunts(&r)[0].2;
    // The plugin applies it: 5 die, the answer is two ticks away.
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    assert_eq!(r.plugin.animals_killed, 5);
    // The second raid is owed before the answer arrives.
    owe(&mut r, 5, 3);
    let [(_, held, op2)] = hunts(&r)[..] else {
        panic!("one hunt: {:?}", hunts(&r));
    };
    assert_eq!(held, 8);
    assert_ne!(op1, op2);
    assert_eq!(pending_kills(&r), 8, "the plugin holds the 8");
    // A save and load in between loses nothing of the replaced hunt.
    r.round_trip();
    assert_eq!(hunts(&r), [(5, 8, op2)]);
    assert_eq!(
        r.c.factions[r.f as usize].hunts()[0].replaced,
        Some((op1, 3)),
        "the replaced op and the 3 the second raid owes alone"
    );
    r.run(at + 5);
    assert_eq!(hunts(&r), [(5, 3, op2)], "the 5 are paid");
    assert_eq!(
        pending_kills(&r),
        3,
        "the plugin keeps the hunt as sent again, in place of the 8"
    );
    // The plugin applies what it holds; its answer spends the hunt.
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    r.run(at + 5);
    assert!(hunts(&r).is_empty(), "spent");
    assert_eq!(
        r.plugin.animals_killed, 8,
        "the animals that die are the animals owed"
    );
    assert_eq!(r.c.stats.hunts_killed, 8, "and so are the animals booked");
    ok(&mut r);
}

/// The race of the test above in its common ordering (the re-check): the plugin applied the first
/// hunt (5) because the circle's chunks tick entities, and they still tick when the second raid's
/// OWED_HUNT arrives, so it applies the second at once, before the engine has read the first's
/// answer. The hunt carries the op it replaced and `rest`, so the plugin, whose last applied op
/// is the replaced one, kills the 3 the second raid owes, not the 8. The engine then reads the
/// first's late answer and sends the second again with 3, which the plugin ignores: it applied
/// that op. 5 and 3 die for the 8 owed.
#[test]
fn a_replacement_applied_before_the_late_answer_is_read_kills_only_its_own_raids_animals() {
    let mut r = grove(34, 9);
    owe(&mut r, 5, 5);
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    owe(&mut r, 5, 3);
    let [(_, 8, op2)] = hunts(&r)[..] else {
        panic!("one hunt of 8: {:?}", hunts(&r));
    };
    // The circle still ticks: op2 is applied as soon as it is read, its answer one tick behind
    // op1's.
    r.plugin.apply_hunts(99, at + 1);
    assert_eq!(r.plugin.animals_killed, 8, "5 and the 3 of the second raid");
    r.run(at + 5);
    assert!(hunts(&r).is_empty(), "spent: {:?}", hunts(&r));
    assert_eq!(
        r.plugin.hunts,
        [],
        "the hunt sent again is ignored by the plugin (it applied that op), and nothing is held"
    );
    let sent = hunt_cmds(&r);
    assert_eq!(
        sent.last().map(|h| (h.op, h.count, h.replaced)),
        Some((op2, 3, 0)),
        "the engine did send the rest again, after op1's answer"
    );
    assert_eq!(
        r.plugin.animals_killed, 8,
        "the animals killed for the 8 owed"
    );
    assert_eq!(
        r.c.stats.hunts_killed, 8,
        "and the engine's books say the same"
    );
    ok(&mut r);
}

/// The same race where the first hunt was the whole cap: the second raid still owes its own 3, so
/// the hunt is not cleared by the late answer. (Before the re-check the late answer left nothing
/// owed, as the cap had clipped the second raid away.)
#[test]
fn a_hunt_the_plugin_already_applied_whole_leaves_the_second_raid_its_own_count() {
    let mut r = grove(35, 9);
    owe(&mut r, 5, 8);
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    owe(&mut r, 5, 3);
    assert_eq!(
        pending_kills(&r),
        8,
        "the plugin holds the second hunt, capped"
    );
    r.run(at + 5);
    assert_eq!(hunts(&r).iter().map(|h| h.1).collect::<Vec<_>>(), [3]);
    assert_eq!(pending_kills(&r), 3, "the hunt as sent again");
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    assert_eq!(r.plugin.animals_killed, 11, "8 and 3, each raid its own");
    assert_eq!(r.c.stats.hunts_killed, 11);
    ok(&mut r);
}

/// A replacing raid that owes nothing of its own (a count of 0) is the hunt cancelled once the
/// plugin's answer for the replaced hunt arrives: the held hunt is cleared and sent as a count of
/// 0, which the plugin applies as a hunt that kills none (MZB-426) and answers all the same.
#[test]
fn a_replacement_owing_nothing_cancels_the_hunt_the_plugin_holds() {
    let mut r = grove(39, 9);
    owe(&mut r, 5, 5);
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    owe(&mut r, 5, 0);
    assert_eq!(pending_kills(&r), 5, "the plugin holds a hunt of 5");
    r.run(at + 5);
    assert!(hunts(&r).is_empty(), "nothing is owed after the 5 died");
    assert_eq!(hunt_cmds(&r).last().map(|h| h.count), Some(0), "cancelled");
    assert_eq!(pending_kills(&r), 0, "it kills none");
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    assert_eq!(r.plugin.animals_killed, 5, "the animals owed, no more");
    assert_eq!(r.c.stats.hunts_killed, 5);
    ok(&mut r);
}

/// One step of a hunt ordering against the plugin's rule.
#[derive(Clone, Copy, Debug)]
enum Hs {
    /// A raid owes this many animals in district 5.
    Owe(u32),
    /// The plugin applies the hunt it holds, finding every animal; its answer is journaled.
    Apply,
    /// The journaled answers reach the engine.
    Read,
}

/// The steps played on a fresh grove, a save and load inserted before step `save` (none past the
/// end): the animals killed in the world, those the engine booked and what it still owes.
fn play_hunts(seed: u64, steps: &[Hs], save: usize) -> (u32, u64, usize) {
    let mut r = grove(seed, 9);
    let mut applies = 0;
    for (i, s) in steps.iter().chain(&[Hs::Read]).enumerate() {
        if i == save {
            r.round_trip();
        }
        match *s {
            Hs::Owe(n) => owe(&mut r, 5, n),
            Hs::Apply => {
                applies += 1;
                let at = r.c.now() + 1 + applies;
                r.plugin.apply_hunts(99, at);
            }
            Hs::Read => {
                let to = r.c.now() + 10;
                r.run(to);
            }
        }
    }
    ok(&mut r);
    (
        r.plugin.animals_killed,
        r.c.stats.hunts_killed,
        hunts(&r).len(),
    )
}

/// MZB-449 (the re-check): every ordering of two raids against the plugin's rule (it keeps the
/// last op it applied, ignores a repeat of it, and kills `rest` for a hunt that replaced it)
/// kills the animals owed: each raid its own count, capped as a lone hunt would be, or the
/// capped sum when the plugin never applied the first. A save and load between any two steps
/// changes nothing.
#[test]
fn every_ordering_of_two_raids_kills_the_animals_owed() {
    use Hs::{Apply, Owe, Read};
    #[rustfmt::skip]
    let table: [(&str, &[Hs], u32); 12] = [
        ("answer read before the second raid", &[Owe(5), Apply, Read, Owe(3), Apply, Read], 8),
        ("second applied before the late answer", &[Owe(5), Apply, Owe(3), Apply, Read], 8),
        ("second applied after the late answer", &[Owe(5), Apply, Owe(3), Read, Apply, Read], 8),
        ("first never applied", &[Owe(5), Owe(3), Apply, Read], 8),
        ("capped pair, answer read first", &[Owe(6), Apply, Read, Owe(6), Apply, Read], 12),
        ("capped pair, second applied at once", &[Owe(6), Apply, Owe(6), Apply, Read], 12),
        ("capped pair, second applied after", &[Owe(6), Apply, Owe(6), Read, Apply, Read], 12),
        ("capped pair, first never applied", &[Owe(6), Owe(6), Apply, Read], 8),
        ("whole cap, second applied at once", &[Owe(8), Apply, Owe(3), Apply, Read], 11),
        ("whole cap, second applied after", &[Owe(8), Apply, Owe(3), Read, Apply, Read], 11),
        ("applied, answered, a third raid later", &[Owe(2), Apply, Read, Owe(4), Owe(1), Apply, Read], 5 + 2),
        ("answered, a second raid, applied later", &[Owe(5), Apply, Read, Owe(5), Read, Apply, Read], 10),
    ];
    for (n, (name, steps, owed)) in table.iter().enumerate() {
        for save in 0..=steps.len() + 1 {
            let (killed, booked, held) = play_hunts(40 + n as u64, steps, save);
            let at = if save <= steps.len() {
                format!("a save before step {save}")
            } else {
                "no save".to_string()
            };
            assert_eq!(killed, *owed, "{name}, {at}: animals killed in the world");
            assert_eq!(booked, u64::from(*owed), "{name}, {at}: animals booked");
            assert_eq!(held, 0, "{name}, {at}: nothing is left owed");
        }
    }
}

/// MZB-449's named residual: a third raid inside the flight of one answer, with the plugin having
/// skipped the second hunt, forgets the second's replaced op, so the first hunt's late answer
/// finds no hunt and the third's hunt (capped) is paid on top of the first's animals. The overpay
/// is the cap at most.
#[test]
fn a_third_raid_inside_one_answers_flight_overpays_by_at_most_the_cap() {
    let mut r = grove(52, 9);
    owe(&mut r, 5, 5);
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    // The plugin has not read the second hunt when the third replaces it.
    owe(&mut r, 5, 3);
    owe(&mut r, 5, 2);
    r.run(at + 5);
    let at = r.c.now() + 2;
    r.plugin.apply_hunts(99, at);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    let owed = 5 + 3 + 2;
    let killed = r.plugin.animals_killed;
    assert!(
        killed >= owed && killed - owed <= 8,
        "{killed} against {owed}"
    );
    assert_eq!(
        killed,
        5 + 8,
        "the first's 5, then the capped sum of the three"
    );
    ok(&mut r);
}

/// `stats.hunts_owed` counts what a raid adds after the cap, not a replacement's whole sum
/// again: 5 then 3 more are 8 animals owed, not 13; 6 then 6 more add the 2 the cap leaves.
#[test]
fn recheck_hunts_owed_counts_a_replacement_once() {
    let mut r = grove(38, 9);
    owe(&mut r, 5, 5);
    owe(&mut r, 5, 3);
    assert_eq!(hunts(&r).len(), 1);
    assert_eq!(r.c.stats.hunts_owed, 8, "5 and 3");
    let mut q = grove(38, 9);
    owe(&mut q, 5, 6);
    owe(&mut q, 5, 6);
    assert_eq!(q.c.stats.hunts_owed, 8, "6, then the 2 the cap leaves");
}

/// MZB-449, review P3-6 and the coordinator's rule: an answer clears the hunt it names by its op
/// id and no other. The first hunt is applied and booked; a later raid makes another the held
/// hunt; a late copy of the first's answer, live and after a save and load, changes nothing: the
/// held hunt keeps its whole count and goes to the plugin again, whole, at a session start.
#[test]
fn a_late_copy_of_a_booked_hunts_answer_leaves_the_hunt_now_held_alone() {
    let mut r = grove(36, 9);
    owe(&mut r, 5, 2);
    let op1 = hunts(&r)[0].2;
    applied(&mut r, 5, op1, 2);
    assert!(hunts(&r).is_empty(), "booked");
    assert_eq!(r.c.stats.hunts_killed, 2);
    // A raid a day on: another hunt in the district, another op.
    let to = r.c.now() + TICKS_PER_DAY;
    r.run(to);
    owe(&mut r, 5, 4);
    let op2 = hunts(&r)[0].2;
    assert_ne!(op1, op2);
    // The copy, live.
    let sent = hunt_cmds(&r).len();
    applied(&mut r, 5, op1, 2);
    assert_eq!(hunts(&r), [(5, 4, op2)], "the whole count, under its op");
    assert_eq!(r.c.stats.hunts_killed, 2, "booked once");
    assert_eq!(hunt_cmds(&r).len(), sent, "and nothing is sent again");
    // Saved and loaded, and the copy again (an answer in the journal's tail replays so).
    r.round_trip();
    applied(&mut r, 5, op1, 2);
    assert_eq!(hunts(&r), [(5, 4, op2)], "after a save and load");
    assert_eq!(r.c.stats.hunts_killed, 2);
    // The next session: the replay brings the copy again, and the hunt goes whole.
    r.restart_to_replay();
    applied(&mut r, 5, op1, 2);
    r.c.resend_notices();
    let at = r.c.now();
    r.flush(at);
    let spawn = hunt_cmds(&r).pop().expect("the hunt again");
    assert_eq!(
        (spawn.district, spawn.count, spawn.op),
        (5, 4, op2),
        "its spawn goes out, the whole count"
    );
    r.plugin.apply_hunts(99, at + 1);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    assert_eq!(r.c.stats.hunts_killed, 6, "2 and 4");
    ok(&mut r);
}

/// MZB-424 (what owes none): nothing is owed on Peaceful, nor by a culture with no `hunt_cap`.
#[test]
fn nothing_is_owed_on_peaceful_or_by_a_culture_without_a_cap() {
    let mut r = grove(32, 9);
    r.c.peaceful = true;
    owe(&mut r, 5, 3);
    assert!(hunts(&r).is_empty() && hunt_cmds(&r).is_empty(), "Peaceful");
    let mut g = CampRun::new(32, "goblin", 3);
    owe(&mut g, 5, 3);
    assert!(hunts(&g).is_empty() && hunt_cmds(&g).is_empty(), "goblins");
}

/// MZB-425 (saves and sessions): an owed hunt is saved; at a new session it is written again,
/// with the same op, once the replay has caught up (`resend_notices`), and answered, it is spent.
#[test]
fn an_owed_hunt_is_saved_and_written_again_with_its_op_at_a_session_start() {
    let mut r = grove(33, 9);
    owe(&mut r, 5, 3);
    owe(&mut r, 6, 2);
    let before = hunts(&r);
    r.round_trip();
    assert_eq!(hunts(&r), before, "saved");
    let sent = hunt_cmds(&r).len();
    r.restart_to_replay();
    r.c.resend_notices();
    let at = r.c.now();
    r.flush(at);
    let again: Vec<_> = hunt_cmds(&r).split_off(sent);
    assert_eq!(
        again
            .iter()
            .map(|h| (h.district, h.count, h.op))
            .collect::<Vec<_>>(),
        before,
        "each hunt again, under its op"
    );
    r.plugin.apply_hunts(2, at + 1);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    ok(&mut r);
}

/// The crash: the engine as the pages `saved` hold it, the next session started with its replay
/// open (the plugin forgets what it held and applied, `restart_to_replay`).
fn crash_to(r: &mut CampRun, saved: &[u8]) {
    r.c = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), saved).expect("loads");
    r.restart_to_replay();
}

/// The hunts the plugin was sent so far, as (op, count).
fn hunts_sent(r: &CampRun) -> Vec<(u64, u32)> {
    r.plugin
        .hunts_sent
        .iter()
        .map(|h| (h.op, h.count))
        .collect()
}

/// MZB-449 (the re-check, R1): a late answer replayed after a crash changes the books and sends
/// nothing. The host moves every command out of the engine's outbox after each ingest and writes
/// its queue at each hand-over, so a hunt pushed while replaying reached the next session's
/// plugin, which had forgotten that it killed for it: the animals died twice. Here the first
/// hunt's late answer and the second's are both journaled after the save; the replay books both,
/// the hunt is spent, and the plugin of the next session hears of none.
#[test]
fn a_replayed_late_answer_sends_no_spent_hunt_to_the_next_session() {
    let mut r = grove(41, 9);
    owe(&mut r, 5, 5);
    let op1 = hunts(&r)[0].2;
    owe(&mut r, 5, 3);
    let [(_, 8, op2)] = hunts(&r)[..] else {
        panic!("{:?}", hunts(&r));
    };
    let saved = r.c.save();
    // The session goes on: the plugin killed 5, then 3.
    applied(&mut r, 5, op1, 5);
    applied(&mut r, 5, op2, 3);
    assert!(hunts(&r).is_empty());
    crash_to(&mut r, &saved);
    let before = hunts_sent(&r);
    assert_eq!(hunts(&r), [(5, 8, op2)], "the pages");
    // The replay: each ingest is followed by the host's take.
    applied(&mut r, 5, 0x1234, 0);
    applied(&mut r, 5, op1, 5);
    assert_eq!(hunts(&r), [(5, 3, op2)], "8 less the 5 killed");
    assert_eq!(hunts_sent(&r), before, "nothing is sent while replaying");
    applied(&mut r, 5, op2, 3);
    assert!(hunts(&r).is_empty(), "spent");
    r.c.resend_notices();
    let at = r.c.now();
    r.flush(at);
    assert_eq!(hunts_sent(&r), before, "a spent hunt goes to no plugin");
    assert!(r.plugin.hunts.is_empty());
    assert_eq!(r.c.stats.hunts_killed, 8);
    ok(&mut r);
}

/// The same crash with the second hunt's own answer lost: the replay books the late answer of the
/// first, which leaves 3 owed, and `resend_notices` sends what is held, once, whole, with no
/// replaced op (the new plugin applied nothing).
#[test]
fn a_hunt_left_owed_by_a_replayed_late_answer_goes_once_when_the_replay_is_over() {
    let mut r = grove(42, 9);
    owe(&mut r, 5, 5);
    let op1 = hunts(&r)[0].2;
    owe(&mut r, 5, 3);
    let [(_, 8, op2)] = hunts(&r)[..] else {
        panic!("{:?}", hunts(&r));
    };
    let saved = r.c.save();
    applied(&mut r, 5, op1, 5);
    crash_to(&mut r, &saved);
    let before = hunts_sent(&r).len();
    applied(&mut r, 5, op1, 5);
    assert_eq!(hunts_sent(&r).len(), before, "nothing while replaying");
    assert_eq!(hunts(&r), [(5, 3, op2)]);
    r.c.resend_notices();
    let at = r.c.now();
    r.flush(at);
    let sent: Vec<_> = r.plugin.hunts_sent.split_off(before);
    assert_eq!(
        sent.iter()
            .map(|h| (h.op, h.count, h.replaced, h.rest))
            .collect::<Vec<_>>(),
        [(op2, 3, 0, 0)],
        "the hunt held, once"
    );
    // The new plugin kills them and answers; the hunt is spent.
    r.plugin.apply_hunts(99, at + 1);
    r.run(at + 5);
    assert!(hunts(&r).is_empty());
    assert_eq!(r.plugin.animals_killed, 3);
    ok(&mut r);
}

/// A raid re-derived by the replay owes its hunt in the books and sends nothing; the hunt goes
/// once, with the others held, when the replay is over.
#[test]
fn a_hunt_owed_while_replaying_goes_once_when_the_replay_is_over() {
    let mut r = grove(43, 9);
    let saved = r.c.save();
    crash_to(&mut r, &saved);
    let before = hunts_sent(&r).len();
    owe(&mut r, 5, 4);
    owe(&mut r, 6, 2);
    assert_eq!(hunts(&r).len(), 2);
    assert_eq!(r.plugin.hunts_sent.len(), before, "nothing while replaying");
    r.c.resend_notices();
    let at = r.c.now();
    r.flush(at);
    let sent: Vec<_> = r.plugin.hunts_sent.split_off(before);
    let mut got: Vec<_> = sent.iter().map(|h| (h.district, h.count)).collect();
    got.sort_unstable();
    assert_eq!(got, [(5, 4), (6, 2)], "each, once");
    ok(&mut r);
}

// ---- plunder ------------------------------------------------------------------------------------

fn taken(c: &Conflict, g: GoodId) -> i128 {
    c.ledger.from_villages.get(&g).copied().unwrap_or(0)
}

/// MZB-427: a grove's plunder band takes its `plunder_goods` (leather and wool) and nothing else,
/// whatever it carries (a raptor carries nothing), and eats what it takes where it stands: the
/// goods are booked destroyed, never landed, and the band brings nothing home.
#[test]
fn a_groves_plunder_takes_livestock_only_and_eats_it() {
    let mut raided = 0;
    for seed in 1..=8u64 {
        let (mut w, _) = synth::world(seed);
        let mut c = synth::conflict(seed, &w);
        // The villages put up no defence, so the band lives to take what it can.
        c.tun.village_defence_per_pop = 0.0;
        for k in &mut c.cultures.list {
            k.def.plunder_mean_days = None;
        }
        c.add_faction(
            &mut w,
            synth::CAMP,
            "umvuthana",
            synth::CAMP_POS,
            Tick::ZERO,
        )
        .expect("the grove");
        let grove_culture = c
            .cultures
            .get(c.cultures.index_of("umvuthana").unwrap())
            .clone();
        let mut prey: Vec<usize> = (0..w.nodes.len())
            .filter(|&i| {
                matches!(w.nodes[i].kind, NodeKind::Village(cu) if grove_culture.hates(cu.name()))
            })
            .collect();
        prey.sort_by_key(|&i| {
            let p = w.nodes[i].pos;
            let (dx, dz) = (
                i64::from(p.x - synth::CAMP_POS.x),
                i64::from(p.z - synth::CAMP_POS.z),
            );
            (dx * dx + dz * dz, i)
        });
        let i = prey[0];
        let p = c
            .order_party(
                &mut w,
                synth::CAMP,
                PartyKind::Plunder,
                Target::Node(i),
                None,
                Tick::from_days(1),
            )
            .expect("a plunder band");
        // To its arrival.
        let at = loop {
            let x = &c.parties[p];
            if x.phase == Phase::Out && x.seg as usize + 2 >= x.route.len() {
                let at = x.times[x.seg as usize + 1];
                w.run_until(at);
                break at;
            }
            let next = c.next_due().expect("the band's next event");
            c.run_until(&mut w, &mut rtsim_ecology::infra::NoInfrastructure, next);
        };
        // Every good stands 100 items over its target.
        let goods: Vec<GoodId> = w.goods.ids().collect();
        let m = w.market(i);
        let rich: BTreeMap<GoodId, Amount> = goods
            .iter()
            .map(|&g| (g, m.line(g).target + 100 * MILLI - m.stock(g)))
            .filter(|e| e.1 > 0)
            .collect();
        w.stock_from_players(i, &rich, at);
        let leather = w.goods.find("leather").unwrap();
        let wool = w.goods.find("wool").unwrap();
        let m = w.market(i);
        let share = grove_culture.def.loot_share_milli;
        let reach: Amount = [leather, wool]
            .iter()
            .map(|&g| (m.stock(g) - m.line(g).target).max(0) * share / 1000)
            .sum();
        let before: Vec<i128> = goods.iter().map(|&g| taken(&c, g)).collect();
        let landed = c.ledger.landed.clone();
        let raids = c.stats.raids_village;
        c.run_until(&mut w, &mut rtsim_ecology::infra::NoInfrastructure, at);
        if c.stats.raids_village == raids {
            continue;
        }
        raided += 1;
        for (k, &g) in goods.iter().enumerate() {
            let got = taken(&c, g) - before[k];
            if g == leather || g == wool {
                assert!(got >= 0, "seed {seed}");
            } else {
                assert_eq!(got, 0, "seed {seed}: good {k} is no livestock");
            }
        }
        let items = (taken(&c, leather) + taken(&c, wool))
            - (before[leather.index()] + before[wool.index()]);
        assert!(items > 0, "seed {seed}: livestock was taken");
        assert!(
            items <= i128::from(reach),
            "seed {seed}: {items} of at most {reach}"
        );
        assert_eq!(
            c.ledger
                .destroyed
                .keys()
                .copied()
                .collect::<Vec<_>>()
                .iter()
                .filter(|g| ![leather, wool].contains(g))
                .count(),
            0,
            "only livestock is eaten"
        );
        assert!(c.ledger.destroyed.values().sum::<i128>() >= items);
        assert_eq!(c.ledger.landed, landed, "nothing lands at the haven");
        assert!(c.parties[p].loot.is_empty(), "nothing carried");
        while c.parties.get(p).is_some() {
            let next = c.next_due().expect("the band's next event");
            c.run_until(&mut w, &mut rtsim_ecology::infra::NoInfrastructure, next);
        }
        w.run_until(c.now());
        assert!(c.check(&w).is_ok(), "seed {seed}: {:?}", c.check(&w));
        let _: Option<Handle<Party>> = None;
    }
    assert!(raided >= 4, "{raided} raids of 8");
}

/// MZB-423, review P3-2: the grove's stock drains with the world's clock, and a chest named at a
/// later tick than the world's is read at that tick. Six nuggets are left; two days on, at the
/// stock's draw of about two a day, two and a few hundredths are. The stack placed is the two whole
/// nuggets that are there, and the rest stays in the stock instead of leaving the books with no
/// stack to show for it.
#[test]
fn a_stack_is_read_at_its_own_tick_and_loses_no_goods_to_a_stock_that_drained() {
    let (mut r, nugget) = tribute_run(21);
    let node = r.c.factions[r.f as usize].node;
    let t0 = r.c.now();
    let held = stock(&r, nugget);
    let drain = BTreeMap::from([(nugget, held - 6 * MILLI)]);
    r.w.stock_to_players(node, &drain, t0);
    assert_eq!(stock(&r, nugget), 6 * MILLI);
    chest_at(&mut r, 27, t0 + days(2.0));
    let placed: Vec<_> = r.c.placings().collect();
    assert_eq!(placed.len(), 1, "one stack: {placed:?}");
    assert_eq!(placed[0].1.amount, 2 * MILLI, "the two whole nuggets");
    let left = stock(&r, nugget);
    assert!(
        (0..MILLI).contains(&left) && left > 0,
        "the rest stays in the stock: {left}"
    );
}

fn chest_at(r: &mut CampRun, free: u32, at: Tick) {
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::HavenChest(boundary::HavenChest {
            faction: r.key,
            dimension: u32::from(synth::CAMP_POS.frame),
            pos: (synth::CAMP_POS.x + 3, 64, synth::CAMP_POS.z + 2),
            free,
        }),
        at,
    );
}
