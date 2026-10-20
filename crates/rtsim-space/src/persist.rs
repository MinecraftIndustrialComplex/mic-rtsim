// SPDX-License-Identifier: AGPL-3.0-or-later
//! Save and restore of the sampled world model (X2-15), in the persist pattern of the other
//! modules (a magic, a version, a state hash checked on load; docs/overnight-choices-persist.md):
//!
//! - **the meta page**: the name tables (biomes, body kinds), the frames with seasons, the
//!   revision and the state hash;
//! - **one page per region** that has a known cell: its marks and its cells in index order.
//!
//! A save writes the meta page and only the regions changed since the last save, so its cost
//! follows what players explored since, not the world. The state hash folds every region's page
//! hash in key order with the tables', so a load recomputes it from the pages it read.

use rtsim_kernel::codec::{CodecError, Reader, Writer};
use rtsim_kernel::rng::fnv1a64;

use crate::habitat::{Creature, Habitat};
use crate::sampled::{Names, Region, RegionKey, SampledWorld};

const META_MAGIC: u32 = u32::from_le_bytes(*b"RTWM");
const REGION_MAGIC: u32 = u32::from_le_bytes(*b"RTWR");
const VERSION: u16 = 1;

/// A region's page id (48 bits): frame (12 bits), region x and z (18 bits each, two's
/// complement). Unique for frames below 4,096 and regions within ±131,072 (±67 million blocks:
/// past the world border).
pub fn region_page_id(k: RegionKey) -> u64 {
    (u64::from(k.frame) & 0xFFF) << 36
        | (k.rx as u32 as u64 & 0x3FFFF) << 18
        | (k.rz as u32 as u64 & 0x3FFFF)
}

fn encode_names(w: &mut Writer, n: &Names) {
    w.var_u64(n.names.len() as u64 - 1);
    for s in n.names.iter().skip(1) {
        w.bytes(s.as_bytes());
    }
}

fn decode_names(r: &mut Reader<'_>) -> Result<Names, CodecError> {
    let n = r.var_u64()? as usize;
    let mut names = Names::new();
    for _ in 0..n {
        let s = std::str::from_utf8(r.bytes()?)
            .map_err(|_| CodecError::Invalid("a name is not UTF-8".into()))?;
        names.intern(s);
    }
    if names.names.len() != n + 1 {
        return Err(CodecError::Invalid("a name table repeats a name".into()));
    }
    Ok(names)
}

fn encode_cell(w: &mut Writer, i: usize, h: &Habitat) {
    w.u8(i as u8);
    w.u16(h.known);
    w.u16(h.water);
    w.u16(h.ice);
    w.u16(h.fluid);
    w.u16(h.biome);
    w.u16(h.land_biome);
    w.u32(h.tags);
    w.u32(h.land_tags);
    for v in [
        h.surface_min,
        h.surface_max,
        h.surface_mean,
        h.water_mean,
        h.floor_mean,
    ] {
        w.u16(v as u16);
    }
    w.f32(h.temperature);
    w.f32(h.downfall);
    w.f32(h.celsius);
    w.u8(h.season);
    w.u8(h.season_count);
    w.u32(h.fingerprint);
    w.u8(h.creatures.len() as u8);
    for c in &h.creatures {
        w.u16(c.body);
        w.u16(c.share);
        w.u16(c.generated);
    }
}

fn decode_cell(r: &mut Reader<'_>) -> Result<(usize, Habitat), CodecError> {
    let i = usize::from(r.u8()?);
    let mut h = Habitat {
        known: r.u16()?,
        water: r.u16()?,
        ice: r.u16()?,
        fluid: r.u16()?,
        biome: r.u16()?,
        land_biome: r.u16()?,
        tags: r.u32()?,
        land_tags: r.u32()?,
        surface_min: r.u16()? as i16,
        surface_max: r.u16()? as i16,
        surface_mean: r.u16()? as i16,
        water_mean: r.u16()? as i16,
        floor_mean: r.u16()? as i16,
        temperature: r.f32()?,
        downfall: r.f32()?,
        celsius: r.f32()?,
        season: r.u8()?,
        season_count: r.u8()?,
        fingerprint: r.u32()?,
        creatures: Vec::new(),
    };
    let n = r.u8()?;
    for _ in 0..n {
        h.creatures.push(Creature {
            body: r.u16()?,
            share: r.u16()?,
            generated: r.u16()?,
        });
    }
    Ok((i, h))
}

fn encode_region(k: RegionKey, reg: &Region) -> Vec<u8> {
    let mut w = Writer::with_capacity(64 + reg.count as usize * 64);
    w.u32(REGION_MAGIC);
    w.u16(VERSION);
    w.u16(k.frame);
    w.u32(k.rx as u32);
    w.u32(k.rz as u32);
    w.u32(reg.marks);
    w.u8(reg.count as u8);
    for (i, h) in reg.cells() {
        encode_cell(&mut w, i, h);
    }
    w.into_bytes()
}

