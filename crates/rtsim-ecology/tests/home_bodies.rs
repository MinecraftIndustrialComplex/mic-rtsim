// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragon home bodies (EQUILIBRIUM.md slice S5, docs/overnight-choices-equilibrium.md EQC100 to
//! EQC149): a dragon roosting at its lair is a vanilla body the plugin keeps (a home body), a
//! puppet when it leaves. The lair census reconciles the engine's occupants with the bodies the
//! plugin finds in the lair's footprint (EQC112), a body found gone is missing (EQC114), and a
//! home body past the leash is a puppet (EQC109).
//!
//! These tests drive the engine with the plugin's records directly; `synth::FakeIaf` runs the
//! same records in the long-horizon checks (`home_bodies_long.rs`).

use rtsim_ecology::boundary::{
    self, AttackerKind, Cmd, DISCARD_AWAY, DISCARD_LAIR_FULL, DISCARD_SECOND_BODY, Obs,
};
use rtsim_ecology::dragon::{State, agent_of};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{
    DragonId, DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth,
};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::sites::{FoundRow, SiteKind, tail_flags};
use rtsim_settle::Pos;
use rtsim_world::World;

const KEY: u64 = 0x5EED_0001;
const PLAYER: (u64, u64) = (5, 6);
const DAY: u64 = TICKS_PER_DAY;

struct Scene {
    w: World,
    d: Dragons,
    l: LairId,
}

fn scene(seed: u64) -> Scene {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        KEY,
        "Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 700, 1500),
        100,
        Tick(0),
    );
    Scene { w, d, l }
}

fn newdragon(name: &str, l: LairId, gender: Gender, age_days: u64) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender,
        variant: 1,
        age_ticks: age_days * DAY,
        lair: Some(l),
        fullness: 90,
        health: None,
        seed: None,
        at: None,
    }
}

/// A body standing at the lair: a census entry.
fn body(u: u64, agent: u64, x: f64) -> boundary::CensusBody {
    boundary::CensusBody {
        entity: (u, 7),
        agent,
        species: Species::Fire,
        flags: boundary::BODY_SLEEPING | boundary::BODY_HAS_HOME,
        age_ticks: (110 * DAY) as i32,
        hunger: 80,
        health: 400.0,
        gender: Gender::Female,
        variant: 2,
        pos: (x, 100.0, 1500.5),
    }
}

fn uuid(u: u64) -> (u64, u64) {
    (u, 7)
}

impl Scene {
    fn dragon(&mut self, name: &str, gender: Gender) -> DragonId {
        self.d
            .add_dragon(&mut self.w, newdragon(name, self.l, gender, 110), Tick(0))
    }

    fn ingest(&mut self, o: Obs, t: u64) {
        self.d
            .ingest(&mut self.w, &mut NoInfrastructure, &o, Tick(t));
    }

    fn run(&mut self, t: u64) {
        self.d
            .run_until(&mut self.w, &mut NoInfrastructure, Tick(t));
    }

    fn census(&mut self, epoch: u64, watched: bool, bodies: &[boundary::CensusBody], t: u64) {
        self.ingest(
            Obs::LairCensus(boundary::LairCensus {
                lair: KEY,
                epoch,
                dimension: 0,
                flags: if watched { boundary::CENSUS_WATCHED } else { 0 },
                bodies: bodies.to_vec(),
            }),
            t,
        );
    }

    fn cmds(&mut self) -> Vec<Cmd> {
        self.d.take_commands()
    }

    /// The plugin answers an ADOPT or MATERIALIZE with its body.
    fn answer(&mut self, agent: u64, epoch: u32, entity: (u64, u64), t: u64) {
        self.ingest(
            Obs::Materialized(boundary::Materialized {
                agent,
                epoch,
                dimension: 0,
                entity,
                home: true,
            }),
            t,
        );
    }

    /// `x` has a home body standing and loaded: a census adopted it and the plugin answered.
    /// Returns (agent, epoch).
    fn homed(&mut self, x: DragonId, u: u64, t: u64) -> (u64, u32) {
        self.census(1, true, &[body(u, agent_of(x), 700.5)], t);
        let (agent, epoch) = self
            .cmds()
            .into_iter()
            .find_map(|c| match c {
                Cmd::Adopt(a) => Some((a.agent, a.epoch)),
                _ => None,
            })
            .expect("the census adopts its body");
        self.answer(agent, epoch, uuid(u), t);
        (agent, epoch)
    }

    fn lost(&self) -> u64 {
        self.d.stats.player_losses
    }

    fn pressure(&self, day: u32) -> f32 {
        self.d.lairs[self.l as usize]
            .pressure
            .now(day, self.d.tun.respawn_half_life_days as f32)
    }
}

fn adopts(cmds: &[Cmd]) -> Vec<boundary::Adopt> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Adopt(a) => Some(*a),
            _ => None,
        })
        .collect()
}

fn discards(cmds: &[Cmd]) -> Vec<boundary::Discard> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Discard(a) => Some(*a),
            _ => None,
        })
        .collect()
}

fn materializes(cmds: &[Cmd]) -> Vec<boundary::Materialize> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Materialize(a) => Some(*a),
            _ => None,
        })
        .collect()
}

// ---- the census and its adoption (EQC112, EQC113) ---------------------------------------------

/// The roost's own dragon, an untagged Ice and Fire dragon in the lair's footprint, is found by the
/// census: it becomes the lair's occupant and its body is adopted as a home body, synced from the
/// body's own age, hunger and health (EQC113). Until the plugin answers the engine believes no body.
#[test]
fn a_lairs_own_dragon_found_by_the_census_becomes_its_occupant_with_a_home_body() {
    let mut s = scene(1);
    assert!(s.d.lairs[s.l as usize].occupants.is_empty());
    s.census(1, false, &[body(0xA1, 0, 700.5)], 100);
    let cmds = s.cmds();
    let adopted = adopts(&cmds);
    assert_eq!(adopted.len(), 1, "one adoption: {cmds:?}");
    assert_eq!(adopted[0].entity, uuid(0xA1));
    assert_eq!(
        adopted[0].flags,
        boundary::ADOPT_HAS_HOME | boundary::ADOPT_HOME_BODY
    );
    assert_eq!(adopted[0].age_ticks, (110 * DAY) as i32);
    assert_eq!(adopted[0].hunger, 80);
    assert!(adopted[0].health > 0.0);
    assert_eq!(adopted[0].home, (700, 100, 1500));
    let occupants = s.d.lairs[s.l as usize].occupants.clone();
    assert_eq!(occupants.len(), 1, "the dragon is the lair's occupant");
    let x = occupants[0];
    assert_eq!(agent_of(x), adopted[0].agent);
    assert!(matches!(
        s.d.dragon(x).unwrap().state,
        State::Embodied { .. }
    ));
    assert_eq!(
        s.d.dragon(x).unwrap().body,
        None,
        "booked only on the answer"
    );
    assert!(
        s.d.lairs[s.l as usize].vacant_since.is_none(),
        "the lair is not waiting for a newcomer"
    );
    s.answer(adopted[0].agent, adopted[0].epoch, uuid(0xA1), 101);
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(0xA1)));
    assert_eq!(s.d.stats.strays_adopted, 1);
    s.d.check(&s.w, None).unwrap();
    // The lair's site row names the body (K 2).
    let row = s.d.lair_site_state(s.l, Tick(101)).unwrap();
    assert_eq!(
        (row.kind, row.k, row.count, row.away),
        (SiteKind::Lair, 2, 1, 0)
    );
    assert_eq!(row.bodies.home, vec![uuid(0xA1)]);
}

