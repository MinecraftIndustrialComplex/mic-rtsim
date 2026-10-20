// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine end to end through the fake world (crates/rtsim-industry/src/fake.rs), with
//! every observation and command through the wire codec.

use rtsim_industry::cmd::Cmd;
use rtsim_industry::engine::{LedgerState, Mode};
use rtsim_industry::fake::{FDistrict, FInventory, FakeWorld, Recipe, Visit};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{InvKind, NotLearnable, Obs, ReconcileResult, Timed};
use rtsim_industry::scenario::{INGOT, IronLine, iron_line, run_iron_line};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

const ORE: Good = Good(1);
const BAR: Good = Good(2);
const MILL: DistrictId = DistrictId(1);

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// One mill: 8 ore into 8 bars every 100 ticks, ore and bars in vaults in different chunks.
fn mill(ore: i64, visits: Vec<(f64, bool)>, tick_first: bool, mode: Mode) -> FakeWorld {
    let mut w = FakeWorld::new(3, Tunables::default(), mode);
    w.machines_tick_first = tick_first;
    w.good(ORE.0, GoodInfo::item("ore", 64));
    w.good(BAR.0, GoodInfo::item("bar", 64));
    let inv = |index: u32, x: i32, blocks: u32, contents: GoodVec| FInventory {
        index,
        pos: BlockPos::new(x, 64, 0),
        kind: InvKind::ITEMS,
        slots: 20 * blocks,
        fluid_cap: 0,
        energy_cap: 0,
        contents,
        destroyed: false,
    };
    w.districts.push(FDistrict {
        id: MILL,
        name: "Mill".into(),
        dim: 0,
        inventories: vec![inv(0, 0, 10, GoodVec::from_pairs([(ORE, ore)])), inv(1, 32, 10, GoodVec::new())],
        recipes: vec![Recipe {
            name: "mill".into(),
            inputs: vec![(ORE, 0, 8)],
            outputs: vec![(BAR, 1, 8)],
            period: 100,
            target: None,
            deposit_per_batch: 0,
            power_from: None,
            machine: BlockPos::new(16, 64, 0),
        }],
        deposit: None,
        deposit_chunk: ChunkPos::default(),
        exposed: vec![],
        survey: None,
        spawn_rate: 0.0,
        loaded: false,
        epochs: Default::default(),
        held: vec![],
        sweep: 0,
        threshold_on: vec![],
        last_ran: vec![],
        broken: Default::default(),
        revision: 0,
        ever_loaded: false,
    });
    w.visits = visits
        .into_iter()
        .map(|(d, here)| Visit {
            from: day(d),
            at: if here { vec![0] } else { vec![] },
        })
        .collect();
    w.declare();
    w
}

fn bars(w: &FakeWorld) -> i64 {
    w.districts[0].inventories.iter().map(|i| i.contents.get(BAR)).sum()
}

#[test]
fn a_learned_district_runs_while_unloaded_and_reconciles_on_load() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false, Mode::Apply);
    w.run_until(day(3.1));
    assert_eq!(w.engine.stats.reconciles, 1);
    // Two days unloaded at 8 per 100 ticks: 3,840 bars by the replay, 1,920 per loaded day.
    let made = bars(&w);
    assert!((made - (3_840 + 1_920 + 192)).abs() <= 16, "bars {made}");
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

#[test]
fn unlearned_districts_freeze() {
    // Unloaded before any window could be steady (W_min is half a day).
    let mut w = mill(20_000, vec![(0.0, true), (0.2, false), (2.0, true)], false, Mode::Apply);
    w.run_until(day(1.0));
    assert!(w.engine.sim_run(MILL).is_none(), "an unlearned district must not replay");
    let l = w.engine.ledger(MILL, day(1.0)).unwrap();
    assert_eq!(l.state, LedgerState::Learning);
    w.run_until(day(2.0) + 5);
    assert_eq!(w.engine.stats.reconciles, 0);
    w.conservation().unwrap();
}

