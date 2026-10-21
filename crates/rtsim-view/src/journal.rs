// SPDX-License-Identifier: AGPL-3.0-or-later
//! Java's dictionary, read from the world's `data/mic_rtsim_journal.dat` (the core's
//! `JournalSavedData`: gzipped NBT, `dictKinds` and `dictNames`, id = index + 1). The pages hold
//! Java's dimension and name ids, not the names: the engine learns the names from the DICT records
//! of each session and keeps them in memory only. Without the file, frames are named by id.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

/// The file, under the world folder.
pub const FILE: &str = "data/mic_rtsim_journal.dat";

/// Java's dictionary: (kind, id) to name.
#[derive(Clone, Debug, Default)]
pub struct Names {
    pub map: BTreeMap<(u32, u32), String>,
    /// Why the file gave no names, if it did not.
    pub missing: Option<String>,
}

impl Names {
    /// The dictionary in the world folder `world`, or an empty one saying why not.
    pub fn read(world: &Path) -> Names {
        let path = world.join(FILE);
        match std::fs::read(&path) {
            Ok(b) => match parse(&b) {
                Ok(map) => Names { map, missing: None },
                Err(e) => Names {
                    map: BTreeMap::new(),
                    missing: Some(format!("{FILE} does not read: {e}")),
                },
            },
            Err(e) => Names {
                map: BTreeMap::new(),
                missing: Some(format!("no {FILE} ({e})")),
            },
        }
    }

    pub fn get(&self, kind: u32, id: u32) -> Option<&str> {
        self.map.get(&(kind, id)).map(String::as_str)
    }
}

/// The dictionary in the bytes of a `mic_rtsim_journal.dat`.
pub fn parse(gz: &[u8]) -> Result<BTreeMap<(u32, u32), String>, String> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut raw)
        .map_err(|e| format!("not gzip: {e}"))?;
    let mut r = Nbt { b: &raw, at: 0 };
    if r.u8()? != COMPOUND {
        return Err("the root is not a compound".into());
    }
    r.string()?;
    let mut kinds: Option<Vec<i32>> = None;
    let mut names: Option<Vec<String>> = None;
    r.compound(&mut |r, ty, name| {
        if ty == COMPOUND && name == "data" {
            r.compound(&mut |r, ty, name| match (ty, name) {
                (INT_ARRAY, "dictKinds") => {
                    kinds = Some(r.int_array()?);
                    Ok(())
                }
                (LIST, "dictNames") => {
                    names = Some(r.string_list()?);
                    Ok(())
                }
                _ => r.skip(ty),
            })
        } else {
            r.skip(ty)
        }
    })?;
    let (Some(kinds), Some(names)) = (kinds, names) else {
        return Err("no dictKinds and dictNames in data".into());
    };
    if kinds.len() != names.len() {
        return Err(format!("{} kinds for {} names", kinds.len(), names.len()));
    }
    Ok(kinds
        .into_iter()
        .zip(names)
        .enumerate()
        .map(|(i, (k, n))| ((k as u32, i as u32 + 1), n))
        .collect())
}

const END: u8 = 0;
const COMPOUND: u8 = 10;
const LIST: u8 = 9;
const STRING: u8 = 8;
const INT_ARRAY: u8 = 11;

/// Big-endian NBT, read only as far as the dictionary needs; everything else is skipped.
struct Nbt<'a> {
    b: &'a [u8],
    at: usize,
}

impl Nbt<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or("truncated")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn len(&mut self) -> Result<usize, String> {
        usize::try_from(self.i32()?).map_err(|_| "a negative length".to_string())
    }

    /// Modified UTF-8; the names the dictionary holds are ASCII, so lossy UTF-8 reads them.
    fn string(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }

    fn int_array(&mut self) -> Result<Vec<i32>, String> {
        let n = self.len()?;
        (0..n).map(|_| self.i32()).collect()
    }

    fn string_list(&mut self) -> Result<Vec<String>, String> {
        let ty = self.u8()?;
        let n = self.len()?;
        if n > 0 && ty != STRING {
            return Err(format!("dictNames is a list of type {ty}"));
        }
        (0..n).map(|_| self.string()).collect()
    }

    fn compound(
        &mut self,
        f: &mut dyn FnMut(&mut Self, u8, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        loop {
            let ty = self.u8()?;
            if ty == END {
                return Ok(());
            }
            let name = self.string()?;
            f(self, ty, &name)?;
        }
    }

    fn skip(&mut self, ty: u8) -> Result<(), String> {
        match ty {
            1 => self.take(1).map(drop),
            2 => self.take(2).map(drop),
            3 | 5 => self.take(4).map(drop),
            4 | 6 => self.take(8).map(drop),
            7 => {
                let n = self.len()?;
                self.take(n).map(drop)
            }
            STRING => self.string().map(drop),
            LIST => {
                let t = self.u8()?;
                let n = self.len()?;
                (0..n).try_for_each(|_| self.skip(t))
            }
            COMPOUND => self.compound(&mut |r, t, _| r.skip(t)),
            INT_ARRAY => {
                let n = self.len()?;
                self.take(n.checked_mul(4).ok_or("too long")?).map(drop)
            }
            12 => {
                let n = self.len()?;
                self.take(n.checked_mul(8).ok_or("too long")?).map(drop)
            }
            t => Err(format!("unknown tag type {t}")),
        }
    }
}

/// Writes a `mic_rtsim_journal.dat` holding only a dictionary: for tests, and for a world the
/// viewer synthesizes. The layout is the core's (`JournalSavedData`).
pub fn write_dictionary(entries: &[(u32, String)]) -> Vec<u8> {
    let mut b = Vec::new();
    let s = |b: &mut Vec<u8>, v: &str| {
        b.extend_from_slice(&(v.len() as u16).to_be_bytes());
        b.extend_from_slice(v.as_bytes());
    };
    b.push(COMPOUND);
    s(&mut b, "");
    b.push(COMPOUND);
    s(&mut b, "data");
    b.push(INT_ARRAY);
    s(&mut b, "dictKinds");
    b.extend_from_slice(&(entries.len() as i32).to_be_bytes());
    for (k, _) in entries {
        b.extend_from_slice(&(*k as i32).to_be_bytes());
    }
    b.push(LIST);
    s(&mut b, "dictNames");
    b.push(STRING);
    b.extend_from_slice(&(entries.len() as i32).to_be_bytes());
    for (_, n) in entries {
        s(&mut b, n);
    }
    b.push(4);
    s(&mut b, "nextObsSeq");
    b.extend_from_slice(&1i64.to_be_bytes());
    b.push(END);
    b.push(3);
    s(&mut b, "DataVersion");
    b.extend_from_slice(&3955i32.to_be_bytes());
    b.push(END);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut gz, &b).expect("in memory");
    gz.finish().expect("in memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_reads_back_with_ids_from_one() {
        let bytes = write_dictionary(&[
            (1, "minecraft:overworld".into()),
            (2, "minecraft:emerald".into()),
            (1, "minecraft:the_nether".into()),
        ]);
        let m = parse(&bytes).unwrap();
        assert_eq!(
            m.get(&(1, 1)).map(String::as_str),
            Some("minecraft:overworld")
        );
        assert_eq!(
            m.get(&(2, 2)).map(String::as_str),
            Some("minecraft:emerald")
        );
        assert_eq!(
            m.get(&(1, 3)).map(String::as_str),
            Some("minecraft:the_nether")
        );
        assert_eq!(m.len(), 3);
        assert!(parse(b"not gzip").is_err());
    }
}
