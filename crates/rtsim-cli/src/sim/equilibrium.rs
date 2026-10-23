// SPDX-License-Identifier: AGPL-3.0-or-later
//! EQUILIBRIUM.md's long-horizon checks on the synthetic worlds (§3.6, §3.7), run as tests:
//! slice S2's herds on the census's `ranges` scenario ([`Ecology::ranges`]: herd ranges founded
//! as the world model founds them, under the phase 1 dragon world's lairs), with nobody watching.
//!
//! - **Settling** ([`settle`]): a world's herds over their K, averaged over a late window.
//! - **Wipes** ([`wipe_run`]): the same seed run twice, a control and a world where every herd of
//!   some ranges is killed at `settle_days`, read as the model reads its kill-all runs
//!   (`research/equilibrium-2026-10-21/run.py`, `killall`): days to half the wiped ranges' mean
//!   over the year before the wipe, days to 90% of the control at the same census, the last
//!   year over the control's, and whether every range holds a herd.
//!
//! - **Dragons** ([`dragon_run`], slice S4): the same worlds' five lairs under the model's roamer
//!   ([`Roamer`]: it wanders the known regions and kills the dragons of the lair it stands by),
//!   read on lairs held and the herds; a kill-all at some day against the same seed's control.
//!
//! Each run is single-threaded; [`par_map`] spreads runs over threads. The always-run herd tests
//! use the 3 x 3 ranges at spawn and one seed (the dragon ones the whole grid, since no lair lies
//! within the 3 x 3, and a few years); the `#[ignore]`d ones judge seeds 1 to 30 on the whole
//! grid: run them in release, `nice -n 10 cargo test --release -p rtsim-cli sim::equilibrium --
//! --ignored --nocapture` (`RTSIM_EQ_THREADS` sets the threads, half the machine by default).

use std::ops::RangeInclusive;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rtsim_ecology::DragonId;
use rtsim_ecology::herds::{HerdId, HerdTunables};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{SlotRng, StreamKey, SystemId};
use rtsim_knowledge::ActorId;
use rtsim_travel::legs::LegIndex;

use super::census::{Ecology, RANGES_X, RANGES_Z};

/// The census grid, days (the model's).
pub const EVERY: u64 = 24;

/// A game year in days (the default calendar).
pub const YEAR: u64 = 96;

/// Which herd ranges a world holds: regions `x` by `z`.
#[derive(Clone, Debug)]
pub struct Grid {
    pub x: RangeInclusive<i32>,
    pub z: RangeInclusive<i32>,
}

impl Grid {
    /// The ranges scenario's 56 ranges.
    pub fn world() -> Grid {
        Grid {
            x: RANGES_X,
            z: RANGES_Z,
        }
    }

    /// The 3 x 3 ranges at spawn.
    pub fn spawn() -> Grid {
        Grid {
            x: -1..=1,
            z: -1..=1,
        }
    }

    pub fn build(&self, seed: u64, tun: HerdTunables) -> Ecology {
        Ecology::ranges_in(seed, tun, self.x.clone(), self.z.clone())
    }
}

/// The herds' tunables with forage `headroom` times today's: every species eats `1 / headroom`
/// of its diet, so the same forage feeds `headroom` times the animals.
pub fn with_headroom(mut tun: HerdTunables, headroom: f64) -> HerdTunables {
    for s in &mut tun.species {
        s.intake_per_day /= headroom;
    }
    tun
}

/// Every herd row's animals at each census from now to `to_day`, every [`EVERY`] days:
/// (day, the rows' counts in row order).
pub fn sample(e: &mut Ecology, to_day: u64) -> Vec<(u64, Vec<u64>)> {
    let rows = rows(e);
    let mut out = Vec::new();
    let mut day = e.h.now().0 / Tick::from_days(1).0;
    loop {
        let now = e.h.now();
        out.push((day, rows.iter().map(|&h| e.h.count(h, now)).collect()));
        if day >= to_day {
            return out;
        }
        day = (day + EVERY).min(to_day);
        e.run_to(Tick::from_days(day));
    }
}

/// The herd rows, in row order.
pub fn rows(e: &Ecology) -> Vec<HerdId> {
    e.h.herds.iter().map(|(h, _)| h).collect()
}

/// Mean herd N/K over a window of a ranges world (nobody watching), and its ranges alive at the
/// end.
#[derive(Clone, Copy, Debug)]
pub struct Settled {
    pub fill: f64,
    pub alive: usize,
    pub ranges: usize,
}

