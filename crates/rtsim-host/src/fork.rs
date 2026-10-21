// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fork (VIEWER.md §12.1): a copy of a world's rtsim state the host can run, made through the
//! read-only open (§5.3) so it holds one whole page generation even from a running server, and
//! never written into the world.
//!
//! A fork is laid out as a world, so the save viewer opens one as it opens a world:
//!
//! ```text
//! <dir>/rtsim/                        the pages: the world's confirmed generation (what the
//!                                     engine's start opens), page for page; then the host's saves
//! <dir>/rtsim/archive/                the world's archive (§10.3), when it has one, less the chunks
//!                                     above the copied generation's save serial (§10.5)
//! <dir>/**/data/surveyor/             Surveyor's terrain summaries of each dimension, when there
//!                                     (§9): the base map; the host never changes them
//! <dir>/data/mic_rtsim_journal.dat    the bridge's journal that pairs with those pages
//! <dir>/level.dat                     the game time (the host keeps it current at each save)
//! <dir>/serverconfig/mic_rtsim-server.toml   the module switches, when the world has the file
//! <dir>/<deep time folder>/manifest.json     Deep Time's layout block, when the world has one
//! <dir>/rtsim-fork.json               written last: where the fork came from ([`ForkInfo`])
//! ```
//!
//! The pages are copied first and the journal after, and the copy is taken again if the journal
//! changed meanwhile or no longer covers the pages (a world save landed during the copy), so the
//! pair is one the engine's start would accept. The view store (`rtsim/view/`) and its lease are
//! not copied: they are the viewer's, rebuilt from the pages.
//!
//! The archive is copied after them, on its own: its index, the chunk files the index names, the
//! dictionary, then the index again, and the archive is copied again from the start when the index
//! changed or a chunk it named was gone (the server flushed, compacted or cut it meanwhile). A
//! chunk file is never rewritten under its name, so the copy's index names exactly the chunks
//! copied with it; the dictionary only grows and is written before the chunks that use it, so one
//! read after the index covers them. The copy then loses its chunks above the save serial the
//! copied pages hold, as the engine's start cuts a world's. The fork's host writes this copy, never
//! the world's archive: the engine runs on the fork's pages folder (VWD-11).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rtsim_archive::store::{DICT_FILE, INDEX_FILE, chunk_name};
use rtsim_archive::{Archive, Index};
use rtsim_jni::engine::{ARCHIVE_DIR, ARCHIVE_KEY};
use rtsim_kernel::page::{Durability, PageError, PageStore, SaveBatch};
use serde::{Deserialize, Serialize};

use crate::save::{self, Journal};

/// The marker file: present means the fork is complete.
pub const MARKER: &str = "rtsim-fork.json";
/// Where pages are copied before the fork is complete.
const PARTIAL: &str = "rtsim.partial";
/// Raw page bytes per commit while copying (TUNABLE): bounds the copy's memory.
const COMMIT_BYTES: usize = 64 << 20;
/// Copies taken before the export gives up on a world that keeps saving under it.
pub const ATTEMPTS: u32 = 8;

/// What `rtsim-fork.json` holds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkInfo {
    /// The world folder, as given (made absolute).
    pub source: String,
    /// When the copy finished, Unix milliseconds.
    pub exported_unix_ms: u64,
    /// The world's confirmed page generation copied: its save number and observation sequence.
    pub pages_save_seq: u64,
    pub pages_obs_seq: u64,
    pub pages: u64,
    pub page_bytes: u64,
    /// The journal copied with them: its first observation and `nextObsSeq`.
    pub journal_first_obs: u64,
    pub journal_next_obs: u64,
    /// `level.dat`'s game time at the copy, if the world has one.
    pub game_time: Option<i64>,
    /// Deep Time's world-data folder copied (its manifest only), if any.
    pub deep_time_folder: Option<String>,
    /// Files copied from the world's archive.
    pub archive_files: u64,
    /// Files copied from Surveyor's terrain summaries, over every dimension.
    pub surveyor_files: u64,
    /// Copies taken (more than one when the world saved during the copy).
    pub attempts: u32,
    /// Notes for the operator (e.g. pages ahead of the journal after a crash).
    pub notes: Vec<String>,
}

impl ForkInfo {
    pub fn read(dir: &Path) -> Result<ForkInfo, String> {
        let path = dir.join(MARKER);
        let text = fs::read_to_string(&path).map_err(|e| {
            format!(
                "{} is not a complete fork (no {MARKER}: {e})",
                dir.display()
            )
        })?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn write(&self, dir: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).expect("serialize");
        save::write_atomic(&dir.join(MARKER), text.as_bytes())
    }
}

