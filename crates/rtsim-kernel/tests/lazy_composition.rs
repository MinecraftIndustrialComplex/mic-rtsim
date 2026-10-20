// SPDX-License-Identifier: AGPL-3.0-or-later
//! SCALE.md §4: catch_up(t0 → t2) == catch_up(t1 → t2) ∘ catch_up(t0 → t1), and the trajectory of
//! a lazy region does not depend on when, or whether, anybody reads it.

use proptest::prelude::*;
use rtsim_kernel::clock::{SlotPeriod, TICKS_PER_DAY, Tick};
use rtsim_kernel::conserve::Diffuser;
use rtsim_kernel::lazy::{Lazy, LazyModel, LazyTable};
use rtsim_kernel::rng::{SlotRng, StreamKey, SystemId};
use rtsim_kernel::synth::{
    DyadicHerd, DyadicRegion, GrainHeld, HeadCount, Herd, Stock, SynthRegion,
};

const KEY: StreamKey = StreamKey::new(0xC0FFEE, SystemId::named("test.region"), 17);

fn region(seed: u64, herd: [u64; 3], amount: i64, per_day: i64, threshold: i64) -> SynthRegion {
    SynthRegion {
        stock: Stock::new(amount, Diffuser::per(per_day, TICKS_PER_DAY), threshold),
        herd: Herd::new(seed, herd),
    }
}

fn arb_region() -> impl Strategy<Value = SynthRegion> {
    (
        any::<u64>(),
        (0u64..10_000, 0u64..10_000, 0u64..10_000),
        -1000i64..1000,
        0i64..5000,
        0i64..100_000,
    )
        .prop_map(|(s, (y, a, o), amt, rate, th)| region(s, [y, a, o], amt, rate, th))
}

/// A float model: exponential decay settled at slot boundaries plus a random kick per slot. The
/// composition rule holds bit for bit even here, because reads never re-anchor.
#[derive(Clone, Debug, PartialEq)]
struct Decay {
    value: f64,
    rate: f64,
}

impl LazyModel for Decay {
    type View = f64;
    fn slot_period(&self) -> Option<SlotPeriod> {
        Some(SlotPeriod::ticks(1000))
    }
    fn settle(&mut self, from: Tick, to: Tick) {
        self.value *= (-self.rate * (to - from) as f64).exp();
    }
    fn commit_slot(&mut self, _slot: u64, rng: &mut SlotRng) {
        self.value += rng.next_f64();
    }
    fn view(&self, anchor: Tick, at: Tick) -> f64 {
        self.value * (-self.rate * (at - anchor) as f64).exp()
    }
}

