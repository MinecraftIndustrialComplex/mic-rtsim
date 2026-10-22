// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons on the shared renewal rule (EQUILIBRIUM.md §2.4.1, slice S4): a farmed lair settles
//! at the closed form's yield, and a lair whose ground is eaten out loses its dragon and waits for
//! the herds before it takes another. The world-level checks (kill all, the 60-year roamer) are in
//! `rtsim-cli`'s `sim::equilibrium`; the oil visitors' wait is `oil.rs`'s
//! `visitor_claims_only_a_due_lair`.

use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{DragonTunables, Dragons, Gender, LairKind, NewDragon, Species, synth};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

/// Days in a game year (the dragons' calendar).
const YEAR: u64 = 96;

/// The scenario world with every village and road moved 100,000 blocks west: nothing near the
/// valley feeds a dragon but the herds a test puts there.
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

/// §3.3: a lair whose every newcomer a player kills within two days of its claim (the model's
/// farmer looks every 2 days) settles near `P* = 2.12` and the closed form's 1.08 arrivals a
/// year; the plan's bound is 1.2. Each kill is the player's, so each raises the lair's pressure,
/// and the next wait reads it.
#[test]
fn farmed_lair_yields_closed_form() {
    // The newcomers' flights are the same length whatever the seed: one seed says it all.
    farmed_lair(11);
}

fn farmed_lair(seed: u64) {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        1,
        "Farmed Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 20_000, 20_000),
        100,
        Tick::ZERO,
    );
    let player = d.player(&mut w, (1, 2));
    let mut kills: Vec<u64> = Vec::new();
    let mut p_after: Vec<f32> = Vec::new();
    let years = 60;
    let mut day = 0;
    while day < years * YEAR {
        day += 2;
        let at = Tick(day * TICKS_PER_DAY);
        d.run_until(&mut w, &mut NoInfrastructure, at);
        for o in d.lairs[l as usize].occupants.clone() {
            d.slay(&mut w, o, Some(player), at);
            kills.push(day);
            let rule = d.tun.renewal();
            p_after.push(
                d.lairs[l as usize]
                    .pressure
                    .now(day as u32, rule.half_life_days),
            );
        }
    }
    assert_eq!(d.stats.player_losses, kills.len() as u64);
    // The farmed state, over the last 30 years.
    let late: Vec<usize> = (0..kills.len())
        .filter(|&i| kills[i] >= 30 * YEAR)
        .collect();
    let rate = late.len() as f64 / 30.0;
    let p_mean = late.iter().map(|&i| f64::from(p_after[i])).sum::<f64>() / late.len() as f64;
    let (p_star, gap) = d.tun.renewal().farmed(1, 2.0);
    let closed = YEAR as f64 / f64::from(gap);
    eprintln!(
        "farmed lair, seed {seed}: {} kills in 60 years, {rate:.3} a year over the last 30 (closed form {closed:.3}), \
         P after a kill {p_mean:.3} (closed form {p_star:.3})",
        kills.len()
    );
    assert!(rate <= 1.2, "{rate:.3} arrivals a year");
    assert!(
        (rate - closed).abs() / closed <= 0.06,
        "{rate:.3} a year against the closed form's {closed:.3}"
    );
    assert!(
        (p_mean - f64::from(p_star)).abs() / f64::from(p_star) <= 0.06,
        "P {p_mean:.3} against {p_star:.3}"
    );
    // Every wait was the rule's: none shorter than the base, none longer than the cap.
    for k in kills.windows(2) {
        let gap = k[1] - k[0];
        assert!(
            (20..=192 + 4).contains(&gap),
            "{gap} days between kills {k:?}"
        );
    }
}

/// §2.4.1: a dragon whose lair's ground feeds it nothing starves out (no player loss: the lair's
/// pressure stays 0) and, with no vacant lair in reach, leaves the sim. Its lair's wait ends after
/// the base, but the ground would not feed a newcomer, so the lair checks again every 24 days and
/// stays empty; once herds graze near it, the next check brings a newcomer, which stays.
#[test]
fn starving_dragon_leaves_and_returns_when_fed() {
    for seed in 17..=24 {
        starving_dragon(seed);
    }
}

