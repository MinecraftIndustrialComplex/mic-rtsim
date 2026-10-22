// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every number the knowledge module runs on, in one place. All are PLACEHOLDERS waiting on
//! Proposal P18 (phase 1 starting values) unless a comment cites where the value comes from;
//! cited values are the recommendation PLAN.md §17 makes, still not decided by Ben.

use rtsim_kernel::clock::TICKS_PER_DAY;

use crate::report::ReportKind;

/// Severity that means "the base delta for this kind". A report's sentiment delta is
/// `base × severity / SEVERITY_UNIT`, so severity 200 counts double. PLACEHOLDER (P18).
pub const SEVERITY_UNIT: u8 = 100;

/// Everything tunable, so a scenario or a test can override one value without forking the code.
#[derive(Clone, Debug, PartialEq)]
pub struct Tunables {
    /// Report lifetimes in ticks, by kind (indexed by [`ReportKind::index`]). TUNABLE (P18:
    /// set from a soak).
    pub lifetime: [u64; ReportKind::COUNT],
    /// Sentiment change toward `who` for a first-hand report of severity [`SEVERITY_UNIT`].
    pub delta: [i16; ReportKind::COUNT],
    /// Second-hand news counts this many percent of first-hand. PLACEHOLDER (P18).
    pub hearsay_percent: u8,
    /// Mean ticks per one-step decay of a sentiment of magnitude m is
    /// `decay_base_ticks × (1 + decay_strong_factor × (m / 127)²)`: weak feelings fade in hours,
    /// strong ones over weeks, as Veloren's table shows (`sentiment.rs:53-75`). Both PLACEHOLDER.
    pub decay_base_ticks: f64,
    pub decay_strong_factor: f64,
    /// Sentiment entries per holder kind (indexed by [`crate::HolderKind::index`]).
    pub sentiment_cap: [u16; 5],
    /// Noticeboard entries per holder kind.
    pub board_cap: [u32; 5],
    /// Reports in the global table. PLACEHOLDER: sized from the memory budget later (A16, P24);
    /// PLAN.md §4.8 budgets 32 MB for a million reports.
    pub report_cap: u32,
    /// (actor, holder, kind) counters kept after reports expire. PLACEHOLDER (A16).
    pub counter_cap: u32,
    /// Reports per gossip packet; the rest go on the next one. PLACEHOLDER (P18).
    pub packet_cap: u32,
    /// Memory slots per notable: 16, as SCALE.md §9 budgets the notable row.
    pub memory_slots: u8,
    /// Memories and sentiments an heir inherits (C24). PLACEHOLDER: P18 lists "how many memories
    /// an heir inherits" among the values to set at the design gate.
    pub heir_memories: u8,
    pub heir_sentiments: u8,
    /// Default witness radius in blocks. Veloren pushes death reports to NPCs within 32 blocks
    /// (`rule/report.rs`); PLACEHOLDER until body kinds carry their own radius as data.
    pub witness_radius: u32,
}

impl Default for Tunables {
    fn default() -> Self {
        let day = TICKS_PER_DAY;
        let mut lifetime = [0u64; ReportKind::COUNT];
        let mut delta = [0i16; ReportKind::COUNT];
        for k in ReportKind::ALL {
            // Lifetimes (P18, Ben: "Vanilla + Veloren + soak"; lengthened now that news travels
            // separately from cargo, overnight choice PR7). Veloren's were murder 15 in-game days
            // and theft 1.5 (`data/report.rs:32-47`): with messengers a report crosses a hop in
            // about half a day, so 1.5 days reached only the nearest villages. Every lifetime is
            // TUNABLE, to be set from a soak; these are starting values: Veloren's doubled, theft
            // and robbery four times (a robbery should reach its victim's home and the villages
            // around it).
            let (life, d) = match k {
                ReportKind::Killing | ReportKind::ResidentKilling => (30 * day, -125),
                ReportKind::Robbery | ReportKind::Theft => (6 * day, -50),
                // Veloren keeps no assault, trade or rescue reports: 10 days is twice Veloren's
                // "other deaths", borrowed as a middle value; a trade lives 3.
                ReportKind::Assault | ReportKind::ResidentAssault => (10 * day, -25),
                ReportKind::Trade => (3 * day, 2),
                ReportKind::Rescue => (10 * day, 25),
                // PLACEHOLDER (OILGATE.md §4.5, choices-oilgate.md OGC3): an assault's weight and
                // life; a dragon resenses a working district once a day, so it stays resented
                // while the district works and forgets it ten days after it stops.
                ReportKind::Encroachment => (10 * day, -25),
            };
            lifetime[k.index()] = life;
            delta[k.index()] = d;
        }
        // Deltas: P18 recommends vanilla's gossip scale (reputation = weight × value): a villager
        // hurt is +25 minor negative (weight -1, so -25), killed is +25 major negative (weight -5,
        // so -125), a trade is +2 trading (weight +1). Robbery -50 and theft -50 (a llama taken is
        // remembered "like a robbery", PLAN.md §8.9 E) and rescue +25 are PLACEHOLDERS.
        Tunables {
            lifetime,
            delta,
            hearsay_percent: 50,
            decay_base_ticks: 2_400.0,
            decay_strong_factor: 99.0,
            // VanillaVillage, RibbitVillage, PlayerBase, Caravan, Notable. Notables hold 16, as
            // SCALE.md §9 budgets; the rest are PLACEHOLDERS.
            sentiment_cap: [64, 64, 16, 16, 16],
            board_cap: [256, 256, 64, 64, 64],
            report_cap: 1 << 20,
            counter_cap: 1 << 20,
            packet_cap: 64,
            memory_slots: 16,
            heir_memories: 4,
            heir_sentiments: 4,
            witness_radius: 32,
        }
    }
}

impl Tunables {
    pub fn lifetime(&self, kind: ReportKind) -> u64 {
        self.lifetime[kind.index()]
    }

    /// The delta toward `who` for a report of this kind and severity, first-hand or not.
    pub fn delta(&self, kind: ReportKind, severity: u8, hearsay: bool) -> i16 {
        let base = i32::from(self.delta[kind.index()]);
        let mut d = base * i32::from(severity) / i32::from(SEVERITY_UNIT);
        if hearsay {
            d = d * i32::from(self.hearsay_percent) / 100;
        }
        d.clamp(-127, 127) as i16
    }

    /// Mean ticks for one decay step at magnitude `m` (1..=127).
    pub fn mean_step_ticks(&self, m: u8) -> f64 {
        let x = f64::from(m) / 127.0;
        self.decay_base_ticks * (1.0 + self.decay_strong_factor * x * x)
    }
}
