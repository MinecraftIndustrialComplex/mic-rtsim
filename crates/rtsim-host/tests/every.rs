// SPDX-License-Identifier: AGPL-3.0-or-later
//! The long-horizon suite on the every-kind world (EQUILIBRIUM.md §4's S11 row;
//! docs/overnight-choices-equilibrium.md EQC401 to EQC430, EQC463, EQC464): every module in the
//! shipped engine on one map, driven by the fake Java.

use rtsim_host::every::{self, Every, KINDS};
use rtsim_kernel::census::CensusLog;
use rtsim_kernel::page::Durability;

/// Prints a census in `sim census`'s fixed-width format.
fn print_census(day: u64, cs: &[rtsim_kernel::census::KindCensus]) {
    let mut log = CensusLog::new();
    println!("{}", CensusLog::HEADER);
    for l in log.lines(day, cs) {
        println!("{l}");
    }
}

/// EQC402, EQC463: after one game day every expected kind is in the census with the world's site
/// count, members above 0 and capacity above 0, and the engine reports no protocol error.
#[test]
fn every_kind_world_has_every_kind() {
    let mut w = Every::open("has-every-kind", 1, false, Durability::NoSync);
    w.run_to(1);
    let cs = w.census();
    print_census(w.day(), &cs);
    for s in w.sites().iter().filter(|s| s.kind.starts_with("conflict")) {
        println!("{s:?}");
    }
    for k in KINDS {
        let Some(want) = w.founded.expected(k, &cs) else {
            println!("{k}: no census row yet (S9)");
            continue;
        };
        let c = every::kind(&cs, k).unwrap_or_else(|| panic!("no {k} in the census"));
        assert_eq!(c.sites, want, "{k}: {c:?}");
        assert!(c.members > 0, "{k}: {c:?}");
        assert!(c.capacity > 0, "{k}: {c:?}");
    }
    assert!(
        w.engine().protocol_error().is_none(),
        "{:?}",
        w.engine().protocol_error()
    );
    w.close();
}

/// The speed probe (EQC453, UNVERIFIED 1, EQC470): `RTSIM_EVERY_PROBE_DAYS` game days (default
/// 2,000) of the every-kind world at max in the soak's loop (a census every 10 game days, real
/// fsyncs, the soak's player in village 0 for the first day), printing game days a second.
#[test]
#[ignore = "a probe: run alone in release with --ignored --nocapture"]
fn every_kind_world_speed_probe() {
    let days: u64 = std::env::var("RTSIM_EVERY_PROBE_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000);
    let mut w = Every::open("speed-probe", 1, true, Durability::Fsync);
    let t0 = std::time::Instant::now();
    let mut last = (t0, 0u64);
    while w.day() < days {
        let next = (w.day() + 10).min(days);
        w.run_to(next);
        let cs = w.census();
        if next.is_multiple_of(500) || next == days {
            let rate = (next - last.1) as f64 / last.0.elapsed().as_secs_f64();
            println!(
                "day {next}: {rate:.1} game days a second over the last {} days, {:.1} overall; \
                 heap {} KiB",
                next - last.1,
                next as f64 / t0.elapsed().as_secs_f64(),
                mic_native_rt::memory::in_use().max(0) / 1024
            );
            print_census(next, &cs);
            last = (std::time::Instant::now(), next);
        }
    }
    // The herds' books by species (where the level comes from): founded, born, died, starved,
    // eaten, killed, lost; their K and the herds.
    w.engine().with_herds(|h, _| {
        let mut k = [(0u64, 0u64, 0u64); 5];
        for (id, x) in h.herds.iter() {
            let e = &mut k[x.species.index()];
            e.0 += 1;
            e.1 += x.k;
            e.2 += h.count(id, h.now());
        }
        for (i, l) in h.ledger().iter().enumerate() {
            println!(
                "herds {i}: {} herds, K {}, now {}; founded {} born {} died {} starved {} eaten {} killed {} lost {}",
                k[i].0, k[i].1, k[i].2, l.founded, l.born, l.died, l.starved, l.eaten, l.killed, l.lost
            );
        }
    });
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "{days} game days in {secs:.1} s: {:.1} game days a second; 262,980 days would take {:.1} \
         hours; {} steps, {:.0} us a step waiting on the engine",
        days as f64 / secs,
        262_980.0 / (days as f64 / secs) / 3600.0,
        w.host.steps,
        w.host.wait.as_secs_f64() * 1e6 / w.host.steps.max(1) as f64
    );
    w.close();
}

