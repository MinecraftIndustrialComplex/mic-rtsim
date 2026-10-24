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
    ChainDirection, ChainPackage, DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, StoreDecl,
    StoreFrogport, SweepEnd, Timed,
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
        pos: frog(d),
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
            pos: w.pos,
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
