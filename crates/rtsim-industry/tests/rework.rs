// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ben's morning answers of 2026-10-20 (DECISIONS.md), end to end through the fake world:
//! learned coupling (I6), simulating the unloaded part of a district (I7), spawn-fed farms
//! replayed (I9), level-wide pollution applied live (I14), and reconciles sent ahead of a
//! player's arrival (the stale-tick race). Choices IR# in docs/choices-indrework.md.

use rtsim_industry::cmd::Cmd;
use rtsim_industry::engine::{DistrictState, Engine};
use rtsim_industry::fake::{ChunkVisit, FDistrict, FInventory, FakeWorld, Recipe, Visit};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::learn::Learned;
use rtsim_industry::obs::{
    DepositPluginId, DepositSample, DistrictDecl, InvKind, InventoryDecl, LoadState, NotLearnable,
    Obs, SweepEnd, Timed,
};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

const ORE: Good = Good(1);
const BAR: Good = Good(2);
const WOOD: Good = Good(3);
const PLANK: Good = Good(4);
const COAL: Good = Good(5);
const D: DistrictId = DistrictId(1);

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

fn vault(index: u32, x: i32, blocks: u32, contents: GoodVec) -> FInventory {
    FInventory {
        index,
        pos: BlockPos::new(x, 64, 0),
        kind: InvKind::ITEMS,
        slots: 20 * blocks,
        fluid_cap: 0,
        energy_cap: 0,
        contents,
        destroyed: false,
    }
}

fn recipe(
    name: &str,
    input: Option<(Good, u32, i64)>,
    output: (Good, u32, i64),
    machine_x: i32,
) -> Recipe {
    Recipe {
        name: name.into(),
        inputs: input.into_iter().collect(),
        outputs: vec![output],
        period: 100,
        target: None,
        deposit_per_batch: 0,
        power_from: None,
        machine: BlockPos::new(machine_x, 64, 0),
    }
}

fn district(inventories: Vec<FInventory>, recipes: Vec<Recipe>) -> FDistrict {
    FDistrict {
        id: D,
        name: "Works".into(),
        dim: 0,
        inventories,
        recipes,
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
    }
}

fn world(tun: Tunables, d: FDistrict, visits: &[(f64, bool)]) -> FakeWorld {
    let mut w = FakeWorld::new(5, tun);
    w.machines_tick_first = false;
    for (g, n) in [
        (ORE, "ore"),
        (BAR, "bar"),
        (WOOD, "wood"),
        (PLANK, "plank"),
        (COAL, "coal"),
    ] {
        w.good(g.0, GoodInfo::item(n, 64));
    }
    w.districts.push(d);
    w.visits = visits
        .iter()
        .map(|&(d, here)| Visit {
            from: day(d),
            at: if here { vec![0] } else { vec![] },
        })
        .collect();
    w
}

fn stock(w: &FakeWorld, g: Good) -> i64 {
    w.districts[0]
        .inventories
        .iter()
        .map(|i| i.contents.get(g))
        .sum()
}

// ---- I6: learned coupling ----------------------------------------------------------------------

/// A mill (ore into bars) and a saw (wood into planks) in one district. While loaded, ore and wood
/// each run out every cycle in turn (a train brings 300 ore at the start of each 6,000 ticks, 150
/// wood halfway), so the learner sees each input starve while the other line goes on. The player
/// leaves with no ore and 5,000 wood.
fn works(min_episodes: u32) -> FakeWorld {
    let mut tun = Tunables::default();
    tun.learn.coupling_settle_ticks = 400;
    tun.learn.coupling_min_span_ticks = 1_000;
    tun.learn.coupling_min_episodes = min_episodes;
    let d = district(
        vec![
            vault(0, 0, 10, GoodVec::new()),
            vault(1, 2, 10, GoodVec::new()),
            vault(2, 4, 10, GoodVec::new()),
            vault(3, 6, 10, GoodVec::new()),
        ],
        vec![
            recipe("mill", Some((ORE, 0, 8)), (BAR, 1, 8), 8),
            recipe("saw", Some((WOOD, 2, 4)), (PLANK, 3, 4), 10),
        ],
    );
    let mut w = world(tun, d, &[(0.0, true), (1.5, false), (3.0, true)]);
    w.declare();
    let leave = day(1.5).0;
    while w.t.0 < leave {
        w.tick();
        let t = w.t.0;
        if t + 300 < leave {
            if t.is_multiple_of(6_000) {
                w.import(0, 0, ORE, 300);
            }
            if t % 6_000 == 3_000 {
                w.import(0, 2, WOOD, 150);
            }
        }
        // Before the last sweep, so the engine knows it (anything after it is the world's).
        if t + 250 == leave {
            w.import(0, 2, WOOD, 5_000);
        }
    }
    w
}

