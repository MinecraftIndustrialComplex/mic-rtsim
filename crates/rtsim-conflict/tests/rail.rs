// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine half of MOWZIE.md's slice M7 (§4.3, ledgers 1 and 17; MZB-300 to MZB-349): trains
//! through a Frostmaw's range are counted as a decayed number of legs per (faction, actor), pushed
//! as one pinned source, and a raid on it goes to the track, not a district. The meter's
//! arithmetic first, then the gate and its wakes, the owners of held passes, the source and the
//! focus, the target, and the page round trip.

use rtsim_conflict::engine::{LineCount, RailMeter};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;

const TAU: f64 = 48_000.0;

fn meter() -> RailMeter {
    RailMeter::new(ActorId::from_parts(7, 1))
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn a_late_pass_counts_as_if_on_time() {
    // The same four exits, in order and with the third and fourth late (reported after the
    // fifth), count the same: adds are exact whatever order they arrive in.
    let exits = [1_000, 9_000, 15_000, 26_000, 40_000];
    let mut in_order = meter();
    for t in exits {
        in_order.add(Tick(t));
    }
    let mut late = meter();
    for t in [1_000, 9_000, 40_000, 15_000, 26_000] {
        late.add(Tick(t));
    }
    let t = Tick(60_000);
    assert!(
        near(in_order.n_at(t), late.n_at(t), 1e-12),
        "{} against {}",
        in_order.n_at(t),
        late.n_at(t)
    );
    // The reference moved only for the on-time ones.
    assert_eq!(in_order.t_ref, Tick(40_000));
    assert_eq!(late.t_ref, Tick(40_000));
    // And a single late pass adds exactly what it would have decayed to.
    let mut m = meter();
    m.add(Tick(10_000));
    let before = m.n;
    m.add(Tick(4_000));
    assert!(near(m.n - before, (-6_000.0 / TAU).exp(), 1e-12));
    assert_eq!(m.t_ref, Tick(10_000), "a late pass leaves the reference");
}

#[test]
fn the_meter_settles_at_n_over_tau() {
    // A leg every `h` ticks settles at n = 1 / (1 - e^(-h/tau)), read just after a leg, and the
    // rate there is that times 24,000 / tau legs a day: close to 24,000 / h for a short headway.
    for h in [600u64, 1_200, 2_400, 6_000] {
        let mut m = meter();
        // Twenty decay times of legs: the start-up's e^-20 is gone.
        let legs = (20.0 * TAU / h as f64) as u64;
        for i in 1..=legs {
            m.add(Tick(i * h));
        }
        let end = Tick(legs * h);
        let settled = 1.0 / (1.0 - (-(h as f64) / TAU).exp());
        assert!(
            near(m.n_at(end), settled, 1e-6),
            "h {h}: {} against {settled}",
            m.n_at(end)
        );
        let per_day = m.per_day(end);
        assert!(
            near(per_day, settled * TICKS_PER_DAY as f64 / TAU, 1e-6),
            "h {h}: {per_day}"
        );
        // Just after a leg the rate runs a half-headway above the mean (n sawtooths from
        // settled - 1 to settled): within 1.3% of the mean legs a day at these headways.
        let mean = TICKS_PER_DAY as f64 / h as f64;
        assert!(
            (per_day - mean).abs() <= mean * (h as f64 / TAU),
            "h {h}: {per_day} against {mean}"
        );
    }
}

#[test]
fn falls_to_is_the_closed_form() {
    let mut m = meter();
    for i in 1..=100 {
        m.add(Tick(i * 1_200));
    }
    let r = m.per_day(m.t_ref);
    // To half the rate: tau ln 2 later, rounded up.
    let half = m.falls_to(r / 2.0);
    let want = m.t_ref.0 + (TAU * 2f64.ln()).ceil() as u64;
    assert_eq!(half, Tick(want));
    // The rate there is at or below the target, one tick before above it.
    assert!(m.per_day(half) <= r / 2.0 * (1.0 + 1e-12));
    assert!(m.per_day(Tick(half.0 - 1)) > r / 2.0 * (1.0 - 1e-12));
    // Already at or below the target: now. A target of nothing is never reached.
    assert_eq!(m.falls_to(r * 2.0), m.t_ref);
    assert!(m.falls_to(0.0).0 > 1 << 60);
    // Retirement: 0.01 legs a day from a rate of 24 is tau ln(2400) ticks on.
    let retire = m.falls_to(0.01);
    let ticks = (retire.0 - m.t_ref.0) as f64;
    let want = TAU * (r / 0.01).ln();
    assert!(near(ticks, want, 1.0), "{ticks} against {want}");
}

#[test]
fn a_line_decays_and_adds_as_a_meter_does() {
    let mut l = LineCount {
        a: 1,
        b: 2,
        n: 1.0,
        t_ref: Tick(1_000),
        point: Pos::new(0, 10, 20),
        y: 64,
        near: None,
    };
    assert!(near(l.n_at(Tick(1_000 + 48_000)), (-1f64).exp(), 1e-12));
    l.add(Tick(1_000 + 48_000));
    assert!(near(l.n, 1.0 + (-1f64).exp(), 1e-12));
    assert_eq!(l.t_ref, Tick(49_000));
    assert!(near(l.per_day(l.t_ref), l.n * 24_000.0 / TAU, 1e-12));
}

fn at(x: i32) -> Pos {
    Pos::new(0, x, 0)
}

#[test]
fn a_line_credit_moves_the_point() {
    let mut m = meter();
    // Either order of the two stations is the same line.
    m.credit_line((9, 4), Tick(1_000), at(100), 64, None);
    m.credit_line((4, 9), Tick(2_000), at(160), 66, None);
    assert_eq!(m.lines.len(), 1);
    let l = m.lines[0];
    assert_eq!((l.a, l.b), (4, 9));
    assert_eq!((l.point, l.y), (at(160), 66), "the latest pass's point");
    assert!(near(l.n, 1.0 + (-1_000.0 / TAU).exp(), 1e-12));
    // A pass reported late counts but leaves the point where the later pass put it (MZB-334).
    m.credit_line((4, 9), Tick(1_500), at(300), 70, None);
    let l = m.lines[0];
    assert_eq!((l.point, l.y), (at(160), 66));
    assert_eq!(l.t_ref, Tick(2_000));
    // A reroute is a second line, and the busiest is the one with more legs.
    m.credit_line((4, 12), Tick(3_000), at(40), 64, None);
    assert_eq!(m.lines.len(), 2);
    assert_eq!(m.busiest(Tick(3_000)).map(|l| (l.a, l.b)), Some((4, 9)));
    m.credit_line((4, 12), Tick(3_100), at(40), 64, None);
    m.credit_line((4, 12), Tick(3_200), at(41), 64, None);
    assert_eq!(m.busiest(Tick(3_200)).map(|l| (l.a, l.b)), Some((4, 12)));
    // A tie goes to the lower pair.
    let mut t = meter();
    t.credit_line((5, 6), Tick(100), at(1), 0, None);
    t.credit_line((1, 2), Tick(100), at(2), 0, None);
    assert_eq!(t.busiest(Tick(100)).map(|l| (l.a, l.b)), Some((1, 2)));
    assert!(meter().busiest(Tick(0)).is_none());
}

#[test]
fn the_seventeenth_line_evicts_the_smallest() {
    let mut m = meter();
    // Sixteen lines, line k credited k + 1 times at the same tick.
    for k in 0..16u64 {
        for _ in 0..=k {
            m.credit_line((k, 100 + k), Tick(5_000), at(k as i32), 0, None);
        }
    }
    assert_eq!(m.lines.len(), 16);
    assert!(
        m.lines
            .windows(2)
            .all(|w| (w[0].a, w[0].b) < (w[1].a, w[1].b))
    );
    // A seventeenth line makes the smallest (line 0, one leg) go.
    m.credit_line((50, 150), Tick(5_000), at(50), 0, None);
    assert_eq!(m.lines.len(), 16);
    assert!(m.lines.iter().all(|l| l.a != 0), "the smallest is gone");
    assert!(m.lines.iter().any(|l| l.a == 50));
    assert!(
        m.lines
            .windows(2)
            .all(|w| (w[0].a, w[0].b) < (w[1].a, w[1].b))
    );
    // The next one evicts the smallest existing line again: line 50, with its one leg (the new
    // line itself is never the one that goes).
    m.credit_line((60, 160), Tick(5_000), at(60), 0, None);
    assert!(m.lines.iter().all(|l| l.a != 50), "the one-leg line goes");
    assert!(m.lines.iter().any(|l| l.a == 60));
    assert_eq!(m.lines.len(), 16);
}
