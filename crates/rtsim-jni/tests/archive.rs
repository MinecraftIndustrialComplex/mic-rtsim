// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive behind the engine (VIEWER.md §10.3 to §10.5), on the fake Java side of
//! `common`: a save writes the events up to its snapshot before its page commit, a restart cuts
//! the chunks whose save serial is above the generation it starts from (a crash between the
//! archive's flush and the page commit, a pressure save rolled back), and the journal's replay
//! records the cut events once.

mod common;

use std::path::Path;
use std::sync::atomic::Ordering::Acquire;
use std::time::{Duration, Instant};

use common::*;
use rtsim_archive::{ArchiveReader, Row};
use rtsim_jni::engine::{Engine, MODULE_HERDS, archive_dir};
use rtsim_jni::orders::Order;
use rtsim_jni::wire::{self, obs};
use rtsim_kernel::page::{Durability, PageStore};

/// The core's four phase 1 modules.
const PHASE1: u32 = 0b1111;

/// Journals a VILLAGE_ADOPTED for a village whose meeting point is (x, z). A free function, so it
/// stays one when the fake Java side moves to rtsim-host (V2).
fn adopt(j: &mut Java, village: u64, structure: &str, x: i32, z: i32) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let st = j.dict_id(wire::DICT_STRUCTURE, structure);
    let cu = j.dict_id(wire::DICT_CULTURE, "human");
    use obs::village_adopted as k;
    j.journal(obs::VILLAGE_ADOPTED, k::SIZE, 0, |r| {
        wire::put_u64(r, k::VILLAGE, village);
        wire::put_u32(r, k::DIM, dim);
        wire::put_u32(r, k::STRUCTURE, st);
        wire::put_u32(r, k::CULTURE, cu);
        wire::put_u32(r, k::PIECES, 90);
        wire::put_i32(r, k::MIN_X, x - 20);
        wire::put_i32(r, k::MIN_Y, 60);
        wire::put_i32(r, k::MIN_Z, z - 20);
        wire::put_i32(r, k::MAX_X, x + 20);
        wire::put_i32(r, k::MAX_Y, 80);
        wire::put_i32(r, k::MAX_Z, z + 20);
        wire::put_i32(r, k::MEET_X, x);
        wire::put_i32(r, k::MEET_Y, 64);
        wire::put_i32(r, k::MEET_Z, z);
    });
}

