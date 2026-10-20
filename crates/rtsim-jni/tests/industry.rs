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
use rtsim_industry::engine::{Engine as Industry, Mode};
use rtsim_industry::fake::{FDistrict, FInventory, FakeWorld, Recipe, Visit, exposed, survey};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{BlockFlags, InvKind, Obs, Timed};
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
    assert_eq!(j.dict_id(wire::DICT_DIMENSION, OVERWORLD), DIM);
    let ids = Ids {
        ore: Good(j.dict_id(DICT_GOOD, "i/64/test:ore")),
        bar: Good(j.dict_id(DICT_GOOD, "i/64/test:bar")),
        fence: j.dict_id(DICT_BLOCK_STATE, "minecraft:oak_fence[east=true]"),
        mill_block: j.dict_id(DICT_BLOCK_STATE, "create:millstone"),
    };
    let mut w = FakeWorld::new(3, Tunables::default(), Mode::Apply).into_remote();
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
    w.districts.push(FDistrict {
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
    });
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

/// Asks for the district's ledger as the Display Source does (IND_LEDGER_REQUEST, latest-wins)
/// and returns the IND_LEDGER line that comes back.
fn ledger(j: &mut Java, d: DistrictId) -> rtsim_industry::cmd::LedgerLine {
    let before = j.mc().ledgers.len();
    let t = j.mc().t;
    j.industry_obs(&Timed {
        at: t,
        obs: Obs::LedgerRequest { district: d },
    });
    for _ in 0..4 {
        j.mc_tick();
        if j.mc().ledgers.len() > before {
            return j.mc().ledgers.last().unwrap().clone();
        }
    }
    panic!("no IND_LEDGER came back");
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

/// Learn while loaded, unload, replay, reload and reconcile; the ledger while away.
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
    let line = ledger(&mut j, MILL);
    eprintln!("ledger while away: {line:?}");
    assert_eq!(line.district, MILL);
    assert_eq!(line.state, 2, "learned, running abstractly");
    let rate = |g: Good| line.goods.iter().find(|x| x.0 == g).map(|x| x.1);
    // 8 a batch every 100 ticks: 5,760 an hour of 72,000 ticks.
    assert!(
        rate(ids.bar).is_some_and(|r| (r - 5_760).abs() <= 60),
        "bars per hour {:?}",
        rate(ids.bar)
    );
    assert!(rate(ids.ore).is_some_and(|r| (r + 5_760).abs() <= 60));
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
        1,
        "one reconcile round"
    );
    assert!(!r.is_empty() && r.iter().all(|x| x.1 == 1), "epoch 1 applied");
    // Two days away at 8 per 100 ticks: 3,840 bars by the replay, give or take the machine that
    // ticks on stale stock before the reconcile (the trim keeps the books exact).
    let made = bars(&j, &ids);
    let expect = loaded_bars + 3_840 + 160;
    assert!((made - expect).abs() <= 24, "bars {made}, expected about {expect}");
    conserves(&j);
    assert_eq!(j.stale_commands, 0, "no command at or below an applied one");
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
    let stats = j.e.with_industry(|_, s| *s);
    eprintln!("link: {stats:?}");
    assert_eq!(stats.bad_records + stats.unknown_kinds + stats.dropped, 0);
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
    while j.e.durable_obs_seq() < through {
        j.mc_tick();
    }
    assert_eq!(j.e.durable_obs_seq(), through, "the pages stop at the world save");
    let crash_at = match crash {
        Crash::AfterTheResult => day(3.0) + 600,
        Crash::BeforeTheLoad => day(3.0) + 600,
        Crash::ChunkSavedAhead => day(3.1),
    };
    j.mc_run_until(crash_at);
    let first = reconciles(&j);
    let bars_first = bars(&j, &ids);
    assert!(!first.is_empty(), "the reconcile was applied before the crash");
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
    eprintln!("after the restart: reconciles {after:?}, bars {}", bars(&j, &ids));
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
            assert_eq!(epoch1(&after), epoch1(&first), "epoch 1 re-derived exactly, applied once");
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
    assert!(!broken.is_empty(), "the raid broke blocks, applied at the load");
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
    let _ids = mill(&mut j, 20_000, &[(0.0, true), (1.0, false), (3.0, true)], false);
    j.mc_run_until(day(3.0) + 500);
    let seqs: Vec<u64> = j.ind_commands.iter().map(|c| c.1).collect();
    assert!(!seqs.is_empty());
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "sequences rise: {seqs:?}");
    for (_, _, c) in &j.ind_commands {
        if let Some(op) = c.op() {
            assert_eq!(op.0 & wire::JAVA_OP_BIT, 0, "an engine op id");
            assert_eq!((op.0 >> 56) & 0x7f, 0x49, "the industry namespace");
        }
    }
    j.e.close(true, Duration::from_secs(5)).unwrap();
}
