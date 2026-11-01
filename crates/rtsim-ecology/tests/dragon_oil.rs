// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragons are not rare near oil (Ben, 2026-10-31; docs/overnight-choices-equilibrium.md EQC561):
//! an oil field is a constant weight for fire dragons, shared by the mouths whose nine regions
//! hold it, that draws newcomers and keeps a dragon from starving out, and breeds none.

use rtsim_ecology::boundary::{self, Obs, OilChunkRec, OilChunks};
use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdTunables, Herds};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

/// The scenario world with every village and road moved 100,000 blocks west.
fn world_without_villages(seed: u64) -> World {
    let (g, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let nodes = synth::nodes()
        .into_iter()
        .map(|mut n| {
            n.pos.x -= 100_000;
            n
        })
        .collect();
    World::build(WorldConfig::new(seed), g, book, nodes)
}

/// A 3 by 3 chunk field from chunk (cx, cz), as Deep Time reports its traps.
fn field(cx: i32, cz: i32) -> Obs {
    Obs::OilChunks(OilChunks {
        dimension: 0,
        source: boundary::OIL_SOURCE_DEEP_TIME,
        chunks: (0..9)
            .map(|k| OilChunkRec {
                cx: cx + k % 3,
                cz: cz + k / 3,
                amount: 1,
                flags: 0,
            })
            .collect(),
    })
}

fn lair(d: &mut Dragons, w: &mut World, key: u64, species: Species, x: i32, z: i32) -> LairId {
    d.add_lair(
        w,
        key,
        "Oil Roost",
        species,
        LairKind::Roost,
        Pos::new(0, x, z),
        100,
        Tick::ZERO,
    )
}

/// No herds and no village anywhere: only the oil and the wild floor.
fn no_herds(seed: u64) -> Herds {
    Herds::new(seed, HerdTunables::default(), Box::new(Uniform), Tick::ZERO)
}

fn run(d: &mut Dragons, w: &mut World, h: &mut Herds, days: u64) {
    for day in 1..=days {
        d.run_until_with_herds(w, &mut NoInfrastructure, h, Tick(day * TICKS_PER_DAY));
    }
}

/// An empty fire lair at a field is filled at its wait's end; an ice lair at another field, with
/// the same nothing else around it, stays shut: the oil row is a fire dragon's.
#[test]
fn oil_holds_a_fire_lair() {
    let mut w = world_without_villages(3);
    let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
    // Fields at chunks (320, 320) and (1,570, 320): blocks 5,120 and 25,120.
    d.ingest(&mut w, &mut NoInfrastructure, &field(320, 320), Tick::ZERO);
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &field(1_570, 320),
        Tick::ZERO,
    );
    let fire = lair(&mut d, &mut w, 1, Species::Fire, 5_200, 5_200);
    let ice = lair(&mut d, &mut w, 2, Species::Ice, 25_200, 5_200);
    let mut h = no_herds(3);
    let s = d.ground_share_with(&w, &mut h, fire, Tick::ZERO);
    let s_ice = d.ground_share_with(&w, &mut h, ice, Tick::ZERO);
    eprintln!("gate at the fire lair {s:.3}, at the ice lair {s_ice:.3}");
    run(&mut d, &mut w, &mut h, 30);
    assert!(
        !d.lairs[fire as usize].occupants.is_empty(),
        "the fire lair at the field stands empty"
    );
    assert!(
        d.lairs[ice as usize].occupants.is_empty(),
        "the ice lair took a newcomer"
    );
}

/// A fire dragon at a field catches nothing for 90 days and stays (and the oil draws it a mate);
/// an ice dragon at another field, catching as little, starves out.
#[test]
fn oil_keeps_a_fire_dragon() {
    let mut w = world_without_villages(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    d.ingest(&mut w, &mut NoInfrastructure, &field(320, 320), Tick::ZERO);
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &field(1_570, 320),
        Tick::ZERO,
    );
    let fire = lair(&mut d, &mut w, 1, Species::Fire, 5_200, 5_200);
    let ice = lair(&mut d, &mut w, 2, Species::Ice, 25_200, 5_200);
    let mut holders = Vec::new();
    for (l, species) in [(fire, Species::Fire), (ice, Species::Ice)] {
        holders.push(d.add_dragon(
            &mut w,
            NewDragon {
                name: "Holder".into(),
                species,
                gender: Gender::Female,
                variant: 0,
                age_ticks: 110 * TICKS_PER_DAY,
                lair: Some(l),
                fullness: 100,
                health: None,
                seed: None,
                at: None,
            },
            Tick::ZERO,
        ));
    }
    let mut h = no_herds(5);
    run(&mut d, &mut w, &mut h, 90);
    eprintln!(
        "fed: fire {:.2}, ice {:.2}; starved away {}",
        d.lairs[fire as usize].fed, d.lairs[ice as usize].fed, d.stats.starved_away
    );
    assert!(
        d.lairs[fire as usize].fed < 0.35,
        "the fire dragon caught food"
    );
    assert!(
        d.lairs[fire as usize].occupants.contains(&holders[0]),
        "the fire dragon left its field"
    );
    assert!(
        d.lairs[ice as usize].occupants.is_empty(),
        "the ice dragon stayed hungry"
    );
}

/// Oil breeds none: a fire pair at a field whose catch is held under maintenance hatches nothing
/// in 240 days, though the oil keeps both.
#[test]
fn oil_breeds_none() {
    let mut w = world_without_villages(7);
    let tun = DragonTunables {
        hunger_tick_rate: u64::MAX / 4,
        ..DragonTunables::default()
    };
    let mut d = Dragons::new(7, tun, Tick::ZERO);
    d.ingest(&mut w, &mut NoInfrastructure, &field(320, 320), Tick::ZERO);
    let l = lair(&mut d, &mut w, 1, Species::Fire, 5_200, 5_200);
    let mut pair = Vec::new();
    for g in [Gender::Female, Gender::Male] {
        pair.push(d.add_dragon(
            &mut w,
            NewDragon {
                name: "Parent".into(),
                species: Species::Fire,
                gender: g,
                variant: 0,
                age_ticks: 110 * TICKS_PER_DAY,
                lair: Some(l),
                fullness: 100,
                health: None,
                seed: None,
                at: None,
            },
            Tick::ZERO,
        ));
    }
    d.pair(&mut w, pair[0], pair[1], Tick::ZERO);
    let mut h = no_herds(7);
    for day in 1..=240 {
        d.lairs[l as usize].fed = 0.3;
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
    }
    assert_eq!(d.stats.births, 0);
    assert_eq!(d.lairs[l as usize].occupants.len(), 2);
}
