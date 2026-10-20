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

/// Where two straight segments cross, as fractions along each, if they do.
fn cross_f(p0: (f64, f64), p1: (f64, f64), q0: (f64, f64), q1: (f64, f64)) -> Option<(f64, f64)> {
    let r = (p1.0 - p0.0, p1.1 - p0.1);
    let s = (q1.0 - q0.0, q1.1 - q0.1);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-9 {
        return None;
    }
    let qp = (q0.0 - p0.0, q0.1 - p0.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / den;
    let u = (qp.0 * r.1 - qp.1 * r.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

impl Topology {
    /// Where two chart segments cross on the world, through any seam: `a0 → a1` and `b0 → b1`
    /// (each from a canonical cell to a chart cell), as fractions along each; the first along
    /// `a` when they cross more than once.
    pub fn crossing(
        &self,
        a0: (i64, i64),
        a1: (i64, i64),
        b0: (i64, i64),
        b1: (i64, i64),
    ) -> Option<(f64, f64)> {
        let half = |p: (i64, i64)| ((2 * p.0 + 1) as f64, (2 * p.1 + 1) as f64);
        let (pa, pb) = (
            self.pieces_half(half(a0), half(a1)),
            self.pieces_half(half(b0), half(b1)),
        );
        let point = |p0: (i64, i64), p1: (i64, i64), f: f64| {
            (
                p0.0 as f64 + 0.5 + (p1.0 - p0.0) as f64 * f,
                p0.1 as f64 + 0.5 + (p1.1 - p0.1) as f64 * f,
            )
        };
        let mut best: Option<(f64, f64)> = None;
        for x in &pa {
            let (x0, x1) = (point(a0, a1, x.f0), point(a0, a1, x.f1));
            let (x0, x1) = (x.iso.point_f(x0.0, x0.1), x.iso.point_f(x1.0, x1.1));
            for y in &pb {
                let (y0, y1) = (point(b0, b1, y.f0), point(b0, b1, y.f1));
                let (y0, y1) = (y.iso.point_f(y0.0, y0.1), y.iso.point_f(y1.0, y1.1));
                for h in self.adjacent() {
                    let (z0, z1) = (h.point_f(y0.0, y0.1), h.point_f(y1.0, y1.1));
                    if let Some((t, u)) = cross_f(x0, x1, z0, z1) {
                        let fa = x.f0 + (x.f1 - x.f0) * t;
                        let fb = y.f0 + (y.f1 - y.f0) * u;
                        if best.is_none_or(|b| fa < b.0) {
                            best = Some((fa, fb));
                        }
                    }
                }
            }
        }
        best
    }
}