/// A save at the next step, waited for until its pages are on disk.
fn save(e: &Engine, j: &mut Java) {
    let done = e.stats.saves_done.load(Acquire);
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.saves_done.load(Acquire) == done {
        assert!(Instant::now() < deadline, "the save never committed");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// An event as the comparison sees it: kind name, tick, frame name, place, subject, object,
/// magnitude.
type Ev = (String, u64, String, i32, i32, u128, u128, i32);

/// Every event row in the archive under `pages`, sorted, and the save serials its chunks are
/// tagged with.
fn events(pages: &Path) -> (Vec<Ev>, Vec<u64>) {
    let Some(r) = ArchiveReader::open(archive_dir(pages), None).unwrap() else {
        return (Vec::new(), Vec::new());
    };
    let d = r.dictionary();
    let mut out: Vec<Ev> = r
        .rows(0, u64::MAX)
        .unwrap()
        .into_iter()
        .filter_map(|rec| match rec.row {
            Row::Event(ev) => Some((
                d.kind_name(ev.kind).unwrap().to_string(),
                ev.tick,
                d.frame_name(ev.frame).unwrap().to_string(),
                ev.x,
                ev.z,
                d.resolve(ev.subject).unwrap(),
                d.resolve(ev.object).unwrap(),
                ev.magnitude,
            )),
            Row::Leg(_) => None,
        })
        .collect();
    out.sort();
    (out, tags(&r))
}

/// The phase 1 run both halves of the crash test share up to the second save: village A joins
/// and is saved, then B and C join. Returns the first save's observations and serial.
fn villages(tag: &str) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java, u64, u64) {
    let dir = tmpdir(tag);
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    adopt(
        &mut j,
        0x0001_0000_0000_0001,
        "minecraft:village_plains",
        0,
        0,
    );
    j.idle();
    save(&e, &mut j);
    let first = e.durable_obs_seq();
    let serial = e.stats.archive_confirmed.load(Acquire);
    assert_eq!(serial, e.stats.archive_serial.load(Acquire));
    adopt(
        &mut j,
        0x0001_0000_0000_0002,
        "minecraft:village_taiga",
        400,
        0,
    );
    adopt(
        &mut j,
        0x0001_0000_0000_0003,
        "minecraft:village_desert",
        0,
        400,
    );
    j.idle();
    (dir, e, j, first, serial)
}

/// Exit test (c) of VIEWER.md §16 V3: save, step, save, and the process dies after the second
/// save's archive flush but before its page commit. The restart's generation is the first save's,
/// so the second save's chunks are cut before anything is written, and the journal's replay
/// records their events again: the archive then holds every event of an uncrashed run once.
#[test]
fn a_crash_before_the_page_commit_cuts_the_saves_chunks_and_the_replay_records_them_once() {
    // The run that does not crash.
    let (dir, e, mut j, _, _) = villages("archive-whole");
    // B's and C's joins wait in the buffer for the next save, and a host that saves on its own
    // clock can see how full it is.
    let waiting = e.stats.archive_pending_rows.load(Acquire);
    assert!(waiting >= 2, "{waiting} rows waiting");
    assert!(e.stats.archive_pending_bytes.load(Acquire) >= 32 * waiting);
    save(&e, &mut j);
    e.close(true, Duration::from_secs(5)).unwrap();
    assert_eq!(e.stats.archive_failures.load(Acquire), 0);
    assert_eq!(
        e.stats.archive_pending_rows.load(Acquire),
        0,
        "the last save took them"
    );
    let (whole, _) = events(&dir);
    let joined = |v: &[Ev]| v.iter().filter(|r| r.0 == "settle.joined").count();
    assert_eq!(joined(&whole), 3, "three villages joined: {whole:?}");

    // The run that crashes between the second save's archive flush and its page commit.
    let (dir, e, mut j, first, serial) = villages("archive-crash");
    let written = e.stats.archive_rows.load(Acquire);
    e.crash_before_next_page_commit();
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.archive_rows.load(Acquire) == written {
        assert!(Instant::now() < deadline, "the second save never flushed");
        std::thread::sleep(Duration::from_millis(2));
    }
    let game_time = j.game_time;
    let saved = j.crash();
    assert_eq!(
        e.durable_obs_seq(),
        first,
        "the second save's pages never landed"
    );
    let (before, tags) = events(&dir);
    assert!(
        tags.iter().any(|&t| t > serial),
        "the second save's chunks are on disk, tagged past the first save ({serial}): {tags:?}"
    );
    assert_eq!(joined(&before), 3, "{before:?}");
    {
        let store = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_eq!(store.obs_seq(), first, "the pages are at the first save");
    }

    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), saved);
    j.module_mask = PHASE1;
    // The world's clock is the one its save holds (the harness starts every session at 1000).
    j.game_time = game_time;
    assert_eq!(j.pages_obs, first);
    assert_eq!(e.stats.archive_confirmed.load(Acquire), serial);
    let (after_open, tags) = events(&dir);
    assert!(
        tags.iter().all(|&t| t <= serial),
        "no chunk past the generation the restart selected ({serial}): {tags:?}"
    );
    assert_eq!(joined(&after_open), 1, "only A's join was saved");
    // The journal replays B and C; a save writes their events again.
    j.idle();
    save(&e, &mut j);
    e.close(true, Duration::from_secs(5)).unwrap();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    assert_eq!(e.stats.archive_failures.load(Acquire), 0);
    let (replayed, _) = events(&dir);
    eprintln!(
        "archive crash test: {} events whole, {} on disk at the crash, {} after the cut, {} after the replay",
        whole.len(),
        before.len(),
        after_open.len(),
        replayed.len()
    );
    assert_eq!(
        replayed, whole,
        "the replay recorded the cut events once, as the uncrashed run did"
    );
}

/// The live half of the pressure tests: 128 herds under a 4 KiB row budget, a save that is
/// confirmed at once, then days of herds moving with no save asked for: the budget takes saves of
/// its own, pending generations whose chunks carry their own serials (PF15). `journaled`: a record
/// goes into the journal each step, so the budget saves' observations pass the journal file taken
/// right after the save; without it the herds journal nothing as they move and the pressure saves
/// share the confirmed generation's observations: only the serial tells their chunks apart.
struct Pressure {
    dir: std::path::PathBuf,
    budget: usize,
    /// The journal's file as of the confirmed save.
    file: WorldSave,
    /// The journal at the crash.
    crashed: WorldSave,
    confirmed: u64,
    confirmed_obs: u64,
    pressure: u64,
}

