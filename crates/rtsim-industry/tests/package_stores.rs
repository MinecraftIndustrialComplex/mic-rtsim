// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package stores (SECTIONS.md §2.5, slice S4): a chain network whose frogports sit in more than one
//! district holds the packages in transit, with a pool per destination frogport. These tests drive
//! the engine with the records Java sends (IND_PACKAGE_STORE, IND_CHAIN_PACKAGE, IND_STORE_CENSUS).

use std::collections::BTreeMap;

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StoreId};
use rtsim_industry::learn::LinkKey;
use rtsim_industry::obs::{
    ChainDirection, ChainPackage, HeldKey, Obs, StoreCensus, StoreDecl, StoreFrogport, Timed,
};
use rtsim_industry::persist::STORE_TABLE;
use rtsim_industry::stores::Pool;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::PageKey;

const IRON: Good = Good(1);
const GOLD: Good = Good(2);
const D1: DistrictId = DistrictId(1);
const D2: DistrictId = DistrictId(2);
const S: StoreId = StoreId(5);
const U: StoreId = StoreId(9);

fn pos(x: i32) -> BlockPos {
    BlockPos::new(x, 64, 0)
}

/// Frogports A (D1), B (D2) and C (a player's `*` frogport, no district), along one chain.
fn a() -> StoreFrogport {
    port(Some(D1), 10, false)
}
fn b() -> StoreFrogport {
    port(Some(D2), 20, false)
}
fn c() -> StoreFrogport {
    port(None, 30, true)
}

fn port(district: Option<DistrictId>, x: i32, star: bool) -> StoreFrogport {
    StoreFrogport {
        district,
        inventory: district.map(|_| 3),
        star,
        pos: pos(x),
        conveyor: pos(x + 1),
        from: None,
    }
}

fn from(mut f: StoreFrogport, s: StoreId) -> StoreFrogport {
    f.from = Some(s);
    f
}

fn decl(store: StoreId, frogports: Vec<StoreFrogport>) -> StoreDecl {
    let n = frogports.len();
    // Hops: the frogports' x apart, in tens.
    let mut hops = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            hops.push(((frogports[j].pos.x - frogports[i].pos.x).unsigned_abs() / 10) as u16);
        }
    }
    StoreDecl {
        store,
        dim: 0,
        epoch: 0,
        into: None,
        frogports,
        hops,
    }
}

fn removal(store: StoreId, into: Option<StoreId>) -> StoreDecl {
    StoreDecl {
        store,
        dim: 0,
        epoch: 0,
        into,
        frogports: vec![],
        hops: vec![],
    }
}

fn engine() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    e.define_good(GOLD, GoodInfo::item("gold", 64));
    e
}

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

fn chain(
    store: StoreId,
    district: Option<DistrictId>,
    direction: ChainDirection,
    frogport: u32,
    destination: Option<u32>,
    packages: u32,
    goods: &[(Good, i64)],
) -> Obs {
    Obs::ChainPackage(ChainPackage {
        store,
        district,
        inventory: district.map(|_| 3),
        direction,
        frogport,
        // The frogports in their usual order (the record's position matters only for writes).
        pos: pos(10 * (frogport as i32 + 1)),
        destination,
        packages,
        address: 0,
        op: None,
        key: None,
        goods: GoodVec::from_pairs(goods.iter().copied()),
    })
}

fn pool(packages: u64, goods: &[(Good, i64)]) -> Pool {
    Pool {
        packages,
        goods: GoodVec::from_pairs(goods.iter().copied()),
    }
}

fn pools(e: &Engine, s: StoreId) -> Vec<Pool> {
    e.stores()[&s].pools.clone()
}

