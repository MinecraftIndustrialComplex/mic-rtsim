// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships engine end to end on the synthetic coast: closed-form timetables that survive a
//! restart, no work while nobody watches, one "enters view" per crossing, raids and loot that
//! keep every good conserved, patrols that lower hazards, and determinism.

use std::collections::BTreeSet;

use proptest::prelude::*;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
use rtsim_settle::Pos;
use rtsim_ships::boundary::{self, By, Obs};
use rtsim_ships::engine::{PState, Ship};
use rtsim_ships::synth::{self, Coastal, FakeShips};
use rtsim_ships::timetable::Phase;
use rtsim_ships::{Role, ShipTunables, Ships};
use rtsim_travel::Medium;
use rtsim_world::World;

const PLAYER: (u64, u64) = (0xAB, 0xCD);

fn coast(seed: u64) -> Coastal {
    synth::coastal(seed, ShipTunables::default())
}

/// The world and the ships read back from their saves, as a restarted engine would.
fn restart(w: &World, s: &Ships, seed: u64) -> (World, Ships) {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let w2 = World::load(synth::world_config(seed), goods, book, &w.save()).expect("world loads");
    let s2 = Ships::load(ShipTunables::default(), &s.save()).expect("ships load");
    (w2, s2)
}

/// Every timetabled ship's position at `t`.
fn positions(s: &Ships, t: Tick) -> Vec<(u64, i32, i32, i32, u64)> {
    s.ships
        .iter()
        .filter_map(|(id, x)| {
            let sv = x.service?;
            let f = s.services[sv as usize].fix(&s.lanes, &s.docks, x.offset, t);
            Some((Ship::agent(id), f.x, f.y, f.z, f.voyage))
        })
        .collect()
}

#[test]
fn timetables_are_closed_forms_and_a_restart_moves_no_ship() {
    let seed = 3;
    let mut c = coast(seed);
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(2));
    let (w2, s2) = restart(&c.world, &c.ships, seed);
    assert_eq!(
        s2.state_hash(),
        c.ships.state_hash(),
        "the save reads back exactly"
    );
    assert_eq!(s2.save(), c.ships.save(), "and writes the same bytes again");
    assert_eq!(w2.state_hash(), c.world.state_hash());
    // Positions are a function of the clock and the saved offsets: the same for the restored
    // engine at every moment, read in any order.
    for t in [
        2 * TICKS_PER_DAY,
        2 * TICKS_PER_DAY + 777,
        5 * TICKS_PER_DAY + 3,
        40 * TICKS_PER_DAY,
    ] {
        assert_eq!(positions(&c.ships, Tick(t)), positions(&s2, Tick(t)));
    }
    let later = positions(&c.ships, Tick(9 * TICKS_PER_DAY));
    let earlier = positions(&c.ships, Tick(3 * TICKS_PER_DAY));
    assert_eq!(later, positions(&c.ships, Tick(9 * TICKS_PER_DAY)));
    assert_ne!(later, earlier, "ships move");
}

#[test]
fn phases_follow_the_timetable_and_cruise_height_is_forced_unloaded() {
    let c = coast(4);
    let s = &c.ships;
    let sky = s
        .services
        .iter()
        .position(|v| v.medium == Medium::Sky)
        .expect("a sky service");
    let sv = &s.services[sky];
    let g = sv.legs[0];
    let off = 0;
    let at = |u: u64| sv.fix(&s.lanes, &s.docks, off, Tick(u));
    let start = g.start;
    assert_eq!(at(start).phase, Phase::Dock);
    let dock_y = s.docks[g.from_dock as usize].y;
    assert_eq!(at(start).y, dock_y, "docked at the dock's height");
    let dep = at(start + g.dock + g.depart / 2);
    assert_eq!(dep.phase, Phase::Depart);
    assert_eq!(dep.y, sv.cruise_y, "unloaded: the forced cruise height");
    assert!(dep.guide_y > dock_y.min(sv.cruise_y) && dep.guide_y < dock_y.max(sv.cruise_y));
    assert_eq!(at(start + g.dock + g.depart + 1).phase, Phase::Cruise);
    let app = at(start + g.dock + g.depart + g.cruise + 1);
    assert_eq!(app.phase, Phase::Approach);
    assert_eq!(app.y, sv.cruise_y);
    // The hop's arrival is the next hop's dock stay.
    let next = at(start + g.total());
    assert_eq!(next.leg, 1 % sv.legs.len());
    assert_eq!(next.phase, Phase::Dock);
    // Two services of a loop cruise at different heights.
    let heights: BTreeSet<i32> = s
        .services
        .iter()
        .filter(|v| v.medium == Medium::Sky)
        .map(|v| v.cruise_y)
        .collect();
    assert!(heights.len() >= 2, "{heights:?}");
}

#[test]
fn nobody_watching_costs_no_ship_events() {
    let mut c = coast(5);
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(20));
    let st = c.ships.stats;
    assert_eq!(st.voyages_booked, 0, "no voyage is booked with nobody near");
    assert_eq!(st.materialized, 0);
    assert_eq!(st.promotions, 0);
    // Every event was a pirate's: four a hunt at most (decide, reach, bored, home), plus the
    // first decisions; or a navy's look for a haven to assault (§2.4.6a rule 3), one per navy
    // every 24 days; or the checks of the ports their hunts warmed (§2.4.6a rule 7, PC25), and
    // one relaunch for each ship a closure laid up, once its port reopened (PC87).
    let pirates = c.ships.havens.iter().map(|h| h.ships.len()).sum::<usize>() as u64;
    let looks = c.ships.navy_counts().looked;
    assert!(st.hunts > 0);
    assert!(
        st.events
            <= 4 * (st.hunts + st.fruitless)
                + pirates
                + looks
                + st.port_checks
                + st.ships_laid_up_by_closure,
        "{st:?} with {pirates} pirates and {looks} navy looks"
    );
    for (k, _) in c.ships.pending_by_kind() {
        assert!(
            [
                "decide",
                "reach_lane",
                "bored",
                "home",
                "refit",
                "assault",
                "dock_check"
            ]
            .contains(&k)
                || (k == "relaunch" && st.reopenings > 0),
            "a pending {k} event with nobody watching"
        );
    }
    c.ships.check(&c.world).expect("conserved");
    // Merchant ships still trade: the lanes carry flows.
    let lanes: i64 = c
        .world
        .edges
        .iter()
        .filter(|e| e.medium != Medium::Road)
        .map(|e| {
            e.dirs[0].volume_slot_milli(&c.world.goods)
                + e.dirs[1].volume_slot_milli(&c.world.goods)
        })
        .sum();
    assert!(lanes > 20, "lanes carry trade: {lanes}");
}

