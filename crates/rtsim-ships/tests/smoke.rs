// SPDX-License-Identifier: AGPL-3.0-or-later
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_ships::{ShipTunables, synth};

#[test]
fn the_coast_builds_and_runs_a_quiet_week() {
    let mut c = synth::coastal(7, ShipTunables::default());
    let s = &c.ships;
    eprintln!(
        "docks {} lanes {} services {} ships {} lints {:?} over_berths {:?}",
        s.docks.len(),
        s.lanes.len(),
        s.services.len(),
        s.ships.len(),
        s.lints,
        s.over_berths
    );
    for (i, sv) in s.services.iter().enumerate() {
        eprintln!(
            "service {i}: {:?} {:?} period {} ships {} stops {:?}",
            sv.kind,
            sv.medium,
            sv.period,
            sv.ships,
            sv.legs.iter().map(|g| g.from_dock).collect::<Vec<_>>()
        );
    }
    for (i, l) in s.lanes.iter().enumerate() {
        eprintln!(
            "lane {i}: {:?} {} -> {} len {} pts {} edge {}",
            l.medium,
            l.a,
            l.b,
            l.path.length(),
            l.path.pts.len(),
            l.edge
        );
    }
    c.ships.log_enabled = true;
    c.ships
        .run_until(&mut c.world, &mut NoInfrastructure, Tick::from_days(30));
    for l in &c.ships.log {
        eprintln!("{:.2} {} {:?} {}", l.day, l.kind, l.ship, l.what);
    }
    eprintln!("{:?}", c.ships.stats);
    eprintln!("{:?}", c.ships.pending_by_kind());
    c.ships.check(&c.world).expect("conserved");
    for (e, edge) in c.world.edges.iter().enumerate() {
        {
            let v0 = edge.dirs[0].volume_slot_milli(&c.world.goods);
            let v1 = edge.dirs[1].volume_slot_milli(&c.world.goods);
            eprintln!("edge {e} {}-{} {:?} cap {} vol {v0} {v1} hazard {:?} {:?}", edge.a, edge.b, edge.medium, edge.cap_slot_milli, c.world.lane_hazard(e as u32, 0), c.world.lane_hazard(e as u32, 1));
        }
    }
}
