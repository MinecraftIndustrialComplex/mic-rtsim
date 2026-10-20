// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lazy regions (SCALE.md §4, A15): a region nobody reads is not stepped. It records when it was
//! last evaluated and catches up when it is read, when an event reaches it, or at a breakpoint.
//!
//! # The rule that makes reads harmless
//!
//! The stored state is only ever re-anchored at moments that do not depend on who reads when:
//! - **slot boundaries**, fixed multiples of the model's [`SlotPeriod`] counted from tick 0, where
//!   random processes commit, each slot with its own counter-RNG stream keyed by (world seed,
//!   system, region, slot);
//! - **event times** ([`Lazy::apply`]): an event is an input, so the trajectory may depend on it;
//! - **breakpoints** the table schedules from the state itself.
//!
//! A read in the middle of a slot commits the slots that ended before it and then *projects* the
//! state to the read time with [`LazyModel::view`], changing nothing. That is why
//! `catch_up(t0 → t2) == catch_up(t1 → t2) ∘ catch_up(t0 → t1)` holds bit for bit, floats included:
//! both sides run the same settles and commits with the same arguments. The property tests in
//! `tests/lazy_composition.rs` check it.
//!
//! # What catch-up costs
//!
//! A model with no slot period (deterministic between events: closed-form growth, piecewise-linear
//! production) costs nothing to catch up; reads are O(1) projections. A model with a slot period
//! costs one settle and one commit per elapsed slot: "catch-up over k slots costs k cheap draws"
//! (SCALE.md §4). That is linear in elapsed time with a small constant, which SCALE.md §10's "never
//! depends on elapsed time" does not allow for; DECISIONS.md overnight choice O5 records the
//! conflict and the scale bench measures it.
//!
//! # Breakpoints and reads
//!
//! Reads never reschedule a breakpoint. If they did, a read would move the next wake, and the
//! trajectory would depend on who looked when. Breakpoints are recomputed only after an event, a
//! breakpoint, or insertion, all of which happen at times that do not depend on reads.
//!
//! # Order within a step
//!
//! A [`LazyTable`] caller must drain [`LazyTable::next_due`] up to `t` before it reads a region or
//! applies an event at `t`. Otherwise an event can supersede a breakpoint that was due first (its
//! crossing is never delivered), or a read can commit slots past a breakpoint that then fires
//! against the later state. Debug builds assert the order.
//!
//! # Wakes are state
//!
//! The time a region's next wake was booked for is saved with the region and restored as is
//! ([`LazyTable::restore`]), never recomputed on load: a random model's breakpoint estimate depends
//! on its anchor, and reads move anchors, so a recomputed wake would depend on who read before the
//! save.

use crate::clock::{SlotPeriod, Tick};
use crate::codec::{self, CodecError, Reader, Writer};
use crate::handle::{Arena, Handle};
use crate::ladder::{Answers, Query};
use crate::rng::{SlotRng, StreamKey, SystemId};
use crate::wheel::TimingWheel;

/// A region's local processes, in the form lazy evaluation needs.
pub trait LazyModel {
    /// What a read sees.
    type View;

    /// The period random commits happen on, aligned to tick 0, or `None` for a model that is
    /// deterministic between events. Must not change over an instance's life.
    fn slot_period(&self) -> Option<SlotPeriod>;

    /// Folds deterministic progress over `[from, to)` into the state. Called only with anchors as
    /// `from` and slot boundaries, event times or breakpoints as `to`; never with a read time.
    fn settle(&mut self, from: Tick, to: Tick);

    /// The random commit at the end of `slot`. Draw only from `rng`: it is the stream for this
    /// region and slot, the same one an eager simulation would use.
    fn commit_slot(&mut self, slot: u64, rng: &mut SlotRng);

    /// The state as seen at `at >= anchor`, without changing it.
    fn view(&self, anchor: Tick, at: Tick) -> Self::View;

    /// When the outputs next change in kind (a stock runs out, a threshold is crossed), strictly
    /// after `anchor`; `None` if nothing is coming. Random models may return an estimate: a wake
    /// that finds nothing to do costs one catch-up.
    fn next_breakpoint(&self, _anchor: Tick) -> Option<Tick> {
        None
    }
}

/// A model plus the moment its state describes.
#[derive(Clone, Debug, PartialEq)]
pub struct Lazy<M> {
    model: M,
    anchor: Tick,
}

impl<M: LazyModel> Lazy<M> {
    pub fn new(model: M, at: Tick) -> Self {
        Self { model, anchor: at }
    }

