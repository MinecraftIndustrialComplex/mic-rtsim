// SPDX-License-Identifier: AGPL-3.0-or-later
//! A dragon's row and its lair (PLAN.md §9).
//!
//! **A dragon is a notable** (PLAN.md §5.7): its feelings and memories live in rtsim-knowledge
//! under its actor, its temperament comes from its seed (C25), and its death of age is one event
//! booked when it is created (C24). Its heir link is the knowledge notable's: its youngest living
//! offspring, else its mate, else its lair.
//!
//! **Nothing here is stepped.** Fullness and health are closed forms anchored at the last change:
//! fullness falls one point every `hunger_tick_rate` ticks from its anchor, health regains a share
//! per day up to the stage's maximum. Age and stage are derived from birth when read.

use std::collections::BTreeMap;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::rows::PagedRows;
use rtsim_knowledge::{ActorId, Temperament};
use rtsim_settle::Pos;
use rtsim_space::topology::Topologies;
use rtsim_travel::legs::{Path, Stamp};
use serde::Serialize;

use crate::oil::OilTunables;
use crate::prey::Item;
use crate::species::{Gender, Species, SpeciesData, Stage};
use crate::tunables::DragonTunables;

pub type DragonId = Handle<Dragon>;
pub type LairId = u32;

/// A lair is Ice and Fire's roost or cave structure (PLAN.md §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum LairKind {
    Roost,
    Cave,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lair {
    /// The structure start's id, as the bridge sends it (LAIR_FOUND).
    pub key: u64,
    pub name: String,
    pub species: Species,
    pub kind: LairKind,
    pub pos: Pos,
    pub y: i32,
    /// The lair as a knowledge holder: the heir of last resort (C24, PLAN.md §9).
    pub actor: ActorId,
    pub occupants: Vec<DragonId>,
    /// What the dragons carried home (valuables), in the engine's ledger.
    pub hoard: BTreeMap<Item, i64>,
    /// Since when nobody has lived here.
    pub vacant_since: Option<Tick>,
    /// Stamps the respawn event booked for this vacancy.
    pub plan: u32,
    /// Tribute coin not yet sent to the lair's chest, whole items (SINK.md §4.6): it waits for
    /// the plugin to report the chest (LAIR_CHEST).
    pub unplaced: BTreeMap<Item, i64>,
    /// Whole items of tribute coin the plugin put in the lair's chest.
    pub placed: i64,
    /// Stacks of tribute coin this lair has sent: the next stack's op id is drawn at this index.
    pub stacks: u64,
}

/// The lairs' row table (A16): page table 0x4411, rows keyed by lair index. Lairs are never
/// removed, so the keys are `0..len` and a `LairId` is a stable key. Resident until
/// [`Lairs::rows_mut`]'s table is attached to a page store; then a lair is read through `&self`
/// and loads its page on a miss, like [`PagedRows`].
pub struct Lairs {
    rows: PagedRows<Lair>,
}

impl std::fmt::Debug for Lairs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rows.fmt(f)
    }
}

impl Default for Lairs {
    fn default() -> Self {
        Lairs::new()
    }
}

impl Lairs {
    /// The lairs' page table, next to the dragons' rows (0x4410).
    pub const TABLE: u16 = 0x4411;
    /// Lairs per page. PLACEHOLDER 1, as the dragons' rows (`engine::DRAGONS_PER_PAGE`): a lair
    /// changes when a dragon claims, leaves or hoards, and pins its page until the next save.
    pub const PER_PAGE: u64 = 1;

    pub fn new() -> Self {
        Lairs {
            rows: PagedRows::new(Self::TABLE, Self::PER_PAGE),
        }
    }

    /// A table from its rows (the save's blob, or a paged open).
    pub fn from_rows(rows: PagedRows<Lair>) -> Self {
        Lairs { rows }
    }

    pub fn rows(&self) -> &PagedRows<Lair> {
        &self.rows
    }

    pub fn rows_mut(&mut self) -> &mut PagedRows<Lair> {
        &mut self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The lair, loading its page if needed.
    pub fn get(&self, l: LairId) -> Option<&Lair> {
        self.rows.get(u64::from(l))
    }

    pub fn get_mut(&mut self, l: LairId) -> Option<&mut Lair> {
        self.rows.get_mut(u64::from(l))
    }

    /// Adds a lair under the next index.
    pub fn push(&mut self, lair: Lair) -> LairId {
        let id = LairId::try_from(self.rows.len()).expect("fewer than 2^32 lairs");
        let old = self.rows.insert(u64::from(id), lair);
        debug_assert!(old.is_none(), "lair {id} added twice");
        id
    }

    /// Every lair in index order. A whole-table pass (saves, checks, tools): in a paged table it
    /// loads every page through `&self`.
    pub fn iter(&self) -> impl Iterator<Item = &Lair> + '_ {
        (0..self.len() as LairId).map(|l| self.get(l).expect("lairs are never removed"))
    }
}

