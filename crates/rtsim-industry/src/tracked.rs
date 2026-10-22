// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the industry engine's pages are made of, with a record of what changed since the last save
//! (docs/choices-industry-dirty-pages.md ID1 to ID9): a save writes only the pages changed since
//! the one before, so a quiet district costs nothing per save.
//!
//! - [`Tracked`] is a map whose every entry is a page. It reads as the `BTreeMap` it holds
//!   (`Deref`), and every way to change an entry, insert it or remove it marks its key. There is
//!   no `DerefMut`: a `&mut` to an entry exists only through a method that marks it.
//! - [`TrackedTable`] is the same for the replay clusters' `LazyTable`: an event, a breakpoint, an
//!   insert or a removal marks the region's key; a read marks it only when the read moved the
//!   region's anchor (a model with slots commits them on a read).
//!
//! A mark is conservative: a `get_mut` that changes nothing still marks. The engine's page check
//! (`Engine::save_changed` in a debug build, or with `RTSIM_VERIFY_PAGES` set) hashes every page
//! at every save and counts both errors: a page that changed unmarked, and a marked one that did
//! not change.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::lazy::{Lazy, LazyModel, LazyTable, Peek, Region, RegionId};
use rtsim_kernel::rng::SystemId;

/// A map whose entries are pages, with the keys changed since [`Tracked::take_changed`].
#[derive(Debug)]
pub struct Tracked<K: Ord + Copy, V> {
    map: BTreeMap<K, V>,
    changed: BTreeSet<K>,
}

impl<K: Ord + Copy, V> Default for Tracked<K, V> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            changed: BTreeSet::new(),
        }
    }
}

impl<K: Ord + Copy, V> Deref for Tracked<K, V> {
    type Target = BTreeMap<K, V>;
    fn deref(&self) -> &BTreeMap<K, V> {
        &self.map
    }
}

impl<K: Ord + Copy, V> Tracked<K, V> {
    /// A map as a restore read it from its pages: nothing changed.
    pub fn restored(map: BTreeMap<K, V>) -> Self {
        Self {
            map,
            changed: BTreeSet::new(),
        }
    }

    /// The entry, mutably; marks it (whether or not the caller changes it).
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let v = self.map.get_mut(k)?;
        self.changed.insert(*k);
        Some(v)
    }

    /// The entry, made with `f` if missing; marks it.
    pub fn get_or_insert_with(&mut self, k: K, f: impl FnOnce() -> V) -> &mut V {
        self.changed.insert(k);
        self.map.entry(k).or_insert_with(f)
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.changed.insert(k);
        self.map.insert(k, v)
    }

    /// Removes the entry; its key is marked, so the next save deletes its page.
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let v = self.map.remove(k)?;
        self.changed.insert(*k);
        Some(v)
    }

    /// Every entry, mutably; marks every key.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.changed.extend(self.map.keys().copied());
        self.map.values_mut()
    }

    /// Marks `k` (a failed save's page, written again by the next one).
    pub fn mark(&mut self, k: K) {
        self.changed.insert(k);
    }

    pub fn changed(&self) -> &BTreeSet<K> {
        &self.changed
    }

    /// The keys changed since the last call: each one's page is written, or deleted when the
    /// entry is gone.
    pub fn take_changed(&mut self) -> BTreeSet<K> {
        std::mem::take(&mut self.changed)
    }
}

/// A `LazyTable` whose regions are pages, with the regions changed since
/// [`TrackedTable::take_changed`], by key (the page id). The handle kept with a key finds the
/// region at save time; a key whose handle no longer reaches a region of that key is a removed
/// region, whose page the save deletes.
pub struct TrackedTable<M: LazyModel> {
    table: LazyTable<M>,
    changed: BTreeMap<u64, RegionId<M>>,
}

impl<M: LazyModel> TrackedTable<M> {
    pub fn new(world_seed: u64, system: SystemId, now: Tick) -> Self {
        Self {
            table: LazyTable::new(world_seed, system, now),
            changed: BTreeMap::new(),
        }
    }

    fn mark_handle(&mut self, h: RegionId<M>) {
        if let Some(r) = self.table.get(h) {
            self.changed.insert(r.key(), h);
        }
    }

