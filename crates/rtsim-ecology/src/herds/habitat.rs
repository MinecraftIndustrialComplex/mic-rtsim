// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herds' climate from the world model (rtsim-space): real biomes, water and temperatures in
//! place of the uniform grassland (Y3), and herds founded in real worlds (Y4, Y20).
//!
//! - [`HabitatClimate`] answers the herds' [`Climate`] per 128-block forage cell from the world
//!   model's [`Area`] of that cell: forage from the land share and its vegetation, growth from
//!   the temperature, terrain from water and relief. **Unknown ground is impassable** (X2-5): a
//!   forage cell is crossed only when every point of it was sampled, so a herd never plans into
//!   ground nobody has seen, and never across water (a cell over half water is closed, X2-10).
//! - [`plan_region`]: one herd per suitable region on its first sample (Y4's recommendation),
//!   the species drawn by the region's own creature spawn data, the count from TUNABLE densities,
//!   vanilla's chunk-generation animals counted as founders (Y20's recommendation, X2-13), all
//!   on the forage cells a herd may enter, and the count capped by what those cells yield a year
//!   ([`range_yield`]), so a range's K is its carrying capacity (EQC500, EQC501);
//!   [`range_capacity`] re-derives it when the region's ground changes (EQC504).
//!
//! **Periodic in the year?** The herds' forage closed form assumes the climate repeats every
//! year (`climate.rs`). The world model grows as players explore: a cell turns from unknown
//! (impassable, so never grazed and holding no forage row) into known, which is harmless, and a
//! known cell changes only when a player rebuilds its ground. A grazed row then reads its past
//! sub-seasons with the new rates (X2-16).

use std::collections::HashMap;
use std::sync::Mutex;

use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_space::habitat::{CELL_BLOCKS as HABITAT_CELL, REGION_BLOCKS};
use rtsim_space::{Area, RegionKey, WorldHandle, WorldModel};

use super::climate::{Climate, ForageRates, Uniform};
use super::engine::NewHerd;
use super::forage::{CELL_BLOCKS, centre_of};
use super::species::{HerdSpecies, HerdSpeciesData};

const _: () = assert!(
    CELL_BLOCKS % HABITAT_CELL == 0,
    "forage cells are whole habitat cells"
);

/// How the herds read the world model. Every number is a TUNABLE PLACEHOLDER.
#[derive(Clone, Debug, PartialEq)]
pub struct HabitatTunables {
    /// Forage capacity of a forage cell of all grassland (vegetation 1), forage units: the
    /// uniform climate's 2,000 (Y3).
    pub capacity: f64,
    /// Growth per day at a mild temperature (the uniform climate's 0.1).
    pub growth_per_day: f64,
    /// The growth ramp (EQC503): full growth from `mild_from` to `hot_from` °C; from there
    /// growth falls linearly to `knee_share` of full at `cold_knee` and `hot_knee` (0 and 45 °C,
    /// where the first ramp reached nothing), then on to `end_share` at `cold_end` and `hot_end`,
    /// the ends of mic-climate's scale (-20 and 56 °C, X2-8); nothing grows only beyond them.
    /// So every biome on the scale grows something, and colder (or hotter) is always less.
    pub cold_end: f32,
    pub cold_knee: f32,
    pub mild_from: f32,
    pub hot_from: f32,
    pub hot_knee: f32,
    pub hot_end: f32,
    pub knee_share: f64,
    pub end_share: f64,
    /// Decay per day at 10 °C below `cold_end`, scaled by how far below it: frost takes standing
    /// forage only where nothing grows (EQC503).
    pub frost_decay_per_day: f64,
    /// A forage cell with more water than this share of its points is closed to herds.
    pub max_water_share: f64,
    /// Lava or another fluid: any of it closes the cell.
    pub max_fluid_share: f64,
    /// Cost added per share of water (fords, marsh) and of ice.
    pub water_cost: f32,
    pub ice_cost: f32,
    /// Blocks of relief (highest less lowest top) that add 1 to the crossing cost.
    pub relief_per_cost: f32,
    /// °C answered for ground nobody sampled (never used for a passable cell).
    pub unknown_celsius: f32,
}

