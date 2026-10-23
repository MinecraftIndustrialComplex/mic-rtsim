// SPDX-License-Identifier: AGPL-3.0-or-later
//! Knowledge as the world uses it: witnesses, packets on roads, vanilla gossip, expiry, heirs and
//! determinism.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::Encode;
use rtsim_knowledge::{
    ActorId, ActorKind, BoardCursor, GossipType, HolderKind, Knowledge, Place, Report, ReportKind,
    SEVERITY_UNIT, Tunables, witnesses,
};

struct World {
    k: Knowledge,
    player: ActorId,
    oakford: ActorId,
    brindle: ActorId,
    mirewater: ActorId,
}

fn world(seed: u64) -> World {
    let mut k = Knowledge::new(seed);
    let player = k.actors_mut().register(ActorKind::Player, 0xbeef);
    let oakford = k.actors_mut().register(ActorKind::Settlement, 1);
    let brindle = k.actors_mut().register(ActorKind::Settlement, 2);
    let mirewater = k.actors_mut().register(ActorKind::Settlement, 3);
    k.add_holder(oakford, HolderKind::VanillaVillage);
    k.add_holder(brindle, HolderKind::VanillaVillage);
    k.add_holder(mirewater, HolderKind::RibbitVillage);
    World {
        k,
        player,
        oakford,
        brindle,
        mirewater,
    }
}

fn robbery(w: &World, victim: ActorId, at: u64) -> Report {
    Report {
        who: w.player,
        whom: victim,
        kind: ReportKind::Robbery,
        severity: SEVERITY_UNIT,
        flags: 0,
        at: Tick(at),
        place: Place::new(0, 100, 64, 200),
    }
}

#[test]
fn news_moves_only_in_packets() {
    let mut w = world(1);
    let id = w.k.record(robbery(&w, w.oakford, 10));
    assert!(w.k.report(id).unwrap().flags & rtsim_knowledge::FLAG_PLAYER != 0);
    // Oakford's own caravan was robbed: it knows first-hand when the trader gets home.
    assert!(w.k.witness(w.oakford, id, Tick(100)));
    assert!(!w.k.witness(w.oakford, id, Tick(101)), "not new twice");
    assert!(w.k.sentiment(w.oakford, w.player, Tick(100)) < 0);
    // Mirewater has not heard: nothing has carried it there.
    assert!(!w.k.knows(w.mirewater, id));
    assert_eq!(w.k.sentiment(w.mirewater, w.player, Tick(200)), 0);
    // A caravan leaves Oakford for Brindle carrying what changed on Oakford's board.
    let mut road = BoardCursor::default();
    let packet = w.k.packet_since(w.oakford, &mut road);
    assert_eq!(packet.len(), 1);
    assert!(
        w.k.packet_since(w.oakford, &mut road).is_empty(),
        "only what changed"
    );
    assert_eq!(w.k.deliver(w.brindle, &packet, Tick(300)), 1);
    assert_eq!(w.k.deliver(w.brindle, &packet, Tick(301)), 0, "deduped");
    // Hearsay counts less than first-hand.
    let first = w.k.sentiment(w.oakford, w.player, Tick(300));
    let heard = w.k.sentiment(w.brindle, w.player, Tick(300));
    assert!(
        heard < 0 && heard > first,
        "first-hand {first}, hearsay {heard}"
    );
    // Brindle relays on; Mirewater learns only from a Brindle packet.
    let mut road2 = BoardCursor::default();
    let p2 = w.k.packet_since(w.brindle, &mut road2);
    assert_eq!(w.k.deliver(w.mirewater, &p2, Tick(500)), 1);
    assert!(w.k.knows(w.mirewater, id));
}

#[test]
fn vanilla_gossip_writes_only_what_vanilla_cannot_know() {
    let mut w = world(2);
    // A robbery on the road reaching a vanilla village first-hand: written.
    let rob = w.k.record(robbery(&w, w.oakford, 0));
    w.k.witness(w.oakford, rob, Tick(1));
    // A trade the village's own villager made: vanilla already wrote it.
    let trade = w.k.record(Report {
        kind: ReportKind::Trade,
        ..robbery(&w, w.oakford, 2)
    });
    w.k.witness(w.oakford, trade, Tick(2));
    // A resident killed in Oakford: vanilla's own villagers saw it.
    let kill = w.k.record(Report {
        kind: ReportKind::ResidentKilling,
        ..robbery(&w, w.oakford, 3)
    });
    w.k.witness(w.oakford, kill, Tick(3));
    let g = w.k.take_vanilla_gossip();
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].report, rob);
    assert_eq!(g[0].gossip, GossipType::MajorNegative);
    assert_eq!(g[0].village, w.oakford);
    assert_eq!(g[0].player, w.player);
    // News of the trade and the killing from another village is written in Brindle, weaker.
    let mut c = BoardCursor::default();
    let p = w.k.packet_since(w.oakford, &mut c);
    assert_eq!(w.k.deliver(w.brindle, &p, Tick(10)), 3);
    let g = w.k.take_vanilla_gossip();
    assert_eq!(g.len(), 3);
    assert!(g.iter().all(|x| x.village == w.brindle));
    let rob_heard = g.iter().find(|x| x.report == rob).unwrap();
    assert!(rob_heard.amount < 25 && rob_heard.amount > 0);
    // Ribbit villages have no vanilla gossip.
    w.k.deliver(w.mirewater, &p, Tick(11));
    assert!(w.k.take_vanilla_gossip().is_empty());
}

