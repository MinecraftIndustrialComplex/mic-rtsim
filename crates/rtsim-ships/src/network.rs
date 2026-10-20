// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ship network: docks, lanes between them, and the closed loops ships run.
//!
//! **Lanes.** Docks of one medium in one frame are Delaunay-triangulated; each triangle edge is a
//! candidate lane. A sky lane is the straight segment; a sea lane is a water path over the mask
//! ([`crate::water`]), kept when it is not too long or too roundabout. Each dock keeps at most
//! [`crate::ShipTunables::max_lanes_per_dock`] lanes, shortest first, so the services through it
//! fit its platforms.
//!
//! **Loops.** The idea is Veloren's (`world/src/civ/airship_travel.rs`, research/veloren-rtsim-
//! 2026-10-19.md §4; Veloren is GPL-3.0, and no code is copied, only the method): make the lane
//! graph Eulerian, walk an Euler circuit, and split it into closed loops of about equal length.
//! Here odd docks are paired greedily by shortest path and the lanes between them doubled; the
//! circuit (Hierholzer) is cut into simple cycles wherever it returns to a dock it is still in,
//! and cycles sharing a dock are merged while both stay under the target length. Every lane is
//! in some loop, so every lane has ships; a doubled lane is in two traversals.
//!
//! **Patrols** walk the lanes within a few hops of their base and back (a depth-first tree walk,
//! so each patrolled lane is sailed both ways once a circuit).

use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use rtsim_travel::Medium;

use crate::geom::{Polyline, delaunay_edges, dist};
use crate::tunables::ShipTunables;
use crate::water::{self, WaterMask};

/// A dock: where a port's ships berth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dock {
    /// The world node (market) it serves.
    pub node: usize,
    pub medium: Medium,
    pub frame: u16,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub berths: u32,
}

impl Dock {
    pub fn xz(&self) -> (i32, i32) {
        (self.x, self.z)
    }
}

/// A lane between two docks, `a` to `b` along `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lane {
    pub a: u32,
    pub b: u32,
    pub medium: Medium,
    pub frame: u16,
    pub path: Polyline,
    /// The world edge whose flows it carries: direction 0 is `a` to `b`.
    pub edge: u32,
}

/// One traversal of a loop: lane `lane`, from `stops[j]` to the next stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hop {
    pub lane: u32,
    /// From the lane's `a` to its `b`.
    pub forward: bool,
}

/// A closed walk over lanes: hop `j` sails from `stops[j]` to `stops[(j + 1) % n]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loop {
    pub stops: Vec<u32>,
    pub hops: Vec<Hop>,
}

impl Loop {
    pub fn length(&self, lanes: &[Lane]) -> u64 {
        self.hops
            .iter()
            .map(|h| lanes[h.lane as usize].path.length())
            .sum()
    }

    /// The same loop sailed the other way round.
    pub fn reversed(&self) -> Loop {
        let n = self.stops.len();
        let mut stops = Vec::with_capacity(n);
        let mut hops = Vec::with_capacity(n);
        // Reversed: stops s0, s_{n-1}, ..., s1; hop k goes s_{n-k} -> s_{n-k-1}.
        stops.push(self.stops[0]);
        for k in 1..n {
            stops.push(self.stops[n - k]);
        }
        for k in 0..n {
            let h = self.hops[(n - 1 - k) % n];
            hops.push(Hop {
                lane: h.lane,
                forward: !h.forward,
            });
        }
        Loop { stops, hops }
    }
}

/// A candidate lane: docks and path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub a: u32,
    pub b: u32,
    pub path: Polyline,
}

/// Why a candidate was dropped (for the CLI and tests).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkLints {
    pub too_long: u32,
    pub no_water_path: u32,
    pub too_roundabout: u32,
    pub over_degree: u32,
    pub docks_not_on_water: Vec<u32>,
}

