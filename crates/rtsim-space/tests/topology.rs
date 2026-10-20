// SPDX-License-Identifier: AGPL-3.0-or-later
//! The topology layer's invariants (docs/choices-topology.md): the distance is symmetric and a
//! local isometry through every seam, canonical keys are unique, a heading carried through a seam
//! and back is the one it started with, and Deep Time's own fold agrees with ours cell for cell.

use deeptime_format::projection::PetroffGuyou;
use proptest::prelude::*;
use rtsim_space::topology::{self, CONE_CLEARANCE, Iso, Kind, Move, Rect, Side, Topology, layout};

/// The worlds every property runs on: a small Guyou world (seams close together, so random
/// points hit them), the 16k one, an x wrap, a torus, Peirce's square, and the plane.
fn worlds() -> Vec<(&'static str, Topology)> {
    vec![
        ("guyou-512", layout::guyou(512, false)),
        ("guyou-512-stop", layout::guyou(512, true)),
        ("guyou-16k", layout::guyou(layout::GUYOU_16K, false)),
        ("x-wrap", Topology::x_wrap(-640, 1280)),
        ("torus", layout::torus(Rect::new(-384, -256, 384, 256))),
        ("peirce", layout::peirce(512)),
        ("flat", Topology::flat()),
    ]
}

/// A cell in the domain (or near the origin on the plane).
fn in_domain(t: &Topology, u: f64, v: f64) -> (i64, i64) {
    match t.domain() {
        Some(d) if d.width() < 1 << 40 && d.depth() < 1 << 40 => (
            d.x0 + ((d.width() as f64 * u) as i64).min(d.width() - 1),
            d.z0 + ((d.depth() as f64 * v) as i64).min(d.depth() - 1),
        ),
        Some(d) => (
            d.x0 + ((d.width().min(4096) as f64 * u) as i64),
            ((v - 0.5) * 4096.0) as i64,
        ),
        None => (((u - 0.5) * 4096.0) as i64, ((v - 0.5) * 4096.0) as i64),
    }
}

