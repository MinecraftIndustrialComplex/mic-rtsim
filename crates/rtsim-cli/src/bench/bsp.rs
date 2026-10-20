// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli bench bsp`: the bulk-synchronous parallel step (SCALE.md §5) at several worker
//! counts, and the determinism gate (PLAN.md §4.7): the same world, run with 1 worker and with N,
//! resident or paged under a budget, must end in the same state hash. `--check` exits 1 when it
//! does not.
//!
//! The world is `regions` trading posts (`rtsim_kernel::synth::TradePost`): each has a herd on
//! day slots (real catch-up work when it wakes), grain, and a caravan that leaves every one to four
//! days for a destination drawn from the counter RNG and arrives half a day to a day and a half
//! later as a message. Each event can also do `work` rounds of hashing, standing for what a module
//! does per event: the bare engine's own work per event is under a microsecond, too little to
//! split across threads. Steps are `step` ticks long; saves run every 6,000 ticks. Every point
//! runs in its own child process.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use rtsim_kernel::bsp::Bsp;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::exec::{Executor, Inline, ThreadPool};
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_kernel::paging::{PagedTable, PagingConfig};
use rtsim_kernel::rng::SystemId;
use rtsim_kernel::synth::TradePost;
use serde::{Deserialize, Serialize};

use crate::sys;

/// Ticks between saves, Minecraft's autosave interval.
const SAVE_EVERY: u64 = 6_000;

/// The worlds the bench runs: (step length in ticks, rounds of work per event). Work 0 is the bare
/// engine; 1,500 rounds stand for a module's work per event (about 10 us on the reference laptop;
/// the results record what it measured). PLACEHOLDER.
const WORLDS: [(u64, u32); 4] = [(20, 0), (600, 0), (20, 1_500), (600, 1_500)];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BspSpec {
    pub regions: u64,
    pub warmup_days: u64,
    pub days: u64,
    /// Ticks per step.
    pub step: u64,
    pub workers: usize,
    /// Resident page budget; `None` keeps every page resident.
    pub budget_bytes: Option<u64>,
    pub regions_per_page: u64,
    pub seed: u64,
    /// Rounds of work per event in each post (`TradePost::work`), standing for module logic.
    #[serde(default)]
    pub work: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BspMeasured {
    /// Wall time per sim day over the measured days, saves (with fsync) included.
    pub wall_ns_per_day: f64,
    /// CPU time of the whole process per sim day (every worker).
    pub process_cpu_ns_per_day: Option<f64>,
    /// Wall time inside `Bsp::step` only, per sim day (no saves): what parallelism buys, and
    /// what the speedup column compares.
    pub step_wall_ns_per_day: f64,
    pub wakes_per_day: f64,
    pub messages_per_day: f64,
    pub regions_per_step: f64,
    pub jobs_per_step: f64,
    pub loads: u64,
    pub evictions: u64,
    pub heap_live_bytes: u64,
    pub rss_peak_bytes: Option<u64>,
    /// CRC32 over every page (caught up to the end) and over the mail in flight.
    pub state_hash: u32,
    pub mail_hash: u32,
    pub conservation: String,
}

fn io<T>(r: rtsim_kernel::page::Result<T>) -> T {
    r.unwrap_or_else(|e| panic!("page store: {e}"))
}

fn scratch_dir(tag: &str) -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.join("bench-scratch")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("{tag}-{}", std::process::id()))
}

