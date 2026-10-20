// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layouts: Deep Time's layout block read into a [`Topology`], and the synthetic worlds the tests
//! use (TP10).
//!
//! The block is linked, not mirrored: [`ProjectionSpec`] is Deep Time's own type
//! (`deeptime_format::projection`, LGPL-3.0-or-later, linked into this AGPL crate), parsed from a
//! world's `manifest.json` `projection` (FORMAT.md "The world's layout", stable from format 0.5).
//! Its edges are data only (`iso: [k, tx, tz]`, the wrap engine's `CellIsometry`): the group they
//! generate, the folds and the shortest images are this crate's ([`super::Glued`]).

use deeptime_format::projection::{Axis, PetroffGuyou, ProjectionSpec, Side as DtSide};

use super::glued::{Edge, Side};
use super::iso::{Iso, Rect};
use super::{Kind, Topology};

/// The 16k Guyou world's hemisphere side b, blocks (FORMAT.md: 13,312 × 6,656).
pub const GUYOU_16K: i64 = 6_656;
/// The 32k one's (26,112 × 13,056).
pub const GUYOU_32K: i64 = 13_056;

fn whole(v: f64, what: &str) -> Result<i64, String> {
    if v.fract() != 0.0 || !v.is_finite() {
        return Err(format!("{what} {v} is not a block boundary"));
    }
    Ok(v as i64)
}

/// The topology a layout block describes. `installed` is whether the wrap engine has installed
/// the layout on the level: until it has, the border's edge_stop edges are stopped seams.
pub fn from_projection(spec: &ProjectionSpec, installed: bool) -> Result<Topology, String> {
    let d = &spec.domain;
    // A glued side's line is a block boundary; a side nothing joins (a Lambert world's pole
    // caps) is a wall, rounded outward to whole blocks.
    let joined = |axis: Axis| spec.edges.iter().any(|e| e.axis == axis);
    let bound = |v: f64, what: &str, axis: Axis, low: bool| -> Result<i64, String> {
        if joined(axis) {
            whole(v, what)
        } else if low {
            Ok(v.floor() as i64)
        } else {
            Ok(v.ceil() as i64)
        }
    };
    let domain = Rect::new(
        bound(d.x_min, "x_min", Axis::X, true)?,
        bound(d.z_min, "z_min", Axis::Z, true)?,
        bound(d.x_max, "x_max", Axis::X, false)?,
        bound(d.z_max, "z_max", Axis::Z, false)?,
    );
    let stops = |name: &str| {
        !installed
            && spec
                .border
                .as_ref()
                .is_some_and(|b| b.kind == "edge_stop" && b.edges.iter().any(|e| e == name))
    };
    let mut edges = Vec::with_capacity(spec.edges.len());
    for e in &spec.edges {
        let side = match (e.axis, e.side) {
            (Axis::X, DtSide::Min) => Side::West,
            (Axis::X, DtSide::Max) => Side::East,
            (Axis::Z, DtSide::Min) => Side::North,
            (Axis::Z, DtSide::Max) => Side::South,
        };
        edges.push(Edge {
            side,
            iso: Iso::new(e.iso[0], e.iso[1], e.iso[2]),
            stopped: stops(&e.name),
        });
    }
    Topology::glued(domain, &edges)
}

/// Parses a layout block from its JSON (the `projection` object of a manifest).
pub fn from_projection_json(json: &str, installed: bool) -> Result<Topology, String> {
    let spec: ProjectionSpec =
        serde_json::from_str(json).map_err(|e| format!("not a layout block: {e}"))?;
    from_projection(&spec, installed)
}

/// Deep Time's Petroff–Guyou layout block for hemisphere side `b` (λc does not change the
/// gluing), with its edge_stop border on the north and south edges.
pub fn guyou_spec(b: u32) -> ProjectionSpec {
    let border = deeptime_format::projection::BorderSpec {
        kind: "edge_stop".into(),
        edges: vec!["north".into(), "south".into()],
        inset_blocks: 1.0,
        temporary: true,
    };
    PetroffGuyou::new(b, 31.25).spec(Some(border))
}

/// A synthetic Petroff–Guyou world of hemisphere side `b`, written out here rather than read
/// from a block (the tests check the two agree): x in [−b, b), z in [−b/2, b/2); west and east
/// slide by ∓2b, north and south are half-turns about (0, ∓b/2). `edge_stop` stops the north
/// and south seams.
pub fn guyou(b: i64, edge_stop: bool) -> Topology {
    let h = b / 2;
    let domain = Rect::new(-b, -h, b, h);
    let edges = [
        Edge {
            side: Side::West,
            iso: Iso::translation(2 * b, 0),
            stopped: false,
        },
        Edge {
            side: Side::East,
            iso: Iso::translation(-2 * b, 0),
            stopped: false,
        },
        Edge {
            side: Side::North,
            iso: Iso::half_turn_about(0, -h),
            stopped: edge_stop,
        },
        Edge {
            side: Side::South,
            iso: Iso::half_turn_about(0, h),
            stopped: edge_stop,
        },
    ];
    Topology::glued(domain, &edges).expect("the Guyou gluing is consistent")
}