impl Default for HabitatTunables {
    fn default() -> Self {
        HabitatTunables {
            capacity: 2_000.0,
            growth_per_day: 0.1,
            cold_end: -20.0,
            cold_knee: 0.0,
            mild_from: 8.0,
            hot_from: 30.0,
            hot_knee: 45.0,
            hot_end: 56.0,
            knee_share: 0.3,
            end_share: 0.25,
            frost_decay_per_day: 0.02,
            max_water_share: 0.5,
            max_fluid_share: 0.0,
            water_cost: 4.0,
            ice_cost: 1.0,
            relief_per_cost: 48.0,
            unknown_celsius: 15.0,
        }
    }
}

/// The herds' climate over the world model.
pub struct HabitatClimate {
    pub world: WorldHandle,
    pub tun: HabitatTunables,
    /// In a frame the world model knows nothing of, answer as the uniform grassland (Y3): herds
    /// founded there by an order (tests, tools) behave as before the world model (X2-6).
    pub uniform_unknown_frames: bool,
    /// What the world model said of each forage cell, at the model's revision: a herd's flow field
    /// reads the same cells many times (forage per sub-season, terrain, temperature). Cleared when
    /// the model changes.
    cache: Mutex<Cache>,
}

/// One forage cell as last read: `None` for a frame answered as the uniform grassland.
#[derive(Clone, Copy)]
struct Seen {
    area: Option<Area>,
    /// The season offsets by season of the year (spring to winter).
    seasons: [f32; 4],
}

#[derive(Default)]
struct Cache {
    revision: u64,
    cells: HashMap<(u16, i32, i32), Seen>,
}

/// Cells kept at most before the cache starts afresh (PLACEHOLDER).
const CACHE_CELLS: usize = 1 << 16;

impl HabitatClimate {
    pub fn new(world: WorldHandle) -> HabitatClimate {
        HabitatClimate {
            world,
            tun: HabitatTunables::default(),
            uniform_unknown_frames: true,
            cache: Mutex::new(Cache::default()),
        }
    }

    /// The forage cell (cx, cz) as the world model has it now, read once per model revision.
    fn seen(&self, frame: u16, cx: i32, cz: i32) -> Seen {
        let w = self.world.read();
        let rev = w.revision();
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if c.revision != rev || c.cells.len() >= CACHE_CELLS {
            c.revision = rev;
            c.cells.clear();
        }
        *c.cells.entry((frame, cx, cz)).or_insert_with(|| {
            if self.uniform_unknown_frames && !w.knows_frame(frame) {
                return Seen {
                    area: None,
                    seasons: [0.0; 4],
                };
            }
            let (x0, z0) = (cx * CELL_BLOCKS, cz * CELL_BLOCKS);
            Seen {
                area: Some(w.area(frame, x0, z0, x0 + CELL_BLOCKS, z0 + CELL_BLOCKS)),
                seasons: [0, 3, 6, 9].map(|s| w.season_offset(frame, s)),
            }
        })
    }

    /// Growth's share of full at `t` °C (the ramp of [`HabitatTunables::cold_end`]).
    pub fn growth_share(&self, t: f32) -> f64 {
        let k = &self.tun;
        // From share `a` at `x0` to share `b` at `x1`, at `t`.
        let lerp = |x0: f32, a: f64, x1: f32, b: f64| {
            let f = f64::from((t - x0) / (x1 - x0)).clamp(0.0, 1.0);
            a + (b - a) * f
        };
        let s = if t.is_nan() || t < k.cold_end || t > k.hot_end {
            0.0
        } else if t < k.cold_knee {
            lerp(k.cold_end, k.end_share, k.cold_knee, k.knee_share)
        } else if t < k.mild_from {
            lerp(k.cold_knee, k.knee_share, k.mild_from, 1.0)
        } else if t <= k.hot_from {
            1.0
        } else if t <= k.hot_knee {
            lerp(k.hot_from, 1.0, k.hot_knee, k.knee_share)
        } else {
            lerp(k.hot_knee, k.knee_share, k.hot_end, k.end_share)
        };
        s.clamp(0.0, 1.0)
    }

