// SPDX-License-Identifier: AGPL-3.0-or-later
//! The headless host's exit test (VIEWER.md §12, §16 row V2): a world's pages open in the host
//! with the state hash they were saved with; long strides against 20-tick hand-overs; exports from
//! a world whose server is saving underneath; a world the host never writes into; and a soak of a
//! thousand game days at max, with the viewer attached and a page reading it, with Java's journal,
//! the commands in flight and the kept outboxes bounded.
//!
//! The populated world is made here by the fake Java side, as a server with a player about would
//! leave it: villages with roads and caravans, sea ports and a haven, a dragon's lair with its
//! dragon, a goblin camp. GameTest worlds come from `RTSIM_HOST_WORLDS` (paths, `:` between them)
//! or this checkout's `neoforge/*/run-gametest/world`.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use mic_viewer::http::{Config, Server};
use mic_viewer::viewer::App;

use rtsim_conflict::boundary as cb;
use rtsim_ecology::boundary::{self as db, SpawnKind};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::species::{Gender, Species};
use rtsim_host::fork;
use rtsim_host::java::{self, Java, OVERWORLD, PLAYER, WorldSave};
use rtsim_host::nbt::{self, Tag};
use rtsim_host::run::{
    ARCHIVE_SAVE_AT, Control, DAY, Host, Options, STRIDE_FAST, Speed, Until, state_hashes,
};
use rtsim_host::save::{self, Journal};
use rtsim_host::view::HostView;
use rtsim_jni::engine::Engine;
use rtsim_jni::wire;
use rtsim_kernel::page::{Durability, PageStore};
use rtsim_ships::boundary as sb;
use rtsim_view::Limits;

// ---- the populated world ------------------------------------------------------------------------

const STRUCTURES: [&str; 5] = [
    "minecraft:village_plains",
    "minecraft:village_taiga",
    "minecraft:village_desert",
    "minecraft:village_savanna",
    "minecraft:village_snowy",
];
const LAIR: u64 = 0x0000_0BB8_0000_0BB8;
const LAIR_HOME: (i32, i32, i32) = (1600, 100, 1600);
const CAMP: u64 = 0xCA_0001;

fn scratch(tag: &str) -> PathBuf {
    let d = java::tmpdir(&format!("host-{tag}"));
    fs::create_dir_all(&d).unwrap();
    d
}

/// Nine villages on a grid 500 blocks apart, six of them sea ports, a haven between them, a fire
/// dragon's roost with its dragon, and a goblin camp.
fn populate(j: &mut Java) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    for i in 0..9u64 {
        let (x, z) = ((i % 3) as i32 * 500, (i / 3) as i32 * 500);
        j.adopt(
            0x0001_0000_0000_0100 + i,
            STRUCTURES[i as usize % STRUCTURES.len()],
            "human",
            90,
            x,
            z,
        );
    }
    j.idle();
    for i in 0..6u64 {
        let (x, z) = ((i % 3) as i32 * 500, (i / 3) as i32 * 500);
        j.ship_obs(&sb::Obs::PortFound(sb::PortFound {
            village: 0x0001_0000_0000_0100 + i,
            medium: rtsim_travel::Medium::Sea,
            dimension: dim,
            structure: 0,
            dock: (x + 8, 63, z + 8),
            berths: 4,
        }));
    }
    j.ship_obs(&sb::Obs::HavenFound(sb::HavenFound {
        haven: 0x42,
        media: 1,
        dimension: dim,
        structure: 0,
        dock: (250, 63, 250),
    }));
    j.dragon_obs(&db::Obs::LairFound(db::LairFound {
        lair: LAIR,
        late: false,
        dimension: dim,
        species: Species::Fire,
        kind: LairKind::Roost,
        min: (LAIR_HOME.0 - 30, 60, LAIR_HOME.2 - 30),
        max: (LAIR_HOME.0 + 30, 140, LAIR_HOME.2 + 30),
        home: LAIR_HOME,
    }));
    j.dragon_obs(&db::Obs::Untagged(db::Untagged {
        entity: (0xD1, 0x1),
        spawn: SpawnKind::Structure,
        species: Species::Fire,
        dimension: dim,
        flags: db::UNTAGGED_HAS_HOME,
        age_ticks: 110 * 24_000,
        hunger: 60,
        gender: Gender::Female,
        variant: 1,
        home: LAIR_HOME,
        lair: LAIR,
        pos: (
            f64::from(LAIR_HOME.0),
            f64::from(LAIR_HOME.1),
            f64::from(LAIR_HOME.2),
        ),
    }));
    let structure = j.dict_id(wire::DICT_STRUCTURE, "goblinstyranny:goblin_camp");
    j.conflict_obs(&cb::Obs::FactionFound(cb::FactionFound {
        faction: CAMP,
        flags: 0,
        dimension: dim,
        structure,
        pos: (750, 64, 250),
    }));
    j.idle();
    j.idle();
}

/// Game time on by `ticks` in hand-overs of `quantum`, the fake Java answering as a server would.
fn play(j: &mut Java, ticks: i64, quantum: i64) {
    let end = j.game_time + ticks;
    while j.game_time < end {
        j.game_time += quantum - 2;
        j.idle();
    }
}

/// The bridge's save as a server makes it: the engine saves through `nextObsSeq - 1`, the
/// journal and `level.dat` written beside the pages.
fn save_world(world: &Path, j: &mut Java) {
    let through = j.next_obs - 1;
    j.e.request_save(through);
    j.run_until("the world's save", |j| j.e.durable_obs_seq() >= through);
    Journal {
        save: j.world_save(),
        data_version: Some(3955),
    }
    .write(world)
    .unwrap();
    write_level(world, j.game_time);
}

/// The bridge's save in the bridge's order (and `Host::save`'s): the journal and `level.dat`
/// written now, the engine's pages at its next step. Returns the observation saved through.
fn save_as_the_bridge(world: &Path, j: &mut Java) -> u64 {
    let through = j.next_obs - 1;
    j.e.request_save(through);
    Journal {
        save: j.world_save(),
        data_version: Some(3955),
    }
    .write(world)
    .unwrap();
    write_level(world, j.game_time);
    through
}

fn write_level(world: &Path, time: i64) {
    let root = Tag::Compound(vec![(
        b"Data".to_vec(),
        Tag::Compound(vec![
            (b"Time".to_vec(), Tag::Long(time)),
            (b"DayTime".to_vec(), Tag::Long(time)),
            (b"LevelName".to_vec(), Tag::string("host-test")),
        ]),
    )]);
    save::write_atomic(&world.join(save::LEVEL), &nbt::write_gz(b"", &root)).unwrap();
}

/// A world a server left: the fake Java with a player in the first village for a day, then
/// nobody for `days - 1` more, saved, and the engine stopped as a server stops.
fn populated(tag: &str, days: i64) -> PathBuf {
    let world = scratch(tag);
    let e = java::engine(&world.join(save::PAGES), 1 << 20);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = save::ALL_MODULES;
    populate(&mut j);
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    j.players = vec![(PLAYER, dim, [10.0, 64.0, 0.0])];
    play(&mut j, DAY, 1_000);
    j.players.clear();
    play(&mut j, (days - 1).max(0) * DAY, 3_000);
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    save_world(&world, &mut j);
    j.crash();
    world
}

