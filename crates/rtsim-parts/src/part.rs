// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parts (VIEWER.md §4.3): generic rows, written by any source and read by one reader.
//!
//! # Keys
//!
//! A [`PartKey`] names a part by its kind, its layer (an index into the store's layer list), its
//! **unit** and, for things that stay put, its region. A unit is a range of the layer's own keys
//! (a node index, a handle's index, a district id): `unit = key / per_unit`, with `per_unit`
//! chosen so a unit holds at most [`MAX_ROWS`] rows. Movers are bucketed by unit alone, so a mover
//! is in one part whichever region it crosses. Things that stay put are bucketed by unit and by
//! the canonical 512-block region their mark is anchored in: encoding a unit writes one part per
//! region its rows fall in. A source encodes whole units, never part of one, so a part is never
//! split and a thing is in exactly one part.
//!
//! # Bytes
//!
//! A part is a magic, a version, its key, a count per row kind per region, and its rows. A row is
//! its kind, its length, its frame, its body (a mark's, [`crate::mark::Mark::encode_body`]) and its
//! card fields. Readers skip rows of kinds they do not know by the length, and parts of kinds they
//! do not know by the key, which is how the glass box's graph parts (V6) fit with no change here:
//! [`kind::GRAPH`] parts keyed by a district's unit (and a region, for a piece of a big graph),
//! holding [`crate::mark::kind::GRAPH_NODE`] and [`crate::mark::kind::GRAPH_EDGE`] rows whose
//! fields carry the rates.

use std::collections::BTreeMap;

use rtsim_kernel::codec::{CodecError, Reader, Writer};

use crate::mark::{Mark, string};

/// Blocks on a side of a region (mic-viewer's `scene::REGION`, Deep Time's `REGION_BLOCKS`).
pub const REGION: f64 = 512.0;

/// The most rows a unit writes (TUNABLE, VIEWER.md §5.2).
pub const MAX_ROWS: u32 = 2_000;

const MAGIC: u32 = u32::from_le_bytes(*b"MICP");
/// 2: an area mark carries rings and a state.
const VERSION: u8 = 2;

/// Part kinds. A reader skips a kind it does not know.
pub mod kind {
    /// Map marks: what every layer writes today.
    pub const MARKS: u8 = 1;
    /// Reserved for V6: a district's compiled glass-box graph, or a region of it.
    pub const GRAPH: u8 = 2;
    /// The archive's rows recorded since the last save (VIEWER.md §10.3).
    pub const ARCHIVE_ROWS: u8 = 3;
    /// The Individual level of one region (`write_samples`).
    pub const SAMPLES: u8 = 4;
}

/// A canonical region: a frame (Java's dimension id) and `floor(x / 512)`, `floor(z / 512)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionKey {
    pub frame: u32,
    pub rx: i32,
    pub rz: i32,
}

const BIAS: i64 = 1 << 23;

impl RegionKey {
    pub fn of(frame: u32, x: f64, z: f64) -> RegionKey {
        let f = |v: f64| {
            let r = (v / REGION).floor();
            if r.is_finite() {
                r.clamp(-(BIAS as f64), (BIAS - 1) as f64) as i32
            } else {
                0
            }
        };
        RegionKey {
            frame,
            rx: f(x),
            rz: f(z),
        }
    }

    /// The region as one integer (the lease's form): the frame in the top 16 bits, then `rx` and
    /// `rz` biased into 24 bits each.
    pub fn pack(self) -> u64 {
        let b = |v: i32| ((i64::from(v) + BIAS).clamp(0, (1 << 24) - 1)) as u64;
        (u64::from(self.frame & 0xffff) << 48) | (b(self.rx) << 24) | b(self.rz)
    }

    pub fn unpack(v: u64) -> RegionKey {
        let u = |b: u64| ((b & 0xff_ffff) as i64 - BIAS) as i32;
        RegionKey {
            frame: (v >> 48) as u32,
            rx: u(v >> 24),
            rz: u(v),
        }
    }

    /// The region's box in blocks: x0, z0, x1, z1.
    pub fn bounds(self) -> [f64; 4] {
        let (x, z) = (f64::from(self.rx) * REGION, f64::from(self.rz) * REGION);
        [x, z, x + REGION, z + REGION]
    }
}

/// A part's name (module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartKey {
    pub kind: u8,
    pub layer: u16,
    pub unit: u64,
    pub region: Option<RegionKey>,
}

impl PartKey {
    /// A unit of marks, before its rows are split by region.
    pub fn unit(layer: u16, unit: u64) -> PartKey {
        PartKey {
            kind: kind::MARKS,
            layer,
            unit,
            region: None,
        }
    }

    /// The unit this part belongs to (its key with no region).
    pub fn unit_key(self) -> PartKey {
        PartKey {
            region: None,
            ..self
        }
    }