#[test]
fn stale_districts_freeze() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false, Mode::Apply);
    w.run_until(day(0.9));
    assert!(w.engine.district(MILL).unwrap().models.len() == 1, "learned by now");
    // The player edits the line: a learned district goes stale (I5).
    let edit = Obs::Edit {
        district: MILL,
        kind: rtsim_industry::obs::EditKind::Broken,
        player: (1, 2),
        pos: BlockPos::new(16, 64, 0),
    };
    w.engine.ingest(&Timed { at: w.t, obs: edit });
    w.run_until(day(2.0));
    assert!(w.engine.sim_run(MILL).is_none(), "a stale district must not replay");
    assert_eq!(w.engine.ledger(MILL, day(2.0)).unwrap().state, LedgerState::Stale);
    w.run_until(day(3.0) + 5);
    assert_eq!(w.engine.stats.reconciles, 0);
    // It relearns once loaded again, and runs on the next unload.
    w.visits.push(Visit { from: day(4.0), at: vec![] });
    w.run_until(day(4.1));
    assert!(w.engine.sim_run(MILL).is_some(), "relearned and replaying");
}

#[test]
fn not_learnable_districts_freeze() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (2.0, true)], false, Mode::Apply);
    let decl = Obs::District(rtsim_industry::obs::DistrictDecl {
        district: MILL,
        revision: 1,
        dim: 0,
        name: 1,
        not_learnable: Some(NotLearnable::MovingContraption),
        inventory_count: 2,
    });
    w.engine.ingest(&Timed { at: w.t, obs: decl });
    w.run_until(day(1.5));
    assert!(w.engine.sim_run(MILL).is_none());
    let l = w.engine.ledger(MILL, day(1.5)).unwrap();
    assert_eq!(l.state, LedgerState::NotLearnable);
    assert_eq!(l.reason.as_deref(), Some("moving drill"));
}

#[test]
fn reconcile_is_idempotent_through_op_ids() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false, Mode::Apply);
    w.run_until(day(3.0) + 10);
    assert_eq!(w.engine.stats.reconciles, 1);
    let snapshot = |w: &FakeWorld| {
        let d = w.engine.district(MILL).unwrap();
        (
            d.inventories.values().map(|i| (i.known.clone(), i.pending.clone())).collect::<Vec<_>>(),
            d.ports.clone(),
            d.owed.clone(),
        )
    };
    let before = snapshot(&w);
    let world_before = bars(&w);
    // The bridge resends a result it already sent (a journal replay after a crash): the engine
    // applies an op once.
    let report = w.engine.district(MILL).unwrap().reports.back().cloned().unwrap();
    assert_eq!(report.epoch, 1);
    let dup = Obs::ReconcileResult(ReconcileResult {
        op: rtsim_industry::cmd::IndustryOps::default().clone().allocate(),
        district: MILL,
        epoch: 1,
        chunk: ChunkPos { dim: 0, x: 2, z: 0 },
        entries: vec![rtsim_industry::obs::ResultEntry { inventory: 1, good: BAR, requested: 3_840, applied: 3_840 }],
        blocks_destroyed: 0,
        blocks_skipped: 0,
    });
    w.engine.ingest(&Timed { at: w.t, obs: dup.clone() });
    w.engine.ingest(&Timed { at: w.t, obs: dup });
    assert_eq!(w.engine.stats.duplicate_results, 2);
    assert_eq!(snapshot(&w), before);
    // And the Java side applies a resent command once: the chunk's epoch says it is done.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::ChunkLoaded { district: MILL, chunk: ChunkPos { dim: 0, x: 2, z: 0 }, applied_epoch: 0 },
    });
    let resent: Vec<Cmd> = w.engine.drain_commands();
    assert!(resent.is_empty(), "nothing is pending once applied: {resent:?}");
    w.run_until(w.t + 10);
    assert!(bars(&w) <= world_before + 8, "only real production since (one batch at most)");
    w.conservation().unwrap();
}