// ---- checks -------------------------------------------------------------------------------------

/// What the last start read back different from what its save had: empty when every module read
/// its pages to the state hash they were saved with and phase 1 replayed its log to its own.
fn restore_failures(e: &Engine) -> Vec<String> {
    let mut bad = Vec::new();
    let (_, replay_ok, mismatches) = e.phase1_state();
    if !replay_ok {
        bad.push("phase 1 replayed its log to another state".to_string());
    }
    if mismatches != 0 {
        bad.push(format!("phase 1: {mismatches} capture mismatches"));
    }
    for (name, (diverged, dropped)) in [
        ("dragons", e.dragons_restore_check()),
        ("ships", e.ships_restore_check()),
        ("herds", e.herds_restore_check()),
        ("conflict", e.conflict_restore_check()),
        ("world model", e.world_model_restore_check()),
    ] {
        if let Some((saved, read)) = diverged {
            bad.push(format!("{name}: saved {saved:016x}, read {read:016x}"));
        }
        if dropped {
            bad.push(format!("{name}: pages dropped"));
        }
    }
    bad
}

/// The state hash pages (TEST saves) in a page directory's confirmed generation: the restore
/// checks compare against these, so none means the check proved nothing.
/// The tables that hold one, in hex.
fn hash_pages(pages: &Path) -> Vec<String> {
    let mut s = PageStore::open_read_only(pages).unwrap();
    s.view_confirmed().unwrap();
    s.try_keys()
        .unwrap()
        .into_iter()
        .filter(|k| {
            s.get_shared(*k)
                .unwrap()
                .is_some_and(|b| b.len() == 12 && b[..4] == *b"RTSH")
        })
        .map(|k| format!("{:#x}", k.table()))
        .collect()
}

/// Bytes and pages by table.
type Tables = BTreeMap<u16, (u64, u64)>;

/// Bytes and pages by table in a page directory's confirmed generation.
fn table_bytes(pages: &Path) -> Tables {
    let mut s = PageStore::open_read_only(pages).unwrap();
    s.view_confirmed().unwrap();
    let mut out = BTreeMap::new();
    for k in s.try_keys().unwrap() {
        let n = s.get_shared(k).unwrap().map_or(0, |b| b.len() as u64);
        let e: &mut (u64, u64) = out.entry(k.table()).or_default();
        e.0 += n;
        e.1 += 1;
    }
    out
}

fn differing(a: &[(&'static str, u64)], b: &[(&'static str, u64)]) -> Vec<&'static str> {
    a.iter()
        .zip(b)
        .filter(|(x, y)| x.1 != y.1)
        .map(|(x, _)| x.0)
        .collect()
}

fn opts(test: bool) -> Options {
    Options {
        speed: Speed::Max,
        test,
        durability: Durability::NoSync,
        ..Options::default()
    }
}

/// Runs the host on `dir` for `days` game days from its start; the hashes after the last step.
fn run_days(dir: &Path, o: &Options, days: i64) -> (Vec<(&'static str, u64)>, Host) {
    let mut host = Host::open(dir, o).unwrap();
    let o = Options {
        until: Some(Until::After(days * DAY)),
        ..o.clone()
    };
    host.run(&o, &Control::new(), Duration::from_secs(3600), |_| {})
        .unwrap();
    assert!(
        host.e.protocol_error().is_none(),
        "{:?}",
        host.e.protocol_error()
    );
    (state_hashes(&host.e), host)
}

/// Every file under `dir` with its bytes and modification time.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.clone(), (fs::read(&p).unwrap(), m.modified().unwrap()));
            }
        }
    }
    out
}

// ---- the tests ----------------------------------------------------------------------------------

