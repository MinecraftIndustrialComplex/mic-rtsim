// SPDX-License-Identifier: AGPL-3.0-or-later
//! A world lane can be closed and reopened (slice S8b-4a; EQUILIBRIUM.md §2.4.6a rule 7, choice
//! PC22): a closed ships' lane carries nothing new, what is already sailing lands, payments for it
//! still travel, and everything is conserved at every step.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_trade::labour::{Recipe, RecipeBook, ResourceId};
use rtsim_trade::{GoodDef, GoodId, GoodKind, Goods, MILLI};
use rtsim_travel::edge::Medium;
use rtsim_world::{LaneSpec, NodeData, World, WorldConfig};

/// The lane's edge index (edge 0 is the road the markets were built with).
const LANE: u32 = 1;

fn catalogue() -> (Goods, RecipeBook, GoodId) {
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
    (goods, book, wheat)
}

fn config(seed: u64) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(250);
    cfg.trade.k_days = 10_000;
    cfg
}

/// A farming village and a minting one, joined by one sea lane (edge 0) and nothing else.
fn lane_world(seed: u64) -> (World, GoodId) {
    lane_world_with(seed, |_| {})
}

/// [`lane_world`] with the configuration adjusted first.
fn lane_world_with(seed: u64, tweak: impl FnOnce(&mut WorldConfig)) -> (World, GoodId) {
    let (goods, book, wheat) = catalogue();
    let mut cfg = config(seed);
    tweak(&mut cfg);
    let (farm, ore) = (ResourceId(0), ResourceId(1));
    let node = |key, x, res: ResourceId, amount: i64| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop: 10,
        resources: BTreeMap::from([(res, amount * MILLI)]),
    };
    let mut w = World::build(
        cfg,
        goods,
        book,
        vec![node(1, 0, farm, 40), node(2, 600, ore, 20)],
    );
    // The markets are built with the road so that each has prices and anchors for what the other
    // makes (a pair built with no link at all has none and never trades). Then the road is
    // taken out of both villages' routes, so the lane added below is the only way across.
    assert_eq!(w.edges.len(), 1);
    for n in &mut w.nodes {
        n.out.retain(|d| d.edge != 0);
    }
    let lane = w.add_lane(
        LaneSpec {
            a: 0,
            b: 1,
            medium: Medium::Sea,
            length_blocks: 600,
            d_quanta: 6,
            dock_quanta: 4,
            cap_slot_milli: 4_000,
        },
        Tick::ZERO,
    );
    assert_eq!(lane, LANE);
    (w, wheat)
}

/// Goods (not coins) the lane carries now, and what is on it, in both directions.
fn lane_goods_rate(w: &World, wheat: GoodId) -> i64 {
    w.edges[LANE as usize]
        .dirs
        .iter()
        .map(|d| {
            d.goods.get(&wheat).map_or(0, |p| p.current_rate())
                + d.raid.get(&wheat).map_or(0, |p| p.current_rate())
        })
        .sum()
}

fn on_the_road(w: &World, at: Tick) -> BTreeMap<GoodId, i128> {
    let b = w.cfg.trade.quantum.index(at);
    let mut out = BTreeMap::new();
    for d in &w.edges[LANE as usize].dirs {
        d.in_transit(b, &mut out);
    }
    out.retain(|_, a| *a != 0);
    out
}

/// Runs one quantum at a time to `to`, checking conservation and the lane invariant at each.
fn run_checked(w: &mut World, to: Tick) {
    let q = w.cfg.trade.quantum.len();
    let mut t = w.now().0;
    while t < to.0 {
        t = (t + q).min(to.0);
        w.run_until(Tick(t));
        w.check_conserved(w.now())
            .unwrap_or_else(|e| panic!("not conserved at {t}: {e}"));
        w.check_lanes().unwrap();
    }
}

