// SPDX-License-Identifier: AGPL-3.0-or-later
//! Player infrastructure as prey (C16 to C18; INDUSTRY.md §13), through the industry engine's
//! threat interface.
//!
//! A dragon raid on a district is a [`Threat`] of class `Flying` from source `Dragon`, with the
//! dragon's passes as its count. The industry engine resolves it: watched districts fight for
//! real (the dragon materializes there instead, and P28's recommendation holds: a watched dragon
//! breaks no blocks, since Ice and Fire griefing stays off); unwatched ones resolve abstractly,
//! never better for the defender than the best comparable real fight, with theft and destroyed
//! blocks capped per attack and applied on load (C18, I22, I23). What the engine books as stolen,
//! this module's ledger splits into eaten, burnt and hoarded.

use std::collections::BTreeMap;

use rtsim_industry::district::Presence;
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{DistrictId, Good, GoodVec, TrainId};
use rtsim_industry::threat::{Threat, ThreatTarget};
use rtsim_kernel::clock::Tick;
use rtsim_settle::Pos;
use rtsim_space::topology::SharedTopologies;

/// A district as a target: where it is and whether a player is near enough to fight for real.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DistrictSite {
    pub id: DistrictId,
    pub pos: Pos,
    pub watched: bool,
    pub loaded: bool,
}

/// What a raid did.
#[derive(Clone, Debug, PartialEq)]
pub struct RaidOutcome {
    /// The attackers' surviving share after the observation floor.
    pub survived: f64,
    /// The best real outcome that floored it, if any.
    pub floor: Option<f64>,
    pub untested: bool,
    pub stolen: GoodVec,
    pub destroyed_blocks: u32,
    pub machine_lost: bool,
    /// Why nothing happened ("watched", "cooldown", "read-only phase").
    pub skipped: Option<&'static str>,
}

/// A booked train leg a dragon's leg can meet: the train, and the window both are under way in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainSighting {
    pub train: TrainId,
    pub from: Tick,
    pub to: Tick,
}

/// What the dragon module needs from the infrastructure side.
pub trait Infrastructure {
    /// Districts whose centre is within `r` blocks of (x, z).
    fn districts_near(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite>;
    /// Delivers a raid and reports what happened, or `None` for an unknown target.
    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome>;
    /// An industry good's item name, for the prey table.
    fn good_name(&self, g: Good) -> Option<String>;
    /// Booked train legs in `regions` that overlap `[from, to]` (INDUSTRY.md §13.3: one lookup
    /// per region, never a scan).
    fn trains_crossed(
        &mut self,
        _frame: u16,
        _regions: &[(i32, i32)],
        _from: Tick,
        _to: Tick,
    ) -> Vec<TrainSighting> {
        Vec::new()
    }
    /// A raid on a train under way (I24): the cargo it lost, if any. Track is never cut.
    fn raid_train(&mut self, _t: &Threat) -> Option<GoodVec> {
        None
    }
}

/// No infrastructure at all (a world with no industry module).
pub struct NoInfrastructure;

impl Infrastructure for NoInfrastructure {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }
    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }
    fn good_name(&self, _: Good) -> Option<String> {
        None
    }
}

/// The industry engine as infrastructure, with a cell index of district centres rebuilt when
/// the district count changes (districts are declared rarely), so a raid looks up cells, not
/// every district.
pub struct IndustryLink<'a> {
    pub engine: &'a mut Engine,
    pub index: &'a mut DistrictIndex,
}

#[derive(Clone, Debug)]
pub struct DistrictIndex {
    cells: BTreeMap<(u16, i64, i64), Vec<(DistrictId, Pos)>>,
    known: usize,
    /// The frames' topologies (TP6): a district's centre and a query's cells are taken through
    /// them, so a district on a seam has one centre and a raid near an edge sees past it.
    topo: SharedTopologies,
}

impl Default for DistrictIndex {
    fn default() -> Self {
        DistrictIndex {
            cells: BTreeMap::new(),
            known: 0,
            topo: rtsim_space::topology::all_flat(),
        }
    }
}

const CELL: i64 = 1_024;

