// SPDX-License-Identifier: AGPL-3.0-or-later
//! The closed set of mark kinds (VIEWER.md §4.3), mirrored from mic-viewer's `api::Mark` field for
//! field, so the engine can write marks without linking the viewer's server, and the sidecar turns
//! each into the viewer's own with one `match` (rtsim-view's `parts`).
//!
//! The JSON forms are the same as mic-viewer's (`k`-tagged, the same field names), and the binary
//! form here is what parts carry.

use rtsim_kernel::codec::{CodecError, Reader, Writer};
use serde::{Deserialize, Serialize};

/// A game tick, as the viewer counts them.
pub type Tick = i64;

/// One mark, tagged by `k`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
pub enum Mark {
    Point {
        id: String,
        x: f64,
        z: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ci: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        c: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        g: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        s: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        l: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ring: bool,
    },
    Leg {
        id: String,
        p: Vec<[f64; 4]>,
        t0: Tick,
        t1: Tick,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ci: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        c: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        g: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        l: Option<String>,
    },
    Path {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        p: Vec<[f64; 2]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ci: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        c: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        w: Option<f64>,
    },
    Edge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        a: [f64; 2],
        b: [f64; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        w: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ci: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        c: Option<String>,
    },
    Area {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        x: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        z: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#box: Option<[f64; 4]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ci: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        c: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        l: Option<String>,
    },
    Cell {
        x0: f64,
        z0: f64,
        size: f64,
        w: u32,
        h: u32,
        v: Vec<Option<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ramp: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lo: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hi: Option<f64>,
    },
    Cluster {
        x: f64,
        z: f64,
        n: u64,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        by: std::collections::BTreeMap<String, u64>,
    },
    Label {
        x: f64,
        z: f64,
        text: String,
    },
}

/// Row kinds 1 to 8 are the mark kinds, in this order. Kinds 16 and 17 are reserved for the glass
/// box's graph rows (V6: nodes, edges with rates); 32 and 33 are the archive's pending rows. A
/// reader skips any kind it does not know by the row's length; a row that is not a mark comes back
/// as its frame and its body's bytes (`part::Row::body`).
pub mod kind {
    pub const POINT: u8 = 1;
    pub const LEG: u8 = 2;
    pub const PATH: u8 = 3;
    pub const EDGE: u8 = 4;
    pub const AREA: u8 = 5;
    pub const CELL: u8 = 6;
    pub const CLUSTER: u8 = 7;
    pub const LABEL: u8 = 8;
    /// Reserved for V6: a graph node (a district's machine or store).
    pub const GRAPH_NODE: u8 = 16;
    /// Reserved for V6: a graph edge with its rate.
    pub const GRAPH_EDGE: u8 = 17;
    /// An event archive row recorded since the last save: its 32 bytes (rtsim-archive's row),
    /// then its payload's, if any (VIEWER.md §10.2).
    pub const ARCHIVE_ROW: u8 = 32;
    /// An archive dictionary name the pending rows use: a byte (0 a kind, 1 a frame), the
    /// number, the name.
    pub const ARCHIVE_NAME: u8 = 33;

    /// The kind's name, as descriptors and `/api/layers` spell it.
    pub fn name(k: u8) -> &'static str {
        match k {
            POINT => "point",
            LEG => "leg",
            PATH => "path",
            EDGE => "edge",
            AREA => "area",
            CELL => "cell",
            CLUSTER => "cluster",
            LABEL => "label",
            GRAPH_NODE => "graph_node",
            GRAPH_EDGE => "graph_edge",
            ARCHIVE_ROW => "archive_row",
            ARCHIVE_NAME => "archive_name",
            _ => "unknown",
        }
    }
}

impl Mark {
    /// A point with only its id and place.
    pub fn point(id: impl ToString, x: f64, z: f64) -> Mark {
        Mark::Point {
            id: id.to_string(),
            x,
            z,
            ci: None,
            c: None,
            g: None,
            s: None,
            l: None,
            ring: false,
        }
    }

    /// A straight leg.
    pub fn leg(id: impl ToString, a: [f64; 2], b: [f64; 2], t0: Tick, t1: Tick) -> Mark {
        Mark::Leg {
            id: id.to_string(),
            p: vec![[a[0], a[1], b[0], b[1]]],
            t0,
            t1,
            ci: None,
            c: None,
            g: None,
            l: None,
        }
    }

