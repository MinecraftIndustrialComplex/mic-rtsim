// SPDX-License-Identifier: AGPL-3.0-or-later
//! Generational handles and arenas (SCALE.md §9): a handle is a 32-bit index plus a 16-bit
//! generation, packed into 6 bytes, so a report row can hold two of them in 12 bytes.
//!
//! Generation 0 is never handed out, so six zero bytes ([`Handle::NULL`]) are never a live handle.
//! A zeroed row in a page therefore reads as "nobody", not as the first row ever allocated.
//!
//! When a slot's generation would wrap past `u16::MAX`, the slot is retired instead of reused.
//! Wrapping would let a handle that was stale 65,535 frees ago validate again; retiring costs one
//! dead slot per 65,535 reuses, which is nothing.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::codec::{self, CodecError, Reader, Writer};

/// A typed handle: index plus generation, 6 bytes, alignment 2.
///
/// `T` only tags which arena the handle belongs to; it is never stored.
#[repr(C, packed(2))]
pub struct Handle<T> {
    index: u32,
    generation: u16,
    _marker: PhantomData<fn() -> T>,
}

const _: () = assert!(std::mem::size_of::<Handle<u64>>() == 6);

impl<T> Handle<T> {
    /// Never live in any arena.
    pub const NULL: Self = Self::from_parts(0, 0);

    pub const fn from_parts(index: u32, generation: u16) -> Self {
        Self {
            index,
            generation,
            _marker: PhantomData,
        }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u16 {
        self.generation
    }

    pub const fn is_null(self) -> bool {
        self.generation == 0
    }

    /// The handle as one integer: generation in bits 32..48, index in bits 0..32. Stable, so it
    /// can key RNG streams and be written to pages.
    pub const fn to_bits(self) -> u64 {
        (self.generation as u64) << 32 | self.index as u64
    }

    pub const fn from_bits(bits: u64) -> Self {
        Self::from_parts(bits as u32, (bits >> 32) as u16)
    }

    /// The same handle, tagged for another arena type.
    pub const fn cast<U>(self) -> Handle<U> {
        Handle::from_parts(self.index, self.generation)
    }

    /// The 6-byte wire form, little endian.
    pub fn to_bytes(self) -> [u8; 6] {
        let mut b = [0; 6];
        b[..4].copy_from_slice(&self.index().to_le_bytes());
        b[4..].copy_from_slice(&self.generation().to_le_bytes());
        b
    }

    pub fn from_bytes(b: [u8; 6]) -> Self {
        Self::from_parts(
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u16::from_le_bytes([b[4], b[5]]),
        )
    }
}

// Manual impls: derives would demand `T: Clone` and friends, and `T` is only a tag.
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}
impl<T> Eq for Handle<T> {}
impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Handle<T> {
    /// By index, then generation: the order regions are processed in (SCALE.md §5).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.index(), self.generation()).cmp(&(other.index(), other.generation()))
    }
}
impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_bits().hash(state);
    }
}
impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "#null")
        } else {
            write!(f, "#{}v{}", self.index(), self.generation())
        }
    }
}

/// A generational arena: insert returns a handle, and every lookup checks the generation.
pub struct Arena<T> {
    pub(crate) slots: Vec<Slot<T>>,
    pub(crate) free: Vec<u32>,
    pub(crate) len: usize,
}

