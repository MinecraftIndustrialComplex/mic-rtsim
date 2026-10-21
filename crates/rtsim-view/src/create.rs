// SPDX-License-Identifier: AGPL-3.0-or-later
//! Create's rail and factories beside the rtsim layers (docs/viewer/industry-map.md).
//!
//! The world's `data/create_tracks.dat` is read once, when the view opens, and drawn as
//! mic-viewer-create's rail layers. Block entities (machines, belts, chains, package ports, vaults)
//! are read on demand by a [`Survey`]: every district's chunks and the chunks round every station
//! at open, the chunks a close view shows when the page asks for marks. Both are composed into the
//! frame's scene next to the layers the parts drew. Every file is read whole into memory and
//! closed; nothing is written.
//!
//! The rtsim train lines (`industry.lines`) are redrawn along the track when both of their stations
//! are Create stations of one network, so a line runs where its trains run.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use mic_viewer::api::{Card, Found, LayerDesc, Mark};
use mic_viewer::scene::{LayerRows, Scene, SceneWriter};
use mic_viewer_create::blocks::{Survey, Thing};
use mic_viewer_create::draw;
use mic_viewer_create::rail::Rail;

/// The rtsim layer redrawn along the track.
pub const LINES: &str = "industry.lines";
/// Chunks one look asks for at most, nearest the view's centre first.
pub const LOOK_CHUNKS: usize = 1024;
/// Looks at this zoom or closer (blocks a CSS pixel) read the block entities they show.
pub const LOOK_BPP: f64 = 2.5;
/// Chunks round a station read at open: this many each way.
const STATION_REACH: i32 = 1;
/// The survey tells the view about new chunks at most this often.
const LANDED_EVERY: Duration = Duration::from_millis(750);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A train line of the rtsim engine, for routing: its mark id (`a-b`), the frame and the two
/// stations' block positions.
pub struct LineAsk {
    pub id: String,
    pub frame: String,
    pub a: [i32; 3],
    pub b: [i32; 3],
}

/// Create's layers for one opened world.
/// A frame's layers by id.
type Rows = HashMap<String, LayerRows>;

pub struct CreateLayers {
    rail: Option<Rail>,
    /// Why there is no rail, when `create_tracks.dat` is there but did not read.
    rail_error: Option<String>,
    survey: Survey,
    /// The rail layers per frame id, drawn once.
    rail_rows: HashMap<String, HashMap<String, LayerRows>>,
    /// The block layers per frame id, as of a survey version.
    block_rows: Mutex<HashMap<String, (u64, Arc<Rows>)>>,
    /// Routed train lines by mark id.
    routes: HashMap<String, Vec<[f64; 2]>>,
    /// Whether the world has region files (see [`CreateLayers::can_land`]).
    can_land: bool,
    /// The line layer composed per frame, keyed by the base layer's version.
    lines: Mutex<HashMap<String, (u64, LayerRows)>>,
}

impl CreateLayers {
    /// Reads `world`'s rail and starts its survey; `landed` is called (on the survey's thread)
    /// when block entities land. `frames` are the view's frame ids: the rail's dimensions are
    /// drawn under the frame of the same id.
    pub fn open(world: &Path, landed: Box<dyn Fn() + Send + Sync>) -> CreateLayers {
        let (rail, rail_error) = match Rail::open(world) {
            Ok(r) => (r, None),
            Err(e) => (None, Some(format!("create_tracks.dat did not read: {e}"))),
        };
        let mut rail_rows = HashMap::new();
        if let Some(r) = &rail {
            for (i, dim) in r.dims.iter().enumerate() {
                let mut w = SceneWriter::new();
                draw::rail_marks(r, i as i32, &mut w);
                let scene = w.finish(0, 0.0);
                if scene.layers.values().any(|l| !l.is_empty()) {
                    rail_rows.insert(dim.clone(), scene.layers);
                }
            }
        }
        let can_land = ["region", "DIM-1/region", "DIM1/region", "dimensions"]
            .iter()
            .any(|d| world.join(d).is_dir());
        CreateLayers {
            rail,
            rail_error,
            can_land,
            survey: Survey::new(world, LANDED_EVERY, landed),
            rail_rows,
            block_rows: Mutex::new(HashMap::new()),
            routes: HashMap::new(),
            lines: Mutex::new(HashMap::new()),
        }
    }

    pub fn rail(&self) -> Option<&Rail> {
        self.rail.as_ref()
    }

    pub fn survey(&self) -> &Survey {
        &self.survey
    }

    /// Whether block entities can land after the page opens: the world has region files to read.
    /// A save without them never publishes, so its page opens no stream.
    pub fn can_land(&self) -> bool {
        self.can_land
    }

    /// Whether the world has Create track.
    pub fn has_rail(&self) -> bool {
        self.rail.as_ref().is_some_and(|r| !r.graphs.is_empty())
    }