    pub fn edge(a: [f64; 2], b: [f64; 2], w: f64) -> Mark {
        Mark::Edge {
            id: None,
            a,
            b,
            w: Some(w),
            ci: None,
            c: None,
        }
    }

    pub fn disc(id: Option<String>, x: f64, z: f64, r: f64) -> Mark {
        Mark::Area {
            id,
            x: Some(x),
            z: Some(z),
            r: Some(r),
            r#box: None,
            ci: None,
            c: None,
            l: None,
        }
    }

    pub fn rect(id: Option<String>, b: [f64; 4]) -> Mark {
        Mark::Area {
            id,
            x: None,
            z: None,
            r: None,
            r#box: Some(b),
            ci: None,
            c: None,
            l: None,
        }
    }

    /// Sets the legend index (no-op for kinds without one).
    pub fn with_ci(mut self, v: u16) -> Mark {
        match &mut self {
            Mark::Point { ci, .. }
            | Mark::Leg { ci, .. }
            | Mark::Path { ci, .. }
            | Mark::Edge { ci, .. }
            | Mark::Area { ci, .. } => *ci = Some(v),
            _ => {}
        }
        self
    }

    /// Sets an explicit colour.
    pub fn with_color(mut self, v: &str) -> Mark {
        match &mut self {
            Mark::Point { c, .. }
            | Mark::Leg { c, .. }
            | Mark::Path { c, .. }
            | Mark::Edge { c, .. }
            | Mark::Area { c, .. } => *c = Some(v.to_owned()),
            _ => {}
        }
        self
    }

    /// Sets the label (points, legs, areas).
    pub fn with_label(mut self, v: impl Into<String>) -> Mark {
        match &mut self {
            Mark::Point { l, .. } | Mark::Leg { l, .. } | Mark::Area { l, .. } => {
                *l = Some(v.into())
            }
            _ => {}
        }
        self
    }

    /// Sets a point's size factor.
    pub fn with_size(mut self, v: f64) -> Mark {
        if let Mark::Point { s, .. } = &mut self {
            *s = Some(v);
        }
        self
    }

    /// Sets a point's or leg's glyph.
    pub fn with_glyph(mut self, v: &str) -> Mark {
        match &mut self {
            Mark::Point { g, .. } | Mark::Leg { g, .. } => *g = Some(v.to_owned()),
            _ => {}
        }
        self
    }

    pub fn with_ring(mut self) -> Mark {
        if let Mark::Point { ring, .. } = &mut self {
            *ring = true;
        }
        self
    }

    /// The row kind ([`kind`]).
    pub fn kind(&self) -> u8 {
        match self {
            Mark::Point { .. } => kind::POINT,
            Mark::Leg { .. } => kind::LEG,
            Mark::Path { .. } => kind::PATH,
            Mark::Edge { .. } => kind::EDGE,
            Mark::Area { .. } => kind::AREA,
            Mark::Cell { .. } => kind::CELL,
            Mark::Cluster { .. } => kind::CLUSTER,
            Mark::Label { .. } => kind::LABEL,
        }
    }

