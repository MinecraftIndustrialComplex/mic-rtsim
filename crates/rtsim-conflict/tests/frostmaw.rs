// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M6 (§4.7, ledger 17; EQUILIBRIUM.md §2.2): the Frostmaw's
//! den renews by a wait, not a rate. A den whose Frostmaw is gone gets a new one after
//! `min(cap, base x 2^P)` days, P being the recent losses a player caused; the arrival is owed a
//! body and spawned at once at a den that is in, and by the next census at one that is out. The
//! chief's death, at the den and away, and the crystal the steal takes are tested here as well.
//! Played against the emulated plugin (`synth::CampRun`) on the shipped cultures.

use rtsim_conflict::Step;
use rtsim_conflict::boundary;
use rtsim_conflict::culture::SignalKind;
use rtsim_conflict::engine::{Party, Signal, SlotState};
use rtsim_conflict::synth::{CAMP_POS, CampRun, Fight};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, RaidOutcome};
use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::ActorId;

fn days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round() as u64
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

/// The booked arrival's tick.
fn arrival_at(r: &CampRun) -> Option<Tick> {
    r.c.camp(r.f).expect("a ledger").arrival_at
}

/// Arrival events on the wheel.
fn arrivals_pending(r: &CampRun) -> usize {
    r.c.pending_by_kind().get("arrival").copied().unwrap_or(0)
}

/// The den's one body, killed by `by` one tick from now: the tick of the kill.
fn kill_the_frostmaw(r: &mut CampRun, by: u32) -> Tick {
    let b = r.plugin.camps[&r.key].bodies[0];
    let at = r.c.now() + 1;
    assert!(r.plugin.camp_kill(r.key, b.uuid, by, at));
    let to = r.c.now() + 5;
    r.run(to);
    at
}

// ---- the wait ----------------------------------------------------------------------------------

/// A Frostmaw killed by a mob (no player's loss) is replaced after the base wait, 6 days (EQ-4 (b),
/// MZB-258): the arrival is booked from the death, one tick in the engine's own clock, and when it
/// comes the den holds one member again, owed a body.
#[test]
fn a_den_killed_by_a_mob_comes_back_in_six_days() {
    let mut r = den(31, 1);
    come_in(&mut r);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(arrival_at(&r), None, "a den that is full waits for nothing");
    assert_eq!(arrivals_pending(&r), 0);
    let died = kill_the_frostmaw(&mut r, boundary::BY_MOB);
    assert_eq!(counts(&r), [0, 0, 0, 0, 0]);
    assert_eq!(
        arrival_at(&r),
        Some(died + days(6.0)),
        "6.00 days after the death"
    );
    assert_eq!(arrivals_pending(&r), 1);
    assert_eq!(
        r.c.camp(r.f).unwrap().pressure.parts().0,
        0.0,
        "a mob's kill is no one's loss"
    );
    // Just before it, nothing; at it, one member.
    r.run(Tick((died + days(6.0)).0 - 1));
    assert_eq!(r.c.stats.arrivals, 0);
    assert_eq!(counts(&r), [0, 0, 0, 0, 0]);
    ok(&mut r);
    r.run(died + days(6.0) + 10);
    assert_eq!(r.c.stats.arrivals, 1);
    assert_eq!(arrival_at(&r), None);
    assert_eq!(arrivals_pending(&r), 0);
    assert_eq!(
        counts(&r),
        [1, 0, 0, 0, 0],
        "owed, and spawned at a den that is in"
    );
    ok(&mut r);
}

/// A den found with no member waits the cap, 192 days, for its migrant (MZB-257, EQUILIBRIUM item
/// 6: found empty is as if farmed to the cap), not the base 6.
#[test]
fn a_den_found_empty_waits_the_cap() {
    let mut r = den(32, 0);
    let t = arrival_at(&r).expect("a wait is booked");
    let wait = (t.0 - 1) as f64 / TICKS_PER_DAY as f64;
    assert!((wait - 192.0).abs() < 1e-3, "{wait} days");
    assert_eq!(counts(&r), [0, 0, 0, 0, 0]);
    // Not at 191 days: still empty.
    r.run(Tick(1) + days(191.0));
    assert_eq!(r.c.stats.arrivals, 0);
    r.run(Tick(1) + days(193.0));
    assert_eq!(r.c.stats.arrivals, 1);
    assert_eq!(
        counts(&r),
        [0, 1, 0, 0, 0],
        "owed: the camp is out, its census spawns it"
    );
    ok(&mut r);
    // A den found with its Frostmaw books nothing.
    let r = den(33, 1);
    assert_eq!(arrival_at(&r), None);
    assert_eq!(arrivals_pending(&r), 0);
}

