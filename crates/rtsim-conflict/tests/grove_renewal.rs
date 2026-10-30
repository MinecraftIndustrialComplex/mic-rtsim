// SPDX-License-Identifier: AGPL-3.0-or-later
//! S9 (MZB-1210 to MZB-1216, MZB-1260): the Umvuthana grove on the shared renewal rule. Its warriors
//! come back one at a time (`renewal` 6, 96, 96, no boss kind), its Umvuthi has a pressure of its
//! own (`Camp::chief_pressure`, 1 a chief), a war party's casualties press the grove by kill credit,
//! and a grove found empty has K 5 and waits the cap. Played against the emulated plugin
//! (`synth::CampRun`) on the shipped cultures.

use rtsim_conflict::boundary::{self, Obs};
use rtsim_conflict::culture::{Cultures, PartyKind};
use rtsim_conflict::engine::{CHIEF_SLOT, Party, SlotState, Target, body_id};
use rtsim_conflict::synth::{self, CampRun};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::renewal::Pressure;

const HALF_LIFE: f32 = 96.0;

fn days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round() as u64
}

fn in_days(from: Tick, to: Tick) -> f64 {
    (to.0 as f64 - from.0 as f64) / TICKS_PER_DAY as f64
}

fn next_op() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    (1 << 62) | (N.fetch_add(1, Ordering::Relaxed) + 1)
}

/// A grove that never plunders, so the only parties are the ones a test orders.
fn quiet(seed: u64, found: u16) -> CampRun {
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", found)
}

fn ok(r: &mut CampRun) {
    let bad = r.check();
    assert!(bad.is_empty(), "the invariants break: {bad:#?}");
    assert!(r.failures.is_empty(), "{:#?}", r.failures);
}

fn come_in(r: &mut CampRun) {
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

fn day_of(t: Tick) -> u32 {
    (t.0 / TICKS_PER_DAY) as u32
}

/// The warriors' pressure now (the members').
fn members_p(r: &CampRun) -> f32 {
    r.c.camp(r.f)
        .unwrap()
        .pressure
        .now(day_of(r.c.now()), HALF_LIFE)
}

/// The Umvuthi's pressure now.
fn chief_p(r: &CampRun) -> f32 {
    r.c.camp(r.f)
        .unwrap()
        .chief_pressure
        .now(day_of(r.c.now()), HALF_LIFE)
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() < 2e-3, "{what}: {a} against {b}");
}

fn chief(r: &CampRun) -> Option<(SlotState, u32)> {
    r.c.camp(r.f)
        .and_then(|c| c.chief())
        .map(|s| (s.state, s.generation))
}

fn heir_due(r: &CampRun) -> Tick {
    r.c.camp(r.f).and_then(|c| c.heir).expect("an heir").due
}

fn whole(r: &CampRun) -> u32 {
    r.c.pool_at(r.f, r.c.now()).floor() as u32
}

/// The warriors standing in the camp's world: tagged, no chief.
fn warrior_uuids(r: &CampRun) -> Vec<boundary::Uuid> {
    r.plugin.camps[&r.key]
        .bodies
        .iter()
        .filter(|b| b.generation != 0 && !b.chief && b.slot != CHIEF_SLOT)
        .map(|b| b.uuid)
        .collect()
}

fn chief_uuid(r: &CampRun) -> boundary::Uuid {
    r.plugin.camps[&r.key]
        .bodies
        .iter()
        .find(|b| b.slot == CHIEF_SLOT && b.generation != 0)
        .expect("a seated Umvuthi")
        .uuid
}

/// `n` warriors killed at the grove, credited as `by` says, and the deaths heard.
fn kill_warriors(r: &mut CampRun, n: usize, by: u32) {
    let at = r.c.now() + 1;
    let ids: Vec<_> = warrior_uuids(r).into_iter().take(n).collect();
    assert_eq!(ids.len(), n, "enough warriors to kill");
    for u in ids {
        assert!(r.plugin.camp_kill(r.key, u, by, at));
    }
    r.run(at + 5);
}

