// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package stores between replays (SECTIONS.md §2.5, "Both ends away", slice S4b): the districts
//! at both ends of a store replay as one cluster around a shared pool per destination frogport,
//! what that pool holds goes back to the store when no replay touches it, and the engine writes it
//! into a loaded frogport as real packages (IND_WRITE_CHAIN).

use rtsim_industry::cmd::{Cmd, WriteChain};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, StoreId};
use rtsim_industry::learn::{Learned, LinkKey};
use rtsim_industry::obs::{
    ChainDirection, ChainPackage, DistrictDecl, InvKind, InventoryDecl, LoadState, Obs,
    StoreDecl, StoreFrogport, SweepEnd, Timed,
};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::PageKey;

const IRON: Good = Good(1);
/// The exporter and the importer.
const MINE: DistrictId = DistrictId(1);
const MILL: DistrictId = DistrictId(2);
const S: StoreId = StoreId(5);
/// Each district's frogport is its inventory 1; its vault is inventory 0.
const PORT: u32 = 1;
/// One package's contents toward the mill.
const BATCH: i64 = 64;

/// The mine's frogport (chunk 0) and the mill's (chunk 1), one hop apart.
fn frog(d: DistrictId) -> BlockPos {
    BlockPos::new(if d == MINE { 10 } else { 20 }, 64, 0)
}

fn vault(d: DistrictId) -> BlockPos {
    BlockPos::new(if d == MINE { -100 } else { 200 }, 64, 0)
}

fn chunks(d: DistrictId) -> [ChunkPos; 2] {
    [vault(d), frog(d)].map(|p| ChunkPos {
        dim: 0,
        x: p.x >> 4,
        z: p.z >> 4,
    })
}

