// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round 2's phase 1 features: sorcerers who remember, ribbit village growth, touring bands,
//! player bases with caravans trading there, and what a player can see (P16, P17, P30; PLAN.md
//! §8.2, §8.8; A15).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_trade::build::Genesis;
use rtsim_trade::{MILLI, OfferSide};
use rtsim_world::bases::CaravanRef;
use rtsim_world::{Command, Side, Visible, World, WorldConfig, synth};

const MIREWATER: usize = 6;
const BOGSEND: usize = 7;
const FARM: usize = 10;

fn world(check: bool) -> World {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = check;
    cfg.genesis = Genesis::Share(500);
    synth::build(cfg, true).0
}

/// A loaded traveller leaving `from`, out of sight of its village, waiting for one if need be.
fn caravan_out_of(w: &mut World, from: usize) -> rtsim_travel::CaravanKey {
    let mut t = w.now();
    loop {
        for d in w.nodes[from].out.clone() {
            for tr in w.travellers(d.edge, d.dir) {
                let load: i64 = tr.cargo.values().sum();
                if load > 0 && tr.progress_milli > 200 && tr.progress_milli < 700 {
                    return tr.key;
                }
            }
        }
        t += 1000;
        w.run_until(t);
        assert!(t < Tick::from_days(200), "no caravan out of {from}");
    }
}

#[test]
fn sorcerers_leave_out_whoever_robbed_their_village() {
    let mut w = world(true);
    let robber = w.player_uuid(0xAAAA, 0x1111);
    let friend = w.player_uuid(0xBBBB, 0x2222);
    w.run_until(Tick::from_days(5));
    assert!(w.sorcerer_includes(MIREWATER, robber));
    let key = caravan_out_of(&mut w, MIREWATER);
    let c = w.promote(key).expect("on the road");
    w.rob(c, robber, 1000).expect("robbed");
    // The caravan carries it home (or on, then home); within a few days its home knows.
    let mut t = w.now();
    while !w.knowledge.knows(
        w.nodes[MIREWATER].actor,
        w.knowledge.reports().iter().last().unwrap().0,
    ) {
        t += 1000;
        w.run_until(t);
        assert!(t < Tick::from_days(40), "Mirewater never heard");
    }
    assert!(
        !w.sorcerer_includes(MIREWATER, robber),
        "the robber is still buffed"
    );
    assert!(w.sorcerer_includes(MIREWATER, friend));
    assert_eq!(
        w.sorcerer_filter(MIREWATER, &[robber, friend]),
        vec![false, true]
    );
    // The snapshot names the robber by UUID, below the threshold, and was sent as a command.
    let snap = w.sentiment_snapshot(MIREWATER).unwrap();
    let e = snap
        .entries
        .iter()
        .find(|e| (e.0, e.1) == (0xAAAA, 0x1111))
        .expect("the robber is in the snapshot");
    assert!(e.2 <= snap.dislike_at_or_below);
    let cmds = w.take_commands();
    assert!(
        cmds.iter()
            .any(|c| matches!(c, Command::Sentiment(s) if s.village == w.nodes[MIREWATER].key))
    );
    w.check_conserved(w.now()).unwrap();
}

