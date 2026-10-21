// SPDX-License-Identifier: AGPL-3.0-or-later
//! Resident structures saved page by page: a timing wheel and an ordered map that stay whole in
//! memory, as before, but whose saves write only what changed since the last one, so a save with
//! nothing changed costs the same at any entity count (docs/choices-paging-followups.md PF5 to
//! PF7).
//!
//! **The wheel** ([`PagedWheel`]) is saved as a log. Every entry gets a sequence number when it is
//! scheduled. A save appends the entries scheduled since the last one to an open tail page, seals
//! the tail into pages of [`WHEEL_PAGE_ENTRIES`] once it is full, and deletes every sealed page
//! whose entries have all popped. Pops come in (time, sequence) order, so the last popped entry's
//! key is a watermark: an entry is pending exactly when its key is above it, and a page whose
//! largest key is at or below it holds nothing pending. Popped entries in a page that still holds a
//! pending one are garbage; once the garbage outweighs the pending entries (plus [`WHEEL_SLACK`]),
//! the save writes the pending entries into fresh pages, so a pop costs at most one entry written,
//! amortized.
//!
//! **The map** ([`PagedMap`]) is split into pages by key range. A change marks its key's page; a
//! save writes the marked pages, splits a page grown past twice [`MAP_PAGE_ENTRIES`], and deletes
//! a page left empty (its range joins the page before it).
//!
//! Each structure writes a small header, its page list, into the module's own meta page and opens
//! from it. Its pages live in a table of their own. Before the first paged save nothing is tracked:
//! a resident module pays one sequence number per wheel entry and nothing per map entry.
//!
//! **The arena** ([`ResidentArena`]) is a [`Arena`] in pages of [`ARENA_PAGE_SLOTS`] slots: a
//! change marks its slot's page, and the free list is saved as a stack of pages, of which a save
//! writes those from the lowest depth it reached since the last save.
//!
//! **A failed save** leaves the store as the last good save left it: the wheel's next save writes
//! every pending entry into fresh pages and deletes every page it knew, and the map's next save
//! writes every page again and deletes the pages the failed save would have, as the row tables do.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Bound, Deref};

use crate::clock::Tick;
use crate::codec::{self, CodecError, Reader, Writer};
use crate::handle::{Arena, Handle, Slot};
use crate::page::{PageKey, SaveBatch};
use crate::rows::PageSource;
use crate::wheel::TimingWheel;

/// Entries a sealed wheel page holds. PLACEHOLDER 1,024: a page of 10 to 30 KB raw. (The
/// kernel's own tests use small pages, so their histories seal, split and rewrite often.)
pub const WHEEL_PAGE_ENTRIES: usize = if cfg!(test) { 64 } else { 1_024 };
/// Garbage a wheel's pages may hold beyond its pending entries before a save rewrites them.
/// PLACEHOLDER 4,096: a small wheel is not rewritten every few saves.
pub const WHEEL_SLACK: usize = if cfg!(test) { 128 } else { 4_096 };
/// Entries a map page holds after a split; a page splits past twice this. PLACEHOLDER 512.
pub const MAP_PAGE_ENTRIES: usize = if cfg!(test) { 32 } else { 512 };

/// Slots an arena page holds. PLACEHOLDER 64: a page of a few KB for rows of tens of bytes.
pub const ARENA_PAGE_SLOTS: usize = if cfg!(test) { 8 } else { 64 };
/// Arena page ids from here up hold the free list.
const FREE_PAGES: u64 = 1 << 40;

/// (due tick, sequence number): the order entries pop in.
type Key = (u64, u64);

fn page_read(source: &dyn PageSource, table: u16, id: u64) -> codec::Result<Vec<u8>> {
    source
        .read(PageKey::new(table, id))
        .map_err(|e| CodecError::Invalid(format!("page {table:#x}/{id}: {e}")))?
        .ok_or_else(|| CodecError::Invalid(format!("page {table:#x}/{id} is missing")))
}

// ---- the wheel ----------------------------------------------------------------------------------

/// A [`TimingWheel`] whose saves append to a log of pages (module docs). Same operations, same
/// pop order.
pub struct PagedWheel<T> {
    wheel: TimingWheel<(u64, T)>,
    next_seq: u64,
    /// The key of the last entry popped; `(0, 0)` before any (sequence numbers start at 1).
    popped: Key,
    log: Option<Log<T>>,
}

struct Log<T> {
    sealed: Vec<Sealed>,
    /// The page that holds `tail` as of the last save.
    open: Option<u64>,
    /// The open page's entries and every entry scheduled since the last save, in sequence order
    /// (after a rewrite, in pop order).
    tail: Vec<(Key, T)>,
    next_page: u64,
    /// Whether `tail` changed since the last save, so its page must be written again.
    tail_dirty: bool,
    rewrite: bool,
    /// Pages a failed save may have left or failed to delete: the next rewrite deletes them.
    stale: Vec<u64>,
}

#[derive(Clone, Copy, Debug)]
struct Sealed {
    id: u64,
    n: usize,
    max: Key,
}

impl<T> Log<T> {
    fn fresh(&mut self) -> u64 {
        let id = self.next_page;
        self.next_page += 1;
        id
    }
}

/// What a wheel's save wrote and deleted, for [`PagedWheel::save_failed`].
#[derive(Debug, Default)]
#[must_use]
pub struct WheelTicket {
    written: Vec<u64>,
    deleted: Vec<u64>,
}

impl WheelTicket {
    /// Pages written.
    pub fn pages(&self) -> usize {
        self.written.len()
    }
}

impl<T> Default for PagedWheel<T> {
    fn default() -> Self {
        Self::new(Tick::ZERO)
    }
}

impl<T> std::fmt::Debug for PagedWheel<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagedWheel")
            .field("now", &self.now())
            .field("len", &self.len())
            .field("paged", &self.log.is_some())
            .finish()
    }
}

impl<T> PagedWheel<T> {
    pub fn new(now: Tick) -> Self {
        PagedWheel {
            wheel: TimingWheel::new(now),
            next_seq: 1,
            popped: (0, 0),
            log: None,
        }
    }

    pub fn now(&self) -> Tick {
        self.wheel.now()
    }

    pub fn len(&self) -> usize {
        self.wheel.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wheel.is_empty()
    }

    /// Whether saves go to pages (after the first [`PagedWheel::save_paged`] or an open).
    pub fn is_paged(&self) -> bool {
        self.log.is_some()
    }

    /// See [`TimingWheel::next_due_lower_bound`].
    pub fn next_due_lower_bound(&self) -> Option<Tick> {
        self.wheel.next_due_lower_bound()
    }

