// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b-3 (EQUILIBRIUM.md §2.4.6a rule 6; docs/choices-ports.md PC9, PC40 to PC47, PC72): pirates
//! raid port villages. A hunt's coast raid that finds no player district or train takes the port
//! village at the hunted lane's nearer end, if nobody watches it and the pirate believes it wins
//! (`p = A² / (A² + D²) > 2/3` against `pop x 0.25`). The square law costs the crew, booked as
//! the toll's; the survivors carry off 10 items a man, at most 600 per mille of each line's
//! surplus over its target (PC72), through `World::stock_to_held` into the hold, and the haven's
//! landing carries them home.
//!
//! The worlds are `synth::sea_havens` (12-person ports, so a full sea crew of 6 raids them) and,
//! for the rules that decline, `synth::coastal` (ports of 30 to 72, which no pirate raids).

use std::collections::BTreeMap;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{NodeKind, Pos};
use rtsim_ships::engine::PState;
use rtsim_ships::synth::{self, FakeShips, SeaHavens};
use rtsim_ships::{ShipTunables, Ships};
use rtsim_world::World;

const SEEDS: std::ops::RangeInclusive<u64> = 1..=30;

/// `f` on each seed, a few seeds at a time on their own threads, in seed order.
fn per_seed<T: Send>(seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 6));
    let chunk = seeds.len().div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        let hs: Vec<_> = seeds
            .chunks(chunk)
            .map(|c| {
                let f = &f;
                sc.spawn(move || c.iter().map(|&s| f(s)).collect::<Vec<T>>())
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("a seed's run panicked"))
            .collect()
    })
}

fn seeds() -> Vec<u64> {
    SEEDS.collect()
}

fn drive(s: &mut SeaHavens, fake: &mut FakeShips, to: Tick, dt: u64) {
    synth::drive(
        &mut s.world,
        &mut s.ships,
        fake,
        &mut NoInfrastructure,
        &s.mask,
        to,
        dt,
    );
}

/// Per good (by index), the sum over nodes `nodes` of `f` of each market line.
fn lines_sum(w: &World, nodes: &[usize], f: impl Fn(&rtsim_trade::line::Line) -> i64) -> Vec<i128> {
    let mut out = Vec::new();
    for &n in nodes {
        let m = w.market(n);
        if out.len() < m.lines.len() {
            out.resize(m.lines.len(), 0);
        }
        for (i, l) in m.lines.iter().enumerate() {
            out[i] += i128::from(f(l));
        }
    }
    out
}

fn by_good(m: &BTreeMap<rtsim_trade::GoodId, i128>, n: usize) -> Vec<i128> {
    let mut v = vec![0; n];
    for (g, a) in m {
        v[g.index()] += a;
    }
    v
}

fn village_nodes(w: &World) -> Vec<usize> {
    (0..w.nodes.len())
        .filter(|&i| matches!(w.nodes[i].kind, NodeKind::Village(_)))
        .collect()
}

/// Raises every line of node `node` to its target plus `extra` thousandths, as goods players
/// brought there (`World::stock_from_players`, so the books stay exact): a rich port whose
/// surplus a raid can take (PC72), for the tests that need raids big enough to fill a hold.
fn enrich(w: &mut World, node: usize, extra: i64, at: Tick) {
    let m = w.market(node);
    let goods: BTreeMap<_, _> = w
        .goods
        .ids()
        .map(|g| (g, m.line(g).target + extra - m.stock(g)))
        .filter(|e| e.1 > 0)
        .collect();
    w.stock_from_players(node, &goods, at);
}

fn haven_nodes(s: &Ships) -> Vec<usize> {
    s.havens.iter().map(|h| h.node).collect()
}

/// Pirates' cargo aboard, per good.
fn aboard(s: &Ships, n: usize) -> Vec<i128> {
    let mut v = vec![0; n];
    for hv in &s.havens {
        for &p in &hv.ships {
            if let Some(x) = s.ship(p) {
                for (g, a) in &x.cargo {
                    v[g.index()] += i128::from(*a);
                }
            }
        }
    }
    v
}

/// What one unwatched run of a sea of havens did.
struct Seen {
    raids: u64,
    items: u64,
    lost_milli: u64,
    /// Coins taken, thousandths (PC43: a treasury is taken like any stock).
    coins: i128,
}

