// SPDX-License-Identifier: AGPL-3.0-or-later
//! Quiescence (economy research failure table: "event rate reaches zero in a closed world";
//! overnight choices E43 to E52). A closed world with no player settles: after a transient, no
//! market event happens at all, and conservation holds throughout.

use rtsim_kernel::clock::Tick;
use rtsim_trade::Rules;
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

fn scenario(rules: Rules, genesis: i64) -> World {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(genesis);
    cfg.trade.rules = rules;
    synth::build(cfg, true).0
}

/// Market events per market per day over `(from, to]` days.
fn rate(w: &mut World, from: u64, to: u64) -> f64 {
    w.run_until(Tick::from_days(from));
    let before = w.stats.market_events;
    w.run_until(Tick::from_days(to));
    (w.stats.market_events - before) as f64 / (to - from) as f64 / w.nodes.len() as f64
}

/// The phase 1 scenario's eleven nodes, no player: busy while stocks fill and money finds its
/// level, then silent. Conservation is checked after every event for the first 60 days.
#[test]
fn the_closed_scenario_world_goes_quiet() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(500);
    let (mut w, _) = synth::build(cfg, true);
    let early = rate(&mut w, 0, 60);
    w.cfg.check_every_event = false;
    let quiet = rate(&mut w, 500, 700);
    eprintln!("events per market per day: days 0-60 {early:.2}, days 500-700 {quiet:.3}");
    assert!(early > 1.0, "the transient should be busy: {early}");
    assert_eq!(quiet, 0.0, "a closed world kept generating events");
    w.check_conserved(w.now()).unwrap();
}

/// Round 1's rules (no sinks, no bands, exports only at the target, barter in flows) on the same
/// world never settle: the comparison the choices file records.
#[test]
fn round_one_rules_never_settle() {
    let mut r = Rules::ROUND1;
    r.barter_in_flows = true;
    let mut w = scenario(r, 500);
    let busy = rate(&mut w, 200, 240);
    eprintln!("round 1 rules: {busy:.2} events per market per day at days 200-240");
    assert!(
        busy > 10.0,
        "round 1's rules were expected to stay busy: {busy}"
    );
    w.check_conserved(w.now()).unwrap();
}

/// A grid of sixteen villages with random resources (the bench's world) falls to a small
/// fraction of an event per market per day.
#[test]
fn a_grid_of_villages_nearly_settles() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut w = World::build(cfg, goods, book, synth::grid(16, 42));
    let early = rate(&mut w, 0, 100);
    let late = rate(&mut w, 400, 600);
    eprintln!("grid of 16: days 0-100 {early:.2}, days 400-600 {late:.3}");
    assert!(late < early / 10.0, "{early} -> {late}");
    assert!(late < 0.2, "{late}");
    w.check_conserved(w.now()).unwrap();
}

/// Villages renew (EQUILIBRIUM.md S6, EQC173): a grid of villages the census has seen alive, a few
/// of them wiped by a player at day 200 and the bridge answering every order at once, against the
/// same grid with no wipe. Settlers, growth and refills run for years of game time; at the end the
/// wiped world is no busier than the control (which, as `a_grid_of_villages_nearly_settles` says
/// of the grid, is not silent either), conserved after every window.
#[test]
fn sim_quiet_stays_quiet_with_settlers() {
    use rtsim_world::Command;
    use rtsim_world::inputs::{Input, apply};
    use std::collections::BTreeMap;

    /// Market events per market per day in each 100-day window of 1,800 days, and the renewals
    /// the run booked.
    fn windows(wipe: bool) -> (Vec<f64>, u64) {
        let mut cfg = WorldConfig::new(42);
        cfg.check_every_event = false;
        cfg.genesis = Genesis::AtTarget;
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        let mut w = World::build(cfg, goods, book, synth::grid(36, 42));
        let mut nodes: BTreeMap<u64, usize> =
            (0..w.nodes.len()).map(|i| (w.nodes[i].key, i)).collect();
        let ribbit = |w: &World, i: usize| {
            w.nodes[i].kind.culture() == Some(rtsim_settle::Culture::Ribbit)
        };
        // The census of every village at day 1: every village is seen alive.
        w.run_until(Tick::from_days(1));
        let mut real: BTreeMap<usize, u32> = BTreeMap::new();
        for i in 0..w.nodes.len() {
            let pop = w.nodes[i].local.pop;
            real.insert(i, pop);
            let beds = if ribbit(&w, i) { 0 } else { pop };
            w.census_village(i, pop, beds, true, w.now());
        }
        let wiped = [0usize, 6, 14, 25];
        assert!(wiped.iter().any(|&i| ribbit(&w, i)) && wiped.iter().any(|&i| !ribbit(&w, i)));
        let mut out = Vec::new();
        let mut before = w.stats.market_events;
        for day in 2..=1_800u64 {
            w.run_until(Tick::from_days(day));
            if wipe && day == 200 {
                for &i in &wiped {
                    let key = w.nodes[i].key;
                    let now = w.now().0;
                    for _ in 0..real[&i] {
                        let report = Input::VillageReport { key, player: (1, 2), kind: 8 };
                        apply(&mut w, &mut nodes, &report, now);
                    }
                    real.insert(i, 0);
                    let beds = if ribbit(&w, i) { 0 } else { w.nodes[i].local.pop.max(8) };
                    for _ in 0..2 {
                        w.census_village(i, 0, beds, true, w.now());
                    }
                }
            }
            // The bridge: every house stands and every resident exists the day it is ordered.
            for _ in 0..4 {
                let cmds = w.take_commands();
                if cmds.is_empty() {
                    break;
                }
                for c in cmds {
                    match c {
                        Command::PlaceHouse(h) => {
                            w.house_placed(h.op);
                        }
                        Command::SpawnResident(r) => {
                            let now = w.now();
                            w.resident_result(r.op, true, now);
                            *real.get_mut(&w.node_of_key(r.village).unwrap()).unwrap() += 1;
                        }
                        _ => {}
                    }
                }
            }
            if day % 100 == 0 {
                w.check_conserved(w.now()).unwrap();
                let events = w.stats.market_events;
                out.push((events - before) as f64 / 100.0 / w.nodes.len() as f64);
                before = events;
            }
        }
        (out, w.stats.settlers.iter().sum::<u64>() + w.stats.grown + w.stats.refills)
    }

    let (control, renewals0) = windows(false);
    let (wiped, renewals) = windows(true);
    eprintln!("control {control:.3?}\nwiped   {wiped:.3?}");
    assert!(renewals >= renewals0 + 4, "the wipe was renewed: {renewals0} -> {renewals}");
    let late = |w: &[f64]| w[w.len() - 3..].iter().sum::<f64>() / 3.0;
    assert!(late(&wiped) < 0.3, "{:.3}", late(&wiped));
    assert!(
        late(&wiped) < late(&control) + 0.1,
        "the renewed world is no busier than the control: {:.3} against {:.3}",
        late(&wiped),
        late(&control)
    );
}
