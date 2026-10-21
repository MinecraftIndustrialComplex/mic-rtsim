// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli view <world>`: the save viewer sidecar (VIEWER.md §2, §4.1). Opens the world's
//! `rtsim/` pages read only, draws them with rtsim-view and serves mic-viewer's page on loopback
//! (or a Unix socket), at idle priority, under a memory cap. It writes nothing anywhere.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use mic_viewer::http::{Config, Server};
use mic_viewer::viewer::App;
use rtsim_view::{Limits, Status, View};

const MIB: u64 = 1 << 20;

/// The default memory cap, in MiB (TUNABLE): the decoded save, the card and tile caches, scenes.
pub const DEFAULT_MEM_CAP_MIB: u64 = 1024;

struct Options {
    world: Option<PathBuf>,
    bind: IpAddr,
    port: u16,
    allow_tailnet: bool,
    socket: Option<PathBuf>,
    mem_cap_mib: u64,
    card_cache_mib: Option<u64>,
    tile_cache_mib: Option<u64>,
    surveyor_cache_mib: Option<u64>,
}

const USAGE: &str = "\
  rtsim-cli view <world> [--port N] [--bind ADDR] [--allow-tailnet] [--socket PATH]
                         [--mem-cap MiB] [--card-cache MiB] [--tile-cache MiB]
                         [--surveyor-cache MiB]
      Serves a saved world (its folder, or its rtsim/ folder) to the viewer page at
      http://127.0.0.1:8740/ (VIEWER.md). Reads the pages, data/mic_rtsim_journal.dat,
      the event archive (rtsim/archive) and Surveyor's terrain (data/surveyor) when there;
      writes nothing. --bind takes a loopback address (a tailnet one with --allow-tailnet);
      --socket serves a Unix socket instead. Runs at idle CPU and IO priority. --mem-cap
      (default 1024) refuses a save that does not fit and district cards once the heap
      reaches it; --port 0 picks a free port.";

