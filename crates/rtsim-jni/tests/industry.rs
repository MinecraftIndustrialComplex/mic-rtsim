// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine through the native link (docs/industry-boundary.md): rtsim-industry's fake
//! world plays Minecraft and the industry module behind the fake Java side of `common`, on the
//! real host. Every observation and command crosses the boundary buffers: industry records through
//! `rtsim_industry::wire`, journaled ones through the bridge's journal and the engine's `Replay`,
//! commands with the bridge's sequences and the engine's dictionary, saves through the page store.
//! The world and the engine run in lockstep (one hand-over a tick, its commands back before the
//! next tick), so every run is deterministic.

mod common;

use std::time::Duration;

use common::*;
use rtsim_industry::cmd::Cmd;
use rtsim_industry::district::Phase;
use rtsim_industry::engine::Engine as Industry;
use rtsim_industry::fake::{FDistrict, FInventory, FakeWorld, Recipe, Visit, exposed, survey};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{BlockFlags, InvKind, Obs, Timed};
use rtsim_industry::scenario::{
    INGOT, IronLine, ORE_COAST, iron_line, iron_line_world, run_iron_line,
};
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_industry::tunables::Tunables;
use rtsim_jni::engine::{Engine, MODULE_INDUSTRY};
use rtsim_jni::orders::Order;
use rtsim_jni::wire;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

const MILL: DistrictId = DistrictId(1);
/// Java's dimension id for the overworld: the first entry of the harness's dictionary.
const DIM: u32 = 1;

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// The goods and block states of the mill, in Java's dictionary.
struct Ids {
    ore: Good,
    bar: Good,
    fence: u32,
    mill_block: u32,
}

/// One mill, as rtsim-industry's end-to-end tests have it: 8 ore into 8 bars every 100 ticks, ore
/// and bars in vaults in different chunks; a fence on its perimeter and the mill itself exposed,
/// all in the ore vault's chunk (the district's chunks are its inventories').
fn mill(j: &mut Java, ore: i64, visits: &[(f64, bool)], tick_first: bool) -> Ids {
    mill_with(j, ore, visits, tick_first, |_, _, _| {})
}

/// [`mill`], with `extra` changing the world and the district before they are declared.
fn mill_with(
    j: &mut Java,
    ore: i64,
    visits: &[(f64, bool)],
    tick_first: bool,
    extra: impl FnOnce(&mut Java, &mut FakeWorld, &mut FDistrict),
) -> Ids {
    assert_eq!(j.dict_id(wire::DICT_DIMENSION, OVERWORLD), DIM);
    let ids = Ids {
        ore: Good(j.dict_id(DICT_GOOD, "i/64/test:ore")),
        bar: Good(j.dict_id(DICT_GOOD, "i/64/test:bar")),
        fence: j.dict_id(DICT_BLOCK_STATE, "minecraft:oak_fence[east=true]"),
        mill_block: j.dict_id(DICT_BLOCK_STATE, "create:millstone"),
    };
    let mut w = FakeWorld::new(3, Tunables::default()).into_remote();
    w.machines_tick_first = tick_first;
    w.good(ids.ore.0, GoodInfo::item("test:ore", 64));
    w.good(ids.bar.0, GoodInfo::item("test:bar", 64));
    let inv = |index: u32, x: i32, contents: GoodVec| FInventory {
        index,
        pos: BlockPos::new(x, 64, 0),
        kind: InvKind::ITEMS,
        slots: 200,
        fluid_cap: 0,
        energy_cap: 0,
        contents,
        destroyed: false,
    };
    let machine = BlockPos::new(8, 64, 4);
    let mut district = FDistrict {
        id: MILL,
        name: "Mill".into(),
        dim: DIM,
        inventories: vec![
            inv(0, 0, GoodVec::from_pairs([(ids.ore, ore)])),
            inv(1, 32, GoodVec::new()),
        ],
        recipes: vec![Recipe {
            name: "mill".into(),
            inputs: vec![(ids.ore, 0, 8)],
            outputs: vec![(ids.bar, 1, 8)],
            period: 100,
            target: None,
            deposit_per_batch: 0,
            power_from: None,
            machine,
        }],
        deposit: None,
        deposit_chunk: ChunkPos::default(),
        exposed: vec![
            exposed(
                BlockPos::new(1, 64, 0),
                ids.fence,
                BlockFlags::PERIMETER | BlockFlags::ROOFLESS,
                None,
            ),
            exposed(
                BlockPos::new(1, 64, 1),
                ids.fence,
                BlockFlags::PERIMETER | BlockFlags::ROOFLESS,
                None,
            ),
            exposed(
                machine,
                ids.mill_block,
                BlockFlags::MACHINE | BlockFlags::ROOFLESS,
                None,
            ),
        ],
        // No wall (no perimeter columns): every attacker gets in.
        survey: Some(survey(MILL, 0, 0, 0, 0, 1, [0.0; 4])),
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
    };
    extra(j, &mut w, &mut district);
    w.districts.push(district);
    w.visits = visits
        .iter()
        .map(|&(d, here)| Visit {
            from: day(d),
            at: if here { vec![0] } else { vec![] },
        })
        .collect();
    w.declare();
    j.mc = Some(w);
    j.module_mask = MODULE_INDUSTRY;
    ids
}

fn bars(j: &Java, ids: &Ids) -> i64 {
    j.mc().districts[0]
        .inventories
        .iter()
        .map(|i| i.contents.get(ids.bar))
        .sum()
}

/// Ticks the world, a lockstep each, until the pages hold every observation through `through`
/// (a save asked for with `request_save`). Panics after 60 s of wall clock instead of spinning
/// for ever: a hung test binary holds the shared cargo lock.
fn mc_tick_until_durable(j: &mut Java, through: u64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while j.e.durable_obs_seq() < through {
        assert!(
            std::time::Instant::now() < deadline,
            "the pages never became durable: waited 60 s for durable_obs_seq to reach the \
             requested save; durable_obs_seq {}, requested through {through}",
            j.e.durable_obs_seq()
        );
        j.mc_tick();
    }
}

fn industry<R>(e: &Engine, f: impl FnOnce(&Industry) -> R) -> R {
    e.with_industry(|ie, _| f(ie))
}

/// The world's books close against the engine's (rtsim-industry's end-to-end check).
fn conserves(j: &Java) {
    let mc = j.mc();
    j.e.with_industry(|ie, _| {
        mc.conservation_with(ie).expect("the world conserves");
        ie.check_books().expect("the replay's books close");
    });
}

fn reconciles(j: &Java) -> Vec<(u64, u32, ChunkPos)> {
    j.mc()
        .reconciled
        .iter()
        .map(|r| (r.0.0, r.2, r.3))
        .collect()
}

fn mill_engine(tag: &str) -> (std::path::PathBuf, Java) {
    let dir = tmpdir(tag);
    let e = engine(&dir, 256 * 1024);
    (dir.clone(), Java::open(e, WorldSave::fresh()))
}

