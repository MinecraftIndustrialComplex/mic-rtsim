// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared pieces of the P4 storage bench (`benches/p4_storage.rs`): the counter RNG both backends
//! draw from, a counting allocator for heap bytes, the timing harness, a description of the machine
//! and the results files. Nothing here knows about bevy_ecs, which stays a dev-dependency of the
//! bench target alone.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::Serialize;

/// SplitMix64: a counter RNG with a fixed seed, so every backend sees the same rows, the same
/// handles in the same order and the same churn.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In `0..n`, by multiply and shift. The tiny bias is the same for every backend.
    pub fn below(&mut self, n: usize) -> usize {
        ((u128::from(self.next_u64()) * n as u128) >> 64) as usize
    }
}

/// Counts live heap bytes. The bench binary installs it as the global allocator; every backend pays
/// the same relaxed atomic add per allocation.
pub struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's layout.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's layout.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded; `ptr` came from this allocator with this layout.
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded; `ptr` came from this allocator with this layout.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

/// Live heap bytes through [`Counting`]. Zero unless the binary installed it.
pub fn heap_live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// What to run, from the environment, so the same binary serves the full run, the pinned run and a
/// quick smoke test.
///
/// - `P4_TARGET_MS`: least wall time of one measured run (default 200).
/// - `P4_RUNS`: measured runs per variant; the median is kept (default 7).
/// - `P4_SIZES`: row counts, comma separated (default `100000,1000000`).
/// - `P4_PROFILE`: a label for the results file, such as `full` or `pinned2` (default `full`).
/// - `P4_WRITE`: `0` prints only and writes no files (default `1`).
/// - `P4_ONLY`: pattern ids to run, comma separated, such as `a,b,g` (default all).
pub struct Config {
    pub target: Duration,
    pub runs: usize,
    pub sizes: Vec<usize>,
    pub profile: String,
    pub write: bool,
    pub only: Option<Vec<String>>,
}

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let target_ms: u64 = var("P4_TARGET_MS").map_or(200, |v| v.parse().expect("P4_TARGET_MS"));
        let runs: usize = var("P4_RUNS").map_or(7, |v| v.parse().expect("P4_RUNS"));
        let sizes = var("P4_SIZES").map_or_else(
            || vec![100_000, 1_000_000],
            |v| {
                v.split(',')
                    .map(|s| s.trim().parse().expect("P4_SIZES"))
                    .collect()
            },
        );
        Config {
            target: Duration::from_millis(target_ms),
            runs: runs.max(1),
            sizes,
            profile: var("P4_PROFILE").unwrap_or_else(|| "full".to_string()),
            write: var("P4_WRITE").is_none_or(|v| v != "0"),
            only: var("P4_ONLY").map(|v| v.split(',').map(|s| s.trim().to_string()).collect()),
        }
    }

    pub fn wants(&self, pattern: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|o| o.iter().any(|p| p == pattern))
    }
}

/// One way of doing one pattern. `batch(k)` runs `k` steps and returns the time they took (setup
/// and teardown excluded).
pub struct Variant<'a> {
    pub name: String,
    pub backend: &'static str,
    pub unit: &'static str,
    /// Operations in one step: rows streamed, lookups made, rows inserted.
    pub ops_per_step: f64,
    pub note: String,
    pub batch: Box<dyn FnMut(u64) -> Duration + 'a>,
}

impl<'a> Variant<'a> {
    pub fn new(
        name: &str,
        backend: &'static str,
        unit: &'static str,
        ops_per_step: f64,
        note: &str,
        batch: impl FnMut(u64) -> Duration + 'a,
    ) -> Self {
        Variant {
            name: name.to_string(),
            backend,
            unit,
            ops_per_step,
            note: note.to_string(),
            batch: Box::new(batch),
        }
    }
}

