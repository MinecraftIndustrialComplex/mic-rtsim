// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragon engine: every dragon's life as events on one timing wheel (PLAN.md §9, §4.3, §5.7).
//!
//! # What happens, and when
//!
//! - **Roosting.** A dragon at its lair books one `Hungry` event for when its fullness falls to
//!   the hunting level (a closed form), and a stay leg at the lair so a player who visits finds it.
//! - **Hunting** (P12's recommendation). It weighs, from its lair: caravans on the roads in its
//!   territory (a hazard on the flow), villages' livestock (market stock), and, when it holds a
//!   grudge, the home of the player it hates (P26's recommendation), whose districts it raids
//!   through the industry threat interface (C16 to C18). Season and temperament bend the weights
//!   (C25). It flies there on a booked leg, hunts for a dwell, and flies home: three events.
//! - **Crossings.** A hungry dragon whose leg crosses a road falls on the caravans passing there
//!   (PLAN.md §9 walkthrough, step 2): one event at the crossing.
//! - **Enters view.** Every leg is booked into the shared leg index (A15). A player's interest
//!   area booked there finds the legs it shares cells with, and the other way round, and the first
//!   moment the two come within range is one `Seen` event, stamped with both legs' generations. If
//!   it is still live when it fires, the dragon materializes (a boundary command) at the edge of
//!   the area.
//! - **Threats get bored** (C24). A materialized dragon harassing a place books a departure;
//!   wounds and hunger bring it forward. When it fires, the plugin flies the dragon off and
//!   captures it.
//! - **Grudges** (C3, C24). Wounds, a kin killed and a robbed hoard are reports the dragon
//!   witnesses; its sentiment toward the player falls. When it dies, its strongest memories and
//!   feelings pass to its heir: its youngest offspring, else its mate, else its lair, whose next
//!   occupant takes them over.
//! - **Population** (P13's recommendation: the sim owns every dragon). Lairs join when their
//!   structure generates; their Ice and Fire dragons are adopted. Pairs breed (M1's
//!   recommendation); young disperse to empty roosts; a roost empty for the respawn delay draws a
//!   young dragon from afar.
//!
//! # Cost
//!
//! O(events): a dragon nobody watches costs a handful of events per hunt and nothing per tick.
//! Decisions look up cells of a static prey index; legs look up the cells they cover in the leg
//! index. Nothing scans the world or every dragon.

use std::collections::BTreeMap;

use rtsim_industry::ids::DistrictId;
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::Writer;
use rtsim_kernel::handle::Arena;
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::{
    ActorId, ActorKind, HolderKind, Place, Report, ReportKind, SEVERITY_UNIT, Trait,
};
use rtsim_settle::Pos;
use rtsim_trade::{GoodId, GoodKind};
use rtsim_travel::legs::{Meeting, Path, Stamp};
use rtsim_world::World;
use serde::Serialize;

use crate::boundary::{self, AttackerKind, Cmd, Obs, SpawnKind, Uuid};
use crate::calendar::Season;
use crate::dragon::{
    Dragon, DragonId, FlightLeg, Lair, LairId, LairKind, Purpose, State, Target, agent_of,
    dragon_of,
};
use crate::flight::{self, dragon_owner, owner_dragon, owner_player, player_owner};
use crate::homes::Homes;
use crate::index::PreyIndex;
use crate::infra::Infrastructure;
use crate::prey::{Item, PreyClass, PreyLedger};
use crate::species::{Gender, Species};
use crate::tunables::{DragonTunables, PreyKind};

pub const SYS_DRAGON: SystemId = SystemId::named("ecology.dragon");
const SYS_HUNT: SystemId = SystemId::named("ecology.hunt");
const SYS_BREED: SystemId = SystemId::named("ecology.breed");

/// A lair's actor: a group actor whose external id is the lair key under this tag.
const LAIR_TAG: u64 = 0x4C41_4952 << 32; // "LAIR"

/// The radius a dragon's own legs are booked with: its body, roughly. PLACEHOLDER 16 blocks.
pub const BODY_RADIUS: u32 = 16;

/// How long the plugin may take to fly a departing dragon out of view before it despawns it
/// anyway, ticks. PLACEHOLDER 2,400.
pub const DEPART_TIMEOUT: u32 = 2_400;

/// A hunt over a road or a village starts this far into its range: thousandths of a road kept
/// clear of the villages at its ends. PLACEHOLDER.
const ROAD_MARGIN: u32 = 150;

/// A player's home is raided within this many blocks of its centre: one region (P26).
const HOME_RADIUS: i64 = 512;

/// Crossings looked at per leg at most (bounds a leg's cost). PLACEHOLDER 4.
const MAX_CROSSINGS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ev {
    Hungry {
        d: DragonId,
        plan: u32,
    },
    LegEnd {
        d: DragonId,
        plan: u32,
    },
    HuntEnd {
        d: DragonId,
        plan: u32,
    },
    Cross {
        d: DragonId,
        plan: u32,
        edge: u32,
        frac_milli: u32,
        from: Tick,
        to: Tick,
    },
    Seen {
        d: DragonId,
        player: ActorId,
        meeting: Meeting,
    },
    /// A player's interest area covers a lair where `d` roosts.
    SeenAtLair {
        d: DragonId,
        player: ActorId,
        interest: Stamp,
        lair: LairId,
    },
    Depart {
        d: DragonId,
        plan: u32,
        /// 1 bored, 2 hungry, 3 wounded, 4 passing through.
        reason: u32,
    },
    Breed {
        d: DragonId,
    },
    Disperse {
        d: DragonId,
    },
    Respawn {
        lair: LairId,
        plan: u32,
    },
    Death {
        d: DragonId,
    },
}

/// Counters for tests, the bench and the CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DragonStats {
    pub events: u64,
    pub stale: u64,
    pub legs: u64,
    pub decisions: u64,
    pub hunts: u64,
    pub fruitless: u64,
    pub crossings: u64,
    pub caravans_attacked: u64,
    pub village_raids: u64,
    pub district_raids: u64,
    pub seen: u64,
    pub seen_stale: u64,
    pub materialized: u64,
    pub departures: u64,
    pub bored: u64,
    pub hurt: u64,
    pub deaths_violent: u64,
    pub deaths_age: u64,
    pub births: u64,
    pub respawns: u64,
    pub claims: u64,
    pub migrations: u64,
    pub adopted: u64,
    pub discarded: u64,
    pub hoards_robbed: u64,
}

/// One line of what happened, for the CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub day: f64,
    pub kind: &'static str,
    pub dragon: Option<String>,
    pub what: String,
}

/// One hunt's result, for the season tables.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HuntRecord {
    pub tick: u64,
    pub day: f64,
    pub season: Season,
    pub dragon: String,
    pub kind: PreyKind,
    pub target: String,
    pub food: i32,
    pub taken: Vec<(String, i64)>,
}

