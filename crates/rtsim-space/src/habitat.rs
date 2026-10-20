// SPDX-License-Identifier: AGPL-3.0-or-later
//! The habitat of one coarse cell, as the bridge's vanilla sampler takes it, and the boundary
//! records that carry it (docs/boundary.md §10; this file is the reference codec, mirrored by the
//! core's `habitat/HabitatRecords.java`: the two change together).
//!
//! **Cells and points.** A cell is 64 × 64 blocks (SCALE.md §2's field cells), so a 512-block
//! region holds 8 × 8 cells. The sampler reads one **point** per chunk, at the chunk's centre
//! column (block 8, 8 of the chunk): 16 points a cell. Point `i` is chunk `(i & 3, i >> 2)` of the
//! cell. Every mask below has bit `i` for point `i`.
//!
//! **What a point is.** The top block of `MOTION_BLOCKING_NO_LEAVES` (a heightmap that counts
//! fluids, so a lake's top is its water): water when that block holds water, ice when it is in
//! `minecraft:ice`, another fluid (lava) when it holds one, land otherwise.

use rtsim_kernel::rng::fnv1a64;

/// Cell edge, blocks (SCALE.md §2: density fields per 64-block cell).
pub const CELL_BLOCKS: i32 = 64;
/// Chunks per cell edge.
pub const CELL_CHUNKS: i32 = CELL_BLOCKS / 16;
/// Points (one per chunk) per cell.
pub const POINTS: usize = (CELL_CHUNKS * CELL_CHUNKS) as usize;
/// Cells per region edge (a 512-block region, as Minecraft's region files and SCALE.md §5).
pub const REGION_CELLS: i32 = 8;
/// Region edge, blocks.
pub const REGION_BLOCKS: i32 = CELL_BLOCKS * REGION_CELLS;
/// Every point of a cell.
pub const ALL_POINTS: u16 = u16::MAX;

/// Biome tag bits (docs/boundary.md §10.3): vanilla's `minecraft:` tags and NeoForge's common
/// `c:` tags, as the sampler finds them on a point's biome (`Holder.is(TagKey)`).
pub mod tag {
    pub const OCEAN: u32 = 1 << 0;
    pub const DEEP_OCEAN: u32 = 1 << 1;
    pub const RIVER: u32 = 1 << 2;
    pub const BEACH: u32 = 1 << 3;
    pub const MOUNTAIN: u32 = 1 << 4;
    pub const HILL: u32 = 1 << 5;
    pub const FOREST: u32 = 1 << 6;
    pub const TAIGA: u32 = 1 << 7;
    pub const JUNGLE: u32 = 1 << 8;
    pub const SAVANNA: u32 = 1 << 9;
    pub const BADLANDS: u32 = 1 << 10;
    pub const SWAMP: u32 = 1 << 11;
    pub const DESERT: u32 = 1 << 12;
    pub const PLAINS: u32 = 1 << 13;
    pub const SNOWY: u32 = 1 << 14;
    pub const COLD: u32 = 1 << 15;
    pub const HOT: u32 = 1 << 16;
    pub const WET: u32 = 1 << 17;
    pub const DRY: u32 = 1 << 18;
    pub const MUSHROOM: u32 = 1 << 19;
    pub const UNDERGROUND: u32 = 1 << 20;
    pub const OVERWORLD: u32 = 1 << 21;
    pub const NETHER: u32 = 1 << 22;
    pub const END: u32 = 1 << 23;
    pub const SPARSE_VEGETATION: u32 = 1 << 24;
    pub const DENSE_VEGETATION: u32 = 1 << 25;
    pub const PEAK: u32 = 1 << 26;
    pub const SLOPE: u32 = 1 << 27;
    pub const PLATEAU: u32 = 1 << 28;
    pub const WINDSWEPT: u32 = 1 << 29;
    pub const STONY_SHORES: u32 = 1 << 30;

    /// The tag ids, bit by bit (bit 31 is reserved).
    pub const NAMES: [&str; 31] = [
        "minecraft:is_ocean",
        "minecraft:is_deep_ocean",
        "minecraft:is_river",
        "minecraft:is_beach",
        "minecraft:is_mountain",
        "minecraft:is_hill",
        "minecraft:is_forest",
        "minecraft:is_taiga",
        "minecraft:is_jungle",
        "minecraft:is_savanna",
        "minecraft:is_badlands",
        "c:is_swamp",
        "c:is_desert",
        "c:is_plains",
        "c:is_snowy",
        "c:is_cold",
        "c:is_hot",
        "c:is_wet",
        "c:is_dry",
        "c:is_mushroom",
        "c:is_underground",
        "minecraft:is_overworld",
        "minecraft:is_nether",
        "minecraft:is_end",
        "c:is_sparse_vegetation",
        "c:is_dense_vegetation",
        "c:is_mountain/peak",
        "c:is_mountain/slope",
        "c:is_plateau",
        "c:is_windswept",
        "c:is_stony_shores",
    ];
}

