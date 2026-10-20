// SPDX-License-Identifier: AGPL-3.0-or-later
//! The vanilla world model: cells the bridge's habitat sampler sent (HABITAT,
//! docs/boundary.md §10), kept per 512-block region, latest wins per cell.
//!
//! **Cost** (SCALE.md §0): memory grows with the sampled cells (players' exploration), never with
//! the world's size; an ingest is O(1); [`WorldModel::area`] is O(cells the rectangle touches)
//! with popcounts over each cell's point masks. Regions changed since the last save are the only
//! pages a save writes ([`SampledWorld::save_pages`]).
//!
//! **Lazy** (SCALE.md §4): a cell nobody sampled has no row and answers as unknown.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::habitat::{
    self, ALL_POINTS, CELL_BLOCKS, CELL_CHUNKS, Creature, Habitat, HabitatRecord, POINTS,
    REGION_BLOCKS, REGION_CELLS, tag,
};
use crate::model::{
    Area, Creatures, SEASON_OFFSETS, WorldModel, season_offset_of, vanilla_celsius,
};

/// A region of one frame: (frame, region x, region z).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionKey {
    pub frame: u16,
    pub rx: i32,
    pub rz: i32,
}

impl RegionKey {
    pub fn of_cell(frame: u16, cx: i32, cz: i32) -> RegionKey {
        let (rx, rz) = habitat::region_of_cell(cx, cz);
        RegionKey { frame, rx, rz }
    }

    pub fn of_block(frame: u16, x: i32, z: i32) -> RegionKey {
        let (cx, cz) = habitat::cell_of(x, z);
        RegionKey::of_cell(frame, cx, cz)
    }

    /// The region's block rectangle, `[x0, x1) × [z0, z1)`.
    pub fn rect(&self) -> (i32, i32, i32, i32) {
        let (x0, z0) = (self.rx * REGION_BLOCKS, self.rz * REGION_BLOCKS);
        (x0, z0, x0 + REGION_BLOCKS, z0 + REGION_BLOCKS)
    }

    /// The first cell of the region.
    pub fn first_cell(&self) -> (i32, i32) {
        (self.rx * REGION_CELLS, self.rz * REGION_CELLS)
    }
}

/// Bits a module keeps on a region with the region's page (founding is decided once, X2-11).
pub mod mark {
    /// The herds evaluated the region for founding and decided (founded or barren).
    pub const HERDS: u32 = 1;
}

pub(crate) const REGION_SLOTS: usize = (REGION_CELLS * REGION_CELLS) as usize;

/// One region's cells.
#[derive(Clone, Debug)]
pub struct Region {
    pub(crate) cells: Vec<Option<Habitat>>,
    pub(crate) count: u32,
    pub(crate) marks: u32,
    /// FNV of the region's page as last written or read (the state hash's part).
    pub(crate) hash: u64,
}

impl Region {
    pub(crate) fn new() -> Region {
        Region {
            cells: vec![None; REGION_SLOTS],
            count: 0,
            marks: 0,
            hash: 0,
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = (usize, &Habitat)> {
        self.cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
    }

    pub fn known_cells(&self) -> u32 {
        self.count
    }

    pub fn marks(&self) -> u32 {
        self.marks
    }
}

/// An append-only table of names; index 0 is "none".
#[derive(Clone, Debug, Default)]
pub(crate) struct Names {
    pub(crate) names: Vec<String>,
    by_name: HashMap<String, u16>,
}

impl Names {
    pub(crate) fn new() -> Names {
        let mut n = Names::default();
        n.names.push(String::new());
        n
    }

    pub(crate) fn intern(&mut self, name: &str) -> u16 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&i) = self.by_name.get(name) {
            return i;
        }
        let i = self.names.len().min(usize::from(u16::MAX)) as u16;
        if usize::from(i) == self.names.len() {
            self.names.push(name.to_string());
            self.by_name.insert(name.to_string(), i);
        }
        i
    }

