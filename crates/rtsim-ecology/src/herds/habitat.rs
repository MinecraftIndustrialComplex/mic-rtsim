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
//!   vanilla's chunk-generation animals counted as founders (Y20's recommendation, X2-13).
//!
//! **Periodic in the year?** The herds' forage closed form assumes the climate repeats every
//! year (`climate.rs`). The world model grows as players explore: a cell turns from unknown
//! (impassable, so never grazed and holding no forage row) into known, which is harmless, and a
//! known cell changes only when a player rebuilds its ground. A grazed row then reads its past
//! sub-seasons with the new rates (X2-16).

use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_space::habitat::{CELL_BLOCKS as HABITAT_CELL, REGION_BLOCKS};
use rtsim_space::{Area, RegionKey, WorldHandle, WorldModel};

use super::climate::{Climate, ForageRates, Uniform};
use super::engine::NewHerd;
use super::forage::{CELL_BLOCKS, centre_of};
use super::species::HerdSpecies;

const _: () = assert!(CELL_BLOCKS % HABITAT_CELL == 0, "forage cells are whole habitat cells");

/// How the herds read the world model. Every number is a TUNABLE PLACEHOLDER.
#[derive(Clone, Debug, PartialEq)]
pub struct HabitatTunables {
    /// Forage capacity of a forage cell of all grassland (vegetation 1), forage units: the
    /// uniform climate's 2,000 (Y3).
    pub capacity: f64,
    /// Growth per day at a mild temperature (the uniform climate's 0.1).
    pub growth_per_day: f64,
    /// °C below which nothing grows, and from which growth ramps to full at `mild_from`.
    pub frost: f32,
    pub mild_from: f32,
    /// °C above which growth falls, reaching nothing at `scorch`.
    pub hot_from: f32,
    pub scorch: f32,
    /// Decay per day at 10 °C of frost, scaled by how far below freezing.
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
            frost: 0.0,
            mild_from: 8.0,
            hot_from: 30.0,
            scorch: 45.0,
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
}

impl HabitatClimate {
    pub fn new(world: WorldHandle) -> HabitatClimate {
        HabitatClimate {
            world,
            tun: HabitatTunables::default(),
            uniform_unknown_frames: true,
        }
    }

    /// Whether `frame` answers as the uniform grassland.
    fn uniform(&self, frame: u16) -> bool {
        self.uniform_unknown_frames && !self.world.read().knows_frame(frame)
    }

    fn area(&self, frame: u16, cx: i32, cz: i32) -> Area {
        let (x0, z0) = (cx * CELL_BLOCKS, cz * CELL_BLOCKS);
        self.world
            .read()
            .area(frame, x0, z0, x0 + CELL_BLOCKS, z0 + CELL_BLOCKS)
    }

