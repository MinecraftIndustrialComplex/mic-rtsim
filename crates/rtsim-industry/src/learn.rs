// SPDX-License-Identifier: AGPL-3.0-or-later
//! Learning a district while it is loaded (INDUSTRY.md §3.4, I5).
//!
//! **The identity.** Internal net production of good g is `N_g(t) = S_g(t) - S_g(t0) - P_g(t)`,
//! stock change minus what crossed the boundary through ports since the window began. Machines
//! never need to be seen: in steady state the work in progress is constant, so it does not bias the
//! slope of `N_g`.
//!
//! **The window.** Each good's rate is the least-squares slope of `N_g` over a window of at least
//! `max(W_min, k × the longest quiet gap in any good's stock)` (both TUNABLE), so a train that calls
//! every T minutes, or a batch machine, is spanned several times. The window is steady when, for
//! every good whose rate is above the floor, the slopes of the two halves agree within ε. A steady
//! window replaces the model; learning never stops while loaded.
//!
//! **Links.** Flows through Ender Transmission channels and packages written into postboxes are
//! ports for `N`, and are learned as their own rates (link processes in the replay).
//!
//! **Targets.** A produced good whose stock sat still for `stop_quiet_ticks` while every consumed
//! good had stock and the output space was free is a stop; two stops within `target_tolerance`
//! confirm a learned target (I6), with the level production restarted at.
//!
//! Everything here is `f64`, deterministic per platform and build, and converted once to the
//! replay's fixed point when a model is adopted; adopted models are stored, never recomputed (the
//! kernel's rule for float results, DECISIONS.md O14).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use serde::Serialize;

use crate::ids::{ChannelId, Good, GoodVec};
use crate::tunables::LearnTunables;

/// A flow learned as its own rate rather than as box production.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum LinkKey {
    /// Ender Transmission channel (I13). Positive: into the district.
    Channel(ChannelId, Good),
    /// Packages the district's packager wrote into a postbox (inventory index) and trains took
    /// away, by address (NAME id). Positive: into the district, so a packager's rate is negative.
    Mail { inventory: u32, address: u32, good: Good },
}

/// Why a window ended without a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum WindowEnd {
    Steady,
    /// The halves disagreed until the window reached its maximum.
    NotSteady,
    /// An edit, a discard, an unload, or a session start threw it away.
    Discarded,
}

/// What a steady window found.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Learned {
    #[serde(serialize_with = "crate::ser::tick")]
    pub at: Tick,
    pub window_ticks: u64,
    /// Units per tick, goods above the floor only; negative are consumed.
    pub rates: Vec<(Good, f64)>,
    pub links: Vec<(LinkKey, f64)>,
    /// 1 when the halves agree exactly, 0 at the edge of ε (TUNABLE gate, `min_confidence`).
    pub confidence: f64,
    pub points: usize,
    pub longest_gap: u64,
}

impl Learned {
    pub fn rate(&self, g: Good) -> f64 {
        self.rates
            .iter()
            .find(|r| r.0 == g)
            .map_or(0.0, |r| r.1)
    }

    pub fn consumed(&self) -> impl Iterator<Item = Good> + '_ {
        self.rates.iter().filter(|r| r.1 < 0.0).map(|r| r.0)
    }

    pub fn produced(&self) -> impl Iterator<Item = Good> + '_ {
        self.rates.iter().filter(|r| r.1 > 0.0).map(|r| r.0)
    }
}

/// A target learned from repeated stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LearnedTarget {
    pub good: Good,
    pub stop_at: i64,
    pub restart_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Point {
    t: u64,
    /// N per column of `Window::goods`.
    n: Vec<i64>,
    /// Cumulative link flow per column of `Window::links`, since the window began.
    l: Vec<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Window {
    start: u64,
    s0: GoodVec,
    p0: GoodVec,
    l0: Vec<(LinkKey, i64)>,
    goods: Vec<Good>,
    links: Vec<LinkKey>,
    points: Vec<Point>,
    last_s: GoodVec,
    /// Per good: when its stock last changed, and whether it has changed in this window.
    last_change: Vec<(Good, u64, bool)>,
    longest_gap: u64,
}

/// The stop tracker for targets.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct Stops {
    /// Per produced good: when its stock last rose.
    last_rise: Vec<(Good, u64)>,
    last_s: GoodVec,
    stopped: bool,
    /// Stock levels at each stop seen, newest last (capped).
    stop_levels: Vec<GoodVec>,
    /// Stock levels when production resumed after a stop.
    restart_levels: Vec<GoodVec>,
}

