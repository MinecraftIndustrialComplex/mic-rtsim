// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where sea ships may sail: a water mask over coarse cells, and paths over it.
//!
//! The engine knows no terrain, and which mod provides ports is open (P8), so the world model is
//! abstract: whoever builds the network passes a [`WaterMask`]. In play it would come from the
//! bridge (biome or heightmap samples taken as chunks generate, or Deep Time's bake where present:
//! not built, overnight choice T9); tests and the CLI use [`SynthCoast`].
//!
//! A lane is found by A* over the mask's cells (8-connected, octile costs), then pulled taut: each
//! corner is kept only when the straight line past it would cross land. The search is bounded
//! (PLACEHOLDER), so an unreachable port costs a fixed amount, once, when the network is built.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// Which cells are open water. Cells are `cell()` blocks on a side, cell (cx, cz) covering blocks
/// `cx * cell .. (cx + 1) * cell`.
pub trait WaterMask {
    fn cell(&self) -> i32;
    fn is_water(&self, frame: u16, cx: i32, cz: i32) -> bool;
}

/// Everything is water (an ocean world, or a frame with no mask yet).
pub struct AllWater(pub i32);

impl WaterMask for AllWater {
    fn cell(&self) -> i32 {
        self.0.max(1)
    }
    fn is_water(&self, _: u16, _: i32, _: i32) -> bool {
        true
    }
}

/// A synthetic coast for tests and the CLI (scaffolding): land north of a wavy shore at
/// `z = shore_z + amp · sin(x / wavelength)`, sea south of it, with round islands of land and
/// round lakes of water carved into either. Only frame 0 has a sea.
#[derive(Clone, Debug)]
pub struct SynthCoast {
    pub cell: i32,
    pub shore_z: i32,
    pub amp: i32,
    pub wavelength: i32,
    /// (x, z, radius) discs of land in the sea.
    pub islands: Vec<(i32, i32, i32)>,
    /// (x, z, radius) discs of water on land (bays, lagoons).
    pub bays: Vec<(i32, i32, i32)>,
}

impl SynthCoast {
    fn in_disc(x: i32, z: i32, d: &(i32, i32, i32)) -> bool {
        let (dx, dz) = (i64::from(x - d.0), i64::from(z - d.1));
        dx * dx + dz * dz <= i64::from(d.2) * i64::from(d.2)
    }

    /// Whether block (x, z) is water.
    pub fn water_at(&self, x: i32, z: i32) -> bool {
        if self.bays.iter().any(|b| Self::in_disc(x, z, b)) {
            return true;
        }
        if self.islands.iter().any(|i| Self::in_disc(x, z, i)) {
            return false;
        }
        let shore = f64::from(self.shore_z)
            + f64::from(self.amp) * (f64::from(x) / f64::from(self.wavelength.max(1))).sin();
        f64::from(z) > shore
    }
}

impl WaterMask for SynthCoast {
    fn cell(&self) -> i32 {
        self.cell
    }
    fn is_water(&self, frame: u16, cx: i32, cz: i32) -> bool {
        frame == 0
            && self.water_at(
                cx * self.cell + self.cell / 2,
                cz * self.cell + self.cell / 2,
            )
    }
}

/// The cell holding block (x, z).
pub fn cell_of(mask: &dyn WaterMask, x: i32, z: i32) -> (i32, i32) {
    let c = mask.cell();
    (x.div_euclid(c), z.div_euclid(c))
}

/// The centre of a cell, in blocks.
pub fn centre(mask: &dyn WaterMask, c: (i32, i32)) -> (i32, i32) {
    let s = mask.cell();
    (c.0 * s + s / 2, c.1 * s + s / 2)
}

/// The water cell nearest block (x, z), searched in rings out to `radius_cells`; ties go to the
/// first in ring order, so the answer is deterministic.
pub fn snap_to_water(
    mask: &dyn WaterMask,
    frame: u16,
    x: i32,
    z: i32,
    radius_cells: i32,
) -> Option<(i32, i32)> {
    let c = cell_of(mask, x, z);
    for r in 0..=radius_cells {
        type Best = Option<((i64, i32, i32), (i32, i32))>;
        let mut best: Best = None;
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let cc = (c.0 + dx, c.1 + dz);
                if mask.is_water(frame, cc.0, cc.1) {
                    let d2 = i64::from(dx) * i64::from(dx) + i64::from(dz) * i64::from(dz);
                    let key = (d2, cc.0, cc.1);
                    if best.is_none_or(|b| key < b.0) {
                        best = Some((key, cc));
                    }
                }
            }
        }
        if let Some((_, cc)) = best {
            return Some(cc);
        }
    }
    None
}

