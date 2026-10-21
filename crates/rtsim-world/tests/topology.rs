// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world on a Deep Time world (docs/choices-topology.md TP20, TP21): the synthetic
//! villages moved so their cluster straddles the east–west seam of a 16k Petroff–Guyou world
//! link by the same roads, of the same lengths, as on the plane, and trade the same; roads cross
//! the seam; travellers on them are where the road runs; a player beside the seam sees across it.

use std::sync::Arc;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_space::topology::{Topologies, layout};
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

const B: i64 = layout::GUYOU_16K;
/// Moves the synthetic cluster (x from -1,000 to 1,300) across the seam at x = b.
const SHIFT: i32 = (B - 500) as i32;

fn grow(shift: i32, topo: Option<Arc<Topologies>>) -> World {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut cfg = WorldConfig::new(7);
    cfg.check_every_event = true;
    cfg.genesis = Genesis::Share(500);
    let mut all = synth::nodes();
    for d in &mut all {
        d.pos = Pos::new(d.pos.frame, d.pos.x + shift, d.pos.z);
    }
    // The first village founds the world; the topology arrives before the rest join, as the
    // bridge's first hand-over brings it.
    let mut w = World::build(cfg, goods, book, vec![all[0].clone()]);
    if let Some(t) = topo {
        w.set_topologies(t);
    }
    for (i, d) in all.iter().enumerate().skip(1) {
        w.join(d.clone(), Tick::from_days(i as u64));
    }
    w
}

fn guyou() -> Arc<Topologies> {
    let mut t = Topologies::new();
    t.set(0, layout::guyou(B, true));
    Arc::new(t)
}

/// The roads as (key, key, length), sorted.
fn roads(w: &World) -> Vec<(u64, u64, u64)> {
    let mut v: Vec<_> = w
        .edges
        .iter()
        .map(|e| {
            let (a, b) = (w.nodes[e.a].key, w.nodes[e.b].key);
            (a.min(b), a.max(b), e.length_blocks)
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn a_caravan_road_crosses_the_x_seam_and_the_world_trades_as_on_the_plane() {
    let mut plane = grow(0, None);
    let mut wrapped = grow(SHIFT, Some(guyou()));
    assert_eq!(
        roads(&plane),
        roads(&wrapped),
        "the same roads, the same lengths"
    );
    // Some road joins villages either side of the seam: canonically 12,000 blocks apart in x,
    // a few hundred along the road.
    let crossing = wrapped
        .edges
        .iter()
        .find(|e| {
            let (a, b) = (wrapped.nodes[e.a].pos, wrapped.nodes[e.b].pos);
            (i64::from(a.x) - i64::from(b.x)).abs() > 10_000
        })
        .expect("a road crosses the seam");
    assert!(crossing.length_blocks < 2_000, "{}", crossing.length_blocks);
    let d = layout::guyou(B, true).domain().unwrap();
    for n in &wrapped.nodes {
        assert!(
            d.contains(i64::from(n.pos.x), i64::from(n.pos.z)),
            "canonical {:?}",
            n.pos
        );
    }
    // Translation through a seam changes nothing the economy sees.
    plane.run_until(Tick::from_days(30));
    wrapped.run_until(Tick::from_days(30));
    plane.check_conserved(plane.now()).unwrap();
    wrapped.check_conserved(wrapped.now()).unwrap();
    assert_eq!(plane.stats.market_events, wrapped.stats.market_events);
    assert_eq!(plane.caravans.len(), wrapped.caravans.len());
    // Every traveller on every road is where the road runs: canonical, and its image is within
    // the road's length of both ends.
    let t = layout::guyou(B, true);
    for (k, e) in wrapped.edges.iter().enumerate() {
        for dir in 0..2u8 {
            for tr in wrapped.travellers(k as u32, dir) {
                let p = wrapped.traveller_pos(&tr);
                assert!(d.contains(p.0, p.1), "{p:?}");
                let a = wrapped.nodes[e.a].pos;
                let da = t.dist(p, (i64::from(a.x), i64::from(a.z)));
                assert!(
                    da <= e.length_blocks as f64 + 2.0,
                    "{da} on a road of {}",
                    e.length_blocks
                );
            }
        }
    }
}

#[test]
fn a_player_beside_the_seam_sees_across_it() {
    let mut w = grow(SHIFT, Some(guyou()));
    w.run_until(Tick::from_days(20));
    // Wherever there is traffic on a road that crosses the seam, a player standing on the
    // canonical side of the seam sees what is on the other side.
    let mut seen_across = 0;
    let mut near_seam = Vec::new();
    for (k, e) in w.edges.iter().enumerate() {
        let (a, b) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
        if (i64::from(a.x) - i64::from(b.x)).abs() < 10_000 {
            continue;
        }
        for dir in 0..2u8 {
            for tr in w.travellers(k as u32, dir) {
                near_seam.push(w.traveller_pos(&tr));
            }
        }
    }
    {
        {
            for p in near_seam {
                // The player stands 20 blocks from it, on the other side of the seam.
                let px = if p.0 < 0 { B - 10 } else { -B + 10 };
                let gap = if p.0 < 0 { p.0 + B } else { B - p.0 };
                if gap > 100 {
                    continue;
                }
                let me = Pos::new(0, px as i32, p.1 as i32);
                let seen = w.visible(me, 160);
                assert!(
                    seen.iter().any(|v| v.pos() == p),
                    "a traveller at {p:?} is in view of a player at {me:?}"
                );
                seen_across += 1;
            }
        }
    }
    let _ = TICKS_PER_DAY;
    assert!(
        seen_across > 0,
        "some traffic passed near the seam to look at"
    );
}
