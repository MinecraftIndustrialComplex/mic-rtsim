// SPDX-License-Identifier: AGPL-3.0-or-later
//! The world's topology: how the edges of a frame are joined, and the geometry every module
//! measures through (DECISIONS.md C31; docs/choices-topology.md TP1 to TP20).
//!
//! Until this layer every module assumed a flat, unbounded plane: `dx² + dz²`, region keys by
//! division, neighbours by ±1. A Deep Time world is not that. Its Petroff–Guyou layout is the
//! rectangle x in [−b, b), z in [−b/2, b/2] with its east and west edges slid onto each other
//! (Toroidal World) and its north and south edges each turned half a turn onto itself (Deep Time's
//! wrap engine), so walking north over a pole comes out heading south on the far side. A herd, a
//! caravan road, a sea lane or a dragon's flight that crosses an edge must see one world.
//!
//! # Providers ([`Topology`])
//!
//! - **Flat**: the plane, with an optional world border (vanilla MIC worlds). Every operation is
//!   the identity, so a flat frame behaves exactly as before this layer, bit for bit (TP1).
//! - **XWrap**: x wraps with period P (Toroidal World alone).
//! - **Glued**: a rectangular domain whose edges are glued by grid motions ([`Glued`]), read from
//!   Deep Time's layout block ([`layout`]) or any manifest-like edge list.
//!
//! A frame no record has described is flat ([`Topologies::of`]).
//!
//! # Conventions
//!
//! - Positions are **block cells** `(x, z)`, i64, as the modules' `Pos` holds them; a cell stands
//!   for its centre (x + 0.5, z + 0.5) wherever a distance or a turn is taken, and the doubled
//!   coordinates of [`Iso::half`] keep that exact. Every distance here is exact integer arithmetic
//!   (TP2): `dist_sq` is a whole number of square blocks, the same on every machine.
//! - **Canonical** positions lie in the domain. Every key a module stores (a region, a province,
//!   a grid cell) is taken of a canonical position (TP4), so one place has one key.
//! - A **chart** is the cover around a canonical point: coordinates that keep going straight past
//!   an edge. A leg is a straight walk in a chart, from its canonical start to the image of its
//!   end nearest that start ([`Topology::toward`]); its position at any time is the chart position
//!   folded ([`Topology::canonical`]), a closed form per piece between seams ([`Topology::pieces`]).
//! - **Directions** turn across a half-turn seam: [`Topology::transport`] carries a heading with
//!   the position, so a herd heading north over a pole heads south after it.
//!
//! # Edge_stop
//!
//! Until Deep Time's wrap engine installs the topology on a level, players, their vehicles and
//! Sable ships stop one block inside the north and south edges. Those seams still exist for
//! distance (abstract travel crosses them), but nothing embodied may: [`Topology::embodied`] is the
//! world with them as walls, and [`Topology::stopped_crossing`] finds where a walk would cross
//! one, so a module dematerializes before it (TP12).
//!
//! # Cone points
//!
//! Where a turn of the group fixes a point (the long edges' midpoints and the corners of a
//! Guyou world), half a turn of planet is drawn in a whole turn of world: walking round one takes
//! half the distance and reverses a heading, and the nearest image of a point close by can be
//! two. Nothing is spawned or steered to within [`CONE_CLEARANCE`] blocks of one
//! ([`Topology::clear_of_cones`], TP13).

mod glued;
mod iso;
pub mod layout;
mod path;
pub mod record;

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

pub use glued::{Edge, Folded, Glued, Side};
pub use iso::{Iso, Rect, rotate, rotate_f};
pub use path::{Move, Piece};

/// Spawns and waypoints keep this many blocks from any cone point. PLACEHOLDER (TP13): three
/// chunks, the "few chunks" within which Deep Time says no heading or metric is consistent.
pub const CONE_CLEARANCE: i64 = 48;