/// S8b-3 `a_port_raid_conserves_goods`. Nobody watches, so pirates never fight a merchant and
/// every good they hold came from a port village. The world's own conservation check holds every
/// six days; at the end the villages' stocks fell by exactly what the ledger says pirates took
/// (the markets' own books, `lump_out`); the ledger's identity is exact (what was taken is what
/// landed, plus what is aboard, plus what sank); and what landed raised the havens' markets'
/// `lump_in` by exactly that.
#[test]
fn a_port_raid_conserves_goods() {
    let days = 150;
    let runs = per_seed(&seeds(), |seed| {
        let tun = ShipTunables::default();
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        while t < Tick::from_days(days) {
            t += 6 * TICKS_PER_DAY;
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            s.ships
                .check(&s.world)
                .unwrap_or_else(|e| panic!("seed {seed} at {t:?}: {e}"));
        }
        let n = s.world.market(0).lines.len();
        let l = &s.ships.ledger;
        assert!(
            l.looted_by_pirates.is_empty(),
            "seed {seed}: nobody watched, so no merchant was looted"
        );
        let took = by_good(&l.looted_from_ports, n);
        let landed = by_good(&l.landed_at_havens, n);
        let sunk = by_good(&l.sunk, n);
        let held = aboard(&s.ships, n);
        for g in 0..n {
            assert_eq!(
                took[g],
                landed[g] + held[g] + sunk[g],
                "seed {seed} good {g}: took = landed + aboard + sunk"
            );
        }
        // The villages' stocks fell by exactly what was taken, in their own books.
        let out = lines_sum(&s.world, &village_nodes(&s.world), |l| l.book.lump_out);
        assert_eq!(out, took, "seed {seed}: the villages' lump_out");
        // And landing at the havens raised their markets' lump_in by what landed.
        let into = lines_sum(&s.world, &haven_nodes(&s.ships), |l| l.book.lump_in);
        assert_eq!(into, landed, "seed {seed}: the havens' lump_in");
        let st = s.ships.stats;
        // Whole items per raid: each raid's floor loses under one item.
        let total = took.iter().sum::<i128>() / 1000;
        let items = i128::from(st.items_from_ports);
        assert!(
            items <= total && total - items <= i128::from(st.port_raids),
            "seed {seed}: {items} items in the stats, {total} in the ledger"
        );
        // Coins are goods like the rest: the identity above covered them, and they are counted.
        let currencies: std::collections::BTreeSet<usize> = village_nodes(&s.world)
            .iter()
            .filter_map(|&v| s.world.nodes[v].currency)
            .map(|g| g.index())
            .collect();
        let coins = currencies.iter().map(|&g| took[g]).sum::<i128>();
        Seen {
            coins,
            raids: st.port_raids,
            items: st.items_from_ports,
            lost_milli: st.port_raid_lost_milli,
        }
    });
    let raids: u64 = runs.iter().map(|r| r.raids).sum();
    let items: u64 = runs.iter().map(|r| r.items).sum();
    let landed_somewhere = runs.iter().filter(|r| r.items > 0).count();
    eprintln!(
        "{raids} port raids in {} seas over {days} days, {items} items, {} men lost",
        runs.len(),
        runs.iter().map(|r| r.lost_milli).sum::<u64>() as f64 / 1000.0
    );
    assert!(raids > 0, "the raids happened");
    assert!(
        runs.iter().any(|r| r.coins > 0),
        "pirates take coins like any other good"
    );
    assert!(landed_somewhere >= 3, "in several seas: {landed_somewhere}");
}

/// Each port raid is news in the world (§2.4.6a rule 7, PC10): a PortRaided report, the raiding
/// haven against the port's village at the dock, severity 1, known to the village itself; nothing
/// goes to chat and no player is named.
#[test]
fn a_port_raid_is_news() {
    use rtsim_knowledge::ReportKind;
    let days = 150;
    let raids = per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let tun = ShipTunables::default();
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let havens: Vec<_> = haven_nodes(&s.ships)
            .iter()
            .map(|&n| s.world.nodes[n].actor)
            .collect();
        let villages = village_nodes(&s.world);
        // A port's news lives 6 days: look each day, and count each report once.
        let mut seen = std::collections::BTreeSet::new();
        let mut t = Tick::ZERO;
        while t < Tick::from_days(days) {
            t += TICKS_PER_DAY;
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            let k = &s.world.knowledge;
            for (id, _) in k.reports().iter() {
                let r = k.report(id).expect("a known kind");
                if r.kind != ReportKind::PortRaided || !seen.insert((id, r.at)) {
                    continue;
                }
                assert!(havens.contains(&r.who), "seed {seed}: by a haven: {r:?}");
                let v = villages
                    .iter()
                    .copied()
                    .find(|&v| s.world.nodes[v].actor == r.whom)
                    .unwrap_or_else(|| panic!("seed {seed}: against a village: {r:?}"));
                assert!(
                    k.knows(s.world.nodes[v].actor, id),
                    "seed {seed}: the village knows"
                );
                assert_eq!((r.severity, r.flags), (1, 0), "seed {seed}: {r:?}");
            }
        }
        let news = seen.len() as u64;
        let raids = s.ships.stats.port_raids;
        assert!(news <= raids, "seed {seed}: {news} reports, {raids} raids");
        assert!(
            raids == 0 || news > 0,
            "seed {seed}: {raids} raids and no news"
        );
        raids
    });
    assert!(raids.iter().sum::<u64>() > 0, "the raids happened");
}

/// The villages of `s` that pirates raided up to `to`, by node, with a log kept.
fn raided_nodes(s: &Ships, w: &World) -> Vec<usize> {
    let mut out = Vec::new();
    for e in &s.log {
        if e.kind != "port raid" {
            continue;
        }
        for n in village_nodes(w) {
            if e.what
                .contains(&format!("port village {} ", w.nodes[n].key))
                && !out.contains(&n)
            {
                out.push(n);
            }
        }
    }
    out
}

/// S8b-3 `a_watched_port_is_never_raided`. A first run finds the villages pirates raid; a second
/// run of the same sea has a player standing at the dock of each of those, all through the run.
/// No pirate raids a village under a player's eye (another may still be raided).
#[test]
fn a_watched_port_is_never_raided() {
    let days = 150;
    let runs = per_seed(&seeds(), |seed| {
        let tun = ShipTunables::default();
        let mut s = synth::sea_havens(seed, tun.clone());
        s.ships.log_enabled = true;
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        drive(&mut s, &mut fake, Tick::from_days(days), TICKS_PER_DAY / 4);
        let raided = raided_nodes(&s.ships, &s.world);
        if raided.is_empty() {
            return (0, 0);
        }
        let mut s = synth::sea_havens(seed, tun.clone());
        s.ships.log_enabled = true;
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        for (i, &n) in raided.iter().enumerate() {
            let p = s.world.nodes[n].pos;
            let dock = s
                .ships
                .docks
                .iter()
                .find(|d| d.node == n && d.medium == rtsim_travel::Medium::Sea)
                .expect("a sea dock");
            let uuid = (0xD0C0 + i as u64, seed);
            s.ships.player_stays(
                &mut s.world,
                uuid,
                Pos::new(p.frame, dock.x, dock.z),
                Tick::ZERO,
                Tick::from_days(days + 10),
            );
        }
        drive(&mut s, &mut fake, Tick::from_days(days), TICKS_PER_DAY / 4);
        s.ships.check(&s.world).expect("check");
        let again = raided_nodes(&s.ships, &s.world);
        for n in &raided {
            assert!(
                !again.contains(n),
                "seed {seed}: village {} was raided under a player's eye",
                s.world.nodes[*n].key
            );
        }
        (raided.len(), again.len())
    });
    let watched: usize = runs.iter().map(|r| r.0).sum();
    eprintln!(
        "villages watched: {watched}; raided elsewhere in those runs: {}",
        runs.iter().map(|r| r.1).sum::<usize>()
    );
    assert!(watched >= 5, "the control raided villages: {watched}");
}

