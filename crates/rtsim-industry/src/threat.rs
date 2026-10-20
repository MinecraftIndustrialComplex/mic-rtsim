// SPDX-License-Identifier: AGPL-3.0-or-later
//! Threats to unwatched infrastructure (C16 to C18; INDUSTRY.md §13, I19 to I25).
//!
//! - **Sources are elsewhere** (I19): dragon legs, raids and bosses are other systems' agents. They
//!   reach this module as [`Threat`]s through [`ThreatFeed`]. [`SyntheticThreats`] stands in for
//!   them in tests and the CLI. The one source that is core is natural-mob **pressure** (I20).
//! - **Pressure** is measured: natural hostile spawns per dark spawnable block per night tick, seen
//!   while a player was there ([`Pressure`]). Unwatched, the expected spawns per day are that rate ×
//!   the district's dark spawnable area × night ticks per day (TUNABLE). The next attack night is
//!   one geometric draw from the counter RNG keyed by (district, previous attack's day), which has
//!   the same distribution as one draw per (district, night slot) but costs one draw per attack
//!   instead of one per night (overnight choice F17).
//! - **Resolution** (I22): defences pass the gates first (walls stop ground attackers except
//!   through gaps; roofs shield machines from flyers); then Lanchester's square law when the
//!   defence is aimed ranged fire (turrets), and the linear law otherwise. The outcome is the
//!   attackers' surviving share, which scales the damage. **It is never better for the defender
//!   than the best real fight observed against that class at comparable strength.** Defences never
//!   tested in a real fight count at a reduced weight (TUNABLE).
//! - **Damage** (I23): theft up to the survivors' carry capacity and a per-attack share of each
//!   good; destroyed blocks from the surveyed exposed set, perimeter first for ground attackers and
//!   roofless first for flyers, capped per attack; any machine lost stops the district. No fire.
//! - **Trains** (I24): an attacked leg loses carriage cargo, bounded the same way. Track is never
//!   cut unwatched.
//! - **Pacing** (I25): after an attack of a class, the district has a cooldown for that class.

use std::collections::BTreeMap;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::{StreamKey, SystemId};
use serde::Serialize;

use crate::ids::{DistrictId, Good, GoodVec, StationId, TrainId};
use crate::obs::{BlockFlags, DefenceSurvey, ExposedBlock, FightObserved, SpawnSample};
use crate::reconcile::apportion;
use crate::tunables::ThreatTunables;

/// The four threat classes of INDUSTRY.md §13.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ThreatClass {
    GroundMelee,
    GroundRanged,
    Flying,
    Siege,
}

impl ThreatClass {
    pub const COUNT: usize = 4;
    pub const ALL: [ThreatClass; 4] = [
        ThreatClass::GroundMelee,
        ThreatClass::GroundRanged,
        ThreatClass::Flying,
        ThreatClass::Siege,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// Ground attackers must get through the perimeter.
    pub const fn walks(self) -> bool {
        !matches!(self, ThreatClass::Flying)
    }
}

/// Who sent a threat. Only `Natural` is core; the rest are plugins and modules (I19).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ThreatSource {
    Natural,
    Dragon,
    Bandit,
    Boss,
    Synthetic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ThreatTarget {
    District(DistrictId),
    /// A booked train leg ([`LegIndex`]).
    Train(TrainId),
}

/// One attack.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Threat {
    /// Unique per attack; keys its deterministic choices.
    pub id: u64,
    pub source: ThreatSource,
    pub class: ThreatClass,
    pub count: u32,
    /// Total strength: count × per-attacker effectiveness, in the units fights are observed in.
    pub strength: f64,
    pub target: ThreatTarget,
    #[serde(serialize_with = "crate::ser::tick")]
    pub at: Tick,
}

/// Other systems hand their attacks over through this (I19).
pub trait ThreatFeed {
    /// Threats due in `[from, to)`, in time order.
    fn due(&mut self, from: Tick, to: Tick, out: &mut Vec<Threat>);
}