#[test]
fn a_store_books_its_packages_by_destination() {
    let mut e = engine();
    send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
    assert_eq!(e.stores()[&S].hops_between(0, 2), Some(2));
    // A sends two packages to B and one nobody takes.
    send(
        &mut e,
        10,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            2,
            &[(IRON, 128)],
        ),
    );
    send(
        &mut e,
        11,
        chain(S, Some(D1), ChainDirection::Out, 0, None, 1, &[(GOLD, 5)]),
    );
    assert_eq!(
        pools(&e, S),
        vec![Pool::default(), pool(2, &[(IRON, 128)]), Pool::default()]
    );
    assert_eq!(e.stores()[&S].no_route, pool(1, &[(GOLD, 5)]));
    // B takes one in, then two more than the store booked: the pool floors at zero.
    send(
        &mut e,
        20,
        chain(
            S,
            Some(D2),
            ChainDirection::In,
            1,
            Some(1),
            1,
            &[(IRON, 64)],
        ),
    );
    assert_eq!(pools(&e, S)[1], pool(1, &[(IRON, 64)]));
    send(
        &mut e,
        21,
        chain(
            S,
            Some(D2),
            ChainDirection::In,
            1,
            Some(1),
            3,
            &[(IRON, 192)],
        ),
    );
    assert_eq!(pools(&e, S)[1], Pool::default());
    let counts = &e.stores()[&S].counts;
    assert_eq!(
        (
            counts.packages_out,
            counts.packages_in,
            counts.unrouted,
            counts.in_beyond
        ),
        (3, 4, 1, 2)
    );
    // A record naming a frogport the store does not list books nothing.
    send(
        &mut e,
        22,
        chain(
            S,
            Some(D2),
            ChainDirection::In,
            7,
            Some(7),
            1,
            &[(IRON, 64)],
        ),
    );
    assert_eq!(e.stores()[&S].counts.stale_records, 1);
}

/// A chain package is a port of its frogport's district, and never a link: a `LinkKey::Mail`
/// would send the district to the black box (SECTIONS.md §2.5).
/// A chain package is a port of its loaded district and, routed to a district's frogport, its
/// chain link (slice S4b, choice SC108): out toward the frogport it resolves to, in at its own.
#[test]
fn a_chain_package_is_a_port_and_a_link_of_its_district() {
    let mut e = engine();
    send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b()])));
    send(
        &mut e,
        10,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            1,
            &[(IRON, 64)],
        ),
    );
    send(
        &mut e,
        30,
        chain(
            S,
            Some(D2),
            ChainDirection::In,
            1,
            Some(1),
            1,
            &[(IRON, 64)],
        ),
    );
    let to_b = |good| LinkKey::Chain {
        inventory: 3,
        to: b().pos,
        good,
    };
    let d1 = e.district(D1).expect("D1");
    assert_eq!(d1.ports.get(IRON), -64);
    assert_eq!(d1.links.iter().collect::<Vec<_>>(), [(&to_b(IRON), &-64)]);
    assert!(
        d1.batches.is_empty(),
        "a mail batch is the postbox's, not the chain's"
    );
    assert_eq!(e.stores()[&S].batch[&b().pos].get(IRON), 64);
    let d2 = e.district(D2).expect("D2");
    assert_eq!(d2.ports.get(IRON), 64);
    assert_eq!(d2.links.iter().collect::<Vec<_>>(), [(&to_b(IRON), &64)]);
    // A player's frogport has no district: the store books it, no district is made.
    send(
        &mut e,
        40,
        chain(S, None, ChainDirection::Out, 1, Some(0), 1, &[(GOLD, 1)]),
    );
    assert_eq!(e.districts().count(), 2);
    // Toward a frogport of no district, or toward none (circling), a package is no link.
    for dest in [Some(2), None] {
        send(
            &mut e,
            50,
            chain(S, Some(D1), ChainDirection::Out, 0, dest, 1, &[(GOLD, 5)]),
        );
    }
    send(&mut e, 50, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
    send(
        &mut e,
        60,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(2),
            1,
            &[(GOLD, 5)],
        ),
    );
    let d1 = e.district(D1).expect("D1");
    assert_eq!(d1.ports.get(GOLD), -15);
    assert_eq!(d1.links.len(), 1, "{:?}", d1.links);
}