/// A dragon the engine owns, owed a body, whose body stands carrying its agent, adopts it: the
/// body gets the engine's closed forms (EQC108), not the other way round.
#[test]
fn a_home_body_found_is_adopted_and_synced_from_the_engine() {
    let mut s = scene(2);
    let x = s.dragon("Scorch", Gender::Female);
    let before = s.d.dragon(x).unwrap().clone();
    let at = 2 * DAY;
    let mut b = body(0xB1, agent_of(x), 700.5);
    b.hunger = 3; // the body's own numbers are not the dragon's
    b.age_ticks = 5;
    b.health = 1.0;
    s.census(1, true, &[b], at);
    let a = adopts(&s.cmds());
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].agent, agent_of(x));
    let want_hunger = before.fullness_at(&s.d.tun, Tick(at));
    assert_eq!(
        a[0].hunger, want_hunger,
        "hunger from the engine's closed form"
    );
    assert_eq!(
        a[0].age_ticks as u64,
        before.age(Tick(at)),
        "age from the engine"
    );
    assert!(a[0].health > 100.0, "health from the engine");
    s.answer(a[0].agent, a[0].epoch, uuid(0xB1), at + 1);
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(0xB1)));
    assert_eq!(s.d.stats.strays_adopted, 0, "its own dragon is no stray");
    assert_eq!(s.d.stats.home_adopts, 1);
}

/// Two bodies of one dragon: the one farther from the lair is removed (reason 4), the other
/// adopted, and the removal comes first (H1).
#[test]
fn a_second_body_of_one_dragon_is_discarded_the_farther() {
    let mut s = scene(3);
    let x = s.dragon("Twin", Gender::Female);
    s.census(
        1,
        true,
        &[
            body(0xC1, agent_of(x), 730.5),
            body(0xC2, agent_of(x), 703.5),
        ],
        100,
    );
    let cmds = s.cmds();
    let kinds: Vec<u16> = cmds.iter().map(Cmd::kind).collect();
    assert_eq!(
        kinds,
        vec![boundary::DRAGON_DISCARD, boundary::DRAGON_ADOPT],
        "removals first: {cmds:?}"
    );
    assert_eq!(discards(&cmds)[0].entity, uuid(0xC1));
    assert_eq!(discards(&cmds)[0].reason, DISCARD_SECOND_BODY);
    assert_eq!(adopts(&cmds)[0].entity, uuid(0xC2));
}

/// A stray above the pair is removed (reason 2), as the old untagged path did.
#[test]
fn a_stray_above_two_is_discarded() {
    let mut s = scene(4);
    let a = s.dragon("A", Gender::Female);
    let b = s.dragon("B", Gender::Male);
    s.census(
        1,
        true,
        &[
            body(0xD1, agent_of(a), 700.5),
            body(0xD2, agent_of(b), 701.5),
            body(0xD3, 0, 702.5),
        ],
        100,
    );
    let cmds = s.cmds();
    assert_eq!(adopts(&cmds).len(), 2);
    let gone = discards(&cmds);
    assert_eq!(gone.len(), 1, "{cmds:?}");
    assert_eq!(
        (gone[0].entity, gone[0].reason),
        (uuid(0xD3), DISCARD_LAIR_FULL)
    );
    assert_eq!(
        s.d.lairs[s.l as usize].occupants.len(),
        2,
        "no third occupant"
    );
}

/// A lair found late with no ledger row waits the cap with its pressure at the cap; its own dragon
/// turning up shows the history was lost, not the dragon (MZB-1012): the pressure resets.
#[test]
fn a_late_lair_with_no_row_resets_its_pressure_when_its_own_dragon_is_found() {
    let (mut w, _) = synth::world(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    let l = d.found_lair(
        &mut w,
        KEY,
        "Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 700, 1500),
        100,
        true,
        &FoundRow::none(),
        Tick(0),
    );
    let h = d.tun.respawn_half_life_days as f32;
    assert!(
        d.lairs[l as usize].pressure.now(0, h) > 3.0,
        "farmed to the cap"
    );
    assert!(d.lairs[l as usize].found_unknown);
    let mut s = Scene { w, d, l };
    s.census(1, false, &[body(0xE1, 0, 700.5)], 100);
    assert_eq!(adopts(&s.cmds()).len(), 1);
    assert_eq!(s.pressure(0), 0.0, "its own dragon: the history was lost");
    assert!(!s.d.lairs[s.l as usize].found_unknown);
}

// ---- away dragons and the recall (EQC115) -----------------------------------------------------

/// Sends dragon `x` hunting from its lair with a body believed standing at the lair.
fn away_with_body(s: &mut Scene, x: DragonId, u: u64) {
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(u));
    // It books its Hungry event when fullness falls to 60: 90,000 ticks after it was made.
    s.run(90_100);
    let st = s.d.dragon(x).unwrap().state;
    assert!(
        matches!(st, State::Flying(_) | State::Hunting { .. }),
        "it is away (a hungry dragon in the scenario world finds prey): {st:?}"
    );
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(u)), "its body stays");
}

/// The lair comes in with nobody near while its dragon is away and its old body stands: the body is
/// removed (reason 3) and forgotten; nothing was lost (the dragon is alive).
#[test]
fn an_away_dragons_standing_body_is_discarded_unwatched() {
    let mut s = scene(6);
    let x = s.dragon("Rover", Gender::Female);
    away_with_body(&mut s, x, 0xF1);
    let t = s.d.now().0 + 10;
    s.census(1, false, &[body(0xF1, agent_of(x), 700.5)], t);
    let cmds = s.cmds();
    let gone = discards(&cmds);
    assert_eq!(gone.len(), 1, "{cmds:?}");
    assert_eq!((gone[0].entity, gone[0].reason), (uuid(0xF1), DISCARD_AWAY));
    assert!(adopts(&cmds).is_empty());
    assert_eq!(s.d.dragon(x).unwrap().body, None);
    assert!(matches!(
        s.d.dragon(x).unwrap().state,
        State::Flying(_) | State::Hunting { .. }
    ));
    assert_eq!(s.lost(), 0);
    assert_eq!(s.d.stats.home_removed, 1);
}

