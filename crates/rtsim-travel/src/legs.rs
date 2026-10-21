// SPDX-License-Identifier: AGPL-3.0-or-later
//! The region-and-time leg index (A15, PLAN.md §4.3 with the 2026-10-19 review fixes).
//!
//! Travellers are not ticked: a leg has a start, an end and a straight path, so position is a
//! formula. Every leg is rasterized at booking into **one shared index** of (frame, region,
//! time window) cells, widened by the detection radius of whatever travels it; dragons, caravans
//! and player interest areas all book here. A player entering a region looks up the legs booked
//! there, so crossings become scheduled events with no scans.
//!
//! **Generations.** Every booking carries its leg's generation. A replan bumps the generation, so
//! the old bookings go stale without being found and removed (cancelling is lazy, as the kernel's
//! wheel cancels). A [`Meeting`] carries both legs' generations, so it cancels in cascade when
//! either leg changes: first the leg's own bookings, then every meeting that depended on them.
//!
//! **Purges** (docs/choices-leg-purge.md LP1 to LP4): a replan leaves its old bookings behind, and
//! nothing reads a window again once it is past, so stale bookings in past windows would stay
//! forever. [`LegIndex::purge_due`], called by the index's owner as its clock moves (rtsim-world's
//! `World::run_until`), drops them once a game day ([`PURGE_EVERY`]). It drops only stale bookings:
//! every read keeps only live ones, so a purged index answers every read exactly as an unpurged
//! one, whenever the purge runs, and strides of any length still agree.
//!
//! **Seams** (docs/choices-topology.md TP7 to TP9): on a wrapped or glued frame a leg starts at a
//! canonical position and its end may lie past an edge (the image of its destination nearest the
//! start); its position is the straight line's point folded ([`LegIndex::position`]). A booking
//! folds each window's box into canonical regions, so a leg crossing a seam is booked on both
//! sides, and a meeting is found through the seams ([`Topology::first_within`]). On a flat frame
//! every number is what it was before.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_space::topology::{Move, Rect, SharedTopologies, Topology};

/// Region edge in blocks, as Minecraft's region files (PLAN.md §5.2).
pub const REGION_BLOCKS: i64 = 512;

/// Game ticks between purges of the past windows' stale bookings ([`LegIndex::purge_due`]).
/// PLACEHOLDER (choice LP2): one game day.
pub const PURGE_EVERY: u64 = 24_000;

/// A straight walk from `(x0, z0)` at `t0` to `(x1, z1)` at `t1` in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub frame: u16,
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub t0: Tick,
    pub t1: Tick,
    /// Detection radius in blocks: bookings are widened by it.
    pub radius: u32,
}

impl Path {
    /// Position at `t`, clamped to the leg's span, in blocks (rounded toward zero).
    pub fn at(&self, t: Tick) -> (i64, i64) {
        let (t0, t1) = (self.t0.0, self.t1.0);
        if t1 <= t0 || t.0 <= t0 {
            return (i64::from(self.x0), i64::from(self.z0));
        }
        if t.0 >= t1 {
            return (i64::from(self.x1), i64::from(self.z1));
        }
        let f = i128::from(t.0 - t0);
        let span = i128::from(t1 - t0);
        let lerp =
            |a: i32, b: i32| (i128::from(a) + (i128::from(b) - i128::from(a)) * f / span) as i64;
        (lerp(self.x0, self.x1), lerp(self.z0, self.z1))
    }
}

/// Who owns a leg, opaque to the index (a caravan row, a dragon, a player's interest area).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(pub u64);

pub struct LegRow {
    pub path: Path,
    pub owner: Owner,
    pub generation: u32,
}

pub type LegId = Handle<LegRow>;

/// A stamped reference to a leg: stale once the leg is replanned or removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stamp {
    pub leg: LegId,
    pub generation: u32,
}

