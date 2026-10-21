// SPDX-License-Identifier: AGPL-3.0-or-later
//! The save viewer on saves the native link wrote (VIEWER.md §16, V1's exit test): rtsim-jni's fake
//! Java (`crates/rtsim-jni/tests/common`, shared as is) drives the real engine on the real host,
//! every module populated, then saves through the real persist path and closes. `load_world` reads
//! the folder back; every layer must draw or say why it is empty, every mark's card must answer,
//! viewing must change no state hash, and the folder must be left exactly as it was.
//!
//! Two worlds: one with every phase 1 and phase 2 module (villages, ports, a haven, a dragon's lair,
//! a herd, a goblin camp with a feud and its envoy out, HABITAT cells), and the industry engine's
//! iron line (districts, stations, lines, trains).
//!
//! The live source (V4) on the same worlds: a viewer's lease and the engine's tap flushed just
//! before the final save, so the view store and the save hold one state. Every layer must draw the
//! save viewer's marks in every frame, and every card must have the save card's sections (with the
//! live fields as a "Now" section first). A third world evicts rows, so units the tap met evicted
//! are drawn from the save.

#[path = "../../rtsim-jni/tests/common/mod.rs"]
mod common;
mod support;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use mic_viewer::api::{Card, EventsAnswer, Histogram, WorldInfo};
use mic_viewer::viewer::{Backend, EventQuery};
use rtsim_conflict::boundary::{self as cb, Cmd as C, Obs as CO};
use rtsim_ecology::boundary::{self as db, Obs as DO, SpawnKind};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::species::{Gender, Species};
use rtsim_industry::ids::Good;
use rtsim_industry::scenario::{IronLine, iron_line_world};
use rtsim_industry::tunables::Tunables;
use rtsim_jni::engine::{
    CONFLICT_TABLE, DRAGONS_TABLE, Engine, HERDS_TABLE, MODULE_CONFLICT, MODULE_ECOLOGY,
    MODULE_HERDS, MODULE_INDUSTRY, MODULE_SHIPS, PHASE1_TABLE, SHIPS_TABLE, WORLD_MODEL_REGIONS,
    WORLD_MODEL_TABLE,
};
use rtsim_jni::orders::Order;
use rtsim_jni::wire::{self, obs};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_ships::boundary::{self as sb, Obs as SO};
use rtsim_space::HabitatRecord;
use rtsim_space::synth::{Painter, land, sea};
use rtsim_view::industry::DistrictPeek;
use rtsim_view::live::{LiveApp, LiveView};
use rtsim_view::{Limits, Status, View, keys};
use support::*;

const PLAYER: (u64, u64) = (0x1111_2222_3333_4444, 0x5555_6666_7777_8888);
const VILLAGES: [(u64, &str, i32, i32); 3] = [
    (0x0001_0000_0000_0011, "minecraft:village_plains", 0, 0),
    (0x0001_0000_0000_0012, "minecraft:village_taiga", 400, 0),
    (0x0001_0000_0000_0013, "minecraft:village_desert", 0, 400),
];
/// A ribbit village big enough for a touring band, the human villages within its reach.
const RIBBITS: (u64, &str, i32, i32) = (0x0001_0000_0000_0014, "ribbits:swamp_village", 400, 400);
const HAVEN: u64 = 0x42;
const LAIR: u64 = 0x0000_0BB8_0000_0BB8;
const HOME: (i32, i32, i32) = (3000, 100, 3000);
const CAMP: u64 = 0xC0B1_0001;
const CAMP_AT: (i32, i32) = (900, 1000);
const BASE: (i32, i32) = (880, 900);

