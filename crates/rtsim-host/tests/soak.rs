// SPDX-License-Identifier: AGPL-3.0-or-later
//! `who_is_alive_soak` (EQUILIBRIUM.md §4's S11 row; docs/overnight-choices-equilibrium.md EQC433
//! to EQC447): the every-kind world with nobody, 262,980 game days in the host at max, every
//! kind's census every 10 game days and the state's bytes, heap and queues at each century's end.
//! It passes only if every kind is present and alive on every sample, every judged century sits
//! in its kind's band, and no series drifts both materially and significantly.
//!
//! Its own binary, so the heap gauge (the whole process's) sees the soak alone; run one test at a
//! time with `--exact`. `RTSIM_SOAK_DAYS`, `RTSIM_SOAK_SEED`, `RTSIM_SOAK_REPORT` (a JSON file),
//! `RTSIM_SOAK_WALL_HOURS` and `RTSIM_SOAK_COMMIT` (named in the report) shape a run;
//! `tools/nightly/soak.sh` sets them.
//!
//! `who_is_alive_soak_short` runs the same code on 1,000 game days with windows of 100 days in
//! place of centuries (EQC446): it proves the code, nothing about centuries.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rtsim_host::every::{self, Every, Founded, KINDS};
use rtsim_host::java::Policy;
use rtsim_kernel::census::{CensusLog, KindCensus, trend};
use rtsim_kernel::page::{Durability, PageStore};
use serde_json::{Value, json};

/// A century of game days (EQC435).
const CENTURY: u64 = 9_600;
/// Game days between samples.
const SAMPLE: u64 = 10;
/// The flat rule's significance (EQC438).
const ALPHA: f64 = 0.001;

/// One run's shape.
#[derive(Clone, Debug)]
struct Plan {
    days: u64,
    /// Game days a window (a century, or 100 for the short cut).
    window: u64,
    /// The judged windows, 1-based (2 to 27; 2 to 10 for the short cut).
    judged: std::ops::RangeInclusive<usize>,
    seed: u64,
    wall: Duration,
    report: Option<PathBuf>,
    commit: String,
    short: bool,
}

fn env<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

/// A kind's measures over one window (EQC435).
#[derive(Clone, Debug, Default)]
struct KindWindow {
    samples: u64,
    fill: f64,
    empty: f64,
    broken: f64,
    p: f64,
    min_sites: u32,
    max_sites: u32,
    /// The fewest members on a sample, and the capacity on the last.
    min_members: u64,
    capacity: u64,
    renewals_at_start: u64,
    renewals_at_end: u64,
    /// Samples on which the kind was not exactly full (every site at its K, none broken).
    not_full: u64,
    /// Samples on which some site was broken, and the largest mean P seen.
    broken_samples: u64,
    max_p: f64,
}

impl KindWindow {
    fn add(&mut self, c: &KindCensus) {
        if self.samples == 0 {
            self.min_sites = c.sites;
            self.max_sites = c.sites;
            self.min_members = c.members;
            self.renewals_at_start = c.renewals;
        }
        self.samples += 1;
        self.fill += c.fill();
        let sites = f64::from(c.sites.max(1));
        self.empty += f64::from(c.empty) / sites;
        self.broken += f64::from(c.broken) / sites;
        self.p += c.mean_p();
        self.min_sites = self.min_sites.min(c.sites);
        self.max_sites = self.max_sites.max(c.sites);
        self.min_members = self.min_members.min(c.members);
        self.capacity = c.capacity;
        self.renewals_at_end = c.renewals;
        if c.members != c.capacity || c.broken > 0 {
            self.not_full += 1;
        }
        if c.broken > 0 {
            self.broken_samples += 1;
        }
        self.max_p = self.max_p.max(c.mean_p());
    }

    fn mean(&self, x: f64) -> f64 {
        x / self.samples.max(1) as f64
    }
}

