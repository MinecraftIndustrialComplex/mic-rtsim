// SPDX-License-Identifier: AGPL-3.0-or-later
//! The oil gate, the dragon half (OILGATE.md §4.1 to §4.5; choices in docs/choices-oilgate.md).
//!
//! - **The oil field index** ([`OilIndex`], OG-2): the oil chunks the plugin reports (Deep Time's
//!   traps at load, CDG's per-chunk amount as chunks generate, a survey around each new lair),
//!   in region lists of 32 by 32 chunks, and the fields they make: 8-neighbour groups, joined
//!   through a union-find as chunks arrive. It only grows: a chunk's oil in the ground is in
//!   nobody's books (OG-6), so nothing drains it.
//! - **The reserve** (OG-4): a fire dragon's `oil` falls one point every `tick_rate` ticks, one a
//!   breath while embodied (DRAGON_BREATHS) and one a raid pass while abstract. A dry dragon's
//!   passes lack fire: its raid strength falls to `dry_strength_milli` of a pass (OG-13).
//! - **Refuelling** (OG-5): a dragon whose reserve is low flies to the nearest oil chunk in reach,
//!   senses the districts around it (OG-8), raids the one with the most fuel in its tanks (the
//!   theft takes fuel first, up to the reserve's room) and rests on the field for a dwell.
//! - **Field flights** (OG-3): In Control's arrival process keyed to fields: each second, per
//!   dimension, with a chance, one random player there; near a field and high enough, a fire
//!   dragon is drawn and flies in from the edge of view. It drinks, harasses and leaves.
//! - **Lairs near oil** (OG-7): a field with no fire lair in reach gets one placed (PLACE_LAIR).
//!
//! **The gate is on only once some oil is known** (OGC12): a world whose index is empty keeps the
//! old rules (no reserve, no drinking), so a fire dragon never runs dry where it could never drink.
//!
//! **The underground half** (OG-9, OILGATE.md §4.6) is not built. Its seam: [`OilIndex::add_chunk`]
//! says when a chunk is new to the index, which is where its first load would send the deep
//! layer's command (0x8507, reserved in `boundary`).


use rtsim_industry::ids::DistrictId;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::meta_pages::PagedMap;
use rtsim_kernel::rng::{SlotRng, StreamKey, SystemId};
use rtsim_knowledge::{ActorId, ActorKind, Report, ReportKind, SEVERITY_UNIT};
use rtsim_settle::Pos;
use rtsim_space::topology::{Rect, Topology};
use rtsim_world::World;
use serde::Serialize;

use crate::boundary::{self, Cmd, Uuid};
use crate::dragon::{DragonId, LairKind, Purpose, SENSED_KEPT, State, Target};
use crate::engine::{BODY_RADIUS, Dragons, Ev, HOME_RADIUS, NewDragon, place};
use crate::flight;
use crate::infra::{DistrictSignals, DistrictSite, Infrastructure};
use crate::prey::PreyClass;
use crate::species::{Gender, Species};
use crate::tunables::PreyKind;

/// The oil index's page tables when the dragons page: the region lists and the fields.
pub const OIL_REGION_TABLE: u16 = 0x4417;
pub const OIL_FIELD_TABLE: u16 = 0x4418;

/// Chunks a side of an index region: 32 (a Minecraft region file's 512 blocks).
pub const REGION_CHUNKS: i64 = 32;
const CHUNK: i64 = 16;

const SYS_FIELD: SystemId = SystemId::named("ecology.field");

/// A district's actor: a group actor whose external id is the district id under this tag.
pub(crate) const DIST_TAG: u64 = 0x4449_5354 << 32; // "DIST"

/// Players' presence entries kept before the stale ones are dropped.
const PRESENCE_KEPT: usize = 32;

/// The oil gate's tunables. Every number is a PLACEHOLDER until OG-10's measurement.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OilTunables {
    /// Reserve points when full (OILGATE.md §4.3).
    pub capacity: i32,
    /// Ticks per point lost with time: the pack's `hungerTickRate`, so oil and food keep one
    /// rhythm.
    pub tick_rate: u64,
    /// A reserve at or below this is low: the dragon wakes for it and weighs refuelling.
    pub refuel_below: i32,
    /// Blocks from where it is to the oil chunk it flies to, at most.
    pub refuel_reach: u32,
    /// Ticks it rests on a field to fill up (OILGATE.md: an hour of game time, 1,000 ticks).
    pub refuel_dwell_ticks: u64,
    /// A pass without fire strikes at this share of a pass, thousandths (OG-13: bite and claws).
    pub dry_strength_milli: u32,
    /// mB of fuel per reserve point drunk from a tank (OG-5).
    pub mb_per_point: i64,
    /// "Near a field": within this many blocks of one of its chunks (In Control's `maxdist`).
    pub near_blocks: u32,
    /// Field flights: the chance per second, per dimension, thousandths (In Control's
    /// `persecond` 0.1).
    pub flight_chance_milli: u32,
    /// Field flights: the player at or above this height (In Control's `minheight`).
    pub flight_min_y: i32,
    /// Field flights alive per dimension at most (In Control's `maxthis`).
    pub flight_cap_frame: u32,
    /// Field flights alive per field at most.
    pub flight_cap_field: u32,
    /// A drawn dragon's age is 1 + below(this) days (Ice and Fire's `finalizeSpawn` roll).
    pub flight_age_days_max: u64,
    /// A field with no fire lair within this many blocks gets one placed (OG-7).
    pub lair_reach: u32,
    /// Districts within this many blocks of the oil chunk a dragon drinks at are sensed (OG-8).
    pub sense_reach: u32,
    /// A district holding at least this much fuel signals (one tank) and draws a dry dragon.
    pub fuel_signal_mb: i64,
    /// A new lair asks for CDG's oil within this many blocks (OIL_SURVEY).
    pub survey_radius: u32,
    /// A player's presence counts for field flights this long after its last report.
    pub presence_fresh_ticks: u64,
    /// The switches the engine starts with (`boundary::OIL_*`); OIL_CONFIG replaces them.
    pub flags: u32,
    /// The goods a dragon drinks (item names; a name also matches its `name/<hash>` variants).
    pub fuels: Vec<String>,
}

