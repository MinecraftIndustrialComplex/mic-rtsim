// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package stores between replays (SECTIONS.md §2.5, "Both ends away", slice S4b): the districts
//! at both ends of a store replay as one cluster around a shared pool per destination frogport,
//! what that pool holds goes back to the store when no replay touches it, and the engine writes it
//! into a loaded frogport as real packages (IND_WRITE_CHAIN).

use rtsim_industry::cmd::{ChunkReconcile, Cmd, DrawAction, HeldDraw, WriteChain};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, StoreId};
use rtsim_industry::learn::{Learned, LinkKey};
use rtsim_industry::obs::{
    ChainDirection, ChainPackage, DistrictDecl, HeldKey, HeldList, InvKind, InventoryDecl,
    LoadState, Obs, ReconcileResult, ResultEntry, StoreCensus, StoreDecl, StoreFrogport, SweepEnd,
    Timed,
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
        epoch: 0,
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
        pos: frog(d),
        destination: Some(1),
        packages,
        address: 0,
        op: op.map(rtsim_kernel::journal::OpId),
        key: None,
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
            pos: w.pos,
            destination: Some(w.frogport),
            packages: w.count,
            address: 0,
            op: Some(w.op),
            key: None,
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
    w.iter()
        .map(|w| i64::from(w.count) * w.goods.get(IRON))
        .sum()
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
    assert!(
        (200..=260).contains(&mill),
        "the mill keeps the rest: {mill}"
    );
    e.check_books().expect("books");
    // Nothing is written while both are away, and the store owes nothing (a cluster holds the
    // pool).
    assert!(writes(&mut e).is_empty());
    assert_eq!(owed(&e), 0);
}

/// A pair Java's walk did not reach (0xFFFF hops) is no route: the replay sends nothing along it,
/// where 65,535 hops read as a chain room past any batch and a travel of days.
#[test]
fn a_pair_of_unknown_hops_replays_no_chain_link() {
    let mut e = engine(Tunables::default());
    let mut decl = store();
    decl.hops = vec![rtsim_industry::stores::UNKNOWN_HOPS];
    send(&mut e, 0, Obs::PackageStore(decl));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    district(&mut e, MILL, 0, -0.04, 0.05);
    send(&mut e, 15, chain(MINE, ChainDirection::Out, 1, None));
    let held = e.stores()[&S].held().packages;
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    e.advance_to(Tick(100 + 24_000));
    // No pool joins their clusters, and the mine keeps the day's 1,200 it would have sent.
    assert!(e.replays(MINE) && e.replays(MILL));
    assert_eq!(e.cluster_count(), 2, "no chain pool joins their clusters");
    let mine = e.sim_stock(MINE).expect("mine").get(IRON);
    let mill = e.sim_stock(MILL).map_or(0, |s| s.get(IRON));
    assert!(
        (2_160..=2_240).contains(&mine),
        "the mine sends nothing down the chain: {mine}"
    );
    assert_eq!(mill, 0, "the mill gets nothing by the chain");
    assert_eq!(
        e.stores()[&S].held().packages,
        held,
        "the replay put no package on the chain"
    );
    e.check_books().expect("books");
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
        assert_eq!(
            x.goods.get(IRON),
            BATCH,
            "whole packages while the mine still sends"
        );
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
    let mut back = Engine::restore(pages, Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    back.advance_to(Tick(9_000));
    e.advance_to(Tick(9_000));
    assert_eq!(
        writes(&mut back),
        writes(&mut e),
        "the restored engine writes as the original"
    );
    assert_eq!(back.sim_stock(MINE), e.sim_stock(MINE));
}

/// A frogport's chunk carries a written `CHAIN_IN` until the engine acknowledges it; one that
/// unloads first journals it again at its next load, after WORLD_SAVED let the replay forget its op
/// (choice SC113). The engine takes it once.
#[test]
fn a_written_chain_in_journaled_again_later_is_taken_once() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    // The mill's frogport is full: the write fits nothing. One long step sends one write, which
    // holds the next until its answer.
    let failed = writes(&mut e);
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(e.command_failed(Tick(20_100), failed[0].op));
    assert_eq!(e.stats.chain_writes_failed, 1);
    // The mine loads: the pool goes back to the store, owed to the mill's frogport.
    live(&mut e, MINE, 20_100);
    let o = owed(&e);
    assert!(o >= 2 * BATCH, "{o}");
    e.advance_to(Tick(20_400));
    let w = writes(&mut e);
    assert_eq!(w.len(), 1, "one write, the next waits for its answer");
    let took = i64::from(w[0].count) * w[0].goods.get(IRON);
    written(&mut e, 20_400, &w[0]);
    assert_eq!(owed(&e), o - took);
    let counted = e.stores()[&S].counts.written;
    // Much later the frogport's chunk loads and journals the same record again.
    e.advance_to(Tick(30_000));
    for x in writes(&mut e) {
        e.command_failed(Tick(30_000), x.op);
    }
    written(&mut e, 20_400, &w[0]);
    assert_eq!(owed(&e), o - took, "taken once");
    assert_eq!(e.stats.chain_writes_again, 1);
    assert_eq!(e.stores()[&S].counts.written, counted);
    e.check_books().expect("books");
}

