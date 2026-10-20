// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli`: the engine's command line (PLAN.md §4.11). Only `bench scale` exists so far; `run`,
//! `replay` and `inspect` come with the modules they drive.

mod bench;
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
      --only picks sweeps by name: latent, regions, events, elapsed, age, paged.
  rtsim-cli bench scale-one --spec JSON
      Runs one configuration and prints its measurements as JSON (used by bench scale).
  rtsim-cli bench bsp [--quick] [--check] [--repeats N] [--workers 1,2,4] [--regions N]
                      [--out DIR | --no-write]
      The parallel step at several worker counts, resident and paged, and the determinism
      gate: every run of a world must end in the same state hash. --check exits 1 when not.
  rtsim-cli bench bsp-one --spec JSON
      Runs one parallel-step configuration (used by bench bsp).";

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
        ["bench", "bsp", rest @ ..] => {
            let mut opts = bench::bsp::Options {
                quick: false,
                check: false,
                repeats: 0,
                out: Some(PathBuf::from("bench/results")),
                workers: Vec::new(),
                regions: None,
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
