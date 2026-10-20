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
//! **Links.** Packages written into postboxes are ports for `N`, and are learned as their own
//! rates (link processes in the replay). Ender Transmission channels are not links (I13, Ben's
//! correction): their endpoints belong to one district, so a channel is internal plumbing.
//!
//! **Coupling (I6, Ben's "learned coupling").** Which inputs feed which outputs is learned from
//! starvation episodes seen while loaded: a consumed good's stock sits at 0 and it stops being
//! consumed, and every other good of the model either keeps moving at its learned rate or stops.
//! A good that kept moving in every one of an input's episodes does not need that input. Until an
//! input has `coupling_min_episodes` such episodes, it is taken to feed everything: the
//! all-or-nothing process is the default, and the replay splits a district into processes only
//! on evidence ([`Coupling::partition`]). The rule stays "never better than reality": a split is
//! made only where a real episode showed the outputs going on without the input.
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

use crate::ids::{Good, GoodVec};
use crate::tunables::LearnTunables;

/// A flow learned as its own rate rather than as box production.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum LinkKey {
    /// Packages the district's packager wrote into a postbox (inventory index) and trains took
    /// away, by address (NAME id). Positive: into the district, so a packager's rate is negative.
    Mail {
        inventory: u32,
        address: u32,
        good: Good,
    },
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
    /// The longest gap between increases of any produced good while it ran: a batch machine's
    /// cycle, at the sampler's resolution. Restarts after a stop wait this long (F23).
    pub latency_ticks: u64,
}

impl Learned {
    pub fn rate(&self, g: Good) -> f64 {
        self.rates.iter().find(|r| r.0 == g).map_or(0.0, |r| r.1)
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
    /// Whole multiples of the longest window this one has run without a steady result.
    failed_marks: u64,
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

/// What an input's confirmed starvation episodes showed (I6).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InputEvidence {
    /// Valid episodes seen: the input sat at 0, was not consumed, and the measured span was long
    /// enough to judge.
    pub episodes: u32,
    /// Goods that kept moving at `coupling_moved_share` of their learned rate in every episode
    /// (sorted): they do not need this input.
    pub independent: Vec<Good>,
}

/// One input sitting at 0, being watched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Episode {
    start: u64,
    /// The first sweep after the settle time: (tick, N per good) where measurement begins.
    measure: Option<(u64, GoodVec)>,
    /// The latest sweep that still found the input at 0.
    last_zero: (u64, GoodVec),
}

/// Learned coupling (I6): starvation episodes in, a partition of the model's goods out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coupling {
    /// Per consumed good, sorted.
    pub evidence: Vec<(Good, InputEvidence)>,
    open: Vec<(Good, Episode)>,
}

/// More goods than this and the district stays one all-or-nothing process (the partition is
/// quadratic in goods). PLACEHOLDER 48.
pub const COUPLING_MAX_GOODS: usize = 48;

impl Coupling {
    /// Forgets everything (a player edit: the plumbing may have changed).
    pub fn reset(&mut self) {
        self.evidence.clear();
        self.open.clear();
    }

    /// Open episodes end unjudged (the window was discarded: an unload, an edit, a session).
    fn abandon(&mut self) {
        self.open.clear();
    }

    pub fn evidence_for(&self, c: Good) -> Option<&InputEvidence> {
        self.evidence
            .binary_search_by_key(&c, |e| e.0)
            .ok()
            .map(|i| &self.evidence[i].1)
    }

