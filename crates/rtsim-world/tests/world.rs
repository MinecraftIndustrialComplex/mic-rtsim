// SPDX-License-Identifier: AGPL-3.0-or-later
//! World-level tests of phase 1's rules: conservation after every event under any script, news
//! carried only by travellers, determinism, and what grudges do to prices.

use proptest::prelude::*;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::synth::{self, SynthGoods};
use rtsim_world::{CaravanOutcome, Side, World, WorldConfig};

fn world(seed: u64, check: bool) -> (World, SynthGoods) {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = check;
    cfg.genesis = Genesis::Share(500);
    synth::build(cfg, true)
}

/// Runs the world an hour at a time from day 6 until a loaded caravan is on a road out of `from`
/// (caravans wait at the dock until full or for the longest dock wait, so a road can be empty
/// for a while).
fn first_caravan(w: &mut World, from: usize) -> rtsim_travel::CaravanKey {
    let mut t = Tick::from_days(6);
    loop {
        w.run_until(t);
        // Out of sight of both villages: past a fifth of the way.
        if let Some(k) = busiest(w, from)
            && w.travellers(k.edge, k.dir)
                .iter()
                .any(|x| x.key == k && x.progress_milli > 200)
        {
            return k;
        }
        t += 1000;
        assert!(
            t < Tick::from_days(30),
            "no caravan left node {from} by day 30"
        );
    }
}

/// The traveller with the most goods on any road out of `from`, if any.
fn busiest(w: &World, from: usize) -> Option<rtsim_travel::CaravanKey> {
    let mut best: Option<(i64, rtsim_travel::CaravanKey)> = None;
    for d in &w.nodes[from].out {
        for t in w.travellers(d.edge, d.dir) {
            let load: i64 = t.cargo.values().sum();
            if load > 0 && best.is_none_or(|b| load > b.0) && t.progress_milli < 600 {
                best = Some((load, t.key));
            }
        }
    }
    best.map(|b| b.1)
}

#[derive(Clone, Debug)]
enum Op {
    Trade {
        node: usize,
        good: usize,
        items: i64,
        buy: bool,
    },
    Rob {
        node: usize,
        share: i64,
    },
    Rescue {
        node: usize,
    },
    TakeLlama {
        node: usize,
    },
    KillLeader {
        node: usize,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0usize..10, 0usize..7, 1i64..80, any::<bool>()).prop_map(|(node, good, items, buy)| {
            Op::Trade {
                node,
                good,
                items,
                buy,
            }
        }),
        (0usize..10, 0i64..=1000).prop_map(|(node, share)| Op::Rob { node, share }),
        (0usize..10).prop_map(|node| Op::Rescue { node }),
        (0usize..10).prop_map(|node| Op::TakeLlama { node }),
        (0usize..10).prop_map(|node| Op::KillLeader { node }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]

    /// Conservation holds after every event (the world checks itself after each one and panics on
    /// a break), whatever a player does: trades of any size, robberies, rescues, stolen llamas,
    /// killed traders, at any time.
    #[test]
    fn conservation_holds_after_every_event(seed in 0u64..1000, ops in prop::collection::vec((1u64..40, op()), 1..12)) {
        let (mut w, ids) = world(seed, true);
        let player = w.player(7);
        let goods = [ids.wheat, ids.bread, ids.leather, ids.iron, ids.wool, ids.slime, ids.clay];
        let mut t = Tick::from_days(2);
        for (gap_quanta, o) in ops {
            t += gap_quanta * 1000;
            w.run_until(t);
            match o {
                Op::Trade { node, good, items, buy } => {
                    let _ = w.trade(node, player, goods[good], items, if buy { Side::Buy } else { Side::Sell }, t);
                }
                Op::Rob { node, share } => {
                    if let Some(k) = busiest(&w, node) && let Some(c) = w.promote(k) {
                        w.rob(c, player, share);
                    }
                }
                Op::Rescue { node } => {
                    if let Some(k) = busiest(&w, node) && let Some(c) = w.promote(k) {
                        w.rescue(c, player);
                    }
                }
                Op::TakeLlama { node } => {
                    if let Some(k) = busiest(&w, node) && let Some(c) = w.promote(k) {
                        let m = w.caravans.get(c).unwrap().members.len() - 1;
                        w.take_member(c, m, player);
                    }
                }
                Op::KillLeader { node } => {
                    if let Some(k) = busiest(&w, node) && let Some(c) = w.promote(k) {
                        w.kill_member(c, 0, player);
                    }
                }
            }
            w.check_conserved(w.now()).unwrap();
        }
        w.run_until(t + 3 * TICKS_PER_DAY);
        w.check_conserved(w.now()).unwrap();
    }
}

