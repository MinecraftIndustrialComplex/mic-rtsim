// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine's row tables on the kernel's page store (A16, SCALE.md §8;
//! docs/choices-paging-modules.md, section Conflict): what grows with the world lives in
//! [`PagedRows`], resident without a store and paged under a budget with one.
//!
//! - **Factions** (table 0x4310, keyed by the faction's index, which is never reused: factions
//!   are never removed): the faction, its feuds (folded into its row, PM-C2: a faction's feuds are
//!   few and every one of them is read with the faction's culture and chief) and the sources that
//!   reach it (folded, PM-C3: the old `source_index`).
//! - **Sources** (table 0x4312, keyed by a source id handed out in order and never reused): a
//!   continuous emitter, its key and kind, whom it counts against and the factions it reaches.
//!   A resident index from (key, kind) to the id and the rate (PM-C4) finds it.
//! - **Places** (table 0x4313, keyed by a packed region of 16 x 16 chunks): which sources have a
//!   chunk (or their position) there, for P14's reattribution and for a new camp's search for the
//!   sources already emitting near it (PM-C5; the old `source_chunks`).
//!
//! Table 0x4311 (feuds) is reserved and unused while feuds are folded into the faction rows.

use std::ops::{Index, IndexMut};

use rtsim_kernel::meta_pages::PagedMap;
use rtsim_kernel::rows::{PagedRows, Row};
use rtsim_knowledge::ActorId;

use crate::culture::SignalKind;
use crate::engine::{Faction, Feud, Source};
use crate::presence::ChunkKey;

/// Row tables take the conflict's table (0x4300) plus 0x10 and up, so rtsim-jni's per-table
/// deletion of 0x4300 pages never touches them.
pub const FACTION_TABLE: u16 = 0x4310;
/// Reserved for feuds, folded into the faction rows for now (PM-C2).
pub const FEUD_TABLE: u16 = 0x4311;
pub const SOURCE_TABLE: u16 = 0x4312;
pub const PLACE_TABLE: u16 = 0x4313;
/// The page tables of the resident structures when the conflict pages
/// (docs/choices-paging-followups.md PF10): the wheel's log, the parties, factions by key and by
/// cell, and the sources' index.
pub const WHEEL_TABLE: u16 = 0x4314;
pub const PARTY_TABLE: u16 = 0x4315;
pub const FACTION_KEY_TABLE: u16 = 0x4316;
pub const FACTION_CELL_TABLE: u16 = 0x4317;
pub const SOURCE_KEY_TABLE: u16 = 0x4318;

/// Factions per page. PLACEHOLDER 4: a faction with two feuds is about 450 bytes encoded (710
/// resident), so a page is about 1.8 KB. An event dirties its faction's page, and dirty pages stay
/// until a save (PM-C10), so the bytes an event pins grow with the page: at 1,024 factions under
/// 256 KiB, 13 saves a day at 4 a page against 44 at 16, and a third less step CPU (PM-C12).
/// Factions found together (a player's region) have neighbouring indices.
pub const FACTIONS_PER_PAGE: u64 = 4;
/// Sources per page. PLACEHOLDER 32 (a source is about 200 bytes with a district's chunks).
pub const SOURCES_PER_PAGE: u64 = 32;
/// Place regions per page: 4 x 4 regions of 16 x 16 chunks, a page 1,024 blocks square.
pub const PLACES_PER_PAGE: u64 = 16;

/// Chunks on a side of a place region (one row).
pub const REGION_CHUNKS: i32 = 16;

/// A faction's row: the faction, its feuds by actor and the sources that reach it.
#[derive(Clone, Debug, PartialEq)]
pub struct FactionRow {
    pub x: Faction,
    /// Feuds by actor, sorted (PM-C2).
    pub feuds: Vec<(ActorId, Feud)>,
    /// The sources that reach this faction, by (kind, source id), sorted (PM-C3).
    pub reach: Vec<(SignalKind, u32)>,
}

impl FactionRow {
    pub fn new(x: Faction) -> Self {
        FactionRow {
            x,
            feuds: Vec::new(),
            reach: Vec::new(),
        }
    }

    pub(crate) fn feud_at(&self, a: ActorId) -> Result<usize, usize> {
        self.feuds.binary_search_by(|e| e.0.cmp(&a))
    }

