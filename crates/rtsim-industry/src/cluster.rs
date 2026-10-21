// SPDX-License-Identifier: AGPL-3.0-or-later
//! The replay (C19, I6; SCALE.md §4 and §7): unloaded districts as piecewise-linear processes over
//! integer pools, one kernel [`LazyModel`] per *cluster* of districts that share stock.
//!
//! # Pools, processes, constraints
//!
//! - A **pool** is integer stock: a district's boundary stock of one good (summed over its
//!   inventories), the packages a district has written into a postbox's offline buffer, or a
//!   finite deposit (a good of class `Deposit`, I11). No pool is shared between districts: an
//!   Ender Transmission channel joins machines of one district (I13, Ben's correction), so it is
//!   internal plumbing, never stock between distant districts.
//! - A **box** process is one group of a district's learned model (I6): one diffuser per good,
//!   negative for the goods it consumes and positive for those it produces. It runs at full rate
//!   or not at all. A district has one per coupling group its starvation episodes showed apart
//!   (one, all-or-nothing, until there is evidence).
//!   Its emissions are mints and burns, booked per pool.
//! - A **link** process moves one good from one pool to another at a rate (a packager filling a
//!   postbox). Its emissions are transfers: one diffuser, so what
//!   leaves one pool is exactly what enters the other.
//! - **Constraints:** every pool stays at or above 0 and at or below its capacity; a process with
//!   a learned **target** stops when the target pool reaches it and restarts at or below its
//!   restart level; a district's shared **item space** (a weighted sum of its item pools) stays
//!   under the slot capacity of the inventories that received its outputs; a process that
//!   **requires** a district (PowerGrid coupling, I13) runs only while every box process of that
//!   district runs: the black box cannot tell which of a source's processes is its generator, so
//!   any of them stopping cuts the power (the safe side).
//!
//! # Breakpoints
//!
//! Between events, every pool is a sum of diffusers, so it is linear with sub-unit rounding. A
//! constraint with a single term (the common case: one producer per good) is solved exactly from
//! the diffuser's carry. One with several terms uses a linear envelope that bounds the rounding
//! from the safe side, so the replay may stop a few units early, never late (overnight choice F9).
//! The next breakpoint is the earliest of: the end of any running process's safe run, the moment a
//! stopped process could restart and run for at least `restart_min_run_ticks`, a package batch
//! completing, and a season boundary. **Catch-up costs O(breakpoints + events)**, independent of
//! elapsed time: a settle over a million ticks is one multiply per diffuser.
//!
//! # Determinism
//!
//! The model has no slot period and no randomness (the district has none, as the brief requires).
//! The running set is re-evaluated only in [`LazyModel::settle`] (at breakpoints and events) and
//! after events, never in reads, so the kernel's composition rule holds trivially.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::{SlotPeriod, Tick};
use rtsim_kernel::conserve::{Conserved, Diffuser, QuantityKind, Totals};
use rtsim_kernel::lazy::LazyModel;
use rtsim_kernel::rng::SlotRng;
use serde::Serialize;

use crate::ids::{DistrictId, Good};

/// One unit in 32.32 fixed point.
const U: i128 = 1 << 32;

/// What a pool holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PoolKey {
    /// A district's boundary stock of a good (a deposit is a good of class `Deposit`).
    Stock(DistrictId, Good),
    /// Goods a district's packager wrote into packages in a postbox offline buffer (inventory
    /// index), to one address (NAME id), not yet fetched by a train (INDUSTRY.md §8.2).
    Mail(DistrictId, u32, u32, Good),
    /// One store's stock of a good in a glass replay (GLASSBOX.md §4.1): the store's IND_INVENTORY
    /// index. A district replaying on its compiled graph books per store, never into `Stock`
    /// pools except for events ([`Cluster::add_to_pool`]).
    Store(DistrictId, u32, Good),
}

impl PoolKey {
    pub fn good(self) -> Good {
        match self {
            PoolKey::Stock(_, g) | PoolKey::Mail(_, _, _, g) | PoolKey::Store(_, _, g) => g,
        }
    }

    pub fn district(self) -> DistrictId {
        match self {
            PoolKey::Stock(d, _) | PoolKey::Mail(d, _, _, _) | PoolKey::Store(d, _, _) => d,
        }
    }
}

/// Integer stock with its books.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pool {
    pub key: PoolKey,
    pub level: i64,
    /// Physical capacity, if any (a tank, an energy store, a postbox's 18 slots).
    pub cap: Option<i64>,
    /// Mail pools: the level the world already holds (packages written, plus real transfers it
    /// reported). The rest is pushed by command (§8.2).
    pub pushed: i64,
    pub books: Books,
}

/// How a pool's level came about; [`Cluster::check_books`] verifies it exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Books {
    pub initial: i64,
    /// Events: imports, exports, corrections, theft (signed).
    pub events: i64,
    pub minted: i64,
    pub burned: i64,
    pub moved_in: i64,
    pub moved_out: i64,
}

/// One good's diffuser inside a box process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Term {
    pub pool: u16,
    #[serde(skip)]
    pub flow: Diffuser,
}

/// What a process does while it runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Body {
    /// The district's learned all-or-nothing process (I6).
    Box { terms: Vec<Term> },
    /// A transfer from one pool to another. `batch` marks a packager: every `batch` units that
    /// reach `to` are one package written into the offline buffer.
    Link {
        from: u16,
        to: u16,
        #[serde(skip)]
        flow: Diffuser,
        batch: Option<i64>,
    },
    /// A strictly periodic sequence of regimes (G-B8, G-B9; GLASSBOX.md §4.2, §4.3): one cycle of
    /// pieces, repeated, in closed form. A settle over n whole cycles is one multiply per pool.
    /// The engine found the cycle and bounds it with the process's deadline: the cluster checks no
    /// constraint against it (its pieces are the breakpoints it was found from).
    Cycle(Box<CycleBody>),
}

/// One piece of a cycle: a regime between two of the breakpoints it was found from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CyclePiece {
    /// Its length in ticks.
    pub len: u64,
    /// Per pool: the 32.32 rate.
    pub rates: Vec<i64>,
    /// The breakpoints at the piece's start, in order: each pool that fired, with the carry its
    /// time shift lands it on ([`shift_back`], choice G2-12).
    pub fires: Vec<CycleFire>,
}

/// A breakpoint inside a cycle: pool `pool` (an index into the cycle's pools) fired, and the
/// regime before it is shifted back in time until that pool's carry is `canon`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CycleFire {
    pub pool: u16,
    pub canon: u32,
}

/// The carry a breakpoint's time shift lands its fired pool on: a rising pool (the crossing is a
/// unit up) at the lowest carry from which a tick of `rate` makes the unit, a falling one at the
/// highest from which a tick loses it. None for a pool not moving that way.
pub fn fire_canon(rate: i64, rising: bool) -> Option<u32> {
    let r = i128::from(rate);
    match (rising, r) {
        (true, r) if r > 0 => u32::try_from((U - r).max(0)).ok(),
        (false, r) if r < 0 => u32::try_from((-r - 1).min(U - 1)).ok(),
        _ => None,
    }
}

/// A breakpoint's time shift (choice G2-12). The regime that just ended (its per-pool `rates`)
/// stopped at the last tick before pool `f` crossed; its carry is somewhere within a tick's rate of
/// the crossing. Shifting the whole regime back by `delta = (c[f] - canon) / rates[f]` ticks
/// (`0 <= delta < 1`) puts that carry on `canon`, so a repeated regime sequence repeats exactly,
/// and moves every other pool back by its own rate times `delta`, rounded so that no output gains
/// and no input gets back more than it gave. Goods stay consistent with the recipes (every flow
/// of the regime loses the same time), and nothing runs ahead of the control. Carries may leave
/// `[0, U)`; the caller turns that into units. False if the carry is not at a crossing.
fn shift_back(c: &mut [i128], rates: &[i64], f: usize, canon: u32) -> bool {
    let rf = i128::from(rates[f]);
    let mut num = c[f] - i128::from(canon);
    let mut den = rf;
    if den < 0 {
        num = -num;
        den = -den;
    }
    if den == 0 || num < 0 || num >= den {
        return false;
    }
    if num == 0 {
        return true;
    }
    for (k, ck) in c.iter_mut().enumerate() {
        if k == f {
            *ck = i128::from(canon);
        } else {
            *ck -= div_ceil(i128::from(rates[k]) * num, den);
        }
    }
    true
}

/// Applies `fires` in order to carries `c` after a regime of `rates`, adding the units they move
/// to `e`.
fn apply_fires(c: &mut [i128], e: &mut [i128], rates: &[i64], fires: &[CycleFire]) {
    for fire in fires {
        shift_back(c, rates, fire.pool as usize, fire.canon);
        for (ck, ek) in c.iter_mut().zip(e.iter_mut()) {
            *ek += ck.div_euclid(U);
            *ck = ck.rem_euclid(U);
        }
    }
}

/// One period of `pieces` from carries `c0` at the first piece's start (its fires applied): per
/// pool, the units emitted and the carry at the next period's start (the first piece's fires
/// applied again).
pub fn cycle_period(pieces: &[CyclePiece], c0: &[u32]) -> (Vec<i128>, Vec<u32>) {
    let mut c: Vec<i128> = c0.iter().map(|&x| i128::from(x)).collect();
    let mut e = vec![0i128; c.len()];
    for (i, p) in pieces.iter().enumerate() {
        if i > 0 {
            apply_fires(&mut c, &mut e, &pieces[i - 1].rates, &p.fires);
        }
        for (j, (cj, ej)) in c.iter_mut().zip(e.iter_mut()).enumerate() {
            let t = *cj + i128::from(p.rates[j]) * i128::from(p.len);
            *ej += t.div_euclid(U);
            *cj = t.rem_euclid(U);
        }
    }
    if let (Some(first), Some(last)) = (pieces.first(), pieces.last()) {
        apply_fires(&mut c, &mut e, &last.rates, &first.fires);
    }
    (e, c.into_iter().map(|x| x as u32).collect())
}

/// The pools some piece of `pieces` fires.
pub fn cycle_fired(pieces: &[CyclePiece], pools: usize) -> Vec<bool> {
    let mut out = vec![false; pools];
    for f in pieces.iter().flat_map(|p| &p.fires) {
        if let Some(x) = out.get_mut(f.pool as usize) {
            *x = true;
        }
    }
    out
}

/// One cycle of a [`Body::Cycle`]: every piece's rates over the same pools.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CycleBody {
    /// The pools the pieces touch.
    pub pools: Vec<u16>,
    pub pieces: Vec<CyclePiece>,
    /// The piece it is in and the ticks into it.
    pub piece: u32,
    pub off: u64,
    /// Per pool: the current piece's diffuser (rate and carry).
    #[serde(skip)]
    pub flows: Vec<Diffuser>,
}

impl CycleBody {
    /// The cycle's length in ticks.
    pub fn period(&self) -> u64 {
        self.pieces.iter().map(|p| p.len).sum()
    }