    pub(crate) fn name(&self, i: u16) -> &str {
        self.names.get(usize::from(i)).map_or("", |s| s.as_str())
    }
}

/// What an ingest did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ingest {
    /// A cell nobody had sampled.
    New,
    /// A cell whose terrain changed.
    Changed,
    /// Only the chunk-generation counts rose.
    CreaturesOnly,
    /// The same as held: nothing changed.
    Unchanged,
}

/// Counters for logs and inspection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampledStats {
    pub records: u64,
    pub new_cells: u64,
    pub changed_cells: u64,
    pub unchanged: u64,
}

/// The vanilla sampler's world model. Frames are Java's dimension ids.
#[derive(Clone, Debug)]
pub struct SampledWorld {
    pub(crate) regions: BTreeMap<RegionKey, Region>,
    pub(crate) biomes: Names,
    pub(crate) bodies: Names,
    /// Frames where a climate plugin reported seasons.
    pub(crate) seasonal: BTreeSet<u16>,
    pub(crate) revision: u64,
    pub(crate) cells: u64,
    /// Regions to write at the next save.
    pub(crate) dirty: BTreeSet<RegionKey>,
    /// Regions with a new or changed cell since a module last asked ([`Self::take_changed`]).
    changed: BTreeSet<RegionKey>,
    pub stats: SampledStats,
}

impl Default for SampledWorld {
    fn default() -> Self {
        SampledWorld::new()
    }
}

/// The vegetation index of a biome by its tags and downfall: 1 is open grassland, 0 bare ground.
/// TUNABLE PLACEHOLDERS, the vanilla sampler's own reading of what grows where (X2-9).
pub fn vegetation(tags: u32, downfall: f32) -> f32 {
    use tag::*;
    let base = if tags & (OCEAN | RIVER) != 0 {
        0.0
    } else if tags & (DESERT | BADLANDS) != 0 {
        0.05
    } else if tags & (PEAK | BEACH | STONY_SHORES) != 0 {
        0.1
    } else if tags & SNOWY != 0 {
        0.2
    } else if tags & SLOPE != 0 {
        0.35
    } else if tags & (PLAINS | SAVANNA) != 0 {
        1.0
    } else if tags & (MUSHROOM | JUNGLE) != 0 {
        0.4
    } else if tags & TAIGA != 0 {
        0.5
    } else if tags & SWAMP != 0 {
        0.55
    } else if tags & (FOREST | HILL | WINDSWEPT | MOUNTAIN | PLATEAU) != 0 {
        0.6
    } else {
        0.3 + 0.6 * downfall.clamp(0.0, 1.0)
    };
    let factor = if tags & SPARSE_VEGETATION != 0 {
        0.6
    } else if tags & DENSE_VEGETATION != 0 {
        0.85
    } else {
        1.0
    };
    base * factor
}

/// The year-round °C of a cell: a plugin's reading less the season it was taken in, else
/// vanilla's biome temperature on mic-climate's scale (X2-8).
pub fn base_celsius(h: &Habitat) -> f32 {
    if h.celsius.is_finite() {
        h.celsius - season_offset_of(h.season, h.season_count)
    } else {
        vanilla_celsius(h.temperature)
    }
}

/// Which points of a cell lie inside `[x0, x1) × [z0, z1)`.
fn points_inside(cx: i32, cz: i32, x0: i32, z0: i32, x1: i32, z1: i32) -> u16 {
    let mut xs = 0u16;
    let mut zs = 0u16;
    for p in 0..CELL_CHUNKS {
        let bx = cx * CELL_BLOCKS + p * 16 + 8;
        if bx >= x0 && bx < x1 {
            xs |= 1 << p;
        }
        let bz = cz * CELL_BLOCKS + p * 16 + 8;
        if bz >= z0 && bz < z1 {
            zs |= 1 << p;
        }
    }
    let mut m = 0u16;
    for pz in 0..CELL_CHUNKS {
        if zs & (1 << pz) != 0 {
            m |= xs << (pz * CELL_CHUNKS);
        }
    }
    m
}

