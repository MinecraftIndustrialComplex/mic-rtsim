// SPDX-License-Identifier: AGPL-3.0-or-later
//! A fake habitat sampler for tests, the CLI and other crates' tests (scaffolding): it builds
//! HABITAT records from a closure over block columns the way the core's `HabitatSampler` does
//! (one point per chunk centre, the most common biome, means), so the engine side can be tested
//! without Minecraft.

use crate::habitat::{self, ALL_POINTS, CELL_BLOCKS, HabitatRecord, POINTS, tag};
use crate::model::WorldModel;
use crate::sampled::{Ingest, SampledWorld};

/// One sampled column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub surface: habitat::Surface,
    /// The biome's name.
    pub biome: &'static str,
    pub y: i16,
}

/// What a test world knows of a biome: its tags, vanilla temperature and downfall, and its
/// herd creatures' spawn shares (per mille).
#[derive(Clone, Debug)]
pub struct BiomeSpec {
    pub name: &'static str,
    pub tags: u32,
    pub temperature: f32,
    pub downfall: f32,
    pub creatures: Vec<(&'static str, u16)>,
}

/// Vanilla's plains, ocean, river, desert, snowy slopes, savanna and forest, with their creature
/// shares from vanilla 1.21.1's spawn data (`BiomeDefaultFeatures.plainsSpawns` and
/// `farmAnimals`, `OverworldBiomes`): plains weights sheep 12, pig 10, chicken 10, cow 8, horse 5,
/// donkey 1 of 46; snowy slopes goat 5 and rabbit 4 of 9; savanna adds horse 1 and donkey 1 to the
/// farm animals (40); forest is the farm animals (40) plus wolf 5 and rabbit 4 in some forests
/// (taken as 40).
pub fn vanilla_biomes() -> Vec<BiomeSpec> {
    vec![
        BiomeSpec {
            name: "minecraft:plains",
            tags: tag::PLAINS | tag::OVERWORLD,
            temperature: 0.8,
            downfall: 0.4,
            creatures: vec![
                ("minecraft:sheep", 261),
                ("minecraft:pig", 217),
                ("minecraft:cow", 174),
                ("minecraft:horse", 109),
            ],
        },
        BiomeSpec {
            name: "minecraft:ocean",
            tags: tag::OCEAN | tag::OVERWORLD,
            temperature: 0.5,
            downfall: 0.5,
            creatures: vec![],
        },
        BiomeSpec {
            name: "minecraft:river",
            tags: tag::RIVER | tag::OVERWORLD,
            temperature: 0.5,
            downfall: 0.5,
            creatures: vec![],
        },
        BiomeSpec {
            name: "minecraft:desert",
            tags: tag::DESERT | tag::HOT | tag::DRY | tag::OVERWORLD,
            temperature: 2.0,
            downfall: 0.0,
            creatures: vec![],
        },
        BiomeSpec {
            name: "minecraft:snowy_slopes",
            tags: tag::MOUNTAIN | tag::SLOPE | tag::SNOWY | tag::COLD | tag::OVERWORLD,
            temperature: -0.3,
            downfall: 0.9,
            creatures: vec![("minecraft:goat", 556)],
        },
        BiomeSpec {
            name: "minecraft:savanna",
            tags: tag::SAVANNA | tag::HOT | tag::DRY | tag::OVERWORLD,
            temperature: 2.0,
            downfall: 0.0,
            creatures: vec![
                ("minecraft:sheep", 300),
                ("minecraft:pig", 250),
                ("minecraft:cow", 200),
                ("minecraft:horse", 25),
            ],
        },
        BiomeSpec {
            name: "minecraft:forest",
            tags: tag::FOREST | tag::OVERWORLD,
            temperature: 0.7,
            downfall: 0.8,
            creatures: vec![
                ("minecraft:sheep", 300),
                ("minecraft:pig", 250),
                ("minecraft:cow", 200),
            ],
        },
    ]
}

/// A painter of test worlds: `paint` samples every point of a cell from a closure and feeds the
/// cell to a [`SampledWorld`] as the bridge would.
pub struct Painter {
    pub biomes: Vec<BiomeSpec>,
    /// Bodies' "dictionary": names by index + 1 (the fake Java dictionary).
    bodies: Vec<&'static str>,
    biome_ids: Vec<&'static str>,
}

impl Default for Painter {
    fn default() -> Self {
        Painter::new(vanilla_biomes())
    }
}

impl Painter {
    pub fn new(biomes: Vec<BiomeSpec>) -> Painter {
        Painter {
            biomes,
            bodies: Vec::new(),
            biome_ids: Vec::new(),
        }
    }

