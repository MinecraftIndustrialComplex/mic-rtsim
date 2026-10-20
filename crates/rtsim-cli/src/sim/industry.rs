// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim industry`: the iron line (crates/rtsim-industry/src/scenario.rs) run for days,
//! headless, through the fake world.
//!
//! Five runs of the same world and seed:
//! 1. **main:** threats on, the engine applies (phases I-B onward);
//! 2. **simulated:** threats off;
//! 3. **simulated, no race:** threats off, and machines never tick before a reconcile lands;
//! 4. **control:** threats off, every district chunk-loaded all the time, so the factories run for
//!    real;
//! 5. **read-only:** phase I-A, where the ledger says what districts would have made and nothing
//!    is applied.
//!
//! **The I-B check** is run 3 against run 4: the simulated world makes at most what the control
//! makes, and no less than the tolerance below it. Run 2 against run 4 measures INDUSTRY.md §19
//! risk 3: a machine that ticks on stale stock before its reconcile lands. The trim keeps a
//! district's own books exact, but a boiler that burns already-replayed coal can power a
//! neighbour's real batches, which no trim sees; the leak is reported, bounded by one machine
//! period per load.
//!
//! It prints what each district learned, the replay's breakpoints, the reconcile deltas, the
//! threat outcomes, the I-B comparison of simulated against control, and the conservation check,
//! and writes all of it as JSON.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use rtsim_industry::engine::{EngineEvent, LedgerReport, Mode, Stats};
use rtsim_industry::fake::{Event, FakeWorld};
use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use serde::Serialize;

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub out: Option<PathBuf>,
    pub quiet: bool,
}

/// I-B's tolerance: the simulated run may make this share less than the control. TUNABLE,
/// PLACEHOLDER 0.10 (INDUSTRY.md §16 leaves it to Ben).
const TOLERANCE: f64 = 0.10;

#[derive(Serialize)]
struct DistrictOut {
    id: u64,
    name: String,
    learned: Option<rtsim_industry::district::Model>,
    ledger_before_return: Option<LedgerReport>,
    ledger_end: Option<LedgerReport>,
    reconciles: Vec<rtsim_industry::district::ReconcileReport>,
    attacks: Vec<rtsim_industry::district::AttackReport>,
}

#[derive(Serialize)]
struct RunOut {
    name: String,
    mode: Mode,
    threats: bool,
    control: bool,
    districts: Vec<DistrictOut>,
    engine_events: Vec<EngineEvent>,
    world_events: Vec<Event>,
    totals: BTreeMap<String, i64>,
    engine_books: rtsim_industry::engine::Books,
    world_books: rtsim_industry::fake::WorldBooks,
    conservation: String,
    cluster_books: String,
    stats: Stats,
    wire_bytes: u64,
    wall_ms: f64,
}

#[derive(Serialize)]
struct Comparison {
    good: String,
    control: i64,
    /// Without the stale-tick race (the I-B check).
    simulated: i64,
    ratio: f64,
    at_or_below_control: bool,
    within_tolerance: bool,
    /// With the race (risk 3): what leaked through machines ticking first.
    simulated_with_race: i64,
    race_leak: i64,
}

#[derive(Serialize)]
struct Report {
    scenario: &'static str,
    seed: u64,
    days: u64,
    tolerance: f64,
    tunables: Tunables,
    runs: Vec<RunOut>,
    comparison: Vec<Comparison>,
    passed: bool,
}

fn names(w: &FakeWorld, v: &GoodVec) -> BTreeMap<String, i64> {
    v.iter()
        .map(|(g, n)| (w.goods.get(&g).map_or(format!("good {}", g.0), |i| i.name.clone()), n))
        .collect()
}

