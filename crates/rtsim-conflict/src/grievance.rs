// SPDX-License-Identifier: AGPL-3.0-or-later
//! The grievance ledger's closed form (PLAN.md §4.3: closed forms between events).
//!
//! One feud's grievance is a vector over [`SignalKind`]s. Between events each component follows
//! `dG/dt = r − λ·G`: a constant inflow `r` (the continuous signals over their tolerance, by
//! weight) and exponential forgiveness at the culture's rate `λ`. Its solution from the anchor
//! `(t₀, G₀)` is
//!
//! `G(t) = G₀·e + (r/λ)·(1 − e)`, with `e = exp(−λ·(t − t₀))`,
//!
//! so a read costs the same after a day as after ten years (catch-up is flat in elapsed time).
//! Impulses (a block broken, a member killed) are jumps at events. The anchor moves only at
//! events (a signal, a rate change, a rung of the ladder, a tribute): reads project and commit
//! nothing (O4's rule), so the trajectory depends only on the events, never on who read it when.
//!
//! The total's crossing of a threshold `θ` is also closed form: `e = (r/λ − θ) / (r/λ − G₀)` going
//! up, `(θ − r/λ) / (G₀ − r/λ)` going down. That time is when the ladder's next rung is booked:
//! one event per rung, nothing ticked.

use rtsim_kernel::clock::Tick;

use crate::culture::SignalKind;

pub const K: usize = SignalKind::COUNT;

/// One feud's grievance: levels at the anchor and inflow rates per tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grievance {
    pub anchor: Tick,
    pub level: [f64; K],
    /// Effective inflow per tick (weighted, over tolerance).
    pub rate: [f64; K],
}

fn decay(lambda: f64, dt: u64) -> f64 {
    if lambda <= 0.0 {
        1.0
    } else {
        (-lambda * dt as f64).exp()
    }
}

impl Grievance {
    pub fn new(at: Tick) -> Self {
        Grievance {
            anchor: at,
            level: [0.0; K],
            rate: [0.0; K],
        }
    }

    /// Each component at `t` (at or after the anchor; earlier reads the anchor).
    pub fn project(&self, t: Tick, lambda: f64) -> [f64; K] {
        let dt = t.0.saturating_sub(self.anchor.0);
        if dt == 0 {
            return self.level;
        }
        let e = decay(lambda, dt);
        let mut out = [0.0; K];
        for (k, o) in out.iter_mut().enumerate() {
            let inflow = if lambda <= 0.0 {
                self.rate[k] * dt as f64
            } else {
                self.rate[k] / lambda * (1.0 - e)
            };
            *o = (self.level[k] * e + inflow).max(0.0);
        }
        out
    }

    pub fn total_at(&self, t: Tick, lambda: f64) -> f64 {
        self.project(t, lambda).iter().sum()
    }

    pub fn level_sum(&self) -> f64 {
        self.level.iter().sum()
    }

    pub fn rate_sum(&self) -> f64 {
        self.rate.iter().sum()
    }

    /// Moves the anchor to `t` (an event).
    pub fn commit(&mut self, t: Tick, lambda: f64) {
        if t.0 <= self.anchor.0 {
            return;
        }
        self.level = self.project(t, lambda);
        self.anchor = t;
    }

    /// An impulse of `amount` (negative takes away, never below zero).
    pub fn add(&mut self, k: SignalKind, amount: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        let l = &mut self.level[k.index()];
        *l = (*l + amount).max(0.0);
    }

    /// Scales every component by `factor` (a tribute or a gift takes a share away).
    pub fn scale(&mut self, factor: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        for l in &mut self.level {
            *l = (*l * factor.max(0.0)).max(0.0);
        }
    }

    /// Takes `amount` away, spread over the components in proportion to their levels.
    pub fn relieve(&mut self, amount: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        let total = self.level_sum();
        if total <= 0.0 || amount <= 0.0 {
            return;
        }
        let f = (1.0 - amount / total).max(0.0);
        for l in &mut self.level {
            *l *= f;
        }
    }

    pub fn set_rate(&mut self, k: SignalKind, per_tick: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        self.rate[k.index()] = per_tick.max(0.0);
    }

    /// The level the total tends to with the current inflow.
    pub fn equilibrium(&self, lambda: f64) -> f64 {
        if lambda <= 0.0 {
            if self.rate_sum() > 0.0 {
                f64::INFINITY
            } else {
                self.level_sum()
            }
        } else {
            self.rate_sum() / lambda
        }
    }