    /// See [`TimingWheel::next_due`].
    pub fn next_due(&self) -> Option<Tick> {
        self.wheel.next_due()
    }

    /// See [`TimingWheel::pop_until`].
    pub fn pop_until(&mut self, limit: Tick) -> Option<(Tick, T)> {
        let (at, (seq, item)) = self.wheel.pop_until(limit)?;
        debug_assert!((at.0, seq) > self.popped);
        self.popped = (at.0, seq);
        Some((at, item))
    }

    /// Every pending entry, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (Tick, &T)> {
        self.wheel.iter().map(|(at, (_, x))| (at, x))
    }

    fn keyed(&self) -> Vec<(Key, &T)> {
        let mut v: Vec<(Key, &T)> = self
            .wheel
            .iter()
            .map(|(at, (seq, x))| ((at.0, *seq), x))
            .collect();
        v.sort_unstable_by_key(|e| e.0);
        v
    }

    /// Every pending entry in the order it will pop.
    pub fn in_pop_order(&self) -> Vec<(Tick, &T)> {
        self.keyed()
            .into_iter()
            .map(|(k, x)| (Tick(k.0), x))
            .collect()
    }

    /// Bytes held on the heap, for the memory budget.
    pub fn heap_bytes(&self) -> usize {
        self.wheel.heap_bytes()
            + self.log.as_ref().map_or(0, |l| {
                l.tail.capacity() * std::mem::size_of::<(Key, T)>()
                    + (l.sealed.capacity() + l.stale.capacity()) * std::mem::size_of::<Sealed>()
            })
    }

    /// The whole wheel as one blob, in [`TimingWheel::encode_with`]'s format.
    pub fn encode_with(&self, w: &mut Writer, mut f: impl FnMut(&T, &mut Writer)) {
        let now = self.wheel.now().0;
        w.u64(now);
        let v = self.keyed();
        w.var_u64(v.len() as u64);
        for ((at, _), item) in v {
            w.var_u64(at - now);
            f(item, w);
        }
    }

    /// Reads [`PagedWheel::encode_with`]'s blob (or a [`TimingWheel`]'s); entries get fresh
    /// sequence numbers in pop order.
    pub fn decode_with(
        r: &mut Reader<'_>,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<T>,
    ) -> codec::Result<Self> {
        let now = r.u64()?;
        let mut out = PagedWheel::new(Tick(now));
        let n = r.len_prefix(1)?;
        let mut last = now;
        for _ in 0..n {
            let at = now
                .checked_add(r.var_u64()?)
                .ok_or_else(|| CodecError::Invalid("wheel entry past the end of time".into()))?;
            if at < last {
                return Err(CodecError::Invalid("wheel entries out of order".into()));
            }
            last = at;
            let item = f(r)?;
            let seq = out.next_seq;
            out.next_seq += 1;
            out.wheel.schedule(Tick(at), (seq, item));
        }
        Ok(out)
    }
}

impl<T: Clone> PagedWheel<T> {
    /// See [`TimingWheel::schedule`].
    pub fn schedule(&mut self, at: Tick, item: T) {
        let seq = self.next_seq;
        self.next_seq += 1;
        if let Some(log) = &mut self.log {
            log.tail
                .push(((at.0.max(self.wheel.now().0), seq), item.clone()));
            log.tail_dirty = true;
        }
        self.wheel.schedule(at, (seq, item));
    }

    /// Writes the wheel's pages changed since the last save into `batch` (table `table`, whose ids
    /// the wheel owns) and its header into `w`, the module's meta page. The first paged save
    /// writes every entry.
    pub fn save_paged(
        &mut self,
        batch: &mut SaveBatch,
        table: u16,
        w: &mut Writer,
        mut f: impl FnMut(&T, &mut Writer),
    ) -> WheelTicket {
        let popped = self.popped;
        let live = self.wheel.len();
        let mut t = WheelTicket::default();
        let log = self.log.get_or_insert_with(|| Log {
            sealed: Vec::new(),
            open: None,
            tail: Vec::new(),
            next_page: 0,
            tail_dirty: true,
            rewrite: true,
            stale: Vec::new(),
        });
        let mut delete = |id: u64, t: &mut WheelTicket| {
            batch.delete(PageKey::new(table, id));
            t.deleted.push(id);
        };
        if !log.rewrite {
            let before = log.tail.len();
            log.tail.retain(|(k, _)| *k > popped);
            log.tail_dirty |= log.tail.len() != before;
            let stored = log.sealed.iter().map(|p| p.n).sum::<usize>() + log.tail.len();
            debug_assert!(stored >= live, "a pending wheel entry is in no page");
            if stored.saturating_sub(live) > live + WHEEL_SLACK {
                log.rewrite = true;
            }
        }
        if log.rewrite {
            let old: Vec<u64> = log
                .sealed
                .drain(..)
                .map(|p| p.id)
                .chain(log.open.take())
                .chain(log.stale.drain(..))
                .collect();
            for id in old {
                delete(id, &mut t);
            }
            let mut all: Vec<(Key, T)> = self
                .wheel
                .iter()
                .map(|(at, (seq, x))| ((at.0, *seq), x.clone()))
                .collect();
            all.sort_unstable_by_key(|e| e.0);
            log.tail = all;
            log.tail_dirty = true;
            log.rewrite = false;
        } else {
            let mut gone = Vec::new();
            log.sealed.retain(|p| {
                let keep = p.max > popped;
                if !keep {
                    gone.push(p.id);
                }
                keep
            });
            for id in gone {
                delete(id, &mut t);
            }
        }
        let mut put = |id: u64, entries: &[(Key, T)], t: &mut WheelTicket| {
            let mut pw = Writer::with_capacity(16 + entries.len() * 16);
            pw.var_u64(entries.len() as u64);
            for ((at, seq), item) in entries {
                pw.u64(*at);
                pw.var_u64(*seq);
                f(item, &mut pw);
            }
            batch.put(PageKey::new(table, id), pw.as_bytes());
            t.written.push(id);
        };
        let full = log.tail.len() / WHEEL_PAGE_ENTRIES * WHEEL_PAGE_ENTRIES;
        for start in (0..full).step_by(WHEEL_PAGE_ENTRIES) {
            let id = match log.open.take() {
                Some(id) => id,
                None => log.fresh(),
            };
            let chunk = &log.tail[start..start + WHEEL_PAGE_ENTRIES];
            put(id, chunk, &mut t);
            let max = chunk.iter().map(|e| e.0).max().expect("a full chunk");
            log.sealed.push(Sealed {
                id,
                n: chunk.len(),
                max,
            });
        }
        log.tail.drain(..full);
        if log.tail.is_empty() {
            if let Some(id) = log.open.take() {
                batch.delete(PageKey::new(table, id));
                t.deleted.push(id);
            }
        } else if log.tail_dirty || log.open.is_none() {
            let id = match log.open {
                Some(id) => id,
                None => log.fresh(),
            };
            log.open = Some(id);
            put(id, &log.tail, &mut t);
        }
        log.tail_dirty = false;
        w.u64(self.wheel.now().0);
        w.var_u64(live as u64);
        w.var_u64(self.next_seq);
        w.u64(popped.0);
        w.var_u64(popped.1);
        w.var_u64(log.next_page);
        w.var_u64(log.open.map_or(0, |id| id + 1));
        w.var_u64(log.sealed.len() as u64);
        for p in &log.sealed {
            w.var_u64(p.id);
            w.var_u64(p.n as u64);
            w.u64(p.max.0);
            w.var_u64(p.max.1);
        }
        t
    }

