// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli bench industry`: the industry engine's cost against its declared slopes
//! (INDUSTRY.md §15, SCALE.md §0 and §10).
//!
//! | Sweep | Varies | Fixed | Metric | Declared slope | Why |
//! |---|---|---|---|---|---|
//! | `quiet` | replaying districts with nothing happening | 10 busy districts, one import a day each | CPU per sim day | 0 (flat) | quiet districts are free per tick (§15) |
//! | `quiet` | same | same | live heap | 1 | a district's model, boundary list and books are state |
//! | `busy` | districts that starve and restock daily | | CPU per sim day | 1 | cost grows with breakpoints and arrivals |
//! | `elapsed` | days a replaying district goes unread | one district | CPU per catch-up read | 0 (flat) | catch-up never grows with elapsed time |
//! | `glass_nodes` | nodes per district (10 to 10,000) | one district, ten-node cells | compile + first solve CPU | 1 | GLASSBOX.md §9.3, rounds fixed (`bench/glass.rs`) |
//! | `glass_slack` | same, a forced split per cell | same | same | 1 | the same with an LP per good |
//!
//! Each point runs in this process, after the previous point's engine is dropped, three times
//! (quick: once) keeping the median. CPU is this thread's CPU time. Tolerances are PLACEHOLDERS,
//! as in `bench scale` (P24): a slope may exceed its declaration by 0.1, and a flat metric may vary
//! by at most 4x (max over min) across the sweep.

use std::path::PathBuf;

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::learn::Learned;
use rtsim_industry::obs::{DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, SweepEnd, Timed};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use serde::Serialize;

use crate::sys;

pub struct Options {
    pub quick: bool,
    pub check: bool,
    pub out: Option<PathBuf>,
}

const ORE: Good = Good(1);
const BAR: Good = Good(2);
/// PLACEHOLDER tolerances (P24).
const SLOPE_TOLERANCE: f64 = 0.1;
const FLAT_RATIO: f64 = 4.0;