#[test]
fn prosperous_ribbit_villages_grow_a_house_at_a_time() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::AtTarget;
    cfg.growth_after_days = 3;
    cfg.growth_max_houses = 2;
    let (mut w, ids) = synth::build(cfg, true);
    let pops: Vec<u32> = w.nodes.iter().map(|n| n.local.pop).collect();
    let bread_eaten: Vec<i64> = (0..w.nodes.len())
        .map(|i| w.market(i).good(ids.bread).household)
        .collect();
    let mbar: Vec<i64> = (0..w.nodes.len())
        .map(|i| {
            let m = w.market(i);
            m.currency.map_or(0, |c| m.line(c).target)
        })
        .collect();
    w.run_until(Tick::from_days(60));
    let cmds = w.take_commands();
    let houses: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Command::PlaceHouse(h) => Some(h.clone()),
            _ => None,
        })
        .collect();
    assert!(!houses.is_empty(), "no ribbit village grew in 60 days");
    for h in &houses {
        let i = w.node_of_key(h.village).unwrap();
        assert!(
            matches!(
                w.nodes[i].kind,
                rtsim_settle::NodeKind::Village(rtsim_settle::Culture::Ribbit)
            ),
            "only ribbit villages grow"
        );
        assert!(h.template < 14);
        assert_eq!(h.op >> 63, 0, "engine op ids keep the top bit clear");
    }
    for (i, n) in w.nodes.iter().enumerate() {
        let grown = w.growth.get(&i).map_or(0, |g| g.houses);
        assert!(grown <= 2, "the cap holds");
        assert_eq!(
            n.local.pop,
            pops[i] + grown,
            "population steps up with each house"
        );
        if grown > 0 {
            // More mouths: more bread eaten, and a bigger treasury target.
            let m = w.market(i);
            assert!(m.good(ids.bread).household > bread_eaten[i]);
            assert!(m.line(m.currency.unwrap()).target > mbar[i]);
        }
    }
    // The bridge places a house; its resident is spawned.
    let n = w.house_placed(houses[0].op);
    assert_eq!(n, 1);
    assert!(
        w.take_commands()
            .iter()
            .any(|c| matches!(c, Command::SpawnResident(r) if r.house == houses[0].op))
    );
    assert_eq!(w.house_placed(houses[0].op), 0, "a house is placed once");
    w.check_conserved(w.now()).unwrap();
}

#[test]
fn bands_tour_ribbit_villages_and_carry_news() {
    let mut w = world(false);
    assert!(!w.bands.is_empty(), "ribbit villages have bands");
    for (b, band) in w.bands.iter().enumerate() {
        assert!(band.route.len() >= 2);
        // Ribbit villages and border human villages (P16); no base is marked here.
        for &s in &band.route {
            assert!(w.band_may_visit(band.home, s, w.now()));
            assert!(matches!(
                w.nodes[s].kind,
                rtsim_settle::NodeKind::Village(_)
            ));
        }
        // The circuit is a closed form: at its start it stays home; a stay later it walks.
        let q = w.cfg.trade.quantum.len();
        let v0 = w.band_view(b, Tick(band.start * q));
        assert_eq!(v0.staying.map(|s| s.0), Some(band.home));
        let v1 = w.band_view(b, Tick((band.start + band.stay) * q + 1));
        assert!(v1.walking.is_some());
        // And it comes back to the same place a circuit later.
        let p = band.period() * q;
        let later = w.band_view(b, Tick(band.start * q + 5 * p + 7));
        let now = w.band_view(b, Tick(band.start * q + 7));
        assert_eq!((later.x, later.z), (now.x, now.z));
    }
    // News: a report Mirewater witnesses reaches Bogsend, and some packets ride bands.
    w.run_until(Tick::from_days(5));
    let player = w.player(9);
    let key = caravan_out_of(&mut w, MIREWATER);
    let c = w.promote(key).unwrap();
    let report = w.rescue(c, player).unwrap();
    let mut t = w.now();
    while !w.knowledge.knows(w.nodes[BOGSEND].actor, report) {
        t += 1000;
        w.run_until(t);
        assert!(t < w.now() + 30 * TICKS_PER_DAY);
    }
    assert!(w.stats.band_packets > 0, "no band carried any news");
    // Somewhere a band is playing now or soon.
    let mut playing = false;
    for h in 0..48 {
        w.run_until(w.now() + 1000 * h.min(1));
        playing |= (0..w.nodes.len()).any(|i| !w.bands_at(i).is_empty());
    }
    assert!(playing);
}

