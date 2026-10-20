// SPDX-License-Identifier: AGPL-3.0-or-later
//! Walks through seams: a straight chart move cut into canonical pieces, and the closed-form
//! meeting and closest-approach tests that see across every seam (TP7 to TP9).
//!
//! A move's pieces each lie in one tile of the cover and carry that tile's motion to canonical
//! coordinates, so a piece is again a straight move (motions are rigid). Two canonical points
//! closer than the domain's narrow side are nearest through one of the images of the domain that
//! touch it ([`Topology::adjacent`]), so a meeting test compares each pair of pieces under those
//! images only: nine for a Guyou world, three for an x wrap, one for the plane.

use super::Topology;
use super::iso::Iso;

/// One piece of a chart walk between seams: fractions `[f0, f1]` of the walk, and the motion
/// chart → canonical there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub f0: f64,
    pub f1: f64,
    pub iso: Iso,
}

/// A straight move in chart coordinates over `[t0, t1]`, as points (a cell's centre is
/// `x + 0.5`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub t0: f64,
    pub t1: f64,
    pub a: (f64, f64),
    pub b: (f64, f64),
}

impl Move {
    /// From cell `a`'s centre at `t0` to cell `b`'s at `t1`.
    pub fn of_cells(t0: f64, t1: f64, a: (i64, i64), b: (i64, i64)) -> Move {
        Move {
            t0,
            t1: t1.max(t0),
            a: (a.0 as f64 + 0.5, a.1 as f64 + 0.5),
            b: (b.0 as f64 + 0.5, b.1 as f64 + 0.5),
        }
    }

    /// Position at `t`, clamped to the span.
    pub fn at(&self, t: f64) -> (f64, f64) {
        if self.t1 <= self.t0 || t <= self.t0 {
            return self.a;
        }
        if t >= self.t1 {
            return self.b;
        }
        let f = (t - self.t0) / (self.t1 - self.t0);
        (
            self.a.0 + (self.b.0 - self.a.0) * f,
            self.a.1 + (self.b.1 - self.a.1) * f,
        )
    }

    fn moved(&self, iso: Iso) -> Move {
        Move {
            t0: self.t0,
            t1: self.t1,
            a: iso.point_f(self.a.0, self.a.1),
            b: iso.point_f(self.b.0, self.b.1),
        }
    }
}

/// The first `f` in `[0, 1]` at which `p0 + (p1 - p0) f` is within `reach` of the origin.
fn first_inside(p0: (f64, f64), p1: (f64, f64), reach: f64) -> Option<f64> {
    if p0.0 * p0.0 + p0.1 * p0.1 <= reach * reach {
        return Some(0.0);
    }
    let (dx, dz) = (p1.0 - p0.0, p1.1 - p0.1);
    let qa = dx * dx + dz * dz;
    if qa == 0.0 {
        return None;
    }
    let qb = 2.0 * (p0.0 * dx + p0.1 * dz);
    let qc = p0.0 * p0.0 + p0.1 * p0.1 - reach * reach;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let f = (-qb - disc.sqrt()) / (2.0 * qa);
    (0.0..=1.0).contains(&f).then_some(f)
}

impl Topology {
    /// The images a query of `reach` blocks needs: the touching ones below
    /// [`Topology::local_reach`], every near tile beyond it.
    pub fn images_within(&self, reach: f64) -> Vec<Iso> {
        if reach < self.local_reach() as f64 {
            return self.adjacent().to_vec();
        }
        match self {
            Topology::Glued(g) => g.glued.near.iter().map(|t| t.h).collect(),
            Topology::XWrap { period, .. } => {
                (-2..=2).map(|n| Iso::translation(n * period, 0)).collect()
            }
            Topology::Flat { .. } => vec![Iso::IDENTITY],
        }
    }

    /// The move cut at seams, each piece moved to canonical coordinates. A flat world gives the
    /// move back whole.
    pub fn canonical_moves(&self, m: &Move) -> Vec<Move> {
        if self.is_flat() {
            return vec![*m];
        }
        let a2 = (2.0 * m.a.0, 2.0 * m.a.1);
        let b2 = (2.0 * m.b.0, 2.0 * m.b.1);
        let span = m.t1 - m.t0;
        self.pieces_half(a2, b2)
            .into_iter()
            .map(|p| {
                let part = Move {
                    t0: m.t0 + span * p.f0,
                    t1: m.t0 + span * p.f1,
                    a: (
                        m.a.0 + (m.b.0 - m.a.0) * p.f0,
                        m.a.1 + (m.b.1 - m.a.1) * p.f0,
                    ),
                    b: (
                        m.a.0 + (m.b.0 - m.a.0) * p.f1,
                        m.a.1 + (m.b.1 - m.a.1) * p.f1,
                    ),
                };
                part.moved(p.iso)
            })
            .collect()
    }

    /// The first time two chart moves come within `reach` of each other through any seam, if
    /// they do while both run.
    pub fn first_within(&self, a: &Move, b: &Move, reach: f64) -> Option<f64> {
        let (pa, pb) = (self.canonical_moves(a), self.canonical_moves(b));
        let images = self.images_within(reach);
        let mut best: Option<f64> = None;
        for x in &pa {
            for y in &pb {
                let lo = x.t0.max(y.t0);
                let hi = x.t1.min(y.t1);
                if hi < lo || best.is_some_and(|t| t <= lo) {
                    continue;
                }
                for &h in &images {
                    let rel = |t: f64| {
                        let p = x.at(t);
                        let q = y.at(t);
                        let q = h.point_f(q.0, q.1);
                        (p.0 - q.0, p.1 - q.1)
                    };
                    if let Some(f) = first_inside(rel(lo), rel(hi), reach) {
                        let t = lo + f * (hi - lo);
                        if best.is_none_or(|b| t < b) {
                            best = Some(t);
                        }
                    }
                }
            }
        }
        best
    }

    /// The closest approach of a chart move to a canonical point through any seam:
    /// (distance, time).
    pub fn closest_to(&self, m: &Move, p: (f64, f64)) -> (f64, f64) {
        let images = self.adjacent();
        let mut best = (f64::INFINITY, m.t0);
        for part in self.canonical_moves(m) {
            for &h in images {
                let q = h.point_f(p.0, p.1);
                let (dx, dz) = (part.b.0 - part.a.0, part.b.1 - part.a.1);
                let l2 = dx * dx + dz * dz;
                let f = if l2 > 0.0 {
                    (((q.0 - part.a.0) * dx + (q.1 - part.a.1) * dz) / l2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (cx, cz) = (part.a.0 + dx * f, part.a.1 + dz * f);
                let d = ((cx - q.0).powi(2) + (cz - q.1).powi(2)).sqrt();
                if d < best.0 {
                    best = (d, part.t0 + (part.t1 - part.t0) * f);
                }
            }
        }
        best
    }
}
