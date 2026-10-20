// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flights as scheduled legs (A15, PLAN.md §4.3; veloren research §4).
//!
//! A dragon is never ticked. A flight is a straight leg from where it is to where it goes at its
//! species' speed, so its position is a formula; it is booked into the **one shared leg index**
//! (`rtsim_travel::LegIndex`, the one caravans and players' interest areas use), and the cost of a
//! flight is the event at its end. While unloaded it flies at a **forced cruise altitude**, as
//! Veloren's unloaded airships do (terrain plus cruise height there; a fixed Y here, since the
//! engine knows no terrain), so it can never materialize underground: the plugin raises it above
//! the surface when it spawns it.
//!
//! **Owners.** Leg owners are opaque to the index. The top bit marks a player's interest area (the
//! convention the phase 1 world's interest areas use), the next one a dragon; caravans' owners are
//! 48-bit row handles, below both.

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_space::topology::{CONE_CLEARANCE, Move, Topology};
use rtsim_travel::legs::{Owner, Path};

use crate::dragon::DragonId;

pub const PLAYER_OWNER: u64 = 1 << 63;
pub const DRAGON_OWNER: u64 = 1 << 62;

pub fn dragon_owner(d: DragonId) -> Owner {
    Owner(DRAGON_OWNER | d.to_bits())
}

pub fn player_owner(p: ActorId) -> Owner {
    Owner(PLAYER_OWNER | p.to_bits())
}

pub fn owner_dragon(o: Owner) -> Option<DragonId> {
    (o.0 & (PLAYER_OWNER | DRAGON_OWNER) == DRAGON_OWNER)
        .then(|| DragonId::from_bits(o.0 & ((1 << 48) - 1)))
}

pub fn owner_player(o: Owner) -> Option<ActorId> {
    (o.0 & PLAYER_OWNER != 0).then(|| ActorId::from_bits(o.0 & ((1 << 48) - 1)))
}

/// The shortest distance through every seam (TP2); on the plane, exactly the old straight line.
pub fn dist(t: &Topology, a: (i64, i64), b: (i64, i64)) -> f64 {
    if t.is_flat() {
        let (dx, dz) = ((a.0 - b.0) as f64, (a.1 - b.1) as f64);
        return (dx * dx + dz * dz).sqrt();
    }
    t.dist(a, b)
}

/// Ticks to fly `blocks` at `speed` blocks per tick, at least 1.
pub fn flight_ticks(blocks: f64, speed: f64) -> u64 {
    ((blocks / speed.max(1e-6)).ceil() as u64).max(1)
}

/// A straight leg from `from` at `t0` to `to`, at `speed`, booked with `radius`: the short way,
/// so its end is the image of `to` nearest `from` (a chart point, past an edge when the flight
/// crosses a seam; TP7). Both ends are kept clear of cone points (TP13).
pub fn leg(t: &Topology, from: Pos, to: Pos, t0: Tick, speed: f64, radius: u32) -> Path {
    let a = (i64::from(from.x), i64::from(from.z));
    let b = (i64::from(to.x), i64::from(to.z));
    let b = if t.is_flat() {
        b
    } else {
        let c = t.clear_of_cones(b.0, b.1, CONE_CLEARANCE);
        t.toward(a, c)
    };
    let d = dist(t, a, b);
    Path {
        frame: from.frame,
        x0: from.x,
        z0: from.z,
        x1: b.0 as i32,
        z1: b.1 as i32,
        t0,
        t1: t0 + flight_ticks(d, speed),
        radius,
    }
}

/// A stay at one place over `[t0, t1]` (a hunt's dwell, a player's interest area).
pub fn stay(at: Pos, t0: Tick, t1: Tick, radius: u32) -> Path {
    Path {
        frame: at.frame,
        x0: at.x,
        z0: at.z,
        x1: at.x,
        z1: at.z,
        t0,
        t1: t1.max(t0),
        radius,
    }
}

/// The point of road `a`→`b` (canonical ends, walked the short way) closest to `p`, as
/// thousandths of the way from `a`, clamped to `[lo, hi]` thousandths so a hunt is over the road
/// and not in a village. Through seams: the road and `p` may lie on either side of one.
pub fn closest_on(
    t: &Topology,
    a: (i64, i64),
    b: (i64, i64),
    p: (i64, i64),
    lo: u32,
    hi: u32,
) -> u32 {
    if t.is_flat() {
        let (dx, dz) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
        let len2 = dx * dx + dz * dz;
        if len2 == 0.0 {
            return lo;
        }
        let f = (((p.0 - a.0) as f64 * dx + (p.1 - a.1) as f64 * dz) / len2).clamp(0.0, 1.0);
        return ((f * 1000.0).round() as u32).clamp(lo, hi);
    }
    let b = t.toward(a, b);
    if b == a {
        return lo;
    }
    let m = Move::of_cells(0.0, 1000.0, a, b);
    let (_, f) = t.closest_to(&m, (p.0 as f64 + 0.5, p.1 as f64 + 0.5));
    (f.round() as u32).clamp(lo, hi)
}