/// Times `k` calls of `step`.
pub fn timed(mut step: impl FnMut()) -> impl FnMut(u64) -> Duration {
    move |k| {
        let t0 = std::time::Instant::now();
        for _ in 0..k {
            step();
        }
        t0.elapsed()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Record {
    pub pattern: String,
    pub title: String,
    pub variant: String,
    pub backend: String,
    pub n: usize,
    pub unit: String,
    pub ops_per_step: f64,
    pub steps_per_run: u64,
    pub runs: usize,
    pub median_ns_per_op: f64,
    pub min_ns_per_op: f64,
    pub max_ns_per_op: f64,
    pub runs_ns_per_op: Vec<f64>,
    /// Median wall time of one step, in microseconds.
    pub median_us_per_step: f64,
    /// Median wall time of one measured run, in milliseconds.
    pub median_ms_per_run: f64,
    pub note: String,
}

fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let m = s.len() / 2;
    if s.len() % 2 == 1 {
        s[m]
    } else {
        (s[m - 1] + s[m]) / 2.0
    }
}

/// Calibrates each variant to at least `cfg.target` per run, then measures `cfg.runs` runs of every
/// variant, interleaved (run 1 of each variant, then run 2 of each, ...) so that drift in clock
/// speed or machine load spreads over all variants alike. Calibration doubles as the warm-up.
pub fn run_group(
    cfg: &Config,
    pattern: &str,
    title: &str,
    n: usize,
    mut variants: Vec<Variant<'_>>,
) -> Vec<Record> {
    let floor = cfg.target / 10;
    let mut steps = Vec::with_capacity(variants.len());
    for v in &mut variants {
        let mut k: u64 = 1;
        loop {
            let t = (v.batch)(k);
            if t >= floor || k >= 1 << 40 {
                let scale = cfg.target.as_secs_f64() * 1.1 / t.as_secs_f64().max(1e-9);
                k = ((k as f64) * scale).ceil().max(1.0) as u64;
                break;
            }
            k *= 2;
        }
        steps.push(k);
    }
    let mut per_op: Vec<Vec<f64>> = vec![Vec::with_capacity(cfg.runs); variants.len()];
    let mut per_run_ms: Vec<Vec<f64>> = vec![Vec::with_capacity(cfg.runs); variants.len()];
    for _ in 0..cfg.runs {
        for (i, v) in variants.iter_mut().enumerate() {
            let t = (v.batch)(steps[i]).as_nanos() as f64;
            per_op[i].push(t / (steps[i] as f64 * v.ops_per_step));
            per_run_ms[i].push(t / 1e6);
        }
    }
    let mut out = Vec::with_capacity(variants.len());
    for (i, v) in variants.iter().enumerate() {
        let med = median(&per_op[i]);
        let rec = Record {
            pattern: pattern.to_string(),
            title: title.to_string(),
            variant: v.name.clone(),
            backend: v.backend.to_string(),
            n,
            unit: v.unit.to_string(),
            ops_per_step: v.ops_per_step,
            steps_per_run: steps[i],
            runs: cfg.runs,
            median_ns_per_op: med,
            min_ns_per_op: per_op[i].iter().copied().fold(f64::MAX, f64::min),
            max_ns_per_op: per_op[i].iter().copied().fold(f64::MIN, f64::max),
            runs_ns_per_op: per_op[i].clone(),
            median_us_per_step: med * v.ops_per_step / 1e3,
            median_ms_per_run: median(&per_run_ms[i]),
            note: v.note.clone(),
        };
        println!(
            "  {pattern} n={n:<8} {:<28} {:>10.3} {:<11} (min {:.3}, max {:.3}, {} steps/run)",
            rec.variant,
            rec.median_ns_per_op,
            rec.unit,
            rec.min_ns_per_op,
            rec.max_ns_per_op,
            steps[i]
        );
        out.push(rec);
    }
    out
}

/// Heap bytes of one store holding `n` rows.
#[derive(Clone, Debug, Serialize)]
pub struct MemRecord {
    pub kind: String,
    pub variant: String,
    pub backend: String,
    pub n: usize,
    /// The sum of the component sizes: the floor any layout must pay.
    pub payload_bytes_per_row: usize,
    /// Live heap after building minus live heap before creating the store.
    pub heap_delta_bytes: usize,
    pub bytes_per_row: f64,
    /// Live heap of the store with no rows, already counted in `heap_delta_bytes`.
    pub empty_store_bytes: usize,
    /// `(heap_delta_bytes - empty_store_bytes) / n`.
    pub marginal_bytes_per_row: f64,
    /// What the store itself reports (`ColumnTable::heap_bytes`), when it can.
    pub reported_heap_bytes: Option<usize>,
    pub note: String,
}

/// The machine and its state, for the results file.
#[derive(Clone, Debug, Serialize)]
pub struct Machine {
    pub cpu: String,
    /// Logical CPUs in the machine, from /proc/cpuinfo (not the affinity mask).
    pub logical_cpus: usize,
    pub mem_total_bytes: Option<u64>,
    pub kernel: Option<String>,
    pub hostname: Option<String>,
}

fn read_trim(p: &str) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

impl Machine {
    pub fn probe() -> Machine {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let cpu = cpuinfo
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .map(|r| r.trim_start_matches([' ', '\t', ':']).to_string())
            })
            .unwrap_or_else(|| std::env::consts::ARCH.to_string());
        let logical_cpus = cpuinfo
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count();
        let mem_total_bytes = std::fs::read_to_string("/proc/meminfo").ok().and_then(|m| {
            m.lines().find_map(|l| {
                let kb: u64 = l
                    .strip_prefix("MemTotal:")?
                    .trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse()
                    .ok()?;
                Some(kb * 1024)
            })
        });
        Machine {
            cpu,
            logical_cpus,
            mem_total_bytes,
            kernel: read_trim("/proc/sys/kernel/osrelease"),
            hostname: read_trim("/proc/sys/kernel/hostname"),
        }
    }
}