    fn spec(&self, name: &str) -> Option<&BiomeSpec> {
        self.biomes.iter().find(|b| b.name == name)
    }

    fn id(list: &mut Vec<&'static str>, name: &'static str) -> u32 {
        if let Some(i) = list.iter().position(|n| *n == name) {
            return i as u32 + 1;
        }
        list.push(name);
        list.len() as u32
    }

    /// The name behind a fake dictionary id (biomes and bodies).
    pub fn biome_name(&self, id: u32) -> &'static str {
        if id == 0 {
            ""
        } else {
            self.biome_ids[id as usize - 1]
        }
    }

    pub fn body_name(&self, id: u32) -> &'static str {
        if id == 0 {
            ""
        } else {
            self.bodies[id as usize - 1]
        }
    }

    /// The HABITAT record the bridge would send for cell (cx, cz) of `frame`, sampling `at`
    /// at each point (its chunk's centre column). `generated` adds chunk-generation animals.
    pub fn record(
        &mut self,
        frame: u16,
        cx: i32,
        cz: i32,
        at: &dyn Fn(i32, i32) -> Point,
        generated: &[(&'static str, u16)],
    ) -> HabitatRecord {
        let pts: Vec<Point> = (0..POINTS)
            .map(|i| {
                let (x, z) = habitat::point_block(cx, cz, i);
                at(x, z)
            })
            .collect();
        let mut water = 0u16;
        let mut ice = 0u16;
        let mut fluid = 0u16;
        for (i, p) in pts.iter().enumerate() {
            match p.surface {
                habitat::Surface::Water => water |= 1 << i,
                habitat::Surface::Ice => ice |= 1 << i,
                habitat::Surface::Fluid => fluid |= 1 << i,
                _ => {}
            }
        }
        let land_mask = ALL_POINTS & !water & !ice & !fluid;
        let most = |mask: u16| -> Option<&'static str> {
            let mut counts: Vec<(&'static str, u32, usize)> = Vec::new();
            for (i, p) in pts.iter().enumerate() {
                if mask & (1 << i) == 0 {
                    continue;
                }
                match counts.iter_mut().find(|c| c.0 == p.biome) {
                    Some(c) => c.1 += 1,
                    None => counts.push((p.biome, 1, i)),
                }
            }
            counts
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)))
                .map(|c| c.0)
        };
        let biome = most(ALL_POINTS).unwrap_or("");
        let land_biome = most(land_mask);
        let mut tags = 0u32;
        let (mut t, mut d) = (0.0f32, 0.0f32);
        for p in &pts {
            if let Some(s) = self.spec(p.biome) {
                tags |= s.tags;
                t += s.temperature;
                d += s.downfall;
            }
        }
        let land_tags = land_biome.and_then(|b| self.spec(b)).map_or(0, |s| s.tags);
        let ys: Vec<i16> = pts.iter().map(|p| p.y).collect();
        let mean = |v: &[i16]| -> i16 {
            if v.is_empty() {
                0
            } else {
                (v.iter().map(|&y| i32::from(y)).sum::<i32>() / v.len() as i32) as i16
            }
        };
        let water_ys: Vec<i16> = pts
            .iter()
            .enumerate()
            .filter(|(i, _)| water & (1 << i) != 0)
            .map(|(_, p)| p.y)
            .collect();
        // Creature shares averaged over the land points.
        let land_n = land_mask.count_ones().max(1);
        let mut shares: Vec<(&'static str, u32)> = Vec::new();
        for (i, p) in pts.iter().enumerate() {
            if land_mask & (1 << i) == 0 {
                continue;
            }
            if let Some(s) = self.spec(p.biome) {
                for &(b, sh) in &s.creatures {
                    match shares.iter_mut().find(|e| e.0 == b) {
                        Some(e) => e.1 += u32::from(sh),
                        None => shares.push((b, u32::from(sh))),
                    }
                }
            }
        }
        let mut creatures: Vec<(&'static str, u16, u16)> = shares
            .into_iter()
            .map(|(b, s)| (b, (s / land_n) as u16, 0))
            .collect();
        for &(b, g) in generated {
            match creatures.iter_mut().find(|e| e.0 == b) {
                Some(e) => e.2 += g,
                None => creatures.push((b, 0, g)),
            }
        }
        let fp = habitat::fingerprint(&[
            u64::from(water) | u64::from(ice) << 16 | u64::from(fluid) << 32,
            rtsim_kernel::rng::fnv1a64(biome.as_bytes()),
            rtsim_kernel::rng::fnv1a64(land_biome.unwrap_or("").as_bytes()),
            ys.iter()
                .fold(0u64, |h, &y| h.rotate_left(5) ^ y as u16 as u64),
        ]);
        let biome_id = if biome.is_empty() {
            0
        } else {
            Self::id(&mut self.biome_ids, biome)
        };
        let land_id = land_biome.map_or(0, |b| Self::id(&mut self.biome_ids, b));
        let creatures = creatures
            .into_iter()
            .map(|(b, s, g)| (Self::id(&mut self.bodies, b), s, g))
            .collect();
        HabitatRecord {
            dimension: u32::from(frame),
            fingerprint: fp,
            cell: (cx, cz),
            known: ALL_POINTS,
            water,
            ice,
            fluid,
            biome: biome_id,
            land_biome: land_id,
            tags,
            land_tags,
            surface_min: *ys.iter().min().expect("16 points"),
            surface_max: *ys.iter().max().expect("16 points"),
            surface_mean: mean(&ys),
            water_mean: mean(&water_ys),
            floor_mean: mean(&water_ys).saturating_sub(4),
            temperature: t / POINTS as f32,
            downfall: d / POINTS as f32,
            celsius: f32::NAN,
            season: 0,
            season_count: 0,
            creatures,
        }
    }

    /// Feeds a record to a world, resolving the fake dictionary.
    pub fn feed(&self, w: &mut SampledWorld, r: &HabitatRecord) -> Ingest {
        let bodies: Vec<&str> = r.creatures.iter().map(|c| self.body_name(c.0)).collect();
        w.ingest(
            r,
            self.biome_name(r.biome),
            self.biome_name(r.land_biome),
            &bodies,
        )
    }

    /// Samples and feeds every cell of the block rectangle `[x0, x1) × [z0, z1)`.
    pub fn paint(
        &mut self,
        w: &mut SampledWorld,
        frame: u16,
        (x0, z0, x1, z1): (i32, i32, i32, i32),
        at: &dyn Fn(i32, i32) -> Point,
    ) {
        let (cx0, cz0) = habitat::cell_of(x0, z0);
        let (cx1, cz1) = habitat::cell_of(x1 - 1, z1 - 1);
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                let r = self.record(frame, cx, cz, at, &[]);
                self.feed(w, &r);
            }
        }
    }
}