/// A new dragon: from a lair's adoption, a hatching, a respawn, or a scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct NewDragon {
    pub name: String,
    pub species: Species,
    pub gender: Gender,
    pub variant: u8,
    pub age_ticks: u64,
    pub lair: Option<LairId>,
    pub fullness: i32,
    /// `None`: full health for its age.
    pub health: Option<f32>,
    /// `None`: derived from the world seed and a counter.
    pub seed: Option<u64>,
    /// Where it is, when it has no lair yet.
    pub at: Option<(Pos, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
struct PlayerView {
    uuid: Uuid,
    /// The player's interest area in the leg index.
    interest: Option<Stamp>,
}

pub struct Dragons {
    pub tun: DragonTunables,
    pub seed: u64,
    pub dragons: Arena<Dragon>,
    pub lairs: Vec<Lair>,
    lair_by_key: BTreeMap<u64, LairId>,
    /// Lairs by region, so an interest area finds roosting dragons without the leg index: a
    /// dragon at rest books nothing there.
    lair_cells: BTreeMap<(u16, i64, i64), Vec<LairId>>,
    wheel: TimingWheel<Ev>,
    pub homes: Homes,
    pub ledger: PreyLedger,
    pub stats: DragonStats,
    pub log_enabled: bool,
    pub log: Vec<LogEntry>,
    pub hunts: Vec<HuntRecord>,
    /// Hunts that found food, per [season][prey kind].
    pub prey_by_season: [[u64; PreyKind::COUNT]; 4],
    /// Fullness gained, per [season][prey kind].
    pub food_by_season: [[i64; PreyKind::COUNT]; 4],
    outbox: Vec<Cmd>,
    players: BTreeMap<ActorId, PlayerView>,
    index: PreyIndex,
    by_entity: BTreeMap<Uuid, DragonId>,
    /// Item dictionary ids the plugin uses (HOARD_ROBBED), mapped to ledger items.
    items: BTreeMap<u32, Item>,
    next_seed: u64,
    now: Tick,
}

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn place(p: Pos, y: i32) -> Place {
    Place::new(p.frame, p.x, y.clamp(i16::MIN as i32, i16::MAX as i32) as i16, p.z)
}

/// The same 64-bit stand-in for a player's UUID the phase 1 world uses (its `uuid_hash`), so one
/// player is one actor in both modules.
pub fn uuid_hash(most: u64, least: u64) -> u64 {
    StreamKey::new(most, SystemId::named("player.uuid"), least).block(0, 0)[0]
}

impl Dragons {
    pub fn new(seed: u64, tun: DragonTunables, now: Tick) -> Self {
        let homes = Homes::new(tun.home_half_life_ticks, tun.home_regions);
        Dragons {
            tun,
            seed,
            dragons: Arena::new(),
            lairs: Vec::new(),
            lair_by_key: BTreeMap::new(),
            lair_cells: BTreeMap::new(),
            wheel: TimingWheel::new(now),
            homes,
            ledger: PreyLedger::default(),
            stats: DragonStats::default(),
            log_enabled: false,
            log: Vec::new(),
            hunts: Vec::new(),
            prey_by_season: [[0; PreyKind::COUNT]; 4],
            food_by_season: [[0; PreyKind::COUNT]; 4],
            outbox: Vec::new(),
            players: BTreeMap::new(),
            index: PreyIndex::default(),
            by_entity: BTreeMap::new(),
            items: BTreeMap::new(),
            next_seed: 0,
            now,
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

    /// Commands for the plugin, oldest first.
    pub fn take_commands(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.outbox)
    }

    /// Maps a plugin item dictionary id to a ledger item (for HOARD_ROBBED).
    pub fn define_item(&mut self, id: u32, item: Item) {
        self.items.insert(id, item);
    }

    pub fn dragon(&self, d: DragonId) -> Option<&Dragon> {
        self.dragons.get(d)
    }

    pub fn by_name(&self, name: &str) -> Option<DragonId> {
        self.dragons
            .iter()
            .find(|(_, d)| d.name == name)
            .map(|(h, _)| h)
    }

    pub fn lair_of_key(&self, key: u64) -> Option<LairId> {
        self.lair_by_key.get(&key).copied()
    }

    fn note(&mut self, at: Tick, kind: &'static str, d: Option<DragonId>, what: String) {
        if !self.log_enabled {
            return;
        }
        let dragon = d.and_then(|d| self.dragons.get(d)).map(|x| x.name.clone());
        self.log.push(LogEntry {
            tick: at.0,
            day: day_of(at),
            kind,
            dragon,
            what,
        });
    }

    fn schedule(&mut self, at: Tick, ev: Ev) {
        self.wheel.schedule(at.max(self.wheel.now()), ev);
    }

    /// The player's actor in the world's knowledge, registered by UUID (idempotent).
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

    // ---- setup ---------------------------------------------------------------------------------

    /// A lair joins when its structure generates (LAIR_FOUND; C22's rule for villages applied to
    /// lairs). Idempotent by key. Its respawn clock starts: if no dragon is adopted into it within
    /// the delay, a young one comes.
    pub fn add_lair(
        &mut self,
        w: &mut World,
        key: u64,
        name: &str,
        species: Species,
        kind: LairKind,
        pos: Pos,
        y: i32,
        at: Tick,
    ) -> LairId {
        if let Some(&l) = self.lair_by_key.get(&key) {
            return l;
        }
        let actor = w
            .knowledge
            .actors_mut()
            .register(ActorKind::Group, LAIR_TAG ^ key);
        w.knowledge.add_holder(actor, HolderKind::Notable);
        let id = self.lairs.len() as LairId;
        self.lairs.push(Lair {
            key,
            name: name.to_string(),
            species,
            kind,
            pos,
            y,
            actor,
            occupants: Vec::new(),
            hoard: BTreeMap::new(),
            vacant_since: Some(at),
            plan: 1,
        });
        self.lair_by_key.insert(key, id);
        let (rx, rz) = rtsim_travel::legs::LegIndex::region_of(i64::from(pos.x), i64::from(pos.z));
        self.lair_cells.entry((pos.frame, rx, rz)).or_default().push(id);
        let delay = self.tun.respawn_delay_days * TICKS_PER_DAY;
        self.schedule(at + delay, Ev::Respawn { lair: id, plan: 1 });
        id
    }

    fn fresh_seed(&mut self) -> u64 {
        self.next_seed += 1;
        StreamKey::new(self.seed, SYS_DRAGON, 0).block(self.next_seed, 0)[0]
    }

    /// A dragon joins the sim at `at`: a notable whose death of age is booked now (C24).
    pub fn add_dragon(&mut self, w: &mut World, n: NewDragon, at: Tick) -> DragonId {
        let seed = n.seed.unwrap_or_else(|| self.fresh_seed());
        let sp = self.tun.species(n.species).clone();
        let life = sp.lifespan_ticks();
        let left = life.saturating_sub(n.age_ticks).max(TICKS_PER_DAY);
        let home = n
            .lair
            .and_then(|l| self.lairs.get(l as usize))
            .map_or(ActorId::NULL, |l| l.actor);
        let actor = w.knowledge.create_notable(seed, home, home, at, left);
        let max = sp.max_health_at(n.age_ticks);
        let row = Dragon {
            actor,
            seed,
            name: n.name,
            species: n.species,
            gender: n.gender,
            variant: n.variant,
            age_base: n.age_ticks,
            age_at: at,
            dies: at + left,
            lair: n.lair,
            fullness: n.fullness.clamp(0, self.tun.full),
            fed_at: at,
            health: n.health.unwrap_or(max).min(max),
            health_at: at,
            state: State::Roosting,
            leg: None,
            plan: 0,
            epoch: 0,
            mate: None,
            parent: None,
            children: Vec::new(),
            victims: Vec::new(),
            fruitless: 0,
            bout: 0,
            last_grudge: None,
            rest: n.at,
        };
        let d = self.dragons.insert(row);
        self.schedule(at + left, Ev::Death { d });
        if let Some(l) = n.lair {
            self.occupy(w, d, l, at, false);
            self.roost(w, d, at);
        }
        d
    }

    /// Two dragons of a lair become mates; the pair breeds (M1's recommendation).
    pub fn pair(&mut self, w: &mut World, a: DragonId, b: DragonId, at: Tick) {
        if a == b || !self.dragons.contains(a) || !self.dragons.contains(b) {
            return;
        }
        self.dragons.get_mut(a).expect("live").mate = Some(b);
        self.dragons.get_mut(b).expect("live").mate = Some(a);
        self.refresh_heir(w, a);
        self.refresh_heir(w, b);
        let female = if self.dragons[a].gender == Gender::Female {
            a
        } else {
            b
        };
        let t = self.next_clutch(female, at);
        self.schedule(t, Ev::Breed { d: female });
    }

    fn next_clutch(&self, d: DragonId, at: Tick) -> Tick {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species);
        // Mature at stage 4 (75 days, Ice and Fire's `isMature`); then every `breed_days`.
        let mature_age = sp.stage_days[2] * TICKS_PER_DAY;
        let until_mature = mature_age.saturating_sub(x.age(at));
        at + until_mature + sp.breed_days * TICKS_PER_DAY
    }

    /// `child` is `parent`'s young (and its mate's): it becomes their heir (C24), and it leaves
    /// to claim a roost of its own when old enough.
    pub fn set_parent(&mut self, w: &mut World, child: DragonId, parent: DragonId, at: Tick) {
        if !self.dragons.contains(child) || !self.dragons.contains(parent) {
            return;
        }
        self.dragons.get_mut(child).expect("live").parent = Some(parent);
        let mate = self.dragons[parent].mate;
        for p in [Some(parent), mate].into_iter().flatten() {
            if let Some(x) = self.dragons.get_mut(p)
                && !x.children.contains(&child)
            {
                x.children.push(child);
            }
            self.refresh_heir(w, p);
        }
        let c = &self.dragons[child];
        let sp = self.tun.species(c.species);
        let due = (sp.disperse_days * TICKS_PER_DAY).saturating_sub(c.age(at));
        self.schedule(at + due, Ev::Disperse { d: child });
    }

    /// The heir link (C24): youngest living child, else mate, else lair.
    fn refresh_heir(&mut self, w: &mut World, d: DragonId) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let youngest = x
            .children
            .iter()
            .copied()
            .filter(|c| self.dragons.contains(*c))
            .min_by_key(|c| (self.dragons[*c].age(self.now), *c));
        let heir = youngest
            .map(|c| self.dragons[c].actor)
            .or_else(|| x.mate.and_then(|m| self.dragons.get(m)).map(|m| m.actor))
            .or_else(|| x.lair.map(|l| self.lairs[l as usize].actor))
            .unwrap_or(ActorId::NULL);
        w.knowledge.set_heir(x.actor, heir);
    }

    // ---- players and interest areas (A15) -------------------------------------------------------

    /// A PLAYER record: where a player is. Feeds the home counter (P26) and books the player's
    /// interest area for the next window, replacing the last (the bridge reports every 20 ticks;
    /// a new area is booked only when the player moved a quarter of the view radius or the old one
    /// ends within a window).
    pub fn observe_player(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) -> ActorId {
        let p = self.player(w, uuid);
        self.homes.observe(p, pos, at);
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
            let path = flight::stay(pos, at, at + 2 * window, self.tun.view_radius);
            self.book_interest(w, p, path);
        }
        p
    }

    /// A stay the caller knows in one piece (a scripted player): one interest area from `from` to
    /// `until`. The home counter still learns from [`Dragons::observe_player`] as time passes.
    pub fn player_stays(
        &mut self,
        w: &mut World,
        uuid: Uuid,
        pos: Pos,
        from: Tick,
        until: Tick,
    ) -> ActorId {
        let p = self.player(w, uuid);
        let path = flight::stay(pos, from, until, self.tun.view_radius);
        self.book_interest(w, p, path);
        p
    }

    /// The player left (logged out, changed dimension): its interest area goes, and with it every
    /// meeting booked against it.
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
            None => w.legs.book(path, player_owner(p)),
        };
        if let Some(v) = self.players.get_mut(&p) {
            v.interest = Some(stamp);
        }
        for other in w.legs.sharing(stamp) {
            let Some(d) = w.legs.get(other.leg).and_then(|r| owner_dragon(r.owner)) else {
                continue;
            };
            if let Some(m) = w.legs.meeting(other, stamp) {
                self.schedule(m.at, Ev::Seen { d, player: p, meeting: m });
            }
        }
        // Dragons at rest in a lair the area covers.
        let r = i64::from(path.radius);
        let (x0, z0) = (i64::from(path.x0.min(path.x1)) - r, i64::from(path.z0.min(path.z1)) - r);
        let (x1, z1) = (i64::from(path.x0.max(path.x1)) + r, i64::from(path.z0.max(path.z1)) + r);
        let (ra, rb) = (
            rtsim_travel::legs::LegIndex::region_of(x0, z0),
            rtsim_travel::legs::LegIndex::region_of(x1, z1),
        );
        let mut found = Vec::new();
        for rx in ra.0..=rb.0 {
            for rz in ra.1..=rb.1 {
                found.extend(self.lair_cells.get(&(path.frame, rx, rz)).into_iter().flatten().copied());
            }
        }
        for l in found {
            let lp = self.lairs[l as usize].pos;
            let (px, pz) = path.at(path.t0);
            if flight::dist((px, pz), (i64::from(lp.x), i64::from(lp.z))) > f64::from(path.radius) {
                continue;
            }
            for d in self.lairs[l as usize].occupants.clone() {
                let at = path.t0.max(self.now);
                self.schedule(at, Ev::SeenAtLair { d, player: p, interest: stamp, lair: l });
            }
        }
    }

    /// A dragon settling at its lair: any player whose interest area covers the lair now sees it.
    fn visitors_at_lair(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(l) = self.dragons[d].lair else {
            return;
        };
        let lp = self.lairs[l as usize].pos;
        let (rx, rz) = rtsim_travel::legs::LegIndex::region_of(i64::from(lp.x), i64::from(lp.z));
        for s in w.legs.in_region(lp.frame, rx, rz, at) {
            let Some(row) = w.legs.get(s.leg) else {
                continue;
            };
            let Some(p) = owner_player(row.owner) else {
                continue;
            };
            if row.path.t1 < at {
                continue;
            }
            let (px, pz) = row.path.at(at);
            if flight::dist((px, pz), (i64::from(lp.x), i64::from(lp.z))) <= f64::from(row.path.radius) {
                self.schedule(at, Ev::SeenAtLair { d, player: p, interest: s, lair: l });
            }
        }
    }

    // ---- the step loop -------------------------------------------------------------------------

    /// Runs every dragon event up to and including `to`, in time order, with the world caught up
    /// to each event's time first. Nothing is ticked.
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        self.index.refresh(w);
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            w.run_until(at);
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        w.run_until(to);
        self.now = self.now.max(to);
    }

    fn handle(&mut self, w: &mut World, infra: &mut dyn Infrastructure, at: Tick, ev: Ev) {
        match ev {
            Ev::Hungry { d, plan } => {
                if !self.live(d, plan) {
                    return;
                }
                if matches!(self.dragons[d].state, State::Roosting) {
                    self.decide(w, d, at, 0);
                }
            }
            Ev::LegEnd { d, plan } => {
                if self.live(d, plan) {
                    self.leg_end(w, infra, d, at);
                }
            }
            Ev::HuntEnd { d, plan } => {
                if self.live(d, plan) {
                    self.hunt_end(w, d, at);
                }
            }
            Ev::Cross {
                d,
                plan,
                edge,
                frac_milli,
                from,
                to,
            } => {
                if !self.live(d, plan) {
                    return;
                }
                let x = &self.dragons[d];
                if x.fullness_at(&self.tun, at) >= self.tun.hunt_below {
                    return;
                }
                self.stats.crossings += 1;
                let (food, taken, victims) = self.hunt_road(w, d, edge, frac_milli, from, to, at);
                self.feed(d, at, food, &victims);
                if food > 0 || !taken.is_empty() {
                    let name = self.road_name(w, edge);
                    self.record_hunt(at, d, PreyKind::Road, format!("{name} (crossing)"), food, taken);
                }
            }
            Ev::Seen { d, player, meeting } => self.seen(w, d, player, meeting, at),
            Ev::SeenAtLair {
                d,
                player,
                interest,
                lair,
            } => {
                let home = self.dragons.get(d).is_some_and(|x| {
                    x.lair == Some(lair) && matches!(x.state, State::Roosting) && x.rest.is_none()
                });
                if !home || !w.legs.is_live(interest) {
                    self.stats.seen_stale += 1;
                    return;
                }
                self.stats.seen += 1;
                self.materialize(w, d, player, false, None, at);
            }
            Ev::Depart { d, plan, reason } => {
                if self.live(d, plan) {
                    self.depart(d, reason, at);
                }
            }
            Ev::Breed { d } => self.breed(w, d, at),
            Ev::Disperse { d } => self.disperse(w, d, at),
            Ev::Respawn { lair, plan } => self.respawn(w, lair, plan, at),
            Ev::Death { d } => {
                let Some(x) = self.dragons.get(d) else {
                    return;
                };
                if x.dies > at {
                    return;
                }
                // The world's loop has already passed its memories on (Knowledge::die) when it
                // caught up to `at`; if not, do it now.
                if w.knowledge.notable(x.actor).is_some() {
                    w.knowledge.die(x.actor, at);
                }
                self.stats.deaths_age += 1;
                self.note(at, "death", Some(d), "dies of age; its heir inherits".into());
                self.remove(w, d, at);
            }
        }
    }

    fn live(&mut self, d: DragonId, plan: u32) -> bool {
        let ok = self.dragons.get(d).is_some_and(|x| x.plan == plan);
        if !ok {
            self.stats.stale += 1;
        }
        ok
    }

    // ---- legs ------------------------------------------------------------------------------------

    fn book_leg(&mut self, w: &mut World, d: DragonId, path: Path, y: i32) {
        self.stats.legs += 1;
        let old = self.dragons[d].leg.map(|l| l.stamp.leg);
        let stamp = match old.and_then(|l| w.legs.replan(l, path)) {
            Some(s) => s,
            None => w.legs.book(path, dragon_owner(d)),
        };
        self.dragons.get_mut(d).expect("live").leg = Some(FlightLeg { path, y, stamp });
        for other in w.legs.sharing(stamp) {
            let Some(p) = w.legs.get(other.leg).and_then(|r| owner_player(r.owner)) else {
                continue;
            };
            if let Some(m) = w.legs.meeting(stamp, other) {
                self.schedule(m.at, Ev::Seen { d, player: p, meeting: m });
            }
        }
    }

    fn cancel_leg(&mut self, w: &mut World, d: DragonId) {
        if let Some(l) = self.dragons.get_mut(d).and_then(|x| x.leg.take()) {
            w.legs.cancel(l.stamp.leg);
        }
    }

    fn bump(&mut self, d: DragonId) -> u32 {
        let x = self.dragons.get_mut(d).expect("live");
        x.plan = x.plan.wrapping_add(1);
        x.plan
    }

    fn target_pos(&self, w: &World, t: Target) -> Pos {
        match t {
            Target::Road { x, z, edge, .. } => {
                let frame = w.nodes[w.edges[edge as usize].a].pos.frame;
                Pos::new(frame, x, z)
            }
            Target::Village { node } => w.nodes[node as usize].pos,
            Target::Home { x, z, .. } => Pos::new(0, x, z),
        }
    }

    fn purpose_pos(&self, w: &World, d: DragonId, p: Purpose) -> (Pos, i32) {
        let x = &self.dragons[d];
        let cruise = self.tun.species(x.species).cruise_y;
        match p {
            Purpose::Hunt(t) => (self.target_pos(w, t), cruise),
            Purpose::Home => match x.lair {
                Some(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
                None => x.position(&self.lairs, self.now),
            },
            Purpose::Claim(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
        }
    }

    /// Flies from where it is now to `to` for `purpose`: one leg, one event at its end, and a
    /// crossing event for each road a hungry dragon passes over.
    fn fly(&mut self, w: &mut World, d: DragonId, purpose: Purpose, at: Tick) {
        let (to, _) = self.purpose_pos(w, d, purpose);
        let (from, _) = self.dragons[d].position(&self.lairs, at);
        let sp = self.tun.species(self.dragons[d].species).clone();
        let path = flight::leg(from, to, at, sp.speed, BODY_RADIUS);
        let plan = self.bump(d);
        {
            let x = self.dragons.get_mut(d).expect("live");
            x.state = State::Flying(purpose);
            x.rest = None;
        }
        self.book_leg(w, d, path, sp.cruise_y);
        self.schedule(path.t1, Ev::LegEnd { d, plan });
        self.book_crossings(w, d, path, purpose, plan);
    }

    fn book_crossings(&mut self, w: &mut World, d: DragonId, path: Path, purpose: Purpose, plan: u32) {
        let skip = match purpose {
            Purpose::Hunt(Target::Road { edge, .. }) => Some(edge),
            _ => None,
        };
        let x = &self.dragons[d];
        // Only a dragon still hungry when it reaches a road hunts there.
        if x.fullness_at(&self.tun, path.t1) >= self.tun.hunt_below {
            return;
        }
        let sp = self.tun.species(x.species);
        let half = flight::flight_ticks(f64::from(sp.reach), sp.speed);
        let (p0, p1) = (
            (i64::from(path.x0), i64::from(path.z0)),
            (i64::from(path.x1), i64::from(path.z1)),
        );
        let (cx, cz) = ((p0.0 + p1.0) / 2, (p0.1 + p1.1) / 2);
        let r = (flight::dist(p0, p1) / 2.0).ceil() as i64 + 1;
        let mut found = Vec::new();
        for e in self.index.roads_near(path.frame, cx, cz, r) {
            if Some(e) == skip {
                continue;
            }
            let edge = &w.edges[e as usize];
            let (a, b) = (w.nodes[edge.a].pos, w.nodes[edge.b].pos);
            let (q0, q1) = (
                (i64::from(a.x), i64::from(a.z)),
                (i64::from(b.x), i64::from(b.z)),
            );
            if let Some((fd, fr)) = flight::crossing(p0, p1, q0, q1)
                && (ROAD_MARGIN..=1000 - ROAD_MARGIN).contains(&fr)
            {
                let tc = path.t0 + (path.t1.0 - path.t0.0) * u64::from(fd) / 1000;
                found.push((tc, e, fr));
            }
        }
        found.sort_unstable();
        for (tc, edge, frac_milli) in found.into_iter().take(MAX_CROSSINGS) {
            let (from, to) = (Tick(tc.0.saturating_sub(half)), tc + half);
            self.schedule(
                to,
                Ev::Cross {
                    d,
                    plan,
                    edge,
                    frac_milli,
                    from,
                    to,
                },
            );
        }
    }

    /// Settles at its lair: one `Hungry` event, and no leg (a visiting player finds it through
    /// the lair index).
    fn roost(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let plan = self.bump(d);
        let hunt_below = self.tun.hunt_below;
        let rest = self.tun.rest_ticks.max(1);
        let x = self.dragons.get_mut(d).expect("live");
        x.state = State::Roosting;
        x.rest = None;
        x.bout = 0;
        let t = x.hungry_at(&self.tun, hunt_below, at).max(at + rest);
        // At rest it books nothing in the leg index: players find it through the lair.
        self.cancel_leg(w, d);
        self.schedule(t, Ev::Hungry { d, plan });
        self.visitors_at_lair(w, d, at);
    }

    /// Sends `d` to lair `l` to claim it (a migration ordered from outside: an admin command, a
    /// test), from `at`, which the caller has run the engine to. Ignored for an embodied dragon,
    /// which the plugin is flying.
    pub fn relocate(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick) {
        if !self.dragons.contains(d) || self.dragons[d].is_embodied() || l as usize >= self.lairs.len() {
            return;
        }
        self.leave_lair(w, d, at);
        self.lairs[l as usize].plan += 1;
        self.lairs[l as usize].vacant_since = None;
        self.fly(w, d, Purpose::Claim(l), at);
    }

    // ---- choosing prey -----------------------------------------------------------------------------

    /// Picks the best prey and flies to it. `chain` counts hunts already made in this bout.
    fn decide(&mut self, w: &mut World, d: DragonId, at: Tick, chain: u8) {
        self.dragons.get_mut(d).expect("live").bout = chain;
        self.stats.decisions += 1;
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let stage = x.stage(&sp, at);
        if !stage.hunts() {
            // A hatchling is fed by its parents (abstractly): it stays full.
            let full = self.tun.full;
            self.dragons.get_mut(d).expect("live").set_fullness(full, at);
            self.roost(w, d, at);
            return;
        }
        match self.choose(w, d, at) {
            Some((t, kind, u)) => {
                let x = self.dragons.get_mut(d).expect("live");
                x.bout = chain + 1;
                if kind == PreyKind::Grudge {
                    x.last_grudge = Some(at);
                }
                let name = self.target_name(w, t);
                self.note(
                    at,
                    "hunt",
                    Some(d),
                    format!("hungry ({}), hunts {} at {name} (utility {u:.1})", self.dragons[d].fullness_at(&self.tun, at), kind.name()),
                );
                self.fly(w, d, Purpose::Hunt(t), at);
            }
            None => {
                self.dragons.get_mut(d).expect("live").fruitless += 1;
                self.stats.fruitless += 1;
                if chain > 0 {
                    self.fly(w, d, Purpose::Home, at);
                } else if !self.migrate(w, d, at) {
                    // Nothing worth it within range: rest and try again in a day.
                    let plan = self.bump(d);
                    self.schedule(at + TICKS_PER_DAY, Ev::Hungry { d, plan });
                }
            }
        }
    }

    /// The best target by utility: expected fullness × season weight, less flight cost and risk,
    /// bent by temperament (C25); a grudge adds its pull. Ties go to the lower kind, then index.
    fn choose(&self, w: &World, d: DragonId, at: Tick) -> Option<(Target, PreyKind, f64)> {
        let mut best: Option<(Target, PreyKind, f64)> = None;
        for (t, k, u) in self.weigh(w, d, at) {
            if u > 0.0 && best.is_none_or(|b| u > b.2) {
                best = Some((t, k, u));
            }
        }
        best
    }

    /// Every target `d` would weigh at `at`, with its utility, in the order weighed (grudges,
    /// villages, roads). For diagnostics; [`Dragons::choose`] takes the best positive one.
    pub fn weigh(&self, w: &World, d: DragonId, at: Tick) -> Vec<(Target, PreyKind, f64)> {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species);
        let (pos, _) = x.position(&self.lairs, at);
        let origin = match x.lair {
            Some(l) => self.lairs[l as usize].pos,
            None => pos,
        };
        let (ox, oz) = (i64::from(origin.x), i64::from(origin.z));
        let stage = x.stage(sp, at);
        let range = i64::from(sp.territory[stage.index()]) * (1 + i64::from(x.fruitless.min(3)));
        let season = self.tun.calendar.season_at(at);
        let temper = x.temperament();
        let need = f64::from((self.tun.full - x.fullness_at(&self.tun, at)).min(sp.appetite));
        let cost = |p: Pos| {
            flight::dist((ox, oz), (i64::from(p.x), i64::from(p.z))) / 1000.0
                * f64::from(self.tun.distance_cost_per_kblock)
        };
        let mut all: Vec<(Target, PreyKind, f64)> = Vec::new();
        let mut offer = |t: Target, k: PreyKind, u: f64| all.push((t, k, u));
        // Grudges: the player it hates, at their home (P26). A starving dragon eats first, and
        // one that just went after a grudge hunts for food for a while.
        let starving = x.fullness_at(&self.tun, at) <= self.tun.starving;
        let cooling = x
            .last_grudge
            .is_some_and(|t| at.0 < t.0 + self.tun.grudge_cooldown_ticks);
        let grudges = if starving || cooling {
            Vec::new()
        } else {
            w.knowledge.sentiment_snapshot(x.actor, at)
        };
        for (target, v) in grudges {
            if v > self.tun.grudge_below || !w.knowledge.actors().is_player(target) {
                continue;
            }
            let Some(home) = self.homes.home(target, at) else {
                continue;
            };
            let dist = flight::dist((ox, oz), (i64::from(home.pos.x), i64::from(home.pos.z)));
            if dist > f64::from(sp.grudge_range) {
                continue;
            }
            let pull = f64::from(temper.weight(Trait::Aggressive, self.tun.grudge_weight * 100))
                / 100.0
                * f64::from(-i32::from(v))
                / 127.0;
            let w_season = f64::from(self.tun.season_weight(season, PreyKind::Grudge)) / 100.0;
            offer(
                Target::Home {
                    player: target,
                    x: home.pos.x,
                    z: home.pos.z,
                },
                PreyKind::Grudge,
                pull * w_season + need * 0.1 - cost(home.pos),
            );
        }
        // Villages' livestock.
        let w_village = f64::from(self.tun.season_weight(season, PreyKind::Village)) / 100.0;
        // Bold dragons fear a village's defenders less (C25): twice the risk at boldness 0,
        // none at 255.
        let fear = f64::from(self.tun.village_risk) * (2.0 - f64::from(temper.bold) / 128.0).max(0.0);
        for node in self.index.villages_near(origin.frame, ox, oz, range) {
            let n = &w.nodes[node as usize];
            let dist = flight::dist((ox, oz), (i64::from(n.pos.x), i64::from(n.pos.z)));
            if dist > range as f64 {
                continue;
            }
            let food = self.village_food(w, node as usize, sp.village_share_milli).min(need);
            if food <= 0.0 {
                continue;
            }
            offer(
                Target::Village { node },
                PreyKind::Village,
                food * w_village - fear - cost(n.pos),
            );
        }
        // Roads.
        let w_road = f64::from(self.tun.season_weight(season, PreyKind::Road)) / 100.0;
        for e in self.index.roads_near(origin.frame, ox, oz, range) {
            let edge = &w.edges[e as usize];
            let (a, b) = (w.nodes[edge.a].pos, w.nodes[edge.b].pos);
            let (qa, qb) = (
                (i64::from(a.x), i64::from(a.z)),
                (i64::from(b.x), i64::from(b.z)),
            );
            let f = flight::closest_on(qa, qb, (ox, oz), ROAD_MARGIN, 1000 - ROAD_MARGIN);
            let p = flight::lerp(qa, qb, f);
            let pt = Pos::new(a.frame, p.0 as i32, p.1 as i32);
            if flight::dist((ox, oz), p) > range as f64 {
                continue;
            }
            let food = self.road_food(w, e as usize, sp.catch_milli, sp.caravan_share_milli).min(need);
            if food <= 0.0 {
                continue;
            }
            offer(
                Target::Road {
                    edge: e,
                    x: pt.x,
                    z: pt.z,
                    frac_milli: f,
                },
                PreyKind::Road,
                food * w_road - cost(pt),
            );
        }
        all
    }

    fn class_of_world(&self, w: &World, g: GoodId) -> PreyClass {
        let def = w.goods.get(g);
        self.tun
            .prey
            .class(&def.name, matches!(def.kind, GoodKind::Currency(_)))
    }

    /// Expected fullness from a raid on a village's livestock.
    fn village_food(&self, w: &World, node: usize, share_milli: i64) -> f64 {
        if !w.nodes[node].kind.is_storehouse() {
            return 0.0;
        }
        // The stock as last anchored: an estimate is enough to weigh, and it costs no copy.
        let Some(region) = w.markets.get(w.nodes[node].region) else {
            return 0.0;
        };
        let m = region.cell().model();
        let mut food = 0.0;
        for (g, _) in w.goods.iter() {
            if let PreyClass::Food {
                per_item,
                herd: true,
            } = self.class_of_world(w, g)
            {
                let avail = m.stock(g) as f64 * share_milli as f64 / 1000.0;
                food += avail * f64::from(per_item) / Item::World(g).unit() as f64;
            }
        }
        food
    }

    /// Expected fullness from a patrol of a road: the caravans on it (its travel time over the
    /// caravan period, each way), the chance each is caught, and the edible share of what each
    /// carries, from the flows' current rates.
    fn road_food(&self, w: &World, e: usize, catch_milli: i64, share_milli: i64) -> f64 {
        let mut total = 0.0;
        let d = w.edges[e].d as f64;
        for dir in &w.edges[e].dirs {
            let period = dir.lattice.current_period().max(1) as f64;
            let passing = (d / period).max(1.0);
            let mut per_caravan = 0.0;
            for (g, p) in dir.goods.iter() {
                if let PreyClass::Food { per_item, .. } = self.class_of_world(w, *g) {
                    let cargo = p.current_rate() as f64 * period;
                    per_caravan += cargo * share_milli as f64 / 1000.0 * f64::from(per_item)
                        / Item::World(*g).unit() as f64;
                }
            }
            total += passing * catch_milli as f64 / 1000.0 * per_caravan;
        }
        total
    }

    fn target_name(&self, w: &World, t: Target) -> String {
        match t {
            Target::Road { edge, .. } => self.road_name(w, edge),
            Target::Village { node } => format!("village {}", w.nodes[node as usize].key),
            Target::Home { player, .. } => format!("the home of player {}", player.to_bits()),
        }
    }

    fn road_name(&self, w: &World, edge: u32) -> String {
        let e = &w.edges[edge as usize];
        format!("road {}-{}", w.nodes[e.a].key, w.nodes[e.b].key)
    }

    /// Moves a starving dragon to an empty roost of its species, if one is in reach (a
    /// migration). Returns whether it left.
    fn migrate(&mut self, w: &mut World, d: DragonId, at: Tick) -> bool {
        let x = &self.dragons[d];
        if x.fruitless < self.tun.fruitless_before_move {
            return false;
        }
        let Some(l) = self.vacant_lair(d, at) else {
            return false;
        };
        self.stats.migrations += 1;
        let name = self.lairs[l as usize].name.clone();
        self.note(at, "migrate", Some(d), format!("finds nothing to eat; migrates to {name}"));
        self.leave_lair(w, d, at);
        self.lairs[l as usize].plan += 1; // its respawn event is void: it is claimed
        self.lairs[l as usize].vacant_since = None;
        self.fly(w, d, Purpose::Claim(l), at);
        true
    }

    /// The nearest roost of `d`'s species with nobody in it and nobody on the way.
    fn vacant_lair(&self, d: DragonId, at: Tick) -> Option<LairId> {
        let x = &self.dragons[d];
        let (p, _) = x.position(&self.lairs, at);
        let range = f64::from(self.tun.species(x.species).grudge_range);
        self.lairs
            .iter()
            .enumerate()
            .filter(|(i, l)| {
                l.species == x.species
                    && l.occupants.is_empty()
                    && l.vacant_since.is_some()
                    && Some(*i as LairId) != x.lair
            })
            .map(|(i, l)| {
                (
                    flight::dist(
                        (i64::from(p.x), i64::from(p.z)),
                        (i64::from(l.pos.x), i64::from(l.pos.z)),
                    ),
                    i as LairId,
                )
            })
            .filter(|(dd, _)| *dd <= range)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, i)| i)
    }

    // ---- arriving, hunting, eating -----------------------------------------------------------------

    fn leg_end(&mut self, w: &mut World, infra: &mut dyn Infrastructure, d: DragonId, at: Tick) {
        let State::Flying(purpose) = self.dragons[d].state else {
            return;
        };
        match purpose {
            Purpose::Home => self.roost(w, d, at),
            Purpose::Claim(l) => {
                self.claim(w, d, l, at);
                self.roost(w, d, at);
            }
            Purpose::Hunt(t @ Target::Home { .. }) => {
                self.raid_home(w, infra, d, t, at);
            }
            Purpose::Hunt(t) => {
                let sp = self.tun.species(self.dragons[d].species).clone();
                let dwell = match t {
                    Target::Road { .. } => sp.patrol_ticks,
                    _ => sp.dwell_ticks,
                };
                let until = at + dwell;
                let plan = self.bump(d);
                let pos = self.target_pos(w, t);
                self.dragons.get_mut(d).expect("live").state = State::Hunting { target: t, until };
                self.book_leg(w, d, flight::stay(pos, at, until, BODY_RADIUS), sp.cruise_y);
                self.schedule(until, Ev::HuntEnd { d, plan });
            }
        }
    }

    fn hunt_end(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let State::Hunting { target, .. } = self.dragons[d].state else {
            return;
        };
        self.stats.hunts += 1;
        let (food, taken, victims, kind) = match target {
            Target::Road { edge, .. } => {
                // A patrol: everything on the road when it ends.
                let (f, t, v) = self.hunt_road(w, d, edge, 0, Tick::ZERO, Tick(u64::MAX), at);
                (f, t, v, PreyKind::Road)
            }
            Target::Village { node } => {
                let (f, t, v) = self.hunt_village(w, d, node as usize, at);
                (f, t, v, PreyKind::Village)
            }
            Target::Home { .. } => (0, Vec::new(), Vec::new(), PreyKind::Grudge),
        };
        self.feed(d, at, food, &victims);
        let name = self.target_name(w, target);
        self.record_hunt(at, d, kind, name, food, taken);
        self.after_hunt(w, d, at, food);
    }

    /// After a hunt: home if fed enough, else another target (a bout chains at most
    /// `bout_hunts`).
    fn after_hunt(&mut self, w: &mut World, d: DragonId, at: Tick, food: i32) {
        let x = self.dragons.get_mut(d).expect("live");
        if food > 0 {
            x.fruitless = 0;
        } else {
            x.fruitless = x.fruitless.saturating_add(1);
            self.stats.fruitless += 1;
        }
        let x = &self.dragons[d];
        let hungry = x.fullness_at(&self.tun, at) < self.tun.hunt_below;
        if hungry && x.bout < self.tun.bout_hunts {
            let chain = x.bout;
            self.decide(w, d, at, chain);
        } else {
            self.fly(w, d, Purpose::Home, at);
        }
    }

    fn feed(&mut self, d: DragonId, at: Tick, food: i32, victims: &[ActorId]) {
        let keep = self.tun.victims_kept;
        let tun = self.tun.clone();
        let x = self.dragons.get_mut(d).expect("live");
        x.eat(&tun, food, at);
        for v in victims {
            x.victims.retain(|o| o != v);
            x.victims.push(*v);
            if x.victims.len() > keep {
                x.victims.remove(0);
            }
        }
    }

    fn record_hunt(
        &mut self,
        at: Tick,
        d: DragonId,
        kind: PreyKind,
        target: String,
        food: i32,
        taken: Vec<(String, i64)>,
    ) {
        let season = self.tun.calendar.season_at(at);
        if food > 0 {
            self.prey_by_season[season.index()][kind.index()] += 1;
            self.food_by_season[season.index()][kind.index()] += i64::from(food);
        }
        if !self.log_enabled {
            return;
        }
        let dragon = self.dragons[d].name.clone();
        self.note(
            at,
            "prey",
            Some(d),
            format!(
                "{} at {target}: +{food} fullness{}",
                kind.name(),
                if taken.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", took {}",
                        taken
                            .iter()
                            .map(|(n, a)| format!("{n} {a}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ),
        );
        self.hunts.push(HuntRecord {
            tick: at.0,
            day: day_of(at),
            season,
            dragon,
            kind,
            target,
            food,
            taken,
        });
    }

    /// Takes goods into the ledger by class; returns fullness gained and the names taken.
    fn take_world(&mut self, w: &World, g: GoodId, amount: i64, out: &mut Vec<(String, i64)>) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let class = self.class_of_world(w, g);
        out.push((w.goods.get(g).name.clone(), amount));
        self.ledger.take(Item::World(g), amount, class)
    }

    fn hoard(&mut self, d: DragonId, item: Item, amount: i64) {
        if amount <= 0 {
            return;
        }
        // A dragon with no lair yet carries it to wherever it settles: the first lair it has, or
        // its species' first lair.
        let l = self.dragons[d]
            .lair
            .or_else(|| {
                self.lairs
                    .iter()
                    .position(|l| l.species == self.dragons[d].species)
                    .map(|i| i as LairId)
            })
            .unwrap_or(0);
        if let Some(lair) = self.lairs.get_mut(l as usize) {
            *lair.hoard.entry(item).or_default() += amount;
        }
    }

    /// Caravans passing the point `frac_milli` along `edge` during `[from, to]`, each caught
    /// with the species' chance (one counter-RNG draw keyed by the dragon and the caravan): the
    /// hazard on the flow, integrated over the hunt (SCALE.md §3). Caught ones become rows and
    /// lose a share of their cargo (rtsim-world's `prey_on_traveller`).
    #[allow(clippy::too_many_arguments)]
    fn hunt_road(
        &mut self,
        w: &mut World,
        d: DragonId,
        edge: u32,
        frac_milli: u32,
        from: Tick,
        to: Tick,
        at: Tick,
    ) -> (i32, Vec<(String, i64)>, Vec<ActorId>) {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let key = StreamKey::new(self.seed, SYS_HUNT, x.seed);
        let mut food = 0;
        let mut taken = Vec::new();
        let mut victims = Vec::new();
        let _ = at;
        for dir in 0..2u8 {
            let f = if dir == 0 {
                u64::from(frac_milli)
            } else {
                1000 - u64::from(frac_milli)
            };
            for t in w.travellers(edge, dir) {
                if t.cargo.values().all(|&a| a <= 0) {
                    continue;
                }
                // A crossing catches what passes its point in its window; a patrol (the whole
                // window) catches everything on the road.
                if (from, to) != (Tick::ZERO, Tick(u64::MAX)) {
                    let pass = t.departed + (t.arrives.0.saturating_sub(t.departed.0)) * f / 1000;
                    if pass < from || pass > to {
                        continue;
                    }
                }
                let k = t.key;
                let slot = fnv1a64(
                    &[
                        u64::from(k.edge).to_le_bytes(),
                        u64::from(k.dir).to_le_bytes(),
                        k.departure.to_le_bytes(),
                    ]
                    .concat(),
                );
                if key.slot(slot).below(1000) >= sp.catch_milli.clamp(0, 1000) as u64 {
                    continue;
                }
                let Some((_, lost)) = w.prey_on_traveller(k, sp.caravan_share_milli) else {
                    continue;
                };
                self.stats.caravans_attacked += 1;
                for (g, a) in lost {
                    let gained = self.take_world(w, g, a, &mut taken);
                    if matches!(self.class_of_world(w, g), PreyClass::Valuable) {
                        self.hoard(d, Item::World(g), a);
                    }
                    food += gained;
                }
                let home = w.nodes[t.from].actor;
                if !victims.contains(&home) {
                    victims.push(home);
                }
            }
        }
        (food, taken, victims)
    }

    /// A raid on a village's livestock: up to the species' share of each herd good, no more than
    /// it can eat. The village sees it (an assault on it by the dragon).
    fn hunt_village(
        &mut self,
        w: &mut World,
        d: DragonId,
        node: usize,
        at: Tick,
    ) -> (i32, Vec<(String, i64)>, Vec<ActorId>) {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let mut need = (self.tun.full - x.fullness_at(&self.tun, at)).min(sp.appetite);
        let mut food = 0;
        let mut taken = Vec::new();
        if !w.nodes[node].kind.is_storehouse() {
            return (0, taken, Vec::new());
        }
        let m = w.market(node);
        let goods: Vec<GoodId> = w.goods.ids().collect();
        for g in goods {
            if need <= 0 {
                break;
            }
            let PreyClass::Food {
                per_item,
                herd: true,
            } = self.class_of_world(w, g)
            else {
                continue;
            };
            let stock = m.stock(g);
            let unit = Item::World(g).unit();
            let cap = stock * sp.village_share_milli / 1000;
            let per = i64::from(per_item.max(1));
            let want = (i64::from(need) * unit + per - 1) / per;
            let t = w.take_stock(node, g, cap.min(want), at);
            let gained = self.take_world(w, g, t, &mut taken);
            food += gained;
            need -= gained;
        }
        if food > 0 {
            self.stats.village_raids += 1;
            let village = w.nodes[node].actor;
            let (p, y) = (w.nodes[node].pos, 64);
            let id = w.knowledge.record(Report {
                who: self.dragons[d].actor,
                whom: village,
                kind: ReportKind::Assault,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: place(p, y),
            });
            w.knowledge.witness(village, id, at);
            return (food, taken, vec![village]);
        }
        (food, taken, Vec::new())
    }

    /// A grudge raid: the districts at the hated player's home, through the industry threat
    /// interface. Watched districts fight for real: the dragon materializes there instead (P28).
    fn raid_home(&mut self, w: &mut World, infra: &mut dyn Infrastructure, d: DragonId, t: Target, at: Tick) {
        let Target::Home { player, x, z } = t else {
            return;
        };
        let x0 = &self.dragons[d];
        let sp = self.tun.species(x0.species).clone();
        let stage = x0.stage(&sp, at);
        let sites = infra.districts_near(0, i64::from(x), i64::from(z), HOME_RADIUS);
        let mut food = 0;
        let mut taken = Vec::new();
        let mut any = false;
        for site in sites.into_iter().take(self.tun.raid_districts) {
            if site.watched {
                // The player is there: a real fight. Materialize at the district.
                let p = site.pos;
                self.materialize(w, d, player, true, Some((p, sp.cruise_y)), at);
                return;
            }
            let passes = sp.passes[stage.index()].max(1);
            let threat = Threat {
                id: fnv1a64(
                    &[
                        self.dragons[d].seed.to_le_bytes(),
                        at.0.to_le_bytes(),
                        site.id.0.to_le_bytes(),
                    ]
                    .concat(),
                ),
                source: ThreatSource::Dragon,
                class: ThreatClass::Flying,
                count: passes,
                strength: f64::from(passes) * sp.strength_per_pass[stage.index()],
                target: ThreatTarget::District(site.id),
                at,
            };
            let Some(o) = infra.raid(&threat) else {
                continue;
            };
            any = true;
            self.stats.district_raids += 1;
            let mut gained = 0;
            for (g, n) in o.stolen.iter() {
                let name = infra.good_name(g).unwrap_or_else(|| format!("good {}", g.0));
                let class = self.tun.prey.class(&name, false);
                gained += self.ledger.take(Item::Industry(g), n, class);
                if matches!(class, PreyClass::Valuable) {
                    self.hoard(d, Item::Industry(g), n);
                }
                taken.push((name, n));
            }
            food += gained;
            // Defences that stopped passes wound it; an abstract raid never kills a dragon (a
            // death needs a real fight), so it keeps at least a tenth of its health.
            let max = self.dragons[d].max_health(&sp, at);
            let h = self.dragons[d].health_at(&sp, at);
            let wound = max * (self.tun.raid_wound_milli as f32 / 1000.0) * (1.0 - o.survived as f32);
            self.dragons
                .get_mut(d)
                .expect("live")
                .set_health((h - wound).max(max * 0.1), at);
            self.note(
                at,
                "raid",
                Some(d),
                format!(
                    "raids district {} (unwatched): survived {:.2}{}{}, {} blocks destroyed{}",
                    site.id.0,
                    o.survived,
                    o.floor.map_or(String::new(), |f| format!(", floored by an observed fight at {f:.2}")),
                    if o.untested { ", untested defences" } else { "" },
                    o.destroyed_blocks,
                    o.skipped.map_or(String::new(), |s| format!(", skipped: {s}"))
                ),
            );
        }
        self.feed(d, at, food, &[]);
        let name = format!("the home of player {}", player.to_bits());
        self.record_hunt(at, d, PreyKind::Grudge, name, food, taken);
        if !any {
            self.dragons.get_mut(d).expect("live").fruitless += 1;
        }
        self.fly(w, d, Purpose::Home, at);
    }

    // ---- the boundary: materializing and what comes back -----------------------------------------

    fn seen(&mut self, w: &mut World, d: DragonId, player: ActorId, m: Meeting, at: Tick) {
        if !w.legs.is_meeting_live(&m) || !self.dragons.contains(d) {
            self.stats.seen_stale += 1;
            return;
        }
        if self.dragons[d].is_embodied() {
            return;
        }
        self.stats.seen += 1;
        let grudge = w.knowledge.sentiment(self.dragons[d].actor, player, at) <= self.tun.grudge_below;
        // It harasses when it hunts here: at its prey, or flying to prey near the player.
        let near = |t: Target| {
            let tp = self.target_pos(w, t);
            self.homes.last_seen(player).is_some_and(|(p, _)| {
                flight::dist(
                    (i64::from(p.x), i64::from(p.z)),
                    (i64::from(tp.x), i64::from(tp.z)),
                ) <= f64::from(2 * self.tun.view_radius)
            })
        };
        let harass = grudge
            || match self.dragons[d].state {
                State::Hunting { .. } => true,
                State::Flying(Purpose::Hunt(Target::Home { player: p, .. })) => p == player,
                State::Flying(Purpose::Hunt(t)) => near(t),
                _ => false,
            };
        self.materialize(w, d, player, harass, None, at);
    }

    /// Makes the dragon a Minecraft entity near `player`: the plugin spawns it with its NBT from
    /// engine state. A harassing dragon books a departure for when it gets bored (C24); one
    /// passing through books its time to cross the view; one at home stays while it is seen.
    fn materialize(
        &mut self,
        w: &mut World,
        d: DragonId,
        player: ActorId,
        harass: bool,
        at_pos: Option<(Pos, i32)>,
        at: Tick,
    ) {
        let (pos, y) = at_pos.unwrap_or_else(|| self.dragons[d].position(&self.lairs, at));
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let purpose = match x.state {
            State::Flying(p) => p,
            State::Hunting { target, .. } => Purpose::Hunt(target),
            _ => Purpose::Home,
        };
        let home = matches!(x.state, State::Roosting);
        let grudge = w.knowledge.sentiment(x.actor, player, at) <= self.tun.grudge_below;
        let temper = x.temperament();
        let starving = x.fullness_at(&self.tun, at) <= self.tun.starving;
        let depart_at = if home {
            Tick(u64::MAX)
        } else if harass && starving {
            // Hunger brings the departure forward (C24): a starving dragon only looks in.
            at + (sp.visit_ticks / 10).max(1)
        } else if harass {
            at + temper.weight(Trait::Aggressive, sp.visit_ticks.min(i32::MAX as u64) as i32).max(1) as u64
        } else {
            at + flight::flight_ticks(f64::from(2 * self.tun.view_radius), sp.speed)
        };
        let epoch = x.epoch + 1;
        let lair = x.lair.map(|l| &self.lairs[l as usize]);
        let (target, _) = if harass {
            match self.homes.last_seen(player) {
                Some((p, _)) => (p, y),
                None => (pos, y),
            }
        } else {
            self.purpose_pos(w, d, purpose)
        };
        let mut flags = boundary::MAT_CLAMP_ABOVE_SURFACE;
        if lair.is_some() {
            flags |= boundary::MAT_HAS_HOME;
        }
        if home {
            flags |= boundary::MAT_SLEEPING;
        }
        if grudge {
            flags |= boundary::MAT_HOSTILE;
        }
        let cmd = boundary::Materialize {
            agent: agent_of(d),
            epoch,
            flags,
            species: x.species,
            dimension: u32::from(pos.frame),
            age_ticks: x.age(at).min(i32::MAX as u64) as i32,
            pos: (f64::from(pos.x), f64::from(y), f64::from(pos.z)),
            hunger: x.fullness_at(&self.tun, at),
            gender: x.gender,
            variant: u32::from(x.variant),
            health: x.health_at(&sp, at),
            home: lair.map_or((0, 0, 0), |l| (l.pos.x, l.y, l.pos.z)),
            target: (f64::from(target.x), f64::from(sp.cruise_y), f64::from(target.z)),
            target_player: if grudge { self.uuid_of(player) } else { (0, 0) },
        };
        self.outbox.push(Cmd::Materialize(cmd));
        self.stats.materialized += 1;
        let health = x.health_at(&sp, at);
        let plan = self.bump(d);
        {
            let x = self.dragons.get_mut(d).expect("live");
            x.epoch = epoch;
            x.set_health(health, at);
            x.state = State::Embodied {
                epoch,
                since: at,
                depart_at,
                purpose,
                harass,
            };
            x.rest = Some((pos, y));
            if grudge {
                x.last_grudge = Some(at);
            }
        }
        self.cancel_leg(w, d);
        let what = format!(
            "enters view of player {}: materializes ({}{})",
            player.to_bits(),
            if home {
                "at home"
            } else if harass {
                "to harass"
            } else {
                "passing"
            },
            if grudge { ", hostile: a grudge" } else { "" }
        );
        self.note(at, "materialize", Some(d), what);
        if depart_at.0 != u64::MAX {
            let reason = if harass && starving {
                2
            } else if harass {
                1
            } else {
                4
            };
            self.schedule(depart_at, Ev::Depart { d, plan, reason });
        }
    }

    /// Tells the plugin to fly the dragon off (C24: it got bored, or wounds or hunger brought
    /// the departure forward). The plugin captures it when it is out of view.
    fn depart(&mut self, d: DragonId, reason: u32, at: Tick) {
        let x = &self.dragons[d];
        let State::Embodied {
            epoch, purpose, since, ..
        } = x.state
        else {
            return;
        };
        let (to, _) = match x.lair {
            Some(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
            None => x.position(&self.lairs, at),
        };
        let cruise = self.tun.species(x.species).cruise_y;
        self.outbox.push(Cmd::Depart(boundary::Depart {
            agent: agent_of(d),
            epoch,
            reason,
            timeout_ticks: DEPART_TIMEOUT,
            target: (f64::from(to.x), f64::from(cruise), f64::from(to.z)),
        }));
        self.stats.departures += 1;
        if reason == 1 {
            self.stats.bored += 1;
        }
        let why = ["", "bored", "hungry", "wounded", "passing through", "recalled"]
            .get(reason as usize)
            .copied()
            .unwrap_or("?");
        self.note(
            at,
            "depart",
            Some(d),
            format!("leaves ({why}) after {:.2} days in view", day_of(Tick(at.0 - since.0))),
        );
        self.bump(d);
        self.dragons.get_mut(d).expect("live").state = State::Leaving { epoch, purpose };
    }

    fn embodied(&self, agent: u64, epoch: u32) -> Option<DragonId> {
        let d = dragon_of(agent)?;
        let x = self.dragons.get(d)?;
        match x.state {
            State::Embodied { epoch: e, .. } | State::Leaving { epoch: e, .. } if e == epoch => Some(d),
            _ => None,
        }
    }

    /// An observation from the plugin, at `at`.
    pub fn ingest(&mut self, w: &mut World, infra: &mut dyn Infrastructure, o: &Obs, at: Tick) {
        let _ = infra;
        self.run_until(w, infra, at);
        match o {
            Obs::LairFound(l) => {
                let pos = Pos::new(l.dimension as u16, l.home.0, l.home.2);
                let name = format!("{} {}", boundary::SpeciesDict::structure_name(l.species, l.kind), l.lair);
                self.add_lair(w, l.lair, &name, l.species, l.kind, pos, l.home.1, at);
            }
            Obs::Untagged(u) => self.untagged(w, u, at),
            Obs::Materialized(m) => {
                if let Some(d) = self.embodied(m.agent, m.epoch) {
                    self.by_entity.insert(m.entity, d);
                }
            }
            Obs::SpawnRejected(r) => {
                if let Some(d) = self.embodied(r.agent, r.epoch) {
                    self.resume(w, d, at, false);
                }
            }
            Obs::State(s) => {
                let Some(d) = self.embodied(s.agent, s.epoch) else {
                    return;
                };
                let starving = self.tun.starving;
                let x = self.dragons.get_mut(d).expect("live");
                x.set_health(s.health, at);
                x.set_fullness(s.hunger, at);
                x.rest = Some((Pos::new(s.dimension as u16, s.pos.0 as i32, s.pos.2 as i32), s.pos.1 as i32));
                if s.hunger <= starving && matches!(x.state, State::Embodied { .. }) {
                    self.depart(d, 2, at);
                }
            }
            Obs::Hurt(h) => self.hurt(w, h, at),
            Obs::Died(dd) => {
                if let Some(d) = self.embodied(dd.agent, dd.epoch) {
                    let killer = (dd.killer_kind == AttackerKind::Player).then(|| self.player(w, dd.killer));
                    let pos = (Pos::new(dd.dimension as u16, dd.pos.0 as i32, dd.pos.2 as i32), dd.pos.1 as i32);
                    self.kill(w, d, killer, pos, at);
                }
            }
            Obs::Dematerialized(m) => {
                let Some(d) = self.embodied(m.agent, m.epoch) else {
                    return;
                };
                {
                    let x = self.dragons.get_mut(d).expect("live");
                    x.set_health(m.health, at);
                    x.set_fullness(m.hunger, at);
                    x.rest = Some((Pos::new(m.dimension as u16, m.pos.0 as i32, m.pos.2 as i32), m.pos.1 as i32));
                }
                self.by_entity.retain(|_, v| *v != d);
                let leaving = matches!(self.dragons[d].state, State::Leaving { .. });
                let what = format!(
                    "captured (health {:.0}, hunger {})",
                    m.health, m.hunger
                );
                self.note(at, "capture", Some(d), what);
                self.resume(w, d, at, leaving);
            }
            Obs::HoardRobbed(h) => self.robbed(w, h, at),
        }
    }

    /// Back to abstract after a visit: a dragon that left (bored, hungry, wounded) flies home; one
    /// that only passed carries on with what it was doing.
    fn resume(&mut self, w: &mut World, d: DragonId, at: Tick, left: bool) {
        let purpose = match self.dragons[d].state {
            State::Embodied { purpose, .. } | State::Leaving { purpose, .. } => purpose,
            _ => Purpose::Home,
        };
        let purpose = match purpose {
            Purpose::Claim(l) => Purpose::Claim(l),
            Purpose::Hunt(t) if !left && !matches!(t, Target::Home { .. }) => Purpose::Hunt(t),
            _ => Purpose::Home,
        };
        self.dragons.get_mut(d).expect("live").state = State::Flying(purpose);
        let at_home = match (purpose, self.dragons[d].lair) {
            (Purpose::Home, Some(l)) => {
                let (p, _) = self.dragons[d].position(&self.lairs, at);
                let lp = self.lairs[l as usize].pos;
                flight::dist((i64::from(p.x), i64::from(p.z)), (i64::from(lp.x), i64::from(lp.z))) < 32.0
            }
            _ => false,
        };
        if at_home {
            self.roost(w, d, at);
        } else {
            self.fly(w, d, purpose, at);
        }
    }

    /// A player (or anything) hurt an embodied dragon. A player's wound is a report the dragon
    /// witnesses, and its lair with it (C3: dragons remember slights); it brings the departure
    /// forward (C24).
    fn hurt(&mut self, w: &mut World, h: &boundary::Hurt, at: Tick) {
        let Some(d) = self.embodied(h.agent, h.epoch) else {
            return;
        };
        self.stats.hurt += 1;
        let sp = self.tun.species(self.dragons[d].species).clone();
        let max = self.dragons[d].max_health(&sp, at).max(1.0);
        let share = f64::from(h.amount) / f64::from(max);
        self.dragons.get_mut(d).expect("live").set_health(h.health_after, at);
        if h.attacker_kind == AttackerKind::Player {
            let p = self.player(w, h.attacker);
            let unit = self.tun.wound_unit_milli as f64 / 1000.0;
            let severity = (f64::from(SEVERITY_UNIT) * (1.0 + share / unit)).clamp(1.0, 255.0) as u8;
            let (pos, y) = self.dragons[d].position(&self.lairs, at);
            let id = w.knowledge.record(Report {
                who: p,
                whom: self.dragons[d].actor,
                kind: ReportKind::Assault,
                severity,
                flags: 0,
                at,
                place: place(pos, y),
            });
            w.knowledge.witness(self.dragons[d].actor, id, at);
            if let Some(l) = self.dragons[d].lair {
                let la = self.lairs[l as usize].actor;
                w.knowledge.witness(la, id, at);
            }
            let s = w.knowledge.sentiment(self.dragons[d].actor, p, at);
            self.note(
                at,
                "hurt",
                Some(d),
                format!("wounded by player {} for {:.0} ({:.0}% of max): sentiment now {s}", p.to_bits(), h.amount, share * 100.0),
            );
        }
        // Wounds bring the departure forward.
        let State::Embodied {
            epoch,
            since,
            depart_at,
            purpose,
            harass,
        } = self.dragons[d].state
        else {
            return;
        };
        if depart_at.0 == u64::MAX {
            return;
        }
        let k = self.tun.wound_shortens_milli as f64 / 1000.0;
        let left = depart_at.0.saturating_sub(at.0) as f64 * (1.0 - k * share).max(0.0);
        let new = at + left as u64;
        if new >= depart_at {
            return;
        }
        if left < 1.0 {
            self.depart(d, 3, at);
            return;
        }
        let plan = self.bump(d);
        self.dragons.get_mut(d).expect("live").state = State::Embodied {
            epoch,
            since,
            depart_at: new,
            purpose,
            harass,
        };
        self.schedule(new, Ev::Depart { d, plan, reason: 3 });
    }

    /// A dragon died. Killed by a player: its kin and lair witness the killing (their grudge), and
    /// the settlements it preyed on think better of the slayer (a positive report the caravans
    /// carry, C24: rescues count). Its strongest memories go to its heir (C24).
    fn kill(&mut self, w: &mut World, d: DragonId, killer: Option<ActorId>, at_pos: (Pos, i32), at: Tick) {
        self.stats.deaths_violent += 1;
        let x = self.dragons[d].clone();
        if let Some(p) = killer {
            let id = w.knowledge.record(Report {
                who: p,
                whom: x.actor,
                kind: ReportKind::Killing,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: place(at_pos.0, at_pos.1),
            });
            let mut kin: Vec<ActorId> = Vec::new();
            for k in x.children.iter().chain(x.mate.iter()).chain(x.parent.iter()) {
                if let Some(o) = self.dragons.get(*k) {
                    kin.push(o.actor);
                }
            }
            if let Some(l) = x.lair {
                kin.push(self.lairs[l as usize].actor);
            }
            for k in &kin {
                w.knowledge.witness(*k, id, at);
            }
            let mut thanks = Vec::new();
            for v in &x.victims {
                let rid = w.knowledge.record(Report {
                    who: p,
                    whom: *v,
                    kind: ReportKind::Rescue,
                    severity: SEVERITY_UNIT,
                    flags: 0,
                    at,
                    place: place(at_pos.0, at_pos.1),
                });
                w.knowledge.witness(*v, rid, at);
                let ext = w.knowledge.actors().external(*v).unwrap_or(0);
                thanks.push(format!("village {ext} {:+}", w.knowledge.sentiment(*v, p, at)));
            }
            self.note(
                at,
                "death",
                Some(d),
                format!(
                    "killed by player {}: {} kin and its lair remember; the settlements it preyed on think better of the slayer ({})",
                    p.to_bits(),
                    kin.len().saturating_sub(x.lair.is_some() as usize),
                    thanks.join(", ")
                ),
            );
        } else {
            self.note(at, "death", Some(d), "killed".into());
        }
        let heir = w.knowledge.die(x.actor, at);
        if let Some(h) = heir {
            let who = self
                .dragons
                .iter()
                .find(|(_, o)| o.actor == h)
                .map(|(_, o)| o.name.clone())
                .unwrap_or_else(|| "its lair".to_string());
            self.note(at, "heir", Some(d), format!("its grudges pass to {who}"));
        }
        self.remove(w, d, at);
    }

    /// Takes a dead dragon out: its lair may stand empty (the respawn clock starts), its kin
    /// lose a link and re-point their heirs.
    fn remove(&mut self, w: &mut World, d: DragonId, at: Tick) {
        self.cancel_leg(w, d);
        self.leave_lair(w, d, at);
        let Some(x) = self.dragons.remove(d) else {
            return;
        };
        self.by_entity.retain(|_, v| *v != d);
        let mut touched = Vec::new();
        if let Some(m) = x.mate
            && let Some(o) = self.dragons.get_mut(m)
        {
            o.mate = None;
            touched.push(m);
        }
        for c in &x.children {
            if let Some(o) = self.dragons.get_mut(*c) {
                o.parent = None;
            }
        }
        if let Some(p) = x.parent
            && let Some(o) = self.dragons.get_mut(p)
        {
            o.children.retain(|c| *c != d);
            touched.push(p);
        }
        for t in touched {
            self.refresh_heir(w, t);
        }
    }

    fn leave_lair(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(l) = self.dragons.get(d).and_then(|x| x.lair) else {
            return;
        };
        // Where it is stays where it is: a dragon at its lair is at the lair.
        let here = self.dragons[d].position(&self.lairs, at);
        let lair = &mut self.lairs[l as usize];
        lair.occupants.retain(|o| *o != d);
        if let Some(x) = self.dragons.get_mut(d) {
            x.lair = None;
            if x.rest.is_none() && !matches!(x.state, State::Flying(_) | State::Hunting { .. }) {
                x.rest = Some(here);
            }
        }
        if lair.occupants.is_empty() {
            lair.vacant_since = Some(at);
            lair.plan += 1;
            let plan = lair.plan;
            let delay = self.tun.respawn_delay_days * TICKS_PER_DAY;
            self.schedule(at + delay, Ev::Respawn { lair: l, plan });
        }
        let _ = w;
    }

    /// Moves into a lair. A newcomer to an empty lair takes over the lair's strongest feelings
    /// (C24: the lair is the heir of last resort).
    fn occupy(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick, inherit: bool) {
        let lair = &mut self.lairs[l as usize];
        let was_empty = lair.occupants.is_empty();
        if !lair.occupants.contains(&d) {
            lair.occupants.push(d);
        }
        lair.vacant_since = None;
        lair.plan += 1;
        let la = lair.actor;
        self.dragons.get_mut(d).expect("live").lair = Some(l);
        if inherit && was_empty {
            let mut feelings = w.knowledge.sentiment_snapshot(la, at);
            feelings.sort_by_key(|(t, v)| (std::cmp::Reverse(i32::from(*v).abs()), *t));
            let actor = self.dragons[d].actor;
            for (t, v) in feelings.into_iter().take(self.tun.lair_feelings) {
                w.knowledge.adjust(actor, t, i16::from(v), at);
            }
        }
        self.refresh_heir(w, d);
    }

    /// Arriving at a roost to claim it: an empty one is taken (with its grudges); an occupied
    /// one is shared, and an unmated adult of the other sex there becomes its mate.
    fn claim(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick) {
        self.stats.claims += 1;
        let occupants = self.lairs[l as usize].occupants.clone();
        let name = self.lairs[l as usize].name.clone();
        self.occupy(w, d, l, at, occupants.is_empty());
        let me = &self.dragons[d];
        let sp = self.tun.species(me.species).clone();
        let partner = occupants.into_iter().find(|o| {
            self.dragons.get(*o).is_some_and(|x| {
                x.mate.is_none()
                    && x.gender != me.gender
                    && x.species == me.species
                    && x.stage(&sp, at).0 >= 3
                    && me.stage(&sp, at).0 >= 3
            })
        });
        self.note(at, "claim", Some(d), format!("claims {name}"));
        if let Some(o) = partner {
            self.pair(w, d, o, at);
            self.note(at, "pair", Some(d), format!("pairs with {} at {name}", self.dragons[o].name));
        }
    }

    // ---- population -----------------------------------------------------------------------------

    fn breed(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let Some(m) = x.mate.filter(|m| self.dragons.contains(*m)) else {
            return;
        };
        let sp = self.tun.species(x.species).clone();
        let (Some(l), true) = (x.lair, self.dragons[m].lair == x.lair) else {
            let t = at + sp.breed_days * TICKS_PER_DAY;
            self.schedule(t, Ev::Breed { d });
            return;
        };
        if !x.stage(&sp, at).is_mature() || !self.dragons[m].stage(&sp, at).is_mature() {
            let t = self.next_clutch(d, at);
            self.schedule(t, Ev::Breed { d });
            return;
        }
        let mut rng = StreamKey::new(self.seed, SYS_BREED, x.seed).slot(at.0);
        let gender = if rng.below(2) == 0 { Gender::Female } else { Gender::Male };
        let variant = rng.below(4) as u8;
        let name = format!("{}'s young {}", x.name, self.stats.births + 1);
        let species = x.species;
        let child = self.add_dragon(
            w,
            NewDragon {
                name,
                species,
                gender,
                variant,
                age_ticks: 0,
                lair: Some(l),
                fullness: 50,
                health: None,
                seed: None,
                at: None,
            },
            at,
        );
        self.stats.births += 1;
        self.set_parent(w, child, d, at);
        self.note(at, "birth", Some(d), format!("hatches a young one, {}", self.dragons[child].name));
        self.schedule(at + sp.breed_days * TICKS_PER_DAY, Ev::Breed { d });
    }

    fn disperse(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        if x.parent.is_none() || x.is_embodied() || !matches!(x.state, State::Roosting) {
            if x.parent.is_some() {
                self.schedule(at + 2 * TICKS_PER_DAY, Ev::Disperse { d });
            }
            return;
        }
        match self.vacant_lair(d, at) {
            Some(l) => {
                let name = self.lairs[l as usize].name.clone();
                self.note(at, "disperse", Some(d), format!("leaves its parents for {name}"));
                self.leave_lair(w, d, at);
                self.lairs[l as usize].plan += 1;
                self.lairs[l as usize].vacant_since = None;
                if let Some(p) = self.dragons[d].parent
                    && let Some(o) = self.dragons.get_mut(p)
                {
                    let _ = o;
                }
                self.fly(w, d, Purpose::Claim(l), at);
            }
            None => self.schedule(at + 10 * TICKS_PER_DAY, Ev::Disperse { d }),
        }
    }

    /// The respawn rule (P13's recommendation, the TUNABLE delay): a roost empty for the delay
    /// draws a young dragon of its species from afar. Its arrival is a flight like any other, so
    /// a player can watch it come.
    fn respawn(&mut self, w: &mut World, l: LairId, plan: u32, at: Tick) {
        let lair = &self.lairs[l as usize];
        if lair.plan != plan || !lair.occupants.is_empty() || lair.vacant_since.is_none() {
            return;
        }
        self.stats.respawns += 1;
        let species = lair.species;
        let mut rng = StreamKey::new(self.seed, SYS_BREED, lair.key).slot(at.0);
        let angle = rng.next_f64() * std::f64::consts::TAU;
        let r = f64::from(self.tun.respawn_from_blocks);
        let from = Pos::new(
            lair.pos.frame,
            lair.pos.x + (r * angle.cos()) as i32,
            lair.pos.z + (r * angle.sin()) as i32,
        );
        let gender = if rng.below(2) == 0 { Gender::Female } else { Gender::Male };
        let variant = rng.below(4) as u8;
        let cruise = self.tun.species(species).cruise_y;
        let name = format!("{} newcomer", lair.name);
        let d = self.add_dragon(
            w,
            NewDragon {
                name,
                species,
                gender,
                variant,
                age_ticks: self.tun.respawn_age_days * TICKS_PER_DAY,
                lair: None,
                fullness: 50,
                health: None,
                seed: None,
                at: Some((from, cruise)),
            },
            at,
        );
        let lair = &mut self.lairs[l as usize];
        lair.plan += 1;
        lair.vacant_since = None;
        let name = lair.name.clone();
        self.note(at, "respawn", Some(d), format!("{name} stood empty: a young dragon flies in to claim it"));
        self.fly(w, d, Purpose::Claim(l), at);
    }

    /// An untagged dragon appeared in the world (P13's recommendation: the sim owns every
    /// dragon). Tamed ones are the player's (A10) and are left alone; a lair's own dragon from
    /// world generation is adopted as its occupant; spawn eggs and commands are adopted as
    /// wanderers; natural and spawner dragons (a rule P13 removes) are discarded.
    fn untagged(&mut self, w: &mut World, u: &boundary::Untagged, at: Tick) {
        if u.flags & boundary::UNTAGGED_TAMED != 0 || u.spawn == SpawnKind::Hatched {
            return;
        }
        if self.by_entity.contains_key(&u.entity) {
            return;
        }
        let lair = (u.lair != 0).then(|| self.lair_of_key(u.lair)).flatten();
        let discard = match u.spawn {
            SpawnKind::Natural | SpawnKind::Spawner => Some(1),
            SpawnKind::Structure if lair.is_some_and(|l| !self.lairs[l as usize].occupants.is_empty()) => Some(2),
            _ => None,
        };
        if let Some(reason) = discard {
            self.outbox.push(Cmd::Discard(boundary::Discard {
                entity: u.entity,
                reason,
            }));
            self.stats.discarded += 1;
            return;
        }
        let pos = Pos::new(u.dimension as u16, u.pos.0 as i32, u.pos.2 as i32);
        let d = self.add_dragon(
            w,
            NewDragon {
                name: format!("{} dragon {:x}", u.species.name(), u.entity.1 & 0xffff),
                species: u.species,
                gender: u.gender,
                variant: (u.variant % 4) as u8,
                age_ticks: u.age_ticks.max(0) as u64,
                lair,
                fullness: u.hunger,
                health: None,
                seed: Some(fnv1a64(&[u.entity.0.to_le_bytes(), u.entity.1.to_le_bytes()].concat())),
                at: Some((pos, u.pos.1 as i32)),
            },
            at,
        );
        self.stats.adopted += 1;
        // It is a live entity now: adopt it as embodied, so the plugin keeps it until it leaves
        // view and then hands it back.
        let epoch = 1;
        let home = lair.map_or((0, 0, 0), |l| {
            let l = &self.lairs[l as usize];
            (l.pos.x, l.y, l.pos.z)
        });
        self.outbox.push(Cmd::Adopt(boundary::Adopt {
            entity: u.entity,
            agent: agent_of(d),
            epoch,
            flags: lair.is_some() as u32,
            home,
        }));
        self.by_entity.insert(u.entity, d);
        let plan = self.bump(d);
        let _ = plan;
        self.cancel_leg(w, d);
        let x = self.dragons.get_mut(d).expect("live");
        x.epoch = epoch;
        x.state = State::Embodied {
            epoch,
            since: at,
            depart_at: Tick(u64::MAX),
            purpose: Purpose::Home,
            harass: false,
        };
    }

    /// A player robbed a lair's hoard: what the engine's hoard held comes out (the rest was the
    /// structure's own treasure, never in the ledger), and the lair and its dragons witness the
    /// robbery: a grudge (M1's egg theft would be the same report, hooks UNVERIFIED).
    fn robbed(&mut self, w: &mut World, h: &boundary::HoardRobbed, at: Tick) {
        let Some(l) = self.lair_of_key(h.lair) else {
            return;
        };
        self.stats.hoards_robbed += 1;
        let p = self.player(w, h.player);
        let mut count = 0i64;
        for &(id, n) in &h.items {
            count += i64::from(n.max(0));
            let Some(item) = self.items.get(&id).copied() else {
                continue;
            };
            let units = i64::from(n.max(0)) * item.unit();
            let lair = &mut self.lairs[l as usize];
            let held = lair.hoard.get(&item).copied().unwrap_or(0);
            let take = units.min(held);
            if take > 0 {
                *lair.hoard.get_mut(&item).expect("held") -= take;
                self.ledger.rob(item, take);
            }
        }
        let lair = &self.lairs[l as usize];
        let severity = (i64::from(SEVERITY_UNIT) * (1 + count / 64)).clamp(1, 255) as u8;
        let id = w.knowledge.record(Report {
            who: p,
            whom: lair.actor,
            kind: ReportKind::Robbery,
            severity,
            flags: 0,
            at,
            place: place(lair.pos, lair.y),
        });
        let mut who = vec![lair.actor];
        for o in &lair.occupants {
            who.push(self.dragons[*o].actor);
        }
        for a in who {
            w.knowledge.witness(a, id, at);
        }
        let name = lair.name.clone();
        let feelings: Vec<String> = lair
            .occupants
            .iter()
            .map(|o| {
                let x = &self.dragons[*o];
                format!("{} {}", x.name, w.knowledge.sentiment(x.actor, p, at))
            })
            .collect();
        self.note(
            at,
            "robbed",
            None,
            format!(
                "player {} robs the hoard of {name} ({count} items); its dragons' feelings: {}",
                p.to_bits(),
                feelings.join(", ")
            ),
        );
    }

    // ---- checks ----------------------------------------------------------------------------------

    /// Conservation with predation (PLAN.md §8.4, §9): the world balances on its own (what
    /// dragons took is destroyed in its books); everything the world counts as destroyed is what
    /// this ledger says dragons took; taken = eaten + burnt + hoarded per good; hoards hold
    /// hoarded − robbed. `industry_stolen`, when given, must equal what dragons took from
    /// districts (true when dragons are the only threat).
    pub fn check(
        &self,
        w: &World,
        industry_stolen: Option<&rtsim_industry::ids::GoodVec>,
    ) -> Result<(), String> {
        let at = w.now();
        w.check_conserved(at).map_err(|e| format!("world: {e}"))?;
        self.ledger.check_split()?;
        let destroyed = w.destroyed_total(at);
        let taken = self.ledger.total_world(&self.ledger.taken);
        if destroyed != taken {
            return Err(format!(
                "the world counts {destroyed:?} destroyed but dragons took {taken:?}"
            ));
        }
        let mut held: BTreeMap<Item, i128> = BTreeMap::new();
        for l in &self.lairs {
            for (k, v) in &l.hoard {
                *held.entry(*k).or_default() += i128::from(*v);
            }
        }
        held.retain(|_, v| *v != 0);
        if held != self.ledger.in_hoards() {
            return Err(format!(
                "hoards hold {held:?} but the ledger says {:?}",
                self.ledger.in_hoards()
            ));
        }
        if let Some(stolen) = industry_stolen {
            let mine = self.ledger.total_industry(&self.ledger.taken);
            let theirs: BTreeMap<_, i128> = stolen
                .iter()
                .filter(|(_, n)| *n != 0)
                .map(|(g, n)| (g, i128::from(n)))
                .collect();
            if mine != theirs {
                return Err(format!(
                    "the industry engine booked {theirs:?} stolen but dragons took {mine:?}"
                ));
            }
        }
        Ok(())
    }

    /// A hash of the dragons' state, for determinism checks.
    pub fn state_hash(&self) -> u64 {
        let mut wr = Writer::new();
        for (h, x) in self.dragons.iter() {
            wr.u64(h.to_bits());
            wr.u64(x.seed);
            wr.u64(x.age_base);
            wr.u64(x.age_at.0);
            wr.raw(&x.fullness.to_le_bytes());
            wr.u64(x.fed_at.0);
            wr.raw(&x.health.to_bits().to_le_bytes());
            wr.u64(x.health_at.0);
            wr.u64(u64::from(x.plan));
            wr.u64(u64::from(x.epoch));
            wr.u64(x.lair.map_or(u64::MAX, u64::from));
            if let Some(l) = x.leg {
                for v in [l.path.x0, l.path.z0, l.path.x1, l.path.z1] {
                    wr.raw(&v.to_le_bytes());
                }
                wr.u64(l.path.t0.0);
                wr.u64(l.path.t1.0);
            }
        }
        for l in &self.lairs {
            wr.u64(l.key);
            wr.u64(l.occupants.len() as u64);
            for (k, v) in &l.hoard {
                wr.raw(k.key().as_bytes());
                wr.raw(&v.to_le_bytes());
            }
        }
        for m in [&self.ledger.taken, &self.ledger.eaten, &self.ledger.burnt, &self.ledger.hoarded] {
            for (k, v) in m {
                wr.raw(k.key().as_bytes());
                wr.raw(&v.to_le_bytes());
            }
        }
        fnv1a64(wr.as_bytes())
    }

    pub fn heap_bytes(&self) -> usize {
        self.dragons.heap_bytes()
            + self.dragons.iter().map(|(_, x)| x.children.capacity() * 8 + x.victims.capacity() * 8 + x.name.capacity()).sum::<usize>()
            + self.lairs.capacity() * std::mem::size_of::<Lair>()
            + self.wheel.heap_bytes()
            + self.homes.heap_bytes()
            + self.index.heap_bytes()
            + self.log.capacity() * std::mem::size_of::<LogEntry>()
    }

    /// The districts a player's home holds, for display.
    pub fn home_districts(&self, infra: &mut dyn Infrastructure, p: ActorId, at: Tick) -> Vec<DistrictId> {
        match self.homes.home(p, at) {
            Some(h) => infra
                .districts_near(h.pos.frame, i64::from(h.pos.x), i64::from(h.pos.z), HOME_RADIUS)
                .into_iter()
                .map(|s| s.id)
                .collect(),
            None => Vec::new(),
        }
    }
}
