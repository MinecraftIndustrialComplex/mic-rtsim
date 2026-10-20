// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 2 engine's contracts (PLAN.md §9 exit tests, headless):
//! - a leg crossing a player's interest area schedules exactly one event (A15);
//! - a dragon nobody watches costs no per-tick work;
//! - conservation with predation (no haven: eaten and burnt are consumption, hoards are holders);
//! - grudges are inherited (C24), down to the lair and its next occupant;
//! - threats get bored, and wounds bring the departure forward (C24);
//! - a raid on an unwatched district is never better for the defender than observed (I22), and a
//!   watched district gets a real fight instead (P28);
//! - the population: untagged dragons adopted or discarded (P13), and the respawn delay.

use proptest::prelude::*;
use rtsim_ecology::boundary::{self, AttackerKind, Cmd, Obs, SpawnKind};
use rtsim_ecology::dragon::{DragonId, LairKind, agent_of};
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::obs::{FightObserved, Obs as IndObs, Timed};
use rtsim_industry::threat::ThreatClass;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::World;

const PLAYER: (u64, u64) = (5, 6);

fn world(seed: u64) -> (World, Dragons) {
    let (w, _) = synth::world(seed);
    (w, Dragons::new(seed, DragonTunables::default(), Tick::ZERO))
}

/// A seed whose temperament is aggressive (C25): a grudge pulls it hardest.
fn aggressive_seed() -> u64 {
    (0..)
        .find(|&s| rtsim_knowledge::Temperament::from_seed(s).aggressive >= 200)
        .unwrap()
}

fn adult(name: &str, lair: u32, fullness: i32, gender: Gender) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender,
        variant: 0,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(lair),
        fullness,
        health: None,
        seed: None,
        at: None,
    }
}

fn materializations(cmds: &[Cmd]) -> Vec<boundary::Materialize> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Materialize(m) => Some(*m),
            _ => None,
        })
        .collect()
}

// ---- legs and interest areas (A15) ------------------------------------------------------------

/// A dragon flies from one lair to another across a player standing on its path; the two legs
/// share many index cells, but exactly one `Seen` event is booked and fires, whichever is booked
/// first, and the dragon materializes at the edge of the player's area. A player off the path
/// books nothing.
#[test]
fn a_leg_crossing_an_interest_area_schedules_exactly_one_event() {
    for (player_first, on_path) in [(true, true), (false, true), (true, false), (false, false)] {
        let (mut w, mut d) = world(3);
        let a = d.add_lair(
            &mut w,
            1,
            "A",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, -3000, 3000),
            100,
            Tick(0),
        );
        let b = d.add_lair(
            &mut w,
            2,
            "B",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 3000, 3000),
            100,
            Tick(0),
        );
        let x = d.add_dragon(&mut w, adult("x", a, 100, Gender::Female), Tick(0));
        let t0 = Tick(1_000);
        d.run_until(&mut w, &mut NoInfrastructure, t0);
        let spot = if on_path {
            Pos::new(0, 0, 3020)
        } else {
            Pos::new(0, 0, 4000)
        };
        if player_first {
            d.player_stays(&mut w, PLAYER, spot, t0, t0 + 100_000);
        }
        d.relocate(&mut w, x, b, t0);
        if !player_first {
            d.player_stays(&mut w, PLAYER, spot, t0, t0 + 100_000);
        }
        // 6,000 blocks at 0.5 blocks a tick: the flight takes 12,000 ticks.
        d.run_until(&mut w, &mut NoInfrastructure, t0 + 20_000);
        let mats = materializations(&d.take_commands());
        let want = usize::from(on_path);
        assert_eq!(
            mats.len(),
            want,
            "player first {player_first}, on path {on_path}"
        );
        assert_eq!(
            d.stats.seen + d.stats.seen_stale,
            want as u64,
            "one event booked"
        );
        if let Some(m) = mats.first() {
            assert_eq!(m.agent, agent_of(x));
            let r = ((m.pos.0 - f64::from(spot.x)).powi(2) + (m.pos.2 - f64::from(spot.z)).powi(2))
                .sqrt();
            let reach = f64::from(d.tun.view_radius + rtsim_ecology::engine::BODY_RADIUS);
            assert!(
                (r - reach).abs() <= 2.0,
                "materialized {r:.1} blocks away, the area's edge is {reach}"
            );
            // At the forced cruise altitude while it flew unloaded.
            assert_eq!(m.pos.1, f64::from(d.tun.species(Species::Fire).cruise_y));
            // Its NBT comes from engine state: fullness fell one point per 3,000 ticks since tick 0.
            assert!((96..=100).contains(&m.hunger), "hunger {}", m.hunger);
        }
    }
}

