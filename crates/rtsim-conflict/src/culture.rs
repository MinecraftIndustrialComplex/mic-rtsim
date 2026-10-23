// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cultures as data (C25: kinds are data, temperament bends them). A culture says what a faction
//! of it tolerates and what it does about the rest: per signal a weight and a tolerated rate, how
//! fast it forgives, the thresholds of each rung of the ladder, its units and the make-up of each
//! party it sends, its haven's market, and whom it hates.
//!
//! The built-in cultures are `data/cultures/goblin.json`, `bandit.json` and `raider.json`, and
//! Mowzie's Mobs' `frostmaw.json` and `umvuthana.json` (MOWZIE.md §4.1, ledger 15). Every
//! number in them is a TUNABLE PLACEHOLDER (P18's soak sets real ones); every body is an abstract
//! body kind (`mic_rtsim:conflict/<culture>/<unit>`) that a plugin maps to a mod's entity (P15;
//! candidates P8: Goblin's Tyranny for goblins, Illager Invasion for raiders, `minecraft:pillager`
//! as the fallback). D7: the plugin's body data picks mob-like bodies.

use std::collections::BTreeMap;

use rtsim_industry::threat::{ThreatClass, ThreatSource};
use rtsim_kernel::clock::TICKS_PER_DAY;
use serde::{Deserialize, Serialize};

/// What a faction can hold against someone (the brief's list: smog, radioactivity, logging,
/// mining, killing members, building on their plots). The numbers are the boundary's and never
/// change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum SignalKind {
    /// Smog and the other air pollution types (Destroy, attributed by position, I14).
    Smog = 0,
    /// Radioactivity (Destroy's chunk type; later Create Nuclear and mic_nuclear).
    Radiation = 1,
    /// Logs broken inside the territory.
    Logging = 2,
    /// Ore broken or drained inside the territory (a block, or a deposit drained by a district).
    Mining = 3,
    /// Members killed.
    Killing = 4,
    /// Blocks placed on the faction's plots.
    Building = 5,
}

impl SignalKind {
    pub const COUNT: usize = 6;
    pub const ALL: [SignalKind; SignalKind::COUNT] = [
        SignalKind::Smog,
        SignalKind::Radiation,
        SignalKind::Logging,
        SignalKind::Mining,
        SignalKind::Killing,
        SignalKind::Building,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_u32(v: u32) -> Option<SignalKind> {
        SignalKind::ALL.get(v as usize).copied()
    }

    pub const fn name(self) -> &'static str {
        match self {
            SignalKind::Smog => "smog",
            SignalKind::Radiation => "radiation",
            SignalKind::Logging => "logging",
            SignalKind::Mining => "mining",
            SignalKind::Killing => "killing",
            SignalKind::Building => "building",
        }
    }

    pub fn from_name(s: &str) -> Option<SignalKind> {
        SignalKind::ALL.into_iter().find(|k| k.name() == s)
    }

    /// The bit of this kind in a TERRITORY record's signal mask.
    pub const fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// The rungs of the escalation ladder (PLAN.md §10.3). The numbers are the boundary's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Step {
    None = 0,
    Notice = 1,
    Envoy = 2,
    Scouts = 3,
    Raid = 4,
    Invasion = 5,
    Siege = 6,
}

impl Step {
    pub const COUNT: usize = 7;
    pub const ALL: [Step; Step::COUNT] = [
        Step::None,
        Step::Notice,
        Step::Envoy,
        Step::Scouts,
        Step::Raid,
        Step::Invasion,
        Step::Siege,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_u8(v: u8) -> Option<Step> {
        Step::ALL.get(usize::from(v)).copied()
    }

    pub fn up(self) -> Option<Step> {
        Step::from_u8(self as u8 + 1)
    }

    pub fn down(self) -> Step {
        Step::from_u8((self as u8).saturating_sub(1)).unwrap_or(Step::None)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Step::None => "none",
            Step::Notice => "notice",
            Step::Envoy => "envoy",
            Step::Scouts => "scouts",
            Step::Raid => "raid",
            Step::Invasion => "invasion",
            Step::Siege => "siege",
        }
    }

    /// A rung that sends a war band.
    pub const fn is_war(self) -> bool {
        matches!(self, Step::Raid | Step::Invasion | Step::Siege)
    }
}

/// How a unit fights: the industry threat classes (INDUSTRY.md §13.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitClass {
    GroundMelee,
    GroundRanged,
    Flying,
    Siege,
}

