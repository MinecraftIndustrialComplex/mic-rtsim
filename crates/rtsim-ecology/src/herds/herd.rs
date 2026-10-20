// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herd group row (SCALE.md §2's group level, PLAN.md §4.4): counts per year-class (and so per
//! cohort), body condition per cohort, a route, and the few members a player has seen. Members are
//! latent: an animal nobody has seen costs zero bytes; its identity comes from (herd seed, member
//! index).

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::Handle;
use rtsim_travel::legs::Stamp;
use serde::Serialize;

use super::cohort::{Class, cohort_counts};
use super::species::{HerdSpecies, HerdSpeciesData};

pub type HerdId = Handle<Herd>;

/// The 64-bit agent id the bridge sees for a herd's materialized group: namespace byte 0x48
/// ('H') over the row handle's 48 bits (docs/boundary.md §8), apart from dragons' 0x44.
pub const AGENT_NAMESPACE: u64 = 0x48 << 56;

pub fn agent_of(h: HerdId) -> u64 {
    AGENT_NAMESPACE | h.to_bits()
}

pub fn herd_of(agent: u64) -> Option<HerdId> {
    (agent >> 56 == AGENT_NAMESPACE >> 56).then(|| Handle::from_bits(agent & ((1 << 48) - 1)))
}

/// Where a herd goes this sub-season: a polyline walked at the species' speed, then a stay at its
/// last point until `until`. Position is a closed form of time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    pub points: Vec<(i32, i32)>,
    /// Arrival at each point; `times[0]` is the departure.
    pub times: Vec<Tick>,
    pub until: Tick,
}

impl Route {
    pub fn stay(at: (i32, i32), from: Tick, until: Tick) -> Route {
        Route {
            points: vec![at],
            times: vec![from],
            until,
        }
    }

    pub fn start(&self) -> (i32, i32) {
        self.points.first().copied().unwrap_or((0, 0))
    }

    pub fn end(&self) -> (i32, i32) {
        self.points.last().copied().unwrap_or((0, 0))
    }

    pub fn arrive(&self) -> Tick {
        self.times.last().copied().unwrap_or(Tick::ZERO)
    }

    /// The segment being walked at `t`: `i` such that times[i] <= t < times[i + 1].
    pub fn segment_at(&self, t: Tick) -> Option<usize> {
        if self.points.len() < 2 || t < self.times[0] || t >= self.arrive() {
            return None;
        }
        Some(self.times.partition_point(|&x| x <= t) - 1)
    }

    /// Position at `t`, blocks (clamped to the route's span).
    pub fn pos(&self, t: Tick) -> (i64, i64) {
        if self.points.is_empty() {
            return (0, 0);
        }
        match self.segment_at(t) {
            None if t < self.times[0] => {
                let p = self.points[0];
                (i64::from(p.0), i64::from(p.1))
            }
            None => {
                let p = self.end();
                (i64::from(p.0), i64::from(p.1))
            }
            Some(i) => {
                let (a, b) = (self.points[i], self.points[i + 1]);
                let (t0, t1) = (self.times[i].0, self.times[i + 1].0);
                let f = i128::from(t.0 - t0);
                let span = i128::from((t1 - t0).max(1));
                let lerp = |p: i32, q: i32| {
                    (i128::from(p) + (i128::from(q) - i128::from(p)) * f / span) as i64
                };
                (lerp(a.0, b.0), lerp(a.1, b.1))
            }
        }
    }

    /// Where the herd is heading at `t`: the next waypoint, or the stay point.
    pub fn heading(&self, t: Tick) -> (i32, i32) {
        match self.segment_at(t) {
            Some(i) => self.points[i + 1],
            None => self.end(),
        }
    }