/// One window's close: the kinds' measures and the state at its end.
#[derive(Clone, Debug, Default)]
struct Window {
    index: usize,
    from: u64,
    to: u64,
    kinds: BTreeMap<&'static str, KindWindow>,
    /// Bytes by page table, the heap, the wheels, the queues and the archive's bytes.
    tables: BTreeMap<u16, u64>,
    heap: u64,
    wheels: BTreeMap<&'static str, u64>,
    queues: BTreeMap<&'static str, u64>,
    archive: u64,
    /// Game days a second over the window.
    rate: f64,
}

impl Window {
    fn renewals(&self, kind: &str) -> u64 {
        self.kinds
            .get(kind)
            .map_or(0, |k| k.renewals_at_end.saturating_sub(k.renewals_at_start))
    }

    fn days(&self) -> f64 {
        (self.to - self.from) as f64
    }
}

/// What a whole run kept.
#[derive(Clone, Debug, Default)]
struct Run {
    reached: u64,
    wall: Duration,
    windows: Vec<Window>,
    /// Sample checks that failed: (day, what).
    sample_failures: Vec<(u64, String)>,
    peak_journal: usize,
    peak_in_flight: u64,
    /// The industry outbox and the core's pages (table 1) at each sample: (day, industry, core).
    outboxes: Vec<(u64, u64, u64)>,
    incomplete: Option<String>,
}

/// Every page table's bytes as last confirmed.
fn table_bytes(pages: &Path) -> BTreeMap<u16, u64> {
    let mut s = PageStore::open_read_only(pages).expect("the fork's pages");
    s.view_confirmed().expect("a confirmed view");
    let mut out = BTreeMap::new();
    for k in s.try_keys().expect("the pages' keys") {
        let n = s.get_shared(k).ok().flatten().map_or(0, |b| b.len() as u64);
        *out.entry(k.table()).or_default() += n;
    }
    out
}

fn dir_bytes(d: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(d) else {
        return 0;
    };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                dir_bytes(&p)
            } else {
                e.metadata().map_or(0, |m| m.len())
            }
        })
        .sum()
}

/// The checks on one sample's census (EQC436 (2) and (3)).
fn check_sample(f: &Founded, day: u64, plan: &Plan, cs: &[KindCensus], out: &mut Vec<(u64, String)>) {
    for k in KINDS {
        let Some(c) = every::kind(cs, k) else {
            out.push((day, format!("{k}: missing")));
            continue;
        };
        if c.sites != f.sites(k) {
            out.push((day, format!("{k}: {} sites, the world has {}", c.sites, f.sites(k))));
        }
        if c.capacity == 0 {
            out.push((day, format!("{k}: capacity 0")));
        }
        if day > plan.window && c.members == 0 {
            out.push((day, format!("{k}: no member")));
        }
        if k == "herd.range" && c.empty > 0 {
            out.push((day, format!("herd.range: {} ranges empty", c.empty)));
        }
    }
    if cs.len() != KINDS.len() {
        out.push((day, format!("{} kinds, expected {}", cs.len(), KINDS.len())));
    }
}