    /// The ids of the sources of `kind` that reach this faction, in id order.
    pub(crate) fn reach_of(&self, kind: SignalKind) -> impl Iterator<Item = u32> + '_ {
        let lo = self.reach.partition_point(|e| e.0 < kind);
        self.reach[lo..]
            .iter()
            .take_while(move |e| e.0 == kind)
            .map(|e| e.1)
    }

    pub(crate) fn add_reach(&mut self, kind: SignalKind, id: u32) {
        if let Err(i) = self.reach.binary_search(&(kind, id)) {
            self.reach.insert(i, (kind, id));
        }
    }

    pub(crate) fn remove_reach(&mut self, kind: SignalKind, id: u32) {
        if let Ok(i) = self.reach.binary_search(&(kind, id)) {
            self.reach.remove(i);
        }
    }
}

impl Row for FactionRow {
    fn heap_bytes(&self) -> usize {
        self.x.observed.capacity() * std::mem::size_of::<(f64, f64)>()
            + self.feuds.capacity() * std::mem::size_of::<(ActorId, Feud)>()
            + self.feuds.iter().filter(|e| e.1.notice.is_some()).count()
                * std::mem::size_of::<crate::boundary::Notice>()
            + self.reach.capacity() * std::mem::size_of::<(SignalKind, u32)>()
            + self.x.hunts.capacity() * std::mem::size_of::<crate::engine::Hunt>()
            + self
                .x
                .camp
                .as_deref()
                .map_or(0, crate::engine::Camp::heap_bytes)
    }
}

/// The factions, by index. Reads (`factions[f]`) load the page through `&self`; changes
/// (`&mut factions[f]`) dirty it.
pub struct FactionTable {
    pub(crate) rows: PagedRows<FactionRow>,
}

impl std::fmt::Debug for FactionTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rows.fmt(f)
    }
}

impl Default for FactionTable {
    fn default() -> Self {
        FactionTable {
            rows: PagedRows::new(FACTION_TABLE, FACTIONS_PER_PAGE),
        }
    }
}

impl FactionTable {
    /// Factions so far (indices `0..len`: none is ever removed).
    pub fn len(&self) -> usize {
        self.rows.len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, f: u32) -> Option<&Faction> {
        self.rows.get(u64::from(f)).map(|r| &r.x)
    }

    /// The faction's row, loading its page if needed.
    pub fn row(&self, f: u32) -> &FactionRow {
        self.rows
            .get(u64::from(f))
            .unwrap_or_else(|| panic!("no faction {f}"))
    }

    pub(crate) fn row_mut(&mut self, f: u32) -> &mut FactionRow {
        self.rows
            .get_mut(u64::from(f))
            .unwrap_or_else(|| panic!("no faction {f}"))
    }

    pub(crate) fn push(&mut self, x: Faction) -> u32 {
        let f = self.rows.len();
        self.rows.insert(f, FactionRow::new(x));
        u32::try_from(f).expect("fewer than 2^32 factions")
    }
}

impl Index<usize> for FactionTable {
    type Output = Faction;

    fn index(&self, f: usize) -> &Faction {
        &self.row(f as u32).x
    }
}

impl IndexMut<usize> for FactionTable {
    fn index_mut(&mut self, f: usize) -> &mut Faction {
        &mut self.row_mut(f as u32).x
    }
}

/// A source's row: its key and kind (so a lookup by id knows them) and the source.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRow {
    pub key: u64,
    pub kind: SignalKind,
    pub s: Source,
}

impl Row for SourceRow {
    fn heap_bytes(&self) -> usize {
        self.s.affects.capacity() * std::mem::size_of::<(u32, f64)>()
            + self.s.chunks.capacity() * std::mem::size_of::<ChunkKey>()
    }
}

/// What the resident index keeps of a source: its row's id and its rate (PM-C4: the native
/// engine compares every district's rate with the source's at every step, so the rate is read
/// without loading a page).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceRef {
    pub id: u32,
    pub rate: f64,
}

/// The continuous sources, by (key, kind) through a resident index to their rows' ids.
pub struct SourceTable {
    pub(crate) rows: PagedRows<SourceRow>,
    pub(crate) ids: PagedMap<(u64, SignalKind), SourceRef>,
    /// The next source id (never reused).
    pub(crate) next: u32,
}