/// The canonical point `milli` thousandths of the way along road `a`→`b` (the short way).
pub fn lerp(t: &Topology, a: (i64, i64), b: (i64, i64), milli: u32) -> (i64, i64) {
    let f = i64::from(milli);
    if t.is_flat() {
        return (a.0 + (b.0 - a.0) * f / 1000, a.1 + (b.1 - a.1) * f / 1000);
    }
    let b = t.toward(a, b);
    t.canonical(a.0 + (b.0 - a.0) * f / 1000, a.1 + (b.1 - a.1) * f / 1000)
}

/// Where a flight `p0`→`p1` (its leg's start and chart end) crosses road `q0`→`q1` (canonical
/// ends, the short way), as thousandths along each, if they do: through seams on a wrapped world.
pub fn crossing(
    t: &Topology,
    p0: (i64, i64),
    p1: (i64, i64),
    q0: (i64, i64),
    q1: (i64, i64),
) -> Option<(u32, u32)> {
    if t.is_flat() {
        let r = ((p1.0 - p0.0) as f64, (p1.1 - p0.1) as f64);
        let s = ((q1.0 - q0.0) as f64, (q1.1 - q0.1) as f64);
        let den = r.0 * s.1 - r.1 * s.0;
        if den.abs() < 1e-9 {
            return None;
        }
        let qp = ((q0.0 - p0.0) as f64, (q0.1 - p0.1) as f64);
        let a = (qp.0 * s.1 - qp.1 * s.0) / den;
        let u = (qp.0 * r.1 - qp.1 * r.0) / den;
        return ((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&u))
            .then(|| ((a * 1000.0).round() as u32, (u * 1000.0).round() as u32));
    }
    let q1 = t.toward(q0, q1);
    t.crossing(p0, p1, q0, q1)
        .map(|(a, u)| ((a * 1000.0).round() as u32, (u * 1000.0).round() as u32))
}

/// A leg's canonical position at `at` (its chart position folded, TP7).
pub fn at(t: &Topology, p: &Path, at: Tick) -> (i64, i64) {
    let (x, z) = p.at(at);
    t.canonical(x, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::handle::Handle;

    #[test]
    fn owners_do_not_collide() {
        let d: DragonId = Handle::from_parts(9, 2);
        let p: ActorId = Handle::from_parts(9, 2);
        assert_eq!(owner_dragon(dragon_owner(d)), Some(d));
        assert_eq!(owner_player(player_owner(p)), Some(p));
        assert_eq!(owner_dragon(player_owner(p)), None);
        assert_eq!(owner_player(dragon_owner(d)), None);
        // A caravan's owner is a plain row handle.
        assert_eq!(owner_dragon(Owner(d.to_bits())), None);
        assert_eq!(owner_player(Owner(d.to_bits())), None);
    }

    #[test]
    fn geometry() {
        let f = Topology::flat();
        assert_eq!(
            crossing(&f, (0, -10), (0, 10), (-10, 0), (10, 0)),
            Some((500, 500))
        );
        assert_eq!(crossing(&f, (0, 1), (0, 10), (-10, 0), (10, 0)), None);
        assert_eq!(closest_on(&f, (0, 0), (1000, 0), (300, 500), 100, 900), 300);
        assert_eq!(
            closest_on(&f, (0, 0), (1000, 0), (-300, 500), 100, 900),
            100
        );
        let l = leg(
            &f,
            Pos::new(0, 0, 0),
            Pos::new(0, 1000, 0),
            Tick(10),
            0.5,
            128,
        );
        assert_eq!(l.t1, Tick(2010));
    }

    #[test]
    fn flights_take_the_short_way_through_seams() {
        // An x wrap of 4,096: from x = 1,900 to x = -1,900 is 296 blocks east, over the seam.
        let t = Topology::x_wrap(-2048, 4096);
        let l = leg(
            &t,
            Pos::new(0, 1900, 0),
            Pos::new(0, -1900, 0),
            Tick(0),
            1.0,
            64,
        );
        assert_eq!((l.x1, l.z1), (2196, 0));
        assert_eq!(l.t1, Tick(296));
        assert_eq!(at(&t, &l, Tick(200)), (-1996, 0));
        // A road across the seam is crossed by a flight across it, and its point nearest a
        // village past the seam is found there.
        // The road from x = 1,950 to x = -2,000 runs 146 blocks east over the seam.
        assert_eq!(
            crossing(&t, (2000, -50), (2000, 50), (1950, 0), (-2000, 0)),
            Some((500, 342))
        );
        assert_eq!(
            closest_on(&t, (1950, 0), (-2000, 0), (-2048, 30), 0, 1000),
            671
        );
        assert_eq!(lerp(&t, (1950, 0), (-2000, 0), 900), (-2015, 0));
    }
}
