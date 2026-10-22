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
use rtsim_industry::ids::{DistrictId, Good, GoodClass, GoodVec, TrainId};
use rtsim_industry::obs::DepositPluginId;
use rtsim_industry::threat::{Threat, ThreatTarget};
use rtsim_kernel::clock::Tick;
use rtsim_settle::Pos;
use rtsim_space::topology::SharedTopologies;

use crate::oil::Near;

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

/// What a dragon drinking at an oil field senses of a district near it (OILGATE.md §4.5, OG-8):
/// an oil deposit sampled there (a pumpjack or a CDG oil chunk in its books), Destroy's level
/// pollution sampled there, and the fuel in its stock, mB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DistrictSignals {
    pub oil_sample: bool,
    pub polluted: bool,
    pub fuel_mb: i64,
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
    /// The chunk test (the oil audit's F3, choices-oilgate.md OGC50): districts with any chunk
    /// whose centre is within `r` blocks of (x, z), each at its chunk centre nearest (x, z), in id
    /// order. A district is where its chunks are: a well joined to a far base is still by its
    /// field. By default, the centre test.
    fn districts_reaching(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.districts_near(frame, x, z, r)
    }
    /// The players who built in `district` (placed or broke a block, or configured a machine) at
    /// or after `since`, by UUID, the latest builder last (the oil audit's F7, OGC54). Nobody by
    /// default.
    fn builders(&mut self, _district: DistrictId, _since: Tick) -> Vec<(u64, u64)> {
        Vec::new()
    }
    /// Every district with a chunk `near` answers for (F2, OGC52), in id order: its site at the
    /// first such chunk's centre, and the answer. `near` is asked with the frame and a chunk's
    /// centre, chunk by chunk, until it answers. Nothing by default.
    fn districts_where(
        &mut self,
        _near: &mut dyn FnMut(u16, i64, i64) -> Option<Near>,
    ) -> Vec<(DistrictSite, Near)> {
        Vec::new()
    }
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
    /// A raid that also drinks (OG-5): the survivors take the goods named in `fuels` (item names;
    /// a name also matches its `name/<hash>` variants) first, up to `room` units. Without a fuel
    /// stock to drink from it is a plain raid.
    fn raid_drinking(&mut self, t: &Threat, _fuels: &[String], _room: i64) -> Option<RaidOutcome> {
        self.raid(t)
    }
    /// What a dragon senses of `district` (OG-8). Nothing by default.
    fn signals(&mut self, _district: DistrictId, _fuels: &[String]) -> DistrictSignals {
        DistrictSignals::default()
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

/// The industry engine as infrastructure, with cell indexes of district centres and chunks
/// rebuilt whenever a district comes, goes or changes its chunks (the engine's
/// [`Engine::districts_rev`]), so a raid looks up cells, not every district.
pub struct IndustryLink<'a> {
    pub engine: &'a mut Engine,
    pub index: &'a mut DistrictIndex,
}

#[derive(Clone, Debug)]
pub struct DistrictIndex {
    cells: BTreeMap<(u16, i64, i64), Vec<(DistrictId, Pos)>>,
    /// Every district chunk's canonical centre, by cell (the chunk test, OGC50).
    chunk_cells: BTreeMap<(u16, i64, i64), Vec<(DistrictId, Pos)>>,
    /// The engine's `districts_rev` the cells were built from; 0 is never one.
    known: u64,
    /// The frames' topologies (TP6): a district's centre and a query's cells are taken through
    /// them, so a district on a seam has one centre and a raid near an edge sees past it.
    topo: SharedTopologies,
}

impl Default for DistrictIndex {
    fn default() -> Self {
        DistrictIndex {
            cells: BTreeMap::new(),
            chunk_cells: BTreeMap::new(),
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
            self.known = 0;
        }
    }

    fn refresh(&mut self, e: &Engine) {
        let rev = e.districts_rev();
        if rev == self.known {
            return;
        }
        self.cells.clear();
        self.chunk_cells.clear();
        for d in e.districts() {
            if d.chunks.is_empty() {
                continue;
            }
            let frame = d.dim as u16;
            let t = self.topo.of(frame);
            for c in &d.chunks {
                let (x, z) = t.canonical(i64::from(c.0) * 16 + 8, i64::from(c.1) * 16 + 8);
                self.chunk_cells
                    .entry((frame, x.div_euclid(CELL), z.div_euclid(CELL)))
                    .or_default()
                    .push((d.id, Pos::new(frame, x as i32, z as i32)));
            }
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
        self.known = rev;
    }

    /// The cells a query of `r` blocks round (x, z) covers, through the frame's topology.
    fn cells_near(&self, frame: u16, x: i64, z: i64, r: i64) -> Vec<(i64, i64)> {
        let t = self.topo.of(frame);
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
        cells
    }
}

impl IndustryLink<'_> {
    /// The industry goods that are fuel, in the order of `fuels`, then by id; deposits never are.
    fn fuel_goods(&self, fuels: &[String]) -> Vec<Good> {
        let mut out: Vec<(usize, Good)> = Vec::new();
        for (g, i) in self.engine.goods() {
            if i.class == GoodClass::Deposit {
                continue;
            }
            let rank = fuels.iter().position(|f| {
                i.name == *f
                    || i.name
                        .strip_prefix(f.as_str())
                        .is_some_and(|r| r.starts_with('/'))
            });
            if let Some(r) = rank {
                out.push((r, *g));
            }
        }
        out.sort_unstable();
        out.into_iter().map(|(_, g)| g).collect()
    }