// ---- pressure (I20) -----------------------------------------------------------------------------

/// Measured natural spawn pressure for one district.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Pressure {
    /// Dark-block night ticks observed.
    pub block_ticks: f64,
    pub spawns: [u64; ThreatClass::COUNT],
}

impl Pressure {
    pub fn add(&mut self, s: &SpawnSample) {
        self.block_ticks += s.dark_blocks as f64 * s.observed_ticks as f64;
        for (a, &b) in self.spawns.iter_mut().zip(&s.spawns) {
            *a += u64::from(b);
        }
    }

    /// Spawns per dark spawnable block per night tick.
    pub fn rate(&self, c: ThreatClass) -> f64 {
        if self.block_ticks <= 0.0 {
            0.0
        } else {
            self.spawns[c.index()] as f64 / self.block_ticks
        }
    }

    /// Expected natural spawns of class `c` per game day on `dark_blocks` dark spawnable blocks.
    pub fn per_day(&self, c: ThreatClass, dark_blocks: u32, tun: &ThreatTunables) -> f64 {
        self.rate(c) * f64::from(dark_blocks) * tun.night_ticks_per_day as f64
    }
}

const PRESSURE: SystemId = SystemId::named("industry.pressure");

/// The next night (game day) at or after `after_day` on which natural mobs of class `c` attack,
/// with how many, from one geometric draw keyed by (district, class, `after_day`). `None` when the
/// expected count is zero.
pub fn next_pressure_attack(
    world_seed: u64,
    d: DistrictId,
    c: ThreatClass,
    mean_per_day: f64,
    after_day: u64,
) -> Option<(u64, u32)> {
    if !(mean_per_day > 0.0) || !mean_per_day.is_finite() {
        return None;
    }
    let p = -(-mean_per_day).exp_m1(); // P(K >= 1) for K ~ Poisson(mean)
    let key = StreamKey::new(world_seed, PRESSURE, d.0 ^ ((c.index() as u64) << 60));
    let mut rng = key.slot(after_day);
    let gap = if p >= 1.0 {
        0
    } else {
        let u = rng.next_f64_open();
        ((u.ln() / (-p).ln_1p()).ceil() as u64).saturating_sub(1)
    };
    // K given K >= 1, by inversion of the zero-truncated Poisson.
    let u = rng.next_f64_open() * p;
    let mut k = 1u32;
    let mut pk = mean_per_day * (-mean_per_day).exp();
    let mut cum = pk;
    while cum < u && k < 10_000 {
        k += 1;
        pk *= mean_per_day / f64::from(k);
        cum += pk;
    }
    Some((after_day.saturating_add(gap), k))
}

// ---- defences (I21) -----------------------------------------------------------------------------

/// A real outcome against one class, normalised for the observation cap.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Observed {
    pub strength: f64,
    /// The attackers' surviving share: what the abstract outcome is compared with.
    pub survived: f64,
}

impl Observed {
    pub fn from_fight(f: &FightObserved) -> Option<Observed> {
        if f.attackers == 0 {
            return None;
        }
        let killed = f.attackers_killed.min(f.attackers);
        Some(Observed {
            strength: f64::from(f.attacker_strength),
            survived: 1.0 - f64::from(killed) / f64::from(f.attackers),
        })
    }
}

/// A district's defence vector (I21) and its fight record.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Defence {
    pub survey: Option<SurveyFacts>,
    pub exposed: Vec<ExposedBlockRow>,
    pub observed: [Vec<Observed>; ThreatClass::COUNT],
    /// No attack of a class lands before this tick (I25).
    #[serde(skip)]
    pub cooldown_until: [Tick; ThreatClass::COUNT],
    pub pressure: Pressure,
}

/// The survey's numbers, kept apart from the record.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SurveyFacts {
    pub mob_griefing: bool,
    pub perimeter_columns: u32,
    pub perimeter_gaps: u32,
    pub dark_spawnable: u32,
    pub machine_blocks: u32,
    pub roofed_machines: u32,
    pub kill_machines_at_gaps: u32,
    pub turret_strength: [f32; ThreatClass::COUNT],
    pub guards: [u32; ThreatClass::COUNT],
    pub ammo_good: Option<Good>,
    pub ammo_per_engagement: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ExposedBlockRow {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub state: u32,
    pub flags: u32,
    pub inventory: Option<u32>,
}