impl std::ops::Index<usize> for Lairs {
    type Output = Lair;

    fn index(&self, l: usize) -> &Lair {
        LairId::try_from(l)
            .ok()
            .and_then(|l| self.get(l))
            .unwrap_or_else(|| panic!("no lair {l}"))
    }
}

impl std::ops::IndexMut<usize> for Lairs {
    fn index_mut(&mut self, l: usize) -> &mut Lair {
        LairId::try_from(l)
            .ok()
            .and_then(|l| self.get_mut(l))
            .unwrap_or_else(|| panic!("no lair {l}"))
    }
}

/// Where a hunt goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A point on a road (an edge of the phase 1 world), `frac_milli` of the way from its `a` end.
    Road {
        edge: u32,
        x: i32,
        z: i32,
        frac_milli: u32,
    },
    /// A village's livestock (a market node).
    Village { node: u32 },
    /// A player's home (P26), when a grudge sends it, in the home's frame (dimension).
    Home {
        player: ActorId,
        frame: u16,
        x: i32,
        z: i32,
    },
    /// A wild herd (`herds`), where it will be when the dragon arrives: `herd` is the herd's row
    /// handle bits.
    Herd {
        herd: u64,
        frame: u16,
        x: i32,
        z: i32,
    },
    /// A district it holds a grudge against (OILGATE.md §4.5, OG-8): industry near an oil field
    /// it drinks at. Raided through the industry threat interface, as a home's districts are.
    District {
        district: u64,
        frame: u16,
        x: i32,
        z: i32,
    },
}

/// Why a dragon is flying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Hunt(Target),
    Home,
    /// Moving to a roost: a young dragon leaving its parents, a respawned one arriving, or a
    /// starved one migrating (all claim it on arrival).
    Claim(LairId),
    /// To an oil field to drink (OILGATE.md §4.3, OG-5): `field` is the field's id when the
    /// flight began, (x, z) the oil chunk's centre it lands on.
    Refuel {
        field: u32,
        frame: u16,
        x: i32,
        z: i32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// At its lair. A `Hungry` event is booked for when it next hunts.
    Roosting,
    /// On a leg.
    Flying(Purpose),
    /// Hunting at a target until a booked end.
    Hunting { target: Target, until: Tick },
    /// A Minecraft entity near a player. A departure is booked (C24: threats get bored).
    Embodied {
        epoch: u32,
        since: Tick,
        depart_at: Tick,
        purpose: Purpose,
        /// It came to harass (hunting here, or a grudge against someone here), not to pass.
        harass: bool,
    },
    /// Told to leave; waiting for the bridge to capture it.
    Leaving { epoch: u32, purpose: Purpose },
}

/// A leg the dragon is on: its path in the shared leg index, at a forced altitude while unloaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightLeg {
    pub path: Path,
    pub y: i32,
    pub stamp: Stamp,
}