    /// The commit holding the ticket's pages is durable. Nothing to do: kept for symmetry with
    /// the row tables.
    pub fn saved(&mut self, _t: WheelTicket) {}

    /// The commit failed: the next save writes every pending entry afresh.
    pub fn save_failed(&mut self, t: WheelTicket) {
        if let Some(log) = &mut self.log {
            log.rewrite = true;
            log.stale.extend(t.written);
            log.stale.extend(t.deleted);
        }
    }

    /// A wheel from the header [`PagedWheel::save_paged`] wrote (read from `r`) and its pages in
    /// `source`. Pops exactly as the saved wheel would have.
    pub fn open_paged(
        r: &mut Reader<'_>,
        table: u16,
        source: &dyn PageSource,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<T>,
    ) -> codec::Result<Self> {
        let bad = |what: &str| Err(CodecError::Invalid(format!("wheel pages: {what}")));
        let now = r.u64()?;
        let live = r.var_u64()?;
        let next_seq = r.var_u64()?;
        let popped = (r.u64()?, r.var_u64()?);
        let next_page = r.var_u64()?;
        let open = r.var_u64()?.checked_sub(1);
        let n = r.len_prefix(11)?;
        let mut sealed = Vec::with_capacity(n);
        for _ in 0..n {
            sealed.push(Sealed {
                id: r.var_u64()?,
                n: usize::try_from(r.var_u64()?)
                    .map_err(|_| CodecError::Invalid("a page count past usize".into()))?,
                max: (r.u64()?, r.var_u64()?),
            });
        }
        let mut read = |id: u64, out: &mut Vec<(Key, T)>| -> codec::Result<usize> {
            let raw = page_read(source, table, id)?;
            let mut pr = Reader::new(&raw);
            let n = pr.len_prefix(9)?;
            for _ in 0..n {
                let k = (pr.u64()?, pr.var_u64()?);
                let item = f(&mut pr)?;
                if k > popped {
                    out.push((k, item));
                }
            }
            pr.finish()?;
            Ok(n)
        };
        let mut all = Vec::new();
        for p in &sealed {
            if p.id >= next_page || read(p.id, &mut all)? != p.n {
                return bad("a sealed page differs from its header");
            }
        }
        let mut tail = Vec::new();
        if let Some(id) = open {
            if id >= next_page {
                return bad("the open page is past the next page id");
            }
            read(id, &mut tail)?;
        }
        all.extend(tail.iter().cloned());
        if all.len() as u64 != live {
            return bad("the pending entries differ from the header's count");
        }
        all.sort_unstable_by_key(|e| e.0);
        if all.windows(2).any(|w| w[0].0 == w[1].0) {
            return bad("two entries share a key");
        }
        if all.first().is_some_and(|e| e.0.0 < now) || all.last().is_some_and(|e| e.0.1 >= next_seq)
        {
            return bad("an entry before the wheel's time or past its next sequence number");
        }
        let mut wheel = TimingWheel::new(Tick(now));
        for ((at, seq), item) in all {
            wheel.schedule(Tick(at), (seq, item));
        }
        Ok(PagedWheel {
            wheel,
            next_seq,
            popped,
            log: Some(Log {
                sealed,
                open,
                tail,
                next_page,
                tail_dirty: false,
                rewrite: false,
                stale: Vec::new(),
            }),
        })
    }
}

// ---- the map ------------------------------------------------------------------------------------

/// A `BTreeMap` whose saves write only the key ranges changed since the last one (module docs).
/// Reads go through `Deref`; changes go through its own methods, which mark the page.
#[derive(Clone)]
pub struct PagedMap<K, V> {
    map: BTreeMap<K, V>,
    pages: Option<MapPages<K>>,
}

#[derive(Clone)]
struct MapPages<K> {
    /// By lower bound; the first page's is `None`, so every key has a page.
    list: Vec<MapPage<K>>,
    next_page: u64,
    stale: Vec<u64>,
}

#[derive(Clone)]
struct MapPage<K> {
    lower: Option<K>,
    id: u64,
    dirty: bool,
}

/// What a map's save deleted, for [`PagedMap::save_failed`].
#[derive(Debug, Default)]
#[must_use]
pub struct MapTicket {
    written: usize,
    deleted: Vec<u64>,
}

impl MapTicket {
    /// Pages written.
    pub fn pages(&self) -> usize {
        self.written
    }
}

impl<K, V> Default for PagedMap<K, V> {
    fn default() -> Self {
        PagedMap {
            map: BTreeMap::new(),
            pages: None,
        }
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for PagedMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.map.fmt(f)
    }
}

impl<K, V> Deref for PagedMap<K, V> {
    type Target = BTreeMap<K, V>;

    fn deref(&self) -> &BTreeMap<K, V> {
        &self.map
    }
}

impl<'a, K, V> IntoIterator for &'a PagedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::btree_map::Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<K: Ord, V> From<BTreeMap<K, V>> for PagedMap<K, V> {
    fn from(map: BTreeMap<K, V>) -> Self {
        PagedMap { map, pages: None }
    }
}

