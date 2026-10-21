// SPDX-License-Identifier: AGPL-3.0-or-later
//! The money sink's works purse (SINK.md, docs/choices-money-sink.md): coin a village's treasury
//! line sinks above its ceiling buys works of its own culture and pays dragons' tribute; every
//! coin stays in the books; no market, caravan or price moves because of it; and the purse
//! survives a save, a reopened page store and a second run byte for byte.

use std::sync::{Arc, RwLock};

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_settle::{Culture, NodeKind};
use rtsim_trade::MILLI;
use rtsim_trade::build::Genesis;
use rtsim_world::synth;
use rtsim_world::works::{Next, WorkGroup, WorkPlace, synthetic_catalog};
use rtsim_world::world::Command;
use rtsim_world::{World, WorldConfig};

fn cfg(seed: u64, genesis: Genesis) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.genesis = genesis;
    cfg.check_every_event = true;
    cfg
}

fn world(seed: u64, genesis: Genesis, works: bool) -> World {
    let (mut w, _) = synth::build(cfg(seed, genesis), true);
    if works {
        let cat = synthetic_catalog(&w.goods);
        w.set_works(cat, Tick::ZERO);
    }
    w
}

fn reload(w: &World, seed: u64, genesis: Genesis) -> World {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    World::load(cfg(seed, genesis), goods, book, &w.save()).expect("the snapshot reads back")
}

/// Runs to `until` an hour at a time as the bridge would: every bought work answered `placed`
/// (a village buys its next work once the last one is answered, MSC13). Returns every command,
/// in order.
fn run(w: &mut World, until: Tick, placed: bool) -> Vec<Command> {
    let mut out = Vec::new();
    while w.now() < until {
        let next = Tick((w.now().0 + 1_000).min(until.0));
        w.run_until(next);
        let cmds = w.take_commands();
        for c in &cmds {
            if let Some(op) = c.work_op() {
                let now = w.now();
                w.work_result(op, placed, now);
            }
        }
        out.extend(cmds);
    }
    out
}

fn culture(w: &World, i: usize) -> Option<Culture> {
    match w.nodes[i].kind {
        NodeKind::Village(c) => Some(c),
        _ => None,
    }
}

/// The catalogue entry a bought template belongs to.
fn def_of_template(w: &World, template: &str) -> usize {
    w.works_catalog
        .iter()
        .position(|d| {
            d.variants.iter().any(
                |v| matches!(&v.place, WorkPlace::Template { template: t, .. } if t == template),
            )
        })
        .expect("a bought template is in the catalogue")
}

fn bought(w: &World, i: usize, group: WorkGroup) -> u32 {
    let Some(s) = w.works_of(i) else { return 0 };
    w.works_catalog
        .iter()
        .filter(|d| d.group == group)
        .map(|d| s.bought(d.key()))
        .sum()
}

/// A village whose purse has bought everything it may: the first with `Next::Nothing` and coin.
fn rich(w: &World) -> usize {
    (0..w.nodes.len())
        .find(|&i| {
            w.works_of(i)
                .is_some_and(|s| s.next == Next::Nothing && !s.tributary && s.in_flight == 0)
                && w.works_purse(i, w.now())
                    .iter()
                    .any(|&(_, a)| a >= 10 * MILLI)
        })
        .expect("a village has bought out the catalogue by now")
}

/// Works leave every market exactly where it would be without them: the purse is coin the line
/// law already sank (MS-1), so markets, prices, caravans and the round 2 quiescence results are
/// the sink-free world's own.
#[test]
fn works_leave_every_market_as_it_was() {
    for (seed, genesis) in [(7, Genesis::AtTarget), (11, Genesis::Share(500))] {
        let mut a = world(seed, genesis, false);
        let mut b = world(seed, genesis, true);
        for day in [30u64, 120, 300] {
            let t = Tick(day * TICKS_PER_DAY);
            a.run_until(t);
            let cb = run(&mut b, t, true);
            for i in 0..a.nodes.len() {
                assert_eq!(a.market(i), b.market(i), "seed {seed} day {day} node {i}");
            }
            let ca = a.take_commands();
            let cb: Vec<Command> = cb
                .into_iter()
                .filter(|c| !matches!(c, Command::PlaceWork(_) | Command::SpawnWork(_)))
                .collect();
            assert_eq!(ca, cb, "seed {seed} day {day}");
            assert_eq!(a.stats.market_events, b.stats.market_events);
            assert_eq!(a.stats.deliveries, b.stats.deliveries);
            assert_eq!(a.caravans.len(), b.caravans.len());
        }
        assert_eq!(a.stats.works_events, 0, "no catalogue, no works events");
        assert!(b.stats.works > 0, "seed {seed}: the sink bought works");
        b.check_conserved(b.now()).unwrap();
    }
}