    /// Its bounding box: x0, z0, x1, z1 (all zero for a mark with no extent), as mic-viewer's.
    pub fn bbox(&self) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut add = |x: f64, z: f64| {
            b[0] = b[0].min(x);
            b[1] = b[1].min(z);
            b[2] = b[2].max(x);
            b[3] = b[3].max(z);
        };
        match self {
            Mark::Point { x, z, .. } | Mark::Cluster { x, z, .. } | Mark::Label { x, z, .. } => {
                add(*x, *z)
            }
            Mark::Leg { p, .. } => {
                for q in p {
                    add(q[0], q[1]);
                    add(q[2], q[3]);
                }
            }
            Mark::Path { p, .. } => {
                for q in p {
                    add(q[0], q[1]);
                }
            }
            Mark::Edge { a, b: e, .. } => {
                add(a[0], a[1]);
                add(e[0], e[1]);
            }
            Mark::Area { x, z, r, r#box, .. } => {
                if let Some(bx) = r#box {
                    add(bx[0], bx[1]);
                    add(bx[2], bx[3]);
                } else if let (Some(x), Some(z)) = (x, z) {
                    let r = r.unwrap_or(0.0);
                    add(x - r, z - r);
                    add(x + r, z + r);
                }
            }
            Mark::Cell {
                x0, z0, size, w, h, ..
            } => {
                add(*x0, *z0);
                add(x0 + size * f64::from(*w), z0 + size * f64::from(*h));
            }
        }
        if !b[0].is_finite() {
            return [0.0; 4];
        }
        b
    }

    /// Where the mark counts for its region: a leg's start, an edge's middle, else its box's
    /// centre (mic-viewer's `anchor` before the leg starts).
    pub fn anchor(&self) -> [f64; 2] {
        match self {
            Mark::Leg { p, .. } => p.first().map_or([0.0, 0.0], |q| [q[0], q[1]]),
            Mark::Edge { a, b, .. } => [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
            _ => {
                let b = self.bbox();
                [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]
            }
        }
    }

    /// The thing's id, for marks that have one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Mark::Point { id, .. } | Mark::Leg { id, .. } => Some(id),
            Mark::Path { id, .. } | Mark::Edge { id, .. } | Mark::Area { id, .. } => id.as_deref(),
            _ => None,
        }
    }

    /// The mark's body in the part format (everything after the row's kind and length).
    pub fn encode_body(&self, w: &mut Writer) {
        match self {
            Mark::Point {
                id,
                x,
                z,
                ci,
                c,
                g,
                s,
                l,
                ring,
            } => {
                w.bytes(id.as_bytes());
                w.f64(*x);
                w.f64(*z);
                opt_u16(w, *ci);
                opt_str(w, c);
                opt_str(w, g);
                opt_f64(w, *s);
                opt_str(w, l);
                w.bool(*ring);
            }
            Mark::Leg {
                id,
                p,
                t0,
                t1,
                ci,
                c,
                g,
                l,
            } => {
                w.bytes(id.as_bytes());
                w.var_u64(p.len() as u64);
                for q in p {
                    for v in q {
                        w.f64(*v);
                    }
                }
                w.var_i64(*t0);
                w.var_i64(*t1);
                opt_u16(w, *ci);
                opt_str(w, c);
                opt_str(w, g);
                opt_str(w, l);
            }
            Mark::Path {
                id,
                p,
                ci,
                c,
                w: wd,
            } => {
                opt_str(w, id);
                w.var_u64(p.len() as u64);
                for q in p {
                    w.f64(q[0]);
                    w.f64(q[1]);
                }
                opt_u16(w, *ci);
                opt_str(w, c);
                opt_f64(w, *wd);
            }
            Mark::Edge {
                id,
                a,
                b,
                w: wd,
                ci,
                c,
            } => {
                opt_str(w, id);
                for v in a.iter().chain(b) {
                    w.f64(*v);
                }
                opt_f64(w, *wd);
                opt_u16(w, *ci);
                opt_str(w, c);
            }
            Mark::Area {
                id,
                x,
                z,
                r,
                r#box,
                ci,
                c,
                l,
            } => {
                opt_str(w, id);
                opt_f64(w, *x);
                opt_f64(w, *z);
                opt_f64(w, *r);
                match r#box {
                    Some(b) => {
                        w.bool(true);
                        for v in b {
                            w.f64(*v);
                        }
                    }
                    None => w.bool(false),
                }
                opt_u16(w, *ci);
                opt_str(w, c);
                opt_str(w, l);
            }
            Mark::Cell {
                x0,
                z0,
                size,
                w: cw,
                h,
                v,
                ramp,
                lo,
                hi,
            } => {
                w.f64(*x0);
                w.f64(*z0);
                w.f64(*size);
                w.var_u64(u64::from(*cw));
                w.var_u64(u64::from(*h));
                w.var_u64(v.len() as u64);
                for x in v {
                    opt_f64(w, *x);
                }
                opt_str(w, ramp);
                opt_f64(w, *lo);
                opt_f64(w, *hi);
            }
            Mark::Cluster { x, z, n, by } => {
                w.f64(*x);
                w.f64(*z);
                w.var_u64(*n);
                w.var_u64(by.len() as u64);
                for (k, v) in by {
                    w.bytes(k.as_bytes());
                    w.var_u64(*v);
                }
            }
            Mark::Label { x, z, text } => {
                w.f64(*x);
                w.f64(*z);
                w.bytes(text.as_bytes());
            }
        }
    }

    /// A mark of kind `k` from its body, or `None` for a kind that is not a mark.
    pub fn decode_body(k: u8, r: &mut Reader<'_>) -> Result<Option<Mark>, CodecError> {
        Ok(Some(match k {
            kind::POINT => Mark::Point {
                id: string(r)?,
                x: r.f64()?,
                z: r.f64()?,
                ci: get_u16(r)?,
                c: get_str(r)?,
                g: get_str(r)?,
                s: get_f64(r)?,
                l: get_str(r)?,
                ring: r.bool()?,
            },
            kind::LEG => {
                let id = string(r)?;
                let n = r.len_prefix(32)?;
                let mut p = Vec::with_capacity(n);
                for _ in 0..n {
                    p.push([r.f64()?, r.f64()?, r.f64()?, r.f64()?]);
                }
                Mark::Leg {
                    id,
                    p,
                    t0: r.var_i64()?,
                    t1: r.var_i64()?,
                    ci: get_u16(r)?,
                    c: get_str(r)?,
                    g: get_str(r)?,
                    l: get_str(r)?,
                }
            }
            kind::PATH => {
                let id = get_str(r)?;
                let n = r.len_prefix(16)?;
                let mut p = Vec::with_capacity(n);
                for _ in 0..n {
                    p.push([r.f64()?, r.f64()?]);
                }
                Mark::Path {
                    id,
                    p,
                    ci: get_u16(r)?,
                    c: get_str(r)?,
                    w: get_f64(r)?,
                }
            }
            kind::EDGE => Mark::Edge {
                id: get_str(r)?,
                a: [r.f64()?, r.f64()?],
                b: [r.f64()?, r.f64()?],
                w: get_f64(r)?,
                ci: get_u16(r)?,
                c: get_str(r)?,
            },
            kind::AREA => Mark::Area {
                id: get_str(r)?,
                x: get_f64(r)?,
                z: get_f64(r)?,
                r: get_f64(r)?,
                r#box: if r.bool()? {
                    Some([r.f64()?, r.f64()?, r.f64()?, r.f64()?])
                } else {
                    None
                },
                ci: get_u16(r)?,
                c: get_str(r)?,
                l: get_str(r)?,
            },
            kind::CELL => {
                let (x0, z0, size) = (r.f64()?, r.f64()?, r.f64()?);
                let w = u32::try_from(r.var_u64()?)
                    .map_err(|_| CodecError::Invalid("cell w".into()))?;
                let h = u32::try_from(r.var_u64()?)
                    .map_err(|_| CodecError::Invalid("cell h".into()))?;
                let n = r.len_prefix(1)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(get_f64(r)?);
                }
                Mark::Cell {
                    x0,
                    z0,
                    size,
                    w,
                    h,
                    v,
                    ramp: get_str(r)?,
                    lo: get_f64(r)?,
                    hi: get_f64(r)?,
                }
            }
            kind::CLUSTER => {
                let (x, z, n) = (r.f64()?, r.f64()?, r.var_u64()?);
                let m = r.len_prefix(2)?;
                let mut by = std::collections::BTreeMap::new();
                for _ in 0..m {
                    by.insert(string(r)?, r.var_u64()?);
                }
                Mark::Cluster { x, z, n, by }
            }
            kind::LABEL => Mark::Label {
                x: r.f64()?,
                z: r.f64()?,
                text: string(r)?,
            },
            _ => return Ok(None),
        }))
    }
}