fn kill_chief(r: &mut CampRun, by: u32) -> Tick {
    let at = r.c.now() + 1;
    let u = chief_uuid(r);
    assert!(r.plugin.camp_kill(r.key, u, by, at));
    r.run(at + 5);
    at
}

/// A grove of `found` bodies whose first is an untagged Umvuthi, adopted at the first census: the
/// Umvuthi stands, its pressure 0 (the jar's own chief, nobody has farmed it).
fn adopted(seed: u64, found: u16) -> CampRun {
    let mut r = quiet(seed, found);
    let umvuthi = body_id(&r.c.cultures, "mic_rtsim:conflict/umvuthana/umvuthi");
    {
        let b = r.plugin.camp_world(r.key).bodies.first_mut().unwrap();
        b.body = umvuthi;
        b.chief = true;
    }
    come_in(&mut r);
    assert!(matches!(chief(&r), Some((SlotState::Body, g)) if g != 0));
    ok(&mut r);
    r
}

/// The camp comes in and waits out its first heir: a seated Umvuthi.
fn seat_chief(r: &mut CampRun) {
    come_in(r);
    let due = heir_due(r);
    r.run(due + 100);
    assert!(matches!(chief(r), Some((SlotState::Body, _))), "seated");
}

// ---- the warriors' renewal ------------------------------------------------------------------------

/// MZB-1210: a wiped-down grove is not refilled at 2 a day: each missing warrior comes back after
/// `min(96, 6 x 2^P)` days, one at a time, the next wait read at the end of the last. Three of
/// seven killed by a player one after another: the first wait starts at the first death and reads
/// the pressure there, 1/7 (6.6 days: the kernel's rule reads P when the wait starts and never
/// pushes a running wait out), the next reads the 3/7 that stands at the first's end (8.0 days),
/// and two days after the first the grove still holds five.
#[test]
fn the_groves_warriors_come_back_one_at_a_time() {
    let mut r = quiet(31, 7);
    come_in(&mut r);
    assert_eq!(r.c.factions[r.f as usize].cap, 7);
    kill_warriors(&mut r, 3, boundary::BY_PLAYER);
    let killed = r.c.now();
    near(members_p(&r), 3.0 / 7.0, "three of seven");
    let first = r.c.camp(r.f).unwrap().arrival_at.expect("a wait is booked");
    let wait = in_days(killed, first);
    assert!(
        (6.5..6.8).contains(&wait),
        "6 x 2^(1/7) days for the first, read at the first death: {wait}"
    );
    assert_eq!(whole(&r), 4);
    r.run(first + 20);
    assert_eq!(r.c.stats.arrivals, 1);
    r.run(first + days(2.0));
    assert_eq!(whole(&r), 5, "one back, two days on the grove holds five");
    let second = r.c.camp(r.f).unwrap().arrival_at.expect("the next wait");
    assert!(
        in_days(first, second) > 7.5 && in_days(first, second) < 8.3,
        "the next wait reads the decayed pressure: {}",
        in_days(first, second)
    );
    r.run(second + days(30.0));
    assert_eq!(r.c.stats.arrivals, 3);
    assert_eq!(whole(&r), 7);
    assert!(r.c.camp(r.f).unwrap().arrival_at.is_none(), "whole: no wait");
    r.settle();
    ok(&mut r);
}

/// MZB-1211: a player's kill of a warrior presses the warriors, `1/K` each, and the Umvuthi's pressure
/// is not moved.
#[test]
fn a_players_kill_of_a_warrior_presses_the_members_and_not_the_chief() {
    let mut r = adopted(32, 9);
    near(chief_p(&r), 0.0, "an adopted Umvuthi, nobody's loss");
    near(members_p(&r), 0.0, "no loss yet");
    kill_warriors(&mut r, 2, boundary::BY_PLAYER);
    near(members_p(&r), 2.0 / 9.0, "two of nine");
    near(chief_p(&r), 0.0, "the Umvuthi is not pressed by warriors");
    ok(&mut r);
}