    /// The terrain cost of an area (`f32::INFINITY` when closed), by this climate's rule.
    pub fn terrain_of(&self, a: &Area) -> f32 {
        let k = &self.tun;
        if !a.fully_known() {
            return f32::INFINITY;
        }
        let n = f64::from(a.points.max(1));
        if a.water_share() > k.max_water_share || f64::from(a.fluid) / n > k.max_fluid_share {
            return f32::INFINITY;
        }
        let relief = (a.max_y - a.min_y).max(0) as f32;
        1.0 + k.water_cost * a.water_share() as f32
            + k.ice_cost * (f64::from(a.ice) / n) as f32
            + relief / k.relief_per_cost.max(1.0)
    }
}

impl Climate for HabitatClimate {
    fn forage(&self, frame: u16, cx: i32, cz: i32, sub: u32) -> ForageRates {
        let seen = self.seen(frame, cx, cz);
        let Some(a) = seen.area else {
            return Uniform.forage(frame, cx, cz, sub);
        };
        if a.points == 0 || a.land == 0 {
            return ForageRates {
                growth_per_day: 0.0,
                capacity: 0.0,
                decay_per_day: 0.0,
            };
        }
        let t = a.base_celsius + seen.seasons[((sub % 12) / 3) as usize];
        let capacity = self.tun.capacity * a.land_share() * f64::from(a.vegetation);
        let decay = if t < self.tun.cold_end {
            self.tun.frost_decay_per_day * f64::from(((self.tun.cold_end - t) / 10.0).min(1.0))
        } else {
            0.0
        };
        ForageRates {
            growth_per_day: if capacity > 0.0 {
                self.tun.growth_per_day * self.growth_share(t)
            } else {
                0.0
            },
            capacity,
            decay_per_day: decay,
        }
    }

    fn temperature(&self, frame: u16, cx: i32, cz: i32, sub: u32) -> f32 {
        let seen = self.seen(frame, cx, cz);
        let Some(a) = seen.area else {
            return Uniform.temperature(frame, cx, cz, sub);
        };
        if a.base_celsius.is_nan() {
            return self.tun.unknown_celsius;
        }
        a.base_celsius + seen.seasons[((sub % 12) / 3) as usize]
    }

    fn terrain(&self, frame: u16, cx: i32, cz: i32) -> f32 {
        match self.seen(frame, cx, cz).area {
            Some(a) => self.terrain_of(&a),
            None => Uniform.terrain(frame, cx, cz),
        }
    }
}

// ---- founding ----------------------------------------------------------------------------------

/// How herds are founded from the world model (Y4's recommendation). TUNABLE PLACEHOLDERS.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundingTunables {
    /// A region is judged once this share of its points is known (its "first sample", X2-11).
    pub min_known_share: f64,
    /// Herd habitat (in cells, `Creatures::habitat_cells` summed over the herd species plus
    /// the founders' weight) a region needs for a herd.
    pub min_habitat_cells: f64,
    /// Animals per cell of a species' habitat, by `HerdSpecies` index (sheep, cow, pig, horse,
    /// goat).
    pub density: [f64; HerdSpecies::COUNT],
    /// Habitat cells one chunk-generation animal is worth when drawing the species (Y20).
    pub founder_weight: f64,
    /// Herd animals at least per chunk-generation animal of the species (Y20).
    pub animals_per_founder: u64,
    /// Bounds on a founded herd's size.
    pub min_count: u64,
    pub max_count: u64,
    /// The share of its open cells' yearly yield ([`range_yield`]) a range's K may eat (EQC500):
    /// K is at most `forage_share x yield / intake`.
    pub forage_share: f64,
    /// Forage an animal eats a day, by species: the herds' diets
    /// ([`HerdSpeciesData::intake_per_day`]).
    pub intake: [f64; HerdSpecies::COUNT],
}

impl Default for FoundingTunables {
    fn default() -> Self {
        FoundingTunables {
            min_known_share: 0.25,
            min_habitat_cells: 2.0,
            density: [24.0, 10.0, 12.0, 6.0, 8.0],
            founder_weight: 0.5,
            animals_per_founder: 4,
            min_count: 8,
            max_count: 4_000,
            forage_share: 0.5,
            intake: HerdSpecies::ALL.map(|s| HerdSpeciesData::of(s).intake_per_day),
        }
    }
}