/// PC103: a port raid in the switch-on's catch-up is judged against the areas live at the
/// world's present, where its stock would leave. A sea of havens runs 20 days; the ships are
/// switched off, the world runs on alone 120 days, and at the switch-on a player stands at every
/// village's dock. The catch-up replays the pirates' hunts and none of them raids a village;
/// without the players, the same catch-ups raid.
#[test]
fn no_port_raid_in_view_in_the_catch_up() {
    let (off, on) = (Tick::from_days(20), Tick::from_days(140));
    let raids = |s: &Ships| s.log.iter().filter(|e| e.kind == "port raid").count();
    let runs = per_seed(&(1..=12).collect::<Vec<u64>>(), |seed| {
        let catch_up = |watched: bool| {
            let tun = ShipTunables::default();
            let mut s = synth::sea_havens(seed, tun.clone());
            s.ships.log_enabled = true;
            let mut fake = FakeShips::new(f64::from(tun.view_radius));
            drive(&mut s, &mut fake, off, TICKS_PER_DAY / 4);
            s.ships.lift_closures(&mut s.world);
            s.world.run_until(on);
            s.ships.begin_replay(true);
            s.ships.replay_done(&mut s.world);
            if watched {
                let villages = village_nodes(&s.world);
                let docks: Vec<Pos> = s
                    .ships
                    .docks
                    .iter()
                    .filter(|d| villages.contains(&d.node))
                    .map(|d| Pos::new(s.world.nodes[d.node].pos.frame, d.x, d.z))
                    .collect();
                for (i, pos) in docks.into_iter().enumerate() {
                    let uuid = (0xD0C0 + i as u64, seed);
                    s.ships
                        .player_stays(&mut s.world, uuid, pos, on, on + 10 * TICKS_PER_DAY);
                }
            }
            let before = raids(&s.ships);
            let hunts = s.ships.stats.hunts;
            s.ships.run_until(&mut s.world, &mut NoInfrastructure, on);
            s.ships.check(&s.world).expect("check");
            assert!(s.ships.stats.hunts > hunts, "seed {seed}: nothing replayed");
            raids(&s.ships) - before
        };
        (catch_up(false), catch_up(true))
    });
    for (seed, &(_, watched)) in (1..).zip(&runs) {
        assert_eq!(watched, 0, "seed {seed}: a raid under a player's eye");
    }
    let open: usize = runs.iter().map(|r| r.0).sum();
    eprintln!("raids in the unwatched catch-ups: {open} ({runs:?})");
    assert!(open > 0, "the unwatched catch-ups raided nothing");
}

/// Sets every village's population of `w` to `pop`.
fn set_pops(w: &mut World, pop: u32) {
    for n in village_nodes(w) {
        let id = w.nodes[n].settlement;
        w.settlements.get_mut(id).expect("settlement").pop = pop;
    }
}

/// S8b-3 `a_strong_village_is_not_raided`. At 12 people a sea crew of 6 believes it wins
/// (p = 0.8) and raids. At 17 (D = 4.25) the belief is 0.666 for a full sea crew, below the
/// hunt's 2/3, so none of the pirates raids; and at 100 nobody does.
#[test]
fn a_strong_village_is_not_raided() {
    let days = 150;
    let runs = per_seed(&seeds(), |seed| {
        let tun = ShipTunables::default();
        let run = |pop: Option<u32>| {
            let mut s = synth::sea_havens(seed, tun.clone());
            if let Some(p) = pop {
                set_pops(&mut s.world, p);
            }
            let mut fake = FakeShips::new(f64::from(tun.view_radius));
            drive(&mut s, &mut fake, Tick::from_days(days), TICKS_PER_DAY / 4);
            s.ships.check(&s.world).expect("check");
            s.ships.stats.port_raids
        };
        (run(None), run(Some(17)), run(Some(100)))
    });
    let open: u64 = runs.iter().map(|r| r.0).sum();
    eprintln!("port raids at pop 12: {open}");
    assert!(open > 0, "the control raided");
    for (i, r) in runs.iter().enumerate() {
        assert_eq!(r.1, 0, "seed {}: a village of 17 was raided", i + 1);
        assert_eq!(r.2, 0, "seed {}: a village of 100 was raided", i + 1);
    }
}

