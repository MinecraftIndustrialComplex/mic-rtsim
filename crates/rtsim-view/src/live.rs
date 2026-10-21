// SPDX-License-Identifier: AGPL-3.0-or-later
//! The live source (VIEWER.md §4.1, §5.2, §11): `rtsim-cli view <world> --live` beside a running
//! server. The sidecar writes one file, the lease (`rtsim/view.lease`, [`LeaseWriter`]), while a
//! page holds a stream open; the engine's tap then encodes the world in its step slack and commits
//! the parts into `rtsim/view/<epoch>/` every 2 s. [`LiveView`] reads that store read only, draws
//! its parts through the one reader ([`crate::parts`]) and serves them with the last save's cards,
//! archive, base maps and whatever the engine had evicted:
//!
//! - a unit the tap met evicted state in (`resident` false) is drawn from the save's pages through
//!   the save viewer's own decode (V1), as is every unit before the tap's first whole cycle;
//! - a layer the store has no unit of after a whole cycle has nothing now;
//! - cards are the save's, with the same sections (a district's processes, a cluster's members),
//!   and the live row's fields as a "Now" section; a thing newer than the save gets a card of
//!   those fields alone;
//! - the timeline is the save's archive and the store's pending rows (§10.3) newer than the save.
//!
//! Nothing here runs a module: the store holds parts, and the save is read as the save viewer
//! reads it. Without `--live` none of this runs, and the sidecar writes nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use mic_viewer::api::{
    Card, EventKind, EventsAnswer, Field, Frame, Freshness, Histogram, Layers, Mark,
    ProjectionDesc, Section, TICKS_PER_DAY, Tick, TimeInfo, Value, WorldInfo,
};
use mic_viewer::http::{Handler, Reply, Request, Response};
use mic_viewer::scene::{LayerRows, Scene};
use mic_viewer::tiles::{Rendered, TileCache, TileError, TileSource, Tiles};
use mic_viewer::viewer::{App, Backend, Changes, EventQuery};
use rtsim_kernel::codec::Reader;
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_layers::{Sel, Spec};
use rtsim_parts::mark::kind as row_kind;
use rtsim_parts::part::kind as part_kind;
use rtsim_parts::store::{self as vstore, Header, Lease};
use rtsim_parts::{PartKey, RegionKey};
use serde_json::json;

use crate::events::{LiveRow, kind_info, live_row};
use crate::layers::{self, Ctx};
use crate::load::{Limits, Saved};
use crate::view::{HeapProbe, PAD, View, archive_frame, frame_id, frame_label, now_ms, union};

/// The lease is renewed this often while a page holds a stream open (VIEWER.md §11; the engine
/// ends a lease 30 s after its last renewal).
pub const RENEW_EVERY: Duration = Duration::from_secs(10);
/// A changed focus is written at most this often.
pub const FOCUS_EVERY: Duration = Duration::from_secs(1);
/// The lease writer's wake-up period.
const LEASE_TICK: Duration = Duration::from_millis(250);
/// How often the reader looks at the view store (TUNABLE: the engine commits every 2 s).
pub const POLL: Duration = Duration::from_millis(500);
/// The save is reread at most this often (TUNABLE): a reread decodes the whole save, and the save
/// only feeds cards, evicted units and the archive, which move with saves (minutes apart).
pub const SAVE_REREAD_EVERY: Duration = Duration::from_secs(30);
/// A view closer than this many blocks a pixel shows things one by one (the Group level ends at
/// 64, VIEWER.md §6): its regions are a focus the tap encodes first.
pub const FOCUS_BPP: f64 = 64.0;
/// A region the page looked at stays in the focus this long.
pub const FOCUS_KEEP: Duration = Duration::from_secs(10);
/// A commit older than this: the server is not ticking, or has stopped tapping.
pub const STALE_AFTER: Duration = Duration::from_secs(10);
/// The engine's archive names unit (rtsim-jni's `tap::ARCHIVE_NAMES`).
const ARCHIVE_NAMES: u64 = u64::MAX;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- the lease ----------------------------------------------------------------------------------

/// Writes `rtsim/view.lease` (the process id and the focus regions) while a page holds a stream
/// open: at once when the first stream opens, every [`RENEW_EVERY`] after, and when the focus
/// changes at most every [`FOCUS_EVERY`]. Its thread is started before the sidecar drops to idle
/// priority, so the renewals keep coming on a busy server. With no stream open it stops renewing
/// (the engine ends the lease 30 s later, so a page reloaded within that keeps its epoch); on
/// [`LeaseWriter::stop`] (and drop) it removes the lease if the lease is still this process's.
pub struct LeaseWriter {
    inner: Arc<LeaseInner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

struct LeaseInner {
    rtsim: PathBuf,
    pid: u32,
    st: Mutex<LeaseState>,
    cv: Condvar,
}

#[derive(Default)]
struct LeaseState {
    watchers: usize,
    focus: Vec<RegionKey>,
    written: Option<Vec<RegionKey>>,
    last: Option<Instant>,
    stop: bool,
    writes: u64,
    /// A lease of this process's is on disk (it may be removed on stop).
    held: bool,
    error: Option<String>,
}

impl LeaseWriter {
    /// Starts the writer for the pages folder `rtsim`. It writes nothing until a stream opens.
    pub fn start(rtsim: &Path) -> std::io::Result<Arc<LeaseWriter>> {
        let inner = Arc::new(LeaseInner {
            rtsim: rtsim.to_path_buf(),
            pid: std::process::id(),
            st: Mutex::new(LeaseState::default()),
            cv: Condvar::new(),
        });
        let me = Arc::clone(&inner);
        let t = std::thread::Builder::new()
            .name("rtsim-view-lease".into())
            .spawn(move || me.run())?;
        Ok(Arc::new(LeaseWriter {
            inner,
            thread: Mutex::new(Some(t)),
        }))
    }

    /// Open streams: the first one writes the lease at once.
    pub fn watchers(&self, open: usize) {
        let mut s = lock(&self.inner.st);
        if s.watchers == 0 && open > 0 {
            s.last = None;
        }
        s.watchers = open;
        self.inner.cv.notify_all();
    }

    /// The regions the page looks at closely, nearest first: at most [`vstore::MAX_FOCUS`] are
    /// kept (the lease holds them sorted).
    pub fn focus(&self, mut regions: Vec<RegionKey>) {
        regions.truncate(vstore::MAX_FOCUS);
        regions.sort_unstable();
        regions.dedup();
        let mut s = lock(&self.inner.st);
        if s.focus != regions {
            s.focus = regions;
            self.inner.cv.notify_all();
        }
    }

    /// Leases written so far.
    pub fn writes(&self) -> u64 {
        lock(&self.inner.st).writes
    }

