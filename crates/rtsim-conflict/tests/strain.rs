// SPDX-License-Identifier: AGPL-3.0-or-later
//! S7, conflict pressure (EQUILIBRIUM.md §2.4.5, docs/overnight-choices-equilibrium.md EQC200 on):
//! a pool camp's recruiting slows by `2^-P` while a player's kills stand, a camp that is thin and
//! under heavy pressure breaks, and a broken camp is refounded after a wait. The body camps (the
//! Frostmaw's den, the Umvuthana's grove) are S9's and keep what they book today.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::culture::SignalKind;
use rtsim_conflict::culture::{
    BANDIT_JSON, Culture, CultureDef, FROSTMAW_JSON, GOBLIN_JSON, RAIDER_JSON, UMVUTHANA_JSON,
};
use rtsim_conflict::engine::{BLOCKED, Demand, Party, Phase, Signal, Target, body_id};
use rtsim_conflict::synth::CampRun;
use rtsim_conflict::{Conflict, ConflictTunables, Cultures, synth};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome};
use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::sites::{FoundRow, state_flags};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

fn def(json: &str) -> CultureDef {
    serde_json::from_str(json).unwrap()
}

/// `json` with `edit` applied, resolved: the error text when the culture is refused.
fn refused(json: &str, edit: impl FnOnce(&mut CultureDef)) -> Option<String> {
    let mut d = def(json);
    edit(&mut d);
    // Resolved from the definition itself: JSON has no NaN to carry.
    Culture::resolve(d).err()
}

// ---------------------------------------------------------------- culture data (EQC202)

/// EQC202: the three pool cultures carry the block with the decided numbers, the two body
/// cultures do not, and each way the block can be wrong is its own error.
#[test]
fn pool_renewal_parses_and_validates() {
    let c = Cultures::builtin();
    for n in ["goblin", "bandit", "raider"] {
        let cu = c.get(c.index_of(n).unwrap());
        let r = cu
            .pool_renewal
            .unwrap_or_else(|| panic!("{n} carries a pool_renewal"));
        // FB-1 (a): a half-life of 192 days, the break at P 2.
        assert_eq!(r.def.half_life_days, 192.0, "{n}");
        assert_eq!(r.def.break_pressure, 2.0, "{n}");
        assert_eq!(r.def.recruit_cap_days, 96.0, "{n}");
        assert_eq!(r.def.break_share, 0.1, "{n}");
        assert_eq!(
            (
                r.def.refound_base_days,
                r.def.refound_cap_days,
                r.def.refound_share
            ),
            (48.0, 384.0, 0.25),
            "{n}"
        );
        // The recruiting rule's base is the culture's own wait for a warrior: at no pressure the
        // delay is 1 / recruits_per_day, and it stops growing at the cap.
        assert!(
            (r.recruit.base_days as f64 - 1.0 / cu.def.recruits_per_day).abs() < 1e-6,
            "{n}: base {}",
            r.recruit.base_days
        );
        assert_eq!(r.recruit.cap_days, 96.0, "{n}");
        assert_eq!(r.recruit.half_life_days, 192.0, "{n}");
        assert_eq!(r.refound.base_days, 48.0, "{n}");
        assert_eq!(r.refound.cap_days, 384.0, "{n}");
        // A pool camp has no renewal of the members' and no bodies.
        assert!(cu.renewal.is_none() && !cu.def.camp_bodies, "{n}");
    }
    for n in ["frostmaw", "umvuthana"] {
        assert!(c.get(c.index_of(n).unwrap()).pool_renewal.is_none(), "{n}");
    }

    // Each wrong block is its own error.
    let e = refused(GOBLIN_JSON, |d| d.camp_bodies = true).expect("a body camp is refused");
    assert!(
        e.contains("pool_renewal is for a camp with no bodies"),
        "{e}"
    );
    let e = refused(GOBLIN_JSON, |d| d.recruits_per_day = 0.0).expect("no rate is refused");
    assert!(e.contains("recruits_per_day"), "{e}");
    let e = refused(FROSTMAW_JSON, |d| {
        d.pool_renewal = def(GOBLIN_JSON).pool_renewal
    })
    .expect("a renewal and a pool_renewal are refused");
    assert!(e.contains("pool_renewal"), "{e}");
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let e = refused(BANDIT_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().half_life_days = bad
        })
        .expect("not finite");
        assert!(e.contains("finite"), "{e}");
        let e = refused(BANDIT_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().refound_share = bad
        })
        .expect("not finite");
        assert!(e.contains("finite"), "{e}");
    }
    for bad in [0.0, 1.0, -0.1, 1.5] {
        let e = refused(RAIDER_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().break_share = bad
        })
        .expect("break_share outside (0, 1)");
        assert!(e.contains("break_share"), "{e}");
    }
    for bad in [0.0, -0.5, 1.01] {
        let e = refused(RAIDER_JSON, |d| {
            d.pool_renewal.as_mut().unwrap().refound_share = bad
        })
        .expect("refound_share outside (0, 1]");
        assert!(e.contains("refound_share"), "{e}");
    }
    // One is a whole band: allowed.
    assert_eq!(
        refused(RAIDER_JSON, |d| d
            .pool_renewal
            .as_mut()
            .unwrap()
            .refound_share = 1.0),
        None
    );
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().break_pressure = 0.0
    })
    .expect("break_pressure above 0");
    assert!(e.contains("break_pressure"), "{e}");
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().half_life_days = 0.0
    })
    .expect("half-life above 0");
    assert!(e.contains("half_life_days"), "{e}");
    // A refound cap under its base, and a recruit cap under the culture's own wait (1 / 2 days).
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().refound_cap_days = 47.0
    })
    .expect("cap under base");
    assert!(e.contains("cap_days"), "{e}");
    assert_eq!(
        refused(GOBLIN_JSON, |d| d
            .pool_renewal
            .as_mut()
            .unwrap()
            .refound_cap_days = 48.0),
        None,
        "a cap equal to its base is allowed"
    );
    let e = refused(GOBLIN_JSON, |d| {
        d.pool_renewal.as_mut().unwrap().recruit_cap_days = 0.4
    })
    .expect("recruit cap under the culture's wait");
    assert!(e.contains("cap_days"), "{e}");
}

/// The body cultures' files carry no `pool_renewal` key at all (S9 owns them), and the document
/// round-trips through the definition type without gaining one.
#[test]
fn the_body_cultures_files_are_untouched() {
    for json in [FROSTMAW_JSON, UMVUTHANA_JSON] {
        assert!(!json.contains("pool_renewal"));
        assert!(def(json).pool_renewal.is_none());
    }
}

// ---------------------------------------------------------------- the recruit rate (EQC200, EQC203)

const DAY: u64 = TICKS_PER_DAY;