#[derive(Clone, Debug, Serialize)]
struct Point {
    n: u64,
    cpu_ns_per_day: Option<f64>,
    cpu_ns_per_read: Option<f64>,
    heap_bytes: Option<u64>,
    setup_ms: f64,
    breakpoints: u64,
    settles: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Sweep {
    name: &'static str,
    metric: &'static str,
    declared: f64,
    flat: bool,
    points: Vec<Point>,
    slope: f64,
    ratio: f64,
    pass: bool,
}

/// A district replaying ore into bars at 0.08 a tick, with `ore` in stock.
fn add_district(e: &mut Engine, id: u64, ore: i64, slots: u32) {
    let d = DistrictId(id);
    let at = e.now();
    let send = |e: &mut Engine, obs: Obs| e.ingest(&Timed { at, obs });
    send(
        e,
        Obs::District(DistrictDecl {
            district: d,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 2,
        }),
    );
    for (index, x) in [(0u32, 0i32), (1, 32)] {
        send(
            e,
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 1,
                index,
                pos: BlockPos::new(x + (id as i32 % 1_000) * 64, 64, (id / 1_000) as i32 * 64),
                kind: InvKind(InvKind::ITEMS),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    send(
        e,
        Obs::StockSample {
            district: d,
            inventory: 0,
            sweep: 1,
            goods: GoodVec::from_pairs([(ORE, ore)]),
        },
    );
    send(
        e,
        Obs::StockSample {
            district: d,
            inventory: 1,
            sweep: 1,
            goods: GoodVec::new(),
        },
    );
    send(
        e,
        Obs::SweepEnd(SweepEnd {
            district: d,
            sweep: 1,
            records: 2,
            inventories: 2,
            live: false,
            creative: false,
            overstressed: false,
        }),
    );
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
    send(
        e,
        Obs::LoadState {
            district: d,
            state: LoadState::Live,
            ticking_chunks: 2,
            total_chunks: 2,
        },
    );
    send(
        e,
        Obs::LoadState {
            district: d,
            state: LoadState::Unloaded,
            ticking_chunks: 0,
            total_chunks: 2,
        },
    );
}

fn engine() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(ORE, GoodInfo::item("ore", 64));
    e.define_good(BAR, GoodInfo::item("bar", 64));
    e
}

fn cpu() -> u64 {
    sys::thread_cpu_ns().unwrap_or(0)
}

/// `quiet` districts never break and never get an event; 10 busy ones get one import a day.
fn quiet(n: u64, days: u64) -> Point {
    let heap0 = sys::heap_live();
    let t0 = std::time::Instant::now();
    let mut e = engine();
    for i in 0..10 {
        add_district(&mut e, i + 1, 1_000, 100_000);
    }
    for i in 0..n {
        add_district(&mut e, 1_000 + i, 1_000_000_000_000, 4_000_000);
    }
    let setup_ms = t0.elapsed().as_secs_f64() * 1e3;
    let heap = sys::heap_live().saturating_sub(heap0) as u64;
    let s0 = e.stats;
    let c0 = cpu();
    for day in 1..=days {
        for i in 0..10 {
            e.advance_to(Tick(day * TICKS_PER_DAY - 12_000 + i * 100));
            e.inject(DistrictId(i + 1), ORE, 960);
        }
        e.advance_to(Tick(day * TICKS_PER_DAY));
    }
    let c1 = cpu();
    Point {
        n,
        cpu_ns_per_day: Some((c1 - c0) as f64 / days as f64),
        cpu_ns_per_read: None,
        heap_bytes: Some(heap),
        setup_ms,
        breakpoints: e.stats.breakpoints - s0.breakpoints,
        settles: e.stats.cluster_settles - s0.cluster_settles,
    }
}

/// Every district starves at midday and is restocked at night.
fn busy(n: u64, days: u64) -> Point {
    let t0 = std::time::Instant::now();
    let mut e = engine();
    for i in 0..n {
        add_district(&mut e, i + 1, 960, 4_000_000);
    }
    let setup_ms = t0.elapsed().as_secs_f64() * 1e3;
    let s0 = e.stats;
    let c0 = cpu();
    for day in 1..=days {
        e.advance_to(Tick(day * TICKS_PER_DAY));
        for i in 0..n {
            e.inject(DistrictId(i + 1), ORE, 960);
        }
    }
    let c1 = cpu();
    Point {
        n,
        cpu_ns_per_day: Some((c1 - c0) as f64 / days as f64),
        cpu_ns_per_read: None,
        heap_bytes: None,
        setup_ms,
        breakpoints: e.stats.breakpoints - s0.breakpoints,
        settles: e.stats.cluster_settles - s0.cluster_settles,
    }
}

/// One district left alone for `days`, then read once (an inspection).
fn elapsed(days: u64) -> Point {
    let mut e = engine();
    add_district(&mut e, 1, 1_000_000_000_000, 4_000_000);
    let s0 = e.stats;
    let c0 = cpu();
    let at = Tick(days * TICKS_PER_DAY);
    e.advance_to(at);
    let l = e.inspect(DistrictId(1), at).expect("district");
    let c1 = cpu();
    assert!(l.made_since.get(BAR) > 0);
    Point {
        n: days,
        cpu_ns_per_day: None,
        cpu_ns_per_read: Some((c1 - c0) as f64),
        heap_bytes: None,
        setup_ms: 0.0,
        breakpoints: e.stats.breakpoints - s0.breakpoints,
        settles: e.stats.cluster_settles - s0.cluster_settles,
    }
}

fn median(mut v: Vec<Point>, key: fn(&Point) -> f64) -> Point {
    v.sort_by(|a, b| key(a).total_cmp(&key(b)));
    v.swap_remove(v.len() / 2)
}

fn slope(points: &[(f64, f64)]) -> f64 {
    let pts: Vec<(f64, f64)> = points
        .iter()
        .filter(|p| p.0 > 0.0 && p.1 > 0.0)
        .map(|p| (p.0.log10(), p.1.log10()))
        .collect();
    if pts.len() < 2 {
        return 0.0;
    }
    let n = pts.len() as f64;
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx) * (p.0 - mx)).sum();
    if sxx == 0.0 { 0.0 } else { sxy / sxx }
}