impl std::fmt::Debug for SourceTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceTable")
            .field("rows", &self.rows)
            .field("next", &self.next)
            .finish()
    }
}

impl Default for SourceTable {
    fn default() -> Self {
        SourceTable {
            rows: PagedRows::new(SOURCE_TABLE, SOURCES_PER_PAGE),
            ids: PagedMap::new(),
            next: 0,
        }
    }
}

impl SourceTable {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The source, loading its page if needed.
    pub fn get(&self, k: &(u64, SignalKind)) -> Option<&Source> {
        let r = self.ids.get(k)?;
        self.rows.get(u64::from(r.id)).map(|x| &x.s)
    }

    /// The source's rate per tick (0: none), from the resident index: no page is read.
    pub fn rate(&self, k: &(u64, SignalKind)) -> f64 {
        self.ids.get(k).map_or(0.0, |r| r.rate)
    }

    /// Every source's key, in order (resident: no page is read).
    pub fn keys(&self) -> impl Iterator<Item = &(u64, SignalKind)> {
        self.ids.keys()
    }

    pub(crate) fn id_of(&self, k: &(u64, SignalKind)) -> Option<u32> {
        self.ids.get(k).map(|r| r.id)
    }

    pub(crate) fn by_id(&self, id: u32) -> Option<&SourceRow> {
        self.rows.get(u64::from(id))
    }

    pub(crate) fn by_id_mut(&mut self, id: u32) -> Option<&mut SourceRow> {
        self.rows.get_mut(u64::from(id))
    }

    /// Adds the source, or replaces the one under its key (keeping its id). Returns the id.
    pub(crate) fn insert(&mut self, k: (u64, SignalKind), s: Source) -> u32 {
        let rate = s.rate;
        let id = match self.ids.get_mut(&k) {
            Some(r) => {
                r.rate = rate;
                r.id
            }
            None => {
                let id = self.next;
                self.next = id.checked_add(1).expect("fewer than 2^32 sources ever");
                self.ids.insert(k, SourceRef { id, rate });
                id
            }
        };
        self.rows.insert(
            u64::from(id),
            SourceRow {
                key: k.0,
                kind: k.1,
                s,
            },
        );
        id
    }

    pub(crate) fn remove(&mut self, k: &(u64, SignalKind)) -> Option<(u32, Source)> {
        let r = self.ids.remove(k)?;
        self.rows.remove(u64::from(r.id)).map(|x| (r.id, x.s))
    }
}

impl Index<&(u64, SignalKind)> for SourceTable {
    type Output = Source;

    fn index(&self, k: &(u64, SignalKind)) -> &Source {
        self.get(k)
            .unwrap_or_else(|| panic!("no source {:x} {:?}", k.0, k.1))
    }
}

/// One place region's entries: (chunk, source id), sorted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceRow {
    pub entries: Vec<(ChunkKey, u32)>,
}

impl Row for PlaceRow {
    fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(ChunkKey, u32)>()
    }
}

/// Region coordinates are biased by this (regions of 256 blocks: plus or minus 33.5 million
/// blocks, past Minecraft's border) and masked to 18 bits. A chunk past the range shares a row
/// with another, which is harmless: entries carry the whole chunk.
const REGION_BIAS: i64 = 1 << 17;
const REGION_MASK: u64 = (1 << 18) - 1;

/// The row key of the region holding `(frame, chunk x, chunk z)`: the page is the frame and the
/// region's coordinates over 4 (48 bits), the slot the region within its 4 x 4 block of regions.
pub fn region_key(frame: u16, rx: i32, rz: i32) -> u64 {
    let bx = ((i64::from(rx) + REGION_BIAS) as u64) & REGION_MASK;
    let bz = ((i64::from(rz) + REGION_BIAS) as u64) & REGION_MASK;
    let page = (u64::from(frame) << 32) | ((bx >> 2) << 16) | (bz >> 2);
    page * PLACES_PER_PAGE + (((bx & 3) << 2) | (bz & 3))
}

fn region_of(c: ChunkKey) -> (i32, i32) {
    (c.1.div_euclid(REGION_CHUNKS), c.2.div_euclid(REGION_CHUNKS))
}

/// Which sources are where, by chunk (PM-C5).
pub struct PlaceIndex {
    pub(crate) rows: PagedRows<PlaceRow>,
}

impl std::fmt::Debug for PlaceIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rows.fmt(f)
    }
}

