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
//! - **Dens and groves** ([`mowzie_run`], slice S9): one Frostmaw den or Umvuthana grove played
//!   against the emulated plugin by a scripted player ([`MowziePresser`]: none, a kill-all, a
//!   farmer, a farmer of each arrival, a visitor), read on the members it holds, its arrivals and
//!   the kernel's own rule replayed over the same kills ([`replay_arrivals`]).
//!
//! Each run is single-threaded; [`par_map`] spreads runs over threads. The always-run herd tests
//! use the 3 x 3 ranges at spawn and one seed (the dragon ones the whole grid, since no lair lies
//! within the 3 x 3, and a few years); the `#[ignore]`d ones judge seeds 1 to 30 on the whole
//! grid: run them in release, `nice -n 10 cargo test --release -p rtsim-cli sim::equilibrium --
//! --ignored --nocapture` (`RTSIM_EQ_THREADS` sets the threads, half the machine by default).

use std::collections::BTreeMap;
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

// ---- villages (slice S6, EQC150 to EQC183) --------------------------------------------------------

/// One run of the synthetic world's villages of a culture ([`Villages`]: the bridge faked at the
/// world level, nobody holding any village, so after the setup census the engine hears nothing and
/// its growth steps run).
pub struct VillageRun {
    /// Real residents over the villages of the culture, at the end of each day from day 0.
    pub total: Vec<u32>,
    /// Residents each wiped village had at the wipe, by node.
    pub killed: BTreeMap<usize, u32>,
    /// Days after the wipe each wiped village was first ordered settlers, by node.
    pub first_pair: BTreeMap<usize, f64>,
    /// Settlers, growth steps and refills the world booked over the run.
    pub settlers: u64,
    pub grown: u64,
    pub refills: u64,
}

/// A run of `seed` to day `to`: with `wipe` the culture's villages are all killed by a player on
/// that day, and the engine's census reads each one empty.
pub fn village_run(
    seed: u64,
    culture: rtsim_settle::Culture,
    wipe: Option<u64>,
    to: u64,
    tweak: impl Fn(&mut rtsim_world::WorldConfig),
) -> VillageRun {
    use super::census::Villages;
    let mut v = Villages::with(seed, tweak);
    let ours: Vec<usize> = (0..v.w.nodes.len()).filter(|&i| v.is(i, culture)).collect();
    let sum = |v: &Villages| ours.iter().map(|i| v.real[i]).sum::<u32>();
    let mut out = VillageRun {
        total: vec![sum(&v); 2],
        killed: BTreeMap::new(),
        first_pair: BTreeMap::new(),
        settlers: 0,
        grown: 0,
        refills: 0,
    };
    for day in 2..=to {
        v.run_to(Tick::from_days(day));
        if wipe == Some(day) {
            for &i in &ours {
                out.killed.insert(i, v.real[&i]);
                v.wipe(i);
            }
        }
        out.total.push(sum(&v));
    }
    out.first_pair = ours
        .iter()
        .filter_map(|i| v.first_pair.get(i).map(|&d| (*i, d as f64)))
        .collect();
    out.settlers = v.w.stats.settlers.iter().sum();
    out.grown = v.w.stats.grown;
    out.refills = v.w.stats.refills;
    v.w.check_conserved(v.w.now())
        .expect("the world is conserved");
    out
}

/// A village wipe against its control, read on the culture's residents.
#[derive(Clone, Debug)]
pub struct VillageRefill {
    pub seed: u64,
    /// Residents of the culture's villages at the wipe.
    pub pre: u32,
    /// Days from the wipe to the first day the wiped world holds 90% of the control's residents.
    pub ninety_ctrl: Option<u64>,
    /// Days from the wipe to the first settlers' order, over the wiped villages: (first, last).
    pub pair: (f64, f64),
    pub wiped_villages: usize,
    /// Wiped villages that were ordered settlers.
    pub paired: usize,
    pub settlers: u64,
    pub grown: u64,
    pub refills: u64,
    /// The wiped world's residents over the control's on the last day.
    pub end_over_ctrl: f64,
}

/// The same seed twice to `wipe_day + after_days`: a control, and a world where every village of
/// the culture is killed by a player on `wipe_day`.
pub fn village_wipe(
    seed: u64,
    culture: rtsim_settle::Culture,
    wipe_day: u64,
    after_days: u64,
    tweak: impl Fn(&mut rtsim_world::WorldConfig),
) -> VillageRefill {
    let to = wipe_day + after_days;
    let ctrl = village_run(seed, culture, None, to, &tweak);
    let run = village_run(seed, culture, Some(wipe_day), to, &tweak);
    let at = |r: &VillageRun, d: u64| f64::from(r.total[d as usize]);
    let ninety_ctrl = (wipe_day + 1..=to)
        .find(|&d| at(&run, d) >= 0.9 * at(&ctrl, d))
        .map(|d| d - wipe_day);
    let firsts: Vec<f64> = run.first_pair.values().copied().collect();
    VillageRefill {
        seed,
        pre: run.killed.values().sum(),
        ninety_ctrl,
        pair: (
            firsts.iter().copied().fold(f64::INFINITY, f64::min),
            firsts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ),
        wiped_villages: run.killed.len(),
        paired: firsts.len(),
        settlers: run.settlers - ctrl.settlers,
        grown: run.grown - ctrl.grown,
        refills: run.refills - ctrl.refills,
        end_over_ctrl: at(&run, to) / at(&ctrl, to).max(1.0),
    }
}

// ---- pool camps (slice S7, EQC230 to EQC233) ------------------------------------------------------

/// The built-in cultures with every one edited (the S7 checks' tweaks: the pool rule removed, a
/// shorter half-life).
pub fn camp_cultures(edit: impl Fn(&mut rtsim_conflict::CultureDef)) -> rtsim_conflict::Cultures {
    use rtsim_conflict::culture::{
        BANDIT_JSON, FROSTMAW_JSON, GOBLIN_JSON, RAIDER_JSON, UMVUTHANA_JSON,
    };
    let docs: Vec<String> = [
        GOBLIN_JSON,
        BANDIT_JSON,
        RAIDER_JSON,
        FROSTMAW_JSON,
        UMVUTHANA_JSON,
    ]
    .into_iter()
    .map(|j| {
        let mut d: rtsim_conflict::CultureDef = serde_json::from_str(j).expect("a culture");
        edit(&mut d);
        serde_json::to_string(&d).expect("a culture")
    })
    .collect();
    let refs: Vec<&str> = docs.iter().map(String::as_str).collect();
    rtsim_conflict::Cultures::from_json(&refs).expect("the edited cultures")
}

/// Who presses the camps of a [`camp_run`].
#[derive(Clone, Copy, Debug)]
pub enum Presser {
    /// Nobody.
    None,
    /// Every warrior of every camp, home and away, killed on this day.
    KillAll(u64),
    /// Every whole warrior at home of every camp killed every `every` days (the model's farmer).
    Farmer { every: u64 },
    /// A wanderer: every second day, with chance `chance`, it kills every warrior at home of one
    /// camp at random.
    Visitor { chance: f64 },
}

/// One camp's break, as the run saw it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CampBreak {
    pub day: f64,
    /// Pressure at the break.
    pub p: f64,
    /// Days of the wait the camp was given.
    pub wait: f64,
    /// Warriors home at the refounding, if it was refounded in the run.
    pub refound_pool: Option<f64>,
}

