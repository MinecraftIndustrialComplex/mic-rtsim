// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships engine: timetabled merchant and navy ships, pirates at havens, and the hazards they
//! put on lanes (PLAN.md §10.1, §10; C5, C24, C25; A13, A15; M2's recommendation).
//!
//! # What happens, and when
//!
//! - **Merchant ships** sail their service's timetable ([`crate::timetable`]). They carry the
//!   lanes' flows (the world's pipelines on lane edges, A13), and cost nothing while nobody
//!   watches: no event, no booking, no row change. Their position is a closed form of the clock.
//! - **Enters view** (A15). A player's interest area is booked in the shared leg index. Booking it
//!   looks up the lanes near it in a static cell index, books there the voyages that overlap its
//!   window (only those: a voyage nobody could see is never booked), and asks the index for every
//!   ship leg it shares cells with; the first moment each ship comes within range is one `Seen`
//!   event, stamped with both legs' generations. If it is still live when it fires, the ship and
//!   its crew materialize as one group (C25).
//! - **Becoming a row** (SCALE.md §3). When something happens to a ship (looted, raided, boarded),
//!   the consignments it carries are cut out of the flow and held by it until it docks, when it
//!   sells them to the port as a caravan row would (C14).
//! - **Pirates** live at havens (C24). Each rests and refits in port (its hull mended, its crew made
//!   up from the haven's pool), chooses a lane to hunt among those worth it (EQUILIBRIUM.md §2.4.6
//!   (b): a win chance it believes above 2/3 against a merchant and the lane's patrols and escorts;
//!   with none it rests; then expected loot, cut by patrols, bent by its captain's temperament,
//!   C25), sails there on a booked leg, hunts until it
//!   gets bored (a scheduled departure, brought forward by damage or a full hold, C24), and sails
//!   home. While it hunts, the lane's hazard is the share of each consignment that sails to the
//!   haven's market instead (`rtsim-world`'s raid pipelines), so stolen goods stay in the totals.
//!   A hunt may also start with a raid on a player district by the coast or a player's booked
//!   leg on the lane, through the industry threat interface (C16 to C18).
//! - **Individual raids.** A merchant seen by a player on a hunted lane is attacked with the
//!   lane's hazard as the chance: the pirate materializes next to it and the plugin plays the
//!   fight. If nobody is watching when the raid comes, it is resolved abstractly by Lanchester's
//!   square law, never better for the defender than the real fights observed ([`crate::fight`]).
//! - **Navies** (M2's recommendation): a patrol service of navy ships whose presence on the lanes
//!   it sails lowers their hazard. Players' escorts add to it ([`Ships::set_escort`]).
//! - **Losses come back** (unlike Veloren's captains): a lost merchant or navy ship relaunches at
//!   its home dock after a delay, on its own saved slot. A haven refits a lost pirate ship's slot
//!   after `min(96, 4 x 2^P)` days once its crew pool can man it (§2.4.6, the shared rule of §2.2:
//!   P rises by 1/slots for each pirate ship a player sinks or takes). The pool hires toward its
//!   cap of 18 in closed form, and captured crews join it up to the cap (M2's crew conversion).
//!
//! # Cost
//!
//! O(events). Unobserved merchant and navy ships cost nothing per tick and nothing per voyage;
//! pirates cost four events a hunt and one market event per hazard change. Encounters cost the
//! cells an interest area covers and the voyages booked there.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_archive::{At, Severity, Tape, kinds};
use rtsim_ecology::infra::Infrastructure;
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::rows::{PagedArena, PagedRows};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::{ActorId, ActorKind, Place, Report, ReportKind, SEVERITY_UNIT, Temperament};
use rtsim_knowledge::{Trait, witnesses};
use rtsim_space::topology::{Iso, SharedTopologies, Topology};
use rtsim_trade::{Amount, GoodId, MILLI, Price};
use rtsim_travel::Medium;
use rtsim_travel::legs::{LegId, LegIndex, Meeting, Owner, Path, Stamp};
use rtsim_world::World;
use serde::Serialize;

use crate::boundary::{self, By, Cmd, Item, Member, Obs, Role as WireRole, Uuid};
use crate::fight::{self, Observed, Pairing};
use crate::geom::{Polyline, dist};
use crate::network::{self, Dock, Lane, Loop, NetworkLints};
use crate::timetable::{self, Phase, Service, ServiceKind, VoyageTimes};
use crate::tunables::ShipTunables;
use crate::water::{self, WaterMask};

mod blockade;
pub use blockade::{HEAT_FLOOR_MILLI, PortState};

pub const SYS_SHIP: SystemId = SystemId::named("ships.ship");
const SYS_PIRATE: SystemId = SystemId::named("ships.pirate");
const SYS_RAID: SystemId = SystemId::named("ships.raid");
const SYS_SERVICE: SystemId = SystemId::named("ships.service");
const SYS_CARGO: SystemId = SystemId::named("ships.cargo");

/// The engine's op ids (SHIP_CARGO): allocator 0x53 in bits 56 to 62 (docs/boundary.md §2.2).
pub const OP_ALLOCATOR: u64 = boundary::AGENT_NAMESPACE << 56;

/// Leg owners: a ship's legs carry this bit over its 48-bit handle (players' interest areas use
/// the top bit, dragons the next, `rtsim-world`'s caravans none).
pub const SHIP_OWNER: u64 = 1 << 61;
const PLAYER_OWNER: u64 = 1 << 63;
const MASK48: u64 = (1 << 48) - 1;

pub type ShipId = Handle<Ship>;

/// Each ship of a service with its next departures: (hop, dock node, tick).
pub type Departures = Vec<(ShipId, Vec<(usize, usize, u64)>)>;

/// What a ship does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Merchant,
    Pirate { haven: u32 },
    Navy { navy: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Active,
    /// Sunk or captured at `since`; relaunched at `back` (merchants and navies), or refitted from
    /// then once the haven's pool can man it (pirates, EQUILIBRIUM.md §2.4.6). A pirate laid up
    /// (§2.4.6a rule 2) is lost with `back` [`LAID_UP`].
    Lost {
        since: Tick,
        back: Tick,
    },
}

/// A voyage whose cargo left the flow: the ship holds it until it docks at `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Promoted {
    pub voyage: u64,
    pub edge: u32,
    pub dir: u8,
    pub from: usize,
    pub to: usize,
    pub arrive: Tick,
}

/// A pirate's part of the cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PState {
    Resting { until: Tick },
    Outbound { lane: u32, arrive: Tick },
    Hunting { lane: u32, since: Tick, until: Tick },
    Homebound { arrive: Tick },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pirate {
    /// The captain, a knowledge notable (C24: it ages and dies on one booked event; its haven
    /// inherits).
    pub captain: ActorId,
    pub state: PState,
    /// Its current leg in the shared index.
    pub leg: Option<Stamp>,
    /// Goods taken from player districts, in the world's goods: they enter the world's books at
    /// the haven.
    pub district_loot: BTreeMap<GoodId, Amount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ship {
    pub role: Role,
    pub medium: Medium,
    pub frame: u16,
    pub seed: u64,
    /// Merchants and navies: the service whose timetable it sails, its slot, and its clock
    /// offset (saved: a restart never moves it).
    pub service: Option<u32>,
    pub slot: u32,
    pub offset: u64,
    pub crew: u32,
    pub hull_milli: u32,
    pub status: Status,
    pub epoch: u32,
    pub embodied: bool,
    /// Held goods (the world counts them as held): a promoted voyage's cargo, or a pirate's loot.
    pub cargo: BTreeMap<GoodId, Amount>,
    pub quotes: BTreeMap<GoodId, Price>,
    pub promoted: Option<Promoted>,
    pub plan: u32,
    pub pirate: Option<Pirate>,
}

impl Ship {
    pub fn agent(id: ShipId) -> u64 {
        boundary::AGENT_NAMESPACE << 56 | (id.to_bits() & MASK48)
    }

    pub fn id_of_agent(agent: u64) -> Option<ShipId> {
        (agent >> 56 == boundary::AGENT_NAMESPACE).then(|| ShipId::from_bits(agent & MASK48))
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, Status::Active)
    }

    /// A pirate laid up for want of prey (§2.4.6a rule 2): lost with no refit due.
    pub fn is_laid_up(&self) -> bool {
        matches!(self.status, Status::Lost { back, .. } if back == LAID_UP)
    }
}

/// A pirate haven (C24): a world node of kind Haven whose market sells what its pirates take.
#[derive(Clone, Debug, PartialEq)]
pub struct Haven {
    pub node: usize,
    pub frame: u16,
    pub dock: (i32, i32, i32),
    /// 1 sea pirates, 2 sky pirates.
    pub media: u32,
    /// Crew waiting ashore, thousandths, as of `pool_at`: it mans refitted ships and makes up
    /// crews in port. Hires fill it toward the cap in closed form ([`Ships::crew_pool_milli`],
    /// EQUILIBRIUM.md §2.4.6), and captured crews join it up to the cap (M2).
    pub pool_milli: u32,
    pub pool_at: Tick,
    /// Players' recent losses (§2.2): each pirate ship a player sinks or captures adds 1/slots,
    /// and an empty slot's refit waits `min(cap, base x 2^P)` days.
    pub pressure: Pressure,
    pub ships: Vec<ShipId>,
    /// Lanes within its pirates' reach, in lane order.
    pub lanes: Vec<u32>,
    /// Fed while `at < fed_until` (§2.4.6a rule 2, PC2): each hunt start sets it `starve_days`
    /// ahead, and so do the founding and a refound. After it the pool deserts instead of hiring
    /// ([`Ships::crew_pool_milli`]).
    pub fed_until: Tick,
    /// Shut down (§2.4.6a rule 4, PC4): no active ship, and starving (or, from rule 3, broken).
    /// `None` while open.
    pub shut: Option<Shut>,
    /// The stamp of the haven's one live [`Ev::Refound`]: a newer look voids the older.
    pub epoch: u32,
    /// What the lifecycle did here (tests, `sim`; saved with the row so a restart keeps them).
    pub counts: HavenCounts,
}

/// A shut haven (§2.4.6a rule 4): since when, when its site next looks for a new band (rule 5),
/// and whether a navy broke it (rule 3, a later slice; false while only starving shuts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shut {
    pub since: Tick,
    pub back: Tick,
    pub broken: bool,
}

/// A haven's lifecycle counters (§2.4.6a): kept on the haven rows, not in [`ShipStats`], so a
/// save with no haven (every saved world today, T36) keeps its bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct HavenCounts {
    /// Crew paid in tolls (rule 1), thousandths of a man: the square law's share.
    pub toll_milli: u64,
    /// Ships laid up (rule 2).
    pub laid_up: u64,
    /// Times shut (rule 4).
    pub shuts: u64,
    /// Times refounded (rule 5).
    pub refounds: u64,
}

impl HavenCounts {
    fn add(&mut self, o: &HavenCounts) {
        self.toll_milli += o.toll_milli;
        self.laid_up += o.laid_up;
        self.shuts += o.shuts;
        self.refounds += o.refounds;
    }
}

/// A laid-up slot's `Status::Lost::back` (§2.4.6a rule 2, PC3): never due, so no refit looks at
/// it until its haven is fed again (PC11) or refounds.
pub const LAID_UP: Tick = Tick(u64::MAX);

/// A navy (M2): a patrol service from a base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navy {
    pub node: usize,
    pub dock: u32,
    pub medium: Medium,
    pub service: u32,
    pub ships: Vec<ShipId>,
}

/// What threatens a lane and what protects it. Strengths are thousandths, so they are exact in
/// saves and hashes. A paged row keyed by its lane (table [`THREAT_TABLE`]); its two small sets
/// are sorted vectors, not B-trees (a B-tree with one entry allocates a node for eleven).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneThreat {
    /// The haven whose pirates hunt it (one at a time: its raids sail there).
    pub haven: Option<u32>,
    /// The pirates hunting it, sorted.
    pub hunters: Vec<ShipId>,
    /// Navy presence, thousandths of strength.
    pub patrol_milli: i64,
    /// Players' escorts by key, thousandths of strength (to be fed by a future plugin), sorted by
    /// key.
    pub escorts: Vec<(u64, i64)>,
    /// The share last applied, thousandths.
    pub share: i64,
    /// The lane's heat at `heat_at`, thousandths (§2.4.6a rule 7, PC20): an exponential average
    /// of `share` with time constant `blockade_days`. Between changes of `share` it relaxes
    /// toward `share` in closed form ([`Ships::heat_at`]); 0 at tick 0 with closures off.
    pub heat_milli: i64,
    /// When `heat_milli` was last brought up to date: the last change of `share`.
    pub heat_at: Tick,
}

impl LaneThreat {
    /// The heat at `t >= heat_at`, thousandths: the stored heat relaxed toward `share` over the
    /// elapsed time, with time constant `tau` ticks, rounded to the nearest thousandth. With
    /// `tau` 0 it is the stored heat (closures off keep no heat).
    pub fn heat(&self, t: Tick, tau: f64) -> i64 {
        if tau <= 0.0 || t <= self.heat_at || self.heat_milli == self.share {
            return self.heat_milli;
        }
        let k = (-((t.0 - self.heat_at.0) as f64) / tau).exp();
        let h = self.share as f64 + (self.heat_milli - self.share) as f64 * k;
        h.round() as i64
    }

    fn defence(&self) -> f64 {
        (self.patrol_milli + self.escorts_milli()) as f64 / 1000.0
    }

    /// Every escort's strength, thousandths.
    pub fn escorts_milli(&self) -> i64 {
        self.escorts.iter().map(|e| e.1).sum()
    }

    pub fn add_hunter(&mut self, s: ShipId) {
        if let Err(i) = self.hunters.binary_search(&s) {
            self.hunters.insert(i, s);
        }
    }

    pub fn remove_hunter(&mut self, s: ShipId) {
        if let Ok(i) = self.hunters.binary_search(&s) {
            self.hunters.remove(i);
        }
    }

    /// Sets escort `key`'s strength; 0 or less removes it.
    pub fn set_escort(&mut self, key: u64, milli: i64) {
        match self.escorts.binary_search_by_key(&key, |e| e.0) {
            Ok(i) if milli <= 0 => {
                self.escorts.remove(i);
            }
            Ok(i) => self.escorts[i].1 = milli,
            Err(_) if milli <= 0 => {}
            Err(i) => self.escorts.insert(i, (key, milli)),
        }
    }
}

/// The ship rows' page table (docs/choices-paging-modules.md: the module's table 0x5300 plus
/// 0x10, so rtsim-jni's per-table deletion of 0x5300 never touches it).
pub const SHIP_TABLE: u16 = 0x5310;
/// The lane threats' page table.
pub const THREAT_TABLE: u16 = 0x5311;
/// Ship rows per page. PLACEHOLDER 16: a service's ships are consecutive keys (at most
/// `max_ships_per_service`, 6 by default), so one or two pages hold a service, and an event that
/// reads one ship loads about 16 × 50 encoded bytes.
pub const SHIP_ROWS_PER_PAGE: u64 = 16;
/// Lane threat rows per page. PLACEHOLDER 64 (a threat row is about 20 bytes encoded).
pub const THREAT_ROWS_PER_PAGE: u64 = 64;
/// The port network's page in a paged save: written only by saves after it changed.
pub const NETWORK_KEY: rtsim_kernel::page::PageKey = rtsim_kernel::page::PageKey::new(0x5312, 0);

/// A service's ships: `n` consecutive row keys from `first` (they are created together and
/// handles are never reused, so the range is the whole index: nothing per ship).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ShipRange {
    pub(crate) first: u32,
    pub(crate) n: u32,
}

impl ShipRange {
    pub(crate) fn ids(self) -> impl Iterator<Item = ShipId> + Clone {
        (self.first..self.first + self.n).map(PagedArena::<Ship>::handle)
    }
}

/// The save tickets of one [`Ships::save_paged`]: hand them back with [`Ships::saved`] once the
/// commit is durable, or [`Ships::save_failed`].
#[derive(Debug)]
pub struct ShipTickets {
    pub(crate) ships: SaveTicket,
    pub(crate) threats: SaveTicket,
    /// The save wrote the network's page.
    pub(crate) network: bool,
}

impl ShipTickets {
    /// Row pages the save wrote.
    pub fn pages(&self) -> usize {
        self.ships.pages() + self.threats.pages()
    }

    /// Whether the save wrote the network's page (it changed since the last).
    pub fn wrote_network(&self) -> bool {
        self.network
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerView {
    pub(crate) uuid: Uuid,
    pub(crate) interest: Option<Stamp>,
}

/// A voyage booked in the leg index because an interest area could see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Booked {
    pub(crate) legs: Vec<LegId>,
    pub(crate) end: Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    Seen {
        ship: ShipId,
        voyage: u64,
        player: ActorId,
        meeting: Meeting,
    },
    SeenPirate {
        ship: ShipId,
        plan: u32,
        player: ActorId,
        meeting: Meeting,
    },
    Unbook {
        ship: ShipId,
        voyage: u64,
    },
    NextLeg {
        ship: ShipId,
        epoch: u32,
    },
    Arrive {
        ship: ShipId,
        voyage: u64,
    },
    Raid {
        pirate: ShipId,
        victim: ShipId,
        voyage: u64,
        plan: u32,
        player: ActorId,
    },
    Decide {
        ship: ShipId,
        plan: u32,
    },
    ReachLane {
        ship: ShipId,
        plan: u32,
    },
    Bored {
        ship: ShipId,
        plan: u32,
        /// 1 bored, 2 hold full, 3 damaged.
        reason: u32,
    },
    Home {
        ship: ShipId,
        plan: u32,
    },
    Respawn {
        ship: ShipId,
    },
    /// A lost pirate ship's slot is due its refit (it waits on, if the pool cannot man it yet).
    /// `plan` as in `Decide`: a slot has one live refit, and a capture that re-plans it voids the
    /// older one.
    Refit {
        ship: ShipId,
        plan: u32,
    },
    /// A haven's look (§2.4.6a rules 4 and 5): open, it shuts if it then has no active ship and
    /// starves (booked at the turn when its last ship went while it was fed); shut, its site
    /// takes a new band or looks again. `epoch` as the haven's: one live look per haven.
    Refound {
        haven: u32,
        epoch: u32,
    },
    /// A trade port's check (§2.4.6a rule 7, PC25): its lanes' heats are weighed, and it is
    /// blockaded, closed or reopened. `epoch` as the port's: one live check per port.
    DockCheck {
        dock: u32,
        epoch: u32,
    },
}

/// Counters for tests, the bench and the CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ShipStats {
    pub events: u64,
    pub stale: u64,
    pub interest_bookings: u64,
    pub voyages_booked: u64,
    pub legs_booked: u64,
    pub seen: u64,
    pub seen_stale: u64,
    pub seen_pirates: u64,
    pub materialized: u64,
    pub promotions: u64,
    pub landings: u64,
    pub hunts: u64,
    pub fruitless: u64,
    pub hazard_changes: u64,
    pub raids_observed: u64,
    pub raids_abstract: u64,
    pub fights_observed: u64,
    pub captures: u64,
    pub sinkings: u64,
    pub loot_events: u64,
    pub coast_raids: u64,
    pub respawns: u64,
    pub rebuilds: u64,
    pub captains_died: u64,
    pub bored: u64,
    /// Ports that close under blockade (§2.4.6a rule 7): blockades begun, ports closed and
    /// reopened, merchant ships laid up by a closure (each time one is), and the days ports were
    /// closed (whole days, credited at each reopening).
    pub blockades: u64,
    pub closures: u64,
    pub reopenings: u64,
    pub ships_laid_up_by_closure: u64,
    pub closed_dock_days: u64,
}

/// One line of what happened, for the CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub day: f64,
    pub kind: &'static str,
    pub ship: Option<String>,
    pub what: String,
}

/// Where goods went, beyond the world's own books (for the checks and the CLI).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShipLedger {
    /// Taken from merchant holds by pirates (held → held).
    pub looted_by_pirates: BTreeMap<GoodId, i128>,
    /// Pirate holds landed at havens' markets.
    pub landed_at_havens: BTreeMap<GoodId, i128>,
    /// Merchant holds landed at ports.
    pub landed_at_ports: BTreeMap<GoodId, i128>,
    /// Taken by players from holds.
    pub taken_by_players: BTreeMap<GoodId, i128>,
    /// Lost with sunk ships.
    pub sunk: BTreeMap<GoodId, i128>,
    /// From player districts and legs (the industry engine's stolen books), mapped to the world's
    /// goods, whole items × 1,000.
    pub from_districts: BTreeMap<GoodId, i128>,
    /// District loot landed at havens.
    pub districts_landed: BTreeMap<GoodId, i128>,
    /// District loot lost with a sunk pirate before it reached the haven.
    pub districts_lost: BTreeMap<GoodId, i128>,
    /// Industry goods with no world good of the same name: kept by the pirates, counted here.
    pub districts_unmapped: BTreeMap<String, i64>,
}

fn add(m: &mut BTreeMap<GoodId, i128>, g: GoodId, a: Amount) {
    *m.entry(g).or_default() += i128::from(a);
}

