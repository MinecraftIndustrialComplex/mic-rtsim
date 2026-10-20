// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli bench o5`: where per-slot commits cost and where dyadic counts fix them, for Ben's
//! decision on DECISIONS.md O5 (and O17). It measures; it changes nothing.
//!
//! **The question.** A random process that commits once per slot (SCALE.md §4) pays one commit per
//! slot that passed since the region was last touched. A dyadic process (O17) pays a fixed cost per
//! read, whatever the gap. Which is cheaper depends on how often a region is touched.
//!
//! **The sweep.** `regions` regions of one model are each read once every `cadence` days, at mid-day,
//! staggered so the same number are read every day, for `days` days after a first `cadence` days
//! that bring every region into the cycle. The cost reported is CPU per
//! region per sim day: the steady-state price of keeping a region's randomness current at that
//! cadence. A slot model pays about one commit per region-day at any cadence (it is eager stepping,
//! paid late); a dyadic model pays one read per `cadence` days.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::lazy::LazyTable;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_kernel::synth::{DyadicRegion, SynthRegion};
use serde::{Deserialize, Serialize};

use super::world::{BenchModel, ModelKind, WorldConfig};
use crate::sys;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct O5Spec {
    pub model: ModelKind,
    pub regions: u64,
    pub cadence_days: u64,
    pub days: u64,
    pub latent_per_region: u64,
    pub seed: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct O5Measured {
    pub cpu_ns_per_region_day: Option<f64>,
    pub wall_ns_per_region_day: f64,
    pub reads: u64,
    pub slot_commits: u64,
    pub ns_per_read: Option<f64>,
}

pub fn run_one(spec: &O5Spec) -> O5Measured {
    match spec.model {
        ModelKind::Herd => run_model::<SynthRegion>(spec),
        ModelKind::Dyadic => run_model::<DyadicRegion>(spec),
        ModelKind::Closed => panic!("the closed model has no randomness to compare"),
    }
}

fn run_model<M: BenchModel>(spec: &O5Spec) -> O5Measured {
    let cfg = WorldConfig {
        model: spec.model,
        regions: spec.regions,
        latent: spec.latent_per_region * spec.regions,
        events_per_day: 0,
        observers: 0,
        regions_per_page: 64,
        warmup_days: 0,
        days: 0,
        seed: spec.seed,
    };
    let mut table: LazyTable<M> =
        LazyTable::new(spec.seed, SystemId::named("bench.region"), Tick::ZERO);
    let setup = StreamKey::new(spec.seed, SystemId::named("bench.setup"), 0);
    let ids: Vec<_> = (0..spec.regions)
        .map(|i| table.insert(i, M::make(i, &cfg, &mut setup.slot(i)), Tick::ZERO))
        .collect();
    let k = spec.cadence_days.max(1);
    // The first k days only bring every region into the cycle (each is read once): the measured
    // window is the `days` after them, the steady state.
    let (mut cpu0, mut wall0) = (sys::thread_cpu_ns(), Instant::now());
    for day in 0..k + spec.days {
        if day == k {
            table.reset_stats();
            (cpu0, wall0) = (sys::thread_cpu_ns(), Instant::now());
        }
        let at = Tick::from_days(day) + TICKS_PER_DAY / 2;
        while table.next_due(at).is_some() {}
        // Regions i with (i + day) % k == 0: every region once per k days, regions / k a day.
        let mut i = (k - day % k) % k;
        while i < spec.regions {
            std::hint::black_box(table.read(ids[i as usize], at));
            i += k;
        }
    }
    let wall = wall0.elapsed().as_nanos() as f64;
    let cpu = match (cpu0, sys::thread_cpu_ns()) {
        (Some(a), Some(b)) => Some((b - a) as f64),
        _ => None,
    };
    let st = table.stats();
    let region_days = (spec.regions * spec.days) as f64;
    O5Measured {
        cpu_ns_per_region_day: cpu.map(|c| c / region_days),
        wall_ns_per_region_day: wall / region_days,
        reads: st.reads,
        slot_commits: st.slot_commits,
        ns_per_read: cpu.map(|c| c / st.reads.max(1) as f64),
    }
}

pub struct Options {
    pub quick: bool,
    pub repeats: u32,
    pub out: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Point {
    pub spec: O5Spec,
    pub measured: O5Measured,
    pub repeats: Vec<Option<f64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub date_utc: String,
    pub mode: String,
    pub git_commit: Option<String>,
    pub build_profile: String,
    pub machine: sys::Machine,
    pub allowed_cpus: Option<String>,
    pub nice: Option<i32>,
    pub loadavg_before: Option<String>,
    pub loadavg_after: Option<String>,
    pub points: Vec<Point>,
}

fn run_child(spec: &O5Spec) -> Result<O5Measured, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let json = serde_json::to_string(spec).map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .args(["bench", "o5-one", "--spec", &json])
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
    let cadences: Vec<u64> = if opts.quick {
        vec![1, 16, 256]
    } else {
        vec![1, 2, 4, 8, 16, 32, 64, 256, 1024]
    };
    let days = if opts.quick { 256 } else { 1024 };
    let regions = if opts.quick { 500 } else { 1_000 };
    let loadavg_before = sys::loadavg();
    let mut points = Vec::new();
    for model in [ModelKind::Herd, ModelKind::Dyadic] {
        for &k in &cadences {
            let spec = O5Spec {
                model,
                regions,
                cadence_days: k,
                days,
                latent_per_region: 1_000,
                seed: 0x0005_0005,
            };
            eprint!("  {model:?} every {k} days ... ");
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
            let repeats: Vec<Option<f64>> = got.iter().map(|m| m.cpu_ns_per_region_day).collect();
            got.sort_by(|a, b| {
                a.cpu_ns_per_region_day
                    .unwrap_or(f64::NAN)
                    .total_cmp(&b.cpu_ns_per_region_day.unwrap_or(f64::NAN))
            });
            let measured = got.swap_remove(got.len() / 2);
            points.push(Point {
                spec,
                measured,
                repeats,
            });
        }
    }
    print_table(&points, &cadences);
    let (date, stamp) = super::scale::now_utc();
    let report = Report {
        schema: 1,
        date_utc: date,
        mode: if opts.quick { "quick" } else { "full" }.into(),
        git_commit: super::scale::command_line("git", &["rev-parse", "--short=12", "HEAD"]),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        machine: sys::Machine::probe(),
        allowed_cpus: sys::allowed_cpus(),
        nice: super::scale::nice_level(),
        loadavg_before,
        loadavg_after: sys::loadavg(),
        points,
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
    0
}

fn write_report(dir: &Path, stamp: &str, report: &Report) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("o5-{stamp}-{}.json", report.mode));
    let mut f = std::fs::File::create(&path)?;
    serde_json::to_writer_pretty(&mut f, report)?;
    writeln!(f)?;
    Ok(path)
}