    pub fn status(&self) -> serde_json::Value {
        let s = lock(&self.inner.st);
        json!({
            "pid": self.inner.pid,
            "streams": s.watchers,
            "focus_regions": s.focus.len(),
            "writes": s.writes,
            "last_write_ms_ago": s.last.map(|t| t.elapsed().as_millis() as u64),
            "renew_every_ms": RENEW_EVERY.as_millis() as u64,
            "error": s.error,
        })
    }

    /// Stops renewing and removes the lease if it is still this process's. Idempotent.
    pub fn stop(&self) {
        lock(&self.inner.st).stop = true;
        self.inner.cv.notify_all();
        if let Some(t) = lock(&self.thread).take() {
            let _ = t.join();
        }
    }
}

impl Drop for LeaseWriter {
    fn drop(&mut self) {
        self.stop();
    }
}

impl LeaseInner {
    fn run(&self) {
        let mut s = lock(&self.st);
        loop {
            if s.stop {
                if s.held {
                    self.remove();
                    s.held = false;
                }
                return;
            }
            let now = Instant::now();
            let since = |p: Duration| s.last.is_none_or(|t| now.duration_since(t) >= p);
            let due = s.watchers > 0
                && (since(RENEW_EVERY)
                    || (s.written.as_ref() != Some(&s.focus) && since(FOCUS_EVERY)));
            if due {
                let lease = Lease {
                    pid: self.pid,
                    focus: s.focus.clone(),
                };
                drop(s);
                let r = lease.write(&self.rtsim);
                s = lock(&self.st);
                s.last = Some(now);
                match r {
                    Ok(()) => {
                        s.writes += 1;
                        s.held = true;
                        s.written = Some(lease.focus);
                        s.error = None;
                    }
                    Err(e) => s.error = Some(e.to_string()),
                }
            }
            s = match self.cv.wait_timeout(s, LEASE_TICK) {
                Ok((g, _)) => g,
                Err(e) => e.into_inner().0,
            };
        }
    }

