// SPDX-License-Identifier: AGPL-3.0-or-later
//! The save viewer on real saves (VIEWER.md §16, V1's exit test): GameTest worlds the mod wrote,
//! each its `rtsim/` pages and `data/mic_rtsim_journal.dat`, copied as they were. The core save is
//! copied without its `rtsim/archive` (the event timeline): `events.rs` needs a real save with no
//! archive and writes its own archive onto a copy of this one. The create save keeps its archive.
//!
//! - `tests/data/core`: neoforge/core's GameTest world, a fresh one, run from commit dd15c3f
//!   (EQUILIBRIUM.md S2 on main a92c5f3: the herds' pages are VERSION 4, each herd row its range's
//!   K, range, home and vacancy; an industry district's glass state is a page of table 0x4903
//!   starting with IGL1, and the industry globals end with the glass counters and records
//!   received; the dragons' pages are RTD6 and RTN6, the nest table; an industry district page
//!   starts with IDS4, the stations' ports and five threat classes). Villages, roads, news, herds,
//!   a district and its glass page; no caravan is on the road when the server stops (the harness
//!   world draws caravans). A herd's route draws only when a herd is on the move at the save,
//!   which depends on when the server stops: 6638edc's save had one, while both runs of 98d6d34
//!   and main's 7e4042a had every herd staying put. So the route layer is not required here; it
//!   still has to draw or say why.
//! - `tests/data/create`: neoforge/create's GameTest world, a fresh one, from the same run of
//!   dd15c3f (the same formats). Every module's pages, mostly empty: herds, districts with their
//!   glass pages and stations draw.
//!
//! A page format change before release makes a module unreadable here: the test then names it,
//! and these folders are replaced by a fresh GameTest run.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use rtsim_view::{Limits, Status, View};
use support::*;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn view_fixture(name: &str, draws: &[&str]) {
    let dir = data(name);
    let before = snapshot(&dir);
    let view = View::open(&dir, Limits::default()).expect("the save opens");
    {
        let s = view.saved();
        for m in &s.modules {
            assert_eq!(
                m.status,
                Status::Loaded,
                "{name}: module {} ({})",
                m.name,
                m.detail
            );
            // What the pages read back to is what the engine saved (VWD-11).
            if let Some((now, Some(saved))) = m.hash {
                assert_eq!(
                    now, saved,
                    "{name}: {} reads back to its saved hash",
                    m.name
                );
            }
        }
        assert!(
            s.names.missing.is_none(),
            "{name}: the journal's names read"
        );
    }
    let drawing = every_layer_draws_or_says_why(&view);
    eprintln!("{name}: layers drawing: {drawing:?}");
    for (id, why) in view.reasons() {
        eprintln!("{name}:   {id}: {why}");
    }
    for l in draws {
        assert!(drawing.iter().any(|d| d == l), "{name}: {l} draws");
    }
    assert!(
        view.frames().any(|f| f == "minecraft:overworld"),
        "{name}: the overworld by its name"
    );
    assert!(!view.tile_layers().is_empty(), "{name}: a HABITAT base map");
    let hashes = view.saved().state_hashes();
    let view = Arc::new(view);
    let s = session(Arc::clone(&view));
    eprintln!("{name}: {s:?}");
    assert!(s.cards > 0 && s.tiles_drawn > 0, "{name}: {s:?}");
    assert_eq!(
        view.saved().state_hashes(),
        hashes,
        "{name}: viewing changed no state"
    );
    drop(view);
    assert_same(&before, &snapshot(&dir));
}

#[test]
fn the_core_gametest_save_draws_every_layer_and_writes_nothing() {
    view_fixture(
        "core",
        &[
            "settle.villages",
            "trade.prices",
            "travel.roads",
            "travel.messengers",
            "ecology.herds",
            "industry.districts",
        ],
    );
}

#[test]
fn the_create_gametest_save_draws_every_layer_and_writes_nothing() {
    view_fixture(
        "create",
        &["ecology.herds", "industry.districts", "industry.stations"],
    );
}