/// S8b-3 `port_raids_off_changes_nothing`. With `port_raid_carry` 0 no raid is tried, and one
/// that is tried and declined (every village too strong) leaves the world as it was: the same
/// runs, byte for byte (`state_hash` of the ships and of the world), on the coast (whose ports
/// no pirate raids) and on a sea of havens with the villages made strong. And on the coast the
/// default carry and 0 are the same run. (That 0 is S8's behaviour is the existing S1 and S8
/// tests, run unchanged, and an A/B of the parent commit in docs/choices-ports.md PC47.)
#[test]
fn port_raids_off_changes_nothing() {
    let days = 120;
    per_seed(&seeds(), |seed| {
        let hash = |carry: u32, strong: bool| {
            let tun = ShipTunables {
                port_raid_carry: carry,
                ..ShipTunables::default()
            };
            let mut s = synth::sea_havens(seed, tun.clone());
            if strong {
                set_pops(&mut s.world, 100);
            }
            let mut fake = FakeShips::new(f64::from(tun.view_radius));
            drive(&mut s, &mut fake, Tick::from_days(days), TICKS_PER_DAY / 4);
            assert_eq!(s.ships.stats.port_raids, 0, "seed {seed}");
            (s.ships.state_hash(), s.world.state_hash())
        };
        assert_eq!(
            hash(10, true),
            hash(0, true),
            "seed {seed}: a declined raid"
        );
        // The coast.
        let coast = |carry: u32| {
            let tun = ShipTunables {
                port_raid_carry: carry,
                ..ShipTunables::default()
            };
            let mut c = synth::coastal(seed, tun.clone());
            let mut fake = FakeShips::new(f64::from(tun.view_radius));
            synth::drive(
                &mut c.world,
                &mut c.ships,
                &mut fake,
                &mut NoInfrastructure,
                &c.mask,
                Tick::from_days(days),
                TICKS_PER_DAY / 4,
            );
            assert_eq!(c.ships.stats.port_raids, 0, "seed {seed}");
            (
                c.ships.state_hash(),
                c.world.state_hash(),
                c.ships.stats.hunts,
            )
        };
        let (a, b) = (coast(10), coast(0));
        assert!(a.2 > 0, "seed {seed}: the pirates hunted");
        assert_eq!(a, b, "seed {seed}: the coast");
    });
}

/// Every port raid in `s`'s log: (the raiding haven, the village's key, the lane).
fn raids_logged(s: &SeaHavens) -> Vec<(usize, u64, u32)> {
    let mut owner = BTreeMap::new();
    for (h, hv) in s.ships.havens.iter().enumerate() {
        for &p in &hv.ships {
            owner.insert(s.ships.ship_name(p), h);
        }
    }
    s.ships
        .log
        .iter()
        .filter(|e| e.kind == "port raid")
        .map(|e| {
            let w: Vec<&str> = e.what.split_whitespace().collect();
            (
                owner[e.ship.as_ref().expect("a ship")],
                w[3].parse().expect("the village key"),
                w[6].trim_end_matches(':').parse().expect("the lane"),
            )
        })
        .collect()
}

/// S8b-3 `a_raid_hits_the_nearer_end` (PC40). Of the hunted lane's two docks the village raided
/// is the one nearer the raiding haven's dock (`Ships::raid_dock`, here checked against the
/// straight distance in the test), the lane's `a` on a tie. And a watched or a too-strong nearer
/// end is declined, never handed to the far end: with every other village made too strong, and
/// with a player at the dock of every third village, each raid still hits the nearer end.
#[test]
fn a_raid_hits_the_nearer_end() {
    let seen = per_seed(&[1, 2, 3, 4, 5, 6, 7, 8], |seed| {
        let tun = ShipTunables::default();
        let d2 = |a: (i32, i32), b: (i32, i32)| {
            let (dx, dz) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
            dx * dx + dz * dz
        };
        let mut n = 0;
        for case in 0..3 {
            let mut s = synth::sea_havens(seed, tun.clone());
            s.ships.log_enabled = true;
            let villages = village_nodes(&s.world);
            match case {
                // Every village with an odd key is too strong to raid.
                1 => {
                    for &v in &villages {
                        if s.world.nodes[v].key % 2 == 1 {
                            let id = s.world.nodes[v].settlement;
                            s.world.settlements.get_mut(id).unwrap().pop = 100;
                        }
                    }
                }
                // A player at the dock of every third village, all through the run.
                2 => {
                    for (i, &v) in villages.iter().enumerate().filter(|(i, _)| i % 3 == 0) {
                        let dock = s.ships.docks.iter().find(|d| d.node == v).unwrap();
                        s.ships.player_stays(
                            &mut s.world,
                            (0xFAE0 + i as u64, seed),
                            Pos::new(dock.frame, dock.x, dock.z),
                            Tick::ZERO,
                            Tick::from_days(160),
                        );
                    }
                }
                _ => {}
            }
            let mut fake = FakeShips::new(f64::from(tun.view_radius));
            drive(&mut s, &mut fake, Tick::from_days(150), TICKS_PER_DAY / 4);
            for (h, key, lane) in raids_logged(&s) {
                let hv = &s.ships.havens[h];
                let ln = &s.ships.lanes[lane as usize];
                let from = (hv.dock.0, hv.dock.2);
                let (a, b) = (&s.ships.docks[ln.a as usize], &s.ships.docks[ln.b as usize]);
                let nearer = if d2(b.xz(), from) < d2(a.xz(), from) {
                    b
                } else {
                    a
                };
                assert_eq!(
                    s.world.nodes[nearer.node].key, key,
                    "seed {seed} case {case}: lane {lane} from haven {h}"
                );
                let dock = s.ships.raid_dock(&s.world, h as u32, lane).unwrap();
                assert_eq!(s.ships.docks[dock as usize].node, nearer.node);
                if case == 1 {
                    assert_eq!(key % 2, 0, "seed {seed}: a strong village was raided");
                }
                n += 1;
            }
        }
        n
    });
    assert!(seen.iter().sum::<usize>() > 20, "{seen:?}");
}