    /// Emissions per pool over `dt` ticks from where it is, advancing it. Exact: whole periods
    /// step one at a time until every fired pool enters a period at the carry it entered the last
    /// one with; from then on each period repeats ([`cycle_period`]), so n more of them are n
    /// times the fired pools' emission and, for the others, `floor((carry + n M) / U)` with M a
    /// period's move.
    fn advance(&mut self, dt: u64, out: &mut [i64]) {
        debug_assert_eq!(out.len(), self.pools.len());
        let period = self.period();
        if period == 0 || self.pieces.is_empty() {
            return;
        }
        let m = self.pools.len();
        let fired = cycle_fired(&self.pieces, m);
        let mut left = dt;
        while left > 0 {
            if self.piece == 0 && self.off == 0 && left >= period {
                let mut n = left / period;
                left -= n * period;
                let mut c: Vec<u32> = self.flows.iter().map(|f| f.carry()).collect();
                let add = |e: &[i128], out: &mut [i64]| {
                    for (o, &x) in out.iter_mut().zip(e) {
                        *o += i64::try_from(x).expect("cycle emission");
                    }
                };
                while n > 0 {
                    let (e1, c1) = cycle_period(&self.pieces, &c);
                    add(&e1, out);
                    n -= 1;
                    if n == 0 {
                        c = c1;
                        break;
                    }
                    let (e2, c2) = cycle_period(&self.pieces, &c1);
                    let steady = (0..m).all(|j| !fired[j] || c1[j] == c2[j]);
                    if !steady {
                        c = c1;
                        continue;
                    }
                    // Steady from c1: n more periods.
                    let n128 = i128::from(n);
                    let mut e = vec![0i128; m];
                    for j in 0..m {
                        if fired[j] {
                            e[j] = e2[j] * n128;
                            c[j] = c1[j];
                        } else {
                            let mv = e2[j] * U + i128::from(c2[j]) - i128::from(c1[j]);
                            let t = i128::from(c1[j]) + mv * n128;
                            e[j] = t.div_euclid(U);
                            c[j] = t.rem_euclid(U) as u32;
                        }
                    }
                    add(&e, out);
                    n = 0;
                }
                for (j, f) in self.flows.iter_mut().enumerate() {
                    *f = Diffuser::with_carry(self.pieces[0].rates[j], c[j]);
                }
                continue;
            }
            let len = self.pieces[self.piece as usize].len;
            let step = left.min(len - self.off);
            for (j, e) in out.iter_mut().enumerate() {
                *e += self.flows[j].emit(step);
            }
            self.off += step;
            left -= step;
            if self.off >= len {
                let before = self.piece as usize;
                self.piece = (self.piece + 1) % self.pieces.len() as u32;
                self.off = 0;
                let mut c: Vec<i128> = self.flows.iter().map(|f| i128::from(f.carry())).collect();
                let mut e = vec![0i128; m];
                let (prev, p) = (&self.pieces[before], &self.pieces[self.piece as usize]);
                apply_fires(&mut c, &mut e, &prev.rates, &p.fires);
                for j in 0..m {
                    out[j] += i64::try_from(e[j]).expect("cycle emission");
                    self.flows[j] = Diffuser::with_carry(p.rates[j], c[j] as u32);
                }
            }
        }
    }
}

/// A learned target (I6): a stock level at which production was seen to stop while inputs were
/// available, with the level it restarted at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Target {
    pub pool: u16,
    pub stop_at: i64,
    pub restart_at: i64,
}

/// Why a process is not running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Stop {
    /// Its district is frozen, live, damaged, or out of season.
    Disabled,
    InputEmpty(PoolKey),
    OutputFull(PoolKey),
    SpaceFull(DistrictId),
    Target(PoolKey),
    /// The process it requires (a PowerGrid source) is not running.
    SourceStopped(DistrictId),
    /// Never started: no restart has passed its check yet.
    Waiting,
    /// A glass replay's bound (a store's space, or a watch on a threshold) with its tag
    /// ([`Space::tag`]).
    Bound(DistrictId, u32),
    /// The process's deadline passed ([`Proc::deadline`]): a glass replay's timer (a heater's
    /// phase, a latency's arrival, a cycle's end).
    Timer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Run {
    Running,
    Stopped(Stop),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Proc {
    pub owner: DistrictId,
    /// Box processes first (0, 1, ...), links after. Lower ids have priority when stock is shared.
    pub id: u16,
    pub body: Body,
    pub targets: Vec<Target>,
    /// A district every box process of which must be running for this one to run (a PowerGrid
    /// source, I13).
    pub requires: Option<DistrictId>,
    pub enabled: bool,
    pub run: Run,
    /// A batch machine starts only at its next cycle after its input arrives: a restart after a
    /// stop waits this long (learned, F23). 0 restarts at once.
    pub latency: u64,
    /// When a restart that passed its check may happen (the latency running).
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub armed: Option<Tick>,
    /// Has run at least once in this cluster.
    pub ran: bool,
    /// Stops it ([`Stop::Timer`]) at this tick, and it does not restart past it (the engine
    /// replaces it).
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub deadline: Option<Tick>,
    /// The least run a restart must allow, in ticks; 0 takes the cluster's
    /// `restart_min_run_ticks` (a glass replay's process uses 1: its breakpoints are exact).
    pub min_run: u64,
}

/// A district's shared item space: `fixed + Σ weight × level ≤ cap`, in slot units. A glass
/// replay's bounds have the same form (a store's space, a watch on a threshold, with negative
/// weights for a level that must not fall), each with its tag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Space {
    pub owner: DistrictId,
    /// 0: the black box's item space ([`Stop::SpaceFull`]); else a glass bound ([`Stop::Bound`]).
    pub tag: u32,
    pub weights: Vec<(u16, i64)>,
    pub fixed: i64,
    pub cap: i64,
}

/// Something the engine must act on, recorded at a breakpoint or event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ClusterEvent {
    Stopped {
        #[serde(serialize_with = "crate::ser::tick")]
        at: Tick,
        owner: DistrictId,
        id: u16,
        why: Stop,
    },
    Started {
        #[serde(serialize_with = "crate::ser::tick")]
        at: Tick,
        owner: DistrictId,
        id: u16,
    },
    /// `count` packages of `batch` units each became ready in a mail pool.
    PackagesReady {
        #[serde(serialize_with = "crate::ser::tick")]
        at: Tick,
        pool: PoolKey,
        count: i64,
        batch: i64,
    },
    SeasonBoundary {
        #[serde(serialize_with = "crate::ser::tick")]
        at: Tick,
    },
}

/// A process as the engine describes it, by pool key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcSpecBody {
    Box(Vec<(PoolKey, Diffuser)>),
    Link {
        from: PoolKey,
        to: PoolKey,
        flow: Diffuser,
        batch: Option<i64>,
    },
    /// A cycle from its first piece: its pools, and its pieces over them. Carries come from the
    /// process it replaces ([`Cluster::replace_procs`]), the first piece's fires already applied
    /// ([`Cluster::shift_back`]).
    Cycle {
        pools: Vec<PoolKey>,
        pieces: Vec<CyclePiece>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcSpec {
    pub id: u16,
    pub body: ProcSpecBody,
    /// (pool, stop at, restart at).
    pub targets: Vec<(PoolKey, i64, i64)>,
    pub requires: Option<DistrictId>,
    /// Restart latency in ticks ([`Proc::latency`]).
    pub latency: u64,
    /// False starts the process disabled (a jammed line, F20).
    pub enabled: bool,
    /// [`Proc::deadline`].
    pub deadline: Option<Tick>,
    /// [`Proc::min_run`].
    pub min_run: u64,
}

/// A district's item space as the engine describes it: (weights by pool, fixed units, cap units).
pub type SpaceSpec = (Vec<(PoolKey, i64)>, i64, i64);

/// Everything a district brings into a cluster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberSpec {
    pub district: DistrictId,
    /// (pool, level, cap).
    pub pools: Vec<(PoolKey, i64, Option<i64>)>,
    pub procs: Vec<ProcSpec>,
    pub space: Option<SpaceSpec>,
    /// A glass replay's tagged bounds ([`Space::tag`], never 0).
    pub bounds: Vec<(u32, SpaceSpec)>,
}

/// What a district takes out of a cluster: its pools and their books.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberOut {
    pub district: DistrictId,
    pub pools: Vec<Pool>,
}

/// What a read sees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClusterView {
    pub levels: Vec<(PoolKey, i64)>,
    pub runs: Vec<(DistrictId, u16, Run)>,
}

impl ClusterView {
    pub fn level(&self, key: PoolKey) -> i64 {
        self.levels
            .binary_search_by_key(&key, |e| e.0)
            .map_or(0, |i| self.levels[i].1)
    }

    pub fn run_of(&self, d: DistrictId, id: u16) -> Option<Run> {
        self.runs
            .iter()
            .find(|r| r.0 == d && r.1 == id)
            .map(|r| r.2)
    }
}

/// One unloaded cluster: districts that share running states (PowerGrid lines, I13).
/// Most clusters hold one district.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    pools: Vec<Pool>,
    procs: Vec<Proc>,
    spaces: Vec<Space>,
    season_boundary: Option<Tick>,
    restart_min_run: u64,
    events: Vec<ClusterEvent>,
    /// Link transfers into and out of pools that have left with their district, so the
    /// remaining books still balance (a link never crosses districts, so this stays balanced on
    /// its own; it is kept as a check).
    departed: (i64, i64),
    /// Settles and evaluations so far: the cost the scale contract counts.
    pub settles: u64,
}

