// SPDX-License-Identifier: AGPL-3.0-or-later
//! The view tap (VIEWER.md §5.2): while a sidecar holds the lease, the engine's slack encodes the
//! layers as parts, and the IO lane commits the changed ones into the view store once a period.
//!
//! - **The lease.** Each step compares its hand-over number with the next stat's. Every
//!   [`STAT_EVERY`] steps ([`STAT_EVERY_LEASED`] while leased) the IO lane `stat`s
//!   `<world>/rtsim/view.lease` and reads it only when its time changed. A lease older than
//!   [`LEASE_LIFE`] is over. A new lease starts a new epoch (a new folder; older ones are
//!   deleted). [`DELETE_AFTER`] without a lease, and at a session's first stat, the IO lane deletes
//!   `view/`; a delete that fails is tried again at the next stat.
//! - **The slack.** Once step h has published its commands and counted itself done, the tap takes
//!   the state lock only if it is free (step h + 1 has not started) and encodes whole units while
//!   hand-over h + 1 has not been published (an acquire load between units), for at most
//!   [`BUDGET`]. It stands down for the step when the hand-over held more than one TICK record
//!   (Java ticked again before the engine took the last one: the engine is behind) or the step took
//!   more than [`STAND_DOWN_STEP`]. Its time is not step time: `last_step_nanos` is stored before it
//!   runs.
//! - **The order.** The layer list once an epoch; the archive's pending rows once a period; the
//!   focus regions' units (found by where each unit's rows were last) and samples once a period;
//!   then every unit round robin. A cycle is one pass over every unit; its length goes in the
//!   header.
//! - **The mailbox.** One slot per unit, latest wins. Once a period, if nothing else is on the IO
//!   lane (`pool_info`) and no commit is outstanding, the mailbox goes to the IO lane, which drops
//!   the parts whose CRC32 matches the last written and commits the rest with the header page as
//!   one `NoSync` commit.
//!
//! The tap reads module state through `&` only, inside the kernel's resident-only reads: a unit
//! that met an evicted row is published as not resident, with no parts, and the sidecar draws it
//! from the sim pages (§5.2 item 6). Nothing it does reaches a module, the pages or the hand-over.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mic_native_rt::{self as rt, Pool};
use rtsim_kernel::codec::Writer;
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rows::resident_only;
use rtsim_layers::{IndustrySource, Sel, Source, Spec, Src};
use rtsim_parts::mark::kind as row_kind;
use rtsim_parts::part::kind as part_kind;
use rtsim_parts::store::{self as vstore, Header, LayerState, Lease, PartEntry, UnitEntry};
use rtsim_parts::{Part, PartKey, PartWriter, RegionKey, ViewSource};

use super::{Engine, Exec, State, lock, log_warn};

/// Encoding time a step (TUNABLE, §5.2 item 2).
pub const BUDGET: Duration = Duration::from_micros(250);
/// A step longer than this, half a tick, means the engine is behind: the tap stands down
/// (TUNABLE).
pub const STAND_DOWN_STEP: Duration = Duration::from_millis(25);
/// The tap period: one commit, one focus pass and one archive pass at most (TUNABLE).
pub const PERIOD: Duration = Duration::from_secs(2);
/// Steps between lease stats, unleased (10 s at 20 TPS) and leased (one period) (TUNABLE).
pub const STAT_EVERY: u64 = 200;
pub const STAT_EVERY_LEASED: u64 = 40;
/// A lease not renewed for this long is over (TUNABLE).
pub const LEASE_LIFE: Duration = Duration::from_secs(30);
/// `view/` is deleted once no lease has been live for this long (TUNABLE).
pub const DELETE_AFTER: Duration = Duration::from_secs(60);
/// Archive rows a part holds.
const ARCHIVE_PER_UNIT: usize = rtsim_parts::MAX_ROWS as usize;
/// The archive dictionary names' unit.
pub const ARCHIVE_NAMES: u64 = u64::MAX;
/// The engine's build, as the header names it.
pub const BUILD: &str = concat!("rtsim-jni ", env!("CARGO_PKG_VERSION"));

/// Every layer, built once.
fn specs() -> &'static [Spec] {
    static SPECS: OnceLock<Vec<Spec>> = OnceLock::new();
    SPECS.get_or_init(rtsim_layers::specs)
}

/// The tap's counters, readable from any thread (in [`super::Stats`]; the header carries them
/// for `/api/status`).
#[derive(Default)]
pub struct Counters {
    /// 1 while a lease is live, and its epoch.
    pub leased: AtomicU64,
    pub epoch: AtomicU64,
    pub lease_stats: AtomicU64,
    /// Steps the tap encoded in, steps it stood down (the engine behind), steps it found the next
    /// hand-over already there, and steps the next hand-over cut short.
    pub steps_tapped: AtomicU64,
    pub stood_down: AtomicU64,
    pub next_already: AtomicU64,
    pub cut_by_next: AtomicU64,
    /// Of the steps stood down, those whose hand-over held more than one TICK record (the engine
    /// behind); and the steps after which the next step (or `close`) already held the state.
    pub behind: AtomicU64,
    pub lock_busy: AtomicU64,
    /// Units encoded, those that met evicted state, their rows and bytes, their total and longest
    /// encode time.
    pub units: AtomicU64,
    pub not_resident: AtomicU64,
    pub rows: AtomicU64,
    pub bytes: AtomicU64,
    pub unit_nanos: AtomicU64,
    pub unit_nanos_max: AtomicU64,
    /// The tap's time over every step and its longest at one step; steps past the budget and the
    /// most past it (at most one unit's encode).
    pub tap_nanos: AtomicU64,
    pub tap_nanos_max: AtomicU64,
    pub over_budget: AtomicU64,
    pub overrun_nanos_max: AtomicU64,
    /// The longest a published hand-over waited for the tap: the time since the tap last saw it
    /// absent (at most one unit's encode).
    pub ready_wait_nanos_max: AtomicU64,
    /// The longest single job (a unit's encode, the archive's, the layer list, a cycle's wrap):
    /// what the tap can run past its budget, and what a ready hand-over can wait.
    pub job_nanos_max: AtomicU64,
    /// Whole cycles in this epoch and the last one's length.
    pub cycles: AtomicU64,
    pub cycle_ms: AtomicU64,
    /// Commits, periods the IO lane was busy, the longest commit, parts written and dropped as
    /// unchanged.
    pub commits: AtomicU64,
    pub commits_deferred: AtomicU64,
    pub commit_nanos_max: AtomicU64,
    pub parts_written: AtomicU64,
    pub parts_unchanged: AtomicU64,
    /// `view/` deletes, and failures (a stat, a delete, a commit, a refused IO job).
    pub deletes: AtomicU64,
    pub failures: AtomicU64,
    /// Encode cost by layer index (the archive's rows last): units, rows, total and longest
    /// nanoseconds. The header names them by layer id, so a page shows which layer costs what.
    by_layer: Mutex<Vec<[u64; 4]>>,
}

