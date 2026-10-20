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
use rtsim_kernel::codec::{Decode, Encode};
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

// ---- dragons -------------------------------------------------------------------------------------

mod dragons {
    use super::*;
    use rtsim_ecology::boundary::Cmd;
    use rtsim_ecology::dragon::{LairKind, agent_of};
    use rtsim_ecology::engine::{BODY_RADIUS, Dragons, NewDragon};
    use rtsim_ecology::infra::NoInfrastructure;
    use rtsim_ecology::species::{Gender, Species};
    use rtsim_ecology::tunables::DragonTunables;
    use rtsim_settle::Pos;

    const PLAYER: (u64, u64) = (5, 6);

    /// A dragon flies between lairs 4,000 blocks apart through the east–west seam (9,312 the
    /// other way round). A player standing just past the seam on its path sees it exactly once,
    /// at the image on the player's side; a player on the long way round sees nothing.
    #[test]
    fn a_dragon_leg_crossing_a_seam_enters_a_players_view_exactly_once() {
        let b = B as i32;
        for (player_first, on_path) in [(true, true), (false, true), (true, false)] {
            let (mut w, _) = rtsim_ecology::synth::world(3);
            let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
            let t = guyou();
            w.set_topologies(t.clone());
            d.set_topologies(t);
            let a = d.add_lair(
                &mut w,
                1,
                "A",
                Species::Fire,
                LairKind::Roost,
                Pos::new(0, b - 2_000, 1_000),
                100,
                Tick(0),
            );
            let l = d.add_lair(
                &mut w,
                2,
                "B",
                Species::Fire,
                LairKind::Roost,
                Pos::new(0, -b + 2_000, 1_000),
                100,
                Tick(0),
            );
            let x = d.add_dragon(
                &mut w,
                NewDragon {
                    name: "x".into(),
                    species: Species::Fire,
                    gender: Gender::Female,
                    variant: 0,
                    age_ticks: 110 * TICKS_PER_DAY,
                    lair: Some(a),
                    fullness: 100,
                    health: None,
                    seed: None,
                    at: None,
                },
                Tick(0),
            );
            let t0 = Tick(1_000);
            d.run_until(&mut w, &mut NoInfrastructure, t0);
            // Ten blocks west of the seam's west side, canonical; or halfway round the long way.
            let spot = if on_path {
                Pos::new(0, -b + 10, 1_020)
            } else {
                Pos::new(0, 0, 1_020)
            };
            if player_first {
                d.player_stays(&mut w, PLAYER, spot, t0, t0 + 100_000);
            }
            d.relocate(&mut w, x, l, t0);
            if !player_first {
                d.player_stays(&mut w, PLAYER, spot, t0, t0 + 100_000);
            }
            // 4,000 blocks at 0.5 a tick: 8,000 ticks.
            d.run_until(&mut w, &mut NoInfrastructure, t0 + 20_000);
            let mats: Vec<_> = d
                .take_commands()
                .into_iter()
                .filter_map(|c| match c {
                    Cmd::Materialize(m) => Some(m),
                    _ => None,
                })
                .collect();
            let want = usize::from(on_path);
            assert_eq!(
                mats.len(),
                want,
                "player first {player_first}, on path {on_path}"
            );
            assert_eq!(
                d.stats.seen + d.stats.seen_stale,
                want as u64,
                "one event booked"
            );
            if let Some(m) = mats.first() {
                assert_eq!(m.agent, agent_of(x));
                // On the player's side of the seam, at the edge of its area: a plain distance in
                // the player's frame.
                let r = ((m.pos.0 - f64::from(spot.x)).powi(2)
                    + (m.pos.2 - f64::from(spot.z)).powi(2))
                .sqrt();
                let reach = f64::from(d.tun.view_radius + BODY_RADIUS);
                assert!(
                    (r - reach).abs() <= 2.0,
                    "materialized {r:.1} blocks away in the player's frame, the edge is {reach}"
                );
            }
        }
    }
}

// ---- determinism and restore ------------------------------------------------------------------------

/// A herd migrating over the pole and a dragon flying through the x seam: the same inputs give
/// the same state hash, and an engine saved halfway and restored (its topology handed over again,
/// as the bridge does at a session start) continues to the same hash and the same bytes.
#[test]
fn determinism_and_restore_give_identical_hashes_on_a_glued_world() {
    let zone = ((cell(2_900), cell(-B / 2)), (cell(3_200), cell(-B / 2) + 2));
    let new = || {
        let mut h = Herds::new(
            4,
            HerdTunables::default(),
            Box::new(Zones(vec![zone])),
            Tick::ZERO,
        );
        h.set_topologies(guyou());
        let id = h.add_herd(
            NewHerd {
                name: "Polar flock".into(),
                species: HerdSpecies::Sheep,
                frame: 0,
                pos: (-3_000, (-B / 2 + 150) as i32),
                count: 300,
                seed: Some(8),
            },
            Tick::ZERO,
        );
        (h, id)
    };
    let end = Tick(40 * TICKS_PER_DAY);
    let half = Tick(20 * TICKS_PER_DAY);
    let (mut a, _) = new();
    let (mut b, _) = new();
    let mut la = LegIndex::new(1_000);
    let mut lb = LegIndex::new(1_000);
    a.run_until(&mut la, end);
    b.run_until(&mut lb, half);
    let bytes = b.save();
    let mut c =
        Herds::load(HerdTunables::default(), Box::new(Zones(vec![zone])), &bytes).expect("loads");
    c.set_topologies(guyou());
    let mut lc = LegIndex::from_bytes(&lb.to_bytes()).expect("legs load");
    lc.set_topologies(guyou());
    assert_eq!(c.state_hash(), b.state_hash(), "restored as saved");
    b.run_until(&mut lb, end);
    c.run_until(&mut lc, end);
    let (mut d, _) = new();
    let mut ld = LegIndex::new(1_000);
    d.run_until(&mut ld, end);
    assert_eq!(
        a.state_hash(),
        d.state_hash(),
        "the same inputs, the same world"
    );
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "running in two steps changes nothing"
    );
    assert_eq!(
        c.state_hash(),
        a.state_hash(),
        "a restored engine continues exactly"
    );
    assert_eq!(c.save(), a.save());
}
