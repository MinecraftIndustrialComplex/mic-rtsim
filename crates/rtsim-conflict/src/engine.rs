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

use rtsim_ecology::infra::Infrastructure;
use rtsim_industry::ids::DistrictId;
use rtsim_industry::threat::{Threat, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::{ActorId, ActorKind, Place, Report, ReportKind, Temperament, Trait};
use rtsim_settle::{NodeKind, Pos};
use rtsim_trade::{Amount, GoodId, MILLI, PRICE_ONE};
use rtsim_travel::legs::{LegIndex, Meeting, Owner, Path, Stamp};
use rtsim_world::interest::{PLAYER_OWNER, Viewer, uuid_hash};
use rtsim_world::{NodeData, World};
use serde::Serialize;

use crate::boundary::{self, Cmd, Obs, Uuid};
use crate::culture::{Culture, Cultures, PartyKind, SignalKind, Step, days_to_ticks};
use crate::grievance::{Grievance, K};
use crate::tunables::ConflictTunables;

/// Leg owner bit of a conflict party (the registry is in `rtsim_world::interest::Viewer`).
pub const PARTY_OWNER: u64 = 1 << 59;
const MASK48: u64 = (1 << 48) - 1;

const SYS_PLUNDER: SystemId = SystemId::named("conflict.plunder");
const SYS_AMBUSH: SystemId = SystemId::named("conflict.ambush");
const SYS_CHIEF: SystemId = SystemId::named("conflict.chief");

/// The settlers actor's external id (P14: one shared actor for unowned signals).
pub const SETTLERS_KEY: u64 = fnv1a64(b"mic_rtsim.conflict.settlers");

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
}