fn engine(tun: Tunables) -> Engine {
    let mut e = Engine::new(1, tun, Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    e
}

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

fn store() -> StoreDecl {
    let port = |d: DistrictId| StoreFrogport {
        district: Some(d),
        inventory: Some(PORT),
        star: false,
        pos: frog(d),
        conveyor: BlockPos::new(frog(d).x + 1, 64, 0),
        from: None,
    };
    StoreDecl {
        store: S,
        dim: 0,
        into: None,
        frogports: vec![port(MINE), port(MILL)],
        hops: vec![1],
    }
}

fn chain(d: DistrictId, direction: ChainDirection, packages: u32, op: Option<u64>) -> Obs {
    Obs::ChainPackage(ChainPackage {
        store: S,
        district: Some(d),
        inventory: Some(PORT),
        direction,
        frogport: if d == MINE { 0 } else { 1 },
        destination: Some(1),
        packages,
        address: 0,
        op: op.map(rtsim_kernel::journal::OpId),
        goods: GoodVec::from_pairs([(IRON, BATCH * i64::from(packages))]),
    })
}

/// A live district with a vault holding `stock` iron and a frogport, learned with `rate` iron a
/// tick made (negative: used) and a chain link of `link` a tick toward the mill's frogport.
fn district(e: &mut Engine, d: DistrictId, stock: i64, rate: f64, link: f64) {
    send(
        e,
        0,
        Obs::District(DistrictDecl {
            district: d,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 2,
        }),
    );
    for (index, pos, kind, slots) in [
        (0u32, vault(d), InvKind::ITEMS, 100u32),
        (PORT, frog(d), InvKind::ITEMS | InvKind::POSTBOX, 18),
    ] {
        send(
            e,
            0,
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 1,
                index,
                pos,
                kind: InvKind(kind),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    live(e, d, 0);
    for (inventory, n) in [(0, stock), (PORT, 0)] {
        send(
            e,
            10,
            Obs::StockSample {
                district: d,
                inventory,
                sweep: 1,
                goods: GoodVec::from_pairs([(IRON, n)]),
            },
        );
    }
    send(
        e,
        10,
        Obs::SweepEnd(SweepEnd {
            district: d,
            sweep: 1,
            records: 2,
            inventories: 2,
            live: true,
            creative: false,
            overstressed: false,
        }),
    );
    e.adopt_model(
        d,
        Learned {
            at: Tick(20),
            window_ticks: 12_000,
            rates: vec![(IRON, rate)],
            links: vec![(
                LinkKey::Chain {
                    inventory: PORT,
                    to: frog(MILL),
                    good: IRON,
                },
                link,
            )],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
}

fn live(e: &mut Engine, d: DistrictId, at: u64) {
    for chunk in chunks(d) {
        send(
            e,
            at,
            Obs::ChunkLoaded {
                district: d,
                chunk,
                applied_epoch: 0,
            },
        );
    }
    send(
        e,
        at,
        Obs::LoadState {
            district: d,
            state: LoadState::Live,
            ticking_chunks: 2,
            total_chunks: 2,
        },
    );
}

/// The district unloads at `at`; its replay starts once the tick's records are in.
fn away(e: &mut Engine, d: DistrictId, at: u64) {
    send(
        e,
        at,
        Obs::LoadState {
            district: d,
            state: LoadState::Unloaded,
            ticking_chunks: 0,
            total_chunks: 2,
        },
    );
    e.advance_to(Tick(at + 1));
}

/// The mine and the mill on one store, the mine's 64-iron packages seen once while loaded.
fn pair(tun: Tunables) -> Engine {
    let mut e = engine(tun);
    send(&mut e, 0, Obs::PackageStore(store()));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    district(&mut e, MILL, 0, -0.04, 0.05);
    send(&mut e, 15, chain(MINE, ChainDirection::Out, 1, None));
    e
}

fn writes(e: &mut Engine) -> Vec<WriteChain> {
    e.drain_commands()
        .into_iter()
        .filter_map(|c| match c {
            Cmd::WriteChain(w) => Some(w),
            _ => None,
        })
        .collect()
}

fn owed(e: &Engine) -> i64 {
    e.stores()[&S].owed_to(frog(MILL)).get(IRON)
}

/// Java writing every IND_WRITE_CHAIN whole: a `CHAIN_IN` with its op for what it wrote.
fn written(e: &mut Engine, at: u64, w: &WriteChain) {
    let n = i64::from(w.count) * w.goods.get(IRON);
    send(
        e,
        at,
        Obs::ChainPackage(ChainPackage {
            store: w.store,
            district: Some(w.district),
            inventory: Some(w.inventory),
            direction: ChainDirection::In,
            frogport: w.frogport,
            destination: Some(w.frogport),
            packages: w.count,
            address: 0,
            op: Some(w.op),
            goods: GoodVec::from_pairs([(IRON, n)]),
        }),
    );
}

/// Advances in steps of `step` ticks to `to`, Java writing every write as it comes; returns the
/// writes.
fn run_writing(e: &mut Engine, from: u64, to: u64, step: u64) -> Vec<WriteChain> {
    let mut all = Vec::new();
    let mut t = from;
    while t < to {
        t = (t + step).min(to);
        e.advance_to(Tick(t));
        for w in writes(e) {
            written(e, t, &w);
            all.push(w);
        }
    }
    all
}

fn iron(w: &[WriteChain]) -> i64 {
    w.iter().map(|w| i64::from(w.count) * w.goods.get(IRON)).sum()
}

#[test]
fn both_ends_away_replay_as_one_cluster_and_conserve_the_iron() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    assert!(e.replays(MINE) && e.replays(MILL));
    assert_eq!(e.cluster_count(), 1, "one pool joins their clusters");
    assert_eq!(
        e.cluster_members(MINE).expect("replays"),
        [MINE, MILL].into_iter().collect()
    );
    let t = 100 + 24_000;
    e.advance_to(Tick(t));
    let mine = e.sim_stock(MINE).expect("mine").get(IRON);
    let mill = e.sim_stock(MILL).expect("mill").get(IRON);
    // A day: the mine makes 1,200 and sends them; the mill takes them a hop later and uses 960.
    assert!(
        (1_000 - 40..=1_000 + 40).contains(&mine),
        "the mine sends what it makes: {mine}"
    );
    assert!((200..=260).contains(&mill), "the mill keeps the rest: {mill}");
    e.check_books().expect("books");
    // Nothing is written while both are away, and the store owes nothing (a cluster holds the
    // pool).
    assert!(writes(&mut e).is_empty());
    assert_eq!(owed(&e), 0);
}

#[test]
fn an_importer_that_loads_first_is_written_what_the_exporter_still_sends() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    e.advance_to(Tick(12_100));
    e.drain_commands();
    // The mill loads; the mine keeps replaying, its exports filling the pool toward the mill.
    live(&mut e, MILL, 12_100);
    assert!(!e.replays(MILL) && e.replays(MINE));
    e.drain_commands();
    let w = run_writing(&mut e, 12_100, 18_100, 50);
    // 6,000 ticks at 0.05 a tick: 300 iron, written as they make whole packages.
    let sent = iron(&w);
    assert!((192..=320).contains(&sent), "{sent}");
    for x in &w {
        assert_eq!(
            (x.store, x.district, x.inventory, x.frogport, x.pos),
            (S, MILL, PORT, 1, frog(MILL))
        );
        assert_eq!(x.goods.get(IRON), BATCH, "whole packages while the mine still sends");
    }
    assert_eq!(e.stores()[&S].counts.written as i64 * BATCH, sent);
    assert!(
        e.district(MILL).expect("mill").links.is_empty(),
        "a write is no link"
    );
    assert_eq!(owed(&e), 0, "a cluster still holds the pool");
    // The mine loads: what is left in the pool goes back to the store and is written, a part
    // package last.
    live(&mut e, MINE, 18_100);
    assert!(!e.replays(MINE));
    let left = owed(&e);
    assert!((1..BATCH).contains(&left), "{left}");
    let w = run_writing(&mut e, 18_100, 19_100, 50);
    assert_eq!(iron(&w), left);
    assert_eq!(owed(&e), 0);
    // Nothing is owed: no more writes.
    e.advance_to(Tick(21_000));
    assert!(writes(&mut e).is_empty());
    e.check_books().expect("books");
}

#[test]
fn an_exporter_alone_fills_the_room_and_the_importer_draws_it_later() {
    let mut e = pair(Tunables::default());
    // The mill stays loaded but its frogport is full: Java writes nothing, so nothing is taken.
    away(&mut e, MINE, 100);
    e.advance_to(Tick(100 + 96_000));
    assert!(!writes(&mut e).is_empty());
    // The room: 20 packages on each of the two conveyors, less the one the store holds.
    let room = (20 * 2 - 1) * BATCH;
    live(&mut e, MINE, 96_100);
    let o = owed(&e);
    assert!(
        (room - BATCH..=room).contains(&o),
        "the export stops at the route's room: {o}"
    );
    // The mill goes away: its replay starts from what it is owed, and draws it.
    away(&mut e, MILL, 96_200);
    assert_eq!(owed(&e), 0, "the pool starts from the store's debt");
    e.advance_to(Tick(96_200 + 72_000));
    live(&mut e, MILL, 168_200);
    assert_eq!(owed(&e), 0, "the mill drew it all");
    e.check_books().expect("books");
}

#[test]
fn past_the_cluster_limit_a_district_replays_without_its_chain_links() {
    let mut tun = Tunables::default();
    tun.replay.cluster_districts_max = 1;
    let mut e = pair(tun);
    e.log_enabled = true;
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    assert!(e.replays(MINE) && e.replays(MILL));
    assert_eq!(e.cluster_count(), 2);
    assert_eq!(e.stats.chain_links_dropped, 1);
    assert!(
        e.log
            .iter()
            .filter(|l| l.what.contains("chain links do not replay"))
            .count()
            == 1
    );
}

#[test]
fn chain_links_are_learned_while_loaded_and_send_the_district_to_the_black_box() {
    let mut e = engine(Tunables::default());
    send(&mut e, 0, Obs::PackageStore(store()));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    district(&mut e, MILL, 0, -0.04, 0.05);
    send(&mut e, 30, chain(MINE, ChainDirection::Out, 2, None));
    send(&mut e, 40, chain(MILL, ChainDirection::In, 1, None));
    let key = LinkKey::Chain {
        inventory: PORT,
        to: frog(MILL),
        good: IRON,
    };
    assert_eq!(e.district(MINE).expect("mine").links[&key], -2 * BATCH);
    assert_eq!(e.district(MILL).expect("mill").links[&key], BATCH);
    // Packages toward no frogport (circling) or across a district's own replay are not learned.
    away(&mut e, MINE, 50);
    send(&mut e, 60, chain(MINE, ChainDirection::Out, 1, None));
    assert_eq!(e.district(MINE).expect("mine").links[&key], -2 * BATCH);
}

#[test]
fn chain_pools_and_debts_survive_a_restore() {
    let mut e = pair(Tunables::default());
    e.set_page_check(true);
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    e.advance_to(Tick(6_100));
    live(&mut e, MILL, 6_100);
    e.advance_to(Tick(6_150));
    e.drain_commands();
    let c = e.save_changed();
    let pages: Vec<(PageKey, Vec<u8>)> = c.put;
    let mut back = Engine::restore(pages.into_iter(), Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    back.advance_to(Tick(9_000));
    e.advance_to(Tick(9_000));
    assert_eq!(writes(&mut back), writes(&mut e), "the restored engine writes as the original");
    assert_eq!(back.sim_stock(MINE), e.sim_stock(MINE));
}