/// Learn while loaded, unload, replay, reconcile ahead of a player who turns away, replay on,
/// reload and reconcile.
#[test]
fn a_mill_learns_replays_and_reconciles_through_the_native_engine() {
    let (_dir, mut j) = mill_engine("ind-mill");
    let ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        true,
    );
    let t0 = std::time::Instant::now();
    // Learning, while loaded: live sweeps every 100 ticks reach the engine as latest-wins records.
    j.mc_run_until(Tick(day(1.0).0 - 1));
    let phase = industry(&j.e, |ie| ie.district(MILL).map(|d| d.phase));
    assert_eq!(phase, Some(Phase::Learned), "learned while loaded");
    let loaded_bars = bars(&j, &ids);
    // Unloaded: the replay runs.
    j.mc_run_until(day(2.0));
    assert!(
        industry(&j.e, |ie| ie.sim_run(MILL).is_some()),
        "replaying while unloaded"
    );
    assert_eq!(bars(&j, &ids), loaded_bars, "nothing runs in the world");
    // IND_APPROACH (the Java side's H23), closing: the reconcile goes out ahead of arrival, held by
    // Java until the chunks load; the player turns away, the hold ends and the replay goes on.
    let t = j.mc().t;
    j.industry_obs(&Timed {
        at: t,
        obs: Obs::Approach {
            district: MILL,
            distance: 120,
            eta_ticks: Some(300),
            closing: true,
        },
    });
    j.mc_tick();
    let approach = industry(&j.e, |ie| ie.district(MILL).and_then(|d| d.approach));
    assert_eq!(
        approach.map(|a| (a.distance, a.eta_ticks, a.closing)),
        Some((120, Some(300), true))
    );
    assert_eq!(industry(&j.e, |ie| ie.stats.reconciles_ahead), 1);
    assert!(
        industry(&j.e, |ie| ie.sim_run(MILL).is_none()),
        "held while the player comes"
    );
    assert!(j.mc().commands_in_flight() > 0, "Java holds the reconcile");
    j.mc_run_until(day(2.0) + 400);
    assert!(
        industry(&j.e, |ie| ie.sim_run(MILL).is_some()),
        "the player turned away: the replay goes on"
    );
    // Reload: the reconcile lands one tick or two after the chunks report their epochs.
    j.mc_run_until(day(3.0) + 2_000);
    let r = reconciles(&j);
    eprintln!(
        "reconciles {r:?}; {} ticks in {:?}",
        j.mc().t.0,
        t0.elapsed()
    );
    assert_eq!(
        industry(&j.e, |ie| ie.stats.reconciles),
        2,
        "two reconcile rounds: ahead of the approach, and at the load"
    );
    assert!(
        !r.is_empty() && r.iter().all(|x| x.1 == 1 || x.1 == 2),
        "epochs 1 and 2 applied: {r:?}"
    );
    // Two days away at 8 per 100 ticks: 3,840 bars by the replay, less the 100 ticks it was held
    // for the player who turned away, give or take the machine that ticks on stale stock before
    // the reconcile (the trim keeps the books exact).
    let made = bars(&j, &ids);
    let expect = loaded_bars + 3_840 - 8 + 160;
    assert!(
        (made - expect).abs() <= 24,
        "bars {made}, expected about {expect}"
    );
    conserves(&j);
    assert_eq!(j.stale_commands, 0, "no command at or below an applied one");
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    let stats = j.e.with_industry(|_, s| *s);
    eprintln!("link: {stats:?}");
    assert_eq!(stats.bad_records + stats.unknown_kinds + stats.dropped, 0);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// Per good, the deltas of a district's chunk reconciles, once per op id.
fn reconcile_sums<'a>(
    cmds: impl IntoIterator<Item = &'a Cmd>,
) -> std::collections::BTreeMap<u32, i64> {
    let mut seen = std::collections::BTreeSet::new();
    let mut sums = std::collections::BTreeMap::new();
    for c in cmds {
        if let Cmd::ChunkReconcile(r) = c
            && r.district == MILL
            && seen.insert(r.op)
        {
            for x in &r.deltas {
                *sums.entry(x.good.0).or_insert(0) += x.delta;
            }
        }
    }
    sums
}

/// The replay offline (`glass_replay_offline`, the stub engine's G2 exit GameTests): a fresh
/// engine over the records the native engine received sends the mill the reconcile the native
/// engine sent, give or take a batch (it steps at each record's time, the link at each hand-over).
#[test]
fn the_offline_replay_sends_the_reconciles_the_native_engine_sent() {
    let (_dir, mut j) = mill_engine("ind-replay-offline");
    let ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        true,
    );
    let mut stream = Vec::new();
    let mut keep = |j: &Java| {
        for r in j.received() {
            if !r.skipped {
                stream.extend_from_slice(&r.bytes);
            }
        }
    };
    while j.mc().t < day(3.0) + 2_000 {
        let to = j.mc().t + 1_000;
        j.mc_run_until(to);
        keep(&j);
    }
    let live = reconcile_sums(j.ind_commands.iter().map(|c| &c.2));
    assert_eq!(industry(&j.e, |ie| ie.stats.reconciles), 1);
    assert!(
        live.get(&ids.bar.0).copied().unwrap_or(0) > 3_000,
        "two days of bars: {live:?}"
    );
    let flat =
        rtsim_jni::engine::glass_replay_offline(&stream, MILL.0).expect("the stream decodes");
    assert_eq!(flat.len() % 3, 0);
    let mut offline = std::collections::BTreeMap::new();
    for t in flat.as_chunks::<3>().0 {
        assert!(t[0] == 0 || t[0] == 1, "the mill's two vaults: {t:?}");
        *offline.entry(t[1] as u32).or_insert(0) += t[2];
    }
    eprintln!("live {live:?}, offline {offline:?}, {} bytes", stream.len());
    for g in live.keys().chain(offline.keys()) {
        let (l, o) = (
            live.get(g).copied().unwrap_or(0),
            offline.get(g).copied().unwrap_or(0),
        );
        assert!((l - o).abs() <= 8, "good {g}: live {l}, offline {o}");
    }
    // A cut stream is refused, not replayed in part.
    assert!(rtsim_jni::engine::glass_replay_offline(&stream[..stream.len() - 3], MILL.0).is_err());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// What a crash leaves: the engine at its pages, the world at its save, maybe a chunk saved ahead.
enum Crash {
    /// The world saved after the reconcile was applied and its result journaled; the pages hold it.
    AfterTheResult,
    /// The world and the pages saved while the mill was away; the crash came after the reconcile.
    BeforeTheLoad,
    /// As `BeforeTheLoad`, but the mill's chunks were saved on unload after the reconcile (ahead of
    /// the journal), with the epoch and the contents (docs/boundary.md §3.1).
    ChunkSavedAhead,
}