    /// The first tick at or after the anchor when the total reaches `theta` from below, if it
    /// ever does with the current inflow.
    pub fn crossing_up(&self, theta: f64, lambda: f64) -> Option<Tick> {
        let g0 = self.level_sum();
        if g0 >= theta {
            return Some(self.anchor);
        }
        let r = self.rate_sum();
        if r <= 0.0 {
            return None;
        }
        if lambda <= 0.0 {
            return Some(self.anchor + ((theta - g0) / r).ceil() as u64);
        }
        let eq = r / lambda;
        if eq <= theta {
            return None;
        }
        let e = (eq - theta) / (eq - g0);
        let dt = (-e.ln() / lambda).ceil();
        if !dt.is_finite() || dt > 1e15 {
            return None;
        }
        // One tick of slack so the projection at the booked tick is at or above theta despite
        // rounding.
        Some(self.anchor + dt as u64 + 1)
    }

    /// The first tick at or after the anchor when the total falls below `theta`, if it ever does
    /// with the current inflow.
    pub fn crossing_down(&self, theta: f64, lambda: f64) -> Option<Tick> {
        let g0 = self.level_sum();
        if g0 < theta {
            return Some(self.anchor);
        }
        if lambda <= 0.0 {
            return None;
        }
        let eq = self.rate_sum() / lambda;
        if eq >= theta {
            return None;
        }
        let e = (theta - eq) / (g0 - eq);
        let dt = (-e.ln() / lambda).ceil();
        if !dt.is_finite() || dt > 1e15 {
            return None;
        }
        Some(self.anchor + dt as u64 + 1)
    }

    /// The kind holding the most grievance at `t`.
    pub fn dominant(&self, t: Tick, lambda: f64) -> SignalKind {
        let v = self.project(t, lambda);
        let mut best = 0;
        for k in 1..K {
            if v[k] > v[best] {
                best = k;
            }
        }
        SignalKind::ALL[best]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rtsim_kernel::clock::TICKS_PER_DAY;

    const LAMBDA: f64 = std::f64::consts::LN_2 / (6.0 * TICKS_PER_DAY as f64);

    #[test]
    fn halves_in_its_half_life_and_tends_to_rate_over_lambda() {
        let mut g = Grievance::new(Tick(0));
        g.add(SignalKind::Logging, 100.0, Tick(0), LAMBDA);
        let t = Tick::from_days(6);
        assert!((g.total_at(t, LAMBDA) - 50.0).abs() < 1e-9);
        let mut h = Grievance::new(Tick(0));
        h.set_rate(
            SignalKind::Smog,
            25.0 / TICKS_PER_DAY as f64,
            Tick(0),
            LAMBDA,
        );
        let far = h.total_at(Tick::from_days(10_000), LAMBDA);
        assert!((far - h.equilibrium(LAMBDA)).abs() < 1e-6);
    }

    #[test]
    fn crossings_land_on_the_threshold() {
        let mut g = Grievance::new(Tick(1000));
        g.set_rate(
            SignalKind::Smog,
            25.0 / TICKS_PER_DAY as f64,
            Tick(1000),
            LAMBDA,
        );
        let up = g.crossing_up(70.0, LAMBDA).unwrap();
        assert!(g.total_at(up, LAMBDA) >= 70.0);
        assert!(g.total_at(Tick(up.0 - 3), LAMBDA) < 70.0);
        assert_eq!(g.crossing_up(1e6, LAMBDA), None, "above equilibrium");
        g.commit(Tick::from_days(30), LAMBDA);
        g.set_rate(SignalKind::Smog, 0.0, Tick::from_days(30), LAMBDA);
        let down = g.crossing_down(20.0, LAMBDA).unwrap();
        assert!(g.total_at(down, LAMBDA) < 20.0);
        assert!(g.total_at(Tick(down.0 - 3), LAMBDA) >= 20.0);
    }

    proptest! {
        /// Reads commit nothing, and committing at a middle time changes the later value only by
        /// float rounding (the composition rule, up to rounding; events alone move the anchor).
        #[test]
        fn composes_up_to_rounding(
            l0 in 0.0f64..500.0,
            r in 0.0f64..100.0,
            t1 in 0u64..2_000_000,
            t2 in 0u64..2_000_000,
        ) {
            let (t1, t2) = (t1.min(t2), t1.max(t2));
            let mut a = Grievance::new(Tick(0));
            a.add(SignalKind::Mining, l0, Tick(0), LAMBDA);
            a.set_rate(SignalKind::Smog, r / TICKS_PER_DAY as f64, Tick(0), LAMBDA);
            let mut b = a;
            b.commit(Tick(t1), LAMBDA);
            let (va, vb) = (a.total_at(Tick(t2), LAMBDA), b.total_at(Tick(t2), LAMBDA));
            prop_assert!((va - vb).abs() <= 1e-9 * va.abs().max(1.0));
        }
    }
}