// ---- the long run (EQC410 to EQC418) ------------------------------------------------------------

/// `f` over `items` on up to `threads` threads, results in order.
fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.clamp(1, items.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(x) = items.get(i) else {
                        break;
                    };
                    let r = f(x);
                    out.lock().expect("no panic holding it")[i] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .expect("no panic holding it")
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

/// Threads for the long checks: `RTSIM_EQ_THREADS`, else half the machine.
fn threads() -> usize {
    std::env::var("RTSIM_EQ_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()) / 2)
        .max(1)
}

/// Median and the 5th and 95th percentiles (nearest rank).
fn band(xs: &[f64]) -> (f64, f64, f64) {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN);
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((q * (v.len() - 1) as f64).round() as usize).min(v.len() - 1)];
    (at(0.5), at(0.05), at(0.95))
}

/// A long run's means over the censuses from `from`: per kind its level ([`every::level`]), the
/// fewest herd ranges alive, the pool camps' broken share and largest P, the ports' closed share
/// and whether every port was closed at once, and the world's renewal events a day.
#[derive(Clone, Debug, Default)]
struct Means {
    level: std::collections::BTreeMap<&'static str, f64>,
    ranges_alive_min: f64,
    camps_broken: f64,
    camps_max_p: f64,
    ports_closed: f64,
    ports_all_closed: bool,
    renewals_a_day: f64,
}

fn means(r: &every::LongRun, from: u64) -> Means {
    let late: Vec<&(u64, Vec<rtsim_kernel::census::KindCensus>)> =
        r.censuses.iter().filter(|(d, _)| *d > from).collect();
    let n = late.len().max(1) as f64;
    let mut m = Means {
        ranges_alive_min: 1.0,
        ..Means::default()
    };
    for k in KINDS {
        let x: f64 = late
            .iter()
            .filter_map(|(_, cs)| every::kind(cs, k).map(every::level))
            .sum();
        m.level.insert(k, x / n);
    }
    for (_, cs) in &late {
        if let Some(h) = every::kind(cs, "herd.range") {
            let alive = 1.0 - f64::from(h.empty) / f64::from(h.sites.max(1));
            m.ranges_alive_min = m.ranges_alive_min.min(alive);
        }
        if let Some(c) = every::kind(cs, "conflict.camp") {
            m.camps_broken += f64::from(c.broken) / f64::from(c.sites.max(1)) / n;
            m.camps_max_p = m.camps_max_p.max(c.mean_p());
        }
        if let Some(p) = every::kind(cs, "ship.port") {
            m.ports_closed += f64::from(p.empty) / f64::from(p.sites.max(1)) / n;
            m.ports_all_closed |= p.members == 0;
        }
    }
    let renewals = |cs: &[rtsim_kernel::census::KindCensus]| cs.iter().map(|c| c.renewals).sum::<u64>();
    if let (Some(a), Some(b)) = (late.first(), r.censuses.last()) {
        m.renewals_a_day = renewals(&b.1).saturating_sub(renewals(&a.1)) as f64 / (b.0 - a.0).max(1) as f64;
    }
    m
}