impl From<&ExposedBlock> for ExposedBlockRow {
    fn from(b: &ExposedBlock) -> Self {
        Self {
            x: b.pos.x,
            y: b.pos.y,
            z: b.pos.z,
            state: b.state,
            flags: b.flags.0,
            inventory: b.inventory,
        }
    }
}

impl Default for Defence {
    fn default() -> Self {
        Self {
            survey: None,
            exposed: Vec::new(),
            observed: Default::default(),
            cooldown_until: [Tick::ZERO; ThreatClass::COUNT],
            pressure: Pressure::default(),
        }
    }
}

impl Defence {
    pub fn set_survey(&mut self, s: &DefenceSurvey) {
        self.survey = Some(SurveyFacts {
            mob_griefing: s.mob_griefing,
            perimeter_columns: s.perimeter_columns,
            perimeter_gaps: s.perimeter_gaps,
            dark_spawnable: s.dark_spawnable,
            machine_blocks: s.machine_blocks,
            roofed_machines: s.roofed_machines,
            kill_machines_at_gaps: s.kill_machines_at_gaps,
            turret_strength: s.turret_strength,
            guards: s.guards,
            ammo_good: s.ammo_good,
            ammo_per_engagement: s.ammo_per_engagement,
        });
    }

    /// Records a real fight. At most 16 kept per class, newest last (TUNABLE, PLACEHOLDER).
    pub fn add_fight(&mut self, f: &FightObserved) {
        if let Some(o) = Observed::from_fight(f) {
            let v = &mut self.observed[f.class.index()];
            v.push(o);
            if v.len() > 16 {
                v.remove(0);
            }
        }
    }
}

// ---- resolution (I22) ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Law {
    /// Aimed ranged fire: strength squared.
    Square,
    /// Melee and unaimed fire.
    Linear,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Outcome {
    /// Attack strength that got past the gates.
    pub through: f64,
    pub defender: f64,
    pub law: Law,
    /// The attackers' surviving share, after the observation floor.
    pub survived: f64,
    /// What Lanchester alone gave.
    pub lanchester: f64,
    /// The best real outcome that capped it, if any.
    pub floor: Option<f64>,
    pub untested: bool,
    pub ammo_used: i64,
}

/// Resolves an attack on a district against its defence (I22). `ammo_held` is the boundary stock
/// of the turrets' ammunition good.
pub fn resolve(t: &Threat, def: &Defence, ammo_held: i64, tun: &ThreatTunables) -> Outcome {
    let c = t.class;
    let s = def.survey;
    // Gates: walls stop ground attackers except through gaps.
    let pass = match (c.walks(), s) {
        (true, Some(s)) if s.perimeter_columns > 0 => {
            (f64::from(s.perimeter_gaps) * tun.gap_passage_per_column).min(1.0)
        }
        _ => 1.0,
    };
    let through = t.strength * pass;
    // Defenders: turrets (if fed), guards, and kill machines at gaps for ground melee.
    let mut ammo_used = 0i64;
    let mut turret = 0.0;
    let mut other = 0.0;
    if let Some(s) = s {
        let need = i64::from(s.ammo_per_engagement);
        let fed = s.ammo_good.is_none() || need == 0 || ammo_held >= need;
        if fed {
            turret = f64::from(s.turret_strength[c.index()]);
            if s.ammo_good.is_some() && turret > 0.0 {
                ammo_used = need;
            }
        }
        other += f64::from(s.guards[c.index()]);
        if c == ThreatClass::GroundMelee {
            other += f64::from(s.kill_machines_at_gaps) * pass;
        }
    }
    let untested = def.observed[c.index()].is_empty();
    let weight = if untested { tun.untested_weight } else { 1.0 };
    let defender = (turret + other) * weight;
    let law = if turret > other { Law::Square } else { Law::Linear };
    let (a, d) = (tun.attacker_effectiveness, tun.defender_effectiveness);
    let lanchester = if through <= 0.0 {
        0.0
    } else {
        match law {
            // a·A² vs d·D²: survivors sqrt(A² - (d/a)·D²).
            Law::Square => {
                let left = through * through - (d / a) * defender * defender;
                if left <= 0.0 { 0.0 } else { left.sqrt() / through }
            }
            Law::Linear => {
                let left = through - (d / a) * defender;
                if left <= 0.0 { 0.0 } else { left / through }
            }
        }
    };
    // Never better than observed: the floor is the best real outcome at comparable strength.
    let k = tun.comparable_strength_factor.max(1.0);
    let floor = def.observed[c.index()]
        .iter()
        .filter(|o| o.strength >= t.strength / k && o.strength <= t.strength * k)
        .map(|o| o.survived)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));
    // Scale by the share that got through: an attack stopped at the wall does nothing.
    let mut survived = lanchester * pass;
    if let Some(f) = floor {
        survived = survived.max(f);
    }
    Outcome {
        through,
        defender,
        law,
        survived: survived.clamp(0.0, 1.0),
        lanchester,
        floor,
        untested,
        ammo_used,
    }
}

