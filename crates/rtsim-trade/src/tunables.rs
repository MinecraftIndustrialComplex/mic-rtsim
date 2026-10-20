// SPDX-License-Identifier: AGPL-3.0-or-later
//! The economy's tunables. `r`, `T` and `k` are Proposal P9 (Ben); the rest are P18's "TUNABLE and
//! set from a soak at the design gate". Every value here is a PLACEHOLDER except where a comment
//! says it is derived. Nothing reads a tunable from anywhere else, so the soak has one place to
//! turn.

use crate::units::{Amount, Quantum};

/// A rational, for slopes that must stay exact in the player trade integral.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ratio {
    pub num: i64,
    pub den: i64,
}

impl Ratio {
    pub const fn new(num: i64, den: i64) -> Self {
        assert!(den > 0);
        Ratio { num, den }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tunables {
    /// The accrual quantum (overnight choice E4).
    pub quantum: Quantum,
    /// `r`, the price band: `p` runs from `p*(1 + r)` at an empty store to `p*(1 - r)` at twice the
    /// target. PLACEHOLDER (P9): 3/4, Victoria 3's 0.75 that the economy research cites. Must stay
    /// below 1 so the floor price is positive.
    pub r: Ratio,
    /// `T` in days: target stock is `T × max(c, y)`. PLACEHOLDER (P9). The derived lower bound
    /// `T ≥ 2 × max d` over a market's edges is applied on top, per market
    /// ([`Tunables::target_quanta`]).
    pub t_days: u64,
    /// `k` in days, Cambridge k: target treasury `Mbar = k × pop / L_mint`. PLACEHOLDER (P9).
    pub k_days: u64,
    /// Labour, in worker-days, that one caravan costs per day on the road: food, wear, the
    /// trader's time. Transport cost `τ = upkeep × d / capacity`. PLACEHOLDER (P18).
    pub upkeep_worker_days_per_day: f64,
    /// Trader llama chest slots per caravan: 2 llamas × 3 × strength (PLAN.md §8.4), at strength 1
    /// (6) until the census reports real strengths. Ribbit caravan capacity is P18; the same
    /// PLACEHOLDER is used for it.
    pub caravan_slots: u32,
    /// Share of a village's working days one craft recipe may use, in thousandths, which caps a
    /// recipe with no resource of its own. PLACEHOLDER (P18).
    pub craft_share_milli: i64,
    /// A foreign currency's holding target, as a share of the home treasury target at mint
    /// parity, in thousandths. PLACEHOLDER (P18).
    pub fx_holding_share_milli: i64,
    /// News resolution: the absorbable rate a market advertises is rounded down to a step of its
    /// own need over this divisor (at least one item a day), so a stock drifting by a few
    /// thousandths is not news. Without it every refill step is a new advertisement and a closed
    /// world never goes quiet. PLACEHOLDER (overnight choice E17).
    pub news_resolution_div: i64,
}

impl Default for Tunables {
    fn default() -> Self {
        Tunables {
            quantum: Quantum::HOUR,
            r: Ratio::new(3, 4),
            t_days: 2,
            k_days: 7,
            upkeep_worker_days_per_day: 1.0,
            caravan_slots: 6,
            craft_share_milli: 100,
            fx_holding_share_milli: 250,
            news_resolution_div: 8,
        }
    }
}

impl Tunables {
    /// `T` in quanta for a market whose longest incident edge takes `max_d` quanta: the
    /// placeholder or the derived bound `2 × max d`, whichever is larger. The bound is what lets
    /// the band between target and twice the target absorb the news lag (PLAN.md §8.4); it is
    /// taken per market, over its own edges (overnight choice E5).
    pub fn target_quanta(&self, max_d: u64) -> u64 {
        self.quantum.days(self.t_days).max(2 * max_d).max(1)
    }

    /// Target stock `T × max(c, y)` from per-quantum consumption and production capacity.
    pub fn target(&self, t_quanta: u64, cons: Amount, cap: Amount) -> Amount {
        (t_quanta as i64).saturating_mul(cons.max(cap).max(0))
    }
}
