// SPDX-License-Identifier: AGPL-3.0-or-later
//! The HABITAT base map (VIEWER.md §9): the world model's sampled cells drawn as tiles, one pixel
//! per chunk point at the deepest zoom. Every frame is a layer of one [`TileSource`], named
//! `habitat-<frame slug>`; tiles are cached in memory only, so nothing reaches the world folder.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use mic_viewer::api::{PyramidDesc, TileLayerDesc};
use mic_viewer::tiles::{Rendered, TILE, TileError, TileSource};
use rtsim_space::habitat::{CELL_CHUNKS, REGION_BLOCKS, tag};
use rtsim_space::{Habitat, Surface, WorldHandle};

/// The deepest zoom: 16 blocks (a chunk, one sample point) a pixel.
pub const MAX_ZOOM: i32 = -4;
/// The widest: 1,024 blocks a pixel, a tile 262,144 blocks across.
pub const MIN_ZOOM: i32 = -10;
/// Bumped when a tile's look changes.
pub const VERSION: u32 = 1;

/// The base map over a world model.
pub struct HabitatTiles {
    world: WorldHandle,
    /// Layer name to frame.
    layers: BTreeMap<String, u16>,
    /// Sampled regions per frame.
    regions: BTreeMap<u16, BTreeSet<(i32, i32)>>,
    /// The view's one worker: a tile renders while it holds this (VWD-15).
    worker: Arc<Mutex<()>>,
}

impl HabitatTiles {
    /// `slugs`: frame to the slug its layer is named by.
    /// `worker`: the lock every expensive answer holds, so the view does one thing at a time.
    pub fn new(
        world: WorldHandle,
        slugs: &BTreeMap<u16, String>,
        worker: Arc<Mutex<()>>,
    ) -> HabitatTiles {
        let mut regions: BTreeMap<u16, BTreeSet<(i32, i32)>> = BTreeMap::new();
        if let Some(s) = world.read().sampled_ref() {
            for (k, _) in s.regions() {
                regions.entry(k.frame).or_default().insert((k.rx, k.rz));
            }
        }
        let layers = regions
            .keys()
            .map(|f| (layer_name(slugs, *f), *f))
            .collect();
        HabitatTiles {
            world,
            layers,
            regions,
            worker,
        }
    }

    /// The base map layer names.
    pub fn layer_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.layers.keys().map(String::as_str)
    }

    /// The frames that have cells.
    pub fn frames(&self) -> impl Iterator<Item = u16> + '_ {
        self.regions.keys().copied()
    }

    /// Blocks `[x0, z0, x1, z1]` the frame's sampled regions cover.
    pub fn bounds(&self, frame: u16) -> Option<[f64; 4]> {
        let r = self.regions.get(&frame)?;
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for &(rx, rz) in r {
            let (x0, z0) = ((rx * REGION_BLOCKS) as f64, (rz * REGION_BLOCKS) as f64);
            b = [
                b[0].min(x0),
                b[1].min(z0),
                b[2].max(x0 + REGION_BLOCKS as f64),
                b[3].max(z0 + REGION_BLOCKS as f64),
            ];
        }
        (b[0] <= b[2]).then_some(b)
    }

    /// The frame's base map descriptor.
    pub fn desc(&self, slugs: &BTreeMap<u16, String>, frame: u16) -> Option<TileLayerDesc> {
        let bounds = self.bounds(frame)?;
        let id = layer_name(slugs, frame);
        Some(TileLayerDesc {
            url: format!("tiles/{id}/{{z}}/{{x}}/{{y}}.png"),
            id,
            label: "Habitat".into(),
            pyramid: PyramidDesc {
                kind: "square".into(),
                min_zoom: MIN_ZOOM,
                max_zoom: MAX_ZOOM,
            },
            bounds,
            overlay: false,
            attribution: "rtsim HABITAT: 64-block cells, a sample per chunk".into(),
            default_on: Some(true),
            alpha: None,
        })
    }

    pub fn has_layer(&self, layer: &str) -> bool {
        self.layers.contains_key(layer)
    }

    fn touches(&self, frame: u16, x0: f64, z0: f64, size: f64) -> bool {
        let Some(set) = self.regions.get(&frame) else {
            return false;
        };
        let rb = REGION_BLOCKS as f64;
        let (rx0, rz0) = ((x0 / rb).floor() as i64, (z0 / rb).floor() as i64);
        let (rx1, rz1) = (
            ((x0 + size) / rb).ceil() as i64,
            ((z0 + size) / rb).ceil() as i64,
        );
        set.iter().any(|&(rx, rz)| {
            (rx0..rx1).contains(&i64::from(rx)) && (rz0..rz1).contains(&i64::from(rz))
        })
    }
}

/// The layer (and tile source) name of a frame.
pub fn layer_name(slugs: &BTreeMap<u16, String>, frame: u16) -> String {
    match slugs.get(&frame) {
        Some(s) => format!("habitat-{s}"),
        None => format!("habitat-dim{frame}"),
    }
}