/// MZB-1211: the Umvuthi killed by a player presses its own pressure, 1, and not the warriors'; the
/// heir is due after `6 x 2^1` = 12 days (the cowing is shorter) whatever the warriors have lost.
#[test]
fn the_umvuthis_pressure_is_its_own() {
    for warriors_killed in [0usize, 3] {
        let mut r = adopted(33, 9);
        if warriors_killed > 0 {
            kill_warriors(&mut r, warriors_killed, boundary::BY_PLAYER);
        }
        let before = members_p(&r);
        let at = kill_chief(&mut r, boundary::BY_PLAYER);
        near(chief_p(&r), 1.0, "one chief lost");
        near(members_p(&r), before, "the warriors' pressure is untouched");
        let wait = in_days(at, heir_due(&r));
        assert!(
            (11.9..12.1).contains(&wait),
            "6 x 2^1 days whatever the warriors lost ({warriors_killed}): {wait}"
        );
        r.run(heir_due(&r) + 100);
        assert!(
            matches!(chief(&r), Some((SlotState::Body, _))),
            "the heir sits"
        );
        // And a warrior killed now presses the warriors alone.
        let chief_before = chief_p(&r);
        kill_warriors(&mut r, 1, boundary::BY_PLAYER);
        near(chief_p(&r), chief_before, "a warrior leaves the chief's alone");
        ok(&mut r);
    }
}

/// MZB-1212 (F7): the warriors that die with the Umvuthi (the jar's `die`, by no one) press nothing;
/// no more does any death no one is credited with at the grove, a boss kind's rule being the
/// Umvuthi's and not theirs.
#[test]
fn the_umvuthis_collateral_deaths_press_nothing() {
    let mut r = adopted(34, 9);
    kill_warriors(&mut r, 3, boundary::BY_NONE);
    near(members_p(&r), 0.0, "no one's loss");
    assert_eq!(r.c.stats.renewal_losses, 0);
    // The wait to replace them is the base one.
    let wait = in_days(r.c.now(), r.c.camp(r.f).unwrap().arrival_at.expect("a wait"));
    assert!((5.5..6.1).contains(&wait), "6 x 2^0 days: {wait}");
    // A mob's kill is a predator's, and presses nothing either.
    kill_warriors(&mut r, 1, boundary::BY_MOB);
    near(members_p(&r), 0.0, "a predator's");
    ok(&mut r);
}

/// MZB-1212: the heir takes a warrior (MASK) and that warrior is no loss: the grove is short by one,
/// the warriors' wait starts at pressure 0, and at its end a warrior comes.
#[test]
fn the_heirs_warrior_is_no_loss_and_is_replaced_by_the_rule() {
    let mut r = quiet(35, 9);
    seat_chief(&mut r);
    near(members_p(&r), 0.0, "no loss");
    assert_eq!(r.c.stats.renewal_losses, 0);
    assert_eq!(whole(&r), 8, "a warrior was taken");
    let due = r.c.camp(r.f).unwrap().arrival_at.expect("a wait is booked");
    let wait = in_days(r.c.now(), due);
    assert!((5.5..6.1).contains(&wait), "6 x 2^0 days: {wait}");
    r.run(due + 100);
    assert_eq!(whole(&r), 9, "replaced at the wait's end");
    assert_eq!(r.c.stats.arrivals, 1);
    r.settle();
    ok(&mut r);
}

// ---- marches ------------------------------------------------------------------------------------

/// A BAND_CASUALTIES for `count` of the party's first unit, credited as `by` says.
fn casualty(r: &CampRun, p: Handle<Party>, epoch: u32, by: u32, count: u32, op: u64) -> Obs {
    let unit = r.c.parties[p].units[0].0;
    let cu = r.c.cultures.get(r.c.factions[r.f as usize].culture);
    Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(p),
        epoch,
        by,
        body: body_id(&r.c.cultures, &cu.unit(unit).body),
        count,
        killer: (0, 0),
        op,
    })
}

/// A party of `kind` to `target`, its members embodied at a new epoch as a watched fight leaves
/// them: the casualties below count against that epoch.
fn embodied_party(r: &mut CampRun, kind: PartyKind, target: Target) -> (Handle<Party>, u32) {
    let at = r.c.now();
    let p =
        r.c.order_party(&mut r.w, r.key, kind, target, None, at)
            .expect("the grove fields it");
    r.flush(at);
    let x = &mut r.c.parties[p];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    (p, x.epoch)
}