/// A player is watching: no body vanishes in front of them. The dragon is recalled from its trip
/// and its body adopted.
#[test]
fn a_watched_lair_recalls_its_away_dragon() {
    let mut s = scene(7);
    let x = s.dragon("Rover", Gender::Female);
    away_with_body(&mut s, x, 0xF2);
    let t = s.d.now().0 + 10;
    s.census(1, true, &[body(0xF2, agent_of(x), 700.5)], t);
    let cmds = s.cmds();
    assert!(
        discards(&cmds).is_empty(),
        "nothing is removed in view: {cmds:?}"
    );
    let a = adopts(&cmds);
    assert_eq!(a.len(), 1, "{cmds:?}");
    let dx = s.d.dragon(x).unwrap();
    assert!(dx.leg.is_none(), "its trip is cancelled");
    assert!(matches!(dx.state, State::Embodied { .. }));
    assert_eq!(s.d.stats.home_recalled, 1);
}

/// A recall books no hunt: the trip's events are stale, nothing is eaten or raided.
#[test]
fn a_recall_cancels_the_trip_and_books_no_hunt() {
    let mut s = scene(8);
    let x = s.dragon("Rover", Gender::Female);
    away_with_body(&mut s, x, 0xF3);
    let hunts = s.d.stats.hunts;
    let t = s.d.now().0 + 10;
    s.census(1, true, &[body(0xF3, agent_of(x), 700.5)], t);
    s.cmds();
    s.run(t + 20 * DAY);
    assert_eq!(
        s.d.stats.hunts, hunts,
        "the abandoned trip's hunt never happens"
    );
    assert_eq!(s.lost(), 0);
}

// ---- owed bodies (EQC116) ---------------------------------------------------------------------

/// A dragon roosting at a lair that is in with nobody near, owed a body, gets one (a lair under a
/// chunk loader never sees a player's area): spawned at the lair, sleeping, as a home body.
#[test]
fn an_owed_dragon_gets_its_body_at_an_unwatched_census() {
    let mut s = scene(9);
    let x = s.dragon("Owed", Gender::Female);
    s.census(1, false, &[], 100);
    let m = materializes(&s.cmds());
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].agent, agent_of(x));
    assert_ne!(m[0].flags & boundary::MAT_HOME_BODY, 0);
    assert_ne!(m[0].flags & boundary::MAT_SLEEPING, 0);
    assert_eq!(m[0].pos, (700.0, 100.0, 1500.0), "at the lair");
    assert!(matches!(
        s.d.dragon(x).unwrap().state,
        State::Embodied { .. }
    ));
    assert_eq!(s.d.stats.home_spawns, 1);
    s.answer(m[0].agent, m[0].epoch, uuid(0x91), 101);
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(0x91)));
}

/// Watched, the census spawns nothing: a player's area materializes the dragon when it covers the
/// roost (`SeenAtLair`).
#[test]
fn a_watched_census_spawns_nothing() {
    let mut s = scene(10);
    s.dragon("Owed", Gender::Female);
    s.census(1, true, &[], 100);
    assert!(s.cmds().is_empty());
}

/// A player's area covering the roost materializes the roosting dragon as a home body.
#[test]
fn seen_at_lair_materializes_an_owed_dragon_as_a_home_body() {
    let mut s = scene(11);
    let x = s.dragon("Owed", Gender::Female);
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 700, 1500),
        Tick(100),
        Tick(150_000),
    );
    s.run(120);
    let m = materializes(&s.cmds());
    assert_eq!(m.len(), 1, "one body at the roost");
    assert_eq!(m[0].agent, agent_of(x));
    assert_ne!(
        m[0].flags & boundary::MAT_HOME_BODY,
        0,
        "a home body, not a puppet"
    );
}

/// A dragon whose home body stands is not materialized again: its body is its embodiment and the
/// census finds it.
#[test]
fn seen_at_lair_ignores_a_dragon_with_a_standing_body() {
    let mut s = scene(12);
    let x = s.dragon("Housed", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x92));
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 700, 1500),
        Tick(100),
        Tick(150_000),
    );
    s.run(120);
    assert!(materializes(&s.cmds()).is_empty());
    assert!(matches!(s.d.dragon(x).unwrap().state, State::Roosting));
}

// ---- missing bodies (EQC114) ------------------------------------------------------------------

/// A roosting dragon's body that no census finds is missing: at the third miss, two game days
/// after the first, it is written off, a player's loss when rtsim ran throughout.
#[test]
fn a_missing_body_is_written_off_at_the_third_miss_over_two_days() {
    let mut s = scene(13);
    let x = s.dragon("Vanished", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x81));
    s.census(1, false, &[], 100);
    s.census(1, false, &[], 100 + DAY);
    assert!(s.d.dragon(x).is_some(), "two misses: still believed");
    s.census(1, false, &[], 100 + DAY + 100);
    assert!(
        s.d.dragon(x).is_some(),
        "a third miss, but not two days after the first"
    );
    s.census(1, false, &[], 100 + 2 * DAY);
    assert!(s.d.dragon(x).is_none(), "written off");
    assert_eq!(s.lost(), 1, "a player's loss");
    assert!(s.pressure(2) > 0.9, "the lair's pressure rose");
    assert_eq!(s.d.lairs[s.l as usize].missing, vec![uuid(0x81)]);
    assert!(s.d.lairs[s.l as usize].occupants.is_empty());
    assert!(
        s.d.lairs[s.l as usize].vacant_since.is_some(),
        "its replacement waits"
    );
    s.d.check(&s.w, None).unwrap();
}

/// Rtsim did not run throughout (the ledger row's GAP): the same write-off is Other, no pressure,
/// the wait is the cap's.
#[test]
fn a_missing_body_after_a_gap_is_written_off_as_other_and_waits_the_cap() {
    let mut s = scene(14);
    let x = s.dragon("Vanished", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x82));
    s.d.lairs[s.l as usize].gap = true;
    for k in 0..3 {
        s.census(1, false, &[], 100 + k * DAY);
    }
    assert!(s.d.dragon(x).is_none(), "written off");
    assert_eq!(s.lost(), 0, "Other: no player's loss");
    assert_eq!(s.pressure(3), 0.0);
    assert_eq!(s.d.lairs[s.l as usize].missing, vec![uuid(0x82)]);
    let cap = s.d.tun.renewal().cap_ticks();
    let lair = &s.d.lairs[s.l as usize];
    assert!(lair.vacant_since.is_some());
    assert_eq!(lair.due.0, 100 + 2 * DAY + cap, "the wait is the cap's");
}