/// One camp of `culture` on the built-in cultures, in the synthetic world.
fn camp(seed: u64, culture: &str) -> (World, Conflict, u32) {
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(
        seed,
        ConflictTunables::default(),
        Cultures::builtin(),
        Tick::ZERO,
    );
    synth::define_names(&mut c, &w);
    c.log_enabled = true;
    let f = c
        .add_faction(&mut w, synth::CAMP, culture, synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    (w, c, f)
}

/// The pool is empty from `at`, as if its warriors had been lost at no pressure.
fn empty_pool(c: &mut Conflict, f: u32, at: Tick) {
    let x = &mut c.factions[f as usize];
    (x.pool, x.pool_at) = (0.0, at);
}

fn pressure(c: &Conflict, f: u32, at: Tick) -> f64 {
    f64::from(c.pressure_now(f, at))
}

/// Only a pool culture has a strain: the goblin, bandit and raider camps do, the den and the grove
/// do not (their pressure is their ledger's, S9's).
#[test]
fn only_a_pool_camp_has_a_strain() {
    for (culture, has) in [
        ("goblin", true),
        ("bandit", true),
        ("raider", true),
        ("frostmaw", false),
        ("umvuthana", false),
    ] {
        let (_, c, f) = camp(3, culture);
        assert_eq!(c.factions[f as usize].strain.is_some(), has, "{culture}");
    }
}

/// EQC203: a camp under pressure P recruits at `r x 2^-P` a day, which is the shared rule's wait
/// `min(cap, base x 2^P)` for its next warrior: a goblin camp (r 2) at P 1 recruits one a day, at
/// P 2 half of one.
#[test]
fn recruits_slow_by_two_to_the_p() {
    let (mut w, mut c, f) = camp(11, "goblin");
    let t0 = Tick(10 * DAY);
    // 60 warriors lost at once is P = 60 / cap 60 = 1.
    assert_eq!(c.slay(&mut w, f, 60, t0), 60);
    assert!((pressure(&c, f, t0) - 1.0).abs() < 1e-6);
    assert!(c.pool_at(f, t0) < 1e-9, "the pool is empty after the kills");
    let day = c.pool_at(f, Tick(t0.0 + DAY));
    assert!((day - 1.0).abs() < 1e-6, "one a day at P 1: {day}");
    // The rate is the rule's: 1 / delay_days at the anchor's pressure.
    let cu = c.cultures.get(c.factions[f as usize].culture);
    let rule = cu.pool_renewal.unwrap().recruit;
    let p = c.factions[f as usize].strain.as_ref().unwrap().pressure;
    let want = 1.0 / f64::from(rule.delay_days(p, (t0.0 / DAY) as u32));
    assert!(
        (c.recruit_rate(f) * DAY as f64 - want).abs() < 1e-6,
        "{want}"
    );
    // Another 60 on top: P 2 (the bump re-anchors first), a half a day.
    assert_eq!(c.slay(&mut w, f, 60, Tick(t0.0 + 60 * DAY)), 60);
    // (the pool had recruited back to 60 in the 60 days at P 1; P decayed by 2^(-60/192).)
    let p2 = pressure(&c, f, Tick(t0.0 + 60 * DAY));
    let want_p = 2f64.powf(-60.0 / 192.0) + 1.0;
    assert!((p2 - want_p).abs() < 1e-5, "{p2} vs {want_p}");
    let half = c.pool_at(f, Tick(t0.0 + 61 * DAY));
    assert!((half - 2.0 * (-p2).exp2()).abs() < 1e-4, "{half}");
}

/// Without a strain (the culture has none), the rate is the culture's, bit for bit.
#[test]
fn a_culture_with_no_strain_recruits_at_its_rate() {
    for culture in ["frostmaw", "umvuthana"] {
        let (_, c, f) = camp(5, culture);
        let r = c
            .cultures
            .get(c.factions[f as usize].culture)
            .def
            .recruits_per_day;
        assert_eq!(c.recruit_rate(f), r / DAY as f64, "{culture}");
    }
    // And the pool cultures at no pressure: exactly the culture's.
    for culture in ["goblin", "bandit", "raider"] {
        let (_, c, f) = camp(5, culture);
        let r = c
            .cultures
            .get(c.factions[f as usize].culture)
            .def
            .recruits_per_day;
        assert_eq!(c.recruit_rate(f), r / DAY as f64, "{culture}");
    }
}

/// EQC203: however high the pressure, the camp recruits at least one a `recruit_cap_days` (96).
#[test]
fn the_rate_floor_holds_at_absurd_pressure() {
    let (_, mut c, f) = camp(7, "goblin");
    let t0 = Tick(5 * DAY);
    empty_pool(&mut c, f, t0);
    c.factions[f as usize].strain.as_mut().unwrap().pressure = Pressure::from_parts(60.0, 5);
    let rate = c.recruit_rate(f) * DAY as f64;
    assert!((rate - 1.0 / 96.0).abs() < 1e-12, "{rate}");
    let got = c.pool_at(f, Tick(t0.0 + 96 * DAY));
    assert!((got - 1.0).abs() < 1e-9, "one warrior in 96 days: {got}");
}

/// EQC200: a bump anchors the pool first, so the gap before a loss recruits at the pressure it was
/// read at, and the line after it is slower.
#[test]
fn a_bump_integrates_the_gap_at_the_old_pressure() {
    let (mut w, mut c, f) = camp(13, "goblin");
    let t0 = Tick(2 * DAY);
    empty_pool(&mut c, f, t0);
    // Ten days at no pressure: 2 a day.
    let t1 = Tick(t0.0 + 10 * DAY);
    assert!((c.pool_at(f, t1) - 20.0).abs() < 1e-9);
    assert_eq!(c.slay(&mut w, f, 10, t1), 10);
    assert!(
        (c.pool_at(f, t1) - 10.0).abs() < 1e-9,
        "the gap at the old rate, then 10 killed"
    );
    let p = pressure(&c, f, t1);
    assert!((p - 10.0 / 60.0).abs() < 1e-6, "{p}");
    let next = c.pool_at(f, Tick(t1.0 + DAY));
    assert!((next - (10.0 + 2.0 * (-p).exp2())).abs() < 1e-4, "{next}");
}

/// `pool_ready` is the time `pool_at` reaches `n`, whatever the pressure, the pool and the room
/// (the three readers of the rate agree).
#[test]
fn pool_ready_agrees_with_pool_at() {
    let (_, mut c, f) = camp(17, "bandit");
    for pressure_p in [0.0f32, 0.3, 1.0, 2.5, 9.0, 80.0] {
        for pool in [0.0f64, 0.4, 3.0, 17.5] {
            for away in [0u32, 6] {
                let t0 = Tick(3 * DAY + 77);
                let x = &mut c.factions[f as usize];
                (x.pool, x.pool_at, x.away) = (pool, t0, away);
                x.strain.as_mut().unwrap().pressure =
                    Pressure::from_parts(pressure_p, (t0.0 / DAY) as u32);
                let room = f64::from(c.factions[f as usize].cap - away);
                for n in [1.0, 2.0, 5.0, 12.0, 30.0, room, room + 1.0] {
                    let ready = c.pool_ready(f, n, t0);
                    if n > room {
                        assert_eq!(ready, BLOCKED, "more than the room");
                        continue;
                    }
                    if pool >= n {
                        assert_eq!(ready, t0);
                        continue;
                    }
                    assert!(ready > t0 && ready < BLOCKED, "n {n}: {ready:?}");
                    // Held at the wake: the closed form has reached n (to the tick's rounding).
                    let have = c.pool_at(f, ready);
                    assert!(
                        have >= n - 1e-6,
                        "P {pressure_p} pool {pool} n {n}: have {have}"
                    );
                    // And two ticks before it, not yet (the wake is not early); a float's last bit
                    // is no earlier.
                    let before = c.pool_at(f, Tick(ready.0 - 2));
                    assert!(
                        before < n + 1e-9,
                        "P {pressure_p} pool {pool} n {n}: early {before}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------- a player's losses (EQC204, EQC205)

fn next_op() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0x5700_0000);
    N.fetch_add(1, Ordering::Relaxed)
}

/// A raid of faction `f` on `target`, ordered at `at`, its members embodied at a new epoch as a
/// watched fight leaves them: the casualties below count against that epoch.
fn embodied_raid(
    c: &mut Conflict,
    w: &mut World,
    f: u32,
    target: Target,
    at: Tick,
) -> (Handle<Party>, u32) {
    let key = c.factions[f as usize].key;
    let p = c
        .order_party(w, key, PartyKind::Raid, target, None, at)
        .expect("a raid");
    let x = &mut c.parties[p];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    (p, x.epoch)
}

/// A BAND_CASUALTIES for `count` of the party's first unit, credited as `by` says.
fn casualty(c: &Conflict, p: Handle<Party>, epoch: u32, by: u32, count: u32, op: u64) -> Obs {
    let f = c.parties[p].faction;
    let cu = c.cultures.get(c.factions[f as usize].culture);
    let body = body_id(&c.cultures, &cu.unit(c.parties[p].units[0].0).body);
    Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(p),
        epoch,
        by,
        body,
        count,
        killer: (0, 0),
        op,
    })
}

fn ingest(c: &mut Conflict, w: &mut World, o: &Obs, at: Tick) {
    c.ingest(w, &mut NoInfrastructure, o, at);
}

fn district() -> Target {
    Target::District(synth::REFINERY, synth::REFINERY_POS)
}

/// EQC204: each warrior a player kills adds `1 / cap`, and the party's loss is the pool camp's.
#[test]
fn a_credited_kill_of_a_party_member_adds_one_over_cap() {
    let (mut w, mut c, f) = camp(21, "goblin");
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    let player = c.player(&mut w, synth::PLAYER);
    let _ = player;
    let at = Tick(200);
    let o = casualty(&c, p, e, boundary::BY_PLAYER, 3, next_op());
    ingest(&mut c, &mut w, &o, at);
    let got = pressure(&c, f, at);
    assert!((got - 3.0 / 60.0).abs() < 1e-6, "three of cap 60: {got}");
    // A taken member (a tame, a lead) is a player's too.
    let o = casualty(&c, p, e, boundary::BY_TAKEN, 1, next_op());
    ingest(&mut c, &mut w, &o, at);
    assert!((pressure(&c, f, at) - 4.0 / 60.0).abs() < 1e-6);
}

/// EQC204: a mob's kill is a predator's, and adds nothing.
#[test]
fn a_mob_kill_adds_nothing() {
    let (mut w, mut c, f) = camp(22, "goblin");
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    let o = casualty(&c, p, e, boundary::BY_MOB, 4, next_op());
    ingest(&mut c, &mut w, &o, Tick(200));
    assert_eq!(pressure(&c, f, Tick(200)), 0.0);
    assert_eq!(c.parties[p].alive(), 12 - 4, "the members did fall");
}

/// EQC204, FB-2 (a): a death that credits no one at a player's district (a trap, a turret, a
/// FakePlayer's kill) is that player's loss: a band fighting or waiting there, as a fight at its
/// walls leaves it. (EQC291: a band still mustering at its camp is not at the district.)
#[test]
fn an_unclaimed_death_at_a_players_district_is_his() {
    for phase in [Phase::Fighting, Phase::Waiting] {
        for by in [boundary::BY_NONE, boundary::BY_OTHER] {
            let (mut w, mut c, f) = camp(23, "goblin");
            let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
            c.parties[p].phase = phase;
            let o = casualty(&c, p, e, by, 2, next_op());
            ingest(&mut c, &mut w, &o, Tick(200));
            let got = pressure(&c, f, Tick(200));
            assert!((got - 2.0 / 60.0).abs() < 1e-6, "{phase:?} by {by}: {got}");
        }
    }
    // The switch off: it is no one's.
    let (mut w, mut c, f) = camp(23, "goblin");
    c.tun.district_unclaimed_is_players = false;
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    c.parties[p].phase = Phase::Fighting;
    let o = casualty(&c, p, e, boundary::BY_NONE, 2, next_op());
    ingest(&mut c, &mut w, &o, Tick(200));
    assert_eq!(pressure(&c, f, Tick(200)), 0.0);
}

/// A raid of `f` on the refinery, run in steps of 60 ticks until `want` holds of the band (its
/// phase and its distance from the refinery, blocks), then embodied at a new epoch as a watched
/// fight leaves it. `NoInfrastructure`: the band raids nothing and walks home.
fn raid_where(
    seed: u64,
    want: impl Fn(Phase, f64) -> bool,
) -> (World, Conflict, u32, Handle<Party>, u32) {
    let (mut w, mut c, f) = camp(seed, "goblin");
    let key = c.factions[f as usize].key;
    let p = c
        .order_party(&mut w, key, PartyKind::Raid, district(), None, Tick(21))
        .expect("a raid");
    let mut t = Tick(21);
    loop {
        t = Tick(t.0 + 60);
        assert!(t.0 < 10 * DAY, "the band never came to where the test wants it");
        c.run_until(&mut w, &mut NoInfrastructure, t);
        assert!(c.parties.contains(p), "the band is gone");
        let (x, z) = c.party_pos(p, t);
        let d = f64::from(x - synth::REFINERY_POS.x).hypot(f64::from(z - synth::REFINERY_POS.z));
        if want(c.parties[p].phase, d) {
            break;
        }
    }
    let x = &mut c.parties[p];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    let e = x.epoch;
    (w, c, f, p, e)
}

fn out_far(p: Phase, d: f64) -> bool {
    p == Phase::Out && d > 200.0
}
fn home_far(p: Phase, d: f64) -> bool {
    p == Phase::Home && d > 200.0
}
fn out_near(p: Phase, d: f64) -> bool {
    p == Phase::Out && d < 100.0
}
fn home_near(p: Phase, d: f64) -> bool {
    p == Phase::Home && d < 100.0
}

/// FB-2 (a), EQC291: a band bound for a player's district that loses a member to a fall, a death
/// no one dealt, on the road out or walking home is not at the district: no one's loss. Walking
/// in or just off the district it is his, as it is fighting or waiting there.
#[test]
fn an_unclaimed_death_on_the_road_to_a_district_is_no_ones() {
    type Want = fn(Phase, f64) -> bool;
    for (what, near, want) in [
        ("on the road out", false, out_far as Want),
        ("walking home", false, home_far as Want),
        ("walking in", true, out_near as Want),
        ("walking off", true, home_near as Want),
    ] {
        for by in [boundary::BY_NONE, boundary::BY_OTHER] {
            let (mut w, mut c, f, p, e) = raid_where(24, want);
            let at = c.now();
            let o = casualty(&c, p, e, by, 2, next_op());
            ingest(&mut c, &mut w, &o, at);
            let got = pressure(&c, f, at);
            if near {
                assert!((got - 2.0 / 60.0).abs() < 1e-6, "{what}, by {by}: {got}");
            } else {
                assert_eq!(got, 0.0, "{what}, by {by}");
            }
        }
    }
}

/// EQC291: a band still mustering at its camp is not at the district it is bound for.
#[test]
fn an_unclaimed_death_while_mustering_is_no_ones() {
    let (mut w, mut c, f) = camp(23, "goblin");
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    assert_eq!(c.parties[p].phase, Phase::Mustering);
    for by in [boundary::BY_NONE, boundary::BY_OTHER] {
        let o = casualty(&c, p, e, by, 1, next_op());
        ingest(&mut c, &mut w, &o, Tick(200));
    }
    assert_eq!(pressure(&c, f, Tick(200)), 0.0);
}

#[test]
fn an_unclaimed_death_on_the_road_is_not() {
    for target in [Target::Node(0), Target::Pos(synth::FOREST_POS)] {
        for by in [boundary::BY_NONE, boundary::BY_OTHER] {
            let (mut w, mut c, f) = camp(24, "goblin");
            let (p, e) = embodied_raid(&mut c, &mut w, f, target, Tick(21));
            let o = casualty(&c, p, e, by, 2, next_op());
            ingest(&mut c, &mut w, &o, Tick(200));
            assert_eq!(pressure(&c, f, Tick(200)), 0.0, "{target:?} by {by}");
        }
    }
}

/// EQC205: a casualty is booked once, whatever replays it (its op id), and a copy of an ended
/// session's record books nothing the original did not.
#[test]
fn a_replayed_casualty_bumps_once() {
    let (mut w, mut c, f) = camp(25, "goblin");
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    let op = next_op();
    let o = casualty(&c, p, e, boundary::BY_PLAYER, 2, op);
    ingest(&mut c, &mut w, &o, Tick(200));
    ingest(&mut c, &mut w, &o, Tick(210));
    ingest(&mut c, &mut w, &o, Tick(220));
    let got = pressure(&c, f, Tick(220));
    assert!((got - 2.0 / 60.0).abs() < 1e-6, "{got}");
}

/// EQC204: a band that is not a war band (an envoy) is not the camp's warriors away: nothing.
#[test]
fn a_killed_envoy_adds_nothing() {
    let (mut w, mut c, f) = camp(26, "goblin");
    let key = c.factions[f as usize].key;
    let p = c
        .order_party(
            &mut w,
            key,
            PartyKind::Envoy,
            Target::Pos(synth::FOREST_POS),
            None,
            Tick(21),
        )
        .expect("an envoy");
    let x = &mut c.parties[p];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    let e = x.epoch;
    let o = casualty(&c, p, e, boundary::BY_PLAYER, 1, next_op());
    ingest(&mut c, &mut w, &o, Tick(200));
    assert_eq!(pressure(&c, f, Tick(200)), 0.0);
}

/// EQC204: the war chief killed by a player is one member.
#[test]
fn a_war_chief_killed_by_a_player_adds_one_over_cap() {
    let (mut w, mut c, f) = camp(27, "goblin");
    let (p, _) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    assert!(c.parties[p].chief, "the chief leads the raid");
    let player = c.player(&mut w, synth::PLAYER);
    c.chief_killed_by(
        &mut w,
        p,
        Some(player),
        boundary::BY_PLAYER,
        None,
        Tick(200),
    );
    let got = pressure(&c, f, Tick(200));
    assert!((got - 1.0 / 60.0).abs() < 1e-6, "{got}");
}

/// EQC291: the war chief of a pool camp killed by a machine (a death no one is credited with) is
/// one member lost, and is the player's at the district as any member's is; on the road it is
/// no one's; a mob's kill is a predator's at either.
#[test]
fn a_war_chief_killed_by_a_machine_at_the_district_is_his() {
    for by in [boundary::BY_NONE, boundary::BY_OTHER] {
        let (mut w, mut c, f) = camp(27, "goblin");
        let (p, _) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
        c.parties[p].phase = Phase::Fighting;
        c.chief_killed_by(&mut w, p, None, by, None, Tick(200));
        let got = pressure(&c, f, Tick(200));
        assert!((got - 1.0 / 60.0).abs() < 1e-6, "at the district, by {by}: {got}");
        // On the road out, far from the district: no one's.
        let (mut w, mut c, f, p, _) = raid_where(27, out_far);
        let at = c.now();
        c.chief_killed_by(&mut w, p, None, by, None, at);
        assert_eq!(pressure(&c, f, at), 0.0, "on the road, by {by}");
    }
    let (mut w, mut c, f) = camp(27, "goblin");
    let (p, _) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    c.parties[p].phase = Phase::Fighting;
    c.chief_killed_by(&mut w, p, None, boundary::BY_MOB, None, Tick(200));
    assert_eq!(pressure(&c, f, Tick(200)), 0.0, "a mob's kill");
}

/// The den and the grove book what they booked: S9's, not S7's.
#[test]
fn the_body_cultures_book_what_they_booked() {
    for culture in ["frostmaw", "umvuthana"] {
        let (mut w, mut c, f) = camp(28, culture);
        let key = c.factions[f as usize].key;
        let Some(p) = c.order_party(&mut w, key, PartyKind::Raid, district(), None, Tick(21))
        else {
            continue;
        };
        let x = &mut c.parties[p];
        x.epoch += 1;
        x.embodied = true;
        x.spawned = x.units.clone();
        let e = x.epoch;
        let o = casualty(&c, p, e, boundary::BY_PLAYER, 1, next_op());
        ingest(&mut c, &mut w, &o, Tick(200));
        assert_eq!(
            pressure(&c, f, Tick(200)),
            0.0,
            "{culture}: a casualty books no pressure"
        );
        assert!(c.factions[f as usize].strain.is_none());
    }
}

/// The test hook takes the pool first, then the parties' members, each a player's loss.
#[test]
fn slay_takes_the_pool_then_the_parties() {
    let (mut w, mut c, f) = camp(29, "goblin");
    let (p, _) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    // 60 at the start, 12 away: the pool is 48 less what the raid's draw took.
    let home = c.pool_at(f, Tick(100)).floor() as u32;
    let away = c.factions[f as usize].away;
    assert_eq!(home + away, 60);
    let at = Tick(100);
    assert_eq!(c.slay(&mut w, f, home, at), home);
    assert_eq!(c.parties[p].alive(), 12, "the pool first");
    assert_eq!(c.slay(&mut w, f, 5, at), 5);
    assert_eq!(c.parties[p].alive(), 7, "then the party");
    assert_eq!(c.factions[f as usize].away, 7);
    let got = pressure(&c, f, at);
    assert!((got - f64::from(home + 5) / 60.0).abs() < 1e-5, "{got}");
    // Everything: u32::MAX takes what is left, and no more.
    assert_eq!(c.slay(&mut w, f, u32::MAX, at), 7);
    assert_eq!(c.parties[p].alive(), 0);
    assert_eq!(c.slay(&mut w, f, u32::MAX, at), 0);
}

/// Walls that hold half of every attacking class and let the rest fall: every district raid is
/// resolved, at the share of the fallen the tunable says.
struct Walls;

impl Infrastructure for Walls {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        Some(RaidOutcome {
            survived: 0.5,
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

/// Goblins that only raid, on the built-in numbers.
fn raiders(seed: u64) -> (World, Conflict, u32) {
    let mut d: CultureDef = def(GOBLIN_JSON);
    d.plunder_mean_days = None;
    d.parties.retain(|k, _| k == "raid");
    let cultures = Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap();
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(seed, ConflictTunables::default(), cultures, Tick::ZERO);
    synth::define_names(&mut c, &w);
    let f = c
        .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    (w, c, f)
}

/// EQC204: a band an unwatched raid on a player's district loses at its walls is a player's loss
/// (the district resolve already says so), and now the camp's pressure takes it: the share that fell,
/// over the cap.
#[test]
fn a_raid_lost_at_a_players_walls_adds_the_lost_share() {
    let (mut w, mut c, f) = raiders(31);
    c.tun.abstract_loss_share = 1.0;
    let key = c.factions[f as usize].key;
    let p = c
        .order_party(&mut w, key, PartyKind::Raid, district(), None, Tick(21))
        .expect("a raid");
    let _ = p;
    c.run_until(&mut w, &mut Walls, Tick(10 * DAY));
    assert_eq!(
        c.stats.raids_district, 2,
        "a warrior class and an archer class: {:?}",
        c.stats
    );
    assert_eq!(c.stats.casualties, 6, "half of 8 and half of 4 fell");
    let got = pressure(&c, f, Tick(10 * DAY));
    // The loss was booked on the day of the raid, and has decayed some since.
    assert!(got > 0.0 && got <= 6.0 / 60.0, "{got}");
    let booked = c.factions[f as usize]
        .strain
        .as_ref()
        .unwrap()
        .pressure
        .parts();
    assert!((booked.0 - 6.0 / 60.0).abs() < 1e-6, "{booked:?}");
}

/// EQC204: a raid a village's defenders beat is the village's doing, not a player's: nothing.
#[test]
fn a_raid_lost_to_a_village_adds_nothing() {
    let (mut w, mut c, f) = raiders(32);
    let ashby = w.node_of_key(4).expect("Ashby");
    w.nodes[ashby].local.pop = 4_000;
    // Ashby stands 40 items over each line's target, so the raiders find something to carry off
    // and fight for it: a raid takes only the surplus over a target (CL-D58).
    let m = w.market(ashby);
    let rich: std::collections::BTreeMap<GoodId, i64> = w
        .goods
        .ids()
        .map(|g| (g, m.line(g).target + 40 * MILLI - m.stock(g)))
        .filter(|e| e.1 > 0)
        .collect();
    w.stock_from_players(ashby, &rich, Tick(10));
    let key = c.factions[f as usize].key;
    c.order_party(
        &mut w,
        key,
        PartyKind::Raid,
        Target::Node(ashby),
        None,
        Tick(21),
    )
    .expect("a raid");
    c.run_until(&mut w, &mut NoInfrastructure, Tick(30 * DAY));
    assert!(
        c.stats.casualties > 0,
        "the village's defenders killed some: {:?}",
        c.stats
    );
    assert_eq!(pressure(&c, f, Tick(30 * DAY)), 0.0);
}

// ---------------------------------------------------------------- a camp breaks (EQC207, EQC208)

/// A goblin camp with `pool` warriors at `at` under pressure `p` as of that day: the state a break
/// is looked at from.
fn strained(c: &mut Conflict, f: u32, pool: f64, p: f32, at: Tick) {
    let x = &mut c.factions[f as usize];
    (x.pool, x.pool_at) = (pool, at);
    x.strain.as_mut().unwrap().pressure = Pressure::from_parts(p, (at.0 / DAY) as u32);
}

fn broken(c: &Conflict, f: u32) -> Option<rtsim_conflict::engine::Broken> {
    c.factions[f as usize].strain.as_ref().unwrap().broken
}

/// EQC207: thin (under a tenth of the cap, home and away), pressure at 2 or more, nothing out: the
/// camp breaks, for `min(384, 48 x 2^P)` days. No warriors, no recruits.
#[test]
fn a_camp_breaks_when_thin_at_p_two() {
    let (mut w, mut c, f) = camp(41, "goblin");
    let at = Tick(30 * DAY);
    // 3 home, P just under 2: the next kill tips it.
    strained(&mut c, f, 3.0, 1.99, at);
    assert_eq!(c.slay(&mut w, f, 1, at), 1);
    let b = broken(&c, f).expect("the camp broke");
    assert_eq!(b.since, at);
    let p = pressure(&c, f, at);
    assert!(p >= 2.0, "{p}");
    let wait = (48.0 * p.exp2()).min(384.0);
    let got = (b.due.0 - b.since.0) as f64 / DAY as f64;
    assert!((got - wait).abs() < 1e-3, "waits {got} days, want {wait}");
    assert_eq!(c.pool_at(f, at), 0.0);
    assert_eq!(c.recruit_rate(f), 0.0);
    assert_eq!(
        c.pool_at(f, Tick(at.0 + 90 * DAY)),
        0.0,
        "no recruits while broken"
    );
    assert!(!c.camp_active(f));
    assert_eq!(c.stats.breaks, 1);
    // The booked wake is at the due tick.
    assert_eq!(c.pending_by_kind().get("strain"), Some(&1));
}

/// EQC207: not at 2, or not thin (a tenth is not thin: strict), no break.
#[test]
fn a_camp_does_not_break_under_p_two_or_when_not_thin() {
    let at = Tick(30 * DAY);
    // Pressure under 2 after the kill (a kill adds a sixtieth): 1.9 to 1.9167.
    let (mut w, mut c, f) = camp(42, "goblin");
    strained(&mut c, f, 3.0, 1.9, at);
    assert_eq!(c.slay(&mut w, f, 1, at), 1);
    assert!(broken(&c, f).is_none(), "P under 2");
    // Not thin: 8 home of 60 is over a tenth.
    let (mut w, mut c, f) = camp(42, "goblin");
    strained(&mut c, f, 9.0, 2.5, at);
    assert_eq!(c.slay(&mut w, f, 1, at), 1);
    assert!(broken(&c, f).is_none(), "8 home of 60");
    // Exactly a tenth is not thin; one under it is.
    let (mut w, mut c, f) = camp(42, "goblin");
    strained(&mut c, f, 7.0, 2.5, at);
    assert_eq!(c.slay(&mut w, f, 1, at), 1, "7 to 6 home");
    assert!(
        broken(&c, f).is_none(),
        "6 of 60 is exactly a tenth: not thin"
    );
    assert_eq!(c.slay(&mut w, f, 1, at), 1, "6 to 5 home");
    assert!(broken(&c, f).is_some(), "5 of 60 is thin");
    assert_eq!(c.stats.breaks, 1);
}

/// EQC208: a camp with a party out is not idle: the break waits a game day and looks again, and
/// goes when the party is home.
#[test]
fn a_camp_with_a_party_out_waits_and_breaks_when_it_ends() {
    let (mut w, mut c, f) = raiders(43);
    let at = Tick(30 * DAY);
    let key = c.factions[f as usize].key;
    // A raid of 12 out, then the pool and all but two of its members killed.
    let p = c
        .order_party(&mut w, key, PartyKind::Raid, district(), None, at)
        .expect("a raid");
    let pool = c.pool_at(f, at);
    strained(&mut c, f, pool, 1.5, at);
    let home = c.pool_at(f, at).floor() as u32;
    assert_eq!(c.slay(&mut w, f, home + 10, at), home + 10);
    assert_eq!(c.parties[p].alive(), 2);
    assert!(broken(&c, f).is_none(), "a party is out: not idle");
    assert!(c.stats.break_deferrals >= 1);
    let check = c.factions[f as usize].strain.as_ref().unwrap().check_at;
    assert_eq!(check, Some(Tick(at.0 + 24_000)), "a day on");
    // Run until the party is home: the camp breaks then or at a recheck, never with a party out.
    let mut t = at;
    while broken(&c, f).is_none() {
        t = Tick(t.0 + 1_200);
        assert!(t.0 < at.0 + 60 * DAY, "it never broke");
        c.run_until(&mut w, &mut NoInfrastructure, t);
        if broken(&c, f).is_some() {
            assert!(
                c.parties.iter().all(|(_, x)| x.faction != f),
                "the camp broke with a party out"
            );
        }
    }
    assert_eq!(c.stats.breaks, 1);
    assert!(
        c.factions[f as usize]
            .strain
            .as_ref()
            .unwrap()
            .check_at
            .is_none()
    );
}

/// EQC208: an envoy's open demand is a feud in the middle of something: the break waits.
#[test]
fn a_demand_open_defers_the_break() {
    let (mut w, mut c, f) = camp(44, "goblin");
    let at = Tick(30 * DAY);
    let player = c.player(&mut w, synth::PLAYER);
    c.signal(
        &mut w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount: 0.01,
            actor: player,
            pos: synth::FOREST_POS,
            district: None,
        },
        at,
    );
    let feud = c.feud_mut(f, player).expect("a feud");
    feud.demand = Some(Demand {
        kind: SignalKind::Killing,
        good: rtsim_trade::GoodId(0),
        tribute: 1,
        paid: 0,
        made: at,
        deadline: Tick(at.0 + 3 * DAY),
        after: 0.0,
        envoy: None,
    });
    strained(&mut c, f, 3.0, 1.99, at);
    c.slay(&mut w, f, 1, at);
    assert!(broken(&c, f).is_none(), "a demand stands");
    assert!(c.stats.break_deferrals >= 1);
}

/// EQC208: a camp that recovers before the recheck is not broken, and no check stays booked.
#[test]
fn a_pool_that_recovers_clears_the_check() {
    let (mut w, mut c, f) = raiders(45);
    let at = Tick(30 * DAY);
    let key = c.factions[f as usize].key;
    c.order_party(&mut w, key, PartyKind::Raid, district(), None, at)
        .expect("a raid");
    let pool = c.pool_at(f, at);
    strained(&mut c, f, pool, 1.5, at);
    let home = c.pool_at(f, at).floor() as u32;
    c.slay(&mut w, f, home + 10, at);
    assert!(
        c.factions[f as usize]
            .strain
            .as_ref()
            .unwrap()
            .check_at
            .is_some()
    );
    // The pool is back: 20 warriors home.
    let x = &mut c.factions[f as usize];
    (x.pool, x.pool_at) = (20.0, Tick(at.0 + 100));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(at.0 + 2 * DAY));
    assert!(broken(&c, f).is_none());
    assert!(
        c.factions[f as usize]
            .strain
            .as_ref()
            .unwrap()
            .check_at
            .is_none()
    );
}

// ---------------------------------------------------------------- the inert camp (EQC209)

/// The camp breaks now: the state a break is looked at from, and the kill that tips it.
fn break_now(c: &mut Conflict, w: &mut World, f: u32, at: Tick) {
    let pool = if c.factions[f as usize].cap > 40 {
        3.0
    } else {
        2.0
    };
    strained(c, f, pool, 1.99, at);
    c.slay(w, f, 1, at);
    assert!(broken(c, f).is_some(), "the camp broke");
}

fn kill_signal(
    c: &mut Conflict,
    w: &mut World,
    p: rtsim_knowledge::ActorId,
    amount: f64,
    at: Tick,
) {
    c.signal(
        w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount,
            actor: p,
            pos: synth::FOREST_POS,
            district: None,
        },
        at,
    );
}

/// EQC209: a broken camp books no feud wake, sends no party (a feud's, an order's or the plunder
/// drive's), whatever the grievance; a camp that is not broken does, on the same grievance.
#[test]
fn a_broken_camp_sends_nothing() {
    for broke in [false, true] {
        let (mut w, mut c, f) = camp(46, "goblin");
        let player = c.player(&mut w, synth::PLAYER);
        if broke {
            break_now(&mut c, &mut w, f, Tick(10 * DAY));
        }
        // A grievance that climbs the ladder to a siege.
        kill_signal(&mut c, &mut w, player, 100.0, Tick(11 * DAY));
        c.run_until(&mut w, &mut NoInfrastructure, Tick(120 * DAY));
        let sent: u64 = c.stats.parties.iter().sum();
        if broke {
            assert_eq!(sent, 0, "a broken camp sent {:?}", c.stats.parties);
            assert_eq!(c.stats.escalations.iter().sum::<u64>(), 0);
            let key = c.factions[f as usize].key;
            assert!(
                c.order_party(
                    &mut w,
                    key,
                    PartyKind::Raid,
                    district(),
                    None,
                    Tick(121 * DAY)
                )
                .is_none(),
                "not even by order"
            );
            assert_eq!(c.pool_at(f, Tick(120 * DAY)), 0.0);
            let kinds = c.pending_by_kind();
            assert_eq!(kinds.get("plunder"), None);
            assert_eq!(kinds.get("feud"), None);
        } else {
            assert!(sent > 0, "the control sends parties on this grievance");
        }
        c.check(&w).expect("books");
    }
}

/// Two camps in reach: goblins that plunder bandits only. The bandits' own drive is off.
fn rivals(seed: u64) -> (World, Conflict, u32, u32) {
    let mut g: CultureDef = def(GOBLIN_JSON);
    g.hates = vec!["bandit".into()];
    g.plunder_mean_days = Some(3.0);
    g.parties.retain(|k, _| k == "plunder");
    let mut b: CultureDef = def(BANDIT_JSON);
    b.plunder_mean_days = None;
    b.parties.retain(|k, _| k == "raid");
    let cultures = Cultures::from_json(&[
        &serde_json::to_string(&g).unwrap(),
        &serde_json::to_string(&b).unwrap(),
    ])
    .unwrap();
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(seed, ConflictTunables::default(), cultures, Tick::ZERO);
    synth::define_names(&mut c, &w);
    let goblin = c
        .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    let bandit = c
        .add_faction(&mut w, 0xC0B1_0002, "bandit", synth::FOREST_POS, Tick::ZERO)
        .unwrap();
    (w, c, goblin, bandit)
}

/// EQC209: a rival that is broken is nothing to plunder: no party picks its camp as a target.
#[test]
fn a_broken_camp_is_not_a_plunder_target() {
    for broke in [false, true] {
        let (mut w, mut c, goblin, bandit) = rivals(47);
        if broke {
            break_now(&mut c, &mut w, bandit, Tick(10));
        }
        let node = c.factions[bandit as usize].node;
        let mut at_it = 0;
        let mut t = Tick(10);
        while t.0 < 60 * DAY {
            t = Tick(t.0 + 3_000);
            c.run_until(&mut w, &mut NoInfrastructure, t);
            at_it += c
                .parties
                .iter()
                .filter(|(_, x)| x.faction == goblin && x.target == Target::Node(node))
                .count();
        }
        if broke {
            assert_eq!(at_it, 0, "nobody plunders a broken camp");
        } else {
            assert!(at_it > 0, "the control plunders the bandits");
        }
    }
}

/// EQC209: a broken camp still hears what is done in its territory: its grievances take their
/// impulses and decay as they do for any camp, so the camp that is refounded remembers.
#[test]
fn a_broken_camps_grievance_keeps_its_closed_form() {
    let mut totals = Vec::new();
    for broke in [false, true] {
        let (mut w, mut c, f) = camp(48, "goblin");
        let player = c.player(&mut w, synth::PLAYER);
        if broke {
            break_now(&mut c, &mut w, f, Tick(10 * DAY));
        }
        kill_signal(&mut c, &mut w, player, 3.0, Tick(11 * DAY));
        kill_signal(&mut c, &mut w, player, 2.0, Tick(13 * DAY));
        let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
        let g = c
            .feud(f, player)
            .expect("a feud")
            .g
            .total_at(Tick(20 * DAY), lambda);
        totals.push(g);
    }
    assert!(totals[0] > 0.0);
    assert!((totals[0] - totals[1]).abs() < 1e-9, "{totals:?}");
}

/// EQC209: Peaceful on and off plans every feud again and must not wake a broken camp.
#[test]
fn a_broken_camp_in_a_peaceful_world_wakes_correctly() {
    let (mut w, mut c, f) = camp(49, "goblin");
    let player = c.player(&mut w, synth::PLAYER);
    break_now(&mut c, &mut w, f, Tick(10 * DAY));
    kill_signal(&mut c, &mut w, player, 100.0, Tick(11 * DAY));
    c.set_difficulty(&mut w, boundary::DIFFICULTY_PEACEFUL);
    c.run_until(&mut w, &mut NoInfrastructure, Tick(20 * DAY));
    c.set_difficulty(&mut w, 2);
    c.run_until(&mut w, &mut NoInfrastructure, Tick(120 * DAY));
    assert_eq!(c.stats.parties.iter().sum::<u64>(), 0);
    assert_eq!(c.pending_by_kind().get("feud"), None);
    assert_eq!(c.pending_by_kind().get("plunder"), None);
    c.check(&w).expect("books");
}

/// EQC209 (property): whatever is killed and whenever, a broken camp has no party, no warriors
/// home or away (the booked wakes of before the break fall stale).
#[test]
fn no_broken_camp_ever_acts() {
    let mut breaks = 0;
    for seed in 0..12u64 {
        let (mut w, mut c, f) = camp(100 + seed, "goblin");
        let player = c.player(&mut w, synth::PLAYER);
        let mut rng = 0x9E37_79B9_7F4A_7C15u64 ^ seed.wrapping_mul(0xD1B5_4A32_D192_ED03);
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        strained(&mut c, f, 12.0, 1.9, Tick(5 * DAY));
        let mut t = Tick(5 * DAY);
        while t.0 < 300 * DAY {
            t = Tick(t.0 + 6_000);
            c.run_until(&mut w, &mut NoInfrastructure, t);
            match next() % 8 {
                0 => {
                    c.slay(&mut w, f, (next() % 5) as u32, t);
                }
                1 => kill_signal(&mut c, &mut w, player, 10.0, t),
                _ => {}
            }
            if broken(&c, f).is_some() {
                assert!(
                    c.parties.iter().all(|(_, x)| x.faction != f),
                    "seed {seed} {t:?}: a party"
                );
                assert_eq!(c.pool_at(f, t), 0.0, "seed {seed} {t:?}: a pool");
                assert_eq!(c.factions[f as usize].away, 0);
            }
            c.check(&w).expect("books");
        }
        breaks += c.stats.breaks;
    }
    assert!(
        breaks >= 3,
        "the property is vacuous: {breaks} breaks in 12 camps"
    );
}

// ---------------------------------------------------------------- the pages (EQC211)

/// EQC211: a pool camp's pressure, its break and its booked wake survive a save: the same state
/// hash, the strain and the wheel's strain wake as they were; a pressure that is not finite is
/// refused.
#[test]
fn pages_round_trip_rtce() {
    let (mut w, mut c, f) = camp(50, "goblin");
    let player = c.player(&mut w, synth::PLAYER);
    kill_signal(&mut c, &mut w, player, 4.0, Tick(2 * DAY));
    c.slay(&mut w, f, 20, Tick(3 * DAY));
    let before = c.factions[f as usize].strain.clone();
    assert!(before.as_ref().unwrap().pressure.parts().0 > 0.3);
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).expect("loads");
    assert_eq!(back.state_hash(), c.state_hash());
    assert_eq!(back.factions[f as usize].strain, before);
    // And broken, with the wake booked at its due tick.
    break_now(&mut c, &mut w, f, Tick(10 * DAY));
    let b = broken(&c, f).unwrap();
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).expect("loads");
    assert_eq!(back.state_hash(), c.state_hash());
    assert_eq!(broken(&back, f), Some(b));
    assert_eq!(back.pending_by_kind().get("strain"), Some(&1));
    assert_eq!(back.next_due(), c.next_due());
    assert!(!back.camp_active(f));
    // The older format is named, not read: the magic is RTCE.
    let mut old = c.save();
    old[..4].copy_from_slice(b"RTCD");
    let e = Conflict::load(c.tun.clone(), c.cultures.clone(), &old)
        .err()
        .expect("refused");
    let e = e.to_string();
    assert!(e.contains("RTCD") && e.contains("RTCE"), "{e}");
}

// ---------------------------------------------------------------- the refounding (EQC210)

/// The camp breaks at `at` and the world runs to the tick its wait is over (and no further).
fn broken_until_due(
    seed: u64,
    culture: &str,
) -> (World, Conflict, u32, rtsim_conflict::engine::Broken) {
    let (mut w, mut c, f) = camp(seed, culture);
    break_now(&mut c, &mut w, f, Tick(10 * DAY));
    let b = broken(&c, f).unwrap();
    (w, c, f, b)
}

/// EQC210: after `min(384, 48 x 2^P)` days the camp is refounded with a quarter of its cap (15 of
/// 60), nothing away, and it recruits again.
#[test]
fn a_camp_refounds_after_the_rule_with_a_quarter_pool() {
    for (culture, want) in [("goblin", 15.0), ("bandit", 10.0), ("raider", 12.0)] {
        let (mut w, mut c, f, b) = broken_until_due(51, culture);
        c.run_until(&mut w, &mut NoInfrastructure, Tick(b.due.0 - 1));
        assert!(
            broken(&c, f).is_some(),
            "{culture}: not before the wait is over"
        );
        c.run_until(&mut w, &mut NoInfrastructure, b.due);
        assert!(
            broken(&c, f).is_none(),
            "{culture}: refounded at {:?}",
            b.due
        );
        assert_eq!(c.stats.refounds, 1);
        assert_eq!(
            c.pool_at(f, b.due),
            want,
            "{culture}: a quarter of the cap, rounded down"
        );
        assert_eq!(c.factions[f as usize].away, 0);
        assert!(c.camp_active(f));
        assert!(c.recruit_rate(f) > 0.0);
        assert!(
            c.pool_at(f, Tick(b.due.0 + 10 * DAY)) > want,
            "recruiting again"
        );
        assert!(
            c.factions[f as usize]
                .strain
                .as_ref()
                .unwrap()
                .check_at
                .is_none()
        );
        c.check(&w).expect("books");
    }
}

/// EQC210: a new band has a new chief and heir, and none of the old camp's fights or its cowing.
#[test]
fn a_refounded_camp_has_a_new_chief_and_heir_and_no_old_fights() {
    let (mut w, mut c, f) = camp(52, "goblin");
    let old = {
        let x = &mut c.factions[f as usize];
        x.observed.push((10.0, 0.5));
        x.crystal = false;
        (x.chief, x.heir, x.chiefs)
    };
    break_now(&mut c, &mut w, f, Tick(10 * DAY));
    let b = broken(&c, f).unwrap();
    c.run_until(&mut w, &mut NoInfrastructure, b.due);
    let x = &c.factions[f as usize];
    assert_ne!(x.chief, old.0);
    assert_ne!(x.heir, old.1);
    assert_eq!(x.chiefs, old.2 + 2);
    assert!(w.knowledge.notable(x.chief).is_some() && w.knowledge.notable(x.heir).is_some());
    assert!(
        x.observed.is_empty(),
        "the old band's fights are not the new one's"
    );
    assert!(x.crystal);
    assert_eq!(x.cowed_until, Tick::ZERO);
    assert!(!x.chief_out);
}

/// EQC210: the pressure stays, decayed by the wait; and a camp refounded with a quarter of its cap
/// is over a tenth of it, so it does not break at once.
#[test]
fn a_refounded_camp_keeps_its_decayed_pressure_and_does_not_break_at_once() {
    let (mut w, mut c, f, b) = broken_until_due(53, "goblin");
    let at_break = pressure(&c, f, b.since);
    c.run_until(&mut w, &mut NoInfrastructure, b.due);
    let waited = (b.due.0 - b.since.0) as f64 / DAY as f64;
    let want = at_break * (-waited / 192.0).exp2();
    let got = pressure(&c, f, b.due);
    assert!((got - want).abs() < 0.01, "{got} vs {want}");
    assert!(got > 0.4, "it is pressured still: {got}");
    c.run_until(&mut w, &mut NoInfrastructure, Tick(b.due.0 + 20 * DAY));
    assert!(broken(&c, f).is_none());
    assert_eq!(c.stats.breaks, 1);
}

/// EQC210: the refounded camp acts again: its plunder drive is booked and its feuds are planned, so
/// the grievance it was holding sends the war bands it would have sent.
#[test]
fn a_refounded_camp_plans_its_plunder_and_its_feuds() {
    let (mut w, mut c, f) = camp(54, "goblin");
    let player = c.player(&mut w, synth::PLAYER);
    break_now(&mut c, &mut w, f, Tick(10 * DAY));
    let b = broken(&c, f).unwrap();
    kill_signal(&mut c, &mut w, player, 100.0, Tick(11 * DAY));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(b.due.0 - 1));
    assert_eq!(
        c.stats.parties.iter().sum::<u64>(),
        0,
        "nothing while broken"
    );
    // The grievance decays at its own half-life while the camp is down: bring a fresh one the day
    // before it is refounded, so the camp wakes to it.
    kill_signal(&mut c, &mut w, player, 100.0, Tick(b.due.0 - 1));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(b.due.0 + 40 * DAY));
    assert_eq!(c.stats.refounds, 1);
    assert!(
        c.stats.parties.iter().sum::<u64>() > 0,
        "the refounded camp acts: {:?}",
        c.stats
    );
    let live = c
        .pending_by_kind()
        .get("plunder")
        .copied()
        .unwrap_or_default()
        + c.stats.parties[PartyKind::Plunder.index()] as usize;
    assert!(live > 0, "its plunder drive is on");
    c.check(&w).expect("books");
}