/// Runs a ranges world of `seed` to `to` days and averages its herds' N/K from `from`.
pub fn settle(grid: &Grid, seed: u64, tun: HerdTunables, from: u64, to: u64) -> Settled {
    let mut e = grid.build(seed, tun);
    let k: u64 = e.h.herds.iter().map(|(_, x)| x.k).sum();
    e.run_to(Tick::from_days(from));
    let s = sample(&mut e, to);
    let fill = s
        .iter()
        .map(|(_, n)| n.iter().sum::<u64>() as f64)
        .sum::<f64>()
        / s.len() as f64
        / k as f64;
    let last = &s.last().expect("a census").1;
    check(&e);
    Settled {
        fill,
        alive: last.iter().filter(|&&n| n > 0).count(),
        ranges: last.len(),
    }
}

/// Which ranges a wipe kills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wipe {
    /// Every herd in the world.
    World,
    /// The 3 x 3 regions at spawn (the model's local wipe).
    Local,
    /// The region at spawn alone.
    Centre,
}

impl Wipe {
    fn takes(self, range: (u16, i32, i32)) -> bool {
        match self {
            Wipe::World => true,
            Wipe::Local => range.1.abs() <= 1 && range.2.abs() <= 1,
            Wipe::Centre => range.1 == 0 && range.2 == 0,
        }
    }
}

/// A wipe against its control, read on the wiped ranges' animals.
#[derive(Clone, Debug)]
pub struct Refill {
    pub seed: u64,
    /// The wiped ranges' mean animals over the year before the wipe.
    pub pre: f64,
    pub killed: u64,
    /// Days from the wipe to the first census at half of `pre`.
    pub half: Option<u64>,
    /// Days from the wipe to the first census at 90% of the control's at the same census.
    pub ninety_ctrl: Option<u64>,
    /// The last year's mean over the control's.
    pub end_over_ctrl: f64,
    /// Days from the wipe to the first census with every wiped range holding a herd.
    pub all_alive: Option<u64>,
    /// Whether every range of the world holds a herd at the end, wiped and control.
    pub alive_end: bool,
    pub alive_end_ctrl: bool,
    /// Frontier foundings and ranges dispersers founded again after the wipe, in the wiped
    /// world and in the control.
    pub frontier: u64,
    pub recolonised: u64,
    pub frontier_ctrl: u64,
    pub recolonised_ctrl: u64,
}

/// Runs a ranges world of `seed` twice to `settle_days + after_days`: a control, and one where
/// `which` is wiped at `settle_days`.
pub fn wipe_run(
    grid: &Grid,
    seed: u64,
    tun: HerdTunables,
    which: Wipe,
    settle_days: u64,
    after_days: u64,
) -> Refill {
    let end = settle_days + after_days;
    let pre_from = settle_days.saturating_sub(YEAR);
    let mut c = grid.build(seed, tun.clone());
    let mut w = grid.build(seed, tun);
    let all = rows(&w);
    let idx: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, h)| which.takes(w.h.herd(**h).expect("a row").range))
        .map(|(i, _)| i)
        .collect();
    let pick: Vec<HerdId> = idx.iter().map(|&i| all[i]).collect();
    let sum = |n: &[u64]| idx.iter().map(|&i| n[i]).sum::<u64>();
    w.run_to(Tick::from_days(pre_from));
    let before: Vec<f64> = sample(&mut w, settle_days)
        .iter()
        .filter(|(d, _)| *d < settle_days)
        .map(|(_, n)| sum(n) as f64)
        .collect();
    let pre = before.iter().sum::<f64>() / before.len().max(1) as f64;
    let (f0, r0) = (w.h.stats.frontier_foundings, w.h.stats.recolonised);
    let at = w.h.now();
    let killed = pick.iter().map(|&h| w.h.cull(h, at)).sum();
    let after = sample(&mut w, end);
    c.run_to(Tick::from_days(settle_days));
    let (fc, rc) = (c.h.stats.frontier_foundings, c.h.stats.recolonised);
    let ctrl = sample(&mut c, end);
    check(&w);
    check(&c);
    let alive = |s: &[(u64, Vec<u64>)]| s.last().expect("a census").1.iter().all(|&n| n > 0);
    let mut r = Refill {
        seed,
        pre,
        killed,
        half: None,
        ninety_ctrl: None,
        end_over_ctrl: 0.0,
        all_alive: None,
        alive_end: alive(&after),
        alive_end_ctrl: alive(&ctrl),
        frontier: w.h.stats.frontier_foundings - f0,
        recolonised: w.h.stats.recolonised - r0,
        frontier_ctrl: c.h.stats.frontier_foundings - fc,
        recolonised_ctrl: c.h.stats.recolonised - rc,
    };
    let (mut tail, mut tail_c) = (0.0, 0.0);
    for ((day, n), (dc, nc)) in after.iter().zip(&ctrl) {
        assert_eq!(day, dc);
        let since = day - settle_days;
        if since == 0 {
            continue;
        }
        let (x, xc) = (sum(n) as f64, sum(nc) as f64);
        if r.half.is_none() && x >= 0.5 * pre {
            r.half = Some(since);
        }
        if r.ninety_ctrl.is_none() && x >= 0.9 * xc {
            r.ninety_ctrl = Some(since);
        }
        if r.all_alive.is_none() && idx.iter().all(|&i| n[i] > 0) {
            r.all_alive = Some(since);
        }
        if *day + YEAR > end {
            tail += x;
            tail_c += xc;
        }
    }
    r.end_over_ctrl = tail / tail_c.max(1.0);
    r
}

