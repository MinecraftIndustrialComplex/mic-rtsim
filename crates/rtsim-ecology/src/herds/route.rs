// SPDX-License-Identifier: AGPL-3.0-or-later
//! Migration: each sub-season a herd picks its next target from a forage flow field within its
//! comfort band (PLAN.md §10.2).
//!
//! **The flow field** is local and built per decision: a shortest-path tree over the forage cells
//! within the herd's reach (Dijkstra on the 8-neighbour grid), with step costs from the ground
//! (the climate's terrain multiplier) and from obstacle reports. Every reached cell is scored:
//! forage there on arrival, against the herd's demand per cell; the temperature this sub-season
//! against the species' comfort band; the cost of getting there; and a penalty where another herd
//! already settled this sub-season. The best cell is the target, and the tree's parent pointers,
//! read back from it, are the route: the field's flow lines.
//!
//! **Cost:** O(cells in reach × log) per decision, each forage read O(sub-seasons a year). Reach
//! is capped (PLACEHOLDER 16 cells), so a decision's cost does not depend on herd size, world size
//! or elapsed time.
//!
//! **Obstacles** (`Obstacles`): when a herd's puppets keep failing to pass a place, the plugin
//! reports it, and entering that cell costs more from then on. A fence across a pass therefore
//! closes every edge into the pass cell it blocks; the herd tries a neighbouring cell next, and
//! once those are reported too it takes the long way round. The extra cost halves every
//! half-life (PLACEHOLDER two sim years), so a fence taken down is forgotten in time.
//!
//! **Seams** (TP4, TP5, TP14): the flow field is a local grid in the chart of the herd's
//! (canonical) position, so it runs straight across a seam: each chart cell reads the ground,
//! forage, obstacles and claims of its canonical cell, and the route is a chart polyline whose
//! folded points are where the herd walks (over a pole a herd heading north comes out heading
//! south). Cells within [`CONE_CLEARANCE`] of a cone point are not entered.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_space::topology::{CONE_CLEARANCE, Topology};

use super::climate::{Climate, Subs};
use super::forage::{CELL_BLOCKS, CellKey, Forage, cell_of, centre_of};
use super::herd::Route;
use super::species::HerdSpeciesData;

/// Extra cost of entering a cell, from obstacle reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    /// Multiplier above 1 at `at`.
    pub extra: f64,
    pub at: Tick,
    pub reports: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Obstacles {
    pub cells: BTreeMap<CellKey, Obstacle>,
}

impl Obstacles {
    /// The extra multiplier of entering `k` at `t`.
    pub fn extra(&self, k: CellKey, t: Tick, half_life: u64) -> f64 {
        self.cells.get(&k).map_or(0.0, |o| {
            let age = t.0.saturating_sub(o.at.0) as f64 / half_life.max(1) as f64;
            o.extra * 0.5f64.powf(age)
        })
    }

    /// A report: entering `k` costs `step` more (decayed to `t` first), up to `max`.
    pub fn report(&mut self, k: CellKey, t: Tick, step: f64, max: f64, half_life: u64) {
        let now = self.extra(k, t, half_life);
        let reports = self.cells.get(&k).map_or(0, |o| o.reports) + 1;
        self.cells.insert(
            k,
            Obstacle {
                extra: (now + step).min(max),
                at: t,
                reports,
            },
        );
    }
}

/// Everything a decision weighs, apart from the herd itself. All PLACEHOLDERS (HerdTunables).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weights {
    /// Share of the sub-season a herd may spend walking.
    pub travel_share: f64,
    /// The flow field looks this many sub-seasons of walking ahead; the herd walks one
    /// sub-season's share toward the target it finds there, and decides again next time.
    pub horizon: f64,
    /// Reach cap, cells.
    pub max_reach_cells: i32,
    /// Score lost per °C outside the comfort band.
    pub comfort_per_degree: f64,
    /// Score lost per 1,000 block-equivalents of route cost.
    pub travel_per_kblock: f64,
    /// Score lost where another herd settled this sub-season (within 2 cells).
    pub claim_penalty: f64,
    /// Score is forage over demand, saturating at this.
    pub food_cap: f64,
    /// Score for the richest forage in reach (1 at the best cell, in proportion below it): herds
    /// follow the green-up even where every cell would feed them.
    pub richness: f64,
    pub obstacle_half_life: u64,
}