fn sweep(
    name: &'static str,
    metric: &'static str,
    declared: f64,
    flat: bool,
    points: Vec<Point>,
    get: fn(&Point) -> Option<f64>,
) -> Sweep {
    let xy: Vec<(f64, f64)> = points
        .iter()
        .filter_map(|p| get(p).map(|y| (p.n as f64, y)))
        .collect();
    let s = slope(&xy);
    let ys: Vec<f64> = xy.iter().map(|p| p.1).collect();
    let max = ys.iter().cloned().fold(f64::MIN, f64::max);
    let min = ys.iter().cloned().fold(f64::MAX, f64::min);
    let ratio = if min > 0.0 { max / min } else { f64::INFINITY };
    let pass = s <= declared + SLOPE_TOLERANCE && (!flat || ratio <= FLAT_RATIO);
    Sweep {
        name,
        metric,
        declared,
        flat,
        points,
        slope: s,
        ratio,
        pass,
    }
}

pub fn run(opts: Options) -> i32 {
    let repeats = if opts.quick { 1 } else { 3 };
    let sizes: Vec<u64> = if opts.quick {
        vec![10, 100, 1_000, 10_000]
    } else {
        vec![10, 100, 1_000, 10_000, 100_000]
    };
    let days = if opts.quick { 10 } else { 30 };
    let mut out = Vec::new();

    let q: Vec<Point> = sizes
        .iter()
        .map(|&n| {
            median((0..repeats).map(|_| quiet(n, days)).collect(), |p| {
                p.cpu_ns_per_day.unwrap_or(0.0)
            })
        })
        .collect();
    out.push(sweep(
        "quiet",
        "cpu_ns_per_day",
        0.0,
        true,
        q.clone(),
        |p| p.cpu_ns_per_day,
    ));
    out.push(sweep("quiet", "heap_bytes", 1.0, false, q, |p| {
        p.heap_bytes.map(|b| b as f64)
    }));

    let busy_sizes: Vec<u64> = sizes
        .iter()
        .copied()
        .filter(|&n| n <= if opts.quick { 10_000 } else { 100_000 })
        .collect();
    let b: Vec<Point> = busy_sizes
        .iter()
        .map(|&n| {
            median((0..repeats).map(|_| busy(n, days.min(10))).collect(), |p| {
                p.cpu_ns_per_day.unwrap_or(0.0)
            })
        })
        .collect();
    out.push(sweep("busy", "cpu_ns_per_day", 1.0, false, b, |p| {
        p.cpu_ns_per_day
    }));

    let spans: Vec<u64> = if opts.quick {
        vec![1, 10, 100, 1_000, 10_000]
    } else {
        vec![1, 10, 100, 1_000, 10_000, 100_000, 1_000_000]
    };
    let el: Vec<Point> = spans
        .iter()
        .map(|&d| {
            median((0..repeats.max(3)).map(|_| elapsed(d)).collect(), |p| {
                p.cpu_ns_per_read.unwrap_or(0.0)
            })
        })
        .collect();
    out.push(sweep("elapsed", "cpu_ns_per_read", 0.0, true, el, |p| {
        p.cpu_ns_per_read
    }));

    // The glass box's compile and first solve (GLASSBOX.md §9.3): without routing slack (the
    // linear bound meets L, no LP) and with it (a forced split per cell, an LP per good).
    let glass_sizes: Vec<u64> = vec![10, 100, 1_000, 10_000];
    let mut glass_points: Vec<(&'static str, super::glass::GlassPoint)> = Vec::new();
    for (name, slack) in [("glass_nodes", false), ("glass_slack", true)] {
        let pts: Vec<super::glass::GlassPoint> = glass_sizes
            .iter()
            .map(|&n| {
                let mut v: Vec<super::glass::GlassPoint> = (0..repeats)
                    .map(|_| super::glass::point(n, slack))
                    .collect();
                v.sort_by(|a, b| a.cpu_ns.total_cmp(&b.cpu_ns));
                v.swap_remove(v.len() / 2)
            })
            .collect();
        out.push(sweep(
            name,
            "cpu_ns_per_compile",
            1.0,
            false,
            pts.iter()
                .map(|p| Point {
                    n: p.nodes,
                    cpu_ns_per_day: None,
                    cpu_ns_per_read: Some(p.cpu_ns),
                    heap_bytes: None,
                    setup_ms: 0.0,
                    breakpoints: 0,
                    settles: 0,
                })
                .collect(),
            |p| p.cpu_ns_per_read,
        ));
        glass_points.extend(pts.into_iter().map(|p| (name, p)));
    }

    let mut ok = true;
    println!(
        "{:<8} {:<16} {:>10} {:>14} {:>14} {:>9} {:>9}",
        "sweep", "metric", "n", "value", "setup ms", "bps", "settles"
    );
    for s in &out {
        for p in &s.points {
            let v = match s.metric {
                "cpu_ns_per_day" => p.cpu_ns_per_day,
                "heap_bytes" => p.heap_bytes.map(|b| b as f64),
                _ => p.cpu_ns_per_read,
            };
            println!(
                "{:<8} {:<16} {:>10} {:>14.0} {:>14.1} {:>9} {:>9}",
                s.name,
                s.metric,
                p.n,
                v.unwrap_or(f64::NAN),
                p.setup_ms,
                p.breakpoints,
                p.settles
            );
        }
        println!(
            "  slope {:.3} (declared {:.1}{}), max/min {:.2}: {}",
            s.slope,
            s.declared,
            if s.flat { ", flat" } else { "" },
            s.ratio,
            if s.pass { "ok" } else { "EXCEEDS CONTRACT" }
        );
        ok &= s.pass;
    }
    println!(
        "{:<12} {:>8} {:>8} {:>14} {:>12} {:>7} {:>9} {:>10} {:>8}",
        "glass",
        "nodes",
        "edges",
        "compile+solve",
        "solve",
        "rounds",
        "LP solves",
        "LP skipped",
        "LP tight"
    );
    for (name, p) in &glass_points {
        println!(
            "{:<12} {:>8} {:>8} {:>11.0} us {:>9.0} us {:>7} {:>9} {:>10} {:>8}",
            name,
            p.nodes,
            p.edges,
            p.cpu_ns / 1e3,
            p.solve_cpu_ns / 1e3,
            p.rounds,
            p.lp_solves,
            p.lp_skipped,
            p.lp_tight
        );
    }
    if let Some(dir) = &opts.out {
        let _ = std::fs::create_dir_all(dir);
        let (when, stamp) = super::scale::now_utc();
        let path = dir.join(format!(
            "industry-{stamp}-{}.json",
            if opts.quick { "quick" } else { "full" }
        ));
        let doc = serde_json::json!({
            "bench": "industry",
            "when": when,
            "quick": opts.quick,
            "machine": sys::Machine::probe(),
            "slope_tolerance": SLOPE_TOLERANCE,
            "flat_ratio": FLAT_RATIO,
            "sweeps": out,
            "glass": glass_points
                .iter()
                .map(|(name, p)| serde_json::json!({ "sweep": name, "point": p }))
                .collect::<Vec<_>>(),
        });
        if let Err(e) = std::fs::write(
            &path,
            serde_json::to_string_pretty(&doc).expect("serialize"),
        ) {
            eprintln!("cannot write {}: {e}", path.display());
        } else {
            println!("wrote {}", path.display());
        }
    }
    if opts.check && !ok { 1 } else { 0 }
}
