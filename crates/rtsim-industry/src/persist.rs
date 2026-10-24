// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the industry engine into kernel pages (SCALE.md §8, A16; INDUSTRY.md §15: "a district
//! keeps its model, boundary list, pending deltas and defence vector, all paged with its region").
//!
//! - **One page per district** (table [`DISTRICT_TABLE`], id = district id), **one per replay
//!   cluster** ([`CLUSTER_TABLE`], id = cluster key, with its booked wake, restored as is: DECISIONS.md
//!   O18), **one per district's glass state** ([`GLASS_TABLE`], id = district id: the compile, its
//!   passed window and the glass replay, `glass::page`, G2-9), **one per 256 booked train legs**
//!   ([`LEG_TABLE`], id = the leg ids divided by 256, ID10: the legs grow with the lines, so they
//!   are not in the globals), **one per package store** ([`STORE_TABLE`], id = the store id,
//!   `stores`), and **one page of globals** ([`GLOBAL_TABLE`], id 0).
//! - **A save writes the globals page and the pages changed since the save before**
//!   (`Engine::save_changed`, docs/choices-industry-dirty-pages.md): the engine marks a district,
//!   cluster, glass or leg page on every change, and a marked page whose entry is gone is deleted.
//!   `Engine::snapshot` is the whole state, for a fresh store.
//! - **Everything else is saved**, open learning windows and half-assembled sweeps included, so a
//!   restored engine continues exactly (tested). A real restart then calls
//!   `Engine::session_start`, which discards the windows (F6). **Not saved:** the engine's text log,
//!   cluster events (drained after every touch) and the glass solve's wall-clock time.
//! - **No format version** before release: a change wipes the save (PLAN.md §4.9). A district
//!   page starts with a magic (`district::DISTRICT_MAGIC`, bumped on a change to the district
//!   format), so an older page fails by name rather than mid-record.
//!
//! The kernel's `Encode`/`Decode` cannot be implemented for std types from this crate, so values go
//! through [`Persist`], a crate-local trait on the kernel's `Writer` and `Reader`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};
use rtsim_kernel::conserve::Diffuser;
use rtsim_kernel::journal::OpId;

use crate::ids::{
    BlockPos, ChunkPos, DistrictId, Good, GoodClass, GoodInfo, GoodVec, StationId, StoreId, TrainId,
    VecMap,
};

/// Page tables of the industry engine (the kernel's `PageKey` top 16 bits). PLACEHOLDER numbers,
/// to be registered with the module registry (PLAN.md §5.4) when it exists.
pub const GLOBAL_TABLE: u16 = 0x4900;
pub const DISTRICT_TABLE: u16 = 0x4901;
pub const CLUSTER_TABLE: u16 = 0x4902;
pub const GLASS_TABLE: u16 = 0x4903;
/// The booked train legs, [`crate::threat::LEGS_PER_PAGE`] ids to a page (id = the page's number, ID10).
pub const LEG_TABLE: u16 = 0x4904;
/// The package stores (SECTIONS.md §2.5, slice S4), one page each (id = the store id). 0x4905 is
/// left free.
pub const STORE_TABLE: u16 = 0x4906;

/// A value that can go into an industry page.
pub trait Persist: Sized {
    fn put(&self, w: &mut Writer);
    fn get(r: &mut Reader<'_>) -> Result<Self>;
}

pub(crate) fn invalid<T>(what: &str) -> Result<T> {
    Err(CodecError::Invalid(what.to_string()))
}

macro_rules! persist_int {
    ($($t:ty => $w:ident, $r:ident);* $(;)?) => {
        $(impl Persist for $t {
            fn put(&self, w: &mut Writer) { w.$w((*self).into()); }
            fn get(r: &mut Reader<'_>) -> Result<Self> {
                let v = r.$r()?;
                <$t>::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} out of range for {}", stringify!($t))))
            }
        })*
    };
}

persist_int! {
    u8 => u8, u8;
    u16 => u16, u16;
    u32 => var_u64, var_u64;
    u64 => var_u64, var_u64;
    i64 => var_i64, var_i64;
    i32 => var_i64, var_i64;
}

impl Persist for usize {
    fn put(&self, w: &mut Writer) {
        w.var_u64(*self as u64);
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        usize::try_from(r.var_u64()?).map_err(|_| CodecError::Invalid("usize".into()))
    }
}

