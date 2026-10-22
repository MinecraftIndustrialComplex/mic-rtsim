// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property tests of the replay (C19, I6) on the kernel's `LazyTable`:
//! - integer conservation, exactly, under random imports, exports and breakpoints;
//! - the composition rule: reads at any times change nothing (SCALE.md §4);
//! - catch-up cost independent of elapsed time (SCALE.md §10).

use proptest::prelude::*;
use rtsim_industry::cluster::{Cluster, MemberSpec, PoolKey, ProcSpec, ProcSpecBody, Run};
use rtsim_industry::ids::{DistrictId, Good};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::conserve::Diffuser;
use rtsim_kernel::lazy::LazyTable;
use rtsim_kernel::rng::SystemId;

const SYS: SystemId = SystemId::named("test.industry.replay");

fn d(n: u64) -> DistrictId {
    DistrictId(n)
}

/// A smelter-like box: `inputs` consumed, one output produced, with an optional target and cap.
#[allow(clippy::too_many_arguments)]
fn boxed(
    id: u64,
    stock: i64,
    rate_in: (i64, u64),
    rate_out: (i64, u64),
    target: Option<(i64, i64)>,
    cap: Option<i64>,
    requires: Option<u64>,
    latency: u64,
) -> MemberSpec {
    let (i, o) = (
        PoolKey::Stock(d(id), Good(1)),
        PoolKey::Stock(d(id), Good(2)),
    );
    MemberSpec {
        district: d(id),
        pools: vec![(i, stock, None), (o, 0, cap)],
        procs: vec![ProcSpec {
            id: 0,
            body: ProcSpecBody::Box(vec![
                (i, Diffuser::per(-rate_in.0, rate_in.1)),
                (o, Diffuser::per(rate_out.0, rate_out.1)),
            ]),
            targets: target.map(|(s, r)| vec![(o, s, r)]).unwrap_or_default(),
            requires: requires.map(d),
            latency,
            enabled: true,
            deadline: None,
            min_run: 0,
        }],
        space: None,
        bounds: Vec::new(),
    }
}

/// One district with two processes its learned coupling split apart (I6): the smelter of
/// [`boxed`] on goods 1 and 2, and a second line on goods 3 and 4 whose output a packager writes
/// into a postbox (a mail link, batches of 16).
fn coupled() -> MemberSpec {
    let mut a = boxed(1, 5_000, (3, 100), (3, 100), None, None, None, 0);
    let (i2, o2) = (PoolKey::Stock(d(1), Good(3)), PoolKey::Stock(d(1), Good(4)));
    let mail = PoolKey::Mail(d(1), 7, 1, Good(4));
    a.pools.push((i2, 2_000, None));
    a.pools.push((o2, 0, None));
    a.pools.push((mail, 0, Some(18 * 16)));
    a.procs.push(ProcSpec {
        id: 1,
        body: ProcSpecBody::Box(vec![
            (i2, Diffuser::per(-2, 100)),
            (o2, Diffuser::per(2, 100)),
        ]),
        targets: vec![],
        requires: None,
        latency: 0,
        enabled: true,
        deadline: None,
        min_run: 0,
    });
    a.procs.push(ProcSpec {
        id: 0x100,
        body: ProcSpecBody::Link {
            from: o2,
            to: mail,
            flow: Diffuser::per(1, 100),
            batch: Some(16),
        },
        targets: vec![],
        requires: None,
        latency: 0,
        enabled: true,
        deadline: None,
        min_run: 0,
    });
    a
}

#[derive(Clone, Debug)]
enum Ev {
    /// Import (or export, negative) into district 1's input.
    Input(i64),
    /// Export (negative) from district 1's output.
    Output(i64),
}

fn world(kind: u8) -> Cluster {
    let mut c = Cluster::new(50);
    match kind % 4 {
        0 => c.add_member(
            boxed(1, 300, (3, 100), (2, 100), None, None, None, 0),
            Tick(0),
        ),
        1 => c.add_member(
            boxed(1, 300, (7, 50), (5, 50), Some((400, 100)), None, None, 100),
            Tick(0),
        ),
        2 => {
            c.add_member(
                boxed(1, 300, (3, 100), (2, 100), None, Some(250), None, 0),
                Tick(0),
            );
            c.add_member(
                boxed(3, 120, (1, 10), (1, 10), None, None, Some(1), 0),
                Tick(0),
            );
        }
        _ => c.add_member(coupled(), Tick(0)),
    }
    c
}

