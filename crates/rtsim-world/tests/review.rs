// SPDX-License-Identifier: AGPL-3.0-or-later
//! Regression tests for the adversarial review of round 2: cross-border imports are paid,
//! a promoted caravan is seen once, a caravan's trades with players go home, and malformed keys
//! from the bridge are refused, not a panic.

use rtsim_kernel::clock::Tick;
use rtsim_settle::Pos;
use rtsim_trade::PRICE_ONE;
use rtsim_trade::build::Genesis;
use rtsim_trade::line::band;
use rtsim_travel::CaravanKey;
use rtsim_world::bases::CaravanRef;
use rtsim_world::{Side, Visible, World, WorldConfig, synth};

fn world(seed: u64) -> World {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    synth::build(cfg, true).0
}

/// Times a payer with money at home paid less than half of a cross-border bill.
fn unpaid(w: &World) -> u64 {
    let nb = w.cfg.trade.quantum.index(w.now()) + 1;
    let mut hits = 0;
    for i in 0..w.nodes.len() {
        let Some(hc) = w.nodes[i].currency else {
            continue;
        };
        let m = w.market(i);
        if !m.is_storehouse() {
            continue;
        }
        for d in &w.nodes[i].out {
            let e = &w.edges[d.edge as usize];
            let (din, dout) = (&e.dirs[1 - d.dir as usize], &e.dirs[d.dir as usize]);
            let Some(tc) = w.nodes[din.from].currency else {
                continue;
            };
            if tc == hc {
                continue;
            }
            let value: i128 = din
                .goods
                .values()
                .map(|p| {
                    let a = p.arrival_at(nb);
                    if a > 0 {
                        i128::from(a) * i128::from(p.arrival_quote(nb)) / i128::from(PRICE_ONE)
                    } else {
                        0
                    }
                })
                .sum();
            if value <= 0 || m.stock(tc) <= 0 || m.price(tc) <= 0 {
                continue;
            }
            let ptc = dout.pay.get(&tc).map_or(0, |p| p.rate_at(nb));
            let phc = dout.pay.get(&hc).map_or(0, |p| p.rate_at(nb));
            let paid =
                i128::from(ptc) * i128::from(m.price(tc)) / i128::from(PRICE_ONE) + i128::from(phc);
            let home = m.line(hc);
            if home.stock >= home.target - band(home.target) && paid * 2 < value {
                hits += 1;
            }
        }
    }
    hits
}

#[test]
fn cross_border_imports_are_paid() {
    let mut hits = 0;
    let mut samples = 0;
    for seed in [1u64, 7] {
        let mut w = world(seed);
        for h in 1..=24 * 120u64 {
            w.run_until(Tick(h * 1000 + 250));
            if h % 6 == 0 {
                hits += unpaid(&w);
                samples += 1;
            }
        }
        w.check_conserved(w.now()).unwrap();
    }
    // The review measured 12,552 over 200 days and four seeds before the fix.
    assert!(hits <= samples / 100, "{hits} unpaid in {samples} samples");
}

/// A loaded traveller out of `from`, out of sight of its village.
fn caravan_out_of(w: &mut World, from: usize) -> CaravanKey {
    let mut t = w.now();
    loop {
        for d in w.nodes[from].out.clone() {
            for tr in w.travellers(d.edge, d.dir) {
                if tr.cargo.values().sum::<i64>() > 0
                    && tr.progress_milli > 200
                    && tr.progress_milli < 700
                {
                    return tr.key;
                }
            }
        }
        t += 1000;
        w.run_until(t);
        assert!(t < Tick::from_days(200));
    }
}

#[test]
fn a_promoted_caravan_is_seen_once() {
    let mut w = world(42);
    w.run_until(Tick::from_days(15));
    let player = w.player_uuid(5, 6);
    let key = caravan_out_of(&mut w, 0);
    let c = w.promote(key).unwrap();
    assert_eq!(w.promote(key), None, "a caravan is promoted once");
    assert_eq!(w.promoted_row(key), Some(c));
    assert!(w.traveller(key).is_none());
    let car = w.caravans.get(c).unwrap().clone();
    let here = w.caravan_place(&car, w.now());
    let now = w.now();
    w.set_player_pos(player, Pos::new(here.frame, here.x, here.z), now);
    let vis = w.visible_to(player);
    assert!(
        vis.iter()
            .any(|v| matches!(v, Visible::Caravan { id, .. } if *id == c))
    );
    assert!(
        !vis.iter()
            .any(|v| matches!(v, Visible::Traveller { traveller, .. } if traveller.key == key)),
        "a ghost of the promoted caravan"
    );
}

#[test]
fn a_caravans_trades_with_players_go_home() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(500);
    let (mut w, _) = synth::build(cfg, true);
    w.run_until(Tick::from_days(15));
    let player = w.player_uuid(1, 1);
    // A caravan with something to sell.
    let (key, good) = loop {
        let key = caravan_out_of(&mut w, 1);
        let offer = w
            .caravan_offers(CaravanRef::Sampled(key), player)
            .into_iter()
            .find(|o| o.side == rtsim_trade::OfferSide::MarketSells);
        if let Some(o) = offer {
            break (key, o.good);
        }
        let t = w.now();
        w.run_until(t + 1000);
    };
    let (c, q) = w
        .caravan_trade(CaravanRef::Sampled(key), None, player, good, Side::Buy, 1)
        .unwrap();
    let car = w.caravans.get(c).unwrap().clone();
    let cur = w.nodes[car.home].currency.unwrap();
    assert_eq!(car.purse.get(&cur).copied(), Some(q.coins * 1000));
    // With the coin it took, it now buys from players for home.
    let dest = car.leg.to;
    let lump_dest_before = w.market(dest).line(cur).book.lump_in;
    // Walk it to its destination: the purse stays with it.
    let arrive = car.leg.end;
    w.run_until(arrive);
    if let Some(car) = w.caravans.get(c).cloned() {
        assert_eq!(car.purse.get(&cur).copied(), Some(q.coins * 1000));
        let home_before = w.market(car.home).line(cur).book.lump_in;
        w.run_until(car.leg.end);
        assert!(w.market(car.home).line(cur).book.lump_in >= home_before + q.coins * 1000);
    }
    // The destination was not handed the player's coin with the cargo's payments.
    let lump_dest = w.market(dest).line(cur).book.lump_in - lump_dest_before;
    assert!(lump_dest < q.coins * 1000 || dest == w.caravans.get(c).map_or(dest, |x| x.home));
    w.check_conserved(w.now()).unwrap();
}

#[test]
fn malformed_keys_are_refused() {
    let mut w = world(42);
    w.run_until(Tick::from_days(5));
    let player = w.player(3);
    for key in [
        CaravanKey {
            edge: 9_999,
            dir: 0,
            departure: 3,
        },
        CaravanKey {
            edge: 0,
            dir: 7,
            departure: 3,
        },
    ] {
        assert!(w.travellers(key.edge, key.dir).is_empty());
        assert!(w.traveller(key).is_none());
        assert!(w.promote(key).is_none());
        assert!(
            w.caravan_offers(CaravanRef::Sampled(key), player)
                .is_empty()
        );
        let g = rtsim_trade::GoodId(0);
        assert!(
            w.caravan_trade(CaravanRef::Sampled(key), None, player, g, Side::Buy, 1)
                .is_err()
        );
    }
}