/// The candidate lanes among `group` (docks of one medium and frame), after the length, water and
/// degree rules.
pub fn candidate_lanes(
    docks: &[Dock],
    group: &[u32],
    tun: &ShipTunables,
    mask: &dyn WaterMask,
    lints: &mut NetworkLints,
) -> Vec<Candidate> {
    let pts: Vec<(i64, i64)> = group
        .iter()
        .map(|&d| {
            (
                i64::from(docks[d as usize].x),
                i64::from(docks[d as usize].z),
            )
        })
        .collect();
    let mt = |d: u32| tun.medium(docks[d as usize].medium);
    let mut out = Vec::new();
    for (i, j) in delaunay_edges(&pts) {
        let (a, b) = (group[i], group[j]);
        let (da, db) = (&docks[a as usize], &docks[b as usize]);
        let m = mt(a);
        let straight = dist(da.xz(), db.xz());
        let path = match da.medium {
            Medium::Sea => {
                let ca = water::cell_of(mask, da.x, da.z);
                let cb = water::cell_of(mask, db.x, db.z);
                match water::water_path(mask, da.frame, ca, cb, tun.max_search_cells) {
                    None => {
                        lints.no_water_path += 1;
                        continue;
                    }
                    Some(cells) => {
                        let mut pts = vec![da.xz()];
                        for c in cells.iter().skip(1).take(cells.len().saturating_sub(2)) {
                            pts.push(water::centre(mask, *c));
                        }
                        pts.push(db.xz());
                        let p = Polyline::new(pts);
                        if p.length() * 1000 > straight.max(1) * m.max_stretch_milli {
                            lints.too_roundabout += 1;
                            continue;
                        }
                        p
                    }
                }
            }
            _ => Polyline::new(vec![da.xz(), db.xz()]),
        };
        if path.length() > m.max_lane_blocks {
            lints.too_long += 1;
            continue;
        }
        out.push(Candidate { a, b, path });
    }
    // Shortest first, each dock to its cap.
    out.sort_by_key(|c| (c.path.length(), c.a, c.b));
    let mut deg: BTreeMap<u32, usize> = BTreeMap::new();
    let mut kept = Vec::new();
    for c in out {
        let (x, y) = (
            deg.get(&c.a).copied().unwrap_or(0),
            deg.get(&c.b).copied().unwrap_or(0),
        );
        if x >= tun.max_lanes_per_dock || y >= tun.max_lanes_per_dock {
            lints.over_degree += 1;
            continue;
        }
        *deg.entry(c.a).or_default() += 1;
        *deg.entry(c.b).or_default() += 1;
        kept.push(c);
    }
    kept.sort_by_key(|c| (c.a, c.b));
    kept
}

/// Shortest paths by lane length from `src` over `adj` (dock -> [(other, lane, length)]).
fn dijkstra(
    adj: &BTreeMap<u32, Vec<(u32, u32, u64)>>,
    src: u32,
) -> BTreeMap<u32, (u64, Option<(u32, u32)>)> {
    let mut best: BTreeMap<u32, (u64, Option<(u32, u32)>)> = BTreeMap::new();
    let mut heap = BinaryHeap::new();
    best.insert(src, (0, None));
    heap.push(std::cmp::Reverse((0u64, src)));
    while let Some(std::cmp::Reverse((d, u))) = heap.pop() {
        if best.get(&u).is_some_and(|b| d > b.0) {
            continue;
        }
        for &(v, lane, len) in adj.get(&u).into_iter().flatten() {
            let nd = d + len;
            if best.get(&v).is_none_or(|b| nd < b.0) {
                best.insert(v, (nd, Some((u, lane))));
                heap.push(std::cmp::Reverse((nd, v)));
            }
        }
    }
    best
}

