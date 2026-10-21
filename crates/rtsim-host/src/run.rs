// SPDX-License-Identifier: AGPL-3.0-or-later
//! The host loop (VIEWER.md §12): the engine on a fork's pages, the Java side of a server where
//! nobody is online ([`Policy::Refuse`]), hand-overs advancing game time by a stride, paced to the
//! wall clock at 1x and 10x and unthrottled at max, a save every 10 game days.
//!
//! Each hand-over waits for the step before it (the fake Java's lockstep): the commands it wrote
//! are answered, `lastAppliedCmdSeq` moves on, and the next TICK goes out. A save follows the
//! bridge's order (docs/boundary.md §3): record `nextObsSeq - 1`, ask the engine to save through
//! it, write the journal and `level.dat`, and report WORLD_SAVED at the next hand-over.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use rtsim_industry::obs::{Conditions, Obs, Timed};
use rtsim_jni::engine::{Config, Engine, Exec};
use rtsim_jni::wire::{self, obs};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageKey, PageStore};

use crate::fork::ForkInfo;
use crate::java::{self, Java, OVERWORLD, Policy};
use crate::save::{self, Journal};

/// Ticks in a game day.
pub const DAY: i64 = 24_000;
/// Hand-over stride at 1x: one second of game time, as the bridge hands over (VIEWER.md §12.1).
pub const STRIDE_1X: i64 = 20;
/// Hand-over stride at 10x: one wall second of game time, so a viewer still sees the world move
/// every second. (`--stride` sets it, at the cost of that.)
pub const STRIDE_10X: i64 = 200;
/// Hand-over stride at max: the state hashes after any stride up to two game days matched a
/// 20-tick run (VIEWER.md §12.4, `strides_agree` and the GameTest worlds in tests/host.rs), so
/// this is chosen for cost, not exactness: a tenth of a game day keeps the host's fixed cost a
/// step (a hand-over, its wait and its answers) far below the simulation's, while a stop, a
/// save or `--until` still lands within it.
pub const STRIDE_FAST: i64 = 2_400;
/// Game time between saves (TUNABLE, VIEWER.md §12.1): 10 game days.
pub const SAVE_EVERY: i64 = 10 * DAY;
/// How long a step may run before the host gives up on it.
const STEP_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    X1,
    X10,
    Max,
}

impl Speed {
    pub fn parse(s: &str) -> Result<Speed, String> {
        match s {
            "1x" | "1" => Ok(Speed::X1),
            "10x" | "10" => Ok(Speed::X10),
            "max" => Ok(Speed::Max),
            _ => Err(format!("--speed {s}: one of 1x, 10x, max")),
        }
    }

    /// Game ticks per wall second, `None` unthrottled.
    pub fn ticks_per_second(self) -> Option<f64> {
        match self {
            Speed::X1 => Some(20.0),
            Speed::X10 => Some(200.0),
            Speed::Max => None,
        }
    }

    fn code(self) -> u8 {
        match self {
            Speed::X1 => 1,
            Speed::X10 => 2,
            Speed::Max => 3,
        }
    }

    fn of_code(c: u8) -> Option<Speed> {
        match c {
            1 => Some(Speed::X1),
            2 => Some(Speed::X10),
            3 => Some(Speed::Max),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Speed::X1 => "1x",
            Speed::X10 => "10x",
            Speed::Max => "max",
        }
    }
}

/// When the run ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// This much game time after the run's start, in ticks.
    After(i64),
    /// At this game time.
    At(i64),
}

impl Until {
    /// `N` ticks or `Nd` game days after the start; `@N` or `@Nd` for a game time.
    pub fn parse(s: &str) -> Result<Until, String> {
        let (at, v) = match s.strip_prefix('@') {
            Some(v) => (true, v),
            None => (false, s),
        };
        let bad =
            || format!("--until {s}: N ticks or Nd game days (after the start; @ for a game time)");
        let ticks = match v.strip_suffix('d') {
            Some(d) => d
                .parse::<i64>()
                .ok()
                .and_then(|d| d.checked_mul(DAY))
                .ok_or_else(bad)?,
            None => v.parse::<i64>().map_err(|_| bad())?,
        };
        if ticks < 0 {
            return Err(bad());
        }
        Ok(if at {
            Until::At(ticks)
        } else {
            Until::After(ticks)
        })
    }

