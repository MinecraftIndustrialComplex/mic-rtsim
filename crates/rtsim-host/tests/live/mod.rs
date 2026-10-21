// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the live tap's host tests share (VIEWER.md §16 row V4): the populated world (as
//! `tests/host.rs` makes it), a fork of it run day by day with the state hashes after each day and
//! everything the hand-over carried back, the sim pages as a fork's confirmed generation holds
//! them, and a page hammering every route of the live source with a moving focus.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mic_viewer::http::{Config, Server};
use rtsim_conflict::boundary as cb;
use rtsim_ecology::boundary::{self as db, SpawnKind};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::species::{Gender, Species};
use rtsim_host::fork;
use rtsim_host::java::{self, Java, OVERWORLD, PLAYER, WorldSave};
use rtsim_host::nbt::{self, Tag};
use rtsim_host::run::{Control, DAY, Host, Options, Speed, Until, state_hashes};
use rtsim_host::save::{self, Journal};
use rtsim_jni::wire;
use rtsim_kernel::page::{Durability, PageStore};
use rtsim_ships::boundary as sb;
use rtsim_view::Limits;
use rtsim_view::live::{LeaseWriter, LiveApp, LiveView};

// ---- the populated world (as tests/host.rs makes it) --------------------------------------------

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

pub fn scratch(tag: &str) -> PathBuf {
    let d = java::tmpdir(&format!("tap-{tag}"));
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

fn play(j: &mut Java, ticks: i64, quantum: i64) {
    let end = j.game_time + ticks;
    while j.game_time < end {
        j.game_time += quantum - 2;
        j.idle();
    }
}

fn write_level(world: &Path, time: i64) {
    let root = Tag::Compound(vec![(
        b"Data".to_vec(),
        Tag::Compound(vec![
            (b"Time".to_vec(), Tag::Long(time)),
            (b"DayTime".to_vec(), Tag::Long(time)),
            (b"LevelName".to_vec(), Tag::string("tap-test")),
        ]),
    )]);
    save::write_atomic(&world.join(save::LEVEL), &nbt::write_gz(b"", &root)).unwrap();
}

/// A world a server left: a player in the first village for a day, then nobody for `days - 1`
/// more, saved as the bridge saves, and the engine stopped as a server stops.
pub fn populated(tag: &str, days: i64) -> PathBuf {
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
    let through = j.next_obs - 1;
    j.e.request_save(through);
    j.run_until("the world's save", |j| j.e.durable_obs_seq() >= through);
    Journal {
        save: j.world_save(),
        data_version: Some(3955),
    }
    .write(&world)
    .unwrap();
    write_level(&world, j.game_time);
    j.crash();
    world
}

/// A fork of `world` in a new folder.
pub fn fork_of(world: &Path, tag: &str) -> PathBuf {
    let dir = scratch(tag).join("fork");
    fork::export(world, &dir).unwrap();
    dir
}

pub fn opts() -> Options {
    Options {
        speed: Speed::Max,
        test: true,
        durability: Durability::NoSync,
        ..Options::default()
    }
}

// ---- a run --------------------------------------------------------------------------------------

/// What a run did, for comparing a leased run with an unleased one.
pub struct Run {
    /// Every module's state hash after each game day.
    pub hashes: Vec<Vec<(&'static str, u64)>>,
    /// What the hand-overs carried back to Java, in order: the core's commands and acks, and each
    /// module's commands as Java applied them.
    pub handed_back: Vec<String>,
    /// The tap's counters at the end.
    pub tap: BTreeMap<String, u64>,
    pub steps: u64,
    pub saves: u64,
    pub wall: Duration,
    /// Time the host waited on steps.
    pub waited: Duration,
}

/// Runs the fork in `dir` for `days` game days at max, a day at a time, each day taking at least
/// `pace` of wall time (so the tap's periods pass); the session closed with its final save.
pub fn run(dir: &Path, days: i64, pace: Duration) -> Run {
    let o = opts();
    let mut host = Host::open(dir, &o).unwrap();
    let start = host.start_time;
    let ctl = Control::new();
    let mut hashes = Vec::new();
    let t0 = Instant::now();
    for d in 1..=days {
        let t = Instant::now();
        let od = Options {
            until: Some(Until::At(start + d * DAY)),
            ..o.clone()
        };
        host.run(&od, &ctl, Duration::from_secs(3600), |_| {})
            .unwrap();
        hashes.push(state_hashes(&host.e));
        if let Some(rest) = pace.checked_sub(t.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let wall = t0.elapsed();
    assert!(
        host.e.protocol_error().is_none(),
        "{:?}",
        host.e.protocol_error()
    );
    let j = &host.j;
    let mut handed_back = vec![
        format!("commands {:?}", j.commands),
        format!("acks {:?}", j.acks),
        format!("industry {:?}", j.ind_commands),
        format!("dragons {:?}", j.iaf),
        format!("ships {:?}", j.ships),
        format!("herds {:?}", j.herds),
        format!("conflict {:?}", j.conflict),
        format!("refused {:?}", j.refused),
    ];
    handed_back.push(format!("next obs {} next cmd {}", j.next_obs, j.next_cmd));
    let tap = host.e.stats.tap.snapshot().into_iter().collect();
    let (steps, saves, waited) = (host.steps, host.saves, host.wait);
    host.close().unwrap();
    Run {
        hashes,
        handed_back,
        tap,
        steps,
        saves,
        wall,
        waited,
    }
}

/// Every page of a fork's confirmed generation, by key.
pub fn pages(dir: &Path) -> BTreeMap<u64, Vec<u8>> {
    let mut s = PageStore::open_read_only(dir.join(save::PAGES)).unwrap();
    s.view_confirmed().unwrap();
    s.try_keys()
        .unwrap()
        .into_iter()
        .map(|k| (k.0, s.get_shared(k).unwrap().unwrap_or_default().to_vec()))
        .collect()
}

/// The leased run's and the unleased run's state agree every day, the hand-overs carried back
/// the same records in the same order, and the sim pages they closed with are byte for byte the
/// same.
pub fn assert_lockstep(a: &Run, b: &Run, a_dir: &Path, b_dir: &Path) {
    assert_eq!(a.hashes.len(), b.hashes.len());
    for (d, (x, y)) in a.hashes.iter().zip(&b.hashes).enumerate() {
        assert_eq!(x, y, "the state hashes on game day {}", d + 1);
    }
    assert_eq!((a.steps, a.saves), (b.steps, b.saves), "steps and saves");
    for (x, y) in a.handed_back.iter().zip(&b.handed_back) {
        assert!(
            x == y,
            "the hand-overs carried back other records: {x:.300} / {y:.300}"
        );
    }
    let (pa, pb) = (pages(a_dir), pages(b_dir));
    assert!(!pa.is_empty());
    let keys = |p: &BTreeMap<u64, Vec<u8>>| p.keys().copied().collect::<Vec<_>>();
    assert_eq!(keys(&pa), keys(&pb), "the same pages");
    let differ: Vec<String> = pa
        .iter()
        .filter(|(k, v)| pb.get(*k) != Some(*v))
        .map(|(k, _)| format!("{k:#x}"))
        .collect();
    assert!(differ.is_empty(), "pages that differ: {differ:?}");
}

// ---- a server's pace ----------------------------------------------------------------------------

/// What a run at a server's pace did.
pub struct Served {
    /// Server ticks, hand-overs submitted, and those holding more than one TICK record (Java
    /// ticked again before the step it handed over last had published: the engine behind).
    pub ticks: u64,
    pub submitted: u64,
    pub behind: u64,
    /// Each server tick's own time (applying the commands, the hand-over and its submit), and
    /// each step's time as the engine measured it, nanoseconds.
    pub tick_nanos: Vec<u64>,
    pub step_nanos: Vec<u64>,
    pub tap: BTreeMap<String, u64>,
    pub wall: Duration,
}

/// Runs the fork in `dir` for `days` game days as a server drives the engine (the bridge's
/// pattern, `Java::tick`): every `tick_every` a server tick moves game time on by `stride`,
/// applies what has come back and hands over; a hand-over the engine is not ready for stays open
/// and the next tick's TICK record joins it. A save is asked for every 10 game days, and the
/// session closes with its final save.
pub fn serve(dir: &Path, days: i64, stride: i64, tick_every: Duration) -> Served {
    let mut host = Host::open(dir, &opts()).unwrap();
    let end = host.j.game_time + days * DAY;
    let mut next_save = host.j.game_time + 10 * DAY;
    let (mut ticks, mut submitted, mut behind, mut open) = (0u64, 0u64, 0u64, 0u64);
    let mut tick_nanos = Vec::with_capacity((days * DAY / stride) as usize);
    let mut step_nanos = Vec::with_capacity(tick_nanos.capacity());
    let mut steps_seen = host.e.stats.steps_done.load(Ordering::Acquire);
    let t0 = Instant::now();
    let mut due = t0;
    while host.j.game_time < end {
        due += tick_every;
        if let Some(rest) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(rest);
        }
        if host.j.game_time >= next_save {
            host.e.request_save(host.j.next_obs - 1);
            next_save += 10 * DAY;
        }
        host.j.game_time += stride - 1;
        let t = Instant::now();
        let handed = host.j.tick();
        tick_nanos.push(t.elapsed().as_nanos() as u64);
        ticks += 1;
        open += 1;
        if handed {
            submitted += 1;
            if open > 1 {
                behind += 1;
            }
            open = 0;
        }
        let done = host.e.stats.steps_done.load(Ordering::Acquire);
        if done != steps_seen {
            step_nanos.push(host.e.stats.last_step_nanos.load(Ordering::Acquire));
            steps_seen = done;
        }
        assert!(
            host.e.protocol_error().is_none(),
            "{:?}",
            host.e.protocol_error()
        );
    }
    let wall = t0.elapsed();
    let tap = host.e.stats.tap.snapshot().into_iter().collect();
    host.close().unwrap();
    Served {
        ticks,
        submitted,
        behind,
        tick_nanos,
        step_nanos,
        tap,
        wall,
    }
}

/// The 50th, 99th and 99.9th percentiles and the most of `v`, in microseconds.
pub fn percentiles(v: &[u64]) -> [f64; 4] {
    let mut s = v.to_vec();
    s.sort_unstable();
    let at = |q: f64| {
        s.get(((s.len() as f64 * q) as usize).min(s.len().saturating_sub(1)))
            .map_or(0.0, |n| *n as f64 / 1e3)
    };
    [
        at(0.5),
        at(0.99),
        at(0.999),
        s.last().map_or(0.0, |n| *n as f64 / 1e3),
    ]
}

pub fn print_served(label: &str, r: &Served) {
    let [t50, t99, t999, tmax] = percentiles(&r.tick_nanos);
    let [s50, s99, s999, smax] = percentiles(&r.step_nanos);
    println!(
        "{label}: {} server ticks in {:.1} s, {} hand-overs, {} holding more than one TICK; a \
         server tick's hand-over p50 {t50:.1} µs, p99 {t99:.1}, p99.9 {t999:.1}, most {tmax:.1}; \
         a step p50 {s50:.1} µs, p99 {s99:.1}, p99.9 {s999:.1}, most {smax:.1} ({} sampled)",
        r.ticks,
        r.wall.as_secs_f64(),
        r.submitted,
        r.behind,
        r.step_nanos.len(),
    );
    print_tap(label, &r.tap, None);
}

// ---- the tap's numbers --------------------------------------------------------------------------

fn ms(n: u64) -> f64 {
    n as f64 / 1e6
}

/// The tap's numbers from its counters (VIEWER.md §5.2), with a lockstep run's steps and waits.
pub fn print_tap(label: &str, t: &BTreeMap<String, u64>, run: Option<&Run>) {
    let g = |k: &str| t.get(k).copied().unwrap_or(0);
    let units = g("units").max(1);
    let steps = g("steps_tapped").max(1);
    if let Some(r) = run {
        println!(
            "{label}: {} steps, {} saves, {:.1} s wall, {:.1} s waiting on steps ({:.3} ms a \
             step)",
            r.steps,
            r.saves,
            r.wall.as_secs_f64(),
            r.waited.as_secs_f64(),
            r.waited.as_secs_f64() * 1e3 / r.steps.max(1) as f64
        );
    }
    println!(
        "{label}: tap in {} steps (stood down {}, behind {}, next already there {}, cut by the \
         next {}, state busy {}); {:.3} ms a tapped step, longest {:.3} ms, budget {:.3} ms; over \
         budget {} times, by at most {:.3} ms",
        g("steps_tapped"),
        g("stood_down"),
        g("behind"),
        g("next_already"),
        g("cut_by_next"),
        g("lock_busy"),
        ms(g("tap_nanos")) / steps as f64,
        ms(g("tap_nanos_max")),
        ms(g("budget_nanos")),
        g("over_budget"),
        ms(g("overrun_nanos_max")),
    );
    println!(
        "{label}: {} units encoded ({} not resident), {} rows, {} bytes; {:.3} ms a unit \
         ({:.1} rows), longest {:.3} ms; longest job {:.3} ms; a ready hand-over waited at most \
         {:.3} ms",
        g("units"),
        g("not_resident"),
        g("rows"),
        g("bytes"),
        ms(g("unit_nanos")) / units as f64,
        g("rows") as f64 / units as f64,
        ms(g("unit_nanos_max")),
        ms(g("job_nanos_max")),
        ms(g("ready_wait_nanos_max")),
    );
    println!(
        "{label}: {} cycles, last {} ms; {} commits ({} deferred, longest {:.3} ms), {} parts \
         written, {} unchanged; {} lease stats, epoch {}, {} deletes, {} failures",
        g("cycles"),
        g("cycle_ms"),
        g("commits"),
        g("commits_deferred"),
        ms(g("commit_nanos_max")),
        g("parts_written"),
        g("parts_unchanged"),
        g("lease_stats"),
        g("epoch"),
        g("deletes"),
        g("failures"),
    );
    for (k, v) in t {
        if let Some(id) = k.strip_prefix("unit_nanos_max.") {
            println!(
                "{label}: {id}: {} rows a unit, {} ns a row, longest unit {:.3} ms",
                g(&format!("rows_per_unit.{id}")),
                g(&format!("nanos_per_row.{id}")),
                ms(*v)
            );
        }
    }
}

/// The tap stayed within its budget plus one job (a unit's encode, or the archive's): the most a
/// tapped step ran past the budget, and the most a ready hand-over waited, are each at most the
/// longest job.
pub fn assert_budget(t: &BTreeMap<String, u64>) {
    let g = |k: &str| t.get(k).copied().unwrap_or(0);
    assert!(g("units") > 0, "the leased run encoded units");
    // The overrun and the wait are each measured over one job, so they cannot pass the longest
    // (the clock reads around the job bound them); a millisecond's slack for the reads themselves.
    assert!(
        g("overrun_nanos_max") <= g("job_nanos_max") + 1_000_000,
        "past the budget by more than a job: {t:?}"
    );
    assert!(
        g("tap_nanos_max") <= g("budget_nanos") + g("job_nanos_max") + 1_000_000,
        "a step's tap time is at most the budget and a job: {t:?}"
    );
    assert!(
        g("ready_wait_nanos_max") <= g("job_nanos_max") + 1_000_000,
        "a ready hand-over waited more than a job: {t:?}"
    );
}

// ---- load ---------------------------------------------------------------------------------------

/// Pins every thread of this process, and every thread it starts later (they inherit the mask),
/// to the last `n` CPUs it may run on. Returns them.
pub fn pin_process(n: usize) -> Vec<usize> {
    // SAFETY: plain libc calls on a zeroed cpu_set_t of the right size.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        assert_eq!(
            libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set),
            0
        );
        let allowed: Vec<usize> = (0..libc::CPU_SETSIZE as usize)
            .filter(|&c| libc::CPU_ISSET(c, &set))
            .collect();
        let cpus: Vec<usize> = allowed[allowed.len().saturating_sub(n)..].to_vec();
        let mut pin: libc::cpu_set_t = std::mem::zeroed();
        for &c in &cpus {
            libc::CPU_SET(c, &mut pin);
        }
        let size = std::mem::size_of::<libc::cpu_set_t>();
        for t in fs::read_dir("/proc/self/task").unwrap().flatten() {
            if let Some(tid) = t.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) {
                libc::sched_setaffinity(tid, size, &pin);
            }
        }
        assert_eq!(libc::sched_setaffinity(0, size, &pin), 0);
        cpus
    }
}

