// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paged rows (A16, SCALE.md §8) for modules whose state is entity rows rather than lazy regions:
//! herds, dragons, factions, ships. The counterpart of [`crate::paging::PagedTable`], with the
//! access modules need: a row is read through `&self` and changed through `&mut self`, as with a
//! [`crate::handle::Arena`], and a page that is not resident loads on the read.
//!
//! # Pages
//!
//! A row is named by a stable `u64` key; its page is `key / rows_per_page`, so keys that are
//! used together should be close (sequential ids, packed coordinates). A page holds its rows in
//! slot order and nothing that points outside it: rows refer to each other by key, so evicting a
//! page never means fixing anything up. Page bytes are canonical (rows in slot order), so equal
//! tables give equal pages whatever was resident when.
//!
//! # Two modes
//!
//! - **Resident** ([`PagedRows::new`]): no page source; every page stays in memory, as an arena
//!   would. Tests, tools and engines without a store run this way.
//! - **Paged** ([`PagedRows::open`], [`PagedRows::attach`]): pages come from a [`PageSource`] (the
//!   page store, shared with the save path) and [`PagedRows::enforce_budget`], called at step
//!   boundaries, evicts least-recently-used pages whose state is on disk until the resident bytes
//!   fit the budget. A page changed since its last save is dirty and stays until a save writes it
//!   and the commit is durable (the same rules as `PagedTable`, DECISIONS.md K3, and two saves in
//!   flight as PM1).
//!
//! Both modes give the same rows, the same page bytes and the same [`PagedRows::state_hash`].
//!
//! # Loading through `&self`
//!
//! A read that misses loads the page and keeps it, through a shared reference. That is sound
//! because a resident page is a heap allocation owned through a raw pointer: a load through
//! `&self` only adds pages, never moves, changes or drops one, and every method that changes or
//! drops a page takes `&mut self`, which the borrow checker forbids while any `&T` lent by `&self`
//! is alive. Between step boundaries the resident set can only grow; the budget is enforced at the
//! boundaries.
//!
//! # IO errors
//!
//! [`PagedRows::try_get`] and [`PagedRows::ensure`] return a failed load as an error and change
//! nothing. The plain accessors ([`PagedRows::get`], [`PagedRows::get_mut`], indexing) panic on
//! one: module code in the middle of an event cannot undo half an event, and a page store that
//! fails a read is as fatal to the session as a failed chunk read is to Minecraft (the engine is
//! dropped and reopened from the last save, as for a step panic). A module that wants a failed
//! load to change nothing calls [`PagedRows::ensure`] on the rows an event will touch before it
//! starts the event.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, RwLock};

use crate::codec::{Decode, Encode, Reader, Writer};
use crate::handle::Handle;
use crate::page::{PageError, PageKey, PageStore, Result, SaveBatch};
use crate::paging::{IdMap, SaveTicket};

/// A row that can live in a page.
pub trait Row: Encode + Decode {
    /// Heap bytes the row owns beyond its own size (vectors, maps), for the budget.
    fn heap_bytes(&self) -> usize {
        0
    }
}

/// Where pages come from: the page store, shared with whatever commits saves.
pub trait PageSource: Send + Sync {
    /// The page's bytes, or `None` if the store has no such page.
    fn read(&self, key: PageKey) -> Result<Option<Vec<u8>>>;
    /// Every page id of `table` the store holds, in order. Whole-table passes only.
    fn table_ids(&self, table: u16) -> Result<Vec<u64>>;
}

impl PageSource for PageStore {
    fn read(&self, key: PageKey) -> Result<Option<Vec<u8>>> {
        self.get_shared(key)
    }

    fn table_ids(&self, table: u16) -> Result<Vec<u64>> {
        PageStore::table_ids(self, table)
    }
}

fn poisoned<T>(_: T) -> PageError {
    PageError::Poisoned
}

impl PageSource for RwLock<PageStore> {
    fn read(&self, key: PageKey) -> Result<Option<Vec<u8>>> {
        self.read().map_err(poisoned)?.get_shared(key)
    }

    fn table_ids(&self, table: u16) -> Result<Vec<u64>> {
        self.read().map_err(poisoned)?.table_ids(table)
    }
}

impl PageSource for Mutex<PageStore> {
    fn read(&self, key: PageKey) -> Result<Option<Vec<u8>>> {
        self.lock().map_err(poisoned)?.get_shared(key)
    }

    fn table_ids(&self, table: u16) -> Result<Vec<u64>> {
        self.lock().map_err(poisoned)?.table_ids(table)
    }
}

/// A store that may be closed (`None`): reads then fail as from a poisoned store.
impl PageSource for Mutex<Option<PageStore>> {
    fn read(&self, key: PageKey) -> Result<Option<Vec<u8>>> {
        match self.lock().map_err(poisoned)?.as_ref() {
            Some(s) => s.get_shared(key),
            None => Err(PageError::Poisoned),
        }
    }

    fn table_ids(&self, table: u16) -> Result<Vec<u64>> {
        match self.lock().map_err(poisoned)?.as_ref() {
            Some(s) => s.table_ids(table),
            None => Err(PageError::Poisoned),
        }
    }
}

/// Counters for the bench and the profiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowStats {
    /// Pages read from the source.
    pub loads: u64,
    /// Stored bytes those loads decoded (raw page bytes).
    pub load_bytes: u64,
    pub evictions: u64,
    /// Budget checks that ended over budget because every remaining page was dirty or saving.
    pub over_budget: u64,
    pub peak_resident_bytes: u64,
}