/// A term of a constraint: `a × emit(dt)` of a diffuser with this rate and carry.
#[derive(Clone, Copy, Debug)]
struct Tm {
    a: i64,
    rate: i64,
    carry: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Lower(i64),
    Upper(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Con {
    Lower(u16),
    Upper(u16),
    /// (process, target index).
    Target(usize, usize),
    Space(usize),
    /// A package batch completing in a mail pool fed by link `usize`.
    Batch(usize),
}

fn div_floor(a: i128, b: i128) -> i128 {
    debug_assert!(b > 0);
    a.div_euclid(b)
}

fn div_ceil(a: i128, b: i128) -> i128 {
    debug_assert!(b > 0);
    -(-a).div_euclid(b)
}

fn to_ticks(x: i128) -> u64 {
    if x <= 0 {
        0
    } else {
        u64::try_from(x).unwrap_or(u64::MAX)
    }
}

/// The largest n such that `v0 + Σ a·emit(dt)` stays within `bound` for every dt in 1..=n, or
/// `None` if it never leaves it. Exact for one term with |a| = 1; a safe-side envelope otherwise.
fn safe_run(v0: i64, bound: Bound, terms: &[Tm]) -> Option<u64> {
    let v0 = i128::from(v0);
    if let [t] = terms
        && t.a.abs() == 1
    {
        let (r, c) = (i128::from(t.rate), i128::from(t.carry));
        return match (bound, t.a) {
            (Bound::Lower(lo), 1) => {
                if r >= 0 {
                    (v0 < i128::from(lo)).then_some(0)
                } else {
                    // floor((c + r dt)/U) >= lo - v0  <=>  |r| dt <= c + (v0 - lo) U
                    Some(to_ticks(div_floor(c + (v0 - i128::from(lo)) * U, -r)))
                }
            }
            (Bound::Lower(lo), _) => {
                // v0 - floor((c + r dt)/U) >= lo  <=>  c + r dt < (v0 - lo + 1) U
                if r <= 0 {
                    (v0 < i128::from(lo)).then_some(0)
                } else {
                    Some(to_ticks(div_floor(
                        (v0 - i128::from(lo) + 1) * U - c - 1,
                        r,
                    )))
                }
            }
            (Bound::Upper(hi), 1) => {
                // v0 + floor((c + r dt)/U) <= hi  <=>  c + r dt < (hi - v0 + 1) U
                if r <= 0 {
                    (v0 > i128::from(hi)).then_some(0)
                } else {
                    Some(to_ticks(div_floor(
                        (i128::from(hi) - v0 + 1) * U - c - 1,
                        r,
                    )))
                }
            }
            (Bound::Upper(hi), _) => {
                // v0 - floor((c + r dt)/U) <= hi  <=>  c + r dt >= (v0 - hi) U
                if r >= 0 {
                    (v0 > i128::from(hi)).then_some(0)
                } else {
                    Some(to_ticks(div_floor(c + (i128::from(hi) - v0) * U, -r)))
                }
            }
        };
    }
    let (alpha, beta) = envelope(v0, bound, terms);
    match bound {
        Bound::Lower(lo) => {
            let lo = i128::from(lo) * U;
            if beta >= 0 {
                (alpha + beta < lo).then_some(0)
            } else {
                Some(to_ticks(div_floor(alpha - lo, -beta)))
            }
        }
        Bound::Upper(hi) => {
            let hi = i128::from(hi) * U;
            if beta <= 0 {
                (alpha + beta > hi).then_some(0)
            } else {
                Some(to_ticks(div_floor(hi - alpha, beta)))
            }
        }
    }
}

/// The safe-side linear envelope of `U × (v0 + Σ a·emit(dt))` as (alpha, beta): `alpha + beta·dt`
/// is a lower bound for `Bound::Lower`, an upper bound for `Bound::Upper`.
fn envelope(v0: i128, bound: Bound, terms: &[Tm]) -> (i128, i128) {
    let mut alpha = v0 * U;
    let mut beta = 0i128;
    for t in terms {
        let a = i128::from(t.a);
        alpha += a * i128::from(t.carry);
        beta += a * i128::from(t.rate);
        // a·floor(x) >= a·x - a for a > 0; a·floor(x) < a·x + |a| for a < 0.
        match bound {
            Bound::Lower(_) if a > 0 => alpha -= a * U,
            Bound::Upper(_) if a < 0 => alpha -= a * U,
            _ => {}
        }
    }
    (alpha, beta)
}

/// When a constraint that is too tight now could hold for `h` ticks with an extra set of terms
/// added: `base` is the envelope without them (moving at its own slope until then), `extra` the
/// terms that join at that moment. `None` if the base does not move in the relieving direction.
fn relief_time(v0: i64, bound: Bound, base: &[Tm], extra: &[Tm], h: u64) -> Option<u64> {
    let v0 = i128::from(v0);
    let (a0, b0) = envelope(v0, bound, base);
    let mut all = base.to_vec();
    all.extend_from_slice(extra);
    let (aw, bw) = envelope(v0, bound, &all);
    let h = i128::from(h.max(1));
    match bound {
        Bound::Lower(lo) => {
            if b0 <= 0 {
                return None;
            }
            let lo = i128::from(lo) * U;
            // aw + b0·τ + bw·h* >= lo, with h* the worst case of h in 1..=h.
            let worst = if bw < 0 { bw * h } else { bw };
            let _ = a0;
            Some(to_ticks(div_ceil(lo - aw - worst, b0)).max(1))
        }
        Bound::Upper(hi) => {
            if b0 >= 0 {
                return None;
            }
            let hi = i128::from(hi) * U;
            let worst = if bw > 0 { bw * h } else { bw };
            Some(to_ticks(div_ceil(aw + worst - hi, -b0)).max(1))
        }
    }
}

impl Cluster {
    pub fn new(restart_min_run: u64) -> Self {
        Self {
            pools: Vec::new(),
            procs: Vec::new(),
            spaces: Vec::new(),
            season_boundary: None,
            restart_min_run,
            events: Vec::new(),
            departed: (0, 0),
            settles: 0,
        }
    }

    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }

    pub fn procs(&self) -> &[Proc] {
        &self.procs
    }

    pub fn is_empty(&self) -> bool {
        self.procs.is_empty() && self.pools.is_empty()
    }

    /// The districts with processes or pools here.
    pub fn members(&self) -> BTreeSet<DistrictId> {
        let mut s: BTreeSet<DistrictId> = self.procs.iter().map(|p| p.owner).collect();
        s.extend(self.pools.iter().map(|p| p.key.district()));
        s
    }

    fn pool_index(&self, key: PoolKey) -> Option<u16> {
        self.pools
            .binary_search_by_key(&key, |p| p.key)
            .ok()
            .map(|i| i as u16)
    }

    pub fn pool(&self, key: PoolKey) -> Option<&Pool> {
        self.pool_index(key).map(|i| &self.pools[i as usize])
    }

    /// A pool's level. A `Stock` key also counts the district's `Store` pools of that good (a
    /// glass replay books per store): what an event sees as the district's stock.
    pub fn level(&self, key: PoolKey) -> Option<i64> {
        let own = self.pool(key).map(|p| p.level);
        match key {
            PoolKey::Stock(d, g) => {
                let mut stores = self.store_pools(d, g).peekable();
                if stores.peek().is_none() {
                    return own;
                }
                Some(own.unwrap_or(0) + stores.map(|i| self.pools[i].level).sum::<i64>())
            }
            _ => own,
        }
    }

    /// Indices of a district's `Store` pools of good `g`, in key order.
    fn store_pools(&self, d: DistrictId, g: Good) -> impl Iterator<Item = usize> + '_ {
        let lo = self
            .pools
            .partition_point(|p| p.key < PoolKey::Store(d, 0, Good(0)));
        self.pools[lo..]
            .iter()
            .take_while(move |p| matches!(p.key, PoolKey::Store(o, _, _) if o == d))
            .enumerate()
            .filter(move |(_, p)| p.key.good() == g)
            .map(move |(i, _)| lo + i)
    }

    /// A district's box processes together: running if any runs, else the first one's stop.
    /// `None` when it has none here.
    pub fn box_run(&self, owner: DistrictId) -> Option<Run> {
        let mut first = None;
        for p in self
            .procs
            .iter()
            .filter(|p| p.owner == owner && matches!(p.body, Body::Box { .. }))
        {
            if p.run == Run::Running {
                return Some(Run::Running);
            }
            first.get_or_insert(p.run);
        }
        first
    }

    pub fn run_of(&self, owner: DistrictId, id: u16) -> Option<Run> {
        self.procs
            .iter()
            .find(|p| p.owner == owner && p.id == id)
            .map(|p| p.run)
    }

    pub fn take_events(&mut self) -> Vec<ClusterEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn set_season_boundary(&mut self, at: Option<Tick>) {
        self.season_boundary = at;
    }

    pub fn heap_bytes(&self) -> usize {
        let procs: usize = self
            .procs
            .iter()
            .map(|p| match &p.body {
                Body::Box { terms } => terms.capacity() * std::mem::size_of::<Term>(),
                Body::Link { .. } => 0,
                Body::Cycle(c) => {
                    std::mem::size_of::<CycleBody>()
                        + c.pools.capacity() * (2 + std::mem::size_of::<Diffuser>())
                        + c.pieces
                            .iter()
                            .map(|p| {
                                std::mem::size_of::<CyclePiece>()
                                    + p.rates.capacity() * 8
                                    + p.fires.capacity() * std::mem::size_of::<CycleFire>()
                            })
                            .sum::<usize>()
                }
            } + p.targets.capacity() * std::mem::size_of::<Target>())
            .sum();
        self.pools.capacity() * std::mem::size_of::<Pool>()
            + self.procs.capacity() * std::mem::size_of::<Proc>()
            + procs
            + self.spaces.capacity() * std::mem::size_of::<Space>()
    }

    // ---- building -------------------------------------------------------------------------

    /// Inserts a pool, keeping indices sorted; returns its index. Existing pools keep their level.
    fn ensure_pool(&mut self, key: PoolKey, level: i64, cap: Option<i64>) -> u16 {
        match self.pools.binary_search_by_key(&key, |p| p.key) {
            Ok(i) => {
                if cap.is_some() {
                    self.pools[i].cap = cap;
                }
                i as u16
            }
            Err(i) => {
                let i16_ = i as u16;
                self.pools.insert(
                    i,
                    Pool {
                        key,
                        level,
                        cap,
                        pushed: level,
                        books: Books {
                            initial: level,
                            ..Books::default()
                        },
                    },
                );
                self.shift_pool_refs(i16_, 1);
                i16_
            }
        }
    }

    /// Every pool reference at or above `from` moves by `by` (an insert or removal).
    fn shift_pool_refs(&mut self, from: u16, by: i32) {
        let fix = |p: &mut u16| {
            if *p >= from {
                *p = (i32::from(*p) + by) as u16;
            }
        };
        for p in &mut self.procs {
            match &mut p.body {
                Body::Box { terms } => terms.iter_mut().for_each(|t| fix(&mut t.pool)),
                Body::Link { from, to, .. } => {
                    fix(from);
                    fix(to);
                }
                Body::Cycle(c) => c.pools.iter_mut().for_each(&fix),
            }
            p.targets.iter_mut().for_each(|t| fix(&mut t.pool));
        }
        for s in &mut self.spaces {
            s.weights.iter_mut().for_each(|w| fix(&mut w.0));
        }
    }

    /// Adds a district. Its processes start stopped and run once [`Cluster::evaluate`] admits them.
    pub fn add_member(&mut self, spec: MemberSpec, at: Tick) {
        debug_assert!(
            !self.procs.iter().any(|p| p.owner == spec.district),
            "district {:?} added twice",
            spec.district
        );
        for &(k, lvl, cap) in &spec.pools {
            self.ensure_pool(k, lvl, cap);
        }
        let none = BTreeMap::new();
        for ps in spec.procs {
            self.insert_proc(spec.district, ps, &none);
        }
        if let Some(sp) = spec.space {
            self.insert_space(spec.district, 0, sp);
        }
        for (tag, sp) in spec.bounds {
            debug_assert!(tag != 0, "tag 0 is the black box's space");
            self.insert_space(spec.district, tag, sp);
        }
        self.evaluate(at);
    }