    pub fn target(self, start: i64) -> i64 {
        match self {
            Until::After(d) => start.saturating_add(d),
            Until::At(t) => t,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub speed: Speed,
    /// The stride at 10x and max; `None`: [`STRIDE_10X`] and [`STRIDE_FAST`].
    pub stride: Option<i64>,
    pub until: Option<Until>,
    /// Serene Seasons' calendar for the industry module: days per season (VIEWER.md §12.1).
    pub season_days: Option<u32>,
    pub save_every: i64,
    /// The engine in TEST mode (state hash pages, restore checks; slower).
    pub test: bool,
    pub durability: Durability,
    /// The host's foreground and background pool sizes; `None`: two and two, as the tests.
    pub threads: Option<(usize, usize)>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            speed: Speed::Max,
            stride: None,
            until: None,
            season_days: None,
            save_every: SAVE_EVERY,
            test: false,
            durability: Durability::Fsync,
            threads: None,
        }
    }
}

impl Options {
    pub fn stride_for(&self, speed: Speed) -> i64 {
        match speed {
            Speed::X1 => STRIDE_1X,
            Speed::X10 => self.stride.unwrap_or(STRIDE_10X),
            Speed::Max => self.stride.unwrap_or(STRIDE_FAST),
        }
    }
}

/// Switches a terminal or a signal flips while the host runs.
pub struct Control {
    pub stop: AtomicBool,
    pub save_now: AtomicBool,
    pub paused: AtomicBool,
    /// 0: as the options say; else a [`Speed`] code.
    speed: AtomicU8,
}

impl Default for Control {
    fn default() -> Control {
        Control::new()
    }
}

impl Control {
    /// All off (for a `static`).
    pub const fn new() -> Control {
        Control {
            stop: AtomicBool::new(false),
            save_now: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            speed: AtomicU8::new(0),
        }
    }

    pub fn set_speed(&self, s: Speed) {
        self.speed.store(s.code(), Ordering::Relaxed);
    }

    fn speed(&self, default: Speed) -> Speed {
        Speed::of_code(self.speed.load(Ordering::Relaxed)).unwrap_or(default)
    }
}

/// The engine running on a fork.
pub struct Host {
    pub dir: PathBuf,
    pub info: ForkInfo,
    pub e: Arc<Engine>,
    pub j: Java,
    data_version: Option<i32>,
    has_level: bool,
    pub mask: u32,
    /// What the session's TOPOLOGY said, for the operator.
    pub topology: String,
    /// The game time of the first hand-over.
    pub start_time: i64,
    /// The game time of the last hand-over.
    pub last_handed: i64,
    pub steps: u64,
    pub saves: u64,
    last_save_at: i64,
    world_saved: Option<u64>,
    season_days: Option<u32>,
    season: Option<u32>,
    /// The deepest the Java journal and the commands in flight got (the soak's bound).
    pub peak_journal: usize,
    pub peak_in_flight: u64,
    pub wait: Duration,
}