#[test]
fn rescues_raise_sentiment() {
    let mut w = world(3);
    let r = w.k.record(Report {
        kind: ReportKind::Rescue,
        ..robbery(&w, w.oakford, 0)
    });
    w.k.witness(w.oakford, r, Tick(0));
    assert!(w.k.sentiment(w.oakford, w.player, Tick(0)) > 0);
}

#[test]
fn reports_expire_into_counters() {
    let mut w = world(4);
    let id = w.k.record(robbery(&w, w.oakford, 0));
    w.k.witness(w.oakford, id, Tick(0));
    let mut c = BoardCursor::default();
    let p = w.k.packet_since(w.oakford, &mut c);
    w.k.deliver(w.brindle, &p, Tick(10));
    // Robbery lives 6 days (P18, lengthened now that news travels separately; TUNABLE).
    let life = TICKS_PER_DAY * 6;
    assert_eq!(w.k.tunables().lifetime(ReportKind::Robbery), life);
    assert_eq!(w.k.next_expiry(), Some(Tick(life)));
    assert_eq!(w.k.expire_due(Tick(life - 1)), 0);
    assert_eq!(w.k.expire_due(Tick(life)), 1);
    assert!(w.k.report(id).is_none());
    assert_eq!(w.k.counter(w.player, w.oakford, ReportKind::Robbery), 1);
    assert_eq!(w.k.counter(w.player, w.brindle, ReportKind::Robbery), 1);
    assert_eq!(w.k.counter(w.player, w.mirewater, ReportKind::Robbery), 0);
    assert!(w.k.board(w.oakford).unwrap().is_empty());
    // The feeling outlives the detail.
    assert!(w.k.sentiment(w.oakford, w.player, Tick(life)) < 0);
    // A packet still naming the expired report delivers nothing.
    assert_eq!(w.k.deliver(w.mirewater, &p, Tick(life + 1)), 0);
}

#[test]
fn a_killed_traders_grudge_lands_on_its_home_market() {
    let mut w = world(5);
    let trader =
        w.k.create_notable(77, w.oakford, ActorId::NULL, Tick(0), 40 * TICKS_PER_DAY);
    assert_eq!(w.k.actors().kind(trader), Some(ActorKind::Notable));
    let bandit = w.k.actors_mut().register(ActorKind::Player, 0xbad);
    // The trader is robbed by the player and assaulted by another player, and likes a third.
    let rob = w.k.record(robbery(&w, trader, 10));
    w.k.witness(trader, rob, Tick(10));
    let assault = w.k.record(Report {
        who: bandit,
        kind: ReportKind::Assault,
        severity: 10,
        ..robbery(&w, trader, 11)
    });
    w.k.witness(trader, assault, Tick(11));
    let friend = w.k.actors_mut().register(ActorKind::Player, 0xf00d);
    w.k.adjust(trader, friend, 3, Tick(12));
    let t = Tunables {
        heir_sentiments: 1,
        heir_memories: 1,
        ..Tunables::default()
    };
    // Heirs inherit the strongest: with the default (4) everything comes across, so check the
    // ranking directly with a fresh world whose tunables keep one.
    let strongest =
        w.k.sentiments(trader)
            .unwrap()
            .strongest(1, Tick(20), w.k.world_seed(), w.k.tunables());
    assert_eq!(strongest[0].0, w.player);
    // The trader is killed on the road: no heir, so its home market inherits.
    assert_eq!(w.k.die(trader, Tick(20)), Some(w.oakford));
    assert!(w.k.notable(trader).is_none());
    assert_eq!(w.k.actors().kind(trader), None);
    assert!(w.k.sentiment(w.oakford, w.player, Tick(20)) < 0);
    assert!(w.k.knows(w.oakford, rob), "the memory passes on");
    assert!(!w.k.knows(trader, rob));

    // With one inherited feeling and memory, the heir gets exactly the strongest.
    let mut k = Knowledge::with_tunables(5, t);
    let player = k.actors_mut().register(ActorKind::Player, 0xbeef);
    let bandit = k.actors_mut().register(ActorKind::Player, 0xbad);
    let home = k.actors_mut().register(ActorKind::Settlement, 1);
    k.add_holder(home, HolderKind::VanillaVillage);
    let n = k.create_notable(9, home, ActorId::NULL, Tick(0), 1000);
    let heir = k.create_notable(10, home, ActorId::NULL, Tick(0), 1_000_000);
    k.set_heir(n, heir);
    let big = k.record(Report {
        who: player,
        whom: n,
        kind: ReportKind::Killing,
        severity: 100,
        flags: 0,
        at: Tick(1),
        place: Place::default(),
    });
    let small = k.record(Report {
        who: bandit,
        whom: n,
        kind: ReportKind::Assault,
        severity: 20,
        flags: 0,
        at: Tick(2),
        place: Place::default(),
    });
    k.witness(n, big, Tick(1));
    k.witness(n, small, Tick(2));
    // Death of age is one booked event.
    assert_eq!(k.next_death(), Some(Tick(1000)));
    assert_eq!(k.death_due(Tick(999)), None);
    assert_eq!(k.death_due(Tick(1000)), Some(n));
    assert_eq!(k.die(n, Tick(1000)), Some(heir));
    assert!(k.sentiment(heir, player, Tick(1000)) < 0);
    assert_eq!(
        k.sentiment(heir, bandit, Tick(1000)),
        0,
        "only the strongest"
    );
    let mem: Vec<_> = k
        .notable(heir)
        .unwrap()
        .memories()
        .iter()
        .map(|m| m.report())
        .collect();
    assert_eq!(mem, vec![big]);
    assert!(!k.knows(heir, small));
}