pub struct Ships {
    pub tun: ShipTunables,
    pub seed: u64,
    pub docks: Vec<Dock>,
    pub lanes: Vec<Lane>,
    pub services: Vec<Service>,
    /// Every ship, a paged row keyed by its handle's index ([`SHIP_TABLE`]): resident without a
    /// store, paged under a budget with one ([`Ships::attach_store`], [`Ships::open_paged`]).
    pub ships: PagedArena<Ship>,
    pub havens: Vec<Haven>,
    pub navies: Vec<Navy>,
    /// Lanes' threats, paged rows keyed by lane ([`THREAT_TABLE`]).
    pub threats: PagedRows<LaneThreat>,
    /// Trade ports' blockade states, by dock (§2.4.6a rule 7): only ports a hunted lane has
    /// touched have one, so the map grows with the docks pirates hunt near, never with the
    /// network; empty with closures off.
    pub ports: BTreeMap<u32, PortState>,
    /// Per dock: the lanes with it at an end, in lane order (an index, rebuilt on load).
    pub(crate) dock_lanes: Vec<Vec<u32>>,
    /// Per lane: the (service, hop) pairs that sail it.
    pub(crate) lane_hops: Vec<Vec<(u32, u32)>>,
    /// Per service: its ships, by slot (consecutive keys).
    pub(crate) service_ships: Vec<ShipRange>,
    /// The ships embodied now (observed only): what a session start makes abstract again, without
    /// a pass over every ship.
    pub(crate) embodied: BTreeSet<ShipId>,
    /// Lanes by region, for interest areas.
    pub(crate) lane_cells: BTreeMap<(u16, i64, i64), Vec<u32>>,
    pub(crate) dock_of: BTreeMap<(usize, Medium), u32>,
    pub(crate) wheel: TimingWheel<Ev>,
    pub(crate) players: BTreeMap<ActorId, PlayerView>,
    pub(crate) booked: BTreeMap<(ShipId, u64), Booked>,
    pub(crate) leg_voyage: BTreeMap<LegId, (ShipId, u64)>,
    /// Voyages that entered a player's view: once per player and voyage (a crossing), whatever
    /// the area's rebookings; forgotten when the voyage ends.
    pub(crate) seen_by: BTreeSet<(ShipId, u64, ActorId)>,
    /// The plugin's op ids already applied (CARGO_LOOTED, SHIP_SUNK, SHIP_CAPTURED), oldest first,
    /// so a replayed record applies once (§2.3's idempotent events). Bounded: PLACEHOLDER 4,096.
    pub(crate) applied_ops: std::collections::VecDeque<u64>,
    pub(crate) applied_set: BTreeSet<u64>,
    /// Real fights by pairing (the floors of [`crate::fight::resolve`]).
    pub observed: [Vec<Observed>; Pairing::COUNT],
    pub ledger: ShipLedger,
    pub stats: ShipStats,
    pub lints: NetworkLints,
    /// Docks with more (service, stop) pairs than berths.
    pub over_berths: Vec<(u32, u32)>,
    pub(crate) outbox: Vec<Cmd>,
    pub log_enabled: bool,
    pub log: Vec<LogEntry>,
    /// The event archive's recorder (VIEWER.md §10.1): off until the host turns it on; not state.
    pub tape: Tape,
    pub(crate) now: Tick,
    pub(crate) built: bool,
    /// The network changed since a paged save last wrote it (not saved: a paged save clears it,
    /// attaching a store sets it).
    pub(crate) network_dirty: bool,
    /// Every frame's topology (TP6): flat until the engine host hands over the TOPOLOGY records'.
    /// Lanes are built under the topology the network sees when it is built.
    pub topo: SharedTopologies,
}

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// The ships' day number for the shared renewal rule's pressure.
fn day_num(t: Tick) -> u32 {
    (t.0 / TICKS_PER_DAY).min(u64::from(u32::MAX)) as u32
}

fn tick_of(t: i64) -> Tick {
    Tick(t.max(0) as u64)
}

/// An event's place for the archive: a block position, the frame being the dimension.
fn spot(at: Tick, frame: u16, x: i32, z: i32) -> At {
    At::new(at.0, u32::from(frame), x, z)
}

/// A ship's archive id: its handle's plain bits.
fn sid(s: ShipId) -> u128 {
    u128::from(s.to_bits())
}

/// A village or base's archive id: its key.
fn nid(w: &World, node: usize) -> u128 {
    u128::from(w.nodes[node].key)
}

/// An actor's archive id: its knowledge handle, as every module records actors.
fn aid(a: ActorId) -> u128 {
    u128::from(a.to_bits())
}

/// The same 64-bit stand-in for a player's UUID the phase 1 world uses.
pub fn uuid_hash(most: u64, least: u64) -> u64 {
    rtsim_world::interest::uuid_hash(most, least)
}

/// Body kind dictionary (kind 3) ids for the abstract bodies: position in this list, plus 1.
pub fn body_names(tun: &ShipTunables) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for m in [&tun.sea, &tun.sky] {
        for n in [
            m.merchant_body,
            m.pirate_body,
            m.navy_body,
            m.merchant_crew_body,
            m.pirate_crew_body,
            m.navy_crew_body,
        ] {
            if !v.contains(&n) {
                v.push(n);
            }
        }
    }
    v
}

pub fn body_id(tun: &ShipTunables, name: &str) -> u32 {
    body_names(tun)
        .iter()
        .position(|n| *n == name)
        .map_or(0, |k| k as u32 + 1)
}

/// Item dictionary ids for the world's goods: the good's index plus 1 (the bridge's DICT records
/// name them).
pub fn item_of(g: GoodId) -> u32 {
    u32::from(g.0) + 1
}

pub fn good_of(item: u32) -> Option<GoodId> {
    item.checked_sub(1)
        .and_then(|i| u16::try_from(i).ok())
        .map(GoodId)
}

impl Ships {
    pub fn new(seed: u64, tun: ShipTunables, now: Tick) -> Ships {
        Ships {
            tun,
            seed,
            docks: Vec::new(),
            lanes: Vec::new(),
            services: Vec::new(),
            ships: PagedArena::new(SHIP_TABLE, SHIP_ROWS_PER_PAGE),
            havens: Vec::new(),
            navies: Vec::new(),
            threats: PagedRows::new(THREAT_TABLE, THREAT_ROWS_PER_PAGE),
            ports: BTreeMap::new(),
            dock_lanes: Vec::new(),
            lane_hops: Vec::new(),
            service_ships: Vec::new(),
            embodied: BTreeSet::new(),
            lane_cells: BTreeMap::new(),
            dock_of: BTreeMap::new(),
            wheel: TimingWheel::new(now),
            players: BTreeMap::new(),
            booked: BTreeMap::new(),
            leg_voyage: BTreeMap::new(),
            seen_by: BTreeSet::new(),
            applied_ops: std::collections::VecDeque::new(),
            applied_set: BTreeSet::new(),
            observed: Default::default(),
            ledger: ShipLedger::default(),
            stats: ShipStats::default(),
            lints: NetworkLints::default(),
            over_berths: Vec::new(),
            outbox: Vec::new(),
            log_enabled: false,
            log: Vec::new(),
            tape: Tape::new(),
            now,
            built: false,
            network_dirty: false,
            topo: rtsim_space::topology::all_flat(),
        }
    }