/// A straight sea lane's middle, for a player to stand on.
fn straight_lane_mid(s: &Ships) -> (u32, (i32, i32)) {
    let l = (0..s.lanes.len() as u32)
        .find(|&l| {
            let ln = &s.lanes[l as usize];
            ln.medium == Medium::Sea && ln.path.pts.len() == 2 && ln.path.length() > 1_000
        })
        .expect("a straight sea lane");
    let ln = &s.lanes[l as usize];
    (l, ln.path.at(ln.path.length() / 2))
}

#[test]
fn a_ship_crossing_a_players_view_enters_it_once() {
    let mut c = coast(6);
    let (lane, mid) = straight_lane_mid(&c.ships);
    let (t0, t1) = (Tick::from_days(1), Tick::from_days(3));
    c.ships.run_until(&mut c.world, &mut NoInfrastructure, t0);
    let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
    fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
    let pos = Pos::new(0, mid.0, mid.1);
    // The player reports its position every 20 ticks, standing still: the area is kept or
    // rebooked by the window, and a crossing still enters view once.
    let mut t = t0;
    while t < t1 {
        c.ships.observe_player(&mut c.world, PLAYER, pos, t);
        synth::drive(
            &mut c.world,
            &mut c.ships,
            &mut fake,
            &mut NoInfrastructure,
            &c.mask,
            t + 20,
            20,
        );
        t += 20;
    }
    // Ground truth from the closed form: the (ship, voyage) pairs sailing this lane that come
    // well inside the view, and those that come anywhere near it.
    let s = &c.ships;
    let reach = f64::from(s.tun.view_radius + s.tun.body_radius);
    let (mut sure, mut maybe) = (BTreeSet::new(), BTreeSet::new());
    let _ = lane;
    for sv in 0..s.services.len() as u32 {
        for id in s.service_ships(sv) {
            let x = s.ship(id).unwrap();
            let mut u = t0.0;
            while u <= t1.0 {
                let f = s.services[sv as usize].fix(&s.lanes, &s.docks, x.offset, Tick(u));
                let d = (f64::from(f.x - mid.0).powi(2) + f64::from(f.z - mid.1).powi(2)).sqrt();
                if d < reach - 24.0 {
                    sure.insert((id, f.voyage));
                }
                if d < reach + 24.0 {
                    maybe.insert((id, f.voyage));
                }
                u += 10;
            }
        }
    }
    assert!(!sure.is_empty(), "ships pass the player in two days");
    let seen = s.stats.seen;
    assert!(
        seen as usize >= sure.len() && seen as usize <= maybe.len(),
        "{seen} entered view; between {} and {} should",
        sure.len(),
        maybe.len()
    );
    assert_eq!(s.stats.materialized, fake.materialized);
    assert_eq!(
        fake.materialized,
        fake.dematerialized + fake.bodies.len() as u64
    );
    assert!(s.stats.voyages_booked > 0 && s.stats.voyages_booked <= 4 * maybe.len() as u64 + 64);
    c.ships.check(&c.world).expect("conserved");
}

/// The player watches a hunted lane: pirates attack merchants in view, the fake plugin plays
/// the fights, loot sails to the haven, and every good stays conserved, checked every hour.
#[test]
fn raids_in_view_send_loot_to_the_haven_and_conserve_every_good() {
    let seed = 11;
    let mut c = coast(seed);
    c.ships.log_enabled = true;
    let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
    // Make the pirates bold and the raids certain enough to see in a short run.
    c.ships.tun.share_per_strength_milli = 60;
    // At that hazard (the 400 per mille cap) traders who price it leave the lane at once
    // (overnight choice U9; `a_hunted_lane_is_priced_as_an_expected_loss`), and no merchant in
    // view would carry anything to loot. This test is about raids in view, so its traders keep
    // the rule that ignores hazard.
    c.world.cfg.price_lane_hazard = false;
    let mut raided = 0;
    let mut t = Tick::ZERO;
    let end = Tick::from_days(16);
    while t < end {
        // Follow the first hunted lane the pirates pick, standing at its middle.
        let hunted = c.ships.hunted_lanes().first().copied();
        if let Some(l) = hunted {
            let ln = &c.ships.lanes[l as usize];
            let mid = ln.path.at(ln.path.length() / 2);
            fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
            c.ships
                .observe_player(&mut c.world, PLAYER, Pos::new(0, mid.0, mid.1), t);
        }
        synth::drive(
            &mut c.world,
            &mut c.ships,
            &mut fake,
            &mut NoInfrastructure,
            &c.mask,
            t + 1_000,
            50,
        );
        t += 1_000;
        c.ships
            .check(&c.world)
            .unwrap_or_else(|e| panic!("at {t:?}: {e}"));
        raided = c.ships.stats.raids_observed + c.ships.stats.raids_abstract;
    }
    let st = c.ships.stats;
    assert!(st.seen > 0, "{st:?}");
    assert!(raided > 0, "a raid happened: {st:?}");
    // What pirates took is landed at the haven's market or still in their holds.
    let looted: i128 = c.ships.ledger.looted_by_pirates.values().sum();
    let landed: i128 = c.ships.ledger.landed_at_havens.values().sum();
    let aboard: i128 = c
        .ships
        .havens
        .iter()
        .flat_map(|h| h.ships.iter())
        .filter_map(|&p| c.ships.ship(p))
        .map(|x| x.cargo.values().map(|&a| i128::from(a)).sum::<i128>())
        .sum();
    let sunk: i128 = c.ships.ledger.sunk.values().sum();
    assert!(looted > 0, "pirates took cargo: {st:?}");
    assert!(
        landed + aboard + sunk >= looted - 1,
        "{landed} + {aboard} + {sunk} vs {looted}"
    );
    // The flow's raided share reached the haven's market too.
    let haven = c.ships.havens[c.haven as usize].node;
    let m = c.world.market(haven);
    let book_in: i64 = m
        .lines
        .iter()
        .map(|l| l.book.arrived + l.book.lump_in)
        .sum();
    assert!(book_in > 0, "the haven's market received stolen goods");
}

