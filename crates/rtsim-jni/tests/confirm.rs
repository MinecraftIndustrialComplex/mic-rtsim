// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slice U-h (P29, docs/choices-leg-purge.md LP12 option a): the engine's world save is a pending
//! generation until the bridge's WORLD_SAVED says the journal's file holds it. The confirm, on the
//! IO lane, moves the acknowledgement, `saves_done` and the archive's confirmed serial; a start
//! selects the generation the journal on disk holds; a module's budget save waits behind a world
//! save that is not confirmed; a stop confirms its final save at once, if the journal's file holds it
//! (slice U-i: `close_through`; a stop whose last journal write was held leaves it pending).
//!
//! The fake Java side (`common::Java`) sends WORLD_SAVED as the bridge does (`Java::world_saved`,
//! `Java::save_world`) and clamps the trim to the journal file's `writtenThrough` (JB9).

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Acquire;
use std::time::{Duration, Instant};

use common::*;
use rtsim_jni::engine::{BUDGET_HOLD_HANDOVERS, Engine, MODULE_HERDS};
use rtsim_jni::orders::Order;
use rtsim_jni::wire;
use rtsim_kernel::page::{Fault, PageStore};

/// Indexes of `Engine::stats_array` (Java's order).
const APPLIED: usize = 4;
const SAVES_DONE: usize = 9;

/// A wall clock, wide on purpose: the step pool and the IO lane share the machine with every
/// other test and agent. Waits that should never time out end at the first yes.
const WAIT: Duration = Duration::from_secs(30);

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits until the IO lane has committed `n` saves of any kind (a commit lands without Java).
fn wait_committed(e: &Engine, n: u64) {
    wait_for(&format!("{n} committed saves"), || {
        e.stats.saves_committed.load(Acquire) >= n
    });
}

fn ledger(e: &Engine, store: u64) -> i64 {
    e.inspect().ledger.get(&store).copied().unwrap_or(0)
}

fn transfers(j: &mut Java, from: u64, n: u64) {
    for i in 0..n {
        j.transfer(wire::JAVA_OP_BIT | (from + i), 1, 2, 1);
    }
}

/// The store's (current, confirmed) generations, read while the engine runs.
fn generations(
    dir: &Path,
) -> (
    rtsim_kernel::page::Generation,
    rtsim_kernel::page::Generation,
) {
    let s = PageStore::open_read_only(dir).unwrap();
    (s.current(), s.confirmed())
}

/// Two hand-overs, then the commit (it lands on the IO lane by itself), then two more so that a
/// command header after the commit has been read.
fn settle(e: &Engine, j: &mut Java, committed: u64) {
    j.idle();
    wait_committed(e, committed);
    j.idle();
}