    /// Growth's share of full at `t` °C.
    fn growth_share(&self, t: f32) -> f64 {
        let k = &self.tun;
        let s = if t <= k.frost || t >= k.scorch {
            0.0
        } else if t < k.mild_from {
            (t - k.frost) / (k.mild_from - k.frost)
        } else if t > k.hot_from {
            (k.scorch - t) / (k.scorch - k.hot_from)
        } else {
            1.0
        };
        f64::from(s.clamp(0.0, 1.0))
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
        if self.uniform(frame) {
            return Uniform.forage(frame, cx, cz, sub);
        }
        let a = self.area(frame, cx, cz);
        if a.points == 0 || a.land == 0 {
            return ForageRates {
                growth_per_day: 0.0,
                capacity: 0.0,
                decay_per_day: 0.0,
            };
        }
        let t = a.base_celsius + self.world.read().season_offset(frame, sub);
        let capacity = self.tun.capacity * a.land_share() * f64::from(a.vegetation);
        let decay = if t < self.tun.frost {
            self.tun.frost_decay_per_day * f64::from(((self.tun.frost - t) / 10.0).min(1.0))
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
        if self.uniform(frame) {
            return Uniform.temperature(frame, cx, cz, sub);
        }
        let a = self.area(frame, cx, cz);
        if a.base_celsius.is_nan() {
            return self.tun.unknown_celsius;
        }
        a.base_celsius + self.world.read().season_offset(frame, sub)
    }

    fn terrain(&self, frame: u16, cx: i32, cz: i32) -> f32 {
        if self.uniform(frame) {
            return Uniform.terrain(frame, cx, cz);
        }
        let a = self.area(frame, cx, cz);
        self.terrain_of(&a)
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

/// The herd species of a body kind name (its vanilla entity id).
pub fn species_of_body(body: &str) -> Option<HerdSpecies> {
    HerdSpecies::ALL.into_iter().find(|s| s.entity_id() == body)
}

/// The stable name of a region's herd, so a founding is never repeated (X2-11).
pub fn region_herd_name(k: RegionKey) -> String {
    format!("herd of region {}:{}:{}", k.frame, k.rx, k.rz)
}

/// Judges region `k` for a herd: once enough of it is known, one herd if the ground supports
/// one, the species drawn by habitat (`seed` and the region's id give the draw), placed on the
/// most suitable forage cell a herd may stand on.
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
    let mut weights = [0.0f64; HerdSpecies::COUNT];
    let mut founders = [0u64; HerdSpecies::COUNT];
    let mut habitat = [0.0f64; HerdSpecies::COUNT];
    for c in world.creatures(k.frame, x0, z0, x1, z1) {
        if let Some(s) = species_of_body(&c.body) {
            habitat[s.index()] += c.habitat_cells;
            founders[s.index()] += u64::from(c.generated);
            weights[s.index()] += c.habitat_cells + tun.founder_weight * f64::from(c.generated);
        }
    }
    let total: f64 = weights.iter().sum();
    let complete = a.fully_known();
    if total < tun.min_habitat_cells {
        return if complete {
            RegionPlan::Barren
        } else {
            RegionPlan::NotYet
        };
    }
    let id = rtsim_space::region_page_id(k);
    let mut rng = StreamKey::new(seed, SystemId::named("herds.founding"), id).slot(0);
    let mut pick = rng.next_f64() * total;
    let mut species = HerdSpecies::ALL[0];
    for s in HerdSpecies::ALL {
        if weights[s.index()] <= 0.0 {
            continue;
        }
        species = s;
        if pick < weights[s.index()] {
            break;
        }
        pick -= weights[s.index()];
    }
    // The forage cell of the region a herd may stand on with the most of the species' habitat.
    let per = REGION_BLOCKS / CELL_BLOCKS;
    let (fx0, fz0) = (x0.div_euclid(CELL_BLOCKS), z0.div_euclid(CELL_BLOCKS));
    let mut best: Option<(f64, (i32, i32))> = None;
    for dz in 0..per {
        for dx in 0..per {
            let c = (fx0 + dx, fz0 + dz);
            if !climate.terrain(k.frame, c.0, c.1).is_finite() {
                continue;
            }
            let (bx, bz) = (c.0 * CELL_BLOCKS, c.1 * CELL_BLOCKS);
            let h: f64 = world
                .creatures(k.frame, bx, bz, bx + CELL_BLOCKS, bz + CELL_BLOCKS)
                .iter()
                .filter(|cr| species_of_body(&cr.body) == Some(species))
                .map(|cr| cr.habitat_cells + tun.founder_weight * f64::from(cr.generated))
                .sum();
            if best.is_none_or(|(b, _)| h > b) {
                best = Some((h, c));
            }
        }
    }
    let Some((_, cell)) = best else {
        return if complete {
            RegionPlan::Barren
        } else {
            RegionPlan::NotYet
        };
    };
    let i = species.index();
    let count = ((tun.density[i] * habitat[i]).round() as u64)
        .max(founders[i] * tun.animals_per_founder)
        .clamp(tun.min_count, tun.max_count);
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
                if x < 256 { land("minecraft:plains") } else { sea() }
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
                if x + z < 300 { sea() } else { land("minecraft:plains") }
            })
        });
        let c = HabitatClimate::new(h.clone());
        let k = RegionKey { frame: 0, rx: 0, rz: 0 };
        let w = h.read();
        let RegionPlan::Found(n) = plan_region(&**w, &c, k, 7, &FoundingTunables::default())
        else {
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
        let k2 = RegionKey { frame: 0, rx: 5, rz: 5 };
        assert_eq!(
            plan_region(&**sea_world.read(), &cs, k2, 7, &FoundingTunables::default()),
            RegionPlan::NotYet
        );
    }

    #[test]
    fn species_follow_the_biomes_spawn_data() {
        // Snowy slopes spawn goats and nothing else of the herd species.
        let h = world(&|p, w| {
            p.paint(w, 0, (0, 0, 512, 512), &|_, _| land("minecraft:snowy_slopes"))
        });
        let c = HabitatClimate::new(h.clone());
        for rx in 0..1 {
            let k = RegionKey { frame: 0, rx, rz: 0 };
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
                RegionKey { frame: 0, rx: 0, rz: 0 },
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
        p.paint(&mut w, 0, (0, 0, 512, 512), &|_, _| land("minecraft:forest"));
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
            RegionKey { frame: 0, rx: 0, rz: 0 },
            11,
            &FoundingTunables::default(),
        ) else {
            panic!("a forest region founds a herd");
        };
        assert_eq!(n.species, HerdSpecies::Cow, "150 cells of founders outweigh the rest");
        assert_eq!(n.count, 1_200, "four animals a founder");
    }
}