#[test]
fn a_shortfall_trims_the_outputs_it_funded() {
    // 3,000 ore runs out after 37,500 ticks of replay. On reload the mill ticks once on stale
    // stock, then runs its regular batch on the load tick, both before the one-tick-late
    // reconcile lands: the ore removal comes up 16 short, and 16 bars are trimmed (8 per 8 ore).
    let mut w = mill(3_000, vec![(0.0, true), (0.6, false), (4.0, true)], true, Mode::Apply);
    w.run_until(day(4.0) + 20);
    let b = &w.engine.books;
    assert_eq!(b.shortfall.get(ORE), 16, "books {b:?}");
    assert_eq!(b.trimmed.get(BAR), 16, "books {b:?}");
    let rep = w.engine.district(MILL).unwrap().reports.back().unwrap();
    assert_eq!(rep.trims.get(BAR), 16);
    w.conservation().unwrap();
}

#[test]
fn read_only_changes_nothing() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false, Mode::ReadOnly);
    w.run_until(day(2.9));
    let l = w.engine.ledger(MILL, day(2.9)).unwrap();
    assert!(l.made_since.get(BAR) > 3_000, "the ledger shows what it would have made: {l:?}");
    let before = bars(&w);
    w.run_until(day(3.0) + 10);
    let r = w.engine.district(MILL).unwrap().reports.back().unwrap().clone();
    assert!(r.read_only);
    // The vaults hold only what the mill really made while loaded: one batch since the load.
    assert_eq!(bars(&w), before + 8);
}

#[test]
fn the_iron_line_conserves_and_meets_ib() {
    let base = IronLine { seed: 11, days: 10, threats: true, control: false, mode: Mode::Apply, tick_first: true };
    let mut main = iron_line(base, Tunables::default());
    run_iron_line(&mut main, &base);
    main.conservation().unwrap();
    main.engine.check_books().unwrap();
    assert!(main.engine.stats.attacks > 0);
    let quiet = IronLine { threats: false, tick_first: false, ..base };
    let mut sim = iron_line(quiet, Tunables::default());
    run_iron_line(&mut sim, &quiet);
    let control_cfg = IronLine { control: true, ..quiet };
    let mut control = iron_line(control_cfg, Tunables::default());
    run_iron_line(&mut control, &control_cfg);
    sim.conservation().unwrap();
    control.conservation().unwrap();
    let made = |w: &FakeWorld| w.books.minted.get(INGOT) + w.engine.books.minted.get(INGOT) - w.engine.books.trimmed.get(INGOT);
    let (s, c) = (made(&sim), made(&control));
    assert!(s <= c, "the replay made more than the control: {s} > {c}");
    assert!(s as f64 >= 0.9 * c as f64, "the replay made far less: {s} vs {c}");
}

#[test]
fn catch_up_cost_is_independent_of_absence() {
    let mut costs = Vec::new();
    for days in [8.0, 80.0, 800.0] {
        let mut w = mill(i64::MAX / 1_000_000_000_000, vec![(0.0, true), (1.0, false)], false, Mode::Apply);
        w.run_until(day(1.0) + 1);
        let before = (w.engine.stats.breakpoints, w.engine.stats.cluster_settles);
        // Nothing reaches it while it is away; then one ledger read.
        w.engine.advance_to(day(days));
        let l = w.engine.ledger(MILL, day(days)).unwrap();
        assert!(l.made_since.get(BAR) > 0);
        costs.push((w.engine.stats.breakpoints - before.0, w.engine.stats.cluster_settles - before.1));
    }
    assert!(costs.windows(2).all(|p| p[0] == p[1]), "cost grew with the absence: {costs:?}");
}