fn run(name: &str, cfg: IronLine) -> (RunOut, FakeWorld) {
    let start = Instant::now();
    let mut w = iron_line(cfg, Tunables::default());
    // Stop just before the player returns to the smelter, to read the ledger (I-A's display).
    let before = Tick(8 * TICKS_PER_DAY - 1);
    w.run_until(before);
    let ids: Vec<(DistrictId, String)> = w.districts.iter().map(|d| (d.id, d.name.clone())).collect();
    let mut before_ledgers = BTreeMap::new();
    for (id, _) in &ids {
        before_ledgers.insert(*id, w.engine.ledger(*id, before));
    }
    run_iron_line(&mut w, &cfg);
    let wall_ms = start.elapsed().as_secs_f64() * 1e3;
    let now = w.t;
    let mut districts = Vec::new();
    for (id, dname) in &ids {
        let end = w.engine.ledger(*id, now);
        let d = w.engine.district(*id).expect("declared");
        districts.push(DistrictOut {
            id: id.0,
            name: dname.clone(),
            learned: d.models.values().next_back().cloned(),
            ledger_before_return: before_ledgers.remove(id).flatten(),
            ledger_end: end,
            reconciles: d.reports.iter().cloned().collect(),
            attacks: d.attacks.iter().cloned().collect(),
        });
    }
    let totals = names(&w, &w.totals());
    let conservation = match (cfg.mode, w.conservation()) {
        (Mode::ReadOnly, _) => "not applicable (read-only)".to_string(),
        (_, Ok(())) => "ok".to_string(),
        (_, Err(e)) => e,
    };
    let cluster_books = match w.engine.check_books() {
        Ok(()) => "ok".to_string(),
        Err(e) => e,
    };
    let out = RunOut {
        name: name.to_string(),
        mode: cfg.mode,
        threats: cfg.threats,
        control: cfg.control,
        districts,
        engine_events: w.engine.log.iter().cloned().collect(),
        world_events: w.log.clone(),
        totals,
        engine_books: w.engine.books.clone(),
        world_books: w.books.clone(),
        conservation,
        cluster_books,
        stats: w.engine.stats,
        wire_bytes: w.wire_bytes,
        wall_ms,
    };
    (out, w)
}

fn day(t: u64) -> String {
    format!("day {:>6.3}", t as f64 / TICKS_PER_DAY as f64)
}