fn decode_region(b: &[u8]) -> Result<(RegionKey, Region), CodecError> {
    let mut r = Reader::new(b);
    if r.u32()? != REGION_MAGIC {
        return Err(CodecError::Invalid("not a world-model region page".into()));
    }
    let v = r.u16()?;
    if v != VERSION {
        return Err(CodecError::Invalid(format!(
            "world-model region page version {v}, this build reads {VERSION}"
        )));
    }
    let k = RegionKey {
        frame: r.u16()?,
        rx: r.u32()? as i32,
        rz: r.u32()? as i32,
    };
    let mut reg = Region::new();
    reg.marks = r.u32()?;
    let n = r.u8()?;
    for _ in 0..n {
        let (i, h) = decode_cell(&mut r)?;
        if i >= reg.cells.len() || reg.cells[i].is_some() {
            return Err(CodecError::Invalid(format!("cell index {i} out of place")));
        }
        reg.cells[i] = Some(h);
        reg.count += 1;
    }
    r.finish()?;
    reg.hash = fnv1a64(b);
    Ok((k, reg))
}

impl SampledWorld {
    fn tables_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        encode_names(&mut w, &self.biomes);
        encode_names(&mut w, &self.bodies);
        w.var_u64(self.seasonal.len() as u64);
        for f in &self.seasonal {
            w.u16(*f);
        }
        w.into_bytes()
    }

    /// The state hash: the tables' and every region's page hash, in key order.
    pub fn state_hash(&self) -> u64 {
        let mut h = fnv1a64(&self.tables_bytes());
        for (k, r) in &self.regions {
            h = h.rotate_left(7) ^ region_page_id(*k).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ r.hash;
        }
        h
    }

    /// The pages a save writes: the meta page, and every region changed since the last save
    /// (all of them with `all`). Returns (meta, regions as (page id, bytes)).
    pub fn save_pages(&mut self, all: bool) -> (Vec<u8>, Vec<(u64, Vec<u8>)>) {
        let keys: Vec<RegionKey> = if all {
            self.regions.keys().copied().collect()
        } else {
            std::mem::take(&mut self.dirty).into_iter().collect()
        };
        if all {
            self.dirty.clear();
        }
        let mut regions = Vec::with_capacity(keys.len());
        for k in keys {
            if let Some(reg) = self.regions.get_mut(&k) {
                let b = encode_region(k, reg);
                reg.hash = fnv1a64(&b);
                regions.push((region_page_id(k), b));
            }
        }
        let mut w = Writer::new();
        w.u32(META_MAGIC);
        w.u16(VERSION);
        w.u64(self.state_hash());
        w.u64(self.revision);
        w.u64(self.regions.len() as u64);
        w.raw(&self.tables_bytes());
        (w.into_bytes(), regions)
    }

    /// Every region is written at the next save (after a save that may not have landed).
    pub fn mark_all_dirty(&mut self) {
        self.dirty = self.regions.keys().copied().collect();
    }

    /// Regions waiting for the next save.
    pub fn dirty_regions(&self) -> usize {
        self.dirty.len()
    }

    /// Reads a world model back: its meta page and its region pages (any order). Returns it and
    /// the state hash the meta page saved, for the caller to compare with
    /// [`SampledWorld::state_hash`].
    pub fn load<'a>(
        meta: &[u8],
        regions: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<(SampledWorld, u64), CodecError> {
        let mut r = Reader::new(meta);
        if r.u32()? != META_MAGIC {
            return Err(CodecError::Invalid("not a world-model meta page".into()));
        }
        let v = r.u16()?;
        if v != VERSION {
            return Err(CodecError::Invalid(format!(
                "world-model meta page version {v}, this build reads {VERSION}"
            )));
        }
        let saved_hash = r.u64()?;
        let revision = r.u64()?;
        let region_count = r.u64()?;
        let mut w = SampledWorld::new();
        w.biomes = decode_names(&mut r)?;
        w.bodies = decode_names(&mut r)?;
        for _ in 0..r.var_u64()? {
            w.seasonal.insert(r.u16()?);
        }
        r.finish()?;
        w.revision = revision;
        for b in regions {
            let (k, reg) = decode_region(b)?;
            for (_, h) in reg.cells() {
                let bad_name = usize::from(h.biome) >= w.biomes.names.len()
                    || usize::from(h.land_biome) >= w.biomes.names.len()
                    || h.creatures
                        .iter()
                        .any(|c| usize::from(c.body) >= w.bodies.names.len());
                if bad_name {
                    return Err(CodecError::Invalid(
                        "a cell names a biome or body the tables lack".into(),
                    ));
                }
            }
            w.cells += u64::from(reg.count);
            if w.regions.insert(k, reg).is_some() {
                return Err(CodecError::Invalid(format!("region {k:?} twice")));
            }
        }
        if w.regions.len() as u64 != region_count {
            return Err(CodecError::Invalid(format!(
                "the meta page counts {region_count} regions, {} were read",
                w.regions.len()
            )));
        }
        Ok((w, saved_hash))
    }
}
