// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b-3 (EQUILIBRIUM.md §2.4.6a rule 6; docs/choices-ports.md PC9 and PC40 to PC47): pirates
//! raid port villages. A hunt's coast raid that finds no player district or train takes the port
//! village at the hunted lane's nearer end, if nobody watches it and the pirate believes it wins
//! (`p = A² / (A² + D²) > 2/3` against `pop x 0.25`). The square law costs the crew, booked as
//! the toll's; the survivors carry off 10 items a man, at most 600 per mille of each stock,
//! through `World::stock_to_held` into the hold, and the haven's landing carries them home.
//!
//! The worlds are `synth::sea_havens` (12-person ports, so a full sea crew of 6 raids them) and,
//! for the rules that decline, `synth::coastal` (ports of 30 to 72, which no pirate raids).

use std::collections::BTreeMap;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{NodeKind, Pos};
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
        Seen {
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
    assert!(landed_somewhere >= 3, "in several seas: {landed_somewhere}");
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

/// S8b-3 `a_raid_hits_the_nearer_end` (PC40): of the hunted lane's two docks, the village raided
/// is the one nearer the raiding haven's dock, in straight distance, the lane's `a` on a tie.
/// (Every port of `sea_havens` is a village, so the other end is never needed.)
#[test]
fn a_raid_hits_the_nearer_end() {
    let seen = per_seed(&[1, 2, 3, 4, 5, 6, 7, 8], |seed| {
        let tun = ShipTunables::default();
        let mut s = synth::sea_havens(seed, tun.clone());
        s.ships.log_enabled = true;
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        drive(&mut s, &mut fake, Tick::from_days(150), TICKS_PER_DAY / 4);
        let mut owner = BTreeMap::new();
        for (h, hv) in s.ships.havens.iter().enumerate() {
            for &p in &hv.ships {
                owner.insert(s.ships.ship_name(p), h);
            }
        }
        let d2 = |a: (i32, i32), b: (i32, i32)| {
            let (dx, dz) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
            dx * dx + dz * dz
        };
        let mut n = 0;
        for e in s.ships.log.iter().filter(|e| e.kind == "port raid") {
            let w: Vec<&str> = e.what.split_whitespace().collect();
            let key: u64 = w[3].parse().expect("the village key");
            let lane: usize = w[6].parse().expect("the lane");
            let h = owner[e.ship.as_ref().expect("a ship")];
            let hv = &s.ships.havens[h];
            let ln = &s.ships.lanes[lane];
            let from = (hv.dock.0, hv.dock.2);
            let (a, b) = (&s.ships.docks[ln.a as usize], &s.ships.docks[ln.b as usize]);
            let nearer = if d2(b.xz(), from) < d2(a.xz(), from) {
                b
            } else {
                a
            };
            assert_eq!(
                s.world.nodes[nearer.node].key, key,
                "seed {seed}: lane {lane} from haven {h}"
            );
            n += 1;
        }
        n
    });
    assert!(seen.iter().sum::<usize>() > 20, "{seen:?}");
}