/// Without an answer (a crash between the command and its result), a frogport is written again only
/// once the answer is taken as lost, ten periods after the engine handed the write on; a write still
/// in the engine's outbox always holds the next, so one long step sends one (choice SC113).
#[test]
fn an_unanswered_write_holds_the_next_until_its_answer_is_lost() {
    let mut e = pair(Tunables::default());
    let period = Tunables::default().replay.chain_write_ticks;
    away(&mut e, MINE, 100);
    // One step of 20,000 ticks; the first whole package is there after about 1,300.
    e.advance_to(Tick(20_100));
    let first = writes(&mut e);
    assert_eq!(first.len(), 1, "one write a step: {first:?}");
    // Handed on at 20,100 and never answered: nothing for ten periods, then one more.
    e.advance_to(Tick(20_100 + 10 * period - 1));
    assert!(writes(&mut e).is_empty());
    e.advance_to(Tick(20_100 + 11 * period));
    let next = writes(&mut e);
    assert_eq!(next.len(), 1, "{next:?}");
    assert_ne!(next[0].op, first[0].op);
    assert_eq!(e.stats.chain_writes_unanswered, 0);
    // Drained a period at a time and never answered: a write each ten periods, each kept until a
    // hundred periods after it went.
    let mut t = 20_100 + 11 * period;
    let mut n = 2;
    while t < 20_100 + 105 * period {
        t += period;
        e.advance_to(Tick(t));
        n += writes(&mut e).len();
    }
    assert!((9..=12).contains(&n), "{n}");
    assert!(e.stats.chain_writes_unanswered >= 1);
    let kept = e.stores()[&S].writing[&frog(MILL)].len();
    assert!(kept <= rtsim_industry::stores::WRITES_KEPT, "{kept}");
    // The first write's answer arrives at last: judged by the mark, which is empty, so taken.
    let counted = e.stores()[&S].counts.written;
    written(&mut e, t, &first[0]);
    assert_eq!(
        e.stores()[&S].counts.written,
        counted + u64::from(first[0].count)
    );
}

/// After a crash the world's clock goes back to its last save, while a frogport's chunk saved at its
/// unload carries a written `CHAIN_IN` of the lost stretch, recorded later than what Java records
/// next (choice SC113). That record is taken once, and the next write, applied at an earlier game
/// time, is taken too: the answer to a write the engine waits for is never judged by the mark.
#[test]
fn a_write_after_a_crash_is_taken_though_a_lost_record_is_later() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    for x in writes(&mut e) {
        e.command_failed(Tick(20_100), x.op);
    }
    live(&mut e, MINE, 20_100);
    let o0 = owed(&e);
    assert!(o0 >= 3 * BATCH, "{o0}");
    // The frogport's chunk journals a write of the lost stretch, its op forgotten, at 23,000.
    let lost = ChainPackage {
        store: S,
        district: Some(MILL),
        inventory: Some(PORT),
        direction: ChainDirection::In,
        frogport: 1,
        pos: frog(MILL),
        destination: Some(1),
        packages: 1,
        address: 0,
        op: Some(rtsim_kernel::journal::OpId((0x49 << 56) | 0xabc)),
        key: None,
        goods: GoodVec::from_pairs([(IRON, BATCH)]),
    };
    send(&mut e, 23_000, Obs::ChainPackage(lost.clone()));
    assert_eq!(owed(&e), o0 - BATCH, "the lost stretch's write is taken");
    send(&mut e, 23_000, Obs::ChainPackage(lost));
    assert_eq!(owed(&e), o0 - BATCH, "once");
    assert_eq!(e.stats.chain_writes_again, 1);
    e.advance_to(Tick(23_400));
    let w = writes(&mut e);
    assert_eq!(w.len(), 1, "{w:?}");
    let o1 = owed(&e);
    let counted = e.stores()[&S].counts.written;
    // Java writes it at its own game time, before the lost record's.
    written(&mut e, 20_500, &w[0]);
    let taken = counted + u64::from(w[0].count);
    assert_eq!(
        e.stores()[&S].counts.written,
        taken,
        "the new write is taken"
    );
    assert_eq!(owed(&e), (o1 - iron(&w)).max(0));
    assert_eq!(e.stats.chain_writes_again, 1);
    // Journaled again later, it is judged by the mark: taken already.
    written(&mut e, 20_500, &w[0]);
    assert_eq!(e.stores()[&S].counts.written, taken);
    assert_eq!(owed(&e), (o1 - iron(&w)).max(0));
    assert_eq!(e.stats.chain_writes_again, 2);
    e.check_books().expect("books");
}