impl Counters {
    /// Name and value of each counter, as the header holds them, and the budget.
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        let all: [(&str, &AtomicU64); 30] = [
            ("leased", &self.leased),
            ("epoch", &self.epoch),
            ("lease_stats", &self.lease_stats),
            ("steps_tapped", &self.steps_tapped),
            ("stood_down", &self.stood_down),
            ("next_already", &self.next_already),
            ("cut_by_next", &self.cut_by_next),
            ("behind", &self.behind),
            ("lock_busy", &self.lock_busy),
            ("units", &self.units),
            ("not_resident", &self.not_resident),
            ("rows", &self.rows),
            ("bytes", &self.bytes),
            ("unit_nanos", &self.unit_nanos),
            ("unit_nanos_max", &self.unit_nanos_max),
            ("tap_nanos", &self.tap_nanos),
            ("tap_nanos_max", &self.tap_nanos_max),
            ("over_budget", &self.over_budget),
            ("overrun_nanos_max", &self.overrun_nanos_max),
            ("ready_wait_nanos_max", &self.ready_wait_nanos_max),
            ("job_nanos_max", &self.job_nanos_max),
            ("cycles", &self.cycles),
            ("cycle_ms", &self.cycle_ms),
            ("commits", &self.commits),
            ("commits_deferred", &self.commits_deferred),
            ("commit_nanos_max", &self.commit_nanos_max),
            ("parts_written", &self.parts_written),
            ("parts_unchanged", &self.parts_unchanged),
            ("deletes", &self.deletes),
            ("failures", &self.failures),
        ];
        let mut out: Vec<(String, u64)> = all
            .iter()
            .map(|(k, a)| ((*k).to_owned(), a.load(Ordering::Acquire)))
            .collect();
        out.push(("budget_nanos".into(), nanos(BUDGET)));
        let by = self.by_layer.lock().unwrap_or_else(|e| e.into_inner());
        let ids = specs()
            .iter()
            .map(|s| s.desc.id.as_str())
            .chain(["archive"]);
        for (id, [units, rows, total, longest]) in ids.zip(by.iter()) {
            if *units > 0 {
                out.push((format!("unit_nanos_max.{id}"), *longest));
                out.push((format!("nanos_per_row.{id}"), total / (*rows).max(1)));
                out.push((format!("rows_per_unit.{id}"), rows / units));
            }
        }
        out
    }

    /// One unit of layer index `layer` (`specs().len()` for the archive's) took `took` for `rows`.
    fn layer_cost(&self, layer: usize, rows: u64, took: u64) {
        let mut by = self.by_layer.lock().unwrap_or_else(|e| e.into_inner());
        if by.len() <= layer {
            by.resize(specs().len().max(layer) + 1, [0; 4]);
        }
        let c = &mut by[layer];
        c[0] += 1;
        c[1] += rows;
        c[2] += took;
        c[3] = c[3].max(took);
    }

    /// Zeroes the measurements (not the lease's state): a bench measures a stretch of steps.
    pub fn reset_measurements(&self) {
        for a in [
            &self.steps_tapped,
            &self.stood_down,
            &self.next_already,
            &self.cut_by_next,
            &self.behind,
            &self.lock_busy,
            &self.units,
            &self.not_resident,
            &self.rows,
            &self.bytes,
            &self.unit_nanos,
            &self.unit_nanos_max,
            &self.tap_nanos,
            &self.tap_nanos_max,
            &self.over_budget,
            &self.overrun_nanos_max,
            &self.ready_wait_nanos_max,
            &self.job_nanos_max,
            &self.commit_nanos_max,
        ] {
            a.store(0, Ordering::Release);
        }
        self.by_layer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

fn add(a: &AtomicU64, v: u64) {
    a.fetch_add(v, Ordering::AcqRel);
}

fn max(a: &AtomicU64, v: u64) {
    a.fetch_max(v, Ordering::AcqRel);
}

fn nanos(d: Duration) -> u64 {
    d.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The step side (in [`State`], touched only under the state lock).
#[derive(Default)]
pub(super) struct Tap {
    shared: Arc<Shared>,
    /// The hand-over number at which the lease is next `stat`ed.
    next_stat: u64,
    /// TICK records in the last hand-over.
    ticks: u32,
    /// The epoch the cursor works for; a new one restarts it.
    epoch: u64,
    globals_done: bool,
    /// This cycle's units, and the next one's index.
    queue: Vec<PartKey>,
    pos: usize,
    cycle_start: Option<Instant>,
    cycles: u64,
    /// Units and sample regions of the focus pass under way.
    focus: VecDeque<PartKey>,
    sample_queue: VecDeque<RegionKey>,
    focus_at: Option<Instant>,
    /// Where each unit's rows were at its last encode (the focus pass finds units by these).
    regions: HashMap<PartKey, Vec<RegionKey>>,
    archive: ArchiveCursor,
    archive_at: Option<Instant>,
    period_at: Option<Instant>,
    mailbox: Mailbox,
}

/// How far the archive's pending rows are encoded.
#[derive(Default)]
struct ArchiveCursor {
    /// The serial of the save that last took the rows (`State::archive_serial`).
    serial: u64,
    /// Rows in whole units already encoded (a whole unit does not change until a save takes it).
    done: usize,
    /// The row count at the last encode of the partial unit.
    last: Option<usize>,
    /// Kinds and frames named at the last names encode.
    names: Option<(usize, usize)>,
}

/// What the step side and the IO jobs share.
#[derive(Default)]
struct Shared {
    leased: AtomicBool,
    epoch: AtomicU64,
    focus: Mutex<Vec<RegionKey>>,
    stating: AtomicBool,
    committing: AtomicBool,
    io: Mutex<Io>,
}

/// Encoded units on their way to the IO lane: one slot per unit, the latest encode.
#[derive(Default)]
struct Mailbox {
    epoch: u64,
    layers: Option<Vec<u8>>,
    units: BTreeMap<PartKey, Encoded>,
    /// The mark units there are, as of the last cycle's start; archive units below this count;
    /// sample regions of the last focus pass. The IO lane drops stored units outside them.
    marks: Option<BTreeSet<PartKey>>,
    archive_units: Option<u64>,
    samples: Option<BTreeSet<u64>>,
    /// Why each layer would be empty, from its last unit's builder.
    empty: BTreeMap<u16, Option<String>>,
    game_time: i64,
    cycles: u64,
    cycle_ms: u64,
    archive_serial: u64,
}

struct Encoded {
    resident: bool,
    at: i64,
    parts: Vec<(Option<RegionKey>, Part)>,
}

/// The IO lane's side.
#[derive(Default)]
struct Io {
    started: bool,
    lease_mtime: Option<SystemTime>,
    lease_ok: bool,
    /// The process id the lease names, and the one the epoch was started for: another sidecar
    /// is another lease.
    lease_pid: u32,
    epoch_pid: u32,
    /// When the last lease ended: its last renewal plus [`LEASE_LIFE`], or when it was found gone.
    unleased_since: Option<SystemTime>,
    delete_due: bool,
    last_epoch: u64,
    epoch: u64,
    store: Option<PageStore>,
    layers: Option<Vec<u8>>,
    layers_written: bool,
    versions: Vec<u64>,
    archive_version: u64,
    empty: BTreeMap<u16, Option<String>>,
    units: BTreeMap<PartKey, UnitEntry>,
    marks: Option<BTreeSet<PartKey>>,
    archive_units: Option<u64>,
    samples: Option<BTreeSet<u64>>,
    next_page: u64,
    commits: u64,
    last: Option<(Instant, i64)>,
}

impl Io {
    /// A new epoch: nothing stored yet.
    fn begin(&mut self, epoch: u64) {
        *self = Io {
            started: self.started,
            lease_mtime: self.lease_mtime,
            lease_ok: self.lease_ok,
            lease_pid: self.lease_pid,
            epoch_pid: self.lease_pid,
            last_epoch: self.last_epoch,
            epoch,
            ..Io::default()
        };
    }

    fn keeps(&self, k: &PartKey) -> bool {
        match k.kind {
            part_kind::MARKS => self.marks.as_ref().is_none_or(|m| m.contains(k)),
            part_kind::ARCHIVE_ROWS => {
                k.unit == ARCHIVE_NAMES || self.archive_units.is_none_or(|n| k.unit < n)
            }
            part_kind::SAMPLES => self.samples.as_ref().is_none_or(|s| s.contains(&k.unit)),
            _ => true,
        }
    }
}

/// Runs `f` on the IO lane (at once without a host). False if the lane refused it.
fn io(e: &Engine, f: impl FnOnce() + Send + 'static) -> bool {
    match e.cfg.exec {
        Exec::Host => rt::io(f).is_ok(),
        Exec::Inline => {
            f();
            true
        }
    }
}

/// Whether the IO lane has nothing else to do (saves and prefetches go first).
fn io_idle(e: &Engine) -> bool {
    match e.cfg.exec {
        Exec::Inline => true,
        Exec::Host => {
            rt::governor::pool_info(Pool::Io).map_or(true, |i| i.running == 0 && i.queued == 0)
        }
    }
}

// ---- the step side ------------------------------------------------------------------------------

/// After step `h` is done (its commands published, `steps_done` counted): the lease check, then,
/// while leased, the tap in the slack and the period's commit.
pub(super) fn after_step(e: &Arc<Engine>, h: u64, step_nanos: u64) {
    let mut guard = match e.state.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        // Step h + 1 (or `close`) has the state: no slack.
        Err(TryLockError::WouldBlock) => {
            add(&e.stats.tap.lock_busy, 1);
            return;
        }
    };
    let st = &mut *guard;
    let leased = st.tap.shared.leased.load(Ordering::Acquire);
    if h >= st.tap.next_stat && !st.tap.shared.stating.swap(true, Ordering::AcqRel) {
        st.tap.next_stat = h + if leased {
            STAT_EVERY_LEASED
        } else {
            STAT_EVERY
        };
        spawn_stat(e, &st.tap.shared);
    }
    let held = st.tap.ticks;
    if !leased {
        return;
    }
    let c = &e.stats.tap;
    let epoch = st.tap.shared.epoch.load(Ordering::Acquire);
    if epoch != st.tap.epoch {
        restart(&mut st.tap, epoch);
    }
    // The header's time is the state's even when the tap encodes nothing this step (it stood down,
    // or the next hand-over was already there): a commit never carries a stale or zero time.
    st.tap.mailbox.game_time = state_now(st);
    st.tap.mailbox.archive_serial = st.archive_serial;
    let t0 = Instant::now();
    if held > 1 || step_nanos > nanos(STAND_DOWN_STEP) {
        add(&c.stood_down, 1);
        if held > 1 {
            add(&c.behind, 1);
        }
    } else {
        slack(e, st, h, t0);
    }
    maybe_commit(e, &mut st.tap);
}

impl Tap {
    /// TICK records the hand-over just read held (`read_observations`).
    pub(super) fn ticks(&mut self, n: u32) {
        self.ticks = n;
    }
}

fn spawn_stat(e: &Arc<Engine>, shared: &Arc<Shared>) {
    let (e2, sh) = (Arc::clone(e), Arc::clone(shared));
    if !io(e, move || {
        stat(&e2, &sh);
        sh.stating.store(false, Ordering::Release);
    }) {
        shared.stating.store(false, Ordering::Release);
        add(&e.stats.tap.failures, 1);
    }
}

fn restart(tap: &mut Tap, epoch: u64) {
    *tap = Tap {
        shared: Arc::clone(&tap.shared),
        next_stat: tap.next_stat,
        ticks: tap.ticks,
        epoch,
        regions: std::mem::take(&mut tap.regions),
        mailbox: Mailbox {
            epoch,
            ..Mailbox::default()
        },
        ..Tap::default()
    };
}

/// Why the tap stopped encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Budget,
    Next,
    /// Nothing to encode (no units), or a flush finished its cycle.
    Done,
}