fn print_long(what: &str, rs: &[every::LongRun], from: u64) {
    let ms: Vec<Means> = rs.iter().map(|r| means(r, from)).collect();
    println!("{what} over {} seeds, median [5th, 95th] of each seed's mean after day {from}:", rs.len());
    for k in KINDS {
        let b = band(&ms.iter().map(|m| m.level[k]).collect::<Vec<_>>());
        println!("  {k:<17} {b:.3?}");
    }
    let b = |f: &dyn Fn(&Means) -> f64| band(&ms.iter().map(f).collect::<Vec<_>>());
    println!("  fewest ranges alive {:.3?}", b(&|m| m.ranges_alive_min));
    println!("  camps broken share  {:.3?}", b(&|m| m.camps_broken));
    println!("  ports closed share  {:.4?}", b(&|m| m.ports_closed));
    println!("  renewals a day      {:.3?}", b(&|m| m.renewals_a_day));
    for r in rs {
        println!(
            "  seed {:>2}: {:.1} game days a second; killed {:?}; checked {:?}; {} wrong, {} bounds broken",
            r.seed,
            r.rate,
            r.teeth.killed,
            r.teeth.checked,
            r.teeth.wrong.len(),
            r.bounds.len()
        );
        for w in r.teeth.wrong.iter().take(5) {
            println!("    booked wrong: {w}");
        }
        for b in r.bounds.iter().take(5) {
            println!("    bound: {b}");
        }
    }
}

/// EQC411 to EQC413, EQC418: the roamer for four years on seed 1. It kills (each kill booked as
/// its player's, kind by kind), every rule bound holds on every site, and every herd range stays
/// alive.
#[test]
fn long_horizon_stability_with_kills() {
    let r = every::long_run(1, true, 4 * 96);
    print_long("roamer, 4 years", std::slice::from_ref(&r), 2 * 96);
    assert!(r.teeth.wrong.is_empty(), "{:?}", r.teeth.wrong);
    assert!(
        r.teeth.killed.get("dragon.lair").is_some_and(|&n| n > 0),
        "{:?}",
        r.teeth.killed
    );
    for (k, n) in &r.teeth.killed {
        if *n > 0 {
            assert!(r.teeth.checked.get(k).is_some_and(|&c| c > 0), "{k}: never checked");
        }
    }
    assert!(r.bounds.is_empty(), "{:?}", r.bounds);
    assert_eq!(means(&r, 0).ranges_alive_min, 1.0);
}