/// Closed loops covering every lane in `lanes` (indices into the full lane table), as described
/// in the module docs. Deterministic in the lanes' order.
pub fn loops(all: &[Lane], lanes: &[u32], target_blocks: u64) -> Vec<Loop> {
    let mut adj: BTreeMap<u32, Vec<(u32, u32, u64)>> = BTreeMap::new();
    for &l in lanes {
        let ln = &all[l as usize];
        let len = ln.path.length().max(1);
        adj.entry(ln.a).or_default().push((ln.b, l, len));
        adj.entry(ln.b).or_default().push((ln.a, l, len));
    }
    // The multigraph: every lane once, plus doubled lanes between paired odd docks.
    let mut multi: Vec<u32> = lanes.to_vec();
    let mut odd: BTreeSet<u32> = adj
        .iter()
        .filter(|(_, v)| v.len() % 2 == 1)
        .map(|(k, _)| *k)
        .collect();
    while let Some(&u) = odd.iter().next() {
        odd.remove(&u);
        let sp = dijkstra(&adj, u);
        let Some((&v, _)) = odd
            .iter()
            .filter_map(|v| sp.get(v).map(|b| (v, b.0)))
            .min_by_key(|&(v, d)| (d, *v))
        else {
            // No other odd dock reachable: impossible in a graph (odd counts are even per
            // component), kept as a guard.
            continue;
        };
        odd.remove(&v);
        let mut cur = v;
        while let Some((_, Some((prev, lane)))) = sp.get(&cur).copied() {
            multi.push(lane);
            cur = prev;
        }
    }
    // Hierholzer over the multigraph, one circuit per component.
    let mut inc: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (k, &l) in multi.iter().enumerate() {
        let ln = &all[l as usize];
        inc.entry(ln.a).or_default().push(k);
        inc.entry(ln.b).or_default().push(k);
    }
    let mut used = vec![false; multi.len()];
    let mut ptr: BTreeMap<u32, usize> = BTreeMap::new();
    let other = |k: usize, v: u32| {
        let ln = &all[multi[k] as usize];
        if ln.a == v { ln.b } else { ln.a }
    };
    let mut cycles: Vec<(Vec<u32>, Vec<Hop>)> = Vec::new();
    let starts: Vec<u32> = inc.keys().copied().collect();
    for s in starts {
        if inc[&s].iter().all(|&k| used[k]) {
            continue;
        }
        // Iterative Hierholzer: (vertex, edge taken into it).
        let mut stack: Vec<(u32, Option<usize>)> = vec![(s, None)];
        let mut circuit: Vec<(u32, Option<usize>)> = Vec::new();
        while let Some(&(v, _)) = stack.last() {
            let list = &inc[&v];
            let p = ptr.entry(v).or_insert(0);
            while *p < list.len() && used[list[*p]] {
                *p += 1;
            }
            if *p < list.len() {
                let k = list[*p];
                used[k] = true;
                stack.push((other(k, v), Some(k)));
            } else {
                circuit.push(stack.pop().expect("non-empty"));
            }
        }
        circuit.reverse();
        // Split into simple cycles where the walk comes back to a dock still on the stack.
        let mut sv: Vec<u32> = vec![circuit[0].0];
        let mut se: Vec<Hop> = Vec::new();
        let mut pos: BTreeMap<u32, usize> = BTreeMap::new();
        pos.insert(circuit[0].0, 0);
        for &(v, e) in &circuit[1..] {
            let k = e.expect("every step but the first has an edge");
            let from = *sv.last().expect("non-empty");
            let ln = &all[multi[k] as usize];
            let hop = Hop {
                lane: multi[k],
                forward: ln.a == from && ln.b == v,
            };
            se.push(hop);
            if let Some(&p) = pos.get(&v) {
                let stops: Vec<u32> = sv[p..].to_vec();
                let hops: Vec<Hop> = se[p..].to_vec();
                for x in &sv[p + 1..] {
                    pos.remove(x);
                }
                sv.truncate(p + 1);
                se.truncate(p);
                cycles.push((stops, hops));
            } else {
                pos.insert(v, sv.len());
                sv.push(v);
            }
        }
    }
    let mut ls: Vec<Loop> = cycles
        .into_iter()
        .map(|(stops, hops)| Loop { stops, hops })
        .collect();
    merge_loops(&mut ls, all, target_blocks);
    ls
}

/// Merges loops that share a dock while the merged loop stays under `target`: the shortest
/// first, with its shortest partner.
fn merge_loops(ls: &mut Vec<Loop>, all: &[Lane], target: u64) {
    loop {
        let mut order: Vec<usize> = (0..ls.len()).collect();
        order.sort_by_key(|&i| (ls[i].length(all), i));
        let mut merged = false;
        'outer: for &i in &order {
            let li = ls[i].length(all);
            let mut best: Option<(u64, usize, u32)> = None;
            for (j, lj) in ls.iter().enumerate() {
                if j == i {
                    continue;
                }
                let total = li + lj.length(all);
                if total > target {
                    continue;
                }
                let shared = ls[i].stops.iter().find(|s| lj.stops.contains(s)).copied();
                if let Some(x) = shared
                    && best.is_none_or(|b| (total, j) < (b.0, b.1))
                {
                    best = Some((total, j, x));
                }
            }
            if let Some((_, j, x)) = best {
                let a = rotate_to(&ls[i], x);
                let b = rotate_to(&ls[j], x);
                let mut stops = b.stops.clone();
                stops.extend(a.stops.iter().copied());
                let mut hops = b.hops.clone();
                hops.extend(a.hops.iter().copied());
                let (hi, lo) = (i.max(j), i.min(j));
                ls.remove(hi);
                ls.remove(lo);
                ls.insert(lo, Loop { stops, hops });
                merged = true;
                break 'outer;
            }
        }
        if !merged {
            return;
        }
    }
}

