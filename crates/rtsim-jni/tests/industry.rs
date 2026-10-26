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

/// Ticks the world, a lockstep each, until the engine has taken the save asked for through
/// `through` (`request_save`), then waits for its pages to be durable without ticking (P3-1 of
/// the slice R review, RP-259). The save is taken in the step that applies `through`: right before
/// the first journaled record past it, or at the step's end. Ticking until it was durable ran as
/// many world ticks as the IO lane took (1 to 67 measured, more alone than under parallel load),
/// so the crash tests' worlds, journals and replays moved with the load. Panics after 60 s of
/// wall clock instead of spinning for ever: a hung test binary holds the shared cargo lock.
fn mc_tick_until_durable(j: &mut Java, through: u64) {
    use std::sync::atomic::Ordering::Acquire;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        j.mc_tick();
        if j.e.stats.applied_through.load(Acquire) >= through {
            break;
        }
    }
    while j.e.durable_obs_seq() < through {
        assert!(
            std::time::Instant::now() < deadline,
            "the pages never became durable: waited 60 s for durable_obs_seq to reach the \
             requested save; durable_obs_seq {}, requested through {through}",
            j.e.durable_obs_seq()
        );
        std::thread::sleep(Duration::from_millis(1));
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

// ---- the crash tests' shape (slice R, docs/choices-mowzie-build.md RP-276) -----------------------

/// Journaled records every industry crash test puts right after its pages (ship damage reports
/// the engine ignores, [`fillers`]): restarted on a minimum-size buffer, the replay then spans
/// several hand-overs, as a long replay in play does, so the test meets what only such a replay
/// does (RP-263: on bc669502 the first replayed hand-over ran the industry engine to the
/// present). About 900 fill one buffer.
const REPLAY_FILLERS: usize = 2_000;

/// The engine started again after a crash, as Java starts it: on a minimum-size buffer (the
/// caller's `e`), from the world's save, at the world's present (`Java::open` starts the game
/// time at 1,000, below every replayed record: RP-269).
fn restart(e: std::sync::Arc<Engine>, jsave: WorldSave, mc: FakeWorld, modules: u32) -> Java {
    let present = mc.t;
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = modules;
    j.game_time = present.0 as i64;
    j
}

/// The hand-overs the session's replay took, which must be several (RP-276). A failure names its
/// path (RP-259): 0 hand-overs with a replay end is a replay that ended at SESSION_START (the
/// pages or Java's journal reached the world save); with none and an engine error, the restarted
/// engine stopped before its first step published its counters.
fn replayed_over_several(j: &Java) -> u64 {
    use std::sync::atomic::Ordering::Acquire;
    let s = &j.e.stats;
    let n = s.replay_handovers.load(Acquire);
    assert!(
        n >= 2,
        "the replay spans hand-overs: {n} ({} replay ends, {} records replayed, pages through {}, \
         journal from {} to {}, engine error {:?})",
        s.replays_ended.load(Acquire),
        s.replay_records.load(Acquire),
        j.pages_obs,
        j.journal_first_at_start,
        j.world_next_at_start,
        j.e.protocol_error()
    );
    n
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
    // The engine is asked for pages that stop here; the world saves after the fillers, so the
    // replay spans hand-overs (RP-276).
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    assert_eq!(j.e.durable_obs_seq(), through, "the pages stop here");
    fillers(&mut j, REPLAY_FILLERS);
    j.mc_tick();
    let jsave = j.world_save();
    let msave = j.mc().saved();
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
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    // To the moment of the crash: a world that never saw what was lost runs it again.
    j.mc_run_until(crash_at);
    replayed_over_several(&j);
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
    iron_java_with(dir, cfg, 256 * 1024, |_| {})
}

/// [`iron_java`] with `capacity` bytes of record area and the engine's configuration adjusted
/// first.
fn iron_java_with(
    dir: &std::path::Path,
    cfg: IronLine,
    capacity: usize,
    f: impl FnOnce(&mut rtsim_jni::engine::Config),
) -> Java {
    let e = engine_with(&dir.to_path_buf(), capacity, |c| {
        c.industry_seed = Some(cfg.seed);
        f(c)
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
    // The pages stop here: their outbox holds the delta. The world saves after the fillers (it
    // never applied the delta), so the replay spans hand-overs (RP-276).
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    assert!(
        j.e.with_industry(|_, l| l.outbox) >= 1,
        "the delta waits in the outbox"
    );
    fillers(&mut j, REPLAY_FILLERS);
    j.mc_tick();
    let jsave = j.world_save();
    let msave = j.mc().saved();
    assert!(jsave.last_applied_cmd < held_seq);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    // The start counts every module's resends for Java's perf line (RP-205).
    assert!(
        j.e.stats_array()[16] >= 1,
        "the start counts the kept delta among its resends"
    );
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
    replayed_over_several(&j);
    conserves(&j);
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert_eq!(j.stale_commands, 0);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// The pages stop at day 2.5 (a save that landed); the world saves later, at day 3 + 100, after the
/// reconcile was applied and its result journaled, and the page commit for that save never lands.
/// The journal then replays 2.5 to 3 + 100 into the engine, over several hand-overs (RP-276): with
/// its session start and its runs of the clock held until the replay is in (W19, RP-274), the
/// engine re-derives the same reconcile under the same op ids, takes the replayed result for it,
/// and the books close. (Review finding 1: run first, the session start
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
    fillers(&mut j, REPLAY_FILLERS);
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
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    j.mc_run_until(day(3.0) + 2_000);
    replayed_over_several(&j);
    let after: Vec<_> = reconciles(&j)[applied_before..].to_vec();
    assert!(after.is_empty(), "nothing applied again: {after:?}");
    let pending = industry(&j.e, |ie| ie.district(MILL).map(|d| d.pending.len()));
    assert_eq!(
        pending,
        Some(0),
        "the replayed results answered the re-derived reconcile"
    );
    assert_eq!(
        ops(&j),
        first_ops,
        "the same reconciles under the same op ids"
    );
    // The replay heard them answered APPLIED, and writes them again all the same: Java answers
    // DUPLICATE (RP-259 drops only an op answered FAILED, the lead's narrowing).
    let unsent = j.e.with_industry(|_, s| s.answered_unsent);
    assert_eq!(unsent, 0, "an op answered APPLIED is written again");
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
        // The world saves after the fillers (RP-276): its journal holds the FAILED answer,
        // lastAppliedCmdSeq >= seq.
        fillers(&mut j, REPLAY_FILLERS);
        j.mc_tick();
        let jsave = j.world_save();
        assert!(jsave.last_applied_cmd >= seq);
        let msave = j.mc().saved();
        let mut mc = j.mc.take().unwrap();
        j.e.close(false, Duration::from_secs(5)).unwrap();
        mc.restore(&msave);
        let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
            c.industry_seed = Some(cfg.seed)
        });
        j = restart(e, jsave, mc, MODULE_INDUSTRY);
    }
    iron_run_to_end(&mut j, &cfg);
    if crash {
        replayed_over_several(&j);
    }
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
/// mill abstractly, carrying and breaking by its own units' loads. The pages save while away,
/// after the raid, and the world after a replay's worth of fillers (RP-276); the mill loads again
/// and the raid's theft and broken blocks land in its reconcile; then a crash: the restarted
/// engine replays over several hand-overs, derives the same reconcile from the same pages, and
/// the world that never saw it gets it once.
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
    // The pages stop while away, after the raid; the world saves after the fillers, so the
    // replay spans hand-overs (RP-276).
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    fillers(&mut j, REPLAY_FILLERS);
    j.mc_tick();
    let jsave = j.world_save();
    let msave = j.mc().saved();
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
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = restart(e, jsave, mc, MODULES);
    assert_eq!(
        j.e.conflict_restore_check(),
        (None, false),
        "the conflict reads back to its saved state hash"
    );
    j.mc_run_until(day(BACK) + 600);
    replayed_over_several(&j);
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

// ---- slice R: the op ids a replay derives (RP-260 to RP-269) --------------------------------

/// Filler a replay must read through: `count` damage reports of a ship the engine never had,
/// which change nothing (about 56 bytes each: about a thousand fill a minimum-size buffer).
fn fillers(j: &mut Java, count: usize) {
    use rtsim_ships::boundary::{self as sb, By, Obs as O};
    for _ in 0..count {
        j.ship_obs(&O::Damaged(sb::Damaged {
            agent: 0,
            epoch: 0,
            by: By::Other,
            hull_milli: 1000,
            crew_lost: 0,
            player: (0, 0),
            ship: 0,
        }));
    }
}

/// What a crash after the pages did to the op ids.
struct Behind {
    /// The reconciles' op ids as Java was sent them before the crash, and again after it.
    before: Vec<u64>,
    after: Vec<u64>,
    /// The hand-overs the replay took.
    replay_handovers: u64,
    /// Reconciles the world applied again after the crash.
    applied_again: usize,
    /// The district's reconciles still waiting for an answer after the replay.
    pending: Option<usize>,
    /// Commands the replay decided again under an op Java answered FAILED, not written (RP-259).
    unsent: u64,
}

/// [`pages_behind_the_world_save_rederive_the_same_reconcile`] with `filler` journaled records
/// right after the pages (so the replay of a small engine spans hand-overs) and the engine
/// reopened with `capacity` bytes of record area, at the game time `present` (the harness's
/// `Java::open` resets it to 1000, below every replayed record, which hides the case).
fn behind_the_save(tag: &str, filler: usize, capacity: usize, present: Tick) -> Behind {
    let (dir, mut j) = mill_engine(tag);
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
    j.mc_run_until(day(2.6));
    fillers(&mut j, filler);
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
    let before = ops(&j);
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(day(3.0) + 600);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let applied_before = mc.reconciled.len();
    let e = engine(&dir, capacity);
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    j.game_time = present.0 as i64;
    j.mc_run_until(day(3.0) + 2_000);
    let replay_handovers =
        j.e.stats
            .replay_handovers
            .load(std::sync::atomic::Ordering::Acquire);
    let applied_again = reconciles(&j)[applied_before..].len();
    let pending = industry(&j.e, |ie| ie.district(MILL).map(|d| d.pending.len()));
    let after = ops(&j);
    let unsent = j.e.with_industry(|_, s| s.answered_unsent);
    eprintln!(
        "{tag}: {replay_handovers} replay hand-overs; first ops {before:x?}, sent after the restart {after:x?}, {unsent} known failed and not sent; applied again {applied_again}, pending {pending:?}"
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    Behind {
        before,
        after,
        replay_handovers,
        applied_again,
        pending,
        unsent,
    }
}

/// The control: a replay of one hand-over, the engine reopened at the world's present. The
/// journal is read whole before `simulate` runs the engine to the present and starts its session,
/// so every record is taken at its own tick, as live: the same reconciles under the same op ids,
/// applied once, answered by the replayed results. Holds.
#[test]
fn a_one_hand_over_replay_rederives_the_same_op_ids() {
    let b = behind_the_save("ind-ids-one", 0, 256 * 1024, day(3.0) + 101);
    assert!(!b.before.is_empty(), "the reconcile ran before the crash");
    assert_eq!(b.replay_handovers, 1, "a replay of one hand-over");
    assert_eq!(b.after, b.before, "the same reconciles, the same op ids");
    // Heard answered APPLIED while replaying and written again: Java answers DUPLICATE (RP-259).
    assert_eq!(b.unsent, 0, "an op answered APPLIED is written again");
    assert_eq!(b.applied_again, 0, "nothing applied again");
    assert_eq!(b.pending, Some(0), "the replayed results answer them");
}

/// A replay of several hand-overs (8,000 filler records after the pages, a minimum-size buffer).
/// On bc669502 the industry engine was not held while replaying, so the first replayed
/// hand-over's `simulate` started the engine's session and ran it to Java's present
/// (`advance_to`) before the later hand-overs' records arrived; those were then taken at
/// `max(t.at, present)` and the day-3 load's reconcile was re-derived at the present with another
/// content and another op id (RP-263). Held while replaying (RP-274), every replayed record is
/// taken at its own tick: the same reconciles under the same op ids.
#[test]
fn a_replay_of_several_hand_overs_rederives_the_same_op_ids() {
    let b = behind_the_save(
        "ind-ids-many",
        8_000,
        rtsim_jni::wire::MIN_CAPACITY,
        day(3.0) + 101,
    );
    assert!(!b.before.is_empty(), "the reconcile ran before the crash");
    // The long replay of the industry crash tests (RP-276): nine hand-overs.
    assert!(
        b.replay_handovers >= 9,
        "the replay spans nine hand-overs: {}",
        b.replay_handovers
    );
    assert_eq!(
        b.after, b.before,
        "the same reconciles under the same op ids (replay hand-overs {})",
        b.replay_handovers
    );
    assert_eq!(b.unsent, 0, "an op answered APPLIED is written again");
    assert_eq!(b.applied_again, 0, "nothing applied again");
    assert_eq!(b.pending, Some(0), "the replayed results answer them");
}

/// What a crash after a station exchange did to the trains.
struct ExchangeBehind {
    /// The exchanges' op ids as Java was sent them before the crash, and again after it.
    before: Vec<u64>,
    after: Vec<u64>,
    /// The hand-overs the replay took.
    replay_handovers: u64,
    /// Station exchanges the world applied after the restart (it applied every one of them
    /// before its save already, and no train calls at a replaying station after the save).
    applied_again: usize,
    /// Every train's cargo at the crash, live and again after the restart.
    cargo_live: Vec<Vec<GoodVec>>,
    cargo_again: Vec<Vec<GoodVec>>,
    /// Commands the replay decided again under an op Java answered FAILED, not written (RP-259).
    unsent: u64,
}

fn cargo(j: &Java) -> Vec<Vec<GoodVec>> {
    j.mc()
        .trains
        .iter()
        .map(|t| t.carriages.iter().map(|c| c.cargo.clone()).collect())
        .collect()
}

/// The iron line's train 1 loads ore at the replaying ore coast at 72,800 (a station exchange,
/// IND_CARRIAGE_DELTA). The pages stop at day 3 (72,000); `filler` journaled records follow at
/// 72,100; the world applies the exchange, journals its result and saves at 72,900; the crash
/// comes at 73,500, before any other exchange. The engine reopens with `capacity` bytes of record
/// area and the world, restored to its save, runs to the crash again: the replay re-decides the
/// exchange, and the world, which applied it before its save, must answer it DUPLICATE.
fn exchange_behind_the_save(tag: &str, filler: usize, capacity: usize) -> ExchangeBehind {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir(tag);
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let paged = j.ind_commands.len();
    j.mc_run_until(Tick(72_100));
    fillers(&mut j, filler);
    j.mc_run_until(Tick(72_900));
    let exchanges = |j: &Java, from: usize| -> Vec<u64> {
        j.ind_commands[from..]
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) => Some(d.op.0),
                _ => None,
            })
            .collect()
    };
    let before: Vec<u64> = exchanges(&j, paged)
        .into_iter()
        .filter(|op| {
            j.mc()
                .applied_ops()
                .contains(&rtsim_kernel::journal::OpId(*op))
        })
        .collect();
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(Tick(73_500));
    let cargo_live = cargo(&j);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let saved_ops = mc.applied_ops().clone();
    let e = engine_with(&dir, capacity, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    j.mc_run_until(Tick(73_500));
    let cargo_again = cargo(&j);
    let replay_handovers =
        j.e.stats
            .replay_handovers
            .load(std::sync::atomic::Ordering::Acquire);
    let after = exchanges(&j, 0);
    let applied_again = after
        .iter()
        .filter(|op| {
            let op = rtsim_kernel::journal::OpId(**op);
            j.mc().applied_ops().contains(&op) && !saved_ops.contains(&op)
        })
        .count();
    let unsent = j.e.with_industry(|_, s| s.answered_unsent);
    eprintln!(
        "{tag}: {replay_handovers} replay hand-overs; exchanges before the save {before:x?}, \
         sent after the restart {after:x?} ({unsent} known failed, not sent); applied again \
         {applied_again}; cargo at the crash {cargo_live:?}, again {cargo_again:?}"
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    ExchangeBehind {
        before,
        after,
        replay_handovers,
        applied_again,
        cargo_live,
        cargo_again,
        unsent,
    }
}

/// Candidate (e) across a crash (RP-262, RP-278): the train's call reaches the live engine late.
/// The live engine runs on a minimum-size buffer and 3,000 filler records are journaled the tick
/// before train 1 calls at the ore coast, so the call waits in Java's journal for the hand-overs
/// the fillers fill (Java's `drainInto` leaves what does not fit for the next one), and the live
/// engine, run to each hand-over's time, takes it at a later game time than its own. The pages
/// stop at day 3, the world saves at 72,900 after the exchange and the crash comes at 73,500, as
/// in [`exchange_behind_the_save`]; the engine restarts on 1 MiB, so its replay is one hand-over
/// (candidate f stays out) and takes the call at its own tick. Returns the call's own game time
/// and the hand-over time the live engine took it at.
fn stale_exchange_behind_the_save(tag: &str) -> (ExchangeBehind, i64, i64) {
    use rtsim_industry::wire::IND_TRAIN_ARRIVED;
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir(tag);
    let mut j = iron_java_with(&dir, cfg, wire::MIN_CAPACITY, |_| {});
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let paged = j.ind_commands.len();
    j.mc_run_until(Tick(72_799));
    fillers(&mut j, 3_000);
    let behind = j.next_obs;
    // The call, journaled behind the fillers: its own game time, and the time of the hand-over
    // that carried it (the live engine's clock had reached the hand-over before's).
    let mut call: Option<(u64, i64)> = None;
    let mut taken = None;
    while j.mc().t < Tick(72_900) {
        j.mc_tick();
        if call.is_none() {
            call = j
                .journal
                .iter()
                .find(|(s, r)| *s >= behind && wire::u16_at(r, wire::R_KIND) == IND_TRAIN_ARRIVED)
                .map(|(s, r)| (*s, wire::i64_at(r, wire::obs::GAME_TIME)));
        }
        if let Some((seq, _)) = call
            && taken.is_none()
            && j.sent_obs >= seq
        {
            taken = Some(j.game_time);
        }
    }
    let (_, stamped) = call.expect("train 1 called after the fillers");
    let taken = taken.expect("the call was handed over");
    let before: Vec<u64> = j.ind_commands[paged..]
        .iter()
        .filter_map(|c| match &c.2 {
            Cmd::CarriageDelta(d) => Some(d.op.0),
            _ => None,
        })
        .filter(|op| {
            j.mc()
                .applied_ops()
                .contains(&rtsim_kernel::journal::OpId(*op))
        })
        .collect();
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(Tick(73_500));
    let cargo_live = cargo(&j);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let saved_ops = mc.applied_ops().clone();
    let e = engine_with(&dir, 1 << 20, |c| c.industry_seed = Some(cfg.seed));
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    j.mc_run_until(Tick(73_500));
    let cargo_again = cargo(&j);
    let replay_handovers =
        j.e.stats
            .replay_handovers
            .load(std::sync::atomic::Ordering::Acquire);
    let after: Vec<u64> = j
        .ind_commands
        .iter()
        .filter_map(|c| match &c.2 {
            Cmd::CarriageDelta(d) => Some(d.op.0),
            _ => None,
        })
        .collect();
    let applied_again = after
        .iter()
        .filter(|op| {
            let op = rtsim_kernel::journal::OpId(**op);
            j.mc().applied_ops().contains(&op) && !saved_ops.contains(&op)
        })
        .count();
    let unsent = j.e.with_industry(|_, s| s.answered_unsent);
    eprintln!(
        "{tag}: the call stamped {stamped}, taken live at {taken}; {replay_handovers} replay \
         hand-overs; exchanges before the save {before:x?}, sent after the restart {after:x?} \
         ({unsent} known failed, not sent); applied again {applied_again}; cargo at the crash \
         {cargo_live:?}, again {cargo_again:?}"
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    (
        ExchangeBehind {
            before,
            after,
            replay_handovers,
            applied_again,
            cargo_live,
            cargo_again,
            unsent,
        },
        stamped,
        taken,
    )
}

/// The double of candidate (e) (RP-262, RP-278): live, the call is taken at the hand-over's time,
/// past its own; replayed in one hand-over, at its own. On bc669502 the exchange's op derived
/// from the engine's clock, so the replay sent another op and the world loaded the train twice.
/// Derived from the call's own tick and named by the train (RP-270, RP-272), it is the live op,
/// answered DUPLICATE. The replay heard it answered APPLIED and writes it all the same (RP-259
/// drops only a FAILED one).
#[test]
fn a_station_exchange_taken_late_live_is_applied_once_after_a_crash() {
    let (b, stamped, taken) = stale_exchange_behind_the_save("ind-exch-stale");
    assert!(
        taken >= stamped + 2,
        "the live engine's clock was past the call when it took it: stamped {stamped}, taken \
         at the hand-over of {taken}"
    );
    assert_eq!(b.before.len(), 1, "one exchange before the save");
    assert_eq!(
        b.replay_handovers, 1,
        "a replay of one hand-over: (e) alone"
    );
    assert_eq!(b.applied_again, 0, "never applied twice");
    assert_eq!(
        b.cargo_again, b.cargo_live,
        "the trains hold what they held"
    );
    assert_eq!(b.after, b.before, "re-derived under its op id");
    assert_eq!(b.unsent, 0, "an op answered APPLIED is written again");
}

/// The control: the replay of one hand-over takes the train's call at its own tick, re-derives
/// the exchange under its op id, and the world answers it DUPLICATE. Holds on bc669502. The
/// replay heard the op answered APPLIED, and writes the copy all the same (RP-259).
#[test]
fn a_station_exchange_replayed_in_one_hand_over_is_applied_once() {
    let b = exchange_behind_the_save("ind-exch-one", 0, 256 * 1024);
    assert_eq!(b.before.len(), 1, "one exchange before the save");
    assert_eq!(b.replay_handovers, 1, "a replay of one hand-over");
    assert_eq!(b.after, b.before, "re-derived under its op id");
    assert_eq!(b.unsent, 0, "an op answered APPLIED is written again");
    assert_eq!(b.applied_again, 0, "never applied twice");
    assert_eq!(
        b.cargo_again, b.cargo_live,
        "the trains hold what they held"
    );
}

/// The double (slice R step 2): over a replay of several hand-overs, the first one's `simulate`
/// runs the industry engine to the present, the train's call is taken at the present and the
/// exchange re-derived under a new op id, which Java's op-id guard (the only guard a carriage
/// delta has) does not know: the world loads the train a second time (on bc669502: 224 ore live,
/// 448 after the restart). Held while replaying (RP-274), with the exchange named by its train
/// and its call's tick (RP-270, RP-272): the same op, answered DUPLICATE, loaded once.
#[test]
fn a_station_exchange_replayed_over_several_hand_overs_is_applied_once() {
    let b = exchange_behind_the_save("ind-exch-many", 8_000, rtsim_jni::wire::MIN_CAPACITY);
    assert_eq!(b.before.len(), 1, "one exchange before the save");
    assert!(
        b.replay_handovers >= 3,
        "the replay spans hand-overs: {}",
        b.replay_handovers
    );
    assert_eq!(b.applied_again, 0, "never applied twice");
    assert_eq!(
        b.cargo_again, b.cargo_live,
        "the trains hold what they held"
    );
    assert_eq!(b.after, b.before, "re-derived under its op id");
    assert_eq!(b.unsent, 0, "an op answered APPLIED is written again");
}

/// Why the replay writes again a copy whose op it heard answered APPLIED (RP-259, the lead's
/// narrowing; RP-310): J4 (RP-196), Java's journal saved after the exchange and its tracks file
/// not (since RP-200 only a failed tracks write). The iron line's train 1 loads ore at the ore
/// coast at 72,800 as in [`exchange_behind_the_save`], over a replay of several hand-overs; the
/// world restarts from its save at 72,900 but for train 1's cargo and the op in its ring (the fake
/// world's applied set, saved with its trains), which are from the tick before it applied the
/// exchange. The replay re-derives the exchange under the live op, and the replayed
/// IND_CARRIAGE_RESULT books it. Written again, the ring that came back without the op applies it
/// to the restored train, once. Taken out at the replayed APPLIED (A's `take_out` acting on the
/// held replay, 7661ca04 to fe4e80bf), it never goes: the train holds nothing while the books say
/// it holds 224 ore.
#[test]
fn a_station_exchange_whose_trains_save_was_lost_is_applied_again_after_the_replay() {
    use rtsim_industry::ids::TrainId;
    use rtsim_kernel::journal::OpId;
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-exch-j4");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let paged = j.ind_commands.len();
    let exchanges = |j: &Java, from: usize| -> Vec<(u64, TrainId)> {
        j.ind_commands[from..]
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) => Some((d.op.0, d.train)),
                _ => None,
            })
            .collect()
    };
    j.mc_run_until(Tick(72_100));
    fillers(&mut j, 8_000);
    // The trains as the tracks file holds them when its write after the exchange fails: as they
    // were in the tick before the world applied it.
    let mut cargo_before = None;
    while j.mc().t < Tick(72_900) {
        let was = cargo(&j);
        j.mc_tick();
        let applied = exchanges(&j, paged)
            .iter()
            .any(|(op, _)| j.mc().applied_ops().contains(&OpId(*op)));
        if applied && cargo_before.is_none() {
            cargo_before = Some(was);
        }
    }
    let cargo_before = cargo_before.expect("the exchange applied before the save");
    let before = exchanges(&j, paged);
    assert_eq!(before.len(), 1, "one exchange before the save");
    let (x, train) = (OpId(before[0].0), before[0].1);
    let cargo_live = cargo(&j);
    assert_ne!(cargo_live, cargo_before, "the exchange loaded its train");
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(Tick(73_500));
    assert_eq!(cargo(&j), cargo_live, "no other exchange before the crash");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    // The control, from a copy of the pages: both of Java's files saved (J1), the restored world's
    // own baseline against the books.
    let control_dir = copy_pages(&dir, "ind-exch-j4-control");
    mc.restore(&msave);
    let e = engine_with(&control_dir, rtsim_jni::wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut c = restart(e, jsave.clone(), mc, MODULE_INDUSTRY);
    c.mc_run_until(Tick(73_500));
    let control = books_delta(&c);
    assert_eq!(cargo(&c), cargo_live, "the control holds what it held");
    let mut mc = c.mc.take().unwrap();
    c.e.close(true, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    // J4: the exchange's train from before it, the rest from the world save after it.
    let (t, was) = mc
        .trains
        .iter_mut()
        .zip(&cargo_before)
        .find(|(t, _)| t.id == train)
        .expect("the exchange's train");
    for (c, was) in t.carriages.iter_mut().zip(was) {
        c.cargo = was.clone();
    }
    assert!(mc.forget_applied(x), "the ring held the op");
    let e = engine_with(&dir, rtsim_jni::wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    j.mc_run_until(Tick(73_500));
    replayed_over_several(&j);
    let after = exchanges(&j, 0);
    let unsent = j.e.with_industry(|_, s| s.answered_unsent);
    let j4 = books_delta(&j);
    eprintln!(
        "ind-exch-j4: exchange {:#x}; sent after the restart {after:x?} ({unsent} known failed, \
         not sent); cargo live {cargo_live:?}, after the restart {:?}; the world against its \
         books {j4:?}, the control's {control:?}",
        x.0,
        cargo(&j)
    );
    assert_eq!(after, before, "re-derived under its op id and written");
    assert!(
        j.mc().applied_ops().contains(&x),
        "applied to the restored train"
    );
    assert_eq!(cargo(&j), cargo_live, "train 1 holds the ore once");
    assert_eq!(j4, control, "the world against its books, as the control's");
    assert!(
        !industry(&j.e, |ie| ie.exchange_pending(x)),
        "the exchange is settled"
    );
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// [`a_station_exchange_whose_trains_save_was_lost_is_applied_again_after_the_replay`] with the
/// restart's industry off, switched on after the replay (RP-313, the build lead's P2-1 on RP-310
/// to RP-312). The copy can only be the pages' there: the replay with the module off ignores the
/// arrivals (U10), so it decides no exchange again. Session 0 writes train 1's exchange X at the
/// ore coast and crashes before the world applies it, its save holding the arrival. Session 1's
/// replay decides X again under its op, and its pages are taken at once, inside the replay end's
/// hand-over before the first drain: they hold X's copy unsent, as a command waiting for a Java
/// name or a full buffer would. X goes after that drain, and the world applies it at S. The world
/// saves (J4: its journal with S, train 1's cargo and the op in its ring from the tick before S),
/// and the engine crashes with no page save past those pages. Session 2 restarts from them with
/// industry off: the replay brings S, and the off path keeps the IND_CARRIAGE_RESULT for the
/// switch. Before RP-313 S also took X's copy out of the engine's commands (`unqueue`), so X was
/// never sent and the switch booked a delivery onto the empty restored train. The control is the
/// same restart from a copy of the pages and J1 (both of Java's files saved).
#[test]
fn a_station_exchange_whose_trains_save_was_lost_is_applied_again_after_a_restart_with_industry_off()
 {
    use rtsim_industry::ids::TrainId;
    use rtsim_kernel::journal::OpId;
    use std::sync::atomic::Ordering::Acquire;
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-exch-j4-off");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let paged = j.ind_commands.len();
    let exchanges = |j: &Java, from: usize| -> Vec<(u64, TrainId)> {
        j.ind_commands[from..]
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) => Some((d.op.0, d.train)),
                _ => None,
            })
            .collect()
    };
    j.mc_run_until(Tick(72_100));
    fillers(&mut j, 8_000);
    // Session 0: X is written, and the world saves before it applies X.
    while exchanges(&j, paged).is_empty() {
        assert!(j.mc().t < Tick(72_900), "the exchange at the ore coast");
        j.mc_tick();
    }
    let before = exchanges(&j, paged);
    assert_eq!(before.len(), 1, "one exchange");
    let (x, train) = (OpId(before[0].0), before[0].1);
    assert!(!j.mc().applied_ops().contains(&x), "not applied yet");
    let jsave0 = j.world_save();
    let msave0 = j.mc().saved();
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave0);
    let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave0, mc, MODULE_INDUSTRY);
    // Session 1: the pages P1 through the journal's end at once, records right behind them.
    let p1 = j.next_obs - 1;
    j.e.request_save(p1);
    fillers(&mut j, 40);
    let mut cargo_before = None;
    while cargo_before.is_none() || j.e.durable_obs_seq() < p1 {
        assert!(j.mc().t < Tick(73_500), "P1 lands and the world applies X");
        let was = cargo(&j);
        j.mc_tick();
        if cargo_before.is_none() && j.mc().applied_ops().contains(&x) {
            cargo_before = Some(was);
        }
    }
    let cargo_before = cargo_before.unwrap();
    assert_eq!(j.e.durable_obs_seq(), p1);
    replayed_over_several(&j);
    let again: Vec<u64> = exchanges(&j, 0).iter().map(|e| e.0).collect();
    assert_eq!(again, vec![x.0], "decided again under its op and written");
    let t = j.mc().t;
    j.mc_run_until(t + 100);
    let jsave1 = j.world_save();
    let msave1 = j.mc().saved();
    let saved_at = j.mc().t;
    j.mc_run_until(saved_at + 600);
    assert_eq!(j.e.durable_obs_seq(), p1, "no page save past P1");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    let (on_at, end) = (saved_at + 300, saved_at + 1_500);
    let off_then_on = |dir: &std::path::PathBuf, jsave: WorldSave, mc: FakeWorld| -> Java {
        let e = engine_with(dir, wire::MIN_CAPACITY, |c| {
            c.industry_seed = Some(cfg.seed)
        });
        let mut j = restart(e, jsave, mc, 0);
        j.mc_run_until(on_at);
        assert!(
            j.e.stats.replays_ended.load(Acquire) > 0,
            "the replay ended while off"
        );
        assert!(
            industry(&j.e, |ie| ie.exchange_pending(x)),
            "off, the replayed IND_CARRIAGE_RESULT waits for the switch"
        );
        j.modules(MODULE_INDUSTRY);
        j.mc_tick();
        j.mc_mut().announce();
        j.mc_run_until(end);
        j
    };
    let control_dir = copy_pages(&dir, "ind-exch-j4-off-control");
    mc.restore(&msave1);
    let mut c = off_then_on(&control_dir, jsave1.clone(), mc);
    let control = books_delta(&c);
    let cargo_control = cargo(&c);
    let mut mc = c.mc.take().unwrap();
    c.e.close(true, Duration::from_secs(5)).unwrap();
    // J4: X's train from the tick before S, the rest from the world save after it.
    mc.restore(&msave1);
    let (t, was) = mc
        .trains
        .iter_mut()
        .zip(&cargo_before)
        .find(|(t, _)| t.id == train)
        .expect("the exchange's train");
    for (c, was) in t.carriages.iter_mut().zip(was) {
        c.cargo = was.clone();
    }
    assert!(mc.forget_applied(x), "the ring held the op");
    let j = off_then_on(&dir, jsave1, mc);
    let sent = exchanges(&j, 0).iter().filter(|e| e.0 == x.0).count();
    let j4 = books_delta(&j);
    eprintln!(
        "ind-exch-j4-off: exchange {:#x}, sent {sent} time(s) after the switch; cargo {:?}, the \
         control's {cargo_control:?}; the world against its books {j4:?}, the control's \
         {control:?}",
        x.0,
        cargo(&j)
    );
    assert_eq!(sent, 1, "X sent once after the switch");
    assert!(
        j.mc().applied_ops().contains(&x),
        "applied to the restored train"
    );
    assert_eq!(
        cargo(&j),
        cargo_control,
        "the trains hold what the control's hold"
    );
    assert_eq!(j4, control, "the world against its books, as the control's");
    assert!(
        !industry(&j.e, |ie| ie.exchange_pending(x)),
        "the exchange is settled"
    );
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// J4 when the pages restored hold the decision of train 1's exchange X at the ore coast (RP-317,
/// the build lead's question on RP-313's trap 4), so the replay decides nothing again and the only
/// copy there can be is the pages' outbox's. `written`: the pages are taken between X's write and
/// its application, X in their outbox and its answer S past them (a world save while the step's
/// commands are in flight takes them so); else at the world save after S, X answered inside them.
/// The world saves W after S, and the crash keeps W's journal file, whose `lastAppliedCmdSeq` is
/// past X's command, and loses W's tracks file: train 1's cargo and its ring (the fake world's
/// applied set) are from the tick before S. The control restarts the same pages with W whole (J1).
/// Each start trims the outbox by `lastAppliedCmdSeq`, which takes X's copy. Not the cause: without
/// the trim the replayed S (its IND_CARRIAGE_RESULT, and its APPLIED, RP-310) takes the copy, and
/// without those Java skips it as stale under its old sequence (RP-317's mutations). So X is never
/// sent again: the control holds what it held, and J4's train lacks X's load, which both restarts'
/// books count. A loss of X's load, never a double, the same whichever side of the pages S is:
/// J4's direction A for a train op the pages decided (one decided past them is decided again and
/// applied by the ring that came back without it, RP-311). Since RP-200 J4 needs a tracks write
/// that failed while the journal's succeeded.
fn exchange_decided_before_the_pages_is_lost_with_its_trains_save(tag: &str, written: bool) {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir(tag);
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let paged = j.ind_commands.len();
    // (cmdSeq, the delta) of each station exchange Java applied from `from` on.
    let exchanges = |j: &Java, from: usize| -> Vec<(u64, rtsim_industry::cmd::CarriageDelta)> {
        j.ind_commands[from..]
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) => Some((c.1, d.clone())),
                _ => None,
            })
            .collect()
    };
    while exchanges(&j, paged).is_empty() {
        assert!(j.mc().t < Tick(73_000), "the exchange at the ore coast");
        j.mc_tick();
    }
    let written_now = exchanges(&j, paged);
    assert_eq!(written_now.len(), 1, "one exchange");
    let (x_seq, d) = written_now[0].clone();
    let x = d.op;
    assert!(
        !j.mc().applied_ops().contains(&x),
        "X written, not applied yet"
    );
    let outbox_at_write = j.e.with_industry(|_, s| s.outbox);
    assert!(outbox_at_write >= 1, "X waits in the outbox for its answer");
    let cargo_before = cargo(&j);
    let p = j.next_obs - 1;
    if written {
        j.e.request_save(p);
        mc_tick_until_durable(&mut j, p);
    } else {
        j.mc_tick();
    }
    assert!(j.mc().applied_ops().contains(&x), "the world applied X");
    let s = answers(&j, x.0);
    assert_eq!(s.len(), 1, "one answer S");
    let (s, status) = s[0];
    assert_eq!(
        status,
        wire::obs::command_result::APPLIED,
        "S is an APPLIED"
    );
    let cargo_live = cargo(&j);
    assert_ne!(cargo_live, cargo_before, "X loaded its train");
    let t = j.mc().t;
    j.mc_run_until(t + 100);
    let jsave = j.world_save();
    let msave = j.mc().saved();
    assert!(
        jsave.last_applied_cmd >= x_seq,
        "W's lastAppliedCmdSeq covers X's command"
    );
    if !written {
        let w = j.next_obs - 1;
        j.e.request_save(w);
        mc_tick_until_durable(&mut j, w);
    }
    let pages = j.e.durable_obs_seq();
    if written {
        assert!(pages == p && s > pages, "the pages end before S");
    } else {
        assert!(pages >= s, "the pages hold S");
    }
    j.mc_run_until(Tick(73_500));
    assert_eq!(j.e.durable_obs_seq(), pages, "no page save past them");
    assert_eq!(cargo(&j), cargo_live, "no other exchange before the crash");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    let run = |dir: &std::path::PathBuf, mc: FakeWorld| -> Java {
        let e = engine_with(dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
        let mut j = restart(e, jsave.clone(), mc, MODULE_INDUSTRY);
        assert_eq!(
            j.e.with_industry(|_, s| s.outbox),
            0,
            "the start trimmed the outbox by lastAppliedCmdSeq"
        );
        j.mc_run_until(Tick(73_500));
        assert!(exchanges(&j, 0).is_empty(), "X never sent again");
        assert_eq!(j.stale_commands, 0, "nothing sent under an old sequence");
        assert!(
            !industry(&j.e, |ie| ie.exchange_pending(x)),
            "the exchange is settled"
        );
        assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
        j
    };
    // The control, from a copy of the pages: both of Java's files saved (J1).
    let control_dir = copy_pages(&dir, &format!("{tag}-control"));
    mc.restore(&msave);
    let mut c = run(&control_dir, mc);
    let control = books_delta(&c);
    let control_totals = c.mc().totals();
    assert_eq!(cargo(&c), cargo_live, "the control holds what it held");
    let mut mc = c.mc.take().unwrap();
    // J4: X's train from the tick before S, the rest from W.
    mc.restore(&msave);
    let (t, was) = mc
        .trains
        .iter_mut()
        .zip(&cargo_before)
        .find(|(t, _)| t.id == d.train)
        .expect("the exchange's train");
    for (c, was) in t.carriages.iter_mut().zip(was) {
        c.cargo = was.clone();
    }
    assert!(mc.forget_applied(x), "the ring held the op");
    let j = run(&dir, mc);
    let j4 = books_delta(&j);
    let load = GoodVec::from_pairs(d.entries.iter().map(|&(_, g, n)| (g, n)));
    let lost = control_totals.minus(&j.mc().totals());
    eprintln!(
        "{tag}: exchange {:#x} (cmdSeq {x_seq}, S {s}), pages through {pages}, W's \
         lastAppliedCmdSeq {}; outbox at the write {outbox_at_write}; the world short of the \
         control's by {lost:?}, X's load {load:?}; the world against its books {j4:?}, the \
         control's {control:?}",
        x.0, jsave.last_applied_cmd
    );
    assert_eq!(
        cargo(&j),
        cargo_before,
        "the restored train never gets X's load"
    );
    assert_eq!(
        lost, load,
        "the world is short of the control's by X's load"
    );
    let books = c.e.with_industry(|ie, _| j.mc().conservation_with(ie));
    assert_eq!(books, j4, "the two restarts' books are the same");
    assert_ne!(j4, control, "J4's world is short of its books");
    c.e.close(true, Duration::from_secs(5)).unwrap();
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// RP-313's trap 4: the pages between X's write and its application, the copy in their outbox.
#[test]
fn a_station_exchange_written_before_the_pages_is_lost_with_its_trains_save() {
    exchange_decided_before_the_pages_is_lost_with_its_trains_save("ind-exch-j4-outbox", true);
}

/// The same with X answered inside the pages: no copy at all, the same loss.
#[test]
fn a_station_exchange_answered_before_the_pages_is_lost_with_its_trains_save() {
    exchange_decided_before_the_pages_is_lost_with_its_trains_save("ind-exch-j4-answered", false);
}

/// Iron in the fake world: (in its stores, on its trains, minted, burned).
fn world_iron(j: &Java) -> (i64, i64, i64, i64) {
    let g = rtsim_industry::ids::Good(1);
    let w = j.mc();
    let mut stores = 0i64;
    for d in &w.districts {
        for i in &d.inventories {
            if !i.destroyed {
                stores += i.contents.get(g);
            }
        }
    }
    let mut carried = 0i64;
    for t in &w.trains {
        for c in &t.carriages {
            carried += c.cargo.get(g);
        }
    }
    (
        stores,
        carried,
        w.books.minted.get(g),
        w.books.burned.get(g),
    )
}

/// A command Java answered FAILED live is never applied after a crash (the review of 2a975cc3,
/// P1-1 there; docs/choices-mowzie-build.md RP-259). Train 1's exchange at the ore coast (72,800)
/// fails live (the world fails every carriage delta from 72,100 to 72,850); the world saves at
/// 72,900 and the crash comes at 73,500. The replay, over several hand-overs, decides the call
/// again under the live op (RP-272), and the copy waits in the held engine (RP-274); the replayed
/// FAILED answer takes the exchange back. On 2a975cc3 the copy went to Java after the replay end,
/// and the world, which keeps only the ops it applied, applied it: 224 ore loaded that never were.
#[test]
fn an_exchange_answered_failed_live_is_not_applied_after_a_crash() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-failed-again");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let paged = j.ind_commands.len();
    j.mc_run_until(Tick(72_100));
    fillers(&mut j, REPLAY_FILLERS);
    j.mc_mut().fail_carriage_deltas = true;
    j.mc_run_until(Tick(72_850));
    j.mc_mut().fail_carriage_deltas = false;
    j.mc_run_until(Tick(72_900));
    let exchanges = |j: &Java, from: usize| -> Vec<u64> {
        j.ind_commands[from..]
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) => Some(d.op.0),
                _ => None,
            })
            .collect()
    };
    let failed: Vec<u64> = exchanges(&j, paged)
        .into_iter()
        .filter(|op| {
            !j.mc()
                .applied_ops()
                .contains(&rtsim_kernel::journal::OpId(*op))
        })
        .collect();
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(Tick(73_500));
    let cargo_live = cargo(&j);
    let iron_live = world_iron(&j);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let saved_ops = mc.applied_ops().clone();
    let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    j.mc_run_until(Tick(73_500));
    let handovers = replayed_over_several(&j);
    let cargo_again = cargo(&j);
    let iron_again = world_iron(&j);
    let after = exchanges(&j, 0);
    let applied_after: Vec<u64> = after
        .iter()
        .copied()
        .filter(|op| {
            let op = rtsim_kernel::journal::OpId(*op);
            j.mc().applied_ops().contains(&op) && !saved_ops.contains(&op)
        })
        .collect();
    eprintln!(
        "{handovers} replay hand-overs; failed live {failed:x?}; sent after the restart {after:x?}; \
         applied after the restart {applied_after:x?}; iron (stores, carried, minted, burned) live \
         {iron_live:?}, again {iron_again:?}; cargo at the crash {cargo_live:?}, again {cargo_again:?}"
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    assert!(!failed.is_empty(), "an exchange failed live");
    assert!(
        applied_after.iter().all(|op| !failed.contains(op)),
        "an exchange answered FAILED live is applied after the restart: {applied_after:x?}"
    );
    assert_eq!(cargo_again, cargo_live, "the trains hold what they held");
    assert_eq!(iron_again, iron_live, "no iron made or lost");
}