/// One district's learner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Learner {
    window: Option<Window>,
    stops: Stops,
    pub windows_steady: u32,
    pub windows_failed: u32,
    pub last_end: Option<WindowEnd>,
}

/// Least-squares slope of `y` against `x`, or 0 with fewer than two distinct x.
fn slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    if x.len() < 2 {
        return 0.0;
    }
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
    }
    if sxx == 0.0 { 0.0 } else { sxy / sxx }
}

impl Window {
    fn open(t: u64, stock: &GoodVec, ports: &GoodVec, links: &[(LinkKey, i64)]) -> Window {
        let mut w = Window {
            start: t,
            s0: stock.clone(),
            p0: ports.clone(),
            l0: links.to_vec(),
            last_s: stock.clone(),
            ..Window::default()
        };
        w.points.push(Point {
            t,
            n: Vec::new(),
            l: Vec::new(),
        });
        w
    }

    fn column(&mut self, g: Good) -> usize {
        match self.goods.binary_search(&g) {
            Ok(i) => i,
            Err(i) => {
                self.goods.insert(i, g);
                for p in &mut self.points {
                    if p.n.len() >= i {
                        p.n.insert(i, 0);
                    }
                }
                i
            }
        }
    }

    fn link_column(&mut self, k: LinkKey) -> usize {
        match self.links.binary_search(&k) {
            Ok(i) => i,
            Err(i) => {
                self.links.insert(i, k);
                for p in &mut self.points {
                    if p.l.len() >= i {
                        p.l.insert(i, 0);
                    }
                }
                i
            }
        }
    }

    fn push(&mut self, t: u64, stock: &GoodVec, ports: &GoodVec, links: &[(LinkKey, i64)]) {
        // Quiet gaps, on stock.
        let goods: Vec<Good> = {
            let mut g: Vec<Good> = stock.goods().chain(self.last_s.goods()).collect();
            g.sort();
            g.dedup();
            g
        };
        for g in goods {
            let changed = stock.get(g) != self.last_s.get(g);
            let i = match self.last_change.binary_search_by_key(&g, |e| e.0) {
                Ok(i) => i,
                Err(i) => {
                    self.last_change.insert(i, (g, self.start, false));
                    i
                }
            };
            if changed {
                let gap = t - self.last_change[i].1;
                if self.last_change[i].2 {
                    self.longest_gap = self.longest_gap.max(gap);
                }
                self.last_change[i] = (g, t, true);
            }
        }
        self.last_s = stock.clone();
        // N per good.
        let mut n = stock.minus(&self.s0);
        n.subtract(&ports.minus(&self.p0));
        for g in n.goods().collect::<Vec<_>>() {
            self.column(g);
        }
        let mut row = vec![0i64; self.goods.len()];
        for (i, &g) in self.goods.iter().enumerate() {
            row[i] = n.get(g);
        }
        for &(k, _) in links {
            self.link_column(k);
        }
        let mut lrow = vec![0i64; self.links.len()];
        for (i, &k) in self.links.iter().enumerate() {
            let now = links.iter().find(|e| e.0 == k).map_or(0, |e| e.1);
            let then = self.l0.iter().find(|e| e.0 == k).map_or(0, |e| e.1);
            lrow[i] = now - then;
        }
        // Earlier points may be shorter than the columns (a good seen first now): pad.
        for p in &mut self.points {
            p.n.resize(self.goods.len(), 0);
            p.l.resize(self.links.len(), 0);
        }
        self.points.push(Point { t, n: row, l: lrow });
    }

    fn decimate(&mut self, max_points: usize) {
        if self.points.len() > max_points.max(4) {
            // Keep the first and every other point after it; the last is kept too.
            let last = self.points.last().cloned();
            let mut i = 0;
            self.points.retain(|_| {
                let keep = i % 2 == 0;
                i += 1;
                keep
            });
            if let Some(l) = last
                && self.points.last() != Some(&l)
            {
                self.points.push(l);
            }
        }
    }

    /// The open gap of goods that changed at least once (a batch longer than the window so far).
    fn effective_gap(&self, t: u64) -> u64 {
        let open = self
            .last_change
            .iter()
            .filter(|e| e.2)
            .map(|e| t - e.1)
            .max()
            .unwrap_or(0);
        self.longest_gap.max(open)
    }

