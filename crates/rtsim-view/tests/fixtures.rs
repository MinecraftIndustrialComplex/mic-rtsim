// SPDX-License-Identifier: AGPL-3.0-or-later
//! The save viewer on real saves (VIEWER.md §16, V1's exit test): GameTest worlds the mod wrote,
//! each its `rtsim/` pages and `data/mic_rtsim_journal.dat`, copied as they were. The core save is
//! copied without its `rtsim/archive` (the event timeline): `events.rs` needs a real save with no
//! archive and writes its own archive onto a copy of this one. The create save keeps its archive.
//!
//! - `tests/data/core`: neoforge/core's GameTest world, a fresh one, run from the S9 build of
//!   `feat/eq-s9-mowzie` (Mowzie dens and groves on the shared renewal rule, EQUILIBRIUM.md S9, FB-1 base 6, FB-2 no plunder
//!   mood, FB-3 the built hold, FB-4 the Umvuthi's pressure in the ledger row; docs/choices-mowzie-build.md MZB-1200 to
//!   MZB-1271; S9's formats on top of S7's: the conflict's pages are RTCF (magic `RTCF`) with meta version 14, a camp's ledger
//!   carrying the Umvuthi's own pressure after its recount stamp and the stats ending with the groves' arrivals; SITE_STATE
//!   (table 0x8006) is 64 bytes, the second pressure at 56, and the found tail 40 bytes, the second pressure at 36; the wheel's
//!   tags are unchanged); before that, the S7 build of
//!   `feat/eq-s7-conflict` at 7f7110eb (Conflict pressure and the outposts, EQUILIBRIUM.md S7, FB-1 (a), FB-5 (b),
//!   FB-8; docs/overnight-choices-equilibrium.md EQC200 to EQC290; S7's formats on top of S6's: the conflict's
//!   pages are RTCE (magic `RTCE`) with meta version 13, a faction row ending with its strain (a pool camp's
//!   pressure, whether it has broken and until when, the stamp and tick of its booked check) and its plunder mood
//!   (the raiders' grief and spoils), the wheel's events may be a strain check or a refound (tag 12), and the stats end with the
//!   breaks, refounds and deferrals; the site ledger's file is unchanged; this save has no news packet on a road at the
//!   stop, so `travel.messengers` is not required here, as the herd routes are not: it draws when a packet is in flight
//!   at the save, which depends on when the server stops); before that, the S6 build of
//!   `feat/eq-s6-villages` at db46b196 (Villages and Ribbits, EQUILIBRIUM.md S6, EQ-1 (a),
//!   merged with main c9111cc0, which carries S5; docs/overnight-choices-equilibrium.md EQC150 to
//!   EQC193; S6's formats on top of S5's: the world model's page is RTW5, a village's row after its
//!   market (the engine's count and K, the settlers' bookings and when they were made, whether it
//!   was seen alive, its kill pressure, the settlers' and the growth clocks' stamps), the wheel's
//!   events may be a settlers' or a breeding step and the resident ops are kept;
//!   phase 1's meta page RTP1 is version 6, the census and the resident answer joining its log
//!   as inputs 22 and 23 and the snapshot part losing its censused list; the dragons' pages stay
//!   RTD10 and RTN10); before that, the S5 build of
//!   `feat/eq-s5-dragon-bodies` (Dragon home bodies, EQUILIBRIUM.md S5, EQ-2 (a), on the base
//!   43062c0b, which is main with M12a; docs/overnight-choices-equilibrium.md EQC100 to EQC149;
//!   S5's formats: the dragons' pages are RTD10 (magic `RTDA`, four bytes) and their meta page
//!   RTN10 (magic `RTNA`), a dragon ending with the body it believes stands and its misses, a lair
//!   ending with a flags byte (the found tail's gap), the bodies it left behind, those missing and those expected
//!   after a reset, and the misses of the last, the meta page's stats gaining the eleven home-body counters, EQC135;
//!   the site ledger's lair rows are K 2, outside the pages and not copied here); before that,
//!   129de219
//!   (Mowzie's Mobs M12a, the site ledger and kill credit, EQUILIBRIUM.md S3, merged with M11 at
//!   13ac4d88 and main d53dbafd; S3's formats on top of M11's and U-e's: the conflict's pages are
//!   RTCD (magic `RTCD`) with meta version 12, a camp's slots ending with their UUIDs and its ledger
//!   with the bodies missing and left behind, a flags byte, the recount stamp and its due tick, the
//!   wheel's events may be a camp recount, MZB-1014, MZB-1015; the dragons' pages are RTD9 and
//!   their meta page RTN9, a lair ending with its `found_unknown` byte and a nest's flags with
//!   `seeded` and `found_unknown`, MZB-1012, MZB-1013; the engine's meta page RTJM stays at
//!   version 3 and phase 1's RTP1 at version 5; the site ledger itself is core's
//!   `data/mic_rtsim_sites.dat`, outside the pages and not copied here); before that, a9318ed7
//!   (Mowzie's Mobs M11 merged with main 60c759a9, which carries U-e with U-e13 and U-e14: both
//!   sides' formats at once, M11's conflict pages RTC12 and U-e's engine meta page RTJM version 3,
//!   as the two entries below say); before that, aec7918b
//!   (Mowzie's Mobs M11, the Umvuthi aloft, with M10 merged in, which carries main 8703d216; M11's
//!   formats: the conflict's pages are RTC12 (magic `RTCC`) with meta version 11, a camp's ledger
//!   ending with the end of the band its chief came home with and the resident state ending with
//!   those ends' index by agent, after the tribute stacks, MZB-853); before that, 1762b1e8
//!   (replay-gate slice U-e, the module switch as a journaled record, RP-290, merged with Mowzie's
//!   Mobs M10 36634dd2, which carries main c7d0f6e2: U-f, U-i, MZB-939, MZB-950; U-e's format: the
//!   engine's meta page RTJM is version 3, the mask the modules are in as of the pages' sequence
//!   appended after the outbox, docs/boundary.md 3.3, docs/choices-journal-barrier.md U-e5; the
//!   conflict's pages are M10's RTC11, magic `RTCB`, meta version 10; U-i changes no format);
//!   before that, 9d123ab5
//!   (Mowzie's Mobs M10, the march, merged with main 8703d216, which carries U-g, J6, U-h and
//!   MZB-760, plus the march GameTests' fixes, MZB-941; M10's formats: the conflict's pages are
//!   RTC11 (magic `RTCB`) with meta version 10, a party's row ending with its march revision, its
//!   due flag, its unseen tick and its unwatched epoch ranges, MZB-453, MZB-706);
//!   before that, da5aea80
//!   (Mowzie's Mobs M8, the lurk by the track, merged with main 589d5f82, which carries slice R;
//!   M8's formats: the conflict's pages are RTC10 (magic `RTCA`) with meta version 9, the stats
//!   ending with the lurk's five counters, MZB-690; the later merges of U-g, J6 and U-h change no
//!   format); before that, 8e3eda72
//!   (replay-gate slice R, the industry op window, merged with main ad04337a, which carries slices
//!   U-b, U-c and U-d; R's formats: the industry globals are IGB2, `decided` after `now`, and an
//!   industry district page starts with IDS5, its reconcile epochs per chunk, each pending
//!   reconcile's round and the packages written per postbox, RP-259); before that, abe7bad3
//!   (replay-gate slice U-d, the joint switch-on, on slice U-c, which carries slice U-b and main
//!   6be65f2c; U-d's formats: phase 1's meta page RTP1 is version 5, the inputs kept while phase 1
//!   was off following its queue, RP-300); before that, c1bd6472 (Mowzie's Mobs M9, the chiefs' heirs, trade memory, tribute and hunts, merged with main
//!   4e694452, which carries slice N; M9's formats: the conflict's pages are RTC9 with meta
//!   version 8, a camp's ledger going on with its heir, its chief's trade memory, the tribute it
//!   owes and is placing and the heir's pair in flight, a faction ending with its owed hunts, the
//!   wheel's events may be a camp heir, and the resident state ends with the tribute stacks being
//!   placed, MZB-437, MZB-448); before that, c934042f (replay-gate slice N, the nest renewal serial, on slice D and merged with main bc669502; N's
//!   formats: the dragons' pages are RTD8, each nest row ending with its renewal serial, and their
//!   meta page RTN8, RP-230 to RP-239); before that, 4f152abc
//!   (Mowzie's Mobs M7, the rail: 32726818, M7 merged with main a11311aa, which carries M6 and
//!   SEC-1's S6, then merged with main 588b229c, which carries SEC-1's S7 and the GameTest flake
//!   fixes and changes no format; M7's formats: the conflict's pages are RTC8 with meta version 7,
//!   a focus may keep its track, a source its pin, a party's target may be a track, a faction
//!   carries its rail meters, the wheel's events may be a rail wake and the resident state ends
//!   with the passes held for a line (MZB-302, MZB-344); before that, 1290d0b7 (Mowzie's Mobs M6,
//!   the Frostmaw, merged with main e75265e7, which carries slices C, H and S of the replay gate on
//!   SEC-1's S5-2; M6's formats: the conflict's pages are RTC7 with meta version 6, a camp's ledger
//!   ending with its renewal pressure and its booked arrival, the wheel's events may be an arrival
//!   (MZB-271), and a party ends with the op ids of the casualties it counted (MZB-589); before
//!   that, c67da61, SEC-1's slice S5-2 merged with main 095be6e; S5-2's formats: the glass pages
//!   IGL7, the regimes' and replays' network inputs and published rows, choice SC153; before that,
//!   b643465, SEC-1's slices S4c2 and S5-1 merged with main 10d2330, which carries the Mowzie's
//!   Mobs camp reconciler M4 and M5; M5's formats: the conflict's pages are RTC6 with meta version
//!   5, each faction row carrying its cap, its members away and its camp ledger, and the wheel's
//!   events may be a camp refill, MOWZIE.md section 4.9; S4c2's and S5-1's formats: the package
//!   stores' pages IST4, the drawn packages toward each frogport, the district each was drawn by
//!   and the drawn counters, choices SC141 to SC143, and the glass pages IGL6, each kinetic
//!   domain's sources and each unit's capacity per speed; before that, 8188cc0, S4c2 and S5-1 on
//!   main 428a331, and 37a34e40, M4 and M5 merged with main 428a331; S4c1's formats: the package
//!   stores' pages IST3, each store's frontier epoch, the packages it holds toward each frogport
//!   with their keys, and the held counters, choice SC137; before that, 02d7ea4, SEC-1's slice S4c1
//!   merged with main f361a6f; before that, edbb390, SEC-1's slice S4p merged with main a95bd2a;
//!   S4p's formats: the engine's meta page RTJM version 2, with the journal's newest acknowledged
//!   generation, and its closed generations as pages of table 2, docs/boundary.md §3.2, and the
//!   journal's `pinnedGen`, SC130; before that, 5d648ce, SEC-1's slice S4 merged with main 5489045;
//!   S4's formats: `IndustryData` format 3, the package stores' pages IST2 in table 0x4906, and
//!   RG14's `rtsim/bridge-life.txt`, the engine life; before that, 21e57d2, SEC-1's slice S3 on
//!   main 3391a5d; the formats of 0a5a4c2, Mowzie M3 merged with main 89fb716 and CL-D61, but for
//!   the glass page: the phase 1 world's catalogue has 10 goods, `gold_nugget` last; the booked
//!   train legs are pages of their own, table 0x4904, ILEG, and the industry globals page starts
//!   with IGB1 (docs/stress-worlds-results.md 10.10); the conflict's pages were RTC5 with meta
//!   version 4, each party's recalled stacks kept per epoch, each feud's kept notice with its
//!   serial and its Envoy hold, the Peaceful flag and start and the last notice serial, each
//!   faction's crystal, the destroyed loot and the repeats; a node may be a lair, tag 3; the ships'
//!   pages are RTS4, each port's blockade state and each laid-up slot's idle spells in a trailing
//!   section written only with closures on, and the world's pages are RTW4, each edge's closed
//!   flag; S8b-3's ships stats and ledger; the dragons' pages are RTD7 and RTN7, each lair's
//!   pressure, wait and fed share, and the nest table; the herds' pages are VERSION 4, each herd
//!   row its range's K, range, home and vacancy; an industry district's glass state is a page of
//!   table 0x4903 (IGL1 before S2), and the industry globals end with the glass counters and
//!   records received; since S3 the glass page starts with IGL5, its sections, the judging units'
//!   windows, the station joins, the station discards' count and each unit window's base at its
//!   counted stores (SECTIONS.md §4.2, §5); an industry district page starts with IDS4, the
//!   stations' ports and five threat classes). Villages, roads, news, herds, a district and its
//!   glass page; no caravan is on the road when the server stops (the harness world draws
//!   caravans). A herd's route draws only when a herd is on the move at the save, which depends on
//!   when the server stops: 6638edc's save had one, while both runs of 98d6d34 and main's 7e4042a
//!   had every herd staying put. So the route layer is not required here; it still has to draw or
//!   say why.
//! - `tests/data/create`: neoforge/create's GameTest world, run from the S9 build of
//!   `feat/eq-s9-mowzie` with `-Psec1fx` (the core save's formats, S9's: RTCF and meta 14, as the core entry says; a fresh
//!   world copied to `target/` after its JVM exited); before that, the S7 build of
//!   `feat/eq-s7-conflict` at 7f7110eb with `-Psec1fx` (the core save's formats, S7's: RTCE and meta 13, as the core
//!   entry says; a fresh world copied to `target/` after its JVM exited); before that, the S6 build of
//!   `feat/eq-s6-villages` at db46b196 (merged with main c9111cc0) with `-Psec1fx` (the core
//!   save's formats, S6's: RTW5 and RTP1 version 6, as the core entry says, and S5's under them; a
//!   fresh world copied to `target/` after its JVM exited); before that, the S5 build of
//!   `feat/eq-s5-dragon-bodies` with `-Psec1fx` (the core save's formats, S5's: RTD10 and RTN10,
//!   as the core entry says; a fresh world copied to `target/` after its JVM exited); before that,
//!   129de219 with `-Psec1fx`
//!   (M12a merged with M11 and main; the core save's formats, S3's, M11's and U-e's); before that,
//!   a9318ed7
//!   (M11 merged with main 60c759a9; the core save's formats, M11's and U-e's); before that, aec7918b
//!   (M11 with M10 merged in; the core save's formats, M11's); before that, 1762b1e8
//!   (U-e merged with M10 and main c7d0f6e2; the core save's formats, U-e's and M10's); before
//!   that, 49fef870 (M10 merged with main 8703d216; the core save's formats, M10's); before that, da5aea80
//!   (the core save's formats, M8's); before that, 2acae5ba (8e3eda72 merged
//!   with main 13051753, the grove tribute earmark, which changes no format: the core save's
//!   formats, R's); before that, 012f1fa2 (abe7bad3 merged
//!   with main 4b3c2343, which adds pages only: the core save's formats, U-d's); before that,
//!   c1bd6472 (M9's formats); before that, c934042f (slice N's formats); before that, 32726818 (the core save's
//!   formats before slice N and before its merge with main 588b229c; before that, 1290d0b7, M6's, and fe87a92, S5-2
//!   merged with main f66cb73, b643465, 37a34e40 as part of the plain `./gametest.sh core create
//!   mowziesmobs` round, 8188cc0, cee946f, S4c1, edbb390, 5d648ce, ec57cd7, 21e57d2 and S3's arm
//!   funnel-point fix) with the section fixtures (`-Psec1fx`), its `run-gametest/world` deleted
//!   first (delete `run-gametest/world` before a create run that follows a stress-generator run,
//!   whose clock is ahead of the game). Every module's pages, mostly empty: herds, districts with
//!   their glass pages and stations draw.
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