    /// Hands the engine every frame's topology (TP6). The lane index is rebuilt.
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) {
            return;
        }
        if *t != *self.topo {
            self.topo = t;
            self.lane_cells.clear();
            for l in 0..self.lanes.len() as u32 {
                self.index_lane(l);
            }
        }
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.topo.of(frame)
    }

    /// Distance in whole blocks through every seam; on the plane the old straight line.
    fn sdist(&self, frame: u16, a: (i32, i32), b: (i32, i32)) -> u64 {
        let t = self.topo(frame);
        if t.is_flat() {
            return dist(a, b);
        }
        t.dist(
            (i64::from(a.0), i64::from(a.1)),
            (i64::from(b.0), i64::from(b.1)),
        )
        .round() as u64
    }

    /// The canonical point `s` blocks along a lane (its path is a chart polyline).
    fn lane_at(&self, ln: &Lane, s: u64) -> (i32, i32) {
        let p = ln.path.at(s);
        self.topo(ln.frame).canonical_i32(p.0, p.1)
    }

    /// A straight leg between two points of one chart, from the first's canonical position
    /// (TP7).
    #[allow(clippy::too_many_arguments)]
    fn chart_leg(
        &self,
        frame: u16,
        a: (i32, i32),
        b: (i32, i32),
        t0: Tick,
        t1: Tick,
        radius: u32,
    ) -> Path {
        let f = self.topo(frame).fold(i64::from(a.0), i64::from(a.1));
        let b = f.iso.cell(i64::from(b.0), i64::from(b.1));
        Path {
            frame,
            x0: f.x as i32,
            z0: f.z as i32,
            x1: b.0 as i32,
            z1: b.1 as i32,
            t0,
            t1,
            radius,
        }
    }

    /// Where a service's ship is at `t`, canonical, with its lane chart's motion there.
    fn fix_of(&self, s: u32, offset: u64, t: Tick) -> timetable::Fix {
        self.services[s as usize].fix_in(&self.topo, &self.lanes, &self.docks, offset, t)
    }

    /// A lane's waypoints from `from`'s end, moved into a body's frame by `to_body` (lane chart
    /// → the bodies' frame, TP11).
    fn body_waypoints(&self, ln: &Lane, forward: bool, to_body: Iso) -> Vec<(i32, i32)> {
        let pts = if forward {
            ln.path.pts.clone()
        } else {
            ln.path.reversed().pts
        };
        if to_body.is_identity() {
            return pts;
        }
        pts.into_iter()
            .map(|p| {
                let q = to_body.cell(i64::from(p.0), i64::from(p.1));
                (q.0 as i32, q.1 as i32)
            })
            .collect()
    }

    /// Whether a lane's path crosses an edge_stop seam (TP12): no ship body sails it.
    fn lane_crosses_stop(&self, ln: &Lane) -> bool {
        let t = self.topo(ln.frame);
        if !t.any_stopped() {
            return false;
        }
        ln.path.pts.windows(2).any(|w| {
            let f = t.fold(i64::from(w[0].0), i64::from(w[0].1));
            let b = f.iso.cell(i64::from(w[1].0), i64::from(w[1].1));
            t.stopped_crossing((f.x, f.z), b).is_some()
        })
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    /// The census (EQUILIBRIUM.md S1) now: `ship.haven`, K its slots, its members the pirates
    /// afloat (not lost and not yet refitted), P its refit pressure; a shut haven is `broken`
    /// (§2.4.6a rule 4); renewals are refits and refounds.
    pub fn census(&self) -> Vec<rtsim_kernel::census::KindCensus> {
        let mut c = rtsim_kernel::census::KindCensus::new("ship.haven");
        let day = day_num(self.now);
        for hv in &self.havens {
            let up = hv
                .ships
                .iter()
                .filter(|&&p| self.ship(p).is_some_and(Ship::is_active))
                .count();
            c.site(
                up as u64,
                hv.ships.len() as u64,
                hv.shut.is_some(),
                hv.pressure.now(day, self.tun.refit.half_life_days),
            );
        }
        c.renewals = self.stats.rebuilds + self.haven_counts().refounds;
        vec![c]
    }

    /// Every haven's lifecycle counters, summed (§2.4.6a).
    pub fn haven_counts(&self) -> HavenCounts {
        let mut c = HavenCounts::default();
        for hv in &self.havens {
            c.add(&hv.counts);
        }
        c
    }

    pub fn pending_events(&self) -> usize {
        self.wheel.len()
    }

    /// A lower bound on the next event's time (O(levels)).
    pub fn next_due(&self) -> Option<Tick> {
        self.wheel.next_due_lower_bound()
    }

    pub fn take_commands(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.outbox)
    }

    fn schedule(&mut self, at: Tick, ev: Ev) {
        self.wheel.schedule(at.max(self.wheel.now()), ev);
    }

    fn note(&mut self, at: Tick, kind: &'static str, ship: Option<ShipId>, what: String) {
        if !self.log_enabled {
            return;
        }
        let ship = ship.map(|s| self.ship_name(s));
        self.log.push(LogEntry {
            tick: at.0,
            day: day_of(at),
            kind,
            ship,
            what,
        });
    }

    /// A short readable name: role, medium and slot.
    pub fn ship_name(&self, s: ShipId) -> String {
        match self.ships.get(s) {
            Some(x) => Self::label(x),
            None => format!("ship {}", s.to_bits()),
        }
    }

    /// A ship's readable name from its own row (the archive's payloads read the row alone).
    fn label(x: &Ship) -> String {
        let m = x.medium.name();
        match x.role {
            Role::Merchant => format!("{m} merchant {}.{}", x.service.unwrap_or(u32::MAX), x.slot),
            Role::Pirate { haven } => format!("{m} pirate {haven}.{}", x.slot),
            Role::Navy { navy } => format!("{m} navy {navy}.{}", x.slot),
        }
    }

    pub fn ship(&self, s: ShipId) -> Option<&Ship> {
        self.ships.get(s)
    }

    pub fn dock_of_node(&self, node: usize, medium: Medium) -> Option<u32> {
        self.dock_of.get(&(node, medium)).copied()
    }

    // ---- setup ---------------------------------------------------------------------------------

    /// A port joins: node `node`'s dock for `medium` at `dock` (a sea dock snaps to the nearest
    /// water cell). Ports serve storehouse markets only. Before [`Ships::build`] it waits for the
    /// network; after, it is linked at once by a shuttle to its nearest dock ([`Ships::connect`]).
    /// Idempotent per (node, medium).
    #[allow(clippy::too_many_arguments)]
    pub fn add_port(
        &mut self,
        w: &mut World,
        node: usize,
        medium: Medium,
        dock: (i32, i32, i32),
        berths: u32,
        mask: &dyn WaterMask,
        at: Tick,
    ) -> Option<u32> {
        if let Some(&d) = self.dock_of.get(&(node, medium)) {
            return Some(d);
        }
        if node >= w.nodes.len() || !w.nodes[node].kind.is_storehouse() || medium == Medium::Road {
            return None;
        }
        self.network_dirty = true;
        let frame = w.nodes[node].pos.frame;
        let (mut x, mut y, mut z) = dock;
        if medium == Medium::Sea {
            let Some(c) = water::snap_to_water(mask, frame, x, z, self.tun.port_snap_cells) else {
                self.lints.docks_not_on_water.push(self.docks.len() as u32);
                return None;
            };
            (x, z) = water::centre(mask, c);
            y = self.tun.sea_level;
        }
        let id = self.docks.len() as u32;
        self.docks.push(Dock {
            node,
            medium,
            frame,
            x,
            y,
            z,
            berths: berths.max(1),
        });
        self.dock_lanes.resize(self.docks.len(), Vec::new());
        self.dock_of.insert((node, medium), id);
        if self.built {
            self.connect(w, id, mask, at);
        }
        Some(id)
    }

    /// A haven joins (HAVEN_FOUND): node `node` (kind Haven) keeps pirates of the media in
    /// `media` (1 sea, 2 sky), `pirates` ships of each, with a crew pool to man one more (§2.4.6's
    /// founding: every slot manned and one spare crew).
    pub fn add_haven(
        &mut self,
        w: &mut World,
        node: usize,
        media: u32,
        dock: (i32, i32, i32),
        pirates: u32,
        at: Tick,
    ) -> Option<u32> {
        if node >= w.nodes.len() || !w.nodes[node].kind.is_storehouse() {
            return None;
        }
        if let Some(h) = self.havens.iter().position(|h| h.node == node) {
            return Some(h as u32);
        }
        let h = self.havens.len() as u32;
        let frame = w.nodes[node].pos.frame;
        self.havens.push(Haven {
            node,
            frame,
            dock,
            media,
            pool_milli: 0,
            pool_at: at,
            pressure: Pressure::default(),
            ships: Vec::new(),
            lanes: Vec::new(),
            // A new band counts as fed for its first month (§2.4.6a rule 5), founding as refound.
            fed_until: at + self.starve_ticks(),
            shut: None,
            epoch: 0,
            counts: HavenCounts::default(),
        });
        self.refresh_haven_lanes(h);
        let mut first_crew = 0;
        for medium in [Medium::Sea, Medium::Sky] {
            if media & medium_bit(medium) == 0 {
                continue;
            }
            for _ in 0..pirates {
                self.new_pirate(w, h, medium, at);
            }
            first_crew = first_crew.max(self.tun.medium(medium).pirate_crew);
        }
        self.havens[h as usize].pool_milli = first_crew.min(self.tun.crew_pool_cap) * 1000;
        Some(h)
    }

    // ---- a haven's crew pool and refits (EQUILIBRIUM.md §2.4.6) ---------------------------------

    /// `starve_days` in ticks (§2.4.6a rule 2).
    fn starve_ticks(&self) -> u64 {
        (self.tun.starve_days.max(0.0) * TICKS_PER_DAY as f64).round() as u64
    }

    /// When haven `h`'s hires stop and its men start to desert (§2.4.6a rule 2, PC2): its
    /// `fed_until`, or never with the lifecycle off (S8's pool).
    fn fed_until(&self, h: u32) -> Tick {
        if self.tun.lifecycle {
            self.havens[h as usize].fed_until
        } else {
            Tick(u64::MAX)
        }
    }

    /// Whether haven `h` starves at `at`: no hunt begun within `starve_days` (§2.4.6a rule 2).
    pub fn starving(&self, h: u32, at: Tick) -> bool {
        at >= self.fed_until(h)
    }

    /// Haven `h`'s crew pool at `at`, thousandths. Fed, hires add `r (1 - pool/cap)` a day, so
    /// from `pool0` at `pool_at` it is `cap - (cap - pool0) e^(-r t / cap)` (§2.4.6); after
    /// `fed_until` it deserts at the same rate, `p_turn e^(-r t / cap)` (§2.4.6a rule 2, PC2).
    /// One piecewise closed form, no event. With `hires_per_day <= 0` the pool is frozen: the
    /// desertion rate is the hire rate (one `k`), so a starving pool does not decay either
    /// (tests turn hires off to hold the pool still; a separate desertion rate would be a new
    /// tunable).
    pub fn crew_pool_milli(&self, h: u32, at: Tick) -> u32 {
        let hv = &self.havens[h as usize];
        if self.tun.hires_per_day <= 0.0 || at <= hv.pool_at {
            return hv.pool_milli;
        }
        let cap = self.tun.crew_pool_cap * 1000;
        let k = self.tun.hires_per_day / f64::from(self.tun.crew_pool_cap.max(1));
        let turn = self.fed_until(h).max(hv.pool_at);
        let mut p = hv.pool_milli;
        // Hires, up to the turn.
        let fed_to = at.min(turn);
        if fed_to > hv.pool_at && p < cap {
            let days = (fed_to.0 - hv.pool_at.0) as f64 / TICKS_PER_DAY as f64;
            let short = f64::from(cap - p) * (-k * days).exp();
            p = cap - (short.round() as u32).min(cap - p);
        }
        // Desertion, after it.
        if at > turn {
            let days = (at.0 - turn.0) as f64 / TICKS_PER_DAY as f64;
            p = ((f64::from(p) * (-k * days).exp()).round() as u32).min(p);
        }
        p
    }

    /// Haven `h`'s pool read at `at` and stored as of `at`, before it changes.
    fn settle_pool(&mut self, h: u32, at: Tick) {
        let p = self.crew_pool_milli(h, at);
        let hv = &mut self.havens[h as usize];
        hv.pool_milli = p;
        hv.pool_at = hv.pool_at.max(at);
    }

    /// When haven `h`'s pool will hold `need` crew (whole), from `at`: `at` if it does now, never
    /// (`None`) if hires cannot get there before the haven starves (§2.4.6a rule 2: after
    /// `fed_until` the pool only falls, and only a hunt start can feed it again). The inverse of
    /// [`Self::crew_pool_milli`], a tick late so the closed form reads at least `need` then. With
    /// hires off nothing is ever hired, so a short pool never gets there.
    fn crew_ready_at(&self, h: u32, need: u32, at: Tick) -> Option<Tick> {
        let have = self.crew_pool_milli(h, at);
        let need = need * 1000;
        if have >= need {
            return Some(at);
        }
        let cap = self.tun.crew_pool_cap * 1000;
        let fed = self.fed_until(h);
        if need > cap || self.tun.hires_per_day <= 0.0 || at >= fed {
            return None;
        }
        let k = self.tun.hires_per_day / f64::from(self.tun.crew_pool_cap.max(1));
        let days = (f64::from(cap - have) / f64::from((cap - need).max(1))).ln() / k;
        let t = at + (days * TICKS_PER_DAY as f64).ceil() as u64 + 1;
        (t <= fed).then_some(t)
    }

    /// Crew joining haven `h`'s pool (captured crews, M2), up to the cap. A slot that is due its
    /// refit and waits on crew looks again now (re-planned, so its later look is void).
    fn add_crew(&mut self, h: u32, crew: u32, at: Tick) {
        self.settle_pool(h, at);
        let cap = self.tun.crew_pool_cap * 1000;
        let hv = &mut self.havens[h as usize];
        hv.pool_milli = (hv.pool_milli + crew.saturating_mul(1000)).min(cap.max(hv.pool_milli));
        if hv.shut.is_some() {
            return;
        }
        let waiting: Vec<ShipId> = self.havens[h as usize]
            .ships
            .iter()
            .copied()
            .filter(|&s| {
                self.ships
                    .get(s)
                    .is_some_and(|x| matches!(x.status, Status::Lost { back, .. } if back <= at))
            })
            .collect();
        for s in waiting {
            let plan = self.bump(s);
            self.schedule(at, Ev::Refit { ship: s, plan });
        }
    }

    /// Takes up to `want` whole crew from haven `h`'s pool at `at`; what it got.
    fn take_crew(&mut self, h: u32, want: u32, at: Tick) -> u32 {
        self.settle_pool(h, at);
        let hv = &mut self.havens[h as usize];
        let got = want.min(hv.pool_milli / 1000);
        hv.pool_milli -= got * 1000;
        got
    }

    /// A pirate refits in port before it decides (§2.4.6): its hull is mended and its crew made
    /// up from the pool, as far as the pool goes beyond the crews of the haven's empty slots.
    /// Those come first, so an empty slot is never starved by top-ups.
    fn refit_in_port(&mut self, s: ShipId, at: Tick) {
        let Some(h) = self.haven_of(s) else {
            return;
        };
        let full = self.tun.medium(self.ships[s].medium).pirate_crew;
        let short = full.saturating_sub(self.ships[s].crew);
        let got = if short > 0 {
            let spare = (self.crew_pool_milli(h, at) / 1000).saturating_sub(self.due_crews(h));
            self.take_crew(h, short.min(spare), at)
        } else {
            0
        };
        let x = &mut self.ships[s];
        x.crew += got;
        x.hull_milli = 1000;
        if got > 0 && self.log_enabled {
            let pool = f64::from(self.crew_pool_milli(h, at)) / 1000.0;
            self.note(
                at,
                "crew",
                Some(s),
                format!("makes up {got} crew from haven {h}'s pool ({pool:.1} left ashore)"),
            );
        }
    }

    /// The crews haven `h`'s empty slots need (§2.4.6): held back from top-ups and tolls, so an
    /// empty slot is never starved by the ships afloat.
    fn due_crews(&self, h: u32) -> u32 {
        self.havens[h as usize]
            .ships
            .iter()
            .filter_map(|&o| self.ships.get(o))
            .filter(|x| matches!(x.status, Status::Lost { .. }))
            .map(|x| self.tun.medium(x.medium).pirate_crew)
            .sum()
    }

    /// The value a pirate of strength `a` puts on hunting a lane of `medium` defended by
    /// `defence` (its patrols and escorts), §2.4.6 (b): its win chance against a merchant there
    /// with that defence is believed to be `p = A² / (A² + D²)` (the square law's weights, not a
    /// roll: fights the engine resolves are certain), and the hunt is worth `p x loot - (1 - p) x
    /// crew`. Positive is worth hunting.
    pub fn hunt_value(&self, a: f64, medium: Medium, defence: f64) -> f64 {
        let t = &self.tun;
        let d = f64::from(t.medium(medium).merchant_crew) * t.merchant_strength + defence.max(0.0);
        let (wa, wd) = (
            t.attacker_effectiveness * a * a,
            t.defender_effectiveness * d * d,
        );
        let p = if wa + wd > 0.0 { wa / (wa + wd) } else { 0.0 };
        p * t.hunt_loot_value - (1.0 - p) * t.hunt_crew_value
    }

    /// Lane `l` as a pirate of haven `h`, of `medium` and strength `a`, sees it from port: if it
    /// may hunt it and it is worth hunting (§2.4.6 (b)), its flow volume, its patrols and
    /// escorts, and the square law's surviving share against those. Not its medium, not yet a
    /// world edge, held by another haven's hunters, empty, or not worth it: `None`. What
    /// [`Self::decide`] chooses among, and what a refound asks of a site (§2.4.6a rule 5, PC5).
    fn huntable(
        &self,
        w: &World,
        h: u32,
        medium: Medium,
        a: f64,
        l: u32,
    ) -> Option<(i64, f64, f64)> {
        let ln = &self.lanes[l as usize];
        if ln.medium != medium || ln.edge == u32::MAX {
            return None;
        }
        if let Some(t) = self.threat(l)
            && let Some(other) = t.haven
            && other != h
            && (!t.hunters.is_empty() || t.share > 0)
        {
            return None;
        }
        let vol = self.lane_volume(w, l);
        if vol <= 0 {
            return None;
        }
        let defence = self.threat(l).map_or(0.0, |t| t.defence());
        // Only a lane worth hunting (§2.4.6 (b)): escorted or patrolled beyond that, it is
        // left alone, and with no such lane the pirate rests in port.
        if self.hunt_value(a, medium, defence) <= 0.0 {
            return None;
        }
        let surv = fight::square_law(a, defence, &self.tun);
        if surv <= 0.0 {
            return None;
        }
        Some((vol, defence, surv))
    }

    fn refresh_haven_lanes(&mut self, h: u32) {
        let hv = &self.havens[h as usize];
        let (hx, hz) = (hv.dock.0, hv.dock.2);
        let r = self.tun.hunt_radius_blocks;
        let mut lanes: Vec<u32> = (0..self.lanes.len() as u32)
            .filter(|&l| {
                let ln = &self.lanes[l as usize];
                let mid = self.lane_at(ln, ln.path.length() / 2);
                ln.frame == hv.frame && self.sdist(ln.frame, mid, (hx, hz)) as i64 <= r
            })
            .collect();
        lanes.sort_unstable();
        self.havens[h as usize].lanes = lanes;
    }

    fn new_pirate(&mut self, w: &mut World, h: u32, medium: Medium, at: Tick) -> ShipId {
        let hv = &self.havens[h as usize];
        let slot = hv.ships.len() as u32;
        let node = hv.node;
        let frame = hv.frame;
        let seed = StreamKey::new(self.seed, SYS_PIRATE, u64::from(h))
            .block(u64::from(slot), self.stats.rebuilds)[0];
        let haven_actor = w.nodes[node].actor;
        let captain = w.knowledge.create_notable(
            seed,
            haven_actor,
            haven_actor,
            at,
            self.tun.captain_lifespan_ticks,
        );
        let mt = self.tun.medium(medium);
        let id = self.ships.insert(Ship {
            role: Role::Pirate { haven: h },
            medium,
            frame,
            seed,
            service: None,
            slot,
            offset: 0,
            crew: mt.pirate_crew,
            hull_milli: 1000,
            status: Status::Active,
            epoch: 0,
            embodied: false,
            cargo: BTreeMap::new(),
            quotes: BTreeMap::new(),
            promoted: None,
            plan: 0,
            pirate: Some(Pirate {
                captain,
                state: PState::Resting { until: at },
                leg: None,
                district_loot: BTreeMap::new(),
            }),
        });
        self.havens[h as usize].ships.push(id);
        // Staggered first decisions, so a haven's pirates do not all sail at once.
        let stagger = StreamKey::new(self.seed, SYS_PIRATE, seed)
            .slot(0)
            .below(self.tun.pirate_rest_ticks.max(1));
        let until = at + stagger;
        if let Some(p) = self.ships[id].pirate.as_mut() {
            p.state = PState::Resting { until };
        }
        self.schedule(until, Ev::Decide { ship: id, plan: 0 });
        id
    }

    /// Builds the network from the ports added so far: lanes, loops, services and their ships,
    /// and the world's lane edges. Called once; ports added later are linked by shuttles.
    pub fn build(&mut self, w: &mut World, mask: &dyn WaterMask, at: Tick) {
        if self.built {
            return;
        }
        self.built = true;
        self.network_dirty = true;
        let mut groups: BTreeMap<(u16, Medium), Vec<u32>> = BTreeMap::new();
        for (i, d) in self.docks.iter().enumerate() {
            groups
                .entry((d.frame, d.medium))
                .or_default()
                .push(i as u32);
        }
        let mut loops_all: Vec<Loop> = Vec::new();
        for ((frame, _), group) in groups {
            let topo = self.topo.clone();
            let cands = network::candidate_lanes(
                &self.docks,
                &group,
                &self.tun,
                mask,
                &mut self.lints,
                topo.of(frame),
            );
            let mut ids = Vec::new();
            for c in cands {
                ids.push(self.push_lane(c.a, c.b, c.path));
            }
            loops_all.extend(network::loops(
                &self.lanes,
                &ids,
                self.tun.loop_target_blocks,
            ));
        }
        for l in &loops_all {
            self.add_loop_services(l);
        }
        self.over_berths = timetable::assign_platforms(&mut self.services, &self.docks);
        for l in 0..self.lanes.len() as u32 {
            self.attach_lane(w, l, at);
        }
        for s in 0..self.services.len() as u32 {
            self.man_service(s, at);
        }
        for h in 0..self.havens.len() as u32 {
            self.refresh_haven_lanes(h);
        }
    }

    fn push_lane(&mut self, a: u32, b: u32, path: Polyline) -> u32 {
        let id = self.lanes.len() as u32;
        let d = &self.docks[a as usize];
        self.lanes.push(Lane {
            a,
            b,
            medium: d.medium,
            frame: d.frame,
            path,
            edge: u32::MAX,
        });
        self.lane_hops.push(Vec::new());
        self.index_lane(id);
        self.index_dock_lane(id);
        id
    }

    /// Adds lane `l` to its end docks' lists.
    pub(crate) fn index_dock_lane(&mut self, l: u32) {
        let n = self.docks.len();
        if self.dock_lanes.len() < n {
            self.dock_lanes.resize(n, Vec::new());
        }
        let ln = &self.lanes[l as usize];
        for d in [ln.a, ln.b] {
            self.dock_lanes[d as usize].push(l);
        }
    }

    pub(crate) fn index_lane(&mut self, l: u32) {
        let ln = &self.lanes[l as usize];
        let mut cells = BTreeSet::new();
        let t = self.topo.of(ln.frame);
        for w in ln.path.pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            // The segment's box in the lane's chart, folded onto canonical regions (TP8).
            let r = rtsim_space::topology::Rect::spanning(
                (i64::from(a.0), i64::from(a.1)),
                (i64::from(b.0), i64::from(b.1)),
                0,
            );
            for part in t.fold_rect(r) {
                let (x0, z0, x1, z1) = part.grid_cells(rtsim_travel::legs::REGION_BLOCKS);
                for rx in x0..=x1 {
                    for rz in z0..=z1 {
                        cells.insert((ln.frame, rx, rz));
                    }
                }
            }
        }
        if ln.path.pts.len() == 1 {
            let p = ln.path.pts[0];
            let (x, z) = t.canonical(i64::from(p.0), i64::from(p.1));
            let r = LegIndex::region_of(x, z);
            cells.insert((ln.frame, r.0, r.1));
        }
        for c in cells {
            self.lane_cells.entry(c).or_default().push(l);
        }
    }

    /// One service each way round a loop; a two-stop shuttle sails both directions already, so it
    /// gets one.
    fn add_loop_services(&mut self, l: &Loop) {
        let mut ways = vec![l.clone()];
        if l.stops.len() > 2 {
            ways.push(l.reversed());
        }
        for way in ways {
            let index = self.services.len();
            let s = timetable::service_of(
                &way,
                &self.lanes,
                &self.docks,
                &self.tun,
                ServiceKind::Merchant,
                index,
                |_| self.tun.dock_ticks,
            );
            self.push_service(s);
        }
    }

    fn push_service(&mut self, s: Service) -> u32 {
        let id = self.services.len() as u32;
        for (j, g) in s.legs.iter().enumerate() {
            self.lane_hops[g.hop.lane as usize].push((id, j as u32));
        }
        self.services.push(s);
        self.service_ships.push(ShipRange::default());
        id
    }

    /// The lane's world edge: travel time and dock wait from the services that sail it, capacity
    /// from their holds.
    fn attach_lane(&mut self, w: &mut World, l: u32, at: Tick) {
        if self.lanes[l as usize].edge != u32::MAX {
            return;
        }
        let q = w.cfg.trade.quantum;
        let ln = &self.lanes[l as usize];
        let mt = self.tun.medium(ln.medium);
        let mut sail = 0u64;
        // Per direction: the smallest gap between departures of one sequence, and the slots
        // carried per period of it.
        let mut gap = [u64::MAX; 2];
        let mut slot_milli_per_quantum = [0i64; 2];
        for &(s, j) in &self.lane_hops[l as usize] {
            let sv = &self.services[s as usize];
            let g = sv.legs[j as usize];
            let dir = usize::from(!g.hop.forward);
            sail = sail.max(g.sail());
            let n = u64::from(sv.ships.max(1));
            gap[dir] = gap[dir].min(sv.period.div_ceil(n));
            slot_milli_per_quantum[dir] +=
                (i128::from(sv.ships) * i128::from(mt.hold_slots) * 1000 * i128::from(q.len())
                    / i128::from(sv.period.max(1))) as i64;
        }
        let wait = gap
            .iter()
            .filter(|&&g| g != u64::MAX)
            .max()
            .copied()
            .unwrap_or(self.tun.headway_ticks);
        let cap = slot_milli_per_quantum
            .iter()
            .copied()
            .filter(|&c| c > 0)
            .min()
            .unwrap_or(1)
            .max(1);
        let spec = rtsim_world::LaneSpec {
            a: self.docks[ln.a as usize].node,
            b: self.docks[ln.b as usize].node,
            medium: ln.medium,
            length_blocks: ln.path.length(),
            d_quanta: q.covering(sail.max(1)),
            dock_quanta: q.covering(wait.max(1)),
            cap_slot_milli: cap,
        };
        let e = w.add_lane(spec, at);
        self.lanes[l as usize].edge = e;
    }

    /// Creates a service's ships, evenly spaced from a phase drawn from the service's index.
    fn man_service(&mut self, s: u32, at: Tick) {
        let sv = &self.services[s as usize];
        let n = sv.ships;
        let phase = StreamKey::new(self.seed, SYS_SERVICE, u64::from(s))
            .slot(0)
            .below(sv.period.max(1));
        let medium = sv.medium;
        let frame = sv.frame;
        let kind = sv.kind;
        let navy = sv.navy;
        let mt = self.tun.medium(medium);
        let crew = match kind {
            ServiceKind::Merchant => mt.merchant_crew,
            ServiceKind::Patrol => mt.navy_crew,
        };
        let first = self.ships.meta().1;
        for k in 0..n {
            let offset = sv.offset_of(phase, k, n);
            let seed = StreamKey::new(self.seed, SYS_SHIP, u64::from(s)).block(u64::from(k), 0)[0];
            let role = match navy {
                Some(nv) => Role::Navy { navy: nv },
                None => Role::Merchant,
            };
            self.ships.insert(Ship {
                role,
                medium,
                frame,
                seed,
                service: Some(s),
                slot: k,
                offset,
                crew,
                hull_milli: 1000,
                status: Status::Active,
                epoch: 0,
                embodied: false,
                cargo: BTreeMap::new(),
                quotes: BTreeMap::new(),
                promoted: None,
                plan: 0,
                pirate: None,
            });
        }
        let _ = at;
        debug_assert_eq!(
            self.ships.meta().1,
            first + n,
            "a service's ships are consecutive"
        );
        self.service_ships[s as usize] = ShipRange { first, n };
    }

    /// A port that joined after the network was built: a shuttle to its nearest dock of the same
    /// medium and frame that a lane can reach. Existing timetables never change (that would move
    /// their ships). Returns the new lane, if any.
    pub fn connect(
        &mut self,
        w: &mut World,
        dock: u32,
        mask: &dyn WaterMask,
        at: Tick,
    ) -> Option<u32> {
        self.network_dirty = true;
        let d = self.docks[dock as usize].clone();
        let mut others: Vec<(u64, u32)> = self
            .docks
            .iter()
            .enumerate()
            .filter(|(i, o)| *i as u32 != dock && o.medium == d.medium && o.frame == d.frame)
            .map(|(i, o)| (self.sdist(d.frame, o.xz(), d.xz()), i as u32))
            .collect();
        others.sort_unstable();
        for (_, o) in others.into_iter().take(4) {
            let mut lints = NetworkLints::default();
            let group = vec![dock.min(o), dock.max(o)];
            let topo = self.topo.clone();
            let c = network::candidate_lanes(
                &self.docks,
                &group,
                &self.tun,
                mask,
                &mut lints,
                topo.of(d.frame),
            );
            let Some(c) = c.into_iter().next() else {
                continue;
            };
            let ends = [c.a, c.b];
            let l = self.push_lane(c.a, c.b, c.path);
            let lp = Loop {
                stops: vec![c.a, c.b],
                hops: vec![
                    network::Hop {
                        lane: l,
                        forward: true,
                    },
                    network::Hop {
                        lane: l,
                        forward: false,
                    },
                ],
            };
            let first = self.services.len();
            self.add_loop_services(&lp);
            let over = timetable::assign_platforms(&mut self.services[first..], &self.docks);
            self.over_berths.extend(over);
            self.attach_lane(w, l, at);
            for s in first as u32..self.services.len() as u32 {
                self.man_service(s, at);
            }
            for h in 0..self.havens.len() as u32 {
                self.refresh_haven_lanes(h);
            }
            // A shuttle to a closed port is laid up from the start, and its lane closed.
            for d in ends {
                if self.port_closed(d) {
                    self.resettle(w, d, at);
                }
            }
            return Some(l);
        }
        None
    }

    /// A navy based at node `node` patrols the lanes of `medium` within the patrol hops of its
    /// dock (M2's recommendation: a patrol is a flow that lowers the lanes' hazard).
    pub fn add_navy(
        &mut self,
        w: &mut World,
        node: usize,
        medium: Medium,
        at: Tick,
    ) -> Option<u32> {
        let dock = self.dock_of_node(node, medium)?;
        let lanes: Vec<u32> = (0..self.lanes.len() as u32)
            .filter(|&l| self.lanes[l as usize].medium == medium)
            .collect();
        let walk = network::patrol_walk(&self.lanes, &lanes, dock, self.tun.patrol_hops);
        if walk.hops.is_empty() {
            return None;
        }
        self.network_dirty = true;
        let nv = self.navies.len() as u32;
        let index = self.services.len();
        let dock_ticks = self.tun.dock_ticks;
        let mut s = timetable::service_of(
            &walk,
            &self.lanes,
            &self.docks,
            &self.tun,
            ServiceKind::Patrol,
            index,
            |d| if d == dock { dock_ticks } else { 0 },
        );
        s.ships = self.tun.navy_ships.max(1);
        s.navy = Some(nv);
        let sid = self.push_service(s);
        let over = timetable::assign_platforms(
            &mut self.services[sid as usize..sid as usize + 1],
            &self.docks,
        );
        self.over_berths.extend(over);
        self.man_service(sid, at);
        let ships: Vec<ShipId> = self.service_ships[sid as usize].ids().collect();
        self.navies.push(Navy {
            node,
            dock,
            medium,
            service: sid,
            ships,
        });
        let touched: BTreeSet<u32> = self.services[sid as usize]
            .legs
            .iter()
            .map(|g| g.hop.lane)
            .collect();
        for &l in &touched {
            self.refresh_patrol(w, l, at);
        }
        // A patrol keeps a lane open that only laid-up merchants sailed (PC22).
        if !self.ports.is_empty() {
            for &l in &touched {
                self.settle_lane(w, l, at);
            }
        }
        self.note(
            at,
            "navy",
            None,
            format!(
                "a navy at node {node} patrols {} lanes with {} ships",
                self.services[sid as usize]
                    .legs
                    .iter()
                    .map(|g| g.hop.lane)
                    .collect::<BTreeSet<_>>()
                    .len(),
                self.tun.navy_ships
            ),
        );
        Some(nv)
    }

    /// Navy presence on lane `l`: for every patrol hop on it, the share of the period its active
    /// ships spend sailing it, times their strength.
    fn patrol_milli(&self, l: u32) -> i64 {
        let mut total = 0f64;
        for &(s, j) in &self.lane_hops[l as usize] {
            let sv = &self.services[s as usize];
            if sv.kind != ServiceKind::Patrol {
                continue;
            }
            let g = sv.legs[j as usize];
            let active: u32 = self.service_ships[s as usize]
                .ids()
                .filter_map(|id| self.ships.get(id))
                .filter(|x| x.is_active())
                .map(|x| x.crew)
                .sum();
            total += g.sail() as f64 / sv.period.max(1) as f64
                * f64::from(active)
                * self.tun.navy_strength;
        }
        (total * 1000.0).round() as i64
    }

    fn refresh_patrol(&mut self, w: &mut World, l: u32, at: Tick) {
        let p = self.patrol_milli(l);
        // Read first: a row lent mutably dirties its page.
        if self.threat(l).is_some_and(|t| t.patrol_milli == p) {
            return;
        }
        let t = self.threat_mut(l);
        if t.patrol_milli != p {
            t.patrol_milli = p;
            self.apply_hazard(w, l, at);
        }
    }

    /// Lane `l`'s threat row, if it has one.
    pub fn threat(&self, l: u32) -> Option<&LaneThreat> {
        self.threats.get(u64::from(l))
    }

    /// Lane `l`'s threat row to change, made empty if it has none.
    fn threat_mut(&mut self, l: u32) -> &mut LaneThreat {
        let k = u64::from(l);
        if !self.threats.contains(k) {
            self.threats.insert(k, LaneThreat::default());
        }
        self.threats.get_mut(k).expect("inserted")
    }

    /// The lanes pirates hunt now, in lane order: from the havens' pirates (a few per haven),
    /// never a pass over every lane or ship.
    pub fn hunted_lanes(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .havens
            .iter()
            .flat_map(|h| h.ships.iter())
            .filter_map(|&p| {
                let x = self.ships.get(p)?;
                match x.pirate.as_ref()?.state {
                    PState::Hunting { lane, .. } if x.is_active() => Some(lane),
                    _ => None,
                }
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Every lane's threat row, in lane order. A whole-table pass: tools and tests.
    pub fn lane_threats(&self) -> Vec<(u32, &LaneThreat)> {
        self.threats
            .keys()
            .into_iter()
            .filter_map(|k| Some((k as u32, self.threats.get(k)?)))
            .collect()
    }

    /// A player's escort on lane `l` (for a future plugin: players lowering hazard with their own
    /// ships), strength in crew-equivalents; 0 removes it.
    pub fn set_escort(&mut self, w: &mut World, l: u32, key: u64, strength: f64, at: Tick) {
        if l as usize >= self.lanes.len() {
            return;
        }
        let v = (strength * 1000.0).round() as i64;
        self.threat_mut(l).set_escort(key, v);
        self.apply_hazard(w, l, at);
    }

    // ---- hazards ---------------------------------------------------------------------------------

    fn strength(&self, s: ShipId) -> f64 {
        let Some(x) = self.ships.get(s) else {
            return 0.0;
        };
        let per = match x.role {
            Role::Pirate { .. } => self.tun.pirate_strength,
            Role::Merchant => self.tun.merchant_strength,
            Role::Navy { .. } => self.tun.navy_strength,
        };
        f64::from(x.crew) * per * f64::from(x.hull_milli.max(100)) / 1000.0
    }

    /// The share of each consignment the lane's hunters take, cut by its defence.
    pub fn lane_share(&self, l: u32) -> i64 {
        let Some(t) = self.threat(l) else {
            return 0;
        };
        let a: f64 = t.hunters.iter().map(|&h| self.strength(h)).sum();
        fight::hazard_milli(a, t.defence(), &self.tun)
    }

    /// What the share would be with no patrol or escort (for the CLI's comparison).
    pub fn lane_share_unprotected(&self, l: u32) -> i64 {
        let Some(t) = self.threat(l) else {
            return 0;
        };
        let a: f64 = t.hunters.iter().map(|&h| self.strength(h)).sum();
        fight::hazard_milli(a, 0.0, &self.tun)
    }

    /// Writes lane `l`'s share into the world: both directions sail their raided share to the
    /// hunting haven.
    fn apply_hazard(&mut self, w: &mut World, l: u32, at: Tick) {
        let share = self.lane_share(l);
        let Some(t) = self.threat(l) else {
            return;
        };
        let Some(h) = t.haven else {
            return;
        };
        if t.share == share {
            return;
        }
        let ln = &self.lanes[l as usize];
        let hv = &self.havens[h as usize];
        let mid = self.lane_at(ln, ln.path.length() / 2);
        let speed = self.tun.medium(ln.medium).speed;
        let q = w.cfg.trade.quantum;
        let home = (self.sdist(ln.frame, mid, (hv.dock.0, hv.dock.2)) as f64 / speed.max(1e-6))
            .ceil() as u64;
        let edge = ln.edge;
        let lag = w.edges[edge as usize].lag();
        let delay = lag + q.covering(home.max(1));
        let node = hv.node;
        let mut ok = true;
        for dir in 0..2u8 {
            ok &= w.set_lane_hazard(edge, dir, node, share, delay, at);
        }
        if ok {
            // The heat follows the applied share (PC20): brought up to now under the old share,
            // then relaxing toward the new one.
            let tau = self.heat_tau();
            let t = self.threats.get_mut(u64::from(l)).expect("checked");
            if tau > 0.0 {
                let at = at.max(t.heat_at);
                t.heat_milli = t.heat(at, tau);
                t.heat_at = at;
            }
            t.share = share;
            self.stats.hazard_changes += 1;
            // The haven's take of the lane, at the lane's middle; the lane has no key, so its
            // place is its name.
            self.tape.event(
                kinds::SHIPS_HAVEN,
                spot(at, ln.frame, mid.0, mid.1),
                nid(w, node),
                0,
                share,
            );
            self.note(
                at,
                "hazard",
                None,
                format!(
                    "lane {l} ({}): pirates of haven {h} now take {share}‰ (patrols and escorts {:.1})",
                    ln.medium.name(),
                    self.threat(l).expect("checked").defence()
                ),
            );
            // A lane hunted again: its trade ports are weighed (PC25).
            if share > 0 && tau > 0.0 {
                self.book_port_checks(w, l, at);
            }
        }
    }

    // ---- players and encounters ----------------------------------------------------------------

    /// The player's actor in the world's knowledge (idempotent).
    pub fn player(&mut self, w: &mut World, uuid: Uuid) -> ActorId {
        let id = w
            .knowledge
            .actors_mut()
            .register(ActorKind::Player, uuid_hash(uuid.0, uuid.1));
        self.players.entry(id).or_insert(PlayerView {
            uuid,
            interest: None,
        });
        id
    }

    fn uuid_of(&self, p: ActorId) -> Uuid {
        self.players.get(&p).map_or((0, 0), |v| v.uuid)
    }

    /// Where a player is (PLAYER, every 20 ticks). The interest area is kept while the player
    /// stays within a quarter of the view radius and it still covers the next window; otherwise a
    /// new one is booked for two windows, and the ships it can see are found.
    pub fn observe_player(
        &mut self,
        w: &mut World,
        uuid: Uuid,
        pos: rtsim_settle::Pos,
        at: Tick,
    ) -> ActorId {
        let p = self.player(w, uuid);
        let window = w.legs.window_ticks();
        let keep = self.players.get(&p).and_then(|v| v.interest).and_then(|s| {
            let row = w.legs.get(s.leg).filter(|r| r.generation == s.generation)?;
            let moved = self.topo(pos.frame).dist_sq(
                (i64::from(row.path.x0), i64::from(row.path.z0)),
                (i64::from(pos.x), i64::from(pos.z)),
            ) as i64;
            let quarter = i64::from(self.tun.view_radius / 4).pow(2);
            (row.path.frame == pos.frame && moved <= quarter && row.path.t1.0 >= at.0 + window)
                .then_some(s)
        });
        if keep.is_none() {
            let path = Path {
                frame: pos.frame,
                x0: pos.x,
                z0: pos.z,
                x1: pos.x,
                z1: pos.z,
                t0: at,
                t1: at + 2 * window,
                radius: self.tun.view_radius,
            };
            self.book_interest(w, p, path);
        }
        p
    }

    /// A stay the caller knows in one piece (a scripted player): one interest area over
    /// `[from, until]`.
    pub fn player_stays(
        &mut self,
        w: &mut World,
        uuid: Uuid,
        pos: rtsim_settle::Pos,
        from: Tick,
        until: Tick,
    ) -> ActorId {
        let p = self.player(w, uuid);
        let path = Path {
            frame: pos.frame,
            x0: pos.x,
            z0: pos.z,
            x1: pos.x,
            z1: pos.z,
            t0: from,
            t1: until.max(from),
            radius: self.tun.view_radius,
        };
        self.book_interest(w, p, path);
        p
    }

    /// The player left: its interest area goes, and every meeting booked against it.
    pub fn player_left(&mut self, w: &mut World, p: ActorId) {
        if let Some(v) = self.players.get_mut(&p)
            && let Some(s) = v.interest.take()
        {
            w.legs.cancel(s.leg);
        }
    }

    fn book_interest(&mut self, w: &mut World, p: ActorId, path: Path) {
        let old = self.players.get(&p).and_then(|v| v.interest);
        let stamp = match old.and_then(|s| w.legs.replan(s.leg, path)) {
            Some(s) => s,
            None => w.legs.book(path, Owner(PLAYER_OWNER | p.to_bits())),
        };
        if let Some(v) = self.players.get_mut(&p) {
            v.interest = Some(stamp);
        }
        self.book_voyages_in_view(w, path);
        let mut found = Vec::new();
        for other in w.legs.sharing(stamp) {
            let Some(row) = w.legs.get(other.leg) else {
                continue;
            };
            if !owns_leg(row.owner) {
                continue;
            }
            if let Some(m) = w.legs.meeting(other, stamp) {
                found.push(m);
            }
        }
        self.seen_ships(w, p, &found);
    }

    /// Books the voyages an interest area could see: lanes in the regions it covers, widened by
    /// the radius, and on them the voyages overlapping its window. Counts one interest booking.
    fn book_voyages_in_view(&mut self, w: &mut World, path: Path) {
        self.stats.interest_bookings += 1;
        let r = i64::from(path.radius) + i64::from(self.tun.body_radius);
        let bbox = rtsim_space::topology::Rect::spanning(
            (i64::from(path.x0), i64::from(path.z0)),
            (i64::from(path.x1), i64::from(path.z1)),
            r,
        );
        let mut lanes = BTreeSet::new();
        for part in self.topo(path.frame).fold_rect(bbox) {
            let (x0, z0, x1, z1) = part.grid_cells(rtsim_travel::legs::REGION_BLOCKS);
            for rx in x0..=x1 {
                for rz in z0..=z1 {
                    if let Some(v) = self.lane_cells.get(&(path.frame, rx, rz)) {
                        lanes.extend(v.iter().copied());
                    }
                }
            }
        }
        let mut want: Vec<(ShipId, u64)> = Vec::new();
        for l in lanes {
            for &(s, j) in &self.lane_hops[l as usize] {
                for id in self.service_ships[s as usize].ids() {
                    let Some(x) = self.ships.get(id) else {
                        continue;
                    };
                    if !x.is_active() {
                        continue;
                    }
                    let sv = &self.services[s as usize];
                    for v in sv.voyages_meeting(x.offset, j as usize, path.t0, path.t1) {
                        want.push((id, v));
                    }
                }
            }
        }
        want.sort_unstable();
        want.dedup();
        for (id, v) in want {
            self.book_voyage(w, id, v);
        }
    }

    /// Ship legs crossing a player's area (`a` the ship's leg): one meeting per ship voyage (or
    /// pirate plan), the earliest, each a `Seen` (or `SeenPirate`) event.
    fn seen_ships(&mut self, w: &World, p: ActorId, found: &[Meeting]) {
        let mut first: BTreeMap<(ShipId, u64), Meeting> = BTreeMap::new();
        let mut pirates: BTreeMap<ShipId, Meeting> = BTreeMap::new();
        for &m in found {
            let Some(row) = w.legs.get(m.a.leg) else {
                continue;
            };
            if !owns_leg(row.owner) {
                continue;
            }
            if let Some(&(id, v)) = self.leg_voyage.get(&m.a.leg) {
                let e = first.entry((id, v)).or_insert(m);
                if m.at < e.at {
                    *e = m;
                }
            } else {
                let id = ShipId::from_bits(row.owner.0 & MASK48);
                let e = pirates.entry(id).or_insert(m);
                if m.at < e.at {
                    *e = m;
                }
            }
        }
        for ((ship, voyage), meeting) in first {
            self.schedule(
                meeting.at,
                Ev::Seen {
                    ship,
                    voyage,
                    player: p,
                    meeting,
                },
            );
        }
        for (ship, meeting) in pirates {
            let Some(plan) = self.ships.get(ship).map(|x| x.plan) else {
                continue;
            };
            self.schedule(
                meeting.at,
                Ev::SeenPirate {
                    ship,
                    plan,
                    player: p,
                    meeting,
                },
            );
        }
    }

    /// A session started (docs/boundary.md §2.3: puppets never outlive a session): every ship
    /// the engine thought embodied is abstract again with the last state it knew, and every
    /// voyage may enter a player's view again (its body is gone). For the native link; the
    /// engine's own runs have no sessions.
    pub fn session_start(&mut self) {
        let was = std::mem::take(&mut self.embodied);
        for &id in &was {
            if let Some(x) = self.ships.get_mut(id) {
                x.embodied = false;
            }
        }
        self.seen_by.clear();
        // A ship kept sailing only because it was in view lays up now (§2.4.6a rule 7, PC23).
        let now = self.now;
        for id in was {
            self.lay_up_if_waiting(id, now);
        }
    }

    /// Marks a ship embodied or abstract, keeping the index of embodied ships.
    fn set_embodied(&mut self, id: ShipId, on: bool) {
        if let Some(x) = self.ships.get_mut(id) {
            x.embodied = on;
        }
        if on {
            self.embodied.insert(id);
        } else {
            self.embodied.remove(&id);
        }
    }

    /// Whether [`Ships::build`] has made the network (ports added later get shuttles).
    pub fn is_built(&self) -> bool {
        self.built
    }

    /// The legs of one voyage, booked in the shared index: its dock stay, then every straight
    /// piece between the path's corners and its phase boundaries, each at its own speed.
    fn voyage_paths(&self, id: ShipId, voyage: u64) -> Option<(Vec<Path>, VoyageTimes)> {
        let x = self.ships.get(id)?;
        let sv = &self.services[x.service? as usize];
        let j = (voyage % sv.legs.len() as u64) as usize;
        let g = sv.legs[j];
        let t = sv.times(x.offset, voyage);
        let ln = &self.lanes[g.hop.lane as usize];
        let len = ln.path.length();
        let path = if g.hop.forward {
            ln.path.clone()
        } else {
            ln.path.reversed()
        };
        let radius = self.tun.body_radius;
        let frame = ln.frame;
        let fd = &self.docks[g.from_dock as usize];
        let mut out = vec![Path {
            frame,
            x0: fd.x,
            z0: fd.z,
            x1: fd.x,
            z1: fd.z,
            t0: tick_of(t.docked),
            t1: tick_of(t.depart),
            radius,
        }];
        // Distance along the voyage to time.
        let (dep_b, app_b) = (g.depart_blocks, g.approach_blocks);
        let mid = len.saturating_sub(dep_b + app_b);
        let time_at = |s: u64| -> i64 {
            if s <= dep_b {
                t.depart + (g.depart as f64 * s as f64 / dep_b.max(1) as f64).round() as i64
            } else if s <= dep_b + mid {
                t.cruise + (g.cruise as f64 * (s - dep_b) as f64 / mid.max(1) as f64).round() as i64
            } else {
                t.approach
                    + (g.approach as f64 * (s - dep_b - mid) as f64 / app_b.max(1) as f64).round()
                        as i64
            }
        };
        let mut cuts: BTreeSet<u64> = path.cum.iter().copied().collect();
        cuts.insert(dep_b.min(len));
        cuts.insert((dep_b + mid).min(len));
        cuts.insert(len);
        let cuts: Vec<u64> = cuts.into_iter().collect();
        for w in cuts.windows(2) {
            let (s0, s1) = (w[0], w[1]);
            if s1 <= s0 {
                continue;
            }
            let (a, b) = (path.at(s0), path.at(s1));
            let (t0, t1) = (time_at(s0), time_at(s1).max(time_at(s0)));
            if t1 < 0 {
                continue;
            }
            out.push(self.chart_leg(frame, a, b, tick_of(t0), tick_of(t1), radius));
        }
        Some((out, t))
    }

    fn book_voyage(&mut self, w: &mut World, id: ShipId, voyage: u64) {
        if self.booked.contains_key(&(id, voyage)) {
            return;
        }
        let Some((paths, t)) = self.voyage_paths(id, voyage) else {
            return;
        };
        let owner = Owner(SHIP_OWNER | (id.to_bits() & MASK48));
        let mut legs = Vec::with_capacity(paths.len());
        for p in paths {
            let s = w.legs.book(p, owner);
            self.leg_voyage.insert(s.leg, (id, voyage));
            legs.push(s.leg);
            self.stats.legs_booked += 1;
        }
        self.stats.voyages_booked += 1;
        let end = tick_of(t.arrive);
        self.booked.insert((id, voyage), Booked { legs, end });
        self.schedule(end + 1, Ev::Unbook { ship: id, voyage });
    }

    fn unbook(&mut self, w: &mut World, id: ShipId, voyage: u64) {
        let gone: Vec<(ShipId, u64, ActorId)> = self
            .seen_by
            .range((id, voyage, ActorId::NULL)..=(id, voyage, ActorId::from_bits(u64::MAX)))
            .copied()
            .collect();
        for k in gone {
            self.seen_by.remove(&k);
        }
        if let Some(b) = self.booked.remove(&(id, voyage)) {
            for l in b.legs {
                self.leg_voyage.remove(&l);
                w.legs.cancel(l);
            }
        }
    }

    // ---- the step loop -------------------------------------------------------------------------

    /// Runs every ship event up to and including `to`, in time order, with the world caught up to
    /// each event's time first. Nothing is ticked. Ends at a step boundary: paged row tables are
    /// held to their budget there ([`Ships::enforce_budget`]).
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            w.run_until(at);
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        w.run_until(to);
        self.now = self.now.max(to);
        self.enforce_budget();
    }

    fn live(&mut self, s: ShipId, plan: u32) -> bool {
        let ok = self
            .ships
            .get(s)
            .is_some_and(|x| x.plan == plan && x.is_active());
        if !ok {
            self.stats.stale += 1;
        }
        ok
    }

    fn handle(&mut self, w: &mut World, infra: &mut dyn Infrastructure, at: Tick, ev: Ev) {
        match ev {
            Ev::Seen {
                ship,
                voyage,
                player,
                meeting,
            } => self.seen(w, ship, voyage, player, meeting, at),
            Ev::SeenPirate {
                ship,
                plan,
                player,
                meeting,
            } => {
                let ok = w.legs.is_meeting_live(&meeting)
                    && self
                        .ships
                        .get(ship)
                        .is_some_and(|x| x.plan == plan && x.is_active() && !x.embodied);
                if !ok {
                    self.stats.seen_stale += 1;
                    return;
                }
                self.stats.seen_pirates += 1;
                let leg = self.ships[ship].pirate.as_ref().and_then(|p| p.leg);
                let frame = self.ships[ship].frame;
                let pos = leg
                    .and_then(|s| w.legs.get(s.leg))
                    .map(|r| {
                        let p = r.path.at(at);
                        self.topo(frame).canonical(p.0, p.1)
                    })
                    .unwrap_or((0, 0));
                let viewer = self.viewer_pos(w, player);
                self.materialize_pirate(ship, (pos.0 as i32, pos.1 as i32), 0, player, viewer, at);
            }
            Ev::Unbook { ship, voyage } => self.unbook(w, ship, voyage),
            Ev::NextLeg { ship, epoch } => self.next_leg(w, ship, epoch, at),
            Ev::Arrive { ship, voyage } => self.arrive(w, ship, voyage, at),
            Ev::Raid {
                pirate,
                victim,
                voyage,
                plan,
                player,
            } => self.raid(w, pirate, victim, voyage, plan, player, at),
            Ev::Decide { ship, plan } => {
                if self.live(ship, plan) {
                    self.decide(w, ship, at);
                }
            }
            Ev::ReachLane { ship, plan } => {
                if self.live(ship, plan) {
                    self.reach_lane(w, infra, ship, at);
                }
            }
            Ev::Bored { ship, plan, reason } => {
                if self.live(ship, plan) {
                    self.leave_lane(w, ship, reason, at);
                }
            }
            Ev::Home { ship, plan } => {
                if self.live(ship, plan) {
                    self.home(w, ship, at);
                }
            }
            Ev::Respawn { ship } => self.respawn(w, ship, at),
            Ev::Refit { ship, plan } => self.refit(w, ship, plan, at),
            Ev::Refound { haven, epoch } => self.haven_look(w, haven, epoch, at),
            Ev::DockCheck { dock, epoch } => self.dock_check(w, dock, epoch, at),
        }
    }

    // ---- merchants and navies in view ----------------------------------------------------------

    /// The current voyage's lane direction, departure tick and hop.
    fn voyage_dir(&self, id: ShipId, voyage: u64) -> Option<(u32, u8, VoyageTimes, usize)> {
        let x = self.ships.get(id)?;
        let sv = &self.services[x.service? as usize];
        let j = (voyage % sv.legs.len() as u64) as usize;
        let g = sv.legs[j];
        let t = sv.times(x.offset, voyage);
        Some((g.hop.lane, u8::from(!g.hop.forward), t, j))
    }

    /// The consignment boundaries a voyage carries: after the previous departure on its lane
    /// direction (any ship of any service sailing it) up to its own, `(lo, hi)`.
    fn window(&self, w: &World, id: ShipId, voyage: u64) -> Option<(u32, u8, u64, u64)> {
        let (lane, dir, t, _) = self.voyage_dir(id, voyage)?;
        let q = w.cfg.trade.quantum;
        let mut prev: Option<i64> = None;
        for &(s, j) in &self.lane_hops[lane as usize] {
            let sv = &self.services[s as usize];
            let g = sv.legs[j as usize];
            if u8::from(!g.hop.forward) != dir {
                continue;
            }
            for o in self.service_ships[s as usize].ids() {
                let Some(ox) = self.ships.get(o) else {
                    continue;
                };
                if let Some((_, d)) = sv.departure_at_or_before(ox.offset, j as usize, t.depart - 1)
                {
                    prev = Some(prev.map_or(d, |p: i64| p.max(d)));
                }
            }
        }
        let edge = self.lanes[lane as usize].edge;
        let hi = (t.depart.max(0) as u64) / q.len();
        let lo = match prev {
            Some(p) => (p.max(0) as u64) / q.len() + 1,
            None => hi.saturating_sub(w.edges[edge as usize].dock),
        };
        Some((edge, dir, lo, hi))
    }

    /// What a voyage carries now: its promoted hold, or its window of the flow (what has left so
    /// far).
    pub fn cargo_of(&self, w: &World, id: ShipId, voyage: u64) -> BTreeMap<GoodId, Amount> {
        let Some(x) = self.ships.get(id) else {
            return BTreeMap::new();
        };
        if x.promoted.is_some_and(|p| p.voyage == voyage) || !matches!(x.role, Role::Merchant) {
            return x.cargo.clone();
        }
        // A laid-up service carries nothing new (PC23): its ship in view shows empty holds.
        if x.service.is_some_and(|s| self.service_laid_up(s)) {
            return BTreeMap::new();
        }
        let Some((edge, dir, lo, hi)) = self.window(w, id, voyage) else {
            return BTreeMap::new();
        };
        let b = w.cfg.trade.quantum.index(w.now());
        let hi = hi.min(b);
        let mut out = BTreeMap::new();
        if hi < lo {
            return out;
        }
        let d = &w.edges[edge as usize].dirs[usize::from(dir)];
        for (g, p) in d.pipelines() {
            let a = p.departed_between(lo, hi);
            if a > 0 {
                *out.entry(g).or_default() += a as Amount;
            }
        }
        out
    }

    fn items(cargo: &BTreeMap<GoodId, Amount>) -> Vec<Item> {
        cargo
            .iter()
            .filter(|(_, a)| **a >= MILLI)
            .map(|(g, a)| Item {
                item: item_of(*g),
                count: (*a / MILLI).min(i64::from(i32::MAX)) as i32,
            })
            .collect()
    }

    fn seen(
        &mut self,
        w: &mut World,
        id: ShipId,
        voyage: u64,
        player: ActorId,
        meeting: Meeting,
        at: Tick,
    ) {
        let ok = w.legs.is_meeting_live(&meeting)
            && self
                .ships
                .get(id)
                .is_some_and(|x| x.is_active() && !x.embodied)
            && !self.seen_by.contains(&(id, voyage, player));
        if !ok {
            self.stats.seen_stale += 1;
            return;
        }
        let Some(x) = self.ships.get(id) else {
            return;
        };
        let Some(s) = x.service else {
            return;
        };
        let fix = self.fix_of(s, x.offset, at);
        if fix.voyage != voyage {
            self.stats.seen_stale += 1;
            return;
        }
        self.stats.seen += 1;
        self.seen_by.insert((id, voyage, player));
        self.materialize_timetabled(w, id, fix, player, at);
        // A merchant seen on a hunted lane: its pirates come with the lane's hazard as the
        // chance (the same share the flow loses unobserved).
        if matches!(self.ships[id].role, Role::Merchant) && fix.phase != Phase::Dock {
            let lane = self.services[s as usize].legs[fix.leg].hop.lane;
            let share = self.threat(lane).map_or(0, |t| t.share);
            let hunter = self.threat(lane).and_then(|t| {
                t.hunters.iter().copied().max_by(|a, b| {
                    self.strength(*a)
                        .total_cmp(&self.strength(*b))
                        .then(b.cmp(a))
                })
            });
            if let Some(p) = hunter
                && share > 0
            {
                let roll = StreamKey::new(self.seed, SYS_RAID, self.ships[id].seed)
                    .slot(voyage)
                    .below(1000);
                if (roll as i64) < share {
                    let plan = self.ships[p].plan;
                    // The pirates strike while the ship is still in view: by the raid delay, or
                    // by the time it takes to sail half the view, whichever is sooner.
                    let speed = self.tun.medium(self.ships[id].medium).speed.max(1e-6);
                    let half_view = (f64::from(self.tun.view_radius) / speed / 2.0) as u64;
                    let delay = self.tun.raid_delay_ticks.min(half_view.max(1));
                    let when = (at + delay).min(tick_of(fix.times.arrive));
                    self.schedule(
                        when,
                        Ev::Raid {
                            pirate: p,
                            victim: id,
                            voyage,
                            plan,
                            player,
                        },
                    );
                }
            }
        }
    }

    fn materialize_timetabled(
        &mut self,
        w: &World,
        id: ShipId,
        fix: timetable::Fix,
        player: ActorId,
        at: Tick,
    ) {
        let x = &self.ships[id];
        let s = x.service.expect("timetabled");
        let sv = &self.services[s as usize];
        let g = sv.legs[fix.leg];
        let ln = &self.lanes[g.hop.lane as usize];
        // A ship body never sails a lane across an edge_stop seam (TP12); it appears at the image
        // of its position on the player's side (TP11), its waypoints moved there with it.
        let t = self.topo(x.frame);
        let viewer = self.viewer_pos(w, player);
        let here = (i64::from(fix.x), i64::from(fix.z));
        let image = if self.lane_crosses_stop(ln) {
            None
        } else {
            t.embodied_image(viewer.unwrap_or(here), here)
        };
        let Some((spawn, to_body)) = image else {
            self.stats.seen_stale += 1;
            self.note(
                at,
                "seen",
                Some(id),
                "stays abstract: its lane crosses an edge_stop seam, or it is across one".into(),
            );
            return;
        };
        let waypoints = self.body_waypoints(ln, g.hop.forward, to_body.compose(fix.chart));
        let mt = self.tun.medium(x.medium);
        let (role, body, crew_body) = match x.role {
            Role::Navy { .. } => (WireRole::Navy, mt.navy_body, mt.navy_crew_body),
            _ => (WireRole::Merchant, mt.merchant_body, mt.merchant_crew_body),
        };
        let cargo = self.cargo_of(w, id, fix.voyage);
        let mut members = vec![Member {
            body: body_id(&self.tun, body),
            flags: 0,
        }];
        for k in 0..x.crew {
            members.push(Member {
                body: body_id(&self.tun, crew_body),
                flags: u32::from(k == 0),
            });
        }
        let epoch = x.epoch + 1;
        let mut flags = boundary::MAT_CLAMP_ABOVE_SURFACE;
        if fix.phase == Phase::Dock {
            flags |= boundary::MAT_DOCKED;
        }
        let cmd = Cmd::Materialize(boundary::Materialize {
            agent: Ship::agent(id),
            epoch,
            body: body_id(&self.tun, body),
            dimension: u32::from(x.frame),
            flags,
            pos: (spawn.0 as f64, f64::from(fix.y), spawn.1 as f64),
            medium: x.medium,
            role,
            phase: fix.phase as u8,
            platform: g.platform,
            cruise_y: sv.cruise_y,
            target: 0,
            crew: x.crew,
            hull_milli: x.hull_milli,
            times: [
                fix.times.depart,
                fix.times.cruise,
                fix.times.approach,
                fix.times.arrive,
            ],
            harbour_blocks: g.depart_blocks as u32,
            waypoints,
            cargo: Self::items(&cargo),
            members,
        });
        self.outbox.push(cmd);
        let arrive = tick_of(fix.times.arrive);
        self.ships[id].epoch = epoch;
        self.set_embodied(id, true);
        self.stats.materialized += 1;
        let uuid = self.uuid_of(player);
        self.note(
            at,
            "seen",
            Some(id),
            format!(
                "enters the view of player {:x}: {:?} at ({}, {}, {}), {} items aboard",
                uuid.1,
                fix.phase,
                fix.x,
                fix.y,
                fix.z,
                cargo.values().sum::<Amount>() / MILLI
            ),
        );
        self.schedule(arrive.max(at + 1), Ev::NextLeg { ship: id, epoch });
    }

    /// The op id of the SHIP_CARGO that sets an embodiment's hold for a voyage: from what it is
    /// (the ship, the epoch, the voyage), so the same command derived again after a crash has the
    /// same id and no other command ever has it (docs/boundary.md §2.2; overnight choice U18).
    pub fn cargo_op(&self, id: ShipId, epoch: u32, voyage: u64) -> u64 {
        let seed = self.ships.get(id).map_or(id.to_bits(), |x| x.seed);
        let low = StreamKey::new(self.seed, SYS_CARGO, seed).block(u64::from(epoch), voyage)[0]
            & ((1 << 56) - 1);
        OP_ALLOCATOR | low.max(1)
    }

    /// An embodied timetabled ship reached its next dock: its next voyage's timetable goes to the
    /// plugin, and so on while it stays embodied (observed, so these events are allowed). A
    /// merchant's hold becomes the next voyage's (SHIP_CARGO, U18): what it carried was landed by
    /// the flow, and a body that kept it would hold goods the sim no longer has.
    fn next_leg(&mut self, w: &World, id: ShipId, epoch: u32, at: Tick) {
        let Some(x) = self.ships.get(id) else {
            return;
        };
        if !x.embodied || x.epoch != epoch || !x.is_active() {
            return;
        }
        let Some(s) = x.service else {
            return;
        };
        let sv = &self.services[s as usize];
        let fix = sv.fix_in(&self.topo, &self.lanes, &self.docks, x.offset, at);
        let g = sv.legs[fix.leg];
        let ln = &self.lanes[g.hop.lane as usize];
        if self.lane_crosses_stop(ln) {
            // Its next lane crosses an edge_stop seam: the body sails off and is captured before
            // it would meet the border; the voyage goes on abstract (TP12).
            let (pos, y) = ((fix.x, fix.z), self.cruise_y_of(id));
            self.outbox.push(Cmd::Depart(boundary::Depart {
                agent: Ship::agent(id),
                epoch,
                reason: 5,
                timeout_ticks: 600,
                pos: (f64::from(pos.0), f64::from(y), f64::from(pos.1)),
            }));
            self.note(
                at,
                "depart",
                Some(id),
                "its next lane crosses an edge_stop seam".into(),
            );
            return;
        }
        // In the lane's chart (from its first dock, canonical): the plugin steers each waypoint
        // by its image nearest the body (TP11).
        let waypoints = self.body_waypoints(ln, g.hop.forward, Iso::IDENTITY);
        let td = &self.docks[g.to_dock as usize];
        self.outbox.push(Cmd::Timetable(boundary::Timetable {
            agent: Ship::agent(id),
            epoch,
            flags: if sv.legs[(fix.leg + 1) % sv.legs.len()].dock > 0 {
                boundary::TT_DOCK_AT_END
            } else {
                0
            },
            times: [
                fix.times.depart,
                fix.times.cruise,
                fix.times.approach,
                fix.times.arrive,
            ],
            cruise_y: sv.cruise_y,
            dock_y: td.y,
            platform: g.platform,
            harbour_blocks: g.depart_blocks as u32,
            waypoints,
        }));
        if matches!(self.ships[id].role, Role::Merchant) {
            let cargo = self.cargo_of(w, id, fix.voyage);
            let op = self.cargo_op(id, epoch, fix.voyage);
            self.outbox.push(Cmd::SetCargo(boundary::SetCargo {
                agent: Ship::agent(id),
                epoch,
                op,
                items: Self::items(&cargo),
            }));
        }
        let next = tick_of(fix.times.arrive).max(at + 1);
        self.schedule(next, Ev::NextLeg { ship: id, epoch });
    }

    /// Makes a merchant's current voyage a row: its consignments leave the flow and it holds
    /// them until it docks. Idempotent per voyage. Returns false when it cannot (docked between
    /// voyages with nothing booked, or not a merchant).
    pub fn promote(&mut self, w: &mut World, id: ShipId, at: Tick) -> bool {
        self.promote_with(w, id, at, true)
    }

    fn promote_with(&mut self, w: &mut World, id: ShipId, at: Tick, may_raid: bool) -> bool {
        let Some(x) = self.ships.get(id) else {
            return false;
        };
        if !matches!(x.role, Role::Merchant) || !x.is_active() {
            return false;
        }
        let Some(s) = x.service else {
            return false;
        };
        let fix = self.fix_of(s, x.offset, at);
        if x.promoted.is_some_and(|p| p.voyage == fix.voyage) {
            return true;
        }
        // A laid-up service's ship still in view sails with empty holds (PC23).
        if self.service_laid_up(s) {
            return false;
        }
        let Some((edge, dir, lo, hi)) = self.window(w, id, fix.voyage) else {
            return false;
        };
        let b = w.cfg.trade.quantum.index(at);
        let hi = hi.min(b);
        let cut = if hi >= lo {
            w.cut_lane_window(edge, dir, lo, hi).unwrap_or_default()
        } else {
            Default::default()
        };
        let d = &w.edges[edge as usize].dirs[usize::from(dir)];
        let (from, to) = (d.from, d.to);
        let arrive = tick_of(fix.times.arrive).max(at);
        let x = &mut self.ships[id];
        for (g, a) in cut.goods {
            *x.cargo.entry(g).or_default() += a;
        }
        for (g, q) in cut.quotes {
            x.quotes.insert(g, q);
        }
        x.promoted = Some(Promoted {
            voyage: fix.voyage,
            edge,
            dir,
            from,
            to,
            arrive,
        });
        self.stats.promotions += 1;
        self.schedule(
            arrive,
            Ev::Arrive {
                ship: id,
                voyage: fix.voyage,
            },
        );
        if may_raid {
            self.maybe_raid_row(id, fix, at);
        }
        true
    }

    /// A merchant row (its cargo out of the flow) on a hunted lane meets the pirates with the
    /// lane's hazard as the chance, in the middle of its cruise: watched, the plugin plays the
    /// fight; not, Lanchester resolves it.
    fn maybe_raid_row(&mut self, id: ShipId, fix: timetable::Fix, at: Tick) {
        let Some(s) = self.ships[id].service else {
            return;
        };
        let lane = self.services[s as usize].legs[fix.leg].hop.lane;
        let Some(t) = self.threat(lane) else {
            return;
        };
        if t.share <= 0 {
            return;
        }
        let Some(p) = t.hunters.iter().copied().max_by(|a, b| {
            self.strength(*a)
                .total_cmp(&self.strength(*b))
                .then(b.cmp(a))
        }) else {
            return;
        };
        let roll = StreamKey::new(self.seed, SYS_RAID, self.ships[id].seed ^ 0x524F_5753)
            .slot(fix.voyage)
            .below(1000);
        if (roll as i64) >= t.share {
            return;
        }
        let when = tick_of((fix.times.cruise + fix.times.approach) / 2).max(at + 1);
        if when.0 as i64 >= fix.times.arrive {
            return;
        }
        let plan = self.ships[p].plan;
        let player = self.players.keys().next().copied().unwrap_or(ActorId::NULL);
        self.schedule(
            when,
            Ev::Raid {
                pirate: p,
                victim: id,
                voyage: fix.voyage,
                plan,
                player,
            },
        );
    }

    fn arrive(&mut self, w: &mut World, id: ShipId, voyage: u64, at: Tick) {
        let Some(x) = self.ships.get(id) else {
            return;
        };
        let Some(p) = x.promoted.filter(|p| p.voyage == voyage) else {
            return;
        };
        if !x.is_active() {
            return;
        }
        let cargo = x.cargo.clone();
        let quotes = x.quotes.clone();
        let stocked = w.land_held(p.to, Some(p.from), &cargo, &quotes, at);
        let x = &mut self.ships[id];
        for (g, a) in &stocked {
            if let Some(c) = x.cargo.get_mut(g) {
                *c -= a;
            }
            add(&mut self.ledger.landed_at_ports, *g, *a);
        }
        x.cargo.retain(|_, a| *a > 0);
        x.promoted = None;
        x.quotes.clear();
        // What a port could not stock (none for storehouses) is lost overboard.
        if !x.cargo.is_empty() {
            let left = std::mem::take(&mut x.cargo);
            for (g, a) in w.destroy_held(&left) {
                add(&mut self.ledger.sunk, g, a);
            }
        }
        self.stats.landings += 1;
        // Its cargo landed: out of view, a ship whose service is laid up lays up now (PC23).
        self.lay_up_if_waiting(id, at);
    }

    // ---- pirates -------------------------------------------------------------------------------

    fn temperament(&self, s: ShipId) -> Temperament {
        Temperament::from_seed(self.ships.get(s).map_or(0, |x| x.seed))
    }

    fn haven_of(&self, s: ShipId) -> Option<u32> {
        match self.ships.get(s)?.role {
            Role::Pirate { haven } => Some(haven),
            _ => None,
        }
    }

    fn bump(&mut self, s: ShipId) -> u32 {
        let x = &mut self.ships[s];
        x.plan = x.plan.wrapping_add(1);
        x.plan
    }

    fn pirate_pos(&self, w: &World, s: ShipId, at: Tick) -> (i32, i32) {
        let leg = self.ships[s].pirate.as_ref().and_then(|p| p.leg);
        if let Some(r) = leg.and_then(|l| w.legs.get(l.leg)) {
            let p = r.path.at(at);
            let (x, z) = self.topo(r.path.frame).canonical(p.0, p.1);
            return (x as i32, z as i32);
        }
        let h = self.haven_of(s).unwrap_or(0) as usize;
        let d = self.havens[h].dock;
        (d.0, d.2)
    }

    fn book_pirate_leg(&mut self, w: &mut World, s: ShipId, path: Path) {
        let old = self.ships[s].pirate.as_ref().and_then(|p| p.leg);
        let stamp = match old.and_then(|l| w.legs.replan(l.leg, path)) {
            Some(st) => st,
            None => w
                .legs
                .book(path, Owner(SHIP_OWNER | (s.to_bits() & MASK48))),
        };
        self.stats.legs_booked += 1;
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.leg = Some(stamp);
        }
        // Booked or replanned, it is a new leg of the archive; the old row stays as it was.
        self.tape.leg(
            kinds::SHIPS_PIRATE_LEG,
            u32::from(path.frame),
            (path.t0.0, path.x0, path.z0),
            (path.t1.0, path.x1, path.z1),
            sid(s),
        );
        // Players already watching where it goes see it come.
        let plan = self.ships[s].plan;
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
                    Ev::SeenPirate {
                        ship: s,
                        plan,
                        player,
                        meeting: m,
                    },
                );
            }
        }
    }

    fn sail_path(&self, s: ShipId, from: (i32, i32), to: (i32, i32), at: Tick) -> Path {
        let x = &self.ships[s];
        let speed = self.tun.medium(x.medium).speed;
        let ticks = ((self.sdist(x.frame, from, to) as f64 / speed.max(1e-6)).ceil() as u64).max(1);
        // The short way: the leg ends at the image of `to` nearest `from` (TP7).
        let to = {
            let t = self.topo(x.frame).toward(
                (i64::from(from.0), i64::from(from.1)),
                (i64::from(to.0), i64::from(to.1)),
            );
            (t.0 as i32, t.1 as i32)
        };
        Path {
            frame: x.frame,
            x0: from.0,
            z0: from.1,
            x1: to.0,
            z1: to.1,
            t0: at,
            t1: at + ticks,
            radius: self.tun.body_radius,
        }
    }

    /// Hold volume in thousandths of a slot.
    fn hold_slot_milli(w: &World, cargo: &BTreeMap<GoodId, Amount>) -> i64 {
        cargo
            .iter()
            .map(|(g, a)| *a / i64::from(w.goods.get(*g).stack.max(1)))
            .sum()
    }

    /// Lane flow volume now, thousandths of a slot per quantum, both directions.
    fn lane_volume(&self, w: &World, l: u32) -> i64 {
        let e = &w.edges[self.lanes[l as usize].edge as usize];
        e.dirs[0].volume_slot_milli(&w.goods) + e.dirs[1].volume_slot_milli(&w.goods)
    }

    fn decide(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let Some(h) = self.haven_of(s) else {
            return;
        };
        self.ensure_captain(w, s, at);
        self.refit_in_port(s, at);
        let tm = self.temperament(s);
        let a = self.strength(s);
        let medium = self.ships[s].medium;
        let hv = &self.havens[h as usize];
        let home = (hv.dock.0, hv.dock.2);
        let mut cands: Vec<(u32, f64)> = Vec::new();
        for &l in &hv.lanes {
            let Some((vol, _, surv)) = self.huntable(w, h, medium, a, l) else {
                continue;
            };
            let ln = &self.lanes[l as usize];
            // Bold captains mind patrols less: the survival share is raised to a power from 2
            // (timid) to 0 (reckless).
            let exp = 2.0 - f64::from(tm.bold) / 128.0;
            let mid = self.lane_at(ln, ln.path.length() / 2);
            let far = 1.0 + self.sdist(ln.frame, mid, home) as f64 / 2_000.0;
            let greed = f64::from(tm.bend(Trait::Aggressive, 1000, 50)) / 1000.0;
            let wgt = vol as f64 * surv.powf(exp.max(0.0)) * greed / far;
            if wgt > 0.0 {
                cands.push((l, wgt));
            }
        }
        let plan = self.bump(s);
        let total: f64 = cands.iter().map(|c| c.1).sum();
        if cands.is_empty() || total <= 0.0 {
            self.stats.fruitless += 1;
            // No prey while the haven starves: the ship is laid up (§2.4.6a rule 2, PC3). One
            // in a player's view rests on instead, so nothing vanishes before their eyes.
            if self.starving(h, at) && !self.ships[s].embodied {
                self.lay_up(w, s, at);
                return;
            }
            let until = at + self.tun.pirate_rest_ticks;
            if let Some(p) = self.ships[s].pirate.as_mut() {
                p.state = PState::Resting { until };
            }
            self.schedule(until, Ev::Decide { ship: s, plan });
            return;
        }
        let mut r = StreamKey::new(self.seed, SYS_PIRATE, self.ships[s].seed)
            .slot(u64::from(plan) + 1)
            .next_f64()
            * total;
        let mut pick = cands[cands.len() - 1].0;
        for &(l, wgt) in &cands {
            if r < wgt {
                pick = l;
                break;
            }
            r -= wgt;
        }
        let ln = &self.lanes[pick as usize];
        let mid = self.lane_at(ln, ln.path.length() / 2);
        let from = self.pirate_pos(w, s, at);
        let path = self.sail_path(s, from, mid, at);
        let arrive = path.t1;
        self.book_pirate_leg(w, s, path);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Outbound { lane: pick, arrive };
        }
        self.schedule(arrive, Ev::ReachLane { ship: s, plan });
        self.note(
            at,
            "sail",
            Some(s),
            format!("sails from haven {h} to hunt lane {pick}"),
        );
    }

    fn ensure_captain(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let Some(captain) = self.ships[s].pirate.as_ref().map(|p| p.captain) else {
            return;
        };
        if w.knowledge.notable(captain).is_some() {
            return;
        }
        // The old captain died of age; the crew follows a new one, who takes over what the haven
        // inherited (C24: heirs inherit grudges).
        self.stats.captains_died += 1;
        let h = self.haven_of(s).unwrap_or(0) as usize;
        let haven_actor = w.nodes[self.havens[h].node].actor;
        let seed = fnv1a64(&[self.ships[s].seed.to_le_bytes(), at.0.to_le_bytes()].concat());
        let c = w.knowledge.create_notable(
            seed,
            haven_actor,
            haven_actor,
            at,
            self.tun.captain_lifespan_ticks,
        );
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.captain = c;
        }
        self.note(
            at,
            "captain",
            Some(s),
            "a new captain takes the ship".into(),
        );
    }

    fn reach_lane(&mut self, w: &mut World, infra: &mut dyn Infrastructure, s: ShipId, at: Tick) {
        let Some(PState::Outbound { lane, .. }) = self.ships[s].pirate.as_ref().map(|p| p.state)
        else {
            return;
        };
        let h = self.haven_of(s).expect("a pirate");
        if self.tun.lifecycle {
            // §2.4.6a rule 1: the fight it judged the lane by costs men; a ship it costs the
            // last man is lost before the hunt begins (and so does not feed the haven).
            if !self.ships[s].embodied && self.pay_toll(w, s, h, lane, at) {
                return;
            }
            self.feed(h, at);
        }
        let tm = self.temperament(s);
        let defence = self.threat(lane).map_or(0.0, |t| t.defence());
        let surv = fight::square_law(self.strength(s), defence, &self.tun);
        // Bored after the hunt's length, bent by boldness, cut by damage and by patrols driving
        // them off (C24: threats get bored).
        let base = tm.bend(Trait::Bold, 1000, 50) as f64 / 1000.0;
        let hull = f64::from(self.ships[s].hull_milli) / 1000.0;
        let len =
            (self.tun.pirate_hunt_ticks as f64 * base * hull * surv.max(0.25)).max(1.0) as u64;
        let until = at + len;
        let plan = self.ships[s].plan;
        let t = self.threat_mut(lane);
        let held = t.haven;
        if t.haven
            .is_none_or(|o| o == h || (t.hunters.is_empty() && t.share == 0))
        {
            t.haven = Some(h);
        }
        let claimed = t.haven != held;
        t.add_hunter(s);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Hunting {
                lane,
                since: at,
                until,
            };
        }
        self.stats.hunts += 1;
        let ln = &self.lanes[lane as usize];
        let mid = self.lane_at(ln, ln.path.length() / 2);
        if claimed {
            // A haven takes the lane: from the haven that held it, or from none.
            let from = held.map_or(0, |o| nid(w, self.havens[o as usize].node));
            self.tape.event(
                kinds::SHIPS_HAVEN,
                spot(at, ln.frame, mid.0, mid.1),
                nid(w, self.havens[h as usize].node),
                from,
                0,
            );
        }
        let stay = Path {
            frame: ln.frame,
            x0: mid.0,
            z0: mid.1,
            x1: mid.0,
            z1: mid.1,
            t0: at,
            t1: until,
            radius: self.tun.body_radius,
        };
        self.book_pirate_leg(w, s, stay);
        self.apply_hazard(w, lane, at);
        self.schedule(
            until,
            Ev::Bored {
                ship: s,
                plan,
                reason: 1,
            },
        );
        self.note(
            at,
            "hunt",
            Some(s),
            format!(
                "hunts lane {lane} until day {:.2}; the lane loses {}‰",
                day_of(until),
                self.threat(lane).map_or(0, |t| t.share)
            ),
        );
        self.coast_raid(w, infra, s, lane, at, until);
    }

    /// A hunt may begin with a raid on a player district by the coast, or on a player's booked leg
    /// crossing the lane (C16 to C18), through the industry threat interface.
    fn coast_raid(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        s: ShipId,
        lane: u32,
        at: Tick,
        until: Tick,
    ) {
        let tm = self.temperament(s);
        let chance = tm.bend(Trait::Aggressive, self.tun.coast_raid_milli as i32, 50);
        let roll = StreamKey::new(self.seed, SYS_RAID, self.ships[s].seed)
            .slot(u64::from(self.ships[s].plan) << 32 | 0xC0A5)
            .below(1000);
        if (roll as i64) >= i64::from(chance) {
            return;
        }
        let ln = &self.lanes[lane as usize];
        let mid = self.lane_at(ln, ln.path.length() / 2);
        let frame = ln.frame;
        let x = &self.ships[s];
        let class = match x.medium {
            Medium::Sky => ThreatClass::Flying,
            _ => ThreatClass::GroundRanged,
        };
        let crew = x.crew;
        let strength = self.strength(s);
        let id = fnv1a64(&[x.seed.to_le_bytes(), at.0.to_le_bytes()].concat());
        let sites = infra.districts_near(
            frame,
            i64::from(mid.0),
            i64::from(mid.1),
            self.tun.coast_raid_blocks,
        );
        let mut regions: Vec<(i32, i32)> = Vec::new();
        for p in ln.path.pts.iter() {
            let r = LegIndex::region_of(i64::from(p.0), i64::from(p.1));
            let r = (r.0 as i32, r.1 as i32);
            if !regions.contains(&r) {
                regions.push(r);
            }
        }
        let (loot, what, object) = if let Some(site) = sites.iter().find(|d| !d.watched) {
            let t = Threat {
                id,
                source: ThreatSource::Bandit,
                class,
                count: crew,
                strength,
                target: ThreatTarget::District(site.id),
                loads: None,
                at,
            };
            let Some(o) = infra.raid(&t) else {
                return;
            };
            (
                o.stolen,
                format!("district {}", site.id.0),
                u128::from(site.id.0),
            )
        } else if let Some(tr) = infra
            .trains_crossed(frame, &regions, at, until)
            .first()
            .copied()
        {
            let t = Threat {
                id,
                source: ThreatSource::Bandit,
                class,
                count: crew,
                strength,
                target: ThreatTarget::Train(tr.train),
                loads: None,
                at,
            };
            let Some(l) = infra.raid_train(&t) else {
                return;
            };
            (
                l,
                format!("player leg {}", tr.train.0),
                u128::from(tr.train.0),
            )
        } else {
            return;
        };
        self.stats.coast_raids += 1;
        let mut got = 0i64;
        let mut took = 0i64;
        for (g, n) in loot.iter() {
            took += n;
            let name = infra
                .good_name(g)
                .unwrap_or_else(|| format!("good {}", g.0));
            match w.goods.find(&name) {
                Some(wg) => {
                    let a = n.saturating_mul(MILLI);
                    if let Some(p) = self.ships[s].pirate.as_mut() {
                        *p.district_loot.entry(wg).or_default() += a;
                    }
                    add(&mut self.ledger.from_districts, wg, a);
                    got += n;
                }
                None => {
                    *self.ledger.districts_unmapped.entry(name).or_default() += n;
                }
            }
        }
        // A raid that took nothing is minor. The place is the lane's middle, where the hunt is.
        let severity = if took > 0 {
            Severity::Notable
        } else {
            Severity::Minor
        };
        self.tape
            .event_as(
                kinds::SHIPS_RAID,
                severity,
                spot(at, frame, mid.0, mid.1),
                sid(s),
                object,
                took,
            )
            .text(|| format!("raids {what} from lane {lane}: {took} items"));
        self.note(
            at,
            "coast raid",
            Some(s),
            format!("raids {what} from lane {lane}: {got} items for the haven"),
        );
    }

    fn leave_lane(&mut self, w: &mut World, s: ShipId, reason: u32, at: Tick) {
        let Some(PState::Hunting { lane, .. }) = self.ships[s].pirate.as_ref().map(|p| p.state)
        else {
            return;
        };
        if let Some(t) = self.threats.get_mut(u64::from(lane)) {
            t.remove_hunter(s);
        }
        self.apply_hazard(w, lane, at);
        if reason == 1 {
            self.stats.bored += 1;
        }
        let h = self.haven_of(s).expect("a pirate");
        let dock = self.havens[h as usize].dock;
        let from = self.pirate_pos(w, s, at);
        let path = self.sail_path(s, from, (dock.0, dock.2), at);
        let arrive = path.t1;
        let plan = self.bump(s);
        self.book_pirate_leg(w, s, path);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Homebound { arrive };
        }
        if self.ships[s].embodied {
            let epoch = self.ships[s].epoch;
            self.outbox.push(Cmd::Depart(boundary::Depart {
                agent: Ship::agent(s),
                epoch,
                reason,
                timeout_ticks: 2_400,
                pos: (
                    f64::from(dock.0),
                    f64::from(self.cruise_y_of(s)),
                    f64::from(dock.2),
                ),
            }));
        }
        self.schedule(arrive, Ev::Home { ship: s, plan });
        let why = match reason {
            2 => "its hold is full",
            3 => "it is damaged",
            _ => "bored",
        };
        self.note(
            at,
            "leave",
            Some(s),
            format!("leaves lane {lane} ({why}) for haven {h}"),
        );
    }

    fn cruise_y_of(&self, s: ShipId) -> i32 {
        match self.ships[s].medium {
            Medium::Sky => self.tun.cruise_y[self.ships[s].slot as usize % self.tun.cruise_y.len()],
            _ => self.tun.sea_level,
        }
    }

    fn home(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let h = self.haven_of(s).expect("a pirate");
        let node = self.havens[h as usize].node;
        let cargo = std::mem::take(&mut self.ships[s].cargo);
        self.ships[s].quotes.clear();
        if !cargo.is_empty() {
            let stocked = w.land_held(node, None, &cargo, &BTreeMap::new(), at);
            let mut left = cargo.clone();
            for (g, a) in &stocked {
                add(&mut self.ledger.landed_at_havens, *g, *a);
                if let Some(v) = left.get_mut(g) {
                    *v -= a;
                }
            }
            left.retain(|_, a| *a > 0);
            self.ships[s].cargo = left;
            self.note(
                at,
                "landed",
                Some(s),
                format!(
                    "lands {} items of loot at haven {h}'s market",
                    stocked.values().sum::<Amount>() / MILLI
                ),
            );
        }
        let dl = self.ships[s]
            .pirate
            .as_mut()
            .map(|p| std::mem::take(&mut p.district_loot))
            .unwrap_or_default();
        if !dl.is_empty() {
            for (g, a) in w.stock_from_players(node, &dl, at) {
                add(&mut self.ledger.districts_landed, g, a);
            }
        }
        // Rest: the leg ends at the haven; nothing is booked while it rests.
        if let Some(l) = self.ships[s].pirate.as_mut().and_then(|p| p.leg.take()) {
            w.legs.cancel(l.leg);
        }
        let tm = self.temperament(s);
        let rest = (self.tun.pirate_rest_ticks as f64
            * f64::from(tm.bend(Trait::Sociable, 1000, 50))
            / 1000.0)
            .max(1.0) as u64;
        let until = at + rest;
        let plan = self.bump(s);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Resting { until };
        }
        self.schedule(until, Ev::Decide { ship: s, plan });
    }

    /// Where a player is, as the centre of its interest area in the shared leg index.
    fn viewer_pos(&self, w: &World, p: ActorId) -> Option<(i64, i64)> {
        let s = self.players.get(&p)?.interest?;
        let r = w.legs.get(s.leg)?;
        Some((i64::from(r.path.x0), i64::from(r.path.z0)))
    }

    #[allow(clippy::too_many_arguments)]
    fn materialize_pirate(
        &mut self,
        s: ShipId,
        pos: (i32, i32),
        target: u64,
        player: ActorId,
        viewer: Option<(i64, i64)>,
        at: Tick,
    ) {
        let x = &self.ships[s];
        // The image on the player's side (TP11); never across an edge_stop seam (TP12). The
        // player's position is its interest area's centre, kept in the shared leg index.
        let t = self.topo(x.frame);
        let _ = player;
        let here = (i64::from(pos.0), i64::from(pos.1));
        let Some((spawn, _)) = t.embodied_image(viewer.unwrap_or(here), here) else {
            self.stats.seen_stale += 1;
            return;
        };
        let pos = (spawn.0 as i32, spawn.1 as i32);
        let mt = self.tun.medium(x.medium);
        let mut members = vec![Member {
            body: body_id(&self.tun, mt.pirate_body),
            flags: 0,
        }];
        for k in 0..x.crew {
            members.push(Member {
                body: body_id(&self.tun, mt.pirate_crew_body),
                flags: u32::from(k == 0),
            });
        }
        let epoch = x.epoch + 1;
        let phase = match x.pirate.as_ref().map(|p| p.state) {
            Some(PState::Hunting { .. }) => 4,
            _ => 5,
        };
        let y = self.cruise_y_of(s);
        let mut flags = boundary::MAT_CLAMP_ABOVE_SURFACE;
        if target != 0 {
            flags |= boundary::MAT_HOSTILE;
        }
        self.outbox.push(Cmd::Materialize(boundary::Materialize {
            agent: Ship::agent(s),
            epoch,
            body: body_id(&self.tun, mt.pirate_body),
            dimension: u32::from(x.frame),
            flags,
            pos: (f64::from(pos.0), f64::from(y), f64::from(pos.1)),
            medium: x.medium,
            role: WireRole::Pirate,
            phase,
            platform: 0,
            cruise_y: y,
            target,
            crew: x.crew,
            hull_milli: x.hull_milli,
            times: [at.0 as i64; 4],
            harbour_blocks: 0,
            waypoints: vec![pos],
            cargo: Self::items(&x.cargo),
            members,
        }));
        self.ships[s].epoch = epoch;
        self.set_embodied(s, true);
        self.stats.materialized += 1;
        let uuid = self.uuid_of(player);
        self.note(
            at,
            "seen",
            Some(s),
            format!(
                "pirates in view of player {:x} at ({}, {}){}",
                uuid.1,
                pos.0,
                pos.1,
                if target != 0 { ", attacking" } else { "" }
            ),
        );
    }

    /// An individual raid on a merchant a player saw on a hunted lane.
    #[allow(clippy::too_many_arguments)]
    fn raid(
        &mut self,
        w: &mut World,
        p: ShipId,
        victim: ShipId,
        voyage: u64,
        plan: u32,
        player: ActorId,
        at: Tick,
    ) {
        let ok = self.ships.get(p).is_some_and(|x| {
            x.plan == plan
                && x.is_active()
                && matches!(
                    x.pirate.as_ref().map(|q| q.state),
                    Some(PState::Hunting { .. })
                )
        }) && self.ships.get(victim).is_some_and(|v| v.is_active());
        if !ok {
            self.stats.stale += 1;
            return;
        }
        let Some(vs) = self.ships[victim].service else {
            return;
        };
        let fix = self.fix_of(vs, self.ships[victim].offset, at);
        if fix.voyage != voyage || fix.phase == Phase::Dock {
            self.stats.stale += 1;
            return;
        }
        self.promote_with(w, victim, at, false);
        if self.ships[victim].embodied {
            // Watched: the pirates come into view beside it (or, already in view, turn on it),
            // and the plugin plays the fight.
            self.stats.raids_observed += 1;
            let pos = (fix.x + 48, fix.z + 48);
            if self.ships[p].embodied {
                let epoch = self.ships[p].epoch;
                self.outbox.push(Cmd::Steer(boundary::Steer {
                    agent: Ship::agent(p),
                    epoch,
                    flags: boundary::STEER_ATTACK,
                    speed: self.tun.medium(self.ships[p].medium).speed as f32,
                    pos: (f64::from(fix.x), f64::from(fix.y), f64::from(fix.z)),
                    target: Ship::agent(victim),
                }));
            } else {
                let viewer = self.viewer_pos(w, player);
                self.materialize_pirate(p, pos, Ship::agent(victim), player, viewer, at);
            }
            // The plugin plays the fight, so what was taken is not known yet: magnitude 0 is
            // "not yet", and it is a player's raid to watch, so it stays notable.
            self.tape
                .event(
                    kinds::SHIPS_RAID,
                    spot(at, self.ships[victim].frame, fix.x, fix.z),
                    sid(p),
                    sid(victim),
                    0,
                )
                .text(|| {
                    format!(
                        "attacks {} in view of a player",
                        Self::label(&self.ships[victim])
                    )
                });
            self.note(
                at,
                "raid",
                Some(p),
                format!("attacks {} in view", self.ship_name(victim)),
            );
        } else {
            self.abstract_fight(w, p, victim, at);
        }
    }

    /// A fight nobody watches: Lanchester's square law, never better for the merchant than the
    /// real fights observed.
    fn abstract_fight(&mut self, w: &mut World, p: ShipId, victim: ShipId, at: Tick) {
        self.stats.raids_abstract += 1;
        let a = self.strength(p);
        let mut d = self.strength(victim);
        if let Some(l) = self.current_lane(victim, at)
            && let Some(t) = self.threat(l)
        {
            d += t.escorts_milli() as f64 / 1000.0;
        }
        let out = fight::resolve(
            a,
            d,
            &self.observed[Pairing::PirateMerchant as usize],
            &self.tun,
        );
        self.after_fight(w, p, victim, out.survived, out.defender_survived, at);
        self.note(
            at,
            "fight",
            Some(p),
            format!(
                "fights {} unobserved: pirates keep {:.0}% (Lanchester {:.0}%{})",
                self.ship_name(victim),
                out.survived * 100.0,
                out.lanchester * 100.0,
                match out.floor {
                    Some(f) if f > out.lanchester => {
                        format!(
                            ", raised by a real fight where pirates kept {:.0}%",
                            f * 100.0
                        )
                    }
                    Some(f) => format!(", a real fight at {:.0}% does not bind", f * 100.0),
                    None => String::new(),
                }
            ),
        );
    }

    fn current_lane(&self, s: ShipId, at: Tick) -> Option<u32> {
        let x = self.ships.get(s)?;
        let sv = &self.services[x.service? as usize];
        let f = sv.fix(&self.lanes, &self.docks, x.offset, at);
        Some(sv.legs[f.leg].hop.lane)
    }

    /// The consequences of a fight: crews, hulls, loot, capture.
    fn after_fight(
        &mut self,
        w: &mut World,
        p: ShipId,
        victim: ShipId,
        survived: f64,
        defender_survived: f64,
        at: Tick,
    ) {
        let pc = self.ships[p].crew;
        let vc = self.ships[victim].crew;
        let pcrew = (f64::from(pc) * survived).ceil() as u32;
        let vcrew = (f64::from(vc) * defender_survived).ceil() as u32;
        self.ships[p].crew = pcrew;
        self.ships[p].hull_milli =
            (f64::from(self.ships[p].hull_milli) * (0.5 + 0.5 * survived)).round() as u32;
        self.ships[victim].crew = vcrew;
        if pcrew == 0 {
            self.tape_raid(w, p, victim, 0, false, at);
            self.lose_pirate(w, p, sid(victim), None, at);
            return;
        }
        // Loot: a share of each good, by how well they did.
        let share = (survived * self.tun.loot_share_milli as f64).round() as i64;
        let captured = survived >= self.tun.capture_at;
        let cargo = self.ships[victim].cargo.clone();
        let mut taken = 0i64;
        for (g, a) in cargo {
            let t = if captured {
                a
            } else {
                a * share.clamp(0, 1000) / 1000
            };
            if t <= 0 {
                continue;
            }
            *self.ships[victim].cargo.entry(g).or_default() -= t;
            *self.ships[p].cargo.entry(g).or_default() += t;
            add(&mut self.ledger.looted_by_pirates, g, t);
            taken += t;
        }
        self.ships[victim].cargo.retain(|_, a| *a > 0);
        self.tape_raid(w, p, victim, taken, captured, at);
        if taken > 0 {
            self.stats.loot_events += 1;
            self.robbery_report(w, p, victim, at);
        }
        if captured {
            let h = self.haven_of(p).expect("a pirate");
            // The beaten crew surrendered before the last man: some of them change sides (M2).
            let standing = vcrew.max((vc * self.tun.surrender_share_milli).div_ceil(1000));
            let joined = (standing * self.tun.crew_conversion_milli).div_ceil(1000);
            self.add_crew(h, joined, at);
            self.stats.captures += 1;
            self.lose_merchant(w, victim, sid(p), "captured by pirates", at);
            self.note(
                at,
                "capture",
                Some(p),
                format!(
                    "captures {}; {joined} of its crew join haven {h}",
                    self.ship_name(victim)
                ),
            );
        }
        self.check_hold(w, p, at);
    }

    /// Who did a thing the plugin reported, as the archive's id: the player's actor if the world
    /// knows them already (this only looks, it never registers), the ship's handle, else 0.
    fn doer(w: &World, by: By, player: Uuid, ship: u64) -> u128 {
        match by {
            By::Player => w
                .knowledge
                .actors()
                .find(ActorKind::Player, uuid_hash(player.0, player.1))
                .map_or(0, aid),
            By::Ship => Ship::id_of_agent(ship).map_or(0, sid),
            _ => 0,
        }
    }

    /// A raid nobody watched, for the archive: pirate `p` on merchant `victim`, `taken` in the
    /// hold's milli-items. One that took nothing is minor. The place is the victim's, where the
    /// fight was.
    fn tape_raid(
        &mut self,
        w: &World,
        p: ShipId,
        victim: ShipId,
        taken: i64,
        captured: bool,
        at: Tick,
    ) {
        let pos = self.position(w, victim, at);
        let items = (taken + MILLI - 1) / MILLI;
        let severity = if taken > 0 {
            Severity::Notable
        } else {
            Severity::Minor
        };
        self.tape
            .event_as(
                kinds::SHIPS_RAID,
                severity,
                spot(at, self.ships[victim].frame, pos.0, pos.1),
                sid(p),
                sid(victim),
                items,
            )
            .text(|| {
                format!(
                    "raids {}: {items} items{}",
                    Self::label(&self.ships[victim]),
                    if captured { ", and takes the ship" } else { "" }
                )
            });
    }

    fn check_hold(&mut self, w: &World, p: ShipId, at: Tick) {
        if !self.ships[p].is_active() {
            return;
        }
        let full = Self::hold_slot_milli(w, &self.ships[p].cargo)
            >= i64::from(self.tun.pirate_hold_slots) * 1000;
        let hull = self.ships[p].hull_milli < 400;
        if full || hull {
            self.leave_now(p, if full { 2 } else { 3 }, at);
        }
    }

    /// Sends a hunting pirate home at `at` (C24). One already homebound, resting or outbound
    /// keeps its plan: a late report (a fight resolved in view after the hunt ran out) must not
    /// make its next event stale, or it would idle forever with nothing booked.
    fn leave_now(&mut self, s: ShipId, reason: u32, at: Tick) {
        let hunting = self.ships[s]
            .pirate
            .as_ref()
            .is_some_and(|p| matches!(p.state, PState::Hunting { .. }));
        if hunting {
            let plan = self.bump(s);
            self.schedule(
                at,
                Ev::Bored {
                    ship: s,
                    plan,
                    reason,
                },
            );
        }
    }

    fn robbery_report(&mut self, w: &mut World, p: ShipId, victim: ShipId, at: Tick) {
        let Some(captain) = self.ships[p].pirate.as_ref().map(|q| q.captain) else {
            return;
        };
        let Some(home) = self.home_node(victim) else {
            return;
        };
        let pos = self.position(w, victim, at);
        let place = Place::new(self.ships[victim].frame, pos.0, 64, pos.1);
        let whom = w.nodes[home].actor;
        let id = w.knowledge.record(Report {
            who: captain,
            whom,
            kind: ReportKind::Robbery,
            severity: SEVERITY_UNIT,
            flags: 0,
            at,
            place,
        });
        w.knowledge.remember(captain, id, 64);
        // The ship's crew tell it at the next port; settlements in sight saw it.
        let near: Vec<(ActorId, Place)> = (0..w.nodes.len())
            .map(|i| {
                let q = w.nodes[i].pos;
                (w.nodes[i].actor, Place::new(q.frame, q.x, 64, q.z))
            })
            .collect();
        let near = w.near_images(place, near);
        let mut seen = witnesses(place, w.cfg.knowledge.witness_radius, near);
        if let Some(to) = self.ships[victim].promoted.map(|q| q.to) {
            seen.push(w.nodes[to].actor);
        }
        seen.sort();
        seen.dedup();
        for h in seen {
            w.knowledge.witness(h, id, at);
        }
    }

    /// Where a ship is now.
    pub fn position(&self, w: &World, s: ShipId, at: Tick) -> (i32, i32) {
        let Some(x) = self.ships.get(s) else {
            return (0, 0);
        };
        match x.service {
            Some(sv) => {
                let f = self.fix_of(sv, x.offset, at);
                (f.x, f.z)
            }
            None => self.pirate_pos(w, s, at),
        }
    }

    /// A merchant or navy ship's home node: its service's first stop.
    fn home_node(&self, s: ShipId) -> Option<usize> {
        let x = self.ships.get(s)?;
        match x.role {
            Role::Pirate { haven } => Some(self.havens[haven as usize].node),
            _ => {
                let sv = &self.services[x.service? as usize];
                Some(self.docks[sv.legs[0].from_dock as usize].node)
            }
        }
    }

    // ---- losses and returns --------------------------------------------------------------------

    /// A merchant or navy ship is lost: its hold goes down with it unless taken first, and it
    /// relaunches at its home dock after the delay, in its own slot.
    ///
    /// `by` is the archive id of what did it (0 for none) and `how` the word for the payload.
    fn lose_merchant(&mut self, w: &mut World, s: ShipId, by: u128, how: &'static str, at: Tick) {
        let cargo = std::mem::take(&mut self.ships[s].cargo);
        let mut gone = 0i64;
        for (g, a) in w.destroy_held(&cargo) {
            gone += a;
            add(&mut self.ledger.sunk, g, a);
        }
        let pos = self.position(w, s, at);
        self.tape
            .event(
                kinds::SHIPS_LOST,
                spot(at, self.ships[s].frame, pos.0, pos.1),
                sid(s),
                by,
                gone / MILLI,
            )
            .text(|| format!("{} is {how}", Self::label(&self.ships[s])));
        let back = self.relaunch_time(s, at + self.tun.respawn_ticks);
        let x = &mut self.ships[s];
        x.promoted = None;
        x.quotes.clear();
        x.status = Status::Lost { since: at, back };
        self.set_embodied(s, false);
        self.schedule(back, Ev::Respawn { ship: s });
        if let Role::Navy { .. } = self.ships[s].role {
            let sv = self.ships[s].service.expect("timetabled");
            let lanes: BTreeSet<u32> = self.services[sv as usize]
                .legs
                .iter()
                .map(|g| g.hop.lane)
                .collect();
            for l in lanes {
                self.refresh_patrol(w, l, at);
            }
        }
    }

    /// The first time at or after `t` that ship `s`'s slot is docked at its service's home dock.
    fn relaunch_time(&self, s: ShipId, t: Tick) -> Tick {
        let x = &self.ships[s];
        let Some(sv) = x.service else {
            return t;
        };
        let sv = &self.services[sv as usize];
        let p = i128::from(sv.period);
        let base = i128::from(sv.legs[0].start) - i128::from(x.offset);
        let m = (i128::from(t.0) - base).div_euclid(p) + 1;
        Tick((m * p + base).max(i128::from(t.0)) as u64)
    }

    fn respawn(&mut self, w: &World, s: ShipId, at: Tick) {
        let Some(x) = self.ships.get(s) else {
            return;
        };
        if !matches!(x.status, Status::Lost { back, .. } if back <= at) {
            return;
        }
        // A merchant due back while its service is laid up stays in port (§2.4.6a rule 7).
        if matches!(x.role, Role::Merchant) && x.service.is_some_and(|v| self.service_laid_up(v)) {
            self.lay_up_merchant(s, at);
            return;
        }
        let mt = self.tun.medium(x.medium);
        let crew = match x.role {
            Role::Navy { .. } => mt.navy_crew,
            _ => mt.merchant_crew,
        };
        let x = &mut self.ships[s];
        x.status = Status::Active;
        x.crew = crew;
        x.hull_milli = 1000;
        self.stats.respawns += 1;
        // Back at its home dock, which is where its timetable has it at `back`.
        let pos = self.position(w, s, at);
        let home = self.home_node(s).map_or(0, |n| nid(w, n));
        self.tape.event(
            kinds::SHIPS_BACK,
            spot(at, self.ships[s].frame, pos.0, pos.1),
            sid(s),
            home,
            i64::from(crew),
        );
        self.note(
            at,
            "relaunch",
            Some(s),
            "a new ship takes the slot at its home dock".into(),
        );
    }

    /// A pirate ship the plugin reports with no crew left (a fight it played, wounds, or a body
    /// leaving view) is lost, as an abstract fight that kills the last man loses it
    /// ([`Self::after_fight`]), and its haven rebuilds it when its pool can man one (T27). Kept
    /// active, a crewless pirate had no strength, so no lane was ever worth hunting again, and it
    /// was never rebuilt. Whether it was lost.
    fn lose_if_crewless(
        &mut self,
        w: &mut World,
        s: ShipId,
        by: u128,
        killer: Option<ActorId>,
        at: Tick,
    ) -> bool {
        let crewless = self
            .ships
            .get(s)
            .is_some_and(|x| x.is_active() && x.crew == 0 && matches!(x.role, Role::Pirate { .. }));
        if crewless {
            self.lose_pirate(w, s, by, killer, at);
        }
        crewless
    }

    /// A pirate ship is sunk, by `by` (an archive id, 0 for none; `killer` is the player's actor
    /// when a player did it, for the books of the knowledge).
    fn lose_pirate(
        &mut self,
        w: &mut World,
        s: ShipId,
        by: u128,
        killer: Option<ActorId>,
        at: Tick,
    ) {
        let Some(h) = self.haven_of(s) else {
            return;
        };
        // Where it went down, before its leg is cancelled.
        let sank_at = self.pirate_pos(w, s, at);
        if let Some(PState::Hunting { lane, .. }) = self.ships[s].pirate.as_ref().map(|p| p.state) {
            if let Some(t) = self.threats.get_mut(u64::from(lane)) {
                t.remove_hunter(s);
            }
            self.apply_hazard(w, lane, at);
        }
        let cargo = std::mem::take(&mut self.ships[s].cargo);
        let mut gone = 0i64;
        for (g, a) in w.destroy_held(&cargo) {
            gone += a;
            add(&mut self.ledger.sunk, g, a);
        }
        // District loot never entered the world's books: it goes down unrecorded there, and the
        // ledger notes it as lost.
        if let Some(p) = self.ships[s].pirate.as_mut() {
            for (g, a) in std::mem::take(&mut p.district_loot) {
                gone += a;
                add(&mut self.ledger.districts_lost, g, a);
            }
        }
        if let Some(l) = self.ships[s].pirate.as_mut().and_then(|p| p.leg.take()) {
            w.legs.cancel(l.leg);
        }
        let captain = self.ships[s].pirate.as_ref().map(|p| p.captain);
        if let (Some(c), Some(k)) = (captain, killer) {
            let pos = self.pirate_pos(w, s, at);
            let place = Place::new(self.ships[s].frame, pos.0, 64, pos.1);
            let haven_actor = w.nodes[self.havens[h as usize].node].actor;
            let id = w.knowledge.record(Report {
                who: k,
                whom: haven_actor,
                kind: ReportKind::Killing,
                severity: SEVERITY_UNIT,
                flags: rtsim_knowledge::FLAG_PLAYER,
                at,
                place,
            });
            w.knowledge.witness(c, id, at);
            w.knowledge.remember(c, id, 255);
        }
        if let Some(c) = captain
            && w.knowledge.notable(c).is_some()
        {
            // Its grudges pass to the haven (C24: heirs inherit grudges).
            w.knowledge.die(c, at);
        }
        // The slot's refit (§2.4.6): a ship a player sank or took presses the haven by 1/slots,
        // and the wait read now is `min(cap, base x 2^P)` days. Losses with no player add nothing.
        let rule = self.tun.refit;
        let day = day_num(at);
        let hv = &mut self.havens[h as usize];
        if killer.is_some() {
            let slots = hv.ships.len().max(1) as f32;
            hv.pressure.bump(day, 1.0 / slots, rule.half_life_days);
        }
        let back = at + rule.delay_ticks(hv.pressure, day).max(1);
        let x = &mut self.ships[s];
        x.status = Status::Lost { since: at, back };
        x.plan = x.plan.wrapping_add(1);
        let plan = x.plan;
        self.set_embodied(s, false);
        self.stats.sinkings += 1;
        self.tape
            .event(
                kinds::SHIPS_LOST,
                spot(at, self.ships[s].frame, sank_at.0, sank_at.1),
                sid(s),
                by,
                gone / MILLI,
            )
            .text(|| {
                format!(
                    "{} is sunk; its haven will refit the slot",
                    Self::label(&self.ships[s])
                )
            });
        self.schedule(back, Ev::Refit { ship: s, plan });
        self.note(
            at,
            "sunk",
            Some(s),
            format!(
                "is sunk; haven {h} refits the slot from day {:.2}",
                day_of(back)
            ),
        );
        // Its haven's last ship gone: it shuts if it starves (§2.4.6a rule 4), and a fed one
        // waits on its refits, fed until a month after the first is due (PC13).
        self.wait_on_refits(h, at);
        self.haven_quiet(h, at);
    }

    /// A fed haven whose ships were all sunk is not shut: S8's refits answer the loss (§2.4.6a
    /// rule 4, PC4). It counts as fed until `starve_days` after its first slot's refit is due
    /// (PC13), so the band waits out S8's `min(96, 4 x 2^P)` days hiring, and a month of hires
    /// mans a slot even from an empty pool (a crew of 6 takes 29.2 days from none). Pirates
    /// that never sail again after that still starve it.
    fn wait_on_refits(&mut self, h: u32, at: Tick) {
        let hv = &self.havens[h as usize];
        if !self.tun.lifecycle || hv.shut.is_some() || self.starving(h, at) {
            return;
        }
        let mut first = None::<Tick>;
        for &s in &hv.ships {
            match self.ships.get(s).map(|x| x.status) {
                Some(Status::Active) | None => return,
                Some(Status::Lost { back, .. }) if back != LAID_UP => {
                    first = Some(first.map_or(back, |f| f.min(back)));
                }
                Some(Status::Lost { .. }) => {}
            }
        }
        let Some(first) = first else {
            return;
        };
        let until = first.max(at) + self.starve_ticks();
        self.settle_pool(h, at);
        let hv = &mut self.havens[h as usize];
        hv.fed_until = hv.fed_until.max(until);
    }

    /// A lost pirate ship's slot is refitted when its wait is over and the haven's pool can man
    /// it (§2.4.6): the lost row comes back with a new captain. Short of crew, it waits for the
    /// moment hires make up the crew (one event), or less if a capture fills the pool first. A
    /// haven that starves first stops hiring (§2.4.6a rule 2), so the slot looks again at the
    /// turn instead, when a hunt begun since may have fed it on; a shut haven refits nothing
    /// (rule 4): its slots wait for a refound (rule 5).
    fn refit(&mut self, w: &mut World, s: ShipId, plan: u32, at: Tick) {
        let Some(x) = self.ships.get(s) else {
            return;
        };
        if x.plan != plan || !matches!(x.status, Status::Lost { back, .. } if back <= at) {
            return;
        }
        let Some(h) = self.haven_of(s) else {
            return;
        };
        if self.havens[h as usize].shut.is_some() {
            return;
        }
        let need = self.tun.medium(x.medium).pirate_crew;
        let pool = self.crew_pool_milli(h, at);
        if pool < need * 1000 {
            // Never, if hires cannot reach a crew: a capture wakes it, and so does a hunt start
            // that feeds a starving haven again (PC11).
            let ready = self.crew_ready_at(h, need, at).map(|t| t.max(at + 1));
            let turn = (self.tun.lifecycle && !self.starving(h, at)).then(|| self.fed_until(h));
            if let Some(t) = ready.or(turn) {
                let plan = self.bump(s);
                self.schedule(t, Ev::Refit { ship: s, plan });
            }
            if self.log_enabled {
                self.note(
                    at,
                    "refit",
                    Some(s),
                    format!(
                        "haven {h} waits for crew: {:.1} ashore of {need}{}",
                        f64::from(pool) / 1000.0,
                        ready.map_or(String::new(), |t| format!(
                            ", hired by day {:.2}",
                            day_of(t)
                        ))
                    ),
                );
            }
            return;
        }
        self.take_crew(h, need, at);
        self.stats.rebuilds += 1;
        self.launch(w, s, h, at);
        self.note(
            at,
            "rebuilt",
            Some(s),
            format!("haven {h} launches a new ship"),
        );
    }

    /// Launches a new ship in pirate slot `s` of haven `h`, reusing the lost row: a new captain,
    /// a full crew, a sound hull, resting before it decides (§2.4.6). The caller has found the
    /// crew: from the pool for a refit, the new band itself for a refound (§2.4.6a rule 5).
    fn launch(&mut self, w: &mut World, s: ShipId, h: u32, at: Tick) {
        let need = self.tun.medium(self.ships[s].medium).pirate_crew;
        let node = self.havens[h as usize].node;
        let haven_actor = w.nodes[node].actor;
        let seed = fnv1a64(&[self.ships[s].seed.to_le_bytes(), at.0.to_le_bytes()].concat());
        let captain = w.knowledge.create_notable(
            seed,
            haven_actor,
            haven_actor,
            at,
            self.tun.captain_lifespan_ticks,
        );
        let plan = self.bump(s);
        let x = &mut self.ships[s];
        x.status = Status::Active;
        x.crew = need;
        x.hull_milli = 1000;
        x.pirate = Some(Pirate {
            captain,
            state: PState::Resting {
                until: at + self.tun.pirate_rest_ticks,
            },
            leg: None,
            district_loot: BTreeMap::new(),
        });
        self.schedule(
            at + self.tun.pirate_rest_ticks,
            Ev::Decide { ship: s, plan },
        );
        // Back at its haven's dock, a new ship in the lost one's row, with a new captain.
        let dock = self.havens[h as usize].dock;
        self.tape.event(
            kinds::SHIPS_BACK,
            spot(at, self.havens[h as usize].frame, dock.0, dock.2),
            sid(s),
            nid(w, node),
            i64::from(need),
        );
    }

    // ---- havens that rise and fall (EQUILIBRIUM.md §2.4.6a, slice S8b) -------------------------

    /// The toll of a hunt (§2.4.6a rule 1, PC1): pirate `s`, unembodied, reaching lane `lane`
    /// pays for the fight it judged the lane by. Its crew `c` of strength `A` against a merchant
    /// crew and the lane's defence `D` loses `c x (1 - square_law(A, D))` men, in thousandths:
    /// the square law's share, never a roll. Haven `h`'s pool pays first (the men ashore who
    /// make up the losses), as far as it goes beyond its empty slots' crews, as a top-up does
    /// (PC12); what it lacks comes off the crew in whole men, rounded up. A crew paid to the last
    /// man loses its ship, as a fight would (no killer, so no pressure). Whether the ship was
    /// lost.
    fn pay_toll(&mut self, w: &mut World, s: ShipId, h: u32, lane: u32, at: Tick) -> bool {
        let a = self.strength(s);
        let mt = self.tun.medium(self.ships[s].medium);
        let d = f64::from(mt.merchant_crew) * self.tun.merchant_strength
            + self.threat(lane).map_or(0.0, |t| t.defence());
        let crew = self.ships[s].crew;
        let lost = 1.0 - fight::square_law(a, d, &self.tun);
        let toll = (f64::from(crew) * 1000.0 * lost).round() as u32;
        if toll == 0 {
            return false;
        }
        self.settle_pool(h, at);
        let due = self.due_crews(h).saturating_mul(1000);
        let hv = &mut self.havens[h as usize];
        let from_pool = toll.min(hv.pool_milli.saturating_sub(due));
        hv.pool_milli -= from_pool;
        hv.counts.toll_milli += u64::from(toll);
        let men = (toll - from_pool).div_ceil(1000);
        self.ships[s].crew = crew.saturating_sub(men);
        if self.log_enabled {
            self.note(
                at,
                "toll",
                Some(s),
                format!(
                    "the hunt on lane {lane} costs {:.2} men: {:.2} from haven {h}'s pool, \
                     {men} from the crew",
                    f64::from(toll) / 1000.0,
                    f64::from(from_pool) / 1000.0
                ),
            );
        }
        men > 0 && self.lose_if_crewless(w, s, 0, None, at)
    }

    /// A hunt begins from haven `h` (§2.4.6a rule 2, PC2): the haven is fed for `starve_days`
    /// more. The pool is settled first, so the closed form keeps what it read before the turn
    /// moved. A haven fed again after starving wakes its waiting slots (PC11).
    fn feed(&mut self, h: u32, at: Tick) {
        let was_starving = self.starving(h, at);
        self.settle_pool(h, at);
        let until = at + self.starve_ticks();
        let hv = &mut self.havens[h as usize];
        hv.fed_until = hv.fed_until.max(until);
        if was_starving {
            self.wake_slots(h, at);
        }
    }

    /// A starving haven fed again (PC11): every slot that waits on crew (its refit due) or lies
    /// laid up looks for its refit now, re-planned so any older look is void.
    fn wake_slots(&mut self, h: u32, at: Tick) {
        let waiting: Vec<ShipId> = self.havens[h as usize]
            .ships
            .iter()
            .copied()
            .filter(|&s| {
                self.ships
                    .get(s)
                    .is_some_and(|x| {
                        matches!(x.status, Status::Lost { back, .. } if back <= at || back == LAID_UP)
                    })
            })
            .collect();
        for s in waiting {
            if let Status::Lost { back, .. } = &mut self.ships[s].status
                && *back == LAID_UP
            {
                *back = at;
            }
            let plan = self.bump(s);
            self.schedule(at, Ev::Refit { ship: s, plan });
        }
    }

    /// A ship laid up (§2.4.6a rule 2, PC3): no lane worth hunting while its haven starves, so
    /// its crew goes ashore (capped as [`Self::add_crew`] caps it) and the row is kept lost with
    /// no refit due ([`LAID_UP`]) until the haven is fed again or refounds. Not sunk: no sinking
    /// counted, no pressure, no "sunk" on the tape. Its captain dies to the books (grudges pass
    /// to the haven, as at a sinking); what its hold kept stays there, still held.
    fn lay_up(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let Some(h) = self.haven_of(s) else {
            return;
        };
        let crew = self.ships[s].crew;
        self.settle_pool(h, at);
        let cap = self.tun.crew_pool_cap * 1000;
        let hv = &mut self.havens[h as usize];
        hv.pool_milli = (hv.pool_milli + crew.saturating_mul(1000)).min(cap.max(hv.pool_milli));
        hv.counts.laid_up += 1;
        if let Some(l) = self.ships[s].pirate.as_mut().and_then(|p| p.leg.take()) {
            w.legs.cancel(l.leg);
        }
        if let Some(c) = self.ships[s].pirate.as_ref().map(|p| p.captain)
            && w.knowledge.notable(c).is_some()
        {
            w.knowledge.die(c, at);
        }
        let x = &mut self.ships[s];
        x.crew = 0;
        x.status = Status::Lost {
            since: at,
            back: LAID_UP,
        };
        x.plan = x.plan.wrapping_add(1);
        self.set_embodied(s, false);
        self.note(
            at,
            "laid_up",
            Some(s),
            format!("is laid up: haven {h} starves and no lane is worth hunting"),
        );
        self.haven_quiet(h, at);
    }

    /// Haven `h` has just lost an active ship (sunk, laid up, or paid out in tolls): with none
    /// left it shuts if it starves (§2.4.6a rule 4, PC4), or looks again when it would starve.
    /// A haven whose ships were sunk while it was fed stays open: its slots refit as before.
    fn haven_quiet(&mut self, h: u32, at: Tick) {
        if !self.tun.lifecycle || self.havens[h as usize].shut.is_some() {
            return;
        }
        let active = self.havens[h as usize]
            .ships
            .iter()
            .any(|&s| self.ships.get(s).is_some_and(Ship::is_active));
        if active {
            return;
        }
        if self.starving(h, at) {
            self.shut(h, false, at);
        } else {
            let hv = &mut self.havens[h as usize];
            hv.epoch = hv.epoch.wrapping_add(1);
            let (t, epoch) = (hv.fed_until, hv.epoch);
            self.schedule(t, Ev::Refound { haven: h, epoch });
        }
    }

    /// Haven `h` shuts (§2.4.6a rule 4, PC4): its slots' pending refits are void, and its site
    /// looks for a new band after `refound.delay_ticks(P, day)` (rule 5). Pressure is kept, so
    /// a site players cleared stays empty for longer.
    fn shut(&mut self, h: u32, broken: bool, at: Tick) {
        let rule = self.tun.refound;
        let hv = &self.havens[h as usize];
        let back = at + rule.delay_ticks(hv.pressure, day_num(at)).max(1);
        let slots = hv.ships.clone();
        for s in slots {
            if self.ships.get(s).is_some_and(|x| !x.is_active()) {
                self.bump(s);
            }
        }
        let hv = &mut self.havens[h as usize];
        hv.shut = Some(Shut {
            since: at,
            back,
            broken,
        });
        hv.epoch = hv.epoch.wrapping_add(1);
        hv.counts.shuts += 1;
        let epoch = hv.epoch;
        self.schedule(back, Ev::Refound { haven: h, epoch });
        self.note(
            at,
            "shut",
            None,
            format!(
                "haven {h} shuts: no ship and no hunt; its site looks again from day {:.2}",
                day_of(back)
            ),
        );
    }

    /// Haven `h`'s look (its [`Ev::Refound`]): open, it shuts if it starves with no ship
    /// (rule 4); shut and due, its site tries for a new band (rule 5).
    fn haven_look(&mut self, w: &mut World, h: u32, epoch: u32, at: Tick) {
        let Some(hv) = self.havens.get(h as usize) else {
            return;
        };
        if !self.tun.lifecycle || hv.epoch != epoch {
            self.stats.stale += 1;
            return;
        }
        match hv.shut {
            None => self.haven_quiet(h, at),
            Some(sh) if at >= sh.back => self.try_refound(w, h, at),
            Some(_) => {}
        }
    }

    /// A shut haven's site tries for a new band (§2.4.6a rule 5, PC5, PC6): only out of every
    /// player's view, and only if a fresh band would find a lane worth hunting. Then it opens
    /// with a quarter of the pool's cap ashore, fed for `starve_days`, and every slot launches
    /// a new ship with its own crew (the band brings them; none come from the pool). Otherwise
    /// the site looks again after `refound.base_days`.
    fn try_refound(&mut self, w: &mut World, h: u32, at: Tick) {
        let in_view = self.dock_in_view(w, h, at);
        if in_view || !self.has_prey(w, h) {
            let base = f64::from(self.tun.refound.base_days) * TICKS_PER_DAY as f64;
            let again = at + (base.round() as u64).max(1);
            let hv = &mut self.havens[h as usize];
            hv.epoch = hv.epoch.wrapping_add(1);
            if let Some(sh) = hv.shut.as_mut() {
                sh.back = again;
            }
            let epoch = hv.epoch;
            self.schedule(again, Ev::Refound { haven: h, epoch });
            self.note(
                at,
                "refound",
                None,
                format!(
                    "haven {h}'s site stays empty ({}); it looks again on day {:.2}",
                    if in_view {
                        "a player is near"
                    } else {
                        "no prey"
                    },
                    day_of(again)
                ),
            );
            return;
        }
        let cap = self.tun.crew_pool_cap * 1000;
        let fed = at + self.starve_ticks();
        let hv = &mut self.havens[h as usize];
        hv.pool_milli = cap / 4;
        hv.pool_at = at;
        hv.fed_until = fed;
        hv.shut = None;
        hv.epoch = hv.epoch.wrapping_add(1);
        hv.counts.refounds += 1;
        let slots = hv.ships.clone();
        for s in slots {
            if self.ships.get(s).is_some_and(|x| !x.is_active()) {
                self.launch(w, s, h, at);
            }
        }
        self.note(
            at,
            "refound",
            None,
            format!("haven {h} is refounded: a new band moves in"),
        );
    }

    /// Whether a fresh band at haven `h` would find prey (§2.4.6a rule 5, PC5): a full crew of
    /// one of its media, with a sound hull, sees a lane worth hunting by [`Self::decide`]'s own
    /// filters.
    fn has_prey(&self, w: &World, h: u32) -> bool {
        let hv = &self.havens[h as usize];
        [Medium::Sea, Medium::Sky]
            .into_iter()
            .filter(|&m| hv.media & medium_bit(m) != 0)
            .any(|m| {
                let a = f64::from(self.tun.medium(m).pirate_crew) * self.tun.pirate_strength;
                hv.lanes
                    .iter()
                    .any(|&l| self.huntable(w, h, m, a, l).is_some())
            })
    }

    /// Whether a player's interest centre is within `view_radius` of haven `h`'s dock at `at`,
    /// on the same frame (§2.4.6a rule 5: nothing appears in a player's view).
    fn dock_in_view(&self, w: &World, h: u32, at: Tick) -> bool {
        let hv = &self.havens[h as usize];
        let dock = (hv.dock.0, hv.dock.2);
        self.players.values().any(|v| {
            v.interest
                .and_then(|s| w.legs.get(s.leg).filter(|r| r.generation == s.generation))
                .is_some_and(|r| {
                    r.path.frame == hv.frame
                        && r.path.t0 <= at
                        && at <= r.path.t1
                        && self.sdist(hv.frame, (r.path.x0, r.path.z0), dock)
                            <= u64::from(self.tun.view_radius)
                })
        })
    }

    // ---- observations --------------------------------------------------------------------------

    fn ship_of(&self, agent: u64, epoch: u32) -> Option<ShipId> {
        let s = Ship::id_of_agent(agent)?;
        let x = self.ships.get(s)?;
        (x.epoch == epoch).then_some(s)
    }

    /// The ship an op-carrying record (CARGO_LOOTED, SHIP_SUNK, SHIP_CAPTURED) is about, whatever
    /// the epoch: what the plugin moved or destroyed really left, even when the record arrives in
    /// a later session (replayed from the journal after a crash, when the pages predate the
    /// materialization; or carried by a chunk) for a body this engine no longer knows. The op id
    /// applies it once (overnight choice U14, as phase 1's review 3 P2).
    fn ship_of_op(&self, agent: u64) -> Option<ShipId> {
        let s = Ship::id_of_agent(agent)?;
        self.ships.get(s).map(|_| s)
    }

    /// Whether a plugin op id was applied already; remembers it if not.
    fn first_time(&mut self, op: u64) -> bool {
        if op == 0 {
            return true;
        }
        if !self.applied_set.insert(op) {
            return false;
        }
        self.applied_ops.push_back(op);
        if self.applied_ops.len() > 4_096
            && let Some(old) = self.applied_ops.pop_front()
        {
            self.applied_set.remove(&old);
        }
        true
    }

    /// What the plugin reported (docs/boundary.md §7.3). Applied at `at`, after the world and the
    /// ships have run to it. A record whose op id was applied before is skipped.
    pub fn ingest(&mut self, w: &mut World, o: &Obs, mask: &dyn WaterMask, at: Tick) {
        let op = match o {
            Obs::Sunk(k) => k.op,
            Obs::Captured(c) => c.op,
            Obs::CargoLooted(c) => c.op,
            _ => 0,
        };
        if !self.first_time(op) {
            return;
        }
        match o {
            Obs::State(_) | Obs::Materialized(_) => {}
            Obs::SpawnRejected(r) => {
                if let Some(s) = self.ship_of(r.agent, r.epoch) {
                    self.set_embodied(s, false);
                    self.lay_up_if_waiting(s, at);
                }
            }
            Obs::Dematerialized(d) => {
                if let Some(s) = self.ship_of(d.agent, d.epoch) {
                    self.set_embodied(s, false);
                    // A lost or laid-up ship's late report changes nothing: its crew went
                    // ashore (or down with it), and a replay must not give it men back.
                    if !self.ships[s].is_active() {
                        return;
                    }
                    let x = &mut self.ships[s];
                    x.hull_milli = d.hull_milli.min(1000);
                    x.crew = d.crew;
                    if !self.lose_if_crewless(w, s, 0, None, at) {
                        // Out of view, a ship whose service is laid up lays up (PC23).
                        self.lay_up_if_waiting(s, at);
                    }
                }
            }
            Obs::Damaged(d) => {
                // A lost ship's late report changes nothing (its slot's refit keeps its plan).
                let Some(s) = self
                    .ship_of(d.agent, d.epoch)
                    .filter(|&s| self.ships[s].is_active())
                else {
                    return;
                };
                let before = self.ships[s].hull_milli;
                let x = &mut self.ships[s];
                x.hull_milli = d.hull_milli.min(before);
                x.crew = x.crew.saturating_sub(d.crew_lost);
                let lost = f64::from(before.saturating_sub(x.hull_milli)) / 1000.0;
                if self.ships[s].crew == 0 && matches!(self.ships[s].role, Role::Pirate { .. }) {
                    let killer = (d.by == By::Player).then(|| self.player(w, d.player));
                    let by = Self::doer(w, d.by, d.player, d.ship);
                    if self.lose_if_crewless(w, s, by, killer, at) {
                        return;
                    }
                }
                // Wounds bring a pirate's departure forward (C24): what is left of the hunt
                // shrinks by three times the share lost, as dragons' visits do (R12).
                if let Some(PState::Hunting { until, .. }) =
                    self.ships[s].pirate.as_ref().map(|p| p.state)
                    && lost > 0.0
                {
                    let left = until.0.saturating_sub(at.0) as f64;
                    let new = at + (left * (1.0 - 3.0 * lost).max(0.0)) as u64;
                    let plan = self.bump(s);
                    if let Some(p) = self.ships[s].pirate.as_mut()
                        && let PState::Hunting { until: u, .. } = &mut p.state
                    {
                        *u = new;
                    }
                    self.schedule(
                        new,
                        Ev::Bored {
                            ship: s,
                            plan,
                            reason: 3,
                        },
                    );
                }
            }
            Obs::Sunk(k) => {
                // A ship already lost is lost once (repeats are no-ops by state).
                let Some(s) = self
                    .ship_of_op(k.agent)
                    .filter(|&s| self.ships[s].is_active())
                else {
                    return;
                };
                let killer = (k.by == By::Player).then(|| self.player(w, k.player));
                let by = Self::doer(w, k.by, k.player, k.ship);
                match self.ships[s].role {
                    Role::Pirate { .. } => self.lose_pirate(w, s, by, killer, at),
                    _ => {
                        if let Some(pl) = killer
                            && let Some(home) = self.home_node(s)
                        {
                            let pos = self.position(w, s, at);
                            let place = Place::new(self.ships[s].frame, pos.0, 64, pos.1);
                            let id = w.knowledge.record(Report {
                                who: pl,
                                whom: w.nodes[home].actor,
                                kind: ReportKind::Killing,
                                severity: SEVERITY_UNIT,
                                flags: rtsim_knowledge::FLAG_PLAYER,
                                at,
                                place,
                            });
                            let ha = w.nodes[home].actor;
                            w.knowledge.witness(ha, id, at);
                        }
                        self.stats.sinkings += 1;
                        self.lose_merchant(w, s, by, "sunk", at);
                        self.note(at, "sunk", Some(s), "is sunk".into());
                    }
                }
            }
            Obs::Boarded(_) => {}
            Obs::Captured(c) => {
                let Some(s) = self
                    .ship_of_op(c.agent)
                    .filter(|&s| self.ships[s].is_active())
                else {
                    return;
                };
                match c.by {
                    By::Player => {
                        // A10: what a player takes leaves the sim; the hold goes with it.
                        let cargo = std::mem::take(&mut self.ships[s].cargo);
                        for (g, a) in w.held_to_players(&cargo) {
                            add(&mut self.ledger.taken_by_players, g, a);
                        }
                        self.stats.captures += 1;
                        match self.ships[s].role {
                            Role::Pirate { .. } => {
                                let pl = self.player(w, c.player);
                                self.lose_pirate(w, s, aid(pl), Some(pl), at);
                            }
                            _ => {
                                let by = Self::doer(w, c.by, c.player, c.ship);
                                self.lose_merchant(w, s, by, "captured by a player", at);
                            }
                        }
                    }
                    By::Ship => {
                        let Some(p) =
                            Ship::id_of_agent(c.ship).filter(|&p| self.haven_of(p).is_some())
                        else {
                            return;
                        };
                        let cargo = std::mem::take(&mut self.ships[s].cargo);
                        for (g, a) in cargo {
                            *self.ships[p].cargo.entry(g).or_default() += a;
                            add(&mut self.ledger.looted_by_pirates, g, a);
                        }
                        let h = self.haven_of(p).expect("checked");
                        self.add_crew(h, c.crew_taken, at);
                        self.stats.captures += 1;
                        self.robbery_report(w, p, s, at);
                        self.lose_merchant(w, s, sid(p), "captured by pirates", at);
                        self.check_hold(w, p, at);
                    }
                    _ => {}
                }
            }
            Obs::CargoLooted(c) => {
                let Some(s) = self.ship_of_op(c.agent) else {
                    return;
                };
                if matches!(self.ships[s].role, Role::Merchant) {
                    self.promote(w, s, at);
                }
                let mut want = BTreeMap::new();
                for it in &c.items {
                    if let Some(g) = good_of(it.item)
                        && it.count > 0
                    {
                        *want.entry(g).or_insert(0) += i64::from(it.count) * MILLI;
                    }
                }
                let mut taken = BTreeMap::new();
                for (g, a) in want {
                    let have = self.ships[s].cargo.get(&g).copied().unwrap_or(0);
                    let t = a.min(have);
                    if t > 0 {
                        *self.ships[s].cargo.entry(g).or_default() -= t;
                        taken.insert(g, t);
                    }
                }
                self.ships[s].cargo.retain(|_, a| *a > 0);
                if taken.is_empty() {
                    return;
                }
                self.stats.loot_events += 1;
                match c.by {
                    By::Player => {
                        for (g, a) in w.held_to_players(&taken) {
                            add(&mut self.ledger.taken_by_players, g, a);
                        }
                        let pl = self.player(w, c.player);
                        if let Some(home) = self.home_node(s) {
                            let pos = self.position(w, s, at);
                            let place = Place::new(self.ships[s].frame, pos.0, 64, pos.1);
                            let id = w.knowledge.record(Report {
                                who: pl,
                                whom: w.nodes[home].actor,
                                kind: ReportKind::Robbery,
                                severity: SEVERITY_UNIT,
                                flags: rtsim_knowledge::FLAG_PLAYER,
                                at,
                                place,
                            });
                            let ha = w.nodes[home].actor;
                            w.knowledge.witness(ha, id, at);
                        }
                    }
                    By::Ship => {
                        let Some(p) =
                            Ship::id_of_agent(c.ship).filter(|&p| self.haven_of(p).is_some())
                        else {
                            // Nobody of ours took it: back into the hold.
                            for (g, a) in taken {
                                *self.ships[s].cargo.entry(g).or_default() += a;
                            }
                            return;
                        };
                        for (g, a) in &taken {
                            *self.ships[p].cargo.entry(*g).or_default() += a;
                            add(&mut self.ledger.looted_by_pirates, *g, *a);
                        }
                        self.robbery_report(w, p, s, at);
                        self.check_hold(w, p, at);
                    }
                    _ => {
                        for (g, a) in taken {
                            *self.ships[s].cargo.entry(g).or_default() += a;
                        }
                    }
                }
            }
            Obs::Fight(f) => {
                let (Some(a), Some(d)) =
                    (Ship::id_of_agent(f.attacker), Ship::id_of_agent(f.defender))
                else {
                    return;
                };
                // A late or replayed report of a fight whose attacker is lost or laid up since
                // changes nothing: it would put men aboard a ship with none, or over a fresh
                // crew (the record carries no epoch, so only the ship's state can tell).
                if !self.ships.get(a).is_some_and(Ship::is_active) {
                    return;
                }
                self.stats.fights_observed += 1;
                let pairing = match self.ships.get(d).map(|x| x.role) {
                    Some(Role::Navy { .. }) => Pairing::PiratePatrol,
                    _ => Pairing::PirateMerchant,
                };
                let per = self.tun.pirate_strength;
                let before = f.attacker_crew.0.max(1);
                let o = Observed {
                    strength: f64::from(before) * per,
                    survived: f64::from(f.attacker_crew.1) / f64::from(before),
                };
                let v = &mut self.observed[pairing as usize];
                v.push(o);
                if v.len() > self.tun.observed_kept {
                    v.remove(0);
                }
                if let Some(x) = self.ships.get_mut(a) {
                    x.crew = f.attacker_crew.1;
                    x.hull_milli = f.attacker_hull_milli.min(1000);
                }
                if let Some(x) = self.ships.get_mut(d).filter(|x| x.is_active()) {
                    x.crew = f.defender_crew.1;
                    x.hull_milli = f.defender_hull_milli.min(1000);
                }
                // Pirates killed to the last man lose their ship, as in an abstract fight.
                self.lose_if_crewless(w, a, sid(d), None, at);
                // The fight is over: the pirates leave with what they took.
                if self
                    .ships
                    .get(a)
                    .is_some_and(|x| x.is_active() && x.crew > 0)
                    && self.haven_of(a).is_some()
                {
                    self.leave_now(a, if f.outcome == 1 { 1 } else { 3 }, at);
                }
            }
            Obs::PortFound(p) => {
                if let Some(node) = w.node_of_key(p.village) {
                    self.add_port(w, node, p.medium, p.dock, p.berths, mask, at);
                }
            }
            Obs::HavenFound(h) => {
                if let Some(node) = w.node_of_key(h.haven) {
                    self.add_haven(w, node, h.media, h.dock, 1, at);
                }
            }
        }
    }

    // ---- checks ----------------------------------------------------------------------------------

    /// The world's own conservation, and that what the ships hold is exactly what the world
    /// counts as held.
    pub fn check(&self, w: &World) -> Result<(), String> {
        w.check_conserved(w.now()).map_err(|e| e.to_string())?;
        let mut held: BTreeMap<GoodId, i128> = BTreeMap::new();
        for (_, x) in self.ships.iter() {
            for (g, a) in &x.cargo {
                if *a < 0 {
                    return Err(format!("a hold with {a} of good {}", g.0));
                }
                add(&mut held, *g, *a);
            }
        }
        held.retain(|_, v| *v != 0);
        let mut world = w.outside.held.clone();
        world.retain(|_, v| *v != 0);
        if held != world {
            return Err(format!(
                "ships hold {held:?}, the world counts {world:?} as held"
            ));
        }
        self.check_indexes()?;
        self.check_ports(w)
    }

    /// The resident indexes agree with the rows: embodied ships, services' key ranges, and the
    /// lanes' hunters with the pirates hunting them. A whole-table pass.
    pub fn check_indexes(&self) -> Result<(), String> {
        let mut embodied = BTreeSet::new();
        for (id, x) in self.ships.iter() {
            if x.embodied {
                embodied.insert(id);
            }
            // A crewless pirate is lost, never an active ship that can no longer hunt.
            if x.pirate.is_some() && x.is_active() && x.crew == 0 {
                return Err(format!("pirate {id:?} is active with no crew"));
            }
            if let Some(PState::Hunting { lane, .. }) = x.pirate.as_ref().map(|p| p.state)
                && x.is_active()
                && !self.threat(lane).is_some_and(|t| t.hunters.contains(&id))
            {
                return Err(format!(
                    "pirate {id:?} hunts lane {lane} but is not its hunter"
                ));
            }
            if let Some(s) = x.service {
                let r = self
                    .service_ships
                    .get(s as usize)
                    .ok_or_else(|| format!("ship {id:?} on service {s}, which does not exist"))?;
                if !r.ids().any(|o| o == id) {
                    return Err(format!("ship {id:?} is not in service {s}'s range {r:?}"));
                }
            }
        }
        if embodied != self.embodied {
            return Err(format!(
                "embodied ships {embodied:?}, the index has {:?}",
                self.embodied
            ));
        }
        // A haven's pool never passes its cap (§2.4.6): hires approach it, captures stop at it.
        for (h, hv) in self.havens.iter().enumerate() {
            if hv.pool_milli > self.tun.crew_pool_cap * 1000 {
                return Err(format!(
                    "haven {h}'s pool holds {} crew, over the cap of {}",
                    f64::from(hv.pool_milli) / 1000.0,
                    self.tun.crew_pool_cap
                ));
            }
        }
        // The lifecycle (§2.4.6a): a shut haven has no active ship, and exactly one live look
        // (an open one at most one); a laid-up ship's crew is ashore.
        let now = self.now();
        let mut looks = vec![0usize; self.havens.len()];
        for (_, e) in self.wheel.iter() {
            if let Ev::Refound { haven, epoch } = *e
                && self
                    .havens
                    .get(haven as usize)
                    .is_some_and(|hv| hv.epoch == epoch)
            {
                looks[haven as usize] += 1;
            }
        }
        for (h, hv) in self.havens.iter().enumerate() {
            let ships = hv
                .ships
                .iter()
                .filter_map(|&s| self.ships.get(s).map(|x| (s, x)));
            for (s, x) in ships {
                if hv.shut.is_some() && x.is_active() {
                    return Err(format!("haven {h} is shut but its ship {s:?} is active"));
                }
                if x.is_laid_up() && x.crew != 0 {
                    return Err(format!("laid-up ship {s:?} keeps {} crew aboard", x.crew));
                }
                // A ship is laid up only in a haven that starves or is shut; a haven fed
                // again wakes its laid-up slots (PC11).
                if x.is_laid_up() && hv.shut.is_none() && !self.starving(h as u32, now) {
                    return Err(format!(
                        "ship {s:?} is laid up in haven {h}, which is open and fed"
                    ));
                }
            }
            let want = if hv.shut.is_some() { 1..=1 } else { 0..=1 };
            if !want.contains(&looks[h]) {
                return Err(format!(
                    "haven {h} ({}) has {} live looks",
                    if hv.shut.is_some() { "shut" } else { "open" },
                    looks[h]
                ));
            }
        }
        for (l, t) in self.lane_threats() {
            for &h in &t.hunters {
                let hunting = self
                    .ships
                    .get(h)
                    .and_then(|x| x.pirate.as_ref())
                    .map(|p| p.state);
                if !matches!(hunting, Some(PState::Hunting { lane, .. }) if lane == l) {
                    return Err(format!("lane {l}'s hunter {h:?} is {hunting:?}"));
                }
            }
        }
        Ok(())
    }

    /// Events pending, by kind (for the no-work-while-unobserved test).
    pub fn pending_by_kind(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for (_, e) in self.wheel.iter() {
            let k = match e {
                Ev::Seen { .. } => "seen",
                Ev::SeenPirate { .. } => "seen_pirate",
                Ev::Unbook { .. } => "unbook",
                Ev::NextLeg { .. } => "next_leg",
                Ev::Arrive { .. } => "arrive",
                Ev::Raid { .. } => "raid",
                Ev::Decide { .. } => "decide",
                Ev::ReachLane { .. } => "reach_lane",
                Ev::Bored { .. } => "bored",
                Ev::Home { .. } => "home",
                Ev::Respawn { .. } => "respawn",
                Ev::Refit { .. } => "refit",
                Ev::Refound { .. } => "refound",
                Ev::DockCheck { .. } => "dock_check",
            };
            *m.entry(k).or_insert(0) += 1;
        }
        m
    }

    /// What the engine holds now: the row tables' resident pages, the network, and the rest of
    /// its resident state. Never a pass over the rows (a paged table counts what is resident).
    pub fn heap_bytes(&self) -> usize {
        self.rows_heap_bytes() + self.network_bytes() + self.resident_bytes_besides()
    }

    /// The row tables' resident pages and their id indexes.
    pub fn rows_heap_bytes(&self) -> usize {
        self.ships.rows().heap_bytes() + self.threats.heap_bytes()
    }

    /// The port network's bytes, the term that grows with the world while rows page (SCALE.md §8
    /// keeps the flow graph resident): see [`Ships::network_breakdown`].
    pub fn network_bytes(&self) -> usize {
        self.network_breakdown().iter().map(|p| p.1).sum()
    }

    /// The network's bytes by part: docks (with the indexes from node to dock and dock to lanes), lanes (with their
    /// paths, their (service, hop) lists and the lane cells), services (with their legs and ship
    /// ranges). Estimates from capacities and sizes, as `heap_bytes` everywhere.
    pub fn network_breakdown(&self) -> [(&'static str, usize); 3] {
        use std::mem::size_of;
        // A B-tree entry with its share of a node (eleven slots, two thirds full).
        let btree = |k: usize, v: usize| (k + v) * 3 / 2 + 8;
        let docks = self.docks.capacity() * size_of::<Dock>()
            + self.dock_of.len() * btree(size_of::<(usize, Medium)>(), 4)
            + self.dock_lanes.capacity() * size_of::<Vec<u32>>()
            + self
                .dock_lanes
                .iter()
                .map(|v| v.capacity() * 4)
                .sum::<usize>();
        let lanes = self.lanes.capacity() * size_of::<Lane>()
            + self
                .lanes
                .iter()
                .map(|l| l.path.heap_bytes())
                .sum::<usize>()
            + self.lane_hops.capacity() * size_of::<Vec<(u32, u32)>>()
            + self
                .lane_hops
                .iter()
                .map(|v| v.capacity() * 8)
                .sum::<usize>()
            + self
                .lane_cells
                .values()
                .map(|v| {
                    btree(size_of::<(u16, i64, i64)>(), size_of::<Vec<u32>>()) + v.capacity() * 4
                })
                .sum::<usize>();
        let services = self.services.capacity() * size_of::<Service>()
            + self.services.iter().map(|s| s.heap_bytes()).sum::<usize>()
            + self.service_ships.capacity() * size_of::<ShipRange>();
        [("docks", docks), ("lanes", lanes), ("services", services)]
    }

    /// A copy of the resident network (docks, lanes, services and their indexes) for a bench to
    /// weigh with its allocator, checking [`Ships::network_breakdown`]'s estimate.
    #[doc(hidden)]
    pub fn clone_network(&self) -> Box<dyn std::any::Any> {
        Box::new((
            self.docks.clone(),
            self.lanes.clone(),
            self.services.clone(),
            self.lane_hops.clone(),
            self.lane_cells.clone(),
            self.dock_of.clone(),
            self.service_ships.clone(),
            self.dock_lanes.clone(),
        ))
    }

    /// Resident state besides the rows and the network: the wheel, havens, navies, players, the
    /// observed-only tables (booked voyages, their legs, crossings seen, embodied ships), applied
    /// op ids, observed fights.
    pub fn resident_bytes_besides(&self) -> usize {
        use std::mem::size_of;
        let btree = |k: usize, v: usize| (k + v) * 3 / 2 + 8;
        self.wheel.heap_bytes()
            + self.havens.capacity() * size_of::<Haven>()
            + self
                .havens
                .iter()
                .map(|h| h.ships.capacity() * 8 + h.lanes.capacity() * 4)
                .sum::<usize>()
            + self.navies.capacity() * size_of::<Navy>()
            + self
                .navies
                .iter()
                .map(|n| n.ships.capacity() * 8)
                .sum::<usize>()
            + self.players.len() * btree(8, size_of::<PlayerView>())
            + self
                .booked
                .values()
                .map(|b| btree(16, size_of::<Booked>()) + b.legs.capacity() * 8)
                .sum::<usize>()
            + self.leg_voyage.len() * btree(8, 16)
            + self.seen_by.len() * btree(24, 0)
            + self.embodied.len() * btree(8, 0)
            + self.applied_ops.capacity() * 8
            + self.applied_set.len() * btree(8, 0)
            + self
                .observed
                .iter()
                .map(|v| v.capacity() * 16)
                .sum::<usize>()
    }

    /// The timetable of a service from `from`: each ship's next `n` departures (hop, dock node,
    /// tick).
    pub fn departures(&self, s: u32, from: Tick, n: usize) -> Departures {
        let sv = &self.services[s as usize];
        let mut out = Vec::new();
        for id in self.service_ships[s as usize].ids() {
            let x = &self.ships[id];
            let f = sv.fix(&self.lanes, &self.docks, x.offset, from);
            let mut v = Vec::new();
            let mut voyage = f.voyage;
            while v.len() < n {
                let t = sv.times(x.offset, voyage);
                let j = (voyage % sv.legs.len() as u64) as usize;
                if t.depart >= from.0 as i64 {
                    v.push((
                        j,
                        self.docks[sv.legs[j].from_dock as usize].node,
                        t.depart as u64,
                    ));
                }
                voyage += 1;
            }
            out.push((id, v));
        }
        out
    }

    /// A service's ships, by slot.
    pub fn service_ships(&self, s: u32) -> impl Iterator<Item = ShipId> + Clone + use<> {
        self.service_ships[s as usize].ids()
    }

    pub fn lane_hops(&self, l: u32) -> &[(u32, u32)] {
        &self.lane_hops[l as usize]
    }
}

