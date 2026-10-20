// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships engine end to end on the synthetic coast: closed-form timetables that survive a
//! restart, no work while nobody watches, one "enters view" per crossing, raids and loot that
//! keep every good conserved, patrols that lower hazards, and determinism.

use std::collections::BTreeSet;

use proptest::prelude::*;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
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
    // first decisions.
    let pirates = c.ships.havens.iter().map(|h| h.ships.len()).sum::<usize>() as u64;
    assert!(st.hunts > 0);
    assert!(
        st.events <= 4 * (st.hunts + st.fruitless) + pirates,
        "{st:?} with {pirates} pirates"
    );
    for (k, _) in c.ships.pending_by_kind() {
        assert!(
            ["decide", "reach_lane", "bored", "home", "rebuild"].contains(&k),
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
        for &id in s.service_ships(sv) {
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
        let hunted = c
            .ships
            .threats
            .iter()
            .find(|(_, th)| !th.hunters.is_empty())
            .map(|(l, _)| *l);
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
            c.ships.threats[&l].patrol_milli > 0,
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
    c.ships.threats.get_mut(&l).unwrap().hunters.insert(p);
    let with = c.ships.lane_share(l);
    let without = c.ships.lane_share_unprotected(l);
    assert!(with < without, "patrolled {with} < open {without}");
    // An escort lowers it further.
    let before = c.ships.lane_share(l);
    c.ships.set_escort(&mut c.world, l, 42, 20.0, Tick::ZERO);
    assert!(c.ships.lane_share(l) < before || before == 0);
    c.ships.threats.get_mut(&l).unwrap().hunters.remove(&p);
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

#[test]
fn lost_ships_come_back_in_their_own_slot() {
    let mut c = coast(41);
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
    let s = sunk.expect("a merchant came into view and was sunk");
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