#[test]
fn a_closed_lane_carries_nothing_new_and_conserves() {
    let (mut w, wheat) = lane_world(7);
    let q = w.cfg.trade.quantum.len();
    let lag = w.edges[LANE as usize].lag();
    run_checked(&mut w, Tick::from_days(20));
    assert!(
        lane_goods_rate(&w, wheat) > 0,
        "wheat flows over the lane before the close"
    );
    let before = w.market(1).stock(wheat);

    // Close: from the next boundary the lane's goods rates are 0 and nothing new is sent.
    let t0 = w.now();
    assert!(w.close_lane(LANE, t0));
    assert!(w.lane_closed(LANE));
    assert_eq!(lane_goods_rate(&w, wheat), 0, "wound down at once");
    w.check_conserved(w.now()).unwrap();
    // Past the lag, everything sent before the close has landed, and payments for it have
    // settled one lag after that (they travel the closed lane).
    let mut paid = 0;
    let mut t = t0.0;
    while t < t0.0 + (2 * lag + 4) * q {
        t += q;
        run_checked(&mut w, Tick(t));
        paid = paid.max(
            w.edges[LANE as usize].dirs[1]
                .pay
                .values()
                .map(|p| p.current_rate())
                .max()
                .unwrap_or(0),
        );
    }
    assert!(
        paid > 0,
        "payments for goods already shipped travel the closed lane"
    );
    assert!(w.lane_closed(LANE));
    assert_eq!(
        lane_goods_rate(&w, wheat),
        0,
        "nothing new on a closed lane"
    );
    assert_eq!(
        on_the_road(&w, w.now()),
        BTreeMap::new(),
        "everything sent before the close arrived, and its payments with it"
    );
    // Run on: it stays shut, the importer's stock falls for want of supply.
    run_checked(&mut w, Tick(t0.0 + 20 * rtsim_kernel::clock::TICKS_PER_DAY));
    assert_eq!(lane_goods_rate(&w, wheat), 0);
    assert!(
        w.market(1).stock(wheat) < before,
        "the importer ran down its stock ({} of {before})",
        w.market(1).stock(wheat)
    );

    // Reopen: goods flow again.
    let t1 = w.now();
    assert!(w.open_lane(LANE, t1));
    assert!(!w.lane_closed(LANE));
    run_checked(&mut w, Tick(t1.0 + 10 * rtsim_kernel::clock::TICKS_PER_DAY));
    assert!(
        lane_goods_rate(&w, wheat) > 0 || !on_the_road(&w, w.now()).is_empty(),
        "wheat flows again after the lane reopens"
    );
}

#[test]
fn closing_a_lane_twice_or_opening_an_open_lane_changes_nothing() {
    let (mut w, _) = lane_world(7);
    run_checked(&mut w, Tick::from_days(10));
    // Opening an open lane: nothing changes (not even the hash).
    let t = w.now();
    let h = w.state_hash();
    assert!(w.open_lane(LANE, t));
    assert_eq!(w.state_hash(), h);
    assert!(!w.lane_closed(LANE));
    // Closing twice: the second does nothing.
    assert!(w.close_lane(LANE, t));
    let h = w.state_hash();
    let events = w.stats.market_events;
    assert!(w.close_lane(LANE, t));
    assert_eq!(w.state_hash(), h);
    assert_eq!(w.stats.market_events, events, "no market woke");
    // A missing edge is refused.
    assert!(!w.close_lane(9, t));
    assert!(!w.open_lane(9, t));
    assert!(!w.lane_closed(9));
    // Reopening twice, likewise.
    assert!(w.open_lane(LANE, t));
    let h = w.state_hash();
    assert!(w.open_lane(LANE, t));
    assert_eq!(w.state_hash(), h);
}

#[test]
fn a_road_cannot_be_closed() {
    let (goods, book, _) = catalogue();
    let (farm, ore) = (ResourceId(0), ResourceId(1));
    let node = |key, x, res: ResourceId| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop: 10,
        resources: BTreeMap::from([(res, 20 * MILLI)]),
    };
    let mut cfg = config(3);
    cfg.edge_rule = rtsim_settle::EdgeRule::P18;
    let mut w = World::build(cfg, goods, book, vec![node(1, 0, farm), node(2, 600, ore)]);
    assert_eq!(w.edges[0].medium, Medium::Road);
    let h = w.state_hash();
    assert!(!w.close_lane(0, Tick::ZERO));
    assert!(!w.lane_closed(0));
    assert_eq!(w.state_hash(), h);
}

#[test]
fn a_closed_lane_survives_save_and_restore() {
    let (mut a, wheat) = lane_world(11);
    run_checked(&mut a, Tick::from_days(15));
    let t = a.now();
    assert!(a.close_lane(LANE, t));
    // Saved while consignments are still sailing and payments are still due.
    let q = a.cfg.trade.quantum.len();
    run_checked(&mut a, Tick(t.0 + 3 * q));
    assert!(!on_the_road(&a, a.now()).is_empty());
    let bytes = a.save();
    let (goods, book, _) = catalogue();
    let mut b = World::load(config(11), goods, book, &bytes).expect("the snapshot reads back");
    assert!(b.lane_closed(LANE), "still closed after the round trip");
    assert_eq!(b.state_hash(), a.state_hash());
    assert_eq!(b.save(), bytes, "byte for byte");
    // The two continue alike, closed, then reopened.
    for day in 16..=22 {
        a.run_until(Tick::from_days(day));
        b.run_until(Tick::from_days(day));
        assert_eq!(a.state_hash(), b.state_hash(), "day {day}");
        assert_eq!(lane_goods_rate(&b, wheat), 0);
    }
    let t = a.now();
    assert!(a.open_lane(LANE, t));
    assert!(b.open_lane(LANE, t));
    a.run_until(Tick(t.0 + 5 * rtsim_kernel::clock::TICKS_PER_DAY));
    b.run_until(Tick(t.0 + 5 * rtsim_kernel::clock::TICKS_PER_DAY));
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(!b.lane_closed(LANE));
}

