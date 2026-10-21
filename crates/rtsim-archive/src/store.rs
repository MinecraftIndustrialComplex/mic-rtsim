// SPDX-License-Identifier: AGPL-3.0-or-later
//! The archive folder (VIEWER.md §10.3, §10.5): `<world>/rtsim/archive/` holds chunk files named by
//! a sequence number (`<seq as 16 hex digits>.rtac`), the dictionary (`dict.rtad`) and the index of
//! chunk headers (`index.rtai`).
//!
//! - **The index is the truth.** Every write goes to `<name>.tmp` and is renamed into place, with no
//!   fsync: the archive is a record, not the truth. A flush writes the dictionary, then its chunks,
//!   then the index; a replacement (compaction) writes its chunks, then the index, then deletes the
//!   chunks it replaced. A chunk file the index does not name is therefore either a flush that never
//!   reached its page commit (the rollback would cut it anyway) or a compaction that never finished
//!   (its rows are still in the chunks it would have replaced), and the writer deletes it on open.
//! - **Rollback.** On open, before anything new is written, the engine cuts the chunks tagged above
//!   the `obs_seq` of the generation it selected ([`Archive::cut_above`]). The reader never cuts: it
//!   ignores chunks above the generation it reads ([`ArchiveReader::open`]).
//! - **Damage.** An unreadable index is rebuilt from the chunk files' own headers. An unreadable
//!   dictionary would make every kind name a guess, so the writer clears the archive and says so
//!   ([`Archive::was_reset`]); the reader shows kinds by number.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rtsim_kernel::codec::{CodecError, Reader, Writer};

use crate::chunk::{
    Chunk, ChunkHeader, PREFIX_BYTES, ROWS_PER_CHUNK, Stamp, Tier, header_len, sort_rows,
};
use crate::dict::{Dictionary, check_crc};
use crate::pending::{Batch, Dropped};
use crate::row::{Leg, RawRow, Row};

pub const INDEX_FILE: &str = "index.rtai";
pub const DICT_FILE: &str = "dict.rtad";
const CHUNK_EXT: &str = "rtac";
const INDEX_MAGIC: &[u8; 4] = b"RTAI";
const INDEX_VERSION: u8 = 1;

/// A world's archive folder.
pub fn dir_for(world: &Path) -> PathBuf {
    world.join("rtsim").join("archive")
}

/// A chunk file's name.
pub fn chunk_name(seq: u64) -> String {
    format!("{seq:016x}.{CHUNK_EXT}")
}

fn seq_of(name: &str) -> Option<u64> {
    let stem = name.strip_suffix(&format!(".{CHUNK_EXT}"))?;
    if stem.len() != 16 {
        return None;
    }
    u64::from_str_radix(stem, 16).ok()
}

fn invalid(e: CodecError, what: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {e}", what.display()),
    )
}

/// `path.tmp`, then a rename over `path`. No fsync (§10.3).
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// One chunk in the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub seq: u64,
    /// The file's size.
    pub bytes: u64,
    pub header: ChunkHeader,
}

/// The index file's contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    /// The next chunk's sequence number.
    pub next_seq: u64,
    /// Everything dropped since the archive began.
    pub dropped: Dropped,
    /// Ascending by sequence.
    pub entries: Vec<Entry>,
}

impl Index {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(64 + self.entries.len() * 96);
        w.raw(INDEX_MAGIC);
        w.u8(INDEX_VERSION);
        w.var_u64(self.next_seq);
        let d = &self.dropped;
        for v in d.by_severity.iter().chain([&d.no_frame, &d.payloads]) {
            w.var_u64(*v);
        }
        w.var_u64(self.entries.len() as u64);
        let mut prev = 0;
        for e in &self.entries {
            w.var_u64(e.seq - prev);
            w.var_u64(e.bytes);
            e.header.write(&mut w);
            prev = e.seq;
        }
        let crc = crc32fast::hash(w.as_bytes());
        w.u32(crc);
        w.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Index, CodecError> {
        let body = check_crc(bytes, "archive index")?;
        let mut r = Reader::new(body);
        if r.take(4)? != INDEX_MAGIC || r.u8()? != INDEX_VERSION {
            return Err(CodecError::Invalid(
                "not an archive index of this version".into(),
            ));
        }
        let next_seq = r.var_u64()?;
        let mut dropped = Dropped::default();
        for v in dropped.by_severity.iter_mut() {
            *v = r.var_u64()?;
        }
        dropped.no_frame = r.var_u64()?;
        dropped.payloads = r.var_u64()?;
        let n = r.len_prefix(16)?;
        let mut entries = Vec::with_capacity(n);
        let mut prev = 0u64;
        for i in 0..n {
            let step = r.var_u64()?;
            if i > 0 && step == 0 {
                return Err(CodecError::Invalid(
                    "archive index sequence not ascending".into(),
                ));
            }
            let seq = prev
                .checked_add(step)
                .ok_or_else(|| CodecError::Invalid("sequence".into()))?;
            let bytes = r.var_u64()?;
            entries.push(Entry {
                seq,
                bytes,
                header: ChunkHeader::read(&mut r)?,
            });
            prev = seq;
        }
        r.finish()?;
        if entries.last().is_some_and(|e| e.seq >= next_seq) {
            return Err(CodecError::Invalid(
                "archive index next sequence behind its entries".into(),
            ));
        }
        Ok(Index {
            next_seq,
            dropped,
            entries,
        })
    }
}