/// A body written off turns up again while its lair is short: home again, the replacement wait
/// cancelled.
#[test]
fn a_missing_body_found_again_while_short_is_home_again() {
    let mut s = scene(15);
    let x = s.dragon("Back", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x83));
    for k in 0..3 {
        s.census(1, false, &[], 100 + k * DAY);
    }
    s.census(1, false, &[], 100 + 3 * DAY);
    assert!(s.d.dragon(x).is_none());
    assert!(s.d.lairs[s.l as usize].vacant_since.is_some());
    s.cmds();
    s.census(1, false, &[body(0x83, 0, 700.5)], 100 + 4 * DAY);
    let a = adopts(&s.cmds());
    assert_eq!(a.len(), 1, "home again");
    assert_eq!(s.d.lairs[s.l as usize].occupants.len(), 1);
    assert!(
        s.d.lairs[s.l as usize].vacant_since.is_none(),
        "the wait is cancelled"
    );
    assert!(s.d.lairs[s.l as usize].missing.is_empty());
}

/// After the lair has refilled, a missing body that turns up is a stray left standing, and its death
/// presses the lair (S3's F3 and F4).
#[test]
fn after_a_refill_a_missing_body_found_is_a_stray_and_its_death_counts() {
    let mut s = scene(16);
    let x = s.dragon("Gone", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x84));
    for k in 0..4 {
        s.census(1, false, &[], 100 + k * DAY);
    }
    assert!(s.d.dragon(x).is_none());
    // The lair refills with two newcomers (a pair).
    s.dragon("New1", Gender::Female);
    s.dragon("New2", Gender::Male);
    s.cmds();
    let lost = s.lost();
    s.census(1, false, &[body(0x84, 0, 700.5)], 100 + 10 * DAY);
    let cmds = s.cmds();
    // A pair already: the stray is neither adopted nor removed by a census (it is left standing).
    assert!(
        adopts(&cmds).iter().all(|a| a.entity != uuid(0x84)),
        "{cmds:?}"
    );
    assert!(
        discards(&cmds).iter().all(|d| d.entity != uuid(0x84)),
        "left standing: {cmds:?}"
    );
    let p0 = s.pressure(10);
    // It dies, whatever the cause: epoch 0, its UUID and its lair.
    s.ingest(
        Obs::Died(boundary::Died {
            agent: agent_of(x),
            epoch: 0,
            killer_kind: AttackerKind::Player,
            killer: PLAYER,
            dimension: 0,
            pos: (700.0, 100.0, 1500.0),
            entity: uuid(0x84),
            lair: KEY,
        }),
        100 + 10 * DAY + 50,
    );
    assert!(
        s.pressure(10) > p0 + 0.9,
        "the stray's death counts: {p0} then {}",
        s.pressure(10)
    );
    assert_eq!(s.d.stats.stray_deaths, 1);
    assert!(s.d.lairs[s.l as usize].missing.is_empty());
    assert_eq!(
        s.lost(),
        lost,
        "the stray's death is not a second dragon's loss"
    );
}

/// Found each time, the misses start over.
#[test]
fn a_census_that_lists_the_body_resets_the_misses() {
    let mut s = scene(17);
    let x = s.dragon("Steady", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x85));
    s.census(1, false, &[], 100);
    s.census(1, false, &[], 100 + DAY);
    assert_eq!(s.d.dragon(x).unwrap().miss.0, 2);
    s.census(1, false, &[body(0x85, agent_of(x), 700.5)], 100 + 2 * DAY);
    s.cmds();
    assert_eq!(s.d.dragon(x).unwrap().miss.0, 0);
}

// ---- leaving, losses, left-behind bodies (EQC117, EQC118) -------------------------------------

/// A dragon that leaves its lair leaves its body: the lair remembers it to remove on sight.
#[test]
fn a_dragon_that_leaves_its_lair_leaves_its_body_to_remove() {
    let mut s = scene(18);
    let other = s.d.add_lair(
        &mut s.w,
        KEY + 1,
        "Other",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 6000, 6000),
        100,
        Tick(0),
    );
    let x = s.dragon("Mover", Gender::Female);
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x71));
    s.d.relocate(&mut s.w, x, other, Tick(100));
    assert_eq!(s.d.dragon(x).unwrap().body, None);
    assert_eq!(s.d.lairs[s.l as usize].left_behind, vec![uuid(0x71)]);
    s.census(1, false, &[body(0x71, agent_of(x), 700.5)], 200);
    let cmds = s.cmds();
    let gone = discards(&cmds);
    assert_eq!(gone.len(), 1, "{cmds:?}");
    assert_eq!((gone[0].entity, gone[0].reason), (uuid(0x71), DISCARD_AWAY));
    // It leaves the list at the next census that does not list it (EQC124): the removal may not
    // have found its chunk.
    assert_eq!(s.d.lairs[s.l as usize].left_behind, vec![uuid(0x71)]);
    s.census(1, false, &[], 300);
    assert!(s.d.lairs[s.l as usize].left_behind.is_empty());
}

/// A dragon that dies of age away from its lair leaves its body too.
#[test]
fn a_dragon_dead_of_age_away_leaves_its_body_to_remove() {
    let mut s = scene(19);
    let life = s.d.tun.species(Species::Fire).lifespan_ticks();
    let x = s.d.add_dragon(
        &mut s.w,
        NewDragon {
            age_ticks: life,
            ..newdragon("Old", s.l, Gender::Female, 0)
        },
        Tick(0),
    );
    s.d.dragons.get_mut(x).unwrap().body = Some(uuid(0x72));
    s.run(3 * DAY);
    assert!(s.d.dragon(x).is_none(), "dead of age");
    assert_eq!(s.d.lairs[s.l as usize].left_behind, vec![uuid(0x72)]);
}

/// A player kills a loaded home body: the lair's pressure rises by the shared rule; the corpse is
/// the loot and is not left behind to be removed.
#[test]
fn a_killed_home_body_is_a_players_loss_and_its_corpse_is_not_left_behind() {
    let mut s = scene(20);
    let x = s.dragon("Victim", Gender::Female);
    let (agent, epoch) = s.homed(x, 0x61, 100);
    s.ingest(
        Obs::Died(boundary::Died {
            agent,
            epoch,
            killer_kind: AttackerKind::Player,
            killer: PLAYER,
            dimension: 0,
            pos: (700.0, 100.0, 1500.0),
            entity: uuid(0x61),
            lair: KEY,
        }),
        200,
    );
    assert!(s.d.dragon(x).is_none());
    assert_eq!(s.lost(), 1);
    assert!(s.pressure(0) > 0.9);
    assert!(
        s.d.lairs[s.l as usize].left_behind.is_empty(),
        "the corpse is the loot"
    );
    assert!(s.d.lairs[s.l as usize].missing.is_empty());
    s.d.check(&s.w, None).unwrap();
}