/// One decision's inputs.
pub struct Ask<'a> {
    /// The frame's topology: chart cells are read at their canonical cells.
    pub topo: &'a Topology,
    pub climate: &'a dyn Climate,
    pub subs: &'a Subs,
    pub forage: &'a Forage,
    pub obstacles: &'a Obstacles,
    /// Cells other herds settled on this sub-season.
    pub claims: &'a BTreeMap<CellKey, u64>,
    pub me: u64,
    pub sp: &'a HerdSpeciesData,
    pub frame: u16,
    pub from: (i64, i64),
    pub count: u64,
    pub at: Tick,
    /// When the next decision comes (the stay ends).
    pub until: Tick,
    pub w: Weights,
}

/// A scored cell: (cell, route cost, path length, arrival, forage on arrival).
type Scored = ((i32, i32), u64, f64, Tick, f64);

/// The best cell so far: (score, route cost, cell, forage, temperature).
type Pick = (f64, u64, (i32, i32), f64, f32);

/// What a decision chose, for the CLI and tests.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub route: Route,
    pub target: (i32, i32),
    pub score: f64,
    pub forage: f64,
    pub temperature: f32,
    pub cells_reached: u32,
}

fn step_cost(a: f64, b: f64, diagonal: bool) -> f64 {
    let d = if diagonal {
        f64::from(CELL_BLOCKS) * std::f64::consts::SQRT_2
    } else {
        f64::from(CELL_BLOCKS)
    };
    d * (a + b) / 2.0
}

fn passable(t: f32) -> bool {
    t.is_finite() && t > 0.0
}

