// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every TUNABLE the industry engine uses, in one place (INDUSTRY.md: "every number is a verified
//! fact, arithmetic from one, or a labelled TUNABLE").
//!
//! **None of the defaults is measured.** Each is a PLACEHOLDER chosen only so the synthetic sims
//! and tests run; INDUSTRY.md §19 risk 1 says phase I-A measures real windows before replay ships.
//! Where a default follows from a verified fact, the comment says which.

use serde::Serialize;

use crate::threat::ThreatClass;

/// Learning (I5).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LearnTunables {
    /// W_min: the shortest window, in ticks. TUNABLE, PLACEHOLDER 12,000 (half a Minecraft day).
    pub window_min_ticks: u64,
    /// k: the window is at least k × the longest quiet gap in any good's stock. TUNABLE,
    /// PLACEHOLDER 4.
    pub window_gap_factor: u64,
    /// The longest window tried before it is abandoned as not steady. TUNABLE, PLACEHOLDER 72,000
    /// (three days).
    pub window_max_ticks: u64,
    /// Fewest sample points in each half-window for the steady test. TUNABLE, PLACEHOLDER 6.
    pub min_points_per_half: usize,
    /// Points kept per window; past it, every other point is dropped (the fit stays unbiased).
    /// TUNABLE, PLACEHOLDER 512.
    pub max_points: usize,
    /// Goods whose rate magnitude is below this many units per day count as constant. TUNABLE,
    /// PLACEHOLDER 1.0.
    pub rate_floor_per_day: f64,
    /// ε: the two half-window slopes must agree within this share of the larger. TUNABLE,
    /// PLACEHOLDER 0.10.
    pub steady_epsilon: f64,
    /// A model is adopted only at or above this confidence (see `learn::confidence`). TUNABLE,
    /// PLACEHOLDER 0.0, so any window that passes the steady test is adopted, as I5 says.
    pub min_confidence: f64,
    /// A produced good flat for this long while every consumed good had stock is a stop (target
    /// learning). TUNABLE, PLACEHOLDER 2,400 ticks (two minutes). At least Create's
    /// `factoryGaugeTimer` of 100 ticks (verified) so a gauge's pause is not mistaken for a stop.
    pub stop_quiet_ticks: u64,
    /// Two stops within this many units of each other confirm a learned target. TUNABLE,
    /// PLACEHOLDER 64 (one stack of a 64-stack item).
    pub target_tolerance: i64,
    /// Learned coupling (I6): an input needs this many valid starvation episodes before the
    /// replay splits anything from it; until then it feeds everything (all-or-nothing). TUNABLE,
    /// PLACEHOLDER 2.
    pub coupling_min_episodes: u32,
    /// Time after an input empties before its episode is measured, for work in progress on belts
    /// and in machines to drain (at least twice the learned batch cycle). TUNABLE, PLACEHOLDER
    /// 1,200 ticks (a minute).
    pub coupling_settle_ticks: u64,
    /// The shortest measured span of an episode (at least ten batch cycles). TUNABLE, PLACEHOLDER
    /// 2,400 ticks.
    pub coupling_min_span_ticks: u64,
    /// A good moved during an episode if it made at least this share of its learned rate.
    /// TUNABLE, PLACEHOLDER 0.8.
    pub coupling_moved_share: f64,
    /// The input itself stopped if it was used at most this share of its learned rate (else it
    /// was fed and used as fast as it came, and the episode says nothing). TUNABLE, PLACEHOLDER
    /// 0.2.
    pub coupling_stopped_share: f64,
    /// Goods expected to move fewer units than this over an episode cannot be judged and stay
    /// coupled. TUNABLE, PLACEHOLDER 4.
    pub coupling_min_units: f64,
}

impl Default for LearnTunables {
    fn default() -> Self {
        Self {
            window_min_ticks: 12_000,
            window_gap_factor: 4,
            window_max_ticks: 72_000,
            min_points_per_half: 6,
            max_points: 512,
            rate_floor_per_day: 1.0,
            steady_epsilon: 0.10,
            min_confidence: 0.0,
            stop_quiet_ticks: 2_400,
            target_tolerance: 64,
            coupling_min_episodes: 2,
            coupling_settle_ticks: 1_200,
            coupling_min_span_ticks: 2_400,
            coupling_moved_share: 0.8,
            coupling_stopped_share: 0.2,
            coupling_min_units: 4.0,
        }
    }
}

