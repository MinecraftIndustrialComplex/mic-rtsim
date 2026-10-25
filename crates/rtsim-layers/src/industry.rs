// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry layers' view of the industry engine: the live engine in the tap, the save's
//! summaries in the sidecar (rtsim-view's `IndustryView`), behind one trait.

use std::ops::Range;

use rtsim_industry::district::{District, Phase, Presence};
use rtsim_industry::engine::{Engine, LineEdge};
use rtsim_industry::ids::{StationId, StoreId};
use rtsim_industry::obs::StationDecl;
use rtsim_industry::stores::PackageStore;
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

/// Trips a Minecraft day (24,000 ticks) a line ran, over the time it was seen; None when it ran once
/// or was seen for less than a day.
pub fn line_per_day(l: &LineEdge) -> Option<f64> {
    let seen = l.last.0.saturating_sub(l.first.0);
    (l.trips > 1 && seen >= 24_000).then(|| l.trips as f64 * 24_000.0 / seen as f64)
}

/// A line's width in pixels: 2 for a line that hardly runs, up to 7, with the trips a day (or the
/// trips, for a line seen less than a day).
pub fn line_width(l: &LineEdge) -> f64 {
    let x = line_per_day(l).unwrap_or(l.trips as f64);
    (2.0 + 2.0 * (1.0 + x).log10()).min(7.0)
}

/// A district state ([`district_state`]) in words for a card or the key.
pub fn district_state_words(state: &str) -> &'static str {
    match state {
        "live" => "Loaded, run by the game",
        "suspended" => "Partly loaded, paused",
        "running" => "Unloaded, run by rtsim",
        "out of season" => "Learned, idle this season",
        "learning" => "Being learned",
        "learned" => "Learned",
        "damaged" => "Stopped: a machine was destroyed",
        "stale" => "Stopped: edited since it was learned",
        "not learnable" => "Stopped: not learnable",
        _ => "Unknown",
    }
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

/// One section of a district's compile (SECTIONS.md §8.3, slice S7), drawn as its box.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionMark {
    pub district: u64,
    pub dim: u32,
    /// Its index in the compile's sections.
    pub index: u32,
    /// [`rtsim_industry::glass::report::section_state`]: also its legend index.
    pub state: u32,
    pub working: bool,
    pub nodes: u32,
    pub stores: u32,
    /// Stores it shares with another section.
    pub shared_stores: u32,
    /// Its judging unit; None with one unit or none (the district's window judges).
    pub unit: Option<u32>,
    /// Its node box grown by a block, in blocks: x0, z0, x1, z1 (z1 and x1 past the last block).
    pub block_box: [f64; 4],
}

/// A section state's name, as the card and the key say it.
pub fn section_state_words(state: u32) -> &'static str {
    use rtsim_industry::glass::report::section_state as ss;
    match state {
        ss::LEARNING => "Learning",
        ss::PASSED => "Passed",
        ss::FAILED => "Failed",
        ss::STALE => "Stale: edited since it was learned",
        ss::FROZEN => "Frozen: its replay fell back",
        ss::REPLAYING => "Replaying",
        _ => "Unknown",
    }
}

/// The outline a section state's legend entry draws (`dash` learning, `bad` failed or stale, `dot`
/// frozen).
pub fn section_st(state: u32) -> Option<&'static str> {
    use rtsim_industry::glass::report::section_state as ss;
    match state {
        ss::LEARNING => Some("dash"),
        ss::FAILED | ss::STALE => Some("bad"),
        ss::FROZEN => Some("dot"),
        _ => None,
    }
}

/// One frogport of a package store.
#[derive(Clone, Debug, PartialEq)]
pub struct StorePort {
    pub x: f64,
    pub z: f64,
    pub district: Option<u64>,
    pub star: bool,
    /// The packages the store holds for it (booked, the world's: no replay draws them).
    pub packages: u64,
}

/// A package store (SECTIONS.md §2.5), drawn as edges between its frogports.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreMark {
    pub id: u64,
    pub dim: u32,
    pub ports: Vec<StorePort>,
    /// Pairs of ports (indices) the chain connects: their conveyors have a known hop count.
    pub links: Vec<(u16, u16)>,
    /// The packages the store holds in all, including the no-route pool.
    pub packages: u64,
    /// The last census's difference from the books, in packages.
    pub residue_packages: i64,
}

/// One part of a shared kinetic network: a district's SHARED domain, at its first source.
#[derive(Clone, Debug, PartialEq)]
pub struct NetPart {
    pub district: u64,
    pub x: f64,
    pub z: f64,
}

/// A shared kinetic network (SECTIONS.md §2.6): the parts the districts' compiles make, joined at
/// the sources they list in common.
#[derive(Clone, Debug, PartialEq)]
pub struct NetMark {
    /// Its index in the registry (the compiles' order, so stable for one save only).
    pub id: u64,
    pub dim: u32,
    /// [`rtsim_industry::glass::net::Status::words`].
    pub status: &'static str,
    pub coupled: bool,
    pub parts: Vec<NetPart>,
}