/// S8b-3 `the_far_end_is_tried_when_the_nearer_is_no_trade_port` (PC40). On a lane whose nearer
/// dock is not a village (made a player base, then a haven, by the test) the far end is the
/// target if it is a village; with neither end a village there is no target. The tie goes to
/// the lane's `a`.
#[test]
fn the_far_end_is_tried_when_the_nearer_is_no_trade_port() {
    let tun = ShipTunables::default();
    let mut s = synth::sea_havens(3, tun.clone());
    let mut tried = 0;
    for h in 0..s.ships.havens.len() as u32 {
        for lane in s.ships.havens[h as usize].lanes.clone() {
            let ln = s.ships.lanes[lane as usize].clone();
            let both = [ln.a, ln.b];
            let near = s
                .ships
                .raid_dock(&s.world, h, lane)
                .expect("a village at both ends");
            assert!(both.contains(&near));
            let far = if near == ln.a { ln.b } else { ln.a };
            let (nn, fnode) = (
                s.ships.docks[near as usize].node,
                s.ships.docks[far as usize].node,
            );
            let (nk, fk) = (s.world.nodes[nn].kind, s.world.nodes[fnode].kind);
            s.world.nodes[nn].kind = NodeKind::PlayerBase { owner: 7 };
            assert_eq!(s.ships.raid_dock(&s.world, h, lane), Some(far));
            s.world.nodes[nn].kind = NodeKind::Haven(rtsim_settle::Culture::Human);
            assert_eq!(s.ships.raid_dock(&s.world, h, lane), Some(far));
            s.world.nodes[fnode].kind = NodeKind::PlayerBase { owner: 7 };
            assert_eq!(s.ships.raid_dock(&s.world, h, lane), None);
            s.world.nodes[nn].kind = nk;
            s.world.nodes[fnode].kind = fk;
            assert_eq!(s.ships.raid_dock(&s.world, h, lane), Some(near));
            tried += 1;
        }
    }
    assert!(tried >= 5, "{tried}");
}

/// S8b-3 `a_hold_never_passes_its_capacity_and_a_sunk_pirate_loses_its_port_loot`. After every
/// step of the run no pirate's hold is over `pirate_hold_slots` (by the hold's own measure, a
/// floor per good of the merged cargo), whatever the raids stacked. Then a pirate carrying port
/// loot is sunk (the plugin's record): the loot is destroyed through the world's books, goes to
/// the ledger's `sunk`, and the identity (took = landed + aboard + sunk) and the world's check
/// hold.
#[test]
fn a_hold_never_passes_its_capacity_and_a_sunk_pirate_loses_its_port_loot() {
    // A raid's loot is under a slot (at most 60 items), so a hold of one slot is what makes the
    // room bind; at the default 27 no hold ever fills. A raid takes only a share of a line's
    // surplus over its target (PC72), so the villages are made rich now and then: the room,
    // not the surplus, bounds the loot.
    let small = ShipTunables {
        pirate_hold_slots: 1,
        ..ShipTunables::default()
    };
    let cap = i64::from(small.pirate_hold_slots) * 1000;
    let hold = |s: &SeaHavens, cargo: &BTreeMap<rtsim_trade::GoodId, i64>| -> i64 {
        cargo
            .iter()
            .map(|(g, a)| *a / i64::from(s.world.goods.get(*g).stack.max(1)))
            .sum()
    };
    let sunk = per_seed(&seeds(), |seed| {
        let tun = small.clone();
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        let mut fullest = 0;
        let mut done = false;
        let mut op = 0xB0A7_0000 + seed * 1000;
        let villages = village_nodes(&s.world);
        while t < Tick::from_days(150) {
            if t.get().is_multiple_of(TICKS_PER_DAY * 24) {
                for &v in &villages {
                    enrich(&mut s.world, v, 128_000, t);
                }
            }
            t += TICKS_PER_DAY / 4;
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            for hv in &s.ships.havens {
                for &p in &hv.ships {
                    let x = s.ships.ship(p).unwrap();
                    let v = hold(&s, &x.cargo);
                    assert!(v <= cap, "seed {seed}: a hold of {v} over {cap}");
                    fullest = fullest.max(v);
                }
            }
            if done {
                continue;
            }
            // The first pirate to carry loot while hunting is sunk where it stands.
            let victim = s
                .ships
                .havens
                .iter()
                .flat_map(|hv| hv.ships.iter())
                .find(|&&p| {
                    s.ships
                        .ship(p)
                        .is_some_and(|x| x.is_active() && !x.cargo.is_empty())
                });
            if let Some(&p) = victim {
                let before = s.ships.ledger.sunk.values().sum::<i128>();
                op += 1;
                s.ships.ingest(
                    &mut s.world,
                    &rtsim_ships::boundary::Obs::Sunk(rtsim_ships::boundary::Sunk {
                        agent: rtsim_ships::engine::Ship::agent(p),
                        epoch: s.ships.ship(p).unwrap().epoch,
                        by: rtsim_ships::boundary::By::None,
                        dimension: 0,
                        player: (0, 0),
                        ship: 0,
                        op,
                        pos: (0.0, 63.0, 0.0),
                    }),
                    &s.mask,
                    t,
                );
                assert!(!s.ships.ship(p).unwrap().is_active());
                assert!(
                    s.ships.ledger.sunk.values().sum::<i128>() > before,
                    "seed {seed}: the loot sank"
                );
                s.ships.check(&s.world).expect("check after the sinking");
                done = true;
            }
        }
        s.ships.check(&s.world).expect("check");
        let n = s.world.market(0).lines.len();
        let l = &s.ships.ledger;
        let (took, landed, sunk, held) = (
            by_good(&l.looted_from_ports, n),
            by_good(&l.landed_at_havens, n),
            by_good(&l.sunk, n),
            aboard(&s.ships, n),
        );
        for g in 0..n {
            assert_eq!(
                took[g],
                landed[g] + sunk[g] + held[g],
                "seed {seed} good {g}"
            );
        }
        (done, fullest)
    });
    assert!(sunk.iter().filter(|r| r.0).count() >= 10, "{sunk:?}");
    assert!(
        sunk.iter().filter(|r| r.1 > cap * 9 / 10).count() >= 10,
        "a hold filled: {sunk:?}"
    );
}