    pub fn encode(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u16(self.layer);
        w.var_u64(self.unit);
        match self.region {
            Some(r) => {
                w.bool(true);
                w.var_u64(u64::from(r.frame));
                w.var_i64(i64::from(r.rx));
                w.var_i64(i64::from(r.rz));
            }
            None => w.bool(false),
        }
    }

    pub fn decode(r: &mut Reader<'_>) -> Result<PartKey, CodecError> {
        let kind = r.u8()?;
        let layer = r.u16()?;
        let unit = r.var_u64()?;
        let region = if r.bool()? {
            let frame = u32::try_from(r.var_u64()?)
                .map_err(|_| CodecError::Invalid("region frame".into()))?;
            let rx =
                i32::try_from(r.var_i64()?).map_err(|_| CodecError::Invalid("region x".into()))?;
            let rz =
                i32::try_from(r.var_i64()?).map_err(|_| CodecError::Invalid("region z".into()))?;
            Some(RegionKey { frame, rx, rz })
        } else {
            None
        };
        Ok(PartKey {
            kind,
            layer,
            unit,
            region,
        })
    }
}

/// A card field's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    I64(i64),
    F64(f64),
    Str(String),
    Bool(bool),
}

impl Value {
    fn encode(&self, w: &mut Writer) {
        match self {
            Value::I64(v) => {
                w.u8(1);
                w.var_i64(*v);
            }
            Value::F64(v) => {
                w.u8(2);
                w.f64(*v);
            }
            Value::Str(s) => {
                w.u8(3);
                w.bytes(s.as_bytes());
            }
            Value::Bool(b) => {
                w.u8(4);
                w.bool(*b);
            }
        }
    }

