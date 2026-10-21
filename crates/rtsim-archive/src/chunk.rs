// SPDX-License-Identifier: AGPL-3.0-or-later
//! A chunk file (VIEWER.md §10.3): up to [`ROWS_PER_CHUNK`] rows sorted by tick, compressed with LZ4
//! as the page store compresses pages, with a header the index copies so the timeline's histogram
//! reads headers only.
//!
//! Layout: `RTAC`, a version byte, the header's length (u32), the header, a CRC32 of everything so
//! far, the body's raw length (u32), a CRC32 of the stored body, and the stored body. The body is
//! the rows (32 bytes each, or 16 for the count tiers, [`crate::counts`]) and then the payload side
//! block: a count, then each payload's row index and bytes (always empty in the count tiers). The
//! header has its own checksum so it can be read without the body.

use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};

use crate::counts::{COUNT_BYTES, Count, RawCount};
use crate::row::{self, PAYLOAD, ROW_BYTES, RawRow};

/// Rows a chunk holds at most (TUNABLE).
pub const ROWS_PER_CHUNK: usize = 4096;
/// A header longer than this is damage (a header with every kind is under 400 KiB).
pub const MAX_HEADER_BYTES: usize = 1 << 20;
/// The bytes before a chunk's header.
pub const PREFIX_BYTES: usize = 4 + 1 + 4;

const MAGIC: &[u8; 4] = b"RTAC";
const VERSION: u8 = 1;

/// Which tier a chunk belongs to (§10.4). The count tier is two kinds of chunk, fine and coarse,
/// so compaction knows which to coarsen from the index alone; they share the tier's budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Tier {
    /// Every row.
    #[default]
    Full = 0,
    /// Rows at or above notable severity, named movers' legs among them.
    Notable = 1,
    /// Count rows per 512-block region per game day.
    Counts = 2,
    /// Count rows per 4,096-block square per 7 game days.
    Coarse = 3,
}

impl Tier {
    fn from_u8(v: u8) -> Result<Tier> {
        match v {
            0 => Ok(Tier::Full),
            1 => Ok(Tier::Notable),
            2 => Ok(Tier::Counts),
            3 => Ok(Tier::Coarse),
            _ => Err(CodecError::Invalid(format!("tier {v}"))),
        }
    }

    /// A count tier's chunk: 16-byte count rows, not events and legs.
    pub fn is_counts(self) -> bool {
        matches!(self, Tier::Counts | Tier::Coarse)
    }

    /// The tier whose budget share the chunk counts against: 0 full, 1 notable, 2 counts.
    pub fn share(self) -> usize {
        match self {
            Tier::Full => 0,
            Tier::Notable => 1,
            Tier::Counts | Tier::Coarse => 2,
        }
    }

    /// The size of the chunk's rows.
    pub fn row_bytes(self) -> usize {
        if self.is_counts() {
            COUNT_BYTES
        } else {
            ROW_BYTES
        }
    }
}

/// The save a chunk was flushed with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// The observation sequence the save's generation includes (`Generation::obs_seq`, §10.5).
    pub obs_seq: u64,
    /// The game tick at the save's snapshot.
    pub tick: u64,
    /// The wall clock at the flush, in milliseconds since the Unix epoch.
    pub wall_ms: u64,
}

/// What the index keeps of a chunk. In the count tiers `rows` is count rows, the ticks are the
/// buckets' (a bucket's first tick, the latest bucket's last tick as `last_end`), and `counts` is
/// the rows counted, so the histogram reads every tier alike.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChunkHeader {
    pub tier: Tier,
    pub rows: u32,
    /// The first row's tick.
    pub first_tick: u64,
    /// The last row's tick.
    pub last_tick: u64,
    /// The latest tick a row covers: a leg's end can pass `last_tick`, so a chunk may hold a leg
    /// covering tick `t` only when `first_tick <= t <= last_end`.
    pub last_end: u64,
    /// The observation sequence of the save the rows were flushed with (§10.5).
    pub obs_seq: u64,
    /// The game tick at that save's snapshot: with `wall_ms`, a point pairing game time with dates.
    pub save_tick: u64,
    /// The wall clock at the flush, in milliseconds since the Unix epoch.
    pub wall_ms: u64,
    /// min x, min z, max x, max z over every point the rows touch.
    pub bbox: [i32; 4],
    /// Rows per frame and kind, ascending by (frame, kind).
    pub counts: Vec<(u8, u16, u32)>,
    /// Bytes in the payload side block.
    pub payload_bytes: u32,
}

