// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving and restoring the industry engine (src/persist.rs): a run saved and restored at any
//! moment ends in exactly the state of a run that never stopped.

use rtsim_industry::engine::{Engine, Mode};
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

fn cfg() -> IronLine {
    IronLine {
        seed: 5,
        days: 10,
        threats: true,
        control: false,
        mode: Mode::Apply,
        tick_first: true,
    }
}

fn snapshot(e: &Engine) -> Vec<(u64, Vec<u8>)> {
    let mut v: Vec<(u64, Vec<u8>)> = e.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    v.sort();
    v
}

#[test]
fn a_restored_engine_continues_exactly() {
    let c = cfg();
    let mut whole = iron_line(c, Tunables::default());
    run_iron_line(&mut whole, &c);
    let want = snapshot(&whole.engine);
    for cut_days in [2.5, 5.0, 8.0, 9.5] {
        let mut w = iron_line(c, Tunables::default());
        // 8.0 days plus two ticks: reconciles are in flight.
        let cut = Tick((cut_days * TICKS_PER_DAY as f64) as u64 + 2);
        w.run_until(cut);
        let pages = w.engine.snapshot();
        let restored = Engine::restore(pages, Tunables::default()).expect("restore");
        assert_eq!(
            snapshot(&restored),
            snapshot(&w.engine),
            "round trip at day {cut_days}"
        );
        w.engine = restored;
        run_iron_line(&mut w, &c);
        let got = snapshot(&w.engine);
        assert_eq!(
            got.len(),
            want.len(),
            "page count after a restore at day {cut_days}"
        );
        for (a, b) in got.iter().zip(&want) {
            assert_eq!(a.0, b.0);
            assert!(
                a.1 == b.1,
                "page {:#x} differs after a restore at day {cut_days}",
                a.0
            );
        }
        w.conservation().expect("conserves across a restore");
    }
}

#[test]
fn corrupt_pages_are_errors_not_panics() {
    let c = cfg();
    let mut w = iron_line(c, Tunables::default());
    w.run_until(Tick(3 * TICKS_PER_DAY));
    let pages = w.engine.snapshot();
    for (i, (k, b)) in pages.iter().enumerate() {
        for cut in [0, b.len() / 2, b.len().saturating_sub(1)] {
            let mut broken = pages.clone();
            broken[i] = (*k, b[..cut].to_vec());
            assert!(
                Engine::restore(broken, Tunables::default()).is_err(),
                "page {i} cut at {cut}"
            );
        }
    }
}