    fn remove(&self) {
        if Lease::read(&self.rtsim).is_ok_and(|l| l.pid == self.pid) {
            let _ = std::fs::remove_file(self.rtsim.join(vstore::LEASE_FILE));
        }
    }
}

/// The regions a view of `b` (`[x0, z0, x1, z1]`, blocks) in frame `frame` covers, nearest its
/// centre first, at most `cap`.
pub fn regions_in(frame: u32, b: [f64; 4], cap: usize) -> Vec<RegionKey> {
    if !b.iter().all(|v| v.is_finite()) || cap == 0 {
        return Vec::new();
    }
    let (lo, hi) = (
        RegionKey::of(frame, b[0], b[1]),
        RegionKey::of(frame, b[2], b[3]),
    );
    let c = RegionKey::of(frame, (b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let reach = (c.rx - lo.rx)
        .max(hi.rx - c.rx)
        .max(c.rz - lo.rz)
        .max(hi.rz - c.rz)
        .max(0);
    let mut out = Vec::new();
    // Ring by ring out from the centre, each ring nearest first: never more than the cap's rings.
    for d in 0..=reach {
        let mut ring: Vec<(i64, RegionKey)> = Vec::new();
        for rx in (c.rx - d).max(lo.rx)..=(c.rx + d).min(hi.rx) {
            for rz in (c.rz - d).max(lo.rz)..=(c.rz + d).min(hi.rz) {
                if (rx - c.rx).abs().max((rz - c.rz).abs()) != d {
                    continue;
                }
                let (dx, dz) = (i64::from(rx - c.rx), i64::from(rz - c.rz));
                ring.push((dx * dx + dz * dz, RegionKey { frame, rx, rz }));
            }
        }
        ring.sort_unstable();
        for (_, r) in ring {
            out.push(r);
            if out.len() == cap {
                return out;
            }
        }
    }
    out
}

// ---- what the reader publishes ------------------------------------------------------------------

/// The page store's root and logs as they stand (name, length, modification time): a commit
/// appends to the newest log, a checkpoint rewrites the root.
fn stamp(dir: &Path) -> Vec<(String, u64, SystemTime)> {
    let mut out: Vec<_> = std::fs::read_dir(dir)
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

type Stamp = Vec<(String, u64, SystemTime)>;

/// One layer as drawn: its rows per frame id, where they came from, and why it is empty.
#[derive(Clone, Default)]
struct DrawnLayer {
    sig: u64,
    rows: BTreeMap<String, LayerRows>,
    bounds: BTreeMap<String, [f64; 4]>,
    count: usize,
    why: Option<String>,
    from: &'static str,
    /// Units drawn from the save because the tap found them evicted (or had not reached them).
    from_save: usize,
}

/// What the reader drew last; HTTP threads clone the `Arc`.
#[derive(Default)]
struct Live {
    /// The last header read (its directory dropped); none before the server's first commit.
    header: Option<Header>,
    error: Option<String>,
    /// The save's tick when drawn.
    save_now: Tick,
    scenes: BTreeMap<String, Arc<Scene>>,
    frames: Vec<Frame>,
    frame_numbers: HashMap<String, u32>,
    layers: BTreeMap<String, (usize, &'static str, usize)>,
    /// The live rows' card fields by (layer, thing id).
    fields: HashMap<(String, String), Vec<Field>>,
    /// Pending archive rows (sequence number, row), oldest first, and their kinds.
    pending: Vec<(u64, LiveRow)>,
    pending_kinds: Vec<EventKind>,
    reader: serde_json::Value,
}

/// One unit's marks and card fields, decoded once per part CRC.
#[derive(Default)]
struct UnitMarks {
    sig: u64,
    marks: Vec<(u32, Mark)>,
    fields: Vec<(String, Vec<(u16, rtsim_parts::Value)>)>,
}

impl UnitMarks {
    fn read(&mut self, bytes: &[u8]) -> Result<(), String> {
        for r in rtsim_parts::read_part(bytes)?.rows {
            if let Some(m) = r.mark {
                if !r.fields.is_empty()
                    && let Some(id) = m.id()
                {
                    self.fields.push((id.to_owned(), r.fields));
                }
                self.marks.push((r.frame, crate::parts::to_viewer(m)));
            }
        }
        Ok(())
    }
}

/// The reader thread's own state.
#[derive(Default)]
struct ReadState {
    save_stamp: Stamp,
    save_read: Option<Instant>,
    store: Option<(u64, Stamp)>,
    layers_json: Option<(u64, Vec<String>)>,
    units: HashMap<PartKey, UnitMarks>,
    save_units: HashMap<(usize, u64), (UnitMarks, Option<String>)>,
    drawn: HashMap<String, DrawnLayer>,
    pending: PendingState,
    polls: u64,
    reads: u64,
    save_rereads: u64,
    last_read: Duration,
    longest_read: Duration,
    longest_save_reread: Duration,
    save_error: Option<String>,
}

#[derive(Default)]
struct PendingState {
    epoch: u64,
    serial: u64,
    /// Rows of this serial given a sequence number.
    count: usize,
    next_seq: u64,
    names_crc: Option<u32>,
    kinds: Vec<String>,
    frames: Vec<String>,
    units: BTreeMap<u64, (u32, Vec<LiveRow>)>,
    /// (sequence number, the serial its part had, row), oldest first.
    rows: Vec<(u64, u64, LiveRow)>,
}

/// The base map of whichever save is shown, behind one cache of the sidecar's.
struct CurrentTiles(Arc<RwLock<Arc<View>>>);

impl CurrentTiles {
    fn view(&self) -> Arc<View> {
        Arc::clone(&self.0.read().unwrap_or_else(|e| e.into_inner()))
    }

    fn source(&self, layer: &str) -> Option<Arc<dyn TileSource>> {
        self.view().tiles(layer).map(|t| Arc::clone(&t.source))
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
        let v = self.view();
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

/// The live source: mic-viewer's [`Backend`] over the engine's view store and the last save.
pub struct LiveView {
    dir: PathBuf,
    rtsim: PathBuf,
    name: String,
    limits: Limits,
    heap: Option<HeapProbe>,
    lease: Option<Arc<LeaseWriter>>,
    save: Arc<RwLock<Arc<View>>>,
    live: RwLock<Arc<Live>>,
    tiles: Tiles,
    changes: Changes,
    open: AtomicUsize,
    focus: Mutex<Vec<(RegionKey, Instant)>>,
    stop: AtomicBool,
    reader: Mutex<ReadState>,
}

fn read_view(dir: &Path, limits: Limits, heap: Option<&HeapProbe>) -> Result<View, String> {
    let v = View::open(dir, limits)?;
    Ok(match heap {
        Some(h) => v.with_heap(Arc::clone(h)),
        None => v,
    })
}

fn hash_of(h: impl Hash) -> u64 {
    let mut s = DefaultHasher::new();
    h.hash(&mut s);
    s.finish()
}

impl LiveView {
    /// Opens the world in `dir` (its folder or its `rtsim/`): the last save, then the view store
    /// as it stands. `lease` (none in tests that drive [`LiveView::poll`] themselves) is renewed
    /// while streams are open.
    pub fn open(
        dir: &Path,
        limits: Limits,
        heap: Option<HeapProbe>,
        lease: Option<Arc<LeaseWriter>>,
    ) -> Result<Arc<LiveView>, String> {
        let view = read_view(dir, limits, heap.as_ref())?;
        let (world_dir, rtsim) = {
            let s = view.saved();
            (s.world_dir.clone(), s.pages_dir.clone())
        };
        let save = Arc::new(RwLock::new(Arc::new(view)));
        let tiles = Tiles::new(
            Arc::new(CurrentTiles(Arc::clone(&save))),
            TileCache::in_memory(limits.tile_cache),
            false,
        );
        let me = Arc::new(LiveView {
            name: crate::load::world_name(&world_dir),
            dir: world_dir,
            rtsim: rtsim.clone(),
            limits,
            heap,
            lease,
            save,
            live: RwLock::new(Arc::new(Live::default())),
            tiles,
            changes: Changes::new(),
            open: AtomicUsize::new(0),
            focus: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            reader: Mutex::new(ReadState {
                save_stamp: stamp(&rtsim),
                save_read: Some(Instant::now()),
                ..ReadState::default()
            }),
        });
        me.poll();
        Ok(me)
    }

    /// Starts the reader thread: a poll every [`POLL`].
    pub fn start(self: &Arc<Self>) -> std::io::Result<JoinHandle<()>> {
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("rtsim-view-live".into())
            .spawn(move || {
                while !me.stop.load(Ordering::Acquire) {
                    std::thread::sleep(POLL);
                    me.poll();
                }
            })
    }

    /// Stops the reader at its next poll, and the lease.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(l) = &self.lease {
            l.stop();
        }
    }

    /// The save shown now.
    pub fn save(&self) -> Arc<View> {
        Arc::clone(&self.save.read().unwrap_or_else(|e| e.into_inner()))
    }

    fn live(&self) -> Arc<Live> {
        Arc::clone(&self.live.read().unwrap_or_else(|e| e.into_inner()))
    }

    /// The last header read, its directory dropped.
    pub fn header(&self) -> Option<Header> {
        self.live().header.clone()
    }

    /// Marks per layer, where they came from ("live", "save", "live and save") and how many units
    /// came from the save.
    pub fn layer_sources(&self) -> BTreeMap<String, (usize, &'static str, usize)> {
        self.live().layers.clone()
    }

    /// The lease writer, if this view renews one.
    pub fn lease(&self) -> Option<&Arc<LeaseWriter>> {
        self.lease.as_ref()
    }

    /// A view of `b` in `frame` at `bpp` blocks a pixel (an `/api/marks` request): a close one
    /// puts its regions first in the lease's focus.
    pub fn look(&self, frame: &str, b: [f64; 4], bpp: f64) {
        let Some(lease) = &self.lease else { return };
        if !(bpp > 0.0 && bpp < FOCUS_BPP) {
            return;
        }
        let Some(n) = self.live().frame_numbers.get(frame).copied() else {
            return;
        };
        let now = Instant::now();
        let asked = regions_in(n, b, vstore::MAX_FOCUS);
        let mut f = lock(&self.focus);
        f.retain(|(r, t)| now.duration_since(*t) < FOCUS_KEEP && !asked.contains(r));
        let mut list: Vec<(RegionKey, Instant)> = asked.iter().map(|r| (*r, now)).collect();
        list.append(&mut f);
        list.truncate(vstore::MAX_FOCUS);
        *f = list;
        lease.focus(f.iter().map(|(r, _)| *r).collect());
    }

    /// One poll: the save reread if it moved (at most every [`SAVE_REREAD_EVERY`]), the store read
    /// if it moved, and the layers redrawn whose units changed. Returns whether it published.
    pub fn poll(&self) -> bool {
        let mut r = lock(&self.reader);
        r.polls += 1;
        let t0 = Instant::now();
        let mut save_moved = false;
        let at = stamp(&self.rtsim);
        if at != r.save_stamp && r.save_read.is_none_or(|t| t.elapsed() >= SAVE_REREAD_EVERY) {
            r.save_read = Some(Instant::now());
            let t = Instant::now();
            match read_view(&self.dir, self.limits, self.heap.as_ref()) {
                Ok(v) => {
                    r.save_stamp = at;
                    r.save_error = None;
                    r.longest_save_reread = r.longest_save_reread.max(t.elapsed());
                    let old = self.save().saved().generation;
                    if v.saved().generation != old {
                        *self.save.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(v);
                        r.save_rereads += 1;
                        r.save_units.clear();
                        r.drawn.clear();
                        save_moved = true;
                    }
                }
                Err(e) => {
                    r.save_error = Some(e.replace(&self.dir.display().to_string(), &self.name))
                }
            }
        }
        let epoch = vstore::epochs(&self.rtsim).last().copied();
        let store_at = epoch.map(|e| (e, stamp(&vstore::epoch_dir(&self.rtsim, e))));
        if !save_moved && store_at == r.store && r.polls > 1 {
            return false;
        }
        let read = match epoch {
            Some(e) => read_store(&vstore::epoch_dir(&self.rtsim, e)).map(Some),
            None => Ok(None),
        };
        let (store, header, error) = match read {
            Ok(Some((s, h))) => (Some(s), Some(h), None),
            Ok(None) => (None, None, None),
            Err(e) => (None, None, Some(e)),
        };
        if error.is_none() {
            r.store = store_at;
        }
        let live = self.draw(&mut r, store.as_ref(), header, error);
        r.reads += 1;
        r.last_read = t0.elapsed();
        r.longest_read = r.longest_read.max(r.last_read);
        let mut live = live;
        live.reader = json!({
            "polls": r.polls,
            "reads": r.reads,
            "last_read_ms": r.last_read.as_secs_f64() * 1e3,
            "longest_read_ms": r.longest_read.as_secs_f64() * 1e3,
            "save_rereads": r.save_rereads,
            "longest_save_reread_ms": r.longest_save_reread.as_secs_f64() * 1e3,
            "save_error": r.save_error,
            "units_cached": r.units.len(),
        });
        drop(r);
        *self.live.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(live);
        self.changes.publish();
        true
    }

    /// Draws every layer (reusing those whose units did not change) and the pending rows.
    fn draw(
        &self,
        r: &mut ReadState,
        store: Option<&PageStore>,
        header: Option<Header>,
        error: Option<String>,
    ) -> Live {
        let save = self.save();
        let specs = layers::specs();
        let saved = save.saved();
        let names = frame_names(&saved);
        let fid = |n: u32| names.get(&n).cloned().unwrap_or_else(|| format!("dim{n}"));
        let save_now = saved.now();
        let generation = (saved.generation.save_seq, saved.generation.obs_seq);
        // The store's layer list, by index: read once an epoch.
        let epoch = header.as_ref().map_or(0, |h| h.epoch);
        let store_ids: Vec<String> = match (store, &r.layers_json) {
            (Some(_), Some((e, ids))) if *e == epoch => ids.clone(),
            (Some(s), _) => {
                let ids = s
                    .get_shared(PageKey::new(vstore::LAYERS_TABLE, 0))
                    .ok()
                    .flatten()
                    .and_then(|b| crate::parts::layers_from_json(&b).ok())
                    .map(|l| l.layers.into_iter().map(|d| d.id).collect())
                    .unwrap_or_default();
                r.layers_json = Some((epoch, ids));
                r.layers_json
                    .as_ref()
                    .map(|(_, i)| i.clone())
                    .unwrap_or_default()
            }
            (None, _) => Vec::new(),
        };
        let mut by_layer: HashMap<u16, Vec<&rtsim_parts::UnitEntry>> = HashMap::new();
        if let Some(h) = &header {
            for u in &h.units {
                if u.key.kind == part_kind::MARKS {
                    by_layer.entry(u.key.layer).or_default().push(u);
                }
            }
        }
        let first_cycle = header.as_ref().is_some_and(|h| h.cycles == 0);
        let cx = Ctx::new(&saved);
        let src = cx.src();
        let mut fields: HashMap<(String, String), Vec<Field>> = HashMap::new();
        let mut live_keys: BTreeSet<PartKey> = BTreeSet::new();
        for (si, s) in specs.iter().enumerate() {
            let id = &s.desc.id;
            let li = store_ids.iter().position(|x| x == id);
            let units: &[&rtsim_parts::UnitEntry] = li
                .and_then(|li| by_layer.get(&(li as u16)))
                .map_or(&[], Vec::as_slice);
            for u in units {
                live_keys.insert(u.key);
            }
            // Before a whole cycle, the save's units the store lacks yet come from the save.
            let save_only: Vec<u64> = if first_cycle && layers::absent(&saved, s.module).is_none() {
                let have: BTreeSet<u64> = units.iter().map(|u| u.key.unit).collect();
                catch_unwind(AssertUnwindSafe(|| s.units(&src)))
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|u| !have.contains(u))
                    .collect()
            } else {
                Vec::new()
            };
            let whole_save = header.is_none() || (units.is_empty() && first_cycle);
            let sig = hash_of((
                generation,
                whole_save,
                first_cycle,
                &save_only,
                units
                    .iter()
                    .map(|u| {
                        (
                            u.key.unit,
                            u.resident,
                            u.parts.iter().map(|p| (p.page, p.crc)).collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
                li.and_then(|li| header.as_ref()?.layers.get(li)?.empty.clone()),
            ));
            if r.drawn.get(id).is_some_and(|d| d.sig == sig) {
                if let Some(d) = r.drawn.get(id) {
                    collect_fields(&mut fields, s, id, units, &r.units, d);
                }
                continue;
            }
            let d = if whole_save {
                save_layer(&save, s, sig)
            } else {
                let mut ms: Vec<&UnitMarks> = Vec::new();
                let mut from_save = 0;
                // Decode the live parts first (the cache is borrowed immutably below).
                for u in units.iter().filter(|u| u.resident) {
                    let sig_u =
                        hash_of(u.parts.iter().map(|p| (p.page, p.crc)).collect::<Vec<_>>());
                    if r.units.get(&u.key).is_none_or(|c| c.sig != sig_u) {
                        let mut m = UnitMarks {
                            sig: sig_u,
                            ..UnitMarks::default()
                        };
                        if let Some(st) = store {
                            for p in &u.parts {
                                if let Ok(Some(b)) =
                                    st.get_shared(PageKey::new(vstore::PART_TABLE, p.page))
                                {
                                    let _ = m.read(&b);
                                }
                            }
                        }
                        r.units.insert(u.key, m);
                    }
                }
                let wanted: Vec<u64> = units
                    .iter()
                    .filter(|u| !u.resident)
                    .map(|u| u.key.unit)
                    .chain(save_only.iter().copied())
                    .collect();
                for u in &wanted {
                    if !r.save_units.contains_key(&(si, *u)) {
                        let built = save_unit(s, si as u16, &src, &saved, *u);
                        r.save_units.insert((si, *u), built);
                    }
                }
                for u in units {
                    if u.resident {
                        if let Some(m) = r.units.get(&u.key) {
                            ms.push(m);
                        }
                    } else if let Some((m, _)) = r.save_units.get(&(si, u.key.unit)) {
                        ms.push(m);
                        from_save += 1;
                    }
                }
                for u in &save_only {
                    if let Some((m, _)) = r.save_units.get(&(si, *u)) {
                        ms.push(m);
                        from_save += 1;
                    }
                }
                let why = li
                    .and_then(|li| header.as_ref()?.layers.get(li)?.empty.clone())
                    .map(|w| saved.scrub(&w));
                let mut d = finish_layer(&ms, &fid, sig);
                d.from = if from_save == 0 {
                    "live"
                } else if from_save == ms.len() {
                    "save"
                } else {
                    "live and save"
                };
                d.from_save = from_save;
                if d.count == 0 {
                    d.why = Some(why.unwrap_or_else(|| "nothing of this kind now".into()));
                }
                d
            };
            r.drawn.insert(id.clone(), d);
            if let Some(d) = r.drawn.get(id) {
                collect_fields(&mut fields, s, id, units, &r.units, d);
            }
        }
        // Units no longer in the store are dropped from the cache.
        r.units.retain(|k, _| live_keys.contains(k));
        // The frames: the save's, and any only the live marks are in.
        let t = header.as_ref().map_or(save_now, |h| h.game_time);
        let mut frames: Vec<Frame> = save.world().frames;
        let mut bounds: BTreeMap<String, [f64; 4]> = BTreeMap::new();
        for d in r.drawn.values() {
            for (f, b) in &d.bounds {
                let e = bounds.get(f).copied();
                bounds.insert(f.clone(), union(e, *b));
            }
        }
        let numbers: HashMap<String, u32> = names.iter().map(|(n, s)| (s.clone(), *n)).collect();
        for f in &mut frames {
            if let Some(b) = bounds.get(&f.id) {
                let p = [b[0] - PAD, b[1] - PAD, b[2] + PAD, b[3] + PAD];
                f.bounds = union(Some(f.bounds), p);
            }
        }
        for (f, b) in &bounds {
            if !frames.iter().any(|x| &x.id == f) {
                let n = numbers
                    .get(f)
                    .copied()
                    .or_else(|| archive_frame(f))
                    .unwrap_or(0);
                frames.push(Frame {
                    label: frame_label(f, n),
                    id: f.clone(),
                    projection: ProjectionDesc::flat(),
                    bounds: [b[0] - PAD, b[1] - PAD, b[2] + PAD, b[3] + PAD],
                    tiles: Vec::new(),
                });
            }
        }
        let mut frame_numbers = HashMap::new();
        for f in &frames {
            if let Some(n) = numbers.get(&f.id).copied().or_else(|| archive_frame(&f.id)) {
                frame_numbers.insert(f.id.clone(), n);
            }
        }
        let mut scenes = BTreeMap::new();
        let mut counts = BTreeMap::new();
        for s in &specs {
            if let Some(d) = r.drawn.get(&s.desc.id) {
                counts.insert(s.desc.id.clone(), (d.count, d.from, d.from_save));
            }
        }
        for f in &frames {
            let mut ls = HashMap::new();
            for s in &specs {
                let id = &s.desc.id;
                let Some(d) = r.drawn.get(id) else { continue };
                let rows = match d.rows.get(&f.id) {
                    Some(rows) => rows.clone(),
                    None if d.count == 0 => LayerRows::empty_because(
                        d.why
                            .clone()
                            .unwrap_or_else(|| "nothing of this kind".into()),
                    ),
                    None => LayerRows::from_marks(Vec::new()),
                };
                ls.insert(id.clone(), rows);
            }
            scenes.insert(
                f.id.clone(),
                Arc::new(Scene {
                    t,
                    period_x: 0.0,
                    layers: ls,
                }),
            );
        }
        // The pending archive rows (§10.3).
        let frame_of = |name: &str| archive_frame(name).map_or_else(|| name.to_owned(), &fid);
        let pending = pending_rows(
            &mut r.pending,
            store,
            header.as_ref(),
            saved.archive_serial(),
            &frame_of,
        );
        let known: BTreeSet<String> = save.world().event_kinds.into_iter().map(|k| k.id).collect();
        let pending_kinds: Vec<EventKind> = r
            .pending
            .kinds
            .iter()
            .filter(|k| !known.contains(*k))
            .map(|k| kind_info(k))
            .collect();
        drop(saved);
        Live {
            header: header.map(|h| Header {
                units: Vec::new(),
                ..h
            }),
            error,
            save_now,
            scenes,
            frames,
            frame_numbers,
            layers: counts,
            fields,
            pending,
            pending_kinds,
            reader: serde_json::Value::Null,
        }
    }

    fn live_status(&self) -> serde_json::Value {
        let live = self.live();
        let now = now_ms();
        let h = live.header.as_ref();
        let layers: BTreeMap<_, _> = live
            .layers
            .iter()
            .map(|(id, (n, from, saved))| {
                (
                    id.clone(),
                    json!({ "marks": n, "from": from, "units_from_save": saved }),
                )
            })
            .collect();
        json!({
            "epoch": h.map(|h| h.epoch),
            "commit": h.map(|h| h.commit),
            "game_time": h.map(|h| h.game_time),
            "age_ms": h.map(|h| (now - h.wall_ms as i64).max(0)),
            "tps": h.map(|h| h.tps),
            "cycles": h.map(|h| h.cycles),
            "cycle_ms": h.map(|h| h.cycle_ms),
            "build": h.map(|h| h.build.clone()),
            "tap": h.map(|h| h.stats.iter().cloned().collect::<BTreeMap<_, _>>()),
            "layers": layers,
            "save_game_time": live.save_now,
            "pending_rows": live.pending.len(),
            "error": live.error,
            "reader": live.reader,
            "lease": self.lease.as_ref().map(|l| l.status()),
            "streams": self.open.load(Ordering::Acquire),
        })
    }
}

/// The store's header, read only.
fn read_store(dir: &Path) -> Result<(PageStore, Header), String> {
    let s = PageStore::open_read_only(dir).map_err(|e| format!("the view store: {e}"))?;
    let b = s
        .get_shared(PageKey::new(vstore::HEADER_TABLE, 0))
        .map_err(|e| format!("the view store's header: {e}"))?
        .ok_or_else(|| "the view store has no header yet".to_string())?;
    let h = Header::decode(&b)?;
    Ok((s, h))
}

/// Frame number to the page's frame id, from the save's journal names.
fn frame_names(saved: &Saved) -> HashMap<u32, String> {
    let mut out: HashMap<u32, String> = saved
        .names
        .map
        .iter()
        .filter(|((k, _), _)| *k == crate::keys::DICT_DIMENSION)
        .map(|((_, n), _)| (*n, frame_id(saved, *n)))
        .collect();
    out.entry(0).or_insert_with(|| frame_id(saved, 0));
    out
}

/// A layer drawn whole from the save: its scenes' rows, as the save viewer drew them.
fn save_layer(save: &View, s: &Spec, sig: u64) -> DrawnLayer {
    let id = &s.desc.id;
    let mut d = DrawnLayer {
        sig,
        from: "save",
        count: save.counts().get(id).copied().unwrap_or(0),
        why: save.reasons().get(id).cloned(),
        ..DrawnLayer::default()
    };
    for f in save.frames() {
        if let Some(rows) = save
            .scene(f, None)
            .and_then(|sc| sc.layers.get(id).cloned())
            && !rows.is_empty()
        {
            let mut b: Option<[f64; 4]> = None;
            for m in rows.marks() {
                let bb = m.bbox();
                if bb.iter().all(|v| v.is_finite()) {
                    b = Some(union(b, bb));
                }
            }
            if let Some(b) = b {
                d.bounds.insert(f.to_owned(), b);
            }
            d.rows.insert(f.to_owned(), rows);
        }
    }
    d
}

/// One unit of layer `si` written from the save (an evicted unit, or one the tap has not reached).
fn save_unit(
    s: &Spec,
    si: u16,
    src: &rtsim_layers::Src,
    saved: &Saved,
    unit: u64,
) -> (UnitMarks, Option<String>) {
    if layers::absent(saved, s.module).is_some() {
        return (UnitMarks::default(), None);
    }
    let mut w = s.writer(si);
    let why = catch_unwind(AssertUnwindSafe(|| {
        s.build(src, &Sel::Keys(s.keys(unit)), &mut w)
    }))
    .ok()
    .flatten();
    let mut m = UnitMarks::default();
    for (_, p) in w.finish() {
        let _ = m.read(&p.bytes);
    }
    (m, why)
}

/// A layer's units' marks as rows per frame.
fn finish_layer(units: &[&UnitMarks], fid: &dyn Fn(u32) -> String, sig: u64) -> DrawnLayer {
    let mut by: BTreeMap<u32, Vec<Mark>> = BTreeMap::new();
    let mut bounds: BTreeMap<u32, [f64; 4]> = BTreeMap::new();
    let mut count = 0;
    for u in units {
        for (f, m) in &u.marks {
            let b = m.bbox();
            if b.iter().all(|v| v.is_finite()) {
                let e = bounds.get(f).copied();
                bounds.insert(*f, union(e, b));
            }
            by.entry(*f).or_default().push(m.clone());
            count += 1;
        }
    }
    DrawnLayer {
        sig,
        rows: by
            .into_iter()
            .map(|(f, v)| (fid(f), LayerRows::from_marks(v)))
            .collect(),
        bounds: bounds.into_iter().map(|(f, b)| (fid(f), b)).collect(),
        count,
        ..DrawnLayer::default()
    }
}

/// The live rows' card fields of one layer, labelled by its descriptor.
fn collect_fields(
    out: &mut HashMap<(String, String), Vec<Field>>,
    s: &Spec,
    id: &str,
    units: &[&rtsim_parts::UnitEntry],
    cache: &HashMap<PartKey, UnitMarks>,
    d: &DrawnLayer,
) {
    if d.from == "save" || s.desc.fields.is_empty() {
        return;
    }
    for u in units.iter().filter(|u| u.resident) {
        let Some(m) = cache.get(&u.key) else { continue };
        for (thing, fs) in &m.fields {
            let list = fs
                .iter()
                .filter_map(|(i, v)| {
                    let label = s.desc.fields.get(usize::from(*i))?;
                    Some(Field::new(label, value(v)))
                })
                .collect();
            out.insert((id.to_owned(), canon(thing)), list);
        }
    }
}

fn value(v: &rtsim_parts::Value) -> Value {
    match v {
        rtsim_parts::Value::I64(n) => Value::from(*n),
        rtsim_parts::Value::F64(n) => Value::from(*n),
        rtsim_parts::Value::Str(t) => Value::from(t.clone()),
        rtsim_parts::Value::Bool(b) => Value::from(*b),
    }
}

/// `3` and `03` are one thing: a number is kept as its value.
fn canon(id: &str) -> String {
    id.parse::<u64>()
        .map_or_else(|_| id.to_owned(), |n| n.to_string())
}

/// The pending archive rows of the store's ARCHIVE_ROWS parts, each given a sequence number the
/// first time it is seen (the stream's `events`). The parts hold the rows no save up to the
/// header's `archive_serial` took, so the save shown (`save_serial`) holds a row exactly when its
/// serial is above the row's. Rows of an earlier serial stay until the save shown is that late; a
/// tick does not tell (a leg starts after the save that recorded it).
fn pending_rows(
    p: &mut PendingState,
    store: Option<&PageStore>,
    header: Option<&Header>,
    save_serial: u64,
    frame_of: &dyn Fn(&str) -> String,
) -> Vec<(u64, LiveRow)> {
    if let (Some(st), Some(h)) = (store, header) {
        if h.epoch != p.epoch {
            // Another epoch (another sidecar, or a lease that ended): its parts start over.
            *p = PendingState {
                epoch: h.epoch,
                next_seq: p.next_seq,
                ..PendingState::default()
            };
        }
        let serial = h.stat("archive_serial").unwrap_or(0);
        if serial != p.serial {
            p.serial = serial;
            p.count = 0;
            p.units.clear();
        }
        let get = |page: u64| {
            st.get_shared(PageKey::new(vstore::PART_TABLE, page))
                .ok()
                .flatten()
        };
        let archive: Vec<_> = h
            .units
            .iter()
            .filter(|u| u.key.kind == part_kind::ARCHIVE_ROWS)
            .collect();
        if let Some(n) = archive.iter().find(|u| u.key.unit == ARCHIVE_NAMES) {
            let crc = n.parts.first().map_or(0, |p| p.crc);
            if p.names_crc != Some(crc) {
                p.names_crc = Some(crc);
                let (mut kinds, mut frames) = (Vec::new(), Vec::new());
                for part in &n.parts {
                    let Some(b) = get(part.page) else { continue };
                    let Ok(v) = rtsim_parts::read_part(&b) else {
                        continue;
                    };
                    for row in v.rows.iter().filter(|r| r.kind == row_kind::ARCHIVE_NAME) {
                        let mut rd = Reader::new(&row.body);
                        let (Ok(tag), Ok(i), Ok(name)) = (rd.u8(), rd.var_u64(), rd.bytes()) else {
                            continue;
                        };
                        let name = String::from_utf8_lossy(name).into_owned();
                        let list = if tag == 0 { &mut kinds } else { &mut frames };
                        let i = i as usize;
                        if list.len() <= i {
                            list.resize(i + 1, String::new());
                        }
                        list[i] = name;
                    }
                }
                p.kinds = kinds;
                p.frames = frames.iter().map(|f| frame_of(f)).collect();
                // Names changed: rows already read keep theirs (names only grow).
            }
        }
        let mut changed = false;
        for u in archive.iter().filter(|u| u.key.unit != ARCHIVE_NAMES) {
            let crc = u.parts.first().map_or(0, |p| p.crc);
            if p.units.get(&u.key.unit).is_some_and(|(c, _)| *c == crc) {
                continue;
            }
            let mut rows = Vec::new();
            for part in &u.parts {
                let Some(b) = get(part.page) else { continue };
                let Ok(v) = rtsim_parts::read_part(&b) else {
                    continue;
                };
                for row in v.rows.iter().filter(|r| r.kind == row_kind::ARCHIVE_ROW) {
                    if let Ok(raw) =
                        <&rtsim_archive::row::RawRow>::try_from(&row.body[..32.min(row.body.len())])
                    {
                        rows.push(live_row(raw, &p.kinds, &p.frames));
                    }
                }
            }
            p.units.insert(u.key.unit, (crc, rows));
            changed = true;
        }
        let present: BTreeSet<u64> = archive.iter().map(|u| u.key.unit).collect();
        p.units.retain(|k, _| present.contains(k));
        if changed {
            let all: Vec<&LiveRow> = p.units.values().flat_map(|(_, r)| r).collect();
            for row in all.iter().skip(p.count) {
                p.rows.push((p.next_seq, p.serial, (*row).clone()));
                p.next_seq += 1;
            }
            p.count = p.count.max(all.len());
        }
    }
    // What the save shown took is in its archive.
    p.rows.retain(|(_, serial, _)| *serial >= save_serial);
    p.rows.iter().map(|(seq, _, r)| (*seq, r.clone())).collect()
}

/// Whether a pending row answers `q` (its frame, area and kinds; the time is the caller's).
fn wants(q: &EventQuery, r: &LiveRow) -> bool {
    if !q.wants(r.kind()) {
        return false;
    }
    let (frame, bx) = match r {
        LiveRow::Event(e) => (&e.frame, [e.x, e.z, e.x, e.z]),
        LiveRow::Leg(l) => {
            let p = l.p.first().copied().unwrap_or_default();
            (
                &l.subject,
                [
                    p[0].min(p[2]),
                    p[1].min(p[3]),
                    p[0].max(p[2]),
                    p[1].max(p[3]),
                ],
            )
        }
    };
    let frame = match r {
        LiveRow::Event(_) => frame.clone(),
        LiveRow::Leg(_) => None,
    };
    if let (Some(want), Some(f)) = (&q.frame, &frame)
        && want != f
    {
        return false;
    }
    q.area
        .is_none_or(|b| bx[2] >= b[0] && bx[0] <= b[2] && bx[3] >= b[1] && bx[1] <= b[3])
}

impl Backend for LiveView {
    fn world(&self) -> WorldInfo {
        let save = self.save();
        let live = self.live();
        let mut w = save.world();
        let save_seq = save.saved().generation.save_seq;
        let now = now_ms();
        w.source = "live".into();
        w.stream = true;
        w.frames = live.frames.clone();
        let mine = format!("rtsim-view {}", env!("CARGO_PKG_VERSION"));
        match &live.header {
            Some(h) => {
                let age = (now - h.wall_ms as i64).max(0) as u64;
                let stale = age > STALE_AFTER.as_millis() as u64;
                w.time = TimeInfo {
                    tick: h.game_time,
                    ticks_per_day: TICKS_PER_DAY,
                    wall_ms: h.wall_ms as i64,
                    tps: if stale { 0.0 } else { h.tps },
                    running: !stale,
                };
                w.freshness = Freshness {
                    age_ms: age,
                    cycle_ms: (h.cycles > 0).then_some(h.cycle_ms),
                    note: if stale {
                        "the server has not committed for a while (stopped, paused or behind)"
                            .into()
                    } else if h.cycles == 0 {
                        "the server's first pass over the world is under way".into()
                    } else {
                        String::new()
                    },
                };
                let skew = h.build.rsplit(' ').next() != mine.rsplit(' ').next();
                w.build = format!("{mine}; the server's tap: {}", h.build);
                w.source_note = Some(format!(
                    "live from the server of {} (view store commit {}, day {:.2}); cards, \
                     evicted things and the archive from its save {save_seq}{}",
                    self.name,
                    h.commit,
                    h.game_time as f64 / TICKS_PER_DAY as f64,
                    if skew {
                        "; WARNING: the server and this viewer are different builds, so layers \
                         they do not share are not drawn"
                    } else {
                        ""
                    }
                ));
            }
            None => {
                w.build = mine;
                w.freshness.note = "waiting for the server's first view commit".into();
                w.source_note = Some(format!(
                    "live from the server of {}: no view store yet (the server reads the lease \
                     within 200 steps, about 10 s, of a page opening{}); showing its save \
                     {save_seq}",
                    self.name,
                    live.error
                        .as_ref()
                        .map(|e| format!(
                            "; {}",
                            e.replace(&self.dir.display().to_string(), &self.name)
                        ))
                        .unwrap_or_default()
                ));
            }
        }
        w.event_kinds.extend(live.pending_kinds.iter().cloned());
        w.has_events |= !live.pending.is_empty();
        w
    }

    fn layers(&self) -> Arc<Layers> {
        self.save().layers()
    }

    /// One time, now: the live marks for every `t`.
    fn scene(&self, frame: &str, _t: Option<Tick>) -> Option<Arc<Scene>> {
        self.live().scenes.get(frame).cloned()
    }

    fn card(&self, layer: &str, id: &str, t: Option<Tick>) -> Option<Card> {
        let live = self.live();
        let save = self.save();
        let key = (layer.to_owned(), canon(id));
        let now = live.fields.get(&key);
        let save_tick = save.saved().now();
        let day = |t: Tick| t as f64 / TICKS_PER_DAY as f64;
        let h = live.header.as_ref();
        match (save.card(layer, id, t), now) {
            (Some(mut c), now) => {
                if let Some(fs) = now {
                    c.sections.insert(
                        0,
                        Section {
                            title: "Now".into(),
                            fields: fs.clone(),
                        },
                    );
                }
                let mut note = format!("from the save at day {:.2}", day(save_tick));
                if let (Some(h), Some(_)) = (h, now) {
                    note.push_str(&format!("; \"Now\" at day {:.2}", day(h.game_time)));
                }
                c.note = Some(match c.note.take() {
                    Some(n) => format!("{n} ({note})"),
                    None => note,
                });
                Some(c)
            }
            (None, Some(fs)) => {
                let label = save
                    .layers()
                    .layers
                    .iter()
                    .find(|d| d.id == layer)
                    .map_or_else(|| layer.to_owned(), |d| d.label.clone());
                let mut c =
                    Card::new(layer, &key.1, format!("{label} {}", key.1)).section(Section {
                        title: "Now".into(),
                        fields: fs.clone(),
                    });
                c.as_of = h.map(|h| h.game_time);
                c.note = Some(
                    "new since the last save: the rest of its card comes with the next save".into(),
                );
                Some(c)
            }
            (None, None) => None,
        }
    }

    fn events(&self, q: &EventQuery) -> EventsAnswer {
        let mut a = self.save().events(q);
        let live = self.live();
        let (mut more_rows, mut more_legs) = (Vec::new(), Vec::new());
        for (_, r) in &live.pending {
            if r.t() < q.from || r.t() >= q.to || !wants(q, r) {
                continue;
            }
            match r {
                LiveRow::Event(e) => more_rows.push(e.clone()),
                LiveRow::Leg(l) => more_legs.push(l.clone()),
            }
        }
        if more_rows.is_empty() && more_legs.is_empty() {
            return a;
        }
        a.rows.extend(more_rows);
        a.legs.extend(more_legs);
        a.rows.sort_by(|x, y| y.t.cmp(&x.t));
        a.legs.sort_by(|x, y| y.t0.cmp(&x.t0));
        if a.rows.len() > q.limit || a.legs.len() > q.limit {
            a.truncated = true;
            a.rows.truncate(q.limit);
            a.legs.truncate(q.limit);
        }
        a
    }

    fn histogram(&self, q: &EventQuery) -> Histogram {
        let live = self.live();
        let now = live
            .header
            .as_ref()
            .map_or_else(|| self.save().saved().now(), |h| h.game_time);
        let mut q2 = q.clone();
        if q2.to == Tick::MAX {
            q2.to = now + 1;
        }
        let mut h = self.save().histogram(&q2);
        if h.from == Tick::MIN {
            h.from = live.pending.first().map_or(now, |(_, r)| r.t()).min(now);
        }
        if h.to <= h.from {
            h.to = h.from + 1;
        }
        let span = (h.to - h.from) as f64;
        let n = h.buckets.max(1);
        for (_, r) in &live.pending {
            let t = r.t();
            if t < h.from || t >= h.to || !wants(q, r) {
                continue;
            }
            let i = (((t - h.from) as f64 / span) * n as f64) as usize;
            let v = h
                .series
                .entry(r.kind().to_owned())
                .or_insert_with(|| vec![0; n]);
            if let Some(b) = v.get_mut(i.min(n - 1)) {
                *b += 1;
            }
        }
        h
    }

    fn events_since(&self, cursor: &mut u64) -> Vec<mic_viewer::api::EventRow> {
        let live = self.live();
        let mut out = Vec::new();
        for (seq, r) in &live.pending {
            if *seq < *cursor {
                continue;
            }
            *cursor = seq + 1;
            if let LiveRow::Event(e) = r {
                out.push(e.clone());
            }
        }
        out
    }

    fn tiles(&self, source: &str) -> Option<&Tiles> {
        self.save().tiles(source).is_some().then_some(&self.tiles)
    }

    fn changes(&self) -> Option<&Changes> {
        Some(&self.changes)
    }

    fn watchers(&self, open: usize) {
        self.open.store(open, Ordering::Release);
        if let Some(l) = &self.lease {
            l.watchers(open);
        }
    }

    fn extra(&self, req: &Request) -> Option<Reply> {
        match req.path.as_str() {
            "/api/status" => {
                let mut status = self.save().status();
                status["source"] = json!("live");
                status["live"] = self.live_status();
                Some(Response::json_of(&status).into())
            }
            _ => None,
        }
    }
}

fn finite(req: &Request, key: &str) -> Option<f64> {
    req.num::<f64>(key).filter(|v| v.is_finite())
}

/// The routes, as mic-viewer's [`App`] answers them, with each `/api/marks` request's view passed
/// to [`LiveView::look`] first (the lease's focus).
pub struct LiveApp {
    app: App<LiveView>,
}

impl LiveApp {
    pub fn new(live: Arc<LiveView>) -> LiveApp {
        LiveApp {
            app: App::new(live),
        }
    }

    pub fn live(&self) -> &Arc<LiveView> {
        &self.app.backend
    }
}

impl Handler for LiveApp {
    fn handle(&self, req: &Request) -> Reply {
        if req.path == "/api/marks"
            && req.method == "GET"
            && let (Some(frame), Some(x0), Some(z0), Some(x1), Some(z1), Some(bpp)) = (
                req.param("frame"),
                finite(req, "x0"),
                finite(req, "z0"),
                finite(req, "x1"),
                finite(req, "z1"),
                finite(req, "bpp"),
            )
        {
            let b = [x0.min(x1), z0.min(z1), x0.max(x1), z0.max(z1)];
            self.app.backend.look(&frame, b, bpp);
        }
        self.app.handle(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rtsim-view-live-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn focus_regions_are_the_views_nearest_its_centre_first() {
        // A view of 3 by 3 regions centred on region (1, 1).
        let r = regions_in(0, [0.0, 0.0, 1535.0, 1535.0], 64);
        assert_eq!(r.len(), 9);
        assert_eq!(
            r[0],
            RegionKey {
                frame: 0,
                rx: 1,
                rz: 1
            }
        );
        // The four sides before the corners.
        let side = |k: &RegionKey| (k.rx - 1).abs() + (k.rz - 1).abs() == 1;
        assert!(r[1..5].iter().all(side));
        assert!(!r[5..].iter().any(side));
        // A huge view: the cap, around its centre.
        let r = regions_in(3, [-1e6, -1e6, 1e6, 1e6], 64);
        assert_eq!(r.len(), 64);
        assert!(
            r.iter()
                .all(|k| k.frame == 3 && k.rx.abs() <= 5 && k.rz.abs() <= 5)
        );
        assert!(regions_in(0, [f64::NAN, 0.0, 1.0, 1.0], 64).is_empty());
    }

    #[test]
    fn the_lease_is_written_while_a_stream_is_open_and_removed_on_stop() {
        let d = tmp("lease");
        let l = LeaseWriter::start(&d).unwrap();
        std::thread::sleep(LEASE_TICK * 2);
        assert!(
            !d.join(vstore::LEASE_FILE).exists(),
            "nothing before a stream"
        );
        l.watchers(1);
        let wait = |f: &dyn Fn() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !f() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&|| Lease::read(&d).is_ok());
        assert_eq!(Lease::read(&d).unwrap().pid, std::process::id());
        // A focus change is written within a second or so.
        let r = RegionKey {
            frame: 0,
            rx: 2,
            rz: -1,
        };
        l.focus(vec![r]);
        wait(&|| Lease::read(&d).is_ok_and(|x| x.focus == vec![r]));
        assert_eq!(l.writes(), 2);
        // Streams closed: no more writes (the engine ends the lease 30 s after the last).
        l.watchers(0);
        l.focus(Vec::new());
        std::thread::sleep(FOCUS_EVERY + LEASE_TICK * 2);
        assert_eq!(l.writes(), 2);
        // Another process's lease is left alone; this process's is removed on stop.
        l.stop();
        assert!(!d.join(vstore::LEASE_FILE).exists());
        let l = LeaseWriter::start(&d).unwrap();
        l.watchers(1);
        wait(&|| Lease::read(&d).is_ok());
        Lease {
            pid: std::process::id() + 1,
            focus: Vec::new(),
        }
        .write(&d)
        .unwrap();
        drop(l);
        assert_eq!(Lease::read(&d).unwrap().pid, std::process::id() + 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
