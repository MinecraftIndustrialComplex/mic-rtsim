// SPDX-License-Identifier: AGPL-3.0-or-later
//! The incremental page store (SCALE.md §8, PLAN.md §4.9): pages are LZ4 with a CRC32, saves write
//! only dirty pages, and the index of where each page lives is itself paged and written
//! incrementally, so neither a save's cost nor the store's memory grows with the world.
//!
//! # Layout of `<world>/rtsim/`
//!
//! - `seg-<n>.rts`: segments, written once and never modified. A **page segment** holds the pages
//!   one save wrote, back to back. An **index segment** holds index blocks.
//! - **Index blocks**: the location (segment, offset, lengths, CRC32) of each of
//!   [`INDEX_BLOCK_PAGES`] consecutive page keys, stored like pages (LZ4 and CRC32). Only
//!   [`DEFAULT_INDEX_CACHE_BLOCKS`] decoded blocks stay in memory, least recently used first out.
//! - `manifest.rtm`: the **root**, written at each checkpoint: the generations, the segments with
//!   their reference counts, where each index block lives, and the previous generation's entries
//!   (P29). Its size is the number of blocks and segments, a few hundred bytes per million regions.
//! - `mlog-<checkpoint>.rtl`: the **log** since that checkpoint, one CRC-framed record per save
//!   (each page it changed, with where the page was and where it is now), per confirmation and
//!   per rollback.
//!
//! A save appends one record whose size is the number of pages it changed. Changes since the last
//! checkpoint stay in memory as an overlay over the blocks; when the overlay reaches
//! [`OVERLAY_MAX_PAGES`] entries or the log [`LOG_MAX_BYTES`], the next save folds them in: it
//! rewrites only the blocks they touch into a new index segment and writes a new root. A fold
//! costs the blocks the overlay touched, never the whole index, so save cost follows the dirty
//! pages and resident memory is the overlay plus the block cache, whatever the world's size.
//! (DECISIONS.md O9 recorded the old whole-manifest rewrite as a known limit; this replaces it.)
//!
//! # Two generations (P29)
//!
//! [`PageStore::commit_pending`] writes a save but keeps the pages of the last *confirmed*
//! generation until [`PageStore::confirm`] says Minecraft's own save completed. If the process dies
//! between the two, [`PageStore::open_for_journal`] opens whichever generation is not ahead of the
//! journal in Minecraft's save: the new one if Minecraft's save got the journal that far, the
//! confirmed one otherwise ([`PageStore::rollback`]). [`PageStore::commit`] is a save that is
//! confirmed at once, as before. The previous generation's entries are resident: they are the
//! pages changed since the last confirmation, bounded by what happened, not by the world.
//!
//! # Crash safety, in Deep Time's order (deeptime-format `fsutil.rs`)
//!
//! - A segment is written to `<name>.tmp`, fsynced, renamed, and its folder fsynced, before any
//!   record or root names it.
//! - A record is appended to the log and the log fsynced; that append is the moment the save
//!   happens. A crash inside the append leaves a torn last record, which the next open cuts off
//!   (the save did not happen). A record that fails its CRC with more records after it is
//!   corruption, not a torn write, and the open stops with an error.
//! - A page segment is deleted only after the record that stops naming it is durable; an index
//!   segment only after the root that stops naming it is durable. (A block on disk may still name
//!   a deleted page segment, but the log's overlay names that page's newer place, and a lookup
//!   reads the overlay first.)
//! - A root is written like a segment, and only then is the log it replaces deleted.
//! - On open, files the index does not name (a crashed save's segment, an old log, `.tmp` files)
//!   are removed.
//!
//! **Why segments rather than a file per page:** an incremental save cannot overwrite a page file
//! the current index still names, and one fsync per page would make a save of a thousand dirty
//! regions a thousand fsyncs. A save costs two fsyncs (segment, log) whatever the page count.
//! Segments whose current pages fall under half of their bytes are compacted: those pages (or
//! blocks) are copied, still compressed and CRC-checked, into the next segment.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
/// A segment's kind, in its header's flags.
const SEG_PAGES: u16 = 0;
const SEG_INDEX: u16 = 1;
/// A segment under this size counts as small. PLACEHOLDER tunable.
const SMALL_SEGMENT_BYTES: u64 = 1 << 20;
/// More small page segments than this, and the next save merges them all. PLACEHOLDER tunable.
/// Bounds the file count when long-lived pages each sit in the small segment of the save that
/// wrote them; each merge makes one bigger segment, so a page is copied about log(saves) times, as
/// in an LSM tree's tiering.
const MAX_SMALL_SEGMENTS: usize = 8;
const BLOCK_BITS: u32 = 9;
/// Page keys per index block. PLACEHOLDER tunable: 512 locations of 24 bytes, 12 KiB decoded.
pub const INDEX_BLOCK_PAGES: u64 = 1 << BLOCK_BITS;
/// Decoded index blocks kept in memory unless [`PageStore::set_index_cache_blocks`] says
/// otherwise: 1.5 MiB. PLACEHOLDER, part of the memory budget (A16).
pub const DEFAULT_INDEX_CACHE_BLOCKS: usize = 128;
/// Pages changed since the last checkpoint at which the next save folds them into the blocks.
/// PLACEHOLDER tunable: about 0.8 MiB of overlay.
pub const OVERLAY_MAX_PAGES: usize = 16_384;
/// Log bytes at which the next save folds the log, whatever the overlay holds. PLACEHOLDER.
pub const LOG_MAX_BYTES: u64 = 4 << 20;

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