/// While the Frostmaw is out on a raid its den is not short of one: no wait is booked, and none
/// when it comes home (MZB-255: short is the pool and the members away under the cap).
#[test]
fn no_wait_while_the_frostmaw_is_out() {
    let mut r = den(34, 1);
    come_in(&mut r);
    let p = r.raid().expect("a raid goes");
    assert_eq!(r.c.factions[r.f as usize].away, 1);
    r.run(r.c.now() + days(3.0));
    assert_eq!(arrival_at(&r), None, "out, not lost");
    assert_eq!(arrivals_pending(&r), 0);
    assert!(r.c.parties.get(p).is_some(), "still out");
    r.run(r.c.now() + days(20.0));
    assert_eq!(r.c.stats.arrivals, 0);
    assert_eq!(arrivals_pending(&r), 0);
    ok(&mut r);
}

/// A stray Frostmaw adopted into the empty den cancels the wait (MZB-255): the stale event, when it
/// comes, books nothing and counts as stale.
#[test]
fn an_adopted_stray_cancels_the_wait() {
    let mut r = den(35, 0);
    assert!(arrival_at(&r).is_some());
    r.plugin.camp_stray(r.key, r.body);
    come_in(&mut r);
    assert_eq!(
        counts(&r),
        [1, 0, 0, 0, 0],
        "the stray is the den's member now"
    );
    assert_eq!(arrival_at(&r), None, "the wait is cancelled");
    let stale = r.c.stats.stale;
    r.run(Tick(1) + days(200.0));
    assert_eq!(r.c.stats.arrivals, 0, "no second Frostmaw");
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert!(r.c.stats.stale > stale, "the old event ran out stale");
    ok(&mut r);
}

/// An arrival at a den that is out is owed and waits for the next census, as any owed slot does
/// (MZB-256): no order goes out, and the census spawns the body with its crystal.
#[test]
fn the_arrival_is_owed_and_spawned_at_the_next_census() {
    let mut r = den(36, 0);
    r.run(Tick(1) + days(193.0));
    assert_eq!(r.c.stats.arrivals, 1);
    assert_eq!(counts(&r), [0, 1, 0, 0, 0]);
    assert!(
        r.cmds
            .iter()
            .all(|(_, c)| !matches!(c, boundary::Cmd::Reconcile(_))),
        "a camp that is out is ordered nothing"
    );
    assert!(
        r.c.factions[r.f as usize].crystal,
        "a new Frostmaw holds its crystal"
    );
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    let spawn = r
        .plugin
        .applied
        .iter()
        .find(|a| a.line.action == boundary::ACTION_SPAWN)
        .expect("a spawn line");
    assert_ne!(
        spawn.line.flags & boundary::LINE_CRYSTAL,
        0,
        "with its crystal"
    );
    assert_ne!(spawn.line.flags & boundary::LINE_CHIEF, 0, "as the chief");
    ok(&mut r);
}

