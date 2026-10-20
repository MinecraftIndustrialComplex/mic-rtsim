// SPDX-License-Identifier: AGPL-3.0-or-later
//! One interest area per player across every module (overnight choices U2 to U6): the dragons'
//! and the ships' travellers are booked against the same area (`World::observe_area`), and each
//! crossing found in its cells goes to the module that owns the traveller (`World::route_area`).

use rtsim_ecology::boundary::Cmd as DragonCmd;
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_ships::ShipTunables;
use rtsim_ships::synth::{self, Coastal, FakeShips};
use rtsim_world::World;
use rtsim_world::interest::{PLAYER_OWNER, Viewer};

const PLAYER: (u64, u64) = (0x51, 0x52);
const VIEW: u32 = 160;

fn player_legs(w: &World) -> usize {
    w.legs
        .iter()
        .filter(|(_, r)| r.owner.0 & PLAYER_OWNER != 0)
        .count()
}

/// The coast with a dragon at rest in a lair beside the middle of lane 0, and where a player
/// standing there is.
fn scene() -> (Coastal, Dragons, Pos) {
    let mut c = synth::coastal(11, ShipTunables::default());
    let mut d = Dragons::new(11, DragonTunables::default(), Tick::ZERO);
    let ln = &c.ships.lanes[0];
    let mid = ln.path.at(ln.path.length() / 2);
    let lair = d.add_lair(
        &mut c.world,
        77,
        "beside the lane",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, mid.0 + 40, mid.1 + 40),
        100,
        Tick::ZERO,
    );
    d.add_dragon(
        &mut c.world,
        NewDragon {
            name: "Ember".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(lair),
            fullness: 100,
            health: None,
            seed: None,
            at: None,
        },
        Tick::ZERO,
    );
    (c, d, Pos::new(0, mid.0, mid.1))
}

fn dragon_materializations(d: &mut Dragons) -> usize {
    d.take_commands()
        .iter()
        .filter(|c| matches!(c, DragonCmd::Materialize(_)))
        .count()
}