/// EQC210: a camp can break again after a refounding, and its second wait is the rule's at the
/// pressure then: the pressure it kept (about 1 after a wait of 194 days) and the band's kills.
#[test]
fn a_camp_broken_twice_waits_by_the_pressure_it_has_then() {
    let (mut w, mut c, f, b) = broken_until_due(55, "bandit");
    c.run_until(&mut w, &mut NoInfrastructure, b.due);
    assert!(broken(&c, f).is_none());
    // The refounded band: 10 of 40, the pressure it kept about 1.0. Kill all 10: 1.25, no break.
    let at = Tick(b.due.0 + DAY);
    let have = c.pool_at(f, at).floor() as u32;
    assert_eq!(c.slay(&mut w, f, have, at), have);
    assert!(
        broken(&c, f).is_none(),
        "P is {} under 2",
        pressure(&c, f, at)
    );
    // Farmed by a player who keeps at it: another 40 recruits killed as they come over 140 days
    // takes the pressure over 2, and the thin camp breaks again, waiting by the rule.
    let mut t = at;
    while broken(&c, f).is_none() {
        t = Tick(t.0 + 5 * DAY);
        assert!(t.0 < at.0 + 800 * DAY, "it never broke again");
        c.run_until(&mut w, &mut NoInfrastructure, t);
        let home = c.pool_at(f, t).floor() as u32;
        c.slay(&mut w, f, home, t);
    }
    let again = broken(&c, f).unwrap();
    let p = pressure(&c, f, again.since);
    let want = (48.0 * p.exp2()).min(384.0);
    let got = (again.due.0 - again.since.0) as f64 / DAY as f64;
    assert!((got - want).abs() < 1e-3, "{got} vs {want} at P {p}");
    assert!(p >= 2.0);
    assert_eq!(c.stats.breaks, 2);
}