/// Runs the soak (or its short cut) as `plan` says.
fn soak(plan: &Plan) -> (Run, Founded) {
    let mut w = Every::open_with(
        if plan.short { "soak-short" } else { "soak" },
        plan.seed,
        true,
        Durability::Fsync,
        Policy::Refuse,
    );
    let founded = w.founded.clone();
    let pages = w.pages();
    let t0 = Instant::now();
    let mut run = Run::default();
    let mut win = Window {
        index: 1,
        from: 0,
        ..Window::default()
    };
    let mut win_t0 = Instant::now();
    let mut log = CensusLog::new();
    println!(
        "who_is_alive_soak: seed {}, {} game days, windows of {} days",
        plan.seed, plan.days, plan.window
    );
    while w.day() < plan.days {
        let next = (w.day() + SAMPLE).min(plan.days);
        w.run_to(next);
        let cs = w.census();
        check_sample(&founded, next, plan, &cs, &mut run.sample_failures);
        for c in &cs {
            if let Some(k) = KINDS.iter().find(|k| **k == c.kind) {
                win.kinds.entry(k).or_default().add(c);
            }
        }
        run.peak_journal = run.peak_journal.max(w.host.j.journal.len());
        run.peak_in_flight = run.peak_in_flight.max(w.host.in_flight());
        let industry = w.engine().with_industry(|_, s| s.outbox + s.queued);
        if next % (plan.window / 10).max(SAMPLE) == 0 || next == plan.days {
            let core = table_bytes(&pages).get(&1).copied().unwrap_or(0);
            run.outboxes.push((next, industry, core));
        }
        let closes = next % plan.window == 0 || next == plan.days;
        if closes {
            win.to = next;
            win.tables = table_bytes(&pages);
            win.heap = mic_native_rt::memory::in_use().max(0) as u64;
            win.wheels = w
                .engine()
                .wheels()
                .into_iter()
                .map(|(k, n)| (k, n as u64))
                .collect();
            win.queues = w
                .engine()
                .queues()
                .into_iter()
                .map(|(k, n)| (k, n as u64))
                .collect();
            win.archive = dir_bytes(&pages.join("archive"));
            win.rate = (win.to - win.from) as f64 / win_t0.elapsed().as_secs_f64().max(1e-9);
            let fills: Vec<String> = win
                .kinds
                .iter()
                .map(|(k, x)| format!("{k} {:.3}", x.mean(x.fill)))
                .collect();
            println!(
                "window {} (days {} to {}): {:.1} game days a second, {:.1} overall; pages {} B, \
                 heap {} KiB; {}",
                win.index,
                win.from,
                win.to,
                win.rate,
                next as f64 / t0.elapsed().as_secs_f64().max(1e-9),
                win.tables.values().sum::<u64>(),
                win.heap / 1024,
                fills.join(", ")
            );
            println!("{}", CensusLog::HEADER);
            for l in log.lines(next, &cs) {
                println!("{l}");
            }
            let index = win.index;
            run.windows.push(std::mem::take(&mut win));
            win = Window {
                index: index + 1,
                from: next,
                ..Window::default()
            };
            win_t0 = Instant::now();
            if t0.elapsed() > plan.wall && next < plan.days {
                run.incomplete = Some(format!(
                    "the wall cap of {:.1} hours at day {next}",
                    plan.wall.as_secs_f64() / 3600.0
                ));
                break;
            }
        }
    }
    run.reached = w.day();
    run.wall = t0.elapsed();
    w.close();
    (run, founded)
}

// ---- the judgement ------------------------------------------------------------------------------

/// One check's result for the report.
#[derive(Clone, Debug)]
struct Check {
    name: String,
    ok: bool,
    detail: Value,
}

/// A kind's level band over a judged window's mean fill (EQC419, provisional until the calibration,
/// EQC442): (low, high), and whether it is exact (every sample exactly full).
fn level_band(kind: &str) -> Option<(f64, f64, bool)> {
    Some(match kind {
        "herd.range" => (0.70, 0.84, false),
        "dragon.lair" => (0.81, 0.98, false),
        "oil.nest" | "conflict.den" | "conflict.umvuthi" => (1.0, 1.0, true),
        "village.human" | "village.ribbit" => (0.95, 1.0, false),
        "conflict.grove" => (0.90, 1.0, false),
        "ship.haven" => (0.83, 1.0, false),
        _ => return None,
    })
}

/// A kind's material drift of its fill over the judged span (EQC420 (1)).
fn fill_delta(kind: &str, capacity: f64) -> f64 {
    match kind {
        "herd.range" => 0.02,
        "dragon.lair" => 0.035,
        "conflict.grove" => 0.012,
        "ship.haven" => 0.078,
        _ => 1.0 / capacity.max(1.0),
    }
}