/// Bytes a resident page costs in the id index (a hash map entry with its control byte and load
/// factor, rounded up). An estimate for the budget, as in `paging`.
const INDEX_ENTRY_BYTES: usize = 24;

struct RowPage<T> {
    id: u64,
    /// Rows by slot, sorted.
    rows: Vec<(u32, T)>,
    /// Changed since it was last written: must be saved before it may be evicted.
    dirty: bool,
    /// The latest snapshot holding it whose ticket has not come back (0: none), as in `paging`.
    saving: u64,
    /// The rows' own heap bytes as last measured.
    heap: usize,
    /// A row was lent mutably since `heap` was measured.
    remeasure: bool,
    /// The table's clock at its last use: least recently used pages go first.
    used: Cell<u64>,
}

impl<T: Row> RowPage<T> {
    fn empty(id: u64) -> Self {
        RowPage {
            id,
            rows: Vec::new(),
            dirty: false,
            saving: 0,
            heap: 0,
            remeasure: false,
            used: Cell::new(0),
        }
    }

    fn bytes(&self) -> usize {
        size_of::<Self>()
            + self.rows.capacity() * size_of::<(u32, T)>()
            + self.heap
            + INDEX_ENTRY_BYTES
    }

    fn find(&self, slot: u32) -> std::result::Result<usize, usize> {
        self.rows.binary_search_by_key(&slot, |r| r.0)
    }

    fn measure(&mut self) {
        self.heap = self.rows.iter().map(|(_, v)| v.heap_bytes()).sum();
        self.remeasure = false;
    }

    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.rows.len() as u64);
        for (s, v) in &self.rows {
            w.var_u64(u64::from(*s));
            v.encode(w);
        }
    }

    fn decode(id: u64, per: u64, bytes: &[u8]) -> Result<Self> {
        let bad = |e: crate::codec::CodecError| PageError::Format(format!("row page {id}: {e}"));
        let mut r = Reader::new(bytes);
        let n = r.len_prefix(2).map_err(bad)?;
        let mut page = RowPage::empty(id);
        page.rows.reserve_exact(n);
        let mut next = 0u64;
        for _ in 0..n {
            let s = r.var_u64().map_err(bad)?;
            if s < next || s >= per {
                return Err(PageError::Format(format!(
                    "row page {id}: slot {s} out of order or past {per}"
                )));
            }
            next = s + 1;
            let v = T::decode(&mut r).map_err(bad)?;
            page.rows.push((s as u32, v));
        }
        r.finish().map_err(bad)?;
        page.measure();
        Ok(page)
    }
}

/// Rows of one kind, keyed by stable `u64` keys, in pages under a memory budget. See the module
/// docs.
pub struct PagedRows<T: Row> {
    table: u16,
    per: u64,
    source: Option<Arc<dyn PageSource>>,
    budget: usize,
    /// Resident pages, each owned through its pointer (`Box::into_raw`): see the module docs.
    pages: RefCell<IdMap<u64, NonNull<RowPage<T>>>>,
    clock: Cell<u64>,
    resident_bytes: Cell<usize>,
    stats: Cell<RowStats>,
    /// Pages lent mutably since the last budget check, to measure again.
    remeasure: Vec<u64>,
    /// Rows in the table, resident or not.
    len: u64,
    /// The last snapshot's number.
    snapshots: u64,
    _owns: PhantomData<Box<RowPage<T>>>,
}

// SAFETY: the pointers in `pages` are owned allocations (`Box<RowPage<T>>`) that nothing else
// refers to, so moving the table to another thread moves them with it, as a `Box` would. The
// `Cell`s and the `RefCell` keep the table `!Sync`.
unsafe impl<T: Row + Send> Send for PagedRows<T> {}

impl<T: Row> Drop for PagedRows<T> {
    fn drop(&mut self) {
        for (_, p) in self.pages.get_mut().drain() {
            // SAFETY: every pointer came from `Box::into_raw` and is freed once, here or in
            // `evict`.
            drop(unsafe { Box::from_raw(p.as_ptr()) });
        }
    }
}

impl<T: Row> std::fmt::Debug for PagedRows<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagedRows")
            .field("table", &self.table)
            .field("len", &self.len)
            .field("resident_pages", &self.resident_pages())
            .field("paged", &self.source.is_some())
            .finish()
    }
}

impl<T: Row> PagedRows<T> {
    /// A resident table: nothing pages until [`PagedRows::attach`].
    pub fn new(table: u16, rows_per_page: u64) -> Self {
        assert!(rows_per_page > 0 && rows_per_page <= 1 << 32);
        PagedRows {
            table,
            per: rows_per_page,
            source: None,
            budget: usize::MAX,
            pages: RefCell::new(IdMap::default()),
            clock: Cell::new(0),
            resident_bytes: Cell::new(0),
            stats: Cell::new(RowStats::default()),
            remeasure: Vec::new(),
            len: 0,
            snapshots: 0,
            _owns: PhantomData,
        }
    }

    /// A table saved earlier: its pages are in `source`, none is resident, and it holds `len` rows
    /// (the module keeps the count in its own resident state).
    pub fn open(
        table: u16,
        rows_per_page: u64,
        source: Arc<dyn PageSource>,
        budget: usize,
        len: u64,
    ) -> Self {
        let mut t = Self::new(table, rows_per_page);
        t.source = Some(source);
        t.budget = budget;
        t.len = len;
        t
    }

    /// Starts paging a resident table to `source`. Its pages stay until a save writes them.
    pub fn attach(&mut self, source: Arc<dyn PageSource>, budget: usize) {
        self.source = Some(source);
        self.budget = budget;
        for p in self.pages.get_mut().values() {
            // SAFETY: `&mut self`: no row is lent.
            let p = unsafe { &mut *p.as_ptr() };
            if p.saving == 0 {
                p.dirty = true;
            }
        }
    }