    fn series(&self, col: usize, link: bool, from: u64, to: u64) -> (Vec<f64>, Vec<f64>) {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for p in &self.points {
            if p.t < from || p.t > to {
                continue;
            }
            x.push((p.t - self.start) as f64);
            y.push(if link { p.l[col] } else { p.n[col] } as f64);
        }
        (x, y)
    }

    fn evaluate(&self, tun: &LearnTunables) -> Option<(Vec<(Good, f64)>, Vec<(LinkKey, f64)>, f64)> {
        let end = self.points.last()?.t;
        let mid = self.start + (end - self.start) / 2;
        let first = self.points.iter().filter(|p| p.t < mid).count();
        let second = self.points.iter().filter(|p| p.t >= mid).count();
        if first < tun.min_points_per_half || second < tun.min_points_per_half {
            return None;
        }
        let floor = tun.rate_floor_per_day / TICKS_PER_DAY as f64;
        let mut worst = 0.0f64;
        let mut check = |col: usize, link: bool| -> Option<f64> {
            let (x, y) = self.series(col, link, self.start, end);
            let r = slope(&x, &y);
            if r.abs() < floor {
                return Some(0.0);
            }
            let (x1, y1) = self.series(col, link, self.start, mid.saturating_sub(1));
            let (x2, y2) = self.series(col, link, mid, end);
            let (r1, r2) = (slope(&x1, &y1), slope(&x2, &y2));
            let big = r1.abs().max(r2.abs());
            let rel = if big == 0.0 { 0.0 } else { (r1 - r2).abs() / (tun.steady_epsilon * big) };
            worst = worst.max(rel);
            (rel <= 1.0).then_some(r)
        };
        let mut rates = Vec::new();
        for (i, &g) in self.goods.iter().enumerate() {
            let r = check(i, false)?;
            if r != 0.0 {
                rates.push((g, r));
            }
        }
        let mut links = Vec::new();
        for (i, &k) in self.links.iter().enumerate() {
            let r = check(i, true)?;
            if r != 0.0 {
                links.push((k, r));
            }
        }
        Some((rates, links, (1.0 - worst).clamp(0.0, 1.0)))
    }
}

/// What one sweep tells the learner.
pub struct Sample<'a> {
    pub at: Tick,
    /// Boundary stock, summed over the district's inventories.
    pub stock: &'a GoodVec,
    /// Cumulative port imports (positive in), any origin; only differences are used.
    pub ports: &'a GoodVec,
    /// Cumulative link flows, same convention.
    pub links: &'a [(LinkKey, i64)],
    /// The receiving inventories have room (an output stop is then not a full output).
    pub space_free: bool,
}

impl Learner {
    pub fn new() -> Self {
        Self::default()
    }

    /// The window has been open since, if one is.
    pub fn window_start(&self) -> Option<Tick> {
        self.window.as_ref().map(|w| Tick(w.start))
    }

    pub fn window_points(&self) -> usize {
        self.window.as_ref().map_or(0, |w| w.points.len())
    }

    /// Ticks the open window still needs at least, for the ledger's "learning: N min more".
    pub fn ticks_to_go(&self, now: Tick, tun: &LearnTunables) -> Option<u64> {
        let w = self.window.as_ref()?;
        let need = tun
            .window_min_ticks
            .max(tun.window_gap_factor * w.effective_gap(now.0))
            .min(tun.window_max_ticks);
        Some(need.saturating_sub(now.0 - w.start))
    }

    /// Throws the open window away (an edit, a discard, an unload, a session start).
    pub fn discard(&mut self) {
        if self.window.take().is_some() {
            self.last_end = Some(WindowEnd::Discarded);
        }
        self.stops.stopped = false;
        self.stops.last_rise.clear();
        self.stops.last_s = GoodVec::new();
    }