/// The renewal classes (EQC436 (4)), provisional until the calibration: kinds that renew in every
/// judged window, and kinds that renew nothing with no player.
const RENEWING: [&str; 3] = ["dragon.lair", "herd.range", "conflict.grove"];
const NEVER: [&str; 6] = [
    "conflict.den",
    "village.human",
    "village.ribbit",
    "oil.nest",
    "conflict.camp",
    "conflict.umvuthi",
];

fn median(xs: &[f64]) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        0.0
    } else if v.len() % 2 == 1 {
        v[v.len() / 2]
    } else {
        (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
    }
}

/// A series under the flat rule (EQC438) with drift threshold `delta`.
fn flat_check(name: String, xs: &[f64], delta: f64, out: &mut Vec<Check>) {
    match trend::flat(xs, delta, ALPHA) {
        Ok(f) => out.push(Check {
            name,
            ok: f.ok,
            detail: json!({"drift": f.drift, "p": f.p, "s": f.s, "delta": delta,
                           "margin": f.drift.abs() <= delta / 2.0, "values": xs}),
        }),
        Err(e) => out.push(Check {
            name,
            ok: false,
            detail: json!({"error": e.to_string()}),
        }),
    }
}

/// The byte cap (EQC439): every window's end at most 1.25 times the larger of windows 2 and 3,
/// plus `floor`.
fn cap_check(name: String, xs: &[f64], floor: f64, out: &mut Vec<Check>) {
    if xs.len() < 2 {
        return;
    }
    let base = xs[0].max(xs[1]);
    let worst = xs.iter().copied().fold(0.0, f64::max);
    out.push(Check {
        name,
        ok: worst <= 1.25 * base + floor,
        detail: json!({"base": base, "worst": worst, "floor": floor}),
    });
}