/// A copy of the page directory `from` (the engine closed) in a new one.
fn copy_pages(from: &std::path::Path, tag: &str) -> std::path::PathBuf {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let dest = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy(&e.path(), &dest);
            } else {
                std::fs::copy(e.path(), dest).unwrap();
            }
        }
    }
    let to = tmpdir(tag);
    copy(from, &to);
    to
}

/// The world against the engine's books ([`FakeWorld::conservation_with`]): Ok, or what is off.
fn books_delta(j: &Java) -> Result<(), String> {
    let mc = j.mc();
    j.e.with_industry(|ie, _| mc.conservation_with(ie))
}

/// The slice R re-check's case (RP-259, P1-2 there): Java's FAILED answers heard while the replay
/// ran (`Link::heard`) are memory only, so pages saved inside the replay end's hand-over, before the
/// first drain, hold the copy the replay decided again and its take-back but not the answers; a
/// second crash before the next page save restores that page, and on R alone the copy was written:
/// the world against its books +352 iron where one crash leaves +128 (the restored world's own
/// baseline). With slice A merged, Java's answer takes the copy out of the engine's commands at once
/// (`Engine::take_out`, RP-285), so the page holds no copy to write. The first restart, run to the
/// same game time, is the control.
#[test]
fn a_second_crash_after_a_save_before_the_first_drain_writes_no_failed_copy() {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-failed-twice");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    j.mc_run_until(Tick(72_100));
    fillers(&mut j, REPLAY_FILLERS);
    j.mc_mut().fail_carriage_deltas = true;
    j.mc_run_until(Tick(72_850));
    j.mc_mut().fail_carriage_deltas = false;
    j.mc_run_until(Tick(72_900));
    let jsave = j.world_save();
    let msave = j.mc().saved();
    j.mc_run_until(Tick(73_500));
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    // The world saves at once, and the pages through its journal's end are asked for with records
    // right behind them, so they are taken inside the replay end's hand-over, before the first
    // drain.
    let s0 = j.next_obs - 1;
    j.e.request_save(s0);
    let jsave2 = j.world_save();
    let msave2 = j.mc().saved();
    fillers(&mut j, 40);
    mc_tick_until_durable(&mut j, s0);
    j.mc_run_until(Tick(73_500));
    let control = books_delta(&j);
    let iron_once = world_iron(&j);
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave2);
    let e = engine_with(&dir, wire::MIN_CAPACITY, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = restart(e, jsave2, mc, MODULE_INDUSTRY);
    j.mc_run_until(Tick(73_500));
    let twice = books_delta(&j);
    let iron_twice = world_iron(&j);
    let unsent = j.e.with_industry(|_, l| l.answered_unsent);
    eprintln!(
        "the world against its books after one crash {control:?}, after the second {twice:?}; iron \
         (stores, carried, minted, burned) {iron_once:?} and {iron_twice:?}; answered_unsent {unsent}"
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    assert_eq!(
        twice, control,
        "a second crash after a save before the first drain"
    );
    assert_eq!(iron_twice, iron_once, "no iron made or lost");
}

/// The other order (P3-2 of the build lead's review of RP-310, RP-314): Java's FAILED heard while
/// the replay ran before the copy is decided again, so nothing takes the copy at the answer, and
/// only `Link::heard`, in memory, drops it at the first drain after the replay end. Pages saved
/// inside the replay end's hand-over (`save_now` runs after `replay_end_before` and before
/// `simulate`'s drain) are past the FAILED answer and hold neither it nor the drop, so a second
/// crash from them decides the copy again with no answer to hear, and it is written. Here a train
/// raid's loot: the raider orders it again after each restart (as its own older pages make it,
/// RP-183), and the held step decides it at the first `simulate` after the replay end. Session 0
/// runs the raid with the world failing every carriage delta, so Java answers the loot FAILED and
/// the engine takes it back (W10); the world saves after the answer and crashes with the pages
/// still before the raid. Session 1 (the control) asks for its pages at once, with records behind
/// them, and drops the copy at the first drain (`answered_unsent`). The second crash restarts from
/// those pages: the loot goes under the live op and the world applies it. Benign: the loot is
/// booked stolen when it is decided and the world's live answer settles it (applied here; a FAILED
/// one would take it back, W10), so the world against its books is the control's. The chain write
/// a store's wake decides again past its FAILED answer (the pages then hold the copy itself) has
/// no crash fixture: the fake world has no package stores and never fails an IND_WRITE_CHAIN.
#[test]
fn a_raid_answered_failed_and_decided_again_past_its_answer_is_booked_after_a_second_crash() {
    use rtsim_industry::fake::{RESULT_APPLIED, RESULT_FAILED};
    use rtsim_industry::ids::TrainId;
    use rtsim_kernel::journal::OpId;
    let cfg = raid_line();
    let dir = tmpdir("ind-raid-failed-twice");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let train = TrainId(1);
    let laden = |j: &Java| {
        industry(&j.e, |ie| {
            ie.legs()
                .leg_of(train, ie.now())
                .and_then(|id| ie.legs().get(id))
                .is_some_and(|l| l.cargo.iter().any(|c| !c.1.is_empty()))
        })
    };
    // The pages stop while the train is under way with cargo, before the raid.
    loop {
        while !laden(&j) {
            j.mc_tick();
            assert!(j.mc().t < day(8.0), "train 1 under way with cargo");
        }
        let through = j.next_obs - 1;
        j.e.request_save(through);
        mc_tick_until_durable(&mut j, through);
        if laden(&j) {
            break;
        }
    }
    let through = j.e.durable_obs_seq();
    let stolen = |j: &Java| industry(&j.e, |ie| ie.books.stolen.total());
    let stolen_before = stolen(&j);
    let raid = Threat {
        id: 0xA11,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::Train(train),
        loads: None,
        at: j.mc().t,
    };
    j.mc_mut().fail_carriage_deltas = true;
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    while loot_ops(&j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < raid.at + 100, "the raid's loot is written");
    }
    let live = loot_ops(&j)[0];
    for _ in 0..5 {
        j.mc_tick();
    }
    j.mc_mut().fail_carriage_deltas = false;
    assert!(
        !j.mc().applied_ops().contains(&OpId(live)),
        "the world refused the raid"
    );
    match answers(&j, live)[..] {
        [(_, RESULT_FAILED)] => {}
        ref a => panic!("one FAILED for the loot op {live:#x}: {a:?}"),
    }
    assert_eq!(
        stolen(&j),
        stolen_before,
        "the FAILED answer took the loot back"
    );
    assert_eq!(j.e.durable_obs_seq(), through, "no later page save");
    let jsave = j.world_save();
    let msave = j.mc().saved();
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    // Session 1: the raider orders the raid again before the first step, so the held step decides
    // it at the replay end. The world saves at once, and the pages through its journal's end are
    // asked for with records right behind them, so they are taken inside the replay end's
    // hand-over, before the first drain.
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = restart(e, jsave, mc, MODULE_INDUSTRY);
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    let s0 = j.next_obs - 1;
    j.e.request_save(s0);
    let jsave2 = j.world_save();
    let msave2 = j.mc().saved();
    fillers(&mut j, 40);
    mc_tick_until_durable(&mut j, s0);
    let end = Tick(j.mc().t.0 + 400);
    j.mc_run_until(end);
    assert_eq!(
        j.e.durable_obs_seq(),
        s0,
        "no page save past the replay end's"
    );
    let control = books_delta(&j);
    let once = (
        loot_ops(&j),
        j.e.with_industry(|_, l| l.answered_unsent),
        stolen(&j),
    );
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave2);
    // The second crash: the pages are past the FAILED answer, so the replay hears none.
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = restart(e, jsave2, mc, MODULE_INDUSTRY);
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    j.mc_run_until(end);
    let twice = books_delta(&j);
    let sent = loot_ops(&j);
    let stolen_twice = stolen(&j);
    let applied = j.mc().applied_ops().contains(&OpId(live));
    let answered = answers(&j, live);
    eprintln!(
        "loot {live:#x}: after one crash sent {:x?}, {} answered and not sent, stolen {} (before \
         the raid {stolen_before}), the world against its books {control:?}; after the second \
         sent {sent:x?}, applied {applied}, answers {answered:?}, stolen {stolen_twice}, the world \
         against its books {twice:?}",
        once.0, once.1, once.2
    );
    j.e.close(true, Duration::from_secs(5)).unwrap();
    assert!(once.0.is_empty(), "after one crash the loot is never sent");
    assert_eq!(once.1, 1, "the drop is `heard`'s, at the first drain");
    assert_eq!(once.2, stolen_before, "and taken back");
    assert_eq!(
        sent,
        vec![live],
        "after the second the loot goes under the live op"
    );
    assert!(applied, "the world applies it");
    assert!(
        matches!(answered[..], [(_, RESULT_APPLIED)]),
        "one APPLIED: {answered:?}"
    );
    assert!(stolen_twice > stolen_before, "the loot is booked stolen");
    assert_eq!(
        twice, control,
        "the world against its books, after the second crash and after one"
    );
}