/// What founding decided for a region.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionPlan {
    /// Too little of it is known yet: judged later.
    NotYet,
    /// No herd: too little habitat, or no ground a herd could stand on. Final.
    Barren,
    Found(NewHerd),
}

/// A forage cell's sustained yield, forage units a day over the year (EQC500): the most a herd
/// can take from it, sub-season after sub-season, without running it down. In a sub-season the
/// cell follows `F' = g F - h F²` (`forage.rs`, g = growth - decay, h = growth / capacity); a
/// harvest H holds F still where `H = g F - h F²`, which is largest at `F = g / 2h` and gives
/// `g² / 4h` (with no decay, `growth x capacity / 4`: the logistic's maximum sustained harvest).
/// The yield is the mean of that over the year's twelve sub-seasons (of equal length). It is
/// exact when the capacity is the same all year and nothing decays, as `HabitatClimate`'s cells
/// are wherever anything grows, since one standing stock, half the capacity, is then best in
/// every sub-season; otherwise it is an upper bound.
pub fn cell_yield(climate: &dyn Climate, frame: u16, cx: i32, cz: i32) -> f64 {
    let subs = super::climate::SUB_SEASONS;
    let mut sum = 0.0;
    for s in 0..subs {
        let r = climate.forage(frame, cx, cz, s);
        if r.growth_per_day <= 0.0 || r.capacity <= 0.0 {
            continue;
        }
        let g = r.growth_per_day - r.decay_per_day;
        if g > 0.0 {
            sum += g * g * r.capacity / (4.0 * r.growth_per_day);
        }
    }
    sum / f64::from(subs)
}

/// What region `(rx, rz)`'s open forage cells (those a herd may enter: known in full, not water)
/// yield a day over the year: the sum of their [`cell_yield`]s.
pub fn range_yield(climate: &dyn Climate, frame: u16, rx: i32, rz: i32) -> f64 {
    let per = REGION_BLOCKS / CELL_BLOCKS;
    let mut sum = 0.0;
    for dz in 0..per {
        for dx in 0..per {
            let (cx, cz) = (rx * per + dx, rz * per + dz);
            if climate.terrain(frame, cx, cz).is_finite() {
                sum += cell_yield(climate, frame, cx, cz);
            }
        }
    }
    sum
}

/// The herd species of a body kind name (its vanilla entity id).
pub fn species_of_body(body: &str) -> Option<HerdSpecies> {
    HerdSpecies::ALL.into_iter().find(|s| s.entity_id() == body)
}

/// The stable name of a region's herd, so a founding is never repeated (X2-11).
pub fn region_herd_name(k: RegionKey) -> String {
    format!("herd of region {}:{}:{}", k.frame, k.rx, k.rz)
}

/// A region's open forage cells (those a herd may enter) and the herd species on them: the ground
/// a range's K counts (EQC501). Cells known only in part are closed to herds (X2-5), so they add
/// nothing until they are known in full.
struct OpenGround {
    /// Each open forage cell, in the region's row order, with each species' weight there
    /// (habitat cells plus the founders' weight).
    cells: Vec<((i32, i32), [f64; HerdSpecies::COUNT])>,
    habitat: [f64; HerdSpecies::COUNT],
    founders: [u64; HerdSpecies::COUNT],
    weights: [f64; HerdSpecies::COUNT],
    /// The open cells' [`range_yield`].
    yields: f64,
}