/// What a store holds for a frogport follows it when the network splits or merges (SECTIONS.md
/// §2.5, identity), in either order of the declarations.
#[test]
fn pools_follow_their_frogports_across_a_split_and_a_merge() {
    for split_first in [false, true] {
        let mut e = engine();
        send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
        send(
            &mut e,
            1,
            chain(
                S,
                Some(D1),
                ChainDirection::Out,
                0,
                Some(1),
                1,
                &[(IRON, 64)],
            ),
        );
        send(
            &mut e,
            2,
            chain(
                S,
                Some(D1),
                ChainDirection::Out,
                0,
                Some(2),
                2,
                &[(GOLD, 9)],
            ),
        );
        send(
            &mut e,
            3,
            chain(S, Some(D1), ChainDirection::Out, 0, None, 1, &[(GOLD, 1)]),
        );
        // The chain is cut between B and C: C goes to a new store U.
        if split_first {
            send(&mut e, 10, Obs::PackageStore(decl(S, vec![a(), b()])));
            send(&mut e, 10, Obs::PackageStore(decl(U, vec![from(c(), S)])));
        } else {
            send(&mut e, 10, Obs::PackageStore(decl(U, vec![from(c(), S)])));
            send(&mut e, 10, Obs::PackageStore(decl(S, vec![a(), b()])));
        }
        assert_eq!(pools(&e, U), vec![pool(2, &[(GOLD, 9)])], "{split_first}");
        assert_eq!(pools(&e, S), vec![Pool::default(), pool(1, &[(IRON, 64)])]);
        assert!(e.stores()[&S].left.is_empty(), "{:?}", e.stores()[&S].left);
        assert_eq!(e.stores()[&S].no_route, pool(1, &[(GOLD, 1)]));
        // Joined again: S, the lower id, keeps its id and takes C back; U goes into S.
        if split_first {
            send(&mut e, 20, Obs::PackageStore(removal(U, Some(S))));
            send(
                &mut e,
                20,
                Obs::PackageStore(decl(S, vec![a(), b(), from(c(), U)])),
            );
        } else {
            send(
                &mut e,
                20,
                Obs::PackageStore(decl(S, vec![a(), b(), from(c(), U)])),
            );
            send(&mut e, 20, Obs::PackageStore(removal(U, Some(S))));
        }
        assert!(!e.stores().contains_key(&U));
        assert_eq!(
            pools(&e, S),
            vec![
                Pool::default(),
                pool(1, &[(IRON, 64)]),
                pool(2, &[(GOLD, 9)])
            ],
            "{split_first}"
        );
        assert_eq!(e.stores()[&S].held(), pool(4, &[(IRON, 64), (GOLD, 10)]));
    }
}

/// A census sets the pools to what is on the chain, and the difference is the residue: here a
/// player took a package off and hung on one with no route.
#[test]
fn the_census_sets_the_pools_and_keeps_the_residue() {
    let mut e = engine();
    send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
    send(
        &mut e,
        1,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            3,
            &[(IRON, 192)],
        ),
    );
    send(
        &mut e,
        200,
        Obs::StoreCensus(StoreCensus {
            store: S,
            packages: vec![(Some(1), 2), (None, 1)],
            goods: vec![(Some(1), IRON, 128), (None, GOLD, 4)],
        }),
    );
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], pool(2, &[(IRON, 128)]));
    assert_eq!(s.no_route, pool(1, &[(GOLD, 4)]));
    assert_eq!(s.residue, GoodVec::from_pairs([(IRON, -64), (GOLD, 4)]));
    assert_eq!(s.residue_packages, 0);
    assert_eq!(s.census_at, Some(Tick(200)));
}

/// A census runs at the end of its tick, after the frogports: it already counts that tick's
/// crossings, whose records reach the engine after it. They move the counts, not the pools; a
/// later tick's records are booked.
#[test]
fn a_census_tick_books_its_crossings_once() {
    let mut e = engine();
    send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
    send(
        &mut e,
        200,
        Obs::StoreCensus(StoreCensus {
            store: S,
            packages: vec![(Some(1), 2)],
            goods: vec![(Some(1), IRON, 128)],
        }),
    );
    // Tick 200's crossings: one package out to b (in the census), one taken off at a's pool,
    // which the census had already left out.
    send(
        &mut e,
        200,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            1,
            &[(IRON, 64)],
        ),
    );
    send(
        &mut e,
        200,
        chain(S, Some(D1), ChainDirection::In, 0, None, 1, &[(IRON, 64)]),
    );
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], pool(2, &[(IRON, 128)]));
    assert_eq!(s.pools[0], Pool::default());
    assert_eq!(
        (
            s.counts.packages_out,
            s.counts.packages_in,
            s.counts.in_beyond,
            s.counts.in_census
        ),
        (1, 1, 0, 2)
    );
    // Tick 201: b takes one off; booked.
    send(
        &mut e,
        201,
        chain(S, Some(D2), ChainDirection::In, 1, None, 1, &[(IRON, 64)]),
    );
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], pool(1, &[(IRON, 64)]));
    assert_eq!(s.counts.in_census, 2);
}