/// Where a page (or an index block) lives: 24 bytes. Segment 0 is never used, so a location with
/// segment 0 marks an empty slot in an index block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageLoc {
    /// Offset of the stored bytes in the segment file.
    offset: u64,
    segment: u32,
    raw_len: u32,
    stored_len: u32,
    crc: u32,
}

const ABSENT: PageLoc = PageLoc {
    offset: 0,
    segment: 0,
    raw_len: 0,
    stored_len: 0,
    crc: 0,
};
const LOC_BYTES: usize = 24;
const BLOCK_MASK: u64 = INDEX_BLOCK_PAGES - 1;

fn present(l: PageLoc) -> Option<PageLoc> {
    (l.segment != 0).then_some(l)
}

fn encode_block(locs: &[PageLoc]) -> Vec<u8> {
    let mut out = Vec::with_capacity(locs.len() * LOC_BYTES);
    for l in locs {
        out.extend_from_slice(&l.offset.to_le_bytes());
        out.extend_from_slice(&l.segment.to_le_bytes());
        out.extend_from_slice(&l.raw_len.to_le_bytes());
        out.extend_from_slice(&l.stored_len.to_le_bytes());
        out.extend_from_slice(&l.crc.to_le_bytes());
    }
    out
}

fn decode_block(raw: &[u8]) -> Option<Box<[PageLoc]>> {
    if raw.len() != INDEX_BLOCK_PAGES as usize * LOC_BYTES {
        return None;
    }
    let u32_at =
        |b: &[u8; LOC_BYTES], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().expect("4"));
    Some(
        raw.as_chunks::<LOC_BYTES>()
            .0
            .iter()
            .map(|b| PageLoc {
                offset: u64::from_le_bytes(b[0..8].try_into().expect("8")),
                segment: u32_at(b, 8),
                raw_len: u32_at(b, 12),
                stored_len: u32_at(b, 16),
                crc: u32_at(b, 20),
            })
            .collect(),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SegmentInfo {
    /// [`SEG_PAGES`] or [`SEG_INDEX`].
    kind: u16,
    file_bytes: u64,
    /// Current pages (or index blocks) in this segment, and their record bytes.
    cur_pages: u64,
    cur_bytes: u64,
    /// Everything that needs this segment: current pages or blocks, and pages only the confirmed
    /// generation still needs (P29). At zero the segment is dead.
    refs: u64,
}

/// Decoded index blocks, least recently used out.
struct BlockCache {
    blocks: HashMap<u64, (Box<[PageLoc]>, u64)>,
    clock: u64,
    cap: usize,
}

impl BlockCache {
    fn get(&mut self, b: u64) -> Option<&[PageLoc]> {
        self.clock += 1;
        let clock = self.clock;
        self.blocks.get_mut(&b).map(|e| {
            e.1 = clock;
            &*e.0
        })
    }

    fn insert(&mut self, b: u64, data: Box<[PageLoc]>) {
        if self.cap == 0 {
            return;
        }
        if !self.blocks.contains_key(&b) && self.blocks.len() >= self.cap {
            // A linear scan for the oldest: the cache holds a few hundred blocks at most, and a
            // miss already costs a read and a decompression.
            if let Some(&old) = self
                .blocks
                .iter()
                .min_by_key(|(_, (_, stamp))| *stamp)
                .map(|(k, _)| k)
            {
                self.blocks.remove(&old);
            }
        }
        self.clock += 1;
        self.blocks.insert(b, (data, self.clock));
    }

    fn remove(&mut self, b: u64) {
        self.blocks.remove(&b);
    }

    fn heap_bytes(&self) -> usize {
        self.blocks.capacity() * (std::mem::size_of::<(u64, (Box<[PageLoc]>, u64))>() + 1)
            + self.blocks.len() * INDEX_BLOCK_PAGES as usize * std::mem::size_of::<PageLoc>()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a commit did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitStats {
    pub segment_bytes: u64,
    /// Index bytes this save wrote: its log record, plus the blocks and root of a fold if it did
    /// one.
    pub manifest_bytes: u64,
    pub pages_written: u64,
    pub pages_deleted: u64,
    pub pages_compacted: u64,
    pub segments_removed: u64,
    /// The log record's bytes.
    pub log_bytes: u64,
    /// When this save folded the log: the index segment's and the root's bytes; else 0.
    pub checkpoint_bytes: u64,
    /// When this save folded the log: index blocks written.
    pub index_blocks: u64,
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
    /// A fold's index segment is durable and the root's temporary file written, not renamed.
    MidCheckpoint,
    /// The new root is in place; the log it replaced and dead index segments were never deleted.
    AfterCheckpoint,
}

/// One save generation: its number and the observation sequence its pages include (A9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Generation {
    pub save_seq: u64,
    pub obs_seq: u64,
}

/// One page's move: where it was before the record and where it is after (`None`: absent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Change {
    key: PageKey,
    old: Option<PageLoc>,
    new: Option<PageLoc>,
}

/// One log record. Changes carry where pages were as well as where they go, so replaying a log
/// never needs to read an index block.
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
        changes: Vec<Change>,
    },
    Confirm {
        save_seq: u64,
    },
    /// Every page changed since the confirmed generation, back to where it was then.
    Rollback {
        changes: Vec<Change>,
    },
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