fn crash_run(crash: Crash) {
    let (dir, mut j) = mill_engine("ind-crash");
    // ChunkSavedAhead: the player leaves again right after the reconcile (the chunks unload and
    // save), then everything loads at the end so every reconcile lands.
    let visits: &[(f64, bool)] = match crash {
        Crash::ChunkSavedAhead => &[
            (0.0, true),
            (1.0, false),
            (3.0, true),
            (3.05, false),
            (3.5, true),
        ],
        _ => &[(0.0, true), (1.0, false), (3.0, true)],
    };
    let ids = mill(&mut j, 20_000, visits, false);
    let save_at = match crash {
        Crash::AfterTheResult => day(3.0) + 100,
        _ => day(2.5),
    };
    j.mc_run_until(save_at);
    // The world saves; the engine is asked for pages that stop exactly there.
    let jsave = j.world_save();
    let msave = j.mc().saved();
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    assert_eq!(
        j.e.durable_obs_seq(),
        through,
        "the pages stop at the world save"
    );
    let crash_at = match crash {
        Crash::AfterTheResult => day(3.0) + 600,
        Crash::BeforeTheLoad => day(3.0) + 600,
        Crash::ChunkSavedAhead => day(3.1),
    };
    j.mc_run_until(crash_at);
    let first = reconciles(&j);
    let bars_first = bars(&j, &ids);
    assert!(
        !first.is_empty(),
        "the reconcile was applied before the crash"
    );
    eprintln!("before the crash: reconciles {first:?}, bars {bars_first}");
    let ahead = j.mc().saved();
    // The crash: no save lands, the world rolls back to its save.
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    match crash {
        Crash::ChunkSavedAhead => mc.restore(&msave.with_chunks_of(&ahead)),
        _ => mc.restore(&msave),
    }
    let applied_before = mc.reconciled.len();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    // To the moment of the crash: a world that never saw what was lost runs it again.
    j.mc_run_until(crash_at);
    let bars_again = bars(&j, &ids);
    let end = match crash {
        Crash::ChunkSavedAhead => day(3.6),
        _ => day(3.0) + 2_000,
    };
    j.mc_run_until(end);
    let after: Vec<_> = reconciles(&j)[applied_before..].to_vec();
    eprintln!(
        "after the restart: reconciles {after:?}, bars {}",
        bars(&j, &ids)
    );
    let epoch1 = |v: &[(u64, u32, ChunkPos)]| {
        let mut x: Vec<_> = v.iter().filter(|r| r.1 == 1).copied().collect();
        x.sort();
        x
    };
    match crash {
        Crash::AfterTheResult => {
            assert!(after.is_empty(), "nothing applied again: {after:?}");
        }
        Crash::BeforeTheLoad => {
            // The same decision at the same tick from the same pages: the same op ids, applied once
            // in the world that never saw them.
            assert_eq!(
                epoch1(&after),
                epoch1(&first),
                "epoch 1 re-derived exactly, applied once"
            );
            assert_eq!(bars_again, bars_first, "the same bars as before the crash");
        }
        Crash::ChunkSavedAhead => {
            assert!(
                epoch1(&after).is_empty(),
                "the chunks hold epoch 1 already: never applied twice ({after:?})"
            );
        }
    }
    conserves(&j);
    assert_eq!(j.stale_commands, 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_reconcile_applies_once_when_the_world_save_holds_its_result() {
    crash_run(Crash::AfterTheResult);
}

#[test]
fn a_reconcile_is_rederived_and_applied_once_after_a_crash_before_the_load() {
    crash_run(Crash::BeforeTheLoad);
}

#[test]
fn a_reconcile_in_a_chunk_saved_ahead_of_the_journal_is_never_applied_twice() {
    crash_run(Crash::ChunkSavedAhead);
}

/// A raid on the mill while it is away resolves at its next load, in the reconcile: the stolen
/// goods leave the vaults and the blocks it broke are broken, by their expected states.
#[test]
fn a_raid_while_away_lands_at_the_next_load() {
    let (_dir, mut j) = mill_engine("ind-raid");
    let ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        false,
    );
    j.mc_run_until(day(2.0));
    let t = j.mc().t;
    j.e.order(vec![Order::IndustryThreat(Threat {
        id: 77,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::District(MILL),
        loads: None,
        at: t,
    })])
    .unwrap();
    j.mc_run_until(day(2.0) + 10);
    let (stolen, attacks) = industry(&j.e, |ie| {
        (
            ie.books.stolen.clone(),
            ie.district(MILL).map_or(0, |d| d.attacks.len()),
        )
    });
    eprintln!("stolen {stolen:?}, attacks {attacks}");
    assert_eq!(attacks, 1, "the attack is booked");
    assert!(stolen.total() > 0, "bandits carry goods away");
    assert!(
        j.mc().districts[0].broken.is_empty(),
        "nothing happens in the world while it is away"
    );
    j.mc_run_until(day(3.0) + 2_000);
    let broken = j.mc().districts[0].broken.clone();
    eprintln!("broken at the load: {broken:?}");
    assert!(
        !broken.is_empty(),
        "the raid broke blocks, applied at the load"
    );
    // The fence went first (perimeter first for walkers), by the BLOCK_STATE the engine named.
    assert!(
        broken.contains(&BlockPos::new(1, 64, 0)) && broken.contains(&BlockPos::new(1, 64, 1)),
        "both fence blocks"
    );
    let destroys: usize = j
        .ind_commands
        .iter()
        .map(|c| match &c.2 {
            Cmd::ChunkReconcile(r) => r.destroy.len(),
            _ => 0,
        })
        .sum();
    assert!(destroys >= broken.len());
    let _ = ids;
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// Industry commands carry the bridge's sequences (rising, never at or below an applied one) and
/// industry op ids; a restart with no pages at all starts the industry engine afresh.
#[test]
fn industry_commands_follow_the_bridge_rules() {
    let (_dir, mut j) = mill_engine("ind-rules");
    let _ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        false,
    );
    j.mc_run_until(day(3.0) + 500);
    let seqs: Vec<u64> = j.ind_commands.iter().map(|c| c.1).collect();
    assert!(!seqs.is_empty());
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "sequences rise: {seqs:?}"
    );
    for (_, _, c) in &j.ind_commands {
        if let Some(op) = c.op() {
            assert_eq!(op.0 & wire::JAVA_OP_BIT, 0, "an engine op id");
            assert_eq!((op.0 >> 56) & 0x7f, 0x49, "the industry namespace");
        }
    }
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// The iron line (rtsim-industry's reference scenario: a drill on a finite Ore Excavation vein, two
/// trains between interface stations, a smelter powered over a PowerGrid line from a boiler, raids
/// on the smelter and the trains, a dragon on the coast) through the native engine, against the
/// same world run on an engine in process.
#[test]
fn the_iron_line_runs_through_the_native_engine() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: true,
        control: false,
        tick_first: true,
        approach: true,
    };
    let t0 = std::time::Instant::now();
    let mut reference = iron_line(cfg, Tunables::default());
    run_iron_line(&mut reference, &cfg);
    let reference_time = t0.elapsed();

    let dir = tmpdir("ind-iron");
    let mut j = iron_java(&dir, cfg);
    let t1 = std::time::Instant::now();
    iron_run_to_end(&mut j, &cfg);
    let native_time = t1.elapsed();
    if std::env::var("IND_DEBUG").is_ok() {
        j.e.with_industry(|ie, _| {
            eprintln!("native books: {:?}", ie.books);
            for ev in ie.log.iter() {
                eprintln!("N {} d{} {}", ev.tick, ev.district, ev.what);
            }
        });
        eprintln!("reference books: {:?}", reference.engine.books);
        for ev in reference.engine.log.iter() {
            eprintln!("R {} d{} {}", ev.tick, ev.district, ev.what);
        }
    }
    conserves(&j);
    let (stats, link) = j.e.with_industry(|ie, l| (ie.stats, *l));
    eprintln!(
        "iron line: {} ticks; in process {reference_time:?}, through the native engine {native_time:?}",
        j.mc().t.0
    );
    eprintln!("native engine: {stats:?}");
    eprintln!("reference engine: {:?}", reference.engine.stats);
    eprintln!("link: {link:?}");
    assert!(stats.attacks > 0, "raids and the dragon happened");
    assert!(stats.exchanges > 0, "simulated stations exchanged cargo");
    assert!(stats.reconciles > 0);
    assert_eq!(link.outbox, 0, "every kept command answered");
    assert_eq!(
        link.results_failed + link.dropped + link.bad_records + link.unknown_kinds,
        0
    );
    assert_eq!(j.stale_commands, 0);
    let made = |w: &FakeWorld, e: &Industry| {
        w.books.minted.get(INGOT) + e.books.minted.get(INGOT) - e.books.trimmed.get(INGOT)
    };
    let native = j.e.with_industry(|ie, _| made(j.mc(), ie));
    let reference_made = made(&reference, &reference.engine);
    eprintln!("ingots made: native {native}, reference {reference_made}");
    let carriage_deltas = j
        .ind_commands
        .iter()
        .filter(|c| matches!(c.2, Cmd::CarriageDelta(_)))
        .count();
    eprintln!(
        "carriage deltas {carriage_deltas}, industry commands {}",
        j.ind_commands.len()
    );
    assert!(carriage_deltas > 0);
    // The same scenario and world seed, the same engine, the boundary between: the link merges a
    // hand-over's latest-wins and journaled records back into game-time order (W9), so the run
    // is the in-process one, decision for decision.
    assert_eq!(stats, reference.engine.stats, "the same run as in process");
    assert_eq!(native, reference_made);
    j.e.with_industry(|ie, _| assert_eq!(ie.books, reference.engine.books));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// The iron line's world against a native engine with the scenario's world seed. Java's dictionary
