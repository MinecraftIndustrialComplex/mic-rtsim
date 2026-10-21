// SPDX-License-Identifier: AGPL-3.0-or-later
//! A static spatial index of prey: villages and roads by coarse cell, so a hungry dragon weighs
//! what is within its territory by looking up the cells around its lair, never by scanning the
//! world (SCALE.md §0). It is rebuilt when the world gains markets or roads (a village joining,
//! C22), which is rare.
//!
//! **Seams** (TP4, TP8): villages are keyed by their canonical cell; a road is keyed by every
//! canonical cell its short way's bounding box covers, across seams; a query folds its square
//! into canonical rectangles, so a dragon near an edge sees the villages and roads beyond it.

use std::collections::BTreeMap;

use rtsim_space::topology::{Rect, Topologies, Topology};
use rtsim_world::World;

/// Cell edge in blocks. PLACEHOLDER 1,024: a territory of 2,500 blocks looks up about 36 cells.
pub const CELL: i64 = 1_024;

#[derive(Clone, Debug, Default)]
pub struct PreyIndex {
    pub(crate) villages: BTreeMap<(u16, i64, i64), Vec<u32>>,
    pub(crate) roads: BTreeMap<(u16, i64, i64), Vec<u32>>,
    pub(crate) nodes: usize,
    pub(crate) edges: usize,
}

fn cell(x: i64) -> i64 {
    x.div_euclid(CELL)
}

impl PreyIndex {
    /// Brings the index up to the world's markets and roads.
    pub fn refresh(&mut self, w: &World, topo: &Topologies) {
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
                .entry((
                    n.pos.frame,
                    cell(i64::from(n.pos.x)),
                    cell(i64::from(n.pos.z)),
                ))
                .or_default()
                .push(i as u32);
        }
        for (k, e) in w.edges.iter().enumerate() {
            let (a, b) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
            let t = topo.of(a.frame);
            let pa = (i64::from(a.x), i64::from(a.z));
            let pb = t.toward(pa, (i64::from(b.x), i64::from(b.z)));
            for part in t.fold_rect(Rect::spanning(pa, pb, 0)) {
                let (x0, z0, x1, z1) = part.grid_cells(CELL);
                for cx in x0..=x1 {
                    for cz in z0..=z1 {
                        let v = self.roads.entry((a.frame, cx, cz)).or_default();
                        if v.last() != Some(&(k as u32)) {
                            v.push(k as u32);
                        }
                    }
                }
            }
        }
        self.nodes = w.nodes.len();
        self.edges = w.edges.len();
    }

    fn near(
        map: &BTreeMap<(u16, i64, i64), Vec<u32>>,
        t: &Topology,
        frame: u16,
        x: i64,
        z: i64,
        r: i64,
    ) -> Vec<u32> {
        let mut out = Vec::new();
        for part in t.rects_near(x, z, r) {
            let (x0, z0, x1, z1) = part.grid_cells(CELL);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    if let Some(v) = map.get(&(frame, cx, cz)) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Market nodes in the cells within `r` of (x, z), across seams: candidates; the caller
    /// checks distance (through the topology).
    pub fn villages_near(&self, t: &Topology, frame: u16, x: i64, z: i64, r: i64) -> Vec<u32> {
        Self::near(&self.villages, t, frame, x, z, r)
    }

    /// Edges whose bounding box touches a cell within `r` of (x, z), across seams.
    pub fn roads_near(&self, t: &Topology, frame: u16, x: i64, z: i64, r: i64) -> Vec<u32> {
        Self::near(&self.roads, t, frame, x, z, r)
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
