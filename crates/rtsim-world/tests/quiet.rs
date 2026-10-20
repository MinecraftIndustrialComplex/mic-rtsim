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