/// Threads that spin until dropped, competing with the engine for the CPUs they share.
pub struct Hogs {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<u64>>,
}

impl Hogs {
    pub fn start(n: usize) -> Hogs {
        let stop = Arc::new(AtomicBool::new(false));
        let threads = (0..n)
            .map(|_| {
                let stop = Arc::clone(&stop);
                std::thread::spawn(move || {
                    let mut x = 0u64;
                    while !stop.load(Ordering::Relaxed) {
                        x = std::hint::black_box(x.wrapping_mul(6_364_136_223_846_793_005) + 1);
                    }
                    x
                })
            })
            .collect();
        Hogs { stop, threads }
    }
}

impl Drop for Hogs {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ---- the page -----------------------------------------------------------------------------------

/// One GET on loopback: (status, body).
pub fn get(addr: SocketAddr, path: &str) -> (u16, Vec<u8>) {
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

/// What the page saw.
#[derive(Debug, Default)]
pub struct Hammered {
    /// Answers, by route.
    pub answers: BTreeMap<&'static str, u64>,
    pub failures: Vec<String>,
    /// Frame notices on the stream.
    pub frames: u64,
    /// The newest tick `/api/world` showed, and the live header's commits the status showed.
    pub last_tick: i64,
    pub commits_seen: u64,
    /// Lease writes (the first, renewals and focus changes).
    pub lease_writes: u64,
}

/// The live source of a world on loopback (`rtsim-cli view --live` as a library: the lease, the
/// reader, the routes), with a page holding a stream open and asking every route in a loop: the
/// world, the layers, the marks over a close view that moves around the world (the lease's focus)
/// and over the whole frame, a card of every thing the close view showed, the events and their
/// histogram, a tile of each base map, and the status.
pub struct Hammer {
    pub live: Arc<LiveView>,
    server: Server,
    reader: JoinHandle<()>,
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    stream: JoinHandle<()>,
    poller: JoinHandle<Hammered>,
}

impl Hammer {
    /// Opens the live source on `dir` and starts the page; returns once the lease is on disk.
    pub fn open(dir: &Path) -> Hammer {
        let rtsim = dir.join(save::PAGES);
        let lease = LeaseWriter::start(&rtsim).unwrap();
        let heap: rtsim_view::view::HeapProbe =
            Arc::new(|| mic_native_rt::memory::in_use().max(0) as u64);
        let live = LiveView::open(dir, Limits::default(), Some(heap), Some(lease)).unwrap();
        let reader = live.start().unwrap();
        let server = Server::start(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            Config {
                allow_tailnet: false,
                name: "rtsim-tap-test".into(),
            },
            Arc::new(LiveApp::new(Arc::clone(&live))),
        )
        .unwrap();
        let addr = server.addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
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
            })
        };
        let poller = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || poll_routes(addr, &stop))
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while !rtsim.join(rtsim_parts::store::LEASE_FILE).exists() {
            assert!(Instant::now() < deadline, "the lease was written");
            std::thread::sleep(Duration::from_millis(10));
        }
        Hammer {
            live,
            server,
            reader,
            stop,
            frames,
            stream,
            poller,
        }
    }

    /// Stops the page, the server, the reader and the lease (which removes it).
    pub fn close(mut self) -> Hammered {
        // A last look, so the page has seen the store as the run left it.
        self.live.poll();
        std::thread::sleep(Duration::from_millis(300));
        self.stop.store(true, Ordering::Relaxed);
        let mut h = self.poller.join().unwrap();
        self.stream.join().unwrap();
        h.frames = self.frames.load(Ordering::Relaxed);
        h.lease_writes = self
            .live
            .lease()
            .map_or(0, |l| l.status()["writes"].as_u64().unwrap_or(0));
        self.live.stop();
        self.server.stop();
        self.reader.join().unwrap();
        h
    }
}