impl Default for OilTunables {
    fn default() -> Self {
        OilTunables {
            capacity: 100,
            tick_rate: 3_000,
            refuel_below: 40,
            refuel_reach: 3_000,
            refuel_dwell_ticks: 1_000,
            dry_strength_milli: 500,
            mb_per_point: 1_000,
            near_blocks: 100,
            flight_chance_milli: 100,
            flight_min_y: 15,
            flight_cap_frame: 200,
            flight_cap_field: 8,
            flight_age_days_max: 80,
            lair_reach: 1_500,
            sense_reach: 400,
            fuel_signal_mb: 1_000,
            survey_radius: 1_024,
            presence_fresh_ticks: 200,
            flags: boundary::OIL_PLACE_LAIRS,
            fuels: [
                "destroy:crude_oil",
                "createdieselgenerators:oil",
                "createdieselgenerators:crude_oil",
                "createdieselgenerators:diesel",
                "createdieselgenerators:gasoline",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}

/// Whether `name` is one of `fuels`, or one's `name/<hash>` variant.
pub fn is_fuel(fuels: &[String], name: &str) -> bool {
    fuels.iter().any(|f| {
        name == f
            || name
                .strip_prefix(f.as_str())
                .is_some_and(|r| r.starts_with('/'))
    })
}

/// One oil chunk in its region's list: the field it joined, its place in the region
/// (`lx * 32 + lz`) and whether it never runs out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OilChunk {
    pub field: u32,
    pub idx: u16,
    pub bottomless: bool,
}

/// An oil field: a union-find node. A root (`parent` itself) holds the field's counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub parent: u32,
    pub frame: u16,
    pub chunks: u32,
    pub bottomless: bool,
    /// The lair check ran (OG-7): once per field.
    pub lair_checked: bool,
    /// Field flights' dragons alive that were drawn for it.
    pub visitors: u32,
}

/// The nearest oil chunk to a point: its field (a root) and its centre block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    pub field: u32,
    pub x: i32,
    pub z: i32,
    pub dist: f64,
}

/// One dimension's field flights: the arrival process (§4.2) runs while `active`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Roll {
    pub plan: u32,
    pub active: bool,
    /// Field flights' dragons alive in the dimension.
    pub visitors: u32,
}

/// The oil field index (module docs). Resident, saved in pages of its own when the dragons page.
#[derive(Debug, Default)]
pub struct OilIndex {
    /// Oil chunks by (frame, region x, region z), sorted by `idx`.
    pub(crate) regions: PagedMap<(u16, i64, i64), Vec<OilChunk>>,
    pub(crate) fields: PagedMap<u32, Field>,
    pub(crate) next_field: u32,
    /// Fields that are roots.
    pub(crate) live: u32,
    pub(crate) chunks: u64,
}

fn region_of(cx: i64, cz: i64) -> (i64, i64, u16) {
    let (rx, rz) = (cx.div_euclid(REGION_CHUNKS), cz.div_euclid(REGION_CHUNKS));
    let idx = (cx - rx * REGION_CHUNKS) * REGION_CHUNKS + (cz - rz * REGION_CHUNKS);
    (rx, rz, idx as u16)
}

impl OilIndex {
    pub fn is_empty(&self) -> bool {
        self.chunks == 0
    }

    /// Oil chunks known.
    pub fn chunks(&self) -> u64 {
        self.chunks
    }

    /// Fields (connected groups of oil chunks).
    pub fn fields(&self) -> u32 {
        self.live
    }

    /// The field record of `f`'s root.
    pub fn field(&self, f: u32) -> Option<&Field> {
        self.fields.get(&self.find(f))
    }

    /// The root of `f` (no path compression: a read never dirties a page; union by size keeps
    /// the chains short).
    pub fn find(&self, mut f: u32) -> u32 {
        while let Some(x) = self.fields.get(&f) {
            if x.parent == f {
                break;
            }
            f = x.parent;
        }
        f
    }

    /// The oil chunk at canonical chunk (cx, cz), if any.
    pub fn chunk_at(&self, frame: u16, cx: i64, cz: i64) -> Option<OilChunk> {
        let (rx, rz, idx) = region_of(cx, cz);
        let v = self.regions.get(&(frame, rx, rz))?;
        v.binary_search_by_key(&idx, |c| c.idx).ok().map(|i| v[i])
    }

    fn root(&self, f: u32) -> Field {
        *self.fields.get(&f).expect("a field root")
    }