pub(crate) fn opt_str(w: &mut Writer, v: &Option<String>) {
    match v {
        Some(s) => {
            w.bool(true);
            w.bytes(s.as_bytes());
        }
        None => w.bool(false),
    }
}

fn opt_f64(w: &mut Writer, v: Option<f64>) {
    match v {
        Some(x) => {
            w.bool(true);
            w.f64(x);
        }
        None => w.bool(false),
    }
}

fn opt_u16(w: &mut Writer, v: Option<u16>) {
    match v {
        Some(x) => {
            w.bool(true);
            w.u16(x);
        }
        None => w.bool(false),
    }
}

pub(crate) fn string(r: &mut Reader<'_>) -> Result<String, CodecError> {
    String::from_utf8(r.bytes()?.to_vec()).map_err(|_| CodecError::Invalid("utf-8".into()))
}

pub(crate) fn get_str(r: &mut Reader<'_>) -> Result<Option<String>, CodecError> {
    Ok(if r.bool()? { Some(string(r)?) } else { None })
}

fn get_f64(r: &mut Reader<'_>) -> Result<Option<f64>, CodecError> {
    Ok(if r.bool()? { Some(r.f64()?) } else { None })
}

fn get_u16(r: &mut Reader<'_>) -> Result<Option<u16>, CodecError> {
    Ok(if r.bool()? { Some(r.u16()?) } else { None })
}