impl UnitClass {
    pub const fn threat(self) -> ThreatClass {
        match self {
            UnitClass::GroundMelee => ThreatClass::GroundMelee,
            UnitClass::GroundRanged => ThreatClass::GroundRanged,
            UnitClass::Flying => ThreatClass::Flying,
            UnitClass::Siege => ThreatClass::Siege,
        }
    }
}

/// Who the industry engine is told attacks, in ambushes and district raids (MOWZIE.md ledger 9):
/// bandits unless the culture says otherwise, a boss for the Frostmaw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampSource {
    #[default]
    Bandit,
    Boss,
}

impl CampSource {
    pub const fn threat(self) -> ThreatSource {
        match self {
            CampSource::Bandit => ThreatSource::Bandit,
            CampSource::Boss => ThreatSource::Boss,
        }
    }
}

/// What a party is for. The numbers are the boundary's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum PartyKind {
    Envoy = 1,
    Scouts = 2,
    Raid = 3,
    Invasion = 4,
    Siege = 5,
    /// A raid on a village or a rival's haven, for loot (C24: stock stolen goes to the haven).
    Plunder = 6,
}

impl PartyKind {
    pub const COUNT: usize = 6;
    pub const ALL: [PartyKind; PartyKind::COUNT] = [
        PartyKind::Envoy,
        PartyKind::Scouts,
        PartyKind::Raid,
        PartyKind::Invasion,
        PartyKind::Siege,
        PartyKind::Plunder,
    ];

    pub const fn index(self) -> usize {
        self as usize - 1
    }

    pub fn from_u8(v: u8) -> Option<PartyKind> {
        PartyKind::ALL.get(usize::from(v).wrapping_sub(1)).copied()
    }

    pub const fn name(self) -> &'static str {
        match self {
            PartyKind::Envoy => "envoy",
            PartyKind::Scouts => "scouts",
            PartyKind::Raid => "raid",
            PartyKind::Invasion => "invasion",
            PartyKind::Siege => "siege",
            PartyKind::Plunder => "plunder",
        }
    }

    pub fn of_step(s: Step) -> Option<PartyKind> {
        Some(match s {
            Step::Envoy => PartyKind::Envoy,
            Step::Scouts => PartyKind::Scouts,
            Step::Raid => PartyKind::Raid,
            Step::Invasion => PartyKind::Invasion,
            Step::Siege => PartyKind::Siege,
            _ => return None,
        })
    }

    /// Parties that fight: they draw on the warrior pool and carry loot home.
    pub const fn is_war(self) -> bool {
        matches!(
            self,
            PartyKind::Raid | PartyKind::Invasion | PartyKind::Siege | PartyKind::Plunder
        )
    }
}

/// One signal's terms: grievance per unit, and the rate (units per day) a faction lets pass.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalDef {
    pub weight: f64,
    /// Continuous signals (pollution, emissions) below this rate cause nothing. Impulses (a block
    /// broken, a member killed) always count at their weight (choice K2-6).
    pub tolerance_per_day: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitDef {
    pub name: String,
    /// The abstract body kind (dictionary kind 3 at the boundary).
    pub body: String,
    pub class: UnitClass,
    /// Fight strength, in the units the industry threat interface observes fights in.
    pub strength: f64,
    /// Items one unit carries off (INDUSTRY.md I23's carry).
    pub carry: u32,
    /// Blocks one unit breaks in a raid on a district (I23), carried on the threat with the carry
    /// (overnight choice C2-1, K2-13's recommendation). PLACEHOLDER; 0 when the data leaves it out.
    #[serde(default)]
    pub breaks: u32,
    /// The unit row this one fights as while its faction's chief has lost its crystal (MOWZIE.md
    /// ledger 9: the Frostmaw is `ground_ranged` with it, `ground_melee` without). Absent: the
    /// same row either way.
    #[serde(default)]
    pub without_crystal: Option<String>,
}