impl DistrictIndex {
    /// Hands the index the frames' topologies; it is rebuilt at the next query.
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) {
            return;
        }
        if *t != *self.topo {
            self.topo = t;
            self.known = usize::MAX;
        }
    }

    fn refresh(&mut self, e: &Engine) {
        let n = e.districts().count();
        if n == self.known {
            return;
        }
        self.cells.clear();
        for d in e.districts() {
            if d.chunks.is_empty() {
                continue;
            }
            let frame = d.dim as u16;
            let t = self.topo.of(frame);
            // The centre in the chart of the district's first chunk, so a district across a
            // seam is not averaged to the far side of the world (TP16).
            let first = d.chunks.iter().next().expect("not empty");
            let origin = (i64::from(first.0) * 16 + 8, i64::from(first.1) * 16 + 8);
            let (sx, sz) = d.chunks.iter().fold((0i64, 0i64), |a, c| {
                let p = (i64::from(c.0) * 16 + 8, i64::from(c.1) * 16 + 8);
                let q = t.toward(origin, p);
                (a.0 + (q.0 - 8) / 16, a.1 + (q.1 - 8) / 16)
            });
            let k = d.chunks.len() as i64;
            let (x, z) = t.canonical(sx * 16 / k + 8, sz * 16 / k + 8);
            self.cells
                .entry((frame, x.div_euclid(CELL), z.div_euclid(CELL)))
                .or_default()
                .push((d.id, Pos::new(frame, x as i32, z as i32)));
        }
        self.known = n;
    }
}

impl Infrastructure for IndustryLink<'_> {
    fn districts_near(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.index.refresh(self.engine);
        let t = self.index.topo.clone();
        let t = t.of(frame);
        let mut cells = Vec::new();
        for part in t.rects_near(x, z, r) {
            let (x0, z0, x1, z1) = part.grid_cells(CELL);
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    cells.push((cx, cz));
                }
            }
        }
        cells.sort_unstable();
        cells.dedup();
        let mut out = Vec::new();
        for (cx, cz) in cells {
            {
                for &(id, pos) in self.index.cells.get(&(frame, cx, cz)).into_iter().flatten() {
                    if t.dist_sq((i64::from(pos.x), i64::from(pos.z)), (x, z)) > (r * r) as u64 {
                        continue;
                    }
                    let Some(d) = self.engine.district(id) else {
                        continue;
                    };
                    out.push(DistrictSite {
                        id,
                        pos,
                        watched: d.watched,
                        loaded: d.presence != Presence::Unloaded,
                    });
                }
            }
        }
        out.sort_by_key(|s| s.id.0);
        out
    }

    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome> {
        let ThreatTarget::District(d) = t.target else {
            self.engine.threat(t);
            return None;
        };
        let before = self.engine.district(d)?.attacks.len();
        self.engine.threat(t);
        let dist = self.engine.district(d)?;
        // The engine keeps a bounded history; the newest entry is this attack's.
        let r = dist.attacks.back()?;
        if dist.attacks.len() == before && r.at != t.at.max(self.engine.now()) {
            return None;
        }
        Some(RaidOutcome {
            survived: r.outcome.map_or(0.0, |o| o.survived),
            floor: r.outcome.and_then(|o| o.floor),
            untested: r.outcome.is_some_and(|o| o.untested),
            stolen: r.stolen.clone(),
            destroyed_blocks: r.destroyed,
            machine_lost: r.machine_lost,
            skipped: r.skipped,
        })
    }

    fn good_name(&self, g: Good) -> Option<String> {
        self.engine.goods().get(&g).map(|i| i.name.clone())
    }

    fn trains_crossed(
        &mut self,
        frame: u16,
        regions: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainSighting> {
        let legs = self.engine.legs();
        let mut out: Vec<TrainSighting> = Vec::new();
        for &r in regions {
            for id in legs.in_region(u32::from(frame), r, from, to) {
                if let Some(l) = legs.get(id)
                    && !out.iter().any(|s| s.train == l.train)
                {
                    out.push(TrainSighting {
                        train: l.train,
                        from: l.depart.max(from),
                        to: l.eta.min(to),
                    });
                }
            }
        }
        out
    }

    fn raid_train(&mut self, t: &Threat) -> Option<GoodVec> {
        // The engine books loot as stolen: what this attack added is its loot.
        let before = self.engine.books.stolen.clone();
        self.engine.threat(t);
        let loot = self.engine.books.stolen.minus(&before);
        (!loot.is_empty()).then_some(loot)
    }
}