/// A robbery reaches a second village only after a traveller carries it there (PLAN.md §8.10
/// exit test 4, headless): the robbed caravan carries it to where it walks next, and from there
/// it moves only in packets, each delivered when a caravan arrives.
#[test]
fn a_robbery_travels_only_with_travellers() {
    let (mut w, _) = world(11, false);
    let player = w.player(7);
    // A caravan out of Oakford (node 1).
    let key = first_caravan(&mut w, 1);
    let c = w.promote(key).expect("on the road");
    let attack = w.rob(c, player, 1000).expect("robbed");
    let report = attack.report;
    let car = w.caravans.get(c).unwrap().clone();
    let robbed_at = w.now();
    // Nobody but the caravan knows: the road is out of sight of every village.
    for (i, n) in w.nodes.iter().enumerate() {
        assert!(!w.knowledge.knows(n.actor, report), "node {i} knew at once");
    }
    assert!(w.knowledge.knows(car.holder, report));
    // Where the caravan walks next learns it exactly when the caravan arrives, not before.
    let first = match attack.outcome {
        CaravanOutcome::TurnedBack => car.home,
        _ => car.leg.to,
    };
    let arrive = w.caravans.get(c).unwrap().leg.end;
    w.run_until(Tick(arrive.0 - 1));
    assert!(!w.knowledge.knows(w.nodes[first].actor, report));
    w.run_until(arrive);
    assert!(w.knowledge.knows(w.nodes[first].actor, report));
    // Every other village learns it only after at least one more leg of travel from a village
    // that knew, and each time a packet is delivered to it.
    let q = w.cfg.trade.quantum.len();
    let mut learned: Vec<Option<Tick>> = vec![None; w.nodes.len()];
    learned[first] = Some(arrive);
    let mut t = arrive.0;
    while t < robbed_at.0 + 6 * TICKS_PER_DAY {
        t += q;
        let deliveries_before = w.stats.deliveries;
        w.run_until(Tick(t));
        for (i, n) in w.nodes.iter().enumerate() {
            if learned[i].is_none() && w.knowledge.knows(n.actor, report) {
                assert!(
                    w.stats.deliveries > deliveries_before || w.stats.caravan_arrivals > 0,
                    "node {i} learned with no traveller arriving"
                );
                // At least the shortest edge's travel time after the first village knew.
                let shortest = w.edges.iter().map(|e| e.d).min().unwrap() * q;
                assert!(
                    t >= arrive.0 + shortest,
                    "node {i} learned faster than any caravan walks"
                );
                learned[i] = Some(Tick(t));
            }
        }
    }
    let spread = learned.iter().filter(|x| x.is_some()).count();
    assert!(
        spread >= 3,
        "the news should have spread along the roads: {learned:?}"
    );
    // Nodes that learned did so in the order of the roads: never before a neighbour that knew.
    for (i, l) in learned.iter().enumerate() {
        if let Some(ti) = l
            && i != first
        {
            let via = w.nodes[i]
                .neighbours
                .iter()
                .filter_map(|&j| learned[j])
                .min();
            assert!(
                via.is_some_and(|tj| tj < *ti),
                "node {i} learned before any neighbour did"
            );
        }
    }
}

/// The same seed and the same script give the same state hash; reads in between change nothing;
/// another seed gives another world.
#[test]
fn determinism() {
    let run = |seed: u64, reads: bool| {
        let (mut w, ids) = world(seed, false);
        let player = w.player(3);
        for day in 1..=20u64 {
            w.run_until(Tick::from_days(day));
            if reads {
                for i in 0..w.nodes.len() {
                    let _ = w.market(i);
                    let _ = w.offers(i, player);
                    let _ = w.quote(i, player, ids.bread, 3, Side::Buy);
                }
            }
            if day == 8 {
                let _ = w.trade(0, player, ids.wheat, 64, Side::Sell, w.now());
            }
            if day == 12
                && let Some(k) = busiest(&w, 1)
                && let Some(c) = w.promote(k)
            {
                w.rob(c, player, 500);
            }
        }
        w.state_hash()
    };
    let a = run(5, false);
    assert_eq!(a, run(5, false));
    assert_eq!(a, run(5, true));
    assert_ne!(a, run(6, false));
}

/// A grudge raises a ribbit village's quotes for the player who earned it (PLAN.md §8.5, §8.7:
/// ribbits have no gossip, so prices rise through the sim's own quote), and never lowers them for
/// anyone.
#[test]
fn ribbit_grudges_raise_prices() {
    let (mut w, ids) = world(4, false);
    let villain = w.player(1);
    let stranger = w.player(2);
    w.run_until(Tick::from_days(3));
    let mirewater = 6;
    let fair = w
        .quote(mirewater, stranger, ids.bread, 2, Side::Buy)
        .expect("bread for sale");
    let actor = w.nodes[mirewater].actor;
    w.knowledge.adjust(actor, villain, -120, w.now());
    let dear = w
        .quote(mirewater, villain, ids.bread, 2, Side::Buy)
        .expect("still for sale");
    assert!(dear.coins > fair.coins, "{dear:?} vs {fair:?}");
    w.knowledge.adjust(actor, stranger, 120, w.now());
    assert_eq!(
        w.quote(mirewater, stranger, ids.bread, 2, Side::Buy)
            .unwrap(),
        fair
    );
    // A vanilla village leaves reputation to vanilla gossip (C20): its quote stays fair.
    let brindle = 0;
    let b_fair = w.quote(brindle, stranger, ids.bread, 2, Side::Buy).unwrap();
    w.knowledge
        .adjust(w.nodes[brindle].actor, villain, -120, w.now());
    assert_eq!(
        w.quote(brindle, villain, ids.bread, 2, Side::Buy).unwrap(),
        b_fair
    );
}

/// A trader killed on the road leaves its grudge with its home market (C24: heirs inherit
/// grudges), and robbing a caravan writes vanilla gossip only where the news lands in a vanilla
/// village (C20).
#[test]
fn heirs_and_vanilla_gossip() {
    let (mut w, _) = world(9, false);
    let player = w.player(1);
    let key = first_caravan(&mut w, 1);
    let c = w.promote(key).unwrap();
    let home = w.nodes[w.caravans.get(c).unwrap().home].actor;
    let before = w.knowledge.sentiment(home, player, w.now());
    let attack = w.kill_member(c, 0, player).unwrap();
    assert_eq!(attack.outcome, CaravanOutcome::Dissolved);
    let after = w.knowledge.sentiment(home, player, w.now());
    assert!(
        after < before,
        "home inherited no grudge: {before} -> {after}"
    );
    w.run_until(Tick::from_days(9));
    let cmds = w.take_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, rtsim_world::Command::VanillaGossip(g) if g.player == player))
    );
    w.check_conserved(w.now()).unwrap();
}
