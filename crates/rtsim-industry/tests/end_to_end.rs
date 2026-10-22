// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine end to end through the fake world (crates/rtsim-industry/src/fake.rs), with
//! every observation and command through the wire codec.

use rtsim_industry::cmd::Cmd;
use rtsim_industry::engine::DistrictState;
use rtsim_industry::fake::{FDistrict, FInventory, FakeWorld, Recipe, Visit};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, StationId};
use rtsim_industry::obs::{InvKind, NotLearnable, Obs, ReconcileResult, Timed};
use rtsim_industry::scenario::{
    INGOT, IronLine, ORE_COAST, iron_line, iron_line_world, run_iron_line,
};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

const ORE: Good = Good(1);
const BAR: Good = Good(2);
const MILL: DistrictId = DistrictId(1);

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// One mill: 8 ore into 8 bars every 100 ticks, ore and bars in vaults in different chunks.
fn mill(ore: i64, visits: Vec<(f64, bool)>, tick_first: bool) -> FakeWorld {
    let mut w = FakeWorld::new(3, Tunables::default());
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
        inventories: vec![
            inv(0, 0, 10, GoodVec::from_pairs([(ORE, ore)])),
            inv(1, 32, 10, GoodVec::new()),
        ],
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
    w.districts[0]
        .inventories
        .iter()
        .map(|i| i.contents.get(BAR))
        .sum()
}

#[test]
fn a_learned_district_runs_while_unloaded_and_reconciles_on_load() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false);
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
    let mut w = mill(20_000, vec![(0.0, true), (0.2, false), (2.0, true)], false);
    w.run_until(day(1.0));
    assert!(
        w.engine.sim_run(MILL).is_none(),
        "an unlearned district must not replay"
    );
    let l = w.engine.inspect(MILL, day(1.0)).unwrap();
    assert_eq!(l.state, DistrictState::Learning);
    w.run_until(day(2.0) + 5);
    assert_eq!(w.engine.stats.reconciles, 0);
    w.conservation().unwrap();
}

#[test]
fn stale_districts_freeze() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false);
    w.run_until(day(0.9));
    assert!(
        w.engine.district(MILL).unwrap().models.len() == 1,
        "learned by now"
    );
    // The player edits the line: a learned district goes stale (I5).
    let edit = Obs::Edit {
        district: MILL,
        kind: rtsim_industry::obs::EditKind::Broken,
        player: (1, 2),
        pos: BlockPos::new(16, 64, 0),
    };
    w.engine.ingest(&Timed { at: w.t, obs: edit });
    w.run_until(day(2.0));
    assert!(
        w.engine.sim_run(MILL).is_none(),
        "a stale district must not replay"
    );
    assert_eq!(
        w.engine.inspect(MILL, day(2.0)).unwrap().state,
        DistrictState::Stale
    );
    w.run_until(day(3.0) + 5);
    assert_eq!(w.engine.stats.reconciles, 0);
    // It relearns once loaded again, and runs on the next unload.
    w.visits.push(Visit {
        from: day(4.0),
        at: vec![],
    });
    w.run_until(day(4.1));
    assert!(w.engine.sim_run(MILL).is_some(), "relearned and replaying");
}

#[test]
fn not_learnable_districts_freeze() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (2.0, true)], false);
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
    let l = w.engine.inspect(MILL, day(1.5)).unwrap();
    assert_eq!(l.state, DistrictState::NotLearnable);
    assert_eq!(l.reason.as_deref(), Some("moving drill"));
}