fn put_changes(w: &mut Writer, changes: &[Change]) {
    w.var_u64(changes.len() as u64);
    let mut prev = 0u64;
    for c in changes {
        w.var_u64(c.key.0 - prev);
        prev = c.key.0;
        w.u8(u8::from(c.old.is_some()) | u8::from(c.new.is_some()) << 1);
        if let Some(l) = &c.old {
            put_loc(w, l);
        }
        if let Some(l) = &c.new {
            put_loc(w, l);
        }
    }
}

fn get_changes(r: &mut Reader<'_>) -> crate::codec::Result<Vec<Change>> {
    let n = r.var_u64()?;
    let mut changes = Vec::with_capacity(n.min(1 << 20) as usize);
    let mut key = 0u64;
    for _ in 0..n {
        key += r.var_u64()?;
        let flags = r.u8()?;
        let old = if flags & 1 != 0 {
            Some(get_loc(r)?)
        } else {
            None
        };
        let new = if flags & 2 != 0 {
            Some(get_loc(r)?)
        } else {
            None
        };
        changes.push(Change {
            key: PageKey(key),
            old,
            new,
        });
    }
    Ok(changes)
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
                put_changes(&mut w, changes);
            }
            Rec::Confirm { save_seq } => {
                w.u8(REC_CONFIRM);
                w.var_u64(*save_seq);
            }
            Rec::Rollback { changes } => {
                w.u8(REC_ROLLBACK);
                put_changes(&mut w, changes);
            }
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
                Rec::Commit {
                    save_seq,
                    obs_seq,
                    pending,
                    segment,
                    changes: get_changes(&mut r)?,
                }
            }
            REC_CONFIRM => Rec::Confirm {
                save_seq: r.var_u64()?,
            },
            REC_ROLLBACK => Rec::Rollback {
                changes: get_changes(&mut r)?,
            },
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
    /// Live pages in the current generation.
    npages: u64,
    /// Pages changed since the last checkpoint: where they are now (`None`: deleted). Read before
    /// the blocks.
    overlay: HashMap<PageKey, Option<PageLoc>>,
    /// For each page changed since the confirmed generation: where it lived then (`None`: it did
    /// not exist). Empty when nothing is pending.
    prev: HashMap<PageKey, Option<PageLoc>>,
    /// Where each index block lives, as of the last checkpoint.
    blocks: BTreeMap<u64, PageLoc>,
    cache: Mutex<BlockCache>,
    segments: BTreeMap<u32, SegmentInfo>,
    /// The checkpoint's number (0: none yet) and the root's size; the log file is named after it.
    ckpt_id: u64,
    ckpt_bytes: u64,
    /// Bytes in the log file, header included; 0 when there is no log file.
    log_bytes: u64,
    log: Option<File>,
    files: Mutex<HashMap<u32, File>>,
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
            npages: 0,
            overlay: HashMap::new(),
            prev: HashMap::new(),
            blocks: BTreeMap::new(),
            cache: Mutex::new(BlockCache {
                blocks: HashMap::new(),
                clock: 0,
                cap: DEFAULT_INDEX_CACHE_BLOCKS,
            }),
            segments: BTreeMap::new(),
            ckpt_id: 0,
            ckpt_bytes: 0,
            log_bytes: 0,
            log: None,
            files: Mutex::new(HashMap::new()),
            poisoned: false,
            fault: None,
        };
        // Only a root that is certainly absent means "start from empty". Any other failure to
        // read it must stop here: treating it as absent would make every segment a stray.
        let manifest = store.dir.join(MANIFEST_NAME);
        match fs::read(&manifest) {
            Ok(bytes) => store.parse_root(&manifest, &bytes)?,
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
        s.select_for_journal(world_obs_seq)?;
        Ok(s)
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
        self.npages as usize
    }

    pub fn is_empty(&self) -> bool {
        self.npages == 0
    }

    /// Whether the store holds the page. May read an index block; an IO error reads as absent
    /// (a [`PageStore::get`] of the page reports it).
    pub fn contains(&self, key: PageKey) -> bool {
        matches!(self.lookup(key), Ok(Some(_)))
    }

    /// Every page key, in order. Reads every index block (it costs the whole index, for whole-world
    /// passes and tools). Panics on an IO error; [`PageStore::try_keys`] returns it.
    pub fn keys(&self) -> impl Iterator<Item = PageKey> + '_ {
        self.try_keys()
            .unwrap_or_else(|e| panic!("reading the page index: {e}"))
            .into_iter()
    }

    /// Every page key, in order, reading every index block without disturbing the cache.
    pub fn try_keys(&self) -> Result<Vec<PageKey>> {
        let mut keys = std::collections::BTreeSet::new();
        for (&b, &bloc) in &self.blocks {
            let cached = lock(&self.cache)
                .get(b)
                .map(|d| d.to_vec().into_boxed_slice());
            let data = match cached {
                Some(d) => d,
                None => self.read_block(bloc)?,
            };
            for (i, l) in data.iter().enumerate() {
                if l.segment != 0 {
                    keys.insert(PageKey(b << BLOCK_BITS | i as u64));
                }
            }
        }
        for (k, v) in &self.overlay {
            if v.is_some() {
                keys.insert(*k);
            } else {
                keys.remove(k);
            }
        }
        Ok(keys.into_iter().collect())
    }

    /// Bytes on disk: every live segment, the root and the log.
    pub fn disk_bytes(&self) -> u64 {
        self.segments.values().map(|s| s.file_bytes).sum::<u64>() + self.ckpt_bytes + self.log_bytes
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Bytes the store holds in memory: the overlay, the previous generation's entries, the
    /// block directory, the block cache and the segment table. None of it grows with the number
    /// of pages; the overlay and the previous generation grow with what changed.
    pub fn heap_bytes(&self) -> usize {
        let entry = std::mem::size_of::<(PageKey, Option<PageLoc>)>() + 1;
        self.overlay.capacity() * entry
            + self.prev.capacity() * entry
            + self.blocks.len() * (std::mem::size_of::<(u64, PageLoc)>() + 16)
            + lock(&self.cache).heap_bytes()
            + self.segments.len() * (std::mem::size_of::<(u32, SegmentInfo)>() + 16)
    }

    /// Bytes in the log since the last checkpoint, and the root's size.
    pub fn index_file_bytes(&self) -> (u64, u64) {
        (self.log_bytes, self.ckpt_bytes)
    }

    /// Index blocks on disk, and how many are decoded in memory.
    pub fn index_blocks(&self) -> (usize, usize) {
        (self.blocks.len(), lock(&self.cache).blocks.len())
    }

    /// How many decoded index blocks to keep in memory (0 keeps none: every lookup outside the
    /// overlay reads its block).
    pub fn set_index_cache_blocks(&mut self, blocks: usize) {
        let mut c = lock(&self.cache);
        c.cap = blocks;
        while c.blocks.len() > blocks {
            let oldest = c
                .blocks
                .iter()
                .min_by_key(|(_, (_, s))| *s)
                .map(|(k, _)| *k)
                .expect("non-empty");
            c.blocks.remove(&oldest);
        }
    }

    #[doc(hidden)]
    pub fn inject_fault(&mut self, fault: Option<Fault>) {
        self.fault = fault;
    }

    /// Where the page lives on disk now, for a read on another thread (the host's IO lane):
    /// [`PageLocation::read`] needs no access to the store. A location stays readable until a
    /// later commit drops its segment; a read that loses that race fails and is simply retried
    /// through [`PageStore::get`]. May read an index block; `None` on an IO error.
    pub fn locate(&self, key: PageKey) -> Option<PageLocation> {
        if self.poisoned {
            return None;
        }
        let loc = self.lookup(key).ok()??;
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

    fn read_stored(&self, loc: PageLoc) -> Result<Vec<u8>> {
        let path = self.segment_path(loc.segment);
        let mut files = lock(&self.files);
        let f = match files.entry(loc.segment) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(File::open(&path).map_err(io(&path))?)
            }
        };
        f.seek(SeekFrom::Start(loc.offset)).map_err(io(&path))?;
        let mut buf = vec![0; loc.stored_len as usize];
        f.read_exact(&mut buf).map_err(io(&path))?;
        Ok(buf)
    }

    fn read_block(&self, bloc: PageLoc) -> Result<Box<[PageLoc]>> {
        let stored = self.read_stored(bloc)?;
        let raw = decode_stored(&stored, bloc.raw_len, bloc.crc, "index block")?;
        decode_block(&raw).ok_or_else(|| PageError::Format("index block size".into()))
    }

    /// Where a page of the current generation lives: the overlay, else its index block.
    fn lookup(&self, key: PageKey) -> Result<Option<PageLoc>> {
        if let Some(v) = self.overlay.get(&key) {
            return Ok(*v);
        }
        let b = key.0 >> BLOCK_BITS;
        let Some(&bloc) = self.blocks.get(&b) else {
            return Ok(None);
        };
        let i = (key.0 & BLOCK_MASK) as usize;
        if let Some(data) = lock(&self.cache).get(b) {
            return Ok(present(data[i]));
        }
        let data = self.read_block(bloc)?;
        let r = present(data[i]);
        lock(&self.cache).insert(b, data);
        Ok(r)
    }

    /// The page's bytes, CRC-checked and decompressed, or `None` if the store has no such page.
    pub fn get(&mut self, key: PageKey) -> Result<Option<Vec<u8>>> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        let Some(loc) = self.lookup(key)? else {
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

    /// [`PageStore::confirm`] keyed by what the bridge reports (docs/boundary.md, WORLD_SAVED):
    /// the world's save holds every observation through `world_obs_seq`, so the current generation
    /// is confirmed if it includes none past that. Returns whether it confirmed anything.
    pub fn confirm_through(&mut self, world_obs_seq: u64) -> Result<bool> {
        if self.obs_seq > world_obs_seq {
            return Ok(false);
        }
        self.confirm(self.save_seq)
    }

    /// The decision [`PageStore::open_for_journal`] makes, on a store already open (the bridge's
    /// SESSION_START arrives after `start` opened the pages): keep the current generation if the
    /// world's save holds every observation it includes, else roll back to the confirmed one.
    /// Returns the generation the store is at; an error if both are ahead.
    pub fn select_for_journal(&mut self, world_obs_seq: u64) -> Result<Generation> {
        if self.obs_seq <= world_obs_seq {
            return Ok(self.current());
        }
        if self.confirmed.obs_seq <= world_obs_seq {
            self.rollback()?;
            return Ok(self.current());
        }
        Err(PageError::Format(format!(
            "both generations are ahead of the journal: current includes observations to {}, \
             confirmed to {}, the world's save to {world_obs_seq}",
            self.obs_seq, self.confirmed.obs_seq
        )))
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
        let mut keys: Vec<PageKey> = self.prev.keys().copied().collect();
        keys.sort_unstable();
        let mut changes = Vec::with_capacity(keys.len());
        for key in keys {
            changes.push(Change {
                key,
                old: self.lookup(key)?,
                new: self.prev[&key],
            });
        }
        self.log_and_apply(Rec::Rollback { changes }).map(|_| true)
    }

    /// Folds the log into the index blocks and a new root now (a clean shutdown can call it so
    /// the next open replays nothing). Returns the bytes it wrote.
    pub fn checkpoint_now(&mut self) -> Result<u64> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        self.poisoned = true;
        let (n, _) = self.checkpoint()?;
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
            let (bytes, blocks) = self.checkpoint()?;
            stats.checkpoint_bytes = bytes;
            stats.index_blocks = blocks;
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

        // Compaction candidates: page segments whose current pages fill under half of them, and
        // every small page segment once there are too many. Their current pages are found from
        // the segments' own record headers, so nothing here walks the whole index.
        let small = self
            .segments
            .values()
            .filter(|s| s.kind == SEG_PAGES && s.file_bytes < SMALL_SEGMENT_BYTES)
            .count();
        let merge_small = small > MAX_SMALL_SEGMENTS;
        let compact: Vec<u32> = self
            .segments
            .iter()
            .filter(|(_, s)| {
                s.kind == SEG_PAGES
                    && s.cur_pages > 0
                    && (s.cur_bytes * 2 < s.file_bytes - SEGMENT_HEADER
                        || (merge_small && s.file_bytes < SMALL_SEGMENT_BYTES))
            })
            .map(|(&k, _)| k)
            .collect();

        // Build the segment in memory: puts in key order, then compacted pages in key order.
        let mut seg = Writer::with_capacity(4096);
        seg.raw(&SEGMENT_MAGIC);
        seg.u16(FORMAT_VERSION);
        seg.u16(SEG_PAGES);
        seg.u64(u64::from(seg_seq));
        let mut changes: BTreeMap<PageKey, Change> = BTreeMap::new();
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
            let old = self.lookup(*key)?;
            match change {
                Some(p) => {
                    let new = push(&mut seg, *key, p.raw_len, p.crc, &p.stored);
                    changes.insert(
                        *key,
                        Change {
                            key: *key,
                            old,
                            new: Some(new),
                        },
                    );
                    stats.pages_written += 1;
                }
                None => {
                    if old.is_some() {
                        changes.insert(
                            *key,
                            Change {
                                key: *key,
                                old,
                                new: None,
                            },
                        );
                        stats.pages_deleted += 1;
                    }
                }
            }
        }
        let mut moved: Vec<(PageKey, PageLoc, Vec<u8>)> = Vec::new();
        for &c in &compact {
            let path = self.segment_path(c);
            let bytes = fs::read(&path).map_err(io(&path))?;
            for (key, loc, stored) in segment_records(&path, c, &bytes)? {
                if !batch.changes.contains_key(&key) && self.lookup(key)? == Some(loc) {
                    let actual = crc32fast::hash(stored);
                    if actual != loc.crc {
                        return Err(PageError::Crc {
                            what: format!("page {key:?} during compaction"),
                            expected: loc.crc,
                            actual,
                        });
                    }
                    moved.push((key, loc, stored.to_vec()));
                }
            }
        }
        moved.sort_by_key(|m| m.0);
        for (key, old, stored) in moved {
            let new = push(&mut seg, key, old.raw_len, old.crc, &stored);
            changes.insert(
                key,
                Change {
                    key,
                    old: Some(old),
                    new: Some(new),
                },
            );
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

        // 2. The log record, then the index in memory; 3. cleanup; 4. maybe a fold.
        let rec = Rec::Commit {
            save_seq,
            obs_seq,
            pending,
            segment: wrote_segment.then_some((seg_seq, seg.len() as u64)),
            changes: changes.into_values().collect(),
        };
        let s = self.log_and_apply(rec)?;
        stats.log_bytes = s.log_bytes;
        stats.checkpoint_bytes = s.checkpoint_bytes;
        stats.index_blocks = s.index_blocks;
        stats.manifest_bytes = s.manifest_bytes;
        stats.segments_removed = s.segments_removed;
        Ok(stats)
    }

    fn addref(&mut self, l: PageLoc, cur: bool) {
        let s = self
            .segments
            .get_mut(&l.segment)
            .expect("a location names a known segment");
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
            .expect("a location names a known segment");
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
                            kind: SEG_PAGES,
                            file_bytes: bytes,
                            ..SegmentInfo::default()
                        },
                    );
                    self.next_segment = self.next_segment.max(seq + 1);
                }
                for &Change { key, old, new } in changes {
                    self.overlay.insert(key, new);
                    self.npages = self.npages + u64::from(new.is_some()) - u64::from(old.is_some());
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
            Rec::Rollback { changes } => {
                for &Change { key, old, new } in changes {
                    self.overlay.insert(key, new);
                    self.npages = self.npages + u64::from(new.is_some()) - u64::from(old.is_some());
                    if let Some(c) = old {
                        self.unref(c, true, &mut touched);
                    }
                    if let Some(r) = new {
                        // The confirmed generation's entry becomes the current one.
                        self.unref(r, false, &mut touched);
                        self.addref(r, true);
                    }
                }
                self.prev.clear();
                self.save_seq = self.confirmed.save_seq;
                self.obs_seq = self.confirmed.obs_seq;
            }
        }
        self.collect_dead(touched)
    }

    fn collect_dead(&mut self, mut touched: Vec<u32>) -> Vec<u32> {
        touched.sort_unstable();
        touched.dedup();
        let dead: Vec<u32> = touched
            .into_iter()
            .filter(|s| self.segments.get(s).is_some_and(|i| i.refs == 0))
            .collect();
        let mut files = lock(&self.files);
        for s in &dead {
            self.segments.remove(s);
            files.remove(s);
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
        self.ckpt_id == 0
            || self.overlay.len() >= OVERLAY_MAX_PAGES
            || self.log_bytes >= LOG_MAX_BYTES
    }

    /// Folds the overlay into the index blocks it touches (written into a new index segment,
    /// with the live blocks of index segments under half used), writes a new root, then deletes
    /// the old log and dead index segments. Returns (bytes written, blocks written).
    fn checkpoint(&mut self) -> Result<(u64, u64)> {
        let id = self.ckpt_id + 1;
        let mut touched: BTreeMap<u64, Vec<(usize, Option<PageLoc>)>> = BTreeMap::new();
        for (k, v) in &self.overlay {
            touched
                .entry(k.0 >> BLOCK_BITS)
                .or_default()
                .push(((k.0 & BLOCK_MASK) as usize, *v));
        }
        // Index segments to empty into this one: those under half used, and every small one once
        // there are too many (as for page segments), so their number stays bounded.
        let small = self
            .segments
            .values()
            .filter(|s| s.kind == SEG_INDEX && s.file_bytes < SMALL_SEGMENT_BYTES)
            .count();
        let merge_small = small > MAX_SMALL_SEGMENTS;
        let sparse: Vec<u32> = self
            .segments
            .iter()
            .filter(|(_, s)| {
                s.kind == SEG_INDEX
                    && (s.cur_bytes * 2 < s.file_bytes - SEGMENT_HEADER
                        || (merge_small && s.file_bytes < SMALL_SEGMENT_BYTES))
            })
            .map(|(&k, _)| k)
            .collect();
        if !sparse.is_empty() {
            for (&b, l) in &self.blocks {
                if sparse.contains(&l.segment) {
                    touched.entry(b).or_default();
                }
            }
        }

        let seg_seq = self.next_segment;
        let mut seg = Writer::with_capacity(4096);
        seg.raw(&SEGMENT_MAGIC);
        seg.u16(FORMAT_VERSION);
        seg.u16(SEG_INDEX);
        seg.u64(u64::from(seg_seq));
        let mut rewritten: Vec<(u64, Option<PageLoc>, Box<[PageLoc]>)> = Vec::new();
        for (b, entries) in touched {
            let cached = lock(&self.cache)
                .get(b)
                .map(|d| d.to_vec().into_boxed_slice());
            let mut data = match (cached, self.blocks.get(&b)) {
                (Some(d), _) => d,
                (None, Some(&bloc)) => self.read_block(bloc)?,
                (None, None) => vec![ABSENT; INDEX_BLOCK_PAGES as usize].into_boxed_slice(),
            };
            for (i, v) in entries {
                data[i] = v.unwrap_or(ABSENT);
            }
            if data.iter().all(|l| l.segment == 0) {
                rewritten.push((b, None, data));
                continue;
            }
            let enc = EncodedPage::encode(&encode_block(&data));
            seg.u64(b);
            seg.u32(enc.raw_len);
            seg.u32(enc.stored.len() as u32);
            seg.u32(enc.crc);
            let offset = seg.len() as u64;
            seg.raw(&enc.stored);
            let loc = PageLoc {
                offset,
                segment: seg_seq,
                raw_len: enc.raw_len,
                stored_len: enc.stored.len() as u32,
                crc: enc.crc,
            };
            rewritten.push((b, Some(loc), data));
        }
        let blocks_written = rewritten.iter().filter(|r| r.1.is_some()).count() as u64;
        let mut written = 0u64;
        // 1. The index segment, durable before the root names it.
        if blocks_written > 0 {
            write_atomic(&self.segment_path(seg_seq), seg.as_bytes(), self.durability)?;
            written += seg.len() as u64;
            self.segments.insert(
                seg_seq,
                SegmentInfo {
                    kind: SEG_INDEX,
                    file_bytes: seg.len() as u64,
                    ..SegmentInfo::default()
                },
            );
            self.next_segment = seg_seq + 1;
        }
        // 2. The index in memory: blocks, their references, the cache; the overlay is folded.
        let mut gone = Vec::new();
        for (b, loc, data) in rewritten {
            if let Some(old) = self.blocks.remove(&b) {
                self.unref(old, true, &mut gone);
            }
            match loc {
                Some(l) => {
                    self.blocks.insert(b, l);
                    self.addref(l, true);
                    lock(&self.cache).insert(b, data);
                }
                None => lock(&self.cache).remove(b),
            }
        }
        self.overlay.clear();
        if self.overlay.capacity() > 2 * OVERLAY_MAX_PAGES {
            self.overlay.shrink_to(OVERLAY_MAX_PAGES);
        }
        let dead = self.collect_dead(gone);

        // 3. The root, last.
        let root = self.encode_root(id);
        let path = self.dir.join(MANIFEST_NAME);
        if self.fault == Some(Fault::MidCheckpoint) {
            let tmp = self.dir.join(format!("{MANIFEST_NAME}.tmp"));
            fs::write(&tmp, &root).map_err(io(&tmp))?;
            return Err(PageError::Injected);
        }
        write_atomic(&path, &root, self.durability)?;
        written += root.len() as u64;
        if self.fault == Some(Fault::AfterCheckpoint) {
            return Err(PageError::Injected);
        }
        // 4. What the root no longer names: the old log and dead index segments.
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
        self.ckpt_bytes = root.len() as u64;
        self.log_bytes = 0;
        self.delete_segments(dead)?;
        Ok((written, blocks_written))
    }

    fn encode_root(&self, id: u64) -> Vec<u8> {
        let mut body = Writer::with_capacity(
            128 + self.segments.len() * 16 + self.blocks.len() * 14 + self.prev.len() * 14,
        );
        body.var_u64(id);
        body.var_u64(self.save_seq);
        body.var_u64(self.obs_seq);
        body.var_u64(self.confirmed.save_seq);
        body.var_u64(self.confirmed.obs_seq);
        body.var_u64(self.last_seq);
        body.var_u64(u64::from(self.next_segment));
        body.var_u64(self.npages);
        body.var_u64(self.segments.len() as u64);
        for (&seq, s) in &self.segments {
            body.var_u64(u64::from(seq));
            body.u16(s.kind);
            body.var_u64(s.file_bytes);
            body.var_u64(s.cur_pages);
            body.var_u64(s.cur_bytes);
            body.var_u64(s.refs);
        }
        body.var_u64(self.blocks.len() as u64);
        let mut prev = 0u64;
        for (&b, l) in &self.blocks {
            body.var_u64(b - prev);
            prev = b;
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
        out.u32(u32::try_from(body.len()).expect("root over 4 GiB"));
        out.u32(stored.len() as u32);
        out.u32(crc32fast::hash(&stored));
        out.raw(&stored);
        out.into_bytes()
    }

    fn parse_root(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
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
        self.npages = r.var_u64().map_err(bad)?;
        let nseg = r.var_u64().map_err(bad)?;
        for _ in 0..nseg {
            let seq = get_u32(&mut r).map_err(bad)?;
            let info = SegmentInfo {
                kind: r.u16().map_err(bad)?,
                file_bytes: r.var_u64().map_err(bad)?,
                cur_pages: r.var_u64().map_err(bad)?,
                cur_bytes: r.var_u64().map_err(bad)?,
                refs: r.var_u64().map_err(bad)?,
            };
            self.segments.insert(seq, info);
        }
        let known = |segments: &BTreeMap<u32, SegmentInfo>, what: &str, l: &PageLoc| {
            if segments.contains_key(&l.segment) {
                Ok(())
            } else {
                Err(PageError::Format(format!(
                    "{what} names unknown segment {}",
                    l.segment
                )))
            }
        };
        let nblocks = r.var_u64().map_err(bad)?;
        let mut b = 0u64;
        for _ in 0..nblocks {
            b += r.var_u64().map_err(bad)?;
            let loc = get_loc(&mut r).map_err(bad)?;
            known(&self.segments, &format!("index block {b}"), &loc)?;
            self.blocks.insert(b, loc);
        }
        let nprev = r.var_u64().map_err(bad)?;
        let mut key = 0u64;
        for _ in 0..nprev {
            key += r.var_u64().map_err(bad)?;
            let old = match r.u8().map_err(bad)? {
                0 => None,
                _ => {
                    let loc = get_loc(&mut r).map_err(bad)?;
                    known(&self.segments, &format!("page {key:#x}"), &loc)?;
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
        // checkpoint it wrote (an index segment and the root) remain.
        assert_eq!(
            names(&dir),
            [
                "manifest.rtm",
                "seg-0000000000000001.rts",
                "seg-0000000000000002.rts"
            ]
        );
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
        assert_eq!(s.segment_count(), 2);
        assert_eq!(
            names(&dir),
            [
                "manifest.rtm",
                "mlog-0000000000000001.rtl",
                "seg-0000000000000002.rts",
                "seg-0000000000000003.rts"
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
        // The bridge's flow: requestSave(through) makes a pending save at `through`, and
        // WORLD_SAVED(through) confirms it; a WORLD_SAVED behind the save confirms nothing.
        let mut b = SaveBatch::new();
        b.put(PageKey(5), b"e");
        s.commit_pending(b, 40).unwrap();
        assert!(!s.confirm_through(39).unwrap());
        assert!(s.confirm_through(40).unwrap());
        assert_eq!(s.confirmed().obs_seq, 40);
        let mut b = SaveBatch::new();
        b.put(PageKey(5), b"f");
        s.commit_pending(b, 50).unwrap();
        // SESSION_START says the world's save stopped at 45: back to 40.
        assert_eq!(s.select_for_journal(45).unwrap().obs_seq, 40);
        assert_eq!(s.get(PageKey(5)).unwrap().unwrap(), b"e");
        // Both generations ahead of the journal is an error, not a guess.
        drop(s);
        assert!(PageStore::open_for_journal(&dir, Durability::NoSync, 5).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Pages that do not compress, so segments have real size.
    fn noise(k: u64) -> Vec<u8> {
        let mut x = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..100)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    /// A save's index bytes follow the pages it changed, not the pages the store holds, and so
    /// does the store's memory: the index is paged.
    #[test]
    fn the_index_grows_with_the_save_not_the_store() {
        let mut heaps = Vec::new();
        for total in [10_000u64, 100_000] {
            let dir = tmpdir(&format!("incremental-{total}"));
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            s.set_index_cache_blocks(4);
            // Chunks of 10,000 pages make segments over the small-segment size, so the rounds
            // below do not merge them.
            for chunk in 0..total / 10_000 {
                let mut b = SaveBatch::new();
                for k in chunk * 10_000..(chunk + 1) * 10_000 {
                    b.put(PageKey(k), &noise(k));
                }
                s.commit(b, 1).unwrap();
                s.checkpoint_now().unwrap();
            }
            for round in 0..20u64 {
                let mut b = SaveBatch::new();
                for k in 0..10 {
                    b.put(
                        PageKey((round * 977 + k * 131) % total),
                        &noise(round + 90_000),
                    );
                }
                let st = s.commit(b, round + 2).unwrap();
                assert_eq!(st.checkpoint_bytes, 0);
                // Each changed page logs where it was and where it is, about 25 bytes,
                // compaction moves included, whatever the store holds.
                let changed = st.pages_written + st.pages_compacted + st.pages_deleted;
                assert!(
                    st.log_bytes <= 32 + 28 * changed,
                    "round {round}: {} bytes for {changed} changes",
                    st.log_bytes
                );
                assert!(changed < 200, "round {round}: {changed} changes");
            }
            let (_, root) = s.index_file_bytes();
            let (blocks, cached) = s.index_blocks();
            assert_eq!(blocks as u64, total.div_ceil(INDEX_BLOCK_PAGES));
            assert!(cached <= 4);
            // The root lists blocks and segments, not pages.
            assert!(
                root < 200 + 40 * blocks as u64,
                "root {root} bytes, {blocks} blocks"
            );
            for k in (0..total).step_by(97) {
                assert!(s.get(PageKey(k)).unwrap().is_some());
            }
            heaps.push(s.heap_bytes());
            assert_eq!(s.keys().count(), total as usize);
            assert_eq!(s.len(), total as usize);
            drop(s);
            fs::remove_dir_all(&dir).unwrap();
        }
        // Ten times the pages, nearly the same memory: the block directory grows by about 50
        // bytes per 512 pages; the rest is the overlay and the cache, both bounded.
        assert!(
            (heaps[1] as f64) < heaps[0] as f64 * 1.05,
            "store memory at 10,000 and 100,000 pages: {heaps:?}"
        );
    }

    /// Folds rewrite only the blocks they touch, and index segments left mostly dead are
    /// compacted, so their number stays bounded however many folds there are.
    #[test]
    fn folds_rewrite_touched_blocks_and_compact_index_segments() {
        let dir = tmpdir("folds");
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        s.set_index_cache_blocks(2);
        let mut b = SaveBatch::new();
        for k in 0..20_000u64 {
            b.put(PageKey(k), &noise(k));
        }
        // The first save folds at once: every block.
        let first = s.commit(b, 0).unwrap().checkpoint_bytes;
        assert!(first > 0);
        let mut model: BTreeMap<u64, Vec<u8>> = (0..20_000u64).map(|k| (k, noise(k))).collect();
        let mut folds = Vec::new();
        for round in 0..60u64 {
            let mut b = SaveBatch::new();
            // One block's worth of keys each round, and a delete.
            let base = (round * 3 % 39) * INDEX_BLOCK_PAGES;
            for k in base..base + 5 {
                b.put(PageKey(k), &noise(k + 1_000_000 * round));
                model.insert(k, noise(k + 1_000_000 * round));
            }
            b.delete(PageKey(base + 7));
            model.remove(&(base + 7));
            s.commit(b, round + 1).unwrap();
            folds.push(s.checkpoint_now().unwrap());
            // At most the small page segments and the small index segments before a merge, plus
            // the big first ones of each kind.
            assert!(
                s.segment_count() <= 2 * (MAX_SMALL_SEGMENTS + 1) + 2,
                "{}",
                s.segment_count()
            );
        }
        // A fold writes the blocks it touched; now and then one also merges the small index
        // segments (tiering), which rewrites more, never more than the whole index.
        folds.sort_unstable();
        assert!(folds[folds.len() / 2] < first / 10, "{folds:?} vs {first}");
        assert!(
            *folds.last().unwrap() < first + first / 10,
            "{folds:?} vs {first}"
        );
        drop(s);
        let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
        assert_eq!(s.len(), model.len());
        for (k, v) in model.iter().step_by(13) {
            assert_eq!(s.get(PageKey(*k)).unwrap().as_ref(), Some(v));
        }
        let segs = names(&dir).iter().filter(|n| n.ends_with(".rts")).count();
        assert_eq!(segs, s.segment_count());
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
        // The page segment and the first fold's index segment.
        assert_eq!(segments, 2);
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
                s.segment_count() <= MAX_SMALL_SEGMENTS + 2,
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
        // Page segments, the first fold's index segment, the root and the log.
        let files = fs::read_dir(&dir).unwrap().count();
        assert!(files <= MAX_SMALL_SEGMENTS + 4, "{files} files");
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
            s.set_index_cache_blocks(2);
            // Keys 32 apart in units of 300 cross about 19 index blocks.
            let key = |k: u8| u64::from(k) * 300;
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
                        batch.put(PageKey(key(k)), &p);
                        pending.insert(key(k), Some(p));
                    }
                    Op::Delete(k) => {
                        batch.delete(PageKey(key(k)));
                        pending.insert(key(k), None);
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
                        s.set_index_cache_blocks(2);
                        batch = SaveBatch::new();
                        pending.clear();
                    }
                }
                prop_assert_eq!(s.obs_seq(), generation.1);
                prop_assert_eq!(s.len(), current.len());
                prop_assert_eq!(s.keys().count(), current.len());
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