pub fn loadavg() -> Option<String> {
    read_trim("/proc/loadavg")
}

/// The nice value: the 19th field of /proc/self/stat.
pub fn nice_level() -> Option<i32> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(16)?.parse().ok()
}

/// The CPUs this process may run on, as the kernel prints them (`0-23`, `4,6`).
pub fn cpus_allowed() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|l| {
        l.strip_prefix("Cpus_allowed_list:")
            .map(|r| r.trim().to_string())
    })
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn git_commit() -> Option<String> {
    git(&["rev-parse", "--short=12", "HEAD"])
}

/// Whether tracked files differ from the commit.
pub fn git_dirty() -> Option<bool> {
    git(&["status", "--porcelain", "--untracked-files=no"]).map(|s| !s.is_empty())
}

pub const RUSTC_VERSION: &str = env!("P4_RUSTC_VERSION");
pub const BEVY_ECS_VERSION: &str = env!("P4_BEVY_ECS_VERSION");

/// Days since 1970-01-01 to (year, month, day): Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// (ISO 8601 UTC time, `YYYY-MM-DD-HHMM` for file names).
pub fn now_utc() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    (
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            s / 3600,
            s / 60 % 60,
            s % 60
        ),
        format!("{y:04}-{m:02}-{d:02}-{:02}{:02}", s / 3600, s / 60 % 60),
    )
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub bench: String,
    pub date_utc: String,
    pub profile: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub rustc: String,
    pub bevy_ecs: String,
    pub bevy_ecs_features: String,
    pub build: String,
    pub debug_assertions: bool,
    pub machine: Machine,
    pub cpus_allowed: Option<String>,
    pub nice: Option<i32>,
    pub loadavg_before: Option<String>,
    pub loadavg_after: Option<String>,
    pub target_ms_per_run: u128,
    pub runs: usize,
    pub sizes: Vec<usize>,
    pub elapsed_s: f64,
    pub results: Vec<Record>,
    pub memory: Vec<MemRecord>,
    pub notes: Vec<String>,
}

/// Where results go: `bench/results/` of this repository.
pub fn results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../results")
}

fn fmt_ns(x: f64) -> String {
    if x >= 100.0 {
        format!("{x:.1}")
    } else if x >= 10.0 {
        format!("{x:.2}")
    } else {
        format!("{x:.3}")
    }
}