    /// Feeds one live sweep. Returns a model when a window turns out steady.
    pub fn push(&mut self, s: Sample<'_>, model: Option<&Learned>, tun: &LearnTunables) -> Option<Learned> {
        let t = s.at.0;
        if let Some(m) = model {
            self.track_stops(t, s.stock, m, s.space_free, tun);
        }
        let Some(w) = self.window.as_mut() else {
            self.window = Some(Window::open(t, s.stock, s.ports, s.links));
            return None;
        };
        if t <= w.points.last().map_or(w.start, |p| p.t) {
            return None;
        }
        w.push(t, s.stock, s.ports, s.links);
        w.decimate(tun.max_points);
        let span = t - w.start;
        let need = tun
            .window_min_ticks
            .max(tun.window_gap_factor.saturating_mul(w.effective_gap(t)))
            .min(tun.window_max_ticks);
        if span < need {
            return None;
        }
        let result = w.evaluate(tun);
        match result {
            Some((rates, links, confidence)) if confidence >= tun.min_confidence => {
                let learned = Learned {
                    at: s.at,
                    window_ticks: span,
                    rates,
                    links,
                    confidence,
                    points: w.points.len(),
                    longest_gap: w.effective_gap(t),
                };
                self.windows_steady += 1;
                self.last_end = Some(WindowEnd::Steady);
                self.window = Some(Window::open(t, s.stock, s.ports, s.links));
                Some(learned)
            }
            _ if span >= tun.window_max_ticks => {
                self.windows_failed += 1;
                self.last_end = Some(WindowEnd::NotSteady);
                self.window = Some(Window::open(t, s.stock, s.ports, s.links));
                None
            }
            _ => None,
        }
    }

    fn track_stops(&mut self, t: u64, stock: &GoodVec, m: &Learned, space_free: bool, tun: &LearnTunables) {
        let st = &mut self.stops;
        let mut rose = false;
        for g in m.produced() {
            let i = match st.last_rise.binary_search_by_key(&g, |e| e.0) {
                Ok(i) => i,
                Err(i) => {
                    st.last_rise.insert(i, (g, t));
                    i
                }
            };
            if stock.get(g) > st.last_s.get(g) {
                st.last_rise[i].1 = t;
                rose = true;
            }
        }
        let inputs_ok = m.consumed().all(|g| stock.get(g) > 0);
        let quiet = !st.last_rise.is_empty()
            && st.last_rise.iter().all(|e| t - e.1 >= tun.stop_quiet_ticks);
        if !st.stopped && quiet && inputs_ok && space_free {
            st.stopped = true;
            st.stop_levels.push(stock.clone());
            if st.stop_levels.len() > 8 {
                st.stop_levels.remove(0);
            }
        } else if st.stopped && rose {
            st.stopped = false;
            // The level it restarted at is the one before the rise.
            st.restart_levels.push(st.last_s.clone());
            if st.restart_levels.len() > 8 {
                st.restart_levels.remove(0);
            }
        }
        st.last_s = stock.clone();
    }

