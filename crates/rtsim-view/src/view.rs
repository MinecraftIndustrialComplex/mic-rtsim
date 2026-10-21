// SPDX-License-Identifier: AGPL-3.0-or-later
//! The viewer backend over one save (VIEWER.md §4, §8): mic-viewer's [`Backend`] answered from a
//! [`Saved`]. Every scene is drawn once, when the view opens; cards are built on demand and kept in
//! a cache bounded by bytes; the base map renders into an in-memory tile cache. Nothing is written
//! anywhere.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use mic_viewer::api::{
    Card, Frame, Freshness, Layers, MARK_BUDGET, ProjectionDesc, TICKS_PER_DAY, Tick, TimeInfo,
    WorldInfo,
};
use mic_viewer::api::{EventsAnswer, Histogram};
use mic_viewer::http::{Reply, Request, Response};
use mic_viewer::scene::Scene;
use mic_viewer::tiles::{TileCache, Tiles};
use mic_viewer::viewer::{Backend, EventQuery};
use serde_json::json;

use crate::events::Events;
use crate::habitat::HabitatTiles;
use crate::layers::{self, Ctx};
use crate::load::{Limits, Saved, Status, load_world};
use crate::surveyor::SurveyorTiles;

/// Heap bytes live now, from the host's allocator (rtsim-cli's counting allocator).
pub type HeapProbe = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Blocks of margin around a frame's content.
const PAD: f64 = 64.0;