/// Embodied things keep this many blocks inside an edge_stop border. PLACEHOLDER (TP12): the
/// border stops a player one block inside the edge; a body keeps clear of it by a few more.
pub const STOP_MARGIN: i64 = 4;

/// Which provider a frame has (the TOPOLOGY record's `kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Kind {
    Flat = 0,
    XWrap = 1,
    Glued = 2,
}

/// A glued world and, while edge_stop holds on some of its seams, the world embodied things see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GluedWorld {
    pub glued: Glued,
    embodied: Option<Box<Topology>>,
}

/// How one frame's edges are joined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Topology {
    /// The plane. `border` is the world border, for what embodied things may reach; it moves no
    /// position.
    Flat {
        border: Option<Rect>,
    },
    /// x wraps with `period`, the canonical lap is [x0, x0 + period).
    XWrap {
        x0: i64,
        period: i64,
        border: Option<Rect>,
        adjacent: [Iso; 3],
    },
    Glued(Box<GluedWorld>),
}

static FLAT: Topology = Topology::Flat { border: None };
static IDENTITY_ONLY: [Iso; 1] = [Iso::IDENTITY];

impl Default for Topology {
    fn default() -> Self {
        Topology::flat()
    }
}

impl Topology {
    pub const fn flat() -> Topology {
        Topology::Flat { border: None }
    }

    pub fn flat_with_border(border: Rect) -> Topology {
        Topology::Flat {
            border: Some(border),
        }
    }

    /// x wraps over [x0, x0 + period).
    pub fn x_wrap(x0: i64, period: i64) -> Topology {
        let p = period.max(1);
        Topology::XWrap {
            x0,
            period: p,
            border: None,
            adjacent: [
                Iso::IDENTITY,
                Iso::translation(-p, 0),
                Iso::translation(p, 0),
            ],
        }
    }

    /// A glued world. A gluing of only the two x slides is an [`Topology::XWrap`] (the same
    /// geometry, cheaper); a domain with no edges is flat with the domain as its border.
    pub fn glued(domain: Rect, edges: &[Edge]) -> Result<Topology, String> {
        let g = Glued::new(domain, edges)?;
        let w = domain.width();
        let only_slides = g.edges().count() == 2
            && g.edge(Side::West)
                .is_some_and(|e| !e.stopped && e.iso == Iso::translation(w, 0))
            && g.edge(Side::East)
                .is_some_and(|e| !e.stopped && e.iso == Iso::translation(-w, 0));
        if only_slides {
            let Topology::XWrap {
                x0,
                period,
                adjacent,
                ..
            } = Topology::x_wrap(domain.x0, w)
            else {
                unreachable!()
            };
            return Ok(Topology::XWrap {
                x0,
                period,
                border: Some(domain),
                adjacent,
            });
        }
        if g.edges().count() == 0 {
            return Ok(Topology::flat_with_border(domain));
        }
        let embodied = g.any_stopped().then(|| {
            let e = g.embodied();
            let edges: Vec<Edge> = e.edges().copied().collect();
            Box::new(Topology::glued(domain, &edges).expect("consistent"))
        });
        Ok(Topology::Glued(Box::new(GluedWorld { glued: g, embodied })))
    }

    pub fn kind(&self) -> Kind {
        match self {
            Topology::Flat { .. } => Kind::Flat,
            Topology::XWrap { .. } => Kind::XWrap,
            Topology::Glued(_) => Kind::Glued,
        }
    }

    /// The plane: every operation is the identity.
    pub fn is_flat(&self) -> bool {
        matches!(self, Topology::Flat { .. })
    }

    /// The domain canonical positions lie in, if the world is bounded or wrapped (the XWrap lap
    /// has unbounded z unless it carries a border).
    pub fn domain(&self) -> Option<Rect> {
        match self {
            Topology::Flat { border } => *border,
            Topology::XWrap {
                x0, period, border, ..
            } => Some(match border {
                Some(b) => Rect::new(*x0, b.z0, *x0 + *period, b.z1),
                None => Rect::new(*x0, i64::MIN / 4, *x0 + *period, i64::MAX / 4),
            }),
            Topology::Glued(g) => Some(g.glued.domain()),
        }
    }