/// A raider whose pages are older than its train raid decides it again after a crash, against an
/// industry engine that is not held (docs/replay-gate.md §3.6; docs/choices-mowzie-build.md
/// RP-180 to RP-186). The raid's op is its identity, so the world answers DUPLICATE, the link
/// drops the loot exchange (through `answered_again`, RP-182), and the goods leave the train
/// once. The world save holds the raid's
/// application and its journaled results (both of Java's saves landed after it, J1); the pages
/// stop before the raid. Against main's derivation the re-decided raid is a new op, the world
/// gives the loot twice, and its books no longer close.
#[test]
fn a_train_raid_decided_again_after_a_crash_is_applied_once() {
    use rtsim_industry::cmd::CarriageReason;
    use rtsim_industry::ids::TrainId;
    use rtsim_kernel::journal::OpId;
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-trainraid");
    let mut j = iron_java(&dir, cfg);
    j.mc_run_until(day(3.0));
    let train = TrainId(1);
    let laden = |j: &Java| {
        industry(&j.e, |ie| {
            ie.legs()
                .leg_of(train, ie.now())
                .and_then(|id| ie.legs().get(id))
                .is_some_and(|l| l.cargo.iter().any(|c| !c.1.is_empty()))
        })
    };
    let loot_ops = |j: &Java| -> Vec<u64> {
        j.ind_commands
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) if d.reason == CarriageReason::Loot => Some(d.op.0),
                _ => None,
            })
            .collect()
    };
    // The pages stop while the train is under way with cargo, before the raid.
    loop {
        while !laden(&j) {
            j.mc_tick();
            assert!(j.mc().t < day(8.0), "train 1 under way with cargo");
        }
        let through = j.next_obs - 1;
        j.e.request_save(through);
        mc_tick_until_durable(&mut j, through);
        if laden(&j) {
            break;
        }
    }
    let through = j.e.durable_obs_seq();
    let raid = Threat {
        id: 0xA11,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::Train(train),
        loads: None,
        at: j.mc().t,
    };
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    while loot_ops(&j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < raid.at + 100, "the raid's loot is written");
    }
    let live = loot_ops(&j)[0];
    for _ in 0..5 {
        j.mc_tick();
    }
    assert!(
        j.mc().applied_ops().contains(&OpId(live)),
        "the world applied the raid"
    );
    let stolen = industry(&j.e, |ie| ie.books.stolen.total());
    assert!(stolen > 0, "the raid took something");
    // The world saves now, after the raid; the pages never save again before the crash.
    let jsave = j.world_save();
    let msave = j.mc().saved();
    assert_eq!(j.e.durable_obs_seq(), through, "no later page save");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    // The journal's results of the live raid come back first (order O2); then the raider, back
    // from its own older pages, decides the raid again, at industry's later `now`.
    j.mc_tick();
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    while loot_ops(&j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < raid.at + 400, "the raid is decided again");
    }
    assert_eq!(
        loot_ops(&j),
        vec![live],
        "the raid decided again carries the op the world applied"
    );
    iron_run_to_end(&mut j, &cfg);
    // The world's DUPLICATE is a second answer for an op the replay heard APPLIED, so `Replay`
    // refuses it as answered; the loot exchange is dropped all the same (RP-182). A copy
    // of the engine is asked to drop it again: nothing is left to drop.
    let pending = industry(&j.e, |ie| {
        let mut copy = Industry::restore(ie.snapshot(), Tunables::default()).expect("restores");
        copy.command_duplicate(OpId(live))
    });
    assert!(
        !pending,
        "the loot exchange of op {live:#x} is still pending after the world's DUPLICATE"
    );
    assert_eq!(industry(&j.e, |ie| ie.books.stolen.total()), stolen);
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// The iron line the train raid tests run.
fn raid_line() -> IronLine {
    IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    }
}

/// The ops of the loot CarriageDeltas this session's Java applied.
fn loot_ops(j: &Java) -> Vec<u64> {
    use rtsim_industry::cmd::CarriageReason;
    j.ind_commands
        .iter()
        .filter_map(|c| match &c.2 {
            Cmd::CarriageDelta(d) if d.reason == CarriageReason::Loot => Some(d.op.0),
            _ => None,
        })
        .collect()
}

/// This session's answers for industry op `op` still in Java's journal: (sequence, the
/// COMMAND_RESULT's status).
fn answers(j: &Java, op: u64) -> Vec<(u64, u32)> {
    use rtsim_jni::wire::obs;
    j.journal
        .iter()
        .filter(|(s, r)| {
            *s > j.replay_up_to
                && wire::u16_at(r, wire::R_KIND) == obs::COMMAND_RESULT
                && wire::u64_at(r, obs::command_result::OP_ID) == op
        })
        .map(|(s, r)| (*s, wire::u32_at(r, wire::R_AUX)))
        .collect()
}

/// A train raid applied live, as the two-files tests take it.
struct LiveRaid {
    threat: Threat,
    /// The loot's op.
    live: u64,
    /// The engine's pages, durable while train 1 is under way laden, before the raid.
    through: u64,
    /// The sequence of the world's APPLIED for the loot: the answer Java's ring waits on.
    s: u64,
    stolen: i128,
    /// Java's two files at `through`.
    jsave_before: WorldSave,
    msave_before: rtsim_industry::fake::SavedWorld,
}

/// Runs the iron line until train 1 is under way with cargo and the pages are durable there, then
/// orders a raid on it and ticks until the world has applied the loot and answered APPLIED.
fn raid_a_laden_train(j: &mut Java) -> LiveRaid {
    use rtsim_industry::fake::RESULT_APPLIED;
    use rtsim_industry::ids::TrainId;
    use rtsim_kernel::journal::OpId;
    j.mc_run_until(day(3.0));
    let train = TrainId(1);
    let laden = |j: &Java| {
        industry(&j.e, |ie| {
            ie.legs()
                .leg_of(train, ie.now())
                .and_then(|id| ie.legs().get(id))
                .is_some_and(|l| l.cargo.iter().any(|c| !c.1.is_empty()))
        })
    };
    // The pages stop while the train is under way with cargo, before the raid.
    loop {
        while !laden(j) {
            j.mc_tick();
            assert!(j.mc().t < day(8.0), "train 1 under way with cargo");
        }
        let through = j.next_obs - 1;
        j.e.request_save(through);
        mc_tick_until_durable(j, through);
        if laden(j) {
            break;
        }
    }
    let through = j.e.durable_obs_seq();
    let jsave_before = j.world_save();
    let msave_before = j.mc().saved();
    let threat = Threat {
        id: 0xA11,
        source: ThreatSource::Bandit,
        class: ThreatClass::Siege,
        count: 3,
        strength: 3.0,
        target: ThreatTarget::Train(train),
        loads: None,
        at: j.mc().t,
    };
    j.e.order(vec![Order::IndustryThreat(threat)]).unwrap();
    while loot_ops(j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < threat.at + 100, "the raid's loot is written");
    }
    let live = loot_ops(j)[0];
    for _ in 0..5 {
        j.mc_tick();
    }
    assert!(
        j.mc().applied_ops().contains(&OpId(live)),
        "the world applied the raid"
    );
    let stolen = industry(&j.e, |ie| ie.books.stolen.total());
    assert!(stolen > 0, "the raid took something");
    let s = match answers(j, live)[..] {
        [(s, RESULT_APPLIED)] => s,
        ref a => panic!("one APPLIED for the loot op {live:#x}: {a:?}"),
    };
    LiveRaid {
        threat,
        live,
        through,
        s,
        stolen,
        jsave_before,
        msave_before,
    }
}

/// A train raid against Java's two files saved apart (docs/choices-mowzie-build.md RP-196, RP-199),
/// through the host: the raider's and industry's pages stop before the raid, the raid is applied,
/// and the restart reads one of Java's files from after the raid and the other from before it.
/// The fake world saves its applied op set with its trains, so `mc.saved()` is the tracks file
/// with the ring (RP-190: the goods and the ring are one file) and `world_save()` the journal's.
/// J3 keeps the trains and loses the journal: the raid decided again is answered DUPLICATE by the
/// ring, and since the journal lost the live answers, `Replay` hears it as a new answer, so it
/// reaches `command_result`, which must drop the loot exchange. J4 keeps the journal and loses the
/// trains: the ring came back without the op, so the raid is applied again to the goods the
/// restart restored, and its result takes the exchange.
///
/// `j3-late` is J3 with the raid decided again only after the engine's first durable save past the
/// replay end (RP-252: the raider re-decides at its own pace, RP-188). The ring keeps the op, its
/// answer unknown, through that save. `j3-late-pruned` drops the op from the fake world's applied
/// set at that save, as a ring trimmed on the first durable save would (the reviewer's rule): the
/// raid is then taken twice and the world no longer conserves. `j1-late` and `j1-late-pruned` are
/// the same with both files from after the raid (hole a): the journal holds the raid's answer, so a
/// ring trimmed on the journal's mark drops the op at its first save, and the journal's op index
/// forgets it once the engine acknowledges that answer, which the first durable save does. The
/// pruned arm is that state: the goods hold the raid and no dedup knows its op.
fn a_train_raid_across_javas_two_files(case: &str) {
    use rtsim_kernel::journal::OpId;
    let cfg = raid_line();
    let dir = tmpdir(&format!("ind-twofiles-{case}"));
    let mut j = iron_java(&dir, cfg);
    let LiveRaid {
        threat: raid,
        live,
        through,
        stolen,
        jsave_before,
        msave_before,
        ..
    } = raid_a_laden_train(&mut j);
    let jsave_after = j.world_save();
    let msave_after = j.mc().saved();
    assert_eq!(j.e.durable_obs_seq(), through, "no later page save");
    let (jsave, msave) = match case {
        // J3: the trains (with their ring) from after the raid, the journal from before it.
        "j3" | "j3-late" | "j3-late-pruned" => (jsave_before, msave_after),
        // J1: both from after the raid; the pages from before it (hole a, RP-252).
        "j1-late" | "j1-late-pruned" => (jsave_after, msave_after),
        // J4: the journal from after the raid, the trains from before it (the chunks from after:
        // the chunk barrier holds the journal, never the chunks).
        "j4" => (jsave_after, msave_before.with_chunks_of(&msave_after)),
        _ => unreachable!(),
    };
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    j.mc_tick();
    if case.ends_with("-late") || case.ends_with("-late-pruned") {
        // The first durable save past the replay end: the pages hold this session's first record.
        let through = j.next_obs - 1;
        j.e.request_save(through);
        mc_tick_until_durable(&mut j, through);
        assert!(
            loot_ops(&j).is_empty(),
            "{case}: nothing decided the raid again yet"
        );
        if case.ends_with("-pruned") {
            assert!(
                j.mc_mut().forget_applied(OpId(live)),
                "the ring held the op from the tracks file"
            );
        }
    }
    j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
    while loot_ops(&j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < raid.at + 400, "the raid is decided again");
    }
    assert_eq!(
        loot_ops(&j),
        vec![live],
        "the raid decided again carries the live op"
    );
    iron_run_to_end(&mut j, &cfg);
    let pending = industry(&j.e, |ie| ie.exchange_pending(OpId(live)));
    let stolen_after = industry(&j.e, |ie| ie.books.stolen.total());
    eprintln!(
        "{case}: stolen live {stolen}, after {stolen_after}, loot exchange pending {pending}, outbox {}",
        j.e.with_industry(|_, l| l.outbox)
    );
    if case.ends_with("-pruned") {
        let mc = j.mc();
        let double = j.e.with_industry(|ie, _| mc.conservation_with(ie));
        eprintln!("{case}: the world against the books: {double:?}");
        assert!(
            double.is_err(),
            "{case}: the op left the ring at the first durable save, so the raid decided after it \
             is applied a second time and the world loses the goods twice"
        );
        j.e.close(true, Duration::from_secs(5)).unwrap();
        return;
    }
    assert!(
        !pending,
        "{case}: the loot exchange of op {live:#x} is still pending"
    );
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// J3 through the host: the ring's DUPLICATE reaches `command_result` (the journal lost the live
/// APPLIED), which drops the loot exchange; without that drop the exchange stays pending.
#[test]
fn a_crash_that_kept_the_trains_and_not_the_journal_takes_a_train_raid_once() {
    a_train_raid_across_javas_two_files("j3");
}

/// J4 through the host: the raid is applied again to the restored goods and the books close.
#[test]
fn a_crash_that_kept_the_journal_and_not_the_trains_takes_a_train_raid_once() {
    a_train_raid_across_javas_two_files("j4");
}

/// J3 with the raid decided again after the engine's first durable save past the replay end
/// (RP-252): the ring settled the op to an unknown answer and kept it through that save, so the
/// raid is answered DUPLICATE and taken once.
#[test]
fn a_j3_raid_decided_again_after_the_first_durable_save_is_taken_once() {
    a_train_raid_across_javas_two_files("j3-late");
}

/// The same J3 under a ring trimmed at the first durable save past the replay end (the reviewer's
/// rule, RP-252; a trim on the journal's mark keeps this op, its answer past the journal on disk):
/// the op has left the ring when the raid is decided again, so it is applied twice. Pins why the
/// ring prunes only on the acknowledgement of its own answer.
#[test]
fn a_j3_raid_decided_again_after_a_ring_trimmed_at_the_first_durable_save_is_taken_twice() {
    a_train_raid_across_javas_two_files("j3-late-pruned");
}

/// Hole (a) (RP-252): both files from after the raid, the pages from before it, and the raid decided
/// again only after the engine's first durable save past the replay end, which acknowledges the
/// raid's old answer. The ring keeps the op (its answer unknown), so the raid is taken once.
#[test]
fn a_j1_raid_decided_again_after_the_first_durable_save_is_taken_once() {
    a_train_raid_across_javas_two_files("j1-late");
}

/// Hole (a) under a trim on the journal's mark (RP-252): the op left the ring at the first save,
/// since the journal on disk held its answer, and left the journal's op index at the acknowledgement
/// of that answer. The raid decided again after both is applied twice.
#[test]
fn a_j1_raid_decided_again_after_a_trim_on_the_journals_mark_is_taken_twice() {
    a_train_raid_across_javas_two_files("j1-late-pruned");
}

/// A station exchange the engine derived from a simulated arrival, applied by Java, then lost with
/// the journal (J3: the trains and their ring saved after the application, the journal before it,
/// with the arrival and without the application's IND_CARRIAGE_RESULT; the pages older than the
/// arrival). The replay derives the exchange again from the arrival under the same op, the ring
/// answers DUPLICATE, and `command_result` drops the exchange whatever its reason (RP-199): no
/// result will come for it. Before RP-199 only loot was dropped, so the exchange stayed pending
/// for ever, into every page.
#[test]
fn a_station_exchange_derived_again_after_a_crash_that_lost_its_result_is_dropped() {
    use rtsim_industry::cmd::CarriageReason;
    use rtsim_kernel::journal::OpId;
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let dir = tmpdir("ind-exchange-dup");
    let mut j = iron_java(&dir, cfg);
    let exchange_ops = |j: &Java| -> Vec<u64> {
        j.ind_commands
            .iter()
            .filter_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) if d.reason == CarriageReason::Exchange => Some(d.op.0),
                _ => None,
            })
            .collect()
    };
    // Past the first simulated exchanges, the pages are taken; then the next exchange.
    j.mc_run_until(day(2.0));
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let through = j.e.durable_obs_seq();
    let before = exchange_ops(&j).len();
    while exchange_ops(&j).len() == before {
        j.mc_tick();
        assert!(j.mc().t < day(8.0), "a simulated station exchange");
    }
    let x = exchange_ops(&j)[before];
    // The journal with the arrival, before the exchange is applied (one tick late, in the world's
    // next tick).
    let jsave = j.world_save();
    assert!(!j.mc().applied_ops().contains(&OpId(x)));
    for _ in 0..3 {
        j.mc_tick();
    }
    assert!(
        j.mc().applied_ops().contains(&OpId(x)),
        "the exchange is applied"
    );
    assert!(
        !industry(&j.e, |ie| ie.exchange_pending(OpId(x))),
        "its result took the exchange live"
    );
    let msave = j.mc().saved();
    assert_eq!(j.e.durable_obs_seq(), through, "no later page save");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, jsave);
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    let t0 = j.mc().t;
    while !exchange_ops(&j).contains(&x) {
        j.mc_tick();
        assert!(j.mc().t < t0 + 400, "the replay derives the exchange again");
    }
    iron_run_to_end(&mut j, &cfg);
    let pending = industry(&j.e, |ie| ie.exchange_pending(OpId(x)));
    assert!(
        !pending,
        "the station exchange of op {x:#x} is still pending after the world's DUPLICATE"
    );
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

// ---- the train ring across rollbacks of the engine's pages (docs/choices-mowzie-build.md RP-258) --

/// Ticks the world, one tick each 2 ms of wall clock, until `done`: a save on the IO lane lands
/// without the world running on far past it (the raid decided again after the crash needs train 1
/// still under way). Returns the ticks taken; panics after 20 s.
fn mc_tick_slowly_until(j: &mut Java, what: &str, mut done: impl FnMut(&Java) -> bool) -> u64 {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut ticks = 0;
    while !done(j) {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        j.mc_tick();
        ticks += 1;
        std::thread::sleep(Duration::from_millis(2));
    }
    ticks
}

/// One op in Java's train ring as `TrainRing` keeps it (RP-250, RP-251): the sequence of the op's
/// last answer in the session that settled the ring last (0, unknown, after a settle that could
/// not prove the engine's pages hold it), and that session's generation (`session`, its nonce).
/// `None` in its place: the op has left the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RingOp {
    at: u64,
    session: u64,
}

impl RingOp {
    /// The op answered at `at` in session `j`, as its ring stands now: it left once the session
    /// applied an acknowledgement at or past `at` (`TrainRing.answered`, `TrainRing.tick`).
    fn after_session(j: &Java, at: u64) -> Option<RingOp> {
        let acked = j.acks.iter().copied().max().unwrap_or(0);
        (acked < at).then_some(RingOp {
            at,
            session: j.nonce,
        })
    }