/// Land of `biome` at y 70.
pub fn land(biome: &'static str) -> Point {
    Point {
        surface: habitat::Surface::Land,
        biome,
        y: 70,
    }
}

/// Open water (an ocean's) at sea level, 62.
pub fn sea() -> Point {
    Point {
        surface: habitat::Surface::Water,
        biome: "minecraft:ocean",
        y: 62,
    }
}

/// Whether block (x, z) is in a known water point's chunk of a world (tests' check of a path).
pub fn water_at(w: &dyn WorldModel, frame: u16, x: i32, z: i32) -> Option<bool> {
    let cx = x.div_euclid(16) * 16;
    let cz = z.div_euclid(16) * 16;
    let a = w.area(frame, cx, cz, cx + 16, cz + 16);
    a.fully_known().then_some(a.water > 0)
}

/// Cells per block span, for tests sizing worlds.
pub const fn cells(blocks: i32) -> i32 {
    blocks / CELL_BLOCKS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Area;

    #[test]
    fn painted_cells_answer_area_queries() {
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        // Sea west of x = 0, plains east.
        p.paint(&mut w, 0, (-256, 0, 256, 256), &|x, _| {
            if x < 0 {
                sea()
            } else {
                land("minecraft:plains")
            }
        });
        assert_eq!(w.cell_count(), 8 * 4);
        let a: Area = w.area(0, -128, 0, 0, 128);
        assert!(a.fully_known() && a.all_water());
        let b = w.area(0, 0, 0, 128, 128);
        assert_eq!((b.points, b.land, b.water), (64, 64, 0));
        assert!((b.base_celsius - 19.52).abs() < 1e-3);
        assert_eq!(b.vegetation, 1.0);
        let c = w.area(0, 0, 300, 128, 428);
        assert_eq!((c.expected, c.points), (64, 0));
        assert!(c.base_celsius.is_nan());
        // Half a chunk-wide quad straddling the shore: four points, two of them water.
        let q = w.area(0, -16, 0, 16, 32);
        assert_eq!((q.points, q.water, q.land), (4, 2, 2));
        let cr = w.creatures(0, 0, 0, 256, 256);
        let sheep = cr.iter().find(|c| c.body == "minecraft:sheep").unwrap();
        assert!((sheep.habitat_cells - 16.0 * 0.261).abs() < 1e-9);
        assert_eq!(water_at(&w, 0, -5, 5), Some(true));
        assert_eq!(water_at(&w, 0, 5, 5), Some(false));
        assert_eq!(water_at(&w, 0, 5, 500), None);
    }

    #[test]
    fn latest_wins_and_generation_counts_only_rise() {
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        let plains = |_: i32, _: i32| land("minecraft:plains");
        let r = p.record(0, 3, 3, &plains, &[("minecraft:sheep", 4)]);
        assert_eq!(p.feed(&mut w, &r), Ingest::New);
        assert_eq!(p.feed(&mut w, &r), Ingest::Unchanged);
        let again = p.record(0, 3, 3, &plains, &[]);
        assert_eq!(
            p.feed(&mut w, &again),
            Ingest::Unchanged,
            "counts kept, not lowered"
        );
        let more = p.record(0, 3, 3, &plains, &[("minecraft:sheep", 6)]);
        assert_eq!(p.feed(&mut w, &more), Ingest::CreaturesOnly);
        let flooded = p.record(0, 3, 3, &|_, _| sea(), &[]);
        assert_eq!(p.feed(&mut w, &flooded), Ingest::Changed);
        let h = w.cell(0, 3, 3).unwrap();
        assert_eq!(h.water, ALL_POINTS);
        let g = h
            .creatures
            .iter()
            .find(|c| w.body_name(c.body) == "minecraft:sheep")
            .unwrap();
        assert_eq!(g.generated, 6, "what vanilla spawned stays counted");
        assert_eq!(w.take_changed().len(), 1);
        assert!(w.take_changed().is_empty());
    }

    #[test]
    fn pages_round_trip_to_the_same_state_hash() {
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        p.paint(&mut w, 0, (-600, -600, 600, 600), &|x, z| {
            if (x + z).rem_euclid(300) < 80 {
                sea()
            } else if x > 200 {
                land("minecraft:snowy_slopes")
            } else {
                land("minecraft:plains")
            }
        });
        p.paint(&mut w, 2, (0, 0, 128, 128), &|_, _| {
            land("minecraft:desert")
        });
        w.set_mark(
            crate::sampled::RegionKey::of_block(0, 0, 0),
            crate::sampled::mark::HERDS,
        );
        let (meta, regions) = w.save_pages(false);
        assert_eq!(regions.len(), w.region_count());
        let (back, saved) =
            SampledWorld::load(&meta, regions.iter().map(|(_, b)| b.as_slice())).unwrap();
        assert_eq!(saved, w.state_hash());
        assert_eq!(back.state_hash(), w.state_hash());
        assert_eq!(back.cell_count(), w.cell_count());
        assert_eq!(
            back.area(0, -600, -600, 600, 600),
            w.area(0, -600, -600, 600, 600)
        );
        assert_eq!(back.marks(crate::sampled::RegionKey::of_block(0, 0, 0)), 1);
        // A second save writes nothing but the meta page; a change writes its region only.
        let mut w2 = back;
        assert!(w2.save_pages(false).1.is_empty());
        p.paint(&mut w2, 0, (0, 0, 64, 64), &|_, _| sea());
        assert_eq!(w2.save_pages(false).1.len(), 1);
        // A truncated page is refused.
        assert!(SampledWorld::load(&meta[..meta.len() - 1], std::iter::empty()).is_err());
    }
}

#[cfg(test)]
mod size {
    use super::*;

    /// What a region costs in memory (bench/results/habitat-sampler.md): its 64 slots and the
    /// creature entries of its land cells.
    #[test]
    fn a_region_costs_a_few_kilobytes() {
        let slot = std::mem::size_of::<Option<crate::Habitat>>();
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        p.paint(&mut w, 0, (0, 0, 512, 512), &|_, _| {
            land("minecraft:plains")
        });
        let per_region = w.heap_bytes();
        eprintln!("a cell slot is {slot} bytes; a plains region holds {per_region} bytes of heap");
        assert!(slot <= 96, "{slot}");
        assert!(per_region < 16 * 1024, "{per_region}");
        let (meta, pages) = w.save_pages(true);
        eprintln!(
            "its page is {} bytes, the meta page {} bytes",
            pages[0].1.len(),
            meta.len()
        );
    }
}