// ---- damage (I23) -------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Damage {
    pub survivors: u32,
    pub stolen: GoodVec,
    pub destroyed: Vec<ExposedBlockRow>,
    pub machine_lost: bool,
}

/// Damage from an outcome (I23): theft to carry and cap; exposed blocks to the cap; any machine
/// lost is flagged. Deterministic: block choice is ordered by gate, then by a hash of the attack
/// id and position.
pub fn damage(t: &Threat, o: &Outcome, stock: &GoodVec, def: &Defence, tun: &ThreatTunables) -> Damage {
    let c = t.class.index();
    let survivors = (f64::from(t.count) * o.survived + 0.5).floor().min(f64::from(t.count)) as u32;
    let mut out = Damage {
        survivors,
        ..Damage::default()
    };
    if survivors == 0 {
        return out;
    }
    // Theft.
    let carry = i64::from(survivors).saturating_mul(tun.carry_per_attacker[c]);
    if carry > 0 {
        let goods: Vec<(Good, i64)> = stock
            .iter()
            .map(|(g, a)| (g, (i128::from(a.max(0)) * i128::from(tun.theft_cap_ppm) / 1_000_000) as i64))
            .filter(|e| e.1 > 0)
            .collect();
        let caps: Vec<i64> = goods.iter().map(|e| e.1).collect();
        let total: i64 = caps.iter().sum();
        for (k, share) in apportion(carry.min(total), &caps).into_iter().enumerate() {
            out.stolen.add(goods[k].0, share);
        }
    }
    // Blocks. Natural mobs break blocks only with mobGriefing on (vanilla).
    let griefing = def.survey.is_some_and(|s| s.mob_griefing);
    let may_break = t.source != ThreatSource::Natural || griefing;
    let n = if may_break {
        (survivors.saturating_mul(tun.blocks_per_attacker[c])).min(tun.destroy_cap_blocks)
    } else {
        0
    };
    if n > 0 && !def.exposed.is_empty() {
        let first = if t.class.walks() { BlockFlags::PERIMETER } else { BlockFlags::ROOFLESS };
        let mut order: Vec<(bool, u64, ExposedBlockRow)> = def
            .exposed
            .iter()
            .map(|b| {
                let h = rtsim_kernel::rng::fnv1a64(
                    &[t.id.to_le_bytes(), (b.x as i64).to_le_bytes(), (b.y as i64 ^ ((b.z as i64) << 20)).to_le_bytes()].concat(),
                );
                (b.flags & first == 0, h, *b)
            })
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        out.destroyed = order.into_iter().take(n as usize).map(|e| e.2).collect();
        out.machine_lost = out.destroyed.iter().any(|b| b.flags & BlockFlags::MACHINE != 0);
    }
    out
}