/// A village whose treasury sinks above its ceiling buys works of its own culture with that coin,
/// within each kind's cap, and only when its purse holds the price; no event runs for a purse
/// that cannot pay.
#[test]
fn a_village_above_its_ceiling_buys_works_of_its_own_culture() {
    let mut w = world(7, Genesis::AtTarget, true);
    let cmds = run(&mut w, Tick::from_days(120), true);
    let mut placed = 0;
    for c in &cmds {
        if let Command::PlaceWork(p) = c {
            placed += 1;
            let i = w.node_of_key(p.village).expect("a known village");
            let d = &w.works_catalog[def_of_template(&w, &p.template)];
            assert_eq!(
                Some(d.culture),
                culture(&w, i),
                "{} at node {i}",
                p.template
            );
            assert_eq!(p.radius, w.cfg.works_radius);
            assert!(p.rotation < 4);
        }
    }
    assert!(placed > 0, "a village bought a lamp or decor");
    for i in 0..w.nodes.len() {
        let Some(s) = w.works_of(i) else { continue };
        for &(_, a) in &w.works_purse(i, w.now()) {
            assert!(a >= 0, "node {i}: a purse never overdraws");
        }
        let pop = w.nodes[i].local.pop;
        for d in &w.works_catalog {
            let cap = match d.cap {
                rtsim_world::works::WorkCap::PerResidents(n) => pop / n,
                rtsim_world::works::WorkCap::Fixed(n) => n,
            };
            assert!(s.bought(d.key()) <= cap, "node {i}: {} over its cap", d.id);
        }
        if s.spent.iter().any(|&(_, a)| a > 0) {
            assert!(s.nth > 0);
        }
    }
    // Every works event bought something: a purse that cannot pay books no event.
    assert_eq!(w.stats.works_events, w.stats.works);
    assert_eq!(
        bought(&w, 0, WorkGroup::Golem),
        0,
        "no golems without a threat"
    );
    w.check_conserved(w.now()).unwrap();
}

/// A raid puts golems first for a season (MS-6, MS-8): a village that had bought out its lamps
/// and decor spawns golems from the purse it kept growing (MS-9), up to its cap.
#[test]
fn a_raid_buys_golems_first() {
    let mut w = world(7, Genesis::AtTarget, true);
    let _ = run(&mut w, Tick::from_days(120), true);
    let i = rich(&w);
    let key = w.nodes[i].key;
    w.village_raided(i, false, w.now());
    let golem = w
        .works_catalog
        .iter()
        .position(|d| d.group == WorkGroup::Golem)
        .unwrap();
    assert!(
        matches!(w.works_of(i).unwrap().next, Next::Work { def, .. } if def as usize == golem),
        "a raided village wants a golem: {:?}",
        w.works_of(i).unwrap().next
    );
    let spawned = run(&mut w, Tick::from_days(126), true)
        .into_iter()
        .filter(|c| matches!(c, Command::SpawnWork(s) if s.village == key && s.body == "minecraft:iron_golem"))
        .count() as u32;
    let cap = w.nodes[i].local.pop / 5;
    assert!(
        spawned > 0 && spawned <= cap,
        "{spawned} golems for a cap of {cap}"
    );
    assert_eq!(bought(&w, i, WorkGroup::Golem), spawned);
    w.check_conserved(w.now()).unwrap();
}

/// A village paying tribute keeps its purse for the lair (MS-5, MS-6): it buys nothing, pays a
/// share in whole coins of the currency it holds most of (MS-10), and when the tribute lapses it
/// buys again, golems first.
#[test]
fn a_tributary_village_buys_nothing_and_pays_whole_coins() {
    let mut w = world(7, Genesis::AtTarget, true);
    let _ = run(&mut w, Tick::from_days(120), true);
    let i = rich(&w);
    let key = w.nodes[i].key;
    let now = w.now();
    w.village_raided(i, true, now);
    assert_eq!(w.works_of(i).unwrap().next, Next::Nothing);
    assert_eq!(w.works_of(i).unwrap().due, None);
    let before = w.works_purse(i, now);
    let (g, a) = w.pay_tribute(i, 250, now).expect("a rich purse pays");
    assert!(a >= MILLI && a % MILLI == 0, "whole coins: {a}");
    let most = before.iter().max_by_key(|&&(_, a)| a).unwrap();
    assert_eq!(g, most.0, "the currency it holds most of");
    assert!(a <= most.1 * 250 / 1000 + MILLI);
    let after = w.works_purse(i, now);
    let got = |v: &[(rtsim_trade::GoodId, i64)]| v.iter().find(|x| x.0 == g).unwrap().1;
    assert_eq!(got(&before) - got(&after), a);
    assert_eq!(w.works_of(i).unwrap().tribute(g), a);
    w.check_conserved(now).unwrap();

    let cmds = run(&mut w, Tick::from_days(160), true);
    let mine = |c: &Command| match c {
        Command::PlaceWork(p) => p.village == key,
        Command::SpawnWork(s) => s.village == key,
        _ => false,
    };
    assert!(!cmds.iter().any(mine), "a tributary buys nothing");

    w.tribute_lapsed(i, w.now());
    assert!(!w.works_of(i).unwrap().tributary);
    assert!(
        run(&mut w, Tick::from_days(166), true)
            .iter()
            .any(|c| mine(c) && matches!(c, Command::SpawnWork(_))),
        "a lapsed tributary buys golems first"
    );
    w.check_conserved(w.now()).unwrap();
}