/// A culture as its JSON says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CultureDef {
    pub name: String,
    /// Structures (dictionary kind 4) a plugin reports as this culture's camps (FACTION_FOUND).
    pub structures: Vec<String>,
    /// Which world currency its haven's market keeps: `human` (emeralds) or `ribbit` (amethyst).
    pub market_culture: String,
    /// The world good it asks tribute in (by name); its haven's currency when absent.
    pub tribute_good: Option<String>,
    pub signals: BTreeMap<String, SignalDef>,
    /// Grievance halves in this many days with nothing new (forgiveness).
    pub half_life_days: f64,
    /// Grievance at which each rung from notice to siege is reached (6 values).
    pub thresholds: Vec<f64>,
    /// A rung is left downward when grievance falls below this share of its threshold.
    pub hysteresis: f64,
    /// Least days at each rung (none to siege, 7 values) before the next one up.
    pub dwell_days: Vec<f64>,
    /// Days an envoy's demand stands.
    pub demand_days: f64,
    /// Tribute asked per point of grievance, in coins of its haven's money, converted to the
    /// tribute good at the haven market's price (MOWZIE.md ledger 12).
    pub tribute_per_grievance: f64,
    /// Days after a tribute paid in full before the feud may climb again.
    pub respite_days: f64,
    /// Share of grievance a tribute paid in full takes away.
    pub tribute_relief: f64,
    /// Share of grievance taken away when the demanded signal stops by the deadline.
    pub cut_relief: f64,
    /// Grievance a gift takes away per coin of value at the haven's market.
    pub gift_relief_per_coin: f64,
    /// Grievance a raid takes away per item stolen (vengeance), up to a share of it.
    pub vengeance_per_item: f64,
    pub units: Vec<UnitDef>,
    /// Each party's make-up: unit name and count, by party kind name.
    pub parties: BTreeMap<String, Vec<(String, u32)>>,
    /// Warriors the faction can field at most, and how fast it recruits back to that.
    pub pool_cap: u32,
    pub recruits_per_day: f64,
    pub march_blocks_per_tick: f64,
    pub muster_days: f64,
    /// A band's patience at its target before it gets bored (C24), at a middling temperament.
    pub patience_days: f64,
    /// Days scouts watch the target.
    pub linger_days: f64,
    /// Days between a besieging band's attacks.
    pub siege_interval_days: f64,
    /// Days a faction stays cowed after its war chief is killed.
    pub cowed_days: f64,
    pub territory_radius: u32,
    /// Emissions are felt this far beyond the territory, falling off linearly.
    pub reach_blocks: u32,
    /// Settlement cultures (`human`, `ribbit`) and conflict cultures it hates: plunder targets,
    /// and those who think better of whoever breaks its bands.
    pub hates: Vec<String>,
    /// Mean days between plunder raids on what it hates, if it plunders.
    pub plunder_mean_days: Option<f64>,
    pub plunder_reach_blocks: u32,
    /// The most of each good of a village's stock one raid carries off, thousandths.
    pub loot_share_milli: i64,
    /// A band routs after losing this share of its units.
    pub rout_share: f64,
    pub chief_lifespan_days: u64,
    /// Days after a war party comes home before a feud still on Raid or Invasion, and at or above
    /// that rung's threshold, sends the rung's party again (MOWZIE.md ledger 3). Absent: never
    /// (only Siege repeats, every `siege_interval_days`).
    #[serde(default)]
    pub repeat_days: Option<f64>,
    /// Who its ambushes and district raids are, to the industry engine (ledger 9).
    #[serde(default)]
    pub source: CampSource,
    /// What its parties take is destroyed where it is taken (eaten), as the dragons' food is,
    /// never carried home (ledger 10).
    #[serde(default)]
    pub loot_destroyed: bool,
    /// Units of the tribute good asked per coin when the haven's market has no price for it
    /// (ledger 12): 9 gold nuggets an emerald for the Umvuthana. 1 when absent.
    #[serde(default = "one")]
    pub tribute_per_coin_fallback: f64,
    /// Whether its camp is a haven with a market (C24). A camp that trades nothing joins the world
    /// as a lair, `NodeKind::Lair` (ledger 13). True when absent.
    #[serde(default = "yes")]
    pub haven: bool,
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

/// A culture resolved for the engine: names turned into indices, rates per tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Culture {
    pub def: CultureDef,
    pub signals: [SignalDef; SignalKind::COUNT],
    /// Forgiveness rate, per tick.
    pub lambda: f64,
    pub thresholds: [f64; 6],
    pub dwell_ticks: [u64; Step::COUNT],
    /// Each party kind's make-up as (unit index, count). Empty for a rung the culture skips.
    pub parties: [Vec<(u8, u32)>; PartyKind::COUNT],
    pub market: rtsim_settle::Culture,
    /// Each unit's row without its chief's crystal (its own index when it has no other).
    pub bare: Vec<u8>,
}

pub fn days_to_ticks(d: f64) -> u64 {
    (d.max(0.0) * TICKS_PER_DAY as f64).ceil() as u64
}

