// SPDX-License-Identifier: AGPL-3.0-or-later
//! The replay (C19, I6; SCALE.md §4 and §7): unloaded districts as piecewise-linear processes over
//! integer pools, one kernel [`LazyModel`] per *cluster* of districts that share stock.
//!
//! # Pools, processes, constraints
//!
//! - A **pool** is integer stock: a district's boundary stock of one good (summed over its
//!   inventories), an Ender Transmission channel buffer (I13), the packages a district has written
//!   into a postbox's offline buffer, or a finite deposit (a good of class `Deposit`, I11).
//! - A **box** process is a district's learned model (I6): one diffuser per good, negative for the
//!   goods it consumes and positive for those it produces. It runs at full rate or not at all.
//!   Its emissions are mints and burns, booked per pool.
//! - A **link** process moves one good from one pool to another at a rate (a transmitter, a
//!   receiver, a packager filling a postbox). Its emissions are transfers: one diffuser, so what
//!   leaves one pool is exactly what enters the other.
//! - **Constraints:** every pool stays at or above 0 and at or below its capacity; a process with
//!   a learned **target** stops when the target pool reaches it and restarts at or below its
//!   restart level; a district's shared **item space** (a weighted sum of its item pools) stays
//!   under the slot capacity of the inventories that received its outputs; a process that
//!   **requires** another (PowerGrid coupling, I13) runs only while that one runs.
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

use std::collections::BTreeSet;

use rtsim_kernel::clock::{SlotPeriod, Tick};
use rtsim_kernel::conserve::{Conserved, Diffuser, QuantityKind, Totals};
use rtsim_kernel::lazy::LazyModel;
use rtsim_kernel::rng::SlotRng;
use serde::Serialize;

use crate::ids::{ChannelId, DistrictId, Good};

/// One unit in 32.32 fixed point.
const U: i128 = 1 << 32;

/// What a pool holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PoolKey {
    /// A district's boundary stock of a good (a deposit is a good of class `Deposit`).
    Stock(DistrictId, Good),
    /// An Ender Transmission channel buffer's content of a good (I13).
    Channel(ChannelId, Good),
    /// Goods a district's packager wrote into packages in a postbox offline buffer (inventory
    /// index), not yet fetched by a train (INDUSTRY.md §8.2).
    Mail(DistrictId, u32, Good),
}

impl PoolKey {
    pub fn good(self) -> Good {
        match self {
            PoolKey::Stock(_, g) | PoolKey::Channel(_, g) | PoolKey::Mail(_, _, g) => g,
        }
    }

    pub fn district(self) -> Option<DistrictId> {
        match self {
            PoolKey::Stock(d, _) | PoolKey::Mail(d, _, _) => Some(d),
            PoolKey::Channel(..) => None,
        }
    }
}

