// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paging under a memory budget (A16, SCALE.md §8): lazy regions that live in pages on disk and
//! come into memory only while something uses them.
//!
//! # What is resident
//!
//! A [`PagedTable`] keeps in memory:
//! - the **resident pages**, at most [`PagingConfig::budget_bytes`] of them (a soft bound, see
//!   below), in least-recently-used order;
//! - the **resident id index**: page id to resident page. A region's page is its key divided by
//!   [`PagingConfig::regions_per_page`], so a region that is not resident costs no memory at all.
//!   Keys are stable ids (packed coordinates, say). Anything that refers to a region from another
//!   region holds its key, never a pointer or an arena handle, so evicting a page never means
//!   fixing anything up: a page is self-contained (anchor, booked wake and model of each region);
//! - the **timing wheel** of booked wakes for every region, resident or not (SCALE.md §8: the
//!   wheel is what says which regions wake next), and a second wheel of prefetches.
//!
//! The strategic tables (settlements, notables, the flow graph) are not here: they are always
//! resident and live in their own tables.
//!
//! # Loading
//!
//! Every access goes through catch-up anyway (SCALE.md §4), so a page is loaded when a region on it
//! is read, receives an event, or wakes. A booked wake fires through [`PagedTable::next_due`] (or
//! the parallel step in [`crate::bsp`]), which loads its page first. Prefetch keeps those loads off
//! the step: see [`PagedTable::prefetch_due`].
//!
//! # Eviction
//!
//! [`PagedTable::enforce_budget`], called at step boundaries (never in the middle of a step),
//! drops least-recently-used pages until the resident bytes fit the budget. Only pages whose state
//! is already on disk may go:
//! - a page changed by an event, an insert, a removal or a breakpoint is **dirty** until a save
//!   writes it ([`PagedTable::save_dirty`], then [`PagedTable::saved`] once the commit is durable);
//! - a page changed only by **catch-up** may go at once. Catch-up is a pure function of the stored
//!   state and the time (DECISIONS.md O4), so a reload recomputes exactly the same slot commits.
//!
//! When every remaining page is dirty the table stays over budget until the next save, and counts
//! it ([`PagingStats::over_budget`]). Dirty pages are bounded by what happened since the last save,
//! not by the world's size. Writing dirty pages out early (a spill) would make a save generation
//! outside Minecraft's save, which P29 has to allow first; it is not built.
//!
//! # Wakes of regions that are not resident
//!
//! A wheel entry is `(region key, booked time)`. It is live when the region's own booked wake (saved
//! in its page) still equals that time; any other entry is stale and is dropped when it fires, as
//! in [`crate::lazy`] (DECISIONS.md O6), only here the check needs the page, so a stale entry may
//! cost a load. Entries due at the same tick fire in key order, whatever order they were booked in,
//! so a table restored from a save fires them in the same order as one that never stopped.
//!
//! # Prefetch
//!
//! Two sources say which pages will be needed soon:
//! - **the timing wheel**: each booked wake also books a prefetch of its page
//!   [`PagingConfig::prefetch_lead`] ticks earlier, and [`PagedTable::prefetch_at`] does the same
//!   for an event the caller has scheduled (a caravan's arrival, a leg crossing a region, A15);
//! - **read-ahead**: [`PagedTable::read_ahead`] takes the regions a player is heading into
//!   ([`read_ahead_grid`] computes them for a square grid of regions).
//!
//! The engine owns no threads, so prefetch is split for the host's IO lane:
//! [`PagedTable::prefetch_due`] returns what to load, the IO lane reads each with
//! [`crate::page::PageLocation::read`] (no access to the store needed), and the step installs the
//! bytes with [`PagedTable::install`], which drops them if the page changed on disk meanwhile.
//! A prefetched page enters at the hot end of the LRU list, as if just used.
//! [`PagedTable::prefetch`] does all of it inline, for tests and the bench.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::mem::size_of;

use crate::clock::Tick;
use crate::codec::{Decode, Encode, Reader, Writer};
use crate::ladder::{Answers, Query};
use crate::lazy::{Lazy, LazyModel, LazyStats};
use crate::page::{PageError, PageKey, PageLocation, PageStore, Result, SaveBatch};
use crate::rng::{StreamKey, SystemId};
use crate::wheel::TimingWheel;

/// A lazy model that can live in a page.
pub trait PagedModel: LazyModel + Encode + Decode {
    /// Heap bytes the model owns beyond its own size (vectors, boxes), for the budget.
    fn heap_bytes(&self) -> usize {
        0
    }
}

/// The resident budget when nothing else is configured. PLACEHOLDER: A16 sets the default from
/// phase 0 measurements, and P24 holds the targets; 64 MiB is only a number that fits a potato.
pub const DEFAULT_BUDGET_BYTES: usize = 64 << 20;
/// Regions per page when nothing else is configured. PLACEHOLDER, the scale bench's value.
pub const DEFAULT_REGIONS_PER_PAGE: u64 = 64;
/// How long before a booked wake its page is prefetched: 5 seconds of game time. PLACEHOLDER.
pub const DEFAULT_PREFETCH_LEAD: u64 = 100;

/// How a [`PagedTable`] pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagingConfig {
    /// The page table id of this table's region pages (the top 16 bits of a [`PageKey`]).
    pub table: u16,
    /// The page table id of this table's own state: the clock and the booked wakes.
    pub meta_table: u16,
    /// A region's page is `key / regions_per_page`. Pack keys so neighbours share pages.
    pub regions_per_page: u64,
    /// Resident page bytes [`PagedTable::enforce_budget`] aims for.
    pub budget_bytes: usize,
    /// Ticks before a booked wake that its page is prefetched; 0 turns prefetch off.
    pub prefetch_lead: u64,
}

impl PagingConfig {
    pub fn new(table: u16, meta_table: u16) -> Self {
        PagingConfig {
            table,
            meta_table,
            regions_per_page: DEFAULT_REGIONS_PER_PAGE,
            budget_bytes: DEFAULT_BUDGET_BYTES,
            prefetch_lead: DEFAULT_PREFETCH_LEAD,
        }
    }
}