impl Culture {
    pub fn resolve(def: CultureDef) -> Result<Culture, String> {
        let n = &def.name;
        let mut signals = [SignalDef {
            weight: 0.0,
            tolerance_per_day: 0.0,
        }; SignalKind::COUNT];
        for (k, v) in &def.signals {
            let kind = SignalKind::from_name(k).ok_or(format!("{n}: unknown signal {k}"))?;
            signals[kind.index()] = *v;
        }
        let thresholds: [f64; 6] = def
            .thresholds
            .clone()
            .try_into()
            .map_err(|_| format!("{n}: thresholds need 6 values (notice to siege)"))?;
        if thresholds.windows(2).any(|w| w[0] >= w[1]) || thresholds[0] <= 0.0 {
            return Err(format!("{n}: thresholds must rise from above 0"));
        }
        let dwell: [f64; Step::COUNT] = def
            .dwell_days
            .clone()
            .try_into()
            .map_err(|_| format!("{n}: dwell_days need 7 values (none to siege)"))?;
        if def.half_life_days <= 0.0 {
            return Err(format!("{n}: half_life_days must be positive"));
        }
        if !(0.0..1.0).contains(&def.hysteresis) {
            return Err(format!("{n}: hysteresis must be in [0, 1)"));
        }
        let mut parties: [Vec<(u8, u32)>; PartyKind::COUNT] = Default::default();
        for (name, list) in &def.parties {
            let kind = PartyKind::ALL
                .into_iter()
                .find(|k| k.name() == name)
                .ok_or(format!("{n}: unknown party {name}"))?;
            for (u, c) in list {
                let i = def
                    .units
                    .iter()
                    .position(|x| &x.name == u)
                    .ok_or(format!("{n}: party {name} names unknown unit {u}"))?;
                parties[kind.index()].push((i as u8, *c));
            }
        }
        // A rung's party may be empty: the climb skips it (MOWZIE.md ledger 2). Plunder is
        // needed only by a culture that plunders.
        if def.plunder_mean_days.is_some() && parties[PartyKind::Plunder.index()].is_empty() {
            return Err(format!("{n}: plunders but has no plunder party"));
        }
        if def.units.len() > usize::from(u8::MAX) {
            return Err(format!("{n}: too many units"));
        }
        let mut bare = Vec::with_capacity(def.units.len());
        for (i, u) in def.units.iter().enumerate() {
            bare.push(match &u.without_crystal {
                Some(b) => def.units.iter().position(|x| &x.name == b).ok_or(format!(
                    "{n}: unit {} names unknown unit {b} without its crystal",
                    u.name
                ))? as u8,
                None => i as u8,
            });
        }
        if def.repeat_days.is_some_and(|d| !(d > 0.0)) {
            return Err(format!("{n}: repeat_days must be positive"));
        }
        if !(def.tribute_per_coin_fallback > 0.0) {
            return Err(format!("{n}: tribute_per_coin_fallback must be positive"));
        }
        if !def.haven && !def.loot_destroyed {
            // A lair has no storehouse: loot carried home could never land (MZB-56).
            return Err(format!("{n}: a camp with no haven must destroy its loot"));
        }
        let market = match def.market_culture.as_str() {
            "human" => rtsim_settle::Culture::Human,
            "ribbit" => rtsim_settle::Culture::Ribbit,
            m => return Err(format!("{n}: unknown market culture {m}")),
        };
        if def.march_blocks_per_tick <= 0.0 {
            return Err(format!("{n}: march speed must be positive"));
        }
        let lambda = std::f64::consts::LN_2 / (def.half_life_days * TICKS_PER_DAY as f64);
        Ok(Culture {
            signals,
            lambda,
            thresholds,
            dwell_ticks: dwell.map(days_to_ticks),
            parties,
            market,
            bare,
            def,
        })
    }

    pub fn name(&self) -> &str {
        &self.def.name
    }

    /// The rung's threshold (`Step::None` has 0).
    pub fn threshold(&self, s: Step) -> f64 {
        match s {
            Step::None => 0.0,
            s => self.thresholds[s.index() - 1],
        }
    }

    pub fn unit(&self, i: u8) -> &UnitDef {
        &self.def.units[usize::from(i)]
    }

    /// Unit `i` as it fights: its own row with the chief's crystal, else its `without_crystal`
    /// row (MOWZIE.md ledger 9).
    pub fn unit_as(&self, i: u8, crystal: bool) -> &UnitDef {
        if crystal {
            self.unit(i)
        } else {
            self.unit(self.bare[usize::from(i)])
        }
    }