fn euclid_sq(a: (i64, i64), b: (i64, i64)) -> u64 {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    (dx * dx + dz * dz) as u64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn distance_is_symmetric_and_never_longer_than_straight(
        u0 in 0.0f64..1.0, v0 in 0.0f64..1.0, u1 in 0.0f64..1.0, v1 in 0.0f64..1.0,
    ) {
        for (name, t) in worlds() {
            let (a, b) = (in_domain(&t, u0, v0), in_domain(&t, u1, v1));
            let (ab, ba) = (t.dist_sq(a, b), t.dist_sq(b, a));
            prop_assert_eq!(ab, ba, "{} {:?} {:?}", name, a, b);
            prop_assert!(ab <= euclid_sq(a, b), "{}: through a seam is never longer", name);
            // toward() is the image the displacement names.
            let to = t.toward(a, b);
            prop_assert_eq!(euclid_sq(a, to), ab, "{}", name);
            prop_assert_eq!(t.canonical(to.0, to.1), t.canonical(b.0, b.1), "{}", name);
            // The triangle inequality through seams.
            let c = in_domain(&t, v0, u1);
            prop_assert!(t.dist(a, b) <= t.dist(a, c) + t.dist(c, b) + 1e-9, "{}", name);
        }
    }

    #[test]
    fn nearest_images_carry_their_motion(
        u0 in 0.0f64..1.0, v0 in 0.0f64..1.0, u1 in 0.0f64..1.0, v1 in 0.0f64..1.0, dx in -40i64..40, dz in -40i64..40,
    ) {
        for (name, t) in worlds() {
            let (a, b) = (in_domain(&t, u0, v0), in_domain(&t, u1, v1));
            let (img, iso) = t.nearest_image(a, b);
            prop_assert_eq!(img, t.toward(a, b), "{}", name);
            let cb = t.canonical(b.0, b.1);
            prop_assert_eq!(iso.cell(cb.0, cb.1), img, "{}", name);
            // A point near b moves into a's frame by the same motion, to a point near the image.
            let near = (cb.0 + dx, cb.1 + dz);
            if t.fold(near.0, near.1).clamped || t.near_cone(cb.0, cb.1, 130) {
                continue;
            }
            let moved = iso.cell(near.0, near.1);
            prop_assert_eq!(t.canonical(moved.0, moved.1), t.canonical(near.0, near.1), "{}", name);
        }
    }

    #[test]
    fn distance_is_invariant_under_every_edge_motion(
        u0 in 0.0f64..1.0, v0 in 0.0f64..1.0, u1 in 0.0f64..1.0, v1 in 0.0f64..1.0,
    ) {
        for (name, t) in worlds() {
            let Some(g) = t.as_glued() else { continue };
            let (a, b) = (in_domain(&t, u0, v0), in_domain(&t, u1, v1));
            let d = t.dist_sq(a, b);
            for e in g.edges() {
                // The images beyond the edge (the inverse motion takes a cell inside to the tile
                // beyond) are the same places.
                let inv = e.iso.inverse();
                let (ia, ib) = (inv.cell(a.0, a.1), inv.cell(b.0, b.1));
                prop_assert_eq!(t.dist_sq(ia, b), d, "{} {:?}", name, e.side);
                prop_assert_eq!(t.dist_sq(a, ib), d, "{} {:?}", name, e.side);
                prop_assert_eq!(t.dist_sq(ia, ib), d, "{} {:?}", name, e.side);
                prop_assert_eq!(t.canonical(ia.0, ia.1), a, "{} {:?}", name, e.side);
            }
        }
    }

    #[test]
    fn a_short_step_across_any_seam_is_its_own_length(
        u in 0.0f64..1.0, side in 0u8..4, along in 0.0f64..1.0, dx in -60i64..60, dz in -60i64..60,
    ) {
        // The world is locally the plane, through every seam: a step of a few dozen blocks from
        // just inside an edge to just beyond it measures what it measures in the chart.
        for (name, t) in worlds() {
            let Some(d) = t.domain() else { continue };
            if d.width() > 1 << 30 { continue }
            let s = Side::from_u8(side).unwrap();
            let edge_cell = match s {
                Side::West => (d.x0 + 2, d.z0 + ((d.depth() as f64 * along) as i64)),
                Side::East => (d.x1 - 3, d.z0 + ((d.depth() as f64 * along) as i64)),
                Side::North => (d.x0 + ((d.width() as f64 * along) as i64), d.z0 + 2),
                Side::South => (d.x0 + ((d.width() as f64 * along) as i64), d.z1 - 3),
            };
            let a = t.canonical(edge_cell.0, edge_cell.1);
            let near_cone = t.near_cone(a.0, a.1, 130);
            let b = (a.0 + dx, a.1 + dz);
            let walls = t.as_glued().is_none_or(|g| g.edge(s).is_none()) && !matches!(t.kind(), Kind::XWrap);
            if walls || near_cone || t.fold(b.0, b.1).clamped {
                continue;
            }
            let _ = u;
            prop_assert_eq!(t.dist_sq(a, b), euclid_sq(a, b), "{} {:?} {:?} -> {:?}", name, s, a, b);
        }
    }

    #[test]
    fn canonical_keys_are_unique(
        x in -3000i64..3000, z in -3000i64..3000, k in 0i64..4, size in prop::sample::select(vec![1i64, 16, 64, 128, 512]),
    ) {
        for (name, t) in worlds() {
            let c = t.canonical(x, z);
            if let Some(d) = t.domain() {
                prop_assert!(d.contains(c.0, c.1), "{}: {:?} folds to {:?} outside {:?}", name, (x, z), c, d);
            }
            prop_assert_eq!(t.canonical(c.0, c.1), c, "{}: folding is idempotent", name);
            // Every chart image of a place has the place's key, and only places at distance 0
            // share a key.
            if let Some(g) = t.as_glued() {
                let e = g.edges().nth(k as usize % g.edges().count().max(1)).map(|e| e.iso.inverse()).unwrap_or(Iso::IDENTITY);
                let img = e.cell(c.0, c.1);
                prop_assert_eq!(t.key(img.0, img.1, size), t.key(c.0, c.1, size), "{}", name);
                prop_assert_eq!(t.dist_sq(img, c), 0, "{}", name);
            }
            let other = t.canonical(x + 1, z);
            prop_assert_eq!(other == c, t.dist_sq((x + 1, z), (x, z)) == 0, "{}", name);
        }
    }

    #[test]
    fn a_heading_through_a_seam_and_back_is_the_identity(
        u in 0.0f64..1.0, v in 0.0f64..1.0, dx in -300i64..300, dz in -300i64..300,
    ) {
        for (name, t) in worlds() {
            let a = in_domain(&t, u, v);
            let ((c, h), _) = (t.transport(a.0 + dx, a.1 + dz, dx, dz), ());
            if t.fold(a.0 + dx, a.1 + dz).clamped {
                continue;
            }
            // Walk back along the turned heading from where we came out.
            let back = (c.0 - h.0, c.1 - h.1);
            let (home, h2) = t.transport(back.0, back.1, -h.0, -h.1);
            prop_assert_eq!(home, a, "{} out at {:?} heading {:?}", name, c, h);
            prop_assert_eq!(h2, (-dx, -dz), "{}", name);
            // A full turn of headings is preserved in length.
            prop_assert_eq!(h.0 * h.0 + h.1 * h.1, dx * dx + dz * dz, "{}", name);
        }
    }

    #[test]
    fn deep_time_folds_cell_for_cell(x in -40_000i64..40_000, z in -20_000i64..20_000) {
        // Our generic fold of the glued group against Deep Time's own `fold_cell`.
        let b = layout::GUYOU_16K;
        let pg = PetroffGuyou::new(b as u32, 31.25);
        let t = layout::guyou(b, false);
        let (dx, dz, n) = pg.fold_cell(x, z);
        let f = t.fold(x, z);
        prop_assert_eq!((f.x, f.z), (dx, dz));
        // An odd number of half-turns reverses a heading.
        prop_assert_eq!(f.iso.k, if n % 2 == 1 { 2 } else { 0 });
    }

    #[test]
    fn rectangles_fold_onto_exactly_their_cells(
        u in 0.0f64..1.0, v in 0.0f64..1.0, w in 1i64..200, h in 1i64..200,
    ) {
        for (name, t) in worlds() {
            let a = in_domain(&t, u, v);
            let r = Rect::new(a.0 - w / 2, a.1 - h / 2, a.0 - w / 2 + w, a.1 - h / 2 + h);
            let parts = t.fold_rect(r);
            let area: i64 = parts.iter().map(|p| p.width() * p.depth()).sum();
            let clamped = (r.x0..r.x1).any(|x| (r.z0..r.z1).any(|z| t.fold(x, z).clamped));
            if !clamped {
                prop_assert_eq!(area, w * h, "{}: {:?} -> {:?}", name, r, parts);
            }
            for x in (r.x0..r.x1).step_by(7) {
                for z in (r.z0..r.z1).step_by(5) {
                    let f = t.fold(x, z);
                    if f.clamped { continue }
                    prop_assert!(parts.iter().any(|p| p.contains(f.x, f.z)), "{}: {:?} not in {:?}", name, (f.x, f.z), parts);
                }
            }
        }
    }

    #[test]
    fn legs_are_continuous_through_seams(
        u0 in 0.0f64..1.0, v0 in 0.0f64..1.0, u1 in 0.0f64..1.0, v1 in 0.0f64..1.0,
    ) {
        for (name, t) in worlds() {
            let (a, b) = (in_domain(&t, u0, v0), in_domain(&t, u1, v1));
            let end = t.toward(a, b);
            let steps = 64i64;
            let mut last = a;
            for s in 1..=steps {
                let p = (a.0 + (end.0 - a.0) * s / steps, a.1 + (end.1 - a.1) * s / steps);
                let c = t.canonical(p.0, p.1);
                let step = ((end.0 - a.0).abs().max((end.1 - a.1).abs()) / steps) + 2;
                prop_assert!(t.dist_sq(last, c) <= (2 * step * step) as u64, "{}: jump {:?} -> {:?}", name, last, c);
                last = c;
            }
            prop_assert_eq!(last, t.canonical(b.0, b.1), "{}", name);
            // The pieces tile the walk and fold its points.
            let pieces = t.pieces(a, end);
            prop_assert!((pieces[0].f0 - 0.0).abs() < 1e-12 && (pieces.last().unwrap().f1 - 1.0).abs() < 1e-12);
            for p in &pieces {
                let f = 0.5 * (p.f0 + p.f1);
                let q = (a.0 as f64 + 0.5 + (end.0 - a.0) as f64 * f, a.1 as f64 + 0.5 + (end.1 - a.1) as f64 * f);
                // The centre of the cell the point is in (a point on a cell boundary would be
                // ambiguous), moved by the piece's motion, is the cell's canonical cell.
                let cell = (q.0.floor() as i64, q.1.floor() as i64);
                let (cx, cz) = p.iso.point_f(cell.0 as f64 + 0.5, cell.1 as f64 + 0.5);
                let want = t.canonical(cell.0, cell.1);
                prop_assert!((cx.floor() as i64, cz.floor() as i64) == want, "{}: piece {:?}", name, p);
            }
        }
    }
}