pub fn run_one(spec: &BspSpec) -> BspMeasured {
    let dir = scratch_dir("bsp");
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = io(PageStore::open(&dir, Durability::NoSync));
    let cfg = PagingConfig {
        regions_per_page: spec.regions_per_page,
        budget_bytes: spec.budget_bytes.map_or(usize::MAX, |b| b as usize),
        ..PagingConfig::new(1, 2)
    };
    let mut table: PagedTable<TradePost> =
        PagedTable::new(spec.seed, SystemId::named("bench.post"), Tick::ZERO, cfg);
    let mut grain0 = 0i64;
    for k in 0..spec.regions {
        let p = TradePost::new(spec.seed, k, spec.regions).with_work(spec.work);
        grain0 += p.grain;
        io(table.insert(&mut store, k, p, Tick::ZERO));
        if spec.budget_bytes.is_some() && (k + 1) % (spec.regions_per_page * 256) == 0 {
            save(&mut table, &mut store);
        }
    }
    save(&mut table, &mut store);
    store.set_durability(Durability::Fsync);
    let pool;
    let exec: &dyn Executor = if spec.workers <= 1 {
        &Inline
    } else {
        pool = ThreadPool::new(spec.workers);
        &pool
    };
    let mut bsp: Bsp<TradePost> = Bsp::new(Tick::ZERO);
    let mut now = Tick::ZERO;
    let mut step_ns = 0u128;
    let mut run = |to: Tick,
                   now: &mut Tick,
                   table: &mut PagedTable<TradePost>,
                   store: &mut PageStore,
                   bsp: &mut Bsp<TradePost>,
                   timed: bool| {
        while *now < to {
            *now = Tick((now.0 + spec.step).min(to.0));
            let t = Instant::now();
            let r = io(bsp.step(table, store, exec, *now));
            if timed {
                step_ns += t.elapsed().as_nanos();
            }
            assert!(r.dead_letters.is_empty(), "every destination exists");
            if now.0 % SAVE_EVERY < spec.step {
                save(table, store);
            }
            io(table.prefetch(store, *now));
            table.enforce_budget();
        }
    };
    run(
        Tick::from_days(spec.warmup_days),
        &mut now,
        &mut table,
        &mut store,
        &mut bsp,
        false,
    );
    bsp.reset_stats();
    table.reset_stats();
    sys::reset_heap_peak();
    sys::trim_heap();
    sys::reset_rss_peak();
    let (cpu0, wall0) = (sys::process_cpu_ns(), Instant::now());
    let end = Tick::from_days(spec.warmup_days + spec.days);
    run(end, &mut now, &mut table, &mut store, &mut bsp, true);
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = match (cpu0, sys::process_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    };
    let days = spec.days as f64;
    let st = bsp.stats();
    let ps = table.stats();
    let heap_live = sys::heap_live() as u64;
    let (_, rss_peak) = sys::rss();

    // Conservation over the stored states (10 grain per region per committed day slot), then the
    // hash, which catches every region up to the end first.
    let (mut held, mut produced) = (0i64, 0i64);
    io(table.for_each(&mut store, |_, cell, _| {
        held += cell.model().grain;
        produced += 10 * cell.anchor().day() as i64;
    }));
    let flying: i64 = bsp.iter_mail().map(|e| e.msg.0).sum();
    let conservation = if held + flying == grain0 + produced {
        "ok".to_string()
    } else {
        format!("held {held} + in flight {flying} != initial {grain0} + produced {produced}")
    };
    let state_hash = io(table.state_hash(&mut store, end));
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
    BspMeasured {
        wall_ns_per_day: wall / days,
        process_cpu_ns_per_day: cpu.map(|c| c / days),
        step_wall_ns_per_day: step_ns as f64 / days,
        wakes_per_day: st.wakes as f64 / days,
        messages_per_day: st.messages as f64 / days,
        regions_per_step: st.regions as f64 / st.steps.max(1) as f64,
        jobs_per_step: st.jobs as f64 / st.steps.max(1) as f64,
        loads: ps.loads,
        evictions: ps.evictions,
        heap_live_bytes: heap_live,
        rss_peak_bytes: rss_peak,
        state_hash,
        mail_hash: bsp.mail_hash(),
        conservation,
    }
}

fn save(table: &mut PagedTable<TradePost>, store: &mut PageStore) {
    let mut b = SaveBatch::new();
    let t = table.save_dirty(&mut b);
    table.save_meta(&mut b);
    io(store.commit(b, 0));
    table.saved(t);
    table.enforce_budget();
}

// ---- The parent side ----

