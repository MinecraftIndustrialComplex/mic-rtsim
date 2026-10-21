// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli run <world> --fork <dir> ...`: the headless host (VIEWER.md §12; rtsim_host::run).
//! The world is exported into `<dir>` first (or a fork already there resumes), then the engine
//! runs on the fork's pages with the Java side of a server where nobody is online. Ctrl-C (or
//! `q` and Enter) stops it with a final save.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

use rtsim_host::fork;
use rtsim_host::run::{Control, DAY, Host, Options, Speed, Until};
use rtsim_kernel::page::Durability;

static CTL: Control = Control::new();

extern "C" fn on_signal(_: libc::c_int) {
    if CTL.stop.swap(true, Ordering::Relaxed) {
        // A second one: stop now, without the final save.
        // SAFETY: _exit is async-signal-safe.
        unsafe { libc::_exit(130) };
    }
}

const KEYS: &str = "keys, then Enter: p pause or resume, s save now, 1x / 10x / max speed, q quit";

struct Args {
    world: PathBuf,
    dir: PathBuf,
    opts: Options,
    view: Option<String>,
    allow_tailnet: bool,
    every: Duration,
}

fn parse(args: &[&str]) -> Result<Args, String> {
    let mut world = None;
    let mut dir = None;
    let mut opts = Options {
        speed: Speed::X1,
        ..Options::default()
    };
    let mut view = None;
    let mut allow_tailnet = false;
    let mut every = Duration::from_secs(5);
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    while i < args.len() {
        match args[i] {
            "--fork" => dir = Some(PathBuf::from(value(&mut i, "--fork")?)),
            "--speed" => opts.speed = Speed::parse(&value(&mut i, "--speed")?)?,
            "--until" => opts.until = Some(Until::parse(&value(&mut i, "--until")?)?),
            "--season-days" => {
                let v = value(&mut i, "--season-days")?;
                opts.season_days = Some(
                    v.parse()
                        .ok()
                        .filter(|d| *d > 0)
                        .ok_or_else(|| format!("--season-days {v}: a positive whole number"))?,
                );
            }
            "--stride" => {
                let v = value(&mut i, "--stride")?;
                opts.stride = Some(
                    v.parse()
                        .ok()
                        .filter(|s| *s > 0)
                        .ok_or_else(|| format!("--stride {v}: ticks, at least 1"))?,
                );
            }
            "--view" => {
                // The address is optional (host:port): 127.0.0.1:8740 without one.
                match args.get(i + 1) {
                    Some(a) if !a.starts_with("--") && a.contains(':') => {
                        view = Some(a.to_string());
                        i += 1;
                    }
                    _ => view = Some("127.0.0.1:8740".to_string()),
                }
            }
            "--allow-tailnet" => allow_tailnet = true,
            "--test" => opts.test = true,
            "--no-fsync" => opts.durability = Durability::NoSync,
            "--status-every" => {
                let v = value(&mut i, "--status-every")?;
                every = Duration::from_secs_f64(
                    v.parse::<f64>()
                        .ok()
                        .filter(|s| *s > 0.0)
                        .ok_or_else(|| format!("--status-every {v}: seconds"))?,
                );
            }
            "--threads" => {
                let v = value(&mut i, "--threads")?;
                let (f, b) = v
                    .split_once(',')
                    .and_then(|(f, b)| Some((f.parse().ok()?, b.parse().ok()?)))
                    .filter(|(f, b): &(usize, usize)| *f > 0 && *b > 0)
                    .ok_or_else(|| format!("--threads {v}: FG,BG"))?;
                opts.threads = Some((f, b));
            }
            a if a.starts_with("--") => return Err(format!("unknown option {a}")),
            a if world.is_none() => world = Some(PathBuf::from(a)),
            a => return Err(format!("unexpected argument {a}")),
        }
        i += 1;
    }
    Ok(Args {
        world: world.ok_or("no world folder")?,
        dir: dir.ok_or("--fork <dir> is required: the host runs on a copy, never on the world")?,
        opts,
        view,
        allow_tailnet,
        every,
    })
}

pub fn cli(args: &[&str]) -> ExitCode {
    let a = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("run: {e}");
            return ExitCode::from(64);
        }
    };
    match run(&a) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("run: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(a: &Args) -> Result<(), String> {
    if let Some(addr) = &a.view {
        let _ = a.allow_tailnet;
        return Err(format!(
            "--view {addr}: the host source (VIEWER.md §4.1) is not built yet; `rtsim-cli view {}` shows the fork as saved",
            a.dir.display()
        ));
    }
    fork::check_apart(&a.world, &a.dir)?;
    if fork::is_fork(&a.dir) {
        let info = fork::ForkInfo::read(&a.dir)?;
        println!(
            "resuming the fork {} (exported from {})",
            a.dir.display(),
            info.source
        );
    } else {
        let info = fork::export(&a.world, &a.dir)?;
        crate::export::print_info(&info, &a.dir);
    }
    let mut host = Host::open(&a.dir, &a.opts)?;
    println!(
        "engine on {}: game time {} (day {:.2}), modules {:#x}, topology {}",
        a.dir.join("rtsim").display(),
        host.start_time,
        host.start_time as f64 / DAY as f64,
        host.mask,
        host.topology
    );
    if let Some(u) = a.opts.until {
        let t = u.target(host.start_time);
        println!("until game time {t} (day {:.2})", t as f64 / DAY as f64);
    }
    // SAFETY: the handler only stores to atomics and calls _exit.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }
    println!("{KEYS}");
    std::thread::Builder::new()
        .name("rtsim-host-keys".into())
        .spawn(keys)
        .map_err(|e| format!("the keys thread: {e}"))?;
    let out = host.run(&a.opts, &CTL, a.every, |s| println!("{s}"));
    let refused = host.j.refused.clone();
    let (peak_journal, peak_flight, waited) = (host.peak_journal, host.peak_in_flight, host.wait);
    let closed = host.close();
    let out = out?;
    closed?;
    println!(
        "ran {} steps, {:.2} game days in {:.1} s: {:.2} game days/s, {:.1} us a step ({:.1} us waiting on the engine); {} saves{}",
        out.steps,
        out.ticks as f64 / DAY as f64,
        out.wall.as_secs_f64(),
        out.game_days_per_second(),
        out.wall_per_step().as_secs_f64() * 1e6,
        waited.as_secs_f64() * 1e6 / out.steps.max(1) as f64,
        out.saves + 1,
        if out.stopped { " (stopped)" } else { "" }
    );
    println!(
        "Java's journal peaked at {peak_journal} records, commands in flight at {peak_flight}; answered as refused: {}",
        refused
            .iter()
            .map(|(k, n)| format!("{k:#06x} x{n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

/// Terminal keys, a line at a time (cheap: no raw mode). Ends at end of input.
fn keys() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        match line.trim() {
            "q" | "quit" => CTL.stop.store(true, Ordering::Relaxed),
            "s" | "save" => CTL.save_now.store(true, Ordering::Relaxed),
            "p" | "pause" => {
                let was = CTL.paused.fetch_xor(true, Ordering::Relaxed);
                println!("{}", if was { "resumed" } else { "paused" });
            }
            "" => {}
            other => match Speed::parse(other) {
                Ok(s) => {
                    CTL.set_speed(s);
                    println!("speed {}", s.name());
                }
                Err(_) => println!("{KEYS}"),
            },
        }
    }
}