/// No free plot for a work marks its kind full for the village (MS-11): the coin stays spent,
/// and the village never asks for that kind again.
#[test]
fn a_refused_work_marks_its_kind_full() {
    let mut w = world(7, Genesis::AtTarget, true);
    w.run_until(Tick::from_days(60));
    let cmds = w.take_commands();
    let first = cmds
        .iter()
        .find_map(|c| match c {
            Command::PlaceWork(p) => Some(p.clone()),
            _ => None,
        })
        .expect("a work was bought in 60 days");
    // One work in flight per village (MSC13): unanswered, the village bought only this one.
    let theirs = cmds
        .iter()
        .filter(|c| matches!(c, Command::PlaceWork(p) if p.village == first.village))
        .count();
    assert_eq!(theirs, 1, "one work in flight");
    let i = w.node_of_key(first.village).unwrap();
    let kind = w.works_catalog[def_of_template(&w, &first.template)].key();
    let spent: i64 = w.works_of(i).unwrap().spent.iter().map(|x| x.1).sum();
    w.work_result(first.op, false, w.now());
    // A second answer for the same op changes nothing.
    w.work_result(first.op, false, w.now());
    let s = w.works_of(i).unwrap();
    assert!(s.is_full(kind));
    assert_eq!(s.spent.iter().map(|x| x.1).sum::<i64>(), spent, "no refund");
    assert_eq!(w.stats.works_refused, 1);
    // The others are placed.
    for c in &cmds {
        if let Some(op) = c.work_op() {
            let now = w.now();
            w.work_result(op, true, now);
        }
    }
    for c in run(&mut w, Tick::from_days(300), true) {
        if let Command::PlaceWork(p) = c
            && p.village == first.village
        {
            assert_ne!(
                w.works_catalog[def_of_template(&w, &p.template)].key(),
                kind,
                "a full kind is never bought again"
            );
        }
    }
    w.check_conserved(w.now()).unwrap();
}

/// The same seed buys the same works on the same ticks; a world read back from its snapshot at
/// any day continues exactly, commands and all.
#[test]
fn works_are_deterministic_and_survive_a_reload() {
    let g = Genesis::AtTarget;
    let mut a = world(7, g, true);
    let mut b = world(7, g, true);
    let mut ops = Vec::new();
    for day in 1..=200u64 {
        a.run_until(Tick::from_days(day));
        b.run_until(Tick::from_days(day));
        if day % 37 == 0 {
            b = reload(&b, 7, g);
        }
        if day == 90 {
            let i = rich(&a);
            a.village_raided(i, true, a.now());
            b.village_raided(i, true, b.now());
            assert_eq!(
                a.pay_tribute(i, 250, a.now()),
                b.pay_tribute(i, 250, b.now())
            );
        }
        if day == 130 {
            let i = rich(&a);
            a.village_raided(i, false, a.now());
            b.village_raided(i, false, b.now());
        }
        assert_eq!(a.state_hash(), b.state_hash(), "day {day}");
        let (ca, cb) = (a.take_commands(), b.take_commands());
        assert_eq!(ca, cb, "day {day}");
        for c in ca {
            if let Some(op) = c.work_op() {
                ops.push(op);
                // Every other work finds no plot.
                let placed = ops.len() % 2 == 0;
                a.work_result(op, placed, a.now());
                b.work_result(op, placed, b.now());
            }
        }
    }
    assert!(a.stats.works > 0);
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.save(), b.save());
    let c = reload(&a, 7, g);
    assert_eq!(c.save(), a.save(), "the snapshot reads back byte for byte");
}

