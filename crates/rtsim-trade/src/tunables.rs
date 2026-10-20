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
    /// The rules every market keeps a copy of for its own decisions.
    pub rules: Rules,
}

/// The parts of the tunables a market decides with, copied into each market.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rules {
    /// News resolution: the absorbable rate a market advertises is rounded to a step of its own
    /// need over this divisor (at least one item a day), so a stock drifting by a few thousandths
    /// is not news. Without it every refill step is a new advertisement and a closed world never
    /// goes quiet. PLACEHOLDER (overnight choice E17).
    pub news_resolution_div: i64,
    /// A goods line's slack above its target, in news steps: a village with plenty eats a little
    /// more and lets a little spoil, so an oversupply smaller than this leaves the stock at the
    /// target (overnight choice E43). 0: nothing sinks. PLACEHOLDER.
    pub goods_slack_steps: i64,
    /// Coin above twice the treasury target (and foreign coin above twice its holding target) is
    /// spent on upkeep and leaves the world, ledgered as sunk (overnight choice E43). Off: it
    /// piles up and the market refuses cash, as round 1 did.
    pub money_sink: bool,
    /// A market exports anywhere in its target's band, sending less below the target so it
    /// refills, instead of nothing until it reaches the target exactly (overnight choice E45).
    pub exporter_band: bool,
    /// The advertised rate is rounded up to the news step instead of toward zero, so what is left
    /// over after rounding is a small oversupply the slack absorbs, not a shortage that creeps
    /// the stock away from its target (overnight choice E43).
    pub absorb_round_up: bool,
    /// An exporter's ask is its price but never above its anchor, the mirror of the destination's
    /// bid floor (E18), so an exporter a little under its target inside the band still sees a
    /// margin of zero on a steady flow (overnight choice E45).
    pub ask_at_anchor: bool,
    /// Several suppliers answering one advertisement each answer their share: an increase split
    /// evenly among those who could supply, a cut in proportion to what each sends (overnight
    /// choice E46). Off: each answers the whole of it, as round 1 did.
    pub split_among_suppliers: bool,
    /// The advertised rate never asks to cut more than arrives (overnight choice E47).
    pub clamp_to_inflow: bool,
    /// Flows are paid in goods when the payee refuses cash or the payer has none (E21). Off: a
    /// payer short of cash pays what it can sustain and the coverage rule (E20) cuts the rest
    /// (overnight choice E48).
    pub barter_in_flows: bool,
    /// A flow or payment whose new rate is within one part in this of its current rate keeps its
    /// current rate (and quote): small moves of stock-dependent terms do not ripple out as
    /// changes and wakes (overnight choice E48). 0: off. PLACEHOLDER.
    pub sticky_div: i64,
    /// The smallest news step, thousandths of an item a day (E17: one item a day). PLACEHOLDER.
    pub step_floor_milli_per_day: i64,
    /// A shortage is shared among destinations in proportion to what each wants, not best
    /// margin first (overnight choice E49).
    pub ration: bool,
    /// Stickiness also applies to goods flows a market sends from what it can spare, not only to
    /// payments.
    pub sticky_goods: bool,
    /// Advertised rates and what an exporter can spare hold their last value while the
    /// unrounded value stays within one news step of it (overnight choice E51).
    pub hysteresis: bool,
    /// A recipe held back by a short input stays held back until the input is back at its
    /// target (overnight choice E51).
    pub input_hysteresis: bool,
    /// A market that cannot pay for what reaches it asks for no more of anything (overnight
    /// choice E52).
    pub budget: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            news_resolution_div: 8,
            goods_slack_steps: 2,
            money_sink: true,
            exporter_band: true,
            absorb_round_up: false,
            ask_at_anchor: true,
            split_among_suppliers: true,
            clamp_to_inflow: true,
            barter_in_flows: false,
            sticky_div: 0,
            step_floor_milli_per_day: 1_000,
            ration: true,
            sticky_goods: false,
            hysteresis: true,
            input_hysteresis: true,
            budget: false,
        }
    }
}

impl Rules {
    /// The rate a pipeline now at `old` should be set to when the allocation asks for `new`:
    /// `old` when both flow and they differ by less than one part in `sticky_div`.
    pub fn sticky(&self, old: crate::Amount, new: crate::Amount) -> crate::Amount {
        if self.sticky_div > 0
            && old > 0
            && new > 0
            && ((i128::from(new) - i128::from(old)).abs() * i128::from(self.sticky_div)
                < i128::from(old.max(new))
                || (new - old).abs() <= 4)
        {
            old
        } else {
            new
        }
    }

    /// Round 1's rules: no sinks, exports only at the target, advertised rates rounded toward
    /// zero. For comparisons.
    pub const ROUND1: Rules = Rules {
        news_resolution_div: 8,
        goods_slack_steps: 0,
        money_sink: false,
        exporter_band: false,
        absorb_round_up: false,
        ask_at_anchor: false,
        split_among_suppliers: false,
        clamp_to_inflow: false,
        barter_in_flows: true,
        sticky_div: 0,
        step_floor_milli_per_day: 1_000,
        ration: false,
        sticky_goods: false,
        hysteresis: false,
        input_hysteresis: false,
        budget: false,
    };

    /// Sets rules from a comma-separated list of `name=value` (`slack=2,money=1,band=1,up=1,
    /// ask=1,div=8`), or `round1`. For comparisons on the command line.
    pub fn parse(spec: &str) -> Result<Rules, String> {
        let mut r = Rules::default();
        for part in spec.split(',').filter(|p| !p.is_empty()) {
            if part == "round1" {
                r = Rules::ROUND1;
                continue;
            }
            let (k, v) = part.split_once('=').ok_or(format!("{part}: expected name=value"))?;
            let n: i64 = v.parse().map_err(|_| format!("{part}: not a number"))?;
            match k {
                "slack" => r.goods_slack_steps = n,
                "money" => r.money_sink = n != 0,
                "band" => r.exporter_band = n != 0,
                "up" => r.absorb_round_up = n != 0,
                "ask" => r.ask_at_anchor = n != 0,
                "split" => r.split_among_suppliers = n != 0,
                "clamp" => r.clamp_to_inflow = n != 0,
                "barter" => r.barter_in_flows = n != 0,
                "sticky" => r.sticky_div = n.max(0),
                "floor" => r.step_floor_milli_per_day = n.max(0),
                "ration" => r.ration = n != 0,
                "stickygoods" => r.sticky_goods = n != 0,
                "hyst" => r.hysteresis = n != 0,
                "inhyst" => r.input_hysteresis = n != 0,
                "budget" => r.budget = n != 0,
                "div" => r.news_resolution_div = n.max(1),
                _ => return Err(format!("unknown rule {k}")),
            }
        }
        Ok(r)
    }
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
            rules: Rules::default(),
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