    /// Closest approach of the route over `[t0, t1]` to a point: (distance, when).
    pub fn closest(&self, p: (i64, i64), t0: Tick, t1: Tick) -> (f64, Tick) {
        let d = |q: (i64, i64)| (((q.0 - p.0) as f64).powi(2) + ((q.1 - p.1) as f64).powi(2)).sqrt();
        let mut best = (d(self.pos(t0)), t0);
        let mut consider = |t: Tick| {
            if t >= t0 && t <= t1 {
                let x = d(self.pos(t));
                if x < best.0 {
                    best = (x, t);
                }
            }
        };
        consider(t1);
        for i in 0..self.points.len().saturating_sub(1) {
            let (a, b) = (self.points[i], self.points[i + 1]);
            let (ax, az, bx, bz) = (
                f64::from(a.0),
                f64::from(a.1),
                f64::from(b.0),
                f64::from(b.1),
            );
            let (dx, dz) = (bx - ax, bz - az);
            let l2 = dx * dx + dz * dz;
            let f = if l2 > 0.0 {
                (((p.0 as f64 - ax) * dx + (p.1 as f64 - az) * dz) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (s0, s1) = (self.times[i].0, self.times[i + 1].0);
            consider(Tick(s0 + ((s1 - s0) as f64 * f) as u64));
        }
        best
    }
}

/// A member a player has seen (PLAN.md §4.4): it never dies quietly off-screen while it is here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub last: Tick,
    /// Sheared at this tick (a notable state; it folds back once the wool has regrown).
    pub sheared_at: Option<Tick>,
}

/// A materialized group (C25: one puppet unit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embodied {
    pub epoch: u32,
    /// The player slot it was materialized for.
    pub player: u32,
    pub members: Vec<u64>,
    pub since: Tick,
    /// The last steer target sent.
    pub steer: (i32, i32),
    /// A despawn was sent; waiting for the capture.
    pub leaving: bool,
}

/// Every animal that entered or left a herd, as integers (SCALE.md §9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HerdLedger {
    pub founded: u64,
    pub born: u64,
    /// Natural deaths committed so far (hazard and old age); the rest since each class's anchor
    /// is pending and counted on read.
    pub died: u64,
    pub starved: u64,
    pub killed: u64,
    pub eaten: u64,
    /// Taken by a player (lead, taming, pickup): left the sim as ordinary entities (A10).
    pub taken: u64,
    /// Killed by something other than a player (a mob, a fall, another mod).
    pub lost: u64,
}

impl HerdLedger {
    /// Founded + born, less everything committed out.
    pub fn expected_at_anchors(&self) -> i128 {
        i128::from(self.founded) + i128::from(self.born)
            - i128::from(self.died)
            - i128::from(self.starved)
            - i128::from(self.killed)
            - i128::from(self.eaten)
            - i128::from(self.taken)
            - i128::from(self.lost)
    }