/// Every check of EQC436 to EQC441 on a finished run.
fn judge(plan: &Plan, run: &Run, f: &Founded) -> Vec<Check> {
    let mut out = Vec::new();
    // (1) The last day reached.
    out.push(Check {
        name: "reached".into(),
        ok: run.incomplete.is_none() && run.reached >= plan.days,
        detail: json!({"reached": run.reached, "days": plan.days}),
    });
    // (2), (3) Every kind on every sample, alive.
    out.push(Check {
        name: "present_and_alive".into(),
        ok: run.sample_failures.is_empty(),
        detail: json!(run.sample_failures.iter().take(40).map(|(d, s)| format!("day {d}: {s}")).collect::<Vec<_>>()),
    });
    let judged: Vec<&Window> = run
        .windows
        .iter()
        .filter(|w| plan.judged.contains(&w.index) && w.to - w.from == plan.window)
        .collect();
    if judged.len() < 3 {
        out.push(Check {
            name: "judged_windows".into(),
            ok: false,
            detail: json!({"judged": judged.len()}),
        });
        return out;
    }
    for k in KINDS {
        let fills: Vec<f64> = judged
            .iter()
            .map(|w| w.kinds.get(k).map_or(0.0, |x| x.mean(x.fill)))
            .collect();
        // Levels (EQC437); the short cut holds only the exact ones (EQC446).
        if let Some((lo, hi, exact)) = level_band(k).filter(|b| b.2 || !plan.short) {
            let bad: Vec<(usize, f64)> = judged
                .iter()
                .zip(&fills)
                .filter(|(w, x)| {
                    let k = w.kinds.get(k);
                    **x < lo - 1e-9 || **x > hi + 1e-9 || (exact && k.is_some_and(|k| k.not_full > 0))
                })
                .map(|(w, x)| (w.index, *x))
                .collect();
            out.push(Check {
                name: format!("level.{k}"),
                ok: bad.is_empty(),
                detail: json!({"band": [lo, hi], "exact": exact, "outside": bad, "values": fills}),
            });
        }
        // Flat (EQC438, EQC420 (1)).
        let capacity = judged
            .last()
            .and_then(|w| w.kinds.get(k))
            .map_or(1, |x| x.capacity);
        flat_check(
            format!("flat.fill.{k}"),
            &fills,
            fill_delta(k, capacity as f64),
            &mut out,
        );
        // Renewals (EQC436 (4), EQC440).
        let counts: Vec<f64> = judged.iter().map(|w| w.renewals(k) as f64).collect();
        if NEVER.contains(&k) {
            out.push(Check {
                name: format!("renewals.none.{k}"),
                ok: counts.iter().all(|&n| n == 0.0),
                detail: json!({"counts": counts}),
            });
        } else if RENEWING.contains(&k) && !plan.short {
            out.push(Check {
                name: format!("renewals.every_window.{k}"),
                ok: counts.iter().all(|&n| n > 0.0),
                detail: json!({"counts": counts}),
            });
        }
        let rates: Vec<f64> = judged
            .iter()
            .zip(&counts)
            .map(|(w, n)| n / w.days())
            .collect();
        let n = median(&counts);
        let delta = (0.1 * median(&rates)).max(3.0 * n.sqrt() / plan.window as f64);
        if !NEVER.contains(&k) {
            flat_check(format!("flat.renewals.{k}"), &rates, delta.max(1e-9), &mut out);
        }
    }
    // The pool camps: no break, no P, on every sample (EQC419's exact rows).
    let camp_bad: Vec<usize> = judged
        .iter()
        .filter(|w| {
            w.kinds
                .get("conflict.camp")
                .is_some_and(|k| k.broken_samples > 0 || k.max_p > 0.0)
        })
        .map(|w| w.index)
        .collect();
    out.push(Check {
        name: "level.conflict.camp.unbroken".into(),
        ok: camp_bad.is_empty(),
        detail: json!({"windows": camp_bad}),
    });
    // Ports: never every port closed at once; closed at most 0.01 of port-days.
    let port: Vec<f64> = judged
        .iter()
        .map(|w| w.kinds.get("ship.port").map_or(0.0, |k| k.mean(k.empty)))
        .collect();
    let all_closed: Vec<usize> = run
        .windows
        .iter()
        .filter(|w| w.kinds.get("ship.port").is_some_and(|k| k.min_members == 0))
        .map(|w| w.index)
        .collect();
    out.push(Check {
        name: "level.ship.port".into(),
        ok: port.iter().all(|&x| x <= 0.01) && all_closed.is_empty(),
        detail: json!({"closed_share": port, "every_port_closed_in": all_closed}),
    });
    // The whole world's renewals a day (EQC440).
    let world: Vec<f64> = judged
        .iter()
        .map(|w| KINDS.iter().map(|k| w.renewals(k)).sum::<u64>() as f64 / w.days())
        .collect();
    let n = median(&world) * plan.window as f64;
    let delta = (0.1 * median(&world)).max(3.0 * n.sqrt() / plan.window as f64);
    flat_check("flat.renewals.world".into(), &world, delta.max(1e-9), &mut out);
    // State bytes (EQC439): every table, their total and the heap.
    let tables: Vec<u16> = judged
        .iter()
        .flat_map(|w| w.tables.keys().copied())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    // (5) Every table the first judged window ends with is there, above 0 bytes, at every end.
    let first: Vec<u16> = judged[0].tables.keys().copied().collect();
    let zero: Vec<String> = judged
        .iter()
        .flat_map(|w| {
            first
                .iter()
                .filter(|t| w.tables.get(t).copied().unwrap_or(0) == 0)
                .map(move |t| format!("window {}: table {t:#06x}", w.index))
        })
        .collect();
    out.push(Check {
        name: "pages.nonzero".into(),
        ok: zero.is_empty(),
        detail: json!(zero),
    });
    for t in tables {
        let xs: Vec<f64> = judged
            .iter()
            .map(|w| w.tables.get(&t).copied().unwrap_or(0) as f64)
            .collect();
        let delta = (0.1 * median(&xs)).max(16.0 * 1024.0);
        flat_check(format!("flat.bytes.{t:#06x}"), &xs, delta, &mut out);
        cap_check(format!("cap.bytes.{t:#06x}"), &xs, 16.0 * 1024.0, &mut out);
    }
    let total: Vec<f64> = judged
        .iter()
        .map(|w| w.tables.values().sum::<u64>() as f64)
        .collect();
    flat_check(
        "flat.bytes.total".into(),
        &total,
        (0.1 * median(&total)).max(16.0 * 1024.0),
        &mut out,
    );
    // The event archive grows by design: held to its budget (EQC439, UNVERIFIED 13), its index and
    // dictionary allowed 1 MiB over the chunks' total.
    let budget = rtsim_archive::store::Budget::default().total + (1 << 20);
    let over: Vec<(usize, u64)> = run
        .windows
        .iter()
        .filter(|w| w.archive > budget)
        .map(|w| (w.index, w.archive))
        .collect();
    out.push(Check {
        name: "cap.archive".into(),
        ok: over.is_empty(),
        detail: json!({"budget": budget, "over": over,
                       "values": judged.iter().map(|w| w.archive).collect::<Vec<_>>()}),
    });
    let heap: Vec<f64> = judged.iter().map(|w| w.heap as f64).collect();
    flat_check(
        "flat.heap".into(),
        &heap,
        (0.1 * median(&heap)).max(1024.0 * 1024.0),
        &mut out,
    );
    cap_check("cap.heap".into(), &heap, 1024.0 * 1024.0, &mut out);
    // Queues (EQC441): the wheels and the modules' outboxes as series, the journal and the
    // commands in flight bounded, the industry and core outboxes bounded.
    let names: Vec<&'static str> = judged
        .iter()
        .flat_map(|w| w.wheels.keys().chain(w.queues.keys()).copied())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for q in names {
        let xs: Vec<f64> = judged
            .iter()
            .map(|w| *w.wheels.get(q).or(w.queues.get(q)).unwrap_or(&0) as f64)
            .collect();
        flat_check(
            format!("flat.queue.{q}"),
            &xs,
            (0.1 * median(&xs)).max(64.0),
            &mut out,
        );
    }
    out.push(Check {
        name: "queues.journal_and_in_flight".into(),
        ok: run.peak_journal < 2_000 && run.peak_in_flight < 1_000,
        detail: json!({"peak_journal": run.peak_journal, "peak_in_flight": run.peak_in_flight}),
    });
    let half = run.outboxes.len() / 2;
    if half > 0 {
        let early = |f: fn(&(u64, u64, u64)) -> u64| run.outboxes[..half].iter().map(f).max().unwrap_or(0);
        let late = |f: fn(&(u64, u64, u64)) -> u64| run.outboxes[half..].iter().map(f).max().unwrap_or(0);
        let (ie, il) = (early(|x| x.1), late(|x| x.1));
        let (ce, cl) = (early(|x| x.2), late(|x| x.2));
        out.push(Check {
            name: "queues.outboxes_bounded".into(),
            ok: il <= 2 * ie + 64 && cl <= 2 * ce + (64 << 10),
            detail: json!({"industry": [ie, il], "core": [ce, cl]}),
        });
    }
    out
}