/// Pirates' share of a lane's flow is a share of what the lane carries, so it falls to 0 with it.
#[test]
fn a_closed_lane_takes_no_raids_either() {
    let (mut w, wheat) = lane_world(7);
    let q = w.cfg.trade.quantum.len();
    let lag = w.edges[LANE as usize].lag();
    // Raiders from the far village's market take a third of what leaves the farm; the haul is
    // booked as its own pipeline.
    assert!(w.set_lane_hazard(LANE, 0, 1, 333, 5, Tick::ZERO));
    run_checked(&mut w, Tick::from_days(8));
    let raid = |w: &World| {
        w.edges[LANE as usize].dirs[0]
            .raid
            .get(&wheat)
            .map_or(0, |p| p.current_rate())
    };
    assert!(
        raid(&w) > 0,
        "a raided share is on the lane before the close"
    );
    let t0 = w.now();
    assert!(w.close_lane(LANE, t0));
    assert_eq!(raid(&w), 0);
    assert_eq!(lane_goods_rate(&w, wheat), 0);
    run_checked(&mut w, Tick(t0.0 + (2 * lag + 4) * q));
    assert_eq!(raid(&w), 0);
    assert_eq!(
        on_the_road(&w, w.now()),
        BTreeMap::new(),
        "the haul landed too"
    );
    // The hazard stays set and takes effect again when the lane reopens.
    assert_eq!(w.lane_hazard(LANE, 0), Some((1, 333)));
    let t1 = w.now();
    assert!(w.open_lane(LANE, t1));
    run_checked(&mut w, Tick(t1.0 + 10 * rtsim_kernel::clock::TICKS_PER_DAY));
    assert!(raid(&w) > 0);
}

/// Every consignment sent before the close is paid for, to the thousandth: with exact payments
/// (no stickiness bands), the coins paid back over the closed lane equal the value of everything
/// that landed, including what was still sailing when the lane closed.
#[test]
fn every_consignment_sent_before_the_close_is_paid_for() {
    let (mut w, _) = lane_world_with(7, |c| {
        c.keep_history = true;
        c.trade.rules.pay_band_div = 0;
        c.trade.rules.sticky_div = 0;
    });
    run_checked(&mut w, Tick::from_days(20));
    let t0 = w.now();
    let q = w.cfg.trade.quantum.len();
    let b0 = w.cfg.trade.quantum.index(t0);
    assert!(w.close_lane(LANE, t0));
    let lag = w.edges[LANE as usize].lag();
    run_checked(&mut w, Tick(t0.0 + (2 * lag + 4) * q));
    let b = w.cfg.trade.quantum.index(w.now());
    let e = &w.edges[LANE as usize];
    let (mut value, mut landed_after_close, mut paid) = (0i128, 0i128, 0i128);
    for k in 0..=b {
        for p in e.dirs[0].goods.values() {
            let v = i128::from(p.arrival_at(k)) * i128::from(p.arrival_quote(k))
                / i128::from(rtsim_trade::PRICE_ONE);
            value += v;
            if k > b0 {
                landed_after_close += v;
            }
        }
        paid += e.dirs[1]
            .pay
            .values()
            .map(|p| i128::from(p.departure_at(k)))
            .sum::<i128>();
    }
    assert!(
        landed_after_close > 0,
        "consignments were still sailing at the close"
    );
    assert_eq!(paid, value, "every landed consignment was paid for");
}

/// Wheat, fish and a coin: a farm village, a fishing village, and a minting resource at both.
fn two_way_catalogue() -> (Goods, RecipeBook, GoodId, GoodId) {
    let mut goods = Goods::new();
    let item = |name: &str| GoodDef {
        name: name.into(),
        kind: GoodKind::Item,
        need_per_pop_day: 1_000,
        stack: 64,
    };
    let wheat = goods.add(item("wheat"));
    let fish = goods.add(item("fish"));
    let emerald = goods.add(GoodDef {
        name: "emerald".into(),
        kind: GoodKind::Currency(Culture::Human),
        need_per_pop_day: 0,
        stack: 64,
    });
    let recipe = |name: &str, output, yield_milli, labour, res| Recipe {
        name: name.into(),
        output,
        yield_per_batch: yield_milli,
        inputs: vec![],
        labour,
        resource: Some(ResourceId(res)),
    };
    let book = RecipeBook {
        recipes: vec![
            recipe("farm", wheat, 10 * MILLI, 1.0, 0),
            recipe("fish", fish, 10 * MILLI, 1.0, 1),
            recipe("mint", emerald, MILLI, 3.0, 2),
        ],
    };
    (goods, book, wheat, fish)
}