impl Persist for bool {
    fn put(&self, w: &mut Writer) {
        w.u8(u8::from(*self));
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => invalid("bool"),
        }
    }
}

impl Persist for f64 {
    fn put(&self, w: &mut Writer) {
        w.f64(*self);
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        r.f64()
    }
}

impl Persist for f32 {
    fn put(&self, w: &mut Writer) {
        w.u32(self.to_bits());
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        r.u32().map(f32::from_bits)
    }
}

impl Persist for String {
    fn put(&self, w: &mut Writer) {
        w.bytes(self.as_bytes());
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        String::from_utf8(r.bytes()?.to_vec()).map_err(|_| CodecError::Invalid("utf-8".into()))
    }
}

impl Persist for i128 {
    fn put(&self, w: &mut Writer) {
        w.raw(&self.to_le_bytes());
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.take(16)?;
        Ok(i128::from_le_bytes(b.try_into().expect("16 bytes")))
    }
}

impl<T: Persist> Persist for Option<T> {
    fn put(&self, w: &mut Writer) {
        match self {
            None => w.u8(0),
            Some(v) => {
                w.u8(1);
                v.put(w);
            }
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        match r.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::get(r)?)),
            _ => invalid("option tag"),
        }
    }
}

impl<T: Persist> Persist for Vec<T> {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for v in self {
            v.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.var_u64()?;
        if n > r.remaining() as u64 {
            return invalid("length past the end");
        }
        (0..n).map(|_| T::get(r)).collect()
    }
}

impl<T: Persist> Persist for VecDeque<T> {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for v in self {
            v.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Vec::<T>::get(r)?.into())
    }
}

impl<K: Persist + Ord, V: Persist> Persist for BTreeMap<K, V> {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for (k, v) in self {
            k.put(w);
            v.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.var_u64()?;
        if n > r.remaining() as u64 {
            return invalid("length past the end");
        }
        let mut m = BTreeMap::new();
        for _ in 0..n {
            let k = K::get(r)?;
            m.insert(k, V::get(r)?);
        }
        Ok(m)
    }
}

impl<K: Persist + Ord> Persist for BTreeSet<K> {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for k in self {
            k.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Vec::<K>::get(r)?.into_iter().collect())
    }
}

impl<K: Persist + Ord + Copy, V: Persist> Persist for VecMap<K, V> {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for (k, v) in self.iter() {
            k.put(w);
            v.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let n = r.var_u64()?;
        if n > r.remaining() as u64 {
            return invalid("length past the end");
        }
        let mut m = VecMap::new();
        for _ in 0..n {
            let k = K::get(r)?;
            m.insert(k, V::get(r)?);
        }
        Ok(m)
    }
}