#[test]
fn threat_legs_find_the_trains_they_cross() {
    let c = IronLine { seed: 2, days: 10, threats: false, control: false, mode: Mode::Apply, tick_first: true };
    let mut w = iron_line(c, Tunables::default());
    w.run_until(day(3.0));
    // Train 1 runs between Ore Coast (x 1,604) and the Smelter (x 3,202): regions 3 to 6 on z 0.
    let now = w.t;
    let met = w.engine.encounters(0, &[(4, 0), (5, 0)], Tick(now.0 - 3_000), now);
    assert!(met.contains(&rtsim_industry::ids::TrainId(1)), "met {met:?}");
    assert!(w.engine.encounters(0, &[(40, 40)], Tick(now.0 - 3_000), now).is_empty());
}

/// A district whose packager fills a postbox (I3, §8.2): while unloaded the replay writes whole
/// packages into the offline buffer, a train takes some, and the reconcile takes exactly what was
/// packed from the vault, never twice.
#[test]
fn packages_written_while_unloaded_are_taken_from_the_vault_once() {
    use rtsim_industry::cmd::Cmd;
    use rtsim_industry::engine::Engine;
    use rtsim_industry::learn::{Learned, LinkKey};
    use rtsim_industry::obs::{Direction, DistrictDecl, InventoryDecl, LoadState, PackageEvent, SweepEnd};
    let mut e = Engine::new(1, Tunables::default(), Mode::Apply, Tick(0));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    let d = MILL;
    let send = |e: &mut Engine, at: u64, obs: Obs| e.ingest(&Timed { at: Tick(at), obs });
    send(&mut e, 0, Obs::District(DistrictDecl { district: d, revision: 1, dim: 0, name: 0, not_learnable: None, inventory_count: 2 }));
    for (index, kind, slots) in [(0u32, InvKind::ITEMS, 100u32), (1, InvKind::ITEMS | InvKind::POSTBOX, 18)] {
        send(&mut e, 0, Obs::Inventory(InventoryDecl { district: d, revision: 1, index, pos: BlockPos::new(index as i32 * 20, 64, 0), kind: InvKind(kind), item_slots: slots, fluid_capacity: 0, energy_capacity: 0 }));
    }
    send(&mut e, 0, Obs::LoadState { district: d, state: LoadState::Live, ticking_chunks: 2, total_chunks: 2 });
    send(&mut e, 10, Obs::StockSample { district: d, inventory: 0, sweep: 1, goods: GoodVec::from_pairs([(BAR, 5_000)]) });
    send(&mut e, 10, Obs::StockSample { district: d, inventory: 1, sweep: 1, goods: GoodVec::new() });
    send(&mut e, 10, Obs::SweepEnd(SweepEnd { district: d, sweep: 1, records: 2, inventories: 2, live: true, creative: false, overstressed: false }));
    // A train took a package of 64 bars addressed "7" while it was loaded: one package's contents.
    send(&mut e, 20, Obs::Package(PackageEvent { district: d, inventory: 1, direction: Direction::Out, address: 7, packages: 1, op: None, goods: GoodVec::from_pairs([(BAR, 64)]) }));
    e.adopt_model(d, Learned { at: Tick(20), window_ticks: 12_000, rates: vec![], links: vec![(LinkKey::Mail { inventory: 1, address: 7, good: BAR }, -0.02)], confidence: 1.0, points: 0, longest_gap: 0, latency_ticks: 0 }, vec![]);
    send(&mut e, 100, Obs::LoadState { district: d, state: LoadState::Unloaded, ticking_chunks: 0, total_chunks: 2 });
    // A day of packing: 480 bars, 7 whole packages written.
    e.advance_to(Tick(100 + 24_000));
    let written: u32 = e.drain_commands().iter().map(|c| match c { Cmd::WritePackage(p) => { assert_eq!(p.goods.get(BAR), 64); p.count } _ => 0 }).sum();
    assert_eq!(written, 7);
    // A train fetches three of them from the offline buffer.
    send(&mut e, 24_200, Obs::Package(PackageEvent { district: d, inventory: 1, direction: Direction::In, address: 7, packages: 0, op: None, goods: GoodVec::new() }));
    send(&mut e, 24_300, Obs::Package(PackageEvent { district: d, inventory: 1, direction: Direction::Out, address: 7, packages: 3, op: None, goods: GoodVec::from_pairs([(BAR, 192)]) }));
    for x in [0, 1] {
        send(&mut e, 24_400, Obs::ChunkLoaded { district: d, chunk: ChunkPos { dim: 0, x, z: 0 }, applied_epoch: 0 });
    }
    send(&mut e, 24_400, Obs::LoadState { district: d, state: LoadState::Live, ticking_chunks: 2, total_chunks: 2 });
    let r = e.district(d).unwrap().reports.back().unwrap().clone();
    // Packed: 24,300 ticks at 0.02 is 486 bars; 7 packages (448) were written, the rest goes back.
    assert_eq!(r.net.get(BAR), -448, "{r:?}");
    let vault_delta: i64 = e.drain_commands().iter().filter_map(|c| match c { Cmd::ChunkReconcile(rc) => Some(rc.deltas.iter().filter(|x| x.inventory == 0).map(|x| x.delta).sum::<i64>()), _ => None }).sum();
    assert_eq!(vault_delta, -448, "the vault gives exactly what was packed into written packages");
}

