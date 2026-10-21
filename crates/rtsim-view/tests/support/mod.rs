// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the save viewer's tests share: a folder's exact contents, requests through mic-viewer's
//! [`App`] as the page would make them, and a whole viewing session.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use mic_viewer::api::{Card, MARK_BUDGET, MarksAnswer, WorldInfo};
use mic_viewer::http::{Handler, Reply, Request};
use mic_viewer::viewer::App;
use rtsim_view::View;

/// Every file under `dir` with its bytes and modification time, and every folder.
pub type Snapshot = BTreeMap<PathBuf, Option<(Vec<u8>, SystemTime)>>;

pub fn snapshot(dir: &Path) -> Snapshot {
    fn walk(root: &Path, dir: &Path, out: &mut Snapshot) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("read the folder")
            .map(|e| e.expect("an entry").path())
            .collect();
        entries.sort();
        for p in entries {
            let rel = p.strip_prefix(root).unwrap().to_path_buf();
            let meta = std::fs::symlink_metadata(&p).expect("metadata");
            if meta.is_dir() {
                out.insert(rel, None);
                walk(root, &p, out);
            } else {
                let bytes = std::fs::read(&p).expect("read a file");
                out.insert(rel, Some((bytes, meta.modified().expect("an mtime"))));
            }
        }
    }
    let mut out = BTreeMap::new();
    let meta = std::fs::metadata(dir).expect("the folder");
    out.insert(PathBuf::new(), Some((Vec::new(), meta.modified().unwrap())));
    walk(dir, dir, &mut out);
    out
}

/// Asserts two snapshots are the same: names, bytes and modification times.
pub fn assert_same(before: &Snapshot, after: &Snapshot) {
    let names = |s: &Snapshot| s.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names(before), names(after), "the folder's names changed");
    for (k, v) in before {
        let w = &after[k];
        match (v, w) {
            (Some((a, ta)), Some((b, tb))) => {
                assert!(a == b, "{} changed its bytes", k.display());
                assert_eq!(ta, tb, "{} changed its modification time", k.display());
            }
            (None, None) => {}
            _ => panic!("{} changed kind", k.display()),
        }
    }
}

/// A GET through the app: (status, body).
pub fn get(app: &App<View>, path: &str, query: &str) -> (u16, Vec<u8>) {
    let req = Request {
        method: "GET".into(),
        path: path.into(),
        query: query.into(),
        host: "127.0.0.1:8740".into(),
        ..Request::default()
    };
    match app.handle(&req) {
        Reply::Response(r) => (r.status, r.body),
        Reply::Stream(_) => (200, Vec::new()),
    }
}

pub fn get_json<T: serde::de::DeserializeOwned>(app: &App<View>, path: &str, query: &str) -> T {
    let (status, body) = get(app, path, query);
    assert_eq!(
        status,
        200,
        "{path}?{query}: {}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// What a whole session saw.
#[derive(Debug, Default)]
pub struct Session {
    pub requests: usize,
    /// Marks per layer at the whole-world request, summed over frames.
    pub marks: BTreeMap<String, u64>,
    /// The most marks one `/api/marks` answer carried.
    pub most_marks: usize,
    pub cards: usize,
    pub tiles: usize,
    pub tiles_drawn: usize,
}

/// Every route a page uses, over everything the save holds: the world, layers and status; marks
/// for every frame at every zoom the layers have, the whole frame in view; every mark's card;
/// base map tiles across each frame at three zooms.
pub fn session(view: Arc<View>) -> Session {
    let app = App::new(Arc::clone(&view));
    let mut s = Session::default();
    let world: WorldInfo = get_json(&app, "/api/world", "");
    let layers: mic_viewer::api::Layers = get_json(&app, "/api/layers", "");
    let _: serde_json::Value = get_json(&app, "/api/status", "");
    s.requests += 3;
    let ids: Vec<&str> = layers.layers.iter().map(|l| l.id.as_str()).collect();
    let all = ids.join(",");
    for f in &world.frames {
        let b = f.bounds;
        for bpp in [0.25, 4.0, 64.0, 1024.0] {
            let q = format!(
                "frame={}&x0={}&z0={}&x1={}&z1={}&bpp={bpp}&layers={all}",
                f.id, b[0], b[1], b[2], b[3]
            );
            let a: MarksAnswer = get_json(&app, "/api/marks", &q);
            s.requests += 1;
            let n: usize = a.layers.values().map(|l| l.marks.len()).sum();
            s.most_marks = s.most_marks.max(n);
            assert!(n <= MARK_BUDGET, "{n} marks over the budget at bpp {bpp}");
            if bpp == 0.25 {
                for (id, l) in &a.layers {
                    *s.marks.entry(id.clone()).or_default() += l.total;
                }
            }
        }
        // Every mark's card, from the scene itself.
        let scene = mic_viewer::viewer::Backend::scene(&*view, &f.id, None).expect("a scene");
        for d in &layers.layers {
            let Some(rows) = scene.layers.get(&d.id) else {
                panic!("layer {} has no rows in frame {}", d.id, f.id)
            };
            if !d.has_cards {
                continue;
            }
            for m in rows.marks() {
                let Some(id) = m.id() else { continue };
                let path = format!("/api/thing/{}/{}", d.id, id);
                let c: Card = get_json(&app, &path, "");
                s.requests += 1;
                s.cards += 1;
                assert_eq!(c.layer, d.id);
                assert!(!c.title.is_empty(), "{path} has no title");
            }
        }
        // Base map tiles over every sampled region (up to 32), at three zooms, and a few far away.
        let regions: Vec<(i32, i32)> = view
            .saved()
            .world_model
            .as_ref()
            .and_then(|w| {
                let g = w.read();
                g.sampled_ref()
                    .map(|m| m.regions().map(|(k, _)| (k.rx, k.rz)).take(32).collect())
            })
            .unwrap_or_default();
        for t in &f.tiles {
            let (lo, hi) = (t.pyramid.min_zoom, t.pyramid.max_zoom);
            for z in [lo, (lo + hi) / 2, hi] {
                let size = 256.0 * 2f64.powi(-z);
                let mut asked = std::collections::BTreeSet::new();
                for &(rx, rz) in &regions {
                    let (x, z0) = (f64::from(rx) * 512.0 + 256.0, f64::from(rz) * 512.0 + 256.0);
                    asked.insert(((x / size).floor() as i64, (z0 / size).floor() as i64));
                }
                for far in [-1_000_000i64, 1_000_000] {
                    asked.insert((far, far));
                }
                for (x, y) in asked {
                    let (status, body) = get(&app, &format!("/tiles/{}/{z}/{x}/{y}.png", t.id), "");
                    s.requests += 1;
                    s.tiles += 1;
                    assert!(status == 200 || status == 204, "tile {z}/{x}/{y}: {status}");
                    if status == 200 && !body.is_empty() {
                        assert_eq!(&body[1..4], b"PNG");
                        s.tiles_drawn += 1;
                    }
                }
            }
        }
    }
    s
}

/// Asserts every layer draws or says why it is empty, and returns the ones that draw.
pub fn every_layer_draws_or_says_why(view: &View) -> Vec<String> {
    let mut drawing = Vec::new();
    for (id, n) in view.counts() {
        if *n > 0 {
            drawing.push(id.clone());
        } else {
            let why = view.reasons().get(id).map(String::as_str).unwrap_or("");
            assert!(!why.is_empty(), "layer {id} is empty and does not say why");
        }
    }
    assert_eq!(view.counts().len(), rtsim_view::layers::specs().len());
    drawing
}
