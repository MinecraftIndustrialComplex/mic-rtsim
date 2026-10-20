// SPDX-License-Identifier: AGPL-3.0-or-later
//! The incremental page store (SCALE.md §8, PLAN.md §4.9): pages are LZ4 with a CRC32, saves write
//! only dirty pages, and the index of where each page lives is itself written incrementally.
//!
//! # Layout of `<world>/rtsim/`
//!
//! - `seg-<n>.rts`: segments. Each save writes one, holding every page it wrote, back to back. A
//!   segment is written once and never modified.
//! - `manifest.rtm`: a **checkpoint** of the index: the generations, the segments, and for every
//!   live page the segment, offset, lengths and CRC32 where it lives.
//! - `mlog-<checkpoint>.rtl`: the **log** since that checkpoint, one CRC-framed record per save
//!   (the pages it changed), per confirmation and per rollback.
//!
//! A save appends one record whose size is the number of pages it changed, so its cost follows
//! the dirty pages, not the world. When the log has grown as big as the checkpoint (and at least
//! [`LOG_MIN_BYTES`]), the next save folds it into a new checkpoint: each checkpoint costs the
//! whole index but comes after as many logged bytes, so the cost per save stays proportional to
//! what it changed, amortized. (DECISIONS.md O9 recorded the old whole-manifest rewrite as a known
//! limit; this replaces it.)
//!
//! # Two generations (P29)
//!
//! [`PageStore::commit_pending`] writes a save but keeps the pages of the last *confirmed*
//! generation until [`PageStore::confirm`] says Minecraft's own save completed. If the process dies
//! between the two, [`PageStore::open_for_journal`] opens whichever generation is not ahead of the
//! journal in Minecraft's save: the new one if Minecraft's save got the journal that far, the
//! confirmed one otherwise ([`PageStore::rollback`]). [`PageStore::commit`] is a save that is
//! confirmed at once, as before.
//!
//! # Crash safety, in Deep Time's order (deeptime-format `fsutil.rs`)
//!
//! - A segment is written to `<name>.tmp`, fsynced, renamed, and its folder fsynced, before any
//!   record names it.
//! - A record is appended to the log and the log fsynced; that append is the moment the save
//!   happens. A crash inside the append leaves a torn last record, which the next open cuts off
//!   (the save did not happen). A record that fails its CRC with more records after it is
//!   corruption, not a torn write, and the open stops with an error.
//! - Superseded segments are deleted only after the record that stops naming them is durable.
//! - A checkpoint is written like a segment (tmp, fsync, rename, fsync folder), and only then is
//!   the log it replaces deleted.
//! - On open, files the index does not name (a crashed save's segment, an old log, `.tmp` files)
//!   are removed.
//!
//! **Why segments rather than a file per page:** an incremental save cannot overwrite a page file
//! the current index still names, and one fsync per page would make a save of a thousand dirty
//! regions a thousand fsyncs. A save costs two fsyncs (segment, log) whatever the page count.
//! Segments whose current pages fall under half of their bytes are compacted: those pages are
//! copied, still compressed and CRC-checked, into the next save's segment.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::codec::{Reader, Writer};

const SEGMENT_MAGIC: [u8; 4] = *b"RTSG";
const MANIFEST_MAGIC: [u8; 4] = *b"RTMF";
const LOG_MAGIC: [u8; 4] = *b"RTML";
/// Bumped on any layout change. Before release a change wipes the save; there is no migration
/// (PLAN.md §4.9).
const FORMAT_VERSION: u16 = 2;
const SEGMENT_HEADER: u64 = 16;
const LOG_HEADER: u64 = 16;
/// key u64, raw length u32, stored length u32, CRC32 u32.
const RECORD_HEADER: u64 = 20;
const MANIFEST_NAME: &str = "manifest.rtm";
/// A segment under this size counts as small. PLACEHOLDER tunable.
const SMALL_SEGMENT_BYTES: u64 = 1 << 20;
/// More small segments than this, and the next save merges them all. PLACEHOLDER tunable. Bounds
/// the file count when long-lived pages each sit in the small segment of the save that wrote them;
/// each merge makes one bigger segment, so a page is copied about log(saves) times, as in an LSM
/// tree's tiering.
const MAX_SMALL_SEGMENTS: usize = 8;
/// The log is folded into a new checkpoint once it holds this many bytes and at least as many as
/// the checkpoint. PLACEHOLDER tunable.
pub const LOG_MIN_BYTES: u64 = 256 << 10;

/// A page's identity. The engine packs (table, region) into it; the store only orders by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageKey(pub u64);

impl PageKey {
    /// Table in the top 16 bits, id in the low 48.
    pub const fn new(table: u16, id: u64) -> PageKey {
        debug_assert!(id < 1 << 48);
        PageKey((table as u64) << 48 | id)
    }

    pub const fn table(self) -> u16 {
        (self.0 >> 48) as u16
    }

    pub const fn id(self) -> u64 {
        self.0 & ((1 << 48) - 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// fsync every file and folder, as a real save must.
    Fsync,
    /// Skip fsync: tests and benches that measure bytes, not durability.
    NoSync,
}

#[derive(Debug)]
pub enum PageError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Crc {
        what: String,
        expected: u32,
        actual: u32,
    },
    Format(String),
    /// A commit failed after the in-memory index changed. Reopen the store.
    Poisoned,
    /// A test-injected crash.
    Injected,
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PageError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            PageError::Crc {
                what,
                expected,
                actual,
            } => write!(
                f,
                "CRC32 mismatch in {what}: expected {expected:#010x}, found {actual:#010x}"
            ),
            PageError::Format(s) => write!(f, "page store format: {s}"),
            PageError::Poisoned => {
                write!(f, "a failed commit left the page store unusable; reopen it")
            }
            PageError::Injected => write!(f, "injected fault"),
        }
    }
}

impl std::error::Error for PageError {}

pub type Result<T> = std::result::Result<T, PageError>;

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> PageError + '_ {
    move |source| PageError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// A page compressed and checksummed, ready to write. Encoding is pure, so the host's IO lane can
/// do it off the step thread (PLAN.md §4.9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedPage {
    raw_len: u32,
    crc: u32,
    stored: Vec<u8>,
}

impl EncodedPage {
    pub fn encode(raw: &[u8]) -> EncodedPage {
        let raw_len = u32::try_from(raw.len()).expect("a page over 4 GiB");
        let stored = lz4_flex::block::compress(raw);
        EncodedPage {
            raw_len,
            crc: crc32fast::hash(&stored),
            stored,
        }
    }

    pub fn stored_len(&self) -> usize {
        self.stored.len()
    }

    pub fn raw_len(&self) -> usize {
        self.raw_len as usize
    }

    pub fn decode(&self) -> Result<Vec<u8>> {
        decode_stored(&self.stored, self.raw_len, self.crc, "page")
    }
}

fn decode_stored(stored: &[u8], raw_len: u32, crc: u32, what: &str) -> Result<Vec<u8>> {
    let actual = crc32fast::hash(stored);
    if actual != crc {
        return Err(PageError::Crc {
            what: what.to_string(),
            expected: crc,
            actual,
        });
    }
    lz4_flex::block::decompress(stored, raw_len as usize)
        .map_err(|e| PageError::Format(format!("{what}: LZ4: {e}")))
}

/// A page's place on disk, from [`PageStore::locate`]. Two locations are equal when they name the
/// same bytes, so a reader can tell whether what it read is still the page's current version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageLocation {
    path: PathBuf,
    segment: u64,
    offset: u64,
    raw_len: u32,
    stored_len: u32,
    crc: u32,
}