/// Plans a herd's next sub-season (the flow field and its best target).
pub fn plan(q: &Ask<'_>) -> Choice {
    let start = cell_of(q.from.0, q.from.1);
    let grid = q.topo.grid(i64::from(CELL_BLOCKS));
    let flat = q.topo.is_flat();
    // The canonical cell of a chart cell (the cell itself on the plane).
    let key = |c: (i32, i32)| -> (i32, i32) {
        if flat {
            return c;
        }
        let (x, z) = grid.canonical(i64::from(c.0), i64::from(c.1));
        (x as i32, z as i32)
    };
    let near_cone = |c: (i32, i32)| {
        !flat && {
            let k = key(c);
            let (x, z) = centre_of(k);
            q.topo.near_cone(
                i64::from(x),
                i64::from(z),
                CONE_CLEARANCE + i64::from(CELL_BLOCKS),
            )
        }
    };
    let days = (q.until.0.saturating_sub(q.at.0)) as f64 / TICKS_PER_DAY as f64;
    let walk = q.sp.speed_blocks_per_day * days * q.w.travel_share;
    let horizon = walk * q.w.horizon.max(1.0);
    let reach = ((horizon / f64::from(CELL_BLOCKS)).ceil() as i32).clamp(1, q.w.max_reach_cells);
    // Dijkstra over cells within `reach` (Chebyshev) and `horizon` blocks of walking, costs
    // scaled to integers so the order is exact and ties break by cell. A dense local grid holds
    // the terrain (read once per cell), the obstacle extras, and the tree.
    let side = (2 * reach + 1) as usize;
    let idx = |c: (i32, i32)| -> Option<usize> {
        let (dx, dz) = (c.0 - start.0 + reach, c.1 - start.1 + reach);
        (dx >= 0 && dz >= 0 && (dx as usize) < side && (dz as usize) < side)
            .then(|| dx as usize * side + dz as usize)
    };
    let cell = |i: usize| {
        (
            (i / side) as i32 - reach + start.0,
            (i % side) as i32 - reach + start.1,
        )
    };
    let mut ground = vec![f32::NAN; side * side];
    let mut ground_at = |i: usize| {
        if ground[i].is_nan() {
            let c = cell(i);
            let k = key(c);
            let t = q.climate.terrain(q.frame, k.0, k.1);
            ground[i] = if passable(t) && !near_cone(c) {
                t
            } else {
                -1.0
            };
        }
        ground[i]
    };
    let has_obstacles = !q.obstacles.cells.is_empty();
    let extra_at = |i: usize| {
        if !has_obstacles {
            return 0.0;
        }
        let c = key(cell(i));
        q.obstacles
            .extra((q.frame, c.0, c.1), q.at, q.w.obstacle_half_life)
    };
    const UNSEEN: u64 = u64::MAX;
    let mut cost_of = vec![UNSEEN; side * side];
    let mut len_of = vec![0.0f64; side * side];
    let mut parent = vec![u32::MAX; side * side];
    let mut done = vec![false; side * side];
    let s0 = idx(start).expect("start is in the grid");
    cost_of[s0] = 0;
    parent[s0] = s0 as u32;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u64, start)));
    let mut settled: Vec<((i32, i32), u64, f64)> = Vec::new();
    let straight = f64::from(CELL_BLOCKS);
    let diagonal = straight * std::f64::consts::SQRT_2;
    while let Some(Reverse((cost, c))) = heap.pop() {
        let ci = idx(c).expect("in the grid");
        if done[ci] {
            continue;
        }
        done[ci] = true;
        let len = len_of[ci];
        settled.push((c, cost, len));
        let tc = f64::from(ground_at(ci)).max(1.0);
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let Some(ni) = idx((c.0 + dx, c.1 + dz)) else {
                    continue;
                };
                let tn = ground_at(ni);
                if tn < 0.0 || done[ni] {
                    continue;
                }
                let diag = dx != 0 && dz != 0;
                // No corner cutting past a closed or obstructed cell: a fence or a cliff
                // across one of the two cells a diagonal step brushes blocks it.
                if diag {
                    let side_a = idx((c.0 + dx, c.1)).expect("inside");
                    let side_b = idx((c.0, c.1 + dz)).expect("inside");
                    if ground_at(side_a) < 0.0
                        || ground_at(side_b) < 0.0
                        || extra_at(side_a) >= 1.0
                        || extra_at(side_b) >= 1.0
                    {
                        continue;
                    }
                }
                let step = step_cost(tc, f64::from(tn).max(1.0), diag) * (1.0 + extra_at(ni));
                let nc = cost + (step * 1000.0).round() as u64;
                let nl = len + if diag { diagonal } else { straight };
                if nl > horizon + straight {
                    continue;
                }
                if nc < cost_of[ni] {
                    cost_of[ni] = nc;
                    len_of[ni] = nl;
                    parent[ni] = ci as u32;
                    heap.push(Reverse((nc, (c.0 + dx, c.1 + dz))));
                }
            }
        }
    }
    // Score the reached cells: every cell within this sub-season's walk, and every other cell
    // on a stride of two beyond it (a quarter of the forage reads, the same field).
    let r = q.sp.footprint_radius(q.count);
    let cells = (std::f64::consts::PI * f64::from(r) * f64::from(r)
        / f64::from(CELL_BLOCKS * CELL_BLOCKS))
    .max(1.0);
    let demand_cell = q.sp.intake_per_day * q.count as f64 * days.max(1.0) / cells;
    let (lo, hi) = q.sp.comfort;
    let near = (walk / f64::from(CELL_BLOCKS)).ceil() as i32;
    let scored: Vec<Scored> = settled
        .iter()
        .filter(|&&(c, _, _)| {
            ((c.0 - start.0).abs() <= near && (c.1 - start.1).abs() <= near)
                || (c.0.rem_euclid(2) == 0 && c.1.rem_euclid(2) == 0)
        })
        .map(|&(c, cost, len)| {
            let travel = (len / q.sp.speed_blocks_per_day.max(1.0) * TICKS_PER_DAY as f64) as u64;
            let arrive = Tick(q.at.0 + travel).max(q.at);
            let k = key(c);
            let f = q.forage.at(q.climate, q.subs, (q.frame, k.0, k.1), arrive);
            (c, cost, len, arrive, f)
        })
        .collect();
    let best_f = scored.iter().map(|x| x.4).fold(1e-9, f64::max);
    let mut pick: Option<Pick> = None;
    for &(c, cost, _, arrive, f) in &scored {
        let food = (f / demand_cell.max(1e-9)).min(q.w.food_cap) + q.w.richness * f / best_f;
        let sub = Subs::in_year(q.subs.index(arrive));
        let k = key(c);
        let temp = q.climate.temperature(q.frame, k.0, k.1, sub);
        let outside = f64::from((lo - temp).max(temp - hi).max(0.0));
        let mut claim = 0.0;
        'near: for dx in -2..=2 {
            for dz in -2..=2 {
                let n = key((c.0 + dx, c.1 + dz));
                if let Some(&o) = q.claims.get(&(q.frame, n.0, n.1))
                    && o != q.me
                {
                    claim = q.w.claim_penalty;
                    break 'near;
                }
            }
        }
        let score = food
            - outside * q.w.comfort_per_degree
            - cost as f64 / 1e6 * q.w.travel_per_kblock
            - claim;
        let better = match pick {
            None => true,
            Some((s, k, cc, _, _)) => score > s || (score == s && (cost, c) < (k, cc)),
        };
        if better {
            pick = Some((score, cost, c, f, temp));
        }
    }
    let sub0 = Subs::in_year(q.subs.index(q.at));
    let (score, _, target, forage, temperature) = pick.unwrap_or((0.0, 0, start, 0.0, {
        let k = key(start);
        q.climate.temperature(q.frame, k.0, k.1, sub0)
    }));
    // Read the route back along the parents, then walk only this sub-season's share of it.
    let mut cells_back = vec![target];
    let mut c = target;
    while c != start {
        c = cell(parent[idx(c).expect("in the grid")] as usize);
        cells_back.push(c);
    }
    cells_back.reverse();
    let mut points = vec![(q.from.0 as i32, q.from.1 as i32)];
    for c in cells_back.iter().skip(1) {
        points.push(centre_of(*c));
    }
    let mut walked = 0.0;
    let mut cut = Vec::with_capacity(points.len());
    cut.push(points[0]);
    for w in points.windows(2) {
        let d = (f64::from(w[1].0 - w[0].0).powi(2) + f64::from(w[1].1 - w[0].1).powi(2)).sqrt();
        if walked + d <= walk {
            cut.push(w[1]);
            walked += d;
            continue;
        }
        let f = ((walk - walked) / d.max(1e-9)).clamp(0.0, 1.0);
        cut.push((
            w[0].0 + (f64::from(w[1].0 - w[0].0) * f).round() as i32,
            w[0].1 + (f64::from(w[1].1 - w[0].1) * f).round() as i32,
        ));
        break;
    }
    let points = cut;
    // Drop points on a straight line (a cheaper polyline, the same closed form).
    let mut slim: Vec<(i32, i32)> = Vec::with_capacity(points.len());
    for p in points {
        while slim.len() >= 2 {
            let (a, b) = (slim[slim.len() - 2], slim[slim.len() - 1]);
            let cross = i64::from(b.0 - a.0) * i64::from(p.1 - b.1)
                - i64::from(b.1 - a.1) * i64::from(p.0 - b.0);
            if cross == 0 {
                slim.pop();
            } else {
                break;
            }
        }
        slim.push(p);
    }
    let mut times = Vec::with_capacity(slim.len());
    let mut t = q.at.0;
    times.push(q.at);
    for w in slim.windows(2) {
        let d = (f64::from(w[1].0 - w[0].0).powi(2) + f64::from(w[1].1 - w[0].1).powi(2)).sqrt();
        t += ((d / q.sp.speed_blocks_per_day.max(1.0)) * TICKS_PER_DAY as f64).ceil() as u64;
        times.push(Tick(t.min(q.until.0)));
    }
    Choice {
        route: Route {
            points: slim,
            times,
            until: q.until,
        },
        target: centre_of(target),
        score,
        forage,
        temperature,
        cells_reached: settled.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::Calendar;
    use crate::herds::climate::ForageRates;
    use crate::herds::species::HerdSpecies;

    static FLAT: Topology = Topology::flat();

    /// Warm in the south (+z, 10 °C a cell), a wall at cz = 0 with a gap at cx = 0: the comfort
    /// band holds only the gap and the two rows south of it.
    struct Valley;
    impl Climate for Valley {
        fn forage(&self, _: u16, _: i32, _: i32, _: u32) -> ForageRates {
            ForageRates {
                growth_per_day: 0.1,
                capacity: 3_000.0,
                decay_per_day: 0.0,
            }
        }
        fn temperature(&self, _: u16, _: i32, cz: i32, _: u32) -> f32 {
            10.0 * cz as f32
        }
        fn terrain(&self, _: u16, cx: i32, cz: i32) -> f32 {
            if cz == 0 && cx != 0 {
                f32::INFINITY
            } else {
                1.0
            }
        }
    }

    fn ask<'a>(
        forage: &'a Forage,
        obstacles: &'a Obstacles,
        claims: &'a BTreeMap<CellKey, u64>,
        sp: &'a HerdSpeciesData,
        subs: &'a Subs,
        from: (i64, i64),
    ) -> Ask<'a> {
        Ask {
            topo: &FLAT,
            climate: &Valley,
            subs,
            forage,
            obstacles,
            claims,
            me: 1,
            sp,
            frame: 0,
            from,
            count: 500,
            at: Tick(0),
            until: Tick::from_days(8),
            w: Weights {
                travel_share: 0.6,
                horizon: 2.5,
                max_reach_cells: 40,
                comfort_per_degree: 0.2,
                travel_per_kblock: 0.05,
                claim_penalty: 0.5,
                food_cap: 1.5,
                richness: 1.0,
                obstacle_half_life: Tick::from_days(192).0,
            },
        }
    }

    #[test]
    fn a_cold_herd_walks_south_through_the_gap_until_fenced() {
        let sp = HerdSpeciesData::of(HerdSpecies::Sheep);
        let subs = Subs::of(&Calendar::default());
        let forage = Forage::new();
        let claims = BTreeMap::new();
        let mut obstacles = Obstacles::default();
        let from = (64, -6 * 128 + 64);
        let c = plan(&ask(&forage, &obstacles, &claims, &sp, &subs, from));
        assert!(
            c.target.1 > 0,
            "it crosses to the warm side: {:?}",
            c.target
        );
        assert!(
            c.route
                .points
                .iter()
                .any(|p| cell_of(i64::from(p.0), i64::from(p.1)) == (0, 0)),
            "through the gap: {:?}",
            c.route.points
        );
        // Fence the gap: the herd stays on its side (no other way within reach).
        for _ in 0..20 {
            obstacles.report((0, 0, 0), Tick(0), 50.0, 1_000.0, 1_000_000);
        }
        let d = plan(&ask(&forage, &obstacles, &claims, &sp, &subs, from));
        assert!(d.target.1 < 0, "fenced: {:?}", d.target);
        assert!((d.route.until.0) == Tick::from_days(8).0);
    }
}