#[test]
fn caravans_stop_and_trade_at_a_marked_base() {
    let mut w = world(true);
    let player = w.player_uuid(1, 2);
    w.run_until(Tick::from_days(10));
    let farm = w.nodes[FARM].clone_key_pos();
    let node = w.mark_base(farm.0, farm.1, player, w.now());
    assert_eq!(node, FARM);
    let stops: Vec<u32> = (0..w.edges.len() as u32)
        .filter(|&e| w.edges[e as usize].stops.contains(&FARM))
        .collect();
    assert!(!stops.is_empty(), "no road passes near the farm");
    // Wait for a traveller on one of those roads with an offer, near the farm.
    let mut t = w.now();
    let traded = loop {
        t += 1000;
        w.run_until(t);
        w.set_player_pos(player, farm.1, t);
        let mut done = None;
        for v in w.visible_to(player) {
            let Visible::Traveller { traveller, .. } = v else {
                continue;
            };
            assert!(traveller.stops.contains(&FARM) || !stops.contains(&traveller.key.edge));
            if !traveller.stops.contains(&FARM) {
                continue;
            }
            let r = CaravanRef::Sampled(traveller.key);
            let offers = w.caravan_offers(r, player);
            if let Some(o) = offers.first().copied() {
                let side = if o.side == OfferSide::MarketSells {
                    Side::Buy
                } else {
                    Side::Sell
                };
                done = Some(
                    w.caravan_trade(r, Some(FARM), player, o.good, side, 1)
                        .unwrap(),
                );
                break;
            }
        }
        if let Some(d) = done {
            break d;
        }
        assert!(t < Tick::from_days(80), "no caravan traded at the farm");
    };
    // The caravan is a row now, the farm has a price, and its news goes out.
    assert!(w.caravans.get(traded.0).is_some());
    assert!(w.market(FARM).base_prices.iter().any(|p| p.is_some()));
    assert!(traded.1.items > 0 && traded.1.coins > 0);
    w.check_conserved(w.now()).unwrap();
    // Unmarked by its only player, the farm is no longer a stop.
    w.unmark_base(farm.0, player);
    assert!(w.edges.iter().all(|e| !e.stops.contains(&FARM)));
    let _ = MILLI;
}

#[test]
fn players_see_the_travellers_on_the_roads_around_them() {
    let mut w = world(false);
    w.run_until(Tick::from_days(15));
    let player = w.player_uuid(5, 6);
    // Stand on every road's midpoint in turn: every traveller seen is within the radius, and
    // every traveller on that road within it is seen.
    let r = w.cfg.view_blocks;
    let mut seen_any = false;
    for e in 0..w.edges.len() {
        let (a, b) = (w.nodes[w.edges[e].a].pos, w.nodes[w.edges[e].b].pos);
        let mid = Pos::new(a.frame, (a.x + b.x) / 2, (a.z + b.z) / 2);
        w.set_player_pos(player, mid, w.now());
        let vis = w.visible_to(player);
        for v in &vis {
            let (x, z) = v.pos();
            let d2 = (x - i64::from(mid.x)).pow(2) + (z - i64::from(mid.z)).pow(2);
            assert!(d2 <= i64::from(r).pow(2));
        }
        for dir in 0..2u8 {
            for t in w.travellers(e as u32, dir) {
                let (x, z) = w.traveller_pos(&t);
                let d2 = (x - i64::from(mid.x)).pow(2) + (z - i64::from(mid.z)).pow(2);
                if d2 <= i64::from(r).pow(2) {
                    seen_any = true;
                    assert!(
                        vis.iter()
                            .any(|v| matches!(v, Visible::Traveller { traveller, .. } if traveller.key == t.key)),
                        "a traveller in sight was not reported"
                    );
                }
            }
        }
    }
    assert!(seen_any, "nobody on any road at day 15");
    // An individual caravan is found through the leg index, and the player's booked area meets
    // it when the player stands on its road ahead of it.
    let key = caravan_out_of(&mut w, 0);
    let c = w.promote(key).unwrap();
    let car = w.caravans.get(c).unwrap().clone();
    let (a, b) = (w.nodes[car.leg.from].pos, w.nodes[car.leg.to].pos);
    let now = w.now();
    let ahead = |f: i32| {
        Pos::new(
            a.frame,
            a.x + (b.x - a.x) * f / 100,
            a.z + (b.z - a.z) * f / 100,
        )
    };
    let here = w.caravan_place(&car, now);
    w.set_player_pos(player, Pos::new(a.frame, here.x, here.z), now);
    assert!(
        w.visible_to(player)
            .iter()
            .any(|v| matches!(v, Visible::Caravan { id, .. } if *id == c))
    );
    // Where it will be a little later: the booked areas meet within the window.
    let p = car.progress_milli(now) as i32 / 10;
    w.set_player_pos(player, ahead((p + 3).min(100)), now);
    let met = w.encounters(player);
    assert!(met.iter().any(|m| m.1 == c), "{met:?}");
}

trait KeyPos {
    fn clone_key_pos(&self) -> (u64, Pos);
}

impl KeyPos for rtsim_world::world::Node {
    fn clone_key_pos(&self) -> (u64, Pos) {
        (self.key, self.pos)
    }
}