    /// One live sweep: `n` is stock minus cumulative ports (internal net production up to a
    /// constant), under the current model.
    fn observe(&mut self, t: u64, stock: &GoodVec, n: &GoodVec, m: &Learned, tun: &LearnTunables) {
        let settle = tun
            .coupling_settle_ticks
            .max(m.latency_ticks.saturating_mul(2));
        let consumed: Vec<Good> = m.consumed().collect();
        // Episodes of goods no longer consumed by the model end unjudged.
        self.open.retain(|(c, _)| consumed.contains(c));
        for c in consumed {
            let empty = stock.get(c) <= 0;
            let at = self.open.binary_search_by_key(&c, |e| e.0);
            match (at, empty) {
                (Err(i), true) => self.open.insert(
                    i,
                    (
                        c,
                        Episode {
                            start: t,
                            measure: None,
                            last_zero: (t, n.clone()),
                        },
                    ),
                ),
                (Ok(i), true) => {
                    let ep = &mut self.open[i].1;
                    if ep.measure.is_none() && t >= ep.start.saturating_add(settle) {
                        ep.measure = Some((t, n.clone()));
                    }
                    ep.last_zero = (t, n.clone());
                }
                (Ok(i), false) => {
                    let (_, ep) = self.open.remove(i);
                    self.judge(c, &ep, m, tun);
                }
                (Err(_), false) => {}
            }
        }
    }

    /// An episode that ended with the input restocked.
    fn judge(&mut self, c: Good, ep: &Episode, m: &Learned, tun: &LearnTunables) {
        let Some((t0, n0)) = &ep.measure else {
            return;
        };
        let (t1, n1) = &ep.last_zero;
        let span = t1.saturating_sub(*t0);
        let min_span = tun
            .coupling_min_span_ticks
            .max(m.latency_ticks.saturating_mul(10));
        if span < min_span.max(1) {
            return;
        }
        let span_f = span as f64;
        // The input must really have stopped: at 0 in stock but fed and used as fast as it came
        // (a train unloading straight into a machine) is no starvation, and judging it would split
        // a line from the input it needs.
        let rc = m.rate(c);
        let used = (n0.get(c) - n1.get(c)) as f64;
        if rc >= 0.0 || used > tun.coupling_stopped_share * -rc * span_f {
            return;
        }
        let mut moved: Vec<Good> = Vec::new();
        for &(g, r) in &m.rates {
            if g == c {
                continue;
            }
            let expect = r * span_f;
            if expect.abs() < tun.coupling_min_units {
                // Too little to tell: not independent (the safe side).
                continue;
            }
            let actual = (n1.get(g) - n0.get(g)) as f64;
            if actual / expect >= tun.coupling_moved_share {
                moved.push(g);
            }
        }
        moved.sort();
        let i = match self.evidence.binary_search_by_key(&c, |e| e.0) {
            Ok(i) => i,
            Err(i) => {
                self.evidence.insert(
                    i,
                    (
                        c,
                        InputEvidence {
                            episodes: 0,
                            independent: moved.clone(),
                        },
                    ),
                );
                i
            }
        };
        let ev = &mut self.evidence[i].1;
        if ev.episodes > 0 {
            ev.independent.retain(|g| moved.binary_search(g).is_ok());
        }
        ev.episodes = ev.episodes.saturating_add(1);
    }

