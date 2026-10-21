// SPDX-License-Identifier: AGPL-3.0-or-later
//! The headless host's exit test (VIEWER.md §12, §16 row V2): a world's pages open in the host
//! with the state hash they were saved with; long strides against 20-tick hand-overs; exports from
//! a world whose server is saving underneath; a world the host never writes into; and a soak of a
//! thousand game days at max with Java's journal, the commands in flight and the kept outboxes
//! bounded.
//!
//! The populated world is made here by the fake Java side, as a server with a player about would
//! leave it: villages with roads and caravans, sea ports and a haven, a dragon's lair with its
//! dragon, a goblin camp. GameTest worlds come from `RTSIM_HOST_WORLDS` (paths, `:` between them)
//! or this checkout's `neoforge/*/run-gametest/world`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use rtsim_conflict::boundary as cb;
use rtsim_ecology::boundary::{self as db, SpawnKind};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::species::{Gender, Species};
use rtsim_host::fork;
use rtsim_host::java::{self, Java, OVERWORLD, PLAYER, WorldSave};
use rtsim_host::nbt::{self, Tag};
use rtsim_host::run::{Control, DAY, Host, Options, STRIDE_FAST, Speed, Until, state_hashes};
use rtsim_host::save::{self, Journal};
use rtsim_jni::engine::Engine;
use rtsim_jni::wire;
use rtsim_kernel::page::{Durability, PageStore};
use rtsim_ships::boundary as sb;

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
/// the host's fast stride.
#[test]
fn gametest_worlds_open_in_the_host_with_their_saved_state() {
    let worlds: Vec<PathBuf> = match std::env::var("RTSIM_HOST_WORLDS") {
        Ok(v) if !v.is_empty() => v.split(':').map(PathBuf::from).collect(),
        _ => {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../neoforge");
            fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path().join("run-gametest/world"))
                .filter(|w| w.join(save::PAGES).is_dir())
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
                save_world(&world, &mut j);
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

/// The host never writes into the world: every file the same, bytes and times, after an export,
/// a run on the fork and a second export. A fork inside the world, the world inside a fork, a
/// folder already in use and a world without rtsim pages are refused.
#[test]
fn the_world_is_never_written_and_overlaps_are_refused() {
    let world = populated("untouched", 2);
    let before = snapshot(&world);
    let parent = scratch("untouched-fork");
    let dir = parent.join("fork");
    fork::export(&world, &dir).unwrap();
    let (_, host) = run_days(&dir, &opts(false), 12);
    host.close().unwrap();
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
    /// Bytes and pages by table on the first sampled day and the last.
    tables: Vec<(i64, Tables)>,
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
        eprintln!(" day  journal  in-flight  industry  core-pages  all-pages     heap  agents");
        for s in self
            .samples
            .iter()
            .step_by((self.samples.len() / 10).max(1))
        {
            eprintln!(
                "{:>4}  {:>7}  {:>9}  {:>8}  {:>10}  {:>9}  {:>4} MiB  {:>6}",
                s.day,
                s.journal,
                s.in_flight,
                s.industry,
                s.core,
                s.pages,
                s.heap >> 20,
                s.agents
            );
        }
        if let [(d0, a), .., (d1, b)] = &self.tables[..] {
            eprintln!("table   bytes (pages) on day {d0}   on day {d1}");
            for (t, (n1, p1)) in b {
                let (n0, p0) = a.get(t).copied().unwrap_or_default();
                if n1 != &n0 {
                    eprintln!("{t:#06x}  {n0:>10} ({p0:>4})  {n1:>10} ({p1:>4})");
                }
            }
        }
    }
}

/// `days` game days at max on a fork of a populated world, the default stride, real fsyncs,
/// sampled after every save.
fn soak(tag: &str, days: i64) -> Soak {
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
    let ctl = Control::new();
    let mut samples = Vec::new();
    let mut tables = Vec::new();
    let t0 = Instant::now();
    let mut ran = 0;
    while ran + 10 <= days {
        ran += 10;
        let oc = Options {
            until: Some(Until::At(start + ran * DAY)),
            ..o.clone()
        };
        host.run(&oc, &ctl, Duration::from_secs(3600), |_| {})
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
        if ran == 10 || ran + 10 > days {
            tables.push((ran, by_table));
        }
    }
    let out = Soak {
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

/// A thousand game days at max (`RTSIM_HOST_SOAK_DAYS` for another length) with the outboxes
/// bounded: Java's journal is trimmed, the commands in flight are answered, the industry outbox
/// and the core's kept command outbox do not grow. The measured rate and cost per step are
/// printed for VIEWER.md §12.4.
#[test]
fn a_thousand_days_at_max_keeps_the_outboxes_bounded() {
    let s = soak("soak", soak_days());
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

/// The whole state over a soak: the pages and the heap flat once the world settles. Fails today:
/// the phase 1 world's leg index (rtsim_travel::legs) never drops its past time windows
/// (`LegIndex::purge` has no caller), so its meta page (table 0x5001) grows about 0.7 KiB a game
/// day with one dragon flying, and the heap with it (VIEWER.md §12.4).
#[test]
#[ignore = "the leg index never purges its past windows (VIEWER.md §12.4)"]
fn a_thousand_days_at_max_keeps_the_state_bounded() {
    let s = soak("soak-state", soak_days());
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
}