/// A store declared again with a frogport added before the mill's shifts the mill's index; a
/// written record journaled again with the old index names the frogport by position as well, so the
/// mark still drops it (choice SC113).
#[test]
fn a_written_record_journaled_again_after_a_redeclare_is_taken_once() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    for x in writes(&mut e) {
        e.command_failed(Tick(20_100), x.op);
    }
    live(&mut e, MINE, 20_100);
    e.advance_to(Tick(20_400));
    let w = writes(&mut e);
    assert_eq!(w.len(), 1, "{w:?}");
    written(&mut e, 20_400, &w[0]);
    let counted = e.stores()[&S].counts.written;
    let ports = e.district(MILL).expect("mill").ports.get(IRON);
    let mut d = store();
    d.frogports.insert(
        1,
        StoreFrogport {
            district: None,
            inventory: None,
            star: false,
            pos: BlockPos::new(15, 64, 0),
            conveyor: BlockPos::new(16, 64, 0),
            from: None,
        },
    );
    d.hops = vec![1, 2, 1];
    send(&mut e, 25_000, Obs::PackageStore(d));
    // Much later the mill's chunk journals the record again, with its old index 1.
    e.advance_to(Tick(30_000));
    for x in writes(&mut e) {
        e.command_failed(Tick(30_000), x.op);
    }
    let o = owed(&e);
    written(&mut e, 20_400, &w[0]);
    assert_eq!(e.stats.chain_writes_again, 1);
    assert_eq!(e.stores()[&S].counts.written, counted);
    assert_eq!(e.district(MILL).expect("mill").ports.get(IRON), ports);
    assert_eq!(owed(&e), o);
}

/// A write made inside an ingest whose frogport moves to another store in the same batch is timed
/// in that store when it is handed on: its answer lost, the engine writes there again.
#[test]
fn a_write_moved_to_another_store_before_its_drain_is_timed_there() {
    let tun = Tunables::default();
    let period = tun.replay.chain_write_ticks;
    let mut e = pair(tun);
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    for x in writes(&mut e) {
        e.command_failed(Tick(20_100), x.op);
    }
    live(&mut e, MINE, 20_100);
    // Any record at 20_400 wakes the store's writes inside the ingest.
    send(
        &mut e,
        20_400,
        Obs::StockSample {
            district: MINE,
            inventory: 0,
            sweep: 9,
            goods: GoodVec::from_pairs([(IRON, 500)]),
        },
    );
    const S2: StoreId = StoreId(6);
    let mut whole = store();
    let mill = whole.frogports.pop().expect("mill");
    send(
        &mut e,
        20_400,
        Obs::PackageStore(StoreDecl {
            store: S2,
            dim: 0,
            epoch: 0,
            into: None,
            frogports: vec![StoreFrogport {
                from: Some(S),
                ..mill
            }],
            hops: vec![],
        }),
    );
    whole.hops = vec![];
    send(&mut e, 20_400, Obs::PackageStore(whole));
    assert!(!writes(&mut e).is_empty());
    let moved = &e.stores()[&S2].writing[&frog(MILL)];
    assert!(moved.iter().all(|w| w.at.is_some()), "{moved:?}");
    // Its answer is lost: past the answer's wait the engine writes there again.
    live(&mut e, MILL, 20_401);
    let mut n = 0;
    let mut t = 20_401;
    while t < 20_401 + 30 * period {
        t += period;
        e.advance_to(Tick(t));
        n += writes(&mut e).len();
    }
    assert!(n > 0, "the frogport is written again");
}

/// A frogport broken and rebuilt into another store leaves its mark in the old one, which still
/// turns away the record its chunk journals again.
#[test]
fn a_frogport_rebuilt_into_another_store_turns_its_old_record_away() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    for x in writes(&mut e) {
        e.command_failed(Tick(20_100), x.op);
    }
    live(&mut e, MINE, 20_100);
    e.advance_to(Tick(20_400));
    let w = writes(&mut e);
    assert_eq!(w.len(), 1, "{w:?}");
    written(&mut e, 20_400, &w[0]);
    let ports = e.district(MILL).expect("mill").ports.get(IRON);
    const S3: StoreId = StoreId(7);
    let mut d = store();
    let mill = d.frogports.pop().expect("mill");
    d.hops = vec![];
    send(&mut e, 25_000, Obs::PackageStore(d));
    send(
        &mut e,
        25_000,
        Obs::PackageStore(StoreDecl {
            store: S3,
            dim: 0,
            epoch: 0,
            into: None,
            frogports: vec![mill],
            hops: vec![],
        }),
    );
    written(&mut e, 20_400, &w[0]);
    assert_eq!(e.stats.chain_writes_again, 1);
    assert_eq!(e.stores()[&S3].counts.written, 0);
    assert_eq!(e.district(MILL).expect("mill").ports.get(IRON), ports);
}