/// A world of the three pool camps under a [`Presser`], day by day.
pub struct CampRun {
    pub culture: Vec<String>,
    /// Warriors the camps hold, home and away, at the end of each day from day 0.
    pub total: Vec<u64>,
    /// Camp-days spent broken, over all camps.
    pub broken_days: u64,
    pub breaks: Vec<Vec<CampBreak>>,
    pub max_p: Vec<f64>,
    pub killed: u64,
    pub stats_breaks: u64,
    pub stats_refounds: u64,
    pub days: u64,
}

/// The camps' warriors, home and away.
fn warriors(c: &rtsim_conflict::Conflict, t: Tick) -> u64 {
    (0..c.factions.len() as u32)
        .map(|f| c.pool_at(f, t).floor() as u64 + u64::from(c.factions[f as usize].away))
        .sum()
}

/// A run of the three camps ([`rtsim_conflict::synth::camps`]) of `seed` to day `to`.
pub fn camp_run(
    seed: u64,
    cultures: rtsim_conflict::Cultures,
    names: &[&str],
    presser: Presser,
    to: u64,
) -> CampRun {
    use rtsim_ecology::infra::NoInfrastructure;
    let (mut w, mut c) = rtsim_conflict::synth::camps(6, names, seed, cultures);
    let n = c.factions.len() as u32;
    let mut rng = StreamKey::new(seed, SystemId::named("eq.camp_visitor"), 0).slot(0);
    let mut out = CampRun {
        culture: names.iter().map(|s| s.to_string()).collect(),
        total: vec![warriors(&c, Tick::ZERO)],
        broken_days: 0,
        breaks: vec![Vec::new(); n as usize],
        max_p: vec![0.0; n as usize],
        killed: 0,
        stats_breaks: 0,
        stats_refounds: 0,
        days: to,
    };
    let mut was = vec![false; n as usize];
    // The breaks and refoundings since the last look: a refounded camp's pool is read before the
    // day's presser acts, a break after it.
    let look = |c: &rtsim_conflict::Conflict, out: &mut CampRun, was: &mut [bool], t: Tick| {
        for f in 0..n {
            let i = f as usize;
            let b = c.factions[i].strain.as_ref().and_then(|s| s.broken);
            if let Some(b) = b
                && !was[i]
            {
                out.breaks[i].push(CampBreak {
                    day: b.since.0 as f64 / TICKS_PER_DAY_F,
                    p: f64::from(c.pressure_now(f, b.since)),
                    wait: (b.due.0 - b.since.0) as f64 / TICKS_PER_DAY_F,
                    refound_pool: None,
                });
            }
            if b.is_none()
                && was[i]
                && let Some(last) = out.breaks[i].last_mut()
            {
                // The band's members, home and away: a plunder (a raider's draw can fall within
                // the day of the refounding) may have taken some out of the pool already.
                last.refound_pool = Some(c.pool_at(f, t).floor() + f64::from(c.factions[i].away));
            }
            was[i] = b.is_some();
        }
    };
    for day in 1..=to {
        let t = Tick::from_days(day);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        look(&c, &mut out, &mut was, t);
        match presser {
            Presser::None => {}
            Presser::KillAll(d) if d == day => {
                for f in 0..n {
                    out.killed += u64::from(c.slay(&mut w, f, u32::MAX, t));
                }
            }
            Presser::KillAll(_) => {}
            Presser::Farmer { every } if day % every == 0 => {
                for f in 0..n {
                    let home = c.pool_at(f, t).floor() as u32;
                    out.killed += u64::from(c.slay(&mut w, f, home, t));
                }
            }
            Presser::Farmer { .. } => {}
            Presser::Visitor { chance } if day % 2 == 0 => {
                if rng.bernoulli(chance) {
                    let f = (rng.next_u64() % u64::from(n)) as u32;
                    let home = c.pool_at(f, t).floor() as u32;
                    out.killed += u64::from(c.slay(&mut w, f, home, t));
                }
            }
            Presser::Visitor { .. } => {}
        }
        look(&c, &mut out, &mut was, t);
        for f in 0..n {
            out.broken_days += u64::from(!c.camp_active(f));
            let m = &mut out.max_p[f as usize];
            *m = m.max(f64::from(c.pressure_now(f, t)));
        }
        out.total.push(warriors(&c, t));
    }
    out.stats_breaks = c.stats.breaks;
    out.stats_refounds = c.stats.refounds;
    c.check(&w).expect("the books close");
    out
}

const TICKS_PER_DAY_F: f64 = rtsim_kernel::clock::TICKS_PER_DAY as f64;

/// A kill-all against its control (the same seed, nobody pressing), read on the camps' warriors.
#[derive(Clone, Debug)]
pub struct CampRefill {
    pub seed: u64,
    /// Warriors killed on the day.
    pub killed: u64,
    /// Days from the kill to the first day the killed world holds 90% of the control's warriors.
    pub ninety_ctrl: Option<u64>,
    /// Days to half the control's.
    pub half_ctrl: Option<u64>,
    pub breaks: u64,
    pub end_over_ctrl: f64,
}

/// The pool camps' cultures, one camp each (S7's camps world).
pub const POOL_CAMPS: [&str; 3] = ["goblin", "bandit", "raider"];

/// The same seed twice to `kill_day + after_days`: a control, and a world where every warrior of
/// every camp is killed by a player on `kill_day`.
pub fn camp_wipe(seed: u64, kill_day: u64, after_days: u64) -> CampRefill {
    camp_wipe_of(seed, &POOL_CAMPS, kill_day, after_days)
}

/// [`camp_wipe`] on a world of the camps `names` (cultures) alone: `["raider"]` is the outposts'
/// row of the kill-all table (slice S11 EQC428).
pub fn camp_wipe_of(seed: u64, names: &[&str], kill_day: u64, after_days: u64) -> CampRefill {
    let to = kill_day + after_days;
    let cultures = || rtsim_conflict::Cultures::builtin();
    let ctrl = camp_run(seed, cultures(), names, Presser::None, to);
    let run = camp_run(seed, cultures(), names, Presser::KillAll(kill_day), to);
    let at = |r: &CampRun, d: u64| r.total[d as usize] as f64;
    let first = |frac: f64| {
        (kill_day + 1..=to)
            .find(|&d| at(&run, d) >= frac * at(&ctrl, d))
            .map(|d| d - kill_day)
    };
    CampRefill {
        seed,
        killed: run.killed,
        ninety_ctrl: first(0.9),
        half_ctrl: first(0.5),
        breaks: run.stats_breaks,
        end_over_ctrl: at(&run, to) / at(&ctrl, to).max(1.0),
    }
}

// ---- Frostmaw dens and Umvuthana groves (slice S9, MZB-1262) ---------------------------------------

/// Who presses a [`mowzie_run`]'s camp: a scripted player who comes to the camp (it is loaded and
/// counted, its owed members spawned), kills every member at home, and leaves (the camp unloads).
#[derive(Clone, Copy, Debug)]
pub enum MowziePresser {
    /// Nobody.
    None,
    /// One visit on this day.
    KillAll(u64),
    /// A visit every `every` days.
    Farmer { every: u64 },
    /// A visit `lag` days after each day an arrival was seen, to kill what came (the den's farmer:
    /// the Frostmaw dies two days after it is back).
    AfterArrival { lag: u64 },
    /// A visitor: every second day, with chance `chance`, a visit.
    Visitor { chance: f64 },
}