/// Whether `dir` holds a complete fork.
pub fn is_fork(dir: &Path) -> bool {
    dir.join(MARKER).is_file()
}

fn absolute(p: &Path) -> Result<PathBuf, String> {
    // The folder may not exist yet: canonicalize the nearest ancestor that does.
    let mut existing = p.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(n), Some(parent)) => {
                rest.push(n.to_os_string());
                existing = if parent.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    parent.to_path_buf()
                };
            }
            _ => return Err(format!("{}: no such folder", p.display())),
        }
    }
    let mut out = existing
        .canonicalize()
        .map_err(|e| format!("{}: {e}", existing.display()))?;
    for n in rest.into_iter().rev() {
        out.push(n);
    }
    Ok(out)
}

/// Refuses a fork folder that overlaps the world: the host never writes into a world.
pub fn check_apart(world: &Path, dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let w = absolute(world)?;
    let d = absolute(dir)?;
    if d.starts_with(&w) || w.starts_with(&d) {
        return Err(format!(
            "the fork {} and the world {} overlap: a fork goes in a folder of its own",
            d.display(),
            w.display()
        ));
    }
    Ok((w, d))
}

/// Copies the world in `world` into a new fork in `dir` (which must not exist or be empty).
pub fn export(world: &Path, dir: &Path) -> Result<ForkInfo, String> {
    export_with(world, dir, |_| {})
}

/// Where [`export_with`]'s hook runs, with the copy's attempt (from 1).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidCopy {
    /// After a copy of the pages, before the journal is read again.
    Pages(u32),
    /// After a first read of the archive's index, before the chunk files it names are copied.
    Archive(u32),
}

/// `export`, with `mid_copy` called in the two races a live server makes by chance ([`MidCopy`]):
/// tests save or compact the world there to force them.
#[doc(hidden)]
pub fn export_with(
    world: &Path,
    dir: &Path,
    mut mid_copy: impl FnMut(MidCopy),
) -> Result<ForkInfo, String> {
    let (world_abs, _) = check_apart(world, dir)?;
    let pages = world.join(save::PAGES);
    if !pages.is_dir() {
        return Err(format!(
            "{} has no rtsim pages ({}): the world must have run rtsim at least once",
            world.display(),
            pages.display()
        ));
    }
    if dir.exists() {
        let mut entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        if dir.join(save::PAGES).exists() && !is_fork(dir) {
            return Err(format!(
                "{} holds an export that did not finish (pages but no {MARKER}): delete the folder and export again",
                dir.display()
            ));
        }
        if entries.next().is_some() {
            return Err(format!(
                "{} is not empty: export writes a new fork into an empty folder",
                dir.display()
            ));
        }
    }
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let partial = dir.join(PARTIAL);
    let mut why = Vec::new();
    for attempt in 1..=ATTEMPTS {
        if attempt > 1 {
            std::thread::sleep(Duration::from_millis(50 << attempt.min(6)));
        }
        let _ = fs::remove_dir_all(&partial);
        match copy_once(world, &pages, &partial, || {
            mid_copy(MidCopy::Pages(attempt))
        }) {
            Ok(Copied::Done(c)) => {
                let done = finish(world, &world_abs, dir, &partial, c, attempt, &mut mid_copy);
                if done.is_err() {
                    let _ = fs::remove_dir_all(&partial);
                }
                return done;
            }
            Ok(Copied::Again(reason)) => why.push(reason),
            Err(Retry::Moved(e)) => why.push(format!("the store moved under the copy ({e})")),
            Err(Retry::Fatal(e)) => {
                let _ = fs::remove_dir_all(&partial);
                return Err(e);
            }
        }
    }
    let _ = fs::remove_dir_all(&partial);
    Err(format!(
        "{} kept saving during {ATTEMPTS} copies; pause its saves (save-off) or try again: {}",
        world.display(),
        why.join("; ")
    ))
}

struct Taken {
    generation: rtsim_kernel::page::Generation,
    pages: u64,
    bytes: u64,
    journal: Option<Vec<u8>>,
    parsed: Journal,
    /// `level.dat` as read once after the copy: the bytes the fork gets and its info describes.
    level: Option<Vec<u8>>,
    /// Observations neither the pages nor the journal hold.
    lost: Option<(u64, u64)>,
}

enum Copied {
    Done(Taken),
    Again(String),
}

enum Retry {
    /// A file vanished or a record tore: a writer is compacting. Copy again.
    Moved(String),
    Fatal(String),
}

