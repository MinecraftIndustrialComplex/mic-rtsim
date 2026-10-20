// SPDX-License-Identifier: AGPL-3.0-or-later
//! Herds on a Deep Time world (docs/choices-topology.md, DECISIONS.md C31): a synthetic 16k
//! Petroff–Guyou world whose only good grazing lies just across a seam. One herd crosses the
//! east–west seam, one goes north over a pole and comes out heading south, and neither walks
//! the long way across the world; the animals are conserved throughout.

use std::sync::Arc;

use rtsim_ecology::herds::climate::{Climate, ForageRates};
use rtsim_ecology::herds::forage::CELL_BLOCKS;
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_space::topology::{Topologies, layout};
use rtsim_travel::legs::LegIndex;

const B: i64 = layout::GUYOU_16K;

/// Rich grazing only in the zones (canonical cells), thin elsewhere; mild everywhere.
struct Zones(Vec<((i32, i32), (i32, i32))>);

impl Climate for Zones {
    fn forage(&self, _: u16, cx: i32, cz: i32, _: u32) -> ForageRates {
        let rich = self
            .0
            .iter()
            .any(|&((x0, z0), (x1, z1))| (x0..=x1).contains(&cx) && (z0..=z1).contains(&cz));
        if rich {
            ForageRates {
                growth_per_day: 0.5,
                capacity: 4_000.0,
                decay_per_day: 0.0,
            }
        } else {
            ForageRates {
                growth_per_day: 0.01,
                capacity: 40.0,
                decay_per_day: 0.0,
            }
        }
    }
    fn temperature(&self, _: u16, _: i32, _: i32, _: u32) -> f32 {
        12.0
    }
    fn terrain(&self, _: u16, _: i32, _: i32) -> f32 {
        1.0
    }
}

fn cell(b: i64) -> i32 {
    b.div_euclid(i64::from(CELL_BLOCKS)) as i32
}

fn guyou() -> Arc<Topologies> {
    let mut t = Topologies::new();
    t.set(0, layout::guyou(B, true));
    Arc::new(t)
}

/// Runs a herd for `days` in steps of `step` ticks, returning its canonical positions (the first
/// at tick 0).
fn walk_by(
    h: &mut Herds,
    legs: &mut LegIndex,
    id: rtsim_ecology::herds::HerdId,
    days: u64,
    step: u64,
) -> Vec<(i64, i64)> {
    let mut out = vec![h.position(id, Tick::ZERO)];
    let mut t = 0;
    while t < days * TICKS_PER_DAY {
        t += step;
        h.run_until(legs, Tick(t));
        h.check(Tick(t))
            .expect("conservation and identity invariants");
        out.push(h.position(id, Tick(t)));
    }
    out
}

fn walk(
    h: &mut Herds,
    legs: &mut LegIndex,
    id: rtsim_ecology::herds::HerdId,
    days: u64,
) -> Vec<(i64, i64)> {
    walk_by(h, legs, id, days, TICKS_PER_DAY)
}

#[test]
fn a_herd_crosses_the_east_west_seam_and_its_animals_are_conserved() {
    // The zone: the first cells east of the western edge, x in [-6656, -5888).
    let zone = ((cell(-B), -3), (cell(-B) + 5, 3));
    let mut h = Herds::new(
        9,
        HerdTunables::default(),
        Box::new(Zones(vec![zone])),
        Tick::ZERO,
    );
    h.set_topologies(guyou());
    let mut legs = LegIndex::new(1_000);
    let id = h.add_herd(
        NewHerd {
            name: "Seam flock".into(),
            species: HerdSpecies::Sheep,
            frame: 0,
            pos: ((B - 200) as i32, 0),
            count: 400,
            seed: Some(3),
        },
        Tick::ZERO,
    );
    let founded = h.herds[id].ledger.founded;
    let path = walk(&mut h, &mut legs, id, 60);
    let d = layout::guyou(B, true).domain().unwrap();
    for p in &path {
        assert!(d.contains(p.0, p.1), "every position is canonical: {p:?}");
        assert!(
            p.0.abs() > B - 1_500,
            "it never walks the long way across the world: {p:?} in {path:?}"
        );
    }
    assert!(
        path.iter().any(|p| p.0 < -B + 900),
        "it crossed the seam to the grazing: {path:?}"
    );
    // `check` held every day: the classes hold exactly what the ledger says entered and left.
    assert!(h.count(id, Tick(60 * TICKS_PER_DAY)) > 0);
    assert_eq!(h.herds[id].ledger.founded, founded);
}

#[test]
fn a_herd_goes_north_over_the_pole_and_comes_out_heading_south() {
    let h2 = B / 2;
    // The grazing just across the north seam from (-3000, -3200): the half-turn about (0, -b/2)
    // puts it at x near +3000, just inside the edge.
    let zone = ((cell(2_900), cell(-h2)), (cell(3_200), cell(-h2) + 2));
    let mut h = Herds::new(
        4,
        HerdTunables::default(),
        Box::new(Zones(vec![zone])),
        Tick::ZERO,
    );
    h.set_topologies(guyou());
    let mut legs = LegIndex::new(1_000);
    let id = h.add_herd(
        NewHerd {
            name: "Polar flock".into(),
            species: HerdSpecies::Sheep,
            frame: 0,
            pos: (-3_000, (-h2 + 150) as i32),
            count: 300,
            seed: Some(8),
        },
        Tick::ZERO,
    );
    let path = walk_by(&mut h, &mut legs, id, 60, 500);
    for p in &path {
        assert!(
            p.0 < -2_000 || p.0 > 2_000,
            "it goes over the pole, not across the world: {p:?} in {path:?}"
        );
        assert!(p.1 >= -h2 && p.1 < h2, "canonical: {p:?}");
    }
    let k = path
        .iter()
        .position(|p| p.0 > 2_000)
        .unwrap_or_else(|| panic!("it reached the grazing across the pole: {path:?}"));
    assert!(k > 0, "it started on the far side");
    // It came out turned: before the seam it walked north (z falling), after it, south (z rising)
    // from the edge, on the far side of the world in x.
    let before = path[..k].iter().map(|p| p.1).collect::<Vec<_>>();
    let after = &path[k..];
    assert!(
        before.windows(2).any(|w| w[1] < w[0]),
        "it walked north first: {before:?}"
    );
    assert!(
        after[0].1 < -h2 + 400,
        "it comes out at the edge: {after:?}"
    );
    assert!(h.count(id, Tick(60 * TICKS_PER_DAY)) > 0);
}