type Store = Arc<RwLock<PageStore>>;
const META: PageKey = PageKey::new(0x5001, 0);

fn save_paged(w: &mut World, store: &Store) {
    let mut b = SaveBatch::new();
    let t = w.save_paged(&mut b, META);
    store.write().unwrap().commit(b, 0).expect("commit");
    w.saved(t);
    w.enforce_budget();
}

/// Paged under a budget of a page or two and reopened from its pages, a world with works runs as
/// the resident one: same state hash, same commands, same snapshot.
#[test]
fn a_paged_world_with_works_runs_as_a_resident_one() {
    let dir = std::env::temp_dir().join(format!("rtsim-world-works-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store: Store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    let g = Genesis::AtTarget;
    let mut a = world(7, g, true);
    let mut b = world(7, g, true);
    b.attach_store(store.clone(), 12_000);
    save_paged(&mut b, &store);
    for day in 1..=150u64 {
        a.run_until(Tick::from_days(day));
        b.run_until(Tick::from_days(day));
        save_paged(&mut b, &store);
        if day % 41 == 0 {
            let (goods, ids) = synth::goods();
            let book = synth::recipes(&ids);
            b = World::open_paged(store.clone(), 12_000, META, cfg(7, g), goods, book)
                .expect("the pages open");
        }
        if day == 100 {
            let i = rich(&a);
            a.village_raided(i, false, a.now());
            b.village_raided(i, false, b.now());
        }
        assert_eq!(a.state_hash(), b.state_hash(), "day {day}");
        let (ca, cb) = (a.take_commands(), b.take_commands());
        assert_eq!(ca, cb, "day {day}");
        for c in ca {
            if let Some(op) = c.work_op() {
                a.work_result(op, true, a.now());
                b.work_result(op, true, b.now());
            }
        }
    }
    assert!(b.markets.rows().stats().loads > 0, "the markets paged");
    assert!(a.stats.works > 0);
    assert_eq!(a.save(), b.save());
    b.check_conserved(b.now()).unwrap();
    drop(b);
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

/// A test's grant goes straight into the purse, ledgered as minted: a village far below its
/// ceiling buys its next work at once.
#[test]
fn a_granted_purse_buys_at_once() {
    let mut w = world(7, Genesis::Share(500), true);
    w.run_until(Tick::from_days(2));
    let _ = w.take_commands();
    let i = (0..w.nodes.len())
        .find(|&i| {
            culture(&w, i) == Some(Culture::Human) && w.works_of(i).is_some_and(|s| s.nth == 0)
        })
        .expect("a human village that bought nothing yet");
    let key = w.nodes[i].key;
    assert!(w.grant_purse(i, 100 * MILLI, w.now()));
    assert!(!w.grant_purse(i, 0, w.now()));
    w.check_conserved(w.now()).unwrap();
    w.run_until(Tick(w.now().0 + 1));
    let bought = w
        .take_commands()
        .into_iter()
        .filter(|c| matches!(c, Command::PlaceWork(p) if p.village == key))
        .count();
    assert!(bought > 0, "the grant bought a work at once");
    w.check_conserved(w.now()).unwrap();
    let c = reload(&w, 7, Genesis::Share(500));
    assert_eq!(c.state_hash(), w.state_hash());
}

/// Nothing per tick (SINK.md §7): a works event runs only when a purse reaches a price. With a
/// catalogue whose every cap is zero nothing is ever due, so years of trading run no works event
/// at all while the purses fill; with the real catalogue every works event buys (above).
#[test]
fn a_sink_with_nothing_to_buy_runs_no_event() {
    let mut w = world(7, Genesis::AtTarget, false);
    let mut cat = synthetic_catalog(&w.goods);
    for d in &mut cat {
        d.cap = rtsim_world::works::WorkCap::Fixed(0);
    }
    w.set_works(cat, Tick::ZERO);
    let cmds = run(&mut w, Tick::from_days(400), true);
    assert!(w.stats.market_events > 0, "the markets traded");
    assert_eq!(
        w.stats.works_events, 0,
        "no works event without a work to buy"
    );
    assert_eq!(w.stats.works, 0);
    assert!(
        !cmds
            .iter()
            .any(|c| matches!(c, Command::PlaceWork(_) | Command::SpawnWork(_))),
        "nothing placed"
    );
    let purses: i64 = (0..w.nodes.len())
        .flat_map(|i| w.works_purse(i, w.now()))
        .map(|x| x.1)
        .sum();
    assert!(purses > 0, "the purses still fill (MS-9)");
    w.check_conserved(w.now()).unwrap();
}