/// Loot from a train leg (I24): the survivors' carry, capped per good like district theft.
pub fn loot(t: &Threat, cargo: &GoodVec, tun: &ThreatTunables) -> GoodVec {
    let o = Outcome {
        through: t.strength,
        defender: 0.0,
        law: Law::Linear,
        survived: 1.0,
        lanchester: 1.0,
        floor: None,
        untested: true,
        ammo_used: 0,
    };
    damage(t, &o, cargo, &Defence::default(), tun).stolen
}

// ---- the leg index (A15) ------------------------------------------------------------------------

/// Region size in blocks (SCALE.md §5: 512 × 512, Minecraft's region files).
pub const REGION_BLOCKS: i32 = 512;
/// Time-window size of the leg index, in ticks. TUNABLE, PLACEHOLDER one day.
pub const LEG_WINDOW_TICKS: u64 = TICKS_PER_DAY;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Leg {
    pub train: TrainId,
    pub from: StationId,
    pub to: Option<StationId>,
    #[serde(serialize_with = "crate::ser::tick")]
    pub depart: Tick,
    #[serde(serialize_with = "crate::ser::tick")]
    pub eta: Tick,
    /// Per carriage: (index, cargo).
    pub cargo: Vec<(u32, GoodVec)>,
}

/// Booked train legs by (dimension, region, time window), so a threat leg finds what it crosses
/// with one lookup per region it passes (A15, INDUSTRY.md §13.3), never a scan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegIndex {
    legs: BTreeMap<u64, Leg>,
    by_cell: BTreeMap<(u32, i32, i32, u64), Vec<u64>>,
    next: u64,
}

impl LegIndex {
    /// Books a leg along the straight line between two block positions (x, z). Returns its id.
    pub fn book(&mut self, leg: Leg, dim: u32, from: (i32, i32), to: (i32, i32)) -> u64 {
        let id = self.next;
        self.next += 1;
        let (r0, r1) = (
            (from.0.div_euclid(REGION_BLOCKS), from.1.div_euclid(REGION_BLOCKS)),
            (to.0.div_euclid(REGION_BLOCKS), to.1.div_euclid(REGION_BLOCKS)),
        );
        let steps = (r1.0 - r0.0).abs().max((r1.1 - r0.1).abs()).max(1);
        let mut cells = Vec::new();
        for s in 0..=steps {
            let x = r0.0 + (r1.0 - r0.0) * s / steps;
            let z = r0.1 + (r1.1 - r0.1) * s / steps;
            cells.push((x, z));
        }
        cells.dedup();
        let (w0, w1) = (leg.depart.0 / LEG_WINDOW_TICKS, leg.eta.0 / LEG_WINDOW_TICKS);
        for (x, z) in cells {
            for w in w0..=w1 {
                self.by_cell.entry((dim, x, z, w)).or_default().push(id);
            }
        }
        self.legs.insert(id, leg);
        id
    }