/// Counters for the profiler and the bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PagingStats {
    /// Pages read from the store because something needed them now.
    pub loads: u64,
    /// Pages read ahead of need ([`PagedTable::install`]).
    pub prefetches: u64,
    /// Prefetched pages that were then used before being evicted.
    pub prefetch_hits: u64,
    /// Prefetched pages evicted unused.
    pub prefetch_wasted: u64,
    /// Stored (compressed) bytes read, by loads and prefetches.
    pub load_bytes: u64,
    pub evictions: u64,
    /// Evicted pages whose only changes were catch-ups (redone on reload).
    pub dropped_catch_ups: u64,
    /// Budget checks that ended over budget because every remaining page was dirty.
    pub over_budget: u64,
    pub peak_resident_bytes: u64,
}

/// A fast hash for integer ids (SplitMix64's finalizer). Not for keys an attacker chooses.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdHasher(u64);

#[inline]
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = mix(self.0 ^ u64::from(b));
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = mix(self.0 ^ n);
    }
}

/// A map keyed by integer ids, hashed with [`IdHasher`].
pub type IdMap<K, V> = HashMap<K, V, BuildHasherDefault<IdHasher>>;

/// The z-order (Morton) key of a region at grid `(x, z)`: neighbours get nearby keys, so a page of
/// 4^k regions is a square block. Each coordinate keeps its low 32 bits.
pub fn morton2(x: u32, z: u32) -> u64 {
    fn spread(v: u32) -> u64 {
        let mut v = u64::from(v);
        v = (v | (v << 16)) & 0x0000_ffff_0000_ffff;
        v = (v | (v << 8)) & 0x00ff_00ff_00ff_00ff;
        v = (v | (v << 4)) & 0x0f0f_0f0f_0f0f_0f0f;
        v = (v | (v << 2)) & 0x3333_3333_3333_3333;
        v = (v | (v << 1)) & 0x5555_5555_5555_5555;
        v
    }
    spread(x) | spread(z) << 1
}