    pub fn table(&self) -> u16 {
        self.table
    }

    pub fn rows_per_page(&self) -> u64 {
        self.per
    }

    pub fn is_paged(&self) -> bool {
        self.source.is_some()
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes;
    }

    /// Rows in the table, resident or not.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn page_of(&self, key: u64) -> u64 {
        key / self.per
    }

    fn slot_of(&self, key: u64) -> u32 {
        (key % self.per) as u32
    }

    pub fn stats(&self) -> RowStats {
        self.stats.get()
    }

    pub fn reset_stats(&mut self) {
        self.stats.set(RowStats {
            peak_resident_bytes: self.resident_bytes.get() as u64,
            ..RowStats::default()
        });
    }

    /// Resident page bytes, the quantity the budget bounds (rows' heap as last measured).
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes.get()
    }

    pub fn resident_pages(&self) -> usize {
        self.pages.borrow().len()
    }

    pub fn is_resident(&self, key: u64) -> bool {
        self.pages.borrow().contains_key(&self.page_of(key))
    }

    /// Every byte the table holds: resident pages and the id index.
    pub fn heap_bytes(&self) -> usize {
        let pages = self.pages.borrow();
        self.resident_bytes.get() - pages.len() * INDEX_ENTRY_BYTES
            + pages.capacity() * (size_of::<(u64, NonNull<RowPage<T>>)>() + 1)
            + self.remeasure.capacity() * 8
    }

    fn account(&self, delta_add: usize, delta_sub: usize) {
        let b = self.resident_bytes.get() + delta_add - delta_sub;
        self.resident_bytes.set(b);
        let mut s = self.stats.get();
        s.peak_resident_bytes = s.peak_resident_bytes.max(b as u64);
        self.stats.set(s);
    }

    fn tick(&self) -> u64 {
        let c = self.clock.get() + 1;
        self.clock.set(c);
        c
    }

    fn install(&self, page: RowPage<T>) -> NonNull<RowPage<T>> {
        let id = page.id;
        page.used.set(self.tick());
        self.account(page.bytes(), 0);
        let p = NonNull::from(Box::leak(Box::new(page)));
        let old = self.pages.borrow_mut().insert(id, p);
        debug_assert!(old.is_none(), "page {id} installed twice");
        p
    }

    /// The page, loading it if the source has it. A page that exists nowhere comes back `None`
    /// in a resident table; in a paged one it is installed empty (clean, evictable), so later
    /// misses on it cost nothing.
    fn page(&self, pid: u64) -> Result<Option<NonNull<RowPage<T>>>> {
        if let Some(&p) = self.pages.borrow().get(&pid) {
            // SAFETY: a resident page lives until `&mut self` evicts it.
            unsafe { p.as_ref() }.used.set(self.tick());
            return Ok(Some(p));
        }
        let Some(source) = &self.source else {
            return Ok(None);
        };
        assert!(pid < 1 << 48, "row page id {pid} past the page key range");
        let page = match source.read(PageKey::new(self.table, pid))? {
            Some(raw) => {
                let mut s = self.stats.get();
                s.loads += 1;
                s.load_bytes += raw.len() as u64;
                self.stats.set(s);
                RowPage::decode(pid, self.per, &raw)?
            }
            None => RowPage::empty(pid),
        };
        Ok(Some(self.install(page)))
    }

    /// The page for a change: loaded, or created empty.
    fn page_for_change(&mut self, pid: u64) -> Result<&mut RowPage<T>> {
        let p = match self.page(pid)? {
            Some(p) => p,
            None => self.install(RowPage::empty(pid)),
        };
        // SAFETY: `&mut self`: no row is lent, so this is the only reference to the page.
        Ok(unsafe { &mut *p.as_ptr() })
    }

    // ---- Rows ----

    /// The row, loading its page if needed; an IO error comes back and changes nothing.
    pub fn try_get(&self, key: u64) -> Result<Option<&T>> {
        let Some(p) = self.page(self.page_of(key))? else {
            return Ok(None);
        };
        // SAFETY: the page lives until `&mut self` evicts or changes it, which cannot happen
        // while the returned reference (tied to `&self`) is alive.
        let page = unsafe { &*p.as_ptr() };
        Ok(page.find(self.slot_of(key)).ok().map(|i| &page.rows[i].1))
    }

    /// The row, loading its page if needed. Panics if the page store fails the read (module
    /// docs).
    pub fn get(&self, key: u64) -> Option<&T> {
        self.try_get(key)
            .unwrap_or_else(|e| panic!("row table {:#x}: page load failed: {e}", self.table))
    }

    pub fn contains(&self, key: u64) -> bool {
        self.get(key).is_some()
    }

    /// The row to change; its page is dirty from now until a save writes it.
    pub fn try_get_mut(&mut self, key: u64) -> Result<Option<&mut T>> {
        let pid = self.page_of(key);
        let slot = self.slot_of(key);
        let Some(p) = self.page(pid)? else {
            return Ok(None);
        };
        // SAFETY: `&mut self`: this is the only reference to the page.
        let page = unsafe { &mut *p.as_ptr() };
        let Ok(i) = page.find(slot) else {
            return Ok(None);
        };
        page.dirty = true;
        if !page.remeasure {
            page.remeasure = true;
            self.remeasure.push(pid);
        }
        Ok(Some(&mut page.rows[i].1))
    }

    /// [`PagedRows::try_get_mut`], panicking on an IO error.
    pub fn get_mut(&mut self, key: u64) -> Option<&mut T> {
        let table = self.table;
        self.try_get_mut(key)
            .unwrap_or_else(|e| panic!("row table {table:#x}: page load failed: {e}"))
    }

    /// Adds the row, or replaces the one under this key (returned).
    pub fn try_insert(&mut self, key: u64, value: T) -> Result<Option<T>> {
        let slot = self.slot_of(key);
        let page = self.page_for_change(self.page_of(key))?;
        let before = page.bytes();
        let add = value.heap_bytes();
        let old = match page.find(slot) {
            Ok(i) => {
                let old = std::mem::replace(&mut page.rows[i].1, value);
                page.heap = page.heap + add - old.heap_bytes();
                Some(old)
            }
            Err(i) => {
                page.rows.insert(i, (slot, value));
                page.heap += add;
                None
            }
        };
        page.dirty = true;
        let after = page.bytes();
        if old.is_none() {
            self.len += 1;
        }
        self.account(after, before);
        Ok(old)
    }

    pub fn insert(&mut self, key: u64, value: T) -> Option<T> {
        let table = self.table;
        self.try_insert(key, value)
            .unwrap_or_else(|e| panic!("row table {table:#x}: page load failed: {e}"))
    }

    pub fn try_remove(&mut self, key: u64) -> Result<Option<T>> {
        let slot = self.slot_of(key);
        let Some(p) = self.page(self.page_of(key))? else {
            return Ok(None);
        };
        // SAFETY: `&mut self`: this is the only reference to the page.
        let page = unsafe { &mut *p.as_ptr() };
        let Ok(i) = page.find(slot) else {
            return Ok(None);
        };
        let before = page.bytes();
        let (_, old) = page.rows.remove(i);
        page.heap -= old.heap_bytes().min(page.heap);
        page.dirty = true;
        let after = page.bytes();
        self.len -= 1;
        self.account(after, before);
        Ok(Some(old))
    }

    pub fn remove(&mut self, key: u64) -> Option<T> {
        let table = self.table;
        self.try_remove(key)
            .unwrap_or_else(|e| panic!("row table {table:#x}: page load failed: {e}"))
    }

    /// Loads the pages of these rows now, so an event can then run without a load that could
    /// fail halfway. An error changes nothing but which pages are resident.
    pub fn ensure(&self, keys: impl IntoIterator<Item = u64>) -> Result<()> {
        for k in keys {
            self.page(self.page_of(k))?;
        }
        Ok(())
    }

    // ---- Budget ----

    /// Evicts least-recently-used pages whose state is on disk until the resident bytes fit the
    /// budget. Call at step boundaries. A resident table evicts nothing. Returns how many pages
    /// went.
    pub fn enforce_budget(&mut self) -> usize {
        for pid in std::mem::take(&mut self.remeasure) {
            if let Some(&p) = self.pages.get_mut().get(&pid) {
                // SAFETY: `&mut self`.
                let page = unsafe { &mut *p.as_ptr() };
                let before = page.bytes();
                page.measure();
                let after = page.bytes();
                self.account(after, before);
            }
        }
        if self.source.is_none() || self.resident_bytes.get() <= self.budget {
            return 0;
        }
        let mut cands: Vec<(u64, u64)> = self
            .pages
            .get_mut()
            .iter()
            .filter_map(|(&id, p)| {
                // SAFETY: `&mut self`.
                let p = unsafe { p.as_ref() };
                (!p.dirty && p.saving == 0).then(|| (p.used.get(), id))
            })
            .collect();
        cands.sort_unstable();
        let mut evicted = 0;
        for (_, id) in cands {
            if self.resident_bytes.get() <= self.budget {
                break;
            }
            self.evict(id);
            evicted += 1;
        }
        if self.resident_bytes.get() > self.budget {
            let mut s = self.stats.get();
            s.over_budget += 1;
            self.stats.set(s);
        }
        evicted
    }

    fn evict(&mut self, pid: u64) {
        let Some(p) = self.pages.get_mut().remove(&pid) else {
            return;
        };
        // SAFETY: the pointer came from `Box::into_raw` (via `Box::leak`) and leaves the map
        // here, so it is freed once.
        let page = unsafe { Box::from_raw(p.as_ptr()) };
        self.account(0, page.bytes());
        let mut s = self.stats.get();
        s.evictions += 1;
        self.stats.set(s);
    }

    // ---- Saves ----

    /// The raw bytes of every page changed since it was last written (`None`: no row left,
    /// delete it), in page order. The pages stay pinned until the ticket comes back through
    /// [`PagedRows::saved`] or [`PagedRows::save_failed`]. Works in a resident table too (the
    /// first save of a table about to [`PagedRows::attach`]).
    #[allow(clippy::type_complexity)]
    pub fn snapshot_dirty(&mut self) -> (Vec<(PageKey, Option<Vec<u8>>)>, SaveTicket) {
        self.snapshots += 1;
        let seq = self.snapshots;
        let mut out = Vec::new();
        let mut pages = Vec::new();
        let mut ids: Vec<u64> = self.pages.get_mut().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let p = self.pages.get_mut()[&id];
            // SAFETY: `&mut self`.
            let page = unsafe { &mut *p.as_ptr() };
            if !page.dirty {
                continue;
            }
            let key = PageKey::new(self.table, id);
            if page.rows.is_empty() {
                out.push((key, None));
            } else {
                let mut w = Writer::with_capacity(64 + 64 * page.rows.len());
                page.encode(&mut w);
                out.push((key, Some(w.into_bytes())));
            }
            page.dirty = false;
            page.saving = seq;
            pages.push(id);
        }
        (out, SaveTicket::new(seq, pages))
    }

    /// [`PagedRows::snapshot_dirty`] into a batch, compressing on the caller.
    pub fn save_dirty(&mut self, batch: &mut SaveBatch) -> SaveTicket {
        let (pages, ticket) = self.snapshot_dirty();
        for (key, raw) in pages {
            match raw {
                Some(raw) => batch.put(key, &raw),
                None => batch.delete(key),
            }
        }
        ticket
    }

    /// The commit holding the ticket's pages is durable: they may be evicted, unless a later
    /// snapshot still holds them.
    pub fn saved(&mut self, ticket: SaveTicket) {
        let (seq, pages) = ticket.into_parts();
        for id in pages {
            if let Some(&p) = self.pages.get_mut().get(&id) {
                // SAFETY: `&mut self`.
                let page = unsafe { &mut *p.as_ptr() };
                if page.saving == seq {
                    page.saving = 0;
                }
            }
        }
    }

    /// The commit failed: the pages are dirty again.
    pub fn save_failed(&mut self, ticket: SaveTicket) {
        let (seq, pages) = ticket.into_parts();
        for id in pages {
            if let Some(&p) = self.pages.get_mut().get(&id) {
                // SAFETY: `&mut self`.
                let page = unsafe { &mut *p.as_ptr() };
                if page.saving == seq {
                    page.saving = 0;
                }
                page.dirty = true;
            }
        }
    }

    // ---- Whole-table passes: they cost the table's size ----

    /// Every page id that exists, resident or in the source, in order.
    pub fn page_ids(&self) -> Result<Vec<u64>> {
        let mut ids: Vec<u64> = match &self.source {
            Some(s) => s.table_ids(self.table)?,
            None => Vec::new(),
        };
        ids.extend(self.pages.borrow().keys().copied());
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    /// Calls `f` on every row in key order, loading pages through `&self` (nothing is evicted:
    /// in a paged table the resident set grows by the table until the next budget check).
    pub fn for_each(&self, mut f: impl FnMut(u64, &T)) {
        let ids = self
            .page_ids()
            .unwrap_or_else(|e| panic!("row table {:#x}: listing pages failed: {e}", self.table));
        for pid in ids {
            let p = self
                .page(pid)
                .unwrap_or_else(|e| panic!("row table {:#x}: page load failed: {e}", self.table));
            if let Some(p) = p {
                // SAFETY: as in `try_get`.
                let page = unsafe { &*p.as_ptr() };
                for (s, v) in &page.rows {
                    f(pid * self.per + u64::from(*s), v);
                }
            }
        }
    }

    /// Removes every row: a whole-table pass (every page becomes an empty, dirty page that the
    /// next save deletes).
    pub fn clear(&mut self) {
        for k in self.keys() {
            self.remove(k);
        }
    }

    /// Every key, in order.
    pub fn keys(&self) -> Vec<u64> {
        let mut out = Vec::with_capacity(self.len as usize);
        self.for_each(|k, _| out.push(k));
        out
    }

    /// Calls `f` on every row in key order, page by page, keeping to the budget as it goes.
    pub fn for_each_bounded(&mut self, mut f: impl FnMut(u64, &T)) -> Result<()> {
        for pid in self.page_ids()? {
            if let Some(p) = self.page(pid)? {
                // SAFETY: `&mut self`; the reference ends before the budget check.
                let page = unsafe { &*p.as_ptr() };
                for (s, v) in &page.rows {
                    f(pid * self.per + u64::from(*s), v);
                }
            }
            self.enforce_budget();
        }
        Ok(())
    }

    /// The rows that are resident now, in key order. Cheap: no load.
    pub fn resident(&self) -> Vec<(u64, &T)> {
        let pages = self.pages.borrow();
        let mut ids: Vec<u64> = pages.keys().copied().collect();
        ids.sort_unstable();
        let mut out = Vec::new();
        for id in ids {
            // SAFETY: as in `try_get`.
            let page = unsafe { &*pages[&id].as_ptr() };
            for (s, v) in &page.rows {
                out.push((id * self.per + u64::from(*s), v));
            }
        }
        out
    }

    /// CRC32 of every non-empty page's id and bytes, in order: equal for equal tables, paged or
    /// resident, whatever was resident when. Keeps to the budget as it goes.
    pub fn state_hash(&mut self) -> Result<u32> {
        let mut h = crc32fast::Hasher::new();
        for pid in self.page_ids()? {
            if let Some(p) = self.page(pid)? {
                // SAFETY: `&mut self`; the reference ends before the budget check.
                let page = unsafe { &*p.as_ptr() };
                if !page.rows.is_empty() {
                    let mut w = Writer::new();
                    w.var_u64(pid);
                    page.encode(&mut w);
                    h.update(w.as_bytes());
                }
            }
            self.enforce_budget();
        }
        Ok(h.finalize())
    }

    /// The whole table as one blob, rows in key order: what a module that saves itself as one
    /// page (no store yet) writes. Equal tables give equal bytes, paged or not.
    pub fn encode_all(&self, w: &mut Writer) {
        w.var_u64(self.len);
        let mut n = 0;
        self.for_each(|k, v| {
            w.var_u64(k);
            v.encode(w);
            n += 1;
        });
        debug_assert_eq!(n, self.len, "row count out of step with the rows");
    }

    /// A resident table from what [`PagedRows::encode_all`] wrote.
    pub fn decode_all(
        table: u16,
        rows_per_page: u64,
        r: &mut Reader<'_>,
    ) -> crate::codec::Result<Self> {
        let n = r.len_prefix(2)?;
        let mut t = Self::new(table, rows_per_page);
        let mut last = None;
        for _ in 0..n {
            let k = r.var_u64()?;
            if last.is_some_and(|l| k <= l) {
                return Err(crate::codec::CodecError::Invalid(
                    "row keys out of order".into(),
                ));
            }
            last = Some(k);
            let v = T::decode(r)?;
            t.insert(k, v);
        }
        Ok(t)
    }
}