/// A world save is acknowledged only after its WORLD_SAVED: the pages are committed as a pending
/// generation, the header names nothing, `saves_done` stays; the WORLD_SAVED confirms the
/// generation on the IO lane, and the next header acknowledges it and trims the journal.
#[test]
fn a_world_save_is_acknowledged_only_after_its_world_saved() {
    let dir = tmpdir("uh-f1");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 10);
    j.idle();
    let t = j.next_obs - 1;
    assert_eq!(t, 12, "the session's JOURNAL_GEN and its module switch, then ten transfers");
    e.request_save(t);
    settle(&e, &mut j, 1);
    assert_eq!(e.committed_obs_seq(), t, "the save holds the journal's end");
    j.idle();
    assert!(
        e.durable_obs_seq() < t,
        "no acknowledgement before WORLD_SAVED: durable {}",
        e.durable_obs_seq()
    );
    assert!(j.acks.iter().all(|a| *a < t), "no ack: {:?}", j.acks);
    assert_eq!(e.stats_array()[SAVES_DONE], 0, "not a save done yet");
    let (current, confirmed) = generations(&dir);
    assert_ne!(current, confirmed, "the generation is pending");
    assert_eq!(current.obs_seq, t);
    assert_eq!(e.stats.confirms.load(Acquire), 0);

    // The journal's file landed: WORLD_SAVED.
    j.written_through = Some(t);
    j.world_saved(t);
    j.idle();
    wait_for("the confirm", || e.durable_obs_seq() == t);
    j.idle();
    assert!(j.acks.contains(&t), "the ack follows: {:?}", j.acks);
    assert!(
        j.journal.front().is_none_or(|(s, _)| *s > t),
        "the journal is trimmed to the save: {:?}",
        j.journal.front().map(|(s, _)| *s)
    );
    assert_eq!(
        e.stats_array()[SAVES_DONE],
        1,
        "one save done, at the confirm"
    );
    assert_eq!(e.stats.confirms.load(Acquire), 1);
    let (current, confirmed) = generations(&dir);
    assert_eq!(current, confirmed, "confirmed");
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// WORLD_SAVED reaches the engine before the save it covers is committed: in the very hand-over
/// that takes the save (the headless host's order), and, with an engine that lags the journal's
/// end, many hand-overs before it. The engine remembers it and confirms once the save commits.
#[test]
fn a_world_saved_ahead_of_its_save_confirms_it_once_committed() {
    // The same hand-over.
    let dir = tmpdir("uh-f2-same");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    let t = j.next_obs - 1;
    j.save_world();
    j.idle();
    wait_for("the confirm", || e.durable_obs_seq() == t);
    j.idle();
    assert!(j.acks.contains(&t), "the ack follows: {:?}", j.acks);
    assert_eq!(e.stats_array()[SAVES_DONE], 1);
    assert_eq!(e.stats.confirms.load(Acquire), 1);
    e.close(true, Duration::from_secs(5)).unwrap();

    // An engine that lags: more journal than two buffers carry, WORLD_SAVED in the first of
    // them, the save taken when the engine reaches the world save's end.
    let dir = tmpdir("uh-f2-lag");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.idle();
    let n = (3 * wire::MIN_CAPACITY / wire::obs::transfer::SIZE) as u64;
    transfers(&mut j, 200, n);
    let t = j.next_obs - 1;
    j.save_world();
    j.flush();
    assert!(
        e.stats_array()[APPLIED] < t,
        "the engine lags the world save's end when it hears WORLD_SAVED: applied {}, saved {t}",
        e.stats_array()[APPLIED]
    );
    j.run_until("the confirm", |j| j.e.durable_obs_seq() == t);
    j.run_until("the ack", |j| j.acks.contains(&t));
    assert_eq!(e.stats_array()[SAVES_DONE], 1);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// What a session left: the first world save confirmed, then more transfers and a second save
/// the engine committed that no WORLD_SAVED ever confirmed.
struct Left {
    dir: PathBuf,
    /// The journal's file at the first world save, and the observations it holds.
    first: WorldSave,
    through_first: u64,
    /// The journal's file at the second world save (it landed, its WORLD_SAVED did not).
    second: WorldSave,
    through_second: u64,
}

fn leave(tag: &str) -> Left {
    let dir = tmpdir(tag);
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    let first = j.save_world();
    let through_first = j.next_obs - 1;
    j.idle();
    wait_for("the first save's confirm", || {
        e.durable_obs_seq() == through_first
    });
    j.idle();
    assert_eq!(ledger(&e, 2), 5);
    transfers(&mut j, 200, 5);
    j.idle();
    let through_second = j.next_obs - 1;
    // The second world save: the pages are asked for and the journal's file is written, and the
    // process dies before WORLD_SAVED.
    e.request_save(through_second);
    let second = j.world_save();
    settle(&e, &mut j, 2);
    assert_eq!(e.committed_obs_seq(), through_second);
    assert_eq!(ledger(&e, 2), 10);
    let _ = j.crash();
    Left {
        dir,
        first,
        through_first,
        second,
        through_second,
    }
}

/// The pages the second world save wrote are ahead of the first save's journal file: a restart
/// from the first file rolls them back, and the replay brings the world to what the file holds.
#[test]
fn a_save_whose_journal_never_landed_is_rolled_back_at_start() {
    let left = leave("uh-f3");
    let e = engine(&left.dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), left.first);
    assert_eq!(
        j.pages_obs, left.through_first,
        "the start selected the generation the first journal file holds"
    );
    j.idle();
    j.idle();
    assert_eq!(
        ledger(&e, 2),
        5,
        "the ledger is the first save's: the second save's transfers died with its file"
    );
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The same crash, restarted from the second file (it landed; only WORLD_SAVED died): the
/// generation is kept, and confirmed by the start.
#[test]
fn a_save_whose_journal_landed_is_kept_and_confirmed_at_start() {
    let left = leave("uh-f4");
    let e = engine(&left.dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), left.second);
    assert_eq!(j.pages_obs, left.through_second, "the second save is kept");
    j.idle();
    j.idle();
    assert_eq!(ledger(&e, 2), 10);
    assert_eq!(e.durable_obs_seq(), left.through_second);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let _ = j.crash();
    let (current, confirmed) = generations(&left.dir);
    assert_eq!(current, confirmed, "the start confirmed it");
    assert_eq!(current.obs_seq, left.through_second);
}