pub(crate) struct Slot<T> {
    pub(crate) generation: u16,
    pub(crate) value: Option<T>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            slots: Vec::with_capacity(n),
            free: Vec::new(),
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, value: T) -> Handle<T> {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            debug_assert!(slot.value.is_none() && slot.generation != 0);
            slot.value = Some(value);
            return Handle::from_parts(index, slot.generation);
        }
        let index = u32::try_from(self.slots.len()).expect("arena index space exhausted");
        self.slots.push(Slot {
            generation: 1,
            value: Some(value),
        });
        Handle::from_parts(index, 1)
    }

    pub fn contains(&self, h: Handle<T>) -> bool {
        self.get(h).is_some()
    }

    pub fn get(&self, h: Handle<T>) -> Option<&T> {
        let slot = self.slots.get(h.index() as usize)?;
        if slot.generation != h.generation() {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, h: Handle<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(h.index() as usize)?;
        if slot.generation != h.generation() {
            return None;
        }
        slot.value.as_mut()
    }

    pub fn remove(&mut self, h: Handle<T>) -> Option<T> {
        let slot = self.slots.get_mut(h.index() as usize)?;
        if slot.generation != h.generation() {
            return None;
        }
        let value = slot.value.take()?;
        self.len -= 1;
        if slot.generation == u16::MAX {
            // Retired: see the module docs.
            slot.generation = 0;
        } else {
            slot.generation += 1;
            self.free.push(h.index());
        }
        Some(value)
    }

    /// Live entries in index order, which is deterministic for a given history of inserts and
    /// removes.
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.value
                .as_ref()
                .map(|v| (Handle::from_parts(i as u32, s.generation), v))
        })
    }

    /// Live entries whose index is in `indices`, in index order.
    pub fn iter_range(
        &self,
        indices: std::ops::Range<u64>,
    ) -> impl Iterator<Item = (Handle<T>, &T)> {
        let n = self.slots.len() as u64;
        let (lo, hi) = (indices.start.min(n) as usize, indices.end.min(n) as usize);
        self.slots[lo..hi.max(lo)]
            .iter()
            .enumerate()
            .filter_map(move |(i, s)| {
                s.value
                    .as_ref()
                    .map(|v| (Handle::from_parts((lo + i) as u32, s.generation), v))
            })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(i, s)| {
            let generation = s.generation;
            s.value
                .as_mut()
                .map(|v| (Handle::from_parts(i as u32, generation), v))
        })
    }

    /// Slots ever allocated, live or not: the bound on indices.
    pub fn capacity_slots(&self) -> usize {
        self.slots.len()
    }

    /// Bytes this arena holds on the heap, for the memory budget (A16). Excludes whatever `T`
    /// itself points to.
    pub fn heap_bytes(&self) -> usize {
        self.slots.capacity() * std::mem::size_of::<Slot<T>>()
            + self.free.capacity() * std::mem::size_of::<u32>()
    }

    /// Writes the whole arena, free slots and the free list included, so [`Arena::decode_with`]
    /// gives back one that hands out the same handles from then on (saves: handles are ids that
    /// other rows and the bridge keep).
    pub fn encode_with(&self, w: &mut Writer, mut f: impl FnMut(&T, &mut Writer)) {
        w.var_u64(self.slots.len() as u64);
        for s in &self.slots {
            w.u16(s.generation);
            match &s.value {
                Some(v) => {
                    w.u8(1);
                    f(v, w);
                }
                None => w.u8(0),
            }
        }
        w.var_u64(self.free.len() as u64);
        for i in &self.free {
            w.u32(*i);
        }
    }

    /// Reads what [`Arena::encode_with`] wrote; checks that the free list names empty, live
    /// generations once each.
    pub fn decode_with(
        r: &mut Reader<'_>,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<T>,
    ) -> codec::Result<Self> {
        let n = r.len_prefix(3)?;
        let mut slots = Vec::with_capacity(n);
        let mut len = 0;
        for _ in 0..n {
            let generation = r.u16()?;
            let value = match r.u8()? {
                0 => None,
                1 => {
                    len += 1;
                    Some(f(r)?)
                }
                t => return Err(CodecError::Invalid(format!("arena slot tag {t}"))),
            };
            if value.is_some() && generation == 0 {
                return Err(CodecError::Invalid(
                    "a live arena slot of generation 0".into(),
                ));
            }
            slots.push(Slot { generation, value });
        }
        let nf = r.len_prefix(4)?;
        let mut free = Vec::with_capacity(nf);
        let mut seen = vec![false; slots.len()];
        for _ in 0..nf {
            let i = r.u32()?;
            let ok = slots
                .get(i as usize)
                .is_some_and(|s: &Slot<T>| s.value.is_none() && s.generation != 0)
                && !std::mem::replace(&mut seen[i as usize], true);
            if !ok {
                return Err(CodecError::Invalid(format!(
                    "arena free list names slot {i}"
                )));
            }
            free.push(i);
        }
        Ok(Arena { slots, free, len })
    }
}

/// `arena[h]` for a handle the caller knows is live; panics on a stale or foreign one, as slice
/// indexing panics out of bounds. Use [`Arena::get`] when the handle may be stale.
impl<T> std::ops::Index<Handle<T>> for Arena<T> {
    type Output = T;

    fn index(&self, h: Handle<T>) -> &T {
        self.get(h).expect("stale or foreign handle")
    }
}

impl<T> std::ops::IndexMut<Handle<T>> for Arena<T> {
    fn index_mut(&mut self, h: Handle<T>) -> &mut T {
        self.get_mut(h).expect("stale or foreign handle")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Reader, Writer};
    use proptest::prelude::*;