/// The row key of a cell `(x, z)` of a grid in `frame`: the frame in bits 40..52 over the z-order
/// (Morton) interleave of the coordinates offset by 2^19, so a page of 4^k keys is a square block
/// of cells and neighbours share pages. Coordinates must lie in `[-2^19, 2^19)` (a cell of 64
/// blocks covers 33 million blocks either way: past Minecraft's 30-million world border) and the
/// frame below 4,096; anything else panics, because two cells must never share a key.
pub fn cell_key(frame: u16, x: i64, z: i64) -> u64 {
    const LIM: i64 = 1 << 19;
    assert!(
        (-LIM..LIM).contains(&x) && (-LIM..LIM).contains(&z) && frame < 1 << 12,
        "cell ({frame}, {x}, {z}) outside the packed key range"
    );
    let bias = |v: i64| (v + LIM) as u32;
    u64::from(frame) << 40 | crate::paging::morton2(bias(x), bias(z))
}

/// The cell [`cell_key`] packed.
pub fn cell_of_key(key: u64) -> (u16, i64, i64) {
    fn squash(mut v: u64) -> u32 {
        v &= 0x5555_5555_5555_5555;
        v = (v | (v >> 1)) & 0x3333_3333_3333_3333;
        v = (v | (v >> 2)) & 0x0f0f_0f0f_0f0f_0f0f;
        v = (v | (v >> 4)) & 0x00ff_00ff_00ff_00ff;
        v = (v | (v >> 8)) & 0x0000_ffff_0000_ffff;
        v = (v | (v >> 16)) & 0x0000_0000_ffff_ffff;
        v as u32
    }
    let m = key & ((1 << 40) - 1);
    let unbias = |u: u32| i64::from(u) - (1 << 19);
    (
        (key >> 40) as u16,
        unbias(squash(m)),
        unbias(squash(m >> 1)),
    )
}