#[test]
fn patrols_and_escorts_lower_a_lanes_hazard() {
    let mut c = coast(12);
    let navy = c.navy.expect("a navy");
    let ps = c.ships.navies[navy as usize].service;
    let patrolled: BTreeSet<u32> = c.ships.services[ps as usize]
        .legs
        .iter()
        .map(|g| g.hop.lane)
        .collect();
    assert!(!patrolled.is_empty());
    for &l in &patrolled {
        assert!(
            c.ships.threat(l).unwrap().patrol_milli > 0,
            "lane {l} has a patrol"
        );
    }
    // Put a pirate on a patrolled lane by hand and compare.
    let l = *patrolled.iter().next().unwrap();
    let p = c.ships.havens[c.haven as usize]
        .ships
        .iter()
        .copied()
        .find(|&p| c.ships.ship(p).unwrap().medium == Medium::Sea)
        .unwrap();
    c.ships.threats.get_mut(u64::from(l)).unwrap().add_hunter(p);
    let with = c.ships.lane_share(l);
    let without = c.ships.lane_share_unprotected(l);
    assert!(with < without, "patrolled {with} < open {without}");
    // An escort lowers it further.
    let before = c.ships.lane_share(l);
    c.ships.set_escort(&mut c.world, l, 42, 20.0, Tick::ZERO);
    assert!(c.ships.lane_share(l) < before || before == 0);
    c.ships
        .threats
        .get_mut(u64::from(l))
        .unwrap()
        .remove_hunter(p);
}

#[test]
fn the_same_inputs_give_the_same_world_and_steps_compose() {
    let seed = 21;
    let run = |split: &[u64]| {
        let mut c = coast(seed);
        for &d in split {
            c.ships
                .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(d));
        }
        (c.ships.state_hash(), c.world.state_hash())
    };
    let a = run(&[12]);
    assert_eq!(a, run(&[12]), "deterministic");
    assert_eq!(
        a,
        run(&[1, 5, 5, 9, 12]),
        "running to 12 in pieces is running to 12"
    );
}

/// A restored engine continues exactly as one that never stopped, with a player watching and the
/// fake plugin answering (its state is outside the engine and carries on).
#[test]
fn a_restored_engine_matches_one_that_never_stopped() {
    let seed = 31;
    let script = |c: &mut Coastal, fake: &mut FakeShips, from: Tick, to: Tick| {
        let (_, mid) = straight_lane_mid(&c.ships);
        let mut t = from;
        while t < to {
            fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
            c.ships
                .observe_player(&mut c.world, PLAYER, Pos::new(0, mid.0, mid.1), t);
            synth::drive(
                &mut c.world,
                &mut c.ships,
                fake,
                &mut NoInfrastructure,
                &c.mask,
                t + 500,
                50,
            );
            t += 500;
        }
    };
    let mid_t = Tick::from_days(2);
    let end = Tick::from_days(4);
    let mut a = coast(seed);
    let mut fa = FakeShips::new(160.0);
    script(&mut a, &mut fa, Tick::ZERO, mid_t);
    let (w2, s2) = restart(&a.world, &a.ships, seed);
    let mut b = Coastal {
        world: w2,
        ships: s2,
        mask: synth::coast(),
        haven: a.haven,
        navy: a.navy,
    };
    let mut fb = FakeShips::new(160.0);
    fb.bodies = fa.bodies.clone();
    fb.seq = fa.seq;
    script(&mut a, &mut fa, mid_t, end);
    script(&mut b, &mut fb, mid_t, end);
    assert_eq!(a.ships.state_hash(), b.ships.state_hash());
    assert_eq!(a.world.state_hash(), b.world.state_hash());
    assert!(a.ships.stats.seen > 0);
}

#[test]
fn a_truncated_or_foreign_save_is_an_error() {
    let c = coast(1);
    let b = c.ships.save();
    for cut in [0, 4, b.len() / 2, b.len() - 1] {
        assert!(Ships::load(ShipTunables::default(), &b[..cut]).is_err());
    }
    assert!(Ships::load(ShipTunables::default(), b"nonsense").is_err());
}

/// A player in view of a straight sea lane sinks the first merchant that comes into it: the ship
/// and the time of the sinking.
fn sink_a_merchant(c: &mut Coastal) -> (rtsim_ships::ShipId, Tick) {
    let mut fake = FakeShips::new(160.0);
    let (_, mid) = straight_lane_mid(&c.ships);
    fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
    let mut t = Tick::ZERO;
    let mut sunk: Option<rtsim_ships::ShipId> = None;
    while t < Tick::from_days(3) && sunk.is_none() {
        c.ships
            .observe_player(&mut c.world, PLAYER, Pos::new(0, mid.0, mid.1), t);
        synth::drive(
            &mut c.world,
            &mut c.ships,
            &mut fake,
            &mut NoInfrastructure,
            &c.mask,
            t + 100,
            50,
        );
        t += 100;
        let target = fake
            .bodies
            .iter()
            .find(|(_, b)| b.role == rtsim_ships::boundary::Role::Merchant)
            .map(|(a, _)| *a);
        if let Some(a) = target {
            let o = fake.player_sinks(a, PLAYER).unwrap();
            c.ships.ingest(&mut c.world, &o, &c.mask, t);
            sunk = Ship::id_of_agent(a);
        }
    }
    (sunk.expect("a merchant came into view and was sunk"), t)
}

#[test]
fn lost_ships_come_back_in_their_own_slot() {
    let mut c = coast(41);
    let (s, t) = sink_a_merchant(&mut c);
    let x = c.ships.ship(s).unwrap();
    let (offset, slot, service) = (x.offset, x.slot, x.service);
    assert!(!x.is_active());
    c.ships.check(&c.world).expect("conserved after a sinking");
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, t + 6 * TICKS_PER_DAY);
    let x = c.ships.ship(s).unwrap();
    assert!(x.is_active(), "relaunched");
    assert_eq!(
        (x.offset, x.slot, x.service),
        (offset, slot, service),
        "in its own slot"
    );
    assert_eq!(c.ships.stats.respawns, 1);
}

