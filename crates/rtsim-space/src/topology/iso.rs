// SPDX-License-Identifier: AGPL-3.0-or-later
//! Grid motions and block rectangles: the integer algebra the topology is made of.
//!
//! An [`Iso`] is the wrap engine's `CellIsometry` (Deep Time, `docs/design/wrap-engine.md` §9.2):
//! k clockwise quarter-turns seen from above (one takes north, −z, to east, +x), then a
//! translation. On **points** (block corners, entity positions) k = 1 is (x, z) → (−z, x); on
//! **cells** (the block at integer (x, z) spans [x, x+1) × [z, z+1)) the turns carry the −1
//! offsets that make them the turns of the cells' centres:
//!
//! ```text
//!   k = 0: (x, z) → ( x,      z)          k = 2: (x, z) → (−x − 1, −z − 1)
//!   k = 1: (x, z) → (−z − 1,  x)          k = 3: (x, z) → ( z,     −x − 1)
//! ```
//!
//! then + (tx, tz). Deep Time's layout block writes every edge's motion beyond → inside in these
//! terms (`edges[].iso`, FORMAT.md "The world's layout"). Everything here is exact integer
//! arithmetic; the **doubled** coordinates ([`Iso::half`]) put cell centres on integers (cell x
//! has its centre at 2x + 1), so cells and points share one formula there.

/// A rigid motion of the block grid: `k` clockwise quarter-turns, then `(tx, tz)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Iso {
    /// 0..=3, clockwise seen from above.
    pub k: u8,
    pub tx: i64,
    pub tz: i64,
}

/// The turn alone, on a vector (points about the origin).
#[inline]
pub fn rotate(k: u8, x: i64, z: i64) -> (i64, i64) {
    match k & 3 {
        1 => (-z, x),
        2 => (-x, -z),
        3 => (z, -x),
        _ => (x, z),
    }
}

/// The turn alone, on a float vector.
#[inline]
pub fn rotate_f(k: u8, x: f64, z: f64) -> (f64, f64) {
    match k & 3 {
        1 => (-z, x),
        2 => (-x, -z),
        3 => (z, -x),
        _ => (x, z),
    }
}

impl Iso {
    pub const IDENTITY: Iso = Iso { k: 0, tx: 0, tz: 0 };

    /// `k` is taken modulo 4.
    pub fn new(k: i64, tx: i64, tz: i64) -> Iso {
        Iso {
            k: k.rem_euclid(4) as u8,
            tx,
            tz,
        }
    }

    pub fn translation(tx: i64, tz: i64) -> Iso {
        Iso { k: 0, tx, tz }
    }

    /// The half-turn about the block corner (a, b): (x, z) → (2a − x, 2b − z) on points.
    pub fn half_turn_about(a: i64, b: i64) -> Iso {
        Iso {
            k: 2,
            tx: 2 * a,
            tz: 2 * b,
        }
    }

    pub fn is_identity(self) -> bool {
        self == Iso::IDENTITY
    }

    /// A point (block corner) moved.
    #[inline]
    pub fn point(self, x: i64, z: i64) -> (i64, i64) {
        let (rx, rz) = rotate(self.k, x, z);
        (rx + self.tx, rz + self.tz)
    }

    /// A float point (an entity position) moved.
    #[inline]
    pub fn point_f(self, x: f64, z: f64) -> (f64, f64) {
        let (rx, rz) = rotate_f(self.k, x, z);
        (rx + self.tx as f64, rz + self.tz as f64)
    }

    /// A block cell moved (the wrap engine's `applyX`/`applyZ`).
    #[inline]
    pub fn cell(self, x: i64, z: i64) -> (i64, i64) {
        let (rx, rz) = match self.k & 3 {
            1 => (-z - 1, x),
            2 => (-x - 1, -z - 1),
            3 => (z, -x - 1),
            _ => (x, z),
        };
        (rx + self.tx, rz + self.tz)
    }

