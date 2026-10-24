// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package stores (SECTIONS.md §2.5, slice S4): a chain network whose frogports sit in more than one
//! district holds the packages in transit, with a pool per destination frogport. These tests drive
//! the engine with the records Java sends (IND_PACKAGE_STORE, IND_CHAIN_PACKAGE, IND_STORE_CENSUS).

use std::collections::BTreeMap;

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StoreId};
use rtsim_industry::obs::{
    ChainDirection, ChainPackage, Obs, StoreCensus, StoreDecl, StoreFrogport, Timed,
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
        into: None,
        frogports,
        hops,
    }
}

fn removal(store: StoreId, into: Option<StoreId>) -> StoreDecl {
    StoreDecl {
        store,
        dim: 0,
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
        destination,
        packages,
        address: 0,
        op: None,
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
#[test]
fn a_chain_package_is_a_port_of_its_district_and_no_link() {
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
    let d1 = e.district(D1).expect("D1");
    assert_eq!(d1.ports.get(IRON), -64);
    assert!(d1.links.is_empty(), "{:?}", d1.links);
    assert!(d1.batches.is_empty());
    let d2 = e.district(D2).expect("D2");
    assert_eq!(d2.ports.get(IRON), 64);
    assert!(d2.links.is_empty());
    // A player's frogport has no district: the store books it, no district is made.
    send(
        &mut e,
        40,
        chain(S, None, ChainDirection::Out, 1, Some(0), 1, &[(GOLD, 1)]),
    );
    assert_eq!(e.districts().count(), 2);
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
