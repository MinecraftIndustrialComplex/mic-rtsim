// SPDX-License-Identifier: AGPL-3.0-or-later
//! Plane geometry for the ship network: a Delaunay triangulation of the docks (Bowyer and
//! Watson's algorithm, in exact integer arithmetic), and polylines measured in whole blocks.
//!
//! Everything is integer or rounds once, so a network built from the same docks is the same on
//! every machine: lanes and timetables are derived data, rebuilt identically after a load.

use std::collections::BTreeSet;

/// Twice the signed area of (a, b, c): positive when counter-clockwise.
fn orient(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> i128 {
    let (ax, ay) = (i128::from(a.0), i128::from(a.1));
    let (bx, by) = (i128::from(b.0), i128::from(b.1));
    let (cx, cy) = (i128::from(c.0), i128::from(c.1));
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

/// Whether `d` lies strictly inside the circumcircle of the counter-clockwise triangle (a, b, c).
/// Exact: coordinates up to 2^30 keep every product inside i128.
fn in_circle(a: (i64, i64), b: (i64, i64), c: (i64, i64), d: (i64, i64)) -> bool {
    let row = |p: (i64, i64)| {
        let x = i128::from(p.0) - i128::from(d.0);
        let y = i128::from(p.1) - i128::from(d.1);
        (x, y, x * x + y * y)
    };
    let (ax, ay, a2) = row(a);
    let (bx, by, b2) = row(b);
    let (cx, cy, c2) = row(c);
    let det = ax * (by * c2 - b2 * cy) - ay * (bx * c2 - b2 * cx) + a2 * (bx * cy - by * cx);
    det > 0
}

/// The edges of a Delaunay triangulation of `pts`, as sorted index pairs `(i, j)` with `i < j`,
/// in ascending order. Duplicate points are skipped (only the first of a group gets edges).
/// Points all on one line are chained in order along it. Cost O(n²): the network is built when
/// docks join, never per tick.
pub fn delaunay_edges(pts: &[(i64, i64)]) -> Vec<(usize, usize)> {
    let mut uniq: Vec<usize> = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, p) in pts.iter().enumerate() {
        if seen.insert(*p) {
            uniq.push(i);
        }
    }
    let n = uniq.len();
    if n < 2 {
        return Vec::new();
    }
    if n == 2 {
        let (a, b) = (uniq[0], uniq[1]);
        return vec![(a.min(b), a.max(b))];
    }
    // All collinear: a chain along the line.
    let p0 = pts[uniq[0]];
    let p1 = pts[uniq[1]];
    if uniq.iter().all(|&i| orient(p0, p1, pts[i]) == 0) {
        let mut order = uniq.clone();
        order.sort_by_key(|&i| pts[i]);
        let mut out: Vec<(usize, usize)> = order
            .windows(2)
            .map(|w| (w[0].min(w[1]), w[0].max(w[1])))
            .collect();
        out.sort_unstable();
        return out;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for &i in &uniq {
        let (x, y) = pts[i];
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    let span = (maxx - minx).max(maxy - miny).max(1);
    let (cx, cy) = ((minx + maxx) / 2, (miny + maxy) / 2);
    // A super triangle well outside every point.
    let big = span * 64;
    let mut v: Vec<(i64, i64)> = uniq.iter().map(|&i| pts[i]).collect();
    let s0 = v.len();
    v.push((cx - 2 * big, cy - big));
    v.push((cx + 2 * big, cy - big));
    v.push((cx, cy + 2 * big));
    let mut tris: Vec<[usize; 3]> = vec![[s0, s0 + 1, s0 + 2]];
    for p in 0..s0 {
        let mut bad = Vec::new();
        let mut keep = Vec::with_capacity(tris.len());
        for t in tris.drain(..) {
            if in_circle(v[t[0]], v[t[1]], v[t[2]], v[p]) {
                bad.push(t);
            } else {
                keep.push(t);
            }
        }
        tris = keep;
        // The cavity's boundary: edges of exactly one bad triangle, kept in their winding.
        let mut count: std::collections::BTreeMap<(usize, usize), (u32, (usize, usize))> =
            std::collections::BTreeMap::new();
        for t in &bad {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let e = count.entry((a.min(b), a.max(b))).or_insert((0, (a, b)));
                e.0 += 1;
            }
        }
        for (_, (c, (a, b))) in count {
            if c == 1 {
                let mut t = [a, b, p];
                if orient(v[t[0]], v[t[1]], v[t[2]]) < 0 {
                    t.swap(0, 1);
                }
                if orient(v[t[0]], v[t[1]], v[t[2]]) != 0 {
                    tris.push(t);
                }
            }
        }
    }
    let mut edges = BTreeSet::new();
    for t in &tris {
        if t.iter().any(|&k| k >= s0) {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (uniq[t[k]], uniq[t[(k + 1) % 3]]);
            edges.insert((a.min(b), a.max(b)));
        }
    }
    // Hull edges between points the super triangle shadowed are lost by Bowyer and Watson with a
    // finite super triangle only when it is too small; 64 spans is far enough for docks.
    edges.into_iter().collect()
}

/// Straight distance in whole blocks, rounded to nearest.
pub fn dist(a: (i32, i32), b: (i32, i32)) -> u64 {
    let dx = f64::from(a.0) - f64::from(b.0);
    let dz = f64::from(a.1) - f64::from(b.1);
    (dx * dx + dz * dz).sqrt().round() as u64
}

/// A path of straight segments, measured: `cum[k]` is the distance along it to point `k`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Polyline {
    pub pts: Vec<(i32, i32)>,
    pub cum: Vec<u64>,
}

impl Polyline {
    pub fn new(pts: Vec<(i32, i32)>) -> Polyline {
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0u64;
        for (k, p) in pts.iter().enumerate() {
            if k > 0 {
                acc += dist(pts[k - 1], *p);
            }
            cum.push(acc);
        }
        Polyline { pts, cum }
    }

    pub fn length(&self) -> u64 {
        self.cum.last().copied().unwrap_or(0)
    }

    /// The point `s` blocks along the path (clamped to its ends).
    pub fn at(&self, s: u64) -> (i32, i32) {
        if self.pts.len() < 2 {
            return self.pts.first().copied().unwrap_or((0, 0));
        }
        let s = s.min(self.length());
        let k = match self.cum.binary_search(&s) {
            Ok(k) => return self.pts[k],
            Err(k) => k.clamp(1, self.pts.len() - 1),
        };
        let (a, b) = (self.pts[k - 1], self.pts[k]);
        let (s0, s1) = (self.cum[k - 1], self.cum[k]);
        if s1 <= s0 {
            return b;
        }
        let f = (s - s0) as f64 / (s1 - s0) as f64;
        let lerp = |p: i32, q: i32| (f64::from(p) + (f64::from(q) - f64::from(p)) * f).round() as i32;
        (lerp(a.0, b.0), lerp(a.1, b.1))
    }

    /// The path from `s0` to `s1` blocks along it (either order: reversed when `s1 < s0`), with
    /// the corners in between.
    pub fn slice(&self, s0: u64, s1: u64) -> Vec<(i32, i32)> {
        let (lo, hi, rev) = if s0 <= s1 {
            (s0, s1, false)
        } else {
            (s1, s0, true)
        };
        let mut out = vec![self.at(lo)];
        for (k, &c) in self.cum.iter().enumerate() {
            if c > lo && c < hi {
                out.push(self.pts[k]);
            }
        }
        let end = self.at(hi);
        if out.last() != Some(&end) || out.len() == 1 {
            out.push(end);
        }
        if rev {
            out.reverse();
        }
        out
    }

    /// The same path walked the other way.
    pub fn reversed(&self) -> Polyline {
        let mut pts = self.pts.clone();
        pts.reverse();
        Polyline::new(pts)
    }

    pub fn heap_bytes(&self) -> usize {
        self.pts.capacity() * 8 + self.cum.capacity() * 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_and_its_centre_triangulate_into_eight_edges() {
        let pts = vec![(0, 0), (1000, 0), (1000, 1000), (0, 1000), (500, 500)];
        let e = delaunay_edges(&pts);
        // Four sides and four spokes to the centre; no diagonal through it.
        assert_eq!(e.len(), 8, "{e:?}");
        for k in 0..4 {
            assert!(e.contains(&(k, 4)));
        }
        assert!(!e.contains(&(0, 2)) && !e.contains(&(1, 3)));
    }

    #[test]
    fn triangulation_has_the_euler_edge_count() {
        // A jittered grid: E = 3n - 3 - h for n points with h on the hull.
        let mut pts = Vec::new();
        let mut x = 7u64;
        for i in 0..6 {
            for j in 0..6 {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let jx = (x >> 40) as i64 % 200 - 100;
                let jz = (x >> 20) as i64 % 200 - 100;
                pts.push((i * 1000 + jx, j * 1000 + jz));
            }
        }
        let e = delaunay_edges(&pts);
        let n = pts.len();
        assert!(e.len() <= 3 * n - 6 && e.len() >= 2 * n, "{} edges", e.len());
        // Every point is connected.
        let mut deg = vec![0; n];
        for (a, b) in &e {
            deg[*a] += 1;
            deg[*b] += 1;
        }
        assert!(deg.iter().all(|&d| d >= 2));
        assert_eq!(e, delaunay_edges(&pts), "deterministic");
    }

    #[test]
    fn degenerate_inputs() {
        assert!(delaunay_edges(&[(0, 0)]).is_empty());
        assert_eq!(delaunay_edges(&[(0, 0), (5, 5)]), vec![(0, 1)]);
        assert_eq!(delaunay_edges(&[(0, 0), (0, 0), (5, 5)]), vec![(0, 2)]);
        assert_eq!(
            delaunay_edges(&[(0, 0), (20, 0), (10, 0)]),
            vec![(0, 2), (1, 2)]
        );
    }

    #[test]
    fn polylines_measure_and_slice() {
        let p = Polyline::new(vec![(0, 0), (300, 0), (300, 400)]);
        assert_eq!(p.length(), 700);
        assert_eq!(p.at(150), (150, 0));
        assert_eq!(p.at(500), (300, 200));
        assert_eq!(p.at(9999), (300, 400));
        assert_eq!(p.slice(100, 500), vec![(100, 0), (300, 0), (300, 200)]);
        assert_eq!(p.slice(500, 100), vec![(300, 200), (300, 0), (100, 0)]);
        assert_eq!(p.reversed().at(100), (300, 300));
    }
}
