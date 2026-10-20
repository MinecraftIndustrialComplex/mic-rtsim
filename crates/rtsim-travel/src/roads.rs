// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which roads pass near a place: a static region index over edges (PLAN.md §4.3, SCALE.md §3).
//!
//! Flows are not booked in the time-windowed [`crate::legs::LegIndex`]: a road carries traffic for
//! as long as it exists, and booking it into every time window would cost per window. A road is
//! rasterized once, when the edge is made, into the 512-block regions its straight path crosses,
//! widened by a margin; a player's interest area then finds the roads to sample travellers from
//! by looking up the regions it overlaps, with no scan over edges.

use std::collections::BTreeMap;

use crate::legs::REGION_BLOCKS;

#[derive(Clone, Debug, Default)]
pub struct RoadIndex {
    /// (frame, region x, region z) to the edges crossing it, sorted.
    cells: BTreeMap<(u16, i64, i64), Vec<u32>>,
}

impl RoadIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Books edge `edge`, a straight road from `a` to `b` in `frame`, widened by `margin` blocks.
    pub fn add(&mut self, edge: u32, frame: u16, a: (i64, i64), b: (i64, i64), margin: i64) {
        let len = ((b.0 - a.0).pow(2) + (b.1 - a.1).pow(2)) as f64;
        // Sample the segment every half region, so every region it crosses is booked.
        let steps = (len.sqrt() / (REGION_BLOCKS as f64 / 2.0)).ceil().max(1.0) as i64;
        for k in 0..=steps {
            let x = a.0 + (b.0 - a.0) * k / steps;
            let z = a.1 + (b.1 - a.1) * k / steps;
            for rx in
                (x - margin).div_euclid(REGION_BLOCKS)..=(x + margin).div_euclid(REGION_BLOCKS)
            {
                for rz in
                    (z - margin).div_euclid(REGION_BLOCKS)..=(z + margin).div_euclid(REGION_BLOCKS)
                {
                    let v = self.cells.entry((frame, rx, rz)).or_default();
                    if let Err(i) = v.binary_search(&edge) {
                        v.insert(i, edge);
                    }
                }
            }
        }
    }

    /// The edges booked in any region a circle of `radius` around `(x, z)` overlaps, sorted.
    pub fn near(&self, frame: u16, x: i64, z: i64, radius: i64) -> Vec<u32> {
        let mut out = Vec::new();
        for rx in (x - radius).div_euclid(REGION_BLOCKS)..=(x + radius).div_euclid(REGION_BLOCKS) {
            for rz in
                (z - radius).div_euclid(REGION_BLOCKS)..=(z + radius).div_euclid(REGION_BLOCKS)
            {
                if let Some(v) = self.cells.get(&(frame, rx, rz)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn heap_bytes(&self) -> usize {
        self.cells
            .values()
            .map(|v| v.capacity() * 4 + 48)
            .sum::<usize>()
    }
}

/// The squared distance from `p` to the segment `a`-`b`, in blocks squared.
pub fn dist2_to_segment(p: (i64, i64), a: (i64, i64), b: (i64, i64)) -> f64 {
    let (px, pz) = (p.0 as f64, p.1 as f64);
    let (ax, az, bx, bz) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
    let (dx, dz) = (bx - ax, bz - az);
    let l2 = dx * dx + dz * dz;
    let t = if l2 == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (pz - az) * dz) / l2).clamp(0.0, 1.0)
    };
    let (cx, cz) = (ax + t * dx, az + t * dz);
    (px - cx).powi(2) + (pz - cz).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roads_are_found_where_they_pass() {
        let mut ix = RoadIndex::new();
        ix.add(3, 0, (0, 0), (2000, 0), 16);
        ix.add(5, 0, (0, 1500), (0, 3000), 16);
        assert_eq!(ix.near(0, 1000, 10, 64), vec![3]);
        assert_eq!(ix.near(0, 5, 2000, 64), vec![5]);
        assert!(ix.near(0, 1000, 1200, 64).is_empty());
        assert!(ix.near(1, 1000, 10, 64).is_empty());
        assert!((dist2_to_segment((1000, 30), (0, 0), (2000, 0)) - 900.0).abs() < 1e-9);
        assert!((dist2_to_segment((-30, 40), (0, 0), (2000, 0)) - 2500.0).abs() < 1e-9);
    }
}