/// A chunk file's header and size, reading only the file's start.
pub fn read_header(path: &Path) -> io::Result<(ChunkHeader, u64)> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let mut start = vec![0u8; PREFIX_BYTES];
    f.read_exact(&mut start)?;
    let len = header_len(&start).map_err(|e| invalid(e, path))?;
    start.resize(PREFIX_BYTES + len + 4, 0);
    f.read_exact(&mut start[PREFIX_BYTES..])?;
    let (h, _) = Chunk::decode_header(&start).map_err(|e| invalid(e, path))?;
    Ok((h, size))
}

/// A chunk file.
pub fn read_chunk(path: &Path) -> io::Result<Chunk> {
    Chunk::decode(&fs::read(path)?).map_err(|e| invalid(e, path))
}

/// The folder's chunk files by sequence, and its temporary files.
fn list(dir: &Path) -> io::Result<(BTreeSet<u64>, Vec<PathBuf>)> {
    let mut seqs = BTreeSet::new();
    let mut tmps = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(seq) = seq_of(name) {
            seqs.insert(seq);
        } else if name.ends_with(".tmp") {
            tmps.push(e.path());
        }
    }
    Ok((seqs, tmps))
}

/// Entries for every readable chunk file in `seqs`.
fn scan(dir: &Path, seqs: &BTreeSet<u64>) -> (Vec<Entry>, Vec<u64>) {
    let mut entries = Vec::new();
    let mut damaged = Vec::new();
    for &seq in seqs {
        match read_header(&dir.join(chunk_name(seq))) {
            Ok((header, bytes)) => entries.push(Entry { seq, bytes, header }),
            Err(_) => damaged.push(seq),
        }
    }
    (entries, damaged)
}

/// The engine's side: the only writer of a world's archive.
#[derive(Debug)]
pub struct Archive {
    dir: PathBuf,
    index: Index,
    dict: Dictionary,
    reset: bool,
}