/// A written record whose store no longer lists its frogport is taken in its own store, never in
/// a store of another dimension that has a frogport at the same position.
#[test]
fn a_written_record_is_never_taken_in_another_dimension() {
    let mut e = pair(Tunables::default());
    const S4: StoreId = StoreId(8);
    let mut d = store();
    let mill = d.frogports.pop().expect("mill");
    d.hops = vec![];
    send(&mut e, 100, Obs::PackageStore(d));
    send(
        &mut e,
        100,
        Obs::PackageStore(StoreDecl {
            store: S4,
            dim: 1,
            epoch: 0,
            into: None,
            frogports: vec![StoreFrogport {
                district: None,
                inventory: None,
                ..mill
            }],
            hops: vec![],
        }),
    );
    let before = e.stores()[&S].counts.written;
    send(&mut e, 200, chain(MILL, ChainDirection::In, 2, Some(999)));
    assert_eq!(e.stores()[&S4].counts.written, 0);
    assert_eq!(e.stores()[&S].counts.written, before + 2);
}

/// A frogport's mark and its unanswered writes are saved with the store: after a restore the record
/// journaled again is dropped, and the answer to the write in flight is taken.
#[test]
fn write_marks_and_unanswered_writes_survive_a_restore() {
    let mut e = pair(Tunables::default());
    e.set_page_check(true);
    away(&mut e, MINE, 100);
    e.advance_to(Tick(20_100));
    for x in writes(&mut e) {
        e.command_failed(Tick(20_100), x.op);
    }
    live(&mut e, MINE, 20_100);
    e.advance_to(Tick(20_400));
    let w = writes(&mut e);
    assert_eq!(w.len(), 1, "{w:?}");
    written(&mut e, 20_400, &w[0]);
    e.advance_to(Tick(20_700));
    let pending = writes(&mut e);
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert!(!e.stores()[&S].marks.is_empty());
    assert!(!e.stores()[&S].writing.is_empty());
    let c = e.save_changed();
    let mut back = Engine::restore(c.put, Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    let o = back.stores()[&S].owed_to(frog(MILL)).get(IRON);
    written(&mut back, 20_400, &w[0]);
    assert_eq!(back.stats.chain_writes_again, 1);
    assert_eq!(back.stores()[&S].owed_to(frog(MILL)).get(IRON), o);
    written(&mut back, 20_700, &pending[0]);
    assert_eq!(
        back.stores()[&S].owed_to(frog(MILL)).get(IRON),
        o - iron(&pending)
    );
    assert!(back.stores()[&S].writing.is_empty());
}

/// A split moves the importer's frogport to another store while a cluster holds the pool toward
/// it: the pool goes back to the store that lists the frogport now.
#[test]
fn a_pool_returns_to_the_store_that_lists_its_frogport_now() {
    let mut e = pair(Tunables::default());
    away(&mut e, MINE, 100);
    away(&mut e, MILL, 100);
    e.advance_to(Tick(6_100));
    const S2: StoreId = StoreId(6);
    let mut whole = store();
    let mill = whole.frogports.pop().expect("mill");
    send(
        &mut e,
        6_100,
        Obs::PackageStore(StoreDecl {
            store: S2,
            dim: 0,
            epoch: 0,
            into: None,
            frogports: vec![rtsim_industry::obs::StoreFrogport {
                from: Some(S),
                ..mill
            }],
            hops: vec![],
        }),
    );
    whole.hops = vec![];
    send(&mut e, 6_100, Obs::PackageStore(whole));
    // The mill loads first and does not draw, then the mine: the pool leaves the cluster.
    live(&mut e, MILL, 6_200);
    live(&mut e, MINE, 6_200);
    assert!(!e.replays(MINE) && !e.replays(MILL));
    let moved = e.stores()[&S2].owed_to(frog(MILL)).get(IRON);
    assert!(moved > 0, "{:?}", e.stores()[&S2].owed);
    assert!(e.stores()[&S].owed.is_empty(), "{:?}", e.stores()[&S].owed);
    assert_eq!(e.stats.chain_pools_lost, 0);
}

/// A district the black box replays in part, its frogport's chunk loaded: packages Create moves
/// there are the world's, an ordinary port, never the replay's stock. Its replay has no import
/// link there (the frogport is not in its part), so the engine writes what the mine sends.
#[test]
fn a_loaded_frogport_of_a_partly_replayed_district_is_no_part_of_the_replay() {
    let mut e = engine(Tunables::default());
    send(&mut e, 0, Obs::PackageStore(store()));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    // The mill's vault has iron to use while away.
    district(&mut e, MILL, 500, -0.04, 0.05);
    send(&mut e, 15, chain(MINE, ChainDirection::Out, 1, None));
    away(&mut e, MINE, 100);
    // The mill's vault unloads, its frogport stays loaded.
    send(
        &mut e,
        200,
        Obs::ChunkUnloaded {
            district: MILL,
            chunk: chunks(MILL)[0],
        },
    );
    send(
        &mut e,
        200,
        Obs::LoadState {
            district: MILL,
            state: LoadState::Partial,
            ticking_chunks: 1,
            total_chunks: 2,
        },
    );
    e.advance_to(Tick(201));
    assert!(e.replays(MILL), "the vault replays");
    e.advance_to(Tick(400));
    let before = e.sim_stock(MILL).expect("replays").get(IRON);
    let ports = e.district(MILL).expect("mill").ports.get(IRON);
    send(&mut e, 400, chain(MILL, ChainDirection::In, 1, None));
    assert_eq!(e.sim_stock(MILL).expect("replays").get(IRON), before);
    assert_eq!(
        e.district(MILL).expect("mill").ports.get(IRON),
        ports + BATCH
    );
    let w = run_writing(&mut e, 400, 12_000, 50);
    assert!(!w.is_empty());
    assert!(w.iter().all(|x| x.pos == frog(MILL) && x.district == MILL));
    assert_eq!(
        e.sim_stock(MILL).expect("replays").get(IRON),
        before - (0.04 * 11_600.0) as i64,
        "the writes are the frogport's, not the vault's"
    );
    e.check_books().expect("books");
}

// ---- the frontier's draw (slice S4c2) ----------------------------------------------------------

const K7: HeldKey = HeldKey {
    epoch: 0,
    serial: 7,
};
const K8: HeldKey = HeldKey {
    epoch: 0,
    serial: 8,
};
const K9: HeldKey = HeldKey {
    epoch: 0,
    serial: 9,
};

/// The mine loaded, three of its packages on the chain toward the mill, and the mill away with
/// nothing to use: its replay's iron is what it imports.
fn frontier() -> Engine {
    let mut e = engine(Tunables::default());
    send(&mut e, 0, Obs::PackageStore(store()));
    district(&mut e, MINE, 1_000, 0.05, -0.05);
    district(&mut e, MILL, 0, 0.0, 0.05);
    send(&mut e, 15, chain(MINE, ChainDirection::Out, 3, None));
    away(&mut e, MILL, 100);
    assert!(e.replays(MILL) && !e.replays(MINE));
    e.drain_commands();
    e
}

/// The frontier's record of key `k`, one package toward the mill's frogport held on the mine's
/// side: `CHAIN_HELD`, `CHAIN_DRAWN` or `CHAIN_RELEASED`.
fn keyed(direction: ChainDirection, k: HeldKey) -> Obs {
    let released = direction == ChainDirection::Released;
    Obs::ChainPackage(ChainPackage {
        store: S,
        district: None,
        inventory: None,
        direction,
        frogport: if released { u32::MAX } else { 1 },
        pos: BlockPos::new(15, 64, 0),
        destination: (!released).then_some(1),
        packages: u32::from(!released),
        address: 0,
        op: None,
        key: Some(k),
        goods: if released {
            GoodVec::new()
        } else {
            GoodVec::from_pairs([(IRON, BATCH)])
        },
    })
}

/// The engine's IND_HELD_DRAW asks, as (action, serials), and its reconciles.
fn asks(e: &mut Engine) -> (Vec<(DrawAction, Vec<u64>)>, Vec<ChunkReconcile>) {
    let mut draws = Vec::new();
    let mut reconciles = Vec::new();
    for c in e.drain_commands() {
        match c {
            Cmd::HeldDraw(HeldDraw {
                store,
                epoch,
                action,
                serials,
            }) => {
                assert_eq!((store, epoch), (S, 0));
                draws.push((action, serials));
            }
            Cmd::ChunkReconcile(r) => reconciles.push(r),
            _ => {}
        }
    }
    (draws, reconciles)
}

/// Java applying every reconcile whole.
fn reconciled(e: &mut Engine, at: u64, rs: &[ChunkReconcile]) {
    for r in rs {
        send(
            e,
            at,
            Obs::ReconcileResult(ReconcileResult {
                op: r.op,
                district: r.district,
                epoch: r.epoch,
                chunk: r.chunk,
                entries: r
                    .deltas
                    .iter()
                    .map(|d| ResultEntry {
                        inventory: d.inventory,
                        good: d.good,
                        requested: d.delta,
                        applied: d.delta,
                    })
                    .collect(),
                destroyed: vec![],
                blocks_skipped: 0,
            }),
        );
    }
}

fn held_keys(e: &Engine) -> Vec<HeldKey> {
    e.stores()[&S]
        .held_toward(frog(MILL))
        .iter()
        .map(|h| h.key)
        .collect()
}

fn drawn_keys(e: &Engine) -> Vec<HeldKey> {
    e.stores()[&S]
        .drawn_toward(frog(MILL))
        .iter()
        .map(|h| h.key)
        .collect()
}

fn chain_packages(e: &Engine) -> u64 {
    e.stores()[&S].pools[1].packages
}

/// A package the frontier holds toward the away mill is drawn by the mill's replay (choice
/// SC141): asked once, booked at Java's `CHAIN_DRAWN`, its iron the replay's import. It stays in
/// the world until the replay has reached it: once the mill loads and its reconcile applied, the
/// engine has it removed, once (choice SC142).
#[test]
fn a_held_package_is_drawn_by_the_far_replay_and_removed_once_its_reconcile_applied() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7])]);
    // Another held package wakes the store again: only the new key is asked.
    send(&mut e, 210, keyed(ChainDirection::Held, K8));
    e.advance_to(Tick(215));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![8])]);
    assert_eq!(e.stats.held_draws_asked, 2);
    assert_eq!(e.sim_stock(MILL).expect("replays").get(IRON), 0);
    // Java draws 7 only (8 moved on meanwhile, say).
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    assert_eq!(drawn_keys(&e), vec![K7]);
    assert_eq!(held_keys(&e), vec![K8]);
    assert_eq!(chain_packages(&e), 2, "the drawn package is no pool's");
    assert_eq!(e.stores()[&S].counts.drawn, 1);
    // Told again: nothing changes.
    send(&mut e, 221, keyed(ChainDirection::Drawn, K7));
    assert_eq!(e.stores()[&S].counts.drawn, 1);
    e.check_books().expect("books");
    // A held record of a drawn key (Java lost its mark) asks the draw again, and stays drawn.
    send(&mut e, 222, keyed(ChainDirection::Held, K7));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7])]);
    assert_eq!(drawn_keys(&e), vec![K7]);
    // The replay imports the package's iron.
    e.advance_to(Tick(3_000));
    assert_eq!(e.sim_stock(MILL).expect("replays").get(IRON), BATCH);
    // The mill loads: no removal while its reconcile is out.
    live(&mut e, MILL, 3_000);
    e.advance_to(Tick(3_100));
    let (draws, rs) = asks(&mut e);
    assert!(draws.is_empty(), "{draws:?}");
    assert!(!rs.is_empty(), "the replay's iron goes back to the world");
    assert_eq!(drawn_keys(&e), vec![K7]);
    reconciled(&mut e, 3_100, &rs);
    e.advance_to(Tick(3_102));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Remove, vec![7])]);
    assert_eq!(e.stats.held_removes_asked, 1);
    // Java removed it: struck.
    send(&mut e, 3_200, keyed(ChainDirection::Released, K7));
    assert!(drawn_keys(&e).is_empty());
    assert_eq!(e.stores()[&S].counts.drawn_removed, 1);
    assert_eq!(held_keys(&e), vec![K8]);
    assert_eq!(chain_packages(&e), 2);
    e.check_books().expect("books");
}