type Disk = BTreeMap<u64, Vec<u8>>;

fn save(e: &mut Engine, disk: &mut Disk) -> (Vec<PageKey>, Vec<PageKey>) {
    let c = e.save_changed();
    for k in &c.delete {
        disk.remove(&k.0);
    }
    let put: Vec<PageKey> = c.put.iter().map(|(k, _)| *k).collect();
    for (k, b) in c.put {
        disk.insert(k.0, b);
    }
    let whole: Disk = e.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    assert!(
        *disk == whole,
        "the saves' pages are the engine's whole state"
    );
    if let Some(c) = e.page_check() {
        assert_eq!(c.unmarked, 0, "{:?}", c.first_unmarked);
    }
    (put, c.delete)
}

fn stores_of(keys: &[PageKey]) -> Vec<u64> {
    keys.iter()
        .filter(|k| k.table() == STORE_TABLE)
        .map(|k| k.id())
        .collect()
}

#[test]
fn stores_are_paged_and_a_restore_continues_exactly() {
    let mut e = engine();
    e.set_page_check(true);
    let mut disk = Disk::new();
    send(&mut e, 0, Obs::PackageStore(decl(S, vec![a(), b(), c()])));
    send(&mut e, 0, Obs::PackageStore(decl(U, vec![b(), c()])));
    send(
        &mut e,
        1,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            3,
            &[(IRON, 192)],
        ),
    );
    let (put, _) = save(&mut e, &mut disk);
    assert_eq!(stores_of(&put), vec![S.0, U.0]);
    // Only the store a record touched is written again.
    send(
        &mut e,
        5,
        chain(
            U,
            Some(D2),
            ChainDirection::Out,
            0,
            Some(1),
            1,
            &[(GOLD, 3)],
        ),
    );
    let (put, _) = save(&mut e, &mut disk);
    assert_eq!(stores_of(&put), vec![U.0]);
    let pages = disk.iter().map(|(k, b)| (PageKey(*k), b.clone()));
    let mut back = Engine::restore(pages, Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    let again: Disk = back.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    assert!(
        again == disk,
        "a restored engine saves what it was restored from"
    );
    // A removed store's page is deleted.
    send(&mut back, 10, Obs::PackageStore(removal(U, None)));
    let (_, deleted) = save(&mut back, &mut disk);
    assert_eq!(stores_of(&deleted), vec![U.0]);
    // A page under another store's key is refused, as a district's is.
    let mut broken = disk.clone();
    let b = broken
        .remove(&PageKey::new(STORE_TABLE, S.0).0)
        .expect("S's page");
    broken.insert(PageKey::new(STORE_TABLE, 77).0, b);
    let pages = broken.iter().map(|(k, b)| (PageKey(*k), b.clone()));
    assert!(Engine::restore(pages, Tunables::default()).is_err());
}

// ---- the frontier (slice S4c) ----------------------------------------------------------------

fn key(epoch: u32, serial: u64) -> HeldKey {
    HeldKey { epoch, serial }
}

/// A `CHAIN_HELD` of one package toward frogport `destination`, held on the conveyor at x = 15.
fn held(store: StoreId, destination: u32, k: HeldKey, goods: &[(Good, i64)]) -> Obs {
    Obs::ChainPackage(ChainPackage {
        store,
        district: None,
        inventory: None,
        direction: ChainDirection::Held,
        frogport: destination,
        pos: pos(15),
        destination: Some(destination),
        packages: 1,
        address: 0,
        op: None,
        key: Some(k),
        goods: GoodVec::from_pairs(goods.iter().copied()),
    })
}

/// A `CHAIN_RELEASED` of key `k`, naming `store`.
fn released(store: StoreId, k: HeldKey) -> Obs {
    Obs::ChainPackage(ChainPackage {
        store,
        district: None,
        inventory: None,
        direction: ChainDirection::Released,
        frogport: u32::MAX,
        pos: pos(15),
        destination: None,
        packages: 0,
        address: 0,
        op: None,
        key: Some(k),
        goods: GoodVec::new(),
    })
}

