// SPDX-License-Identifier: AGPL-3.0-or-later
//! The live tap's lockstep exit test on the headless host (VIEWER.md §5.4, §16 row V4 (a), (c),
//! (d)): a fork run for 100 game days with the live source attached (its lease held, a page
//! hammering every route, its close view moving round the world so the lease's focus keeps
//! changing) against a fork of the same world run with no lease. The state hashes agree every game
//! day, the hand-overs carried back the same records in the same order, and the sim pages the two
//! closed with are byte for byte the same. The leased run's tap stayed within its budget plus one
//! unit, and its numbers are printed for VIEWER.md §5.2.
//!
//! The server thread's hand-over (row V4 (d)): the lockstep compares every record the hand-overs
//! carried back, in order; `the_server_threads_hand_over_keeps_its_timing_class` runs the same
//! world at a server's pace, leased and not, and compares what a server tick's hand-over costs.
//!
//! The under-load run is `tests/tap_load.rs` (it pins the whole process to two CPUs).

mod live;

use std::sync::Mutex;
use std::time::Duration;

use live::*;

const DAYS: i64 = 100;

/// One test at a time: each measures time, and the other's engine and page would be its load.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// 100 game days leased with a page hammering the live source, against 100 unleased: the same
/// state every day, the same records handed back, the same pages at the end; the leased tap
/// within budget and the page answered on every route.
#[test]
fn a_leased_run_with_a_page_hammering_it_keeps_lockstep_with_an_unleased_one() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let world = populated("lockstep", 3);
    let (a_dir, b_dir) = (
        fork_of(&world, "lockstep-leased"),
        fork_of(&world, "lockstep-bare"),
    );

    let page = Hammer::open(&a_dir);
    let a = run(&a_dir, DAYS, Duration::from_millis(150));
    let seen = page.close();
    let b = run(&b_dir, DAYS, Duration::ZERO);

    print_tap("leased", &a.tap, Some(&a));
    print_tap("unleased", &b.tap, Some(&b));
    println!("the page: {seen:?}");

    assert_lockstep(&a, &b, &a_dir, &b_dir);
    assert_budget(&a.tap);
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

/// Row V4 (d), the timing: the same world for 100 game days at a server's pace (a server tick a
/// millisecond, a hundredth of a game day each), unleased and then leased with the page hammering.
/// The tap runs on the engine's pool after the step has published, the hand-over never waits for
/// a step (a submit the engine is not ready for leaves the buffer open), so a server tick costs
/// the same leased: the same class, its median and its 99th percentile within twice the unleased
/// run's and a little noise.
#[test]
fn the_server_threads_hand_over_keeps_its_timing_class() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let world = populated("timing", 3);
    let (a_dir, b_dir) = (
        fork_of(&world, "timing-leased"),
        fork_of(&world, "timing-bare"),
    );
    let (stride, every) = (240, Duration::from_millis(1));
    let b = serve(&b_dir, DAYS, stride, every);
    let page = Hammer::open(&a_dir);
    let a = serve(&a_dir, DAYS, stride, every);
    let seen = page.close();
    print_served("leased at a server's pace", &a);
    print_served("unleased at a server's pace", &b);
    println!("the page: {seen:?}");

    assert_budget(&a.tap);
    assert!(seen.failures.is_empty(), "{:#?}", seen.failures);
    let g = |r: &Served, k: &str| r.tap.get(k).copied().unwrap_or(0);
    assert_eq!(g(&b, "units"), 0);
    assert!(g(&a, "commits") >= 3, "{:?}", a.tap);
    let ([a50, a99, ..], [b50, b99, ..]) = (percentiles(&a.tick_nanos), percentiles(&b.tick_nanos));
    assert!(
        a50 <= 2.0 * b50 + 20.0,
        "a server tick's median: {a50:.1} µs leased, {b50:.1} unleased"
    );
    assert!(
        a99 <= 2.0 * b99 + 200.0,
        "a server tick's 99th percentile: {a99:.1} µs leased, {b99:.1} unleased"
    );
    let _ = std::fs::remove_dir_all(&world);
    let _ = std::fs::remove_dir_all(a_dir.parent().unwrap());
    let _ = std::fs::remove_dir_all(b_dir.parent().unwrap());
}