// ---------------------------------------------------------------- the site ledger (EQC212, EQC213)

/// The site ledger's row for the camp as the engine has it now, handed back `elapsed` ticks later
/// (`FoundRow::from_state`, as core's ledger and rtsim-host's fake one hand it back).
fn row_of(c: &Conflict, f: u32, elapsed: u64) -> FoundRow {
    let s = c.camp_site_state(f, c.now()).expect("a row");
    FoundRow::from_state(&s, elapsed, false)
}

/// EQC212: a pool camp's row carries the pressure it is under; a broken camp's is EMPTY, BROKEN
/// and WAITING with no members, and the wait left to its refounding.
#[test]
fn a_pool_camps_row_carries_pressure_and_the_refound_wait() {
    let (mut w, mut c, f) = camp(60, "goblin");
    let at = Tick(30 * DAY);
    c.slay(&mut w, f, 30, at);
    let s = c.camp_site_state(f, at).unwrap();
    assert!((f64::from(s.p) - 0.5).abs() < 1e-5, "{}", s.p);
    assert_eq!((s.k, s.count, s.away), (60, 30, 0));
    assert_eq!(s.flags & state_flags::SHORT, state_flags::SHORT);
    assert_eq!(s.flags & (state_flags::BROKEN | state_flags::WAITING), 0);
    // Broken.
    break_now(&mut c, &mut w, f, Tick(40 * DAY));
    let b = broken(&c, f).unwrap();
    let later = Tick(b.since.0 + 5 * DAY);
    let s = c.camp_site_state(f, later).unwrap();
    assert_eq!(
        s.flags,
        state_flags::SHORT | state_flags::EMPTY | state_flags::BROKEN | state_flags::WAITING
    );
    assert_eq!((s.count, s.away), (0, 0));
    assert_eq!(s.wait_left, b.due.0 - later.0);
    assert!(s.waiting());
    // The row is taken after a change: the break marks it.
    let rows = c.take_site_states();
    assert!(rows.iter().any(|r| r.key == c.factions[f as usize].key));
}