pub struct Options {
    pub quick: bool,
    pub check: bool,
    pub repeats: u32,
    pub out: Option<PathBuf>,
    pub workers: Vec<usize>,
    pub regions: Option<u64>,
    /// Appended to the result file's name and recorded in it (e.g. `potato`).
    pub tag: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub spec: BspSpec,
    pub measured: BspMeasured,
    /// Step wall ns per day in every repeat; the median is kept.
    pub repeats: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gate {
    /// One world (step length and work per event); the hashes of every worker count and budget
    /// that ran it.
    pub step: u64,
    pub work: u32,
    pub hashes: Vec<(usize, Option<u64>, u32, u32)>,
    pub pass: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub date_utc: String,
    pub mode: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub build_profile: String,
    pub machine: sys::Machine,
    /// Logical CPUs this process may run on (a `taskset` shows here).
    pub allowed_cpus: Option<String>,
    pub tag: Option<String>,
    pub nice: Option<i32>,
    pub loadavg_before: Option<String>,
    pub loadavg_after: Option<String>,
    pub points: Vec<Point>,
    pub gates: Vec<Gate>,
}

fn run_child(spec: &BspSpec) -> Result<BspMeasured, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(spec).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .args(["bench", "bsp-one", "--spec", &json])
        .output()
        .map_err(|e| format!("spawn: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "child failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("child output: {e}"))
}

pub fn run(opts: Options) -> i32 {
    let regions = opts
        .regions
        .unwrap_or(if opts.quick { 20_000 } else { 200_000 });
    let workers = if opts.workers.is_empty() {
        if opts.quick {
            vec![1, 4]
        } else {
            vec![1, 2, 4, 8, 16]
        }
    } else {
        opts.workers.clone()
    };
    let days = if opts.quick { 2 } else { 6 };
    // A budget a quarter of the world's pages (TradePost pages are about 9 KiB), so the paged
    // points evict and reload all the time. PLACEHOLDER.
    let budget = (regions / 64).max(1) * 9 * 1024 / 4;
    let mut specs = Vec::new();
    for (step, work) in WORLDS {
        for &w in &workers {
            specs.push(BspSpec {
                regions,
                warmup_days: 1,
                days,
                step,
                workers: w,
                budget_bytes: None,
                regions_per_page: 64,
                seed: 0x0B5F_0001,
                work,
            });
        }
        // The same world paged: at 1 worker and at the most workers.
        for &w in [workers[0], *workers.last().expect("workers")].iter() {
            specs.push(BspSpec {
                regions,
                warmup_days: 1,
                days,
                step,
                workers: w,
                budget_bytes: Some(budget),
                regions_per_page: 64,
                seed: 0x0B5F_0001,
                work,
            });
        }
    }
    let loadavg_before = sys::loadavg();
    eprintln!(
        "bench bsp ({}): {} points, {regions} regions, load {}",
        if opts.quick { "quick" } else { "full" },
        specs.len(),
        loadavg_before.as_deref().unwrap_or("?")
    );
    let mut points = Vec::new();
    for spec in specs {
        eprint!(
            "  step {} work {} workers {} {} ... ",
            spec.step,
            spec.work,
            spec.workers,
            spec.budget_bytes
                .map_or("resident".to_string(), |b| format!(
                    "budget {} KiB",
                    b / 1024
                ))
        );
        let t = Instant::now();
        let mut got = Vec::new();
        for _ in 0..opts.repeats.max(1) {
            match run_child(&spec) {
                Ok(m) => got.push(m),
                Err(e) => {
                    eprintln!("FAILED: {e}");
                    return 2;
                }
            }
        }
        eprintln!("{:.1} s", t.elapsed().as_secs_f64());
        let repeats: Vec<f64> = got.iter().map(|m| m.step_wall_ns_per_day).collect();
        got.sort_by(|a, b| a.step_wall_ns_per_day.total_cmp(&b.step_wall_ns_per_day));
        let measured = got.swap_remove(got.len() / 2);
        points.push(Point {
            spec,
            measured,
            repeats,
        });
    }

    let mut gates = Vec::new();
    for (step, work) in WORLDS {
        let hashes: Vec<(usize, Option<u64>, u32, u32)> = points
            .iter()
            .filter(|p| p.spec.step == step && p.spec.work == work)
            .map(|p| {
                (
                    p.spec.workers,
                    p.spec.budget_bytes,
                    p.measured.state_hash,
                    p.measured.mail_hash,
                )
            })
            .collect();
        let pass = hashes
            .iter()
            .all(|h| (h.2, h.3) == (hashes[0].2, hashes[0].3));
        gates.push(Gate {
            step,
            work,
            hashes,
            pass,
        });
    }

    print_tables(&points, &gates);

    let (date, stamp) = super::scale::now_utc();
    let report = Report {
        schema: 1,
        date_utc: date,
        mode: if opts.quick { "quick" } else { "full" }.into(),
        git_commit: super::scale::command_line("git", &["rev-parse", "--short=12", "HEAD"]),
        git_dirty: super::scale::command_line(
            "git",
            &["status", "--porcelain", "--untracked-files=no"],
        )
        .map(|s| !s.is_empty()),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        machine: sys::Machine::probe(),
        allowed_cpus: sys::allowed_cpus(),
        tag: opts.tag.clone(),
        nice: super::scale::nice_level(),
        loadavg_before,
        loadavg_after: sys::loadavg(),
        points,
        gates,
    };
    if let Some(dir) = &opts.out {
        match write_report(dir, &stamp, &report) {
            Ok(p) => eprintln!("wrote {}", p.display()),
            Err(e) => {
                eprintln!("could not write the report: {e}");
                return 2;
            }
        }
    }
    let broken: Vec<&Point> = report
        .points
        .iter()
        .filter(|p| p.measured.conservation != "ok")
        .collect();
    for p in &broken {
        eprintln!(
            "conservation broken at step {} workers {}: {}",
            p.spec.step, p.spec.workers, p.measured.conservation
        );
    }
    let failed = report.gates.iter().any(|g| !g.pass);
    if failed {
        eprintln!("determinism gate FAILED: state hashes differ across worker counts or budgets");
    }
    if opts.check && (failed || !broken.is_empty()) {
        return 1;
    }
    if opts.check {
        eprintln!("determinism gate passed");
    }
    0
}

fn write_report(dir: &Path, stamp: &str, report: &Report) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let tag = report
        .tag
        .as_ref()
        .map_or(String::new(), |t| format!("-{t}"));
    let path = dir.join(format!("bsp-{stamp}-{}{tag}.json", report.mode));
    let mut f = std::fs::File::create(&path)?;
    serde_json::to_writer_pretty(&mut f, report)?;
    writeln!(f)?;
    Ok(path)
}

