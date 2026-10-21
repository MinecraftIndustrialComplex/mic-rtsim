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
//!   the save serial of the generation it selected ([`Archive::cut_above_serial`]). Saves with no
//!   journaled record between them share an `obs_seq`, so only the serial tells a rolled-back
//!   save's chunks from the confirmed one's. A copy of a save that knows only the generation's
//!   `obs_seq` cuts by that ([`Archive::cut_above`]), which keeps those same-`obs_seq` chunks until
//!   the engine's own start cuts them. The reader never cuts: it ignores chunks above the
//!   generation it reads ([`ArchiveReader::open`]).
//! - **Damage.** An unreadable index is rebuilt from the chunk files' own headers. An unreadable
//!   dictionary would make every kind name a guess, so the writer clears the archive and says so
//!   ([`Archive::was_reset`]); the reader shows kinds by number.
//! - **Budget** (§10.4). A flush writes the full tier, and drops its lowest-severity rows (counted)
//!   rather than pass the tier's share. After a save, [`Archive::compact`] moves the oldest chunks
//!   down a tier when a tier is near its share or past its horizon: full rows at notable and above
//!   go to the notable tier and the rest are counted, notable rows are counted, fine counts are
//!   coarsened, and the oldest coarse counts go. It also merges small chunks (a quiet world writes a
//!   tiny chunk a save). It reads and writes at most [`Budget::work`] bytes a save and never touches
//!   a chunk whose serial is above the confirmed generation's, which a rollback may still cut.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rtsim_kernel::codec::{CodecError, Reader, Writer};

use crate::chunk::{
    Chunk, ChunkHeader, PREFIX_BYTES, ROWS_PER_CHUNK, Stamp, Tier, header_len, sort_rows,
};
use crate::counts::{Count, TICKS_PER_DAY, Tally};
use crate::dict::{Dictionary, check_crc};
use crate::pending::{Batch, Dropped};
use crate::row::{self, Handle, Leg, RawRow, Row, Severity};

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

/// The archive's size limits (§10.4, VWD-4; every field TUNABLE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Bytes of chunk files in all, every tier.
    pub total: u64,
    /// Each tier's share of `total` in percent: full, notable, counts.
    pub shares: [u64; 3],
    /// Game days the full tier keeps at most.
    pub full_days: u64,
    /// Game days the notable tier keeps at most.
    pub notable_days: u64,
    /// Bytes compaction reads and writes a save at most (as the page store's `COMPACT_MAX_BYTES`).
    pub work: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            total: 256 << 20,
            shares: [50, 35, 15],
            full_days: 504,
            notable_days: 7_200,
            work: 4 << 20,
        }
    }
}

impl Budget {
    /// A tier's share in bytes (0 full, 1 notable, 2 counts).
    pub fn share(&self, tier: usize) -> u64 {
        self.total / 100 * self.shares[tier] + self.total % 100 * self.shares[tier] / 100
    }

    /// The level compaction keeps a tier under: its share less room for what the next save brings
    /// (an eighth of the share, at most a save's compaction work).
    pub fn target(&self, tier: usize) -> u64 {
        let s = self.share(tier);
        s - (s / 8).min(self.work)
    }
}

/// Bytes of chunk files per tier share: full, notable, counts.
pub fn tier_bytes(entries: &[Entry]) -> [u64; 3] {
    let mut b = [0; 3];
    for e in entries {
        b[e.header.tier.share()] += e.bytes;
    }
    b
}

