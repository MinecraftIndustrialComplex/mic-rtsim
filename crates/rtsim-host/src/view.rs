// SPDX-License-Identifier: AGPL-3.0-or-later
//! The host source (VIEWER.md §4.1): the viewer served by `rtsim-cli run --view`, in the engine's
//! process.
//!
//! V1 draws its scenes straight from a decoded save and has no part format yet (VIEWER.md §16, "V1
//! as built"), so the host shows its fork's own saves: whenever the fork's page log moves (a save,
//! or its confirmation), a reader thread reopens the fork read only, exactly as the save viewer
//! opens a world (rtsim-view's [`View`]), swaps the new view in, and open streams get a frame
//! notice. While a page holds a stream open (the lease, VIEWER.md §11) the host also saves every
//! [`LEASE_SAVE_EVERY`] of wall time, so the map trails the engine by about that and one reread;
//! with no page open, nothing beyond the 10-day saves is written or read.
//!
//! Like every source, it only reads: the page cannot change the run (speed, pause and stop are the
//! terminal's, VIEWER.md §12.5).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mic_viewer::api::{Card, EventsAnswer, Freshness, Histogram, Layers, Tick, WorldInfo};
use mic_viewer::http::{Reply, Request, Response};
use mic_viewer::scene::Scene;
use mic_viewer::tiles::{Rendered, TileCache, TileError, TileSource, Tiles};
use mic_viewer::viewer::{Backend, Changes, EventQuery};
use rtsim_kernel::page::Generation;
use rtsim_view::view::HeapProbe;
use rtsim_view::{Limits, View};
use serde_json::json;

use crate::run::{Control, DAY, Speed};
use crate::save;

/// While a page is open, the host saves at least this often in wall time (TUNABLE): the map's lag
/// behind the engine, about, plus one reread.
pub const LEASE_SAVE_EVERY: Duration = Duration::from_secs(2);
/// How often the reader looks at the fork's page log.
const POLL: Duration = Duration::from_millis(200);
/// The engine's rate is measured over this much wall time.
const RATE_WINDOW: Duration = Duration::from_secs(2);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The view shown now; HTTP threads clone the `Arc` and serve from it while a reread replaces it.
type Shown = Arc<RwLock<Arc<View>>>;

fn current(shown: &Shown) -> Arc<View> {
    Arc::clone(&shown.read().unwrap_or_else(|e| e.into_inner()))
}

/// The page store's root and logs as they stand (name, length, modification time): a save and a
/// confirmation each append to the newest log, a checkpoint rewrites the root.
fn stamp(pages: &Path) -> Vec<(String, u64, SystemTime)> {
    let mut out: Vec<_> = std::fs::read_dir(pages)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name != "manifest.rtm" && !name.starts_with("mlog-") {
                return None;
            }
            let m = e.metadata().ok()?;
            Some((name, m.len(), m.modified().ok()?))
        })
        .collect();
    out.sort();
    out
}

/// What the reader knows, for `/api/world` and `/api/status`.
struct State {
    /// The stamp the shown view was read at (`None`: read again at the next poll).
    stamp: Option<Vec<(String, u64, SystemTime)>>,
    /// The shown save's generation and when it was written (its log's modification time, ms).
    generation: Generation,
    written_ms: i64,
    rereads: u64,
    /// Rereads that found the save already shown (a save not confirmed yet).
    unchanged: u64,
    last_reread: Duration,
    longest_reread: Duration,
    error: Option<String>,
    /// (when, game time) samples for the engine's rate.
    samples: Vec<(Instant, i64)>,
    last_lease_save: Option<Instant>,
    lease_saves: u64,
}

/// The base map of whichever view is shown, behind one cache of the host's.
struct CurrentTiles(Shown);

impl CurrentTiles {
    fn source(&self, layer: &str) -> Option<Arc<dyn TileSource>> {
        let v = current(&self.0);
        v.tiles(layer).map(|t| Arc::clone(&t.source))
    }
}

impl TileSource for CurrentTiles {
    fn render(&self, layer: &str, z: i32, x: i64, y: i64) -> Result<Rendered, TileError> {
        match self.source(layer) {
            Some(s) => s.render(layer, z, x, y),
            None => Err(TileError::Missing(format!("no base map layer {layer}"))),
        }
    }

    fn opaque(&self, layer: &str) -> bool {
        self.source(layer).is_some_and(|s| s.opaque(layer))
    }

    fn version(&self) -> u32 {
        let v = current(&self.0);
        let first = v
            .tile_layers()
            .iter()
            .chain(v.surveyor_layers())
            .next()
            .cloned();
        first
            .and_then(|l| v.tiles(&l).map(|t| t.source.version()))
            .unwrap_or(0)
    }
}

/// The host source: mic-viewer's [`Backend`] over the fork's latest save, reread after each one.
pub struct HostView {
    dir: PathBuf,
    name: String,
    limits: Limits,
    heap: Option<HeapProbe>,
    ctl: &'static Control,
    speed: Speed,
    shown: Shown,
    tiles: Tiles,
    changes: Changes,
    open: AtomicUsize,
    stop: AtomicBool,
    state: Mutex<State>,
}