    pub fn as_glued(&self) -> Option<&Glued> {
        match self {
            Topology::Glued(g) => Some(&g.glued),
            _ => None,
        }
    }

    /// Brings a chart cell into the domain, with the motion that did it.
    pub fn fold(&self, x: i64, z: i64) -> Folded {
        match self {
            Topology::Flat { .. } => Folded {
                x,
                z,
                iso: Iso::IDENTITY,
                clamped: false,
            },
            Topology::XWrap { x0, period, .. } => {
                let n = (x - x0).div_euclid(*period);
                Folded {
                    x: x - n * period,
                    z,
                    iso: Iso::translation(-n * period, 0),
                    clamped: false,
                }
            }
            Topology::Glued(g) => g.glued.fold_cell(x, z),
        }
    }

    /// The canonical cell of a chart cell.
    pub fn canonical(&self, x: i64, z: i64) -> (i64, i64) {
        let f = self.fold(x, z);
        (f.x, f.z)
    }

    /// The canonical cell of an i32 position (the modules' `Pos`), as i32.
    pub fn canonical_i32(&self, x: i32, z: i32) -> (i32, i32) {
        let (x, z) = self.canonical(i64::from(x), i64::from(z));
        (sat_i32(x), sat_i32(z))
    }

    /// An entity position brought into the domain, keeping its place inside its block: the
    /// motion of the block it is in, applied to the point.
    pub fn canonical_point(&self, x: f64, z: f64) -> (f64, f64) {
        if self.is_flat() || !x.is_finite() || !z.is_finite() {
            return (x, z);
        }
        let f = self.fold(x.floor() as i64, z.floor() as i64);
        f.iso.point_f(x, z)
    }

    /// The canonical cell of an entity position (floored to its block first).
    pub fn canonical_f(&self, x: f64, z: f64) -> (i64, i64) {
        self.canonical(x.floor() as i64, z.floor() as i64)
    }

    /// The shortest displacement from `a` to `b` through every seam, blocks (cell centre to cell
    /// centre). Both are canonicalized first, so any chart cells may be given.
    pub fn displacement(&self, a: (i64, i64), b: (i64, i64)) -> (i64, i64) {
        match self {
            Topology::Flat { .. } => (b.0 - a.0, b.1 - a.1),
            Topology::XWrap { period, .. } => {
                let p = *period;
                let h = p / 2;
                (((b.0 - a.0) + h).rem_euclid(p) - h, b.1 - a.1)
            }
            Topology::Glued(g) => {
                let (fa, fb) = (g.glued.fold_cell(a.0, a.1), g.glued.fold_cell(b.0, b.1));
                g.glued.displacement((fa.x, fa.z), (fb.x, fb.z))
            }
        }
    }

    /// The squared shortest distance, square blocks.
    pub fn dist_sq(&self, a: (i64, i64), b: (i64, i64)) -> u64 {
        let (dx, dz) = self.displacement(a, b);
        (dx * dx + dz * dz) as u64
    }

    /// The shortest distance, blocks.
    pub fn dist(&self, a: (i64, i64), b: (i64, i64)) -> f64 {
        (self.dist_sq(a, b) as f64).sqrt()
    }

    /// The image of `to` nearest `from`, in `from`'s chart: where something at `from` heads to
    /// reach `to` the short way (a leg's end, a steering target, a spawn near a player).
    pub fn toward(&self, from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
        if self.is_flat() {
            return to;
        }
        let f = self.fold(from.0, from.1);
        let d = self.displacement((f.x, f.z), to);
        let (dx, dz) = f.iso.inverse().vector(d.0, d.1);
        (from.0 + dx, from.1 + dz)
    }