    /// Builds a process from its spec and inserts it in priority order. A diffuser on a pool in
    /// `carries` starts from that carry (a glass replay's regime change keeps the fraction).
    fn insert_proc(&mut self, owner: DistrictId, ps: ProcSpec, carries: &BTreeMap<PoolKey, u32>) {
        let carried = |k: PoolKey, f: Diffuser| match carries.get(&k) {
            Some(&c) => Diffuser::with_carry(f.rate_fixed(), c),
            None => f,
        };
        // Every pool first: an insert moves the indices after it.
        let mut keys: Vec<PoolKey> = ps.targets.iter().map(|t| t.0).collect();
        match &ps.body {
            ProcSpecBody::Box(terms) => keys.extend(terms.iter().map(|t| t.0)),
            ProcSpecBody::Link { from, to, .. } => keys.extend([*from, *to]),
            ProcSpecBody::Cycle { pools, .. } => keys.extend(pools.iter().copied()),
        }
        for k in keys {
            self.ensure_pool(k, 0, None);
        }
        let ix = |me: &Cluster, k: PoolKey| me.pool_index(k).expect("ensured");
        let body = match ps.body {
            ProcSpecBody::Box(terms) => Body::Box {
                terms: terms
                    .into_iter()
                    .map(|(k, flow)| Term {
                        pool: ix(self, k),
                        flow: carried(k, flow),
                    })
                    .collect(),
            },
            ProcSpecBody::Link {
                from,
                to,
                flow,
                batch,
            } => Body::Link {
                from: ix(self, from),
                to: ix(self, to),
                flow,
                batch,
            },
            ProcSpecBody::Cycle { pools, pieces } => {
                let idx = pools
                    .iter()
                    .map(|&k| self.pool_index(k).expect("ensured"))
                    .collect();
                let flows = pools
                    .iter()
                    .enumerate()
                    .map(|(j, &k)| {
                        let first = pieces.first();
                        let rate = first.and_then(|p| p.rates.get(j)).copied().unwrap_or(0);
                        Diffuser::with_carry(rate, carries.get(&k).copied().unwrap_or(0))
                    })
                    .collect();
                Body::Cycle(Box::new(CycleBody {
                    pools: idx,
                    pieces,
                    piece: 0,
                    off: 0,
                    flows,
                }))
            }
        };
        let targets = ps
            .targets
            .iter()
            .map(|&(k, stop_at, restart_at)| Target {
                pool: ix(self, k),
                stop_at,
                restart_at,
            })
            .collect();
        let proc_ = Proc {
            owner,
            id: ps.id,
            body,
            targets,
            requires: ps.requires,
            enabled: ps.enabled,
            run: Run::Stopped(if ps.enabled {
                Stop::Waiting
            } else {
                Stop::Disabled
            }),
            latency: ps.latency,
            armed: None,
            ran: false,
            deadline: ps.deadline,
            min_run: ps.min_run,
        };
        let pos = self
            .procs
            .binary_search_by_key(&(proc_.owner, proc_.id), |p| (p.owner, p.id))
            .unwrap_or_else(|e| e);
        self.procs.insert(pos, proc_);
    }

    fn insert_space(&mut self, owner: DistrictId, tag: u32, (weights, fixed, cap): SpaceSpec) {
        for &(k, _) in &weights {
            self.ensure_pool(k, 0, None);
        }
        let weights = weights
            .into_iter()
            .map(|(k, w)| (self.pool_index(k).expect("ensured"), w))
            .collect();
        let s = Space {
            owner,
            tag,
            weights,
            fixed,
            cap,
        };
        let pos = self
            .spaces
            .binary_search_by_key(&(s.owner, s.tag), |x| (x.owner, x.tag))
            .unwrap_or_else(|e| e);
        self.spaces.insert(pos, s);
    }

    /// A glass replay's breakpoint time shift ([`shift_back`], choice G2-12): district `d`'s
    /// running regime stopped because pool `key` would cross next tick (`rising`: a unit up);
    /// every flow of the regime moves back by the same fraction of a tick, so `key`'s carry lands
    /// on [`fire_canon`]. The units this moves are booked like the regime's own. Returns the
    /// carry, or None (nothing changed) if `key` is not at a crossing, or a pool would go below 0.
    /// Callers settle the cluster to the moment first.
    pub fn shift_back(&mut self, d: DistrictId, key: PoolKey, rising: bool) -> Option<u32> {
        let ix = self.pool_index(key)?;
        let i = self
            .procs
            .iter()
            .position(|p| p.owner == d && !matches!(p.body, Body::Link { .. }))?;
        let (pools, mut c, rates): (Vec<u16>, Vec<i128>, Vec<i64>) = match &self.procs[i].body {
            Body::Box { terms } => (
                terms.iter().map(|t| t.pool).collect(),
                terms.iter().map(|t| i128::from(t.flow.carry())).collect(),
                terms.iter().map(|t| t.flow.rate_fixed()).collect(),
            ),
            Body::Cycle(cy) => (
                cy.pools.clone(),
                cy.flows.iter().map(|f| i128::from(f.carry())).collect(),
                cy.flows.iter().map(|f| f.rate_fixed()).collect(),
            ),
            Body::Link { .. } => return None,
        };
        let f = pools.iter().position(|&p| p == ix)?;
        let canon = fire_canon(rates[f], rising)?;
        if !shift_back(&mut c, &rates, f, canon) {
            return None;
        }
        let units: Vec<i64> = c
            .iter()
            .map(|x| i64::try_from(x.div_euclid(U)).unwrap_or(0))
            .collect();
        if pools
            .iter()
            .zip(&units)
            .any(|(&p, &u)| self.pools[p as usize].level + u < 0)
        {
            return None;
        }
        for (&p, &u) in pools.iter().zip(&units) {
            let pool = &mut self.pools[p as usize];
            pool.level += u;
            if u >= 0 {
                pool.books.minted += u;
            } else {
                pool.books.burned -= u;
            }
        }
        let carry = |j: usize| c[j].rem_euclid(U) as u32;
        match &mut self.procs[i].body {
            Body::Box { terms } => {
                for (j, t) in terms.iter_mut().enumerate() {
                    t.flow = Diffuser::with_carry(t.flow.rate_fixed(), carry(j));
                }
            }
            Body::Cycle(cy) => {
                for (j, fl) in cy.flows.iter_mut().enumerate() {
                    *fl = Diffuser::with_carry(fl.rate_fixed(), carry(j));
                }
            }
            Body::Link { .. } => {}
        }
        Some(canon)
    }

    /// Per pool of district `d`'s processes (not links): its diffuser's carry.
    pub fn carries_of(&self, d: DistrictId) -> BTreeMap<PoolKey, u32> {
        let mut carries: BTreeMap<PoolKey, u32> = BTreeMap::new();
        for p in self.procs.iter().filter(|p| p.owner == d) {
            match &p.body {
                Body::Box { terms } => {
                    for t in terms {
                        carries.insert(self.pools[t.pool as usize].key, t.flow.carry());
                    }
                }
                Body::Cycle(c) => {
                    for (j, &k) in c.pools.iter().enumerate() {
                        carries.insert(self.pools[k as usize].key, c.flows[j].carry());
                    }
                }
                Body::Link { .. } => {}
            }
        }
        carries
    }

    /// Replaces a district's processes and bounds in place, keeping its pools (a glass replay's
    /// regime change, GLASSBOX.md §4.1). Each new diffuser keeps the carry its pool had in the
    /// old processes; new pools start at 0. Callers settle the cluster to `at` first.
    pub fn replace_procs(
        &mut self,
        d: DistrictId,
        procs: Vec<ProcSpec>,
        bounds: Vec<(u32, SpaceSpec)>,
        at: Tick,
    ) {
        let carries = self.carries_of(d);
        let mut procs = procs;
        // A pool the new regime does not move keeps its carry at rate 0 (a stopped machine's
        // part-used input or part-made output), so a stop and restart lose nothing.
        let used: BTreeSet<PoolKey> = procs
            .iter()
            .flat_map(|ps| match &ps.body {
                ProcSpecBody::Box(terms) => terms.iter().map(|t| t.0).collect::<Vec<_>>(),
                ProcSpecBody::Cycle { pools, .. } => pools.clone(),
                ProcSpecBody::Link { .. } => Vec::new(),
            })
            .collect();
        let parked: Vec<PoolKey> = carries
            .iter()
            .filter(|&(k, &c)| c != 0 && !used.contains(k))
            .map(|(&k, _)| k)
            .collect();
        if let Some(ps) = procs
            .iter_mut()
            .find(|ps| !matches!(ps.body, ProcSpecBody::Link { .. }))
        {
            match &mut ps.body {
                ProcSpecBody::Box(terms) => {
                    terms.extend(parked.iter().map(|&k| (k, Diffuser::from_fixed(0))));
                }
                ProcSpecBody::Cycle { pools, pieces } => {
                    pools.extend(parked.iter().copied());
                    for p in pieces.iter_mut() {
                        p.rates.resize(pools.len(), 0);
                    }
                }
                ProcSpecBody::Link { .. } => {}
            }
        }
        self.procs.retain(|p| p.owner != d);
        self.spaces.retain(|s| s.owner != d);
        for ps in procs {
            self.insert_proc(d, ps, &carries);
        }
        for (tag, sp) in bounds {
            self.insert_space(d, tag, sp);
        }
        self.evaluate(at);
    }

    /// Takes a district out: its processes, its space, and the pools it owns. Callers settle the
    /// cluster to the moment first (an event).
    pub fn remove_member(&mut self, d: DistrictId, at: Tick) -> MemberOut {
        self.procs.retain(|p| p.owner != d);
        self.spaces.retain(|s| s.owner != d);
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.pools.len() {
            if self.pools[i].key.district() == d {
                out.push(self.pools.remove(i));
                self.shift_pool_refs(i as u16 + 1, -1);
            } else {
                i += 1;
            }
        }
        for p in &out {
            self.departed.0 += p.books.moved_in;
            self.departed.1 += p.books.moved_out;
        }
        self.evaluate(at);
        MemberOut {
            district: d,
            pools: out,
        }
    }

    /// Merges another cluster settled to the same moment into this one.
    pub fn absorb(&mut self, other: Cluster, at: Tick) {
        for p in &other.pools {
            match self.pools.binary_search_by_key(&p.key, |q| q.key) {
                Ok(_) => panic!("pool {:?} in two clusters", p.key),
                Err(_) => {
                    self.ensure_pool(p.key, p.level, p.cap);
                    let i = self.pool_index(p.key).expect("just inserted") as usize;
                    self.pools[i].pushed = p.pushed;
                    self.pools[i].books = p.books;
                }
            }
        }
        let remap = |o: u16, me: &Cluster| me.pool_index(other.pools[o as usize].key).unwrap();
        for mut p in other.procs.clone() {
            match &mut p.body {
                Body::Box { terms } => terms.iter_mut().for_each(|t| t.pool = remap(t.pool, self)),
                Body::Link { from, to, .. } => {
                    *from = remap(*from, self);
                    *to = remap(*to, self);
                }
                Body::Cycle(c) => c.pools.iter_mut().for_each(|k| *k = remap(*k, self)),
            }
            for t in &mut p.targets {
                t.pool = remap(t.pool, self);
            }
            let pos = self
                .procs
                .binary_search_by_key(&(p.owner, p.id), |q| (q.owner, q.id))
                .unwrap_or_else(|e| e);
            self.procs.insert(pos, p);
        }
        for mut s in other.spaces.clone() {
            for w in &mut s.weights {
                w.0 = remap(w.0, self);
            }
            self.spaces.push(s);
        }
        self.spaces.sort_by_key(|s| (s.owner, s.tag));
        self.events.extend(other.events);
        self.departed.0 += other.departed.0;
        self.departed.1 += other.departed.1;
        self.season_boundary = match (self.season_boundary, other.season_boundary) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.evaluate(at);
    }

    // ---- events ---------------------------------------------------------------------------