/// Runs a schedule of events, with optional reads; returns the final model and the books check.
fn run(
    kind: u8,
    events: &[(u64, Ev)],
    reads: &[u64],
    end: u64,
) -> (Cluster, Tick, Result<(), String>) {
    let mut t: LazyTable<Cluster> = LazyTable::new(1, SYS, Tick(0));
    let h = t.insert(1, world(kind), Tick(0));
    let mut sched: Vec<(u64, Option<Ev>)> =
        events.iter().map(|(a, e)| (*a, Some(e.clone()))).collect();
    sched.extend(reads.iter().map(|&a| (a, None)));
    sched.sort_by_key(|e| (e.0, e.1.is_none()));
    let mut books = Ok(());
    for (at, ev) in sched {
        let at = Tick(at.min(end));
        while t.next_due(at).is_some() {}
        match ev {
            Some(Ev::Input(n)) => {
                t.apply(h, at, |c, at| {
                    c.add_to_pool(PoolKey::Stock(d(1), Good(1)), n, false, at)
                });
            }
            Some(Ev::Output(n)) => {
                t.apply(h, at, |c, at| {
                    c.add_to_pool(PoolKey::Stock(d(1), Good(2)), n, false, at)
                });
            }
            None => {
                t.read(h, at);
            }
        }
        if books.is_ok() {
            books = t.get(h).unwrap().cell().model().check_books();
        }
    }
    while t.next_due(Tick(end)).is_some() {}
    // Settle to the end through an event that changes nothing.
    t.apply(h, Tick(end), |_, _| ());
    let (_, lazy) = t.remove(h).unwrap();
    let (mut m, anchor) = lazy.into_parts();
    m.settles = 0;
    let _ = m.take_events();
    (m, anchor, books)
}