    /// The image of `to` nearest `from`, in `from`'s chart, and the motion that takes `to`'s
    /// canonical frame onto that image's (so anything near `to`, a waypoint, a heading, moves into
    /// `from`'s frame by it): `image == iso.cell(canonical(to))`.
    pub fn nearest_image(&self, from: (i64, i64), to: (i64, i64)) -> ((i64, i64), Iso) {
        match self {
            Topology::Flat { .. } => (to, Iso::IDENTITY),
            Topology::XWrap { .. } => {
                let f = self.fold(from.0, from.1);
                let t = self.fold(to.0, to.1);
                let d = self.displacement((f.x, f.z), (t.x, t.z));
                let image = (from.0 + d.0, from.1 + d.1);
                (image, Iso::translation(image.0 - t.x, image.1 - t.z))
            }
            Topology::Glued(g) => {
                let f = g.glued.fold_cell(from.0, from.1);
                let t = g.glued.fold_cell(to.0, to.1);
                let (_, h) = g.glued.nearest((f.x, f.z), (t.x, t.z));
                // canonical `to` → its nearest tile (canonical `from`'s chart) → `from`'s chart.
                let iso = f.iso.inverse().compose(h);
                (iso.cell(t.x, t.z), iso)
            }
        }
    }

    /// [`Topology::nearest_image`] for a body: `None` when the short way crosses a stopped seam
    /// (TP12).
    pub fn embodied_image(&self, from: (i64, i64), to: (i64, i64)) -> Option<((i64, i64), Iso)> {
        let (image, iso) = self.nearest_image(from, to);
        if self.any_stopped() {
            let f = self.fold(from.0, from.1);
            let end = f.iso.cell(image.0, image.1);
            if self.stopped_crossing((f.x, f.z), end).is_some() {
                return None;
            }
        }
        Some((image, iso))
    }

    /// A chart cell and a heading there, carried to the canonical cell and turned with it (a
    /// heading north over a pole comes out south).
    pub fn transport(&self, x: i64, z: i64, dx: i64, dz: i64) -> ((i64, i64), (i64, i64)) {
        let f = self.fold(x, z);
        ((f.x, f.z), f.iso.vector(dx, dz))
    }

    /// The float version of [`Topology::transport`] for headings.
    pub fn transport_f(&self, x: i64, z: i64, dx: f64, dz: f64) -> ((i64, i64), (f64, f64)) {
        let f = self.fold(x, z);
        ((f.x, f.z), f.iso.vector_f(dx, dz))
    }

    /// The canonical rectangles a chart rectangle covers (parts past a wall dropped). Flat gives
    /// the rectangle back.
    pub fn fold_rect(&self, r: Rect) -> Vec<Rect> {
        match self {
            Topology::Flat { .. } => {
                if r.is_empty() {
                    Vec::new()
                } else {
                    vec![r]
                }
            }
            Topology::XWrap { x0, period, .. } => {
                if r.is_empty() {
                    return Vec::new();
                }
                let p = *period;
                if r.width() >= p {
                    return vec![Rect::new(*x0, r.z0, *x0 + p, r.z1)];
                }
                let (n0, n1) = ((r.x0 - x0).div_euclid(p), (r.x1 - 1 - x0).div_euclid(p));
                (n0..=n1)
                    .filter_map(|n| {
                        let lap = Rect::new(x0 + n * p, r.z0, x0 + (n + 1) * p, r.z1);
                        let s = r.intersect(&lap);
                        (!s.is_empty()).then(|| Iso::translation(-n * p, 0).rect(s))
                    })
                    .collect()
            }
            Topology::Glued(g) => g.glued.fold_rect(r),
        }
    }

    /// The canonical rectangles within `r` blocks (Chebyshev) of a cell: a spatial query's cells,
    /// seen across every seam.
    pub fn rects_near(&self, x: i64, z: i64, r: i64) -> Vec<Rect> {
        let (cx, cz) = self.canonical(x, z);
        self.fold_rect(Rect::around(cx, cz, r))
    }