    /// An import (positive) or export (negative) at an event, creating the pool if needed. A
    /// `physical` change is one the world already made (a package a train fetched), so it is not
    /// pushed again. Returns the amount applied: an export never takes a
    /// pool below 0.
    ///
    /// On a district with `Store` pools (a glass replay), an export from its `Stock` key takes
    /// what the `Stock` pool holds, then from the stores in key order; an import goes to the
    /// `Stock` pool, which the replay's processes do not read: it waits for the next reconcile
    /// (choice G2-14).
    pub fn add_to_pool(&mut self, key: PoolKey, delta: i64, physical: bool, at: Tick) -> i64 {
        let i = self.ensure_pool(key, 0, None) as usize;
        let p = &mut self.pools[i];
        let mut applied = if delta < 0 {
            delta.max(-p.level)
        } else {
            delta
        };
        p.level += applied;
        p.books.events += applied;
        if physical {
            p.pushed += applied;
        }
        if let PoolKey::Stock(d, g) = key
            && applied > delta
        {
            let stores: Vec<usize> = self.store_pools(d, g).collect();
            for j in stores {
                let want = delta - applied;
                if want >= 0 {
                    break;
                }
                let p = &mut self.pools[j];
                let take = want.max(-p.level);
                p.level += take;
                p.books.events += take;
                if physical {
                    p.pushed += take;
                }
                applied += take;
            }
        }
        self.evaluate(at);
        applied
    }

    /// A change the world already made (a package written into or taken from an offline buffer
    /// while a glass replay runs): the level and its start move together, so the reconcile,
    /// which writes `level - initial`, does not write it again. A take is limited to the level.
    /// Returns what was applied.
    pub fn rebase(&mut self, key: PoolKey, delta: i64, at: Tick) -> i64 {
        let i = self.ensure_pool(key, 0, None) as usize;
        let p = &mut self.pools[i];
        let applied = delta.max(-p.level);
        p.level += applied;
        p.books.initial += applied;
        p.pushed += applied;
        self.evaluate(at);
        applied
    }

    /// What a stock pool gained since it was last pushed to the world, now marked as pushed (I14:
    /// level pollution applied live while unloaded). 0 for a pool that does not exist.
    pub fn take_unpushed(&mut self, key: PoolKey) -> i64 {
        let Some(i) = self.pool_index(key) else {
            return 0;
        };
        let p = &mut self.pools[i as usize];
        let d = p.level - p.pushed;
        p.pushed = p.level;
        d
    }

    /// Enables or disables a district's processes (frozen, damaged, live, out of season).
    pub fn set_enabled(&mut self, d: DistrictId, enabled: bool, at: Tick) {
        for p in &mut self.procs {
            if p.owner == d {
                p.enabled = enabled;
                if !enabled && p.run == Run::Running {
                    self.events.push(ClusterEvent::Stopped {
                        at,
                        owner: d,
                        id: p.id,
                        why: Stop::Disabled,
                    });
                }
                if !enabled {
                    p.run = Run::Stopped(Stop::Disabled);
                } else if p.run == Run::Stopped(Stop::Disabled) {
                    p.run = Run::Stopped(Stop::Waiting);
                }
            }
        }
        self.evaluate(at);
    }

    /// Changes what a district's box processes require (a PowerGrid source starting or stopping
    /// its replay).
    pub fn set_requires(&mut self, owner: DistrictId, requires: Option<DistrictId>, at: Tick) {
        for p in &mut self.procs {
            if p.owner == owner && matches!(p.body, Body::Box { .. }) {
                p.requires = requires;
            }
        }
        self.evaluate(at);
    }

    // ---- the running set ------------------------------------------------------------------

    fn is_running(&self, i: usize) -> bool {
        self.procs[i].run == Run::Running
    }

    fn requirement_met(&self, i: usize, running: &dyn Fn(usize) -> bool) -> bool {
        match self.procs[i].requires {
            None => true,
            Some(d) => {
                let mut any = false;
                for (j, p) in self.procs.iter().enumerate() {
                    if p.owner == d && matches!(p.body, Body::Box { .. }) {
                        if !running(j) {
                            return false;
                        }
                        any = true;
                    }
                }
                any
            }
        }
    }

    /// Terms on pool `k` from processes `running` admits, scaled by `coef`.
    fn pool_terms(&self, k: u16, coef: i64, running: &dyn Fn(usize) -> bool, out: &mut Vec<Tm>) {
        for (i, p) in self.procs.iter().enumerate() {
            if !running(i) {
                continue;
            }
            self.proc_terms_on(p, k, coef, out);
        }
    }

    fn proc_terms_on(&self, p: &Proc, k: u16, coef: i64, out: &mut Vec<Tm>) {
        match &p.body {
            Body::Box { terms } => {
                for t in terms.iter().filter(|t| t.pool == k) {
                    out.push(Tm {
                        a: coef,
                        rate: t.flow.rate_fixed(),
                        carry: t.flow.carry(),
                    });
                }
            }
            Body::Link { from, to, flow, .. } if from != to => {
                if *from == k {
                    out.push(Tm {
                        a: -coef,
                        rate: flow.rate_fixed(),
                        carry: flow.carry(),
                    });
                }
                if *to == k {
                    out.push(Tm {
                        a: coef,
                        rate: flow.rate_fixed(),
                        carry: flow.carry(),
                    });
                }
            }
            Body::Link { .. } => {}
            // The engine bounds a cycle by its deadline; no constraint sees it.
            Body::Cycle(_) => {}
        }
    }

    /// The constraint's current value, bound and terms under `running`.
    fn con_terms(
        &self,
        c: Con,
        running: &dyn Fn(usize) -> bool,
        out: &mut Vec<Tm>,
    ) -> (i64, Bound) {
        out.clear();
        match c {
            Con::Lower(k) => {
                self.pool_terms(k, 1, running, out);
                (self.pools[k as usize].level, Bound::Lower(0))
            }
            Con::Upper(k) => {
                self.pool_terms(k, 1, running, out);
                let p = &self.pools[k as usize];
                (p.level, Bound::Upper(p.cap.unwrap_or(i64::MAX)))
            }
            Con::Target(pi, ti) => {
                let t = self.procs[pi].targets[ti];
                self.pool_terms(t.pool, 1, running, out);
                (self.pools[t.pool as usize].level, Bound::Upper(t.stop_at))
            }
            Con::Space(si) => {
                let s = &self.spaces[si];
                let mut v = s.fixed;
                for &(k, w) in &s.weights {
                    v = v.saturating_add(w.saturating_mul(self.pools[k as usize].level));
                    self.pool_terms(k, w, running, out);
                }
                (v, Bound::Upper(s.cap))
            }
            Con::Batch(li) => {
                let Body::Link { to, batch, .. } = &self.procs[li].body else {
                    unreachable!("batch constraint on a box")
                };
                let to = *to;
                let b = batch.expect("batch link");
                self.pool_terms(to, 1, running, out);
                let p = &self.pools[to as usize];
                (p.level, Bound::Upper(p.pushed_units_next(b) - 1))
            }
        }
    }

    fn constraints(&self) -> Vec<Con> {
        let mut v = Vec::new();
        for k in 0..self.pools.len() as u16 {
            v.push(Con::Lower(k));
            if self.pools[k as usize].cap.is_some() {
                v.push(Con::Upper(k));
            }
        }
        for (pi, p) in self.procs.iter().enumerate() {
            for ti in 0..p.targets.len() {
                v.push(Con::Target(pi, ti));
            }
        }
        for si in 0..self.spaces.len() {
            v.push(Con::Space(si));
        }
        v
    }

    /// Does process `i` push constraint `c` toward its bound (and so get stopped by it)?
    fn blamed(&self, c: Con, i: usize) -> bool {
        let p = &self.procs[i];
        let mut ts = Vec::new();
        match c {
            Con::Lower(k) => {
                self.proc_terms_on(p, k, 1, &mut ts);
                ts.iter().any(|t| i128::from(t.a) * i128::from(t.rate) < 0)
            }
            Con::Upper(k) => {
                self.proc_terms_on(p, k, 1, &mut ts);
                ts.iter().any(|t| i128::from(t.a) * i128::from(t.rate) > 0)
            }
            Con::Target(pi, _) => pi == i,
            Con::Space(si) => {
                let s = &self.spaces[si];
                for &(k, w) in &s.weights {
                    self.proc_terms_on(p, k, w, &mut ts);
                }
                ts.iter()
                    .map(|t| i128::from(t.a) * i128::from(t.rate))
                    .sum::<i128>()
                    > 0
            }
            Con::Batch(_) => false,
        }
    }

    fn stop_reason(&self, c: Con) -> Stop {
        match c {
            Con::Lower(k) => Stop::InputEmpty(self.pools[k as usize].key),
            Con::Upper(k) => Stop::OutputFull(self.pools[k as usize].key),
            Con::Target(pi, ti) => {
                Stop::Target(self.pools[self.procs[pi].targets[ti].pool as usize].key)
            }
            Con::Space(si) => {
                let s = &self.spaces[si];
                if s.tag == 0 {
                    Stop::SpaceFull(s.owner)
                } else {
                    Stop::Bound(s.owner, s.tag)
                }
            }
            Con::Batch(_) => Stop::Waiting,
        }
    }

    fn set_run(&mut self, i: usize, run: Run, at: Tick) {
        let p = &mut self.procs[i];
        if p.run == run {
            return;
        }
        p.run = run;
        p.armed = None;
        if run == Run::Running {
            p.ran = true;
        }
        self.events.push(match run {
            Run::Running => ClusterEvent::Started {
                at,
                owner: p.owner,
                id: p.id,
            },
            Run::Stopped(why) => ClusterEvent::Stopped {
                at,
                owner: p.owner,
                id: p.id,
                why,
            },
        });
    }