impl ChunkHeader {
    /// The header of rows sorted by tick.
    fn of(tier: Tier, rows: &[RawRow], payload_bytes: usize, stamp: Stamp) -> ChunkHeader {
        let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let mut counts: std::collections::BTreeMap<(u8, u16), u32> = Default::default();
        let mut last_end = 0;
        for r in rows {
            for (x, z) in row::points(r) {
                bbox = [
                    bbox[0].min(x),
                    bbox[1].min(z),
                    bbox[2].max(x),
                    bbox[3].max(z),
                ];
            }
            *counts.entry((row::frame(r), row::kind(r))).or_default() += 1;
            last_end = last_end.max(row::end(r));
        }
        if rows.is_empty() {
            bbox = [0; 4];
        }
        ChunkHeader {
            tier,
            rows: rows.len() as u32,
            first_tick: rows.first().map_or(0, row::tick),
            last_tick: rows.last().map_or(0, row::tick),
            last_end,
            obs_seq: stamp.obs_seq,
            save_tick: stamp.tick,
            wall_ms: stamp.wall_ms,
            bbox,
            counts: counts.into_iter().map(|((f, k), n)| (f, k, n)).collect(),
            payload_bytes: payload_bytes as u32,
        }
    }

    /// The header of count rows sorted by key.
    fn of_counts(tier: Tier, counts: &[Count], stamp: Stamp) -> ChunkHeader {
        let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let mut per: std::collections::BTreeMap<(u8, u16), u32> = Default::default();
        let mut last_end = 0;
        for c in counts {
            let b = c.bbox();
            bbox = [
                bbox[0].min(b[0]),
                bbox[1].min(b[1]),
                bbox[2].max(b[2]),
                bbox[3].max(b[3]),
            ];
            let n = per.entry((c.frame, c.kind)).or_default();
            *n = n.saturating_add(u32::from(c.count));
            last_end = last_end.max(c.last_tick());
        }
        if counts.is_empty() {
            bbox = [0; 4];
        }
        ChunkHeader {
            tier,
            rows: counts.len() as u32,
            first_tick: counts.first().map_or(0, Count::first_tick),
            last_tick: counts.last().map_or(0, Count::first_tick),
            last_end,
            obs_seq: stamp.obs_seq,
            save_tick: stamp.tick,
            wall_ms: stamp.wall_ms,
            bbox,
            counts: per.into_iter().map(|((f, k), n)| (f, k, n)).collect(),
            payload_bytes: 0,
        }
    }

    /// The save the chunk was flushed with.
    pub fn stamp(&self) -> Stamp {
        Stamp {
            obs_seq: self.obs_seq,
            tick: self.save_tick,
            wall_ms: self.wall_ms,
        }
    }

    /// Rows counted, every kind (in a count tier, the rows its count rows count).
    pub fn counted(&self) -> u64 {
        self.counts.iter().map(|c| u64::from(c.2)).sum()
    }

    /// Whether the chunk may hold rows touching ticks in `t0..=t1`.
    pub fn overlaps(&self, t0: u64, t1: u64) -> bool {
        self.first_tick <= t1 && self.last_end >= t0
    }

    /// Rows of one kind, in every frame.
    pub fn count(&self, kind: u16) -> u32 {
        self.counts
            .iter()
            .filter(|c| c.1 == kind)
            .map(|c| c.2)
            .sum()
    }

    /// The frames present, ascending.
    pub fn frames(&self) -> Vec<u8> {
        let mut f: Vec<u8> = self.counts.iter().map(|c| c.0).collect();
        f.dedup();
        f
    }