/// Java's hop count for a pair it could not measure (`PackageStores.UNKNOWN_HOPS`).
pub const UNKNOWN_HOPS: u16 = 0xFFFF;

/// The pairs of frogports (indices) that follow one another along the chain: a pair with a known
/// hop count and no third frogport between them (`hops(i, k) + hops(k, j) == hops(i, j)`), about
/// `n - 1` for a chain of `n`. A pair Java could not measure is no link.
pub fn chain_links(s: &PackageStore, n: usize) -> Vec<(u16, u16)> {
    let known = |i: usize, j: usize| {
        s.hops_between(i, j)
            .filter(|&h| h != UNKNOWN_HOPS)
            .map(u32::from)
    };
    let mut links = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            let Some(h) = known(i, j) else { continue };
            let between = (0..n).any(|k| {
                k != i
                    && k != j
                    && matches!((known(i, k), known(k, j)), (Some(a), Some(b)) if a + b == h)
            });
            if !between {
                links.push((i as u16, j as u16));
            }
        }
    }
    links
}

/// A package store's mark: its frogports and the pairs the chain connects.
pub fn store_mark(s: &PackageStore) -> StoreMark {
    let ports: Vec<StorePort> = s
        .frogports
        .iter()
        .zip(&s.pools)
        .map(|(f, p)| StorePort {
            x: f64::from(f.pos.x),
            z: f64::from(f.pos.z),
            district: f.district.map(|d| d.0),
            star: f.star,
            packages: p.packages,
        })
        .collect();
    let links = chain_links(s, ports.len());
    StoreMark {
        id: s.id.0,
        dim: s.dim,
        packages: ports.iter().map(|p| p.packages).sum::<u64>() + s.no_route.packages,
        ports,
        links,
        residue_packages: s.residue_packages,
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
    /// Keyed by the district's id (its sections).
    Sections,
    /// Keyed by the store's id.
    Stores,
    /// Keyed by the network's index.
    Networks,
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
    /// The sections of the districts with ids in `ids` (slice S7): none by default, as only a
    /// save holds the glass pages.
    fn sections(&self, _ids: Range<u64>, _f: &mut dyn FnMut(&SectionMark)) {}
    /// Why a save's glass pages give no sections, when some did not read (the sections and
    /// networks layers say so when they are empty).
    fn glass_why(&self) -> Option<String> {
        None
    }
    /// Why a save's package stores are fewer than its pages, when some did not read.
    fn stores_why(&self) -> Option<String> {
        None
    }
    /// The package stores with ids in `ids`.
    fn stores(&self, _ids: Range<u64>, _f: &mut dyn FnMut(&StoreMark)) {}
    /// The shared networks with indices in `ids`.
    fn networks(&self, _ids: Range<u64>, _f: &mut dyn FnMut(&NetMark)) {}
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

    fn stores(&self, ids: Range<u64>, f: &mut dyn FnMut(&StoreMark)) {
        for (id, s) in self.stores().range(StoreId(ids.start)..) {
            if id.0 >= ids.end {
                break;
            }
            f(&store_mark(s));
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
            Table::Stores => self
                .stores()
                .range(StoreId(from)..)
                .next()
                .map(|(id, _)| id.0),
            // The glass pages are the save's: the live engine's glass state is not walked here.
            Table::Sections | Table::Networks => None,
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

    fn store_of(n: i32, hops: Vec<u16>) -> PackageStore {
        use rtsim_industry::ids::BlockPos;
        use rtsim_industry::stores::{Frogport, Pool};
        let at = |x| BlockPos { x, y: 64, z: 0 };
        PackageStore {
            frogports: (0..n)
                .map(|i| Frogport {
                    district: None,
                    inventory: None,
                    star: false,
                    pos: at(i * 10),
                    conveyor: at(i * 10),
                })
                .collect(),
            pools: vec![Pool::default(); n as usize],
            hops,
            ..PackageStore::default()
        }
    }

    /// A chain of four frogports links neighbours only, not every pair; a pair Java could not
    /// measure links nothing.
    #[test]
    fn a_store_links_the_chain_and_skips_unknown_hops() {
        let chain = store_of(4, vec![10, 20, 30, 10, 20, 10]);
        assert_eq!(chain_links(&chain, 4), [(0, 1), (1, 2), (2, 3)]);
        let u = UNKNOWN_HOPS;
        let unknown = store_of(3, vec![u, 5, 7]);
        assert_eq!(chain_links(&unknown, 3), [(0, 2), (1, 2)]);
        let all_unknown = store_of(3, vec![u, u, u]);
        assert!(store_mark(&all_unknown).links.is_empty());
        // No hop table: no link, and a lone frogport has none either.
        assert!(chain_links(&store_of(3, Vec::new()), 3).is_empty());
        assert!(chain_links(&store_of(1, Vec::new()), 1).is_empty());
    }
}