/// What stands at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// Nobody sampled it: modules avoid it (X2-5).
    Unknown,
    Land,
    Water,
    Ice,
    /// Lava or another fluid that is not water.
    Fluid,
}

/// One creature kind a cell's ground supports, from the vanilla spawn data of its biomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Creature {
    /// The body kind, interned in the world's body table (`SampledWorld::body_name`).
    pub body: u16,
    /// The kind's share of the biome's `CREATURE` spawn weight, per mille, averaged over the
    /// cell's land points.
    pub share: u16,
    /// Animals of the kind vanilla spawned in the cell at chunk generation
    /// (`MobSpawnType.CHUNK_GENERATION`), counted when their chunks generated.
    pub generated: u16,
}

/// One sampled cell. Heights are block y of the top block; temperatures and downfall are
/// vanilla's biome values (unitless), the mean over the known points.
#[derive(Clone, Debug, PartialEq)]
pub struct Habitat {
    pub known: u16,
    pub water: u16,
    pub ice: u16,
    pub fluid: u16,
    /// The most common biome of the known points (interned; 0 none).
    pub biome: u16,
    /// The most common biome of the land points (interned; 0 none).
    pub land_biome: u16,
    /// Tag bits of every known point's biome, together.
    pub tags: u32,
    /// Tag bits of `land_biome`.
    pub land_tags: u32,
    pub surface_min: i16,
    pub surface_max: i16,
    pub surface_mean: i16,
    /// Mean top of the water points (0 when there are none).
    pub water_mean: i16,
    /// Mean `OCEAN_FLOOR` top under the water points (0 when there are none).
    pub floor_mean: i16,
    pub temperature: f32,
    pub downfall: f32,
    /// A climate plugin's °C at the cell's centre when it was sampled; NaN without one.
    pub celsius: f32,
    /// The plugin's season when sampled (`season` of `season_count`); count 0: no seasons.
    pub season: u8,
    pub season_count: u8,
    /// The bridge's fingerprint of the terrain fields (docs/boundary.md §10.4).
    pub fingerprint: u32,
    pub creatures: Vec<Creature>,
}

impl Habitat {
    /// Point index of a chunk position inside its cell.
    pub fn point_of_chunk(chunk_x: i32, chunk_z: i32) -> usize {
        (chunk_x.rem_euclid(CELL_CHUNKS) + chunk_z.rem_euclid(CELL_CHUNKS) * CELL_CHUNKS) as usize
    }

    /// The land points: known and none of water, ice or another fluid.
    pub fn land(&self) -> u16 {
        self.known & !self.water & !self.ice & !self.fluid
    }

    pub fn surface(&self, point: usize) -> Surface {
        let b = 1u16 << point;
        if self.known & b == 0 {
            Surface::Unknown
        } else if self.water & b != 0 {
            Surface::Water
        } else if self.ice & b != 0 {
            Surface::Ice
        } else if self.fluid & b != 0 {
            Surface::Fluid
        } else {
            Surface::Land
        }
    }

    pub fn known_points(&self) -> u32 {
        self.known.count_ones()
    }

    /// Whether every point was sampled (the vanilla sampler sends only whole cells).
    pub fn complete(&self) -> bool {
        self.known == ALL_POINTS
    }
}

/// The cell holding a block position.
pub fn cell_of(x: i32, z: i32) -> (i32, i32) {
    (x.div_euclid(CELL_BLOCKS), z.div_euclid(CELL_BLOCKS))
}

/// The region holding a cell.
pub fn region_of_cell(cx: i32, cz: i32) -> (i32, i32) {
    (cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS))
}

/// A cell's index inside its region (0 to 63, x fastest).
pub fn index_in_region(cx: i32, cz: i32) -> usize {
    (cx.rem_euclid(REGION_CELLS) + cz.rem_euclid(REGION_CELLS) * REGION_CELLS) as usize
}