    /// A point in doubled coordinates (half-blocks: a cell's centre is (2x + 1, 2z + 1)) moved.
    #[inline]
    pub fn half(self, x2: i64, z2: i64) -> (i64, i64) {
        let (rx, rz) = rotate(self.k, x2, z2);
        (rx + 2 * self.tx, rz + 2 * self.tz)
    }

    /// A vector (a heading, a velocity, a displacement) turned; no translation.
    #[inline]
    pub fn vector(self, dx: i64, dz: i64) -> (i64, i64) {
        rotate(self.k, dx, dz)
    }

    #[inline]
    pub fn vector_f(self, dx: f64, dz: f64) -> (f64, f64) {
        rotate_f(self.k, dx, dz)
    }

    /// `self ∘ other`: first `other`, then `self`.
    pub fn compose(self, other: Iso) -> Iso {
        // self(other(p)) = R1 (R2 p + t2) + t1 = (R1 R2) p + (R1 t2 + t1).
        let (x, z) = rotate(self.k, other.tx, other.tz);
        Iso {
            k: (self.k + other.k) & 3,
            tx: x + self.tx,
            tz: z + self.tz,
        }
    }

    pub fn inverse(self) -> Iso {
        // p = R⁻¹ (q − t), so the translation is −R⁻¹ t.
        let k = (4 - self.k) & 3;
        let (x, z) = rotate(k, -self.tx, -self.tz);
        Iso { k, tx: x, tz: z }
    }

    /// Yaw change in degrees for something carried by this motion (Minecraft yaw grows clockwise
    /// seen from above).
    pub fn yaw_degrees(self) -> f32 {
        90.0 * f32::from(self.k)
    }

    /// A rectangle of cells moved: still axis-aligned, since every turn is a quarter turn.
    pub fn rect(self, r: Rect) -> Rect {
        let (ax, az) = self.point(r.x0, r.z0);
        let (bx, bz) = self.point(r.x1, r.z1);
        Rect {
            x0: ax.min(bx),
            z0: az.min(bz),
            x1: ax.max(bx),
            z1: az.max(bz),
        }
    }

    /// The fixed point of a turn, in doubled coordinates (a turn has exactly one); `None` for a
    /// translation.
    pub fn fixed_half(self) -> Option<(i64, i64)> {
        // (I − R) c = t, so c = (I − R)⁻¹ t. In doubled coordinates C = 2c.
        let (tx, tz) = (self.tx, self.tz);
        match self.k & 3 {
            // R = −I: c = −c + t, so 2c = t: in doubled units C = t.
            2 => Some((tx, tz)),
            // R (x, z) = (−z, x): c − R c = (cx + cz, cz − cx) = t, so cx = (tx − tz) / 2,
            // cz = (tx + tz) / 2, and C = 2c = (tx − tz, tx + tz).
            1 => Some((tx - tz, tx + tz)),
            // R (x, z) = (z, −x): c − R c = (cx − cz, cz + cx) = t, so C = (tx + tz, tz − tx).
            3 => Some((tx + tz, tz - tx)),
            _ => None,
        }
    }
}

/// A half-open rectangle of block cells, [x0, x1) × [z0, z1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rect {
    pub x0: i64,
    pub z0: i64,
    pub x1: i64,
    pub z1: i64,
}

impl Rect {
    pub const fn new(x0: i64, z0: i64, x1: i64, z1: i64) -> Rect {
        Rect { x0, z0, x1, z1 }
    }

    /// The square of cells within `r` blocks (Chebyshev) of cell (x, z): [x − r, x + r].
    pub fn around(x: i64, z: i64, r: i64) -> Rect {
        let r = r.max(0);
        Rect {
            x0: x - r,
            z0: z - r,
            x1: x + r + 1,
            z1: z + r + 1,
        }
    }

