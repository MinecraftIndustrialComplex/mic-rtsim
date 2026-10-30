// SPDX-License-Identifier: AGPL-3.0-or-later
//! S7, conflict pressure (EQUILIBRIUM.md §2.4.5, docs/overnight-choices-equilibrium.md EQC200 on):
//! a pool camp's recruiting slows by `2^-P` while a player's kills stand, a camp that is thin and
//! under heavy pressure breaks, and a broken camp is refounded after a wait. The body camps (the
//! Frostmaw's den, the Umvuthana's grove) are S9's and keep what they book today.

use rtsim_conflict::boundary::{self, Obs};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::culture::{
    BANDIT_JSON, Culture, CultureDef, FROSTMAW_JSON, GOBLIN_JSON, RAIDER_JSON, UMVUTHANA_JSON,
};
use rtsim_conflict::engine::{BLOCKED, Party, Target, body_id};
use rtsim_conflict::{Conflict, ConflictTunables, Cultures, synth};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome};
use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::threat::Threat;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::renewal::Pressure;
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
/// FakePlayer's kill) is that player's loss; the same death on the road is no one's.
#[test]
fn an_unclaimed_death_at_a_players_district_is_his() {
    for by in [boundary::BY_NONE, boundary::BY_OTHER] {
        let (mut w, mut c, f) = camp(23, "goblin");
        let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
        let o = casualty(&c, p, e, by, 2, next_op());
        ingest(&mut c, &mut w, &o, Tick(200));
        let got = pressure(&c, f, Tick(200));
        assert!((got - 2.0 / 60.0).abs() < 1e-6, "by {by}: {got}");
    }
    // The switch off: it is no one's.
    let (mut w, mut c, f) = camp(23, "goblin");
    c.tun.district_unclaimed_is_players = false;
    let (p, e) = embodied_raid(&mut c, &mut w, f, district(), Tick(21));
    let o = casualty(&c, p, e, boundary::BY_NONE, 2, next_op());
    ingest(&mut c, &mut w, &o, Tick(200));
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
