// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase 1's event archive rows (VIEWER.md §10.1): a village joining, a base marked and unmarked,
//! houses, caravans as rows and their legs, bands' walks, trade shifts, and the knowledge reports
//! the world makes, each under a registered kind of its group, at its place, with real ids.

use std::collections::BTreeMap;

use rtsim_archive::kinds::{self, Kind};
use rtsim_archive::record::{kinds_on, rows_on};
use rtsim_archive::{Row, Severity};
use rtsim_kernel::clock::Tick;
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::build::Genesis;
use rtsim_world::{NodeData, World, WorldConfig, synth};

const MIREWATER: usize = 6;
const FARM: usize = 10;

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
fn phase_one_records_its_events_where_they_happen() {
    let mut cfg = WorldConfig::new(42);
    cfg.genesis = Genesis::AtTarget;
    cfg.growth_after_days = 3;
    cfg.growth_max_houses = 2;
    let (mut w, _) = synth::build(cfg, true);
    assert!(
        w.tape.is_empty(),
        "off by default: the build records nothing"
    );
    w.tape.set_on(true);
    w.knowledge.tape.set_on(true);
    let player = w.player_uuid(1, 2);
    w.run_until(Tick::from_days(5));
    let joined = w.join(
        NodeData {
            key: 99,
            kind: NodeKind::Village(Culture::Human),
            pos: Pos::new(0, 900, 900),
            pop: 8,
            resources: BTreeMap::new(),
        },
        w.now(),
    );
    let key = caravan_out_of(&mut w, MIREWATER);
    let c = w.promote(key).expect("on the road");
    w.rob(c, player, 1000).expect("robbed");
    let (fk, fp) = (w.nodes[FARM].key, w.nodes[FARM].pos);
    w.mark_base(fk, fp, player, w.now());
    w.run_until(Tick::from_days(60));
    w.unmark_base(fk, player);

    let got = kinds_on(&w.tape);
    for k in &got {
        assert!(
            ["settle", "travel", "trade"].contains(&k.group()),
            "{} recorded by the world",
            k.name()
        );
    }
    let want: [Kind; 9] = [
        kinds::SETTLE_JOINED,
        kinds::SETTLE_BASE,
        kinds::SETTLE_UNMARKED,
        kinds::SETTLE_HOUSE,
        kinds::TRAVEL_CARAVAN,
        kinds::TRAVEL_CARAVAN_LEG,
        kinds::TRAVEL_ARRIVED,
        kinds::TRAVEL_BAND_LEG,
        kinds::TRADE_SHIFT,
    ];
    for k in want {
        assert!(got.contains(&k), "no {} in {got:?}", k.name());
    }
    assert!(kinds_on(&w.knowledge.tape).contains(&kinds::KNOWLEDGE_ROBBERY));

    let rows = rows_on(&w.tape);
    let of = |k: Kind| rows.iter().filter(move |r| r.0 == k).map(|r| r.2);
    // The village joined at its place, by its key, once.
    let j: Vec<_> = of(kinds::SETTLE_JOINED).collect();
    assert_eq!(j.len(), 1);
    let Row::Event(e) = j[0] else { panic!() };
    assert_eq!((e.x, e.z, e.subject.value, e.magnitude), (900, 900, 99, 8));
    assert_eq!(w.nodes[joined].key, 99);
    // The base: marked and unmarked once, by the player's actor.
    for k in [kinds::SETTLE_BASE, kinds::SETTLE_UNMARKED] {
        let v: Vec<_> = of(k).collect();
        assert_eq!(v.len(), 1, "{}", k.name());
        let Row::Event(e) = v[0] else { panic!() };
        assert_eq!((e.subject.value, e.object.value), (fk, player.to_bits()));
        assert_eq!((e.x, e.z), (fp.x, fp.z));
    }
    // The caravan row: one leg per decision (out, and back from where it turned or arrived), each
    // a straight piece in time order; it set out and arrived.
    let legs: Vec<_> = of(kinds::TRAVEL_CARAVAN_LEG)
        .filter_map(|r| match r {
            Row::Leg(l) if l.subject.value == c.to_bits() => Some(l),
            _ => None,
        })
        .collect();
    assert!(legs.len() >= 2, "{legs:?}");
    assert!(legs.windows(2).all(|p| p[0].t0 <= p[1].t0));
    assert!(legs.iter().all(|l| l.duration > 0));
    assert!(
        of(kinds::TRAVEL_ARRIVED)
            .any(|r| matches!(r, Row::Event(e) if e.subject.value == c.to_bits()))
    );
    // Bands walk between villages: legs with real ends, routine.
    for r in of(kinds::TRAVEL_BAND_LEG) {
        let Row::Leg(l) = r else { panic!() };
        assert_eq!(l.severity, Severity::Routine);
        assert!((l.x0, l.z0) != (l.x1, l.z1) && l.duration > 0, "{l:?}");
    }
    // Trade shifts carry the index, and a payload only when notable.
    for r in of(kinds::TRADE_SHIFT) {
        let Row::Event(e) = r else { panic!() };
        assert!(e.magnitude > 0);
        assert_eq!(e.payload, e.severity >= Severity::Notable);
    }
}