/// A drawn package whose district is removed is given back (choice SC143): nothing will reach
/// the world for it, so it is held again and the world's.
#[test]
fn a_drawn_package_whose_district_is_gone_is_given_back() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    assert_eq!(chain_packages(&e), 2);
    send(&mut e, 300, Obs::DistrictRemoved { district: MILL });
    e.advance_to(Tick(302));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Return, vec![7])]);
    assert_eq!(held_keys(&e), vec![K7]);
    assert!(drawn_keys(&e).is_empty());
    assert_eq!(chain_packages(&e), 3);
    assert_eq!(e.stores()[&S].counts.drawn_back, 1);
    assert_eq!(e.stats.held_returns_asked, 1);
}

/// Java's mark that comes after the mill loaded is given back: no replay draws that frogport now
/// (choice SC141).
#[test]
fn a_draw_marked_after_its_district_loaded_is_given_back() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7])]);
    live(&mut e, MILL, 210);
    e.advance_to(Tick(215));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Return, vec![7])]);
    assert_eq!(held_keys(&e), vec![K7]);
    assert_eq!(e.stores()[&S].counts.drawn, 0);
    assert_eq!(chain_packages(&e), 3);
}

/// A drawn package that lands anyway (its key's keyed `CHAIN_IN`) is the world's again: back in
/// the pool and taken there, a duplicate of what the replay drew at worst (choice SC143).
#[test]
fn a_drawn_package_that_lands_goes_back_into_the_pool_first() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    live(&mut e, MILL, 300);
    let mut landed = chain(MILL, ChainDirection::In, 1, None);
    if let Obs::ChainPackage(p) = &mut landed {
        p.key = Some(K7);
    }
    send(&mut e, 310, landed);
    let s = &e.stores()[&S];
    assert!(drawn_keys(&e).is_empty());
    assert_eq!(
        (s.counts.held_landed, s.counts.drawn_back),
        (1, 1),
        "{:?}",
        s.counts
    );
    assert_eq!(chain_packages(&e), 2);
    e.check_books().expect("books");
}

