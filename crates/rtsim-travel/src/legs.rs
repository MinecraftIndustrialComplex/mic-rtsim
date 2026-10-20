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

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::{Arena, Handle};

/// Region edge in blocks, as Minecraft's region files (PLAN.md §5.2).
pub const REGION_BLOCKS: i64 = 512;

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
    legs: Arena<LegRow>,
    cells: BTreeMap<Cell, Vec<Stamp>>,
    /// Ticks per time window. PLACEHOLDER (overnight choice E13): one in-game hour, the accrual
    /// quantum.
    window: u64,
    stats: LegStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LegStats {
    pub booked_cells: u64,
    pub stale_dropped: u64,
}

impl LegIndex {
    pub fn new(window_ticks: u64) -> Self {
        LegIndex {
            legs: Arena::new(),
            cells: BTreeMap::new(),
            window: window_ticks.max(1),
            stats: LegStats::default(),
        }
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
        let mut out = Vec::new();
        for w in w0..=w1 {
            let a = p.at(Tick((w * self.window).max(t0)));
            let b = p.at(Tick(((w + 1) * self.window).min(t1)));
            let (xl, xh) = (a.0.min(b.0) - r, a.0.max(b.0) + r);
            let (zl, zh) = (a.1.min(b.1) - r, a.1.max(b.1) + r);
            for rx in xl.div_euclid(REGION_BLOCKS)..=xh.div_euclid(REGION_BLOCKS) {
                for rz in zl.div_euclid(REGION_BLOCKS)..=zh.div_euclid(REGION_BLOCKS) {
                    out.push((w, p.frame, rx, rz));
                }
            }
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

    /// Drops every window that ended before `before`.
    pub fn purge(&mut self, before: Tick) {
        let w = before.0 / self.window;
        self.cells = self.cells.split_off(&(w, 0, i64::MIN, i64::MIN));
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
        ix.purge(Tick(10_000));
        assert!(ix.in_region(0, 0, 0, Tick(500)).is_empty());
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
}