/// A player who keeps moving inside the area replans its interest leg: the old meeting goes stale
/// and a new one is booked, so the dragon still materializes once.
#[test]
fn replanned_interest_areas_cancel_their_meetings_in_cascade() {
    let (mut w, mut d) = world(3);
    let a = d.add_lair(
        &mut w,
        1,
        "A",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, -3000, 3000),
        100,
        Tick(0),
    );
    let b = d.add_lair(
        &mut w,
        2,
        "B",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 3000, 3000),
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("x", a, 100, Gender::Female), Tick(0));
    let t0 = Tick(1_000);
    d.run_until(&mut w, &mut NoInfrastructure, t0);
    d.relocate(&mut w, x, b, t0);
    // The player walks along the path toward the dragon, reporting every 20 ticks.
    let mut t = t0;
    let mut px = 1_500;
    while t.0 < t0.0 + 14_000 {
        d.observe_player(&mut w, PLAYER, Pos::new(0, px, 3010), t);
        d.run_until(&mut w, &mut NoInfrastructure, t);
        t += 20;
        px -= 1;
    }
    let mats = materializations(&d.take_commands());
    assert_eq!(mats.len(), 1);
    assert_eq!(d.stats.seen, 1);
    assert!(
        d.stats.seen_stale > 0,
        "meetings booked against old areas went stale"
    );
}

// ---- cost -------------------------------------------------------------------------------------

fn populated(seed: u64) -> (World, Dragons) {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    (w, d)
}

/// Stepping the engine every tick does exactly the work of stepping it once: the same events,
/// the same state. Nothing happens per tick for a dragon nobody watches, and a dragon's events
/// per day are a handful.
#[test]
fn an_unobserved_dragon_costs_no_per_tick_work() {
    let days = 5;
    let end = Tick::from_days(days);
    let (mut w1, mut d1) = populated(11);
    d1.run_until(&mut w1, &mut NoInfrastructure, end);
    let (mut w2, mut d2) = populated(11);
    let mut t = Tick::ZERO;
    while t < end {
        t += 1;
        d2.run_until(&mut w2, &mut NoInfrastructure, t);
    }
    assert_eq!(d1.stats, d2.stats);
    assert_eq!(d1.state_hash(), d2.state_hash());
    assert_eq!(w1.state_hash(), w2.state_hash());
    let n = d1.dragons.len() as f64;
    let per_dragon_day = d1.stats.events as f64 / (n * days as f64);
    assert!(
        per_dragon_day < 15.0,
        "{per_dragon_day:.1} events per dragon per day"
    );
    assert!(d1.stats.hunts > 0);
}

// ---- conservation -----------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4))]

    /// What dragons take leaves the world only as consumption, or sits in a hoard: the world's
    /// books balance, everything they count as destroyed is in the dragons' ledger, the ledger
    /// splits exactly, and the hoards hold what it says, all along the run.
    #[test]
    fn conservation_with_predation(seed in 0u64..1_000) {
        let (mut w, mut d) = populated(seed);
        for day in 1..=30 {
            d.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(day));
            if day % 3 == 0 {
                prop_assert!(d.check(&w, None).is_ok(), "{:?}", d.check(&w, None));
            }
        }
        prop_assert!(d.ledger.taken.values().sum::<i128>() > 0);
    }
}

// ---- grudges and heirs (C24) --------------------------------------------------------------------

fn embodied_at_home(
    w: &mut World,
    d: &mut Dragons,
    lair_pos: Pos,
    at: Tick,
) -> Vec<boundary::Materialize> {
    d.player_stays(w, PLAYER, lair_pos, at, at + 50_000);
    d.run_until(w, &mut NoInfrastructure, at + 10);
    materializations(&d.take_commands())
}

