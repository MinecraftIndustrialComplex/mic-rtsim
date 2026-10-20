// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ben's morning review of 2026-10-20, in the phase 1 world (choices PR1 onward in
//! docs/overnight-choices-p1rework.md): news by messengers, touring bands on their new circuits,
//! currency exchange at border markets, and a caravan member's loss booked as what it really held.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ReportKind;
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_trade::{Amount, GoodId, MILLI};
use rtsim_travel::Medium;
use rtsim_world::{NodeData, World, WorldConfig, synth};

fn scenario() -> World {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    synth::build(cfg, true).0
}

/// News leaves with the next messenger, never later than a messenger period, and arrives a
/// messenger's walk after: well before a caravan's dock wait would let it go (PR6).
#[test]
fn news_leaves_with_the_next_messenger_not_the_next_caravan() {
    let mut w = scenario();
    w.run_until(Tick::from_days(5));
    let player = w.player(7);
    let at = w.now();
    let report = w.village_report(0, player, ReportKind::ResidentAssault, at);
    let q = w.cfg.trade.quantum;
    let nb = q.index(at) + 1;
    let mut latest = at;
    let mut told = Vec::new();
    for d in w.nodes[0].out.clone() {
        let e = &w.edges[d.edge as usize];
        assert_eq!(e.medium, Medium::Road);
        assert!(e.msg_period > 0, "every road has messengers");
        let to = e.dirs[d.dir as usize].to;
        let by = q.at(nb + w.cfg.messenger_quanta + e.news_walk());
        // A caravan could have waited the whole dock wait before walking.
        assert!(
            by < q.at(nb + e.dock + e.d),
            "messengers are no faster than caravans"
        );
        latest = latest.max(by);
        told.push(to);
    }
    w.run_until(latest);
    for to in told {
        assert!(
            w.knowledge.knows(w.nodes[to].actor, report),
            "neighbour {to} did not hear within a messenger's period and walk"
        );
    }
    w.check_conserved(w.now()).unwrap();
}

/// Lifetimes are long enough for news to cross several roads now (P18, PR7).
#[test]
fn robbery_reports_outlive_several_hops() {
    let w = scenario();
    let t = &w.cfg.knowledge;
    let walk = w.edges.iter().map(|e| e.news_walk()).max().unwrap();
    let hop = w.cfg.trade.quantum.at(w.cfg.messenger_quanta + walk).0;
    assert!(t.lifetime(ReportKind::Robbery) >= 4 * hop);
    assert!(t.lifetime(ReportKind::Killing) > t.lifetime(ReportKind::Robbery));
}

const R1: u64 = 1;
const R2: u64 = 2;
const HUMAN: u64 = 3;
const BASE: u64 = 4;

/// Two ribbit villages, a human village beside them, and a player's farm.
fn small_world() -> World {
    let mut cfg = WorldConfig::new(7);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(500);
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let res = |r: &[(rtsim_trade::labour::ResourceId, i64)]| -> BTreeMap<_, Amount> {
        r.iter().map(|&(k, v)| (k, v * MILLI)).collect()
    };
    let nodes = vec![
        NodeData {
            key: R1,
            kind: NodeKind::Village(Culture::Ribbit),
            pos: Pos::new(0, 0, 0),
            pop: 12,
            resources: res(&[(synth::GEODE, 6), (synth::SWAMP, 4)]),
        },
        NodeData {
            key: R2,
            kind: NodeKind::Village(Culture::Ribbit),
            pos: Pos::new(0, 500, 0),
            pop: 10,
            resources: res(&[(synth::SWAMP, 6)]),
        },
        NodeData {
            key: HUMAN,
            kind: NodeKind::Village(Culture::Human),
            pos: Pos::new(0, 0, 600),
            pop: 15,
            resources: res(&[(synth::FARMLAND, 80), (synth::SHEEP, 4)]),
        },
        NodeData {
            key: BASE,
            kind: NodeKind::PlayerBase { owner: 0xF00D },
            pos: Pos::new(0, -400, 0),
            pop: 0,
            resources: BTreeMap::new(),
        },
    ];
    World::build(cfg, goods, book, nodes)
}

fn idx(w: &World, key: u64) -> usize {
    w.node_of_key(key).unwrap()
}