/// A drawn package that lands at another frogport than the one it was held toward goes back into
/// the pool of the frogport it landed at, so that landing takes it and no other pool is short.
#[test]
fn a_drawn_package_that_lands_elsewhere_goes_back_where_it_landed() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    assert_eq!(chain_packages(&e), 2);
    let mut landed = chain(MINE, ChainDirection::In, 1, None);
    if let Obs::ChainPackage(p) = &mut landed {
        p.key = Some(K7);
    }
    send(&mut e, 310, landed);
    let s = &e.stores()[&S];
    assert!(drawn_keys(&e).is_empty());
    assert_eq!(
        (
            s.counts.held_landed,
            s.counts.drawn_back,
            s.counts.in_beyond
        ),
        (1, 1, 0),
        "{:?}",
        s.counts
    );
    assert_eq!(
        s.pools[0].packages, 0,
        "the landing took it where it landed"
    );
    assert_eq!(chain_packages(&e), 2, "the mill's pool is not given it");
    e.check_books().expect("books");
}

/// A drawn package's landing that a census counted already (recorded before it, taken after it)
/// takes nothing, and the census, which counts no drawn package, has the pools right: the package
/// goes into none.
#[test]
fn a_drawn_landing_a_census_counted_goes_into_no_pool() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    send(
        &mut e,
        400,
        Obs::StoreCensus(StoreCensus {
            store: S,
            packages: vec![(Some(1), 2)],
            goods: vec![(Some(1), IRON, 2 * BATCH)],
        }),
    );
    assert_eq!(chain_packages(&e), 2);
    let mut landed = chain(MINE, ChainDirection::In, 1, None);
    if let Obs::ChainPackage(p) = &mut landed {
        p.key = Some(K7);
    }
    send(&mut e, 310, landed);
    let s = &e.stores()[&S];
    assert!(drawn_keys(&e).is_empty());
    assert_eq!(
        (
            s.counts.held_landed,
            s.counts.drawn_back,
            s.counts.in_beyond
        ),
        (1, 1, 0),
        "{:?}",
        s.counts
    );
    assert_eq!(s.pools[0].packages, 0, "no pool is given it");
    assert_eq!(chain_packages(&e), 2);
    e.check_books().expect("books");
}