/// The report (EQC445): written to `plan.report` if set; returns the verdict and failing checks.
fn report(plan: &Plan, run: &Run, checks: &[Check]) -> (String, Vec<String>) {
    let failing: Vec<String> = checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.name.clone())
        .collect();
    let verdict = if run.incomplete.is_some() || run.reached < plan.days {
        "INCOMPLETE"
    } else if failing.is_empty() {
        "PASS"
    } else {
        "FAIL"
    };
    let windows: Vec<Value> = run
        .windows
        .iter()
        .map(|w| {
            let kinds: BTreeMap<&str, Value> = w
                .kinds
                .iter()
                .map(|(k, x)| {
                    (
                        *k,
                        json!({
                            "fill": x.mean(x.fill), "empty": x.mean(x.empty),
                            "broken": x.mean(x.broken), "p": x.mean(x.p),
                            "renewals": w.renewals(k),
                            "renewals_a_day": w.renewals(k) as f64 / w.days(),
                            "sites": [x.min_sites, x.max_sites], "not_full_samples": x.not_full,
                        }),
                    )
                })
                .collect();
            let tables: BTreeMap<String, u64> = w
                .tables
                .iter()
                .map(|(t, b)| (format!("{t:#06x}"), *b))
                .collect();
            json!({
                "index": w.index, "from": w.from, "to": w.to,
                "game_days_a_second": w.rate, "kinds": kinds, "tables": tables,
                "heap": w.heap, "wheels": w.wheels, "queues": w.queues, "archive": w.archive,
            })
        })
        .collect();
    let r = json!({
        "commit": plan.commit, "seed": plan.seed, "days": plan.days, "reached": run.reached,
        "window": plan.window, "judged": [plan.judged.start(), plan.judged.end()],
        "wall_seconds": run.wall.as_secs_f64(),
        "game_days_a_second": run.reached as f64 / run.wall.as_secs_f64().max(1e-9),
        "incomplete": run.incomplete,
        "verdict": verdict, "failing": failing,
        "checks": checks.iter().map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail})).collect::<Vec<_>>(),
        "windows": windows,
    });
    if let Some(p) = &plan.report {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        std::fs::write(p, serde_json::to_vec_pretty(&r).expect("the report encodes"))
            .expect("the report written");
    }
    (verdict.to_string(), failing)
}

