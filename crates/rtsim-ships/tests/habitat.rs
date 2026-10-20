// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sea lanes on the world model (rtsim-space) instead of the all-water mask (U11): the coast's
//! ports linked over water the habitat sampler reported, never across land or unknown ground.

use rtsim_travel::Medium;
use rtsim_ships::synth::{coast, coastal_on};
use rtsim_ships::tunables::ShipTunables;
use rtsim_ships::water::{HabitatWater, WaterMask};
use rtsim_space::synth::{Painter, land, sea};
use rtsim_space::{SampledWorld, WorldHandle};

/// The synthetic coast, as the bridge's sampler would report it: one point per chunk.
fn sampled_coast(rect: (i32, i32, i32, i32)) -> WorldHandle {
    let c = coast();
    let mut w = SampledWorld::new();
    let mut p = Painter::default();
    p.paint(&mut w, 0, rect, &|x, z| {
        if c.water_at(x, z) { sea() } else { land("minecraft:plains") }
    });
    WorldHandle::new(Box::new(w))
}

/// Points every 8 blocks along every sea lane.
fn sea_lane_points(s: &rtsim_ships::Ships) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for ln in s.lanes.iter().filter(|l| l.medium == Medium::Sea) {
        let len = ln.path.length();
        let mut d = 0;
        while d <= len {
            out.push(ln.path.at(d));
            d += 8;
        }
        out.push(ln.path.at(len));
    }
    out
}

#[test]
fn a_sea_lane_never_crosses_land() {
    let world = sampled_coast((-3_584, -1_024, 4_608, 4_096));
    let mask = HabitatWater::new(world.clone());
    let (_, ships, _, _) = coastal_on(5, ShipTunables::default(), &mask);
    let sea_lanes = ships.lanes.iter().filter(|l| l.medium == Medium::Sea).count();
    assert!(sea_lanes >= 4, "the coast's ports are linked: {sea_lanes} sea lanes");
    // The island between Brightwater and Gullhaven bends some lane: not every lane is straight.
    assert!(
        ships
            .lanes
            .iter()
            .any(|l| l.medium == Medium::Sea && l.path.pts.len() > 2),
        "some lane goes round land"
    );
    let pts = sea_lane_points(&ships);
    let w = world.read();
    for (x, z) in pts {
        assert_eq!(
            rtsim_space::synth::water_at(&**w, 0, x, z),
            Some(true),
            "a sea lane at ({x}, {z}) is not over known water"
        );
    }
}

#[test]
fn unknown_water_is_never_sailed() {
    // Only the western half of the coast was sampled: the eastern ports have no lanes, and no
    // lane reaches into the unsampled half.
    let world = sampled_coast((-3_584, -1_024, 640, 4_096));
    let mask = HabitatWater::new(world.clone());
    let (_, ships, _, _) = coastal_on(5, ShipTunables::default(), &mask);
    for (x, _) in sea_lane_points(&ships) {
        assert!(x < 640, "a lane sails unsampled water at x {x}");
    }
    // The mask itself: known sea is water, unknown and land are not.
    assert!(mask.is_water(0, 0, 40));
    assert!(!mask.is_water(0, 0, -20), "land");
    assert!(!mask.is_water(0, 100, 40), "unknown");
}