/// Two legs come within both radii of each other at `at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Meeting {
    pub at: Tick,
    pub a: Stamp,
    pub b: Stamp,
}

/// Index cell: (time window, frame, region x, region z). Window first, so old windows purge as a
/// prefix.
type Cell = (u64, u16, i64, i64);

pub struct LegIndex {
    pub(crate) legs: Arena<LegRow>,
    pub(crate) cells: BTreeMap<Cell, Vec<Stamp>>,
    /// Ticks per time window. PLACEHOLDER (overnight choice E13): one in-game hour, the accrual
    /// quantum.
    pub(crate) window: u64,
    pub(crate) stats: LegStats,
    /// Every frame's topology (TP6): session state, set by the engine host; not saved.
    pub(crate) topo: SharedTopologies,
    /// Ticks between purges, 0 for none ([`LegIndex::set_purge_every`]): session state, not saved.
    pub(crate) purge_every: u64,
    /// The last purge boundary passed ([`LegIndex::purge_due`]): session state, not saved, so the
    /// first purge after a load runs at the first boundary the owner's clock passes.
    pub(crate) purged_through: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LegStats {
    pub booked_cells: u64,
    /// Stale bookings dropped, by the reads that met them and by the purges.
    pub stale_dropped: u64,
}

impl LegIndex {
    pub fn new(window_ticks: u64) -> Self {
        LegIndex {
            legs: Arena::new(),
            cells: BTreeMap::new(),
            window: window_ticks.max(1),
            stats: LegStats::default(),
            topo: rtsim_space::topology::all_flat(),
            purge_every: PURGE_EVERY,
            purged_through: 0,
        }
    }