/// The player stands by the lane and the lair for two days, reported every 20 ticks: one leg in
/// the index is the player's, whatever the modules; the dragon at rest is seen once (no second
/// meeting found it already embodied, R31); the ships on the lane enter view.
#[test]
fn one_area_per_player_serves_the_dragons_and_the_ships() {
    let (mut c, mut d, pos) = scene();
    let mut fake = FakeShips::new(f64::from(VIEW));
    fake.set_player(PLAYER, f64::from(pos.x), f64::from(pos.z));
    let (mut t, end) = (Tick(1_000), Tick(1_000 + 2 * TICKS_PER_DAY));
    let (mut routed, mut mats) = (0, 0);
    while t < end {
        let (p, area) = c.world.observe_area(PLAYER, pos, t, VIEW);
        d.observe_home(&mut c.world, PLAYER, pos, t);
        if let Some(a) = area {
            routed += 1;
            let mut viewers: [&mut dyn Viewer; 2] = [&mut d, &mut c.ships];
            let rest = c.world.route_area(p, a, &mut viewers);
            assert!(
                rest.iter().all(|m| c
                    .world
                    .legs
                    .get(m.a.leg)
                    .is_some_and(|r| r.owner.0 >> 48 == 0)),
                "only caravan rows are left unclaimed"
            );
        }
        assert_eq!(player_legs(&c.world), 1, "one area for the player at {t:?}");
        d.run_until(&mut c.world, &mut NoInfrastructure, t);
        mats += dragon_materializations(&mut d);
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
    assert!(
        routed >= 2,
        "the area was booked and booked again ({routed})"
    );
    assert_eq!(mats, 1, "the dragon at rest materialized once");
    assert_eq!(
        d.stats.seen, 1,
        "and was seen through one meeting: {:?}",
        d.stats
    );
    let s = c.ships.stats;
    assert!(
        s.seen > 0 && s.materialized > 0,
        "the ships entered view: {s:?}"
    );
    assert_eq!(
        s.interest_bookings, routed,
        "every routed area booked the voyages in view"
    );
    c.ships.check(&c.world).expect("conserved");
}

/// A dragon flying over a player, booked while the player's areas stand: with each module
/// booking its own area (the engine before U2: the world's, the dragons' and the ships') the
/// flight finds three areas for one player and books three meetings, the later two finding the
/// dragon already embodied (R31); with the one shared area, one leg and one meeting.
#[test]
fn a_flight_over_a_player_meets_one_area() {
    let run = |shared: bool| -> (usize, u64, u64) {
        let (mut c, mut d, pos) = scene();
        let t = Tick(1_000);
        // Two roosts west and east of the player, outside its view; the dragon flies between.
        let west = d.add_lair(
            &mut c.world,
            78,
            "west",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, pos.x - 400, pos.z - 200),
            100,
            Tick::ZERO,
        );
        let east = d.add_lair(
            &mut c.world,
            79,
            "east",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, pos.x + 600, pos.z - 200),
            100,
            Tick::ZERO,
        );
        let x = d.add_dragon(
            &mut c.world,
            NewDragon {
                name: "Cinder".into(),
                species: Species::Fire,
                gender: Gender::Male,
                variant: 1,
                age_ticks: 110 * TICKS_PER_DAY,
                lair: Some(west),
                fullness: 100,
                health: None,
                seed: None,
                at: None,
            },
            Tick::ZERO,
        );
        d.run_until(&mut c.world, &mut NoInfrastructure, t);
        let _ = dragon_materializations(&mut d);
        // Ember, at rest beside the player, is seen at the first area: count from here.
        let stand = Pos::new(0, pos.x, pos.z - 200);
        if shared {
            let (p, area) = c.world.observe_area(PLAYER, stand, t, VIEW);
            let mut viewers: [&mut dyn Viewer; 2] = [&mut d, &mut c.ships];
            c.world
                .route_area(p, area.expect("a new area"), &mut viewers);
        } else {
            let p = c.world.player_uuid(PLAYER.0, PLAYER.1);
            c.world.set_player_pos(p, stand, t);
            d.observe_player(&mut c.world, PLAYER, stand, t);
            c.ships.observe_player(&mut c.world, PLAYER, stand, t);
        }
        let legs = player_legs(&c.world);
        let before = d.stats;
        d.relocate(&mut c.world, x, east, t);
        d.run_until(&mut c.world, &mut NoInfrastructure, t + 1_900);
        let mats = dragon_materializations(&mut d);
        assert_eq!(mats, 1, "the flight materializes once (shared: {shared})");
        (
            legs,
            d.stats.seen - before.seen,
            d.stats.seen_stale - before.seen_stale,
        )
    };
    assert_eq!(
        run(false),
        (3, 1, 2),
        "own areas: three legs, two meetings too many"
    );
    assert_eq!(
        run(true),
        (1, 1, 0),
        "the shared area: one leg, one meeting"
    );
}

/// A player who leaves takes the area with it; every module hears of it, and a rebooked area
/// after a move is one leg replanned, never a second.
#[test]
fn leaving_and_moving_keep_one_leg() {
    let (mut c, mut d, pos) = scene();
    let t = Tick(1_000);
    let (p, a) = c.world.observe_area(PLAYER, pos, t, VIEW);
    let a = a.expect("booked");
    let (_, kept) = c
        .world
        .observe_area(PLAYER, Pos::new(0, pos.x + 10, pos.z), t + 20, VIEW);
    assert!(kept.is_none(), "a small move keeps the area");
    let (_, moved) = c
        .world
        .observe_area(PLAYER, Pos::new(0, pos.x + 100, pos.z), t + 40, VIEW);
    let moved = moved.expect("a move past a quarter of the radius books it again");
    assert_eq!(moved.leg, a.leg, "the same leg, replanned");
    assert!(
        !c.world.legs.is_live(a),
        "meetings against the old area go stale"
    );
    assert_eq!(c.world.interest_area(p), Some(moved));
    let mut viewers: [&mut dyn Viewer; 2] = [&mut d, &mut c.ships];
    c.world.leave_area(p, &mut viewers);
    assert_eq!(player_legs(&c.world), 0);
    assert_eq!(c.world.interest_area(p), None);
}