/// [`PagedRows`] behind the [`crate::handle::Arena`] interface: `insert` hands out handles and
/// lookups take them. A handle's index is the row key and is never reused (generation 1 always),
/// so a stale handle finds nothing and no free list or generation table stays resident.
pub struct PagedArena<T: Row> {
    rows: PagedRows<T>,
    next: u32,
}

impl<T: Row> std::fmt::Debug for PagedArena<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagedArena")
            .field("rows", &self.rows)
            .field("next", &self.next)
            .finish()
    }
}

impl<T: Row> PagedArena<T> {
    /// The generation of every handle this arena hands out.
    pub const GENERATION: u16 = 1;

    pub fn new(table: u16, rows_per_page: u64) -> Self {
        PagedArena {
            rows: PagedRows::new(table, rows_per_page),
            next: 0,
        }
    }

    /// An arena saved earlier: `len` rows, the next index `next` (both from the module's own
    /// resident state, [`PagedArena::meta`]).
    pub fn open(
        table: u16,
        rows_per_page: u64,
        source: Arc<dyn PageSource>,
        budget: usize,
        (len, next): (u64, u32),
    ) -> Self {
        PagedArena {
            rows: PagedRows::open(table, rows_per_page, source, budget, len),
            next,
        }
    }

    /// What [`PagedArena::open`] needs back: the row count and the next index.
    pub fn meta(&self) -> (u64, u32) {
        (self.rows.len(), self.next)
    }