    /// The canonical key of a block in a grid of `size` blocks (regions, provinces, cells).
    pub fn key(&self, x: i64, z: i64, size: i64) -> (i64, i64) {
        let (x, z) = self.canonical(x, z);
        (x.div_euclid(size.max(1)), z.div_euclid(size.max(1)))
    }

    /// A grid of `cell` blocks on this world.
    pub fn grid(&self, cell: i64) -> Grid<'_> {
        Grid {
            topo: self,
            cell: cell.max(1),
        }
    }

    /// The images of the domain that touch it, identity first: every image a query shorter than
    /// [`Topology::local_reach`] can need.
    pub fn adjacent(&self) -> &[Iso] {
        match self {
            Topology::Flat { .. } => &IDENTITY_ONLY,
            Topology::XWrap { adjacent, .. } => adjacent,
            Topology::Glued(g) => &g.glued.adjacent,
        }
    }

    /// Below this distance two canonical points' nearest images are among [`Topology::adjacent`].
    pub fn local_reach(&self) -> i64 {
        match self {
            Topology::Flat { .. } => i64::MAX,
            Topology::XWrap { period, .. } => *period,
            Topology::Glued(g) => g.glued.domain().width().min(g.glued.domain().depth()),
        }
    }

    /// Whether the world joins any edge (anything but flat).
    pub fn has_seams(&self) -> bool {
        !self.is_flat()
    }

    /// The cone points, blocks (block corners, so whole or half numbers); each image in the
    /// domain's closure once.
    pub fn cones(&self) -> Vec<(f64, f64)> {
        match self {
            Topology::Glued(g) => g
                .glued
                .cones
                .iter()
                .map(|&(x, z)| (x as f64 / 2.0, z as f64 / 2.0))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether a canonical cell's centre lies within `r` blocks of a cone point.
    pub fn near_cone(&self, x: i64, z: i64, r: i64) -> bool {
        let Topology::Glued(g) = self else {
            return false;
        };
        let (cx, cz) = (2 * x + 1, 2 * z + 1);
        let r2 = 4 * r * r;
        g.glued.cones.iter().any(|&(px, pz)| {
            let (dx, dz) = (cx - px, cz - pz);
            dx * dx + dz * dz < r2
        })
    }

    /// A canonical cell moved off any cone point: a cell within `clearance` blocks of one is
    /// pushed straight away from it to that distance, then canonicalized (TP13). Other cells are
    /// returned as they are. Deterministic: the same cell always moves the same way.
    pub fn clear_of_cones(&self, x: i64, z: i64, clearance: i64) -> (i64, i64) {
        let Topology::Glued(g) = self else {
            return (x, z);
        };
        let (cx, cz) = (2 * x + 1, 2 * z + 1);
        let r2 = 4 * clearance * clearance;
        let mut hit: Option<(i64, i64, i64)> = None;
        for &(px, pz) in &g.glued.cones {
            let (dx, dz) = (cx - px, cz - pz);
            let d2 = dx * dx + dz * dz;
            if d2 < r2 && hit.is_none_or(|(b, _, _)| d2 < b) {
                hit = Some((d2, px, pz));
            }
        }
        let Some((d2, px, pz)) = hit else {
            return (x, z);
        };
        // Cell centres are odd and cone points even in doubled units, so d2 > 0.
        let d = (d2 as f64).sqrt();
        let s = 2.0 * (clearance as f64 + 1.0) / d;
        let nx = px as f64 + (cx - px) as f64 * s;
        let nz = pz as f64 + (cz - pz) as f64 * s;
        let cell = ((nx / 2.0).floor() as i64, (nz / 2.0).floor() as i64);
        self.canonical(cell.0, cell.1)
    }

    /// Whether any seam is under edge_stop.
    pub fn any_stopped(&self) -> bool {
        matches!(self, Topology::Glued(g) if g.embodied.is_some())
    }

    /// The world embodied things move in: stopped seams are walls. The topology itself when
    /// nothing is stopped.
    pub fn embodied(&self) -> &Topology {
        match self {
            Topology::Glued(g) => g.embodied.as_deref().unwrap_or(self),
            _ => self,
        }
    }

    /// Where something embodied at `from` is sent to reach canonical `to`: the image of `to`
    /// nearest `from`, kept [`STOP_MARGIN`] inside every stopped edge; `None` when the short way
    /// crosses a stopped seam (edge_stop), so the caller dematerializes first or does not
    /// materialize at all (TP12). On a world with nothing stopped it is [`Topology::toward`].
    pub fn embodied_toward(&self, from: (i64, i64), to: (i64, i64)) -> Option<(i64, i64)> {
        if self.is_flat() {
            return Some(to);
        }
        let end = self.toward(from, to);
        if !self.any_stopped() {
            return Some(end);
        }
        let f = self.fold(from.0, from.1);
        let end_c = f.iso.cell(end.0, end.1);
        if self.stopped_crossing((f.x, f.z), end_c).is_some() {
            return None;
        }
        Some(self.off_stopped(end.0, end.1))
    }

    /// Where a body at `from` is steered to reach canonical `to` without ever crossing a
    /// stopped seam: the nearest image in the embodied world (stopped seams are walls, so the
    /// route goes round), kept [`STOP_MARGIN`] inside them (TP12). [`Topology::toward`] when
    /// nothing is stopped.
    pub fn embodied_route(&self, from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
        if !self.any_stopped() {
            return self.toward(from, to);
        }
        let e = self.embodied().toward(from, to);
        self.off_stopped(e.0, e.1)
    }

    /// A chart cell kept [`STOP_MARGIN`] blocks inside every stopped edge (in its own tile's
    /// frame), so a body sent there never meets the edge_stop border.
    pub fn off_stopped(&self, x: i64, z: i64) -> (i64, i64) {
        let Topology::Glued(g) = self else {
            return (x, z);
        };
        if g.embodied.is_none() {
            return (x, z);
        }
        let f = g.glued.fold_cell(x, z);
        let d = g.glued.domain();
        let m = STOP_MARGIN.min(d.width() / 4).min(d.depth() / 4);
        let (mut cx, mut cz) = (f.x, f.z);
        for e in g.glued.edges().filter(|e| e.stopped) {
            match e.side {
                Side::West => cx = cx.max(d.x0 + m),
                Side::East => cx = cx.min(d.x1 - 1 - m),
                Side::North => cz = cz.max(d.z0 + m),
                Side::South => cz = cz.min(d.z1 - 1 - m),
            }
        }
        if (cx, cz) == (f.x, f.z) {
            return (x, z);
        }
        f.iso.inverse().cell(cx, cz)
    }

    /// Where a chart walk from canonical `a` to chart `b` first crosses a stopped seam, as the
    /// fraction of the walk; `None` when it crosses none (or nothing is stopped).
    pub fn stopped_crossing(&self, a: (i64, i64), b: (i64, i64)) -> Option<f64> {
        let Topology::Glued(g) = self else {
            return None;
        };
        g.embodied.as_ref()?;
        let glued = &g.glued;
        let d = glued.domain();
        let a2 = ((2 * a.0 + 1) as f64, (2 * a.1 + 1) as f64);
        let b2 = ((2 * b.0 + 1) as f64, (2 * b.1 + 1) as f64);
        let pieces = glued.pieces_half(a2, b2);
        for w in pieces.windows(2) {
            let (f, iso) = (w[0].1, w[0].2);
            let q = (a2.0 + (b2.0 - a2.0) * f, a2.1 + (b2.1 - a2.1) * f);
            // The crossing in the canonical frame of the piece before it: on D's boundary.
            let (px, pz) = iso.point_f(q.0 / 2.0, q.1 / 2.0);
            let sides = [
                (Side::West, (px - d.x0 as f64).abs()),
                (Side::East, (px - d.x1 as f64).abs()),
                (Side::North, (pz - d.z0 as f64).abs()),
                (Side::South, (pz - d.z1 as f64).abs()),
            ];
            let near = sides.iter().map(|s| s.1).fold(f64::INFINITY, f64::min);
            for (s, e) in sides {
                if e <= near + 1e-6 && glued.edge(s).is_some_and(|e| e.stopped) {
                    return Some(f);
                }
            }
        }
        None
    }

    /// The pieces of a chart walk from canonical cell `a` to chart cell `b`, between seams: each
    /// with its fraction span and the motion chart → canonical, so the walk's position anywhere in
    /// a piece is that motion of the straight line (a closed form per piece).
    pub fn pieces(&self, a: (i64, i64), b: (i64, i64)) -> Vec<Piece> {
        let a2 = ((2 * a.0 + 1) as f64, (2 * a.1 + 1) as f64);
        let b2 = ((2 * b.0 + 1) as f64, (2 * b.1 + 1) as f64);
        self.pieces_half(a2, b2)
    }

    pub(crate) fn pieces_half(&self, a2: (f64, f64), b2: (f64, f64)) -> Vec<Piece> {
        let raw: Vec<(f64, f64, Iso)> = match self {
            Topology::Flat { .. } => vec![(0.0, 1.0, Iso::IDENTITY)],
            Topology::XWrap { x0, period, .. } => {
                let (o, s) = (2.0 * *x0 as f64, 2.0 * *period as f64);
                let mut fs = vec![0.0f64, 1.0];
                if a2.0 != b2.0 {
                    let (ia, ib) = (((a2.0 - o) / s).floor(), ((b2.0 - o) / s).floor());
                    for i in (ia.min(ib) as i64 + 1)..=(ia.max(ib) as i64) {
                        let f = (o + i as f64 * s - a2.0) / (b2.0 - a2.0);
                        if f > 0.0 && f < 1.0 {
                            fs.push(f);
                        }
                    }
                }
                fs.sort_by(f64::total_cmp);
                fs.dedup();
                fs.windows(2)
                    .map(|w| {
                        let m = 0.5 * (w[0] + w[1]);
                        let mx = a2.0 + (b2.0 - a2.0) * m;
                        let cell = (mx / 2.0).floor() as i64;
                        (w[0], w[1], self.fold(cell, 0).iso)
                    })
                    .collect()
            }
            Topology::Glued(g) => g.glued.pieces_half(a2, b2),
        };
        raw.into_iter()
            .map(|(f0, f1, iso)| Piece { f0, f1, iso })
            .collect()
    }
}

fn sat_i32(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// A grid of square cells on a world: canonical cell keys, neighbours across seams.
///
/// When the grid is **aligned** (the domain and every edge motion are whole cells, as Deep Time's
/// 64- and 128-block grids are on a Guyou world: b is a multiple of 256) a cell folds whole onto
/// a canonical cell. Otherwise a cell is keyed by where its centre block folds, which is
/// deterministic but approximate along a seam (TP5); index grids (regions, prey cells) never rely
/// on it: they fold rectangles ([`Topology::fold_rect`]).
#[derive(Clone, Copy, Debug)]
pub struct Grid<'a> {
    topo: &'a Topology,
    cell: i64,
}

impl Grid<'_> {
    pub fn cell_blocks(&self) -> i64 {
        self.cell
    }

    /// Whether cells fold whole onto cells.
    pub fn aligned(&self) -> bool {
        let c = self.cell;
        match self.topo {
            Topology::Flat { .. } => true,
            Topology::XWrap { x0, period, .. } => x0.rem_euclid(c) == 0 && period % c == 0,
            Topology::Glued(g) => {
                let d = g.glued.domain();
                [d.x0, d.z0, d.width(), d.depth()]
                    .iter()
                    .all(|v| v.rem_euclid(c) == 0)
                    && g.glued
                        .edges()
                        .all(|e| e.iso.tx.rem_euclid(c) == 0 && e.iso.tz.rem_euclid(c) == 0)
            }
        }
    }

    /// A chart cell brought to its canonical cell, with the quarter turns a heading takes there.
    pub fn fold(&self, cx: i64, cz: i64) -> (i64, i64, u8) {
        if self.topo.is_flat() {
            return (cx, cz, 0);
        }
        let c = self.cell;
        let f = self.topo.fold(cx * c + c / 2, cz * c + c / 2);
        (f.x.div_euclid(c), f.z.div_euclid(c), f.iso.k)
    }

    pub fn canonical(&self, cx: i64, cz: i64) -> (i64, i64) {
        let (x, z, _) = self.fold(cx, cz);
        (x, z)
    }

    /// The canonical cell of a block.
    pub fn of_block(&self, x: i64, z: i64) -> (i64, i64) {
        let (x, z) = self.topo.canonical(x, z);
        (x.div_euclid(self.cell), z.div_euclid(self.cell))
    }

    /// The cell next to canonical cell `(cx, cz)` one step `(dx, dz)` away, across a seam if need
    /// be, and the quarter turns a heading takes on the way.
    pub fn neighbour(&self, cx: i64, cz: i64, dx: i64, dz: i64) -> (i64, i64, u8) {
        self.fold(cx + dx, cz + dz)
    }

    /// The chart cell of canonical cell `to` nearest canonical cell `from` (cells, not blocks).
    pub fn toward(&self, from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
        if self.topo.is_flat() {
            return to;
        }
        let c = self.cell;
        let centre = |p: (i64, i64)| (p.0 * c + c / 2, p.1 * c + c / 2);
        let t = self.topo.toward(centre(from), centre(to));
        (t.0.div_euclid(c), t.1.div_euclid(c))
    }
}