/// A wound is a grudge; a kin killed is a grudge; the dead dragon's strongest feelings pass to
/// its heir; when the line ends, the lair keeps them, and the next dragon to claim it takes them
/// over.
#[test]
fn grudges_are_inherited_down_to_the_lair_and_its_next_occupant() {
    let (mut w, mut d) = world(5);
    let lp = Pos::new(0, 4000, 4000);
    let l = d.add_lair(
        &mut w,
        9,
        "Lair",
        Species::Fire,
        LairKind::Roost,
        lp,
        100,
        Tick(0),
    );
    let parent = d.add_dragon(&mut w, adult("parent", l, 100, Gender::Female), Tick(0));
    let mut young = adult("young", l, 100, Gender::Male);
    young.age_ticks = 30 * TICKS_PER_DAY;
    let child = d.add_dragon(&mut w, young, Tick(0));
    d.set_parent(&mut w, child, parent, Tick(0));
    let mats = embodied_at_home(&mut w, &mut d, lp, Tick(100));
    assert_eq!(mats.len(), 2, "both roost there and materialize at home");
    let mp = *mats.iter().find(|m| m.agent == agent_of(parent)).unwrap();
    assert!(mp.flags & boundary::MAT_SLEEPING != 0);
    assert_eq!(mp.hunger, 100);
    assert_eq!(mp.home, (lp.x, 100, lp.z));
    let player = d.player(&mut w, PLAYER);
    let at = Tick(500);
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Hurt(boundary::Hurt {
            agent: mp.agent,
            epoch: mp.epoch,
            attacker_kind: AttackerKind::Player,
            attacker: PLAYER,
            hits: 3,
            amount: mp.health.max(400.0) * 0.4,
            health_after: 200.0,
        }),
        at,
    );
    let pa = d.dragon(parent).unwrap().actor;
    let ca = d.dragon(child).unwrap().actor;
    let grudge = d.tun.grudge_below;
    assert!(
        w.knowledge.sentiment(pa, player, at) <= grudge,
        "a wound is a grudge"
    );
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Died(boundary::Died {
            agent: mp.agent,
            epoch: mp.epoch,
            killer_kind: AttackerKind::Player,
            killer: PLAYER,
            dimension: 0,
            pos: (f64::from(lp.x), 100.0, f64::from(lp.z)),
        }),
        Tick(600),
    );
    assert!(d.dragon(parent).is_none());
    assert!(w.knowledge.notable(pa).is_none());
    let felt = w.knowledge.sentiment(ca, player, Tick(600));
    assert!(
        felt <= grudge && felt <= -100,
        "the heir holds the grudge ({felt})"
    );
    // The heir remembers what its parent remembered.
    assert!(!w.knowledge.notable(ca).unwrap().memories().is_empty());
    // The line ends: the lair keeps the grudge.
    let mc = *mats.iter().find(|m| m.agent == agent_of(child)).unwrap();
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Died(boundary::Died {
            agent: mc.agent,
            epoch: mc.epoch,
            killer_kind: AttackerKind::Player,
            killer: PLAYER,
            dimension: 0,
            pos: (f64::from(lp.x), 100.0, f64::from(lp.z)),
        }),
        Tick(700),
    );
    let lair_actor = d.lairs[l as usize].actor;
    assert!(
        w.knowledge.sentiment(lair_actor, player, Tick(700)) <= grudge,
        "the lair keeps it"
    );
    assert!(d.lairs[l as usize].occupants.is_empty());
    // Not before the respawn delay; then a young dragon claims the lair and takes the grudge over.
    d.player_left(&mut w, player);
    let delay = d.tun.respawn_delay_days * TICKS_PER_DAY;
    d.run_until(&mut w, &mut NoInfrastructure, Tick(700 + delay - 10));
    assert_eq!(d.stats.respawns, 0);
    d.run_until(&mut w, &mut NoInfrastructure, Tick(700 + delay + 20_000));
    assert_eq!(d.stats.respawns, 1);
    let newcomer = d.lairs[l as usize]
        .occupants
        .first()
        .copied()
        .expect("claimed");
    let na = d.dragon(newcomer).unwrap().actor;
    let felt = w.knowledge.sentiment(na, player, d.now());
    assert!(
        felt <= grudge,
        "the newcomer takes the lair's grudge over ({felt})"
    );
}

// ---- threats get bored (C24) ----------------------------------------------------------------------