    pub(crate) fn write(&self, w: &mut Writer) {
        w.u8(self.tier as u8);
        w.var_u64(u64::from(self.rows));
        w.var_u64(self.first_tick);
        w.var_u64(self.last_tick - self.first_tick);
        w.var_u64(self.last_end - self.first_tick);
        w.var_u64(self.obs_seq);
        w.var_u64(self.save_tick);
        w.var_u64(self.wall_ms);
        for v in self.bbox {
            w.var_i64(i64::from(v));
        }
        w.var_u64(self.counts.len() as u64);
        let mut prev = 0u32;
        for &(f, k, n) in &self.counts {
            let key = u32::from(f) << 16 | u32::from(k);
            w.var_u64(u64::from(key - prev));
            w.var_u64(u64::from(n));
            prev = key;
        }
        w.var_u64(u64::from(self.payload_bytes));
    }

    pub(crate) fn read(r: &mut Reader) -> Result<ChunkHeader> {
        let bad = |what: &str| CodecError::Invalid(format!("chunk header: {what}"));
        let tier = Tier::from_u8(r.u8()?)?;
        let rows = u32::try_from(r.var_u64()?).map_err(|_| bad("rows"))?;
        let first_tick = r.var_u64()?;
        let last_tick = first_tick
            .checked_add(r.var_u64()?)
            .ok_or_else(|| bad("last tick"))?;
        let last_end = first_tick
            .checked_add(r.var_u64()?)
            .ok_or_else(|| bad("last end"))?;
        let obs_seq = r.var_u64()?;
        let save_tick = r.var_u64()?;
        let wall_ms = r.var_u64()?;
        let mut bbox = [0i32; 4];
        for v in &mut bbox {
            *v = i32::try_from(r.var_i64()?).map_err(|_| bad("bbox"))?;
        }
        let n = r.len_prefix(2)?;
        let mut counts = Vec::with_capacity(n);
        let mut prev = 0u64;
        for i in 0..n {
            let key = prev + r.var_u64()?;
            if key > u64::from(u32::from(u8::MAX) << 16 | u32::from(u16::MAX))
                || (i > 0 && key == prev)
            {
                return Err(bad("frames and kinds"));
            }
            let c = u32::try_from(r.var_u64()?).map_err(|_| bad("count"))?;
            counts.push(((key >> 16) as u8, key as u16, c));
            prev = key;
        }
        let payload_bytes = u32::try_from(r.var_u64()?).map_err(|_| bad("payload bytes"))?;
        Ok(ChunkHeader {
            tier,
            rows,
            first_tick,
            last_tick,
            last_end,
            obs_seq,
            save_tick,
            wall_ms,
            bbox,
            counts,
            payload_bytes,
        })
    }
}

/// A chunk's rows and payloads, or its count rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chunk {
    pub header: ChunkHeader,
    /// Empty in the count tiers.
    pub rows: Vec<RawRow>,
    /// Row index and bytes, ascending by index; a row has the [`PAYLOAD`] flag exactly when it has
    /// an entry here.
    pub payloads: Vec<(u32, Vec<u8>)>,
    /// The count tiers' rows, ascending by key; empty otherwise.
    pub counts: Vec<Count>,
}

/// Sorts rows by tick (stable: rows of one tick keep their order) and carries each payload to its
/// row's new place.
pub fn sort_rows(rows: &mut Vec<RawRow>, payloads: &mut [(u32, Vec<u8>)]) {
    if rows.is_sorted_by_key(row::tick) {
        return;
    }
    let mut order: Vec<u32> = (0..rows.len() as u32).collect();
    order.sort_by_key(|&i| row::tick(&rows[i as usize]));
    let mut new_of = vec![0u32; rows.len()];
    for (new, &old) in order.iter().enumerate() {
        new_of[old as usize] = new as u32;
    }
    *rows = order.iter().map(|&i| rows[i as usize]).collect();
    for p in payloads.iter_mut() {
        p.0 = new_of[p.0 as usize];
    }
    payloads.sort_by_key(|p| p.0);
}