/// A faction found again with its row: the pressure aged by the ticks elapsed, the pool the row's
/// count, regrowing from the find.
#[test]
fn a_reset_resumes_a_pool_camps_pressure() {
    let mut r = CampRun::new(61, "goblin", 0);
    let f = r.f;
    let at = Tick(20 * DAY);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, at);
    assert_eq!(r.c.slay(&mut r.w, f, 40, at), 40);
    let want_p = r.c.pressure_now(f, Tick(at.0 + 30 * DAY));
    let row = row_of(&r.c, f, 30 * DAY);
    assert_eq!(row.count, 20);
    r.reborn(row, Tick(at.0 + 30 * DAY));
    let f = r.f;
    let now = Tick(at.0 + 30 * DAY);
    let got = r.c.pressure_now(f, now);
    assert!((got - want_p).abs() < 1e-4, "{got} vs {want_p}");
    assert_eq!(r.c.pool_at(f, now).floor(), 20.0);
    assert!(r.c.camp_active(f));
}

/// EQC212: a broken camp found again is broken for the wait that was left, and refounded at the
/// end of it with the pressure aged; a reset that took longer than the wait refounds it at once.
#[test]
fn a_reset_resumes_a_pool_camps_break() {
    for (elapsed_days, refounded) in [(60.0, false), (400.0, true)] {
        let mut r = CampRun::new(62, "goblin", 0);
        let f = r.f;
        let at = Tick(20 * DAY);
        r.c.run_until(&mut r.w, &mut NoInfrastructure, at);
        strained(&mut r.c, f, 3.0, 1.99, at);
        r.c.slay(&mut r.w, f, 1, at);
        let b = broken(&r.c, f).expect("broke");
        let wait = b.due.0 - b.since.0;
        let elapsed = (elapsed_days * DAY as f64) as u64;
        let row = row_of(&r.c, f, elapsed);
        assert!(row.waiting() && row.count == 0, "a broken camp's row");
        let now = Tick(at.0 + elapsed);
        r.reborn(row, now);
        let f = r.f;
        let again = broken(&r.c, f);
        if refounded {
            assert!(wait < elapsed, "the test's own premise");
            // Due one tick on.
            assert_eq!(again.unwrap().due, Tick(now.0 + 1));
            r.c.run_until(&mut r.w, &mut NoInfrastructure, Tick(now.0 + 2));
            assert!(broken(&r.c, f).is_none(), "refounded at once");
            assert_eq!(r.c.stats.refounds, 1);
            let want = 0.25 * 60.0;
            assert_eq!(r.c.pool_at(f, Tick(now.0 + 2)).floor(), want);
        } else {
            let again = again.expect("still broken");
            assert_eq!(again.due.0, now.0 + (wait - elapsed));
            assert_eq!(r.c.pool_at(f, Tick(now.0 + DAY)), 0.0);
            assert!(!r.c.camp_active(f));
            assert_eq!(
                r.c.stats.breaks, 0,
                "found broken is not a break of this life"
            );
            // Its plunder drive is off: no wake of the drive is live.
            r.c.check(&r.w).expect("books");
        }
        // Aged: the pressure read at the find is the row's, decayed by the elapsed days.
        let p = r.c.pressure_now(f, now);
        let was = pressure(&r.c, f, now);
        assert!((f64::from(p) - was).abs() < 1e-9);
        assert!(p > 0.0);
    }
}

