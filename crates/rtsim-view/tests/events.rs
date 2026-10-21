// SPDX-License-Identifier: AGPL-3.0-or-later
//! The event archive as the page's timeline (VIEWER.md §10.6, V3's viewer side): the core GameTest
//! save with an archive beside its pages answers `/api/events` and the histogram in the page's
//! frame names, gives a frame that has only events a page frame, hides chunks tagged above the
//! generation it views (§10.5), and writes nothing. Without an archive the timeline is off.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mic_viewer::api::{EventsAnswer, Histogram, MarksAnswer, Tick, WorldInfo};
use mic_viewer::viewer::App;
use rtsim_archive::{
    Archive, Dictionary, Event, Handle, Leg, Pending, Row, Severity, Stamp, dir_for,
};
use rtsim_view::{Limits, View};
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

fn core_copy(tag: &str) -> Tmp {
    let dir = std::env::temp_dir().join(format!("rtsim-view-events-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&data("core"), &dir);
    Tmp(dir)
}

fn raid(d: &mut Dictionary, tick: u64, frame: &str, x: i32) -> Row {
    Row::Event(Event {
        tick,
        kind: d.kind("conflict.raid").unwrap(),
        frame: d.frame(frame).unwrap(),
        severity: Severity::Notable,
        x,
        z: 40,
        subject: Handle::direct(12),
        object: Handle::direct(34),
        magnitude: 5,
        ..Event::default()
    })
}

/// Two flushes: one tagged at the save's own generation, one above it (a save the world never
/// confirmed). The first holds overworld raids, a dragon's leg and nether deals in `dim7`, a
/// dimension the save has no marks in.
fn archive(world: &Path, obs_seq: u64, now: u64) {
    let mut a = Archive::open(dir_for(world)).unwrap();
    let mut d = Dictionary::new();
    let mut p = Pending::new();
    for i in 0..40u64 {
        let t = now - 4_000 + i * 100;
        let r = raid(&mut d, t, "dim0", 100 + i as i32);
        if i == 39 {
            p.push_with_payload(&r, b"loot: 12 iron ingot".to_vec());
        } else {
            p.push(&r);
        }
        if i % 4 == 0 {
            p.push(&Row::Event(Event {
                tick: t,
                kind: d.kind("trade.deal").unwrap(),
                frame: d.frame("dim7").unwrap(),
                severity: Severity::Routine,
                x: -8,
                z: 8,
                subject: Handle::direct(3),
                ..Event::default()
            }));
        }
    }
    p.push(&Row::Leg(Leg {
        t0: now - 1_000,
        duration: 600,
        kind: d.kind("dragons.flight").unwrap(),
        frame: d.frame("dim0").unwrap(),
        severity: Severity::Notable,
        x0: 0,
        z0: 0,
        x1: 300,
        z1: 300,
        subject: Handle::direct(99),
        ..Leg::default()
    }));
    a.flush(
        p.take(),
        Stamp {
            obs_seq,
            tick: now,
            wall_ms: 1_700_000_000_000,
        },
        Some(&d),
    )
    .unwrap();
    let mut p = Pending::new();
    p.push(&raid(&mut d, now + 50, "dim0", 999));
    a.flush(
        p.take(),
        Stamp {
            obs_seq: obs_seq + 1,
            tick: now + 100,
            wall_ms: 1_700_000_005_000,
        },
        Some(&d),
    )
    .unwrap();
}

#[test]
fn the_archive_is_the_timeline_in_the_pages_frame_names() {
    let w = core_copy("timeline");
    let (obs, now, overworld) = {
        let v = View::open(&w.0, Limits::default()).unwrap();
        let s = v.saved();
        (
            s.generation.obs_seq,
            s.now(),
            rtsim_view::view::frame_id(&s, 0),
        )
    };
    archive(&w.0, obs, now as u64);
    let before = snapshot(&w.0);
    let view = Arc::new(View::open(&w.0, Limits::default()).unwrap());
    let app = App::new(Arc::clone(&view));
    let world: WorldInfo = get_json(&app, "/api/world", "");
    assert!(world.has_events);
    let kinds: Vec<&str> = world.event_kinds.iter().map(|k| k.id.as_str()).collect();
    assert_eq!(kinds, ["conflict.raid", "dragons.flight", "trade.deal"]);
    assert!(!world.stream, "a save does not stream");
    assert!(
        world.frames.iter().any(|f| f.id == "dim7"),
        "a frame with only events is a page frame"
    );

    // The newest rows first, in the page's frame name; the chunk above the generation is hidden.
    let a: EventsAnswer = get_json(&app, "/api/events", &format!("frame={overworld}&limit=10"));
    assert!(a.truncated);
    assert_eq!(a.rows.len(), 10);
    let r = &a.rows[0];
    assert_eq!(r.t, now - 4_000 + 3_900);
    assert_eq!(
        r.x, 139.0,
        "the newest raid, not the unconfirmed one at x 999"
    );
    assert_eq!(r.frame.as_deref(), Some(overworld.as_str()));
    assert_eq!(
        (r.subject.as_deref(), r.object.as_deref()),
        (Some("12"), Some("34"))
    );
    assert_eq!(r.text, "loot: 12 iron ingot");
    assert_eq!(a.legs.len(), 1);
    assert_eq!((a.legs[0].t0, a.legs[0].t1), (now - 1_000, now - 400));
    let all: EventsAnswer = get_json(&app, "/api/events", "frame=dim7&limit=500");
    assert_eq!(all.rows.len(), 10);
    assert!(
        all.rows
            .iter()
            .all(|r| r.kind == "trade.deal" && r.frame.as_deref() == Some("dim7"))
    );
    // A kind filter and an area.
    let k: EventsAnswer = get_json(
        &app,
        "/api/events",
        &format!("frame={overworld}&kinds=dragons.flight&limit=50"),
    );
    assert!(k.rows.is_empty() && k.legs.len() == 1);
    let ar: EventsAnswer = get_json(
        &app,
        "/api/events",
        &format!("frame={overworld}&x0=100&z0=0&x1=109&z1=100&limit=50"),
    );
    assert_eq!(ar.rows.len(), 10);

    // The strip: every confirmed raid counted once, the wall clock beside it.
    let h: Histogram = get_json(
        &app,
        "/api/events/histogram",
        &format!("frame={overworld}&buckets=20"),
    );
    assert_eq!(h.series["conflict.raid"].iter().sum::<u64>(), 40);
    assert_eq!(h.series["dragons.flight"].iter().sum::<u64>(), 1);
    assert!(!h.series.contains_key("trade.deal"));
    assert_eq!(h.wall, [[now, 1_700_000_000_000]]);
    assert!(h.from <= now - 4_000 && h.to > now);
    let st: serde_json::Value = get_json(&app, "/api/status", "");
    assert_eq!(st["events"]["chunks"], 1, "{st}");
    assert_eq!(st["events"]["damaged_chunks"], 0);

    // The page's whole session, then the folder as it was: the archive is read, never written.
    let s = session(Arc::clone(&view));
    assert!(s.events >= 11, "{s:?}");
    drop(app);
    drop(view);
    assert_same(&before, &snapshot(&w.0));
}

#[test]
fn a_save_without_an_archive_has_no_timeline() {
    let view = View::open(&data("core"), Limits::default()).unwrap();
    let app = App::new(Arc::new(view));
    let world: WorldInfo = get_json(&app, "/api/world", "");
    assert!(!world.has_events);
    assert!(world.event_kinds.is_empty());
    let a: EventsAnswer = get_json(&app, "/api/events", "limit=10");
    assert!(a.rows.is_empty() && a.legs.is_empty());
    let h: Histogram = get_json(&app, "/api/events/histogram", "from=0&to=24000&buckets=10");
    assert!(h.series.is_empty());
    assert_eq!((h.from, h.to), (0, 24_000 as Tick));
    // A save never publishes: the stream is refused, so no connection waits on it.
    assert_eq!(get(&app, "/api/stream", "").0, 204);
}

#[test]
fn an_empty_layer_says_why_in_its_marks() {
    let view = View::open(&data("core"), Limits::default()).unwrap();
    let (id, why) = view
        .reasons()
        .iter()
        .next()
        .map(|(a, b)| (a.clone(), b.clone()))
        .expect("the core save has an empty layer");
    let app = App::new(Arc::new(view));
    let world: WorldInfo = get_json(&app, "/api/world", "");
    let f = &world.frames[0];
    let b = f.bounds;
    let q = format!(
        "frame={}&x0={}&z0={}&x1={}&z1={}&bpp=4&layers={id}",
        f.id, b[0], b[1], b[2], b[3]
    );
    let a: MarksAnswer = get_json(&app, "/api/marks", &q);
    let lm = a.layers.get(&id).unwrap_or_else(|| panic!("{id} at bpp 4"));
    assert_eq!(lm.empty.as_deref(), Some(why.as_str()));
    let layers: mic_viewer::api::Layers = get_json(&app, "/api/layers", "");
    let d = layers.layers.iter().find(|d| d.id == id).unwrap();
    assert!(!d.description.contains(&why), "said once, by the marks");
}
