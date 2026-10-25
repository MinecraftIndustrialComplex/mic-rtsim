// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herd engine: every herd's life as events on one timing wheel (PLAN.md §10.2, §4.3, §4.4;
//! SCALE.md §2 to §4).
//!
//! # What happens, and when
//!
//! - **A decision per sub-season** (staggered by a hash of the herd so nothing spikes). The herd
//!   is charged for its grazing since the last one (the forage field falls, body condition moves,
//!   a hungry herd starves), gives birth if this is its breeding sub-season (one binomial draw on
//!   the adults), and picks its next target from the forage flow field within its comfort band
//!   (`route`). Its route is a closed form of time until the next decision.
//! - **Deaths are never stepped.** They are dyadic counts per year-class (`cohort`), read in
//!   O(log) whenever anything asks, so a herd left alone for years costs nothing between its
//!   decisions, and a read after ten thousand days costs what a read after one does (O5).
//! - **Legs are booked lazily** (A15). A herd's route is indexed by region in the engine itself;
//!   only when a player's interest area is near does the part of the route inside the area's
//!   window go into the shared leg index (one leg per herd and player), and the first moment the
//!   footprint reaches into the area is one `Seen` event, stamped with both legs' generations.
//! - **Materialization** samples the group from the herd (`members`): count × the share of the
//!   footprint inside the area at closest approach, capped per herd and per player. The group
//!   materializes, steers and despawns as one unit (C25), once per approach; it is looked at every
//!   few hundred ticks while embodied (an observed instance: allowed cost).
//! - **What players do** comes back as records (`boundary`): a kill or a taking removes that
//!   member from its class (the dead set keeps it from coming back, A10); a shearing makes it
//!   notable until the wool regrows; an obstacle raises the cost of entering the cell the group
//!   could not, and the herd replans at once.
//! - **Predators** (`prey_near`, `predate`): dragons weigh herds near their lair and take latent
//!   animals by cohort weight; a predator that ate from a herd follows it farther.
//!
//! # Cost
//!
//! O(events): a decision per herd per sub-season (bounded flow field), the events players cause,
//! and a check per embodied group. Nothing grows with the members of a herd, the size of the map
//! or the time since anything was read.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_archive::{At, Tape, kinds};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::{self, Decode, Encode, Reader, Writer};
use rtsim_kernel::meta_pages::{PagedMap, PagedWheel};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::rows::{PagedArena, PagedRows, cell_key};
use rtsim_settle::Pos;
use rtsim_space::topology::{CONE_CLEARANCE, Iso, STOP_MARGIN, SharedTopologies, Topology};
use rtsim_travel::legs::{LegIndex, Meeting, Owner, Path, REGION_BLOCKS, Stamp};
use serde::Serialize;

use super::boundary::{
    self, Capture, Cmd, KillerKind, MAT_SURFACE, MEMBER_SHEARED, MemberSpawn, Obs, Uuid,
};
use super::climate::{Climate, Subs, Uniform};
use super::cohort::Class;
use super::forage::{CELL_BLOCKS, CellKey, Forage, cell_of};
use super::herd::{
    Embodied, Herd, HerdId, HerdLedger, NO_PLAYER, Route, agent_of, herd_of, route_regions,
};
use super::members::{self, member};
use super::outbox::{GateCounts, Outbox};
use super::route::{self, Ask, Obstacles};
use super::species::{Cohort, HerdSpecies, HerdSpeciesData};
use super::tunables::HerdTunables;

pub const SYS_HERD: SystemId = SystemId::named("herds.herd");
const SYS_BIRTH: SystemId = SystemId::named("herds.birth");
const SYS_STARVE: SystemId = SystemId::named("herds.starve");
const SYS_PLACE: SystemId = SystemId::named("herds.place");
const SYS_STAGGER: SystemId = SystemId::named("herds.stagger");
const SYS_DISPERSE: SystemId = SystemId::named("herds.disperse");
const SYS_FRONTIER: SystemId = SystemId::named("herds.frontier");

/// Leg owners (opaque to the index): bit 60 marks a herd module leg, apart from a player's
/// interest area (bit 63, rtsim-world and the dragons), a dragon (62) and a ship (61). Under it,
/// bit 59 marks a player's herd interest area (by player slot); otherwise the low 48 bits are the
/// herd's handle.
pub const HERD_OWNER: u64 = 1 << 60;
const PLAYER_TAG: u64 = 1 << 59;
const OWNER_MASK: u64 = 0xF8 << 56;

pub fn herd_owner(h: HerdId) -> Owner {
    Owner(HERD_OWNER | h.to_bits())
}

fn player_owner(slot: u32) -> Owner {
    Owner(HERD_OWNER | PLAYER_TAG | u64::from(slot))
}

pub fn owner_herd(o: Owner) -> Option<HerdId> {
    (o.0 & OWNER_MASK == HERD_OWNER).then(|| HerdId::from_bits(o.0 & ((1 << 48) - 1)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    /// The herd's decision for a sub-season; stale unless it is the herd's `next_decide`.
    Decide { h: HerdId },
    /// The footprint reaches into a player's interest area.
    Seen {
        h: HerdId,
        player: u32,
        meeting: Meeting,
    },
    /// An embodied group is looked at.
    Check { h: HerdId, epoch: u32 },
    /// A year begins: the forage sweep.
    Year,
}

/// Counters for tests, the bench and the CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HerdStats {
    pub events: u64,
    pub stale: u64,
    pub decisions: u64,
    pub cells_reached: u64,
    pub breedings: u64,
    pub starvations: u64,
    pub clips: u64,
    pub seen: u64,
    pub seen_stale: u64,
    pub seen_empty: u64,
    pub materialized: u64,
    pub despawns: u64,
    pub captures: u64,
    pub rejected: u64,
    pub steers: u64,
    pub killed: u64,
    pub sheared: u64,
    pub taken: u64,
    pub obstacles: u64,
    pub replans: u64,
    pub predations: u64,
    pub duplicate_ops: u64,
    pub late_records: u64,
    /// Ranges whose last animal died or left (each then waits for its frontier founding).
    pub vacated: u64,
    /// Empty ranges founded again by newcomers from beyond (EQUILIBRIUM.md §2.4.2).
    pub frontier_foundings: u64,
    /// Frontier foundings put off a sub-season because a player stood near.
    pub frontier_postponed: u64,
    /// Springs in which a herd sent some of its adults to thin neighbours, and the animals sent.
    pub dispersals: u64,
    pub dispersed: u64,
    /// Empty ranges founded again by dispersers from a neighbour.
    pub recolonised: u64,
}

/// One line of what happened, for the CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HerdLog {
    pub tick: u64,
    pub day: f64,
    pub kind: &'static str,
    pub herd: Option<String>,
    pub what: String,
}

/// One decision, for the CLI's route traces.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Decision {
    pub tick: u64,
    pub day: f64,
    pub herd: String,
    pub sub_season: u32,
    pub count: u64,
    pub cohorts: [u64; 3],
    pub condition: [u16; 3],
    pub satisfaction: u16,
    pub from: (i64, i64),
    pub target: (i32, i32),
    pub route: Vec<(i32, i32)>,
    pub temperature: f32,
    pub forage: f64,
    pub cells_reached: u32,
}

/// One predation, for the CLI (kept only while the log is on).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PreyRecord {
    pub tick: u64,
    pub day: f64,
    pub herd: String,
    pub predator: u64,
    pub animals: u64,
    /// The herd's centre when it was taken from.
    pub herd_pos: (i64, i64),
    /// Where the predator hunted.
    pub at: (i64, i64),
}

/// A player as the herds know it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerView {
    pub uuid: Uuid,
    /// The player's actor in the phase 1 world's knowledge (its bits), when the shared interest
    /// area introduced it (`Viewer::area_booked`); 0 otherwise.
    pub actor: u64,
    pub pos: Option<(Pos, Tick)>,
    /// The player's herd interest area in the leg index.
    pub interest: Option<Stamp>,
    /// Herd puppets embodied for this player.
    pub puppets: u32,
}

/// A new herd: a scenario, a plugin's census of a found herd, or the bench.
#[derive(Clone, Debug, PartialEq)]
pub struct NewHerd {
    pub name: String,
    pub species: HerdSpecies,
    pub frame: u16,
    pub pos: (i32, i32),
    pub count: u64,
    /// `None`: derived from the world seed and a counter.
    pub seed: Option<u64>,
}

/// A herd a predator could hunt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HerdOffer {
    pub herd: HerdId,
    pub species: HerdSpecies,
    pub pos: (i32, i32),
    pub radius: u32,
    /// Animals a predator could take: the latent ones (embodied animals are the plugin's).
    pub available: u64,
    /// Fullness per animal (Ice and Fire's food points of the species' box).
    pub food_each: i32,
    /// The predator follows this herd (it ate from it recently).
    pub followed: bool,
}

/// What a predator took.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Predation {
    pub species: Option<HerdSpecies>,
    pub animals: u64,
    pub food: i32,
}

pub struct Herds {
    pub tun: HerdTunables,
    pub climate: Box<dyn Climate>,
    pub seed: u64,
    /// Every herd's row, in pages (docs/choices-paging-modules.md).
    pub herds: PagedArena<Herd>,
    pub forage: Forage,
    pub obstacles: Obstacles,
    pub(crate) wheel: PagedWheel<Ev>,
    /// Herds by the regions their current route covers (with the footprint), keyed by
    /// [`region_key`], in pages. Each herd's row holds its own list ([`Herd::regions`]), so the
    /// index is never recomputed from every herd but after a topology change.
    pub(crate) regions: PagedRows<RegionHerds>,
    /// Herds that may hold legs for players or an embodied group: a superset, pruned when a player
    /// leaves or a session starts, so neither walks every herd.
    pub(crate) active: BTreeSet<HerdId>,
    pub(crate) players: Vec<PlayerView>,
    pub(crate) player_by_key: BTreeMap<u64, u32>,
    /// Cells herds settled on in sub-season `claims_sub`.
    pub(crate) claims: PagedMap<CellKey, u64>,
    pub(crate) claims_sub: i64,
    /// The commands to the plugin, behind the replay gate (docs/replay-gate.md §4.6): the only way
    /// to queue one is `Outbox::push`.
    pub(crate) outbox: Outbox,
    /// The epoch floors (docs/replay-gate.md §5.3, RP-7, RP-30, RP-57, RP-81): agent to the
    /// highest epoch a replayed record or a copy named for it that its herd's own `epoch` does
    /// not hold. A herd's next group takes an epoch above both. Folded into the herds' epochs at
    /// the replay end; in memory only.
    pub(crate) floors: BTreeMap<u64, u32>,
    /// Whether the session's replay runs on an engine that came back from its pages (RP-31):
    /// only then is a replayed HERD_MATERIALIZED the last session's answer to this engine's
    /// decision (docs/replay-gate.md §5.2). Set by [`Herds::begin_replay`]; in memory only.
    pub(crate) replay_from_pages: bool,
    /// Groups the replay honoured since it began (the replay end's log line); in memory only.
    pub(crate) replay_honoured: u64,
    /// One past the highest herd handle index at the session's replay begin (RP-84): the handles
    /// the last session's records can name. A herd founded later takes an index at or above it,
    /// and may be another herd than the one the last session had under that index (the world
    /// model founds in the order the world is explored), so neither a replayed
    /// HERD_MATERIALIZED nor a copy is applied to one. `u32::MAX` before any replay begins
    /// (every handle counts). In memory only.
    pub(crate) replay_bound: u32,
    /// Predator → herds it follows.
    pub(crate) follows: BTreeMap<u64, BTreeSet<HerdId>>,
    /// Plugin op ids applied, and when (forgotten after a while at the yearly sweep).
    pub(crate) ops: BTreeMap<u64, Tick>,
    pub stats: HerdStats,
    pub log_enabled: bool,
    pub log: Vec<HerdLog>,
    pub trace_enabled: bool,
    pub trace: Vec<Decision>,
    pub predations: Vec<PreyRecord>,
    /// The event archive's recorder (VIEWER.md §10.1): off until the host turns it on; not state.
    pub tape: Tape,
    pub(crate) next_seed: u64,
    pub(crate) now: Tick,
    /// Every frame's topology (TP6): flat until the engine host hands over the TOPOLOGY records'.
    pub topo: SharedTopologies,
}

fn day_of(t: Tick) -> i64 {
    (t.0 / TICKS_PER_DAY) as i64
}

fn dayf(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// The shortest distance through every seam; on the plane the old straight line, bit for bit.
fn dist(t: &Topology, a: (i64, i64), b: (i64, i64)) -> f64 {
    if t.is_flat() {
        return (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt();
    }
    t.dist(a, b)
}

/// Area of the intersection of two discs of radii `r1`, `r2` whose centres are `d` apart.
pub fn lens_area(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        let r = r1.min(r2);
        return std::f64::consts::PI * r * r;
    }
    let a = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1))
        .clamp(-1.0, 1.0)
        .acos();
    let b = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2))
        .clamp(-1.0, 1.0)
        .acos();
    let k = ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2)).max(0.0);
    r1 * r1 * a + r2 * r2 * b - 0.5 * k.sqrt()
}

/// The same 64-bit stand-in for a player's UUID the phase 1 world and the dragons use.
pub fn player_key(uuid: Uuid) -> u64 {
    crate::engine::uuid_hash(uuid.0, uuid.1)
}

