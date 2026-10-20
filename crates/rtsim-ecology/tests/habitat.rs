// SPDX-License-Identifier: AGPL-3.0-or-later
//! Herds on the world model (rtsim-space): a herd never plans across water or into ground nobody
//! sampled, herds founded per region stand on land, and they stay there while they migrate.

use rtsim_ecology::herds::forage::{CELL_BLOCKS, cell_of};
use rtsim_ecology::herds::habitat::{FoundingTunables, HabitatClimate, RegionPlan, plan_region};
use rtsim_ecology::herds::{HerdSpecies, climate::Climate};
use rtsim_ecology::herds::{HerdTunables, Herds, NewHerd};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_space::synth::{Painter, land, sea};
use rtsim_space::{RegionKey, SampledWorld, WorldHandle};
use rtsim_travel::legs::LegIndex;

/// West pasture (thin forest), a sea strip two forage cells wide, then rich plains east. With
/// `bridge`, a land bridge crosses the strip far to the north.
fn strait(bridge: bool) -> WorldHandle {
    let mut w = SampledWorld::new();
    let mut p = Painter::default();
    p.paint(&mut w, 0, (0, 0, 4096, 4096), &|x, z| {
        if (1536..2048).contains(&x) && !(bridge && (3584..3840).contains(&z)) {
            sea()
        } else if x < 1536 {
            land("minecraft:forest")
        } else {
            land("minecraft:plains")
        }
    });
    WorldHandle::new(Box::new(w))
}

fn in_strait(x: i64, z: i64, bridge: bool) -> bool {
    (1536..2048).contains(&x) && !(bridge && (3584..3840).contains(&z))
}

/// Every point every 8 blocks along the herd's route, from its start to its end.
fn route_samples(h: &Herds) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for (_, x) in h.herds.iter() {
        for w in x.route.points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = f64::from((b.0 - a.0).abs().max((b.1 - a.1).abs()));
            let n = (d / 8.0).ceil().max(1.0) as i32;
            for i in 0..=n {
                let f = f64::from(i) / f64::from(n);
                out.push((
                    (f64::from(a.0) + f * f64::from(b.0 - a.0)).round() as i64,
                    (f64::from(a.1) + f * f64::from(b.1 - a.1)).round() as i64,
                ));
            }
        }
        if x.route.points.len() == 1 {
            let p = x.route.points[0];
            out.push((i64::from(p.0), i64::from(p.1)));
        }
    }
    out
}

fn herds_on(world: &WorldHandle) -> Herds {
    Herds::new(
        41,
        HerdTunables::default(),
        Box::new(HabitatClimate::new(world.clone())),
        Tick::ZERO,
    )
}

fn check_routes(h: &Herds, climate: &HabitatClimate, bridge: bool) -> usize {
    let samples = route_samples(h);
    for &(x, z) in &samples {
        assert!(
            !in_strait(x, z, bridge),
            "a route crosses the sea at ({x}, {z})"
        );
        let c = cell_of(x, z);
        assert!(
            climate.terrain(0, c.0, c.1).is_finite(),
            "a route enters a closed cell at ({x}, {z})"
        );
    }
    samples.len()
}

#[test]
fn a_herd_never_plans_across_water() {
    for bridge in [false, true] {
        let world = strait(bridge);
        let climate = HabitatClimate::new(world.clone());
        let mut h = herds_on(&world);
        let mut legs = LegIndex::new(1_000);
        for (i, z) in [400, 1200, 2000, 2800].into_iter().enumerate() {
            h.add_herd(
                NewHerd {
                    name: format!("west {i}"),
                    species: HerdSpecies::Sheep,
                    frame: 0,
                    pos: (1300, z),
                    count: 3_000,
                    seed: Some(i as u64 + 1),
                },
                Tick::ZERO,
            );
        }
        let mut checked = 0;
        let mut crossed_east = false;
        let mut t = Tick::ZERO;
        let end = Tick::from_days(2 * 96);
        while t < end {
            t = (t + 2 * TICKS_PER_DAY).min(end);
            h.run_until(&mut legs, t);
            h.check(t).expect("invariants");
            checked += check_routes(&h, &climate, bridge);
            for (_, x) in h.herds.iter() {
                let p = x.route.pos(t);
                assert!(!in_strait(p.0, p.1, bridge), "a herd stands in the sea");
                crossed_east |= p.0 >= 2048;
            }
        }
        assert!(checked > 1_000, "routes were sampled: {checked}");
        let moved = h
            .herds
            .iter()
            .filter(|(_, x)| {
                let (a, b) = (x.route.pos(end), x.route.pos(Tick::ZERO));
                (a.0 - b.0).abs().max((a.1 - b.1).abs()) > 256
            })
            .count();
        assert!(moved > 0, "the herds migrate");
        // The east is richer: with the bridge they go round by it, without it they never get there.
        assert_eq!(crossed_east, bridge, "east reached only over the bridge");
    }
}