/// The tap in step h's slack.
fn slack(e: &Engine, st: &mut State, h: u64, t0: Instant) {
    let c = &e.stats.tap;
    let next = e.obs_buffer(h as usize + 1);
    if next.published().load(Ordering::Acquire) == h + 1 {
        add(&c.next_already, 1);
        return;
    }
    let deadline = t0 + BUDGET;
    let mut clear = t0;
    let mut stop = || {
        let now = Instant::now();
        if next.published().load(Ordering::Acquire) == h + 1 {
            max(&c.ready_wait_nanos_max, nanos(now - clear));
            return Some(Stop::Next);
        }
        clear = now;
        (now >= deadline).then_some(Stop::Budget)
    };
    let why = run(st, c, false, &mut stop);
    if why == Stop::Next {
        add(&c.cut_by_next, 1);
    }
    let took = t0.elapsed();
    add(&c.steps_tapped, 1);
    add(&c.tap_nanos, nanos(took));
    max(&c.tap_nanos_max, nanos(took));
    if took > BUDGET {
        add(&c.over_budget, 1);
        max(&c.overrun_nanos_max, nanos(took - BUDGET));
    }
}

/// One job of the tap's order.
enum Job {
    Globals,
    Archive,
    Sample(RegionKey),
    Unit(PartKey),
    Wrap,
}