impl Archive {
    /// Opens the folder for writing, creating it. Temporary files and chunk files the index does not
    /// name are deleted; entries whose file is gone are dropped. Call [`Archive::cut_above`] next.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Archive> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        let (mut seqs, tmps) = list(&dir)?;
        for t in tmps {
            fs::remove_file(t)?;
        }
        let mut reset = false;
        let dict = match fs::read(dir.join(DICT_FILE)) {
            Ok(b) => match Dictionary::decode(&b) {
                Ok(d) => d,
                Err(_) => {
                    // Kind numbers in the chunks would mean nothing: start again.
                    for &seq in &seqs {
                        fs::remove_file(dir.join(chunk_name(seq)))?;
                    }
                    let _ = fs::remove_file(dir.join(INDEX_FILE));
                    fs::remove_file(dir.join(DICT_FILE))?;
                    seqs.clear();
                    reset = true;
                    Dictionary::new()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Dictionary::new(),
            Err(e) => return Err(e),
        };
        let stored = match fs::read(dir.join(INDEX_FILE)) {
            Ok(b) => Index::decode(&b).ok(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let (index, changed) = match stored {
            Some(mut index) => {
                let before = index.entries.len();
                index.entries.retain(|e| seqs.contains(&e.seq));
                let named: HashSet<u64> = index.entries.iter().map(|e| e.seq).collect();
                let mut orphans = false;
                for &seq in seqs.iter().filter(|s| !named.contains(s)) {
                    fs::remove_file(dir.join(chunk_name(seq)))?;
                    orphans = true;
                }
                let changed = orphans || index.entries.len() != before;
                (index, changed)
            }
            None => {
                let (entries, damaged) = scan(&dir, &seqs);
                for seq in damaged {
                    fs::remove_file(dir.join(chunk_name(seq)))?;
                }
                let next_seq = seqs.last().map_or(0, |s| s + 1);
                (
                    Index {
                        next_seq,
                        dropped: Dropped::default(),
                        entries,
                    },
                    !seqs.is_empty() || reset,
                )
            }
        };
        let mut a = Archive {
            dir,
            index,
            dict,
            reset,
        };
        a.index.next_seq = a.index.next_seq.max(seqs.last().map_or(0, |s| s + 1));
        if changed {
            a.write_index()?;
        }
        Ok(a)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The stored dictionary: the engine starts from it, so numbers keep their meaning.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dict
    }

    pub fn entries(&self) -> &[Entry] {
        &self.index.entries
    }

    /// Everything dropped since the archive began.
    pub fn dropped(&self) -> &Dropped {
        &self.index.dropped
    }

    /// Whether open found the dictionary damaged and cleared the archive.
    pub fn was_reset(&self) -> bool {
        self.reset
    }

    /// Bytes in chunk files.
    pub fn bytes(&self) -> u64 {
        self.index.entries.iter().map(|e| e.bytes).sum()
    }

    fn write_index(&self) -> io::Result<()> {
        write_atomic(&self.dir.join(INDEX_FILE), &self.index.encode())
    }

    /// Rollback (§10.5): deletes the chunks tagged above `obs_seq`, the observation sequence of the
    /// generation the engine selected. Returns how many went.
    pub fn cut_above(&mut self, obs_seq: u64) -> io::Result<usize> {
        let cut: Vec<u64> = self
            .index
            .entries
            .iter()
            .filter(|e| e.header.obs_seq > obs_seq)
            .map(|e| e.seq)
            .collect();
        if cut.is_empty() {
            return Ok(0);
        }
        // Files first: if this stops halfway, open drops the entries whose files are gone, and the
        // next open cuts the rest.
        for &seq in &cut {
            match fs::remove_file(self.dir.join(chunk_name(seq))) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        self.index.entries.retain(|e| e.header.obs_seq <= obs_seq);
        self.write_index()?;
        Ok(cut.len())
    }

    /// Writes one save's rows (§10.3): the dictionary if given (pass it when it changed), the rows
    /// sorted by tick in chunks of at most [`ROWS_PER_CHUNK`] stamped with the save, then the index.
    /// Returns the new chunks' sequence numbers. On the IO lane this must finish before the page
    /// commit: open's deletion of unnamed chunk files relies on it.
    pub fn flush(
        &mut self,
        batch: Batch,
        stamp: Stamp,
        dict: Option<&Dictionary>,
    ) -> io::Result<Vec<u64>> {
        if let Some(d) = dict {
            write_atomic(&self.dir.join(DICT_FILE), &d.encode())?;
            self.dict = d.clone();
            self.dict.mark_saved();
        }
        let Batch {
            mut rows,
            mut payloads,
            dropped,
        } = batch;
        self.index.dropped.add(&dropped);
        sort_rows(&mut rows, &mut payloads);
        let mut chunks = Vec::new();
        let mut payloads = payloads.into_iter().peekable();
        for (c, part) in rows.chunks(ROWS_PER_CHUNK).enumerate() {
            let base = (c * ROWS_PER_CHUNK) as u32;
            let end = base + part.len() as u32;
            let mut mine = Vec::new();
            while let Some(p) = payloads.next_if(|p| p.0 < end) {
                mine.push((p.0 - base, p.1));
            }
            chunks.push(Chunk::new(Tier::Full, part.to_vec(), mine, stamp));
        }
        let seqs = self.write_chunks(chunks)?;
        self.write_index()?;
        Ok(seqs)
    }

    fn write_chunks(&mut self, chunks: Vec<Chunk>) -> io::Result<Vec<u64>> {
        let mut seqs = Vec::with_capacity(chunks.len());
        for c in chunks {
            let seq = self.index.next_seq;
            let bytes = c.encode();
            write_atomic(&self.dir.join(chunk_name(seq)), &bytes)?;
            self.index.next_seq += 1;
            self.index.entries.push(Entry {
                seq,
                bytes: bytes.len() as u64,
                header: c.header,
            });
            seqs.push(seq);
        }
        Ok(seqs)
    }

    /// Replaces chunks with others (compaction, §10.4): the new chunks, then the index, then the old
    /// files. A crash before the index leaves the old chunks named and the new ones orphans, which
    /// open deletes; after it, the old files are orphans of nothing and open deletes them.
    pub fn replace(&mut self, old: &[u64], new: Vec<Chunk>) -> io::Result<Vec<u64>> {
        let seqs = self.write_chunks(new)?;
        let old: HashSet<u64> = old.iter().copied().collect();
        self.index.entries.retain(|e| !old.contains(&e.seq));
        self.write_index()?;
        for &seq in &old {
            match fs::remove_file(self.dir.join(chunk_name(seq))) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(seqs)
    }

    /// A chunk by sequence number.
    pub fn read(&self, seq: u64) -> io::Result<Chunk> {
        read_chunk(&self.dir.join(chunk_name(seq)))
    }
}

/// The viewer's side: reads a world's archive and never writes it.
#[derive(Debug)]
pub struct ArchiveReader {
    dir: PathBuf,
    entries: Vec<Entry>,
    dict: Dictionary,
    dropped: Dropped,
    /// Chunks [`ArchiveReader::rows`] and [`ArchiveReader::legs_at`] skipped as damaged.
    damaged: AtomicU32,
}

/// A row as read, with where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub row: Row,
    pub payload: Option<Vec<u8>>,
    pub tier: Tier,
}

impl ArchiveReader {
    /// The archive at `dir`, or `None` when there is none. Chunks tagged above `max_obs_seq` (the
    /// generation the viewer reads) are ignored. Opens files read-only and writes nothing, so it
    /// works on a read-only copy of a save.
    pub fn open(
        dir: impl Into<PathBuf>,
        max_obs_seq: Option<u64>,
    ) -> io::Result<Option<ArchiveReader>> {
        let dir = dir.into();
        let (seqs, _) = match list(&dir) {
            Ok(l) => l,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let dict = fs::read(dir.join(DICT_FILE))
            .ok()
            .and_then(|b| Dictionary::decode(&b).ok())
            .unwrap_or_default();
        let index = fs::read(dir.join(INDEX_FILE))
            .ok()
            .and_then(|b| Index::decode(&b).ok());
        let (mut entries, dropped) = match index {
            Some(i) => (
                i.entries
                    .into_iter()
                    .filter(|e| seqs.contains(&e.seq))
                    .collect(),
                i.dropped,
            ),
            None => (scan(&dir, &seqs).0, Dropped::default()),
        };
        if let Some(max) = max_obs_seq {
            entries.retain(|e: &Entry| e.header.obs_seq <= max);
        }
        Ok(Some(ArchiveReader {
            dir,
            entries,
            dict,
            dropped,
            damaged: AtomicU32::new(0),
        }))
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dict
    }

    pub fn dropped(&self) -> &Dropped {
        &self.dropped
    }

    /// How many damaged chunks reads have skipped so far (the page says so rather than failing).
    pub fn damaged(&self) -> u32 {
        self.damaged.load(Ordering::Relaxed)
    }

    /// The first and last tick any row touches.
    pub fn span(&self) -> Option<(u64, u64)> {
        let first = self.entries.iter().map(|e| e.header.first_tick).min()?;
        let last = self.entries.iter().map(|e| e.header.last_end).max()?;
        Some((first, last))
    }

    /// Rows per kind per bin over `t0..t1` in `bins` equal bins, from the headers alone: a chunk's
    /// rows are spread evenly over its first to last tick. `frame` keeps one frame's rows; `kinds`
    /// says which kinds to count. Kinds with no rows in the span are left out.
    pub fn histogram(
        &self,
        t0: u64,
        t1: u64,
        bins: usize,
        frame: Option<u8>,
        kinds: impl Fn(u16) -> bool,
    ) -> BTreeMap<u16, Vec<f64>> {
        let mut out: BTreeMap<u16, Vec<f64>> = BTreeMap::new();
        if bins == 0 || t1 <= t0 {
            return out;
        }
        let width = (t1 - t0) as f64 / bins as f64;
        for e in &self.entries {
            let h = &e.header;
            if h.last_tick < t0 || h.first_tick >= t1 {
                continue;
            }
            // The chunk as the ticks [first, last + 1), and its share in each bin it touches.
            let (a, b) = (h.first_tick as f64, h.last_tick as f64 + 1.0);
            let lo = ((a - t0 as f64) / width).floor().max(0.0) as usize;
            let hi = (((b - t0 as f64) / width).ceil() as usize).min(bins);
            let shares: Vec<(usize, f64)> = (lo..hi)
                .map(|i| {
                    let (s, t) = (
                        t0 as f64 + i as f64 * width,
                        t0 as f64 + (i + 1) as f64 * width,
                    );
                    (i, (t.min(b) - s.max(a)).max(0.0) / (b - a))
                })
                .collect();
            for &(f, k, n) in &h.counts {
                if frame.is_some_and(|want| want != f) || !kinds(k) {
                    continue;
                }
                let series = out.entry(k).or_insert_with(|| vec![0.0; bins]);
                for &(i, share) in &shares {
                    series[i] += f64::from(n) * share;
                }
            }
        }
        out
    }

    /// Game ticks paired with wall-clock milliseconds, one pair a save, ascending by tick: the page
    /// writes dates under game days from them.
    pub fn wall(&self) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = self
            .entries
            .iter()
            .map(|e| (e.header.save_tick, e.header.wall_ms))
            .collect();
        v.sort_unstable();
        v.dedup_by_key(|p| p.0);
        v
    }

    /// A chunk; `None` when its file went (a cut or a compaction since open).
    pub fn read(&self, e: &Entry) -> io::Result<Option<Chunk>> {
        match read_chunk(&self.dir.join(chunk_name(e.seq))) {
            Ok(c) => Ok(Some(c)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// [`ArchiveReader::read`], but a damaged chunk is skipped and counted: one bad file costs the
    /// page its rows, not the whole answer.
    fn read_or_skip(&self, e: &Entry) -> io::Result<Option<Chunk>> {
        match self.read(e) {
            Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                self.damaged.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            other => other,
        }
    }

    /// Every row touching `t0..=t1` (an event in it, a leg overlapping it), oldest chunk first.
    pub fn rows(&self, t0: u64, t1: u64) -> io::Result<Vec<Record>> {
        let mut out = Vec::new();
        for e in self.entries.iter().filter(|e| e.header.overlaps(t0, t1)) {
            let Some(c) = self.read_or_skip(e)? else {
                continue;
            };
            for (i, r) in c.rows.iter().enumerate() {
                if crate::row::tick(r) <= t1 && crate::row::end(r) >= t0 {
                    out.push(Record {
                        row: Row::decode(r),
                        payload: c.payload(i).map(<[u8]>::to_vec),
                        tier: e.header.tier,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The newest `limit` rows touching `t0..=t1` that `keep` accepts, newest first (by tick, a
    /// leg by its start), and whether there may be more. Reads chunks newest first and stops at the
    /// first chunk too old to hold a row among them, so a long span costs what the answer costs;
    /// a stop with chunks left counts as more.
    pub fn newest(
        &self,
        t0: u64,
        t1: u64,
        limit: usize,
        keep: impl Fn(&Row) -> bool,
    ) -> io::Result<(Vec<Record>, bool)> {
        let mut order: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| e.header.overlaps(t0, t1))
            .collect();
        order.sort_by_key(|e| std::cmp::Reverse(e.header.last_tick));
        let mut out: Vec<Record> = Vec::new();
        let mut more = false;
        for e in order {
            if out.len() == limit
                && out
                    .last()
                    .is_some_and(|r| e.header.last_tick < start(&r.row))
            {
                more = true;
                break;
            }
            let Some(c) = self.read_or_skip(e)? else {
                continue;
            };
            for (i, r) in c.rows.iter().enumerate() {
                if crate::row::tick(r) > t1 || crate::row::end(r) < t0 {
                    continue;
                }
                let row = Row::decode(r);
                if keep(&row) {
                    out.push(Record {
                        row,
                        payload: c.payload(i).map(<[u8]>::to_vec),
                        tier: e.header.tier,
                    });
                }
            }
            out.sort_by_key(|r| std::cmp::Reverse(start(&r.row)));
            if out.len() > limit {
                out.truncate(limit);
                more = true;
            }
        }
        Ok((out, more))
    }

    /// The legs covering `tick`: every mover's exact position then ([`Leg::at`]).
    pub fn legs_at(&self, tick: u64) -> io::Result<Vec<Leg>> {
        let mut out = Vec::new();
        for e in self
            .entries
            .iter()
            .filter(|e| e.header.overlaps(tick, tick))
        {
            let Some(c) = self.read_or_skip(e)? else {
                continue;
            };
            out.extend(c.rows.iter().filter_map(|r: &RawRow| match Row::decode(r) {
                Row::Leg(l) if l.covers(tick) => Some(l),
                _ => None,
            }));
        }
        Ok(out)
    }
}

/// A row's tick: an event's, a leg's start.
fn start(row: &Row) -> u64 {
    match row {
        Row::Event(e) => e.tick,
        Row::Leg(l) => l.t0,
    }
}