/// The loop started at stop `x` (one of its stops).
fn rotate_to(l: &Loop, x: u32) -> Loop {
    let k = l.stops.iter().position(|&s| s == x).unwrap_or(0);
    let mut stops = l.stops.clone();
    let mut hops = l.hops.clone();
    stops.rotate_left(k);
    hops.rotate_left(k);
    Loop { stops, hops }
}

/// A patrol's walk from dock `base`: every lane within `hops` lane hops, out and back, depth
/// first in lane order. Empty when the base has no lanes.
pub fn patrol_walk(all: &[Lane], lanes: &[u32], base: u32, hops: u32) -> Loop {
    let mut adj: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for &l in lanes {
        let ln = &all[l as usize];
        adj.entry(ln.a).or_default().push((ln.b, l));
        adj.entry(ln.b).or_default().push((ln.a, l));
    }
    let mut stops = Vec::new();
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    seen.insert(base);
    #[allow(clippy::too_many_arguments)]
    fn walk(
        all: &[Lane],
        adj: &BTreeMap<u32, Vec<(u32, u32)>>,
        v: u32,
        depth: u32,
        max: u32,
        seen: &mut BTreeSet<u32>,
        stops: &mut Vec<u32>,
        out: &mut Vec<Hop>,
    ) {
        if depth >= max {
            return;
        }
        for &(w, l) in adj.get(&v).into_iter().flatten() {
            if !seen.insert(w) {
                continue;
            }
            let ln = &all[l as usize];
            stops.push(v);
            out.push(Hop {
                lane: l,
                forward: ln.a == v,
            });
            walk(all, adj, w, depth + 1, max, seen, stops, out);
            stops.push(w);
            out.push(Hop {
                lane: l,
                forward: ln.a == w,
            });
        }
    }
    walk(all, &adj, base, 0, hops, &mut seen, &mut stops, &mut out);
    Loop { stops, hops: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(a: u32, b: u32, pa: (i32, i32), pb: (i32, i32)) -> Lane {
        Lane {
            a,
            b,
            medium: Medium::Sky,
            frame: 0,
            path: Polyline::new(vec![pa, pb]),
            edge: 0,
        }
    }

    fn check_closed(l: &Loop, all: &[Lane]) {
        let n = l.stops.len();
        assert_eq!(l.hops.len(), n);
        for j in 0..n {
            let h = l.hops[j];
            let ln = &all[h.lane as usize];
            let (from, to) = if h.forward {
                (ln.a, ln.b)
            } else {
                (ln.b, ln.a)
            };
            assert_eq!(from, l.stops[j], "hop {j} of {l:?}");
            assert_eq!(to, l.stops[(j + 1) % n], "hop {j} of {l:?}");
        }
    }

    #[test]
    fn loops_cover_every_lane_and_close() {
        // A triangle with a tail: dock 3 hangs off dock 0 (odd degrees at 0 and 3).
        let p = [(0, 0), (1000, 0), (500, 800), (-900, 0)];
        let all = vec![
            lane(0, 1, p[0], p[1]),
            lane(1, 2, p[1], p[2]),
            lane(0, 2, p[0], p[2]),
            lane(0, 3, p[0], p[3]),
        ];
        let ids: Vec<u32> = (0..all.len() as u32).collect();
        for target in [1, 100_000] {
            let ls = loops(&all, &ids, target);
            let mut covered = BTreeSet::new();
            for l in &ls {
                check_closed(l, &all);
                check_closed(&l.reversed(), &all);
                covered.extend(l.hops.iter().map(|h| h.lane));
            }
            assert_eq!(covered.len(), 4, "every lane sailed: {ls:?}");
            let total: usize = ls.iter().map(|l| l.hops.len()).sum();
            // The tail is doubled to make the graph Eulerian.
            assert_eq!(total, 5);
            if target == 1 {
                assert_eq!(ls.len(), 2, "a triangle and a shuttle: {ls:?}");
            } else {
                assert_eq!(ls.len(), 1, "merged under a long target: {ls:?}");
            }
        }
    }

    #[test]
    fn patrols_walk_out_and_back() {
        let p = [(0, 0), (1000, 0), (2000, 0), (3000, 0)];
        let all = vec![
            lane(0, 1, p[0], p[1]),
            lane(1, 2, p[1], p[2]),
            lane(2, 3, p[2], p[3]),
        ];
        let ids: Vec<u32> = vec![0, 1, 2];
        let w = patrol_walk(&all, &ids, 0, 2);
        check_closed(&w, &all);
        assert_eq!(w.stops, vec![0, 1, 2, 1]);
        assert!(patrol_walk(&all, &ids, 9, 2).hops.is_empty());
    }
}