#[test]
fn reconcile_is_idempotent_through_op_ids() {
    let mut w = mill(20_000, vec![(0.0, true), (1.0, false), (3.0, true)], false);
    w.run_until(day(3.0) + 10);
    assert_eq!(w.engine.stats.reconciles, 1);
    let snapshot = |w: &FakeWorld| {
        let d = w.engine.district(MILL).unwrap();
        (
            d.inventories
                .values()
                .map(|i| (i.known.clone(), i.pending.clone()))
                .collect::<Vec<_>>(),
            d.ports.clone(),
            d.owed.clone(),
        )
    };
    let before = snapshot(&w);
    let world_before = bars(&w);
    // The bridge resends a result it already sent (a journal replay after a crash): the engine
    // applies an op once.
    let report = w
        .engine
        .district(MILL)
        .unwrap()
        .reports
        .back()
        .cloned()
        .unwrap();
    assert_eq!(report.epoch, 1);
    // The op the world applied for epoch 1 in the bar vault's chunk.
    let chunk = ChunkPos { dim: 0, x: 2, z: 0 };
    let op = w
        .reconciled
        .iter()
        .find(|r| r.1 == MILL && r.2 == 1 && r.3 == chunk)
        .expect("epoch 1 applied in the bar vault's chunk")
        .0;
    let dup = Obs::ReconcileResult(ReconcileResult {
        op,
        district: MILL,
        epoch: 1,
        chunk: ChunkPos { dim: 0, x: 2, z: 0 },
        entries: vec![rtsim_industry::obs::ResultEntry {
            inventory: 1,
            good: BAR,
            requested: 3_840,
            applied: 3_840,
        }],
        destroyed: vec![],
        blocks_skipped: 0,
    });
    w.engine.ingest(&Timed {
        at: w.t,
        obs: dup.clone(),
    });
    w.engine.ingest(&Timed { at: w.t, obs: dup });
    assert_eq!(w.engine.stats.duplicate_results, 2);
    assert_eq!(snapshot(&w), before);
    // And the Java side applies a resent command once: the chunk's epoch says it is done.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::ChunkLoaded {
            district: MILL,
            chunk: ChunkPos { dim: 0, x: 2, z: 0 },
            applied_epoch: 0,
        },
    });
    let resent: Vec<Cmd> = w.engine.drain_commands();
    assert!(
        resent.is_empty(),
        "nothing is pending once applied: {resent:?}"
    );
    w.run_until(w.t + 10);
    assert!(
        bars(&w) <= world_before + 8,
        "only real production since (one batch at most)"
    );
    w.conservation().unwrap();
}

#[test]
fn a_shortfall_trims_the_outputs_it_funded() {
    // 3,000 ore runs out after 37,500 ticks of replay. On reload the mill ticks once on stale
    // stock, then runs its regular batch on the load tick, both before the one-tick-late
    // reconcile lands: the ore removal comes up 16 short, and 16 bars are trimmed (8 per 8 ore).
    let mut w = mill(3_000, vec![(0.0, true), (0.6, false), (4.0, true)], true);
    w.run_until(day(4.0) + 20);
    let b = &w.engine.books;
    assert_eq!(b.shortfall.get(ORE), 16, "books {b:?}");
    assert_eq!(b.trimmed.get(BAR), 16, "books {b:?}");
    let rep = w.engine.district(MILL).unwrap().reports.back().unwrap();
    assert_eq!(rep.trims.get(BAR), 16);
    // Both chunks' commands were sent at once, so the trim is a new epoch of removals, never an
    // edit of a command Java already applied (review finding 3): nothing is left pending.
    assert!(
        w.engine
            .district(MILL)
            .unwrap()
            .inventories
            .values()
            .all(|i| i.pending.is_empty())
    );
    assert!(w.engine.district(MILL).unwrap().epoch >= 2);
    w.conservation().unwrap();
}

/// I-B (INDUSTRY.md §16): the iron line, simulated, makes at most what a chunk-loaded control
/// makes, and not much less. Measured with the protocol as built: machines may tick on stale
/// stock after a load, and the Java side sends IND_APPROACH, so reconciles go out ahead of
/// arrival (Ben's answer to the stale-tick race). Before it, the same world leaked +40 ingots
/// (0.4%) above the control through the power plant's boiler.
#[test]
fn the_iron_line_conserves_and_meets_ib() {
    let base = IronLine {
        seed: 11,
        days: 10,
        threats: true,
        control: false,
        tick_first: true,
        approach: true,
    };
    let mut main = iron_line(base, Tunables::default());
    run_iron_line(&mut main, &base);
    main.conservation().unwrap();
    main.engine.check_books().unwrap();
    assert!(main.engine.stats.attacks > 0);
    assert!(
        main.engine.stats.reconciles_ahead > 0,
        "the approach signal sent reconciles ahead"
    );
    let quiet = IronLine {
        threats: false,
        ..base
    };
    let made = |w: &FakeWorld| {
        w.books.minted.get(INGOT) + w.engine.books.minted.get(INGOT)
            - w.engine.books.trimmed.get(INGOT)
    };
    let run = |cfg: IronLine| {
        let mut w = iron_line(cfg, Tunables::default());
        run_iron_line(&mut w, &cfg);
        w.conservation().unwrap();
        w.engine.check_books().unwrap();
        made(&w)
    };
    let s = run(quiet);
    let c = run(IronLine {
        control: true,
        ..quiet
    });
    let without = run(IronLine {
        approach: false,
        ..quiet
    });
    let ideal = run(IronLine {
        tick_first: false,
        approach: false,
        ..quiet
    });
    eprintln!(
        "I-B ingots: control {c}, simulated {s} ({:+}, {:.4}), without approach {without} ({:+}), ideal {ideal} ({:+})",
        s - c,
        s as f64 / c as f64,
        without - c,
        ideal - c
    );
    assert!(s <= c, "the replay made more than the control: {s} > {c}");
    assert!(
        s as f64 >= 0.9 * c as f64,
        "the replay made far less: {s} vs {c}"
    );
}

