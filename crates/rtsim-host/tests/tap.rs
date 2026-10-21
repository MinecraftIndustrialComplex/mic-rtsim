// SPDX-License-Identifier: AGPL-3.0-or-later
//! The live tap's lockstep exit test on the headless host (VIEWER.md §5.4, §16 row V4 (a), (c),
//! (d)): a fork run for 100 game days with the live source attached (its lease held, a page
//! hammering every route, its close view moving round the world so the lease's focus keeps
//! changing) against a fork of the same world run with no lease. The state hashes agree every game
//! day, the hand-overs carried back the same records in the same order, and the sim pages the two
//! closed with are byte for byte the same. The leased run's tap stayed within its budget plus one
//! unit, and its numbers are printed for VIEWER.md §5.2.
//!
//! The under-load run is `tests/tap_load.rs` (it pins the whole process to two CPUs).

mod live;

use std::time::Duration;

use live::*;

const DAYS: i64 = 100;

fn ms(n: u64) -> f64 {
    n as f64 / 1e6
}

/// The tap's numbers from its counters (VIEWER.md §5.2).
pub fn print_tap(label: &str, r: &Run) {
    let t = &r.tap;
    let g = |k: &str| t.get(k).copied().unwrap_or(0);
    let units = g("units").max(1);
    let steps = g("steps_tapped").max(1);
    println!(
        "{label}: {} steps, {} saves, {:.1} s wall, {:.1} s waiting on steps ({:.3} ms a step)",
        r.steps,
        r.saves,
        r.wall.as_secs_f64(),
        r.waited.as_secs_f64(),
        r.waited.as_secs_f64() * 1e3 / r.steps.max(1) as f64
    );
    println!(
        "{label}: tap in {} steps (stood down {}, behind {}, next already there {}, cut by the \
         next {}, state busy {}); {:.3} ms a tapped step, longest {:.3} ms, budget {:.3} ms; over \
         budget {} times, by at most {:.3} ms",
        g("steps_tapped"),
        g("stood_down"),
        g("behind"),
        g("next_already"),
        g("cut_by_next"),
        g("lock_busy"),
        ms(g("tap_nanos")) / steps as f64,
        ms(g("tap_nanos_max")),
        ms(g("budget_nanos")),
        g("over_budget"),
        ms(g("overrun_nanos_max")),
    );
    println!(
        "{label}: {} units encoded ({} not resident), {} rows, {} bytes; {:.3} ms a unit \
         ({:.1} rows), longest {:.3} ms; longest job {:.3} ms; a ready hand-over waited at most \
         {:.3} ms",
        g("units"),
        g("not_resident"),
        g("rows"),
        g("bytes"),
        ms(g("unit_nanos")) / units as f64,
        g("rows") as f64 / units as f64,
        ms(g("unit_nanos_max")),
        ms(g("job_nanos_max")),
        ms(g("ready_wait_nanos_max")),
    );
    println!(
        "{label}: {} cycles, last {} ms; {} commits ({} deferred, longest {:.3} ms), {} parts \
         written, {} unchanged; {} lease stats, epoch {}, {} deletes, {} failures",
        g("cycles"),
        g("cycle_ms"),
        g("commits"),
        g("commits_deferred"),
        ms(g("commit_nanos_max")),
        g("parts_written"),
        g("parts_unchanged"),
        g("lease_stats"),
        g("epoch"),
        g("deletes"),
        g("failures"),
    );
}

/// The tap stayed within its budget plus one job (a unit's encode, or the archive's): the most a
/// tapped step ran past the budget, and the most a ready hand-over waited, are each at most the
/// longest job.
pub fn assert_budget(r: &Run) {
    let g = |k: &str| r.tap.get(k).copied().unwrap_or(0);
    assert!(g("units") > 0, "the leased run encoded units");
    // The overrun and the wait are each measured over one job, so they cannot pass the longest
    // (the clock reads around the job bound them); a millisecond's slack for the reads themselves.
    assert!(
        g("overrun_nanos_max") <= g("job_nanos_max") + 1_000_000,
        "past the budget by more than a job: {:?}",
        r.tap
    );
    assert!(
        g("tap_nanos_max") <= g("budget_nanos") + g("job_nanos_max") + 1_000_000,
        "a step's tap time is at most the budget and a job: {:?}",
        r.tap
    );
    assert!(
        g("ready_wait_nanos_max") <= g("job_nanos_max") + 1_000_000,
        "a ready hand-over waited more than a job: {:?}",
        r.tap
    );
}

/// 100 game days leased with a page hammering the live source, against 100 unleased: the same
/// state every day, the same records handed back, the same pages at the end; the leased tap
/// within budget and the page answered on every route.
#[test]
fn a_leased_run_with_a_page_hammering_it_keeps_lockstep_with_an_unleased_one() {
    let world = populated("lockstep", 3);
    let (a_dir, b_dir) = (
        fork_of(&world, "lockstep-leased"),
        fork_of(&world, "lockstep-bare"),
    );

    let page = Hammer::open(&a_dir);
    let a = run(&a_dir, DAYS, Duration::from_millis(150));
    let seen = page.close();
    let b = run(&b_dir, DAYS, Duration::ZERO);

    print_tap("leased", &a);
    print_tap("unleased", &b);
    println!("the page: {seen:?}");

    assert_lockstep(&a, &b, &a_dir, &b_dir);
    assert_budget(&a);
    let g = |r: &Run, k: &str| r.tap.get(k).copied().unwrap_or(0);
    // The leased run's tap worked: a lease, cycles, commits; the unleased one's never encoded.
    assert!(g(&a, "epoch") >= 1 && g(&a, "cycles") >= 1, "{:?}", a.tap);
    assert!(g(&a, "commits") >= 3, "{:?}", a.tap);
    assert_eq!(g(&a, "failures"), 0, "{:?}", a.tap);
    assert_eq!(
        (g(&b, "units"), g(&b, "steps_tapped"), g(&b, "commits")),
        (0, 0, 0)
    );
    assert!(
        g(&b, "lease_stats") >= 1,
        "the unleased run looked for a lease"
    );
    assert!(!b_dir.join("rtsim/view").exists(), "no view store unleased");
    // The page: every route answered, the stream got frame notices, the focus moved.
    assert!(seen.failures.is_empty(), "{:#?}", seen.failures);
    for route in [
        "world",
        "layers",
        "marks",
        "thing",
        "events",
        "histogram",
        "status",
    ] {
        assert!(
            seen.answers.get(route).copied().unwrap_or(0) > 0,
            "{route} asked: {seen:?}"
        );
    }
    assert!(seen.frames >= 2, "frame notices: {seen:?}");
    assert!(seen.commits_seen >= 2, "the page saw commits: {seen:?}");
    assert!(
        seen.lease_writes >= 3,
        "focus changes rewrote the lease: {seen:?}"
    );
    let _ = std::fs::remove_dir_all(&world);
    let _ = std::fs::remove_dir_all(a_dir.parent().unwrap());
    let _ = std::fs::remove_dir_all(b_dir.parent().unwrap());
}
