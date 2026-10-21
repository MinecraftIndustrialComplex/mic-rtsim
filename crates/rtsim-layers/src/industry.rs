// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry layers' view of the industry engine: the live engine in the tap, the save's
//! summaries in the sidecar (rtsim-view's `IndustryView`), behind one trait.

use std::ops::Range;

use rtsim_industry::district::{District, Phase, Presence};
use rtsim_industry::engine::{Engine, LineEdge};
use rtsim_industry::ids::StationId;
use rtsim_industry::obs::StationDecl;
use rtsim_industry::threat::Leg;

/// What a district's map mark needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistrictMark {
    pub id: u64,
    pub dim: u32,
    /// Java's name id (DICT_NAME), 0 if unnamed.
    pub name: u32,
    pub chunks: u32,
    /// Chunk bounds: min x, min z, max x, max z.
    pub chunk_box: (i32, i32, i32, i32),
    /// What the engine's inspect would call it ("running", "learning", ...).
    pub state: &'static str,
    /// Its chunks' outline ([`chunk_outline`]), in chunk corners.
    pub outline: Vec<Vec<[i32; 2]>>,
}

impl DistrictMark {
    /// Its chunk box in blocks: x0, z0, x1, z1.
    pub fn block_box(&self) -> [f64; 4] {
        block_box(self.chunk_box)
    }

    /// Its outline in blocks.
    pub fn block_rings(&self) -> Vec<Vec<[f64; 2]>> {
        self.outline
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| [f64::from(p[0]) * 16.0, f64::from(p[1]) * 16.0])
                    .collect()
            })
            .collect()
    }
}

/// The outline of a set of chunks: closed rings of chunk corners (corner `(x, z)` is block
/// `(16x, 16z)`), one per boundary, walked clockwise as the map shows them (z down) with the
/// chunks on the right; a hole's ring runs the other way. Straight runs are one segment each.
/// Filled even-odd, the rings cover exactly the chunks.
pub fn chunk_outline<'a>(chunks: impl IntoIterator<Item = &'a (i32, i32)>) -> Vec<Vec<[i32; 2]>> {
    use std::collections::{BTreeMap, BTreeSet};
    let set: BTreeSet<(i32, i32)> = chunks.into_iter().copied().collect();
    // Boundary edges, from corner to corner, keyed by their start.
    let mut out: BTreeMap<[i32; 2], Vec<[i32; 2]>> = BTreeMap::new();
    let mut n = 0usize;
    for &(x, z) in &set {
        let mut edge = |a: [i32; 2], b: [i32; 2]| {
            out.entry(a).or_default().push(b);
            n += 1;
        };
        if !set.contains(&(x, z - 1)) {
            edge([x, z], [x + 1, z]);
        }
        if !set.contains(&(x + 1, z)) {
            edge([x + 1, z], [x + 1, z + 1]);
        }
        if !set.contains(&(x, z + 1)) {
            edge([x + 1, z + 1], [x, z + 1]);
        }
        if !set.contains(&(x - 1, z)) {
            edge([x, z + 1], [x, z]);
        }
    }
    let mut rings = Vec::new();
    while let Some((&start, _)) = out.iter().next() {
        let mut ring = vec![start];
        let mut at = start;
        while let Some(ends) = out.get_mut(&at) {
            let next = ends.pop().unwrap_or(start);
            if ends.is_empty() {
                out.remove(&at);
            }
            n = n.saturating_sub(1);
            if next == start {
                break;
            }
            ring.push(next);
            at = next;
        }
        rings.push(straighten(ring));
    }
    debug_assert_eq!(n, 0);
    rings
}

/// A closed ring with the corners dropped that lie on a straight run.
fn straighten(ring: Vec<[i32; 2]>) -> Vec<[i32; 2]> {
    let k = ring.len();
    if k < 4 {
        return ring;
    }
    let dir = |a: [i32; 2], b: [i32; 2]| [(b[0] - a[0]).signum(), (b[1] - a[1]).signum()];
    (0..k)
        .filter(|&i| {
            let (p, q, r) = (ring[(i + k - 1) % k], ring[i], ring[(i + 1) % k]);
            dir(p, q) != dir(q, r)
        })
        .map(|i| ring[i])
        .collect()
}

/// A chunk box in blocks: x0, z0, x1, z1.
pub fn block_box((x0, z0, x1, z1): (i32, i32, i32, i32)) -> [f64; 4] {
    [
        x0 as f64 * 16.0,
        z0 as f64 * 16.0,
        (x1 as f64 + 1.0) * 16.0,
        (z1 as f64 + 1.0) * 16.0,
    ]
}

/// A district's chunk bounds (all zero with no chunks).
pub fn chunk_box(d: &District) -> (i32, i32, i32, i32) {
    let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &(x, z) in &d.chunks {
        b = (b.0.min(x), b.1.min(z), b.2.max(x), b.3.max(z));
    }
    if d.chunks.is_empty() {
        b = (0, 0, 0, 0);
    }
    b
}