/// The page table of the herd rows (docs/choices-paging-modules.md, "Page tables").
pub const TABLE_HERDS: u16 = 0x4810;
/// The page table of the region index.
pub const TABLE_REGIONS: u16 = 0x4812;
/// The page table of the wheel's log pages (docs/choices-paging-followups.md PF8).
pub const TABLE_WHEEL: u16 = 0x4814;
/// The page table of the claims map's pages (PF8).
pub const TABLE_CLAIMS: u16 = 0x4815;
/// Herds per page: a row is 0.5 to 2 KB resident, a few hundred bytes on disk.
pub const HERDS_PER_PAGE: u64 = 32;
/// Regions per page: 4 x 4 regions of [`REGION_BLOCKS`] (an aligned square, 2,048 blocks a side).
pub const REGIONS_PER_PAGE: u64 = 16;

/// The region index's row key of a region (frame, region x, region z): neighbours share pages.
pub fn region_key(r: (u16, i64, i64)) -> u64 {
    cell_key(r.0, r.1, r.2)
}

/// One region's row of the region index: the herds whose routes cover it, and the herds whose
/// range it is (EQUILIBRIUM.md §2.4.2: one in a world-model world; a synthetic world may found
/// two in one region), each by handle index, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionHerds {
    pub routes: Vec<u32>,
    pub ranges: Vec<u32>,
}

impl RegionHerds {
    fn is_empty(&self) -> bool {
        self.routes.is_empty() && self.ranges.is_empty()
    }
}

fn put_indices(w: &mut Writer, v: &[u32]) {
    w.var_u64(v.len() as u64);
    let mut last = 0u32;
    for &i in v {
        w.var_u64(u64::from(i - last));
        last = i;
    }
}

fn get_indices(r: &mut Reader<'_>) -> codec::Result<Vec<u32>> {
    let n = r.len_prefix(1)?;
    let mut out = Vec::with_capacity(n);
    let mut last = 0u64;
    for k in 0..n {
        let d = r.var_u64()?;
        if k > 0 && d == 0 {
            return Err(codec::CodecError::Invalid(
                "region herds out of order".into(),
            ));
        }
        last += d;
        out.push(
            u32::try_from(last)
                .map_err(|_| codec::CodecError::Invalid("herd index over u32".into()))?,
        );
    }
    Ok(out)
}

/// Inserts `i` into a sorted list (once).
fn add_sorted(v: &mut Vec<u32>, i: u32) {
    if let Err(p) = v.binary_search(&i) {
        v.insert(p, i);
    }
}

impl Encode for RegionHerds {
    fn encode(&self, w: &mut Writer) {
        put_indices(w, &self.routes);
        put_indices(w, &self.ranges);
    }
}

impl Decode for RegionHerds {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(RegionHerds {
            routes: get_indices(r)?,
            ranges: get_indices(r)?,
        })
    }
}

impl rtsim_kernel::rows::Row for RegionHerds {
    fn heap_bytes(&self) -> usize {
        (self.routes.capacity() + self.ranges.capacity()) * 4
    }
}

/// The range (frame, region x, region z) of a canonical block position.
pub fn range_of(frame: u16, pos: (i32, i32)) -> (u16, i32, i32) {
    let r = REGION_BLOCKS as i32;
    (frame, pos.0.div_euclid(r), pos.1.div_euclid(r))
}

/// A range's row key in the region index.
fn range_key(r: (u16, i32, i32)) -> u64 {
    region_key((r.0, i64::from(r.1), i64::from(r.2)))
}

/// A herd's archive id: its handle's plain bits, without the leg index's owner namespace.
fn hid(h: HerdId) -> u128 {
    u128::from(h.to_bits())
}

fn herd_at(i: u32) -> HerdId {
    PagedArena::<Herd>::handle(i)
}

impl Herds {
    pub fn new(seed: u64, tun: HerdTunables, climate: Box<dyn Climate>, now: Tick) -> Self {
        let mut h = Herds {
            tun,
            climate,
            seed,
            herds: PagedArena::new(TABLE_HERDS, HERDS_PER_PAGE),
            forage: Forage::new(),
            obstacles: Obstacles::default(),
            wheel: PagedWheel::new(now),
            regions: PagedRows::new(TABLE_REGIONS, REGIONS_PER_PAGE),
            active: BTreeSet::new(),
            players: Vec::new(),
            player_by_key: BTreeMap::new(),
            claims: PagedMap::new(),
            claims_sub: i64::MIN,
            outbox: Outbox::default(),
            floors: BTreeMap::new(),
            replay_from_pages: false,
            replay_honoured: 0,
            replay_bound: u32::MAX,
            follows: BTreeMap::new(),
            ops: BTreeMap::new(),
            stats: HerdStats::default(),
            log_enabled: false,
            log: Vec::new(),
            trace_enabled: false,
            trace: Vec::new(),
            predations: Vec::new(),
            tape: Tape::new(),
            next_seed: 0,
            now,
            topo: rtsim_space::topology::all_flat(),
        };
        let subs = h.subs();
        let n = subs.index(now);
        let next_year = subs.start((Subs::year(n) + 1) * 12);
        h.wheel.schedule(next_year, Ev::Year);
        h
    }