fn ingest(r: &mut CampRun, o: &Obs) {
    let at = r.c.now();
    r.c.ingest(&mut r.w, &mut *r.infra, o, at);
}

fn district() -> Target {
    Target::District(synth::REFINERY, synth::REFINERY_POS)
}

/// MZB-1213: a member of a war party killed on the road by a player presses the grove, `1/K` each,
/// before the party's loss plans the wait: S7's finding that a grove's march losses book no pressure
/// is closed. A taken member is a player's too.
#[test]
fn a_credited_march_casualty_presses_the_grove() {
    let mut r = quiet(36, 9);
    come_in(&mut r);
    let (p, e) = embodied_party(&mut r, PartyKind::Raid, Target::Pos(synth::FOREST_POS));
    let o = casualty(&r, p, e, boundary::BY_PLAYER, 2, next_op());
    ingest(&mut r, &o);
    near(members_p(&r), 2.0 / 9.0, "two of nine");
    assert_eq!(r.c.stats.renewal_losses, 2);
    let o = casualty(&r, p, e, boundary::BY_TAKEN, 1, next_op());
    ingest(&mut r, &o);
    near(members_p(&r), 3.0 / 9.0, "a taken member too");
    near(chief_p(&r), chief_p(&r), "the Umvuthi's is its own");
    // The wait was planned from the pressure: the grove is short by the members away and killed.
    assert!(r.c.camp(r.f).unwrap().arrival_at.is_none() || members_p(&r) > 0.0);
}

/// MZB-1213: a mob's kill on the road is a predator's: the members fall and the grove is not pressed.
#[test]
fn a_mobs_kill_on_a_march_does_not() {
    let mut r = quiet(37, 9);
    come_in(&mut r);
    let (p, e) = embodied_party(&mut r, PartyKind::Raid, Target::Pos(synth::FOREST_POS));
    let before = r.c.parties[p].alive();
    let o = casualty(&r, p, e, boundary::BY_MOB, 2, next_op());
    ingest(&mut r, &o);
    near(members_p(&r), 0.0, "a predator's");
    assert_eq!(r.c.parties[p].alive(), before - 2, "the members did fall");
    assert_eq!(r.c.stats.renewal_losses, 0);
}

/// MZB-1213 (F8, S7's FB-2 (a)): a death that credits no one is no one's loss on the road, a player's
/// at a district (a trap, a turret, a FakePlayer killed it there), and the switch turns that off.
#[test]
fn an_unclaimed_march_death_does_not_and_one_at_a_players_district_does() {
    for by in [boundary::BY_NONE, boundary::BY_OTHER] {
        let mut r = quiet(38, 9);
        come_in(&mut r);
        let (p, e) = embodied_party(&mut r, PartyKind::Raid, Target::Pos(synth::FOREST_POS));
        let o = casualty(&r, p, e, by, 2, next_op());
        ingest(&mut r, &o);
        near(members_p(&r), 0.0, "on the road");

        let mut r = quiet(39, 9);
        come_in(&mut r);
        let (p, e) = embodied_party(&mut r, PartyKind::Raid, district());
        let o = casualty(&r, p, e, by, 2, next_op());
        ingest(&mut r, &o);
        near(members_p(&r), 2.0 / 9.0, "at a player's district");

        let mut r = quiet(39, 9);
        r.c.tun.district_unclaimed_is_players = false;
        come_in(&mut r);
        let (p, e) = embodied_party(&mut r, PartyKind::Raid, district());
        let o = casualty(&r, p, e, by, 2, next_op());
        ingest(&mut r, &o);
        near(members_p(&r), 0.0, "the switch is off");
    }
}