    fn decode(r: &mut Reader<'_>) -> Result<Value, CodecError> {
        Ok(match r.u8()? {
            1 => Value::I64(r.var_i64()?),
            2 => Value::F64(r.f64()?),
            3 => Value::Str(string(r)?),
            4 => Value::Bool(r.bool()?),
            t => return Err(CodecError::Invalid(format!("value tag {t}"))),
        })
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::I64(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Str(s) => f.write_str(s),
            Value::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// One encoded part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub bytes: Vec<u8>,
    pub rows: u32,
    /// CRC32 of the bytes: the tap drops a part whose CRC matches the last one written.
    pub crc: u32,
    /// The regions its rows fall in, sorted (the tap finds a focus region's units by these).
    pub regions: Vec<RegionKey>,
}

#[derive(Default)]
struct Buf {
    rows: u32,
    body: Writer,
    counts: BTreeMap<(RegionKey, u8), u32>,
}

/// Collects one layer's rows into parts (module docs).
pub struct PartWriter {
    kind: u8,
    layer: u16,
    per_unit: u64,
    by_region: bool,
    bufs: BTreeMap<PartKey, Buf>,
    row: Writer,
    rows: usize,
}

impl PartWriter {
    /// A writer for `layer`: `per_unit` keys a unit; `by_region` for things that stay put.
    pub fn new(kind: u8, layer: u16, per_unit: u64, by_region: bool) -> PartWriter {
        PartWriter {
            kind,
            layer,
            per_unit: per_unit.max(1),
            by_region,
            bufs: BTreeMap::new(),
            row: Writer::new(),
            rows: 0,
        }
    }

    /// A marks writer.
    pub fn marks(layer: u16, per_unit: u64, by_region: bool) -> PartWriter {
        PartWriter::new(kind::MARKS, layer, per_unit, by_region)
    }

    pub fn per_unit(&self) -> u64 {
        self.per_unit
    }

    /// The unit holding `key`.
    pub fn unit_of(&self, key: u64) -> u64 {
        key / self.per_unit
    }

    /// Rows written so far.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// A mark of the thing with key `key`, in frame `frame`.
    pub fn push(&mut self, key: u64, frame: u32, m: &Mark) {
        self.push_with(key, frame, m, &[]);
    }

    /// A mark and its card fields (indices into the descriptor's `fields`).
    pub fn push_with(&mut self, key: u64, frame: u32, m: &Mark, fields: &[(u16, Value)]) {
        let a = m.anchor();
        let region = RegionKey::of(frame, a[0], a[1]);
        self.row.clear();
        self.row.var_u64(u64::from(frame));
        m.encode_body(&mut self.row);
        self.row.var_u64(fields.len() as u64);
        for (i, v) in fields {
            self.row.var_u64(u64::from(*i));
            v.encode(&mut self.row);
        }
        let k = m.kind();
        let pk = PartKey {
            kind: self.kind,
            layer: self.layer,
            unit: key / self.per_unit,
            region: self.by_region.then_some(region),
        };
        let b = self.bufs.entry(pk).or_default();
        b.body.u8(k);
        b.body.bytes(self.row.as_bytes());
        b.rows += 1;
        *b.counts.entry((region, k)).or_default() += 1;
        self.rows += 1;
    }

    /// A row that is not a mark (an archive row; V6's graph rows): its kind, the frame and place
    /// it is counted in, and its body. Readers that do not know `kind` skip it.
    pub fn push_raw(&mut self, key: u64, frame: u32, at: [f64; 2], kind: u8, body: &[u8]) {
        let region = RegionKey::of(frame, at[0], at[1]);
        self.row.clear();
        self.row.var_u64(u64::from(frame));
        self.row.raw(body);
        let pk = PartKey {
            kind: self.kind,
            layer: self.layer,
            unit: key / self.per_unit,
            region: self.by_region.then_some(region),
        };
        let b = self.bufs.entry(pk).or_default();
        b.body.u8(kind);
        b.body.bytes(self.row.as_bytes());
        b.rows += 1;
        *b.counts.entry((region, kind)).or_default() += 1;
        self.rows += 1;
    }

    /// The parts written, by key; units with no rows write none.
    pub fn finish(self) -> Vec<(PartKey, Part)> {
        self.bufs
            .into_iter()
            .map(|(k, b)| {
                let mut w = Writer::with_capacity(b.body.len() + 64);
                w.u32(MAGIC);
                w.u8(VERSION);
                k.encode(&mut w);
                w.var_u64(b.counts.len() as u64);
                for ((r, kind), n) in &b.counts {
                    w.var_u64(u64::from(r.frame));
                    w.var_i64(i64::from(r.rx));
                    w.var_i64(i64::from(r.rz));
                    w.u8(*kind);
                    w.var_u64(u64::from(*n));
                }
                w.var_u64(u64::from(b.rows));
                w.raw(b.body.as_bytes());
                let bytes = w.into_bytes();
                let crc = crc32fast::hash(&bytes);
                let mut regions: Vec<RegionKey> = b.counts.keys().map(|(r, _)| *r).collect();
                regions.dedup();
                (
                    k,
                    Part {
                        bytes,
                        rows: b.rows,
                        crc,
                        regions,
                    },
                )
            })
            .collect()
    }
}

/// One row read back.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: u8,
    pub frame: u32,
    /// The mark, for the mark kinds; `None` for any other kind.
    pub mark: Option<Mark>,
    pub fields: Vec<(u16, Value)>,
    /// A row that is not a mark: its body after the frame (empty for a mark).
    pub body: Vec<u8>,
}

/// A part read back: what the one reader in the core gives every consumer.
#[derive(Clone, Debug, PartialEq)]
pub struct PartView {
    pub key: PartKey,
    /// Rows per kind per region.
    pub counts: Vec<(RegionKey, u8, u32)>,
    pub rows: Vec<Row>,
}

/// Reads a part. Rows of unknown kinds come back with no mark.
pub fn read_part(bytes: &[u8]) -> Result<PartView, String> {
    let bad = |e: CodecError| format!("part: {e}");
    let mut r = Reader::new(bytes);
    if r.u32().map_err(bad)? != MAGIC {
        return Err("part: bad magic".into());
    }
    let v = r.u8().map_err(bad)?;
    if v != VERSION {
        return Err(format!("part: version {v}, this reader knows {VERSION}"));
    }
    let key = PartKey::decode(&mut r).map_err(bad)?;
    let n = r.len_prefix(5).map_err(bad)?;
    let mut counts = Vec::with_capacity(n);
    for _ in 0..n {
        let frame = r.var_u64().map_err(bad)? as u32;
        let rx = r.var_i64().map_err(bad)? as i32;
        let rz = r.var_i64().map_err(bad)? as i32;
        let k = r.u8().map_err(bad)?;
        let c = r.var_u64().map_err(bad)? as u32;
        counts.push((RegionKey { frame, rx, rz }, k, c));
    }
    let n = r.len_prefix(2).map_err(bad)?;
    let mut rows = Vec::with_capacity(n);
    for _ in 0..n {
        let k = r.u8().map_err(bad)?;
        let body = r.bytes().map_err(bad)?;
        let mut b = Reader::new(body);
        let frame = b.var_u64().map_err(bad)? as u32;
        let Some(mark) = Mark::decode_body(k, &mut b).map_err(bad)? else {
            rows.push(Row {
                kind: k,
                frame,
                mark: None,
                fields: Vec::new(),
                body: b.take(b.remaining()).map_err(bad)?.to_vec(),
            });
            continue;
        };
        let nf = b.len_prefix(2).map_err(bad)?;
        let mut fields = Vec::with_capacity(nf);
        for _ in 0..nf {
            let i = b.var_u64().map_err(bad)? as u16;
            fields.push((i, Value::decode(&mut b).map_err(bad)?));
        }
        b.finish().map_err(bad)?;
        rows.push(Row {
            kind: k,
            frame,
            mark: Some(mark),
            fields,
            body: Vec::new(),
        });
    }
    r.finish().map_err(bad)?;
    Ok(PartView { key, counts, rows })
}
