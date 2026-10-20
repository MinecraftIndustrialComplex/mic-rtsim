// SPDX-License-Identifier: AGPL-3.0-or-later
//! Caravans batch their road's flow (overnight choice E53): a caravan leaves when the dock has
//! filled it or when its oldest load has waited the longest dock wait, and its whole load is on
//! the road from departure to arrival. Sampled travellers stay consistent with the flow (SCALE.md
//! §3): what the pipelines count in transit is exactly what waits at the two docks plus what the
//! caravans on the road carry.

use std::collections::BTreeMap;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_trade::labour::{Recipe, RecipeBook, ResourceId};
use rtsim_trade::{GoodDef, GoodKind, Goods, MILLI};
use rtsim_world::{NodeData, World, WorldConfig, synth};

/// For every direction of every road and every pipeline on it: in transit = origin dock + the
/// windows of caravans on the road + destination dock, and the travellers carry exactly the
/// windows on the road.
fn check_docks(w: &World) {
    let q = w.cfg.trade.quantum;
    let b = q.index(w.now());
    for (k, e) in w.edges.iter().enumerate() {
        for di in 0..2u8 {
            let d = &e.dirs[di as usize];
            let travellers = w.travellers(k as u32, di);
            let mut carried: BTreeMap<rtsim_trade::GoodId, i128> = BTreeMap::new();
            for t in &travellers {
                for (g, a) in &t.cargo {
                    *carried.entry(*g).or_default() += i128::from(*a);
                }
            }
            let lo = (b + 1).saturating_sub(e.lag());
            // Departures whose windows still hold something in transit.
            let deps = d.lattice.departures(lo.saturating_sub(e.dock + 1), b);
            let last = deps.last().copied();
            for (g, p) in d.goods.iter().chain(d.pay.iter()) {
                let transit = p.in_transit(b);
                let dock_out = match last {
                    Some(l) if l < b => p.departed_between(l + 1, b),
                    None => p.departed_between(lo, b),
                    _ => 0,
                };
                let mut road = 0i128;
                let mut dock_in = 0i128;
                for &dep in &deps {
                    let (w0, _) = d.lattice.window(dep);
                    let held = p.departed_between(w0.max(lo), dep);
                    if dep + e.d > b {
                        road += held;
                    } else {
                        dock_in += held;
                    }
                }
                assert_eq!(
                    transit,
                    dock_out + road + dock_in,
                    "edge {k} dir {di} good {g:?} at {b}: docks and road do not add up"
                );
                assert_eq!(
                    carried.get(g).copied().unwrap_or(0),
                    road,
                    "edge {k} dir {di} good {g:?}: travellers carry something else than the road"
                );
            }
        }
    }
}

#[test]
fn travellers_are_the_flow_on_the_road() {
    let mut cfg = WorldConfig::new(3);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let (mut w, _) = synth::build(cfg, true);
    for h in 1..=24 * 30 {
        w.run_until(Tick(h * 1000 + 250));
        if h % 5 == 0 {
            check_docks(&w);
        }
    }
}

/// A caravan's load is the same at every point of its walk: nothing of it arrives early.
#[test]
fn a_caravans_load_does_not_shrink_on_the_road() {
    let mut cfg = WorldConfig::new(3);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let (mut w, _) = synth::build(cfg, true);
    w.run_until(Tick::from_days(10));
    let mut seen: BTreeMap<rtsim_travel::CaravanKey, BTreeMap<rtsim_trade::GoodId, i64>> =
        BTreeMap::new();
    let mut checked = 0;
    for h in 0..24 * 10 {
        w.run_until(Tick::from_days(10) + h * 1000);
        for k in 0..w.edges.len() {
            for di in 0..2u8 {
                for t in w.travellers(k as u32, di) {
                    match seen.get(&t.key) {
                        Some(c) => {
                            assert_eq!(c, &t.cargo, "{:?} changed load on the road", t.key);
                            checked += 1;
                        }
                        None => {
                            seen.insert(t.key, t.cargo.clone());
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 100, "too few caravans seen twice: {checked}");
}

/// Two villages, one farming for both, many people: the flow is several slots a quantum, so
/// caravans leave as soon as they are full (6 slots), every few quanta.
#[test]
fn busy_roads_send_full_caravans() {
    let mut goods = Goods::new();
    let wheat = goods.add(GoodDef {
        name: "wheat".into(),
        kind: GoodKind::Item,
        need_per_pop_day: 1_000,
        stack: 64,
    });
    goods.add(GoodDef {
        name: "emerald".into(),
        kind: GoodKind::Currency(Culture::Human),
        need_per_pop_day: 0,
        stack: 64,
    });
    let emerald = goods.find("emerald").unwrap();
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
    let mut cfg = WorldConfig::new(1);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(250);
    let node = |key, x, pop, res: &[(ResourceId, i64)]| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, 0),
        pop,
        resources: res.iter().map(|&(r, a)| (r, a * MILLI)).collect(),
    };
    let mut w = World::build(
        cfg,
        goods,
        book,
        vec![
            node(1, 0, 100, &[(farm, 20_000)]),
            node(2, 600, 1_500, &[(ore, 2_000)]),
        ],
    );
    w.run_until(Tick::from_days(20));
    let slots = |t: &rtsim_travel::Traveller| *t.cargo.get(&wheat).unwrap_or(&0) as f64 / 64_000.0;
    let mut loads = Vec::new();
    for h in 0..48 {
        w.run_until(Tick::from_days(20) + h * 1000);
        let d = w.nodes[0].out[0];
        for t in w.travellers(d.edge, d.dir) {
            loads.push((t.key.departure, slots(&t)));
        }
    }
    loads.sort_by_key(|x| x.0);
    loads.dedup_by_key(|x| x.0);
    eprintln!("wheat caravans 0 to 1: {loads:?}");
    assert!(loads.len() >= 4, "caravans should leave every few quanta");
    let full = loads.iter().filter(|x| x.1 >= 6.0).count();
    assert!(
        full * 2 >= loads.len(),
        "most caravans on a busy road should be full: {loads:?}"
    );
    w.check_conserved(w.now()).unwrap();
    let _ = TICKS_PER_DAY;
}