fn read_opt(path: &Path) -> Result<Option<Vec<u8>>, Retry> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Retry::Fatal(format!("{}: {e}", path.display()))),
    }
}

fn page_err(e: PageError) -> Retry {
    match &e {
        PageError::Busy(_) => Retry::Moved(e.to_string()),
        PageError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            Retry::Moved(e.to_string())
        }
        PageError::Crc { .. } | PageError::Format(_) => Retry::Moved(e.to_string()),
        _ => Retry::Fatal(format!("the world's pages: {e}")),
    }
}

/// `level.dat`, or `level.dat_old` while the server is between the two renames of its save
/// (vanilla's `Util.safeReplaceFile` moves the old file aside before the new one goes in, and
/// falls back to `level.dat_old` itself when `level.dat` is missing).
fn read_level(world: &Path) -> Result<Option<Vec<u8>>, Retry> {
    match read_opt(&world.join(save::LEVEL))? {
        Some(b) => Ok(Some(b)),
        None => read_opt(&world.join(format!("{}_old", save::LEVEL))),
    }
}

/// One copy of the confirmed generation into `partial`, and the journal read before and after.
/// `level.dat` is read once, after: it only gives the game time and the Deep Time folder, and a
/// server writes it at every save, so a level.dat a save newer than the pages is what a crash
/// leaves too (the engine starts from its pages either way).
fn copy_once(
    world: &Path,
    pages: &Path,
    partial: &Path,
    mid_copy: impl FnOnce(),
) -> Result<Copied, Retry> {
    let journal_path = world.join(save::JOURNAL);
    let before = read_opt(&journal_path)?;
    let mut src = PageStore::open_read_only(pages).map_err(page_err)?;
    src.view_confirmed().map_err(page_err)?;
    let generation = src.confirmed();
    let keys = src.try_keys().map_err(page_err)?;
    let mut dst = PageStore::open(partial, Durability::Fsync)
        .map_err(|e| Retry::Fatal(format!("{}: {e}", partial.display())))?;
    let mut batch = SaveBatch::new();
    let (mut n, mut bytes) = (0u64, 0u64);
    for k in keys {
        let Some(b) = src.get_shared(k).map_err(page_err)? else {
            return Err(Retry::Moved(format!(
                "page {:#x} is listed but absent",
                k.0
            )));
        };
        n += 1;
        bytes += b.len() as u64;
        batch.put(k, &b);
        if batch.raw_bytes() >= COMMIT_BYTES {
            dst.commit(std::mem::take(&mut batch), generation.obs_seq)
                .map_err(|e| Retry::Fatal(format!("{}: {e}", partial.display())))?;
        }
    }
    dst.commit(batch, generation.obs_seq)
        .map_err(|e| Retry::Fatal(format!("{}: {e}", partial.display())))?;
    drop(dst);
    mid_copy();
    let after = read_opt(&journal_path)?;
    if before != after {
        return Ok(Copied::Again(
            "the journal was written during the copy".into(),
        ));
    }
    let level = read_level(world)?;
    let parsed = match &after {
        Some(b) => Journal::parse(b)
            .map_err(|e| Retry::Fatal(format!("{}: {e}", journal_path.display())))?,
        None => Journal::fresh(),
    };
    // A journal trimmed past the pages: with the journal the same before and after the copy,
    // the pages read in between are at least the generation that trim followed, so the gap is
    // the world's own (its server would find the same observations lost at its next start, and
    // the engine logs it then), not a save racing the copy.
    let lost = (parsed.first_obs() > generation.obs_seq + 1)
        .then(|| (generation.obs_seq + 1, parsed.first_obs() - 1));
    Ok(Copied::Done(Taken {
        generation,
        pages: n,
        bytes,
        journal: after,
        parsed,
        level,
        lost,
    }))
}

fn copy_file(from: &Path, to: &Path) -> Result<bool, String> {
    match fs::read(from) {
        Ok(b) => {
            save::write_atomic(to, &b)?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", from.display())),
    }
}

/// Copies a file with no fsync (the archive and Surveyor's summaries are records the world itself
/// writes unsynced, and the marker is written after them); `false` when `from` is not there.
fn copy_loose(from: &Path, to: &Path) -> Result<bool, String> {
    match fs::copy(from, to) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", from.display())),
    }
}