fn with_key(o: Obs, k: HeldKey) -> Obs {
    match o {
        Obs::ChainPackage(mut p) => {
            p.key = Some(k);
            Obs::ChainPackage(p)
        }
        o => o,
    }
}

fn epoch(mut d: StoreDecl, e: u32) -> StoreDecl {
    d.epoch = e;
    d
}

fn on_hold(e: &Engine, s: StoreId, f: StoreFrogport) -> Vec<HeldKey> {
    e.stores()[&s]
        .held_toward(f.pos)
        .iter()
        .map(|h| h.key)
        .collect()
}

/// A held package is booked toward its destination with its key, once, and leaves the pools as
/// they were (its `CHAIN_OUT` booked it); a key told again stays one package, in its place, and
/// moves with a new destination; its keyed `CHAIN_IN` strikes it and takes it from the pool as any
/// landing does. The held packages are paged with the store.
#[test]
fn the_frontier_holds_once_and_a_keyed_landing_strikes() {
    let mut e = engine();
    e.set_page_check(true);
    send(
        &mut e,
        0,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 1)),
    );
    send(
        &mut e,
        1,
        chain(
            S,
            Some(D1),
            ChainDirection::Out,
            0,
            Some(1),
            2,
            &[(IRON, 128)],
        ),
    );
    send(&mut e, 40, held(S, 1, key(1, 7), &[(IRON, 64)]));
    send(&mut e, 41, held(S, 1, key(1, 8), &[(IRON, 64)]));
    // Told again (a new session tells each held key once): still one package, in its place.
    send(&mut e, 90, held(S, 1, key(1, 7), &[(IRON, 64)]));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 7), key(1, 8)]);
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], pool(2, &[(IRON, 128)]));
    assert_eq!(
        (s.counts.held, s.counts.held_again, s.counts.held_stale),
        (2, 1, 0)
    );
    // Toward another destination (its route changed): it moves there.
    send(&mut e, 91, held(S, 2, key(1, 8), &[(IRON, 64)]));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 7)]);
    assert_eq!(on_hold(&e, S, c()), vec![key(1, 8)]);
    send(&mut e, 92, held(S, 1, key(1, 8), &[(IRON, 64)]));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 7), key(1, 8)]);
    // No port and no link: the district's books are as the `CHAIN_OUT` left them.
    let mut disk = Disk::new();
    save(&mut e, &mut disk);
    let pages = disk.iter().map(|(k, b)| (PageKey(*k), b.clone()));
    let back = Engine::restore(pages, Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    // The first lands at b, keyed.
    send(
        &mut e,
        300,
        with_key(
            chain(S, Some(D2), ChainDirection::In, 1, None, 1, &[(IRON, 64)]),
            key(1, 7),
        ),
    );
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 8)]);
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], pool(1, &[(IRON, 64)]));
    assert_eq!(s.counts.held_landed, 1);
    // A key the store does not hold (struck already) lands as any package does.
    send(
        &mut e,
        301,
        with_key(
            chain(S, Some(D2), ChainDirection::In, 1, None, 1, &[(IRON, 64)]),
            key(1, 7),
        ),
    );
    let s = &e.stores()[&S];
    assert_eq!(s.pools[1], Pool::default());
    assert_eq!(s.counts.held_landed, 1);
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 8)]);
}

