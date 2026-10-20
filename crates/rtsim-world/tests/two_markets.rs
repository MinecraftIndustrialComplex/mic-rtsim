// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two markets and one good (economy research failure table: "Cobweb from news lag | Two markets,
//! one good: amplitude decays; event rate reaches zero in a closed world (quiescence)").
//!
//! A farms wheat; B cannot and must import it, walking 6 quanta. B mints its own emeralds, so its
//! treasury sits at target and pays for the wheat; A's treasury target is set far away so it never
//! refuses cash inside the test (a one-good world has nothing for A to spend its money on). B starts
//! empty-handed relative to its need, so the first fill overshoots: news is stale by the time it
//! lands. Each later swing must be smaller, and the world must then stop generating events.

use std::collections::BTreeMap;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_trade::labour::{Recipe, RecipeBook, ResourceId};
use rtsim_trade::{GoodDef, GoodKind, Goods, MILLI};
use rtsim_world::{NodeData, World, WorldConfig};

fn world(seed: u64) -> (World, rtsim_trade::GoodId) {
    let mut goods = Goods::new();
    let wheat = goods.add(GoodDef {
        name: "wheat".into(),
        kind: GoodKind::Item,
        need_per_pop_day: 1_000,
        stack: 64,
    });
    let emerald = goods.add(GoodDef {
        name: "emerald".into(),
        kind: GoodKind::Currency(Culture::Human),
        need_per_pop_day: 0,
        stack: 64,
    });
    let (farm, ore) = (ResourceId(0), ResourceId(1));
    let book = RecipeBook {
        recipes: vec![
            Recipe {
                name: "farm".into(),
                output: wheat,
                yield_per_batch: 10 * MILLI,
                inputs: vec![],
                labour: 1.0,
                resource: Some(farm),
            },
            Recipe {
                name: "mint".into(),
                output: emerald,
                yield_per_batch: MILLI,
                inputs: vec![],
                labour: 3.0,
                resource: Some(ore),
            },
        ],
    };
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(250);
    // A's cash target far away, so a one-good world never makes A refuse cash in the test.
    cfg.trade.k_days = 10_000;
    let node = |key, x, pop, res: &[(ResourceId, i64)]| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop,
        resources: res
            .iter()
            .map(|&(r, a)| (r, a * MILLI))
            .collect::<BTreeMap<_, _>>(),
    };
    let w = World::build(
        cfg,
        goods,
        book,
        vec![
            node(1, 0, 10, &[(farm, 40)]),
            node(2, 600, 10, &[(ore, 20)]),
        ],
    );
    (w, wheat)
}

#[test]
fn overshoot_decays_and_the_world_goes_quiet() {
    let (mut w, wheat) = world(7);
    let target = w.market(1).line(wheat).target;
    assert!(target > 0);
    // Sample B's stock every quantum for 60 days.
    let q = w.cfg.trade.quantum.len();
    let mut samples = Vec::new();
    let mut t = 0;
    while t <= 60 * TICKS_PER_DAY {
        w.run_until(Tick(t));
        samples.push(w.market(1).stock(wheat));
        t += q;
    }
    // Swings above the target: the peak of each excursion.
    let mut peaks = Vec::new();
    let mut cur = 0;
    for &s in &samples {
        if s > target {
            cur = cur.max(s - target);
        } else if cur > 0 {
            peaks.push(cur);
            cur = 0;
        }
    }
    if cur > 0 {
        peaks.push(cur);
    }
    eprintln!(
        "target {target}, overshoot peaks {peaks:?}, stats {:?}",
        w.stats
    );
    assert!(!peaks.is_empty(), "the first fill overshoots (news lag)");
    for p in peaks.windows(2) {
        assert!(p[1] <= p[0], "overshoot grew: {peaks:?}");
    }
    // The overshoot stays inside the band the stability rule reserves for it.
    assert!(
        peaks[0] <= target,
        "first overshoot {} beyond the band",
        peaks[0]
    );
    // It settles inside the news dead band around the target.
    let end = w.market(1).stock(wheat);
    assert!(
        (end - target).abs() <= target / 8,
        "settled at {end}, target {target}"
    );
    // Quiet: no market event at all over the last 20 days.
    let before = w.stats.market_events;
    w.run_until(Tick(80 * TICKS_PER_DAY));
    assert_eq!(
        w.stats.market_events, before,
        "a closed world kept generating events"
    );
    w.check_conserved(w.now()).unwrap();
}