    /// The frame ids the rail is drawn in, and each one's bounds (x0, z0, x1, z1).
    pub fn rail_frames(&self) -> Vec<(String, [f64; 4])> {
        let mut out = Vec::new();
        for (dim, rows) in &self.rail_rows {
            let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
            for m in rows.values().flat_map(|l| l.marks()) {
                let mb = m.bbox();
                b = [
                    b[0].min(mb[0]),
                    b[1].min(mb[1]),
                    b[2].max(mb[2]),
                    b[3].max(mb[3]),
                ];
            }
            if b[0] <= b[2] {
                out.push((dim.clone(), b));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The layers this adds, after the rtsim ones (rail layers only when there is rail).
    pub fn layer_descs(&self) -> Vec<LayerDesc> {
        let rail = self.has_rail();
        draw::layer_descs()
            .into_iter()
            .filter(|d| rail || !draw::RAIL_LAYERS.contains(&d.id.as_str()))
            .collect()
    }

    /// Queues the chunks of `dim` the view will want first: districts' (given) and those round
    /// every station.
    pub fn want_at_open(&self, districts: &[(String, Vec<(i32, i32)>)]) {
        if let Some(r) = &self.rail {
            let mut by_dim: BTreeMap<&str, Vec<(i32, i32)>> = BTreeMap::new();
            for s in &r.stations {
                let dim = r
                    .dims
                    .get(r.graphs[s.place.graph].dim as usize)
                    .map_or("", String::as_str);
                let (cx, cz) = (
                    (s.place.at[0].floor() as i32) >> 4,
                    (s.place.at[2].floor() as i32) >> 4,
                );
                let v = by_dim.entry(dim).or_default();
                for dx in -STATION_REACH..=STATION_REACH {
                    for dz in -STATION_REACH..=STATION_REACH {
                        v.push((cx + dx, cz + dz));
                    }
                }
            }
            for (dim, chunks) in by_dim {
                self.survey.want(dim, chunks, false);
            }
        }
        for (dim, chunks) in districts {
            self.survey.want(dim, chunks.iter().copied(), false);
        }
    }

    /// A page looks at `area` of `frame` at `bpp`: a close look reads its chunks, centre first.
    pub fn look(&self, frame: &str, area: [f64; 4], bpp: f64) {
        if !(bpp > 0.0 && bpp <= LOOK_BPP) {
            return;
        }
        let c =
            |v: f64| (v.floor() as i64 >> 4).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let (x0, z0, x1, z1) = (c(area[0]), c(area[1]), c(area[2]), c(area[3]));
        let (mx, mz) = ((x0 + x1) / 2, (z0 + z1) / 2);
        // Rings round the centre until the cap or the box is covered.
        let mut chunks = Vec::new();
        let reach = (x1 - x0).max(z1 - z0) / 2 + 1;
        for r in 0..=reach {
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let (x, z) = (mx + dx, mz + dz);
                    if (x0..=x1).contains(&x) && (z0..=z1).contains(&z) {
                        chunks.push((x, z));
                    }
                }
            }
            if chunks.len() >= LOOK_CHUNKS {
                break;
            }
        }
        chunks.truncate(LOOK_CHUNKS);
        self.survey.want(frame, chunks, true);
    }

    /// Redraws the rtsim train lines along the track where it can: each line whose two stations
    /// are Create stations of one network.
    pub fn route_lines(&mut self, asks: &[LineAsk]) {
        let Some(r) = &self.rail else { return };
        let mut at: HashMap<(i32, [i32; 3]), usize> = HashMap::new();
        for (i, s) in r.stations.iter().enumerate() {
            if let Some(b) = s.block {
                at.insert((r.graphs[s.place.graph].dim, b), i);
            }
        }
        for a in asks {
            let Some(d) = r.dim_index(&a.frame) else {
                continue;
            };
            let (Some(&sa), Some(&sb)) = (at.get(&(d, a.a)), at.get(&(d, a.b))) else {
                continue;
            };
            if let Some(line) = r.route(&r.stations[sa].place, &r.stations[sb].place)
                && line.len() >= 2
            {
                self.routes.insert(a.id.clone(), line);
            }
        }
    }

    /// How many train lines run along the track.
    pub fn routed(&self) -> usize {
        self.routes.len()
    }

    fn block_layers(&self, frame: &str) -> Arc<HashMap<String, LayerRows>> {
        let v = self.survey.version();
        let mut m = lock(&self.block_rows);
        if let Some((at, rows)) = m.get(frame)
            && *at == v
        {
            return Arc::clone(rows);
        }
        let things: Vec<Thing> = self
            .survey
            .all(frame)
            .iter()
            .flat_map(|c| c.iter().cloned())
            .collect();
        let mut w = SceneWriter::new();
        draw::block_marks(&things, frame, &mut w);
        let rows = Arc::new(w.finish(0, 0.0).layers);
        m.insert(frame.to_owned(), (v, Arc::clone(&rows)));
        rows
    }

    /// The line layer with routed lines in place of straight ones.
    fn lines_layer(&self, frame: &str, base: &LayerRows) -> LayerRows {
        let mut m = lock(&self.lines);
        if let Some((v, rows)) = m.get(frame)
            && *v == base.version
        {
            return rows.clone();
        }
        let marks: Vec<Mark> = base
            .marks()
            .iter()
            .map(|mk| match mk {
                Mark::Edge {
                    id: Some(id),
                    w,
                    ci,
                    c,
                    ..
                } => match self.routes.get(id) {
                    Some(p) => Mark::Path {
                        id: Some(id.clone()),
                        p: p.clone(),
                        ci: *ci,
                        c: c.clone(),
                        w: *w,
                        st: None,
                        l: None,
                        arrow: false,
                        n: None,
                    },
                    None => mk.clone(),
                },
                _ => mk.clone(),
            })
            .collect();
        let rows = LayerRows::from_marks(marks);
        m.insert(frame.to_owned(), (base.version, rows.clone()));
        rows
    }

    /// `base` (a frame's rtsim scene) with Create's layers added.
    pub fn compose(&self, frame: &str, base: &Scene) -> Scene {
        let mut layers = base.layers.clone();
        if let Some(l) = base.layers.get(LINES)
            && !l.is_empty()
            && !self.routes.is_empty()
        {
            layers.insert(LINES.to_owned(), self.lines_layer(frame, l));
        }
        if self.rail.is_some() {
            match self.rail_rows.get(frame) {
                Some(rows) => layers.extend(rows.iter().map(|(k, v)| (k.clone(), v.clone()))),
                None => {
                    for l in draw::RAIL_LAYERS {
                        layers.insert(
                            l.to_owned(),
                            LayerRows::empty_because("no Create track in this dimension"),
                        );
                    }
                }
            }
        }
        let blocks = self.block_layers(frame);
        for l in draw::BLOCK_LAYERS {
            let rows = blocks.get(l).cloned().filter(|r| !r.is_empty()).unwrap_or_else(|| {
                LayerRows::empty_because(if self.survey.stats().0 == 0 {
                    "nothing read here yet: zoom in closer than 2 blocks a pixel to read the chunks shown"
                } else {
                    "none in the chunks read so far: zoom in closer than 2 blocks a pixel to read more"
                })
            });
            layers.insert(l.to_owned(), rows);
        }
        Scene {
            t: base.t,
            period_x: base.period_x,
            layers,
        }
    }

    /// A card for one of this module's layers (`rail.*`, `fact.*`, `logi.*`).
    pub fn card(&self, layer: &str, id: &str) -> Option<Card> {
        if layer.starts_with("rail.") {
            return draw::rail_card(self.rail.as_ref()?, layer, id);
        }
        if layer.starts_with("fact.") || layer.starts_with("logi.") {
            return draw::block_card(&self.survey, layer, id);
        }
        None
    }

    /// Whether `layer` is one of this module's.
    pub fn owns(layer: &str) -> bool {
        layer.starts_with("rail.") || layer.starts_with("fact.") || layer.starts_with("logi.")
    }

    /// Stations and trains whose names hold `q`, best first.
    pub fn find(&self, q: &str, limit: usize) -> Vec<(u8, Found)> {
        let Some(r) = &self.rail else {
            return Vec::new();
        };
        let dims = r.dims.clone();
        let frame_of = move |d: i32| dims.get(d as usize).cloned().unwrap_or_default();
        draw::find(r, &frame_of, q, limit)
    }

    /// The block entities read in `chunks` of `frame` (asking for those not read yet, first),
    /// and how many of the chunks are not read yet.
    pub fn in_chunks(&self, frame: &str, chunks: &[(i32, i32)]) -> (Vec<Thing>, usize) {
        self.survey.want(frame, chunks.iter().copied(), true);
        self.survey.in_chunks(frame, chunks.iter().copied())
    }

    /// For `/api/status`.
    pub fn status(&self) -> serde_json::Value {
        let (chunks, queued, failed, regions) = self.survey.stats();
        serde_json::json!({
            "rail": self.rail.as_ref().map(|r| serde_json::json!({
                "networks": r.graphs.len(),
                "edges": r.graphs.iter().map(|g| g.edges.len()).sum::<usize>(),
                "stations": r.stations.len(),
                "signals": r.signals.len(),
                "trains": r.trains.len(),
            })),
            "rail_error": self.rail_error,
            "lines_on_track": self.routes.len(),
            "survey": { "chunks_read": chunks, "chunks_queued": queued, "chunks_failed": failed, "regions_read": regions },
        })
    }

    /// Why the rail is missing, if it did not read.
    pub fn rail_error(&self) -> Option<&str> {
        self.rail_error.as_deref()
    }
}
