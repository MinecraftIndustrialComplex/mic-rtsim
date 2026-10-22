// SPDX-License-Identifier: AGPL-3.0-or-later
//! The shared renewal rule (EQUILIBRIUM.md §2.2, slice S0): a site's pressure from recent players'
//! losses, and the wait before its next member arrives.
//!
//! - **Pressure.** `P(t) = p x 2^(-(t - at) / H)`, stored as two numbers and read in closed form. Each
//!   member a player kills or takes adds `1/K` at the time of the loss; no other loss adds anything.
//! - **Delay.** A short site's next member arrives after `min(cap, base x 2^P)` days, with P read when
//!   the wait starts. A wait already running is not pushed out by later losses.
//! - **Farmed steady state.** Under constant farming (each arrival killed within c days) P settles at
//!   `P* = (1/K) / (1 - 2^(-gap/H))` with `gap = min(cap, base x 2^P*) + c` (§3.3).
//!
//! Every engine that keeps sites (the dragons' lairs, the oil nests, MOWZIE's dens, the conflict camps)
//! reads its waits from here, so the engines, the model and the tests agree. The fields live in each
//! engine's own site rows, never in a shared table. Days are the engine's own: a test engine that runs
//! short days passes its own day numbers and converts [`RenewalRule::delay_days`] itself.

use crate::clock::TICKS_PER_DAY;

/// Recent player-caused losses at one site: `P(t) = p x 2^(-(t - at_day) / H)`. 8 bytes. `at_day` is the
/// engine's own day; elapsed time is `day.saturating_sub(at_day)`, so a clock that restarted after a reset
/// can never read as a huge gap.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Pressure {
    p: f32,
    at_day: u32,
}

impl Pressure {
    /// A pressure of `p` as of `day` (persistence and tests).
    pub fn from_parts(p: f32, at_day: u32) -> Self {
        Self { p: p.max(0.0), at_day }
    }

    /// The stored value and its day (persistence).
    pub fn parts(self) -> (f32, u32) {
        (self.p, self.at_day)
    }

    /// The pressure at `day`.
    pub fn now(self, day: u32, half_life_days: f32) -> f32 {
        if self.p == 0.0 {
            return 0.0;
        }
        let elapsed = day.saturating_sub(self.at_day) as f32;
        if half_life_days <= 0.0 {
            return 0.0;
        }
        self.p * (-elapsed / half_life_days).exp2()
    }

    /// `amount` is members lost / K; only `LossCause::Player` losses call this.
    pub fn bump(&mut self, day: u32, amount: f32, half_life_days: f32) {
        let now = self.now(day, half_life_days);
        self.p = (now + amount.max(0.0)).max(0.0);
        self.at_day = day.max(self.at_day);
    }

    /// A site found empty with no ledger row (§2.3): as if farmed to the cap, so its wait is the cap.
    pub fn at_cap(rule: &RenewalRule, day: u32) -> Self {
        let p = if rule.base_days > 0.0 && rule.cap_days > rule.base_days {
            (rule.cap_days / rule.base_days).log2()
        } else {
            0.0
        };
        Self { p, at_day: day }
    }
}

/// One kind's renewal timing. Every field TUNABLE, read from the module's tunables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenewalRule {
    pub base_days: f32,
    pub cap_days: f32,
    pub half_life_days: f32,
}

impl RenewalRule {
    /// `min(cap, base x 2^P)`, in days, with P read at `day`.
    pub fn delay_days(&self, p: Pressure, day: u32) -> f32 {
        self.delay_for(p.now(day, self.half_life_days))
    }

    /// `min(cap, base x 2^P)`, in ticks of 24,000 a day.
    pub fn delay_ticks(&self, p: Pressure, day: u32) -> u64 {
        (f64::from(self.delay_days(p, day)) * TICKS_PER_DAY as f64).round() as u64
    }

    fn delay_for(&self, p: f32) -> f32 {
        (self.base_days * p.exp2()).min(self.cap_days).max(0.0)
    }

    /// The farmed steady state for a site of K members, each killed within `check_days` of its arrival:
    /// (P* just after a kill, days per arrival). Tests and `sim census` compare against it.
    pub fn farmed(&self, k: u16, check_days: f32) -> (f32, f32) {
        let per = 1.0 / f64::from(k.max(1));
        let h = f64::from(self.half_life_days.max(f32::MIN_POSITIVE));
        let gap = |p: f64| f64::from(self.delay_for(p as f32)) + f64::from(check_days.max(0.0));
        // f(P) = per / (1 - 2^(-gap(P)/H)) - P falls as P rises: one root, by bisection.
        let f = |p: f64| per / (1.0 - (-gap(p) / h).exp2()) - p;
        let (mut lo, mut hi) = (per, per);
        while f(hi) > 0.0 && hi < 1e6 {
            hi *= 2.0;
        }
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let p = 0.5 * (lo + hi);
        (p as f32, gap(p) as f32)
    }
}

/// Where a member comes from, in world terms (§2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// From inside the site (a hatchling of the lair's pair, a grove's heir).
    Heir,
    /// From a site of the same kind in reach that has more than it needs (a move, not a mint).
    Neighbour,
    /// From beyond the explored edge, when no neighbour can give.
    Frontier,
    /// The site's own supply (a camp's recruits, an oil nest's worms while it is drilled).
    Regrow,
}

