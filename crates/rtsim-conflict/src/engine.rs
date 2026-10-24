// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine: factions, feuds, parties and their events on one timing wheel
//! (PLAN.md §10.3; C16 to C18, C24, C25).
//!
//! - **Factions** are a culture (data), a camp that is a haven node in the world (C24: what they
//!   steal is stocked and sold there), a territory, a chief and an heir (knowledge notables:
//!   temperament from the seed bends every choice, C25; grudges pass to the heir, C24), and a pool
//!   of warriors recruited back in closed form.
//! - **Feuds** are one faction's grievance against one actor (P14: a player for direct acts, the
//!   shared settlers actor for unowned signals), in closed form ([`crate::grievance`]), with a rung
//!   on the ladder (none, notice, envoy, scouts, raid, invasion, siege). Each feud books one event:
//!   the earliest of its next rung's crossing (after the rung's dwell), its fall below the current
//!   rung (hysteresis), or its demand's deadline. Nothing is ticked.
//! - **Parties** (envoys, scouts, war bands, plunderers) are group agents with unit stacks that
//!   muster at the camp, march the road graph as legs in the world's leg index (owner bit 59, so a
//!   player's interest area sees them come), wait or fight at the target, get bored (C24: a booked
//!   departure brought forward by losses) and go home with what they took.
//! - **Raids on player districts and trains** go through the industry threat interface
//!   (`rtsim_ecology::infra::Infrastructure`): never better than observed, capped, applied on load
//!   even when the owner is offline (C16 to C18, I22 to I24). **Raids on villages and rival
//!   havens** move stock between the world's markets: taken into the band's hold, stocked at the
//!   haven when it comes home, conservation exact.
//! - **The raid director**: a band arriving where a player watches materializes in waves on an arc
//!   at the edge of the entity-ticking area on the band's side, under the spawn quota; survivors
//!   withdraw and casualties come back as records ([`crate::boundary`]).

use std::collections::{BTreeMap, BTreeSet};

use rtsim_archive::{At, Severity, Tape, kinds};
use rtsim_ecology::infra::Infrastructure;
use rtsim_industry::ids::DistrictId;
use rtsim_industry::threat::{Loads, Threat, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::meta_pages::{PagedMap, PagedWheel, ResidentArena};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_knowledge::{
    ActorId, ActorKind, Place, Report, ReportKind, SEVERITY_UNIT, Temperament, Trait,
};
use rtsim_settle::{NodeKind, Pos};
use rtsim_space::topology::{SharedTopologies, Topology};
use rtsim_trade::{Amount, GoodId, MILLI, PRICE_ONE};
use rtsim_travel::legs::{LegIndex, Meeting, Owner, Path, Stamp};
use rtsim_world::interest::{PLAYER_OWNER, Viewer};
use rtsim_world::{NodeData, World};
use serde::Serialize;

use crate::boundary::{self, Cmd, Obs, Uuid};
use crate::culture::{Culture, Cultures, PartyKind, SignalKind, Step, days_to_ticks};
use crate::grievance::{Grievance, K};
use crate::presence::{ChunkKey, Presence, chunk_of};
use crate::tables::{FactionTable, PlaceIndex, SourceTable};
use crate::tunables::ConflictTunables;

/// Leg owner bit of a conflict party (the registry is in `rtsim_world::interest::Viewer`).
pub const PARTY_OWNER: u64 = 1 << 59;
const MASK48: u64 = (1 << 48) - 1;

const SYS_PLUNDER: SystemId = SystemId::named("conflict.plunder");
const SYS_AMBUSH: SystemId = SystemId::named("conflict.ambush");
const SYS_CHIEF: SystemId = SystemId::named("conflict.chief");
const SYS_PEACE: SystemId = SystemId::named("conflict.peace");

/// The most a market price may raise a demand above the culture's fallback per coin (MZB-57): a
/// good whose price has collapsed would otherwise be asked by the million.
pub const TRIBUTE_PRICE_CAP: f64 = 4.0;
/// The settlers actor's external id (P14: one shared actor for unowned signals).
pub const SETTLERS_KEY: u64 = fnv1a64(b"mic_rtsim.conflict.settlers");

pub mod camp;

pub use camp::{Answered, Camp, Line, Retired, Slot, SlotState, Throne};

/// A feud's `wait_until` while only a change of its camp's room or lines can end the wait: no
/// event is booked for it, and `Conflict::unblock` plans it again when one comes.
pub const BLOCKED: Tick = Tick(u64::MAX / 2);

pub type Purpose = PartyKind;

/// A faction: a culture, a camp (a haven node in the world), a territory, a chief and an heir.
#[derive(Clone, Debug, PartialEq)]
pub struct Faction {
    pub key: u64,
    pub culture: u16,
    pub node: usize,
    pub pos: Pos,
    pub radius: u32,
    /// The faction itself as a knowledge actor (villages' sentiment toward it).
    pub actor: ActorId,
    pub chief: ActorId,
    pub chief_seed: u64,
    pub heir: ActorId,
    pub heir_seed: u64,
    /// Chiefs so far (keys the next one's seed).
    pub chiefs: u32,
    /// Warriors at the anchor time (closed form: recruits back to the cap).
    pub pool_at: Tick,
    pub pool: f64,
    pub cowed_until: Tick,
    pub plunders: u32,
    pub plunder_stamp: u32,
    /// Real fights seen (attack strength, the attackers' surviving share), newest last.
    pub observed: Vec<(f64, f64)>,
    /// The war chief is out with a band.
    pub chief_out: bool,
    /// The chief holds its crystal (MOWZIE.md ledger 9): units with a `without_crystal` row fight
    /// as that row while it does not. True until a report says otherwise.
    pub crystal: bool,
    /// The most members the camp holds, its pool and its war parties' members away together:
    /// the culture's `pool_cap`, or for a camp whose bodies a census counts, the count of its
    /// bodies at FACTION_FOUND bounded by the culture's (MOWZIE.md ledger 7).
    pub cap: u32,
    /// Members away: the units alive of its war parties. The pool refills only while it and
    /// these are under `cap` (ledger 6), so a band's survivors always have their slots.
    pub away: u32,
    /// The ledger of the camp's bodies (MOWZIE.md §4.9), kept for a camp whose bodies a census
    /// counts. Boxed: a camp with none costs a word.
    pub camp: Option<Box<Camp>>,
}

/// What an envoy asked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Demand {
    pub kind: SignalKind,
    pub good: GoodId,
    /// Whole items of `good`.
    pub tribute: i64,
    pub paid: i64,
    pub made: Tick,
    pub deadline: Tick,
    /// Impulses of `kind` since the demand (a signal is cut only if none came).
    pub after: f64,
    pub envoy: Option<Handle<Party>>,
}

/// Where a feud's raids go: the strongest recent source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Focus {
    pub pos: Pos,
    pub district: Option<DistrictId>,
    pub weight: f64,
}

/// One faction's grievance against one actor.
#[derive(Clone, Debug, PartialEq)]
pub struct Feud {
    pub g: Grievance,
    /// Raw continuous rates per tick by kind (sources × falloff), before weight and tolerance.
    pub raw: [f64; K],
    pub step: Step,
    pub since: Tick,
    pub demand: Option<Demand>,
    pub party: Option<Handle<Party>>,
    pub stamp: u32,
    pub focus: Option<Focus>,
    /// No escalation before this (a band could not be mustered).
    pub wait_until: Tick,
    pub raids: u32,
    /// The feud's in-world notice the plugin has not played yet (NOTICE_SHOWN): sent again at
    /// every session start until it is (MOWZIE.md §4.12, ledger 14). A later notice replaces it;
    /// it goes with the feud.
    pub notice: Option<Box<boundary::Notice>>,
    /// The envoy's demand was withdrawn (DEMAND_SETTLED 5): the feud holds on Envoy with no climb
    /// until `wait_until`, the withdrawn demand's deadline, and then sends a fresh envoy with a
    /// new demand instead of climbing (MZB-22).
    pub envoy_again: bool,
}

impl Feud {
    pub fn new(at: Tick) -> Self {
        Feud {
            g: Grievance::new(at),
            raw: [0.0; K],
            step: Step::None,
            since: at,
            demand: None,
            party: None,
            stamp: 0,
            focus: None,
            wait_until: Tick::ZERO,
            raids: 0,
            notice: None,
            envoy_again: false,
        }
    }

    /// Held on Envoy after a withdrawn demand ([`Feud::envoy_again`]); a feud no longer on Envoy
    /// is not.
    fn holds_envoy(&self) -> bool {
        self.envoy_again && self.step == Step::Envoy
    }
}

/// A continuous source with no direct actor: a district's or a chunk's emissions. P14 as Ben
/// changed it: it counts against the player who has spent the most time in its chunks
/// ([`crate::presence`]), and against the shared settlers only while nobody has.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub pos: Pos,
    pub district: Option<DistrictId>,
    /// Units per tick.
    pub rate: f64,
    /// (faction, falloff) it reaches.
    pub affects: Vec<(u32, f64)>,
    /// Whom it counts against now.
    pub actor: ActorId,
    /// The chunks it comes from (a district's chunks, or a record's one chunk).
    pub chunks: Vec<ChunkKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    District(DistrictId, Pos),
    Node(usize),
    Pos(Pos),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Phase {
    Mustering = 0,
    Out = 1,
    Waiting = 2,
    Fighting = 3,
    Home = 4,
}

impl Phase {
    pub fn from_u8(v: u8) -> Option<Phase> {
        Some(match v {
            0 => Phase::Mustering,
            1 => Phase::Out,
            2 => Phase::Waiting,
            3 => Phase::Fighting,
            4 => Phase::Home,
            _ => return None,
        })
    }
}

/// A group agent: envoy, scouts, war band or plunderers (C25: one unit).
#[derive(Clone, Debug, PartialEq)]
pub struct Party {
    pub faction: u32,
    pub kind: PartyKind,
    /// The feud it serves (none for plunder).
    pub feud: Option<ActorId>,
    /// Alive, by unit index.
    pub units: Vec<(u8, u32)>,
    /// In Minecraft now, by unit index.
    pub spawned: Vec<(u8, u32)>,
    /// In Minecraft when the engine recalled it, until the plugin counts the survivors.
    pub recalled: Vec<(u8, u32)>,
    pub start_units: u32,
    pub target: Target,
    pub frame: u16,
    pub route: Vec<(i32, i32)>,
    pub times: Vec<Tick>,
    pub seg: u32,
    pub phase: Phase,
    pub until: Tick,
    pub bored_at: Tick,
    pub plan: u32,
    pub leg: Option<Stamp>,
    pub epoch: u32,
    pub embodied: bool,
    pub seen_by: Option<ActorId>,
    pub wave: u16,
    /// World goods it holds (`Outside::held`), stocked at the haven when it comes home.
    pub loot: BTreeMap<GoodId, Amount>,
    /// Goods taken from player districts or trains (the industry engine booked them stolen),
    /// stocked at the haven as players' goods coming in.
    pub from_players: BTreeMap<GoodId, Amount>,
    pub chief: bool,
    /// The player who killed the most of it, for rescue reports.
    pub breaker: Option<ActorId>,
    pub fight_strength: f64,
}

impl Party {
    pub fn alive(&self) -> u32 {
        self.units.iter().map(|u| u.1).sum()
    }

    pub fn agent(id: Handle<Party>) -> u64 {
        boundary::AGENT_NAMESPACE << 56 | (id.to_bits() & MASK48)
    }

    pub fn of_agent(agent: u64) -> Option<Handle<Party>> {
        (agent >> 56 == boundary::AGENT_NAMESPACE).then(|| Handle::from_bits(agent & MASK48))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    Feud {
        faction: u32,
        actor: ActorId,
        stamp: u32,
    },
    Party {
        party: Handle<Party>,
        plan: u32,
    },
    Seen {
        party: Handle<Party>,
        player: ActorId,
        meeting: Meeting,
    },
    Unseen {
        party: Handle<Party>,
        epoch: u32,
    },
    Wave {
        party: Handle<Party>,
        plan: u32,
    },
    Ambush {
        party: Handle<Party>,
        plan: u32,
        train: u64,
    },
    Plunder {
        faction: u32,
        stamp: u32,
    },
    /// A camp that is in has gained a whole member: its slot is owed a body (`Camp::refill`).
    Refill {
        faction: u32,
        stamp: u32,
    },
}

/// Counters for the CLI, tests and the bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConflictStats {
    pub events: u64,
    pub feud_wakes: u64,
    pub stale: u64,
    pub signals: u64,
    pub emissions: u64,
    pub escalations: [u64; Step::COUNT],
    pub calmed: u64,
    pub demands: u64,
    pub tributes: u64,
    pub cuts: u64,
    pub expired: u64,
    pub gifts: u64,
    pub parties: [u64; PartyKind::COUNT],
    pub raids_district: u64,
    pub raids_village: u64,
    pub raids_watched: u64,
    pub raids_skipped: u64,
    pub ambushes: u64,
    pub casualties: u64,
    pub routs: u64,
    pub bored: u64,
    pub chiefs_killed: u64,
    pub successions: u64,
    pub seen: u64,
    pub materialized: u64,
    pub waves: u64,
    pub recalls: u64,
    pub items_from_districts: u64,
    pub items_from_villages: u64,
    pub blocks_destroyed: u64,
    pub feuds_forgotten: u64,
    /// Sources whose owner changed: the player with the most time in their chunks moved (P14).
    pub reattributed: u64,
    /// Demands withdrawn: the envoy party fell with no player among its killers (outcome 5).
    pub withdrawn: u64,
    /// War parties Peaceful held back: stood down at their muster's end, or home with nothing
    /// done; and ambushes and plunders it stopped.
    pub peace_held: u64,
    /// Notices the plugin played (NOTICE_SHOWN).
    pub notices_shown: u64,
    /// War parties sent again on the same rung, `repeat_days` after the last came home (MOWZIE.md
    /// ledger 3).
    pub repeats: u64,
    /// Camp ledgers (MOWZIE.md §4.9): censuses booked, CAMP_RECONCILEs and the lines in them
    /// sent, answers and deaths booked, and members written off after three misses.
    pub camp_censuses: u64,
    pub camp_orders: u64,
    pub camp_lines: u64,
    pub camp_answers: u64,
    pub camp_deaths: u64,
    pub camp_written_off: u64,
}

/// Where goods went, per world good (thousandths).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConflictLedger {
    pub from_villages: BTreeMap<GoodId, i128>,
    pub from_players: BTreeMap<GoodId, i128>,
    pub landed: BTreeMap<GoodId, i128>,
    pub landed_from_players: BTreeMap<GoodId, i128>,
    pub tribute: BTreeMap<GoodId, i128>,
    pub gifts: BTreeMap<GoodId, i128>,
    /// Industry goods stolen that the world's catalogue has no good for, by name (items).
    pub unmapped: BTreeMap<String, i64>,
    /// Taken from villages by a culture whose loot is destroyed where it is taken (MOWZIE.md
    /// ledger 10): part of `from_villages`, never carried.
    pub destroyed: BTreeMap<GoodId, i128>,
    /// The same for goods taken from players: part of `from_players`.
    pub destroyed_from_players: BTreeMap<GoodId, i128>,
}

fn add(m: &mut BTreeMap<GoodId, i128>, g: GoodId, a: Amount) {
    *m.entry(g).or_default() += i128::from(a);
}

/// One line of the engine's log (CLI and tests; not saved).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub day: f64,
    pub faction: String,
    pub kind: &'static str,
    pub what: String,
}

/// A signal as it comes in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Signal {
    /// The faction whose territory it is; `None` finds them by position.
    pub faction: Option<u64>,
    pub kind: SignalKind,
    pub amount: f64,
    /// A player (direct act) or the settlers (P14).
    pub actor: ActorId,
    pub pos: Pos,
    pub district: Option<DistrictId>,
}

/// A feud as the CLI and tests read it (a projection; commits nothing).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FeudView {
    pub faction: u64,
    pub actor: u64,
    pub step: Step,
    pub grievance: f64,
    pub by_kind: BTreeMap<&'static str, f64>,
    pub demand: Option<(SignalKind, i64, i64, u64)>,
    pub party: Option<PartyKind>,
}

/// World nodes by cell, rebuilt when the node count changes (nodes join rarely, never leave), so
/// a nearest-node or plunder-target search looks at the rings of cells around a place, never at
/// every node. Derived from the world: not saved.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeIndex {
    cells: BTreeMap<(u16, i32, i32), Vec<usize>>,
    known: usize,
}

const NODE_CELL: i32 = 1_024;

impl NodeIndex {
    /// Bytes held: one B-tree entry per cell (about 40) and its vector of node indexes.
    fn heap_bytes(&self) -> usize {
        self.cells.values().map(|v| 40 + v.capacity() * 8).sum()
    }

    /// Nodes only join, at the end, and never move: a refresh adds the newcomers (a faction's
    /// camp joining costs its own node, never the world's).
    fn refresh(&mut self, w: &World) {
        if w.nodes.len() == self.known {
            return;
        }
        if w.nodes.len() < self.known {
            // Another world: start again.
            self.cells.clear();
            self.known = 0;
        }
        for (k, n) in w.nodes[self.known..].iter().enumerate() {
            let i = self.known + k;
            self.cells
                .entry((
                    n.pos.frame,
                    n.pos.x.div_euclid(NODE_CELL),
                    n.pos.z.div_euclid(NODE_CELL),
                ))
                .or_default()
                .push(i);
        }
        self.known = w.nodes.len();
    }