    /// Splits a model's goods into processes, each of which runs at full rate or not at all
    /// (I6). `goods` are the model's goods with the sign of their rate (negative: consumed), and
    /// `targets` the goods with a learned target. Two goods share a process unless the evidence
    /// separates them:
    ///
    /// 1. every consumed good feeds every produced good, unless it has `min_episodes` confirmed
    ///    episodes and that output kept moving in every one of them;
    /// 2. a learned target was seen with every output quiet, so a target good joins every
    ///    produced good;
    /// 3. outputs fed by no input (a mob farm, a generator) are one process: nothing ever showed
    ///    them apart.
    ///
    /// Components come out sorted by their smallest good, each sorted: deterministic.
    pub fn partition(
        &self,
        goods: &[(Good, i64)],
        targets: &[Good],
        min_episodes: u32,
    ) -> Vec<Vec<Good>> {
        let n = goods.len();
        if n == 0 {
            return Vec::new();
        }
        if n > COUPLING_MAX_GOODS {
            let mut all: Vec<Good> = goods.iter().map(|g| g.0).collect();
            all.sort();
            return vec![all];
        }
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        fn union(p: &mut [usize], a: usize, b: usize) {
            let (ra, rb) = (find(p, a), find(p, b));
            if ra != rb {
                p[ra.max(rb)] = ra.min(rb);
            }
        }
        let produced: Vec<usize> = (0..n).filter(|&i| goods[i].1 > 0).collect();
        for (ci, &(c, rate)) in goods.iter().enumerate() {
            if rate >= 0 {
                continue;
            }
            let ev = self
                .evidence_for(c)
                .filter(|e| e.episodes >= min_episodes.max(1));
            for &o in &produced {
                let free = ev.is_some_and(|e| e.independent.binary_search(&goods[o].0).is_ok());
                if !free {
                    union(&mut parent, ci, o);
                }
            }
        }
        if let Some(&first) = produced.first() {
            for (ti, _) in goods
                .iter()
                .enumerate()
                .filter(|(_, g)| targets.contains(&g.0))
            {
                for &o in &produced {
                    union(&mut parent, ti, o);
                }
                union(&mut parent, ti, first);
            }
        }
        // Components holding no consumed good are one process.
        let has_input =
            |p: &mut Vec<usize>, root: usize| (0..n).any(|i| goods[i].1 < 0 && find(p, i) == root);
        let mut inputless: Option<usize> = None;
        for i in 0..n {
            let r = find(&mut parent, i);
            if r == i && !has_input(&mut parent, r) {
                match inputless {
                    None => inputless = Some(r),
                    Some(a) => union(&mut parent, a, r),
                }
            }
        }
        let mut groups: std::collections::BTreeMap<usize, Vec<Good>> = Default::default();
        for (i, g) in goods.iter().enumerate() {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(g.0);
        }
        let mut out: Vec<Vec<Good>> = groups
            .into_values()
            .map(|mut v| {
                v.sort();
                v
            })
            .collect();
        out.sort();
        out
    }
}