    pub fn rows(&self) -> &PagedRows<T> {
        &self.rows
    }

    pub fn rows_mut(&mut self) -> &mut PagedRows<T> {
        &mut self.rows
    }

    pub fn handle(index: u32) -> Handle<T> {
        Handle::from_parts(index, Self::GENERATION)
    }

    fn key(h: Handle<T>) -> Option<u64> {
        (h.generation() == Self::GENERATION).then_some(u64::from(h.index()))
    }

    pub fn len(&self) -> usize {
        self.rows.len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn insert(&mut self, value: T) -> Handle<T> {
        let i = self.next;
        self.next = i.checked_add(1).expect("fewer than 2^32 rows ever");
        self.rows.insert(u64::from(i), value);
        Self::handle(i)
    }

    pub fn get(&self, h: Handle<T>) -> Option<&T> {
        Self::key(h).and_then(|k| self.rows.get(k))
    }

    pub fn try_get(&self, h: Handle<T>) -> Result<Option<&T>> {
        match Self::key(h) {
            Some(k) => self.rows.try_get(k),
            None => Ok(None),
        }
    }

    pub fn get_mut(&mut self, h: Handle<T>) -> Option<&mut T> {
        Self::key(h).and_then(|k| self.rows.get_mut(k))
    }

    pub fn contains(&self, h: Handle<T>) -> bool {
        self.get(h).is_some()
    }

    pub fn remove(&mut self, h: Handle<T>) -> Option<T> {
        Self::key(h).and_then(|k| self.rows.remove(k))
    }

    /// Loads the pages of these rows now (see [`PagedRows::ensure`]).
    pub fn ensure(&self, hs: impl IntoIterator<Item = Handle<T>>) -> Result<()> {
        self.rows.ensure(hs.into_iter().filter_map(Self::key))
    }

    /// Every handle, in order. A whole-table pass.
    pub fn handles(&self) -> Vec<Handle<T>> {
        let mut out = Vec::with_capacity(self.len());
        self.rows.for_each(|k, _| out.push(Self::handle(k as u32)));
        out
    }

    /// Every row in handle order. A whole-table pass that loads through `&self`.
    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.handles()
            .into_iter()
            .map(|h| (h, self.get(h).expect("listed")))
    }