/// The page's loop (see [`Hammer`]).
fn poll_routes(addr: SocketAddr, stop: &AtomicBool) -> Hammered {
    let mut h = Hammered {
        last_tick: i64::MIN,
        ..Hammered::default()
    };
    let ask = |route: &'static str, path: &str, h: &mut Hammered| -> Option<serde_json::Value> {
        let (status, body) = get(addr, path);
        *h.answers.entry(route).or_default() += 1;
        // A thing the close view showed can be gone by its card's request.
        if status == 200 || status == 204 || (status == 404 && route == "thing") {
            serde_json::from_slice(&body).ok()
        } else {
            h.failures.push(format!(
                "{path}: {status} {}",
                String::from_utf8_lossy(&body)
            ));
            None
        }
    };
    let mut step = 0u32;
    while !stop.load(Ordering::Relaxed) {
        let layers: Vec<String> = ask("layers", "/api/layers", &mut h)
            .and_then(|l| {
                l["layers"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|d| d["id"].as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default();
        let Some(w) = ask("world", "/api/world", &mut h) else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        assert_eq!(w["source"], "live");
        let t = w["time"]["tick"].as_i64().unwrap_or(i64::MIN);
        assert!(
            t >= h.last_tick,
            "the map never goes back: {t} after {}",
            h.last_tick
        );
        h.last_tick = t;
        let f = &w["frames"][0];
        let id = f["id"].as_str().unwrap_or("").to_owned();
        let b: Vec<f64> = (0..4)
            .map(|i| f["bounds"][i].as_f64().unwrap_or(0.0))
            .collect();
        let all = layers.join(",");
        // The close view: 600 blocks on a side, its centre walking round the frame (a new focus
        // about every second).
        let a = f64::from(step / 10) * 0.7;
        let (cx, cz) = (
            (b[0] + b[2]) / 2.0 + (b[2] - b[0]) / 2.0 * a.cos(),
            (b[1] + b[3]) / 2.0 + (b[3] - b[1]) / 2.0 * a.sin(),
        );
        let close = format!(
            "/api/marks?frame={id}&x0={}&z0={}&x1={}&z1={}&bpp=2&layers={all}",
            cx - 300.0,
            cz - 300.0,
            cx + 300.0,
            cz + 300.0
        );
        if let Some(m) = ask("marks", &close, &mut h) {
            let mut cards = 0;
            for (layer, lm) in m["layers"].as_object().into_iter().flatten() {
                for mk in lm["marks"].as_array().into_iter().flatten() {
                    if let Some(thing) = mk["id"].as_str()
                        && cards < 8
                    {
                        cards += 1;
                        ask("thing", &format!("/api/thing/{layer}/{thing}"), &mut h);
                    }
                }
            }
        }
        let bpp = ((b[2] - b[0]) / 1000.0).max(0.25);
        ask(
            "marks",
            &format!(
                "/api/marks?frame={id}&x0={}&z0={}&x1={}&z1={}&bpp={bpp}&layers={all}",
                b[0], b[1], b[2], b[3]
            ),
            &mut h,
        );
        ask(
            "events",
            &format!("/api/events?frame={id}&limit=500"),
            &mut h,
        );
        ask(
            "events",
            &format!(
                "/api/events?frame={id}&x0={}&z0={}&x1={}&z1={}&from={}&limit=100",
                cx - 300.0,
                cz - 300.0,
                cx + 300.0,
                cz + 300.0,
                t - 10 * DAY
            ),
            &mut h,
        );
        ask(
            "histogram",
            &format!("/api/events/histogram?frame={id}&buckets=120"),
            &mut h,
        );
        for tile in f["tiles"].as_array().into_iter().flatten() {
            let Some(url) = tile["url"].as_str() else {
                continue;
            };
            let z = tile["pyramid"]["max_zoom"].as_i64().unwrap_or(0);
            let path = url
                .replace("{z}", &z.to_string())
                .replace("{x}", "0")
                .replace("{y}", "0");
            ask("tiles", &format!("/{path}"), &mut h);
        }
        if let Some(s) = ask("status", "/api/status", &mut h) {
            assert_eq!(s["source"], "live");
            assert!(s["live"]["error"].is_null(), "{}", s["live"]["error"]);
            h.commits_seen = h
                .commits_seen
                .max(s["live"]["commit"].as_u64().unwrap_or(0));
        }
        step += 1;
        std::thread::sleep(Duration::from_millis(50));
    }
    h
}
