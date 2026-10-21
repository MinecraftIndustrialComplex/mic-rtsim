// SPDX-License-Identifier: AGPL-3.0-or-later
//! The determinism check the GameTest saves carry (VWD-11): a TEST save writes each module's state
//! hash beside its meta page, and the viewer's decode must read back to it. Which modules carry
//! one is pinned here, so a module that stops writing its hash (or a viewer that stops reading it)
//! fails instead of passing an empty check; and a hash that differs is flagged in `/api/status`
//! (and by `rtsim-cli view`'s start-up lines, from the same `Module::hash_matches`).

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mic_viewer::viewer::App;
use rtsim_kernel::codec::{Reader, Writer};
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_view::{Limits, View, keys};
use support::*;

/// Every module but industry, whose pages carry a hash of their bytes instead (`pages_hash`).
const HASHED: [&str; 6] = [
    "phase1",
    "world_model",
    "dragons",
    "herds",
    "ships",
    "conflict",
];

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dest = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_gametest_save_carries_and_matches_each_modules_hash() {
    for name in ["core", "create"] {
        let view = View::open(&data(name), Limits::default()).expect("the save opens");
        let s = view.saved();
        let matched: Vec<&str> = s
            .modules
            .iter()
            .filter(|m| m.hash_matches() == Some(true))
            .map(|m| m.name)
            .collect();
        assert_eq!(matched, HASHED, "{name}: the modules whose hash matches");
        assert!(
            s.modules.iter().all(|m| m.hash_matches() != Some(false)),
            "{name}: no hash differs"
        );
        let industry = s.modules.iter().find(|m| m.name == "industry").unwrap();
        assert_eq!(industry.hash_matches(), None, "{name}: industry has none");
    }
}

#[test]
fn a_hash_that_differs_is_flagged() {
    let dir = std::env::temp_dir().join(format!("rtsim-view-hashes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&data("core"), &dir);
    let w = Tmp(dir);
    // The herds' hash page, rewritten one bit off, in a save of its own at the same journal point.
    {
        let mut store = PageStore::open(w.0.join("rtsim"), Durability::NoSync).unwrap();
        let key = keys::hash_key(keys::HERDS);
        let page = store.get(key).unwrap().expect("a TEST save's hash page");
        let mut r = Reader::new(&page);
        assert_eq!(r.u32().unwrap(), keys::HASH_MAGIC);
        let hash = r.u64().unwrap();
        let mut out = Writer::new();
        out.u32(keys::HASH_MAGIC);
        out.u64(hash ^ 1);
        let mut batch = SaveBatch::new();
        batch.put(key, &out.into_bytes());
        let obs = store.current().obs_seq;
        store.commit(batch, obs).unwrap();
    }
    let view = Arc::new(View::open(&w.0, Limits::default()).expect("the save opens"));
    {
        let s = view.saved();
        for m in &s.modules {
            let want = match m.name {
                "herds" => Some(false),
                "industry" => None,
                _ => Some(true),
            };
            assert_eq!(m.hash_matches(), want, "{}", m.name);
        }
    }
    let app = App::new(Arc::clone(&view));
    let st: serde_json::Value = get_json(&app, "/api/status", "");
    let herds = st["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "herds")
        .unwrap()
        .clone();
    assert_eq!(herds["hash_matches"], false, "{herds}");
    assert_ne!(herds["hash"], herds["saved_hash"], "{herds}");
}