/// An arrival at a den that is in sends its spawn in the tick the wait ends (MZB-256, EQUILIBRIUM
/// item 8: a camp that never unloads gets its arrival when the wait ends).
#[test]
fn the_arrival_at_a_den_that_is_in_sends_its_spawn() {
    let mut r = den(37, 1);
    come_in(&mut r);
    let died = kill_the_frostmaw(&mut r, boundary::BY_MOB);
    let due = died + days(6.0);
    r.run(Tick(due.0 - 1));
    let sent = r.cmds.len();
    r.run(due);
    let new: Vec<_> = r.cmds[sent..]
        .iter()
        .filter_map(|(t, c)| match c {
            boundary::Cmd::Reconcile(x) => Some((*t, x.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(new.len(), 1, "one order, in the arrival's own tick");
    assert_eq!(new[0].0, due);
    assert_eq!(new[0].1.lines.len(), 1);
    assert_eq!(new[0].1.lines[0].action, boundary::ACTION_SPAWN);
    assert_ne!(new[0].1.lines[0].flags & boundary::LINE_CRYSTAL, 0);
    r.run(due + 10);
    assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
    assert_eq!(r.plugin.camps[&r.key].bodies.len(), 1);
    let s = &r.c.camp(r.f).unwrap().slots[0];
    assert_eq!(s.state, SlotState::Body);
    ok(&mut r);
}

// ---- the pressure ------------------------------------------------------------------------------

fn pressure_of(r: &CampRun) -> f32 {
    r.c.camp(r.f).unwrap().pressure.parts().0
}

/// A Frostmaw a player kills makes the next wait 12 days, twice the base (MZB-258, EQ-4 (b)):
/// pressure 1 at a cap of 1, `6 x 2^1`.
#[test]
fn a_first_player_kill_makes_the_next_wait_twelve_days() {
    let mut r = den(41, 1);
    come_in(&mut r);
    let died = kill_the_frostmaw(&mut r, boundary::BY_PLAYER);
    assert_eq!(pressure_of(&r), 1.0);
    assert_eq!(r.c.stats.renewal_losses, 1);
    assert_eq!(
        arrival_at(&r),
        Some(died + days(12.0)),
        "12.00 days after the death"
    );
    ok(&mut r);
}

/// Which deaths are a player's loss (MZB-258): a player's kill, a body taken and an unclaimed
/// death of a boss (`BY_NONE`, `BY_OTHER`) bump the pressure; a mob's kill does not.
#[test]
fn only_a_players_losses_bump_the_pressure() {
    for (by, bumps) in [
        (boundary::BY_PLAYER, true),
        (boundary::BY_TAKEN, true),
        (boundary::BY_NONE, true),
        (boundary::BY_OTHER, true),
        (boundary::BY_MOB, false),
    ] {
        let mut r = den(42, 1);
        come_in(&mut r);
        let died = kill_the_frostmaw(&mut r, by);
        let want = if bumps { 12.0 } else { 6.0 };
        assert_eq!(
            arrival_at(&r),
            Some(died + days(want)),
            "killed by kind {by}"
        );
        assert_eq!(r.c.stats.renewal_losses, u64::from(bumps), "kind {by}");
        ok(&mut r);
    }
}

/// A den whose Frostmaw is killed by a player each time it comes, 2 days after its arrival, waits as
/// the kernel's rule says on the days it ran, and as the closed form does within 3% (MZB-284): 12.0,
/// 22.45, 36.4, 47.2, 50.6 days, P settling at 3.109 (a wait of 51.76 days).
#[test]
fn a_farmed_den_follows_the_closed_form() {
    use rtsim_kernel::renewal::Pressure;
    let mut r = den(43, 1);
    come_in(&mut r);
    let rule =
        r.c.cultures
            .get(r.c.factions[r.f as usize].culture)
            .renewal
            .unwrap()
            .rule;
    // The kernel's own bookkeeping of the same kills, kept apart from the engine's.
    let mut model = Pressure::default();
    let mut waits = Vec::new();
    let mut at = r.c.now();
    for _ in 0..8 {
        let died = at + 1;
        let b = r.plugin.camps[&r.key].bodies[0];
        assert!(r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, died));
        r.run(died + 5);
        let day = (died.0 / TICKS_PER_DAY) as u32;
        model.bump(day, 1.0, rule.half_life_days);
        let booked = arrival_at(&r).expect("a wait");
        let want = rule.delay_ticks(model, day);
        assert_eq!(booked.0 - died.0, want, "the kernel's wait on day {day}");
        waits.push((booked.0 - died.0) as f64 / TICKS_PER_DAY as f64);
        // It comes, is spawned, and is killed again 2 days after.
        r.run(booked + 10);
        assert_eq!(counts(&r), [1, 0, 0, 0, 0]);
        at = booked + days(2.0);
        r.run(at);
        ok(&mut r);
    }
    for (got, want) in waits.iter().zip([12.0, 22.45, 36.4, 47.2, 50.6]) {
        assert!(
            (got - want).abs() <= 0.03 * want,
            "{waits:?}: {got} against {want}"
        );
    }
    assert_eq!(r.c.stats.arrivals, 8);
}

/// A member a census writes off (three misses over two days) is a player's loss, whatever took it
/// (MZB-259, EQUILIBRIUM item 11): the pressure steps up and the wait it plans is 12 days.
#[test]
fn a_write_off_bumps_the_pressure() {
    let mut r = den(44, 1);
    come_in(&mut r);
    // The body vanishes with no record: nothing tells the books.
    r.plugin.camps.get_mut(&r.key).unwrap().bodies.clear();
    let mut wrote_off = None;
    for _ in 0..8 {
        r.camp_out();
        let to = r.c.now() + 3;
        r.run(to);
        r.camp_in();
        let to = r.c.now() + days(0.8);
        r.run(to);
        if r.c.stats.camp_written_off == 1 {
            wrote_off = Some(r.c.now());
            break;
        }
    }
    let at = wrote_off.expect("written off after three censuses");
    assert_eq!(r.c.stats.renewal_losses, 1);
    assert_eq!(pressure_of(&r), 1.0);
    let booked = arrival_at(&r).expect("a wait");
    assert!(
        booked.0 > at.0 && booked.0 - at.0 <= days(12.0) && booked.0 - at.0 > days(11.0),
        "12 days from the census that wrote it off, not 6 ({} ticks)",
        booked.0 - at.0
    );
    ok(&mut r);
}

/// The pressure and the running wait are in the save, as a blob and through the page store: a den
/// saved mid-wait and loaded again has the same books and its arrival comes on the same tick as in
/// the run that never saved (MZB-271).
#[test]
fn the_den_saves_its_pressure_and_wait() {
    use rtsim_conflict::scenario::META_KEY;
    use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
    use std::sync::{Arc, RwLock};
    let end = |save: bool| {
        let mut r = den(45, 1);
        come_in(&mut r);
        let died = kill_the_frostmaw(&mut r, boundary::BY_PLAYER);
        r.run(died + days(5.0));
        if save {
            r.round_trip();
            assert!(r.failures.is_empty(), "{:#?}", r.failures);
            let c = r.c.camp(r.f).unwrap();
            assert_eq!(c.pressure.parts().0, 1.0);
            assert_eq!(c.arrival_at, Some(died + days(12.0)));
        }
        r.run(died + days(12.0));
        assert_eq!(r.c.stats.arrivals, 1, "the arrival came on the tick");
        r.run(died + days(12.0) + 10);
        (r.c.state_hash(), counts(&r), r.c.stats.arrivals)
    };
    assert_eq!(end(true), end(false));
    // Through the page store, with a budget that evicts everything.
    let dir = std::env::temp_dir().join(format!("rtsim-den-paged-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    let mut a = den(46, 1);
    a.c.attach_store(store.clone(), 8 << 10);
    come_in(&mut a);
    let died = kill_the_frostmaw(&mut a, boundary::BY_PLAYER);
    a.run(died + days(5.0));
    let mut batch = SaveBatch::new();
    let t = a.c.save_paged(&mut batch, META_KEY);
    store.write().unwrap().commit(batch, 0).unwrap();
    a.c.saved(t);
    a.c.enforce_budget();
    let dir_b = std::env::temp_dir().join(format!("rtsim-den-paged-b-{}", std::process::id()));
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
    assert_eq!(back.state_hash(), a.c.state_hash());
    assert_eq!(back.camp(a.f), a.c.camp(a.f));
    assert_eq!(back.camp(a.f).unwrap().pressure.parts().0, 1.0);
    assert_eq!(back.camp(a.f).unwrap().arrival_at, Some(died + days(12.0)));
    assert_eq!(back.pending_by_kind().get("arrival"), Some(&1));
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&dir_b).unwrap();
}

// ---- the chief's fall --------------------------------------------------------------------------

/// A player at the den: the Killing report, the successor, the cowed faction, the pressure, and
/// the next wait that follows from it (MZB-262, 264, 265).
fn feud_with(r: &mut CampRun, who: ActorId, step: Step) {
    let at = r.c.now() + 1;
    r.c.signal(
        &mut r.w,
        Signal {
            faction: Some(r.key),
            kind: SignalKind::Killing,
            // 80 grievance: above the Raid rung's 70, so a replan leaves a feud standing there.
            amount: 10.0,
            actor: who,
            pos: CAMP_POS,
            district: None,
        },
        at,
    );
    r.c.feud_mut(r.f, who).expect("a feud").step = step;
}

fn step_of(r: &CampRun, who: ActorId) -> Option<Step> {
    r.c.feud(r.f, who).map(|f| f.step)
}

/// The Frostmaw is the camp's chief: a player that kills its body at the den fells a war chief. The
/// faction is cowed, an heir takes over, the player is reported for the killing, and the death is
/// a player's loss, so the wait is 12 days (MZB-264, 265).
#[test]
fn a_player_killing_the_frostmaw_at_its_den_is_the_chiefs_fall() {
    let mut r = den(51, 1);
    come_in(&mut r);
    let chief = r.c.factions[r.f as usize].chief;
    assert_eq!(r.c.stats.chiefs_killed, 0);
    let died = kill_the_frostmaw(&mut r, boundary::BY_PLAYER);
    assert_eq!(r.c.stats.chiefs_killed, 1);
    assert_eq!(r.c.stats.successions, 1);
    assert_ne!(r.c.factions[r.f as usize].chief, chief, "the heir is chief");
    assert!(
        r.c.factions[r.f as usize].cowed_until > died,
        "cowed for a while"
    );
    assert!(
        r.w.knowledge.notable(chief).is_none(),
        "the old chief is dead"
    );
    assert_eq!(pressure_of(&r), 1.0);
    assert_eq!(arrival_at(&r), Some(died + days(12.0)));
    assert_eq!(counts(&r), [0, 0, 0, 0, 0]);
    ok(&mut r);
}

/// A mob's kill of the Frostmaw is the chief's fall too, with no killer to blame and no loss a
/// player caused: the base wait, no Killing report (MZB-262, 265).
#[test]
fn a_mob_killing_the_frostmaw_at_its_den_is_a_fall_with_no_pressure() {
    let mut r = den(52, 1);
    come_in(&mut r);
    let died = kill_the_frostmaw(&mut r, boundary::BY_MOB);
    assert_eq!(r.c.stats.chiefs_killed, 1);
    assert_eq!(pressure_of(&r), 0.0);
    assert_eq!(arrival_at(&r), Some(died + days(6.0)));
    assert!(
        r.c.feud(r.f, r.player).is_none(),
        "no one is to blame: no feud against the player"
    );
    ok(&mut r);
}

/// Only the killer's own feud steps down at the den, from Raid to Notice (the culture has no rung
/// between); a third player's feud stands where it was, since no band broke (MZB-264). A feud at
/// Notice is not raised by it, nor is its `since` touched, and a player with no feud gets none
/// from a step down (MZB-262).
#[test]
fn only_the_killers_feud_steps_down_at_the_den() {
    let mut r = den(53, 1);
    come_in(&mut r);
    let other = r.c.player(&mut r.w, (0x0713, 0xE2));
    let idle = r.c.player(&mut r.w, (0x1D1E, 0x01));
    let me = r.player;
    feud_with(&mut r, me, Step::Raid);
    feud_with(&mut r, other, Step::Raid);
    assert_eq!(step_of(&r, idle), None);
    kill_the_frostmaw(&mut r, boundary::BY_PLAYER);
    assert_eq!(step_of(&r, r.player), Some(Step::Notice), "the killer's");
    assert_eq!(step_of(&r, other), Some(Step::Raid), "a third player's");
    assert_eq!(step_of(&r, idle), None);
    ok(&mut r);
    // At Notice, a step down changes nothing, `since` included.
    let mut r = den(54, 1);
    come_in(&mut r);
    let me = r.player;
    feud_with(&mut r, me, Step::Notice);
    let since = r.c.feud(r.f, r.player).unwrap().since;
    kill_the_frostmaw(&mut r, boundary::BY_PLAYER);
    assert_eq!(step_of(&r, r.player), Some(Step::Notice));
    assert_eq!(r.c.feud(r.f, r.player).unwrap().since, since);
}

/// A body carried off is the player's kill of a chief too (BY_TAKEN, MZB-265), but a killer the
/// record does not name is no one: `d.killer` of 0 reports no one, and no feud steps down.
#[test]
fn a_body_taken_names_its_taker_only_when_the_record_does() {
    let mut r = den(55, 1);
    come_in(&mut r);
    let me = r.player;
    feud_with(&mut r, me, Step::Raid);
    // The emulated plugin names a killer for a player's kill only: a taken body carries none.
    kill_the_frostmaw(&mut r, boundary::BY_TAKEN);
    assert_eq!(r.c.stats.chiefs_killed, 1);
    assert_eq!(step_of(&r, r.player), Some(Step::Raid), "no killer named");
    assert_eq!(pressure_of(&r), 1.0, "but a taken body is a loss");
    ok(&mut r);
}

/// The Frostmaw out on a raid and killed by the player it fights: a chief's fall away (MZB-263),
/// the pressure bumped once, the band routed home, and the den waits 12 days from the band's loss.
/// The synth no longer lets the dead chief come home as a survivor (MZB-272).
#[test]
fn the_frostmaw_killed_on_a_raid_is_a_fall_and_the_den_waits() {
    let mut r = den(56, 1);
    r.plugin.fight = Some(Fight {
        kill_share: 0.0,
        after: 200,
        kill_chief: true,
        only_at_base: false,
    });
    come_in(&mut r);
    r.watch = true;
    let p = r.raid().expect("a raid goes");
    assert!(r.c.parties[p].chief, "the Frostmaw leads its own raid");
    r.camp_out();
    r.run(Tick(days(4.0)));
    assert_eq!(r.c.stats.chiefs_killed, 1, "{:#?}", r.c.stats);
    assert_eq!(r.c.stats.successions, 1);
    assert_eq!(pressure_of(&r), 1.0, "bumped once, not by the loss too");
    assert!(war_parties(&r).is_empty(), "the band is gone");
    assert_eq!(r.c.factions[r.f as usize].away, 0);
    let t = arrival_at(&r).expect("a wait is booked");
    assert!(t > r.c.now(), "the wait is still running");
    ok(&mut r);
    r.run(t + 10);
    assert_eq!(r.c.stats.arrivals, 1);
    ok(&mut r);
}

fn war_parties(r: &CampRun) -> Vec<Handle<Party>> {
    r.c.parties
        .iter()
        .filter(|(_, x)| x.faction == r.f && x.kind.is_war())
        .map(|(h, _)| h)
        .collect()
}

/// Walls that hold no one: every raid on a district is lost to the last unit.
struct Walls;

impl Infrastructure for Walls {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        Some(RaidOutcome {
            survived: 0.0,
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

/// A Frostmaw lost to a district's defences, abstractly, with no one watching, leaves no unit
/// alive and loses its chief with it (MZB-266): M4 kept the chief through the wipe-out, so a
/// Frostmaw the walls killed came back as the same chief. It is a loss a player's district caused:
/// the pressure is bumped once and the den waits 12 days.
#[test]
fn a_frostmaw_lost_to_a_districts_walls_loses_its_chief_and_is_a_player_loss() {
    let mut r = den(57, 1);
    r.infra = Box::new(Walls);
    come_in(&mut r);
    let chief = r.c.factions[r.f as usize].chief;
    r.raid().expect("a raid goes");
    r.run(Tick(days(4.0)));
    assert_eq!(r.c.stats.raids_district, 1, "the walls were met");
    assert_eq!(r.c.stats.chiefs_killed, 1);
    assert_ne!(r.c.factions[r.f as usize].chief, chief);
    assert!(
        r.c.factions[r.f as usize].cowed_until > Tick(1),
        "cowed by it"
    );
    assert_eq!(pressure_of(&r), 1.0);
    assert!(war_parties(&r).is_empty());
    let t = arrival_at(&r).expect("a wait is booked");
    assert!(t > r.c.now());
    ok(&mut r);
    r.run(t + 10);
    assert_eq!(r.c.stats.arrivals, 1);
    ok(&mut r);
}

/// A Dead slot's body killed later is no second fall of the chief (MZB-265): the Frostmaw that died
/// away with its body left in the camp fell once, when it died.
#[test]
fn a_dead_slots_chief_body_is_not_felled_twice() {
    let mut r = den(58, 1);
    r.plugin.fight = Some(Fight {
        kill_share: 1.0,
        after: 200,
        kill_chief: false,
        only_at_base: false,
    });
    come_in(&mut r);
    r.watch = true;
    r.raid().expect("a raid goes");
    r.camp_out();
    r.run(Tick(days(4.0)));
    r.watch = false;
    assert_eq!(counts(&r)[4], 1, "its slot is dead, its body still stands");
    let falls = r.c.stats.chiefs_killed;
    let b = r.plugin.camps[&r.key].bodies[0];
    assert!(b.chief, "the body is the chief's");
    let at = r.c.now() + 1;
    assert!(r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at));
    let to = r.c.now() + 5;
    r.run(to);
    assert_eq!(r.c.stats.chiefs_killed, falls, "no second fall");
    ok(&mut r);
}