/// S8b-3 `a_save_mid_hunt_with_port_loot_aboard_continues_exactly`. At a moment when a pirate
/// carries port loot, the ships and the world are saved and loaded; both runs then go on, one on
/// the loaded copy, and match to the byte at the end (state hashes, the ledger's port loot, the
/// stats), with the world's check holding.
#[test]
fn a_save_mid_hunt_with_port_loot_aboard_continues_exactly() {
    per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let tun = ShipTunables::default();
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        loop {
            t += TICKS_PER_DAY / 4;
            assert!(t < Tick::from_days(150), "seed {seed}: no loot aboard");
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            let aboard_now = s
                .ships
                .havens
                .iter()
                .flat_map(|h| h.ships.iter())
                .any(|&p| {
                    s.ships.ship(p).is_some_and(|x| {
                        x.cargo.values().sum::<i64>() > 0
                            && matches!(
                                x.pirate.as_ref().map(|q| q.state),
                                Some(PState::Hunting { .. })
                            )
                    })
                });
            if aboard_now {
                break;
            }
        }
        // The copy: load the saved world and ships (the fake plugin has no state to carry here).
        let (goods, ids) = rtsim_world::synth::goods();
        let book = rtsim_world::synth::recipes(&ids);
        let mut w2 = World::load(synth::world_config(seed), goods, book, &s.world.save())
            .expect("the world loads");
        let mut s2 = Ships::load(tun.clone(), &s.ships.save()).expect("the ships load");
        assert_eq!(s2.state_hash(), s.ships.state_hash());
        let mut fake2 = FakeShips::new(f64::from(tun.view_radius));
        let end = t + 40 * TICKS_PER_DAY;
        drive(&mut s, &mut fake, end, TICKS_PER_DAY / 4);
        synth::drive(
            &mut w2,
            &mut s2,
            &mut fake2,
            &mut NoInfrastructure,
            &s.mask,
            end,
            TICKS_PER_DAY / 4,
        );
        s2.check(&w2).expect("the copy checks");
        assert_eq!(s2.stats, s.ships.stats, "seed {seed}");
        assert_eq!(s2.ledger, s.ships.ledger, "seed {seed}");
        assert_eq!(s2.state_hash(), s.ships.state_hash(), "seed {seed}");
        assert_eq!(w2.state_hash(), s.world.state_hash(), "seed {seed}");
    });
}

// ---- the measurement -----------------------------------------------------------------------------

/// One world's unattended run (`years` model years), with port raids on or off.
#[derive(Clone, Debug, Default)]
struct Measure {
    raids: u64,
    items: u64,
    /// Each raid's share of the village's whole stock it took, and its items.
    shares: Vec<f64>,
    hunts: u64,
    /// The mean share of the havens' slots manned at a 24-day census.
    manned: f64,
    lost_men: f64,
}

fn run_measure<M: rtsim_ships::water::WaterMask>(
    mut world: World,
    mut ships: Ships,
    mask: &M,
    havens: &[u32],
    years: u64,
) -> Measure {
    ships.log_enabled = true;
    let mut fake = FakeShips::new(f64::from(ships.tun.view_radius));
    let slots: usize = havens
        .iter()
        .map(|&h| ships.havens[h as usize].ships.len())
        .sum();
    let (mut up_sum, mut samples) = (0.0, 0u64);
    let mut t = Tick::ZERO;
    while t < Tick::from_days(years * 96) {
        t += 24 * TICKS_PER_DAY;
        synth::drive(
            &mut world,
            &mut ships,
            &mut fake,
            &mut NoInfrastructure,
            mask,
            t,
            TICKS_PER_DAY / 4,
        );
        let up: usize = havens
            .iter()
            .map(|&h| {
                ships.havens[h as usize]
                    .ships
                    .iter()
                    .filter(|&&p| ships.ship(p).is_some_and(|x| x.is_active()))
                    .count()
            })
            .sum();
        up_sum += up as f64 / slots as f64;
        samples += 1;
    }
    ships.check(&world).expect("check");
    let mut m = Measure {
        raids: ships.stats.port_raids,
        items: ships.stats.items_from_ports,
        hunts: ships.stats.hunts,
        manned: up_sum / samples as f64,
        lost_men: ships.stats.port_raid_lost_milli as f64 / 1000.0,
        ..Measure::default()
    };
    for e in ships.log.iter().filter(|e| e.kind == "port raid") {
        // "raids port village K from lane L: N of S items there; ..."
        let w: Vec<&str> = e.what.split_whitespace().collect();
        // "... took T of S thousandths in the village; ...", both in thousandths of an item.
        let k = w.iter().position(|x| *x == "took").unwrap();
        let (took, stock): (f64, f64) = (w[k + 1].parse().unwrap(), w[k + 3].parse().unwrap());
        m.shares.push(if stock > 0.0 { took / stock } else { 0.0 });
    }
    m
}

fn measure(kind: &str, seed: u64, carry: u32, years: u64) -> Measure {
    let tun = ShipTunables {
        port_raid_carry: carry,
        ..ShipTunables::default()
    };
    match kind {
        "sea_havens" => {
            let s = synth::sea_havens(seed, tun);
            run_measure(s.world, s.ships, &s.mask, &s.havens, years)
        }
        _ => {
            let mut c = synth::coastal(seed, tun);
            if kind == "coastal_pop12" {
                set_pops(&mut c.world, 12);
            }
            run_measure(c.world, c.ships, &c.mask, &[c.haven], years)
        }
    }
}

