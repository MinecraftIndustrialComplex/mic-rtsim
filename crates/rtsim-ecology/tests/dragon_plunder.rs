// SPDX-License-Identifier: AGPL-3.0-or-later
//! A hungry dragon plunders settlements for food (Ben, 2026-10-31; docs/overnight-choices-
//! equilibrium.md EQC556 to EQC560, PP-1 (a), PP-4 (a)).
//!
//! - A settlement a dragon raided rests 8 days (`dragon_rest.rs` for villages).
//! - A camp whose members are its prey loses one a raid, never its last two, as no player's loss.
//! - A player district's stores, unwatched, lose food and nothing else; watched, the dragon comes
//!   in person.

use std::cell::RefCell;

use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdTunables, Herds};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome};
use rtsim_ecology::trophic::CampPrey;
use rtsim_ecology::{
    DragonId, DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth,
};
use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::threat::Threat;
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

fn hungry_adult(d: &mut Dragons, w: &mut World, l: LairId) -> DragonId {
    d.add_dragon(
        w,
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
    )
}

fn lair(d: &mut Dragons, w: &mut World, x: i32, z: i32) -> LairId {
    d.add_lair(
        w,
        1,
        "Hungry Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, x, z),
        100,
        Tick::ZERO,
    )
}

/// A hungry dragon whose only food is a raider camp 400 blocks away: it takes one member a raid,
/// leaves the camp its last two, and the losses go to the camp's engine (the host books them).
#[test]
fn a_hungry_dragon_raids_a_camp() {
    let mut w = world_without_villages(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    let l = lair(&mut d, &mut w, 5_200, 5_200);
    hungry_adult(&mut d, &mut w, l);
    let actor = w.nodes[0].actor;
    let mut h = Herds::new(5, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    // The camp holds 5: 3 above its refuge of 2.
    let mut pool: u32 = 5;
    let mut losses = 0;
    for day in 1..=60 {
        d.set_camps(vec![CampPrey {
            key: 77,
            actor,
            pos: Pos::new(0, 5_500, 5_400),
            takeable: pool.saturating_sub(2),
        }]);
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
        for loss in d.take_camp_losses() {
            assert_eq!(loss.key, 77);
            pool -= 1;
            losses += 1;
        }
    }
    assert_eq!(losses, 3, "every member above the refuge, and no more");
    assert_eq!(pool, 2);
    assert_eq!(d.camp_raids, 3);
}

/// A district 300 blocks from a hungry dragon's lair with food in store: unwatched, it is raided
/// through `raid_eating` alone (food only, no block, no plain raid); watched with its player near,
/// the dragon comes in person and nothing is resolved abstractly.
#[derive(Default)]
struct Stores {
    watched: bool,
    eating: RefCell<Vec<(i64, bool, bool)>>,
    other: RefCell<u32>,
}

impl Infrastructure for Stores {
    fn districts_near(&mut self, frame: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        vec![DistrictSite {
            id: DistrictId(9),
            pos: Pos::new(frame, 5_400, 5_400),
            watched: self.watched,
            loaded: self.watched,
        }]
    }
    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        *self.other.borrow_mut() += 1;
        None
    }
    fn raid_drinking(&mut self, _: &Threat, _: &[String], _: i64) -> Option<RaidOutcome> {
        *self.other.borrow_mut() += 1;
        None
    }
    fn raid_in_sight(&mut self, _: &Threat, _: &[String], _: i64) -> Option<RaidOutcome> {
        *self.other.borrow_mut() += 1;
        None
    }
    fn good_name(&self, g: Good) -> Option<String> {
        (g == Good(1)).then(|| "minecraft:beef".to_string())
    }
    fn edible_food(&mut self, _: DistrictId, food: &dyn Fn(&str, i64) -> i64) -> i64 {
        food("minecraft:beef", 64) + food("minecraft:diamond", 64)
    }
    fn raid_eating(
        &mut self,
        t: &Threat,
        edible: &dyn Fn(&str) -> bool,
        room: i64,
    ) -> Option<RaidOutcome> {
        self.eating.borrow_mut().push((
            room,
            edible("minecraft:beef"),
            edible("minecraft:diamond") || t.loads.is_some_and(|l| l.blocks > 0),
        ));
        let mut stolen = GoodVec::new();
        stolen.add(Good(1), 20);
        Some(RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: false,
            stolen,
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }
}

#[test]
fn a_district_raid_takes_food_only() {
    let run = |watched: bool| {
        let mut w = world_without_villages(7);
        let mut d = Dragons::new(7, DragonTunables::default(), Tick::ZERO);
        let l = lair(&mut d, &mut w, 5_200, 5_200);
        let dragon = hungry_adult(&mut d, &mut w, l);
        let mut infra = Stores {
            watched,
            ..Stores::default()
        };
        let mut h = Herds::new(7, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
        for day in 1..=6 {
            let t = Tick(day * TICKS_PER_DAY);
            if watched {
                d.observe_player(&mut w, (1, 2), Pos::new(0, 5_420, 5_420), t);
            }
            d.run_until_with_herds(&mut w, &mut infra, &mut h, t);
        }
        let eating = infra.eating.borrow().clone();
        let other = *infra.other.borrow();
        (
            eating,
            other,
            d.larder_raids,
            d.stats.materialized,
            d.dragon(dragon).is_some(),
        )
    };
    let (eating, other, raids, placed, _) = run(false);
    assert!(
        raids >= 1 && !eating.is_empty(),
        "no hunger raid: {eating:?}"
    );
    for (room, beef, other_kind) in &eating {
        assert!(*room > 0 && *beef, "room {room}, beef edible {beef}");
        assert!(!other_kind, "it would take a diamond or break a block");
    }
    assert_eq!(other, 0, "a plain, drinking or in-sight raid");
    assert_eq!(placed, 0);
    let (eating, other, _, placed, _) = run(true);
    assert!(
        eating.is_empty() && other == 0,
        "watched, resolved abstractly"
    );
    assert!(placed >= 1, "watched, the dragon did not come in person");
}

/// A village whose market holds no live animals, as every village of a real world (the bridge's
/// goods have no `cattle`, `sheep` or `pig`): a hungry dragon with nothing else to eat raids it for
/// villagers under PP-2 (a), one a raid, eight days apart, never its last two (EQC579). Its offer
/// was its livestock alone, so it was never raided while the arrival gate counted it as food.
#[test]
fn a_village_with_no_livestock_loses_villagers_to_a_hungry_dragon() {
    let (g, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let nodes = synth::nodes()
        .into_iter()
        .filter(|n| n.key == 1)
        .collect();
    let mut w = World::build(WorldConfig::new(11), g, book, nodes);
    let v = w.nodes.iter().position(|n| n.key == 1).expect("the village");
    assert!(w.village_row(v).is_some());
    let before = w.village_members(v);
    assert!(before > 4, "{before}");
    let mut d = Dragons::new(11, DragonTunables::default(), Tick::ZERO);
    let l = lair(&mut d, &mut w, 300, 100);
    hungry_adult(&mut d, &mut w, l);
    let mut h = Herds::new(11, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    let mut takes = Vec::new();
    for day in 1..=40 {
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
        for c in d.take_commands() {
            if let rtsim_ecology::boundary::Cmd::TakeResident(t) = c {
                assert_eq!(t.village, 1);
                takes.push(day);
            }
        }
    }
    assert!(!takes.is_empty(), "no villager taken");
    assert!(
        takes.windows(2).all(|p| p[1] - p[0] >= 8),
        "a village rests 8 days: {takes:?}"
    );
    assert_eq!(d.villagers_taken, takes.len() as u64);
    assert_eq!(w.village_members(v), before - takes.len() as u32);
    assert!(w.village_members(v) >= 2);
}
