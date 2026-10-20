// SPDX-License-Identifier: AGPL-3.0-or-later
//! The incremental page store (SCALE.md §8, PLAN.md §4.9): pages are LZ4 with a CRC32, saves write
//! only dirty pages, and a manifest written last decides what a save contains.
//!
//! # Layout of `<world>/rtsim/`
//!
//! - `seg-<save>.rts`: one segment per save, holding every page that save wrote, back to back.
//!   A segment is written once and never modified.
//! - `manifest.rtm`: the save number, the observation sequence the pages include (A9), and for
//!   every live page the segment, offset, lengths and CRC32 where it lives.
//!
//! # Crash safety, in Deep Time's order (deeptime-format `fsutil.rs`)
//!
//! Every file is written to `<name>.tmp`, fsynced, renamed over its name, and its folder fsynced.
//! The segment goes first, the manifest last, and superseded segments are deleted only after the
//! manifest that stops referring to them is durable. So at any crash point the manifest on disk
//! names complete, CRC-checked pages of exactly one save. On open, files the manifest does not
//! name (a crashed save's segment, `.tmp` files) are removed.
//!
//! **Why segments rather than a file per page:** Deep Time writes whole folders, so it can rename
//! files in place. An incremental save cannot overwrite a page file the current manifest still
//! names, and one fsync per page would make a save of a thousand dirty regions a thousand fsyncs.
//! One new segment per save costs two fsyncs (segment, manifest) whatever the page count.
//! Segments whose live bytes fall under half are compacted: their live pages are copied, still
//! compressed and CRC-checked, into the next save's segment.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::codec::{Reader, Writer};

const SEGMENT_MAGIC: [u8; 4] = *b"RTSG";
const MANIFEST_MAGIC: [u8; 4] = *b"RTMF";
/// Bumped on any layout change. Before release a change wipes the save; there is no migration
/// (PLAN.md §4.9).
const FORMAT_VERSION: u16 = 1;
const SEGMENT_HEADER: u64 = 16;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageLoc {
    segment: u64,
    /// Offset of the stored bytes in the segment file.
    offset: u64,
    raw_len: u32,
    stored_len: u32,
    crc: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SegmentInfo {
    file_bytes: u64,
    /// Record headers plus stored bytes of the pages the manifest still points into this segment.
    live_bytes: u64,
    live_pages: u64,
}

/// What a commit did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitStats {
    pub segment_bytes: u64,
    pub manifest_bytes: u64,
    pub pages_written: u64,
    pub pages_deleted: u64,
    pub pages_compacted: u64,
    pub segments_removed: u64,
}

/// Where a commit stops, for crash tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The segment is durable; the manifest was never written.
    BeforeManifest,
    /// The manifest is durable; superseded segments were never deleted.
    BeforeCleanup,
}

pub struct PageStore {
    dir: PathBuf,
    durability: Durability,
    save_seq: u64,
    obs_seq: u64,
    pages: HashMap<PageKey, PageLoc>,
    segments: BTreeMap<u64, SegmentInfo>,
    manifest_bytes: u64,
    files: HashMap<u64, File>,
    poisoned: bool,
    fault: Option<Fault>,
}

impl PageStore {
    /// Opens (or creates) the store in `dir`, removing whatever a crashed save left behind.
    pub fn open(dir: impl Into<PathBuf>, durability: Durability) -> Result<PageStore> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(io(&dir))?;
        let mut store = PageStore {
            dir,
            durability,
            save_seq: 0,
            obs_seq: 0,
            pages: HashMap::new(),
            segments: BTreeMap::new(),
            manifest_bytes: 0,
            files: HashMap::new(),
            poisoned: false,
            fault: None,
        };
        // Only a manifest that is certainly absent means an empty store. Any other failure to read
        // it must stop here: treating it as absent would make every segment a stray to delete.
        let manifest = store.dir.join(MANIFEST_NAME);
        match fs::read(&manifest) {
            Ok(bytes) => store.parse_manifest(&manifest, &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(PageError::Io {
                    path: manifest,
                    source: e,
                });
            }
        }
        store.remove_strays()?;
        for (&seq, info) in &store.segments {
            let path = store.segment_path(seq);
            let len = fs::metadata(&path).map_err(io(&path))?.len();
            if len != info.file_bytes {
                return Err(PageError::Format(format!(
                    "{}: {len} bytes, the manifest says {}",
                    path.display(),
                    info.file_bytes
                )));
            }
        }
        Ok(store)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The number of the last durable save; 0 before the first.
    pub fn save_seq(&self) -> u64 {
        self.save_seq
    }