/// Whether the straight line between two cells crosses only water (a supercover walk).
pub fn line_of_water(mask: &dyn WaterMask, frame: u16, a: (i32, i32), b: (i32, i32)) -> bool {
    let (mut x, mut z) = a;
    let (dx, dz) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
    let (sx, sz) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
    let mut err = dx - dz;
    loop {
        if !mask.is_water(frame, x, z) {
            return false;
        }
        if (x, z) == b {
            return true;
        }
        let e2 = 2 * err;
        // A diagonal step must not cut a land corner: both side cells must be water too.
        if e2 > -dz && e2 < dx {
            if !mask.is_water(frame, x + sx, z) || !mask.is_water(frame, x, z + sz) {
                return false;
            }
            err += -dz + dx;
            x += sx;
            z += sz;
        } else if e2 > -dz {
            err -= dz;
            x += sx;
        } else {
            err += dx;
            z += sz;
        }
    }
}

/// A water path between two cells: A* over 8 neighbours (no corner cutting), bounded to
/// `max_expanded` cells, pulled taut. Returns the corner cells, ends included.
pub fn water_path(
    mask: &dyn WaterMask,
    frame: u16,
    from: (i32, i32),
    to: (i32, i32),
    max_expanded: usize,
) -> Option<Vec<(i32, i32)>> {
    if !mask.is_water(frame, from.0, from.1) || !mask.is_water(frame, to.0, to.1) {
        return None;
    }
    if from == to {
        return Some(vec![from]);
    }
    let h = |c: (i32, i32)| {
        let (dx, dz) = ((c.0 - to.0).unsigned_abs(), (c.1 - to.1).unsigned_abs());
        let (lo, hi) = (dx.min(dz), dx.max(dz));
        u64::from(14 * lo + 10 * (hi - lo))
    };
    let mut open = BinaryHeap::new();
    let mut g: BTreeMap<(i32, i32), u64> = BTreeMap::new();
    let mut came: BTreeMap<(i32, i32), (i32, i32)> = BTreeMap::new();
    g.insert(from, 0);
    open.push(Reverse((h(from), 0u64, from)));
    let mut expanded = 0usize;
    while let Some(Reverse((_, gc, c))) = open.pop() {
        if g.get(&c).is_some_and(|&best| gc > best) {
            continue;
        }
        if c == to {
            let mut cells = vec![c];
            let mut cur = c;
            while let Some(&p) = came.get(&cur) {
                cells.push(p);
                cur = p;
            }
            cells.reverse();
            return Some(pull_taut(mask, frame, &cells));
        }
        expanded += 1;
        if expanded > max_expanded {
            return None;
        }
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let n = (c.0 + dx, c.1 + dz);
            if !mask.is_water(frame, n.0, n.1) {
                continue;
            }
            let diag = dx != 0 && dz != 0;
            if diag
                && (!mask.is_water(frame, c.0 + dx, c.1) || !mask.is_water(frame, c.0, c.1 + dz))
            {
                continue;
            }
            let ng = gc + if diag { 14 } else { 10 };
            if g.get(&n).is_none_or(|&old| ng < old) {
                g.insert(n, ng);
                came.insert(n, c);
                open.push(Reverse((ng + h(n), ng, n)));
            }
        }
    }
    None
}

/// Keeps a corner only when the line past it would cross land.
fn pull_taut(mask: &dyn WaterMask, frame: u16, cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = vec![cells[0]];
    let mut i = 0;
    while i < cells.len() - 1 {
        let mut j = cells.len() - 1;
        while j > i + 1 && !line_of_water(mask, frame, cells[i], cells[j]) {
            j -= 1;
        }
        out.push(cells[j]);
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coast() -> SynthCoast {
        SynthCoast {
            cell: 32,
            shore_z: 0,
            amp: 0,
            wavelength: 1000,
            islands: vec![(0, 1000, 400)],
            bays: vec![],
        }
    }

    #[test]
    fn a_path_goes_round_an_island_and_stays_at_sea() {
        let m = coast();
        let a = cell_of(&m, -1200, 1000);
        let b = cell_of(&m, 1200, 1000);
        let p = water_path(&m, 0, a, b, 100_000).expect("reachable");
        assert!(p.len() >= 3, "it must bend round the island: {p:?}");
        for w in p.windows(2) {
            assert!(line_of_water(&m, 0, w[0], w[1]));
        }
        // Not through land straight across.
        assert!(!line_of_water(&m, 0, a, b));
        assert_eq!(
            p,
            water_path(&m, 0, a, b, 100_000).unwrap(),
            "deterministic"
        );
    }

    #[test]
    fn land_locked_and_bounded_searches_fail() {
        let m = coast();
        let land = cell_of(&m, 0, -500);
        let sea = cell_of(&m, 0, 2000);
        assert!(water_path(&m, 0, land, sea, 10_000).is_none());
        let far = cell_of(&m, 50_000, 2000);
        assert!(water_path(&m, 0, sea, far, 50).is_none());
        // The village on the shore snaps to the water next to it.
        let s = snap_to_water(&m, 0, 0, -40, 4).expect("sea within four cells");
        assert!(m.is_water(0, s.0, s.1));
    }
}
