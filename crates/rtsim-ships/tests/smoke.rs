// SPDX-License-Identifier: AGPL-3.0-or-later
//! The coast builds a network of both media, its lanes are world edges with hold caps, and a
//! month with nobody watching stays conserved with the lanes carrying trade.

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_ships::{ShipTunables, synth};
use rtsim_travel::Medium;

#[test]
fn the_coast_builds_and_runs_a_quiet_month() {
    let mut c = synth::coastal(7, ShipTunables::default());
    let s = &c.ships;
    assert!(s.lanes.iter().any(|l| l.medium == Medium::Sea));
    assert!(s.lanes.iter().any(|l| l.medium == Medium::Sky));
    assert!(
        s.lanes.iter().any(|l| l.path.pts.len() > 2),
        "some sea lane bends round land"
    );
    assert!(s.over_berths.is_empty(), "{:?}", s.over_berths);
    for l in &s.lanes {
        let e = &c.world.edges[l.edge as usize];
        assert_eq!(e.medium, l.medium);
        assert!(e.cap_slot_milli > 0);
        let mid = l.path.at(l.path.length() / 2);
        let near = c
            .world
            .roads
            .near(l.frame, i64::from(mid.0), i64::from(mid.1), 64);
        assert!(
            !near.contains(&l.edge),
            "a lane is not a road: no caravans sampled on it"
        );
    }
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(30));
    c.ships.check(&c.world).expect("conserved");
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
    let roads: i64 = c
        .world
        .edges
        .iter()
        .filter(|e| e.medium == Medium::Road)
        .map(|e| {
            e.dirs[0].volume_slot_milli(&c.world.goods)
                + e.dirs[1].volume_slot_milli(&c.world.goods)
        })
        .sum();
    assert!(
        lanes > roads,
        "ships take the coast's trade from slow roads: {lanes} against {roads}"
    );
}