/// The block column of point `i` of cell (cx, cz): the centre of its chunk.
pub fn point_block(cx: i32, cz: i32, i: usize) -> (i32, i32) {
    let (px, pz) = (i as i32 % CELL_CHUNKS, i as i32 / CELL_CHUNKS);
    (
        cx * CELL_BLOCKS + px * 16 + 8,
        cz * CELL_BLOCKS + pz * 16 + 8,
    )
}

// ---- the boundary records (docs/boundary.md §10) ------------------------------------------------

/// Kinds (docs/boundary.md §10.1).
pub const HABITAT: u16 = 0x00A0;
pub const HABITAT_KNOWN: u16 = 0x8900;

/// The world model's latest-wins observation range.
pub const STATE_KINDS: std::ops::RangeInclusive<u16> = 0x00A0..=0x00AF;
/// The world model's command range.
pub const COMMAND_KINDS: std::ops::RangeInclusive<u16> = 0x8900..=0x89FF;

/// Dictionary kind of biome names (docs/boundary.md §10.2).
pub const DICT_BIOME: u32 = 0x30;

/// HABITAT_KNOWN flag: the last record of this session's list.
pub const KNOWN_END: u32 = 1;

/// HABITAT field offsets (docs/boundary.md §10.3).
pub mod habitat_at {
    pub const DIM: usize = 8;
    pub const FINGERPRINT: usize = 12;
    pub const CELL_X: usize = 16;
    pub const CELL_Z: usize = 20;
    pub const KNOWN: usize = 24;
    pub const WATER: usize = 26;
    pub const ICE: usize = 28;
    pub const FLUID: usize = 30;
    pub const BIOME: usize = 32;
    pub const LAND_BIOME: usize = 36;
    pub const TAGS: usize = 40;
    pub const LAND_TAGS: usize = 44;
    pub const SURFACE_MIN: usize = 48;
    pub const SURFACE_MAX: usize = 50;
    pub const SURFACE_MEAN: usize = 52;
    pub const WATER_MEAN: usize = 54;
    pub const FLOOR_MEAN: usize = 56;
    pub const TEMPERATURE: usize = 60;
    pub const DOWNFALL: usize = 64;
    pub const CELSIUS: usize = 68;
    pub const SEASON: usize = 72;
    pub const SEASON_COUNT: usize = 73;
    pub const CREATURES: usize = 80;
    /// Bytes per creature entry: u32 body, u16 share, u16 generated.
    pub const CREATURE: usize = 8;

    pub const fn size(n: usize) -> usize {
        CREATURES + CREATURE * n
    }
}

/// HABITAT_KNOWN field offsets (docs/boundary.md §10.5).
pub mod known_at {
    pub const DIM: usize = 16;
    pub const REGION_X: usize = 20;
    pub const REGION_Z: usize = 24;
    pub const COUNT: usize = 28;
    pub const ENTRIES: usize = 32;
    /// Bytes per entry: u8 cell index in the region, 3 reserved, u32 fingerprint.
    pub const ENTRY: usize = 8;

    pub const fn size(n: usize) -> usize {
        ENTRIES + ENTRY * n
    }
}

/// A HABITAT record as it crosses the boundary: names are Java's dictionary ids.
#[derive(Clone, Debug, PartialEq)]
pub struct HabitatRecord {
    pub dimension: u32,
    pub fingerprint: u32,
    pub cell: (i32, i32),
    pub known: u16,
    pub water: u16,
    pub ice: u16,
    pub fluid: u16,
    pub biome: u32,
    pub land_biome: u32,
    pub tags: u32,
    pub land_tags: u32,
    pub surface_min: i16,
    pub surface_max: i16,
    pub surface_mean: i16,
    pub water_mean: i16,
    pub floor_mean: i16,
    pub temperature: f32,
    pub downfall: f32,
    pub celsius: f32,
    pub season: u8,
    pub season_count: u8,
    /// (body kind id, share per mille, generated).
    pub creatures: Vec<(u32, u16, u16)>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn i16_at(b: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}
fn i32_at(b: &[u8], at: usize) -> i32 {
    u32_at(b, at) as i32
}
fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}
fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