/// A draw Java declined (its frogport ticked then) comes back as a held record: the engine may
/// ask it again.
#[test]
fn a_declined_draw_is_asked_again() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7])]);
    send(&mut e, 230, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(235));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7])]);
    assert_eq!(e.stats.held_draws_asked, 2);
}

/// After a restart the drawn packages are as saved (IST4), and Java's held list puts the marks
/// right (choice SC143): a key Java does not name goes back to the pools, one the engine drew that
/// Java lists only held is drawn again, and one Java marked drawn whose record was lost is booked
/// drawn, its frogport's replay still on.
#[test]
fn the_held_list_puts_the_drawn_marks_right_after_a_restart() {
    let mut e = frontier();
    e.set_page_check(true);
    // Held in one tick: one ask.
    for k in [K7, K8, K9] {
        send(&mut e, 200, keyed(ChainDirection::Held, k));
    }
    e.advance_to(Tick(205));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Draw, vec![7, 8, 9])]);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    send(&mut e, 221, keyed(ChainDirection::Drawn, K9));
    assert_eq!(chain_packages(&e), 1);
    e.advance_to(Tick(230));
    e.drain_commands();
    let c = e.save_changed();
    let mut back = Engine::restore(c.put, Tunables::default()).expect("restore");
    assert_eq!(back.stores(), e.stores());
    assert_eq!(
        back.stores()[&S].drawn_toward(frog(MILL))[0].by,
        Some(MILL),
        "the drawing district is saved"
    );
    assert!(back.replays(MILL));
    send(
        &mut back,
        300,
        Obs::HeldList(HeldList {
            epoch: 0,
            serials: vec![7],
            drawn: vec![8],
        }),
    );
    back.advance_to(Tick(302));
    assert_eq!(asks(&mut back).0, vec![(DrawAction::Draw, vec![7])]);
    let s = &back.stores()[&S];
    assert_eq!(drawn_keys(&back), vec![K7, K8]);
    assert!(held_keys(&back).is_empty());
    assert_eq!(
        (s.counts.held_unlisted, s.counts.drawn_back, s.counts.drawn),
        (1, 1, 3)
    );
    assert_eq!(chain_packages(&back), 1);
    back.check_books().expect("books");
}