fn next_job(tap: &mut Tap, flush: bool, now: Instant) -> Job {
    if !tap.globals_done {
        return Job::Globals;
    }
    if tap.archive_at.is_none_or(|t| !flush && now - t >= PERIOD) {
        return Job::Archive;
    }
    if !flush && tap.focus_at.is_none_or(|t| now - t >= PERIOD) {
        refill_focus(tap, now);
    }
    if let Some(r) = tap.sample_queue.pop_front() {
        return Job::Sample(r);
    }
    if let Some(k) = tap.focus.pop_front() {
        return Job::Unit(k);
    }
    if tap.pos < tap.queue.len() {
        tap.pos += 1;
        return Job::Unit(tap.queue[tap.pos - 1]);
    }
    Job::Wrap
}

/// The focus pass: the units whose rows were last in a focus region, and the regions' samples.
fn refill_focus(tap: &mut Tap, now: Instant) {
    tap.focus_at = Some(now);
    let focus: BTreeSet<RegionKey> = lock(&tap.shared.focus).iter().copied().collect();
    let mut units: Vec<PartKey> = tap
        .regions
        .iter()
        .filter(|(_, rs)| rs.iter().any(|r| focus.contains(r)))
        .map(|(k, _)| *k)
        .collect();
    units.sort_unstable();
    tap.focus = units.into();
    tap.sample_queue = focus.iter().copied().collect();
    tap.mailbox.samples = Some(focus.iter().map(|r| r.pack()).collect());
    tap.mailbox
        .units
        .retain(|k, _| k.kind != part_kind::SAMPLES || focus.contains(&RegionKey::unpack(k.unit)));
}

/// The state's time as the save viewer reads a save's (rtsim-view's `Saved::now`): the phase 1
/// clock or the industry engine's, whichever is later; the last TICK when both are behind it.
fn state_now(st: &State) -> i64 {
    (st.phase1.world.now().0.saturating_sub(st.phase1.offset) as i64)
        .max(st.industry.engine.now().0 as i64)
        .max(st.game_time)
}

/// Encodes jobs until `stop` says so (checked before each job) or, in a flush, until a whole
/// cycle is done.
fn run(st: &mut State, c: &Counters, flush: bool, stop: &mut dyn FnMut() -> Option<Stop>) -> Stop {
    let now_game = state_now(st);
    let State {
        tap,
        phase1,
        dragons,
        herds,
        ships,
        conflict,
        industry,
        archive,
        archive_serial,
        ..
    } = st;
    let src = Src {
        world: Some(&phase1.world),
        dragons: Some(&dragons.engine),
        herds: Some(&herds.engine),
        ships: Some(&ships.engine),
        conflict: Some(&conflict.engine),
        industry: Some(&industry.engine as &dyn IndustrySource),
        names: &industry.java_names,
        wnow: phase1.world.now(),
        offset: phase1.offset,
    };
    let source = Source {
        src,
        specs: specs(),
    };
    tap.mailbox.game_time = now_game;
    tap.mailbox.archive_serial = *archive_serial;
    loop {
        if !flush && let Some(s) = stop() {
            return s;
        }
        let now = Instant::now();
        let job = next_job(tap, flush, now);
        let done = run_job(
            job,
            tap,
            &source,
            archive,
            *archive_serial,
            now_game,
            c,
            flush,
            now,
        );
        max(&c.job_nanos_max, nanos(now.elapsed()));
        if !done {
            return Stop::Done;
        }
    }
}

