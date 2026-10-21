// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive behind the engine (VIEWER.md §10): the modules' tapes go into the recorder
//! in the engine's state each step, a save takes the recorder's rows at its snapshot, and the IO
//! lane writes them as chunks right before the page commit, then compacts.
//!
//! - **The save serial** ([`KEY`]): every save the session records takes the next serial, its
//!   chunks carry it, and its pages hold it. Observation sequences alone cannot tag a save: only
//!   journaled records move them, so saves with none between them (the budget's pressure saves
//!   while the world is quiet) share one.
//! - **Open** (§10.5): after the pages are at the generation this session starts from, the archive
//!   cuts every chunk tagged above that generation's serial before anything is written (a crash or
//!   a rolled-back pressure save left them describing a past the journal replays), and the
//!   recorder starts from the stored dictionary, so numbers keep their meaning.
//! - **Tapes** are turned on once every module has loaded: a module that restores by replaying its
//!   inputs records nothing twice.
//! - **The flush** never fails a save: an error is logged and counted (`archive_failures`), and the
//!   archive counts the rows it lost.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use rtsim_archive::{Archive, Batch, Dictionary, Recorder, Stamp};
use rtsim_kernel::page::{PageKey, PageStore};

use super::{State, Stats, lock, log_error, log_info, log_warn};

/// The archive's folder under the pages folder: `<world>/rtsim/archive`, where the viewer reads it
/// (`rtsim_archive::dir_for`).
pub const DIR: &str = "archive";

pub fn dir(pages_dir: &Path) -> PathBuf {
    pages_dir.join(DIR)
}

/// The page with the serial of the last save that recorded events: a u64, little endian.
pub const KEY: PageKey = PageKey::new(1, 0x41);

/// The serial the pages' generation holds: 0 for pages that never recorded, or a page that
/// cannot be read (logged), which cuts the whole archive rather than keep chunks of unknown
/// standing.
pub(super) fn load_serial(store: &mut PageStore) -> u64 {
    match store.get(KEY) {
        Ok(None) => 0,
        Ok(Some(b)) if b.len() == 8 => u64::from_le_bytes(b[..].try_into().unwrap()),
        Ok(Some(b)) => {
            log_warn(&format!(
                "the event archive's serial page has {} bytes, not 8: the archive starts again",
                b.len()
            ));
            0
        }
        Err(e) => {
            log_warn(&format!(
                "cannot read the event archive's serial page ({e}): the archive starts again"
            ));
            0
        }
    }
}

/// Opens the archive for a session whose pages hold save serial `serial`, and cuts what lies
/// above it. `None` (logged and counted) when it cannot open: the session runs without one.
pub(super) fn open(pages_dir: &Path, serial: u64, stats: &Stats) -> Option<Archive> {
    let path = dir(pages_dir);
    let mut a = match Archive::open(&path) {
        Ok(a) => a,
        Err(e) => {
            stats.archive_failures.fetch_add(1, Ordering::AcqRel);
            log_error(&format!(
                "cannot open the event archive in {}: {e}; this session records no events",
                path.display()
            ));
            return None;
        }
    };
    if a.was_reset() {
        log_warn("the event archive's dictionary was damaged: the archive starts again empty");
    }
    match a.cut_above_serial(serial) {
        Ok(0) => {}
        Ok(n) => log_info(&format!(
            "cut {n} event archive chunks past save {serial}; the journal's replay records them again"
        )),
        Err(e) => {
            stats.archive_failures.fetch_add(1, Ordering::AcqRel);
            log_error(&format!(
                "cannot cut the event archive back to save {serial}: {e}; this session records no events"
            ));
            return None;
        }
    }
    stats.archive_serial.store(serial, Ordering::Release);
    stats.archive_confirmed.store(serial, Ordering::Release);
    stats
        .archive_dropped
        .store(a.dropped().rows(), Ordering::Release);
    Some(a)
}

/// The recorder for a session on `archive` whose pages hold save serial `serial`, every module's
/// tape on. Called once the modules have loaded.
pub(super) fn start(st: &mut State, archive: &Archive, serial: u64) {
    st.archive = Recorder::new(archive.dictionary().clone());
    st.archive_on = true;
    st.archive_serial = serial;
    for_each_tape(st, |t| t.set_on(true));
}

