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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl DistrictMark {
    /// Its chunk box in blocks: x0, z0, x1, z1.
    pub fn block_box(&self) -> [f64; 4] {
        block_box(self.chunk_box)
    }
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