/// The numbers for the doc (docs/choices-ports.md PC46): port raids per year per sea, items per
/// raid, the share of a village's stock a raid takes (mean and max), hunts per year and the
/// haven's manned share, with `port_raid_carry` 10 against 0, seeds 1 to 30. Run in release:
/// `nice -n 10 cargo test --release -p rtsim-ships --test port_raids -- --ignored --nocapture`.
#[test]
#[ignore = "long: ten model years on 30 seeds, three worlds, carry 10 and 0"]
fn port_raids_measured() {
    let years = 10;
    for kind in ["coastal", "coastal_pop12", "sea_havens"] {
        let on = per_seed(&seeds(), |s| measure(kind, s, 10, years));
        let off = per_seed(&seeds(), |s| measure(kind, s, 0, years));
        let n = on.len() as f64;
        let mean = |v: &[Measure], f: fn(&Measure) -> f64| v.iter().map(f).sum::<f64>() / n;
        let raids: u64 = on.iter().map(|m| m.raids).sum();
        let items: u64 = on.iter().map(|m| m.items).sum();
        let shares: Vec<f64> = on.iter().flat_map(|m| m.shares.iter().copied()).collect();
        let (smean, smax) = (
            shares.iter().sum::<f64>() / shares.len().max(1) as f64 + 0.0,
            shares.iter().copied().fold(0.0, f64::max),
        );
        eprintln!(
            "{kind}: carry 10 against 0, {} seeds x {years} years\n  port raids per year per sea \
             {:.2} (carry 0: {:.2})\n  items per raid {:.1}\n  share of a village's stock per \
             raid: mean {:.3}, max {:.3} ({} raids)\n  men lost per raid {:.2}\n  hunts per year \
             {:.1} (carry 0: {:.1})\n  haven manned share {:.3} (carry 0: {:.3})",
            on.len(),
            raids as f64 / n / years as f64,
            off.iter().map(|m| m.raids).sum::<u64>() as f64 / n / years as f64,
            items as f64 / raids.max(1) as f64,
            smean,
            smax,
            shares.len(),
            on.iter().map(|m| m.lost_men).sum::<f64>() / raids.max(1) as f64,
            mean(&on, |m| m.hunts as f64) / years as f64,
            mean(&off, |m| m.hunts as f64) / years as f64,
            mean(&on, |m| m.manned),
            mean(&off, |m| m.manned),
        );
    }
}

/// S8b-3 `a_raid_fills_a_part_full_hold_exactly_to_its_capacity` (PC71). A pirate whose hold
/// already holds goods (taken from a village into its hold by the test, as any held goods) raids
/// a port made rich (PC72: a raid takes from the surplus over the target only): the merged hold,
/// by the hold's own floor-per-good measure, never passes its capacity, across many part-full
/// holds. (The world's check is not called: each trial's hold is destroyed outside the ledger.)
/// Port raids alone never meet a part full hold (a hunt has one raid and the hold is emptied at
/// home), so this makes one.
#[test]
fn a_raid_fills_a_part_full_hold_exactly_to_its_capacity() {
    let tun = ShipTunables {
        pirate_hold_slots: 1,
        ..ShipTunables::default()
    };
    let cap = i64::from(tun.pirate_hold_slots) * 1000;
    let mut filled = 0;
    let mut raided = 0;
    for seed in 1..=12u64 {
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        let (p, lane, h) = loop {
            t += TICKS_PER_DAY / 4;
            assert!(t < Tick::from_days(100), "seed {seed}: no hunt");
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            let hit = s.ships.havens.iter().enumerate().find_map(|(h, hv)| {
                hv.ships
                    .iter()
                    .find_map(|&p| match s.ships.ship(p)?.pirate.as_ref()?.state {
                        PState::Hunting { lane, .. } => Some((p, lane, h as u32)),
                        _ => None,
                    })
            });
            if let Some(hit) = hit {
                break hit;
            }
        };
        let dock = s.ships.raid_dock(&s.world, h, lane).expect("a port");
        let node = s.ships.docks[dock as usize].node;
        let goods: Vec<_> = s.world.goods.ids().collect();
        for trial in 0..24u64 {
            // A part-full hold: some of one good, of another, the amounts stepping through
            // every remainder of the stack so rounding goes both ways.
            let mut want = BTreeMap::new();
            for (k, g) in goods
                .iter()
                .enumerate()
                .skip(trial as usize % 3)
                .step_by(3)
                .take(2)
            {
                want.insert(*g, (trial as i64 * 37 + k as i64 * 11 + 1) * 40);
            }
            let got = s.world.stock_to_held(node, &want, t);
            let x = s.ships.ships.get_mut(p).unwrap();
            for (g, a) in got {
                *x.cargo.entry(g).or_default() += a;
            }
            x.crew = tun.medium(x.medium).pirate_crew;
            // The hold as the raid finds it: one already full (the rich port gave the test all
            // it asked) is not raided (PC70) and stays as it is.
            let before = x.cargo.clone();
            enrich(&mut s.world, node, 128_000, t);
            let raids = s.ships.stats.port_raids;
            s.ships.port_raid(&mut s.world, p, lane, t);
            let fought = s.ships.stats.port_raids > raids;
            raided += usize::from(fought);
            let x = s.ships.ship(p).unwrap();
            let v: i64 = x
                .cargo
                .iter()
                .map(|(g, a)| *a / i64::from(s.world.goods.get(*g).stack.max(1)))
                .sum();
            assert!(
                v <= cap || x.cargo == before,
                "seed {seed} trial {trial}: {v} over {cap}"
            );
            filled += usize::from(fought && v > cap * 9 / 10);
            // Back to an empty hold for the next trial: what was aboard sinks with nothing
            // owed (it is destroyed through the world's books).
            let cargo = std::mem::take(&mut s.ships.ships.get_mut(p).unwrap().cargo);
            s.world.destroy_held(&cargo);
        }
    }
    eprintln!("{raided} raids, {filled} holds filled to the brim");
    assert!(raided > 100, "{raided} raids");
    assert!(filled > 20, "{filled} holds filled to the brim");
}

