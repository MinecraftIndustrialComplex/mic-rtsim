// SPDX-License-Identifier: AGPL-3.0-or-later
//! A save that does not fully read (VIEWER.md §8, §13): each page file of the core GameTest save
//! made unreadable in turn, and a district's pages taken away under a running view. The viewer
//! never crashes, says which modules did not read, keeps the others drawing, never caches a card
//! that failed, and no answer names a filesystem path beyond the world's name. Then one session
//! over a copy no one may write to, so any attempt to create, truncate or rename fails loudly.
#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mic_viewer::api::{Card, Layers, MarksAnswer, WorldInfo};
use mic_viewer::viewer::{App, Backend};
use rtsim_view::{Limits, Status, View};
use support::*;

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

fn set_mode(p: &Path, mode: u32) {
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Every mode under `dir` set to `file` for files and `folder` for folders (the root last when
/// taking write away, first when giving it back).
fn set_tree(dir: &Path, file: u32, folder: u32) {
    if folder & 0o200 != 0 {
        set_mode(dir, folder);
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            set_tree(&p, file, folder);
        } else {
            set_mode(&p, file);
        }
    }
    if folder & 0o200 == 0 {
        set_mode(dir, folder);
    }
}

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        if self.0.exists() {
            set_tree(&self.0, 0o644, 0o755);
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn core_copy(tag: &str) -> Tmp {
    let dir = std::env::temp_dir().join(format!(
        "rtsim-view-unreadable-{tag}-{}",
        std::process::id()
    ));
    if dir.exists() {
        set_tree(&dir, 0o644, 0o755);
        let _ = std::fs::remove_dir_all(&dir);
    }
    copy_dir(&data("core"), &dir);
    Tmp(dir)
}

/// Every answer a page could ask for: world, layers, status, marks in every frame at two zooms,
/// every mark's card. Panics if any carries `path`. Returns the cards answered.
fn no_answer_names(app: &App<View>, view: &View, path: &str) -> usize {
    let check = |what: &str, body: &[u8]| {
        let text = String::from_utf8_lossy(body);
        assert!(!text.contains(path), "{what} names the folder: {text}");
    };
    for route in ["/api/world", "/api/layers", "/api/status"] {
        let (status, body) = get(app, route, "");
        assert_eq!(status, 200, "{route}");
        check(route, &body);
    }
    let world: WorldInfo = get_json(app, "/api/world", "");
    let layers: Layers = get_json(app, "/api/layers", "");
    let all: Vec<&str> = layers.layers.iter().map(|l| l.id.as_str()).collect();
    let mut cards = 0;
    for f in &world.frames {
        let b = f.bounds;
        for bpp in [0.5, 64.0] {
            let q = format!(
                "frame={}&x0={}&z0={}&x1={}&z1={}&bpp={bpp}&layers={}",
                f.id,
                b[0],
                b[1],
                b[2],
                b[3],
                all.join(",")
            );
            let (status, body) = get(app, "/api/marks", &q);
            assert_eq!(status, 200);
            check("/api/marks", &body);
            let _: MarksAnswer = serde_json::from_slice(&body).unwrap();
        }
        let scene = view.scene(&f.id, None).unwrap();
        for d in &layers.layers {
            for m in scene.layers.get(&d.id).map(|r| r.marks()).unwrap_or(&[]) {
                let Some(id) = m.id() else { continue };
                let route = format!("/api/thing/{}/{id}", d.id);
                let (status, body) = get(app, &route, "");
                if status == 200 {
                    check(&route, &body);
                    let _: Card = serde_json::from_slice(&body).unwrap();
                    cards += 1;
                }
            }
        }
    }
    cards
}

#[test]
fn each_page_file_unreadable_in_turn_never_crashes_or_names_a_path() {
    let pages: Vec<String> = std::fs::read_dir(data("core/rtsim"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("seg-") || n.starts_with("mlog-"))
        .collect();
    assert!(pages.len() >= 3, "{pages:?}");
    let mut opened = 0;
    let mut unreadable_modules = 0;
    let mut caught_panics = 0;
    for page in &pages {
        let w = core_copy("each");
        let file = w.0.join("rtsim").join(page);
        set_mode(&file, 0o000);
        let root = w.0.display().to_string();
        // The store itself may refuse to open (its error goes to the terminal, not a page).
        let Ok(view) = View::open(&w.0, Limits::default()) else {
            continue;
        };
        opened += 1;
        for m in &view.saved().modules {
            if let Status::Unreadable(e) = &m.status {
                unreadable_modules += 1;
                // A paged table that panics on a page it cannot read (rows.rs) is caught.
                caught_panics += usize::from(e.starts_with("reading its pages failed"));
            }
        }
        let view = Arc::new(view);
        let app = App::new(Arc::clone(&view));
        no_answer_names(&app, &view, &root);
    }
    assert!(opened >= 2, "most single-file losses still open");
    assert!(unreadable_modules > 0, "and say what did not read");
    // Whether a loss reaches a paged table that panics depends on the save's segment layout (the
    // guard has its own test in load.rs); when one does, it was reported, not a crash.
    eprintln!("{caught_panics} module loads panicked and were reported");
}

#[test]
fn a_district_whose_pages_go_away_is_not_cached_and_names_no_path() {
    let w = core_copy("gone");
    let view = Arc::new(View::open(&w.0, Limits::default()).unwrap());
    let id = {
        let s = view.saved();
        let v = s.industry.as_ref().expect("the core save has industry");
        v.districts.first().expect("a district").id
    };
    let app = App::new(Arc::clone(&view));
    let root = w.0.display().to_string();
    // The segments move away, as a live engine's compaction would replace them.
    let aside = w.0.join("aside");
    std::fs::create_dir_all(&aside).unwrap();
    let segs: Vec<PathBuf> = std::fs::read_dir(w.0.join("rtsim"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("seg-"))
        .collect();
    for p in &segs {
        std::fs::rename(p, aside.join(p.file_name().unwrap())).unwrap();
    }
    let route = format!("/api/thing/industry.districts/{id}");
    let c: Card = get_json(&app, &route, "");
    let note = c.note.clone().unwrap_or_default();
    assert!(note.contains("did not read"), "{note}");
    assert!(note.contains("restart the viewer"), "{note}");
    assert!(!note.contains(&root), "{note}");
    // Back again: the card reads, under its own id and with a leading zero alike.
    for p in &segs {
        std::fs::rename(aside.join(p.file_name().unwrap()), p).unwrap();
    }
    for asked in [id.to_string(), format!("0{id}")] {
        let c: Card = get_json(&app, &format!("/api/thing/industry.districts/{asked}"), "");
        assert!(
            !c.note.clone().unwrap_or_default().contains("did not read"),
            "{asked}: {:?}",
            c.note
        );
        assert_eq!(c.id, id.to_string());
    }
}

#[test]
fn a_session_over_a_save_no_one_may_write_to() {
    let w = core_copy("readonly");
    set_tree(&w.0, 0o444, 0o555);
    let before = snapshot(&w.0);
    let view = Arc::new(View::open(&w.0, Limits::default()).expect("a read-only save opens"));
    let s = session(Arc::clone(&view));
    assert!(s.cards > 0 && s.tiles_drawn > 0, "{s:?}");
    drop(view);
    assert_same(&before, &snapshot(&w.0));
}