/// One den's or grove's run: the members it holds each day, the kills that were made and the
/// arrivals the rule brought.
#[derive(Clone, Debug)]
pub struct MowzieRun {
    /// K, the camp's cap.
    pub k: u32,
    /// Members the camp holds, home and away, at the end of each day from day 0.
    pub members: Vec<u32>,
    pub arrivals: u64,
    /// The arrivals by the run's middle day.
    pub arrivals_mid: u64,
    /// The visits that killed: (tick, members killed).
    pub kills: Vec<(u64, u32)>,
    /// The size of the engine's save a quarter of the way through and at the end, bytes.
    pub save_bytes: (usize, usize),
    pub days: u64,
    /// The plunders the camp drew a wait for by the end (the faction's own draw count: one a plunder
    /// scheduled, so the last may still be on its way), S9 FB-S9-2.
    pub plunders: u32,
    /// The most grief (a share of the cap lost in war parties lately) the camp's plunder mood held
    /// at the end of any day; 0 for a camp with no mood.
    pub grief_peak: f32,
    /// The size of the engine's save at the end of each game year of an [`unattended_grove`] run
    /// (empty for the other runs), bytes.
    pub save_yearly: Vec<usize>,
}

impl MowzieRun {
    /// Days between two plunders, the run's days over the plunders drawn.
    pub fn plunder_days(&self) -> f64 {
        self.days as f64 / f64::from(self.plunders.max(1))
    }

    /// The mean members over K across days `from..=to`.
    pub fn held(&self, from: u64, to: u64) -> f64 {
        let xs = &self.members[from as usize..=to as usize];
        xs.iter().map(|&m| f64::from(m)).sum::<f64>() / xs.len() as f64 / f64::from(self.k.max(1))
    }

    /// The first day at or after `from` on which the camp holds all K members again.
    pub fn whole_from(&self, from: u64) -> Option<u64> {
        (from..=self.days).find(|&d| self.members[d as usize] >= self.k)
    }
}

/// A visit: the player comes (the camp is loaded and counted), kills every member at home and
/// leaves. Returns how many he killed.
fn mowzie_visit(r: &mut rtsim_conflict::synth::CampRun) -> u32 {
    use rtsim_conflict::boundary;
    use rtsim_conflict::engine::CHIEF_SLOT;
    r.camp_in();
    let t = r.c.now() + 10;
    r.run(t);
    let ids: Vec<_> = r.plugin.camps[&r.key]
        .bodies
        .iter()
        .filter(|b| b.generation != 0 && b.slot != CHIEF_SLOT)
        .map(|b| b.uuid)
        .collect();
    let at = r.c.now() + 1;
    let mut killed = 0;
    for u in ids {
        killed += u32::from(r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at));
    }
    r.run(at + 5);
    r.camp_out();
    let t = r.c.now() + 3;
    r.run(t);
    killed
}

/// A camp of `culture` found with `found` bodies on `cultures`, run to day `to` against its
/// scripted player.
pub fn mowzie_run(
    seed: u64,
    cultures: rtsim_conflict::Cultures,
    culture: &'static str,
    found: u16,
    presser: MowziePresser,
    to: u64,
) -> MowzieRun {
    use rtsim_conflict::synth::CampRun;
    let mut r = CampRun::with_cultures(seed, cultures, culture, found);
    r.audits = false;
    r.round_trips = false;
    let members = |r: &CampRun| {
        r.c.pool_at(r.f, r.c.now()).max(0.0).floor() as u32 + r.c.factions[r.f as usize].away
    };
    let mut rng = StreamKey::new(seed, SystemId::named("eq.mowzie_visitor"), 0).slot(0);
    let mut out = MowzieRun {
        k: r.c.factions[r.f as usize].cap,
        members: vec![members(&r)],
        arrivals: 0,
        arrivals_mid: 0,
        kills: Vec::new(),
        save_bytes: (0, 0),
        days: to,
        plunders: 0,
        grief_peak: 0.0,
        save_yearly: Vec::new(),
    };
    // The den's farmer comes first `lag` days in, to the Frostmaw that stands there.
    let (mut seen, mut due_visit): (u64, Option<u64>) = (
        0,
        match presser {
            MowziePresser::AfterArrival { lag } => Some(lag),
            _ => None,
        },
    );
    for day in 1..=to {
        let t = Tick::from_days(day);
        if r.c.now() < t {
            r.run(t);
        }
        if r.c.stats.arrivals > seen {
            seen = r.c.stats.arrivals;
            if let MowziePresser::AfterArrival { lag } = presser {
                due_visit.get_or_insert(day + lag);
            }
        }
        let visit = match presser {
            MowziePresser::None => false,
            MowziePresser::KillAll(d) => d == day,
            MowziePresser::Farmer { every } => day % every == 0,
            MowziePresser::AfterArrival { .. } => due_visit == Some(day),
            MowziePresser::Visitor { chance } => day % 2 == 0 && rng.bernoulli(chance),
        };
        if visit {
            due_visit = None;
            let at = r.c.now();
            let n = mowzie_visit(&mut r);
            if n > 0 {
                out.kills.push((at.0, n));
            }
        }
        out.members.push(members(&r));
        if day == to / 4 {
            out.save_bytes.0 = r.c.save().len();
        }
        if day == to / 2 {
            out.arrivals_mid = r.c.stats.arrivals;
        }
    }
    out.arrivals = r.c.stats.arrivals;
    out.plunders = r.c.factions[r.f as usize].plunders;
    out.save_bytes.1 = r.c.save().len();
    out
}

/// How well stocked the villages of [`unattended_grove`]'s world are: a plunder finds something worth
/// taking (livestock over the target) only in a village that has a surplus, and the grid's villages
/// do not keep one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VillageStock {
    /// As the grid builds them: a raid usually finds nothing worth taking and no fight is fought.
    Plain,
    /// Every village stands 100 items over its target in every good, topped up every few days: a
    /// raid is always worth it and the village's defence decides what comes back.
    Rich,
}

/// A grove of `found` warriors among the six villages of a grid world, nobody watching and no
/// plugin answering ([`rtsim_conflict::synth::mowzie_world_on`]): the members it holds (home and
/// away) at the end of each day from day 0. For the unattended groves, whose plunder reaches the
/// grid's villages, which the plugin's single emulated camp world (`CampRun`) is too far from.
pub fn unattended_grove(
    seed: u64,
    cultures: rtsim_conflict::Cultures,
    found: u16,
    to: u64,
    villages: VillageStock,
) -> MowzieRun {
    unattended_grove_with(seed, cultures, found, to, villages, true)
}