fn open_ground(
    world: &dyn WorldModel,
    climate: &HabitatClimate,
    k: RegionKey,
    tun: &FoundingTunables,
) -> OpenGround {
    let mut g = OpenGround {
        cells: Vec::new(),
        habitat: [0.0; HerdSpecies::COUNT],
        founders: [0; HerdSpecies::COUNT],
        weights: [0.0; HerdSpecies::COUNT],
        yields: 0.0,
    };
    let per = REGION_BLOCKS / CELL_BLOCKS;
    let (x0, z0, _, _) = k.rect();
    let (fx0, fz0) = (x0.div_euclid(CELL_BLOCKS), z0.div_euclid(CELL_BLOCKS));
    for dz in 0..per {
        for dx in 0..per {
            let c = (fx0 + dx, fz0 + dz);
            if !climate.terrain(k.frame, c.0, c.1).is_finite() {
                continue;
            }
            g.yields += cell_yield(climate, k.frame, c.0, c.1);
            let (bx, bz) = (c.0 * CELL_BLOCKS, c.1 * CELL_BLOCKS);
            let mut here = [0.0f64; HerdSpecies::COUNT];
            for cr in world.creatures(k.frame, bx, bz, bx + CELL_BLOCKS, bz + CELL_BLOCKS) {
                if let Some(s) = species_of_body(&cr.body) {
                    let w = cr.habitat_cells + tun.founder_weight * f64::from(cr.generated);
                    g.habitat[s.index()] += cr.habitat_cells;
                    g.founders[s.index()] += u64::from(cr.generated);
                    g.weights[s.index()] += w;
                    here[s.index()] += w;
                }
            }
            g.cells.push((c, here));
        }
    }
    g
}

/// A range's K for `species` on ground `g` (EQUILIBRIUM.md §2.4.2's carrying capacity; EQC500,
/// EQC501): the founding density times the species' habitat on the open cells (at least
/// `animals_per_founder` a chunk-generation animal there), within the count bounds, and never
/// more than the open cells' yearly yield feeds, `floor(forage_share x yield / intake)`. `None`
/// when that yield cannot feed `min_count` animals.
fn capacity(g: &OpenGround, species: HerdSpecies, tun: &FoundingTunables) -> Option<u64> {
    let i = species.index();
    let fed = (tun.forage_share * g.yields / tun.intake[i].max(1e-9)).floor();
    if !fed.is_finite() || fed < tun.min_count as f64 {
        return None;
    }
    let by_habitat = ((tun.density[i] * g.habitat[i]).round() as u64)
        .max(g.founders[i] * tun.animals_per_founder)
        .clamp(tun.min_count, tun.max_count);
    Some(by_habitat.min(fed as u64))
}

/// The K a founded range of `species` on region `k` has on the ground as it is now (EQC504): what
/// founding would give it, and `min_count` when its open cells can no longer feed that many. The
/// world model re-caps a range with it whenever the region's cells change.
pub fn range_capacity(
    world: &dyn WorldModel,
    climate: &HabitatClimate,
    k: RegionKey,
    species: HerdSpecies,
    tun: &FoundingTunables,
) -> u64 {
    capacity(&open_ground(world, climate, k, tun), species, tun).unwrap_or(tun.min_count)
}