/// Two districts on one Ender Transmission channel (I13): one exports into the shared buffer and
/// the other imports from it while both are unloaded, in one cluster; the engine pushes the
/// buffer's changes as channel deltas, and the books close.
#[test]
fn a_channel_couples_two_unloaded_districts() {
    use rtsim_industry::cmd::Cmd;
    use rtsim_industry::engine::Engine;
    use rtsim_industry::ids::ChannelId;
    use rtsim_industry::learn::{Learned, LinkKey};
    use rtsim_industry::obs::{DistrictDecl, InventoryDecl, LoadState, SweepEnd};
    let mut e = Engine::new(1, Tunables::default(), Mode::Apply, Tick(0));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    let ch = ChannelId(42);
    let send = |e: &mut Engine, at: u64, obs: Obs| e.ingest(&Timed { at: Tick(at), obs });
    send(&mut e, 0, Obs::ChannelSample { channel: ch, capacity: 10_000, goods: GoodVec::new() });
    for (d, stock, rate) in [(DistrictId(1), 3_000i64, -0.04f64), (DistrictId(2), 0, 0.03)] {
        send(&mut e, 0, Obs::District(DistrictDecl { district: d, revision: 1, dim: 0, name: 0, not_learnable: None, inventory_count: 1 }));
        send(&mut e, 0, Obs::Inventory(InventoryDecl { district: d, revision: 1, index: 0, pos: BlockPos::new(d.0 as i32 * 1_000, 64, 0), kind: InvKind(InvKind::ITEMS), item_slots: 200, fluid_capacity: 0, energy_capacity: 0 }));
        send(&mut e, 0, Obs::LoadState { district: d, state: LoadState::Live, ticking_chunks: 1, total_chunks: 1 });
        send(&mut e, 10, Obs::StockSample { district: d, inventory: 0, sweep: 1, goods: GoodVec::from_pairs([(BAR, stock)]) });
        send(&mut e, 10, Obs::SweepEnd(SweepEnd { district: d, sweep: 1, records: 1, inventories: 1, live: true, creative: false, overstressed: false }));
        e.adopt_model(d, Learned { at: Tick(10), window_ticks: 12_000, rates: vec![], links: vec![(LinkKey::Channel(ch, BAR), rate)], confidence: 1.0, points: 0, longest_gap: 0, latency_ticks: 0 }, vec![]);
    }
    for d in [DistrictId(1), DistrictId(2)] {
        send(&mut e, 100, Obs::LoadState { district: d, state: LoadState::Unloaded, ticking_chunks: 0, total_chunks: 1 });
    }
    assert_eq!(e.cluster_count(), 1, "a shared channel makes one cluster");
    e.advance_to(Tick(100 + 24_000));
    let a = e.sim_stock(DistrictId(1)).unwrap().get(BAR);
    let b = e.sim_stock(DistrictId(2)).unwrap().get(BAR);
    // 3,000 bars leave A at 0.04 a tick (empty after 75,000 ticks); B takes 0.03 a tick.
    assert!(a < 3_000 && b > 0, "a {a}, b {b}");
    e.check_books().unwrap();
    // The exporter's breakpoint and later events push the buffer's change by command.
    e.advance_to(Tick(100 + 80_000));
    let pushed: i64 = e.drain_commands().iter().map(|c| match c { Cmd::ChannelDelta(d) => d.goods.get(BAR), _ => 0 }).sum();
    let a2 = e.sim_stock(DistrictId(1)).unwrap().get(BAR);
    let b2 = e.sim_stock(DistrictId(2)).unwrap().get(BAR);
    assert!(pushed > 0, "the exporter's changes reach the world: pushed {pushed}");
    let c2 = e.channel_level(ch, BAR);
    assert_eq!(a2 + b2 + c2, 3_000, "bars are only moved: a {a2}, b {b2}, channel {c2}");
    e.check_books().unwrap();
}