/// A home body that is hungry is the I&F AI's to feed: the engine never flies it off. A puppet in
/// the same state is sent away.
#[test]
fn a_home_bodys_hunger_never_sends_a_depart() {
    let mut s = scene(21);
    let x = s.dragon("Hungry", Gender::Female);
    let (agent, epoch) = s.homed(x, 0x62, 100);
    let starving = |agent, epoch| {
        Obs::State(boundary::State {
            agent,
            epoch,
            flags: 0,
            dimension: 0,
            health: 400.0,
            max_health: 500.0,
            hunger: 5,
            age_ticks: 1,
            pos: (700.5, 100.0, 1500.5),
        })
    };
    s.ingest(starving(agent, epoch), 200);
    assert!(
        s.cmds().iter().all(|c| !matches!(c, Cmd::Depart(_))),
        "a home body is left to its AI"
    );
}

/// A dragon removed while its home body is loaded is discarded with it (DEPART reason 5 on a home
/// body discards it).
#[test]
fn remove_discards_a_home_body() {
    let mut s = scene(22);
    let life = s.d.tun.species(Species::Fire).lifespan_ticks();
    let x = s.d.add_dragon(
        &mut s.w,
        NewDragon {
            age_ticks: life,
            ..newdragon("Old", s.l, Gender::Female, 0)
        },
        Tick(0),
    );
    let (agent, epoch) = s.homed(x, 0x63, 100);
    s.run(3 * DAY);
    let cmds = s.cmds();
    let dep = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Depart(dd) if dd.agent == agent && dd.epoch == epoch => Some(*dd),
            _ => None,
        })
        .expect("it is sent away");
    assert_eq!(dep.reason, 5);
    assert!(
        s.d.lairs[s.l as usize].left_behind.is_empty(),
        "discarded, not left behind"
    );
}

// ---- the swaps (EQC109, EQC110, EQC117) -------------------------------------------------------

/// A home body past the leash is a puppet: the engine forgets its body and keeps the dragon.
#[test]
fn a_home_body_left_home_is_a_puppet_and_the_engine_forgets_its_body() {
    let mut s = scene(23);
    let x = s.dragon("Lured", Gender::Female);
    let (agent, epoch) = s.homed(x, 0x51, 100);
    s.ingest(
        Obs::DragonLeftHome(boundary::DragonLeftHome {
            agent,
            epoch,
            dimension: 0,
            entity: uuid(0x51),
            pos: (760.0, 100.0, 1500.0),
        }),
        200,
    );
    let dx = s.d.dragon(x).unwrap();
    assert_eq!(dx.body, None);
    assert!(matches!(dx.state, State::Embodied { epoch: e, .. } if e == epoch));
    assert_eq!(dx.lair, Some(s.l), "still the lair's occupant");
    assert_eq!(s.lost(), 0, "luring costs the lair nothing (FB-1 a)");
    assert_eq!(s.pressure(0), 0.0);
    assert!(s.d.lairs[s.l as usize].missing.is_empty());
    assert_eq!(s.d.stats.home_left, 1);
    s.d.check(&s.w, None).unwrap();
}

/// A settled puppet at a lair that is in becomes a home body, ordered once.
#[test]
fn hand_back_is_ordered_only_for_a_settled_puppet_at_a_lair_that_is_in() {
    let mut s = scene(24);
    let x = s.dragon("Visitor", Gender::Female);
    // A puppet in view: the lair is out, so a settled state orders nothing.
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 700, 1500),
        Tick(100),
        Tick(150_000),
    );
    s.d.dragons.get_mut(x).unwrap().fullness = 100;
    // Make it a puppet: a hunting dragon seen by the player is materialized without the flag.
    let (agent, epoch) = {
        let m = {
            s.run(120);
            materializes(&s.cmds())
        };
        let m = m.first().expect("seen at its lair");
        (m.agent, m.epoch)
    };
    // The plugin made a puppet of it (it was a third occupant): no home body.
    s.ingest(
        Obs::Materialized(boundary::Materialized {
            agent,
            epoch,
            dimension: 0,
            entity: uuid(0x40),
            home: false,
        }),
        121,
    );
    assert_eq!(s.d.dragon(x).unwrap().body, None);
    let settled = |flags: u32, x: f64, y: f64| {
        Obs::State(boundary::State {
            agent,
            epoch,
            flags,
            dimension: 0,
            health: 400.0,
            max_health: 500.0,
            hunger: 80,
            age_ticks: 1,
            pos: (x, y, 1500.5),
        })
    };
    s.ingest(settled(boundary::STATE_SLEEPING, 701.5, 100.0), 200);
    assert!(
        s.cmds().iter().all(|c| !matches!(c, Cmd::HandBack(_))),
        "the lair is out"
    );
    s.census(1, true, &[], 210);
    s.cmds();
    s.ingest(settled(boundary::STATE_FLYING, 701.5, 100.0), 220);
    assert!(
        s.cmds().iter().all(|c| !matches!(c, Cmd::HandBack(_))),
        "flying"
    );
    s.ingest(settled(0, 800.5, 100.0), 230);
    assert!(
        s.cmds().iter().all(|c| !matches!(c, Cmd::HandBack(_))),
        "too far"
    );
    s.ingest(settled(0, 701.5, 150.0), 235);
    assert!(
        s.cmds().iter().all(|c| !matches!(c, Cmd::HandBack(_))),
        "too high"
    );
    s.ingest(settled(boundary::STATE_SLEEPING, 701.5, 101.0), 240);
    let hb: Vec<_> = s
        .cmds()
        .into_iter()
        .filter_map(|c| match c {
            Cmd::HandBack(h) => Some(h),
            _ => None,
        })
        .collect();
    assert_eq!(hb, vec![boundary::HandBack { agent, epoch }]);
    // One in flight at a time.
    s.ingest(settled(boundary::STATE_SLEEPING, 701.5, 101.0), 260);
    assert!(s.cmds().iter().all(|c| !matches!(c, Cmd::HandBack(_))));
    s.answer(agent, epoch, uuid(0x41), 270);
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(0x41)));
    assert_eq!(s.d.stats.home_hand_backs, 1);
}

