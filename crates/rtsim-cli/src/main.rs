// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli`: the engine's command line (PLAN.md §4.11). Only `bench scale` exists so far; `run`,
//! `replay` and `inspect` come with the modules they drive.

mod bench;
mod sim;
mod sys;
mod view;

use std::path::PathBuf;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: sys::Counting = sys::Counting;

const USAGE: &str = "\
usage:
  rtsim-cli bench scale [--quick] [--check] [--repeats N] [--out DIR | --no-write] [--only NAME,...]
                        [--tag NAME]
      Runs SCALE.md §10's sweeps, prints the table and slopes, writes JSON to DIR
      (default bench/results). --check exits 1 when a slope exceeds its declared contract.
      --tag names the run in its file name and record (e.g. potato).
      --repeats runs each point N times and keeps the median (default 3, quick 1).
      --only picks sweeps by name: latent, regions, events, elapsed, age, markets, edges,
      markets-paged, dragons, dragons_paged, dragons_resident, ships, ships-grid, ships-paged,
      herds, herds-wide, herds-paged, members, factions, factions-paged, paged.
  rtsim-cli bench scale-one --spec JSON
      Runs one configuration and prints its measurements as JSON (used by bench scale).
  rtsim-cli bench bsp [--quick] [--check] [--repeats N] [--workers 1,2,4] [--regions N]
                      [--out DIR | --no-write] [--tag NAME]
      The parallel step at several worker counts, resident and paged, and the determinism
      gate: every run of a world must end in the same state hash. --check exits 1 when not.
  rtsim-cli bench bsp-one --spec JSON
      Runs one parallel-step configuration (used by bench bsp).
  rtsim-cli bench o5 [--quick] [--repeats N] [--out DIR | --no-write] [--tag NAME]
      Slot commits against dyadic counts at read cadences from 1 to 1,024 days (DECISIONS O5).
  rtsim-cli bench o5-one --spec JSON
      Runs one cadence point (used by bench o5).
  rtsim-cli bench industry [--quick] [--check] [--only PREFIX] [--out DIR | --no-write]
      The industry engine's cost sweeps (quiet and busy district counts, elapsed time, the glass
      box's compile and replay) against their declared slopes; JSON to DIR (default
      bench/results). --only runs the sweeps whose names start with PREFIX (e.g. glass_).
      --check exits 1 on a breach.
  rtsim-cli bench restore [--markets N] [--days D] [--repeats R] [--out DIR | --no-write]
      The phase 1 world's snapshot against its whole input log: a grid of N villages (default
      1,024) joined one by one and run D days (default 100), then save, restore (in memory and
      through a page store) and one more day; JSON to DIR (default bench/results).
  rtsim-cli sim industry [--days N] [--seed S] [--out FILE | --no-write] [--quiet]
      The iron line (rtsim-industry's synthetic scenario) for N days (at least 10): learning,
      replay, reconciles, threats, and simulated against control. Writes JSON to FILE
      (default bench/results/sim-industry.json). Exits 1 if a check fails.
  rtsim-cli sim phase1 [--days N] [--seed S] [--every D] [--no-script] [--check] [--trace D]
                       [--rules LIST] [--out DIR | --no-write]
      Runs the phase 1 scenario (PLAN.md §8) on the synthetic world: prices, stocks, flows,
      treasuries, exchange rates and news spread, with a scripted player (dump, rob, rescue).
      Writes JSON to DIR (default scenario/results). --check verifies conservation after every
      event (slow). --trace D prints what caused the market events of the last D days.
      --rules sets the markets' decision rules for comparisons: round1, or name=value pairs
      (slack, money, band, up, ask, div; see rtsim_trade::tunables::Rules).
  rtsim-cli sim quiet [--markets N] [--days D] [--every E] [--seed S] [--genesis G] [--rules LIST]
                     [--trace D] [--grow] [--no-growth] [--no-bands] [--works]
      Runs a closed world with no player (the phase 1 world, or a grid of N villages) and prints
      market events per market per day for each E-day window: does it go quiet? --grow builds
      the phase 1 world by joining its villages one a day (C22). --works loads the synthetic
      works catalogue (SINK.md) and adds works bought and coin spent per window.
  rtsim-cli sim memory [--markets N] [--days D]
      Builds a grid of N villages (default 1,024), runs D days (default 20) and prints where the
      memory goes, per market, by component, next to the counting allocator's total.
  rtsim-cli sim dragons [--days N] [--seed S] [--out FILE | --no-write] [--quiet]
      Phase 2's dragons (PLAN.md §9) from autumn into winter on the synthetic world, with a
      scripted player and a farm district: grudges, heirs, bored departures, raids, prey by
      season. Writes JSON to FILE (default scenario/results/sim-dragons.json). Exits 1 if a
      check fails.
  rtsim-cli sim ships [--days N] [--seed S] [--out FILE | --no-write] [--quiet]
      Ships (PLAN.md §10.1) on a synthetic coast: sea and sky lanes, timetables, merchant
      flows, a pirate haven, a navy patrol, and a scripted player who watches ships pass, sees
      a raid, loots a hold and sinks a pirate; a restart in the middle. Writes JSON to FILE
      (default scenario/results/sim-ships.json). Exits 1 if a check fails.
  rtsim-cli sim herds [--days N] [--seed S] [--out FILE | --no-write] [--quiet]
      Migrating herds (PLAN.md §10.2) in a synthetic valley for N days (default 200, two sim
      years): about 33,000 animals in six herds, a player who hunts, walks away and back, and
      fences a pass; dragons that hunt and follow herds; a restart in the middle. Writes JSON to
      FILE (default scenario/results/sim-herds.json). Exits 1 if a check fails.
  rtsim-cli sim conflict [--days N] [--seed S] [--out FILE | --no-write] [--quiet]
      Conflict (PLAN.md §10.3): a goblin tribe, a player's polluting refinery and a logged
      forest. Runs ignore (notice, envoy and demand, scouts, a war band that raids while the
      player is offline, applied on load), tribute and converter (de-escalation), and a save and
      restore in the middle. Writes JSON to FILE (default scenario/results/sim-conflict.json).
      Exits 1 if a check fails.
  rtsim-cli view <world> [options]
      Serves a saved world to the viewer page on loopback (VIEWER.md); --help for options.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["bench", "scale", rest @ ..] => {
            let mut opts = bench::scale::Options {
                quick: false,
                check: false,
                repeats: 0,
                out: Some(PathBuf::from("bench/results")),
                only: Vec::new(),
                tag: None,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                match a {
                    "--quick" => opts.quick = true,
                    "--repeats" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.repeats = n,
                        None => return usage("--repeats needs a number"),
                    },
                    "--check" => opts.check = true,
                    "--no-write" => opts.out = None,
                    "--out" => match it.next() {
                        Some(d) => opts.out = Some(PathBuf::from(d)),
                        None => return usage("--out needs a folder"),
                    },
                    "--only" => match it.next() {
                        Some(list) => opts.only = list.split(',').map(str::to_string).collect(),
                        None => return usage("--only needs sweep names"),
                    },
                    "--tag" => match it.next() {
                        Some(t) => opts.tag = Some(t.to_string()),
                        None => return usage("--tag needs a name"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            if opts.repeats == 0 {
                opts.repeats = if opts.quick { 1 } else { 3 };
            }
            ExitCode::from(bench::scale::run(opts) as u8)
        }
        ["bench", "scale-one", "--spec", json] => {
            let spec: bench::scale::Spec = match serde_json::from_str(json) {
                Ok(s) => s,
                Err(e) => return usage(&format!("bad --spec: {e}")),
            };
            let m = bench::scale::run_one(&spec);
            println!("{}", serde_json::to_string(&m).expect("serialize"));
            ExitCode::SUCCESS
        }
        ["bench", "industry", rest @ ..] => {
            let mut opts = bench::industry::Options {
                quick: false,
                check: false,
                out: Some(PathBuf::from("bench/results")),
                only: None,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                match a {
                    "--quick" => opts.quick = true,
                    "--check" => opts.check = true,
                    "--no-write" => opts.out = None,
                    "--out" => match it.next() {
                        Some(d) => opts.out = Some(PathBuf::from(d)),
                        None => return usage("--out needs a folder"),
                    },
                    "--only" => match it.next() {
                        Some(p) => opts.only = Some((*p).to_string()),
                        None => return usage("--only needs a sweep name or prefix"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            ExitCode::from(bench::industry::run(opts) as u8)
        }
        ["sim", "industry", rest @ ..] => {
            let mut opts = sim::industry::Options {
                days: 10,
                seed: 7,
                out: Some(PathBuf::from("bench/results/sim-industry.json")),
                quiet: false,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                match a {
                    "--days" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.days = n,
                        None => return usage("--days needs a number"),
                    },
                    "--seed" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.seed = n,
                        None => return usage("--seed needs a number"),
                    },
                    "--out" => match it.next() {
                        Some(f) => opts.out = Some(PathBuf::from(f)),
                        None => return usage("--out needs a file"),
                    },
                    "--no-write" => opts.out = None,
                    "--quiet" => opts.quiet = true,
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            ExitCode::from(sim::industry::main(opts) as u8)
        }
        ["sim", "phase1", rest @ ..] => {
            let mut o = sim::phase1::Options {
                days: 120,
                seed: 42,
                script: true,
                check: false,
                out: Some(PathBuf::from("scenario/results")),
                every: 10,
                trace_days: 0,
                rules: None,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
                match a {
                    "--days" => match num(it.next()) {
                        Some(n) => o.days = n,
                        None => return usage("--days needs a number"),
                    },
                    "--seed" => match num(it.next()) {
                        Some(n) => o.seed = n,
                        None => return usage("--seed needs a number"),
                    },
                    "--every" => match num(it.next()) {
                        Some(n) if n > 0 => o.every = n,
                        _ => return usage("--every needs a positive number"),
                    },
                    "--trace" => match num(it.next()) {
                        Some(n) => o.trace_days = n,
                        None => return usage("--trace needs a number of days"),
                    },
                    "--rules" => match it.next().map(|r| rtsim_trade::Rules::parse(r)) {
                        Some(Ok(r)) => o.rules = Some(r),
                        Some(Err(e)) => return usage(&format!("--rules: {e}")),
                        None => return usage("--rules needs a list"),
                    },
                    "--no-script" => o.script = false,
                    "--check" => o.check = true,
                    "--no-write" => o.out = None,
                    "--out" => match it.next() {
                        Some(d) => o.out = Some(PathBuf::from(d)),
                        None => return usage("--out needs a folder"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            ExitCode::from(sim::phase1::run(o) as u8)
        }
        ["sim", "dragons", rest @ ..] => sim::dragons::cli(rest),
        ["sim", "ships", rest @ ..] => sim::ships::cli(rest),
        ["sim", "herds", rest @ ..] => sim::herds::cli(rest),
        ["sim", "conflict", rest @ ..] => sim::conflict::cli(rest),
        ["bench", "restore", rest @ ..] => bench::restore::cli(rest),
        ["sim", "quiet", rest @ ..] => {
            let mut o = sim::quiet::Options {
                markets: 0,
                days: 400,
                every: 25,
                seed: 42,
                genesis: 500,
                rules: None,
                trace_days: 0,
                grow: false,
                growth: true,
                bands: true,
                works: false,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
                match a {
                    "--markets" => match num(it.next()) {
                        Some(n) => o.markets = n,
                        None => return usage("--markets needs a number"),
                    },
                    "--days" => match num(it.next()) {
                        Some(n) => o.days = n,
                        None => return usage("--days needs a number"),
                    },
                    "--every" => match num(it.next()) {
                        Some(n) if n > 0 => o.every = n,
                        _ => return usage("--every needs a positive number"),
                    },
                    "--seed" => match num(it.next()) {
                        Some(n) => o.seed = n,
                        None => return usage("--seed needs a number"),
                    },
                    "--trace" => match num(it.next()) {
                        Some(n) => o.trace_days = n,
                        None => return usage("--trace needs a number of days"),
                    },
                    "--grow" => o.grow = true,
                    "--no-growth" => o.growth = false,
                    "--no-bands" => o.bands = false,
                    "--works" => o.works = true,
                    "--genesis" => match num(it.next()) {
                        Some(n) => o.genesis = n as i64,
                        None => return usage("--genesis needs thousandths of target"),
                    },
                    "--rules" => match it.next().map(|r| rtsim_trade::Rules::parse(r)) {
                        Some(Ok(r)) => o.rules = Some(r),
                        Some(Err(e)) => return usage(&format!("--rules: {e}")),
                        None => return usage("--rules needs a list"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            ExitCode::from(sim::quiet::run(o) as u8)
        }
        ["sim", "memory", rest @ ..] => {
            let (mut markets, mut days) = (1_024u64, 20u64);
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
                match a {
                    "--markets" => match num(it.next()) {
                        Some(n) if n > 0 => markets = n,
                        _ => return usage("--markets needs a positive number"),
                    },
                    "--days" => match num(it.next()) {
                        Some(n) => days = n,
                        None => return usage("--days needs a number"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            ExitCode::from(sim::memory::run(markets, days) as u8)
        }
        ["bench", "bsp", rest @ ..] => {
            let mut opts = bench::bsp::Options {
                quick: false,
                check: false,
                repeats: 0,
                out: Some(PathBuf::from("bench/results")),
                workers: Vec::new(),
                regions: None,
                tag: None,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                match a {
                    "--quick" => opts.quick = true,
                    "--check" => opts.check = true,
                    "--no-write" => opts.out = None,
                    "--repeats" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.repeats = n,
                        None => return usage("--repeats needs a number"),
                    },
                    "--regions" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.regions = Some(n),
                        None => return usage("--regions needs a number"),
                    },
                    "--workers" => match it.next().map(|l| {
                        l.split(',')
                            .map(str::parse)
                            .collect::<Result<Vec<usize>, _>>()
                    }) {
                        Some(Ok(w)) if !w.is_empty() => opts.workers = w,
                        _ => return usage("--workers needs a list of numbers"),
                    },
                    "--out" => match it.next() {
                        Some(d) => opts.out = Some(PathBuf::from(d)),
                        None => return usage("--out needs a folder"),
                    },
                    "--tag" => match it.next() {
                        Some(t) => opts.tag = Some(t.to_string()),
                        None => return usage("--tag needs a name"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            if opts.repeats == 0 {
                opts.repeats = if opts.quick { 1 } else { 3 };
            }
            ExitCode::from(bench::bsp::run(opts) as u8)
        }
        ["bench", "bsp-one", "--spec", json] => {
            let spec: bench::bsp::BspSpec = match serde_json::from_str(json) {
                Ok(s) => s,
                Err(e) => return usage(&format!("bad --spec: {e}")),
            };
            let m = bench::bsp::run_one(&spec);
            println!("{}", serde_json::to_string(&m).expect("serialize"));
            ExitCode::SUCCESS
        }
        ["bench", "o5", rest @ ..] => {
            let mut opts = bench::o5::Options {
                quick: false,
                repeats: 0,
                out: Some(PathBuf::from("bench/results")),
                tag: None,
            };
            let mut it = rest.iter();
            while let Some(&a) = it.next() {
                match a {
                    "--quick" => opts.quick = true,
                    "--no-write" => opts.out = None,
                    "--repeats" => match it.next().and_then(|n| n.parse().ok()) {
                        Some(n) => opts.repeats = n,
                        None => return usage("--repeats needs a number"),
                    },
                    "--out" => match it.next() {
                        Some(d) => opts.out = Some(PathBuf::from(d)),
                        None => return usage("--out needs a folder"),
                    },
                    "--tag" => match it.next() {
                        Some(t) => opts.tag = Some(t.to_string()),
                        None => return usage("--tag needs a name"),
                    },
                    other => return usage(&format!("unknown option {other}")),
                }
            }
            if opts.repeats == 0 {
                opts.repeats = if opts.quick { 1 } else { 3 };
            }
            ExitCode::from(bench::o5::run(opts) as u8)
        }
        ["bench", "o5-one", "--spec", json] => {
            let spec: bench::o5::O5Spec = match serde_json::from_str(json) {
                Ok(s) => s,
                Err(e) => return usage(&format!("bad --spec: {e}")),
            };
            let m = bench::o5::run_one(&spec);
            println!("{}", serde_json::to_string(&m).expect("serialize"));
            ExitCode::SUCCESS
        }
        ["view", rest @ ..] => view::cli(rest),
        ["help" | "--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => usage("no command"),
    }
}

fn usage(why: &str) -> ExitCode {
    eprintln!("{why}\n{USAGE}");
    ExitCode::from(64)
}
