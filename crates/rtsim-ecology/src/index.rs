// SPDX-License-Identifier: AGPL-3.0-or-later
//! A static spatial index of prey: villages and roads by coarse cell, so a hungry dragon weighs
//! what is within its territory by looking up the cells around its lair, never by scanning the
//! world (SCALE.md §0). It is rebuilt when the world gains markets or roads (a village joining,
//! C22), which is rare.

use std::collections::BTreeMap;

use rtsim_world::World;

/// Cell edge in blocks. PLACEHOLDER 1,024: a territory of 2,500 blocks looks up about 36 cells.
pub const CELL: i64 = 1_024;

#[derive(Clone, Debug, Default)]
pub struct PreyIndex {
    villages: BTreeMap<(u16, i64, i64), Vec<u32>>,
    roads: BTreeMap<(u16, i64, i64), Vec<u32>>,
    nodes: usize,
    edges: usize,
}

fn cell(x: i64) -> i64 {
    x.div_euclid(CELL)
}

impl PreyIndex {
    /// Brings the index up to the world's markets and roads.
    pub fn refresh(&mut self, w: &World) {
        if self.nodes == w.nodes.len() && self.edges == w.edges.len() {
            return;
        }
        self.villages.clear();
        self.roads.clear();
        for (i, n) in w.nodes.iter().enumerate() {
            if !n.kind.is_storehouse() {
                continue;
            }
            self.villages
                .entry((n.pos.frame, cell(i64::from(n.pos.x)), cell(i64::from(n.pos.z))))
                .or_default()
                .push(i as u32);
        }
        for (k, e) in w.edges.iter().enumerate() {
            let (a, b) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
            let (x0, x1) = (i64::from(a.x.min(b.x)), i64::from(a.x.max(b.x)));
            let (z0, z1) = (i64::from(a.z.min(b.z)), i64::from(a.z.max(b.z)));
            for cx in cell(x0)..=cell(x1) {
                for cz in cell(z0)..=cell(z1) {
                    self.roads.entry((a.frame, cx, cz)).or_default().push(k as u32);
                }
            }
        }
        self.nodes = w.nodes.len();
        self.edges = w.edges.len();
    }

    fn near(
        map: &BTreeMap<(u16, i64, i64), Vec<u32>>,
        frame: u16,
        x: i64,
        z: i64,
        r: i64,
    ) -> Vec<u32> {
        let mut out = Vec::new();
        for cx in cell(x - r)..=cell(x + r) {
            for cz in cell(z - r)..=cell(z + r) {
                if let Some(v) = map.get(&(frame, cx, cz)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Market nodes in the cells within `r` of (x, z): candidates; the caller checks distance.
    pub fn villages_near(&self, frame: u16, x: i64, z: i64, r: i64) -> Vec<u32> {
        Self::near(&self.villages, frame, x, z, r)
    }

    /// Edges whose bounding box touches a cell within `r` of (x, z).
    pub fn roads_near(&self, frame: u16, x: i64, z: i64, r: i64) -> Vec<u32> {
        Self::near(&self.roads, frame, x, z, r)
    }

    pub fn heap_bytes(&self) -> usize {
        (self.villages.len() + self.roads.len()) * 64
            + self
                .villages
                .values()
                .chain(self.roads.values())
                .map(|v| v.capacity() * 4)
                .sum::<usize>()
    }
}