#[test]
fn deep_time_layout_block_is_the_synthetic_world() {
    // Linked, not mirrored: the block Deep Time writes, parsed by its own type.
    let spec = layout::guyou_spec(layout::GUYOU_16K as u32);
    let t = layout::from_projection(&spec, false).expect("the Guyou block builds");
    assert_eq!(t, layout::guyou(layout::GUYOU_16K, true));
    let installed = layout::from_projection(&spec, true).expect("builds");
    assert_eq!(installed, layout::guyou(layout::GUYOU_16K, false));
    // And through JSON, as a manifest carries it.
    let json = serde_json::to_string(&spec).unwrap();
    assert_eq!(layout::from_projection_json(&json, false).unwrap(), t);
    // FORMAT.md's cone points: the long edges' midpoints and the corners.
    let mut cones = t.cones();
    cones.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        cones,
        vec![
            (-6656.0, -3328.0),
            (-6656.0, 3328.0),
            (0.0, -3328.0),
            (0.0, 3328.0),
            (6656.0, -3328.0),
            (6656.0, 3328.0)
        ]
    );
    assert!(t.any_stopped());
    assert_eq!(t.embodied().kind(), Kind::XWrap);
    // A Lambert world's block is an x wrap with its poles as the border.
    let lambert = deeptime_format::projection::lambert_spec(
        &deeptime_format::projection::Projection::new(16_384),
    );
    let l = layout::from_projection(&lambert, false).expect("Lambert builds");
    assert_eq!(l.kind(), Kind::XWrap);
}