/// A continuous source (P14: unowned, the settlers'): a district's or a chunk's emissions.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub pos: Pos,
    pub district: Option<DistrictId>,
    /// Units per tick.
    pub rate: f64,
    /// (faction, falloff) it reaches.
    pub affects: Vec<(u32, f64)>,
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
    fn refresh(&mut self, w: &World) {
        if w.nodes.len() == self.known {
            return;
        }
        self.cells.clear();
        for (i, n) in w.nodes.iter().enumerate() {
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

    /// The nearest node to `pos` within `reach` blocks that `ok` accepts (ties to the lower
    /// index), searching rings of cells outward and stopping once no nearer ring can hold a
    /// better one.
    fn nearest(
        &self,
        w: &World,
        pos: Pos,
        reach: f64,
        mut ok: impl FnMut(usize) -> bool,
    ) -> Option<(f64, usize)> {
        let (cx, cz) = (pos.x.div_euclid(NODE_CELL), pos.z.div_euclid(NODE_CELL));
        let rings = (reach / f64::from(NODE_CELL)).ceil() as i32 + 1;
        let mut best: Option<(f64, usize)> = None;
        for r in 0..=rings {
            // Every point of ring r is at least (r - 1) cells away.
            if let Some(b) = best
                && b.0 <= f64::from((r - 1).max(0) * NODE_CELL)
            {
                break;
            }
            for x in cx - r..=cx + r {
                for z in cz - r..=cz + r {
                    if (x - cx).abs() != r && (z - cz).abs() != r {
                        continue;
                    }
                    for &i in self.cells.get(&(pos.frame, x, z)).map(Vec::as_slice).unwrap_or(&[]) {
                        let d = dist(pxz(w.nodes[i].pos), pxz(pos));
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

pub struct Conflict {
    pub seed: u64,
    pub tun: ConflictTunables,
    pub cultures: Cultures,
    pub factions: Vec<Faction>,
    pub(crate) by_key: BTreeMap<u64, u32>,
    pub(crate) cells: BTreeMap<(u16, i32, i32), Vec<u32>>,
    pub feuds: BTreeMap<(u32, ActorId), Feud>,
    pub sources: BTreeMap<(u64, SignalKind), Source>,
    pub parties: Arena<Party>,
    pub(crate) wheel: TimingWheel<Ev>,
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
    pub(crate) nodes: NodeIndex,
    /// Sources reaching each (faction, kind): derived from `sources`, rebuilt on load.
    pub(crate) source_index: BTreeMap<(u32, SignalKind), BTreeSet<u64>>,
}

fn dist(a: (i64, i64), b: (i64, i64)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
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
        Conflict {
            seed,
            tun,
            cultures,
            factions: Vec::new(),
            by_key: BTreeMap::new(),
            cells: BTreeMap::new(),
            feuds: BTreeMap::new(),
            sources: BTreeMap::new(),
            parties: Arena::new(),
            wheel: TimingWheel::new(now),
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
            nodes: NodeIndex::default(),
            source_index: BTreeMap::new(),
        }
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

    /// A faction's camp: joins the world as a haven node (C24) if no node has its key, registers
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
                    kind: NodeKind::Haven(cu.market),
                    pos,
                    pop: (cu.def.pool_cap / 4).max(1),
                    resources: BTreeMap::new(),
                },
                at,
            ),
        };
        let f = self.factions.len() as u32;
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
        self.factions.push(x);
        self.by_key.insert(key, f);
        let cell = self.cell_of(pos);
        self.cells.entry(cell).or_default().push(f);
        let mask = SignalKind::ALL
            .into_iter()
            .filter(|k| cu.signals[k.index()].weight > 0.0)
            .fold(0, |m, k| m | k.bit());
        self.outbox.push(Cmd::Territory(boundary::Territory {
            faction: key,
            flags: 0,
            dimension: u32::from(pos.frame),
            signals: mask,
            x: pos.x,
            z: pos.z,
            radius: cu.def.territory_radius,
        }));
        self.schedule_plunder(f, at);
        // Sources already emitting reach the newcomer (only the newcomer's feud changes).
        let reach = f64::from(cu.def.territory_radius) + f64::from(cu.def.reach_blocks);
        let near: Vec<((u64, SignalKind), f64)> = self
            .sources
            .iter()
            .filter_map(|(k, s)| {
                let d = dist(pxz(s.pos), pxz(pos));
                (s.pos.frame == pos.frame && d <= reach).then(|| {
                    let r = f64::from(cu.def.territory_radius);
                    let fall = if d <= r {
                        1.0
                    } else {
                        (1.0 - (d - r) / f64::from(cu.def.reach_blocks.max(1))).max(0.0)
                    };
                    (*k, fall)
                })
            })
            .filter(|e| e.1 > 0.0)
            .collect();
        if !near.is_empty() {
            let settlers = self.settlers(w);
            let mut kinds = Vec::new();
            for (k, fall) in near {
                if let Some(s) = self.sources.get_mut(&k) {
                    s.affects.push((f, fall));
                    s.affects.sort_by_key(|e| e.0);
                }
                self.source_index.entry((f, k.1)).or_default().insert(k.0);
                if !kinds.contains(&k.1) {
                    kinds.push(k.1);
                }
            }
            for kind in kinds {
                self.refresh_rates(w, f, settlers, kind, at);
            }
        }
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
        let mut out = Vec::new();
        for cx in (pos.x - far).div_euclid(c)..=(pos.x + far).div_euclid(c) {
            for cz in (pos.z - far).div_euclid(c)..=(pos.z + far).div_euclid(c) {
                for &f in self
                    .cells
                    .get(&(pos.frame, cx, cz))
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                {
                    let x = &self.factions[f as usize];
                    let d = dist(pxz(x.pos), pxz(pos));
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

    /// The faction's warriors at `t` (closed form).
    pub fn pool_at(&self, f: u32, t: Tick) -> f64 {
        let x = &self.factions[f as usize];
        let c = self.culture(f);
        let rate = c.def.recruits_per_day / TICKS_PER_DAY as f64;
        (x.pool + rate * t.0.saturating_sub(x.pool_at.0) as f64).min(f64::from(c.def.pool_cap))
    }

    fn pool_add(&mut self, f: u32, n: f64, at: Tick) {
        let v = self.pool_at(f, at);
        let cap = f64::from(self.culture(f).def.pool_cap);
        let x = &mut self.factions[f as usize];
        x.pool = (v + n).clamp(0.0, cap);
        x.pool_at = at;
    }

    /// When the pool will hold `n` warriors.
    fn pool_ready(&self, f: u32, n: f64, at: Tick) -> Tick {
        let have = self.pool_at(f, at);
        if have >= n {
            return at;
        }
        let rate = self.culture(f).def.recruits_per_day / TICKS_PER_DAY as f64;
        if rate <= 0.0 {
            return Tick(u64::MAX / 2);
        }
        at + ((n - have) / rate).ceil() as u64 + 1
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
            let c = w
                .knowledge
                .create_notable(s, home, new_heir, at, lifespan);
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
            let feud = self.feud_mut(w, f, p, at);
            feud.g.add(SignalKind::Killing, g, at, lambda);
        }
        let what = match killer {
            Some(_) => "its war chief is killed; the heir takes over, holding the grudge".to_string(),
            None => "its chief dies of age; the heir takes over".to_string(),
        };
        self.note(at, Some(f), "heir", format!("{what} ({} grudges)", grudges.len()));
        // Thresholds follow the new chief's temperament: every feud of the faction replans.
        let actors: Vec<ActorId> = self
            .feuds
            .range((f, Handle::NULL)..)
            .take_while(|(k, _)| k.0 == f)
            .map(|(k, _)| k.1)
            .collect();
        for a in actors {
            self.plan_feud(f, a, at, false);
        }
    }

    // ---- signals --------------------------------------------------------------------------

    fn feud_mut(&mut self, w: &World, f: u32, a: ActorId, at: Tick) -> &mut Feud {
        let _ = w;
        self.feuds.entry((f, a)).or_insert_with(|| Feud {
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
        })
    }

    /// A direct act or an observed impulse (a block broken in a territory, ore drained, a member
    /// killed): grievance jumps by its weight in every faction whose territory it is in.
    pub fn signal(&mut self, w: &mut World, s: Signal, at: Tick) {
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
            let feud = self.feud_mut(w, f, s.actor, at);
            feud.g.add(s.kind, amount, at, lambda);
            if let Some(d) = feud.demand.as_mut()
                && d.kind == s.kind
            {
                d.after += s.amount;
            }
            let better = feud.focus.is_none_or(|fo| fo.district.is_none() || s.district.is_some());
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

    /// A continuous source's rate (P14: unowned, so the settlers'): a district's emissions while
    /// unloaded (the industry engine's learned rate of a pollution good) or a loaded chunk's
    /// (POLLUTION records). Units per day; 0 stops it.
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
        self.run_to(at);
        self.stats.emissions += 1;
        self.set_emission_inner(
            w,
            (key, kind),
            pos,
            district,
            per_day.max(0.0) / TICKS_PER_DAY as f64,
            at,
        );
    }

    fn set_emission_inner(
        &mut self,
        w: &mut World,
        key: (u64, SignalKind),
        pos: Pos,
        district: Option<DistrictId>,
        rate: f64,
        at: Tick,
    ) {
        let settlers = self.settlers(w);
        let old: Vec<u32> = self
            .sources
            .get(&key)
            .map(|s| s.affects.iter().map(|a| a.0).collect())
            .unwrap_or_default();
        let affects = if rate > 0.0 {
            self.factions_near(pos, true)
        } else {
            Vec::new()
        };
        let mut touched: Vec<u32> = old;
        touched.extend(affects.iter().map(|a| a.0));
        touched.sort();
        touched.dedup();
        for f in &touched {
            if let Some(v) = self.source_index.get_mut(&(*f, key.1)) {
                v.remove(&key.0);
            }
        }
        if rate > 0.0 {
            for (f, _) in &affects {
                self.source_index.entry((*f, key.1)).or_default().insert(key.0);
            }
            self.sources.insert(
                key,
                Source {
                    pos,
                    district,
                    rate,
                    affects,
                },
            );
        } else {
            self.sources.remove(&key);
        }
        for f in touched {
            self.refresh_rates(w, f, settlers, key.1, at);
        }
    }

    /// Recomputes a settlers feud's raw rate of one kind from the sources (exact, no drift) and
    /// its focus (the strongest source with a district).
    fn refresh_rates(&mut self, w: &World, f: u32, a: ActorId, kind: SignalKind, at: Tick) {
        let mut raw = 0.0;
        let mut best: Option<Focus> = None;
        // Only the sources that reach this faction (the index), in key order.
        for &key in self.source_index.get(&(f, kind)).into_iter().flatten() {
            let Some(s) = self.sources.get(&(key, kind)) else {
                continue;
            };
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
        if raw <= 0.0 && !self.feuds.contains_key(&(f, a)) {
            return;
        }
        let feud = self.feud_mut(w, f, a, at);
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

    /// Books the feud's next event: the earliest of its next rung (after the dwell), its fall
    /// below the current rung, its demand's deadline. A new stamp makes the older booking stale.
    fn plan_feud(&mut self, f: u32, a: ActorId, at: Tick, from_wake: bool) {
        let Some(feud) = self.feuds.get(&(f, a)) else {
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
        if let Some(d) = feud.demand {
            take(Some(d.deadline));
        } else if feud.party.is_none() {
            if let Some(up) = feud.step.up() {
                let dwell = feud.since + self.culture(f).dwell_ticks[feud.step.index()];
                take(
                    feud.g
                        .crossing_up(self.theta(f, up), lambda)
                        .map(|t| t.max(dwell).max(cowed).max(feud.wait_until)),
                );
            }
            if feud.step > Step::None {
                take(
                    feud.g
                        .crossing_down(self.down_theta(f, feud.step), lambda),
                );
            } else if feud.raw.iter().all(|r| *r <= 0.0) {
                take(feud.g.crossing_down(self.tun.forget_below, lambda));
            }
        }
        let feud = self.feuds.get_mut(&(f, a)).expect("checked");
        feud.stamp = feud.stamp.wrapping_add(1);
        let stamp = feud.stamp;
        if let Some(t) = next {
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
        let Some(feud) = self.feuds.get_mut(&(f, a)) else {
            return;
        };
        feud.g.commit(at, lambda);
        let g = feud.g.level_sum();
        let (step, since, party, demand, wait) =
            (feud.step, feud.since, feud.party, feud.demand, feud.wait_until);
        if let Some(d) = demand {
            if at >= d.deadline {
                self.demand_due(w, f, a, at);
            }
        } else if party.is_none() {
            let cowed = self.factions[f as usize].cowed_until > at;
            let dwell = since + self.culture(f).dwell_ticks[step.index()];
            let up = step.up();
            if let Some(up) = up.filter(|u| {
                !cowed && at >= dwell && at >= wait && g >= self.theta(f, *u) * (1.0 - 1e-9)
            }) {
                self.escalate(w, f, a, up, at);
            } else if step > Step::None && g < self.down_theta(f, step) {
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
                feud.step = step.down();
                feud.since = at;
                self.stats.calmed += 1;
                let to = step.down();
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
                && self.feuds[&(f, a)].raw.iter().all(|r| *r <= 0.0)
            {
                self.feuds.remove(&(f, a));
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

    /// Climbs to rung `to`, doing what it says.
    fn escalate(&mut self, w: &mut World, f: u32, a: ActorId, to: Step, at: Tick) {
        let lambda = self.culture(f).lambda;
        let feud = self.feuds.get(&(f, a)).expect("exists");
        let g = feud.g.total_at(at, lambda);
        let kind = feud.g.dominant(at, lambda);
        let focus = feud.focus;
        if to.is_war() || to == Step::Scouts || to == Step::Envoy {
            let pk = PartyKind::of_step(to).expect("a party rung");
            let target = self.target_of(w, f, focus, at);
            let Some(p) = self.send_party(w, f, Some(a), pk, target, at) else {
                // Not enough warriors: wait for the pool (closed form).
                let need = self.party_size(f, pk) as f64 / 2.0;
                let ready = self.pool_ready(f, need, at);
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
                feud.wait_until = ready;
                return;
            };
            let feud = self.feuds.get_mut(&(f, a)).expect("exists");
            feud.party = Some(p);
            if to == Step::Envoy {
                let good = self.tribute_good(w, f);
                let base = self.culture(f).def.tribute_per_grievance * g;
                let bend = self.temper(f).bend(Trait::Aggressive, 1000, 50);
                let tribute = ((base * f64::from(bend) / 1000.0).ceil() as i64).max(1);
                let deadline = at + days_to_ticks(self.culture(f).def.demand_days);
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
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
            if to.is_war() {
                let chief = self.factions[f as usize].chief;
                if w.knowledge.actors().is_player(a) {
                    w.knowledge.adjust(chief, a, self.tun.raid_grudge, at);
                }
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
                feud.raids += 1;
            }
        }
        let feud = self.feuds.get_mut(&(f, a)).expect("exists");
        feud.step = to;
        feud.since = at;
        feud.wait_until = Tick::ZERO;
        self.stats.escalations[to.index()] += 1;
        let pos = focus.map_or(self.factions[f as usize].pos, |fo| fo.pos);
        if to == Step::Notice {
            let player = self.uuid_of(w, a);
            self.outbox.push(Cmd::Notice(boundary::Notice {
                faction: self.factions[f as usize].key,
                step: to as u32,
                player,
                dimension: u32::from(pos.frame),
                signal: kind as u32,
                x: pos.x,
                z: pos.z,
                grievance: g as f32,
            }));
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

    fn tribute_good(&self, w: &World, f: u32) -> GoodId {
        let c = self.culture(f);
        c.def
            .tribute_good
            .as_deref()
            .and_then(|n| w.goods.find(n))
            .or_else(|| w.goods.currency(c.market))
            .unwrap_or(GoodId(0))
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
        let lambda = self.culture(f).lambda;
        let feud = self.feuds.get(&(f, a)).expect("exists");
        let Some(d) = feud.demand else {
            return;
        };
        let cut = feud.g.rate[d.kind.index()] <= 0.0 && d.after <= 0.0;
        if d.paid >= d.tribute {
            self.settle(w, f, a, boundary::SETTLED_PAID, at);
        } else if cut {
            self.settle(w, f, a, boundary::SETTLED_CUT, at);
        } else {
            self.stats.expired += 1;
            self.settle_outbox(w, f, a, d, boundary::SETTLED_EXPIRED);
            self.envoy_home(w, d.envoy, at);
            let g = self.feuds[&(f, a)].g.total_at(at, lambda);
            let feud = self.feuds.get_mut(&(f, a)).expect("exists");
            feud.demand = None;
            feud.party = None;
            self.note(
                at,
                Some(f),
                "refused",
                format!(
                    "the demand on {} goes unmet (grievance {g:.1})",
                    self.actor_name(w, a)
                ),
            );
            if g >= self.theta(f, Step::Scouts) * (1.0 - 1e-9) {
                self.escalate(w, f, a, Step::Scouts, at);
            } else if g < self.down_theta(f, Step::Envoy) {
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
                feud.step = Step::Notice;
                feud.since = at;
            } else {
                let feud = self.feuds.get_mut(&(f, a)).expect("exists");
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
        let feud = self.feuds.get_mut(&(f, a)).expect("exists");
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
            .find(|a| self.feuds.get(&(f, *a)).is_some_and(|x| x.demand.is_some()));
        let Some(a) = who else {
            // No demand open: a tribute is a gift.
            self.gift_value(w, f, payer, goods, at);
            return false;
        };
        let feud = self.feuds.get_mut(&(f, a)).expect("found");
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
            && let Some(x) = self.feuds.get_mut(&(f, a))
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
        if let Some(feud) = self.feuds.get_mut(&(f, a)) {
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
        let mut out = vec![(start.x, start.z)];
        let direct = dist(pxz(start), pxz(to));
        let nearest = self
            .nodes
            .nearest(w, to, direct, |i| w.nodes[i].pos.frame == to.frame)
            .map(|b| b.1);
        if let Some(goal) = nearest.filter(|&g| g != from) {
            // A* over road edges (lanes are ships'), the straight line as the heuristic: an edge
            // is never shorter than the straight line between its ends (the path factor is at
            // least 1), so the first time the goal is popped its path is the shortest.
            let gp = pxz(w.nodes[goal].pos);
            let h = |i: usize| dist(pxz(w.nodes[i].pos), gp) as u64;
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
            t = t + ((d / speed).ceil() as u64).max(1);
            out.push(t);
        }
        out
    }

    /// Musters a party at the camp. War parties draw on the pool (at least half their make-up, or
    /// none goes); envoys and scouts always go.
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
        let want: u32 = make.iter().map(|u| u.1).sum();
        let units = if kind.is_war() {
            let have = self.pool_at(f, at).floor() as u32;
            if have * 2 < want {
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
            let total: u32 = v.iter().map(|u| u.1).sum();
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
        let pos = self.factions[f as usize].pos;
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
        let path = Path {
            frame: self.parties[p].frame,
            x0: from.0,
            z0: from.1,
            x1: to.0,
            z1: to.1,
            t0,
            t1: t1.max(t0),
            radius: self.tun.party_radius,
        };
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

    /// Where a party is at `t`.
    pub fn party_pos(&self, p: Handle<Party>, t: Tick) -> (i32, i32) {
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

    fn party_event(&mut self, w: &mut World, infra: &mut dyn Infrastructure, p: Handle<Party>, at: Tick) {
        match self.parties[p].phase {
            Phase::Mustering => {
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
        if embodied {
            let speed = self.culture(self.parties[p].faction).def.march_blocks_per_tick as f32;
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
        if phase == Phase::Out && matches!(kind, PartyKind::Raid | PartyKind::Invasion | PartyKind::Siege) {
            let mut regions: Vec<(i32, i32)> = Vec::new();
            let steps = 8;
            for k in 0..=steps {
                let x = i64::from(a.0) + (i64::from(b.0) - i64::from(a.0)) * k / steps;
                let z = i64::from(a.1) + (i64::from(b.1) - i64::from(a.1)) * k / steps;
                let r = LegIndex::region_of(x, z);
                let r = (r.0 as i32, r.1 as i32);
                if !regions.contains(&r) {
                    regions.push(r);
                }
            }
            if let Some(tr) = infra.trains_crossed(frame, &regions, t0.max(at), t1).first().copied() {
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
        let now = self.party_pos(p, at);
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

    fn strength_by_class(&self, p: Handle<Party>) -> BTreeMap<rtsim_industry::threat::ThreatClass, (u32, f64)> {
        let x = &self.parties[p];
        let c = self.culture(x.faction);
        let mut m = BTreeMap::new();
        for &(u, n) in &x.units {
            if n == 0 {
                continue;
            }
            let ud = c.unit(u);
            let e = m.entry(ud.class.threat()).or_insert((0u32, 0.0f64));
            e.0 += n;
            e.1 += f64::from(n) * ud.strength;
        }
        m
    }

    fn strength(&self, p: Handle<Party>) -> f64 {
        let x = &self.parties[p];
        let c = self.culture(x.faction);
        x.units
            .iter()
            .map(|&(u, n)| f64::from(n) * c.unit(u).strength)
            .sum()
    }

    fn ambush(&mut self, infra: &mut dyn Infrastructure, p: Handle<Party>, train: u64, at: Tick, w: &mut World) {
        let id = fnv1a64(&[Party::agent(p).to_le_bytes(), at.0.to_le_bytes(), train.to_le_bytes()].concat());
        let s = self.strength(p);
        let n = self.parties[p].alive();
        let t = Threat {
            id,
            source: ThreatSource::Bandit,
            class: rtsim_industry::threat::ThreatClass::GroundMelee,
            count: n,
            strength: s,
            target: ThreatTarget::Train(rtsim_industry::ids::TrainId(train)),
            at,
        };
        let Some(loot) = infra.raid_train(&t) else {
            return;
        };
        self.stats.ambushes += 1;
        let got = self.take_industry_loot(w, infra, p, &loot);
        let f = self.parties[p].faction;
        self.note(at, Some(f), "ambush", format!("falls on train {train} on the way: {got} items"));
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
            let name = infra.good_name(g).unwrap_or_else(|| format!("good {}", g.0));
            let short = name.rsplit(':').next().unwrap_or(&name).to_string();
            match w.goods.find(&name).or_else(|| w.goods.find(&short)) {
                Some(wg) => {
                    let a = n.saturating_mul(MILLI);
                    *self.parties[p].from_players.entry(wg).or_default() += a;
                    add(&mut self.ledger.from_players, wg, a);
                    got += n;
                }
                None => *self.ledger.unmapped.entry(name).or_default() += n,
            }
        }
        self.stats.items_from_districts += got.max(0) as u64;
        got
    }

    /// A party reaches its target.
    fn arrive(&mut self, w: &mut World, infra: &mut dyn Infrastructure, p: Handle<Party>, at: Tick) {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let kind = self.parties[p].kind;
        let bend = self.temper(f).bend(Trait::Bold, 1000, 50);
        let patience = (days_to_ticks(c.def.patience_days) as f64 * f64::from(bend) / 1000.0) as u64;
        self.parties[p].bored_at = at + patience.max(1);
        let target = self.parties[p].target;
        let tpos = self.target_pos(w, target);
        match kind {
            PartyKind::Envoy => {
                let deadline = self.parties[p]
                    .feud
                    .and_then(|a| self.feuds.get(&(f, a)))
                    .and_then(|x| x.demand)
                    .map_or(at, |d| d.deadline);
                self.wait(w, p, deadline.max(at + 1), at);
                self.note(at, Some(f), "envoy", format!("the envoy waits at ({}, {})", tpos.x, tpos.z));
            }
            PartyKind::Scouts => {
                self.wait(w, p, at + days_to_ticks(c.def.linger_days), at);
                self.note(at, Some(f), "scouts", format!("scouts watch ({}, {}) from the edge", tpos.x, tpos.z));
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

    fn wait_over(&mut self, w: &mut World, infra: &mut dyn Infrastructure, p: Handle<Party>, at: Tick) {
        let x = &self.parties[p];
        let (kind, f) = (x.kind, x.faction);
        if kind == PartyKind::Siege && at < x.bored_at && x.alive() > 0 {
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
                self.note(at, Some(f), "bored", "the band gets bored and goes home".into());
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
                    .then(|| (dist(pxz(pp), pxz(pos)), *a))
            })
            .filter(|(d, _)| *d <= r)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|e| e.1)
    }

    /// An unwatched raid, resolved abstractly: through the industry threat interface for a
    /// district (C16 to C18), through the world's markets for a village or a haven.
    fn resolve(&mut self, w: &mut World, infra: &mut dyn Infrastructure, p: Handle<Party>, at: Tick) {
        let f = self.parties[p].faction;
        let target = self.parties[p].target;
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
        // Vengeance: what it took eases the feud it served (capped).
        if let Some(a) = self.parties[p].feud {
            let c = self.culture(f);
            let lambda = c.lambda;
            let v = c.def.vengeance_per_item * (stolen as f64 + blocks as f64 * self.tun.block_value_items);
            let cap = self.tun.vengeance_cap_share;
            if let Some(feud) = self.feuds.get_mut(&(f, a)) {
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
        for (class, (count, strength)) in classes {
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
                source: ThreatSource::Bandit,
                class,
                count,
                strength,
                target: ThreatTarget::District(d),
                at,
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
                o.floor.map_or(String::new(), |fl| format!(", floor {fl:.2}")),
                if o.untested { ", untested defences" } else { "" },
                o.destroyed_blocks,
                if o.machine_lost { ", a machine lost" } else { "" }
            ));
        }
        self.note(at, Some(f), "raid", format!("raids district {} unwatched: {}", d.0, words.join("; ")));
        (stolen_items, blocks)
    }

    /// Takes `n` units away from the stacks whose class matches, in stack order.
    fn lose_units(&mut self, p: Handle<Party>, class: impl Fn(crate::culture::UnitClass) -> bool, mut n: u32) {
        let f = self.parties[p].faction;
        let classes: Vec<bool> = self.parties[p]
            .units
            .iter()
            .map(|&(u, _)| class(self.culture(f).unit(u).class))
            .collect();
        let x = &mut self.parties[p];
        for (k, e) in x.units.iter_mut().enumerate() {
            if n == 0 {
                break;
            }
            if classes[k] {
                let t = e.1.min(n);
                e.1 -= t;
                n -= t;
                self.stats.casualties += u64::from(t);
            }
        }
    }

    /// A raid on a village's or a rival haven's market: Lanchester's linear law against the
    /// residents' defence, floored by real fights seen (never better for the defender than
    /// observed); the survivors carry off up to their carry and a share of each good, into the
    /// band's hold (`World::stock_to_held`), conservation exact.
    fn raid_node(&mut self, w: &mut World, p: Handle<Party>, i: usize, at: Tick) -> i64 {
        let f = self.parties[p].faction;
        let c = self.culture(f).clone();
        let s = self.strength(p);
        let node = &w.nodes[i];
        let per = match node.kind {
            NodeKind::Haven(_) => self.tun.haven_defence_per_pop,
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
        let carry: i64 = self.parties[p]
            .units
            .iter()
            .map(|&(u, n)| i64::from(n) * i64::from(c.unit(u).carry))
            .sum::<i64>()
            * MILLI;
        let m = w.market(i);
        let caps: Vec<(GoodId, Amount)> = w
            .goods
            .ids()
            .map(|g| (g, m.stock(g) * c.def.loot_share_milli / 1000))
            .filter(|e| e.1 > 0)
            .collect();
        let shares = apportion(carry.min(caps.iter().map(|e| e.1).sum()), &caps);
        let want: BTreeMap<GoodId, Amount> = shares.into_iter().filter(|e| e.1 > 0).collect();
        let got = w.stock_to_held(i, &want, at);
        let mut items = 0;
        for (g, a) in &got {
            *self.parties[p].loot.entry(*g).or_default() += a;
            add(&mut self.ledger.from_villages, *g, *a);
            items += a / MILLI;
        }
        self.stats.items_from_villages += items.max(0) as u64;
        // The village remembers who robbed it.
        let fa = self.factions[f as usize].actor;
        let va = w.nodes[i].actor;
        let pos = w.nodes[i].pos;
        let r = w.knowledge.record(Report {
            who: fa,
            whom: va,
            kind: ReportKind::Robbery,
            severity: 2,
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
            if let Some(feud) = self.feuds.get_mut(&(f, a))
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
        if kind.is_war() {
            self.pool_add(f, f64::from(alive), at);
        }
        if chief {
            self.factions[f as usize].chief_out = false;
        }
        if let Some(l) = self.parties[p].leg.take() {
            w.legs.cancel(l.leg);
        }
        if let Some(a) = self.parties[p].feud
            && let Some(feud) = self.feuds.get_mut(&(f, a))
            && feud.party == Some(p)
        {
            feud.party = None;
            feud.since = at;
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
            format!("{} home with {alive} left and {items} items for the haven", kind.name()),
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
        self.ensure_chief(w, f, at);
        let c = self.culture(f).clone();
        let camp = self.factions[f as usize].pos;
        let reach = f64::from(c.def.plunder_reach_blocks);
        let own = self.factions[f as usize].node;
        self.nodes.refresh(w);
        let by_key = &self.by_key;
        let cultures = &self.cultures;
        let factions = &self.factions;
        let best = self.nodes.nearest(w, camp, reach, |i| {
            let n = &w.nodes[i];
            if i == own || n.pos.frame != camp.frame {
                return false;
            }
            match n.kind {
                NodeKind::Village(cu) => c.hates(cu.name()),
                NodeKind::Haven(_) => by_key
                    .get(&n.key)
                    .is_some_and(|&o| c.hates(cultures.get(factions[o as usize].culture).name())),
                NodeKind::PlayerBase { .. } => false,
            }
        });
        let Some((_, i)) = best else {
            self.schedule_plunder(f, at);
            return;
        };
        if self.send_party(w, f, None, PartyKind::Plunder, Target::Node(i), at).is_none() {
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
        let centre = ppos.map_or(*x.route.last().expect("route"), |pp| (pp.x, pp.z));
        let bearing = (f64::from(from.1 - centre.1)).atan2(f64::from(from.0 - centre.0)).to_degrees() as f32;
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
        self.schedule(at + self.tun.wave_interval_ticks, Ev::Wave { party: p, plan });
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
        let (centre, bearing, radius, half) = match arc {
            Some((c0, b)) => (c0, b, self.tun.spawn_edge_blocks, self.tun.arc_half_width_deg),
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
            flags: (if x.kind.is_war() { boundary::BAND_HOSTILE } else { 0 })
                | boundary::BAND_SURFACE
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
        if x.phase != Phase::Fighting {
            return;
        }
        let Some(player) = x.seen_by else {
            return;
        };
        let alive = x.alive();
        let in_mc: u32 = x.spawned.iter().map(|e| e.1).sum();
        if alive > in_mc {
            let ppos = w.players.get(&player).and_then(|s| s.pos).map(|e| (e.0.x, e.0.z));
            let from = x.route[x.route.len().saturating_sub(2)];
            let centre = ppos.unwrap_or(*x.route.last().expect("route"));
            let bearing =
                (f64::from(from.1 - centre.1)).atan2(f64::from(from.0 - centre.0)).to_degrees() as f32;
            self.spawn_wave(w, p, player, Some((centre, bearing)), at);
        }
        let plan = self.parties[p].plan;
        self.schedule(at + self.tun.wave_interval_ticks, Ev::Wave { party: p, plan });
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
                self.rescue_news(w, f, player, at);
            }
        }
        if reason == boundary::RECALL_BORED {
            self.stats.bored += 1;
        }
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

    /// Villages that hate the faction hear that a player broke its band (C24: rescues count).
    fn rescue_news(&mut self, w: &mut World, f: u32, player: ActorId, at: Tick) {
        let c = self.culture(f).clone();
        let camp = self.factions[f as usize].pos;
        let r = f64::from(self.tun.rescue_news_blocks);
        let mut heard = 0;
        for i in 0..w.nodes.len() {
            let n = &w.nodes[i];
            let NodeKind::Village(cu) = n.kind else {
                continue;
            };
            let hates_us = c.hates(cu.name());
            if !hates_us || n.pos.frame != camp.frame || dist(pxz(n.pos), pxz(camp)) > r {
                continue;
            }
            let (va, pos) = (n.actor, n.pos);
            let rep = w.knowledge.record(Report {
                who: player,
                whom: va,
                kind: ReportKind::Rescue,
                severity: 2,
                flags: 0,
                at,
                place: Place::new(pos.frame, pos.x, 64, pos.z),
            });
            if w.knowledge.witness(va, rep, at) {
                heard += 1;
            }
        }
        self.note(at, Some(f), "rescue", format!("{heard} village(s) that hate the {} hear the band was broken", c.name()));
    }

    // ---- seen on the march -----------------------------------------------------------------

    fn seen(&mut self, w: &mut World, p: Handle<Party>, player: ActorId, m: Meeting, at: Tick) {
        if !w.legs.is_meeting_live(&m) || !self.parties.contains(p) {
            self.stats.stale += 1;
            return;
        }
        let x = &self.parties[p];
        if x.embodied || matches!(x.phase, Phase::Home | Phase::Mustering | Phase::Fighting) {
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
        self.note(at, Some(f), "seen", format!("{} seen by a player", kind.name()));
    }

    fn materialize_envoy(&mut self, w: &World, p: Handle<Party>, player: ActorId, at: Tick) {
        let x = &self.parties[p];
        let f = x.faction;
        let c = self.culture(f).clone();
        let here = self.party_pos(p, at);
        let demand = x.feud.and_then(|a| self.feuds.get(&(f, a))).and_then(|fd| fd.demand);
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
                (d.tribute - d.paid).max(0) as i32,
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
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            self.now = self.now.max(at);
            w.run_until(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        self.now = self.now.max(to);
    }

    fn handle(&mut self, w: &mut World, infra: &mut dyn Infrastructure, at: Tick, ev: Ev) {
        match ev {
            Ev::Feud {
                faction,
                actor,
                stamp,
            } => {
                if self.feuds.get(&(faction, actor)).is_none_or(|f| f.stamp != stamp) {
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
        }
    }

    // ---- players ----------------------------------------------------------------------------

    /// Where a player is, through the world's shared interest area (choice U2): the area is kept
    /// or booked anew and routed to this engine.
    pub fn observe_player(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) -> ActorId {
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

    /// A session ended: every puppet is gone (they never outlive a session, S11).
    pub fn session_start(&mut self) {
        let ids: Vec<Handle<Party>> = self.parties.iter().map(|(h, _)| h).collect();
        for p in ids {
            let x = &mut self.parties[p];
            x.embodied = false;
            x.seen_by = None;
            x.spawned.clear();
        }
        self.outbox.retain(|c| c.agent().is_none());
    }

    // ---- observations (docs/boundary.md §9) --------------------------------------------------

    fn first_time(&mut self, op: u64) -> bool {
        self.applied_ops.insert(op)
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

    /// A record from the plugin, at its game time.
    pub fn ingest(&mut self, w: &mut World, infra: &mut dyn Infrastructure, o: &Obs, at: Tick) {
        let at = at.max(self.now);
        self.run_until(w, infra, at);
        if let Some(op) = o.op()
            && !self.first_time(op)
        {
            return;
        }
        match o {
            Obs::State(_) | Obs::Materialized(_) => {}
            Obs::FactionFound(ff) => {
                let Some(name) = self.java_dict.get(&(4, ff.structure)).cloned() else {
                    return;
                };
                let Some(ci) = self.cultures.of_structure(&name) else {
                    return;
                };
                let cname = self.cultures.get(ci).name().to_string();
                let pos = Pos::new(ff.dimension as u16, ff.pos.0, ff.pos.2);
                self.add_faction(w, ff.faction, &cname, pos, at);
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
                    pos: Pos::new(s.dimension as u16, s.pos.0, s.pos.2),
                    district: None,
                };
                self.signal(w, sig, at);
            }
            Obs::Pollution(pl) => {
                let Some(name) = self.java_dict.get(&(boundary::DICT_POLLUTION, pl.ptype)).cloned()
                else {
                    return;
                };
                let Some((kind, scale)) = self.tun.pollution_signal(&name) else {
                    return;
                };
                let per_day = if pl.window_ticks == 0 {
                    0.0
                } else {
                    pl.amount.max(0) as f64 * scale * TICKS_PER_DAY as f64 / f64::from(pl.window_ticks)
                };
                let pos = Pos::new(pl.dimension as u16, pl.x, pl.z);
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
                let back_of = |u: u8, dict: &BTreeMap<(u8, u32), String>, cultures: &Cultures| -> u32 {
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
            }
            Obs::Casualties(cs) => self.casualties(w, cs, at),
            Obs::ChiefKilled(ck) => {
                let Some(p) = Party::of_agent(ck.agent).filter(|p| self.parties.contains(*p)) else {
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
                    *self.parties[p].from_players.entry(g).or_default() += a;
                    add(&mut self.ledger.from_players, g, a);
                }
            }
        }
    }

    fn refuse(&mut self, w: &mut World, f: u32, a: ActorId, at: Tick, _insult: f64) {
        if let Some(feud) = self.feuds.get_mut(&(f, a))
            && let Some(d) = feud.demand.as_mut()
        {
            d.deadline = at;
            self.demand_due(w, f, a, at);
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
            name.as_deref() == Some(c.unit(u).body.as_str()) || body_id(&self.cultures, &c.unit(u).body) == cs.body
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
                self.schedule(at + self.tun.wave_interval_ticks, Ev::Wave { party: p, plan });
            }
        } else if x.alive() == 0 && x.phase != Phase::Home {
            self.go_home(w, p, boundary::RECALL_ROUTED, at);
        }
    }

    /// The war chief falls (C24, C25): the band routs, the heir takes over holding the grudge,
    /// the faction is cowed for a while (a bold heir recovers sooner), and the killer's feud drops
    /// rungs.
    pub fn chief_killed(&mut self, w: &mut World, p: Handle<Party>, killer: Option<ActorId>, at: Tick) {
        let f = self.parties[p].faction;
        if !self.parties[p].chief {
            return;
        }
        self.stats.chiefs_killed += 1;
        self.parties[p].chief = false;
        let chief = self.factions[f as usize].chief;
        let heir = self.factions[f as usize].heir;
        if let Some(k) = killer {
            let pos = self.factions[f as usize].pos;
            let r = w.knowledge.record(Report {
                who: k,
                whom: chief,
                kind: ReportKind::Killing,
                severity: 4,
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
        let cowed = (days_to_ticks(self.culture(f).def.cowed_days) as f64 * 1000.0 / f64::from(bend)) as u64;
        self.factions[f as usize].cowed_until = at + cowed;
        if let Some(a) = self.parties[p].feud.or(killer) {
            let steps = self.tun.chief_rout_steps;
            if let Some(feud) = self.feuds.get_mut(&(f, a)) {
                for _ in 0..steps {
                    feud.step = feud.step.down();
                }
                feud.step = feud.step.max(Step::Notice);
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
        self.note(at, Some(f), "chief", format!("the war chief falls: the band routs; cowed for {:.1} days", cowed as f64 / TICKS_PER_DAY as f64));
        let actors: Vec<ActorId> = self
            .feuds
            .range((f, Handle::NULL)..)
            .take_while(|(k, _)| k.0 == f)
            .map(|(k, _)| k.1)
            .collect();
        for a in actors {
            self.plan_feud(f, a, at, false);
        }
    }

    // ---- reads ------------------------------------------------------------------------------

    /// A feud as it stands at the engine's time (a projection).
    pub fn feud_view(&self, faction: u64, actor: ActorId) -> Option<FeudView> {
        let f = *self.by_key.get(&faction)?;
        let x = self.feuds.get(&(f, actor))?;
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
            if *taken != landed + h {
                return Err(format!("good {}: taken {taken}, landed {landed}, held {h}", g.0));
            }
            if h > w.outside.held.get(g).copied().unwrap_or(0) {
                return Err(format!("good {}: parties hold more than the world's held", g.0));
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
            if *taken != landed + c {
                return Err(format!("good {}: from players {taken}, landed {landed}, carried {c}", g.0));
            }
        }
        w.check_conserved(w.now()).map_err(|e| e.to_string())
    }

    pub fn heap_bytes(&self) -> usize {
        self.factions.len() * (std::mem::size_of::<Faction>() + 16 * 16)
            + self.feuds.len() * (std::mem::size_of::<Feud>() + 48)
            + self.sources.values().map(|s| 64 + s.affects.len() * 16).sum::<usize>()
            + self
                .parties
                .iter()
                .map(|(_, x)| std::mem::size_of::<Party>() + x.route.len() * 16 + x.units.len() * 8)
                .sum::<usize>()
            + self.wheel.heap_bytes()
            + self.applied_ops.len() * 16
    }
}

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
        floor.map_or(lanchester, |fl| lanchester.max(fl)).clamp(0.0, 1.0),
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
        // Keep the UUID: a player who comes back is the same actor.
        let _ = uuid_hash;
    }
}