impl SampledWorld {
    pub fn new() -> SampledWorld {
        SampledWorld {
            regions: BTreeMap::new(),
            biomes: Names::new(),
            bodies: Names::new(),
            seasonal: BTreeSet::new(),
            revision: 0,
            cells: 0,
            dirty: BTreeSet::new(),
            changed: BTreeSet::new(),
            stats: SampledStats::default(),
        }
    }

    /// Cells known, every frame.
    pub fn cell_count(&self) -> u64 {
        self.cells
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn region(&self, k: RegionKey) -> Option<&Region> {
        self.regions.get(&k)
    }

    pub fn regions(&self) -> impl Iterator<Item = (&RegionKey, &Region)> {
        self.regions.iter()
    }

    pub fn biome_name(&self, i: u16) -> &str {
        self.biomes.name(i)
    }

    pub fn body_name(&self, i: u16) -> &str {
        self.bodies.name(i)
    }

    /// The cell (cx, cz) of a frame.
    pub fn cell(&self, frame: u16, cx: i32, cz: i32) -> Option<&Habitat> {
        self.regions.get(&RegionKey::of_cell(frame, cx, cz))?.cells
            [habitat::index_in_region(cx, cz)]
        .as_ref()
    }

    /// Takes a HABITAT record, its names already resolved from Java's dictionary: `biome` and
    /// `land_biome` (empty for none) and each creature's body kind name, in the record's order.
    /// Latest wins per cell; chunk-generation counts only ever rise (X2-14).
    pub fn ingest(
        &mut self,
        rec: &HabitatRecord,
        biome: &str,
        land_biome: &str,
        bodies: &[&str],
    ) -> Ingest {
        self.stats.records += 1;
        let frame = rec.dimension as u16;
        let (cx, cz) = rec.cell;
        let mut creatures: Vec<Creature> = rec
            .creatures
            .iter()
            .zip(bodies)
            .filter(|(_, n)| !n.is_empty())
            .map(|(&(_, share, generated), n)| Creature {
                body: self.bodies.intern(n),
                share,
                generated,
            })
            .collect();
        creatures.sort_by_key(|c| c.body);
        creatures.dedup_by_key(|c| c.body);
        let mut h = Habitat {
            known: rec.known,
            water: rec.water & rec.known,
            ice: rec.ice & rec.known,
            fluid: rec.fluid & rec.known,
            biome: self.biomes.intern(biome),
            land_biome: self.biomes.intern(land_biome),
            tags: rec.tags,
            land_tags: rec.land_tags,
            surface_min: rec.surface_min,
            surface_max: rec.surface_max,
            surface_mean: rec.surface_mean,
            water_mean: rec.water_mean,
            floor_mean: rec.floor_mean,
            temperature: rec.temperature,
            downfall: rec.downfall,
            celsius: rec.celsius,
            season: rec.season,
            season_count: rec.season_count,
            fingerprint: rec.fingerprint,
            creatures,
        };
        if rec.season_count > 0 {
            self.seasonal.insert(frame);
        }
        let key = RegionKey::of_cell(frame, cx, cz);
        let idx = habitat::index_in_region(cx, cz);
        let region = self.regions.entry(key).or_insert_with(Region::new);
        let verdict = match &region.cells[idx] {
            None => Ingest::New,
            Some(old) => {
                // Generation counts only rise: what vanilla spawned once stays spawned.
                for c in &mut h.creatures {
                    if let Some(o) = old.creatures.iter().find(|o| o.body == c.body) {
                        c.generated = c.generated.max(o.generated);
                    }
                }
                for o in &old.creatures {
                    if o.generated > 0 && !h.creatures.iter().any(|c| c.body == o.body) {
                        h.creatures.push(Creature {
                            body: o.body,
                            share: 0,
                            generated: o.generated,
                        });
                    }
                }
                h.creatures.sort_by_key(|c| c.body);
                if terrain_eq(old, &h) {
                    if old.creatures == h.creatures {
                        Ingest::Unchanged
                    } else {
                        Ingest::CreaturesOnly
                    }
                } else {
                    Ingest::Changed
                }
            }
        };
        match verdict {
            Ingest::Unchanged => {
                self.stats.unchanged += 1;
                return verdict;
            }
            Ingest::New => {
                region.count += 1;
                self.cells += 1;
                self.stats.new_cells += 1;
            }
            Ingest::Changed | Ingest::CreaturesOnly => self.stats.changed_cells += 1,
        }
        region.cells[idx] = Some(h);
        self.revision += 1;
        self.dirty.insert(key);
        self.changed.insert(key);
        verdict
    }

    /// Regions with a new or changed cell since the last call, in key order.
    pub fn take_changed(&mut self) -> Vec<RegionKey> {
        std::mem::take(&mut self.changed).into_iter().collect()
    }

    /// Sets mark bits on a region (it exists once a cell of it is known).
    pub fn set_mark(&mut self, k: RegionKey, bits: u32) {
        if let Some(r) = self.regions.get_mut(&k)
            && r.marks & bits != bits
        {
            r.marks |= bits;
            self.dirty.insert(k);
        }
    }

    pub fn marks(&self, k: RegionKey) -> u32 {
        self.regions.get(&k).map_or(0, |r| r.marks)
    }

    /// What the world model holds, per region in key order, for HABITAT_KNOWN: (region, cells
    /// as (index, fingerprint)).
    pub fn known_lists(&self) -> Vec<(RegionKey, Vec<(u8, u32)>)> {
        self.regions
            .iter()
            .map(|(k, r)| {
                (
                    *k,
                    r.cells()
                        .map(|(i, h)| (i as u8, h.fingerprint))
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// Bytes of heap held, roughly (SCALE.md §6's accounting).
    pub fn heap_bytes(&self) -> usize {
        let per_cell = std::mem::size_of::<Option<Habitat>>();
        self.regions.len() * (REGION_SLOTS * per_cell + 64)
            + self
                .regions
                .values()
                .flat_map(|r| r.cells())
                .map(|(_, h)| h.creatures.capacity() * std::mem::size_of::<Creature>())
                .sum::<usize>()
    }
}

fn terrain_eq(a: &Habitat, b: &Habitat) -> bool {
    a.known == b.known
        && a.water == b.water
        && a.ice == b.ice
        && a.fluid == b.fluid
        && a.biome == b.biome
        && a.land_biome == b.land_biome
        && a.tags == b.tags
        && a.land_tags == b.land_tags
        && a.surface_min == b.surface_min
        && a.surface_max == b.surface_max
        && a.surface_mean == b.surface_mean
        && a.water_mean == b.water_mean
        && a.floor_mean == b.floor_mean
        && a.temperature.to_bits() == b.temperature.to_bits()
        && a.downfall.to_bits() == b.downfall.to_bits()
        && a.celsius.to_bits() == b.celsius.to_bits()
        && a.season == b.season
        && a.season_count == b.season_count
        && a.fingerprint == b.fingerprint
        && a.creatures
            .iter()
            .map(|c| (c.body, c.share))
            .eq(b.creatures.iter().map(|c| (c.body, c.share)))
}

impl WorldModel for SampledWorld {
    fn name(&self) -> &str {
        "vanilla sampler"
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn knows_frame(&self, frame: u16) -> bool {
        let lo = RegionKey {
            frame,
            rx: i32::MIN,
            rz: i32::MIN,
        };
        let hi = RegionKey {
            frame,
            rx: i32::MAX,
            rz: i32::MAX,
        };
        self.regions.range(lo..=hi).next().is_some()
    }

    fn area(&self, frame: u16, x0: i32, z0: i32, x1: i32, z1: i32) -> Area {
        let mut a = Area::default();
        if x1 <= x0 || z1 <= z0 {
            return a;
        }
        let (cx0, cz0) = habitat::cell_of(x0, z0);
        let (cx1, cz1) = habitat::cell_of(x1 - 1, z1 - 1);
        let (mut sum_y, mut sum_c, mut sum_veg, mut sum_down) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let (mut min_y, mut max_y) = (i32::MAX, i32::MIN);
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let inside = points_inside(cx, cz, x0, z0, x1, z1);
                if inside == 0 {
                    continue;
                }
                a.expected += inside.count_ones();
                let Some(h) = self.cell(frame, cx, cz) else {
                    continue;
                };
                let known = inside & h.known;
                if known == 0 {
                    continue;
                }
                let k = known.count_ones();
                let land = (known & h.land()).count_ones();
                a.points += k;
                a.land += land;
                a.water += (known & h.water).count_ones();
                a.ice += (known & h.ice).count_ones();
                a.fluid += (known & h.fluid).count_ones();
                min_y = min_y.min(i32::from(h.surface_min));
                max_y = max_y.max(i32::from(h.surface_max));
                sum_y += f64::from(k) * f64::from(h.surface_mean);
                sum_c += f64::from(k) * f64::from(base_celsius(h));
                sum_down += f64::from(k) * f64::from(h.downfall);
                sum_veg += f64::from(land) * f64::from(vegetation(h.land_tags, h.downfall));
            }
        }
        if a.points > 0 {
            let n = f64::from(a.points);
            a.min_y = min_y;
            a.max_y = max_y;
            a.mean_y = (sum_y / n) as f32;
            a.base_celsius = (sum_c / n) as f32;
            a.downfall = (sum_down / n) as f32;
        } else {
            a.base_celsius = f32::NAN;
        }
        if a.land > 0 {
            a.vegetation = (sum_veg / f64::from(a.land)) as f32;
        }
        a
    }

    fn season_offset(&self, frame: u16, sub: u32) -> f32 {
        if self.seasonal.contains(&frame) {
            SEASON_OFFSETS[((sub % 12) / 3) as usize]
        } else {
            0.0
        }
    }

    fn creatures(&self, frame: u16, x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<Creatures> {
        let mut by: BTreeMap<u16, (f64, u32)> = BTreeMap::new();
        if x1 <= x0 || z1 <= z0 {
            return Vec::new();
        }
        let (cx0, cz0) = habitat::cell_of(x0, z0);
        let (cx1, cz1) = habitat::cell_of(x1 - 1, z1 - 1);
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let inside = points_inside(cx, cz, x0, z0, x1, z1);
                let Some(h) = self.cell(frame, cx, cz) else {
                    continue;
                };
                let land = (inside & h.land()).count_ones();
                // A cell's generation counts go to the rectangle holding its centre.
                let (mx, mz) = (
                    cx * CELL_BLOCKS + CELL_BLOCKS / 2,
                    cz * CELL_BLOCKS + CELL_BLOCKS / 2,
                );
                let centre_in = mx >= x0 && mx < x1 && mz >= z0 && mz < z1;
                for c in &h.creatures {
                    let e = by.entry(c.body).or_default();
                    e.0 += f64::from(c.share) / 1000.0 * f64::from(land) / POINTS as f64;
                    if centre_in {
                        e.1 += u32::from(c.generated);
                    }
                }
            }
        }
        let mut out: Vec<Creatures> = by
            .into_iter()
            .map(|(b, (habitat_cells, generated))| Creatures {
                body: self.bodies.name(b).to_string(),
                habitat_cells,
                generated,
            })
            .collect();
        out.sort_by(|a, b| a.body.cmp(&b.body));
        out
    }

    fn habitat_at(&self, frame: u16, x: i32, z: i32) -> Option<Habitat> {
        let (cx, cz) = habitat::cell_of(x, z);
        self.cell(frame, cx, cz).cloned()
    }

    fn sampled(&mut self) -> Option<&mut SampledWorld> {
        Some(self)
    }

    fn sampled_ref(&self) -> Option<&SampledWorld> {
        Some(self)
    }
}

/// Whether a record covers every point (the vanilla sampler sends whole cells only).
pub fn whole(rec: &HabitatRecord) -> bool {
    rec.known == ALL_POINTS
}