/// Whether a leg's owner is a ship's (a voyage or a pirate's leg), not a player's area.
fn owns_leg(owner: Owner) -> bool {
    owner.0 & (PLAYER_OWNER | SHIP_OWNER) == SHIP_OWNER
}

/// The ships as a viewer of the engine's shared interest areas (one per player across every
/// module; overnight choices U2 to U6): an area books the voyages it could see, then the ship
/// legs it crosses enter the player's view, exactly as the ships' own areas
/// ([`Ships::observe_player`]) do. A player's view here keeps no area of its own (`interest`
/// stays `None`), so [`Ships::player_left`] never cancels the shared one.
impl rtsim_world::interest::Viewer for Ships {
    fn owns(&self, owner: Owner) -> bool {
        owns_leg(owner)
    }

    fn area_booked(&mut self, w: &mut World, player: ActorId, uuid: Uuid, area: Stamp) {
        self.players.entry(player).or_insert(PlayerView {
            uuid,
            interest: None,
        });
        if let Some(path) = w.legs.get(area.leg).map(|r| r.path) {
            self.book_voyages_in_view(w, path);
        }
    }

    fn crossings(&mut self, w: &mut World, player: ActorId, area: Stamp, found: &[Meeting]) {
        let _ = area;
        self.seen_ships(w, player, found);
    }
}

fn medium_bit(m: Medium) -> u32 {
    match m {
        Medium::Sea => 1,
        Medium::Sky => 2,
        Medium::Road => 0,
    }
}