/// A (farms wheat, mints) and B (fishes, mints), each needing what the other makes, joined by
/// one sea lane: goods flow both ways over it, wheat A to B and fish B to A.
fn two_way_world(seed: u64, tweak: impl FnOnce(&mut WorldConfig)) -> (World, GoodId, GoodId) {
    two_way_world_with(seed, true, tweak)
}

/// [`two_way_world`], B minting or not.
fn two_way_world_with(
    seed: u64,
    b_mints: bool,
    tweak: impl FnOnce(&mut WorldConfig),
) -> (World, GoodId, GoodId) {
    let (goods, book, wheat, fish) = two_way_catalogue();
    let mut cfg = config(seed);
    tweak(&mut cfg);
    let node = |key, x, res: &[(u16, i64)]| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop: 10,
        resources: res
            .iter()
            .map(|&(r, a)| (ResourceId(r), a * MILLI))
            .collect(),
    };
    let mut w = World::build(
        cfg,
        goods,
        book,
        vec![
            node(1, 0, &[(0, 40), (2, 20)]),
            node(2, 600, &[(1, 40), (2, if b_mints { 20 } else { 0 })]),
        ],
    );
    assert_eq!(w.edges.len(), 1);
    for n in &mut w.nodes {
        n.out.retain(|d| d.edge != 0);
    }
    let lane = w.add_lane(
        LaneSpec {
            a: 0,
            b: 1,
            medium: Medium::Sea,
            length_blocks: 600,
            d_quanta: 6,
            dock_quanta: 4,
            cap_slot_milli: 4_000,
        },
        Tick::ZERO,
    );
    assert_eq!(lane, LANE);
    (w, wheat, fish)
}

fn dir_rate(w: &World, dir: usize, g: GoodId) -> i64 {
    w.edges[LANE as usize].dirs[dir]
        .goods
        .get(&g)
        .map_or(0, |p| p.current_rate())
}

/// The far end exports over the lane too: closing must wind its direction down as well, which
/// only the far end's own re-allocation can do.
#[test]
fn the_far_end_stops_exporting_too() {
    let (mut w, wheat, fish) = two_way_world(7, |_| {});
    run_checked(&mut w, Tick::from_days(25));
    assert!(dir_rate(&w, 0, wheat) > 0, "A sends wheat to B");
    assert!(dir_rate(&w, 1, fish) > 0, "B sends fish to A");
    let t0 = w.now();
    let q = w.cfg.trade.quantum.len();
    let lag = w.edges[LANE as usize].lag();
    assert!(w.close_lane(LANE, t0));
    assert_eq!(dir_rate(&w, 0, wheat), 0, "A's direction wound down");
    assert_eq!(dir_rate(&w, 1, fish), 0, "B's direction wound down");
    run_checked(&mut w, Tick(t0.0 + (2 * lag + 4) * q));
    assert_eq!(on_the_road(&w, w.now()), BTreeMap::new());
    let t1 = w.now();
    assert!(w.open_lane(LANE, t1));
    run_checked(&mut w, Tick(t1.0 + 10 * rtsim_kernel::clock::TICKS_PER_DAY));
    assert!(dir_rate(&w, 1, fish) > 0 || dir_rate(&w, 0, wheat) > 0);
}