    /// Hands the engine every frame's topology (TP6). The region index is rebuilt.
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) {
            return;
        }
        if *t != *self.topo {
            self.topo = t;
            self.rebuild_regions();
        }
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.topo.of(frame)
    }

    /// A herd's canonical position at `t`: its route's chart position folded (TP7).
    fn at_pos(&self, x: &Herd, t: Tick) -> (i64, i64) {
        let p = x.route.pos(t);
        self.topo(x.frame).canonical(p.0, p.1)
    }

    /// The canonical regions of a square of half-side `r` around a block, across seams.
    fn regions_near(&self, frame: u16, x: i64, z: i64, r: i64) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        for part in self.topo(frame).rects_near(x, z, r) {
            let (x0, z0, x1, z1) = part.grid_cells(REGION_BLOCKS);
            for rx in x0..=x1 {
                for rz in z0..=z1 {
                    out.push((rx, rz));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Where the bodies of an embodied herd are sent: its next waypoint in their frame (TP11).
    /// `None` when the walk there crosses an edge_stop seam within reach of the herd now, so it
    /// is dematerialized first (TP12); short of that the waypoint is pulled back to the border.
    fn body_target(&self, x: &Herd, chart: Iso, at: Tick) -> Option<(i32, i32)> {
        let w = x.route.heading(at);
        let t = self.topo(x.frame);
        if t.any_stopped() {
            let p = x.route.pos(at);
            let f = t.fold(p.0, p.1);
            let wc = f.iso.cell(i64::from(w.0), i64::from(w.1));
            if let Some(cut) = t.stopped_crossing((f.x, f.z), wc) {
                let (dx, dz) = (wc.0 - f.x, wc.1 - f.z);
                let left = cut * ((dx * dx + dz * dz) as f64).sqrt();
                if left <= f64::from(self.tun.hysteresis) + 4.0 * STOP_MARGIN as f64 {
                    return None;
                }
                let stop = (
                    f.x + (dx as f64 * cut) as i64,
                    f.z + (dz as f64 * cut) as i64,
                );
                let back = f.iso.inverse().cell(stop.0, stop.1);
                let b = chart.cell(back.0, back.1);
                let (bx, bz) = t.off_stopped(b.0, b.1);
                return Some((bx as i32, bz as i32));
            }
        }
        let b = chart.cell(i64::from(w.0), i64::from(w.1));
        Some((b.0 as i32, b.1 as i32))
    }

    /// An engine with no herds and a uniform climate: a placeholder to swap with.
    pub fn empty() -> Self {
        Herds::new(0, HerdTunables::default(), Box::new(Uniform), Tick::ZERO)
    }

    pub fn subs(&self) -> Subs {
        Subs::of(&self.tun.calendar)
    }

    pub fn year_days(&self) -> u32 {
        self.subs().year_days()
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    pub fn next_due(&self) -> Option<Tick> {
        self.wheel.next_due()
    }

    pub fn pending_events(&self) -> usize {
        self.wheel.len()
    }

    pub fn take_commands(&mut self) -> Vec<Cmd> {
        self.outbox.take()
    }

    /// Whether the session's replay runs (between [`Herds::begin_replay`] and
    /// [`Herds::replay_done`]).
    pub fn replaying(&self) -> bool {
        self.outbox.replaying()
    }

    /// The replay gate's counts (docs/replay-gate.md §4.3): what the gate dropped, what it saw
    /// of a kind that embodies, and the decisions that embody it returned from early. Since the
    /// engine was built or loaded.
    pub fn replay_gate_counts(&self) -> GateCounts {
        self.outbox.counts()
    }

    /// Whether the session's replay honours HERD_MATERIALIZED (RP-31): the engine came back from
    /// its pages.
    pub fn replay_from_pages(&self) -> bool {
        self.replay_from_pages
    }

    /// Groups the session's replay honoured (§5.2).
    pub fn replay_honoured(&self) -> u64 {
        self.replay_honoured
    }

    pub fn herd(&self, h: HerdId) -> Option<&Herd> {
        self.herds.get(h)
    }

    pub fn by_name(&self, name: &str) -> Option<HerdId> {
        self.herds
            .iter()
            .find(|(_, x)| x.name == name)
            .map(|(h, _)| h)
    }

    pub fn sp(&self, h: HerdId) -> &HerdSpeciesData {
        self.tun.species(self.herds[h].species)
    }

    /// Animals alive in a herd at `t` (a pure read: dyadic counts, O(classes × log)).
    pub fn count(&self, h: HerdId, t: Tick) -> u64 {
        let x = &self.herds[h];
        x.alive(self.tun.species(x.species), self.year_days(), day_of(t))
    }

    pub fn cohorts(&self, h: HerdId, t: Tick) -> [u64; 3] {
        let x = &self.herds[h];
        x.cohorts(self.tun.species(x.species), self.year_days(), day_of(t))
    }

    /// Where a herd's centre is at `t` (closed form), canonical.
    pub fn position(&self, h: HerdId, t: Tick) -> (i64, i64) {
        self.at_pos(&self.herds[h], t)
    }

    pub fn footprint(&self, h: HerdId, t: Tick) -> u32 {
        let c = self.count(h, t);
        self.sp(h).footprint_radius(c)
    }

    /// Every animal alive, per species, at `t`.
    pub fn totals(&self, t: Tick) -> [u64; HerdSpecies::COUNT] {
        let mut out = [0; HerdSpecies::COUNT];
        for (h, x) in self.herds.iter() {
            out[x.species.index()] += self.count(h, t);
        }
        out
    }

    /// The census (EQUILIBRIUM.md S1) now: `herd.range`, one site per herd row, K the range's,
    /// its members the animals alive (an empty range is an empty site); renewals are the yearly
    /// calvings, the frontier foundings and the ranges dispersers founded again.
    pub fn census(&self) -> Vec<rtsim_kernel::census::KindCensus> {
        let mut c = rtsim_kernel::census::KindCensus::new("herd.range");
        for (h, x) in self.herds.iter() {
            c.site(self.count(h, self.now), x.k, false, 0.0);
        }
        c.renewals = self.stats.breedings + self.stats.frontier_foundings + self.stats.recolonised;
        vec![c]
    }

    /// The refuge of a range of K `k` (EQUILIBRIUM.md §3.4): predators never take a herd below
    /// it.
    pub fn refuge(&self, k: u64) -> u64 {
        ((self.tun.refuge_share * k as f64).round() as u64).max(self.tun.refuge_min)
    }

    /// Every herd's ledger, summed per species.
    pub fn ledger(&self) -> [HerdLedger; HerdSpecies::COUNT] {
        let mut out = [HerdLedger::default(); HerdSpecies::COUNT];
        for (_, x) in self.herds.iter() {
            out[x.species.index()].add(&x.ledger);
        }
        out
    }

    fn note(&mut self, at: Tick, kind: &'static str, h: Option<HerdId>, what: String) {
        if !self.log_enabled {
            return;
        }
        let herd = h.and_then(|h| self.herds.get(h)).map(|x| x.name.clone());
        self.log.push(HerdLog {
            tick: at.0,
            day: dayf(at),
            kind,
            herd,
            what,
        });
    }

    fn schedule(&mut self, at: Tick, ev: Ev) {
        self.wheel.schedule(at.max(self.now), ev);
    }

    fn fresh_seed(&mut self) -> u64 {
        self.next_seed += 1;
        StreamKey::new(self.seed, SYS_HERD, self.next_seed).block(0, 0)[0]
    }

    /// When in a sub-season the herd decides: the herds of one row page decide on consecutive
    /// ticks, at a time hashed from the page, so a sub-season loads and dirties each page once
    /// (not once per herd on it), and decisions stay spread over `stagger_ticks`.
    fn stagger(&self, h: HerdId) -> u64 {
        let n = self.tun.stagger_ticks.max(1);
        let i = u64::from(h.index());
        let page = i / HERDS_PER_PAGE;
        let base = StreamKey::new(self.seed, SYS_STAGGER, page).block(0, 0)[0] % n;
        (base + i % HERDS_PER_PAGE) % n
    }

    fn decide_at(&self, h: HerdId, at: Tick) -> Tick {
        let subs = self.subs();
        let n = subs.index(at);
        let s = self.stagger(h);
        let this = subs.start(n) + s;
        if this > at {
            this
        } else {
            subs.start(n + 1) + s
        }
    }

    // ---- herds ---------------------------------------------------------------------------------

    /// Adds a herd at `at` with a stable age pyramid (founders), and books its first decision at
    /// once. Its count is its range's K (EQUILIBRIUM.md §2.4.2), and its range is the region of
    /// `pos`.
    pub fn add_herd(&mut self, n: NewHerd, at: Tick) -> HerdId {
        let seed = match n.seed {
            Some(s) => s,
            None => self.fresh_seed(),
        };
        let sp = self.tun.species(n.species).clone();
        let yd = i64::from(self.year_days());
        let day = day_of(at);
        let subs = self.subs();
        let in_year = Subs::in_year(subs.index(at));
        // Days since this year's birth season began: founders' ages are whole years plus this.
        let born_sub = u32::from(sp.breeding_sub_seasons.first().copied().unwrap_or(0));
        let since_sub = (in_year + 12 - born_sub) % 12;
        let phase = (i64::from(since_sub) * subs.len as i64
            + (at.0 as i64 - subs.start(subs.index(at)).0 as i64))
            / TICKS_PER_DAY as i64;
        // Stable pyramid: survival to each year of age, at the species' hazards.
        let mut weights = Vec::new();
        let mut s = 1.0f64;
        let mut age = 0i64;
        while age < i64::from(sp.lifespan_days) {
            let a = age + phase;
            if a >= i64::from(sp.lifespan_days) {
                break;
            }
            weights.push((a, s));
            let c = sp.cohort_at(a).unwrap_or(Cohort::Old);
            s *= 1.0 - sp.annual_mortality[c.index()];
            age += yd;
        }
        let total: f64 = weights.iter().map(|w| w.1).sum();
        let mut counts: Vec<u64> = weights
            .iter()
            .map(|w| (n.count as f64 * w.1 / total).floor() as u64)
            .collect();
        let mut left = n.count - counts.iter().sum::<u64>();
        let mut k = 0;
        let len = counts.len();
        while left > 0 && len > 0 {
            counts[k % len] += 1;
            left -= 1;
            k += 1;
        }
        // Oldest first, so member indices rise with birth.
        let mut classes = Vec::new();
        let mut next = 0u64;
        for (i, (a, _)) in weights.iter().enumerate().rev() {
            let c = counts[i];
            if c == 0 {
                continue;
            }
            classes.push(Class::new(next, c as u32, day - a, day, c as u32));
            next += c;
        }
        let home = self.topo(n.frame).canonical_i32(n.pos.0, n.pos.1);
        let herd = Herd {
            name: n.name,
            species: n.species,
            seed,
            frame: n.frame,
            k: n.count,
            range: range_of(n.frame, home),
            home,
            vacant: None,
            classes,
            next_index: next,
            condition: [800; 3],
            satisfaction: 1000,
            route: Route::stay(home, at, at),
            reach: 0,
            plan: 0,
            bred_year: Subs::year(subs.index(at)),
            decided: at,
            next_decide: Tick::ZERO,
            seen: BTreeMap::new(),
            dead: BTreeSet::new(),
            embodied: None,
            epoch: 0,
            followers: Vec::new(),
            ledger: HerdLedger {
                founded: n.count,
                ..HerdLedger::default()
            },
            clips: BTreeMap::new(),
            regions: Vec::new(),
        };
        let id = self.herds.insert(herd);
        self.index_range(id);
        self.herds[id].next_decide = at;
        self.schedule(at, Ev::Decide { h: id });
        let start = self.herds[id].route.start();
        self.tape.event(
            kinds::HERDS_FOUNDED,
            At::new(at.0, u32::from(n.frame), start.0, start.1),
            hid(id),
            0,
            n.count as i64,
        );
        self.note(
            at,
            "herd",
            Some(id),
            format!("{} {} founded at {:?}", n.count, n.species.name(), n.pos),
        );
        id
    }

    // ---- the step loop -------------------------------------------------------------------------

    /// Runs every herd event up to and including `to`, in time order. Nothing is ticked.
    pub fn run_until(&mut self, legs: &mut LegIndex, to: Tick) {
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(legs, at, ev);
        }
        self.now = self.now.max(to);
        self.enforce_budget();
    }

    fn handle(&mut self, legs: &mut LegIndex, at: Tick, ev: Ev) {
        match ev {
            Ev::Decide { h } => match self.herds.get(h) {
                Some(x) if x.next_decide == at && x.vacant.is_some() => self.frontier(legs, h, at),
                Some(x) if x.next_decide == at => self.decide(legs, h, at),
                _ => self.stats.stale += 1,
            },
            Ev::Seen { h, player, meeting } => self.seen(legs, h, player, meeting, at),
            Ev::Check { h, epoch } => self.check_group(legs, h, epoch, at),
            Ev::Year => {
                let climate = &*self.climate;
                let subs = Subs::of(&self.tun.calendar);
                self.forage.sweep(climate, &subs, at);
                let keep = Tick(at.0.saturating_sub(subs.year_ticks()));
                self.ops.retain(|_, t| *t >= keep);
                let n = subs.index(at);
                let next = subs.start((Subs::year(n) + 1) * 12);
                self.schedule(next.max(at + 1), Ev::Year);
            }
        }
    }

    fn decide(&mut self, legs: &mut LegIndex, h: HerdId, at: Tick) {
        self.stats.decisions += 1;
        let subs = self.subs();
        let yd = self.year_days();
        let day = day_of(at);
        let n = subs.index(at);
        self.graze(h, at);
        let sp = self.sp(h).clone();
        let (in_year, year) = (Subs::in_year(n), Subs::year(n));
        if sp.breeds_in(in_year) && self.herds[h].bred_year != year {
            self.breed(h, at, year);
            self.disperse(h, at, year);
        }
        // Classes with nobody left go (their deaths committed first), except one holding a
        // member embodied right now: it stays until the capture.
        {
            let x = self.herds.get_mut(h).expect("live");
            let mut died = 0u64;
            let watched: Vec<u64> = x
                .embodied
                .as_ref()
                .map(|e| e.members.clone())
                .unwrap_or_default();
            x.classes.retain_mut(|c| {
                if c.alive_on(&sp, yd, x.seed, day) == 0 && !watched.iter().any(|k| c.contains(*k))
                {
                    died += u64::from(c.reanchor(&sp, yd, x.seed, day));
                    false
                } else {
                    true
                }
            });
            x.ledger.died += died;
            members::prune(x);
        }
        let alive = self.count(h, at);
        if alive == 0 {
            let end = self.herds[h].route.end();
            let end = self.topo(self.herds[h].frame).canonical_i32(end.0, end.1);
            let x = self.herds.get_mut(h).expect("live");
            x.route = Route::stay(end, at, at);
            x.vacant = Some(at);
            self.stats.vacated += 1;
            self.set_regions(h, Vec::new());
            self.book_frontier(h, at);
            let frame = self.herds[h].frame;
            self.tape
                .event(
                    kinds::HERDS_EXTINCT,
                    At::new(at.0, u32::from(frame), end.0, end.1),
                    hid(h),
                    0,
                    0,
                )
                .text(|| format!("the last of {} died", self.herds[h].name));
            self.note(at, "extinct", Some(h), "the last animal died".into());
            return;
        }
        // The next decision, then the route to it.
        let next = self.decide_at(h, at + 1);
        self.herds.get_mut(h).expect("live").next_decide = next;
        self.plan(legs, h, at, next);
        self.schedule(next, Ev::Decide { h });
    }

    /// Plans the route from where the herd is at `at` until `until`, then refreshes its regions,
    /// its claim, the embodied group's steering and the players' meetings.
    fn plan(&mut self, legs: &mut LegIndex, h: HerdId, at: Tick, until: Tick) {
        let subs = self.subs();
        let n = subs.index(at);
        if self.claims_sub != n {
            self.claims.clear();
            self.claims_sub = n;
        }
        let count = self.count(h, at);
        let x = &self.herds[h];
        let sp = self.tun.species(x.species);
        // The new route starts at the canonical position (TP7): its chart is the canonical frame
        // there, so an embodied group's frame is carried over to it.
        let chart_now = x.route.pos(at);
        let folded = self.topo(x.frame).fold(chart_now.0, chart_now.1);
        let from = (folded.x, folded.z);
        let topo = self.topo.clone();
        let choice = route::plan(&Ask {
            topo: topo.of(x.frame),
            climate: &*self.climate,
            subs: &subs,
            forage: &self.forage,
            obstacles: &self.obstacles,
            claims: &self.claims,
            me: h.to_bits(),
            sp,
            frame: x.frame,
            from,
            count,
            at,
            until,
            w: self.tun.weights(),
        });
        self.stats.cells_reached += u64::from(choice.cells_reached);
        let frame = x.frame;
        let tc = topo
            .of(frame)
            .grid(i64::from(CELL_BLOCKS))
            .of_block(i64::from(choice.target.0), i64::from(choice.target.1));
        self.claims
            .insert((frame, tc.0 as i32, tc.1 as i32), h.to_bits());
        if self.trace_enabled {
            let x = &self.herds[h];
            let sp = self.tun.species(x.species);
            self.trace.push(Decision {
                tick: at.0,
                day: dayf(at),
                herd: x.name.clone(),
                sub_season: Subs::in_year(n),
                count,
                cohorts: x.cohorts(sp, self.year_days(), day_of(at)),
                condition: x.condition,
                satisfaction: x.satisfaction,
                from,
                target: choice.target,
                route: choice.route.points.clone(),
                temperature: choice.temperature,
                forage: choice.forage,
                cells_reached: choice.cells_reached,
            });
        }
        let radius = sp.footprint_radius(count);
        {
            let x = self.herds.get_mut(h).expect("live");
            x.route = choice.route;
            x.reach = radius;
            x.plan = x.plan.wrapping_add(1);
            if let Some(e) = x.embodied.as_mut() {
                e.chart = e.chart.compose(folded.iso.inverse());
            }
        }
        // One leg per straight piece of the route just decided.
        let route = &self.herds[h].route;
        for (p, t) in route.points.windows(2).zip(route.times.windows(2)) {
            self.tape.leg(
                kinds::HERDS_MIGRATION,
                u32::from(frame),
                (t[0].0, p[0].0, p[0].1),
                (t[1].0, p[1].0, p[1].1),
                hid(h),
            );
        }
        let regions = route_regions(&self.herds[h].route, frame, radius, topo.of(frame));
        self.set_regions(h, regions);
        self.steer(h, at);
        self.meet_players(legs, h, at);
    }

    /// Indexes the herd under `list` (its route's regions), in place of its old list. An
    /// unchanged list touches no page.
    fn set_regions(&mut self, h: HerdId, list: Vec<(u16, i64, i64)>) {
        let x = self.herds.get(h).expect("live");
        if x.regions == list {
            return;
        }
        let i = h.index();
        let old = std::mem::take(&mut self.herds.get_mut(h).expect("live").regions);
        for r in old {
            let k = region_key(r);
            let empty = match self.regions.get_mut(k) {
                Some(s) => {
                    if let Ok(p) = s.routes.binary_search(&i) {
                        s.routes.remove(p);
                    }
                    s.is_empty()
                }
                None => false,
            };
            if empty {
                self.regions.remove(k);
            }
        }
        for r in &list {
            let k = region_key(*r);
            match self.regions.get_mut(k) {
                Some(s) => add_sorted(&mut s.routes, i),
                None => {
                    self.regions.insert(
                        k,
                        RegionHerds {
                            routes: vec![i],
                            ranges: Vec::new(),
                        },
                    );
                }
            }
        }
        self.herds.get_mut(h).expect("live").regions = list;
    }

    /// Indexes the herd under its range (once, at its founding: a range keeps its row).
    fn index_range(&mut self, h: HerdId) {
        let k = range_key(self.herds[h].range);
        let i = h.index();
        match self.regions.get_mut(k) {
            Some(s) => add_sorted(&mut s.ranges, i),
            None => {
                self.regions.insert(
                    k,
                    RegionHerds {
                        routes: Vec::new(),
                        ranges: vec![i],
                    },
                );
            }
        }
    }

    /// The herd rows of range (frame, region x, region z), alive or empty: the world model's
    /// guard against founding a range twice (EQUILIBRIUM.md §2.4.2), and dispersal's neighbours.
    pub fn range_herds(&self, range: (u16, i32, i32)) -> Vec<HerdId> {
        self.regions
            .get(range_key(range))
            .map(|s| s.ranges.iter().map(|&i| herd_at(i)).collect())
            .unwrap_or_default()
    }

    /// Recomputes every herd's regions from its route (a new topology): a whole-table pass.
    pub(crate) fn rebuild_regions(&mut self) {
        self.regions.clear();
        for h in self.herds.handles() {
            self.index_range(h);
            let x = &self.herds[h];
            // Empty, or not planned yet (a route is indexed when it is planned).
            let list = if x.vacant.is_some() || x.reach == 0 {
                Vec::new()
            } else {
                route_regions(&x.route, x.frame, x.reach, self.topo.of(x.frame))
            };
            if !self.herds[h].regions.is_empty() {
                self.herds.get_mut(h).expect("live").regions = Vec::new();
            }
            self.set_regions(h, list);
            self.enforce_budget();
        }
    }

    /// The region index from the herds' own lists (a whole-table pass: a blob load).
    pub(crate) fn index_regions(&mut self) {
        self.regions.clear();
        // (region key, 0 route or 1 range, herd index)
        let mut pairs: Vec<(u64, u8, u32)> = Vec::new();
        self.herds.rows().for_each(|k, x| {
            for r in &x.regions {
                pairs.push((region_key(*r), 0, k as u32));
            }
            pairs.push((range_key(x.range), 1, k as u32));
        });
        pairs.sort_unstable();
        pairs.dedup();
        for chunk in pairs.chunk_by(|a, b| a.0 == b.0) {
            let pick = |t: u8| chunk.iter().filter(|p| p.1 == t).map(|p| p.2).collect();
            self.regions.insert(
                chunk[0].0,
                RegionHerds {
                    routes: pick(0),
                    ranges: pick(1),
                },
            );
        }
    }

    /// Forgets the herds that hold no leg and no group any more.
    fn prune_active(&mut self) {
        let herds = &self.herds;
        self.active.retain(|h| {
            herds
                .get(*h)
                .is_some_and(|x| !x.clips.is_empty() || x.embodied.is_some())
        });
    }

    /// Charges the herd for its grazing since its last decision, at the place it stayed: forage
    /// falls in its footprint, condition moves toward what was met, a hungry herd starves.
    fn graze(&mut self, h: HerdId, at: Tick) {
        let x = &self.herds[h];
        let days = at.0.saturating_sub(x.decided.0) as f64 / TICKS_PER_DAY as f64;
        if days <= 0.0 {
            return;
        }
        let subs = self.subs();
        let yd = self.year_days();
        let day = day_of(at);
        let sp = self.tun.species(x.species).clone();
        let count = x.alive(&sp, yd, day);
        let demand = count as f64 * sp.intake_per_day * days;
        let centre = self.at_pos(x, at);
        let frame = x.frame;
        let r = f64::from(sp.footprint_radius(count));
        let c0 = cell_of(centre.0, centre.1);
        let span = (r / f64::from(CELL_BLOCKS)).ceil() as i32;
        let t = self.topo(frame);
        let grid = t.grid(i64::from(CELL_BLOCKS));
        let mut cells = Vec::new();
        for dx in -span..=span {
            for dz in -span..=span {
                // The footprint in the chart of the herd's position, each cell grazed at its
                // canonical cell, once (TP14).
                let c = (c0.0 + dx, c0.1 + dz);
                let cc = super::forage::centre_of(c);
                let k = if t.is_flat() {
                    c
                } else {
                    let (kx, kz) = grid.canonical(i64::from(c.0), i64::from(c.1));
                    (kx as i32, kz as i32)
                };
                let d = (((i64::from(cc.0) - centre.0) as f64).powi(2)
                    + ((i64::from(cc.1) - centre.1) as f64).powi(2))
                .sqrt();
                if d <= r + f64::from(CELL_BLOCKS) / 2.0
                    && self.climate.terrain(frame, k.0, k.1).is_finite()
                    && !cells.contains(&(frame, k.0, k.1))
                {
                    cells.push((frame, k.0, k.1));
                }
            }
        }
        let climate = &*self.climate;
        let floor_of = |k: CellKey| Forage::best_capacity(climate, k) * super::forage::FLOOR_SHARE;
        let avail: Vec<f64> = cells
            .iter()
            .map(|&k| (self.forage.at(climate, &subs, k, at) - floor_of(k)).max(0.0))
            .collect();
        let total: f64 = avail.iter().sum();
        let take = demand.min(total * self.tun.graze_share);
        if total > 0.0 && take > 0.0 {
            for (k, a) in cells.iter().zip(&avail) {
                self.forage.graze(climate, &subs, *k, at, take * a / total);
            }
        }
        let sat = if demand > 0.0 {
            (take / demand).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let rate = self.tun.condition_rate;
        let starve_below = self.tun.starve_below;
        let n = subs.index(at);
        let world = self.seed;
        let x = self.herds.get_mut(h).expect("live");
        x.satisfaction = (sat * 1000.0).round() as u16;
        let sens = [1.2, 1.0, 1.1];
        for (c, s) in x.condition.iter_mut().zip(sens) {
            let target = (1000.0 - (1.0 - sat) * 1000.0 * s).clamp(0.0, 1000.0);
            *c = (f64::from(*c) + (target - f64::from(*c)) * rate).round() as u16;
        }
        x.decided = at;
        if sat < starve_below {
            let q = (sp.starve_hazard * (1.0 - sat / starve_below)).clamp(0.0, 1.0);
            let key = StreamKey::new(world, SYS_STARVE, x.seed);
            let mut starved = 0u64;
            let mut died = 0u64;
            for c in x.classes.iter_mut() {
                let Some(k) = c.cohort_on(&sp, day) else {
                    continue;
                };
                let alive = c.alive_on(&sp, yd, x.seed, day);
                let p = (q * sens[k.index()]).min(1.0);
                let s = key
                    .slot((n as u64) << 20 ^ c.first)
                    .binomial(u64::from(alive), p) as u32;
                if s > 0 {
                    died += u64::from(c.reanchor(&sp, yd, x.seed, day));
                    c.alive -= s.min(c.alive);
                    starved += u64::from(s);
                }
            }
            x.ledger.died += died;
            x.ledger.starved += starved;
            if starved > 0 {
                self.stats.starvations += 1;
                self.tape.event(
                    kinds::HERDS_STARVATION,
                    At::new(at.0, u32::from(frame), centre.0 as i32, centre.1 as i32),
                    hid(h),
                    0,
                    starved as i64,
                );
                self.note(
                    at,
                    "starve",
                    Some(h),
                    format!("met {:.0}% of its demand; {starved} starved", sat * 100.0),
                );
            }
        }
    }

    /// Births: one binomial draw on the adults alive, at the condition's share of fertility and
    /// the density term `1 / (1 + A/M)` toward the range's K (EQUILIBRIUM.md §2.4.2, §3.2).
    /// Every class's natural deaths are committed first (the yearly anchor).
    fn breed(&mut self, h: HerdId, at: Tick, year: i64) {
        let yd = self.year_days();
        let day = day_of(at);
        let world = self.seed;
        let sp = self.sp(h).clone();
        let m_per_k = if self.tun.density_term {
            sp.bh_m_per_k(yd)
        } else {
            None
        };
        let x = self.herds.get_mut(h).expect("live");
        let mut died = 0u64;
        for c in x.classes.iter_mut() {
            died += u64::from(c.reanchor(&sp, yd, x.seed, day));
        }
        x.ledger.died += died;
        let adults: u64 = x
            .classes
            .iter()
            .filter(|c| c.cohort_on(&sp, day) == Some(Cohort::Adult))
            .map(|c| u64::from(c.alive))
            .sum();
        let cond = f64::from(x.condition[Cohort::Adult.index()]) / 1000.0;
        let density = match m_per_k {
            Some(r) if x.k > 0 => 1.0 / (1.0 + adults as f64 / (r * x.k as f64)),
            _ => 1.0,
        };
        let litters = sp.fertility.ceil().max(1.0);
        let p = (sp.fertility * cond * density / litters).clamp(0.0, 1.0);
        let births = StreamKey::new(world, SYS_BIRTH, x.seed)
            .slot(year as u64)
            .binomial(adults * litters as u64, p);
        let births = births.min(u64::from(u32::MAX));
        if births > 0 {
            x.classes.push(Class::new(
                x.next_index,
                births as u32,
                day,
                day,
                births as u32,
            ));
            x.next_index += births;
            x.ledger.born += births;
        }
        x.bred_year = year;
        self.stats.breedings += 1;
        self.note(
            at,
            "births",
            Some(h),
            format!(
                "{births} born to {adults} adults (condition {:.0}%, density {:.2})",
                cond * 100.0,
                density
            ),
        );
    }

    /// Takes up to `n` latent members (alive and never seen) out of herd `h`'s classes at `day`,
    /// in proportion to each class's latent members times `weight` of its cohort (largest
    /// remainder). Every touched class is re-anchored first (its deaths committed). Returns what
    /// each class gave, by class index, and the total.
    fn take_latent(
        &mut self,
        h: HerdId,
        day: i64,
        n: u64,
        weight: impl Fn(Cohort) -> u64,
    ) -> (Vec<(usize, u64)>, u64) {
        let yd = self.year_days();
        let sp = self.sp(h).clone();
        let x = self.herds.get_mut(h).expect("live");
        let avail: Vec<u64> = x
            .classes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let alive = u64::from(c.alive_on(&sp, yd, x.seed, day));
                let seen = x.seen_in(i).count() as u64;
                alive.saturating_sub(seen)
            })
            .collect();
        let wt: Vec<u64> = x
            .classes
            .iter()
            .zip(&avail)
            .map(|(c, a)| c.cohort_on(&sp, day).map_or(0, |k| a * weight(k)))
            .collect();
        let wsum: u64 = wt.iter().sum();
        if wsum == 0 || n == 0 {
            return (Vec::new(), 0);
        }
        let n = n.min(
            avail
                .iter()
                .zip(&wt)
                .filter(|(_, w)| **w > 0)
                .map(|(a, _)| *a)
                .sum(),
        );
        let mut take: Vec<(u64, u64, usize)> = wt
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                let q = u128::from(n) * u128::from(w);
                (
                    ((q / u128::from(wsum)) as u64).min(avail[i]),
                    (q % u128::from(wsum)) as u64,
                    i,
                )
            })
            .collect();
        let mut given: u64 = take.iter().map(|t| t.0).sum();
        let mut order: Vec<usize> = (0..take.len()).collect();
        order.sort_by(|&a, &b| take[b].1.cmp(&take[a].1).then(a.cmp(&b)));
        while given < n {
            let before = given;
            for &i in &order {
                if given >= n {
                    break;
                }
                if take[i].0 < avail[i] && wt[i] > 0 {
                    take[i].0 += 1;
                    given += 1;
                }
            }
            if given == before {
                break;
            }
        }
        let mut died = 0u64;
        let mut out = Vec::new();
        for (k, _, i) in take {
            if k == 0 {
                continue;
            }
            let c = &mut x.classes[i];
            died += u64::from(c.reanchor(&sp, yd, x.seed, day));
            c.alive -= (k as u32).min(c.alive);
            out.push((i, k));
        }
        x.ledger.died += died;
        (out, given)
    }

    /// Spring dispersal (EQUILIBRIUM.md §2.4.2), at the breeding decision: a herd above
    /// `donor_above` of its K sends `disperse_share` of its latent adults (one binomial draw) to
    /// the 4-neighbouring ranges below `taker_below` of theirs, split evenly. A neighbour takes
    /// them if it is empty (it becomes the donor's species) or of the same species; the
    /// dispersers keep their ages.
    fn disperse(&mut self, h: HerdId, at: Tick, year: i64) {
        let t = &self.tun;
        if t.disperse_share <= 0.0 {
            return;
        }
        let (share, donor_above, taker_below) = (t.disperse_share, t.donor_above, t.taker_below);
        let x = &self.herds[h];
        let alive = self.count(h, at);
        if (alive as f64) <= donor_above * x.k as f64 {
            return;
        }
        let (species, frame, range, seed) = (x.species, x.frame, x.range, x.seed);
        // The neighbours, across seams: the range centres' canonical regions.
        let rb = REGION_BLOCKS as i32;
        let centre = (range.1 * rb + rb / 2, range.2 * rb + rb / 2);
        let mut takers: Vec<HerdId> = Vec::new();
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let c = self
                .topo(frame)
                .canonical_i32(centre.0 + dx * rb, centre.1 + dz * rb);
            for g in self.range_herds(range_of(frame, c)) {
                if g == h || takers.contains(&g) {
                    continue;
                }
                let y = &self.herds[g];
                let thin = (self.count(g, at) as f64) < taker_below * y.k as f64;
                if thin && (y.vacant.is_some() || y.species == species) {
                    takers.push(g);
                }
            }
        }
        if takers.is_empty() {
            return;
        }
        let day = day_of(at);
        let adults = self.cohorts(h, at)[Cohort::Adult.index()];
        let latent_adults = adults.saturating_sub(self.herds[h].seen.len() as u64);
        let go = StreamKey::new(self.seed, SYS_DISPERSE, seed)
            .slot(year as u64)
            .binomial(latent_adults, share);
        if go == 0 {
            return;
        }
        let (gave, sent) = self.take_latent(h, day, go, |c| u64::from(c == Cohort::Adult));
        if sent == 0 {
            return;
        }
        // Birth days of what left, in class order: each taker takes the next `quota` of them.
        let mut leaving: Vec<(i64, u64)> = gave
            .iter()
            .map(|&(i, k)| (self.herds[h].classes[i].birth_day, k))
            .collect();
        let condition = self.herds[h].condition;
        self.herds.get_mut(h).expect("live").ledger.left += sent;
        self.stats.dispersals += 1;
        self.stats.dispersed += sent;
        let n = takers.len() as u64;
        for (j, g) in takers.into_iter().enumerate() {
            let quota = sent / n + u64::from((j as u64) < sent % n);
            let mut got: Vec<(i64, u64)> = Vec::new();
            let mut need = quota;
            while need > 0 {
                let Some(first) = leaving.first_mut() else {
                    break;
                };
                let k = first.1.min(need);
                got.push((first.0, k));
                first.1 -= k;
                need -= k;
                if first.1 == 0 {
                    leaving.remove(0);
                }
            }
            self.arrive(g, &got, species, condition, at, year);
        }
        self.note(
            at,
            "disperse",
            Some(h),
            format!("{sent} adults left for thin neighbours"),
        );
    }

    /// Dispersers reach range `g`: one class per birth day, booked as arrived. An empty range
    /// takes their species, is scheduled to decide and does not breed again this year.
    fn arrive(
        &mut self,
        g: HerdId,
        got: &[(i64, u64)],
        species: HerdSpecies,
        condition: [u16; 3],
        at: Tick,
        year: i64,
    ) {
        let total: u64 = got.iter().map(|c| c.1).sum();
        if total == 0 {
            return;
        }
        let day = day_of(at);
        let y = self.herds.get_mut(g).expect("live");
        for &(birth, k) in got {
            y.classes
                .push(Class::new(y.next_index, k as u32, birth, day, k as u32));
            y.next_index += k;
        }
        y.ledger.arrived += total;
        if y.vacant.take().is_none() {
            return;
        }
        y.species = species;
        y.condition = condition;
        y.satisfaction = 1000;
        y.bred_year = year;
        y.decided = at;
        y.route = Route::stay(y.home, at, at);
        let (frame, home) = (y.frame, y.home);
        let next = self.decide_at(g, at + 1);
        self.herds.get_mut(g).expect("live").next_decide = next;
        self.schedule(next, Ev::Decide { h: g });
        self.stats.recolonised += 1;
        self.tape.event(
            kinds::HERDS_FOUNDED,
            At::new(at.0, u32::from(frame), home.0, home.1),
            hid(g),
            0,
            total as i64,
        );
        self.note(
            at,
            "recolonise",
            Some(g),
            format!("{total} {} came from a neighbour", species.name()),
        );
    }

    /// Books an empty range's frontier founding (EQUILIBRIUM.md §2.4.2): one exponential wait at
    /// `frontier_per_year` a range-year, drawn from the moment it emptied. No rate: never.
    fn book_frontier(&mut self, h: HerdId, at: Tick) {
        let rate = self.tun.frontier_per_year;
        let x = self.herds.get_mut(h).expect("live");
        if rate <= 0.0 {
            x.next_decide = Tick(u64::MAX);
            return;
        }
        let u = StreamKey::new(self.seed, SYS_FRONTIER, x.seed)
            .slot(at.0)
            .next_f64_open();
        let year_ticks = self.subs().year_ticks() as f64;
        let wait = (-u.ln() / rate * year_ticks).min(1e15) as u64;
        let when = at + wait.max(1);
        let x = self.herds.get_mut(h).expect("live");
        x.next_decide = when;
        self.schedule(when, Ev::Decide { h });
    }

    /// An empty range's frontier founding is due: `max(frontier_min, frontier_share x K)` young
    /// adults of its species arrive, on the side of its home facing the nearest live herd within
    /// `frontier_search` ranges (at the home itself if none), unless a player stands near, which
    /// puts it off to the next sub-season.
    fn frontier(&mut self, legs: &mut LegIndex, h: HerdId, at: Tick) {
        let x = &self.herds[h];
        let (frame, home, range, k, species) = (x.frame, x.home, x.range, x.k, x.species);
        let n = ((self.tun.frontier_share * k as f64).round() as u64).max(self.tun.frontier_min);
        let sp = self.tun.species(species).clone();
        // Where they come in.
        let mut from: Option<(f64, (i64, i64))> = None;
        let topo = self.topo(frame);
        let hp = (i64::from(home.0), i64::from(home.1));
        let rb = REGION_BLOCKS as i32;
        let centre = (range.1 * rb + rb / 2, range.2 * rb + rb / 2);
        'rings: for ring in 1..=self.tun.frontier_search.max(0) {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring {
                        continue;
                    }
                    let c = topo.canonical_i32(centre.0 + dx * rb, centre.1 + dz * rb);
                    for g in self.range_herds(range_of(frame, c)) {
                        let y = &self.herds[g];
                        if y.vacant.is_some() || self.count(g, at) == 0 {
                            continue;
                        }
                        let p = self.at_pos(y, at);
                        let d = dist(topo, hp, p);
                        if from.is_none_or(|(b, _)| d < b) {
                            from = Some((d, p));
                        }
                    }
                }
            }
            if from.is_some() {
                break 'rings;
            }
        }
        let mut arrive = home;
        if let Some((d, p)) = from
            && d > 1.0
        {
            let step = (f64::from(rb) * 3.0 / 8.0).min(d);
            let (vx, vz) = (p.0 - hp.0, p.1 - hp.1);
            let (ux, uz) = (vx as f64 / d, vz as f64 / d);
            let q = topo.canonical(
                hp.0 + (ux * step).round() as i64,
                hp.1 + (uz * step).round() as i64,
            );
            let cell = cell_of(q.0, q.1);
            if self.climate.terrain(frame, cell.0, cell.1).is_finite() {
                arrive = (q.0 as i32, q.1 as i32);
            }
        }
        // Not in sight of a player: wanderers walk in, they do not appear.
        let r = f64::from(self.tun.view_radius + self.tun.hysteresis + sp.footprint_radius(n));
        let ap = (i64::from(arrive.0), i64::from(arrive.1));
        let watched = self.players.iter().any(|p| {
            p.pos.is_some_and(|(pos, _)| {
                pos.frame == frame && dist(topo, ap, (i64::from(pos.x), i64::from(pos.z))) < r
            })
        });
        if watched {
            let next = self.decide_at(h, at + 1);
            self.herds.get_mut(h).expect("live").next_decide = next;
            self.schedule(next, Ev::Decide { h });
            self.stats.frontier_postponed += 1;
            return;
        }
        let day = day_of(at);
        let subs = self.subs();
        let year = Subs::year(subs.index(at));
        // Young adults: born in the last breeding season but one before now.
        let born_sub = u32::from(sp.breeding_sub_seasons.first().copied().unwrap_or(0));
        let since_sub = (Subs::in_year(subs.index(at)) + 12 - born_sub) % 12;
        let phase = (i64::from(since_sub) * subs.len as i64
            + (at.0 as i64 - subs.start(subs.index(at)).0 as i64))
            / TICKS_PER_DAY as i64;
        let age = i64::from(sp.maturity_days) + phase;
        let x = self.herds.get_mut(h).expect("live");
        x.classes
            .push(Class::new(x.next_index, n as u32, day - age, day, n as u32));
        x.next_index += n;
        x.ledger.founded += n;
        x.vacant = None;
        x.condition = [800; 3];
        x.satisfaction = 1000;
        x.bred_year = year;
        x.decided = at;
        x.route = Route::stay(arrive, at, at);
        self.stats.frontier_foundings += 1;
        self.tape.event(
            kinds::HERDS_FOUNDED,
            At::new(at.0, u32::from(frame), arrive.0, arrive.1),
            hid(h),
            0,
            n as i64,
        );
        self.note(
            at,
            "frontier",
            Some(h),
            format!("{n} {} came from beyond", species.name()),
        );
        self.decide(legs, h, at);
    }

    /// Every latent member of herd `h` dies at `at`, booked as killed (a test and tool hook: the
    /// world wipe of EQUILIBRIUM.md §3.6). Members a player has seen stay. The range empties at
    /// its next decision.
    pub fn cull(&mut self, h: HerdId, at: Tick) -> u64 {
        let (_, n) = self.take_latent(h, day_of(at), u64::MAX, |_| 1);
        self.herds.get_mut(h).expect("live").ledger.killed += n;
        n
    }

    // ---- players -------------------------------------------------------------------------------

    fn slot_of(&mut self, uuid: Uuid) -> u32 {
        let key = player_key(uuid);
        if let Some(&s) = self.player_by_key.get(&key) {
            return s;
        }
        let s = self.players.len() as u32;
        self.players.push(PlayerView {
            uuid,
            actor: 0,
            pos: None,
            interest: None,
            puppets: 0,
        });
        self.player_by_key.insert(key, s);
        s
    }

    /// Where a player is (PLAYER, every 20 ticks). Books or keeps the player's herd interest area
    /// and looks for herds whose routes come near.
    pub fn observe_player(&mut self, legs: &mut LegIndex, uuid: Uuid, pos: Pos, at: Tick) {
        let slot = self.slot_of(uuid);
        let window = legs.window_ticks();
        let view = self.tun.view_radius;
        let p = &self.players[slot as usize];
        let keep = p.interest.and_then(|s| {
            let row = legs.get(s.leg).filter(|r| r.generation == s.generation)?;
            let moved = self.topo(pos.frame).dist_sq(
                (i64::from(row.path.x0), i64::from(row.path.z0)),
                (i64::from(pos.x), i64::from(pos.z)),
            ) as i64;
            let quarter = i64::from(view / 4).pow(2);
            (row.path.frame == pos.frame && moved <= quarter && row.path.t1.0 >= at.0 + window)
                .then_some(s)
        });
        self.players[slot as usize].pos = Some((pos, at));
        if keep.is_some() {
            return;
        }
        let path = Path {
            frame: pos.frame,
            x0: pos.x,
            z0: pos.z,
            x1: pos.x,
            z1: pos.z,
            t0: at,
            t1: at + 2 * window,
            radius: view,
        };
        let old = self.players[slot as usize].interest;
        let stamp = match old.and_then(|s| legs.replan(s.leg, path)) {
            Some(s) => s,
            None => legs.book(path, player_owner(slot)),
        };
        self.players[slot as usize].interest = Some(stamp);
        // Herds whose routes pass near, across seams.
        let reach = i64::from(view) + 1_024;
        let (x, z) = (i64::from(pos.x), i64::from(pos.z));
        let mut near = BTreeSet::new();
        for (rx, rz) in self.regions_near(pos.frame, x, z, reach) {
            if let Some(s) = self.regions.get(region_key((pos.frame, rx, rz))) {
                near.extend(s.routes.iter().map(|i| herd_at(*i)));
            }
        }
        for h in near {
            if self.herds.get(h).is_some_and(|x| x.embodied.is_none()) {
                self.clip(legs, h, slot, at);
            }
        }
    }

    /// The player left (logged out, changed dimension): its area goes, and its groups go home.
    pub fn player_left(&mut self, legs: &mut LegIndex, uuid: Uuid, at: Tick) {
        let slot = self.slot_of(uuid);
        if let Some(s) = self.players[slot as usize].interest.take() {
            legs.cancel(s.leg);
        }
        self.players[slot as usize].pos = None;
        // Its herd legs go too: nothing can meet an area that is gone. Only the active herds
        // can hold one (or a group), in handle order as before.
        let ids: Vec<HerdId> = self.active.iter().copied().collect();
        for &h in &ids {
            if self.herds[h].clips.contains_key(&slot)
                && let Some(s) = self.herds.get_mut(h).expect("live").clips.remove(&slot)
            {
                legs.cancel(s.leg);
            }
        }
        let mine: Vec<HerdId> = ids
            .into_iter()
            .filter(|h| {
                self.herds[*h]
                    .embodied
                    .as_ref()
                    .is_some_and(|e| e.player == slot && !e.leaving)
            })
            .collect();
        for h in mine {
            self.despawn(h, 2, at);
        }
        self.prune_active();
    }

    /// Meetings for every player whose area is near the herd's (new) route.
    fn meet_players(&mut self, legs: &mut LegIndex, h: HerdId, at: Tick) {
        if self.herds[h].embodied.is_some() {
            return;
        }
        let regions = &self.herds[h].regions;
        if regions.is_empty() {
            return;
        }
        let near: Vec<u32> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.interest.is_some_and(|s| legs.is_live(s))
                    && p.pos.is_some_and(|(pos, _)| {
                        let (rx, rz) = (
                            i64::from(pos.x).div_euclid(REGION_BLOCKS),
                            i64::from(pos.z).div_euclid(REGION_BLOCKS),
                        );
                        regions.iter().any(|r| {
                            r.0 == pos.frame && (r.1 - rx).abs() <= 1 && (r.2 - rz).abs() <= 1
                        })
                    })
            })
            .map(|(i, _)| i as u32)
            .collect();
        for slot in near {
            self.clip(legs, h, slot, at);
        }
    }

    /// Books (or replans) the part of the herd's route inside the player's area window into the
    /// leg index, and the meeting of the two (A15: lazily, only where a player could see it).
    fn clip(&mut self, legs: &mut LegIndex, h: HerdId, slot: u32, at: Tick) {
        self.clip_with(legs, h, slot, at, true);
    }

    /// Books the herd's leg for a player's area; with `meet`, also its meeting (without, the
    /// caller learns the meeting another way: the world's `route_area` hands it to
    /// `Viewer::crossings`).
    fn clip_with(&mut self, legs: &mut LegIndex, h: HerdId, slot: u32, at: Tick, meet: bool) {
        let Some(interest) = self.players[slot as usize].interest else {
            return;
        };
        let Some(ip) = legs
            .get(interest.leg)
            .filter(|r| r.generation == interest.generation)
            .map(|r| r.path)
        else {
            return;
        };
        let x = &self.herds[h];
        if ip.frame != x.frame {
            return;
        }
        let (t0, t1) = (ip.t0.max(at), ip.t1);
        if t1 <= t0 {
            return;
        }
        let r = self.footprint(h, at);
        let depth = self.tun.depth.min(r.saturating_sub(1));
        let (a, b) = (x.route.pos(t0), x.route.pos(t1));
        // A leg from the canonical start to the end in the same chart (TP7).
        let f = self.topo(x.frame).fold(a.0, a.1);
        let (a, b) = ((f.x, f.z), f.iso.cell(b.0, b.1));
        let path = Path {
            frame: x.frame,
            x0: a.0 as i32,
            z0: a.1 as i32,
            x1: b.0 as i32,
            z1: b.1 as i32,
            t0,
            t1,
            radius: (r - depth).max(1),
        };
        let old = x.clips.get(&slot).copied();
        let stamp = match old.and_then(|s| legs.replan(s.leg, path)) {
            Some(s) => s,
            None => legs.book(path, herd_owner(h)),
        };
        self.herds
            .get_mut(h)
            .expect("live")
            .clips
            .insert(slot, stamp);
        self.active.insert(h);
        self.stats.clips += 1;
        if !meet {
            return;
        }
        if let Some(m) = legs.meeting(stamp, interest) {
            self.schedule(
                m.at,
                Ev::Seen {
                    h,
                    player: slot,
                    meeting: m,
                },
            );
        }
    }

    fn seen(&mut self, legs: &mut LegIndex, h: HerdId, slot: u32, m: Meeting, at: Tick) {
        // Nothing embodies while the session's replay runs (docs/replay-gate.md §6, RP-3, RP-83):
        // a meeting booked before the session (the pages' wheel holds them) is a sighting of the
        // last session's world, and the player is not known again until the replay end. The group
        // is not decided; the herd stays abstract and consistent, and the sighting the replay end
        // routes books its own meeting.
        if self.outbox.defer() {
            return;
        }
        if !legs.is_meeting_live(&m) || self.herds.get(h).is_none_or(|x| x.embodied.is_some()) {
            self.stats.seen_stale += 1;
            return;
        }
        let Some((ppos, _)) = self.players.get(slot as usize).and_then(|p| p.pos) else {
            self.stats.seen_stale += 1;
            return;
        };
        self.stats.seen += 1;
        let budget = self
            .tun
            .player_cap
            .saturating_sub(self.players[slot as usize].puppets);
        let yd = self.year_days();
        let day = day_of(at);
        let sp = self.sp(h).clone();
        let alive = self.count(h, at);
        let r = f64::from(sp.footprint_radius(alive));
        let view = f64::from(self.tun.view_radius);
        let p = (i64::from(ppos.x), i64::from(ppos.z));
        let window = legs.window_ticks();
        // The herd's centre and the image of it on the player's side (TP11); across an
        // edge_stop seam it stays abstract (TP12).
        let topo = self.topo.clone();
        let t = topo.of(self.herds[h].frame);
        let here = self.at_pos(&self.herds[h], at);
        let Some((centre, to_body)) = t.embodied_image(p, here) else {
            self.stats.seen_stale += 1;
            self.note(
                at,
                "materialize",
                Some(h),
                format!("stays abstract for player slot {slot}: across an edge_stop seam"),
            );
            return;
        };
        let chart = {
            let rp = self.herds[h].route.pos(at);
            to_body.compose(t.fold(rp.0, rp.1).iso)
        };
        let dmin = if t.is_flat() {
            self.herds[h].route.closest(p, at, at + 2 * window).0
        } else {
            self.herds[h]
                .route
                .closest_through(t, p, at, at + 2 * window)
                .0
        };
        let share = lens_area(r, view, dmin) / (std::f64::consts::PI * r * r);
        let n = ((alive as f64 * share).round() as u64)
            .max(1)
            .min(u64::from(self.tun.group_cap))
            .min(u64::from(budget))
            .min(alive) as usize;
        if n == 0 {
            self.stats.seen_empty += 1;
            return;
        }
        let Some(heading) = self.body_target(&self.herds[h], chart, at) else {
            self.stats.seen_stale += 1;
            return;
        };
        let seen_cap = self.tun.seen_cap;
        let regrow = self.tun.sheared_ticks;
        let x = self.herds.get_mut(h).expect("live");
        members::reconcile(x, &sp, yd, day);
        let picked = members::pick(x, &sp, yd, day, n, at, seen_cap);
        // Above the herd's own epoch and above its floor, which it spends (§5.3, RP-81). The
        // floor goes in before the wrap rule, so that it survives it.
        let floor = self.floors.remove(&agent_of(h)).unwrap_or(0);
        x.epoch = x.epoch.max(floor).wrapping_add(1).max(1);
        let epoch = x.epoch;
        let d = (((centre.0 - p.0) as f64).powi(2) + ((centre.1 - p.1) as f64).powi(2)).sqrt();
        let inner = (view - 4.0).max(1.0);
        let place = StreamKey::new(x.seed, SYS_PLACE, u64::from(epoch));
        let mut spawns = Vec::with_capacity(picked.len());
        for &k in &picked {
            let Some(mb) = member(x, &sp, k, day, at, regrow) else {
                continue;
            };
            let mut rng = place.slot(k);
            let mut spot = None;
            for _ in 0..24 {
                let rr = r * rng.next_f64().sqrt();
                let th = rng.next_f64() * std::f64::consts::TAU;
                let q = (
                    centre.0 + (rr * th.cos()) as i64,
                    centre.1 + (rr * th.sin()) as i64,
                );
                if (((q.0 - p.0) as f64).powi(2) + ((q.1 - p.1) as f64).powi(2)).sqrt() <= inner
                    && !t.near_cone(q.0, q.1, CONE_CLEARANCE)
                {
                    spot = Some(q);
                    break;
                }
            }
            let spot = spot.unwrap_or_else(|| {
                let s = ((d + r - inner) / 2.0).clamp(0.0, r);
                let f = if d > 0.0 { s / d } else { 0.0 };
                (
                    centre.0 + ((p.0 - centre.0) as f64 * f) as i64,
                    centre.1 + ((p.1 - centre.1) as f64 * f) as i64,
                )
            });
            let young = mb.cohort == Cohort::Young;
            let vanilla_age = if young {
                let left = 1.0 - f64::from(mb.age_days) / f64::from(sp.maturity_days.max(1));
                -((24_000.0 * left).round() as i32).max(1)
            } else {
                0
            };
            spawns.push(MemberSpawn {
                member: k,
                seed: mb.seed,
                variant: mb.look.variant(),
                vanilla_age,
                age_days: mb.age_days,
                cohort: mb.cohort,
                flags: if mb.sheared { MEMBER_SHEARED } else { 0 },
                x: spot.0 as i32,
                z: spot.1 as i32,
            });
        }
        let species = x.species;
        let frame = x.frame;
        let count = spawns.len() as u32;
        self.active.insert(h);
        let x = self.herds.get_mut(h).expect("live");
        x.embodied = Some(Embodied {
            epoch,
            player: slot,
            members: spawns.iter().map(|s| s.member).collect(),
            since: at,
            steer: heading,
            leaving: false,
            chart,
        });
        self.players[slot as usize].puppets += count;
        self.outbox.push(Cmd::Materialize(boundary::Materialize {
            agent: agent_of(h),
            epoch,
            species,
            dimension: u32::from(frame),
            flags: MAT_SURFACE,
            centre: (centre.0 as i32, centre.1 as i32),
            target: heading,
            speed: (sp.speed_blocks_per_day / TICKS_PER_DAY as f64) as f32,
            radius: r as u32,
            members: spawns,
        }));
        self.stats.materialized += 1;
        self.schedule(at + self.tun.check_ticks, Ev::Check { h, epoch });
        self.note(
            at,
            "materialize",
            Some(h),
            format!(
                "{count} of {alive} for player slot {slot} ({:.1}% of the footprint in view)",
                share * 100.0
            ),
        );
    }

    fn check_group(&mut self, legs: &mut LegIndex, h: HerdId, epoch: u32, at: Tick) {
        let Some(x) = self.herds.get(h) else {
            return;
        };
        let Some(e) = x
            .embodied
            .as_ref()
            .filter(|e| e.epoch == epoch && !e.leaving)
        else {
            return;
        };
        // A slot the engine does not hold (a group the pages kept for a player it no longer
        // knows) is a player who is gone.
        let p = self.players.get(e.player as usize);
        let gone =
            p.is_none_or(|p| p.pos.is_none() || !p.interest.is_some_and(|s| legs.is_live(s)));
        if gone {
            self.despawn(h, 2, at);
            return;
        }
        let (pos, _) = p.and_then(|p| p.pos).expect("checked");
        let r = f64::from(self.footprint(h, at));
        let far = f64::from(self.tun.view_radius) + r + f64::from(self.tun.hysteresis);
        if dist(
            self.topo(x.frame),
            self.at_pos(x, at),
            (i64::from(pos.x), i64::from(pos.z)),
        ) > far
        {
            self.despawn(h, 1, at);
            return;
        }
        // Its walk reaches an edge_stop seam: it goes before any body would cross (TP12).
        if self.body_target(x, e.chart, at).is_none() {
            self.note(
                at,
                "despawn",
                Some(h),
                "its walk reaches an edge_stop seam".into(),
            );
            self.despawn(h, 3, at);
            return;
        }
        self.steer(h, at);
        self.schedule(at + self.tun.check_ticks, Ev::Check { h, epoch });
    }

    fn steer(&mut self, h: HerdId, at: Tick) {
        let speed = (self.sp(h).speed_blocks_per_day / TICKS_PER_DAY as f64) as f32;
        let Some(chart) = self.herds[h].embodied.as_ref().map(|e| e.chart) else {
            return;
        };
        // The next waypoint in the bodies' frame (TP11), short of any stopped seam (TP12).
        let Some(heading) = self.body_target(&self.herds[h], chart, at) else {
            return;
        };
        let x = self.herds.get_mut(h).expect("live");
        let arrive = match x.route.segment_at(at) {
            Some(i) => x.route.times[i + 1],
            None => x.route.until,
        };
        let agent = agent_of(h);
        let Some(e) = x.embodied.as_mut().filter(|e| !e.leaving) else {
            return;
        };
        if e.steer == heading {
            return;
        }
        e.steer = heading;
        let epoch = e.epoch;
        self.outbox.push(Cmd::Steer(boundary::Steer {
            agent,
            epoch,
            speed,
            target: heading,
            arrive: arrive.0 as i64,
        }));
        self.stats.steers += 1;
    }

    fn despawn(&mut self, h: HerdId, reason: u32, at: Tick) {
        let x = self.herds.get_mut(h).expect("live");
        let Some(e) = x.embodied.as_mut().filter(|e| !e.leaving) else {
            return;
        };
        e.leaving = true;
        let epoch = e.epoch;
        self.outbox.push(Cmd::Despawn(boundary::Despawn {
            agent: agent_of(h),
            epoch,
            reason,
        }));
        self.stats.despawns += 1;
        self.note(at, "despawn", Some(h), format!("reason {reason}"));
    }

    /// A session's replay begins (docs/replay-gate.md §5.1, RP-5, RP-80): no group changes. What
    /// the pages hold embodied stays embodied, at its epoch, with its members, so that the
    /// journal's records of the last session's bodies (their casualties, their shearing, their
    /// capture) count against the bodies they were. What is still queued takes its verdict (every
    /// kind goes: a command of an earlier session names a group Java no longer has), and every
    /// push is gated until [`Herds::replay_done`]. The players' work waits for it too (§5.6,
    /// RP-26): the host leaves the herds out of the session start's `player_left`.
    ///
    /// `from_pages`: whether this engine came back from its pages (RP-31): only then is a
    /// replayed HERD_MATERIALIZED honoured (§5.2).
    pub fn begin_replay(&mut self, from_pages: bool) {
        self.replay_from_pages = from_pages;
        self.replay_honoured = 0;
        self.replay_bound = self.herds.next_index();
        self.outbox.begin_replay();
    }

    /// Whether herd `h` existed when the session's replay began (RP-84): only such a herd is the
    /// one the last session's records name. Nothing founds a herd while replaying (the host holds
    /// every founding to the replay end), so this is the herds the pages held; it keeps a herd
    /// founded after the replay end, on a handle the last session also used for another, from
    /// taking that session's copies.
    fn existed_at_replay(&self, h: HerdId) -> bool {
        h.index() < self.replay_bound
    }

    /// The session's replay has caught up (docs/replay-gate.md §5.6): no group outlives a
    /// session (docs/boundary.md §2.3). Every embodied group is abstract again with what the
    /// engine knew (its members seen at `at`), every leg booked for a player goes (the players
    /// report again), the players' puppet counts end, and the epoch floors become the herds'
    /// epochs (§5.3). Then pushes are queued as usual. A no-op unless a replay runs.
    pub fn replay_done(&mut self, legs: &mut LegIndex, at: Tick) {
        if !self.outbox.replaying() {
            return;
        }
        self.outbox.replay_done();
        let ids: Vec<HerdId> = std::mem::take(&mut self.active).into_iter().collect();
        for h in ids {
            for (_, s) in std::mem::take(&mut self.herds.get_mut(h).expect("live").clips) {
                legs.cancel(s.leg);
            }
            if let Some(e) = self.herds.get_mut(h).expect("live").embodied.take() {
                for k in &e.members {
                    if let Some(s) = self.herds.get_mut(h).expect("live").seen.get_mut(k) {
                        s.last = at;
                    }
                }
            }
        }
        for p in &mut self.players {
            if let Some(s) = p.interest.take() {
                legs.cancel(s.leg);
            }
            p.puppets = 0;
            p.pos = None;
        }
        // Every group is abstract now: a floor becomes its herd's epoch, for the pages (§5.3). A
        // floor of an agent with no herd yet stays: its first group takes an epoch above it.
        let floors = std::mem::take(&mut self.floors);
        for (agent, floor) in floors {
            match herd_of(agent).and_then(|h| self.herds.get_mut(h)) {
                Some(x) => x.epoch = x.epoch.max(floor),
                None => {
                    self.floors.insert(agent, floor);
                }
            }
        }
    }

    // ---- epochs --------------------------------------------------------------------------------

    /// No epoch is used twice (docs/replay-gate.md §5.3, RP-7, RP-57, RP-81): a replayed record or
    /// a copy that names a group's epoch ([`Obs::group_epoch`]) raises the agent's floor,
    /// whatever its kind and whatever the record does after (a refusal, a stale capture, a record
    /// of a herd not held), before any check that drops it. Live records name this session's
    /// epochs, which the herd holds. rtsim-jni calls it for a record it drops before the
    /// engine's (none today).
    pub fn fold_epoch(&mut self, o: &Obs) {
        if let Some((agent, epoch)) = o.group_epoch() {
            self.raise_floor(agent, epoch);
        }
    }

    /// An epoch the engine has seen for `agent` that its herd may not hold (§5.3).
    fn raise_floor(&mut self, agent: u64, epoch: u32) {
        let held = herd_of(agent)
            .and_then(|h| self.herds.get(h))
            .is_some_and(|x| x.epoch >= epoch);
        if !held {
            let f = self.floors.entry(agent).or_insert(0);
            *f = (*f).max(epoch);
        }
    }

    // ---- records -------------------------------------------------------------------------------

    fn embodied_for(&self, agent: u64, epoch: u32) -> Option<HerdId> {
        let h = herd_of(agent)?;
        self.herds
            .get(h)
            .and_then(|x| x.embodied.as_ref())
            .filter(|e| e.epoch == epoch)
            .map(|_| h)
    }

    fn first_time(&mut self, op: u64, at: Tick) -> bool {
        if op != 0 && self.ops.contains_key(&op) {
            self.stats.duplicate_ops += 1;
            return false;
        }
        if op != 0 {
            self.ops.insert(op, at);
        }
        true
    }

    /// Applies one record from the plugin. While the session's replay runs, the record's epoch
    /// reaches the floor first, whatever the record does after (RP-57, RP-81).
    pub fn ingest(&mut self, legs: &mut LegIndex, o: &Obs, at: Tick) {
        if self.outbox.replaying() {
            self.fold_epoch(o);
        }
        self.apply(legs, o, at);
    }

    /// A copy of a record from an ended session (S4p's REASSERT; docs/replay-gate.md §2.3, §5.7,
    /// RP-8, RP-9, RP-84), called by the host instead of [`Herds::ingest`] while it applies one.
    /// Its group is gone, so a copy matches no body. Every copy raises the floor to the epoch it
    /// names, live or replayed (RP-57). Then HERD_KILLED, HERD_TAKEN and HERD_SHEARED, which go
    /// by member key (and op id) already, as [`Herds::ingest`], to a herd that existed when the
    /// session's replay began (RP-84; any other counts as late); every other kind (a
    /// HERD_MATERIALIZED, a refusal, a capture, an obstacle, a group state) only counts as late.
    pub fn ingest_copy(&mut self, legs: &mut LegIndex, o: &Obs, at: Tick) {
        self.fold_epoch(o);
        match o {
            Obs::Killed(_) | Obs::Sheared(_) | Obs::Taken(_) => {
                // Only a herd the last session could have named (RP-84); the floor has the rest.
                let known = o
                    .group_epoch()
                    .and_then(|(agent, _)| herd_of(agent))
                    .is_some_and(|h| self.existed_at_replay(h));
                if known {
                    self.apply(legs, o, at);
                } else {
                    self.stats.late_records += 1;
                }
            }
            Obs::GroupState(_)
            | Obs::Materialized(_)
            | Obs::SpawnRejected(_)
            | Obs::Dematerialized(_)
            | Obs::Obstacle(_) => self.stats.late_records += 1,
        }
    }

    fn apply(&mut self, legs: &mut LegIndex, o: &Obs, at: Tick) {
        match o {
            Obs::GroupState(_) => {}
            Obs::Materialized(m) => self.honour(m, at),
            Obs::SpawnRejected(r) => {
                if let Some(h) = self.embodied_for(r.agent, r.epoch) {
                    self.stats.rejected += 1;
                    self.release(h);
                }
            }
            Obs::Dematerialized(d) => {
                if let Some(h) = self.embodied_for(d.agent, d.epoch) {
                    self.capture(h, &d.members, at);
                } else {
                    self.stats.late_records += 1;
                }
            }
            Obs::Killed(k) => {
                if !self.first_time(k.op, at) {
                    return;
                }
                let Some(h) = herd_of(k.agent).filter(|h| self.herds.contains(*h)) else {
                    self.stats.late_records += 1;
                    return;
                };
                if self.remove_member(h, k.member, at) {
                    let x = self.herds.get_mut(h).expect("live");
                    if k.by == KillerKind::Player {
                        x.ledger.killed += 1;
                    } else {
                        x.ledger.lost += 1;
                    }
                    self.stats.killed += 1;
                    self.note(
                        at,
                        "killed",
                        Some(h),
                        format!("member {} by {:?}", k.member, k.by),
                    );
                }
            }
            Obs::Sheared(s) => {
                let Some(h) = herd_of(s.agent).filter(|h| self.herds.contains(*h)) else {
                    return;
                };
                if let Some(m) = self.herds.get_mut(h).expect("live").seen.get_mut(&s.member) {
                    m.sheared_at = Some(at);
                    self.stats.sheared += 1;
                }
            }
            Obs::Taken(t) => {
                if !self.first_time(t.op, at) {
                    return;
                }
                let Some(h) = herd_of(t.agent).filter(|h| self.herds.contains(*h)) else {
                    self.stats.late_records += 1;
                    return;
                };
                if self.remove_member(h, t.member, at) {
                    self.herds.get_mut(h).expect("live").ledger.taken += 1;
                    self.stats.taken += 1;
                    self.note(
                        at,
                        "taken",
                        Some(h),
                        format!("member {} ({:?}): leaves the sim (A10)", t.member, t.how),
                    );
                }
            }
            Obs::Obstacle(ob) => {
                let Some(h) = self.embodied_for(ob.agent, ob.epoch) else {
                    self.stats.late_records += 1;
                    return;
                };
                // A group honoured from a replayed record has no chart (its player is not known),
                // so on a topology that folds, its report cannot be put back on the route's cell.
                let honoured = self.herds[h]
                    .embodied
                    .as_ref()
                    .is_some_and(|e| e.player == NO_PLAYER);
                if honoured && !self.topo(self.herds[h].frame).is_flat() {
                    self.stats.late_records += 1;
                    return;
                }
                self.obstacle(legs, h, ob, at);
            }
        }
    }

    /// A replayed HERD_MATERIALIZED (docs/replay-gate.md §5.2, RP-82): the last session's answer
    /// to a decision of this engine. On an engine that came back from its pages the group is
    /// honoured: the herd is embodied at the record's epoch with the members the decision picked
    /// (the engine's own pick, from the herd as it is now: seen members first, then fresh), and no
    /// player, so that the records that follow (a capture, a loss, an obstacle) count against the
    /// group they were. It places nothing and sends nothing; no puppet count carries it; no check
    /// is booked. The replay end makes it abstract. An epoch at or below the herd's group changes
    /// nothing (the pages hold it, or a newer one). A live record, a fresh engine's and a copy's
    /// are not honoured (RP-31, RP-8); the floor has them all.
    fn honour(&mut self, m: &boundary::Materialized, at: Tick) {
        if !self.outbox.replaying() || !self.replay_from_pages {
            return;
        }
        let Some(h) = herd_of(m.agent)
            .filter(|h| self.herds.contains(*h))
            .filter(|h| self.existed_at_replay(*h))
        else {
            return;
        };
        // Above the herd's own epoch (§5.2: "above the agent's") and above the group the pages
        // hold. The pages' state need not be a prefix of the journal: a herd abstract at epoch 7
        // has already used 5, and a replayed record of 5 is history, not a group to honour.
        if self.herds[h].epoch >= m.epoch
            || self.herds[h]
                .embodied
                .as_ref()
                .is_some_and(|e| e.epoch >= m.epoch)
        {
            return;
        }
        let yd = self.year_days();
        let day = day_of(at);
        let sp = self.sp(h).clone();
        let alive = self.count(h, at);
        let n = u64::from(m.spawned).min(alive) as usize;
        if n == 0 {
            return;
        }
        // An older group of the pages' whose end was not journaled: its record is lost, its
        // members stay seen.
        self.release(h);
        let heading = self.at_pos(&self.herds[h], at);
        let seen_cap = self.tun.seen_cap;
        let x = self.herds.get_mut(h).expect("live");
        members::reconcile(x, &sp, yd, day);
        let picked = members::pick(x, &sp, yd, day, n, at, seen_cap);
        x.embodied = Some(Embodied {
            epoch: m.epoch,
            player: NO_PLAYER,
            members: picked,
            since: at,
            steer: (heading.0 as i32, heading.1 as i32),
            leaving: false,
            chart: Iso::IDENTITY,
        });
        self.active.insert(h);
        self.replay_honoured += 1;
        self.note(
            at,
            "honour",
            Some(h),
            format!(
                "a replayed materialization of epoch {} ({n} members)",
                m.epoch
            ),
        );
    }

    fn release(&mut self, h: HerdId) {
        let x = self.herds.get_mut(h).expect("live");
        if let Some(e) = x.embodied.take()
            && let Some(p) = self.players.get_mut(e.player as usize)
        {
            p.puppets = p.puppets.saturating_sub(e.members.len() as u32);
        }
    }

    fn capture(&mut self, h: HerdId, caps: &[Capture], at: Tick) {
        let regrow = self.tun.sheared_ticks;
        let x = self.herds.get_mut(h).expect("live");
        for c in caps {
            if let Some(s) = x.seen.get_mut(&c.member) {
                s.last = at;
                if c.flags & MEMBER_SHEARED != 0 && s.sheared_at.is_none() {
                    s.sheared_at = Some(at);
                }
            }
        }
        // Notables whose wool has regrown fold back into the cohort (they stay seen).
        for s in x.seen.values_mut() {
            if s.sheared_at.is_some_and(|t| at.0 >= t.0 + regrow) {
                s.sheared_at = None;
            }
        }
        self.stats.captures += 1;
        self.release(h);
        self.note(at, "capture", Some(h), format!("{} captured", caps.len()));
    }

    /// A member leaves its herd for good (killed, taken). Returns whether it was alive.
    fn remove_member(&mut self, h: HerdId, member: u64, at: Tick) -> bool {
        let yd = self.year_days();
        let day = day_of(at);
        let sp = self.sp(h).clone();
        let x = self.herds.get_mut(h).expect("live");
        if x.dead.contains(&member) {
            self.stats.late_records += 1;
            return false;
        }
        let Some(ci) = x.class_of(member) else {
            self.stats.late_records += 1;
            return false;
        };
        let died = x.classes[ci].reanchor(&sp, yd, x.seed, day);
        x.ledger.died += u64::from(died);
        if x.classes[ci].alive == 0 {
            self.stats.late_records += 1;
            return false;
        }
        x.classes[ci].alive -= 1;
        x.seen.remove(&member);
        x.dead.insert(member);
        let mut slot = None;
        if let Some(e) = x.embodied.as_mut()
            && let Some(i) = e.members.iter().position(|&k| k == member)
        {
            e.members.remove(i);
            slot = Some(e.player);
        }
        if let Some(s) = slot
            && let Some(p) = self.players.get_mut(s as usize)
        {
            p.puppets = p.puppets.saturating_sub(1);
        }
        true
    }

    /// The group could not pass: entering the cell it was steered into costs more, and the herd
    /// replans from where it is.
    fn obstacle(&mut self, legs: &mut LegIndex, h: HerdId, ob: &boundary::Obstacle, at: Tick) {
        let x = &self.herds[h];
        let frame = x.frame;
        // The plugin reports in the bodies' frame: back to the route's chart, then keyed by
        // canonical cell (TP11, TP4).
        let back = x
            .embodied
            .as_ref()
            .map_or(Iso::IDENTITY, |e| e.chart)
            .inverse();
        let topo = self.topo.clone();
        let t = topo.of(frame);
        let grid = t.grid(i64::from(CELL_BLOCKS));
        let key = |p: (i64, i64)| -> (i32, i32) {
            if t.is_flat() {
                return cell_of(p.0, p.1);
            }
            let q = back.cell(p.0, p.1);
            let (cx, cz) = grid.of_block(q.0, q.1);
            (cx as i32, cz as i32)
        };
        let stuck = (i64::from(ob.stuck.0), i64::from(ob.stuck.1));
        let target = (i64::from(ob.target.0), i64::from(ob.target.1));
        let here = key(stuck);
        let d = (((stuck.0 - target.0) as f64).powi(2) + ((stuck.1 - target.1) as f64).powi(2))
            .sqrt()
            .max(1.0);
        let step = f64::from(CELL_BLOCKS);
        let ahead = (
            stuck.0 + ((target.0 - stuck.0) as f64 * step / d) as i64,
            stuck.1 + ((target.1 - stuck.1) as f64 * step / d) as i64,
        );
        let mut blocked = key(ahead);
        if blocked == here {
            blocked = key(target);
        }
        let stuck = back.cell(stuck.0, stuck.1);
        let share = f64::from(ob.failures) / f64::from(ob.present.max(ob.failures).max(1));
        let add = self.tun.obstacle_step * share.max(0.25);
        self.obstacles.report(
            (frame, blocked.0, blocked.1),
            at,
            add,
            self.tun.obstacle_max,
            self.tun.obstacle_half_life_ticks,
        );
        self.stats.obstacles += 1;
        let extra = self.obstacles.extra(
            (frame, blocked.0, blocked.1),
            at,
            self.tun.obstacle_half_life_ticks,
        );
        self.note(
            at,
            "obstacle",
            Some(h),
            format!(
                "{} of {} failed to pass into cell {blocked:?}; its cost is now {:.1}x",
                ob.failures,
                ob.present,
                1.0 + extra
            ),
        );
        // Stop where it is stuck and replan for the rest of the sub-season.
        let until = self.herds[h].next_decide;
        {
            let x = self.herds.get_mut(h).expect("live");
            x.route = Route::stay((stuck.0 as i32, stuck.1 as i32), at, until);
        }
        self.stats.replans += 1;
        self.plan(legs, h, at, until);
    }

    // ---- prey (the ecology prey interface) -----------------------------------------------------

    /// Herds a predator at (x, z) could hunt at `at`: within `range` of it, or within
    /// `range × follow_factor` if it follows them. Found through the region index (no scan).
    pub fn prey_near(
        &self,
        frame: u16,
        x: i64,
        z: i64,
        range: i64,
        at: Tick,
        predator: u64,
    ) -> Vec<HerdOffer> {
        let mut cands = BTreeSet::new();
        for (rx, rz) in self.regions_near(frame, x, z, range) {
            if let Some(s) = self.regions.get(region_key((frame, rx, rz))) {
                cands.extend(s.routes.iter().map(|i| herd_at(*i)));
            }
        }
        let followed: BTreeSet<HerdId> = self
            .follows
            .get(&predator)
            .map(|s| {
                s.iter()
                    .copied()
                    .filter(|h| {
                        self.herds.get(*h).is_some_and(|x| {
                            x.followers
                                .iter()
                                .any(|&(p, t)| p == predator && at.0 < t.0 + self.tun.follow_ticks)
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        cands.extend(followed.iter().copied());
        let mut out = Vec::new();
        for h in cands {
            let Some(hx) = self.herds.get(h) else {
                continue;
            };
            if hx.frame != frame || hx.vacant.is_some() {
                continue;
            }
            let f = followed.contains(&h);
            let reach = if f {
                range as f64 * self.tun.follow_factor
            } else {
                range as f64
            };
            let p = self.at_pos(hx, at);
            if dist(self.topo(frame), p, (x, z)) > reach {
                continue;
            }
            let alive = self.count(h, at);
            // Nothing below the refuge (EQUILIBRIUM.md §3.4): a herd at it is not worth a hunt.
            let available = alive
                .saturating_sub(hx.embodied_count())
                .min(alive.saturating_sub(self.refuge(hx.k)));
            if available == 0 {
                continue;
            }
            out.push(HerdOffer {
                herd: h,
                species: hx.species,
                pos: (p.0 as i32, p.1 as i32),
                radius: self.tun.species(hx.species).footprint_radius(alive),
                available,
                food_each: hx.species.food_points(),
                followed: f,
            });
        }
        out
    }

    /// A predator ate from herd `h` at `at`, hunting at (x, z): up to `want_food` fullness worth,
    /// no more than the predation share, only latent animals (an embodied group's are the
    /// plugin's to lose in a real fight), weighted toward the young and the old. Exact integers
    /// leave the herd's classes and enter its ledger as eaten. The predator follows the herd from
    /// now on.
    pub fn predate(
        &mut self,
        h: HerdId,
        x: i64,
        z: i64,
        at: Tick,
        want_food: i32,
        predator: u64,
    ) -> Predation {
        let (x_pos, z_pos) = (x, z);
        let Some(hx) = self.herds.get(h) else {
            return Predation::default();
        };
        let r = f64::from(self.footprint(h, at));
        if dist(self.topo(hx.frame), self.at_pos(hx, at), (x, z))
            > r + f64::from(self.tun.predation_reach)
        {
            return Predation::default();
        }
        let yd = self.year_days();
        let day = day_of(at);
        let sp = self.sp(h).clone();
        let alive = self.count(h, at);
        let food_each = hx.species.food_points().max(1);
        let want = (i64::from(want_food.max(0)) + i64::from(food_each) - 1) / i64::from(food_each);
        let cap = (alive * u64::from(self.tun.predation_share_milli) / 1000).max(1);
        let latent = alive.saturating_sub(hx.embodied_count());
        // Never below the refuge (EQUILIBRIUM.md §3.4): no herd is eaten out.
        let above = alive.saturating_sub(self.refuge(hx.k));
        let n = (want as u64).min(cap).min(latent).min(above);
        if n == 0 {
            return Predation::default();
        }
        let species = hx.species;
        let x_ = self.herds.get_mut(h).expect("live");
        // Weighted shares over the classes' unseen members (largest remainder).
        let avail: Vec<u64> = x_
            .classes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let alive = u64::from(c.alive_on(&sp, yd, x_.seed, day));
                let seen = x_.seen_in(i).count() as u64;
                alive.saturating_sub(seen)
            })
            .collect();
        let weight: Vec<u64> = x_
            .classes
            .iter()
            .zip(&avail)
            .map(|(c, a)| {
                c.cohort_on(&sp, day)
                    .map_or(0, |k| a * u64::from(sp.predation_weight[k.index()]))
            })
            .collect();
        let wsum: u64 = weight.iter().sum();
        if wsum == 0 {
            return Predation::default();
        }
        let n = n.min(avail.iter().sum());
        let mut take: Vec<(u64, u64, usize)> = weight
            .iter()
            .enumerate()
            .map(|(i, &wt)| {
                let q = u128::from(n) * u128::from(wt);
                (
                    ((q / u128::from(wsum)) as u64).min(avail[i]),
                    (q % u128::from(wsum)) as u64,
                    i,
                )
            })
            .collect();
        let mut given: u64 = take.iter().map(|t| t.0).sum();
        let mut order: Vec<usize> = (0..take.len()).collect();
        order.sort_by(|&a, &b| take[b].1.cmp(&take[a].1).then(a.cmp(&b)));
        let mut pass = 0;
        while given < n && pass < 4 {
            for &i in &order {
                if given >= n {
                    break;
                }
                if take[i].0 < avail[i] && weight[i] > 0 {
                    take[i].0 += 1;
                    given += 1;
                }
            }
            pass += 1;
        }
        let mut died = 0u64;
        for (k, _, i) in take {
            if k == 0 {
                continue;
            }
            let c = &mut x_.classes[i];
            died += u64::from(c.reanchor(&sp, yd, x_.seed, day));
            c.alive -= (k as u32).min(c.alive);
        }
        x_.ledger.died += died;
        x_.ledger.eaten += given;
        x_.followers.retain(|&(p, _)| p != predator);
        x_.followers.push((predator, at));
        let cap = self.tun.followers_cap;
        while x_.followers.len() > cap {
            let (p, _) = x_.followers.remove(0);
            if let Some(s) = self.follows.get_mut(&p) {
                s.remove(&h);
            }
        }
        self.follows.entry(predator).or_default().insert(h);
        self.stats.predations += 1;
        if self.log_enabled {
            let x = &self.herds[h];
            self.predations.push(PreyRecord {
                tick: at.0,
                day: dayf(at),
                herd: x.name.clone(),
                predator,
                animals: given,
                herd_pos: self.at_pos(x, at),
                at: (x_pos, z_pos),
            });
        }
        let food = (given as i64 * i64::from(food_each)).min(i64::from(i32::MAX)) as i32;
        if self.tape.is_on() {
            let x = &self.herds[h];
            let (hx, hz) = self.at_pos(x, at);
            self.tape.event(
                kinds::HERDS_PREDATION,
                At::new(at.0, u32::from(x.frame), hx as i32, hz as i32),
                hid(h),
                u128::from(predator),
                given as i64,
            );
        }
        self.note(
            at,
            "predation",
            Some(h),
            format!("predator {predator:#x} ate {given} {}", species.name()),
        );
        Predation {
            species: Some(species),
            animals: given,
            food,
        }
    }

    // ---- checks --------------------------------------------------------------------------------

    /// Conservation and identity invariants at `at`: per herd, the classes hold exactly what the
    /// ledger says entered and left (integers), indices are dense and monotonic, the seen and
    /// dead sets never overlap, embodied members are seen, and an empty range holds nothing;
    /// over the world, every disperser that left a range arrived in another.
    pub fn check(&self, at: Tick) -> Result<(), String> {
        let yd = self.year_days();
        let day = day_of(at);
        let (mut arrived, mut left) = (0u64, 0u64);
        for (h, x) in self.herds.iter() {
            arrived += x.ledger.arrived;
            left += x.ledger.left;
            if x.vacant.is_some() && x.classes.iter().any(|c| c.alive > 0) {
                return Err(format!("herd {}: an empty range holds animals", x.name));
            }
            let sp = self.tun.species(x.species);
            let anchors: i128 = x.classes.iter().map(|c| i128::from(c.alive)).sum();
            if anchors != x.ledger.expected_at_anchors() {
                return Err(format!(
                    "herd {} holds {anchors} at its anchors but its ledger says {}",
                    x.name,
                    x.ledger.expected_at_anchors()
                ));
            }
            let alive: u64 = x
                .classes
                .iter()
                .map(|c| u64::from(c.alive_on(sp, yd, x.seed, day)))
                .sum();
            if u128::from(alive) > anchors as u128 {
                return Err(format!("herd {} grew between events", x.name));
            }
            let entered = x.ledger.founded + x.ledger.born + x.ledger.arrived;
            if entered != x.next_index {
                return Err(format!(
                    "herd {}: {entered} founded, born and arrived but next index {}",
                    x.name, x.next_index
                ));
            }
            for w in x.classes.windows(2) {
                if w[0].end() > w[1].first {
                    return Err(format!("herd {}: classes overlap", x.name));
                }
            }
            if let Some(k) = x.seen.keys().find(|k| x.dead.contains(k)) {
                return Err(format!("herd {}: member {k} is seen and dead", x.name));
            }
            if let Some(e) = &x.embodied
                && let Some(k) = e.members.iter().find(|k| !x.seen.contains_key(k))
            {
                return Err(format!("herd {}: embodied member {k} is not seen", x.name));
            }
            let _ = h;
        }
        if arrived != left {
            return Err(format!("{left} dispersers left but {arrived} arrived"));
        }
        Ok(())
    }

    /// A hash of the whole state: the save's bytes.
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
    }

    /// The engine's heap: the resident pages of its row tables and its resident state.
    pub fn heap_bytes(&self) -> usize {
        self.heap_parts().iter().map(|p| p.1).sum()
    }

    /// [`Herds::heap_bytes`] by part, for the bench and the docs.
    pub fn heap_parts(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("herd rows", self.herds.rows().heap_bytes()),
            ("forage rows", self.forage.heap_bytes()),
            ("region index", self.regions.heap_bytes()),
            ("obstacles", self.obstacles.cells.len() * 64),
            ("wheel", self.wheel.heap_bytes()),
            ("active", self.active.len() * 16),
            ("claims", self.claims.len() * 40),
            ("ops", self.ops.len() * 32),
            (
                "follows",
                self.follows
                    .values()
                    .map(|s| 32 + s.len() * 16)
                    .sum::<usize>(),
            ),
            (
                "players",
                self.players.capacity() * std::mem::size_of::<PlayerView>(),
            ),
        ]
    }
}

// ---- one interest area per player, shared with the other modules ----------------------------------

impl Herds {
    /// A player's shared interest area (the phase 1 world's, `World::observe_area`) was booked or
    /// replanned: it is this player's area for the herds too, so the herds book no area of their
    /// own. The legs of herds whose routes come near are booked against it; their meetings come
    /// back through `Viewer::crossings`.
    pub fn shared_area(&mut self, legs: &mut LegIndex, uuid: Uuid, actor: u64, area: Stamp) {
        let slot = self.slot_of(uuid);
        let Some(path) = legs
            .get(area.leg)
            .filter(|r| r.generation == area.generation)
            .map(|r| r.path)
        else {
            return;
        };
        let pos = Pos::new(path.frame, path.x0, path.z0);
        {
            let p = &mut self.players[slot as usize];
            p.actor = actor;
            if let Some(old) = p.interest.filter(|o| o.leg != area.leg) {
                // An area of the herds' own from before: the shared one replaces it.
                legs.cancel(old.leg);
            }
            p.interest = Some(area);
            p.pos = Some((pos, path.t0));
        }
        let at = path.t0.max(self.now);
        let reach = i64::from(self.tun.view_radius.max(path.radius)) + 1_536;
        let (x, z) = (i64::from(pos.x), i64::from(pos.z));
        let mut near = BTreeSet::new();
        for (rx, rz) in self.regions_near(pos.frame, x, z, reach) {
            if let Some(s) = self.regions.get(region_key((pos.frame, rx, rz))) {
                near.extend(s.routes.iter().map(|i| herd_at(*i)));
            }
        }
        for h in near {
            if self.herds.get(h).is_some_and(|x| x.embodied.is_none()) {
                self.clip_with(legs, h, slot, at, false);
            }
        }
    }

    /// The interest area the herds hold for a player (their own, or the shared one).
    pub fn player_area(&self, uuid: Uuid) -> Option<Stamp> {
        let slot = *self.player_by_key.get(&player_key(uuid))?;
        self.players.get(slot as usize).and_then(|p| p.interest)
    }

    /// Herd puppets embodied for a player: 0 for one the herds do not know. They end at the
    /// session's replay end (docs/replay-gate.md §5.6).
    pub fn puppets(&self, uuid: Uuid) -> u32 {
        self.player_by_key
            .get(&player_key(uuid))
            .and_then(|s| self.players.get(*s as usize))
            .map_or(0, |p| p.puppets)
    }

    fn slot_of_actor(&self, actor: u64) -> Option<u32> {
        self.players
            .iter()
            .position(|p| p.actor == actor && actor != 0)
            .map(|i| i as u32)
    }
}

impl rtsim_world::interest::Viewer for Herds {
    /// Herd legs: owner bit 60 (the registry in `rtsim_world::interest`), not the herds' own
    /// player areas (bit 59 under it).
    fn owns(&self, owner: Owner) -> bool {
        owner_herd(owner).is_some() && owner.0 & PLAYER_TAG == 0
    }

    fn area_booked(
        &mut self,
        w: &mut rtsim_world::World,
        player: rtsim_knowledge::ActorId,
        uuid: Uuid,
        area: Stamp,
    ) {
        self.shared_area(&mut w.legs, uuid, player.to_bits(), area);
    }

    fn crossings(
        &mut self,
        w: &mut rtsim_world::World,
        player: rtsim_knowledge::ActorId,
        area: Stamp,
        found: &[Meeting],
    ) {
        let _ = area;
        let Some(slot) = self.slot_of_actor(player.to_bits()) else {
            return;
        };
        for m in found {
            let Some(h) = w.legs.get(m.a.leg).and_then(|r| owner_herd(r.owner)) else {
                continue;
            };
            self.schedule(
                m.at,
                Ev::Seen {
                    h,
                    player: slot,
                    meeting: *m,
                },
            );
        }
    }

    fn player_left(&mut self, w: &mut rtsim_world::World, player: rtsim_knowledge::ActorId) {
        let Some(slot) = self.slot_of_actor(player.to_bits()) else {
            return;
        };
        let uuid = self.players[slot as usize].uuid;
        // The area itself is the world's (it cancels it): forget it before the herds' own
        // `player_left`, which would cancel it a second time.
        self.players[slot as usize].interest = None;
        let at = self.now;
        self.player_left(&mut w.legs, uuid, at);
    }
}