/// MZB-1213: a casualty is booked once, however often the record comes (its op id).
#[test]
fn a_replayed_casualty_bumps_a_grove_once() {
    let mut r = quiet(40, 9);
    come_in(&mut r);
    let (p, e) = embodied_party(&mut r, PartyKind::Raid, Target::Pos(synth::FOREST_POS));
    let o = casualty(&r, p, e, boundary::BY_PLAYER, 2, next_op());
    ingest(&mut r, &o);
    ingest(&mut r, &o);
    let at = r.c.now();
    r.c.ingest_copy(&mut r.w, &mut *r.infra, &o, at);
    near(members_p(&r), 2.0 / 9.0, "booked once");
    assert_eq!(r.c.stats.renewal_losses, 2);
}

/// MZB-1212: an envoy or a scout killed draws on no pool and presses nothing.
#[test]
fn an_envoy_or_a_scout_presses_nothing() {
    for kind in [PartyKind::Envoy, PartyKind::Scouts] {
        let mut r = quiet(41, 9);
        come_in(&mut r);
        let (p, e) = embodied_party(&mut r, kind, Target::Pos(synth::FOREST_POS));
        let o = casualty(&r, p, e, boundary::BY_PLAYER, 1, next_op());
        ingest(&mut r, &o);
        near(members_p(&r), 0.0, &format!("{kind:?}"));
        assert_eq!(r.c.stats.renewal_losses, 0, "{kind:?}");
    }
}

/// MZB-1211, `chief_killed_by`: the Umvuthi that leads a band and falls on a march presses its own
/// pressure by one and not the warriors' by `1/K` (the den's rule, "a den has no chief slot", would
/// have pressed both).
#[test]
fn an_umvuthi_killed_on_a_march_presses_only_its_own() {
    let mut r = quiet(42, 9);
    seat_chief(&mut r);
    let generation = chief(&r).unwrap().1;
    // The payer is remembered, so the chief is told apart from one that carries nothing.
    let g = r.w.goods.find("gold_nugget").expect("a good");
    let id = rtsim_conflict::engine::item_of(g);
    r.c.define(2, id, "gold_nugget");
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot: CHIEF_SLOT,
            generation,
            flags: 0,
            player: synth::PLAYER,
            op: 0x5111,
            items: vec![boundary::Item { item: id, count: 9 }],
        }),
        at,
    );
    let at = r.c.now();
    let p =
        r.c.order_party(&mut r.w, r.key, PartyKind::Invasion, district(), None, at)
            .expect("an invasion");
    r.flush(at);
    assert!(r.c.parties[p].chief, "the chief leads");
    let muster = r.c.parties[p].until;
    r.run(muster + 200);
    let (members0, chief0) = (members_p(&r), chief_p(&r));
    let at = r.c.now();
    let who = r.player;
    r.c.chief_killed_by(&mut r.w, p, Some(who), boundary::BY_PLAYER, None, at);
    r.flush(at);
    near(members_p(&r), members0, "the warriors' pressure is untouched");
    near(chief_p(&r), chief0 + 1.0, "the Umvuthi's takes one");
}

// ---- found empty, adopted, found again ------------------------------------------------------------

/// MZB-1216: a grove found with no warrior and no row has K 5 (`found_cap`), not 1, and its first
/// warrior comes after the cap's 96 days; its Umvuthi waits the cap's 192.
#[test]
fn a_grove_found_empty_has_k_five_and_waits_the_cap() {
    let mut r = quiet(43, 0);
    assert_eq!(r.c.factions[r.f as usize].cap, 5, "the jar's least");
    assert_eq!(whole(&r), 0);
    near(members_p(&r), 4.0, "the warriors' pressure at the cap (96/6 = 2^4)");
    near(chief_p(&r), 5.0, "the Umvuthi's at the cap (192/6 = 2^5)");
    let found = r.c.now();
    let first = r.c.camp(r.f).unwrap().arrival_at.expect("a wait");
    let wait = in_days(found, first);
    assert!((95.9..96.1).contains(&wait), "the cap: {wait}");
    let heir = in_days(found, heir_due(&r));
    assert!((191.9..192.1).contains(&heir), "the cap: {heir}");
    r.run(first + 100);
    assert_eq!(r.c.stats.arrivals, 1);
    assert_eq!(whole(&r), 1);
    // A grove found short keeps its count for K (MOWZIE.md §4.5).
    let r = quiet(44, 3);
    assert_eq!(r.c.factions[r.f as usize].cap, 3);
}