impl Chunk {
    /// A chunk of `rows` (sorted here if they are not) and payloads for some of them. A payload
    /// whose index is out of range is dropped; the [`PAYLOAD`] flag is set from the payloads.
    pub fn new(
        tier: Tier,
        mut rows: Vec<RawRow>,
        mut payloads: Vec<(u32, Vec<u8>)>,
        stamp: Stamp,
    ) -> Chunk {
        assert!(rows.len() <= u32::MAX as usize);
        assert!(!tier.is_counts(), "count rows go in Chunk::of_counts");
        payloads.retain(|p| (p.0 as usize) < rows.len());
        payloads.sort_by_key(|p| p.0);
        payloads.dedup_by_key(|p| p.0);
        sort_rows(&mut rows, &mut payloads);
        for r in &mut rows {
            r[9] &= !PAYLOAD;
        }
        for p in &payloads {
            rows[p.0 as usize][9] |= PAYLOAD;
        }
        let payload_bytes = payloads.iter().map(|p| p.1.len()).sum();
        Chunk {
            header: ChunkHeader::of(tier, &rows, payload_bytes, stamp),
            rows,
            payloads,
            counts: Vec::new(),
        }
    }

    /// A count tier's chunk (coarse when `coarse`); the rows are sorted here by key.
    pub fn of_counts(coarse: bool, mut counts: Vec<Count>, stamp: Stamp) -> Chunk {
        counts.sort_by_key(Count::key);
        let tier = if coarse { Tier::Coarse } else { Tier::Counts };
        Chunk {
            header: ChunkHeader::of_counts(tier, &counts, stamp),
            rows: Vec::new(),
            payloads: Vec::new(),
            counts,
        }
    }

    /// A row's payload.
    pub fn payload(&self, row: usize) -> Option<&[u8]> {
        let i = self
            .payloads
            .binary_search_by_key(&(row as u32), |p| p.0)
            .ok()?;
        Some(&self.payloads[i].1)
    }

    /// The file's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut h = Writer::new();
        self.header.write(&mut h);
        let mut w = Writer::with_capacity(PREFIX_BYTES + h.len() + 12);
        w.raw(MAGIC);
        w.u8(VERSION);
        w.u32(h.len() as u32);
        w.raw(h.as_bytes());
        let crc = crc32fast::hash(w.as_bytes());
        w.u32(crc);
        let mut body = Writer::with_capacity(
            self.rows.len() * ROW_BYTES
                + self.counts.len() * COUNT_BYTES
                + self.header.payload_bytes as usize
                + 8,
        );
        for r in &self.rows {
            body.raw(r);
        }
        for c in &self.counts {
            body.raw(&c.encode());
        }
        body.var_u64(self.payloads.len() as u64);
        for (i, p) in &self.payloads {
            body.var_u64(u64::from(*i));
            body.bytes(p);
        }
        let stored = lz4_flex::block::compress(body.as_bytes());
        w.u32(body.len() as u32);
        w.u32(crc32fast::hash(&stored));
        w.raw(&stored);
        w.into_bytes()
    }

    /// The header and how many bytes the file's start up to the body's lengths takes. `bytes` may be
    /// only the file's start; [`header_len`] says how much is needed.
    pub fn decode_header(bytes: &[u8]) -> Result<(ChunkHeader, usize)> {
        let len = header_len(bytes)?;
        let end = PREFIX_BYTES + len;
        let mut r = Reader::new(bytes);
        let all = r.take(end + 4)?;
        let (covered, crc) = all.split_at(end);
        if crc32fast::hash(covered).to_le_bytes() != crc {
            return Err(CodecError::Invalid("chunk header checksum mismatch".into()));
        }
        let mut h = Reader::new(&covered[PREFIX_BYTES..]);
        let header = ChunkHeader::read(&mut h)?;
        h.finish()?;
        Ok((header, end + 4))
    }

    pub fn decode(bytes: &[u8]) -> Result<Chunk> {
        let (header, at) = Chunk::decode_header(bytes)?;
        let mut r = Reader::new(&bytes[at..]);
        let raw_len = r.u32()? as usize;
        let crc = r.u32()?;
        let stored = r.take(r.remaining())?;
        if crc32fast::hash(stored).to_le_bytes() != crc.to_le_bytes() {
            return Err(CodecError::Invalid("chunk body checksum mismatch".into()));
        }
        let row_bytes = header.tier.row_bytes();
        if header.tier.is_counts() && header.payload_bytes != 0 {
            return Err(CodecError::Invalid("payloads in a count chunk".into()));
        }
        let need = header.rows as usize * row_bytes + header.payload_bytes as usize;
        if raw_len < need || raw_len > need + 16 * (header.rows as usize + 1) {
            return Err(CodecError::Invalid(format!(
                "chunk body of {raw_len} bytes for its header"
            )));
        }
        let body = lz4_flex::block::decompress(stored, raw_len)
            .map_err(|e| CodecError::Invalid(format!("chunk body: {e}")))?;
        // The body's checksum covers the stored bytes, not the length beside them.
        if body.len() != raw_len {
            return Err(CodecError::Invalid(format!(
                "chunk body of {} bytes, not {raw_len}",
                body.len()
            )));
        }
        let mut b = Reader::new(&body);
        let stored_rows = b.take(header.rows as usize * row_bytes)?;
        let (rows, counts): (Vec<RawRow>, Vec<Count>) = if header.tier.is_counts() {
            let counts: Vec<Count> = stored_rows
                .as_chunks::<COUNT_BYTES>()
                .0
                .iter()
                .map(|c: &RawCount| Count::decode(c))
                .collect();
            if !counts.is_sorted_by_key(Count::key) {
                return Err(CodecError::Invalid("count rows out of order".into()));
            }
            (Vec::new(), counts)
        } else {
            (stored_rows.as_chunks::<ROW_BYTES>().0.to_vec(), Vec::new())
        };
        let n = b.len_prefix(2)?;
        let mut payloads = Vec::with_capacity(n);
        for _ in 0..n {
            let i = b.var_u64()?;
            if i >= rows.len() as u64
                || payloads
                    .last()
                    .is_some_and(|p: &(u32, Vec<u8>)| u64::from(p.0) >= i)
            {
                return Err(CodecError::Invalid("payload row index".into()));
            }
            payloads.push((i as u32, b.bytes()?.to_vec()));
        }
        b.finish()?;
        let chunk = Chunk {
            header,
            rows,
            payloads,
            counts,
        };
        if chunk.payloads.iter().map(|p| p.1.len()).sum::<usize>()
            != chunk.header.payload_bytes as usize
        {
            return Err(CodecError::Invalid(
                "payload bytes disagree with the header".into(),
            ));
        }
        Ok(chunk)
    }
}

