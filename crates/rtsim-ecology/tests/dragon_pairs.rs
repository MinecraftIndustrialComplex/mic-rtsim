// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dragons breed only as pairs, and a lair with surplus draws a mate (Ben, 2026-10-31: "Surplus
//! draws a mate"; docs/overnight-choices-equilibrium.md EQC553 to EQC555, EQUILIBRIUM.md §2.4.1).
//!
//! - A lone dragon never breeds, however well fed.
//! - A lone mature dragon whose lair would feed one more mouth draws a mate of the other sex and
//!   its species at its clutch timer; with no surplus it draws none.
//! - A lone breeder in reach takes a Frontier dragon as its mate before an empty lair takes it.
//! - Mouths, not lairs, share the ground: a pair counts two.

use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{
    DragonId, DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth,
};
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

fn adult(name: &str, gender: Gender, lair: LairId) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender,
        variant: 0,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(lair),
        fullness: 100,
        health: None,
        seed: None,
        at: None,
    }
}

/// Dragons that never grow hungry, so no hunt moves a fed share or a herd.
fn sated() -> DragonTunables {
    DragonTunables {
        hunger_tick_rate: u64::MAX / 4,
        ..DragonTunables::default()
    }
}

fn roost(d: &mut Dragons, w: &mut World, key: u64, x: i32, z: i32) -> LairId {
    d.add_lair(
        w,
        key,
        "Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, x, z),
        100,
        Tick::ZERO,
    )
}

/// Herds enough that a lair at (5,200, 5,200) would feed two dragons: three flocks in its region.
fn rich_herds(seed: u64) -> Herds {
    let mut h = Herds::new(seed, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    for (i, pos) in [(5_300, 5_250), (5_100, 5_400), (5_400, 5_050)].into_iter().enumerate() {
        h.add_herd(
            NewHerd {
                name: format!("Flock {i}"),
                species: HerdSpecies::Sheep,
                frame: 0,
                pos,
                count: 330,
                seed: Some(9 + i as u64),
            },
            Tick::ZERO,
        );
    }
    h
}

/// A lone adult, its lair's fed share held full, hatches nothing in 240 days (its ground has no
/// herd and no village, so it draws no mate either); a pair, held the same, hatches one young a
/// clutch.
#[test]
fn a_lone_dragon_never_breeds() {
    let born = |paired: bool| {
        let mut w = world_without_villages(5);
        let mut d = Dragons::new(5, sated(), Tick::ZERO);
        let mut h = Herds::new(5, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
        let mut lairs = Vec::new();
        for i in 0..10 {
            let l = roost(&mut d, &mut w, 100 + i, 20_000 * (i as i32 + 1), 20_000);
            let f: DragonId = d.add_dragon(&mut w, adult("Dam", Gender::Female, l), Tick::ZERO);
            if paired {
                let m = d.add_dragon(&mut w, adult("Sire", Gender::Male, l), Tick::ZERO);
                d.pair(&mut w, f, m, Tick::ZERO);
            }
            lairs.push(l);
        }
        for day in 1..=240 {
            for &l in &lairs {
                d.lairs[l as usize].fed = 1.0;
            }
            d.run_until_with_herds(
                &mut w,
                &mut NoInfrastructure,
                &mut h,
                Tick(day * TICKS_PER_DAY),
            );
        }
        d.stats.births
    };
    assert_eq!(born(false), 0, "a lone dragon breeds");
    assert_eq!(born(true), 40, "ten pairs, four clutches each, all at a full share");
}

/// A lone mature female on rich ground draws a male of her species at her first clutch timer
/// (day 60) and pairs with him; on ground that feeds no one more (no herd, no village) she draws
/// none.
#[test]
fn a_surplus_lair_draws_a_mate() {
    let run = |rich: bool| {
        let mut w = world_without_villages(7);
        let mut d = Dragons::new(7, sated(), Tick::ZERO);
        let l = roost(&mut d, &mut w, 1, 5_200, 5_200);
        let f = d.add_dragon(&mut w, adult("Dam", Gender::Female, l), Tick::ZERO);
        let mut h = if rich {
            rich_herds(7)
        } else {
            Herds::new(7, HerdTunables::default(), Box::new(Uniform), Tick::ZERO)
        };
        for day in 1..=70 {
            d.lairs[l as usize].fed = 1.0;
            d.run_until_with_herds(
                &mut w,
                &mut NoInfrastructure,
                &mut h,
                Tick(day * TICKS_PER_DAY),
            );
        }
        let mate = d.dragon(f).and_then(|x| x.mate);
        mate.map(|m| {
            let x = d.dragon(m).expect("live mate");
            (x.gender, x.species, x.lair)
        })
    };
    let rich = run(true);
    assert!(
        matches!(rich, Some((Gender::Male, Species::Fire, Some(_)))),
        "rich ground: {rich:?}"
    );
    assert_eq!(run(false), None, "no surplus, no mate");
}

/// An empty lair whose wait ends (day 20) and a lone mature female 300 blocks away whose lair
/// would feed one more: the Frontier dragon comes as her mate, and the empty lair waits on.
#[test]
fn a_mate_comes_before_a_newcomer() {
    let mut w = world_without_villages(9);
    let mut d = Dragons::new(9, sated(), Tick::ZERO);
    let held = roost(&mut d, &mut w, 1, 5_200, 5_200);
    let empty = roost(&mut d, &mut w, 2, 5_400, 5_300);
    let f = d.add_dragon(&mut w, adult("Dam", Gender::Female, held), Tick::ZERO);
    let mut h = rich_herds(9);
    for day in 1..=26 {
        d.lairs[held as usize].fed = 1.0;
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
    }
    let mate = d.dragon(f).and_then(|x| x.mate);
    assert!(mate.is_some(), "no mate by day 26");
    assert!(
        d.lairs[empty as usize].occupants.is_empty(),
        "the empty lair took the Frontier dragon"
    );
    assert_eq!(d.stats.respawns, 1, "one Frontier dragon, the mate");
}

/// Mouths share the ground: what a newcomer would read at an empty lair beside a held one is
/// lower when that lair holds a pair than when it holds one dragon.
#[test]
fn mouths_share_the_ground() {
    let gate = |pair: bool| {
        let mut w = world_without_villages(3);
        let mut d = Dragons::new(3, sated(), Tick::ZERO);
        let a = roost(&mut d, &mut w, 1, 5_200, 5_200);
        let b = roost(&mut d, &mut w, 2, 5_400, 5_350);
        let f = d.add_dragon(&mut w, adult("Dam", Gender::Female, b), Tick::ZERO);
        if pair {
            let m = d.add_dragon(&mut w, adult("Sire", Gender::Male, b), Tick::ZERO);
            d.pair(&mut w, f, m, Tick::ZERO);
        }
        let mut h = rich_herds(3);
        let day = Tick(TICKS_PER_DAY);
        d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, day);
        let s = d.ground_share_with(&w, &mut h, a, day);
        d.tun.gate_half * s / (1.0 - s)
    };
    let (one, two) = (gate(false), gate(true));
    eprintln!("W at the empty lair: beside one dragon {one:.1}, beside a pair {two:.1}");
    // Every weight there is held by both lairs' nine regions: W / 2 beside one, W / 3 beside two.
    assert!((two / one - 2.0 / 3.0).abs() < 1e-6, "{two:.2} against {one:.2}");
}