    /// The bounding box of two cells, widened by `r`.
    pub fn spanning(a: (i64, i64), b: (i64, i64), r: i64) -> Rect {
        let r = r.max(0);
        Rect {
            x0: a.0.min(b.0) - r,
            z0: a.1.min(b.1) - r,
            x1: a.0.max(b.0) + r + 1,
            z1: a.1.max(b.1) + r + 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.z1 <= self.z0
    }

    pub fn width(&self) -> i64 {
        self.x1 - self.x0
    }

    pub fn depth(&self) -> i64 {
        self.z1 - self.z0
    }

    pub fn contains(&self, x: i64, z: i64) -> bool {
        x >= self.x0 && x < self.x1 && z >= self.z0 && z < self.z1
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        Rect {
            x0: self.x0.max(o.x0),
            z0: self.z0.max(o.z0),
            x1: self.x1.min(o.x1),
            z1: self.z1.min(o.z1),
        }
    }

    /// The cells of a grid of `cell` blocks that the rectangle touches, inclusive ranges
    /// `(cx0, cz0, cx1, cz1)`. Empty rectangles touch none (`cx1 < cx0`).
    pub fn grid_cells(&self, cell: i64) -> (i64, i64, i64, i64) {
        let c = cell.max(1);
        if self.is_empty() {
            return (0, 0, -1, -1);
        }
        (
            self.x0.div_euclid(c),
            self.z0.div_euclid(c),
            (self.x1 - 1).div_euclid(c),
            (self.z1 - 1).div_euclid(c),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_match_the_wrap_engine() {
        // CellIsometry's table (Deep Time, wrapengine/CellIsometry.java).
        let p = (5, -3);
        assert_eq!(Iso::new(0, 0, 0).cell(p.0, p.1), (5, -3));
        assert_eq!(Iso::new(1, 0, 0).cell(p.0, p.1), (2, 5));
        assert_eq!(Iso::new(2, 0, 0).cell(p.0, p.1), (-6, 2));
        assert_eq!(Iso::new(3, 0, 0).cell(p.0, p.1), (-3, -6));
        // A quarter turn takes north to east.
        assert_eq!(Iso::new(1, 0, 0).vector(0, -1), (1, 0));
        // The half-turn about (0, -3328): the north edge of a 16k Guyou world.
        let n = Iso::new(2, 0, -6656);
        assert_eq!(n, Iso::half_turn_about(0, -3328));
        assert_eq!(n.point(100, -3328 - 7), (-100, -3328 + 7));
        assert_eq!(n.cell(100, -3329), (-101, -3328));
    }

    #[test]
    fn composition_and_inverse() {
        let isos = [
            Iso::new(1, 7, -2),
            Iso::new(2, 0, -6656),
            Iso::new(3, -11, 4),
            Iso::translation(13312, 0),
        ];
        for a in isos {
            assert!(a.compose(a.inverse()).is_identity(), "{a:?}");
            assert!(a.inverse().compose(a).is_identity(), "{a:?}");
            for b in isos {
                let p = (17, -29);
                assert_eq!(a.compose(b).point(p.0, p.1), {
                    let q = b.point(p.0, p.1);
                    a.point(q.0, q.1)
                });
                assert_eq!(a.compose(b).cell(p.0, p.1), {
                    let q = b.cell(p.0, p.1);
                    a.cell(q.0, q.1)
                });
            }
            // Doubled coordinates of a cell centre agree with the cell rule.
            let (x, z) = a.cell(17, -29);
            assert_eq!(a.half(35, -57), (2 * x + 1, 2 * z + 1));
            if let Some(c) = a.fixed_half() {
                assert_eq!(a.half(c.0, c.1), c, "{a:?}");
            }
        }
    }

    #[test]
    fn rects_move_whole() {
        let r = Rect::new(0, 0, 4, 2);
        let t = Iso::new(1, 0, 0).rect(r);
        assert_eq!((t.width(), t.depth()), (2, 4));
        for x in r.x0..r.x1 {
            for z in r.z0..r.z1 {
                let (a, b) = Iso::new(1, 0, 0).cell(x, z);
                assert!(t.contains(a, b));
            }
        }
        assert_eq!(Rect::new(-5, 0, 3, 1).grid_cells(4), (-2, 0, 0, 0));
    }
}
