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

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Arena;
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_settle::Pos;
use rtsim_travel::legs::{LegIndex, Meeting, Owner, Path, REGION_BLOCKS, Stamp};
use serde::Serialize;

use super::boundary::{
    self, Capture, Cmd, KillerKind, MAT_SURFACE, MEMBER_SHEARED, MemberSpawn, Obs, Uuid,
};
use super::climate::{Climate, Subs, Uniform};
use super::cohort::Class;
use super::forage::{CELL_BLOCKS, CellKey, Forage, cell_of};
use super::herd::{
    Embodied, Herd, HerdId, HerdLedger, Route, agent_of, herd_of, route_regions,
};
use super::members::{self, member};
use super::route::{self, Ask, Obstacles};
use super::species::{Cohort, HerdSpecies, HerdSpeciesData};
use super::tunables::HerdTunables;

pub const SYS_HERD: SystemId = SystemId::named("herds.herd");
const SYS_BIRTH: SystemId = SystemId::named("herds.birth");
const SYS_STARVE: SystemId = SystemId::named("herds.starve");
const SYS_PLACE: SystemId = SystemId::named("herds.place");

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
    (o.0 & OWNER_MASK == HERD_OWNER)
        .then(|| HerdId::from_bits(o.0 & ((1 << 48) - 1)))
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

/// A player as the herds know it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerView {
    pub uuid: Uuid,
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
    pub herds: Arena<Herd>,
    pub forage: Forage,
    pub obstacles: Obstacles,
    pub(crate) wheel: TimingWheel<Ev>,
    /// Herds by the regions their current route covers (with the footprint): derived from the
    /// routes, so not saved.
    pub(crate) regions: BTreeMap<(u16, i64, i64), BTreeSet<HerdId>>,
    pub(crate) herd_regions: BTreeMap<HerdId, Vec<(u16, i64, i64)>>,
    pub(crate) players: Vec<PlayerView>,
    pub(crate) player_by_key: BTreeMap<u64, u32>,
    /// Cells herds settled on in sub-season `claims_sub`.
    pub(crate) claims: BTreeMap<CellKey, u64>,
    pub(crate) claims_sub: i64,
    pub(crate) outbox: Vec<Cmd>,
    /// Predator → herds it follows.
    pub(crate) follows: BTreeMap<u64, BTreeSet<HerdId>>,
    /// Plugin op ids applied, and when (forgotten after a while at the yearly sweep).
    pub(crate) ops: BTreeMap<u64, Tick>,
    pub stats: HerdStats,
    pub log_enabled: bool,
    pub log: Vec<HerdLog>,
    pub trace_enabled: bool,
    pub trace: Vec<Decision>,
    pub(crate) next_seed: u64,
    pub(crate) now: Tick,
}

fn day_of(t: Tick) -> i64 {
    (t.0 / TICKS_PER_DAY) as i64
}

fn dayf(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn dist(a: (i64, i64), b: (i64, i64)) -> f64 {
    (((a.0 - b.0) as f64).powi(2) + ((a.1 - b.1) as f64).powi(2)).sqrt()
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
    let a = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0).acos();
    let b = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0).acos();
    let k = ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2)).max(0.0);
    r1 * r1 * a + r2 * r2 * b - 0.5 * k.sqrt()
}

/// The same 64-bit stand-in for a player's UUID the phase 1 world and the dragons use.
pub fn player_key(uuid: Uuid) -> u64 {
    crate::engine::uuid_hash(uuid.0, uuid.1)
}

