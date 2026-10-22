// SPDX-License-Identifier: AGPL-3.0-or-later
//! Create's layers on a save (docs/viewer/industry-map.md): the core GameTest save with the
//! create-rail fixture's `create_tracks.dat` and region file beside it. The track, its stations and
//! its train draw at open; the factory's block entities are read only when the page looks closer
//! than `LOOK_BPP`, land on the stream, and fill the district and rail cards. Viewing writes
//! nothing.
//!
//! `tests/data/create-rail` is cut from a GameTest world mic-rtsim's `RailFixtureGameTests` built:
//! a straight, a quarter turn, an S-bend, two stations ("Fixture Central", "Fixture Yard"), a
//! signal, a two-carriage train ("Fixture Express"), a chain conveyor, frogport, packager, belt and
//! vault beside it (see `extract_rail_fixture.py`).

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mic_viewer::api::WorldInfo;
use mic_viewer::viewer::App;
use rtsim_view::create::LOOK_BPP;
use rtsim_view::{Limits, View};
use serde_json::Value;
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

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The core save with Create's rail and region beside it.
fn world(tag: &str) -> Tmp {
    let dir = std::env::temp_dir().join(format!("rtsim-view-create-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&data("core"), &dir);
    copy_dir(&data("create-rail"), &dir);
    Tmp(dir)
}

fn overworld(w: &WorldInfo) -> String {
    w.frames
        .iter()
        .find(|f| f.id.contains("overworld"))
        .map(|f| f.id.clone())
        .expect("an overworld frame")
}

/// The fixture's marks in `layers` at `bpp`, as JSON, by layer.
fn marks(app: &App<View>, frame: &str, layers: &[&str], bpp: f64) -> Value {
    let q = format!(
        "frame={frame}&layers={}&x0=590&z0=1590&x1=660&z1=1670&bpp={bpp}",
        layers.join(",")
    );
    get_json::<Value>(app, "/api/marks", &q)["layers"].clone()
}

fn count(v: &Value, layer: &str) -> usize {
    v[layer]["marks"].as_array().map_or(0, Vec::len)
}

#[test]
fn the_rail_draws_at_open_and_the_factory_when_looked_at() {
    let w = world("draw");
    let before = snapshot(&w.0);
    let view = Arc::new(View::open(&w.0, Limits::default()).unwrap());
    let app = App::new(Arc::clone(&view));
    let info: WorldInfo = get_json(&app, "/api/world", "");
    assert!(info.stream, "a save with region files streams its landings");
    assert!(info.find);
    let frame = overworld(&info);
    let layer_ids: Vec<String> = get_json::<Value>(&app, "/api/layers", "")["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_owned())
        .collect();
    for l in [
        "rail.track",
        "rail.stations",
        "rail.trains",
        "logi.chains",
        "fact.storage",
    ] {
        assert!(layer_ids.iter().any(|x| x == l), "{l} in {layer_ids:?}");
    }

    // Far out, the track, its stations and the train draw; nothing is read for the factory.
    let rail = ["rail.track", "rail.stations", "rail.signals", "rail.trains"];
    let far = marks(&app, &frame, &rail, 8.0);
    assert!(count(&far, "rail.track") > 0, "{far}");
    assert_eq!(count(&far, "rail.stations"), 2);
    assert_eq!(count(&far, "rail.trains"), 1);
    let train = &far["rail.trains"]["marks"][0];
    assert_eq!(train["k"], "train");
    assert_eq!(train["cars"].as_array().unwrap().len(), 2);
    assert!(train["l"].as_str().unwrap().contains("Fixture Express"));
    // The chunks round each station are read at open; a far look reads nothing more.
    let survey = view.create().survey();
    assert!(survey.settle(Duration::from_secs(60)));
    let at_open = survey.stats();
    assert!(at_open.0 >= 9, "3 by 3 chunks round a station: {at_open:?}");
    let blocks = ["logi.chains", "logi.belts", "logi.ports", "fact.storage"];
    let _ = marks(&app, &frame, &blocks, 8.0);
    assert!(survey.settle(Duration::from_secs(60)));
    assert_eq!(survey.stats(), at_open, "a far look reads nothing");

    // Close in, the chunks shown are read too; once they land the factory draws.
    let _ = marks(&app, &frame, &blocks, LOOK_BPP / 2.0);
    assert!(survey.settle(Duration::from_secs(60)));
    let looked = survey.stats();
    assert!(looked.0 > at_open.0, "{at_open:?} then {looked:?}");
    assert_eq!(looked.2, 0, "no chunk failed");
    let near = marks(&app, &frame, &blocks, LOOK_BPP / 2.0);
    assert_eq!(count(&near, "logi.chains"), 1, "{near}");
    assert_eq!(count(&near, "logi.belts"), 1, "the belt draws as one run");
    assert!(count(&near, "logi.ports") >= 2);
    assert_eq!(count(&near, "fact.storage"), 1);

    let status: Value = get_json(&app, "/api/status", "");
    assert_eq!(status["create"]["rail"]["trains"], 1, "{status}");
    assert!(status["create"]["survey"]["chunks_read"].as_u64().unwrap() > 0);
    drop(app);
    drop(view);
    assert_same(&before, &snapshot(&w.0));
}

#[test]
fn find_and_cards_name_the_rail() {
    let w = world("find");
    let view = Arc::new(View::open(&w.0, Limits::default()).unwrap());
    let app = App::new(Arc::clone(&view));
    let found: Value = get_json(&app, "/api/find", "q=fixture&limit=10");
    let labels: Vec<&str> = found["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["label"].as_str().unwrap())
        .collect();
    for want in ["Fixture Central", "Fixture Yard", "Fixture Express"] {
        assert!(
            labels.iter().any(|l| l.contains(want)),
            "{want} in {labels:?}"
        );
    }
    let express = found["found"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"].as_str().unwrap().contains("Fixture Express"))
        .unwrap();
    let card: Value = get_json(
        &app,
        &format!(
            "/api/thing/{}/{}",
            express["layer"].as_str().unwrap(),
            express["id"].as_str().unwrap()
        ),
        "",
    );
    let text = card.to_string();
    assert!(text.contains("Fixture Yard"), "its schedule: {text}");
    assert!(text.contains("Coal"), "its cargo: {text}");
}

#[test]
fn a_save_without_create_says_why_its_rail_is_empty() {
    let view = View::open(&data("core"), Limits::default()).unwrap();
    let app = App::new(Arc::new(view));
    let info: WorldInfo = get_json(&app, "/api/world", "");
    assert!(!info.stream, "no region files: nothing lands, no stream");
    let layers: Value = get_json(&app, "/api/layers", "");
    let ids: Vec<&str> = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert!(
        !ids.contains(&"rail.track"),
        "no track, no rail layers: {ids:?}"
    );
    // The factory's layers are there, and say why they are empty.
    let frame = overworld(&info);
    let m = marks(&app, &frame, &["logi.chains"], 1.0);
    let why = m["logi.chains"]["empty"].as_str().unwrap_or_default();
    assert!(why.contains("zoom in") || why.contains("none"), "{m}");
}