    fn raid_with(&mut self, t: &Threat, fuels: &[Good], room: i64) -> Option<RaidOutcome> {
        let ThreatTarget::District(d) = t.target else {
            self.engine.threat_drinking(t, fuels, room);
            return None;
        };
        let before = self.engine.district(d)?.attacks.len();
        self.engine.threat_drinking(t, fuels, room);
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
}

impl IndustryLink<'_> {
    fn site(&self, id: DistrictId, pos: Pos) -> Option<DistrictSite> {
        let d = self.engine.district(id)?;
        Some(DistrictSite {
            id,
            pos,
            watched: d.watched,
            loaded: d.presence != Presence::Unloaded,
        })
    }
}

impl Infrastructure for IndustryLink<'_> {
    fn districts_reaching(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.index.refresh(self.engine);
        let topo = self.index.topo.clone();
        let t = topo.of(frame);
        // Each district's chunk centre nearest (x, z); a tie goes to the lower chunk.
        let mut best: BTreeMap<DistrictId, (u64, Pos)> = BTreeMap::new();
        for (cx, cz) in self.index.cells_near(frame, x, z, r) {
            for &(id, pos) in self
                .index
                .chunk_cells
                .get(&(frame, cx, cz))
                .into_iter()
                .flatten()
            {
                let d = t.dist_sq((i64::from(pos.x), i64::from(pos.z)), (x, z));
                if d > (r * r) as u64 {
                    continue;
                }
                let e = best.entry(id).or_insert((d, pos));
                if (d, pos.x, pos.z) < (e.0, e.1.x, e.1.z) {
                    *e = (d, pos);
                }
            }
        }
        best.into_iter()
            .filter_map(|(id, (_, pos))| self.site(id, pos))
            .collect()
    }

    fn builders(&mut self, district: DistrictId, since: Tick) -> Vec<(u64, u64)> {
        self.engine.district(district).map_or_else(Vec::new, |d| {
            d.editors
                .iter()
                .filter(|(_, t)| *t >= since)
                .map(|(p, _)| *p)
                .collect()
        })
    }

    fn districts_where(
        &mut self,
        near: &mut dyn FnMut(u16, i64, i64) -> Option<Near>,
    ) -> Vec<(DistrictSite, Near)> {
        let topo = self.index.topo.clone();
        let mut out = Vec::new();
        for d in self.engine.districts() {
            let frame = d.dim as u16;
            let t = topo.of(frame);
            for c in &d.chunks {
                let (x, z) = t.canonical(i64::from(c.0) * 16 + 8, i64::from(c.1) * 16 + 8);
                if let Some(n) = near(frame, x, z) {
                    out.push((
                        DistrictSite {
                            id: d.id,
                            pos: Pos::new(frame, x as i32, z as i32),
                            watched: d.watched,
                            loaded: d.presence != Presence::Unloaded,
                        },
                        n,
                    ));
                    break;
                }
            }
        }
        out
    }

    fn districts_near(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.index.refresh(self.engine);
        let t = self.index.topo.clone();
        let t = t.of(frame);
        let cells = self.index.cells_near(frame, x, z, r);
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
        self.raid_with(t, &[], 0)
    }

    fn raid_drinking(&mut self, t: &Threat, fuels: &[String], room: i64) -> Option<RaidOutcome> {
        let goods = self.fuel_goods(fuels);
        self.raid_with(t, &goods, room)
    }

    fn signals(&mut self, district: DistrictId, fuels: &[String]) -> DistrictSignals {
        let goods = self.fuel_goods(fuels);
        let stock = match self.engine.sim_stock(district) {
            Some(s) => s,
            None => self
                .engine
                .district(district)
                .map_or_else(GoodVec::new, |d| d.effective_stock()),
        };
        let fuel_mb = goods.iter().map(|g| stock.get(*g).max(0)).sum();
        let Some(d) = self.engine.district(district) else {
            return DistrictSignals::default();
        };
        let names = self.engine.goods();
        let oil_sample = d.deposits.values().any(|s| {
            matches!(
                s.plugin,
                DepositPluginId::DieselOil | DepositPluginId::DestroyCrude
            ) && (s.infinite || s.remaining > 0)
        });
        let polluted = d.deposits.values().any(|s| {
            s.remaining > 0
                && names
                    .get(&s.good)
                    .is_some_and(|i| rtsim_industry::deposit::is_level_pollution(&i.name))
        });
        DistrictSignals {
            oil_sample,
            polluted,
            fuel_mb,
        }
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

#[cfg(test)]
mod tests {
    use rtsim_industry::obs::{DistrictDecl, Obs, Timed};
    use rtsim_industry::tunables::Tunables;

    use super::*;

    fn declare(e: &mut Engine, id: u32, revision: u32, chunks: Vec<(i32, i32)>) {
        let at = Tick(0);
        let district = DistrictId(u64::from(id));
        e.ingest(&Timed {
            at,
            obs: Obs::District(DistrictDecl {
                district,
                revision,
                dim: 0,
                name: id,
                not_learnable: None,
                inventory_count: 0,
            }),
        });
        e.ingest(&Timed {
            at,
            obs: Obs::DistrictChunks {
                district,
                revision,
                part: 0,
                chunks,
            },
        });
    }

    /// The oil audit's B2 and C3: a well joined to a base 3,000 blocks away is one district whose
    /// centre is half way between. The centre test misses it at the well; the chunk test finds
    /// it, at the well's chunk.
    #[test]
    fn a_well_joined_to_a_far_base_is_sensed_at_the_well() {
        let mut e = Engine::new(1, Tunables::default(), Tick(0));
        let mut index = DistrictIndex::default();
        // The base: chunks round (0, 0). The well: one chunk at x = 3,000.
        let mut chunks: Vec<(i32, i32)> = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| (x, z)))
            .collect();
        chunks.push((3_000 / 16, 0));
        declare(&mut e, 7, 1, chunks);
        let mut link = IndustryLink {
            engine: &mut e,
            index: &mut index,
        };
        let (wx, wz) = (3_000, 8);
        assert!(
            link.districts_near(0, wx, wz, 64).is_empty(),
            "the centre test misses the joined well"
        );
        let found = link.districts_reaching(0, wx, wz, 64);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].id, DistrictId(7));
        assert_eq!(
            (found[0].pos.x, found[0].pos.z),
            (3_000 / 16 * 16 + 8, 8),
            "the site is the well's chunk"
        );
        // The base itself is still found at the base.
        assert_eq!(link.districts_reaching(0, 0, 0, 64).len(), 1);
        assert!(link.districts_reaching(0, 1_500, 0, 64).is_empty());
        // The fields' watch (OGC52) asks chunk by chunk: a field under the well answers there.
        let mut asked = 0;
        let hits = link.districts_where(&mut |_, x, z| {
            asked += 1;
            (x > 2_900).then_some(Near {
                field: 5,
                x: x as i32,
                z: z as i32,
                dist: 0.0,
            })
        });
        assert_eq!(asked, 26, "every chunk asked once, the well last");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.id, DistrictId(7));
        assert_eq!((hits[0].0.pos.x, hits[0].1.field), (3_000 / 16 * 16 + 8, 5));
    }

    /// `DistrictIndex::refresh` rebuilds when a district grows, not only when the count changes.
    #[test]
    fn the_index_sees_a_district_that_grew() {
        let mut e = Engine::new(1, Tunables::default(), Tick(0));
        let mut index = DistrictIndex::default();
        declare(&mut e, 3, 1, vec![(0, 0)]);
        {
            let mut link = IndustryLink {
                engine: &mut e,
                index: &mut index,
            };
            assert_eq!(link.districts_reaching(0, 8, 8, 32).len(), 1);
            assert!(link.districts_reaching(0, 2_008, 8, 32).is_empty());
        }
        // The same district, one more chunk far off: the count is unchanged.
        declare(&mut e, 3, 2, vec![(0, 0), (125, 0)]);
        let mut link = IndustryLink {
            engine: &mut e,
            index: &mut index,
        };
        let found = link.districts_reaching(0, 2_008, 8, 32);
        assert_eq!(
            found.iter().map(|s| s.id).collect::<Vec<_>>(),
            [DistrictId(3)]
        );
        // And a district that shrank away is gone from where it was.
        declare(link.engine, 3, 3, vec![(125, 0)]);
        assert!(link.districts_reaching(0, 8, 8, 32).is_empty());
        assert_eq!(link.districts_reaching(0, 2_008, 8, 32).len(), 1);
    }
}