/// A chunk's header length from the file's first [`PREFIX_BYTES`] bytes: the header and its
/// checksum end at `PREFIX_BYTES + len + 4`.
pub fn header_len(prefix: &[u8]) -> Result<usize> {
    let mut r = Reader::new(prefix);
    if r.take(4)? != MAGIC {
        return Err(CodecError::Invalid("not an archive chunk".into()));
    }
    let v = r.u8()?;
    if v != VERSION {
        return Err(CodecError::Invalid(format!("archive chunk version {v}")));
    }
    let len = r.u32()? as usize;
    if len > MAX_HEADER_BYTES {
        return Err(CodecError::Invalid(format!(
            "a chunk header of {len} bytes"
        )));
    }
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{Event, Handle, Leg, Row, Severity};

    fn ev(tick: u64, kind: u16, x: i32, sev: Severity) -> RawRow {
        Row::Event(Event {
            tick,
            kind,
            x,
            z: -x,
            severity: sev,
            subject: Handle::direct(tick),
            ..Event::default()
        })
        .encode()
    }

    #[test]
    fn a_chunk_reads_back_with_its_header() {
        let leg = Row::Leg(Leg {
            t0: 50,
            duration: 1_000,
            kind: 9,
            frame: 2,
            x0: -100,
            z0: 0,
            x1: 0,
            z1: 400,
            ..Leg::default()
        })
        .encode();
        // Out of order: the chunk sorts, carrying the payload to its row.
        let rows = vec![
            ev(300, 1, 5, Severity::Major),
            ev(100, 1, -7, Severity::Routine),
            leg,
            ev(200, 4, 1, Severity::Notable),
        ];
        let c = Chunk::new(
            Tier::Full,
            rows,
            vec![(0, b"loot".to_vec()), (9, b"gone".to_vec())],
            Stamp {
                obs_seq: 77,
                tick: 400,
                wall_ms: 1_700_000_000_000,
            },
        );
        assert_eq!(
            c.rows.iter().map(row::tick).collect::<Vec<_>>(),
            [50, 100, 200, 300]
        );
        assert_eq!(c.payloads, [(3, b"loot".to_vec())]);
        assert_eq!(c.payload(3), Some(&b"loot"[..]));
        assert_eq!(c.payload(2), None);
        assert_ne!(row::flags(&c.rows[3]) & PAYLOAD, 0);
        let h = &c.header;
        assert_eq!(
            (
                h.rows,
                h.first_tick,
                h.last_tick,
                h.last_end,
                h.obs_seq,
                h.save_tick
            ),
            (4, 50, 300, 1_050, 77, 400)
        );
        assert_eq!(h.bbox, [-100, -5, 5, 400]);
        assert_eq!(h.frames(), [0, 2]);
        assert_eq!(h.counts, [(0, 1, 2), (0, 4, 1), (2, 9, 1)]);
        assert_eq!((h.count(1), h.count(2)), (2, 0));
        assert!(h.overlaps(1_000, 2_000) && !h.overlaps(1_051, 2_000) && !h.overlaps(0, 49));
        let bytes = c.encode();
        assert_eq!(Chunk::decode(&bytes).unwrap(), c);
        let (head, at) = Chunk::decode_header(&bytes).unwrap();
        assert_eq!(&head, h);
        assert_eq!(at, PREFIX_BYTES + header_len(&bytes).unwrap() + 4);
        // The header alone is enough for the header.
        assert_eq!(Chunk::decode_header(&bytes[..at]).unwrap().0, head);
    }

    #[test]
    fn damage_anywhere_is_an_error() {
        let rows = (0..300)
            .map(|i| ev(i * 7, (i % 5) as u16, i as i32, Severity::Minor))
            .collect();
        let bytes = Chunk::new(
            Tier::Notable,
            rows,
            vec![(4, vec![1, 2, 3])],
            Stamp::default(),
        )
        .encode();
        for cut in 0..bytes.len() {
            assert!(Chunk::decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        for i in 0..bytes.len() {
            let mut b = bytes.clone();
            b[i] ^= 0x10;
            assert!(Chunk::decode(&b).is_err(), "flip at {i}");
        }
        let mut huge = bytes.clone();
        huge[5..9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Chunk::decode_header(&huge).is_err());
    }

    #[test]
    fn a_count_chunk_reads_back_and_counts_what_it_counted() {
        use crate::counts::Count;
        let counts = vec![
            Count {
                day: 9,
                kind: 4,
                frame: 1,
                rx: 2,
                count: 7,
                magnitude: 3,
                ..Count::default()
            },
            Count {
                day: 2,
                kind: 4,
                rx: -1,
                rz: 3,
                count: 40,
                ..Count::default()
            },
        ];
        let c = Chunk::of_counts(false, counts, Stamp::default());
        assert_eq!(c.counts[0].day, 2);
        let h = &c.header;
        assert_eq!((h.tier, h.rows, h.counted()), (Tier::Counts, 2, 47));
        assert_eq!(
            (h.first_tick, h.last_tick, h.last_end),
            (48_000, 216_000, 240_000 - 1)
        );
        assert_eq!(h.bbox, [-512, 0, 1_535, 2_047]);
        assert_eq!(h.counts, [(0, 4, 40), (1, 4, 7)]);
        let bytes = c.encode();
        assert_eq!(Chunk::decode(&bytes).unwrap(), c);
        for cut in 0..bytes.len() {
            assert!(Chunk::decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let coarse = Chunk::of_counts(true, vec![], Stamp::default());
        assert_eq!(coarse.header.tier, Tier::Coarse);
        assert_eq!(Chunk::decode(&coarse.encode()).unwrap(), coarse);
        assert!(Tier::Coarse.is_counts() && !Tier::Notable.is_counts());
    }

    #[test]
    fn an_empty_chunk_round_trips() {
        let c = Chunk::new(Tier::Full, vec![], vec![], Stamp::default());
        assert_eq!(Chunk::decode(&c.encode()).unwrap(), c);
        assert_eq!(c.header.bbox, [0; 4]);
    }
}