pub fn cli(args: &[&str]) -> ExitCode {
    let mut o = Options {
        world: None,
        bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: mic_viewer::DEFAULT_PORT,
        allow_tailnet: false,
        socket: None,
        mem_cap_mib: DEFAULT_MEM_CAP_MIB,
        card_cache_mib: None,
        tile_cache_mib: None,
        surveyor_cache_mib: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match *a {
            "--port" => match num(it.next()).and_then(|n| u16::try_from(n).ok()) {
                Some(p) => o.port = p,
                None => return bad("--port takes a port number"),
            },
            "--bind" => match it.next().and_then(|v| v.parse().ok()) {
                Some(ip) => o.bind = ip,
                None => return bad("--bind takes an IP address (127.0.0.1, ::1)"),
            },
            "--help" | "-h" => {
                println!("usage:\n{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--allow-tailnet" => o.allow_tailnet = true,
            "--socket" => match it.next() {
                Some(p) => o.socket = Some(PathBuf::from(p)),
                None => return bad("--socket takes a path"),
            },
            "--mem-cap" => match num(it.next()) {
                Some(m) => o.mem_cap_mib = m,
                None => return bad("--mem-cap takes MiB"),
            },
            "--card-cache" => match num(it.next()) {
                Some(m) => o.card_cache_mib = Some(m),
                None => return bad("--card-cache takes MiB"),
            },
            "--tile-cache" => match num(it.next()) {
                Some(m) => o.tile_cache_mib = Some(m),
                None => return bad("--tile-cache takes MiB"),
            },
            "--surveyor-cache" => match num(it.next()) {
                Some(m) => o.surveyor_cache_mib = Some(m),
                None => return bad("--surveyor-cache takes MiB"),
            },
            w if !w.starts_with("--") && o.world.is_none() => o.world = Some(PathBuf::from(w)),
            other => return bad(&format!("unknown option {other}")),
        }
    }
    let Some(world) = o.world.clone() else {
        return bad("give the world folder");
    };
    // Where it will listen, checked before the save is read: a typo should not cost a full load.
    if o.socket.is_none() && !mic_viewer::http::address_allowed(o.bind, o.allow_tailnet) {
        eprintln!(
            "rtsim-cli view: {} is not a loopback address (reach the viewer with `ssh -L`; \
             tailnet addresses need --allow-tailnet)",
            o.bind
        );
        return ExitCode::from(2);
    }
    if let Some(sock) = &o.socket
        && under(sock, &world)
    {
        eprintln!(
            "rtsim-cli view: the socket would be inside the world folder, and the viewer writes \
             nothing there; put it elsewhere"
        );
        return ExitCode::from(2);
    }
    // Before any thread starts, so every thread inherits it.
    let priority = idle_priority();

    let mut limits = Limits {
        heap_cap: (o.mem_cap_mib > 0).then_some(o.mem_cap_mib * MIB),
        ..Limits::default()
    };
    if let Some(m) = o.card_cache_mib {
        limits.card_cache = m * MIB;
    }
    if let Some(m) = o.tile_cache_mib {
        limits.tile_cache = m * MIB;
    }
    if let Some(m) = o.surveyor_cache_mib {
        limits.surveyor_cache = m * MIB;
    }
    crate::sys::reset_heap_peak();
    let view = match View::open(&world, limits) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rtsim-cli view: {e}");
            return ExitCode::from(1);
        }
    };
    let peak = crate::sys::heap_peak() as u64;
    crate::sys::trim_heap();
    let heap = crate::sys::heap_live() as u64;
    if let Some(cap) = limits.heap_cap
        && heap >= cap
    {
        eprintln!(
            "rtsim-cli view: the save takes {} MiB decoded, over the memory cap of {} MiB \
             (--mem-cap)",
            heap / MIB,
            cap / MIB
        );
        return ExitCode::from(1);
    }
    let view = view.with_heap(Arc::new(|| crate::sys::heap_live() as u64));
    describe(&view, (heap, peak), limits, &priority);

    let config = Config {
        allow_tailnet: o.allow_tailnet,
        name: "rtsim-view".into(),
    };
    let app = Arc::new(App::new(Arc::new(view)));
    let server = match &o.socket {
        #[cfg(unix)]
        Some(path) => Server::start_unix(path, config, app),
        #[cfg(not(unix))]
        Some(_) => {
            eprintln!("rtsim-cli view: --socket needs a Unix system");
            return ExitCode::from(2);
        }
        None => Server::start(SocketAddr::new(o.bind, o.port), config, app),
    };
    let server = match server {
        Ok(s) => s,
        Err(e) => {
            eprintln!("rtsim-cli view: {e}");
            return ExitCode::from(2);
        }
    };
    let user = std::env::var("USER").unwrap_or_else(|_| "<user>".into());
    let host = hostname().unwrap_or_else(|| "<server>".into());
    match server.addr() {
        Some(a) => {
            let scope = if a.ip().is_loopback() {
                "loopback only"
            } else {
                "tailnet"
            };
            println!("serving {} ({scope}; Ctrl-C to stop)", server.bound());
            println!(
                "from your machine: ssh -L {p}:127.0.0.1:{p} {user}@{host}, then open \
                 http://localhost:{p}/",
                p = a.port()
            );
        }
        None => {
            println!("serving {} (Ctrl-C to stop)", server.bound());
            if let Some(p) = &o.socket {
                println!(
                    "from your machine: ssh -L 8740:{} {user}@{host}, then open \
                     http://localhost:8740/",
                    p.display()
                );
            }
        }
    }
    use std::io::Write;
    let _ = std::io::stdout().flush();
    server.join();
    ExitCode::SUCCESS
}

/// Whether `path` would sit inside `folder` (both made absolute, `..` and links resolved as far
/// as they exist).
pub(crate) fn under(path: &std::path::Path, folder: &std::path::Path) -> bool {
    let abs = |p: &std::path::Path| {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        // The socket itself does not exist yet: resolve its folder.
        match (p.parent(), p.file_name()) {
            (Some(dir), Some(name)) if !p.exists() => std::fs::canonicalize(dir)
                .map(|d| d.join(name))
                .unwrap_or(p),
            _ => std::fs::canonicalize(&p).unwrap_or(p),
        }
    };
    abs(path).starts_with(abs(folder))
}

fn bad(why: &str) -> ExitCode {
    eprintln!("rtsim-cli view: {why}\nusage:\n{USAGE}");
    ExitCode::from(64)
}