/// Runs one job; false when the cycle's wrap says to stop.
#[allow(clippy::too_many_arguments)]
fn run_job(
    job: Job,
    tap: &mut Tap,
    source: &Source,
    archive: &rtsim_archive::Recorder,
    archive_serial: u64,
    now_game: i64,
    c: &Counters,
    flush: bool,
    now: Instant,
) -> bool {
    match job {
        Job::Globals => {
            tap.mailbox.layers = Some(rtsim_layers::layers(specs()).to_json());
            tap.globals_done = true;
        }
        Job::Archive => {
            if !archive_job(tap, archive, archive_serial, now_game, c) {
                tap.archive_at = Some(now);
            }
        }
        Job::Sample(r) => {
            let mut w = PartWriter::new(part_kind::SAMPLES, 0, u64::MAX, true);
            let _ = catch_unwind(AssertUnwindSafe(|| {
                resident_only(|| source.write_samples(r, now_game, &mut w))
            }));
            let key = PartKey {
                kind: part_kind::SAMPLES,
                layer: 0,
                unit: r.pack(),
                region: None,
            };
            if w.rows() > 0 {
                let parts = w.finish().into_iter().map(|(k, p)| (k.region, p)).collect();
                tap.mailbox.units.insert(
                    key,
                    Encoded {
                        resident: true,
                        at: now_game,
                        parts,
                    },
                );
            } else {
                tap.mailbox.units.remove(&key);
            }
        }
        Job::Unit(key) => {
            let (enc, why, regions) = encode_unit(source, key, now_game, c);
            tap.regions.insert(key, regions);
            tap.mailbox.empty.insert(key.layer, why);
            tap.mailbox.units.insert(key, enc);
        }
        Job::Wrap => return wrap(tap, source, c, flush, now),
    }
    true
}

/// Encodes one unit whole, resident only. A unit that met evicted state, or whose builder
/// panicked, is not resident and carries no parts.
fn encode_unit(
    source: &Source,
    key: PartKey,
    now: i64,
    c: &Counters,
) -> (Encoded, Option<String>, Vec<RegionKey>) {
    let t = Instant::now();
    let spec = &source.specs[usize::from(key.layer)];
    let mut w = spec.writer(key.layer);
    let r = catch_unwind(AssertUnwindSafe(|| {
        resident_only(|| source.write_part(key, now, &mut w))
    }));
    let (why, resident) = match r {
        Ok((why, missed)) => (why, !missed),
        Err(_) => (None, false),
    };
    let mut regions: Vec<RegionKey> = Vec::new();
    let mut parts = Vec::new();
    if resident {
        for (k, p) in w.finish() {
            regions.extend_from_slice(&p.regions);
            parts.push((k.region, p));
        }
        regions.sort_unstable();
        regions.dedup();
    } else {
        add(&c.not_resident, 1);
    }
    let took = nanos(t.elapsed());
    add(&c.units, 1);
    add(&c.unit_nanos, took);
    max(&c.unit_nanos_max, took);
    let rows = parts.iter().map(|(_, p)| u64::from(p.rows)).sum::<u64>();
    add(&c.rows, rows);
    c.layer_cost(usize::from(key.layer), rows, took);
    add(
        &c.bytes,
        parts.iter().map(|(_, p)| p.bytes.len() as u64).sum::<u64>(),
    );
    (
        Encoded {
            resident,
            at: now,
            parts,
        },
        why,
        regions,
    )
}

/// The end of the queue: a whole cycle is done (counted), and the next cycle's units are listed.
/// False when there is nothing to encode, or a flush's cycle is done.
fn wrap(tap: &mut Tap, source: &Source, c: &Counters, flush: bool, now: Instant) -> bool {
    if !tap.queue.is_empty() {
        tap.cycles += 1;
        let ms = tap
            .cycle_start
            .map_or(0, |t| now.duration_since(t).as_millis() as u64);
        tap.mailbox.cycles = tap.cycles;
        tap.mailbox.cycle_ms = ms;
        c.cycles.store(tap.cycles, Ordering::Release);
        c.cycle_ms.store(ms, Ordering::Release);
        if flush {
            tap.queue.clear();
            tap.pos = 0;
            return false;
        }
    }
    let units = match catch_unwind(AssertUnwindSafe(|| resident_only(|| source.parts()))) {
        Ok((u, _)) => u,
        Err(_) => {
            add(&c.failures, 1);
            Vec::new()
        }
    };
    let set: BTreeSet<PartKey> = units.iter().copied().collect();
    tap.regions.retain(|k, _| set.contains(k));
    tap.mailbox
        .units
        .retain(|k, _| k.kind != part_kind::MARKS || set.contains(k));
    // Layers with no units at all still say why they are empty.
    for (i, s) in source.specs.iter().enumerate() {
        let i = i as u16;
        if !set.iter().any(|k| k.layer == i) {
            let mut w = s.writer(i);
            let why = catch_unwind(AssertUnwindSafe(|| {
                resident_only(|| s.build(&source.src, &Sel::Keys(0..0), &mut w)).0
            }))
            .ok()
            .flatten();
            tap.mailbox.empty.insert(i, why);
        }
    }
    tap.mailbox.marks = Some(set);
    tap.queue = units;
    tap.pos = 0;
    tap.cycle_start = Some(now);
    if tap.queue.is_empty() {
        // Nothing to encode: an empty world's cycle is whole at once.
        tap.cycles = tap.cycles.max(1);
        tap.mailbox.cycles = tap.cycles;
        c.cycles.store(tap.cycles, Ordering::Release);
        return false;
    }
    true
}