/// The printed table: one block per pattern, one line per variant, one column per row count with
/// the median, the spread of the runs ((max - min) / median) and the ratio to the block's first line.
pub fn table(report: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "P4 storage bench, profile {}, {}, commit {}{}",
        report.profile,
        report.date_utc,
        report.git_commit.as_deref().unwrap_or("?"),
        if report.git_dirty == Some(true) {
            " (dirty)"
        } else {
            ""
        }
    );
    let _ = writeln!(
        s,
        "{}; bevy_ecs {} (features: {}); {}",
        report.rustc, report.bevy_ecs, report.bevy_ecs_features, report.build
    );
    let _ = writeln!(
        s,
        "{}, {} logical CPUs, allowed {}, nice {}, load before {} / after {}",
        report.machine.cpu,
        report.machine.logical_cpus,
        report.cpus_allowed.as_deref().unwrap_or("?"),
        report.nice.map_or("?".to_string(), |n| n.to_string()),
        report.loadavg_before.as_deref().unwrap_or("?"),
        report.loadavg_after.as_deref().unwrap_or("?"),
    );
    let _ = writeln!(
        s,
        "Median of {} runs, each at least {} ms; spread = (max - min) / median; x = ratio to the block's first line. {:.0} s in all.",
        report.runs, report.target_ms_per_run, report.elapsed_s
    );
    let sizes = &report.sizes;
    let mut patterns: Vec<(&str, &str)> = Vec::new();
    for r in &report.results {
        if !patterns.iter().any(|(p, _)| *p == r.pattern) {
            patterns.push((&r.pattern, &r.title));
        }
    }
    for (p, title) in patterns {
        let _ = writeln!(s);
        let _ = write!(s, "{p:<3} {title}");
        let _ = writeln!(s);
        let _ = write!(s, "    {:<30} {:<12}", "variant", "unit");
        for n in sizes {
            let _ = write!(s, " {:>26}", format!("n = {n}"));
        }
        let _ = writeln!(s);
        let mut variants: Vec<&str> = Vec::new();
        for r in report.results.iter().filter(|r| r.pattern == p) {
            if !variants.contains(&r.variant.as_str()) {
                variants.push(&r.variant);
            }
        }
        for v in &variants {
            let unit = report
                .results
                .iter()
                .find(|r| r.pattern == p && r.variant == *v)
                .map_or("", |r| r.unit.as_str());
            let _ = write!(s, "    {v:<30} {unit:<12}");
            for n in sizes {
                let first = report
                    .results
                    .iter()
                    .find(|r| r.pattern == p && r.n == *n)
                    .map(|r| r.median_ns_per_op);
                let cell = report
                    .results
                    .iter()
                    .find(|r| r.pattern == p && r.variant == *v && r.n == *n)
                    .map_or_else(
                        || "-".to_string(),
                        |r| {
                            let spread =
                                100.0 * (r.max_ns_per_op - r.min_ns_per_op) / r.median_ns_per_op;
                            let x = first.map_or(f64::NAN, |f| r.median_ns_per_op / f);
                            format!(
                                "{} ±{:.0}% {:.2}x",
                                fmt_ns(r.median_ns_per_op),
                                spread / 2.0,
                                x
                            )
                        },
                    );
                let _ = write!(s, " {cell:>26}");
            }
            let _ = writeln!(s);
        }
    }
    if !report.memory.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "g   memory: live heap after building minus before creating the store (counting allocator)"
        );
        let _ = writeln!(
            s,
            "    {:<8} {:<22} {:>8} {:>8} {:>14} {:>9} {:>9} {:>12} {:>14}",
            "kind",
            "variant",
            "n",
            "payload",
            "heap bytes",
            "B/row",
            "marginal",
            "empty store",
            "heap_bytes()"
        );
        for m in &report.memory {
            let _ = writeln!(
                s,
                "    {:<8} {:<22} {:>8} {:>8} {:>14} {:>9.1} {:>9.1} {:>12} {:>14}",
                m.kind,
                m.variant,
                m.n,
                m.payload_bytes_per_row,
                m.heap_delta_bytes,
                m.bytes_per_row,
                m.marginal_bytes_per_row,
                m.empty_store_bytes,
                m.reported_heap_bytes
                    .map_or("-".to_string(), |b| b.to_string())
            );
        }
    }
    if !report.notes.is_empty() {
        let _ = writeln!(s);
        let _ = writeln!(s, "Notes:");
        for n in &report.notes {
            let _ = writeln!(s, "- {n}");
        }
    }
    s
}
