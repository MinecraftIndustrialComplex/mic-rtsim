// SPDX-License-Identifier: AGPL-3.0-or-later
//! A predator's raid on a village's stock taken in one pass (MOWZIE.md ledger 10, choice MZB-55),
//! and a lair joining the world as no news (ledger 13, MZB-59).

use std::collections::BTreeMap;

use rtsim_archive::kinds;
use rtsim_archive::record::rows_on;
use rtsim_kernel::clock::Tick;
use rtsim_settle::{NodeKind, Pos};
use rtsim_trade::{Amount, GoodId};
use rtsim_world::{NodeData, World, WorldConfig, synth};

fn world() -> World {
    synth::build(WorldConfig::new(42), false).0
}

/// A village and two goods it holds, with half of each stock as the raid's want.
fn a_village_and_its_stock(w: &World) -> (usize, BTreeMap<GoodId, Amount>) {
    for i in 0..w.nodes.len() {
        let m = w.market(i);
        if !m.is_storehouse() {
            continue;
        }
        let held: Vec<GoodId> = w
            .goods
            .ids()
            .filter(|g| !w.goods.is_currency(*g) && m.stock(*g) > 1_000)
            .take(2)
            .collect();
        if held.len() == 2 {
            return (i, held.iter().map(|g| (*g, m.stock(*g) / 2)).collect());
        }
    }
    panic!("no village holds two goods");
}

/// `take_stocks` takes what one `take_stock` per good would, books it destroyed the same way and
/// conserves the world, with one market event where the goods one by one make one each.
#[test]
fn take_stocks_takes_what_take_stock_would_in_one_pass() {
    let at = Tick::from_days(2);
    let (mut a, mut b) = (world(), world());
    a.run_until(at);
    b.run_until(at);
    let (i, want) = a_village_and_its_stock(&a);
    let (ea, eb) = (a.stats.market_events, b.stats.market_events);
    let one_by_one: BTreeMap<GoodId, Amount> = want
        .iter()
        .map(|(&g, &n)| (g, a.take_stock(i, g, n, at)))
        .collect();
    let batched = b.take_stocks(i, &want, at);
    assert_eq!(one_by_one, want, "half of each stock is there to take");
    assert_eq!(batched, one_by_one);
    assert_eq!(a.stats.market_events - ea, 2);
    assert_eq!(b.stats.market_events - eb, 1, "one pass");
    for g in want.keys() {
        assert_eq!(a.market(i).stock(*g), b.market(i).stock(*g));
    }
    assert_eq!(a.destroyed_total(at), b.destroyed_total(at));
    b.check_conserved(at).expect("conserved");
    // Asking more than the stock takes the stock; nothing asked takes nothing and makes no news.
    let all: BTreeMap<GoodId, Amount> = want.keys().map(|g| (*g, Amount::MAX / 4)).collect();
    let rest = b.take_stocks(i, &all, at);
    for g in want.keys() {
        assert_eq!(b.market(i).stock(*g), 0);
        assert!(rest[g] > 0);
    }
    let e = b.stats.market_events;
    let none: BTreeMap<GoodId, Amount> = want.keys().map(|g| (*g, 0)).collect();
    assert!(b.take_stocks(i, &none, at).is_empty());
    assert_eq!(b.stats.market_events, e);
    b.check_conserved(at).expect("conserved");
}

/// A lair joining is not news: no SETTLE_JOINED, where a village joining has one.
#[test]
fn a_lair_joins_without_news() {
    let mut w = world();
    w.tape.set_on(true);
    let at = Tick::from_days(1);
    w.run_until(at);
    let node = |key: u64, kind: NodeKind, x: i32| NodeData {
        key,
        kind,
        pos: Pos::new(0, x, 900),
        pop: 1,
        resources: BTreeMap::new(),
    };
    let lair = w.join(node(77, NodeKind::Lair, 600), at);
    assert_eq!(w.nodes[lair].kind, NodeKind::Lair);
    let village = w.join(
        node(
            78,
            NodeKind::Village(rtsim_settle::Culture::Human),
            1_200,
        ),
        at,
    );
    let joined: Vec<u64> = rows_on(&w.tape)
        .into_iter()
        .filter(|r| r.0 == kinds::SETTLE_JOINED)
        .filter_map(|r| match r.2 {
            rtsim_archive::Row::Event(e) => Some(e.subject.value),
            _ => None,
        })
        .collect();
    assert_eq!(joined, vec![w.nodes[village].key]);
}