/// One step of the archive's pending rows: the names when they changed, else the next unit not
/// yet encoded as it stands. False when there is nothing left to encode this period.
fn archive_job(
    tap: &mut Tap,
    rec: &rtsim_archive::Recorder,
    serial: u64,
    now: i64,
    c: &Counters,
) -> bool {
    let rows = rec.pending().rows();
    let cur = &mut tap.archive;
    if serial != cur.serial || rows.len() < cur.done.max(cur.last.unwrap_or(0)) {
        // A save took the rows (they are in the archive now): start over.
        *cur = ArchiveCursor {
            serial,
            names: cur.names,
            ..ArchiveCursor::default()
        };
        tap.mailbox
            .units
            .retain(|k, _| k.kind != part_kind::ARCHIVE_ROWS || k.unit == ARCHIVE_NAMES);
        tap.mailbox.archive_units = Some(0);
    }
    let dict = rec.dictionary();
    let names = (dict.kinds().len(), dict.frames().len());
    let key = |unit| PartKey {
        kind: part_kind::ARCHIVE_ROWS,
        layer: 0,
        unit,
        region: None,
    };
    let t = Instant::now();
    if cur.names != Some(names) {
        cur.names = Some(names);
        // One key per unit, so the part's own key is the directory's.
        let mut w = PartWriter::new(part_kind::ARCHIVE_ROWS, 0, 1, false);
        let mut b = Writer::new();
        for (tag, list) in [(0u8, dict.kinds()), (1u8, dict.frames())] {
            for (i, name) in list.iter().enumerate() {
                b.clear();
                b.u8(tag);
                b.var_u64(i as u64);
                b.bytes(name.as_bytes());
                w.push_raw(
                    ARCHIVE_NAMES,
                    0,
                    [0.0, 0.0],
                    row_kind::ARCHIVE_NAME,
                    b.as_bytes(),
                );
            }
        }
        put_archive(tap, key(ARCHIVE_NAMES), w, now, c, t);
        return true;
    }
    if cur.last == Some(rows.len()) && cur.done <= rows.len() {
        return false;
    }
    let u = cur.done / ARCHIVE_PER_UNIT;
    let end = rows.len().min((u + 1) * ARCHIVE_PER_UNIT);
    let mut w = PartWriter::new(part_kind::ARCHIVE_ROWS, 0, 1, false);
    for raw in &rows[u * ARCHIVE_PER_UNIT..end] {
        let (x, z) = rtsim_archive::row::points(raw)[0];
        let frame = u32::from(rtsim_archive::row::frame(raw));
        w.push_raw(
            u as u64,
            frame,
            [f64::from(x), f64::from(z)],
            row_kind::ARCHIVE_ROW,
            raw,
        );
    }
    if end == (u + 1) * ARCHIVE_PER_UNIT {
        cur.done = end;
    } else {
        cur.last = Some(rows.len());
    }
    tap.mailbox.archive_units = Some(rows.len().div_ceil(ARCHIVE_PER_UNIT) as u64);
    put_archive(tap, key(u as u64), w, now, c, t);
    true
}

fn put_archive(tap: &mut Tap, key: PartKey, w: PartWriter, now: i64, c: &Counters, t: Instant) {
    let parts: Vec<_> = w.finish().into_iter().map(|(k, p)| (k.region, p)).collect();
    let took = nanos(t.elapsed());
    add(&c.units, 1);
    add(&c.unit_nanos, took);
    max(&c.unit_nanos_max, took);
    let rows = parts.iter().map(|(_, p)| u64::from(p.rows)).sum::<u64>();
    c.layer_cost(specs().len(), rows, took);
    tap.mailbox.units.insert(
        key,
        Encoded {
            resident: true,
            at: now,
            parts,
        },
    );
}

/// Once a period: the mailbox to the IO lane, unless a commit is outstanding or the lane is busy.
fn maybe_commit(e: &Arc<Engine>, tap: &mut Tap) {
    let now = Instant::now();
    if tap.period_at.is_some_and(|t| now - t < PERIOD) {
        return;
    }
    tap.period_at = Some(now);
    if tap.shared.committing.load(Ordering::Acquire) || !io_idle(e) {
        add(&e.stats.tap.commits_deferred, 1);
        return;
    }
    let out = take_mailbox(tap);
    tap.shared.committing.store(true, Ordering::Release);
    let (e2, sh) = (Arc::clone(e), Arc::clone(&tap.shared));
    if !io(e, move || {
        commit(&e2, &sh, out);
        sh.committing.store(false, Ordering::Release);
    }) {
        tap.shared.committing.store(false, Ordering::Release);
        add(&e.stats.tap.failures, 1);
    }
}

/// The mailbox's contents; the latest scalars stay for the next period.
fn take_mailbox(tap: &mut Tap) -> Mailbox {
    let m = &tap.mailbox;
    let keep = Mailbox {
        epoch: m.epoch,
        game_time: m.game_time,
        cycles: m.cycles,
        cycle_ms: m.cycle_ms,
        archive_serial: m.archive_serial,
        ..Mailbox::default()
    };
    std::mem::replace(&mut tap.mailbox, keep)
}

/// TEST mode: the lease read now, every unit encoded once (a whole cycle, no budget) with the
/// archive's pending rows, and committed, waited for. Errors when no lease is live.
pub(super) fn flush(e: &Arc<Engine>) -> Result<(), String> {
    let sh = Arc::clone(&lock(&e.state).tap.shared);
    {
        let (e2, sh2) = (Arc::clone(e), Arc::clone(&sh));
        e.run_io_waiting(move || stat(&e2, &sh2))?;
    }
    if !sh.leased.load(Ordering::Acquire) {
        return Err("no live view lease".into());
    }
    let out = {
        let mut guard = lock(&e.state);
        let st = &mut *guard;
        let epoch = sh.epoch.load(Ordering::Acquire);
        if epoch != st.tap.epoch {
            restart(&mut st.tap, epoch);
        }
        st.tap.queue.clear();
        st.tap.pos = 0;
        st.tap.archive_at = None;
        st.tap.focus.clear();
        st.tap.sample_queue.clear();
        run(st, &e.stats.tap, true, &mut || None);
        st.tap.period_at = Some(Instant::now());
        take_mailbox(&mut st.tap)
    };
    let (e2, sh2) = (Arc::clone(e), Arc::clone(&sh));
    e.run_io_waiting(move || commit(&e2, &sh2, out))
}

// ---- the IO lane --------------------------------------------------------------------------------