/// What the engine's inspect would call a district. A learned district is "running" when it
/// replays and "out of season" when not; `replays: None` leaves it "learned" for the caller.
pub fn district_state(d: &District, replays: Option<bool>) -> &'static str {
    match (d.presence, d.phase) {
        (Presence::Live, _) => "live",
        (Presence::Partial, _) => "suspended",
        (_, Phase::Damaged) => "damaged",
        (_, Phase::Learning) => "learning",
        (_, Phase::Stale) => "stale",
        (_, Phase::NotLearnable(_)) => "not learnable",
        (_, Phase::Learned) => match replays {
            Some(true) => "running",
            Some(false) => "out of season",
            None => "learned",
        },
    }
}

/// A district's mark.
pub fn district_mark(d: &District, replays: bool) -> DistrictMark {
    DistrictMark {
        id: d.id.0,
        dim: d.dim,
        name: d.name,
        chunks: d.chunks.len() as u32,
        chunk_box: chunk_box(d),
        state: district_state(d, Some(replays)),
        outline: chunk_outline(&d.chunks),
    }
}

/// An industry table the layers page through by key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Districts,
    Stations,
    /// Keyed by the first station's id.
    Lines,
    Legs,
}

/// The industry state the layers read.
pub trait IndustrySource {
    /// The districts with ids in `ids`, in id order.
    fn districts(&self, ids: Range<u64>, f: &mut dyn FnMut(DistrictMark));
    /// The stations with ids in `ids`, in id order.
    fn stations(&self, ids: Range<u64>, f: &mut dyn FnMut(&StationDecl));
    fn station(&self, id: u64) -> Option<&StationDecl>;
    /// The lines whose first station's id is in `ids`.
    fn lines(&self, ids: Range<u64>, f: &mut dyn FnMut((u64, u64), &LineEdge));
    /// The booked legs with ids in `ids`.
    fn legs(&self, ids: Range<u64>, f: &mut dyn FnMut(u64, &Leg));
    /// The first key of `t` at or after `from`.
    fn next_key(&self, t: Table, from: u64) -> Option<u64>;
}

impl IndustrySource for Engine {
    fn districts(&self, ids: Range<u64>, f: &mut dyn FnMut(DistrictMark)) {
        for d in self.districts_in(ids) {
            f(district_mark(d, self.replays(d.id)));
        }
    }

    fn stations(&self, ids: Range<u64>, f: &mut dyn FnMut(&StationDecl)) {
        for s in self.stations_in(ids) {
            f(s);
        }
    }

    fn station(&self, id: u64) -> Option<&StationDecl> {
        self.station_decl(StationId(id))
    }

    fn lines(&self, ids: Range<u64>, f: &mut dyn FnMut((u64, u64), &LineEdge)) {
        let lo = (StationId(ids.start), StationId(0));
        for ((a, b), l) in self.lines().range(lo..) {
            if a.0 >= ids.end {
                break;
            }
            f((a.0, b.0), l);
        }
    }

    fn legs(&self, ids: Range<u64>, f: &mut dyn FnMut(u64, &Leg)) {
        for (id, l) in self.legs().range(ids) {
            f(id, l);
        }
    }

    fn next_key(&self, t: Table, from: u64) -> Option<u64> {
        match t {
            Table::Districts => self.districts_in(from..u64::MAX).next().map(|d| d.id.0),
            Table::Stations => self.stations_in(from..u64::MAX).next().map(|s| s.station.0),
            Table::Lines => self
                .lines()
                .range((StationId(from), StationId(0))..)
                .next()
                .map(|((a, _), _)| a.0),
            Table::Legs => self.legs().range(from..u64::MAX).next().map(|(id, _)| id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rings: &[Vec<[i32; 2]>]) -> i64 {
        rings
            .iter()
            .map(|r| {
                let k = r.len();
                (0..k)
                    .map(|i| {
                        let (a, b) = (r[i], r[(i + 1) % k]);
                        i64::from(a[0]) * i64::from(b[1]) - i64::from(b[0]) * i64::from(a[1])
                    })
                    .sum::<i64>()
            })
            .sum::<i64>()
            / 2
    }

    #[test]
    fn an_outline_follows_the_chunks_not_their_box() {
        // An L of three chunks: one ring of six corners, area 3 (clockwise on the map: positive
        // with z down).
        let l = chunk_outline(&[(0, 0), (1, 0), (0, 1)]);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].len(), 6);
        assert_eq!(area(&l), 3);
        // A ring of eight chunks round a hole: two rings, the hole's turning the other way.
        let mut ring = Vec::new();
        for x in 0..3 {
            for z in 0..3 {
                if (x, z) != (1, 1) {
                    ring.push((x, z));
                }
            }
        }
        let o = chunk_outline(&ring);
        assert_eq!(o.len(), 2);
        assert_eq!(area(&o), 8);
        assert!(
            o.iter().all(|r| r.len() == 4),
            "straight runs are one segment: {o:?}"
        );
        // Two pieces.
        let two = chunk_outline(&[(0, 0), (5, 5)]);
        assert_eq!(two.len(), 2);
        assert_eq!(area(&two), 2);
        assert!(chunk_outline(&[]).is_empty());
    }
}