/// A torus: both axes slide (Toroidal World looping x and z).
pub fn torus(domain: Rect) -> Topology {
    let (w, h) = (domain.width(), domain.depth());
    let edges = [
        Edge {
            side: Side::West,
            iso: Iso::translation(w, 0),
            stopped: false,
        },
        Edge {
            side: Side::East,
            iso: Iso::translation(-w, 0),
            stopped: false,
        },
        Edge {
            side: Side::North,
            iso: Iso::translation(0, h),
            stopped: false,
        },
        Edge {
            side: Side::South,
            iso: Iso::translation(0, -h),
            stopped: false,
        },
    ];
    Topology::glued(domain, &edges).expect("a torus is consistent")
}

/// Peirce's quincuncial square of side `a`: every edge a half-turn about its midpoint (the
/// layout Deep Time compared and did not ship; here to keep the provider general).
pub fn peirce(a: i64) -> Topology {
    let h = a / 2;
    let domain = Rect::new(-h, -h, h, h);
    let edges = [
        Edge {
            side: Side::West,
            iso: Iso::half_turn_about(-h, 0),
            stopped: false,
        },
        Edge {
            side: Side::East,
            iso: Iso::half_turn_about(h, 0),
            stopped: false,
        },
        Edge {
            side: Side::North,
            iso: Iso::half_turn_about(0, -h),
            stopped: false,
        },
        Edge {
            side: Side::South,
            iso: Iso::half_turn_about(0, h),
            stopped: false,
        },
    ];
    Topology::glued(domain, &edges).expect("the Peirce gluing is consistent")
}

/// One edge of a [`Spec`]: the TOPOLOGY record's edge entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecEdge {
    pub side: Side,
    pub iso: Iso,
    pub stopped: bool,
}

/// What the TOPOLOGY record carries for one frame (docs/boundary.md §11): the provider, its
/// domain or border, and the edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub kind: Kind,
    /// Flat: the border, if any. XWrap: the lap's x range and the border's z range. Glued: the
    /// domain.
    pub rect: Option<Rect>,
    pub edges: Vec<SpecEdge>,
}

impl Spec {
    pub fn flat(border: Option<Rect>) -> Spec {
        Spec {
            kind: Kind::Flat,
            rect: border,
            edges: Vec::new(),
        }
    }

    /// The topology the spec describes.
    pub fn build(&self) -> Result<Topology, String> {
        match self.kind {
            Kind::Flat => Ok(match self.rect {
                Some(b) => Topology::flat_with_border(b),
                None => Topology::flat(),
            }),
            Kind::XWrap => {
                let r = self.rect.ok_or("an x wrap needs its lap")?;
                if r.width() <= 0 {
                    return Err(format!("an x wrap of period {}", r.width()));
                }
                let t = Topology::x_wrap(r.x0, r.width());
                let Topology::XWrap {
                    x0,
                    period,
                    adjacent,
                    ..
                } = t
                else {
                    unreachable!()
                };
                // A z range of the whole i32 span is "no border".
                let border =
                    (r.z0 > i64::from(i32::MIN) || r.z1 < i64::from(i32::MAX)).then_some(r);
                Ok(Topology::XWrap {
                    x0,
                    period,
                    border,
                    adjacent,
                })
            }
            Kind::Glued => {
                let r = self.rect.ok_or("a glued world needs its domain")?;
                let edges: Vec<Edge> = self
                    .edges
                    .iter()
                    .map(|e| Edge {
                        side: e.side,
                        iso: e.iso,
                        stopped: e.stopped,
                    })
                    .collect();
                Topology::glued(r, &edges)
            }
        }
    }

    /// The spec of a topology (what the bridge would send for it).
    pub fn of(t: &Topology) -> Spec {
        match t {
            Topology::Flat { border } => Spec::flat(*border),
            Topology::XWrap {
                x0, period, border, ..
            } => Spec {
                kind: Kind::XWrap,
                rect: Some(Rect::new(
                    *x0,
                    border.map_or(i64::from(i32::MIN), |b| b.z0),
                    *x0 + *period,
                    border.map_or(i64::from(i32::MAX), |b| b.z1),
                )),
                edges: Vec::new(),
            },
            Topology::Glued(g) => Spec {
                kind: Kind::Glued,
                rect: Some(g.glued.domain()),
                edges: g
                    .glued
                    .edges()
                    .map(|e| SpecEdge {
                        side: e.side,
                        iso: e.iso,
                        stopped: e.stopped,
                    })
                    .collect(),
            },
        }
    }
}