fn describe(view: &View, (heap, peak): (u64, u64), limits: Limits, priority: &str) {
    let s = view.saved();
    let frames: Vec<&str> = view.frames().collect();
    println!(
        "world: {} ({}; flat, no wrap)",
        s.world_dir.display(),
        frames.join(", ")
    );
    let now = s.now();
    let day = now.div_euclid(24_000);
    let hour = (now.rem_euclid(24_000) / 1000 + 6) % 24;
    println!(
        "pages: save {}, {} pages, game day {day} {hour:02}:00{}",
        s.generation.save_seq,
        s.pages,
        if s.unconfirmed_hidden {
            " (a later save the world never confirmed is not shown)"
        } else {
            ""
        }
    );
    let modules: Vec<String> = s
        .modules
        .iter()
        .map(|m| match &m.status {
            Status::Loaded => format!("{} loaded", m.name),
            Status::Absent => format!("{} absent", m.name),
            Status::Unreadable(e) => format!("{} UNREADABLE ({e})", m.name),
        })
        .collect();
    println!("modules: {}", modules.join(", "));
    for m in &s.modules {
        if let (Some(false), Some((now, Some(saved)))) = (m.hash_matches(), m.hash) {
            println!(
                "WARNING: {} reads back to state hash {now:016x}, but its save wrote \
                 {saved:016x}: the viewer decodes it differently from the engine, so what it \
                 draws may be wrong",
                m.name
            );
        }
    }
    match s.world_model.as_ref().and_then(|w| {
        let g = w.read();
        g.sampled_ref().map(|m| (m.cell_count(), m.region_count()))
    }) {
        Some((cells, regions)) => {
            println!("base map: HABITAT ({cells} cells in {regions} regions)")
        }
        None => println!("base map: none (no world model pages)"),
    }
    if !view.surveyor_layers().is_empty() {
        let names: Vec<&str> = view.surveyor_layers().iter().map(String::as_str).collect();
        println!(
            "terrain: Surveyor at a block a pixel ({})",
            names.join(", ")
        );
    }
    if s.names.missing.is_some() {
        println!("names: no data/mic_rtsim_journal.dat, so dimensions show as ids");
    }
    drop(s);
    // status() takes the save's lock: after the guard above is dropped.
    let status = view.status();
    if let Some(n) = status["events"]["chunks"].as_u64() {
        println!(
            "events: {n} archive chunks, {} kinds",
            status["events"]["kinds"].as_u64().unwrap_or(0)
        );
    }
    let drawing = view.counts().values().filter(|n| **n > 0).count();
    println!(
        "layers: {drawing} of {} draw ({} marks)",
        view.counts().len(),
        view.counts().values().sum::<usize>()
    );
    let cap = limits
        .heap_cap
        .map_or_else(|| "no cap".into(), |c| format!("{} MiB cap", c / MIB));
    println!(
        "memory: {} MiB decoded ({} MiB at most while reading), {cap}; cards cached up to {} \
         MiB, tiles up to {} MiB, Surveyor pictures up to {} MiB",
        heap / MIB,
        peak / MIB,
        limits.card_cache / MIB,
        limits.tile_cache / MIB,
        limits.surveyor_cache / MIB
    );
    println!("priority: {priority}");
}

fn hostname() -> Option<String> {
    let h = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .ok()?;
    let h = h.trim();
    (!h.is_empty()).then(|| h.to_owned())
}

/// Lowers this thread (and every thread it starts) to idle CPU and IO priority (VIEWER.md §5.5).
#[cfg(target_os = "linux")]
fn idle_priority() -> String {
    let param = libc::sched_param { sched_priority: 0 };
    // SAFETY: plain syscalls on the calling thread with a valid parameter block.
    let cpu = unsafe { libc::sched_setscheduler(0, libc::SCHED_IDLE, &param) } == 0;
    const IOPRIO_WHO_PROCESS: libc::c_long = 1;
    const IOPRIO_CLASS_IDLE: libc::c_long = 3;
    const IOPRIO_CLASS_SHIFT: libc::c_long = 13;
    // SAFETY: ioprio_set takes three integers and touches no memory.
    let io = unsafe {
        libc::syscall(
            libc::SYS_ioprio_set,
            IOPRIO_WHO_PROCESS,
            0 as libc::c_long,
            IOPRIO_CLASS_IDLE << IOPRIO_CLASS_SHIFT,
        )
    } == 0;
    let cpu = if cpu {
        "SCHED_IDLE".to_string()
    } else {
        // SAFETY: setpriority on the calling process.
        let ok = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 19) } == 0;
        if ok {
            "nice 19 (SCHED_IDLE refused)".into()
        } else {
            "normal (SCHED_IDLE and nice refused)".into()
        }
    };
    format!(
        "{cpu}, {} IO",
        if io { "idle class" } else { "normal (refused)" }
    )
}

#[cfg(all(unix, not(target_os = "linux")))]
fn idle_priority() -> String {
    // SAFETY: setpriority on the calling process.
    let ok = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 19) } == 0;
    if ok {
        "nice 19".into()
    } else {
        "normal (nice refused)".into()
    }
}

#[cfg(not(unix))]
fn idle_priority() -> String {
    "normal (idle priority is not set on this system)".into()
}