/// SG33 to SG35 (the drained factories of stress-worlds results §10.6): a simulated stop books on
/// its own station's ports, never on the district's stock. The Ore Coast gains a factory's input
/// chest of ore that never goes near the station. The line runs and learns, the player leaves,
/// and the ore train slows, so more is made between calls than any real call moved (not so much
/// that the vault fills: the reconcile's plan then puts the drill's overflow in the chest, which
/// counts as a receiver of ore from its first sample). Every simulated load is at most the
/// learned amount, and after the reloads the chest holds what it held. The old exchange took a
/// whole carriage from the district's stock, chest included.
#[test]
fn simulated_stops_book_only_on_their_ports() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let mut w = iron_line_world(cfg, Tunables::default());
    w.districts[ORE_COAST].inventories.push(FInventory {
        index: 1,
        pos: BlockPos::new(1_600, 64, 8),
        kind: InvKind::ITEMS,
        slots: 27,
        fluid_cap: 0,
        energy_cap: 0,
        contents: GoodVec::from_pairs([(ORE, 500)]),
        destroyed: false,
    });
    w.declare();
    let cargo =
        |w: &FakeWorld| -> i64 { w.trains[0].carriages.iter().map(|c| c.cargo.get(ORE)).sum() };
    let cap_now = |w: &FakeWorld| {
        w.engine.district(DistrictId(1)).unwrap().exchange[&StationId(1)].load_cap(ORE)
    };
    // The player is away from day 1.5 to day 8.5; the loads in that span are simulated.
    let mut simulated = Vec::new();
    while w.t < day(8.0) {
        if w.t == day(1.5) {
            w.trains[0].travel = 1_400;
        }
        let before = cargo(&w);
        let loaded = w.districts[ORE_COAST].loaded;
        w.tick();
        let got = cargo(&w) - before;
        if got > 0 && !loaded && !w.districts[ORE_COAST].loaded {
            simulated.push(got);
        }
    }
    let cap = cap_now(&w);
    assert!(
        cap > 0 && cap < 27 * 64,
        "learned from real calls, less than a carriage: {cap}"
    );
    assert!(simulated.len() >= 5, "simulated loads {simulated:?}");
    assert!(
        simulated.iter().all(|&n| n <= cap),
        "simulated loads {simulated:?} over the learned {cap}"
    );
    assert!(
        simulated.contains(&cap),
        "the slowed train finds more than a real call moved: {simulated:?}"
    );
    // The player comes back (the backlog's catch-up call is real), leaves, and all reloads.
    w.run_until(Tick(cfg.days * TICKS_PER_DAY) + 2_000);
    assert!(
        cap_now(&w) < 27 * 64,
        "one catch-up call does not lift the cap: {}",
        cap_now(&w)
    );
    let chest = w.districts[ORE_COAST]
        .inventories
        .iter()
        .find(|i| i.index == 1)
        .unwrap();
    assert_eq!(
        chest.contents.get(ORE),
        500,
        "the factory's chest is untouched"
    );
    let rule = &w.engine.district(DistrictId(1)).unwrap().exchange[&StationId(1)];
    assert_eq!(rule.ports, vec![0], "the station's port is the vault");
    assert_eq!(w.engine.stats.exchanges_portless, 0);
    assert!(w.engine.stats.arrivals_approach > 0);
    assert_eq!(w.engine.stats.arrivals_late, 0);
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

/// SG33: a station whose ports no loaded call has reported moves nothing while simulated; it
/// never falls back to the district's stock.
#[test]
fn a_station_without_known_ports_moves_nothing() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let mut w = iron_line_world(cfg, Tunables::default());
    w.station_ports = false;
    w.declare();
    run_iron_line(&mut w, &cfg);
    assert_eq!(w.engine.stats.exchanges, 0);
    assert!(w.engine.stats.exchanges_portless > 0);
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

#[test]
fn catch_up_cost_is_independent_of_absence() {
    let mut costs = Vec::new();
    for days in [8.0, 80.0, 800.0] {
        let mut w = mill(
            i64::MAX / 1_000_000_000_000,
            vec![(0.0, true), (1.0, false)],
            false,
        );
        w.run_until(day(1.0) + 1);
        let before = (w.engine.stats.breakpoints, w.engine.stats.cluster_settles);
        // Nothing reaches it while it is away; then one ledger read.
        w.engine.advance_to(day(days));
        let l = w.engine.inspect(MILL, day(days)).unwrap();
        assert!(l.made_since.get(BAR) > 0);
        costs.push((
            w.engine.stats.breakpoints - before.0,
            w.engine.stats.cluster_settles - before.1,
        ));
    }
    assert!(
        costs.windows(2).all(|p| p[0] == p[1]),
        "cost grew with the absence: {costs:?}"
    );
}

