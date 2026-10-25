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

/// The kinds a grievance projects: one component for each [`SignalKind`].
pub const K: usize = SignalKind::COUNT;

/// The kinds a grievance keeps inline: all but [`SignalKind::Rail`], the last. A feud that has no
/// rail signal pays for six components as before M7; the Rail kind's three numbers ride in
/// [`RailTail`], which only a feud with a rail signal has (MZB-313, amended by the lead for
/// MZB-331's gate).
pub const KB: usize = K - 1;

const _: () = assert!(SignalKind::Rail.index() == KB);

/// The Rail kind's components of a feud: its level and inflow rate, which the grievance projects
/// with the rest, and its raw rate (`Feud::raw_of`), kept here so that a feud carries one
/// pointer for the three. `None` while all three are zero.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct RailTail {
    pub level: f64,
    pub rate: f64,
    pub raw: f64,
}

impl RailTail {
    fn is_zero(&self) -> bool {
        self.level == 0.0 && self.rate == 0.0 && self.raw == 0.0
    }
}

/// One feud's grievance: levels at the anchor and inflow rates per tick.
#[derive(Clone, Debug, PartialEq)]
pub struct Grievance {
    pub anchor: Tick,
    /// The levels of the kinds before Rail, by [`SignalKind::index`]; see [`Grievance::level_of`]
    /// for any kind.
    pub level: [f64; KB],
    /// Effective inflow per tick (weighted, over tolerance) of the kinds before Rail; see
    /// [`Grievance::rate_of`].
    pub rate: [f64; KB],
    /// The Rail kind's components, if the feud has any.
    pub rail: Option<Box<RailTail>>,
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
            level: [0.0; KB],
            rate: [0.0; KB],
            rail: None,
        }
    }

    /// The level of `k` at the anchor.
    pub fn level_of(&self, k: SignalKind) -> f64 {
        match self.level.get(k.index()) {
            Some(l) => *l,
            None => self.rail.as_ref().map_or(0.0, |t| t.level),
        }
    }

    /// The inflow of `k` per tick.
    pub fn rate_of(&self, k: SignalKind) -> f64 {
        match self.rate.get(k.index()) {
            Some(r) => *r,
            None => self.rail.as_ref().map_or(0.0, |t| t.rate),
        }
    }

    /// The Rail kind's components, made if the feud has none yet.
    fn rail_mut(&mut self) -> &mut RailTail {
        self.rail.get_or_insert_with(Default::default)
    }

    /// Drops the Rail components once all three are zero, so that a feud with none is the same
    /// value whichever way it got there.
    pub(crate) fn trim_rail(&mut self) {
        if self.rail.as_ref().is_some_and(|t| t.is_zero()) {
            self.rail = None;
        }
    }

    /// The Rail kind's raw rate (before weight and tolerance; see `Feud::raw_of`).
    pub(crate) fn raw_rail(&self) -> f64 {
        self.rail.as_ref().map_or(0.0, |t| t.raw)
    }

    pub(crate) fn set_raw_rail(&mut self, raw: f64) {
        if raw != 0.0 || self.rail.is_some() {
            self.rail_mut().raw = raw;
            self.trim_rail();
        }
    }

    /// Each component at `t` (at or after the anchor; earlier reads the anchor).
    pub fn project(&self, t: Tick, lambda: f64) -> [f64; K] {
        let mut out = [0.0; K];
        let rail = self.rail.as_deref().copied().unwrap_or_default();
        let dt = t.0.saturating_sub(self.anchor.0);
        if dt == 0 {
            out[..KB].copy_from_slice(&self.level);
            out[KB] = rail.level;
            return out;
        }
        let e = decay(lambda, dt);
        let one = |level: f64, rate: f64| {
            let inflow = if lambda <= 0.0 {
                rate * dt as f64
            } else {
                rate / lambda * (1.0 - e)
            };
            (level * e + inflow).max(0.0)
        };
        for (k, o) in out.iter_mut().enumerate().take(KB) {
            *o = one(self.level[k], self.rate[k]);
        }
        out[KB] = one(rail.level, rail.rate);
        out
    }

    pub fn total_at(&self, t: Tick, lambda: f64) -> f64 {
        self.project(t, lambda).iter().sum()
    }

    pub fn level_sum(&self) -> f64 {
        self.level.iter().sum::<f64>() + self.rail.as_ref().map_or(0.0, |t| t.level)
    }

    pub fn rate_sum(&self) -> f64 {
        self.rate.iter().sum::<f64>() + self.rail.as_ref().map_or(0.0, |t| t.rate)
    }

    /// Moves the anchor to `t` (an event).
    pub fn commit(&mut self, t: Tick, lambda: f64) {
        if t.0 <= self.anchor.0 {
            return;
        }
        let p = self.project(t, lambda);
        self.level.copy_from_slice(&p[..KB]);
        if let Some(r) = self.rail.as_mut() {
            r.level = p[KB];
        }
        self.anchor = t;
    }

    /// An impulse of `amount` (negative takes away, never below zero).
    pub fn add(&mut self, k: SignalKind, amount: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        let l = match self.level.get_mut(k.index()) {
            Some(l) => l,
            None => &mut self.rail_mut().level,
        };
        *l = (*l + amount).max(0.0);
        self.trim_rail();
    }

    /// Scales every component by `factor` (a tribute or a gift takes a share away).
    pub fn scale(&mut self, factor: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        for l in self
            .level
            .iter_mut()
            .chain(self.rail.as_mut().map(|t| &mut t.level))
        {
            *l = (*l * factor.max(0.0)).max(0.0);
        }
        self.trim_rail();
    }

    /// Takes `amount` away, spread over the components in proportion to their levels.
    pub fn relieve(&mut self, amount: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        let total = self.level_sum();
        if total <= 0.0 || amount <= 0.0 {
            return;
        }
        let f = (1.0 - amount / total).max(0.0);
        for l in self
            .level
            .iter_mut()
            .chain(self.rail.as_mut().map(|t| &mut t.level))
        {
            *l *= f;
        }
        self.trim_rail();
    }

    pub fn set_rate(&mut self, k: SignalKind, per_tick: f64, t: Tick, lambda: f64) {
        self.commit(t, lambda);
        let v = per_tick.max(0.0);
        match self.rate.get_mut(k.index()) {
            Some(r) => *r = v,
            // A rail signal of nothing makes no components.
            None if v == 0.0 && self.rail.is_none() => {}
            None => {
                self.rail_mut().rate = v;
                self.trim_rail();
            }
        }
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

    /// The Rail kind rides in a tail the other kinds do not, and every method treats it as one
    /// more kind: the same operations on Rail and on Killing give the same numbers, bit for bit.
    #[test]
    fn the_rail_kind_projects_as_any_other_kind_does() {
        fn run(kind: SignalKind) -> (Grievance, [f64; K]) {
            let mut g = Grievance::new(Tick(0));
            g.add(kind, 40.0, Tick(10), LAMBDA);
            g.set_rate(kind, 3.0 / TICKS_PER_DAY as f64, Tick(2_000), LAMBDA);
            g.add(SignalKind::Logging, 7.0, Tick(3_000), LAMBDA);
            g.relieve(5.0, Tick(4_000), LAMBDA);
            g.scale(0.5, Tick(5_000), LAMBDA);
            let at = Tick::from_days(2);
            (g.clone(), g.project(at, LAMBDA))
        }
        let (a, pa) = run(SignalKind::Killing);
        let (b, pb) = run(SignalKind::Rail);
        let (ka, kb) = (SignalKind::Killing.index(), SignalKind::Rail.index());
        assert_eq!(pa[ka].to_bits(), pb[kb].to_bits());
        assert_eq!(pa[kb], 0.0);
        assert_eq!(pb[ka], 0.0);
        assert_eq!(
            a.level_of(SignalKind::Killing),
            b.level_of(SignalKind::Rail)
        );
        assert_eq!(a.rate_of(SignalKind::Killing), b.rate_of(SignalKind::Rail));
        assert_eq!(a.level_sum().to_bits(), b.level_sum().to_bits());
        assert_eq!(a.rate_sum().to_bits(), b.rate_sum().to_bits());
        assert_eq!(
            a.total_at(Tick::from_days(2), LAMBDA).to_bits(),
            pa.iter().sum::<f64>().to_bits()
        );
        assert_eq!(
            a.crossing_up(30.0, LAMBDA),
            b.crossing_up(30.0, LAMBDA),
            "the same crossing"
        );
        assert_eq!(a.dominant(Tick::from_days(2), LAMBDA), SignalKind::Killing);
        assert_eq!(b.dominant(Tick::from_days(2), LAMBDA), SignalKind::Rail);
    }

    /// Only a grievance with a Rail component has the tail, and it is the same value whichever
    /// way it got there.
    #[test]
    fn the_tail_is_made_by_a_rail_component_and_dropped_with_the_last() {
        let mut g = Grievance::new(Tick(0));
        g.add(SignalKind::Logging, 10.0, Tick(0), LAMBDA);
        g.set_rate(SignalKind::Rail, 0.0, Tick(5), LAMBDA);
        g.add(SignalKind::Rail, 0.0, Tick(6), LAMBDA);
        assert!(g.rail.is_none(), "nothing of Rail makes no tail");
        g.set_rate(SignalKind::Rail, 1e-3, Tick(10), LAMBDA);
        assert_eq!(g.rail.as_deref().map(|t| t.rate), Some(1e-3));
        g.set_rate(SignalKind::Rail, 0.0, Tick(20), LAMBDA);
        assert!(g.rail.is_some(), "the inflow left a level");
        assert!(g.level_of(SignalKind::Rail) > 0.0);
        g.relieve(1e9, Tick(20), LAMBDA);
        assert!(g.rail.is_none(), "all zero again");
        g.set_raw_rail(2.0);
        assert_eq!(g.raw_rail(), 2.0);
        assert!(g.rail.is_some(), "a raw rate keeps the tail");
        g.set_raw_rail(0.0);
        assert!(g.rail.is_none());
    }

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
            let mut b = a.clone();
            b.commit(Tick(t1), LAMBDA);
            let (va, vb) = (a.total_at(Tick(t2), LAMBDA), b.total_at(Tick(t2), LAMBDA));
            prop_assert!((va - vb).abs() <= 1e-9 * va.abs().max(1.0));
        }
    }
}