    /// When the stored state was last evaluated: the last slot boundary, event or breakpoint it
    /// was settled to.
    pub fn anchor(&self) -> Tick {
        self.anchor
    }

    /// The stored state as of [`Lazy::anchor`]. For saving and inspection; systems read through
    /// [`Lazy::read`].
    pub fn model(&self) -> &M {
        &self.model
    }

    pub fn into_parts(self) -> (M, Tick) {
        (self.model, self.anchor)
    }

    /// The stored state, mutably, for the paged table's event path (which settles first).
    pub(crate) fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    /// Commits every slot that ends at or before `to`; returns how many. Never moves the anchor
    /// to `to` itself unless `to` is a boundary.
    pub fn catch_up(&mut self, to: Tick, key: StreamKey) -> u64 {
        let Some(period) = self.model.slot_period() else {
            return 0;
        };
        let mut commits = 0;
        loop {
            let slot = period.slot_of(self.anchor);
            let end = period.end(slot);
            if end > to {
                return commits;
            }
            self.model.settle(self.anchor, end);
            let mut rng = key.slot(slot);
            self.model.commit_slot(slot, &mut rng);
            self.anchor = end;
            commits += 1;
        }
    }

    /// Catches up, then projects to `at`. Reading before the anchor is a caller bug (debug builds
    /// assert); release builds project to the anchor instead of underflowing.
    pub fn read(&mut self, at: Tick, key: StreamKey) -> M::View {
        debug_assert!(
            at >= self.anchor,
            "read at {at:?} before the anchor {:?}",
            self.anchor
        );
        self.catch_up(at, key);
        self.model.view(self.anchor, at.max(self.anchor))
    }

    /// The view at `at` from the stored state alone. Only valid while no slot boundary lies
    /// between the anchor and `at`; use it for "what does the page say" checks.
    pub fn view_without_catch_up(&self, at: Tick) -> M::View {
        self.model.view(self.anchor, at.max(self.anchor))
    }

    /// Delivers an event at `at`: catches up, settles to `at` (re-anchoring there), then lets `f`
    /// change the state.
    pub fn apply<R>(&mut self, at: Tick, key: StreamKey, f: impl FnOnce(&mut M, Tick) -> R) -> R {
        self.catch_up(at, key);
        self.settle_to(at);
        f(&mut self.model, at)
    }

    /// Settles the stored state to `at` and re-anchors there. Only for event and breakpoint
    /// times (module docs); `at` must not be before the anchor.
    pub(crate) fn settle_to(&mut self, at: Tick) {
        debug_assert!(
            at >= self.anchor,
            "event at {at:?} before the anchor {:?}",
            self.anchor
        );
        if at > self.anchor {
            self.model.settle(self.anchor, at);
            self.anchor = at;
        }
    }
}

/// One row of a [`LazyTable`].
pub struct Region<M> {
    cell: Lazy<M>,
    /// The stable key of this region (packed coordinates, say). Keys its RNG streams, so it must
    /// survive save and load unchanged; arena handles need not.
    key: u64,
    /// Bumped whenever the breakpoint is rescheduled; a wake with an older stamp is stale.
    stamp: u32,
    /// When the live wake was booked for: saved with the region (see the module docs).
    wake: Option<Tick>,
    /// Last read or event, for eviction by recency (SCALE.md §8).
    touched: Tick,
}

impl<M> Region<M> {
    pub fn key(&self) -> u64 {
        self.key
    }

    pub fn touched(&self) -> Tick {
        self.touched
    }

    pub fn cell(&self) -> &Lazy<M> {
        &self.cell
    }

    /// The booked wake, to save with the region.
    pub fn wake(&self) -> Option<Tick> {
        self.wake
    }
}

/// A scheduled wake: the region and the stamp it had when booked.
struct Wake<M> {
    region: Handle<Region<M>>,
    stamp: u32,
}

/// Counters for the profiler and the scale bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LazyStats {
    pub reads: u64,
    pub events: u64,
    pub slot_commits: u64,
    pub breakpoints: u64,
    pub stale_wakes: u64,
}

/// Many lazy regions of one model, with their breakpoints on a timing wheel.
///
/// Cost per call is the catch-up of the one region touched plus O(1) wheel work. Nothing here
/// iterates over all regions; [`LazyTable::iter`] exists for saving.
pub struct LazyTable<M: LazyModel> {
    rows: Arena<Region<M>>,
    wheel: TimingWheel<Wake<M>>,
    world_seed: u64,
    system: SystemId,
    stats: LazyStats,
}