    #[test]
    fn iter_range_keeps_to_its_indices() {
        let mut a = Arena::new();
        let hs: Vec<_> = (0..8u32).map(|i| a.insert(i)).collect();
        a.remove(hs[4]);
        let got: Vec<u32> = a.iter_range(2..6).map(|(_, v)| *v).collect();
        assert_eq!(got, vec![2, 3, 5]);
        assert_eq!(a.iter_range(6..100).count(), 2);
        assert_eq!(a.iter_range(50..60).count(), 0);
    }

    #[test]
    fn handle_is_six_bytes_and_null_is_zero() {
        assert_eq!(std::mem::size_of::<Handle<String>>(), 6);
        assert_eq!(std::mem::align_of::<Handle<String>>(), 2);
        assert_eq!(Handle::<u8>::NULL.to_bytes(), [0; 6]);
        assert!(Handle::<u8>::from_bytes([0; 6]).is_null());
    }

    #[test]
    fn stale_handles_do_not_validate() {
        let mut a = Arena::new();
        let h1 = a.insert("a");
        assert_eq!(a.remove(h1), Some("a"));
        let h2 = a.insert("b");
        assert_eq!(h1.index(), h2.index());
        assert_ne!(h1.generation(), h2.generation());
        assert_eq!(a.get(h1), None);
        assert_eq!(a.get(h2), Some(&"b"));
        assert_eq!(a.remove(h1), None);
        assert!(!a.contains(Handle::NULL));
    }

    #[test]
    fn a_slot_retires_instead_of_wrapping() {
        let mut a = Arena::new();
        let mut h = a.insert(0u32);
        let first = h;
        for i in 1..=u32::from(u16::MAX) {
            a.remove(h).unwrap();
            h = a.insert(i);
            if h.index() != first.index() {
                // The slot retired after generation u16::MAX was freed.
                assert_eq!(i, u32::from(u16::MAX));
                assert!(a.get(first).is_none());
                return;
            }
        }
        panic!("slot never retired");
    }

    proptest! {
        /// An arena saved after any history of inserts and removes hands out the same handles as
        /// the original from then on.
        #[test]
        fn a_saved_arena_continues_as_the_original(ops in prop::collection::vec((0u8..2, any::<u16>()), 1..300), more in prop::collection::vec((0u8..2, any::<u16>()), 1..100)) {
            let mut a: Arena<u32> = Arena::new();
            let mut live: Vec<Handle<u32>> = Vec::new();
            for (op, x) in ops {
                if op == 0 || live.is_empty() {
                    live.push(a.insert(u32::from(x)));
                } else {
                    let h = live.swap_remove(x as usize % live.len());
                    a.remove(h);
                }
            }
            let mut w = Writer::new();
            a.encode_with(&mut w, |v, w| w.u32(*v));
            let bytes = w.into_bytes();
            let mut r = Reader::new(&bytes);
            let mut b = Arena::decode_with(&mut r, |r| r.u32()).unwrap();
            r.finish().unwrap();
            prop_assert_eq!(a.len(), b.len());
            for (op, x) in more {
                if op == 0 || live.is_empty() {
                    let (ha, hb) = (a.insert(u32::from(x)), b.insert(u32::from(x)));
                    prop_assert_eq!(ha, hb);
                    live.push(ha);
                } else {
                    let h = live.swap_remove(x as usize % live.len());
                    prop_assert_eq!(a.remove(h), b.remove(h));
                }
            }
            let ea: Vec<_> = a.iter().map(|(h, v)| (h, *v)).collect();
            let eb: Vec<_> = b.iter().map(|(h, v)| (h, *v)).collect();
            prop_assert_eq!(ea, eb);
        }
    }

    proptest! {
        /// The arena agrees with a model that remembers every handle ever issued.
        #[test]
        fn arena_matches_model(ops in prop::collection::vec((0u8..3, any::<u16>()), 1..400)) {
            let mut a = Arena::new();
            let mut issued: Vec<(Handle<u32>, Option<u32>)> = Vec::new();
            for (op, x) in ops {
                match op {
                    0 => {
                        let h = a.insert(u32::from(x));
                        prop_assert!(issued.iter().all(|(o, _)| *o != h));
                        issued.push((h, Some(u32::from(x))));
                    }
                    1 if !issued.is_empty() => {
                        let i = x as usize % issued.len();
                        let (h, v) = issued[i];
                        prop_assert_eq!(a.remove(h), v);
                        issued[i].1 = None;
                    }
                    _ if !issued.is_empty() => {
                        let i = x as usize % issued.len();
                        let (h, v) = issued[i];
                        prop_assert_eq!(a.get(h).copied(), v);
                    }
                    _ => {}
                }
                prop_assert_eq!(a.len(), issued.iter().filter(|(_, v)| v.is_some()).count());
            }
        }
    }
}