impl HabitatRecord {
    /// Decodes a HABITAT record (header included).
    pub fn decode(b: &[u8]) -> Result<HabitatRecord, String> {
        use habitat_at as a;
        if b.len() < a::CREATURES || u16_at(b, 0) != HABITAT {
            return Err(format!("not a HABITAT record ({} bytes)", b.len()));
        }
        let size = u16_at(b, 2) as usize;
        let n = u32_at(b, 4) as usize;
        if size > b.len() || size < a::size(n) {
            return Err(format!("HABITAT of {size} bytes cannot hold {n} creatures"));
        }
        let creatures = (0..n)
            .map(|i| {
                let at = a::CREATURES + a::CREATURE * i;
                (u32_at(b, at), u16_at(b, at + 4), u16_at(b, at + 6))
            })
            .collect();
        Ok(HabitatRecord {
            dimension: u32_at(b, a::DIM),
            fingerprint: u32_at(b, a::FINGERPRINT),
            cell: (i32_at(b, a::CELL_X), i32_at(b, a::CELL_Z)),
            known: u16_at(b, a::KNOWN),
            water: u16_at(b, a::WATER),
            ice: u16_at(b, a::ICE),
            fluid: u16_at(b, a::FLUID),
            biome: u32_at(b, a::BIOME),
            land_biome: u32_at(b, a::LAND_BIOME),
            tags: u32_at(b, a::TAGS),
            land_tags: u32_at(b, a::LAND_TAGS),
            surface_min: i16_at(b, a::SURFACE_MIN),
            surface_max: i16_at(b, a::SURFACE_MAX),
            surface_mean: i16_at(b, a::SURFACE_MEAN),
            water_mean: i16_at(b, a::WATER_MEAN),
            floor_mean: i16_at(b, a::FLOOR_MEAN),
            temperature: f32_at(b, a::TEMPERATURE),
            downfall: f32_at(b, a::DOWNFALL),
            celsius: f32_at(b, a::CELSIUS),
            season: b[a::SEASON],
            season_count: b[a::SEASON_COUNT],
            creatures,
        })
    }

    /// Encodes the record (header included), as the bridge writes it.
    pub fn encode(&self, out: &mut Vec<u8>) {
        use habitat_at as a;
        let size = a::size(self.creatures.len());
        let start = out.len();
        out.resize(start + size, 0);
        let b = &mut out[start..];
        put(b, 0, &HABITAT.to_le_bytes());
        put(b, 2, &(size as u16).to_le_bytes());
        put(b, 4, &(self.creatures.len() as u32).to_le_bytes());
        put(b, a::DIM, &self.dimension.to_le_bytes());
        put(b, a::FINGERPRINT, &self.fingerprint.to_le_bytes());
        put(b, a::CELL_X, &self.cell.0.to_le_bytes());
        put(b, a::CELL_Z, &self.cell.1.to_le_bytes());
        put(b, a::KNOWN, &self.known.to_le_bytes());
        put(b, a::WATER, &self.water.to_le_bytes());
        put(b, a::ICE, &self.ice.to_le_bytes());
        put(b, a::FLUID, &self.fluid.to_le_bytes());
        put(b, a::BIOME, &self.biome.to_le_bytes());
        put(b, a::LAND_BIOME, &self.land_biome.to_le_bytes());
        put(b, a::TAGS, &self.tags.to_le_bytes());
        put(b, a::LAND_TAGS, &self.land_tags.to_le_bytes());
        put(b, a::SURFACE_MIN, &self.surface_min.to_le_bytes());
        put(b, a::SURFACE_MAX, &self.surface_max.to_le_bytes());
        put(b, a::SURFACE_MEAN, &self.surface_mean.to_le_bytes());
        put(b, a::WATER_MEAN, &self.water_mean.to_le_bytes());
        put(b, a::FLOOR_MEAN, &self.floor_mean.to_le_bytes());
        put(b, a::TEMPERATURE, &self.temperature.to_bits().to_le_bytes());
        put(b, a::DOWNFALL, &self.downfall.to_bits().to_le_bytes());
        put(b, a::CELSIUS, &self.celsius.to_bits().to_le_bytes());
        b[a::SEASON] = self.season;
        b[a::SEASON_COUNT] = self.season_count;
        for (i, &(body, share, generated)) in self.creatures.iter().enumerate() {
            let at = a::CREATURES + a::CREATURE * i;
            put(b, at, &body.to_le_bytes());
            put(b, at + 4, &share.to_le_bytes());
            put(b, at + 6, &generated.to_le_bytes());
        }
    }
}

/// A HABITAT_KNOWN command: what the engine's world model holds of one region, so the bridge
/// sends again only what changed (docs/boundary.md §10.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownRecord {
    pub flags: u32,
    /// The engine's dictionary id of the dimension (0 on a bare END).
    pub dimension: u32,
    pub region: (i32, i32),
    /// (cell index in the region, fingerprint).
    pub cells: Vec<(u8, u32)>,
}

impl KnownRecord {
    pub fn size(&self) -> usize {
        known_at::size(self.cells.len())
    }