pub type RegionId<M> = Handle<Region<M>>;

impl<M: LazyModel> LazyTable<M> {
    pub fn new(world_seed: u64, system: SystemId, now: Tick) -> Self {
        Self {
            rows: Arena::new(),
            wheel: TimingWheel::new(now),
            world_seed,
            system,
            stats: LazyStats::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn stats(&self) -> LazyStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = LazyStats::default();
    }

    /// Pending wakes, stale ones included.
    pub fn pending_wakes(&self) -> usize {
        self.wheel.len()
    }

    fn stream(&self, key: u64) -> StreamKey {
        StreamKey::new(self.world_seed, self.system, key)
    }

    pub fn insert(&mut self, key: u64, model: M, at: Tick) -> RegionId<M> {
        let h = self.rows.insert(Region {
            cell: Lazy::new(model, at),
            key,
            stamp: 0,
            wake: None,
            touched: at,
        });
        self.reschedule(h);
        h
    }

    /// Re-inserts a cell loaded from a page with the wake it had booked ([`Region::wake`]), as is.
    /// A wake already past fires at once, late.
    pub fn restore(&mut self, key: u64, cell: Lazy<M>, wake: Option<Tick>) -> RegionId<M> {
        let touched = cell.anchor();
        let h = self.rows.insert(Region {
            cell,
            key,
            stamp: 0,
            wake,
            touched,
        });
        if let Some(at) = wake {
            let at = at.max(self.wheel.now());
            self.wheel.schedule(
                at,
                Wake {
                    region: h,
                    stamp: 0,
                },
            );
        }
        h
    }

    pub fn remove(&mut self, h: RegionId<M>) -> Option<(u64, Lazy<M>)> {
        self.rows.remove(h).map(|r| (r.key, r.cell))
    }

    pub fn get(&self, h: RegionId<M>) -> Option<&Region<M>> {
        self.rows.get(h)
    }

    pub fn iter(&self) -> impl Iterator<Item = (RegionId<M>, &Region<M>)> {
        self.rows.iter()
    }

    fn reschedule(&mut self, h: RegionId<M>) {
        let Some(row) = self.rows.get_mut(h) else {
            return;
        };
        row.stamp = row.stamp.wrapping_add(1);
        let anchor = row.cell.anchor();
        row.wake = row.cell.model().next_breakpoint(anchor);
        if let Some(at) = row.wake {
            debug_assert!(at > anchor, "breakpoint {at:?} not after anchor {anchor:?}");
            let stamp = row.stamp;
            self.wheel
                .schedule(at.max(self.wheel.now()), Wake { region: h, stamp });
        }
    }

    /// A read: catch up, then project. `None` if the handle is stale. Drain
    /// [`LazyTable::next_due`] up to `at` first (module docs).
    pub fn read(&mut self, h: RegionId<M>, at: Tick) -> Option<M::View> {
        debug_assert!(
            at <= self.wheel.now(),
            "read at {at:?} before next_due was drained to it (wheel at {:?})",
            self.wheel.now()
        );
        let key = self.stream(self.rows.get(h)?.key);
        let row = self.rows.get_mut(h)?;
        row.touched = row.touched.max(at);
        self.stats.reads += 1;
        self.stats.slot_commits += row.cell.catch_up(at, key);
        Some(
            row.cell
                .model
                .view(row.cell.anchor, at.max(row.cell.anchor)),
        )
    }

    /// What systems call: a question answered from the region's view, whatever level holds the
    /// data (SCALE.md §2, "systems only ever call queries").
    pub fn query<Q: Query>(&mut self, h: RegionId<M>, at: Tick, q: &Q) -> Option<Q::Answer>
    where
        M::View: Answers<Q>,
    {
        self.read(h, at).map(|v| v.answer(q))
    }

    /// An event at `at`: catch up, settle to `at`, apply `f`, and reschedule the breakpoint. Drain
    /// [`LazyTable::next_due`] up to `at` first (module docs).
    pub fn apply<R>(
        &mut self,
        h: RegionId<M>,
        at: Tick,
        f: impl FnOnce(&mut M, Tick) -> R,
    ) -> Option<R> {
        debug_assert!(
            at <= self.wheel.now(),
            "event at {at:?} before next_due was drained to it (wheel at {:?})",
            self.wheel.now()
        );
        let key = self.stream(self.rows.get(h)?.key);
        let row = self.rows.get_mut(h)?;
        row.touched = row.touched.max(at);
        self.stats.events += 1;
        self.stats.slot_commits += row.cell.catch_up(at, key);
        row.cell.settle_to(at);
        let r = f(&mut row.cell.model, at);
        self.reschedule(h);
        Some(r)
    }

    /// Pops the next live breakpoint due at or before `limit`. The region comes back caught up
    /// and settled to the breakpoint, with its next breakpoint already booked; the caller acts on
    /// it with [`LazyTable::apply`] at the returned time. Stale wakes are skipped and counted.
    pub fn next_due(&mut self, limit: Tick) -> Option<(Tick, RegionId<M>)> {
        while let Some((at, wake)) = self.wheel.pop_until(limit) {
            let Some(row) = self.rows.get(wake.region) else {
                self.stats.stale_wakes += 1;
                continue;
            };
            if row.stamp != wake.stamp {
                self.stats.stale_wakes += 1;
                continue;
            }
            let key = self.stream(row.key);
            let row = self.rows.get_mut(wake.region).expect("checked above");
            self.stats.breakpoints += 1;
            self.stats.slot_commits += row.cell.catch_up(at, key);
            row.cell.settle_to(at);
            self.reschedule(wake.region);
            return Some((at, wake.region));
        }
        None
    }

    /// The latest `limit` [`LazyTable::next_due`] has run to with nothing left due.
    pub fn now(&self) -> Tick {
        self.wheel.now()
    }

    /// Commits every region up to `at`. The eager path: tests use it to show lazy and eager agree;
    /// it is never part of a step, because its cost is the region count.
    pub fn catch_up_all(&mut self, at: Tick) {
        let (seed, system) = (self.world_seed, self.system);
        for (_, row) in self.rows.iter_mut() {
            self.stats.slot_commits += row.cell.catch_up(at, StreamKey::new(seed, system, row.key));
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.rows.heap_bytes() + self.wheel.heap_bytes()
    }

    /// Writes the whole table: every region (key, stamp, booked wake, touched, anchor and the
    /// model through `f`), the arena's free slots, and the wake wheel in pop order, stale wakes
    /// included. [`LazyTable::decode_with`] gives back a table that continues exactly as this one
    /// would: the same handles, the same wakes in the same order (wakes are state: module docs).
    pub fn encode_with(&self, w: &mut Writer, mut f: impl FnMut(&M, &mut Writer)) {
        w.u64(self.world_seed);
        w.u64(self.system.0);
        self.rows.encode_with(w, |row, w| {
            w.u64(row.key);
            w.u32(row.stamp);
            match row.wake {
                Some(t) => {
                    w.u8(1);
                    w.u64(t.0);
                }
                None => w.u8(0),
            }
            w.u64(row.touched.0);
            w.u64(row.cell.anchor.0);
            f(&row.cell.model, w);
        });
        self.wheel.encode_with(w, |wake, w| {
            w.u64(wake.region.to_bits());
            w.u32(wake.stamp);
        });
        let s = &self.stats;
        for v in [
            s.reads,
            s.events,
            s.slot_commits,
            s.breakpoints,
            s.stale_wakes,
        ] {
            w.var_u64(v);
        }
    }

    pub fn decode_with(
        r: &mut Reader<'_>,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<M>,
    ) -> codec::Result<Self> {
        let world_seed = r.u64()?;
        let system = SystemId(r.u64()?);
        let rows = Arena::decode_with(r, |r| {
            let key = r.u64()?;
            let stamp = r.u32()?;
            let wake = match r.u8()? {
                0 => None,
                1 => Some(Tick(r.u64()?)),
                t => return Err(CodecError::Invalid(format!("wake tag {t}"))),
            };
            let touched = Tick(r.u64()?);
            let anchor = Tick(r.u64()?);
            let model = f(r)?;
            Ok(Region {
                cell: Lazy { model, anchor },
                key,
                stamp,
                wake,
                touched,
            })
        })?;
        let wheel = TimingWheel::decode_with(r, |r| {
            Ok(Wake {
                region: Handle::from_bits(r.u64()?),
                stamp: r.u32()?,
            })
        })?;
        let stats = LazyStats {
            reads: r.var_u64()?,
            events: r.var_u64()?,
            slot_commits: r.var_u64()?,
            breakpoints: r.var_u64()?,
            stale_wakes: r.var_u64()?,
        };
        Ok(LazyTable {
            rows,
            wheel,
            world_seed,
            system,
            stats,
        })
    }
}