/// Player-movement read-ahead on a grid of regions: the cells a player at `(x, z)`, moving
/// `(dx, dz)` regions per step, reaches within `steps` steps, widened by `radius` on each side.
/// Cells outside `[0, side)` are skipped. The caller maps cells to keys and passes them to
/// [`PagedTable::read_ahead`].
pub fn read_ahead_grid(
    (x, z): (i64, i64),
    (dx, dz): (i64, i64),
    steps: u32,
    radius: i64,
    side: i64,
) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for s in 1..=i64::from(steps) {
        let (cx, cz) = (x + dx * s, z + dz * s);
        for oz in -radius..=radius {
            for ox in -radius..=radius {
                let (px, pz) = (cx + ox, cz + oz);
                if (0..side).contains(&px) && (0..side).contains(&pz) {
                    out.push((px, pz));
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// A wheel entry: the region and the time its wake was booked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Booked {
    pub(crate) key: u64,
    pub(crate) at: Tick,
}

pub(crate) struct Slot<M> {
    pub(crate) cell: Lazy<M>,
    pub(crate) wake: Option<Tick>,
}

pub(crate) struct Page<M> {
    pub(crate) id: u64,
    pub(crate) slots: Vec<Option<Slot<M>>>,
    pub(crate) present: u32,
    /// Changed by something other than catch-up since it was last written: it must be saved
    /// before it may be evicted.
    pub(crate) dirty: bool,
    /// Changed by catch-up only: may be evicted and recomputed.
    pub(crate) caught_up: bool,
    /// In a save whose commit is not yet durable: may not be evicted.
    pub(crate) saving: bool,
    /// Loaded by prefetch and not used yet.
    pub(crate) prefetched: bool,
    /// The models' own heap bytes.
    pub(crate) heap: usize,
}

/// Bytes a resident page costs in the id index (a hash map entry with its control byte and load
/// factor, rounded up). An estimate for the budget.
const INDEX_ENTRY_BYTES: usize = 24;

impl<M: PagedModel> Page<M> {
    fn empty(id: u64, per: u64) -> Self {
        Page {
            id,
            slots: (0..per).map(|_| None).collect(),
            present: 0,
            dirty: false,
            caught_up: false,
            saving: false,
            prefetched: false,
            heap: 0,
        }
    }

    pub(crate) fn bytes(&self) -> usize {
        size_of::<Page<M>>()
            + self.slots.capacity() * size_of::<Option<Slot<M>>>()
            + self.heap
            + INDEX_ENTRY_BYTES
    }

    /// The page's bytes: each present region's slot, anchor, booked wake and model. Nothing in a
    /// page points outside it.
    pub(crate) fn encode(&self, w: &mut Writer) {
        w.var_u64(u64::from(self.present));
        for (i, s) in self.slots.iter().enumerate() {
            if let Some(s) = s {
                w.var_u64(i as u64);
                w.var_u64(s.cell.anchor().0);
                w.var_u64(s.wake.map_or(0, |t| t.0 + 1));
                s.cell.model().encode(w);
            }
        }
    }

    fn decode(id: u64, per: u64, bytes: &[u8]) -> Result<Self> {
        let bad = |e: crate::codec::CodecError| PageError::Format(format!("page {id}: {e}"));
        let mut page = Page::empty(id, per);
        let mut r = Reader::new(bytes);
        let n = r.var_u64().map_err(bad)?;
        let mut next = 0u64;
        for _ in 0..n {
            let i = r.var_u64().map_err(bad)?;
            if i < next || i >= per {
                return Err(PageError::Format(format!(
                    "page {id}: slot {i} out of order or past {per}"
                )));
            }
            next = i + 1;
            let anchor = Tick(r.var_u64().map_err(bad)?);
            let wake = match r.var_u64().map_err(bad)? {
                0 => None,
                t => Some(Tick(t - 1)),
            };
            let model = M::decode(&mut r).map_err(bad)?;
            page.heap += model.heap_bytes();
            page.slots[i as usize] = Some(Slot {
                cell: Lazy::new(model, anchor),
                wake,
            });
            page.present += 1;
        }
        r.finish().map_err(bad)?;
        Ok(page)
    }
}

const NIL: u32 = u32::MAX;

/// A page's place in the LRU list. Only evictable pages (neither dirty nor in a save) are in the
/// list, so eviction never walks past pages it may not take.
#[derive(Clone, Copy)]
struct Link {
    prev: u32,
    next: u32,
    linked: bool,
}

/// What [`PagedTable::save_dirty`] put in a batch; hand it back with [`PagedTable::saved`] once the
/// commit is durable, or [`PagedTable::save_failed`].
#[derive(Debug, Default)]
#[must_use = "pages stay pinned until the ticket comes back through saved or save_failed"]
pub struct SaveTicket {
    pages: Vec<u64>,
}

impl SaveTicket {
    pub fn pages(&self) -> usize {
        self.pages.len()
    }
}

/// A page to read ahead of need, from [`PagedTable::prefetch_due`].
#[derive(Clone, Debug)]
pub struct Prefetch {
    pub page: u64,
    pub key: PageKey,
    pub location: PageLocation,
}

/// Lazy regions of one model, paged to a [`PageStore`] under a memory budget. The paged
/// counterpart of [`crate::lazy::LazyTable`], with the same rules (module docs there): drain
/// [`PagedTable::next_due`] up to `t` before reading or applying an event at `t`.
///
/// The store is passed to every call that may load a page; the table never owns it, so one
/// store holds every table's pages and one commit saves them all.
pub struct PagedTable<M: PagedModel> {
    pub(crate) cfg: PagingConfig,
    world_seed: u64,
    system: SystemId,
    pub(crate) slab: Vec<Option<Page<M>>>,
    links: Vec<Link>,
    free: Vec<u32>,
    pub(crate) index: IdMap<u64, u32>,
    head: u32,
    tail: u32,
    pub(crate) resident_bytes: usize,
    pub(crate) wheel: TimingWheel<Booked>,
    /// Entries popped for `due_at`, sorted so `pop` gives them in key order.
    pub(crate) due: Vec<Booked>,
    due_at: Tick,
    prefetch: TimingWheel<u64>,
    hints: Vec<u64>,
    pub(crate) stats: PagingStats,
    pub(crate) lazy: LazyStats,
}

impl<M: PagedModel> PagedTable<M> {
    pub fn new(world_seed: u64, system: SystemId, now: Tick, cfg: PagingConfig) -> Self {
        assert!(
            cfg.regions_per_page > 0,
            "regions_per_page must be positive"
        );
        PagedTable {
            cfg,
            world_seed,
            system,
            slab: Vec::new(),
            links: Vec::new(),
            free: Vec::new(),
            index: IdMap::default(),
            head: NIL,
            tail: NIL,
            resident_bytes: 0,
            wheel: TimingWheel::new(now),
            due: Vec::new(),
            due_at: now,
            prefetch: TimingWheel::new(now),
            hints: Vec::new(),
            stats: PagingStats::default(),
            lazy: LazyStats::default(),
        }
    }

    /// Opens a table saved with [`PagedTable::save_meta`]: the clock and every booked wake come
    /// back, and no page is resident. A store with no meta page gives an empty table at tick 0.
    pub fn open(
        world_seed: u64,
        system: SystemId,
        cfg: PagingConfig,
        store: &mut PageStore,
    ) -> Result<Self> {
        let meta = store.get(PageKey::new(cfg.meta_table, 0))?;
        let Some(meta) = meta else {
            return Ok(Self::new(world_seed, system, Tick::ZERO, cfg));
        };
        let bad = |e: crate::codec::CodecError| PageError::Format(format!("paged table meta: {e}"));
        let mut r = Reader::new(&meta);
        let now = Tick(r.var_u64().map_err(bad)?);
        let mut t = Self::new(world_seed, system, now, cfg);
        let n = r.var_u64().map_err(bad)?;
        for _ in 0..n {
            let at = Tick(r.var_u64().map_err(bad)?);
            let key = r.var_u64().map_err(bad)?;
            let booked = Tick(r.var_u64().map_err(bad)?);
            t.wheel.schedule(at.max(now), Booked { key, at: booked });
            t.book_prefetch(t.page_of(key), booked);
        }
        r.finish().map_err(bad)?;
        Ok(t)
    }

    pub fn config(&self) -> &PagingConfig {
        &self.cfg
    }

    pub fn set_budget(&mut self, bytes: usize) {
        self.cfg.budget_bytes = bytes;
    }

    pub fn page_of(&self, key: u64) -> u64 {
        key / self.cfg.regions_per_page
    }

    fn slot_of(&self, key: u64) -> usize {
        (key % self.cfg.regions_per_page) as usize
    }

    fn page_key(&self, page: u64) -> PageKey {
        PageKey::new(self.cfg.table, page)
    }

    pub(crate) fn stream(&self, key: u64) -> StreamKey {
        StreamKey::new(self.world_seed, self.system, key)
    }

    pub(crate) fn world_seed(&self) -> u64 {
        self.world_seed
    }

    pub(crate) fn system(&self) -> SystemId {
        self.system
    }

    /// The latest `limit` [`PagedTable::next_due`] (or a parallel step) has run to.
    pub fn now(&self) -> Tick {
        self.wheel.now()
    }

    pub fn stats(&self) -> PagingStats {
        self.stats
    }

    pub fn lazy_stats(&self) -> LazyStats {
        self.lazy
    }

    pub fn reset_stats(&mut self) {
        self.stats = PagingStats {
            peak_resident_bytes: self.resident_bytes as u64,
            ..PagingStats::default()
        };
        self.lazy = LazyStats::default();
    }

    /// Resident page bytes, the quantity the budget bounds.
    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    pub fn resident_pages(&self) -> usize {
        self.index.len()
    }

    pub fn is_resident(&self, key: u64) -> bool {
        self.index.contains_key(&self.page_of(key))
    }

    /// Pending wake entries, stale ones included.
    pub fn pending_wakes(&self) -> usize {
        self.wheel.len() + self.due.len()
    }

    /// Every byte the table holds: resident pages, the id index, both wheels and buffers.
    pub fn heap_bytes(&self) -> usize {
        let index = self.index.capacity() * (size_of::<(u64, u32)>() + 1);
        self.resident_bytes - self.index.len() * INDEX_ENTRY_BYTES
            + index
            + self.slab.capacity() * size_of::<Option<Page<M>>>()
            + self.links.capacity() * size_of::<Link>()
            + self.free.capacity() * 4
            + self.wheel.heap_bytes()
            + self.prefetch.heap_bytes()
            + self.hints.capacity() * 8
            + self.due.capacity() * size_of::<Booked>()
    }

    // ---- The LRU list over evictable resident pages ----

    fn unlink(&mut self, i: u32) {
        let Link { prev, next, linked } = self.links[i as usize];
        if !linked {
            return;
        }
        self.links[i as usize].linked = false;
        if prev != NIL {
            self.links[prev as usize].next = next;
        } else {
            self.head = next;
        }
        if next != NIL {
            self.links[next as usize].prev = prev;
        } else {
            self.tail = prev;
        }
    }

    fn push_front(&mut self, i: u32) {
        debug_assert!(!self.links[i as usize].linked);
        self.links[i as usize] = Link {
            prev: NIL,
            next: self.head,
            linked: true,
        };
        if self.head != NIL {
            self.links[self.head as usize].prev = i;
        } else {
            self.tail = i;
        }
        self.head = i;
    }

    fn evictable(&self, i: u32) -> bool {
        let page = self.slab[i as usize].as_ref().expect("resident page");
        !page.dirty && !page.saving
    }

    /// Puts the page in the LRU list if it may be evicted, takes it out if not. Call after any
    /// change to its dirty or saving state.
    pub(crate) fn sync(&mut self, i: u32) {
        match (self.evictable(i), self.links[i as usize].linked) {
            (true, false) => self.push_front(i),
            (false, true) => self.unlink(i),
            _ => {}
        }
    }

    fn touch(&mut self, i: u32) {
        if self.links[i as usize].linked && self.head != i {
            self.unlink(i);
            self.push_front(i);
        }
        let page = self.slab[i as usize].as_mut().expect("resident page");
        if page.prefetched {
            page.prefetched = false;
            self.stats.prefetch_hits += 1;
        }
    }

    fn install_page(&mut self, page: Page<M>) -> u32 {
        let id = page.id;
        self.resident_bytes += page.bytes();
        self.stats.peak_resident_bytes = self
            .stats
            .peak_resident_bytes
            .max(self.resident_bytes as u64);
        let i = if let Some(i) = self.free.pop() {
            self.slab[i as usize] = Some(page);
            i
        } else {
            self.slab.push(Some(page));
            self.links.push(Link {
                prev: NIL,
                next: NIL,
                linked: false,
            });
            u32::try_from(self.slab.len() - 1).expect("fewer than 2^32 resident pages")
        };
        self.index.insert(id, i);
        self.links[i as usize].linked = false;
        self.sync(i);
        i
    }

    fn evict(&mut self, i: u32) {
        self.unlink(i);
        let page = self.slab[i as usize].take().expect("resident page");
        self.index.remove(&page.id);
        self.resident_bytes -= page.bytes();
        self.free.push(i);
        self.stats.evictions += 1;
        if page.caught_up {
            self.stats.dropped_catch_ups += 1;
        }
        if page.prefetched {
            self.stats.prefetch_wasted += 1;
        }
    }

    /// Recomputes a page's share of the resident bytes after its contents changed size.
    pub(crate) fn reaccount(&mut self, i: u32, before: usize) {
        let after = self.slab[i as usize]
            .as_ref()
            .expect("resident page")
            .bytes();
        self.resident_bytes = self.resident_bytes + after - before;
        self.stats.peak_resident_bytes = self
            .stats
            .peak_resident_bytes
            .max(self.resident_bytes as u64);
    }

    /// The page's slab index, loading it if it is on disk; `None` if it exists nowhere.
    pub(crate) fn ensure(&mut self, store: &mut PageStore, page: u64) -> Result<Option<u32>> {
        if let Some(&i) = self.index.get(&page) {
            self.touch(i);
            return Ok(Some(i));
        }
        let Some(raw) = store.get(self.page_key(page))? else {
            return Ok(None);
        };
        if let Some(loc) = store.locate(self.page_key(page)) {
            self.stats.load_bytes += loc.stored_len() as u64;
        }
        let p = Page::decode(page, self.cfg.regions_per_page, &raw)?;
        self.stats.loads += 1;
        Ok(Some(self.install_page(p)))
    }

    fn ensure_or_create(&mut self, store: &mut PageStore, page: u64) -> Result<u32> {
        if let Some(i) = self.ensure(store, page)? {
            return Ok(i);
        }
        Ok(self.install_page(Page::empty(page, self.cfg.regions_per_page)))
    }

    fn book_prefetch(&mut self, page: u64, at: Tick) {
        if self.cfg.prefetch_lead == 0 {
            return;
        }
        let when = Tick(at.0.saturating_sub(self.cfg.prefetch_lead)).max(self.prefetch.now());
        self.prefetch.schedule(when, page);
    }

    /// Books the region's next breakpoint from its anchor. Called after inserts, events and
    /// breakpoints, never after reads (DECISIONS.md O4).
    pub(crate) fn book(&mut self, key: u64, wake: Option<Tick>) {
        if let Some(at) = wake {
            self.wheel
                .schedule(at.max(self.wheel.now()), Booked { key, at });
            self.book_prefetch(self.page_of(key), at);
        }
    }

    fn reschedule(&mut self, i: u32, key: u64) -> Option<Tick> {
        let s = self.slot_of(key);
        let slot = self.slab[i as usize].as_mut().expect("resident page").slots[s]
            .as_mut()
            .expect("present region");
        slot.wake = slot.cell.model().next_breakpoint(slot.cell.anchor());
        if let Some(at) = slot.wake {
            debug_assert!(at > slot.cell.anchor(), "breakpoint not after the anchor");
        }
        let wake = slot.wake;
        self.book(key, wake);
        wake
    }

    // ---- Regions ----

    /// Adds a region (or replaces the one with this key) as of `at`, and books its breakpoint.
    pub fn insert(&mut self, store: &mut PageStore, key: u64, model: M, at: Tick) -> Result<()> {
        let pid = self.page_of(key);
        assert!(pid < 1 << 48, "region key {key} past the page id range");
        let i = self.ensure_or_create(store, pid)?;
        let s = self.slot_of(key);
        let page = self.slab[i as usize].as_mut().expect("resident page");
        let before = page.bytes();
        if let Some(old) = page.slots[s].take() {
            page.heap -= old.cell.model().heap_bytes();
        } else {
            page.present += 1;
        }
        page.heap += model.heap_bytes();
        page.slots[s] = Some(Slot {
            cell: Lazy::new(model, at),
            wake: None,
        });
        page.dirty = true;
        self.reaccount(i, before);
        self.sync(i);
        self.reschedule(i, key);
        Ok(())
    }

    /// Removes a region; its booked wake goes stale. Returns its cell.
    pub fn remove(&mut self, store: &mut PageStore, key: u64) -> Result<Option<Lazy<M>>> {
        let Some(i) = self.ensure(store, self.page_of(key))? else {
            return Ok(None);
        };
        let s = self.slot_of(key);
        let page = self.slab[i as usize].as_mut().expect("resident page");
        let before = page.bytes();
        let Some(old) = page.slots[s].take() else {
            return Ok(None);
        };
        page.present -= 1;
        page.heap -= old.cell.model().heap_bytes();
        page.dirty = true;
        self.reaccount(i, before);
        self.sync(i);
        Ok(Some(old.cell))
    }

    /// Whether the region exists (loading its page to find out).
    pub fn contains(&mut self, store: &mut PageStore, key: u64) -> Result<bool> {
        let Some(i) = self.ensure(store, self.page_of(key))? else {
            return Ok(false);
        };
        let s = self.slot_of(key);
        Ok(self.slab[i as usize].as_ref().expect("resident page").slots[s].is_some())
    }

    /// A read: load the page if needed, catch up, project. `None` if there is no such region.
    pub fn read(&mut self, store: &mut PageStore, key: u64, at: Tick) -> Result<Option<M::View>> {
        debug_assert!(
            at <= self.wheel.now(),
            "read at {at:?} before next_due was drained to it (wheel at {:?})",
            self.wheel.now()
        );
        let Some(i) = self.ensure(store, self.page_of(key))? else {
            return Ok(None);
        };
        let s = self.slot_of(key);
        let stream = self.stream(key);
        let page = self.slab[i as usize].as_mut().expect("resident page");
        let Some(slot) = page.slots[s].as_mut() else {
            return Ok(None);
        };
        self.lazy.reads += 1;
        let commits = slot.cell.catch_up(at, stream);
        if commits > 0 {
            page.caught_up = true;
            self.lazy.slot_commits += commits;
        }
        Ok(Some(slot.cell.view_without_catch_up(at)))
    }

    /// A question answered from the region's view (SCALE.md §2).
    pub fn query<Q: Query>(
        &mut self,
        store: &mut PageStore,
        key: u64,
        at: Tick,
        q: &Q,
    ) -> Result<Option<Q::Answer>>
    where
        M::View: Answers<Q>,
    {
        Ok(self.read(store, key, at)?.map(|v| v.answer(q)))
    }

    /// An event at `at`: load, catch up, settle to `at`, apply `f`, book the next breakpoint.
    pub fn apply<R>(
        &mut self,
        store: &mut PageStore,
        key: u64,
        at: Tick,
        f: impl FnOnce(&mut M, Tick) -> R,
    ) -> Result<Option<R>> {
        debug_assert!(
            at <= self.wheel.now(),
            "event at {at:?} before next_due was drained to it (wheel at {:?})",
            self.wheel.now()
        );
        let Some(i) = self.ensure(store, self.page_of(key))? else {
            return Ok(None);
        };
        let s = self.slot_of(key);
        let stream = self.stream(key);
        let page = self.slab[i as usize].as_mut().expect("resident page");
        let before = page.bytes();
        let Some(slot) = page.slots[s].as_mut() else {
            return Ok(None);
        };
        self.lazy.events += 1;
        self.lazy.slot_commits += slot.cell.catch_up(at, stream);
        slot.cell.settle_to(at);
        let heap0 = slot.cell.model().heap_bytes();
        let r = f(slot.cell.model_mut(), at);
        let heap1 = slot.cell.model().heap_bytes();
        page.heap = page.heap + heap1 - heap0;
        page.dirty = true;
        self.reaccount(i, before);
        self.sync(i);
        self.reschedule(i, key);
        Ok(Some(r))
    }

    /// Pops the next live breakpoint due at or before `limit`, loading its page. The region comes
    /// back caught up and settled to the breakpoint, with its next one booked; act on it with
    /// [`PagedTable::apply`] at the returned time. Wakes due at the same tick come in key order.
    pub fn next_due(&mut self, store: &mut PageStore, limit: Tick) -> Result<Option<(Tick, u64)>> {
        loop {
            if let Some(b) = self.due.pop() {
                let at = self.due_at;
                if self.fire(store, at, b)? {
                    return Ok(Some((at, b.key)));
                }
                continue;
            }
            if !self.refill_due(limit) {
                return Ok(None);
            }
        }
    }

    /// Pops every wheel entry due at the earliest due tick up to `limit` into `due`, sorted.
    pub(crate) fn refill_due(&mut self, limit: Tick) -> bool {
        let Some((t, b)) = self.wheel.pop_until(limit) else {
            return false;
        };
        self.due.push(b);
        while let Some((t2, b2)) = self.wheel.pop_until(t) {
            debug_assert_eq!(t2, t);
            self.due.push(b2);
        }
        self.due.sort_unstable_by(|a, b| b.cmp(a));
        self.due.dedup();
        self.due_at = t;
        true
    }

    fn fire(&mut self, store: &mut PageStore, at: Tick, b: Booked) -> Result<bool> {
        let Some(i) = self.ensure(store, self.page_of(b.key))? else {
            self.lazy.stale_wakes += 1;
            return Ok(false);
        };
        let s = self.slot_of(b.key);
        let stream = self.stream(b.key);
        let page = self.slab[i as usize].as_mut().expect("resident page");
        let live = page.slots[s].as_ref().is_some_and(|x| x.wake == Some(b.at));
        if !live {
            self.lazy.stale_wakes += 1;
            return Ok(false);
        }
        let slot = page.slots[s].as_mut().expect("checked above");
        self.lazy.breakpoints += 1;
        self.lazy.slot_commits += slot.cell.catch_up(at, stream);
        slot.cell.settle_to(at);
        page.dirty = true;
        self.sync(i);
        self.reschedule(i, b.key);
        Ok(true)
    }

    // ---- Budget ----

    /// Evicts least-recently-used pages whose state is on disk until the resident bytes fit the
    /// budget. Call at step boundaries. Returns how many pages went.
    pub fn enforce_budget(&mut self) -> usize {
        let mut evicted = 0;
        while self.resident_bytes > self.cfg.budget_bytes && self.tail != NIL {
            self.evict(self.tail);
            evicted += 1;
        }
        if self.resident_bytes > self.cfg.budget_bytes {
            self.stats.over_budget += 1;
        }
        evicted
    }

    // ---- Prefetch ----

    /// Asks for the pages of these regions to be read ahead (player movement, A15 legs).
    pub fn read_ahead(&mut self, keys: impl IntoIterator<Item = u64>) {
        for k in keys {
            let p = self.page_of(k);
            if !self.index.contains_key(&p) {
                self.hints.push(p);
            }
        }
    }

    /// Books a prefetch of `key`'s page ahead of an event the caller scheduled for `at`.
    pub fn prefetch_at(&mut self, key: u64, at: Tick) {
        self.book_prefetch(self.page_of(key), at);
    }

    /// The pages to read ahead now: prefetches booked up to `now` and read-ahead hints, minus
    /// pages already resident or nowhere on disk. Call once per step when prefetch is on.
    pub fn prefetch_due(&mut self, store: &PageStore, now: Tick) -> Vec<Prefetch> {
        while let Some((_, p)) = self.prefetch.pop_until(now) {
            self.hints.push(p);
        }
        self.hints.sort_unstable();
        self.hints.dedup();
        let mut out = Vec::new();
        for p in std::mem::take(&mut self.hints) {
            if self.index.contains_key(&p) {
                continue;
            }
            let key = self.page_key(p);
            if let Some(location) = store.locate(key) {
                out.push(Prefetch {
                    page: p,
                    key,
                    location,
                });
            }
        }
        out
    }

    /// Installs a page read ahead on another thread. Returns false, dropping the bytes, if the
    /// page became resident meanwhile or changed on disk since it was located.
    pub fn install(&mut self, store: &PageStore, p: &Prefetch, raw: &[u8]) -> Result<bool> {
        if self.index.contains_key(&p.page) || store.locate(p.key).as_ref() != Some(&p.location) {
            return Ok(false);
        }
        let mut page = Page::decode(p.page, self.cfg.regions_per_page, raw)?;
        page.prefetched = true;
        self.stats.prefetches += 1;
        self.stats.load_bytes += p.location.stored_len() as u64;
        // At the hot end, like a page just used: it is needed within the lead time, and at the
        // cold end the very next budget check would evict it again.
        self.install_page(page);
        Ok(true)
    }

    /// [`PagedTable::prefetch_due`], the reads and [`PagedTable::install`] inline, on the caller.
    /// Returns how many pages it installed.
    pub fn prefetch(&mut self, store: &mut PageStore, now: Tick) -> Result<usize> {
        let mut n = 0;
        for p in self.prefetch_due(store, now) {
            let raw = p.location.read()?;
            n += usize::from(self.install(store, &p, &raw)?);
        }
        Ok(n)
    }

    // ---- Saves ----

    /// Puts every resident page that changed since it was last written into `batch` (a page with
    /// no region left becomes a delete), compressing on the calling thread. The pages stay pinned
    /// until the ticket comes back.
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

    /// The step-boundary half of a save (PLAN.md §4.9): the raw bytes of every resident page that
    /// changed since it was last written (`None`: no region left, delete it), without
    /// compressing. The host's IO lane compresses them (`SaveBatch::put`, or
    /// `EncodedPage::encode` then `put_encoded`) and commits, off the step thread; the pages stay
    /// pinned until the ticket comes back through [`PagedTable::saved`] or
    /// [`PagedTable::save_failed`].
    #[allow(clippy::type_complexity)]
    pub fn snapshot_dirty(&mut self) -> (Vec<(PageKey, Option<Vec<u8>>)>, SaveTicket) {
        let mut ticket = SaveTicket::default();
        let mut out = Vec::new();
        let table = self.cfg.table;
        for i in 0..self.slab.len() as u32 {
            let Some(page) = self.slab[i as usize].as_mut() else {
                continue;
            };
            if !(page.dirty || page.caught_up) {
                continue;
            }
            let key = PageKey::new(table, page.id);
            if page.present == 0 {
                out.push((key, None));
            } else {
                let mut w = Writer::with_capacity(64 * page.present as usize);
                page.encode(&mut w);
                out.push((key, Some(w.into_bytes())));
            }
            page.dirty = false;
            page.caught_up = false;
            page.saving = true;
            ticket.pages.push(page.id);
            self.sync(i);
        }
        (out, ticket)
    }

    /// Puts the table's own state (the clock and every booked wake) into `batch`. Its size is the
    /// number of pending wakes, not the number of regions.
    pub fn save_meta(&self, batch: &mut SaveBatch) {
        let mut entries: Vec<(Tick, Booked)> = self.wheel.iter().map(|(t, b)| (t, *b)).collect();
        entries.extend(self.due.iter().map(|b| (self.due_at, *b)));
        entries.sort_unstable();
        entries.dedup();
        let mut w = Writer::with_capacity(16 + entries.len() * 8);
        w.var_u64(self.wheel.now().0);
        w.var_u64(entries.len() as u64);
        for (at, b) in entries {
            w.var_u64(at.0);
            w.var_u64(b.key);
            w.var_u64(b.at.0);
        }
        batch.put(PageKey::new(self.cfg.meta_table, 0), w.as_bytes());
    }

    /// The commit holding `ticket`'s pages is durable: they may be evicted again.
    pub fn saved(&mut self, ticket: SaveTicket) {
        for p in ticket.pages {
            if let Some(&i) = self.index.get(&p) {
                self.slab[i as usize]
                    .as_mut()
                    .expect("resident page")
                    .saving = false;
                self.sync(i);
            }
        }
    }

    /// The commit failed: the pages are dirty again.
    pub fn save_failed(&mut self, ticket: SaveTicket) {
        for p in ticket.pages {
            if let Some(&i) = self.index.get(&p) {
                let page = self.slab[i as usize].as_mut().expect("resident page");
                page.saving = false;
                page.dirty = true;
                self.sync(i);
            }
        }
    }

    // ---- Whole-world passes (tests, the bench, tools): they cost the world's size ----

    /// Every page id that exists, resident or on disk, in order.
    pub fn page_ids(&self, store: &PageStore) -> Vec<u64> {
        let mut ids: Vec<u64> = store
            .keys()
            .filter(|k| k.table() == self.cfg.table)
            .map(|k| k.id())
            .collect();
        ids.extend(self.index.keys().copied());
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Calls `f` on every region in key order, page by page, keeping to the budget as it goes.
    pub fn for_each(
        &mut self,
        store: &mut PageStore,
        mut f: impl FnMut(u64, &Lazy<M>, Option<Tick>),
    ) -> Result<()> {
        let per = self.cfg.regions_per_page;
        for pid in self.page_ids(store) {
            if let Some(i) = self.ensure(store, pid)? {
                let page = self.slab[i as usize].as_ref().expect("resident page");
                for (s, slot) in page.slots.iter().enumerate() {
                    if let Some(slot) = slot {
                        f(pid * per + s as u64, &slot.cell, slot.wake);
                    }
                }
            }
            self.enforce_budget();
        }
        Ok(())
    }

    /// Catches every region up to `at` and hashes every page in order: equal for equal worlds,
    /// whatever was resident when, and whatever order things ran in. CRC32 of the page bytes.
    pub fn state_hash(&mut self, store: &mut PageStore, at: Tick) -> Result<u32> {
        let mut h = crc32fast::Hasher::new();
        for pid in self.page_ids(store) {
            if let Some(i) = self.ensure(store, pid)? {
                let base = pid * self.cfg.regions_per_page;
                let streams: Vec<StreamKey> = (0..self.cfg.regions_per_page)
                    .map(|s| self.stream(base + s))
                    .collect();
                let page = self.slab[i as usize].as_mut().expect("resident page");
                let mut commits = 0;
                for (s, slot) in page.slots.iter_mut().enumerate() {
                    if let Some(slot) = slot {
                        commits += slot.cell.catch_up(at, streams[s]);
                    }
                }
                if commits > 0 {
                    page.caught_up = true;
                }
                self.lazy.slot_commits += commits;
                if page.present > 0 {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conserve::Diffuser;
    use crate::page::Durability;
    use crate::synth::{Herd, Stock, SynthRegion};

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rtsim-paging-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn region(i: u64, threshold: i64) -> SynthRegion {
        SynthRegion {
            stock: Stock::new(10, Diffuser::per(40 + (i % 7) as i64, 24_000), threshold),
            herd: Herd::new(i * 31 + 1, [20 + i % 5, 40, 10]),
        }
    }

    fn cfg(budget: usize) -> PagingConfig {
        PagingConfig {
            regions_per_page: 8,
            budget_bytes: budget,
            ..PagingConfig::new(1, 2)
        }
    }

    /// Builds `n` regions, saving and evicting as it goes.
    fn build(
        store: &mut PageStore,
        n: u64,
        budget: usize,
        threshold: impl Fn(u64) -> i64,
    ) -> PagedTable<SynthRegion> {
        let mut t = PagedTable::new(7, SystemId::named("test.region"), Tick::ZERO, cfg(budget));
        for i in 0..n {
            t.insert(store, i, region(i, threshold(i)), Tick::ZERO)
                .unwrap();
            if i % 64 == 63 {
                save(&mut t, store);
            }
        }
        save(&mut t, store);
        t
    }

    fn save(t: &mut PagedTable<SynthRegion>, store: &mut PageStore) {
        let mut b = SaveBatch::new();
        let ticket = t.save_dirty(&mut b);
        t.save_meta(&mut b);
        store.commit(b, 0).unwrap();
        t.saved(ticket);
        t.enforce_budget();
    }

    #[test]
    fn eviction_keeps_to_the_budget_and_reads_reload() {
        let dir = tmpdir("budget");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let page_bytes = Page::<SynthRegion>::empty(0, 8).bytes();
        let budget = page_bytes * 4;
        let mut t = build(&mut store, 400, budget, |_| i64::MAX);
        assert!(
            t.resident_bytes() <= budget,
            "{} > {budget}",
            t.resident_bytes()
        );
        assert!(t.resident_pages() <= 4);
        // Reading every region loads every page; the budget holds at each step boundary.
        let at = Tick::from_days(3);
        while t.next_due(&mut store, at).unwrap().is_some() {}
        for k in 0..400 {
            let v = t.read(&mut store, k, at).unwrap().unwrap();
            assert!(v.grain > 10);
            t.enforce_budget();
            assert!(t.resident_bytes() <= budget);
        }
        assert!(t.stats().loads >= 40);
        // Catch-up-only pages were dropped, not saved.
        assert!(t.stats().dropped_catch_ups > 0);
        assert_eq!(t.read(&mut store, 400, at).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The same world read and changed the same way gives the same state whatever the budget:
    /// eviction, reloads and recomputed catch-ups change nothing.
    #[test]
    fn the_budget_never_changes_the_result() {
        let mut hashes = Vec::new();
        for budget in [usize::MAX, 2_000, 1] {
            let dir = tmpdir(&format!("same-{budget}"));
            let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
            // Every third region has a reachable threshold, so breakpoints fire.
            let mut t = build(&mut store, 200, budget, |i| {
                if i % 3 == 0 { 100 } else { i64::MAX }
            });
            let mut fired = 0;
            for day in 1..=6u64 {
                let now = Tick::from_days(day) + 7;
                while let Some((at, key)) = t.next_due(&mut store, now).unwrap() {
                    fired += 1;
                    // Act on the breakpoint: ship the surplus away.
                    t.apply(&mut store, key, at, |m, _| m.stock.amount -= 300)
                        .unwrap();
                }
                for k in (day * 13..200).step_by(17) {
                    t.read(&mut store, k, now).unwrap();
                    t.apply(&mut store, (k * 7) % 200, now, |m, _| m.stock.amount += 1)
                        .unwrap();
                }
                save(&mut t, &mut store);
            }
            assert!(fired > 0);
            let h = t.state_hash(&mut store, Tick::from_days(7)).unwrap();
            hashes.push((budget, h, t.stats().evictions, fired));
            std::fs::remove_dir_all(&dir).unwrap();
        }
        assert!(hashes[2].2 > 0, "the tiny budget must evict: {hashes:?}");
        assert!(
            hashes
                .iter()
                .all(|h| h.1 == hashes[0].1 && h.3 == hashes[0].3),
            "{hashes:?}"
        );
    }

    /// Wakes of evicted regions still fire, and a table reopened from a save fires the same ones.
    #[test]
    fn wakes_survive_eviction_and_reopen() {
        let dir = tmpdir("wakes");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut t = build(&mut store, 100, 1, |_| 100);
        assert_eq!(t.resident_pages(), 0);
        let mut first = Vec::new();
        let limit = Tick::from_days(4);
        while let Some((at, k)) = t.next_due(&mut store, limit).unwrap() {
            first.push((at, k));
        }
        assert_eq!(first.len(), 100);
        // Same tick, key order.
        assert!(first.windows(2).all(|w| w[0] <= w[1]));

        // Reopen from a save taken at tick 0 and run the same span.
        let dir2 = tmpdir("wakes2");
        let mut store2 = PageStore::open(&dir2, Durability::NoSync).unwrap();
        let t2 = build(&mut store2, 100, 1, |_| 100);
        drop(t2);
        drop(store2);
        let mut store2 = PageStore::open(&dir2, Durability::NoSync).unwrap();
        let mut t2: PagedTable<SynthRegion> =
            PagedTable::open(7, SystemId::named("test.region"), cfg(1), &mut store2).unwrap();
        let mut second = Vec::new();
        while let Some((at, k)) = t2.next_due(&mut store2, limit).unwrap() {
            second.push((at, k));
        }
        assert_eq!(first, second);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&dir2).unwrap();
    }

    #[test]
    fn prefetch_loads_pages_before_their_wakes() {
        let dir = tmpdir("prefetch");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut t = build(&mut store, 64, 1, |i| if i < 8 { 100 } else { i64::MAX });
        assert_eq!(t.resident_pages(), 0);
        let wake = (0..8)
            .map(|i| region(i, 100).stock.breakpoint(Tick::ZERO).unwrap())
            .min()
            .unwrap();
        // Before the lead nothing is due; at the lead the page comes in.
        assert_eq!(t.prefetch(&mut store, Tick(wake.0 - 200)).unwrap(), 0);
        assert_eq!(
            t.prefetch(&mut store, Tick(wake.0 - DEFAULT_PREFETCH_LEAD))
                .unwrap(),
            1
        );
        assert!(t.is_resident(0));
        let loads = t.stats().loads;
        assert!(t.next_due(&mut store, wake).unwrap().is_some());
        assert_eq!(t.stats().loads, loads, "the wake found its page resident");
        assert_eq!(t.stats().prefetch_hits, 1);
        // Read-ahead: hints load the page of the region asked for.
        t.read_ahead([40]);
        assert_eq!(t.prefetch(&mut store, wake).unwrap(), 1);
        assert!(t.is_resident(40));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stale_prefetch_is_dropped() {
        let dir = tmpdir("stale");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut t = build(&mut store, 16, 1, |_| i64::MAX);
        t.read_ahead([3]);
        let reqs = t.prefetch_due(&store, Tick::ZERO);
        assert_eq!(reqs.len(), 1);
        let raw = reqs[0].location.read().unwrap();
        // Meanwhile the page is loaded, changed, saved and evicted.
        t.apply(&mut store, 3, Tick::ZERO, |m, _| m.stock.amount = 999)
            .unwrap();
        save(&mut t, &mut store);
        assert!(!t.is_resident(3));
        assert!(!t.install(&store, &reqs[0], &raw).unwrap());
        assert_eq!(
            t.read(&mut store, 3, Tick::ZERO).unwrap().unwrap().grain,
            999
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dirty_pages_wait_for_their_save() {
        let dir = tmpdir("dirty");
        let mut store = PageStore::open(&dir, Durability::NoSync).unwrap();
        let mut t = build(&mut store, 64, usize::MAX, |_| i64::MAX);
        t.set_budget(1);
        for k in 0..64 {
            t.apply(&mut store, k, Tick::ZERO, |m, _| m.stock.amount += 1)
                .unwrap();
        }
        assert_eq!(t.enforce_budget(), 0);
        assert_eq!(t.stats().over_budget, 1);
        let mut b = SaveBatch::new();
        let ticket = t.save_dirty(&mut b);
        assert_eq!(ticket.pages(), 8);
        // Written but not yet durable: still pinned.
        assert_eq!(t.enforce_budget(), 0);
        store.commit(b, 1).unwrap();
        t.saved(ticket);
        assert_eq!(t.enforce_budget(), 8);
        assert_eq!(
            t.read(&mut store, 5, Tick::ZERO).unwrap().unwrap().grain,
            11
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn helpers() {
        assert_eq!(morton2(0, 0), 0);
        assert_eq!(morton2(1, 0), 1);
        assert_eq!(morton2(0, 1), 2);
        assert_eq!(morton2(3, 3), 15);
        // A 2x2 block shares a page of 4.
        let keys: Vec<u64> = [(2, 2), (3, 2), (2, 3), (3, 3)]
            .iter()
            .map(|&(x, z)| morton2(x, z) / 4)
            .collect();
        assert!(keys.iter().all(|&k| k == keys[0]));
        let cells = read_ahead_grid((5, 5), (1, 0), 2, 1, 7);
        // Centres x = 6 and 7, widened by 1: x 5..=8 minus the off-grid 7 and 8; z 4..=6.
        assert_eq!(cells.len(), 6);
        assert!(cells.iter().all(|&(x, _)| x == 5 || x == 6));
    }
}