    /// Marks the region of `key` (a failed save's page). An entry already there is newer: kept.
    pub fn mark(&mut self, key: u64, h: RegionId<M>) {
        self.changed.entry(key).or_insert(h);
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn heap_bytes(&self) -> usize {
        self.table.heap_bytes()
    }

    pub fn get(&self, h: RegionId<M>) -> Option<&Region<M>> {
        self.table.get(h)
    }

    pub fn iter(&self) -> impl Iterator<Item = (RegionId<M>, &Region<M>)> {
        self.table.iter()
    }

    pub fn insert(&mut self, key: u64, model: M, at: Tick) -> RegionId<M> {
        let h = self.table.insert(key, model, at);
        self.changed.insert(key, h);
        h
    }

    /// A region read from its page: not changed.
    pub fn restore(&mut self, key: u64, cell: Lazy<M>, wake: Option<Tick>) -> RegionId<M> {
        self.table.restore(key, cell, wake)
    }

    pub fn remove(&mut self, h: RegionId<M>) -> Option<(u64, Lazy<M>)> {
        let (key, cell) = self.table.remove(h)?;
        self.changed.insert(key, h);
        Some((key, cell))
    }

    /// `LazyTable::read`. A model without slots changes nothing on a read (its anchor stays, and
    /// `touched` is not saved); one whose read committed a slot is marked.
    pub fn read(&mut self, h: RegionId<M>, at: Tick) -> Option<M::View> {
        let before = self.table.get(h)?.cell().anchor();
        let v = self.table.read(h, at);
        if self.table.get(h).is_some_and(|r| r.cell().anchor() != before) {
            self.mark_handle(h);
        }
        v
    }

    pub fn peek(&self, h: RegionId<M>, at: Tick) -> Option<Peek<M::View>>
    where
        M: Clone,
    {
        self.table.peek(h, at)
    }

    pub fn apply<R>(
        &mut self,
        h: RegionId<M>,
        at: Tick,
        f: impl FnOnce(&mut M, Tick) -> R,
    ) -> Option<R> {
        let r = self.table.apply(h, at, f)?;
        self.mark_handle(h);
        Some(r)
    }

    /// `LazyTable::next_due`: the region it hands back was settled and booked again, so marked.
    pub fn next_due(&mut self, limit: Tick) -> Option<(Tick, RegionId<M>)> {
        let (at, h) = self.table.next_due(limit)?;
        self.mark_handle(h);
        Some((at, h))
    }

    pub fn changed(&self) -> &BTreeMap<u64, RegionId<M>> {
        &self.changed
    }

    /// The regions changed since the last call, by key.
    pub fn take_changed(&mut self) -> BTreeMap<u64, RegionId<M>> {
        std::mem::take(&mut self.changed)
    }

    /// The region of `key` if `h` still reaches it (else it was removed).
    pub fn live(&self, key: u64, h: RegionId<M>) -> Option<&Region<M>> {
        self.table.get(h).filter(|r| r.key() == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_change_marks_its_key_and_reads_mark_nothing() {
        let mut t: Tracked<u32, u32> = Tracked::default();
        t.insert(1, 10);
        t.insert(2, 20);
        assert_eq!(t.take_changed(), BTreeSet::from([1, 2]));
        assert_eq!(t.get(&1), Some(&10));
        assert!(t.contains_key(&2) && t.len() == 2);
        assert!(t.changed().is_empty(), "reads mark nothing");
        *t.get_mut(&2).expect("there") += 1;
        assert!(t.get_mut(&3).is_none(), "a missing key is not marked");
        assert_eq!(t.take_changed(), BTreeSet::from([2]));
        t.remove(&1);
        t.remove(&9);
        assert_eq!(t.take_changed(), BTreeSet::from([1]));
        t.get_or_insert_with(4, || 40);
        t.values_mut().for_each(|v| *v += 1);
        assert_eq!(t.take_changed(), BTreeSet::from([2, 4]));
        let r = Tracked::restored(BTreeMap::from([(7u32, 70u32)]));
        assert!(r.changed().is_empty(), "a restored map starts clean");
    }
}