    /// Whether rung `s` is on this culture's ladder: Notice always (it sends nothing), a party
    /// rung when its party is not empty (MOWZIE.md ledger 2), `Step::None` never.
    pub fn has_rung(&self, s: Step) -> bool {
        match PartyKind::of_step(s) {
            Some(k) => !self.parties[k.index()].is_empty(),
            None => s == Step::Notice,
        }
    }

    /// The rung a climb from `s` goes to: the next one up on the ladder, skipping empty rungs.
    /// `None` at the top.
    pub fn next_up(&self, s: Step) -> Option<Step> {
        let mut u = s.up()?;
        while !self.has_rung(u) {
            u = u.up()?;
        }
        Some(u)
    }

    /// The rung a step down from `s` lands on: the next one down with a party, floored at
    /// Notice; Notice steps down to `Step::None`.
    pub fn next_down(&self, s: Step) -> Step {
        let mut d = s.down();
        while d > Step::Notice && !self.has_rung(d) {
            d = d.down();
        }
        d
    }

    pub fn hates(&self, culture: &str) -> bool {
        self.def.hates.iter().any(|h| h == culture)
    }
}

/// The cultures a world knows, in a fixed order (their index is saved).
#[derive(Clone, Debug, PartialEq)]
pub struct Cultures {
    pub list: Vec<Culture>,
}

pub const GOBLIN_JSON: &str = include_str!("../data/cultures/goblin.json");
pub const BANDIT_JSON: &str = include_str!("../data/cultures/bandit.json");
pub const RAIDER_JSON: &str = include_str!("../data/cultures/raider.json");
pub const FROSTMAW_JSON: &str = include_str!("../data/cultures/frostmaw.json");
pub const UMVUTHANA_JSON: &str = include_str!("../data/cultures/umvuthana.json");

impl Cultures {
    /// Cultures from JSON documents, one culture each.
    pub fn from_json(docs: &[&str]) -> Result<Cultures, String> {
        let mut list = Vec::new();
        for d in docs {
            let def: CultureDef = serde_json::from_str(d).map_err(|e| e.to_string())?;
            if list.iter().any(|c: &Culture| c.def.name == def.name) {
                return Err(format!("culture {} twice", def.name));
            }
            list.push(Culture::resolve(def)?);
        }
        Ok(Cultures { list })
    }

    /// Goblins, bandits, raiders, the Frostmaw and the Umvuthana (`data/cultures`), in that
    /// order: an index is saved, so new cultures go last.
    pub fn builtin() -> Cultures {
        Cultures::from_json(&[
            GOBLIN_JSON,
            BANDIT_JSON,
            RAIDER_JSON,
            FROSTMAW_JSON,
            UMVUTHANA_JSON,
        ])
        .expect("built-in cultures")
    }

    pub fn index_of(&self, name: &str) -> Option<u16> {
        self.list
            .iter()
            .position(|c| c.def.name == name)
            .map(|i| i as u16)
    }

    /// The culture whose camps are this structure.
    pub fn of_structure(&self, structure: &str) -> Option<u16> {
        self.list
            .iter()
            .position(|c| c.def.structures.iter().any(|s| s == structure))
            .map(|i| i as u16)
    }

    pub fn get(&self, i: u16) -> &Culture {
        &self.list[usize::from(i)]
    }

    /// A fingerprint of the data, saved with the engine so a save is never read with other data.
    pub fn fingerprint(&self) -> u64 {
        let s: Vec<String> = self
            .list
            .iter()
            .map(|c| serde_json::to_string(&c.def).expect("serialize"))
            .collect();
        rtsim_kernel::rng::fnv1a64(s.join("\n").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_cultures_resolve() {
        let c = Cultures::builtin();
        assert_eq!(c.list.len(), 5);
        let g = c.get(c.index_of("goblin").unwrap());
        assert!(g.signals[SignalKind::Logging.index()].weight > 0.0);
        assert_eq!(g.threshold(Step::None), 0.0);
        assert!(g.threshold(Step::Siege) > g.threshold(Step::Raid));
        assert!(c.of_structure("goblinstyranny:goblin_camp").is_some());
        assert_ne!(c.fingerprint(), 0);
    }

    #[test]
    fn bad_data_is_refused() {
        let mut def: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
        def.thresholds = vec![1.0, 2.0];
        assert!(Culture::resolve(def.clone()).is_err());
        def.thresholds = vec![10.0, 5.0, 20.0, 30.0, 40.0, 50.0];
        assert!(Culture::resolve(def).is_err());
    }
}