/// The herds' invariants (conservation, the ledgers, the indexes).
fn check(e: &Ecology) {
    let now = e.h.now();
    if let Err(why) = e.h.check(now) {
        panic!("herds: {why}");
    }
}

/// `f` over `items` on up to `threads` threads, results in order.
pub fn par_map<T: Sync, R: Send>(
    items: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, items.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(x) = items.get(i) else {
                        break;
                    };
                    let r = f(x);
                    out.lock().expect("no panic holding it")[i] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .expect("no panic holding it")
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// Threads for the long checks: `RTSIM_EQ_THREADS`, else half the machine.
pub fn threads() -> usize {
    std::env::var("RTSIM_EQ_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()) / 2)
        .max(1)
}

/// Median and the 5th and 95th percentiles (nearest rank) of the finite `xs`.
pub fn band(xs: &[f64]) -> (f64, f64, f64) {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((q * (v.len() - 1) as f64).round() as usize).min(v.len() - 1)];
    (at(0.5), at(0.05), at(0.95))
}

/// Days as f64, never (`None`) as infinity.
fn days(x: Option<u64>) -> f64 {
    x.map_or(f64::INFINITY, |d| d as f64)
}

/// The roamer's player.
const ROAMER: (u64, u64) = (0xE0, 0x5A3);

/// The model's roamer (`research/equilibrium-2026-10-21/model.py`, `Player`, kind `roamer`), on the
/// dragons alone (the other kinds' slices bring theirs): it starts in the region at spawn, and on
/// every second day steps to one of the nine regions around it (a step past the known radius,
/// `min(6, 2 + day / 48)` regions of spawn, is refused); each day, with chance one half, it kills
/// every dragon of the lairs in its region. Each kill is its player's (the lair's pressure rises).
pub struct Roamer {
    rng: SlotRng,
    /// The region it stands in.
    pub at: (i64, i64),
    pub player: ActorId,
    /// Whether it still kills.
    pub kills: bool,
    pub killed: u64,
}

impl Roamer {
    pub fn new(e: &mut Ecology, seed: u64) -> Roamer {
        Roamer {
            rng: StreamKey::new(seed, SystemId::named("eq.roamer"), 0).slot(0),
            at: (0, 0),
            player: e.d.player(&mut e.w, ROAMER),
            kills: true,
            killed: 0,
        }
    }

    /// Day `day`'s turn, after the world has run to it.
    pub fn day(&mut self, e: &mut Ecology, day: u64) {
        if day.is_multiple_of(2) {
            let r = (2 + day / 48).min(6) as i64;
            let dx = self.rng.below(3) as i64 - 1;
            let dz = self.rng.below(3) as i64 - 1;
            let next = (self.at.0 + dx, self.at.1 + dz);
            if next.0.abs() <= r && next.1.abs() <= r {
                self.at = next;
            }
        }
        if !self.rng.bernoulli(0.5) || !self.kills {
            return;
        }
        let here: Vec<DragonId> =
            e.d.lairs
                .iter()
                .filter(|l| LegIndex::region_of(i64::from(l.pos.x), i64::from(l.pos.z)) == self.at)
                .flat_map(|l| l.occupants.iter().copied())
                .collect();
        let at = Tick::from_days(day);
        for d in here {
            e.d.slay(&mut e.w, d, Some(self.player), at);
            self.killed += 1;
        }
        e.pump(at);
    }
}

/// What a dragon run reads at a census.
#[derive(Clone, Copy, Debug)]
pub struct DragonCensus {
    pub day: u64,
    /// Lairs with a dragon at home, of `lairs`.
    pub held: usize,
    pub lairs: usize,
    /// The herds' animals, their K, the ranges holding a herd and the ranges.
    pub animals: u64,
    pub k: u64,
    pub alive: usize,
    pub ranges: usize,
}

impl DragonCensus {
    pub fn held_share(&self) -> f64 {
        self.held as f64 / self.lairs.max(1) as f64
    }
}

/// A shock to a dragon run at a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shock {
    None,
    /// Every dragon in the world killed by the roamer's player at the day, which then stops
    /// killing (the model's `killall`: a roamer, then a peaceful player).
    KillAll(u64),
    /// The same seed's control: the roamer stops killing at the day, nothing is killed.
    Control(u64),
}