/// holds the scenario's goods as ids 1 to 4 and its block states as 11 to 13 and 21 to 23 (the
/// harness numbers every kind in one sequence, so it is filled to match).
fn iron_java(dir: &std::path::Path, cfg: IronLine) -> Java {
    let e = engine_with(&dir.to_path_buf(), 256 * 1024, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = Java::open(e, WorldSave::fresh());
    // Java's dictionary: the scenario's goods are ids 1 to 4 and its block states 11 to 13 and
    // 21 to 23, so the harness's one sequence of ids is filled to match.
    let mut w = iron_line_world(cfg, Tunables::default()).into_remote();
    for g in 1..=4u32 {
        let name = w.goods[&Good(g)].dictionary_name();
        assert_eq!(j.dict_id(DICT_GOOD, &name), g);
    }
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let mut next = dim + 1;
    for state in [11u32, 12, 13, 21, 22, 23] {
        while next < state {
            j.dict_id(0x7E, &format!("pad/{next}"));
            next += 1;
        }
        assert_eq!(
            j.dict_id(DICT_BLOCK_STATE, &format!("test:block_{state}")),
            state
        );
        next += 1;
    }
    w.move_to_dim(dim);
    w.declare();
    j.mc = Some(w);
    j.module_mask = MODULE_INDUSTRY;
    j
}

/// Runs the iron line to its end as `run_iron_line` does (the player repairs the coast when
/// there), then until every command is applied: the conservation check's premise.
fn iron_run_to_end(j: &mut Java, cfg: &IronLine) {
    let end = Tick(cfg.days * TICKS_PER_DAY);
    let mut repaired = false;
    while j.mc().t < end + 2_000 {
        j.mc_tick();
        let t = j.mc().t;
        if !repaired && t >= day(8.6) && t < end && j.mc().districts[ORE_COAST].loaded {
            j.mc_mut().repair_all(ORE_COAST);
            repaired = true;
        }
    }
    let mut settle = 0;
    while j.mc().commands_in_flight() > 0 {
        j.mc_tick();
        settle += 1;
        assert!(settle < 100, "commands keep coming");
    }
}

/// A world whose bridge answers every IND_CARRIAGE_DELTA failed (W10): nothing moved, so the engine
/// gives each district back what the exchange took or gave, and the books still close.
#[test]
fn a_failed_carriage_delta_is_taken_back() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-failcar");
    let mut j = iron_java(&dir, cfg);
    j.mc_mut().fail_carriage_deltas = true;
    iron_run_to_end(&mut j, &cfg);
    let (stats, link) = j.e.with_industry(|ie, l| (ie.stats, *l));
    eprintln!("engine {stats:?}; link {link:?}");
    assert!(stats.exchanges > 0);
    assert!(link.results_failed > 0, "Java failed the carriage deltas");
    assert_eq!(link.outbox, 0, "a failed command is answered");
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// MODULES with or without the industry bit, as the bridge sends it mid-session.
fn switch_industry(j: &mut Java, on: bool) {
    let Some(r) = j.reserve(
        rtsim_jni::wire::obs::MODULES,
        rtsim_jni::wire::obs::modules::SIZE,
        0,
    ) else {
        panic!("room for MODULES")
    };
    let mask = if on { MODULE_INDUSTRY } else { 0 };
    rtsim_jni::wire::put_u32(r, rtsim_jni::wire::obs::modules::MASK, mask);
}

/// The industry module switch (C25, U10, replacing W8): off, the engine ignores every industry
/// record, journaled ones included, as the dragons and ships do; on again, Java announces its
/// districts as at a session start and the engine learns.
#[test]
fn the_module_switch_holds_the_industry_engine() {
    let (_dir, mut j) = mill_engine("ind-switch");
    let _ids = mill(&mut j, 20_000, &[(0.0, true)], false);
    // The session starts with the module off (SESSION_START carries the mask).
    j.module_mask = 0;
    j.mc_run_until(day(0.8));
    let (known, ingested, ignored) =
        j.e.with_industry(|ie, l| (ie.district(MILL).is_some(), l.ingested, l.ignored));
    assert!(!known, "not even the declarations went in");
    assert_eq!(ingested, 0);
    assert!(ignored > 0, "every industry record was ignored and counted");
    // MODULES with the industry bit: Java announces the district again, sweeps flow, it learns.
    switch_industry(&mut j, true);
    j.mc_tick();
    j.mc_mut().announce();
    j.mc_run_until(day(1.6));
    let phase = industry(&j.e, |ie| ie.district(MILL).map(|d| d.phase));
    assert_eq!(phase, Some(Phase::Learned));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// U10: a district replaying when the module is switched off is frozen with it, and catches up
/// when it is switched on again: the replay covers the time it was off, and the reconcile at the
/// next load brings it all.
#[test]
fn a_switched_off_industry_catches_up_when_on_again() {
    let (_dir, mut j) = mill_engine("ind-catchup");
    let ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        false,
    );
    j.mc_run_until(day(1.2));
    assert!(industry(&j.e, |ie| ie.sim_run(MILL).is_some()), "replaying");
    let loaded_bars = bars(&j, &ids);
    switch_industry(&mut j, false);
    j.mc_run_until(day(2.2));
    let (now, ignored) = j.e.with_industry(|ie, l| (ie.now(), l.ignored));
    assert!(now < day(1.3), "frozen while off: {now:?}");
    let _ = ignored;
    switch_industry(&mut j, true);
    j.mc_tick();
    j.mc_mut().announce();
    j.mc_run_until(day(2.3));
    let now = industry(&j.e, |ie| ie.now());
    assert!(now >= day(2.2), "caught up: {now:?}");
    assert!(
        industry(&j.e, |ie| ie.sim_run(MILL).is_some()),
        "replaying on"
    );
    j.mc_run_until(day(3.0) + 2_000);
    // Two days away at 8 per 100 ticks, the day it was off included: 3,840 bars by the replay.
    let made = bars(&j, &ids);
    let expect = loaded_bars + 3_840 + 160;
    assert!(
        (made - expect).abs() <= 24,
        "bars {made}, expected about {expect}"
    );
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// A kept command (IND_CARRIAGE_DELTA) written, saved in the pages' outbox and never applied by a
/// world save that a crash then loses, is written again first after the restart, with its own
/// sequence and op id, and applied once.
#[test]
fn a_kept_command_is_resent_after_a_crash_with_its_sequence_and_op() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-resend");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    // From here Java reads the command buffers but its world never applies them: a world save
    // that will be lost.
    j.hold_commands = true;
    while !j
        .held_ind
        .iter()
        .any(|c| matches!(c.1, Cmd::CarriageDelta(_)))
    {
        j.mc_tick();
        assert!(j.mc().t < day(4.0), "a station exchange within a day");
    }
    let (held_seq, held) = j
        .held_ind
        .iter()
        .find(|c| matches!(c.1, Cmd::CarriageDelta(_)))
        .cloned()
        .unwrap();
    let held_op = held.op().unwrap();
    // The world saves now (it never applied the delta), and the pages stop at the same point:
    // their outbox holds the delta.
    let jsave = j.world_save();
    let msave = j.mc().saved();
    let through = j.next_obs - 1;
    assert!(jsave.last_applied_cmd < held_seq);
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    assert!(
        j.e.with_industry(|_, l| l.outbox) >= 1,
        "the delta waits in the outbox"
    );
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, jsave);
    // The start counts every module's resends for Java's perf line (RP-205).
    assert!(
        j.e.stats_array()[16] >= 1,
        "the start counts the kept delta among its resends"
    );
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    j.mc_tick();
    let first = j
        .ind_commands
        .iter()
        .find(|c| matches!(c.2, Cmd::CarriageDelta(_)))
        .cloned()
        .expect("the delta is written again in the first step");
    eprintln!("held seq {held_seq} op {:#x}; resent {first:?}", held_op.0);
    assert_eq!(first.1, held_seq, "its original sequence");
    assert_eq!(first.2.op(), Some(held_op), "its own op id");
    assert!(j.e.with_industry(|_, l| l.resent) >= 1);
    j.mc_tick();
    j.mc_tick();
    assert!(
        j.mc().applied_ops().contains(&held_op),
        "applied in the world"
    );
    assert_eq!(
        j.ind_commands
            .iter()
            .filter(|c| c.2.op() == Some(held_op))
            .count(),
        1,
        "written once in this session"
    );
    iron_run_to_end(&mut j, &cfg);
    conserves(&j);
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert_eq!(j.stale_commands, 0);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// The pages stop at day 2.5 (a save that landed); the world saves later, at day 3 + 100, after the
/// reconcile was applied and its result journaled, and the page commit for that save never lands.
/// The journal then replays 2.5 to 3 + 100 into the engine: with its session start held until the
/// replay is in (W19), the engine re-derives the same reconcile under the same op ids, takes the
/// replayed result for it, and the books close. (Review finding 1: run first, the session start
/// carried the engine to day 3 + 100 and the replay re-derived a different reconcile.)
#[test]
fn pages_behind_the_world_save_rederive_the_same_reconcile() {
    let (dir, mut j) = mill_engine("ind-behind");
    let _ids = mill(
        &mut j,
        20_000,
        &[(0.0, true), (1.0, false), (3.0, true)],
        false,
    );
    j.mc_run_until(day(2.5));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    j.mc_run_until(day(3.0) + 100);
    assert!(!reconciles(&j).is_empty(), "applied before the world save");
    let ops = |j: &Java| -> Vec<u64> {
        let mut v: Vec<u64> = j
            .ind_commands
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::ChunkReconcile(r) => Some(r.op.0),
                _ => None,
            })
            .collect();
        v.sort();
        v
    };
    let first_ops = ops(&j);
    let jsave = j.world_save();
    let msave = j.mc().saved();
    assert_eq!(j.e.durable_obs_seq(), through, "no later page save");
    j.mc_run_until(day(3.0) + 600);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let applied_before = mc.reconciled.len();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    j.mc_run_until(day(3.0) + 2_000);
    let after: Vec<_> = reconciles(&j)[applied_before..].to_vec();
    assert!(after.is_empty(), "nothing applied again: {after:?}");
    let pending = industry(&j.e, |ie| ie.district(MILL).map(|d| d.pending.len()));
    assert_eq!(
        pending,
        Some(0),
        "the replayed results answered the re-derived reconcile"
    );
    eprintln!("first ops {first_ops:x?}, re-derived {:x?}", ops(&j));
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// A carriage delta answered failed (its train gone) is taken back from the engine's exchange
/// record, with or without a crash after the answer. With the crash the restart trims the
/// outbox by the world save's lastAppliedCmdSeq, which already counts the failed command, so the
/// replayed answer finds no outbox copy (review finding 2).
fn failed_carriage(crash: bool) {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-failed");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    j.fail_next_carriage = true;
    while j.failed_carriage.is_none() {
        j.mc_tick();
        assert!(j.mc().t < day(5.0), "a station exchange");
    }
    let (seq, _op, through) = j.failed_carriage.unwrap();
    mc_tick_until_durable(&mut j, through);
    assert_eq!(j.e.durable_obs_seq(), through);
    if crash {
        // The world saves now: its journal holds the FAILED answer, lastAppliedCmdSeq >= seq.
        let jsave = j.world_save();
        assert!(jsave.last_applied_cmd >= seq);
        let msave = j.mc().saved();
        let mut mc = j.mc.take().unwrap();
        j.e.close(false, Duration::from_secs(5)).unwrap();
        mc.restore(&msave);
        let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
        j = Java::open(e, jsave);
        j.mc = Some(mc);
        j.module_mask = MODULE_INDUSTRY;
    }
    iron_run_to_end(&mut j, &cfg);
    assert!(j.e.with_industry(|_, l| l.results_failed) >= 1);
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_failed_carriage_delta_is_taken_back_without_a_crash() {
    failed_carriage(false);
}

#[test]
fn a_failed_carriage_delta_is_taken_back_after_a_crash() {
    failed_carriage(true);
}

/// Industry pages another build wrote in another format (no backward compatibility before
/// release, W23) are dropped with an error and the industry engine starts afresh; the engine as a
/// whole still starts, and the next save replaces them.
#[test]
fn industry_pages_in_another_format_start_the_industry_engine_afresh() {
    use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
    let dir = tmpdir("ind-format");
    {
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey::new(0x4900, 0), b"not a globals page");
        b.put(PageKey::new(0x4901, 7), b"nor a district");
        b.put(rtsim_jni::engine::KEY_INDUSTRY_LINK, b"nor a link page");
        store.commit(b, 0).unwrap();
    }
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e, WorldSave::fresh());
    j.idle();
    assert_eq!(j.e.with_industry(|ie, _| ie.districts().count()), 0);
    j.e.close(true, Duration::from_secs(5)).unwrap();
    let store = PageStore::open(&dir, Durability::NoSync).unwrap();
    assert!(
        !store.contains(PageKey::new(0x4901, 7)),
        "the stale district page is gone"
    );
    drop(store);
    // And the pages written now restore.
    let e = engine(&dir, 256 * 1024);
    let _j = Java::open(e.clone(), WorldSave::fresh());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The conflict engine raids a district through the industry engine's threat interface (choices
