// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the settlement table (the phase 1 world's snapshot, docs/overnight-choices-persist.md):
//! the rows with their handles, free slots included, and the grid as it stands, so a restored
//! table answers every search exactly as the saved one.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};

use crate::{Culture, NodeKind, Pos, Settlement, Settlements};

impl Encode for Culture {
    fn encode(&self, w: &mut Writer) {
        w.u8(*self as u8);
    }
}

impl Decode for Culture {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        match r.u8()? {
            0 => Ok(Culture::Human),
            1 => Ok(Culture::Ribbit),
            c => Err(CodecError::Invalid(format!("culture {c}"))),
        }
    }
}

impl Encode for NodeKind {
    fn encode(&self, w: &mut Writer) {
        match self {
            NodeKind::Village(c) => {
                w.u8(0);
                c.encode(w);
            }
            NodeKind::PlayerBase { owner } => {
                w.u8(1);
                w.u64(*owner);
            }
            NodeKind::Haven(c) => {
                w.u8(2);
                c.encode(w);
            }
        }
    }
}

impl Decode for NodeKind {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(match r.u8()? {
            0 => NodeKind::Village(Culture::decode(r)?),
            1 => NodeKind::PlayerBase { owner: r.u64()? },
            2 => NodeKind::Haven(Culture::decode(r)?),
            t => return Err(CodecError::Invalid(format!("node kind {t}"))),
        })
    }
}

impl Encode for Pos {
    fn encode(&self, w: &mut Writer) {
        w.u16(self.frame);
        w.var_i64(i64::from(self.x));
        w.var_i64(i64::from(self.z));
    }
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

impl Decode for Pos {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Pos {
            frame: r.u16()?,
            x: i32_of(r.var_i64()?)?,
            z: i32_of(r.var_i64()?)?,
        })
    }
}

impl Encode for Settlement {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.key);
        self.kind.encode(w);
        self.pos.encode(w);
        w.u32(self.pop);
        w.u64(self.joined.0);
    }
}

impl Decode for Settlement {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Settlement {
            key: r.u64()?,
            kind: NodeKind::decode(r)?,
            pos: Pos::decode(r)?,
            pop: r.u32()?,
            joined: Tick(r.u64()?),
        })
    }
}

impl Encode for Settlements {
    fn encode(&self, w: &mut Writer) {
        self.rows.encode_with(w, |s, w| s.encode(w));
        w.var_u64(self.grid.len() as u64);
        for (&(frame, cx, cz), ids) in &self.grid {
            w.u16(frame);
            w.var_i64(i64::from(cx));
            w.var_i64(i64::from(cz));
            w.var_u64(ids.len() as u64);
            for id in ids {
                w.u64(id.to_bits());
            }
        }
    }
}

impl Decode for Settlements {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let rows: Arena<Settlement> = Arena::decode_with(r, Settlement::decode)?;
        let by_key = rows.iter().map(|(id, s)| (s.key, id)).collect();
        let mut grid = std::collections::BTreeMap::new();
        for _ in 0..r.len_prefix(4)? {
            let cell = (r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?);
            let n = r.len_prefix(8)?;
            let mut ids = Vec::with_capacity(n);
            for _ in 0..n {
                let id = Handle::from_bits(r.u64()?);
                if rows.get(id).is_none() {
                    return Err(CodecError::Invalid(
                        "a grid cell names no settlement".into(),
                    ));
                }
                ids.push(id);
            }
            grid.insert(cell, ids);
        }
        Ok(Settlements { rows, by_key, grid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EdgeRule, Settlement};

    #[test]
    fn a_saved_table_finds_the_same_neighbours_and_hands_out_the_same_ids() {
        let mut s = Settlements::new();
        let mut ids = Vec::new();
        for i in 0..40u64 {
            let x = ((i * 7919) % 6000) as i32 - 3000;
            let z = ((i * 104_729) % 6000) as i32 - 3000;
            ids.push(s.insert(Settlement {
                key: 1000 + i,
                kind: if i % 3 == 0 {
                    NodeKind::Village(Culture::Ribbit)
                } else {
                    NodeKind::Village(Culture::Human)
                },
                pos: Pos::new((i % 2) as u16, x, z),
                pop: 5 + i as u32,
                joined: Tick(i * 10),
            }));
        }
        s.remove(ids[3]);
        s.remove(ids[17]);
        let mut back = Settlements::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(back.to_bytes(), s.to_bytes());
        let rule = EdgeRule::default();
        assert_eq!(back.edges(&rule), s.edges(&rule));
        for &id in &ids {
            assert_eq!(back.nearest(id, 4, 6400), s.nearest(id, 4, 6400));
        }
        let row = Settlement {
            key: 77,
            kind: NodeKind::PlayerBase { owner: 9 },
            pos: Pos::new(0, 10, 10),
            pop: 0,
            joined: Tick(99),
        };
        assert_eq!(back.insert(row.clone()), s.insert(row));
    }
}