/// What the replay did not use of a drawn package goes back to the store and is written into the
/// loaded frogport first: the removal waits for that write's answer (choice SC142).
#[test]
fn a_drawn_package_is_removed_after_the_rest_of_it_is_written() {
    let mut e = frontier();
    send(&mut e, 200, keyed(ChainDirection::Held, K7));
    e.advance_to(Tick(205));
    asks(&mut e);
    send(&mut e, 220, keyed(ChainDirection::Drawn, K7));
    // 280 ticks at 0.05: 14 iron imported, the rest still in the pool.
    e.advance_to(Tick(500));
    live(&mut e, MILL, 500);
    let rest = owed(&e);
    assert!((40..BATCH).contains(&rest), "{rest}");
    e.advance_to(Tick(510));
    let mut w = Vec::new();
    let mut rs = Vec::new();
    for c in e.drain_commands() {
        match c {
            Cmd::WriteChain(x) => w.push(x),
            Cmd::ChunkReconcile(r) => rs.push(r),
            Cmd::HeldDraw(d) => panic!("asked before the rest is written: {d:?}"),
            _ => {}
        }
    }
    assert_eq!(iron(&w), rest, "{w:?}");
    reconciled(&mut e, 510, &rs);
    e.advance_to(Tick(600));
    assert!(asks(&mut e).0.is_empty(), "the write is not answered yet");
    for x in &w {
        written(&mut e, 600, x);
    }
    assert_eq!(owed(&e), 0);
    // At the store's next write wake.
    e.advance_to(Tick(800));
    assert_eq!(asks(&mut e).0, vec![(DrawAction::Remove, vec![7])]);
    e.check_books().expect("books");
}

/// The engine's side of a write answered while industry is off (docs/choices-journal-barrier.md
/// RP-285, P1-1). The mine replays toward the mill; the engine hands a write X on at 20,100, and
/// Java writes it at 20,101, journaling its `CHAIN_IN` with X's op, as industry goes off. The
/// switch on comes thirty periods later, past the ten after which an unanswered write is taken as
/// lost (`WRITE_ANSWER_PERIODS`, choice SC113), and runs the engine's session start, which catches
/// up the thirty periods. The link keeps the `CHAIN_IN` heard while off and books it at its own
/// tick, before that session start: the store counts X's packages, and from then on Java is handed
/// the iron the mine sent, once. The control is the link at 9bbf1569, which ignored the record
/// while off: the catch-up takes X as lost, and the writes after it hand Java X's iron a second
/// time under new ops. Then Java writes every write as it comes, both ends load, and the rest
/// is written. This stands in for a native fixture: the iron line learns no chain link, so no
/// native rig makes an engine write past ten periods (RP-285's Tests).
#[test]
fn a_write_answered_while_industry_is_off_is_taken_at_its_tick_before_the_catch_up() {
    let period = Tunables::default().replay.chain_write_ticks;
    let on = 20_100 + 30 * period;
    // (X's iron, packages counted written after the switch, X's included, iron handed to Java).
    let run = |kept: bool| {
        let mut e = pair(Tunables::default());
        away(&mut e, MINE, 100);
        e.advance_to(Tick(20_100));
        let x = writes(&mut e);
        assert_eq!(x.len(), 1, "one write a step: {x:?}");
        assert_eq!(e.stores()[&S].counts.written, 0);
        if kept {
            written(&mut e, 20_101, &x[0]);
        }
        e.session_start(Tick(on));
        let counted = e.stores()[&S].counts.written;
        let caught_up = writes(&mut e);
        for w in &caught_up {
            written(&mut e, on, w);
        }
        live(&mut e, MILL, on);
        live(&mut e, MINE, on);
        let rest = run_writing(&mut e, on, on + 40 * period, period);
        assert_eq!(owed(&e), 0, "everything owed is written");
        e.check_books().expect("books");
        (iron(&x), counted, iron(&x) + iron(&caught_up) + iron(&rest))
    };
    let (x, counted, handed) = run(true);
    let (_, lost_counted, lost_handed) = run(false);
    assert_eq!(counted, 1, "X's package is counted once, at the switch");
    assert_eq!(lost_counted, 0, "the control never hears X");
    assert_eq!(
        lost_handed - handed,
        x,
        "the control hands Java X's iron again: {lost_handed} against {handed}"
    );
}

/// `take_out` (the bridge's link, RP-285): a command decided and not drained yet leaves by its op,
/// once, and the drain no longer hands it on.
#[test]
fn a_command_not_drained_yet_is_taken_out_by_its_op() {
    let decided = || {
        let mut e = pair(Tunables::default());
        away(&mut e, MINE, 100);
        e.advance_to(Tick(20_100));
        e
    };
    let w = writes(&mut decided());
    assert_eq!(w.len(), 1, "{w:?}");
    let mut e = decided();
    let other = rtsim_kernel::journal::OpId((0x49 << 56) | 0x1234);
    assert_eq!(e.take_out(other), None);
    assert_eq!(e.take_out(w[0].op), Some(Cmd::WriteChain(w[0].clone())));
    assert_eq!(e.take_out(w[0].op), None);
    assert!(e.drain_commands().is_empty());
}
