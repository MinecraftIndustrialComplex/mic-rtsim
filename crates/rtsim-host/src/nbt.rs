// SPDX-License-Identifier: AGPL-3.0-or-later
//! Minecraft's NBT, read and written whole: the gzipped files the host reads from a world and
//! writes into a fork (`data/mic_rtsim_journal.dat`, `level.dat`). Strings are kept as their bytes
//! (Java's modified UTF-8), so a file read and written again is the same file.

use std::io::{Read, Write};

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(Vec<u8>),
    /// The element type, then the elements (an empty list keeps its type).
    List(u8, Vec<Tag>),
    /// In file order.
    Compound(Vec<(Vec<u8>, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

pub const END: u8 = 0;
pub const BYTE: u8 = 1;
pub const SHORT: u8 = 2;
pub const INT: u8 = 3;
pub const LONG: u8 = 4;
pub const FLOAT: u8 = 5;
pub const DOUBLE: u8 = 6;
pub const BYTE_ARRAY: u8 = 7;
pub const STRING: u8 = 8;
pub const LIST: u8 = 9;
pub const COMPOUND: u8 = 10;
pub const INT_ARRAY: u8 = 11;
pub const LONG_ARRAY: u8 = 12;

/// Deeper nesting than this is refused (vanilla's limit is 512).
const MAX_DEPTH: usize = 512;

impl Tag {
    pub fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => BYTE,
            Tag::Short(_) => SHORT,
            Tag::Int(_) => INT,
            Tag::Long(_) => LONG,
            Tag::Float(_) => FLOAT,
            Tag::Double(_) => DOUBLE,
            Tag::ByteArray(_) => BYTE_ARRAY,
            Tag::String(_) => STRING,
            Tag::List(..) => LIST,
            Tag::Compound(_) => COMPOUND,
            Tag::IntArray(_) => INT_ARRAY,
            Tag::LongArray(_) => LONG_ARRAY,
        }
    }

    /// A compound's entry.
    pub fn get(&self, name: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(es) => es
                .iter()
                .find(|(n, _)| n == name.as_bytes())
                .map(|(_, t)| t),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Tag> {
        match self {
            Tag::Compound(es) => es
                .iter_mut()
                .find(|(n, _)| n == name.as_bytes())
                .map(|(_, t)| t),
            _ => None,
        }
    }

    /// Sets a compound's entry, replacing one of the same name in place.
    pub fn put(&mut self, name: &str, tag: Tag) {
        if let Tag::Compound(es) = self {
            match es.iter_mut().find(|(n, _)| n == name.as_bytes()) {
                Some(e) => e.1 = tag,
                None => es.push((name.as_bytes().to_vec(), tag)),
            }
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Tag::Byte(v) => Some(i64::from(v)),
            Tag::Short(v) => Some(i64::from(v)),
            Tag::Int(v) => Some(i64::from(v)),
            Tag::Long(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<String> {
        match self {
            Tag::String(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    pub fn string(s: &str) -> Tag {
        Tag::String(s.as_bytes().to_vec())
    }
}

/// A gzipped NBT file's root: its name and its compound.
pub fn read_gz(gz: &[u8]) -> Result<(Vec<u8>, Tag), String> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut raw)
        .map_err(|e| format!("not gzip: {e}"))?;
    read(&raw)
}

/// An uncompressed NBT root.
pub fn read(raw: &[u8]) -> Result<(Vec<u8>, Tag), String> {
    let mut r = Reader { b: raw, at: 0 };
    let ty = r.u8()?;
    if ty != COMPOUND {
        return Err(format!("the root is tag {ty}, not a compound"));
    }
    let name = r.string()?;
    let tag = r.payload(COMPOUND, 0)?;
    Ok((name, tag))
}

pub fn write_gz(name: &[u8], root: &Tag) -> Vec<u8> {
    let raw = write(name, root);
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(&raw).expect("gzip into memory");
    e.finish().expect("gzip into memory")
}

pub fn write(name: &[u8], root: &Tag) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(root.id());
    put_string(&mut out, name);
    put_payload(&mut out, root);
    out
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or_else(|| format!("truncated at byte {}", self.at))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn arr<const N: usize>(&mut self) -> Result<[u8; N], String> {
        Ok(self.take(N)?.try_into().expect("N bytes"))
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn len(&mut self) -> Result<usize, String> {
        let n = i32::from_be_bytes(self.arr()?);
        usize::try_from(n).map_err(|_| format!("a negative length {n}"))
    }

    fn string(&mut self) -> Result<Vec<u8>, String> {
        let n = u16::from_be_bytes(self.arr()?) as usize;
        Ok(self.take(n)?.to_vec())
    }

    fn payload(&mut self, ty: u8, depth: usize) -> Result<Tag, String> {
        if depth > MAX_DEPTH {
            return Err("nested too deep".into());
        }
        Ok(match ty {
            BYTE => Tag::Byte(self.u8()? as i8),
            SHORT => Tag::Short(i16::from_be_bytes(self.arr()?)),
            INT => Tag::Int(i32::from_be_bytes(self.arr()?)),
            LONG => Tag::Long(i64::from_be_bytes(self.arr()?)),
            FLOAT => Tag::Float(f32::from_be_bytes(self.arr()?)),
            DOUBLE => Tag::Double(f64::from_be_bytes(self.arr()?)),
            BYTE_ARRAY => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.to_vec())
            }
            STRING => Tag::String(self.string()?),
            LIST => {
                let et = self.u8()?;
                let n = self.len()?;
                if n > 0 && et == END {
                    return Err("a list of END tags".into());
                }
                // Every element is at least one byte: a length past the bytes left is corrupt.
                if n > self.b.len() - self.at {
                    return Err(format!("a list of {n} past the end"));
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(self.payload(et, depth + 1)?);
                }
                Tag::List(et, v)
            }
            COMPOUND => {
                let mut es = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == END {
                        break;
                    }
                    let name = self.string()?;
                    es.push((name, self.payload(t, depth + 1)?));
                }
                Tag::Compound(es)
            }
            INT_ARRAY => {
                let n = self.len()?;
                let b = self.take(n.checked_mul(4).ok_or("too long")?)?;
                Tag::IntArray(
                    b.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|c| i32::from_be_bytes(*c))
                        .collect(),
                )
            }
            LONG_ARRAY => {
                let n = self.len()?;
                let b = self.take(n.checked_mul(8).ok_or("too long")?)?;
                Tag::LongArray(
                    b.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| i64::from_be_bytes(*c))
                        .collect(),
                )
            }
            t => return Err(format!("unknown tag {t}")),
        })
    }
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    let n = u16::try_from(s.len()).expect("an NBT string under 64 KiB");
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(s);
}

