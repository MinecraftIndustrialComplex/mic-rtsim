// SPDX-License-Identifier: AGPL-3.0-or-later
//! EQUILIBRIUM.md's long-horizon checks on the synthetic worlds (§3.6, §3.7), run as tests:
//! slice S2's herds on the census's `ranges` scenario ([`Ecology::ranges`]: 56 herd ranges under
//! the phase 1 dragon world's lairs), with nobody watching.
//!
//! - **Settling** ([`settle`]): a world's herds over their K, averaged over a late window.
//! - **Wipes** ([`wipe_run`]): the same seed run twice, a control and a world where every herd of
//!   some ranges is killed at `settle_days`; read as the model reads its kill-all runs
//!   (`research/equilibrium-2026-10-21/run.py`, `killall`): days to half the year before the wipe,
//!   days to 90% of the control at the same census, the last year over the control's, and
//!   whether every range holds a herd.
//!
//! Each run is single-threaded; [`par_map`] spreads seeds over threads. The long versions are
//! `#[ignore]`d: run them in release, `nice -n 10 cargo test --release -p rtsim-cli
//! equilibrium -- --ignored --nocapture`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use rtsim_ecology::herds::{HerdId, HerdTunables};
use rtsim_kernel::clock::Tick;

use super::census::Ecology;

/// The census grid, days (the model's).
pub const EVERY: u64 = 24;

/// A game year in days (the default calendar).
pub const YEAR: u64 = 96;

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

/// The rows whose range is one of the 3 x 3 regions at spawn (the model's local wipe).
pub fn local_rows(e: &Ecology) -> Vec<HerdId> {
    e.h.herds
        .iter()
        .filter(|(_, x)| x.range.1.abs() <= 1 && x.range.2.abs() <= 1)
        .map(|(h, _)| h)
        .collect()
}

/// Kills every latent member of the rows `which` now; the animals killed.
pub fn wipe(e: &mut Ecology, which: &[HerdId]) -> u64 {
    let at = e.h.now();
    which.iter().map(|&h| e.h.cull(h, at)).sum()
}

/// Mean herd N/K over days `from` to `to` of a ranges world (nobody watching), and its ranges
/// alive at the end.
#[derive(Clone, Copy, Debug)]
pub struct Settled {
    pub fill: f64,
    pub alive: usize,
    pub ranges: usize,
}

/// Runs a ranges world of `seed` to `to` days and averages its herds' N/K from `from`.
pub fn settle(seed: u64, tun: HerdTunables, from: u64, to: u64) -> Settled {
    let mut e = Ecology::ranges(seed, tun);
    let k: u64 = e.h.herds.iter().map(|(_, x)| x.k).sum();
    e.run_to(Tick::from_days(from));
    let s = sample(&mut e, to);
    let fill = s.iter().map(|(_, n)| n.iter().sum::<u64>() as f64).sum::<f64>()
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
    /// The 3 x 3 regions at spawn.
    Local,
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
    /// Whether every range of the world holds a herd at the end.
    pub alive_end: bool,
    /// Frontier foundings and ranges dispersers founded again, after the wipe.
    pub frontier: u64,
    pub recolonised: u64,
}

/// Runs a ranges world of `seed` twice to `settle_days + after_days`: a control, and one where
/// `which` is wiped at `settle_days`.
pub fn wipe_run(
    seed: u64,
    tun: HerdTunables,
    which: Wipe,
    settle_days: u64,
    after_days: u64,
) -> Refill {
    let end = settle_days + after_days;
    let pre_from = settle_days.saturating_sub(YEAR);
    let mut c = Ecology::ranges(seed, tun.clone());
    let mut w = Ecology::ranges(seed, tun);
    let pick = match which {
        Wipe::World => rows(&w),
        Wipe::Local => local_rows(&w),
    };
    let all = rows(&w);
    let idx: Vec<usize> = pick
        .iter()
        .map(|h| all.iter().position(|g| g == h).expect("a row"))
        .collect();
    let sum = |n: &[u64]| idx.iter().map(|&i| n[i]).sum::<u64>();
    w.run_to(Tick::from_days(pre_from));
    let before = sample(&mut w, settle_days);
    let pre = before
        .iter()
        .filter(|(d, _)| *d < settle_days)
        .map(|(_, n)| sum(n) as f64)
        .sum::<f64>()
        / before.iter().filter(|(d, _)| *d < settle_days).count().max(1) as f64;
    let (f0, r0) = (w.h.stats.frontier_foundings, w.h.stats.recolonised);
    let killed = wipe(&mut w, &pick);
    let after = sample(&mut w, end);
    c.run_to(Tick::from_days(settle_days));
    let ctrl = sample(&mut c, end);
    check(&w);
    check(&c);
    let mut r = Refill {
        seed,
        pre,
        killed,
        half: None,
        ninety_ctrl: None,
        end_over_ctrl: 0.0,
        all_alive: None,
        alive_end: after.last().expect("a census").1.iter().all(|&n| n > 0),
        frontier: w.h.stats.frontier_foundings - f0,
        recolonised: w.h.stats.recolonised - r0,
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

/// The herds' and dragons' own invariants.
fn check(e: &Ecology) {
    let now = e.h.now();
    if let Err(why) = e.h.check(now) {
        panic!("herds: {why}");
    }
}

/// `f` over `items` on up to `threads` threads, results in order.
pub fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
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
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, |n| n.get()) / 2
        })
        .max(1)
}

/// Median and the 5th and 95th percentiles (nearest rank) of `xs`.
pub fn band(xs: &[f64]) -> (f64, f64, f64) {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((q * (v.len() - 1) as f64).round() as usize).min(v.len() - 1)];
    (at(0.5), at(0.05), at(0.95))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forage headroom against herd level, density term on and off (a calibration print).
    #[test]
    #[ignore]
    fn headroom_probe() {
        let jobs: Vec<(bool, f64)> = [true, false]
            .into_iter()
            .flat_map(|d| [0.25, 0.5, 1.0, 1.5, 4.0].map(|x| (d, x)))
            .collect();
        let got = par_map(&jobs, threads(), |&(d, x)| {
            let tun = HerdTunables {
                density_term: d,
                ..with_headroom(Ecology::herd_tunables(), x)
            };
            settle(7, tun, 960, 1920)
        });
        for ((d, x), s) in jobs.iter().zip(got) {
            println!("density {d} headroom {x}: {s:?}");
        }
    }
}
