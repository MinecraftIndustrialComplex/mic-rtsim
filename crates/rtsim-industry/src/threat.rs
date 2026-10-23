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

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::{StreamKey, SystemId};
use serde::Serialize;

use crate::ids::{DistrictId, Good, GoodVec, StationId, TrainId};
use crate::obs::{BlockFlags, DefenceSurvey, ExposedBlock, FightObserved, SpawnSample};
use crate::reconcile::apportion;
use crate::tunables::ThreatTunables;

/// The threat classes: the four of INDUSTRY.md §13.2, and burrowing (OILGATE.md §4.6, OG-9e).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ThreatClass {
    GroundMelee,
    GroundRanged,
    Flying,
    Siege,
    /// An oil nest's worms (OG-9e), sent by the dragons' engine only: they come up from below, so no
    /// wall, turret or guard stands in their way, and they break underground machines first. A machine on
    /// an armoured floor (`BlockFlags::ARMOURED`) is out of their reach. No entity is surveyed as
    /// burrowing: a worm on the surface fights as ground melee.
    Burrowing,
}

impl ThreatClass {
    pub const COUNT: usize = 5;
    pub const ALL: [ThreatClass; 5] = [
        ThreatClass::GroundMelee,
        ThreatClass::GroundRanged,
        ThreatClass::Flying,
        ThreatClass::Siege,
        ThreatClass::Burrowing,
    ];
    /// The classes the bridge surveys and samples: the SPAWNS, TURRET_STRENGTH and GUARDS arrays of
    /// the boundary carry these, in this order, and no more (docs/industry-boundary.md §3.2).
    pub const SURVEYED: usize = 4;
    pub const SURVEYED_ALL: [ThreatClass; 4] = [
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
        !matches!(self, ThreatClass::Flying | ThreatClass::Burrowing)
    }

    /// Flyers meet the roof gate (OG-11).
    pub const fn flies(self) -> bool {
        matches!(self, ThreatClass::Flying)
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
    /// An oil nest of the dragons' engine (OG-9e).
    Nest,
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
    /// What the attackers carry off and break between them, when the source knows its units (a
    /// conflict war band: its units' carry and blocks; overnight choice C2-1, K2-13's
    /// recommendation). `None` takes the per-class tunables (`carry_per_attacker`,
    /// `blocks_per_attacker`), as natural mobs, dragons and ships do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loads: Option<Loads>,
}

/// A threat's own carry and block breaking ([`Threat::loads`]): totals over all its attackers,
/// of which the survivors' share applies (I23), floored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Loads {
    /// Items all the attackers together carry off.
    pub carry: i64,
    /// Blocks all the attackers together break.
    pub blocks: u32,
}

