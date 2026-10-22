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
//! Each run is single-threaded; [`par_map`] spreads runs over threads. The always-run tests use
//! the 3 x 3 ranges at spawn and one seed; the `#[ignore]`d ones judge seeds 1 to 30 on the whole
//! grid: run them in release, `nice -n 10 cargo test --release -p rtsim-cli sim::equilibrium --
//! --ignored --nocapture` (`RTSIM_EQ_THREADS` sets the threads, half the machine by default).

use std::ops::RangeInclusive;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rtsim_ecology::herds::{HerdId, HerdTunables};
use rtsim_kernel::clock::Tick;

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
        println!("  days to half the year before:  {:?}", b(&|r| days(r.half)));
        println!("  days to 90% of the control:    {:?}", b(&|r| days(r.ninety_ctrl)));
        println!("  days to every range alive:     {:?}", b(&|r| days(r.all_alive)));
        println!("  last year over the control:    {:?}", b(&|r| r.end_over_ctrl));
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
        println!("N/K: density term {on1:.3} and {on4:.3}, births as before {off1:.3} and {off4:.3}");
        assert!((on1 - on4).abs() <= 0.1, "density term: {on1:.3} and {on4:.3}");
        assert!(on1 < 1.0 && on4 < 1.0, "{on1:.3} {on4:.3}");
        assert!(off1 > 2.0 && off4 > off1, "births as before: {off1:.3} and {off4:.3}");
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
            let fills: Vec<f64> = got.iter().skip(i).step_by(xs.len()).map(|s| s.fill).collect();
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
        let r = wipe_run(&Grid::spawn(), 1, Ecology::herd_tunables(), Wipe::World, 2 * YEAR, 6 * YEAR);
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
            wipe_run(&Grid::world(), seed, Ecology::herd_tunables(), Wipe::World, 10 * YEAR, 40 * YEAR)
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
        let r = wipe_run(&Grid::spawn(), 1, Ecology::herd_tunables(), Wipe::Centre, 2 * YEAR, 2 * YEAR);
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
            wipe_run(&Grid::world(), seed, Ecology::herd_tunables(), Wipe::Local, 10 * YEAR, 40 * YEAR)
        });
        print_refills("local wipe", &rs);
        for r in &rs {
            assert!(r.half.is_some_and(|d| d <= 25 * YEAR), "{r:?}");
            assert!(r.alive_end && r.all_alive.is_some(), "{r:?}");
        }
        let recol: u64 = rs.iter().map(|r| r.recolonised).sum();
        assert!(recol > 0, "no range refilled from a neighbour");
    }
}