/// MZB-1211, MZB-405: an untagged Umvuthi seen at a grove found empty resets the Umvuthi's pressure,
/// which was only the cap's stand-in, and the warriors' pressure is left as it stands.
#[test]
fn an_untagged_umvuthi_resets_only_the_chiefs_stand_in() {
    let mut r = quiet(45, 9);
    let umvuthi = body_id(&r.c.cultures, "mic_rtsim:conflict/umvuthana/umvuthi");
    {
        let b = r.plugin.camp_world(r.key).bodies.first_mut().unwrap();
        b.body = umvuthi;
        b.chief = true;
    }
    assert!(r.c.camp(r.f).unwrap().heir.is_some_and(|h| h.found_empty));
    near(chief_p(&r), 5.0, "the stand-in");
    // The warriors have lost two to a player meanwhile.
    let day = day_of(r.c.now());
    r.c.factions[r.f as usize]
        .camp
        .as_mut()
        .unwrap()
        .pressure
        .bump(day, 0.5, HALF_LIFE);
    come_in(&mut r);
    assert!(r.c.camp(r.f).unwrap().heir.is_none(), "the wait is over");
    near(chief_p(&r), 0.0, "the stand-in is gone");
    near(members_p(&r), 0.5, "the warriors' are left alone");
    ok(&mut r);
}

/// The site ledger's row for the camp as the engine has it now, handed back `elapsed` ticks later.
fn row_of(r: &CampRun, elapsed: u64, gap: bool) -> rtsim_kernel::sites::FoundRow {
    let s = r.c.camp_site_state(r.f, r.c.now()).expect("a row");
    rtsim_kernel::sites::FoundRow::from_state(&s, elapsed, gap)
}

/// MZB-1218, MZB-1231: a reset hands a grove its row: the warriors' pressure in `p`, the Umvuthi's
/// in `p2`, each aged by the ticks elapsed; with the Umvuthi standing, a kill of it after the reset
/// waits by the pressure it had (6 x 2^(1 + P)), not the 12 days of a fresh one.
#[test]
fn the_row_restores_both_pressures_aged() {
    let mut r = quiet(46, 9);
    seat_chief(&mut r);
    kill_warriors(&mut r, 2, boundary::BY_PLAYER);
    let (m0, c0) = (members_p(&r), chief_p(&r));
    assert!(c0 > 1.0, "the Umvuthi seated by the cap's wait is at {c0}");
    near(m0, 2.0 / 9.0, "two of nine");
    let to = r.c.now() + days(2.0);
    r.run(to);
    let row = row_of(&r, days(1.0), false);
    assert!(row.waiting(), "the warriors' wait is booked");
    near(row.p, members_p(&r), "p is the warriors'");
    near(row.p2, chief_p(&r), "p2 is the Umvuthi's");
    let at = r.c.now() + days(1.0);
    let aged = 2f32.powf(-(2.0 * TICKS_PER_DAY as f32 / TICKS_PER_DAY as f32) / HALF_LIFE);
    let (want_m, want_c) = (row.p * aged.sqrt(), row.p2 * aged.sqrt());
    r.reborn(row, at);
    assert!(
        chief(&r).is_some() && r.c.camp(r.f).unwrap().heir.is_none(),
        "the Umvuthi is owed its body, not waited for"
    );
    near(members_p(&r), want_m, "the warriors' aged one day");
    near(chief_p(&r), want_c, "the Umvuthi's aged one day");
    // The Umvuthi killed now waits what its pressure says, not the 12 days of a fresh one.
    come_in(&mut r);
    let killed = kill_chief(&mut r, boundary::BY_PLAYER);
    let wait = in_days(killed, heir_due(&r));
    let want = 6.0 * (f64::from(want_c) + 1.0).exp2();
    assert!(
        (wait - want).abs() < want * 0.03,
        "6 x 2^(P + 1) = {want:.1} days, not 12: {wait}"
    );
    assert!(wait > 20.0);
}