    /// Encodes the command with its sequence.
    pub fn encode(&self, cmd_seq: u64, out: &mut Vec<u8>) {
        let size = self.size();
        let start = out.len();
        out.resize(start + size, 0);
        let b = &mut out[start..];
        put(b, 0, &HABITAT_KNOWN.to_le_bytes());
        put(b, 2, &(size as u16).to_le_bytes());
        put(b, 4, &self.flags.to_le_bytes());
        put(b, 8, &cmd_seq.to_le_bytes());
        put(b, known_at::DIM, &self.dimension.to_le_bytes());
        put(b, known_at::REGION_X, &self.region.0.to_le_bytes());
        put(b, known_at::REGION_Z, &self.region.1.to_le_bytes());
        put(b, known_at::COUNT, &(self.cells.len() as u32).to_le_bytes());
        for (i, &(idx, fp)) in self.cells.iter().enumerate() {
            let at = known_at::ENTRIES + known_at::ENTRY * i;
            b[at] = idx;
            put(b, at + 4, &fp.to_le_bytes());
        }
    }

    /// Decodes the command (the fake Java side in tests).
    pub fn decode(b: &[u8]) -> Result<(u64, KnownRecord), String> {
        if b.len() < known_at::ENTRIES || u16_at(b, 0) != HABITAT_KNOWN {
            return Err("not a HABITAT_KNOWN record".into());
        }
        let n = u32_at(b, known_at::COUNT) as usize;
        if b.len() < known_at::size(n) {
            return Err(format!("HABITAT_KNOWN too short for {n} cells"));
        }
        let cells = (0..n)
            .map(|i| {
                let at = known_at::ENTRIES + known_at::ENTRY * i;
                (b[at], u32_at(b, at + 4))
            })
            .collect();
        Ok((
            u64::from_le_bytes(b[8..16].try_into().expect("8 bytes")),
            KnownRecord {
                flags: u32_at(b, 4),
                dimension: u32_at(b, known_at::DIM),
                region: (i32_at(b, known_at::REGION_X), i32_at(b, known_at::REGION_Z)),
                cells,
            },
        ))
    }
}

/// FNV-1a over a few words, folded to 32 bits: the fingerprint arithmetic the bridge uses
/// (docs/boundary.md §10.4), for the fake bridge in tests.
pub fn fingerprint(words: &[u64]) -> u32 {
    let mut bytes = Vec::with_capacity(words.len() * 8);
    for w in words {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    let h = fnv1a64(&bytes);
    (h ^ (h >> 32)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let r = HabitatRecord {
            dimension: 3,
            fingerprint: 0xDEAD_BEEF,
            cell: (-7, 12),
            known: ALL_POINTS,
            water: 0x00F0,
            ice: 0x0100,
            fluid: 0,
            biome: 9,
            land_biome: 10,
            tags: tag::PLAINS | tag::RIVER,
            land_tags: tag::PLAINS,
            surface_min: -61,
            surface_max: 80,
            surface_mean: 64,
            water_mean: 62,
            floor_mean: 55,
            temperature: 0.8,
            downfall: 0.4,
            celsius: f32::NAN,
            season: 2,
            season_count: 4,
            creatures: vec![(4, 260, 3), (5, 170, 0)],
        };
        let mut b = Vec::new();
        r.encode(&mut b);
        assert_eq!(b.len(), 96);
        let back = HabitatRecord::decode(&b).unwrap();
        assert!(back.celsius.is_nan());
        assert_eq!(
            HabitatRecord {
                celsius: 0.0,
                ..back
            },
            HabitatRecord { celsius: 0.0, ..r }
        );
        let k = KnownRecord {
            flags: KNOWN_END,
            dimension: 1,
            region: (-3, 4),
            cells: vec![(0, 7), (63, 9)],
        };
        let mut b = Vec::new();
        k.encode(77, &mut b);
        assert_eq!(KnownRecord::decode(&b).unwrap(), (77, k));
    }

    #[test]
    fn cells_regions_and_points() {
        assert_eq!(cell_of(-1, 64), (-1, 1));
        assert_eq!(region_of_cell(-1, 8), (-1, 1));
        assert_eq!(index_in_region(-1, 9), 7 + 8);
        assert_eq!(point_block(0, 0, 0), (8, 8));
        assert_eq!(point_block(-1, 0, 15), (-64 + 56, 56));
        assert_eq!(Habitat::point_of_chunk(-1, 2), 3 + 8);
    }
}
