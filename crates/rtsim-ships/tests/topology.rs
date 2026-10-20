// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sea lanes on a Deep Time world (docs/choices-topology.md TP15): docks on either side of the
//! east–west seam, and either side of the north seam near a pole, get the short lane through the
//! seam; on the plane the same docks get the long one.

use rtsim_ships::ShipTunables;
use rtsim_ships::network::{Dock, NetworkLints, candidate_lanes};
use rtsim_ships::water::{AllWater, SynthCoast, WaterMask};
use rtsim_space::topology::{Topology, layout};
use rtsim_travel::Medium;

const B: i64 = layout::GUYOU_16K;

fn dock(node: usize, x: i32, z: i32) -> Dock {
    Dock {
        node,
        medium: Medium::Sea,
        frame: 0,
        x,
        y: 62,
        z,
        berths: 2,
    }
}

fn lanes(
    t: &Topology,
    docks: &[Dock],
    mask: &dyn WaterMask,
) -> Vec<rtsim_ships::network::Candidate> {
    let group: Vec<u32> = (0..docks.len() as u32).collect();
    let mut lints = NetworkLints::default();
    candidate_lanes(docks, &group, &ShipTunables::default(), mask, &mut lints, t)
}

#[test]
fn a_sea_lane_crosses_the_east_west_seam() {
    let b = B as i32;
    let docks = vec![
        dock(0, b - 150, 400),
        dock(1, -b + 150, 400),
        dock(2, b - 300, 1_400),
    ];
    let guyou = layout::guyou(B, true);
    let ls = lanes(&guyou, &docks, &AllWater(32));
    let across = ls
        .iter()
        .find(|c| (c.a, c.b) == (0, 1))
        .expect("the docks either side of the seam are linked");
    assert!(
        across.path.length() < 400,
        "the lane runs through the seam: {} blocks, {:?}",
        across.path.length(),
        across.path.pts
    );
    // Its path is a chart polyline from dock 0 that continues past the edge to dock 1's image,
    // and every point folds onto the world.
    let end = *across.path.pts.last().unwrap();
    assert!(end.0 > b, "{end:?}");
    assert_eq!(guyou.canonical_i32(end.0, end.1), (-b + 150, 400));
    // On the plane the same two docks are 13,000 blocks apart: no lane.
    let flat = lanes(&Topology::flat(), &docks, &AllWater(32));
    assert!(
        flat.iter()
            .all(|c| c.path.length() > 10_000 || (c.a, c.b) != (0, 1))
    );
}

#[test]
fn a_lane_over_the_north_seam_follows_the_water_mask_at_canonical_cells() {
    let h = (B / 2) as i32;
    // Two docks near the north edge, mirrored through the pivot (0, -b/2): through the seam they
    // are a few hundred blocks apart, directly 4,000.
    let docks = vec![dock(0, -2_000, -h + 120), dock(1, 2_000, -h + 120)];
    let guyou = layout::guyou(B, false);
    // Land everywhere south of z = -h + 300, so the long way round is closed to the search.
    let coast = SynthCoast {
        cell: 32,
        shore_z: 0,
        amp: 0,
        wavelength: 1,
        islands: Vec::new(),
        bays: vec![(0, -h, 600), (-2_000, -h, 500), (2_000, -h, 500)],
    };
    let ls = lanes(&guyou, &docks, &coast);
    let c = ls
        .iter()
        .find(|c| (c.a, c.b) == (0, 1))
        .expect("a lane over the north seam");
    assert!(
        c.path.length() < 800,
        "{} blocks: {:?}",
        c.path.length(),
        c.path.pts
    );
    // Its chart points pass beyond the north edge; folded, every one is water.
    assert!(c.path.pts.iter().any(|p| p.1 < -h));
    for p in &c.path.pts {
        let q = guyou.canonical_i32(p.0, p.1);
        assert!(coast.water_at(q.0, q.1), "{p:?} folds to {q:?}, not water");
    }
}
