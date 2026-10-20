// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli`: the engine's command line (PLAN.md §4.11). Only `bench scale` exists so far; `run`,
//! `replay` and `inspect` come with the modules they drive.

mod bench;
mod sim;
mod sys;

use std::path::PathBuf;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: sys::Counting = sys::Counting;

const USAGE: &str = "\
usage:
  rtsim-cli bench scale [--quick] [--check] [--repeats N] [--out DIR | --no-write] [--only NAME,...]
      Runs SCALE.md §10's sweeps, prints the table and slopes, writes JSON to DIR
      (default bench/results). --check exits 1 when a slope exceeds its declared contract.
      --repeats runs each point N times and keeps the median (default 3, quick 1).
      --only picks sweeps by name: latent, regions, events, elapsed, age, markets, edges.
  rtsim-cli bench scale-one --spec JSON
      Runs one configuration and prints its measurements as JSON (used by bench scale).
  rtsim-cli sim phase1 [--days N] [--seed S] [--every D] [--no-script] [--check] [--trace D]
                       [--rules LIST] [--out DIR | --no-write]
      Runs the phase 1 scenario (PLAN.md §8) on the synthetic world: prices, stocks, flows,
      treasuries, exchange rates and news spread, with a scripted player (dump, rob, rescue).
      Writes JSON to DIR (default scenario/results). --check verifies conservation after every
      event (slow). --trace D prints what caused the market events of the last D days.
      --rules sets the markets' decision rules for comparisons: round1, or name=value pairs
      (slack, money, band, up, ask, div; see rtsim_trade::tunables::Rules).
  rtsim-cli sim quiet [--markets N] [--days D] [--every E] [--seed S] [--genesis G] [--rules LIST]
                     [--trace D]
      Runs a closed world with no player (the phase 1 world, or a grid of N villages) and prints
      market events per market per day for each E-day window: does it go quiet?";

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
        ["sim", "quiet", rest @ ..] => {
            let mut o = sim::quiet::Options {
                markets: 0,
                days: 400,
                every: 25,
                seed: 42,
                genesis: 500,
                rules: None,
                trace_days: 0,
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