/// The time and reason of the first departure after a grudge visit to the player's home, with
/// and without a wound right after it materializes.
fn grudge_visit(wound: bool) -> (Tick, boundary::Depart, Tick) {
    let (mut w, mut d) = world(9);
    let home = Pos::new(0, 3000, 5000);
    let l = d.add_lair(
        &mut w,
        4,
        "L",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 4200, 5000),
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("x", l, 61, Gender::Male), Tick(0));
    let player = d.player(&mut w, PLAYER);
    let xa = d.dragon(x).unwrap().actor;
    w.knowledge.adjust(xa, player, -110, Tick(0));
    // The player lives there: a day of position reports, and an interest area for the visit.
    let mut t = Tick(0);
    while t.0 < 20_000 {
        d.observe_player(&mut w, PLAYER, home, t);
        d.run_until(&mut w, &mut NoInfrastructure, t);
        t += 1_200;
    }
    d.player_stays(&mut w, PLAYER, home, t, t + 100_000);
    let mut mat = None;
    let mut dep = None;
    while t.0 < 100_000 && dep.is_none() {
        t += 20;
        d.run_until(&mut w, &mut NoInfrastructure, t);
        for c in d.take_commands() {
            match c {
                Cmd::Materialize(m) => {
                    assert!(
                        m.flags & boundary::MAT_HOSTILE != 0,
                        "a grudge visit is hostile"
                    );
                    assert_eq!(m.target_player, PLAYER);
                    mat = Some((t, m));
                    if wound {
                        d.ingest(
                            &mut w,
                            &mut NoInfrastructure,
                            &Obs::Hurt(boundary::Hurt {
                                agent: m.agent,
                                epoch: m.epoch,
                                attacker_kind: AttackerKind::Player,
                                attacker: PLAYER,
                                hits: 2,
                                amount: 100.0,
                                health_after: 300.0,
                            }),
                            t + 1,
                        );
                    }
                }
                Cmd::Depart(dp) => dep = Some((t, dp)),
                _ => {}
            }
        }
    }
    let (tm, _) = mat.expect("it came for the player");
    let (td, dp) = dep.expect("it left");
    (tm, dp, td)
}

#[test]
fn threats_get_bored_and_wounds_bring_the_departure_forward() {
    let (m0, d0, t0) = grudge_visit(false);
    assert_eq!(d0.reason, 1, "bored");
    let stay = t0.0 - m0.0;
    assert!((2_000..=10_000).contains(&stay), "a visit of {stay} ticks");
    let (m1, d1, t1) = grudge_visit(true);
    assert_eq!(d1.reason, 3, "wounded");
    assert!(t1.0 - m1.0 < stay, "the wound shortened the visit");
}

// ---- infrastructure: never better than observed, and watched fights (C16 to C18, P28) -----------

fn grudge_raid(
    watched: bool,
    turrets: f32,
    observed: Option<(u32, u32)>,
) -> (Dragons, rtsim_industry::fake::FakeWorld, World) {
    let (mut w, mut d) = world(13);
    // Watched: someone stays at the farm all along (the fake world loads it, and a loaded
    // district with a player near is watched).
    let visits = if watched {
        vec![rtsim_industry::fake::Visit {
            from: Tick(0),
            at: vec![0],
        }]
    } else {
        Vec::new()
    };
    let mut fake = synth::farm(13, [turrets; 4], visits);
    fake.run_until(Tick(1));
    if let Some((attackers, killed)) = observed {
        fake.engine.ingest(&Timed {
            at: Tick(1),
            obs: IndObs::Fight(FightObserved {
                district: synth::FARM,
                class: ThreatClass::Flying,
                attackers,
                // Stage 5: 4 passes of strength 4.
                attacker_strength: 16.0,
                attackers_killed: killed,
                blocks_destroyed: 0,
                machines_destroyed: 0,
                guards_lost: 0,
                items_stolen: 0,
                duration_ticks: 600,
            }),
        });
    }
    let farm = Pos::new(0, 302, 1301);
    let l = d.add_lair(
        &mut w,
        4,
        "L",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 1800, 2500),
        100,
        Tick(0),
    );
    // Hungry at 30,000 ticks, after the player has left.
    let mut n = adult("x", l, 70, Gender::Male);
    n.seed = Some(aggressive_seed());
    let x = d.add_dragon(&mut w, n, Tick(0));
    let player = d.player(&mut w, PLAYER);
    let xa = d.dragon(x).unwrap().actor;
    w.knowledge.adjust(xa, player, -127, Tick(0));
    let mut index = DistrictIndex::default();
    let mut t = Tick(0);
    while t.0 < 30_000 {
        fake.run_until(t.max(Tick(1)));
        d.observe_player(&mut w, PLAYER, farm, t);
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index: &mut index,
        };
        d.run_until(&mut w, &mut link, t);
        t += 1_200;
    }
    // The player leaves for good (unwatched unless `watched` says the district still is).
    d.player_left(&mut w, player);
    while t.0 < 80_000 {
        t += 1_000;
        fake.run_until(t);
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index: &mut index,
        };
        d.run_until(&mut w, &mut link, t);
    }
    (d, fake, w)
}

