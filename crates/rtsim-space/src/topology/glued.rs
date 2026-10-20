// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glued provider: a rectangular fundamental domain whose edges are glued by grid motions
//! (Deep Time's layout block, FORMAT.md "The world's layout"; choices TP2 to TP5).
//!
//! **The cover.** Unfold the world: copies ("tiles") of the domain D tile the plane, each the image
//! of D by one motion of the group the edge motions generate. Every edge motion takes a tile onto
//! a neighbouring grid cell of D's size, so the tiles form a grid and tile (i, j) is the rectangle
//! `[x0 + iW, x0 + (i + 1)W) × [z0 + jH, z0 + (j + 1)H)`. A straight walk in the cover is a
//! straight walk on the world; it crosses a seam where it leaves a tile. The tile at (i, j) and its
//! motion h (canonical → tile) are found by a breadth-first walk over the edges from D, which also
//! checks the gluing is consistent (two ways round to one tile must give one motion).
//!
//! **Shortest displacement** between canonical cells a and b is the shortest vector from a to any
//! image h(b). Every image lies in its tile, so a tile whose gap from D exceeds the best found so
//! far cannot beat it: the near tiles are kept sorted by that gap, and the search stops at the
//! first tile beyond the best (TP3). The identity alone bounds the answer by D's diameter, so
//! only tiles within it are kept. Ties go to the first tile in that order, so every reader agrees.
//!
//! **Walls.** A side without an edge is a wall: nothing is beyond it. A cell past a wall is
//! clamped (and reported so), and rectangles past it are dropped. Deep Time's edge_stop border is
//! *not* a wall here: stopped seams exist for distance and are walls only in [`Glued::embodied`].

use std::collections::{BTreeMap, VecDeque};

use super::iso::{Iso, Rect};

/// A side of the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Side {
    /// x min (beyond is x < x0).
    West = 0,
    /// x max (beyond is x ≥ x1).
    East = 1,
    /// z min (beyond is z < z0).
    North = 2,
    /// z max (beyond is z ≥ z1, cells).
    South = 3,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::West, Side::East, Side::North, Side::South];

    pub fn from_u8(v: u8) -> Option<Side> {
        Side::ALL.get(usize::from(v)).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::West => "west",
            Side::East => "east",
            Side::North => "north",
            Side::South => "south",
        }
    }
}

/// One glued edge: the motion that takes what lies beyond it back inside (`edges[].iso`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub side: Side,
    /// Beyond → inside, as the wrap engine's `CellIsometry`.
    pub iso: Iso,
    /// Deep Time's temporary edge_stop border holds here: the seam exists for distance, but
    /// players, their vehicles and ships cannot cross it, so embodied things must not either.
    pub stopped: bool,
}

/// A tile of the cover near the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tile {
    pub i: i64,
    pub j: i64,
    /// Canonical → this tile.
    pub h: Iso,
    /// The squared gap between this tile and D, doubled units.
    pub gap2: i64,
}

/// Folds past this many seams are given up and clamped: a position that far from the world is a
/// bug upstream, and this keeps a bad input from looping.
const MAX_FOLDS: usize = 4096;

/// A world glued from a rectangular domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glued {
    /// The domain, cells: x in [x0, x1), z in [z0, z1).
    pub(crate) d: Rect,
    pub(crate) edges: [Option<Edge>; 4],
    /// Tiles within D's diameter, by (gap, |i| + |j|, i, j).
    pub(crate) near: Vec<Tile>,
    /// The motions of the tiles touching D (gap 0), identity first: every image a short query
    /// (shorter than the domain's narrow side) can need.
    pub(crate) adjacent: Vec<Iso>,
    /// Cone points: block corners in D's closure fixed by a turn of the group, doubled units.
    pub(crate) cones: Vec<(i64, i64)>,
    /// Both x edges slide by the width (Toroidal World's wrap): fold x in one step.
    slides_x: bool,
    slides_z: bool,
}

/// What a fold did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Folded {
    pub x: i64,
    pub z: i64,
    /// Cover → canonical: `(x, z) = iso.cell(input)`.
    pub iso: Iso,
    /// The input lay past a wall and was clamped (no motion brings it inside).
    pub clamped: bool,
}