impl TileSource for HabitatTiles {
    fn render(&self, layer: &str, z: i32, x: i64, y: i64) -> Result<Rendered, TileError> {
        let Some(&frame) = self.layers.get(layer) else {
            return Err(TileError::Missing(format!("no base map {layer:?}")));
        };
        if !(MIN_ZOOM..=MAX_ZOOM).contains(&z) {
            return Err(TileError::Missing(format!(
                "zoom {z} is outside {MIN_ZOOM}..={MAX_ZOOM}"
            )));
        }
        let bpp = 2f64.powi(-z);
        let size = bpp * f64::from(TILE);
        let (x0, z0) = (x as f64 * size, y as f64 * size);
        if !x0.is_finite() || !z0.is_finite() || !self.touches(frame, x0, z0, size) {
            return Ok(Rendered::Empty);
        }
        let _one = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        let guard = self.world.read();
        let Some(world) = guard.sampled_ref() else {
            return Ok(Rendered::Empty);
        };
        let n = TILE as usize;
        let mut px = vec![0u8; n * n * 4];
        let mut any = false;
        let mut last: Option<((i32, i32), Option<&Habitat>)> = None;
        for row in 0..n {
            let bz = z0 + (row as f64 + 0.5) * bpp;
            let chunk_z = (bz / 16.0).floor() as i32;
            for col in 0..n {
                let bx = x0 + (col as f64 + 0.5) * bpp;
                let chunk_x = (bx / 16.0).floor() as i32;
                let cell = (
                    chunk_x.div_euclid(CELL_CHUNKS),
                    chunk_z.div_euclid(CELL_CHUNKS),
                );
                let h = match last {
                    Some((c, h)) if c == cell => h,
                    _ => {
                        let h = world.cell(frame, cell.0, cell.1);
                        last = Some((cell, h));
                        h
                    }
                };
                let Some(h) = h else { continue };
                let point = Habitat::point_of_chunk(chunk_x, chunk_z);
                if let Some(rgb) = colour(h, point) {
                    let i = (row * n + col) * 4;
                    px[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                    any = true;
                }
            }
        }
        Ok(if any {
            Rendered::Rgba(px)
        } else {
            Rendered::Empty
        })
    }

    fn opaque(&self, _layer: &str) -> bool {
        false
    }

    fn version(&self) -> u32 {
        VERSION
    }
}

/// A point's colour: water blue by depth, ice, other fluids orange, land by its biome's tags and
/// shaded by height; unknown points transparent (`None`).
pub fn colour(h: &Habitat, point: usize) -> Option<[u8; 3]> {
    match h.surface(point) {
        Surface::Unknown => None,
        Surface::Water => {
            let depth = f64::from(h.water_mean) - f64::from(h.floor_mean);
            let deep = (depth / 40.0).clamp(0.0, 1.0);
            let (shallow, abyss) = if h.tags & tag::RIVER != 0 {
                ([90.0, 150.0, 210.0], [50.0, 110.0, 180.0])
            } else {
                ([70.0, 130.0, 200.0], [20.0, 50.0, 120.0])
            };
            Some(mix(shallow, abyss, deep))
        }
        Surface::Ice => Some([205, 230, 245]),
        Surface::Fluid => Some([225, 100, 30]),
        Surface::Land => {
            let base = land(h.land_tags);
            let shade = (0.8 + (f64::from(h.surface_mean) - 64.0) / 320.0).clamp(0.7, 1.25);
            Some([
                (base[0] * shade).clamp(0.0, 255.0) as u8,
                (base[1] * shade).clamp(0.0, 255.0) as u8,
                (base[2] * shade).clamp(0.0, 255.0) as u8,
            ])
        }
    }
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [u8; 3] {
    [
        (a[0] + (b[0] - a[0]) * t) as u8,
        (a[1] + (b[1] - a[1]) * t) as u8,
        (a[2] + (b[2] - a[2]) * t) as u8,
    ]
}

/// Land colour by the first tag that names it, in this order.
fn land(tags: u32) -> [f64; 3] {
    const ORDER: [(u32, [f64; 3]); 17] = [
        (tag::NETHER, [120.0, 45.0, 40.0]),
        (tag::END, [215.0, 215.0, 165.0]),
        (tag::SNOWY, [232.0, 238.0, 242.0]),
        (tag::PEAK, [150.0, 150.0, 150.0]),
        (tag::MOUNTAIN, [140.0, 138.0, 128.0]),
        (tag::STONY_SHORES, [130.0, 130.0, 128.0]),
        (tag::BEACH, [225.0, 214.0, 160.0]),
        (tag::DESERT, [222.0, 202.0, 140.0]),
        (tag::BADLANDS, [190.0, 112.0, 62.0]),
        (tag::MUSHROOM, [150.0, 112.0, 150.0]),
        (tag::SWAMP, [82.0, 100.0, 62.0]),
        (tag::JUNGLE, [42.0, 130.0, 42.0]),
        (tag::TAIGA, [62.0, 108.0, 82.0]),
        (tag::FOREST, [60.0, 124.0, 56.0]),
        (tag::SAVANNA, [172.0, 164.0, 82.0]),
        (tag::PLAINS, [122.0, 170.0, 82.0]),
        (tag::HILL, [110.0, 140.0, 85.0]),
    ];
    ORDER
        .iter()
        .find(|(t, _)| tags & t != 0)
        .map_or([112.0, 150.0, 92.0], |(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_colour_by_surface() {
        let mut h = Habitat {
            known: 0,
            water: 0,
            ice: 0,
            fluid: 0,
            biome: 0,
            land_biome: 0,
            tags: 0,
            land_tags: 0,
            surface_min: 0,
            surface_max: 0,
            surface_mean: 0,
            water_mean: 0,
            floor_mean: 0,
            temperature: 0.0,
            downfall: 0.0,
            celsius: 0.0,
            season: 0,
            season_count: 0,
            fingerprint: 0,
            creatures: Vec::new(),
        };
        assert_eq!(colour(&h, 0), None);
        h.known = 0b111;
        h.water = 0b010;
        h.ice = 0b100;
        h.land_tags = tag::DESERT;
        h.surface_mean = 64;
        assert_eq!(colour(&h, 0), Some([177, 161, 112]));
        assert!(colour(&h, 1).is_some_and(|c| c[2] > c[0]));
        assert_eq!(colour(&h, 2), Some([205, 230, 245]));
    }
}