/// The replay (I6) and reconciliation (I8).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReplayTunables {
    /// A stopped process restarts only when it could then run at least this long. Without it, a
    /// consumer fed by a continuous trickle (a channel) would stop and start every few ticks.
    /// TUNABLE, PLACEHOLDER 200 ticks (10 s).
    pub restart_min_run_ticks: u64,
    /// Item slots held back per produced item good and receiving inventory, for partial stacks
    /// (fragmentation). TUNABLE, PLACEHOLDER 1.
    pub fragment_margin_slots: i64,
    /// A closing IND_APPROACH at most this far (blocks to the district's chunk box) sends the
    /// district's reconcile ahead of arrival. TUNABLE, PLACEHOLDER 256: the Java side's own
    /// approach radius (H23), so every closing record counts.
    pub approach_distance_blocks: u32,
    /// The replay stays stopped this long after the last closing record; then the player is
    /// taken to have turned away and it goes on. TUNABLE, PLACEHOLDER 100 ticks: five of the Java
    /// side's 20-tick reports.
    pub approach_hold_ticks: u64,
}

impl Default for ReplayTunables {
    fn default() -> Self {
        Self {
            restart_min_run_ticks: 200,
            fragment_margin_slots: 1,
            approach_distance_blocks: 256,
            approach_hold_ticks: 100,
        }
    }
}

/// Per threat class, indexed by [`ThreatClass::index`].
pub type PerClass<T> = [T; ThreatClass::COUNT];

/// Threats (C16 to C18, I19 to I25).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ThreatTunables {
    /// Ticks of each game day natural mobs can spawn in, for turning a measured per-night-tick
    /// rate into a per-day one. TUNABLE, PLACEHOLDER 10,000. Vanilla's real value depends on
    /// `dayTime` and the moon (UNVERIFIED here); the bridge should measure it alongside the rate.
    pub night_ticks_per_day: u64,
    /// Defences never seen in a real fight count at this weight (I22). TUNABLE, PLACEHOLDER 0.5.
    pub untested_weight: f64,
    /// Observed fights count as "comparable" when the attacker strength is within this factor
    /// (I22). TUNABLE, PLACEHOLDER 2.0.
    pub comparable_strength_factor: f64,
    /// Items one surviving attacker carries away (I23). TUNABLE, PLACEHOLDER: 0 for classes that
    /// do not steal. Natural mobs do not take from containers (vanilla) and the pack sets dragon
    /// `stealItems` false (verified, iaf-common.json), so only siege (bandit) carries anything.
    pub carry_per_attacker: PerClass<i64>,
    /// The most one attack may steal, as parts per million of each good's boundary stock (I23).
    /// TUNABLE, PLACEHOLDER 100,000 (10%).
    pub theft_cap_ppm: i64,
    /// The most blocks one attack may destroy (I23). TUNABLE, PLACEHOLDER 8.
    pub destroy_cap_blocks: u32,
    /// Blocks one surviving attacker destroys (I23). TUNABLE, PLACEHOLDER: ground melee 0 (most
    /// mobs break nothing, vanilla), ranged 0, flying 1, siege 1 (creepers, raiders, dragons).
    pub blocks_per_attacker: PerClass<u32>,
    /// After an attack of a class, a district is safe from that class this long (I25). TUNABLE,
    /// PLACEHOLDER 24,000 ticks (a day) for every class.
    pub cooldown_ticks: PerClass<u64>,
    /// Lanchester attrition coefficient of one defender against one attacker, per tick of combat
    /// (I22). TUNABLE, PLACEHOLDER 1.0. Only its ratio to `attacker_effectiveness` matters.
    pub defender_effectiveness: f64,
    /// The same for one attacker against one defender. TUNABLE, PLACEHOLDER 1.0.
    pub attacker_effectiveness: f64,
    /// Share of a ground attack that gets past a perimeter, per open gap column (I21, I22).
    /// TUNABLE, PLACEHOLDER 0.05: twenty gaps let everything through.
    pub gap_passage_per_column: f64,
}

impl Default for ThreatTunables {
    fn default() -> Self {
        Self {
            night_ticks_per_day: 10_000,
            untested_weight: 0.5,
            comparable_strength_factor: 2.0,
            carry_per_attacker: [0, 0, 0, 64],
            theft_cap_ppm: 100_000,
            destroy_cap_blocks: 8,
            blocks_per_attacker: [0, 0, 1, 1],
            cooldown_ticks: [24_000; ThreatClass::COUNT],
            defender_effectiveness: 1.0,
            attacker_effectiveness: 1.0,
            gap_passage_per_column: 0.05,
        }
    }
}

/// All of them.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Tunables {
    pub learn: LearnTunables,
    pub replay: ReplayTunables,
    pub threat: ThreatTunables,
}