/// One dragon run.
#[derive(Clone, Debug)]
pub struct DragonRun {
    pub seed: u64,
    pub censuses: Vec<DragonCensus>,
    /// The roamer's kills and the shock's.
    pub roamer_kills: u64,
    pub shock_kills: u64,
    pub stats: rtsim_ecology::DragonStats,
}

/// A ranges world of `seed` run to `to` days, with the roamer if `roamer` (else nobody), a census
/// every [`EVERY`] days and `shock`.
pub fn dragon_run(grid: &Grid, seed: u64, roamer: bool, shock: Shock, to: u64) -> DragonRun {
    let mut e = grid.build(seed, Ecology::herd_tunables());
    let k: u64 = e.h.herds.iter().map(|(_, x)| x.k).sum();
    let rows = rows(&e);
    let mut ro = roamer.then(|| Roamer::new(&mut e, seed));
    let mut out = DragonRun {
        seed,
        censuses: Vec::new(),
        roamer_kills: 0,
        shock_kills: 0,
        stats: Default::default(),
    };
    for day in 0..=to {
        e.run_to(Tick::from_days(day));
        let at = Tick::from_days(day);
        match shock {
            Shock::KillAll(d) if d == day => {
                if let Some(r) = &mut ro {
                    r.kills = false;
                }
                let player = e.d.player(&mut e.w, ROAMER);
                let all: Vec<DragonId> = e.d.dragons.iter().map(|(d, _)| d).collect();
                for d in all {
                    if e.d.dragon(d).is_some_and(|x| !x.is_embodied()) {
                        e.d.slay(&mut e.w, d, Some(player), at);
                        out.shock_kills += 1;
                    }
                }
                e.pump(at);
            }
            Shock::Control(d) if d == day => {
                if let Some(r) = &mut ro {
                    r.kills = false;
                }
            }
            _ => {}
        }
        if let Some(r) = &mut ro {
            r.day(&mut e, day);
        }
        if day.is_multiple_of(EVERY) {
            let counts: Vec<u64> = rows.iter().map(|&h| e.h.count(h, at)).collect();
            out.censuses.push(DragonCensus {
                day,
                held: e.d.lairs.iter().filter(|l| !l.occupants.is_empty()).count(),
                lairs: e.d.lairs.len(),
                animals: counts.iter().sum(),
                k,
                alive: counts.iter().filter(|&&n| n > 0).count(),
                ranges: counts.len(),
            });
        }
    }
    check(&e);
    out.roamer_kills = ro.map_or(0, |r| r.killed);
    out.stats = e.d.stats;
    out
}

/// A dragon run's means over the censuses from `from` days on: lairs held, herds N/K, and the
/// fewest ranges alive over the ranges.
#[derive(Clone, Copy, Debug)]
pub struct DragonMeans {
    pub held: f64,
    pub fill: f64,
    pub alive_min: f64,
}

pub fn dragon_means(r: &DragonRun, from: u64) -> DragonMeans {
    let late: Vec<&DragonCensus> = r.censuses.iter().filter(|c| c.day >= from).collect();
    let n = late.len().max(1) as f64;
    DragonMeans {
        held: late.iter().map(|c| c.held_share()).sum::<f64>() / n,
        fill: late
            .iter()
            .map(|c| c.animals as f64 / c.k.max(1) as f64)
            .sum::<f64>()
            / n,
        alive_min: late
            .iter()
            .map(|c| c.alive as f64 / c.ranges.max(1) as f64)
            .fold(1.0, f64::min),
    }
}