    /// The settle at the start of session `j`, which loaded Java's journal file `save`
    /// (`TrainRing.settle`, `RingView`): the op leaves when its answer is at or below the engine's
    /// pages and the view proves those pages are the ring's own session's; else it stays, its
    /// answer unknown, in this session's generation, where no acknowledgement takes it.
    fn settle(self, j: &Java, save: &WorldSave) -> Option<RingOp> {
        use rtsim_jni::wire::obs;
        let first = j.replay_up_to + 1;
        let writer_gen = save
            .pinned_gen
            .iter()
            .chain(save.journal.iter())
            .filter(|(s, r)| *s < first && wire::u16_at(r, wire::R_KIND) == obs::JOURNAL_GEN)
            .max_by_key(|(s, _)| *s)
            .map_or(0, |(_, r)| wire::u64_at(r, obs::journal_gen::GEN));
        let written = save.next_obs - 1;
        let proven = writer_gen != 0 && written < first && j.pages_obs <= written;
        let bound = if self.session == writer_gen && proven {
            j.pages_obs
        } else {
            0
        };
        (self.at == 0 || self.at > bound).then_some(RingOp {
            at: 0,
            session: j.nonce,
        })
    }
}

/// How the engine's pages past a train raid's answer are lost in
/// [`a_train_raid_and_a_rolled_back_save`].
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rollback {
    /// Java asks for a save past the answer; it writes its events and stops before its page
    /// commit, as a process killed there would, and the engine saves nothing after it
    /// (`crash_before_next_page_commit`). Java asks again; the crash comes later.
    CrashAtPageCommit,
    /// Java asks for a save past the answer; its commit fails (a torn log) and the store reopens
    /// at its last durable generation (`recover_store`). The crash comes before another save.
    FailedSave,
    /// As `FailedSave`, then a save past the answer is made and acknowledged; the crash after.
    FailedThenMade,
    /// No save is asked for past the answer: herds founded after it outgrow the row budget, which
    /// takes saves of its own (pending generations, PF15), and the restart rolls them back.
    Pressure,
}

/// The train ring's rollback cases (docs/choices-mowzie-build.md RP-258). A train raid is applied
/// and answered APPLIED at `s`, then the engine's pages past `s` are lost ([`Rollback`]) and the
/// server crashes with Java's two files from that moment (J1). The property the ring's prune rests
/// on: no acknowledgement reaches `s` before the pages holding `s` are durable, so an op whose
/// answer the engine acknowledged is never sent again. Each case checks that no acknowledgement
/// Java applied is past the pages the restart finds, models Java's ring ([`RingOp`]: the op stays
/// until an acknowledgement of `s`, and the restart's settle keeps it, unknown, when the pages
/// lack `s`), and plays on: pages without `s` decide the raid again and the ring answers
/// DUPLICATE; pages with it never send the op again. The world conserves either way.
fn a_train_raid_and_a_rolled_back_save(how: Rollback) {
    use rtsim_industry::fake::RESULT_DUPLICATE;
    use rtsim_jni::engine::MODULE_HERDS;
    use rtsim_kernel::journal::OpId;
    use rtsim_kernel::page::Fault;
    use std::sync::atomic::Ordering::Acquire;
    let cfg = raid_line();
    let dir = tmpdir(&format!("ind-rollback-{how:?}"));
    let pressure = how == Rollback::Pressure;
    let config = move |c: &mut rtsim_jni::engine::Config| {
        if pressure {
            c.row_budget = 4 * 1024;
        }
    };
    let mask = if pressure {
        MODULE_INDUSTRY | MODULE_HERDS
    } else {
        MODULE_INDUSTRY
    };
    let mut j = iron_java_with(&dir, cfg, 256 * 1024, config);
    j.module_mask = mask;
    let r = raid_a_laden_train(&mut j);
    let (s, live) = (r.s, r.live);
    assert!(j.sent_obs >= s, "the answer reached the engine");
    assert_eq!(j.e.durable_obs_seq(), r.through, "no save since the raid");
    let saves = j.e.stats.saves_done.load(Acquire);
    let mut budget_saves = 0;
    let mut ticks = 0;
    match how {
        Rollback::CrashAtPageCommit => {
            let serial = j.e.stats.archive_serial.load(Acquire);
            j.e.crash_before_next_page_commit();
            let asked = j.next_obs - 1;
            j.e.request_save(asked);
            // The save flushes its events (the archive's serial moves) and stops there.
            ticks += mc_tick_slowly_until(&mut j, "the save to reach its page commit", |j| {
                j.e.stats.archive_serial.load(Acquire) != serial
            });
            for _ in 0..200 {
                j.mc_tick();
            }
            // Java asks again, as its autosave would; the engine saves nothing more.
            let asked = j.next_obs - 1;
            j.e.request_save(asked);
            for _ in 0..200 {
                j.mc_tick();
            }
        }
        Rollback::FailedSave | Rollback::FailedThenMade => {
            let failed = j.e.stats_array()[15];
            j.e.inject_store_fault(Some(Fault::TornLog));
            let asked = j.next_obs - 1;
            j.e.request_save(asked);
            ticks += mc_tick_slowly_until(&mut j, "the failed save", |j| {
                j.e.stats_array()[15] != failed
            });
            for _ in 0..200 {
                j.mc_tick();
            }
            if how == Rollback::FailedThenMade {
                // A save built in the moment between the failure's count and the store's
                // recovery is stale and refused too (PF14): ask again, at most twice more.
                let asked = j.next_obs - 1;
                let mut refused = 0;
                loop {
                    let failed = j.e.stats_array()[15];
                    j.e.request_save(asked);
                    ticks += mc_tick_slowly_until(&mut j, "the save after the failed one", |j| {
                        j.e.durable_obs_seq() >= asked || j.e.stats_array()[15] != failed
                    });
                    if j.e.durable_obs_seq() >= asked {
                        break;
                    }
                    refused += 1;
                    assert!(refused < 3, "{refused} saves refused after the failed one");
                }
                ticks += mc_tick_slowly_until(&mut j, "the acknowledgement of the answer", |j| {
                    j.acks.iter().any(|&a| a >= s)
                });
                for _ in 0..20 {
                    j.mc_tick();
                }
            }
        }
        Rollback::Pressure => {
            // Herds whose rows outgrow the budget (archive.rs's recipe), founded once the engine
            // has read the answer: the budget takes a save of its own at once, at a step past the
            // answer (the pending generation's observations are checked on disk below).
            let before = j.e.stats.pressure_saves.load(Acquire);
            let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
            let unit = (0..128u64)
                .map(|i| Order::HerdFound {
                    name: format!("Testdown flock {i}"),
                    species: "minecraft:sheep".into(),
                    dimension: dim,
                    x: 1_000 + 64 * i as i32,
                    z: 1_000,
                    count: 20,
                    seed: i + 1,
                })
                .collect();
            j.e.order(unit).unwrap();
            ticks += mc_tick_slowly_until(&mut j, "a save of the budget's own", |j| {
                j.e.stats.pressure_saves.load(Acquire) > before
            });
            for _ in 0..200 {
                j.mc_tick();
            }
            budget_saves = j.e.stats.pressure_saves.load(Acquire) - before;
        }
    }
    let max_ack = j.acks.iter().copied().max().unwrap_or(0);
    let durable = j.e.durable_obs_seq();
    eprintln!(
        "{how:?}: answer at {s}, pages before the raid {}, durable {durable}, highest acknowledgement \
         applied {max_ack}, journal next {}, saves made since the raid {}, failed saves {}, budget \
         saves past the answer {budget_saves}, ticks waited {ticks}",
        r.through,
        j.next_obs,
        j.e.stats.saves_done.load(Acquire) - saves,
        j.e.stats_array()[15]
    );
    assert!(
        max_ack <= durable,
        "{how:?}: an acknowledgement past the durable pages"
    );
    if how == Rollback::FailedThenMade {
        assert!(
            max_ack >= s,
            "the save made past the answer is acknowledged"
        );
    } else {
        assert_eq!(durable, r.through, "{how:?}: no save past the answer made");
        assert_eq!(j.e.stats.saves_done.load(Acquire), saves);
        assert!(
            max_ack < s,
            "{how:?}: an acknowledgement reached the answer {s} while the pages holding it were \
             not durable"
        );
    }
    let ring = RingOp::after_session(&j, s);
    assert_eq!(ring.is_none(), how == Rollback::FailedThenMade);
    if ring.is_none() {
        assert!(
            j.mc_mut().forget_applied(OpId(live)),
            "the ring held the op until its acknowledgement"
        );
    }
    // The crash, with Java's two files as they stand (J1).
    let jsave = j.world_save();
    let msave = j.mc().saved();
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    if pressure {
        use rtsim_kernel::page::{Durability, PageStore};
        let store = PageStore::open(&dir, Durability::NoSync).unwrap();
        eprintln!(
            "Pressure: on disk, the confirmed generation at {}, the newest pending one at {}",
            store.confirmed().obs_seq,
            store.current().obs_seq
        );
        assert_eq!(store.confirmed().obs_seq, r.through);
        assert!(
            store.current().obs_seq >= s,
            "a pending generation holds the answer"
        );
    }
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| {
        c.industry_seed = Some(cfg.seed);
        config(c)
    });
    let mut j = Java::open(e, jsave.clone());
    j.mc = Some(mc);
    j.module_mask = mask;
    eprintln!(
        "{how:?}: the restart's pages {}, journal next {}",
        j.pages_obs, j.next_obs
    );
    assert!(
        j.pages_obs >= max_ack,
        "{how:?}: Java applied an acknowledgement of {max_ack}, past the pages the restart found ({})",
        j.pages_obs
    );
    let ring = ring.and_then(|o| o.settle(&j, &jsave));
    let sent_again = |j: &Java| {
        j.ind_commands
            .iter()
            .filter(|c| matches!(&c.2, Cmd::CarriageDelta(d) if d.op.0 == live))
            .count()
    };
    if how == Rollback::FailedThenMade {
        assert!(j.pages_obs >= s, "the pages hold the answer");
        assert_eq!(ring, None);
        // The raider's pages hold the raid: nothing decides it again, and the engine never sends
        // the acknowledged op again.
        iron_run_to_end(&mut j, &cfg);
        assert_eq!(
            sent_again(&j),
            0,
            "the engine sent op {live:#x} again after acknowledging its answer"
        );
    } else {
        assert_eq!(
            j.pages_obs, r.through,
            "{how:?}: the restart found the pages from before the raid"
        );
        assert_eq!(
            ring,
            Some(RingOp {
                at: 0,
                session: j.nonce
            }),
            "{how:?}: the settle keeps the op, its answer unknown"
        );
        j.mc_tick();
        // The raider, back from pages before the raid, decides it again.
        let t0 = j.mc().t;
        j.e.order(vec![Order::IndustryThreat(r.threat)]).unwrap();
        while loot_ops(&j).is_empty() {
            j.mc_tick();
            assert!(j.mc().t < t0 + 400, "{how:?}: the raid is decided again");
        }
        assert_eq!(loot_ops(&j), vec![live], "{how:?}: under the live op");
        for _ in 0..5 {
            j.mc_tick();
        }
        assert_eq!(
            answers(&j, live)
                .into_iter()
                .map(|a| a.1)
                .collect::<Vec<_>>(),
            vec![RESULT_DUPLICATE],
            "{how:?}: the ring answered the raid decided again"
        );
        iron_run_to_end(&mut j, &cfg);
        assert_eq!(sent_again(&j), 1);
    }
    assert!(
        !industry(&j.e, |ie| ie.exchange_pending(OpId(live))),
        "{how:?}: the loot exchange is still pending"
    );
    assert_eq!(industry(&j.e, |ie| ie.books.stolen.total()), r.stolen);
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// RP-258 case 1, `crash_at_page_commit`: the save past a train raid's answer dies before its page
/// commit, and no acknowledgement reaches the answer; after the crash the raid is taken once.
#[test]
fn a_save_crashed_before_its_page_commit_never_acknowledges_a_train_raid_answer() {
    a_train_raid_and_a_rolled_back_save(Rollback::CrashAtPageCommit);
}

/// RP-258 case 1, a failed save and `recover_store`: no acknowledgement reaches the answer; after
/// the crash the raid is taken once.
#[test]
fn a_failed_save_never_acknowledges_a_train_raid_answer() {
    a_train_raid_and_a_rolled_back_save(Rollback::FailedSave);
}

/// RP-258 case 1, a failed save, `recover_store`, then a save made and acknowledged: the restart's
/// pages hold every acknowledgement Java applied, so the op the ring dropped is never sent again.
#[test]
fn a_save_made_after_a_failed_one_acknowledges_only_what_the_restart_finds() {
    a_train_raid_and_a_rolled_back_save(Rollback::FailedThenMade);
}

/// RP-258 case 1, pending generations: the budget's own saves past the answer are never
/// acknowledged, and the restart rolls them back; the raid is taken once.
#[test]
fn a_budget_save_never_acknowledges_a_train_raid_answer() {
    a_train_raid_and_a_rolled_back_save(Rollback::Pressure);
}