impl PageLocation {
    /// Reads, CRC-checks and decompresses the page, opening the segment itself.
    pub fn read(&self) -> Result<Vec<u8>> {
        let mut f = File::open(&self.path).map_err(io(&self.path))?;
        f.seek(SeekFrom::Start(self.offset))
            .map_err(io(&self.path))?;
        let mut buf = vec![0; self.stored_len as usize];
        f.read_exact(&mut buf).map_err(io(&self.path))?;
        decode_stored(&buf, self.raw_len, self.crc, "page")
    }

    pub fn stored_len(&self) -> usize {
        self.stored_len as usize
    }
}

/// The pages one save writes or deletes. Later puts of a key replace earlier ones.
#[derive(Debug, Default)]
pub struct SaveBatch {
    changes: BTreeMap<PageKey, Option<EncodedPage>>,
}

impl SaveBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, key: PageKey, raw: &[u8]) {
        self.changes.insert(key, Some(EncodedPage::encode(raw)));
    }

    pub fn put_encoded(&mut self, key: PageKey, page: EncodedPage) {
        self.changes.insert(key, Some(page));
    }

    pub fn delete(&mut self, key: PageKey) {
        self.changes.insert(key, None);
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Where a page lives. 24 bytes, so the index costs 32 bytes per page with its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageLoc {
    /// Offset of the stored bytes in the segment file.
    offset: u64,
    segment: u32,
    raw_len: u32,
    stored_len: u32,
    crc: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SegmentInfo {
    file_bytes: u64,
    /// Pages of the current generation in this segment, and their record bytes.
    cur_pages: u64,
    cur_bytes: u64,
    /// Every page that needs this segment: the current generation's, and those only the
    /// confirmed generation still needs (P29). At zero the segment is dead.
    refs: u64,
}

/// What a commit did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitStats {
    pub segment_bytes: u64,
    /// Index bytes this save wrote: its log record plus a checkpoint if it wrote one.
    pub manifest_bytes: u64,
    pub pages_written: u64,
    pub pages_deleted: u64,
    pub pages_compacted: u64,
    pub segments_removed: u64,
    /// The log record's bytes.
    pub log_bytes: u64,
    /// A checkpoint's bytes, when this save folded the log into one; else 0.
    pub checkpoint_bytes: u64,
}

/// Where a commit stops, for crash tests. Each is a point of the save order (module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The segment is durable; its log record was never written.
    BeforeManifest,
    /// The log record is durable; superseded segments were never deleted.
    BeforeCleanup,
    /// Half the segment's temporary file is written; nothing is renamed.
    MidSegment,
    /// Half the log record is written and synced: a torn record.
    TornLog,
    /// A checkpoint's temporary file is written but not renamed.
    MidCheckpoint,
    /// The checkpoint is in place; the log it replaced was never deleted.
    AfterCheckpoint,
}

/// One save generation: its number and the observation sequence its pages include (A9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generation {
    pub save_seq: u64,
    pub obs_seq: u64,
}

/// One log record.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rec {
    Commit {
        save_seq: u64,
        obs_seq: u64,
        /// Keep the confirmed generation (P29) or confirm this one at once.
        pending: bool,
        /// The segment this save wrote, with its size.
        segment: Option<(u32, u64)>,
        /// In key order.
        changes: Vec<(PageKey, Option<PageLoc>)>,
    },
    Confirm {
        save_seq: u64,
    },
    Rollback,
}

const REC_COMMIT: u8 = 1;
const REC_CONFIRM: u8 = 2;
const REC_ROLLBACK: u8 = 3;

fn put_loc(w: &mut Writer, l: &PageLoc) {
    w.var_u64(u64::from(l.segment));
    w.var_u64(l.offset);
    w.var_u64(u64::from(l.raw_len));
    w.var_u64(u64::from(l.stored_len));
    w.u32(l.crc);
}

fn get_u32(r: &mut Reader<'_>) -> crate::codec::Result<u32> {
    let v = r.var_u64()?;
    u32::try_from(v).map_err(|_| crate::codec::CodecError::Invalid(format!("{v} over u32")))
}

fn get_loc(r: &mut Reader<'_>) -> crate::codec::Result<PageLoc> {
    Ok(PageLoc {
        segment: get_u32(r)?,
        offset: r.var_u64()?,
        raw_len: get_u32(r)?,
        stored_len: get_u32(r)?,
        crc: r.u32()?,
    })
}

impl Rec {
    fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(64);
        match self {
            Rec::Commit {
                save_seq,
                obs_seq,
                pending,
                segment,
                changes,
            } => {
                w.u8(REC_COMMIT);
                w.var_u64(*save_seq);
                w.var_u64(*obs_seq);
                w.u8(u8::from(*pending));
                let (seg, bytes) = segment.map_or((0, 0), |(s, b)| (u64::from(s) + 1, b));
                w.var_u64(seg);
                w.var_u64(bytes);
                w.var_u64(changes.len() as u64);
                let mut prev = 0u64;
                for (k, l) in changes {
                    w.var_u64(k.0 - prev);
                    prev = k.0;
                    match l {
                        Some(l) => {
                            w.u8(1);
                            put_loc(&mut w, l);
                        }
                        None => w.u8(0),
                    }
                }
            }
            Rec::Confirm { save_seq } => {
                w.u8(REC_CONFIRM);
                w.var_u64(*save_seq);
            }
            Rec::Rollback => w.u8(REC_ROLLBACK),
        }
        w.into_bytes()
    }

    fn decode(b: &[u8]) -> crate::codec::Result<Rec> {
        let mut r = Reader::new(b);
        let rec = match r.u8()? {
            REC_COMMIT => {
                let save_seq = r.var_u64()?;
                let obs_seq = r.var_u64()?;
                let pending = r.u8()? != 0;
                let seg = r.var_u64()?;
                let bytes = r.var_u64()?;
                let segment = match seg {
                    0 => None,
                    s => Some((
                        u32::try_from(s - 1).map_err(|_| {
                            crate::codec::CodecError::Invalid(format!("segment {s}"))
                        })?,
                        bytes,
                    )),
                };
                let n = r.var_u64()?;
                let mut changes = Vec::with_capacity(n.min(1 << 20) as usize);
                let mut key = 0u64;
                for _ in 0..n {
                    key += r.var_u64()?;
                    let l = match r.u8()? {
                        0 => None,
                        _ => Some(get_loc(&mut r)?),
                    };
                    changes.push((PageKey(key), l));
                }
                Rec::Commit {
                    save_seq,
                    obs_seq,
                    pending,
                    segment,
                    changes,
                }
            }
            REC_CONFIRM => Rec::Confirm {
                save_seq: r.var_u64()?,
            },
            REC_ROLLBACK => Rec::Rollback,
            k => {
                return Err(crate::codec::CodecError::Invalid(format!(
                    "log record kind {k}"
                )));
            }
        };
        r.finish()?;
        Ok(rec)
    }
}

pub struct PageStore {
    dir: PathBuf,
    durability: Durability,
    /// The current generation.
    save_seq: u64,
    obs_seq: u64,
    /// The last generation Minecraft's save confirmed; equal to the current one unless a pending
    /// save waits for confirmation.
    confirmed: Generation,
    /// The highest save number ever used: a rolled-back number is never reused.
    last_seq: u64,
    next_segment: u32,
    pages: HashMap<PageKey, PageLoc>,
    /// For each page changed since the confirmed generation: where it lived then (`None`: it did
    /// not exist). Empty when nothing is pending.
    prev: HashMap<PageKey, Option<PageLoc>>,
    segments: BTreeMap<u32, SegmentInfo>,
    /// The checkpoint's number (0: none yet) and size; the log file is named after it.
    ckpt_id: u64,
    ckpt_bytes: u64,
    /// Bytes in the log file, header included; 0 when there is no log file.
    log_bytes: u64,
    log: Option<File>,
    files: HashMap<u32, File>,
    poisoned: bool,
    fault: Option<Fault>,
}

