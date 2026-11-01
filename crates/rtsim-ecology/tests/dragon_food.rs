// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragon food gate (Ben, 2026-10-31; docs/overnight-choices-equilibrium.md EQC530 onward,
//! EQUILIBRIUM.md §2.4.1): new dragons come only from a food surplus.
//!
//! - A lair's breeder hatches a young one at a clutch with a chance that rises with its lair's fed
//!   share above maintenance (`starve_leave`), and is zero at or below it, mate or no mate.
//! - The arrival gate shares each herd's and each village's weight among the occupied lairs whose
//!   nine regions hold it, the newcomer counted, and a village alone no longer opens it.

use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{
    DragonId, DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth,
};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

/// The scenario world with every village and road moved 100,000 blocks west: nothing near the
/// lairs but what a test puts there.
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

/// Dragons that never grow hungry: no hunt and no empty day ever moves a lair's fed share, so a
/// test sets it and reads the clutches it gives.
fn sated(seed: u64) -> (World, Dragons) {
    let w = world_without_villages(seed);
    let tun = DragonTunables {
        hunger_tick_rate: u64::MAX / 4,
        ..DragonTunables::default()
    };
    (w, Dragons::new(seed, tun, Tick::ZERO))
}

/// `n` lairs 20,000 blocks apart, each with an adult female (and a mated male with `paired`), each
/// lair's fed share held at `fed`; run for `days`. Returns the births.
fn clutches(seed: u64, n: usize, paired: bool, fed: f32, days: u64) -> u64 {
    let (mut w, mut d) = sated(seed);
    let mut lairs = Vec::new();
    for i in 0..n {
        let l = d.add_lair(
            &mut w,
            100 + i as u64,
            "Held Roost",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 20_000 * (i as i32 + 1), 20_000),
            100,
            Tick::ZERO,
        );
        let f: DragonId = d.add_dragon(&mut w, adult("Dam", Gender::Female, l), Tick::ZERO);
        if paired {
            let m = d.add_dragon(&mut w, adult("Sire", Gender::Male, l), Tick::ZERO);
            d.pair(&mut w, f, m, Tick::ZERO);
        }
        lairs.push(l);
    }
    let mut day = 0;
    while day < days {
        for &l in &lairs {
            let lair = &mut d.lairs[l as usize];
            lair.fed = fed;
        }
        day += 1;
        d.run_until(&mut w, &mut NoInfrastructure, Tick(day * TICKS_PER_DAY));
    }
    d.stats.births
}

/// No surplus, no young: at or below maintenance (`starve_leave`, 0.35) a lair's breeder hatches
/// nothing, mated or alone, however many clutches come round.
#[test]
fn no_birth_without_surplus() {
    let m = DragonTunables::default().starve_leave;
    for fed in [0.0, 0.2, m] {
        for paired in [true, false] {
            let born = clutches(5, 12, paired, fed, 240);
            assert_eq!(born, 0, "fed {fed}, paired {paired}: {born} born");
        }
    }
}

/// Births rise with the surplus above maintenance: at a full fed share every clutch hatches one
/// (one in `breed_days`), half way between maintenance and full about half do. A pair is one
/// breeder (EQC555: only pairs breed; `dragon_pairs.rs` has the lone dragon).
#[test]
fn births_scale_with_surplus() {
    let m = DragonTunables::default().starve_leave;
    let half = m + (1.0 - m) / 2.0;
    // 30 pairs, 240 days: each pair (110 days old, mature) has its first clutch at day 60, then
    // at 120, 180 and 240: four clutches a lair, 120 in all.
    let full = clutches(7, 30, true, 1.0, 240);
    let mid = clutches(7, 30, true, half, 240);
    let low = clutches(7, 30, true, m + 0.05, 240);
    eprintln!("births in 120 clutches: full {full}, half {mid}, just above {low}");
    assert_eq!(full, 120, "every clutch hatches at a full fed share");
    assert!((40..=80).contains(&mid), "{mid} of 120 at half the surplus");
    assert!(
        low < mid && low <= 25,
        "{low} just above maintenance, {mid} at half"
    );
}

/// Two lairs in one region, their nine regions the same: with the second lair held, every weight
/// the gate reads at the first (its herd, the wild floor) is shared between the two, so the
/// weight a newcomer would find there is half what it finds with the second lair empty.
#[test]
fn lairs_on_the_same_herds_share_them() {
    let gate = |held: bool| {
        let mut w = world_without_villages(3);
        let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
        let a = d.add_lair(
            &mut w,
            1,
            "First Roost",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 5_200, 5_200),
            100,
            Tick::ZERO,
        );
        let b = d.add_lair(
            &mut w,
            2,
            "Second Roost",
            Species::Ice,
            LairKind::Roost,
            Pos::new(0, 5_400, 5_350),
            100,
            Tick::ZERO,
        );
        if held {
            let mut x = adult("Holder", Gender::Female, b);
            x.species = Species::Ice;
            d.add_dragon(&mut w, x, Tick::ZERO);
        }
        let mut h = Herds::new(3, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
        h.add_herd(
            NewHerd {
                name: "Shared flock".into(),
                species: HerdSpecies::Sheep,
                frame: 0,
                pos: (5_300, 5_250),
                count: 330,
                seed: Some(9),
            },
            Tick::ZERO,
        );
        // A day on, the herd stands in its region's index; nobody is hungry yet.
        let day = Tick(TICKS_PER_DAY);
        d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, day);
        let s = d.ground_share_with(&w, &mut h, a, day);
        let half = d.tun.gate_half;
        (s, half * s / (1.0 - s))
    };
    let (alone, w_alone) = gate(false);
    let (shared, w_shared) = gate(true);
    eprintln!(
        "gate at the first lair: alone {alone:.3} (W {w_alone:.1}), shared {shared:.3} (W {w_shared:.1})"
    );
    assert!(
        (w_shared / w_alone - 0.5).abs() < 1e-6,
        "W {w_shared:.2} shared against {w_alone:.2} alone"
    );
    assert!(
        alone >= f64::from(DragonTunables::default().arrive_food),
        "alone {alone:.3}"
    );
    assert!(
        shared < f64::from(DragonTunables::default().arrive_food),
        "shared {shared:.3}"
    );
}

/// A village alone opens nothing: a lair whose nine regions hold one village and no herd stays
/// shut at its wait's end and at each check after, and no newcomer comes.
#[test]
fn a_village_alone_opens_nothing() {
    // The scenario world's village 8 stands at (100, -1,200), alone in the nine regions around
    // region (0, -3).
    let (mut w, _) = synth::world(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        1,
        "Village Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 300, -1_300),
        100,
        Tick::ZERO,
    );
    let mut h = Herds::new(5, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    let share = d.ground_share_with(&w, &mut h, l, Tick::ZERO);
    let arrive = f64::from(d.tun.arrive_food);
    assert!(
        share < arrive,
        "a village alone reads {share:.3}, the gate {arrive}"
    );
    // The wait ends at day 20; checks at 20, 44 and 68.
    d.run_until_with_herds(
        &mut w,
        &mut NoInfrastructure,
        &mut h,
        Tick(70 * TICKS_PER_DAY),
    );
    assert_eq!((d.stats.respawns, d.stats.heirs_sent), (0, 0));
    assert_eq!(d.stats.arrivals_gated, 3);
    assert!(d.lairs[l as usize].occupants.is_empty());
}
