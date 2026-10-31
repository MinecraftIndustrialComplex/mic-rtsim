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
    for k in KINDS {
        let c = every::kind(&cs, k).unwrap_or_else(|| panic!("no {k} in the census"));
        assert_eq!(c.sites, w.founded.sites(k), "{k}: {c:?}");
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