/// Every module's tape, in one order.
fn for_each_tape(st: &mut State, mut f: impl FnMut(&mut rtsim_archive::Tape)) {
    f(&mut st.industry.engine.tape);
    f(&mut st.phase1.world.tape);
    f(&mut st.phase1.world.knowledge.tape);
    f(&mut st.dragons.engine.tape);
    f(&mut st.herds.engine.tape);
    f(&mut st.ships.engine.tape);
    f(&mut st.conflict.engine.tape);
}

/// Moves the modules' rows into the recorder (each step, and at a save's snapshot), and says how
/// full its buffer is (`archive_pending_rows`, `archive_pending_bytes`): past half of either cap
/// (`rtsim_archive::pending::MAX_PENDING_ROWS`, `MAX_PENDING_BYTES`) a host that saves on its own
/// clock saves sooner, so a fast run drops nothing (§12).
pub(super) fn absorb(st: &mut State, stats: &Stats) {
    if !st.archive_on {
        return;
    }
    let mut rec = std::mem::take(&mut st.archive);
    for_each_tape(st, |t| rec.absorb(t));
    st.archive = rec;
    pending(st, stats);
}

fn pending(st: &State, stats: &Stats) {
    let p = st.archive.pending();
    stats
        .archive_pending_rows
        .store(p.len() as u64, Ordering::Release);
    stats
        .archive_pending_bytes
        .store(p.bytes() as u64, Ordering::Release);
}

/// One save's rows on their way to the IO lane.
pub(super) struct Taken {
    batch: Batch,
    dict: Option<Dictionary>,
    /// The save's serial, which its pages hold under [`KEY`].
    pub(super) serial: u64,
    tick: u64,
    wall_ms: u64,
}

/// The rows up to this snapshot, with the save's serial (on the step thread, at the save's
/// snapshot).
pub(super) fn take(st: &mut State, stats: &Stats) -> Option<Taken> {
    if !st.archive_on {
        return None;
    }
    absorb(st, stats);
    let (batch, dict) = st.archive.take();
    pending(st, stats);
    st.archive_serial += 1;
    Some(Taken {
        batch,
        dict,
        serial: st.archive_serial,
        tick: u64::try_from(st.game_time).unwrap_or(0),
        wall_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
    })
}

/// Writes a save's rows, tagged with its serial and the observations its generation includes (on
/// the IO lane, before the page commit). Never fails the save.
pub(super) fn flush(archive: &Mutex<Option<Archive>>, taken: Taken, obs_seq: u64, stats: &Stats) {
    let mut guard = lock(archive);
    let Some(a) = guard.as_mut() else { return };
    let n = taken.batch.rows.len() as u64;
    stats.archive_serial.store(taken.serial, Ordering::Release);
    let stamp = Stamp {
        obs_seq,
        serial: taken.serial,
        tick: taken.tick,
        wall_ms: taken.wall_ms,
    };
    match a.flush(taken.batch, stamp, taken.dict.as_ref()) {
        Ok(_) => {
            stats.archive_rows.fetch_add(n, Ordering::AcqRel);
        }
        Err(e) => {
            stats.archive_failures.fetch_add(1, Ordering::AcqRel);
            log_error(&format!(
                "writing the event archive failed (the save goes on without its events): {e}"
            ));
        }
    }
    stats
        .archive_dropped
        .store(a.dropped().rows(), Ordering::Release);
}

/// Compaction after a page commit: never past `confirmed`, the serial of the generation a restart
/// would select.
pub(super) fn compact(archive: &Mutex<Option<Archive>>, confirmed: u64, stats: &Stats) {
    let mut guard = lock(archive);
    let Some(a) = guard.as_mut() else { return };
    if let Err(e) = a.compact(confirmed) {
        stats.archive_failures.fetch_add(1, Ordering::AcqRel);
        log_error(&format!("compacting the event archive failed: {e}"));
    }
    stats
        .archive_dropped
        .store(a.dropped().rows(), Ordering::Release);
}