fn print_table(points: &[Point], cadences: &[u64]) {
    println!(
        "{:>8} | {:>13} {:>13} {:>11} | {:>12} {:>12} | {:>7}",
        "every k",
        "slot ns/r-day",
        "dyad ns/r-day",
        "slot/dyad",
        "slot ns/read",
        "dyad ns/read",
        "commits"
    );
    let get = |m: ModelKind, k: u64| {
        points
            .iter()
            .find(|p| p.spec.model == m && p.spec.cadence_days == k)
            .map(|p| &p.measured)
    };
    for &k in cadences {
        let (Some(h), Some(d)) = (get(ModelKind::Herd, k), get(ModelKind::Dyadic, k)) else {
            continue;
        };
        let f = |x: Option<f64>| x.map_or("-".into(), |v| format!("{v:.1}"));
        println!(
            "{:>8} | {:>13} {:>13} {:>11} | {:>12} {:>12} | {:>7.1}",
            k,
            f(h.cpu_ns_per_region_day),
            f(d.cpu_ns_per_region_day),
            match (h.cpu_ns_per_region_day, d.cpu_ns_per_region_day) {
                (Some(a), Some(b)) => format!("{:.2}", a / b),
                _ => "-".into(),
            },
            f(h.ns_per_read),
            f(d.ns_per_read),
            h.slot_commits as f64 / h.reads.max(1) as f64,
        );
    }
}