/// EQC417: seeds 1 to 30, 60 years, the roamer against nobody on the same seeds, read over years
/// 30 to 60 on the 24-day census: every band of the first table of the S11 records, every kind
/// the roamer kills killed and booked in every seed, and every rule bound held.
#[test]
#[ignore = "hours: run in release with --ignored --nocapture"]
fn long_horizon_stability_with_kills_30_seeds() {
    let jobs: Vec<(u64, bool)> = (1..=30u64).flat_map(|s| [(s, true), (s, false)]).collect();
    let got = par_map(&jobs, threads(), |&(seed, roamer)| {
        every::long_run(seed, roamer, 60 * 96)
    });
    let roam: Vec<every::LongRun> = got.iter().filter(|r| r.roamer).cloned().collect();
    let none: Vec<every::LongRun> = got.iter().filter(|r| !r.roamer).cloned().collect();
    let from = 30 * 96;
    print_long("roamer, 60 years", &roam, from);
    print_long("nobody, 60 years", &none, from);
    let mr: Vec<Means> = roam.iter().map(|r| means(r, from)).collect();
    let mn: Vec<Means> = none.iter().map(|r| means(r, from)).collect();
    let mut bad = Vec::new();
    let mut need = |ok: bool, why: String| {
        if !ok {
            bad.push(why);
        }
    };
    for r in roam.iter().chain(&none) {
        need(r.bounds.is_empty(), format!("seed {} roamer {}: bounds {:?}", r.seed, r.roamer, r.bounds));
        need(r.teeth.wrong.is_empty(), format!("seed {}: booked wrong {:?}", r.seed, r.teeth.wrong));
    }
    for r in &roam {
        for (k, _) in every::ROAMER_CHANCE {
            // The Umvuthi's kills wait for S9's chief path (EQC457).
            if k == "conflict.umvuthi" {
                continue;
            }
            need(
                r.teeth.killed.get(k).is_some_and(|&n| n > 0),
                format!("seed {}: the roamer never killed a {k}", r.seed),
            );
        }
    }
    let lv = |ms: &[Means], k: &str| band(&ms.iter().map(|m| m.level[k]).collect::<Vec<_>>());
    let (hr, hn) = (lv(&mr, "herd.range"), lv(&mn, "herd.range"));
    need((0.73..=0.83).contains(&hn.0), format!("herds nobody {hn:?}"));
    need(hr.1 >= 0.50 && hr.2 < 1.0 && hr.0 < hn.0, format!("herds roamer {hr:?} nobody {hn:?}"));
    for m in mr.iter().chain(&mn) {
        need(m.ranges_alive_min == 1.0, format!("a herd range empty: {m:?}"));
    }
    let (dr, dn) = (lv(&mr, "dragon.lair"), lv(&mn, "dragon.lair"));
    need((0.88..=1.0).contains(&dn.0), format!("lairs nobody {dn:?}"));
    need(
        (0.80..=0.97).contains(&dr.0) && dr.1 >= 0.70 && dr.0 < dn.0,
        format!("lairs roamer {dr:?} nobody {dn:?}"),
    );
    for m in &mn {
        need(m.level["oil.nest"] == 1.0, format!("nests nobody {:?}", m.level["oil.nest"]));
        need(m.camps_broken == 0.0 && m.camps_max_p == 0.0, format!("camps nobody {m:?}"));
    }
    need(lv(&mr, "oil.nest").0 >= 0.86, format!("nests roamer {:?}", lv(&mr, "oil.nest")));
    need(lv(&mn, "village.human").0 >= 0.95, format!("villages nobody {:?}", lv(&mn, "village.human")));
    need(lv(&mr, "village.human").0 >= 0.93, format!("villages roamer {:?}", lv(&mr, "village.human")));
    need(lv(&mn, "village.ribbit").0 >= 0.95, format!("ribbits nobody {:?}", lv(&mn, "village.ribbit")));
    need(lv(&mr, "village.ribbit").0 >= 0.95, format!("ribbits roamer {:?}", lv(&mr, "village.ribbit")));
    need(lv(&mr, "conflict.camp").0 >= 0.91, format!("camps roamer {:?}", lv(&mr, "conflict.camp")));
    let broken = band(&mr.iter().map(|m| m.camps_broken).collect::<Vec<_>>());
    need(broken.0 <= 0.05, format!("camps roamer broken {broken:?}"));
    need(lv(&mn, "conflict.den").0 >= 0.995, format!("dens nobody {:?}", lv(&mn, "conflict.den")));
    need(lv(&mr, "conflict.den").0 >= 0.907, format!("dens roamer {:?}", lv(&mr, "conflict.den")));
    let gn = lv(&mn, "conflict.grove");
    need((0.90..=1.0).contains(&gn.0), format!("groves nobody {gn:?}"));
    need(lv(&mr, "conflict.grove").0 >= 0.866, format!("groves roamer {:?}", lv(&mr, "conflict.grove")));
    need(lv(&mn, "ship.haven").0 >= 0.83, format!("havens nobody {:?}", lv(&mn, "ship.haven")));
    need(lv(&mr, "ship.haven").0 >= 0.78, format!("havens roamer {:?}", lv(&mr, "ship.haven")));
    for m in mr.iter().chain(&mn) {
        need(!m.ports_all_closed && m.ports_closed <= 0.01, format!("ports {m:?}"));
    }
    for (a, b) in mr.iter().zip(&mn) {
        need(a.renewals_a_day > b.renewals_a_day, format!("renewals roamer {a:?} nobody {b:?}"));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
