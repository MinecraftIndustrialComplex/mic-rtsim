// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ice and Fire's lairs as the pack's jar places them, for the scenarios and the S11 every-kind world
//! (moved from `rtsim-cli`'s `sim::equilibrium` so the host's world can use them, slice S11 EQC495).

use std::ops::RangeInclusive;

use rtsim_kernel::rng::{StreamKey, SystemId};

use super::LairSpec;
use crate::dragon::LairKind;
use crate::species::{Gender, Species};

/// Where the jar's lairs may stand on a ranges world, which has no biomes: a region holds a
/// biome some dragon's structure tag names with chance `region_share`, and in such a region each
/// candidate chunk's surface biome is in the tag with chance `cell_share`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LairLayout {
    pub region_share: f64,
    pub cell_share: f64,
}

/// The server's world's lair density (the dragon food gate, docs/overnight-choices-equilibrium.md
/// EQC539): a region holds a biome some dragon's tag names with chance 0.42, and in it a candidate
/// chunk's with chance 0.51, which gives the 0.5 lairs a region of the 2026-10-21 snapshot (0.48
/// to 0.53 outside the spawn disk, 30% of its regions holding one, 1.8 where any).
pub const REAL_LAIRS: LairLayout = LairLayout {
    region_share: 0.42,
    cell_share: 0.51,
};

/// Ice and Fire's lairs as the pack's jar places them (`iceandfire-2.1-beta.1`, read with CFR,
/// on vanilla 1.21.1's `ChunkGenerator.createStructures`) over regions `rx` x `rz`, under `layout`'s biomes or, where `region` is given, in the regions it
/// names (a painted world's biomes):
///
/// - two `random_spread` sets, roosts (spacing 15 chunks, separation 6) and caves (18, 12), one
///   candidate chunk per grid cell, its offset uniform over `spacing - separation` per axis;
/// - a candidate within 10 chunks (Chebyshev) of a `minecraft:villages` candidate (spacing 34,
///   separation 8) is excluded, whatever stands there;
/// - one draw per candidate, `nextDouble() < 0.5` (the pack's `generate*Chance`): every structure
///   of the set reads the same stream (`Structure.GenerationContext.makeRandom`), so a biome two
///   tags hold does not raise it;
/// - none within 1,000 blocks of spawn (`dangerousDistanceLimit`), spawn at the origin.
///
/// Each lair holds one dragon, as the structure spawns it: a roost's 52 to 59 days old, a cave's
/// 75 to 124, hunger 50; species and sex at random.
pub fn jar_lairs(
    rx: RangeInclusive<i32>,
    rz: RangeInclusive<i32>,
    seed: u64,
    layout: LairLayout,
    region: Option<&dyn Fn(i32, i32) -> bool>,
) -> Vec<LairSpec> {
    let key = StreamKey::new(seed, SystemId::named("eq.jar_lairs"), 0);
    let cell = |set: u64, i: i32, j: i32| {
        (set << 60) | (u64::from(i as u32 & 0x3FFF_FFFF) << 30) | u64::from(j as u32 & 0x3FFF_FFFF)
    };
    let village = |i: i32, j: i32| {
        let mut r = key.slot(cell(0, i, j));
        (i * 34 + r.below(26) as i32, j * 34 + r.below(26) as i32)
    };
    let excluded = |cx: i32, cz: i32| {
        ((cx - 10).div_euclid(34)..=(cx + 10).div_euclid(34)).any(|i| {
            ((cz - 10).div_euclid(34)..=(cz + 10).div_euclid(34)).any(|j| {
                let (vx, vz) = village(i, j);
                (vx - cx).abs() <= 10 && (vz - cz).abs() <= 10
            })
        })
    };
    // A painted world names its biomes; a ranges world has none, so the layout's share stands in.
    let region_ok = |x: i32, z: i32| match region {
        Some(f) => f(x, z),
        None => key.slot(cell(3, x, z)).bernoulli(layout.region_share),
    };
    let (c0x, c1x) = (rx.start() * 32, (rx.end() + 1) * 32);
    let (c0z, c1z) = (rz.start() * 32, (rz.end() + 1) * 32);
    let mut out = Vec::new();
    for (set, kind, spacing, sep) in [(1, LairKind::Roost, 15, 6), (2, LairKind::Cave, 18, 12)] {
        for i in c0x.div_euclid(spacing)..=(c1x - 1).div_euclid(spacing) {
            for j in c0z.div_euclid(spacing)..=(c1z - 1).div_euclid(spacing) {
                let mut r = key.slot(cell(set, i, j));
                let cx = i * spacing + r.below((spacing - sep) as u64) as i32;
                let cz = j * spacing + r.below((spacing - sep) as u64) as i32;
                if !(c0x..c1x).contains(&cx) || !(c0z..c1z).contains(&cz) {
                    continue;
                }
                let (x, z) = (cx * 16 + 8, cz * 16 + 8);
                let roll = r.bernoulli(0.5);
                let biome = region_ok(x.div_euclid(512), z.div_euclid(512))
                    && r.bernoulli(layout.cell_share);
                let far = f64::from(x).hypot(f64::from(z)) >= 1000.0;
                if !roll || !biome || !far || excluded(cx, cz) {
                    continue;
                }
                let species = Species::ALL[r.below(3) as usize];
                let gender = if r.bernoulli(0.5) {
                    Gender::Male
                } else {
                    Gender::Female
                };
                let (age, y) = match kind {
                    LairKind::Roost => (52 + r.below(8), 100),
                    LairKind::Cave => (75 + r.below(50), 40),
                };
                out.push(LairSpec {
                    key: 1_000 + out.len() as u64,
                    name: "jar lair",
                    species,
                    kind,
                    x,
                    z,
                    y,
                    dragons: vec![("jar dragon", gender, age, 50)],
                });
            }
        }
    }
    out
}