/// A home body whose lair went out (or whose chunk unloaded) stays in the world and vanilla saves
/// it: the engine keeps its body and the dragon roosts.
#[test]
fn reason_8_keeps_the_body_and_resumes_roosting() {
    let mut s = scene(25);
    let x = s.dragon("Saved", Gender::Female);
    let (agent, epoch) = s.homed(x, 0x31, 100);
    s.ingest(
        Obs::Dematerialized(boundary::Dematerialized {
            agent,
            epoch,
            reason: boundary::DEMAT_HOME_SAVED,
            dimension: 0,
            health: 380.0,
            max_health: 500.0,
            hunger: 70,
            age_ticks: 1,
            pos: (720.0, 100.0, 1500.0),
        }),
        200,
    );
    s.ingest(
        Obs::LairOut(boundary::LairOut {
            lair: KEY,
            epoch: 1,
        }),
        200,
    );
    let dx = s.d.dragon(x).unwrap();
    assert!(matches!(dx.state, State::Roosting), "{:?}", dx.state);
    assert_eq!(dx.body, Some(uuid(0x31)), "its body still stands");
    assert_eq!(
        dx.fullness_at(&s.d.tun, Tick(200)),
        70,
        "its hunger as captured"
    );
    assert_eq!(s.d.stats.home_saved, 1);
    s.d.check(&s.w, None).unwrap();
}

// ---- reset and rows (EQC119, EQC120) ----------------------------------------------------------

fn reset_lair(seed: u64, home: Vec<(u64, u64)>, gap: bool) -> Scene {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let row = FoundRow {
        flags: tail_flags::ROW | if gap { tail_flags::GAP } else { 0 },
        k: 2,
        count: home.len() as u16,
        p: 1.5,
        away: 0,
        wait_left: 0,
        elapsed: 0,
        bodies: rtsim_kernel::sites::Bodies {
            home,
            left_behind: vec![uuid(0x22)],
            missing: vec![uuid(0x23)],
        },
    };
    let l = d.found_lair(
        &mut w,
        KEY,
        "Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 700, 1500),
        100,
        false,
        &row,
        Tick(0),
    );
    Scene { w, d, l }
}

/// After a reset the ledger's row names the lair's home bodies; the census adopts each as an
/// occupant and no wait is left. The pressure is the row's.
#[test]
fn a_reset_lair_expects_its_rows_bodies_and_the_census_adopts_them() {
    let mut s = reset_lair(26, vec![uuid(0x21)], false);
    let lair = &s.d.lairs[s.l as usize];
    assert_eq!(lair.expect, vec![uuid(0x21)]);
    assert_eq!(lair.left_behind, vec![uuid(0x22)]);
    assert_eq!(lair.missing, vec![uuid(0x23)]);
    assert!(lair.occupants.is_empty());
    assert!(
        lair.vacant_since.is_some(),
        "waits until its bodies are found"
    );
    let p0 = s.pressure(0);
    assert!(p0 > 1.4);
    s.census(1, false, &[body(0x21, 0, 700.5)], 100);
    let a = adopts(&s.cmds());
    assert_eq!(a.len(), 1);
    let lair = &s.d.lairs[s.l as usize];
    assert_eq!(lair.occupants.len(), 1);
    assert!(lair.vacant_since.is_none(), "no wait is left");
    assert!(lair.expect.is_empty());
    assert!((s.pressure(0) - p0).abs() < 0.01, "the pressure is kept");
}

/// An expected body not found three times over two days is given up: no pressure, the cap's wait.
#[test]
fn a_body_not_found_after_a_reset_is_other_and_waits_the_cap() {
    let mut s = reset_lair(27, vec![uuid(0x24)], false);
    let p0 = s.pressure(0);
    for k in 0..3 {
        s.census(1, false, &[], 100 + k * DAY);
    }
    let lair = &s.d.lairs[s.l as usize];
    assert!(lair.expect.is_empty());
    assert_eq!(s.lost(), 0);
    assert!((s.pressure(3) - p0 * 0.5f32.powf(3.0 / 96.0)).abs() < 0.05);
    assert!(lair.vacant_since.is_some());
}

/// The lair's row: K 2, home, left-behind and missing bodies, each bounded to two.
#[test]
fn a_lairs_row_carries_home_left_and_missing_bodies_bounded_to_two() {
    let mut s = scene(28);
    let a = s.dragon("A", Gender::Female);
    let b = s.dragon("B", Gender::Male);
    let c = s.dragon("C", Gender::Female);
    for (x, u) in [(a, 1), (b, 2), (c, 3)] {
        s.d.dragons.get_mut(x).unwrap().body = Some(uuid(u));
    }
    {
        let lair = &mut s.d.lairs[s.l as usize];
        lair.left_behind = vec![uuid(10), uuid(11), uuid(12)];
        lair.missing = vec![uuid(20), uuid(21), uuid(22)];
    }
    let row = s.d.lair_site_state(s.l, Tick(10)).unwrap();
    assert_eq!(row.k, 2);
    assert_eq!(row.count, 3);
    let mut r = row.clone();
    r.bound();
    assert_eq!(r.bodies.home.len(), 2);
    assert_eq!(r.bodies.left_behind.len(), 2);
    assert_eq!(r.bodies.missing, vec![uuid(21), uuid(22)], "the newest two");
}

// ---- the replay gate (EQC123) -----------------------------------------------------------------

/// While a session's replay runs a census only marks its lair: no dragon is made, no line sent.
#[test]
fn a_replayed_census_marks_in_and_sends_nothing() {
    let mut s = scene(29);
    s.d.begin_replay(true);
    s.census(1, false, &[body(0x11, 0, 700.5)], 100);
    assert!(s.cmds().is_empty());
    assert!(s.d.lairs[s.l as usize].occupants.is_empty());
    s.d.replay_done(&mut s.w, Tick(200));
}

// ---- the S5 review's findings (EQC310 onward) -------------------------------------------------

/// A live dragon of another lair, with the agent `agent_of(y)`: an agent a rebuilt engine (or a
/// crash's replay) handed out again, as a body of this lair may still carry an old one.
fn other_lairs_dragon(s: &mut Scene) -> DragonId {
    let other = s.d.add_lair(
        &mut s.w,
        KEY + 9,
        "Elsewhere",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 6000, 6000),
        100,
        Tick(0),
    );
    s.d.add_dragon(
        &mut s.w,
        newdragon("Elsewhere", other, Gender::Male, 110),
        Tick(0),
    )
}