/// Copies a folder tree file by file; a file that vanishes during the copy is skipped.
fn copy_tree(from: &Path, to: &Path) -> Result<u64, String> {
    let mut n = 0;
    let entries = match fs::read_dir(from) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {e}", from.display())),
    };
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
        let ty = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", entry.path().display()))?;
        let dst = to.join(entry.file_name());
        if ty.is_dir() {
            n += copy_tree(&entry.path(), &dst)?;
        } else if ty.is_file() && copy_loose(&entry.path(), &dst)? {
            n += 1;
        }
    }
    Ok(n)
}

/// What [`copy_archive`] copied: the files, and the copies it took.
#[derive(Debug, PartialEq, Eq)]
struct ArchiveCopy {
    files: u64,
    attempts: u32,
}

fn read_archive_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Copies the archive in `from`, which its server may be flushing and compacting, into `to`, so
/// the copy's index names exactly the chunk files beside it (the module's doc). `mid_copy(attempt)`
/// runs after each first read of the index, before the chunk files are copied.
fn copy_archive(
    from: &Path,
    to: &Path,
    mut mid_copy: impl FnMut(u32),
) -> Result<ArchiveCopy, String> {
    let index_path = from.join(INDEX_FILE);
    let mut why = Vec::new();
    for attempt in 1..=ATTEMPTS {
        if attempt > 1 {
            std::thread::sleep(Duration::from_millis(50 << attempt.min(6)));
        }
        let _ = fs::remove_dir_all(to);
        if !from.is_dir() {
            return Ok(ArchiveCopy {
                files: 0,
                attempts: attempt,
            });
        }
        let read = read_archive_file(&index_path)?;
        let Some((bytes, index)) = read.and_then(|b| Index::decode(&b).ok().map(|i| (b, i))) else {
            // No index to hold the copy to (none written yet, or a damaged one): file by file,
            // and the copy's open rebuilds its index from the chunks' own headers, as the world's
            // own open would.
            let files = copy_tree(from, to)?;
            return Ok(ArchiveCopy {
                files,
                attempts: attempt,
            });
        };
        fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        mid_copy(attempt);
        let mut gone = None;
        for e in &index.entries {
            let name = chunk_name(e.seq);
            if !copy_loose(&from.join(&name), &to.join(&name))? {
                gone = Some(name);
                break;
            }
        }
        if let Some(name) = gone {
            why.push(format!("{name} went during the copy"));
            continue;
        }
        let dict = copy_loose(&from.join(DICT_FILE), &to.join(DICT_FILE))?;
        if read_archive_file(&index_path)?.as_ref() != Some(&bytes) {
            why.push("the index changed during the copy".to_string());
            continue;
        }
        let index_copy = to.join(INDEX_FILE);
        fs::write(&index_copy, &bytes).map_err(|e| format!("{}: {e}", index_copy.display()))?;
        return Ok(ArchiveCopy {
            files: index.entries.len() as u64 + 1 + u64::from(dict),
            attempts: attempt,
        });
    }
    let _ = fs::remove_dir_all(to);
    Err(format!(
        "the archive in {} kept changing during {ATTEMPTS} copies; pause the world's saves (save-off) or try again: {}",
        from.display(),
        why.join("; ")
    ))
}

/// The save serial the copied pages hold (`ARCHIVE_KEY`), read as the engine's start reads it: 0
/// when they hold none.
fn copied_serial(pages: &Path) -> Result<u64, String> {
    let err = |e: PageError| format!("the copied pages {}: {e}", pages.display());
    let mut s = PageStore::open_read_only(pages).map_err(err)?;
    s.view_confirmed().map_err(err)?;
    Ok(match s.get_shared(ARCHIVE_KEY).map_err(err)? {
        Some(b) if b.len() == 8 => u64::from_le_bytes(b[..].try_into().expect("8 bytes")),
        _ => 0,
    })
}

/// Folders under a dimension's folder that never hold another dimension.
const NOT_DIMENSIONS: [&str; 4] = ["region", "entities", "poi", "data"];
/// How deep `dimensions/<namespace>/<path>` is searched (a path may have parts).
const DIMENSION_DEPTH: usize = 6;

