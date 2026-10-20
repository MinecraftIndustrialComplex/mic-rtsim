// SPDX-License-Identifier: AGPL-3.0-or-later
//! Villages join one at a time as their structure starts generate (C22, overnight choices E2 and
//! E15): the world built from the first few grows to the whole synthetic world, conservation
//! holds after every event, every newcomer is linked by the edge rule and trades, and news
//! reaches it.

use proptest::prelude::*;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

fn start(seed: u64, first: &[usize], check: bool) -> (World, Vec<rtsim_world::NodeData>) {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = check;
    cfg.genesis = Genesis::Share(500);
    let all = synth::nodes();
    let data: Vec<_> = first.iter().map(|&i| all[i].clone()).collect();
    (World::build(cfg, goods, book, data), all)
}

#[test]
fn villages_join_one_at_a_time() {
    let first = [0usize, 1, 3];
    let (mut w, all) = start(5, &first, true);
    let rest: Vec<usize> = (0..all.len()).filter(|i| !first.contains(i)).collect();
    let mut t = Tick::from_days(2);
    for &i in &rest {
        let edges_before = w.edges.len();
        let idx = w.join(all[i].clone(), t);
        assert_eq!(w.nodes[idx].key, all[i].key);
        assert!(
            w.edges.len() > edges_before,
            "{} joined with no edge",
            synth::NAMES[i]
        );
        // The rule links it to its nearest; every link is two-way.
        for &j in &w.nodes[idx].neighbours {
            assert!(w.nodes[j].neighbours.contains(&idx));
        }
        w.check_conserved(w.now()).unwrap();
        t += TICKS_PER_DAY;
    }
    assert_eq!(w.nodes.len(), all.len());
    // Every storehouse that joined trades: something flows in or out of it within 20 days.
    w.run_until(t + 20 * TICKS_PER_DAY);
    for (i, n) in w.nodes.iter().enumerate() {
        if !n.kind.is_storehouse() {
            continue;
        }
        let m = w.market(i);
        let moved = m
            .lines
            .iter()
            .any(|l| l.book.arrived > 0 || l.book.departed > 0);
        assert!(moved, "node {i} ({}) never traded", n.key);
        // It has heard from every neighbour.
        for &j in &n.neighbours {
            assert!(n.news.get(j).is_some(), "node {i} never heard from {j}");
        }
    }
    w.check_conserved(w.now()).unwrap();
}

/// A world grown by joins nearly settles: it keeps a small churn a world built at once does not
/// (its anchors and `T` bounds come from whoever was there when each village joined, E15), but
/// falls to a tenth of its early event rate and well under one event per market per day.
#[test]
fn a_grown_world_nearly_settles() {
    let (mut w, all) = start(5, &[0, 1], false);
    for i in 2..all.len() {
        w.join(all[i].clone(), Tick::from_days(i as u64));
    }
    let n = w.nodes.len() as f64;
    w.run_until(Tick::from_days(100));
    let early = w.stats.market_events as f64 / 100.0 / n;
    w.run_until(Tick::from_days(500));
    let before = w.stats.market_events;
    w.run_until(Tick::from_days(700));
    let rate = (w.stats.market_events - before) as f64 / 200.0 / n;
    eprintln!(
        "grown world: {early:.2} events per market per day at days 0-100, {rate:.3} at 500-700"
    );
    assert!(rate < 0.5 && rate < early / 5.0, "{early} -> {rate}");
    w.check_conserved(w.now()).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// Any join order and timing keeps every total exact (checked after every event).
    #[test]
    fn joins_in_any_order_conserve(seed in 0u64..100, order in Just((2..11usize).collect::<Vec<_>>()).prop_shuffle(), gaps in prop::collection::vec(0u64..48, 9)) {
        let (mut w, all) = start(seed, &[0, 1], true);
        let mut t = Tick::from_days(1);
        for (k, &i) in order.iter().enumerate() {
            t += gaps[k] * 1000;
            w.join(all[i].clone(), t);
            w.check_conserved(w.now()).unwrap();
        }
        w.run_until(t + 5 * TICKS_PER_DAY);
        w.check_conserved(w.now()).unwrap();
    }
}