/// The lease's `stat` (a read only when its time changed), a new epoch on a new lease, and the
/// deletes of `view/`.
fn stat(e: &Engine, sh: &Shared) {
    if e.dead.load(Ordering::Acquire) {
        return;
    }
    let c = &e.stats.tap;
    let rtsim = e.cfg.pages_dir.as_path();
    let mut io = lock(&sh.io);
    add(&c.lease_stats, 1);
    let live = read_lease(&mut io, sh, rtsim);
    let was = sh.leased.load(Ordering::Acquire);
    if live && (!was || io.lease_pid != io.epoch_pid) {
        let epoch = unix_ms().max(io.last_epoch + 1);
        io.last_epoch = epoch;
        io.begin(epoch);
        for old in vstore::epochs(rtsim) {
            if old != epoch
                && let Err(err) = std::fs::remove_dir_all(vstore::epoch_dir(rtsim, old))
            {
                add(&c.failures, 1);
                log_warn(&format!("view tap: could not delete an older epoch: {err}"));
            }
        }
        io.unleased_since = None;
        sh.epoch.store(epoch, Ordering::Release);
        sh.leased.store(true, Ordering::Release);
        c.epoch.store(epoch, Ordering::Release);
        c.leased.store(1, Ordering::Release);
    } else if !live && was {
        sh.leased.store(false, Ordering::Release);
        io.store = None;
        let now = SystemTime::now();
        io.unleased_since = Some(
            io.lease_mtime
                .and_then(|m| m.checked_add(LEASE_LIFE))
                .map_or(now, |end| end.min(now)),
        );
        c.leased.store(0, Ordering::Release);
    }
    if !live {
        let due = !io.started
            || io.delete_due
            || io.unleased_since.is_some_and(|t| {
                SystemTime::now()
                    .duration_since(t)
                    .is_ok_and(|d| d >= DELETE_AFTER)
            });
        let view = rtsim.join(vstore::VIEW_DIR);
        if due && view.exists() {
            match std::fs::remove_dir_all(&view) {
                Ok(()) => {
                    io.delete_due = false;
                    add(&c.deletes, 1);
                }
                Err(err) => {
                    // Windows refuses while a reader holds a file open: tried again next stat.
                    io.delete_due = true;
                    add(&c.failures, 1);
                    log_warn(&format!(
                        "view tap: could not delete {}: {err}",
                        view.display()
                    ));
                }
            }
        } else if due {
            io.delete_due = false;
        }
    }
    io.started = true;
}

/// Whether the lease is live; its focus is read when its time changed.
fn read_lease(io: &mut Io, sh: &Shared, rtsim: &Path) -> bool {
    let mtime = std::fs::metadata(rtsim.join(vstore::LEASE_FILE)).and_then(|m| m.modified());
    let Ok(mtime) = mtime else {
        io.lease_mtime = None;
        io.lease_ok = false;
        return false;
    };
    if io.lease_mtime != Some(mtime) {
        io.lease_mtime = Some(mtime);
        match Lease::read(rtsim) {
            Ok(l) => {
                *lock(&sh.focus) = l.focus;
                io.lease_pid = l.pid;
                io.lease_ok = true;
            }
            Err(_) => io.lease_ok = false,
        }
    }
    io.lease_ok
        && SystemTime::now()
            .duration_since(mtime)
            .map_or(true, |age| age < LEASE_LIFE)
}