/// [`unattended_grove`], optionally with a host that never takes the engine's commands. A host takes
/// them every tick (`Conflict::take_commands`, the bridge's), so the engine's outbox is empty at a save;
/// an engine nobody drains queues every command it ever decided (a march record for each raid, about 35
/// bytes), which is not state of the grove's and made the first S9 build's saves look like they grew by
/// 7 bytes a game day.
pub fn unattended_grove_with(
    seed: u64,
    cultures: rtsim_conflict::Cultures,
    found: u16,
    to: u64,
    villages: VillageStock,
    drain: bool,
) -> MowzieRun {
    use rtsim_ecology::infra::NoInfrastructure;
    use rtsim_settle::NodeKind;
    let (mut w, mut c, keys) =
        rtsim_conflict::synth::mowzie_world_on(6, 0, 1, found, seed, cultures);
    let f = c.faction_of_key(keys[0]).expect("the grove");
    let members = |c: &rtsim_conflict::Conflict, t: Tick| {
        c.pool_at(f, t).max(0.0).floor() as u32 + c.factions[f as usize].away
    };
    let mut out = MowzieRun {
        k: c.factions[f as usize].cap,
        members: vec![members(&c, Tick::ZERO)],
        arrivals: 0,
        arrivals_mid: 0,
        kills: Vec::new(),
        save_bytes: (0, 0),
        days: to,
        plunders: 0,
        grief_peak: 0.0,
        save_yearly: Vec::new(),
    };
    let villages_at: Vec<usize> = (0..w.nodes.len())
        .filter(|&i| matches!(w.nodes[i].kind, NodeKind::Village(_)))
        .collect();
    for day in 1..=to {
        let t = Tick::from_days(day);
        if villages == VillageStock::Rich && day % 4 == 1 {
            let goods: Vec<rtsim_trade::GoodId> = w.goods.ids().collect();
            for &i in &villages_at {
                let m = w.market(i);
                let rich: BTreeMap<rtsim_trade::GoodId, rtsim_trade::Amount> = goods
                    .iter()
                    .map(|&g| (g, m.line(g).target + 100 * rtsim_trade::MILLI - m.stock(g)))
                    .filter(|e| e.1 > 0)
                    .collect();
                w.stock_from_players(i, &rich, w.now());
            }
        }
        c.run_until(&mut w, &mut NoInfrastructure, t);
        w.run_until(t);
        if drain {
            let _ = c.take_commands();
        }
        out.members.push(members(&c, t));
        if day % YEAR == 0 {
            out.save_yearly.push(c.save().len());
        }
        if let Some((grief, _)) = c.mood_now(f, t) {
            out.grief_peak = out.grief_peak.max(grief);
        }
        if day == to / 4 {
            out.save_bytes.0 = c.save().len();
        }
        if day == to / 2 {
            out.arrivals_mid = c.stats.arrivals;
        }
    }
    out.arrivals = c.stats.arrivals;
    out.plunders = c.factions[f as usize].plunders;
    out.save_bytes.1 = c.save().len();
    out
}