/// MZB-1218: a known grove found again with its row, nothing killed meanwhile, changes nothing: K,
/// both pressures and the wait stand and no body is minted.
#[test]
fn a_known_grove_found_again_changes_nothing() {
    let mut r = adopted(47, 9);
    let to = r.c.now() + days(3.0);
    r.run(to);
    let row = row_of(&r, 0, false);
    assert!(!row.waiting() && row.count == 9, "whole: nine warriors and the Umvuthi");
    let before = (r.c.factions[r.f as usize].cap, members_p(&r), chief_p(&r));
    let spawns = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_SPAWN)
        .count();
    let at = r.c.now() + 100;
    r.reborn(row, at);
    assert_eq!(r.c.factions[r.f as usize].cap, before.0);
    near(members_p(&r), before.1, "warriors");
    near(chief_p(&r), before.2, "chief");
    assert!(r.c.camp(r.f).unwrap().arrival_at.is_none(), "no wait");
    come_in(&mut r);
    let after = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_SPAWN)
        .count();
    assert_eq!(after, spawns, "nothing minted");
    ok(&mut r);
}

/// MZB-1224: a save and load at any point changes no decision for a grove, both pressures and the
/// waits among them: the farmed life that saved ends in the state of the one that did not.
#[test]
fn a_restart_changes_no_decision_for_a_grove() {
    let life = |saves: &[u32]| -> (u64, u64, u64) {
        let mut r = adopted(48, 9);
        for day in 1..=60u32 {
            let to = r.c.now() + days(1.0);
            r.run(to);
            if day % 3 == 0
                && let Some(&u) = warrior_uuids(&r).first()
            {
                let at = r.c.now() + 1;
                r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at);
            }
            if day == 30 {
                // The Umvuthi is farmed once.
                let u = chief_uuid(&r);
                let at = r.c.now() + 1;
                r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at);
            }
            if saves.contains(&day) {
                r.round_trip();
            }
        }
        assert!(r.failures.is_empty(), "{:#?}", r.failures);
        (r.c.state_hash(), r.c.stats.arrivals, r.c.stats.renewal_losses)
    };
    let straight = life(&[]);
    assert!(straight.1 >= 5 && straight.2 >= 10, "a farmed life: {straight:?}");
    for saves in [&[10u32][..], &[5, 15, 25, 35, 45, 55], &[29, 30, 31]] {
        assert_eq!(life(saves), straight, "saves {saves:?}");
    }
}

/// MZB-1230: a camp's chief pressure rides the page: a save and load keeps it, and the state hash
/// covers it.
#[test]
fn the_chiefs_pressure_is_in_the_save() {
    let mut r = adopted(49, 9);
    kill_chief(&mut r, boundary::BY_PLAYER);
    let h = r.c.state_hash();
    near(chief_p(&r), 1.0, "one");
    let back = rtsim_conflict::Conflict::load(
        r.c.tun.clone(),
        r.c.cultures.clone(),
        &r.c.save(),
    )
    .expect("loads");
    assert_eq!(back.state_hash(), h);
    let cp = back.camp(r.f).unwrap().chief_pressure;
    assert_eq!(cp.parts(), r.c.camp(r.f).unwrap().chief_pressure.parts());
    // The hash moves with the chief's pressure alone.
    r.c.factions[r.f as usize]
        .camp
        .as_mut()
        .unwrap()
        .chief_pressure = Pressure::from_parts(2.5, 3);
    assert_ne!(r.c.state_hash(), h);
}

/// MZB-1219: the census names the groves and the dens apart from the pool camps, with the warriors'
/// pressure as P and the arrivals as renewals.
#[test]
fn the_census_names_the_grove_apart() {
    let mut r = quiet(50, 9);
    come_in(&mut r);
    kill_warriors(&mut r, 3, boundary::BY_PLAYER);
    let census = r.c.census();
    let kinds: Vec<_> = census.iter().map(|c| c.kind).collect();
    assert!(
        kinds.contains(&"conflict.grove"),
        "a grove is its own kind: {kinds:?}"
    );
    let grove = census.iter().find(|c| c.kind == "conflict.grove").unwrap();
    assert_eq!((grove.sites, grove.members, grove.capacity), (1, 6, 9));
    assert!((grove.mean_p() - 3.0 / 9.0).abs() < 2e-3, "{}", grove.mean_p());
    assert_eq!(grove.short, 1);
    let camp = census.iter().find(|c| c.kind == "conflict.camp");
    assert!(camp.is_none_or(|c| c.sites == 0), "no pool camp here");
    // A Frostmaw's den is a kind of its own.
    let d = CampRun::new(51, "frostmaw", 1);
    let kinds: Vec<_> = d.c.census().iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&"conflict.den"), "{kinds:?}");
    let _ = NoInfrastructure;
}

