// SPDX-License-Identifier: AGPL-3.0-or-later
//! A settlement a dragon raided rests 8 days (docs/overnight-choices-equilibrium.md EQC557): no
//! dragon raids it again before. Here for villages, whose raids the engine had before plunder.

use std::collections::BTreeMap;

use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdTunables, Herds};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{DragonTunables, Dragons, Gender, LairKind, NewDragon, Species, synth};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;

/// A dragon among the scenario world's villages, with no herd: no village is raided twice within
/// 8 days.
#[test]
fn a_raided_village_rests() {
    let (mut w, _) = synth::world(3);
    let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
    d.log_enabled = true;
    let l = d.add_lair(
        &mut w,
        1,
        "Hungry Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 300, 100),
        100,
        Tick::ZERO,
    );
    d.add_dragon(
        &mut w,
        NewDragon {
            name: "Raider".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(l),
            fullness: 40,
            health: None,
            seed: None,
            at: None,
        },
        Tick::ZERO,
    );
    let mut h = Herds::new(3, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    for day in 1..=40 {
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
    }
    let mut last: BTreeMap<String, f64> = BTreeMap::new();
    let mut raids = 0;
    for e in d
        .log
        .iter()
        .filter(|e| e.kind == "prey" && e.what.starts_with("village "))
    {
        if e.what.contains("+0 fullness") {
            continue;
        }
        // "village at village 3: +20 fullness, took ..."
        let head = e.what.split(": ").next().unwrap_or_default();
        let village = head.trim_start_matches("village at ").to_string();
        if let Some(prev) = last.insert(village.clone(), e.day) {
            assert!(
                e.day - prev >= 8.0,
                "{village} raided on day {prev:.1} and again on day {:.1}",
                e.day
            );
        }
        raids += 1;
    }
    assert!(raids >= 2, "{raids} village raids in 40 days");
}