impl HostView {
    /// Reads the fork in `dir` as last saved. `ctl` is the run's; `speed` the options' default.
    pub fn open(
        dir: &Path,
        ctl: &'static Control,
        speed: Speed,
        limits: Limits,
        heap: Option<HeapProbe>,
    ) -> Result<Arc<HostView>, String> {
        let pages = dir.join(save::PAGES);
        let at = stamp(&pages);
        let t0 = Instant::now();
        let view = read(dir, limits, heap.as_ref())?;
        let took = t0.elapsed();
        let generation = view.saved().generation;
        let shown: Shown = Arc::new(RwLock::new(Arc::new(view)));
        let tiles = Tiles::new(
            Arc::new(CurrentTiles(Arc::clone(&shown))),
            TileCache::in_memory(limits.tile_cache),
            false,
        );
        let name = dir
            .file_name()
            .map_or_else(|| "fork".into(), |n| n.to_string_lossy().into_owned());
        Ok(Arc::new(HostView {
            dir: dir.to_path_buf(),
            name,
            limits,
            heap,
            ctl,
            speed,
            shown,
            tiles,
            changes: Changes::new(),
            open: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            state: Mutex::new(State {
                written_ms: written_ms(&at),
                stamp: Some(at),
                generation,
                rereads: 1,
                unchanged: 0,
                last_reread: took,
                longest_reread: took,
                error: None,
                samples: Vec::new(),
                last_lease_save: None,
                lease_saves: 0,
            }),
        }))
    }

    /// Starts the reader: it rereads the fork after each save and keeps the lease's saves coming.
    pub fn start(self: &Arc<Self>) -> std::io::Result<JoinHandle<()>> {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("rtsim-host-view".into())
            .spawn(move || {
                while !me.stop.load(Ordering::Acquire) {
                    std::thread::sleep(POLL);
                    me.poll();
                }
            })
    }

    /// Stops the reader at its next poll.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// The view shown now.
    pub fn view(&self) -> Arc<View> {
        current(&self.shown)
    }

    /// Rereads so far (the first read included), and how many found nothing newer.
    pub fn rereads(&self) -> (u64, u64) {
        let s = lock(&self.state);
        (s.rereads, s.unchanged)
    }

    /// The last reread's error, if the last one failed.
    pub fn error(&self) -> Option<String> {
        lock(&self.state).error.clone()
    }

    /// The generation shown.
    pub fn generation(&self) -> Generation {
        lock(&self.state).generation
    }

    /// Saves asked for by the lease.
    pub fn lease_saves(&self) -> u64 {
        lock(&self.state).lease_saves
    }

    /// The longest reread so far.
    pub fn longest_reread(&self) -> Duration {
        lock(&self.state).longest_reread
    }

    /// One poll: the rate sample, the lease's save, and a reread if the page log moved.
    pub fn poll(&self) {
        let now = Instant::now();
        let paused = self.ctl.paused.load(Ordering::Relaxed);
        {
            let mut s = lock(&self.state);
            s.samples
                .push((now, self.ctl.game_time.load(Ordering::Relaxed)));
            let keep = s
                .samples
                .iter()
                .rposition(|(t, _)| now.duration_since(*t) >= RATE_WINDOW)
                .unwrap_or(0);
            s.samples.drain(..keep);
            if self.open.load(Ordering::Acquire) > 0
                && !paused
                && s.last_lease_save
                    .is_none_or(|t| now.duration_since(t) >= LEASE_SAVE_EVERY)
            {
                self.ctl.save_now.store(true, Ordering::Relaxed);
                s.last_lease_save = Some(now);
                s.lease_saves += 1;
            }
        }
        let at = stamp(&self.dir.join(save::PAGES));
        if lock(&self.state).stamp.as_ref() == Some(&at) {
            return;
        }
        let t0 = Instant::now();
        let read = read(&self.dir, self.limits, self.heap.as_ref());
        let took = t0.elapsed();
        let mut s = lock(&self.state);
        s.rereads += 1;
        s.last_reread = took;
        s.longest_reread = s.longest_reread.max(took);
        match read {
            Ok(v) => {
                s.error = None;
                s.stamp = Some(at.clone());
                let generation = v.saved().generation;
                if generation == s.generation {
                    // The new save is not confirmed yet: its confirmation moves the log again.
                    s.unchanged += 1;
                    return;
                }
                s.generation = generation;
                s.written_ms = written_ms(&at);
                drop(s);
                *self.shown.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(v);
                self.changes.publish();
            }
            Err(e) => {
                // Read again at the next poll (a compaction can remove a segment mid-read).
                s.stamp = None;
                s.error = Some(e.replace(&self.dir.display().to_string(), &self.name));
            }
        }
    }

