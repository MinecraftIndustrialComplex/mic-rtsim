// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli view` as the op runs it: the binary, started on a save, asked over its socket
//! (VIEWER.md §2, §4.1, §16 V1's exit test).
//!
//! - On a GameTest save: loopback only, a forged Host refused, anything but GET refused, a public
//!   bind refused at start, a Unix socket served, and the folder left byte for byte.
//! - At 100,000 synthetic districts (the industry benches' world, through the real persist path):
//!   a full-world request returns within the mark budget, and the process's peak resident size
//!   stays under its memory cap.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::learn::Learned;
use rtsim_industry::obs::{DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, SweepEnd, Timed};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};

/// The 2,000 marks a request may carry (VIEWER.md §6, mic-viewer's `MARK_BUDGET`).
const MARK_BUDGET: usize = mic_viewer::api::MARK_BUDGET;

/// A `rtsim-cli view` this test started; killed when dropped (only this child, never anything
/// else on the machine).
struct Sidecar {
    child: Child,
    port: Option<u16>,
    said: Vec<String>,
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Sidecar {
    /// Starts the sidecar and reads its stdout to the "serving" line.
    fn start(world: &Path, args: &[&str]) -> Sidecar {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rtsim-cli"))
            .arg("view")
            .arg(world)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start rtsim-cli view");
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut said = Vec::new();
        let port = loop {
            let line = match lines.next() {
                Some(Ok(l)) => l,
                _ => panic!(
                    "rtsim-cli view stopped before serving ({:?}): {said:#?}",
                    child.wait()
                ),
            };
            said.push(line.clone());
            if let Some(rest) = line.strip_prefix("serving http://127.0.0.1:") {
                break Some(rest.split('/').next().unwrap().parse().unwrap());
            }
            if line.starts_with("serving unix:") {
                break None;
            }
        };
        // Drain what follows so the pipe never fills.
        std::thread::spawn(move || lines.for_each(drop));
        Sidecar { child, port, said }
    }

    /// The process's resident size now and at its peak, in bytes (Linux).
    fn rss(&self) -> (u64, u64) {
        let s = std::fs::read_to_string(format!("/proc/{}/status", self.child.id())).unwrap();
        let kb = |key: &str| {
            s.lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        };
        (kb("VmRSS:"), kb("VmHWM:"))
    }

    fn get(&self, path: &str) -> (u16, Vec<u8>) {
        raw(
            self.port.unwrap(),
            &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
        )
    }

    fn json(&self, path: &str) -> serde_json::Value {
        let (status, body) = self.get(path);
        assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
        serde_json::from_slice(&body).unwrap()
    }
}

/// One request written as given; (status, body).
fn raw(port: u16, request: &str) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    s.write_all(request.as_bytes()).unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).unwrap();
    split(&out)
}

fn split(out: &[u8]) -> (u16, Vec<u8>) {
    let head_end = out
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a response head");
    let head = String::from_utf8_lossy(&out[..head_end]);
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, out[head_end + 4..].to_vec())
}

/// Every file under `dir` with its bytes and modification time.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let meta = std::fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                todo.push(p.clone());
            }
            let bytes = if meta.is_file() {
                std::fs::read(&p).unwrap()
            } else {
                Vec::new()
            };
            out.insert(p, (bytes, meta.modified().unwrap()));
        }
    }
    out
}

fn core_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../rtsim-view/tests/data/core")
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rtsim-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_sidecar_serves_loopback_reads_only_and_writes_nothing() {
    let world = core_fixture();
    let before = snapshot(&world);
    {
        let s = Sidecar::start(&world, &["--port", "0"]);
        let port = s.port.unwrap();
        assert!(
            s.said.iter().any(|l| l.starts_with("priority: SCHED_IDLE")),
            "idle priority: {:#?}",
            s.said
        );
        let w = s.json("/api/world");
        assert_eq!(w["source"], "save");
        let status = s.json("/api/status");
        assert!(status["modules"].is_array(), "{status}");
        // DNS rebinding: a page on another name reaching the loopback port.
        let (forged, _) = raw(
            port,
            "GET /api/world HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n",
        );
        assert_eq!(forged, 403, "a forged Host is refused");
        // A page on another origin may send a GET (a simple request) but never read the answer:
        // nothing grants it CORS.
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(
            b"GET /api/world HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://evil.example\r\n\
              Connection: close\r\n\r\n",
        )
        .unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();
        let head = String::from_utf8_lossy(&out).to_ascii_lowercase();
        assert!(
            !head.contains("access-control-allow"),
            "no CORS grant: {head}"
        );
        let (post, _) = raw(
            port,
            "POST /api/world HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\
             Connection: close\r\n\r\n{}",
        );
        assert!(post == 405 || post == 403, "a POST is refused: {post}");
        let (page, body) = s.get("/");
        assert_eq!(page, 200);
        assert!(String::from_utf8_lossy(&body).contains("html"));
    }
    assert!(before == snapshot(&world), "the save's folder changed");
}