/// A pirate crew the plugin reports killed to the last man loses the ship, as an abstract fight
/// that kills the last man does (`after_fight`), and the haven rebuilds it once its pool can man
/// one. Before, the ship stayed active with no crew: never lost, so never rebuilt, and with no
/// strength no lane was ever worth hunting again. `sim ships`' pirates stopped hunting for good
/// once the fights they lost in view had whittled their crews away.
#[test]
fn a_pirate_crew_killed_in_view_loses_the_ship_and_its_haven_rebuilds_it() {
    let mut c = coast(7);
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(1));
    let haven = c.haven as usize;
    let (fought, wounded) = (
        c.ships.havens[haven].ships[0],
        c.ships.havens[haven].ships[1],
    );
    let merchant = c
        .ships
        .ships
        .iter()
        .find(|(_, x)| matches!(x.role, Role::Merchant))
        .map(|(id, _)| id)
        .expect("a merchant");
    let at = c.ships.now();
    // A fight the plugin played, lost by the pirates to the last man.
    let crew = c.ships.ship(fought).unwrap().crew;
    let fight = Obs::Fight(boundary::Fight {
        attacker: Ship::agent(fought),
        defender: Ship::agent(merchant),
        outcome: 2,
        attacker_crew: (crew, 0),
        defender_crew: (4, 3),
        attacker_hull_milli: 800,
        defender_hull_milli: 900,
    });
    c.ships.ingest(&mut c.world, &fight, &c.mask, at);
    // Wounds that kill the other pirate ship's last man.
    let x = c.ships.ship(wounded).unwrap();
    let damaged = Obs::Damaged(boundary::Damaged {
        agent: Ship::agent(wounded),
        epoch: x.epoch,
        by: By::Other,
        hull_milli: 700,
        crew_lost: x.crew,
        player: (0, 0),
        ship: 0,
    });
    c.ships.ingest(&mut c.world, &damaged, &c.mask, at);
    for p in [fought, wounded] {
        let x = c.ships.ship(p).unwrap();
        assert!(
            !x.is_active(),
            "a pirate ship with no crew is lost: {:?}",
            x.status
        );
    }
    assert_eq!(c.ships.stats.sinkings, 2);
    assert_eq!(c.ships.pending_by_kind().get("refit"), Some(&2));
    c.ships.check(&c.world).expect("conserved after the losses");
    // With a full pool ashore, the haven mans both again and they hunt: no player sank them, so
    // the refit waits only its base of 4 days.
    c.ships.havens[haven].pool_milli = c.ships.tun.crew_pool_cap * 1000;
    c.ships.havens[haven].pool_at = at;
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, at + 12 * TICKS_PER_DAY);
    for p in [fought, wounded] {
        let x = c.ships.ship(p).unwrap();
        assert!(x.is_active() && x.crew > 0, "rebuilt: {x:?}");
    }
    assert_eq!(c.ships.stats.rebuilds, 2);
    assert!(
        c.ships.stats.hunts > 0,
        "the rebuilt pirates hunt: {:?}",
        c.ships.stats
    );
}