fn times(n: usize) -> impl Strategy<Value = Vec<u64>> {
    prop::collection::vec(
        prop_oneof![
            0u64..100,
            0u64..TICKS_PER_DAY * 3,
            0u64..TICKS_PER_DAY * 400
        ],
        n,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// The composition rule, for a region with a random herd and a deterministic stock.
    #[test]
    fn catch_up_composes(r in arb_region(), start in 0u64..TICKS_PER_DAY * 5, d in times(2)) {
        let t0 = Tick(start);
        let t1 = t0 + d[0];
        let t2 = t1 + d[1];
        let mut direct = Lazy::new(r.clone(), t0);
        direct.catch_up(t2, KEY);
        let mut split = Lazy::new(r, t0);
        split.catch_up(t1, KEY);
        split.catch_up(t2, KEY);
        prop_assert_eq!(&direct, &split);
        prop_assert_eq!(direct.view_without_catch_up(t2), split.view_without_catch_up(t2));
    }

    /// Any number of reads in between changes nothing, and lazy equals eager (a catch-up every
    /// slot).
    #[test]
    fn reads_are_invisible(r in arb_region(), start in 0u64..TICKS_PER_DAY, reads in times(12)) {
        let t0 = Tick(start);
        let r0 = r.clone();
        let mut read_often = Lazy::new(r.clone(), t0);
        let mut t = t0;
        let mut views = Vec::new();
        for dt in reads {
            t += dt;
            views.push((t, read_often.read(t, KEY)));
        }
        let mut never_read = Lazy::new(r.clone(), t0);
        let end = never_read.read(t, KEY);
        prop_assert_eq!(read_often.read(t, KEY), end);
        prop_assert_eq!(&read_often, &never_read);

        let mut eager = Lazy::new(r, t0);
        let mut day = t0;
        while day < t {
            day = Tick(((day.0 / TICKS_PER_DAY) + 1) * TICKS_PER_DAY).min(t);
            eager.catch_up(day, KEY);
        }
        prop_assert_eq!(&eager, &never_read);

        // Each read saw what a fresh catch-up to that moment sees.
        for (at, v) in views {
            let mut fresh = Lazy::new(r0.clone(), t0);
            prop_assert_eq!(fresh.read(at, KEY), v);
        }
    }

    /// Floats compose exactly too.
    #[test]
    fn float_models_compose_bit_for_bit(v in 0.0f64..1e6, rate in 0.0f64..1e-3, d in times(3)) {
        let m = Decay { value: v, rate };
        let t0 = Tick(d[0]);
        let t1 = t0 + d[1];
        let t2 = t1 + d[2];
        let mut a = Lazy::new(m.clone(), t0);
        a.catch_up(t2, KEY);
        let mut b = Lazy::new(m, t0);
        b.read(t1, KEY);
        b.catch_up(t2, KEY);
        prop_assert_eq!(a.model().value.to_bits(), b.model().value.to_bits());
        prop_assert_eq!(a.read(t2, KEY).to_bits(), b.read(t2, KEY).to_bits());
    }

    /// A deterministic model never commits on a read, and its projection is exactly what settling
    /// would give: error diffusion composes.
    #[test]
    fn deterministic_projection_is_exact(amount in -10_000i64..10_000, per_day in -5000i64..5000, d in times(2)) {
        let s = Stock::new(amount, Diffuser::per(per_day, TICKS_PER_DAY), i64::MAX);
        let t0 = Tick(d[0]);
        let t1 = t0 + d[1];
        let mut cell = Lazy::new(s, t0);
        let seen = cell.read(t1, KEY);
        prop_assert_eq!(cell.anchor(), t0);
        let mut settled = s;
        LazyModel::settle(&mut settled, t0, t1);
        prop_assert_eq!(seen, settled.amount);
    }
}

#[derive(Clone, Debug)]
enum Ev {
    /// A transfer of grain out of a region (an event, which re-anchors).
    Take { region: usize, amount: i64, dt: u64 },
    /// A read (which must not matter).
    Read { region: usize, dt: u64 },
}

fn ev(n: usize) -> impl Strategy<Value = Ev> {
    prop_oneof![
        (0..n, 0i64..500, 0u64..TICKS_PER_DAY * 4).prop_map(|(region, amount, dt)| Ev::Take {
            region,
            amount,
            dt
        }),
        (0..n, 0u64..TICKS_PER_DAY * 4).prop_map(|(region, dt)| Ev::Read { region, dt }),
    ]
}

/// (time, region index) of every breakpoint that fired, in order.
type BreakpointLog = Vec<(Tick, usize)>;

/// Runs a table through events and breakpoints; returns the final states and the breakpoint log.
fn run(
    regions: &[SynthRegion],
    evs: &[Ev],
    with_reads: bool,
) -> (Vec<(Tick, SynthRegion)>, BreakpointLog) {
    let mut t: LazyTable<SynthRegion> = LazyTable::new(99, SystemId::named("test.table"), Tick(0));
    let ids: Vec<_> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| t.insert(i as u64, r.clone(), Tick(0)))
        .collect();
    let mut log = Vec::new();
    let mut now = Tick(0);
    let fire_until = |t: &mut LazyTable<SynthRegion>, until: Tick, log: &mut Vec<(Tick, usize)>| {
        while let Some((at, h)) = t.next_due(until) {
            let i = ids.iter().position(|x| *x == h).unwrap();
            log.push((at, i));
            // On reaching the threshold, ship half the stock away and raise the threshold.
            t.apply(h, at, |m, _| {
                m.stock.amount -= m.stock.amount / 2;
                m.stock.threshold += 1000;
            });
        }
    };
    for e in evs {
        match *e {
            Ev::Take { region, amount, dt } => {
                now += dt;
                fire_until(&mut t, now, &mut log);
                t.apply(ids[region], now, |m, _| {
                    m.stock.amount -= amount.min(m.stock.amount.max(0))
                });
            }
            Ev::Read { region, dt } => {
                now += dt;
                fire_until(&mut t, now, &mut log);
                if with_reads {
                    t.query(ids[region], now, &HeadCount).unwrap();
                    t.query(ids[region], now, &GrainHeld).unwrap();
                }
            }
        }
    }
    fire_until(&mut t, now, &mut log);
    t.catch_up_all(now);
    let out = ids
        .iter()
        .map(|&h| {
            let c = t.get(h).unwrap().cell();
            (c.anchor(), c.model().clone())
        })
        .collect();
    (out, log)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Breakpoints fire at the same times in the same order, and every region ends in the same
    /// state, whether or not anybody reads in between.
    #[test]
    fn tables_ignore_reads(
        regions in prop::collection::vec(arb_region(), 1..6),
        evs in prop::collection::vec(ev(6), 1..60),
    ) {
        let evs: Vec<Ev> = evs.into_iter().map(|e| match e {
            Ev::Take { region, amount, dt } => Ev::Take { region: region % regions.len(), amount, dt },
            Ev::Read { region, dt } => Ev::Read { region: region % regions.len(), dt },
        }).collect();
        let (a, la) = run(&regions, &evs, true);
        let (b, lb) = run(&regions, &evs, false);
        prop_assert_eq!(la, lb);
        prop_assert_eq!(a, b);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A dyadic region (O17) has nothing to commit, yet its herd is random: any read order sees the
    /// same head counts, and a read long after the last one costs no slot commits.
    #[test]
    fn dyadic_regions_are_read_order_free(seed in any::<u64>(), herd in (0u64..100_000, 0u64..100_000, 0u64..100_000), reads in times(8)) {
        let r = DyadicRegion {
            stock: Stock::new(0, Diffuser::per(50, TICKS_PER_DAY), i64::MAX),
            herd: DyadicHerd::new(seed, 3, 0, [herd.0, herd.1, herd.2]),
            day: 0,
        };
        let mut a = Lazy::new(r.clone(), Tick(0));
        let mut t = Tick(0);
        let mut seen = Vec::new();
        for dt in reads {
            t += dt;
            prop_assert_eq!(a.catch_up(t, KEY), 0);
            seen.push((t, a.read(t, KEY)));
        }
        // Reverse order, fresh cell: the same views.
        let b = Lazy::new(r, Tick(0));
        for (at, v) in seen.into_iter().rev() {
            prop_assert_eq!(b.view_without_catch_up(at), v);
        }
        prop_assert_eq!(&a, &b);
    }
}

/// A breakpoint booked before a save fires at its booked time after a restore, whatever reads did
/// to the anchor in between.
#[test]
fn restored_wakes_keep_their_time() {
    let r = region(9, [500, 500, 500], 0, 10, 25);
    let mut a: LazyTable<SynthRegion> = LazyTable::new(5, SystemId::named("test.table"), Tick(0));
    let h = a.insert(0, r, Tick(0));
    let booked = a
        .get(h)
        .unwrap()
        .wake()
        .expect("the stock books its crossing");
    // Reads move the anchor past day boundaries; the booked wake must not move with it.
    let mid = Tick(booked.0 / 2);
    assert_eq!(a.next_due(mid), None);
    a.read(h, mid).unwrap();
    let row = a.get(h).unwrap();
    let (cell, wake) = (row.cell().clone(), row.wake());
    assert_eq!(wake, Some(booked));

    let mut b: LazyTable<SynthRegion> = LazyTable::new(5, SystemId::named("test.table"), mid);
    let h2 = b.restore(0, cell, wake);
    assert_eq!(b.next_due(Tick(booked.0 - 1)), None);
    assert_eq!(b.next_due(booked), Some((booked, h2)));
    assert_eq!(a.next_due(booked), Some((booked, h)));
    assert_eq!(a.get(h).unwrap().cell(), b.get(h2).unwrap().cell());
}

/// Applying an event at a time whose breakpoints were not drained is the bug that loses
/// crossings; debug builds refuse it.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "before next_due was drained")]
fn events_before_draining_breakpoints_are_refused() {
    let r = region(9, [1, 1, 1], 0, 10, 25);
    let mut t: LazyTable<SynthRegion> = LazyTable::new(5, SystemId::named("test.table"), Tick(0));
    let h = t.insert(0, r, Tick(0));
    let booked = t.get(h).unwrap().wake().unwrap();
    t.apply(h, booked, |m, _| m.stock.amount += 1);
}

/// Runs a table as [`run`] does, but at event `cut` saves it with `encode_with` and goes on with
/// the table read back: the same breakpoints at the same times and the same final states.
fn run_saved_at(regions: &[SynthRegion], evs: &[Ev], cut: usize) -> (Vec<(Tick, SynthRegion)>, BreakpointLog) {
    use rtsim_kernel::codec::{Decode, Encode, Reader, Writer};
    let mut t: LazyTable<SynthRegion> = LazyTable::new(99, SystemId::named("test.table"), Tick(0));
    let ids: Vec<_> = regions
        .iter()
        .enumerate()
        .map(|(i, r)| t.insert(i as u64, r.clone(), Tick(0)))
        .collect();
    let mut log = Vec::new();
    let mut now = Tick(0);
    let fire_until = |t: &mut LazyTable<SynthRegion>, until: Tick, log: &mut Vec<(Tick, usize)>| {
        while let Some((at, h)) = t.next_due(until) {
            let i = ids.iter().position(|x| *x == h).unwrap();
            log.push((at, i));
            t.apply(h, at, |m, _| {
                m.stock.amount -= m.stock.amount / 2;
                m.stock.threshold += 1000;
            });
        }
    };
    for (k, e) in evs.iter().enumerate() {
        if k == cut {
            let mut w = Writer::new();
            t.encode_with(&mut w, |m, w| m.encode(w));
            let bytes = w.into_bytes();
            let mut r = Reader::new(&bytes);
            t = LazyTable::decode_with(&mut r, SynthRegion::decode).unwrap();
            r.finish().unwrap();
        }
        match *e {
            Ev::Take { region, amount, dt } => {
                now += dt;
                fire_until(&mut t, now, &mut log);
                t.apply(ids[region], now, |m, _| {
                    m.stock.amount -= amount.min(m.stock.amount.max(0))
                });
            }
            Ev::Read { region, dt } => {
                now += dt;
                fire_until(&mut t, now, &mut log);
                t.query(ids[region], now, &HeadCount).unwrap();
                t.query(ids[region], now, &GrainHeld).unwrap();
            }
        }
    }
    fire_until(&mut t, now, &mut log);
    t.catch_up_all(now);
    let out = ids
        .iter()
        .map(|&h| {
            let c = t.get(h).unwrap().cell();
            (c.anchor(), c.model().clone())
        })
        .collect();
    (out, log)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A table saved and read back at any point continues exactly as one that never stopped.
    #[test]
    fn a_saved_table_continues_as_the_original(
        regions in prop::collection::vec(arb_region(), 1..6),
        evs in prop::collection::vec(ev(6), 1..60),
        cut in 0usize..60,
    ) {
        let evs: Vec<Ev> = evs.into_iter().map(|e| match e {
            Ev::Take { region, amount, dt } => Ev::Take { region: region % regions.len(), amount, dt },
            Ev::Read { region, dt } => Ev::Read { region: region % regions.len(), dt },
        }).collect();
        let (a, la) = run(&regions, &evs, true);
        let (b, lb) = run_saved_at(&regions, &evs, cut);
        prop_assert_eq!(la, lb);
        prop_assert_eq!(a, b);
    }
}