/// The viewer's mirror of the link's page keys is the link's (keys.rs cannot link rtsim-jni).
#[test]
fn the_mirrored_page_keys_are_the_links() {
    assert_eq!(keys::phase1::TABLE, PHASE1_TABLE);
    assert_eq!(keys::DRAGONS.table(), DRAGONS_TABLE);
    assert_eq!(keys::HERDS.table(), HERDS_TABLE);
    assert_eq!(keys::SHIPS.table(), SHIPS_TABLE);
    assert_eq!(keys::CONFLICT.table(), CONFLICT_TABLE);
    assert_eq!(keys::WORLD_MODEL.table(), WORLD_MODEL_TABLE);
    assert_eq!(keys::WORLD_MODEL_REGIONS, WORLD_MODEL_REGIONS);
    assert_eq!(keys::DICT_DIMENSION, wire::DICT_DIMENSION);
    assert_eq!(keys::DICT_NAME, DICT_NAME);
    assert_eq!(
        Limits::default().row_budget,
        rtsim_jni::engine::DEFAULT_ROW_BUDGET
    );
    use rtsim_jni::engine::mirrored as m;
    assert_eq!(keys::phase1::SNAPSHOT_TABLE, m::phase1::SNAPSHOT_TABLE);
    assert_eq!(keys::phase1::KEY_META, m::phase1::KEY_META);
    assert_eq!(keys::phase1::WORLD_META, m::phase1::WORLD_META);
    assert_eq!(keys::phase1::CHUNK, m::phase1::CHUNK);
    assert_eq!(keys::phase1::PIECE, m::phase1::PIECE);
    assert_eq!(keys::phase1::META_MAGIC, m::phase1::META_MAGIC);
    assert_eq!(keys::phase1::META_VERSION, m::phase1::META_VERSION);
    assert_eq!(keys::phase1::SNAP_MAGIC, m::phase1::SNAP_MAGIC);
    assert_eq!(keys::phase1::WORLD_SEED, m::phase1::WORLD_SEED);
    assert_eq!(keys::phase1::TABLE, m::phase1::TABLE);
    assert_eq!(keys::DRAGONS, m::KEY_DRAGONS);
    assert_eq!(keys::HERDS, m::KEY_HERDS);
    assert_eq!(keys::SHIPS, m::KEY_SHIPS);
    assert_eq!(keys::CONFLICT, m::KEY_CONFLICT);
    assert_eq!(keys::WORLD_MODEL, m::WORLD_MODEL_META);
    assert_eq!(keys::HASH_MAGIC, m::HASH_MAGIC);
    for k in [m::KEY_DRAGONS, m::KEY_HERDS, m::phase1::KEY_META] {
        assert_eq!(keys::hash_key(k), m::hash_key(k));
    }
}

/// Java's VILLAGE_ADOPTED for a village meeting at (x, z) (rtsim-jni's protocol tests' `adopt`).
fn adopt(j: &mut Java, village: u64, structure: &str, culture: &str, pieces: u32, x: i32, z: i32) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let st = j.dict_id(wire::DICT_STRUCTURE, structure);
    let cu = j.dict_id(wire::DICT_CULTURE, culture);
    use obs::village_adopted as k;
    j.journal(obs::VILLAGE_ADOPTED, k::SIZE, 0, |r| {
        wire::put_u64(r, k::VILLAGE, village);
        wire::put_u32(r, k::DIM, dim);
        wire::put_u32(r, k::STRUCTURE, st);
        wire::put_u32(r, k::CULTURE, cu);
        wire::put_u32(r, k::PIECES, pieces);
        wire::put_i32(r, k::MIN_X, x - 20);
        wire::put_i32(r, k::MIN_Y, 60);
        wire::put_i32(r, k::MIN_Z, z - 20);
        wire::put_i32(r, k::MAX_X, x + 20);
        wire::put_i32(r, k::MAX_Y, 80);
        wire::put_i32(r, k::MAX_Z, z + 20);
        wire::put_i32(r, k::MEET_X, x);
        wire::put_i32(r, k::MEET_Y, 64);
        wire::put_i32(r, k::MEET_Z, z);
    });
}

/// Region (0, 0): sea west of x = 128, plains east of it.
fn region(p: &mut Painter) -> Vec<HabitatRecord> {
    let mut out = Vec::new();
    for cz in 0..8 {
        for cx in 0..8 {
            out.push(p.record(
                0,
                cx,
                cz,
                &|x, _| {
                    if x < 128 {
                        sea()
                    } else {
                        land("minecraft:plains")
                    }
                },
                &[],
            ));
        }
    }
    out
}

/// Java's side of a world save: the journal SavedData, holding the dictionary the viewer names
/// dimensions and districts by.
fn write_journal(world: &Path, dict: &[(u32, String)]) {
    let path = world.join(rtsim_view::journal::FILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, rtsim_view::journal::write_dictionary(dict)).unwrap();
}