/// A census leaves the held packages (they are on the chain; Java releases the keys it did not
/// find), and a release strikes one. A declaration of another epoch (Java's epochs start anywhere,
/// so a lower one too) drops the keys of the one before, and a hold of any epoch but the
/// declaration's is turned away.
#[test]
fn a_release_and_another_epoch_clear_the_held() {
    let mut e = engine();
    send(
        &mut e,
        0,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 1)),
    );
    send(&mut e, 10, held(S, 1, key(1, 1), &[(IRON, 64)]));
    send(&mut e, 11, held(S, 0, key(1, 2), &[(GOLD, 3)]));
    assert_eq!(on_hold(&e, S, a()), vec![key(1, 2)]);
    send(
        &mut e,
        200,
        Obs::StoreCensus(StoreCensus {
            store: S,
            packages: vec![(Some(1), 1)],
            goods: vec![(Some(1), IRON, 64)],
        }),
    );
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 1)]);
    assert_eq!(on_hold(&e, S, a()), vec![key(1, 2)]);
    send(&mut e, 200, held(S, 1, key(1, 3), &[(IRON, 64)]));
    send(&mut e, 201, released(S, key(1, 1)));
    send(&mut e, 202, released(S, key(1, 2)));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 3)]);
    assert!(on_hold(&e, S, a()).is_empty());
    let s = &e.stores()[&S];
    assert_eq!(s.counts.held_released, 2);
    // A release of a key no store holds changes nothing.
    send(&mut e, 203, released(S, key(1, 1)));
    assert_eq!(e.stores()[&S].counts.held_released, 2);
    send(&mut e, 204, released(S, key(1, 3)));
    send(&mut e, 205, held(S, 1, key(1, 4), &[(IRON, 64)]));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 4)]);
    // Java's epoch moves on (an engine reset, a re-enable): it declares every store again.
    send(
        &mut e,
        300,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 2)),
    );
    assert!(e.stores()[&S].on_hold.is_empty());
    send(&mut e, 301, held(S, 1, key(1, 5), &[(IRON, 64)]));
    assert!(e.stores()[&S].on_hold.is_empty());
    send(&mut e, 302, held(S, 1, key(2, 6), &[(IRON, 64)]));
    assert_eq!(on_hold(&e, S, b()), vec![key(2, 6)]);
    // A hold of an epoch the store was not declared with is turned away, a later one too.
    send(&mut e, 303, held(S, 0, key(3, 7), &[(GOLD, 3)]));
    assert_eq!(e.stores()[&S].epoch, 2);
    assert!(on_hold(&e, S, a()).is_empty());
    // A lower epoch is another epoch: it drops the keys of the one before.
    send(
        &mut e,
        400,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 1)),
    );
    assert!(e.stores()[&S].on_hold.is_empty());
    send(&mut e, 401, held(S, 0, key(1, 8), &[(GOLD, 3)]));
    assert_eq!(on_hold(&e, S, a()), vec![key(1, 8)]);
    assert_eq!(e.stores()[&S].counts.held_stale, 2);
}

/// Held packages follow their frogport into another store (a split), and into the store a
/// removed one names.
#[test]
fn held_packages_follow_their_frogport() {
    let mut e = engine();
    send(
        &mut e,
        0,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 1)),
    );
    send(&mut e, 10, held(S, 1, key(1, 1), &[(IRON, 64)]));
    // b moves to U.
    send(
        &mut e,
        20,
        Obs::PackageStore(epoch(decl(S, vec![a(), c()]), 1)),
    );
    send(
        &mut e,
        20,
        Obs::PackageStore(epoch(decl(U, vec![from(b(), S)]), 1)),
    );
    assert_eq!(on_hold(&e, U, b()), vec![key(1, 1)]);
    assert!(e.stores()[&S].on_hold.is_empty());
    // U goes, into S, which lists b again.
    send(
        &mut e,
        30,
        Obs::PackageStore(epoch(decl(S, vec![a(), b(), c()]), 1)),
    );
    send(&mut e, 30, Obs::PackageStore(removal(U, Some(S))));
    assert_eq!(on_hold(&e, S, b()), vec![key(1, 1)]);
    // A record naming another store than the one holding the key: a hold moves it there, a
    // release or a keyed landing strikes it where it is.
    send(
        &mut e,
        40,
        Obs::PackageStore(epoch(decl(U, vec![from(c(), S)]), 1)),
    );
    send(&mut e, 41, held(U, 0, key(1, 1), &[(IRON, 64)]));
    assert!(e.stores()[&S].on_hold.is_empty());
    assert_eq!(on_hold(&e, U, c()), vec![key(1, 1)]);
    assert_eq!(
        (e.stores()[&U].counts.held, e.stores()[&U].counts.held_again),
        (0, 1)
    );
    send(&mut e, 42, released(S, key(1, 1)));
    assert!(e.stores()[&U].on_hold.is_empty());
    assert_eq!(e.stores()[&U].counts.held_released, 1);
}