fn gap2(a: &Rect, b: &Rect) -> i64 {
    // Doubled units: gaps between cell rectangles' closures (saturating: a domain unbounded along
    // one axis, a z-only loop, has tiles astronomically far apart there).
    let gx = (b.x0 - a.x1).max(a.x0 - b.x1).max(0).saturating_mul(2);
    let gz = (b.z0 - a.z1).max(a.z0 - b.z1).max(0).saturating_mul(2);
    gx.saturating_mul(gx).saturating_add(gz.saturating_mul(gz))
}

impl Glued {
    /// A glued world on `domain` (cells) with `edges` (at most one per side). Fails when an edge
    /// does not take its side's neighbouring tile onto the domain, when a turn would lay a tile
    /// across the grid (an odd turn of a domain that is not square), or when two ways round give
    /// one tile two motions.
    pub fn new(domain: Rect, edges: &[Edge]) -> Result<Glued, String> {
        if domain.is_empty() {
            return Err(format!("empty domain {domain:?}"));
        }
        let (w, h) = (domain.width(), domain.depth());
        let mut by_side: [Option<Edge>; 4] = [None; 4];
        for e in edges {
            if by_side[e.side as usize].is_some() {
                return Err(format!("two edges on the {} side", e.side.name()));
            }
            // The tile beyond the side, by grid position, must be what the motion's inverse makes
            // of D.
            let want = match e.side {
                Side::West => Rect::new(domain.x0 - w, domain.z0, domain.x0, domain.z1),
                Side::East => Rect::new(domain.x1, domain.z0, domain.x1 + w, domain.z1),
                Side::North => Rect::new(domain.x0, domain.z0 - h, domain.x1, domain.z0),
                Side::South => Rect::new(domain.x0, domain.z1, domain.x1, domain.z1 + h),
            };
            let got = e.iso.inverse().rect(domain);
            if got != want {
                return Err(format!(
                    "the {} edge's motion {:?} takes the domain onto {got:?}, not the tile beyond it {want:?}",
                    e.side.name(),
                    e.iso
                ));
            }
            by_side[e.side as usize] = Some(*e);
        }
        let slide = |s: Side, t: Iso| by_side[s as usize].is_some_and(|e| e.iso == t);
        let slides_x =
            slide(Side::West, Iso::translation(w, 0)) && slide(Side::East, Iso::translation(-w, 0));
        let slides_z = slide(Side::North, Iso::translation(0, h))
            && slide(Side::South, Iso::translation(0, -h));
        // The tiles within the domain's diameter (doubled units), breadth first over the edges.
        let reach2 = w
            .saturating_mul(w)
            .saturating_add(h.saturating_mul(h))
            .saturating_mul(4);
        let mut seen: BTreeMap<(i64, i64), Iso> = BTreeMap::new();
        seen.insert((0, 0), Iso::IDENTITY);
        let mut queue = VecDeque::from([(0i64, 0i64)]);
        while let Some((i, j)) = queue.pop_front() {
            let here = seen[&(i, j)];
            for e in by_side.iter().flatten() {
                let next = here.compose(e.iso.inverse());
                let r = next.rect(domain);
                if r.width() != w || r.depth() != h {
                    return Err(format!(
                        "the motion {next:?} turns the {w} x {h} domain across the grid"
                    ));
                }
                let (dx, dz) = (r.x0 - domain.x0, r.z0 - domain.z0);
                if dx.rem_euclid(w) != 0 || dz.rem_euclid(h) != 0 {
                    return Err(format!(
                        "the motion {next:?} lays a tile off the grid at {r:?}"
                    ));
                }
                let key = (dx.div_euclid(w), dz.div_euclid(h));
                if let Some(&old) = seen.get(&key) {
                    if old != next {
                        return Err(format!(
                            "inconsistent gluing: tile {key:?} is reached as {old:?} and as {next:?}"
                        ));
                    }
                    continue;
                }
                if gap2(&domain, &r) <= reach2 {
                    seen.insert(key, next);
                    queue.push_back(key);
                }
            }
        }
        let mut near: Vec<Tile> = seen
            .iter()
            .map(|(&(i, j), &t)| Tile {
                i,
                j,
                h: t,
                gap2: gap2(&domain, &t.rect(domain)),
            })
            .collect();
        near.sort_by_key(|t| (t.gap2, t.i.abs() + t.j.abs(), t.i, t.j));
        let adjacent: Vec<Iso> = near.iter().filter(|t| t.gap2 == 0).map(|t| t.h).collect();
        let mut cones: Vec<(i64, i64)> = adjacent
            .iter()
            .filter_map(|t| t.fixed_half())
            .filter(|&(x, z)| {
                x >= 2 * domain.x0 && x <= 2 * domain.x1 && z >= 2 * domain.z0 && z <= 2 * domain.z1
            })
            .collect();
        cones.sort_unstable();
        cones.dedup();
        Ok(Glued {
            d: domain,
            edges: by_side,
            near,
            adjacent,
            cones,
            slides_x,
            slides_z,
        })
    }