impl Default for PlaceIndex {
    fn default() -> Self {
        PlaceIndex {
            rows: PagedRows::new(PLACE_TABLE, PLACES_PER_PAGE),
        }
    }
}

impl PlaceIndex {
    fn key_of(c: ChunkKey) -> u64 {
        let (rx, rz) = region_of(c);
        region_key(c.0, rx, rz)
    }

    pub(crate) fn add(&mut self, c: ChunkKey, id: u32) {
        let k = Self::key_of(c);
        let e = (c, id);
        match self.rows.get_mut(k) {
            Some(r) => {
                if let Err(i) = r.entries.binary_search(&e) {
                    r.entries.insert(i, e);
                }
            }
            None => {
                self.rows.insert(k, PlaceRow { entries: vec![e] });
            }
        }
    }

    pub(crate) fn remove(&mut self, c: ChunkKey, id: u32) {
        let k = Self::key_of(c);
        let Some(r) = self.rows.get(k) else {
            return;
        };
        let Ok(i) = r.entries.binary_search(&(c, id)) else {
            return;
        };
        if r.entries.len() == 1 {
            self.rows.remove(k);
        } else if let Some(r) = self.rows.get_mut(k) {
            r.entries.remove(i);
        }
    }

    /// The ids with an entry at `c`, in id order.
    pub(crate) fn at(&self, c: ChunkKey) -> Vec<u32> {
        let Some(r) = self.rows.get(Self::key_of(c)) else {
            return Vec::new();
        };
        let lo = r.entries.partition_point(|e| e.0 < c);
        r.entries[lo..]
            .iter()
            .take_while(|e| e.0 == c)
            .map(|e| e.1)
            .collect()
    }

    /// The ids with an entry in the chunk rectangle `x0..=x1`, `z0..=z1` of `frame`, sorted and
    /// deduplicated. Reads the rows of the regions it covers, nothing else.
    pub(crate) fn in_rect(&self, frame: u16, x0: i64, z0: i64, x1: i64, z1: i64) -> Vec<u32> {
        let mut out = Vec::new();
        let rc = i64::from(REGION_CHUNKS);
        for rx in x0.div_euclid(rc)..=x1.div_euclid(rc) {
            for rz in z0.div_euclid(rc)..=z1.div_euclid(rc) {
                let Some(r) = self.rows.get(region_key(frame, rx as i32, rz as i32)) else {
                    continue;
                };
                for &((f, cx, cz), id) in &r.entries {
                    let (cx, cz) = (i64::from(cx), i64::from(cz));
                    if f == frame && cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1 {
                        out.push(id);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_keys_stay_in_the_page_key_range_and_group_neighbours() {
        for (f, x, z) in [
            (0u16, 0, 0),
            (u16::MAX, -117_200, 117_200),
            (3, i32::MIN / 16, i32::MAX / 16),
        ] {
            let k = region_key(f, x, z);
            assert!(k / PLACES_PER_PAGE < 1 << 48, "{f} {x} {z}");
        }
        // A 4 x 4 block of regions shares a page.
        let p = region_key(0, 4, 8) / PLACES_PER_PAGE;
        for x in 4..8 {
            for z in 8..12 {
                assert_eq!(region_key(0, x, z) / PLACES_PER_PAGE, p);
            }
        }
        assert_ne!(region_key(0, 8, 8) / PLACES_PER_PAGE, p);
        assert_ne!(region_key(1, 4, 8) / PLACES_PER_PAGE, p);
    }

    #[test]
    fn places_find_what_is_in_a_rectangle() {
        let mut p = PlaceIndex::default();
        p.add((0, 3, 4), 7);
        p.add((0, 3, 4), 2);
        p.add((0, -40, 17), 9);
        p.add((1, 3, 4), 5);
        assert_eq!(p.at((0, 3, 4)), vec![2, 7]);
        assert_eq!(p.in_rect(0, -50, 0, 10, 20), vec![2, 7, 9]);
        assert_eq!(p.in_rect(0, 0, 0, 10, 10), vec![2, 7]);
        p.remove((0, 3, 4), 7);
        p.remove((0, -40, 17), 9);
        assert_eq!(p.in_rect(0, -50, 0, 10, 20), vec![2]);
        assert_eq!(p.rows.len(), 2, "an emptied region's row goes");
    }
}