/// What one compaction did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Compaction {
    /// Chunk bytes read and written.
    pub read: u64,
    pub written: u64,
    /// Chunks moved down a tier, from the full and from the notable tier.
    pub demoted: [u32; 2],
    /// Fine count chunks coarsened.
    pub coarsened: u32,
    /// Coarse count chunks dropped as the oldest, and the rows they counted.
    pub expired: u32,
    pub expired_rows: u64,
    /// Small chunks merged into others.
    pub merged: u32,
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
    budget: Budget,
    /// The dictionary changed and is not on disk yet (a flush that failed keeps it due).
    dict_due: bool,
    /// The latest save's tick: the horizons count back from it.
    now: u64,
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
        let now = index
            .entries
            .iter()
            .map(|e| e.header.save_tick)
            .max()
            .unwrap_or(0);
        let mut a = Archive {
            dir,
            index,
            dict,
            reset,
            budget: Budget::default(),
            dict_due: false,
            now,
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

    /// Other limits than [`Budget::default`].
    pub fn with_budget(mut self, budget: Budget) -> Archive {
        self.budget = budget;
        self
    }

    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    /// Bytes of chunk files per tier share: full, notable, counts.
    pub fn tier_bytes(&self) -> [u64; 3] {
        tier_bytes(&self.index.entries)
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

    /// Rollback (§10.5): deletes the chunks tagged above `serial`, the save serial of the
    /// generation the engine selected. Returns how many went.
    pub fn cut_above_serial(&mut self, serial: u64) -> io::Result<usize> {
        self.cut(|h| h.serial > serial)
    }

    /// Deletes the chunks tagged above `obs_seq`, a generation's observation sequence, for a copy
    /// of a save that does not know the generation's serial. Coarser than
    /// [`Archive::cut_above_serial`]: a save rolled back with no journaled record after the
    /// generation shares its `obs_seq` and stays. Returns how many went.
    pub fn cut_above(&mut self, obs_seq: u64) -> io::Result<usize> {
        self.cut(|h| h.obs_seq > obs_seq)
    }

    fn cut(&mut self, above: impl Fn(&ChunkHeader) -> bool) -> io::Result<usize> {
        let cut: Vec<u64> = self
            .index
            .entries
            .iter()
            .filter(|e| above(&e.header))
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
        self.index.entries.retain(|e| !above(&e.header));
        self.write_index()?;
        Ok(cut.len())
    }

    /// Writes one save's rows (§10.3): the dictionary if given (pass it when it changed), the rows
    /// sorted by tick in chunks of at most [`ROWS_PER_CHUNK`] stamped with the save, then the index.
    /// Returns the new chunks' sequence numbers. On the IO lane this must finish before the page
    /// commit: open's deletion of unnamed chunk files relies on it.
    ///
    /// Rows that would take the full tier past its share are dropped, the lowest severity and the
    /// oldest first, and counted. On an error the rows not written are counted as dropped too, and
    /// the dictionary stays due for the next flush, so a failed flush costs its rows and nothing
    /// after it.
    pub fn flush(
        &mut self,
        batch: Batch,
        stamp: Stamp,
        dict: Option<&Dictionary>,
    ) -> io::Result<Vec<u64>> {
        if let Some(d) = dict {
            self.dict = d.clone();
            self.dict.mark_saved();
            self.dict_due = true;
        }
        self.now = self.now.max(stamp.tick);
        let Batch {
            mut rows,
            mut payloads,
            dropped,
        } = batch;
        self.index.dropped.add(&dropped);
        if self.dict_due {
            if let Err(e) = write_atomic(&self.dir.join(DICT_FILE), &self.dict.encode()) {
                self.lose(&rows);
                return Err(e);
            }
            self.dict_due = false;
        }
        sort_rows(&mut rows, &mut payloads);
        let room = self.budget.share(0).saturating_sub(self.tier_bytes()[0]);
        let mut encoded = encode_all(chunks_of(&rows, &payloads, stamp));
        // Each pass sheds to the share the encoded bytes say fits, a little under it; a chunk's
        // fixed cost makes the last pass a guess, so after four the rows all go.
        let mut passes = 0;
        while total(&encoded) > room && !rows.is_empty() {
            passes += 1;
            let keep = if passes > 4 {
                0
            } else {
                (rows.len() as u128 * u128::from(room) / u128::from(total(&encoded)) * 15 / 16)
                    as usize
            };
            let d = shed(&mut rows, &mut payloads, keep);
            self.index.dropped.add(&d);
            encoded = encode_all(chunks_of(&rows, &payloads, stamp));
        }
        let before = self.index.entries.len();
        let written = self.write_encoded(encoded);
        let index = written.and_then(|seqs| self.write_index().map(|()| seqs));
        if index.is_err() {
            let kept: u64 = self.index.entries[before..]
                .iter()
                .map(|e| u64::from(e.header.rows))
                .sum();
            self.lose(&rows[kept as usize..]);
        }
        index
    }

    /// Counts rows that never reached a chunk file.
    fn lose(&mut self, rows: &[RawRow]) {
        for r in rows {
            self.index.dropped.by_severity[row::severity(r) as usize] += 1;
        }
    }

    fn write_encoded(&mut self, chunks: Vec<(ChunkHeader, Vec<u8>)>) -> io::Result<Vec<u64>> {
        let mut seqs = Vec::with_capacity(chunks.len());
        for (header, bytes) in chunks {
            let seq = self.index.next_seq;
            write_atomic(&self.dir.join(chunk_name(seq)), &bytes)?;
            self.index.next_seq += 1;
            self.index.entries.push(Entry {
                seq,
                bytes: bytes.len() as u64,
                header,
            });
            seqs.push(seq);
        }
        Ok(seqs)
    }

    fn write_chunks(&mut self, chunks: Vec<Chunk>) -> io::Result<Vec<u64>> {
        self.write_encoded(encode_all(chunks))
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

    /// Compaction (§10.4), after a save whose generation is confirmed at `confirmed` (the save
    /// serial of the newest generation a restart would select). In order of need: the count tier
    /// over its target (coarsen the oldest fine counts, then drop the oldest coarse ones), the
    /// notable tier over its target or past its horizon (count its oldest chunks), the full tier
    /// likewise (its oldest chunks' notable rows to the notable tier, the rest counted), then small
    /// chunks merged. Stops once [`Budget::work`] bytes are read and written; a chunk whose serial
    /// is above `confirmed` is never touched. A damaged chunk in the way is dropped (its rows are
    /// lost either way) so it cannot stall the tier.
    pub fn compact(&mut self, confirmed: u64) -> io::Result<Compaction> {
        let mut done = Compaction::default();
        let b = self.budget;
        loop {
            let spent = done.read + done.written;
            if spent >= b.work {
                break;
            }
            // At least one chunk a step, so a chunk larger than the work still moves.
            let reads = ((b.work - spent) / 2).max(1);
            let bytes = self.tier_bytes();
            if bytes[2] > b.target(2) {
                let run = self.run(Tier::Counts, confirmed, reads, |_, left| left > b.target(2));
                if !run.is_empty() {
                    self.coarsen(&run, &mut done)?;
                    continue;
                }
                let run = self.run(Tier::Coarse, confirmed, u64::MAX, |_, left| {
                    left > b.target(2)
                });
                if !run.is_empty() {
                    done.expired += run.len() as u32;
                    done.expired_rows += run.iter().map(|e| e.header.counted()).sum::<u64>();
                    let old: Vec<u64> = run.iter().map(|e| e.seq).collect();
                    self.replace(&old, vec![])?;
                    continue;
                }
            }
            let now = self.now;
            let past = |days: u64| {
                move |e: &Entry| e.header.last_end.saturating_add(days * TICKS_PER_DAY) < now
            };
            let (notable_past, full_past) = (past(b.notable_days), past(b.full_days));
            let run = self.run(Tier::Notable, confirmed, reads, |e, left| {
                left > b.target(1) || notable_past(e)
            });
            if !run.is_empty() {
                self.demote(&run, &mut done)?;
                done.demoted[1] += run.len() as u32;
                continue;
            }
            let run = self.run(Tier::Full, confirmed, reads, |e, left| {
                left > b.target(0) || full_past(e)
            });
            if !run.is_empty() {
                self.demote(&run, &mut done)?;
                done.demoted[0] += run.len() as u32;
                continue;
            }
            if let Some(run) = self.small_run(confirmed, reads) {
                self.merge(&run, &mut done)?;
                continue;
            }
            break;
        }
        Ok(done)
    }

    /// A tier's oldest chunks at or below `confirmed`, oldest first, while `need` says the next is
    /// still to go (given the chunk and the tier's bytes left without the run so far) and their
    /// bytes stay within `reads` (at least one chunk).
    fn run(
        &self,
        tier: Tier,
        confirmed: u64,
        reads: u64,
        need: impl Fn(&Entry, u64) -> bool,
    ) -> Vec<Entry> {
        let mut left = self.tier_bytes()[tier.share()];
        let mut order: Vec<&Entry> = self
            .index
            .entries
            .iter()
            .filter(|e| e.header.tier == tier && e.header.serial <= confirmed)
            .collect();
        order.sort_by_key(|e| (e.header.first_tick, e.seq));
        let mut run = Vec::new();
        let mut read = 0;
        for e in order {
            if !need(e, left) || (!run.is_empty() && read + e.bytes > reads) {
                break;
            }
            read += e.bytes;
            left = left.saturating_sub(e.bytes);
            run.push(e.clone());
        }
        run
    }

    /// The chunks of a run that read cleanly, the bytes read counted; a damaged one is left in the
    /// run's old list, so the replacement deletes it.
    fn read_run(&self, run: &[Entry], done: &mut Compaction) -> io::Result<Vec<Chunk>> {
        let mut out = Vec::with_capacity(run.len());
        for e in run {
            done.read += e.bytes;
            match self.read(e.seq) {
                Ok(c) => out.push(c),
                Err(err) if matches!(err.kind(), io::ErrorKind::InvalidData) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        Ok(out)
    }

    fn replace_run(
        &mut self,
        run: &[Entry],
        new: Vec<Chunk>,
        done: &mut Compaction,
    ) -> io::Result<()> {
        let encoded = encode_all(new);
        done.written += total(&encoded);
        let old: Vec<u64> = run.iter().map(|e| e.seq).collect();
        let seqs = self.write_encoded(encoded)?;
        let gone: HashSet<u64> = old.iter().copied().collect();
        self.index.entries.retain(|e| !gone.contains(&e.seq));
        self.write_index()?;
        for &seq in &old {
            match fs::remove_file(self.dir.join(chunk_name(seq))) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        debug_assert!(seqs.iter().all(|s| !gone.contains(s)));
        Ok(())
    }

    /// Moves a run down a tier: from the full tier its rows at notable and above to the notable
    /// tier and the rest counted; from the notable tier every row counted.
    fn demote(&mut self, run: &[Entry], done: &mut Compaction) -> io::Result<()> {
        let chunks = self.read_run(run, done)?;
        let stamp = newest_stamp(run);
        let mut keep: Vec<RawRow> = Vec::new();
        let mut payloads: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut tally = Tally::new();
        for c in chunks {
            let notable = c.header.tier == Tier::Full;
            for (i, r) in c.rows.iter().enumerate() {
                if notable && row::severity(r) >= Severity::Notable {
                    if let Some(p) = c.payload(i) {
                        payloads.push((keep.len() as u32, p.to_vec()));
                    }
                    keep.push(*r);
                } else {
                    tally.row(r);
                }
            }
        }
        sort_rows(&mut keep, &mut payloads);
        let mut new = Vec::new();
        let mut notable = chunks_of(&keep, &payloads, stamp);
        for c in &mut notable {
            c.header.tier = Tier::Notable;
        }
        new.extend(notable);
        new.extend(count_chunks(false, tally.rows(), stamp));
        self.replace_run(run, new, done)
    }

    /// Coarsens a run of fine count chunks.
    fn coarsen(&mut self, run: &[Entry], done: &mut Compaction) -> io::Result<()> {
        let chunks = self.read_run(run, done)?;
        let mut tally = Tally::new();
        for c in &chunks {
            for n in &c.counts {
                tally.count(n, true);
            }
        }
        done.coarsened += run.len() as u32;
        let new = count_chunks(true, tally.rows(), newest_stamp(run));
        self.replace_run(run, new, done)
    }

    /// The oldest run of two or more small chunks next to each other in one tier (by first tick,
    /// none above `confirmed` between them) that fits one chunk and `reads`.
    fn small_run(&self, confirmed: u64, reads: u64) -> Option<Vec<Entry>> {
        let small = |e: &Entry| (e.header.rows as usize) < ROWS_PER_CHUNK / 2;
        for tier in [Tier::Full, Tier::Notable, Tier::Counts, Tier::Coarse] {
            let mut order: Vec<&Entry> = self
                .index
                .entries
                .iter()
                .filter(|e| e.header.tier == tier)
                .collect();
            order.sort_by_key(|e| (e.header.first_tick, e.seq));
            let mut run: Vec<Entry> = Vec::new();
            let (mut rows, mut read) = (0usize, 0u64);
            for e in order {
                let fits = rows + e.header.rows as usize <= ROWS_PER_CHUNK
                    && (run.is_empty() || read + e.bytes <= reads);
                if e.header.serial <= confirmed && small(e) && fits {
                    rows += e.header.rows as usize;
                    read += e.bytes;
                    run.push(e.clone());
                    continue;
                }
                if run.len() >= 2 {
                    return Some(run);
                }
                run.clear();
                (rows, read) = (0, 0);
                if e.header.serial <= confirmed && small(e) {
                    rows = e.header.rows as usize;
                    read = e.bytes;
                    run.push(e.clone());
                }
            }
            if run.len() >= 2 {
                return Some(run);
            }
        }
        None
    }

    /// Merges a run of small chunks of one tier into one.
    fn merge(&mut self, run: &[Entry], done: &mut Compaction) -> io::Result<()> {
        let tier = run[0].header.tier;
        let chunks = self.read_run(run, done)?;
        let stamp = newest_stamp(run);
        let new = if tier.is_counts() {
            let mut tally = Tally::new();
            for c in &chunks {
                for n in &c.counts {
                    tally.count(n, false);
                }
            }
            count_chunks(tier == Tier::Coarse, tally.rows(), stamp)
        } else {
            let mut rows = Vec::new();
            let mut payloads = Vec::new();
            for c in chunks {
                let base = rows.len() as u32;
                payloads.extend(c.payloads.into_iter().map(|(i, p)| (i + base, p)));
                rows.extend(c.rows);
            }
            sort_rows(&mut rows, &mut payloads);
            let mut out = chunks_of(&rows, &payloads, stamp);
            for c in &mut out {
                c.header.tier = tier;
            }
            out
        };
        done.merged += run.len() as u32;
        self.replace_run(run, new, done)
    }

    /// What is wrong with the folder against the index: a named chunk missing, of another size or
    /// with another header; a chunk file the index does not name. Empty when consistent.
    pub fn verify(&self) -> io::Result<Vec<String>> {
        let mut problems = Vec::new();
        let (seqs, _) = list(&self.dir)?;
        let named: HashSet<u64> = self.index.entries.iter().map(|e| e.seq).collect();
        for e in &self.index.entries {
            match read_header(&self.dir.join(chunk_name(e.seq))) {
                Ok((h, bytes)) => {
                    if bytes != e.bytes {
                        problems.push(format!("{}: {bytes} bytes, index says {}", e.seq, e.bytes));
                    }
                    if h != e.header {
                        problems.push(format!("{}: header differs from the index", e.seq));
                    }
                }
                Err(err) => problems.push(format!("{}: {err}", e.seq)),
            }
            if e.seq >= self.index.next_seq {
                problems.push(format!("{}: at or past the next sequence", e.seq));
            }
        }
        for s in seqs.iter().filter(|s| !named.contains(s)) {
            problems.push(format!("{s}: a chunk file the index does not name"));
        }
        let stored = Index::decode(&fs::read(self.dir.join(INDEX_FILE))?)
            .map_err(|e| invalid(e, &self.dir.join(INDEX_FILE)))?;
        if stored != self.index {
            problems.push("the index file differs from the archive's".into());
        }
        Ok(problems)
    }
}

/// Full-tier chunks of rows sorted by tick, at most [`ROWS_PER_CHUNK`] each; payloads go with
/// their rows.
fn chunks_of(rows: &[RawRow], payloads: &[(u32, Vec<u8>)], stamp: Stamp) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut payloads = payloads.iter().peekable();
    for (c, part) in rows.chunks(ROWS_PER_CHUNK).enumerate() {
        let base = (c * ROWS_PER_CHUNK) as u32;
        let end = base + part.len() as u32;
        let mut mine = Vec::new();
        while let Some(p) = payloads.next_if(|p| p.0 < end) {
            mine.push((p.0 - base, p.1.clone()));
        }
        chunks.push(Chunk::new(Tier::Full, part.to_vec(), mine, stamp));
    }
    chunks
}

/// Count chunks of rows sorted by key, at most [`ROWS_PER_CHUNK`] rows each.
fn count_chunks(coarse: bool, counts: Vec<Count>, stamp: Stamp) -> Vec<Chunk> {
    counts
        .chunks(ROWS_PER_CHUNK)
        .map(|part| Chunk::of_counts(coarse, part.to_vec(), stamp))
        .collect()
}

fn encode_all(chunks: Vec<Chunk>) -> Vec<(ChunkHeader, Vec<u8>)> {
    chunks
        .into_iter()
        .map(|c| {
            let bytes = c.encode();
            (c.header, bytes)
        })
        .collect()
}

fn total(encoded: &[(ChunkHeader, Vec<u8>)]) -> u64 {
    encoded.iter().map(|c| c.1.len() as u64).sum()
}

/// The stamp of a run's newest save: a chunk made from others keeps it, so its tag is never
/// below a row it holds.
fn newest_stamp(run: &[Entry]) -> Stamp {
    run.iter()
        .map(|e| e.header.stamp())
        .max_by_key(|s| (s.serial, s.obs_seq, s.tick))
        .unwrap_or_default()
}

/// Keeps `keep` rows of rows sorted by tick, dropping the lowest severity first and the oldest
/// first within it; payloads follow their rows. Returns what went.
fn shed(rows: &mut Vec<RawRow>, payloads: &mut Vec<(u32, Vec<u8>)>, keep: usize) -> Dropped {
    let mut d = Dropped::default();
    if rows.len() <= keep {
        return d;
    }
    let mut order: Vec<u32> = (0..rows.len() as u32).collect();
    order.sort_by_key(|&i| row::severity(&rows[i as usize]));
    let mut gone = vec![false; rows.len()];
    for &i in &order[..rows.len() - keep] {
        gone[i as usize] = true;
        d.by_severity[row::severity(&rows[i as usize]) as usize] += 1;
    }
    let mut new_of = vec![u32::MAX; rows.len()];
    let mut k = 0;
    for (i, g) in gone.iter().enumerate() {
        if !g {
            new_of[i] = k;
            k += 1;
        }
    }
    let mut i = 0;
    rows.retain(|_| {
        i += 1;
        !gone[i - 1]
    });
    payloads.retain_mut(|p| {
        p.0 = new_of[p.0 as usize];
        p.0 != u32::MAX
    });
    d
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
    /// rows are spread evenly over its first to last tick (a count chunk's over its buckets, to the
    /// last bucket's end), so the strip still shows years whose full rows are gone. `frame` keeps
    /// one frame's rows; `kinds` says which kinds to count. Kinds with no rows in the span are left
    /// out.
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
            let last = if h.tier.is_counts() {
                h.last_end
            } else {
                h.last_tick
            };
            if last < t0 || h.first_tick >= t1 {
                continue;
            }
            // The chunk as the ticks [first, last + 1), and its share in each bin it touches.
            let (a, b) = (h.first_tick as f64, last as f64 + 1.0);
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
        for e in self
            .entries
            .iter()
            .filter(|e| !e.header.tier.is_counts() && e.header.overlaps(t0, t1))
        {
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
            .filter(|e| !e.header.tier.is_counts() && e.header.overlaps(t0, t1))
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

    /// The count rows whose buckets touch `t0..=t1`, one frame's or every frame's: the page draws
    /// them as heat where the full rows are gone (§10.6). Fine and coarse rows both; a coarse row
    /// covers a 4,096-block square and 7 days ([`Count::bbox`], [`Count::first_tick`]).
    pub fn count_rows(&self, t0: u64, t1: u64, frame: Option<u8>) -> io::Result<Vec<Count>> {
        let mut out = Vec::new();
        for e in self
            .entries
            .iter()
            .filter(|e| e.header.tier.is_counts() && e.header.overlaps(t0, t1))
        {
            if frame.is_some_and(|f| !e.header.counts.iter().any(|c| c.0 == f)) {
                continue;
            }
            let Some(c) = self.read_or_skip(e)? else {
                continue;
            };
            out.extend(
                c.counts
                    .into_iter()
                    .filter(|n| n.overlaps(t0, t1) && frame.is_none_or(|f| n.frame == f)),
            );
        }
        Ok(out)
    }

    /// The legs covering `tick`: every mover's exact position then ([`Leg::at`]). A mover is its
    /// kind and subject, and its latest decision is where it is: a leg is passed over when the same
    /// mover has a leg that starts later but by `tick` (a replan booked before the old leg ended,
    /// even one that has itself ended). Finding those reads the chunks back to the oldest covering
    /// leg's start that hold rows of its kind. Legs with no subject are all kept. Ordered by kind,
    /// then subject.
    pub fn legs_at(&self, tick: u64) -> io::Result<Vec<Leg>> {
        let none = Handle::default();
        let mut out = Vec::new();
        // Per mover, its latest leg start by `tick`.
        let mut latest: BTreeMap<(u16, bool, u64), u64> = BTreeMap::new();
        let mut note = |l: &Leg| {
            if l.t0 <= tick && l.subject != none {
                let t = latest
                    .entry((l.kind, l.subject.dict, l.subject.value))
                    .or_default();
                *t = (*t).max(l.t0);
            }
        };
        let legs = |c: Chunk| {
            c.rows
                .iter()
                .filter_map(|r: &RawRow| match Row::decode(r) {
                    Row::Leg(l) => Some(l),
                    Row::Event(_) => None,
                })
                .collect::<Vec<_>>()
        };
        let chunks = || self.entries.iter().filter(|e| !e.header.tier.is_counts());
        for e in chunks().filter(|e| e.header.overlaps(tick, tick)) {
            let Some(c) = self.read_or_skip(e)? else {
                continue;
            };
            for l in legs(c) {
                note(&l);
                if l.covers(tick) {
                    out.push(l);
                }
            }
        }
        let named = out.iter().filter(|l| l.subject != none);
        if let Some(lo) = named.clone().map(|l| l.t0).min() {
            let kinds: BTreeSet<u16> = named.map(|l| l.kind).collect();
            // Chunks that ended before `tick` but started legs after the oldest covering one.
            for e in chunks().filter(|e| {
                !e.header.overlaps(tick, tick)
                    && e.header.last_tick > lo
                    && kinds.iter().any(|&k| e.header.count(k) > 0)
            }) {
                let Some(c) = self.read_or_skip(e)? else {
                    continue;
                };
                legs(c).iter().for_each(&mut note);
            }
        }
        out.retain(|l| {
            l.subject == none
                || latest.get(&(l.kind, l.subject.dict, l.subject.value)) == Some(&l.t0)
        });
        out.sort_by_key(|l| (l.kind, l.subject.dict, l.subject.value, l.t0));
        // Two legs of one mover from one tick: the first written stands.
        out.dedup_by(|a, b| a.subject != none && (a.kind, a.subject) == (b.kind, b.subject));
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