    /// Fixes the running set at `at`: stops whatever would break a constraint within the next
    /// tick (lowest priority first), then restarts, in priority order, whatever can run for at
    /// least `restart_min_run_ticks`. Deterministic: it reads only the state.
    pub fn evaluate(&mut self, at: Tick) {
        self.settles += 1;
        let cons = self.constraints();
        let mut ts = Vec::new();
        // Deadlines first: the engine replaces what they stop.
        for i in 0..self.procs.len() {
            if self.is_running(i) && self.procs[i].deadline.is_some_and(|t| t <= at) {
                self.set_run(i, Run::Stopped(Stop::Timer), at);
            }
        }
        // Stops, to a fixed point.
        'stops: loop {
            for i in 0..self.procs.len() {
                if self.is_running(i) && !self.requirement_met(i, &|j| self.is_running(j)) {
                    let src = self.procs[i].requires.expect("requirement");
                    self.set_run(i, Run::Stopped(Stop::SourceStopped(src)), at);
                    continue 'stops;
                }
            }
            for &c in &cons {
                let running = |j: usize| self.is_running(j);
                let (v0, bound) = self.con_terms(c, &running, &mut ts);
                if safe_run(v0, bound, &ts) != Some(0) {
                    continue;
                }
                if let Some(victim) = (0..self.procs.len())
                    .rev()
                    .find(|&j| self.is_running(j) && self.blamed(c, j))
                {
                    let why = self.stop_reason(c);
                    self.set_run(victim, Run::Stopped(why), at);
                    continue 'stops;
                }
            }
            break;
        }
        // Restarts, one at a time in priority order.
        loop {
            let mut started = false;
            // A process that never ran says what holds it back (it keeps its first-start right).
            for i in 0..self.procs.len() {
                if self.procs[i].run == Run::Stopped(Stop::Waiting)
                    && !self.procs[i].ran
                    && let Some(why) = self.blocking(i, &cons)
                {
                    self.procs[i].run = Run::Stopped(why);
                }
            }
            for i in 0..self.procs.len() {
                let p = &self.procs[i];
                if !p.enabled || p.run == Run::Running || p.deadline.is_some_and(|t| t <= at) {
                    continue;
                }
                if !self.requirement_met(i, &|j| self.is_running(j)) {
                    self.procs[i].armed = None;
                    continue;
                }
                if !self.can_restart(i, &cons) {
                    self.procs[i].armed = None;
                    continue;
                }
                // A first start (the district was running when it unloaded) is immediate; a
                // restart after a stop waits for the machine's next cycle.
                let first = !p.ran || p.latency == 0;
                let go = first || self.procs[i].armed.is_some_and(|t| at >= t);
                if go {
                    self.set_run(i, Run::Running, at);
                    started = true;
                    break;
                }
                if self.procs[i].armed.is_none() {
                    self.procs[i].armed = Some(at + self.procs[i].latency);
                }
            }
            if !started {
                break;
            }
        }
    }

    /// The first constraint that keeps stopped process `i` from starting, as a reason.
    /// The least run a restart of process `i` must allow.
    fn min_run_of(&self, i: usize) -> u64 {
        match self.procs[i].min_run {
            0 => self.restart_min_run.max(1),
            n => n,
        }
    }

    fn blocking(&self, i: usize, cons: &[Con]) -> Option<Stop> {
        let h = self.min_run_of(i);
        if !self.requirement_met(i, &|j| self.is_running(j)) {
            return self.procs[i].requires.map(Stop::SourceStopped);
        }
        let mut ts = Vec::new();
        let with = |j: usize| j == i || self.is_running(j);
        for &c in cons {
            if !self.blamed(c, i) {
                continue;
            }
            let (v0, bound) = self.con_terms(c, &with, &mut ts);
            if let Some(n) = safe_run(v0, bound, &ts)
                && n < h
            {
                return Some(self.stop_reason(c));
            }
        }
        None
    }

    /// Could process `i` start now and run at least `restart_min_run_ticks`?
    fn can_restart(&self, i: usize, cons: &[Con]) -> bool {
        let h = self.min_run_of(i);
        let p = &self.procs[i];
        // A target stop restarts only at or below its restart level.
        if let Run::Stopped(Stop::Target(k)) = p.run
            && let Some(t) = p
                .targets
                .iter()
                .find(|t| self.pools[t.pool as usize].key == k)
            && self.pools[t.pool as usize].level > t.restart_at
        {
            return false;
        }
        let mut ts = Vec::new();
        let with = |j: usize| j == i || self.is_running(j);
        for &c in cons {
            if !self.blamed(c, i) {
                continue;
            }
            let (v0, bound) = self.con_terms(c, &with, &mut ts);
            if let Some(n) = safe_run(v0, bound, &ts)
                && n < h
            {
                return false;
            }
        }
        true
    }

    /// When stopped process `i` could restart, if anything running moves its blocking
    /// constraints the right way.
    fn restart_time(&self, i: usize, cons: &[Con]) -> Option<u64> {
        let h = self.min_run_of(i);
        let p = &self.procs[i];
        let mut worst: u64 = 0;
        let running = |j: usize| self.is_running(j);
        let with = |j: usize| j == i || self.is_running(j);
        let mut ts = Vec::new();
        if let Run::Stopped(Stop::Target(k)) = p.run
            && let Some(t) = p
                .targets
                .iter()
                .find(|t| self.pools[t.pool as usize].key == k)
            && self.pools[t.pool as usize].level > t.restart_at
        {
            // When the others bring the pool down to the restart level.
            self.pool_terms(t.pool, 1, &running, &mut ts);
            let lvl = self.pools[t.pool as usize].level;
            worst = worst.max(relief_time(lvl, Bound::Upper(t.restart_at), &ts, &[], 1)?);
        }
        for &c in cons {
            if !self.blamed(c, i) {
                continue;
            }
            let (v0, bound) = self.con_terms(c, &with, &mut ts);
            match safe_run(v0, bound, &ts) {
                Some(n) if n < h => {
                    let mut base = Vec::new();
                    let (v0b, _) = self.con_terms(c, &running, &mut base);
                    let mut mine = Vec::new();
                    match c {
                        Con::Space(si) => {
                            for &(k, w) in &self.spaces[si].weights {
                                self.proc_terms_on(p, k, w, &mut mine);
                            }
                        }
                        Con::Lower(k) | Con::Upper(k) => self.proc_terms_on(p, k, 1, &mut mine),
                        Con::Target(pi, ti) => {
                            self.proc_terms_on(p, self.procs[pi].targets[ti].pool, 1, &mut mine)
                        }
                        Con::Batch(_) => {}
                    }
                    worst = worst.max(relief_time(v0b, bound, &base, &mine, h)?);
                }
                _ => {}
            }
        }
        (worst > 0).then_some(worst)
    }

    fn breakpoint_after(&self, anchor: Tick) -> Option<Tick> {
        let cons = self.constraints();
        let mut best: Option<u64> = None;
        let mut keep = |n: u64| best = Some(best.map_or(n, |b: u64| b.min(n)));
        let mut ts = Vec::new();
        let running = |j: usize| self.is_running(j);
        for &c in &cons {
            if !(0..self.procs.len()).any(|j| self.is_running(j) && self.blamed(c, j)) {
                continue;
            }
            let (v0, bound) = self.con_terms(c, &running, &mut ts);
            if let Some(n) = safe_run(v0, bound, &ts) {
                // The last safe tick is where the process stops; never the anchor itself.
                keep(n.max(1));
            }
        }
        for (i, p) in self.procs.iter().enumerate() {
            if let Some(t) = p.deadline
                && p.run == Run::Running
            {
                keep(t.since(anchor).max(1));
            }
            if let Some(t) = p.armed
                && p.run != Run::Running
            {
                keep(t.since(anchor).max(1));
            }
            if p.enabled
                && p.run != Run::Running
                && p.armed.is_none()
                && !p.deadline.is_some_and(|t| t <= anchor)
                && self.requirement_met(i, &running)
                && let Some(n) = self.restart_time(i, &cons)
            {
                keep(n.saturating_add(p.latency));
            }
            if p.run == Run::Running && matches!(p.body, Body::Link { batch: Some(_), .. }) {
                let (v0, bound) = self.con_terms(Con::Batch(i), &running, &mut ts);
                if let Some(n) = safe_run(v0, bound, &ts) {
                    // The batch completes the tick after the last one below it.
                    keep(n + 1);
                }
            }
        }
        if let Some(b) = self.season_boundary
            && b > anchor
        {
            keep(b - anchor);
        }
        best.and_then(|n| anchor.0.checked_add(n.max(1)).map(Tick))
    }

    /// Package batches completed in mail pools: recorded and marked as written.
    fn collect_batches(&mut self, at: Tick) {
        for i in 0..self.procs.len() {
            if let Body::Link {
                to, batch: Some(b), ..
            } = self.procs[i].body
            {
                let p = &mut self.pools[to as usize];
                let ready = (p.level - p.pushed).div_euclid(b);
                if ready > 0 {
                    p.pushed += ready * b;
                    self.events.push(ClusterEvent::PackagesReady {
                        at,
                        pool: p.key,
                        count: ready,
                        batch: b,
                    });
                }
            }
        }
    }

    /// Every pool's level comes from its books, exactly; and transfers balance.
    pub fn check_books(&self) -> Result<(), String> {
        let (mut ins, mut outs) = (i128::from(self.departed.0), i128::from(self.departed.1));
        for p in &self.pools {
            let b = p.books;
            let expect = b.initial + b.events + b.minted - b.burned + b.moved_in - b.moved_out;
            if expect != p.level {
                return Err(format!(
                    "pool {:?}: level {} but books say {expect} ({b:?})",
                    p.key, p.level
                ));
            }
            if p.level < 0 {
                return Err(format!("pool {:?} is negative: {}", p.key, p.level));
            }
            ins += i128::from(b.moved_in);
            outs += i128::from(b.moved_out);
        }
        if ins != outs {
            return Err(format!("links moved {outs} out but {ins} in"));
        }
        Ok(())
    }
}

impl Pool {
    /// The level at which the next package of `batch` units is complete.
    fn pushed_units_next(&self, batch: i64) -> i64 {
        self.pushed.saturating_add(batch)
    }
}

impl LazyModel for Cluster {
    type View = ClusterView;

    fn slot_period(&self) -> Option<SlotPeriod> {
        None
    }

    fn settle(&mut self, from: Tick, to: Tick) {
        let dt = to - from;
        if dt > 0 {
            for i in 0..self.procs.len() {
                if self.procs[i].run != Run::Running {
                    continue;
                }
                match &mut self.procs[i].body {
                    Body::Box { terms } => {
                        for t in terms.iter_mut() {
                            let e = t.flow.emit(dt);
                            let p = &mut self.pools[t.pool as usize];
                            p.level += e;
                            if e >= 0 {
                                p.books.minted += e;
                            } else {
                                p.books.burned -= e;
                            }
                        }
                    }
                    Body::Link { from, to, flow, .. } => {
                        let e = flow.emit(dt);
                        let (f, t) = (*from as usize, *to as usize);
                        self.pools[f].level -= e;
                        self.pools[f].books.moved_out += e;
                        self.pools[t].level += e;
                        self.pools[t].books.moved_in += e;
                    }
                    Body::Cycle(c) => {
                        let mut out = vec![0i64; c.pools.len()];
                        c.advance(dt, &mut out);
                        for (j, &k) in c.pools.iter().enumerate() {
                            let e = out[j];
                            let p = &mut self.pools[k as usize];
                            p.level += e;
                            if e >= 0 {
                                p.books.minted += e;
                            } else {
                                p.books.burned -= e;
                            }
                        }
                    }
                }
            }
        }
        debug_assert!(
            self.pools.iter().all(|p| p.level >= 0),
            "a pool went negative settling {from:?}..{to:?}: {:?}",
            self.pools
                .iter()
                .filter(|p| p.level < 0)
                .map(|p| (p.key, p.level))
                .collect::<Vec<_>>()
        );
        self.collect_batches(to);
        if let Some(b) = self.season_boundary
            && to >= b
        {
            self.season_boundary = None;
            self.events.push(ClusterEvent::SeasonBoundary { at: to });
        }
        self.evaluate(to);
    }

    fn commit_slot(&mut self, _slot: u64, _rng: &mut SlotRng) {}

    fn view(&self, anchor: Tick, at: Tick) -> ClusterView {
        let dt = at.since(anchor);
        let mut levels: Vec<(PoolKey, i64)> = self.pools.iter().map(|p| (p.key, p.level)).collect();
        if dt > 0 {
            for p in &self.procs {
                if p.run != Run::Running {
                    continue;
                }
                match &p.body {
                    Body::Box { terms } => {
                        for t in terms {
                            levels[t.pool as usize].1 += t.flow.peek(dt);
                        }
                    }
                    Body::Link { from, to, flow, .. } => {
                        let e = flow.peek(dt);
                        levels[*from as usize].1 -= e;
                        levels[*to as usize].1 += e;
                    }
                    Body::Cycle(c) => {
                        let mut c = c.clone();
                        let mut out = vec![0i64; c.pools.len()];
                        c.advance(dt, &mut out);
                        for (j, &k) in c.pools.iter().enumerate() {
                            levels[k as usize].1 += out[j];
                        }
                    }
                }
            }
        }
        ClusterView {
            levels,
            runs: self.procs.iter().map(|p| (p.owner, p.id, p.run)).collect(),
        }
    }