/// C2-1 and C2-7 in docs/overnight-choices-conflictjni.md): a goblin camp near the mill, whose
/// smog (the Destroy plugin's attribution, I14, a deposit good the industry engine learns as an
/// output) is the settlers' grievance (P14) once the model is learned; the ladder climbs while the
/// player is away (an envoy nobody meets, scouts, a raid), and the war band meets the unloaded
/// mill abstractly, carrying and breaking by its own units' loads. The world and the pages save
/// while away, after the raid; the mill loads again and the raid's theft and broken blocks land in
/// its reconcile; then a crash: the restarted engine derives the same reconcile from the same
/// pages and the world that never saw it gets it once.
#[test]
fn a_war_band_raids_a_district_while_away_and_lands_once_across_a_crash() {
    use rtsim_conflict::boundary::{self as cb, Obs as O};
    use rtsim_jni::engine::MODULE_CONFLICT;

    const CAMP: u64 = 0xC0B1_0002;
    const SMOG: &str = "d/0/destroy:smog/00c0ffee00c0ffee";
    // The mill's chunks are (0, 0) and (2, 0): its centre is (24, 8); the camp 145 blocks off.
    const AT: [f64; 3] = [24.5, 64.0, 8.5];
    const MODULES: u32 = 0b1111 | MODULE_INDUSTRY | MODULE_CONFLICT;
    const BACK: f64 = 8.0;
    let player = (0xA11C_E000_0000_0001u64, 0xB0B0_0000_0000_0002u64);
    let visits: &[(f64, bool)] = &[(0.0, true), (1.0, false), (BACK, true)];
    let at_mill = |j: &mut Java, here: bool| {
        j.players = if here {
            vec![(player, DIM, AT)]
        } else {
            Vec::new()
        };
    };
    let (dir, mut j) = mill_engine("ind-conflict");
    let mut smog = Good(0);
    let ids = mill_with(&mut j, 20_000, visits, false, |j, w, d| {
        // One smog a batch: 240 a day while the mill runs.
        smog = Good(j.dict_id(DICT_GOOD, SMOG));
        w.good(smog.0, GoodInfo::deposit(&SMOG[4..]));
        d.deposit = Some((smog, 0));
        d.deposit_chunk = ChunkPos {
            dim: DIM,
            x: 0,
            z: 0,
        };
        d.recipes[0].deposit_per_batch = -1;
    });
    j.module_mask = MODULES;
    let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
    at_mill(&mut j, true);
    j.conflict_obs(&O::FactionFound(cb::FactionFound {
        faction: CAMP,
        flags: 0,
        dimension: DIM,
        structure,
        pos: (150, 64, 80),
        home_bodies: 0,
        census: 0,
    }));
    j.mc_run_until(Tick(day(1.0).0 - 1));
    let phase = industry(&j.e, |ie| ie.district(MILL).map(|d| d.phase));
    assert_eq!(phase, Some(Phase::Learned), "learned while loaded");
    let fed = |j: &Java| {
        j.e.with_conflict(|c, _| {
            c.sources
                .get(&(MILL.0, rtsim_conflict::SignalKind::Smog))
                .map(|s| s.rate * TICKS_PER_DAY as f64)
        })
    };
    let rate = fed(&j).expect("the mill's learned smog is a source");
    assert!((rate - 240.0).abs() < 5.0, "240 smog a day: {rate}");
    // Away: the replay runs and so does the smog; the ladder climbs with nobody to meet it.
    at_mill(&mut j, false);
    j.mc_run_until(day(BACK - 0.5));
    let (raids, blocks, unmapped) = j.e.with_conflict(|c, _| {
        (
            c.stats.raids_district,
            c.stats.blocks_destroyed,
            c.ledger.unmapped.values().sum::<i64>(),
        )
    });
    let stolen = industry(&j.e, |ie| ie.books.stolen.total());
    let log: Vec<String> = j
        .conflict
        .iter()
        .map(|(s, c)| format!("{s} {}", c.name()))
        .collect();
    eprintln!(
        "conflict while away: {raids} raids, {stolen} items stolen ({unmapped} goods the world lacks), {blocks} blocks; {log:?}"
    );
    assert!(raids >= 1, "a war band raided the mill while it was away");
    assert!(
        blocks > 0 && stolen > 0,
        "the band's own loads carry and break (C2-1)"
    );
    assert_eq!(
        i128::from(unmapped),
        stolen,
        "the haven counts the mill's goods it has no market good for (K2-15)"
    );
    let attacks = industry(&j.e, |ie| ie.district(MILL).map_or(0, |d| d.attacks.len()));
    assert!(attacks >= 1, "the industry engine booked the attack");
    let machine_lost = industry(&j.e, |ie| {
        ie.district(MILL)
            .is_some_and(|d| d.attacks.iter().any(|a| a.machine_lost))
    });
    if machine_lost {
        assert_eq!(
            fed(&j),
            None,
            "a machine lost: the replay stops, and the mill's smog with it"
        );
    }
    assert!(
        j.mc().districts[0].broken.is_empty(),
        "nothing happens in the world while it is away"
    );
    // The world saves while away, after the raid; the pages stop exactly there.
    let jsave = j.world_save();
    let msave = j.mc().saved();
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let gt = j.game_time;
    // The player comes back: the raid lands in the reconcile.
    at_mill(&mut j, true);
    j.mc_run_until(day(BACK) + 600);
    let first = reconciles(&j);
    let broken = j.mc().districts[0].broken.clone();
    let bars_first = bars(&j, &ids);
    eprintln!("at the load: reconciles {first:?}, broken {broken:?}");
    assert!(!broken.is_empty(), "the raid's blocks broken at the load");
    assert!(!first.is_empty(), "reconciled before the crash");
    conserves(&j);
    // The crash: no save lands, the world rolls back to its save.
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let applied_before = mc.reconciled.len();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULES;
    j.game_time = gt;
    assert_eq!(
        j.e.conflict_restore_check(),
        (None, false),
        "the conflict reads back to its saved state hash"
    );
    j.mc_run_until(day(BACK) + 600);
    let after: Vec<_> = reconciles(&j)[applied_before..].to_vec();
    let epoch = |v: &[(u64, u32, ChunkPos)]| {
        let mut x: Vec<_> = v.to_vec();
        x.sort();
        x
    };
    assert_eq!(
        epoch(&after),
        epoch(&first),
        "the same reconcile re-derived from the same pages, applied once"
    );
    assert_eq!(
        j.mc().districts[0].broken,
        broken,
        "the same blocks broken, once"
    );
    assert_eq!(bars(&j, &ids), bars_first, "the same theft, once");
    assert_eq!(
        j.e.with_conflict(|c, _| c.stats.raids_district),
        raids,
        "the raid is not raided again"
    );
    conserves(&j);
    assert_eq!(j.stale_commands, 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

// ---- saves write what changed (docs/choices-industry-dirty-pages.md) ----------------------------

/// The mill and `extra` more like it, each with its own vaults 4,096 blocks east of the one
/// before, all learned while loaded from day 0 and replaying while away from day 1.
fn mills(j: &mut Java, extra: u64) {
    mill_with(j, 20_000, &[(0.0, true), (1.0, false)], true, |_, w, d| {
        for i in 1..=extra {
            let mut c = d.clone();
            let id = DistrictId(100 + i);
            let dx = 4_096 * i as i32;
            c.id = id;
            c.name = format!("Mill {i}");
            for inv in &mut c.inventories {
                inv.pos.x += dx;
            }
            for b in &mut c.exposed {
                b.pos.x += dx;
            }
            for r in &mut c.recipes {
                r.machine.x += dx;
            }
            c.survey = Some(survey(id, 0, 0, 0, 0, 1, [0.0; 4]));
            w.districts.push(c);
        }
    });
    let all: Vec<usize> = (0..=extra as usize).collect();
    j.mc_mut().visits = vec![
        Visit {
            from: day(0.0),
            at: all,
        },
        Visit {
            from: day(1.0),
            at: vec![],
        },
    ];
}

/// One save, taken at the next tick's step; the world waits (no step) until it is durable, so the
/// engine still holds what it saved. The industry pages it wrote, their bytes, and those it
/// deleted.
fn industry_save(j: &mut Java) -> (u64, u64, u64) {
    industry_save_tries(j, 1)
}

/// [`industry_save`] straight after a save that failed. A failed commit counts its failure
/// (`stats_array()[15]`) a moment before it hands its tickets back, and a save the step builds in
/// that moment is stale: it is not made, it counts as a failure too, and the next save is the one
/// that writes the pages again (PF14, `paging::Job::stale`). A caller that saw the first failure
/// cannot tell which side of that moment its next save falls on, so a save that is answered with
/// a failure is asked for again, up to three times; the one that is made is the one returned.
fn industry_save_after_failure(j: &mut Java) -> (u64, u64, u64) {
    industry_save_tries(j, 3)
}

fn industry_save_tries(j: &mut Java, tries: u32) -> (u64, u64, u64) {
    use std::sync::atomic::Ordering::Acquire;
    let done = j.e.stats.saves_done.load(Acquire);
    'attempt: for attempt in 1..=tries {
        let failed = j.e.stats_array()[15];
        j.e.order(vec![Order::SaveAtNextStep]).unwrap();
        j.mc_tick();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if j.e.stats.saves_done.load(Acquire) != done {
                break 'attempt;
            }
            if j.e.stats_array()[15] != failed {
                // This save was answered with a failure, not a commit.
                assert!(
                    attempt < tries,
                    "save {attempt} of {tries} failed too (failed saves {failed} then {})",
                    j.e.stats_array()[15]
                );
                continue 'attempt;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the save never committed (failed saves {failed} then {})",
                j.e.stats_array()[15]
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    j.e.with_industry(|_, s| (s.save_pages, s.save_bytes, s.save_deleted))
}

/// The industry pages the store in `dir` holds, by key (the engine closed).
fn stored_industry_pages(dir: &std::path::Path) -> Vec<(u64, Vec<u8>)> {
    use rtsim_kernel::page::{Durability, PageKey, PageStore};
    let mut store = PageStore::open(dir, Durability::NoSync).unwrap();
    let mut out = Vec::new();
    for table in rtsim_jni::engine::INDUSTRY_TABLES {
        for id in store.table_ids(table).unwrap() {
            let k = PageKey::new(table, id);
            out.push((k.0, store.get(k).unwrap().expect("listed")));
        }
    }
    out.sort();
    out
}

fn industry_pages(e: &Engine) -> Vec<(u64, Vec<u8>)> {
    let mut v: Vec<(u64, Vec<u8>)> = industry(e, |ie| {
        ie.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect()
    });
    v.sort();
    v
}

/// Industry saves write only the pages changed since the save before: with every mill replaying
/// while away, a save 400 ticks after the one before writes the globals page and the link's and
/// no district's, at 1 mill as at 16; the first save after the unload wrote every mill's.
#[test]
fn a_quiet_industry_save_is_flat_in_the_district_count() {
    let quiet = |extra: u64| {
        let (_dir, mut j) = mill_engine("ind-quiet");
        mills(&mut j, extra);
        j.mc_run_until(day(1.5));
        let first = industry_save(&mut j);
        let replaying = industry(&j.e, |ie| {
            ie.districts().filter(|d| d.sim.is_some()).count()
        });
        assert_eq!(replaying as u64, 1 + extra, "every mill replays");
        let t = j.mc().t;
        j.mc_run_until(t + 400);
        let quiet = industry_save(&mut j);
        assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
        j.e.close(true, Duration::from_secs(5)).unwrap();
        (first, quiet)
    };
    let ((first_1, quiet_1), (first_16, quiet_16)) = (quiet(0), quiet(15));
    eprintln!(
        "industry saves (pages, bytes, deleted): 1 mill {first_1:?} then {quiet_1:?}; 16 mills {first_16:?} then {quiet_16:?}"
    );
    assert!(
        first_16.0 > first_1.0,
        "the first save writes every mill's pages"
    );
    assert_eq!(quiet_1.0, 2, "the globals page and the link's");
    assert_eq!(quiet_16.0, 2, "the globals page and the link's");
    assert_eq!((quiet_1.2, quiet_16.2), (0, 0));
}

/// A district Java removes leaves the store with the next save: its pages are deleted in that
/// commit (no index read: the engine marked them), and the engine started again from the store
/// has no trace of it.
#[test]
fn a_removed_district_leaves_the_store_with_the_next_save() {
    use rtsim_industry::persist::{DISTRICT_TABLE, GLASS_TABLE};
    use rtsim_kernel::page::{Durability, PageKey, PageStore};
    let (dir, mut j) = mill_engine("ind-removed");
    mills(&mut j, 1);
    j.mc_run_until(day(1.5));
    industry_save(&mut j);
    let other = DistrictId(101);
    let t = j.mc().t;
    j.industry_obs(&Timed {
        at: t,
        obs: Obs::DistrictRemoved { district: other },
    });
    let (_, _, deleted) = industry_save(&mut j);
    assert!(deleted >= 1, "its pages are deleted");
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(false, Duration::from_secs(5)).unwrap();
    {
        let store = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert!(
            store.contains(PageKey::new(DISTRICT_TABLE, MILL.0)),
            "the mill stays"
        );
        assert!(!store.contains(PageKey::new(DISTRICT_TABLE, other.0)));
        assert!(!store.contains(PageKey::new(GLASS_TABLE, other.0)));
    }
    let e = engine(&dir, 256 * 1024);
    let j = Java::open(e.clone(), j.world_save());
    assert_eq!(
        industry(&e, |ie| ie.districts().map(|d| d.id).collect::<Vec<_>>()),
        vec![MILL]
    );
    drop(j);
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A save whose write fails hands its industry pages back: the next save writes them again, and
/// the store then holds the engine that saved, page for page.
#[test]
fn a_failed_industry_save_is_written_again_by_the_next() {
    use rtsim_kernel::page::Fault;
    let (dir, mut j) = mill_engine("ind-failed");
    mills(&mut j, 3);
    j.mc_run_until(day(0.9));
    industry_save(&mut j);
    // The unload: every mill's pages change, in a save that fails.
    j.mc_run_until(day(1.2));
    let failures = j.e.stats_array()[15];
    j.e.inject_store_fault(Some(Fault::TornLog));
    j.e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.mc_tick();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while j.e.stats_array()[15] == failures {
        assert!(
            std::time::Instant::now() < deadline,
            "the save never failed"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let lost = j.e.with_industry(|_, s| s.save_pages);
    assert!(lost > 2, "the failed save had district pages: {lost}");
    // A tick from the failed save to the next: it writes the lost pages again (the first save the
    // step builds may be a stale one, which is not made).
    let (pages, _, _) = industry_save_after_failure(&mut j);
    assert!(
        pages >= lost,
        "the next save writes the lost pages again: {pages} of {lost}"
    );
    let saved = industry_pages(&j.e);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(false, Duration::from_secs(5)).unwrap();
    assert!(
        stored_industry_pages(&dir) == saved,
        "the store holds the engine that saved"
    );
}

/// A `CHAIN_IN` that an IND_WRITE_CHAIN wrote carries the engine's op (SECTIONS.md §2.5, SC113):
/// the frogport's chunk carries it with the COMMAND_RESULT and a later session journals both
/// again, so `Replay` takes it once by that op, and the COMMAND_RESULT after it is an answer heard
/// again. A `CHAIN_IN` Create moved carries no op and is taken every time.
#[test]
fn a_written_chain_package_journaled_again_is_taken_once() {
    use rtsim_industry::ids::StoreId;
    use rtsim_industry::obs::{ChainDirection, ChainPackage, StoreDecl, StoreFrogport};
    let (_dir, mut j) = mill_engine("ind-chain-once");
    mills(&mut j, 0);
    j.mc_run_until(Tick(100));
    let store = StoreId(7);
    let port = |x: i32| StoreFrogport {
        district: None,
        inventory: None,
        star: false,
        pos: BlockPos::new(x, 64, 0),
        conveyor: BlockPos::new(x + 1, 64, 0),
        from: None,
    };
    let at = j.mc().t;
    j.industry_obs(&Timed {
        at,
        obs: Obs::PackageStore(StoreDecl {
            store,
            dim: 0,
            epoch: 0,
            into: None,
            frogports: vec![port(9_000), port(9_020)],
            hops: vec![1],
        }),
    });
    let op = 0x49u64 << 56 | 0x5eed;
    let chain_in = |op: Option<u64>| Timed {
        at,
        obs: Obs::ChainPackage(ChainPackage {
            store,
            district: None,
            inventory: None,
            direction: ChainDirection::In,
            frogport: 1,
            pos: BlockPos::new(9_020, 64, 0),
            destination: Some(1),
            packages: 2,
            address: 0,
            op: op.map(rtsim_kernel::journal::OpId),
            key: None,
            goods: GoodVec::from_pairs([(Good(1), 128)]),
        }),
    };
    // Written, its COMMAND_RESULT, then both again (a chunk's pending records at its load).
    let written_and_answered = |j: &mut Java| {
        j.industry_obs(&chain_in(Some(op)));
        j.journal(
            wire::obs::COMMAND_RESULT,
            wire::obs::command_result::SIZE,
            wire::obs::command_result::APPLIED,
            |r| {
                wire::put_u64(r, wire::obs::command_result::CMD_SEQ, 0);
                wire::put_u64(r, wire::obs::command_result::OP_ID, op);
            },
        );
    };
    written_and_answered(&mut j);
    written_and_answered(&mut j);
    // Two that Create moved: no op, both taken.
    j.industry_obs(&chain_in(None));
    j.industry_obs(&chain_in(None));
    j.mc_run_until(Tick(at.0 + 40));
    // A durable world save lets the replay forget the op; a chunk that unloaded before the
    // acknowledgement still carries both records and journals them again at its next load
    // (review finding 1): the store's mark takes the record once all the same.
    let durable = j.next_obs - 1;
    {
        let r = j
            .reserve(wire::obs::WORLD_SAVED, wire::obs::world_saved::SIZE, 0)
            .expect("room");
        wire::put_u64(r, wire::obs::world_saved::DURABLE_OBS_SEQ, durable);
    }
    j.mc_run_until(Tick(at.0 + 80));
    written_and_answered(&mut j);
    j.mc_run_until(Tick(at.0 + 120));
    let (written, moved, again) = industry(&j.e, |ie| {
        let s = &ie.stores()[&store];
        (
            s.counts.written,
            s.counts.packages_in,
            ie.stats.chain_writes_again,
        )
    });
    assert_eq!(written, 2, "the written packages are taken once");
    assert_eq!(moved, 4, "packages Create moved are taken every time");
    assert_eq!(again, 1, "the third copy came after the op was forgotten");
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}