    pub fn add(&mut self, o: &HerdLedger) {
        self.founded += o.founded;
        self.born += o.born;
        self.died += o.died;
        self.starved += o.starved;
        self.killed += o.killed;
        self.eaten += o.eaten;
        self.taken += o.taken;
        self.lost += o.lost;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Herd {
    pub name: String,
    pub species: HerdSpecies,
    pub seed: u64,
    pub frame: u16,
    /// Year-classes in order of birth (and so of member index).
    pub classes: Vec<Class>,
    /// The next member index a birth takes (monotonic, PLAN.md §4.4).
    pub next_index: u64,
    /// Body condition per cohort, thousandths of fully fed.
    pub condition: [u16; 3],
    /// What the last sub-season's grazing met of its demand, thousandths.
    pub satisfaction: u16,
    pub route: Route,
    /// The footprint radius the route was planned with: the region index widens the route by it.
    pub reach: u32,
    /// Bumped at every replan: events carry it, and a stale one does nothing.
    pub plan: u32,
    /// The year of the last births.
    pub bred_year: i64,
    /// The tick of the last decision (grazing is charged from it).
    pub decided: Tick,
    /// When the next decision is booked (`Tick(u64::MAX)`: none, the herd is extinct). A
    /// decision event at any other time is stale.
    pub next_decide: Tick,
    pub seen: BTreeMap<u64, Seen>,
    /// Members a player has seen that died or left the sim: never sampled again.
    pub dead: BTreeSet<u64>,
    pub embodied: Option<Embodied>,
    pub epoch: u32,
    /// Predators that ate from it recently (a dragon's id bits) and when they last did: they
    /// follow it farther than their usual range.
    pub followers: Vec<(u64, Tick)>,
    pub ledger: HerdLedger,
    /// The legs booked for players who could see it, per player slot (A15, lazily).
    pub clips: BTreeMap<u32, Stamp>,
}

/// The regions (frame, region x, region z) a route covers, widened by `radius`.
pub fn route_regions(route: &Route, frame: u16, radius: u32) -> Vec<(u16, i64, i64)> {
    use rtsim_travel::legs::REGION_BLOCKS;
    let radius = i64::from(radius);
    let pts = &route.points;
    let segs: Vec<((i32, i32), (i32, i32))> = match pts.len() {
        0 => Vec::new(),
        1 => vec![(pts[0], pts[0])],
        _ => pts.windows(2).map(|w| (w[0], w[1])).collect(),
    };
    let mut out = BTreeSet::new();
    for (a, b) in segs {
        let (x0, x1) = (
            i64::from(a.0.min(b.0)) - radius,
            i64::from(a.0.max(b.0)) + radius,
        );
        let (z0, z1) = (
            i64::from(a.1.min(b.1)) - radius,
            i64::from(a.1.max(b.1)) + radius,
        );
        for rx in x0.div_euclid(REGION_BLOCKS)..=x1.div_euclid(REGION_BLOCKS) {
            for rz in z0.div_euclid(REGION_BLOCKS)..=z1.div_euclid(REGION_BLOCKS) {
                out.insert((frame, rx, rz));
            }
        }
    }
    out.into_iter().collect()
}

impl Herd {
    pub fn alive(&self, sp: &HerdSpeciesData, year_days: u32, day: i64) -> u64 {
        self.classes
            .iter()
            .map(|c| u64::from(c.alive_on(sp, year_days, self.seed, day)))
            .sum()
    }

    pub fn cohorts(&self, sp: &HerdSpeciesData, year_days: u32, day: i64) -> [u64; 3] {
        cohort_counts(&self.classes, sp, year_days, self.seed, day)
    }

    pub fn class_of(&self, index: u64) -> Option<usize> {
        let i = self.classes.partition_point(|c| c.end() <= index);
        (i < self.classes.len() && self.classes[i].contains(index)).then_some(i)
    }

    /// Seen members in class `i`'s index range.
    pub fn seen_in(&self, i: usize) -> impl Iterator<Item = (&u64, &Seen)> {
        let c = &self.classes[i];
        self.seen.range(c.first..c.end())
    }

    pub fn embodied_count(&self) -> u64 {
        self.embodied.as_ref().map_or(0, |e| e.members.len() as u64)
    }

    pub fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.classes.capacity() * std::mem::size_of::<Class>()
            + self.route.points.capacity() * 8
            + self.route.times.capacity() * 8
            + self.seen.len() * (8 + std::mem::size_of::<Seen>() + 16)
            + self.dead.len() * 24
            + self
                .embodied
                .as_ref()
                .map_or(0, |e| e.members.capacity() * 8)
            + self.followers.capacity() * 16
            + self.clips.len() * 40
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_positions_are_closed_form() {
        let r = Route {
            points: vec![(0, 0), (100, 0), (100, 100)],
            times: vec![Tick(0), Tick(100), Tick(300)],
            until: Tick(1_000),
        };
        assert_eq!(r.pos(Tick(50)), (50, 0));
        assert_eq!(r.pos(Tick(200)), (100, 50));
        assert_eq!(r.pos(Tick(900)), (100, 100));
        assert_eq!(r.heading(Tick(50)), (100, 0));
        assert_eq!(r.heading(Tick(500)), (100, 100));
        let (d, t) = r.closest((150, 50), Tick(0), Tick(1_000));
        assert!((d - 50.0).abs() < 1.0, "{d}");
        assert_eq!(t, Tick(200));
    }

    #[test]
    fn agent_ids_round_trip() {
        let h: HerdId = Handle::from_bits(0x0000_1234_0000_0007);
        assert_eq!(herd_of(agent_of(h)), Some(h));
        assert_eq!(herd_of(0x44 << 56 | 7), None);
    }
}