/// EQC212: no row is a full pool and no pressure, as before S7; a row with warriors and a wait
/// (never written for a pool camp, but a plugin's) is not a broken camp.
#[test]
fn a_camp_found_with_no_row_is_full_and_unpressured() {
    let r = CampRun::new(63, "goblin", 0);
    let f = r.f;
    assert_eq!(r.c.pressure_now(f, r.c.now()), 0.0);
    assert_eq!(r.c.pool_at(f, r.c.now()), 60.0);
    assert!(r.c.camp_active(f));
}

#[test]
fn a_row_with_count_and_waiting_is_not_broken() {
    let mut r = CampRun::new(64, "goblin", 0);
    let f = r.f;
    let at = Tick(20 * DAY);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, at);
    let mut row = row_of(&r.c, f, 100);
    row.count = 30;
    row.flags |= rtsim_kernel::sites::tail_flags::ROW | rtsim_kernel::sites::tail_flags::WAITING;
    row.wait_left = 50 * DAY;
    r.reborn(row, Tick(at.0 + 100));
    assert!(r.c.camp_active(r.f), "warriors home: not broken");
    assert_eq!(r.c.pool_at(r.f, Tick(at.0 + 100)).floor(), 30.0);
}

// ---------------------------------------------------------------- the census (EQC213)