#[test]
fn an_unwatched_raid_is_never_better_than_observed() {
    // Strong turrets that have never fought: the untested weight applies, still enough to stop it.
    let (d, fake, w) = grudge_raid(false, 200.0, None);
    let a = fake
        .engine
        .district(synth::FARM)
        .unwrap()
        .attacks
        .back()
        .cloned()
        .expect("raided");
    let o = a.outcome.expect("resolved abstractly");
    assert!(o.untested);
    assert_eq!(d.stats.district_raids, 1);
    // The same turrets after a real fight where they killed one pass in four: the abstract raid
    // can do no better for the defender than that.
    let (d2, fake2, w2) = grudge_raid(false, 200.0, Some((4, 1)));
    let a2 = fake2
        .engine
        .district(synth::FARM)
        .unwrap()
        .attacks
        .back()
        .cloned()
        .expect("raided");
    let o2 = a2.outcome.unwrap();
    assert_eq!(o2.floor, Some(0.75));
    assert!(
        o2.survived >= 0.75,
        "never better than observed: {}",
        o2.survived
    );
    assert!(
        o2.lanchester < 0.75,
        "Lanchester alone would have done better for the defender"
    );
    // Damage is capped (I23) and the books agree with the ledger.
    assert!(a2.destroyed <= fake2.engine.tun.threat.destroy_cap_blocks);
    d2.check(&w2, Some(&fake2.engine.books.stolen)).unwrap();
    d.check(&w, Some(&fake.engine.books.stolen)).unwrap();
}

#[test]
fn a_watched_district_gets_a_real_fight_and_no_abstract_damage() {
    let (d, fake, _) = grudge_raid(true, 0.0, None);
    assert_eq!(d.stats.district_raids, 0);
    let farm = fake.engine.district(synth::FARM).unwrap();
    assert!(farm.watched);
    assert!(
        farm.attacks
            .iter()
            .all(|a| a.source != rtsim_industry::threat::ThreatSource::Dragon)
    );
    assert!(d.stats.materialized >= 1, "the dragon came in person");
}

// ---- the population (P13) -----------------------------------------------------------------------

#[test]
fn untagged_dragons_are_adopted_or_discarded() {
    let (mut w, mut d) = world(17);
    let l = d.add_lair(
        &mut w,
        77,
        "Cave",
        Species::Ice,
        LairKind::Cave,
        Pos::new(0, -4000, 4000),
        30,
        Tick(0),
    );
    let u = |entity: u64, spawn: SpawnKind, flags: u32, lair: u64| {
        Obs::Untagged(boundary::Untagged {
            entity: (entity, entity),
            spawn,
            species: Species::Ice,
            dimension: 0,
            flags,
            age_ticks: 90 * 24_000,
            hunger: 50,
            gender: Gender::Female,
            variant: 1,
            home: (-4000, 30, 4000),
            lair,
            pos: (-4000.0, 30.0, 4000.0),
        })
    };
    let mut infra = NoInfrastructure;
    // The lair's own dragon from world generation: adopted as its occupant.
    d.ingest(
        &mut w,
        &mut infra,
        &u(1, SpawnKind::Structure, boundary::UNTAGGED_SLEEPING, 77),
        Tick(10),
    );
    // A second structure dragon for the same lair: discarded (it has its dragon).
    d.ingest(
        &mut w,
        &mut infra,
        &u(2, SpawnKind::Structure, 0, 77),
        Tick(11),
    );
    // A natural or spawner dragon: discarded (the sim owns the population).
    d.ingest(
        &mut w,
        &mut infra,
        &u(3, SpawnKind::Spawner, 0, 0),
        Tick(12),
    );
    // A tamed one is the player's (A10): left alone.
    d.ingest(
        &mut w,
        &mut infra,
        &u(4, SpawnKind::SpawnEgg, boundary::UNTAGGED_TAMED, 0),
        Tick(13),
    );
    // A spawn egg's: adopted, as a wanderer.
    d.ingest(
        &mut w,
        &mut infra,
        &u(5, SpawnKind::SpawnEgg, 0, 0),
        Tick(14),
    );
    let cmds = d.take_commands();
    let adopted: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Adopt(a) => Some(a.entity.0),
            _ => None,
        })
        .collect();
    let discarded: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Discard(x) => Some((x.entity.0, x.reason)),
            _ => None,
        })
        .collect();
    assert_eq!(adopted, vec![1, 5]);
    assert_eq!(discarded, vec![(2, 2), (3, 1)]);
    assert_eq!(d.lairs[l as usize].occupants.len(), 1);
    assert_eq!(d.stats.adopted, 2);
    // Adopted dragons are embodied: when the plugin captures one, it goes on abstractly.
    let a1 = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Adopt(a) if a.entity.0 == 1 => Some(*a),
            _ => None,
        })
        .unwrap();
    d.ingest(
        &mut w,
        &mut infra,
        &Obs::Dematerialized(boundary::Dematerialized {
            agent: a1.agent,
            epoch: a1.epoch,
            reason: 3,
            dimension: 0,
            health: 400.0,
            max_health: 400.0,
            hunger: 50,
            age_ticks: 90 * 24_000,
            pos: (-4000.0, 30.0, 4000.0),
        }),
        Tick(100),
    );
    let id: DragonId = rtsim_ecology::dragon::dragon_of(a1.agent).unwrap();
    assert!(!d.dragon(id).unwrap().is_embodied());
    d.run_until(&mut w, &mut infra, Tick::from_days(3));
    assert!(
        d.stats.hunts + d.stats.fruitless > 0,
        "it lives on in the sim"
    );
}