/// A world's pages open in the host with the state hash they were saved with: every module's
/// restore check passes against the hash pages its save wrote (and there are some), phase 1
/// replays its log to its own hash, and a session closed and opened again reads back the very
/// state it closed with.
#[test]
fn a_populated_world_opens_in_the_host_with_its_saved_state() {
    let world = populated("same-hash", 3);
    let dir = scratch("same-hash-fork");
    let dir = dir.join("fork");
    let info = fork::export(&world, &dir).unwrap();
    assert_eq!(info.attempts, 1);
    assert!(info.pages > 0);
    let hp = hash_pages(&dir.join(save::PAGES));
    eprintln!("state hash pages in tables {hp:?}");
    assert!(
        hp.len() >= 4,
        "the TEST save wrote state hash pages: {hp:?}"
    );

    let host = Host::open(&dir, &opts(true)).unwrap();
    assert_eq!(restore_failures(&host.e), Vec::<String>::new());
    let opened = state_hashes(&host.e);
    let (now, edges, flow, dragons) = host.e.with_dragons(|d, w| {
        let flow: i64 = w
            .edges
            .iter()
            .flat_map(|e| e.dirs.iter())
            .map(|d| d.goods.values().map(|p| p.current_rate()).sum::<i64>())
            .sum();
        (
            w.now().0,
            w.edges.len(),
            flow,
            (d.dragons.len(), d.lairs.len()),
        )
    });
    eprintln!(
        "populated world at world tick {now}: {edges} roads and lanes carrying {flow} thousandths a quantum, dragons and lairs {dragons:?}; {opened:x?}"
    );
    assert!(flow > 0, "goods on the roads");
    assert_eq!(dragons, (1, 1));
    host.close().unwrap();

    // Ten game days in the host, closed with its final save, opened again.
    let (ran, host) = run_days(&dir, &opts(true), 10);
    assert_eq!(restore_failures(&host.e), Vec::<String>::new());
    assert!(host.saves >= 1);
    host.close().unwrap();
    let again = Host::open(&dir, &opts(true)).unwrap();
    assert_eq!(restore_failures(&again.e), Vec::<String>::new());
    assert_eq!(
        differing(&ran, &state_hashes(&again.e)),
        Vec::<&str>::new(),
        "the next session reads the state the last one closed with"
    );
    again.close().unwrap();
    let _ = fs::remove_dir_all(&world);
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

/// GameTest worlds (made by real servers with the mods in) open in the host with the state hash
/// their saves wrote, and three game days there come out the same at 20 ticks a hand-over and at
/// the host's fast stride. Always the core and create GameTest saves rtsim-view keeps as fixtures,
/// plus any GameTest world in this checkout (or `RTSIM_HOST_WORLDS` instead of both).
#[test]
fn gametest_worlds_open_in_the_host_with_their_saved_state() {
    let worlds: Vec<PathBuf> = match std::env::var("RTSIM_HOST_WORLDS") {
        Ok(v) if !v.is_empty() => v.split(':').map(PathBuf::from).collect(),
        _ => {
            let here = Path::new(env!("CARGO_MANIFEST_DIR"));
            let fixtures = ["core", "create"]
                .map(|w| here.join("../rtsim-view/tests/data").join(w))
                .into_iter();
            let root = here.join("../../neoforge");
            let checkout = fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path().join("run-gametest/world"));
            // A world the stub engine ran last has an rtsim folder with only stub-pages.bin: the
            // native store's manifest is what makes it the engine's.
            fixtures
                .chain(checkout)
                .filter(|w| w.join(save::PAGES).join("manifest.rtm").is_file())
                .collect()
        }
    };
    if worlds.is_empty() {
        assert!(
            std::env::var_os("RTSIM_HOST_REQUIRE_WORLDS").is_none(),
            "RTSIM_HOST_REQUIRE_WORLDS: no GameTest world"
        );
        eprintln!(
            "SKIPPED: no GameTest world (set RTSIM_HOST_WORLDS, or run the GameTests in this checkout)"
        );
        return;
    }
    for world in worlds {
        let dir = scratch("gametest").join("fork");
        let info = fork::export(&world, &dir)
            .unwrap_or_else(|e| panic!("export {}: {e}", world.display()));
        let hp = hash_pages(&dir.join(save::PAGES));
        let host = Host::open(&dir, &opts(true))
            .unwrap_or_else(|e| panic!("open {}: {e}", world.display()));
        let failures = restore_failures(&host.e);
        let hashes = state_hashes(&host.e);
        eprintln!(
            "{}: {} pages, generation {} through obs {}, state hash pages in tables {hp:?}, modules {:#x}; restore {}; {hashes:x?}",
            world.display(),
            info.pages,
            info.pages_save_seq,
            info.pages_obs_seq,
            host.mask,
            if failures.is_empty() {
                "same".to_string()
            } else {
                failures.join("; ")
            }
        );
        assert!(failures.is_empty(), "{}: {failures:?}", world.display());
        assert!(!hp.is_empty(), "{}: no state hash pages", world.display());
        host.close().unwrap();
        // Three game days at 20 ticks, at the host's fast stride and at two days a step, each
        // on its own copy of the fork.
        let mut runs = Vec::new();
        for (i, s) in [20, STRIDE_FAST, 48_000].into_iter().enumerate() {
            let copy = dir.parent().unwrap().join(format!("s{i}"));
            fork::export(&dir, &copy).unwrap();
            let o = Options {
                stride: Some(s),
                ..opts(false)
            };
            let (h, host) = run_days(&copy, &o, 3);
            host.close().unwrap();
            runs.push((s, h));
        }
        for (s, h) in &runs[1..] {
            let d = differing(&runs[0].1, h);
            eprintln!(
                "  stride {s} against 20 over 3 game days: {}",
                if d.is_empty() {
                    "the same".to_string()
                } else {
                    format!("differs in {d:?}")
                }
            );
        }
        assert_eq!(
            differing(&runs[0].1, &runs[1].1),
            Vec::<&str>::new(),
            "{}: the fast stride agrees with 20 ticks",
            world.display()
        );
        // With a season a game day (`--season-days 1`), three days cross two seasons: the season
        // reaches industry at the first hand-over after it turns, up to a stride late, and the
        // fast stride must still agree.
        let mut seasons = Vec::new();
        for (i, s) in [20, STRIDE_FAST].into_iter().enumerate() {
            let copy = dir.parent().unwrap().join(format!("season{i}"));
            fork::export(&dir, &copy).unwrap();
            let o = Options {
                stride: Some(s),
                season_days: Some(1),
                ..opts(false)
            };
            let (h, host) = run_days(&copy, &o, 3);
            host.close().unwrap();
            seasons.push(h);
        }
        eprintln!(
            "  with a season a day, against none: {:?} differ",
            differing(&runs[0].1, &seasons[0])
        );
        assert_eq!(
            differing(&seasons[0], &seasons[1]),
            Vec::<&str>::new(),
            "{}: with seasons turning, the fast stride agrees with 20 ticks",
            world.display()
        );
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
}

/// Hand-overs 20 ticks apart against longer strides, on copies of one fork: the module state
/// hashes after the same game time. Two 20-tick runs agree (the comparison means something), and
/// so does the stride the host uses at 10x and max ([`STRIDE_FAST`]).
#[test]
fn strides_agree() {
    let days: i64 = std::env::var("RTSIM_HOST_STRIDE_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    let world = populated("strides", 3);
    let base = scratch("strides-forks");
    fork::export(&world, &base.join("base")).unwrap();
    let mut strides = vec![
        20, 20, 40, 100, 200, 400, 1_200, 2_400, 6_000, 12_000, 24_000, 48_000,
    ];
    if !strides.contains(&STRIDE_FAST) {
        strides.push(STRIDE_FAST);
    }
    let mut rows = Vec::new();
    for (i, s) in strides.iter().enumerate() {
        let dir = base.join(format!("s{i}-{s}"));
        fork::export(&base.join("base"), &dir).unwrap();
        let o = Options {
            stride: Some(*s),
            ..opts(false)
        };
        let t0 = Instant::now();
        let (h, host) = run_days(&dir, &o, days);
        let wall = t0.elapsed();
        let steps = host.steps;
        host.close().unwrap();
        rows.push((*s, h, steps, wall));
    }
    let reference = rows[0].1.clone();
    eprintln!("stride  steps     wall      differs from 20 ticks ({days} game days)");
    for (s, h, steps, wall) in &rows {
        let d = differing(&reference, h);
        eprintln!(
            "{s:>6}  {steps:>8}  {:>7.2} s  {}",
            wall.as_secs_f64(),
            if d.is_empty() {
                "none".to_string()
            } else {
                d.join(", ")
            }
        );
    }
    assert_eq!(
        differing(&reference, &rows[1].1),
        Vec::<&str>::new(),
        "two 20-tick runs agree"
    );
    let fast = rows.iter().find(|r| r.0 == STRIDE_FAST).unwrap();
    assert_eq!(
        differing(&reference, &fast.1),
        Vec::<&str>::new(),
        "the host's fast stride agrees with 20 ticks"
    );
    let _ = fs::remove_dir_all(&world);
    let _ = fs::remove_dir_all(&base);
}

/// Exports from a world whose server keeps running and saving underneath: each fork holds one
/// whole committed generation and a journal that continues it, so it opens with its saved state
/// and runs.
#[test]
fn exports_from_a_live_world_open_clean() {
    let world = populated("live", 2);
    let stop = Arc::new(AtomicBool::new(false));
    let server = {
        let (world, stop) = (world.clone(), stop.clone());
        std::thread::spawn(move || {
            let e = java::engine(&world.join(save::PAGES), 1 << 20);
            let mut j = Java::open(e, Journal::read(&world).unwrap().save);
            j.module_mask = save::ALL_MODULES;
            let mut saves = 0;
            while !stop.load(Ordering::Relaxed) {
                play(&mut j, 4 * 3_000, 3_000);
                save_as_the_bridge(&world, &mut j);
                saves += 1;
            }
            j.crash();
            saves
        })
    };
    let mut retried = 0;
    let forks = 12;
    for k in 0..forks {
        let dir = scratch(&format!("live-{k}")).join("fork");
        let info = fork::export(&world, &dir).unwrap();
        retried += info.attempts - 1;
        // The pair copied: the journal starts no later than the observation after the pages.
        assert!(
            info.journal_first_obs <= info.pages_obs_seq + 1,
            "fork {k}: {info:?}"
        );
        assert!(
            info.notes.iter().all(|n| !n.contains("in neither")),
            "fork {k}: {:?}",
            info.notes
        );
        let host = Host::open(&dir, &opts(true)).unwrap();
        assert_eq!(restore_failures(&host.e), Vec::<String>::new(), "fork {k}");
        host.close().unwrap();
        let (_, host) = run_days(&dir, &opts(true), 1);
        host.close().unwrap();
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
    stop.store(true, Ordering::Relaxed);
    let saves = server.join().unwrap();
    eprintln!("{forks} exports while the server saved {saves} times: {retried} copies again");
    assert!(saves >= 2, "the server saved while the exports ran");
    let _ = fs::remove_dir_all(&world);
}

/// A save landing between the copy of the pages and the second read of the journal (where a live
/// server's save falls by chance in the test above) makes the export copy again, and the fork
/// holds the pair the second copy read: the world's journal as it is now and the pages it
/// confirms.
#[test]
fn a_save_during_the_copy_makes_the_export_copy_again() {
    let world = populated("race", 2);
    let e = java::engine(&world.join(save::PAGES), 1 << 20);
    let mut j = Java::open(e, Journal::read(&world).unwrap().save);
    j.module_mask = save::ALL_MODULES;
    play(&mut j, 4 * 3_000, 3_000);
    let before = fs::read(world.join(save::JOURNAL)).unwrap();
    let dir = scratch("race-fork").join("fork");
    let mut hooked = Vec::new();
    let info = fork::export_with(&world, &dir, |at| {
        let fork::MidCopy::Pages(attempt) = at else {
            return;
        };
        hooked.push(attempt);
        if attempt == 1 {
            let through = save_as_the_bridge(&world, &mut j);
            j.run_until("the pages of the save", |j| {
                j.e.durable_obs_seq() >= through
            });
        }
    })
    .unwrap();
    assert_eq!(hooked, [1, 2]);
    assert_eq!(info.attempts, 2, "{info:?}");
    let now = fs::read(world.join(save::JOURNAL)).unwrap();
    assert_ne!(now, before, "the hook saved the world");
    assert_eq!(fs::read(dir.join(save::JOURNAL)).unwrap(), now);
    let mut src = PageStore::open_read_only(world.join(save::PAGES)).unwrap();
    src.view_confirmed().unwrap();
    assert_eq!(info.pages_obs_seq, src.confirmed().obs_seq);
    assert!(info.journal_first_obs <= info.pages_obs_seq + 1, "{info:?}");
    assert!(
        info.notes.iter().all(|n| !n.contains("in neither")),
        "{:?}",
        info.notes
    );
    let host = Host::open(&dir, &opts(true)).unwrap();
    assert_eq!(restore_failures(&host.e), Vec::<String>::new());
    host.close().unwrap();
    drop(src);
    j.crash();
    let _ = fs::remove_dir_all(dir.parent().unwrap());
    let _ = fs::remove_dir_all(&world);
}

/// A compaction, then a flush, landing between the export's read of the archive's index and its
/// copy of the chunk files the index names (where a live server's fall by chance) make the export
/// copy the archive again, and the fork's index names exactly the chunk files copied with it: the
/// world's archive as the flush left it.
#[test]
fn an_archive_written_during_the_copy_makes_the_export_copy_it_again() {
    use rtsim_archive::store::{INDEX_FILE, chunk_name};
    use rtsim_archive::{Archive, ArchiveReader, Index, Pending, Stamp};
    let world = populated("archive-race", 2);
    let (obs, serial) = {
        let mut s = PageStore::open_read_only(world.join(save::PAGES)).unwrap();
        s.view_confirmed().unwrap();
        (
            s.confirmed().obs_seq,
            archive_serial(&world.join(save::PAGES)),
        )
    };
    let from = rtsim_archive::dir_for(&world);
    let first = Archive::open(&from).unwrap().entries()[0].seq;
    let dir = scratch("archive-race-fork").join("fork");
    let mut hooked = Vec::new();
    let info = fork::export_with(&world, &dir, |at| {
        let fork::MidCopy::Archive(attempt) = at else {
            return;
        };
        hooked.push(attempt);
        let mut a = Archive::open(&from).unwrap();
        match attempt {
            // A compaction's replacement: a new chunk file, the index, then the old file goes.
            1 => {
                let c = a.read(first).unwrap();
                a.replace(&[first], vec![c]).unwrap();
            }
            // A save's flush: a new chunk file, then the index. At the pages' serial, so the
            // fork keeps it.
            2 => {
                let mut d = a.dictionary().clone();
                let mut p = Pending::new();
                p.push(&raid(&mut d, 1_000, 7));
                let stamp = Stamp {
                    obs_seq: obs,
                    serial,
                    tick: 1_000,
                    wall_ms: 1_700_000_000_000,
                };
                a.flush(p.take(), stamp, Some(&d)).unwrap();
            }
            _ => {}
        }
    })
    .unwrap();
    assert_eq!(hooked, [1, 2, 3]);
    assert!(
        info.notes
            .iter()
            .any(|n| n.starts_with("the archive was copied 3 times")),
        "{:?}",
        info.notes
    );
    let to = rtsim_archive::dir_for(&dir);
    let index = |d: &Path| Index::decode(&fs::read(d.join(INDEX_FILE)).unwrap()).unwrap();
    let (world_index, fork_index) = (index(&from), index(&to));
    assert!(!world_index.entries.iter().any(|e| e.seq == first));
    assert_eq!(fork_index, world_index);
    let named: Vec<String> = fork_index
        .entries
        .iter()
        .map(|e| chunk_name(e.seq))
        .collect();
    let mut files: Vec<String> = fs::read_dir(&to)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".rtac"))
        .collect();
    files.sort();
    assert_eq!(
        files, named,
        "the fork's index names exactly its chunk files"
    );
    assert_eq!(info.archive_files, named.len() as u64 + 2, "{info:?}");
    let rows = |d: &Path| {
        ArchiveReader::open(d, None)
            .unwrap()
            .unwrap()
            .rows(0, u64::MAX)
            .unwrap()
    };
    let r = rows(&to);
    assert!(
        r.iter().any(|r| matches!(
            &r.row,
            rtsim_archive::Row::Event(e) if (e.tick, e.x, e.z) == (1_000, 7, 40)
        )),
        "the flush's raid is in the fork"
    );
    assert_eq!(r, rows(&from));
    let _ = fs::remove_dir_all(dir.parent().unwrap());
    let _ = fs::remove_dir_all(&world);
}

/// The host brings a save forward when the event archive's buffer reaches its mark, so a fast run
/// keeps its rows (VIEWER.md §12): with saves due every 1,000 game days, a mark of one row makes
/// every save of a three-day run the buffer's, and the default mark (half the buffer) none.
#[test]
fn a_filling_archive_buffer_brings_the_save_forward() {
    let world = populated("archive-save", 2);
    let parent = scratch("archive-save-fork");
    let mut seen = Vec::new();
    for (name, mark) in [("one", (1, u64::MAX)), ("half", ARCHIVE_SAVE_AT)] {
        let dir = parent.join(name);
        fork::export(&world, &dir).unwrap();
        let o = Options {
            save_every: 1_000 * DAY,
            archive_save_at: mark,
            ..opts(false)
        };
        let (_, host) = run_days(&dir, &o, 3);
        seen.push((host.saves, host.archive_saves));
        assert_eq!(host.e.stats.archive_failures.load(Ordering::Acquire), 0);
        host.close().unwrap();
    }
    eprintln!("  saves, of them the buffer's: {seen:?}");
    assert!(seen[0].1 > 0, "{seen:?}");
    assert_eq!(seen[0].0, seen[0].1, "{seen:?}");
    assert_eq!(seen[1], (0, 0), "{seen:?}");
    let _ = fs::remove_dir_all(&parent);
    let _ = fs::remove_dir_all(&world);
}

/// The host never writes into the world: every file the same, bytes and times, after an export,
/// a run on the fork and a second export. A fork inside the world, the world inside a fork, a
/// folder already in use and a world without rtsim pages are refused.
#[test]
fn the_world_is_never_written_and_overlaps_are_refused() {
    let world = populated("untouched", 2);
    let before = snapshot(&world);
    let world_archive = rtsim_archive::dir_for(&world);
    assert!(
        before
            .keys()
            .any(|p| p.starts_with(&world_archive) && p.extension().is_some_and(|x| x == "rtac")),
        "the world's engine recorded an archive, which the check covers"
    );
    let parent = scratch("untouched-fork");
    let dir = parent.join("fork");
    fork::export(&world, &dir).unwrap();
    let (_, host) = run_days(&dir, &opts(false), 12);
    host.close().unwrap();
    // The fork's engine recorded into the fork's archive: saves past the world's serial, and
    // chunks tagged with them.
    let serial = archive_serial(&world.join(save::PAGES));
    assert!(archive_serial(&dir.join(save::PAGES)) > serial);
    let fork_archive = rtsim_archive::ArchiveReader::open(rtsim_archive::dir_for(&dir), None)
        .unwrap()
        .unwrap();
    assert!(
        fork_archive
            .entries()
            .iter()
            .any(|e| e.header.serial > serial),
        "the fork's own saves wrote chunks into its archive"
    );
    let again = parent.join("again");
    fork::export(&world, &again).unwrap();
    assert!(
        snapshot(&world) == before,
        "the world's files are as they were"
    );
    // The fork resumed: a second session on the same folder goes on from the first.
    let first = fork::ForkInfo::read(&dir).unwrap();
    let host = Host::open(&dir, &opts(false)).unwrap();
    assert!(host.start_time >= first.game_time.unwrap() + 12 * DAY);
    host.close().unwrap();

    assert!(fork::export(&world, &world.join("inside")).is_err());
    assert!(fork::export(&world, &world).is_err());
    assert!(fork::check_apart(&world, world.parent().unwrap()).is_err());
    assert!(fork::export(&world, &dir).is_err(), "a folder in use");
    let not_a_world = scratch("not-a-world");
    assert!(fork::export(&not_a_world, &parent.join("none")).is_err());
    let _ = fs::remove_dir_all(&world);
    let _ = fs::remove_dir_all(&parent);
    let _ = fs::remove_dir_all(&not_a_world);
}

/// The save serial of the confirmed generation in a page directory (the engine's `ARCHIVE_KEY`).
fn archive_serial(pages: &Path) -> u64 {
    let mut s = PageStore::open_read_only(pages).unwrap();
    s.view_confirmed().unwrap();
    let b = s
        .get_shared(rtsim_jni::engine::ARCHIVE_KEY)
        .unwrap()
        .unwrap();
    u64::from_le_bytes(b[..].try_into().unwrap())
}

/// A notable raid at (x, 40) in dim0, numbered in `d`.
fn raid(d: &mut rtsim_archive::Dictionary, tick: u64, x: i32) -> rtsim_archive::Row {
    rtsim_archive::Row::Event(rtsim_archive::Event {
        tick,
        kind: d.kind("conflict.raid").unwrap(),
        frame: d.frame("dim0").unwrap(),
        severity: rtsim_archive::Severity::Notable,
        x,
        z: 40,
        subject: rtsim_archive::Handle::direct(12),
        ..rtsim_archive::Event::default()
    })
}

/// An archive beside `world`'s pages: raids at x 100 to 104 in a flush tagged with the world's
/// generation, and one at x 999 tagged with the save after it, which the world never confirmed.
/// The world's own engine recorded into the same archive; the raids take its save serial (the
/// pages' `ARCHIVE_KEY`) and the one after it.
fn archive(world: &Path, obs_seq: u64, now: u64) {
    use rtsim_archive::{Archive, Pending, Stamp};
    let serial = archive_serial(&world.join(save::PAGES));
    let mut a = Archive::open(rtsim_archive::dir_for(world)).unwrap();
    let mut d = a.dictionary().clone();
    for (seq, serial, rows) in [
        (obs_seq, serial, 100..105),
        (obs_seq + 1, serial + 1, 999..1000),
    ] {
        let mut p = Pending::new();
        for x in rows {
            p.push(&raid(&mut d, now - 1_000 + x as u64, x));
        }
        let stamp = Stamp {
            obs_seq: seq,
            serial,
            tick: now,
            wall_ms: 1_700_000_000_000,
        };
        a.flush(p.take(), stamp, Some(&d)).unwrap();
    }
}

/// A fork keeps the world's base map and timeline, and the host source serves them: Surveyor's
/// summaries of every dimension are copied, and the archive is copied less the chunk of a save
/// the world never confirmed, which the host's own later saves would otherwise bring into view.
#[test]
fn a_fork_keeps_the_worlds_terrain_and_its_confirmed_timeline() {
    let world = populated("terrain", 2);
    let summary =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../rtsim-view/tests/data/surveyor/c.2.0.dat");
    for d in [
        "data/surveyor",
        "dimensions/mic_test/deep/below/data/surveyor",
    ] {
        fs::create_dir_all(world.join(d)).unwrap();
        fs::copy(&summary, world.join(d).join("c.2.0.dat")).unwrap();
    }
    let (obs, now, overworld) = {
        let v = rtsim_view::View::open(&world, Limits::default()).unwrap();
        let s = v.saved();
        (
            s.generation.obs_seq,
            s.now() as u64,
            rtsim_view::view::frame_id(&s, 0),
        )
    };
    let chunks = |d: &Path| {
        fs::read_dir(rtsim_archive::dir_for(d))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".rtac")
            })
            .count()
    };
    let recorded = chunks(&world);
    archive(&world, obs, now);
    let parent = scratch("terrain-fork");
    let dir = parent.join("fork");
    let info = fork::export(&world, &dir).unwrap();
    assert_eq!(info.surveyor_files, 2, "{info:?}");
    assert!(
        info.notes.iter().any(|n| n.starts_with("1 archive chunks")),
        "{:?}",
        info.notes
    );
    assert!(
        dir.join("dimensions/mic_test/deep/below/data/surveyor/c.2.0.dat")
            .is_file()
    );

    assert_eq!((chunks(&world), chunks(&dir)), (recorded + 2, recorded + 1));

    // Two game days in the host with a player about (a fork run by hand may have one), so its
    // saves go past the generation the cut chunk was tagged with.
    let mut host = Host::open(&dir, &opts(false)).unwrap();
    let dim = host.j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    host.j.players = vec![(PLAYER, dim, [10.0, 64.0, 0.0])];
    let o = Options {
        until: Some(Until::After(2 * DAY)),
        ..opts(false)
    };
    host.run(&o, &Control::new(), Duration::from_secs(3600), |_| {})
        .unwrap();
    host.close().unwrap();
    let ctl: &'static Control = Box::leak(Box::new(Control::new()));
    let page = Page::open(&dir, ctl);
    let shown = page.hv.generation().obs_seq;
    assert!(
        shown > obs + 1,
        "the fork's saves went on: {shown} after {obs}"
    );
    let addr = page.server.addr().unwrap();
    let (code, body) = get(addr, "/api/world");
    assert_eq!(code, 200);
    let w: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(w["has_events"], true, "{w}");
    // The raids' window: the world and the fork record rows of their own around it.
    let (code, body) = get(
        addr,
        &format!(
            "/api/events?frame={overworld}&from={}&to={}&limit=100",
            now - 1_000,
            now + 1
        ),
    );
    assert_eq!(code, 200);
    let a: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // The test's raids among the rows the world and the fork recorded themselves.
    let xs: Vec<f64> = a["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["z"].as_f64() == Some(40.0))
        .map(|r| r["x"].as_f64().unwrap())
        .filter(|x| (100.0..105.0).contains(x) || *x == 999.0)
        .collect();
    assert_eq!(xs.len(), 5, "the confirmed raids only: {a}");
    assert!(!xs.contains(&999.0), "{xs:?}");
    let (code, _) = get(
        addr,
        &format!("/api/events/histogram?frame={overworld}&limit=10"),
    );
    assert_eq!(code, 200);
    // Region (2, 0): the four tiles over it at one block a pixel, and the ones above at -1 and -2.
    let mut drawn = Vec::new();
    for (z, x, y) in [
        (0, 4, 0),
        (0, 5, 0),
        (0, 4, 1),
        (0, 5, 1),
        (-1, 2, 0),
        (-2, 1, 0),
    ] {
        let (code, png) = get(
            addr,
            &format!("/tiles/surveyor-minecraft-overworld/{z}/{x}/{y}.png"),
        );
        assert!(
            code == 200 || code == 204,
            "{code}: {}",
            String::from_utf8_lossy(&png)
        );
        if code == 200 {
            assert!(png.starts_with(b"\x89PNG"));
            drawn.push((z, x, y));
        }
    }
    assert!(
        !drawn.is_empty(),
        "Surveyor's terrain is drawn from the fork"
    );
    std::thread::sleep(Duration::from_millis(500));
    let v = page.close(i64::MIN);
    assert_eq!(v.failures, Vec::<String>::new());
    assert_eq!(v.error, None);
    let _ = fs::remove_dir_all(&world);
    let _ = fs::remove_dir_all(&parent);
}

