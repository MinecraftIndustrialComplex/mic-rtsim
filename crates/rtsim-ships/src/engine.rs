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
//! - **Pirates** live at havens (C24). Each rests, chooses a lane to hunt (expected loot, cut by
//!   patrols, bent by its captain's temperament, C25), sails there on a booked leg, hunts until it
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
//!   its home dock after a delay, on its own saved slot; a haven rebuilds a pirate ship when its
//!   crew pool can man one, and captured crews join that pool (M2's crew conversion).
//!
//! # Cost
//!
//! O(events). Unobserved merchant and navy ships cost nothing per tick and nothing per voyage;
//! pirates cost four events a hunt and one market event per hazard change. Encounters cost the
//! cells an interest area covers and the voyages booked there.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_ecology::infra::Infrastructure;
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::{ActorId, ActorKind, Place, Report, ReportKind, SEVERITY_UNIT, Temperament};
use rtsim_knowledge::{Trait, witnesses};
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

pub const SYS_SHIP: SystemId = SystemId::named("ships.ship");
const SYS_PIRATE: SystemId = SystemId::named("ships.pirate");
const SYS_RAID: SystemId = SystemId::named("ships.raid");
const SYS_SERVICE: SystemId = SystemId::named("ships.service");

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
    /// Sunk or captured at `since`; relaunched at `back` (merchants and navies; pirates wait for
    /// their haven).
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
}

/// A pirate haven (C24): a world node of kind Haven whose market sells what its pirates take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Haven {
    pub node: usize,
    pub frame: u16,
    pub dock: (i32, i32, i32),
    /// 1 sea pirates, 2 sky pirates.
    pub media: u32,
    /// Crew waiting ashore to man a new ship (M2: captured crews join it).
    pub crew_pool: u32,
    pub ships: Vec<ShipId>,
    /// Lanes within its pirates' reach, in lane order.
    pub lanes: Vec<u32>,
}

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
/// saves and hashes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneThreat {
    /// The haven whose pirates hunt it (one at a time: its raids sail there).
    pub haven: Option<u32>,
    pub hunters: BTreeSet<ShipId>,
    /// Navy presence, thousandths of strength.
    pub patrol_milli: i64,
    /// Players' escorts by key, thousandths of strength (to be fed by a future plugin).
    pub escorts: BTreeMap<u64, i64>,
    /// The share last applied, thousandths.
    pub share: i64,
}

impl LaneThreat {
    fn defence(&self) -> f64 {
        (self.patrol_milli + self.escorts.values().sum::<i64>()) as f64 / 1000.0
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
    Rebuild {
        haven: u32,
        medium: Medium,
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
    pub ships: Arena<Ship>,
    pub havens: Vec<Haven>,
    pub navies: Vec<Navy>,
    pub threats: BTreeMap<u32, LaneThreat>,
    /// Per lane: the (service, hop) pairs that sail it.
    pub(crate) lane_hops: Vec<Vec<(u32, u32)>>,
    /// Per service: its ships, by slot.
    pub(crate) service_ships: Vec<Vec<ShipId>>,
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
    pub(crate) now: Tick,
    pub(crate) built: bool,
}

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn tick_of(t: i64) -> Tick {
    Tick(t.max(0) as u64)
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
            ships: Arena::new(),
            havens: Vec::new(),
            navies: Vec::new(),
            threats: BTreeMap::new(),
            lane_hops: Vec::new(),
            service_ships: Vec::new(),
            lane_cells: BTreeMap::new(),
            dock_of: BTreeMap::new(),
            wheel: TimingWheel::new(now),
            players: BTreeMap::new(),
            booked: BTreeMap::new(),
            leg_voyage: BTreeMap::new(),
            seen_by: BTreeSet::new(),
            observed: Default::default(),
            ledger: ShipLedger::default(),
            stats: ShipStats::default(),
            lints: NetworkLints::default(),
            over_berths: Vec::new(),
            outbox: Vec::new(),
            log_enabled: false,
            log: Vec::new(),
            now,
            built: false,
        }
    }