#[test]
fn witnesses_are_within_radius_in_the_same_frame() {
    let mut k = Knowledge::new(6);
    let a = k.actors_mut().register(ActorKind::Group, 1);
    let b = k.actors_mut().register(ActorKind::Group, 2);
    let c = k.actors_mut().register(ActorKind::Group, 3);
    let at = Place::new(0, 0, 64, 0);
    let got = witnesses(
        at,
        k.tunables().witness_radius,
        [
            (a, Place::new(0, 20, 64, 20)),
            (b, Place::new(0, 40, 64, 0)),
            (c, Place::new(1, 0, 64, 0)),
        ],
    );
    assert_eq!(got, vec![a]);
}

#[test]
fn caps_hold_under_load() {
    let mut t = Tunables {
        report_cap: 50,
        ..Tunables::default()
    };
    t.sentiment_cap[HolderKind::VanillaVillage.index()] = 8;
    t.board_cap[HolderKind::VanillaVillage.index()] = 16;
    let mut k = Knowledge::with_tunables(7, t);
    let home = k.actors_mut().register(ActorKind::Settlement, 1);
    k.add_holder(home, HolderKind::VanillaVillage);
    let mut strongest_player = None;
    for i in 0..500u64 {
        let who = k.actors_mut().register(ActorKind::Player, i);
        let severity = if i == 3 { 250 } else { 1 + (i % 50) as u8 };
        if i == 3 {
            strongest_player = Some(who);
        }
        let id = k.record(Report {
            who,
            whom: home,
            kind: ReportKind::Killing,
            severity,
            flags: 0,
            at: Tick(i),
            place: Place::default(),
        });
        k.witness(home, id, Tick(i));
        assert!(k.reports().len() <= 50);
        assert!(k.board(home).unwrap().len() <= 16);
        assert!(k.sentiments(home).unwrap().len() <= 8);
    }
    // The strongest grudge survived 497 later, weaker ones.
    assert!(k.sentiment(home, strongest_player.unwrap(), Tick(500)) < -100);
}

#[test]
fn same_seed_same_state() {
    let run = |seed: u64| {
        let mut w = world(seed);
        for i in 0..40u64 {
            let victim = if i % 2 == 0 { w.oakford } else { w.brindle };
            let id = w.k.record(robbery(&w, victim, i * 997));
            w.k.witness(victim, id, Tick(i * 997));
            let mut c = BoardCursor::default();
            let p = w.k.packet_since(victim, &mut c);
            w.k.deliver(w.mirewater, &p, Tick(i * 997 + 500));
            w.k.expire_due(Tick(i * 997));
        }
        w.k.to_bytes()
    };
    assert_eq!(run(11), run(11));
    // The world seed reaches decay through the sentiment draws and is written in the state.
    assert_ne!(run(11), run(12));
}