/// Both generations hold observations the world's save lacks (a world older than its pages): the
/// start still opens, as it did before the journal chose the generation, and the replay skips
/// what the pages have. Refusing the start would be a regression (P29's EngineAhead).
#[test]
fn a_start_with_both_generations_ahead_of_the_journal_still_opens() {
    let dir = tmpdir("uh-f5");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    wait_for("the save", || e.durable_obs_seq() > 0);
    let at = e.durable_obs_seq();
    assert!(at > 0);
    let _ = j.crash();
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    assert_eq!(j.pages_obs, at, "the pages are kept, ahead of the world");
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(ledger(&e, 2), 5, "the pages' state, not replayed twice");
    e.close(true, Duration::from_secs(5)).unwrap();
}

// ---- the hold on budget saves ---------------------------------------------------------------------

const MODULES: u32 = 0b1111 | MODULE_HERDS;
const AT: (i32, i32) = (1_000, 1_000);
const BUDGET: usize = 4 * 1024;

/// `n` herds of 20 sheep, 64 blocks apart along a line, in one order unit.
fn found_many(e: &Engine, j: &mut Java, n: u64) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let unit = (0..n)
        .map(|i| Order::HerdFound {
            name: format!("Testdown flock {i}"),
            species: "minecraft:sheep".into(),
            dimension: dim,
            x: AT.0 + 64 * i as i32,
            z: AT.1,
            count: 20,
            seed: i + 1,
        })
        .collect();
    e.order(unit).unwrap();
    j.idle();
    assert_eq!(e.herd_agents().len() as u64, n, "every herd founded");
}

fn herd_session(tag: &str, budget: usize, herds: u64) -> (std::sync::Arc<Engine>, Java) {
    let dir = tmpdir(tag);
    let e = engine_with(&dir, 256 * 1024, |c| c.row_budget = budget);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = MODULES;
    found_many(&e, &mut j, herds);
    (e, j)
}

fn resident(e: &Engine) -> usize {
    e.with_herds(|h, _| h.rows_resident_bytes())
}

/// Moves the herds on for `steps` hand-overs of 1,200 ticks each, returning the rows' peak.
fn days(j: &mut Java, e: &Engine, steps: u64) -> usize {
    let mut peak = 0;
    for _ in 0..steps {
        j.game_time += 1_200;
        j.flush();
        peak = peak.max(resident(e));
    }
    peak
}

