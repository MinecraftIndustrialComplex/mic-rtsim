// SPDX-License-Identifier: AGPL-3.0-or-later
//! The archive's dictionary (VIEWER.md §10.2): kind names, frame names and the ids too big for a
//! row's 48 bits, each given a number on first use. It only grows, so a number never changes
//! meaning: after a rollback the replay asks for the same names and gets the same numbers.

use std::collections::HashMap;

use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};

use crate::row::{Handle, MAX_48};

/// Frames a row can name: the frame byte's values 0 to 254. A further dimension's rows are dropped
/// and counted, never mislabelled.
pub const MAX_FRAMES: usize = 255;

const MAGIC: &[u8; 4] = b"RTAD";
const VERSION: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    kinds: Vec<String>,
    frames: Vec<String>,
    ids: Vec<u128>,
    kind_of: HashMap<String, u16>,
    frame_of: HashMap<String, u8>,
    id_of: HashMap<u128, u64>,
    /// Entries added since [`Dictionary::mark_saved`].
    dirty: bool,
}

impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary::default()
    }

    /// A kind's number (`"conflict.raid"`), added on first use; `None` past 65,536 kinds.
    pub fn kind(&mut self, name: &str) -> Option<u16> {
        if let Some(&k) = self.kind_of.get(name) {
            return Some(k);
        }
        let k = u16::try_from(self.kinds.len()).ok()?;
        self.kinds.push(name.to_string());
        self.kind_of.insert(name.to_string(), k);
        self.dirty = true;
        Some(k)
    }

    /// A frame's number (a dimension id), added on first use; `None` past [`MAX_FRAMES`].
    pub fn frame(&mut self, name: &str) -> Option<u8> {
        if let Some(&f) = self.frame_of.get(name) {
            return Some(f);
        }
        if self.frames.len() >= MAX_FRAMES {
            return None;
        }
        let f = self.frames.len() as u8;
        self.frames.push(name.to_string());
        self.frame_of.insert(name.to_string(), f);
        self.dirty = true;
        Some(f)
    }

    /// An entity id as a row's 48-bit handle: itself when it fits, otherwise its dictionary entry
    /// (a player's UUID, a 64-bit id with high bits set). Nothing is truncated.
    pub fn handle(&mut self, id: u128) -> Handle {
        if id <= u128::from(MAX_48) {
            return Handle {
                value: id as u64,
                dict: false,
            };
        }
        if let Some(&i) = self.id_of.get(&id) {
            return Handle {
                value: i,
                dict: true,
            };
        }
        let i = self.ids.len() as u64;
        self.ids.push(id);
        self.id_of.insert(id, i);
        self.dirty = true;
        Handle {
            value: i,
            dict: true,
        }
    }

    /// The entity id behind a handle; `None` for a dictionary entry this dictionary lacks.
    pub fn resolve(&self, h: Handle) -> Option<u128> {
        if h.dict {
            self.ids.get(h.value as usize).copied()
        } else {
            Some(u128::from(h.value))
        }
    }

    pub fn kind_name(&self, k: u16) -> Option<&str> {
        self.kinds.get(k as usize).map(String::as_str)
    }

    pub fn frame_name(&self, f: u8) -> Option<&str> {
        self.frames.get(f as usize).map(String::as_str)
    }

    pub fn kinds(&self) -> &[String] {
        &self.kinds
    }

    pub fn frames(&self) -> &[String] {
        &self.frames
    }

    /// Whether entries were added since the last [`Dictionary::mark_saved`].
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Every entry, ending in a CRC32 of the rest.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(MAGIC);
        w.u8(VERSION);
        for names in [&self.kinds, &self.frames] {
            w.var_u64(names.len() as u64);
            for n in names {
                w.bytes(n.as_bytes());
            }
        }
        w.var_u64(self.ids.len() as u64);
        for &id in &self.ids {
            w.i128(id as i128);
        }
        let crc = crc32fast::hash(w.as_bytes());
        w.u32(crc);
        w.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Dictionary> {
        let body = check_crc(bytes, "dictionary")?;
        let mut r = Reader::new(body);
        if r.take(4)? != MAGIC || r.u8()? != VERSION {
            return Err(CodecError::Invalid(
                "not an archive dictionary of this version".into(),
            ));
        }
        let mut d = Dictionary::new();
        let mut names = || -> Result<Vec<String>> {
            let n = r.len_prefix(1)?;
            (0..n)
                .map(|_| {
                    String::from_utf8(r.bytes()?.to_vec())
                        .map_err(|_| CodecError::Invalid("a name not UTF-8".into()))
                })
                .collect()
        };
        let kinds = names()?;
        let frames = names()?;
        if kinds.len() > 1 << 16 || frames.len() > MAX_FRAMES {
            return Err(CodecError::Invalid("too many kinds or frames".into()));
        }
        let n = r.len_prefix(16)?;
        let ids = (0..n)
            .map(|_| r.i128().map(|v| v as u128))
            .collect::<Result<Vec<_>>>()?;
        r.finish()?;
        for k in kinds {
            d.kind(&k);
        }
        for f in frames {
            d.frame(&f);
        }
        for id in ids {
            d.handle(id);
        }
        d.dirty = false;
        Ok(d)
    }
}

