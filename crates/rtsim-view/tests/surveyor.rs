// SPDX-License-Identifier: AGPL-3.0-or-later
//! Surveyor's terrain as the base map (VIEWER.md §16, V5's exit test, on a fixture): the core
//! GameTest save with one Surveyor region beside it draws that region at one block a pixel over
//! HABITAT, and the same save without it keeps HABITAT alone. Viewing writes nothing either way.
//!
//! `tests/data/surveyor/c.2.0.dat` is region (2, 0) of the MICTest save's overworld (blocks 1,024
//! to 1,535 east, 0 to 511 south) as Surveyor 1.2.4 wrote it: game data, no Surveyor code.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mic_viewer::api::WorldInfo;
use mic_viewer::tiles::{Rendered, TILE, TileSource};
use mic_viewer::viewer::App;
use mic_viewer_surveyor::Surveyor;
use rtsim_view::surveyor::SurveyorTiles;
use rtsim_view::{Limits, View};
use support::*;

const OVERWORLD: &str = "surveyor-minecraft-overworld";

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

/// The core save with Surveyor's region (2, 0) in its overworld.
fn world_with_surveyor(tag: &str) -> Tmp {
    let dir =
        std::env::temp_dir().join(format!("rtsim-view-surveyor-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&data("core"), &dir);
    let s = dir.join("data/surveyor");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::copy(data("surveyor/c.2.0.dat"), s.join("c.2.0.dat")).unwrap();
    Tmp(dir)
}

#[test]
fn surveyor_draws_one_block_a_pixel_over_habitat() {
    let w = world_with_surveyor("draws");
    let before = snapshot(&w.0);
    let view = Arc::new(View::open(&w.0, Limits::default()).expect("the save opens"));
    assert_eq!(
        view.surveyor_layers().iter().collect::<Vec<_>>(),
        [OVERWORLD]
    );
    let app = App::new(Arc::clone(&view));
    let world: WorldInfo = get_json(&app, "/api/world", "");
    let f = world
        .frames
        .iter()
        .find(|f| f.id == "minecraft:overworld")
        .expect("the overworld");
    // HABITAT first, Surveyor over it, both shown.
    let ids: Vec<&str> = f.tiles.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(ids[0].starts_with("habitat"), "{ids:?}");
    assert_eq!(ids[1], OVERWORLD);
    let d = &f.tiles[1];
    assert_eq!(d.pyramid.max_zoom, 0, "zoom 0 is a block a pixel");
    assert_eq!(d.default_on, Some(true));
    assert_eq!(d.bounds, [1_024.0, 0.0, 1_536.0, 512.0]);
    assert!(
        f.bounds[0] <= 1_024.0 && f.bounds[2] >= 1_536.0,
        "the frame holds the map"
    );

    // Zoom 0: four tiles of 256 blocks cover the region, a recorded column a pixel.
    let region = Surveyor::open(data("surveyor"))
        .unwrap()
        .unwrap()
        .read_region(2, 0)
        .unwrap();
    assert!(
        region.known() > 8_000,
        "the fixture has land: {}",
        region.known()
    );
    let tiles = SurveyorTiles::open(
        &w.0,
        &[("minecraft:overworld".into(), "minecraft-overworld".into())],
        16 << 20,
        Arc::new(std::sync::Mutex::new(())),
    )
    .unwrap();
    let t = TILE as i64;
    let mut opaque = 0;
    let mut drawn = Vec::new();
    for ty in 0..2 {
        for tx in 4..6 {
            let Rendered::Rgba(px) = tiles.render(OVERWORLD, 0, tx, ty).unwrap() else {
                continue;
            };
            drawn.push((tx, ty));
            assert_eq!(px.len() as i64, t * t * 4);
            for (i, p) in px.as_chunks::<4>().0.iter().enumerate() {
                let (x, z) = (tx * t + i as i64 % t, ty * t + i as i64 / t);
                let known = region.floor_at(x as i32, z as i32).is_some();
                assert_eq!(p[3] > 0, known, "block ({x}, {z})");
                opaque += usize::from(p[3] > 0);
            }
        }
    }
    assert_eq!(opaque, region.known());
    // Outside the region: nothing (204), and a zoom past the pyramid is 404.
    let (status, _) = get(&app, &format!("/tiles/{OVERWORLD}/0/0/0.png"), "");
    assert_eq!(status, 204);
    let (status, _) = get(&app, &format!("/tiles/{OVERWORLD}/1/8/0.png"), "");
    assert_eq!(status, 404);
    // Through the route, as the page asks: a PNG.
    let (tx, ty) = drawn[0];
    let (status, body) = get(&app, &format!("/tiles/{OVERWORLD}/0/{tx}/{ty}.png"), "");
    assert_eq!(status, 200);
    assert_eq!(&body[..8], b"\x89PNG\r\n\x1a\n");
    let st: serde_json::Value = get_json(&app, "/api/status", "");
    assert!(
        st["surveyor"]["regions_parsed"].as_u64().unwrap() >= 1,
        "{st}"
    );

    // A whole session over it, then the folder as it was.
    let s = session(Arc::clone(&view));
    assert!(s.tiles_drawn > 0, "{s:?}");
    drop(app);
    drop(view);
    assert_same(&before, &snapshot(&w.0));
}

#[test]
fn a_world_without_surveyor_keeps_habitat() {
    let view = View::open(&data("core"), Limits::default()).expect("the save opens");
    assert!(view.surveyor_layers().is_empty());
    assert!(!view.tile_layers().is_empty());
    let app = App::new(Arc::new(view));
    let world: WorldInfo = get_json(&app, "/api/world", "");
    for f in &world.frames {
        assert!(f.tiles.iter().all(|t| !t.id.starts_with("surveyor")));
    }
    assert!(world.frames.iter().any(|f| !f.tiles.is_empty()));
}

#[test]
fn a_frame_known_only_by_number_has_no_surveyor_folder() {
    let w = world_with_surveyor("dim0");
    let none = SurveyorTiles::open(
        &w.0,
        &[("dim0".into(), "dim0".into())],
        1 << 20,
        Arc::new(std::sync::Mutex::new(())),
    );
    assert!(none.is_none());
}