/// Runs until band `b` is staying at `node`, at most `days` days on.
fn until_staying(w: &mut World, b: usize, node: usize, days: u64) {
    let end = w.now() + days * rtsim_kernel::clock::TICKS_PER_DAY;
    loop {
        let v = w.band_view(b, w.now());
        if v.staying.is_some_and(|s| s.0 == node) {
            return;
        }
        assert!(w.now() < end, "band {b} never stayed at {node}");
        let t = w.now() + 250;
        w.run_until(t);
    }
}

/// P16: a band tours ribbit villages and the human village on the border; a player settlement
/// joins its circuit at the next stay at home once marked, while its owner is in good standing,
/// and leaves it once the owner is not. Every stay away from home raises the host's opinion of
/// the band's home village.
#[test]
fn bands_tour_the_border_and_settlements_in_good_standing() {
    let mut w = small_world();
    let (r1, r2, human, base) = (idx(&w, R1), idx(&w, R2), idx(&w, HUMAN), idx(&w, BASE));
    let b = w
        .bands
        .iter()
        .position(|x| x.home == r1)
        .expect("the bigger ribbit village has a band");
    assert!(w.bands[b].route.contains(&r2));
    assert!(
        w.bands[b].route.contains(&human),
        "a human village with a ribbit neighbour is on the border: {:?}",
        w.bands[b].route
    );
    assert!(
        !w.bands[b].route.contains(&base),
        "nobody marks the farm yet"
    );

    // A stay at the other ribbit village raises its opinion of the band's home.
    let (h2, home) = (w.nodes[r2].actor, w.nodes[r1].actor);
    assert_eq!(w.knowledge.sentiment(h2, home, w.now()), 0);
    until_staying(&mut w, b, r2, 10);
    assert!(w.knowledge.sentiment(h2, home, w.now()) > 0);
    assert!(w.stats.band_plays > 0);

    // A player marks the farm: from the next stay at home it is a stop.
    let player = w.player_uuid(0xF00D, 0xF00D);
    let at = w.now();
    w.mark_base(BASE, Pos::new(0, -400, 0), player, at);
    until_staying(&mut w, b, r1, 10);
    w.run_until(w.now() + 1);
    assert!(
        w.bands[b].route.contains(&base),
        "a settlement in good standing: {:?}",
        w.bands[b].route
    );
    until_staying(&mut w, b, base, 10);
    let hb = w.nodes[base].actor;
    assert!(
        w.knowledge.sentiment(hb, home, w.now()) > 0,
        "it played there"
    );

    // The owner robs the band's home: no more performances there, and the next circuit leaves
    // the farm out.
    let at = w.now();
    w.knowledge.adjust(home, player, -60, at);
    until_staying(&mut w, b, r1, 10);
    w.run_until(w.now() + 1);
    assert!(
        !w.bands[b].route.contains(&base),
        "a settlement whose owner is disliked: {:?}",
        w.bands[b].route
    );
    // The world saves and loads with its new circuits and booked stays.
    let bytes = w.save();
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut back = World::load(w.cfg.clone(), goods, book, &bytes).unwrap();
    assert_eq!(back.state_hash(), w.state_hash());
    let plays = w.stats.band_plays;
    w.run_days(6);
    back.run_days(6);
    assert!(w.stats.band_plays > plays, "the band kept playing");
    assert_eq!(back.state_hash(), w.state_hash());
    assert_eq!(back.stats.band_plays, w.stats.band_plays);
    w.check_conserved(w.now()).unwrap();
}

/// A band's stays are events, not market events: a quiet world with bands stays quiet in its
/// markets (E57's promise kept with P16's performances).
#[test]
fn performances_are_not_market_events() {
    let mut w = scenario();
    w.run_until(Tick::from_days(500));
    let (events, plays) = (w.stats.market_events, w.stats.band_plays);
    w.run_until(Tick::from_days(600));
    assert!(w.stats.band_plays > plays, "bands play in a quiet world");
    assert_eq!(w.stats.market_events, events, "a performance woke a market");
}

#[allow(dead_code)]
fn good(w: &World, name: &str) -> GoodId {
    w.goods
        .ids()
        .find(|&g| w.goods.get(g).name == name)
        .unwrap()
}