/// The bytes before a trailing CRC32, if it matches.
pub(crate) fn check_crc<'a>(bytes: &'a [u8], what: &str) -> Result<&'a [u8]> {
    let Some(split) = bytes.len().checked_sub(4) else {
        return Err(CodecError::Truncated {
            wanted: 4,
            left: bytes.len(),
        });
    };
    let (body, crc) = bytes.split_at(split);
    if crc32fast::hash(body).to_le_bytes() != crc {
        return Err(CodecError::Invalid(format!("{what} checksum mismatch")));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_stable_and_survive_a_round_trip() {
        let mut d = Dictionary::new();
        assert_eq!(d.kind("conflict.raid"), Some(0));
        assert_eq!(d.kind("dragons.leg"), Some(1));
        assert_eq!(d.kind("conflict.raid"), Some(0));
        assert_eq!(d.frame("minecraft:overworld"), Some(0));
        let small = d.handle(42);
        assert_eq!(
            small,
            Handle {
                value: 42,
                dict: false
            }
        );
        let uuid = 0x1234_5678_9ABC_DEF0_0FED_CBA9_8765_4321u128;
        let big = d.handle(uuid);
        assert_eq!(
            big,
            Handle {
                value: 0,
                dict: true
            }
        );
        assert_eq!(d.handle(uuid), big);
        assert_eq!(
            d.handle(u128::from(MAX_48) + 1),
            Handle {
                value: 1,
                dict: true
            }
        );
        assert_eq!(d.resolve(big), Some(uuid));
        assert_eq!(d.resolve(small), Some(42));
        assert_eq!(
            d.resolve(Handle {
                value: 9,
                dict: true
            }),
            None
        );
        assert!(d.is_dirty());
        let back = Dictionary::decode(&d.encode()).unwrap();
        assert_eq!(back.kind_name(1), Some("dragons.leg"));
        assert_eq!(back.frame_name(0), Some("minecraft:overworld"));
        assert_eq!(back.resolve(big), Some(uuid));
        assert!(!back.is_dirty());
        let mut again = back.clone();
        assert_eq!(again.kind("dragons.leg"), Some(1));
        assert!(!again.is_dirty());
        assert_eq!(again.kind("herds.leg"), Some(2));
        assert!(again.is_dirty());
    }

    #[test]
    fn frames_stop_at_255() {
        let mut d = Dictionary::new();
        for i in 0..MAX_FRAMES {
            assert_eq!(d.frame(&format!("dim:{i}")), Some(i as u8));
        }
        assert_eq!(d.frame("dim:overflow"), None);
        assert_eq!(d.frame("dim:3"), Some(3));
        assert_eq!(
            Dictionary::decode(&d.encode()).unwrap().frames().len(),
            MAX_FRAMES
        );
    }

    #[test]
    fn damage_is_an_error() {
        let mut d = Dictionary::new();
        d.kind("a");
        d.handle(u128::MAX);
        let bytes = d.encode();
        for cut in 0..bytes.len() {
            assert!(Dictionary::decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        for i in 0..bytes.len() {
            let mut b = bytes.clone();
            b[i] ^= 0x40;
            assert!(Dictionary::decode(&b).is_err(), "flip at {i}");
        }
    }
}