pub fn main(opts: Options) -> i32 {
    let base = IronLine {
        seed: opts.seed,
        days: opts.days.max(10),
        threats: true,
        control: false,
        mode: Mode::Apply,
        tick_first: true,
    };
    let (main_run, w) = run("main", base);
    let (sim, ws) = run("simulated", IronLine { threats: false, ..base });
    let (norace, wn) = run("simulated, no race", IronLine { threats: false, tick_first: false, ..base });
    let (control, wc) = run("control", IronLine { threats: false, control: true, ..base });
    let (ro, _) = run("read-only", IronLine { threats: false, mode: Mode::ReadOnly, ..base });

    // I-B: the simulated world makes at most what the control does, and not much less.
    let mut comparison = Vec::new();
    let goods: Vec<Good> = w.goods.keys().copied().collect();
    let made = |x: &FakeWorld, g: Good| -> i64 {
        x.books.minted.get(g) + x.engine.books.minted.get(g) - x.engine.books.trimmed.get(g)
    };
    for g in goods {
        if !rtsim_industry::fake::is_item(&w.goods, g) {
            continue;
        }
        let (c, s, r) = (made(&wc, g), made(&wn, g), made(&ws, g));
        if c == 0 && s == 0 {
            continue;
        }
        let ratio = if c == 0 { f64::INFINITY } else { s as f64 / c as f64 };
        comparison.push(Comparison {
            good: w.goods[&g].name.clone(),
            control: c,
            simulated: s,
            ratio,
            at_or_below_control: s <= c,
            within_tolerance: ratio >= 1.0 - TOLERANCE,
            simulated_with_race: r,
            race_leak: r - s,
        });
    }
    let conserved = [&main_run, &sim, &norace, &control]
        .iter()
        .all(|r| r.conservation == "ok" && r.cluster_books == "ok");
    let passed = conserved && comparison.iter().all(|c| c.at_or_below_control && c.within_tolerance);

    if !opts.quiet {
        print_run(&main_run, &w);
        println!("\n== read-only (I-A): what the ledger showed just before the player came back ==");
        for d in &ro.districts {
            if let Some(l) = &d.ledger_before_return {
                println!(
                    "  {:<12} {:?}{}: would have made {}",
                    d.name,
                    l.state,
                    l.reason.as_ref().map_or(String::new(), |r| format!(" ({r})")),
                    words(&w, &l.made_since)
                );
            }
        }
        println!(
            "\n== I-B: made in {} days, simulated against control (threats off; tolerance {:.0}% below) ==",
            base.days,
            TOLERANCE * 100.0
        );
        for c in &comparison {
            println!(
                "  {:<26} control {:>7}  simulated {:>7}  ratio {:.4}  {}   with the stale-tick race {:>7} ({:+} leaked)",
                c.good,
                c.control,
                c.simulated,
                c.ratio,
                if c.at_or_below_control && c.within_tolerance { "ok" } else { "OUT OF BOUNDS" },
                c.simulated_with_race,
                c.race_leak
            );
        }
        println!("\n== checks ==");
        for r in [&main_run, &sim, &norace, &control, &ro] {
            println!(
                "  {:<19} conservation {}, cluster books {}, {} observations, {} breakpoints, {} settles, {} wire bytes, {:.0} ms",
                r.name, r.conservation, r.cluster_books, r.stats.observations, r.stats.breakpoints, r.stats.cluster_settles, r.wire_bytes, r.wall_ms
            );
        }
        let _ = (&ws, &wc, &wn);
        println!("  {}", if passed { "PASSED" } else { "FAILED" });
    }

    let report = Report {
        scenario: "iron_line",
        seed: opts.seed,
        days: base.days,
        tolerance: TOLERANCE,
        tunables: Tunables::default(),
        runs: vec![main_run, sim, norace, control, ro],
        comparison,
        passed,
    };
    if let Some(path) = opts.out {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&path, serde_json::to_string_pretty(&report).expect("serialize")) {
            Ok(()) => {
                if !opts.quiet {
                    println!("  wrote {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("cannot write {}: {e}", path.display());
                return 1;
            }
        }
    }
    if passed { 0 } else { 1 }
}

fn words(w: &FakeWorld, v: &GoodVec) -> String {
    let parts: Vec<String> = v
        .iter()
        .map(|(g, n)| format!("{n:+} {}", w.goods.get(&g).map_or("?", |i| i.name.as_str())))
        .collect();
    if parts.is_empty() { "nothing".to_string() } else { parts.join(", ") }
}

fn print_run(r: &RunOut, w: &FakeWorld) {
    println!("== the iron line: Ore Coast -> (train 1) -> Smelter (+ Power Plant) -> (train 2) -> Home ==");
    println!("\n-- learned (I5, I6) --");
    for d in &r.districts {
        match &d.learned {
            Some(m) => {
                let rates: Vec<String> = m
                    .learned
                    .rates
                    .iter()
                    .map(|&(g, x)| format!("{:+.0}/h {}", x * 72_000.0, w.goods.get(&g).map_or("?", |i| i.name.as_str())))
                    .collect();
                println!(
                    "  {:<12} window {:>6} ticks, confidence {:.2}, targets {}: {}",
                    d.name,
                    m.learned.window_ticks,
                    m.learned.confidence,
                    m.targets.len(),
                    if rates.is_empty() { "idle".to_string() } else { rates.join(", ") }
                );
            }
            None => println!("  {:<12} not learned", d.name),
        }
    }
    println!("\n-- replay breakpoints, reconciles, trims and attacks (engine log, exchanges summarised) --");
    let mut exchanges = 0usize;
    for e in &r.engine_events {
        if e.what.starts_with("station exchange") {
            exchanges += 1;
            continue;
        }
        let who = r
            .districts
            .iter()
            .find(|d| d.id == e.district)
            .map_or("trains".to_string(), |d| d.name.clone());
        println!("  {} {:<12} {}", day(e.tick), who, e.what);
    }
    println!("  ({exchanges} emulated station exchanges at simulated stations, I15)");
    println!("\n-- what the player saw (world) --");
    for e in &r.world_events {
        println!("  {} {}", day(e.tick), e.what);
    }
    println!("\n-- ledger at the end --");
    for d in &r.districts {
        if let Some(l) = &d.ledger_end {
            println!(
                "  {:<12} {:?}; last reconcile: {}; attacks: {}",
                d.name,
                l.state,
                l.last_reconcile.as_ref().map_or("none".to_string(), |x| format!(
                    "epoch {} net {}{}{}",
                    x.epoch,
                    words(w, &x.net),
                    if x.trims.is_empty() { String::new() } else { format!(", trims {}", words(w, &x.trims)) },
                    if x.untracked.is_empty() { String::new() } else { format!(", untracked {}", words(w, &x.untracked)) }
                )),
                l.attacks.len()
            );
        }
    }
    println!("\n-- books --");
    println!("  engine minted {}", words(w, &r.engine_books.minted));
    println!("  engine burned {}", words(w, &r.engine_books.burned));
    println!("  stolen {}", words(w, &r.engine_books.stolen));
    println!("  dropped by destroyed blocks {}", words(w, &r.engine_books.dropped));
    println!("  shortfalls {} -> trimmed {}", words(w, &r.engine_books.shortfall), words(w, &r.engine_books.trimmed));
    println!("  world machines minted {}", words(w, &r.world_books.minted));
}