#[test]
fn a_herd_never_plans_into_unknown_ground() {
    // Only a 1,024-block square is known; the herds' reach is far larger.
    let mut w = SampledWorld::new();
    let mut p = Painter::default();
    p.paint(&mut w, 0, (0, 0, 1024, 1024), &|_, _| {
        land("minecraft:plains")
    });
    let world = WorldHandle::new(Box::new(w));
    let mut h = herds_on(&world);
    let mut legs = LegIndex::new(1_000);
    h.add_herd(
        NewHerd {
            name: "boxed".into(),
            species: HerdSpecies::Horse,
            frame: 0,
            pos: (512, 512),
            count: 2_000,
            seed: Some(5),
        },
        Tick::ZERO,
    );
    let end = Tick::from_days(96);
    let mut t = Tick::ZERO;
    while t < end {
        t = (t + 4 * TICKS_PER_DAY).min(end);
        h.run_until(&mut legs, t);
        for (x, z) in route_samples(&h) {
            assert!(
                (0..1024).contains(&x) && (0..1024).contains(&z),
                "a route leaves the sampled ground at ({x}, {z})"
            );
        }
    }
}

#[test]
fn herds_founded_per_region_stand_on_land_and_stay_there() {
    // An archipelago: land where a coarse checker says so, sea elsewhere, over 3 × 3 regions.
    let mut w = SampledWorld::new();
    let mut p = Painter::default();
    p.paint(&mut w, 0, (0, 0, 1536, 1536), &|x, z| {
        let (a, b) = (x.div_euclid(384), z.div_euclid(384));
        if (a + b) % 2 == 0 {
            land(if a == 2 {
                "minecraft:snowy_slopes"
            } else {
                "minecraft:plains"
            })
        } else {
            sea()
        }
    });
    let world = WorldHandle::new(Box::new(w));
    let climate = HabitatClimate::new(world.clone());
    let mut h = herds_on(&world);
    let mut legs = LegIndex::new(1_000);
    let mut founded = Vec::new();
    for rz in 0..3 {
        for rx in 0..3 {
            let k = RegionKey { frame: 0, rx, rz };
            let plan = plan_region(
                &**world.read(),
                &climate,
                k,
                9,
                &FoundingTunables::default(),
            );
            if let RegionPlan::Found(n) = plan {
                let c = cell_of(i64::from(n.pos.0), i64::from(n.pos.1));
                assert!(
                    climate.terrain(0, c.0, c.1).is_finite(),
                    "{n:?} on open ground"
                );
                founded.push(n.species);
                h.add_herd(n, Tick::ZERO);
            }
        }
    }
    assert!(founded.len() >= 4, "most regions hold land: {founded:?}");
    assert!(
        founded.contains(&HerdSpecies::Goat),
        "the snowy east founds goats"
    );
    let end = Tick::from_days(96);
    let mut t = Tick::ZERO;
    while t < end {
        t = (t + 3 * TICKS_PER_DAY).min(end);
        h.run_until(&mut legs, t);
        h.check(t).expect("invariants");
        for (x, z) in route_samples(&h) {
            let c = cell_of(x, z);
            assert!(
                climate.terrain(0, c.0, c.1).is_finite(),
                "({x}, {z}) is open ground"
            );
        }
    }
    // Forage cells are whole habitat cells.
    assert_eq!(CELL_BLOCKS % rtsim_space::habitat::CELL_BLOCKS, 0);
}

/// The cost of a herd's decisions on the world model against the uniform climate (Y3), for
/// bench/results/habitat-sampler.md: `cargo test --release -p rtsim-ecology --test habitat --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn decision_cost_on_the_world_model() {
    use rtsim_ecology::herds::climate::Uniform;
    let world = strait(true);
    for (name, climate) in [
        (
            "world model",
            Box::new(HabitatClimate::new(world.clone())) as Box<dyn Climate>,
        ),
        ("uniform", Box::new(Uniform) as Box<dyn Climate>),
    ] {
        let mut h = Herds::new(41, HerdTunables::default(), climate, Tick::ZERO);
        let mut legs = LegIndex::new(1_000);
        for i in 0..16u64 {
            h.add_herd(
                NewHerd {
                    name: format!("h{i}"),
                    species: HerdSpecies::Sheep,
                    frame: 0,
                    pos: (200 + (i as i32 % 4) * 1000, 200 + (i as i32 / 4) * 1000),
                    count: 3_000,
                    seed: Some(i + 1),
                },
                Tick::ZERO,
            );
        }
        let t0 = std::time::Instant::now();
        h.run_until(&mut legs, Tick::from_days(2 * 96));
        let spent = t0.elapsed();
        let decisions = h.stats.decisions.max(1);
        eprintln!(
            "{name}: {decisions} decisions in {:.1} ms, {:.1} us each",
            spent.as_secs_f64() * 1e3,
            spent.as_secs_f64() * 1e6 / decisions as f64
        );
    }
}
