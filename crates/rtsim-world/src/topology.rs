// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world on a wrapped or glued frame (docs/choices-topology.md TP20, TP21): one
//! topology for the leg index, the settlement table and the road index, and the few measures the
//! world takes between its nodes.
//!
//! Node positions are canonical (a village is keyed where it is on the world). A road from `a` to
//! `b` runs the short way, from `a` to the image of `b` nearest it ([`World::road_point`]), so a
//! caravan, a traveller sampled from a flow and a touring band are on the road where it runs,
//! across a seam if it crosses one. On a flat frame every measure is the old one.

use rtsim_knowledge::{ActorId, Place};
use rtsim_settle::Pos;
use rtsim_space::topology::{Move, SharedTopologies, Topology};
use rtsim_travel::RoadIndex;

use crate::world::World;

impl World {
    /// Hands the world every frame's topology: its leg index (live legs re-booked), its
    /// settlement table and, when it changed, its road index (rebuilt from the edges).
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        let changed =
            !std::sync::Arc::ptr_eq(&t, self.legs.topologies()) && *t != **self.legs.topologies();
        self.legs.set_topologies(t.clone());
        self.settlements.set_topologies(t);
        if changed {
            self.reindex_roads();
        }
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.legs.topo(frame)
    }

    /// The road index booked again from every edge (a topology change).
    pub(crate) fn reindex_roads(&mut self) {
        let mut roads = RoadIndex::new();
        for (e, edge) in self.edges.iter().enumerate() {
            let (pa, pb) = (self.nodes[edge.a].pos, self.nodes[edge.b].pos);
            if pa.frame == pb.frame {
                roads.add_on(
                    self.topo(pa.frame),
                    e as u32,
                    pa.frame,
                    (i64::from(pa.x), i64::from(pa.z)),
                    (i64::from(pb.x), i64::from(pb.z)),
                    i64::from(self.cfg.view_blocks),
                );
            }
        }
        self.roads = roads;
    }

    /// A canonical position for a node the bridge reported (TP4).
    pub(crate) fn canonical_pos(&self, p: Pos) -> Pos {
        let (x, z) = self.topo(p.frame).canonical_i32(p.x, p.z);
        Pos::new(p.frame, x, z)
    }

    /// Whole blocks between two positions through every seam, `None` across frames: the old
    /// `Pos::dist` on a flat frame.
    pub fn pos_dist(&self, a: Pos, b: Pos) -> Option<u64> {
        self.settlements.dist_sq(a, b).map(rtsim_settle::isqrt)
    }

    /// The canonical point `milli` thousandths of the way along the road from `a` to `b` (the
    /// short way): the old integer lerp on a flat frame.
    pub fn road_point(&self, a: Pos, b: Pos, milli: i64) -> (i64, i64) {
        let t = self.topo(a.frame);
        let (ax, az) = (i64::from(a.x), i64::from(a.z));
        let (bx, bz) = t.toward((ax, az), (i64::from(b.x), i64::from(b.z)));
        let p = (ax + (bx - ax) * milli / 1000, az + (bz - az) * milli / 1000);
        if t.is_flat() {
            p
        } else {
            t.canonical(p.0, p.1)
        }
    }

    /// The squared distance from `p` to the road `a`-`b` through every seam (the road the short
    /// way, `p` at any image): the old point-to-segment distance on a flat frame.
    pub fn dist2_to_road(&self, p: (i64, i64), a: Pos, b: Pos) -> f64 {
        let t = self.topo(a.frame);
        let (pa, pb) = (
            (i64::from(a.x), i64::from(a.z)),
            (i64::from(b.x), i64::from(b.z)),
        );
        if t.is_flat() {
            return rtsim_travel::roads::dist2_to_segment(p, pa, pb);
        }
        let end = t.toward(pa, pb);
        let m = Move::of_cells(0.0, 1.0, pa, end);
        let (d, _) = t.closest_to(&m, (p.0 as f64 + 0.5, p.1 as f64 + 0.5));
        d * d
    }

    /// Candidates for a witness test at `place`, each moved to its image nearest `place`, so
    /// `rtsim_knowledge::witnesses` (a plain distance) sees across seams.
    pub fn near_images(&self, place: Place, near: Vec<(ActorId, Place)>) -> Vec<(ActorId, Place)> {
        let t = self.topo(place.frame);
        if t.is_flat() {
            return near;
        }
        let from = (i64::from(place.x), i64::from(place.z));
        near.into_iter()
            .map(|(a, q)| {
                if q.frame != place.frame {
                    return (a, q);
                }
                let (x, z) = t.toward(from, (i64::from(q.x), i64::from(q.z)));
                (a, Place::new(q.frame, x as i32, q.y, z as i32))
            })
            .collect()
    }
}