    /// Joins two fields: the bigger keeps its id (a tie, the lower).
    fn union(&mut self, a: u32, b: u32) -> u32 {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return a;
        }
        let (fa, fb) = (self.root(a), self.root(b));
        let (win, lose, fw, fl) = if fa.chunks > fb.chunks || (fa.chunks == fb.chunks && a < b) {
            (a, b, fa, fb)
        } else {
            (b, a, fb, fa)
        };
        self.fields.insert(
            win,
            Field {
                parent: win,
                frame: fw.frame,
                chunks: fw.chunks + fl.chunks,
                bottomless: fw.bottomless || fl.bottomless,
                lair_checked: fw.lair_checked || fl.lair_checked,
                visitors: fw.visitors + fl.visitors,
            },
        );
        self.fields.insert(
            lose,
            Field {
                parent: win,
                visitors: 0,
                ..fl
            },
        );
        self.live -= 1;
        win
    }

    /// An oil chunk joins the index (canonical through the frame's topology, TP4). Returns
    /// whether it is new: the seam for the underground half (OG-9b), whose deep layer would be
    /// placed on a new chunk's first load. A chunk known already may become bottomless.
    pub fn add_chunk(
        &mut self,
        t: &Topology,
        frame: u16,
        cx: i64,
        cz: i64,
        bottomless: bool,
    ) -> bool {
        let g = t.grid(CHUNK);
        let (cx, cz) = g.canonical(cx, cz);
        let (rx, rz, idx) = region_of(cx, cz);
        let key = (frame, rx, rz);
        if let Some(c) = self.chunk_at(frame, cx, cz) {
            if bottomless && !c.bottomless {
                let v = self.regions.get_mut(&key).expect("held");
                if let Ok(i) = v.binary_search_by_key(&idx, |c| c.idx) {
                    v[i].bottomless = true;
                }
                let r = self.find(c.field);
                if let Some(f) = self.fields.get_mut(&r) {
                    f.bottomless = true;
                }
            }
            return false;
        }
        let mut roots: Vec<u32> = Vec::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                if (dx, dz) == (0, 0) {
                    continue;
                }
                let (nx, nz, _) = g.neighbour(cx, cz, dx, dz);
                if let Some(c) = self.chunk_at(frame, nx, nz) {
                    let r = self.find(c.field);
                    if !roots.contains(&r) {
                        roots.push(r);
                    }
                }
            }
        }
        roots.sort_unstable();
        let field = match roots.split_first() {
            None => {
                let id = self.next_field;
                self.next_field += 1;
                self.live += 1;
                self.fields.insert(
                    id,
                    Field {
                        parent: id,
                        frame,
                        chunks: 0,
                        bottomless: false,
                        lair_checked: false,
                        visitors: 0,
                    },
                );
                id
            }
            Some((&first, rest)) => rest.iter().fold(first, |a, &b| self.union(a, b)),
        };
        let f = self.fields.get_mut(&field).expect("a root");
        f.chunks += 1;
        f.bottomless |= bottomless;
        let v = self.regions.get_or_default(key);
        let i = v.partition_point(|c| c.idx < idx);
        v.insert(
            i,
            OilChunk {
                field,
                idx,
                bottomless,
            },
        );
        self.chunks += 1;
        true
    }

    /// The oil chunk nearest (x, z) whose centre is within `r` blocks, through the topology
    /// (TP3); ties go to the lower chunk. One lookup per index region the circle covers.
    pub fn near(&self, t: &Topology, frame: u16, x: i64, z: i64, r: i64) -> Option<Near> {
        if self.chunks == 0 {
            return None;
        }
        let mut best: Option<Near> = None;
        for part in t.rects_near(x, z, r) {
            let (cx0, cz0, cx1, cz1) = part.grid_cells(CHUNK);
            if cx1 < cx0 || cz1 < cz0 {
                continue;
            }
            for rx in cx0.div_euclid(REGION_CHUNKS)..=cx1.div_euclid(REGION_CHUNKS) {
                for rz in cz0.div_euclid(REGION_CHUNKS)..=cz1.div_euclid(REGION_CHUNKS) {
                    let Some(v) = self.regions.get(&(frame, rx, rz)) else {
                        continue;
                    };
                    let (bx, bz) = (rx * REGION_CHUNKS, rz * REGION_CHUNKS);
                    let (lx0, lx1) = ((cx0 - bx).max(0), (cx1 - bx).min(REGION_CHUNKS - 1));
                    let (lz0, lz1) = ((cz0 - bz).max(0), (cz1 - bz).min(REGION_CHUNKS - 1));
                    let a = v.partition_point(|c| i64::from(c.idx) < lx0 * REGION_CHUNKS);
                    let b = v.partition_point(|c| i64::from(c.idx) < (lx1 + 1) * REGION_CHUNKS);
                    for c in &v[a..b] {
                        let (lx, lz) = (
                            i64::from(c.idx) / REGION_CHUNKS,
                            i64::from(c.idx) % REGION_CHUNKS,
                        );
                        if lz < lz0 || lz > lz1 {
                            continue;
                        }
                        let p = ((bx + lx) * CHUNK + CHUNK / 2, (bz + lz) * CHUNK + CHUNK / 2);
                        let d = t.dist((x, z), p);
                        if d > r as f64 {
                            continue;
                        }
                        let better = best.as_ref().is_none_or(|b| {
                            d.total_cmp(&b.dist)
                                .then((p.0, p.1).cmp(&(i64::from(b.x), i64::from(b.z))))
                                .is_lt()
                        });
                        if better {
                            best = Some(Near {
                                field: self.find(c.field),
                                x: p.0 as i32,
                                z: p.1 as i32,
                                dist: d,
                            });
                        }
                    }
                }
            }
        }
        best
    }

    /// Heap bytes, estimated as B-tree entries at two-thirds fill plus each region's list.
    pub fn heap_bytes(&self) -> usize {
        let node = |entry: usize| entry * 3 / 2;
        self.regions.len() * node(24 + 24)
            + self
                .regions
                .values()
                .map(|v| v.capacity() * std::mem::size_of::<OilChunk>())
                .sum::<usize>()
            + self.fields.len() * node(4 + std::mem::size_of::<Field>())
            + self.regions.heap_bytes()
            + self.fields.heap_bytes()
    }
}

/// What a raid's passes burn and drink: the strength factor, the points burnt, the room in mB the
/// theft may fill with fuel, and whether some passes lacked fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OilPasses {
    pub factor: f64,
    pub burn: i32,
    pub room_mb: i64,
    pub dry: bool,
}

impl Dragons {
    /// Whether the oil rules apply to `d`: its species breathes oil and some oil is known (module
    /// docs, OGC12).
    pub fn oil_gated(&self, d: DragonId) -> bool {
        !self.oil.is_empty()
            && self
                .dragons
                .get(d)
                .is_some_and(|x| self.tun.species(x.species).uses_oil)
    }