fn starving_dragon(seed: u64) {
    let mut w = world_without_villages(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    d.log_enabled = true;
    let lair_pos = Pos::new(0, 0, 2_500);
    let l = d.add_lair(
        &mut w,
        7,
        "Bare Roost",
        Species::Fire,
        LairKind::Roost,
        lair_pos,
        100,
        Tick::ZERO,
    );
    let old = d.add_dragon(
        &mut w,
        NewDragon {
            name: "Gaunt".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(l),
            fullness: 30,
            health: None,
            seed: None,
            at: None,
        },
        Tick::ZERO,
    );
    let mut h = Herds::new(
        seed,
        HerdTunables::default(),
        Box::new(Uniform),
        Tick::ZERO,
    );
    let tun = d.tun.clone();
    // Starving: the fed share falls by e^(-1/30) a day from 1, below 0.35 after 32 days.
    let mut left = None;
    for day in 1..=60u64 {
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
        if d.dragon(old).is_none() {
            left = Some(day);
            break;
        }
    }
    let left = left.expect("the starving dragon left");
    assert!((30..=40).contains(&left), "left on day {left}");
    assert_eq!(
        (d.stats.starved_away, d.stats.migrations, d.stats.player_losses),
        (1, 0, 0)
    );
    let lair = &d.lairs[l as usize];
    assert!(lair.occupants.is_empty());
    assert!(lair.fed < tun.starve_leave);
    assert_eq!(lair.pressure.now(left as u32, 96.0), 0.0);
    // It left during the day before `left`: the wait is the base from then.
    let base = tun.respawn_delay_days * TICKS_PER_DAY;
    assert!(
        lair.due > Tick((left - 1) * TICKS_PER_DAY) + base
            && lair.due <= Tick(left * TICKS_PER_DAY) + base,
        "due {:?}, left on day {left}",
        lair.due
    );
    // The wait ends after the base, but nothing would feed a newcomer: checked, put off, and
    // checked again 24 days later.
    let due = lair.due;
    let gated_until = due + 2 * tun.arrive_recheck_days * TICKS_PER_DAY + TICKS_PER_DAY;
    d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, gated_until);
    assert_eq!(d.stats.arrivals_gated, 3, "at the wait's end and twice after");
    assert_eq!((d.stats.respawns, d.stats.heirs_sent), (0, 0));
    assert!(d.lairs[l as usize].occupants.is_empty());
    // Herds come to graze by the lair (on uniform grassland, so they stay near it): the next
    // check brings a newcomer.
    for (i, (dx, dz)) in [(-250, -150), (200, 100), (0, 300)].into_iter().enumerate() {
        h.add_herd(
            NewHerd {
                name: format!("Returning flock {i}"),
                species: HerdSpecies::Sheep,
                frame: 0,
                pos: (lair_pos.x + dx, lair_pos.z + dz),
                count: 2_000,
                seed: Some(40 + i as u64),
            },
            gated_until,
        );
    }
    let next_check = due + 3 * tun.arrive_recheck_days * TICKS_PER_DAY;
    d.run_until_with_herds(
        &mut w,
        &mut NoInfrastructure,
        &mut h,
        next_check + 2 * TICKS_PER_DAY,
    );
    assert_eq!(d.stats.arrivals_gated, 3, "fed ground passes the gate");
    assert_eq!(d.stats.respawns, 1);
    let newcomer = *d.lairs[l as usize]
        .occupants
        .first()
        .expect("a newcomer claimed the lair");
    // It hunts the herds and stays: two seasons on, it still holds the lair, well fed.
    let later = next_check + 2 * 24 * TICKS_PER_DAY * 2;
    d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, later);
    assert_eq!(d.stats.starved_away, 1);
    assert_eq!(d.lairs[l as usize].occupants, vec![newcomer]);
    assert!(d.stats.herd_hunts > 0);
    let fed = d.lairs[l as usize].fed;
    assert!(fed >= tun.arrive_food, "fed share {fed}");
}