/// One save, served.
pub struct View {
    /// Paged rows cache through `RefCell`s, so the save is read by one thread at a time.
    saved: Mutex<Saved>,
    now: Tick,
    limits: Limits,
    world: WorldInfo,
    /// The manifest's modification time (when the save was written), ms since the epoch.
    saved_ms: i64,
    layers: Arc<Layers>,
    scenes: BTreeMap<String, Arc<Scene>>,
    counts: BTreeMap<String, usize>,
    reasons: BTreeMap<String, String>,
    tiles: Option<Tiles>,
    tile_layers: BTreeSet<String>,
    /// Surveyor's terrain, over HABITAT where it has both (VIEWER.md §9).
    surveyor: Option<Tiles>,
    surveyor_layers: BTreeSet<String>,
    surveyor_parses: Option<Arc<SurveyorTiles>>,
    /// The one worker (VWD-15): a card's build and a tile's render hold it.
    worker: Arc<Mutex<()>>,
    cards: Mutex<CardCache>,
    heap: Option<HeapProbe>,
    /// The save's event archive as of its generation (VIEWER.md §10.6), if it has one.
    events: Option<Events>,
    /// Why the archive did not open, when it is there but unreadable.
    events_error: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn mtime_ms(p: &Path) -> Option<i64> {
    let t = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

/// A frame's id: its dimension name from the journal, else `dim<id>`.
pub fn frame_id(saved: &Saved, frame: u32) -> String {
    saved
        .names
        .get(crate::keys::DICT_DIMENSION, frame)
        .map_or_else(|| format!("dim{frame}"), str::to_owned)
}

/// The engine's dimension number in an archive frame name (`dim3`).
fn archive_frame(name: &str) -> Option<u32> {
    name.strip_prefix("dim")?.parse().ok()
}

/// "minecraft:the_nether" as "The Nether".
fn frame_label(id: &str, frame: u32) -> String {
    if id.starts_with("dim") && id[3..].parse::<u32>().is_ok() {
        return format!("Dimension {frame}");
    }
    let tail = id.rsplit(':').next().unwrap_or(id);
    tail.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn slug(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

impl View {
    /// Opens the save in `dir` (a world folder or its `rtsim/`) and draws it.
    pub fn open(dir: &Path, limits: Limits) -> Result<View, String> {
        Ok(View::new(load_world(dir, limits)?))
    }

    pub fn new(saved: Saved) -> View {
        let specs = layers::specs();
        let drawn = layers::build(&saved, &specs);
        let worker = Arc::new(Mutex::new(()));
        let mut frames: BTreeSet<u32> = drawn.frames.keys().copied().collect();
        // The archive names frames as the engine numbers them (`dim<n>`); a frame with events but
        // no marks still gets a page frame, so its timeline can be read.
        let (events, events_error) =
            match Events::open(&saved.world_dir, saved.generation.obs_seq, |name| {
                archive_frame(name).map_or_else(|| name.to_owned(), |n| frame_id(&saved, n))
            }) {
                Ok(e) => (e, None),
                Err(e) => (None, Some(e.to_string())),
            };
        if let Some(e) = &events {
            frames.extend(e.frame_numbers().filter_map(archive_frame));
        }
        let mut slugs: BTreeMap<u16, String> = BTreeMap::new();
        let habitat = saved.world_model.clone().map(|w| {
            let probe = HabitatTiles::new(w.clone(), &BTreeMap::new(), Arc::clone(&worker));
            for f in probe.frames() {
                slugs.insert(f, slug(&frame_id(&saved, u32::from(f))));
            }
            HabitatTiles::new(w, &slugs, Arc::clone(&worker))
        });
        if let Some(h) = &habitat {
            frames.extend(h.frames().map(u32::from));
        }
        if frames.is_empty() {
            frames.insert(0);
        }
        let named: Vec<(String, String)> = frames
            .iter()
            .map(|&f| {
                let id = frame_id(&saved, f);
                let s = slug(&id);
                (id, s)
            })
            .collect();
        let surveyor = SurveyorTiles::open(
            &saved.world_dir,
            &named,
            usize::try_from(saved.limits.surveyor_cache).unwrap_or(usize::MAX),
            Arc::clone(&worker),
        )
        .map(Arc::new);
        let t: Tick = saved.now();
        let mut writers = drawn.frames;
        let mut scenes = BTreeMap::new();
        let mut infos = Vec::new();
        for &f in &frames {
            let id = frame_id(&saved, f);
            let mut w = writers.remove(&f).unwrap_or_default();
            for s in &specs {
                match drawn.reasons.get(&s.desc.id) {
                    Some(why) => w.empty(&s.desc.id, why.clone()),
                    None => w.layer(&s.desc.id),
                }
            }
            scenes.insert(id.clone(), Arc::new(w.finish(t, 0.0)));
            let mut bounds = drawn.bounds.get(&f).copied();
            let mut tiles = Vec::new();
            if let Some(h) = &habitat
                && let Ok(f16) = u16::try_from(f)
                && let Some(d) = h.desc(&slugs, f16)
            {
                bounds = Some(union(bounds, d.bounds));
                tiles.push(d);
            }
            // Over HABITAT: Surveyor's unrecorded columns are transparent.
            if let Some(d) = surveyor.as_ref().and_then(|s| s.desc(&slug(&id))) {
                bounds = Some(union(bounds, d.bounds));
                tiles.push(d);
            }
            let b = bounds.unwrap_or([-512.0, -512.0, 512.0, 512.0]);
            infos.push(Frame {
                label: frame_label(&id, f),
                id,
                projection: ProjectionDesc::flat(),
                bounds: [b[0] - PAD, b[1] - PAD, b[2] + PAD, b[3] + PAD],
                tiles,
            });
        }
        let manifest = saved.pages_dir.join("manifest.rtm");
        let saved_ms = mtime_ms(&manifest).unwrap_or_else(now_ms);
        let name = saved
            .world_dir
            .file_name()
            .map_or_else(|| "world".into(), |n| n.to_string_lossy().into_owned());
        let g = saved.generation;
        let mut note = format!(
            "save {} (observations to {}), {} pages",
            g.save_seq, g.obs_seq, saved.pages
        );
        if saved.unconfirmed_hidden {
            note.push_str("; a save the world never confirmed is not shown");
        }
        let world = WorldInfo {
            name,
            source: "save".into(),
            source_note: Some(note),
            build: format!("rtsim-view {}", env!("CARGO_PKG_VERSION")),
            frames: infos,
            time: TimeInfo {
                tick: t,
                ticks_per_day: TICKS_PER_DAY,
                wall_ms: saved_ms,
                tps: 0.0,
                running: false,
            },
            freshness: Freshness {
                age_ms: 0,
                cycle_ms: None,
                note: "a save: nothing moves".into(),
            },
            budget: MARK_BUDGET,
            event_kinds: events
                .as_ref()
                .map(|e| e.kinds().to_vec())
                .unwrap_or_default(),
            has_events: events.as_ref().is_some_and(Events::any),
            ..WorldInfo::default()
        };
        let layers = Arc::new(layers::layers(&specs));
        // The tile cache's bytes are shared when both base maps are there.
        let tile_cache = match (&habitat, &surveyor) {
            (Some(_), Some(_)) => saved.limits.tile_cache / 2,
            _ => saved.limits.tile_cache,
        };
        let (tiles, tile_layers) = match habitat {
            Some(h) => {
                let names = h.layer_names().map(str::to_owned).collect();
                let cache = TileCache::in_memory(tile_cache);
                (Some(Tiles::new(Arc::new(h), cache, false)), names)
            }
            None => (None, BTreeSet::new()),
        };
        let (surveyor_tiles, surveyor_layers) = match &surveyor {
            Some(s) => {
                let names = s.layer_names().map(str::to_owned).collect();
                let cache = TileCache::in_memory(tile_cache);
                let source: Arc<SurveyorTiles> = Arc::clone(s);
                (Some(Tiles::new(source, cache, false)), names)
            }
            None => (None, BTreeSet::new()),
        };
        let cap = saved.limits.card_cache;
        View {
            now: t,
            limits: saved.limits,
            saved: Mutex::new(saved),
            world,
            saved_ms,
            layers,
            scenes,
            counts: drawn.counts,
            reasons: drawn.reasons,
            tiles,
            tile_layers,
            surveyor: surveyor_tiles,
            surveyor_layers,
            surveyor_parses: surveyor,
            worker,
            cards: Mutex::new(CardCache::new(cap)),
            heap: None,
            events,
            events_error,
        }
    }

    /// Reads the heap through `probe`; the cap is [`Limits::heap_cap`].
    pub fn with_heap(mut self, probe: HeapProbe) -> View {
        self.heap = Some(probe);
        self
    }

    /// The save (held: cards wait while it is).
    pub fn saved(&self) -> MutexGuard<'_, Saved> {
        lock(&self.saved)
    }

    /// Marks drawn per layer.
    pub fn counts(&self) -> &BTreeMap<String, usize> {
        &self.counts
    }

    /// Why each empty layer is empty.
    pub fn reasons(&self) -> &BTreeMap<String, String> {
        &self.reasons
    }

    /// The frame ids, in order.
    pub fn frames(&self) -> impl Iterator<Item = &str> + '_ {
        self.scenes.keys().map(String::as_str)
    }

    /// The HABITAT base map layer names.
    pub fn tile_layers(&self) -> &BTreeSet<String> {
        &self.tile_layers
    }

    /// The Surveyor base map layer names (none when the world has no Surveyor summaries).
    pub fn surveyor_layers(&self) -> &BTreeSet<String> {
        &self.surveyor_layers
    }

    fn over_cap(&self) -> Option<(u64, u64)> {
        let cap = self.limits.heap_cap?;
        let live = (self.heap.as_ref()?)();
        (live >= cap).then_some((live, cap))
    }

    /// `/api/status`: what loaded, what each layer drew, the caches. No filesystem path beyond the
    /// world's name (VIEWER.md §13): the world folder is said as its name.
    pub fn status(&self) -> serde_json::Value {
        let s = lock(&self.saved);
        let root = s.world_dir.display().to_string();
        let scrub = |text: &str| text.replace(&root, &self.world.name);
        let modules: Vec<_> = s
            .modules
            .iter()
            .map(|m| {
                let (status, error) = match &m.status {
                    Status::Absent => ("absent", None),
                    Status::Loaded => ("loaded", None),
                    Status::Unreadable(e) => ("unreadable", Some(scrub(e))),
                };
                json!({
                    "name": m.name,
                    "status": status,
                    "error": error,
                    "detail": scrub(&m.detail),
                    "hash": m.hash.map(|(now, _)| format!("{now:016x}")),
                    "saved_hash": m.hash.and_then(|(_, h)| h).map(|h| format!("{h:016x}")),
                })
            })
            .collect();
        let layers: BTreeMap<_, _> = self
            .counts
            .iter()
            .map(|(id, n)| {
                (
                    id.clone(),
                    json!({ "marks": n, "empty": self.reasons.get(id) }),
                )
            })
            .collect();
        let cards = lock(&self.cards).stats();
        let hashes: BTreeMap<_, _> = s
            .state_hashes()
            .into_iter()
            .map(|(m, h)| (m, format!("{h:016x}")))
            .collect();
        let tiles = self.tiles.as_ref().map(|t| t.cache.stats());
        let surveyor = self.surveyor.as_ref().map(|t| t.cache.stats());
        json!({
            "world": self.world.name,
            "pages": s
                .pages_dir
                .strip_prefix(&s.world_dir)
                .map_or_else(|_| "rtsim".into(), |p| p.display().to_string()),
            "save_seq": s.generation.save_seq,
            "obs_seq": s.generation.obs_seq,
            "page_count": s.pages,
            "unconfirmed_hidden": s.unconfirmed_hidden,
            "tick": s.now(),
            "modules": modules,
            "state_hashes": hashes,
            "layers": layers,
            "frames": self.scenes.keys().collect::<Vec<_>>(),
            "limits": {
                "row_budget": s.limits.row_budget,
                "card_cache": s.limits.card_cache,
                "tile_cache": s.limits.tile_cache,
                "surveyor_cache": s.limits.surveyor_cache,
                "heap_cap": s.limits.heap_cap,
            },
            "caches": {
                "cards": { "bytes": cards.0, "entries": cards.1, "cap": cards.2 },
                "tiles": tiles.map(|(b, n, c)| json!({ "bytes": b, "entries": n, "cap": c })),
                "surveyor_tiles": surveyor.map(|(b, n, c)| json!({ "bytes": b, "entries": n, "cap": c })),
            },
            "surveyor": self.surveyor_parses.as_ref().map(|s| json!({
                "layers": self.surveyor_layers,
                "regions_parsed": s.parses(),
                "picture_bytes": s.cached_bytes(),
            })),
            "heap": self.heap.as_ref().map(|h| h()),
            "events": match &self.events {
                Some(e) => {
                    let (dropped, damaged) = e.losses();
                    json!({
                        "kinds": e.kinds().len(),
                        "chunks": e.chunks(),
                        "span": e.span(),
                        "dropped_rows": dropped,
                        "damaged_chunks": damaged,
                    })
                }
                None => json!({ "error": self.events_error.as_deref().map(|t| t.replace(&root, &self.world.name)) }),
            },
        })
    }

    /// A thing's card, and whether it may be cached (a card refused at the cap may not).
    fn build_card(&self, layer: &str, id: &str) -> Option<(Card, bool)> {
        let _one = lock(&self.worker);
        let saved = lock(&self.saved);
        if layer == "industry.districts" {
            let id: u64 = id.parse().ok()?;
            saved.industry.as_ref()?.district(id)?;
            if self.over_cap().is_some() {
                lock(&self.cards).clear();
            }
            if let Some((live, cap)) = self.over_cap() {
                let c = Card::new(layer, id, format!("District {id}")).note(format!(
                    "the viewer is at its memory cap ({} of {} MiB), so it did not decode this \
                     district's pages",
                    live >> 20,
                    cap >> 20
                ));
                return Some((c, false));
            }
            return crate::cards::district(&saved, id).map(|c| (c, true));
        }
        crate::cards::card(&Ctx::new(&saved), layer, id).map(|c| (c, true))
    }
}

impl Backend for View {
    fn world(&self) -> WorldInfo {
        let mut w = self.world.clone();
        w.freshness.age_ms = (now_ms() - self.saved_ms).max(0) as u64;
        w
    }

    fn layers(&self) -> Arc<Layers> {
        Arc::clone(&self.layers)
    }

    /// A save has one time: every `t` gets the save's scene (legs still place movers at `t`).
    fn scene(&self, frame: &str, _t: Option<Tick>) -> Option<Arc<Scene>> {
        self.scenes.get(frame).cloned()
    }

    fn card(&self, layer: &str, id: &str, _t: Option<Tick>) -> Option<Card> {
        let key = (layer.to_owned(), id.to_owned());
        if let Some(c) = lock(&self.cards).get(&key) {
            return Some(c);
        }
        let (mut c, keep) = self.build_card(layer, id)?;
        c.as_of = Some(self.now);
        if keep {
            lock(&self.cards).put(key, c.clone());
        }
        Some(c)
    }

    fn events(&self, q: &EventQuery) -> EventsAnswer {
        self.events
            .as_ref()
            .map(|e| e.events(q))
            .unwrap_or_default()
    }

    fn histogram(&self, q: &EventQuery) -> Histogram {
        match &self.events {
            Some(e) => e.histogram(q, self.now),
            None => Histogram {
                from: q.from,
                to: q.to,
                buckets: q.limit,
                ..Histogram::default()
            },
        }
    }

    fn tiles(&self, source: &str) -> Option<&Tiles> {
        if self.surveyor_layers.contains(source) {
            return self.surveyor.as_ref();
        }
        if self.tile_layers.contains(source) {
            self.tiles.as_ref()
        } else {
            None
        }
    }

    fn extra(&self, req: &Request) -> Option<Reply> {
        match req.path.as_str() {
            "/api/status" => Some(Response::json_of(&self.status()).into()),
            _ => None,
        }
    }
}

/// Built cards, least recently used out first, bounded by their JSON size.
struct CardCache {
    cap: u64,
    bytes: u64,
    seq: u64,
    map: HashMap<(String, String), (u64, u64, Card)>,
    order: BTreeMap<u64, (String, String)>,
}

impl CardCache {
    fn new(cap: u64) -> CardCache {
        CardCache {
            cap,
            bytes: 0,
            seq: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn get(&mut self, key: &(String, String)) -> Option<Card> {
        self.seq += 1;
        let seq = self.seq;
        let (s, _, c) = self.map.get_mut(key)?;
        self.order.remove(s);
        *s = seq;
        self.order.insert(seq, key.clone());
        Some(c.clone())
    }

    fn put(&mut self, key: (String, String), c: Card) {
        let size = serde_json::to_vec(&c).map_or(0, |v| v.len() as u64)
            + (key.0.len() + key.1.len()) as u64
            + 64;
        if size > self.cap {
            return;
        }
        if let Some((s, b, _)) = self.map.remove(&key) {
            self.order.remove(&s);
            self.bytes -= b;
        }
        while self.bytes + size > self.cap {
            let Some((_, k)) = self.order.pop_first() else {
                break;
            };
            if let Some((_, b, _)) = self.map.remove(&k) {
                self.bytes -= b;
            }
        }
        self.seq += 1;
        self.order.insert(self.seq, key.clone());
        self.map.insert(key, (self.seq, size, c));
        self.bytes += size;
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.bytes = 0;
    }

    /// (bytes, entries, cap).
    fn stats(&self) -> (u64, usize, u64) {
        (self.bytes, self.map.len(), self.cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_slugs() {
        assert_eq!(frame_label("minecraft:the_nether", 1), "The Nether");
        assert_eq!(frame_label("dim7", 7), "Dimension 7");
        assert_eq!(slug("minecraft:overworld"), "minecraft-overworld");
    }

    #[test]
    fn card_cache_keeps_under_its_cap() {
        let mut c = CardCache::new(2_000);
        for i in 0..100 {
            c.put(
                ("l".into(), i.to_string()),
                Card::new("l", i, "x".repeat(100)),
            );
            assert!(c.bytes <= 2_000);
        }
        assert!(c.get(&("l".into(), "99".into())).is_some());
        assert!(c.get(&("l".into(), "0".into())).is_none());
        let (bytes, n, _) = c.stats();
        assert!(n > 1 && bytes <= 2_000);
    }
}