/// The TALK news kinds (TALK.md §9.4) are news and never a grudge: learning one, first-hand or as
/// hearsay, writes no sentiment toward `who` (even a player) and no vanilla gossip, and a village
/// keeps it on its board for the lifetime the table gives it.
#[test]
fn news_kinds_write_no_sentiment_and_no_vanilla_gossip() {
    let news = [
        ReportKind::Feud,
        ReportKind::PactSworn,
        ReportKind::PactBroken,
        ReportKind::Raid,
        ReportKind::Fell,
        ReportKind::PortRaided,
        ReportKind::Blockade,
        ReportKind::PortClosed,
        ReportKind::PortReopened,
        ReportKind::HavenFell,
    ];
    let t = Tunables::default();
    let day = TICKS_PER_DAY;
    let lives = [10, 30, 30, 20, 30, 6, 6, 6, 6, 6];
    let mut w = world(7);
    // Every one is a player-involved report (the player is `who`), the case that would write the
    // most: a sentiment toward the player and a vanilla gossip entry.
    let mut ids = Vec::new();
    for (i, kind) in news.into_iter().enumerate() {
        assert_eq!(t.delta[kind.index()], 0, "{kind:?} has a sentiment delta");
        assert_eq!(t.delta(kind, 255, false), 0);
        assert_eq!(t.lifetime(kind), lives[i] * day, "{kind:?}");
        assert!(!kind.is_positive());
        assert!(!rtsim_knowledge::gossip::vanilla_sees_first_hand(kind));
        assert_eq!(
            rtsim_knowledge::gossip::vanilla_mapping(kind),
            None,
            "{kind:?}"
        );
        let id = w.k.record(Report {
            kind,
            severity: 255,
            ..robbery(&w, w.oakford, 10 + i as u64)
        });
        assert!(w.k.witness(w.oakford, id, Tick(100)), "{kind:?} first-hand");
        ids.push(id);
    }
    assert_eq!(w.k.sentiment(w.oakford, w.player, Tick(100)), 0);
    assert!(
        w.k.sentiments(w.oakford).unwrap().is_empty(),
        "no sentiment entry at all"
    );
    assert!(w.k.take_vanilla_gossip().is_empty(), "first-hand");
    // Hearsay in a vanilla village and a ribbit village: still nothing.
    let mut road = BoardCursor::default();
    let packet = w.k.packet_since(w.oakford, &mut road);
    assert_eq!(packet.len(), news.len());
    assert_eq!(w.k.deliver(w.brindle, &packet, Tick(200)), news.len());
    assert_eq!(w.k.deliver(w.mirewater, &packet, Tick(200)), news.len());
    for h in [w.brindle, w.mirewater] {
        assert_eq!(w.k.sentiment(h, w.player, Tick(200)), 0);
        assert!(w.k.sentiments(h).unwrap().is_empty());
    }
    assert!(w.k.take_vanilla_gossip().is_empty(), "hearsay");
    // The news is on the board and survives to its lifetime, then expires like any report.
    for id in &ids {
        assert!(w.k.knows(w.oakford, *id));
    }
    // A control: an old kind on the same village still writes both.
    let rob = w.k.record(robbery(&w, w.oakford, 300));
    w.k.witness(w.oakford, rob, Tick(300));
    assert!(w.k.sentiment(w.oakford, w.player, Tick(300)) < 0);
    assert_eq!(w.k.take_vanilla_gossip().len(), 1);
}

/// The event archive (VIEWER.md §10.1): every report is archived as it is made, at its own place,
/// who did it to whom, under a registered knowledge kind; nothing while the tape is off.
#[test]
fn every_report_is_archived_as_made() {
    use rtsim_archive::record::{kinds_on, rows_on};
    use rtsim_archive::{Row, kinds};
    let mut w = world(4);
    let base = robbery(&w, w.oakford, 0);
    let report = |kind, at| Report {
        kind,
        at: Tick(at),
        ..base
    };
    w.k.record(report(ReportKind::Theft, 5));
    assert!(w.k.tape.is_empty(), "off by default");
    w.k.tape.set_on(true);
    for (i, kind) in ReportKind::ALL.into_iter().enumerate() {
        w.k.record(report(kind, 100 + i as u64));
    }
    let got = kinds_on(&w.k.tape);
    let want: Vec<_> = kinds::of_group("knowledge").collect();
    assert_eq!(got, want, "one registered kind per report kind");
    for (k, dim, r) in rows_on(&w.k.tape) {
        let Row::Event(e) = r else {
            panic!("{} is a leg", k.name())
        };
        assert_eq!((dim, e.x, e.z), (0, 100, 200));
        assert_eq!(e.subject.value, w.player.to_bits());
        assert_eq!(e.object.value, w.oakford.to_bits());
        assert_eq!(e.magnitude, i32::from(SEVERITY_UNIT));
    }
}