    fn next_breakpoint(&self, anchor: Tick) -> Option<Tick> {
        self.breakpoint_after(anchor)
    }
}

impl Conserved for Cluster {
    fn add_totals(&self, out: &mut Totals) {
        for p in &self.pools {
            out.add(QuantityKind(p.key.good().0), i128::from(p.level));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    const FIXED_ONE_I: i64 = rtsim_kernel::conserve::FIXED_ONE;

    fn brute_safe(v0: i64, bound: Bound, terms: &[(i64, Diffuser)], limit: u64) -> Option<u64> {
        let mut ds: Vec<(i64, Diffuser)> = terms.to_vec();
        let mut v = v0;
        for dt in 1..=limit {
            for (a, d) in &mut ds {
                v += *a * d.emit(1);
            }
            let ok = match bound {
                Bound::Lower(lo) => v >= lo,
                Bound::Upper(hi) => v <= hi,
            };
            if !ok {
                return Some(dt - 1);
            }
        }
        None
    }

    fn tms(terms: &[(i64, Diffuser)]) -> Vec<Tm> {
        terms
            .iter()
            .map(|(a, d)| Tm {
                a: *a,
                rate: d.rate_fixed(),
                carry: d.carry(),
            })
            .collect()
    }

    proptest! {
        /// One term: the closed form equals stepping tick by tick.
        #[test]
        fn single_term_safe_run_is_exact(
            v0 in 0i64..200,
            rate in -(3i64 << 32)..(3i64 << 32),
            carry in any::<u32>(),
            a in prop::sample::select(vec![1i64, -1]),
            upper in any::<bool>(),
            bound_v in 0i64..400,
        ) {
            let d = Diffuser::with_carry(rate, carry);
            let bound = if upper { Bound::Upper(bound_v.max(v0)) } else { Bound::Lower(bound_v.min(v0)) };
            let got = safe_run(v0, bound, &tms(&[(a, d)]));
            let want = brute_safe(v0, bound, &[(a, d)], 5_000);
            match (got, want) {
                (Some(g), Some(w)) => prop_assert_eq!(g, w),
                (Some(g), None) => prop_assert!(g >= 5_000, "closed form {g}, brute none"),
                (None, w) => prop_assert!(w.is_none(), "closed form never, brute {w:?}"),
            }
        }

        /// Several terms: the envelope never claims a tick that breaks the bound.
        #[test]
        fn envelope_is_safe(
            v0 in 0i64..200,
            terms in prop::collection::vec((prop::sample::select(vec![1i64, -1, 3]), -(2i64 << 32)..(2i64 << 32), any::<u32>()), 2..5),
            upper in any::<bool>(),
            slack in 0i64..300,
        ) {
            let ds: Vec<(i64, Diffuser)> = terms.iter().map(|&(a, r, c)| (a, Diffuser::with_carry(r, c))).collect();
            let bound = if upper { Bound::Upper(v0 + slack) } else { Bound::Lower(v0 - slack) };
            if let Some(n) = safe_run(v0, bound, &tms(&ds)) {
                let n = n.min(4_000);
                let brute = brute_safe(v0, bound, &ds, n);
                prop_assert!(brute.is_none(), "envelope said {n} safe, brute broke at {brute:?}");
            }
        }
    }

    fn d(n: u64) -> DistrictId {
        DistrictId(n)
    }

    /// Ore in, ingots out: a box that stops when ore runs out.
    fn smelter(ore: i64, ingot_cap: Option<i64>) -> MemberSpec {
        let (o, i) = (PoolKey::Stock(d(1), Good(1)), PoolKey::Stock(d(1), Good(2)));
        MemberSpec {
            district: d(1),
            pools: vec![(o, ore, None), (i, 0, ingot_cap)],
            procs: vec![ProcSpec {
                id: 0,
                body: ProcSpecBody::Box(vec![
                    (o, Diffuser::per(-3, 100)),
                    (i, Diffuser::per(3, 100)),
                ]),
                targets: vec![],
                requires: None,
                latency: 0,
                enabled: true,
                deadline: None,
                min_run: 0,
            }],
            space: None,
            bounds: Vec::new(),
        }
    }

    #[test]
    fn box_runs_until_its_input_empties() {
        let mut c = Cluster::new(1);
        c.add_member(smelter(30, None), Tick(0));
        assert_eq!(c.run_of(d(1), 0), Some(Run::Running));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        // 30 ore at 3 per 100 ticks: the first unit is taken up front (error diffusion rounds
        // consumption up), so the last tick that still has ore for its draw is 999.
        assert!(bp.0 >= 990 && bp.0 <= 1000, "breakpoint {bp:?}");
        c.settle(Tick(0), bp);
        assert_eq!(
            c.run_of(d(1), 0),
            Some(Run::Stopped(Stop::InputEmpty(PoolKey::Stock(
                d(1),
                Good(1)
            ))))
        );
        assert!(c.level(PoolKey::Stock(d(1), Good(1))).unwrap() >= 0);
        c.check_books().unwrap();
        assert_eq!(
            c.next_breakpoint(bp),
            None,
            "a starved box waits for an event"
        );
    }

    #[test]
    fn import_restarts_a_starved_box() {
        let mut c = Cluster::new(1);
        c.add_member(smelter(3, None), Tick(0));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        c.settle(Tick(0), bp);
        assert_ne!(c.run_of(d(1), 0), Some(Run::Running));
        c.add_to_pool(PoolKey::Stock(d(1), Good(1)), 300, false, bp);
        assert_eq!(c.run_of(d(1), 0), Some(Run::Running));
        c.check_books().unwrap();
    }

    #[test]
    fn output_cap_stops_the_box() {
        let mut c = Cluster::new(1);
        c.add_member(smelter(10_000, Some(50)), Tick(0));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        c.settle(Tick(0), bp);
        assert_eq!(c.level(PoolKey::Stock(d(1), Good(2))), Some(50));
        assert!(matches!(
            c.run_of(d(1), 0),
            Some(Run::Stopped(Stop::OutputFull(_)))
        ));
    }

    #[test]
    fn power_coupling_stops_the_sink_with_the_source() {
        let mut c = Cluster::new(1);
        c.add_member(smelter(30, None), Tick(0));
        let (a, b) = (PoolKey::Stock(d(2), Good(3)), PoolKey::Stock(d(2), Good(4)));
        c.add_member(
            MemberSpec {
                district: d(2),
                pools: vec![(a, 1_000_000, None), (b, 0, None)],
                procs: vec![ProcSpec {
                    id: 0,
                    body: ProcSpecBody::Box(vec![
                        (a, Diffuser::per(-1, 10)),
                        (b, Diffuser::per(1, 10)),
                    ]),
                    targets: vec![],
                    requires: Some(d(1)),
                    latency: 0,
                    enabled: true,
                    deadline: None,
                    min_run: 0,
                }],
                space: None,
                bounds: Vec::new(),
            },
            Tick(0),
        );
        assert_eq!(c.run_of(d(2), 0), Some(Run::Running));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        c.settle(Tick(0), bp);
        assert_eq!(
            c.run_of(d(2), 0),
            Some(Run::Stopped(Stop::SourceStopped(d(1))))
        );
        c.add_to_pool(PoolKey::Stock(d(1), Good(1)), 300, false, bp);
        assert_eq!(
            c.run_of(d(2), 0),
            Some(Run::Running),
            "the sink restarts with its source"
        );
    }

    fn glass_proc(body: ProcSpecBody, deadline: Option<Tick>) -> ProcSpec {
        ProcSpec {
            id: 0,
            body,
            targets: vec![],
            requires: None,
            latency: 0,
            enabled: true,
            deadline,
            min_run: 1,
        }
    }

    fn piece(len: u64, rates: Vec<i64>, fires: &[(u16, u32)]) -> CyclePiece {
        CyclePiece {
            len,
            rates,
            fires: fires
                .iter()
                .map(|&(pool, canon)| CycleFire { pool, canon })
                .collect(),
        }
    }

    type PieceArgs = (u64, Vec<i64>, Vec<(u16, u32)>);

    fn cycle_of(pieces: &[PieceArgs], carries: &[u32]) -> CycleBody {
        let pieces: Vec<CyclePiece> = pieces
            .iter()
            .map(|(l, r, z)| piece(*l, r.clone(), z))
            .collect();
        CycleBody {
            pools: (0..carries.len() as u16).collect(),
            flows: pieces[0]
                .rates
                .iter()
                .zip(carries)
                .map(|(&r, &c)| Diffuser::with_carry(r, c))
                .collect(),
            pieces,
            piece: 0,
            off: 0,
        }
    }

    proptest! {
        /// A cycle's closed form over whole periods equals stepping it tick by tick, and any
        /// split of the elapsed time gives the same emissions (the composition rule, §4.5).
        #[test]
        fn cycle_closed_form_equals_stepping(
            pieces in prop::collection::vec((
                1u64..300,
                prop::collection::vec(-(3i64 << 32)..(3i64 << 32), 3),
                prop::collection::vec((0u16..3, any::<u32>()), 0..3),
            ), 1..5),
            carries in prop::collection::vec(any::<u32>(), 3),
            cuts in prop::collection::vec(1u64..2_000, 1..6),
        ) {
            let total: u64 = cuts.iter().sum();
            let mut whole = cycle_of(&pieces, &carries);
            let mut a = vec![0i64; 3];
            whole.advance(total, &mut a);
            let mut split = cycle_of(&pieces, &carries);
            let mut b = vec![0i64; 3];
            for &c in &cuts {
                split.advance(c, &mut b);
            }
            let mut ticks = cycle_of(&pieces, &carries);
            let mut t = vec![0i64; 3];
            for _ in 0..total {
                ticks.advance(1, &mut t);
            }
            prop_assert_eq!(&a, &b);
            prop_assert_eq!(&a, &t);
            prop_assert_eq!((whole.piece, whole.off), (ticks.piece, ticks.off));
            prop_assert_eq!(&whole.flows, &ticks.flows);
        }
    }

    #[test]
    fn a_cycle_replays_in_closed_form_until_its_deadline() {
        let mut c = Cluster::new(1);
        let (a, b) = (
            PoolKey::Store(d(1), 1, Good(1)),
            PoolKey::Store(d(1), 2, Good(1)),
        );
        // 10 ticks moving 1 per tick from a to b, then 30 ticks idle: 10 per 40 ticks.
        let pieces = vec![
            piece(10, vec![-FIXED_ONE_I, FIXED_ONE_I], &[]),
            piece(30, vec![0, 0], &[]),
        ];
        c.add_member(
            MemberSpec {
                district: d(1),
                pools: vec![(a, 1_000, None), (b, 0, None)],
                procs: vec![glass_proc(
                    ProcSpecBody::Cycle {
                        pools: vec![a, b],
                        pieces,
                    },
                    Some(Tick(4_000)),
                )],
                space: None,
                bounds: Vec::new(),
            },
            Tick(0),
        );
        assert_eq!(c.run_of(d(1), 0), Some(Run::Running));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        assert_eq!(
            bp,
            Tick(4_000),
            "no constraint sees a cycle; its deadline is the breakpoint"
        );
        c.settle(Tick(0), Tick(1_234));
        // 30 whole cycles (300) and 34 ticks into the next: 10 more.
        assert_eq!(c.level(b), Some(310));
        c.settle(Tick(1_234), bp);
        assert_eq!(c.level(b), Some(1_000));
        assert_eq!(c.level(a), Some(0));
        assert_eq!(c.run_of(d(1), 0), Some(Run::Stopped(Stop::Timer)));
        assert!(c.take_events().iter().any(|e| matches!(
            e,
            ClusterEvent::Stopped {
                why: Stop::Timer,
                ..
            }
        )));
        assert_eq!(
            c.next_breakpoint(bp),
            None,
            "a timed-out process never restarts"
        );
        c.check_books().unwrap();
    }

    #[test]
    fn replacing_processes_keeps_each_pools_carry() {
        let mut c = Cluster::new(1);
        let (src, out) = (
            PoolKey::Store(d(1), 1, Good(1)),
            PoolKey::Store(d(1), 2, Good(2)),
        );
        let body = || {
            ProcSpecBody::Box(vec![
                (src, Diffuser::per(-3, 8)),
                (out, Diffuser::per(3, 8)),
            ])
        };
        c.add_member(
            MemberSpec {
                district: d(1),
                pools: vec![(src, 100, None), (out, 0, None)],
                procs: vec![glass_proc(body(), None)],
                space: None,
                bounds: Vec::new(),
            },
            Tick(0),
        );
        c.settle(Tick(0), Tick(2));
        c.replace_procs(d(1), vec![glass_proc(body(), None)], Vec::new(), Tick(2));
        c.settle(Tick(2), Tick(3));
        c.check_books().unwrap();
        assert_eq!(c.level(out), Some(1), "3/4 carried, 3/8 more makes a unit");
    }

    /// A breakpoint's time shift moves every flow of the regime back by the same fraction of a
    /// tick: the fired pool lands on its canonical carry, the others keep their recipe ratio, and
    /// nothing gains.
    #[test]
    fn a_time_shift_moves_every_flow_back_together() {
        let mut c = Cluster::new(1);
        let (src, out, side) = (
            PoolKey::Store(d(1), 1, Good(1)),
            PoolKey::Store(d(1), 2, Good(2)),
            PoolKey::Store(d(1), 3, Good(3)),
        );
        // 3/8 in, 3/8 out, 3/16 of a by-product.
        c.add_member(
            MemberSpec {
                district: d(1),
                pools: vec![(src, 100, None), (out, 0, None), (side, 0, None)],
                procs: vec![glass_proc(
                    ProcSpecBody::Box(vec![
                        (src, Diffuser::per(-3, 8)),
                        (out, Diffuser::per(3, 8)),
                        (side, Diffuser::per(3, 16)),
                    ]),
                    None,
                )],
                space: None,
                bounds: Vec::new(),
            },
            Tick(0),
        );
        // At 2: out carries 6/8, within a tick (3/8) of its next unit.
        c.settle(Tick(0), Tick(2));
        let canon = c.shift_back(d(1), out, true).expect("at a crossing");
        assert_eq!(i128::from(canon), U - U * 3 / 8);
        // Shifted back by 1/3 tick: out 6/8 -> 5/8, src gets 1/8 back, side 6/16 -> 5/16.
        let carry = |k: PoolKey| match &c.procs[0].body {
            Body::Box { terms } => terms
                .iter()
                .find(|t| c.pools[t.pool as usize].key == k)
                .map(|t| t.flow.carry()),
            _ => None,
        };
        assert_eq!(carry(out), Some((U * 5 / 8) as u32));
        assert_eq!(carry(side), Some((U * 5 / 16) as u32));
        assert_eq!(c.level(src), Some(99));
        assert_eq!(carry(src), Some((U * 3 / 8) as u32));
        c.check_books().unwrap();
        // Not at a crossing: nothing moves.
        assert_eq!(c.shift_back(d(1), side, true), None);
        assert_eq!(c.shift_back(d(1), out, false), None);
    }

    #[test]
    fn district_stock_counts_its_stores_and_exports_take_from_them() {
        let mut c = Cluster::new(1);
        let g = Good(5);
        let (s1, s2, st) = (
            PoolKey::Store(d(1), 1, g),
            PoolKey::Store(d(1), 2, g),
            PoolKey::Stock(d(1), g),
        );
        c.add_member(
            MemberSpec {
                district: d(1),
                pools: vec![(s1, 5, None), (s2, 7, None), (st, 1, None)],
                procs: vec![],
                space: None,
                bounds: Vec::new(),
            },
            Tick(0),
        );
        assert_eq!(c.level(st), Some(13));
        assert_eq!(c.add_to_pool(st, -10, false, Tick(0)), -10);
        assert_eq!(
            (c.level(s1), c.level(s2), c.pool(st).map(|p| p.level)),
            (Some(0), Some(3), Some(0))
        );
        assert_eq!(
            c.add_to_pool(st, -10, false, Tick(0)),
            -3,
            "only what the stores hold"
        );
        assert_eq!(
            c.add_to_pool(st, 4, false, Tick(0)),
            4,
            "imports wait in the district pool"
        );
        assert_eq!(c.pool(st).map(|p| p.level), Some(4));
        c.check_books().unwrap();
    }

    #[test]
    fn a_glass_cluster_round_trips_through_its_page() {
        let mut c = Cluster::new(1);
        let (a, b) = (
            PoolKey::Store(d(1), 1, Good(1)),
            PoolKey::Store(d(1), 2, Good(1)),
        );
        c.add_member(
            MemberSpec {
                district: d(1),
                pools: vec![(a, 1_000, None), (b, 0, None)],
                procs: vec![glass_proc(
                    ProcSpecBody::Cycle {
                        pools: vec![a, b],
                        pieces: vec![
                            piece(7, vec![-(FIXED_ONE_I / 3), FIXED_ONE_I / 3], &[(1, 0)]),
                            piece(5, vec![0, 0], &[]),
                        ],
                    },
                    Some(Tick(900)),
                )],
                space: None,
                bounds: vec![(3, (vec![(b, 1)], 0, 500))],
            },
            Tick(0),
        );
        c.settle(Tick(0), Tick(55));
        c.take_events();
        let bytes = crate::persist::to_bytes(&c);
        let mut back: Cluster = crate::persist::from_bytes(&bytes).unwrap();
        c.settles = 0;
        assert_eq!(back, c);
        c.settle(Tick(55), Tick(400));
        back.settle(Tick(55), Tick(400));
        c.settles = 0;
        back.settles = 0;
        assert_eq!(back, c);
    }
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct};

    impl Persist for PoolKey {
        fn put(&self, w: &mut Writer) {
            match *self {
                PoolKey::Stock(d, g) => {
                    w.u8(0);
                    d.put(w);
                    g.put(w);
                }
                PoolKey::Mail(d, i, a, g) => {
                    w.u8(2);
                    d.put(w);
                    i.put(w);
                    a.put(w);
                    g.put(w);
                }
                PoolKey::Store(d, i, g) => {
                    w.u8(3);
                    d.put(w);
                    i.put(w);
                    g.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => PoolKey::Stock(Persist::get(r)?, Persist::get(r)?),
                2 => PoolKey::Mail(
                    Persist::get(r)?,
                    Persist::get(r)?,
                    Persist::get(r)?,
                    Persist::get(r)?,
                ),
                3 => PoolKey::Store(Persist::get(r)?, Persist::get(r)?, Persist::get(r)?),
                _ => return invalid("pool key"),
            })
        }
    }

    persist_struct!(Books {
        initial,
        events,
        minted,
        burned,
        moved_in,
        moved_out
    });
    persist_struct!(Pool {
        key,
        level,
        cap,
        pushed,
        books
    });
    persist_struct!(Term { pool, flow });
    persist_struct!(Target {
        pool,
        stop_at,
        restart_at
    });
    persist_struct!(Space {
        owner,
        tag,
        weights,
        fixed,
        cap
    });

    impl Persist for Body {
        fn put(&self, w: &mut Writer) {
            match self {
                Body::Box { terms } => {
                    w.u8(0);
                    terms.put(w);
                }
                Body::Link {
                    from,
                    to,
                    flow,
                    batch,
                } => {
                    w.u8(1);
                    from.put(w);
                    to.put(w);
                    flow.put(w);
                    batch.put(w);
                }
                Body::Cycle(c) => {
                    w.u8(2);
                    c.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Body::Box {
                    terms: Persist::get(r)?,
                },
                1 => Body::Link {
                    from: Persist::get(r)?,
                    to: Persist::get(r)?,
                    flow: Persist::get(r)?,
                    batch: Persist::get(r)?,
                },
                2 => Body::Cycle(Box::new(Persist::get(r)?)),
                _ => return invalid("process body"),
            })
        }
    }

    persist_struct!(CyclePiece { len, rates, fires });

    persist_struct!(CycleFire { pool, canon });

    persist_struct!(CycleBody {
        pools,
        pieces,
        piece,
        off,
        flows
    });

    impl Persist for Stop {
        fn put(&self, w: &mut Writer) {
            match *self {
                Stop::Disabled => w.u8(0),
                Stop::InputEmpty(k) => {
                    w.u8(1);
                    k.put(w);
                }
                Stop::OutputFull(k) => {
                    w.u8(2);
                    k.put(w);
                }
                Stop::SpaceFull(d) => {
                    w.u8(3);
                    d.put(w);
                }
                Stop::Target(k) => {
                    w.u8(4);
                    k.put(w);
                }
                Stop::SourceStopped(d) => {
                    w.u8(5);
                    d.put(w);
                }
                Stop::Waiting => w.u8(6),
                Stop::Bound(d, tag) => {
                    w.u8(7);
                    d.put(w);
                    tag.put(w);
                }
                Stop::Timer => w.u8(8),
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Stop::Disabled,
                1 => Stop::InputEmpty(Persist::get(r)?),
                2 => Stop::OutputFull(Persist::get(r)?),
                3 => Stop::SpaceFull(Persist::get(r)?),
                4 => Stop::Target(Persist::get(r)?),
                5 => Stop::SourceStopped(Persist::get(r)?),
                6 => Stop::Waiting,
                7 => Stop::Bound(Persist::get(r)?, Persist::get(r)?),
                8 => Stop::Timer,
                _ => return invalid("stop reason"),
            })
        }
    }

    impl Persist for Run {
        fn put(&self, w: &mut Writer) {
            match self {
                Run::Running => w.u8(0),
                Run::Stopped(s) => {
                    w.u8(1);
                    s.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Run::Running,
                1 => Run::Stopped(Persist::get(r)?),
                _ => return invalid("run state"),
            })
        }
    }

    persist_struct!(Proc {
        owner,
        id,
        body,
        targets,
        requires,
        enabled,
        run,
        latency,
        armed,
        ran,
        deadline,
        min_run
    });

    /// Events are drained after every touch, so a saved cluster has none.
    impl Persist for Cluster {
        fn put(&self, w: &mut Writer) {
            debug_assert!(
                self.events.is_empty(),
                "saving a cluster with undrained events"
            );
            self.pools.put(w);
            self.procs.put(w);
            self.spaces.put(w);
            self.season_boundary.put(w);
            self.restart_min_run.put(w);
            self.departed.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(Cluster {
                pools: Persist::get(r)?,
                procs: Persist::get(r)?,
                spaces: Persist::get(r)?,
                season_boundary: Persist::get(r)?,
                restart_min_run: Persist::get(r)?,
                events: Vec::new(),
                departed: Persist::get(r)?,
                settles: 0,
            })
        }
    }
}
