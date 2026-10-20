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