#[test]
fn pirates_get_bored_and_go_home() {
    let mut c = coast(51);
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(25));
    let st = c.ships.stats;
    assert!(st.bored >= 2, "{st:?}");
    // Each pirate is somewhere in its cycle, never stuck.
    for h in &c.ships.havens {
        for &p in &h.ships {
            let x = c.ships.ship(p).unwrap();
            assert!(matches!(x.role, Role::Pirate { .. }));
            assert!(x.pirate.is_some());
            let st = x.pirate.as_ref().unwrap().state;
            match st {
                PState::Hunting { until, .. } => assert!(until > c.ships.now()),
                PState::Resting { until } => assert!(until >= c.ships.now()),
                PState::Outbound { arrive, .. } | PState::Homebound { arrive } => {
                    assert!(arrive >= c.ships.now())
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Saved at a random moment of a random watched script, restored, and run on: the same
    /// hashes as the engine that never stopped.
    #[test]
    fn restored_at_any_moment_continues_exactly(seed in 0u64..1_000, cut in 1u64..60, watch in any::<bool>()) {
        let cut = Tick(cut * 1_000);
        let end = Tick(80_000);
        let go = |c: &mut Coastal, fake: &mut FakeShips, from: Tick, to: Tick| {
            let (_, mid) = straight_lane_mid(&c.ships);
            let mut t = from;
            while t < to {
                let next = (t + 1_000).min(to);
                if watch {
                    fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
                    c.ships.observe_player(&mut c.world, PLAYER, Pos::new(0, mid.0, mid.1), t);
                }
                synth::drive(&mut c.world, &mut c.ships, fake, &mut NoInfrastructure, &c.mask, next, 100);
                t = next;
            }
        };
        let mut a = coast(seed);
        let mut fa = FakeShips::new(160.0);
        go(&mut a, &mut fa, Tick::ZERO, cut);
        let (w2, s2) = restart(&a.world, &a.ships, seed);
        let mut b = Coastal { world: w2, ships: s2, mask: synth::coast(), haven: a.haven, navy: a.navy };
        let mut fb = FakeShips::new(160.0);
        fb.bodies = fa.bodies.clone();
        fb.seq = fa.seq;
        go(&mut a, &mut fa, cut, end);
        go(&mut b, &mut fb, cut, end);
        prop_assert_eq!(a.ships.state_hash(), b.ships.state_hash());
        prop_assert_eq!(a.world.state_hash(), b.world.state_hash());
        prop_assert!(a.ships.check(&a.world).is_ok());
    }
}

/// Infrastructure with one unwatched coastal district everywhere: every raid takes ten iron
/// ingots and five of a good the world does not know.
struct Coastline {
    raids: u32,
}

impl rtsim_ecology::infra::Infrastructure for Coastline {
    fn districts_near(
        &mut self,
        frame: u16,
        x: i64,
        z: i64,
        _r: i64,
    ) -> Vec<rtsim_ecology::infra::DistrictSite> {
        vec![rtsim_ecology::infra::DistrictSite {
            id: rtsim_industry::ids::DistrictId(77),
            pos: Pos::new(frame, x as i32, z as i32),
            watched: false,
            loaded: false,
        }]
    }
    fn raid(
        &mut self,
        t: &rtsim_industry::threat::Threat,
    ) -> Option<rtsim_ecology::infra::RaidOutcome> {
        assert_eq!(t.source, rtsim_industry::threat::ThreatSource::Bandit);
        self.raids += 1;
        Some(rtsim_ecology::infra::RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: true,
            stolen: rtsim_industry::ids::GoodVec::from_pairs([
                (rtsim_industry::ids::Good(1), 10),
                (rtsim_industry::ids::Good(2), 5),
            ]),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }
    fn good_name(&self, g: rtsim_industry::ids::Good) -> Option<String> {
        Some(
            if g.0 == 1 {
                "iron_ingot"
            } else {
                "copper_ingot"
            }
            .into(),
        )
    }
}

#[test]
fn pirates_raid_coastal_districts_through_the_threat_interface() {
    let t = ShipTunables {
        coast_raid_milli: 2_000,
        ..ShipTunables::default()
    };
    let mut c = synth::coastal(61, t);
    let mut infra = Coastline { raids: 0 };
    c.ships
        .run_until(&mut c.world, &mut infra, Tick::from_days(12));
    assert!(infra.raids > 0);
    assert_eq!(u64::from(infra.raids), c.ships.stats.coast_raids);
    let iron = c.world.goods.find("iron_ingot").unwrap();
    let from = c.ships.ledger.from_districts[&iron];
    assert_eq!(from, i128::from(infra.raids) * 10_000);
    assert_eq!(
        c.ships.ledger.districts_unmapped["copper_ingot"],
        i64::from(infra.raids) * 5
    );
    // What came home is in the haven's market, booked as players' goods coming in.
    let landed = c
        .ships
        .ledger
        .districts_landed
        .get(&iron)
        .copied()
        .unwrap_or(0);
    let aboard: i128 = c
        .ships
        .havens
        .iter()
        .flat_map(|h| h.ships.iter())
        .filter_map(|&p| c.ships.ship(p)?.pirate.as_ref())
        .map(|p| i128::from(p.district_loot.get(&iron).copied().unwrap_or(0)))
        .sum();
    assert_eq!(landed + aboard, from);
    assert!(landed > 0, "a raid's loot reached the haven");
    c.ships.check(&c.world).expect("conserved");
}

#[test]
fn a_replayed_loot_applies_once() {
    let mut c = coast(71);
    let mut fake = FakeShips::new(160.0);
    let (_, mid) = straight_lane_mid(&c.ships);
    fake.set_player(PLAYER, f64::from(mid.0), f64::from(mid.1));
    let mut t = Tick::ZERO;
    let mut done = false;
    while t < Tick::from_days(6) && !done {
        c.ships
            .observe_player(&mut c.world, PLAYER, Pos::new(0, mid.0, mid.1), t);
        synth::drive(
            &mut c.world,
            &mut c.ships,
            &mut fake,
            &mut NoInfrastructure,
            &c.mask,
            t + 100,
            50,
        );
        t += 100;
        let target = fake
            .bodies
            .iter()
            .find(|(_, b)| b.role == rtsim_ships::boundary::Role::Merchant && !b.cargo.is_empty())
            .map(|(a, _)| *a);
        if let Some(a) = target
            && let Some(o) = fake.player_loots(a, PLAYER, 1.0)
        {
            c.ships.ingest(&mut c.world, &o, &c.mask, t);
            let once: i128 = c.ships.ledger.taken_by_players.values().sum();
            assert!(once > 0);
            c.ships.ingest(&mut c.world, &o, &c.mask, t);
            let twice: i128 = c.ships.ledger.taken_by_players.values().sum();
            assert_eq!(once, twice, "the same op id applies once");
            c.ships.check(&c.world).expect("conserved");
            done = true;
        }
    }
    assert!(done, "a merchant with cargo came into view");
}

/// Goods allocated to a lane now, both directions, the raided share included (thousandths a
/// quantum): what its origins' traders choose to send on it.
fn allocated(w: &World, edge: u32) -> i64 {
    w.edges[edge as usize]
        .dirs
        .iter()
        .map(|d| {
            d.goods.values().map(|p| p.current_rate()).sum::<i64>()
                + d.raid.values().map(|p| p.current_rate()).sum::<i64>()
        })
        .sum()
}

/// Overnight choice U9: a lane that pirates hunt is priced as an expected loss in the trade
/// allocation (its margin falls by the raided share of what the destination pays), so traders
/// send less on it than traders who ignore the hazard, and every good stays conserved.
#[test]
fn a_hunted_lane_is_priced_as_an_expected_loss() {
    let run = |priced: bool| -> (i64, i64) {
        let mut c = coast(5);
        c.world.cfg.price_lane_hazard = priced;
        // Only the world runs: no pirate sets a hazard but the one below.
        let warm = Tick::from_days(4);
        c.world.run_until(warm);
        let lane = (0..c.ships.lanes.len())
            .max_by_key(|&l| {
                (
                    allocated(&c.world, c.ships.lanes[l].edge),
                    std::cmp::Reverse(l),
                )
            })
            .expect("a lane");
        let edge = c.ships.lanes[lane].edge;
        let before = allocated(&c.world, edge);
        let haven = c.ships.havens[c.haven as usize].node;
        for dir in 0..2 {
            assert!(c.world.set_lane_hazard(edge, dir, haven, 300, 4, warm));
        }
        let end = Tick::from_days(10);
        c.world.run_until(end);
        c.world.check_conserved(end).expect("conserved");
        (before, allocated(&c.world, edge))
    };
    let (before, ignored) = run(false);
    let (before_priced, priced) = run(true);
    eprintln!("U9: {before} before the hazard; after 6 days, {ignored} ignored, {priced} priced");
    assert_eq!(before, before_priced, "the same world until the hazard");
    assert!(before > 0, "the busiest lane carries trade");
    assert!(ignored > 0, "traders who ignore the hazard keep sending");
    assert!(
        priced < ignored,
        "a hunted lane is avoided: {priced} sent priced, {ignored} ignored, {before} before"
    );
}

/// A lane given up by one haven (its share set to 0) passes to another only once nothing raided
/// for the first is still sailing to it, and the hand-over loses no good: tried at every quantum
/// after the first haven leaves, the world refuses while a raid is in transit, and its books are
/// conserved once it accepts. (Found by S8b's `unattended_havens_rise_and_fall_1000_years`:
/// the hand-over dropped the raid arriving at the next boundary.)
#[test]
fn a_lane_passed_to_another_haven_keeps_its_raids_in_transit() {
    let mut c = coast(5);
    // Traders keep sending on the hunted lane, so there are raids in transit to drop.
    c.world.cfg.price_lane_hazard = false;
    let warm = Tick::from_days(4);
    c.world.run_until(warm);
    let lane = (0..c.ships.lanes.len())
        .max_by_key(|&l| {
            (
                allocated(&c.world, c.ships.lanes[l].edge),
                std::cmp::Reverse(l),
            )
        })
        .expect("a lane");
    let edge = c.ships.lanes[lane].edge;
    let first = c.ships.havens[c.haven as usize].node;
    let ends = [
        c.world.edges[edge as usize].dirs[0].from,
        c.world.edges[edge as usize].dirs[1].from,
    ];
    let second = (0..c.world.nodes.len())
        .find(|&i| i != first && !ends.contains(&i))
        .expect("another node");
    for dir in 0..2 {
        assert!(c.world.set_lane_hazard(edge, dir, first, 300, 4, warm));
    }
    let left = Tick::from_days(6);
    c.world.run_until(left);
    for dir in 0..2 {
        assert!(c.world.set_lane_hazard(edge, dir, first, 0, 4, left));
    }
    let q = c.world.cfg.trade.quantum;
    let mut passed = [false; 2];
    let mut refused = 0;
    let mut b = q.index(left);
    while !passed.iter().all(|&p| p) {
        b += 1;
        assert!(b < q.index(left) + 40, "never passed: {passed:?}");
        let t = q.at(b) + 1;
        c.world.run_until(t);
        for dir in 0..2u8 {
            if !passed[usize::from(dir)] {
                passed[usize::from(dir)] = c.world.set_lane_hazard(edge, dir, second, 300, 4, t);
                refused += u32::from(!passed[usize::from(dir)]);
                c.world
                    .check_conserved(t)
                    .expect("conserved at the hand-over");
            }
        }
    }
    assert!(
        refused > 0,
        "raids were in transit when the first haven left"
    );
    let end = Tick::from_days(8);
    c.world.run_until(end);
    c.world.check_conserved(end).expect("conserved");
}

/// Overnight choice U18: a merchant that stays embodied across a docking gets its next voyage's
/// hold (SHIP_CARGO), so the body never keeps goods the flow already landed; its op id comes from
/// what it is (allocator 0x53; the same when the run is derived again; never another command's).
#[test]
fn a_ship_embodied_across_a_docking_gets_its_next_hold() {
    use rtsim_ships::boundary::Cmd;
    let run = || -> Vec<(u64, u32, u64, usize)> {
        let mut c = coast(3);
        let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
        // The player waits at a dock: merchants arrive in view, stay docked, and leave again.
        let (x, z) = (c.ships.docks[0].x, c.ships.docks[0].z);
        fake.set_player(PLAYER, f64::from(x), f64::from(z));
        let mut sets = Vec::new();
        let mut t = Tick::ZERO;
        while t < Tick::from_days(3) {
            if t.0.is_multiple_of(1_000) {
                c.ships
                    .observe_player(&mut c.world, PLAYER, Pos::new(0, x, z), t);
            }
            t += 50;
            c.ships.run_until(&mut c.world, &mut NoInfrastructure, t);
            let cmds = c.ships.take_commands();
            let mut obs = fake.apply(&cmds, t);
            for cmd in &cmds {
                if let Cmd::SetCargo(s) = cmd {
                    sets.push((s.agent, s.epoch, s.op, s.items.len()));
                    let body = fake.bodies.get(&s.agent).expect("sent to an embodied ship");
                    assert_eq!(body.epoch, s.epoch, "to its current body");
                    assert_eq!(body.cargo, s.items, "the body holds what the voyage holds");
                }
            }
            obs.extend(fake.step(t));
            for o in obs {
                c.ships.ingest(&mut c.world, &o, &c.mask, t);
            }
        }
        c.ships.check(&c.world).expect("conserved");
        sets
    };
    let sets = run();
    assert!(
        !sets.is_empty(),
        "a merchant stayed in view across a docking"
    );
    assert!(
        sets.iter().all(|s| s.2 >> 56 == 0x53),
        "the ships' allocator"
    );
    let ops: BTreeSet<u64> = sets.iter().map(|s| s.2).collect();
    assert_eq!(ops.len(), sets.len(), "one op id per command");
    assert_eq!(run(), sets, "derived again, the same commands and op ids");
    eprintln!("U18: {} SHIP_CARGO in 3 days at a dock", sets.len());
}

/// The coastline whose raids find nothing to take.
struct Dry(Coastline);

impl rtsim_ecology::infra::Infrastructure for Dry {
    fn districts_near(
        &mut self,
        frame: u16,
        x: i64,
        z: i64,
        r: i64,
    ) -> Vec<rtsim_ecology::infra::DistrictSite> {
        self.0.districts_near(frame, x, z, r)
    }
    fn raid(
        &mut self,
        t: &rtsim_industry::threat::Threat,
    ) -> Option<rtsim_ecology::infra::RaidOutcome> {
        let mut o = self.0.raid(t)?;
        o.stolen = rtsim_industry::ids::GoodVec::default();
        Some(o)
    }
    fn good_name(&self, g: rtsim_industry::ids::Good) -> Option<String> {
        self.0.good_name(g)
    }
}

/// The tape's events of `kind`.
fn events_of(s: &Ships, kind: rtsim_archive::Kind) -> Vec<rtsim_archive::Event> {
    rtsim_archive::record::rows_on(&s.tape)
        .into_iter()
        .filter_map(|(k, _, r)| match r {
            rtsim_archive::Row::Event(e) if k == kind => Some(e),
            _ => None,
        })
        .collect()
}

/// A ship's archive id (a handle's plain bits, which fit the id's 64 bits).
fn sid(s: rtsim_ships::ShipId) -> u64 {
    s.to_bits()
}

/// The ships' archive rows (VIEWER.md §10.1) on three existing scenarios: the scripted coast
/// (pirates under way and hunting, raids in view and unwatched, a merchant taken, a pirate sunk
/// by the player, both coming back, the haven's take of the lanes), the coastal-district raids,
/// and a merchant sunk by a player.
#[test]
fn the_archive_records_the_ships() {
    use rtsim_archive::{Row, Severity, kinds};

    // The scripted coast, a month: the player's day 7 loot, day 9 sinking and the rest. With
    // S8b's lifecycle off: its toll (§2.4.6a rule 1, PC1) takes 2.9 men from the pool before
    // the sinking, so the slot the player sank waits for a crew until day 30.3, just past the
    // month, and no pirate comes back to be taped. `lifecycle.rs` tapes a refound's ships back.
    let tun = ShipTunables {
        lifecycle: false,
        ..ShipTunables::default()
    };
    let mut c = synth::coastal(7, tun.clone());
    c.ships.tape.set_on(true);
    let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
    synth::Script::default().run(&mut c, &mut fake, Tick::ZERO, Tick::from_days(30));
    // The tape is not state: the same month with it off ends in the same hashes.
    let mut off = synth::coastal(7, tun);
    let mut fake_off = FakeShips::new(f64::from(off.ships.tun.view_radius));
    synth::Script::default().run(&mut off, &mut fake_off, Tick::ZERO, Tick::from_days(30));
    assert!(off.ships.tape.is_empty());
    assert_eq!(c.ships.state_hash(), off.ships.state_hash());
    assert_eq!(c.world.state_hash(), off.world.state_hash());
    for k in rtsim_archive::record::kinds_on(&c.ships.tape) {
        assert_eq!(k.group(), "ships", "{}", k.name());
        assert_eq!(kinds::by_name(k.name()), Some(k));
    }
    let player = c.ships.player(&mut c.world, synth::SCRIPT_PLAYER).to_bits();
    let pirates: BTreeSet<u64> = c
        .ships
        .havens
        .iter()
        .flat_map(|h| h.ships.iter())
        .map(|&p| sid(p))
        .collect();
    let merchants: BTreeSet<u64> = c
        .ships
        .ships
        .iter()
        .filter(|(_, x)| x.role == Role::Merchant)
        .map(|(id, _)| sid(id))
        .collect();
    assert_eq!(c.ships.havens.len(), 1);
    let haven = &c.ships.havens[c.haven as usize];
    let haven_key = c.world.nodes[haven.node].key;
    let mids: Vec<(i32, i32)> = c
        .ships
        .lanes
        .iter()
        .map(|ln| ln.path.at(ln.path.length() / 2))
        .collect();

    // Legs: a pirate's every booking, notable (named movers). The first sortie leaves the
    // haven's dock, and a hunt is a leg that goes nowhere, at a lane's middle, which begins
    // where the sortie's leg ended.
    let legs: Vec<rtsim_archive::Leg> = rtsim_archive::record::rows_on(&c.ships.tape)
        .into_iter()
        .filter_map(|(k, dim, r)| match r {
            Row::Leg(l) if k == kinds::SHIPS_PIRATE_LEG => {
                assert_eq!(dim, 0);
                Some(l)
            }
            _ => None,
        })
        .collect();
    assert!(legs.len() > 20, "{} legs", legs.len());
    assert!(
        legs.iter()
            .all(|l| l.severity == Severity::Notable && pirates.contains(&l.subject.value))
    );
    assert!(
        legs.iter()
            .any(|l| (l.x0, l.z0) == (haven.dock.0, haven.dock.2))
    );
    let stays: Vec<_> = legs
        .iter()
        .filter(|l| (l.x0, l.z0) == (l.x1, l.z1) && l.duration > 0)
        .collect();
    assert!(!stays.is_empty());
    for l in &stays {
        assert!(mids.contains(&(l.x0, l.z0)), "{l:?}");
        assert!(
            legs.iter()
                .any(|p| p.subject == l.subject && p.end() == l.t0 && (p.x1, p.z1) == (l.x0, l.z0)),
            "a hunt begins where the sortie ended: {l:?}"
        );
    }

    // Raids: pirate on merchant. In view the plugin plays the fight, so nothing is known taken
    // (magnitude 0) and it is notable for the watcher; a fight nobody watches knows what was
    // taken, and one that took nothing would be minor.
    let raids = events_of(&c.ships, kinds::SHIPS_RAID);
    assert!(raids.len() >= 6, "{} raids", raids.len());
    for e in &raids {
        assert!(pirates.contains(&e.subject.value), "{e:?}");
        assert!(merchants.contains(&e.object.value), "{e:?}");
        assert!(e.severity == Severity::Notable || e.magnitude == 0, "{e:?}");
        assert!(e.payload || e.severity == Severity::Minor, "{e:?}");
    }
    assert!(raids.iter().any(|e| e.magnitude == 0));
    let taken = raids.iter().find(|e| e.magnitude > 0).expect("a raid took");
    assert_eq!(taken.severity, Severity::Notable);

    // Lost: the merchant that raid took (the same instant and place, by that pirate), and the
    // pirate the player sank on day 9 (by the player's actor, at a lane's middle).
    let lost = events_of(&c.ships, kinds::SHIPS_LOST);
    assert_eq!(lost.len(), 2, "{lost:?}");
    assert!(
        lost.iter()
            .all(|e| e.severity == Severity::Notable && e.payload)
    );
    let merchant = lost
        .iter()
        .find(|e| merchants.contains(&e.subject.value))
        .expect("a merchant lost");
    assert_eq!(
        (merchant.tick, merchant.x, merchant.z, merchant.object),
        (taken.tick, taken.x, taken.z, taken.subject)
    );
    assert_eq!(merchant.subject, taken.object);
    let pirate = lost
        .iter()
        .find(|e| pirates.contains(&e.subject.value))
        .expect("a pirate lost");
    assert_eq!(pirate.object.value, player);
    assert!(pirate.tick >= Tick::from_days(9).0);
    assert!(mids.contains(&(pirate.x, pirate.z)), "{pirate:?}");

    // Back: the pirate's replacement at its haven's dock, no sooner than one refit wait later
    // (the player sank one of the haven's two ships, so its pressure is 1/2 and the wait
    // 4 x 2^(1/2) days) and later still if the pool held no full crew by then (§2.4.6), and the
    // merchant at its home village's dock, no sooner than the respawn delay.
    let back = events_of(&c.ships, kinds::SHIPS_BACK);
    assert_eq!(back.len(), 2, "{back:?}");
    assert!(back.iter().all(|e| e.severity == Severity::Minor));
    let rebuilt = back
        .iter()
        .find(|e| e.subject == pirate.subject)
        .expect("the pirate was rebuilt");
    let day = (pirate.tick / TICKS_PER_DAY) as u32;
    let wait = c
        .ships
        .tun
        .refit
        .delay_ticks(Pressure::from_parts(0.5, day), day);
    assert!(
        rebuilt.tick >= pirate.tick + wait,
        "{rebuilt:?} {pirate:?} {wait}"
    );
    assert_eq!((rebuilt.x, rebuilt.z), (haven.dock.0, haven.dock.2));
    assert_eq!(rebuilt.object.value, haven_key);
    assert_eq!(rebuilt.magnitude, c.ships.tun.sea.pirate_crew as i32);
    let returned = back
        .iter()
        .find(|e| e.subject == merchant.subject)
        .expect("the merchant relaunched");
    assert!(returned.tick >= merchant.tick + c.ships.tun.respawn_ticks);
    let x = c
        .ships
        .ship(rtsim_ships::ShipId::from_bits(returned.subject.value))
        .unwrap();
    assert!(x.is_active());
    let sv = &c.ships.services[x.service.unwrap() as usize];
    let home = c.ships.docks[sv.legs[0].from_dock as usize].node;
    assert_eq!(returned.object.value, c.world.nodes[home].key);
    assert_eq!(
        returned.magnitude,
        c.ships.tun.medium(x.medium).merchant_crew as i32
    );

    // The haven's take of the lanes: minor rows of the haven's key at a lane's middle. The first
    // claims an unclaimed lane (nothing yet), and the hunt's share follows at once.
    let hold = events_of(&c.ships, kinds::SHIPS_HAVEN);
    assert!(hold.len() >= 4, "{} rows", hold.len());
    for e in &hold {
        assert_eq!(e.severity, Severity::Minor);
        assert_eq!(e.subject.value, haven_key);
        assert!(mids.contains(&(e.x, e.z)), "{e:?}");
        assert!((0..=1000).contains(&e.magnitude), "{e:?}");
    }
    assert_eq!((hold[0].object.value, hold[0].magnitude), (0, 0));
    assert_eq!(
        (hold[1].tick, hold[1].x, hold[1].z),
        (hold[0].tick, hold[0].x, hold[0].z)
    );
    assert!(hold[1].magnitude > 0);

    // The coastal-district raids: every raid is a row, on the district the interface named,
    // with what it stole (ten iron ingots and five of a good the world does not know).
    let t = ShipTunables {
        coast_raid_milli: 2_000,
        ..ShipTunables::default()
    };
    let mut c = synth::coastal(61, t);
    c.ships.tape.set_on(true);
    let mut infra = Coastline { raids: 0 };
    c.ships
        .run_until(&mut c.world, &mut infra, Tick::from_days(12));
    let pirates: BTreeSet<u64> = c
        .ships
        .havens
        .iter()
        .flat_map(|h| h.ships.iter())
        .map(|&p| sid(p))
        .collect();
    let raids = events_of(&c.ships, kinds::SHIPS_RAID);
    assert!(infra.raids > 0);
    assert_eq!(raids.len(), infra.raids as usize);
    for e in &raids {
        assert!(pirates.contains(&e.subject.value), "{e:?}");
        assert_eq!((e.object.value, e.magnitude), (77, 15), "{e:?}");
        assert!(e.severity == Severity::Notable && e.payload, "{e:?}");
    }

    // A raid that found nothing is minor, with no payload.
    let t = ShipTunables {
        coast_raid_milli: 2_000,
        ..ShipTunables::default()
    };
    let mut c = synth::coastal(61, t);
    c.ships.tape.set_on(true);
    let mut infra = Dry(Coastline { raids: 0 });
    c.ships
        .run_until(&mut c.world, &mut infra, Tick::from_days(12));
    let raids = events_of(&c.ships, kinds::SHIPS_RAID);
    assert_eq!(raids.len(), infra.0.raids as usize);
    assert!(!raids.is_empty());
    for e in &raids {
        assert_eq!((e.object.value, e.magnitude), (77, 0), "{e:?}");
        assert!(e.severity == Severity::Minor && !e.payload, "{e:?}");
    }

    // A merchant a player sank: lost by the player's actor where the ship was, and back at its
    // home dock after the delay.
    let mut c = coast(41);
    c.ships.tape.set_on(true);
    let (s, t) = sink_a_merchant(&mut c);
    let at = c.ships.position(&c.world, s, t);
    let actor = c.ships.player(&mut c.world, PLAYER).to_bits();
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, t + 6 * TICKS_PER_DAY);
    let lost = events_of(&c.ships, kinds::SHIPS_LOST);
    assert_eq!(lost.len(), 1);
    let e = &lost[0];
    assert_eq!((e.subject.value, e.object.value), (sid(s), actor));
    assert_eq!((e.tick, (e.x, e.z)), (t.0, at));
    assert!(e.severity == Severity::Notable && e.payload);
    let back = events_of(&c.ships, kinds::SHIPS_BACK);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].subject.value, sid(s));
    assert!(back[0].tick >= t.0 + c.ships.tun.respawn_ticks);
    for k in rtsim_archive::record::kinds_on(&c.ships.tape) {
        assert_eq!(k.group(), "ships", "{}", k.name());
    }
}