    /// The observation sequence the last durable save includes (A9).
    pub fn obs_seq(&self) -> u64 {
        self.obs_seq
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

    /// Bytes on disk: every live segment plus the manifest.
    pub fn disk_bytes(&self) -> u64 {
        self.segments.values().map(|s| s.file_bytes).sum::<u64>() + self.manifest_bytes
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[doc(hidden)]
    pub fn inject_fault(&mut self, fault: Option<Fault>) {
        self.fault = fault;
    }

    fn segment_path(&self, seq: u64) -> PathBuf {
        self.dir.join(format!("seg-{seq:016x}.rts"))
    }

    fn file(&mut self, seq: u64) -> Result<&mut File> {
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
            segment: loc.segment,
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

    /// Makes `batch` durable as one save that includes observations up to `obs_seq`.
    pub fn commit(&mut self, batch: SaveBatch, obs_seq: u64) -> Result<CommitStats> {
        if self.poisoned {
            return Err(PageError::Poisoned);
        }
        let seq = self.save_seq + 1;
        let mut stats = CommitStats::default();

        // Compaction candidates: segments under half live, and every small segment once there are
        // too many of them.
        let small = self
            .segments
            .values()
            .filter(|s| s.file_bytes < SMALL_SEGMENT_BYTES)
            .count();
        let merge_small = small > MAX_SMALL_SEGMENTS;
        let compact: Vec<u64> = self
            .segments
            .iter()
            .filter(|(_, s)| {
                s.live_bytes * 2 < s.file_bytes - SEGMENT_HEADER
                    || (merge_small && s.file_bytes < SMALL_SEGMENT_BYTES)
            })
            .map(|(&k, _)| k)
            .collect();
        let mut moved: Vec<(PageKey, PageLoc)> = self
            .pages
            .iter()
            .filter(|(k, l)| compact.contains(&l.segment) && !batch.changes.contains_key(k))
            .map(|(&k, &l)| (k, l))
            .collect();
        moved.sort_by_key(|(k, _)| *k);

        // Build the segment in memory. Puts and compacted pages, in key order within each group.
        let mut seg = Writer::with_capacity(4096);
        seg.raw(&SEGMENT_MAGIC);
        seg.u16(FORMAT_VERSION);
        seg.u16(0);
        seg.u64(seq);
        let mut new_locs: Vec<(PageKey, PageLoc)> = Vec::new();
        let mut push = |seg: &mut Writer, key: PageKey, raw_len: u32, crc: u32, stored: &[u8]| {
            seg.u64(key.0);
            seg.u32(raw_len);
            seg.u32(stored.len() as u32);
            seg.u32(crc);
            let offset = seg.len() as u64;
            seg.raw(stored);
            new_locs.push((
                key,
                PageLoc {
                    segment: seq,
                    offset,
                    raw_len,
                    stored_len: stored.len() as u32,
                    crc,
                },
            ));
        };
        for (key, change) in &batch.changes {
            if let Some(p) = change {
                push(&mut seg, *key, p.raw_len, p.crc, &p.stored);
                stats.pages_written += 1;
            }
        }
        for (key, loc) in &moved {
            let stored = self.read_stored(*loc)?;
            let actual = crc32fast::hash(&stored);
            if actual != loc.crc {
                return Err(PageError::Crc {
                    what: format!("page {key:?} during compaction"),
                    expected: loc.crc,
                    actual,
                });
            }
            push(&mut seg, *key, loc.raw_len, loc.crc, &stored);
            stats.pages_compacted += 1;
        }

        // 1. The segment, durable before anything names it.
        if !new_locs.is_empty() {
            let path = self.segment_path(seq);
            write_atomic(&path, seg.as_bytes(), self.durability)?;
            stats.segment_bytes = seg.len() as u64;
        }

        // From here the in-memory index changes; a failure poisons the store.
        self.poisoned = true;
        for (key, change) in &batch.changes {
            if change.is_none() && self.unlink(*key) {
                stats.pages_deleted += 1;
            }
        }
        if !new_locs.is_empty() {
            self.segments.insert(
                seq,
                SegmentInfo {
                    file_bytes: seg.len() as u64,
                    ..SegmentInfo::default()
                },
            );
        }
        for (key, loc) in new_locs {
            self.unlink(key);
            let s = self.segments.get_mut(&seq).expect("inserted above");
            s.live_bytes += RECORD_HEADER + u64::from(loc.stored_len);
            s.live_pages += 1;
            self.pages.insert(key, loc);
        }
        self.save_seq = seq;
        self.obs_seq = obs_seq;

        if self.fault == Some(Fault::BeforeManifest) {
            return Err(PageError::Injected);
        }

        // Segments with no live page leave the index now, so the manifest stops naming them;
        // their files go only after the manifest is durable.
        let dead: Vec<u64> = self
            .segments
            .iter()
            .filter(|(_, s)| s.live_pages == 0)
            .map(|(&k, _)| k)
            .collect();
        for seq in &dead {
            self.segments.remove(seq);
            self.files.remove(seq);
        }

        // 2. The manifest, last.
        let manifest = self.encode_manifest();
        let path = self.dir.join(MANIFEST_NAME);
        write_atomic(&path, &manifest, self.durability)?;
        self.manifest_bytes = manifest.len() as u64;
        stats.manifest_bytes = self.manifest_bytes;

        if self.fault == Some(Fault::BeforeCleanup) {
            return Err(PageError::Injected);
        }

        // 3. Superseded segments, now that nothing durable names them.
        for seq in dead {
            let path = self.segment_path(seq);
            fs::remove_file(&path).map_err(io(&path))?;
            stats.segments_removed += 1;
        }
        self.poisoned = false;
        Ok(stats)
    }

    /// Drops a key from the index and its segment's live counts. Returns whether it existed.
    fn unlink(&mut self, key: PageKey) -> bool {
        let Some(old) = self.pages.remove(&key) else {
            return false;
        };
        if let Some(s) = self.segments.get_mut(&old.segment) {
            s.live_bytes -= RECORD_HEADER + u64::from(old.stored_len);
            s.live_pages -= 1;
        }
        true
    }

    fn encode_manifest(&self) -> Vec<u8> {
        let mut body = Writer::with_capacity(64 + self.pages.len() * 12);
        body.u64(self.save_seq);
        body.u64(self.obs_seq);
        body.var_u64(self.segments.len() as u64);
        for (&seq, s) in &self.segments {
            body.var_u64(seq);
            body.var_u64(s.file_bytes);
        }
        let mut pages: Vec<(&PageKey, &PageLoc)> = self.pages.iter().collect();
        pages.sort_unstable_by_key(|(k, _)| **k);
        body.var_u64(pages.len() as u64);
        let mut prev = 0u64;
        for (k, l) in pages {
            body.var_u64(k.0 - prev);
            prev = k.0;
            body.var_u64(l.segment);
            body.var_u64(l.offset);
            body.var_u64(u64::from(l.raw_len));
            body.var_u64(u64::from(l.stored_len));
            body.u32(l.crc);
        }
        let body = body.into_bytes();
        let stored = lz4_flex::block::compress(&body);
        let mut out = Writer::with_capacity(stored.len() + 20);
        out.raw(&MANIFEST_MAGIC);
        out.u16(FORMAT_VERSION);
        out.u16(0);
        out.u32(u32::try_from(body.len()).expect("manifest over 4 GiB"));
        out.u32(stored.len() as u32);
        out.u32(crc32fast::hash(&stored));
        out.raw(&stored);
        out.into_bytes()
    }

    fn parse_manifest(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
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
        self.save_seq = r.u64().map_err(bad)?;
        self.obs_seq = r.u64().map_err(bad)?;
        let nseg = r.var_u64().map_err(bad)?;
        for _ in 0..nseg {
            let seq = r.var_u64().map_err(bad)?;
            let file_bytes = r.var_u64().map_err(bad)?;
            self.segments.insert(
                seq,
                SegmentInfo {
                    file_bytes,
                    ..SegmentInfo::default()
                },
            );
        }
        let npages = r.var_u64().map_err(bad)?;
        let mut key = 0u64;
        for _ in 0..npages {
            key += r.var_u64().map_err(bad)?;
            let loc = PageLoc {
                segment: r.var_u64().map_err(bad)?,
                offset: r.var_u64().map_err(bad)?,
                raw_len: r.var_u64().map_err(bad)? as u32,
                stored_len: r.var_u64().map_err(bad)? as u32,
                crc: r.u32().map_err(bad)?,
            };
            let s = self.segments.get_mut(&loc.segment).ok_or_else(|| {
                PageError::Format(format!(
                    "page {key:#x} names unknown segment {}",
                    loc.segment
                ))
            })?;
            s.live_bytes += RECORD_HEADER + u64::from(loc.stored_len);
            s.live_pages += 1;
            self.pages.insert(PageKey(key), loc);
        }
        r.finish().map_err(bad)?;
        self.manifest_bytes = bytes.len() as u64;
        Ok(())
    }

    /// Removes `.tmp` files and segments the manifest does not name. Only touches names this
    /// store writes.
    fn remove_strays(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.dir).map_err(io(&self.dir))?;
        for entry in entries {
            let entry = entry.map_err(io(&self.dir))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let stray = if name.ends_with(".tmp")
                && (name.starts_with("seg-") || name.starts_with("manifest"))
            {
                true
            } else if let Some(hex) = name
                .strip_prefix("seg-")
                .and_then(|n| n.strip_suffix(".rts"))
            {
                match u64::from_str_radix(hex, 16) {
                    Ok(seq) => !self.segments.contains_key(&seq),
                    Err(_) => false,
                }
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
        // The crashed save's segment was removed as a stray.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
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
        // The superseded segment outlived the crash, but the new manifest does not name it, so
        // opening removed it as a stray.
        assert_eq!(s.segment_count(), 1);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
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
        let files = fs::read_dir(&dir).unwrap().count();
        assert!(files <= MAX_SMALL_SEGMENTS + 2, "{files} files");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[derive(Clone, Debug)]
    enum Op {
        Put(u8, u16),
        Delete(u8),
        Commit,
        Reopen,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            6 => (any::<u8>(), 0u16..3000).prop_map(|(k, n)| Op::Put(k % 32, n)),
            2 => any::<u8>().prop_map(|k| Op::Delete(k % 32)),
            3 => Just(Op::Commit),
            1 => Just(Op::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]
        /// The store agrees with a map of committed pages through puts, deletes, commits,
        /// compactions and reopens; uncommitted batches vanish on reopen.
        #[test]
        fn matches_a_model(ops in prop::collection::vec(op(), 1..60)) {
            let dir = tmpdir("model");
            let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
            let mut committed: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
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
                    Op::Commit => {
                        obs += 1;
                        s.commit(std::mem::take(&mut batch), obs).unwrap();
                        for (k, v) in std::mem::take(&mut pending) {
                            match v {
                                Some(p) => { committed.insert(k, p); }
                                None => { committed.remove(&k); }
                            }
                        }
                    }
                    Op::Reopen => {
                        drop(s);
                        s = PageStore::open(&dir, Durability::NoSync).unwrap();
                        batch = SaveBatch::new();
                        pending.clear();
                        prop_assert_eq!(s.obs_seq(), obs);
                    }
                }
                prop_assert_eq!(s.len(), committed.len());
                for (k, v) in &committed {
                    let got = s.get(PageKey(*k)).unwrap();
                    prop_assert_eq!(got.as_ref(), Some(v));
                }
            }
            drop(s);
            fs::remove_dir_all(&dir).unwrap();
        }
    }
}