#[test]
fn threat_legs_find_the_trains_they_cross() {
    let c = IronLine {
        seed: 2,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let mut w = iron_line(c, Tunables::default());
    w.run_until(day(3.0));
    // Train 1 runs between Ore Coast (x 1,604) and the Smelter (x 3,202): regions 3 to 6 on z 0.
    let now = w.t;
    let met = w
        .engine
        .encounters(0, &[(4, 0), (5, 0)], Tick(now.0 - 3_000), now);
    assert!(
        met.contains(&rtsim_industry::ids::TrainId(1)),
        "met {met:?}"
    );
    assert!(
        w.engine
            .encounters(0, &[(40, 40)], Tick(now.0 - 3_000), now)
            .is_empty()
    );
}

/// A district whose packager fills a postbox (I3, §8.2): while unloaded the replay writes whole
/// packages into the offline buffer, a train takes some, and the reconcile takes exactly what was
/// packed from the vault, never twice.
#[test]
fn packages_written_while_unloaded_are_taken_from_the_vault_once() {
    use rtsim_industry::cmd::Cmd;
    use rtsim_industry::engine::Engine;
    use rtsim_industry::learn::{Learned, LinkKey};
    use rtsim_industry::obs::{
        Direction, DistrictDecl, InventoryDecl, LoadState, PackageEvent, SweepEnd,
    };
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    let d = MILL;
    let send = |e: &mut Engine, at: u64, obs: Obs| e.ingest(&Timed { at: Tick(at), obs });
    send(
        &mut e,
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
    for (index, kind, slots) in [
        (0u32, InvKind::ITEMS, 100u32),
        (1, InvKind::ITEMS | InvKind::POSTBOX, 18),
    ] {
        send(
            &mut e,
            0,
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 1,
                index,
                pos: BlockPos::new(index as i32 * 20, 64, 0),
                kind: InvKind(kind),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    send(
        &mut e,
        0,
        Obs::LoadState {
            district: d,
            state: LoadState::Live,
            ticking_chunks: 2,
            total_chunks: 2,
        },
    );
    send(
        &mut e,
        10,
        Obs::StockSample {
            district: d,
            inventory: 0,
            sweep: 1,
            goods: GoodVec::from_pairs([(BAR, 5_000)]),
        },
    );
    send(
        &mut e,
        10,
        Obs::StockSample {
            district: d,
            inventory: 1,
            sweep: 1,
            goods: GoodVec::new(),
        },
    );
    send(
        &mut e,
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
    // A train took a package of 64 bars addressed "7" while it was loaded: one package's contents.
    send(
        &mut e,
        20,
        Obs::Package(PackageEvent {
            district: d,
            inventory: 1,
            direction: Direction::Out,
            address: 7,
            packages: 1,
            op: None,
            goods: GoodVec::from_pairs([(BAR, 64)]),
        }),
    );
    e.adopt_model(
        d,
        Learned {
            at: Tick(20),
            window_ticks: 12_000,
            rates: vec![],
            links: vec![(
                LinkKey::Mail {
                    inventory: 1,
                    address: 7,
                    good: BAR,
                },
                -0.02,
            )],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
    send(
        &mut e,
        100,
        Obs::LoadState {
            district: d,
            state: LoadState::Unloaded,
            ticking_chunks: 0,
            total_chunks: 2,
        },
    );
    // A day of packing: 480 bars, 7 whole packages written.
    e.advance_to(Tick(100 + 24_000));
    let written: u32 = e
        .drain_commands()
        .iter()
        .map(|c| match c {
            Cmd::WritePackage(p) => {
                assert_eq!(p.goods.get(BAR), 64);
                assert_eq!(p.address, 7);
                p.count
            }
            _ => 0,
        })
        .sum();
    assert_eq!(written, 7);
    // A train fetches three of them from the offline buffer.
    send(
        &mut e,
        24_200,
        Obs::Package(PackageEvent {
            district: d,
            inventory: 1,
            direction: Direction::In,
            address: 7,
            packages: 0,
            op: None,
            goods: GoodVec::new(),
        }),
    );
    send(
        &mut e,
        24_300,
        Obs::Package(PackageEvent {
            district: d,
            inventory: 1,
            direction: Direction::Out,
            address: 7,
            packages: 3,
            op: None,
            goods: GoodVec::from_pairs([(BAR, 192)]),
        }),
    );
    for x in [0, 1] {
        send(
            &mut e,
            24_400,
            Obs::ChunkLoaded {
                district: d,
                chunk: ChunkPos { dim: 0, x, z: 0 },
                applied_epoch: 0,
            },
        );
    }
    send(
        &mut e,
        24_400,
        Obs::LoadState {
            district: d,
            state: LoadState::Live,
            ticking_chunks: 2,
            total_chunks: 2,
        },
    );
    let r = e.district(d).unwrap().reports.back().unwrap().clone();
    // Packed: 24,300 ticks at 0.02 is 486 bars; 7 packages (448) were written, the rest goes back.
    assert_eq!(r.net.get(BAR), -448, "{r:?}");
    let vault_delta: i64 = e
        .drain_commands()
        .iter()
        .filter_map(|c| match c {
            Cmd::ChunkReconcile(rc) => Some(
                rc.deltas
                    .iter()
                    .filter(|x| x.inventory == 0)
                    .map(|x| x.delta)
                    .sum::<i64>(),
            ),
            _ => None,
        })
        .sum();
    assert_eq!(
        vault_delta, -448,
        "the vault gives exactly what was packed into written packages"
    );
}

/// A chunk-loaded district with nobody near is attacked abstractly and the damage lands at once
/// (C18, INDUSTRY.md §13.1), not at some later load.
#[test]
fn a_loaded_unwatched_district_is_raided_at_once() {
    use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
    let mut w = mill(20_000, vec![(0.0, true)], false);
    w.run_until(day(0.1));
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::Watched {
            district: MILL,
            watched: false,
        },
    });
    let ore = |w: &FakeWorld| w.districts[0].inventories[0].contents.get(ORE);
    let before = ore(&w);
    w.engine.threat(&Threat {
        id: 9,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: w.t,
    });
    // Three bandits carry a stack each, split over the goods in proportion to their caps.
    assert_eq!(w.engine.books.stolen.total(), 3 * 64);
    let stolen = w.engine.books.stolen.get(ORE);
    w.run_until(w.t + 3);
    // The mill used at most one batch in those ticks; the theft is in the vault already.
    let after = ore(&w);
    assert!(
        before - after >= stolen && before - after <= stolen + 8,
        "ore {before} to {after}"
    );
    w.conservation().unwrap();
    // Watched again, the next raid is a real fight, not the engine's.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::Watched {
            district: MILL,
            watched: true,
        },
    });
    w.engine.threat(&Threat {
        id: 10,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: w.t + 30_000,
    });
    assert_eq!(w.engine.books.stolen.get(ORE), stolen);
}

/// A district learned in one season freezes at the next season's boundary while unloaded, unless
/// it has a model for that season (I12).
#[test]
fn a_season_change_freezes_a_district_without_that_seasons_model() {
    use rtsim_industry::obs::Conditions;
    let mut w = mill(
        1_000_000,
        vec![(0.0, true), (0.7, false), (3.0, true)],
        false,
    );
    // Seasons of 40,000 ticks, starting at 0: season 0 until tick 40,000, then season 1.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::Conditions(Conditions {
            season: 0,
            season_count: 4,
            season_length: 40_000,
            season_origin: Tick(0),
        }),
    });
    w.run_until(day(0.7) + 10);
    assert!(w.engine.sim_run(MILL).is_some(), "replaying in season 0");
    let before = w.engine.sim_stock(MILL).unwrap().get(BAR);
    w.run_until(Tick(40_000 + 10));
    assert!(
        w.engine.sim_run(MILL).is_none(),
        "frozen at the boundary: no season 1 model"
    );
    assert_eq!(
        w.engine.inspect(MILL, w.t).unwrap().state,
        DistrictState::OutOfSeason
    );
    // What it made in season 0 is kept and reconciled at the next load.
    w.run_until(day(3.0) + 10);
    let r = w
        .engine
        .district(MILL)
        .unwrap()
        .reports
        .back()
        .cloned()
        .unwrap();
    assert!(r.net.get(BAR) >= before, "season 0's bars reconcile: {r:?}");
    w.conservation().unwrap();
}