/// Every module's state hash, as the step left it (between steps only).
pub fn state_hashes(e: &Engine) -> Vec<(&'static str, u64)> {
    let industry = e.with_industry(|i, _| {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for (k, b) in i.snapshot() {
            for byte in k.0.to_le_bytes().iter().chain(&b) {
                h = (h ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h
    });
    vec![
        ("phase1", e.phase1_state().0),
        ("dragons", e.with_dragons(|d, _| d.state_hash())),
        ("ships", e.with_ships(|s, _| s.state_hash())),
        ("herds", e.with_herds(|h, _| h.state_hash())),
        ("conflict", e.with_conflict(|c, _| c.state_hash())),
        ("world_model", e.with_world_model(|m| m.revision())),
        ("industry", industry),
    ]
}

/// What a run did.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub steps: u64,
    pub ticks: i64,
    pub wall: Duration,
    pub saves: u64,
    pub stopped: bool,
}

impl Outcome {
    pub fn game_days_per_second(&self) -> f64 {
        self.ticks as f64 / DAY as f64 / self.wall.as_secs_f64().max(1e-9)
    }

    pub fn wall_per_step(&self) -> Duration {
        self.wall / self.steps.max(1) as u32
    }
}

/// One line of progress.
#[derive(Clone, Debug)]
pub struct Status {
    pub game_time: i64,
    pub day: f64,
    pub steps: u64,
    pub saves: u64,
    pub speed: Speed,
    pub stride: i64,
    pub game_days_per_second: f64,
    pub journal: usize,
    pub in_flight: u64,
    pub refused: u64,
    pub heap_bytes: i64,
    pub agents: u64,
    pub paused: bool,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "day {:.2} (t {}), {} steps, {} saves, {} at stride {}{}: {:.2} game days/s; journal {}, commands in flight {}, answered {}; {} agents; heap {} MiB",
            self.day,
            self.game_time,
            self.steps,
            self.saves,
            self.speed.name(),
            self.stride,
            if self.paused { " (paused)" } else { "" },
            self.game_days_per_second,
            self.journal,
            self.in_flight,
            self.refused,
            self.agents,
            self.heap_bytes >> 20,
        )
    }
}

impl Host {
    /// Opens the engine on the fork in `dir` and sends the session's preamble.
    pub fn open(dir: &Path, opts: &Options) -> Result<Host, String> {
        let info = ForkInfo::read(dir)?;
        let journal = Journal::read(dir)?;
        let level = save::level(dir)?;
        let level_time = level.as_ref().and_then(save::level_time);
        let mask = save::module_mask(dir);
        let folder = save::deep_time_folder(level.as_ref());
        let layout = save::deep_time_layout(dir, &folder)?;
        let (fg, bg) = opts.threads.unwrap_or((2, 2));
        java::host_with(fg, bg);
        let mut cfg = Config::new(dir.join(save::PAGES));
        cfg.test = opts.test;
        cfg.durability = opts.durability;
        cfg.exec = Exec::Host;
        let e = Engine::open(cfg)?;
        let mut j = Java::try_open(e.clone(), journal.save)?;
        j.policy = Policy::Refuse;
        j.module_mask = mask;
        // The phase 1 world's clock is the last step's game time; a level.dat behind it (a crash
        // between the engine's save and the world's) starts where the pages are.
        let pages_now = e.with_dragons(|_, w| w.now().0) as i64;
        let industry_now = e.with_industry(|i, _| i.now().0) as i64;
        let start = level_time.unwrap_or(0).max(pages_now).max(industry_now);
        j.game_time = start;
        let topology = match &layout {
            Some(l) => {
                let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
                if !j.preamble() || !j.copy(&l.record(dim)) {
                    return Err("the session preamble does not fit a buffer".into());
                }
                format!(
                    "{} from {} ({:?})",
                    describe(&l.topology),
                    l.manifest.display(),
                    l.source
                )
            }
            None => {
                "flat (no Deep Time layout block; a Toroidal World x wrap needs Minecraft)".into()
            }
        };
        let mut host = Host {
            dir: dir.to_path_buf(),
            info,
            e,
            j,
            data_version: journal.data_version,
            has_level: level.is_some(),
            mask,
            topology,
            start_time: start,
            last_handed: start,
            steps: 0,
            saves: 0,
            last_save_at: start,
            world_saved: None,
            season_days: opts.season_days,
            season: None,
            peak_journal: 0,
            peak_in_flight: 0,
            wait: Duration::ZERO,
        };
        host.seasons();
        Ok(host)
    }

    /// Serene Seasons' calendar from `--season-days`: IND_CONDITIONS at the start and at each
    /// change, as `Conditions.java` journals it (four seasons, origin 0).
    fn seasons(&mut self) {
        let Some(days) = self.season_days.filter(|d| *d > 0) else {
            return;
        };
        let length = i64::from(days) * DAY;
        let season = ((self.j.game_time.max(0) / length) % 4) as u32;
        if self.season == Some(season) {
            return;
        }
        self.season = Some(season);
        let at = Tick(self.j.game_time.max(0) as u64);
        self.j.industry_obs(&Timed {
            at,
            obs: Obs::Conditions(Conditions {
                season,
                season_count: 4,
                season_length: length as u64,
                season_origin: Tick(0),
            }),
        });
    }

