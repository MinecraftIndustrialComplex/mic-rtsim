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

#[path = "../../rtsim-jni/tests/common/mod.rs"]
mod common;
mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::*;
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

/// Every phase 1 and phase 2 module through the link, saved and closed.
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
    write_journal(world, &j.dict);
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// The iron line (rtsim-industry's scenario) through the link for `days`, saved and closed.
fn iron_line(world: &Path, days: u64) {
    let cfg = IronLine {
        seed: 11,
        days,
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
    write_journal(world, &j.dict);
    e.close(true, Duration::from_secs(5)).unwrap();
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
    iron_line(&world, 3);
    let (drawing, _) = view_world("iron line", &world);
    for l in ["industry.districts", "industry.stations", "industry.lines"] {
        assert!(drawing.iter().any(|d| d == l), "{l} draws");
    }
    let _ = std::fs::remove_dir_all(&world);
}