    /// `d`'s oil reserve at `at`.
    pub fn oil_level(&self, d: DragonId, at: Tick) -> Option<i32> {
        self.dragons.get(d).map(|x| x.oil_level(&self.tun.oil, at))
    }

    /// Field flights alive in `frame`.
    pub fn field_visitors(&self, frame: u16) -> u32 {
        self.rolls.get(&frame).map_or(0, |r| r.visitors)
    }

    /// A PLAYER record with the player's height (the bridge's): the home counter (P26), and the
    /// oil gate's presence, which starts a dimension's field flights (§4.2) and checks a field
    /// for a lair (OG-7) when the player is near one.
    pub fn player_at(&mut self, w: &mut World, uuid: Uuid, pos: Pos, y: i32, at: Tick) -> ActorId {
        let p = self.observe_home(w, uuid, pos, at);
        self.presence.insert(p, (pos, y, at));
        if self.presence.len() > PRESENCE_KEPT {
            let fresh = self.tun.oil.presence_fresh_ticks;
            self.presence.retain(|_, (_, _, t)| at.since(*t) <= fresh);
        }
        self.oil_presence(pos, y, at);
        p
    }

    fn oil_presence(&mut self, pos: Pos, y: i32, at: Tick) {
        if self.oil.is_empty() {
            return;
        }
        let o = &self.tun.oil;
        let flights = self.oil_flags & boundary::OIL_FIELD_FLIGHTS != 0
            && o.flight_chance_milli > 0
            && y >= o.flight_min_y;
        let lairs = self.oil_flags & boundary::OIL_PLACE_LAIRS != 0;
        if !flights && !lairs {
            return;
        }
        let r = i64::from(o.near_blocks);
        let topo = self.topo.clone();
        let Some(n) = self.oil.near(
            topo.of(pos.frame),
            pos.frame,
            i64::from(pos.x),
            i64::from(pos.z),
            r,
        ) else {
            return;
        };
        if flights && !self.rolls.get(&pos.frame).is_some_and(|r| r.active) {
            let r = self.rolls.entry(pos.frame).or_default();
            r.active = true;
            r.plan = r.plan.wrapping_add(1);
            let plan = r.plan;
            let mut rng =
                StreamKey::new(self.seed, SYS_FIELD, u64::from(pos.frame) | 1 << 32).slot(at.0);
            let next = at + self.roll_gap(&mut rng);
            self.schedule(
                next,
                Ev::FieldRoll {
                    frame: pos.frame,
                    plan,
                },
            );
        }
        if lairs {
            self.check_lair(n, pos.frame, at);
        }
    }

    /// Ticks to the next second whose roll succeeds: a geometric number of seconds (In Control's
    /// chance is per run, one run a second), at least one.
    fn roll_gap(&self, rng: &mut SlotRng) -> u64 {
        let p = f64::from(self.tun.oil.flight_chance_milli) / 1000.0;
        let k = if p >= 1.0 {
            1
        } else if p <= 0.0 {
            1 << 32
        } else {
            let u = rng.next_f64_open();
            ((u.ln() / (1.0 - p).ln()).ceil() as u64).clamp(1, 1 << 32)
        };
        20 * k
    }

    /// OG-7: a field with no fire lair within `lair_reach` gets one placed near the oil chunk the
    /// player is near (the plugin finds a free site on its first load). Once per field.
    fn check_lair(&mut self, n: Near, frame: u16, at: Tick) {
        let root = n.field;
        if self.oil.fields.get(&root).is_none_or(|f| f.lair_checked) {
            return;
        }
        if let Some(f) = self.oil.fields.get_mut(&root) {
            f.lair_checked = true;
        }
        let reach = i64::from(self.tun.oil.lair_reach);
        let (x, z) = (i64::from(n.x), i64::from(n.z));
        let topo = self.topo.clone();
        let t = topo.of(frame);
        for (rx, rz) in self.regions_near(frame, Rect::around(x, z, reach)) {
            for &l in self.lair_cells.get(&(frame, rx, rz)).into_iter().flatten() {
                let lair = &self.lairs[l as usize];
                if self.tun.species(lair.species).uses_oil
                    && flight::dist(t, (x, z), (i64::from(lair.pos.x), i64::from(lair.pos.z)))
                        <= reach as f64
                {
                    return;
                }
            }
        }
        self.stats.lairs_placed += 1;
        self.outbox.push(Cmd::PlaceLair(boundary::PlaceLair {
            field: u64::from(root),
            dimension: u32::from(frame),
            species: Species::Fire,
            kind: LairKind::Roost,
            x: n.x,
            z: n.z,
            radius: self.tun.oil.near_blocks,
        }));
        self.note(
            at,
            "oil",
            None,
            format!(
                "field {root} has no fire lair within {reach} blocks: one is placed near ({}, {})",
                n.x, n.z
            ),
        );
    }

    /// A dimension's field-flight roll (§4.2): one random player of the dimension; near a field,
    /// high enough and under the caps, a fire dragon is drawn. The process stops when no player
    /// there qualifies (a player coming near a field starts it again).
    pub(crate) fn field_roll(&mut self, w: &mut World, frame: u16, plan: u32, at: Tick) {
        let Some(r) = self.rolls.get(&frame).copied() else {
            return;
        };
        if r.plan != plan || !r.active {
            self.stats.stale += 1;
            return;
        }
        self.stats.field_rolls += 1;
        let o = self.tun.oil.clone();
        let topo = self.topo.clone();
        let t = topo.of(frame);
        let fresh: Vec<(Pos, i32)> = self
            .presence
            .values()
            .filter(|(p, _, s)| p.frame == frame && at.since(*s) <= o.presence_fresh_ticks)
            .map(|(p, y, _)| (*p, *y))
            .collect();
        let on = self.oil_flags & boundary::OIL_FIELD_FLIGHTS != 0 && o.flight_chance_milli > 0;
        let near = |oil: &OilIndex, p: &Pos| {
            oil.near(
                t,
                frame,
                i64::from(p.x),
                i64::from(p.z),
                i64::from(o.near_blocks),
            )
        };
        let any = on
            && fresh
                .iter()
                .any(|(p, y)| *y >= o.flight_min_y && near(&self.oil, p).is_some());
        if !any {
            if let Some(r) = self.rolls.get_mut(&frame) {
                r.active = false;
            }
            return;
        }
        let mut rng = StreamKey::new(self.seed, SYS_FIELD, u64::from(frame)).slot(at.0);
        let (p, y) = fresh[rng.below(fresh.len() as u64) as usize];
        if y >= o.flight_min_y
            && let Some(n) = near(&self.oil, &p)
        {
            let on_field = self.oil.fields.get(&n.field).map_or(0, |f| f.visitors);
            if r.visitors < o.flight_cap_frame && on_field < o.flight_cap_field {
                self.draw_visitor(w, p, n, &mut rng, at);
            }
        }
        let next = at + self.roll_gap(&mut rng);
        self.schedule(next, Ev::FieldRoll { frame, plan });
    }