/// Every frame's topology. A frame nobody described is flat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Topologies {
    frames: BTreeMap<u16, Topology>,
}

impl Topologies {
    pub fn new() -> Topologies {
        Topologies::default()
    }

    pub fn of(&self, frame: u16) -> &Topology {
        self.frames.get(&frame).unwrap_or(&FLAT)
    }

    /// Sets a frame's topology; a flat one without a border is the default and is not kept.
    pub fn set(&mut self, frame: u16, t: Topology) {
        if t == FLAT {
            self.frames.remove(&frame);
        } else {
            self.frames.insert(frame, t);
        }
    }

    /// Every frame is flat: modules take their old paths.
    pub fn all_flat(&self) -> bool {
        self.frames.values().all(Topology::is_flat)
    }

    pub fn frames(&self) -> impl Iterator<Item = (u16, &Topology)> {
        self.frames.iter().map(|(f, t)| (*f, t))
    }

    pub fn shared(self) -> SharedTopologies {
        Arc::new(self)
    }

    // ---- the operations, by frame ---------------------------------------------------------------

    pub fn canonical(&self, frame: u16, x: i64, z: i64) -> (i64, i64) {
        self.of(frame).canonical(x, z)
    }

    pub fn dist_sq(&self, frame: u16, a: (i64, i64), b: (i64, i64)) -> u64 {
        self.of(frame).dist_sq(a, b)
    }

    pub fn dist(&self, frame: u16, a: (i64, i64), b: (i64, i64)) -> f64 {
        self.of(frame).dist(a, b)
    }

    pub fn toward(&self, frame: u16, from: (i64, i64), to: (i64, i64)) -> (i64, i64) {
        self.of(frame).toward(from, to)
    }
}

/// The topologies the modules share: replaced whole when a TOPOLOGY record changes one, never
/// written in place, so reading costs nothing (TP6).
pub type SharedTopologies = Arc<Topologies>;

static ALL_FLAT: LazyLock<SharedTopologies> = LazyLock::new(|| Arc::new(Topologies::default()));

/// The shared all-flat topologies: every module's default.
pub fn all_flat() -> SharedTopologies {
    ALL_FLAT.clone()
}