/// I6 (Ben: "learned coupling"): the starvation episodes split the district into its two lines,
/// so with no ore the replay keeps sawing planks; with too little evidence the all-or-nothing
/// default stops everything; and neither makes more than the real saw could.
#[test]
fn learned_coupling_keeps_an_independent_line_running() {
    let mut coupled = works(2);
    let ev = coupled
        .engine
        .district(D)
        .unwrap()
        .learner
        .coupling
        .evidence
        .clone();
    eprintln!("evidence: {ev:?}");
    let ore = ev
        .iter()
        .find(|e| e.0 == ORE)
        .expect("ore starved")
        .1
        .clone();
    let wood = ev
        .iter()
        .find(|e| e.0 == WOOD)
        .expect("wood starved")
        .1
        .clone();
    assert!(ore.episodes >= 2 && wood.episodes >= 2, "{ev:?}");
    assert!(ore.independent.contains(&PLANK) && !ore.independent.contains(&BAR));
    assert!(wood.independent.contains(&BAR) && !wood.independent.contains(&PLANK));
    coupled.run_until(day(1.5) + 10);
    let procs = coupled.engine.sim_processes(D);
    assert_eq!(procs.len(), 2, "two processes: {procs:?}");
    let planks_at_unload = stock(&coupled, PLANK);
    coupled.run_until(day(2.5));
    let made = coupled.engine.sim_stock(D).unwrap().get(PLANK) - planks_at_unload;
    // The saw's learned average is 150 wood per 6,000 ticks; a day of it is 600 planks. The real
    // saw, never starved, could make 4 per 100 ticks: 960.
    assert!((550..=650).contains(&made), "planks while away: {made}");
    assert!(made <= 960, "never better than the real saw");

    let mut blind = works(1_000);
    blind.run_until(day(1.5) + 10);
    assert_eq!(
        blind.engine.sim_processes(D).len(),
        1,
        "all-or-nothing default"
    );
    let planks_at_unload = stock(&blind, PLANK);
    blind.run_until(day(2.5));
    let made_blind = blind
        .engine
        .sim_stock(D)
        .map_or(0, |s| s.get(PLANK) - planks_at_unload);
    assert_eq!(made_blind, 0, "no ore, so the one process does not run");

    coupled.run_until(day(3.0) + 200);
    coupled.conservation().unwrap();
    coupled.engine.check_books().unwrap();
}

// ---- I7: simulate the rest ---------------------------------------------------------------------

/// A mill whose ore vault (chunk 0), machine (chunk 1) and bar vault (chunk 2) are apart, with a
/// coal chest far off in chunk 9. The player stands by the chest for a while (only chunk 9
/// loads), then by the ore vault (only chunk 0).
fn edge_mill() -> FakeWorld {
    let d = district(
        vec![
            vault(0, 0, 10, GoodVec::from_pairs([(ORE, 20_000)])),
            vault(1, 32, 10, GoodVec::new()),
            vault(2, 150, 1, GoodVec::from_pairs([(COAL, 100)])),
        ],
        vec![recipe("mill", Some((ORE, 0, 8)), (BAR, 1, 8), 16)],
    );
    let mut w = world(
        Tunables::default(),
        d,
        &[(0.0, true), (1.0, false), (4.0, true)],
    );
    let chunk = |x: i32| ChunkPos { dim: 0, x, z: 0 };
    w.chunk_visits = vec![
        ChunkVisit {
            from: day(2.0),
            district: 0,
            chunks: vec![chunk(9)],
        },
        ChunkVisit {
            from: day(2.5),
            district: 0,
            chunks: vec![],
        },
        ChunkVisit {
            from: day(3.0),
            district: 0,
            chunks: vec![chunk(0)],
        },
        ChunkVisit {
            from: day(3.5),
            district: 0,
            chunks: vec![],
        },
    ];
    w.declare();
    w
}