    /// Targets confirmed by two stops within the tolerance, for the produced goods of `m`.
    /// Conservative: the lowest stop level and the lowest restart level seen.
    pub fn targets(&self, m: &Learned, tun: &LearnTunables) -> Vec<LearnedTarget> {
        let st = &self.stops;
        let mut out = Vec::new();
        for g in m.produced() {
            let levels: Vec<i64> = st.stop_levels.iter().map(|v| v.get(g)).filter(|&l| l > 0).collect();
            let mut confirmed: Option<i64> = None;
            for (i, a) in levels.iter().enumerate() {
                for b in &levels[i + 1..] {
                    if (a - b).abs() <= tun.target_tolerance {
                        let lo = (*a).min(*b);
                        confirmed = Some(confirmed.map_or(lo, |c: i64| c.min(lo)));
                    }
                }
            }
            if let Some(stop_at) = confirmed {
                let restart_at = st
                    .restart_levels
                    .iter()
                    .map(|v| v.get(g))
                    .filter(|&l| l < stop_at)
                    .min()
                    .unwrap_or(stop_at - tun.target_tolerance)
                    .max(0);
                out.push(LearnedTarget {
                    good: g,
                    stop_at,
                    restart_at,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORE: Good = Good(1);
    const INGOT: Good = Good(2);

    fn tun() -> LearnTunables {
        LearnTunables {
            window_min_ticks: 4_000,
            window_gap_factor: 4,
            window_max_ticks: 40_000,
            min_points_per_half: 4,
            ..LearnTunables::default()
        }
    }

    /// A smelter: every 100 ticks it takes 2 ore and makes 2 ingots; a train brings 200 ore every
    /// 2,000 ticks. Sampled every 100 ticks.
    fn run(ticks: u64, learner: &mut Learner, t: &LearnTunables) -> Option<Learned> {
        let mut ore = 100i64;
        let mut ingot = 0i64;
        let mut ports = GoodVec::new();
        let mut got = None;
        for step in 0..=ticks / 100 {
            let now = step * 100;
            if now % 2_000 == 0 && now > 0 {
                ore += 200;
                ports.add(ORE, 200);
            }
            if now > 0 && ore >= 2 {
                ore -= 2;
                ingot += 2;
            }
            let stock = GoodVec::from_pairs([(ORE, ore), (INGOT, ingot)]);
            let s = Sample {
                at: Tick(now),
                stock: &stock,
                ports: &ports,
                links: &[],
                space_free: true,
            };
            if let Some(l) = learner.push(s, None, t) {
                got.get_or_insert(l);
            }
        }
        got
    }

    #[test]
    fn learns_a_steady_line_through_train_steps() {
        let t = tun();
        let mut l = Learner::new();
        let m = run(20_000, &mut l, &t).expect("a steady window");
        // 2 per 100 ticks = 0.02 per tick, consumed and produced; train steps are ports.
        assert!((m.rate(ORE) + 0.02).abs() < 1e-3, "ore {}", m.rate(ORE));
        assert!((m.rate(INGOT) - 0.02).abs() < 1e-3, "ingot {}", m.rate(INGOT));
        assert!(m.window_ticks >= 4_000);
        assert!(m.confidence > 0.5);
    }

    #[test]
    fn a_rate_change_mid_window_is_not_steady() {
        let t = LearnTunables {
            window_max_ticks: 8_000,
            ..tun()
        };
        let mut l = Learner::new();
        let mut ingot = 0i64;
        let ports = GoodVec::new();
        let mut got = None;
        for step in 0..=80 {
            let now = step * 100;
            // An accelerating line (a ramp): no two half-windows agree.
            ingot += 1 + step as i64 / 4;
            let stock = GoodVec::from_pairs([(INGOT, ingot)]);
            if let Some(m) = l.push(
                Sample {
                    at: Tick(now),
                    stock: &stock,
                    ports: &ports,
                    links: &[],
                    space_free: true,
                },
                None,
                &t,
            ) {
                got = Some(m);
            }
        }
        assert!(got.is_none());
        assert_eq!(l.last_end, Some(WindowEnd::NotSteady));
    }

    #[test]
    fn quiet_gaps_stretch_the_window() {
        // A batch machine that outputs 50 every 3,000 ticks needs a window of 4 × 3,000.
        let t = tun();
        let mut l = Learner::new();
        let ports = GoodVec::new();
        let mut learned_at = None;
        for step in 0..=300 {
            let now = step * 100;
            let ingot = (now / 3_000) as i64 * 50;
            let stock = GoodVec::from_pairs([(INGOT, ingot)]);
            if let Some(m) = l.push(
                Sample {
                    at: Tick(now),
                    stock: &stock,
                    ports: &ports,
                    links: &[],
                    space_free: true,
                },
                None,
                &t,
            ) {
                learned_at.get_or_insert(m.window_ticks);
            }
        }
        let w = learned_at.expect("learned");
        assert!(w >= 12_000, "window {w} shorter than 4 batches");
    }

    #[test]
    fn repeated_stops_confirm_a_target() {
        let t = LearnTunables {
            stop_quiet_ticks: 500,
            ..tun()
        };
        let model = Learned {
            at: Tick(0),
            window_ticks: 1,
            rates: vec![(ORE, -0.02), (INGOT, 0.02)],
            links: vec![],
            confidence: 1.0,
            points: 0,
            longest_gap: 0,
        };
        let mut l = Learner::new();
        let ports = GoodVec::new();
        // A threshold switch: produce to 1,000, a train takes 600, produce again.
        let mut ingot = 0i64;
        let mut running = true;
        for step in 0..2_000u64 {
            let now = step * 100;
            if running {
                ingot += 10;
                if ingot >= 1_000 {
                    running = false;
                }
            }
            if !running && step % 300 == 0 {
                ingot -= 600;
                running = true;
            }
            let stock = GoodVec::from_pairs([(ORE, 10_000), (INGOT, ingot)]);
            l.push(
                Sample {
                    at: Tick(now),
                    stock: &stock,
                    ports: &ports,
                    links: &[],
                    space_free: true,
                },
                Some(&model),
                &t,
            );
        }
        let targets = l.targets(&model, &t);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].good, INGOT);
        assert_eq!(targets[0].stop_at, 1_000);
        assert_eq!(targets[0].restart_at, 400);
    }
}