/// The archive records the iron line (VIEWER.md §10.1): each district found at its centre when
/// its chunks arrive, learned, edited and relearned, reconciled, raided, and each train line seen.
#[test]
fn the_iron_line_is_archived() {
    use rtsim_archive::kinds::{self, Kind};
    use rtsim_archive::record::{kinds_on, rows_on};
    use rtsim_archive::{Row, Severity};
    use rtsim_industry::scenario::{IronLine, iron_line_world, run_iron_line};
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: true,
        control: false,
        tick_first: true,
        approach: true,
    };
    let mut w = iron_line_world(cfg, Tunables::default());
    w.engine.tape.set_on(true);
    w.declare();
    run_iron_line(&mut w, &cfg);
    let got = kinds_on(&w.engine.tape);
    for k in &got {
        assert_eq!(k.group(), "industry", "{}", k.name());
        assert_eq!(kinds::by_name(k.name()), Some(*k));
    }
    let main: [Kind; 6] = [
        kinds::INDUSTRY_DETECTED,
        kinds::INDUSTRY_LEARNED,
        kinds::INDUSTRY_STALE,
        kinds::INDUSTRY_RECONCILE,
        kinds::INDUSTRY_ATTACK,
        kinds::INDUSTRY_LINE,
    ];
    for k in main {
        assert!(got.contains(&k), "no {} in {got:?}", k.name());
    }
    let rows = rows_on(&w.engine.tape);
    for (k, dim, r) in &rows {
        assert_eq!(*dim, 0, "{}: the iron line is in the overworld", k.name());
        let Row::Event(e) = r else {
            panic!("{} is a leg", k.name())
        };
        assert_ne!(e.subject.value, 0, "{} has no subject", k.name());
        assert!(e.x != 0 || e.z != 0, "{} at the origin", k.name());
    }
    let of = |kind: Kind| {
        rows.iter()
            .filter(move |(k, _, _)| *k == kind)
            .map(|(_, _, r)| match r {
                Row::Event(e) => *e,
                Row::Leg(_) => unreachable!(),
            })
            .collect::<Vec<_>>()
    };
    // Four districts found once each, at the centre of their chunks: the smelter's inventories
    // are in chunks 200 to 208 on x (blocks 3,200 to 3,215 and 3,208 to 3,223), z 0 and 8.
    let found = of(kinds::INDUSTRY_DETECTED);
    let mut ids: Vec<u64> = found.iter().map(|e| e.subject.value).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2, 3, 4], "found {found:?}");
    for e in &found {
        assert_eq!(e.severity, Severity::Minor);
        assert!(e.magnitude > 0, "its chunks are counted: {e:?}");
    }
    let smelter = found.iter().find(|e| e.subject.value == 2).unwrap();
    assert!(
        (3_200..3_240).contains(&smelter.x) && (0..24).contains(&smelter.z),
        "{smelter:?}"
    );
    // Learned with its confidence in thousandths; the coast was edited and went stale once.
    for e in of(kinds::INDUSTRY_LEARNED) {
        assert!((1..=1_000).contains(&e.magnitude), "{e:?}");
    }
    let stale = of(kinds::INDUSTRY_STALE);
    assert!(stale.iter().any(|e| e.subject.value == 1), "{stale:?}");
    // Reconcile reports: routine, with the items they moved.
    let reconciles = of(kinds::INDUSTRY_RECONCILE);
    assert!(
        reconciles
            .iter()
            .all(|e| e.severity == Severity::Routine && e.object.value == 0)
    );
    assert!(reconciles.iter().any(|e| e.magnitude > 0));
    // Attacks: a landed one is major with its payload, and the train's is at a real place.
    let attacks = of(kinds::INDUSTRY_ATTACK);
    let landed = attacks
        .iter()
        .find(|e| e.severity == Severity::Major)
        .expect("a landed attack");
    assert!(landed.payload, "{landed:?}");
    assert!(
        attacks
            .iter()
            .any(|e| e.severity == Severity::Major && e.magnitude > 0),
        "something was stolen: {attacks:?}"
    );
    assert!(attacks.iter().all(|e| e.object.value == 0));
    // Train 1 runs along z 0 between its stations (x 1,604 and 3,202); its ambushes are there, and
    // a district's rows are at the district's centre (z 8).
    let ambush = attacks
        .iter()
        .find(|e| e.subject.value == 1 && e.z == 0 && e.severity == Severity::Major)
        .expect("train 1 was ambushed");
    assert!((1_604..=3_202).contains(&ambush.x), "{ambush:?}");
    assert!(ambush.magnitude > 0 && ambush.payload, "{ambush:?}");
    assert!(
        attacks
            .iter()
            .filter(|e| e.z != 0)
            .all(|e| e.z == 8 && (e.x == 1_600 || e.x == 3_200)),
        "{attacks:?}"
    );
    // Lines: from a station to another, at the station it arrived at.
    let lines = of(kinds::INDUSTRY_LINE);
    let line = lines
        .iter()
        .find(|e| e.subject.value == 1 && e.object.value == 2)
        .expect("the ore line");
    assert_eq!((line.x, line.z), (3_202, 0), "at the smelter's station");
    assert_eq!(line.severity, Severity::Minor);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert!(lines.iter().all(|e| e.subject.value != e.object.value));
    // The tape is not state: a run with it off reaches the same state.
    let mut plain = iron_line_world(cfg, Tunables::default());
    plain.declare();
    run_iron_line(&mut plain, &cfg);
    assert_eq!(plain.engine.stats.attacks, w.engine.stats.attacks);
    assert_eq!(plain.engine.stats.reconciles, w.engine.stats.reconciles);
    assert!(plain.engine.tape.is_empty());
}