/// One sample of a soak, after a save.
#[derive(Clone, Copy, Debug, Default)]
struct Sample {
    day: i64,
    journal: usize,
    in_flight: u64,
    /// The industry outbox and its queue.
    industry: u64,
    /// The core's pages (table 1): the replay state and the kept command outbox.
    core: u64,
    /// Every page.
    pages: u64,
    heap: u64,
    agents: u64,
}

/// The phase 1 world's meta page by section (raw bytes), then its leg index's legs, bookings and
/// cells and its market rows and market wakes.
type WorldSample = (Vec<(&'static str, usize)>, [usize; 5]);

struct Soak {
    days: i64,
    steps: u64,
    wall: Duration,
    waited: Duration,
    saves: u64,
    peak_journal: usize,
    peak_in_flight: u64,
    refused: BTreeMap<u16, u64>,
    samples: Vec<Sample>,
    /// Bytes and pages by table on day 10, day 100 and the last sampled day.
    tables: Vec<(i64, Tables)>,
    /// On the same days: the phase 1 world's meta page by section, and its counters.
    world: Vec<WorldSample>,
    /// The game time the run ended at.
    end: i64,
    viewer: Option<Viewed>,
    /// The who-is-alive census (EQUILIBRIUM.md S1, `Engine::census`) every 10 game days.
    census: Vec<(i64, Vec<rtsim_kernel::census::KindCensus>)>,
}

impl Soak {
    /// The largest of `f` over the first tenth of the samples, and over the last tenth.
    fn early_late(&self, f: impl Fn(&Sample) -> u64) -> (u64, u64) {
        let tenth = self.samples.len() / 10;
        let early = self.samples[..=tenth].iter().map(&f).max().unwrap();
        let late = self.samples[self.samples.len() - 1 - tenth..]
            .iter()
            .map(&f)
            .max()
            .unwrap();
        (early, late)
    }

    fn print(&self) {
        let secs = self.wall.as_secs_f64();
        eprintln!(
            "soak: {} game days, {} steps in {secs:.1} s: {:.1} game days/s, {:.1} us a step ({:.1} us waiting on the engine); {} saves",
            self.days,
            self.steps,
            self.days as f64 / secs,
            secs * 1e6 / self.steps as f64,
            self.waited.as_secs_f64() * 1e6 / self.steps as f64,
            self.saves
        );
        eprintln!(
            "journal peak {}, in flight peak {}, answered as refused {:x?}",
            self.peak_journal, self.peak_in_flight, self.refused
        );
        eprintln!(" day  journal  in-flight  industry  core-pages  all-pages      heap  agents");
        // Day 10, then every 100 days (or every tenth sample in a short soak), and the last.
        let every = if self.days >= 200 {
            100
        } else {
            (self.days / 10).max(10)
        };
        let last = self.samples.last().map_or(0, |s| s.day);
        let shown = |d: i64| d == 10 || d % every == 0 || d == last;
        for s in self
            .samples
            .iter()
            .filter(|s| shown(s.day))
        {
            eprintln!(
                "{:>4}  {:>7}  {:>9}  {:>8}  {:>10}  {:>9}  {:>5} KiB  {:>6}",
                s.day,
                s.journal,
                s.in_flight,
                s.industry,
                s.core,
                s.pages,
                s.heap >> 10,
                s.agents
            );
        }
        // The census on the same days (`sim census`'s format; renewals since the previous line).
        eprintln!("census:\n{}", rtsim_kernel::census::CensusLog::HEADER);
        let mut log = rtsim_kernel::census::CensusLog::new();
        for (d, cs) in self.census.iter().filter(|(d, _)| shown(*d)) {
            for l in log.lines(*d as u64, cs) {
                eprintln!("{l}");
            }
        }
        if let Some(v) = &self.viewer {
            eprintln!(
                "viewer: {} rereads ({} found nothing newer, the longest {:.1} ms), {} lease saves; the page got {} frame notices and {} answers ({} not 200), and saw the map reach day {:.1}",
                v.rereads,
                v.unchanged,
                v.longest_reread.as_secs_f64() * 1e3,
                v.lease_saves,
                v.frames,
                v.answers,
                v.failures.len(),
                v.last_tick as f64 / DAY as f64
            );
        }
        if let [(_, a), .., (_, b)] = &self.tables[..] {
            let days: String = self
                .tables
                .iter()
                .map(|(d, _)| format!("  {:>17}", format!("day {d}")))
                .collect();
            eprintln!("table   bytes (pages) on{days}");
            for (t, (n1, _)) in b {
                if a.get(t).map_or(0, |x| x.0) == *n1 {
                    continue;
                }
                let row: String = self
                    .tables
                    .iter()
                    .map(|(_, x)| {
                        let (n, p) = x.get(t).copied().unwrap_or_default();
                        format!("  {n:>10} ({p:>4})")
                    })
                    .collect();
                eprintln!("{t:#06x}  {row}");
            }
            eprintln!("phase 1 meta page, raw bytes by section, on the same days");
            if let Some((first, _)) = self.world.first() {
                for (k, (name, _)) in first.iter().enumerate() {
                    let row: String = self
                        .world
                        .iter()
                        .map(|(m, _)| format!("  {:>17}", m.get(k).map_or(0, |x| x.1)))
                        .collect();
                    eprintln!("{name:>24}{row}");
                }
            }
            let names = [
                "legs",
                "leg bookings",
                "leg cells",
                "market rows",
                "market wakes",
            ];
            for (k, name) in names.iter().enumerate() {
                let row: String = self
                    .world
                    .iter()
                    .map(|(_, c)| format!("  {:>17}", c[k]))
                    .collect();
                eprintln!("{name:>24}{row}");
            }
        }
    }
}

/// What the viewer did over a soak, and what a page reading it saw.
#[derive(Debug, Default)]
struct Viewed {
    rereads: u64,
    unchanged: u64,
    longest_reread: Duration,
    lease_saves: u64,
    /// Frame notices on the page's stream.
    frames: u64,
    /// Answers to the page's requests, and the ones that were not 200.
    answers: u64,
    failures: Vec<String>,
    /// The newest tick `/api/world` showed.
    last_tick: i64,
    error: Option<String>,
}

/// One GET on loopback: (status, body).
fn get(addr: SocketAddr, path: &str) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    s.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).unwrap();
    let end = out
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a response head");
    let status = String::from_utf8_lossy(&out[..end])
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap();
    (status, out[end + 4..].to_vec())
}