impl Herds {
    pub fn new(seed: u64, tun: HerdTunables, climate: Box<dyn Climate>, now: Tick) -> Self {
        let mut h = Herds {
            tun,
            climate,
            seed,
            herds: Arena::new(),
            forage: Forage::new(),
            obstacles: Obstacles::default(),
            wheel: TimingWheel::new(now),
            regions: BTreeMap::new(),
            herd_regions: BTreeMap::new(),
            players: Vec::new(),
            player_by_key: BTreeMap::new(),
            claims: BTreeMap::new(),
            claims_sub: i64::MIN,
            outbox: Vec::new(),
            follows: BTreeMap::new(),
            ops: BTreeMap::new(),
            stats: HerdStats::default(),
            log_enabled: false,
            log: Vec::new(),
            trace_enabled: false,
            trace: Vec::new(),
            next_seed: 0,
            now,
        };
        let subs = h.subs();
        let n = subs.index(now);
        let next_year = subs.start((Subs::year(n) + 1) * 12);
        h.wheel.schedule(next_year, Ev::Year);
        h
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
        std::mem::take(&mut self.outbox)
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

    /// Where a herd's centre is at `t` (closed form).
    pub fn position(&self, h: HerdId, t: Tick) -> (i64, i64) {
        self.herds[h].route.pos(t)
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

    fn stagger(&self, x: &Herd) -> u64 {
        x.seed % self.tun.stagger_ticks.max(1)
    }

    fn decide_at(&self, x: &Herd, at: Tick) -> Tick {
        let subs = self.subs();
        let n = subs.index(at);
        let s = self.stagger(x);
        let this = subs.start(n) + s;
        if this > at { this } else { subs.start(n + 1) + s }
    }

    // ---- herds ---------------------------------------------------------------------------------

    /// Adds a herd at `at` with a stable age pyramid (founders), and books its first decision at
    /// once.
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
        let herd = Herd {
            name: n.name,
            species: n.species,
            seed,
            frame: n.frame,
            classes,
            next_index: next,
            condition: [800; 3],
            satisfaction: 1000,
            route: Route::stay(n.pos, at, at),
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
        };
        let id = self.herds.insert(herd);
        self.herds[id].next_decide = at;
        self.schedule(at, Ev::Decide { h: id });
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
    }

    fn handle(&mut self, legs: &mut LegIndex, at: Tick, ev: Ev) {
        match ev {
            Ev::Decide { h } => {
                if self.herds.get(h).is_some_and(|x| x.next_decide == at) {
                    self.decide(legs, h, at);
                } else {
                    self.stats.stale += 1;
                }
            }
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
        }
        // Classes with nobody left go (their deaths committed first).
        {
            let x = self.herds.get_mut(h).expect("live");
            let mut died = 0u64;
            x.classes.retain_mut(|c| {
                if c.alive_on(&sp, yd, x.seed, day) == 0 {
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
            let x = self.herds.get_mut(h).expect("live");
            x.route = Route::stay(x.route.end(), at, at);
            x.next_decide = Tick(u64::MAX);
            self.set_regions(h, Vec::new());
            self.note(at, "extinct", Some(h), "the last animal died".into());
            return;
        }
        // The next decision, then the route to it.
        let next = self.decide_at(&self.herds[h], at + 1);
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
        let from = x.route.pos(at);
        let choice = route::plan(&Ask {
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
        let tc = cell_of(i64::from(choice.target.0), i64::from(choice.target.1));
        self.claims.insert((frame, tc.0, tc.1), h.to_bits());
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
        }
        let regions = route_regions(&self.herds[h].route, frame, radius);
        self.set_regions(h, regions);
        self.steer(h, at);
        self.meet_players(legs, h, at);
    }

    fn set_regions(&mut self, h: HerdId, list: Vec<(u16, i64, i64)>) {
        if let Some(old) = self.herd_regions.remove(&h) {
            for r in old {
                if let Some(s) = self.regions.get_mut(&r) {
                    s.remove(&h);
                    if s.is_empty() {
                        self.regions.remove(&r);
                    }
                }
            }
        }
        for r in &list {
            self.regions.entry(*r).or_default().insert(h);
        }
        if !list.is_empty() {
            self.herd_regions.insert(h, list);
        }
    }

    /// Rebuilds the region index from every herd's route (after a load).
    pub(crate) fn rebuild_regions(&mut self) {
        self.regions.clear();
        self.herd_regions.clear();
        let ids: Vec<HerdId> = self.herds.iter().map(|(h, _)| h).collect();
        for h in ids {
            let x = &self.herds[h];
            // Extinct, or not planned yet (a route is indexed when it is planned).
            if x.next_decide == Tick(u64::MAX) || x.reach == 0 {
                continue;
            }
            let regions = route_regions(&x.route, x.frame, x.reach);
            self.set_regions(h, regions);
        }
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
        let centre = x.route.pos(at);
        let frame = x.frame;
        let r = f64::from(sp.footprint_radius(count));
        let c0 = cell_of(centre.0, centre.1);
        let span = (r / f64::from(CELL_BLOCKS)).ceil() as i32;
        let mut cells = Vec::new();
        for dx in -span..=span {
            for dz in -span..=span {
                let c = (c0.0 + dx, c0.1 + dz);
                let cc = super::forage::centre_of(c);
                if dist((i64::from(cc.0), i64::from(cc.1)), centre)
                    <= r + f64::from(CELL_BLOCKS) / 2.0
                    && self.climate.terrain(frame, c.0, c.1).is_finite()
                {
                    cells.push((frame, c.0, c.1));
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
                self.note(
                    at,
                    "starve",
                    Some(h),
                    format!("met {:.0}% of its demand; {starved} starved", sat * 100.0),
                );
            }
        }
    }

    /// Births: one binomial draw on the adults alive, at the condition's share of fertility.
    /// Every class's natural deaths are committed first (the yearly anchor).
    fn breed(&mut self, h: HerdId, at: Tick, year: i64) {
        let yd = self.year_days();
        let day = day_of(at);
        let world = self.seed;
        let sp = self.sp(h).clone();
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
        let litters = sp.fertility.ceil().max(1.0);
        let p = (sp.fertility * cond / litters).clamp(0.0, 1.0);
        let births = StreamKey::new(world, SYS_BIRTH, x.seed)
            .slot(year as u64)
            .binomial(adults * litters as u64, p);
        let births = births.min(u64::from(u32::MAX));
        if births > 0 {
            x.classes
                .push(Class::new(x.next_index, births as u32, day, day, births as u32));
            x.next_index += births;
            x.ledger.born += births;
        }
        x.bred_year = year;
        self.stats.breedings += 1;
        self.note(
            at,
            "births",
            Some(h),
            format!("{births} born to {adults} adults (condition {:.0}%)", cond * 100.0),
        );
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
            let (dx, dz) = (row.path.x0 - pos.x, row.path.z0 - pos.z);
            let moved = i64::from(dx).pow(2) + i64::from(dz).pow(2);
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
        // Herds whose routes pass near.
        let reach = i64::from(view) + 1_024;
        let (x, z) = (i64::from(pos.x), i64::from(pos.z));
        let (a, b) = (
            ((x - reach).div_euclid(REGION_BLOCKS), (z - reach).div_euclid(REGION_BLOCKS)),
            ((x + reach).div_euclid(REGION_BLOCKS), (z + reach).div_euclid(REGION_BLOCKS)),
        );
        let mut near = BTreeSet::new();
        for rx in a.0..=b.0 {
            for rz in a.1..=b.1 {
                if let Some(s) = self.regions.get(&(pos.frame, rx, rz)) {
                    near.extend(s.iter().copied());
                }
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
        let mine: Vec<HerdId> = self
            .herds
            .iter()
            .filter(|(_, x)| {
                x.embodied
                    .as_ref()
                    .is_some_and(|e| e.player == slot && !e.leaving)
            })
            .map(|(h, _)| h)
            .collect();
        for h in mine {
            self.despawn(h, 2, at);
        }
    }

    /// Meetings for every player whose area is near the herd's (new) route.
    fn meet_players(&mut self, legs: &mut LegIndex, h: HerdId, at: Tick) {
        if self.herds[h].embodied.is_some() {
            return;
        }
        let Some(regions) = self.herd_regions.get(&h) else {
            return;
        };
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
        self.stats.clips += 1;
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
        let (dmin, _) = self.herds[h].route.closest(p, at, at + 2 * window);
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
        let seen_cap = self.tun.seen_cap;
        let regrow = self.tun.sheared_ticks;
        let x = self.herds.get_mut(h).expect("live");
        members::reconcile(x, &sp, yd, day);
        let picked = members::pick(x, &sp, yd, day, n, at, seen_cap);
        x.epoch = x.epoch.wrapping_add(1).max(1);
        let epoch = x.epoch;
        let centre = x.route.pos(at);
        let heading = x.route.heading(at);
        let d = dist(centre, p);
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
                if dist(q, p) <= inner {
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
        x.embodied = Some(Embodied {
            epoch,
            player: slot,
            members: spawns.iter().map(|s| s.member).collect(),
            since: at,
            steer: heading,
            leaving: false,
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
        let Some(e) = x.embodied.as_ref().filter(|e| e.epoch == epoch && !e.leaving) else {
            return;
        };
        let slot = e.player;
        let p = &self.players[slot as usize];
        let gone = p.pos.is_none() || !p.interest.is_some_and(|s| legs.is_live(s));
        if gone {
            self.despawn(h, 2, at);
            return;
        }
        let (pos, _) = p.pos.expect("checked");
        let r = f64::from(self.footprint(h, at));
        let far = f64::from(self.tun.view_radius) + r + f64::from(self.tun.hysteresis);
        if dist(x.route.pos(at), (i64::from(pos.x), i64::from(pos.z))) > far {
            self.despawn(h, 1, at);
            return;
        }
        self.steer(h, at);
        self.schedule(at + self.tun.check_ticks, Ev::Check { h, epoch });
    }

    fn steer(&mut self, h: HerdId, at: Tick) {
        let speed = (self.sp(h).speed_blocks_per_day / TICKS_PER_DAY as f64) as f32;
        let x = self.herds.get_mut(h).expect("live");
        let heading = x.route.heading(at);
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

    /// A new session: no group outlives one (docs/boundary.md §2.3). Every embodied group is
    /// abstract again with what the engine knew.
    pub fn session_start(&mut self, at: Tick) {
        let ids: Vec<HerdId> = self.herds.iter().map(|(h, _)| h).collect();
        for h in ids {
            if let Some(e) = self.herds.get_mut(h).expect("live").embodied.take() {
                for k in &e.members {
                    if let Some(s) = self.herds.get_mut(h).expect("live").seen.get_mut(k) {
                        s.last = at;
                    }
                }
            }
        }
        for p in &mut self.players {
            p.puppets = 0;
            p.interest = None;
            p.pos = None;
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

    /// Applies one record from the plugin.
    pub fn ingest(&mut self, legs: &mut LegIndex, o: &Obs, at: Tick) {
        match o {
            Obs::GroupState(_) | Obs::Materialized(_) => {}
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
                self.obstacle(legs, h, ob, at);
            }
        }
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
        let stuck = (i64::from(ob.stuck.0), i64::from(ob.stuck.1));
        let target = (i64::from(ob.target.0), i64::from(ob.target.1));
        let here = cell_of(stuck.0, stuck.1);
        let d = dist(stuck, target).max(1.0);
        let step = f64::from(CELL_BLOCKS);
        let ahead = (
            stuck.0 + ((target.0 - stuck.0) as f64 * step / d) as i64,
            stuck.1 + ((target.1 - stuck.1) as f64 * step / d) as i64,
        );
        let mut blocked = cell_of(ahead.0, ahead.1);
        if blocked == here {
            blocked = cell_of(target.0, target.1);
        }
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
        let (a, b) = (
            ((x - range).div_euclid(REGION_BLOCKS), (z - range).div_euclid(REGION_BLOCKS)),
            ((x + range).div_euclid(REGION_BLOCKS), (z + range).div_euclid(REGION_BLOCKS)),
        );
        for rx in a.0..=b.0 {
            for rz in a.1..=b.1 {
                if let Some(s) = self.regions.get(&(frame, rx, rz)) {
                    cands.extend(s.iter().copied());
                }
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
                            x.followers.iter().any(|&(p, t)| {
                                p == predator && at.0 < t.0 + self.tun.follow_ticks
                            })
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
            if hx.frame != frame || hx.next_decide == Tick(u64::MAX) {
                continue;
            }
            let f = followed.contains(&h);
            let reach = if f {
                range as f64 * self.tun.follow_factor
            } else {
                range as f64
            };
            let p = hx.route.pos(at);
            if dist(p, (x, z)) > reach {
                continue;
            }
            let alive = self.count(h, at);
            let available = alive.saturating_sub(hx.embodied_count());
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
        let Some(hx) = self.herds.get(h) else {
            return Predation::default();
        };
        let r = f64::from(self.footprint(h, at));
        if dist(hx.route.pos(at), (x, z)) > r + f64::from(self.tun.predation_reach) {
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
        let n = (want as u64).min(cap).min(latent);
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
        let food = (given as i64 * i64::from(food_each)).min(i64::from(i32::MAX)) as i32;
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
    /// dead sets never overlap, and embodied members are seen.
    pub fn check(&self, at: Tick) -> Result<(), String> {
        let yd = self.year_days();
        let day = day_of(at);
        for (h, x) in self.herds.iter() {
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
            if x.ledger.founded + x.ledger.born != x.next_index {
                return Err(format!(
                    "herd {}: {} founded and born but next index {}",
                    x.name,
                    x.ledger.founded + x.ledger.born,
                    x.next_index
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
        Ok(())
    }

    /// A hash of the whole state: the save's bytes.
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
    }

    pub fn heap_bytes(&self) -> usize {
        self.herds.heap_bytes()
            + self
                .herds
                .iter()
                .map(|(_, x)| x.heap_bytes())
                .sum::<usize>()
            + self.forage.heap_bytes()
            + self.obstacles.cells.len() * 64
            + self.wheel.heap_bytes()
            + self.regions.values().map(|s| 48 + s.len() * 16).sum::<usize>()
            + self.herd_regions.values().map(|v| 32 + v.capacity() * 24).sum::<usize>()
            + self.claims.len() * 40
            + self.ops.len() * 32
            + self.follows.values().map(|s| 32 + s.len() * 16).sum::<usize>()
            + self.players.capacity() * std::mem::size_of::<PlayerView>()
    }
}
