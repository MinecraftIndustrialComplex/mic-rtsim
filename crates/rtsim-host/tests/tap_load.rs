// SPDX-License-Identifier: AGPL-3.0-or-later
//! The live tap under load (VIEWER.md §5.4 "Under load", §16 row V4 (a)): the populated world for
//! 100 game days at a server's pace, the whole process pinned to two CPUs with spinning threads
//! competing for them, so steps run late and Java's next server tick often finds the engine still
//! on the last hand-over (that hand-over then holds more than one TICK record). A loaded run can
//! differ from itself (PLAN.md §4.7), so this measures the tap instead of comparing hashes: it
//! stood down at every step whose hand-over held more than one TICK, and no step whose input was
//! ready waited longer than one job (a part's encode).
//!
//! Its own binary: the pinning holds for the whole process.

mod live;

use std::time::Duration;

use live::*;

const DAYS: i64 = 100;

#[test]
fn under_load_the_tap_stands_down_whenever_the_engine_is_behind() {
    let world = populated("load", 3);
    let (a_dir, b_dir) = (fork_of(&world, "load-leased"), fork_of(&world, "load-bare"));
    let cpus = pin_process(2);
    let hogs = Hogs::start(3);
    // A server tick a millisecond, a hundredth of a game day each: faster than the loaded engine
    // keeps up with, so hand-overs bunch.
    let (stride, every) = (240, Duration::from_millis(1));
    let b = serve(&b_dir, DAYS, stride, every);
    let page = Hammer::open(&a_dir);
    let a = serve(&a_dir, DAYS, stride, every);
    let seen = page.close();
    drop(hogs);
    println!("pinned to CPUs {cpus:?} with 3 spinning threads");
    print_served("leased under load", &a);
    print_served("unleased under load", &b);
    println!("the page: {seen:?}");

    let g = |k: &str| a.tap.get(k).copied().unwrap_or(0);
    // The load worked: the engine fell behind, in both runs.
    assert!(
        a.behind > 0 && b.behind > 0,
        "hand-overs holding more than one TICK: leased {}, unleased {}",
        a.behind,
        b.behind
    );
    // Every leased step whose hand-over held more than one TICK stood the tap down, unless the
    // next step already held the state (then the tap never ran at all). The engine counts what
    // Java counted, less the steps the tap met before the lease was first read.
    assert!(g("behind") > 0, "{:?}", a.tap);
    assert!(
        g("behind") <= a.behind,
        "stood down as behind {} times, Java saw {}",
        g("behind"),
        a.behind
    );
    assert!(
        g("behind") + g("lock_busy") + 2 * 40 >= a.behind,
        "stood down as behind {} times (state busy {}), Java saw {}",
        g("behind"),
        g("lock_busy"),
        a.behind
    );
    // No step whose input was ready waited longer than a job; the tap stayed in its budget and a
    // job.
    assert_budget(&a.tap);
    // On a machine already loaded past this test's own load, the engine can be behind at every
    // step the tap meets: then it never encodes, which is what it must do, and there is no encode
    // to time. (Commits are the lockstep tests' to check: here the tap may have run only in the
    // run's last tap period.)
    if g("units") == 0 {
        println!(
            "the tap found no step with slack (the machine was loaded beyond the test's own load): {:?}",
            a.tap
        );
    } else {
        assert!(g("steps_tapped") > 0, "{:?}", a.tap);
    }
    assert_eq!(g("failures"), 0, "{:?}", a.tap);
    assert!(seen.failures.is_empty(), "{:#?}", seen.failures);
    assert_eq!(b.tap.get("units").copied().unwrap_or(0), 0);
    assert!(!b_dir.join("rtsim/view").exists(), "no view store unleased");
    let _ = std::fs::remove_dir_all(&world);
    let _ = std::fs::remove_dir_all(a_dir.parent().unwrap());
    let _ = std::fs::remove_dir_all(b_dir.parent().unwrap());
}