/// The Surveyor folders of `world`'s dimensions (`dir_for` in mic-viewer-surveyor), relative.
fn surveyor_dirs(world: &Path) -> Result<Vec<PathBuf>, String> {
    let at = |base: PathBuf| base.join("data").join("surveyor");
    let mut found: Vec<PathBuf> = [PathBuf::new(), "DIM-1".into(), "DIM1".into()]
        .into_iter()
        .map(at)
        .filter(|d| world.join(d).is_dir())
        .collect();
    let mut stack = vec![(PathBuf::from("dimensions"), 0)];
    while let Some((rel, depth)) = stack.pop() {
        let entries = match fs::read_dir(world.join(&rel)) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", world.join(&rel).display())),
        };
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", world.join(&rel).display()))?;
            let name = entry.file_name();
            if !entry.file_type().is_ok_and(|t| t.is_dir())
                || NOT_DIMENSIONS.iter().any(|n| name == *n)
            {
                continue;
            }
            let sub = rel.join(&name);
            if world.join(at(sub.clone())).is_dir() {
                found.push(at(sub.clone()));
            }
            if depth < DIMENSION_DEPTH {
                stack.push((sub, depth + 1));
            }
        }
    }
    found.sort();
    Ok(found)
}

fn finish(
    world: &Path,
    world_abs: &Path,
    dir: &Path,
    partial: &Path,
    c: Taken,
    attempts: u32,
    mid_copy: &mut impl FnMut(MidCopy),
) -> Result<ForkInfo, String> {
    let archive = copy_archive(
        &world.join(save::PAGES).join(ARCHIVE_DIR),
        &partial.join(ARCHIVE_DIR),
        |attempt| mid_copy(MidCopy::Archive(attempt)),
    )?;
    let archive_files = archive.files;
    let mut notes = Vec::new();
    if archive.attempts > 1 {
        notes.push(format!(
            "the archive was copied {} times: the world wrote it during the copy",
            archive.attempts
        ));
    }
    if archive_files > 0 {
        // The world's newest chunks may be from a save it never confirmed, which the fork's pages
        // do not hold: cut them by the copied pages' save serial, as the engine's start does
        // (§10.5), or the host's own later saves would bring them into view.
        let serial = copied_serial(partial)?;
        let mut a = Archive::open(partial.join(ARCHIVE_DIR))
            .map_err(|e| format!("the archive's copy: {e}"))?;
        let cut = a
            .cut_above_serial(serial)
            .map_err(|e| format!("the archive's copy: {e}"))?;
        if cut > 0 {
            notes.push(format!(
                "{cut} archive chunks from saves after the copied generation were left out, as the world's next start would drop them"
            ));
        }
    }
    let mut surveyor_files = 0;
    for d in surveyor_dirs(world)? {
        surveyor_files += copy_tree(&world.join(&d), &dir.join(&d))?;
    }
    let pages_dir = dir.join(save::PAGES);
    fs::rename(partial, &pages_dir).map_err(|e| format!("{}: {e}", pages_dir.display()))?;
    if let Some(b) = &c.journal {
        save::write_atomic(&dir.join(save::JOURNAL), b)?;
    }
    let level = match &c.level {
        Some(b) => {
            save::write_atomic(&dir.join(save::LEVEL), b)?;
            Some(
                crate::nbt::read_gz(b)
                    .map(|(_, t)| t)
                    .map_err(|e| format!("{}: {e}", world.join(save::LEVEL).display()))?,
            )
        }
        None => None,
    };
    copy_file(
        &world.join(save::SERVER_CONFIG),
        &dir.join(save::SERVER_CONFIG),
    )?;
    let folder = save::deep_time_folder(level.as_ref());
    let manifest = Path::new(&folder).join("manifest.json");
    let deep_time_folder =
        copy_file(&world.join(&manifest), &dir.join(&manifest))?.then_some(folder);
    if c.parsed.save.next_obs.saturating_sub(1) < c.generation.obs_seq {
        notes.push(format!(
            "the pages hold observations through {}, the journal only to {}: the world stopped between the engine's save and the journal's (P29); the host starts from the pages, as the game would",
            c.generation.obs_seq,
            c.parsed.save.next_obs.saturating_sub(1)
        ));
    }
    if let Some((from, to)) = c.lost {
        notes.push(format!(
            "the journal starts at observation {}, the pages hold observations through {}: observations {from} to {to} are in neither, as the world itself has it (its server would lose them at its next start too)",
            to + 1,
            from - 1
        ));
    }
    let info = ForkInfo {
        source: world_abs.display().to_string(),
        exported_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        pages_save_seq: c.generation.save_seq,
        pages_obs_seq: c.generation.obs_seq,
        pages: c.pages,
        page_bytes: c.bytes,
        journal_first_obs: c.parsed.first_obs(),
        journal_next_obs: c.parsed.save.next_obs,
        game_time: level.as_ref().and_then(save::level_time),
        deep_time_folder,
        archive_files,
        surveyor_files,
        attempts,
        notes,
    };
    info.write(dir)?;
    Ok(info)
}