/// What the archive records of a district's phases, one run each (VIEWER.md §10.1): a raid that
/// breaks a machine, a learned district edited, one whose membership changes, and one declared
/// not learnable before it has any chunks.
#[test]
fn phase_changes_are_archived() {
    use rtsim_archive::kinds::{self, Kind};
    use rtsim_archive::record::rows_on;
    use rtsim_archive::{Event, Row, Severity};
    use rtsim_industry::fake::{exposed, survey};
    use rtsim_industry::obs::{BlockFlags, DistrictDecl, EditKind};
    use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
    let rows = |w: &FakeWorld| -> Vec<(Kind, Event)> {
        rows_on(&w.engine.tape)
            .into_iter()
            .map(|(k, _, r)| match r {
                Row::Event(e) => (k, e),
                Row::Leg(_) => panic!("{} is a leg", k.name()),
            })
            .collect()
    };
    let of = |w: &FakeWorld, kind: Kind| -> Vec<Event> {
        rows(w)
            .into_iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, e)| e)
            .collect()
    };
    let decl = |revision: u32, not_learnable| {
        Obs::District(rtsim_industry::obs::DistrictDecl {
            district: MILL,
            revision,
            dim: 0,
            name: 1,
            not_learnable,
            inventory_count: 2,
        })
    };
    // The mill's chunks are x 0, 1 and 2 on z 0: its centre is the middle of chunk 1.
    let centre = (24, 8);

    // A raid on a learned mill with one machine block exposed: through open gates (every gap in
    // the perimeter) the bandits live to break it; through a perimeter of a hundred columns and
    // one gap none survive.
    let raided = |gaps: u32| {
        let mut w = mill(20_000, vec![(0.0, true)], false);
        w.districts[0].exposed = vec![exposed(
            BlockPos::new(16, 64, 0),
            11,
            BlockFlags::MACHINE,
            None,
        )];
        w.districts[0].survey = Some(survey(MILL, 0, gaps, 100, 0, 1, [0.0; 4]));
        w.announce();
        w.engine.tape.set_on(true);
        w.run_until(day(0.9));
        w.engine.ingest(&Timed {
            at: w.t,
            obs: Obs::Watched {
                district: MILL,
                watched: false,
            },
        });
        w.engine.threat(&Threat {
            id: 9,
            source: ThreatSource::Bandit,
            class: ThreatClass::Siege,
            count: 3,
            strength: 3.0,
            target: ThreatTarget::District(MILL),
            loads: None,
            at: w.t,
        });
        w
    };
    let mut w = raided(100);
    let learned = of(&w, kinds::INDUSTRY_LEARNED);
    assert_eq!(learned.len(), 1, "{learned:?}");
    assert_eq!((learned[0].x, learned[0].z), centre);
    assert!((1..=1_000).contains(&learned[0].magnitude));
    let attack = of(&w, kinds::INDUSTRY_ATTACK);
    let damaged = of(&w, kinds::INDUSTRY_DAMAGED);
    assert_eq!(attack.len(), 1, "{attack:?}");
    assert_eq!(damaged.len(), 1, "{damaged:?}");
    for e in attack.iter().chain(&damaged) {
        assert_eq!((e.subject.value, e.object.value), (1, 0));
        assert_eq!((e.x, e.z), centre);
        assert!(e.payload, "{e:?}");
    }
    // Major, with what the bandits carried off as its magnitude.
    assert_eq!(attack[0].severity, Severity::Major);
    assert_eq!(attack[0].magnitude, w.engine.books.stolen.total() as i32);
    assert_eq!(damaged[0].severity, Severity::Notable);
    assert_eq!(damaged[0].magnitude, 1, "one machine block");
    // Raided again at once: the cooldown skips it, a minor row with nothing in it.
    w.engine.threat(&Threat {
        id: 10,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: w.t + 10,
    });
    let attack = of(&w, kinds::INDUSTRY_ATTACK);
    assert_eq!(attack.len(), 2, "{attack:?}");
    assert_eq!(attack[1].severity, Severity::Minor);
    assert_eq!(attack[1].magnitude, 0);
    assert!(!attack[1].payload);
    // The same raid on a perimeter that holds: the attack landed and took nothing, so it is minor
    // and nothing is damaged.
    let held = raided(1);
    let attack = of(&held, kinds::INDUSTRY_ATTACK);
    assert_eq!(attack.len(), 1, "{attack:?}");
    assert_eq!(attack[0].severity, Severity::Minor);
    assert_eq!(held.engine.stats.attacks, 1, "it was not skipped");
    assert!(of(&held, kinds::INDUSTRY_DAMAGED).is_empty());

    // A player edit makes a learned district stale (reason 2); a new declaration of the same
    // revision, not learnable, is recorded with its reason code.
    let mut w = mill(20_000, vec![(0.0, true)], false);
    w.engine.tape.set_on(true);
    w.run_until(day(0.9));
    let edit = Obs::Edit {
        district: MILL,
        kind: EditKind::Broken,
        player: (1, 2),
        pos: BlockPos::new(16, 64, 0),
    };
    w.engine.ingest(&Timed { at: w.t, obs: edit });
    let stale = of(&w, kinds::INDUSTRY_STALE);
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!((stale[0].x, stale[0].z), centre);
    assert_eq!((stale[0].subject.value, stale[0].magnitude), (1, 2));
    // Edited again: it is no longer learned, so it does not go stale twice.
    let edit = Obs::Edit {
        district: MILL,
        kind: EditKind::Placed,
        player: (1, 2),
        pos: BlockPos::new(16, 64, 0),
    };
    w.engine.ingest(&Timed { at: w.t, obs: edit });
    assert_eq!(of(&w, kinds::INDUSTRY_STALE).len(), 1);
    let code = NotLearnable::MovingContraption.code();
    w.engine.ingest(&Timed {
        at: w.t,
        obs: decl(1, Some(NotLearnable::MovingContraption)),
    });
    let unlearnable = of(&w, kinds::INDUSTRY_UNLEARNABLE);
    assert_eq!(unlearnable.len(), 1, "{unlearnable:?}");
    assert_eq!(unlearnable[0].magnitude, i32::try_from(code).unwrap());
    assert_eq!((unlearnable[0].x, unlearnable[0].z), centre);
    // The same declaration again changes nothing.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: decl(1, Some(NotLearnable::MovingContraption)),
    });
    assert_eq!(of(&w, kinds::INDUSTRY_UNLEARNABLE).len(), 1);

    // A new revision of a learned district: stale for membership (reason 1), placed where the
    // chunks were before the declaration cleared them.
    let mut w = mill(20_000, vec![(0.0, true)], false);
    w.engine.tape.set_on(true);
    w.run_until(day(0.9));
    w.engine.ingest(&Timed {
        at: w.t,
        obs: decl(2, None),
    });
    let stale = of(&w, kinds::INDUSTRY_STALE);
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!((stale[0].x, stale[0].z), centre);
    assert_eq!(stale[0].magnitude, 1);
    // Its chunks come with the new revision: not a district seen for the first time.
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::DistrictChunks {
            district: MILL,
            revision: 2,
            part: 0,
            chunks: vec![(0, 0)],
        },
    });
    assert!(of(&w, kinds::INDUSTRY_DETECTED).is_empty());

    // A district declared not learnable before it has chunks: found, and not learnable, at its
    // first chunks, once.
    let mut w = mill(20_000, vec![(0.0, true)], false);
    w.engine.tape.set_on(true);
    let other = DistrictId(7);
    let new = |revision| {
        Obs::District(DistrictDecl {
            district: other,
            revision,
            dim: 0,
            name: 9,
            not_learnable: Some(NotLearnable::MovingContraption),
            inventory_count: 0,
        })
    };
    w.engine.ingest(&Timed {
        at: w.t,
        obs: new(1),
    });
    assert!(rows(&w).is_empty(), "no chunks, no place, no row yet");
    w.engine.ingest(&Timed {
        at: w.t + 5,
        obs: Obs::DistrictChunks {
            district: other,
            revision: 1,
            part: 0,
            chunks: vec![(10, 20), (12, 20)],
        },
    });
    let found = of(&w, kinds::INDUSTRY_DETECTED);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].subject.value, found[0].magnitude), (7, 2));
    assert_eq!(
        (found[0].x, found[0].z, found[0].tick),
        (184, 328, w.t.0 + 5)
    );
    assert_eq!(of(&w, kinds::INDUSTRY_UNLEARNABLE).len(), 1);
    w.engine.ingest(&Timed {
        at: w.t + 6,
        obs: new(1),
    });
    w.engine.ingest(&Timed {
        at: w.t + 6,
        obs: Obs::DistrictChunks {
            district: other,
            revision: 1,
            part: 1,
            chunks: vec![(14, 20)],
        },
    });
    assert_eq!(of(&w, kinds::INDUSTRY_DETECTED).len(), 1);
    assert_eq!(of(&w, kinds::INDUSTRY_UNLEARNABLE).len(), 1);
}
