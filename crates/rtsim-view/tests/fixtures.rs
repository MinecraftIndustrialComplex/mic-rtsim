// SPDX-License-Identifier: AGPL-3.0-or-later
//! The save viewer on real saves (VIEWER.md §16, V1's exit test): GameTest worlds the mod wrote,
//! each its `rtsim/` pages and `data/mic_rtsim_journal.dat`, copied as they were. The core save is
//! copied without its `rtsim/archive` (the event timeline): `events.rs` needs a real save with no
//! archive and writes its own archive onto a copy of this one. The create save keeps its archive.
//!
//! - `tests/data/core`: neoforge/core's GameTest world, a fresh one, run from commit c67da61
//!   (SEC-1's slice S5-2 merged with main 095be6e; S5-2's formats: the glass pages IGL7, the
//!   regimes' and replays' network inputs and published rows, choice SC153; before that, b643465,
//!   SEC-1's slices S4c2 and S5-1 merged with main 10d2330, which carries the Mowzie's Mobs camp
//!   reconciler M4 and M5; M5's formats: the conflict's pages are RTC6 with meta version 5, each
//!   faction row carrying its cap, its members away and its camp ledger, and the wheel's events may
//!   be a camp refill, MOWZIE.md section 4.9; S4c2's and S5-1's formats: the package stores' pages
//!   IST4, the drawn packages toward each frogport, the district each was drawn by and the drawn
//!   counters, choices SC141 to SC143, and the glass pages IGL6, each kinetic domain's sources and
//!   each unit's capacity per speed; before that, 8188cc0, S4c2 and S5-1 on main 428a331, and
//!   37a34e40, M4 and M5 merged with main 428a331; S4c1's formats: the package stores' pages IST3,
//!   each store's frontier epoch, the packages it holds toward each frogport with their keys, and
//!   the held counters, choice SC137; before that, 02d7ea4, SEC-1's slice S4c1 merged with main
//!   f361a6f; before that, edbb390, SEC-1's slice S4p merged with main a95bd2a; S4p's formats: the
//!   engine's meta page RTJM version 2, with the journal's newest acknowledged generation, and its
//!   closed generations as pages of table 2, docs/boundary.md §3.2, and the journal's `pinnedGen`,
//!   SC130; before that, 5d648ce, SEC-1's slice S4 merged with main 5489045; S4's formats:
//!   `IndustryData` format 3, the package stores' pages IST2 in table 0x4906, and RG14's
//!   `rtsim/bridge-life.txt`, the engine life; before that, 21e57d2, SEC-1's slice S3 on main
//!   3391a5d; the formats of 0a5a4c2, Mowzie M3 merged with main 89fb716 and CL-D61, but for the
//!   glass page: the phase 1 world's catalogue has 10 goods, `gold_nugget` last; the booked train
//!   legs are pages of their own, table 0x4904, ILEG, and the industry globals page starts with
//!   IGB1 (docs/stress-worlds-results.md 10.10); the conflict's pages were RTC5 with meta version
//!   4, each party's recalled stacks kept per epoch, each feud's kept notice with its serial and
//!   its Envoy hold, the Peaceful flag and start and the last notice serial, each faction's
//!   crystal, the destroyed loot and the repeats; a node may be a lair, tag 3; the ships' pages are
//!   RTS4, each port's blockade state and each laid-up slot's idle spells in a trailing section
//!   written only with closures on, and the world's pages are RTW4, each edge's closed flag;
//!   S8b-3's ships stats and ledger; the dragons' pages are RTD7 and RTN7, each lair's pressure,
//!   wait and fed share, and the nest table; the herds' pages are VERSION 4, each herd row its
//!   range's K, range, home and vacancy; an industry district's glass state is a page of table
//!   0x4903 (IGL1 before S2), and the industry globals end with the glass counters and records
//!   received; since S3 the glass page starts with IGL5, its sections, the judging units' windows,
//!   the station joins, the station discards' count and each unit window's base at its counted
//!   stores (SECTIONS.md §4.2, §5); an industry district page starts with IDS4, the stations' ports
//!   and five threat classes). Villages, roads, news, herds, a district and its glass page; no
//!   caravan is on the road when the server stops (the harness world draws caravans). A herd's
//!   route draws only when a herd is on the move at the save, which depends on when the server
//!   stops: 6638edc's save had one, while both runs of 98d6d34 and main's 7e4042a had every herd
//!   staying put. So the route layer is not required here; it still has to draw or say why.
//! - `tests/data/create`: neoforge/create's GameTest world, run from fe87a92 (S5-2 merged with
//!   main f66cb73; the core save's formats; before that, b643465, 37a34e40 as part of the plain
//!   `./gametest.sh core create mowziesmobs` round, 8188cc0, cee946f, S4c1, edbb390, 5d648ce,
//!   ec57cd7, 21e57d2 and S3's arm funnel-point fix) with the section fixtures (`-Psec1fx`), its
//!   `run-gametest/world` deleted first (delete `run-gametest/world` before a create run that
//!   follows a stress-generator run, whose clock is ahead of the game). Every module's pages,
//!   mostly empty: herds, districts with their glass pages and stations draw.
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

