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
//! <dir>/rtsim/archive/                the world's archive (§10.3), when it has one
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

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
        match copy_once(world, &pages, &partial) {
            Ok(Copied::Done(c)) => return finish(world, &world_abs, dir, &partial, c, attempt),
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

/// One copy of the confirmed generation into `partial`, and the journal read before and after.
fn copy_once(world: &Path, pages: &Path, partial: &Path) -> Result<Copied, Retry> {
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
    let after = read_opt(&journal_path)?;
    if before != after {
        return Ok(Copied::Again(
            "the journal was written during the copy".into(),
        ));
    }
    let parsed = match &after {
        Some(b) => Journal::parse(b)
            .map_err(|e| Retry::Fatal(format!("{}: {e}", journal_path.display())))?,
        None => Journal::fresh(),
    };
    // The journal must hold every observation after the pages' (else a later save's trim
    // already dropped some the pages never had).
    if parsed.first_obs() > generation.obs_seq + 1 {
        return Ok(Copied::Again(format!(
            "the journal starts at observation {}, past the pages' {}",
            parsed.first_obs(),
            generation.obs_seq
        )));
    }
    Ok(Copied::Done(Taken {
        generation,
        pages: n,
        bytes,
        journal: after,
        parsed,
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
        } else if ty.is_file() && copy_file(&entry.path(), &dst)? {
            n += 1;
        }
    }
    Ok(n)
}

fn finish(
    world: &Path,
    world_abs: &Path,
    dir: &Path,
    partial: &Path,
    c: Taken,
    attempts: u32,
) -> Result<ForkInfo, String> {
    let archive_files = copy_tree(
        &world.join(save::PAGES).join("archive"),
        &partial.join("archive"),
    )?;
    let pages_dir = dir.join(save::PAGES);
    fs::rename(partial, &pages_dir).map_err(|e| format!("{}: {e}", pages_dir.display()))?;
    if let Some(b) = &c.journal {
        save::write_atomic(&dir.join(save::JOURNAL), b)?;
    }
    copy_file(&world.join(save::LEVEL), &dir.join(save::LEVEL))?;
    copy_file(
        &world.join(save::SERVER_CONFIG),
        &dir.join(save::SERVER_CONFIG),
    )?;
    let level = save::level(world)?;
    let folder = save::deep_time_folder(level.as_ref());
    let manifest = Path::new(&folder).join("manifest.json");
    let deep_time_folder =
        copy_file(&world.join(&manifest), &dir.join(&manifest))?.then_some(folder);
    let mut notes = Vec::new();
    if c.parsed.save.next_obs.saturating_sub(1) < c.generation.obs_seq {
        notes.push(format!(
            "the pages hold observations through {}, the journal only to {}: the world stopped between the engine's save and the journal's (P29); the host starts from the pages, as the game would",
            c.generation.obs_seq,
            c.parsed.save.next_obs.saturating_sub(1)
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
        attempts,
        notes,
    };
    info.write(dir)?;
    Ok(info)
}