    /// Waits for the step in flight to publish its commands.
    fn wait_step(&mut self) -> Result<(), String> {
        let t0 = Instant::now();
        let mut spins = 0u32;
        while !self.j.last_step_published() {
            if let Some(err) = self.e.protocol_error() {
                return Err(format!("the engine stopped: {err}"));
            }
            let waited = t0.elapsed();
            if waited > STEP_TIMEOUT {
                return Err(format!(
                    "step {} did not finish in {} s",
                    self.j.submitted,
                    STEP_TIMEOUT.as_secs()
                ));
            }
            if spins < 2_000 {
                std::hint::spin_loop();
                spins += 1;
            } else if waited < Duration::from_millis(2) {
                std::thread::yield_now();
            } else {
                std::thread::sleep(Duration::from_micros(100));
            }
        }
        self.wait += t0.elapsed();
        Ok(())
    }

    /// The commands of the step in flight answered, then one hand-over at the current game time;
    /// the game time then moves on by `stride`, never past `limit`.
    pub fn step(&mut self, stride: i64, limit: i64) -> Result<(), String> {
        self.wait_step()?;
        self.j.apply_commands();
        if let Some(err) = self.e.protocol_error() {
            return Err(format!("the engine stopped: {err}"));
        }
        if let Some(d) = self.world_saved.take() {
            let Some(r) = self.j.reserve(obs::WORLD_SAVED, obs::world_saved::SIZE, 0) else {
                return Err("no room for WORLD_SAVED".into());
            };
            wire::put_u64(r, obs::world_saved::DURABLE_OBS_SEQ, d);
        }
        self.seasons();
        if !self.j.hand_over() {
            return Err(format!(
                "the hand-over at game time {} was refused",
                self.j.game_time
            ));
        }
        self.steps += 1;
        self.last_handed = self.j.game_time;
        self.peak_journal = self.peak_journal.max(self.j.journal.len());
        self.peak_in_flight = self.peak_in_flight.max(self.in_flight());
        let next = self.j.game_time.saturating_add(stride.max(1));
        self.j.game_time = if self.j.game_time < limit {
            next.min(limit)
        } else {
            next
        };
        Ok(())
    }

    /// Commands the engine wrote that Java has not answered yet.
    pub fn in_flight(&self) -> u64 {
        let next = self.e.stats_array()[7];
        next.saturating_sub(1)
            .saturating_sub(self.j.last_applied_cmd)
    }

    /// The bridge's save: the engine saves through `nextObsSeq - 1` at its next step, the journal
    /// and `level.dat` are written now, and WORLD_SAVED goes out at the next hand-over.
    pub fn save(&mut self) -> Result<(), String> {
        let through = self.j.next_obs - 1;
        self.e.request_save(through);
        self.write_world()?;
        self.world_saved = Some(through);
        self.last_save_at = self.j.game_time;
        self.saves += 1;
        Ok(())
    }

    fn write_world(&self) -> Result<(), String> {
        Journal {
            save: self.j.world_save(),
            data_version: self.data_version,
        }
        .write(&self.dir)?;
        if self.has_level {
            save::set_level_time(&self.dir, self.j.game_time)?;
        }
        Ok(())
    }

    /// Ends the session: the last step's commands answered, the journal written and the engine
    /// closed with a final save.
    pub fn close(mut self) -> Result<(), String> {
        self.wait_step()?;
        self.j.apply_commands();
        self.write_world()?;
        self.e.close(true, STEP_TIMEOUT)
    }

    /// Ends the session as a crash would: no final save.
    pub fn abandon(self) -> Result<(), String> {
        self.e.close(false, STEP_TIMEOUT)
    }

    pub fn status(&self, speed: Speed, stride: i64, rate: f64, paused: bool) -> Status {
        let s = self.e.stats_array();
        Status {
            game_time: self.j.game_time,
            day: self.j.game_time as f64 / DAY as f64,
            steps: self.steps,
            saves: self.saves,
            speed,
            stride,
            game_days_per_second: rate,
            journal: self.j.journal.len(),
            in_flight: self.in_flight(),
            refused: self.j.refused.values().sum(),
            heap_bytes: mic_native_rt::memory::in_use(),
            agents: s[11],
            paused,
        }
    }