/// RP-258 case 2, a sequence reused after a rollback (P29). J3: the trains (with their ring) from
/// after the raid, the journal from before it, the pages from before both. The journal lost the
/// answer's sequence `s` and the pages do not reach it, so the restart (`next_obs` is the larger
/// of the save's and the pages' + 1) journals another record at `s`, and a save through it is
/// acknowledged. The settle made the op's answer unknown in the new generation, so that
/// acknowledgement of another record leaves the op in the ring: the raid decided again is answered
/// DUPLICATE and taken once. `seq_only` is a ring keyed by sequence alone (no generation, no
/// settle): it takes that acknowledgement for the answer's and drops the op, and the raid is taken
/// twice.
fn a_train_raid_answer_whose_sequence_is_reused(seq_only: bool) {
    use rtsim_industry::fake::RESULT_DUPLICATE;
    use rtsim_kernel::journal::OpId;
    let cfg = raid_line();
    let dir = tmpdir(&format!("ind-reuse-{seq_only}"));
    let mut j = iron_java(&dir, cfg);
    let r = raid_a_laden_train(&mut j);
    let (s, live) = (r.s, r.live);
    let answer = j.journaled(s).expect("the answer in the journal");
    let ring = RingOp::after_session(&j, s).expect("nothing acknowledged the answer");
    let msave = j.mc().saved();
    assert_eq!(j.e.durable_obs_seq(), r.through, "no later page save");
    let mut mc = j.mc.take().unwrap();
    j.e.close(false, Duration::from_secs(5)).unwrap();
    mc.restore(&msave);
    let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
    let mut j = Java::open(e, r.jsave_before.clone());
    j.mc = Some(mc);
    j.module_mask = MODULE_INDUSTRY;
    assert!(
        j.next_obs <= s,
        "the restart journals from {}, below the lost answer's {s}",
        j.next_obs
    );
    let settled = ring.settle(&j, &r.jsave_before);
    assert_eq!(
        settled,
        Some(RingOp {
            at: 0,
            session: j.nonce
        }),
        "the settle keeps the op, its answer unknown"
    );
    while j.next_obs <= s {
        j.mc_tick();
    }
    let reused = j.journaled(s).expect("this session's record at s");
    assert_ne!(reused, answer, "another record took the answer's sequence");
    let through = j.next_obs - 1;
    j.e.request_save(through);
    mc_tick_until_durable(&mut j, through);
    let mut n = 0;
    while !j.acks.iter().any(|&a| a >= s) {
        j.mc_tick();
        n += 1;
        assert!(n < 100, "the save through {through} is acknowledged");
    }
    let acked = j.acks.iter().copied().max().unwrap();
    eprintln!(
        "reuse (seq_only {seq_only}): answer at {s} (kind {:#x}), the restart's first sequence {}, \
         the record that took {s} has kind {:#x}, acknowledged {acked}",
        wire::u16_at(&answer, wire::R_KIND),
        j.replay_up_to + 1,
        wire::u16_at(&reused, wire::R_KIND)
    );
    if seq_only {
        // A ring that waits on the sequence alone takes this acknowledgement for the answer's.
        assert!(j.mc_mut().forget_applied(OpId(live)));
    }
    // The raider, back from pages before the raid, decides it again at its own pace.
    let t0 = j.mc().t;
    j.e.order(vec![Order::IndustryThreat(r.threat)]).unwrap();
    while loot_ops(&j).is_empty() {
        j.mc_tick();
        assert!(j.mc().t < t0 + 400, "the raid is decided again");
    }
    assert_eq!(loot_ops(&j), vec![live], "under the live op");
    for _ in 0..5 {
        j.mc_tick();
    }
    let again = answers(&j, live);
    iron_run_to_end(&mut j, &cfg);
    if seq_only {
        let mc = j.mc();
        let double = j.e.with_industry(|ie, _| mc.conservation_with(ie));
        eprintln!("reuse (seq_only): answers {again:?}; the world against the books: {double:?}");
        assert!(
            double.is_err(),
            "a ring keyed by sequence alone dropped the op at the acknowledgement of the record \
             that reused its answer's sequence, so the raid is applied twice"
        );
        j.e.close(true, Duration::from_secs(5)).unwrap();
        return;
    }
    assert_eq!(
        again.into_iter().map(|a| a.1).collect::<Vec<_>>(),
        vec![RESULT_DUPLICATE],
        "the ring answered the raid decided again"
    );
    assert!(!industry(&j.e, |ie| ie.exchange_pending(OpId(live))));
    assert_eq!(j.e.with_industry(|_, l| l.outbox), 0);
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    conserves(&j);
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// RP-258 case 2 under the ring's generation and settle: taken once.
#[test]
fn a_ring_op_whose_answers_sequence_the_restart_reuses_is_taken_once() {
    a_train_raid_answer_whose_sequence_is_reused(false);
}

/// RP-258 case 2 under a ring keyed by sequence alone: taken twice. Pins why the ring keeps a
/// generation and settles at a session start.
#[test]
fn a_ring_keyed_by_sequence_alone_takes_a_raid_twice_when_its_answers_sequence_is_reused() {
    a_train_raid_answer_whose_sequence_is_reused(true);
}

/// Slice U: the real engine never sends an op again after acknowledging Java's live answer for it
/// (docs/choices-journal-barrier.md RP-209; docs/choices-mowzie-build.md RP-250, RP-251, RP-257).
///
/// Java's op rings prune on that promise. Java answers op X with a COMMAND_RESULT at journal seq S,
/// live in session G (above G's replay end), and stamps its ring entry with S. When Java reads an
/// engine acknowledgement A >= S in G, it removes X; the journal is trimmed through A at the same
/// time, so nothing in Java knows X after that. A ring from G read by a later session G' loses X at
/// G''s start only when G''s pages are at or past S and provably G's (the loaded journal's newest
/// JOURNAL_GEN below G''s first seq is G's, and the pages are at or below the journal file's end);
/// every other entry stays, its answer unknown, until a resend's refusal stamps it live. A command
/// carrying X after either would be applied twice.
///
/// Java keeps rings for a train's ops only (RP-190); every other op holds the same promise through
/// Java's journal: the journal's op index refuses an op Java applied, and forgets it when Java
/// trims the journal through the engine's acknowledgement. The checker models rings for every op,
/// a stricter rule.
///
/// Every case runs the iron line through the native engine with Java's autosave (a world save and
/// a page save asked through the journal's end) and [`AckCheck`] watching every op-carrying
/// industry command the engine sends.
mod ack_never_resends {
    use super::*;
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    use rtsim_industry::cmd::CarriageReason;
    use rtsim_industry::fake::{Outgoing, SavedWorld};
    use rtsim_industry::ids::TrainId;
    use rtsim_industry::wire as iw;
    use rtsim_jni::wire::{Records, cmd, obs};
    use rtsim_kernel::journal::OpId;

    const APPLIED: u32 = obs::command_result::APPLIED;
    const DUPLICATE: u32 = obs::command_result::DUPLICATE_OP;
    const FAILED: u32 = obs::command_result::FAILED;

    /// Java's rings as the checker models them: every op Java answered APPLIED or DUPLICATE and
    /// has not pruned, with the live answer it was stamped with in this session (the session's
    /// index and the answer's journal seq), or None when its answer is unknown (carried from an
    /// earlier session and not settled). A superset of the real rings, which keep only their own
    /// kinds: the checker holds every op-carrying command to the rule.
    type Ring = HashMap<u64, Option<(usize, u64)>>;

    struct Session {
        nonce: u64,
        /// The session's first record (its JOURNAL_GEN): its replay end plus one.
        first: u64,
        /// The pages at its start.
        pages: u64,
    }

    /// One of Java's live COMMAND_RESULTs for an industry op.
    #[derive(Clone, Copy, Debug)]
    struct Live {
        op: u64,
        seq: u64,
        session: usize,
        status: u32,
    }

    /// One op-carrying industry command in a command buffer the engine published.
    #[derive(Clone, Copy, Debug)]
    struct Arrival {
        session: usize,
        buffer: u64,
        cmd_seq: u64,
        op: u64,
        kind: &'static str,
        /// At or below Java's lastAppliedCmdSeq: Java skips it unapplied.
        stale: bool,
    }

    /// A world save: Java's journal file, the world (the fake world's goods, trains and its own
    /// never-pruned op set), and the checker's model of Java's rings saved with it.
    #[derive(Clone)]
    struct Saved {
        jsave: WorldSave,
        msave: SavedWorld,
        ring: Ring,
        gone: HashMap<u64, String>,
        /// The journal file's end (its last seq).
        end: u64,
    }

    /// What a restart's settle did with the ring it loaded.
    struct Start {
        settled: Vec<u64>,
        unknown: usize,
        writer: Option<usize>,
        provable: bool,
    }

    /// The checker every case uses. For every op-carrying industry command Java applied it records
    /// the seq S of Java's live COMMAND_RESULT and the session it was given in, and fails when
    /// - (a) a command carrying X arrives after the engine sent an ack >= S in that same session
    ///   (sent, whether Java read it or not; a command in the buffer that carries the ack is
    ///   applied before the ack, as BoundarySession does, so only later buffers count);
    /// - (b) a command carrying X arrives when nothing in Java knows X: in a later session whose
    ///   start settled X (pages >= S, provably G's), or after a restart from a world save whose
    ///   ring Java had already pruned of X on reading an ack.
    ///
    /// At every restart the pages must be at or past every ack the engine sent before the crash,
    /// read or not.
    #[derive(Default)]
    struct AckCheck {
        sessions: Vec<Session>,
        lives: Vec<Live>,
        arrivals: Vec<Arrival>,
        /// This session's live answers, by op.
        live_now: HashMap<u64, Vec<u64>>,
        ring: Ring,
        /// Ops nothing in Java knows any more, and why.
        gone: HashMap<u64, String>,
        /// The highest ack the engine sent in this session, before the buffer being read.
        max_ack: u64,
        /// Every ack the engine sent: (session, command buffer, ack).
        acks: Vec<(usize, u64, u64)>,
        /// Every ack Java read: (session, ack after Java's clamp).
        read: Vec<(usize, u64)>,
        /// Per session: ops pruned on an ack Java read, ops settled at its start.
        pruned: Vec<usize>,
        settled: Vec<usize>,
        violations: Vec<String>,
        /// A broken Java for the negative control: at a start, it keeps every ring entry the
        /// settle did not take with its old answer, as if given live in the new session, so the
        /// new session's acks prune on answers it only replayed.
        stamp_replayed: bool,
    }

    fn kind_of(c: &Cmd) -> &'static str {
        match c {
            Cmd::ChunkReconcile(_) => "reconcile",
            Cmd::CarriageDelta(d) if d.reason == CarriageReason::Loot => "loot",
            Cmd::CarriageDelta(_) => "exchange",
            Cmd::DepositDelta(_) => "deposit delta",
            Cmd::WritePackage(_) => "package",
            Cmd::WriteChain(_) => "chain package",
            Cmd::ProbeSet(_) | Cmd::CountSet(_) | Cmd::HeldDraw(_) => "latest-wins",
        }
    }

    impl AckCheck {
        fn new(j: &Java) -> Self {
            AckCheck {
                sessions: vec![Session {
                    nonce: j.nonce,
                    first: j.replay_up_to + 1,
                    pages: j.pages_obs,
                }],
                pruned: vec![0],
                settled: vec![0],
                ..Default::default()
            }
        }

        fn cur(&self) -> usize {
            self.sessions.len() - 1
        }

        /// Java journaled its answer for `op` at `seq`, live in this session.
        fn answered(&mut self, op: u64, seq: u64, status: u32) {
            let s = self.cur();
            self.lives.push(Live {
                op,
                seq,
                session: s,
                status,
            });
            if status == FAILED {
                return;
            }
            self.ring.insert(op, Some((s, seq)));
            self.live_now.entry(op).or_default().push(seq);
        }

        /// Reads the command buffer of the step Java just submitted, before Java applies it (the
        /// step published it and leaves it alone until Java consumes it): every op-carrying
        /// industry command in it against both rules, then the buffer's ack. Returns the ack.
        fn arrived(&mut self, j: &Java) -> u64 {
            let k = j.submitted;
            let b = j.cmd_buf(k);
            assert_eq!(
                b.published().load(Ordering::Acquire),
                k,
                "step {k} published"
            );
            let used = b.get_u32(wire::H_USED) as usize;
            let ack = b.get_u64(wire::H_WORD_A);
            // SAFETY: as Java::apply_commands: the step published k and does not touch this
            // buffer until Java stores consumed = k and submits k + 2.
            let area = unsafe { b.area(used) }.to_vec();
            let mut last = j.last_applied_cmd;
            let mut recs = Records::new(&area);
            for rec in &mut recs {
                if rec.kind == cmd::DICT {
                    continue;
                }
                // Java's own skip: at or below the last cmdSeq it applied (of any kind).
                let seq = wire::u64_at(rec.bytes, cmd::CMD_SEQ);
                let stale = seq <= last;
                if !stale {
                    last = seq;
                }
                if !(0x8400..=0x84FF).contains(&rec.kind) {
                    continue;
                }
                let Ok(Some((_, c))) = iw::decode_cmd(rec.bytes) else {
                    continue;
                };
                if let Some(op) = c.op() {
                    self.arrival(k, seq, op.0, kind_of(&c), stale);
                }
            }
            assert!(recs.error().is_none(), "{:?}", recs.error());
            self.acks.push((self.cur(), k, ack));
            self.max_ack = self.max_ack.max(ack);
            ack
        }

        fn arrival(&mut self, buffer: u64, cmd_seq: u64, op: u64, kind: &'static str, stale: bool) {
            let s = self.cur();
            let note = if stale { ", stale" } else { "" };
            if let Some(answers) = self.live_now.get(&op) {
                for &a in answers.iter().filter(|a| **a <= self.max_ack) {
                    self.violations.push(format!(
                        "(a) session {s}: {kind} op {op:#x} arrived in command buffer {buffer} \
                         (cmdSeq {cmd_seq}{note}) after the engine sent ack {} in this session, at \
                         or past the op's live answer at journal seq {a}",
                        self.max_ack
                    ));
                }
            }
            if let Some(why) = self.gone.get(&op) {
                self.violations.push(format!(
                    "(b) session {s}: {kind} op {op:#x} arrived in command buffer {buffer} \
                     (cmdSeq {cmd_seq}{note}), though nothing in Java knows it: {why}"
                ));
            }
            self.arrivals.push(Arrival {
                session: s,
                buffer,
                cmd_seq,
                op,
                kind,
                stale,
            });
        }

        /// Java read ack `a` (after its clamp): its rings prune every op whose live answer in this
        /// session is at or below it. Entries with an unknown answer stay.
        fn read_ack(&mut self, a: u64) {
            let s = self.cur();
            self.read.push((s, a));
            let pruned: Vec<(u64, u64)> = self
                .ring
                .iter()
                .filter_map(|(op, st)| match st {
                    Some((g, seq)) if *g == s && *seq <= a => Some((*op, *seq)),
                    _ => None,
                })
                .collect();
            for (op, seq) in pruned {
                self.ring.remove(&op);
                self.gone.insert(
                    op,
                    format!(
                        "Java pruned it in session {s} on reading ack {a}, at or past its live \
                         answer at {seq}"
                    ),
                );
                self.pruned[s] += 1;
            }
        }

        fn save(&self, j: &Java) -> Saved {
            Saved {
                jsave: j.world_save(),
                msave: j.mc().saved(),
                ring: self.ring.clone(),
                gone: self.gone.clone(),
                end: j.next_obs - 1,
            }
        }

        /// Every ack the engine sent before the crash, read or not: the highest.
        fn crash(&self, j: &Java) -> u64 {
            let sent = self.acks.iter().map(|a| a.2).max().unwrap_or(0);
            sent.max(j.e.stats.acked_sent.load(Ordering::Acquire))
        }

        /// The restart opened `j` on the world save `loaded`: the restart assertion, then Java's
        /// settle on the ring it loaded.
        fn restart(&mut self, j: &Java, loaded: &Saved, acked: u64) -> Start {
            let pages = j.pages_obs;
            assert!(
                pages >= acked,
                "the restart's pages ({pages}) are behind an ack the engine sent before the crash \
                 ({acked})"
            );
            let first = j.replay_up_to + 1;
            let end = loaded.jsave.next_obs - 1;
            assert_eq!(end, loaded.end);
            // The newest JOURNAL_GEN the loaded journal holds below this session's first record
            // (a trimmed one is pinned).
            let writer = loaded
                .jsave
                .pinned_gen
                .iter()
                .chain(&loaded.jsave.journal)
                .filter(|(s, r)| *s < first && wire::u16_at(r, wire::R_KIND) == obs::JOURNAL_GEN)
                .max_by_key(|(s, _)| *s)
                .map(|(_, r)| wire::u64_at(r, obs::journal_gen::GEN))
                .and_then(|n| self.sessions.iter().position(|x| x.nonce == n));
            let provable = pages <= end;
            let next = self.sessions.len();
            let mut ring = Ring::new();
            let mut gone = loaded.gone.clone();
            let mut settled = Vec::new();
            let mut replayed = Vec::new();
            for (&op, &stamp) in &loaded.ring {
                match stamp {
                    Some((g, seq)) if provable && Some(g) == writer && pages >= seq => {
                        gone.insert(
                            op,
                            format!(
                                "settled at session {next}'s start: its pages {pages} are at or \
                                 past its live answer at {seq}, and are session {g}'s"
                            ),
                        );
                        settled.push(op);
                    }
                    Some((_, seq)) if self.stamp_replayed => {
                        ring.insert(op, Some((next, seq)));
                        replayed.push((op, seq));
                    }
                    _ => {
                        ring.insert(op, None);
                    }
                }
            }
            let unknown = ring.values().filter(|st| st.is_none()).count();
            self.ring = ring;
            self.gone = gone;
            self.live_now.clear();
            for (op, seq) in replayed {
                self.live_now.entry(op).or_default().push(seq);
            }
            self.max_ack = 0;
            self.sessions.push(Session {
                nonce: j.nonce,
                first,
                pages,
            });
            self.pruned.push(0);
            self.settled.push(settled.len());
            eprintln!(
                "session {next} starts: pages {pages}, first seq {first}, journal file's end {end}, \
                 writer {writer:?}, provable {provable}; {} settled, {unknown} unknown, {} gone",
                settled.len(),
                self.gone.len()
            );
            Start {
                settled,
                unknown,
                writer,
                provable,
            }
        }

        fn lives_in(&self, s: usize) -> impl Iterator<Item = &Live> {
            self.lives
                .iter()
                .filter(move |l| l.session == s && l.status != FAILED)
        }

        fn arrivals_of(&self, op: u64, s: usize) -> Vec<Arrival> {
            self.arrivals
                .iter()
                .filter(|a| a.op == op && a.session == s)
                .copied()
                .collect()
        }

        /// The first command buffer of session `s` whose ack is at or past `seq`.
        fn first_ack_at_least(&self, s: usize, seq: u64) -> Option<u64> {
            self.acks
                .iter()
                .find(|a| a.0 == s && a.2 >= seq)
                .map(|a| a.1)
        }

        /// The run's summary, then no violation.
        fn finish(&self) {
            let mut kinds: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            let mut seen: HashMap<u64, usize> = HashMap::new();
            for a in &self.arrivals {
                let e = kinds.entry(a.kind).or_default();
                e.0 += 1;
                let n = seen.entry(a.op).or_default();
                if *n > 0 {
                    e.1 += 1;
                }
                *n += 1;
            }
            eprintln!(
                "ops: {} distinct; by kind (arrivals, again): {kinds:?}",
                seen.len()
            );
            for (i, s) in self.sessions.iter().enumerate() {
                let arrivals: Vec<&Arrival> =
                    self.arrivals.iter().filter(|a| a.session == i).collect();
                let stale = arrivals.iter().filter(|a| a.stale).count();
                let earlier = arrivals
                    .iter()
                    .filter(|a| {
                        self.arrivals.iter().any(|b| {
                            b.op == a.op
                                && (b.session < i || (b.session == i && b.buffer < a.buffer))
                        })
                    })
                    .count();
                let status = |st| {
                    self.lives
                        .iter()
                        .filter(|l| l.session == i && l.status == st)
                        .count()
                };
                let sent = self.acks.iter().filter(|a| a.0 == i).map(|a| a.2).max();
                let read = self.read.iter().filter(|a| a.0 == i).map(|a| a.1).max();
                eprintln!(
                    "session {i} (nonce {:#x}, first {}, pages {}): {} op commands ({stale} \
                     stale, {earlier} carrying an op sent before), live answers {} applied {} \
                     duplicate {} failed, acks sent up to {sent:?}, read up to {read:?}, {} \
                     pruned on a read ack, {} settled at its start",
                    s.nonce,
                    s.first,
                    s.pages,
                    arrivals.len(),
                    status(APPLIED),
                    status(DUPLICATE),
                    status(FAILED),
                    self.pruned[i],
                    self.settled[i],
                );
            }
            assert!(
                self.violations.is_empty(),
                "the engine sent an op again after acknowledging its live answer:\n{}",
                self.violations.join("\n")
            );
        }
    }

    /// The iron line, Java and the checker, with Java's autosave every `every` world ticks
    /// (0: none).
    struct Rig {
        dir: PathBuf,
        cfg: IronLine,
        j: Java,
        c: AckCheck,
        every: u64,
        last: Option<Saved>,
        repaired: bool,
        /// The buffer capacity and the module mask an engine restarts with ([`Rig::restart`]).
        capacity: usize,
        modules: u32,
    }

    impl Rig {
        fn new(tag: &str, threats: bool) -> Rig {
            let cfg = IronLine {
                seed: 11,
                days: 10,
                threats,
                control: false,
                tick_first: true,
                approach: true,
            };
            let dir = tmpdir(tag);
            let j = iron_java(&dir, cfg);
            let c = AckCheck::new(&j);
            Rig {
                dir,
                cfg,
                j,
                c,
                every: 3_000,
                last: None,
                repaired: false,
                capacity: 256 * 1024,
                modules: MODULE_INDUSTRY,
            }
        }

        /// One server tick in lockstep, as `Java::mc_tick`, with the checker watching: Java's
        /// answers are journaled here, so the checker has each one's seq; `before` runs after them
        /// and before the hand-over; the step's command buffer is read by the checker before Java
        /// applies it, and `apply` given its ack (and Java and the checker, to journal or save
        /// there) says whether Java does (false: Java dies first). Returns the buffer's ack and
        /// whether Java applied it.
        fn step(
            &mut self,
            apply: impl FnOnce(&mut Java, &mut AckCheck, u64) -> bool,
            before: impl FnOnce(&mut Java, &[(u64, u64, u32)]),
        ) -> (u64, bool) {
            let (j, c) = (&mut self.j, &mut self.c);
            let mc = j.mc.as_mut().expect("an industry world");
            mc.tick();
            let t = mc.t;
            let out = std::mem::take(&mut mc.outgoing);
            let threats = std::mem::take(&mut mc.threats_out);
            j.game_time = t.0 as i64;
            for th in threats {
                j.e.order(vec![Order::IndustryThreat(th)])
                    .expect("a threat order");
            }
            let mut answers = Vec::new();
            for o in out {
                match o {
                    Outgoing::Obs(t) => j.industry_obs(&t),
                    Outgoing::CommandResult { op, status } => {
                        let op = op.0;
                        let cmd_seq = j.ind_cmd_seq.get(&op).copied().unwrap_or(0);
                        let s = j.journal(
                            obs::COMMAND_RESULT,
                            obs::command_result::SIZE,
                            status,
                            |r| {
                                wire::put_u64(r, obs::command_result::CMD_SEQ, cmd_seq);
                                wire::put_u64(r, obs::command_result::OP_ID, op);
                            },
                        );
                        c.answered(op, s, status);
                        answers.push((op, s, status));
                    }
                }
            }
            before(j, &answers);
            let gt = j.game_time;
            assert!(j.tick(), "a lockstep tick always submits");
            j.game_time = gt;
            let deadline = Instant::now() + Duration::from_secs(20);
            while j.e.stats_array()[1] < j.submitted {
                assert!(
                    Instant::now() < deadline,
                    "step {} never finished",
                    j.submitted
                );
                std::hint::spin_loop();
                std::thread::yield_now();
            }
            let ack = c.arrived(j);
            if !apply(j, c, ack) {
                return (ack, false);
            }
            let read = j.acks.len();
            j.apply_commands();
            assert_eq!(j.next_cmd, j.submitted + 1);
            if j.acks.len() > read {
                c.read_ack(*j.acks.last().unwrap());
            }
            (ack, true)
        }

        fn tick(&mut self) {
            self.step(|_, _, _| true, |_, _| {});
            self.after_tick();
        }

        /// The player repairs the coast as `run_iron_line` does; Java autosaves on its period.
        fn after_tick(&mut self) {
            let t = self.j.mc().t;
            let end = Tick(self.cfg.days * TICKS_PER_DAY);
            if !self.repaired && t >= day(8.6) && t < end && self.j.mc().districts[ORE_COAST].loaded
            {
                self.j.mc_mut().repair_all(ORE_COAST);
                self.repaired = true;
            }
            if self.every > 0 && t.0.is_multiple_of(self.every) {
                self.autosave();
            }
        }

        /// Ticks until `done`; fails past world time `limit` or 60 s of wall clock.
        fn run(&mut self, what: &str, limit: Tick, mut done: impl FnMut(&Rig) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !done(self) {
                assert!(Instant::now() < deadline, "{what}: 60 s of wall clock");
                assert!(self.j.mc().t < limit, "{what}: not by {limit:?}");
                self.tick();
            }
        }

        fn run_to(&mut self, t: Tick) {
            self.run("the world's clock", t + 1, |r| r.j.mc().t >= t);
        }

        /// Java's autosave: the world saves, and asks the engine for its pages through the
        /// journal's end.
        fn autosave(&mut self) -> Saved {
            let s = self.c.save(&self.j);
            self.j.e.request_save(s.end);
            self.last = Some(s.clone());
            s
        }

        /// Runs until Java has read an ack at or past `seq`.
        fn until_read(&mut self, seq: u64) {
            let limit = self.j.mc().t + 24_000;
            self.run("Java reads the ack", limit, |r| {
                r.j.acks.last().is_some_and(|a| *a >= seq)
            });
        }

        /// A crash (`close(false)`: queued saves dropped) and a restart on the same pages from
        /// the world save `saved`, on `capacity` with `modules`, at the world's present
        /// (`Java::open` starts the game time at 1,000).
        fn restart(&mut self, saved: &Saved) -> Start {
            let acked = self.c.crash(&self.j);
            let mut mc = self.j.mc.take().expect("an industry world");
            self.j.e.close(false, Duration::from_secs(5)).unwrap();
            mc.restore(&saved.msave);
            let seed = self.cfg.seed;
            let e = engine_with(&self.dir, self.capacity, |c| c.industry_seed = Some(seed));
            let mut j = Java::open(e, saved.jsave.clone());
            j.game_time = mc.t.0 as i64;
            j.mc = Some(mc);
            j.module_mask = self.modules;
            self.j = j;
            self.last = None;
            self.repaired = false;
            self.c.restart(&self.j, saved, acked)
        }

        /// A crash after Java read the ack of an autosave, restarting from that autosave.
        fn crash_after_a_read_ack(&mut self) -> Start {
            let w = self.autosave();
            self.until_read(w.end);
            self.restart(&w)
        }

        /// To the iron line's end and until every command is applied, then the checker and the
        /// books.
        fn finish(self) {
            self.finish_with(|_, _| {});
        }

        /// [`Rig::finish`], with the case's own checks on the checker's record (and the last
        /// session's Java) before it closes.
        fn finish_with(mut self, extra: impl FnOnce(&AckCheck, &Java)) {
            let end = Tick(self.cfg.days * TICKS_PER_DAY);
            self.run("the iron line's end", end + 2_001, |r| {
                r.j.mc().t >= end + 2_000
            });
            self.run("every command applied", end + 4_000, |r| {
                r.j.mc().commands_in_flight() == 0
            });
            self.c.finish();
            extra(&self.c, &self.j);
            let link = self.j.e.with_industry(|_, l| *l);
            eprintln!("link: {link:?}");
            assert_eq!(link.outbox, 0, "every kept command answered");
            assert!(
                self.j.e.protocol_error().is_none(),
                "{:?}",
                self.j.e.protocol_error()
            );
            conserves(&self.j);
            self.j.e.close(true, Duration::from_secs(5)).unwrap();
        }
    }

    /// Case 1, the crash before the page save that covers S: Java's autosaves stop at day 3, the
    /// last one's pages land, and the line runs on until ten answers are past them; the world saves
    /// (its journal holds the answers) and the engine crashes before any page save covers them.
    /// The engine sent no ack at or past any of them, the restart's pages are behind them, and the
    /// settle keeps each one's answer unknown: a resend or a re-decision of those ops is allowed.
    /// The second session then runs with autosaves, crashes after a read ack, and the third runs
    /// to the end, the checker on every op throughout.
    #[test]
    fn a_crash_before_the_page_save_that_covers_an_answer_may_send_its_op_again() {
        let mut r = Rig::new("ack-c1", true);
        r.run_to(day(3.0));
        r.every = 0;
        let w0 = r.last.clone().expect("an autosave by day 3");
        r.run("the last autosave's pages", day(4.0), |r| {
            r.j.e.durable_obs_seq() >= w0.end
        });
        let pages = r.j.e.durable_obs_seq();
        let past = |r: &Rig| r.c.lives_in(0).filter(|l| l.seq > pages).count();
        r.run("ten answers past the pages", day(6.0), |r| past(r) >= 10);
        let w = r.c.save(&r.j);
        assert_eq!(r.j.e.durable_obs_seq(), pages, "no page save since");
        let answers: Vec<Live> = r.c.lives_in(0).filter(|l| l.seq > pages).copied().collect();
        let sent = r.c.crash(&r.j);
        for l in &answers {
            assert!(
                sent < l.seq,
                "the engine sent ack {sent} before the page save covering op {:#x}'s answer at {}",
                l.op,
                l.seq
            );
        }
        let start = r.restart(&w);
        assert_eq!(start.writer, Some(0));
        assert!(start.provable);
        for l in &answers {
            assert!(
                l.seq > r.j.pages_obs,
                "the restart's pages are behind op {:#x}'s answer",
                l.op
            );
            assert!(
                !start.settled.contains(&l.op),
                "op {:#x} stays unknown",
                l.op
            );
        }
        r.every = 3_000;
        r.run_to(day(6.0));
        let again = answers
            .iter()
            .filter(|l| !r.c.arrivals_of(l.op, 1).is_empty())
            .count();
        eprintln!(
            "case 1: {} answers past the pages at the crash; {again} of their ops sent again in \
             session 1",
            answers.len()
        );
        let start = r.crash_after_a_read_ack();
        assert!(
            !start.settled.is_empty(),
            "the second restart settled what it read"
        );
        r.finish();
    }

    /// Case 2, the crash between the save and the ack, with Java holding the acks (`hold_acks`):
    /// from day 3 Java reads no acknowledgement; ten answers come past the last ack it read; an
    /// autosave asks for pages through its journal's end and the engine sends an ack at or past
    /// it, which Java never reads. The line runs on a quarter of a day in that session (rule (a)
    /// watches every later buffer). The restart from that world save has pages at or past every
    /// such answer and provably the first session's, so the settle takes each op: none is ever
    /// sent again, through two more sessions.
    #[test]
    fn an_answer_acknowledged_while_java_held_the_acks_is_never_sent_again() {
        let mut r = Rig::new("ack-c2-held", true);
        r.run_to(day(3.0));
        r.every = 0;
        r.j.hold_acks = true;
        let a0 = r.j.acks.last().copied().unwrap_or(0);
        let read = r.j.acks.len();
        let past = |r: &Rig| r.c.lives_in(0).filter(|l| l.seq > a0).count();
        r.run("ten answers past the last ack Java read", day(6.0), |r| {
            past(r) >= 10
        });
        let w = r.autosave();
        r.run("the engine sends the save's ack", day(6.0), |r| {
            r.c.max_ack >= w.end
        });
        let unread: Vec<Live> =
            r.c.lives_in(0)
                .filter(|l| l.seq > a0 && l.seq <= w.end)
                .copied()
                .collect();
        assert!(unread.len() >= 10);
        let t = r.j.mc().t;
        r.run_to(t + 6_000);
        assert_eq!(r.j.acks.len(), read, "Java read no ack since it held them");
        let start = r.restart(&w);
        assert_eq!(start.writer, Some(0));
        assert!(start.provable);
        for l in &unread {
            assert!(r.j.pages_obs >= l.seq);
            assert!(
                start.settled.contains(&l.op),
                "op {:#x} (answered at {}) settled at the restart",
                l.op,
                l.seq
            );
        }
        eprintln!(
            "case 2 (held): {} answers acknowledged and never read; {} settled, {} unknown",
            unread.len(),
            start.settled.len(),
            start.unknown
        );
        r.every = 3_000;
        r.run_to(day(6.0));
        r.crash_after_a_read_ack();
        r.finish();
    }

    /// Case 2, the crash between the save and the ack, with Java dying before it applies the
    /// command buffer that carries the ack: ten answers past the pages, an autosave, and the line
    /// runs (Java reading every earlier buffer) until the first buffer whose ack is at or past the
    /// save; the crash comes before Java applies it. The answers above the last ack Java read are
    /// settled at the restart, and none of their ops is ever sent again.
    #[test]
    fn an_answer_acknowledged_in_a_buffer_java_never_read_is_never_sent_again() {
        let mut r = Rig::new("ack-c2-unread", true);
        r.run_to(day(3.0));
        r.every = 0;
        let pages = r.j.e.durable_obs_seq();
        let past = |r: &Rig| r.c.lives_in(0).filter(|l| l.seq > pages).count();
        r.run("ten answers past the pages", day(6.0), |r| past(r) >= 10);
        let w = r.autosave();
        let deadline = Instant::now() + Duration::from_secs(60);
        let unread_ack = loop {
            let (ack, applied) = r.step(|_, _, ack| ack < w.end, |_, _| {});
            if !applied {
                break ack;
            }
            r.after_tick();
            assert!(Instant::now() < deadline, "the save's ack never came");
            assert!(r.j.mc().t < day(6.0));
        };
        let a0 = r.j.acks.last().copied().unwrap_or(0);
        assert!(a0 < w.end, "Java never read an ack covering the save");
        let unread: Vec<Live> =
            r.c.lives_in(0)
                .filter(|l| l.seq > a0 && l.seq <= w.end)
                .copied()
                .collect();
        assert!(!unread.is_empty());
        let start = r.restart(&w);
        assert!(r.j.pages_obs >= unread_ack);
        assert_eq!(start.writer, Some(0));
        assert!(start.provable);
        for l in &unread {
            assert!(
                start.settled.contains(&l.op),
                "op {:#x} (answered at {}) settled at the restart",
                l.op,
                l.seq
            );
        }
        eprintln!(
            "case 2 (unread buffer): ack {unread_ack} never read; {} answers above the last read \
             ack {a0}; {} settled, {} unknown",
            unread.len(),
            start.settled.len(),
            start.unknown
        );
        r.every = 3_000;
        r.run_to(day(6.0));
        r.crash_after_a_read_ack();
        r.finish();
    }

    /// Case 3, the crash after Java read the ack: the line runs four and a half days with
    /// autosaves (Java prunes on each ack it reads, and every later buffer of the session is
    /// checked against rule (a)), then crashes right after Java read the ack of an autosave and
    /// restarts from that world save (taken before the read: the settle takes its answers). The
    /// second session runs two more days with autosaves, Java reads the ack of one, saves its world
    /// again after that read (its rings pruned), runs on with no page save, and crashes; the third
    /// session restarts from that later world save with pages behind it, so every op Java pruned
    /// before it is unknown to Java for good. No pruned or settled op is ever sent again.
    #[test]
    fn an_answer_whose_acknowledgement_java_read_is_never_sent_again() {
        let mut r = Rig::new("ack-c3", true);
        r.run_to(day(4.5));
        assert!(r.c.pruned[0] > 0, "Java pruned on the acks it read");
        let start = r.crash_after_a_read_ack();
        assert!(
            !start.settled.is_empty(),
            "the restart settled the answers it read"
        );
        r.run_to(day(6.5));
        r.every = 0;
        let wa = r.autosave();
        r.until_read(wa.end);
        let pruned = r.c.gone.len();
        let wb = r.c.save(&r.j);
        let t = r.j.mc().t;
        r.run_to(t + 3_000);
        assert_eq!(
            r.j.e.durable_obs_seq(),
            wa.end,
            "no page save after the autosave's"
        );
        let start = r.restart(&wb);
        assert_eq!(
            r.j.pages_obs, wa.end,
            "the pages are behind the later world save"
        );
        assert!(start.provable);
        for (op, why) in &wb.gone {
            assert!(
                !r.c.ring.contains_key(op),
                "op {op:#x} ({why}) is not in the rings the restart loaded"
            );
        }
        eprintln!(
            "case 3: {} ops pruned or settled in the first two sessions, carried in the later \
             world save",
            pruned
        );
        r.every = 3_000;
        r.finish();
    }

    /// Case 4, the replayed answer, as a control (slice T's j1-late shape). The train raid's loot X
    /// is answered APPLIED live at S in session 1; the pages stop before the raid; the world saves
    /// after it and the engine crashes. Session 2 opens with pages behind S, replays S, and its
    /// first durable save sends an ack past S, which Java reads; the raider, back from its older
    /// pages, then decides the raid again, and the engine sends X again, after that ack. That is
    /// allowed: S was not given live in session 2, so Java's rings never prune on it (the settle
    /// kept X with its answer unknown). This is exactly why Java registers acknowledgements only
    /// for its own live answers: had session 2's ack past S pruned X, the raid decided again would
    /// be applied twice. Java refuses the re-decided X with DUPLICATE live at S2 in session 2;
    /// after the ack past S2, and across a crash after it, X is never sent again.
    #[test]
    fn a_replayed_answer_acknowledged_is_sent_again_and_its_refusal_never_after() {
        replayed_answer(false);
    }

    /// The negative control of case 4: the checker under a Java that registered acknowledgements
    /// for answers it only replayed (its rings stamping X with S again in session 2) flags the
    /// same engine run, because session 2's ack past S would have pruned X before the raid was
    /// decided again, and the re-decided X would have been applied twice.
    #[test]
    fn the_checker_flags_the_replayed_answer_under_a_java_that_registers_replayed_acks() {
        replayed_answer(true);
    }

    fn replayed_answer(stamp_replayed: bool) {
        let mut r = Rig::new(
            if stamp_replayed {
                "ack-c4-broken"
            } else {
                "ack-c4"
            },
            false,
        );
        r.c.stamp_replayed = stamp_replayed;
        r.run_to(day(3.0));
        r.every = 0;
        let train = TrainId(1);
        let laden = |j: &Java| {
            industry(&j.e, |ie| {
                ie.legs()
                    .leg_of(train, ie.now())
                    .and_then(|id| ie.legs().get(id))
                    .is_some_and(|l| l.cargo.iter().any(|c| !c.1.is_empty()))
            })
        };
        let loot = |r: &Rig, s: usize| -> Vec<Arrival> {
            r.c.arrivals
                .iter()
                .filter(|a| a.session == s && a.kind == "loot")
                .copied()
                .collect()
        };
        // The pages stop while the train is under way with cargo, before the raid.
        loop {
            r.run("train 1 under way with cargo", day(8.0), |r| laden(&r.j));
            let through = r.j.next_obs - 1;
            r.j.e.request_save(through);
            r.run("the pages", day(8.0), |r| {
                r.j.e.durable_obs_seq() >= through
            });
            if laden(&r.j) {
                break;
            }
        }
        let through = r.j.e.durable_obs_seq();
        let raid = Threat {
            id: 0xA11,
            source: ThreatSource::Bandit,
            class: ThreatClass::Siege,
            count: 3,
            strength: 3.0,
            target: ThreatTarget::Train(train),
            loads: None,
            at: r.j.mc().t,
        };
        r.j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
        r.run("the raid's loot is written", raid.at + 100, |r| {
            !loot(r, 0).is_empty()
        });
        let x = loot(&r, 0)[0].op;
        r.run("the world answers the loot", raid.at + 100, |r| {
            r.c.lives_in(0).any(|l| l.op == x)
        });
        let s1 = *r.c.lives_in(0).find(|l| l.op == x).unwrap();
        assert_eq!(s1.status, APPLIED);
        // The world saves after the raid; the pages never save again before the crash.
        let w = r.c.save(&r.j);
        assert_eq!(r.j.e.durable_obs_seq(), through, "no later page save");
        assert!(r.c.crash(&r.j) < s1.seq, "no ack at or past S in session 1");
        let start = r.restart(&w);
        assert!(r.j.pages_obs < s1.seq, "the pages are behind S");
        assert!(!start.settled.contains(&x));
        if !stamp_replayed {
            assert_eq!(r.c.ring.get(&x), Some(&None), "X stays, its answer unknown");
        }
        r.tick();
        // The first durable save past the replay end acknowledges the raid's old answer.
        let through = r.j.next_obs - 1;
        assert!(through >= s1.seq);
        r.j.e.request_save(through);
        r.run("the engine acks past S and Java reads it", day(9.0), |r| {
            r.j.acks.last().is_some_and(|a| *a >= s1.seq)
        });
        let acked_at = r.c.first_ack_at_least(1, s1.seq).expect("an ack past S");
        assert!(loot(&r, 1).is_empty(), "nothing decided the raid again yet");
        if stamp_replayed {
            assert!(
                r.c.gone.contains_key(&x),
                "the broken Java pruned X on the replayed answer"
            );
        } else {
            assert_eq!(r.c.ring.get(&x), Some(&None), "Java's rings kept X");
        }
        // The raider decides the raid again.
        r.j.e.order(vec![Order::IndustryThreat(raid)]).unwrap();
        r.run("the raid is decided again", raid.at + 400, |r| {
            !loot(r, 1).is_empty()
        });
        let again = r.c.arrivals_of(x, 1);
        assert_eq!(again.len(), 1, "X sent once in session 2: {again:?}");
        if stamp_replayed {
            let flagged: Vec<&String> =
                r.c.violations
                    .iter()
                    .filter(|v| v.contains(&format!("{x:#x}")))
                    .collect();
            eprintln!("the broken Java's checker: {flagged:#?}");
            assert!(
                flagged.iter().any(|v| v.starts_with("(a)"))
                    && flagged.iter().any(|v| v.starts_with("(b)")),
                "the checker flags X's re-decision under rules (a) and (b)"
            );
            r.j.e.close(true, Duration::from_secs(5)).unwrap();
            return;
        }
        assert!(
            again[0].buffer > acked_at,
            "the engine sends X after an ack past S (buffer {} after {acked_at})",
            again[0].buffer
        );
        eprintln!(
            "case 4: X {x:#x} answered at S {} in session 1; session 2's ack past S in buffer \
             {acked_at}; X sent again in buffer {} as cmdSeq {}",
            s1.seq, again[0].buffer, again[0].cmd_seq
        );
        r.run("Java refuses X", raid.at + 400, |r| {
            r.c.lives_in(1).any(|l| l.op == x)
        });
        let s2 = *r.c.lives_in(1).find(|l| l.op == x).unwrap();
        assert_eq!(
            s2.status, DUPLICATE,
            "the ring refused the re-decided X, live in session 2"
        );
        // Java's autosave again: the ack past S2 prunes X, and the line runs on two days.
        r.every = 3_000;
        r.until_read(s2.seq);
        assert!(r.c.gone.contains_key(&x), "pruned on the ack past S2");
        r.run_to(day(6.0));
        r.crash_after_a_read_ack();
        r.finish_with(|c, _| {
            assert_eq!(c.arrivals_of(x, 0).len(), 1);
            assert_eq!(c.arrivals_of(x, 1).len(), 1);
            assert!(
                c.arrivals_of(x, 2).is_empty(),
                "never sent after the ack past S2"
            );
        });
    }

    /// Case 5, a replayed DUPLICATE that meets the copy the held replay decided again (RP-312).
    /// J3 on the ring (RP-199): the world applies a station exchange X live in session 0 and
    /// saves its goods and rings with X, its journal file from before X's answer S; the pages stop
    /// before X's decision. Session 1's replay decides X again (it heard no answer for X: S went
    /// with the journal), the engine sends it after the replay end, and the ring refuses it
    /// DUPLICATE live at S2. The world saves again, S2 in its journal, and the pages still never
    /// save. Session 2's replay decides X again, the copy waiting in the engine's commands (the
    /// hold, RP-274), and then hears S2, X's first answer in that replay, so S2 reaches
    /// `command_result` while replaying. S2 is not live in session 2: the settle kept X with its
    /// answer unknown, and only the acknowledgement of a live answer prunes it, so the copy must
    /// be sent (RP-310). Taken out there, as A's `take_out` would, X is never sent in session 2
    /// and stays in Java's rings with its answer unknown for good (RP-250's leftovers).
    #[test]
    fn a_replayed_duplicate_leaves_the_copy_the_replay_decided_again_to_be_sent() {
        let mut r = Rig::new("ack-c5-dup", false);
        r.run_to(day(2.0));
        r.every = 0;
        let exchanges = |r: &Rig| -> Vec<u64> {
            r.c.arrivals
                .iter()
                .filter(|a| a.session == 0 && a.kind == "exchange")
                .map(|a| a.op)
                .collect()
        };
        // The pages stop past the first simulated exchanges; then the next exchange, X.
        let through = r.j.next_obs - 1;
        r.j.e.request_save(through);
        r.run("the pages", day(8.0), |r| {
            r.j.e.durable_obs_seq() >= through
        });
        let through = r.j.e.durable_obs_seq();
        let before = exchanges(&r).len();
        r.run("a station exchange is written", day(8.0), |r| {
            exchanges(r).len() > before
        });
        let x = exchanges(&r)[before];
        // Java's journal file with the arrival, before the world applies X.
        let journal = r.c.save(&r.j);
        assert!(!r.j.mc().applied_ops().contains(&OpId(x)));
        r.run("the world answers X", day(8.0), |r| {
            r.c.lives_in(0).any(|l| l.op == x)
        });
        let s1 = *r.c.lives_in(0).find(|l| l.op == x).unwrap();
        assert_eq!(s1.status, APPLIED);
        // J3: the goods and the rings saved with X, the journal file from before S.
        let j3 = Saved {
            jsave: journal.jsave,
            end: journal.end,
            ..r.c.save(&r.j)
        };
        assert_eq!(r.j.e.durable_obs_seq(), through, "no later page save");
        r.restart(&j3);
        assert!(r.j.pages_obs < s1.seq, "the pages are behind S");
        assert_eq!(r.c.ring.get(&x), Some(&None), "X stays, its answer unknown");
        // Session 1's replay decides X again; the ring refuses the copy live.
        let t1 = r.j.mc().t;
        r.run("X sent again in session 1", t1 + 400, |r| {
            !r.c.arrivals_of(x, 1).is_empty()
        });
        r.run("the ring refuses X", t1 + 400, |r| {
            r.c.lives_in(1).any(|l| l.op == x)
        });
        let s2 = *r.c.lives_in(1).find(|l| l.op == x).unwrap();
        assert_eq!(
            s2.status, DUPLICATE,
            "the ring refused X, live in session 1"
        );
        // The world saves with S2 in its journal; the pages never save past X's decision.
        let w = r.c.save(&r.j);
        assert_eq!(r.j.e.durable_obs_seq(), through, "no later page save");
        assert!(
            r.c.crash(&r.j) < s2.seq,
            "no ack at or past S2 in session 1"
        );
        r.restart(&w);
        assert!(r.j.pages_obs < s2.seq, "the pages are behind S2");
        assert_eq!(r.c.ring.get(&x), Some(&None), "X stays, its answer unknown");
        // Session 2's replay decides X again and hears S2; the copy goes after the replay end.
        let t2 = r.j.mc().t;
        r.run_to(t2 + 400);
        let sent = r.c.arrivals_of(x, 2);
        assert_eq!(
            sent.len(),
            1,
            "X, decided again by the replay that heard its DUPLICATE, sent once in session 2: \
             {sent:?}"
        );
        r.run("the ring refuses X again", t2 + 800, |r| {
            r.c.lives_in(2).any(|l| l.op == x)
        });
        let s3 = *r.c.lives_in(2).find(|l| l.op == x).unwrap();
        assert_eq!(
            s3.status, DUPLICATE,
            "the ring refused X, live in session 2"
        );
        assert!(
            !industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "X's exchange is dropped"
        );
        eprintln!(
            "case 5: X {x:#x} applied at S {} in session 0 (J3: the journal from before it), \
             refused at S2 {} in session 1, replayed in session 2 and sent there as cmdSeq {}, \
             refused at S3 {}",
            s1.seq, s2.seq, sent[0].cmd_seq, s3.seq
        );
        // Java's autosave again: the ack past S3 prunes X.
        r.every = 3_000;
        r.until_read(s3.seq);
        assert!(r.c.gone.contains_key(&x), "pruned on the ack past S3");
        r.finish_with(|c, _| {
            assert_eq!(c.arrivals_of(x, 2).len(), 1);
            assert!(!c.ring.contains_key(&x), "no ring keeps X");
        });
    }

    /// Case 5 with the pages holding the copy, restarted with industry off (RP-313, the build
    /// lead's P2-1 on RP-310 to RP-312). The copy can only be the pages' there: the replay with the
    /// module off ignores the arrivals (U10), so it decides no exchange again. Session 0 is case
    /// 5's J3: the world applies a station exchange X live and saves its goods and rings with X,
    /// its journal file from before X's answer; the pages stop before X's decision. Session 1 asks
    /// for its pages at once after its restart, with records right behind the journal's end, so
    /// they are taken inside the replay end's hand-over before the first drain: they hold the copy
    /// of X the replay decided again, unsent, as a command waiting for a Java name or a full buffer
    /// would. The copy goes after that drain and the ring refuses it DUPLICATE live at S2; with
    /// `pad`, a long run of records the industry ignores follows S2. The world saves, and the
    /// engine crashes with no page save past those pages and no ack at or past S2. Session 2
    /// restarts from them with industry off, on `capacity`. Returns the rig, X and S2.
    fn a_duplicate_restarted_off(tag: &str, pad: bool, capacity: usize) -> (Rig, u64, Live) {
        let mut r = Rig::new(tag, false);
        r.run_to(day(2.0));
        r.every = 0;
        let exchanges = |r: &Rig| -> Vec<u64> {
            r.c.arrivals
                .iter()
                .filter(|a| a.session == 0 && a.kind == "exchange")
                .map(|a| a.op)
                .collect()
        };
        let through = r.j.next_obs - 1;
        r.j.e.request_save(through);
        r.run("the pages", day(8.0), |r| {
            r.j.e.durable_obs_seq() >= through
        });
        let before = exchanges(&r).len();
        r.run("a station exchange is written", day(8.0), |r| {
            exchanges(r).len() > before
        });
        let x = exchanges(&r)[before];
        let journal = r.c.save(&r.j);
        assert!(!r.j.mc().applied_ops().contains(&OpId(x)));
        r.run("the world answers X", day(8.0), |r| {
            r.c.lives_in(0).any(|l| l.op == x)
        });
        let s1 = *r.c.lives_in(0).find(|l| l.op == x).unwrap();
        assert_eq!(s1.status, APPLIED);
        let j3 = Saved {
            jsave: journal.jsave,
            end: journal.end,
            ..r.c.save(&r.j)
        };
        r.restart(&j3);
        assert!(r.j.pages_obs < s1.seq, "the pages are behind S");
        // Session 1's pages P1, inside the replay end's hand-over.
        let p1 = r.j.next_obs - 1;
        r.j.e.request_save(p1);
        pad_journal(&mut r.j, 40);
        let t1 = r.j.mc().t;
        r.run("the pages P1", t1 + 400, |r| r.j.e.durable_obs_seq() >= p1);
        assert_eq!(r.j.e.durable_obs_seq(), p1);
        r.run("X sent again in session 1", t1 + 400, |r| {
            !r.c.arrivals_of(x, 1).is_empty()
        });
        r.run("the ring refuses X", t1 + 400, |r| {
            r.c.lives_in(1).any(|l| l.op == x)
        });
        let s2 = *r.c.lives_in(1).find(|l| l.op == x).unwrap();
        assert_eq!(
            s2.status, DUPLICATE,
            "the ring refused X, live in session 1"
        );
        if pad {
            pad_journal(&mut r.j, PAD);
        }
        let w = r.c.save(&r.j);
        assert_eq!(r.j.e.durable_obs_seq(), p1, "no page save past P1");
        assert!(
            r.c.crash(&r.j) < s2.seq,
            "no ack at or past S2 in session 1"
        );
        r.capacity = capacity;
        r.modules = 0;
        r.restart(&w);
        assert_eq!(r.j.pages_obs, p1, "session 2 restarts from P1");
        assert_eq!(r.c.ring.get(&x), Some(&None), "X stays, its answer unknown");
        (r, x, s2)
    }

    /// After the switch in [`a_duplicate_restarted_off`]: X sent once in session 2 and refused
    /// DUPLICATE live, its exchange dropped; the ack past that answer prunes it, and the line runs
    /// to its end with no ring keeping X.
    fn the_copy_goes_once_after_the_switch(mut r: Rig, x: u64) {
        let t = r.j.mc().t;
        r.run("X sent in session 2", t + 400, |r| {
            !r.c.arrivals_of(x, 2).is_empty()
        });
        r.run("the ring refuses X again", t + 800, |r| {
            r.c.lives_in(2).any(|l| l.op == x)
        });
        let s3 = *r.c.lives_in(2).find(|l| l.op == x).unwrap();
        assert_eq!(
            s3.status, DUPLICATE,
            "the ring refused X, live in session 2"
        );
        assert!(
            !industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "X's exchange is dropped"
        );
        r.every = 3_000;
        r.until_read(s3.seq);
        assert!(r.c.gone.contains_key(&x), "pruned on the ack past S3");
        r.finish_with(|c, _| {
            assert_eq!(c.arrivals_of(x, 2).len(), 1);
            assert!(!c.ring.contains_key(&x), "no ring keeps X");
        });
    }

    /// [`a_duplicate_restarted_off`], switched on after the replay. Off, the replay brings S2 (X's
    /// first answer in it, so `keep_answer`), which waits for the switch. Before RP-313 S2 also
    /// took the copy out of the engine's commands (`unqueue`), so after the switch X was never sent
    /// and Java's rings kept X with its answer unknown for good; and the switch booked the kept S2
    /// with `false` for the replay, which takes it too.
    #[test]
    fn a_replayed_duplicate_heard_with_industry_off_leaves_the_copy_for_the_switch() {
        let (mut r, x, _) = a_duplicate_restarted_off("ack-c5-off", false, 256 * 1024);
        let t2 = r.j.mc().t;
        r.run("the replay ends", t2 + 100, |r| {
            r.j.e.stats.replays_ended.load(Ordering::Acquire) > 0
        });
        assert!(
            industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "off, S2 waits for the switch"
        );
        assert!(r.c.arrivals_of(x, 2).is_empty(), "nothing goes while off");
        switch_on(&mut r);
        the_copy_goes_once_after_the_switch(r, x);
    }

    /// [`a_duplicate_restarted_off`] on a minimum-size buffer, a long run of records behind S2, and
    /// the switch on at the front of a later hand-over of the same replay: MODULES(on) comes after
    /// the hand-over that carried S2 and ahead of more of the replay, so the switch books S2 while
    /// the engine still replays (`set_enabled` while `replaying`). The kept S2 was replayed and is
    /// booked as replayed: its copy stays. Booked with `false` for the replay, it took the copy.
    #[test]
    fn a_switch_on_inside_a_later_replay_hand_over_leaves_the_replayed_duplicates_copy() {
        let (mut r, x, s2) = a_duplicate_restarted_off("ack-c5-off-mid", true, wire::MIN_CAPACITY);
        let deadline = Instant::now() + Duration::from_secs(60);
        while r.j.sent_obs < s2.seq {
            assert!(Instant::now() < deadline, "60 s of wall clock");
            hand_over_only(&mut r);
        }
        assert!(
            r.j.sent_obs < r.j.replay_up_to,
            "S2's hand-over is not the replay's last"
        );
        assert!(
            industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "off, S2 waits for the switch"
        );
        r.modules = MODULE_INDUSTRY;
        r.j.modules(MODULE_INDUSTRY);
        hand_over_only(&mut r);
        assert!(
            r.j.sent_obs < r.j.replay_up_to,
            "the switch came inside the replay, more of it behind"
        );
        assert_eq!(
            r.j.e.stats.replays_ended.load(Ordering::Acquire),
            0,
            "still replaying"
        );
        assert!(
            !industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "the switch booked S2"
        );
        r.j.mc_mut().announce();
        let n = replayed_over_several(&mut r);
        eprintln!(
            "the switch on came inside the replay ({n} hand-overs), after S2 {}",
            s2.seq
        );
        the_copy_goes_once_after_the_switch(r, x);
    }

    /// An answer that reaches the engine while the industry module is off (RP-285). The server's
    /// config switches industry off in the tick Java applied a station exchange X and journaled
    /// its answer S, so MODULES goes ahead of S in that hand-over (BoundarySession.handOver writes
    /// the preamble, MODULES included, before it drains the journal, so every answer not drained
    /// yet when the switch comes lands behind it: this tick's, and any a full open buffer left
    /// over). Java's autosave then takes the pages past S and Java reads that ack: S was live in
    /// this session, so its rings prune X (they prune with the module off, RP-257), and Java trims
    /// its journal through it, and the journal's op index, which refuses an op Java applied,
    /// forgets X with it. The fake world's applied set never prunes, so the probe drops X from it
    /// there, as those do. On again, the engine sends its outbox under new sequences
    /// (`set_enabled`).
    ///
    /// Before RP-285 (at 37b2dcfb) the engine ignored S while off (U10), kept X in its outbox and
    /// sent it again after the ack past S, and the world, which no longer knew X, applied it a
    /// second time. Now S takes X out at once. The exchange's IND_CARRIAGE_RESULT, journaled just
    /// before S, is an answer too: the engine keeps it, with its tick, and books it at the switch
    /// (ignored before, it left the exchange waiting for a result that never came).
    #[test]
    fn an_answer_that_reached_the_engine_with_industry_off_is_never_sent_again_after_its_ack() {
        let mut r = Rig::new("ack-switch", false);
        r.run_to(day(3.0));
        r.every = 0;
        let (x, s, at) = switch_off_at(&mut r, APPLIED, None);
        let (ignored, outbox) = r.j.e.with_industry(|_, l| (l.ignored, l.outbox));
        eprintln!(
            "probe: X {x:#x} answered at S {s} (tick {at:?}); industry off: ignored {ignored}, \
             outbox {outbox}"
        );
        assert!(
            industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "the exchange's IND_CARRIAGE_RESULT heard while off waits for the switch"
        );
        let w = r.autosave();
        r.until_read(s);
        eprintln!(
            "probe: autosave through {}, Java read ack {:?}; X pruned: {}",
            w.end,
            r.j.acks.last(),
            r.c.gone.contains_key(&x)
        );
        assert!(
            r.j.mc_mut().forget_applied(OpId(x)),
            "the world applied X, and its journal's op index forgets it at the ack"
        );
        assert!(
            clock(&r) <= at,
            "the engine's clock stands at or before the kept answers"
        );
        switch_on(&mut r);
        assert!(
            !industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "the switch booked the kept IND_CARRIAGE_RESULT"
        );
        let t = r.j.mc().t;
        r.run_to(t + 2_000);
        eprintln!(
            "probe: the first ack at or past S: command buffer {:?}; X's answers: {:?}",
            r.c.first_ack_at_least(0, s),
            r.c.lives.iter().filter(|l| l.op == x).collect::<Vec<_>>()
        );
        for a in r.c.arrivals_of(x, 0) {
            eprintln!(
                "probe: X in command buffer {} as cmdSeq {}{} (acks sent before it up to {:?})",
                a.buffer,
                a.cmd_seq,
                if a.stale { ", stale" } else { "" },
                r.c.acks
                    .iter()
                    .filter(|k| k.1 < a.buffer)
                    .map(|k| k.2)
                    .max()
            );
        }
        let applied = |r: &Rig| {
            r.c.lives
                .iter()
                .filter(|l| l.op == x && l.status == APPLIED)
                .count()
        };
        assert_eq!(applied(&r), 1, "the world applied X once");
        r.every = 3_000;
        r.finish_with(|c, _| {
            assert_eq!(c.arrivals_of(x, 0).len(), 1, "X sent once");
        });
    }

    /// Records the engine ignores, for a crash test's replay to span several hand-overs, as a long
    /// replay in play does (RP-276's shape; slice R's `fillers` and `REPLAY_FILLERS` are the same
    /// thing, and these tests switch to them once both are merged): ship damage reports for agent
    /// 0, with the ships switched off. About 900 fill a minimum-size buffer.
    const PAD: usize = 2_000;

    fn pad_journal(j: &mut Java, n: usize) {
        use rtsim_ships::boundary::{self as sb, By, Obs as O};
        for _ in 0..n {
            j.ship_obs(&O::Damaged(sb::Damaged {
                agent: 0,
                epoch: 0,
                by: By::Other,
                hull_milli: 1000,
                crew_lost: 0,
                player: (0, 0),
                ship: 0,
            }));
        }
    }

    /// The hand-overs the restarted session's replay took: several (RP-276).
    fn replayed_over_several(r: &mut Rig) -> u64 {
        let t = r.j.mc().t;
        r.run_to(t + 100);
        let n = r.j.e.stats.replay_handovers.load(Ordering::Acquire);
        assert!(n >= 2, "the replay spans hand-overs: {n}");
        n
    }

    /// The industry command of op `op` this session's Java received, if any.
    fn sent(j: &Java, op: u64) -> Option<&Cmd> {
        j.ind_commands
            .iter()
            .find(|c| c.2.op() == Some(OpId(op)))
            .map(|c| &c.2)
    }

    /// The industry engine's clock.
    fn clock(r: &Rig) -> Tick {
        industry(&r.j.e, |ie| ie.now())
    }

    /// Steps until one of Java's answers of a tick, (op, seq, status), is one `pick` takes, and
    /// switches industry off in that tick, so MODULES goes ahead of the tick's journaled records in
    /// the hand-over (the answer, and the records Java journaled with it). Returns the op, the
    /// answer's seq and its game time. Also stops the world failing carriage deltas.
    fn switch_off_on(
        r: &mut Rig,
        what: &str,
        mut pick: impl FnMut(&Java, &(u64, u64, u32)) -> bool,
    ) -> (u64, u64, Tick) {
        let mut hit: Option<(u64, u64, Tick)> = None;
        let deadline = Instant::now() + Duration::from_secs(60);
        let limit = Tick(r.cfg.days * TICKS_PER_DAY);
        while hit.is_none() {
            r.step(
                |_, _, _| true,
                |j, answers| {
                    if let Some(&(op, seq, _)) = answers.iter().find(|a| pick(j, a)) {
                        j.modules(0);
                        j.mc_mut().fail_carriage_deltas = false;
                        hit = Some((op, seq, Tick(j.game_time as u64)));
                    }
                },
            );
            r.after_tick();
            assert!(Instant::now() < deadline, "60 s of wall clock");
            assert!(r.j.mc().t < limit, "{what}: not by the line's end");
        }
        hit.unwrap()
    }

    /// [`switch_off_on`] at a station exchange Java answers with `status` (the op `only`, if given).
    fn switch_off_at(r: &mut Rig, status: u32, only: Option<u64>) -> (u64, u64, Tick) {
        let what = format!("a station exchange answered {status}");
        switch_off_on(r, &what, |j, &(op, _, st)| {
            st == status
                && only.is_none_or(|o| o == op)
                && matches!(sent(j, op), Some(Cmd::CarriageDelta(d))
                    if d.reason == CarriageReason::Exchange)
        })
    }

    /// With industry off since the answer S of X: a page save past S lands and Java reads its ack
    /// (its rings forget X, and its journal, with the op index that refuses X, is trimmed past S),
    /// the world saves after a long run of records the engine ignores, and the engine crashes with
    /// the module still off. Restarts on a minimum-size buffer with the module off, checks the
    /// replay spans hand-overs, and returns the restart's settle and the engine's clock before the
    /// crash.
    fn crash_while_off_after_the_ack(r: &mut Rig, x: u64, s: u64) -> (Start, Tick) {
        let through = r.j.next_obs - 1;
        assert!(through >= s);
        r.j.e.request_save(through);
        r.until_read(through);
        let acked = r.c.max_ack;
        assert!(acked >= s, "the engine acknowledged S");
        pad_journal(&mut r.j, PAD);
        let w = r.c.save(&r.j);
        r.capacity = wire::MIN_CAPACITY;
        r.modules = 0;
        let frozen = clock(r);
        let start = r.restart(&w);
        assert!(r.j.pages_obs >= s, "the restart's pages are past S");
        let n = replayed_over_several(r);
        eprintln!(
            "op {x:#x}: crashed off after the ack of S {s}; the replay took {n} hand-overs; the \
             engine's clock {frozen:?} before the crash, {:?} after the replay, world {:?}",
            clock(r),
            r.j.mc().t
        );
        (start, frozen)
    }

    /// Industry on again: MODULES in this tick's hand-over (the engine books the answers it kept,
    /// each at its own tick, then runs its own session start), and Java announces its districts
    /// after the switch.
    fn switch_on(r: &mut Rig) {
        r.modules = MODULE_INDUSTRY;
        r.j.modules(MODULE_INDUSTRY);
        r.tick();
        r.j.mc_mut().announce();
    }

    /// After the switch: autosaves run, and the line runs to its end with X never sent in the
    /// session `session`, nor any command `same` takes for one of X's effect under another op (the
    /// last session's Java received them all after the switch: none goes out while off).
    fn to_the_end(mut r: Rig, x: u64, session: usize, same: impl Fn(&Cmd) -> bool) {
        r.every = 3_000;
        r.finish_with(|c, j| {
            assert!(
                c.arrivals_of(x, session).is_empty(),
                "X never sent after its answer heard while off: {:?}",
                c.arrivals_of(x, session)
            );
            let again: Vec<&Cmd> = j
                .ind_commands
                .iter()
                .map(|c| &c.2)
                .filter(|c| same(c))
                .collect();
            assert!(
                again.is_empty(),
                "no command of X's effect after the switch, under any op: {again:?}"
            );
        });
    }

    /// [`switch_on`], then [`to_the_end`].
    fn on_again_to_the_end(mut r: Rig, x: u64, session: usize, same: impl Fn(&Cmd) -> bool) {
        switch_on(&mut r);
        to_the_end(r, x, session, same);
    }

    /// RP-285, FAILED heard while off, across a crash while still off. The world fails every
    /// station exchange for a while, and industry goes off in the tick Java answers the first
    /// FAILED at S (MODULES ahead of S). Off, S takes X out of the outbox at once (before RP-285
    /// X waited there and went again under a new sequence), and the engine keeps S, with its
    /// tick, for the switch: the give-back is booked then, at S's tick, as a replay with the module
    /// on would book it, never at the clock the switch froze (a decision after it derives its op
    /// from its tick). Then [`crash_while_off_after_the_ack`]: the restart's pages hold the
    /// exchange still waiting and S kept, the clock where the switch froze it, and the replay does
    /// not bring S again. On, the switch books S once (one FAILED counted) and the engine never
    /// sends X. The world's books close at the end.
    #[test]
    fn a_failed_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off() {
        let mut r = Rig::new("ack-off-failed", false);
        r.run_to(day(3.0));
        r.every = 0;
        r.j.mc_mut().fail_carriage_deltas = true;
        let failed = r.j.e.with_industry(|_, l| l.results_failed);
        let (x, s, at) = switch_off_at(&mut r, FAILED, None);
        let pending = |r: &Rig| industry(&r.j.e, |ie| ie.exchange_pending(OpId(x)));
        let (frozen, failed_off) = r.j.e.with_industry(|ie, l| (ie.now(), l.results_failed));
        assert_eq!(
            failed_off, failed,
            "S waits for the switch: nothing booked while off"
        );
        assert!(pending(&r), "the exchange waits for S's booking");
        assert!(
            frozen <= at,
            "S's tick {at:?} is at or past the frozen clock {frozen:?}"
        );
        let (_, before) = crash_while_off_after_the_ack(&mut r, x, s);
        assert_eq!(before, frozen, "the engine stood still while off");
        assert_eq!(
            clock(&r),
            frozen,
            "the restart with the module off left the clock"
        );
        assert!(pending(&r), "the pages hold the exchange still waiting");
        assert_eq!(
            r.j.e.with_industry(|_, l| l.results_failed),
            0,
            "the replay did not bring S again"
        );
        switch_on(&mut r);
        assert!(!pending(&r), "the switch booked S: the exchange taken back");
        assert_eq!(
            r.j.e.with_industry(|_, l| l.results_failed),
            1,
            "S booked once"
        );
        to_the_end(r, x, 1, |_| false);
    }

    /// RP-285, DUPLICATE heard while off, across a crash while still off. First a J3 crash (the
    /// shape of RP-199's case): the pages stop before a station exchange
    /// X, the journal is saved with X's arrival and a long run of records the engine ignores, and
    /// the trains, their ring and the world's applied set after Java applied X. The replay
    /// (several hand-overs) derives X again under the same op and the world refuses it DUPLICATE
    /// live at S, in the tick industry goes off (MODULES ahead of S). Off, the engine hears S: X
    /// leaves the outbox at once, and S is kept for the switch, which books it at S's tick as
    /// `command_result` does with the module on. Then
    /// [`crash_while_off_after_the_ack`] (S kept in the link page) and on again: X is never sent in
    /// the third session. The switch books S as `command_result` does with the module on, so the
    /// exchange derived again is dropped there (RP-199), never booked a second time.
    #[test]
    fn a_duplicate_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off() {
        let mut r = Rig::new("ack-off-dup", false);
        r.run_to(day(2.0));
        r.every = 0;
        let t0 = r.j.next_obs - 1;
        r.j.e.request_save(t0);
        r.run("the pages", day(3.0), |r| r.j.e.durable_obs_seq() >= t0);
        let t0 = r.j.e.durable_obs_seq();
        let exchanges = |r: &Rig| -> Vec<u64> {
            r.c.arrivals
                .iter()
                .filter(|a| a.session == 0 && a.kind == "exchange")
                .map(|a| a.op)
                .collect()
        };
        let before = exchanges(&r).len();
        r.run("a station exchange", day(8.0), |r| {
            exchanges(r).len() > before
        });
        let x = exchanges(&r)[before];
        assert!(!r.j.mc().applied_ops().contains(&OpId(x)));
        pad_journal(&mut r.j, PAD);
        let mut w = r.c.save(&r.j);
        r.run("the world applies X", day(8.0), |r| {
            r.j.mc().applied_ops().contains(&OpId(x)) && r.c.lives_in(0).any(|l| l.op == x)
        });
        assert!(!industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))));
        let later = r.c.save(&r.j);
        w.msave = later.msave;
        w.ring = later.ring;
        w.gone = later.gone;
        assert_eq!(r.j.e.durable_obs_seq(), t0, "no later page save");
        r.capacity = wire::MIN_CAPACITY;
        r.restart(&w);
        let (x2, s, at) = switch_off_at(&mut r, DUPLICATE, Some(x));
        assert_eq!(x2, x);
        replayed_over_several(&mut r);
        let pending = |r: &Rig| industry(&r.j.e, |ie| ie.exchange_pending(OpId(x)));
        assert!(
            pending(&r),
            "the DUPLICATE heard while off waits for the switch"
        );
        assert!(clock(&r) <= at);
        assert_eq!(
            r.c.arrivals_of(x, 1).len(),
            1,
            "derived again and sent once"
        );
        crash_while_off_after_the_ack(&mut r, x, s);
        assert!(
            pending(&r),
            "the pages hold the exchange derived again, S kept"
        );
        switch_on(&mut r);
        assert!(
            !pending(&r),
            "the switch books S: the exchange derived again is dropped (RP-199)"
        );
        to_the_end(r, x, 2, |_| false);
    }

    /// RP-285 with a chunk's carried answer (`Industry.commandResultInChunk`, the chunk's
    /// `mic_rtsim:pending` record). A test's order writes a package P into a postbox's offline
    /// buffer; the pages stop with P queued; the world saves in the step that writes P (after a
    /// long run of records the engine ignores), before Java applies it. Java applies P into the
    /// block entity and answers it (S1), and its chunk carries S1 until the engine acknowledges
    /// it. The engine crashes before any page save covers S1, the chunk having reached disk (J3
    /// for the chunk: the journal lost S1). The restart, industry off, has P queued in its pages;
    /// the chunk loads and Java journals its carried record again (`RtsimSession.reassert`: a
    /// REASSERT of S1 under the first session's generation): a first answer for P, heard while
    /// off, which takes P out of the queue. Java reads an ack past it (its chunk then drops the
    /// carry and the journal's op index forgets P, so nothing refuses P any more), and once on
    /// the engine never sends P. Before RP-285 the copy was ignored and P went out at the switch.
    #[test]
    fn a_carried_answer_heard_with_industry_off_takes_its_queued_command() {
        carried_answer("ack-off-carried", false);
    }

    /// RP-285 with a postbox's carried answer (`Industry.carryAppliedInChunk`, RP-206), as
    /// [`a_carried_answer_heard_with_industry_off_takes_its_queued_command`] but with the second
    /// record a postbox carries: Java applies P and answers it live (S1), then the postbox takes
    /// its buffer back and its chunk carries P's APPLIED again, journaled with no command sequence
    /// (S2, as `resultInChunk` writes it). The crash loses both (J3 for the chunk). The restart,
    /// industry off, hears S2 again (REASSERT under the first session's generation) as a first
    /// answer for P, which takes P out of the queue; Java reads an ack past it (its chunk then
    /// drops the carry and its rings forget P, so nothing refuses P any more), and once on the
    /// engine never sends P.
    #[test]
    fn a_postbox_carried_answer_heard_with_industry_off_takes_its_queued_command() {
        carried_answer("ack-off-postbox", true);
    }

    /// The carried cases. `postbox`: the chunk carries the postbox's own record of P (S2, no
    /// command sequence), journaled after the live answer S1; otherwise it carries S1 itself.
    fn carried_answer(tag: &str, postbox: bool) {
        use rtsim_industry::cmd::{IndustryOps, WritePackage};
        let mut r = Rig::new(tag, false);
        r.run_to(day(3.0));
        r.every = 0;
        let name = "mic:ack_test_postbox".to_string();
        let address = r.j.dict_id(DICT_NAME, &name);
        let district = r.j.mc().districts[ORE_COAST].id;
        let mut p = Cmd::WritePackage(WritePackage {
            op: OpId(0),
            district,
            inventory: 0,
            pos: BlockPos::default(),
            address,
            count: 1,
            goods: GoodVec::new(),
        });
        // Derived at this tick, as the engine's (only a train raid's loot is keyed, RP-180).
        p.set_op(IndustryOps::default().derive(r.cfg.seed, r.j.mc().t, &p));
        let mut record = Vec::new();
        iw::encode_cmd(0, &p, &mut record).expect("a package command encodes");
        r.j.e
            .order(vec![Order::IndustryCommand {
                names: vec![(DICT_NAME, address, name)],
                record,
            }])
            .unwrap();
        let through = r.j.next_obs - 1;
        r.j.e.request_save(through);
        let mut w = None;
        r.step(
            |j, c, _| {
                pad_journal(j, PAD);
                w = Some(c.save(j));
                true
            },
            |_, _| {},
        );
        r.after_tick();
        let w = w.expect("the world saved");
        let pk =
            r.c.arrivals
                .iter()
                .find(|a| a.kind == "package")
                .expect("the package written in that step")
                .op;
        let mut carried: Option<(u64, Vec<u8>)> = None;
        let mut s1 = None;
        let deadline = Instant::now() + Duration::from_secs(60);
        while carried.is_none() {
            r.step(
                |_, _, _| true,
                |j, answers| {
                    if let Some(a) = answers.iter().find(|a| a.0 == pk) {
                        assert_eq!(a.2, APPLIED);
                        s1 = Some(a.1);
                        let at = if postbox {
                            // The postbox takes its buffer back: its chunk carries P's APPLIED
                            // again, with no command sequence (`carryAppliedInChunk`).
                            j.journal(
                                obs::COMMAND_RESULT,
                                obs::command_result::SIZE,
                                APPLIED,
                                |rec| {
                                    wire::put_u64(rec, obs::command_result::CMD_SEQ, 0);
                                    wire::put_u64(rec, obs::command_result::OP_ID, pk);
                                },
                            )
                        } else {
                            // `commandResultInChunk` carries the live answer itself.
                            a.1
                        };
                        carried = Some((at, j.journaled(at).expect("journaled")));
                    }
                },
            );
            r.after_tick();
            assert!(Instant::now() < deadline);
        }
        let (carry, bytes) = carried.unwrap();
        let s1 = s1.unwrap();
        if postbox {
            r.c.answered(pk, carry, APPLIED);
        }
        r.run("the pages", day(4.0), |r| {
            r.j.e.durable_obs_seq() >= through
        });
        assert_eq!(r.j.e.durable_obs_seq(), through, "no later page save");
        assert!(r.c.max_ack < s1, "no ack at or past S1");
        r.capacity = wire::MIN_CAPACITY;
        r.modules = 0;
        r.restart(&w);
        assert!(r.j.pages_obs < s1, "the pages are behind S1");
        replayed_over_several(&mut r);
        let queued = r.j.e.with_industry(|_, l| l.queued);
        assert!(queued >= 1, "P waits in the queue, the module off");
        let nonce = r.c.sessions[0].nonce;
        let applied = r.j.e.stats.copies_applied.load(Ordering::Acquire);
        let s = r.j.reassert(nonce, &bytes);
        r.c.answered(pk, s, APPLIED);
        r.tick();
        assert_eq!(
            r.j.e.stats.copies_applied.load(Ordering::Acquire),
            applied + 1,
            "the copy is new (past the first session's last record heard), so it reaches `journaled`"
        );
        let queued_after = r.j.e.with_industry(|_, l| l.queued);
        assert_eq!(
            queued_after + 1,
            queued,
            "the carried answer took P out of the queue"
        );
        r.autosave();
        r.until_read(s);
        eprintln!(
            "{tag}: P {pk:#x} live S1 {s1}, carried {carry} in its chunk, lost with the journal; \
             REASSERT at {s}, heard while off and acknowledged"
        );
        on_again_to_the_end(r, pk, 1, |_| false);
    }

    /// RP-285: a restart with industry off leaves the engine's clock where the switch froze it, so
    /// an answer kept for the switch is booked at its own tick. Industry goes off at day 3 and stays
    /// off for 500 ticks; a page save lands, Java reads its ack, and the engine crashes with the
    /// module still off and restarts off. At 2a751255 `simulate` ran `begin_session` ahead of its
    /// switch gate, so the restart ran the engine's own session start at the restart's game time
    /// with the module off: every breakpoint and wake of the time it had been off fired, and their
    /// decisions waited in the engine's out for the switch (this case's clock went from 72,000 to
    /// 72,502). Now the session start waits for the switch, which runs its own.
    #[test]
    fn a_restart_with_industry_off_leaves_the_engine_clock_where_the_switch_froze_it() {
        let mut r = Rig::new("ack-off-clock", false);
        r.run_to(day(3.0));
        r.every = 0;
        r.j.modules(0);
        r.tick();
        let frozen = clock(&r);
        let t = r.j.mc().t;
        r.run_to(t + 500);
        assert_eq!(clock(&r), frozen, "off, the engine stands still");
        let s = r.j.next_obs - 1;
        let (_, before) = crash_while_off_after_the_ack(&mut r, 0, s);
        assert_eq!(before, frozen);
        assert_eq!(
            clock(&r),
            frozen,
            "a restart with the module off does not start the engine's session"
        );
        switch_on(&mut r);
        let now = clock(&r);
        assert!(
            now > frozen + 500,
            "the switch ran the session start, to the world's present: {now:?}"
        );
        r.every = 3_000;
        r.finish();
    }

    /// RP-285 (P2-1), IND_RECONCILE_RESULT heard while off, across a crash while still off. Industry
    /// goes off in the tick Java applies a chunk reconcile X and journals its IND_RECONCILE_RESULT
    /// and APPLIED (MODULES ahead of both). Off, the engine keeps the result with its tick: X stays
    /// pending until the switch books it there. Then [`crash_while_off_after_the_ack`] (the pages
    /// hold X pending, the result kept), and on again: the switch books the result before Java's
    /// announce, and no reconcile of X's chunk and epoch goes out again, under X's op or another.
    /// At 2a751255 the result was ignored off, X stayed pending past the switch, and its booking
    /// waited for a ChunkLoaded.
    #[test]
    fn a_reconcile_result_heard_with_industry_off_is_booked_once_across_a_crash_while_off() {
        let mut r = Rig::new("ack-off-reconcile", false);
        r.run_to(day(1.0));
        r.every = 0;
        let (x, s, at) = switch_off_on(&mut r, "a chunk reconcile answered", |j, &(op, _, st)| {
            st == APPLIED && matches!(sent(j, op), Some(Cmd::ChunkReconcile(_)))
        });
        let Some(Cmd::ChunkReconcile(rc)) = sent(&r.j, x).cloned() else {
            unreachable!("picked a reconcile")
        };
        let (d, chunk, epoch) = (rc.district, rc.chunk, rc.epoch);
        let pending = |r: &Rig| {
            industry(&r.j.e, |ie| {
                ie.district(d)
                    .is_some_and(|x2| x2.pending_ops.contains_key(&OpId(x)))
            })
        };
        assert!(
            pending(&r),
            "the result heard while off waits for the switch"
        );
        assert!(clock(&r) <= at);
        crash_while_off_after_the_ack(&mut r, x, s);
        assert!(pending(&r), "the pages hold X pending, its result kept");
        switch_on(&mut r);
        assert!(!pending(&r), "the switch booked the kept result");
        to_the_end(r, x, 1, move |c| {
            matches!(c, Cmd::ChunkReconcile(c)
                if c.district == d && c.chunk == chunk && c.epoch == epoch)
        });
    }

    /// A station exchange that unloads a carriage, delivered to the world in the step just run and
    /// not applied yet, whose carriage holds at least two of that entry's good: (op, train,
    /// carriage, good, what the world would move).
    fn delivered_unload(r: &mut Rig) -> (u64, TrainId, u32, Good, i64) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let seen = r.j.ind_commands.len();
            r.tick();
            let mc = r.j.mc();
            let found = r.j.ind_commands[seen..].iter().find_map(|(_, _, c)| {
                let Cmd::CarriageDelta(cd) = c else {
                    return None;
                };
                if cd.reason != CarriageReason::Exchange {
                    return None;
                }
                let train = mc.trains.iter().find(|t| t.id == cd.train)?;
                cd.entries.iter().find_map(|&(ci, g, delta)| {
                    let cargo = train.carriages.iter().find(|c| c.index == ci)?.cargo.get(g);
                    let m = cargo.min(-delta);
                    (delta < 0 && m >= 2).then_some((cd.op.0, cd.train, ci, g, m))
                })
            });
            if let Some(f) = found {
                return f;
            }
            assert!(Instant::now() < deadline, "60 s of wall clock");
            assert!(r.j.mc().t < day(9.0), "an unloading exchange by day 9");
        }
    }

    /// RP-285 (P2-2), a short IND_CARRIAGE_RESULT heard while off, across a crash while still off.
    /// A station exchange X unloads a carriage; before the world applies it, half of what it would
    /// move leaves the carriage (burned in the world's books), so Java moves less than asked.
    /// Industry goes off in the tick Java applies X (MODULES ahead of its IND_CARRIAGE_RESULT and
    /// APPLIED). Off, the engine keeps the result; the switch, after a crash while off, books it at
    /// its tick, so the district's stock is corrected by the shortfall once. At 2a751255 the result
    /// was ignored off and the exchange stayed pending past the switch, waiting for a result Java
    /// had already sent, so the district kept goods Java never moved: the world's books do not
    /// close. (On slice T, whose APPLIED heard off dropped the exchange, it was dropped unbooked.)
    #[test]
    fn a_short_carriage_result_heard_with_industry_off_is_corrected_once_across_a_crash_while_off()
    {
        let mut r = Rig::new("ack-off-short", false);
        r.run_to(day(3.0));
        r.every = 0;
        let (x, train, ci, g, m) = delivered_unload(&mut r);
        let k = {
            let mc = r.j.mc_mut();
            let car = mc
                .trains
                .iter_mut()
                .find(|t| t.id == train)
                .and_then(|t| t.carriages.iter_mut().find(|c| c.index == ci))
                .expect("the carriage");
            let k = car.cargo.get(g) - m / 2;
            car.cargo.add(g, -k);
            mc.books.burned.add(g, k);
            k
        };
        let (_, s, at) = switch_off_on(&mut r, "X's answer", |_, &(op, _, st)| {
            op == x && st == APPLIED
        });
        eprintln!(
            "short: X {x:#x} moves {} of {m} (took {k} out first), answered at {at:?}",
            m / 2
        );
        crash_while_off_after_the_ack(&mut r, x, s);
        switch_on(&mut r);
        assert!(
            !industry(&r.j.e, |ie| ie.exchange_pending(OpId(x))),
            "the switch booked the result"
        );
        to_the_end(r, x, 1, |_| false);
    }

    /// RP-285 (P1-1), a chain write's `CHAIN_IN` heard while off, across a crash while still off. A
    /// test's order (W17) writes two packages of ingots into a frogport of a package store Java
    /// declared (StoreId 7, one frogport in no district). Java applies the write and journals the
    /// `CHAIN_IN` carrying the write's op, then its APPLIED, and industry goes off in that tick.
    /// Off, the engine keeps the `CHAIN_IN`; after a crash while off, the frogport's chunk
    /// journals it again (REASSERT), and the switch takes it once: the store counts two packages
    /// written, whatever the copy. At 2a751255 the `CHAIN_IN` was ignored off, so nothing lowered
    /// what the store owed or its pools, and an engine write past ten periods went again under a
    /// new op (SC113): the engine-level case in `chain_replay.rs` shows that write; this one shows
    /// the record booked.
    #[test]
    fn a_chain_write_answered_with_industry_off_is_taken_once_across_a_crash_while_off() {
        use rtsim_industry::cmd::{IndustryOps, WriteChain};
        use rtsim_industry::ids::StoreId;
        use rtsim_industry::obs::{ChainDirection, ChainPackage, StoreDecl, StoreFrogport};
        let mut r = Rig::new("ack-off-chain", false);
        r.run_to(day(3.0));
        r.every = 0;
        let store = StoreId(7);
        let pos = BlockPos::new(9_000, 64, 0);
        let (dim, district, at) = {
            let mc = r.j.mc();
            (
                mc.districts[ORE_COAST].dim,
                mc.districts[ORE_COAST].id,
                mc.t,
            )
        };
        r.j.industry_obs(&Timed {
            at,
            obs: Obs::PackageStore(StoreDecl {
                store,
                dim,
                epoch: 0,
                into: None,
                frogports: vec![StoreFrogport {
                    district: None,
                    inventory: None,
                    star: false,
                    pos,
                    conveyor: BlockPos::new(9_001, 64, 0),
                    from: None,
                }],
                hops: vec![],
            }),
        });
        let mut w = Cmd::WriteChain(WriteChain {
            op: OpId(0),
            store,
            district,
            inventory: 0,
            frogport: 0,
            pos,
            count: 2,
            goods: GoodVec::from_pairs([(INGOT, 64)]),
        });
        // Derived at this tick, as the engine's (only a train raid's loot is keyed, RP-180).
        let op = IndustryOps::default().derive(r.cfg.seed, r.j.mc().t, &w);
        w.set_op(op);
        let mut record = Vec::new();
        iw::encode_cmd(0, &w, &mut record).expect("a chain write encodes");
        r.j.e
            .order(vec![Order::IndustryCommand {
                names: vec![],
                record,
            }])
            .unwrap();
        let x = op.0;
        r.run("the write delivered", day(4.0), |r| sent(&r.j, x).is_some());
        // Java applies it at the next tick, journaling the `CHAIN_IN` ahead of its APPLIED.
        let t = r.j.mc().t + 1;
        r.j.mc_mut().outgoing.push(Outgoing::Obs(Timed {
            at: t,
            obs: Obs::ChainPackage(ChainPackage {
                store,
                district: None,
                inventory: None,
                direction: ChainDirection::In,
                frogport: 0,
                pos,
                destination: Some(0),
                packages: 2,
                address: 0,
                op: Some(op),
                key: None,
                goods: GoodVec::from_pairs([(INGOT, 128)]),
            }),
        }));
        let (_, s, at) = switch_off_on(&mut r, "the chain write's answer", |_, &(o, _, st)| {
            o == x && st == APPLIED
        });
        assert_eq!(at, t);
        let bytes = (1..s)
            .rev()
            .filter_map(|q| r.j.journaled(q))
            .find(|b| wire::u16_at(b, wire::R_KIND) == iw::IND_CHAIN_PACKAGE)
            .expect("the CHAIN_IN, journaled ahead of the APPLIED");
        let written = |r: &Rig| {
            industry(&r.j.e, |ie| {
                ie.stores().get(&store).map_or(0, |st| st.counts.written)
            })
        };
        assert_eq!(
            written(&r),
            0,
            "the CHAIN_IN heard while off waits for the switch"
        );
        assert!(clock(&r) <= at);
        crash_while_off_after_the_ack(&mut r, x, s);
        assert!(
            industry(&r.j.e, |ie| ie.stores().contains_key(&store)),
            "the restart reads the store's page back"
        );
        assert_eq!(written(&r), 0, "the pages hold it kept, not taken");
        let nonce = r.c.sessions[0].nonce;
        r.j.reassert(nonce, &bytes);
        r.tick();
        switch_on(&mut r);
        assert_eq!(written(&r), 2, "the switch took the CHAIN_IN once");
        to_the_end(
            r,
            x,
            1,
            move |c| matches!(c, Cmd::WriteChain(w) if w.pos == pos),
        );
    }

    /// A restart from a copy of the crash snapshot (`pages`, `saved`), on a fresh copy of the iron
    /// line's world restored from it, with `modules`, at the world's present.
    fn from_snapshot(
        pages: &std::path::Path,
        saved: &Saved,
        cfg: IronLine,
        modules: u32,
        tag: &str,
    ) -> Rig {
        let dir = copy_pages(pages, tag);
        let mut mc = iron_line_world(cfg, Tunables::default()).into_remote();
        mc.restore(&saved.msave);
        let e = engine_with(&dir, 256 * 1024, |c| c.industry_seed = Some(cfg.seed));
        let mut j = Java::open(e, saved.jsave.clone());
        j.game_time = mc.t.0 as i64;
        j.mc = Some(mc);
        j.module_mask = modules;
        let c = AckCheck::new(&j);
        Rig {
            dir,
            cfg,
            j,
            c,
            every: 0,
            last: None,
            repaired: false,
            capacity: 256 * 1024,
            modules,
        }
    }

    /// One hand-over and Java's reading of its commands, with no world tick and no game time
    /// passing: [`Rig::step`]'s part after the world's.
    fn hand_over_only(r: &mut Rig) {
        let (j, c) = (&mut r.j, &mut r.c);
        let gt = j.game_time;
        assert!(j.tick(), "a lockstep tick always submits");
        j.game_time = gt;
        let deadline = Instant::now() + Duration::from_secs(20);
        while j.e.stats_array()[1] < j.submitted {
            assert!(
                Instant::now() < deadline,
                "step {} never finished",
                j.submitted
            );
            std::thread::yield_now();
        }
        c.arrived(j);
        let read = j.acks.len();
        j.apply_commands();
        if j.acks.len() > read {
            c.read_ack(*j.acks.last().unwrap());
        }
    }

    /// What two restarts are compared by: the industry engine's pages, its clock, and every op id
    /// of the commands this session's Java received.
    type Outcome = (Vec<(u64, Vec<u8>)>, Tick, std::collections::BTreeSet<u64>);

    fn outcome(r: &Rig) -> Outcome {
        let pages = industry(&r.j.e, |ie| {
            let mut v: Vec<(u64, Vec<u8>)> =
                ie.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
            v.sort();
            v
        });
        let ops =
            r.j.ind_commands
                .iter()
                .filter_map(|c| c.2.op())
                .map(|o| o.0)
                .collect();
        (pages, clock(r), ops)
    }

    /// The pages of two outcomes that differ, by key.
    fn differing(a: &Outcome, b: &Outcome) -> Vec<u64> {
        let ma: BTreeMap<u64, &Vec<u8>> = a.0.iter().map(|(k, v)| (*k, v)).collect();
        let mb: BTreeMap<u64, &Vec<u8>> = b.0.iter().map(|(k, v)| (*k, v)).collect();
        let mut keys: Vec<u64> = ma.keys().chain(mb.keys()).copied().collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .filter(|k| ma.get(k) != mb.get(k))
            .collect()
    }

    /// RP-285: a restart with industry off, switched on, books the answers its replay brings as a
    /// restart with industry on books them. One crash snapshot: a station exchange X is cut short
    /// (half of what it would unload leaves the carriage first), and in a tick whose journaled
    /// records are all Java's answers (X's IND_CARRIAGE_RESULT and APPLIED among them), industry
    /// goes off and a page save lands right before them; the engine crashes after that tick, the
    /// journal holding the window. From copies of the snapshot: B restarts with the module on (the
    /// replay books the answers at their ticks); A restarts with it off (the replay's answers are
    /// kept) and switches on in its second hand-over, at B's session start's game time (no world
    /// tick between the two). Both run 24,000 ticks; the engines' pages and clocks, and the op ids
    /// of every command Java received, must be equal. Two B restarts from the snapshot are the
    /// control and come first. This tests the replay-while-off path; the live path (answers heard
    /// off in the session that switched) is the failing-first cases above.
    #[test]
    fn a_restart_with_industry_off_books_its_replayed_answers_as_a_restart_with_it_on() {
        let mut r = Rig::new("ack-off-same", false);
        r.run_to(day(3.0));
        r.every = 0;
        let deadline = Instant::now() + Duration::from_secs(60);
        let (x, first) = loop {
            assert!(Instant::now() < deadline, "60 s of wall clock");
            let (x, train, ci, g, m) = delivered_unload(&mut r);
            {
                let mc = r.j.mc_mut();
                let car = mc
                    .trains
                    .iter_mut()
                    .find(|t| t.id == train)
                    .and_then(|t| t.carriages.iter_mut().find(|c| c.index == ci))
                    .expect("the carriage");
                let k = car.cargo.get(g) - m / 2;
                car.cargo.add(g, -k);
                mc.books.burned.add(g, k);
            }
            let first = r.j.next_obs;
            let mut hit = false;
            r.step(
                |_, _, _| true,
                |j, answers| {
                    let answers_only = (first..j.next_obs).all(|q| {
                        j.journaled(q).is_some_and(|b| {
                            [obs::COMMAND_RESULT, iw::IND_CARRIAGE_RESULT]
                                .contains(&wire::u16_at(&b, wire::R_KIND))
                        })
                    });
                    if answers_only && answers.iter().any(|a| a.0 == x && a.2 == APPLIED) {
                        j.modules(0);
                        j.e.request_save(first - 1);
                        hit = true;
                    }
                },
            );
            r.after_tick();
            if hit {
                break (x, first);
            }
        };
        let window = r.j.next_obs - first;
        let until = Instant::now() + Duration::from_secs(20);
        while r.j.e.durable_obs_seq() < first - 1 {
            assert!(
                Instant::now() < until,
                "the page save before the window never landed"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            r.j.e.durable_obs_seq(),
            first - 1,
            "the pages stop before the window"
        );
        let saved = r.c.save(&r.j);
        r.j.e.close(false, Duration::from_secs(5)).unwrap();
        eprintln!(
            "same: X {x:#x} cut short; the window is seqs {first} to {} ({window} answers), world {:?}",
            saved.end,
            r.j.mc().t
        );
        let run = |modules: u32, tag: &str| {
            let mut a = from_snapshot(&r.dir, &saved, r.cfg, modules, tag);
            hand_over_only(&mut a);
            if modules == 0 {
                assert!(
                    industry(&a.j.e, |ie| ie.exchange_pending(OpId(x))),
                    "off, the replayed IND_CARRIAGE_RESULT is kept"
                );
                a.modules = MODULE_INDUSTRY;
                a.j.modules(MODULE_INDUSTRY);
            }
            hand_over_only(&mut a);
            assert!(
                !industry(&a.j.e, |ie| ie.exchange_pending(OpId(x))),
                "X is booked"
            );
            let start = outcome(&a);
            let t = a.j.mc().t;
            a.run_to(t + 24_000);
            let end = outcome(&a);
            a.j.e.close(true, Duration::from_secs(5)).unwrap();
            (start, end)
        };
        let b1 = run(MODULE_INDUSTRY, "ack-off-same-b1");
        let b2 = run(MODULE_INDUSTRY, "ack-off-same-b2");
        assert!(
            b1.0 == b2.0,
            "control, at the session start: {:?}",
            differing(&b1.0, &b2.0)
        );
        assert!(
            b1.1 == b2.1,
            "control, at the end: {:?}",
            differing(&b1.1, &b2.1)
        );
        let a = run(0, "ack-off-same-a");
        assert!(
            a.0 == b1.0,
            "at the session start: {:?}",
            differing(&a.0, &b1.0)
        );
        assert_eq!(a.1.1, b1.1.1, "the clocks at the end");
        assert_eq!(
            a.1.2.symmetric_difference(&b1.1.2).collect::<Vec<_>>(),
            Vec::<&u64>::new(),
            "the op ids Java received"
        );
        assert!(a.1 == b1.1, "at the end: {:?}", differing(&a.1, &b1.1));
        eprintln!(
            "same: {} pages, {} ops, the clock {:?}",
            a.1.0.len(),
            a.1.2.len(),
            a.1.1
        );
    }

    /// Where the crash snapshot's pages stop in [`a_failed_exchange_restarted_with_industry_off`].
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Pages {
        /// After X's decision, right before the tick of Java's FAILED answer.
        AfterTheDecision,
        /// Before X's decision: the switch's catch-up decides X again under the op Java failed.
        BeforeTheDecision,
        /// As [`Pages::BeforeTheDecision`], and the restart with the module off autosaves and
        /// crashes again, still off, before the switch.
        BeforeAndACrashWhileOff,
    }

    /// What a restart from the snapshot ends with: X's commands Java received in the session, the
    /// FAILED answers the link booked, whether X's exchange still waits, the world against the
    /// engine's books, and the trains' cargo.
    type Ending = (usize, u64, bool, Result<(), String>, Vec<Vec<GoodVec>>);

    /// The SEC-1 off path on slice R (RP-259, RP-285): a station exchange X the world answers
    /// FAILED, and a crash with the pages at `pages`; the world saved after the answer, with
    /// industry switched off in the answer's tick (MODULES ahead of it). From copies of the
    /// snapshot, B restarts with the module on: the replay hears the FAILED answer (`Link::heard`),
    /// and with the pages before X's decision decides X again from the replayed IND_TRAIN_ARRIVED
    /// under the same op and leaves the copy unsent. A restarts with it off (the answer is kept,
    /// RP-285) and switches on in its second hand-over, where the kept answer is booked at its tick
    /// before the catch-up. A station exchange is decided only from Java's IND_TRAIN_ARRIVED, which
    /// the replay with the module off ignores (U10), so with the pages before X's decision A never
    /// decides X again: the catch-up cannot write a copy the booked answer failed to meet. Both
    /// run a day and until every command is applied. In each, X is never sent, its FAILED is booked
    /// once, the exchange is not left waiting, and the world against the engine's books (and the
    /// trains' cargo) is B's.
    fn a_failed_exchange_restarted_with_industry_off(pages: Pages, tag: &str) {
        let before = pages != Pages::AfterTheDecision;
        let mut r = Rig::new(tag, false);
        r.run_to(day(3.0));
        r.every = 0;
        r.j.mc_mut().fail_carriage_deltas = true;
        // X: the first station exchange decided from now (the world fails it). With the pages
        // before X's decision, a page save is asked for at every step's end until X is decided,
        // so the last is taken right before the step that decides it.
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut asked = 0;
        let (x, decided_from) = loop {
            assert!(Instant::now() < deadline, "60 s of wall clock");
            assert!(r.j.mc().t < day(8.0), "a station exchange by day 8");
            if before {
                asked = r.j.next_obs - 1;
                r.j.e.request_save(asked);
            }
            let seen = r.j.ind_commands.len();
            let from = r.j.next_obs;
            r.tick();
            let found = r.j.ind_commands[seen..].iter().find_map(|c| match &c.2 {
                Cmd::CarriageDelta(d) if d.reason == CarriageReason::Exchange => Some(d.op.0),
                _ => None,
            });
            if let Some(op) = found {
                break (op, from);
            }
        };
        // The tick of X's FAILED answer: industry goes off ahead of it, and with the pages after
        // X's decision, they are asked for right before the tick's records.
        let mut hit: Option<u64> = None;
        while hit.is_none() {
            assert!(Instant::now() < deadline, "60 s of wall clock");
            assert!(r.j.mc().t < day(8.0), "X answered by day 8");
            let first = r.j.next_obs;
            r.step(
                |_, _, _| true,
                |j, answers| {
                    if answers.iter().any(|a| a.0 == x && a.2 == FAILED) {
                        j.modules(0);
                        j.mc_mut().fail_carriage_deltas = false;
                        if !before {
                            j.e.request_save(first - 1);
                        }
                        hit = Some(first);
                    }
                },
            );
            r.after_tick();
        }
        let first = hit.unwrap();
        let through = if before { asked } else { first - 1 };
        let until = Instant::now() + Duration::from_secs(20);
        while r.j.e.durable_obs_seq() < through {
            assert!(Instant::now() < until, "the pages asked for never landed");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(r.j.e.durable_obs_seq(), through, "the pages stop where asked");
        if before {
            assert!(through < decided_from, "the pages stop before X's decision");
        }
        let window: Vec<u16> = (through + 1..r.j.next_obs)
            .filter_map(|q| r.j.journaled(q).map(|b| wire::u16_at(&b, wire::R_KIND)))
            .collect();
        let saved = r.c.save(&r.j);
        r.j.e.close(false, Duration::from_secs(5)).unwrap();
        eprintln!(
            "{pages:?}: X {x:#x} decided in the step from seq {decided_from}, answered FAILED in \
             the tick from seq {first}; pages through {through}; the window's kinds {window:?}"
        );
        let cfg = r.cfg;
        let run = |modules: u32, tag: &str| -> Ending {
            let mut a = from_snapshot(&r.dir, &saved, cfg, modules, tag);
            hand_over_only(&mut a);
            if modules == 0 && pages == Pages::BeforeAndACrashWhileOff {
                // Still off: an autosave whose pages land, then a crash and a restart off again.
                let w = a.c.save(&a.j);
                a.j.e.request_save(w.end);
                let until = Instant::now() + Duration::from_secs(20);
                while a.j.e.durable_obs_seq() < w.end {
                    assert!(Instant::now() < until, "the autosave while off never landed");
                    hand_over_only(&mut a);
                }
                a.j.e.close(false, Duration::from_secs(5)).unwrap();
                let again = from_snapshot(&a.dir, &w, cfg, 0, &format!("{tag}-again"));
                a = again;
                hand_over_only(&mut a);
            }
            if modules == 0 {
                assert!(
                    !industry(&a.j.e, |ie| ie.exchange_pending(OpId(x))) || !before,
                    "off, the engine has not decided X again"
                );
                a.modules = MODULE_INDUSTRY;
                a.j.modules(MODULE_INDUSTRY);
            }
            hand_over_only(&mut a);
            let t = a.j.mc().t;
            a.run_to(t + 24_000);
            a.run("every command applied", t + 30_000, |r| {
                r.j.mc().commands_in_flight() == 0
            });
            let sent = a
                .j
                .ind_commands
                .iter()
                .filter(|c| c.2.op() == Some(OpId(x)))
                .count();
            let failed = a.j.e.with_industry(|_, l| l.results_failed);
            let pending = industry(&a.j.e, |ie| ie.exchange_pending(OpId(x)));
            let ending = (sent, failed, pending, books_delta(&a.j), cargo(&a.j));
            a.j.e.close(true, Duration::from_secs(5)).unwrap();
            ending
        };
        let b = run(MODULE_INDUSTRY, &format!("{tag}-b"));
        let a = run(0, &format!("{tag}-a"));
        eprintln!(
            "{pages:?}: on, X sent {} times, {} FAILED booked, pending {}, books {:?}; off then on, \
             X sent {} times, {} FAILED booked, pending {}, books {:?}; the same cargo: {}",
            b.0,
            b.1,
            b.2,
            b.3,
            a.0,
            a.1,
            a.2,
            a.3,
            a.4 == b.4
        );
        for (arm, e) in [("on", &b), ("off then on", &a)] {
            assert_eq!(e.0, 0, "{arm}: X never sent after the restart");
            assert_eq!(e.1, 1, "{arm}: X's FAILED booked once");
            assert!(!e.2, "{arm}: X's exchange is not left waiting");
        }
        assert_eq!(a.3, b.3, "the world against the engine's books, off then on and on");
        assert!(a.4 == b.4, "the trains' cargo, off then on and on");
    }

    #[test]
    fn a_failed_exchange_with_pages_after_its_decision_restarted_off_is_booked_as_on() {
        a_failed_exchange_restarted_with_industry_off(Pages::AfterTheDecision, "off-failed-after");
    }

    #[test]
    fn a_failed_exchange_with_pages_before_its_decision_restarted_off_is_never_sent() {
        a_failed_exchange_restarted_with_industry_off(
            Pages::BeforeTheDecision,
            "off-failed-before",
        );
    }

    #[test]
    fn a_failed_exchange_restarted_off_and_crashed_while_off_is_never_sent() {
        a_failed_exchange_restarted_with_industry_off(
            Pages::BeforeAndACrashWhileOff,
            "off-failed-crash-off",
        );
    }
}