#[test]
fn over_the_pole_heading_north_comes_out_heading_south() {
    let b = layout::GUYOU_16K;
    let t = layout::guyou(b, false);
    let h = b / 2;
    // Walking north at x = -3328 (the western pole's meridian... the pole itself is on the edge),
    // ten blocks past the north edge.
    let from = (-1000, -h + 5);
    let ((c, heading), _) = (t.transport(from.0, from.1 - 15, 0, -1), ());
    assert_eq!(c, (999, -h + 9));
    assert_eq!(heading, (0, 1), "north becomes south");
    // The distance across is the short one.
    assert_eq!(t.dist_sq(from, c), 15 * 15);
}

#[test]
fn edge_stop_marks_the_crossing_and_walls_the_embodied_world() {
    let b = 512;
    let t = layout::guyou(b, true);
    let h = b / 2;
    let a = (100, -h + 10);
    let beyond = (100, -h - 30);
    let f = t
        .stopped_crossing(a, beyond)
        .expect("crosses the stopped north seam");
    assert!((f - 10.0 / 40.0).abs() < 0.03, "{f}");
    // The east-west seam is not stopped.
    assert!(t.stopped_crossing((b - 5, 0), (b + 20, 0)).is_none());
    // Embodied, the north edge is a wall: the short way to a point just across it is not
    // through it.
    let across = t.canonical(beyond.0, beyond.1);
    assert!(t.dist(a, across) < 50.0);
    assert!(t.embodied().dist(a, across) > 100.0);
    assert!(
        layout::guyou(b, false)
            .stopped_crossing(a, beyond)
            .is_none()
    );
}