/// EQC310 (finding 1): after a reset the lair expects its body by UUID; the body still carries an
/// agent of the old engine life, which in the new engine is a live dragon of another lair. The body
/// is the lair's own, matched by its UUID first: adopted, not removed as that dragon's.
#[test]
fn a_reset_lairs_body_carrying_another_lairs_agent_is_adopted_by_its_uuid() {
    let mut s = reset_lair(40, vec![uuid(0x2A)], false);
    let y = other_lairs_dragon(&mut s);
    s.census(1, false, &[body(0x2A, agent_of(y), 700.5)], 100);
    let cmds = s.cmds();
    assert!(
        discards(&cmds).iter().all(|d| d.entity != uuid(0x2A)),
        "the lair's own body is not removed: {cmds:?}"
    );
    let a = adopts(&cmds);
    assert_eq!(a.len(), 1, "adopted by its UUID: {cmds:?}");
    assert_eq!(a[0].entity, uuid(0x2A));
    assert_ne!(a[0].agent, agent_of(y), "a dragon of its own, not the other lair's");
    let lair = &s.d.lairs[s.l as usize];
    assert_eq!(lair.occupants.len(), 1);
    assert!(lair.vacant_since.is_none(), "no wait for the cap");
    assert_eq!(
        s.d.dragon(y).unwrap().body,
        None,
        "the other lair's dragon is not given it"
    );
}

/// EQC310 (finding 1): an agent that names a live dragon of another lair, on a body at an empty
/// lair with no row: a stray, adopted as the lair's own, not removed as the other dragon's.
#[test]
fn an_agent_of_another_lairs_dragon_at_an_empty_lair_is_a_stray_not_removed() {
    let mut s = scene(41);
    let y = other_lairs_dragon(&mut s);
    s.census(1, false, &[body(0x2B, agent_of(y), 700.5)], 100);
    let cmds = s.cmds();
    assert!(discards(&cmds).is_empty(), "nothing removed: {cmds:?}");
    let a = adopts(&cmds);
    assert_eq!(a.len(), 1, "{cmds:?}");
    assert_eq!(a[0].entity, uuid(0x2B));
    assert_ne!(a[0].agent, agent_of(y));
    assert_eq!(s.d.lairs[s.l as usize].occupants.len(), 1);
    assert_eq!(s.d.stats.strays_adopted, 1);
}

/// EQC311 (finding 2): a dragon that is a puppet (led past the leash) and an old home body of it
/// listed by a watched census: the old body is removed, watched or not, so it does not tick beside
/// the puppet (H1).
#[test]
fn a_watched_census_removes_an_old_body_of_a_dragon_that_is_a_puppet() {
    let mut s = scene(42);
    let x = s.dragon("Lured", Gender::Female);
    let (agent, epoch) = s.homed(x, 0x5A, 100);
    s.ingest(
        Obs::DragonLeftHome(boundary::DragonLeftHome {
            agent,
            epoch,
            dimension: 0,
            entity: uuid(0x5A),
            pos: (760.0, 100.0, 1500.0),
        }),
        200,
    );
    s.cmds();
    // A crash between the swap and the chunk's rewrite left the old home body on disk.
    s.census(2, true, &[body(0x5B, agent, 701.5)], 300);
    let cmds = s.cmds();
    let gone = discards(&cmds);
    assert_eq!(gone.len(), 1, "the old body goes: {cmds:?}");
    assert_eq!(gone[0].entity, uuid(0x5B));
    assert!(adopts(&cmds).is_empty(), "{cmds:?}");
    assert_eq!(s.d.lairs[s.l as usize].occupants, vec![x], "no second dragon");
}

/// EQC313 (finding 4): a lair that got a newcomer while it was seen only from afar, and Ice and
/// Fire's own generated dragon found by its first census: the generated dragon is removed (the
/// untagged path's rule), not adopted as a second occupant.
#[test]
fn a_lairs_generated_dragon_found_after_a_newcomer_is_removed_not_adopted() {
    let mut s = scene(43);
    let x = s.dragon("Newcomer", Gender::Female);
    s.census(1, false, &[body(0x4A, 0, 702.5)], 100);
    let cmds = s.cmds();
    assert!(adopts(&cmds).is_empty(), "no second occupant: {cmds:?}");
    let gone = discards(&cmds);
    assert_eq!(gone.len(), 1, "{cmds:?}");
    assert_eq!(
        (gone[0].entity, gone[0].reason),
        (uuid(0x4A), DISCARD_LAIR_FULL)
    );
    assert_eq!(s.d.lairs[s.l as usize].occupants, vec![x]);
    assert_eq!(s.d.stats.strays_adopted, 0);
    assert_eq!(
        materializes(&cmds).len(),
        1,
        "the newcomer, owed, gets its body"
    );
}

/// EQC315 (finding 6): a passing puppet (its departure booked) settles at its lair and is handed
/// back as a home body. Its booked departure and the wound rule are a puppet's: the home body is
/// never sent away.
#[test]
fn a_handed_back_body_keeps_no_departure() {
    let mut s = scene(44);
    let x = s.dragon("Passer", Gender::Female);
    s.run(90_100);
    assert!(
        matches!(s.d.dragon(x).unwrap().state, State::Flying(_)),
        "it left to hunt: {:?}",
        s.d.dragon(x).unwrap().state
    );
    s.cmds();
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 700, 1500),
        Tick(90_100),
        Tick(400_000),
    );
    s.run(90_200);
    let m = materializes(&s.cmds())
        .into_iter()
        .find(|m| m.agent == agent_of(x))
        .expect("the player sees it pass");
    assert_eq!(m.flags & boundary::MAT_HOME_BODY, 0, "a puppet");
    let (agent, epoch) = (m.agent, m.epoch);
    let State::Embodied { depart_at, .. } = s.d.dragon(x).unwrap().state else {
        panic!("embodied")
    };
    assert_ne!(depart_at.0, u64::MAX, "a passing puppet books its departure");
    s.ingest(
        Obs::Materialized(boundary::Materialized {
            agent,
            epoch,
            dimension: 0,
            entity: uuid(0x6A),
            home: false,
        }),
        90_201,
    );
    s.census(1, true, &[], 90_210);
    s.cmds();
    s.ingest(
        Obs::State(boundary::State {
            agent,
            epoch,
            flags: boundary::STATE_SLEEPING,
            dimension: 0,
            health: 400.0,
            max_health: 500.0,
            hunger: 80,
            age_ticks: 1,
            pos: (701.5, 100.0, 1500.5),
        }),
        90_220,
    );
    assert!(
        s.cmds().iter().any(|c| matches!(c, Cmd::HandBack(h) if h.agent == agent)),
        "settled at its lair, the lair in: handed back"
    );
    s.answer(agent, epoch, uuid(0x6A), 90_230);
    assert_eq!(s.d.dragon(x).unwrap().body, Some(uuid(0x6A)));
    s.run(depart_at.0 + 100);
    let cmds = s.cmds();
    assert!(
        cmds.iter()
            .all(|c| !matches!(c, Cmd::Depart(dp) if dp.agent == agent)),
        "the home body is not sent away: {cmds:?}"
    );
    let dx = s.d.dragon(x).unwrap();
    assert!(
        matches!(dx.state, State::Embodied { depart_at, harass: false, .. } if depart_at.0 == u64::MAX),
        "{:?}",
        dx.state
    );
    // A heavy wound does not send it off either (the wound rule is a puppet's).
    let t = s.d.now().0 + 10;
    s.ingest(
        Obs::Hurt(boundary::Hurt {
            agent,
            epoch,
            attacker_kind: AttackerKind::Player,
            attacker: PLAYER,
            hits: 9,
            amount: 10_000.0,
            health_after: 1.0,
        }),
        t,
    );
    assert!(
        s.cmds()
            .iter()
            .all(|c| !matches!(c, Cmd::Depart(dp) if dp.agent == agent)),
        "wounded, a home body still stays"
    );
}