fn ev() -> impl Strategy<Value = (u64, Ev)> {
    (
        0u64..200_000,
        prop_oneof![
            (-200i64..400).prop_map(Ev::Input),
            (-300i64..0).prop_map(Ev::Output)
        ],
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

    /// Every pool's level is its books, exactly, after any events and breakpoints; no pool goes
    /// negative; links move what they take.
    #[test]
    fn replay_conserves(kind in 0u8..4, events in prop::collection::vec(ev(), 0..40)) {
        let (m, _, books) = run(kind, &events, &[], 250_000);
        prop_assert!(books.is_ok(), "{books:?}");
        prop_assert!(m.check_books().is_ok(), "{:?}", m.check_books());
    }

    /// Reads at any times leave the trajectory exactly as it is without them.
    #[test]
    fn reads_change_nothing(
        kind in 0u8..4,
        events in prop::collection::vec(ev(), 0..30),
        reads in prop::collection::vec(0u64..250_000, 0..30),
    ) {
        let (a, ta, _) = run(kind, &events, &[], 250_000);
        let (b, tb, _) = run(kind, &events, &reads, 250_000);
        prop_assert_eq!(ta, tb);
        prop_assert_eq!(a, b);
    }

    /// Catching up in one step or in pieces (events that change nothing) lands on the same state,
    /// for constraints with one term (exact). Several terms re-anchor their safe-side envelope at
    /// an event, which may stop a process a little earlier: events are inputs (F9).
    #[test]
    fn catch_up_composes(kind in 0u8..2, cut in 1u64..250_000) {
        let (a, _, _) = run(kind, &[], &[], 250_000);
        let (b, _, _) = run(kind, &[(cut, Ev::Input(0))], &[], 250_000);
        prop_assert_eq!(a, b);
    }
}

/// A district with stock for a billion ticks costs the same to catch up after one tick as after
/// a hundred million: one settle, no breakpoints (SCALE.md §10).
#[test]
fn catch_up_cost_is_independent_of_elapsed_time() {
    let mut costs = Vec::new();
    for elapsed in [1u64, 1_000, 1_000_000, 100_000_000] {
        let mut t: LazyTable<Cluster> = LazyTable::new(1, SYS, Tick(0));
        let h = t.insert(
            1,
            {
                let mut c = Cluster::new(50);
                c.add_member(
                    boxed(1, i64::MAX / 4, (3, 100), (2, 100), None, None, None, 0),
                    Tick(0),
                );
                c
            },
            Tick(0),
        );
        t.apply(h, Tick(0), |c, _| c.settles = 0);
        t.reset_stats();
        while t.next_due(Tick(elapsed)).is_some() {}
        let v = t.read(h, Tick(elapsed)).unwrap();
        let made = v.level(PoolKey::Stock(d(1), Good(2)));
        t.apply(h, Tick(elapsed), |_, _| ());
        let settles = t.get(h).unwrap().cell().model().settles;
        let stats = t.stats();
        costs.push((elapsed, settles, stats.breakpoints, made));
    }
    for w in costs.windows(2) {
        assert_eq!(
            (w[0].1, w[0].2),
            (w[1].1, w[1].2),
            "cost moved with elapsed time: {costs:?}"
        );
    }
    // And it really did catch up: 2 per 100 ticks.
    let last = costs.last().unwrap();
    assert!((last.3 - 2_000_000).abs() <= 1, "{costs:?}");
}

/// Breakpoints follow the stock, not elapsed time: a district that starves once has one
/// breakpoint however long it then waits.
#[test]
fn a_starved_district_costs_one_breakpoint() {
    for wait in [10_000u64, 10_000_000] {
        let mut t: LazyTable<Cluster> = LazyTable::new(1, SYS, Tick(0));
        let mut c = Cluster::new(50);
        c.add_member(
            boxed(1, 30, (3, 100), (2, 100), None, None, None, 0),
            Tick(0),
        );
        let h = t.insert(1, c, Tick(0));
        t.reset_stats();
        while t.next_due(Tick(wait)).is_some() {}
        assert_eq!(t.stats().breakpoints, 1, "wait {wait}");
        let run = t.get(h).unwrap().cell().model().run_of(d(1), 0);
        assert!(matches!(run, Some(Run::Stopped(_))));
    }
}

/// The sink of a power line stops at its source's stop breakpoint and restarts with it (I13).
#[test]
fn power_coupling_follows_the_source() {
    let mut t: LazyTable<Cluster> = LazyTable::new(1, SYS, Tick(0));
    let mut c = Cluster::new(1);
    c.add_member(
        boxed(1, 30, (3, 100), (2, 100), None, None, None, 0),
        Tick(0),
    );
    c.add_member(
        boxed(2, 1_000_000, (1, 10), (1, 10), None, None, Some(1), 0),
        Tick(0),
    );
    let h = t.insert(1, c, Tick(0));
    let (at, _) = t.next_due(Tick(1_000_000)).expect("the source starves");
    let m = t.get(h).unwrap().cell().model();
    assert!(
        matches!(m.run_of(d(2), 0), Some(Run::Stopped(_))),
        "sink stops at {at:?}"
    );
    while t.next_due(Tick(5_000)).is_some() {}
    t.apply(h, Tick(5_000), |c, at| {
        c.add_to_pool(PoolKey::Stock(d(1), Good(1)), 600, false, at)
    });
    let m = t.get(h).unwrap().cell().model();
    assert_eq!(m.run_of(d(1), 0), Some(Run::Running));
    assert_eq!(m.run_of(d(2), 0), Some(Run::Running));
}

/// Two coupling groups of one district stop apart (I6): when the first runs out of its input the
/// second goes on, and the packager link behind it keeps writing whole packages.
#[test]
fn coupling_groups_stop_apart() {
    let mut t: LazyTable<Cluster> = LazyTable::new(1, SYS, Tick(0));
    let mut c = Cluster::new(50);
    let mut m = coupled();
    // A small input for the first group: 30 units at 3 per 100 ticks.
    m.pools[0].1 = 30;
    c.add_member(m, Tick(0));
    let h = t.insert(1, c, Tick(0));
    while t.next_due(Tick(20_000)).is_some() {}
    let v = t.read(h, Tick(20_000)).unwrap();
    assert!(
        matches!(v.run_of(d(1), 0), Some(Run::Stopped(_))),
        "the first group starved"
    );
    assert_eq!(v.run_of(d(1), 1), Some(Run::Running), "the second goes on");
    assert!(v.level(PoolKey::Stock(d(1), Good(3))) < 2_000 - 300);
    t.apply(h, Tick(20_000), |_, _| ());
    t.get(h).unwrap().cell().model().check_books().unwrap();
    assert_eq!(
        t.get(h).unwrap().cell().model().box_run(d(1)),
        Some(Run::Running),
        "the district runs while any group does"
    );
}