impl PageStore {
    /// Opens (or creates) the store in `dir` at its current generation, removing whatever a
    /// crashed save left behind.
    pub fn open(dir: impl Into<PathBuf>, durability: Durability) -> Result<PageStore> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(io(&dir))?;
        let mut store = PageStore {
            dir,
            durability,
            save_seq: 0,
            obs_seq: 0,
            confirmed: Generation::default(),
            last_seq: 0,
            next_segment: 1,
            pages: HashMap::new(),
            prev: HashMap::new(),
            segments: BTreeMap::new(),
            ckpt_id: 0,
            ckpt_bytes: 0,
            log_bytes: 0,
            log: None,
            files: HashMap::new(),
            poisoned: false,
            fault: None,
        };
        // Only a checkpoint that is certainly absent means "start from empty". Any other failure
        // to read it must stop here: treating it as absent would make every segment a stray.
        let manifest = store.dir.join(MANIFEST_NAME);
        match fs::read(&manifest) {
            Ok(bytes) => store.parse_checkpoint(&manifest, &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(PageError::Io {
                    path: manifest,
                    source: e,
                });
            }
        }
        store.replay_log()?;
        store.remove_strays()?;
        for (&seq, info) in &store.segments {
            let path = store.segment_path(seq);
            let len = fs::metadata(&path).map_err(io(&path))?.len();
            if len != info.file_bytes {
                return Err(PageError::Format(format!(
                    "{}: {len} bytes, the index says {}",
                    path.display(),
                    info.file_bytes
                )));
            }
        }
        Ok(store)
    }

    /// Opens the store at the newest generation that is not ahead of Minecraft's journal (P29):
    /// `world_obs_seq` is the last observation sequence the world's save holds. If the current
    /// generation includes observations past it, Minecraft's save never completed after this
    /// save, so the store rolls back to the confirmed generation.
    pub fn open_for_journal(
        dir: impl Into<PathBuf>,
        durability: Durability,
        world_obs_seq: u64,
    ) -> Result<PageStore> {
        let mut s = PageStore::open(dir, durability)?;
        if s.obs_seq <= world_obs_seq {
            return Ok(s);
        }
        if s.confirmed.obs_seq <= world_obs_seq {
            s.rollback()?;
            return Ok(s);
        }
        Err(PageError::Format(format!(
            "both generations are ahead of the journal: current includes observations to {}, \
             confirmed to {}, the world's save to {world_obs_seq}",
            s.obs_seq, s.confirmed.obs_seq
        )))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The number of the current generation's save; 0 before the first.
    pub fn save_seq(&self) -> u64 {
        self.save_seq
    }

    /// The observation sequence the current generation includes (A9).
    pub fn obs_seq(&self) -> u64 {
        self.obs_seq
    }

    /// The current generation.
    pub fn current(&self) -> Generation {
        Generation {
            save_seq: self.save_seq,
            obs_seq: self.obs_seq,
        }
    }

    /// The last confirmed generation (equal to [`PageStore::current`] when nothing is pending).
    pub fn confirmed(&self) -> Generation {
        self.confirmed
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn contains(&self, key: PageKey) -> bool {
        self.pages.contains_key(&key)
    }

    pub fn keys(&self) -> impl Iterator<Item = PageKey> + '_ {
        self.pages.keys().copied()
    }

    /// Bytes on disk: every live segment, the checkpoint and the log.
    pub fn disk_bytes(&self) -> u64 {
        self.segments.values().map(|s| s.file_bytes).sum::<u64>() + self.ckpt_bytes + self.log_bytes
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Bytes the in-memory index holds: it is resident always (SCALE.md §8), one entry per page.
    pub fn heap_bytes(&self) -> usize {
        let entry = std::mem::size_of::<(PageKey, PageLoc)>() + 1;
        self.pages.capacity() * entry
            + self.prev.capacity() * (std::mem::size_of::<(PageKey, Option<PageLoc>)>() + 1)
            + self.segments.len() * (std::mem::size_of::<(u32, SegmentInfo)>() + 16)
    }

    /// Bytes in the log since the last checkpoint, and the checkpoint's size.
    pub fn index_file_bytes(&self) -> (u64, u64) {
        (self.log_bytes, self.ckpt_bytes)
    }

    #[doc(hidden)]
    pub fn inject_fault(&mut self, fault: Option<Fault>) {
        self.fault = fault;
    }

    /// Where the page lives on disk now, for a read on another thread (the host's IO lane):
    /// [`PageLocation::read`] needs no access to the store. A location stays readable until a
    /// later commit drops its segment; a read that loses that race fails and is simply retried
    /// through [`PageStore::get`].
    pub fn locate(&self, key: PageKey) -> Option<PageLocation> {
        if self.poisoned {
            return None;
        }
        let loc = *self.pages.get(&key)?;
        Some(PageLocation {
            path: self.segment_path(loc.segment),
            segment: u64::from(loc.segment),
            offset: loc.offset,
            raw_len: loc.raw_len,
            stored_len: loc.stored_len,
            crc: loc.crc,
        })
    }

    /// Switches fsync on or off for later commits (a bulk build can skip it, then turn it on).
    pub fn set_durability(&mut self, durability: Durability) {
        self.durability = durability;
    }

    fn segment_path(&self, seq: u32) -> PathBuf {
        self.dir.join(format!("seg-{seq:016x}.rts"))
    }

    fn log_path(&self, id: u64) -> PathBuf {
        self.dir.join(format!("mlog-{id:016x}.rtl"))
    }

    fn file(&mut self, seq: u32) -> Result<&mut File> {
        if !self.files.contains_key(&seq) {
            let path = self.segment_path(seq);
            let f = File::open(&path).map_err(io(&path))?;
            self.files.insert(seq, f);
        }
        Ok(self.files.get_mut(&seq).expect("inserted above"))
    }

    fn read_stored(&mut self, loc: PageLoc) -> Result<Vec<u8>> {
        let path = self.segment_path(loc.segment);
        let f = self.file(loc.segment)?;
        f.seek(SeekFrom::Start(loc.offset)).map_err(io(&path))?;
        let mut buf = vec![0; loc.stored_len as usize];
        f.read_exact(&mut buf).map_err(io(&path))?;
        Ok(buf)
    }

    /// The page's bytes, CRC-checked and decompressed, or `None` if the store has no such page.
    pub fn get(&mut self, key: PageKey) -> Result<Option<Vec<u8>>> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        let Some(&loc) = self.pages.get(&key) else {
            return Ok(None);
        };
        let stored = self.read_stored(loc)?;
        decode_stored(&stored, loc.raw_len, loc.crc, &format!("page {key:?}")).map(Some)
    }

    /// Makes `batch` durable as one save that includes observations up to `obs_seq`, confirmed at
    /// once: no previous generation is kept.
    pub fn commit(&mut self, batch: SaveBatch, obs_seq: u64) -> Result<CommitStats> {
        self.commit_inner(batch, obs_seq, false)
    }

    /// Makes `batch` durable as a new generation, keeping the confirmed generation's pages until
    /// [`PageStore::confirm`] (P29). Reads see the new generation at once.
    pub fn commit_pending(&mut self, batch: SaveBatch, obs_seq: u64) -> Result<CommitStats> {
        self.commit_inner(batch, obs_seq, true)
    }

    /// Minecraft's save that goes with generation `save_seq` completed: that generation becomes
    /// the confirmed one and the previous generation's superseded pages are dropped. Returns
    /// false (and does nothing) unless `save_seq` is the current, pending generation.
    pub fn confirm(&mut self, save_seq: u64) -> Result<bool> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        if save_seq != self.save_seq || self.confirmed == self.current() {
            return Ok(false);
        }
        self.log_and_apply(Rec::Confirm { save_seq }).map(|_| true)
    }

    /// Drops every pending generation and returns to the confirmed one. Returns false when
    /// nothing was pending.
    pub fn rollback(&mut self) -> Result<bool> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        if self.confirmed == self.current() {
            return Ok(false);
        }
        self.log_and_apply(Rec::Rollback).map(|_| true)
    }

    /// Folds the log into a new checkpoint now (a clean shutdown can call it so the next open
    /// replays nothing). Returns the checkpoint's bytes.
    pub fn checkpoint_now(&mut self) -> Result<u64> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        self.poisoned = true;
        let n = self.checkpoint()?;
        self.poisoned = false;
        Ok(n)
    }

    fn log_and_apply(&mut self, rec: Rec) -> Result<CommitStats> {
        let mut stats = CommitStats::default();
        self.poisoned = true;
        stats.log_bytes = self.append(&rec.encode())?;
        let dead = self.apply(&rec);
        if self.fault == Some(Fault::BeforeCleanup) {
            return Err(PageError::Injected);
        }
        stats.segments_removed = self.delete_segments(dead)?;
        if self.checkpoint_due() {
            stats.checkpoint_bytes = self.checkpoint()?;
        }
        stats.manifest_bytes = stats.log_bytes + stats.checkpoint_bytes;
        self.poisoned = false;
        Ok(stats)
    }

    fn commit_inner(
        &mut self,
        batch: SaveBatch,
        obs_seq: u64,
        pending: bool,
    ) -> Result<CommitStats> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        let save_seq = self.last_seq + 1;
        let seg_seq = self.next_segment;
        let mut stats = CommitStats::default();

        // Compaction candidates: segments whose current pages fill under half of them, and every
        // small segment once there are too many. Their current pages are found from the segments'
        // own record headers, so nothing here walks the whole index.
        let small = self
            .segments
            .values()
            .filter(|s| s.file_bytes < SMALL_SEGMENT_BYTES)
            .count();
        let merge_small = small > MAX_SMALL_SEGMENTS;
        let compact: Vec<u32> = self
            .segments
            .iter()
            .filter(|(_, s)| {
                s.cur_pages > 0
                    && (s.cur_bytes * 2 < s.file_bytes - SEGMENT_HEADER
                        || (merge_small && s.file_bytes < SMALL_SEGMENT_BYTES))
            })
            .map(|(&k, _)| k)
            .collect();

        // Build the segment in memory: puts in key order, then compacted pages in key order.
        let mut seg = Writer::with_capacity(4096);
        seg.raw(&SEGMENT_MAGIC);
        seg.u16(FORMAT_VERSION);
        seg.u16(0);
        seg.u64(u64::from(seg_seq));
        let mut changes: BTreeMap<PageKey, Option<PageLoc>> = BTreeMap::new();
        let push = |seg: &mut Writer, key: PageKey, raw_len: u32, crc: u32, stored: &[u8]| {
            seg.u64(key.0);
            seg.u32(raw_len);
            seg.u32(stored.len() as u32);
            seg.u32(crc);
            let offset = seg.len() as u64;
            seg.raw(stored);
            PageLoc {
                segment: seg_seq,
                offset,
                raw_len,
                stored_len: stored.len() as u32,
                crc,
            }
        };
        for (key, change) in &batch.changes {
            match change {
                Some(p) => {
                    let loc = push(&mut seg, *key, p.raw_len, p.crc, &p.stored);
                    changes.insert(*key, Some(loc));
                    stats.pages_written += 1;
                }
                None => {
                    if self.pages.contains_key(key) {
                        changes.insert(*key, None);
                        stats.pages_deleted += 1;
                    }
                }
            }
        }
        let mut moved: Vec<(PageKey, u32, u32, Vec<u8>)> = Vec::new();
        for &c in &compact {
            let path = self.segment_path(c);
            let bytes = fs::read(&path).map_err(io(&path))?;
            for (key, loc, stored) in segment_records(&path, c, &bytes)? {
                if self.pages.get(&key) == Some(&loc) && !batch.changes.contains_key(&key) {
                    let actual = crc32fast::hash(stored);
                    if actual != loc.crc {
                        return Err(PageError::Crc {
                            what: format!("page {key:?} during compaction"),
                            expected: loc.crc,
                            actual,
                        });
                    }
                    moved.push((key, loc.raw_len, loc.crc, stored.to_vec()));
                }
            }
        }
        moved.sort_by_key(|m| m.0);
        for (key, raw_len, crc, stored) in moved {
            let loc = push(&mut seg, key, raw_len, crc, &stored);
            changes.insert(key, Some(loc));
            stats.pages_compacted += 1;
        }
        let wrote_segment = stats.pages_written + stats.pages_compacted > 0;

        // 1. The segment, durable before anything names it.
        if wrote_segment {
            let path = self.segment_path(seg_seq);
            if self.fault == Some(Fault::MidSegment) {
                let mut name = path.file_name().unwrap_or_default().to_os_string();
                name.push(".tmp");
                let tmp = path.with_file_name(name);
                fs::write(&tmp, &seg.as_bytes()[..seg.len() / 2]).map_err(io(&tmp))?;
                self.poisoned = true;
                return Err(PageError::Injected);
            }
            write_atomic(&path, seg.as_bytes(), self.durability)?;
            stats.segment_bytes = seg.len() as u64;
        }
        if self.fault == Some(Fault::BeforeManifest) {
            self.poisoned = true;
            return Err(PageError::Injected);
        }

        // 2. The log record, then the index in memory; 3. cleanup; 4. maybe a checkpoint.
        let rec = Rec::Commit {
            save_seq,
            obs_seq,
            pending,
            segment: wrote_segment.then_some((seg_seq, seg.len() as u64)),
            changes: changes.into_iter().collect(),
        };
        let s = self.log_and_apply(rec)?;
        stats.log_bytes = s.log_bytes;
        stats.checkpoint_bytes = s.checkpoint_bytes;
        stats.manifest_bytes = s.manifest_bytes;
        stats.segments_removed = s.segments_removed;
        Ok(stats)
    }

    fn addref(&mut self, l: PageLoc, cur: bool) {
        let s = self
            .segments
            .get_mut(&l.segment)
            .expect("a page names a known segment");
        s.refs += 1;
        if cur {
            s.cur_pages += 1;
            s.cur_bytes += RECORD_HEADER + u64::from(l.stored_len);
        }
    }

    fn unref(&mut self, l: PageLoc, cur: bool, touched: &mut Vec<u32>) {
        let s = self
            .segments
            .get_mut(&l.segment)
            .expect("a page names a known segment");
        s.refs -= 1;
        if cur {
            s.cur_pages -= 1;
            s.cur_bytes -= RECORD_HEADER + u64::from(l.stored_len);
        }
        touched.push(l.segment);
    }

    fn drop_prev(&mut self, touched: &mut Vec<u32>) {
        for (_, old) in std::mem::take(&mut self.prev) {
            if let Some(o) = old {
                self.unref(o, false, touched);
            }
        }
    }

    /// Applies a record to the in-memory index, live or on replay, and returns the segments it
    /// left with no reference (their files are deleted by the caller once the record is durable).
    fn apply(&mut self, rec: &Rec) -> Vec<u32> {
        let mut touched = Vec::new();
        match rec {
            Rec::Commit {
                save_seq,
                obs_seq,
                pending,
                segment,
                changes,
            } => {
                if let Some((seq, bytes)) = *segment {
                    self.segments.insert(
                        seq,
                        SegmentInfo {
                            file_bytes: bytes,
                            ..SegmentInfo::default()
                        },
                    );
                    self.next_segment = self.next_segment.max(seq + 1);
                }
                for &(key, new) in changes {
                    let old = match new {
                        Some(l) => self.pages.insert(key, l),
                        None => self.pages.remove(&key),
                    };
                    if let Some(n) = new {
                        self.addref(n, true);
                    }
                    if let Some(o) = old {
                        self.unref(o, true, &mut touched);
                    }
                    if *pending && !self.prev.contains_key(&key) {
                        self.prev.insert(key, old);
                        if let Some(o) = old {
                            self.addref(o, false);
                        }
                    }
                }
                self.last_seq = self.last_seq.max(*save_seq);
                self.save_seq = *save_seq;
                self.obs_seq = *obs_seq;
                if !*pending {
                    self.drop_prev(&mut touched);
                    self.confirmed = self.current();
                }
            }
            Rec::Confirm { save_seq } => {
                if *save_seq == self.save_seq {
                    self.drop_prev(&mut touched);
                    self.confirmed = self.current();
                }
            }
            Rec::Rollback => {
                for (key, old) in std::mem::take(&mut self.prev) {
                    if let Some(c) = self.pages.remove(&key) {
                        self.unref(c, true, &mut touched);
                    }
                    if let Some(o) = old {
                        self.unref(o, false, &mut touched);
                        self.pages.insert(key, o);
                        self.addref(o, true);
                    }
                }
                self.save_seq = self.confirmed.save_seq;
                self.obs_seq = self.confirmed.obs_seq;
            }
        }
        touched.sort_unstable();
        touched.dedup();
        let dead: Vec<u32> = touched
            .into_iter()
            .filter(|s| self.segments.get(s).is_some_and(|i| i.refs == 0))
            .collect();
        for s in &dead {
            self.segments.remove(s);
            self.files.remove(s);
        }
        dead
    }

    fn delete_segments(&mut self, dead: Vec<u32>) -> Result<u64> {
        let mut n = 0;
        for seq in dead {
            let path = self.segment_path(seq);
            match fs::remove_file(&path) {
                Ok(()) => n += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(PageError::Io { path, source: e }),
            }
        }
        Ok(n)
    }

    /// Appends one framed record to the log and syncs it. Returns the bytes appended.
    fn append(&mut self, payload: &[u8]) -> Result<u64> {
        let path = self.log_path(self.ckpt_id);
        if self.log.is_none() {
            let f = if self.log_bytes == 0 {
                let mut f = File::create(&path).map_err(io(&path))?;
                let mut h = Writer::with_capacity(LOG_HEADER as usize);
                h.raw(&LOG_MAGIC);
                h.u16(FORMAT_VERSION);
                h.u16(0);
                h.u64(self.ckpt_id);
                f.write_all(h.as_bytes()).map_err(io(&path))?;
                if self.durability == Durability::Fsync {
                    f.sync_all().map_err(io(&path))?;
                    sync_dir(&self.dir);
                }
                self.log_bytes = LOG_HEADER;
                f
            } else {
                OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .map_err(io(&path))?
            };
            self.log = Some(f);
        }
        let mut rec = Vec::with_capacity(payload.len() + 8);
        rec.extend_from_slice(
            &u32::try_from(payload.len())
                .expect("record under 4 GiB")
                .to_le_bytes(),
        );
        rec.extend_from_slice(&crc32fast::hash(payload).to_le_bytes());
        rec.extend_from_slice(payload);
        let f = self.log.as_mut().expect("opened above");
        if self.fault == Some(Fault::TornLog) {
            f.write_all(&rec[..rec.len() / 2]).map_err(io(&path))?;
            let _ = f.sync_data();
            return Err(PageError::Injected);
        }
        f.write_all(&rec).map_err(io(&path))?;
        if self.durability == Durability::Fsync {
            f.sync_data().map_err(io(&path))?;
        }
        self.log_bytes += rec.len() as u64;
        Ok(rec.len() as u64)
    }

    fn checkpoint_due(&self) -> bool {
        self.ckpt_id == 0 || self.log_bytes >= self.ckpt_bytes.max(LOG_MIN_BYTES)
    }

    /// Writes a checkpoint of the whole index, then deletes the log it replaces.
    fn checkpoint(&mut self) -> Result<u64> {
        let id = self.ckpt_id + 1;
        let bytes = self.encode_checkpoint(id);
        let path = self.dir.join(MANIFEST_NAME);
        if self.fault == Some(Fault::MidCheckpoint) {
            let tmp = self.dir.join(format!("{MANIFEST_NAME}.tmp"));
            fs::write(&tmp, &bytes).map_err(io(&tmp))?;
            return Err(PageError::Injected);
        }
        write_atomic(&path, &bytes, self.durability)?;
        if self.fault == Some(Fault::AfterCheckpoint) {
            return Err(PageError::Injected);
        }
        let old = self.log_path(self.ckpt_id);
        self.log = None;
        match fs::remove_file(&old) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(PageError::Io {
                    path: old,
                    source: e,
                });
            }
        }
        self.ckpt_id = id;
        self.ckpt_bytes = bytes.len() as u64;
        self.log_bytes = 0;
        Ok(self.ckpt_bytes)
    }

    fn encode_checkpoint(&self, id: u64) -> Vec<u8> {
        let mut body = Writer::with_capacity(64 + self.pages.len() * 14);
        body.var_u64(id);
        body.var_u64(self.save_seq);
        body.var_u64(self.obs_seq);
        body.var_u64(self.confirmed.save_seq);
        body.var_u64(self.confirmed.obs_seq);
        body.var_u64(self.last_seq);
        body.var_u64(u64::from(self.next_segment));
        body.var_u64(self.segments.len() as u64);
        for (&seq, s) in &self.segments {
            body.var_u64(u64::from(seq));
            body.var_u64(s.file_bytes);
        }
        let mut pages: Vec<(&PageKey, &PageLoc)> = self.pages.iter().collect();
        pages.sort_unstable_by_key(|(k, _)| **k);
        body.var_u64(pages.len() as u64);
        let mut prev = 0u64;
        for (k, l) in pages {
            body.var_u64(k.0 - prev);
            prev = k.0;
            put_loc(&mut body, l);
        }
        let mut old: Vec<(&PageKey, &Option<PageLoc>)> = self.prev.iter().collect();
        old.sort_unstable_by_key(|(k, _)| **k);
        body.var_u64(old.len() as u64);
        let mut prevk = 0u64;
        for (k, l) in old {
            body.var_u64(k.0 - prevk);
            prevk = k.0;
            match l {
                Some(l) => {
                    body.u8(1);
                    put_loc(&mut body, l);
                }
                None => body.u8(0),
            }
        }
        let body = body.into_bytes();
        let stored = lz4_flex::block::compress(&body);
        let mut out = Writer::with_capacity(stored.len() + 20);
        out.raw(&MANIFEST_MAGIC);
        out.u16(FORMAT_VERSION);
        out.u16(0);
        out.u32(u32::try_from(body.len()).expect("checkpoint over 4 GiB"));
        out.u32(stored.len() as u32);
        out.u32(crc32fast::hash(&stored));
        out.raw(&stored);
        out.into_bytes()
    }

    fn parse_checkpoint(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
        let bad =
            |e: crate::codec::CodecError| PageError::Format(format!("{}: {e}", path.display()));
        let mut r = Reader::new(bytes);
        if r.take(4).map_err(bad)? != MANIFEST_MAGIC {
            return Err(PageError::Format(format!(
                "{}: not a manifest",
                path.display()
            )));
        }
        let version = r.u16().map_err(bad)?;
        if version != FORMAT_VERSION {
            return Err(PageError::Format(format!(
                "{}: format {version}, this build reads {FORMAT_VERSION}",
                path.display()
            )));
        }
        let _flags = r.u16().map_err(bad)?;
        let raw_len = r.u32().map_err(bad)?;
        let stored_len = r.u32().map_err(bad)?;
        let crc = r.u32().map_err(bad)?;
        let stored = r.take(stored_len as usize).map_err(bad)?;
        r.finish().map_err(bad)?;
        let body = decode_stored(stored, raw_len, crc, "manifest")?;

        let mut r = Reader::new(&body);
        self.ckpt_id = r.var_u64().map_err(bad)?;
        self.save_seq = r.var_u64().map_err(bad)?;
        self.obs_seq = r.var_u64().map_err(bad)?;
        self.confirmed = Generation {
            save_seq: r.var_u64().map_err(bad)?,
            obs_seq: r.var_u64().map_err(bad)?,
        };
        self.last_seq = r.var_u64().map_err(bad)?;
        self.next_segment = get_u32(&mut r).map_err(bad)?;
        let nseg = r.var_u64().map_err(bad)?;
        for _ in 0..nseg {
            let seq = get_u32(&mut r).map_err(bad)?;
            let file_bytes = r.var_u64().map_err(bad)?;
            self.segments.insert(
                seq,
                SegmentInfo {
                    file_bytes,
                    ..SegmentInfo::default()
                },
            );
        }
        let known = |segments: &BTreeMap<u32, SegmentInfo>, key: u64, l: &PageLoc| {
            if segments.contains_key(&l.segment) {
                Ok(())
            } else {
                Err(PageError::Format(format!(
                    "page {key:#x} names unknown segment {}",
                    l.segment
                )))
            }
        };
        let npages = r.var_u64().map_err(bad)?;
        let mut key = 0u64;
        for _ in 0..npages {
            key += r.var_u64().map_err(bad)?;
            let loc = get_loc(&mut r).map_err(bad)?;
            known(&self.segments, key, &loc)?;
            self.addref(loc, true);
            self.pages.insert(PageKey(key), loc);
        }
        let nprev = r.var_u64().map_err(bad)?;
        let mut key = 0u64;
        for _ in 0..nprev {
            key += r.var_u64().map_err(bad)?;
            let old = match r.u8().map_err(bad)? {
                0 => None,
                _ => {
                    let loc = get_loc(&mut r).map_err(bad)?;
                    known(&self.segments, key, &loc)?;
                    self.addref(loc, false);
                    Some(loc)
                }
            };
            self.prev.insert(PageKey(key), old);
        }
        r.finish().map_err(bad)?;
        self.ckpt_bytes = bytes.len() as u64;
        Ok(())
    }

    /// Replays the log after the checkpoint. A torn last record (a crash mid-append) is cut off;
    /// a bad record with more after it is corruption and stops the open.
    fn replay_log(&mut self) -> Result<()> {
        let path = self.log_path(self.ckpt_id);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(PageError::Io { path, source: e }),
        };
        let len = bytes.len() as u64;
        if len < LOG_HEADER {
            // A crash while the log was being created: it holds no record.
            fs::remove_file(&path).map_err(io(&path))?;
            return Ok(());
        }
        let mut r = Reader::new(&bytes[..LOG_HEADER as usize]);
        let bad =
            |e: crate::codec::CodecError| PageError::Format(format!("{}: {e}", path.display()));
        let magic = r.take(4).map_err(bad)?;
        let version = r.u16().map_err(bad)?;
        let _flags = r.u16().map_err(bad)?;
        let id = r.u64().map_err(bad)?;
        if magic != LOG_MAGIC || version != FORMAT_VERSION || id != self.ckpt_id {
            return Err(PageError::Format(format!(
                "{}: not the log of checkpoint {} in format {FORMAT_VERSION}",
                path.display(),
                self.ckpt_id
            )));
        }
        let mut off = LOG_HEADER;
        let mut torn = false;
        while off < len {
            if len - off < 8 {
                torn = true;
                break;
            }
            let at = off as usize;
            let rlen = u64::from(u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4")));
            let crc = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("4"));
            if off + 8 + rlen > len {
                torn = true;
                break;
            }
            let payload = &bytes[at + 8..at + 8 + rlen as usize];
            let actual = crc32fast::hash(payload);
            if actual != crc {
                if off + 8 + rlen == len {
                    torn = true;
                    break;
                }
                return Err(PageError::Crc {
                    what: format!("{} record at {off}", path.display()),
                    expected: crc,
                    actual,
                });
            }
            let rec = Rec::decode(payload).map_err(bad)?;
            if let Rec::Commit {
                segment: Some((seq, _)),
                ..
            } = &rec
                && self.segments.contains_key(seq)
            {
                return Err(PageError::Format(format!(
                    "{}: segment {seq} written twice",
                    path.display()
                )));
            }
            self.apply(&rec);
            off += 8 + rlen;
        }
        if torn {
            let f = OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(io(&path))?;
            f.set_len(off).map_err(io(&path))?;
            if self.durability == Durability::Fsync {
                f.sync_all().map_err(io(&path))?;
            }
        }
        self.log_bytes = off;
        Ok(())
    }

    /// Removes `.tmp` files, segments the index does not name, and logs of other checkpoints.
    /// Only touches names this store writes.
    fn remove_strays(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.dir).map_err(io(&self.dir))?;
        for entry in entries {
            let entry = entry.map_err(io(&self.dir))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let hex_of = |prefix: &str, suffix: &str| {
                name.strip_prefix(prefix)
                    .and_then(|n| n.strip_suffix(suffix))
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
            };
            let stray = if name.ends_with(".tmp")
                && (name.starts_with("seg-")
                    || name.starts_with("manifest")
                    || name.starts_with("mlog-"))
            {
                true
            } else if let Some(seq) = hex_of("seg-", ".rts") {
                u32::try_from(seq).map_or(true, |s| !self.segments.contains_key(&s))
            } else if let Some(id) = hex_of("mlog-", ".rtl") {
                id != self.ckpt_id
            } else {
                false
            };
            if stray {
                let path = entry.path();
                fs::remove_file(&path).map_err(io(&path))?;
            }
        }
        Ok(())
    }
}