/// The kernel's rule replayed over a run's kills, alone: `kills` are (tick, members killed) at a
/// camp of K members whose renewal is `rule`, a player's loss each (`n / K` of pressure, read when
/// the wait starts, a running wait never pushed out, the next wait read at an arrival). Returns
/// the arrivals by tick `to`. The engine's own bookkeeping measured against it (S9, MZB-1262).
pub fn replay_arrivals(
    k: u32,
    rule: &rtsim_kernel::renewal::RenewalRule,
    kills: &[(u64, u32)],
    to: u64,
) -> u64 {
    use rtsim_kernel::clock::TICKS_PER_DAY;
    use rtsim_kernel::renewal::Pressure;
    let day = |t: u64| (t / TICKS_PER_DAY) as u32;
    let (mut p, mut missing, mut due, mut arrivals) = (Pressure::default(), 0u32, None::<u64>, 0u64);
    let mut kills = kills.iter().peekable();
    loop {
        let next_kill = kills.peek().map(|k| k.0);
        match due {
            Some(d) if next_kill.is_none_or(|t| d <= t) => {
                if d > to {
                    break;
                }
                missing -= 1;
                arrivals += 1;
                due = (missing > 0).then(|| d + rule.delay_ticks(p, day(d)).max(1));
            }
            _ => {
                let Some(&(t, n)) = kills.next() else { break };
                if t > to {
                    break;
                }
                for _ in 0..n {
                    missing += 1;
                    p.bump(day(t), 1.0 / k.max(1) as f32, rule.half_life_days);
                    if due.is_none() {
                        due = Some(t + rule.delay_ticks(p, day(t)).max(1));
                    }
                }
            }
        }
    }
    arrivals
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

    fn print_village_refills(what: &str, rs: &[VillageRefill]) {
        let b = |f: &dyn Fn(&VillageRefill) -> f64| band(&rs.iter().map(f).collect::<Vec<_>>());
        println!("{what} over {} seeds, median [5th, 95th]:", rs.len());
        println!("  days to the first settlers:    {:?}", b(&|r| r.pair.0));
        println!("  days to the last settlers:     {:?}", b(&|r| r.pair.1));
        println!(
            "  days to 90% of the control:    {:?}",
            b(&|r| days(r.ninety_ctrl))
        );
        println!(
            "  last day over the control:     {:?}",
            b(&|r| r.end_over_ctrl)
        );
        println!(
            "  never 90% of the control: {}, a wiped village never settled: {}",
            rs.iter().filter(|r| r.ninety_ctrl.is_none()).count(),
            rs.iter().filter(|r| r.paired < r.wiped_villages).count(),
        );
        for r in rs {
            println!(
                "  seed {:>2}: pre {:>3} villages {} settled {} pair day {:.0} to {:.0} 90% {:?} settlers {} grown {} refills {} end/ctrl {:.3}",
                r.seed,
                r.pre,
                r.wiped_villages,
                r.paired,
                r.pair.0,
                r.pair.1,
                r.ninety_ctrl,
                r.settlers,
                r.grown,
                r.refills,
                r.end_over_ctrl
            );
        }
    }

    /// Every human village of the synthetic world killed by a player on day 100 (the engine's
    /// census reading each empty): each is ordered its pair 32 days after (P is 1.00 at K), the
    /// unloaded villages grow by the logistic, and 90% of the control's villagers are back within
    /// 600 days (EQUILIBRIUM.md S6, EQ-1 (a); the model: 480).
    #[test]
    fn kill_all_villagers() {
        let r = village_wipe(1, rtsim_settle::Culture::Human, 100, 700, |_| {});
        print_village_refills("kill all villagers", std::slice::from_ref(&r));
        assert_eq!(r.wiped_villages, 6, "{r:?}");
        assert_eq!(r.paired, 6, "every wiped village was settled: {r:?}");
        assert!(
            r.pair.0 >= 32.0 && r.pair.1 <= 33.0,
            "32 days after the wipe: {r:?}"
        );
        assert!(r.ninety_ctrl.is_some_and(|d| d <= 600), "{r:?}");
    }

    /// `kill_all_villagers` on seeds 1 to 30: every village settled on day 32 and the median
    /// seed back to 90% of the control within 600 days.
    #[test]
    #[ignore]
    fn kill_all_villagers_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| {
            village_wipe(seed, rtsim_settle::Culture::Human, 100, 900, |_| {})
        });
        print_village_refills("kill all villagers", &rs);
        for r in &rs {
            assert_eq!(r.paired, r.wiped_villages, "{r:?}");
            assert!(r.pair.0 >= 32.0 && r.pair.1 <= 33.0, "{r:?}");
        }
        let b = band(&rs.iter().map(|r| days(r.ninety_ctrl)).collect::<Vec<_>>());
        assert!(b.0 <= 600.0, "median days to 90% of the control {b:?}");
    }

    /// Every Ribbit village killed by a player on day 100: each is settled 32 days after, refills
    /// its empty houses before building (the population booked as each resident exists), and 90%
    /// of the control's Ribbits are back within 300 days (§3.6).
    #[test]
    fn kill_all_ribbits() {
        let r = village_wipe(1, rtsim_settle::Culture::Ribbit, 100, 500, |_| {});
        print_village_refills("kill all ribbits", std::slice::from_ref(&r));
        assert_eq!(r.wiped_villages, 3, "{r:?}");
        assert_eq!(r.paired, 3, "every wiped village was settled: {r:?}");
        assert!(
            r.pair.0 >= 32.0 && r.pair.1 <= 33.0,
            "32 days after the wipe: {r:?}"
        );
        assert!(r.ninety_ctrl.is_some_and(|d| d <= 300), "{r:?}");
    }

    /// Where the Ribbit refill's prosperity test costs time (EQC167's knob): the same wipe with the
    /// test on (the default) and off, three seeds each. Prints, asserts nothing.
    #[test]
    #[ignore]
    fn the_refill_knob_probe() {
        for needs in [true, false] {
            let rs: Vec<_> = (1..=3)
                .map(|seed| {
                    village_wipe(seed, rtsim_settle::Culture::Ribbit, 100, 700, |c| {
                        c.ribbit_refill_needs_prosperity = needs;
                    })
                })
                .collect();
            print_village_refills(&format!("ribbits, refill needs prosperity {needs}"), &rs);
        }
    }

    /// `kill_all_ribbits` on seeds 1 to 30: the median seed back to 90% of the control within 300
    /// days.
    #[test]
    #[ignore]
    fn kill_all_ribbits_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| {
            village_wipe(seed, rtsim_settle::Culture::Ribbit, 100, 700, |_| {})
        });
        print_village_refills("kill all ribbits", &rs);
        for r in &rs {
            assert_eq!(r.paired, r.wiped_villages, "{r:?}");
        }
        let b = band(&rs.iter().map(|r| days(r.ninety_ctrl)).collect::<Vec<_>>());
        assert!(b.0 <= 300.0, "median days to 90% of the control {b:?}");
    }

    // ---- pool camps (slice S7) -----------------------------------------------------------------

    fn print_camp_refills(what: &str, rs: &[CampRefill]) {
        let b = |f: &dyn Fn(&CampRefill) -> f64| band(&rs.iter().map(f).collect::<Vec<_>>());
        println!("{what} over {} seeds, median [5th, 95th]:", rs.len());
        println!(
            "  days to half of the control:   {:?}",
            b(&|r| days(r.half_ctrl))
        );
        println!(
            "  days to 90% of the control:    {:?}",
            b(&|r| days(r.ninety_ctrl))
        );
        println!(
            "  last day over the control:     {:?}",
            b(&|r| r.end_over_ctrl)
        );
        println!(
            "  never 90%: {}, breaks: {}",
            rs.iter().filter(|r| r.ninety_ctrl.is_none()).count(),
            rs.iter().map(|r| r.breaks).sum::<u64>()
        );
        for r in rs {
            println!(
                "  seed {:>2}: killed {:>3} half {:?} 90% {:?} breaks {} end/ctrl {:.3}",
                r.seed, r.killed, r.half_ctrl, r.ninety_ctrl, r.breaks, r.end_over_ctrl
            );
        }
    }

    /// Every warrior of every camp killed by a player on day 100 (a full camp's kill is P 1.0): 90%
    /// of the control's warriors are back within 120 days and nothing breaks (EQUILIBRIUM.md S7,
    /// EQC233; the model reads 45 to 54 days).
    #[test]
    fn kill_all_camps() {
        let r = camp_wipe(1, 100, 240);
        print_camp_refills("kill all camps", std::slice::from_ref(&r));
        assert!(r.killed >= 100, "{r:?}");
        assert_eq!(r.breaks, 0, "{r:?}");
        assert!(r.ninety_ctrl.is_some_and(|d| d <= 120), "{r:?}");
    }

    /// `kill_all_camps` on seeds 1 to 30: the median seed within 96 days, every one within 120.
    #[test]
    #[ignore]
    fn kill_all_camps_30_seeds() {
        let rs = par_map(&seeds(), threads(), |&seed| camp_wipe(seed, 100, 240));
        print_camp_refills("kill all camps", &rs);
        for r in &rs {
            assert_eq!(r.breaks, 0, "{r:?}");
            assert!(r.ninety_ctrl.is_some_and(|d| d <= 120), "{r:?}");
        }
        let b = band(&rs.iter().map(|r| days(r.ninety_ctrl)).collect::<Vec<_>>());
        assert!(b.0 <= 96.0, "median days to 90% of the control {b:?}");
    }

    fn print_camp_breaks(what: &str, r: &CampRun) {
        let years = r.days as f64 / YEAR as f64;
        println!("{what}: {} days, {} killed", r.days, r.killed);
        for (i, name) in r.culture.iter().enumerate() {
            let bs = &r.breaks[i];
            println!(
                "  {name:7}: {} breaks ({:.3} a year), first {:?}, max P {:.2}",
                bs.len(),
                bs.len() as f64 / years,
                bs.first().map(|b| b.day.round()),
                r.max_p[i]
            );
            for b in bs.iter().take(4) {
                println!(
                    "      day {:>6.0} P {:.2} wait {:>6.1} refound pool {:?}",
                    b.day, b.p, b.wait, b.refound_pool
                );
            }
        }
        println!(
            "  broken share {:.3}",
            r.broken_days as f64 / (r.days as f64 * r.culture.len() as f64)
        );
    }

    /// A player who kills every whole warrior a camp has every second day, for 60 years (the
    /// model's farmer, `camps.py` A and B): each camp breaks at P 2 and is refounded with a quarter
    /// of its cap after `min(384, 48 x 2^P)` days (EQUILIBRIUM.md S7, EQC233).
    #[test]
    fn farmed_camp_breaks_and_refounds() {
        let r = camp_run(
            1,
            rtsim_conflict::Cultures::builtin(),
            &["goblin", "bandit", "raider"],
            Presser::Farmer { every: 2 },
            60 * YEAR,
        );
        print_camp_breaks("farmed 60 years", &r);
        let years = r.days as f64 / YEAR as f64;
        for (i, (name, cap)) in [("goblin", 60.0), ("bandit", 40.0), ("raider", 50.0)]
            .into_iter()
            .enumerate()
        {
            let bs = &r.breaks[i];
            assert!(!bs.is_empty(), "{name} never broke");
            let first = bs[0].day;
            assert!(
                (100.0..=300.0).contains(&first),
                "{name}: first break day {first}"
            );
            let rate = bs.len() as f64 / years;
            assert!(
                (0.15..=0.40).contains(&rate),
                "{name}: {rate} breaks a year"
            );
            for b in bs {
                assert!(b.p >= 2.0, "{name}: {b:?}");
                let want = (48.0 * b.p.exp2()).min(384.0);
                assert!(
                    (b.wait - want).abs() < 0.01,
                    "{name}: waits {} wants {want}",
                    b.wait
                );
                if let Some(pool) = b.refound_pool {
                    // A quarter of the cap, and what a day's recruiting at the pressure then added
                    // before the harness looked (at most 2).
                    let want = (0.25f64 * cap).floor();
                    assert!((want..=want + 2.0).contains(&pool), "{name}: {b:?}");
                }
            }
        }
        let share = r.broken_days as f64 / (r.days as f64 * 3.0);
        assert!((0.35..=0.70).contains(&share), "broken share {share}");
        assert_eq!(
            r.stats_breaks as usize,
            r.breaks.iter().map(Vec::len).sum::<usize>()
        );
    }

    /// With the half-life at 96 days (the plan's), the same farmer never breaks a camp in 60 years:
    /// the finding that decided FB-1 (EQC202).
    #[test]
    fn farmed_camps_do_not_break_at_a_half_life_of_96() {
        let cultures = camp_cultures(|d| {
            if let Some(r) = d.pool_renewal.as_mut() {
                r.half_life_days = 96.0;
            }
        });
        let r = camp_run(
            1,
            cultures,
            &["goblin", "bandit", "raider"],
            Presser::Farmer { every: 2 },
            60 * YEAR,
        );
        print_camp_breaks("farmed 60 years at half-life 96", &r);
        assert_eq!(r.stats_breaks, 0, "{:?}", r.breaks);
        assert!(r.max_p.iter().all(|&p| p < 2.0), "{:?}", r.max_p);
    }

    /// With nobody pressing the camps, 60 years: no break, no refound, no pressure; the warriors
    /// are the same as the engine's with `pool_renewal` removed (the pre-S7 world).
    #[test]
    fn no_player_no_pressure_for_60_years() {
        let names = ["goblin", "bandit", "raider"];
        let on = camp_run(
            2,
            rtsim_conflict::Cultures::builtin(),
            &names,
            Presser::None,
            60 * YEAR,
        );
        let off = camp_run(
            2,
            camp_cultures(|d| d.pool_renewal = None),
            &names,
            Presser::None,
            60 * YEAR,
        );
        assert_eq!(
            (on.stats_breaks, on.stats_refounds, on.broken_days),
            (0, 0, 0)
        );
        assert!(on.max_p.iter().all(|&p| p == 0.0));
        assert_eq!(on.total, off.total, "the same world as before S7");
    }

    /// A wanderer who visits each camp once in `mean_days` on average and kills the warriors at
    /// home: the camps' warriors over years 30 to 60 against the control's, and the share of
    /// camp-days spent broken.
    fn wanderer(mean_days: f64) -> (f64, f64) {
        let names = ["goblin", "bandit", "raider"];
        let ctrl = camp_run(
            3,
            rtsim_conflict::Cultures::builtin(),
            &names,
            Presser::None,
            60 * YEAR,
        );
        let run = camp_run(
            3,
            rtsim_conflict::Cultures::builtin(),
            &names,
            // One camp in three on every second day it goes out.
            Presser::Visitor {
                chance: 6.0 / mean_days,
            },
            60 * YEAR,
        );
        print_camp_breaks(&format!("a wanderer, a visit in {mean_days} days"), &run);
        let late = (30 * YEAR) as usize..=(60 * YEAR) as usize;
        let mean = |r: &CampRun| {
            late.clone().map(|d| r.total[d] as f64).sum::<f64>() / late.clone().count() as f64
        };
        let ratio = mean(&run) / mean(&ctrl);
        let share = run.broken_days as f64 / (run.days as f64 * 3.0);
        println!("  warriors over the control: {ratio:.3}, broken share {share:.3}");
        (ratio, share)
    }

    /// A wanderer who visits each camp once in 800 days (a visit is a full wipe of what is home:
    /// P 1.0, halved in 192 days): the camps hold at least 0.9 of the control's warriors over
    /// years 30 to 60 and are broken at most 5% of the time (EQUILIBRIUM.md §3.6's roamer).
    #[test]
    fn a_wanderer_leaves_the_camps_near_their_caps() {
        let (ratio, share) = wanderer(800.0);
        assert!(ratio >= 0.9, "{ratio}");
        assert!(share <= 0.05, "{share}");
    }

    /// The same at a visit in 400 days: the camps still hold most of their warriors (a visit
    /// that follows another within a season takes P over 2 now and then), and are broken at most
    /// a tenth of the time.
    #[test]
    fn a_wanderer_every_400_days_leaves_most_of_the_warriors() {
        let (ratio, share) = wanderer(400.0);
        assert!(ratio >= 0.75, "{ratio}");
        assert!(share <= 0.10, "{share}");
    }

    /// A player who wipes each camp every half year is farming it: P settles over 2 (each wipe
    /// of a full camp is 1.0, halved by 192 days to 0.52 before the next), so the camps break.
    /// The finding that the break is a farmer's, not a hunter's, threshold (EQC233).
    #[test]
    fn a_wipe_every_half_year_breaks_the_camps_in_the_end() {
        let (ratio, share) = wanderer(180.0);
        assert!(share > 0.05, "{share}");
        assert!(ratio < 0.9, "{ratio}");
    }

    // ---- Frostmaw dens and Umvuthana groves (slice S9, MZB-1262) -------------------------------

    /// The built-in cultures with no plunder (nobody sends a pack anywhere, so nothing the faction
    /// loses itself blurs the rule's own numbers).
    fn quiet_cultures() -> rtsim_conflict::Cultures {
        camp_cultures(|d| {
            if d.name == "umvuthana" {
                d.plunder_mean_days = None;
                d.plunder_mood = None; // the mood bends the wait that is gone (FB-S9-2)
            }
        })
    }

    /// The built-in cultures with the grove's plunder out of reach of every village.
    fn out_of_reach_cultures() -> rtsim_conflict::Cultures {
        camp_cultures(|d| {
            if d.name == "umvuthana" {
                d.plunder_reach_blocks = 20;
            }
        })
    }

    fn rule_of(cultures: &rtsim_conflict::Cultures, name: &str) -> rtsim_kernel::renewal::RenewalRule {
        let c = cultures.get(cultures.index_of(name).expect("a culture"));
        c.renewal.expect("a renewal").rule
    }

    fn mowzie_seeds(n: u64) -> Vec<u64> {
        (1..=n).collect()
    }

    /// Every den killed by a player on a day: each Frostmaw is back in 12.0 days (the wait at P 1),
    /// every seed within 13, and the census line `conflict.den` shows it whole again at 24.
    fn kill_all_dens_over(seeds: &[u64]) {
        let rs = par_map(seeds, threads(), |&seed| {
            mowzie_run(
                seed,
                quiet_cultures(),
                "frostmaw",
                1,
                MowziePresser::KillAll(30),
                60,
            )
        });
        let ds: Vec<f64> = rs
            .iter()
            .map(|r| {
                assert_eq!(r.members[29], 1, "a den holds its Frostmaw before the kill");
                assert_eq!(r.members[30], 0, "killed on day 30");
                r.whole_from(31).map_or(f64::INFINITY, |d| (d - 30) as f64)
            })
            .collect();
        println!("kill all dens over {} seeds, days to whole: {:?}", rs.len(), band(&ds));
        for (r, d) in rs.iter().zip(&ds) {
            assert!((11.5..=13.0).contains(d), "{d} days");
            assert_eq!(r.members[54], 1, "whole at 24 days");
            assert_eq!(r.arrivals, 1);
        }
    }

    #[test]
    fn kill_all_dens() {
        kill_all_dens_over(&mowzie_seeds(3));
    }

    #[test]
    #[ignore]
    fn kill_all_dens_30_seeds() {
        kill_all_dens_over(&seeds());
    }

    /// Every grove (K 5, 7 and 9) wiped by a player on a day, plunder out: the warriors come back
    /// one at a time, whole in 45 to 100 days (K 5, 7, 9 in that order), and all groves of one K
    /// are whole on the same day (no chance in the rule). The design's model reads 54, 73 and 91
    /// days with every kill read at once; the engine's first wait is read at the first death.
    fn kill_all_groves_over(seeds: &[u64]) {
        let mut at = Vec::new();
        for k in [5u16, 7, 9] {
            let rs = par_map(seeds, threads(), |&seed| {
                mowzie_run(
                    seed,
                    quiet_cultures(),
                    "umvuthana",
                    k,
                    MowziePresser::KillAll(30),
                    160,
                )
            });
            let ds: Vec<u64> = rs
                .iter()
                .map(|r| {
                    assert_eq!(r.members[29], u32::from(k));
                    assert_eq!(r.members[30], 0, "wiped on day 30");
                    r.whole_from(31).map_or(u64::MAX, |d| d - 30)
                })
                .collect();
            println!("kill all groves K {k}, days to whole over {} seeds: {ds:?}", rs.len());
            assert!(ds.iter().all(|&d| d == ds[0]), "one day for one K: {ds:?}");
            assert!((45..=100).contains(&ds[0]), "K {k}: {}", ds[0]);
            at.push(ds[0]);
        }
        assert!(at[0] < at[1] && at[1] < at[2], "a bigger grove takes longer: {at:?}");
    }

    #[test]
    fn kill_all_groves() {
        kill_all_groves_over(&mowzie_seeds(2));
    }

    #[test]
    #[ignore]
    fn kill_all_groves_30_seeds() {
        kill_all_groves_over(&seeds());
    }

    /// A den farmed one kill two days after each arrival, years 4 to 12: the arrivals a game year
    /// are the kernel's closed form (`RenewalRule::farmed`, 1.79) within 5%, and the engine's
    /// count equals the rule replayed over its own kills within 5%.
    fn farmed_den_over(seeds: &[u64], years: u64) {
        let cultures = quiet_cultures();
        let rule = rule_of(&cultures, "frostmaw");
        let (_, per_arrival) = rule.farmed(1, 2.0);
        let closed = f64::from(YEAR as f32 / per_arrival);
        let rs = par_map(seeds, threads(), |&seed| {
            mowzie_run(
                seed,
                quiet_cultures(),
                "frostmaw",
                1,
                MowziePresser::AfterArrival { lag: 2 },
                years * YEAR,
            )
        });
        let mut yields = Vec::new();
        for r in &rs {
            let to = years * YEAR;
            let from = 4 * YEAR * Tick::from_days(1).0;
            let late = r
                .kills
                .iter()
                .filter(|k| k.0 >= from)
                .count() as f64
                / (years - 4) as f64;
            let replayed = replay_arrivals(r.k, &rule, &r.kills, Tick::from_days(to).0);
            assert!(
                (r.arrivals as f64 - replayed as f64).abs() <= 0.05 * replayed as f64 + 1.0,
                "the engine {} against the rule {replayed}",
                r.arrivals
            );
            yields.push(late);
        }
        println!("farmed den, kills a game year over years 4 to {years}: {:?}, closed form {closed:.3}", band(&yields));
        for y in &yields {
            assert!((y - closed).abs() <= 0.05 * closed, "{y} against the closed form {closed}");
        }
    }

    #[test]
    fn farmed_den_yields_closed_form() {
        farmed_den_over(&mowzie_seeds(4), 12);
    }

    #[test]
    #[ignore]
    fn farmed_den_yields_closed_form_30_seeds() {
        farmed_den_over(&seeds(), 60);
    }

    /// A grove farmed by a visitor every two days (it kills what is home): the warriors a game
    /// year are the kernel's rule replayed over the same kills (the chain: a farmer who kills what
    /// is there leaves the grove short by several, so the waits chain, 5.7, 6.7 and 7.5 for K 5,
    /// 7 and 9 in the model, not the one-at-a-time 5.8), within 5%.
    fn farmed_grove_over(seeds: &[u64], years: u64) {
        let cultures = quiet_cultures();
        let rule = rule_of(&cultures, "umvuthana");
        for k in [5u16, 7, 9] {
            let rs = par_map(seeds, threads(), |&seed| {
                mowzie_run(
                    seed,
                    quiet_cultures(),
                    "umvuthana",
                    k,
                    MowziePresser::Farmer { every: 2 },
                    years * YEAR,
                )
            });
            let mut yields = Vec::new();
            for r in &rs {
                let to = Tick::from_days(years * YEAR).0;
                let replayed = replay_arrivals(r.k, &rule, &r.kills, to);
                assert!(
                    (r.arrivals as f64 - replayed as f64).abs() <= 0.05 * replayed as f64 + 1.0,
                    "K {k}: the engine {} against the rule {replayed}",
                    r.arrivals
                );
                // The Umvuthi's heir takes a warrior once; the rest are the farmed warriors.
                yields.push(r.arrivals as f64 / years as f64);
            }
            println!("farmed grove K {k}, warriors a game year: {:?}", band(&yields));
            // The chain is not the one-at-a-time form: it yields more than 5% over §3.3's 5.8
            // for a grove of 7 and under it for 5.
            let m = band(&yields).0;
            assert!(m > 4.0 && m < 9.5, "K {k}: {m}");
        }
    }

    #[test]
    fn farmed_grove_yields_the_chained_rate() {
        farmed_grove_over(&mowzie_seeds(2), 12);
    }

    #[test]
    #[ignore]
    fn farmed_grove_yields_the_chained_rate_30_seeds() {
        farmed_grove_over(&seeds(), 60);
    }

    /// No player, villages of pop 6 and over in reach: a grove loses a raptor pack to a village's
    /// defence at nearly every plunder (the faction's own loss, no pressure), which with its plunder
    /// mood (FB-S9-2) puts its next plunder off to about every 34 days; it holds 0.96 to 0.98 of its
    /// warriors for years (without the mood 0.91 to 0.95, plundering every 12), flat by century;
    /// never pressed.
    fn unattended_groves_over(seeds: &[u64], years: u64) {
        for stock in [VillageStock::Plain, VillageStock::Rich] {
            let rs = par_map(seeds, threads(), |&seed| {
                [5u16, 7, 9].map(|k| {
                    unattended_grove(seed, rtsim_conflict::Cultures::builtin(), k, years * YEAR, stock)
                })
            });
            for (i, k) in [5u16, 7, 9].into_iter().enumerate() {
                let held: Vec<f64> = rs.iter().map(|r| r[i].held(YEAR, years * YEAR)).collect();
                let early: Vec<f64> =
                    rs.iter().map(|r| r[i].held(YEAR, years * YEAR / 2)).collect();
                let late: Vec<f64> =
                    rs.iter().map(|r| r[i].held(years * YEAR / 2, years * YEAR)).collect();
                println!(
                    "unattended grove K {k}, {stock:?} villages, share held: {:?}, early {:?}, late {:?}",
                    band(&held),
                    band(&early),
                    band(&late)
                );
                let (m, _, _) = band(&held);
                let gap: Vec<f64> = rs.iter().map(|r| r[i].plunder_days()).collect();
                let grief: Vec<f64> = rs.iter().map(|r| f64::from(r[i].grief_peak)).collect();
                println!(
                    "  plunder every {:?} days, peak grief {:?}",
                    band(&gap),
                    band(&grief)
                );
                match stock {
                    // Nothing worth taking, no fight, no loss.
                    VillageStock::Plain => assert!(m >= 0.97, "K {k}: {m}"),
                    // Its mood (FB-S9-2, Ben, 2026-10-30): a lost pack puts the next plunder off, to about every
                    // 34 days against 12 (30 seeds, 60 years: 33.7 [23.7, 62.6]), and the grove holds 0.963, 0.974
                    // and 0.979 of K 5, 7 and 9 (5th to 95th 0.947 to 0.988). The bands leave room for 2 seeds.
                    VillageStock::Rich => {
                        let (lo, hi) = match k {
                            5 => (0.94, 0.985),
                            7 => (0.955, 0.99),
                            _ => (0.96, 0.995),
                        };
                        assert!((lo..=hi).contains(&m), "K {k}: {m}");
                        assert!((26.0..=42.0).contains(&band(&gap).0), "K {k}: {gap:?}");
                    }
                }
                assert!((band(&early).0 - band(&late).0).abs() < 0.06, "flat: {early:?} {late:?}");
            }
        }
    }

    /// A grove's saved state does not grow with the years (S9, the coordinator's check for S11's soak): sixty game
    /// years among villages with a surplus, plundering about every 34 days, its save at the end of each year stays
    /// under a fixed bound and within 256 bytes of its first year's, for K 5, 7 and 9. The first S9 build's run
    /// looked like growth of 7 bytes a game day (1.8 MB over a soak of 262,980 days): the harness never took the
    /// engine's commands, so the outbox kept a march record for each raid. Undrained it still grows, a host that
    /// takes commands being the engine's contract; drained it is flat (1.6 to 1.8 KB at K 5 to 9).
    #[test]
    fn a_groves_save_does_not_grow_with_the_years() {
        for k in [5u16, 7, 9] {
            let r = unattended_grove(1, rtsim_conflict::Cultures::builtin(), k, 60 * YEAR, VillageStock::Rich);
            assert_eq!(r.save_yearly.len(), 60);
            assert!(r.plunders >= 100, "K {k}: the raids were exercised: {}", r.plunders);
            let (lo, hi) = (
                *r.save_yearly[1..].iter().min().unwrap(),
                *r.save_yearly.iter().max().unwrap(),
            );
            println!("K {k}: save bytes by year min {lo} max {hi}, {} plunders", r.plunders);
            assert!(hi <= 4096, "K {k}: the save reached {hi} bytes: {:?}", r.save_yearly);
            assert!(hi - lo <= 256, "K {k}: {lo} to {hi}: {:?}", r.save_yearly);
            assert!(
                r.save_yearly[59] <= r.save_yearly[0] + 256,
                "K {k}: {} then {}",
                r.save_yearly[0],
                r.save_yearly[59]
            );
        }
        // The cause, pinned: the same grove with a host that never takes the commands grows by a march
        // record for each raid.
        let a = unattended_grove_with(1, rtsim_conflict::Cultures::builtin(), 7, 30 * YEAR, VillageStock::Rich, false);
        assert!(
            a.save_yearly[29] > a.save_yearly[4] + 500,
            "undrained: {:?}",
            a.save_yearly
        );
    }

    #[test]
    fn unattended_groves_hold_their_band() {
        unattended_groves_over(&mowzie_seeds(2), 12);
    }

    #[test]
    #[ignore]
    fn unattended_groves_hold_their_band_30_seeds() {
        unattended_groves_over(&seeds(), 60);
    }

    /// With no village within the plunder reach nothing is lost and the grove holds everything.
    #[test]
    fn a_grove_with_no_village_in_reach_holds_everything() {
        for k in [5u16, 7, 9] {
            let r = unattended_grove(1, out_of_reach_cultures(), k, 12 * YEAR, VillageStock::Rich);
            assert_eq!(r.held(0, 12 * YEAR), 1.0, "K {k}");
        }
    }

    /// Sixty game years under each script, dens and groves on separate worlds: a den with no one
    /// pressing holds 1.00 and one under a roamer 0.93 or more; a grove with no one holds 0.95 to
    /// 0.995 with its plunder mood (K 7: 0.974 [0.962, 0.986], FB-S9-2; 1.00 with no village in reach); the renewal events a year do not drift by century;
    /// the engine's save does not grow. The roamer's bands are the first run's own (EQUILIBRIUM.md §4).
    fn long_horizon_over(seeds: &[u64], years: u64) {
        let to = years * YEAR;
        // The drift between the halves is judged on many seeds: a roamer's visits are rare.
        let judged = seeds.len() >= 20;
        for (culture, found, name) in [("frostmaw", 1u16, "den"), ("umvuthana", 7u16, "grove")] {
            let scripts = [
                ("none", MowziePresser::None),
                ("roamer", MowziePresser::Visitor { chance: 0.01 }),
                ("wanderer", MowziePresser::Visitor { chance: 2.0 / 400.0 }),
            ];
            for (script, presser) in scripts {
                let rs = par_map(seeds, threads(), |&seed| {
                    if name == "grove" && script == "none" {
                        // Villages in reach with a surplus: the plunders lose their packs.
                        unattended_grove(
                            seed,
                            rtsim_conflict::Cultures::builtin(),
                            found,
                            to,
                            VillageStock::Rich,
                        )
                    } else {
                        mowzie_run(seed, rtsim_conflict::Cultures::builtin(), culture, found, presser, to)
                    }
                });
                let held: Vec<f64> = rs.iter().map(|r| r.held(YEAR, to)).collect();
                let mean = |f: &dyn Fn(&MowzieRun) -> f64| rs.iter().map(f).sum::<f64>() / rs.len() as f64;
                let (a, b) = (mean(&|r| r.held(YEAR, to / 2)), mean(&|r| r.held(to / 2, to)));
                let (ra, rb) = (
                    mean(&|r| r.arrivals_mid as f64),
                    mean(&|r| (r.arrivals - r.arrivals_mid) as f64),
                );
                let bytes: Vec<(usize, usize)> = rs.iter().map(|r| r.save_bytes).collect();
                println!(
                    "{name} under {script}: held {:?}, halves {a:.3} {b:.3}, arrivals by half {ra:.1} {rb:.1}, save bytes {:?}",
                    band(&held),
                    bytes.iter().take(3).collect::<Vec<_>>()
                );
                let (m, _, _) = band(&held);
                match (name, script) {
                    ("den", "none") => assert!(m >= 0.995, "{m}"),
                    ("den", _) => assert!(m >= 0.90, "{m}"),
                    ("grove", "none") => assert!((0.95..=0.995).contains(&m), "{m}"),
                    // Under a visitor the grove is pressed and a warrior comes back one at a time at the wait the
                    // pressure sets: measured on the two seeds of the quick run, 0.870 under a roamer (the worst seed
                    // 0.703) and 0.922 under a wanderer (0.766), after the S9 review's P2 fix; a mean under 0.75 is a
                    // grove that does not renew.
                    ("grove", _) => assert!((0.75..=1.0).contains(&m), "{m}"),
                    _ => assert!((0.0..=1.0).contains(&m), "{m}"),
                }
                if judged || script == "none" {
                    assert!((a - b).abs() < 0.08, "{name} {script}: drift {a} {b}");
                    assert!(
                        (ra - rb).abs() <= 0.25 * ra.max(rb).max(1.0),
                        "{name} {script}: the renewals drift {ra} {rb}"
                    );
                }
                // The engine's save does not grow with the years. (The rich-village grove's once seemed to,
                // by 7 bytes a game day: its harness never took the engine's commands, so the outbox
                // grew by a march record for each raid, `a_groves_save_does_not_grow_with_the_years`.)
                for (a, b) in &bytes {
                    assert!(*b <= 2 * *a + 4096, "{name} {script}: the save grows {a} {b}");
                }
            }
        }
    }

    #[test]
    fn long_horizon_mowzie_with_kills() {
        long_horizon_over(&mowzie_seeds(2), 12);
    }

    #[test]
    #[ignore]
    fn long_horizon_mowzie_with_kills_30_seeds() {
        long_horizon_over(&seeds(), 60);
    }
}