    /// A field flight's dragon: Ice and Fire's own age, sex and colour rolls, a full reserve,
    /// from just past the edge of the player's view, flying to the oil chunk near the player.
    fn draw_visitor(&mut self, w: &mut World, player: Pos, n: Near, rng: &mut SlotRng, at: Tick) {
        let frame = player.frame;
        let age_days = 1 + rng.below(self.tun.oil.flight_age_days_max.max(1));
        let gender = if rng.below(2) == 0 {
            Gender::Female
        } else {
            Gender::Male
        };
        let variant = rng.below(4) as u8;
        let angle = rng.next_f64() * std::f64::consts::TAU;
        let r = f64::from(self.tun.view_radius + 32);
        let (fx, fz) = self.topo(frame).canonical_i32(
            player.x + (r * angle.cos()) as i32,
            player.z + (r * angle.sin()) as i32,
        );
        let cruise = self.tun.species(Species::Fire).cruise_y;
        self.stats.field_draws += 1;
        let full = self.tun.full;
        let d = self.add_dragon(
            w,
            NewDragon {
                name: format!("field visitor {}", self.stats.field_draws),
                species: Species::Fire,
                gender,
                variant,
                age_ticks: age_days * TICKS_PER_DAY,
                lair: None,
                fullness: full,
                health: None,
                seed: None,
                at: Some((Pos::new(frame, fx, fz), cruise)),
            },
            at,
        );
        self.dragons.get_mut(d).expect("live").visitor = Some(n.field);
        if let Some(f) = self.oil.fields.get_mut(&n.field) {
            f.visitors += 1;
        }
        if let Some(r) = self.rolls.get_mut(&frame) {
            r.visitors += 1;
        }
        self.note(
            at,
            "visitor",
            Some(d),
            format!(
                "drawn for field {} ({age_days} days old): flies in to drink at ({}, {})",
                n.field, n.x, n.z
            ),
        );
        self.fly(
            w,
            d,
            Purpose::Refuel {
                field: n.field,
                frame,
                x: n.x,
                z: n.z,
            },
            at,
        );
    }

    /// A visitor's visit is over (it drank, or it left a player's view): one carrying nothing
    /// leaves the sim; one carrying valuables claims a vacant lair in reach or stays a wanderer.
    pub(crate) fn visitor_done(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        if x.visitor.is_none() {
            return;
        }
        if x.carried.is_empty() {
            self.stats.visitors_left += 1;
            let actor = x.actor;
            self.note(at, "visitor", Some(d), "leaves the sim".into());
            if w.knowledge.notable(actor).is_some() {
                w.knowledge.die(actor, at);
            }
            self.remove(w, d, at);
            return;
        }
        self.end_visit(d);
        match self.vacant_lair(d, at) {
            Some(l) => {
                let name = self.lairs[l as usize].name.clone();
                self.note(
                    at,
                    "visitor",
                    Some(d),
                    format!("carries a hoard: settles at {name}"),
                );
                self.lairs[l as usize].plan += 1;
                self.lairs[l as usize].vacant_since = None;
                self.track_lair(l);
                self.fly(w, d, Purpose::Claim(l), at);
            }
            None => self.roost(w, d, at),
        }
    }

    /// `d` is no longer a field flight's dragon: its field's and dimension's counts fall.
    pub(crate) fn end_visit(&mut self, d: DragonId) {
        let Some(f) = self.dragons.get(d).and_then(|x| x.visitor) else {
            return;
        };
        self.dragons.get_mut(d).expect("live").visitor = None;
        let root = self.oil.find(f);
        let frame = self.oil.fields.get(&root).map(|x| x.frame);
        if let Some(x) = self.oil.fields.get_mut(&root) {
            x.visitors = x.visitors.saturating_sub(1);
        }
        if let Some(fr) = frame
            && let Some(r) = self.rolls.get_mut(&fr)
        {
            r.visitors = r.visitors.saturating_sub(1);
        }
    }

    /// Whether `d`'s reserve is low (and the gate applies to it).
    pub(crate) fn oil_low(&self, d: DragonId, at: Tick) -> bool {
        self.oil_gated(d) && self.dragons[d].oil_level(&self.tun.oil, at) <= self.tun.oil.refuel_below
    }