/// The records of a segment file: (key, location, stored bytes).
fn segment_records<'a>(
    path: &Path,
    seq: u32,
    bytes: &'a [u8],
) -> Result<Vec<(PageKey, PageLoc, &'a [u8])>> {
    let bad = |what: &str| PageError::Format(format!("{}: {what}", path.display()));
    if bytes.len() < SEGMENT_HEADER as usize || bytes[..4] != SEGMENT_MAGIC {
        return Err(bad("not a segment"));
    }
    let mut out = Vec::new();
    let mut off = SEGMENT_HEADER as usize;
    while off < bytes.len() {
        if bytes.len() - off < RECORD_HEADER as usize {
            return Err(bad("truncated record header"));
        }
        let key = u64::from_le_bytes(bytes[off..off + 8].try_into().expect("8"));
        let raw_len = u32::from_le_bytes(bytes[off + 8..off + 12].try_into().expect("4"));
        let stored_len = u32::from_le_bytes(bytes[off + 12..off + 16].try_into().expect("4"));
        let crc = u32::from_le_bytes(bytes[off + 16..off + 20].try_into().expect("4"));
        let start = off + RECORD_HEADER as usize;
        let end = start + stored_len as usize;
        if end > bytes.len() {
            return Err(bad("truncated record"));
        }
        out.push((
            PageKey(key),
            PageLoc {
                offset: start as u64,
                segment: seq,
                raw_len,
                stored_len,
                crc,
            },
            &bytes[start..end],
        ));
        off = end;
    }
    Ok(out)
}