/// A module's budget save never lands past the world save under it while that save waits for its
/// WORLD_SAVED: a budget generation on top could outrun the journal file the WORLD_SAVED reports.
/// While held, saved rows still leave and the rest stay resident, over their budget; once the
/// WORLD_SAVED confirms the save, the budget saves resume and the rows come back under it.
/// Prints the held row peak against the unheld one (a budget save every few steps) and the unpaged
/// footprint (D1).
#[test]
fn a_budget_save_waits_for_the_world_saved_of_the_save_under_it() {
    const STEPS: u64 = 80;
    // The control: no world save waits (a test's save confirms at once), so the budget saves run.
    let (e, mut j) = herd_session("uh-f6-free", BUDGET, 128);
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    wait_committed(&e, 1);
    let free_peak = days(&mut j, &e, STEPS);
    let free_end = resident(&e);
    assert!(
        e.stats.pressure_saves.load(Acquire) > 0,
        "the control's budget saves ran"
    );
    assert_eq!(e.stats.budget_saves_held.load(Acquire), 0);
    let _ = j.crash();
    // The unpaged footprint.
    let (e, mut j) = herd_session("uh-f6-unpaged", 1 << 30, 128);
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    wait_committed(&e, 1);
    let unpaged = days(&mut j, &e, STEPS);
    let _ = j.crash();

    let (e, mut j) = herd_session("uh-f6-held", BUDGET, 128);
    let dir = e.config().pages_dir.clone();
    let t = j.next_obs - 1;
    e.request_save(t);
    settle(&e, &mut j, 1);
    let pressure = e.stats.pressure_saves.load(Acquire);
    let held_peak = days(&mut j, &e, STEPS);
    let held_end = resident(&e);
    let total = e.with_herds(|h, _| h.rows_budget());
    eprintln!(
        "F6 herd rows over {STEPS} hand-overs, budget {total} B: peak {held_peak} B and {held_end} B \
         at the end held behind an unconfirmed world save; {free_peak} B and {free_end} B with budget \
         saves running; {unpaged} B unpaged; {} budget steps held",
        e.stats.budget_saves_held.load(Acquire)
    );
    assert_eq!(
        e.stats.pressure_saves.load(Acquire),
        pressure,
        "no budget save while the world save waits for its WORLD_SAVED"
    );
    assert!(
        e.stats.budget_saves_held.load(Acquire) > 0,
        "steps that wanted a budget save were held"
    );
    assert!(
        held_peak <= unpaged,
        "{held_peak} B held, {unpaged} B unpaged"
    );
    let (current, confirmed) = generations(&dir);
    assert_ne!(current, confirmed, "the world save is still pending");
    assert_eq!(current.obs_seq, t);

    j.written_through = Some(t);
    j.world_saved(t);
    j.idle();
    wait_for("the confirm", || e.durable_obs_seq() == t);
    let after = days(&mut j, &e, STEPS);
    let _ = after;
    assert!(
        e.stats.pressure_saves.load(Acquire) > pressure,
        "the budget saves resume once the world save is confirmed"
    );
    assert!(
        resident(&e) <= total,
        "the rows are back under their budget: {} B against {total} B",
        resident(&e)
    );
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let _ = j.crash();
}

/// A WORLD_SAVED that never comes does not hold the rows over their budget for good: past
/// `BUDGET_HOLD_HANDOVERS` hand-overs the budget save is taken on top of the world save, once the
/// log line says why, and the next world save that is confirmed confirms everything under it.
#[test]
fn a_world_saved_that_never_comes_lets_the_budget_save_after_the_hold() {
    let (e, mut j) = herd_session("uh-f7", BUDGET, 64);
    let dir = e.config().pages_dir.clone();
    let t = j.next_obs - 1;
    e.request_save(t);
    settle(&e, &mut j, 1);
    let grew = e.stats.window_grew.load(Acquire);
    // The founding's own budget save (before the world save was asked for) is not the hold's.
    let before = e.stats.pressure_saves.load(Acquire);
    // Inside the hold: nothing.
    days(&mut j, &e, BUDGET_HOLD_HANDOVERS - 100);
    assert_eq!(e.stats.pressure_saves.load(Acquire), before, "held");
    assert!(e.stats.budget_saves_held.load(Acquire) > 0);
    assert_eq!(e.stats.window_grew.load(Acquire), grew, "no log line yet");
    // Past it: the valve lets one through, and logs once.
    days(&mut j, &e, 300);
    wait_for("a budget save past the hold", || {
        e.stats.pressure_saves.load(Acquire) > before
    });
    assert_eq!(
        e.stats.window_grew.load(Acquire),
        grew + 1,
        "one line for the streak, not one a step"
    );
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    // The next world save, and its WORLD_SAVED, confirm everything under it.
    transfers(&mut j, 500, 3);
    j.idle();
    let t2 = j.next_obs - 1;
    assert!(t2 > t);
    j.save_world();
    j.idle();
    wait_for("the confirm", || e.durable_obs_seq() == t2);
    j.idle();
    assert_eq!(e.stats_array()[SAVES_DONE], 2, "both world saves are done");
    let (current, confirmed) = generations(&dir);
    assert_eq!(current, confirmed);
    assert_eq!(current.obs_seq, t2);
    let _ = j.crash();
}