    /// Every node within `r` blocks of `pos` (same frame), in index order: the cells the circle
    /// covers, never every node.
    fn within(&self, w: &World, t: &Topology, pos: Pos, r: f64) -> Vec<usize> {
        let mut out = Vec::new();
        for part in t.rects_near(i64::from(pos.x), i64::from(pos.z), r.ceil() as i64) {
            let (x0, z0, x1, z1) = part.grid_cells(i64::from(NODE_CELL));
            for x in x0..=x1 {
                for z in z0..=z1 {
                    for &i in self
                        .cells
                        .get(&(pos.frame, x as i32, z as i32))
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                    {
                        if tdist(t, pxz(w.nodes[i].pos), pxz(pos)) <= r {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The nearest node to `pos` within `reach` blocks that `ok` accepts (ties to the lower
    /// index), searching rings of cells outward and stopping once no nearer ring can hold a
    /// better one.
    fn nearest(
        &self,
        w: &World,
        t: &Topology,
        pos: Pos,
        reach: f64,
        mut ok: impl FnMut(usize) -> bool,
    ) -> Option<(f64, usize)> {
        let (cx, cz) = (pos.x.div_euclid(NODE_CELL), pos.z.div_euclid(NODE_CELL));
        let rings = (reach / f64::from(NODE_CELL)).ceil() as i32 + 1;
        let mut best: Option<(f64, usize)> = None;
        // On a world with seams a ring's chart cells are folded onto canonical cells, each looked
        // at once (TP8): everything within (r - 1) cells through any seam is in the chart square
        // of ring r - 1.
        let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
        for r in 0..=rings {
            // Every point of ring r is at least (r - 1) cells away.
            if let Some(b) = best
                && b.0 <= f64::from((r - 1).max(0) * NODE_CELL)
            {
                break;
            }
            let ring: Vec<(i32, i32)> = if t.is_flat() {
                let mut v = Vec::new();
                for x in cx - r..=cx + r {
                    for z in cz - r..=cz + r {
                        if (x - cx).abs() == r || (z - cz).abs() == r {
                            v.push((x, z));
                        }
                    }
                }
                v
            } else {
                let c = i64::from(NODE_CELL);
                let sq = rtsim_space::topology::Rect::new(
                    i64::from(cx - r) * c,
                    i64::from(cz - r) * c,
                    i64::from(cx + r + 1) * c,
                    i64::from(cz + r + 1) * c,
                );
                let mut v = Vec::new();
                for part in t.fold_rect(sq) {
                    let (x0, z0, x1, z1) = part.grid_cells(c);
                    for x in x0..=x1 {
                        for z in z0..=z1 {
                            if seen.insert((x as i32, z as i32)) {
                                v.push((x as i32, z as i32));
                            }
                        }
                    }
                }
                v
            };
            for (x, z) in ring {
                {
                    for &i in self
                        .cells
                        .get(&(pos.frame, x, z))
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                    {
                        let d = tdist(t, pxz(w.nodes[i].pos), pxz(pos));
                        if d > reach || !ok(i) {
                            continue;
                        }
                        if best.is_none_or(|b| d < b.0 || (d == b.0 && i < b.1)) {
                            best = Some((d, i));
                        }
                    }
                }
            }
        }
        best
    }
}

/// Where `pos` is at `at`, for the archive.
/// `t` plus `dt` ticks, held at the end of time rather than wrapping.
fn later(t: Tick, dt: u64) -> Tick {
    Tick(t.0.saturating_add(dt))
}

fn spot(at: Tick, pos: Pos) -> At {
    At::new(at.0, u32::from(pos.frame), pos.x, pos.z)
}

/// An actor's archive id: its knowledge handle, as every module records actors (a player's UUID
/// is in the world's player table).
fn aid(a: ActorId) -> u128 {
    u128::from(a.to_bits())
}

/// A party's archive id: its handle, as the viewer's parties layer names it.
fn pid(p: Handle<Party>) -> u128 {
    u128::from(p.to_bits())
}

/// War bands are named movers (VIEWER.md §10.4): their rows are notable; envoys' and scouts' minor.
fn party_severity(kind: PartyKind) -> Severity {
    if kind.is_war() {
        Severity::Notable
    } else {
        Severity::Minor
    }
}

pub struct Conflict {
    pub seed: u64,
    pub tun: ConflictTunables,
    pub cultures: Cultures,
    /// Factions by index, each with its feuds and the sources that reach it (paged rows, table
    /// 0x4310: [`crate::tables`]).
    pub factions: FactionTable,
    /// Factions by key (resident, saved in pages of its own: an index from id to row, PF10).
    pub(crate) by_key: PagedMap<u64, u32>,
    /// Factions by cell of their camp (resident, saved in pages of its own: an index from
    /// position to id, PF10). A set: the values are empty.
    pub(crate) cells: PagedMap<((u16, i32, i32), u32), ()>,
    /// Feuds in every faction's row.
    pub(crate) feud_count: u64,
    /// Continuous sources (paged rows, table 0x4312, through a resident index by key).
    pub sources: SourceTable,
    /// Which sources are where, by chunk (paged rows, table 0x4313).
    pub(crate) places: PlaceIndex,
    /// The page store the row tables page to, when paged.
    pub(crate) store: Option<std::sync::Arc<dyn rtsim_kernel::rows::PageSource>>,
    /// Pages loaded when a budget check last ended over budget with nothing it could evict:
    /// until a load or a save makes a page evictable, another check would find nothing either.
    pub(crate) budget_mark: Option<u64>,
    /// Parties in flight (resident, saved in pages of their own, PF10).
    pub parties: ResidentArena<Party>,
    pub(crate) wheel: PagedWheel<Ev>,
    /// Players seen through the interest areas: their UUIDs.
    pub(crate) players: BTreeMap<ActorId, Uuid>,
    pub(crate) applied_ops: BTreeSet<u64>,
    pub(crate) settlers: ActorId,
    /// Java's dictionary (kind, id) to name, for observations.
    pub(crate) java_dict: BTreeMap<(u8, u32), String>,
    pub ledger: ConflictLedger,
    pub stats: ConflictStats,
    pub(crate) outbox: Vec<Cmd>,
    pub(crate) now: Tick,
    pub log: Vec<LogEntry>,
    pub log_enabled: bool,
    /// The event archive's recorder (VIEWER.md §10.1): off until the host turns it on; not state.
    pub tape: Tape,
    pub(crate) nodes: NodeIndex,
    /// Time spent per chunk (P14): whom an unowned source counts against.
    pub presence: Presence,
    /// Every frame's topology (TP6): flat until the engine host hands over the TOPOLOGY records'.
    pub topo: SharedTopologies,
    /// The world is on Peaceful (DIFFICULTY, MOWZIE.md §4.12): no war party leaves, and ambushes,
    /// raids and plunder do nothing. Saved: the plugin reports it again at each session start.
    pub peaceful: bool,
    /// When the world last became Peaceful: the held feuds' dwell clocks pause from here (MZB-16).
    pub peaceful_since: Tick,
    /// The last notice's serial (from 1, one per notice, engine-wide; FACTION_NOTICE carries it
    /// at 60): the NOTICE_SHOWN that echoes it answers that notice and no other, a feud forgotten
    /// and made again included (MZB-10).
    pub notice_serial: u32,
    /// The record being ingested is a replay of an earlier session's journal, not state of this
    /// one: a CAMP_CENSUS then only marks its camp out (MOWZIE.md §4.9). Set by the host around
    /// the records it replays; not state.
    pub replaying: bool,
    /// The journal sequence of the record being ingested (0 where the host has none, as in the
    /// simulations, whose answers are then kept to the end of their epoch): a CAMP_BODIES answer is
    /// kept with it. Set by the host; not state.
    pub obs_seq: u64,
    /// The highest journal sequence the host has reported durable in a world save (WORLD_SAVED):
    /// answers booked from records at or before it are forgotten, as `Replay` forgets op ids
    /// (docs/boundary.md §2.3, MZB-177). Set by the host, which never lowers it; not state.
    pub durable_obs_seq: u64,
}

fn dist(a: (i64, i64), b: (i64, i64)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
}

/// The shortest distance through every seam (TP2); on the plane the straight line, bit for bit.
fn tdist(t: &Topology, a: (i64, i64), b: (i64, i64)) -> f64 {
    if t.is_flat() {
        return dist(a, b);
    }
    t.dist(a, b)
}

fn pxz(p: Pos) -> (i64, i64) {
    (i64::from(p.x), i64::from(p.z))
}

/// Every body kind of every culture, in a fixed order: the engine's body dictionary (ids from 1).
pub fn body_names(c: &Cultures) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for cu in &c.list {
        for u in &cu.def.units {
            if !v.contains(&u.body) {
                v.push(u.body.clone());
            }
        }
    }
    v
}

pub fn body_id(c: &Cultures, name: &str) -> u32 {
    body_names(c)
        .iter()
        .position(|n| n == name)
        .map_or(0, |i| i as u32 + 1)
}

/// The engine's item dictionary: a world good's id + 1 (named by the catalogue).
pub fn item_of(g: GoodId) -> u32 {
    u32::from(g.0) + 1
}

impl Conflict {
    pub fn new(seed: u64, tun: ConflictTunables, cultures: Cultures, now: Tick) -> Conflict {
        let presence = Presence::new(
            tun.presence_half_life_ticks,
            tun.presence_top,
            tun.presence_chunks,
            tun.presence_max_gap,
        );
        Conflict {
            seed,
            tun,
            cultures,
            factions: FactionTable::default(),
            by_key: PagedMap::new(),
            cells: PagedMap::new(),
            feud_count: 0,
            sources: SourceTable::default(),
            places: PlaceIndex::default(),
            store: None,
            budget_mark: None,
            parties: ResidentArena::new(),
            wheel: PagedWheel::new(now),
            players: BTreeMap::new(),
            applied_ops: BTreeSet::new(),
            settlers: Handle::NULL,
            java_dict: BTreeMap::new(),
            ledger: ConflictLedger::default(),
            stats: ConflictStats::default(),
            outbox: Vec::new(),
            now,
            log: Vec::new(),
            log_enabled: false,
            tape: Tape::new(),
            nodes: NodeIndex::default(),
            presence,
            topo: rtsim_space::topology::all_flat(),
            peaceful: false,
            peaceful_since: Tick::ZERO,
            notice_serial: 0,
            replaying: false,
            obs_seq: 0,
            durable_obs_seq: 0,
        }
    }

    /// Hands the engine every frame's topology (TP6).
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) {
            return;
        }
        self.topo = t;
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.topo.of(frame)
    }

    /// The canonical position of a block the plugin reported (TP4).
    fn canon(&self, dimension: u32, x: i32, z: i32) -> Pos {
        let frame = dimension as u16;
        let (x, z) = self.topo(frame).canonical_i32(x, z);
        Pos::new(frame, x, z)
    }

    /// A party's position on its route at `t`, in the route's chart.
    fn party_chart_pos(&self, p: Handle<Party>, t: Tick) -> (i32, i32) {
        let x = &self.parties[p];
        match x.phase {
            Phase::Mustering => x.route[0],
            Phase::Waiting | Phase::Fighting => *x.route.last().expect("a route"),
            Phase::Out | Phase::Home => {
                let s = (x.seg as usize).min(x.route.len().saturating_sub(2));
                let (a, b) = (x.route[s], x.route[(s + 1).min(x.route.len() - 1)]);
                let (t0, t1) = (x.times[s], x.times[(s + 1).min(x.times.len() - 1)]);
                if t1 <= t0 || t <= t0 {
                    return a;
                }
                if t >= t1 {
                    return b;
                }
                let fr = (t.0 - t0.0) as f64 / (t1.0 - t0.0) as f64;
                (
                    (f64::from(a.0) + (f64::from(b.0) - f64::from(a.0)) * fr) as i32,
                    (f64::from(a.1) + (f64::from(b.1) - f64::from(a.1)) * fr) as i32,
                )
            }
        }
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    /// The census (EQUILIBRIUM.md S1) now: `conflict.camp`, K its culture's pool cap, its members
    /// the whole warriors in its pool. No pressure, no break and no booked recruit yet (slice S7):
    /// P reads 0 and renewals 0.
    pub fn census(&self) -> Vec<rtsim_kernel::census::KindCensus> {
        let mut c = rtsim_kernel::census::KindCensus::new("conflict.camp");
        for f in 0..self.factions.len() as u32 {
            let pool = self.pool_at(f, self.now).max(0.0).floor() as u64;
            c.site(pool, u64::from(self.factions[f as usize].cap), false, 0.0);
        }
        vec![c]
    }

    pub fn next_due(&self) -> Option<Tick> {
        self.wheel.next_due()
    }

    pub fn pending_events(&self) -> usize {
        self.wheel.len()
    }

    /// Pending events by kind (diagnostics and the bench).
    pub fn pending_by_kind(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for (_, e) in self.wheel.iter() {
            let k = match e {
                Ev::Feud { .. } => "feud",
                Ev::Party { .. } => "party",
                Ev::Seen { .. } => "seen",
                Ev::Unseen { .. } => "unseen",
                Ev::Wave { .. } => "wave",
                Ev::Ambush { .. } => "ambush",
                Ev::Plunder { .. } => "plunder",
                Ev::Refill { .. } => "refill",
            };
            *m.entry(k).or_default() += 1;
        }
        m
    }

    pub fn take_commands(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.outbox)
    }

    /// A name in Java's dictionary (kind 2 items, 3 bodies, 4 structures, 0x20 pollution types).
    pub fn define(&mut self, kind: u8, id: u32, name: &str) {
        self.java_dict.insert((kind, id), name.to_string());
    }

    fn schedule(&mut self, at: Tick, ev: Ev) {
        self.wheel.schedule(at.max(self.wheel.now()), ev);
    }

    fn note(&mut self, at: Tick, f: Option<u32>, kind: &'static str, what: String) {
        if !self.log_enabled {
            return;
        }
        let faction = f.map_or(String::new(), |f| self.faction_name(f));
        self.log.push(LogEntry {
            tick: at.0,
            day: at.0 as f64 / TICKS_PER_DAY as f64,
            faction,
            kind,
            what,
        });
    }

    /// A faction's archive id: its key, as the plugin knows it.
    fn fid(&self, f: u32) -> u128 {
        u128::from(self.factions[f as usize].key)
    }

    pub fn faction_name(&self, f: u32) -> String {
        let x = &self.factions[f as usize];
        format!("{} camp {}", self.cultures.get(x.culture).name(), x.key)
    }

    pub fn faction_of_key(&self, key: u64) -> Option<u32> {
        self.by_key.get(&key).copied()
    }

    fn culture(&self, f: u32) -> &Culture {
        self.cultures.get(self.factions[f as usize].culture)
    }

    fn temper(&self, f: u32) -> Temperament {
        Temperament::from_seed(self.factions[f as usize].chief_seed)
    }

    /// The settlers actor (P14), registered in the world's knowledge on first use.
    pub fn settlers(&mut self, w: &mut World) -> ActorId {
        if self.settlers.is_null() {
            self.settlers = w
                .knowledge
                .actors_mut()
                .register(ActorKind::Group, SETTLERS_KEY);
        }
        self.settlers
    }

    /// A player by UUID (idempotent).
    pub fn player(&mut self, w: &mut World, uuid: Uuid) -> ActorId {
        let p = w.player_uuid(uuid.0, uuid.1);
        self.players.insert(p, uuid);
        p
    }

    fn uuid_of(&self, w: &World, a: ActorId) -> Uuid {
        if a == self.settlers {
            return (0, 0);
        }
        self.players
            .get(&a)
            .copied()
            .or_else(|| w.players.get(&a).map(|p| p.uuid))
            .unwrap_or((0, 0))
    }

    fn actor_of_uuid(&mut self, w: &mut World, u: Uuid) -> ActorId {
        if u == (0, 0) {
            self.settlers(w)
        } else {
            self.player(w, u)
        }
    }

    // ---- factions -------------------------------------------------------------------------

    /// A faction's camp: joins the world as a haven node (C24), or as a lair for a culture with no
    /// haven (MOWZIE.md ledger 13), if no node has its key, registers
    /// the faction, its chief and heir, books its plunder drive, and tells the plugin its
    /// territory. Idempotent by key. `None` for an unknown culture.
    pub fn add_faction(
        &mut self,
        w: &mut World,
        key: u64,
        culture: &str,
        pos: Pos,
        at: Tick,
    ) -> Option<u32> {
        if let Some(&f) = self.by_key.get(&key) {
            return Some(f);
        }
        let ci = self.cultures.index_of(culture)?;
        self.run_to(at);
        let cu = self.cultures.get(ci).clone();
        let node = match w.node_of_key(key) {
            Some(i) => i,
            None => w.join(
                NodeData {
                    key,
                    kind: if cu.def.haven {
                        NodeKind::Haven(cu.market)
                    } else {
                        NodeKind::Lair
                    },
                    pos,
                    pop: (cu.def.pool_cap / 4).max(1),
                    resources: BTreeMap::new(),
                },
                at,
            ),
        };
        let actor = w.knowledge.actors_mut().register(ActorKind::Group, key);
        let home = w.nodes[node].actor;
        let mut x = Faction {
            key,
            culture: ci,
            node,
            pos,
            radius: cu.def.territory_radius,
            actor,
            chief: Handle::NULL,
            chief_seed: 0,
            heir: Handle::NULL,
            heir_seed: 0,
            chiefs: 0,
            pool_at: at,
            pool: f64::from(cu.def.pool_cap),
            cowed_until: Tick::ZERO,
            plunders: 0,
            plunder_stamp: 0,
            observed: Vec::new(),
            chief_out: false,
            crystal: true,
            cap: cu.def.pool_cap,
            away: 0,
            camp: None,
        };
        let lifespan = cu.def.chief_lifespan_days * TICKS_PER_DAY;
        let seed_of = |n: u32| StreamKey::new(self.seed, SYS_CHIEF, key).block(u64::from(n), 0)[0];
        x.heir_seed = seed_of(1);
        x.heir = w
            .knowledge
            .create_notable(x.heir_seed, home, home, at, lifespan * 2);
        x.chief_seed = seed_of(0);
        x.chief = w
            .knowledge
            .create_notable(x.chief_seed, home, x.heir, at, lifespan);
        x.chiefs = 2;
        let f = self.factions.push(x);
        self.by_key.insert(key, f);
        let cell = self.cell_of(pos);
        self.cells.insert((cell, f), ());
        let t = self.territory(f);
        self.outbox.push(t);
        self.schedule_plunder(f, at);
        // Sources already emitting reach the newcomer (only the newcomer's feud changes). The
        // place index's regions around the camp hold every source whose position is in reach.
        let reach = f64::from(cu.def.territory_radius) + f64::from(cu.def.reach_blocks);
        let topo = self.topo.clone();
        let t = topo.of(pos.frame);
        let mut ids = Vec::new();
        for part in t.rects_near(i64::from(pos.x), i64::from(pos.z), reach.ceil() as i64) {
            let (x0, z0, x1, z1) = part.grid_cells(16);
            ids.extend(self.places.in_rect(pos.frame, x0, z0, x1, z1));
        }
        ids.sort_unstable();
        ids.dedup();
        let mut near: Vec<((u64, SignalKind), u32, f64)> = ids
            .into_iter()
            .filter_map(|id| {
                let row = self.sources.by_id(id)?;
                let s = &row.s;
                let d = tdist(t, pxz(s.pos), pxz(pos));
                (s.pos.frame == pos.frame && d <= reach).then(|| {
                    let r = f64::from(cu.def.territory_radius);
                    let fall = if d <= r {
                        1.0
                    } else {
                        (1.0 - (d - r) / f64::from(cu.def.reach_blocks.max(1))).max(0.0)
                    };
                    ((row.key, row.kind), id, fall)
                })
            })
            .filter(|e| e.2 > 0.0)
            .collect();
        near.sort_by_key(|e| e.0);
        if !near.is_empty() {
            // Each source counts against its own actor (P14): one refresh per (actor, kind).
            let mut feeds: Vec<(ActorId, SignalKind)> = Vec::new();
            for (k, id, fall) in near {
                if let Some(row) = self.sources.by_id_mut(id) {
                    let s = &mut row.s;
                    s.affects.push((f, fall));
                    s.affects.sort_by_key(|e| e.0);
                    if !feeds.contains(&(s.actor, k.1)) {
                        feeds.push((s.actor, k.1));
                    }
                }
                self.factions.row_mut(f).add_reach(k.1, id);
            }
            for (a, kind) in feeds {
                self.refresh_rates(f, a, kind, at);
            }
        }
        let (fid, pool) = (self.fid(f), self.factions[f as usize].pool as i64);
        self.tape
            .event(kinds::CONFLICT_FACTION, spot(at, pos), fid, 0, pool);
        self.note(
            at,
            Some(f),
            "faction",
            format!(
                "a {} camp at ({}, {}), territory {} blocks",
                cu.name(),
                pos.x,
                pos.z,
                cu.def.territory_radius
            ),
        );
        Some(f)
    }

    /// A faction's TERRITORY command: its circle and the signals its culture minds.
    fn territory(&self, f: u32) -> Cmd {
        let x = &self.factions[f as usize];
        let cu = self.culture(f);
        let mask = SignalKind::ALL
            .into_iter()
            .filter(|k| cu.signals[k.index()].weight > 0.0)
            .fold(0, |m, k| m | k.bit());
        Cmd::Territory(boundary::Territory {
            faction: x.key,
            flags: 0,
            dimension: u32::from(x.pos.frame),
            signals: mask,
            x: x.pos.x,
            z: x.pos.z,
            radius: x.radius,
        })
    }

    fn cell_of(&self, p: Pos) -> (u16, i32, i32) {
        let c = self.tun.cell_blocks;
        (p.frame, p.x.div_euclid(c), p.z.div_euclid(c))
    }

    /// Factions whose territory, widened by `extra` blocks (and their own reach when `reach`),
    /// covers `pos`: (faction, falloff in 0..=1). Looks up the cells around, never every
    /// faction.
    fn factions_near(&self, pos: Pos, reach: bool) -> Vec<(u32, f64)> {
        let far = self
            .cultures
            .list
            .iter()
            .map(|c| c.def.territory_radius + if reach { c.def.reach_blocks } else { 0 })
            .max()
            .unwrap_or(0) as i32;
        let c = self.tun.cell_blocks;
        let t = self.topo.of(pos.frame);
        let mut cells: Vec<(i32, i32)> = Vec::new();
        for part in t.rects_near(i64::from(pos.x), i64::from(pos.z), i64::from(far)) {
            let (x0, z0, x1, z1) = part.grid_cells(i64::from(c));
            for cx in x0..=x1 {
                for cz in z0..=z1 {
                    cells.push((cx as i32, cz as i32));
                }
            }
        }
        cells.sort_unstable();
        cells.dedup();
        let mut out = Vec::new();
        for (cx, cz) in cells {
            let c = (pos.frame, cx, cz);
            {
                for (&(_, f), ()) in self.cells.range((c, 0)..=(c, u32::MAX)) {
                    let x = &self.factions[f as usize];
                    let d = tdist(t, pxz(x.pos), pxz(pos));
                    let r = f64::from(x.radius);
                    let fall = if d <= r {
                        1.0
                    } else if reach {
                        let extra = f64::from(self.culture(f).def.reach_blocks);
                        if extra <= 0.0 {
                            0.0
                        } else {
                            (1.0 - (d - r) / extra).max(0.0)
                        }
                    } else {
                        0.0
                    };
                    if fall > 0.0 {
                        out.push((f, fall));
                    }
                }
            }
        }
        out.sort_by_key(|e| e.0);
        out
    }

    /// The faction's warriors at `t` (closed form): recruited back at the culture's rate to the
    /// camp's cap less the members away (MOWZIE.md ledger 6), so a band's survivors always have
    /// their slots to come home to. This holds for every culture, with a ledger or without (a
    /// camp's cap is its culture's `pool_cap` unless a census says fewer), and a pool above that
    /// room is clamped to it.
    pub fn pool_at(&self, f: u32, t: Tick) -> f64 {
        let x = &self.factions[f as usize];
        let c = self.culture(f);
        let rate = c.def.recruits_per_day / TICKS_PER_DAY as f64;
        let room = f64::from(x.cap.saturating_sub(x.away));
        (x.pool + rate * t.0.saturating_sub(x.pool_at.0) as f64)
            .min(room)
            .max(x.pool.min(room))
    }

    /// The pool changes by `n` at `at`. A camp with a ledger first adds the slots the pool gained
    /// since it was last read (I2), so that every change of the pool moves a slot with it.
    fn pool_add(&mut self, f: u32, n: f64, at: Tick) {
        self.camp_sync(f, at);
        let v = self.pool_at(f, at);
        let x = &mut self.factions[f as usize];
        let room = f64::from(x.cap.saturating_sub(x.away));
        x.pool = (v + n).clamp(0.0, room);
        x.pool_at = at;
    }

    /// When the pool will hold `n` warriors: [`BLOCKED`] when the camp's cap less its members
    /// away holds fewer, or it recruits none, so that only a change of the room can end the wait
    /// (`unblock`).
    fn pool_ready(&self, f: u32, n: f64, at: Tick) -> Tick {
        let x = &self.factions[f as usize];
        if n > f64::from(x.cap.saturating_sub(x.away)) {
            return BLOCKED;
        }
        let have = self.pool_at(f, at);
        if have >= n {
            return at;
        }
        let rate = self.culture(f).def.recruits_per_day / TICKS_PER_DAY as f64;
        if rate <= 0.0 {
            return BLOCKED;
        }
        later(at, (((n - have) / rate).ceil() as u64).saturating_add(1))
    }

    /// The chief, after a death of age the world's knowledge processed (the heir takes over).
    fn ensure_chief(&mut self, w: &mut World, f: u32, at: Tick) {
        let chief = self.factions[f as usize].chief;
        if w.knowledge.notable(chief).is_some() {
            return;
        }
        self.succeed(w, f, at, None);
    }

    /// The heir becomes chief (C24: it holds the grudges it inherited) and a new heir is named.
    fn succeed(&mut self, w: &mut World, f: u32, at: Tick, killer: Option<ActorId>) {
        let key = self.factions[f as usize].key;
        let node = self.factions[f as usize].node;
        let home = w.nodes[node].actor;
        let lifespan = self.culture(f).def.chief_lifespan_days * TICKS_PER_DAY;
        let (heir, heir_seed) = {
            let x = &self.factions[f as usize];
            (x.heir, x.heir_seed)
        };
        let n = self.factions[f as usize].chiefs;
        let seed_of = |n: u32| StreamKey::new(self.seed, SYS_CHIEF, key).block(u64::from(n), 0)[0];
        let new_heir_seed = seed_of(n);
        let new_heir = w
            .knowledge
            .create_notable(new_heir_seed, home, home, at, lifespan * 2);
        let (chief, chief_seed) = if w.knowledge.notable(heir).is_some() {
            w.knowledge.set_heir(heir, new_heir);
            (heir, heir_seed)
        } else {
            let s = seed_of(n + 1);
            let c = w.knowledge.create_notable(s, home, new_heir, at, lifespan);
            (c, s)
        };
        {
            let x = &mut self.factions[f as usize];
            x.chief = chief;
            x.chief_seed = chief_seed;
            x.heir = new_heir;
            x.heir_seed = new_heir_seed;
            x.chiefs = n + 2;
            x.chief_out = false;
        }
        self.stats.successions += 1;
        // The grudges the new chief inherited become grievance against those players (C24).
        let feelings = w.knowledge.sentiment_snapshot(chief, at);
        let lambda = self.culture(f).lambda;
        let factor = self.tun.heir_grudge_factor;
        let mut grudges = Vec::new();
        for (target, v) in feelings {
            if v < 0 && w.knowledge.actors().is_player(target) {
                grudges.push((target, f64::from(-i32::from(v)) * factor));
            }
        }
        for &(p, g) in &grudges {
            let feud = self.feud_entry(f, p, at);
            feud.g.add(SignalKind::Killing, g, at, lambda);
        }
        let what = match killer {
            Some(_) => {
                "its war chief is killed; the heir takes over, holding the grudge".to_string()
            }
            None => "its chief dies of age; the heir takes over".to_string(),
        };
        let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
        self.tape
            .event(
                kinds::CONFLICT_HEIR,
                spot(at, camp),
                fid,
                killer.map_or(0, aid),
                grudges.len() as i64,
            )
            .text(|| format!("{what} ({} grudges)", grudges.len()));
        self.note(
            at,
            Some(f),
            "heir",
            format!("{what} ({} grudges)", grudges.len()),
        );
        // Thresholds follow the new chief's temperament: every feud of the faction replans.
        for a in self.feud_actors(f) {
            self.plan_feud(f, a, at, false);
        }
    }

    // ---- signals --------------------------------------------------------------------------

    /// One faction's feud against one actor (in the faction's row, PM-C2).
    pub fn feud(&self, f: u32, a: ActorId) -> Option<&Feud> {
        let r = self.factions.rows.get(u64::from(f))?;
        r.feud_at(a).ok().map(|i| &r.feuds[i].1)
    }

    /// The feud to change. A miss changes nothing (the faction's page stays clean).
    pub fn feud_mut(&mut self, f: u32, a: ActorId) -> Option<&mut Feud> {
        self.feud(f, a)?;
        let r = self.factions.rows.get_mut(u64::from(f))?;
        let i = r.feud_at(a).ok()?;
        Some(&mut r.feuds[i].1)
    }

    /// Every feud of one faction, by actor.
    pub fn feuds_of(&self, f: u32) -> &[(ActorId, Feud)] {
        self.factions
            .rows
            .get(u64::from(f))
            .map_or(&[], |r| r.feuds.as_slice())
    }

    /// Feuds held by every faction (resident count).
    pub fn feud_count(&self) -> u64 {
        self.feud_count
    }

    fn feud_actors(&self, f: u32) -> Vec<ActorId> {
        self.feuds_of(f).iter().map(|e| e.0).collect()
    }

    /// The feud, begun now if there was none.
    fn feud_entry(&mut self, f: u32, a: ActorId, at: Tick) -> &mut Feud {
        let r = self.factions.row_mut(f);
        let i = match r.feud_at(a) {
            Ok(i) => i,
            Err(i) => {
                r.feuds.insert(i, (a, Feud::new(at)));
                self.feud_count += 1;
                i
            }
        };
        &mut r.feuds[i].1
    }

    fn remove_feud(&mut self, f: u32, a: ActorId) {
        if self.feud(f, a).is_none() {
            return;
        }
        let r = self.factions.row_mut(f);
        if let Ok(i) = r.feud_at(a) {
            r.feuds.remove(i);
            self.feud_count -= 1;
        }
    }

    /// A direct act or an observed impulse (a block broken in a territory, ore drained, a member
    /// killed): grievance jumps by its weight in every faction whose territory it is in.
    pub fn signal(&mut self, w: &mut World, s: Signal, at: Tick) {
        let _ = w;
        self.run_to(at);
        self.stats.signals += 1;
        let pollution = matches!(s.kind, SignalKind::Smog | SignalKind::Radiation);
        let targets: Vec<(u32, f64)> = match s.faction.and_then(|k| self.by_key.get(&k)) {
            Some(&f) => vec![(f, 1.0)],
            None => self.factions_near(s.pos, pollution),
        };
        for (f, fall) in targets {
            let c = self.culture(f);
            let wgt = c.signals[s.kind.index()].weight;
            if wgt <= 0.0 {
                continue;
            }
            let lambda = c.lambda;
            let amount = s.amount * wgt * fall;
            let feud = self.feud_entry(f, s.actor, at);
            feud.g.add(s.kind, amount, at, lambda);
            if let Some(d) = feud.demand.as_mut()
                && d.kind == s.kind
            {
                d.after += s.amount;
            }
            let better = feud
                .focus
                .is_none_or(|fo| fo.district.is_none() || s.district.is_some());
            if better {
                feud.focus = Some(Focus {
                    pos: s.pos,
                    district: s.district.or(feud.focus.and_then(|fo| fo.district)),
                    weight: amount,
                });
            }
            self.plan_feud(f, s.actor, at, false);
        }
    }

    /// A continuous source's rate: a district's emissions while unloaded (the industry engine's
    /// learned rate of a pollution good) or a loaded chunk's (POLLUTION records). Units per day; 0
    /// stops it. It has no direct actor, so it counts against the player who has spent the most
    /// time in the chunk of `pos` (P14, changed), or the settlers while nobody has.
    #[allow(clippy::too_many_arguments)]
    pub fn set_emission(
        &mut self,
        w: &mut World,
        key: u64,
        kind: SignalKind,
        pos: Pos,
        district: Option<DistrictId>,
        per_day: f64,
        at: Tick,
    ) {
        self.set_emission_in(w, key, kind, pos, district, &[], per_day, at);
    }

    /// [`Conflict::set_emission`] for a source that spans `chunks` (an industry district's): it
    /// counts against the player with the most time in them together. No chunks: the chunk of
    /// `pos`.
    #[allow(clippy::too_many_arguments)]
    pub fn set_emission_in(
        &mut self,
        w: &mut World,
        key: u64,
        kind: SignalKind,
        pos: Pos,
        district: Option<DistrictId>,
        chunks: &[ChunkKey],
        per_day: f64,
        at: Tick,
    ) {
        self.run_to(at);
        self.stats.emissions += 1;
        let mut chunks = if chunks.is_empty() {
            vec![chunk_of(pos)]
        } else {
            chunks.to_vec()
        };
        chunks.sort();
        chunks.dedup();
        self.set_emission_inner(
            w,
            (key, kind),
            pos,
            district,
            chunks,
            per_day.max(0.0) / TICKS_PER_DAY as f64,
            at,
        );
    }

    /// Whom a source counts against now (P14): the player with the most time in its chunks, or the
    /// settlers.
    fn owner_of(&mut self, w: &mut World, chunks: &[ChunkKey], at: Tick) -> ActorId {
        match self.presence.top_of(chunks, at) {
            Some(p) => p,
            None => self.settlers(w),
        }
    }

    /// The chunks a source is filed under in the place index: its own chunks and its position's
    /// (canonical, so a new camp's search around itself finds it), sorted.
    fn places_of(&self, chunks: &[ChunkKey], pos: Pos) -> Vec<ChunkKey> {
        let (x, z) = self.topo(pos.frame).canonical_i32(pos.x, pos.z);
        let mut v = chunks.to_vec();
        v.push(chunk_of(Pos::new(pos.frame, x, z)));
        v.sort_unstable();
        v.dedup();
        v
    }

    #[allow(clippy::too_many_arguments)]
    fn set_emission_inner(
        &mut self,
        w: &mut World,
        key: (u64, SignalKind),
        pos: Pos,
        district: Option<DistrictId>,
        chunks: Vec<ChunkKey>,
        rate: f64,
        at: Tick,
    ) {
        let actor = self.owner_of(w, &chunks, at);
        let old_id = self.sources.id_of(&key);
        let (old, old_actor, old_places) = match self.sources.get(&key) {
            Some(s) => (
                s.affects.iter().map(|a| a.0).collect::<Vec<u32>>(),
                Some(s.actor),
                self.places_of(&s.chunks, s.pos),
            ),
            None => (Vec::new(), None, Vec::new()),
        };
        let affects = if rate > 0.0 {
            self.factions_near(pos, true)
        } else {
            Vec::new()
        };
        let mut touched: Vec<u32> = old.clone();
        touched.extend(affects.iter().map(|a| a.0));
        touched.sort();
        touched.dedup();
        let new_places = if rate > 0.0 {
            self.places_of(&chunks, pos)
        } else {
            Vec::new()
        };
        if rate > 0.0 {
            let id = self.sources.insert(
                key,
                Source {
                    pos,
                    district,
                    rate,
                    affects: affects.clone(),
                    actor,
                    chunks,
                },
            );
            // Only the factions and places that changed are touched (their pages dirtied).
            let kept = old_id == Some(id);
            for f in &old {
                if (!kept || !affects.iter().any(|a| a.0 == *f))
                    && let Some(o) = old_id
                {
                    self.factions.row_mut(*f).remove_reach(key.1, o);
                }
            }
            for (f, _) in &affects {
                if !kept || !old.contains(f) {
                    self.factions.row_mut(*f).add_reach(key.1, id);
                }
            }
            for c in &old_places {
                if (!kept || new_places.binary_search(c).is_err())
                    && let Some(o) = old_id
                {
                    self.places.remove(*c, o);
                }
            }
            for c in &new_places {
                if !kept || old_places.binary_search(c).is_err() {
                    self.places.add(*c, id);
                }
            }
        } else if let Some((o, _)) = self.sources.remove(&key) {
            for f in &old {
                self.factions.row_mut(*f).remove_reach(key.1, o);
            }
            for c in &old_places {
                self.places.remove(*c, o);
            }
        }
        for f in touched {
            self.refresh_rates(f, actor, key.1, at);
            if let Some(o) = old_actor.filter(|o| *o != actor) {
                self.refresh_rates(f, o, key.1, at);
            }
        }
    }

    /// A player was seen at `pos` (P14's time-spent counter). When the player who has spent the
    /// most time in a chunk changes, the sources there count against the new one from now on.
    /// The shared interest service calls this for every PLAYER report (rtsim-jni `shared.rs`);
    /// [`Conflict::observe_player`] does too.
    pub fn observe_presence(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) {
        let p = self.player(w, uuid);
        if let Some(chunk) = self.presence.observe(p, pos, at) {
            self.reattribute(w, chunk, at);
        }
    }

    /// The sources in `chunk` follow its new leading player.
    fn reattribute(&mut self, w: &mut World, chunk: ChunkKey, at: Tick) {
        // The place index holds each source under its chunks and its position's chunk: only
        // those with the chunk among their own count here, in key order.
        let mut keys: Vec<((u64, SignalKind), u32)> = self
            .places
            .at(chunk)
            .into_iter()
            .filter_map(|id| {
                let r = self.sources.by_id(id)?;
                r.s.chunks
                    .binary_search(&chunk)
                    .is_ok()
                    .then_some(((r.key, r.kind), id))
            })
            .collect();
        if keys.is_empty() {
            return;
        }
        keys.sort_unstable_by_key(|e| e.0);
        self.run_to(at);
        for (key, id) in keys {
            let Some(r) = self.sources.by_id(id) else {
                continue;
            };
            let chunks = r.s.chunks.clone();
            let old = r.s.actor;
            let factions: Vec<u32> = r.s.affects.iter().map(|a| a.0).collect();
            let new = self.owner_of(w, &chunks, at);
            if new == old {
                continue;
            }
            self.sources.by_id_mut(id).expect("listed").s.actor = new;
            self.stats.reattributed += 1;
            for f in factions {
                self.refresh_rates(f, old, key.1, at);
                self.refresh_rates(f, new, key.1, at);
            }
        }
    }

    /// Recomputes a feud's raw rate of one kind from the sources that count against its actor
    /// (exact, no drift) and its focus (the strongest source with a district).
    fn refresh_rates(&mut self, f: u32, a: ActorId, kind: SignalKind, at: Tick) {
        let mut raw = 0.0;
        let mut best: Option<Focus> = None;
        // Only the sources that reach this faction (its row's list), in id order.
        for id in self.factions.row(f).reach_of(kind) {
            let Some(s) = self.sources.by_id(id).map(|r| &r.s) else {
                continue;
            };
            if s.actor != a {
                continue;
            }
            if let Some(&(_, fall)) = s.affects.iter().find(|x| x.0 == f) {
                raw += s.rate * fall;
                let wgt = s.rate * fall;
                if best.is_none_or(|b| wgt > b.weight) {
                    best = Some(Focus {
                        pos: s.pos,
                        district: s.district,
                        weight: wgt,
                    });
                }
            }
        }
        let c = self.culture(f);
        let sd = c.signals[kind.index()];
        let lambda = c.lambda;
        let eff = (raw - sd.tolerance_per_day / TICKS_PER_DAY as f64).max(0.0) * sd.weight;
        if raw <= 0.0 && self.feud(f, a).is_none() {
            return;
        }
        let feud = self.feud_entry(f, a, at);
        feud.raw[kind.index()] = raw;
        feud.g.set_rate(kind, eff, at, lambda);
        if let Some(b) = best
            && b.district.is_some()
        {
            feud.focus = Some(b);
        }
        self.plan_feud(f, a, at, false);
    }

    // ---- the ladder -------------------------------------------------------------------------

    /// A rung's threshold for this faction's chief: an aggressive chief climbs sooner (C25).
    fn theta(&self, f: u32, s: Step) -> f64 {
        let base = self.culture(f).threshold(s);
        let bend = self.temper(f).bend(Trait::Aggressive, 1000, 40).max(1);
        base * 1000.0 / f64::from(bend)
    }

    fn down_theta(&self, f: u32, s: Step) -> f64 {
        self.culture(f).def.hysteresis * self.theta(f, s)
    }

    /// The earliest tick a feud on Raid or Invasion with no party out and no demand may send its
    /// rung's party again (MOWZIE.md ledger 3, MZB-51): `repeat_days` after the later of the
    /// rung's start and the arrival home of any war party of this feud still on its way home, and
    /// not while cowed or waiting for the pool. `None` for a culture that does not repeat, a feud
    /// that cannot repeat now, or a rung whose party is empty (MZB-64).
    fn repeat_after(&self, f: u32, a: ActorId, feud: &Feud) -> Option<Tick> {
        let c = self.culture(f);
        let days = c.def.repeat_days?;
        // On Peaceful no war party leaves; leaving it plans every feud again (`set_difficulty`).
        if self.peaceful
            || !matches!(feud.step, Step::Raid | Step::Invasion)
            || !c.has_rung(feud.step)
            || feud.party.is_some()
            || feud.demand.is_some()
        {
            return None;
        }
        // The pool refills while a party walks home: counting from its turn could field a second
        // band (a second Frostmaw) before the first is back.
        let back = self
            .parties
            .iter()
            .filter(|(_, x)| {
                x.faction == f && x.feud == Some(a) && x.phase == Phase::Home && x.kind.is_war()
            })
            .filter_map(|(_, x)| x.times.last().copied())
            .max();
        let from = back.map_or(feud.since, |b| b.max(feud.since));
        Some(
            later(from, days_to_ticks(days))
                .max(self.factions[f as usize].cowed_until)
                .max(feud.wait_until),
        )
    }

    /// When the feud repeats: at [`Self::repeat_after`], once the grievance is at the rung's
    /// threshold.
    fn repeat_at(&self, f: u32, a: ActorId, feud: &Feud) -> Option<Tick> {
        let after = self.repeat_after(f, a, feud)?;
        let lambda = self.culture(f).lambda;
        feud.g
            .crossing_up(self.theta(f, feud.step), lambda)
            .map(|t| t.max(after))
    }

    /// Books the feud's next event: the earliest of its next rung (after the dwell), its rung's
    /// repeat, its fall below the current rung, its demand's deadline. A new stamp makes the older
    /// booking stale. Empty rungs are skipped (MOWZIE.md ledger 2): a climb's threshold is the
    /// rung it goes to, its dwell the rung it leaves.
    fn plan_feud(&mut self, f: u32, a: ActorId, at: Tick, from_wake: bool) {
        let Some(feud) = self.feud(f, a) else {
            return;
        };
        let lambda = self.culture(f).lambda;
        let cowed = self.factions[f as usize].cowed_until;
        let mut next: Option<Tick> = None;
        let mut take = |t: Option<Tick>| {
            if let Some(t) = t {
                next = Some(next.map_or(t, |n: Tick| n.min(t)));
            }
        };
        // On Peaceful no war party leaves: the climb to a war rung waits for the world to change
        // (`set_difficulty` plans every feud again then).
        let peaceful = self.peaceful;
        if let Some(d) = feud.demand {
            take(Some(d.deadline));
        } else if feud.party.is_none() {
            if feud.holds_envoy() {
                // A withdrawn demand: no climb, a fresh envoy at the old deadline (MZB-22).
                take(Some(feud.wait_until.max(cowed)));
            } else if let Some(up) = self
                .culture(f)
                .next_up(feud.step)
                .filter(|u| !(peaceful && u.is_war()))
            {
                let dwell = feud.since + self.culture(f).dwell_ticks[feud.step.index()];
                take(
                    feud.g
                        .crossing_up(self.theta(f, up), lambda)
                        .map(|t| t.max(dwell).max(cowed).max(feud.wait_until)),
                );
            }
            take(self.repeat_at(f, a, feud));
            if feud.step > Step::None {
                take(feud.g.crossing_down(self.down_theta(f, feud.step), lambda));
            } else if feud.raw.iter().all(|r| *r <= 0.0) {
                take(feud.g.crossing_down(self.tun.forget_below, lambda));
            }
        }
        let feud = self.feud_mut(f, a).expect("checked");
        feud.stamp = feud.stamp.wrapping_add(1);
        let stamp = feud.stamp;
        // A wait only the camp's room or lines can end books nothing: `unblock` plans it again.
        if let Some(t) = next.filter(|t| *t < BLOCKED) {
            let t = if from_wake { t.max(at + 1) } else { t.max(at) };
            self.schedule(
                t,
                Ev::Feud {
                    faction: f,
                    actor: a,
                    stamp,
                },
            );
        }
    }

    fn feud_wake(&mut self, w: &mut World, f: u32, a: ActorId, at: Tick) {
        self.stats.feud_wakes += 1;
        self.ensure_chief(w, f, at);
        let lambda = self.culture(f).lambda;
        let Some(feud) = self.feud_mut(f, a) else {
            return;
        };
        feud.g.commit(at, lambda);
        let g = feud.g.level_sum();
        let (step, since, party, demand, wait, again) = (
            feud.step,
            feud.since,
            feud.party,
            feud.demand,
            feud.wait_until,
            feud.holds_envoy(),
        );
        if let Some(d) = demand {
            if at >= d.deadline {
                self.demand_due(w, f, a, at);
            }
        } else if party.is_none() {
            let cowed = self.factions[f as usize].cowed_until > at;
            let dwell = since + self.culture(f).dwell_ticks[step.index()];
            let up = self.culture(f).next_up(step);
            // A repeat of the rung's war party (MOWZIE.md ledger 3); never on Peaceful
            // (`repeat_after`).
            let repeat = self
                .feud(f, a)
                .and_then(|feud| self.repeat_after(f, a, feud))
                .is_some_and(|t| at >= t && g >= self.theta(f, step) * (1.0 - 1e-9));
            if again && !cowed && at >= wait && g >= self.down_theta(f, step) {
                self.envoy_again(w, f, a, at);
            } else if let Some(up) = up.filter(|u| {
                !again
                    && !cowed
                    && !(self.peaceful && u.is_war())
                    && at >= dwell
                    && at >= wait
                    && g >= self.theta(f, *u) * (1.0 - 1e-9)
            }) {
                self.escalate(w, f, a, up, at, false);
            } else if repeat {
                self.escalate(w, f, a, step, at, true);
            } else if step > Step::None && g < self.down_theta(f, step) {
                let to = self.culture(f).next_down(step);
                let feud = self.feud_mut(f, a).expect("exists");
                feud.step = to;
                feud.since = at;
                feud.envoy_again = false;
                self.stats.calmed += 1;
                let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
                self.tape.event(
                    kinds::CONFLICT_CALM,
                    spot(at, camp),
                    fid,
                    aid(a),
                    to.index() as i64,
                );
                self.note(
                    at,
                    Some(f),
                    "calm",
                    format!(
                        "grievance against {} falls to {g:.1}: back to {}",
                        self.actor_name(w, a),
                        to.name()
                    ),
                );
            } else if step == Step::None
                && g < self.tun.forget_below
                && self
                    .feud(f, a)
                    .expect("exists")
                    .raw
                    .iter()
                    .all(|r| *r <= 0.0)
            {
                self.remove_feud(f, a);
                self.stats.feuds_forgotten += 1;
                return;
            }
        }
        self.plan_feud(f, a, at, true);
    }

    pub fn actor_name(&self, w: &World, a: ActorId) -> String {
        if a == self.settlers {
            "the settlers".to_string()
        } else {
            let u = self.uuid_of(w, a);
            format!("player {:x}", u.1)
        }
    }

    /// Climbs to rung `to`, doing what it says; or, `again`, sends the party of the rung the feud
    /// is on once more (MOWZIE.md ledger 3), counted as a repeat and not as a step.
    fn escalate(&mut self, w: &mut World, f: u32, a: ActorId, to: Step, at: Tick, again: bool) {
        let lambda = self.culture(f).lambda;
        let feud = self.feud(f, a).expect("exists");
        let g = feud.g.total_at(at, lambda);
        let kind = feud.g.dominant(at, lambda);
        let focus = feud.focus;
        if to.is_war() || to == Step::Scouts || to == Step::Envoy {
            let pk = PartyKind::of_step(to).expect("a party rung");
            if self.culture(f).parties[pk.index()].is_empty() {
                // `next_up` and `repeat_after` never pick an empty rung. Were one picked anyway,
                // a pool wait for no warriors would wake the feud every tick: change and book
                // nothing (MZB-64).
                debug_assert!(false, "escalate to the empty rung {}", to.name());
                return;
            }
            let target = self.target_of(w, f, focus, at);
            let Some(p) = self.send_party(w, f, Some(a), pk, target, at) else {
                // Not enough warriors: wait for the pool (closed form). The whole warriors
                // `send_party` needs, so the wait ends when it can send (MOWZIE.md ledger 6).
                let need = (self.party_size(f, pk) as f64 / 2.0).ceil();
                let ready = self.bodies_ready(f, need, at);
                let feud = self.feud_mut(f, a).expect("exists");
                feud.wait_until = ready;
                return;
            };
            let feud = self.feud_mut(f, a).expect("exists");
            feud.party = Some(p);
            if to == Step::Envoy {
                self.demand(w, f, a, p, g, kind, at);
            }
            if to.is_war() {
                let chief = self.factions[f as usize].chief;
                if w.knowledge.actors().is_player(a) {
                    w.knowledge.adjust(chief, a, self.tun.raid_grudge, at);
                }
                let feud = self.feud_mut(f, a).expect("exists");
                feud.raids += 1;
            }
        }
        let feud = self.feud_mut(f, a).expect("exists");
        feud.step = to;
        feud.since = at;
        feud.wait_until = Tick::ZERO;
        feud.envoy_again = false;
        if again {
            self.stats.repeats += 1;
            self.note(
                at,
                Some(f),
                "repeat",
                format!(
                    "grievance against {} still at {g:.1} ({} first): the {} comes again",
                    self.actor_name(w, a),
                    kind.name(),
                    if to == Step::Invasion {
                        "invasion"
                    } else {
                        "war band"
                    }
                ),
            );
            return;
        }
        self.stats.escalations[to.index()] += 1;
        let pos = focus.map_or(self.factions[f as usize].pos, |fo| fo.pos);
        let severity = if matches!(to, Step::Invasion | Step::Siege) {
            Severity::Major
        } else {
            Severity::Notable
        };
        let fid = self.fid(f);
        self.tape
            .event_as(
                kinds::CONFLICT_STEP,
                severity,
                spot(at, pos),
                fid,
                aid(a),
                to.index() as i64,
            )
            .text(|| format!("{} ({} first, grievance {g:.1})", to.name(), kind.name()));
        if to == Step::Notice {
            let player = self.uuid_of(w, a);
            self.notice_serial = self.notice_serial.wrapping_add(1).max(1);
            let serial = self.notice_serial;
            let n = boundary::Notice {
                faction: self.factions[f as usize].key,
                step: to as u32,
                serial,
                player,
                dimension: u32::from(pos.frame),
                signal: kind as u32,
                x: pos.x,
                z: pos.z,
                grievance: g as f32,
            };
            // Kept until the plugin plays it (NOTICE_SHOWN), and sent again at every session
            // start: a notice is never lost (MOWZIE.md §4.12).
            self.feud_mut(f, a).expect("exists").notice = Some(Box::new(n));
            self.outbox.push(Cmd::Notice(n));
        }
        self.note(
            at,
            Some(f),
            to.name(),
            format!(
                "grievance against {} at {g:.1} ({} first): {}",
                self.actor_name(w, a),
                kind.name(),
                match to {
                    Step::Notice => "the tribe takes notice",
                    Step::Envoy => "an envoy with a demand",
                    Step::Scouts => "scouts go out",
                    Step::Raid => "a war band musters",
                    Step::Invasion => "an invasion musters",
                    Step::Siege => "a siege musters",
                    Step::None => "",
                }
            ),
        );
    }

    /// The demand envoy `p` carries: tribute by the grievance `g`, due in the culture's days.
    #[allow(clippy::too_many_arguments)]
    fn demand(
        &mut self,
        w: &World,
        f: u32,
        a: ActorId,
        p: Handle<Party>,
        g: f64,
        kind: SignalKind,
        at: Tick,
    ) {
        let good = self.tribute_good(w, f);
        let base = self.culture(f).def.tribute_per_grievance * g;
        let bend = self.temper(f).bend(Trait::Aggressive, 1000, 50);
        let per_coin = self.tribute_per_coin(w, f, good);
        // Held to what the envoy's wire count (an i32) can say (MZB-57).
        let tribute = ((base * f64::from(bend) / 1000.0 * per_coin).ceil() as i64)
            .clamp(1, i64::from(i32::MAX));
        let deadline = at + days_to_ticks(self.culture(f).def.demand_days);
        let feud = self.feud_mut(f, a).expect("exists");
        feud.demand = Some(Demand {
            kind,
            good,
            tribute,
            paid: 0,
            made: at,
            deadline,
            after: 0.0,
            envoy: Some(p),
        });
        self.stats.demands += 1;
        let gname = w.goods.get(good).name.clone();
        let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
        self.tape
            .event(kinds::CONFLICT_DEMAND, spot(at, camp), fid, aid(a), tribute)
            .text(|| {
                format!(
                    "stop the {} or pay {tribute} {gname} by day {:.1}",
                    kind.name(),
                    deadline.0 as f64 / TICKS_PER_DAY as f64
                )
            });
        self.note(
            at,
            Some(f),
            "demand",
            format!(
                "an envoy sets out for {}: stop the {} or pay {tribute} {gname} by day {:.1}",
                self.actor_name(w, a),
                kind.name(),
                deadline.0 as f64 / TICKS_PER_DAY as f64
            ),
        );
    }

    /// The withdrawn demand's deadline came with the feud still on Envoy: a fresh envoy sets out
    /// with a new demand, where an expired one would have climbed (MZB-22). No rung changes. With
    /// too few warriors it waits for the pool, still holding.
    fn envoy_again(&mut self, w: &mut World, f: u32, a: ActorId, at: Tick) {
        let lambda = self.culture(f).lambda;
        let feud = self.feud(f, a).expect("exists");
        let (g, kind, focus) = (
            feud.g.total_at(at, lambda),
            feud.g.dominant(at, lambda),
            feud.focus,
        );
        let target = self.target_of(w, f, focus, at);
        let Some(p) = self.send_party(w, f, Some(a), PartyKind::Envoy, target, at) else {
            // The whole warriors `send_party` needs, as `escalate` waits for (MZB-52).
            let need = (self.party_size(f, PartyKind::Envoy) as f64 / 2.0).ceil();
            let ready = self.bodies_ready(f, need, at);
            self.feud_mut(f, a).expect("exists").wait_until = ready;
            return;
        };
        let feud = self.feud_mut(f, a).expect("exists");
        feud.party = Some(p);
        feud.envoy_again = false;
        feud.since = at;
        feud.wait_until = Tick::ZERO;
        self.demand(w, f, a, p, g, kind, at);
    }

    fn tribute_good(&self, w: &World, f: u32) -> GoodId {
        let c = self.culture(f);
        c.def
            .tribute_good
            .as_deref()
            .and_then(|n| w.goods.find(n))
            .or_else(|| w.goods.currency(c.market))
            .unwrap_or(GoodId(0))
    }

    /// Units of the tribute good one coin of the haven's money buys (MOWZIE.md ledger 12): 1 for
    /// money itself, else at the haven market's price, else the culture's fallback while that
    /// market has no price for it. A price that has fallen near nothing asks at most
    /// [`TRIBUTE_PRICE_CAP`] times the fallback (MZB-57).
    fn tribute_per_coin(&self, w: &World, f: u32, good: GoodId) -> f64 {
        if w.goods.is_currency(good) {
            return 1.0;
        }
        let price = w.market(self.factions[f as usize].node).price(good);
        let fallback = self.culture(f).def.tribute_per_coin_fallback;
        if price > 0 {
            (PRICE_ONE as f64 / price as f64).min(TRIBUTE_PRICE_CAP * fallback)
        } else {
            fallback
        }
    }

    /// Where a feud's party goes: its focus's district, else the place, else the camp's edge.
    fn target_of(&self, w: &World, f: u32, focus: Option<Focus>, at: Tick) -> Target {
        let _ = (w, at);
        match focus {
            Some(Focus {
                district: Some(d),
                pos,
                ..
            }) => Target::District(d, pos),
            Some(fo) => Target::Pos(fo.pos),
            None => Target::Pos(self.factions[f as usize].pos),
        }
    }

    fn demand_due(&mut self, w: &mut World, f: u32, a: ActorId, at: Tick) {
        let feud = self.feud(f, a).expect("exists");
        let Some(d) = feud.demand else {
            return;
        };
        let cut = feud.g.rate[d.kind.index()] <= 0.0 && d.after <= 0.0;
        if d.paid >= d.tribute {
            self.settle(w, f, a, boundary::SETTLED_PAID, at);
        } else if cut {
            self.settle(w, f, a, boundary::SETTLED_CUT, at);
        } else {
            self.demand_unmet(w, f, a, d, boundary::SETTLED_EXPIRED, at);
        }
    }

    /// A demand that went unmet: expired at its deadline, or refused (ENVOY_ANSWER), which is
    /// never a signal cut even when no impulse came since (choice C2-12 in
    /// docs/overnight-choices-conflictjni.md). Scouts if grievance is at their threshold, else the
    /// feud stays at envoy above the hysteresis or falls back to notice.
    fn demand_unmet(
        &mut self,
        w: &mut World,
        f: u32,
        a: ActorId,
        d: Demand,
        outcome: u32,
        at: Tick,
    ) {
        let lambda = self.culture(f).lambda;
        {
            self.stats.expired += 1;
            self.settle_outbox(w, f, a, d, outcome);
            self.envoy_home(w, d.envoy, at);
            let g = self.feud(f, a).expect("exists").g.total_at(at, lambda);
            let feud = self.feud_mut(f, a).expect("exists");
            feud.demand = None;
            feud.party = None;
            let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
            self.tape
                .event(
                    kinds::CONFLICT_UNMET,
                    spot(at, camp),
                    fid,
                    aid(a),
                    d.tribute - d.paid,
                )
                .text(|| {
                    format!(
                        "{} of {} unpaid, grievance {g:.1}",
                        d.tribute - d.paid,
                        d.tribute
                    )
                });
            self.note(
                at,
                Some(f),
                "refused",
                format!(
                    "the demand on {} goes unmet (grievance {g:.1})",
                    self.actor_name(w, a)
                ),
            );
            // The next rung up the culture has (MOWZIE.md ledger 2): Scouts, unless it skips them.
            let up = self.culture(f).next_up(Step::Envoy);
            if let Some(up) = up.filter(|u| g >= self.theta(f, *u) * (1.0 - 1e-9)) {
                self.escalate(w, f, a, up, at, false);
            } else if g < self.down_theta(f, Step::Envoy) {
                let to = self.culture(f).next_down(Step::Envoy);
                let feud = self.feud_mut(f, a).expect("exists");
                feud.step = to;
                feud.since = at;
            } else {
                let feud = self.feud_mut(f, a).expect("exists");
                feud.since = at;
            }
        }
    }

    fn settle_outbox(&mut self, w: &World, f: u32, a: ActorId, d: Demand, outcome: u32) {
        let player = self.uuid_of(w, a);
        self.outbox.push(Cmd::Settled(boundary::DemandSettled {
            faction: self.factions[f as usize].key,
            outcome,
            agent: d.envoy.map_or(0, Party::agent),
            player,
            signal: d.kind as u32,
        }));
    }

    /// A demand met: paid in full or its signal cut by the deadline. The feud steps back to
    /// notice and loses a share of its grievance.
    fn settle(&mut self, w: &mut World, f: u32, a: ActorId, outcome: u32, at: Tick) {
        let c = self.culture(f);
        let lambda = c.lambda;
        let relief = if outcome == boundary::SETTLED_PAID {
            c.def.tribute_relief
        } else {
            c.def.cut_relief
        };
        let respite = days_to_ticks(c.def.respite_days);
        let feud = self.feud_mut(f, a).expect("exists");
        let Some(d) = feud.demand.take() else {
            return;
        };
        feud.g.scale(1.0 - relief, at, lambda);
        feud.step = Step::Notice;
        feud.since = at;
        feud.party = None;
        if outcome == boundary::SETTLED_PAID {
            // A paid tribute buys a respite: the ladder holds at notice for a while.
            feud.wait_until = at + respite;
        }
        let g = feud.g.level_sum();
        if outcome == boundary::SETTLED_PAID {
            self.stats.tributes += 1;
        } else {
            self.stats.cuts += 1;
        }
        self.settle_outbox(w, f, a, d, outcome);
        self.envoy_home(w, d.envoy, at);
        let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
        if outcome == boundary::SETTLED_PAID {
            let gname = w.goods.get(d.good).name.clone();
            self.tape
                .event(
                    kinds::CONFLICT_TRIBUTE,
                    spot(at, camp),
                    fid,
                    aid(a),
                    d.tribute,
                )
                .text(|| format!("{} {gname}", d.tribute));
        } else {
            self.tape
                .event(kinds::CONFLICT_COMPLIED, spot(at, camp), fid, aid(a), 0)
                .text(|| format!("the {} stopped", d.kind.name()));
        }
        self.note(
            at,
            Some(f),
            "settled",
            format!(
                "{} meets the demand ({}): back to notice, grievance {g:.1}",
                self.actor_name(w, a),
                if outcome == boundary::SETTLED_PAID {
                    "tribute paid"
                } else {
                    "the signal stopped"
                }
            ),
        );
        self.plan_feud(f, a, at, false);
    }

    /// The envoy party `p` fell with no player among its killers (MOWZIE.md ledger 14): its
    /// demand is withdrawn (DEMAND_SETTLED outcome 5), with no relief, no refusal and no climb.
    /// The feud holds on Envoy to the withdrawn demand's deadline, climbing nowhere, and then
    /// sends a fresh envoy with a new demand (MZB-22): a withdrawal never brings a war forward.
    fn withdraw(&mut self, w: &mut World, p: Handle<Party>, at: Tick) {
        let (f, feud) = (self.parties[p].faction, self.parties[p].feud);
        let Some(a) = feud else {
            return;
        };
        let theirs = self
            .feud(f, a)
            .and_then(|x| x.demand)
            .is_some_and(|d| d.envoy == Some(p));
        if !theirs {
            return;
        }
        let feud = self.feud_mut(f, a).expect("checked");
        let d = feud.demand.take().expect("checked");
        feud.party = None;
        feud.since = at;
        // Held on Envoy to the old deadline, then a fresh envoy (MZB-22): a withdrawn demand
        // never brings the climb forward.
        feud.wait_until = feud.wait_until.max(d.deadline);
        feud.envoy_again = true;
        self.parties[p].feud = None;
        self.stats.withdrawn += 1;
        self.settle_outbox(w, f, a, d, boundary::SETTLED_WITHDRAWN);
        self.note(
            at,
            Some(f),
            "withdrawn",
            format!(
                "the envoy to {} fell to no player's hand: the demand is withdrawn",
                self.actor_name(w, a)
            ),
        );
        self.plan_feud(f, a, at, false);
    }

    fn envoy_home(&mut self, w: &mut World, envoy: Option<Handle<Party>>, at: Tick) {
        let Some(p) = envoy.filter(|p| self.parties.contains(*p)) else {
            return;
        };
        if let Some(x) = self.parties.get_mut(p) {
            x.feud = None;
        }
        if self.parties[p].phase != Phase::Home {
            self.go_home(w, p, boundary::RECALL_SETTLED, at);
        }
    }

    // ---- tribute, gifts, trade --------------------------------------------------------------

    /// Tribute paid to a faction (at its envoy or its camp): the goods enter the world at the
    /// haven's market (players' goods coming in), and the open demand on the payer (or else on the
    /// settlers) counts them. Returns whether a demand was met.
    pub fn pay_tribute(
        &mut self,
        w: &mut World,
        faction: u64,
        payer: ActorId,
        envoy: Option<u64>,
        goods: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) -> bool {
        self.run_to(at);
        let Some(&f) = self.by_key.get(&faction) else {
            return false;
        };
        let node = self.factions[f as usize].node;
        for (g, a) in w.stock_from_players(node, goods, at) {
            add(&mut self.ledger.tribute, g, a);
        }
        let settlers = self.settlers(w);
        // The envoy it was paid to names the demand; at the camp, the payer's own, then the
        // settlers'.
        let by_envoy = envoy
            .and_then(Party::of_agent)
            .and_then(|p| self.parties.get(p))
            .filter(|x| x.faction == f)
            .and_then(|x| x.feud);
        let who = by_envoy
            .into_iter()
            .chain([payer, settlers])
            .find(|a| self.feud(f, *a).is_some_and(|x| x.demand.is_some()));
        let Some(a) = who else {
            // No demand open: a tribute is a gift.
            self.gift_value(w, f, payer, goods, at);
            return false;
        };
        let feud = self.feud_mut(f, a).expect("found");
        let d = feud.demand.as_mut().expect("found");
        d.paid += goods.get(&d.good).copied().unwrap_or(0) / MILLI;
        if d.paid >= d.tribute {
            self.settle(w, f, a, boundary::SETTLED_PAID, at);
            true
        } else {
            false
        }
    }

    /// A gift to a faction: stocked at its haven; grievance against the giver falls by its value
    /// at the haven's market (a sociable chief forgives more, C25).
    pub fn gift(
        &mut self,
        w: &mut World,
        faction: u64,
        giver: ActorId,
        goods: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) {
        self.run_to(at);
        let Some(&f) = self.by_key.get(&faction) else {
            return;
        };
        let node = self.factions[f as usize].node;
        for (g, a) in w.stock_from_players(node, goods, at) {
            add(&mut self.ledger.gifts, g, a);
        }
        self.gift_value(w, f, giver, goods, at);
    }

    /// Trade at a faction's haven (the world books the trade itself): its value in coins softens
    /// the trader's feud, at a tenth of a gift's rate (PLACEHOLDER).
    pub fn traded(&mut self, w: &mut World, faction: u64, trader: ActorId, coins: f64, at: Tick) {
        self.run_to(at);
        let Some(&f) = self.by_key.get(&faction) else {
            return;
        };
        self.relieve(w, f, trader, coins * 0.1, at);
    }

    /// Sends a party now, as an order (tests, a plugin's GameTests, an admin command): a war band
    /// at a district, plunderers at a node, scouts at a place. Plunder and orders with no feud
    /// serve none. `None` when the faction is unknown or cannot field it.
    pub fn order_party(
        &mut self,
        w: &mut World,
        faction: u64,
        kind: PartyKind,
        target: Target,
        feud: Option<ActorId>,
        at: Tick,
    ) -> Option<Handle<Party>> {
        self.run_to(at);
        let f = *self.by_key.get(&faction)?;
        let p = self.send_party(w, f, feud, kind, target, at)?;
        if let Some(a) = feud
            && let Some(x) = self.feud_mut(f, a)
        {
            x.party = Some(p);
        }
        Some(p)
    }

    fn gift_value(
        &mut self,
        w: &mut World,
        f: u32,
        giver: ActorId,
        goods: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) {
        let m = w.market(self.factions[f as usize].node);
        let mut coins = 0.0;
        for (&g, &a) in goods {
            let per = if w.goods.is_currency(g) {
                1.0
            } else {
                m.price(g) as f64 / PRICE_ONE as f64
            };
            coins += per * a as f64 / MILLI as f64;
        }
        self.stats.gifts += 1;
        self.relieve(w, f, giver, coins, at);
    }

    fn relieve(&mut self, w: &mut World, f: u32, a: ActorId, coins: f64, at: Tick) {
        let c = self.culture(f);
        let lambda = c.lambda;
        let bend = self.temper(f).bend(Trait::Sociable, 1000, 50);
        let amount = coins * c.def.gift_relief_per_coin * f64::from(bend) / 1000.0;
        if let Some(feud) = self.feud_mut(f, a) {
            feud.g.relieve(amount, at, lambda);
            self.note(
                at,
                Some(f),
                "gift",
                format!("a gift worth {coins:.1} coins softens it by {amount:.1}"),
            );
            self.plan_feud(f, a, at, false);
        }
        let _ = w;
    }

    // ---- parties ----------------------------------------------------------------------------

    fn party_size(&self, f: u32, k: PartyKind) -> u32 {
        self.culture(f).parties[k.index()].iter().map(|u| u.1).sum()
    }

    fn target_pos(&self, w: &World, t: Target) -> Pos {
        match t {
            Target::District(_, p) | Target::Pos(p) => p,
            Target::Node(i) => w.nodes[i].pos,
        }
    }

    /// The road from the camp to `to`: the shortest path over road edges to the node nearest the
    /// target, then straight to it. Straight when no road gets nearer.
    fn route(&mut self, w: &World, from: usize, to: Pos) -> Vec<(i32, i32)> {
        self.nodes.refresh(w);
        let start = w.nodes[from].pos;
        let topo = self.topo.clone();
        let t = topo.of(start.frame);
        let mut out = vec![(start.x, start.z)];
        let direct = tdist(t, pxz(start), pxz(to));
        let nearest = self
            .nodes
            .nearest(w, t, to, direct, |i| w.nodes[i].pos.frame == to.frame)
            .map(|b| b.1);
        if let Some(goal) = nearest.filter(|&g| g != from) {
            // A* over road edges (lanes are ships'), the straight line as the heuristic: an edge
            // is never shorter than the straight line between its ends (the path factor is at
            // least 1), so the first time the goal is popped its path is the shortest.
            let gp = pxz(w.nodes[goal].pos);
            let h = |i: usize| tdist(t, pxz(w.nodes[i].pos), gp) as u64;
            let mut best: BTreeMap<usize, u64> = BTreeMap::new();
            let mut prev: BTreeMap<usize, usize> = BTreeMap::new();
            let mut heap = std::collections::BinaryHeap::new();
            best.insert(from, 0);
            heap.push(std::cmp::Reverse((h(from), 0u64, from)));
            let mut found = false;
            while let Some(std::cmp::Reverse((_, d, i))) = heap.pop() {
                if d > best.get(&i).copied().unwrap_or(u64::MAX) {
                    continue;
                }
                if i == goal {
                    found = true;
                    break;
                }
                for r in &w.nodes[i].out {
                    let e = &w.edges[r.edge as usize];
                    if e.medium != rtsim_travel::Medium::Road {
                        continue;
                    }
                    let j = e.dirs[r.dir as usize].to;
                    let nd = d + e.length_blocks;
                    if nd < best.get(&j).copied().unwrap_or(u64::MAX) {
                        best.insert(j, nd);
                        prev.insert(j, i);
                        heap.push(std::cmp::Reverse((nd + h(j), nd, j)));
                    }
                }
            }
            if found {
                let mut path = vec![goal];
                while let Some(&p) = path.last() {
                    if p == from {
                        break;
                    }
                    path.push(prev[&p]);
                }
                path.reverse();
                for &i in &path[1..] {
                    let p = w.nodes[i].pos;
                    out.push((p.x, p.z));
                }
            }
        }
        if out.last() != Some(&(to.x, to.z)) || out.len() < 2 {
            out.push((to.x, to.z));
        }
        // A chart polyline from the canonical start: each point the image of the next stop
        // nearest the one before, so a march over a seam goes straight across it (TP7).
        if !t.is_flat() {
            for k in 1..out.len() {
                let prev = (i64::from(out[k - 1].0), i64::from(out[k - 1].1));
                let next = (i64::from(out[k].0), i64::from(out[k].1));
                let c = t.toward(prev, next);
                out[k] = (c.0 as i32, c.1 as i32);
            }
        }
        out
    }

    fn times_for(&self, f: u32, route: &[(i32, i32)], start: Tick) -> Vec<Tick> {
        let speed = self.culture(f).def.march_blocks_per_tick;
        let mut t = start;
        let mut out = vec![t];
        for s in route.windows(2) {
            let d = dist(
                (i64::from(s[0].0), i64::from(s[0].1)),
                (i64::from(s[1].0), i64::from(s[1].1)),
            );
            t += ((d / speed).ceil() as u64).max(1);
            out.push(t);
        }
        out
    }

    /// Musters a party at the camp. War parties draw on the pool (at least half their make-up, or
    /// none goes); envoys and scouts always go. A kind the culture has no party for sends nothing
    /// (MOWZIE.md ledger 2; the ladder never climbs to such a rung).
    fn send_party(
        &mut self,
        w: &mut World,
        f: u32,
        feud: Option<ActorId>,
        kind: PartyKind,
        target: Target,
        at: Tick,
    ) -> Option<Handle<Party>> {
        let make: Vec<(u8, u32)> = self.culture(f).parties[kind.index()].clone();
        if make.is_empty() {
            return None;
        }
        let want: u32 = make.iter().map(|u| u.1).sum();
        let units = if kind.is_war() {
            // The smaller of the whole warriors and the slots at home with no line in flight
            // (MOWZIE.md ledger 7): a slot whose body may be going or coming is not drawn.
            self.camp_sync(f, at);
            let mut have = self.pool_at(f, at).floor() as u32;
            if let Some(slots) = self.camp_drawable(f) {
                have = have.min(slots);
            }
            if have == 0 || have * 2 < want {
                return None;
            }
            let share = f64::from(have.min(want)) / f64::from(want.max(1));
            let mut v: Vec<(u8, u32)> = make
                .iter()
                .map(|&(u, c)| (u, ((f64::from(c) * share).floor() as u32).max(1)))
                .collect();
            let total: u32 = v.iter().map(|u| u.1).sum();
            if total > have {
                v[0].1 = v[0].1.saturating_sub(total - have).max(1);
            }
            // Every kind keeps one, so a make-up with more kinds than `have` (or a pair of one
            // each with one warrior at home) still overdraws: the last kinds give theirs up. A
            // party never has more units than the slots (or the pool) it was drawn from.
            let mut total: u32 = v.iter().map(|u| u.1).sum();
            for u in v.iter_mut().rev() {
                let cut = total.saturating_sub(have).min(u.1);
                u.1 -= cut;
                total -= cut;
            }
            v.retain(|u| u.1 > 0);
            debug_assert!(total <= have && total > 0, "a draw of {total} from {have}");
            self.pool_add(f, -f64::from(total), at);
            v
        } else {
            make
        };
        let start_units: u32 = units.iter().map(|u| u.1).sum();
        let node = self.factions[f as usize].node;
        let to = self.target_pos(w, target);
        let route = self.route(w, node, to);
        let muster = if kind.is_war() {
            days_to_ticks(self.culture(f).def.muster_days)
        } else {
            0
        };
        let times = self.times_for(f, &route, at + muster);
        let chief = matches!(
            kind,
            PartyKind::Raid | PartyKind::Invasion | PartyKind::Siege
        ) && !self.factions[f as usize].chief_out;
        if chief {
            self.factions[f as usize].chief_out = true;
        }
        let p = self.parties.insert(Party {
            faction: f,
            kind,
            feud,
            units,
            spawned: Vec::new(),
            recalled: Vec::new(),
            start_units,
            target,
            frame: self.factions[f as usize].pos.frame,
            route,
            times,
            seg: 0,
            phase: Phase::Mustering,
            until: at + muster,
            bored_at: Tick(u64::MAX / 2),
            plan: 0,
            leg: None,
            epoch: 0,
            embodied: false,
            seen_by: None,
            wave: 0,
            loot: BTreeMap::new(),
            from_players: BTreeMap::new(),
            chief,
            breaker: None,
            fight_strength: 0.0,
        });
        self.stats.parties[kind.index()] += 1;
        if kind.is_war() {
            self.factions[f as usize].away += start_units;
            self.camp_draw(f, p, start_units, at);
        }
        let pos = self.factions[f as usize].pos;
        let fid = self.fid(f);
        self.tape
            .event_as(
                kinds::CONFLICT_PARTY,
                party_severity(kind),
                spot(at, pos),
                pid(p),
                fid,
                i64::from(start_units),
            )
            .text(|| format!("{} of {start_units} for ({}, {})", kind.name(), to.x, to.z));
        self.book_leg(w, p, (pos.x, pos.z), (pos.x, pos.z), at, at + muster);
        self.schedule(at + muster, Ev::Party { party: p, plan: 0 });
        self.note(
            at,
            Some(f),
            "muster",
            format!(
                "{} of {start_units} {} for ({}, {}), leaving day {:.2}",
                kind.name(),
                if kind.is_war() { "warriors" } else { "members" },
                to.x,
                to.z,
                (at + muster).0 as f64 / TICKS_PER_DAY as f64
            ),
        );
        Some(p)
    }

    fn bump(&mut self, p: Handle<Party>) -> u32 {
        let x = &mut self.parties[p];
        x.plan = x.plan.wrapping_add(1);
        x.plan
    }

    /// Books (or replans) the party's leg, and finds the players already watching where it goes.
    fn book_leg(
        &mut self,
        w: &mut World,
        p: Handle<Party>,
        from: (i32, i32),
        to: (i32, i32),
        t0: Tick,
        t1: Tick,
    ) {
        // From the canonical start, the end in the same chart (TP7).
        let frame = self.parties[p].frame;
        let f = self.topo(frame).fold(i64::from(from.0), i64::from(from.1));
        let to = f.iso.cell(i64::from(to.0), i64::from(to.1));
        let path = Path {
            frame,
            x0: f.x as i32,
            z0: f.z as i32,
            x1: to.0 as i32,
            z1: to.1 as i32,
            t0,
            t1: t1.max(t0),
            radius: self.tun.party_radius,
        };
        let severity = party_severity(self.parties[p].kind);
        self.tape.leg_as(
            kinds::CONFLICT_PARTY_LEG,
            severity,
            u32::from(frame),
            (path.t0.0, path.x0, path.z0),
            (path.t1.0, path.x1, path.z1),
            pid(p),
        );
        let old = self.parties[p].leg;
        let stamp = match old.and_then(|l| w.legs.replan(l.leg, path)) {
            Some(s) => s,
            None => w
                .legs
                .book(path, Owner(PARTY_OWNER | (p.to_bits() & MASK48))),
        };
        self.parties[p].leg = Some(stamp);
        for other in w.legs.sharing(stamp) {
            let Some(row) = w.legs.get(other.leg) else {
                continue;
            };
            if row.owner.0 & PLAYER_OWNER == 0 {
                continue;
            }
            let player = ActorId::from_bits(row.owner.0 & MASK48);
            if let Some(m) = w.legs.meeting(stamp, other) {
                self.schedule(
                    m.at,
                    Ev::Seen {
                        party: p,
                        player,
                        meeting: m,
                    },
                );
            }
        }
    }

    /// Where a party is at `t`, canonical (its route is a chart polyline, TP7).
    pub fn party_pos(&self, p: Handle<Party>, t: Tick) -> (i32, i32) {
        let c = self.party_chart_pos(p, t);
        self.topo(self.parties[p].frame).canonical_i32(c.0, c.1)
    }

    fn party_event(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        at: Tick,
    ) {
        match self.parties[p].phase {
            Phase::Mustering if self.peaceful && self.parties[p].kind.is_war() => {
                self.stand_down(w, p, at);
            }
            Phase::Mustering => {
                self.camp_set_out(p, at);
                self.parties[p].phase = Phase::Out;
                self.parties[p].seg = 0;
                self.start_segment(w, infra, p, at);
            }
            Phase::Out | Phase::Home => {
                let x = &mut self.parties[p];
                x.seg += 1;
                if x.seg as usize + 1 >= x.route.len() {
                    if x.phase == Phase::Out {
                        self.arrive(w, infra, p, at);
                    } else {
                        self.home(w, p, at);
                    }
                } else {
                    self.start_segment(w, infra, p, at);
                }
            }
            Phase::Waiting => self.wait_over(w, infra, p, at),
            Phase::Fighting => {
                let reason = if at >= self.parties[p].bored_at {
                    boundary::RECALL_BORED
                } else {
                    boundary::RECALL_DONE
                };
                self.end_fight(w, p, reason, at);
            }
        }
    }

    /// Peaceful at a war party's muster's end (MOWZIE.md §4.12): it never leaves. Its bodies, if
    /// any, go; its warriors are back in the pool at once; the feud it served steps back a rung,
    /// so that it climbs to it again once the world is not Peaceful.
    fn stand_down(&mut self, w: &mut World, p: Handle<Party>, at: Tick) {
        let x = &self.parties[p];
        let (f, kind, embodied, epoch) = (x.faction, x.kind, x.embodied, x.epoch);
        self.stats.peace_held += 1;
        if embodied {
            let camp = self.factions[f as usize].pos;
            self.recall(p, epoch, boundary::RECALL_DONE, (camp.x, camp.z));
        }
        self.step_back(p);
        self.note(
            at,
            Some(f),
            "peaceful",
            format!("the {} stands down: the world is Peaceful", kind.name()),
        );
        self.home(w, p, at);
    }

    fn start_segment(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        at: Tick,
    ) {
        let x = &self.parties[p];
        let s = x.seg as usize;
        let (a, b) = (x.route[s], x.route[s + 1]);
        let (t0, t1) = (x.times[s], x.times[s + 1]);
        let (kind, phase, embodied, epoch, frame) = (x.kind, x.phase, x.embodied, x.epoch, x.frame);
        self.book_leg(w, p, a, b, t0.max(at), t1);
        let plan = self.bump(p);
        self.schedule(t1, Ev::Party { party: p, plan });
        // A watched band never walks a body across an edge_stop seam: it is recalled at the
        // start of a segment that would, and marches on abstract (TP12).
        let crosses_stop = {
            let t = self.topo(frame);
            t.any_stopped() && {
                let f = t.fold(i64::from(a.0), i64::from(a.1));
                let e = f.iso.cell(i64::from(b.0), i64::from(b.1));
                t.stopped_crossing((f.x, f.z), e).is_some()
            }
        };
        if embodied && crosses_stop {
            self.note(
                at,
                Some(self.parties[p].faction),
                "recall",
                "its march crosses an edge_stop seam".into(),
            );
            self.recall(p, epoch, boundary::RECALL_PASSING, b);
        } else if embodied {
            let speed = self
                .culture(self.parties[p].faction)
                .def
                .march_blocks_per_tick as f32;
            let last = s + 2 == self.parties[p].route.len();
            self.outbox.push(Cmd::Steer(boundary::BandSteer {
                agent: Party::agent(p),
                epoch,
                flags: if last && phase == Phase::Out && kind.is_war() {
                    boundary::STEER_ATTACK
                } else {
                    0
                },
                speed,
                pos: (f64::from(b.0), 64.0, f64::from(b.1)),
                arrive: t1.0 as i64,
            }));
            self.book_unseen(w, p, at);
        }
        // Supply lines (C16, I24): a war band on its way out may fall on a train leg it crosses.
        if phase == Phase::Out
            && matches!(
                kind,
                PartyKind::Raid | PartyKind::Invasion | PartyKind::Siege
            )
        {
            let mut regions: Vec<(i32, i32)> = Vec::new();
            let steps = 8;
            for k in 0..=steps {
                let x = i64::from(a.0) + (i64::from(b.0) - i64::from(a.0)) * k / steps;
                let z = i64::from(a.1) + (i64::from(b.1) - i64::from(a.1)) * k / steps;
                let (x, z) = self.topo(frame).canonical(x, z);
                let r = LegIndex::region_of(x, z);
                let r = (r.0 as i32, r.1 as i32);
                if !regions.contains(&r) {
                    regions.push(r);
                }
            }
            if let Some(tr) = infra
                .trains_crossed(frame, &regions, t0.max(at), t1)
                .first()
                .copied()
            {
                let roll = StreamKey::new(self.seed, SYS_AMBUSH, p.to_bits())
                    .slot(u64::from(self.parties[p].seg) << 32 | u64::from(plan))
                    .below(1000);
                if (roll as u32) < self.tun.ambush_milli {
                    self.schedule(
                        tr.from.max(at),
                        Ev::Ambush {
                            party: p,
                            plan,
                            train: tr.train.0,
                        },
                    );
                }
            }
        }
    }

    /// When a party walking its current segment leaves a circle of `radius` around `c`: `None`
    /// when it is still inside at the segment's end; `at` when it is not inside now.
    fn exit_time(&self, p: Handle<Party>, c: (i32, i32), radius: u32, at: Tick) -> Option<Tick> {
        let x = &self.parties[p];
        let s = (x.seg as usize).min(x.route.len().saturating_sub(2));
        let (a, b) = (x.route[s], x.route[s + 1]);
        let (t0, t1) = (x.times[s], x.times[s + 1]);
        // In the route's chart: the watcher's image nearest the party (TP7).
        let now = self.party_chart_pos(p, at);
        let c = {
            let i = self.topo(x.frame).toward(
                (i64::from(now.0), i64::from(now.1)),
                (i64::from(c.0), i64::from(c.1)),
            );
            (i.0 as i32, i.1 as i32)
        };
        let r = f64::from(radius);
        let inside = |q: (i32, i32)| {
            let (dx, dz) = (f64::from(q.0 - c.0), f64::from(q.1 - c.1));
            dx * dx + dz * dz <= r * r
        };
        if !inside(now) {
            return Some(at);
        }
        if inside(b) || t1 <= t0 {
            return None;
        }
        // |A + D f - C| = r, the larger root: where the walk leaves the circle.
        let (ax, az) = (f64::from(a.0 - c.0), f64::from(a.1 - c.1));
        let (dx, dz) = (f64::from(b.0 - a.0), f64::from(b.1 - a.1));
        let qa = dx * dx + dz * dz;
        let qb = 2.0 * (ax * dx + az * dz);
        let qc = ax * ax + az * az - r * r;
        let disc = (qb * qb - 4.0 * qa * qc).max(0.0);
        let f = ((-qb + disc.sqrt()) / (2.0 * qa)).clamp(0.0, 1.0);
        Some(Tick(t0.0 + ((t1.0 - t0.0) as f64 * f).floor() as u64).max(at))
    }

    /// Books the time an embodied party walks out of its watcher's view.
    fn book_unseen(&mut self, w: &World, p: Handle<Party>, at: Tick) {
        let x = &self.parties[p];
        if !x.embodied || x.phase != Phase::Out {
            return;
        }
        let Some(player) = x.seen_by else {
            return;
        };
        let epoch = x.epoch;
        let c = w
            .players
            .get(&player)
            .and_then(|s| s.pos)
            .map(|e| (e.0.x, e.0.z));
        let leave = match c {
            Some(c) => self.exit_time(p, c, self.tun.view_radius + self.tun.party_radius + 16, at),
            None => Some(at),
        };
        if let Some(t) = leave {
            self.schedule(t, Ev::Unseen { party: p, epoch });
        }
    }

    /// The party's alive units by threat class: (count, strength, loads). The loads are its
    /// units' own carry and block breaking (overnight choice C2-1, K2-13's recommendation), so a
    /// raid never depends on the industry engine's per-class defaults, which carry and break
    /// nothing for ground attackers.
    fn strength_by_class(
        &self,
        p: Handle<Party>,
    ) -> BTreeMap<rtsim_industry::threat::ThreatClass, (u32, f64, Loads)> {
        let x = &self.parties[p];
        let mut m = BTreeMap::new();
        for &(u, n) in &x.units {
            if n == 0 {
                continue;
            }
            let ud = self.unit_row(x.faction, u);
            let e = m
                .entry(ud.class.threat())
                .or_insert((0u32, 0.0f64, Loads::default()));
            e.0 += n;
            e.1 += f64::from(n) * ud.strength;
            e.2.carry += i64::from(n) * i64::from(ud.carry);
            e.2.blocks += n * ud.breaks;
        }
        m
    }

    fn strength(&self, p: Handle<Party>) -> f64 {
        let x = &self.parties[p];
        x.units
            .iter()
            .map(|&(u, n)| f64::from(n) * self.unit_row(x.faction, u).strength)
            .sum()
    }

    /// Unit `u` of faction `f` as it fights now: the row its chief's crystal picks (MOWZIE.md
    /// ledger 9). A body is always its own row's.
    fn unit_row(&self, f: u32, u: u8) -> &crate::culture::UnitDef {
        self.culture(f)
            .unit_as(u, self.factions[f as usize].crystal)
    }

    /// A war band falls on a train it crosses (C16, I24): one threat per class of unit, each with
    /// that class's count, strength and carry (MOWZIE.md ledger 9, MZB-53), as district raids
    /// send them. One ambush however many classes took something.
    fn ambush(
        &mut self,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        train: u64,
        at: Tick,
        w: &mut World,
    ) {
        if self.peaceful {
            self.stats.peace_held += 1;
            return;
        }
        let f = self.parties[p].faction;
        let source = self.culture(f).def.source.threat();
        let mut hit = false;
        let mut got = 0;
        for (class, (count, strength, loads)) in self.strength_by_class(p) {
            let id = fnv1a64(
                &[
                    Party::agent(p).to_le_bytes(),
                    at.0.to_le_bytes(),
                    train.to_le_bytes(),
                    (class.index() as u64).to_le_bytes(),
                ]
                .concat(),
            );
            let t = Threat {
                id,
                source,
                class,
                count,
                strength,
                target: ThreatTarget::Train(rtsim_industry::ids::TrainId(train)),
                at,
                loads: Some(Loads {
                    carry: loads.carry,
                    blocks: 0,
                }),
            };
            let Some(loot) = infra.raid_train(&t) else {
                continue;
            };
            hit = true;
            got += self.take_industry_loot(w, infra, p, &loot);
        }
        if !hit {
            return;
        }
        self.stats.ambushes += 1;
        let (x, z) = self.party_pos(p, at);
        let frame = u32::from(self.parties[p].frame);
        self.tape
            .event(
                kinds::CONFLICT_AMBUSH,
                At::new(at.0, frame, x, z),
                pid(p),
                u128::from(train),
                got,
            )
            .text(|| format!("train {train}: {got} items"));
        self.note(
            at,
            Some(f),
            "ambush",
            format!("falls on train {train} on the way: {got} items"),
        );
    }

    /// Goods `a` (thousandths) of `g` taken from players by party `p`: carried home, or destroyed
    /// where taken by a culture whose loot is (MOWZIE.md ledger 10).
    fn took_from_players(&mut self, p: Handle<Party>, g: GoodId, a: Amount) {
        add(&mut self.ledger.from_players, g, a);
        if self.culture(self.parties[p].faction).def.loot_destroyed {
            add(&mut self.ledger.destroyed_from_players, g, a);
        } else {
            *self.parties[p].from_players.entry(g).or_default() += a;
        }
    }

    /// Maps industry goods to the world's by name into the party's players' goods. Returns items.
    fn take_industry_loot(
        &mut self,
        w: &World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        loot: &rtsim_industry::ids::GoodVec,
    ) -> i64 {
        let mut got = 0;
        for (g, n) in loot.iter() {
            let name = infra
                .good_name(g)
                .unwrap_or_else(|| format!("good {}", g.0));
            let short = name.rsplit(':').next().unwrap_or(&name).to_string();
            match w.goods.find(&name).or_else(|| w.goods.find(&short)) {
                Some(wg) => {
                    self.took_from_players(p, wg, n.saturating_mul(MILLI));
                    got += n;
                }
                None => *self.ledger.unmapped.entry(name).or_default() += n,
            }
        }
        self.stats.items_from_districts += got.max(0) as u64;
        got
    }

    /// A party reaches its target.
    fn arrive(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        at: Tick,
    ) {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let kind = self.parties[p].kind;
        let bend = self.temper(f).bend(Trait::Bold, 1000, 50);
        let patience =
            (days_to_ticks(c.def.patience_days) as f64 * f64::from(bend) / 1000.0) as u64;
        self.parties[p].bored_at = at + patience.max(1);
        let target = self.parties[p].target;
        let tpos = self.target_pos(w, target);
        match kind {
            PartyKind::Envoy => {
                let deadline = self.parties[p]
                    .feud
                    .and_then(|a| self.feud(f, a))
                    .and_then(|x| x.demand)
                    .map_or(at, |d| d.deadline);
                self.wait(w, p, deadline.max(at + 1), at);
                self.note(
                    at,
                    Some(f),
                    "envoy",
                    format!("the envoy waits at ({}, {})", tpos.x, tpos.z),
                );
            }
            PartyKind::Scouts => {
                self.wait(w, p, at + days_to_ticks(c.def.linger_days), at);
                self.note(
                    at,
                    Some(f),
                    "scouts",
                    format!("scouts watch ({}, {}) from the edge", tpos.x, tpos.z),
                );
            }
            _ if self.peaceful => {
                // Peaceful: the raid does nothing, and the band goes home (MOWZIE.md §4.12).
                self.stats.peace_held += 1;
                self.note(
                    at,
                    Some(f),
                    "peaceful",
                    format!("the {} does nothing: the world is Peaceful", kind.name()),
                );
                self.go_home(w, p, boundary::RECALL_DONE, at);
            }
            _ => {
                if let Some(player) = self.watcher(w, tpos, at) {
                    self.start_fight(w, p, player, at);
                } else {
                    if self.parties[p].embodied {
                        let epoch = self.parties[p].epoch;
                        self.recall(p, epoch, boundary::RECALL_PASSING, (tpos.x, tpos.z));
                    }
                    self.resolve(w, infra, p, at);
                    let alive = self.parties[p].alive();
                    if kind == PartyKind::Siege && alive > 0 && at < self.parties[p].bored_at {
                        let next = at + days_to_ticks(c.def.siege_interval_days);
                        self.wait(w, p, next, at);
                    } else {
                        self.go_home(w, p, boundary::RECALL_DONE, at);
                    }
                }
            }
        }
    }

    fn wait(&mut self, w: &mut World, p: Handle<Party>, until: Tick, at: Tick) {
        let until = until.min(self.parties[p].bored_at).max(at + 1);
        let x = &mut self.parties[p];
        x.phase = Phase::Waiting;
        x.until = until;
        let here = *x.route.last().expect("a route");
        self.book_leg(w, p, here, here, at, until);
        let plan = self.bump(p);
        self.schedule(until, Ev::Party { party: p, plan });
    }

    fn wait_over(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        at: Tick,
    ) {
        let x = &self.parties[p];
        let (kind, f) = (x.kind, x.faction);
        if kind == PartyKind::Siege && at < x.bored_at && x.alive() > 0 && !self.peaceful {
            let tpos = self.target_pos(w, x.target);
            if let Some(player) = self.watcher(w, tpos, at) {
                self.start_fight(w, p, player, at);
                return;
            }
            self.resolve(w, infra, p, at);
            if self.parties[p].alive() > 0 && at < self.parties[p].bored_at {
                let next = at + days_to_ticks(self.culture(f).def.siege_interval_days);
                self.wait(w, p, next, at);
                return;
            }
        }
        let reason = if at >= self.parties[p].bored_at {
            self.stats.bored += 1;
            if kind.is_war() {
                self.note(
                    at,
                    Some(f),
                    "bored",
                    "the band gets bored and goes home".into(),
                );
            }
            boundary::RECALL_BORED
        } else {
            boundary::RECALL_DONE
        };
        self.go_home(w, p, reason, at);
    }

    /// A player near enough to watch `pos` (reported recently enough).
    fn watcher(&self, w: &World, pos: Pos, at: Tick) -> Option<ActorId> {
        let r = f64::from(self.tun.view_radius);
        let stale = self.tun.player_stale_ticks;
        w.players
            .iter()
            .filter_map(|(a, s)| {
                let (pp, t) = s.pos?;
                (pp.frame == pos.frame && at.0.saturating_sub(t.0) <= stale)
                    .then(|| (tdist(self.topo.of(pos.frame), pxz(pp), pxz(pos)), *a))
            })
            .filter(|(d, _)| *d <= r)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|e| e.1)
    }

    /// An unwatched raid, resolved abstractly: through the industry threat interface for a
    /// district (C16 to C18), through the world's markets for a village or a haven.
    fn resolve(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        at: Tick,
    ) {
        let f = self.parties[p].faction;
        let target = self.parties[p].target;
        let before = self.parties[p].alive();
        let (stolen, blocks) = match target {
            Target::District(d, _) => self.raid_district(w, infra, p, d, at),
            Target::Node(i) => (self.raid_node(w, p, i, at), 0),
            Target::Pos(pos) => {
                // Nothing of the player's here that the sim knows: look for a district near it.
                let sites = infra.districts_near(pos.frame, i64::from(pos.x), i64::from(pos.z), 64);
                match sites.iter().find(|s| !s.watched) {
                    Some(s) => {
                        self.parties[p].target = Target::District(s.id, s.pos);
                        self.raid_district(w, infra, p, s.id, at)
                    }
                    None => {
                        self.note(at, Some(f), "raid", "finds nothing to raid".into());
                        (0, 0)
                    }
                }
            }
        };
        let (aim, after) = (self.target_pos(w, target), self.parties[p].alive());
        let what = match self.parties[p].target {
            Target::District(d, _) => format!("district {}", d.0),
            Target::Node(i) => format!("node {}", w.nodes[i].key),
            Target::Pos(_) => "nothing".to_string(),
        };
        let severity = if stolen == 0 && blocks == 0 {
            Severity::Minor
        } else {
            Severity::Major
        };
        let object = self.parties[p].feud.map_or(0, aid);
        self.tape
            .event_as(
                kinds::CONFLICT_RAID,
                severity,
                spot(at, aim),
                pid(p),
                object,
                stolen,
            )
            .text(|| {
                format!(
                    "{what}: took {stolen} items and {blocks} blocks; {} of {before} fell",
                    before.saturating_sub(after)
                )
            });
        // Vengeance: what it took eases the feud it served (capped).
        if let Some(a) = self.parties[p].feud {
            let c = self.culture(f);
            let lambda = c.lambda;
            let v = c.def.vengeance_per_item
                * (stolen as f64 + blocks as f64 * self.tun.block_value_items);
            let cap = self.tun.vengeance_cap_share;
            if let Some(feud) = self.feud_mut(f, a) {
                let g = feud.g.total_at(at, lambda);
                feud.g.relieve(v.min(g * cap), at, lambda);
            }
        }
    }

    fn raid_district(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        p: Handle<Party>,
        d: DistrictId,
        at: Tick,
    ) -> (i64, u32) {
        let f = self.parties[p].faction;
        let classes = self.strength_by_class(p);
        let mut stolen_items = 0i64;
        let mut blocks = 0u32;
        let mut words = Vec::new();
        for (class, (count, strength, loads)) in classes {
            let id = fnv1a64(
                &[
                    Party::agent(p).to_le_bytes(),
                    at.0.to_le_bytes(),
                    (class.index() as u64).to_le_bytes(),
                ]
                .concat(),
            );
            let t = Threat {
                id,
                source: self.culture(f).def.source.threat(),
                class,
                count,
                strength,
                target: ThreatTarget::District(d),
                at,
                loads: Some(loads),
            };
            let Some(o) = infra.raid(&t) else {
                continue;
            };
            if let Some(why) = o.skipped {
                self.stats.raids_skipped += 1;
                words.push(format!("{class:?} x{count}: skipped ({why})"));
                continue;
            }
            self.stats.raids_district += 1;
            // Those held at the walls are not all dead: a share of the ones that did not make it
            // fall (choice K2-12).
            let missing = f64::from(count) * (1.0 - o.survived);
            let lost = ((missing * self.tun.abstract_loss_share).round() as u32).min(count);
            self.lose_units(p, |c| c.threat() == class, lost);
            let got = self.take_industry_loot(w, infra, p, &o.stolen);
            stolen_items += got;
            blocks += o.destroyed_blocks;
            self.stats.blocks_destroyed += u64::from(o.destroyed_blocks);
            words.push(format!(
                "{class:?} x{count}: {lost} fall (survived {:.2}{}{}), {got} items, {} blocks{}",
                o.survived,
                o.floor
                    .map_or(String::new(), |fl| format!(", floor {fl:.2}")),
                if o.untested {
                    ", untested defences"
                } else {
                    ""
                },
                o.destroyed_blocks,
                if o.machine_lost {
                    ", a machine lost"
                } else {
                    ""
                }
            ));
        }
        self.note(
            at,
            Some(f),
            "raid",
            format!("raids district {} unwatched: {}", d.0, words.join("; ")),
        );
        (stolen_items, blocks)
    }

    /// What party `p`'s living members carry between them, thousandths, each unit as its chief's
    /// crystal has it fight now (`unit_row`, MOWZIE.md ledger 9).
    fn carry_milli(&self, p: Handle<Party>) -> Amount {
        let x = &self.parties[p];
        x.units
            .iter()
            .map(|&(u, n)| i64::from(n) * i64::from(self.unit_row(x.faction, u).carry))
            .sum::<i64>()
            * MILLI
    }

    /// Takes `n` units away from the stacks whose class matches, in stack order.
    fn lose_units(
        &mut self,
        p: Handle<Party>,
        class: impl Fn(crate::culture::UnitClass) -> bool,
        mut n: u32,
    ) {
        let f = self.parties[p].faction;
        let classes: Vec<bool> = self.parties[p]
            .units
            .iter()
            .map(|&(u, _)| class(self.unit_row(f, u).class))
            .collect();
        let x = &mut self.parties[p];
        let mut lost = 0;
        for (k, e) in x.units.iter_mut().enumerate() {
            if n == 0 {
                break;
            }
            if classes[k] {
                let t = e.1.min(n);
                e.1 -= t;
                n -= t;
                lost += t;
                self.stats.casualties += u64::from(t);
            }
        }
        self.party_lost(p, lost, self.now);
    }

    /// A raid on a village's or a rival haven's market: Lanchester's linear law against the
    /// residents' defence, floored by real fights seen (never better for the defender than
    /// observed); the survivors carry off up to their carry and the culture's share of each line's
    /// surplus over its target, coins included, into the band's hold (`World::stock_to_held`),
    /// conservation exact. A band that could not carry off one whole item if nobody fell does not
    /// fight at all (CL-D58, as the ports' PC72 and PC78). A culture whose loot is destroyed takes
    /// it out of the world where it stands (`World::take_stocks`, MOWZIE.md ledger 10), from the
    /// same capped shares. A lair defends as a haven does (MZB-59) and has no stock to take.
    fn raid_node(&mut self, w: &mut World, p: Handle<Party>, i: usize, at: Tick) -> i64 {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        // Something to take, before any blood is spilled: a raid takes from each line's surplus
        // over its target only. Below its target a line needs imports or a mint to refill, and a
        // quiet economy sends neither, so whole-stock raids drained villages for good (CL-D58).
        let m = w.market(i);
        let caps: Vec<(GoodId, Amount)> = w
            .goods
            .ids()
            .map(|g| {
                let surplus = (m.stock(g) - m.line(g).target).max(0);
                (g, surplus * c.def.loot_share_milli / 1000)
            })
            .filter(|e| e.1 > 0)
            .collect();
        let reach: Amount = caps.iter().map(|e| e.1).sum();
        if self.carry_milli(p).min(reach) < MILLI {
            self.note(
                at,
                Some(f),
                "plunder",
                format!(
                    "finds nothing worth taking at node {}: {} items spare within reach",
                    w.nodes[i].key,
                    reach / MILLI
                ),
            );
            return 0;
        }
        let s = self.strength(p);
        let node = &w.nodes[i];
        let per = match node.kind {
            NodeKind::Haven(_) | NodeKind::Lair => self.tun.haven_defence_per_pop,
            _ => self.tun.village_defence_per_pop,
        };
        let pop = w.settlements.get(node.settlement).map_or(0, |x| x.pop);
        let d = f64::from(pop) * per;
        let (survived, floor) = village_outcome(
            s,
            d,
            &self.factions[f as usize].observed,
            self.tun.comparable_strength_factor,
        );
        let alive = self.parties[p].alive();
        let lost = alive - ((f64::from(alive) * survived).round() as u32).min(alive);
        self.lose_units(p, |_| true, lost);
        self.stats.raids_village += 1;
        // The survivors carry off what they can of the caps read before the fight (the fight takes
        // no time, so the market has not moved).
        let shares = apportion(self.carry_milli(p).min(reach), &caps);
        let want: BTreeMap<GoodId, Amount> = shares.into_iter().filter(|e| e.1 > 0).collect();
        let got = if c.def.loot_destroyed {
            w.take_stocks(i, &want, at)
        } else {
            w.stock_to_held(i, &want, at)
        };
        let mut items = 0;
        for (g, a) in &got {
            if c.def.loot_destroyed {
                add(&mut self.ledger.destroyed, *g, *a);
            } else {
                *self.parties[p].loot.entry(*g).or_default() += a;
            }
            add(&mut self.ledger.from_villages, *g, *a);
            items += a / MILLI;
        }
        self.stats.items_from_villages += items.max(0) as u64;
        // The village remembers who robbed it, at the kind's base delta (severity on the knowledge
        // layer's unit, as every module records its robberies: MZB-36).
        let fa = self.factions[f as usize].actor;
        let va = w.nodes[i].actor;
        let pos = w.nodes[i].pos;
        let r = w.knowledge.record(Report {
            who: fa,
            whom: va,
            kind: ReportKind::Robbery,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place: Place::new(pos.frame, pos.x, 64, pos.z),
        });
        w.knowledge.witness(va, r, at);
        self.note(
            at,
            Some(f),
            "plunder",
            format!(
                "raids node {} (defence {d:.1} against {s:.1}): {lost} fall, carries off {items} items{}",
                w.nodes[i].key,
                floor.map_or(String::new(), |fl| format!(" (floor {fl:.2})"))
            ),
        );
        items
    }

    /// Back to the camp along the road, from where it is now.
    fn go_home(&mut self, w: &mut World, p: Handle<Party>, reason: u32, at: Tick) {
        let here = self.party_pos(p, at);
        let f = self.parties[p].faction;
        if self.parties[p].embodied {
            let camp = self.factions[f as usize].pos;
            let epoch = self.parties[p].epoch;
            self.recall(p, epoch, reason, (camp.x, camp.z));
        }
        let mut route = self.parties[p].route.clone();
        let s = (self.parties[p].seg as usize + 1).min(route.len());
        route.truncate(s);
        route.reverse();
        if route.first() != Some(&here) {
            route.insert(0, here);
        }
        if route.len() < 2 {
            route.push(here);
        }
        let times = self.times_for(f, &route, at);
        let x = &mut self.parties[p];
        x.route = route;
        x.times = times;
        x.seg = 0;
        x.phase = Phase::Home;
        x.feud = x.feud.filter(|_| x.kind != PartyKind::Envoy);
        let (a, b, t1) = (x.route[0], x.route[1], x.times[1]);
        self.book_leg(w, p, a, b, at, t1);
        let plan = self.bump(p);
        self.schedule(t1, Ev::Party { party: p, plan });
        // The feud it served may move on while it walks home.
        if let Some(a) = self.parties[p].feud {
            if let Some(feud) = self.feud_mut(f, a)
                && feud.party == Some(p)
            {
                feud.party = None;
                feud.since = at;
            }
            self.plan_feud(f, a, at, false);
        }
    }

    /// Despawns the party's bodies. From here the engine's counts are the truth; the survivors the
    /// plugin reports can only lower them (those that died in Minecraft on the way out).
    fn recall(&mut self, p: Handle<Party>, epoch: u32, reason: u32, toward: (i32, i32)) {
        self.stats.recalls += 1;
        let x = &mut self.parties[p];
        if x.embodied && x.epoch == epoch {
            let s = std::mem::take(&mut x.spawned);
            for (u, n) in s {
                match x.recalled.iter_mut().find(|e| e.0 == u) {
                    Some(e) => e.1 += n,
                    None => x.recalled.push((u, n)),
                }
            }
            x.embodied = false;
            x.seen_by = None;
        }
        self.outbox.push(Cmd::Recall(boundary::BandRecall {
            agent: Party::agent(p),
            epoch,
            reason,
            timeout_ticks: self.tun.recall_timeout_ticks,
            toward: (f64::from(toward.0), f64::from(toward.1)),
        }));
    }

    /// Home: loot stocked at the haven (C24), survivors back to the pool.
    fn home(&mut self, w: &mut World, p: Handle<Party>, at: Tick) {
        let f = self.parties[p].faction;
        let node = self.factions[f as usize].node;
        let loot = std::mem::take(&mut self.parties[p].loot);
        let mut items = 0;
        if !loot.is_empty() {
            let stocked = w.land_held(node, None, &loot, &BTreeMap::new(), at);
            for (g, a) in &stocked {
                add(&mut self.ledger.landed, *g, *a);
                items += a / MILLI;
            }
            let mut left = loot.clone();
            for (g, a) in &stocked {
                *left.get_mut(g).expect("landed") -= a;
            }
            left.retain(|_, a| *a > 0);
            self.parties[p].loot = left;
        }
        let fp = std::mem::take(&mut self.parties[p].from_players);
        if !fp.is_empty() {
            for (g, a) in w.stock_from_players(node, &fp, at) {
                add(&mut self.ledger.landed_from_players, g, a);
                items += a / MILLI;
            }
        }
        let x = &self.parties[p];
        let (kind, alive, chief) = (x.kind, x.alive(), x.chief);
        let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
        self.tape
            .event_as(
                kinds::CONFLICT_PARTY_HOME,
                party_severity(kind),
                spot(at, camp),
                pid(p),
                fid,
                items,
            )
            .text(|| format!("{} home: {alive} left, {items} items", kind.name()));
        if kind.is_war() {
            self.camp_home(p, at);
            self.pool_return(f, alive, at);
            self.unblock(f, at);
        }
        if chief {
            self.factions[f as usize].chief_out = false;
        }
        if let Some(l) = self.parties[p].leg.take() {
            w.legs.cancel(l.leg);
        }
        if let Some(a) = self.parties[p].feud
            && let Some(feud) = self.feud_mut(f, a)
            && feud.party == Some(p)
        {
            feud.party = None;
            feud.since = at;
            self.plan_feud(f, a, at, false);
        } else if let Some(a) = self.parties[p].feud
            && self.culture(f).def.repeat_days.is_some()
            && let Some(feud) = self.feud_mut(f, a)
            && feud.party.is_none()
            && feud.demand.is_none()
            && PartyKind::of_step(feud.step) == Some(kind)
        {
            // A repeat counts from the arrival home (MZB-51): the rung it served starts again
            // now, so a replan cannot book the repeat from the turn.
            feud.since = feud.since.max(at);
            self.plan_feud(f, a, at, false);
        }
        if self.parties[p].loot.is_empty() {
            self.parties.remove(p);
        } else {
            // A haven with no storehouse (never, by construction): keep the row so the goods
            // stay accounted for.
            self.parties[p].phase = Phase::Waiting;
        }
        self.note(
            at,
            Some(f),
            "home",
            format!(
                "{} home with {alive} left and {items} items for the haven",
                kind.name()
            ),
        );
        if kind == PartyKind::Plunder {
            self.schedule_plunder(f, at);
        }
    }

    // ---- plunder (C24: havens) -------------------------------------------------------------

    fn schedule_plunder(&mut self, f: u32, at: Tick) {
        let Some(mean) = self.culture(f).def.plunder_mean_days else {
            return;
        };
        let bend = self.temper(f).bend(Trait::Aggressive, 1000, 50).max(1);
        let mean = mean * 1000.0 / f64::from(bend);
        let x = &mut self.factions[f as usize];
        x.plunders += 1;
        x.plunder_stamp = x.plunder_stamp.wrapping_add(1);
        let (key, n, stamp) = (x.key, x.plunders, x.plunder_stamp);
        let u = StreamKey::new(self.seed, SYS_PLUNDER, key)
            .slot(u64::from(n))
            .next_f64_open();
        let gap = (-u.ln() * mean * TICKS_PER_DAY as f64).ceil().max(1.0) as u64;
        self.schedule(at + gap, Ev::Plunder { faction: f, stamp });
    }

    fn plunder(&mut self, w: &mut World, f: u32, at: Tick) {
        if self.peaceful {
            // No war party leaves on Peaceful: the drive tries again later.
            self.stats.peace_held += 1;
            self.schedule_plunder(f, at);
            return;
        }
        self.ensure_chief(w, f, at);
        let c = self.culture(f).clone();
        let camp = self.factions[f as usize].pos;
        let reach = f64::from(c.def.plunder_reach_blocks);
        let own = self.factions[f as usize].node;
        self.nodes.refresh(w);
        let by_key = &self.by_key;
        let cultures = &self.cultures;
        let factions = &self.factions;
        let topo = self.topo.clone();
        let best = self
            .nodes
            .nearest(w, topo.of(camp.frame), camp, reach, |i| {
                let n = &w.nodes[i];
                if i == own || n.pos.frame != camp.frame {
                    return false;
                }
                match n.kind {
                    NodeKind::Village(cu) => c.hates(cu.name()),
                    NodeKind::Haven(_) => by_key.get(&n.key).is_some_and(|&o| {
                        c.hates(cultures.get(factions[o as usize].culture).name())
                    }),
                    // A lair has nothing to plunder (MOWZIE.md ledger 13).
                    NodeKind::PlayerBase { .. } | NodeKind::Lair => false,
                }
            });
        let Some((_, i)) = best else {
            self.schedule_plunder(f, at);
            return;
        };
        if self
            .send_party(w, f, None, PartyKind::Plunder, Target::Node(i), at)
            .is_none()
        {
            self.schedule_plunder(f, at);
        }
    }

    // ---- watched: the raid director --------------------------------------------------------

    /// A band arrives where a player watches: it materializes in waves on an arc at the edge of
    /// the entity-ticking area, on the side it came from, under the spawn quota.
    fn start_fight(&mut self, w: &mut World, p: Handle<Party>, player: ActorId, at: Tick) {
        let f = self.parties[p].faction;
        self.stats.raids_watched += 1;
        let ppos = w.players.get(&player).and_then(|s| s.pos).map(|e| e.0);
        let x = &self.parties[p];
        let from = if x.route.len() >= 2 {
            x.route[x.route.len() - 2]
        } else {
            x.route[0]
        };
        let centre = ppos.map_or(
            {
                let l = *x.route.last().expect("route");
                self.topo(x.frame).canonical_i32(l.0, l.1)
            },
            |pp| (pp.x, pp.z),
        );
        // The side it came from, through any seam (TP2).
        let (bx, bz) = self.topo(x.frame).displacement(
            (i64::from(centre.0), i64::from(centre.1)),
            (i64::from(from.0), i64::from(from.1)),
        );
        let bearing = (bz as f64).atan2(bx as f64).to_degrees() as f32;
        let ends = (at + self.tun.watched_fight_ticks).min(x.bored_at.max(at + 1));
        let strength = self.strength(p);
        {
            let x = &mut self.parties[p];
            x.phase = Phase::Fighting;
            x.until = ends;
            x.fight_strength = strength;
            x.epoch += 1;
            x.embodied = true;
            x.seen_by = Some(player);
            x.wave = 0;
            x.spawned.clear();
        }
        let here = *self.parties[p].route.last().expect("route");
        self.book_leg(w, p, here, here, at, ends);
        let plan = self.bump(p);
        self.schedule(ends, Ev::Party { party: p, plan });
        self.spawn_wave(w, p, player, Some((centre, bearing)), at);
        self.schedule(
            at + self.tun.wave_interval_ticks,
            Ev::Wave { party: p, plan },
        );
        self.note(
            at,
            Some(f),
            "director",
            format!(
                "{} arrives where a player watches: waves on an arc {} blocks out at {bearing:.0} degrees",
                self.parties[p].kind.name(),
                self.tun.spawn_edge_blocks
            ),
        );
    }

    /// One wave (or the whole group when seen on the march): members up to the quota that are
    /// not in Minecraft yet.
    fn spawn_wave(
        &mut self,
        w: &World,
        p: Handle<Party>,
        player: ActorId,
        arc: Option<((i32, i32), f32)>,
        at: Tick,
    ) {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let x = &self.parties[p];
        let in_mc: u32 = x.spawned.iter().map(|e| e.1).sum();
        let mut room = self.tun.spawn_quota.saturating_sub(in_mc);
        let mut stacks = Vec::new();
        let mut add_spawned = Vec::new();
        for &(u, n) in &x.units {
            let already = x.spawned.iter().find(|e| e.0 == u).map_or(0, |e| e.1);
            let t = n.saturating_sub(already).min(room);
            if t > 0 {
                room -= t;
                stacks.push(boundary::Stack {
                    body: body_id(&self.cultures, &c.unit(u).body),
                    count: t,
                    flags: if x.chief && stacks.is_empty() && x.wave == 0 {
                        boundary::MEMBER_CHIEF
                    } else {
                        0
                    },
                });
                add_spawned.push((u, t));
            }
        }
        if stacks.is_empty() {
            return;
        }
        let here = self.party_pos(p, at);
        // Seen on the march: at the image on the player's side (TP11).
        let here = match w.players.get(&player).and_then(|s| s.pos) {
            Some((pp, _)) if pp.frame == self.parties[p].frame => {
                let i = self.topo(pp.frame).toward(
                    (i64::from(pp.x), i64::from(pp.z)),
                    (i64::from(here.0), i64::from(here.1)),
                );
                (i.0 as i32, i.1 as i32)
            }
            _ => here,
        };
        let (centre, bearing, radius, half) = match arc {
            Some((c0, b)) => (
                c0,
                b,
                self.tun.spawn_edge_blocks,
                self.tun.arc_half_width_deg,
            ),
            None => (here, 0.0, 0, 0.0),
        };
        let x = &self.parties[p];
        let tpos = self.target_pos(w, x.target);
        let cmd = Cmd::Materialize(boundary::BandMaterialize {
            agent: Party::agent(p),
            epoch: x.epoch,
            dimension: u32::from(x.frame),
            faction: self.factions[f as usize].key,
            purpose: x.kind as u8,
            flags: (if x.kind.is_war() {
                boundary::BAND_HOSTILE
            } else {
                0
            }) | boundary::BAND_SURFACE
                | if x.chief { boundary::BAND_CHIEF } else { 0 },
            wave: x.wave,
            arc_centre: (f64::from(centre.0), f64::from(centre.1)),
            bearing_deg: bearing,
            arc_radius: radius,
            half_width_deg: half,
            target: (f64::from(tpos.x), 64.0, f64::from(tpos.z)),
            target_player: self.uuid_of(w, player),
            speed: c.def.march_blocks_per_tick as f32,
            members: stacks,
        });
        self.outbox.push(cmd);
        let x = &mut self.parties[p];
        for (u, t) in add_spawned {
            match x.spawned.iter_mut().find(|e| e.0 == u) {
                Some(e) => e.1 += t,
                None => x.spawned.push((u, t)),
            }
        }
        x.wave += 1;
        self.stats.waves += 1;
        self.stats.materialized += 1;
    }

    fn wave(&mut self, w: &World, p: Handle<Party>, at: Tick) {
        let x = &self.parties[p];
        // On Peaceful no war body spawns (MZB-15).
        if x.phase != Phase::Fighting || (self.peaceful && x.kind.is_war()) {
            return;
        }
        let Some(player) = x.seen_by else {
            return;
        };
        let alive = x.alive();
        let in_mc: u32 = x.spawned.iter().map(|e| e.1).sum();
        if alive > in_mc {
            let ppos = w
                .players
                .get(&player)
                .and_then(|s| s.pos)
                .map(|e| (e.0.x, e.0.z));
            let from = x.route[x.route.len().saturating_sub(2)];
            let centre = ppos.unwrap_or_else(|| {
                let l = *x.route.last().expect("route");
                self.topo(x.frame).canonical_i32(l.0, l.1)
            });
            let (bx, bz) = self.topo(x.frame).displacement(
                (i64::from(centre.0), i64::from(centre.1)),
                (i64::from(from.0), i64::from(from.1)),
            );
            let bearing = (bz as f64).atan2(bx as f64).to_degrees() as f32;
            self.spawn_wave(w, p, player, Some((centre, bearing)), at);
        }
        let plan = self.parties[p].plan;
        self.schedule(
            at + self.tun.wave_interval_ticks,
            Ev::Wave { party: p, plan },
        );
    }

    /// A watched fight ends: survivors withdraw (the plugin despawns them and reports who is left),
    /// the outcome joins the faction's observed fights, and the band goes home.
    fn end_fight(&mut self, w: &mut World, p: Handle<Party>, reason: u32, at: Tick) {
        let f = self.parties[p].faction;
        let x = &self.parties[p];
        let survived = f64::from(x.alive()) / f64::from(x.start_units.max(1));
        let strength = x.fight_strength;
        let obs = &mut self.factions[f as usize].observed;
        obs.push((strength, survived));
        if obs.len() > 16 {
            obs.remove(0);
        }
        if reason == boundary::RECALL_ROUTED {
            self.stats.routs += 1;
            if let Some(player) = self.parties[p].breaker {
                self.rescue_news(w, p, player, at);
            }
        }
        if reason == boundary::RECALL_BORED {
            self.stats.bored += 1;
        }
        let (x, z) = self.party_pos(p, at);
        let x_ = &self.parties[p];
        let (frame, alive, start) = (u32::from(x_.frame), x_.alive(), x_.start_units);
        let object = x_.breaker.map_or(0, aid);
        self.tape
            .event(
                kinds::CONFLICT_FIGHT,
                At::new(at.0, frame, x, z),
                pid(p),
                object,
                i64::from(alive),
            )
            .text(|| {
                let how = match reason {
                    boundary::RECALL_ROUTED => "routed",
                    boundary::RECALL_BORED => "bored",
                    _ => "withdraws",
                };
                format!("{how}: {alive} of {start} left")
            });
        self.note(
            at,
            Some(f),
            "fight",
            format!(
                "the watched fight ends ({}): {} of {} left",
                match reason {
                    boundary::RECALL_ROUTED => "routed",
                    boundary::RECALL_BORED => "bored",
                    _ => "withdraws",
                },
                self.parties[p].alive(),
                self.parties[p].start_units
            ),
        );
        self.go_home(w, p, reason, at);
    }

    /// Villages that hate the faction hear that a player broke its band (C24: rescues count). A
    /// rout is one report (K2-25, CL-D60): every village in range whose culture the faction hates
    /// witnesses it, and the others hear it only as news. Knowledge dedups per report, so a report
    /// per witness made each village count the rout once per witness. The report names whom the
    /// band was after (the node it marched on, else the feud's actor unless that is the breaker,
    /// else no one) and the place it broke.
    fn rescue_news(&mut self, w: &mut World, p: Handle<Party>, player: ActorId, at: Tick) {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let camp = self.factions[f as usize].pos;
        let r = f64::from(self.tun.rescue_news_blocks);
        // The villages in news range: the node index's cells around the camp, in node order.
        self.nodes.refresh(w);
        let topo = self.topo.clone();
        let near = self.nodes.within(w, topo.of(camp.frame), camp, r);
        let witnesses: Vec<ActorId> = near
            .into_iter()
            .filter_map(|i| {
                let n = &w.nodes[i];
                let NodeKind::Village(cu) = n.kind else {
                    return None;
                };
                (c.hates(cu.name())
                    && n.pos.frame == camp.frame
                    && tdist(self.topo.of(camp.frame), pxz(n.pos), pxz(camp)) <= r)
                    .then_some(n.actor)
            })
            .collect();
        let mut heard = 0;
        if !witnesses.is_empty() {
            let x = &self.parties[p];
            let whom = match x.target {
                Target::Node(i) => w.nodes.get(i).map_or(ActorId::NULL, |n| n.actor),
                _ => x.feud.filter(|a| *a != player).unwrap_or(ActorId::NULL),
            };
            let frame = x.frame;
            let (bx, bz) = self.party_pos(p, at);
            let rep = w.knowledge.record(Report {
                who: player,
                whom,
                kind: ReportKind::Rescue,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: Place::new(frame, bx, 64, bz),
            });
            for va in witnesses {
                if w.knowledge.witness(va, rep, at) {
                    heard += 1;
                }
            }
        }
        self.note(
            at,
            Some(f),
            "rescue",
            format!(
                "{heard} village(s) that hate the {} hear the band was broken",
                c.name()
            ),
        );
    }

    // ---- seen on the march -----------------------------------------------------------------

    fn seen(&mut self, w: &mut World, p: Handle<Party>, player: ActorId, m: Meeting, at: Tick) {
        if !w.legs.is_meeting_live(&m) || !self.parties.contains(p) {
            self.stats.stale += 1;
            return;
        }
        let x = &self.parties[p];
        if x.embodied
            || matches!(x.phase, Phase::Home | Phase::Mustering | Phase::Fighting)
            || (self.peaceful && x.kind.is_war())
        {
            return;
        }
        self.stats.seen += 1;
        let f = x.faction;
        {
            let x = &mut self.parties[p];
            x.epoch += 1;
            x.embodied = true;
            x.seen_by = Some(player);
            x.spawned.clear();
            x.wave = 0;
        }
        if self.parties[p].kind == PartyKind::Envoy {
            self.materialize_envoy(w, p, player, at);
        } else {
            self.spawn_wave(w, p, player, None, at);
        }
        let x = &self.parties[p];
        if x.phase == Phase::Out {
            let s = x.seg as usize;
            let next = x.route[s + 1];
            let arrive = x.times[s + 1];
            let epoch = x.epoch;
            let speed = self.culture(f).def.march_blocks_per_tick;
            self.outbox.push(Cmd::Steer(boundary::BandSteer {
                agent: Party::agent(p),
                epoch,
                flags: 0,
                speed: speed as f32,
                pos: (f64::from(next.0), 64.0, f64::from(next.1)),
                arrive: arrive.0 as i64,
            }));
        }
        // Passing through: it leaves view where its walk leaves the area, unless it stops here.
        self.book_unseen(w, p, at);
        let kind = self.parties[p].kind;
        self.note(
            at,
            Some(f),
            "seen",
            format!("{} seen by a player", kind.name()),
        );
    }

    fn materialize_envoy(&mut self, w: &World, p: Handle<Party>, player: ActorId, at: Tick) {
        let x = &self.parties[p];
        let f = x.faction;
        let c = self.culture(f).clone();
        let here = self.party_pos(p, at);
        let demand = x
            .feud
            .and_then(|a| self.feud(f, a))
            .and_then(|fd| fd.demand);
        let whom = x.feud.map_or((0, 0), |a| self.uuid_of(w, a));
        let members: Vec<(u32, u32)> = x
            .units
            .iter()
            .map(|&(u, n)| (body_id(&self.cultures, &c.unit(u).body), n))
            .collect();
        let (flags, signal, item, count, deadline) = match demand {
            Some(d) => (
                boundary::DEMAND_TRIBUTE | boundary::DEMAND_STOP,
                d.kind as u32,
                item_of(d.good),
                (d.tribute - d.paid).clamp(0, i64::from(i32::MAX)) as i32,
                d.deadline.0 as i64,
            ),
            None => (0, 0, 0, 0, 0),
        };
        let _ = player;
        self.outbox.push(Cmd::Envoy(boundary::EnvoyMaterialize {
            agent: Party::agent(p),
            epoch: x.epoch,
            dimension: u32::from(x.frame),
            faction: self.factions[f as usize].key,
            pos: (f64::from(here.0), 64.0, f64::from(here.1)),
            player: whom,
            demand_flags: flags,
            signal,
            tribute_item: item,
            tribute_count: count,
            deadline,
            members,
        }));
        self.stats.materialized += 1;
    }

    fn unseen(&mut self, p: Handle<Party>, epoch: u32, at: Tick) {
        let Some(x) = self.parties.get(p) else {
            return;
        };
        if !x.embodied || x.epoch != epoch || x.phase != Phase::Out {
            return;
        }
        let next = x.route[(x.seg as usize + 1).min(x.route.len() - 1)];
        self.recall(p, epoch, boundary::RECALL_PASSING, next);
        let _ = at;
    }

    // ---- the run loop -----------------------------------------------------------------------

    fn run_to(&mut self, at: Tick) {
        self.now = self.now.max(at);
    }

    /// Runs every event up to and including `to`, the world run to each event's time first.
    /// Paged: each event is a step boundary (no row is lent between events), so the row tables
    /// keep to their budget after each one ([`Conflict::enforce_budget`]).
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            self.now = self.now.max(at);
            w.run_until(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
            if self.store.is_some() {
                self.enforce_budget();
            }
        }
        self.now = self.now.max(to);
        self.enforce_budget();
    }

    fn handle(&mut self, w: &mut World, infra: &mut dyn Infrastructure, at: Tick, ev: Ev) {
        match ev {
            Ev::Feud {
                faction,
                actor,
                stamp,
            } => {
                if self.feud(faction, actor).is_none_or(|f| f.stamp != stamp) {
                    self.stats.stale += 1;
                    return;
                }
                self.feud_wake(w, faction, actor, at);
            }
            Ev::Party { party, plan } => {
                if self.parties.get(party).is_none_or(|x| x.plan != plan) {
                    self.stats.stale += 1;
                    return;
                }
                self.party_event(w, infra, party, at);
            }
            Ev::Seen {
                party,
                player,
                meeting,
            } => self.seen(w, party, player, meeting, at),
            Ev::Unseen { party, epoch } => self.unseen(party, epoch, at),
            Ev::Wave { party, plan } => {
                if self.parties.get(party).is_none_or(|x| x.plan != plan) {
                    return;
                }
                self.wave(w, party, at);
            }
            Ev::Ambush { party, plan, train } => {
                if self.parties.get(party).is_none_or(|x| x.plan != plan) {
                    return;
                }
                self.ambush(infra, party, train, at, w);
            }
            Ev::Plunder { faction, stamp } => {
                if self.factions[faction as usize].plunder_stamp != stamp {
                    return;
                }
                self.plunder(w, faction, at);
            }
            Ev::Refill { faction, stamp } => self.camp_refill(faction, stamp, at),
        }
    }

    // ---- players ----------------------------------------------------------------------------

    /// Where a player is, through the world's shared interest area (choice U2): the area is kept
    /// or booked anew and routed to this engine.
    pub fn observe_player(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) -> ActorId {
        self.observe_presence(w, uuid, pos, at);
        let (p, stamp) = w.observe_area(uuid, pos, at, self.tun.view_radius);
        self.players.insert(p, uuid);
        if let Some(s) = stamp {
            let _ = w.route_area(p, s, &mut [self]);
        }
        p
    }

    /// The player left (logged out): its area goes, and its parties withdraw.
    pub fn player_left(&mut self, w: &mut World, p: ActorId) {
        w.leave_area(p, &mut [self]);
    }

    /// A session ended: every puppet is gone (they never outlive a session, S11), a watched
    /// fight with nobody watching goes on abstractly at its next event (as when its player
    /// leaves), and the plugin hears every faction's territory again (docs/boundary.md §9.4).
    /// The kept notices go again later, from [`Conflict::resend_notices`], once the session's
    /// replay has caught up (MZB-31).
    pub fn session_start(&mut self) {
        let ids: Vec<Handle<Party>> = self.parties.iter().map(|(h, _)| h).collect();
        for p in ids {
            let x = &mut self.parties[p];
            x.embodied = false;
            x.seen_by = None;
            x.spawned.clear();
            x.recalled.clear();
            if x.phase == Phase::Fighting {
                x.phase = Phase::Waiting;
            }
        }
        // Per session: a camp's reconcile in flight is void (MOWZIE.md §4.9); the notices go
        // again from what is kept, after the replay (`resend_notices`).
        self.outbox.retain(|c| {
            c.agent().is_none()
                && !matches!(c, Cmd::Territory(_) | Cmd::Notice(_) | Cmd::Reconcile(_))
        });
        // Every faction's territory, once a session: page by page, keeping to the budget (PM-C8).
        let per = crate::tables::FACTIONS_PER_PAGE as u32;
        for f in 0..self.factions.len() as u32 {
            self.camp_session_start(f);
            let t = self.territory(f);
            self.outbox.push(t);
            if (f + 1) % per == 0 {
                self.enforce_budget();
            }
        }
        self.enforce_budget();
    }

    /// Every notice of a feud not played yet goes to the plugin again, once a session (MOWZIE.md
    /// §4.11). The host calls it after the session's replay has caught up, so a NOTICE_SHOWN
    /// journaled last session and not yet in the pages has cleared its notice first (MZB-31). A
    /// notice still queued (one a replayed record made again) is dropped first: what is kept is
    /// what goes. Page by page, keeping to the budget (PM-C8).
    pub fn resend_notices(&mut self) {
        self.outbox.retain(|c| !matches!(c, Cmd::Notice(_)));
        let per = crate::tables::FACTIONS_PER_PAGE as u32;
        for f in 0..self.factions.len() as u32 {
            let kept: Vec<Cmd> = self
                .feuds_of(f)
                .iter()
                .filter_map(|(_, x)| x.notice.as_deref().map(|n| Cmd::Notice(*n)))
                .collect();
            self.outbox.extend(kept);
            if (f + 1) % per == 0 {
                self.enforce_budget();
            }
        }
        self.enforce_budget();
    }

    /// The world's difficulty (DIFFICULTY, MOWZIE.md §4.12). While it is Peaceful no war party
    /// leaves (`plan_feud` books no climb to a war rung, a muster's end stands down), and
    /// ambushes, raids and plunder do nothing; feuds keep their grievance and decay as usual.
    ///
    /// On entering Peaceful every war party already out stops (MZB-15): one on its way or waiting
    /// at its target goes home, a watched fight ends, both with RECALL_SETTLED; one that has not
    /// arrived steps its feud back a rung, as a muster that stands down does.
    ///
    /// When it ends, a feud held below a war rung has its dwell clock paused over the Peaceful
    /// span, then pushed back by a jitter of its own in [0, that rung's dwell) (MZB-16), and
    /// every feud is planned again: the held climbs come one by one, never in one burst, and none
    /// before its dwell. Applied at the engine's present; an unchanged value changes nothing.
    pub fn set_difficulty(&mut self, w: &mut World, difficulty: u32) {
        let peaceful = difficulty == boundary::DIFFICULTY_PEACEFUL;
        if peaceful == self.peaceful {
            return;
        }
        self.peaceful = peaceful;
        let at = self.now;
        self.note(
            at,
            None,
            "difficulty",
            if peaceful {
                "the world is Peaceful: no war party leaves".into()
            } else {
                "the world is no longer Peaceful".into()
            },
        );
        if peaceful {
            self.peaceful_since = at;
            self.stop_war_parties(w, at);
            return;
        }
        let since = self.peaceful_since;
        let per = crate::tables::FACTIONS_PER_PAGE as u32;
        for f in 0..self.factions.len() as u32 {
            let key = self.factions[f as usize].key;
            for a in self.feud_actors(f) {
                let dwell = {
                    let x = self.feud(f, a).expect("listed");
                    self.culture(f).dwell_ticks[x.step.index()]
                };
                let jitter = if dwell == 0 {
                    0
                } else {
                    StreamKey::new(self.seed, SYS_PEACE, key)
                        .slot(a.to_bits())
                        .below(dwell)
                };
                // Held: a climb to a war rung (the culture's next rung up, MZB-50), or the repeat
                // of the war rung it is on (MZB-51).
                let (step, repeats) = {
                    let x = self.feud(f, a).expect("listed");
                    (x.step, self.culture(f).def.repeat_days.is_some())
                };
                let war_next = self.culture(f).next_up(step).is_some_and(Step::is_war)
                    || (repeats && matches!(step, Step::Raid | Step::Invasion));
                let x = self.feud_mut(f, a).expect("listed");
                let held = x.party.is_none() && x.demand.is_none() && war_next;
                if held {
                    let paused = x.since + at.since(x.since.max(since));
                    x.since = paused.max(Tick(at.0.saturating_sub(dwell))) + jitter;
                }
                self.plan_feud(f, a, at, false);
            }
            if (f + 1) % per == 0 {
                self.enforce_budget();
            }
        }
        self.enforce_budget();
    }

    /// Entering Peaceful: every war party out stops (MZB-15). In handle order.
    fn stop_war_parties(&mut self, w: &mut World, at: Tick) {
        let out: Vec<Handle<Party>> = self
            .parties
            .iter()
            .filter(|(_, x)| {
                x.kind.is_war() && matches!(x.phase, Phase::Out | Phase::Waiting | Phase::Fighting)
            })
            .map(|(h, _)| h)
            .collect();
        for p in out {
            let x = &self.parties[p];
            let (f, kind, phase) = (x.faction, x.kind, x.phase);
            self.stats.peace_held += 1;
            if phase == Phase::Out {
                self.step_back(p);
            }
            self.note(
                at,
                Some(f),
                "peaceful",
                format!("the {} turns back: the world is Peaceful", kind.name()),
            );
            if phase == Phase::Fighting {
                self.end_fight(w, p, boundary::RECALL_SETTLED, at);
            } else {
                self.go_home(w, p, boundary::RECALL_SETTLED, at);
            }
        }
    }

    /// A war party that never did its war: the feud it served steps back a rung, so that it climbs
    /// to it again once the world is not Peaceful.
    fn step_back(&mut self, p: Handle<Party>) {
        let x = &self.parties[p];
        let (f, kind, feud) = (x.faction, x.kind, x.feud);
        if let Some(a) = feud
            && self
                .feud(f, a)
                .is_some_and(|x| x.party == Some(p) && PartyKind::of_step(x.step) == Some(kind))
        {
            // Past empty rungs, as every step down (MZB-50).
            let to = self
                .culture(f)
                .next_down(self.feud(f, a).expect("checked").step);
            self.feud_mut(f, a).expect("checked").step = to;
        }
    }

    /// The plugin played a feud's notice (NOTICE_SHOWN): it is not sent again. A step other than
    /// the kept notice's is an answer to an older one, and changes nothing.
    fn notice_shown(&mut self, n: &boundary::NoticeShown) {
        let Some(&f) = self.by_key.get(&n.faction) else {
            return;
        };
        // Matched on the kept notice itself (whom it named and its serial), not on an actor
        // looked up again: a player the engine knows only through the world's own records still
        // matches, and an answer to an older notice of the same feud changes nothing (MZB-10).
        let Some(a) = self.feuds_of(f).iter().find_map(|(a, x)| {
            x.notice
                .as_deref()
                .filter(|k| k.player == n.player && k.serial == n.serial)
                .map(|_| *a)
        }) else {
            return;
        };
        self.feud_mut(f, a).expect("found").notice = None;
        self.stats.notices_shown += 1;
    }

    /// Notices kept until played, in faction then actor order. A whole-engine pass: tests and
    /// tools only.
    pub fn kept_notices(&self) -> Vec<boundary::Notice> {
        (0..self.factions.len() as u32)
            .flat_map(|f| {
                self.feuds_of(f)
                    .iter()
                    .filter_map(|(_, x)| x.notice.as_deref().copied())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    // ---- observations (docs/boundary.md §9) --------------------------------------------------

    fn first_time(&mut self, op: u64) -> bool {
        self.applied_ops.insert(op)
    }

    /// Whether a record with this op id was taken already (a repeat changes nothing).
    pub fn op_applied(&self, op: u64) -> bool {
        self.applied_ops.contains(&op)
    }

    fn party_of(&self, agent: u64, epoch: u32) -> Option<Handle<Party>> {
        let p = Party::of_agent(agent)?;
        self.parties.get(p).filter(|x| x.epoch == epoch).map(|_| p)
    }

    fn goods_of(&self, w: &World, items: &[boundary::Item]) -> BTreeMap<GoodId, Amount> {
        let mut m = BTreeMap::new();
        for it in items {
            if it.count <= 0 {
                continue;
            }
            let Some(name) = self.java_dict.get(&(2, it.item)) else {
                continue;
            };
            let short = name.rsplit(':').next().unwrap_or(name);
            if let Some(g) = w.goods.find(name).or_else(|| w.goods.find(short)) {
                *m.entry(g).or_default() += i64::from(it.count) * MILLI;
            }
        }
        m
    }

    /// The records whose engine half is not built yet (MOWZIE.md §9's later slices): the rail
    /// meter's, the chief's and the grove's. The camp reconciler's (CAMP_CENSUS, CAMP_OUT,
    /// CAMP_BODIES, CAMP_DEATH) are built (M4).
    pub fn later(o: &Obs) -> bool {
        matches!(
            o,
            Obs::ChiefState(_)
                | Obs::HavenChest(_)
                | Obs::RailPass(_)
                | Obs::TrainPassed(_)
                | Obs::Traded(_)
                | Obs::ChiefPaid(_)
        )
    }

    /// A record from the plugin, at its game time.
    pub fn ingest(&mut self, w: &mut World, infra: &mut dyn Infrastructure, o: &Obs, at: Tick) {
        let at = at.max(self.now);
        self.run_until(w, infra, at);
        if let Some(op) = o.op()
            && !self.first_time(op)
        {
            return;
        }
        if Self::later(o) {
            // The engine half of these comes with a later slice (MOWZIE.md §9): dropped, its op
            // id spent as rtsim-jni spends it (MZB-23).
            return;
        }
        match o {
            Obs::State(_) | Obs::Materialized(_) => {}
            Obs::Difficulty(d) => self.set_difficulty(w, d.difficulty),
            Obs::NoticeShown(n) => self.notice_shown(n),
            Obs::ChiefState(_)
            | Obs::HavenChest(_)
            | Obs::RailPass(_)
            | Obs::TrainPassed(_)
            | Obs::Traded(_)
            | Obs::ChiefPaid(_) => {}
            Obs::CampDeath(d) => self.camp_death(w, d, at),
            Obs::CampCensus(c) => self.camp_census(c, at),
            Obs::CampBodies(b) => self.camp_bodies(b, at),
            Obs::CampOut(o) => self.camp_out(o, at),
            Obs::FactionFound(ff) => {
                let Some(name) = self.java_dict.get(&(4, ff.structure)).cloned() else {
                    return;
                };
                let Some(ci) = self.cultures.of_structure(&name) else {
                    return;
                };
                let cname = self.cultures.get(ci).name().to_string();
                let pos = self.canon(ff.dimension, ff.pos.0, ff.pos.2);
                let known = self.by_key.contains_key(&ff.faction);
                if let Some(f) = self.add_faction(w, ff.faction, &cname, pos, at)
                    && !known
                {
                    self.camp_found(f, ff.home_bodies, at);
                }
            }
            Obs::Signal(s) => {
                let Some(kind) = SignalKind::from_u32(s.kind) else {
                    return;
                };
                let actor = self.actor_of_uuid(w, s.player);
                let sig = Signal {
                    faction: (s.faction != 0).then_some(s.faction),
                    kind,
                    amount: f64::from(s.count),
                    actor,
                    pos: self.canon(s.dimension, s.pos.0, s.pos.2),
                    district: None,
                };
                self.signal(w, sig, at);
            }
            Obs::Pollution(pl) => {
                let Some(name) = self
                    .java_dict
                    .get(&(boundary::DICT_POLLUTION, pl.ptype))
                    .cloned()
                else {
                    return;
                };
                let Some((kind, scale)) = self.tun.pollution_signal(&name) else {
                    return;
                };
                let per_day = if pl.window_ticks == 0 {
                    0.0
                } else {
                    pl.amount.max(0) as f64 * scale * TICKS_PER_DAY as f64
                        / f64::from(pl.window_ticks)
                };
                let pos = self.canon(pl.dimension, pl.x, pl.z);
                let district = (pl.district != 0).then_some(DistrictId(pl.district));
                let key = if pl.district != 0 {
                    pl.district
                } else {
                    fnv1a64(
                        &[
                            u64::from(pl.dimension).to_le_bytes(),
                            i64::from(pl.x >> 4).to_le_bytes(),
                            i64::from(pl.z >> 4).to_le_bytes(),
                        ]
                        .concat(),
                    )
                };
                self.set_emission(w, key, kind, pos, district, per_day, at);
            }
            Obs::SpawnRejected(r) => {
                if let Some(p) = self.party_of(r.agent, r.epoch)
                    && r.wave == 0
                {
                    let x = &mut self.parties[p];
                    x.embodied = false;
                    x.spawned.clear();
                }
            }
            Obs::Dematerialized(d) => {
                let Some(p) = self.party_of(d.agent, d.epoch) else {
                    return;
                };
                let f = self.parties[p].faction;
                let c = self.culture(f).clone();
                let back_of =
                    |u: u8, dict: &BTreeMap<(u8, u32), String>, cultures: &Cultures| -> u32 {
                        let body = &c.unit(u).body;
                        let id = body_id(cultures, body);
                        d.survivors
                            .iter()
                            .filter(|s| dict.get(&(3, s.0)).is_some_and(|n| n == body) || s.0 == id)
                            .map(|s| s.1)
                            .sum()
                    };
                let units: Vec<u8> = self.parties[p].units.iter().map(|e| e.0).collect();
                let backs: Vec<u32> = units
                    .iter()
                    .map(|&u| back_of(u, &self.java_dict, &self.cultures))
                    .collect();
                let before = self.parties[p].alive();
                let x = &mut self.parties[p];
                if x.embodied {
                    // The plugin despawned it (its chunks unloaded): alive are those never
                    // spawned plus the survivors it counts.
                    for (k, e) in x.units.iter_mut().enumerate() {
                        let spawned = x.spawned.iter().find(|s| s.0 == e.0).map_or(0, |s| s.1);
                        e.1 = e.1.saturating_sub(spawned) + backs[k].min(spawned);
                    }
                    x.spawned.clear();
                    x.embodied = false;
                    x.seen_by = None;
                } else {
                    // After the engine's recall: only those that died on the way out go.
                    for (k, e) in x.units.iter_mut().enumerate() {
                        let r = x.recalled.iter().find(|s| s.0 == e.0).map_or(0, |s| s.1);
                        e.1 = e.1.saturating_sub(r.saturating_sub(backs[k]));
                    }
                }
                x.recalled.clear();
                let lost = before.saturating_sub(x.alive());
                self.party_lost(p, lost, at);
            }
            Obs::Casualties(cs) => self.casualties(w, cs, at),
            Obs::ChiefKilled(ck) => {
                let Some(p) = Party::of_agent(ck.agent).filter(|p| self.parties.contains(*p))
                else {
                    return;
                };
                let killer = (ck.by == boundary::BY_PLAYER).then(|| self.player(w, ck.killer));
                self.chief_killed(w, p, killer, at);
            }
            Obs::EnvoyAnswer(a) => {
                let Some(p) = self.party_of(a.agent, a.epoch) else {
                    return;
                };
                let f = self.parties[p].faction;
                let Some(actor) = self.parties[p].feud else {
                    return;
                };
                match a.answer {
                    boundary::ANSWER_REFUSED => self.refuse(w, f, actor, at, 0.0),
                    boundary::ANSWER_ATTACKED => {
                        let insult = self.tun.envoy_insult_kills;
                        let attacker = self.actor_of_uuid(w, a.player);
                        let sig = Signal {
                            faction: Some(self.factions[f as usize].key),
                            kind: SignalKind::Killing,
                            amount: insult,
                            actor: attacker,
                            pos: self.factions[f as usize].pos,
                            district: None,
                        };
                        self.signal(w, sig, at);
                        self.refuse(w, f, actor, at, insult);
                    }
                    _ => {}
                }
            }
            Obs::TributePaid(t) => {
                let goods = self.goods_of(w, &t.items);
                let payer = self.actor_of_uuid(w, t.player);
                let envoy = (t.agent != 0).then_some(t.agent);
                self.pay_tribute(w, t.faction, payer, envoy, &goods, at);
            }
            Obs::GiftGiven(g) => {
                let goods = self.goods_of(w, &g.items);
                let giver = self.actor_of_uuid(w, g.player);
                self.gift(w, g.faction, giver, &goods, at);
            }
            Obs::Looted(l) => {
                let Some(p) = Party::of_agent(l.agent).filter(|p| self.parties.contains(*p)) else {
                    return;
                };
                // Items real mobs carried off from containers: they left the player's world.
                let goods = self.goods_of(w, &l.items);
                for (g, a) in goods {
                    self.took_from_players(p, g, a);
                }
            }
        }
    }

    /// The player refused the demand or attacked the envoy: unmet now (DEMAND_SETTLED refused),
    /// unless it was paid in full already.
    fn refuse(&mut self, w: &mut World, f: u32, a: ActorId, at: Tick, _insult: f64) {
        if let Some(feud) = self.feud_mut(f, a)
            && let Some(d) = feud.demand.as_mut()
        {
            d.deadline = at;
            let d = *d;
            if d.paid >= d.tribute {
                self.settle(w, f, a, boundary::SETTLED_PAID, at);
            } else {
                self.demand_unmet(w, f, a, d, boundary::SETTLED_REFUSED, at);
            }
            self.plan_feud(f, a, at, false);
        }
    }

    fn casualties(&mut self, w: &mut World, cs: &boundary::BandCasualties, at: Tick) {
        let Some(p) = self.party_of(cs.agent, cs.epoch) else {
            return;
        };
        if !self.parties[p].embodied {
            // After its DEMATERIALIZED the survivors were counted: a straggler is not a death.
            return;
        }
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let name = self.java_dict.get(&(3, cs.body)).cloned();
        let unit = self.parties[p].units.iter().position(|&(u, _)| {
            name.as_deref() == Some(c.unit(u).body.as_str())
                || body_id(&self.cultures, &c.unit(u).body) == cs.body
        });
        let Some(k) = unit else {
            return;
        };
        let x = &mut self.parties[p];
        let t = x.units[k].1.min(cs.count);
        x.units[k].1 -= t;
        let u = x.units[k].0;
        if let Some(s) = x.spawned.iter_mut().find(|s| s.0 == u) {
            s.1 = s.1.saturating_sub(t);
        }
        self.stats.casualties += u64::from(t);
        self.party_lost(p, t, at);
        let killer = (cs.by == boundary::BY_PLAYER).then(|| self.player(w, cs.killer));
        if let Some(k) = killer {
            self.parties[p].breaker = Some(k);
            // Killing members is a direct act against the faction (P14: the killer).
            let (x, z) = self.party_pos(p, at);
            let district = match self.parties[p].target {
                Target::District(d, _) => Some(d),
                _ => None,
            };
            let sig = Signal {
                faction: Some(self.factions[f as usize].key),
                kind: SignalKind::Killing,
                amount: f64::from(t),
                actor: k,
                pos: Pos::new(self.parties[p].frame, x, z),
                district,
            };
            self.signal(w, sig, at);
        }
        // Losses bring the boredom forward (C24) and a heavy loss routs the band.
        let x = &self.parties[p];
        let lost_share = 1.0 - f64::from(x.alive()) / f64::from(x.start_units.max(1));
        if x.kind.is_war() && x.phase == Phase::Fighting {
            if lost_share >= c.def.rout_share {
                self.end_fight(w, p, boundary::RECALL_ROUTED, at);
            } else {
                let left = x.until.0.saturating_sub(at.0);
                let until = at + (left as f64 * (1.0 - lost_share)).ceil() as u64;
                let x = &mut self.parties[p];
                x.until = until;
                x.bored_at = x.bored_at.min(until);
                let plan = self.bump(p);
                self.schedule(until, Ev::Party { party: p, plan });
                self.schedule(
                    at + self.tun.wave_interval_ticks,
                    Ev::Wave { party: p, plan },
                );
            }
        } else if x.alive() == 0 && x.phase != Phase::Home {
            // An envoy party no player struck down withdraws its demand (outcome 5); one a player
            // killed leaves it to run out as an insult (the killings above, `refuse` on ATTACKED).
            if x.kind == PartyKind::Envoy && x.breaker.is_none() {
                self.withdraw(w, p, at);
                // Nobody left in Minecraft: nothing to recall. Else the rest go as a settled
                // demand's envoy does (MZB-32).
                let x = &mut self.parties[p];
                if x.spawned.iter().all(|e| e.1 == 0) {
                    x.embodied = false;
                    x.seen_by = None;
                    x.spawned.clear();
                }
                self.go_home(w, p, boundary::RECALL_SETTLED, at);
            } else {
                self.go_home(w, p, boundary::RECALL_ROUTED, at);
            }
        }
    }

    /// The war chief falls (C24, C25): the band routs, the heir takes over holding the grudge,
    /// the faction is cowed for a while (a bold heir recovers sooner), and the killer's feud drops
    /// rungs.
    pub fn chief_killed(
        &mut self,
        w: &mut World,
        p: Handle<Party>,
        killer: Option<ActorId>,
        at: Tick,
    ) {
        let f = self.parties[p].faction;
        if !self.parties[p].chief {
            return;
        }
        self.stats.chiefs_killed += 1;
        self.parties[p].chief = false;
        let (x, z) = self.party_pos(p, at);
        let fall = At::new(at.0, u32::from(self.parties[p].frame), x, z);
        let chief = self.factions[f as usize].chief;
        let heir = self.factions[f as usize].heir;
        if let Some(k) = killer {
            let pos = self.factions[f as usize].pos;
            let r = w.knowledge.record(Report {
                who: k,
                whom: chief,
                kind: ReportKind::Killing,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: Place::new(pos.frame, pos.x, 64, pos.z),
            });
            w.knowledge.witness(chief, r, at);
            w.knowledge.witness(heir, r, at);
        }
        w.knowledge.die(chief, at);
        self.succeed(w, f, at, killer);
        let bend = self.temper(f).bend(Trait::Bold, 1000, 50).max(1);
        let cowed = (days_to_ticks(self.culture(f).def.cowed_days) as f64 * 1000.0
            / f64::from(bend)) as u64;
        self.factions[f as usize].cowed_until = at + cowed;
        let fid = self.fid(f);
        self.tape
            .event(kinds::CONFLICT_CHIEF, fall, fid, killer.map_or(0, aid), 0)
            .text(|| {
                format!(
                    "the band routs; cowed for {:.1} days",
                    cowed as f64 / TICKS_PER_DAY as f64
                )
            });
        if let Some(a) = self.parties[p].feud.or(killer) {
            let steps = self.tun.chief_rout_steps;
            if let Some(from) = self.feud(f, a).map(|x| x.step) {
                // Each step down skips the rungs the culture has no party for (MOWZIE.md
                // ledger 2).
                let c = self.culture(f);
                let mut to = from;
                for _ in 0..steps {
                    to = c.next_down(to);
                }
                let feud = self.feud_mut(f, a).expect("exists");
                feud.step = to.max(Step::Notice);
                feud.since = at;
            }
        }
        if let Some(k) = killer {
            self.parties[p].breaker = Some(k);
        }
        if self.parties[p].phase == Phase::Fighting {
            self.end_fight(w, p, boundary::RECALL_ROUTED, at);
        } else if self.parties[p].phase != Phase::Home {
            self.stats.routs += 1;
            self.go_home(w, p, boundary::RECALL_ROUTED, at);
        }
        self.note(
            at,
            Some(f),
            "chief",
            format!(
                "the war chief falls: the band routs; cowed for {:.1} days",
                cowed as f64 / TICKS_PER_DAY as f64
            ),
        );
        // Every feud replans here, the one whose step just fell among them, also when the band
        // was already walking home and neither `go_home` nor `end_fight` ran above.
        for a in self.feud_actors(f) {
            self.plan_feud(f, a, at, false);
        }
    }

    // ---- reads ------------------------------------------------------------------------------

    /// The grievance at which faction `f`'s feuds climb to rung `s`, bent by its chief's
    /// temperament (K2-8: up to 40 percent either way).
    pub fn rung_threshold(&self, f: u32, s: Step) -> f64 {
        self.theta(f, s)
    }

    /// A feud as it stands at the engine's time (a projection).
    pub fn feud_view(&self, faction: u64, actor: ActorId) -> Option<FeudView> {
        let f = *self.by_key.get(&faction)?;
        let x = self.feud(f, actor)?;
        let lambda = self.culture(f).lambda;
        let v = x.g.project(self.now, lambda);
        Some(FeudView {
            faction,
            actor: actor.to_bits(),
            step: x.step,
            grievance: v.iter().sum(),
            by_kind: SignalKind::ALL
                .iter()
                .map(|k| (k.name(), v[k.index()]))
                .filter(|e| e.1 > 0.0)
                .collect(),
            demand: x.demand.map(|d| (d.kind, d.tribute, d.paid, d.deadline.0)),
            party: x.party.and_then(|p| self.parties.get(p)).map(|p| p.kind),
        })
    }

    /// Goods every party holds in the world's books (`Outside::held`).
    pub fn held(&self) -> BTreeMap<GoodId, i128> {
        let mut m = BTreeMap::new();
        for (_, x) in self.parties.iter() {
            for (g, a) in &x.loot {
                *m.entry(*g).or_default() += i128::from(*a);
            }
        }
        m
    }

    /// The conflict's books close: what was taken from villages is landed at havens or held by a
    /// party; what was taken from players is landed or carried; and the world's own sum holds.
    pub fn check(&self, w: &World) -> Result<(), String> {
        let held = self.held();
        for (g, taken) in &self.ledger.from_villages {
            let landed = self.ledger.landed.get(g).copied().unwrap_or(0);
            let h = held.get(g).copied().unwrap_or(0);
            let gone = self.ledger.destroyed.get(g).copied().unwrap_or(0);
            if *taken != landed + h + gone {
                return Err(format!(
                    "good {}: taken {taken}, landed {landed}, held {h}, destroyed {gone}",
                    g.0
                ));
            }
            if h > w.outside.held.get(g).copied().unwrap_or(0) {
                return Err(format!(
                    "good {}: parties hold more than the world's held",
                    g.0
                ));
            }
        }
        let mut carried: BTreeMap<GoodId, i128> = BTreeMap::new();
        for (_, x) in self.parties.iter() {
            for (g, a) in &x.from_players {
                *carried.entry(*g).or_default() += i128::from(*a);
            }
        }
        for (g, taken) in &self.ledger.from_players {
            let landed = self.ledger.landed_from_players.get(g).copied().unwrap_or(0);
            let c = carried.get(g).copied().unwrap_or(0);
            let gone = self
                .ledger
                .destroyed_from_players
                .get(g)
                .copied()
                .unwrap_or(0);
            if *taken != landed + c + gone {
                return Err(format!(
                    "good {}: from players {taken}, landed {landed}, carried {c}, destroyed {gone}",
                    g.0
                ));
            }
        }
        w.check_conserved(w.now()).map_err(|e| e.to_string())
    }

    /// Bytes the engine holds: the row tables' resident pages and page indexes (bounded by the
    /// budget when paged), the resident indexes ([`Conflict::index_bytes`]), presence, parties,
    /// the wheel, the node index and the op set. Estimates; [`Conflict::memory`] splits them.
    pub fn heap_bytes(&self) -> usize {
        self.memory().values().sum()
    }

    /// [`Conflict::heap_bytes`] by part, in bytes (as `World::memory`): what a bench reports to
    /// say where the resident bytes per faction go.
    pub fn memory(&self) -> BTreeMap<&'static str, usize> {
        let party =
            |x: &Party| std::mem::size_of::<Party>() + x.route.len() * 16 + x.units.len() * 8;
        BTreeMap::from([
            ("factions", self.factions.rows.heap_bytes()),
            ("sources", self.sources.rows.heap_bytes()),
            ("places", self.places.rows.heap_bytes()),
            ("index by key", self.by_key.len() * BTREE_KEY_BYTES),
            ("index by cell", self.cells.len() * BTREE_CELL_BYTES),
            (
                "index of sources",
                self.sources.ids.len() * BTREE_SOURCE_BYTES,
            ),
            ("presence", self.presence.heap_bytes()),
            (
                "parties",
                self.parties.heap_bytes()
                    + self.parties.iter().map(|(_, x)| party(x)).sum::<usize>(),
            ),
            ("wheel", self.wheel.heap_bytes()),
            ("nodes", self.nodes.heap_bytes()),
            ("applied ops", self.applied_ops.len() * BTREE_U64_BYTES),
            ("players", self.players.len() * BTREE_PLAYER_BYTES),
        ])
    }

    /// The resident indexes that grow with the world: factions by key, factions by cell, and
    /// sources by key (PM-C4, PM-C6): about 60 bytes per faction and 64 per source (measured
    /// B-tree costs per entry).
    pub fn index_bytes(&self) -> usize {
        self.by_key.len() * BTREE_KEY_BYTES
            + self.cells.len() * BTREE_CELL_BYTES
            + self.sources.ids.len() * BTREE_SOURCE_BYTES
    }

    /// The row tables' resident page bytes (what the budget bounds).
    pub fn resident_bytes(&self) -> usize {
        self.factions.rows.resident_bytes()
            + self.sources.rows.resident_bytes()
            + self.places.rows.resident_bytes()
    }
}

/// B-tree bytes per entry (see [`Conflict::index_bytes`]), measured with a counting allocator
/// over 100,000 entries on 2026-10-20 (std's B-tree, leaves of 11): factions by key (u64 to u32,
/// 26.3 inserted in order, 20.7 at random), by cell ((u16, i32, i32), u32: 33.6), sources by key
/// ((u64, kind) to id and rate: 63.6), the op set (u64 at random: 15.4) and the players (u64 to
/// two u64, estimated).
const BTREE_KEY_BYTES: usize = 26;
const BTREE_CELL_BYTES: usize = 34;
const BTREE_SOURCE_BYTES: usize = 64;
const BTREE_U64_BYTES: usize = 16;
const BTREE_PLAYER_BYTES: usize = 48;

/// A raid on a village's or a haven's market (I22's rule applied to residents): Lanchester's
/// linear law of the band's strength against the defence, floored by the faction's real fights at
/// comparable strength (never better for the defender than observed). Returns the attackers'
/// surviving share and the floor that applied.
pub fn village_outcome(
    strength: f64,
    defence: f64,
    observed: &[(f64, f64)],
    comparable: f64,
) -> (f64, Option<f64>) {
    let lanchester = if strength <= 0.0 {
        0.0
    } else {
        ((strength - defence) / strength).max(0.0)
    };
    let k = comparable.max(1.0);
    let floor = observed
        .iter()
        .filter(|o| o.0 >= strength / k && o.0 <= strength * k)
        .map(|o| o.1)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
    (
        floor
            .map_or(lanchester, |fl| lanchester.max(fl))
            .clamp(0.0, 1.0),
        floor,
    )
}

/// Splits `total` over `caps` in proportion, never above a cap, remainders to the largest
/// fractions (ties to the first): integer and deterministic.
fn apportion(total: Amount, caps: &[(GoodId, Amount)]) -> Vec<(GoodId, Amount)> {
    let sum: i128 = caps.iter().map(|e| i128::from(e.1)).sum();
    if sum <= 0 || total <= 0 {
        return caps.iter().map(|e| (e.0, 0)).collect();
    }
    let total = i128::from(total).min(sum);
    let mut out: Vec<(GoodId, Amount)> = Vec::new();
    let mut rem: Vec<(i128, usize)> = Vec::new();
    let mut given = 0i128;
    for (k, e) in caps.iter().enumerate() {
        let num = total * i128::from(e.1);
        let q = num / sum;
        given += q;
        out.push((e.0, q as Amount));
        rem.push((num % sum, k));
    }
    rem.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut left = total - given;
    for (_, k) in rem {
        if left <= 0 {
            break;
        }
        if out[k].1 < caps[k].1 {
            out[k].1 += 1;
            left -= 1;
        }
    }
    out
}

fn owns_leg(owner: Owner) -> bool {
    owner.0 >> 59 == 1
}

impl Viewer for Conflict {
    fn owns(&self, owner: Owner) -> bool {
        owns_leg(owner)
    }

    fn area_booked(&mut self, w: &mut World, player: ActorId, uuid: (u64, u64), area: Stamp) {
        let _ = (w, area);
        self.players.insert(player, uuid);
    }

    fn crossings(&mut self, w: &mut World, player: ActorId, area: Stamp, found: &[Meeting]) {
        let _ = area;
        for m in found {
            let Some(owner) = w.legs.get(m.a.leg).map(|r| r.owner) else {
                continue;
            };
            if !owns_leg(owner) {
                continue;
            }
            let p = Handle::from_bits(owner.0 & MASK48);
            self.schedule(
                m.at,
                Ev::Seen {
                    party: p,
                    player,
                    meeting: *m,
                },
            );
        }
    }

    fn player_left(&mut self, w: &mut World, player: ActorId) {
        let _ = w;
        // P14's counter: the gap until the player's next report is not time spent anywhere.
        self.presence.left(player);
        let ids: Vec<Handle<Party>> = self
            .parties
            .iter()
            .filter(|(_, x)| x.embodied && x.seen_by == Some(player))
            .map(|(h, _)| h)
            .collect();
        for p in ids {
            let camp = self.factions[self.parties[p].faction as usize].pos;
            let epoch = self.parties[p].epoch;
            self.recall(p, epoch, boundary::RECALL_PLAYER_LEFT, (camp.x, camp.z));
            let x = &mut self.parties[p];
            // A watched fight with nobody watching goes on abstractly at its next event.
            if x.phase == Phase::Fighting {
                x.phase = Phase::Waiting;
            }
        }
    }
}