    /// Runs until `opts.until`, `ctl.stop`, or an error; `report` gets a status line every
    /// `every` of wall time and at each save. Saves every `opts.save_every` of game time.
    pub fn run(
        &mut self,
        opts: &Options,
        ctl: &Control,
        every: Duration,
        mut report: impl FnMut(&Status),
    ) -> Result<Outcome, String> {
        let target = opts.until.map(|u| u.target(self.start_time));
        let limit = target.unwrap_or(i64::MAX);
        let t0 = Instant::now();
        let first = self.j.game_time;
        let steps0 = self.steps;
        let saves0 = self.saves;
        let mut speed = ctl.speed(opts.speed);
        let mut origin = (Instant::now(), self.j.game_time);
        let mut last_report = (Instant::now(), self.j.game_time);
        let mut stopped = false;
        loop {
            if target.is_some_and(|t| self.j.game_time > t) {
                break;
            }
            if ctl.stop.load(Ordering::Relaxed) {
                stopped = true;
                break;
            }
            if ctl.paused.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                origin = (Instant::now(), self.j.game_time);
                continue;
            }
            let now_speed = ctl.speed(opts.speed);
            if now_speed != speed {
                speed = now_speed;
                origin = (Instant::now(), self.j.game_time);
            }
            if let Some(tps) = speed.ticks_per_second() {
                let due =
                    origin.0 + Duration::from_secs_f64((self.j.game_time - origin.1) as f64 / tps);
                let now = Instant::now();
                if due > now {
                    // In slices, so a stop, a pause or a new speed is seen at once.
                    std::thread::sleep((due - now).min(Duration::from_millis(50)));
                    continue;
                } else if now - due > Duration::from_secs(5) {
                    // Too far behind to catch up: pace from here.
                    origin = (now, self.j.game_time);
                }
            }
            let stride = opts.stride_for(speed);
            self.step(stride, limit)?;
            let save_due = self.last_handed - self.last_save_at >= opts.save_every;
            if save_due || ctl.save_now.swap(false, Ordering::Relaxed) {
                self.save()?;
                let rate = rate_since(last_report, self.last_handed);
                report(&self.status(speed, stride, rate, false));
                last_report = (Instant::now(), self.last_handed);
            } else if last_report.0.elapsed() >= every {
                let rate = rate_since(last_report, self.last_handed);
                report(&self.status(speed, stride, rate, false));
                last_report = (Instant::now(), self.last_handed);
            }
        }
        // The step in flight finishes before the run counts as done.
        self.wait_step()?;
        Ok(Outcome {
            steps: self.steps - steps0,
            ticks: self.last_handed - first,
            wall: t0.elapsed(),
            saves: self.saves - saves0,
            stopped,
        })
    }

    /// Bytes of every table's root page (id 0) and of the core's table 1 in the fork's pages as
    /// last saved: the kept outboxes and replay state live there, so a growing outbox shows here.
    pub fn root_page_bytes(&self) -> Result<u64, String> {
        let mut s = PageStore::open_read_only(self.dir.join(save::PAGES))
            .map_err(|e| format!("the fork's pages: {e}"))?;
        s.view_confirmed()
            .map_err(|e| format!("the fork's pages: {e}"))?;
        let mut n = 0;
        for k in s.try_keys().map_err(|e| e.to_string())? {
            if k.id() == 0 || k.table() == 1 {
                n += s
                    .get_shared(PageKey(k.0))
                    .map_err(|e| e.to_string())?
                    .map_or(0, |b| b.len() as u64);
            }
        }
        Ok(n)
    }
}

fn rate_since(from: (Instant, i64), now_time: i64) -> f64 {
    (now_time - from.1) as f64 / DAY as f64 / from.0.elapsed().as_secs_f64().max(1e-9)
}

fn describe(t: &rtsim_space::topology::Topology) -> String {
    if t.is_flat() {
        "flat".into()
    } else if t.any_stopped() {
        "glued, with stopped seams".into()
    } else {
        "glued".into()
    }
}
