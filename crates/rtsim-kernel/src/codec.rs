// SPDX-License-Identifier: AGPL-3.0-or-later
//! Little-endian binary encoding for page contents.
//!
//! Hand-written rather than serde: page rows are fixed layouts read and written in bulk, and a
//! format change before release wipes the save (PLAN.md §4.9), so there is no schema evolution to
//! pay for. Varints keep small counts small; LZ4 in [`crate::page`] takes care of the rest.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// The input ended inside a value.
    Truncated { wanted: usize, left: usize },
    /// A value was out of range for its type.
    Invalid(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Truncated { wanted, left } => {
                write!(f, "truncated: wanted {wanted} bytes, {left} left")
            }
            CodecError::Invalid(s) => write!(f, "invalid: {s}"),
        }
    }
}

impl std::error::Error for CodecError {}

pub type Result<T> = std::result::Result<T, CodecError>;

#[derive(Clone, Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            buf: Vec::with_capacity(n),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_bits().to_le_bytes());
    }

    /// LEB128.
    pub fn var_u64(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }

    /// Zigzag, then LEB128.
    pub fn var_i64(&mut self, v: i64) {
        self.var_u64(((v << 1) ^ (v >> 63)) as u64);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.var_u64(b.len() as u64);
        self.buf.extend_from_slice(b);
    }

    pub fn raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub fn i128(&mut self, v: i128) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
}

#[derive(Clone, Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.buf.len() < n {
            return Err(CodecError::Truncated {
                wanted: n,
                left: self.buf.len(),
            });
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.array().map(u16::from_le_bytes)
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.array().map(u32::from_le_bytes)
    }

    pub fn u64(&mut self) -> Result<u64> {
        self.array().map(u64::from_le_bytes)
    }

    pub fn i64(&mut self) -> Result<i64> {
        self.array().map(i64::from_le_bytes)
    }

    pub fn f64(&mut self) -> Result<f64> {
        self.u64().map(f64::from_bits)
    }

    pub fn var_u64(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let byte = self.u8()?;
            let bits = u64::from(byte & 0x7f);
            if i == 9 && byte > 1 {
                return Err(CodecError::Invalid("varint overflows u64".into()));
            }
            v |= bits << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(CodecError::Invalid("varint longer than 10 bytes".into()))
    }

    pub fn var_i64(&mut self) -> Result<i64> {
        let z = self.var_u64()?;
        Ok((z >> 1) as i64 ^ -((z & 1) as i64))
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.var_u64()?;
        let n = usize::try_from(n).map_err(|_| CodecError::Invalid("length".into()))?;
        self.take(n)
    }

    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(CodecError::Invalid(format!("{v} is not a bool"))),
        }
    }

    pub fn f32(&mut self) -> Result<f32> {
        self.u32().map(f32::from_bits)
    }

    pub fn i128(&mut self) -> Result<i128> {
        self.array().map(i128::from_le_bytes)
    }

    /// A length prefix (LEB128), bounded by the bytes left: each element takes at least
    /// `min_bytes`, so a corrupt length fails here instead of allocating without bound.
    pub fn len_prefix(&mut self, min_bytes: usize) -> Result<usize> {
        let n = self.var_u64()?;
        let n = usize::try_from(n).map_err(|_| CodecError::Invalid(format!("length {n}")))?;
        if n.saturating_mul(min_bytes.max(1)) > self.buf.len() {
            return Err(CodecError::Invalid(format!(
                "length {n} with only {} bytes left",
                self.buf.len()
            )));
        }
        Ok(n)
    }

    /// Fails unless everything was read: a page with trailing bytes was written by other code.
    pub fn finish(self) -> Result<()> {
        if self.buf.is_empty() {
            Ok(())
        } else {
            Err(CodecError::Invalid(format!(
                "{} trailing bytes",
                self.buf.len()
            )))
        }
    }
}

/// A value that can be written into a page.
pub trait Encode {
    fn encode(&self, w: &mut Writer);

    fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode(&mut w);
        w.into_bytes()
    }
}

/// A value that can be read back from a page.
pub trait Decode: Sized {
    fn decode(r: &mut Reader<'_>) -> Result<Self>;

    fn from_bytes(b: &[u8]) -> Result<Self> {
        let mut r = Reader::new(b);
        let v = Self::decode(&mut r)?;
        r.finish()?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn round_trips(a in any::<u64>(), b in any::<i64>(), c in any::<u16>(), d in prop::collection::vec(any::<u8>(), 0..300), f in any::<f64>()) {
            let mut w = Writer::new();
            w.var_u64(a);
            w.var_i64(b);
            w.u16(c);
            w.bytes(&d);
            w.u64(a);
            w.i64(b);
            w.f64(f);
            let bytes = w.into_bytes();
            let mut r = Reader::new(&bytes);
            prop_assert_eq!(r.var_u64().unwrap(), a);
            prop_assert_eq!(r.var_i64().unwrap(), b);
            prop_assert_eq!(r.u16().unwrap(), c);
            prop_assert_eq!(r.bytes().unwrap(), &d[..]);
            prop_assert_eq!(r.u64().unwrap(), a);
            prop_assert_eq!(r.i64().unwrap(), b);
            prop_assert_eq!(r.f64().unwrap().to_bits(), f.to_bits());
            r.finish().unwrap();
        }

        #[test]
        fn truncation_is_an_error_not_a_panic(bytes in prop::collection::vec(any::<u8>(), 0..20)) {
            let mut r = Reader::new(&bytes);
            while r.var_u64().is_ok() {}
            let mut r = Reader::new(&bytes);
            let _ = r.bytes();
        }
    }

    #[test]
    fn small_varints_are_one_byte() {
        let mut w = Writer::new();
        w.var_u64(127);
        w.var_i64(-64);
        assert_eq!(w.len(), 2);
    }
}