/// EQC213: the conflict census reads pressure, broken camps and renewals (arrivals and refounds).
#[test]
fn census_reports_pressure_broken_and_renewals() {
    let (mut w, mut c, f) = camp(65, "goblin");
    let at = Tick(30 * DAY);
    c.slay(&mut w, f, 30, at);
    let k = &c.census()[0];
    assert_eq!(
        (k.kind, k.sites, k.members, k.broken),
        ("conflict.camp", 1, 30, 0)
    );
    assert!((k.mean_p() - 0.5).abs() < 0.01, "{}", k.mean_p());
    assert_eq!(k.renewals, 0);
    break_now(&mut c, &mut w, f, Tick(40 * DAY));
    let k = &c.census()[0];
    assert_eq!((k.members, k.broken), (0, 1));
    let b = broken(&c, f).unwrap();
    c.run_until(&mut w, &mut NoInfrastructure, b.due);
    let k = &c.census()[0];
    assert_eq!(k.broken, 0);
    assert_eq!(k.renewals, 1, "a refound is a renewal");
    assert_eq!(k.members, 15);
}

// ---------------------------------------------------------------- a restart (EQC211, EQC226)

fn world_from(seed: u64, pages: &[u8]) -> World {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    World::load(synth::world_config(seed), goods, book, pages).unwrap()
}

/// One life of a goblin camp under a player who keeps killing: day by day a slay of what is home
/// (so the pressure climbs and the camp breaks and is refounded), with the engine saved and
/// loaded at each of `restarts` (days). The state hash at the end, and the breaks and refounds.
fn farmed_life(seed: u64, restarts: &[u64]) -> (u64, u64, u64) {
    let (mut w, mut c, f) = camp(seed, "goblin");
    let mut t = Tick::ZERO;
    for day in 1..=700u64 {
        t = Tick(day * DAY);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        if day % 7 == 0 {
            let home = c.pool_at(f, t).floor() as u32;
            c.slay(&mut w, f, home, t);
        }
        if restarts.contains(&day) {
            let (wb, cb) = (w.save(), c.save());
            w = world_from(seed, &wb);
            c = Conflict::load(c.tun.clone(), c.cultures.clone(), &cb).expect("loads");
            synth::define_names(&mut c, &w);
        }
    }
    c.run_until(&mut w, &mut NoInfrastructure, t);
    (c.state_hash(), c.stats.breaks, c.stats.refounds)
}

/// A restart changes no decision: the saved pressure, break, wait and wake give the life a camp
/// that never restarted had, through breaks and refoundings.
#[test]
fn a_restart_changes_no_decision() {
    let straight = farmed_life(71, &[]);
    assert!(
        straight.1 >= 1 && straight.2 >= 1,
        "the life breaks and refounds: {straight:?}"
    );
    for restarts in [
        &[100u64][..],
        &[50, 150, 250, 350, 450, 550, 650],
        &[299, 300, 301],
    ] {
        assert_eq!(farmed_life(71, restarts), straight, "restarts {restarts:?}");
    }
}

// ---------------------------------------------------------------- the outposts (EQC237 to EQC254, FB-5, FB-8)

/// The raider culture claims the vanilla pillager outpost and the Illager Invasion fort: a camp the
/// reporter declares under either id is a raider camp (a cap of 50, a full pool, a pool camp with
/// a strain), with a dictionary that knows the structure.
#[test]
fn the_raider_claims_the_outposts() {
    let c = Cultures::builtin();
    let raider = c.index_of("raider").unwrap();
    assert_eq!(c.of_structure("minecraft:pillager_outpost"), Some(raider));
    assert_eq!(c.of_structure("illagerinvasion:illager_fort"), Some(raider));
    // An outpost reported through FACTION_FOUND joins the world as a raider camp.
    let mut r = CampRun::new(80, "raider", 0);
    let f = r.f;
    assert_eq!(
        r.c.cultures.get(r.c.factions[f as usize].culture).name(),
        "raider"
    );
    assert_eq!(r.c.factions[f as usize].cap, 50);
    assert_eq!(r.c.pool_at(f, r.c.now()), 50.0);
    assert!(r.c.factions[f as usize].strain.is_some());
    assert_eq!(r.c.pressure_now(f, r.c.now()), 0.0);
    // No bodies at home: nothing for a census to count, so no camp ledger.
    assert!(r.c.camp(f).is_none());
    let _ = &mut r;
}