    /// Legs booked in a region during a window that overlaps `[from, to]`.
    pub fn in_region(&self, dim: u32, region: (i32, i32), from: Tick, to: Tick) -> Vec<u64> {
        let mut out = Vec::new();
        for w in from.0 / LEG_WINDOW_TICKS..=to.0 / LEG_WINDOW_TICKS {
            if let Some(v) = self.by_cell.get(&(dim, region.0, region.1, w)) {
                for &id in v {
                    let l = &self.legs[&id];
                    if l.depart <= to && l.eta >= from && !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    pub fn get(&self, id: u64) -> Option<&Leg> {
        self.legs.get(&id)
    }

    /// The most recent leg of a train that is under way at `at`.
    pub fn leg_of(&self, train: TrainId, at: Tick) -> Option<u64> {
        self.legs
            .iter()
            .rev()
            .find(|(_, l)| l.train == train && l.depart <= at && l.eta >= at)
            .map(|(&id, _)| id)
    }

    /// Takes looted cargo off a booked leg.
    pub fn take_cargo(&mut self, id: u64, carriage: u32, g: Good, n: i64) {
        if let Some(l) = self.legs.get_mut(&id)
            && let Some(c) = l.cargo.iter_mut().find(|c| c.0 == carriage)
        {
            c.1.add(g, -n.min(c.1.get(g)));
        }
    }

    /// Drops legs that ended before `before`, with their cells. Cost: the legs dropped.
    pub fn expire(&mut self, before: Tick) {
        let old: Vec<u64> = self
            .legs
            .iter()
            .filter(|(_, l)| l.eta < before)
            .map(|(&id, _)| id)
            .collect();
        if old.is_empty() {
            return;
        }
        for id in &old {
            self.legs.remove(id);
        }
        self.by_cell.retain(|_, v| {
            v.retain(|id| !old.contains(id));
            !v.is_empty()
        });
    }

    pub fn len(&self) -> usize {
        self.legs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.legs.is_empty()
    }
}

// ---- the synthetic source -----------------------------------------------------------------------

const SYNTH: SystemId = SystemId::named("industry.synthetic_threats");

/// One synthetic attacker stream: attacks on a target at a mean interval.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SynthStream {
    pub source: ThreatSource,
    pub target: ThreatTarget,
    pub class: ThreatClass,
    pub mean_interval_ticks: u64,
    pub count: (u32, u32),
    pub strength_per_attacker: f64,
    /// When the stream starts.
    #[serde(serialize_with = "crate::ser::tick")]
    pub start: Tick,
}

/// A deterministic threat source for tests and `rtsim-cli sim industry`: attack n of stream s comes
/// at a gap drawn from the counter RNG keyed by (s, n). Scaffolding, to be deleted once the dragon
/// and conflict modules send real threats.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntheticThreats {
    world_seed: u64,
    streams: Vec<(SynthStream, u64, Tick)>,
    next_id: u64,
}

impl SyntheticThreats {
    pub fn new(world_seed: u64, streams: Vec<SynthStream>) -> Self {
        let mut s = Self {
            world_seed,
            streams: Vec::new(),
            next_id: 1 << 48,
        };
        for (i, st) in streams.into_iter().enumerate() {
            let first = s.gap(i as u64, 0, st.mean_interval_ticks);
            let start = st.start;
            s.streams.push((st, 0, start + first));
        }
        s
    }

    fn gap(&self, stream: u64, n: u64, mean: u64) -> u64 {
        let mut rng = StreamKey::new(self.world_seed, SYNTH, stream).slot(n);
        let u = rng.next_f64_open();
        (-(u.ln()) * mean as f64).ceil() as u64
    }
}

impl ThreatFeed for SyntheticThreats {
    fn due(&mut self, from: Tick, to: Tick, out: &mut Vec<Threat>) {
        let mut found = Vec::new();
        for i in 0..self.streams.len() {
            loop {
                let (st, n, at) = self.streams[i].clone();
                if at >= to {
                    break;
                }
                if at >= from {
                    let mut rng = StreamKey::new(self.world_seed, SYNTH, i as u64 | 1 << 32).slot(n);
                    let span = u64::from(st.count.1.saturating_sub(st.count.0)) + 1;
                    let count = st.count.0 + rng.below(span) as u32;
                    found.push(Threat {
                        id: self.next_id,
                        source: st.source,
                        class: st.class,
                        count,
                        strength: f64::from(count) * st.strength_per_attacker,
                        target: st.target,
                        at,
                    });
                    self.next_id += 1;
                }
                let g = self.gap(i as u64, n + 1, st.mean_interval_ticks).max(1);
                self.streams[i] = (st, n + 1, at + g);
            }
        }
        found.sort_by_key(|t| (t.at, t.id));
        out.extend(found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn survey(gaps: u32, turrets: f32, guards: u32) -> DefenceSurvey {
        DefenceSurvey {
            district: DistrictId(1),
            revision: 1,
            mob_griefing: true,
            perimeter_columns: 100,
            perimeter_gaps: gaps,
            dark_spawnable: 50,
            machine_blocks: 10,
            roofed_machines: 0,
            kill_machines_at_gaps: 0,
            turret_strength: [turrets; 4],
            guards: [guards; 4],
            ammo_good: None,
            ammo_per_engagement: 0,
        }
    }

    fn threat(class: ThreatClass, count: u32) -> Threat {
        Threat {
            id: 7,
            source: ThreatSource::Bandit,
            class,
            count,
            strength: f64::from(count),
            target: ThreatTarget::District(DistrictId(1)),
            at: Tick(1000),
        }
    }

    #[test]
    fn a_closed_wall_stops_ground_attackers() {
        let mut d = Defence::default();
        d.set_survey(&survey(0, 0.0, 0));
        let o = resolve(&threat(ThreatClass::GroundMelee, 10), &d, 0, &ThreatTunables::default());
        assert_eq!(o.survived, 0.0);
        let o = resolve(&threat(ThreatClass::Flying, 10), &d, 0, &ThreatTunables::default());
        assert_eq!(o.survived, 1.0, "walls do nothing against flyers");
    }

    #[test]
    fn turrets_use_the_square_law() {
        let mut d = Defence::default();
        d.set_survey(&survey(100, 8.0, 0));
        let tun = ThreatTunables {
            untested_weight: 1.0,
            ..ThreatTunables::default()
        };
        let o = resolve(&threat(ThreatClass::Flying, 10), &d, 0, &tun);
        assert_eq!(o.law, Law::Square);
        assert!((o.survived - 0.6).abs() < 1e-9, "sqrt(100 - 64)/10, got {}", o.survived);
    }

    #[test]
    fn natural_mobs_break_nothing_without_griefing() {
        let mut d = Defence::default();
        let mut s = survey(100, 0.0, 0);
        s.mob_griefing = false;
        d.set_survey(&s);
        d.exposed = vec![ExposedBlockRow { x: 0, y: 0, z: 0, state: 1, flags: 5, inventory: None }];
        let mut t = threat(ThreatClass::Siege, 5);
        t.source = ThreatSource::Natural;
        let tun = ThreatTunables::default();
        let o = resolve(&t, &d, 0, &tun);
        let dmg = damage(&t, &o, &GoodVec::new(), &d, &tun);
        assert!(dmg.destroyed.is_empty());
    }

    #[test]
    fn pressure_draws_are_deterministic_and_zero_when_lit() {
        assert_eq!(next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.0, 5), None);
        let a = next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.3, 5);
        assert_eq!(a, next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.3, 5));
        let (day, k) = a.unwrap();
        assert!(day >= 5 && k >= 1);
    }

    #[test]
    fn pressure_matches_its_mean() {
        // Attacks per night over many draws approach P(K >= 1) = 1 - e^-m.
        let m = 0.2;
        let mut day = 0u64;
        let mut attacks = 0u64;
        while day < 200_000 {
            let (d, _) = next_pressure_attack(9, DistrictId(3), ThreatClass::GroundMelee, m, day).unwrap();
            attacks += 1;
            day = d + 1;
        }
        let rate = attacks as f64 / 200_000.0;
        let want = -(-m as f64).exp_m1();
        assert!((rate - want).abs() < 0.01, "rate {rate}, want {want}");
    }

    #[test]
    fn legs_are_found_by_region_and_window() {
        let mut idx = LegIndex::default();
        let leg = Leg {
            train: TrainId(1),
            from: StationId(1),
            to: Some(StationId(2)),
            depart: Tick(100),
            eta: Tick(5_000),
            cargo: vec![(0, GoodVec::from_pairs([(Good(1), 640)]))],
        };
        let id = idx.book(leg, 0, (0, 0), (2_000, 0));
        assert_eq!(idx.in_region(0, (2, 0), Tick(0), Tick(10_000)), vec![id]);
        assert!(idx.in_region(0, (0, 5), Tick(0), Tick(10_000)).is_empty());
        assert!(idx.in_region(0, (2, 0), Tick(6_000), Tick(7_000)).is_empty());
    }

    #[test]
    fn synthetic_threats_are_reproducible() {
        let st = SynthStream {
            source: ThreatSource::Synthetic,
            target: ThreatTarget::District(DistrictId(1)),
            class: ThreatClass::Siege,
            mean_interval_ticks: 10_000,
            count: (2, 6),
            strength_per_attacker: 1.0,
            start: Tick(0),
        };
        let mut a = SyntheticThreats::new(4, vec![st.clone()]);
        let mut b = SyntheticThreats::new(4, vec![st]);
        let (mut va, mut vb) = (Vec::new(), Vec::new());
        a.due(Tick(0), Tick(500_000), &mut va);
        // Asking in pieces gives the same attacks.
        b.due(Tick(0), Tick(123_456), &mut vb);
        b.due(Tick(123_456), Tick(500_000), &mut vb);
        assert_eq!(va, vb);
        assert!(va.len() > 20);
    }

    proptest! {
        /// I22: the abstract outcome is never better for the defender than the best comparable
        /// real outcome, whatever the defences.
        #[test]
        fn never_better_than_observed(
            count in 1u32..200,
            gaps in 0u32..100,
            turrets in 0.0f32..300.0,
            guards in 0u32..50,
            fights in prop::collection::vec((0.2f64..5.0, 0u32..100, 1u32..100), 1..6),
            class in 0usize..4,
        ) {
            let class = ThreatClass::ALL[class];
            let mut d = Defence::default();
            d.set_survey(&survey(gaps, turrets, guards));
            for &(scale, killed, attackers) in &fights {
                d.add_fight(&FightObserved {
                    district: DistrictId(1),
                    class,
                    attackers,
                    attacker_strength: (f64::from(count) * scale) as f32,
                    attackers_killed: killed.min(attackers),
                    blocks_destroyed: 0,
                    machines_destroyed: 0,
                    guards_lost: 0,
                    items_stolen: 0,
                    duration_ticks: 100,
                });
            }
            let tun = ThreatTunables::default();
            let t = threat(class, count);
            let o = resolve(&t, &d, 1_000, &tun);
            let k = tun.comparable_strength_factor;
            for ob in &d.observed[class.index()] {
                if ob.strength >= t.strength / k && ob.strength <= t.strength * k {
                    prop_assert!(o.survived + 1e-12 >= ob.survived.min(o.floor.unwrap()));
                    prop_assert!(o.survived + 1e-12 >= o.floor.unwrap());
                }
            }
        }

        /// I23: theft never exceeds the carry or the per-good cap; blocks never exceed the cap.
        #[test]
        fn damage_caps_hold(
            count in 1u32..500,
            stock in prop::collection::vec((1u32..20, 1i64..100_000), 0..10),
            blocks in 0usize..40,
            class in 0usize..4,
        ) {
            let class = ThreatClass::ALL[class];
            let tun = ThreatTunables::default();
            let mut d = Defence::default();
            d.set_survey(&survey(100, 0.0, 0));
            d.exposed = (0..blocks).map(|i| ExposedBlockRow { x: i as i32, y: 0, z: 0, state: 1, flags: (i % 8) as u32, inventory: None }).collect();
            let stock = GoodVec::from_pairs(stock.into_iter().map(|(g, a)| (Good(g), a)));
            let t = threat(class, count);
            let o = resolve(&t, &d, 0, &tun);
            let dmg = damage(&t, &o, &stock, &d, &tun);
            prop_assert!(dmg.stolen.total() <= i128::from(dmg.survivors) * i128::from(tun.carry_per_attacker[class.index()]));
            for (g, n) in dmg.stolen.iter() {
                prop_assert!(i128::from(n) * 1_000_000 <= i128::from(stock.get(g)) * i128::from(tun.theft_cap_ppm));
            }
            prop_assert!(dmg.destroyed.len() as u32 <= tun.destroy_cap_blocks);
            prop_assert!(dmg.survivors <= count);
        }
    }
}