/// Slice S7's layers on the create GameTest save: it holds package stores (frogports with no
/// district, from the S4 GameTests) that draw as edges and have cards; its glass pages are in an
/// older format, so the sections layer is empty and says so, and the viewer names the pages it
/// left out.
#[test]
fn the_create_save_draws_its_package_stores_and_says_why_sections_are_empty() {
    let dir = data("create");
    let before = snapshot(&dir);
    let view = View::open(&dir, Limits::default()).expect("the save opens");
    let drawing = every_layer_draws_or_says_why(&view);
    assert!(
        drawing.iter().any(|d| d == "industry.stores"),
        "the stores draw: {drawing:?}"
    );
    let n = view
        .saved()
        .industry
        .as_ref()
        .expect("industry")
        .stores
        .len();
    assert_eq!(n, 6, "the save holds six package stores");
    for l in ["industry.sections", "industry.networks"] {
        if !drawing.iter().any(|d| d == l) {
            assert!(view.reasons().contains_key(l), "{l} says why it is empty");
        }
    }
    let unread = view.saved().industry.as_ref().unwrap().glass_unread.pages();
    if unread > 0 {
        let why = &view.reasons()["industry.sections"];
        assert!(
            why.contains("glass page") && why.contains("did not read"),
            "{why}"
        );
    }
    // The layer draws a chain, not every pair of a store's frogports, and every frogport of a
    // store whose hops Java measured is on it.
    for st in &view.saved().industry.as_ref().unwrap().stores {
        let n = st.frogports.len();
        let links = rtsim_layers::industry::chain_links(st, n, usize::MAX);
        eprintln!("store {}: {n} frogports, links {links:?}", st.id.0);
        assert!(
            links.len() <= n.saturating_sub(1),
            "store {}: {links:?} for {n}",
            st.id.0
        );
        if !st.hops.contains(&rtsim_layers::industry::UNKNOWN_HOPS) {
            let mut on = vec![false; n];
            on[0] = true;
            for _ in 0..n {
                for &(i, j) in &links {
                    let (i, j) = (usize::from(i), usize::from(j));
                    let joined = on[i] || on[j];
                    on[i] = joined;
                    on[j] = joined;
                }
            }
            assert!(
                on.iter().all(|&x| x),
                "store {}: links {links:?} leave a frogport off the chain (hops {:?})",
                st.id.0,
                st.hops
            );
        }
    }
    let (id, edge) = {
        let saved = view.saved();
        let first = &saved.industry.as_ref().unwrap().stores[0];
        (
            first.id.0,
            rtsim_layers::industry::store_mark(first).links[0],
        )
    };
    let view = Arc::new(view);
    let app = mic_viewer::viewer::App::new(Arc::clone(&view));
    let card: mic_viewer::api::Card =
        get_json(&app, &format!("/api/thing/industry.stores/{id}"), "");
    assert_eq!(card.layer, "industry.stores");
    assert!(card.sections.iter().any(|s| s.title == "Frogports"));
    // A drawn edge's id ("store:i-j") opens its store's card.
    let by_edge: mic_viewer::api::Card = get_json(
        &app,
        &format!("/api/thing/industry.stores/{id}:{}-{}", edge.0, edge.1),
        "",
    );
    assert_eq!(by_edge.layer, "industry.stores");
    assert_eq!(by_edge.title, card.title, "the edge's card is its store's");
    let (status, _) = get(&app, "/api/thing/industry.stores/999999", "");
    assert_eq!(status, 404, "a store the save lacks has no card");
    let layers: mic_viewer::api::Layers = get_json(&app, "/api/layers", "");
    let sections = layers
        .layers
        .iter()
        .find(|l| l.id == "industry.sections")
        .expect("the sections layer");
    assert!(
        sections.max_bpp <= 1.0,
        "sections draw at a block a pixel and closer"
    );
    drop(view);
    assert_same(&before, &snapshot(&dir));
}