impl Loads {
    /// `total`'s share for `survivors` of `count` attackers, floored.
    pub fn share(total: i64, survivors: u32, count: u32) -> i64 {
        if count == 0 {
            return 0;
        }
        (i128::from(total.max(0)) * i128::from(survivors.min(count)) / i128::from(count)) as i64
    }
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
    pub spawns: [u64; ThreatClass::SURVEYED],
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
            self.spawns.get(c.index()).map_or(0.0, |&n| n as f64) / self.block_ticks
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
    if mean_per_day.is_nan() || mean_per_day <= 0.0 || !mean_per_day.is_finite() {
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
    // K given K >= 1: inversion of the zero-truncated Poisson for small means; for large ones
    // (where e^-mean underflows) a binomial of the same mean, which zero almost never hits.
    let k = if mean_per_day > 50.0 {
        let n = 1u64 << 40;
        rng.binomial(n, mean_per_day / n as f64)
            .clamp(1, u64::from(u32::MAX)) as u32
    } else {
        let u = rng.next_f64_open() * p;
        let mut k = 1u32;
        let mut pk = mean_per_day * (-mean_per_day).exp();
        let mut cum = pk;
        while cum < u && k < 1_000 {
            k += 1;
            pk *= mean_per_day / f64::from(k);
            cum += pk;
        }
        k
    };
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
    pub turret_strength: [f32; ThreatClass::SURVEYED],
    pub guards: [u32; ThreatClass::SURVEYED],
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

impl ExposedBlockRow {
    /// The chunk the block is in.
    pub fn chunk(&self) -> (i32, i32) {
        (self.x >> 4, self.z >> 4)
    }
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

    /// One IND_SURVEY_BLOCKS part (the oil gate audit's F5): part 0 replaces the rows of its chunk
    /// (a partial survey, which has no walk, keeps the chunk's wall faces), every part adds its own.
    /// The rows of other chunks stay until their chunk is surveyed again, so a district larger than
    /// the loaded area keeps an exposed set.
    pub fn survey_blocks(
        &mut self,
        chunk: (i32, i32),
        part: u32,
        partial: bool,
        blocks: &[ExposedBlock],
    ) {
        if part == 0 {
            self.exposed.retain(|b| {
                b.chunk() != chunk || (partial && b.flags & BlockFlags::PERIMETER != 0)
            });
        }
        self.exposed
            .extend(blocks.iter().map(ExposedBlockRow::from));
    }

    /// Drops the rows of chunks that are no longer the district's. An empty set (the engine was
    /// never told the district's chunks) keeps them all.
    pub fn keep_chunks(&mut self, chunks: &BTreeSet<(i32, i32)>) {
        if !chunks.is_empty() {
            self.exposed.retain(|b| chunks.contains(&b.chunk()));
        }
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
    /// The roof gate (INDUSTRY.md §13.6, OILGATE.md OG-11): for a flying attack, the share of the
    /// district's machines with sky access, which scales how many blocks it may break; 1 for a
    /// ground attack, for a district not surveyed and for one with no machines. A roof shields
    /// blocks only: theft and the fight itself do not see it.
    pub roof_pass: f64,
}

/// The roof gate of a flying attack: the unroofed share of the surveyed machines, 1 when there is
/// no survey or no machine (OG-11).
pub fn roof_pass(c: ThreatClass, s: Option<SurveyFacts>) -> f64 {
    match (c.flies(), s) {
        (true, Some(s)) if s.machine_blocks > 0 => {
            let open = s.machine_blocks.saturating_sub(s.roofed_machines);
            f64::from(open) / f64::from(s.machine_blocks)
        }
        _ => 1.0,
    }
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
        // Burrowing: no surveyed defence stands in a worm's way (the arrays stop at SURVEYED).
        if fed {
            turret = f64::from(s.turret_strength.get(c.index()).copied().unwrap_or(0.0));
            if s.ammo_good.is_some() && turret > 0.0 {
                ammo_used = need;
            }
        }
        other += f64::from(s.guards.get(c.index()).copied().unwrap_or(0));
        if c == ThreatClass::GroundMelee {
            other += f64::from(s.kill_machines_at_gaps) * pass;
        }
    }
    // Untested: no real fight against this class at comparable strength (a skirmish with one
    // zombie does not vouch for the walls against a horde).
    let k = tun.comparable_strength_factor.max(1.0);
    let comparable = |o: &&Observed| o.strength >= t.strength / k && o.strength <= t.strength * k;
    let untested = !def.observed[c.index()].iter().any(|o| comparable(&o));
    let weight = if untested { tun.untested_weight } else { 1.0 };
    let defender = (turret + other) * weight;
    let law = if turret > other {
        Law::Square
    } else {
        Law::Linear
    };
    let (a, d) = (tun.attacker_effectiveness, tun.defender_effectiveness);
    let lanchester = if through <= 0.0 {
        0.0
    } else {
        match law {
            // a·A² vs d·D²: survivors sqrt(A² - (d/a)·D²).
            Law::Square => {
                let left = through * through - (d / a) * defender * defender;
                if left <= 0.0 {
                    0.0
                } else {
                    left.sqrt() / through
                }
            }
            Law::Linear => {
                let left = through - (d / a) * defender;
                if left <= 0.0 { 0.0 } else { left / through }
            }
        }
    };
    // Never better than observed: the floor is the best real outcome at comparable strength.
    let floor = def.observed[c.index()]
        .iter()
        .filter(comparable)
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
        roof_pass: roof_pass(c, s),
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
pub fn damage(
    t: &Threat,
    o: &Outcome,
    stock: &GoodVec,
    def: &Defence,
    tun: &ThreatTunables,
) -> Damage {
    let c = t.class.index();
    let survivors = (f64::from(t.count) * o.survived + 0.5)
        .floor()
        .min(f64::from(t.count)) as u32;
    let mut out = Damage {
        survivors,
        ..Damage::default()
    };
    if survivors == 0 {
        return out;
    }
    // Theft. Natural mobs take nothing from containers (vanilla), whatever their class.
    let carry = match t.loads {
        _ if t.source == ThreatSource::Natural => 0,
        Some(l) => Loads::share(l.carry, survivors, t.count),
        None => i64::from(survivors).saturating_mul(tun.carry_per_attacker[c]),
    };
    if carry > 0 {
        let goods: Vec<(Good, i64)> = stock
            .iter()
            .map(|(g, a)| {
                (
                    g,
                    (i128::from(a.max(0)) * i128::from(tun.theft_cap_ppm) / 1_000_000) as i64,
                )
            })
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
        let n = match t.loads {
            Some(l) => {
                u32::try_from(Loads::share(i64::from(l.blocks), survivors, t.count)).unwrap_or(0)
            }
            None => survivors.saturating_mul(tun.blocks_per_attacker[c]),
        };
        n.min(tun.destroy_cap_blocks)
    } else {
        0
    };
    // The roof gate (OG-11): a flyer breaks only blocks with sky access, and fewer of them as more
    // of the district's machines are roofed.
    let flies = t.class.flies();
    let burrows = t.class == ThreatClass::Burrowing;
    let n = if flies {
        (f64::from(n) * o.roof_pass.clamp(0.0, 1.0)).floor() as u32
    } else {
        n
    };
    if n > 0 && !def.exposed.is_empty() {
        // Walkers take the perimeter first, flyers what the sky reaches, and a nest's worms (OG-9e) the
        // underground machines and inventories first, then the machines, never one on an armoured floor.
        let (first, second) = if burrows {
            (BlockFlags::SUBTERRANEAN, BlockFlags::MACHINE)
        } else if t.class.walks() {
            (BlockFlags::PERIMETER, 0)
        } else {
            (BlockFlags::ROOFLESS, 0)
        };
        let reach = |b: &&ExposedBlockRow| {
            if burrows {
                b.flags & (BlockFlags::MACHINE | BlockFlags::INVENTORY) != 0
                    && b.flags & BlockFlags::ARMOURED == 0
            } else {
                !flies || b.flags & BlockFlags::ROOFLESS != 0
            }
        };
        let mut order: Vec<(bool, bool, u64, ExposedBlockRow)> = def
            .exposed
            .iter()
            .filter(reach)
            .map(|b| {
                let h = rtsim_kernel::rng::fnv1a64(
                    &[
                        t.id.to_le_bytes(),
                        (b.x as i64).to_le_bytes(),
                        (b.y as i64 ^ ((b.z as i64) << 20)).to_le_bytes(),
                    ]
                    .concat(),
                );
                (b.flags & first == 0, second != 0 && b.flags & second == 0, h, *b)
            })
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        out.destroyed = order.into_iter().take(n as usize).map(|e| e.3).collect();
        out.machine_lost = out
            .destroyed
            .iter()
            .any(|b| b.flags & BlockFlags::MACHINE != 0);
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
        roof_pass: 1.0,
    };
    damage(t, &o, cargo, &Defence::default(), tun).stolen
}

// ---- the leg index (A15) ------------------------------------------------------------------------

/// Region size in blocks (SCALE.md §5: 512 × 512, Minecraft's region files).
pub const REGION_BLOCKS: i32 = 512;
/// Time-window size of the leg index, in ticks. TUNABLE, PLACEHOLDER one day.
pub const LEG_WINDOW_TICKS: u64 = TICKS_PER_DAY;
/// The longest a booked leg lasts, in ticks (30 days). TUNABLE, PLACEHOLDER. A booking indexes one
/// entry per cell for every window of its span, so a train that stalls (an ETA of distance over a
/// near-zero speed) must not book years of windows; a restore refuses a longer one, so a corrupt
/// page cannot ask for an unbounded index (ID10).
pub const LEG_MAX_TICKS: u64 = 30 * TICKS_PER_DAY;

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

/// Legs per page of the leg table (`persist::LEG_TABLE`, ID10): the page of a leg is its id divided
/// by this, so a save rewrites the page being filled and the pages a looted or expired leg sits in,
/// never every leg. TUNABLE: a page is about 256 legs of a few dozen bytes.
pub const LEGS_PER_PAGE: u64 = 256;

/// A leg with where it was booked: the dimension and the regions its line crosses, in the order
/// they were booked, so the cell and train indexes are rebuilt from the legs alone ([`LegIndex::from_pages`]).
#[derive(Clone, Debug, PartialEq)]
struct Booked {
    leg: Leg,
    dim: u32,
    cells: Vec<(i32, i32)>,
}

/// Booked train legs by (dimension, region, time window), so a threat leg finds what it crosses
/// with one lookup per region it passes (A15, INDUSTRY.md §13.3), never a scan.
///
/// The legs are the state; they are saved in pages of [`LEGS_PER_PAGE`] ids (ID10), which the
/// index marks as it changes ([`LegIndex::take_changed`]). The two lookups are rebuilt from the
/// legs on a restore.
#[derive(Clone, Debug, Default)]
pub struct LegIndex {
    legs: BTreeMap<u64, Booked>,
    by_cell: BTreeMap<(u32, i32, i32, u64), Vec<u64>>,
    /// Each train's legs, oldest first, so finding a train's leg never scans every leg.
    by_train: BTreeMap<TrainId, Vec<u64>>,
    next: u64,
    /// Pages changed since the last [`LegIndex::take_changed`]. Not part of equality.
    changed: BTreeSet<u64>,
}

impl PartialEq for LegIndex {
    fn eq(&self, o: &Self) -> bool {
        self.legs == o.legs
            && self.by_cell == o.by_cell
            && self.by_train == o.by_train
            && self.next == o.next
    }
}

impl LegIndex {
    /// Books a leg along the straight line between two block positions (x, z). Returns its id.
    pub fn book(&mut self, leg: Leg, dim: u32, from: (i32, i32), to: (i32, i32)) -> u64 {
        self.book_on(&rtsim_space::topology::Topology::flat(), leg, dim, from, to)
    }

    /// [`LegIndex::book`] on a world with seams (docs/choices-topology.md TP16): the line goes the
    /// short way, from `from` to the image of `to` nearest it, and every region it passes is
    /// keyed canonically, so a line across a seam is booked where it runs. On the plane, the
    /// same cells as `book`.
    pub fn book_on(
        &mut self,
        topo: &rtsim_space::topology::Topology,
        mut leg: Leg,
        dim: u32,
        from: (i32, i32),
        to: (i32, i32),
    ) -> u64 {
        // A leg lasts at most `LEG_MAX_TICKS` and ends after it began (what a restore accepts).
        let latest = Tick(leg.depart.0.saturating_add(LEG_MAX_TICKS));
        leg.eta = leg.eta.max(leg.depart).min(latest);
        if !topo.is_flat() {
            let id = self.next;
            self.next += 1;
            let a = topo.canonical(i64::from(from.0), i64::from(from.1));
            let b = topo.toward(a, (i64::from(to.0), i64::from(to.1)));
            let half = i64::from(REGION_BLOCKS) / 2;
            let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()) / half).max(1);
            let mut cells: Vec<(i32, i32)> = (0..=steps)
                .map(|s| {
                    let (x, z) = topo
                        .canonical(a.0 + (b.0 - a.0) * s / steps, a.1 + (b.1 - a.1) * s / steps);
                    (
                        x.div_euclid(i64::from(REGION_BLOCKS)) as i32,
                        z.div_euclid(i64::from(REGION_BLOCKS)) as i32,
                    )
                })
                .collect();
            cells.sort_unstable();
            cells.dedup();
            self.insert(id, leg, dim, cells);
            return id;
        }
        let id = self.next;
        self.next += 1;
        let (r0, r1) = (
            (
                from.0.div_euclid(REGION_BLOCKS),
                from.1.div_euclid(REGION_BLOCKS),
            ),
            (
                to.0.div_euclid(REGION_BLOCKS),
                to.1.div_euclid(REGION_BLOCKS),
            ),
        );
        let steps = (r1.0 - r0.0).abs().max((r1.1 - r0.1).abs()).max(1);
        let mut cells = Vec::new();
        for s in 0..=steps {
            let x = r0.0 + (r1.0 - r0.0) * s / steps;
            let z = r0.1 + (r1.1 - r0.1) * s / steps;
            cells.push((x, z));
        }
        cells.dedup();
        self.insert(id, leg, dim, cells);
        id
    }

    /// Enters a leg under `id`: its cells and windows in the cell index, its train in the train
    /// index. The one path a booking and a restore both take, so a restored index is the booked one.
    fn insert(&mut self, id: u64, leg: Leg, dim: u32, cells: Vec<(i32, i32)>) {
        let (w0, w1) = (
            leg.depart.0 / LEG_WINDOW_TICKS,
            leg.eta.0 / LEG_WINDOW_TICKS,
        );
        for &(x, z) in &cells {
            for w in w0..=w1 {
                self.by_cell.entry((dim, x, z, w)).or_default().push(id);
            }
        }
        self.by_train.entry(leg.train).or_default().push(id);
        self.changed.insert(Self::page_of(id));
        self.legs.insert(id, Booked { leg, dim, cells });
    }

    /// The page a leg id is saved in (ID10).
    pub fn page_of(id: u64) -> u64 {
        id / LEGS_PER_PAGE
    }

    /// The id the next booking takes (a globals field: it outlives the legs it numbered).
    pub fn next_id(&self) -> u64 {
        self.next
    }

    /// Pages changed since the last call (booked into, looted, or emptied by an expiry): each
    /// one's page is written, or deleted when it holds no leg ([`LegIndex::page_bytes`]).
    pub fn take_changed(&mut self) -> BTreeSet<u64> {
        std::mem::take(&mut self.changed)
    }

    /// Pages waiting for the next save.
    pub fn changed(&self) -> &BTreeSet<u64> {
        &self.changed
    }

    /// Marks a page (a failed save's page, written again by the next one).
    pub fn mark(&mut self, page: u64) {
        self.changed.insert(page);
    }

    /// Legs booked in a region during a window that overlaps `[from, to]`.
    pub fn in_region(&self, dim: u32, region: (i32, i32), from: Tick, to: Tick) -> Vec<u64> {
        let mut out = Vec::new();
        for w in from.0 / LEG_WINDOW_TICKS..=to.0 / LEG_WINDOW_TICKS {
            if let Some(v) = self.by_cell.get(&(dim, region.0, region.1, w)) {
                for &id in v {
                    let l = &self.legs[&id].leg;
                    if l.depart <= to && l.eta >= from && !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    pub fn get(&self, id: u64) -> Option<&Leg> {
        self.legs.get(&id).map(|b| &b.leg)
    }

    /// Every booked leg by id, in id order (a viewer's map, VIEWER.md §7).
    pub fn iter(&self) -> impl Iterator<Item = (u64, &Leg)> {
        self.legs.iter().map(|(id, b)| (*id, &b.leg))
    }

    /// The legs with ids in `ids`, in id order.
    pub fn range(&self, ids: std::ops::Range<u64>) -> impl Iterator<Item = (u64, &Leg)> {
        self.legs.range(ids).map(|(id, b)| (*id, &b.leg))
    }

    /// The most recent leg of a train that is under way at `at`.
    pub fn leg_of(&self, train: TrainId, at: Tick) -> Option<u64> {
        self.by_train.get(&train)?.iter().rev().copied().find(|id| {
            self.legs
                .get(id)
                .is_some_and(|b| b.leg.depart <= at && b.leg.eta >= at)
        })
    }

    /// Takes looted cargo off a booked leg.
    pub fn take_cargo(&mut self, id: u64, carriage: u32, g: Good, n: i64) {
        if let Some(b) = self.legs.get_mut(&id)
            && let Some(c) = b.leg.cargo.iter_mut().find(|c| c.0 == carriage)
        {
            let k = n.min(c.1.get(g));
            if k != 0 {
                c.1.add(g, -k);
                self.changed.insert(Self::page_of(id));
            }
        }
    }

    /// Drops legs that ended before `before`, with their cells. Cost: the legs and cells held.
    pub fn expire(&mut self, before: Tick) {
        let old: BTreeSet<u64> = self
            .legs
            .iter()
            .filter(|(_, b)| b.leg.eta < before)
            .map(|(&id, _)| id)
            .collect();
        if old.is_empty() {
            return;
        }
        for id in &old {
            self.legs.remove(id);
            self.changed.insert(Self::page_of(*id));
        }
        self.by_cell.retain(|_, v| {
            v.retain(|id| !old.contains(id));
            !v.is_empty()
        });
        self.by_train.retain(|_, v| {
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
                    let mut rng =
                        StreamKey::new(self.world_seed, SYNTH, i as u64 | 1 << 32).slot(n);
                    let span = u64::from(st.count.1.saturating_sub(st.count.0)) + 1;
                    let count = st.count.0 + rng.below(span) as u32;
                    found.push(Threat {
                        id: self.next_id,
                        source: st.source,
                        class: st.class,
                        count,
                        strength: f64::from(count) * st.strength_per_attacker,
                        target: st.target,
                        loads: None,
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
            loads: None,
            at: Tick(1000),
        }
    }

    #[test]
    fn a_closed_wall_stops_ground_attackers() {
        let mut d = Defence::default();
        d.set_survey(&survey(0, 0.0, 0));
        let o = resolve(
            &threat(ThreatClass::GroundMelee, 10),
            &d,
            0,
            &ThreatTunables::default(),
        );
        assert_eq!(o.survived, 0.0);
        let o = resolve(
            &threat(ThreatClass::Flying, 10),
            &d,
            0,
            &ThreatTunables::default(),
        );
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
        assert!(
            (o.survived - 0.6).abs() < 1e-9,
            "sqrt(100 - 64)/10, got {}",
            o.survived
        );
    }

    #[test]
    fn natural_mobs_break_nothing_without_griefing() {
        let mut d = Defence::default();
        let mut s = survey(100, 0.0, 0);
        s.mob_griefing = false;
        d.set_survey(&s);
        d.exposed = vec![ExposedBlockRow {
            x: 0,
            y: 0,
            z: 0,
            state: 1,
            flags: 5,
            inventory: None,
        }];
        let mut t = threat(ThreatClass::Siege, 5);
        t.source = ThreatSource::Natural;
        let tun = ThreatTunables::default();
        let o = resolve(&t, &d, 0, &tun);
        let dmg = damage(&t, &o, &GoodVec::new(), &d, &tun);
        assert!(dmg.destroyed.is_empty());
    }

    #[test]
    fn natural_mobs_never_steal() {
        let mut d = Defence::default();
        d.set_survey(&survey(100, 0.0, 0));
        let mut t = threat(ThreatClass::Siege, 20);
        t.source = ThreatSource::Natural;
        let tun = ThreatTunables::default();
        let o = resolve(&t, &d, 0, &tun);
        let stock = GoodVec::from_pairs([(Good(1), 10_000)]);
        assert!(damage(&t, &o, &stock, &d, &tun).stolen.is_empty());
    }

    /// C2-1 (K2-13's recommendation): a threat that names its loads carries and breaks by them,
    /// the survivors' share, whatever the per-class tunables say; one that does not is unchanged.
    #[test]
    fn a_threat_with_its_own_loads_carries_and_breaks_by_them() {
        let mut d = Defence::default();
        d.set_survey(&survey(100, 0.0, 0));
        d.exposed = (0..20)
            .map(|i| ExposedBlockRow {
                x: i,
                y: 0,
                z: 0,
                state: 1,
                flags: BlockFlags::PERIMETER,
                inventory: None,
            })
            .collect();
        let tun = ThreatTunables::default();
        assert_eq!(tun.carry_per_attacker[ThreatClass::GroundMelee.index()], 0);
        let stock = GoodVec::from_pairs([(Good(1), 10_000)]);
        let mut t = threat(ThreatClass::GroundMelee, 10);
        let o = resolve(&t, &d, 0, &tun);
        let dmg = damage(&t, &o, &stock, &d, &tun);
        assert!(
            dmg.stolen.is_empty() && dmg.destroyed.is_empty(),
            "class defaults"
        );
        t.loads = Some(Loads {
            carry: 80,
            blocks: 5,
        });
        let o = Outcome {
            survived: 0.5,
            ..resolve(&t, &d, 0, &tun)
        };
        let dmg = damage(&t, &o, &stock, &d, &tun);
        assert_eq!(dmg.survivors, 5);
        assert_eq!(dmg.stolen.total(), 40, "half of 80 items");
        assert_eq!(dmg.destroyed.len(), 2, "half of 5 blocks, floored");
        assert_eq!(Loads::share(7, 3, 0), 0);
        assert_eq!(Loads::share(7, 9, 3), 7, "never more than all survive");
    }

    /// OG-11, the roof gate: a flyer breaks fewer blocks as more machines are roofed, and only
    /// blocks with sky access; a ground attacker and the theft do not see the roof.
    #[test]
    fn a_roof_shields_blocks_from_flyers_only() {
        let mut d = Defence::default();
        let mut s = survey(100, 0.0, 0);
        s.machine_blocks = 10;
        s.roofed_machines = 6;
        d.set_survey(&s);
        // Twenty blocks, the even ones under a roof.
        d.exposed = (0..20)
            .map(|i| ExposedBlockRow {
                x: i,
                y: 0,
                z: 0,
                state: 1,
                flags: BlockFlags::PERIMETER | if i % 2 == 1 { BlockFlags::ROOFLESS } else { 0 },
                inventory: None,
            })
            .collect();
        let tun = ThreatTunables::default();
        let stock = GoodVec::from_pairs([(Good(1), 10_000)]);
        let loads = Some(Loads {
            carry: 40,
            blocks: 5,
        });
        let mut fly = threat(ThreatClass::Flying, 4);
        fly.loads = loads;
        let o = resolve(&fly, &d, 0, &tun);
        assert!((o.roof_pass - 0.4).abs() < 1e-12, "{}", o.roof_pass);
        let dmg = damage(&fly, &o, &stock, &d, &tun);
        assert_eq!(dmg.destroyed.len(), 2, "5 blocks at 0.4, floored");
        assert!(
            dmg.destroyed
                .iter()
                .all(|b| b.flags & BlockFlags::ROOFLESS != 0),
            "only roofless blocks: {:?}",
            dmg.destroyed
        );
        assert_eq!(dmg.stolen.total(), 40, "the roof does not stop theft");
        // Every machine roofed: no blocks at all.
        s.roofed_machines = 10;
        d.set_survey(&s);
        let o = resolve(&fly, &d, 0, &tun);
        assert_eq!(o.roof_pass, 0.0);
        assert!(damage(&fly, &o, &stock, &d, &tun).destroyed.is_empty());
        // A ground attacker ignores the roof.
        let mut walk = threat(ThreatClass::GroundMelee, 4);
        walk.loads = loads;
        let o = Outcome {
            survived: 1.0,
            ..resolve(&walk, &d, 0, &tun)
        };
        assert_eq!(o.roof_pass, 1.0);
        assert_eq!(damage(&walk, &o, &stock, &d, &tun).destroyed.len(), 5);
        // No survey: the gate is open.
        assert_eq!(roof_pass(ThreatClass::Flying, None), 1.0);
    }

    fn block(x: i32, y: i32, z: i32, flags: u32) -> ExposedBlock {
        ExposedBlock {
            pos: crate::ids::BlockPos::new(x, y, z),
            state: 1,
            flags: BlockFlags(flags),
            inventory: None,
        }
    }

    fn row(x: i32, y: i32, z: i32, flags: u32) -> ExposedBlockRow {
        ExposedBlockRow::from(&block(x, y, z, flags))
    }

    /// OGB-7 (the oil gate audit's F4): a roof within reach is a roofless row of its own, and the
    /// machine under it does not count as roofed. A flyer takes the roof and leaves the machine,
    /// whose own row has no sky access; the next survey finds that machine open and the next raid
    /// takes it. Cover beyond reach still shields: no roof row, and the machine counts as roofed.
    #[test]
    fn a_flyer_takes_a_roof_in_reach_before_the_machine_under_it() {
        const M: u32 = BlockFlags::MACHINE;
        const OPEN: u32 = BlockFlags::ROOFLESS;
        const ROOF: u32 = BlockFlags::ROOFLESS | BlockFlags::ROOF;
        let mut d = Defence::default();
        let mut s = survey(100, 0.0, 0);
        // A machine in a shed (its roof 3 above), and one under 12 blocks of cover (shielded).
        s.machine_blocks = 2;
        s.roofed_machines = 1;
        d.set_survey(&s);
        d.survey_blocks(
            (0, 0),
            0,
            false,
            &[block(2, 64, 2, M), block(9, 64, 9, M), block(2, 67, 2, ROOF)],
        );
        let tun = ThreatTunables::default();
        let stock = GoodVec::default();
        let mut fly = threat(ThreatClass::Flying, 4);
        fly.loads = Some(Loads {
            carry: 0,
            blocks: 8,
        });
        let o = resolve(&fly, &d, 0, &tun);
        assert!((o.roof_pass - 0.5).abs() < 1e-12, "{}", o.roof_pass);
        let dmg = damage(&fly, &o, &stock, &d, &tun);
        assert_eq!(
            dmg.destroyed,
            vec![row(2, 67, 2, ROOF)],
            "the shed's roof, neither machine"
        );
        assert!(!dmg.machine_lost);
        // The next survey: the roof is gone, the shed's machine sees the sky; the deep cover holds.
        d.survey_blocks((0, 0), 0, false, &[block(2, 64, 2, M | OPEN), block(9, 64, 9, M)]);
        let mut again = fly;
        again.id += 1;
        let o = resolve(&again, &d, 0, &tun);
        let dmg = damage(&again, &o, &stock, &d, &tun);
        assert_eq!(dmg.destroyed, vec![row(2, 64, 2, M | OPEN)], "now the machine");
        assert!(dmg.machine_lost);
    }

    /// OG-9e: a nest's worms come up from below. A closed wall, turrets and guards do not stop them;
    /// they take underground machines and inventories first, then machines, never a block on an
    /// armoured floor (or a pumpjack's hole, `burrow_spared`), and carry nothing off.
    #[test]
    fn burrowers_pass_walls_and_turrets_and_spare_armoured_floors() {
        const M: u32 = BlockFlags::MACHINE;
        const I: u32 = BlockFlags::INVENTORY;
        const DEEP: u32 = BlockFlags::SUBTERRANEAN;
        const ARM: u32 = BlockFlags::ARMOURED;
        let mut d = Defence::default();
        d.set_survey(&survey(0, 50.0, 20));
        d.exposed = vec![
            row(0, 64, 0, M),
            row(1, 64, 0, I | BlockFlags::PERIMETER),
            row(2, 40, 0, M | DEEP),
            row(3, 40, 0, I | DEEP),
            row(4, 40, 0, M | DEEP | ARM),
            row(5, 64, 0, M | ARM),
            row(6, 64, 0, BlockFlags::PERIMETER),
        ];
        let tun = ThreatTunables::default();
        let mut worms = threat(ThreatClass::Burrowing, 2);
        worms.source = ThreatSource::Nest;
        let o = resolve(&worms, &d, 0, &tun);
        assert_eq!(o.survived, 1.0, "walls, turrets and guards are above them");
        assert_eq!(o.roof_pass, 1.0);
        let stock = GoodVec::from_pairs([(Good(1), 10_000)]);
        let mut big = worms;
        big.count = 20;
        let dmg = damage(&big, &resolve(&big, &d, 0, &tun), &stock, &d, &tun);
        assert!(dmg.stolen.is_empty(), "worms carry nothing");
        let mut got: Vec<(i32, u32)> = dmg.destroyed.iter().map(|b| (b.x, b.flags)).collect();
        // The underground ones first, then the machine; the armoured and the bare wall never.
        let first: Vec<i32> = got.iter().take(2).map(|e| e.0).collect();
        assert!(first.contains(&2) && first.contains(&3), "{got:?}");
        assert_eq!(got.get(2).map(|e| e.0), Some(0), "{got:?}");
        got.sort_unstable();
        assert_eq!(
            got.iter().map(|e| e.0).collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "never an armoured block or a wall"
        );
        // A walker on the same district: the closed wall stops it.
        let o = resolve(&threat(ThreatClass::GroundMelee, 2), &d, 0, &tun);
        assert_eq!(o.survived, 0.0);
    }

    /// The oil gate audit's F5: IND_SURVEY_BLOCKS replaces one chunk's rows at a time. A partial
    /// survey (some of the district's chunks unloaded) replaces the chunks it read, keeping their
    /// wall faces, and leaves every other chunk's rows; a chunk read with no rows is emptied; a later
    /// part adds to its chunk; rows of a chunk the district lost go.
    #[test]
    fn a_partial_survey_keeps_the_other_chunks_rows() {
        const WALL: u32 = BlockFlags::PERIMETER;
        const M: u32 = BlockFlags::MACHINE | BlockFlags::ROOFLESS;
        const TANK: u32 = BlockFlags::INVENTORY | BlockFlags::ROOFLESS;
        let at = |d: &Defence| {
            let mut v: Vec<(i32, i32)> = d.exposed.iter().map(|b| (b.x, b.z)).collect();
            v.sort();
            v
        };
        let mut d = Defence::default();
        // A whole survey of three chunks, one of them west of x 0.
        d.survey_blocks((0, 0), 0, false, &[block(1, 64, 1, WALL), block(5, 64, 5, M)]);
        d.survey_blocks((1, 0), 0, false, &[block(20, 64, 5, M)]);
        d.survey_blocks((-1, 0), 0, false, &[block(-3, 64, 5, TANK)]);
        assert_eq!(at(&d), vec![(-3, 5), (1, 1), (5, 5), (20, 5)]);
        // Only chunk (0, 0) loaded: its machine moved; its wall face stays (a partial survey has no
        // walk); the unloaded chunks keep theirs, so a raid there still has blocks to break.
        d.survey_blocks((0, 0), 0, true, &[block(6, 64, 6, M)]);
        assert_eq!(at(&d), vec![(-3, 5), (1, 1), (6, 6), (20, 5)]);
        let mut fly = threat(ThreatClass::Flying, 4);
        fly.loads = Some(Loads {
            carry: 0,
            blocks: 8,
        });
        let tun = ThreatTunables::default();
        let o = resolve(&fly, &d, 0, &tun);
        let broken = damage(&fly, &o, &GoodVec::default(), &d, &tun).destroyed;
        assert!(
            broken.contains(&row(20, 64, 5, M)) && broken.contains(&row(-3, 64, 5, TANK)),
            "the unloaded chunks' blocks are still there to break: {broken:?}"
        );
        // Chunk (1, 0) read again with nothing in it: emptied.
        d.survey_blocks((1, 0), 0, true, &[]);
        assert_eq!(at(&d), vec![(-3, 5), (1, 1), (6, 6)]);
        // A second part adds to its chunk.
        d.survey_blocks((-1, 0), 1, true, &[block(-4, 64, 5, TANK)]);
        assert_eq!(at(&d), vec![(-4, 5), (-3, 5), (1, 1), (6, 6)]);
        // A whole survey's part 0 replaces the wall faces too.
        d.survey_blocks((0, 0), 0, false, &[block(6, 64, 6, M)]);
        assert_eq!(at(&d), vec![(-4, 5), (-3, 5), (6, 6)]);
        // The district lost chunk (-1, 0).
        d.keep_chunks(&BTreeSet::from([(0, 0), (1, 0)]));
        assert_eq!(at(&d), vec![(6, 6)]);
        // An engine never told the district's chunks drops nothing.
        d.keep_chunks(&BTreeSet::new());
        assert_eq!(at(&d), vec![(6, 6)]);
    }

    #[test]
    fn a_large_pressure_mean_gives_a_sane_count() {
        let (_, k) =
            next_pressure_attack(3, DistrictId(1), ThreatClass::GroundMelee, 1_000.0, 0).unwrap();
        assert!((800..1_200).contains(&k), "count {k} for a mean of 1,000");
    }

    #[test]
    fn pressure_draws_are_deterministic_and_zero_when_lit() {
        assert_eq!(
            next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.0, 5),
            None
        );
        let a = next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.3, 5);
        assert_eq!(
            a,
            next_pressure_attack(1, DistrictId(1), ThreatClass::Siege, 0.3, 5)
        );
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
            let (d, _) =
                next_pressure_attack(9, DistrictId(3), ThreatClass::GroundMelee, m, day).unwrap();
            attacks += 1;
            day = d + 1;
        }
        let rate = attacks as f64 / 200_000.0;
        let want = -(-m).exp_m1();
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
        assert!(
            idx.in_region(0, (2, 0), Tick(6_000), Tick(7_000))
                .is_empty()
        );
    }

    fn booked(train: u64, depart: u64, eta: u64) -> Leg {
        Leg {
            train: TrainId(train),
            from: StationId(1),
            to: Some(StationId(2)),
            depart: Tick(depart),
            eta: Tick(eta),
            cargo: vec![(0, GoodVec::from_pairs([(Good(1), 640)]))],
        }
    }

    /// The leg pages mark exactly the pages a booking, a loot and an expiry change, and the index
    /// the pages describe is the booked one, in the order the lookups were built (ID10).
    #[test]
    fn leg_pages_follow_the_changes_and_rebuild_the_index() {
        let mut idx = LegIndex::default();
        let n = 3 * LEGS_PER_PAGE;
        for i in 0..n {
            // Day 0 for the first half, day 3 for the rest; some cross a region and a window.
            let d = if i < n / 2 {
                100
            } else {
                3 * TICKS_PER_DAY + 100
            };
            idx.book(
                booked(i % 5, d, d + TICKS_PER_DAY / 2 + i),
                0,
                (0, 0),
                (1_100 + i as i32, 0),
            );
        }
        assert_eq!(
            idx.take_changed().into_iter().collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(idx.take_changed().is_empty());
        let back = LegIndex::from_pages(idx.next_id(), idx.pages()).unwrap();
        assert_eq!(back, idx, "restored legs, cell index and train index");
        assert!(
            back.changed().is_empty(),
            "a restored index has nothing to save"
        );

        // A loot marks the page of the leg, and only when something is taken.
        let id = LEGS_PER_PAGE + 3;
        idx.take_cargo(id, 0, Good(1), 100);
        assert_eq!(idx.take_changed().into_iter().collect::<Vec<_>>(), vec![1]);
        idx.take_cargo(id, 0, Good(2), 100);
        idx.take_cargo(id, 9, Good(1), 100);
        idx.take_cargo(n + 5, 0, Good(1), 100);
        assert!(
            idx.take_changed().is_empty(),
            "nothing taken, nothing marked"
        );
        let back = LegIndex::from_pages(idx.next_id(), idx.pages()).unwrap();
        assert_eq!(back, idx);
        assert_eq!(back.get(id).unwrap().cargo[0].1.get(Good(1)), 540);

        // An expiry marks the pages it takes legs from; a page it empties has no bytes.
        idx.expire(Tick(2 * TICKS_PER_DAY));
        let first_half = n / 2;
        assert_eq!(idx.len() as u64, n - first_half);
        let marked: Vec<u64> = idx.take_changed().into_iter().collect();
        assert_eq!(
            marked,
            vec![0, 1],
            "legs 0..{first_half} sit in pages 0 and 1"
        );
        assert!(idx.page_bytes(0).is_none(), "page 0 holds no leg any more");
        assert!(idx.page_bytes(1).is_some(), "page 1 keeps its second half");
        assert!(idx.page_bytes(2).is_some());
        assert!(idx.page_bytes(3).is_none());
        let back = LegIndex::from_pages(idx.next_id(), idx.pages()).unwrap();
        assert_eq!(back, idx);
        // The lookups still answer for what is left, and a booking goes on from the counter.
        assert!(
            !back
                .in_region(0, (1, 0), Tick(3 * TICKS_PER_DAY), Tick(4 * TICKS_PER_DAY))
                .is_empty()
        );
        let mut back = back;
        assert_eq!(back.book(booked(1, 5, 9), 0, (0, 0), (0, 0)), n);
    }

    /// A line across a seam is booked where it runs and restores to the same cells.
    #[test]
    fn leg_pages_keep_the_cells_of_a_line_across_a_seam() {
        use rtsim_space::topology::{Rect, layout};
        let topo = layout::torus(Rect::new(-1_024, -1_024, 1_024, 1_024));
        let mut idx = LegIndex::default();
        for i in 0..4 {
            idx.book_on(
                &topo,
                booked(i, 10, 5_000),
                0,
                (900, 100 * i as i32),
                (-900, 100 * i as i32),
            );
        }
        let cells = idx.in_region(0, (-2, 0), Tick(0), Tick(10_000));
        assert!(
            !cells.is_empty(),
            "the short way from x 900 to x -900 crosses the seam into region -2"
        );
        let back = LegIndex::from_pages(idx.next_id(), idx.pages()).unwrap();
        assert_eq!(back, idx);
        assert_eq!(back.in_region(0, (-2, 0), Tick(0), Tick(10_000)), cells);
        assert!(
            back.in_region(0, (0, 0), Tick(0), Tick(10_000)).is_empty(),
            "the long way is not booked"
        );
    }

    /// A booking lasts at most `LEG_MAX_TICKS` (a stalled train's ETA is years), and a restore
    /// refuses a page whose leg lasts longer or ends before it began, instead of indexing every
    /// window of it (ID10).
    #[test]
    fn a_leg_is_bounded_in_time_and_a_page_with_a_longer_one_is_refused() {
        let mut idx = LegIndex::default();
        let id = idx.book(booked(1, 100, u64::MAX), 0, (0, 0), (1_100, 0));
        let leg = idx.get(id).unwrap();
        assert_eq!(
            leg.eta.0 - leg.depart.0,
            LEG_MAX_TICKS,
            "clamped, not years of windows"
        );
        assert!(
            idx.in_region(0, (0, 0), Tick(0), Tick(10)).is_empty(),
            "a window before the departure holds nothing"
        );
        let early = idx.book(booked(2, 5_000, 100), 0, (0, 0), (0, 0));
        assert_eq!(
            idx.get(early).unwrap().eta,
            Tick(5_000),
            "never before it began"
        );
        let back = LegIndex::from_pages(idx.next_id(), idx.pages()).unwrap();
        assert_eq!(back, idx);
        // A page written by something else: the same legs, one lasting past the bound or ending early.
        for (depart, eta) in [(0, u64::MAX), (0, LEG_MAX_TICKS + 1), (10, 9)] {
            let mut bad = idx.clone();
            let b = bad.legs.get_mut(&id).unwrap();
            b.leg.depart = Tick(depart);
            b.leg.eta = Tick(eta);
            assert!(
                LegIndex::from_pages(bad.next_id(), bad.pages()).is_err(),
                "a leg from tick {depart} to {eta}"
            );
        }
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

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};
    use rtsim_kernel::codec::{Reader, Result, Writer};

    persist_unit_enum!(
        ThreatClass,
        [
            ThreatClass::GroundMelee,
            ThreatClass::GroundRanged,
            ThreatClass::Flying,
            ThreatClass::Siege,
            ThreatClass::Burrowing
        ]
    );
    persist_unit_enum!(
        ThreatSource,
        [
            ThreatSource::Natural,
            ThreatSource::Dragon,
            ThreatSource::Bandit,
            ThreatSource::Boss,
            ThreatSource::Synthetic,
            ThreatSource::Nest
        ]
    );
    persist_unit_enum!(Law, [Law::Square, Law::Linear]);
    persist_struct!(Outcome {
        through,
        defender,
        law,
        survived,
        lanchester,
        floor,
        untested,
        ammo_used,
        roof_pass
    });
    persist_struct!(Observed { strength, survived });
    persist_struct!(Pressure {
        block_ticks,
        spawns
    });
    persist_struct!(SurveyFacts {
        mob_griefing,
        perimeter_columns,
        perimeter_gaps,
        dark_spawnable,
        machine_blocks,
        roofed_machines,
        kill_machines_at_gaps,
        turret_strength,
        guards,
        ammo_good,
        ammo_per_engagement,
    });
    persist_struct!(ExposedBlockRow {
        x,
        y,
        z,
        state,
        flags,
        inventory
    });
    persist_struct!(Defence {
        survey,
        exposed,
        observed,
        cooldown_until,
        pressure
    });
    persist_struct!(Leg {
        train,
        from,
        to,
        depart,
        eta,
        cargo
    });
    persist_struct!(Booked { leg, dim, cells });

    /// A leg page starts with this, so an older format fails by name.
    pub const LEG_MAGIC: u32 = u32::from_le_bytes(*b"ILEG");

    impl LegIndex {
        /// The bytes of a page of the leg table (ID10): its legs in id order, each with where it was
        /// booked. `None` for a page with no leg, which a save deletes.
        pub fn page_bytes(&self, page: u64) -> Option<Vec<u8>> {
            let lo = page.checked_mul(LEGS_PER_PAGE)?;
            let rows: Vec<(&u64, &Booked)> = self
                .legs
                .range(lo..lo.saturating_add(LEGS_PER_PAGE))
                .collect();
            if rows.is_empty() {
                return None;
            }
            let mut w = Writer::new();
            w.u32(LEG_MAGIC);
            w.var_u64(rows.len() as u64);
            for (id, b) in rows {
                id.put(&mut w);
                b.put(&mut w);
            }
            Some(w.into_bytes())
        }

        /// Every page of the index in page order (a snapshot).
        pub fn pages(&self) -> Vec<(u64, Vec<u8>)> {
            let mut out = Vec::new();
            let mut last = None;
            for &id in self.legs.keys() {
                let p = Self::page_of(id);
                if last.replace(p) != Some(p) {
                    out.extend(self.page_bytes(p).map(|b| (p, b)));
                }
            }
            out
        }

        /// The index its pages and id counter describe: the cell and train indexes rebuilt from the
        /// legs in id order, as the bookings made them. A page must hold only its own ids (a save
        /// writes and deletes a page at its own key only), in order, below `next`.
        pub fn from_pages(
            next: u64,
            pages: impl IntoIterator<Item = (u64, Vec<u8>)>,
        ) -> Result<LegIndex> {
            let mut rows = BTreeMap::new();
            for (page, bytes) in pages {
                let mut r = Reader::new(&bytes);
                let magic = r.u32()?;
                if magic != LEG_MAGIC {
                    return invalid(&format!(
                        "leg page magic {magic:#010x}, not {LEG_MAGIC:#010x}: an older format"
                    ));
                }
                let n = r.var_u64()?;
                if n == 0 || n > r.remaining() as u64 {
                    return invalid("leg page length");
                }
                let mut prev = None;
                for _ in 0..n {
                    let id = u64::get(&mut r)?;
                    if LegIndex::page_of(id) != page || id >= next || prev.is_some_and(|p| p >= id)
                    {
                        return invalid(&format!("leg {id} in the leg page {page} (next {next})"));
                    }
                    prev = Some(id);
                    let b = Booked::get(&mut r)?;
                    // A booking is clamped to this span ([`LegIndex::book_on`]); a page that holds
                    // another would ask for an index of any size.
                    if b.leg.eta < b.leg.depart || b.leg.eta.0 - b.leg.depart.0 > LEG_MAX_TICKS {
                        return invalid(&format!(
                            "leg {id} lasts from tick {} to {}",
                            b.leg.depart.0, b.leg.eta.0
                        ));
                    }
                    rows.insert(id, b);
                }
                r.finish()?;
            }
            let mut idx = LegIndex {
                next,
                ..LegIndex::default()
            };
            for (id, b) in rows {
                idx.insert(id, b.leg, b.dim, b.cells);
            }
            idx.changed.clear();
            Ok(idx)
        }
    }
}