/// Judges region `k` for a herd: once enough of it is known, one herd if its open ground supports
/// one, the species drawn by the habitat there (`seed` and the region's id give the draw), placed
/// on the open forage cell with the most of the species' habitat, its count the range's K
/// ([`capacity`]).
pub fn plan_region(
    world: &dyn WorldModel,
    climate: &HabitatClimate,
    k: RegionKey,
    seed: u64,
    tun: &FoundingTunables,
) -> RegionPlan {
    let (x0, z0, x1, z1) = k.rect();
    let a = world.area(k.frame, x0, z0, x1, z1);
    if a.known_share() < tun.min_known_share {
        return RegionPlan::NotYet;
    }
    let complete = a.fully_known();
    let not_now = if complete {
        RegionPlan::Barren
    } else {
        RegionPlan::NotYet
    };
    let g = open_ground(world, climate, k, tun);
    let total: f64 = g.weights.iter().sum();
    if total < tun.min_habitat_cells {
        return not_now;
    }
    let id = rtsim_space::region_page_id(k);
    let mut rng = StreamKey::new(seed, SystemId::named("herds.founding"), id).slot(0);
    let mut pick = rng.next_f64() * total;
    let mut species = HerdSpecies::ALL[0];
    for s in HerdSpecies::ALL {
        if g.weights[s.index()] <= 0.0 {
            continue;
        }
        species = s;
        if pick < g.weights[s.index()] {
            break;
        }
        pick -= g.weights[s.index()];
    }
    // The open forage cell with the most of the species' habitat (the first of equals).
    let mut best: Option<(f64, (i32, i32))> = None;
    for &(c, here) in &g.cells {
        let h = here[species.index()];
        if best.is_none_or(|(b, _)| h > b) {
            best = Some((h, c));
        }
    }
    let Some((_, cell)) = best else {
        return not_now;
    };
    let Some(count) = capacity(&g, species, tun) else {
        return not_now;
    };
    RegionPlan::Found(NewHerd {
        name: region_herd_name(k),
        species,
        frame: k.frame,
        pos: centre_of(cell),
        count,
        seed: Some(rng.next_u64()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_space::SampledWorld;
    use rtsim_space::synth::{Painter, land, sea};

    fn world(paint: &dyn Fn(&mut Painter, &mut SampledWorld)) -> WorldHandle {
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        paint(&mut p, &mut w);
        WorldHandle::new(Box::new(w))
    }

    #[test]
    fn water_and_unknown_ground_are_closed_and_land_is_open() {
        let h = world(&|p, w| {
            p.paint(w, 0, (0, 0, 512, 256), &|x, _| {
                if x < 256 {
                    land("minecraft:plains")
                } else {
                    sea()
                }
            })
        });
        let c = HabitatClimate::new(h);
        assert!(c.terrain(0, 0, 0).is_finite());
        assert_eq!(c.terrain(0, 2, 0), f32::INFINITY, "sea");
        assert_eq!(c.terrain(0, 0, 5), f32::INFINITY, "unknown");
        let f = c.forage(0, 0, 0, 3);
        assert_eq!(f.capacity, 2_000.0);
        assert!(f.growth_per_day > 0.0);
        assert_eq!(c.forage(0, 2, 0, 3).capacity, 0.0);
        assert!((c.temperature(0, 0, 0, 0) - 19.52).abs() < 1e-3);
    }

    #[test]
    fn a_plains_region_founds_one_herd_of_a_plains_species_on_land() {
        let h = world(&|p, w| {
            p.paint(w, 0, (0, 0, 512, 512), &|x, z| {
                if x + z < 300 {
                    sea()
                } else {
                    land("minecraft:plains")
                }
            })
        });
        let c = HabitatClimate::new(h.clone());
        let k = RegionKey {
            frame: 0,
            rx: 0,
            rz: 0,
        };
        let w = h.read();
        let RegionPlan::Found(n) = plan_region(&**w, &c, k, 7, &FoundingTunables::default()) else {
            panic!("a plains region founds a herd");
        };
        assert!(matches!(
            n.species,
            HerdSpecies::Sheep | HerdSpecies::Pig | HerdSpecies::Cow | HerdSpecies::Horse
        ));
        assert!(n.count >= 8);
        let f = super::super::forage::cell_of(i64::from(n.pos.0), i64::from(n.pos.1));
        assert!(c.terrain(0, f.0, f.1).is_finite(), "founded on open ground");
        // The same region and seed always found the same herd.
        assert_eq!(
            plan_region(&**w, &c, k, 7, &FoundingTunables::default()),
            RegionPlan::Found(n)
        );
        // An all-sea region is barren; an unknown one is not yet judged.
        drop(w);
        let sea_world = world(&|p, w| p.paint(w, 0, (0, 0, 512, 512), &|_, _| sea()));
        let cs = HabitatClimate::new(sea_world.clone());
        assert_eq!(
            plan_region(&**sea_world.read(), &cs, k, 7, &FoundingTunables::default()),
            RegionPlan::Barren
        );
        let k2 = RegionKey {
            frame: 0,
            rx: 5,
            rz: 5,
        };
        assert_eq!(
            plan_region(
                &**sea_world.read(),
                &cs,
                k2,
                7,
                &FoundingTunables::default()
            ),
            RegionPlan::NotYet
        );
    }

    #[test]
    fn species_follow_the_biomes_spawn_data() {
        // Snowy slopes spawn goats and nothing else of the herd species.
        let h = world(&|p, w| {
            p.paint(w, 0, (0, 0, 512, 512), &|_, _| {
                land("minecraft:snowy_slopes")
            })
        });
        let c = HabitatClimate::new(h.clone());
        for rx in 0..1 {
            let k = RegionKey {
                frame: 0,
                rx,
                rz: 0,
            };
            let RegionPlan::Found(n) =
                plan_region(&**h.read(), &c, k, 3, &FoundingTunables::default())
            else {
                panic!("goats");
            };
            assert_eq!(n.species, HerdSpecies::Goat);
        }
        // A desert spawns none: barren.
        let d = world(&|p, w| p.paint(w, 0, (0, 0, 512, 512), &|_, _| land("minecraft:desert")));
        let cd = HabitatClimate::new(d.clone());
        assert_eq!(
            plan_region(
                &**d.read(),
                &cd,
                RegionKey {
                    frame: 0,
                    rx: 0,
                    rz: 0
                },
                3,
                &FoundingTunables::default()
            ),
            RegionPlan::Barren
        );
    }

    #[test]
    fn chunk_generation_animals_weigh_the_draw_and_floor_the_count() {
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        // A forest region (sheep, pig and cow habitat) where vanilla spawned 300 cows.
        p.paint(&mut w, 0, (0, 0, 512, 512), &|_, _| {
            land("minecraft:forest")
        });
        let r = p.record(
            0,
            0,
            0,
            &|_, _| land("minecraft:forest"),
            &[("minecraft:cow", 300)],
        );
        p.feed(&mut w, &r);
        let h = WorldHandle::new(Box::new(w));
        let c = HabitatClimate::new(h.clone());
        let RegionPlan::Found(n) = plan_region(
            &**h.read(),
            &c,
            RegionKey {
                frame: 0,
                rx: 0,
                rz: 0,
            },
            11,
            &FoundingTunables::default(),
        ) else {
            panic!("a forest region founds a herd");
        };
        assert_eq!(
            n.species,
            HerdSpecies::Cow,
            "150 cells of founders outweigh the rest"
        );
        // Four animals a founder would be 1,200 cows; half the forest's yield feeds 60 (EQC500).
        let yields = range_yield(&c, 0, 0, 0);
        assert!((yields - 480.0).abs() < 1e-3, "{yields}");
        assert_eq!(n.count, 60, "the forage caps the founders' floor");

        // On plains, 15 founders floor a horse herd at 60, above its 42 by habitat and under
        // the 80 half its forage feeds.
        let mut w = SampledWorld::new();
        let mut p = Painter::default();
        let r = p.record(
            0,
            0,
            0,
            &|_, _| land("minecraft:plains"),
            &[("minecraft:horse", 15)],
        );
        p.feed(&mut w, &r);
        p.paint(&mut w, 0, (64, 0, 512, 512), &|_, _| {
            land("minecraft:plains")
        });
        p.paint(&mut w, 0, (0, 64, 64, 512), &|_, _| {
            land("minecraft:plains")
        });
        let h = WorldHandle::new(Box::new(w));
        let c = HabitatClimate::new(h.clone());
        let k = RegionKey {
            frame: 0,
            rx: 0,
            rz: 0,
        };
        let horses: Vec<u64> = (0..64)
            .filter_map(|seed| {
                match plan_region(&**h.read(), &c, k, seed, &FoundingTunables::default()) {
                    RegionPlan::Found(n) if n.species == HerdSpecies::Horse => Some(n.count),
                    _ => None,
                }
            })
            .collect();
        assert!(!horses.is_empty(), "some draw founds horses");
        assert!(horses.iter().all(|&n| n == 60), "{horses:?}");
    }

    /// The ramp (EQC503): something grows everywhere on mic-climate's scale (-20 to 56 °C),
    /// nothing beyond it, full growth from 8 to 30 °C, and less the further from that.
    #[test]
    fn the_growth_ramp_reaches_the_scales_ends() {
        let c = HabitatClimate::new(WorldHandle::sampled());
        let g = |t: f32| c.growth_share(t);
        assert_eq!((g(-20.5), g(56.5), g(f32::NAN)), (0.0, 0.0, 0.0));
        assert!((g(-20.0) - 0.25).abs() < 1e-9 && (g(56.0) - 0.25).abs() < 1e-9);
        assert!((g(0.0) - 0.3).abs() < 1e-9 && (g(45.0) - 0.3).abs() < 1e-9);
        assert_eq!((g(8.0), g(19.5), g(30.0)), (1.0, 1.0, 1.0));
        let mut last = 0.0;
        for t in -200..=80 {
            let s = g(t as f32 / 10.0);
            assert!(s >= last, "colder is less: {} at {} °C", s, t as f32 / 10.0);
            last = s;
        }
        for t in 300..=560 {
            let (a, b) = (g(t as f32 / 10.0), g((t + 1) as f32 / 10.0));
            assert!(
                b <= a,
                "hotter is less: {b} at {} °C",
                (t + 1) as f32 / 10.0
            );
        }
        // Vanilla's ends land on the scale's: savanna (2.0) at 56 °C, frozen peaks (-0.7) at -20.
        assert!(g(rtsim_space::model::vanilla_celsius(2.0)) > 0.0);
        assert!(g(rtsim_space::model::vanilla_celsius(-0.7)) > 0.0);
    }

    /// Ben's answer (EQC500): founding caps a range's K at what its open forage cells yield, so
    /// K is the range's carrying capacity (EQUILIBRIUM.md §2.4.2) in every biome that founds a
    /// herd, the hot and the frozen ones included.
    #[test]
    fn a_founded_range_is_fed_by_its_own_forage() {
        let k = RegionKey {
            frame: 0,
            rx: 0,
            rz: 0,
        };
        for biome in [
            "minecraft:plains",
            "minecraft:forest",
            "minecraft:savanna",
            "minecraft:snowy_slopes",
        ] {
            let h = world(&|p, w| p.paint(w, 0, (0, 0, 512, 512), &|_, _| land(biome)));
            let c = HabitatClimate::new(h.clone());
            let yields = range_yield(&c, 0, 0, 0);
            for seed in 0..16 {
                let RegionPlan::Found(n) =
                    plan_region(&**h.read(), &c, k, seed, &FoundingTunables::default())
                else {
                    panic!("{biome} founds a herd (seed {seed})");
                };
                let need = n.count as f64
                    * super::super::species::HerdSpeciesData::of(n.species).intake_per_day;
                assert!(
                    need <= yields,
                    "{biome}, seed {seed}: {} {:?} eat {need:.0} a day; the range yields {yields:.0}",
                    n.count,
                    n.species
                );
            }
        }
    }

    /// A region judged while partly explored (EQC501): its K counts only the forage cells a herd
    /// may enter (known in full), so the known cells around them add nothing.
    #[test]
    fn a_partly_explored_regions_range_gets_k_from_its_open_cells_only() {
        let k = RegionKey {
            frame: 0,
            rx: 0,
            rz: 0,
        };
        let paint = |cells: &[(i32, i32)]| {
            let mut w = SampledWorld::new();
            let mut p = Painter::default();
            for &(cx, cz) in cells {
                let r = p.record(0, cx, cz, &|_, _| land("minecraft:plains"), &[]);
                p.feed(&mut w, &r);
            }
            WorldHandle::new(Box::new(w))
        };
        // Four forage cells known in full: habitat cells 0..4 x 0..4.
        let open: Vec<(i32, i32)> = (0..4).flat_map(|z| (0..4).map(move |x| (x, z))).collect();
        // The same, and one habitat cell in each of eight other forage cells (each 1 of its 4).
        let mut ragged = open.clone();
        ragged.extend([
            (4, 0),
            (4, 2),
            (6, 0),
            (6, 2),
            (0, 4),
            (2, 4),
            (0, 6),
            (2, 6),
        ]);
        let (a, b) = (paint(&open), paint(&ragged));
        let (ca, cb) = (
            HabitatClimate::new(a.clone()),
            HabitatClimate::new(b.clone()),
        );
        let open_cells = |c: &HabitatClimate| {
            (0..4)
                .flat_map(|z| (0..4).map(move |x| (x, z)))
                .filter(|&(x, z)| c.terrain(0, x, z).is_finite())
                .count()
        };
        assert_eq!(
            (open_cells(&ca), open_cells(&cb)),
            (4, 4),
            "four open cells each"
        );
        for seed in 0..8 {
            let pa = plan_region(&**a.read(), &ca, k, seed, &FoundingTunables::default());
            let pb = plan_region(&**b.read(), &cb, k, seed, &FoundingTunables::default());
            assert!(matches!(pa, RegionPlan::Found(_)), "{pa:?}");
            assert_eq!(
                pb, pa,
                "seed {seed}: the cells a herd cannot enter add nothing to K"
            );
        }
    }
}