/// `path.tmp`, fsync, rename, fsync of the folder: Deep Time's `write_atomic`.
fn write_atomic(path: &Path, bytes: &[u8], durability: Durability) -> Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    {
        let mut f = File::create(&tmp).map_err(io(&tmp))?;
        f.write_all(bytes).map_err(io(&tmp))?;
        if durability == Durability::Fsync {
            f.sync_all().map_err(io(&tmp))?;
        }
    }
    fs::rename(&tmp, path).map_err(io(path))?;
    if durability == Durability::Fsync {
        sync_dir(path.parent().unwrap_or(Path::new(".")));
    }
    Ok(())
}

/// Makes a rename durable. Windows cannot open a folder to sync it; there the rename is as
/// durable as the OS makes it (as in Deep Time).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-page-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn page(n: u64, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| ((i as u64 * 31 + n * 7) % 13) as u8)
            .collect()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn put_get_reopen() {
        let dir = tmpdir("basic");
        let mut s = PageStore::open(&dir, Durability::Fsync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey::new(1, 5), &page(5, 1000));
        b.put(PageKey::new(1, 6), &page(6, 0));
        let st = s.commit(b, 42).unwrap();
        assert_eq!(st.pages_written, 2);
        assert!(st.segment_bytes > 0 && st.manifest_bytes > 0);
        drop(s);
        let mut s = PageStore::open(&dir, Durability::Fsync).unwrap();
        assert_eq!(s.obs_seq(), 42);
        assert_eq!(s.save_seq(), 1);
        assert_eq!(s.get(PageKey::new(1, 5)).unwrap().unwrap(), page(5, 1000));
        assert_eq!(
            s.get(PageKey::new(1, 6)).unwrap().unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(s.get(PageKey::new(1, 7)).unwrap(), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crash_before_the_manifest_keeps_the_previous_save() {
        let dir = tmpdir("crash");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"old");
        s.commit(b, 1).unwrap();
        s.inject_fault(Some(Fault::BeforeManifest));
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"new");
        b.put(PageKey(2), b"two");
        assert!(matches!(s.commit(b, 2), Err(PageError::Injected)));
        assert!(matches!(s.get(PageKey(1)), Err(PageError::Poisoned)));
        drop(s);
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_eq!(s.obs_seq(), 1);
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"old");
        assert_eq!(s.get(PageKey(2)).unwrap(), None);
        // The crashed save's segment was removed as a stray: the first save's segment and the
        // checkpoint it wrote remain.
        assert_eq!(names(&dir), ["manifest.rtm", "seg-0000000000000001.rts"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crash_before_cleanup_keeps_the_new_save() {
        let dir = tmpdir("cleanup");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"old");
        s.commit(b, 1).unwrap();
        s.inject_fault(Some(Fault::BeforeCleanup));
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"new");
        assert!(s.commit(b, 2).is_err());
        drop(s);
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_eq!(s.obs_seq(), 2);
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"new");
        // The superseded segment outlived the crash, but the index does not name it, so opening
        // removed it as a stray.
        assert_eq!(s.segment_count(), 1);
        assert_eq!(
            names(&dir),
            [
                "manifest.rtm",
                "mlog-0000000000000001.rtl",
                "seg-0000000000000002.rts"
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Every point of the save order: after a crash there, the store opens at the last save that
    /// completed (or at the new one, once its record is durable), and nothing stray remains.
    #[test]
    fn every_crash_point_opens_at_a_complete_save() {
        for fault in [
            Fault::MidSegment,
            Fault::BeforeManifest,
            Fault::TornLog,
            Fault::BeforeCleanup,
            Fault::MidCheckpoint,
            Fault::AfterCheckpoint,
        ] {
            let dir = tmpdir(&format!("points-{fault:?}"));
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            for round in 0..3u64 {
                let mut b = SaveBatch::new();
                for k in 0..4 {
                    b.put(PageKey(k), &page(k + 10 * round, 300));
                }
                s.commit(b, round + 1).unwrap();
            }
            let mut b = SaveBatch::new();
            b.put(PageKey(0), &page(99, 300));
            b.delete(PageKey(3));
            s.inject_fault(Some(fault));
            let r = match fault {
                Fault::MidCheckpoint | Fault::AfterCheckpoint => {
                    // Checkpoint faults: a save that then folds its log.
                    s.inject_fault(None);
                    s.commit(b, 4).unwrap();
                    s.inject_fault(Some(fault));
                    s.checkpoint_now().map(|_| ())
                }
                _ => s.commit(b, 4).map(|_| ()),
            };
            assert!(matches!(r, Err(PageError::Injected)), "{fault:?}");
            drop(s);
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            let new = matches!(
                fault,
                Fault::BeforeCleanup | Fault::MidCheckpoint | Fault::AfterCheckpoint
            );
            assert_eq!(s.obs_seq(), if new { 4 } else { 3 }, "{fault:?}");
            let want0 = if new { page(99, 300) } else { page(20, 300) };
            assert_eq!(s.get(PageKey(0)).unwrap().unwrap(), want0, "{fault:?}");
            assert_eq!(s.get(PageKey(3)).unwrap().is_some(), !new, "{fault:?}");
            assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), page(21, 300));
            assert!(
                names(&dir).iter().all(|n| !n.ends_with(".tmp")),
                "{fault:?}: {:?}",
                names(&dir)
            );
            // And the store keeps working.
            let mut b = SaveBatch::new();
            b.put(PageKey(7), b"after");
            s.commit(b, 5).unwrap();
            drop(s);
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            assert_eq!(s.get(PageKey(7)).unwrap().unwrap(), b"after");
            fs::remove_dir_all(&dir).unwrap();
        }
    }

    /// P29: a pending save keeps the confirmed generation's pages until it is confirmed, and a
    /// store opened against a journal that never got past the confirmed generation rolls back.
    #[test]
    fn two_generations() {
        let dir = tmpdir("gens");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"a1");
        b.put(PageKey(2), b"b1");
        s.commit_pending(b, 10).unwrap();
        assert!(s.confirm(1).unwrap());
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"a2");
        b.delete(PageKey(2));
        b.put(PageKey(3), b"c2");
        s.commit_pending(b, 20).unwrap();
        // Reads see the pending generation.
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"a2");
        assert_eq!(s.get(PageKey(2)).unwrap(), None);
        assert_eq!(s.confirmed().obs_seq, 10);
        // A second pending save before confirmation still keeps generation 1.
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"a3");
        s.commit_pending(b, 30).unwrap();
        drop(s);

        // The world's save reached observation 30: keep the newest.
        let mut s = PageStore::open_for_journal(&dir, Durability::NoSync, 30).unwrap();
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"a3");
        drop(s);
        // The world's save only reached 15: roll back to the confirmed generation.
        let mut s = PageStore::open_for_journal(&dir, Durability::NoSync, 15).unwrap();
        assert_eq!(
            s.current(),
            Generation {
                save_seq: 1,
                obs_seq: 10
            }
        );
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"a1");
        assert_eq!(s.get(PageKey(2)).unwrap().unwrap(), b"b1");
        assert_eq!(s.get(PageKey(3)).unwrap(), None);
        // A new save after a rollback never reuses a rolled-back number.
        let mut b = SaveBatch::new();
        b.put(PageKey(4), b"d");
        s.commit_pending(b, 16).unwrap();
        assert_eq!(s.save_seq(), 4);
        assert!(s.confirm(4).unwrap());
        drop(s);
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_eq!(s.get(PageKey(4)).unwrap().unwrap(), b"d");
        assert_eq!(s.get(PageKey(1)).unwrap().unwrap(), b"a1");
        // Only live segments remain on disk.
        let segs = names(&dir).iter().filter(|n| n.ends_with(".rts")).count();
        assert_eq!(segs, s.segment_count());
        // Both generations ahead of the journal is an error, not a guess.
        drop(s);
        assert!(PageStore::open_for_journal(&dir, Durability::NoSync, 5).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A save's index bytes follow the pages it changed, not the pages the store holds.
    #[test]
    fn the_index_grows_with_the_save_not_the_store() {
        let dir = tmpdir("incremental");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        // Pages that do not compress, so the first segment is not a small one.
        let noise = |k: u64| -> Vec<u8> {
            let mut x = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            (0..100)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                })
                .collect()
        };
        let mut b = SaveBatch::new();
        for k in 0..20_000u64 {
            b.put(PageKey(k), &noise(k));
        }
        s.commit(b, 1).unwrap();
        for round in 0..20u64 {
            let mut b = SaveBatch::new();
            for k in 0..10 {
                b.put(PageKey(round * 97 + k), &noise(round + 50_000));
            }
            let st = s.commit(b, round + 2).unwrap();
            assert_eq!(st.checkpoint_bytes, 0);
            // About sixteen bytes per changed page (compaction moves count as changes), whatever
            // the 20,000 pages the store holds.
            let changed = st.pages_written + st.pages_compacted + st.pages_deleted;
            assert!(
                st.log_bytes <= 32 + 16 * changed,
                "round {round}: {} bytes for {changed} changes",
                st.log_bytes
            );
            assert!(changed < 200, "round {round}: {changed} changes");
        }
        let (_, ckpt) = s.index_file_bytes();
        assert!(
            ckpt > 50_000,
            "the checkpoint holds the whole index: {ckpt}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A manifest that exists but cannot be read must stop the open, not turn every segment into
    /// a stray to delete.
    #[test]
    fn an_unreadable_manifest_deletes_nothing() {
        let dir = tmpdir("unreadable");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey(1), b"keep me");
        s.commit(b, 1).unwrap();
        drop(s);
        let manifest = dir.join(MANIFEST_NAME);
        fs::remove_file(&manifest).unwrap();
        fs::create_dir(&manifest).unwrap();
        assert!(PageStore::open(&dir, Durability::NoSync).is_err());
        let segments = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "rts")
            })
            .count();
        assert_eq!(segments, 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corruption_is_detected() {
        let dir = tmpdir("corrupt");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        b.put(PageKey(9), &page(9, 500));
        s.commit(b, 0).unwrap();
        drop(s);
        let seg = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "rts"))
            .unwrap();
        let mut bytes = fs::read(&seg).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x40;
        fs::write(&seg, bytes).unwrap();
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert!(matches!(s.get(PageKey(9)), Err(PageError::Crc { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A bit flip in a log record that has records after it is corruption: the open fails
    /// rather than silently dropping the later saves.
    #[test]
    fn a_bad_record_mid_log_stops_the_open() {
        let dir = tmpdir("midlog");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        for round in 0..3u64 {
            let mut b = SaveBatch::new();
            b.put(PageKey(round), b"x");
            s.commit(b, round).unwrap();
        }
        drop(s);
        let log = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "rtl"))
            .unwrap();
        let mut bytes = fs::read(&log).unwrap();
        bytes[LOG_HEADER as usize + 9] ^= 0x01;
        fs::write(&log, bytes).unwrap();
        assert!(matches!(
            PageStore::open(&dir, Durability::NoSync),
            Err(PageError::Crc { .. })
        ));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rewrites_compact_old_segments() {
        let dir = tmpdir("compact");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut b = SaveBatch::new();
        for k in 0..10 {
            b.put(PageKey(k), &page(k, 2000));
        }
        s.commit(b, 0).unwrap();
        // One page rewritten per save, forty saves: the first segment drops under half live and
        // its survivors move, and the one-page segments get merged whenever they pile up.
        for round in 0..40u64 {
            let mut b = SaveBatch::new();
            b.put(PageKey(round % 8), &page(round + 100, 2000));
            s.commit(b, round + 1).unwrap();
            assert!(
                s.segment_count() <= MAX_SMALL_SEGMENTS + 1,
                "{} segments after save {round}",
                s.segment_count()
            );
        }
        for k in 0..10 {
            let want = if k < 8 {
                page(32 + k + 100, 2000)
            } else {
                page(k, 2000)
            };
            assert_eq!(s.get(PageKey(k)).unwrap().unwrap(), want);
        }
        drop(s);
        // Segments, the checkpoint and the log.
        let files = fs::read_dir(&dir).unwrap().count();
        assert!(files <= MAX_SMALL_SEGMENTS + 3, "{files} files");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[derive(Clone, Debug)]
    enum Op {
        Put(u8, u16),
        Delete(u8),
        Commit,
        Pending,
        Confirm,
        Rollback,
        Checkpoint,
        Reopen,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            6 => (any::<u8>(), 0u16..3000).prop_map(|(k, n)| Op::Put(k % 32, n)),
            2 => any::<u8>().prop_map(|k| Op::Delete(k % 32)),
            2 => Just(Op::Commit),
            2 => Just(Op::Pending),
            1 => Just(Op::Confirm),
            1 => Just(Op::Rollback),
            1 => Just(Op::Checkpoint),
            1 => Just(Op::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        /// The store agrees with a model of two generations through puts, deletes, commits,
        /// pending commits, confirmations, rollbacks, checkpoints, compactions and reopens;
        /// uncommitted batches vanish on reopen.
        #[test]
        fn matches_a_model(ops in prop::collection::vec(op(), 1..80)) {
            let dir = tmpdir("model");
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            let mut current: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
            let mut confirmed: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
            let mut generation = (0u64, 0u64);
            let mut conf_gen = (0u64, 0u64);
            let mut pending: BTreeMap<u64, Option<Vec<u8>>> = BTreeMap::new();
            let mut batch = SaveBatch::new();
            let mut obs = 0;
            for op in ops {
                match op {
                    Op::Put(k, n) => {
                        let p = page(u64::from(n), n as usize);
                        batch.put(PageKey(u64::from(k)), &p);
                        pending.insert(u64::from(k), Some(p));
                    }
                    Op::Delete(k) => {
                        batch.delete(PageKey(u64::from(k)));
                        pending.insert(u64::from(k), None);
                    }
                    Op::Commit | Op::Pending => {
                        obs += 1;
                        let b = std::mem::take(&mut batch);
                        if matches!(op, Op::Commit) {
                            s.commit(b, obs).unwrap();
                        } else {
                            s.commit_pending(b, obs).unwrap();
                        }
                        for (k, v) in std::mem::take(&mut pending) {
                            match v {
                                Some(p) => { current.insert(k, p); }
                                None => { current.remove(&k); }
                            }
                        }
                        generation = (s.save_seq(), obs);
                        if matches!(op, Op::Commit) {
                            confirmed = current.clone();
                            conf_gen = generation;
                        }
                    }
                    Op::Confirm => {
                        if s.confirm(generation.0).unwrap() {
                            confirmed = current.clone();
                            conf_gen = generation;
                        }
                        prop_assert_eq!(s.confirmed().obs_seq, conf_gen.1);
                    }
                    Op::Rollback => {
                        s.rollback().unwrap();
                        current = confirmed.clone();
                        generation = conf_gen;
                    }
                    Op::Checkpoint => {
                        s.checkpoint_now().unwrap();
                    }
                    Op::Reopen => {
                        drop(s);
                        s = PageStore::open(&dir, Durability::NoSync).unwrap();
                        batch = SaveBatch::new();
                        pending.clear();
                    }
                }
                prop_assert_eq!(s.obs_seq(), generation.1);
                prop_assert_eq!(s.len(), current.len());
                for (k, v) in &current {
                    let got = s.get(PageKey(*k)).unwrap();
                    prop_assert_eq!(got.as_ref(), Some(v));
                }
            }
            // Whatever happened, a rollback lands on the confirmed model.
            s.rollback().unwrap();
            prop_assert_eq!(s.len(), confirmed.len());
            for (k, v) in &confirmed {
                let got = s.get(PageKey(*k)).unwrap();
                prop_assert_eq!(got.as_ref(), Some(v));
            }
            drop(s);
            // No stray files: every segment on disk is named by the index.
            let s = PageStore::open(&dir, Durability::NoSync).unwrap();
            let segs = names(&dir).iter().filter(|n| n.ends_with(".rts")).count();
            prop_assert_eq!(segs, s.segment_count());
            drop(s);
            fs::remove_dir_all(&dir).unwrap();
        }
    }
}