    /// The engine's game ticks per wall second over the last [`RATE_WINDOW`] (0 while paused).
    fn tps(&self, s: &State) -> f64 {
        if self.ctl.paused.load(Ordering::Relaxed) {
            return 0.0;
        }
        match (s.samples.first(), s.samples.last()) {
            (Some(a), Some(b)) if b.0 > a.0 => (b.1 - a.1) as f64 / (b.0 - a.0).as_secs_f64(),
            _ => self.ctl.speed(self.speed).ticks_per_second().unwrap_or(0.0),
        }
    }

    fn host_status(&self) -> serde_json::Value {
        let s = lock(&self.state);
        let t = self.ctl.game_time.load(Ordering::Relaxed);
        json!({
            "engine_tick": t,
            "engine_day": t as f64 / DAY as f64,
            "speed": self.ctl.speed(self.speed).name(),
            "paused": self.ctl.paused.load(Ordering::Relaxed),
            "ticks_per_second": self.tps(&s),
            "shown_save_seq": s.generation.save_seq,
            "shown_obs_seq": s.generation.obs_seq,
            "rereads": s.rereads,
            "rereads_unchanged": s.unchanged,
            "last_reread_ms": s.last_reread.as_secs_f64() * 1e3,
            "longest_reread_ms": s.longest_reread.as_secs_f64() * 1e3,
            "streams": self.open.load(Ordering::Acquire),
            "lease_saves": s.lease_saves,
            "lease_save_every_ms": LEASE_SAVE_EVERY.as_millis() as u64,
            "error": s.error,
        })
    }
}

/// The newest modification time in a stamp, ms since the epoch.
fn written_ms(at: &[(String, u64, SystemTime)]) -> i64 {
    at.iter()
        .map(|(_, _, t)| ms(*t))
        .max()
        .unwrap_or_else(|| ms(SystemTime::now()))
}

fn read(dir: &Path, limits: Limits, heap: Option<&HeapProbe>) -> Result<View, String> {
    let v = View::open(dir, limits)?;
    Ok(match heap {
        Some(h) => v.with_heap(Arc::clone(h)),
        None => v,
    })
}

impl Backend for HostView {
    fn world(&self) -> WorldInfo {
        let v = self.view();
        let mut w = v.world();
        let s = lock(&self.state);
        let paused = self.ctl.paused.load(Ordering::Relaxed);
        let engine = self.ctl.game_time.load(Ordering::Relaxed);
        let speed = self.ctl.speed(self.speed).name();
        let now = ms(SystemTime::now());
        let open = self.open.load(Ordering::Acquire) > 0;
        w.source = "host".into();
        w.stream = true;
        w.build = format!("rtsim-host {}", env!("CARGO_PKG_VERSION"));
        w.source_note = Some(format!(
            "the headless host on the fork {}: the engine at day {:.2}, {speed}{}; shown: its save \
             {} (day {:.2}), reread after each save{}",
            self.name,
            engine as f64 / DAY as f64,
            if paused { ", paused" } else { "" },
            s.generation.save_seq,
            w.time.tick as f64 / DAY as f64,
            if open {
                format!(
                    ", saved every {} s while a page is open",
                    LEASE_SAVE_EVERY.as_secs()
                )
            } else {
                String::new()
            }
        ));
        w.time.wall_ms = s.written_ms;
        w.time.tps = self.tps(&s);
        w.time.running = !paused && !self.ctl.stop.load(Ordering::Relaxed);
        w.freshness = Freshness {
            age_ms: (now - s.written_ms).max(0) as u64,
            cycle_ms: (open && !paused)
                .then(|| (LEASE_SAVE_EVERY + s.last_reread + POLL).as_millis() as u64),
            note: if paused {
                "paused".into()
            } else if open {
                String::new()
            } else {
                "a save every 10 game days".into()
            },
        };
        w
    }

    fn layers(&self) -> Arc<Layers> {
        self.view().layers()
    }

    fn scene(&self, frame: &str, t: Option<Tick>) -> Option<Arc<Scene>> {
        self.view().scene(frame, t)
    }

    fn card(&self, layer: &str, id: &str, t: Option<Tick>) -> Option<Card> {
        self.view().card(layer, id, t)
    }

    fn events(&self, q: &EventQuery) -> EventsAnswer {
        self.view().events(q)
    }

    fn histogram(&self, q: &EventQuery) -> Histogram {
        self.view().histogram(q)
    }

    fn tiles(&self, source: &str) -> Option<&Tiles> {
        // HABITAT and Surveyor alike, through the host's one cache.
        self.view().tiles(source).is_some().then_some(&self.tiles)
    }

    fn changes(&self) -> Option<&Changes> {
        Some(&self.changes)
    }

    fn watchers(&self, open: usize) {
        let was = self.open.swap(open, Ordering::AcqRel);
        if was == 0 && open > 0 {
            // A page just opened: save soon, not a lease period from the last one.
            lock(&self.state).last_lease_save = None;
        }
    }

    fn extra(&self, req: &Request) -> Option<Reply> {
        match req.path.as_str() {
            "/api/status" => {
                let mut status = self.view().status();
                status["source"] = json!("host");
                status["host"] = self.host_status();
                Some(Response::json_of(&status).into())
            }
            _ => None,
        }
    }
}