/// One period's commit: the changed parts, the dropped units' deletes, the header.
fn commit(e: &Engine, sh: &Shared, out: Mailbox) {
    if e.dead.load(Ordering::Acquire) {
        return;
    }
    let c = &e.stats.tap;
    let t0 = Instant::now();
    let mut guard = lock(&sh.io);
    let io = &mut *guard;
    if out.epoch == 0 || out.epoch != io.epoch || !sh.leased.load(Ordering::Acquire) {
        return;
    }
    if let Some(l) = out.layers {
        io.layers = Some(l);
        io.layers_written = false;
    }
    if out.marks.is_some() {
        io.marks = out.marks;
    }
    if out.archive_units.is_some() {
        io.archive_units = out.archive_units;
    }
    if out.samples.is_some() {
        io.samples = out.samples;
    }
    io.empty.extend(out.empty);
    if io.store.is_none() {
        let dir = vstore::epoch_dir(&e.cfg.pages_dir, io.epoch);
        match PageStore::open(&dir, Durability::NoSync) {
            Ok(s) => io.store = Some(s),
            Err(err) => {
                add(&c.failures, 1);
                log_warn(&format!("view tap: cannot open {}: {err}", dir.display()));
                return;
            }
        }
    }
    let part_key = |page| PageKey::new(vstore::PART_TABLE, page);
    let mut batch = SaveBatch::new();
    let mut units = io.units.clone();
    let mut next_page = io.next_page;
    let mut changed: BTreeSet<PartKey> = BTreeSet::new();
    let (mut written, mut same) = (0u64, 0u64);
    units.retain(|k, u| {
        if io.keeps(k) {
            return true;
        }
        for p in &u.parts {
            batch.delete(part_key(p.page));
        }
        changed.insert(*k);
        false
    });
    for (k, enc) in out.units {
        if !io.keeps(&k) {
            continue;
        }
        let old = units.remove(&k);
        let mut parts = Vec::with_capacity(enc.parts.len());
        for (region, part) in enc.parts {
            let prev = old
                .as_ref()
                .and_then(|u| u.parts.iter().find(|p| p.region == region));
            let page = match prev {
                Some(p) => p.page,
                None => {
                    next_page += 1;
                    next_page - 1
                }
            };
            if prev.is_some_and(|p| p.crc == part.crc && p.rows == part.rows) {
                same += 1;
            } else {
                batch.put(part_key(page), &part.bytes);
                written += 1;
                changed.insert(k);
            }
            parts.push(PartEntry {
                region,
                page,
                crc: part.crc,
                rows: part.rows,
            });
        }
        match &old {
            Some(old) => {
                for p in &old.parts {
                    if !parts.iter().any(|n| n.region == p.region) {
                        batch.delete(part_key(p.page));
                        changed.insert(k);
                    }
                }
                if old.resident != enc.resident {
                    changed.insert(k);
                }
            }
            None => {
                changed.insert(k);
            }
        }
        units.insert(
            k,
            UnitEntry {
                key: k,
                resident: enc.resident,
                at: enc.at,
                parts,
            },
        );
    }
    let n_layers = specs().len();
    let mut versions = io.versions.clone();
    versions.resize(n_layers.max(versions.len()), 0);
    let mut archive_version = io.archive_version;
    for k in &changed {
        match k.kind {
            part_kind::MARKS => {
                if let Some(v) = versions.get_mut(usize::from(k.layer)) {
                    *v += 1;
                }
            }
            part_kind::ARCHIVE_ROWS => archive_version += 1,
            _ => {}
        }
    }
    let mut layers: Vec<LayerState> = versions
        .iter()
        .enumerate()
        .map(|(i, v)| LayerState {
            version: *v,
            per_unit: specs().get(i).map_or(0, |s| s.per_unit),
            ..LayerState::default()
        })
        .collect();
    for u in units.values() {
        if u.key.kind == part_kind::MARKS
            && let Some(l) = layers.get_mut(usize::from(u.key.layer))
        {
            l.rows += u.parts.iter().map(|p| u64::from(p.rows)).sum::<u64>();
        }
    }
    for (i, why) in &io.empty {
        if let Some(l) = layers.get_mut(usize::from(*i)) {
            l.empty.clone_from(why);
        }
    }
    if !io.layers_written
        && let Some(j) = &io.layers
    {
        batch.put(PageKey::new(vstore::LAYERS_TABLE, 0), j);
    }
    let now = Instant::now();
    let tps = match io.last {
        Some((t, g)) if now > t => (out.game_time - g) as f64 / (now - t).as_secs_f64(),
        _ => 0.0,
    };
    let mut stats = c.snapshot();
    stats.push(("archive_serial".into(), out.archive_serial));
    stats.push(("archive_version".into(), archive_version));
    stats.push(("parts_written_now".into(), written));
    let header = Header {
        epoch: io.epoch,
        commit: io.commits + 1,
        game_time: out.game_time,
        wall_ms: unix_ms(),
        tps,
        build: BUILD.into(),
        cycle_ms: out.cycle_ms,
        cycles: out.cycles,
        layers,
        units: units.values().cloned().collect(),
        stats,
    };
    batch.put(PageKey::new(vstore::HEADER_TABLE, 0), &header.encode());
    let store = io.store.as_mut().expect("opened above");
    match store.commit(batch, 0) {
        Ok(_) => {
            io.units = units;
            io.next_page = next_page;
            io.versions = versions;
            io.archive_version = archive_version;
            io.commits += 1;
            io.layers_written = true;
            io.last = Some((now, out.game_time));
            add(&c.commits, 1);
            add(&c.parts_written, written);
            add(&c.parts_unchanged, same);
        }
        Err(err) => {
            // Whatever reached the disk, the next commit rewrites from what is known stored.
            io.store = None;
            add(&c.failures, 1);
            log_warn(&format!("view tap: commit failed: {err}"));
        }
    }
    max(&c.commit_nanos_max, nanos(t0.elapsed()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_name_every_value_once() {
        let c = Counters::default();
        c.units.store(7, Ordering::Release);
        let s = c.snapshot();
        let names: BTreeSet<&str> = s.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names.len(), s.len());
        assert!(s.contains(&("units".to_owned(), 7)));
        assert!(s.contains(&("budget_nanos".to_owned(), 250_000)));
        c.reset_measurements();
        assert_eq!(c.units.load(Ordering::Acquire), 0);
    }

    #[test]
    fn io_keeps_only_the_units_there_are() {
        let mut io = Io::default();
        let m = |u| PartKey::unit(1, u);
        let a = |u| PartKey {
            kind: part_kind::ARCHIVE_ROWS,
            layer: 0,
            unit: u,
            region: None,
        };
        assert!(io.keeps(&m(5)) && io.keeps(&a(9)));
        io.marks = Some([m(1)].into());
        io.archive_units = Some(2);
        assert!(io.keeps(&m(1)) && !io.keeps(&m(5)));
        assert!(io.keeps(&a(1)) && !io.keeps(&a(2)) && io.keeps(&a(ARCHIVE_NAMES)));
    }

    #[test]
    fn the_focus_pass_takes_the_units_last_seen_in_a_focus_region() {
        let mut tap = Tap::default();
        let r = |rx, rz| RegionKey { frame: 0, rx, rz };
        let (a, b, c) = (
            PartKey::unit(1, 3),
            PartKey::unit(2, 0),
            PartKey::unit(1, 9),
        );
        tap.regions.insert(c, vec![r(0, 0), r(4, 4)]);
        tap.regions.insert(a, vec![r(0, 0)]);
        tap.regions.insert(b, vec![r(1, 0)]);
        let sample = |reg: RegionKey| PartKey {
            kind: part_kind::SAMPLES,
            layer: 0,
            unit: reg.pack(),
            region: None,
        };
        let enc = || Encoded {
            resident: true,
            at: 0,
            parts: Vec::new(),
        };
        tap.mailbox.units.insert(sample(r(1, 0)), enc());
        tap.mailbox.units.insert(sample(r(4, 4)), enc());
        tap.mailbox.units.insert(a, enc());
        *lock(&tap.shared.focus) = vec![r(4, 4), r(0, 0)];
        refill_focus(&mut tap, Instant::now());
        // Units in a focus region, in key order; each focus region's samples.
        assert_eq!(Vec::from(tap.focus.clone()), vec![a, c]);
        assert_eq!(Vec::from(tap.sample_queue.clone()), vec![r(0, 0), r(4, 4)]);
        assert_eq!(
            tap.mailbox.samples,
            Some([r(0, 0).pack(), r(4, 4).pack()].into())
        );
        // A region out of focus loses its samples' slot, not the units'.
        assert!(!tap.mailbox.units.contains_key(&sample(r(1, 0))));
        assert!(tap.mailbox.units.contains_key(&sample(r(4, 4))));
        assert!(tap.mailbox.units.contains_key(&a));
    }
}