/// A dragon kill-all against its control, read on lairs held as the model reads it
/// (`run.py`, `killall`).
#[derive(Clone, Debug)]
pub struct DragonRefill {
    pub seed: u64,
    pub killed: u64,
    /// Lairs held, mean over the year before the shock.
    pub pre: f64,
    /// Days from the shock to the first census at half of `pre`, and at 90% of the control's
    /// lairs held at the same census.
    pub half: Option<u64>,
    pub ninety_ctrl: Option<u64>,
    /// Lairs held over the control's, over the second half of the run after the shock.
    pub end_over_ctrl: f64,
}

/// Runs a ranges world of `seed` with the roamer to `shock_day + after_days` twice: every dragon
/// killed at `shock_day`, and the control.
pub fn dragon_kill_all(grid: &Grid, seed: u64, shock_day: u64, after_days: u64) -> DragonRefill {
    let end = shock_day + after_days;
    let w = dragon_run(grid, seed, true, Shock::KillAll(shock_day), end);
    let c = dragon_run(grid, seed, true, Shock::Control(shock_day), end);
    let before: Vec<f64> = w
        .censuses
        .iter()
        .filter(|x| x.day + YEAR >= shock_day && x.day < shock_day)
        .map(|x| x.held as f64)
        .collect();
    let pre = before.iter().sum::<f64>() / before.len().max(1) as f64;
    let mut r = DragonRefill {
        seed,
        killed: w.shock_kills,
        pre,
        half: None,
        ninety_ctrl: None,
        end_over_ctrl: 0.0,
    };
    let (mut tail, mut tail_c) = (0.0, 0.0);
    for (x, xc) in w.censuses.iter().zip(&c.censuses) {
        assert_eq!(x.day, xc.day);
        if x.day <= shock_day {
            continue;
        }
        let since = x.day - shock_day;
        let (n, nc) = (x.held as f64, xc.held as f64);
        if r.half.is_none() && n >= 0.5 * pre {
            r.half = Some(since);
        }
        if r.ninety_ctrl.is_none() && n >= 0.9 * nc {
            r.ninety_ctrl = Some(since);
        }
        if 2 * since >= after_days {
            tail += n;
            tail_c += nc;
        }
    }
    r.end_over_ctrl = tail / tail_c.max(1.0);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seeds 1 to 30 (the long checks).
    fn seeds() -> Vec<u64> {
        (1..=30).collect()
    }

    fn print_refills(what: &str, rs: &[Refill]) {
        let b = |f: &dyn Fn(&Refill) -> f64| band(&rs.iter().map(f).collect::<Vec<_>>());
        println!("{what} over {} seeds, median [5th, 95th]:", rs.len());
        println!(
            "  days to half the year before:  {:?}",
            b(&|r| days(r.half))
        );
        println!(
            "  days to 90% of the control:    {:?}",
            b(&|r| days(r.ninety_ctrl))
        );
        println!(
            "  days to every range alive:     {:?}",
            b(&|r| days(r.all_alive))
        );
        println!(
            "  last year over the control:    {:?}",
            b(&|r| r.end_over_ctrl)
        );
        println!(
            "  never half: {}, never 90% of the control: {}, a range empty at the end: {}",
            rs.iter().filter(|r| r.half.is_none()).count(),
            rs.iter().filter(|r| r.ninety_ctrl.is_none()).count(),
            rs.iter().filter(|r| !r.alive_end).count(),
        );
        for r in rs {
            println!(
                "  seed {:>2}: pre {:>8.0} killed {:>6} half {:?} 90% {:?} all alive {:?} end/ctrl {:.3} frontier {} recolonised {}",
                r.seed,
                r.pre,
                r.killed,
                r.half,
                r.ninety_ctrl,
                r.all_alive,
                r.end_over_ctrl,
                r.frontier,
                r.recolonised
            );
        }
    }

    /// With the density term a herd's level is set by its births and its predators, not by its
    /// forage: forage headroom 1 and 4 (every diet a quarter) leave the 3 x 3 ranges at spawn
    /// within 0.1 K of each other, where births without it grow far past K, further with more
    /// forage (§3.2).
    #[test]
    fn herds_settle_without_forage_tuning() {
        let jobs = [(true, 1.0), (true, 4.0), (false, 1.0), (false, 4.0)];
        let got = par_map(&jobs, jobs.len(), |&(density, x)| {
            let tun = HerdTunables {
                density_term: density,
                ..with_headroom(Ecology::herd_tunables(), x)
            };
            settle(&Grid::spawn(), 1, tun, 384, 960)
        });
        for (j, s) in jobs.iter().zip(&got) {
            assert_eq!(s.alive, s.ranges, "{j:?}: {s:?}");
        }
        let (on1, on4, off1, off4) = (got[0].fill, got[1].fill, got[2].fill, got[3].fill);
        println!(
            "N/K: density term {on1:.3} and {on4:.3}, births as before {off1:.3} and {off4:.3}"
        );
        assert!(
            (on1 - on4).abs() <= 0.1,
            "density term: {on1:.3} and {on4:.3}"
        );
        assert!(on1 < 1.0 && on4 < 1.0, "{on1:.3} {on4:.3}");
        assert!(
            off1 > 2.0 && off4 > off1,
            "births as before: {off1:.3} and {off4:.3}"
        );
    }

    /// §3.7's forage sweep on the whole grid, seeds 1 to 30: headroom 1, 1.5 and 4 give one
    /// band (medians within 0.1 K, each seed's spread within 0.15 K), every range alive.
    #[test]
    #[ignore]
    fn herds_settle_without_forage_tuning_30_seeds() {
        let xs = [1.0, 1.5, 4.0];
        let jobs: Vec<(u64, f64)> = seeds()
            .into_iter()
            .flat_map(|s| xs.map(|x| (s, x)))
            .collect();
        let got = par_map(&jobs, threads(), |&(seed, x)| {
            settle(
                &Grid::world(),
                seed,
                with_headroom(Ecology::herd_tunables(), x),
                10 * YEAR,
                20 * YEAR,
            )
        });
        let mut medians = Vec::new();
        for (i, x) in xs.iter().enumerate() {
            let fills: Vec<f64> = got
                .iter()
                .skip(i)
                .step_by(xs.len())
                .map(|s| s.fill)
                .collect();
            let b = band(&fills);
            println!("headroom {x}: N/K {b:?}");
            medians.push(b.0);
        }
        for (s, chunk) in seeds().iter().zip(got.chunks(xs.len())) {
            let f: Vec<f64> = chunk.iter().map(|c| c.fill).collect();
            println!("  seed {s:>2}: {f:.3?}");
            for c in chunk {
                assert_eq!(c.alive, c.ranges, "seed {s}: {c:?}");
            }
            let spread = f.iter().copied().fold(f64::MIN, f64::max)
                - f.iter().copied().fold(f64::MAX, f64::min);
            assert!(spread <= 0.15, "seed {s}: {f:?}");
        }
        let spread = medians.iter().copied().fold(f64::MIN, f64::max)
            - medians.iter().copied().fold(f64::MAX, f64::min);
        assert!(spread <= 0.1, "medians {medians:?}");
    }

    /// Every herd of the 3 x 3 ranges at spawn killed in their second year: the empty ranges
    /// are founded again from the frontier (booked, the control none), and no animal comes
    /// from anywhere else (the invariants, checked at the end of both runs).
    #[test]
    fn kill_all_herds() {
        let r = wipe_run(
            &Grid::spawn(),
            1,
            Ecology::herd_tunables(),
            Wipe::World,
            2 * YEAR,
            6 * YEAR,
        );
        print_refills("world wipe, 3 x 3", std::slice::from_ref(&r));
        assert!(r.killed > 0 && r.pre > 0.0, "{r:?}");
        assert!(r.frontier >= 2, "{r:?}");
        assert_eq!((r.frontier_ctrl, r.recolonised_ctrl), (0, 0), "{r:?}");
        assert!(r.alive_end_ctrl, "{r:?}");
    }

    /// EQUILIBRIUM.md §3.6's world wipe, seeds 1 to 30: every herd on the grid killed after ten
    /// years; half of the year before is back within 25 years and every range holds a herd 40
    /// years on, in every seed (the model: half in 1,368 [1,248, 1,656] days).
    #[test]
    #[ignore]
    fn kill_all_herds_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| {
            wipe_run(
                &Grid::world(),
                seed,
                Ecology::herd_tunables(),
                Wipe::World,
                10 * YEAR,
                40 * YEAR,
            )
        });
        print_refills("world wipe", &rs);
        for r in &rs {
            assert!(r.half.is_some_and(|d| d <= 25 * YEAR), "{r:?}");
            assert!(r.alive_end && r.all_alive.is_some(), "{r:?}");
        }
    }

    /// The range at spawn wiped in its second year: its neighbours' dispersers found it again
    /// within two springs.
    #[test]
    fn local_wipe_refills_from_neighbours() {
        let r = wipe_run(
            &Grid::spawn(),
            1,
            Ecology::herd_tunables(),
            Wipe::Centre,
            2 * YEAR,
            2 * YEAR,
        );
        print_refills("centre wipe, 3 x 3", std::slice::from_ref(&r));
        assert!(r.killed > 0, "{r:?}");
        assert!(r.recolonised >= 1, "{r:?}");
        assert!(r.all_alive.is_some() && r.alive_end, "{r:?}");
    }

    /// §3.6's local wipe, seeds 1 to 30: the 3 x 3 ranges at spawn wiped after ten years; half
    /// of the year before is back within 25 years, all nine hold a herd 40 years on, and
    /// neighbours' dispersers refounded some of them (the model: half in 1,128 [792, 1,608]
    /// days).
    #[test]
    #[ignore]
    fn local_wipe_refills_from_neighbours_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| {
            wipe_run(
                &Grid::world(),
                seed,
                Ecology::herd_tunables(),
                Wipe::Local,
                10 * YEAR,
                40 * YEAR,
            )
        });
        print_refills("local wipe", &rs);
        for r in &rs {
            assert!(r.half.is_some_and(|d| d <= 25 * YEAR), "{r:?}");
            assert!(r.alive_end && r.all_alive.is_some(), "{r:?}");
        }
        let recol: u64 = rs.iter().map(|r| r.recolonised).sum();
        assert!(recol > 0, "no range refilled from a neighbour");
    }

    fn print_dragon_refills(rs: &[DragonRefill]) {
        let b = |f: &dyn Fn(&DragonRefill) -> f64| band(&rs.iter().map(f).collect::<Vec<_>>());
        println!(
            "dragon kill-all over {} seeds, median [5th, 95th]:",
            rs.len()
        );
        println!(
            "  days to half the year before:  {:?}",
            b(&|r| days(r.half))
        );
        println!(
            "  days to 90% of the control:    {:?}",
            b(&|r| days(r.ninety_ctrl))
        );
        println!(
            "  end over the control:          {:?}",
            b(&|r| r.end_over_ctrl)
        );
        for r in rs {
            println!(
                "  seed {:>2}: killed {} pre {:.2} half {:?} 90% {:?} end/ctrl {:.3}",
                r.seed, r.killed, r.pre, r.half, r.ninety_ctrl, r.end_over_ctrl
            );
        }
    }

    /// Every dragon killed in the second year, then a peaceful player: the lairs wait out the
    /// pressure the kill raised (40 days) and newcomers fill them, back to 90% of the same seed's
    /// control within 72 days (§3.6's kill-all row: 48 [48, 72]).
    #[test]
    fn kill_all_dragons() {
        let r = dragon_kill_all(&Grid::world(), 1, YEAR, 2 * YEAR);
        print_dragon_refills(std::slice::from_ref(&r));
        assert!(r.killed >= 4, "{r:?}");
        assert!(r.ninety_ctrl.is_some_and(|d| d <= 72), "{r:?}");
        assert!(r.end_over_ctrl >= 0.9, "{r:?}");
    }

    /// §3.6's dragon kill-all, seeds 1 to 30: the roamer for 20 years, every dragon killed, a
    /// peaceful player for 6 more; 90% of the control's lairs held within 72 days at the median,
    /// every seed back within the rule's cap (192 days, read on the 24-day census), and the end
    /// level the control's (the model: 50% at 48, 90% at 48 [48, 72], end over the control
    /// 1.01). Five lairs make 90% of the control every lair the control holds, so one lair the
    /// roamer pressed hard before the shock (P near 2.9 after it: a wait near 144 days) holds a
    /// seed back; the model averages over many lairs (docs/overnight-choices-equilibrium.md,
    /// the S4 findings).
    #[test]
    #[ignore]
    fn kill_all_dragons_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| {
            dragon_kill_all(&Grid::world(), seed, 20 * YEAR, 6 * YEAR)
        });
        print_dragon_refills(&rs);
        let cap = 192 + EVERY;
        for r in &rs {
            assert!(r.killed > 0, "{r:?}");
            assert!(r.ninety_ctrl.is_some_and(|d| d <= cap), "{r:?}");
        }
        let ninety = band(&rs.iter().map(|r| days(r.ninety_ctrl)).collect::<Vec<_>>());
        assert!(ninety.0 <= 72.0, "{ninety:?}");
        let end = band(&rs.iter().map(|r| r.end_over_ctrl).collect::<Vec<_>>());
        assert!((0.9..=1.1).contains(&end.0), "{end:?}");
    }

    fn print_dragon_runs(what: &str, rs: &[DragonRun], from: u64) {
        let m: Vec<DragonMeans> = rs.iter().map(|r| dragon_means(r, from)).collect();
        let b = |f: &dyn Fn(&DragonMeans) -> f64| band(&m.iter().map(f).collect::<Vec<_>>());
        println!("{what} over {} seeds, median [5th, 95th]:", rs.len());
        println!("  lairs held:          {:?}", b(&|x| x.held));
        println!("  herds N/K:           {:?}", b(&|x| x.fill));
        println!("  fewest ranges alive: {:?}", b(&|x| x.alive_min));
        for (r, x) in rs.iter().zip(&m) {
            let s = &r.stats;
            println!(
                "  seed {:>2}: held {:.3} N/K {:.3} alive {:.2} roamer kills {} | losses {} starved {} gated {} newcomers {} heirs {} migrations {} age {}",
                r.seed,
                x.held,
                x.fill,
                x.alive_min,
                r.roamer_kills,
                s.player_losses,
                s.starved_away,
                s.arrivals_gated,
                s.respawns,
                s.heirs_sent,
                s.migrations,
                s.deaths_age
            );
        }
    }

    /// The roamer for four years over the whole grid: it kills dragons (each its player's loss),
    /// the lairs it empties come back, every herd range stays alive, and most lairs are held.
    #[test]
    fn long_horizon_dragons_and_herds_with_kills() {
        let r = dragon_run(&Grid::world(), 1, true, Shock::None, 4 * YEAR);
        print_dragon_runs("roamer, 4 years", std::slice::from_ref(&r), 2 * YEAR);
        let m = dragon_means(&r, 2 * YEAR);
        assert!(r.roamer_kills > 0, "{r:?}");
        assert_eq!(r.stats.player_losses, r.roamer_kills, "{:?}", r.stats);
        assert_eq!(m.alive_min, 1.0, "{m:?}");
        assert!(m.held >= 0.6, "{m:?}");
    }

    /// §3.6's steady state with the roamer, seeds 1 to 30, 60 years, read over the last 30
    /// against the same seeds with nobody: every herd range alive at every census, the lairs
    /// held near the model's (roamer 0.89, nobody 0.95), the roamer's below nobody's, and the
    /// herds where the engine's own no-player band has them (the roamer kills no herd).
    #[test]
    #[ignore]
    fn long_horizon_dragons_and_herds_with_kills_30_seeds() {
        let jobs: Vec<(u64, bool)> = seeds()
            .into_iter()
            .flat_map(|s| [(s, true), (s, false)])
            .collect();
        let got = par_map(&jobs, threads(), |&(seed, roamer)| {
            dragon_run(&Grid::world(), seed, roamer, Shock::None, 60 * YEAR)
        });
        let roam: Vec<DragonRun> = got.iter().step_by(2).cloned().collect();
        let none: Vec<DragonRun> = got.iter().skip(1).step_by(2).cloned().collect();
        let from = 30 * YEAR;
        print_dragon_runs("roamer, 60 years", &roam, from);
        print_dragon_runs("nobody, 60 years", &none, from);
        let mr: Vec<DragonMeans> = roam.iter().map(|r| dragon_means(r, from)).collect();
        let mn: Vec<DragonMeans> = none.iter().map(|r| dragon_means(r, from)).collect();
        for (r, m) in roam.iter().zip(&mr).chain(none.iter().zip(&mn)) {
            assert_eq!(m.alive_min, 1.0, "seed {}: {m:?}", r.seed);
        }
        let held_r = band(&mr.iter().map(|m| m.held).collect::<Vec<_>>());
        let held_n = band(&mn.iter().map(|m| m.held).collect::<Vec<_>>());
        assert!((0.80..=0.97).contains(&held_r.0), "roamer {held_r:?}");
        assert!(held_r.1 >= 0.7, "roamer {held_r:?}");
        assert!((0.88..=1.0).contains(&held_n.0), "nobody {held_n:?}");
        assert!(held_r.0 < held_n.0, "roamer {held_r:?} nobody {held_n:?}");
        let fill_r = band(&mr.iter().map(|m| m.fill).collect::<Vec<_>>());
        let fill_n = band(&mn.iter().map(|m| m.fill).collect::<Vec<_>>());
        assert!(
            (fill_r.0 - fill_n.0).abs() <= 0.05,
            "roamer {fill_r:?} nobody {fill_n:?}"
        );
        assert!(fill_r.1 >= 0.5 && fill_r.2 < 1.0, "roamer {fill_r:?}");
    }
}