/// One district's learner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Learner {
    window: Option<Window>,
    stops: Stops,
    /// Which inputs feed which outputs (I6): kept across windows and unloads, reset by edits.
    pub coupling: Coupling,
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

    /// The steady test on the trailing window `[from, end]`: an early transient (a line waiting for
    /// its first train) leaves the window instead of failing every test after it.
    /// The longest gap between increases of N for good column `col` within `[from, end]`.
    fn rise_gap(&self, col: usize, from: u64) -> u64 {
        let mut last: Option<(u64, i64)> = None;
        let mut gap = 0;
        for p in self.points.iter().filter(|p| p.t >= from) {
            let n = p.n.get(col).copied().unwrap_or(0);
            match last {
                Some((t0, n0)) if n > n0 => {
                    gap = gap.max(p.t - t0);
                    last = Some((p.t, n));
                }
                None => last = Some((p.t, n)),
                _ => {}
            }
        }
        gap
    }

    fn evaluate(&self, from: u64, tun: &LearnTunables) -> Option<Fit> {
        let end = self.points.last()?.t;
        let from = from.max(self.start);
        let mid = from + (end - from) / 2;
        let first = self
            .points
            .iter()
            .filter(|p| p.t >= from && p.t < mid)
            .count();
        let second = self.points.iter().filter(|p| p.t >= mid).count();
        if first < tun.min_points_per_half || second < tun.min_points_per_half {
            return None;
        }
        let floor = tun.rate_floor_per_day / TICKS_PER_DAY as f64;
        let mut worst = 0.0f64;
        let mut check = |col: usize, link: bool| -> Option<f64> {
            let (x, y) = self.series(col, link, from, end);
            let r = slope(&x, &y);
            if r.abs() < floor {
                return Some(0.0);
            }
            let (x1, y1) = self.series(col, link, from, mid.saturating_sub(1));
            let (x2, y2) = self.series(col, link, mid, end);
            let (r1, r2) = (slope(&x1, &y1), slope(&x2, &y2));
            let big = r1.abs().max(r2.abs());
            let rel = if big == 0.0 {
                0.0
            } else {
                (r1 - r2).abs() / (tun.steady_epsilon * big)
            };
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

/// A steady window's rates, link rates and confidence.
type Fit = (Vec<(Good, f64)>, Vec<(LinkKey, f64)>, f64);

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
        self.coupling.abandon();
    }

    /// Feeds one live sweep. Returns a model when a window turns out steady.
    pub fn push(
        &mut self,
        s: Sample<'_>,
        model: Option<&Learned>,
        tun: &LearnTunables,
    ) -> Option<Learned> {
        let t = s.at.0;
        if let Some(m) = model {
            self.track_stops(t, s.stock, m, s.space_free, tun);
            let n = s.stock.minus(s.ports);
            self.coupling.observe(t, s.stock, &n, m, tun);
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
        let result = w.evaluate(t - need, tun);
        let latency = result.as_ref().map_or(0, |(rates, _, _)| {
            rates
                .iter()
                .filter(|r| r.1 > 0.0)
                .filter_map(|r| w.goods.binary_search(&r.0).ok())
                .map(|col| w.rise_gap(col, t - need))
                .max()
                .unwrap_or(0)
        });
        // Points older than the longest window are never used again.
        let horizon = t.saturating_sub(tun.window_max_ticks);
        if w.points.first().is_some_and(|p| p.t < horizon) {
            w.points.retain(|p| p.t >= horizon);
        }
        match result {
            Some((rates, links, confidence)) if confidence >= tun.min_confidence => {
                let learned = Learned {
                    at: s.at,
                    window_ticks: need,
                    rates,
                    links,
                    confidence,
                    points: w.points.len(),
                    longest_gap: w.effective_gap(t),
                    latency_ticks: latency,
                };
                self.windows_steady += 1;
                self.last_end = Some(WindowEnd::Steady);
                self.window = Some(Window::open(t, s.stock, s.ports, s.links));
                Some(learned)
            }
            // Not steady yet: the trailing window slides on. Count a failure once per window length.
            _ => {
                let marks = span / tun.window_max_ticks.max(1);
                if marks > w.failed_marks {
                    w.failed_marks = marks;
                    self.windows_failed += 1;
                    self.last_end = Some(WindowEnd::NotSteady);
                }
                None
            }
        }
    }

    fn track_stops(
        &mut self,
        t: u64,
        stock: &GoodVec,
        m: &Learned,
        space_free: bool,
        tun: &LearnTunables,
    ) {
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
            let levels: Vec<i64> = st
                .stop_levels
                .iter()
                .map(|v| v.get(g))
                .filter(|&l| l > 0)
                .collect();
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
        assert!(
            (m.rate(INGOT) - 0.02).abs() < 1e-3,
            "ingot {}",
            m.rate(INGOT)
        );
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
            latency_ticks: 0,
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

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    impl Persist for LinkKey {
        fn put(&self, w: &mut Writer) {
            match *self {
                LinkKey::Mail {
                    inventory,
                    address,
                    good,
                } => {
                    w.u8(1);
                    inventory.put(w);
                    address.put(w);
                    good.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                1 => LinkKey::Mail {
                    inventory: Persist::get(r)?,
                    address: Persist::get(r)?,
                    good: Persist::get(r)?,
                },
                _ => return invalid("link key"),
            })
        }
    }

    persist_unit_enum!(
        WindowEnd,
        [
            WindowEnd::Steady,
            WindowEnd::NotSteady,
            WindowEnd::Discarded
        ]
    );
    persist_struct!(Learned {
        at,
        window_ticks,
        rates,
        links,
        confidence,
        points,
        longest_gap,
        latency_ticks
    });
    persist_struct!(LearnedTarget {
        good,
        stop_at,
        restart_at
    });
    persist_struct!(Stops {
        last_rise,
        last_s,
        stopped,
        stop_levels,
        restart_levels
    });
    persist_struct!(Point { t, n, l });
    persist_struct!(InputEvidence {
        episodes,
        independent
    });
    persist_struct!(Episode {
        start,
        measure,
        last_zero
    });
    persist_struct!(Coupling { evidence, open });
    persist_struct!(Window {
        start,
        s0,
        p0,
        l0,
        goods,
        links,
        points,
        last_s,
        last_change,
        longest_gap,
        failed_marks,
    });
    // The open window is saved too, so a snapshot is exact; a session start still discards it
    // (F6), which is the host's call after a real restart.
    persist_struct!(Learner {
        window,
        stops,
        coupling,
        windows_steady,
        windows_failed,
        last_end
    });
}