fn run_plan(plan: Plan) {
    let (run, founded) = soak(&plan);
    let checks = judge(&plan, &run, &founded);
    for c in &checks {
        println!("{} {} {}", if c.ok { "ok  " } else { "FAIL" }, c.name, c.detail);
    }
    let (verdict, failing) = report(&plan, &run, &checks);
    println!(
        "who_is_alive_soak seed {}: {verdict} ({} of {} days, {:.1} game days a second) {}",
        plan.seed,
        run.reached,
        plan.days,
        run.reached as f64 / run.wall.as_secs_f64().max(1e-9),
        failing.join(" ")
    );
    assert_eq!(verdict, "PASS", "failing: {failing:?}");
}

/// EQC433 to EQC445: the every-kind world with nobody for 262,980 game days at max (or
/// `RTSIM_SOAK_DAYS`), judged by century.
#[test]
#[ignore = "hours: tools/nightly/soak.sh runs it alone in release"]
fn who_is_alive_soak() {
    let days = env("RTSIM_SOAK_DAYS").unwrap_or(262_980);
    let judged_to = ((days / CENTURY) as usize).clamp(3, 27);
    run_plan(Plan {
        days,
        window: CENTURY,
        judged: 2..=judged_to,
        seed: env("RTSIM_SOAK_SEED").unwrap_or(1),
        wall: Duration::from_secs_f64(env::<f64>("RTSIM_SOAK_WALL_HOURS").unwrap_or(6.0) * 3600.0),
        report: std::env::var("RTSIM_SOAK_REPORT").ok().map(PathBuf::from),
        commit: std::env::var("RTSIM_SOAK_COMMIT").unwrap_or_default(),
        short: false,
    });
}

/// EQC446: the soak's code on 1,000 game days, windows of 100 days (2 to 10 judged).
#[test]
fn who_is_alive_soak_short() {
    run_plan(Plan {
        days: 1_000,
        window: 100,
        judged: 2..=10,
        seed: 1,
        wall: Duration::from_secs(3600),
        report: None,
        commit: String::new(),
        short: true,
    });
}