macro_rules! persist_tuple {
    ($($n:ident $i:tt),+) => {
        impl<$($n: Persist),+> Persist for ($($n,)+) {
            fn put(&self, w: &mut Writer) { $(self.$i.put(w);)+ }
            fn get(r: &mut Reader<'_>) -> Result<Self> { Ok(($($n::get(r)?,)+)) }
        }
    };
}

persist_tuple!(A 0, B 1);
persist_tuple!(A 0, B 1, C 2);
persist_tuple!(A 0, B 1, C 2, D 3);
persist_tuple!(A 0, B 1, C 2, D 3, E 4);
persist_tuple!(A 0, B 1, C 2, D 3, E 4, F 5);

impl<T: Persist, const N: usize> Persist for [T; N] {
    fn put(&self, w: &mut Writer) {
        for v in self {
            v.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let v: Vec<T> = (0..N).map(|_| T::get(r)).collect::<Result<_>>()?;
        v.try_into()
            .map_err(|_| CodecError::Invalid("array length".into()))
    }
}

/// A struct whose fields all persist, in declaration order.
macro_rules! persist_struct {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl $crate::persist::Persist for $t {
            fn put(&self, w: &mut rtsim_kernel::codec::Writer) {
                $($crate::persist::Persist::put(&self.$f, w);)*
            }
            fn get(r: &mut rtsim_kernel::codec::Reader<'_>) -> rtsim_kernel::codec::Result<Self> {
                Ok(Self { $($f: $crate::persist::Persist::get(r)?),* })
            }
        }
    };
}
pub(crate) use persist_struct;

/// A tuple struct of one field.
macro_rules! persist_newtype {
    ($($t:ident),* $(,)?) => {
        $(impl Persist for $t {
            fn put(&self, w: &mut Writer) { self.0.put(w); }
            fn get(r: &mut Reader<'_>) -> Result<Self> { Ok($t(Persist::get(r)?)) }
        })*
    };
}

persist_newtype!(DistrictId, Good, StationId, StoreId, TrainId, Tick);

impl Persist for OpId {
    fn put(&self, w: &mut Writer) {
        w.u64(self.0);
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        r.u64().map(OpId)
    }
}

impl Persist for Diffuser {
    fn put(&self, w: &mut Writer) {
        w.i64(self.rate_fixed());
        w.u32(self.carry());
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let rate = r.i64()?;
        let carry = r.u32()?;
        Ok(Diffuser::with_carry(rate, carry))
    }
}

impl Persist for GoodVec {
    fn put(&self, w: &mut Writer) {
        w.var_u64(self.len() as u64);
        for (g, n) in self.iter() {
            g.put(w);
            n.put(w);
        }
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let v: Vec<(Good, i64)> = Persist::get(r)?;
        Ok(GoodVec::from_pairs(v))
    }
}

persist_struct!(ChunkPos { dim, x, z });
persist_struct!(BlockPos { x, y, z });

impl Persist for GoodClass {
    fn put(&self, w: &mut Writer) {
        w.u8(match self {
            GoodClass::Item => 0,
            GoodClass::Fluid => 1,
            GoodClass::Energy => 2,
            GoodClass::Deposit => 3,
        });
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        Ok(match r.u8()? {
            0 => GoodClass::Item,
            1 => GoodClass::Fluid,
            2 => GoodClass::Energy,
            3 => GoodClass::Deposit,
            _ => return invalid("good class"),
        })
    }
}

persist_struct!(GoodInfo {
    class,
    max_stack,
    name
});

/// A fieldless enum by its index in a list of its variants.
macro_rules! persist_unit_enum {
    ($t:ty, [$($v:expr),* $(,)?]) => {
        impl $crate::persist::Persist for $t {
            fn put(&self, w: &mut rtsim_kernel::codec::Writer) {
                let all = [$($v),*];
                let i = all.iter().position(|x| x == self).expect("listed variant");
                w.u8(i as u8);
            }
            fn get(r: &mut rtsim_kernel::codec::Reader<'_>) -> rtsim_kernel::codec::Result<Self> {
                let all = [$($v),*];
                let i = r.u8()? as usize;
                all.get(i).copied().ok_or_else(|| rtsim_kernel::codec::CodecError::Invalid(format!("{} variant {i}", stringify!($t))))
            }
        }
    };
}
pub(crate) use persist_unit_enum;

/// Encodes a value into bytes.
pub fn to_bytes<T: Persist>(v: &T) -> Vec<u8> {
    let mut w = Writer::new();
    v.put(&mut w);
    w.into_bytes()
}

/// Decodes a value that must fill `b` exactly.
pub fn from_bytes<T: Persist>(b: &[u8]) -> Result<T> {
    let mut r = Reader::new(b);
    let v = T::get(&mut r)?;
    r.finish()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tuple of most leaf kinds at once.
    type Leaves = (
        GoodVec,
        Option<Tick>,
        Vec<(ChunkPos, BTreeSet<Good>)>,
        Diffuser,
        f64,
        String,
    );

    #[test]
    fn leaves_round_trip() {
        let v: Leaves = (
            GoodVec::from_pairs([(Good(3), -7), (Good(1), 1 << 40)]),
            Some(Tick(123)),
            vec![(
                ChunkPos {
                    dim: 2,
                    x: -5,
                    z: 9,
                },
                BTreeSet::from([Good(4), Good(1)]),
            )],
            Diffuser::with_carry(-(5 << 31), 77),
            -0.125,
            "i/64/minecraft:iron_ingot".to_string(),
        );
        let b = to_bytes(&v);
        let back: Leaves = from_bytes(&b).unwrap();
        assert_eq!(back, v);
        assert!(from_bytes::<GoodVec>(&b[..3]).is_err());
    }
}