/// Occurrences of `m` in `tail`; leaves only what could start a cut one.
fn count(tail: &mut Vec<u8>, m: &[u8]) -> u64 {
    let (mut n, mut i, mut after) = (0, 0, 0);
    while i + m.len() <= tail.len() {
        if &tail[i..i + m.len()] == m {
            n += 1;
            i += m.len();
            after = i;
        } else {
            i += 1;
        }
    }
    let keep = after.max(tail.len().saturating_sub(m.len() - 1));
    tail.drain(..keep);
    n
}

/// The host source on loopback, with a page reading it: one stream held open (counting its frame
/// notices) and, every 100 ms, the world, the marks of every layer over the whole frame, the
/// status and a tile of each base map.
struct Page {
    hv: Arc<HostView>,
    server: Server,
    reader: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    /// The newest tick the page's `/api/world` showed.
    seen: Arc<AtomicI64>,
    threads: Vec<JoinHandle<(u64, Vec<String>, i64)>>,
}

impl Page {
    fn open(dir: &Path, ctl: &'static Control) -> Page {
        let heap: rtsim_view::view::HeapProbe =
            Arc::new(|| mic_native_rt::memory::in_use().max(0) as u64);
        let hv = HostView::open(dir, ctl, Speed::Max, Limits::default(), Some(heap)).unwrap();
        let reader = hv.start().unwrap();
        let config = Config {
            allow_tailnet: false,
            name: "rtsim-host-test".into(),
        };
        let server = Server::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            config,
            Arc::new(App::new(Arc::clone(&hv))),
        )
        .unwrap();
        let addr = server.addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let seen = Arc::new(AtomicI64::new(i64::MIN));
        let stream = {
            let (stop, frames) = (Arc::clone(&stop), Arc::clone(&frames));
            std::thread::spawn(move || {
                let mut s = TcpStream::connect(addr).unwrap();
                s.set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                s.write_all(b"GET /api/stream HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
                    .unwrap();
                let mut buf = vec![0u8; 1 << 16];
                let mut tail = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    match s.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            tail.extend_from_slice(&buf[..n]);
                            frames.fetch_add(count(&mut tail, b"event: frame"), Ordering::Relaxed);
                        }
                        Err(_) => continue,
                    }
                }
                (0, Vec::new(), 0)
            })
        };
        let poller = {
            let (stop, seen) = (Arc::clone(&stop), Arc::clone(&seen));
            std::thread::spawn(move || {
                let mut answers = 0u64;
                let mut failures = Vec::new();
                let mut last_tick = i64::MIN;
                let mut check = |path: &str| -> Option<serde_json::Value> {
                    let (status, body) = get(addr, path);
                    answers += 1;
                    if status != 200 && status != 204 {
                        failures.push(format!(
                            "{path}: {status} {}",
                            String::from_utf8_lossy(&body)
                        ));
                        return None;
                    }
                    serde_json::from_slice(&body).ok()
                };
                let layers: Vec<String> = check("/api/layers")
                    .and_then(|l| {
                        l["layers"].as_array().map(|a| {
                            a.iter()
                                .filter_map(|d| d["id"].as_str().map(str::to_owned))
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                while !stop.load(Ordering::Relaxed) {
                    if let Some(w) = check("/api/world") {
                        assert_eq!(w["source"], "host");
                        let t = w["time"]["tick"].as_i64().unwrap();
                        assert!(
                            t >= last_tick,
                            "the map never goes back: {t} after {last_tick}"
                        );
                        last_tick = t;
                        seen.store(t, Ordering::Relaxed);
                        let f = &w["frames"][0];
                        let b: Vec<f64> =
                            (0..4).map(|i| f["bounds"][i].as_f64().unwrap()).collect();
                        let bpp = ((b[2] - b[0]) / 1000.0).max(0.25);
                        check(&format!(
                            "/api/marks?frame={}&x0={}&z0={}&x1={}&z1={}&bpp={bpp}&layers={}",
                            f["id"].as_str().unwrap(),
                            b[0],
                            b[1],
                            b[2],
                            b[3],
                            layers.join(",")
                        ));
                        for t in f["tiles"].as_array().into_iter().flatten() {
                            let Some(url) = t["url"].as_str() else {
                                continue;
                            };
                            let z = t["pyramid"]["max_zoom"].as_i64().unwrap_or(0);
                            let path = url
                                .replace("{z}", &z.to_string())
                                .replace("{x}", "0")
                                .replace("{y}", "0");
                            check(&format!("/{path}"));
                        }
                    }
                    if let Some(s) = check("/api/status") {
                        assert!(s["host"]["error"].is_null(), "{}", s["host"]["error"]);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                (answers, failures, last_tick)
            })
        };
        Page {
            hv,
            server,
            reader,
            stop,
            frames,
            seen,
            threads: vec![stream, poller],
        }
    }

    /// Stops the page once it shows a save from `until` or later. The engine's IO lane commits
    /// saves behind the step, and on a loaded machine (the other soak's fsyncs) it can be several
    /// saves behind when the run ends, so look again until the fork has caught up, then give the
    /// page time to ask (a while, when the machine is loaded).
    fn close(mut self, until: i64) -> Viewed {
        let t0 = Instant::now();
        let mut shown = i64::MIN;
        while t0.elapsed() < Duration::from_secs(30) {
            self.hv.poll();
            shown = self.hv.view().saved().now();
            if shown >= until {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        while self.seen.load(Ordering::Relaxed) < shown && t0.elapsed() < Duration::from_secs(50) {
            std::thread::sleep(Duration::from_millis(50));
        }
        self.stop.store(true, Ordering::Relaxed);
        let mut v = Viewed::default();
        for t in self.threads {
            let (answers, failures, last) = t.join().unwrap();
            v.answers += answers;
            v.failures.extend(failures);
            v.last_tick = v.last_tick.max(last);
        }
        self.hv.stop();
        self.server.stop();
        self.reader.join().unwrap();
        (v.rereads, v.unchanged) = self.hv.rereads();
        v.longest_reread = self.hv.longest_reread();
        v.lease_saves = self.hv.lease_saves();
        v.frames = self.frames.load(Ordering::Relaxed);
        v.error = self.hv.error();
        v
    }
}

/// `days` game days at max on a fork of a populated world, the default stride, real fsyncs,
/// sampled every 10 game days; `viewer`: with the host source served and a page reading it.
fn soak(tag: &str, days: i64, viewer: bool) -> Soak {
    let world = populated(tag, 3);
    let parent = scratch(&format!("{tag}-fork"));
    let dir = parent.join("fork");
    fork::export(&world, &dir).unwrap();
    let o = Options {
        durability: Durability::Fsync,
        ..opts(false)
    };
    let mut host = Host::open(&dir, &o).unwrap();
    let start = host.start_time;
    let ctl: &'static Control = Box::leak(Box::new(Control::new()));
    let page = viewer.then(|| Page::open(&dir, ctl));
    let mut samples = Vec::new();
    let mut tables = Vec::new();
    let mut sections = Vec::new();
    let mut census = Vec::new();
    let t0 = Instant::now();
    let mut ran = 0;
    while ran + 10 <= days {
        ran += 10;
        let oc = Options {
            until: Some(Until::At(start + ran * DAY)),
            ..o.clone()
        };
        host.run(&oc, ctl, Duration::from_secs(3600), |_| {})
            .unwrap();
        let by_table = table_bytes(&dir.join(save::PAGES));
        samples.push(Sample {
            day: ran,
            journal: host.j.journal.len(),
            in_flight: host.in_flight(),
            industry: host.e.with_industry(|_, s| s.outbox + s.queued),
            core: by_table.get(&1).map_or(0, |t| t.0),
            pages: by_table.values().map(|t| t.0).sum(),
            heap: mic_native_rt::memory::in_use().max(0) as u64,
            agents: host.e.stats_array()[11],
        });
        census.push((ran, host.e.census()));
        if ran == 10 || ran == 100 || ran + 10 > days {
            tables.push((ran, by_table));
            sections.push(host.e.with_dragons(|_, w| {
                let (bookings, cells) = w.legs.booking_counts();
                let markets = [w.markets.len(), w.markets.pending_wakes()];
                let counts = [w.legs.len(), bookings, cells, markets[0], markets[1]];
                (w.meta_sections(), counts)
            }));
        }
    }
    // The newest save a page can show is at most two 10-day saves before the end: the engine
    // takes a requested save at its next step, and WORLD_SAVED confirms it a hand-over later.
    let viewer = page.map(|p| p.close(host.last_handed - 20 * DAY));
    let out = Soak {
        viewer,
        end: host.last_handed,
        days: ran,
        steps: host.steps,
        wall: t0.elapsed(),
        waited: host.wait,
        saves: host.saves,
        peak_journal: host.peak_journal,
        peak_in_flight: host.peak_in_flight,
        refused: host.j.refused.clone(),
        samples,
        tables,
        world: sections,
        census,
    };
    assert!(
        host.e.protocol_error().is_none(),
        "{:?}",
        host.e.protocol_error()
    );
    host.close().unwrap();
    out.print();
    let _ = fs::remove_dir_all(&world);
    let _ = fs::remove_dir_all(&parent);
    out
}

fn soak_days() -> i64 {
    std::env::var("RTSIM_HOST_SOAK_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000)
}

/// A thousand game days at max (`RTSIM_HOST_SOAK_DAYS` for another length) with the viewer
/// attached and a page reading it, and the outboxes bounded: Java's journal is trimmed, the
/// commands in flight are answered, the industry outbox and the core's kept command outbox do not
/// grow. Every answer the page got was good, its stream got frame notices, and its map kept up
/// with the run. The measured rate and cost per step are printed for VIEWER.md §12.4.
#[test]
fn a_thousand_days_at_max_with_the_viewer_attached_keeps_the_outboxes_bounded() {
    let s = soak("soak", soak_days(), true);
    let v = s.viewer.as_ref().unwrap();
    assert_eq!(v.failures, Vec::<String>::new(), "every answer was good");
    assert_eq!(v.error, None, "every reread read");
    assert!(v.answers > 10, "the page asked: {v:?}");
    assert!(v.frames > 2, "the stream got frame notices: {v:?}");
    assert!(v.rereads > 10, "the fork was reread after its saves: {v:?}");
    assert!(
        v.last_tick >= s.end - 20 * DAY,
        "the map kept up with the run: {v:?}"
    );
    outboxes_bounded(&s);
}

/// The same soak with no viewer: the host's own rate, for VIEWER.md §12.4.
#[test]
fn a_thousand_days_at_max_keeps_the_outboxes_bounded() {
    outboxes_bounded(&soak("soak-bare", soak_days(), false));
}

fn outboxes_bounded(s: &Soak) {
    assert!(s.days >= 100, "a soak of at least 100 game days");
    assert!(
        s.peak_journal < 2_000,
        "Java's journal is trimmed: peak {}",
        s.peak_journal
    );
    assert!(
        s.peak_in_flight < 1_000,
        "commands are answered: {} in flight at most",
        s.peak_in_flight
    );
    let industry = s.early_late(|x| x.industry);
    assert!(
        industry.1 <= 2 * industry.0 + 64,
        "the industry outbox does not grow: {industry:?}"
    );
    let core = s.early_late(|x| x.core);
    assert!(
        core.1 <= 2 * core.0 + (64 << 10),
        "the core's kept outbox does not grow: {core:?}"
    );
}

/// The whole state over a soak: the pages and the heap flat once the world settles. It failed
/// before the leg index purged its past windows (docs/choices-leg-purge.md LP1): the phase 1
/// world's meta page (table 0x5001) grew from 62 KB on day 10 to 706 KB on day 1,000 with one
/// dragon flying, and the heap with it (VIEWER.md §12.4).
#[test]
fn a_thousand_days_at_max_keeps_the_state_bounded() {
    let s = soak("soak-state", soak_days(), false);
    let pages = s.early_late(|x| x.pages);
    assert!(
        pages.1 <= 2 * pages.0 + (64 << 10),
        "the pages do not grow: {pages:?}"
    );
    let heap = s.early_late(|x| x.heap);
    assert!(
        heap.1 <= 2 * heap.0 + (16 << 20),
        "the heap does not grow: {heap:?}"
    );
    // The census (EQUILIBRIUM.md S1) counts every module's kinds on every sample.
    let kinds = |cs: &[rtsim_kernel::census::KindCensus]| cs.iter().map(|c| c.kind).collect::<Vec<_>>();
    let first = kinds(&s.census[0].1);
    for k in [
        "village.human",
        "dragon.lair",
        "oil.nest",
        "herd.range",
        "ship.haven",
        "conflict.camp",
    ] {
        assert!(first.contains(&k), "the census has {k}: {first:?}");
    }
    for (d, cs) in &s.census {
        assert_eq!(kinds(cs), first, "day {d}");
    }
}