    /// Hands the index every frame's topology. When it changes, every live leg is booked again
    /// under it (a world seen flat before and glued now).
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) || *t == *self.topo {
            self.topo = t;
            return;
        }
        self.topo = t;
        self.cells.clear();
        let live: Vec<(Path, Stamp)> = self
            .legs
            .iter()
            .map(|(id, r)| {
                (
                    r.path,
                    Stamp {
                        leg: id,
                        generation: r.generation,
                    },
                )
            })
            .collect();
        for (p, s) in live {
            self.rasterize(p, s);
        }
    }

    /// The frames' topologies, for modules that book here.
    pub fn topologies(&self) -> &SharedTopologies {
        &self.topo
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.topo.of(frame)
    }

    /// A leg's canonical position at `t`: its chart position folded (TP7).
    pub fn position(&self, p: &Path, t: Tick) -> (i64, i64) {
        let (x, z) = p.at(t);
        self.topo(p.frame).canonical(x, z)
    }

    pub fn stats(&self) -> LegStats {
        self.stats
    }

    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }

    pub fn get(&self, id: LegId) -> Option<&LegRow> {
        self.legs.get(id)
    }

    /// Every live leg, in handle order (tests and diagnostics: the index is never scanned on a
    /// hot path).
    pub fn iter(&self) -> impl Iterator<Item = (LegId, &LegRow)> + '_ {
        self.legs.iter()
    }

    pub fn is_live(&self, s: Stamp) -> bool {
        self.legs
            .get(s.leg)
            .is_some_and(|r| r.generation == s.generation)
    }

    pub fn is_meeting_live(&self, m: &Meeting) -> bool {
        self.is_live(m.a) && self.is_live(m.b)
    }

    pub fn stamp(&self, id: LegId) -> Option<Stamp> {
        self.legs.get(id).map(|r| Stamp {
            leg: id,
            generation: r.generation,
        })
    }

    /// Books a new leg into every cell it passes.
    pub fn book(&mut self, path: Path, owner: Owner) -> Stamp {
        let leg = self.legs.insert(LegRow {
            path,
            owner,
            generation: 1,
        });
        let s = Stamp { leg, generation: 1 };
        self.rasterize(path, s);
        s
    }

    /// Replaces a leg's path. Its old bookings and every meeting stamped with its old generation
    /// go stale (cascade cancellation).
    pub fn replan(&mut self, id: LegId, path: Path) -> Option<Stamp> {
        let row = self.legs.get_mut(id)?;
        row.generation = row.generation.wrapping_add(1).max(1);
        row.path = path;
        let s = Stamp {
            leg: id,
            generation: row.generation,
        };
        self.rasterize(path, s);
        Some(s)
    }

    /// Removes a leg; its bookings and meetings go stale.
    pub fn cancel(&mut self, id: LegId) -> Option<LegRow> {
        self.legs.remove(id)
    }

    fn rasterize(&mut self, p: Path, s: Stamp) {
        for cell in self.cells_of(p) {
            self.cells.entry(cell).or_default().push(s);
            self.stats.booked_cells += 1;
        }
    }

    /// The index cells a path covers: per time window, the regions of the part of the leg inside
    /// it, as a bounding box widened by the radius.
    fn cells_of(&self, p: Path) -> Vec<Cell> {
        let (t0, t1) = (p.t0.0, p.t1.0.max(p.t0.0));
        let (w0, w1) = (t0 / self.window, t1 / self.window);
        let r = i64::from(p.radius);
        let topo = self.topo(p.frame);
        let mut out = Vec::new();
        for w in w0..=w1 {
            let a = p.at(Tick((w * self.window).max(t0)));
            let b = p.at(Tick(((w + 1) * self.window).min(t1)));
            if topo.is_flat() {
                let (xl, xh) = (a.0.min(b.0) - r, a.0.max(b.0) + r);
                let (zl, zh) = (a.1.min(b.1) - r, a.1.max(b.1) + r);
                for rx in xl.div_euclid(REGION_BLOCKS)..=xh.div_euclid(REGION_BLOCKS) {
                    for rz in zl.div_euclid(REGION_BLOCKS)..=zh.div_euclid(REGION_BLOCKS) {
                        out.push((w, p.frame, rx, rz));
                    }
                }
                continue;
            }
            // The window's box in the leg's chart, folded onto canonical regions (TP8).
            let start = out.len();
            for part in topo.fold_rect(Rect::spanning(a, b, r)) {
                let (x0, z0, x1, z1) = part.grid_cells(REGION_BLOCKS);
                for rx in x0..=x1 {
                    for rz in z0..=z1 {
                        out.push((w, p.frame, rx, rz));
                    }
                }
            }
            out[start..].sort_unstable();
            let mut k = start;
            for i in start..out.len() {
                if i == start || out[i] != out[k - 1] {
                    out[k] = out[i];
                    k += 1;
                }
            }
            out.truncate(k);
        }
        out
    }

    /// Ticks per time window.
    pub fn window_ticks(&self) -> u64 {
        self.window
    }

    /// The live legs other than `s` booked in at least one cell `s` covers: the only legs a
    /// meeting with `s` can involve (A15). Each once, in stamp order. Cost: the cells `s` covers
    /// and what is booked in them, never a scan of the index. Stale bookings found on the way are
    /// dropped. Empty when `s` itself is stale.
    pub fn sharing(&mut self, s: Stamp) -> Vec<Stamp> {
        let Some(row) = self.legs.get(s.leg) else {
            return Vec::new();
        };
        if row.generation != s.generation {
            return Vec::new();
        }
        let cells = self.cells_of(row.path);
        let mut out = Vec::new();
        for cell in cells {
            let Some(v) = self.cells.get_mut(&cell) else {
                continue;
            };
            let before = v.len();
            let legs = &self.legs;
            v.retain(|b| {
                legs.get(b.leg)
                    .is_some_and(|r| r.generation == b.generation)
            });
            self.stats.stale_dropped += (before - v.len()) as u64;
            out.extend(v.iter().copied().filter(|b| b.leg != s.leg));
        }
        out.sort();
        out.dedup();
        out
    }

    /// Live legs booked in a region during the window containing `at`. Stale bookings found on
    /// the way are dropped.
    pub fn in_region(&mut self, frame: u16, rx: i64, rz: i64, at: Tick) -> Vec<Stamp> {
        let cell = (at.0 / self.window, frame, rx, rz);
        let Some(v) = self.cells.get_mut(&cell) else {
            return Vec::new();
        };
        let before = v.len();
        let legs = &self.legs;
        v.retain(|s| {
            legs.get(s.leg)
                .is_some_and(|r| r.generation == s.generation)
        });
        self.stats.stale_dropped += (before - v.len()) as u64;
        let mut out = v.clone();
        out.sort();
        out.dedup();
        out
    }

    /// The region a block position is in.
    pub fn region_of(x: i64, z: i64) -> (i64, i64) {
        (x.div_euclid(REGION_BLOCKS), z.div_euclid(REGION_BLOCKS))
    }

    /// When two legs first come within the sum of their radii, if they do: closest approach of
    /// two straight walks over the time they share, in closed form.
    pub fn meeting(&self, a: Stamp, b: Stamp) -> Option<Meeting> {
        let (ra, rb) = (self.legs.get(a.leg)?, self.legs.get(b.leg)?);
        if ra.generation != a.generation || rb.generation != b.generation {
            return None;
        }
        let (pa, pb) = (ra.path, rb.path);
        if pa.frame != pb.frame {
            return None;
        }
        let lo = pa.t0.0.max(pb.t0.0);
        let hi = pa.t1.0.min(pb.t1.0);
        if hi < lo {
            return None;
        }
        let reach = f64::from(pa.radius) + f64::from(pb.radius);
        let topo = self.topo(pa.frame);
        if !topo.is_flat() {
            // Through the seams (TP9): both walks cut at seams, compared under the images that
            // touch the domain.
            let mv = |p: &Path| {
                Move::of_cells(
                    p.t0.0 as f64,
                    p.t1.0.max(p.t0.0) as f64,
                    (i64::from(p.x0), i64::from(p.z0)),
                    (i64::from(p.x1), i64::from(p.z1)),
                )
            };
            let t = topo.first_within(&mv(&pa), &mv(&pb), reach)?;
            let at = if t <= lo as f64 { lo } else { t.ceil() as u64 };
            return Some(Meeting {
                at: Tick(at.clamp(lo, hi)),
                a,
                b,
            });
        }
        // Relative position as a linear function of t over [lo, hi].
        let rel = |t: u64| {
            let (x, z) = pa.at(Tick(t));
            let (u, v) = pb.at(Tick(t));
            ((x - u) as f64, (z - v) as f64)
        };
        let (p0, p1) = (rel(lo), rel(hi));
        let span = (hi - lo) as f64;
        let d2 = |f: f64| {
            let x = p0.0 + (p1.0 - p0.0) * f;
            let z = p0.1 + (p1.1 - p0.1) * f;
            x * x + z * z
        };
        if d2(0.0) <= reach * reach {
            return Some(Meeting { at: Tick(lo), a, b });
        }
        // First f in [0, 1] with |p0 + (p1 - p0) f| = reach: the smaller root of a quadratic.
        let (dx, dz) = (p1.0 - p0.0, p1.1 - p0.1);
        let qa = dx * dx + dz * dz;
        if qa == 0.0 {
            return None;
        }
        let qb = 2.0 * (p0.0 * dx + p0.1 * dz);
        let qc = p0.0 * p0.0 + p0.1 * p0.1 - reach * reach;
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            return None;
        }
        let f = (-qb - disc.sqrt()) / (2.0 * qa);
        if !(0.0..=1.0).contains(&f) {
            return None;
        }
        Some(Meeting {
            at: Tick(lo + (f * span).ceil() as u64),
            a,
            b,
        })
    }

    /// Drops the stale bookings (of legs replanned or removed since) in every window that ended at
    /// or before `before`, and every cell there left empty (by this purge or by the reads, which
    /// drop stale bookings but keep the cell); returns how many bookings it dropped.
    ///
    /// Exact (choice LP1): every read keeps only live bookings ([`LegIndex::sharing`],
    /// [`LegIndex::in_region`]), so no read can tell a purged index from an unpurged one, and a
    /// purge may run at any time. Live bookings stay, however old: a leg booked with a start in
    /// the past (a caravan or a herd booked when a player comes near, a module catching up after
    /// a switch-off) still finds every live leg it crosses there. So the past windows hold at most
    /// the live legs' own bookings. Only [`LegStats::stale_dropped`] and the save's size show it.
    ///
    /// Cost: the cells of the windows purged, live bookings kept from earlier purges included.
    pub fn purge_stale(&mut self, before: Tick) -> u64 {
        let end = (before.0 / self.window, 0, i64::MIN, i64::MIN);
        let legs = &self.legs;
        let mut dropped = 0u64;
        let mut empty = Vec::new();
        for (cell, v) in self.cells.range_mut(..end) {
            let n = v.len();
            v.retain(|s| {
                legs.get(s.leg)
                    .is_some_and(|r| r.generation == s.generation)
            });
            dropped += (n - v.len()) as u64;
            if v.is_empty() {
                empty.push(*cell);
            } else if v.len() != n {
                v.shrink_to_fit();
            }
        }
        for cell in &empty {
            self.cells.remove(cell);
        }
        self.stats.stale_dropped += dropped;
        dropped
    }

    /// The owner's clock reached `now`: purges the past windows ([`LegIndex::purge_stale`]) when
    /// `now` passed a purge boundary (a multiple of [`PURGE_EVERY`] game ticks) since the last
    /// one. Boundaries of game time, not of the caller's steps, so the cost is the same at any
    /// stride (choice LP2). Cheap when no boundary passed: one division.
    pub fn purge_due(&mut self, now: Tick) {
        if self.purge_every == 0 {
            return;
        }
        let b = now.0 / self.purge_every * self.purge_every;
        if b > self.purged_through {
            self.purged_through = b;
            self.purge_stale(Tick(b));
        }
    }

    /// Ticks between the purges [`LegIndex::purge_due`] runs, 0 for none (tests compare a purged
    /// index with an unpurged one). Session state, not saved.
    pub fn set_purge_every(&mut self, ticks: u64) {
        self.purge_every = ticks;
    }

    /// Bookings in the index, live or stale, and how many cells hold them (tests and
    /// diagnostics: a pass over the whole index).
    pub fn booking_counts(&self) -> (usize, usize) {
        (self.cells.values().map(Vec::len).sum(), self.cells.len())
    }

    /// Every live booking, in cell order and then stamp order, as (window, frame, region x,
    /// region z, stamp): what any read can see (tests: a purged index against an unpurged one).
    pub fn live_bookings(&self) -> Vec<(u64, u16, i64, i64, Stamp)> {
        let mut out = Vec::new();
        for (&(w, f, x, z), v) in &self.cells {
            let mut live: Vec<Stamp> = v.iter().copied().filter(|s| self.is_live(*s)).collect();
            live.sort();
            live.dedup();
            out.extend(live.into_iter().map(|s| (w, f, x, z, s)));
        }
        out
    }

    pub fn heap_bytes(&self) -> usize {
        self.legs.heap_bytes()
            + self
                .cells
                .values()
                .map(|v| v.capacity() * std::mem::size_of::<Stamp>() + 64)
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(x0: i32, x1: i32, t0: u64, t1: u64) -> Path {
        Path {
            frame: 0,
            x0,
            z0: 0,
            x1,
            z1: 0,
            t0: Tick(t0),
            t1: Tick(t1),
            radius: 16,
        }
    }

    #[test]
    fn bookings_find_legs_without_scans() {
        let mut ix = LegIndex::new(1000);
        let s = ix.book(path(0, 2048, 0, 4000), Owner(7));
        // At t = 2,500 the caravan is at x = 1,280: region 2.
        assert_eq!(ix.in_region(0, 2, 0, Tick(2500)), vec![s]);
        assert!(ix.in_region(0, 3, 0, Tick(500)).is_empty());
    }

    #[test]
    fn replanning_cancels_bookings_and_meetings_in_cascade() {
        let mut ix = LegIndex::new(1000);
        let a = ix.book(path(0, 2000, 0, 2000), Owner(1));
        let b = ix.book(path(2000, 0, 0, 2000), Owner(2));
        let m = ix.meeting(a, b).expect("they cross");
        // They close at 2 blocks a tick from 2,000 apart, and meet within 32 blocks.
        assert!(m.at.0 > 950 && m.at.0 <= 1000);
        assert!(ix.is_meeting_live(&m));
        // The first caravan turns back: its bookings and the meeting go stale.
        let a2 = ix.replan(a.leg, path(1000, 0, 1000, 2000)).unwrap();
        assert!(!ix.is_meeting_live(&m));
        assert!(!ix.is_live(a));
        assert!(ix.is_live(a2));
        assert_eq!(ix.in_region(0, 3, 0, Tick(500)), vec![b]);
        // The old path's booking for region 3 in the second window is found stale and dropped.
        assert!(ix.in_region(0, 3, 0, Tick(1500)).is_empty());
        assert!(ix.stats().stale_dropped >= 1);
    }

    #[test]
    fn a_purge_drops_only_the_past_windows_stale_bookings() {
        let mut ix = LegIndex::new(1000);
        let a = ix.book(path(0, 2000, 0, 2000), Owner(1));
        let b = ix.book(path(2000, 0, 0, 2000), Owner(2));
        let in_window = |ix: &LegIndex, s: Stamp, w: u64| {
            ix.cells
                .iter()
                .filter(|(c, v)| c.0 == w && v.contains(&s))
                .count()
        };
        let a_w0 = in_window(&ix, a, 0);
        assert!(a_w0 > 0);
        // a turns back at 1,000: every booking of its first path goes stale.
        let a2 = ix.replan(a.leg, path(1000, 0, 1000, 2000)).unwrap();
        let live = ix.live_bookings();
        let (all, _) = ix.booking_counts();
        // By 1,000 only window 0 has ended: a's stale bookings there go, b's live ones stay.
        assert_eq!(ix.purge_stale(Tick(1000)), a_w0 as u64);
        assert_eq!(ix.booking_counts().0, all - a_w0);
        assert_eq!(ix.live_bookings(), live, "every live booking stays");
        assert!(in_window(&ix, b, 0) > 0);
        assert!(in_window(&ix, a, 1) > 0, "window 1 has not ended");
        // Once b is cancelled and every window has ended, a2's bookings are all that is left, and
        // a cell a read emptied goes too.
        let only_b = *ix.cells.iter().find(|(_, v)| **v == [b]).unwrap().0;
        ix.cancel(b.leg);
        let (w, frame, rx, rz) = only_b;
        assert!(ix.in_region(frame, rx, rz, Tick(w * 1000)).is_empty());
        assert!(ix.cells[&only_b].is_empty(), "the read keeps the cell");
        ix.purge_stale(Tick(10_000));
        assert!(!ix.cells.contains_key(&only_b));
        let a2_cells = ix.cells_of(ix.get(a2.leg).unwrap().path).len();
        assert_eq!(ix.booking_counts(), (a2_cells, a2_cells));
        assert_eq!(ix.stats().stale_dropped, (all - a2_cells) as u64);
        assert_eq!(ix.in_region(0, 1, 0, Tick(1500)), vec![a2]);
    }

    #[test]
    fn purges_run_at_game_time_boundaries() {
        let mut ix = LegIndex::new(1000);
        let a = ix.book(path(0, 2000, 0, 2000), Owner(1));
        ix.replan(a.leg, path(0, 10, 3000, 4000)).unwrap();
        let (before, _) = ix.booking_counts();
        ix.purge_due(Tick(PURGE_EVERY - 1));
        assert_eq!(ix.booking_counts().0, before, "no boundary passed");
        ix.purge_due(Tick(PURGE_EVERY));
        let (after, _) = ix.booking_counts();
        assert!(after < before);
        assert_eq!(after, ix.cells_of(ix.get(a.leg).unwrap().path).len());
        // The same boundary again does nothing; turned off, nothing runs.
        let stale = ix.stats().stale_dropped;
        ix.purge_due(Tick(PURGE_EVERY + 500));
        assert_eq!(ix.stats().stale_dropped, stale);
        let b = ix.book(path(0, 10, 0, 1000), Owner(2));
        ix.cancel(b.leg);
        ix.set_purge_every(0);
        ix.purge_due(Tick(10 * PURGE_EVERY));
        assert_eq!(ix.stats().stale_dropped, stale);
    }

    #[test]
    fn sharing_finds_each_crossing_leg_once() {
        let mut ix = LegIndex::new(1000);
        let a = ix.book(path(0, 4000, 0, 4000), Owner(1));
        // Shares many cells with a: found once.
        let b = ix.book(path(4000, 0, 0, 4000), Owner(2));
        // Far away in space.
        let mut far = path(0, 10, 0, 4000);
        far.z0 = 100_000;
        far.z1 = 100_000;
        let c = ix.book(far, Owner(3));
        // Same place, another time.
        let d = ix.book(path(0, 10, 50_000, 60_000), Owner(4));
        assert_eq!(ix.sharing(a), vec![b]);
        assert!(ix.sharing(c).is_empty());
        assert!(ix.sharing(d).is_empty());
        // A replanned leg's old bookings are not found, and a stale stamp finds nothing.
        let b2 = ix.replan(b.leg, path(9000, 9500, 0, 500)).unwrap();
        assert!(ix.sharing(a).is_empty());
        assert!(ix.sharing(b).is_empty());
        assert!(ix.sharing(b2).is_empty());
        assert_eq!(ix.window_ticks(), 1000);
    }

    fn seam_index() -> LegIndex {
        // An x wrap of 4,096 blocks: the seam at x = -2,048 / 2,048.
        let mut t = rtsim_space::Topologies::new();
        t.set(0, Topology::x_wrap(-2_048, 4_096));
        let mut ix = LegIndex::new(1000);
        ix.set_topologies(std::sync::Arc::new(t));
        ix
    }

    #[test]
    fn a_leg_across_a_seam_is_booked_on_both_sides_and_meets_once() {
        let mut ix = seam_index();
        // East from x = 1,900 to the image of x = -1,900 (2,196): over the seam.
        let a = ix.book(path(1900, 2196, 0, 1000), Owner(1));
        // Regions 3 (x in [1536, 2048)) and -4 (x in [-2048, -1536)) both hold it, in the same
        // window.
        assert_eq!(ix.in_region(0, 3, 0, Tick(500)), vec![a]);
        assert_eq!(ix.in_region(0, -4, 0, Tick(500)), vec![a]);
        assert_eq!(
            ix.position(&ix.get(a.leg).unwrap().path, Tick(1000)),
            (-1900, 0)
        );
        // A player standing just past the seam, at x = -1,950.
        let p = ix.book(path(-1950, -1950, 0, 1000), Owner(1 << 63));
        assert_eq!(ix.sharing(p), vec![a]);
        let m = ix.meeting(a, p).expect("they meet through the seam");
        // Chart x 2,146 is -1,950: within 32 blocks from chart 2,114, a tenth of the way less.
        assert!(m.at.0 > 700 && m.at.0 < 760, "{m:?}");
        assert_eq!(ix.sharing(a), vec![p]);
    }
}