fn print_tables(points: &[Point], gates: &[Gate]) {
    println!(
        "{:>5} {:>5} {:>7} {:>10} | {:>11} {:>8} {:>11} {:>11} | {:>9} {:>9} {:>8} {:>8} | {:>7} {:>7} | {:>8} {:>8} conserved",
        "step",
        "work",
        "workers",
        "pages",
        "wall ms/day",
        "step x",
        "step ms/day",
        "cpu ms/day",
        "wakes/day",
        "msgs/day",
        "reg/step",
        "jobs/st",
        "loads",
        "evicts",
        "hash",
        "mail"
    );
    for p in points {
        let base = points
            .iter()
            .find(|q| {
                q.spec.step == p.spec.step
                    && q.spec.work == p.spec.work
                    && q.spec.budget_bytes == p.spec.budget_bytes
                    && q.spec.workers == 1
            })
            .map(|q| q.measured.step_wall_ns_per_day);
        let m = &p.measured;
        println!(
            "{:>5} {:>5} {:>7} {:>10} | {:>11.1} {:>8} {:>11.1} {:>11} | {:>9.0} {:>9.0} {:>8.1} {:>8.1} | {:>7} {:>7} | {:08x} {:08x} {}",
            p.spec.step,
            p.spec.work,
            p.spec.workers,
            p.spec
                .budget_bytes
                .map_or("resident".into(), |b| format!("{}KiB", b / 1024)),
            m.wall_ns_per_day / 1e6,
            base.map_or("-".into(), |b| format!(
                "{:.2}x",
                b / m.step_wall_ns_per_day
            )),
            m.step_wall_ns_per_day / 1e6,
            m.process_cpu_ns_per_day
                .map_or("-".into(), |c| format!("{:.1}", c / 1e6)),
            m.wakes_per_day,
            m.messages_per_day,
            m.regions_per_step,
            m.jobs_per_step,
            m.loads,
            m.evictions,
            m.state_hash,
            m.mail_hash,
            m.conservation
        );
    }
    println!();
    for g in gates {
        println!(
            "determinism gate, step {} work {}: {} ({} runs: {})",
            g.step,
            g.work,
            if g.pass { "pass" } else { "FAIL" },
            g.hashes.len(),
            g.hashes
                .iter()
                .map(|(w, b, h, m)| format!(
                    "{w}w{} {h:08x}/{m:08x}",
                    b.map_or(String::new(), |b| format!("@{}KiB", b / 1024))
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