/// Integer stock with its books.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pool {
    pub key: PoolKey,
    pub level: i64,
    /// Physical capacity, if any (a tank, an energy store, a channel buffer, a postbox's 18
    /// slots).
    pub cap: Option<i64>,
    /// Channel and mail pools: the level the world already holds (what Java was last told, plus
    /// real transfers it reported). The rest is pushed by command (I13, §8.2).
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Run {
    Running,
    Stopped(Stop),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Proc {
    pub owner: DistrictId,
    /// 0 is the box; links follow. Lower ids have priority when stock is shared.
    pub id: u16,
    pub body: Body,
    pub targets: Vec<Target>,
    /// (district, process id) that must be running for this one to run.
    pub requires: Option<(DistrictId, u16)>,
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
}

/// A district's shared item space: `fixed + Σ weight × level ≤ cap`, in slot units.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Space {
    pub owner: DistrictId,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcSpec {
    pub id: u16,
    pub body: ProcSpecBody,
    /// (pool, stop at, restart at).
    pub targets: Vec<(PoolKey, i64, i64)>,
    pub requires: Option<(DistrictId, u16)>,
    /// Restart latency in ticks ([`Proc::latency`]).
    pub latency: u64,
}

/// Everything a district brings into a cluster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberSpec {
    pub district: DistrictId,
    /// (pool, level, cap). Shared pools (channels) that already exist keep their level; the one
    /// given here is used only when the pool is new.
    pub pools: Vec<(PoolKey, i64, Option<i64>)>,
    pub procs: Vec<ProcSpec>,
    /// (weights by pool, fixed units, cap units).
    pub space: Option<(Vec<(PoolKey, i64)>, i64, i64)>,
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

/// One unloaded cluster: districts that share pools (channels) or running states (power lines).
/// Most clusters hold one district.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    pools: Vec<Pool>,
    procs: Vec<Proc>,
    spaces: Vec<Space>,
    season_boundary: Option<Tick>,
    restart_min_run: u64,
    events: Vec<ClusterEvent>,
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
fn relief_time(
    v0: i64,
    bound: Bound,
    base: &[Tm],
    extra: &[Tm],
    h: u64,
) -> Option<u64> {
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
        s.extend(self.pools.iter().filter_map(|p| p.key.district()));
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

    pub fn level(&self, key: PoolKey) -> Option<i64> {
        self.pool(key).map(|p| p.level)
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
        for ps in spec.procs {
            let body = match ps.body {
                ProcSpecBody::Box(terms) => Body::Box {
                    terms: terms
                        .into_iter()
                        .map(|(k, flow)| Term {
                            pool: self.ensure_pool(k, 0, None),
                            flow,
                        })
                        .collect(),
                },
                ProcSpecBody::Link {
                    from,
                    to,
                    flow,
                    batch,
                } => {
                    let from = self.ensure_pool(from, 0, None);
                    let to = self.ensure_pool(to, 0, None);
                    Body::Link {
                        from,
                        to,
                        flow,
                        batch,
                    }
                }
            };
            let targets = ps
                .targets
                .iter()
                .map(|&(k, stop_at, restart_at)| Target {
                    pool: self.ensure_pool(k, 0, None),
                    stop_at,
                    restart_at,
                })
                .collect();
            let proc_ = Proc {
                owner: spec.district,
                id: ps.id,
                body,
                targets,
                requires: ps.requires,
                enabled: true,
                run: Run::Stopped(Stop::Waiting),
                latency: ps.latency,
                armed: None,
                ran: false,
            };
            let pos = self
                .procs
                .binary_search_by_key(&(proc_.owner, proc_.id), |p| (p.owner, p.id))
                .unwrap_or_else(|e| e);
            self.procs.insert(pos, proc_);
        }
        if let Some((weights, fixed, cap)) = spec.space {
            let weights = weights
                .into_iter()
                .map(|(k, w)| (self.ensure_pool(k, 0, None), w))
                .collect();
            self.spaces.push(Space {
                owner: spec.district,
                weights,
                fixed,
                cap,
            });
            self.spaces.sort_by_key(|s| s.owner);
        }
        self.evaluate(at);
    }

    /// Takes a district out: its processes, its space, and the pools it owns. Shared pools
    /// (channels) stay. Callers settle the cluster to the moment first (an event).
    pub fn remove_member(&mut self, d: DistrictId, at: Tick) -> MemberOut {
        self.procs.retain(|p| p.owner != d);
        self.spaces.retain(|s| s.owner != d);
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.pools.len() {
            if self.pools[i].key.district() == Some(d) {
                out.push(self.pools.remove(i));
                self.shift_pool_refs(i as u16 + 1, -1);
            } else {
                i += 1;
            }
        }
        // Channels nobody links to any more are dropped; the engine keeps their level.
        let used: BTreeSet<u16> = self
            .procs
            .iter()
            .flat_map(|p| match &p.body {
                Body::Box { terms } => terms.iter().map(|t| t.pool).collect::<Vec<_>>(),
                Body::Link { from, to, .. } => vec![*from, *to],
            })
            .collect();
        let mut i = 0;
        while i < self.pools.len() {
            if !used.contains(&(i as u16)) && matches!(self.pools[i].key, PoolKey::Channel(..)) {
                out.push(self.pools.remove(i));
                self.shift_pool_refs(i as u16 + 1, -1);
            } else {
                i += 1;
            }
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
        self.spaces.sort_by_key(|s| s.owner);
        self.events.extend(other.events);
        self.season_boundary = match (self.season_boundary, other.season_boundary) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.evaluate(at);
    }

    // ---- events ---------------------------------------------------------------------------

    /// An import (positive) or export (negative) at an event, creating the pool if needed. A
    /// `physical` change is one the world already made (a real channel transfer, a package a train
    /// fetched), so it is not pushed again. Returns the amount applied: an export never takes a
    /// pool below 0.
    pub fn add_to_pool(&mut self, key: PoolKey, delta: i64, physical: bool, at: Tick) -> i64 {
        let i = self.ensure_pool(key, 0, None) as usize;
        let p = &mut self.pools[i];
        let applied = if delta < 0 { delta.max(-p.level) } else { delta };
        p.level += applied;
        p.books.events += applied;
        if physical {
            p.pushed += applied;
        }
        self.evaluate(at);
        applied
    }

    /// The changes to channel and mail pools not yet in the world, marked as pushed.
    pub fn take_unpushed(&mut self) -> Vec<(PoolKey, i64)> {
        let mut out = Vec::new();
        for p in &mut self.pools {
            if matches!(p.key, PoolKey::Channel(..)) && p.level != p.pushed {
                out.push((p.key, p.level - p.pushed));
                p.pushed = p.level;
            }
        }
        out
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

    /// Changes what a process requires (a PowerGrid source starting or stopping its replay).
    pub fn set_requires(&mut self, owner: DistrictId, id: u16, requires: Option<(DistrictId, u16)>, at: Tick) {
        for p in &mut self.procs {
            if p.owner == owner && p.id == id {
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
            Some((d, id)) => self
                .procs
                .iter()
                .position(|p| p.owner == d && p.id == id)
                .is_some_and(running),
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
        }
    }

    /// The constraint's current value, bound and terms under `running`.
    fn con_terms(&self, c: Con, running: &dyn Fn(usize) -> bool, out: &mut Vec<Tm>) -> (i64, Bound) {
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
            Con::Space(si) => Stop::SpaceFull(self.spaces[si].owner),
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
        // Stops, to a fixed point.
        'stops: loop {
            for i in 0..self.procs.len() {
                if self.is_running(i) && !self.requirement_met(i, &|j| self.is_running(j)) {
                    let src = self.procs[i].requires.expect("requirement").0;
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
                if !p.enabled || p.run == Run::Running {
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
    fn blocking(&self, i: usize, cons: &[Con]) -> Option<Stop> {
        let h = self.restart_min_run.max(1);
        if !self.requirement_met(i, &|j| self.is_running(j)) {
            return self.procs[i].requires.map(|r| Stop::SourceStopped(r.0));
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
        let h = self.restart_min_run.max(1);
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
        let h = self.restart_min_run.max(1);
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
            if let Some(t) = p.armed
                && p.run != Run::Running
            {
                keep(t.since(anchor).max(1));
            }
            if p.enabled && p.run != Run::Running && p.armed.is_none() && self.requirement_met(i, &running) {
                if let Some(n) = self.restart_time(i, &cons) {
                    keep(n.saturating_add(p.latency));
                }
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
                to,
                batch: Some(b),
                ..
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
        let (mut ins, mut outs) = (0i128, 0i128);
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
                body: ProcSpecBody::Box(vec![(o, Diffuser::per(-3, 100)), (i, Diffuser::per(3, 100))]),
                targets: vec![],
                requires: None,
                latency: 0,
            }],
            space: None,
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
        assert_eq!(c.run_of(d(1), 0), Some(Run::Stopped(Stop::InputEmpty(PoolKey::Stock(d(1), Good(1))))));
        assert!(c.level(PoolKey::Stock(d(1), Good(1))).unwrap() >= 0);
        c.check_books().unwrap();
        assert_eq!(c.next_breakpoint(bp), None, "a starved box waits for an event");
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
        assert!(matches!(c.run_of(d(1), 0), Some(Run::Stopped(Stop::OutputFull(_)))));
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
                    body: ProcSpecBody::Box(vec![(a, Diffuser::per(-1, 10)), (b, Diffuser::per(1, 10))]),
                    targets: vec![],
                    requires: Some((d(1), 0)),
                    latency: 0,
                }],
                space: None,
            },
            Tick(0),
        );
        assert_eq!(c.run_of(d(2), 0), Some(Run::Running));
        let bp = c.next_breakpoint(Tick(0)).unwrap();
        c.settle(Tick(0), bp);
        assert_eq!(c.run_of(d(2), 0), Some(Run::Stopped(Stop::SourceStopped(d(1)))));
        c.add_to_pool(PoolKey::Stock(d(1), Good(1)), 300, false, bp);
        assert_eq!(c.run_of(d(2), 0), Some(Run::Running), "the sink restarts with its source");
    }
}