    /// Refuelling as a choice (§4.3): to the nearest oil chunk in reach of where it is, its
    /// utility the reserve's room less the flight's cost. `None` when not low or nothing is in
    /// reach.
    pub(crate) fn refuel_choice(&self, d: DragonId, at: Tick) -> Option<(Purpose, f64)> {
        if !self.oil_low(d, at) {
            return None;
        }
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species);
        if !x.stage(sp, at).hunts() {
            return None;
        }
        let o = &self.tun.oil;
        let level = x.oil_level(o, at);
        let (p, _) = x.position(&self.topo, &self.lairs, at);
        let n = self.oil.near(
            self.topo(p.frame),
            p.frame,
            i64::from(p.x),
            i64::from(p.z),
            i64::from(o.refuel_reach),
        )?;
        let u = f64::from(o.capacity - level)
            - n.dist / 1000.0 * f64::from(self.tun.distance_cost_per_kblock);
        (u > 0.0).then_some((
            Purpose::Refuel {
                field: n.field,
                frame: p.frame,
                x: n.x,
                z: n.z,
            },
            u,
        ))
    }

    /// Arriving at an oil chunk: it senses the districts around (OG-8); not full, it raids the
    /// one holding the most fuel (OG-5; watched, it materializes there instead); then it rests
    /// on the field to fill up, unless the tanks filled it. A visitor always rests a while.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn refuel_arrive(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        field: u32,
        frame: u16,
        x: i32,
        z: i32,
        at: Tick,
    ) {
        let pos = Pos::new(frame, x, z);
        let sensed = self.sense(w, infra, d, pos, at);
        let full = |s: &Dragons| s.dragons[d].oil_level(&s.tun.oil, at) >= s.tun.oil.capacity;
        if !full(self) && self.oil_gated(d) {
            let signal = self.tun.oil.fuel_signal_mb;
            let tank = sensed
                .iter()
                .filter(|(_, s, _)| s.fuel_mb >= signal)
                .max_by(|a, b| a.1.fuel_mb.cmp(&b.1.fuel_mb).then(b.0.id.0.cmp(&a.0.id.0)))
                .map(|(s, _, fresh)| (*s, *fresh));
            if let Some((site, fresh)) = tank {
                let cruise = self.tun.species(self.dragons[d].species).cruise_y;
                // A watched district's tanks draw it once a day (the day it is first sensed):
                // driven off, it rests on the field instead of coming back at once.
                if site.watched {
                    if fresh && let Some(p) = self.player_near(site.pos, 2 * self.tun.view_radius) {
                        self.note(
                            at,
                            "refuel",
                            Some(d),
                            format!("dry: flies at the tanks of district {} (watched)", site.id.0),
                        );
                        self.materialize(w, d, p, true, false, Some((site.pos, cruise)), at);
                        return;
                    }
                } else {
                    let r = self.raid_sites(infra, d, vec![site], at);
                    self.feed(d, at, r.food, &[]);
                    if r.any {
                        let name = format!("district {} (tanks)", site.id.0);
                        self.record_hunt(at, d, PreyKind::Grudge, name, r.food, r.taken);
                    }
                }
            }
        }
        if full(self) && self.dragons[d].visitor.is_none() {
            self.after_refuel(w, d, at);
            return;
        }
        let plan = self.bump(d);
        let until = at + self.tun.oil.refuel_dwell_ticks.max(1);
        self.book_leg(w, d, flight::stay(pos, at, until, BODY_RADIUS), 64);
        self.note(
            at,
            "refuel",
            Some(d),
            format!("rests on field {field} at ({x}, {z})"),
        );
        self.schedule(until, Ev::RefuelEnd { d, plan });
    }

    /// Flies to drink (a choice [`Dragons::refuel_choice`] made).
    pub(crate) fn refuel_now(&mut self, w: &mut World, d: DragonId, p: Purpose, u: f64, at: Tick) {
        if let Purpose::Refuel { field, x, z, .. } = p {
            let level = self.dragons[d].oil_level(&self.tun.oil, at);
            self.note(
                at,
                "refuel",
                Some(d),
                format!("low on oil ({level}): flies to field {field} at ({x}, {z}) (utility {u:.1})"),
            );
        }
        self.fly(w, d, p, at);
    }

    /// The dwell on a field ends: it leaves full.
    pub(crate) fn refuel_end(&mut self, w: &mut World, d: DragonId, at: Tick) {
        if !matches!(self.dragons[d].state, State::Flying(Purpose::Refuel { .. })) {
            return;
        }
        let cap = self.tun.oil.capacity;
        self.dragons
            .get_mut(d)
            .expect("live")
            .set_oil(&self.tun.oil, cap, at);
        self.stats.refuels += 1;
        self.after_refuel(w, d, at);
    }

    /// After refuelling: a visitor's visit is over; a hungry dragon hunts; else home.
    fn after_refuel(&mut self, w: &mut World, d: DragonId, at: Tick) {
        if self.dragons[d].visitor.is_some() {
            self.visitor_done(w, d, at);
            return;
        }
        if self.dragons[d].fullness_at(&self.tun, at) < self.tun.hunt_below {
            self.decide(w, d, at, 1);
        } else {
            self.fly(w, d, Purpose::Home, at);
        }
    }

    /// Sensing (OG-8): each district within `sense_reach` of the oil chunk `d` drinks at, once a
    /// day each, is an Encroachment report against the district (and against each player whose
    /// home is there, P26's homes standing in for its builders) that `d` witnesses. Returns every
    /// district near, with its signals and whether it was sensed now (not already today).
    pub(crate) fn sense(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        pos: Pos,
        at: Tick,
    ) -> Vec<(DistrictSite, DistrictSignals, bool)> {
        let reach = i64::from(self.tun.oil.sense_reach);
        let sites = infra.districts_near(pos.frame, i64::from(pos.x), i64::from(pos.z), reach);
        let day = at.day().min(u64::from(u32::MAX)) as u32;
        let fuels = self.tun.oil.fuels.clone();
        let signal = self.tun.oil.fuel_signal_mb;
        let mut out = Vec::with_capacity(sites.len());
        for site in sites {
            let sig = infra.signals(site.id, &fuels);
            if self.dragons[d].sensed.contains(&(site.id.0, day)) {
                out.push((site, sig, false));
                continue;
            }
            out.push((site, sig, true));
            let units = 1
                + 2 * u32::from(sig.oil_sample)
                + u32::from(sig.fuel_mb >= signal)
                + u32::from(sig.polluted);
            let severity = (units * u32::from(SEVERITY_UNIT)).clamp(1, 255) as u8;
            let district = w
                .knowledge
                .actors_mut()
                .register(ActorKind::Group, DIST_TAG ^ site.id.0);
            self.districts.insert(district, (site.id.0, site.pos));
            let me = self.dragons[d].actor;
            let t = self.topo(site.pos.frame);
            let mut who = vec![district];
            for &p in self.players.keys() {
                let home = self.homes.home(p, at).filter(|h| {
                    h.pos.frame == site.pos.frame
                        && flight::dist(
                            t,
                            (i64::from(h.pos.x), i64::from(h.pos.z)),
                            (i64::from(site.pos.x), i64::from(site.pos.z)),
                        ) <= HOME_RADIUS as f64
                });
                if home.is_some() {
                    who.push(p);
                }
            }
            for a in who {
                let id = w.knowledge.record(Report {
                    who: a,
                    whom: me,
                    kind: ReportKind::Encroachment,
                    severity,
                    flags: 0,
                    at,
                    place: place(pos, 64),
                });
                w.knowledge.witness(me, id, at);
            }
            let x = self.dragons.get_mut(d).expect("live");
            x.sensed.retain(|(id, _)| *id != site.id.0);
            x.sensed.push((site.id.0, day));
            if x.sensed.len() > SENSED_KEPT {
                x.sensed.remove(0);
            }
            self.stats.encroachments += 1;
            let s = w.knowledge.sentiment(me, district, at);
            self.note(
                at,
                "sense",
                Some(d),
                format!(
                    "senses district {} (severity {severity}: oil sample {}, fuel {} mB, polluted {}): sentiment {s}",
                    site.id.0, sig.oil_sample, sig.fuel_mb, sig.polluted
                ),
            );
        }
        out
    }

    /// The player last seen nearest `pos`, within `r` blocks (a watched district's player).
    pub(crate) fn player_near(&self, pos: Pos, r: u32) -> Option<ActorId> {
        let t = self.topo(pos.frame);
        let mut best: Option<(f64, ActorId)> = None;
        for &p in self.players.keys() {
            let Some((q, _)) = self.homes.last_seen(p) else {
                continue;
            };
            if q.frame != pos.frame {
                continue;
            }
            let dd = flight::dist(
                t,
                (i64::from(q.x), i64::from(q.z)),
                (i64::from(pos.x), i64::from(pos.z)),
            );
            if dd <= f64::from(r) && best.is_none_or(|b| dd.total_cmp(&b.0).then(p.cmp(&b.1)).is_lt())
            {
                best = Some((dd, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// A grudge raid on a district (OG-8, `Target::District`): through the threat interface as
    /// a home's districts are; watched, a real fight at the district.
    pub(crate) fn raid_district(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        t: Target,
        at: Tick,
    ) {
        let Target::District {
            district,
            frame,
            x,
            z,
        } = t
        else {
            return;
        };
        self.stats.district_target_raids += 1;
        let sites: Vec<DistrictSite> = infra
            .districts_near(frame, i64::from(x), i64::from(z), HOME_RADIUS)
            .into_iter()
            .filter(|s| s.id == DistrictId(district))
            .collect();
        let r = self.raid_sites(infra, d, sites, at);
        if let Some(site) = r.watched {
            let cruise = self.tun.species(self.dragons[d].species).cruise_y;
            if let Some(p) = self.player_near(site.pos, 2 * self.tun.view_radius) {
                self.materialize(w, d, p, true, true, Some((site.pos, cruise)), at);
                return;
            }
        }
        self.feed(d, at, r.food, &[]);
        let name = format!("district {district}");
        self.record_hunt(at, d, PreyKind::Grudge, name, r.food, r.taken);
        if !r.any {
            self.dragons.get_mut(d).expect("live").fruitless += 1;
        }
        self.fly(w, d, Purpose::Home, at);
    }

    /// What a raid of `passes` passes burns (§4.3): one point a pass while the reserve lasts; the
    /// passes beyond it strike at `dry_strength_milli` (OG-13). The theft may drink up to the
    /// room left after the burn. Nothing for a dragon the gate does not apply to.
    pub(crate) fn oil_passes(&self, d: DragonId, passes: u32, at: Tick) -> OilPasses {
        if !self.oil_gated(d) {
            return OilPasses {
                factor: 1.0,
                burn: 0,
                room_mb: 0,
                dry: false,
            };
        }
        let o = &self.tun.oil;
        let level = self.dragons[d].oil_level(o, at);
        let p = passes.clamp(1, i32::MAX as u32) as i32;
        if level >= p {
            OilPasses {
                factor: 1.0,
                burn: p,
                room_mb: i64::from(o.capacity - (level - p)) * o.mb_per_point,
                dry: false,
            }
        } else {
            let share = f64::from(o.dry_strength_milli) / 1000.0;
            OilPasses {
                factor: (f64::from(level) + f64::from(p - level) * share) / f64::from(p),
                burn: level,
                room_mb: i64::from(o.capacity) * o.mb_per_point,
                dry: true,
            }
        }
    }

    /// Stolen goods that are fuel go into the reserve (burnt in the ledger's split); returns the
    /// mB drunk.
    pub(crate) fn drink(&mut self, d: DragonId, mb: i64, at: Tick) {
        if mb <= 0 {
            return;
        }
        let pts = (mb / self.tun.oil.mb_per_point.max(1)).min(i64::from(i32::MAX)) as i32;
        self.dragons
            .get_mut(d)
            .expect("live")
            .refill_oil(&self.tun.oil, pts, at);
        self.stats.oil_raids += 1;
        self.stats.oil_drunk_mb += mb as u64;
    }

    /// The class a stolen industry good takes in the ledger: fuel a dragon drank is burnt.
    pub(crate) fn stolen_class(&self, name: &str, drinking: bool) -> (PreyClass, bool) {
        if drinking && is_fuel(&self.tun.oil.fuels, name) {
            return (PreyClass::Other, true);
        }
        (self.tun.prey.class(name, false), false)
    }

    /// OIL_CHUNKS: the chunks with oil join the index (OG-2).
    pub(crate) fn oil_chunks(&mut self, o: &boundary::OilChunks, at: Tick) {
        let frame = o.dimension as u16;
        let topo = self.topo.clone();
        let t = topo.of(frame);
        let before = self.oil.chunks;
        for c in &o.chunks {
            if c.amount <= 0 {
                continue;
            }
            // A new chunk is the underground half's seam (OG-9b: its first load would place the
            // deep layer, command 0x8507). Nothing is sent for it yet.
            let _new = self.oil.add_chunk(
                t,
                frame,
                i64::from(c.cx),
                i64::from(c.cz),
                c.flags & boundary::OIL_BOTTOMLESS != 0,
            );
        }
        let added = self.oil.chunks - before;
        self.stats.oil_chunks += added;
        self.stats.fields = u64::from(self.oil.live);
        if added > 0 {
            self.note(
                at,
                "oil",
                None,
                format!(
                    "{added} oil chunks join the index (source {}): {} chunks in {} fields",
                    o.source, self.oil.chunks, self.oil.live
                ),
            );
        }
    }

    /// DRAGON_BREATHS: an embodied dragon's fire costs a point a breath (OG-4).
    pub(crate) fn breaths(&mut self, b: &boundary::Breaths, at: Tick) {
        self.stats.breaths += u64::from(b.count);
        let Some(d) = self.embodied(b.agent, b.epoch) else {
            return;
        };
        if self.oil_gated(d) {
            let n = b.count.min(i32::MAX as u32) as i32;
            self.dragons
                .get_mut(d)
                .expect("live")
                .burn_oil(&self.tun.oil, n, at);
        }
    }

    /// A new fire lair asks for the oil around it ahead of generation (OIL_SURVEY, OG-2).
    pub(crate) fn survey(&mut self, dimension: u32, x: i32, z: i32) {
        self.stats.surveys += 1;
        self.outbox.push(Cmd::OilSurvey(boundary::OilSurvey {
            dimension,
            x,
            z,
            radius: self.tun.oil.survey_radius,
        }));
    }

    /// The oil gate's resident maps, estimated (the index apart).
    pub(crate) fn oil_gate_bytes(&self) -> usize {
        let node = |entry: usize| entry * 3 / 2;
        self.presence.len() * node(8 + 32)
            + self.rolls.len() * node(2 + 12)
            + self.districts.len() * node(8 + 8 + 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_space::topology::all_flat;

    #[test]
    fn fields_join_through_neighbours() {
        let topo = all_flat();
        let t = topo.of(0);
        let mut ix = OilIndex::default();
        assert!(ix.add_chunk(t, 0, 0, 0, false));
        assert!(ix.add_chunk(t, 0, 5, 0, false));
        assert_eq!(ix.fields(), 2);
        assert!(!ix.add_chunk(t, 0, 0, 0, false));
        // A chunk two from each touches neither: a third field. One more joins it to the first.
        assert!(ix.add_chunk(t, 0, 2, 1, false));
        assert_eq!(ix.fields(), 3);
        assert!(ix.add_chunk(t, 0, 1, 0, false));
        assert_eq!(ix.fields(), 2);
        assert!(ix.add_chunk(t, 0, 3, 1, false));
        assert!(ix.add_chunk(t, 0, 4, 0, true));
        assert_eq!(ix.fields(), 1);
        let root = ix.find(ix.chunk_at(0, 5, 0).unwrap().field);
        assert_eq!(ix.find(ix.chunk_at(0, 0, 0).unwrap().field), root);
        let f = ix.field(root).unwrap();
        assert_eq!(f.chunks, 6);
        assert!(f.bottomless);
        assert_eq!(ix.chunks(), 6);
        // Across a region edge.
        assert!(ix.add_chunk(t, 0, -1, -1, false));
        assert_eq!(ix.fields(), 1);
        assert_eq!(ix.field(root).unwrap().chunks, 7);
        // A chunk known already may turn bottomless, and counts once.
        assert!(!ix.add_chunk(t, 0, -1, -1, true));
        assert!(ix.chunk_at(0, -1, -1).unwrap().bottomless);
        assert_eq!(ix.chunks(), 7);
    }

    #[test]
    fn near_finds_the_closest_chunk() {
        let topo = all_flat();
        let t = topo.of(0);
        let mut ix = OilIndex::default();
        assert_eq!(ix.near(t, 0, 0, 0, 1_000), None);
        ix.add_chunk(t, 0, 10, 10, false);
        ix.add_chunk(t, 0, 40, 10, false);
        ix.add_chunk(t, 0, -40, -3, false);
        let n = ix.near(t, 0, 168, 168, 100).unwrap();
        assert_eq!((n.x, n.z), (168, 168));
        assert_eq!(n.dist, 0.0);
        let n = ix.near(t, 0, 500, 168, 500).unwrap();
        assert_eq!((n.x, n.z), (648, 168));
        assert!(ix.near(t, 0, 400, 400, 100).is_none());
        let n = ix.near(t, 0, -600, -40, 100).unwrap();
        assert_eq!((n.x, n.z), (-632, -40));
        // Another frame holds nothing.
        assert!(ix.near(t, 1, 168, 168, 100).is_none());
    }

    #[test]
    fn fuel_names_match_their_variants() {
        let f = OilTunables::default().fuels;
        assert!(is_fuel(&f, "destroy:crude_oil"));
        assert!(is_fuel(&f, "createdieselgenerators:diesel/9a1f"));
        assert!(!is_fuel(&f, "createdieselgenerators:diesel_engine"));
        assert!(!is_fuel(&f, "minecraft:water"));
    }
}