    /// The whole arena as one blob (the next index, then the rows): see
    /// [`PagedRows::encode_all`].
    pub fn encode_all(&self, w: &mut Writer) {
        w.u32(self.next);
        self.rows.encode_all(w);
    }

    pub fn decode_all(
        table: u16,
        rows_per_page: u64,
        r: &mut Reader<'_>,
    ) -> crate::codec::Result<Self> {
        let next = r.u32()?;
        let rows = PagedRows::decode_all(table, rows_per_page, r)?;
        let mut max = None;
        rows.for_each(|k, _| max = Some(k));
        if max.is_some_and(|m| m >= u64::from(next)) {
            return Err(crate::codec::CodecError::Invalid(
                "a row at or past the arena's next index".into(),
            ));
        }
        Ok(PagedArena { rows, next })
    }
}

impl<T: Row> std::ops::Index<Handle<T>> for PagedArena<T> {
    type Output = T;

    fn index(&self, h: Handle<T>) -> &T {
        self.get(h)
            .unwrap_or_else(|| panic!("no live row for handle {h:?}"))
    }
}

impl<T: Row> std::ops::IndexMut<Handle<T>> for PagedArena<T> {
    fn index_mut(&mut self, h: Handle<T>) -> &mut T {
        self.get_mut(h)
            .unwrap_or_else(|| panic!("no live row for handle {h:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;
    use crate::page::Durability;
    use proptest::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct R {
        a: u64,
        tail: Vec<u8>,
    }

    impl Encode for R {
        fn encode(&self, w: &mut Writer) {
            w.var_u64(self.a);
            w.var_u64(self.tail.len() as u64);
            for b in &self.tail {
                w.u8(*b);
            }
        }
    }

    impl Decode for R {
        fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
            let a = r.var_u64()?;
            let n = r.len_prefix(1)?;
            let mut tail = Vec::with_capacity(n);
            for _ in 0..n {
                tail.push(r.u8()?);
            }
            Ok(R { a, tail })
        }
    }

    impl Row for R {
        fn heap_bytes(&self) -> usize {
            self.tail.capacity()
        }
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-rows-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn row(k: u64) -> R {
        R {
            a: k * 3 + 1,
            tail: vec![k as u8; (k % 40) as usize],
        }
    }

    fn save(t: &mut PagedRows<R>, store: &RwLock<PageStore>) {
        let mut b = SaveBatch::new();
        let ticket = t.save_dirty(&mut b);
        store.write().unwrap().commit(b, 0).unwrap();
        t.saved(ticket);
        t.enforce_budget();
    }

    #[test]
    fn pages_out_under_a_budget_and_reads_load_back() {
        let dir = tmpdir("budget");
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).unwrap(),
        ));
        let mut t: PagedRows<R> = PagedRows::new(7, 16);
        for k in 0..1_000 {
            t.insert(k * 3, row(k));
        }
        t.attach(store.clone(), 4_000);
        assert_eq!(t.enforce_budget(), 0, "nothing is on disk yet");
        save(&mut t, &store);
        assert!(t.resident_bytes() <= 4_000, "{}", t.resident_bytes());
        // Reads through `&self` load what they need.
        let r = &t;
        let v: Vec<&R> = (0..1_000)
            .step_by(97)
            .map(|k| r.get(k * 3).unwrap())
            .collect();
        assert_eq!(v[3], &row(3 * 97));
        assert!(t.stats().loads > 5);
        assert_eq!(t.get(1), None);
        // A change dirties the page; it stays until saved.
        t.get_mut(300).unwrap().a = 99;
        t.set_budget(0);
        t.enforce_budget();
        assert!(t.is_resident(300));
        save(&mut t, &store);
        assert_eq!(t.resident_pages(), 0);
        assert_eq!(t.get(300).unwrap().a, 99);
        assert_eq!(t.len(), 1_000);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reopened_tables_read_what_was_saved() {
        let dir = tmpdir("reopen");
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).unwrap(),
        ));
        let mut t: PagedRows<R> = PagedRows::new(7, 8);
        for k in 0..100 {
            t.insert(k, row(k));
        }
        t.remove(50);
        let h = t.state_hash().unwrap();
        t.attach(store.clone(), 0);
        save(&mut t, &store);
        let len = t.len();
        drop(t);
        let mut back: PagedRows<R> = PagedRows::open(7, 8, store.clone(), 0, len);
        assert_eq!(back.state_hash().unwrap(), h);
        assert_eq!(back.get(51), Some(&row(51)));
        assert_eq!(back.get(50), None);
        assert_eq!(back.keys().len(), 99);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_saves_in_flight_keep_the_later_change() {
        let dir = tmpdir("two");
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).unwrap(),
        ));
        let mut t: PagedRows<R> = PagedRows::new(7, 8);
        for k in 0..16 {
            t.insert(k, row(k));
        }
        t.attach(store.clone(), 0);
        save(&mut t, &store);
        t.get_mut(3).unwrap().a = 1;
        let mut b1 = SaveBatch::new();
        let t1 = t.save_dirty(&mut b1);
        t.get_mut(3).unwrap().a = 2;
        let mut b2 = SaveBatch::new();
        let t2 = t.save_dirty(&mut b2);
        store.write().unwrap().commit(b1, 1).unwrap();
        t.saved(t1);
        t.enforce_budget();
        assert!(t.is_resident(3), "the second save still holds the page");
        t.save_failed(t2);
        save(&mut t, &store);
        assert!(!t.is_resident(3));
        assert_eq!(t.get(3).unwrap().a, 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_read_is_an_error_and_changes_nothing() {
        let dir = tmpdir("fault");
        let store = Arc::new(RwLock::new(
            PageStore::open(&dir, Durability::NoSync).unwrap(),
        ));
        let mut t: PagedRows<R> = PagedRows::new(7, 8);
        for k in 0..64 {
            t.insert(k, row(k));
        }
        t.attach(store.clone(), 0);
        save(&mut t, &store);
        store
            .write()
            .unwrap()
            .inject_fault(Some(crate::page::Fault::Read { after: 1 }));
        assert!(t.ensure([0, 20]).is_err());
        assert!(t.try_get(40).is_err());
        assert!(t.try_get_mut(41).is_err());
        store.write().unwrap().inject_fault(None);
        assert_eq!(t.get(40), Some(&row(40)));
        assert_eq!(t.len(), 64);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cell_keys_round_trip_and_keep_neighbours_together() {
        for &(f, x, z) in &[
            (0u16, 0i64, 0i64),
            (3, -1, 0),
            (4095, (1 << 19) - 1, -(1 << 19)),
            (7, 12_345, -54_321),
        ] {
            assert_eq!(cell_of_key(cell_key(f, x, z)), (f, x, z));
        }
        // An aligned 2 x 2 block shares a page of 4, on both sides of zero.
        for (x0, z0) in [(2, 2), (-2, -4), (-2, 0)] {
            let p: Vec<u64> = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|&(dx, dz)| cell_key(1, x0 + dx, z0 + dz) / 4)
                .collect();
            assert!(p.iter().all(|&k| k == p[0]), "{x0},{z0}: {p:?}");
        }
        assert!(std::panic::catch_unwind(|| cell_key(0, 1 << 19, 0)).is_err());
    }

    #[test]
    fn arena_handles_are_never_reused() {
        let mut a: PagedArena<R> = PagedArena::new(3, 4);
        let h0 = a.insert(row(0));
        let h1 = a.insert(row(1));
        assert_eq!(a[h1], row(1));
        a.remove(h0);
        let h2 = a.insert(row(2));
        assert_ne!(h2, h0);
        assert_eq!(a.get(h0), None);
        assert_eq!(a.handles(), vec![h1, h2]);
        a[h2].a = 7;
        assert_eq!(a.get(h2).unwrap().a, 7);
        let mut w = Writer::new();
        a.encode_all(&mut w);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let b: PagedArena<R> = PagedArena::decode_all(3, 4, &mut r).unwrap();
        r.finish().unwrap();
        assert_eq!(b.meta(), a.meta());
        assert_eq!(b.handles(), a.handles());
        assert!(
            a.get(Handle::from_parts(1, 2)).is_none(),
            "another generation"
        );
    }

    #[derive(Clone, Debug)]
    enum Op {
        Insert(u64, u8),
        Remove(u64),
        Change(u64, u64),
        Read(u64),
        Save,
        Budget(u16),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            4 => (0u64..300, any::<u8>()).prop_map(|(k, b)| Op::Insert(k, b)),
            1 => (0u64..300).prop_map(Op::Remove),
            3 => (0u64..300, any::<u64>()).prop_map(|(k, v)| Op::Change(k, v)),
            3 => (0u64..300).prop_map(Op::Read),
            1 => Just(Op::Save),
            1 => (0u16..2_000).prop_map(Op::Budget),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        /// A paged table under any budget, saved and evicted at any moment, holds exactly what a
        /// resident one does, and both hash the same, before and after a reopen.
        #[test]
        fn paged_equals_resident(ops in prop::collection::vec(op(), 1..300)) {
            let dir = tmpdir("prop");
            let store = Arc::new(RwLock::new(PageStore::open(&dir, Durability::NoSync).unwrap()));
            let mut a: PagedRows<R> = PagedRows::new(9, 8);
            let mut b: PagedRows<R> = PagedRows::new(9, 8);
            b.attach(store.clone(), 1_000);
            for op in &ops {
                match *op {
                    Op::Insert(k, x) => {
                        let v = R { a: k, tail: vec![x; (x % 50) as usize] };
                        prop_assert_eq!(a.insert(k, v.clone()), b.insert(k, v));
                    }
                    Op::Remove(k) => prop_assert_eq!(a.remove(k), b.remove(k)),
                    Op::Change(k, v) => {
                        if let Some(r) = a.get_mut(k) { r.a = v; }
                        if let Some(r) = b.get_mut(k) { r.a = v; }
                    }
                    Op::Read(k) => prop_assert_eq!(a.get(k), b.get(k)),
                    Op::Save => save(&mut b, &store),
                    Op::Budget(x) => {
                        b.set_budget(usize::from(x));
                        b.enforce_budget();
                    }
                }
                prop_assert_eq!(a.len(), b.len());
            }
            let ha = a.state_hash().unwrap();
            prop_assert_eq!(ha, b.state_hash().unwrap());
            let mut wa = Writer::new();
            a.encode_all(&mut wa);
            let mut wb = Writer::new();
            b.encode_all(&mut wb);
            prop_assert_eq!(wa.as_bytes(), wb.as_bytes());
            save(&mut b, &store);
            let len = b.len();
            drop(b);
            let mut c: PagedRows<R> = PagedRows::open(9, 8, store.clone(), 0, len);
            prop_assert_eq!(ha, c.state_hash().unwrap());
            prop_assert_eq!(c.resident_pages(), 0);
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}