impl<K: Ord + Clone, V> PagedMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether saves go to pages (after the first [`PagedMap::save_paged`] or an open).
    pub fn is_paged(&self) -> bool {
        self.pages.is_some()
    }

    fn mark(&mut self, k: &K) {
        if let Some(p) = &mut self.pages {
            let i = p
                .list
                .partition_point(|pg| pg.lower.as_ref().is_none_or(|l| l <= k));
            p.list[i - 1].dirty = true;
        }
    }

    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        self.mark(&k);
        self.map.insert(k, v)
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        let v = self.map.remove(k)?;
        self.mark(k);
        Some(v)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        if self.map.contains_key(k) {
            self.mark(k);
        }
        self.map.get_mut(k)
    }

    /// `entry(k).or_insert_with(f)`.
    pub fn get_or_insert_with(&mut self, k: K, f: impl FnOnce() -> V) -> &mut V {
        self.mark(&k);
        self.map.entry(k).or_insert_with(f)
    }

    /// `entry(k).or_default()`.
    pub fn get_or_default(&mut self, k: K) -> &mut V
    where
        V: Default,
    {
        self.get_or_insert_with(k, V::default)
    }

    /// Removes every entry: every page is written (empty) by the next save.
    pub fn clear(&mut self) {
        self.map.clear();
        if let Some(p) = &mut self.pages {
            p.list.iter_mut().for_each(|pg| pg.dirty = true);
        }
    }

    /// The whole map with its pages forgotten.
    pub fn into_inner(self) -> BTreeMap<K, V> {
        self.map
    }

    /// Bytes the page list holds (the entries are the caller's to count).
    pub fn heap_bytes(&self) -> usize {
        self.pages.as_ref().map_or(0, |p| {
            p.list.capacity() * std::mem::size_of::<MapPage<K>>() + p.stale.capacity() * 8
        })
    }

    /// Writes the pages changed since the last save into `batch` (table `table`, whose ids the
    /// map owns) and the page list into `w`, the module's meta page. The first paged save writes
    /// every entry.
    pub fn save_paged(
        &mut self,
        batch: &mut SaveBatch,
        table: u16,
        w: &mut Writer,
        mut fk: impl FnMut(&K, &mut Writer),
        mut fv: impl FnMut(&V, &mut Writer),
    ) -> MapTicket {
        let PagedMap { map, pages } = self;
        let pages = pages.get_or_insert_with(|| {
            let mut list = vec![MapPage {
                lower: None,
                id: 0,
                dirty: true,
            }];
            for (i, k) in map.keys().enumerate() {
                if i > 0 && i % MAP_PAGE_ENTRIES == 0 {
                    list.push(MapPage {
                        lower: Some(k.clone()),
                        id: list.len() as u64,
                        dirty: true,
                    });
                }
            }
            MapPages {
                next_page: list.len() as u64,
                list,
                stale: Vec::new(),
            }
        });
        let mut t = MapTicket::default();
        for id in pages.stale.drain(..) {
            batch.delete(PageKey::new(table, id));
            t.deleted.push(id);
        }
        let old = std::mem::take(&mut pages.list);
        let mut out = Vec::with_capacity(old.len());
        for (i, p) in old.iter().enumerate() {
            if !p.dirty {
                out.push(p.clone());
                continue;
            }
            let lo = p.lower.as_ref().map_or(Bound::Unbounded, Bound::Included);
            let hi = old
                .get(i + 1)
                .and_then(|q| q.lower.as_ref())
                .map_or(Bound::Unbounded, Bound::Excluded);
            let entries: Vec<(&K, &V)> = map.range::<K, _>((lo, hi)).collect();
            if entries.is_empty() && !(out.is_empty() && i + 1 == old.len()) {
                batch.delete(PageKey::new(table, p.id));
                t.deleted.push(p.id);
                continue;
            }
            let per = if entries.len() > 2 * MAP_PAGE_ENTRIES {
                MAP_PAGE_ENTRIES
            } else {
                entries.len().max(1)
            };
            for (j, chunk) in entries
                .chunks(per)
                .chain(
                    // An empty page still writes itself (the map's only page).
                    entries.is_empty().then_some(&[][..]),
                )
                .enumerate()
            {
                let (id, lower) = if j == 0 {
                    (p.id, p.lower.clone())
                } else {
                    let id = pages.next_page;
                    pages.next_page += 1;
                    (id, Some(chunk[0].0.clone()))
                };
                let mut pw = Writer::with_capacity(16 + chunk.len() * 16);
                pw.var_u64(chunk.len() as u64);
                for (k, v) in chunk {
                    fk(k, &mut pw);
                    fv(v, &mut pw);
                }
                batch.put(PageKey::new(table, id), pw.as_bytes());
                t.written += 1;
                out.push(MapPage {
                    lower,
                    id,
                    dirty: false,
                });
            }
        }
        if let Some(first) = out.first_mut() {
            first.lower = None;
        }
        pages.list = out;
        w.var_u64(pages.next_page);
        w.var_u64(pages.list.len() as u64);
        for (i, p) in pages.list.iter().enumerate() {
            w.var_u64(p.id);
            if i > 0 {
                fk(p.lower.as_ref().expect("a later page has a lower bound"), w);
            }
        }
        t
    }

    /// The commit holding the ticket's pages is durable. Nothing to do: kept for symmetry.
    pub fn saved(&mut self, _t: MapTicket) {}

    /// The commit failed: the next save writes every page and deletes what this one would have.
    pub fn save_failed(&mut self, t: MapTicket) {
        if let Some(p) = &mut self.pages {
            p.list.iter_mut().for_each(|pg| pg.dirty = true);
            p.stale.extend(t.deleted);
        }
    }

    /// A map from the page list [`PagedMap::save_paged`] wrote (read from `r`) and its pages in
    /// `source`.
    pub fn open_paged(
        r: &mut Reader<'_>,
        table: u16,
        source: &dyn PageSource,
        mut gk: impl FnMut(&mut Reader<'_>) -> codec::Result<K>,
        mut gv: impl FnMut(&mut Reader<'_>) -> codec::Result<V>,
    ) -> codec::Result<Self> {
        let next_page = r.var_u64()?;
        let n = r.len_prefix(1)?;
        let mut list: Vec<MapPage<K>> = Vec::with_capacity(n);
        for i in 0..n {
            let id = r.var_u64()?;
            let lower = if i > 0 { Some(gk(r)?) } else { None };
            if id >= next_page || list.last().is_some_and(|q| q.lower >= lower) {
                return Err(CodecError::Invalid("map pages out of order".into()));
            }
            list.push(MapPage {
                lower,
                id,
                dirty: false,
            });
        }
        let mut map = BTreeMap::new();
        for (i, p) in list.iter().enumerate() {
            let upper = list.get(i + 1).and_then(|q| q.lower.as_ref());
            let raw = page_read(source, table, p.id)?;
            let mut pr = Reader::new(&raw);
            for _ in 0..pr.len_prefix(1)? {
                let k = gk(&mut pr)?;
                let v = gv(&mut pr)?;
                let in_range = p.lower.as_ref().is_none_or(|l| *l <= k)
                    && upper.is_none_or(|u| k < *u)
                    && map.last_key_value().is_none_or(|(l, _)| *l < k);
                if !in_range {
                    return Err(CodecError::Invalid(format!(
                        "map page {table:#x}/{} holds a key out of its range",
                        p.id
                    )));
                }
                map.insert(k, v);
            }
            pr.finish()?;
        }
        Ok(PagedMap {
            map,
            pages: Some(MapPages {
                list,
                next_page,
                stale: Vec::new(),
            }),
        })
    }
}

// ---- the arena ----------------------------------------------------------------------------------

/// An [`Arena`] whose saves write only the slot pages changed since the last one (module docs).
/// Reads go through `Deref`; changes go through its own methods and `IndexMut`.
pub struct ResidentArena<T> {
    arena: Arena<T>,
    pages: Option<ArenaPages>,
}

struct ArenaPages {
    dirty: BTreeSet<usize>,
    /// The shortest the free list was since the last save: its pages from there up changed.
    free_low: usize,
    /// The free list's length at the last save.
    saved_free_len: usize,
    /// Free-list pages the store holds as of the last save.
    stored_free: usize,
    /// Write every page at the next save (the first one, or after a failed one).
    full: bool,
}

/// What an arena's save wrote, for [`ResidentArena::save_failed`].
#[derive(Debug, Default)]
#[must_use]
pub struct ArenaTicket {
    written: usize,
    stored_free_before: usize,
}

impl ArenaTicket {
    /// Pages written.
    pub fn pages(&self) -> usize {
        self.written
    }
}

impl<T> Default for ResidentArena<T> {
    fn default() -> Self {
        ResidentArena {
            arena: Arena::new(),
            pages: None,
        }
    }
}

impl<T> Deref for ResidentArena<T> {
    type Target = Arena<T>;

    fn deref(&self) -> &Arena<T> {
        &self.arena
    }
}

impl<T> From<Arena<T>> for ResidentArena<T> {
    fn from(arena: Arena<T>) -> Self {
        ResidentArena { arena, pages: None }
    }
}

impl<T> std::ops::Index<Handle<T>> for ResidentArena<T> {
    type Output = T;

    fn index(&self, h: Handle<T>) -> &T {
        &self.arena[h]
    }
}

impl<T> std::ops::IndexMut<Handle<T>> for ResidentArena<T> {
    fn index_mut(&mut self, h: Handle<T>) -> &mut T {
        self.mark(h.index());
        &mut self.arena[h]
    }
}

fn pages_for(n: usize, per: usize) -> usize {
    n.div_ceil(per)
}

impl<T> ResidentArena<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether saves go to pages (after the first [`ResidentArena::save_paged`] or an open).
    pub fn is_paged(&self) -> bool {
        self.pages.is_some()
    }

    fn mark(&mut self, index: u32) {
        if let Some(p) = &mut self.pages {
            p.dirty.insert(index as usize / ARENA_PAGE_SLOTS);
        }
    }

    fn note_free(&mut self) {
        if let Some(p) = &mut self.pages {
            p.free_low = p.free_low.min(self.arena.free.len());
        }
    }

    pub fn insert(&mut self, value: T) -> Handle<T> {
        let h = self.arena.insert(value);
        self.mark(h.index());
        self.note_free();
        h
    }

    pub fn get_mut(&mut self, h: Handle<T>) -> Option<&mut T> {
        if self.arena.contains(h) {
            self.mark(h.index());
        }
        self.arena.get_mut(h)
    }

    pub fn remove(&mut self, h: Handle<T>) -> Option<T> {
        let v = self.arena.remove(h)?;
        self.mark(h.index());
        Some(v)
    }

    /// The whole arena with its pages forgotten.
    pub fn into_inner(self) -> Arena<T> {
        self.arena
    }

    /// Bytes held on the heap: the arena's and the dirty set's.
    pub fn heap_bytes(&self) -> usize {
        self.arena.heap_bytes() + self.pages.as_ref().map_or(0, |p| p.dirty.len() * 24)
    }

    /// Writes the slot and free-list pages changed since the last save into `batch` (table
    /// `table`, whose ids the arena owns) and the counts into `w`, the module's meta page. The
    /// first paged save writes every page.
    pub fn save_paged(
        &mut self,
        batch: &mut SaveBatch,
        table: u16,
        w: &mut Writer,
        mut f: impl FnMut(&T, &mut Writer),
    ) -> ArenaTicket {
        let a = &self.arena;
        let p = self.pages.get_or_insert_with(|| ArenaPages {
            dirty: BTreeSet::new(),
            free_low: 0,
            saved_free_len: 0,
            stored_free: 0,
            full: true,
        });
        let mut t = ArenaTicket {
            written: 0,
            stored_free_before: p.stored_free,
        };
        let slot_pages = pages_for(a.slots.len(), ARENA_PAGE_SLOTS);
        let pages: Vec<usize> = if p.full {
            (0..slot_pages).collect()
        } else {
            p.dirty
                .iter()
                .copied()
                .filter(|&i| i < slot_pages)
                .collect()
        };
        for i in pages {
            let lo = i * ARENA_PAGE_SLOTS;
            let hi = (lo + ARENA_PAGE_SLOTS).min(a.slots.len());
            let mut pw = Writer::with_capacity(64 * (hi - lo));
            for s in &a.slots[lo..hi] {
                pw.u16(s.generation);
                match &s.value {
                    Some(v) => {
                        pw.u8(1);
                        f(v, &mut pw);
                    }
                    None => pw.u8(0),
                }
            }
            batch.put(PageKey::new(table, i as u64), pw.as_bytes());
            t.written += 1;
        }
        let free_pages = pages_for(a.free.len(), ARENA_PAGE_SLOTS);
        let from = if p.full {
            0
        } else if p.free_low < a.free.len().max(p.saved_free_len) {
            p.free_low / ARENA_PAGE_SLOTS
        } else {
            free_pages
        };
        for j in from..free_pages {
            let lo = j * ARENA_PAGE_SLOTS;
            let hi = (lo + ARENA_PAGE_SLOTS).min(a.free.len());
            let mut pw = Writer::with_capacity(4 * (hi - lo));
            a.free[lo..hi].iter().for_each(|i| pw.u32(*i));
            batch.put(PageKey::new(table, FREE_PAGES + j as u64), pw.as_bytes());
            t.written += 1;
        }
        for j in free_pages..p.stored_free {
            batch.delete(PageKey::new(table, FREE_PAGES + j as u64));
        }
        p.stored_free = free_pages;
        p.dirty.clear();
        p.free_low = a.free.len();
        p.saved_free_len = a.free.len();
        p.full = false;
        w.var_u64(a.slots.len() as u64);
        w.var_u64(a.len as u64);
        w.var_u64(a.free.len() as u64);
        t
    }

    /// The commit holding the ticket's pages is durable. Nothing to do: kept for symmetry.
    pub fn saved(&mut self, _t: ArenaTicket) {}

    /// The commit failed: the next save writes every page and deletes the free-list pages the
    /// store may still hold.
    pub fn save_failed(&mut self, t: ArenaTicket) {
        if let Some(p) = &mut self.pages {
            p.full = true;
            p.stored_free = p.stored_free.max(t.stored_free_before);
        }
    }

    /// An arena from the counts [`ResidentArena::save_paged`] wrote (read from `r`) and its pages
    /// in `source`; checked as [`Arena::decode_with`] checks a blob.
    pub fn open_paged(
        r: &mut Reader<'_>,
        table: u16,
        source: &dyn PageSource,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<T>,
    ) -> codec::Result<Self> {
        let n = usize::try_from(r.var_u64()?).map_err(|_| CodecError::Invalid("slots".into()))?;
        let want_len = r.var_u64()?;
        let nf = usize::try_from(r.var_u64()?).map_err(|_| CodecError::Invalid("free".into()))?;
        let bad = |what: String| Err(CodecError::Invalid(format!("arena pages: {what}")));
        let mut slots = Vec::with_capacity(n.min(1 << 20));
        let mut len = 0usize;
        for i in 0..pages_for(n, ARENA_PAGE_SLOTS) {
            let raw = page_read(source, table, i as u64)?;
            let mut pr = Reader::new(&raw);
            let k = (n - i * ARENA_PAGE_SLOTS).min(ARENA_PAGE_SLOTS);
            for _ in 0..k {
                let generation = pr.u16()?;
                let value = match pr.u8()? {
                    0 => None,
                    1 => {
                        len += 1;
                        Some(f(&mut pr)?)
                    }
                    t => return bad(format!("slot tag {t}")),
                };
                if value.is_some() && generation == 0 {
                    return bad("a live slot of generation 0".into());
                }
                slots.push(Slot { generation, value });
            }
            pr.finish()?;
        }
        if len as u64 != want_len {
            return bad(format!("{len} live slots, the header says {want_len}"));
        }
        let mut free = Vec::with_capacity(nf.min(1 << 20));
        let mut seen = vec![false; slots.len()];
        for j in 0..pages_for(nf, ARENA_PAGE_SLOTS) {
            let raw = page_read(source, table, FREE_PAGES + j as u64)?;
            let mut pr = Reader::new(&raw);
            for _ in 0..(nf - j * ARENA_PAGE_SLOTS).min(ARENA_PAGE_SLOTS) {
                let i = pr.u32()?;
                let ok = slots
                    .get(i as usize)
                    .is_some_and(|s: &Slot<T>| s.value.is_none() && s.generation != 0)
                    && !std::mem::replace(&mut seen[i as usize], true);
                if !ok {
                    return bad(format!("the free list names slot {i}"));
                }
                free.push(i);
            }
            pr.finish()?;
        }
        Ok(ResidentArena {
            arena: Arena { slots, free, len },
            pages: Some(ArenaPages {
                dirty: BTreeSet::new(),
                free_low: nf,
                saved_free_len: nf,
                stored_free: pages_for(nf, ARENA_PAGE_SLOTS),
                full: false,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Durability, PageStore};
    use proptest::prelude::*;
    use std::sync::RwLock;

    const TABLE: u16 = 0x7e10;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-meta-pages-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn get_u32(r: &mut Reader<'_>) -> codec::Result<u32> {
        r.u32()
    }

    /// Saves the wheel through the store; returns the header and the raw bytes written.
    fn save_wheel(
        wh: &mut PagedWheel<u32>,
        store: &RwLock<PageStore>,
        fail: bool,
    ) -> (Vec<u8>, usize) {
        let mut b = SaveBatch::new();
        let mut w = Writer::new();
        let t = wh.save_paged(&mut b, TABLE, &mut w, |x, w| w.u32(*x));
        let bytes = b.raw_bytes();
        if fail {
            wh.save_failed(t);
        } else {
            store.write().unwrap().commit(b, 0).unwrap();
            wh.saved(t);
        }
        (w.into_bytes(), bytes)
    }

    fn open_wheel(header: &[u8], store: &RwLock<PageStore>) -> PagedWheel<u32> {
        let mut r = Reader::new(header);
        let wh = PagedWheel::open_paged(&mut r, TABLE, store, get_u32).unwrap();
        r.finish().unwrap();
        wh
    }

    fn drain(mut wh: PagedWheel<u32>) -> Vec<(Tick, u32)> {
        let mut v = Vec::new();
        while let Some(e) = wh.pop_until(Tick(u64::MAX)) {
            v.push(e);
        }
        v
    }

    fn blob(wh: &PagedWheel<u32>) -> Vec<u8> {
        let mut w = Writer::new();
        wh.encode_with(&mut w, |x, w| w.u32(*x));
        w.into_bytes()
    }

    #[test]
    fn the_blob_is_the_timing_wheels() {
        let mut a = TimingWheel::new(Tick(5));
        let mut b = PagedWheel::new(Tick(5));
        for (i, at) in [9u64, 5, 70, 9, 4_000, 5].into_iter().enumerate() {
            a.schedule(Tick(at), i as u32);
            b.schedule(Tick(at), i as u32);
        }
        let mut wa = Writer::new();
        a.encode_with(&mut wa, |x, w| w.u32(*x));
        assert_eq!(wa.as_bytes(), &blob(&b)[..]);
        let back = PagedWheel::decode_with(&mut Reader::new(wa.as_bytes()), get_u32).unwrap();
        assert_eq!(blob(&back), blob(&b));
    }

    #[test]
    fn a_quiet_save_writes_the_tail_only() {
        let store = RwLock::new(PageStore::open(tmpdir("quiet"), Durability::NoSync).unwrap());
        let mut small = Vec::new();
        for n in [5_000u32, 50_000] {
            let mut wh = PagedWheel::new(Tick(0));
            for i in 0..n {
                wh.schedule(Tick(1 + u64::from(i) * 7 % 9_973), i);
            }
            let (_, first) = save_wheel(&mut wh, &store, false);
            assert!(first > n as usize * 9, "the first save writes every entry");
            let (_, quiet) = save_wheel(&mut wh, &store, false);
            small.push(quiet);
        }
        assert!(
            small[1] <= small[0] + 64,
            "a quiet save grew with the wheel: {small:?}"
        );
        assert_eq!(small[1], 0, "nothing changed, nothing written");
        // A few entries scheduled and popped write the tail page, not the wheel.
        let mut wh = PagedWheel::new(Tick(0));
        for i in 0..50_000u32 {
            wh.schedule(Tick(1 + u64::from(i) * 7 % 9_973), i);
        }
        save_wheel(&mut wh, &store, false);
        for i in 0..10 {
            wh.schedule(Tick(20_000 + i), 1);
        }
        wh.pop_until(Tick(3));
        let (_, bytes) = save_wheel(&mut wh, &store, false);
        assert!(bytes < 20 * WHEEL_PAGE_ENTRIES, "{bytes}");
    }

    #[derive(Clone, Debug)]
    enum Op {
        Schedule(u64, u32),
        Pop(u64),
        Save,
        FailedSave,
        Reopen,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            6 => (0u64..3_000, any::<u32>()).prop_map(|(d, x)| Op::Schedule(d, x)),
            3 => (0u64..400).prop_map(Op::Pop),
            1 => Just(Op::Save),
            1 => Just(Op::FailedSave),
            1 => Just(Op::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Whatever the saves, failed saves and reopens, the paged wheel pops as a plain one.
        #[test]
        fn a_reopened_wheel_pops_as_a_resident_one(ops in prop::collection::vec(op(), 1..160)) {
            let dir = tmpdir("prop");
            let store = RwLock::new(PageStore::open(&dir, Durability::NoSync).unwrap());
            let mut plain: TimingWheel<u32> = TimingWheel::new(Tick(0));
            let mut paged: PagedWheel<u32> = PagedWheel::new(Tick(0));
            let mut saved: Option<(Vec<u8>, Vec<u8>)> = None;
            for o in ops {
                match o {
                    Op::Schedule(d, x) => {
                        // Bursts of many entries, so pages seal and garbage builds.
                        for k in 0..40u32 {
                            let at = Tick(plain.now().0 + (d + u64::from(k) * 31) % 3_000);
                            plain.schedule(at, x ^ k);
                            paged.schedule(at, x ^ k);
                        }
                    }
                    Op::Pop(d) => {
                        let limit = Tick(plain.now().0 + d);
                        loop {
                            let a = plain.pop_until(limit);
                            let b = paged.pop_until(limit);
                            prop_assert_eq!(a, b);
                            if a.is_none() { break; }
                        }
                    }
                    Op::Save => {
                        let (h, _) = save_wheel(&mut paged, &store, false);
                        saved = Some((h, blob(&paged)));
                    }
                    Op::FailedSave => { save_wheel(&mut paged, &store, true); }
                    Op::Reopen => {
                        if let Some((h, want)) = &saved {
                            // Back to the last good save, the plain wheel with it.
                            let back = open_wheel(h, &store);
                            prop_assert_eq!(&blob(&back), want);
                            plain = TimingWheel::decode_with(&mut Reader::new(want), get_u32)
                                .unwrap();
                            paged = back;
                        }
                    }
                }
            }
            let (h, _) = save_wheel(&mut paged, &store, false);
            let back = open_wheel(&h, &store);
            prop_assert_eq!(blob(&back), blob(&paged));
            let mut want = Vec::new();
            while let Some(e) = plain.pop_until(Tick(u64::MAX)) { want.push(e); }
            // No page is left behind: the store holds exactly the pages the header lists.
            let log = back.log.as_ref().unwrap();
            let mut listed: Vec<u64> = log.sealed.iter().map(|p| p.id).chain(log.open).collect();
            listed.sort_unstable();
            prop_assert_eq!(store.read().unwrap().table_ids(TABLE).unwrap(), listed);
            prop_assert_eq!(drain(back), want);
            drop(store);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // ---- the map --------------------------------------------------------------------------------

    fn save_map(
        m: &mut PagedMap<u64, u32>,
        store: &RwLock<PageStore>,
        fail: bool,
    ) -> (Vec<u8>, usize) {
        let mut b = SaveBatch::new();
        let mut w = Writer::new();
        let t = m.save_paged(&mut b, TABLE, &mut w, |k, w| w.u64(*k), |v, w| w.u32(*v));
        let bytes = b.raw_bytes();
        if fail {
            m.save_failed(t);
        } else {
            store.write().unwrap().commit(b, 0).unwrap();
            m.saved(t);
        }
        (w.into_bytes(), bytes)
    }

    fn open_map(header: &[u8], store: &RwLock<PageStore>) -> PagedMap<u64, u32> {
        let mut r = Reader::new(header);
        let m = PagedMap::open_paged(&mut r, TABLE, store, |r| r.u64(), get_u32).unwrap();
        r.finish().unwrap();
        m
    }

    #[test]
    fn a_map_save_writes_the_pages_changed() {
        let store = RwLock::new(PageStore::open(tmpdir("map"), Durability::NoSync).unwrap());
        let mut quiet = Vec::new();
        let mut one = Vec::new();
        for n in [4_000u64, 40_000] {
            let mut m = PagedMap::new();
            for k in 0..n {
                m.insert(k * 3, k as u32);
            }
            save_map(&mut m, &store, false);
            quiet.push(save_map(&mut m, &store, false).1);
            m.insert(7, 1);
            *m.get_mut(&(n / 2 * 3)).unwrap() += 1;
            one.push(save_map(&mut m, &store, false).1);
        }
        assert!(quiet[1] <= quiet[0] + 64, "{quiet:?}");
        assert!(one[1] <= one[0] + 64, "{one:?}");
        assert!(one[1] < 4 * MAP_PAGE_ENTRIES * 12, "{one:?}");
    }

    #[derive(Clone, Debug)]
    enum MapOp {
        Insert(u64, u32),
        Run(u64, u32),
        Remove(u64),
        RemoveRange(u64, u64),
        Clear,
        Save,
        FailedSave,
        Reopen,
    }

    fn map_op() -> impl Strategy<Value = MapOp> {
        prop_oneof![
            4 => (0u64..5_000, any::<u32>()).prop_map(|(k, v)| MapOp::Insert(k, v)),
            2 => (0u64..5_000, 1u32..1_500).prop_map(|(k, n)| MapOp::Run(k, n)),
            2 => (0u64..5_000).prop_map(MapOp::Remove),
            2 => (0u64..5_000, 1u64..2_000).prop_map(|(k, n)| MapOp::RemoveRange(k, n)),
            1 => Just(MapOp::Clear),
            2 => Just(MapOp::Save),
            1 => Just(MapOp::FailedSave),
            1 => Just(MapOp::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Whatever the changes, saves, failed saves and reopens, the map reopens as it was saved.
        #[test]
        fn a_reopened_map_is_the_saved_one(ops in prop::collection::vec(map_op(), 1..80)) {
            let dir = tmpdir("map-prop");
            let store = RwLock::new(PageStore::open(&dir, Durability::NoSync).unwrap());
            let mut m: PagedMap<u64, u32> = PagedMap::new();
            let mut saved: Option<(Vec<u8>, BTreeMap<u64, u32>)> = None;
            for o in ops {
                match o {
                    MapOp::Insert(k, v) => { m.insert(k, v); }
                    MapOp::Run(k, n) => {
                        for i in 0..u64::from(n) {
                            *m.get_or_default(k + i) += 1;
                        }
                    }
                    MapOp::Remove(k) => { m.remove(&k); }
                    MapOp::RemoveRange(k, n) => {
                        let ks: Vec<u64> = m.range(k..k + n).map(|e| *e.0).collect();
                        for k in ks { m.remove(&k); }
                    }
                    MapOp::Clear => m.clear(),
                    MapOp::Save => {
                        let (h, _) = save_map(&mut m, &store, false);
                        saved = Some((h, m.map.clone()));
                    }
                    MapOp::FailedSave => { save_map(&mut m, &store, true); }
                    MapOp::Reopen => {
                        if let Some((h, want)) = &saved {
                            m = open_map(h, &store);
                            prop_assert_eq!(&m.map, want);
                        }
                    }
                }
            }
            let (h, _) = save_map(&mut m, &store, false);
            let back = open_map(&h, &store);
            prop_assert_eq!(&back.map, &m.map);
            // No page is left behind: the store holds exactly the listed pages.
            let ids = store.read().unwrap().table_ids(TABLE).unwrap();
            let mut listed: Vec<u64> = back.pages.as_ref().unwrap().list.iter().map(|p| p.id).collect();
            listed.sort_unstable();
            prop_assert_eq!(ids, listed);
            drop(store);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    // ---- the arena ------------------------------------------------------------------------------

    fn arena_blob(a: &Arena<u32>) -> Vec<u8> {
        let mut w = Writer::new();
        a.encode_with(&mut w, |x, w| w.u32(*x));
        w.into_bytes()
    }

    fn save_arena(
        a: &mut ResidentArena<u32>,
        store: &RwLock<PageStore>,
        fail: bool,
    ) -> (Vec<u8>, usize) {
        let mut b = SaveBatch::new();
        let mut w = Writer::new();
        let t = a.save_paged(&mut b, TABLE, &mut w, |x, w| w.u32(*x));
        let bytes = b.raw_bytes();
        if fail {
            a.save_failed(t);
        } else {
            store.write().unwrap().commit(b, 0).unwrap();
            a.saved(t);
        }
        (w.into_bytes(), bytes)
    }

    fn open_arena(header: &[u8], store: &RwLock<PageStore>) -> ResidentArena<u32> {
        let mut r = Reader::new(header);
        let a = ResidentArena::open_paged(&mut r, TABLE, store, get_u32).unwrap();
        r.finish().unwrap();
        a
    }

    #[test]
    fn an_arena_save_writes_the_slots_changed() {
        let store = RwLock::new(PageStore::open(tmpdir("arena"), Durability::NoSync).unwrap());
        let mut one = Vec::new();
        for n in [400u32, 4_000] {
            let mut a = ResidentArena::new();
            let hs: Vec<_> = (0..n).map(|i| a.insert(i)).collect();
            for h in hs.iter().step_by(3) {
                a.remove(*h);
            }
            save_arena(&mut a, &store, false);
            assert_eq!(save_arena(&mut a, &store, false).1, 0, "nothing changed");
            a[hs[1]] += 1;
            a.insert(9);
            one.push(save_arena(&mut a, &store, false).1);
        }
        assert!(one[1] <= one[0] + 16, "{one:?}");
    }

    #[derive(Clone, Debug)]
    enum ArenaOp {
        Insert(u32, u8),
        Remove(u16),
        Change(u16),
        Save,
        FailedSave,
        Reopen,
    }

    fn arena_op() -> impl Strategy<Value = ArenaOp> {
        prop_oneof![
            4 => (any::<u32>(), 1u8..40).prop_map(|(v, n)| ArenaOp::Insert(v, n)),
            3 => any::<u16>().prop_map(ArenaOp::Remove),
            2 => any::<u16>().prop_map(ArenaOp::Change),
            2 => Just(ArenaOp::Save),
            1 => Just(ArenaOp::FailedSave),
            1 => Just(ArenaOp::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Whatever the changes, saves, failed saves and reopens, the arena reopens as it was
        /// saved and hands out the same handles after.
        #[test]
        fn a_reopened_arena_is_the_saved_one(ops in prop::collection::vec(arena_op(), 1..120)) {
            let dir = tmpdir("arena-prop");
            let store = RwLock::new(PageStore::open(&dir, Durability::NoSync).unwrap());
            let mut a: ResidentArena<u32> = ResidentArena::new();
            let mut saved: Option<(Vec<u8>, Vec<u8>)> = None;
            let live = |a: &ResidentArena<u32>| a.iter().map(|(h, _)| h).collect::<Vec<_>>();
            for o in ops {
                match o {
                    ArenaOp::Insert(v, n) => {
                        for k in 0..u32::from(n) {
                            a.insert(v ^ k);
                        }
                    }
                    ArenaOp::Remove(i) => {
                        let hs = live(&a);
                        if !hs.is_empty() {
                            // Remove a run, so the free list grows by pages.
                            for h in hs.iter().skip(i as usize % hs.len()).take(1 + i as usize % 23) {
                                a.remove(*h);
                            }
                        }
                    }
                    ArenaOp::Change(i) => {
                        let hs = live(&a);
                        if !hs.is_empty() {
                            *a.get_mut(hs[i as usize % hs.len()]).unwrap() += 1;
                        }
                    }
                    ArenaOp::Save => {
                        let (h, _) = save_arena(&mut a, &store, false);
                        saved = Some((h, arena_blob(&a)));
                    }
                    ArenaOp::FailedSave => { save_arena(&mut a, &store, true); }
                    ArenaOp::Reopen => {
                        if let Some((h, want)) = &saved {
                            a = open_arena(h, &store);
                            prop_assert_eq!(&arena_blob(&a), want);
                        }
                    }
                }
            }
            let (h, _) = save_arena(&mut a, &store, false);
            let want_free = pages_for(a.free.len(), ARENA_PAGE_SLOTS);
            let mut back = open_arena(&h, &store);
            prop_assert_eq!(arena_blob(&back), arena_blob(&a));
            // The same handles from then on.
            for v in 0..30 {
                prop_assert_eq!(back.insert(v), a.insert(v));
            }
            // No free-list page is left behind.
            let free_pages = store.read().unwrap().table_ids(TABLE).unwrap()
                .into_iter().filter(|&id| id >= FREE_PAGES).count();
            prop_assert_eq!(free_pages, want_free);
            drop(store);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