impl FlightLeg {
    pub fn at(&self, t: Tick) -> (i64, i32, i64) {
        let (x, z) = self.path.at(t);
        (x, self.y, z)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dragon {
    pub actor: ActorId,
    pub seed: u64,
    pub name: String,
    pub species: Species,
    pub gender: Gender,
    /// Colour, 0 to 3 within the species. Ice and Fire's `Variant` NBT is a string; its eggs pick
    /// a colour as `DRAGON_COLOR.byId(random(4) + getStartMetaForType())`, offsets 0, 4 and 8 for
    /// fire, ice and lightning (verified), so the plugin maps this index the same way.
    pub variant: u8,
    /// Age in ticks at `age_at` (a dragon adopted from the world can be older than the sim).
    pub age_base: u64,
    pub age_at: Tick,
    pub dies: Tick,
    pub lair: Option<LairId>,
    /// Fullness at `fed_at`: Ice and Fire's `Hunger` NBT, 0 to 100, higher is fuller (verified).
    pub fullness: i32,
    pub fed_at: Tick,
    /// Health at `health_at`; it regains toward the stage's maximum.
    pub health: f32,
    pub health_at: Tick,
    pub state: State,
    pub leg: Option<FlightLeg>,
    /// Bumped on every replan: events carrying an older value are stale (DECISIONS.md O6).
    pub plan: u32,
    /// Materializations so far: each embodiment's epoch.
    pub epoch: u32,
    pub mate: Option<DragonId>,
    pub parent: Option<DragonId>,
    pub children: Vec<DragonId>,
    /// Settlements it preyed on, newest last: they thank its slayer.
    pub victims: Vec<ActorId>,
    /// Hunts in a row that found nothing.
    pub fruitless: u8,
    /// Hunts in the current bout (since it last left its lair).
    pub bout: u8,
    /// When it last went after a grudge (a raid or a visit), for the grudge cooldown.
    pub last_grudge: Option<Tick>,
    /// Valuables it carries while it has no lair; they go to the hoard of the lair it claims.
    pub carried: BTreeMap<Item, i64>,
    /// Where it last was when it stopped flying (a captured position).
    pub rest: Option<(Pos, i32)>,
    /// Its oil reserve at `oil_at` (OILGATE.md §4.3, OG-4): it falls one point every
    /// `oil.tick_rate` ticks and one per breath. Only species with `uses_oil` read it.
    pub oil: i32,
    pub oil_at: Tick,
    /// A field flight's dragon (OILGATE.md §4.2): the field it was drawn for. It leaves the sim
    /// after it drinks unless it carries something.
    pub visitor: Option<u32>,
    /// Districts it sensed lately and the day it did (OG-8: one report per district per day),
    /// newest last, at most [`SENSED_KEPT`].
    pub sensed: Vec<(u64, u32)>,
}

/// Districts a dragon remembers sensing (for the once-a-day rule).
pub const SENSED_KEPT: usize = 8;

impl Dragon {
    pub fn temperament(&self) -> Temperament {
        Temperament::from_seed(self.seed)
    }

    pub fn age(&self, at: Tick) -> u64 {
        self.age_base + at.since(self.age_at)
    }

    pub fn stage(&self, d: &SpeciesData, at: Tick) -> Stage {
        d.stage(self.age(at))
    }

    /// Fullness at `at`: one point lost every `hunger_tick_rate` ticks since the last meal.
    pub fn fullness_at(&self, tun: &DragonTunables, at: Tick) -> i32 {
        let lost = at.since(self.fed_at) / tun.hunger_tick_rate.max(1);
        (i64::from(self.fullness) - lost.min(i64::MAX as u64) as i64).max(0) as i32
    }

    /// When fullness falls to `level` (now if it already has).
    pub fn hungry_at(&self, tun: &DragonTunables, level: i32, now: Tick) -> Tick {
        if self.fullness <= level {
            return now.max(self.fed_at);
        }
        let t = self.fed_at + (self.fullness - level) as u64 * tun.hunger_tick_rate.max(1);
        t.max(now)
    }

    /// Eats `points`: fullness re-anchored at `at`, capped at full.
    pub fn eat(&mut self, tun: &DragonTunables, points: i32, at: Tick) {
        let f = self.fullness_at(tun, at);
        if points <= 0 {
            return;
        }
        self.fullness = f.saturating_add(points).min(tun.full);
        self.fed_at = at;
    }

    /// Sets fullness as captured from the body.
    pub fn set_fullness(&mut self, f: i32, at: Tick) {
        self.fullness = f;
        self.fed_at = at;
    }

    pub fn max_health(&self, d: &SpeciesData, at: Tick) -> f32 {
        d.max_health_at(self.age(at))
    }

    /// Health at `at`, regained since the anchor, capped at the stage's maximum.
    pub fn health_at(&self, d: &SpeciesData, at: Tick) -> f32 {
        let max = self.max_health(d, at);
        if matches!(self.state, State::Embodied { .. } | State::Leaving { .. }) {
            return self.health.min(max);
        }
        let days = at.since(self.health_at) as f64 / TICKS_PER_DAY as f64;
        let regen = f64::from(max) * d.regen_per_day_milli as f64 / 1000.0 * days;
        ((f64::from(self.health) + regen) as f32).min(max)
    }

    pub fn set_health(&mut self, h: f32, at: Tick) {
        self.health = h.max(0.0);
        self.health_at = at;
    }

    /// Its oil reserve at `at`: one point lost every `tick_rate` ticks since the anchor.
    pub fn oil_level(&self, tun: &OilTunables, at: Tick) -> i32 {
        let lost = at.since(self.oil_at) / tun.tick_rate.max(1);
        (i64::from(self.oil) - lost.min(i64::MAX as u64) as i64).max(0) as i32
    }

    /// When the reserve falls to `level` (now if it already has).
    pub fn oil_low_at(&self, tun: &OilTunables, level: i32, now: Tick) -> Tick {
        if self.oil <= level {
            return now.max(self.oil_at);
        }
        let t = self.oil_at + (self.oil - level) as u64 * tun.tick_rate.max(1);
        t.max(now)
    }

    /// Sets the reserve at `at`, within 0 and the capacity.
    pub fn set_oil(&mut self, tun: &OilTunables, v: i32, at: Tick) {
        self.oil = v.clamp(0, tun.capacity);
        self.oil_at = at;
    }

    /// Burns up to `n` points; returns the points burnt.
    pub fn burn_oil(&mut self, tun: &OilTunables, n: i32, at: Tick) -> i32 {
        let have = self.oil_level(tun, at);
        let b = n.clamp(0, have);
        if b > 0 {
            self.set_oil(tun, have - b, at);
        }
        b
    }

    /// Adds up to `n` points, capped at the capacity; returns the points added.
    pub fn refill_oil(&mut self, tun: &OilTunables, n: i32, at: Tick) -> i32 {
        let have = self.oil_level(tun, at);
        let add = n.clamp(0, tun.capacity - have);
        if add > 0 {
            self.set_oil(tun, have + add, at);
        }
        add
    }

    pub fn is_embodied(&self) -> bool {
        matches!(self.state, State::Embodied { .. } | State::Leaving { .. })
    }

    /// Where it is at `at` while unloaded: on its leg (at the forced altitude) or at rest.
    /// Where the dragon is at `at`: on its leg (the leg's chart position folded, TP7), at rest, or
    /// at its lair. Always canonical.
    pub fn position(&self, topo: &Topologies, lairs: &Lairs, at: Tick) -> (Pos, i32) {
        if let (State::Flying(_) | State::Hunting { .. }, Some(l)) = (self.state, self.leg) {
            let (x, y, z) = l.at(at);
            let (x, z) = topo.canonical(l.path.frame, x, z);
            return (Pos::new(l.path.frame, x as i32, z as i32), y);
        }
        if let Some(r) = self.rest {
            return r;
        }
        match self.lair.and_then(|l| lairs.get(l)) {
            Some(l) => (l.pos, l.y),
            None => (Pos::new(0, 0, 0), 64),
        }
    }
}

/// The 64-bit agent id the bridge sees for a dragon: namespace byte 0x44 ('D') over the row
/// handle's 48 bits, so it never meets a caravan's agent id (docs/boundary.md §6).
pub const AGENT_NAMESPACE: u64 = 0x44 << 56;

pub fn agent_of(d: DragonId) -> u64 {
    AGENT_NAMESPACE | d.to_bits()
}

pub fn dragon_of(agent: u64) -> Option<DragonId> {
    (agent >> 56 == AGENT_NAMESPACE >> 56).then(|| Handle::from_bits(agent & ((1 << 48) - 1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dragon() -> Dragon {
        Dragon {
            actor: Handle::NULL,
            seed: 1,
            name: "t".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_base: 0,
            age_at: Tick(0),
            dies: Tick(u64::MAX),
            lair: None,
            fullness: 80,
            fed_at: Tick(0),
            health: 100.0,
            health_at: Tick(0),
            state: State::Roosting,
            leg: None,
            plan: 0,
            epoch: 0,
            mate: None,
            parent: None,
            children: vec![],
            victims: vec![],
            fruitless: 0,
            bout: 0,
            last_grudge: None,
            carried: BTreeMap::new(),
            rest: None,
            oil: 100,
            oil_at: Tick(0),
            visitor: None,
            sensed: Vec::new(),
        }
    }

    #[test]
    fn hunger_is_closed_form() {
        let tun = DragonTunables::default();
        let mut d = dragon();
        assert_eq!(d.fullness_at(&tun, Tick(2_999)), 80);
        assert_eq!(d.fullness_at(&tun, Tick(3_000)), 79);
        // 20 points at 3,000 ticks each.
        assert_eq!(d.hungry_at(&tun, 60, Tick(0)), Tick(60_000));
        assert_eq!(d.fullness_at(&tun, Tick(60_000)), 60);
        d.eat(&tun, 50, Tick(60_000));
        assert_eq!(d.fullness_at(&tun, Tick(60_000)), 100);
        assert_eq!(d.fullness_at(&tun, Tick(10_000_000)), 0);
    }

    #[test]
    fn oil_is_closed_form() {
        let tun = OilTunables::default();
        let mut d = dragon();
        assert_eq!(d.oil_level(&tun, Tick(2_999)), 100);
        assert_eq!(d.oil_level(&tun, Tick(3_000)), 99);
        assert_eq!(d.oil_low_at(&tun, 40, Tick(0)), Tick(180_000));
        assert_eq!(d.burn_oil(&tun, 5, Tick(3_000)), 5);
        assert_eq!(d.oil_level(&tun, Tick(3_000)), 94);
        assert_eq!(d.burn_oil(&tun, 500, Tick(3_000)), 94);
        assert_eq!(d.oil_level(&tun, Tick(3_000)), 0);
        assert_eq!(d.refill_oil(&tun, 30, Tick(9_000)), 30);
        assert_eq!(d.refill_oil(&tun, 500, Tick(9_000)), 70);
        assert_eq!(d.oil_level(&tun, Tick(9_000)), 100);
    }

    #[test]
    fn agent_ids_round_trip() {
        let h: DragonId = Handle::from_parts(77, 3);
        assert_eq!(dragon_of(agent_of(h)), Some(h));
        assert_eq!(dragon_of(5), None);
    }
}