/// MZB-1260: the audit names a pressure the culture has no rule for (a den keeps no chief's), a
/// pressure that is not a number, and a rate refill booked for a camp on the renewal rule.
#[test]
fn the_audit_names_a_pressure_a_culture_does_not_keep() {
    let mut den = CampRun::new(52, "frostmaw", 1);
    come_in(&mut den);
    assert!(den.c.camp_audit(den.f, den.c.now()).is_empty());
    let f = den.f as usize;
    den.c.factions[f].camp.as_mut().unwrap().chief_pressure = Pressure::from_parts(1.0, 0);
    let bad = den.c.camp_audit(den.f, den.c.now());
    assert!(bad.iter().any(|b| b.starts_with("MZB-1211")), "{bad:?}");

    let mut r = quiet(53, 9);
    come_in(&mut r);
    assert!(r.c.camp_audit(r.f, r.c.now()).is_empty());
    let f = r.f as usize;
    r.c.factions[f].camp.as_mut().unwrap().pressure = Pressure::from_parts(f32::INFINITY, 0);
    let bad = r.c.camp_audit(r.f, r.c.now());
    assert!(bad.iter().any(|b| b.contains("members'")), "{bad:?}");
    r.c.factions[f].camp.as_mut().unwrap().pressure = Pressure::default();
    r.c.factions[f].camp.as_mut().unwrap().refill_at = Some(Tick(5));
    let bad = r.c.camp_audit(r.f, r.c.now());
    assert!(bad.iter().any(|b| b.starts_with("MZB-1210")), "{bad:?}");
}

// ---- the grove's plunder mood (FB-S9-2, Ben, 2026-10-30) -------------------------------------------

/// The grove carries the raider's `plunder_mood` block: a raptor pack lost to a village's defence is
/// `1/K` of the cap in grief, and the next plunder waits `exp(2.8 x grief)` times as long, up to the
/// block's 3 (the raider's `max_factor`), so a grove that loses its pack every time plunders every
/// 35 days or so instead of every 12.
#[test]
fn a_groves_lost_pack_lengthens_its_plunder_wait() {
    let (_w, mut c, keys) = synth::mowzie_world_on(6, 0, 1, 7, 51, Cultures::builtin());
    let f = c.faction_of_key(keys[0]).expect("the grove");
    let at = Tick(10);
    assert_eq!(c.mood_now(f, at), Some((0.0, 0.0)), "a grove has a mood");
    let before = c.plunder_mean_days(f, at).expect("a grove plunders");
    // The mean over temperaments is the culture's 12 days (the camps' spread is the raider's).
    assert!((6.0..24.0).contains(&before), "{before}");
    c.grieve(f, 1.0 / 7.0, at);
    let one = c.plunder_mean_days(f, at).unwrap();
    assert!(
        (one / before - (2.8f64 / 7.0).exp()).abs() < 1e-6,
        "one pack lost: {one} against {before}"
    );
    c.gloat(f, at);
    let shorter = c.plunder_mean_days(f, at).unwrap();
    assert!(
        (shorter / before - (2.8f64 / 7.0 - 0.15).exp()).abs() < 1e-6,
        "loot home shortens it: {shorter} against {before}"
    );
    assert!(shorter < one);
    // A whole cap lost on top: the factor is held at the block's three.
    c.grieve(f, 1.0, at);
    let clamped = c.plunder_mean_days(f, at).unwrap();
    assert!(
        (clamped / before - 3.0).abs() < 1e-6,
        "clamped at three times: {clamped} against {before}"
    );
}