#[test]
fn cone_points_are_cleared() {
    let t = layout::guyou(layout::GUYOU_16K, false);
    let h = layout::GUYOU_16K / 2;
    let near = (3, -h + 2);
    assert!(t.near_cone(near.0, near.1, CONE_CLEARANCE));
    let moved = t.clear_of_cones(near.0, near.1, CONE_CLEARANCE);
    assert!(!t.near_cone(moved.0, moved.1, CONE_CLEARANCE), "{moved:?}");
    assert_eq!(t.clear_of_cones(moved.0, moved.1, CONE_CLEARANCE), moved);
    // The same cell always moves the same way.
    assert_eq!(t.clear_of_cones(near.0, near.1, CONE_CLEARANCE), moved);
    let far = (1000, 0);
    assert_eq!(t.clear_of_cones(far.0, far.1, CONE_CLEARANCE), far);
}

#[test]
fn grids_fold_whole_cells_when_aligned() {
    let t = layout::guyou(layout::GUYOU_16K, false);
    assert!(t.grid(64).aligned());
    assert!(t.grid(128).aligned());
    assert!(!t.grid(512).aligned(), "b/2 = 3,328 is not a whole region");
    let g = t.grid(128);
    // A cell north of the north edge is its half-turn image, and a heading turns half a turn.
    let (cx, cz, k) = g.fold(10, -27);
    assert_eq!((cx, cz, k), (-11, -26, 2));
    // Every block of an aligned cell folds into the one canonical cell.
    for (bx, bz) in [(10 * 128, -27 * 128), (10 * 128 + 127, -27 * 128 + 127)] {
        assert_eq!(g.of_block(bx, bz), (cx, cz));
    }
    // North of the cell just inside the north edge is, across the seam, the cell mirrored
    // through the pivot, entered heading south.
    assert_eq!(g.neighbour(-11, -26, 0, -1), (10, -26, 2));
    assert_eq!(g.toward((-11, -26), (10, -26)), (-11, -27));
}

#[test]
fn moves_meet_through_a_seam_once() {
    let t = Topology::x_wrap(-640, 1280);
    // A walker going east over the seam, and one standing just past it.
    let a = Move::of_cells(0.0, 100.0, (600, 0), (700, 0));
    let stay = Move::of_cells(0.0, 100.0, (-575, 0), (-575, 0));
    // -575 is chart 705: the walker comes within 10 blocks at chart 695, at t = 95.
    let at = t
        .first_within(&a, &stay, 10.0)
        .expect("they meet across the seam");
    assert!((at - 95.0).abs() < 0.5, "{at}");
    let (d, when) = t.closest_to(&a, (-574.5, 0.5));
    assert!(
        (d - 5.0).abs() < 1e-6 && (when - 100.0).abs() < 1e-6,
        "{d} at {when}"
    );
    // On the plane they never meet.
    assert!(Topology::flat().first_within(&a, &stay, 10.0).is_none());
}

#[test]
fn shared_topologies_default_to_flat() {
    let mut all = topology::Topologies::new();
    assert!(all.of(7).is_flat());
    all.set(0, layout::guyou(512, true));
    assert!(!all.all_flat());
    all.set(0, Topology::flat());
    assert!(all.all_flat());
    assert!(topology::all_flat().of(0).is_flat());
}