/// S8b-3 `a_port_raid_takes_only_the_surplus` (PC72). A pirate raids a port some of whose lines
/// stand above their targets (made so by the test) and some at or below: every line at or below
/// its target is untouched, and every line above keeps at least its target, coins included.
#[test]
fn a_port_raid_takes_only_the_surplus() {
    let tun = ShipTunables::default();
    let (mut raided, mut took_coins) = (0, 0);
    for seed in 1..=8u64 {
        let mut s = synth::sea_havens(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        let (p, lane, h) = loop {
            t += TICKS_PER_DAY / 4;
            assert!(t < Tick::from_days(100), "seed {seed}: no hunt");
            drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
            let hit = s.ships.havens.iter().enumerate().find_map(|(h, hv)| {
                hv.ships
                    .iter()
                    .find_map(|&p| match s.ships.ship(p)?.pirate.as_ref()?.state {
                        PState::Hunting { lane, .. } => Some((p, lane, h as u32)),
                        _ => None,
                    })
            });
            if let Some(hit) = hit {
                break hit;
            }
        };
        let dock = s.ships.raid_dock(&s.world, h, lane).expect("a port");
        let node = s.ships.docks[dock as usize].node;
        let goods: Vec<_> = s.world.goods.ids().collect();
        for trial in 0..6usize {
            // Every other good, by the trial, stands up to 40 items over its target.
            let m = s.world.market(node);
            let rich: BTreeMap<_, _> = goods
                .iter()
                .enumerate()
                .filter(|(k, _)| (k + trial) % 2 == 0)
                .map(|(k, &g)| {
                    let want = m.line(g).target + (k as i64 + 1) * 40_000 / goods.len() as i64;
                    (g, want - m.stock(g))
                })
                .filter(|e| e.1 > 0)
                .collect();
            s.world.stock_from_players(node, &rich, t);
            let m = s.world.market(node);
            let before: Vec<(i64, i64)> = goods
                .iter()
                .map(|&g| (m.stock(g), m.line(g).target))
                .collect();
            let x = s.ships.ships.get_mut(p).unwrap();
            x.crew = tun.medium(x.medium).pirate_crew;
            let raids = s.ships.stats.port_raids;
            s.ships.port_raid(&mut s.world, p, lane, t);
            raided += usize::from(s.ships.stats.port_raids > raids);
            let m = s.world.market(node);
            for (k, &g) in goods.iter().enumerate() {
                let (stock, target) = before[k];
                let after = m.stock(g);
                if stock <= target {
                    assert_eq!(
                        after, stock,
                        "seed {seed} trial {trial} good {k}: at its target"
                    );
                } else {
                    let share = (stock - target) * tun.loot_share_milli / 1000;
                    assert!(
                        after >= target && after >= stock - share && after <= stock,
                        "seed {seed} trial {trial} good {k}: {stock} to {after}, target {target}"
                    );
                    took_coins += usize::from(s.world.goods.is_currency(g) && after < stock);
                }
            }
            // Back to an empty hold for the next trial (destroyed through the world's books).
            let cargo = std::mem::take(&mut s.ships.ships.get_mut(p).unwrap().cargo);
            s.world.destroy_held(&cargo);
        }
    }
    assert!(raided >= 24, "{raided} raids");
    assert!(
        took_coins > 0,
        "a raid takes coins over the treasury's target"
    );
}

/// The share of haven sites open (sampled every 24 days), the port raids fought and the villages
/// whose treasury is under one coin, after `days` of `seed`'s `sea_havens` with `port_raid_carry`
/// `carry`.
fn sea_outcome(seed: u64, carry: u32, days: u64) -> (f64, u64, usize) {
    let tun = ShipTunables {
        port_raid_carry: carry,
        ..ShipTunables::default()
    };
    let mut s = synth::sea_havens(seed, tun.clone());
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let (mut open, mut seen) = (0usize, 0usize);
    let mut t = Tick::ZERO;
    while t < Tick::from_days(days) {
        t += TICKS_PER_DAY * 24;
        drive(&mut s, &mut fake, t, TICKS_PER_DAY / 4);
        open += s
            .havens
            .iter()
            .filter(|&&h| s.ships.havens[h as usize].shut.is_none())
            .count();
        seen += s.havens.len();
    }
    let broke = village_nodes(&s.world)
        .into_iter()
        .filter(|&v| s.world.market(v).treasury() < 1000)
        .count();
    (open as f64 / seen as f64, s.ships.stats.port_raids, broke)
}

/// S8b-3 `port_raids_leave_the_villages_solvent` (PC73), the cheap cut of the 1,000-year haven
/// run's regression: when a raid took 600 per mille of every stock, raided villages went broke
/// and the lane trickles their havens hunted on stopped, so sites open fell from 0.958 with no
/// port raids to 0.885. On seeds 25, 26 and 30 for 600 days, paired against `port_raid_carry` 0:
/// raids happen, no village's treasury is under one coin, and the sites open are within 0.05 of
/// the pair's, on the mean of the seeds. Before PC72 these seeds had 1, 4 and 1 villages broke
/// and sites open 0.824, 0.984 and 0.680 against 0.968, 0.984 and 1.000.
#[test]
fn port_raids_leave_the_villages_solvent() {
    let seeds = [25u64, 26, 30];
    let runs = per_seed(&[25, 26, 30, 125, 126, 130], |j| {
        sea_outcome(j % 100, if j > 100 { 0 } else { 10 }, 600)
    });
    let (on, off) = runs.split_at(seeds.len());
    for (k, seed) in seeds.iter().enumerate() {
        eprintln!(
            "seed {seed}: open {:.3} against {:.3}, {} raids, {} villages broke",
            on[k].0, off[k].0, on[k].1, on[k].2
        );
        assert!(on[k].1 > 0, "seed {seed}: raids happen");
        assert_eq!(on[k].2, 0, "seed {seed}: a village went broke");
    }
    let gap: f64 = on.iter().zip(off).map(|(a, b)| b.0 - a.0).sum::<f64>() / seeds.len() as f64;
    assert!(
        gap <= 0.05,
        "sites open {gap:.3} under the pair's: {on:?} {off:?}"
    );
}