/// A chunk-loaded district with nobody near is attacked abstractly and the damage lands at once
/// (C18, INDUSTRY.md §13.1), not at some later load.
#[test]
fn a_loaded_unwatched_district_is_raided_at_once() {
    use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
    let mut w = mill(20_000, vec![(0.0, true)], false, Mode::Apply);
    w.run_until(day(0.1));
    w.engine.ingest(&Timed { at: w.t, obs: Obs::Watched { district: MILL, watched: false } });
    let ore = |w: &FakeWorld| w.districts[0].inventories[0].contents.get(ORE);
    let before = ore(&w);
    w.engine.threat(&Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        at: w.t,
    });
    // Three bandits carry a stack each, split over the goods in proportion to their caps.
    assert_eq!(w.engine.books.stolen.total(), 3 * 64);
    let stolen = w.engine.books.stolen.get(ORE);
    w.run_until(w.t + 3);
    // The mill used at most one batch in those ticks; the theft is in the vault already.
    let after = ore(&w);
    assert!(before - after >= stolen && before - after <= stolen + 8, "ore {before} to {after}");
    w.conservation().unwrap();
    // Watched again, the next raid is a real fight, not the engine's.
    w.engine.ingest(&Timed { at: w.t, obs: Obs::Watched { district: MILL, watched: true } });
    w.engine.threat(&Threat {
        id: 10,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        at: w.t + 30_000,
    });
    assert_eq!(w.engine.books.stolen.get(ORE), stolen);
}

/// The ledger says what happens next and when (I18): "ore runs out in H h M min".
#[test]
fn the_ledger_forecasts_the_next_breakpoint() {
    use rtsim_industry::cluster::{Forecast, PoolKey};
    // 3,000 ore at 8 per 100 ticks lasts 37,500 ticks of replay.
    let mut w = mill(3_000, vec![(0.0, true), (0.6, false)], false, Mode::Apply);
    w.run_until(day(0.6) + 10);
    let l = w.engine.ledger(MILL, w.t).unwrap();
    assert_eq!(l.next, Some(Forecast::InputEmpty(PoolKey::Stock(MILL, ORE))), "{l:?}");
    let at = l.next_breakpoint.unwrap();
    let left = w.engine.district(MILL).unwrap().sim.as_ref().unwrap().base.get(ORE);
    let expect = Tick(day(0.6).0 + 1 + (left as u64) * 100 / 8);
    assert!(at.0.abs_diff(expect.0) <= 100, "forecast {at:?}, expected about {expect:?}");
    // And the Display Source line carries it as kind 1 (input empties) of ore.
    w.engine.ingest(&Timed { at: w.t, obs: Obs::LedgerRequest { district: MILL } });
    let line = w.engine.drain_commands().into_iter().find_map(|c| match c { Cmd::Ledger(l) => Some(l), _ => None }).unwrap();
    assert_eq!((line.next_kind, line.next_good), (1, Some(ORE)));
}