/// A viewer's lease (this process's), and the tap's whole pass committed: called just before the
/// final save, so the view store and the save hold the same state.
fn tap_flush(world: &Path, e: &Arc<Engine>) {
    rtsim_parts::store::Lease {
        pid: std::process::id(),
        focus: Vec::new(),
    }
    .write(&world.join("rtsim"))
    .unwrap();
    e.view_tap_flush().unwrap();
}

/// Every phase 1 and phase 2 module through the link, the tap flushed, saved and closed.
fn every_module_world(world: &Path) {
    let e = engine(&world.join("rtsim"), 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = 0b1111 | MODULE_ECOLOGY | MODULE_SHIPS | MODULE_HERDS | MODULE_CONFLICT;
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.idle();

    // Villages with sea ports, and a haven between them.
    for (v, structure, x, z) in VILLAGES {
        adopt(&mut j, v, structure, "human", 90, x, z);
    }
    let (v, structure, x, z) = RIBBITS;
    adopt(&mut j, v, structure, "ribbit", 90, x, z);
    j.idle();
    for (v, _, x, z) in VILLAGES {
        j.ship_obs(&SO::PortFound(sb::PortFound {
            village: v,
            medium: rtsim_travel::Medium::Sea,
            dimension: dim,
            structure: 0,
            dock: (x + 8, 63, z + 8),
            berths: 4,
        }));
    }
    j.ship_obs(&SO::HavenFound(sb::HavenFound {
        haven: HAVEN,
        media: 1,
        dimension: dim,
        structure: 0,
        dock: (200, 63, 200),
    }));
    j.idle();
    j.idle();

    // A fire dragon's roost and its dragon.
    j.dragon_obs(&DO::LairFound(db::LairFound {
        lair: LAIR,
        late: false,
        dimension: dim,
        species: Species::Fire,
        kind: LairKind::Roost,
        min: (HOME.0 - 30, 60, HOME.2 - 30),
        max: (HOME.0 + 30, 140, HOME.2 + 30),
        home: HOME,
    }));
    j.dragon_obs(&DO::Untagged(db::Untagged {
        entity: (0xD1, 0x1),
        spawn: SpawnKind::Structure,
        species: Species::Fire,
        dimension: dim,
        flags: db::UNTAGGED_HAS_HOME,
        age_ticks: 110 * 24_000,
        hunger: 60,
        gender: Gender::Female,
        variant: 1,
        home: HOME,
        lair: LAIR,
        pos: (f64::from(HOME.0), f64::from(HOME.1), f64::from(HOME.2)),
    }));
    j.idle();
    j.idle();
    // Its chunk unloads: the dragon is abstract again, hungry, at its roost.
    let adopted = j
        .iaf
        .iter()
        .find_map(|(_, c)| match c {
            db::Cmd::Adopt(a) => Some(*a),
            _ => None,
        })
        .expect("the dragon adopted");
    j.dragon_obs(&DO::Dematerialized(db::Dematerialized {
        agent: adopted.agent,
        epoch: adopted.epoch,
        reason: 2,
        dimension: dim,
        health: 400.0,
        max_health: 500.0,
        hunger: 10,
        age_ticks: 110 * 24_000,
        pos: (f64::from(HOME.0), f64::from(HOME.1), f64::from(HOME.2)),
    }));
    j.idle();

    // A flock by the roost (the dragon's prey), and HABITAT cells for the base map.
    e.order(vec![Order::HerdFound {
        name: "Viewdown flock".into(),
        species: "minecraft:sheep".into(),
        dimension: dim,
        x: HOME.0 - 200,
        z: HOME.2 - 200,
        count: 30,
        seed: 0,
    }])
    .unwrap();
    j.idle();
    let mut p = Painter::default();
    for r in region(&mut p) {
        while !j.habitat(&r, &|i| p.biome_name(i).to_string(), &|i| {
            p.body_name(i).to_string()
        }) {
            j.tick();
        }
    }
    j.idle();

    // Days on: trade, caravans, ships.
    for _ in 0..24 {
        e.order(vec![Order::Phase1Advance(3_000)]).unwrap();
        j.idle();
    }

    // A goblin camp; a player logs its forest (a feud); its envoy sets out.
    let base = [f64::from(BASE.0) + 0.5, 64.0, f64::from(BASE.1) + 0.5];
    j.players = vec![(PLAYER, dim, base)];
    j.conflict_plugin = rtsim_conflict::synth::FakeConflict::new(PLAYER);
    let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
    j.conflict_obs(&CO::FactionFound(cb::FactionFound {
        faction: CAMP,
        flags: 0,
        dimension: dim,
        structure,
        pos: (CAMP_AT.0, 64, CAMP_AT.1),
    }));
    j.idle();
    j.conflict_obs(&CO::Signal(cb::TerritorySignal {
        faction: 0,
        kind: 2,
        dimension: dim,
        count: 300,
        player: PLAYER,
        pos: (BASE.0, 64, BASE.1),
    }));
    j.idle();
    let end = j.game_time + 2 * TICKS_PER_DAY as i64;
    while !j.conflict.iter().any(|(_, c)| matches!(c, C::Envoy(_))) {
        assert!(j.game_time < end, "no envoy in two days");
        j.conflict_flush();
        j.idle();
        j.game_time += 600;
    }
    j.idle();

    // On until the dragon hunts, and save then.
    let hunting = |e: &Engine| {
        e.with_dragons(|d, _| {
            d.dragons
                .iter()
                .any(|(_, x)| rtsim_view::layers::hunt_target(&x.state).is_some())
        })
    };
    let end = j.game_time + 3 * TICKS_PER_DAY as i64;
    while !hunting(&e) {
        if j.game_time >= end {
            e.with_dragons(|d, w| {
                for (_, x) in d.dragons.iter() {
                    eprintln!(
                        "dragon {:?} fullness {} now {:?} game {}",
                        x.state,
                        x.fullness_at(&d.tun, w.now()),
                        w.now(),
                        j.game_time
                    );
                }
            });
        }
        assert!(j.game_time < end, "no hunt in three days");
        j.conflict_flush();
        j.idle();
        j.game_time += 200;
    }
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    tap_flush(world, &e);
    write_journal(world, &j.dict);
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The iron line (rtsim-industry's scenario) through the link for `days`, saved and closed: what
/// the engine itself says of each district just before the save (VIEWER.md §5.3: the card must
/// show the same). The scenario is set for ten days, so the player is home from day 1.5 and the
/// other three districts replay (the smelter and its power plant as one cluster, I13) when the
/// run stops before day 8.
fn iron_line(world: &Path, days: u64) -> BTreeMap<u64, DistrictPeek> {
    let cfg = IronLine {
        seed: 11,
        days: 10,
        threats: false,
        control: false,
        tick_first: true,
        approach: true,
    };
    let e: std::sync::Arc<Engine> = engine_with(&world.join("rtsim"), 256 * 1024, |c| {
        c.industry_seed = Some(cfg.seed)
    });
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    // Java's dictionary as rtsim-jni's iron line test fills it: goods 1 to 4, block states 11 to
    // 13 and 21 to 23.
    let mut w = iron_line_world(cfg, Tunables::default()).into_remote();
    for g in 1..=4u32 {
        let name = w.goods[&Good(g)].dictionary_name();
        assert_eq!(j.dict_id(DICT_GOOD, &name), g);
    }
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let mut next = dim + 1;
    for state in [11u32, 12, 13, 21, 22, 23] {
        while next < state {
            j.dict_id(0x7E, &format!("pad/{next}"));
            next += 1;
        }
        assert_eq!(
            j.dict_id(DICT_BLOCK_STATE, &format!("test:block_{state}")),
            state
        );
        next += 1;
    }
    w.move_to_dim(dim);
    w.declare();
    j.mc = Some(w);
    j.module_mask = MODULE_INDUSTRY;
    j.mc_run_until(Tick(days * TICKS_PER_DAY));
    let engine = e.with_industry(|ie, _| {
        ie.districts()
            .map(|d| {
                let peek = DistrictPeek {
                    inspection: ie.inspect_peek(d.id, ie.now()).expect("a district"),
                    stock: ie.stock_peek(d.id),
                    members: ie
                        .cluster_members(d.id)
                        .map(|m| m.into_iter().map(|m| m.0).collect())
                        .unwrap_or_default(),
                    processes: ie.processes_peek(d.id).unwrap_or_default(),
                };
                (d.id.0, peek)
            })
            .collect()
    });
    tap_flush(world, &e);
    write_journal(world, &j.dict);
    e.close(true, Duration::from_secs(5)).unwrap();
    engine
}

/// Opens `world`, checks what every save must satisfy, runs a whole session and returns the
/// layers that draw.
fn view_world(name: &str, world: &Path) -> (Vec<String>, Session) {
    let before = snapshot(world);
    let view = View::open(world, Limits::default()).expect("the save opens");
    {
        let s = view.saved();
        for m in &s.modules {
            assert!(
                !matches!(m.status, Status::Unreadable(_)),
                "{name}: {} did not read: {:?}",
                m.name,
                m.status
            );
            if let Some((now, Some(saved))) = m.hash {
                assert_eq!(
                    now, saved,
                    "{name}: {} reads back to its saved hash",
                    m.name
                );
            }
        }
        assert!(s.names.missing.is_none());
    }
    let drawing = every_layer_draws_or_says_why(&view);
    eprintln!("{name}: drawing {drawing:?}");
    for (id, why) in view.reasons() {
        eprintln!("{name}:   {id}: {why}");
    }
    let hashes = view.saved().state_hashes();
    let view = Arc::new(view);
    let s = session(Arc::clone(&view));
    eprintln!("{name}: {s:?}");
    assert_eq!(
        view.saved().state_hashes(),
        hashes,
        "{name}: viewing changed no state (VWD-11)"
    );
    drop(view);
    assert_same(&before, &snapshot(world));
    (drawing, s)
}

#[test]
fn every_module_saved_by_the_link_draws_and_viewing_writes_nothing() {
    let world = tmpdir("view-every");
    every_module_world(&world);
    let (drawing, s) = view_world("every module", &world);
    for l in [
        "settle.villages",
        "trade.prices",
        "travel.roads",
        "travel.caravans",
        "travel.bands",
        "travel.messengers",
        "ships.docks",
        "ships.lanes",
        "ships.ships",
        "ecology.lairs",
        "ecology.dragons",
        "ecology.hunts",
        "ecology.herds",
        "conflict.factions",
        "conflict.feuds",
        "conflict.parties",
    ] {
        assert!(drawing.iter().any(|d| d == l), "{l} draws");
    }
    assert!(s.tiles_drawn > 0, "the base map draws");
    let _ = std::fs::remove_dir_all(&world);
}

#[test]
fn the_iron_line_saved_by_the_link_draws_its_industry_layers() {
    let world = tmpdir("view-iron");
    let engine = iron_line(&world, 3);
    let (drawing, _) = view_world("iron line", &world);
    for l in ["industry.districts", "industry.stations", "industry.lines"] {
        assert!(drawing.iter().any(|d| d == l), "{l} draws");
    }
    // Every district's card decodes what the engine held: its inspection, replay stock, cluster
    // and processes from the saved pages equal the live engine's just before the save.
    let view = View::open(&world, Limits::default()).expect("the save opens");
    let saved = view.saved();
    let industry = saved.industry.as_ref().expect("industry pages");
    assert!(!engine.is_empty());
    // Only the industry module ran, so the phase 1 clock stayed at its start: the save's time is
    // the industry engine's.
    assert!(
        industry.now >= (2 * TICKS_PER_DAY) as i64,
        "{}",
        industry.now
    );
    assert_eq!(saved.now(), industry.now);
    for (id, want) in &engine {
        let got = industry
            .inspect(&*saved.source, *id)
            .unwrap_or_else(|e| panic!("district {id}: {e}"));
        eprintln!(
            "iron line: district {id} {:?}, {} processes, {} in its cluster",
            got.inspection.state,
            got.processes.len(),
            got.members.len()
        );
        assert_eq!(&got, want, "district {id}");
    }
    // The run stopped with the player home: the other three replay, the smelter and its power
    // plant in one cluster, each with its processes.
    let replaying: Vec<&DistrictPeek> = engine.values().filter(|p| p.stock.is_some()).collect();
    assert_eq!(replaying.len(), 3, "{engine:#?}");
    assert!(replaying.iter().all(|p| !p.processes.is_empty()));
    assert!(
        replaying.iter().any(|p| p.members.len() == 2),
        "{engine:#?}"
    );
    let _ = std::fs::remove_dir_all(&world);
}

// ---- the live source (V4) -----------------------------------------------------------------------

/// Marks as sorted JSON: the same marks, whatever order each source drew them in.
fn sorted(rows: Option<&mic_viewer::scene::LayerRows>) -> Vec<String> {
    let mut v: Vec<String> = rows
        .map(|r| r.marks())
        .unwrap_or(&[])
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect();
    v.sort();
    v
}

/// What the live source's comparison with the save saw.
#[derive(Debug, Default)]
struct LiveSeen {
    marks: usize,
    cards: usize,
    /// Cards with a "Now" section (the live row's fields).
    now: usize,
    /// Section titles over every card.
    sections: BTreeMap<String, usize>,
}

/// The live source over `world` (the tap flushed at the save's state) against the save viewer:
/// every layer draws the same marks in every frame, every card the same sections, the timeline the
/// same rows; every route answers; and opening it without a lease writer writes nothing.
fn live_matches_save(name: &str, world: &Path) -> LiveSeen {
    let before = snapshot(world);
    let save = Arc::new(View::open(world, Limits::default()).expect("the save opens"));
    let live = LiveView::open(world, Limits::default(), None, None).expect("the live view opens");
    let h = live.header().expect("the tap's header");
    assert!(h.cycles >= 1, "{name}: a whole pass");
    assert_eq!(h.game_time, save.saved().now(), "{name}: one state");
    let w: WorldInfo = live.world();
    assert_eq!(w.source, "live");
    assert!(w.stream);
    assert_eq!(w.time.tick, h.game_time);
    let layers = save.layers();
    let mut seen = LiveSeen::default();
    let frames: Vec<String> = save.frames().map(str::to_owned).collect();
    assert!(!frames.is_empty());
    for f in &frames {
        let a = save.scene(f, None).expect("a save scene");
        let b = live.scene(f, None).expect("a live scene");
        assert_eq!(b.t, h.game_time);
        for d in &layers.layers {
            let (ra, rb) = (a.layers.get(&d.id), b.layers.get(&d.id));
            assert!(rb.is_some(), "{name}: {} has rows in {f}", d.id);
            assert_eq!(
                sorted(ra),
                sorted(rb),
                "{name}: layer {} in frame {f} draws the save's marks",
                d.id
            );
            if ra.is_some_and(|r| r.is_empty()) {
                assert!(
                    rb.is_some_and(|r| r.is_empty()),
                    "{name}: {} is empty live too",
                    d.id
                );
            }
            let Some(ra) = ra else { continue };
            seen.marks += ra.len();
            if !d.has_cards {
                continue;
            }
            for m in ra.marks() {
                let Some(id) = m.id() else { continue };
                let cs: Card = save.card(&d.id, id, None).expect("a save card");
                let cl: Card = live.card(&d.id, id, None).expect("a live card");
                seen.cards += 1;
                assert_eq!(cl.title, cs.title, "{name}: {} {id}", d.id);
                assert_eq!(cl.subtitle, cs.subtitle, "{name}: {} {id}", d.id);
                let json = |x: &[mic_viewer::api::Field]| serde_json::to_value(x).unwrap();
                assert_eq!(json(&cl.fields), json(&cs.fields), "{name}: {} {id}", d.id);
                let mut sl = cl.sections.clone();
                if sl.first().is_some_and(|s| s.title == "Now") {
                    let now = sl.remove(0);
                    assert!(!now.fields.is_empty(), "{name}: {} {id}", d.id);
                    seen.now += 1;
                }
                assert_eq!(
                    serde_json::to_value(&sl).unwrap(),
                    serde_json::to_value(&cs.sections).unwrap(),
                    "{name}: the card of {} {id} has the save card's sections",
                    d.id
                );
                for s in &cs.sections {
                    *seen.sections.entry(s.title.clone()).or_default() += 1;
                }
            }
        }
        // The timeline: nothing pending is newer than the save, so the save's own answers.
        let q = EventQuery {
            frame: Some(f.clone()),
            limit: 500,
            ..EventQuery::default()
        };
        let (ea, eb): (EventsAnswer, EventsAnswer) = (save.events(&q), live.events(&q));
        assert_eq!(
            serde_json::to_value(&ea).unwrap(),
            serde_json::to_value(&eb).unwrap(),
            "{name}: events in {f}"
        );
        let q = EventQuery {
            frame: Some(f.clone()),
            limit: 120,
            to: h.game_time + 1,
            ..EventQuery::default()
        };
        let (ha, hb): (Histogram, Histogram) = (save.histogram(&q), live.histogram(&q));
        assert_eq!(ha.series, hb.series, "{name}: the histogram in {f}");
    }
    // Every route, as a page asks them, through the live handler; the base map is the save's.
    let app = LiveApp::new(Arc::clone(&live));
    let s = session_on(&app, &*live, &sampled_regions(&save));
    eprintln!("{name} live: {s:?}; {seen:?}");
    let st: serde_json::Value = get_json(&app, "/api/status", "");
    assert_eq!(st["source"], "live");
    assert!(st["live"]["error"].is_null(), "{}", st["live"]["error"]);
    eprintln!("{name} live layers: {}", st["live"]["layers"]);
    drop(app);
    drop(live);
    assert_same(&before, &snapshot(world));
    seen
}

#[test]
fn the_live_source_draws_every_modules_save_and_its_cards() {
    let world = tmpdir("view-live-every");
    every_module_world(&world);
    let seen = live_matches_save("every module", &world);
    assert!(seen.marks > 20, "{seen:?}");
    assert!(seen.cards > 10, "{seen:?}");
    assert!(seen.now > 0, "live fields reach the cards: {seen:?}");
    let _ = std::fs::remove_dir_all(&world);
}

#[test]
fn the_live_source_draws_the_iron_lines_districts_with_their_processes_and_clusters() {
    let world = tmpdir("view-live-iron");
    iron_line(&world, 3);
    let seen = live_matches_save("iron line", &world);
    for s in ["Processes", "Replays with (one cluster)"] {
        assert!(
            seen.sections.get(s).is_some_and(|n| *n > 0),
            "live district cards have \"{s}\": {seen:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&world);
}

/// Rows evicted under a tiny row budget: the tap publishes the units that met them as not
/// resident, and the live source draws those from the save, the same marks the save viewer draws.
/// A fresh world keeps its rows in memory; ships reopened from their pages read them lazily.
#[test]
fn units_the_tap_met_evicted_are_drawn_from_the_save() {
    let world = tmpdir("view-live-evicted");
    let mask = 0b1111 | MODULE_SHIPS;
    let e = engine(&world.join("rtsim"), 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = mask;
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    for (v, structure, x, z) in VILLAGES {
        adopt(&mut j, v, structure, "human", 90, x, z);
    }
    j.idle();
    for (v, _, x, z) in VILLAGES {
        j.ship_obs(&SO::PortFound(sb::PortFound {
            village: v,
            medium: rtsim_travel::Medium::Sea,
            dimension: dim,
            structure: 0,
            dock: (x + 8, 63, z + 8),
            berths: 4,
        }));
    }
    j.idle();
    j.idle();
    let ws = j.world_save();
    e.close(true, Duration::from_secs(5)).unwrap();
    // Reopened with a budget of one byte: the start's restore check reads every row, and the
    // steps' budget checks evict the ones nothing changed.
    let e = engine_with(&world.join("rtsim"), 256 * 1024, |c| c.row_budget = 1);
    let mut j = Java::open(e.clone(), ws);
    j.module_mask = mask;
    j.idle();
    j.idle();
    tap_flush(&world, &e);
    let evicted = e
        .stats
        .tap
        .not_resident
        .load(std::sync::atomic::Ordering::Acquire);
    write_journal(&world, &j.dict);
    e.close(true, Duration::from_secs(5)).unwrap();
    assert!(evicted > 0, "the tap met evicted rows");
    let seen = live_matches_save("evicted", &world);
    assert!(seen.marks > 0, "{seen:?}");
    let live = LiveView::open(&world, Limits::default(), None, None).unwrap();
    let from_save: usize = live.layer_sources().values().map(|(_, _, n)| n).sum();
    eprintln!(
        "evicted: {evicted} units not resident; {:?}",
        live.layer_sources()
    );
    assert!(from_save > 0, "units drawn from the save");
    let _ = std::fs::remove_dir_all(&world);
}