    pub fn now(&self) -> Tick {
        self.now
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
        let Some(x) = self.ships.get(s) else {
            return format!("ship {}", s.to_bits());
        };
        let m = x.medium.name();
        match x.role {
            Role::Merchant => format!(
                "{m} merchant {}.{}",
                x.service.unwrap_or(u32::MAX),
                x.slot
            ),
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
        if node >= w.nodes.len() || !w.nodes[node].kind.is_storehouse() || medium == Medium::Road
        {
            return None;
        }
        let frame = w.nodes[node].pos.frame;
        let (mut x, mut y, mut z) = dock;
        if medium == Medium::Sea {
            let Some(c) = water::snap_to_water(mask, frame, x, z, self.tun.port_snap_cells) else {
                self.lints
                    .docks_not_on_water
                    .push(self.docks.len() as u32);
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
        self.dock_of.insert((node, medium), id);
        if self.built {
            self.connect(w, id, mask, at);
        }
        Some(id)
    }

    /// A haven joins (HAVEN_FOUND): node `node` (kind Haven) keeps pirates of the media in
    /// `media` (1 sea, 2 sky), `pirates` ships of each, with a crew pool to man one more.
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
            crew_pool: 0,
            ships: Vec::new(),
            lanes: Vec::new(),
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
        self.havens[h as usize].crew_pool = first_crew;
        Some(h)
    }

    fn refresh_haven_lanes(&mut self, h: u32) {
        let hv = &self.havens[h as usize];
        let (hx, hz) = (hv.dock.0, hv.dock.2);
        let r = self.tun.hunt_radius_blocks;
        let mut lanes: Vec<u32> = (0..self.lanes.len() as u32)
            .filter(|&l| {
                let ln = &self.lanes[l as usize];
                let mid = ln.path.at(ln.path.length() / 2);
                ln.frame == hv.frame && dist(mid, (hx, hz)) as i64 <= r
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
        let seed = StreamKey::new(self.seed, SYS_PIRATE, u64::from(h)).block(
            u64::from(slot),
            self.stats.rebuilds,
        )[0];
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
        let mut groups: BTreeMap<(u16, Medium), Vec<u32>> = BTreeMap::new();
        for (i, d) in self.docks.iter().enumerate() {
            groups.entry((d.frame, d.medium)).or_default().push(i as u32);
        }
        let mut loops_all: Vec<Loop> = Vec::new();
        for (_, group) in groups {
            let cands =
                network::candidate_lanes(&self.docks, &group, &self.tun, mask, &mut self.lints);
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
        id
    }

    pub(crate) fn index_lane(&mut self, l: u32) {
        let ln = &self.lanes[l as usize];
        let mut cells = BTreeSet::new();
        for w in ln.path.pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ra, rb) = (
                LegIndex::region_of(i64::from(a.0.min(b.0)), i64::from(a.1.min(b.1))),
                LegIndex::region_of(i64::from(a.0.max(b.0)), i64::from(a.1.max(b.1))),
            );
            for rx in ra.0..=rb.0 {
                for rz in ra.1..=rb.1 {
                    cells.insert((ln.frame, rx, rz));
                }
            }
        }
        if ln.path.pts.len() == 1 {
            let p = ln.path.pts[0];
            let r = LegIndex::region_of(i64::from(p.0), i64::from(p.1));
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
        self.service_ships.push(Vec::new());
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
            slot_milli_per_quantum[dir] += (i128::from(sv.ships)
                * i128::from(mt.hold_slots)
                * 1000
                * i128::from(q.len())
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
        let mut ids = Vec::new();
        for k in 0..n {
            let offset = sv.offset_of(phase, k, n);
            let seed = StreamKey::new(self.seed, SYS_SHIP, u64::from(s)).block(u64::from(k), 0)[0];
            let role = match navy {
                Some(nv) => Role::Navy { navy: nv },
                None => Role::Merchant,
            };
            ids.push(self.ships.insert(Ship {
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
            }));
        }
        let _ = at;
        self.service_ships[s as usize] = ids;
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
        let d = self.docks[dock as usize].clone();
        let mut others: Vec<(u64, u32)> = self
            .docks
            .iter()
            .enumerate()
            .filter(|(i, o)| *i as u32 != dock && o.medium == d.medium && o.frame == d.frame)
            .map(|(i, o)| (dist(o.xz(), d.xz()), i as u32))
            .collect();
        others.sort_unstable();
        for (_, o) in others.into_iter().take(4) {
            let mut lints = NetworkLints::default();
            let group = vec![dock.min(o), dock.max(o)];
            let c = network::candidate_lanes(&self.docks, &group, &self.tun, mask, &mut lints);
            let Some(c) = c.into_iter().next() else {
                continue;
            };
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
            return Some(l);
        }
        None
    }

    /// A navy based at node `node` patrols the lanes of `medium` within the patrol hops of its
    /// dock (M2's recommendation: a patrol is a flow that lowers the lanes' hazard).
    pub fn add_navy(&mut self, w: &mut World, node: usize, medium: Medium, at: Tick) -> Option<u32> {
        let dock = self.dock_of_node(node, medium)?;
        let lanes: Vec<u32> = (0..self.lanes.len() as u32)
            .filter(|&l| self.lanes[l as usize].medium == medium)
            .collect();
        let walk = network::patrol_walk(&self.lanes, &lanes, dock, self.tun.patrol_hops);
        if walk.hops.is_empty() {
            return None;
        }
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
        let ships = self.service_ships[sid as usize].clone();
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
        for l in touched {
            self.refresh_patrol(w, l, at);
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
                .iter()
                .filter_map(|&id| self.ships.get(id))
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
        let t = self.threats.entry(l).or_default();
        if t.patrol_milli != p {
            t.patrol_milli = p;
            self.apply_hazard(w, l, at);
        }
    }

    /// A player's escort on lane `l` (for a future plugin: players lowering hazard with their own
    /// ships), strength in crew-equivalents; 0 removes it.
    pub fn set_escort(&mut self, w: &mut World, l: u32, key: u64, strength: f64, at: Tick) {
        if l as usize >= self.lanes.len() {
            return;
        }
        let v = (strength * 1000.0).round() as i64;
        let t = self.threats.entry(l).or_default();
        if v <= 0 {
            t.escorts.remove(&key);
        } else {
            t.escorts.insert(key, v);
        }
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
        let Some(t) = self.threats.get(&l) else {
            return 0;
        };
        let a: f64 = t.hunters.iter().map(|&h| self.strength(h)).sum();
        fight::hazard_milli(a, t.defence(), &self.tun)
    }

    /// What the share would be with no patrol or escort (for the CLI's comparison).
    pub fn lane_share_unprotected(&self, l: u32) -> i64 {
        let Some(t) = self.threats.get(&l) else {
            return 0;
        };
        let a: f64 = t.hunters.iter().map(|&h| self.strength(h)).sum();
        fight::hazard_milli(a, 0.0, &self.tun)
    }

    /// Writes lane `l`'s share into the world: both directions sail their raided share to the
    /// hunting haven.
    fn apply_hazard(&mut self, w: &mut World, l: u32, at: Tick) {
        let share = self.lane_share(l);
        let Some(t) = self.threats.get(&l) else {
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
        let mid = ln.path.at(ln.path.length() / 2);
        let speed = self.tun.medium(ln.medium).speed;
        let q = w.cfg.trade.quantum;
        let home = (dist(mid, (hv.dock.0, hv.dock.2)) as f64 / speed.max(1e-6)).ceil() as u64;
        let edge = ln.edge;
        let lag = w.edges[edge as usize].lag();
        let delay = lag + q.covering(home.max(1));
        let node = hv.node;
        let mut ok = true;
        for dir in 0..2u8 {
            ok &= w.set_lane_hazard(edge, dir, node, share, delay, at);
        }
        if ok {
            self.threats.get_mut(&l).expect("checked").share = share;
            self.stats.hazard_changes += 1;
            self.note(
                at,
                "hazard",
                None,
                format!(
                    "lane {l} ({}): pirates of haven {h} now take {share}‰ (patrols and escorts {:.1})",
                    ln.medium.name(),
                    self.threats[&l].defence()
                ),
            );
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
    pub fn observe_player(&mut self, w: &mut World, uuid: Uuid, pos: rtsim_settle::Pos, at: Tick) -> ActorId {
        let p = self.player(w, uuid);
        let window = w.legs.window_ticks();
        let keep = self.players.get(&p).and_then(|v| v.interest).and_then(|s| {
            let row = w.legs.get(s.leg).filter(|r| r.generation == s.generation)?;
            let (dx, dz) = (row.path.x0 - pos.x, row.path.z0 - pos.z);
            let moved = i64::from(dx).pow(2) + i64::from(dz).pow(2);
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
        self.stats.interest_bookings += 1;
        let old = self.players.get(&p).and_then(|v| v.interest);
        let stamp = match old.and_then(|s| w.legs.replan(s.leg, path)) {
            Some(s) => s,
            None => w.legs.book(path, Owner(PLAYER_OWNER | p.to_bits())),
        };
        if let Some(v) = self.players.get_mut(&p) {
            v.interest = Some(stamp);
        }
        // Book the voyages this area could see: lanes in the regions it covers, widened by the
        // radius, and on them the voyages overlapping its window.
        let r = i64::from(path.radius) + i64::from(self.tun.body_radius);
        let (x0, z0) = (i64::from(path.x0.min(path.x1)) - r, i64::from(path.z0.min(path.z1)) - r);
        let (x1, z1) = (i64::from(path.x0.max(path.x1)) + r, i64::from(path.z0.max(path.z1)) + r);
        let (ra, rb) = (LegIndex::region_of(x0, z0), LegIndex::region_of(x1, z1));
        let mut lanes = BTreeSet::new();
        for rx in ra.0..=rb.0 {
            for rz in ra.1..=rb.1 {
                if let Some(v) = self.lane_cells.get(&(path.frame, rx, rz)) {
                    lanes.extend(v.iter().copied());
                }
            }
        }
        let mut want: Vec<(ShipId, u64)> = Vec::new();
        for l in lanes {
            for &(s, j) in &self.lane_hops[l as usize] {
                for &id in &self.service_ships[s as usize] {
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
        // Every ship leg sharing a cell with the area: one meeting per ship voyage (or pirate
        // plan), the earliest.
        let mut first: BTreeMap<(ShipId, u64), Meeting> = BTreeMap::new();
        let mut pirates: BTreeMap<ShipId, Meeting> = BTreeMap::new();
        for other in w.legs.sharing(stamp) {
            let Some(row) = w.legs.get(other.leg) else {
                continue;
            };
            if row.owner.0 & (PLAYER_OWNER | SHIP_OWNER) != SHIP_OWNER {
                continue;
            }
            let Some(m) = w.legs.meeting(other, stamp) else {
                continue;
            };
            if let Some(&(id, v)) = self.leg_voyage.get(&other.leg) {
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
            out.push(Path {
                frame,
                x0: a.0,
                z0: a.1,
                x1: b.0,
                z1: b.1,
                t0: tick_of(t0),
                t1: tick_of(t1),
                radius,
            });
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
    /// each event's time first. Nothing is ticked.
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            w.run_until(at);
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        w.run_until(to);
        self.now = self.now.max(to);
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
                let pos = leg
                    .and_then(|s| w.legs.get(s.leg))
                    .map(|r| r.path.at(at))
                    .unwrap_or((0, 0));
                self.materialize_pirate(ship, (pos.0 as i32, pos.1 as i32), 0, player, at);
            }
            Ev::Unbook { ship, voyage } => self.unbook(w, ship, voyage),
            Ev::NextLeg { ship, epoch } => self.next_leg(ship, epoch, at),
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
            Ev::Respawn { ship } => self.respawn(ship, at),
            Ev::Rebuild { haven, medium } => self.rebuild(w, haven, medium, at),
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
            for &o in &self.service_ships[s as usize] {
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
        let fix = self.services[s as usize].fix(&self.lanes, &self.docks, x.offset, at);
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
            let share = self.threats.get(&lane).map_or(0, |t| t.share);
            let hunter = self
                .threats
                .get(&lane)
                .and_then(|t| {
                    t.hunters
                        .iter()
                        .copied()
                        .max_by(|a, b| {
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
        let mt = self.tun.medium(x.medium);
        let (role, body, crew_body) = match x.role {
            Role::Navy { .. } => (WireRole::Navy, mt.navy_body, mt.navy_crew_body),
            _ => (WireRole::Merchant, mt.merchant_body, mt.merchant_crew_body),
        };
        let cargo = self.cargo_of(w, id, fix.voyage);
        let waypoints = if g.hop.forward {
            ln.path.pts.clone()
        } else {
            ln.path.reversed().pts
        };
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
            pos: (f64::from(fix.x), f64::from(fix.y), f64::from(fix.z)),
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
        let x = &mut self.ships[id];
        x.epoch = epoch;
        x.embodied = true;
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

    /// An embodied timetabled ship reached its next dock: its next voyage's timetable goes to the
    /// plugin, and so on while it stays embodied (observed, so these events are allowed).
    fn next_leg(&mut self, id: ShipId, epoch: u32, at: Tick) {
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
        let fix = sv.fix(&self.lanes, &self.docks, x.offset, at);
        let g = sv.legs[fix.leg];
        let ln = &self.lanes[g.hop.lane as usize];
        let waypoints = if g.hop.forward {
            ln.path.pts.clone()
        } else {
            ln.path.reversed().pts
        };
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
        let fix = self.services[s as usize].fix(&self.lanes, &self.docks, x.offset, at);
        if x.promoted.is_some_and(|p| p.voyage == fix.voyage) {
            return true;
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
        let Some(t) = self.threats.get(&lane) else {
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
            return (p.0 as i32, p.1 as i32);
        }
        let h = self.haven_of(s).unwrap_or(0) as usize;
        let d = self.havens[h].dock;
        (d.0, d.2)
    }

    fn book_pirate_leg(&mut self, w: &mut World, s: ShipId, path: Path) {
        let old = self.ships[s].pirate.as_ref().and_then(|p| p.leg);
        let stamp = match old.and_then(|l| w.legs.replan(l.leg, path)) {
            Some(st) => st,
            None => w.legs.book(path, Owner(SHIP_OWNER | (s.to_bits() & MASK48))),
        };
        self.stats.legs_booked += 1;
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.leg = Some(stamp);
        }
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
        let ticks = ((dist(from, to) as f64 / speed.max(1e-6)).ceil() as u64).max(1);
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
        let tm = self.temperament(s);
        let a = self.strength(s);
        let medium = self.ships[s].medium;
        let hv = &self.havens[h as usize];
        let home = (hv.dock.0, hv.dock.2);
        let mut cands: Vec<(u32, f64)> = Vec::new();
        for &l in &hv.lanes {
            let ln = &self.lanes[l as usize];
            if ln.medium != medium || ln.edge == u32::MAX {
                continue;
            }
            if let Some(t) = self.threats.get(&l)
                && let Some(other) = t.haven
                && other != h
                && (!t.hunters.is_empty() || t.share > 0)
            {
                continue;
            }
            let vol = self.lane_volume(w, l);
            if vol <= 0 {
                continue;
            }
            let defence = self.threats.get(&l).map_or(0.0, |t| t.defence());
            let surv = fight::square_law(a, defence, &self.tun);
            if surv <= 0.0 {
                continue;
            }
            // Bold captains mind patrols less: the survival share is raised to a power from 2
            // (timid) to 0 (reckless).
            let exp = 2.0 - f64::from(tm.bold) / 128.0;
            let mid = ln.path.at(ln.path.length() / 2);
            let far = 1.0 + dist(mid, home) as f64 / 2_000.0;
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
        let mid = ln.path.at(ln.path.length() / 2);
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
        self.note(at, "captain", Some(s), "a new captain takes the ship".into());
    }

    fn reach_lane(&mut self, w: &mut World, infra: &mut dyn Infrastructure, s: ShipId, at: Tick) {
        let Some(PState::Outbound { lane, .. }) = self.ships[s].pirate.as_ref().map(|p| p.state)
        else {
            return;
        };
        let h = self.haven_of(s).expect("a pirate");
        let tm = self.temperament(s);
        let defence = self.threats.get(&lane).map_or(0.0, |t| t.defence());
        let surv = fight::square_law(self.strength(s), defence, &self.tun);
        // Bored after the hunt's length, bent by boldness, cut by damage and by patrols driving
        // them off (C24: threats get bored).
        let base = tm.bend(Trait::Bold, 1000, 50) as f64 / 1000.0;
        let hull = f64::from(self.ships[s].hull_milli) / 1000.0;
        let len = (self.tun.pirate_hunt_ticks as f64 * base * hull * surv.max(0.25)).max(1.0) as u64;
        let until = at + len;
        let plan = self.ships[s].plan;
        let t = self.threats.entry(lane).or_default();
        if t.haven.is_none_or(|o| o == h || (t.hunters.is_empty() && t.share == 0)) {
            t.haven = Some(h);
        }
        t.hunters.insert(s);
        if let Some(p) = self.ships[s].pirate.as_mut() {
            p.state = PState::Hunting {
                lane,
                since: at,
                until,
            };
        }
        self.stats.hunts += 1;
        let ln = &self.lanes[lane as usize];
        let mid = ln.path.at(ln.path.length() / 2);
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
                self.threats[&lane].share
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
        let mid = ln.path.at(ln.path.length() / 2);
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
        let (loot, what) = if let Some(site) = sites.iter().find(|d| !d.watched) {
            let t = Threat {
                id,
                source: ThreatSource::Bandit,
                class,
                count: crew,
                strength,
                target: ThreatTarget::District(site.id),
                at,
            };
            let Some(o) = infra.raid(&t) else {
                return;
            };
            (o.stolen, format!("district {}", site.id.0))
        } else if let Some(tr) = infra.trains_crossed(frame, &regions, at, until).first().copied() {
            let t = Threat {
                id,
                source: ThreatSource::Bandit,
                class,
                count: crew,
                strength,
                target: ThreatTarget::Train(tr.train),
                at,
            };
            let Some(l) = infra.raid_train(&t) else {
                return;
            };
            (l, format!("player leg {}", tr.train.0))
        } else {
            return;
        };
        self.stats.coast_raids += 1;
        let mut got = 0i64;
        for (g, n) in loot.iter() {
            let name = infra.good_name(g).unwrap_or_else(|| format!("good {}", g.0));
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
        if let Some(t) = self.threats.get_mut(&lane) {
            t.hunters.remove(&s);
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

    fn materialize_pirate(
        &mut self,
        s: ShipId,
        pos: (i32, i32),
        target: u64,
        player: ActorId,
        at: Tick,
    ) {
        let x = &self.ships[s];
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
        let x = &mut self.ships[s];
        x.epoch = epoch;
        x.embodied = true;
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
        let fix = self.services[vs as usize].fix(&self.lanes, &self.docks, self.ships[victim].offset, at);
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
                self.materialize_pirate(p, pos, Ship::agent(victim), player, at);
            }
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
            && let Some(t) = self.threats.get(&l)
        {
            d += t.escorts.values().sum::<i64>() as f64 / 1000.0;
        }
        let out = fight::resolve(a, d, &self.observed[Pairing::PirateMerchant as usize], &self.tun);
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
                        format!(", raised by a real fight where pirates kept {:.0}%", f * 100.0)
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
        self.ships[p].hull_milli = (f64::from(self.ships[p].hull_milli)
            * (0.5 + 0.5 * survived))
            .round() as u32;
        self.ships[victim].crew = vcrew;
        if pcrew == 0 {
            self.lose_pirate(w, p, None, at);
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
        if taken > 0 {
            self.stats.loot_events += 1;
            self.robbery_report(w, p, victim, at);
        }
        if captured {
            let h = self.haven_of(p).expect("a pirate");
            // The beaten crew surrendered before the last man: some of them change sides (M2).
            let standing = vcrew.max((vc * self.tun.surrender_share_milli).div_ceil(1000));
            let joined = (standing * self.tun.crew_conversion_milli).div_ceil(1000);
            self.havens[h as usize].crew_pool += joined;
            self.stats.captures += 1;
            self.lose_merchant(w, victim, at);
            self.note(
                at,
                "capture",
                Some(p),
                format!("captures {}; {joined} of its crew join haven {h}", self.ship_name(victim)),
            );
        }
        self.check_hold(w, p, at);
    }

    fn check_hold(&mut self, w: &World, p: ShipId, at: Tick) {
        let full = Self::hold_slot_milli(w, &self.ships[p].cargo)
            >= i64::from(self.tun.pirate_hold_slots) * 1000;
        let hull = self.ships[p].hull_milli < 400;
        if full || hull {
            let plan = self.bump(p);
            self.schedule(
                at,
                Ev::Bored {
                    ship: p,
                    plan,
                    reason: if full { 2 } else { 3 },
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
                let f = self.services[sv as usize].fix(&self.lanes, &self.docks, x.offset, at);
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
    fn lose_merchant(&mut self, w: &mut World, s: ShipId, at: Tick) {
        let cargo = std::mem::take(&mut self.ships[s].cargo);
        for (g, a) in w.destroy_held(&cargo) {
            add(&mut self.ledger.sunk, g, a);
        }
        let back = self.relaunch_time(s, at + self.tun.respawn_ticks);
        let x = &mut self.ships[s];
        x.promoted = None;
        x.quotes.clear();
        x.status = Status::Lost { since: at, back };
        x.embodied = false;
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

    fn respawn(&mut self, s: ShipId, at: Tick) {
        let Some(x) = self.ships.get(s) else {
            return;
        };
        if !matches!(x.status, Status::Lost { back, .. } if back <= at) {
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
        self.note(at, "relaunch", Some(s), "a new ship takes the slot at its home dock".into());
    }

    fn lose_pirate(&mut self, w: &mut World, s: ShipId, killer: Option<ActorId>, at: Tick) {
        let Some(h) = self.haven_of(s) else {
            return;
        };
        if let Some(PState::Hunting { lane, .. }) = self.ships[s].pirate.as_ref().map(|p| p.state)
        {
            if let Some(t) = self.threats.get_mut(&lane) {
                t.hunters.remove(&s);
            }
            self.apply_hazard(w, lane, at);
        }
        let cargo = std::mem::take(&mut self.ships[s].cargo);
        for (g, a) in w.destroy_held(&cargo) {
            add(&mut self.ledger.sunk, g, a);
        }
        // District loot never entered the world's books: it goes down unrecorded there, and the
        // ledger notes it as lost.
        if let Some(p) = self.ships[s].pirate.as_mut() {
            for (g, a) in std::mem::take(&mut p.district_loot) {
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
        let x = &mut self.ships[s];
        x.status = Status::Lost {
            since: at,
            back: Tick(u64::MAX),
        };
        x.embodied = false;
        x.plan = x.plan.wrapping_add(1);
        self.stats.sinkings += 1;
        let medium = self.ships[s].medium;
        self.schedule(at + self.tun.haven_rebuild_ticks, Ev::Rebuild { haven: h, medium });
        self.note(at, "sunk", Some(s), format!("is sunk; haven {h} will rebuild"));
    }

    /// A haven replaces a lost pirate ship when its crew pool can man one: the lost row comes
    /// back with a new captain.
    fn rebuild(&mut self, w: &mut World, h: u32, medium: Medium, at: Tick) {
        let need = self.tun.medium(medium).pirate_crew;
        let lost = self.havens[h as usize].ships.iter().copied().find(|&s| {
            self.ships
                .get(s)
                .is_some_and(|x| x.medium == medium && !x.is_active())
        });
        let Some(s) = lost else {
            return;
        };
        if self.havens[h as usize].crew_pool < need {
            self.schedule(at + self.tun.haven_rebuild_ticks, Ev::Rebuild { haven: h, medium });
            return;
        }
        self.havens[h as usize].crew_pool -= need;
        self.stats.rebuilds += 1;
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
        self.schedule(at + self.tun.pirate_rest_ticks, Ev::Decide { ship: s, plan });
        self.note(at, "rebuilt", Some(s), format!("haven {h} launches a new ship"));
    }

    // ---- observations --------------------------------------------------------------------------

    fn ship_of(&self, agent: u64, epoch: u32) -> Option<ShipId> {
        let s = Ship::id_of_agent(agent)?;
        let x = self.ships.get(s)?;
        (x.epoch == epoch).then_some(s)
    }

    /// What the plugin reported (docs/boundary.md §7.3). Applied at `at`, after the world and the
    /// ships have run to it.
    pub fn ingest(&mut self, w: &mut World, o: &Obs, mask: &dyn WaterMask, at: Tick) {
        match o {
            Obs::State(_) | Obs::Materialized(_) => {}
            Obs::SpawnRejected(r) => {
                if let Some(s) = self.ship_of(r.agent, r.epoch) {
                    self.ships[s].embodied = false;
                }
            }
            Obs::Dematerialized(d) => {
                if let Some(s) = self.ship_of(d.agent, d.epoch) {
                    let x = &mut self.ships[s];
                    x.embodied = false;
                    x.hull_milli = d.hull_milli.min(1000);
                    x.crew = d.crew;
                }
            }
            Obs::Damaged(d) => {
                let Some(s) = self.ship_of(d.agent, d.epoch) else {
                    return;
                };
                let before = self.ships[s].hull_milli;
                let x = &mut self.ships[s];
                x.hull_milli = d.hull_milli.min(before);
                x.crew = x.crew.saturating_sub(d.crew_lost);
                let lost = f64::from(before.saturating_sub(x.hull_milli)) / 1000.0;
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
                let Some(s) = self.ship_of(k.agent, k.epoch) else {
                    return;
                };
                let killer = (k.by == By::Player).then(|| self.player(w, k.player));
                match self.ships[s].role {
                    Role::Pirate { .. } => self.lose_pirate(w, s, killer, at),
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
                        self.lose_merchant(w, s, at);
                        self.note(at, "sunk", Some(s), "is sunk".into());
                    }
                }
            }
            Obs::Boarded(_) => {}
            Obs::Captured(c) => {
                let Some(s) = self.ship_of(c.agent, c.epoch) else {
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
                                self.lose_pirate(w, s, Some(pl), at);
                            }
                            _ => self.lose_merchant(w, s, at),
                        }
                    }
                    By::Ship => {
                        let Some(p) = Ship::id_of_agent(c.ship).filter(|&p| self.haven_of(p).is_some())
                        else {
                            return;
                        };
                        let cargo = std::mem::take(&mut self.ships[s].cargo);
                        for (g, a) in cargo {
                            *self.ships[p].cargo.entry(g).or_default() += a;
                            add(&mut self.ledger.looted_by_pirates, g, a);
                        }
                        let h = self.haven_of(p).expect("checked");
                        self.havens[h as usize].crew_pool += c.crew_taken;
                        self.stats.captures += 1;
                        self.robbery_report(w, p, s, at);
                        self.lose_merchant(w, s, at);
                        self.check_hold(w, p, at);
                    }
                    _ => {}
                }
            }
            Obs::CargoLooted(c) => {
                let Some(s) = self.ship_of(c.agent, c.epoch) else {
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
                        let Some(p) = Ship::id_of_agent(c.ship).filter(|&p| self.haven_of(p).is_some())
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
                self.stats.fights_observed += 1;
                let (Some(a), Some(d)) = (Ship::id_of_agent(f.attacker), Ship::id_of_agent(f.defender))
                else {
                    return;
                };
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
                if let Some(x) = self.ships.get_mut(d) {
                    x.crew = f.defender_crew.1;
                    x.hull_milli = f.defender_hull_milli.min(1000);
                }
                // The fight is over: the pirates leave with what they took.
                if self.ships.get(a).is_some_and(|x| x.is_active() && x.crew > 0)
                    && self.haven_of(a).is_some()
                {
                    let plan = self.bump(a);
                    self.schedule(
                        at,
                        Ev::Bored {
                            ship: a,
                            plan,
                            reason: if f.outcome == 1 { 1 } else { 3 },
                        },
                    );
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
            return Err(format!("ships hold {held:?}, the world counts {world:?} as held"));
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
                Ev::Rebuild { .. } => "rebuild",
            };
            *m.entry(k).or_insert(0) += 1;
        }
        m
    }

    pub fn heap_bytes(&self) -> usize {
        self.ships.heap_bytes()
            + self.ships.iter().map(|(_, x)| x.cargo.len() * 48).sum::<usize>()
            + self.lanes.iter().map(|l| l.path.heap_bytes() + 64).sum::<usize>()
            + self.services.iter().map(|s| s.heap_bytes() + 64).sum::<usize>()
            + self.lane_hops.iter().map(|v| v.capacity() * 8 + 24).sum::<usize>()
            + self.service_ships.iter().map(|v| v.capacity() * 8 + 24).sum::<usize>()
            + self.lane_cells.len() * 64
            + self.wheel.heap_bytes()
            + self.booked.len() * 96
            + self.leg_voyage.len() * 48
            + self.threats.len() * 96
    }

    /// The timetable of a service from `from`: each ship's next `n` departures (hop, dock node,
    /// tick).
    pub fn departures(&self, s: u32, from: Tick, n: usize) -> Departures {
        let sv = &self.services[s as usize];
        let mut out = Vec::new();
        for &id in &self.service_ships[s as usize] {
            let x = &self.ships[id];
            let f = sv.fix(&self.lanes, &self.docks, x.offset, from);
            let mut v = Vec::new();
            let mut voyage = f.voyage;
            while v.len() < n {
                let t = sv.times(x.offset, voyage);
                let j = (voyage % sv.legs.len() as u64) as usize;
                if t.depart >= from.0 as i64 {
                    v.push((j, self.docks[sv.legs[j].from_dock as usize].node, t.depart as u64));
                }
                voyage += 1;
            }
            out.push((id, v));
        }
        out
    }

    pub fn service_ships(&self, s: u32) -> &[ShipId] {
        &self.service_ships[s as usize]
    }

    pub fn lane_hops(&self, l: u32) -> &[(u32, u32)] {
        &self.lane_hops[l as usize]
    }
}

fn medium_bit(m: Medium) -> u32 {
    match m {
        Medium::Sea => 1,
        Medium::Sky => 2,
        Medium::Road => 0,
    }
}