    pub fn domain(&self) -> Rect {
        self.d
    }

    pub fn edge(&self, s: Side) -> Option<&Edge> {
        self.edges[s as usize].as_ref()
    }

    pub fn edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges.iter().flatten()
    }

    /// Whether any edge is under edge_stop.
    pub fn any_stopped(&self) -> bool {
        self.edges().any(|e| e.stopped)
    }

    /// The same world with its stopped seams turned into walls: what embodied things (bodies,
    /// their vehicles, ships) may move through while edge_stop holds.
    pub fn embodied(&self) -> Glued {
        let open: Vec<Edge> = self.edges().filter(|e| !e.stopped).copied().collect();
        Glued::new(self.d, &open).expect("a subset of a consistent gluing is consistent")
    }

    /// Brings any cell into the domain through the edges (clamped past a wall).
    pub fn fold_cell(&self, x: i64, z: i64) -> Folded {
        let d = self.d;
        let (w, h) = (d.width(), d.depth());
        let (mut x, mut z) = (x, z);
        let mut g = Iso::IDENTITY;
        let mut clamped = false;
        for _ in 0..MAX_FOLDS {
            let side = if x < d.x0 {
                Side::West
            } else if x >= d.x1 {
                Side::East
            } else if z < d.z0 {
                Side::North
            } else if z >= d.z1 {
                Side::South
            } else {
                return Folded {
                    x,
                    z,
                    iso: g,
                    clamped,
                };
            };
            let along_x = matches!(side, Side::West | Side::East);
            if along_x && self.slides_x {
                let n = (x - d.x0).div_euclid(w);
                x -= n * w;
                g = Iso::translation(-n * w, 0).compose(g);
                continue;
            }
            if !along_x && self.slides_z {
                let n = (z - d.z0).div_euclid(h);
                z -= n * h;
                g = Iso::translation(0, -n * h).compose(g);
                continue;
            }
            match &self.edges[side as usize] {
                Some(e) => {
                    (x, z) = e.iso.cell(x, z);
                    g = e.iso.compose(g);
                }
                None => {
                    clamped = true;
                    if along_x {
                        x = x.clamp(d.x0, d.x1 - 1);
                    } else {
                        z = z.clamp(d.z0, d.z1 - 1);
                    }
                }
            }
        }
        Folded {
            x: x.clamp(d.x0, d.x1 - 1),
            z: z.clamp(d.z0, d.z1 - 1),
            iso: g,
            clamped: true,
        }
    }

    /// The shortest displacement from canonical cell `a` to canonical cell `b`, blocks.
    pub fn displacement(&self, a: (i64, i64), b: (i64, i64)) -> (i64, i64) {
        let (d, _) = self.nearest(a, b);
        d
    }

    /// The shortest displacement from canonical `a` to canonical `b` and the motion of the tile
    /// it reaches `b`'s image in (canonical → that tile).
    pub(crate) fn nearest(&self, a: (i64, i64), b: (i64, i64)) -> ((i64, i64), Iso) {
        let (ax, az) = (2 * a.0 + 1, 2 * a.1 + 1);
        let (bx, bz) = (2 * b.0 + 1, 2 * b.1 + 1);
        let mut best: Option<(i64, i64, i64, Iso)> = None;
        for t in &self.near {
            if let Some((d2, _, _, _)) = best
                && t.gap2 > d2
            {
                break;
            }
            let (ix, iz) = t.h.half(bx, bz);
            let (dx, dz) = (ix - ax, iz - az);
            let d2 = dx * dx + dz * dz;
            if best.is_none_or(|(b2, _, _, _)| d2 < b2) {
                best = Some((d2, dx, dz, t.h));
            }
        }
        let (_, dx, dz, h) = best.expect("the identity tile is always near");
        ((dx / 2, dz / 2), h)
    }

    /// The pieces of the cover's straight walk from doubled point `a2` to `b2` inside each tile:
    /// `(f0, f1, cover → canonical)` with f the fraction of the walk, in order.
    pub(crate) fn pieces_half(&self, a2: (f64, f64), b2: (f64, f64)) -> Vec<(f64, f64, Iso)> {
        let d = self.d;
        let (w2, h2) = (2.0 * d.width() as f64, 2.0 * d.depth() as f64);
        let (x0, z0) = (2.0 * d.x0 as f64, 2.0 * d.z0 as f64);
        let mut fs = vec![0.0f64, 1.0];
        let mut cuts = |a: f64, b: f64, o: f64, s: f64| {
            if a == b {
                return;
            }
            let (ia, ib) = (((a - o) / s).floor(), ((b - o) / s).floor());
            let (lo, hi) = (ia.min(ib) as i64, ia.max(ib) as i64);
            for i in (lo + 1)..=hi {
                let line = o + i as f64 * s;
                let f = (line - a) / (b - a);
                if f > 0.0 && f < 1.0 {
                    fs.push(f);
                }
            }
        };
        cuts(a2.0, b2.0, x0, w2);
        cuts(a2.1, b2.1, z0, h2);
        fs.sort_by(f64::total_cmp);
        fs.dedup();
        let mut out: Vec<(f64, f64, Iso)> = Vec::with_capacity(fs.len());
        for p in fs.windows(2) {
            let (f0, f1) = (p[0], p[1]);
            let m = 0.5 * (f0 + f1);
            let (mx, mz) = (a2.0 + (b2.0 - a2.0) * m, a2.1 + (b2.1 - a2.1) * m);
            let cell = ((mx / 2.0).floor() as i64, (mz / 2.0).floor() as i64);
            let g = self.fold_cell(cell.0, cell.1).iso;
            match out.last_mut() {
                Some(last) if last.2 == g => last.1 = f1,
                _ => out.push((f0, f1, g)),
            }
        }
        out
    }

    /// The canonical rectangles a cover rectangle covers, one per tile it overlaps (parts past a
    /// wall dropped). A rectangle holding a whole tile covers the whole domain.
    pub fn fold_rect(&self, r: Rect) -> Vec<Rect> {
        if r.is_empty() {
            return Vec::new();
        }
        let d = self.d;
        let (w, h) = (d.width(), d.depth());
        let (i0, i1) = ((r.x0 - d.x0).div_euclid(w), (r.x1 - 1 - d.x0).div_euclid(w));
        let (j0, j1) = ((r.z0 - d.z0).div_euclid(h), (r.z1 - 1 - d.z0).div_euclid(h));
        // A rectangle holding the domain, or spanning more tiles than a query near the world
        // ever does, covers everything.
        if r.intersect(&d) == d || (i1 - i0 + 1) * (j1 - j0 + 1) > 64 {
            return vec![d];
        }
        let mut out = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                let tile = Rect::new(
                    d.x0 + i * w,
                    d.z0 + j * h,
                    d.x0 + (i + 1) * w,
                    d.z0 + (j + 1) * h,
                );
                let sub = r.intersect(&tile);
                if sub.is_empty() {
                    continue;
                }
                let f = self.fold_cell(sub.x0, sub.z0);
                if f.clamped {
                    continue;
                }
                if sub == tile {
                    return vec![d];
                }
                out.push(f.iso.rect(sub));
            }
        }
        out
    }
}