/// How a member appears (§2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arrival {
    /// Placed when the site's chunk next loads, only with no player within 128 blocks.
    OnLoad,
    /// A puppet walks or flies in from the source's direction while a player is near.
    WalkIn,
    /// The count changes while the site is unloaded; the next load's census reconciles.
    Abstract,
}

/// Why a member was lost. Player: §2.2's four cases (vanilla's kill credit, A10 removals, an unclaimed
/// death of a boss kind, a home body gone missing while rtsim ran). Only Player bumps pressure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LossCause {
    Player,
    Age,
    Starved,
    Faction,
    Predator,
    Other,
}

impl LossCause {
    pub const ALL: [LossCause; 6] = [
        LossCause::Player,
        LossCause::Age,
        LossCause::Starved,
        LossCause::Faction,
        LossCause::Predator,
        LossCause::Other,
    ];

    /// The wire code (0 Player, 1 Age, 2 Starved, 3 Faction, 4 Predator, 5 Other).
    pub fn code(self) -> u32 {
        self as u32
    }

    /// The cause for a wire code; unknown codes are Other.
    pub fn from_code(code: u32) -> Self {
        Self::ALL.get(code as usize).copied().unwrap_or(LossCause::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAIR: RenewalRule = RenewalRule { base_days: 20.0, cap_days: 192.0, half_life_days: 96.0 };

    #[test]
    fn pressure_halves_each_half_life() {
        let mut p = Pressure::default();
        p.bump(10, 1.0, 96.0);
        assert!((p.now(10, 96.0) - 1.0).abs() < 1e-6);
        assert!((p.now(106, 96.0) - 0.5).abs() < 1e-6);
        assert!((p.now(202, 96.0) - 0.25).abs() < 1e-6);
        // A clock that restarted reads no time passed, never a huge gap.
        assert!((p.now(3, 96.0) - 1.0).abs() < 1e-6);
        // A bump adds to what is left.
        p.bump(106, 1.0, 96.0);
        assert!((p.now(106, 96.0) - 1.5).abs() < 1e-6);
        assert!((p.now(202, 96.0) - 0.75).abs() < 1e-6);
        let (v, at) = p.parts();
        assert_eq!(Pressure::from_parts(v, at), p);
    }

    #[test]
    fn delay_is_capped() {
        let none = Pressure::default();
        assert_eq!(LAIR.delay_days(none, 0), 20.0);
        assert_eq!(LAIR.delay_ticks(none, 0), 20 * TICKS_PER_DAY);
        let mut one = Pressure::default();
        one.bump(0, 1.0, LAIR.half_life_days);
        assert_eq!(LAIR.delay_days(one, 0), 40.0);
        let mut lots = Pressure::default();
        lots.bump(0, 50.0, LAIR.half_life_days);
        assert_eq!(LAIR.delay_days(lots, 0), 192.0);
        // Found empty with no ledger: the cap.
        let cap = Pressure::at_cap(&LAIR, 7);
        assert!((LAIR.delay_days(cap, 7) - 192.0).abs() < 1e-3);
        // A nest's rule (§2.4.8): 12 days, capped at 96.
        let nest = RenewalRule { base_days: 12.0, cap_days: 96.0, half_life_days: 96.0 };
        assert_eq!(nest.delay_days(none, 0), 12.0);
        assert_eq!(nest.delay_days(lots, 0), 96.0);
    }

    /// Kill-on-arrival for 60 years against the closed form, within 5% (§3.3's table: a lair farmed
    /// with c = 2 settles near P 2.12, 89 days an arrival).
    #[test]
    fn farmed_steady_state_matches_closed_form() {
        let nest = RenewalRule { base_days: 12.0, cap_days: 96.0, half_life_days: 96.0 };
        let grove = RenewalRule { base_days: 6.0, cap_days: 96.0, half_life_days: 96.0 };
        let (p, gap) = LAIR.farmed(1, 2.0);
        assert!((p - 2.12).abs() < 0.02, "lair P* {p}");
        assert!((gap - 89.0).abs() < 1.5, "lair gap {gap}");
        for (rule, k) in [(LAIR, 1u16), (nest, 2), (grove, 7)] {
            let check = 2u32;
            let (_, gap) = rule.farmed(k, check as f32);
            let years = 60u32;
            let end = years * 96;
            // One member at a time; each arrival is killed `check` days later, which starts the next wait.
            let mut p = Pressure::default();
            let mut day = 0u32;
            let mut arrivals = 0u32;
            let mut counted_from = None;
            loop {
                let wait = rule.delay_days(p, day).ceil() as u32;
                let arrive = day + wait.max(1);
                if arrive > end {
                    break;
                }
                // Measure after a ten-year settling run.
                if arrive >= 10 * 96 {
                    counted_from.get_or_insert(arrive);
                    arrivals += 1;
                }
                day = arrive + check;
                p.bump(day, 1.0 / f32::from(k), rule.half_life_days);
            }
            let span = (end - counted_from.unwrap()) as f32;
            let sim_gap = span / arrivals as f32;
            assert!(
                (sim_gap - gap).abs() / gap < 0.05,
                "K {k} base {}: simulated {sim_gap} days an arrival, closed form {gap}",
                rule.base_days
            );
        }
    }

    #[test]
    fn loss_cause_codes_round_trip() {
        for c in LossCause::ALL {
            assert_eq!(LossCause::from_code(c.code()), c);
        }
        assert_eq!(LossCause::from_code(99), LossCause::Other);
    }
}