fn pressure_run(tag: &str, journaled: bool) -> Pressure {
    let dir = tmpdir(tag);
    let budget = 4 * 1024;
    let e = engine_with(&dir, 256 * 1024, |c| c.row_budget = budget);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1 | MODULE_HERDS;
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
    e.order(unit).unwrap();
    j.idle();
    save(&e, &mut j);
    let confirmed_obs = e.durable_obs_seq();
    let confirmed = e.stats.archive_confirmed.load(Acquire);
    assert!(confirmed > 0);
    let file = j.world_save();
    // The herds move on for days with no save asked for: the budget takes saves of its own.
    for i in 0..80u64 {
        if journaled {
            j.transfer(wire::JAVA_OP_BIT | (i + 1), 1, 2, 1);
        }
        j.game_time += 1_200;
        j.idle();
    }
    let pressure = e.stats.pressure_saves.load(Acquire);
    assert!(pressure > 0, "the budget took saves of its own");
    // The pending generations' rows reach the disk with them.
    let deadline = Instant::now() + Duration::from_secs(20);
    while events_tags(&dir).iter().all(|&t| t <= confirmed) {
        assert!(
            Instant::now() < deadline,
            "no chunk from a pressure save ({pressure} saves, {} rows written)",
            e.stats.archive_rows.load(Acquire)
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        e.stats.archive_confirmed.load(Acquire),
        confirmed,
        "no WORLD_SAVED came: pressure saves do not confirm"
    );
    let crashed = j.crash();
    {
        let store = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_ne!(store.current(), store.confirmed(), "pending generations");
        assert_eq!(store.confirmed().obs_seq, confirmed_obs);
    }
    Pressure {
        dir,
        budget,
        file,
        crashed,
        confirmed,
        confirmed_obs,
        pressure,
    }
}

/// Item 3 of VIEWER.md §10.5: a save taken for the paged modules' budgets (a pending generation,
/// PF15) tags its chunks with its own serial, and a restart that rolls those generations back cuts
/// them. The records journaled since the confirmed save move the pressure saves' observations
/// past the journal's file (the world they belong to died with the crash), so the start (slice
/// U-h: it opens the generation that file holds) rolls them back.
#[test]
fn a_rolled_back_pressure_save_takes_its_chunks_with_it() {
    let p = pressure_run("archive-pressure", true);
    let above = events_tags(&p.dir)
        .into_iter()
        .filter(|&t| t > p.confirmed)
        .count();
    let e = engine_with(&p.dir, 256 * 1024, |c| c.row_budget = p.budget);
    let j = Java::open(e.clone(), p.file);
    assert_eq!(
        j.pages_obs, p.confirmed_obs,
        "the start rolled the pending generations back"
    );
    assert_eq!(e.stats.archive_confirmed.load(Acquire), p.confirmed);
    let tags = events_tags(&p.dir);
    eprintln!(
        "archive pressure test: {} pressure saves, {above} chunks past the confirmed save cut, {} left",
        p.pressure,
        tags.len()
    );
    assert!(above > 0);
    assert!(
        tags.iter().all(|&t| t <= p.confirmed),
        "every pressure save's chunk was cut: {tags:?}"
    );
    let _ = j.crash();
}

/// The other half (slice U-h): the herds journal nothing as they move, so the pressure saves
/// share the confirmed generation's observations and the crash-time journal holds them. The start
/// keeps the newest and confirms it, and its chunks stay: the archive holds what the pages do.
#[test]
fn a_kept_pressure_save_keeps_its_chunks() {
    let p = pressure_run("archive-pressure-kept", false);
    let before = events_tags(&p.dir);
    let newest = *before.iter().max().unwrap();
    assert!(newest > p.confirmed);
    let e = engine_with(&p.dir, 256 * 1024, |c| c.row_budget = p.budget);
    let j = Java::open(e.clone(), p.crashed);
    assert_eq!(j.pages_obs, p.confirmed_obs, "the pressure saves share its observations");
    assert_eq!(
        e.stats.archive_confirmed.load(Acquire),
        e.stats.archive_serial.load(Acquire),
        "the generation the start kept is the archive's confirmed one"
    );
    assert!(
        e.stats.archive_confirmed.load(Acquire) > p.confirmed,
        "a pressure save's serial"
    );
    let tags = events_tags(&p.dir);
    assert!(
        tags.iter().any(|&t| t > p.confirmed),
        "the kept save's chunks stay: {tags:?}"
    );
    let store = PageStore::open_read_only(&p.dir).unwrap();
    assert_eq!(store.current(), store.confirmed(), "confirmed by the start");
    drop(store);
    let _ = j.crash();
}

/// A world save and its WORLD_SAVED (the bridge's order), waited for until the engine's pages are
/// confirmed through it: the save's serial is then the archive's confirmed one.
fn world_save_confirmed(e: &Engine, j: &mut Java) -> WorldSave {
    let through = j.next_obs - 1;
    let file = j.save_world();
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.durable_obs_seq() < through {
        assert!(Instant::now() < deadline, "the world save never confirmed");
        std::thread::sleep(Duration::from_millis(2));
    }
    j.idle();
    file
}

/// Compaction never passes the confirmed save (U-h, docs/choices-leg-purge.md LP12): a world save
/// the engine has committed stays a pending generation until its WORLD_SAVED, so the serial the
/// archive compacts through is the save before it, and the pending save's chunk is left whole. The
/// WORLD_SAVED moves the confirmed serial to it.
#[test]
fn the_archive_compacts_only_through_the_confirmed_save() {
    let dir = tmpdir("archive-confirm");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    adopt(&mut j, 0x0001_0000_0000_0001, "minecraft:village_plains", 0, 0);
    j.idle();
    world_save_confirmed(&e, &mut j);
    let confirmed = e.stats.archive_confirmed.load(Acquire);
    assert!(confirmed > 0);
    assert_eq!(confirmed, e.stats.archive_serial.load(Acquire));
    adopt(&mut j, 0x0001_0000_0000_0002, "minecraft:village_taiga", 400, 0);
    j.idle();
    // The second world save: its pages are asked for, its WORLD_SAVED is not sent yet.
    let through = j.next_obs - 1;
    let committed = e.stats.saves_committed.load(Acquire);
    e.request_save(through);
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.saves_committed.load(Acquire) == committed {
        assert!(Instant::now() < deadline, "the second save never committed");
        std::thread::sleep(Duration::from_millis(2));
    }
    j.idle();
    let serial = e.stats.archive_serial.load(Acquire);
    assert!(serial > confirmed, "the second save recorded: {serial}");
    assert_eq!(
        e.stats.archive_confirmed.load(Acquire),
        confirmed,
        "the archive compacts through the confirmed save, not the pending one"
    );
    assert!(
        events_tags(&dir).contains(&serial),
        "the pending save's chunk is on disk, whole: {:?}",
        events_tags(&dir)
    );
    j.written_through = Some(through);
    j.world_saved(through);
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.archive_confirmed.load(Acquire) != serial {
        assert!(Instant::now() < deadline, "the confirm never moved the serial");
        std::thread::sleep(Duration::from_millis(2));
    }
    e.close(true, Duration::from_secs(5)).unwrap();
    assert_eq!(e.stats.archive_failures.load(Acquire), 0);
}

/// A world save whose journal file never landed (the process died before it, or the write was
/// held) is rolled back at the start from the older file, and its chunks go with it: the archive
/// holds only what the world the file describes had recorded.
#[test]
fn a_rolled_back_world_save_takes_its_chunks_with_it() {
    let dir = tmpdir("archive-world-rollback");
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    adopt(&mut j, 0x0001_0000_0000_0001, "minecraft:village_plains", 0, 0);
    j.idle();
    let first_obs = j.next_obs - 1;
    let first = world_save_confirmed(&e, &mut j);
    let confirmed = e.stats.archive_confirmed.load(Acquire);
    assert!(confirmed > 0);
    adopt(&mut j, 0x0001_0000_0000_0002, "minecraft:village_taiga", 400, 0);
    adopt(&mut j, 0x0001_0000_0000_0003, "minecraft:village_desert", 0, 400);
    j.idle();
    // The second world save: committed by the engine, never confirmed.
    let committed = e.stats.saves_committed.load(Acquire);
    e.request_save(j.next_obs - 1);
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.saves_committed.load(Acquire) == committed {
        assert!(Instant::now() < deadline, "the second save never committed");
        std::thread::sleep(Duration::from_millis(2));
    }
    let (on_disk, _) = events(&dir);
    assert_eq!(
        on_disk.iter().filter(|r| r.0 == "settle.joined").count(),
        3,
        "the second save's events are on disk: {on_disk:?}"
    );
    assert!(events_tags(&dir).iter().any(|&t| t > confirmed));
    let game_time = j.game_time;
    let _ = j.crash();

    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), first);
    j.module_mask = PHASE1;
    j.game_time = game_time;
    assert_eq!(j.pages_obs, first_obs, "the start rolled the second save back");
    assert_eq!(e.stats.archive_confirmed.load(Acquire), confirmed);
    let (after, tags) = events(&dir);
    assert!(
        tags.iter().all(|&t| t <= confirmed),
        "no chunk past the generation the restart selected ({confirmed}): {tags:?}"
    );
    assert_eq!(
        after.iter().filter(|r| r.0 == "settle.joined").count(),
        1,
        "only the first village's join: the other two died with the world's file: {after:?}"
    );
    let _ = j.crash();
}

/// The save serials the archive's chunks are tagged with.
fn events_tags(pages: &Path) -> Vec<u64> {
    ArchiveReader::open(archive_dir(pages), None)
        .unwrap()
        .map(|r| tags(&r))
        .unwrap_or_default()
}

fn tags(r: &ArchiveReader) -> Vec<u64> {
    r.entries().iter().map(|e| e.header.serial).collect()
}