fn put_len(out: &mut Vec<u8>, n: usize) {
    let n = i32::try_from(n).expect("an NBT array under 2^31 elements");
    out.extend_from_slice(&n.to_be_bytes());
}

fn put_payload(out: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::Byte(v) => out.push(*v as u8),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::ByteArray(b) => {
            put_len(out, b.len());
            out.extend_from_slice(b);
        }
        Tag::String(s) => put_string(out, s),
        Tag::List(et, v) => {
            out.push(if v.is_empty() { *et } else { v[0].id() });
            put_len(out, v.len());
            for e in v {
                put_payload(out, e);
            }
        }
        Tag::Compound(es) => {
            for (n, e) in es {
                out.push(e.id());
                put_string(out, n);
                put_payload(out, e);
            }
            out.push(END);
        }
        Tag::IntArray(v) => {
            put_len(out, v.len());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            put_len(out, v.len());
            for x in v {
                out.extend_from_slice(&x.to_be_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Tag {
        Tag::Compound(vec![
            (
                b"data".to_vec(),
                Tag::Compound(vec![
                    (b"lastAppliedCmdSeq".to_vec(), Tag::Long(7)),
                    (b"seqs".to_vec(), Tag::LongArray(vec![5, 6, -1])),
                    (
                        b"records".to_vec(),
                        Tag::List(BYTE_ARRAY, vec![Tag::ByteArray(vec![1, 2, 3])]),
                    ),
                    (b"dictKinds".to_vec(), Tag::IntArray(vec![1, 2])),
                    (b"empty".to_vec(), Tag::List(STRING, vec![])),
                    (
                        b"names".to_vec(),
                        Tag::List(STRING, vec![Tag::string("minecraft:overworld")]),
                    ),
                    (b"f".to_vec(), Tag::Float(1.5)),
                    (b"d".to_vec(), Tag::Double(-2.25)),
                    (b"b".to_vec(), Tag::Byte(-3)),
                    (b"s".to_vec(), Tag::Short(300)),
                    (b"i".to_vec(), Tag::Int(-70_000)),
                ]),
            ),
            (b"DataVersion".to_vec(), Tag::Int(3955)),
        ])
    }

    #[test]
    fn a_tree_round_trips_byte_for_byte() {
        let t = sample();
        let raw = write(b"", &t);
        let (name, back) = read(&raw).unwrap();
        assert!(name.is_empty());
        assert_eq!(back, t);
        assert_eq!(write(b"", &back), raw);
        let (_, gz) = read_gz(&write_gz(b"", &t)).unwrap();
        assert_eq!(gz, t);
        assert_eq!(
            t.get("data")
                .and_then(|d| d.get("lastAppliedCmdSeq"))
                .and_then(Tag::as_i64),
            Some(7)
        );
    }

    #[test]
    fn truncated_and_corrupt_files_are_errors() {
        let raw = write(b"", &sample());
        for cut in [0, 1, 5, raw.len() / 2, raw.len() - 1] {
            assert!(read(&raw[..cut]).is_err(), "cut at {cut}");
        }
        assert!(read(&[LIST, 0, 0]).is_err(), "a list root");
        assert!(read_gz(b"not gzip").is_err());
    }
}