// ---- trains (INDUSTRY.md §13.3, I24) -------------------------------------------------------------

/// A hungry dragon whose flight crosses a booked train leg meets the train while both are under
/// way: the industry engine loots the carriages within its caps, and the books agree with the
/// dragons' ledger. A fed dragon flies on.
#[test]
fn a_hungry_dragon_raids_a_train_it_crosses() {
    use rtsim_industry::engine::Engine;
    use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
    use rtsim_industry::obs::{Carriage, StationDecl, TrainDeparture};
    for (fullness, raids) in [(40, 1), (100, 0)] {
        let (mut w, mut d) = world(19);
        let mut tun = rtsim_industry::tunables::Tunables::default();
        tun.threat.carry_per_attacker[ThreatClass::Flying.index()] = 32;
        let mut e = Engine::new(19, tun, Tick::ZERO);
        let (beef, gold) = (Good(2), Good(3));
        e.define_good(beef, GoodInfo::item("minecraft:beef", 64));
        e.define_good(gold, GoodInfo::item("minecraft:gold_ingot", 64));
        let station = |id: u64, x: i32| {
            IndObs::Station(StationDecl {
                station: StationId(id),
                district: None,
                dim: 0,
                pos: BlockPos::new(x, 64, 6_000),
                name: id as u32,
            })
        };
        e.ingest(&Timed {
            at: Tick(1),
            obs: station(1, -2_000),
        });
        e.ingest(&Timed {
            at: Tick(1),
            obs: station(2, 2_000),
        });
        e.ingest(&Timed {
            at: Tick(1_000),
            obs: IndObs::TrainDeparted(TrainDeparture {
                train: TrainId(7),
                station: StationId(1),
                district: DistrictId(0),
                destination: Some(StationId(2)),
                eta_ticks: 20_000,
                carriages: vec![Carriage {
                    index: 0,
                    item_slots: 27,
                    fluid_capacity: 0,
                    goods: GoodVec::from_pairs([(beef, 640), (gold, 64)]),
                }],
            }),
        });
        let a = d.add_lair(
            &mut w,
            1,
            "A",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 0, 3_000),
            100,
            Tick(0),
        );
        let b = d.add_lair(
            &mut w,
            2,
            "B",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 0, 9_000),
            100,
            Tick(0),
        );
        let x = d.add_dragon(&mut w, adult("x", a, fullness, Gender::Female), Tick(0));
        let mut index = DistrictIndex::default();
        let t0 = Tick(1_500);
        {
            let mut link = IndustryLink {
                engine: &mut e,
                index: &mut index,
            };
            d.run_until(&mut w, &mut link, t0);
        }
        d.relocate(&mut w, x, b, t0);
        let mut link = IndustryLink {
            engine: &mut e,
            index: &mut index,
        };
        d.run_until(&mut w, &mut link, Tick(30_000));
        assert_eq!(d.stats.train_raids, raids, "fullness {fullness}");
        if raids > 0 {
            let stolen = &e.books.stolen;
            // I23's per-good cap: a tenth of the carriage, whatever the carry.
            assert!(stolen.get(beef) > 0 && stolen.get(beef) <= 64);
            assert!(stolen.get(gold) <= 6);
            d.check(&w, Some(stolen)).unwrap();
            assert!(d.dragon(x).unwrap().fullness_at(&d.tun, Tick(30_000)) > 0);
        } else {
            assert!(e.books.stolen.is_empty());
        }
    }
}
