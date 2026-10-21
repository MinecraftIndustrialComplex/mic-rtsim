// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli run <world> --fork <dir> ...`: the headless host (VIEWER.md §12; rtsim_host::run).
//! The world is exported into `<dir>` first (or a fork already there resumes), then the engine
//! runs on the fork's pages with the Java side of a server where nobody is online. Ctrl-C (or
//! `q` and Enter) stops it with a final save. `--view` serves the viewer page from the fork's
//! saves as the run makes them (the host source, rtsim_host::view).

use std::io::BufRead;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use mic_viewer::http::{Config, Server};
use mic_viewer::viewer::App;
use rtsim_host::fork;
use rtsim_host::run::{Control, DAY, Host, Options, Speed, Until};
use rtsim_host::view::{HostView, LEASE_SAVE_EVERY};
use rtsim_kernel::page::Durability;
use rtsim_view::Limits;

static CTL: Control = Control::new();
/// Ctrl-C and SIGTERM so far: only the handler counts them, so a `q` before one still gets the
/// final save.
static SIGNALS: AtomicU8 = AtomicU8::new(0);

extern "C" fn on_signal(_: libc::c_int) {
    CTL.stop.store(true, Ordering::Relaxed);
    if SIGNALS.fetch_add(1, Ordering::Relaxed) >= 1 {
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
    view: Option<SocketAddr>,
    /// Serve the viewer on this Unix socket instead of a TCP port (a server where others have
    /// shells: only the socket's owner can reach it, VIEWER.md §13).
    socket: Option<PathBuf>,
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
    let mut socket = None;
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
                // The address is optional (IP:port): loopback at the viewer's port without one.
                match args.get(i + 1) {
                    Some(a) if !a.starts_with("--") && a.contains(':') => {
                        view = Some(a.parse().map_err(|_| {
                            format!("--view {a}: an IP address and port (127.0.0.1:8740)")
                        })?);
                        i += 1;
                    }
                    _ => view = Some(SocketAddr::from(([127, 0, 0, 1], mic_viewer::DEFAULT_PORT))),
                }
            }
            "--socket" => socket = Some(PathBuf::from(value(&mut i, "--socket")?)),
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
        socket,
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
    // Where the viewer will listen, checked before the export: never a public address (VWD-7).
    if let Some(addr) = a.view
        && !mic_viewer::http::address_allowed(addr.ip(), a.allow_tailnet)
    {
        return Err(format!(
            "--view {addr}: not a loopback address (reach the viewer with `ssh -L`; tailnet \
             addresses need --allow-tailnet)"
        ));
    }
    if let Some(sock) = &a.socket
        && crate::view::under(sock, &a.world)
    {
        return Err(format!(
            "--socket {}: inside the world folder, which the host never writes into; put it elsewhere",
            sock.display()
        ));
    }
    let (world_abs, _) = fork::check_apart(&a.world, &a.dir)?;
    if fork::is_fork(&a.dir) {
        let info = fork::ForkInfo::read(&a.dir)?;
        if Path::new(&info.source) != world_abs {
            return Err(format!(
                "the fork {} was exported from {}, not {}: name that world to resume it, or export into a new folder",
                a.dir.display(),
                info.source,
                world_abs.display()
            ));
        }
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
    let viewer = match (&a.socket, a.view) {
        (Some(path), _) => Some(serve(a, Listen::Socket(path))?),
        (None, Some(addr)) => Some(serve(a, Listen::Tcp(addr))?),
        (None, None) => None,
    };
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
    if let Some((hv, mut server, reader)) = viewer {
        hv.stop();
        server.stop();
        let _ = reader.join();
        let (n, unchanged) = hv.rereads();
        println!(
            "the viewer reread the fork {n} times ({unchanged} found nothing newer), the longest in {:.1} ms; the lease asked for {} saves",
            hv.longest_reread().as_secs_f64() * 1e3,
            hv.lease_saves()
        );
    }
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

/// Where the host source listens.
enum Listen<'a> {
    Tcp(SocketAddr),
    Socket(&'a Path),
}

/// The host source on `at`: the fork as last saved, reread after each save (rtsim_host::view).
fn serve(
    a: &Args,
    at: Listen,
) -> Result<(Arc<HostView>, Server, std::thread::JoinHandle<()>), String> {
    let heap: rtsim_view::view::HeapProbe = Arc::new(|| crate::sys::heap_live() as u64);
    let hv = HostView::open(&a.dir, &CTL, a.opts.speed, Limits::default(), Some(heap))
        .map_err(|e| format!("the viewer: {e}"))?;
    let reader = hv
        .start()
        .map_err(|e| format!("the viewer's reader: {e}"))?;
    let config = Config {
        allow_tailnet: a.allow_tailnet,
        name: "rtsim-host".into(),
    };
    let app = Arc::new(App::new(Arc::clone(&hv)));
    let addr = match at {
        Listen::Tcp(addr) => addr,
        #[cfg(unix)]
        Listen::Socket(path) => {
            let server = Server::start_unix(path, config, app)
                .map_err(|e| format!("the viewer on {}: {e}", path.display()))?;
            println!(
                "viewer: on the Unix socket {}: the fork's last save, reread after each one; saved every {} s while a page is open",
                path.display(),
                LEASE_SAVE_EVERY.as_secs()
            );
            println!(
                "  from your machine: ssh -L 8740:{} <this host>, then open http://localhost:8740/",
                path.display()
            );
            return Ok((hv, server, reader));
        }
        #[cfg(not(unix))]
        Listen::Socket(_) => return Err("--socket needs a Unix system".into()),
    };
    let server =
        Server::start(addr, config, app).map_err(|e| format!("the viewer on {addr}: {e}"))?;
    let bound = server.addr().unwrap_or(addr);
    let scope = if bound.ip().is_loopback() {
        "loopback only"
    } else {
        "tailnet"
    };
    println!(
        "viewer: http://{bound}/ ({scope}): the fork's last save, reread after each one; saved every {} s while a page is open",
        LEASE_SAVE_EVERY.as_secs()
    );
    println!(
        "  from another machine: ssh -L {p}:127.0.0.1:{p} <this host>, then open http://localhost:{p}/",
        p = bound.port()
    );
    Ok((hv, server, reader))
}

/// Terminal keys, a line at a time (cheap: no raw mode). Ends at end of input.
fn keys() {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        match line.trim() {
            "q" | "quit" => CTL.stop.store(true, Ordering::Relaxed),
            "s" | "save" => {
                CTL.save_now.store(true, Ordering::Relaxed);
                if CTL.paused.load(Ordering::Relaxed) {
                    println!("paused: the save is taken at the first step after resuming");
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rtsim-view/tests/data")
            .join(name)
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rtsim-cli-run-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn socket_implies_the_viewer_and_must_be_outside_the_world() {
        let world = fixture("core");
        let a = parse(&[
            world.to_str().unwrap(),
            "--fork",
            "/tmp/x",
            "--socket",
            "/tmp/v.sock",
        ])
        .unwrap();
        assert_eq!(a.socket.as_deref(), Some(Path::new("/tmp/v.sock")));
        let inside = world.join("viewer.sock");
        let fork = scratch("socket");
        let a = parse(&[
            world.to_str().unwrap(),
            "--fork",
            fork.to_str().unwrap(),
            "--socket",
            inside.to_str().unwrap(),
        ])
        .unwrap();
        let e = run(&a).unwrap_err();
        assert!(e.contains("inside the world folder"), "{e}");
        assert!(!fork.exists(), "refused before the export");
        assert!(!inside.exists());
    }

    #[test]
    fn a_fork_resumes_only_with_the_world_it_came_from() {
        let fork = scratch("resume").join("fork");
        fork::export(&fixture("core"), &fork).unwrap();
        let other = fixture("create");
        let a = parse(&[other.to_str().unwrap(), "--fork", fork.to_str().unwrap()]).unwrap();
        let e = run(&a).unwrap_err();
        assert!(e.contains("was exported from"), "{e}");
        let _ = std::fs::remove_dir_all(fork.parent().unwrap());
    }
}