/// With barter in flows, payments carry items over the lane, and they still travel when it is
/// closed: conservation holds across the closure, and the items are paid for in kind.
#[test]
fn barter_payments_still_travel_a_closed_lane_and_conserve() {
    let (mut w, ..) = two_way_world(7, |c| {
        c.trade.rules.barter_in_flows = true;
        // The money sink would hold a rich treasury at twice its target, short of the glut that
        // makes it refuse cash.
        c.trade.rules.money_sink = false;
    });
    // A is flush with coin from the start (a player's hoard), so it refuses cash and B pays for
    // its wheat in fish.
    let coin = GoodId(2);
    let hoard = BTreeMap::from([(coin, 10 * w.market(0).line(coin).target)]);
    w.stock_from_players(0, &hoard, Tick::ZERO);
    let items = |w: &World, goods: &Goods| -> i64 {
        w.edges[LANE as usize]
            .dirs
            .iter()
            .flat_map(|d| d.pay.iter())
            .filter(|(g, _)| goods.get(**g).kind == GoodKind::Item)
            .map(|(_, p)| p.current_rate())
            .sum()
    };
    let mut seen_before = 0;
    let mut t = 0;
    let q = w.cfg.trade.quantum.len();
    while t < Tick::from_days(25).0 {
        t += q;
        run_checked(&mut w, Tick(t));
        seen_before = seen_before.max(items(&w, &w.goods));
    }
    assert!(seen_before > 0, "payments in kind travel the open lane");
    let t0 = w.now();
    let lag = w.edges[LANE as usize].lag();
    assert!(w.close_lane(LANE, t0));
    let mut seen_closed = 0;
    let mut t = t0.0;
    while t < t0.0 + (2 * lag + 4) * q {
        t += q;
        run_checked(&mut w, Tick(t));
        seen_closed = seen_closed.max(items(&w, &w.goods));
    }
    assert!(
        seen_closed > 0,
        "payments in kind travel the closed lane (PC57)"
    );
    assert_eq!(on_the_road(&w, w.now()), BTreeMap::new());
}

/// Three villages: S1 (node 0) and S2 (node 2) farm wheat, D (node 1) mints. S1 reaches D only
/// by the lane; S2 reaches D by a road. Returns the world, wheat, the road's edge index.
fn supplier_world(seed: u64) -> (World, GoodId, u32) {
    let (goods, book, wheat) = catalogue();
    let (farm, ore) = (ResourceId(0), ResourceId(1));
    let node = |key, x, res: ResourceId, amount: i64| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop: 10,
        resources: BTreeMap::from([(res, amount * MILLI)]),
    };
    let mut w = World::build(
        config(seed),
        goods,
        book,
        vec![
            node(1, 0, farm, 40),
            node(2, 600, ore, 20),
            node(3, 1_200, farm, 40),
        ],
    );
    // Keep only the road between D and S2; S1 gets its lane to D below.
    let keep = w
        .edges
        .iter()
        .position(|e| (e.a, e.b) == (1, 2) || (e.a, e.b) == (2, 1))
        .expect("a road joins D and S2") as u32;
    for n in &mut w.nodes {
        n.out.retain(|d| d.edge == keep);
    }
    w.add_lane(
        LaneSpec {
            a: 0,
            b: 1,
            medium: Medium::Sea,
            length_blocks: 600,
            d_quanta: 6,
            dock_quanta: 4,
            cap_slot_milli: 4_000,
        },
        Tick::ZERO,
    );
    (w, wheat, keep)
}

/// A closed lane's far end is no supplier: with two suppliers of a destination, one over a lane
/// and one over a road, closing the lane makes the destination advertise one supplier, and the
/// road supplier is asked for the whole want at its next allocation (PC60), not half of it.
#[test]
fn a_closed_lane_is_no_supplier_and_the_road_supplier_is_asked_for_the_whole_want() {
    let (mut w, wheat, road) = supplier_world(7);
    let lane = w.edges.len() as u32 - 1;
    let q = w.cfg.trade.quantum.len();
    let rate = |w: &World, e: u32| {
        w.edges[e as usize]
            .dirs
            .iter()
            .map(|d| d.goods.get(&wheat).map_or(0, |p| p.current_rate()))
            .sum::<i64>()
    };
    let suppliers = |w: &World| {
        w.nodes[1]
            .last_sent
            .as_ref()
            .map(|s| s.good(wheat).suppliers)
    };
    run_checked(&mut w, Tick::from_days(30));
    let (from_lane, from_road) = (rate(&w, lane), rate(&w, road));
    assert!(from_lane > 0 && from_road > 0, "both suppliers send");
    assert_eq!(suppliers(&w), Some(2));

    let t0 = w.now();
    assert!(w.close_lane(lane, t0));
    assert_eq!(suppliers(&w), Some(1), "D now advertises one supplier");
    // Once the news has crossed the road (a few quanta), the road supplier takes up the whole of
    // what the lane carried.
    run_checked(&mut w, Tick(t0.0 + 30 * q));
    assert_eq!(rate(&w, lane), 0);
    assert!(
        rate(&w, road) >= from_lane + from_road - 4,
        "the road supplier sends {} of the {} the two sent",
        rate(&w, road),
        from_lane + from_road
    );
    // Reopened, the lane is a supplier again.
    let t1 = w.now();
    assert!(w.open_lane(lane, t1));
    assert_eq!(suppliers(&w), Some(2));
}