/// A stop confirms its final save at once: the world's last save has been written by the time the
/// engine closes, and the folder is as the viewer and the next start read it.
#[test]
fn a_stop_confirms_its_final_save_at_once() {
    let dir = tmpdir("uh-f8");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    let t = j.next_obs - 1;
    e.request_save(t);
    settle(&e, &mut j, 1);
    assert!(
        e.durable_obs_seq() < t,
        "the world save waits for its WORLD_SAVED"
    );
    transfers(&mut j, 200, 5);
    j.idle();
    j.idle();
    let applied = e.stats_array()[APPLIED];
    e.close(true, Duration::from_secs(5)).unwrap();
    assert_eq!(e.durable_obs_seq(), applied, "the stop's save is durable");
    let (current, confirmed) = generations(&dir);
    assert_eq!(current, confirmed, "and confirmed");
    assert_eq!(current.obs_seq, applied);
}

// ---- a stop whose last journal write was held (slice U-i) --------------------------------------------

/// A world stopped after two world saves, the second of which the journal's file may not hold.
struct Stopped {
    dir: PathBuf,
    /// The journal's file at the first world save, which landed and was confirmed: `through_first`.
    first: WorldSave,
    through_first: u64,
    /// The journal as the stop's world save serialized it: the file that would have landed, had its
    /// write not been held.
    second: WorldSave,
    /// What the engine's final save includes (the stop's world save took the same).
    final_obs: u64,
}

/// Five transfers and a world save that lands (the file `first`, confirmed), five more and the
/// stop's world save, whose pages are committed and whose WORLD_SAVED never comes (its journal write
/// was held, as U-g7's flush save is), then the stop: `close_through` with what
/// `written_through(through_first, final_obs)` names as the file's last observation.
fn stop(tag: &str, written_through: impl FnOnce(u64, u64) -> u64) -> Stopped {
    let dir = tmpdir(tag);
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    let first = j.save_world();
    let through_first = j.next_obs - 1;
    j.idle();
    wait_for("the first save's confirm", || {
        e.durable_obs_seq() == through_first
    });
    j.idle();
    assert_eq!(ledger(&e, 2), 5);
    transfers(&mut j, 200, 5);
    j.idle();
    let through_second = j.next_obs - 1;
    e.request_save(through_second);
    let second = j.world_save();
    settle(&e, &mut j, 2);
    assert_eq!(e.committed_obs_seq(), through_second);
    assert!(
        e.durable_obs_seq() < through_second,
        "the stop's world save has no WORLD_SAVED"
    );
    assert_eq!(ledger(&e, 2), 10);
    let final_obs = e.stats.applied_through.load(Acquire);
    assert_eq!(final_obs, through_second, "nothing came after the save");
    let through = written_through(through_first, final_obs);
    e.close_through(true, through, Duration::from_secs(5))
        .unwrap();
    Stopped {
        dir,
        first,
        through_first,
        second,
        final_obs,
    }
}

/// The restart: the engine opens on `dir` against the journal's file `save`.
fn restart(dir: &Path, save: WorldSave) -> (std::sync::Arc<Engine>, Java) {
    let e = engine(&dir.to_path_buf(), wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), save);
    j.idle();
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    (e, j)
}

/// The journal's file holds the final save (`writtenThrough` equal to what the save includes, the
/// edge, or past it, a clean stop): the save is confirmed at once, and the restart from that file
/// keeps it.
#[test]
fn a_stop_whose_journal_file_holds_its_final_save_confirms_it_at_once() {
    for (what, past) in [("equal", 0), ("past", 100)] {
        let s = stop(&format!("ui-f15-{what}"), |_, fin| fin + past);
        let (current, confirmed) = generations(&s.dir);
        assert_eq!(current, confirmed, "{what}: the final save is confirmed");
        assert_eq!(current.obs_seq, s.final_obs, "{what}");
        let (e, j) = restart(&s.dir, s.second);
        assert_eq!(j.pages_obs, s.final_obs, "{what}: the restart keeps it");
        assert_eq!(ledger(&e, 2), 10, "{what}");
        e.close(true, Duration::from_secs(5)).unwrap();
    }
}

/// The journal's file lacks part of the final save (the last write was held: `writtenThrough` is
/// the older file's, or just one observation short of the save): the final save stays a pending
/// generation, the older one confirmed, and nothing is acknowledged by the stop.
#[test]
fn a_stop_whose_journal_write_was_held_leaves_its_final_save_pending() {
    for (what, short) in [("older-file", None), ("one-short", Some(1))] {
        let s = stop(&format!("ui-f16-{what}"), |first, fin| {
            short.map_or(first, |n| fin - n)
        });
        let (current, confirmed) = generations(&s.dir);
        assert_ne!(
            current, confirmed,
            "{what}: the final save is pending, not confirmed"
        );
        assert_eq!(current.obs_seq, s.final_obs, "{what}");
        assert_eq!(
            confirmed.obs_seq, s.through_first,
            "{what}: the confirmed generation is the first world save"
        );
    }
}