/// I7 (Ben: "simulate the rest"): while only the coal chest's chunk is loaded the mill, all of
/// whose goods live in unloaded chunks, keeps running in the replay. While its ore vault is
/// loaded, the mill reaches a loaded chunk and its machine is in an unloaded one: it runs nowhere
/// (never both, never better than reality). The ore vault's chunk is reconciled when it loads, on
/// its own epoch; the bar vault's later, at the full load.
#[test]
fn the_unloaded_part_of_a_district_keeps_running() {
    let mut w = edge_mill();
    w.run_until(day(2.0) + 100);
    let dist = w.engine.district(D).unwrap();
    assert_eq!(dist.presence, rtsim_industry::district::Presence::Partial);
    assert_eq!(
        w.engine.sim_run(D),
        Some(rtsim_industry::cluster::Run::Running),
        "the mill replays while only the chest is loaded"
    );
    let a = w.engine.sim_stock(D).unwrap().get(BAR);
    w.run_until(day(2.4));
    let b = w.engine.sim_stock(D).unwrap().get(BAR);
    // 9,500 ticks at 8 a batch every 100: 760.
    assert!(b - a >= 750, "bars {a} -> {b} while in part loaded");
    assert_eq!(
        w.engine.inspect(D, w.t).unwrap().state,
        DistrictState::Suspended
    );

    w.run_until(day(3.0) + 100);
    let ore_chunk = ChunkPos { dim: 0, x: 0, z: 0 };
    let bar_chunk = ChunkPos { dim: 0, x: 2, z: 0 };
    let first = w
        .reconciled
        .iter()
        .find(|r| r.3 == ore_chunk)
        .map(|r| r.2)
        .expect("the ore vault's chunk reconciled at its load");
    assert!(
        !w.reconciled.iter().any(|r| r.3 == bar_chunk),
        "the bar vault's chunk waits for its own load"
    );
    assert!(
        w.engine.sim_run(D).is_none(),
        "the mill reaches a loaded chunk: it does not replay"
    );
    let bars_world = stock(&w, BAR);
    w.run_until(day(3.4));
    assert_eq!(stock(&w, BAR), bars_world, "nor does it run for real");

    w.run_until(day(4.0) + 200);
    let last = w
        .reconciled
        .iter()
        .filter(|r| r.3 == bar_chunk)
        .map(|r| r.2)
        .max()
        .expect("the bar vault's chunk reconciled at the full load");
    assert!(
        last > first,
        "each chunk on its own epoch: {first} then {last}"
    );
    // Ten days of ore at 8 per 100 ticks were never short: the bars made are the replay's two
    // days unloaded and its half day in part loaded, plus the loaded days.
    let held: i64 = w
        .engine
        .district(D)
        .unwrap()
        .inventories
        .values()
        .map(|i| i.held.total() as i64)
        .sum();
    assert_eq!(held, 0, "nothing left held once the whole district loaded");
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

/// The Java side that never reports a chunk unloading (before IND_CHUNK_UNLOADED): with no chunk
/// ticking, the whole district counts as unloaded again, as before.
#[test]
fn without_chunk_unload_records_an_unloaded_district_is_whole_again() {
    let mut w = edge_mill();
    w.chunk_unload_records = false;
    w.run_until(day(2.6));
    assert_eq!(
        w.engine.district(D).unwrap().presence,
        rtsim_industry::district::Presence::Unloaded
    );
    assert!(w.engine.district(D).unwrap().loaded_chunks.is_empty());
    assert!(w.engine.sim_run(D).is_some(), "the whole district replays");
    w.run_until(day(4.0) + 200);
    w.conservation().unwrap();
}

// ---- I9: replay everything -----------------------------------------------------------------------

/// I9 (Ben: "Everything"): a spawn-fed mob farm, which an older Java side declares not learnable
/// (code 6), learns and replays like any other district.
#[test]
fn a_spawn_fed_farm_replays() {
    let bone = Good(6);
    let d = district(
        vec![vault(0, 0, 10, GoodVec::new())],
        vec![recipe("drops", None, (bone, 0, 4), 2)],
    );
    let mut w = world(Tunables::default(), d, &[(0.0, true), (1.0, false)]);
    w.good(bone.0, GoodInfo::item("minecraft:bone", 64));
    w.declare();
    w.engine.ingest(&Timed {
        at: w.t,
        obs: Obs::District(DistrictDecl {
            district: D,
            revision: 1,
            dim: 0,
            name: 1,
            not_learnable: Some(NotLearnable::SpawnFedFarm),
            inventory_count: 1,
        }),
    });
    assert!(w.engine.district(D).unwrap().not_learnable.is_none());
    w.run_until(day(1.5));
    assert!(w.engine.sim_run(D).is_some(), "the farm replays");
    let before = stock(&w, bone);
    let made = w.engine.sim_stock(D).unwrap().get(bone) - before;
    assert!(made > 400, "bones while away: {made}");
}

// ---- the stale-tick race: reconcile ahead of arrival ---------------------------------------------

/// One mill, 3,000 ore: it runs out after 37,500 ticks of replay. Unloaded at day 0.6, back at
/// day 4. Machines tick on stale stock when the district loads.
fn short_mill(approach: bool) -> FakeWorld {
    let d = district(
        vec![
            vault(0, 0, 10, GoodVec::from_pairs([(ORE, 3_000)])),
            vault(1, 32, 10, GoodVec::new()),
        ],
        vec![recipe("mill", Some((ORE, 0, 8)), (BAR, 1, 8), 16)],
    );
    let mut w = world(
        Tunables::default(),
        d,
        &[(0.0, true), (0.6, false), (4.0, true)],
    );
    w.machines_tick_first = true;
    w.approach_signal = approach;
    w.declare();
    w
}

/// Ben's answer to the stale-tick race: when IND_APPROACH says a player is closing in, the
/// reconcile goes out before the chunks tick, and Java applies it at the chunk's load. The mill
/// that used to take 16 ore the replay had already counted (a shortfall, trimmed) finds the vault
/// as the replay left it.
#[test]
fn a_reconcile_ahead_of_arrival_leaves_no_stale_tick() {
    let mut old = short_mill(false);
    old.run_until(day(4.0) + 20);
    assert_eq!(
        old.engine.books.shortfall.get(ORE),
        16,
        "the race, as before"
    );

    let mut w = short_mill(true);
    w.run_until(day(4.0) + 20);
    assert!(w.engine.stats.reconciles_ahead >= 1);
    assert!(
        w.engine.books.shortfall.is_empty(),
        "no machine ran on stock the replay counted: {:?}",
        w.engine.books.shortfall
    );
    assert!(w.engine.books.trimmed.is_empty());
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

/// A player heads for the district and turns away: the replay stops when the reconcile goes
/// ahead, and goes on from the reconciled state once the closing records stop.
#[test]
fn a_player_who_turns_away_lets_the_replay_go_on() {
    let mut w = short_mill(false);
    w.run_until(day(1.0));
    assert!(w.engine.sim_run(D).is_some());
    let t = w.t;
    w.engine.ingest(&Timed {
        at: t,
        obs: Obs::Approach {
            district: D,
            distance: 200,
            eta_ticks: Some(400),
            closing: true,
        },
    });
    w.engine.advance_to(t);
    assert!(w.engine.sim_run(D).is_none(), "held for the player");
    assert_eq!(w.engine.stats.reconciles_ahead, 1);
    let sent: Vec<Cmd> = w.engine.drain_commands();
    assert!(
        sent.iter().any(|c| matches!(c, Cmd::ChunkReconcile(_))),
        "every chunk's command goes now"
    );
    for c in sent {
        w.deliver(c);
    }
    let hold = Tunables::default().replay.approach_hold_ticks;
    w.run_until(t + hold + 10);
    assert!(w.engine.sim_run(D).is_some(), "the replay goes on");
    w.run_until(day(4.0) + 200);
    assert_eq!(w.engine.stats.reconciles, 2);
    w.conservation().unwrap();
    w.engine.check_books().unwrap();
}

// ---- I14: level-wide pollution live --------------------------------------------------------------

/// I14 (Ben): Destroy's level-wide pollution from an unloaded district is applied live, chunk
/// types at the load. A smelter learned to emit greenhouse gas and smog (the Destroy plugin's
/// attributed deposit goods, its Q9) pushes greenhouse to the level every
/// `live_pollution_ticks` while away, as IND_DEPOSIT_DELTA; smog waits for the anchor chunk's
/// reconcile. What reaches the world is exactly what the replay made.
#[test]
fn level_pollution_reaches_the_level_while_away() {
    let greenhouse = Good(10);
    let smog = Good(11);
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(ORE, GoodInfo::item("ore", 64));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    e.define_good(
        greenhouse,
        GoodInfo::deposit("destroy:greenhouse/00000000000000aa"),
    );
    e.define_good(smog, GoodInfo::deposit("destroy:smog/00000000000000aa"));
    let send = |e: &mut Engine, at: u64, obs: Obs| e.ingest(&Timed { at: Tick(at), obs });
    let anchor = ChunkPos { dim: 0, x: 0, z: 0 };
    send(
        &mut e,
        0,
        Obs::District(DistrictDecl {
            district: D,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 1,
        }),
    );
    send(
        &mut e,
        0,
        Obs::Inventory(InventoryDecl {
            district: D,
            revision: 1,
            index: 0,
            pos: BlockPos::new(1, 64, 1),
            kind: InvKind(InvKind::ITEMS),
            item_slots: 2_000,
            fluid_capacity: 0,
            energy_capacity: 0,
        }),
    );
    send(
        &mut e,
        0,
        Obs::LoadState {
            district: D,
            state: LoadState::Live,
            ticking_chunks: 1,
            total_chunks: 1,
        },
    );
    send(
        &mut e,
        0,
        Obs::ChunkLoaded {
            district: D,
            chunk: anchor,
            applied_epoch: 0,
        },
    );
    send(
        &mut e,
        10,
        Obs::StockSample {
            district: D,
            inventory: 0,
            sweep: 1,
            goods: GoodVec::from_pairs([(ORE, 100_000)]),
        },
    );
    for g in [greenhouse, smog] {
        send(
            &mut e,
            10,
            Obs::DepositSample(DepositSample {
                district: D,
                good: g,
                plugin: DepositPluginId::DestroyCrude,
                chunk: anchor,
                remaining: 0,
                infinite: false,
                sweep: 1,
            }),
        );
    }
    send(
        &mut e,
        10,
        Obs::SweepEnd(SweepEnd {
            district: D,
            sweep: 1,
            records: 3,
            inventories: 1,
            live: true,
            creative: false,
            overstressed: false,
        }),
    );
    e.adopt_model(
        D,
        Learned {
            at: Tick(10),
            window_ticks: 12_000,
            rates: vec![(ORE, -0.04), (BAR, 0.04), (greenhouse, 0.01), (smog, 0.01)],
            links: vec![],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
            latency_ticks: 0,
        },
        vec![],
    );
    e.advance_to(Tick(10));
    e.drain_commands();
    send(
        &mut e,
        100,
        Obs::LoadState {
            district: D,
            state: LoadState::Unloaded,
            ticking_chunks: 0,
            total_chunks: 1,
        },
    );
    e.advance_to(Tick(100 + 6_000));
    let live: Vec<(Good, i64)> = e
        .drain_commands()
        .into_iter()
        .filter_map(|c| match c {
            Cmd::DepositDelta(d) => Some((d.good, d.delta)),
            _ => None,
        })
        .collect();
    assert_eq!(
        live.len(),
        5,
        "one push a minute for five minutes: {live:?}"
    );
    assert!(
        live.iter().all(|x| x.0 == greenhouse),
        "only the level type"
    );
    let pushed: i64 = live.iter().map(|x| x.1).sum();
    assert!((58..=60).contains(&pushed), "greenhouse pushed {pushed}");
    assert_eq!(e.stats.pollution_pushes, 5);
    // Back: smog in the anchor chunk's reconcile, greenhouse's remainder at once.
    send(
        &mut e,
        100 + 6_500,
        Obs::LoadState {
            district: D,
            state: LoadState::Live,
            ticking_chunks: 1,
            total_chunks: 1,
        },
    );
    send(
        &mut e,
        100 + 6_500,
        Obs::ChunkLoaded {
            district: D,
            chunk: anchor,
            applied_epoch: 0,
        },
    );
    e.advance_to(Tick(100 + 6_500));
    let (mut smog_back, mut gh_back) = (0, 0);
    for c in e.drain_commands() {
        match c {
            Cmd::ChunkReconcile(r) => {
                smog_back += r
                    .deltas
                    .iter()
                    .filter(|x| x.good == smog)
                    .map(|x| x.delta)
                    .sum::<i64>();
                assert!(r.deltas.iter().all(|x| x.good != greenhouse));
            }
            Cmd::DepositDelta(d) if d.good == greenhouse => gh_back += d.delta,
            _ => {}
        }
    }
    let minted = e.books.minted.clone();
    assert_eq!(pushed + gh_back, minted.get(greenhouse), "greenhouse once");
    assert_eq!(smog_back, minted.get(smog), "smog at the load");
    assert!(
        (63..=65).contains(&minted.get(smog)),
        "smog {}",
        minted.get(smog)
    );
    let s = &e.district(D).unwrap().deposits;
    assert_eq!(
        s.get(&greenhouse).unwrap().remaining,
        minted.get(greenhouse)
    );
}
