// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages renew, growth and the Ribbit rule (EQUILIBRIUM.md §2.4.4, slice S6;
//! docs/overnight-choices-equilibrium.md EQC163 to EQC167): an unloaded human village grows toward
//! its beds by scheduled steps of the logistic, and a Ribbit village fills the houses it has before
//! it builds another, its population booked when a resident exists.
//!
//! The synthetic world's nodes: 2 Highmoor (human, pop 12), 8 Reedholm (Ribbit, pop 10: the one
//! Ribbit village whose markets sit at their targets long enough to grow, so the one the tests use).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_travel::caravan::Body;
use rtsim_world::inputs::{Input, Uuid, apply};
use rtsim_world::{Arrive, Command, PlaceHouse, SpawnResident, World, WorldConfig, synth};
use std::collections::BTreeMap;

const HIGHMOOR: usize = 2;
const REEDHOLM: usize = 8;
const PLAYER: Uuid = (0x11, 0x22);

type Nodes = BTreeMap<u64, usize>;

fn config(seed: u64) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::AtTarget;
    cfg.growth_after_days = 3;
    cfg.growth_max_houses = 2;
    cfg
}

fn world(seed: u64) -> (World, Nodes) {
    let (w, _) = synth::build(config(seed), false);
    let nodes = (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
    (w, nodes)
}

/// The Ribbit tests' world: the one `features.rs` grows houses in (a raid on the road keeps the
/// markets' stocks moving). `refill_unconditionally` is the knob's default (EQC167, EQC184: the
/// refill takes no prosperity test); the house tests leave the test on a refill so a house is
/// only built when the village is prosperous, as before S6.
fn ribbit_world(seed: u64, refill_unconditionally: bool) -> (World, Nodes) {
    let mut cfg = config(seed);
    cfg.ribbit_refill_needs_prosperity = !refill_unconditionally;
    let (w, _) = synth::build(cfg, true);
    let nodes = (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
    (w, nodes)
}

fn day(d: u64) -> Tick {
    Tick(d * TICKS_PER_DAY)
}

fn census(w: &mut World, nodes: &mut Nodes, i: usize, residents: u32, beds: u32, at: Tick) {
    let key = w.nodes[i].key;
    let out = apply(w, nodes, &Input::Census { key, residents, beds, complete: true }, at.0);
    assert!(!out.refused);
}

fn result(w: &mut World, nodes: &mut Nodes, op: u64, placed: bool, at: Tick) {
    let out = apply(w, nodes, &Input::ResidentResult { op, placed }, at.0);
    assert!(!out.refused, "the world holds op {op:x}");
}

/// What the world commanded since the last call.
#[derive(Default)]
struct Out {
    residents: Vec<SpawnResident>,
    houses: Vec<PlaceHouse>,
}

/// Runs to `to` and takes what was commanded for village node `i` (the others' orders are dropped).
fn run(w: &mut World, to: Tick, i: usize) -> Out {
    w.run_until(to);
    let key = w.nodes[i].key;
    let mut out = Out::default();
    for c in w.take_commands() {
        match c {
            Command::SpawnResident(s) if s.village == key => out.residents.push(s),
            Command::PlaceHouse(h) if h.village == key => out.houses.push(h),
            _ => {}
        }
    }
    out
}

/// Runs a day at a time until Mirewater orders a house, and returns the order.
fn first_house(w: &mut World, from: u64) -> PlaceHouse {
    for d in from..from + 200 {
        let got = run(w, day(d), REEDHOLM);
        if let Some(h) = got.houses.into_iter().next() {
            return h;
        }
    }
    panic!("no house in 200 days");
}

// ---- EQC163, EQC164: an unloaded human village grows --------------------------------------------

/// Highmoor seen alive with `members` of `beds`, left unloaded: runs its growth steps, answering
/// each as placed, and returns the day each villager arrived.
fn grow_unloaded(seed: u64, members: u32, beds: u32, until: u64) -> Vec<f64> {
    let (mut w, mut nodes) = world(seed);
    census(&mut w, &mut nodes, HIGHMOOR, members, beds, day(1));
    let mut days = Vec::new();
    loop {
        let due = w.village_row(HIGHMOOR).unwrap().breed_due;
        if due == u64::MAX || due > day(until).0 {
            break;
        }
        let got = run(&mut w, Tick(due), HIGHMOOR);
        if got.residents.is_empty() {
            // A step that falls inside the half day a census leaves a village loaded is dropped
            // and drawn again (EQC164).
            let r = w.village_row(HIGHMOOR).unwrap();
            assert!(r.loaded_until > due, "a step with no order was dropped while loaded: {r:?}");
            continue;
        }
        assert_eq!(
            got.residents.len(),
            1,
            "one villager a step (step {}, due {due}, now {:?}, {:?})",
            days.len(),
            w.now(),
            w.village_row(HIGHMOOR)
        );
        assert_eq!(got.residents[0].arrive, Arrive::Growth);
        assert_eq!(got.residents[0].body, Body::VillagerResident);
        days.push(due as f64 / TICKS_PER_DAY as f64);
        result(&mut w, &mut nodes, got.residents[0].op, true, Tick(due));
    }
    days
}

#[test]
fn human_growth_follows_the_logistic() {
    // From 2 to 11 villagers at K 12 the logistic at g 0.01 takes sum 1 / (g n (1 - n / K)) days,
    // 385.7 (the continuous form's ln 55 / g is 400.7). The mean of 30 seeds is within 10% of 400.
    let times: Vec<f64> = (1..=30)
        .map(|seed| {
            let d = grow_unloaded(seed, 2, 12, 3_000);
            assert_eq!(d.len(), 10, "seed {seed}: grows to the beds");
            d[8] - 1.0
        })
        .collect();
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    assert!((mean - 400.0).abs() < 40.0, "mean {mean:.1} days from 2 to 11");
}

#[test]
fn growth_stops_at_k() {
    let d = grow_unloaded(3, 2, 4, 5_000);
    assert_eq!(d.len(), 2, "two beds free, two villagers come");
    let (mut w, mut nodes) = world(3);
    census(&mut w, &mut nodes, HIGHMOOR, 4, 4, day(1));
    assert_eq!(w.village_row(HIGHMOOR).unwrap().breed_due, u64::MAX, "a full village books nothing");
    assert!(run(&mut w, day(2_000), HIGHMOOR).residents.is_empty());
}

#[test]
fn a_loaded_village_does_not_grow_in_the_sim() {
    let (mut w, mut nodes) = world(5);
    // A census every 6,000 ticks, the link's heartbeat, for 800 days: vanilla's rules are what
    // runs there, so the sim breeds nobody.
    let end = day(800).0;
    let mut t = day(1).0;
    while t < end {
        census(&mut w, &mut nodes, HIGHMOOR, 2, 12, Tick(t));
        assert!(run(&mut w, Tick(t), HIGHMOOR).residents.is_empty(), "no growth at tick {t}");
        t += 6_000;
    }
    assert_eq!(w.village_members(HIGHMOOR), 2);
    // The heartbeat stops (nobody is near any more): it grows again.
    let got = run(&mut w, Tick(end + 3_000 * TICKS_PER_DAY), HIGHMOOR);
    assert!(!got.residents.is_empty(), "an unloaded village grows");
}

#[test]
fn growth_is_not_slowed_by_pressure() {
    let times = |kills: u32| {
        let (mut w, mut nodes) = world(9);
        census(&mut w, &mut nodes, HIGHMOOR, 12, 12, day(1));
        let key = w.nodes[HIGHMOOR].key;
        for _ in 0..kills {
            apply(&mut w, &mut nodes, &Input::VillageReport { key, player: PLAYER, kind: 8 }, day(1).0);
        }
        census(&mut w, &mut nodes, HIGHMOOR, 2, 12, day(1));
        let pressure = w.village_pressure(HIGHMOOR);
        let mut at = Vec::new();
        for _ in 0..4 {
            let due = w.village_row(HIGHMOOR).unwrap().breed_due;
            assert_ne!(due, u64::MAX, "a growth step is booked");
            at.push(due);
            let got = run(&mut w, Tick(due), HIGHMOOR);
            result(&mut w, &mut nodes, got.residents[0].op, true, Tick(due));
        }
        (at, pressure)
    };
    let (calm, p0) = times(0);
    let (farmed, p1) = times(10);
    assert_eq!(p0, 0.0);
    assert!(p1 > 0.5, "the kills left pressure: {p1}");
    assert_eq!(calm, farmed, "and growth ran on the same steps");
}

// ---- EQC165 to EQC167: a Ribbit village refills before it builds ---------------------------------

/// Reedholm seen alive at 10 and cut to `left` residents.
fn cut_to(w: &mut World, nodes: &mut Nodes, left: u32) {
    census(w, nodes, REEDHOLM, 10, 0, day(1));
    census(w, nodes, REEDHOLM, left, 0, day(2));
}

#[test]
fn ribbit_refill_fills_before_it_builds() {
    let (mut w, mut nodes) = ribbit_world(11, true);
    cut_to(&mut w, &mut nodes, 5);
    assert_eq!(w.village_members(REEDHOLM), 5);
    assert_eq!(w.village_row(REEDHOLM).unwrap().k, 10, "K is the village's peak");
    let mut refills = 0;
    let mut t = day(2);
    while refills < 5 {
        t = Tick(t.0 + 3 * TICKS_PER_DAY + 1_000);
        let got = run(&mut w, t, REEDHOLM);
        assert!(got.houses.is_empty(), "no house before the empty ones are filled");
        for r in &got.residents {
            assert_eq!(r.arrive, Arrive::Refill);
            assert_eq!(r.body, Body::RibbitResident);
            result(&mut w, &mut nodes, r.op, true, t);
            refills += 1;
        }
        assert!(t.0 < day(400).0, "the refills stalled at {refills}");
    }
    assert_eq!(w.village_members(REEDHOLM), 10);
    assert_eq!(w.stats.refills, 5);
    assert_eq!(w.nodes[REEDHOLM].local.pop, 10, "the market's pop follows the residents");
    // Only now does it build.
    let house = first_house(&mut w, t.0 / TICKS_PER_DAY + 1);
    assert_eq!(house.village, w.nodes[REEDHOLM].key, "a full village builds");
}

#[test]
fn a_full_ribbit_village_still_builds() {
    let (mut w, mut nodes) = ribbit_world(13, false);
    census(&mut w, &mut nodes, REEDHOLM, 10, 0, day(1));
    let house = first_house(&mut w, 1);
    assert_eq!(house.village, w.nodes[REEDHOLM].key);
    assert_eq!(w.stats.refills, 0, "no refill: nothing stands empty");
}

#[test]
fn pop_is_booked_on_spawn_not_on_order() {
    let (mut w, mut nodes) = ribbit_world(13, false);
    census(&mut w, &mut nodes, REEDHOLM, 10, 0, day(1));
    let house = first_house(&mut w, 1).op;
    let at = w.now();
    let r = *w.village_row(REEDHOLM).unwrap();
    assert_eq!(r.members, 10, "an ordered house adds no resident");
    assert_eq!(w.nodes[REEDHOLM].local.pop, 10);
    assert_eq!((r.pending_houses, r.k), (1, 11), "it adds a bed the village will have");
    // The bridge placed the house: its resident is ordered, still not counted.
    let out = apply(&mut w, &mut nodes, &Input::HousePlaced { op: house }, at.0);
    assert!(!out.refused);
    let got = run(&mut w, at, REEDHOLM);
    assert_eq!(got.residents.len(), 1);
    assert_eq!(got.residents[0].arrive, Arrive::House);
    assert_eq!(w.village_members(REEDHOLM), 10);
    // And the resident exists.
    result(&mut w, &mut nodes, got.residents[0].op, true, at);
    let r = *w.village_row(REEDHOLM).unwrap();
    assert_eq!((r.members, r.k, r.pending_houses, r.owed), (11, 11, 0, 0));
    assert_eq!(w.nodes[REEDHOLM].local.pop, 11);
}

#[test]
fn a_failed_house_gives_its_booking_back() {
    let (mut w, mut nodes) = ribbit_world(13, false);
    census(&mut w, &mut nodes, REEDHOLM, 10, 0, day(1));
    let house = first_house(&mut w, 1).op;
    let at = w.now();
    result(&mut w, &mut nodes, house, false, at);
    let r = *w.village_row(REEDHOLM).unwrap();
    assert_eq!((r.members, r.k, r.pending_houses, r.owed), (10, 10, 0, 0));
    assert_eq!(w.stats.lapsed, 1);
    assert_eq!(w.growth[&REEDHOLM].houses, 1, "the house stays booked: the clock counts it");
}

#[test]
fn ribbit_settlers_below_two() {
    // Nobody left: a pair walks in after the wait, counted when they exist.
    let (mut w, mut nodes) = ribbit_world(17, true);
    cut_to(&mut w, &mut nodes, 0);
    assert_eq!(w.nodes[REEDHOLM].local.pop, 1, "the market keeps its floor");
    let early = run(&mut w, Tick(day(2).0 + 15 * TICKS_PER_DAY), REEDHOLM);
    assert!(early.residents.is_empty() && early.houses.is_empty(), "nobody builds in an empty village");
    let got = run(&mut w, Tick(day(2).0 + 17 * TICKS_PER_DAY), REEDHOLM);
    assert_eq!(got.residents.len(), 2);
    assert!(got.residents.iter().all(|r| matches!(r.arrive, Arrive::Settler { .. })));
    assert!(got.residents.iter().all(|r| r.body == Body::RibbitResident));
    assert_eq!(w.village_members(REEDHOLM), 0, "a Ribbit village counts them when they exist");
    for r in &got.residents {
        result(&mut w, &mut nodes, r.op, true, day(20));
    }
    assert_eq!(w.village_members(REEDHOLM), 2);
    // One left: the refill, at the growth clock's 3 days, is a booking too, so no pair is sent.
    let (mut w, mut nodes) = ribbit_world(17, true);
    cut_to(&mut w, &mut nodes, 1);
    let got = run(&mut w, Tick(day(2).0 + 17 * TICKS_PER_DAY), REEDHOLM);
    assert!(got.residents.iter().all(|r| r.arrive == Arrive::Refill), "{:?}", got.residents.len());
    assert!(!got.residents.is_empty());
}