/// The open case of U-h8: a stop whose last journal write was held, and a restart from the older
/// journal file. The pages the stop saved are rolled back with the journal (the start selects the
/// generation the file holds, no `EngineAhead`), the replay brings the world to what the file
/// holds, and the engine's books are the file's: five transfers, as a world whose last save was the
/// first one. The control stops the same world with its file landed and restarts from it: ten.
#[test]
fn a_restart_from_the_older_file_rolls_back_the_pages_of_a_stop_whose_journal_write_was_held() {
    let held = stop("ui-f17-held", |first, _| first);
    let (e, j) = restart(&held.dir, held.first);
    assert_eq!(
        j.pages_obs, held.through_first,
        "the start selected the generation the older journal file holds, not the final save's ({})",
        held.final_obs
    );
    assert_eq!(
        ledger(&e, 2),
        5,
        "the books are the older file's: the second batch died with its journal write"
    );
    let (current, confirmed) = generations(&held.dir);
    assert_eq!(current, confirmed, "the start rolled the pending save back");
    assert_eq!(current.obs_seq, held.through_first);
    e.close(true, Duration::from_secs(5)).unwrap();

    let control = stop("ui-f17-control", |_, fin| fin);
    let (e, j) = restart(&control.dir, control.second);
    assert_eq!(j.pages_obs, control.final_obs, "the landed file keeps it");
    assert_eq!(
        ledger(&e, 2),
        10,
        "the control's books: both batches, from the file that landed"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The first stop of a new world whose first journal write was held: the file does not exist
/// (`writtenThrough` is 0), the world starts over from an empty journal, and the engine's final
/// save rolls back to the empty pages with it.
#[test]
fn a_stop_of_a_new_world_whose_first_journal_write_was_held_rolls_back_to_empty_pages() {
    let dir = tmpdir("ui-f18");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    j.idle();
    assert_eq!(ledger(&e, 2), 5);
    e.close_through(true, 0, Duration::from_secs(5)).unwrap();
    let (current, confirmed) = generations(&dir);
    assert_ne!(current, confirmed, "the final save is pending");
    let (e, j) = restart(&dir, WorldSave::fresh());
    assert_eq!(j.pages_obs, 0, "the empty pages the world starts over from");
    assert_eq!(ledger(&e, 2), 0, "the engine did not keep the world it lost");
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A confirm that fails (its log record torn) leaves the generation pending and the engine
/// running; the store reopens from its files, and the next WORLD_SAVED, or any commit, confirms it.
#[test]
fn a_confirm_that_fails_keeps_the_save_pending_and_the_next_world_saved_confirms_it() {
    let dir = tmpdir("uh-f14");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    transfers(&mut j, 100, 5);
    j.idle();
    let t = j.next_obs - 1;
    e.request_save(t);
    settle(&e, &mut j, 1);
    e.inject_store_fault(Some(Fault::TornLog));
    j.written_through = Some(t);
    j.world_saved(t);
    j.idle();
    wait_for("the failed confirm", || {
        e.stats.confirm_failures.load(Acquire) >= 1
    });
    j.idle();
    assert!(
        e.durable_obs_seq() < t,
        "a failed confirm acknowledges nothing"
    );
    assert_eq!(e.stats.confirms.load(Acquire), 0);
    assert_eq!(e.stats_array()[SAVES_DONE], 0);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let (current, confirmed) = generations(&dir);
    assert_ne!(current, confirmed, "still pending, the torn record cut");
    // The next WORLD_SAVED (the bridge sends one with every world save).
    j.world_saved(t);
    j.idle();
    wait_for("the confirm", || e.durable_obs_seq() == t);
    j.idle();
    assert_eq!(e.stats.confirms.load(Acquire), 1);
    assert_eq!(e.stats_array()[SAVES_DONE], 1);
    assert!(j.acks.contains(&t), "{:?}", j.acks);
    let (current, confirmed) = generations(&dir);
    assert_eq!(current, confirmed);
    e.close(true, Duration::from_secs(5)).unwrap();
}