/// A square oil field of `side` by `side` chunks from chunk (cx, cz): the engine knows an oil field,
/// so its fire dragons carry a reserve (OG-4).
fn oil_field(cx: i32, cz: i32, side: i32) -> Obs {
    Obs::OilChunks(boundary::OilChunks {
        dimension: 0,
        source: boundary::OIL_SOURCE_DEEP_TIME,
        chunks: (0..side * side)
            .map(|k| boundary::OilChunkRec {
                cx: cx + k % side,
                cz: cz + k / side,
                amount: 1,
                flags: 0,
            })
            .collect(),
    })
}

/// EQC317 (finding 8): a fire dragon's home body adopted by a census carries the dragon's oil
/// reserve, as a spawned body does (OG-4, OG-13).
#[test]
fn a_census_adoption_carries_the_oil_reserve() {
    let mut s = scene(45);
    s.ingest(oil_field(40, 90, 3), 10);
    let x = s.dragon("Fiery", Gender::Female);
    s.census(1, false, &[body(0x8A, agent_of(x), 700.5)], 100);
    let a = adopts(&s.cmds());
    assert_eq!(a.len(), 1);
    assert_ne!(
        a[0].flags & boundary::ADOPT_OIL,
        0,
        "the reserve rides on the adoption: {:?}",
        a[0]
    );
    let want = s.d.dragon(x).unwrap().oil_level(&s.d.tun.oil, Tick(100));
    assert!(want > 0, "a reserve to carry: {want}");
    assert_eq!(a[0].oil, want, "the dragon's own reserve");
}

/// EQC318 (finding 9): a home spawn refused because the roost's chunk does not entity-tick (a
/// simulation distance under the view distance) holds only a moment: the next sighting asks again,
/// not a day later. A refusal still makes no record a step.
#[test]
fn a_spawn_refused_for_a_chunk_not_ticking_is_asked_again_soon() {
    let mut s = scene(46);
    let x = s.dragon("Owed", Gender::Female);
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 700, 1500),
        Tick(100),
        Tick(3_000),
    );
    s.run(120);
    let m = materializes(&s.cmds());
    assert_eq!(m.len(), 1, "seen at its lair");
    assert_eq!(m[0].agent, agent_of(x));
    s.ingest(
        Obs::SpawnRejected(boundary::SpawnRejected {
            agent: m[0].agent,
            epoch: m[0].epoch,
            reason: 1,
            cmd_seq: 1,
        }),
        130,
    );
    s.run(200);
    assert!(
        materializes(&s.cmds()).is_empty(),
        "not again at once: it settles at its lair, and the refusal holds"
    );
    // The player comes closer: its area is booked again, and the roost's chunk ticks now.
    s.d.player_stays(
        &mut s.w,
        PLAYER,
        Pos::new(0, 710, 1500),
        Tick(2_500),
        Tick(150_000),
    );
    s.run(2_520);
    let m2 = materializes(&s.cmds());
    assert_eq!(m2.len(), 1, "asked again at the next sighting");
    assert_ne!(m2[0].flags & boundary::MAT_HOME_BODY, 0);
}

/// The engine's commands through the fake plugin and its answers due by `at`, until quiet.
fn pump(s: &mut Scene, iaf: &mut synth::FakeIaf, at: Tick) {
    loop {
        let cmds = s.d.take_commands();
        let quiet = cmds.is_empty();
        iaf.apply(cmds, at);
        let due = iaf.due(at);
        if quiet && due.is_empty() {
            break;
        }
        for (t, o) in due {
            let t = t.max(s.d.now());
            s.d.ingest(&mut s.w, &mut NoInfrastructure, &o, t);
        }
    }
}

/// EQC314 (finding 5): a dragon dead of age while its home body is loaded: the plugin discards the
/// body at once on DEPART reason 5 (Java waited for the capture, and a lair going out in that window
/// saved a dead dragon's body, which the next census adopted). The lair going out and coming in
/// again finds nothing: no body saved, no stray.
#[test]
fn a_dead_of_age_dragons_home_body_is_gone_at_once_and_never_comes_back() {
    let mut s = scene(48);
    let life = s.d.tun.species(Species::Fire).lifespan_ticks();
    let x = s.d.add_dragon(
        &mut s.w,
        NewDragon {
            age_ticks: life,
            ..newdragon("Old", s.l, Gender::Female, 0)
        },
        Tick(0),
    );
    let mut iaf = synth::FakeIaf::new(PLAYER);
    iaf.home.on = true;
    iaf.home.lairs.insert(KEY, (700, 1500));
    iaf.set_lair_in(KEY, false, Tick(100));
    pump(&mut s, &mut iaf, Tick(100));
    pump(&mut s, &mut iaf, Tick(110));
    assert!(s.d.dragon(x).unwrap().body.is_some(), "its owed body spawned");
    s.run(3 * DAY);
    assert!(s.d.dragon(x).is_none(), "dead of age");
    pump(&mut s, &mut iaf, Tick(3 * DAY));
    assert!(
        iaf.home.bodies.is_empty(),
        "discarded at once: {:?}",
        iaf.home.bodies
    );
    iaf.set_lair_out(KEY, Tick(3 * DAY + 10));
    pump(&mut s, &mut iaf, Tick(3 * DAY + 10));
    iaf.set_lair_in(KEY, false, Tick(3 * DAY + 400));
    pump(&mut s, &mut iaf, Tick(3 * DAY + 400));
    assert_eq!(s.d.stats.strays_adopted, 0, "the dead dragon does not come back");
    assert!(s.d.lairs[s.l as usize].occupants.is_empty());
}