#[test]
fn the_sidecar_refuses_a_public_bind() {
    let out = Command::new(env!("CARGO_BIN_EXE_rtsim-cli"))
        .args(["view"])
        .arg(core_fixture())
        .args(["--port", "0", "--bind", "0.0.0.0"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("loopback") || err.contains("tailnet"),
        "says why: {err}"
    );
}

#[cfg(unix)]
#[test]
fn the_sidecar_serves_a_unix_socket() {
    use std::os::unix::net::UnixStream;
    let dir = tmpdir("socket");
    let path = dir.join("view.sock");
    let s = Sidecar::start(&core_fixture(), &["--socket", path.to_str().unwrap()]);
    assert!(s.port.is_none());
    let mut c = UnixStream::connect(&path).unwrap();
    c.write_all(b"GET /api/world HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut out = Vec::new();
    c.read_to_end(&mut out).unwrap();
    let (status, body) = split(&out);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- 100,000 districts ------------------------------------------------------------------------

const DISTRICTS: u64 = 100_000;
const ORE: Good = Good(1);
const BAR: Good = Good(2);

/// rtsim-cli's industry bench's district (bench/industry.rs), with two chunks so it has a box on
/// the map: two chests, a learned model, loaded once and unloaded (so it replays).
fn add_district(e: &mut Engine, id: u64) {
    let d = DistrictId(id);
    let at = e.now();
    let mut send = |obs: Obs| e.ingest(&Timed { at, obs });
    let (x, z) = ((id as i32 % 1_000) * 64, (id / 1_000) as i32 * 64);
    send(Obs::District(DistrictDecl {
        district: d,
        revision: 1,
        dim: 0,
        name: 0,
        not_learnable: None,
        inventory_count: 2,
    }));
    send(Obs::DistrictChunks {
        district: d,
        revision: 1,
        part: 0,
        chunks: vec![(x >> 4, z >> 4), ((x + 32) >> 4, z >> 4)],
    });
    for (index, dx) in [(0u32, 0i32), (1, 32)] {
        send(Obs::Inventory(InventoryDecl {
            district: d,
            revision: 1,
            index,
            pos: BlockPos::new(x + dx, 64, z),
            kind: InvKind(InvKind::ITEMS),
            item_slots: 27,
            fluid_capacity: 0,
            energy_capacity: 0,
        }));
    }
    send(Obs::StockSample {
        district: d,
        inventory: 0,
        sweep: 1,
        goods: GoodVec::from_pairs([(ORE, 1_000 + (id % 7) as i64)]),
    });
    send(Obs::StockSample {
        district: d,
        inventory: 1,
        sweep: 1,
        goods: GoodVec::new(),
    });
    send(Obs::SweepEnd(SweepEnd {
        district: d,
        sweep: 1,
        records: 2,
        inventories: 2,
        live: false,
        creative: false,
        overstressed: false,
    }));
    e.adopt_model(
        d,
        Learned {
            at,
            window_ticks: 12_000,
            rates: vec![(ORE, -0.08), (BAR, 0.08)],
            links: vec![],
            confidence: 1.0,
            points: 0,
            longest_gap: 100,
            latency_ticks: 100,
        },
        vec![],
    );
    for (state, ticking) in [(LoadState::Live, 2), (LoadState::Unloaded, 0)] {
        e.ingest(&Timed {
            at,
            obs: Obs::LoadState {
                district: d,
                state,
                ticking_chunks: ticking,
                total_chunks: 2,
            },
        });
    }
}

/// A save holding only the industry engine's pages, `n` districts, committed as the link commits.
fn many_districts(world: &Path, n: u64) -> (usize, usize) {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(ORE, GoodInfo::item("ore", 64));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    for id in 1..=n {
        add_district(&mut e, id);
    }
    let pages = e.snapshot();
    drop(e);
    let mut store = PageStore::open(world.join("rtsim"), Durability::NoSync).unwrap();
    let mut batch = SaveBatch::new();
    for (k, raw) in &pages {
        batch.put(*k, raw);
    }
    store.commit(batch, 1).unwrap();
    (pages.len(), pages.iter().map(|(_, p)| p.len()).sum())
}

#[test]
fn at_100000_districts_a_full_world_request_keeps_the_budget_and_the_sidecar_its_cap() {
    let world = tmpdir("districts");
    let t = Instant::now();
    let (pages, bytes) = many_districts(&world, DISTRICTS);
    eprintln!(
        "{DISTRICTS} districts: {pages} pages ({:.1} MiB raw) written in {:.1} s",
        bytes as f64 / (1u64 << 20) as f64,
        t.elapsed().as_secs_f64()
    );
    let cap_mib = 1024u64;
    let t = Instant::now();
    let s = Sidecar::start(&world, &["--port", "0", "--mem-cap", &cap_mib.to_string()]);
    let started = t.elapsed();
    for l in &s.said {
        eprintln!("  | {l}");
    }
    let w = s.json("/api/world");
    let f = &w["frames"][0];
    let b: Vec<f64> = f["bounds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let layers = s.json("/api/layers");
    let all: Vec<String> = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap().to_string())
        .collect();
    // The whole world on a 1,600 px wide screen, then the whole world at the finest zoom.
    let wide = (b[2] - b[0]) / 1_600.0;
    let mut most = 0;
    let mut slowest = Duration::ZERO;
    let mut districts_total = 0;
    for bpp in [wide, 0.25] {
        let q = format!(
            "/api/marks?frame={}&x0={}&z0={}&x1={}&z1={}&bpp={bpp}&layers={}",
            f["id"].as_str().unwrap(),
            b[0],
            b[1],
            b[2],
            b[3],
            all.join(",")
        );
        for _ in 0..2 {
            let t = Instant::now();
            let a = s.json(&q);
            let took = t.elapsed();
            slowest = slowest.max(took);
            let n: usize = a["layers"]
                .as_object()
                .unwrap()
                .values()
                .map(|l| l["marks"].as_array().unwrap().len())
                .sum();
            districts_total = a["layers"]["industry.districts"]["total"]
                .as_u64()
                .unwrap_or(0);
            eprintln!(
                "  full world at {bpp:.2} bpp: {n} marks of {districts_total} districts in {:.1} ms",
                took.as_secs_f64() * 1e3
            );
            assert!(n <= MARK_BUDGET, "{n} marks over the budget");
            most = most.max(n);
        }
    }
    assert_eq!(districts_total, DISTRICTS, "every district counted");
    // District cards: each restores an engine from three pages.
    let t = Instant::now();
    for id in [1u64, 50_000, DISTRICTS] {
        let c = s.json(&format!("/api/thing/industry.districts/{id}"));
        assert!(!c["title"].as_str().unwrap().is_empty());
    }
    let cards = t.elapsed() / 3;
    let status = s.json("/api/status");
    let heap = status["heap"].as_u64().unwrap_or(0);
    let (rss, hwm) = s.rss();
    let mib = |b: u64| b as f64 / (1u64 << 20) as f64;
    eprintln!(
        "  started in {:.1} s; slowest full-world request {:.1} ms; a district card {:.1} ms; \
         heap {:.0} MiB, resident {:.0} MiB, peak resident {:.0} MiB, cap {cap_mib} MiB",
        started.as_secs_f64(),
        slowest.as_secs_f64() * 1e3,
        cards.as_secs_f64() * 1e3,
        mib(heap),
        mib(rss),
        mib(hwm)
    );
    assert!(most > 0);
    assert!(
        hwm < cap_mib << 20,
        "peak resident {:.0} MiB over the cap",
        mib(hwm)
    );
    assert!(
        slowest < Duration::from_secs(5),
        "a full-world request took {slowest:?}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(&world);
}