/// FB-8 (b): the raider plunders about once in 48 days, not 8 (the data's one number): over a
/// thousand days a camp's plunder drive fires some 20 times, not 125.
#[test]
fn the_raider_plunders_at_the_slower_mean() {
    let c = Cultures::builtin();
    let raider = c.get(c.index_of("raider").unwrap());
    assert_eq!(raider.def.plunder_mean_days, Some(48.0));
    let mut plunders = 0u32;
    for seed in 0..6u64 {
        let (mut w, mut c, f) = camp(200 + seed, "raider");
        c.run_until(&mut w, &mut NoInfrastructure, Tick(1000 * DAY));
        plunders += c.factions[f as usize].plunders;
    }
    // Each camp: a draw each time the drive fires (or fails to send); 6 camps x 1000 days / 48.
    let want = 6.0 * 1000.0 / 48.0;
    let got = f64::from(plunders);
    assert!(
        (0.6 * want..=1.5 * want).contains(&got),
        "{got} draws against {want}"
    );
}

// ---------------------------------------------------------------- the S7 review's findings (EQC291 on)

/// A fresh engine of the same seed finds the goblin camp again at `at` with `row`, while a
/// player's smog already reaches it: the plugin reports its sources before the camp's FACTION_FOUND,
/// so `add_faction` plans a feud for the newcomer before the row says it is broken (EQC292).
fn found_in_smog(seed: u64, row: FoundRow, at: Tick) -> (World, Conflict, u32) {
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(
        seed,
        ConflictTunables::default(),
        Cultures::builtin(),
        Tick::ZERO,
    );
    synth::define_names(&mut c, &w);
    let ci = c.cultures.index_of("goblin").unwrap();
    let structure = c.cultures.get(ci).def.structures[0].clone();
    c.define(4, synth::CAMP_STRUCTURE, &structure);
    // The player has worked at the refinery for two hours: its smog is his.
    let mut t = 20u64;
    while t <= 20 + 144_000 {
        c.observe_presence(&mut w, synth::PLAYER, synth::REFINERY_POS, Tick(t));
        t += 1_200;
    }
    c.set_emission(
        &mut w,
        1,
        SignalKind::Smog,
        synth::REFINERY_POS,
        Some(synth::REFINERY),
        300.0,
        Tick(144_100),
    );
    c.run_until(&mut w, &mut NoInfrastructure, Tick(at.0 - 1));
    c.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::FactionFound(boundary::FactionFound {
            faction: synth::CAMP,
            flags: 0,
            dimension: u32::from(synth::CAMP_POS.frame),
            structure: synth::CAMP_STRUCTURE,
            pos: (synth::CAMP_POS.x, 64, synth::CAMP_POS.z),
            home_bodies: 0,
            census: 0,
            row,
        }),
        at,
    );
    let f = c.faction_of_key(synth::CAMP).expect("found");
    (w, c, f)
}

/// EQC292: a camp found broken after a reset, with a player's pollution in its reach, books no
/// wake of a feud (the one `add_faction` planned as an active camp's is void), sends nothing and
/// no notice while it is broken, and its feud is planned again at the refounding.
#[test]
fn a_camp_found_broken_in_a_players_smog_books_no_feud_wake() {
    let (mut w1, mut c1, f1) = camp(70, "goblin");
    break_now(&mut c1, &mut w1, f1, Tick(10 * DAY));
    let elapsed = 5 * DAY;
    let row = row_of(&c1, f1, elapsed);
    assert!(row.waiting() && row.count == 0, "a broken camp's row");
    let wait = row.wait_now_ticks();
    let now = Tick(10 * DAY + elapsed);
    let (mut w, mut c, f) = found_in_smog(70, row, now);
    let player = c.player(&mut w, synth::PLAYER);
    assert!(
        c.feud(f, player).is_some(),
        "the player's smog reached the camp: a feud"
    );
    assert!(broken(&c, f).is_some(), "found broken");
    c.check(&w).expect("books: no live feud wake on a broken camp");
    c.take_commands();
    let due = broken(&c, f).unwrap().due;
    assert_eq!(due.0, now.0 + wait);
    c.run_until(&mut w, &mut NoInfrastructure, Tick(due.0 - 1));
    assert!(broken(&c, f).is_some());
    assert_eq!(c.stats.feud_wakes, 0, "no feud woke");
    assert_eq!(c.stats.parties.iter().sum::<u64>(), 0);
    assert!(
        !c.take_commands().iter().any(|x| matches!(x, Cmd::Notice(_))),
        "a notice was sent by a broken camp"
    );
    assert!(c.kept_notices().is_empty());
    c.check(&w).expect("books");
    // The refounding plans the feud again: the smog goes on, and the camp acts.
    c.run_until(&mut w, &mut NoInfrastructure, Tick(due.0 + 30 * DAY));
    assert!(broken(&c, f).is_none(), "refounded");
    assert!(c.stats.feud_wakes > 0, "the feud is planned again");
    c.check(&w).expect("books");
}

/// EQC292: the refounding ends a wait only the pool could end (`BLOCKED`), as the break does: the
/// new band has a pool, and a feud held at `BLOCKED` is planned again, not stuck until some other
/// change of the room unblocks it.
#[test]
fn the_refound_clears_a_wait_only_the_pool_could_end() {
    let (mut w, mut c, f, b) = broken_until_due(72, "goblin");
    let player = c.player(&mut w, synth::PLAYER);
    // A grievance a day before the refounding, which the broken camp hears; its feud holds at
    // `BLOCKED`, as a wake on the broken camp used to leave it.
    kill_signal(&mut c, &mut w, player, 100.0, Tick(b.due.0 - DAY));
    c.feud_mut(f, player).expect("a feud").wait_until = BLOCKED;
    c.run_until(&mut w, &mut NoInfrastructure, b.due);
    assert!(broken(&c, f).is_none(), "refounded");
    let feud = c.feud(f, player).expect("the grievance stands");
    assert!(
        feud.wait_until < BLOCKED,
        "the wait for the pool is over: {:?}",
        feud.wait_until
    );
}

/// EQC293: a camp that breaks keeps no notice (it sends nothing, and a notice kept would go again
/// at the next session start), and `resend_notices` sends none for a broken camp whatever a feud
/// holds; an active camp's notice goes again, as it did.
#[test]
fn a_broken_camp_resends_no_notice() {
    let (mut w, mut c, f) = camp(73, "goblin");
    let player = c.player(&mut w, synth::PLAYER);
    kill_signal(&mut c, &mut w, player, 2.0, Tick(10 * DAY));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(10 * DAY + DAY / 4));
    let notices = c.kept_notices();
    assert_eq!(notices.len(), 1, "a notice kept unplayed");
    let n = notices[0];
    c.take_commands();
    c.resend_notices();
    assert_eq!(
        c.take_commands()
            .iter()
            .filter(|x| matches!(x, Cmd::Notice(_)))
            .count(),
        1,
        "an active camp's notice goes again"
    );
    break_now(&mut c, &mut w, f, Tick(11 * DAY));
    assert!(
        c.kept_notices().is_empty(),
        "the break keeps no notice: {:?}",
        c.kept_notices()
    );
    // A notice a feud holds anyway (put back by hand) is not sent for a broken camp.
    c.feud_mut(f, player).expect("a feud").notice = Some(Box::new(n));
    c.resend_notices();
    assert!(
        !c.take_commands().iter().any(|x| matches!(x, Cmd::Notice(_))),
        "a broken camp sent a notice at the session start"
    );
}

/// EQC294: a plunder band already on its way to a rival camp that has broken since finds it
/// scattered and takes nothing (the pick already leaves a broken camp alone); a camp not broken is
/// plundered as before.
#[test]
fn a_plunder_band_arriving_at_a_broken_camp_takes_nothing() {
    for broke in [false, true] {
        let (mut w, mut c, goblin, bandit) = rivals(75);
        c.log_enabled = true;
        let node = c.factions[bandit as usize].node;
        // The haven stands over its targets, so there is something to carry off.
        let m = w.market(node);
        let rich: std::collections::BTreeMap<GoodId, i64> = w
            .goods
            .ids()
            .map(|g| (g, m.line(g).target + 40 * MILLI - m.stock(g)))
            .filter(|e| e.1 > 0)
            .collect();
        w.stock_from_players(node, &rich, Tick(10));
        let key = c.factions[goblin as usize].key;
        c.order_party(
            &mut w,
            key,
            PartyKind::Plunder,
            Target::Node(node),
            None,
            Tick(21),
        )
        .expect("a plunder band");
        // On its way (it leaves its camp after the muster), the rival breaks.
        c.run_until(&mut w, &mut NoInfrastructure, Tick(2_000));
        if broke {
            break_now(&mut c, &mut w, bandit, Tick(2_100));
        }
        c.run_until(&mut w, &mut NoInfrastructure, Tick(20 * DAY));
        if broke {
            assert_eq!(c.stats.raids_village, 0, "nothing was plundered");
            assert!(
                c.log
                    .iter()
                    .any(|e| e.what.contains("broken: nothing to take")),
                "the band's note"
            );
        } else {
            assert!(c.stats.raids_village > 0, "the control plunders the camp");
        }
        c.check(&w).expect("books");
    }
}

