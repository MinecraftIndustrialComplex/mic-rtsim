// SPDX-License-Identifier: AGPL-3.0-or-later
//! Staggered wakes (docs/choices-paging-followups.md PF1): a paged lazy table whose regions all
//! book breakpoints on the same boundaries wakes them a page at a time, spread over the period,
//! and delivers a held breakpoint first when something else reaches the region.

use rtsim_kernel::clock::{SlotPeriod, Tick};
use rtsim_kernel::codec::{Reader, Writer};
use rtsim_kernel::lazy::{LazyModel, ModelCodec, PagedLazyTable, RegionId};
use rtsim_kernel::rng::{SlotRng, SystemId};

const Q: u64 = 1_000;
const PER: u64 = 4;

/// Breaks on every boundary of `Q`, and counts the boundaries it settled over and the
/// breakpoints delivered to it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Boundary {
    settled: u64,
    seen: u64,
}

impl LazyModel for Boundary {
    type View = u64;
    fn slot_period(&self) -> Option<SlotPeriod> {
        None
    }
    fn settle(&mut self, from: Tick, to: Tick) {
        self.settled += to.0 / Q - from.0 / Q;
    }
    fn commit_slot(&mut self, _slot: u64, _rng: &mut SlotRng) {
        unreachable!()
    }
    fn view(&self, anchor: Tick, at: Tick) -> u64 {
        self.settled + at.0 / Q - anchor.0 / Q
    }
    fn next_breakpoint(&self, anchor: Tick) -> Option<Tick> {
        Some(Tick((anchor.0 / Q + 1) * Q))
    }
}

struct Codec;

impl ModelCodec<Boundary> for Codec {
    fn put(m: &Boundary, w: &mut Writer) {
        w.u64(m.settled);
        w.u64(m.seen);
    }
    fn get(r: &mut Reader<'_>) -> rtsim_kernel::codec::Result<Boundary> {
        Ok(Boundary {
            settled: r.u64()?,
            seen: r.u64()?,
        })
    }
}

type Table = PagedLazyTable<Boundary, Codec>;

fn table(n: u64) -> (Table, Vec<RegionId<Boundary>>) {
    let mut t =
        Table::new(1, SystemId::named("test.stagger"), Tick::ZERO, 0x7001, PER).staggered(Q);
    let hs = (0..n)
        .map(|k| t.insert(k, Boundary::default(), Tick::ZERO))
        .collect();
    (t, hs)
}

/// Drains every wake up to `limit`, marking each delivery in its model.
fn drain(t: &mut Table, limit: Tick) -> Vec<(Tick, RegionId<Boundary>)> {
    let mut out = Vec::new();
    while let Some((at, h)) = t.next_due(limit) {
        t.apply(h, at, |m, _| m.seen += 1);
        out.push((at, h));
    }
    out
}

#[test]
fn wakes_on_one_boundary_spread_over_the_period_a_page_at_a_time() {
    let (mut t, hs) = table(64);
    let woke = drain(&mut t, Tick(2 * Q - 1));
    assert_eq!(woke.len(), 64, "every region woke once in the quantum");
    let mut times = std::collections::BTreeMap::<Tick, Vec<u64>>::new();
    for (at, h) in &woke {
        assert!(at.0 >= Q && at.0 < 2 * Q, "{at:?} left the quantum");
        assert_eq!(at.0 - Q, t.phase(*h));
        times
            .entry(*at)
            .or_default()
            .push(u64::from(h.index()) / PER);
    }
    // Page-mates share a tick and no two pages do.
    assert_eq!(times.len(), 64 / PER as usize);
    for pages in times.values() {
        assert!(pages.windows(2).all(|w| w[0] == w[1]), "{pages:?}");
    }
    // Spread: no two pages' phases closer than a tenth of the even spacing.
    let ts: Vec<u64> = times.keys().map(|t| t.0).collect();
    let min_gap = ts.windows(2).map(|w| w[1] - w[0]).min().unwrap();
    assert!(min_gap >= Q / 16 / 10, "phases {ts:?}");
    // And the next quantum keeps the same phases.
    let next = drain(&mut t, Tick(3 * Q - 1));
    assert_eq!(next.len(), 64);
    for (at, h) in next {
        assert_eq!(at.0, 2 * Q + t.phase(h));
    }
    assert!(
        hs.iter()
            .all(|h| t.get(*h).unwrap().cell().model().seen == 2)
    );
}

#[test]
fn a_held_breakpoint_is_delivered_first_and_only_once() {
    let (mut t, hs) = table(64);
    let h = *hs
        .iter()
        .find(|h| t.phase(**h) > 10)
        .expect("a page with a phase");
    let p = t.phase(h);
    // Before the boundary nothing is held; at it, the breakpoint is.
    drain(&mut t, Tick(Q - 1));
    assert!(!t.is_held(h, Tick(Q - 1)));
    drain(&mut t, Tick(Q + 5));
    assert!(t.is_held(h, Tick(Q + 5)));
    assert!(t.take_held(h, Tick(Q + 5)));
    t.apply(h, Tick(Q + 5), |m, _| m.seen += 1);
    assert!(!t.take_held(h, Tick(Q + 5)), "taken once");
    // The wake booked at the phase went stale; the next is a quantum on.
    let woke = drain(&mut t, Tick(2 * Q - 1));
    assert!(
        woke.iter().all(|(_, w)| *w != h),
        "the held wake fired again"
    );
    assert_eq!(t.get(h).unwrap().wake(), Some(Tick(2 * Q + p)));
    assert!(t.stats().stale_wakes >= 1);
}

#[test]
fn the_stagger_survives_a_save() {
    let (mut t, hs) = table(64);
    drain(&mut t, Tick(Q + 300));
    let mut w = Writer::new();
    t.encode_meta(&mut w);
    t.encode_rows(&mut w);
    let bytes = w.into_bytes();
    let r = &mut Reader::new(&bytes);
    let (mut u, meta) = Table::decode_meta(r, 0x7001, PER).unwrap();
    u.decode_rows(r, meta).unwrap();
    assert_eq!(u.stagger(), Q);
    for h in &hs {
        assert_eq!(u.phase(*h), t.phase(*h));
    }
    assert_eq!(drain(&mut u, Tick(3 * Q)), drain(&mut t, Tick(3 * Q)));
}

/// A region booked again before its wake is due (a delivery and then an event, or an event on a
/// held breakpoint) leaves one wake on the wheel, not a trail that grows with every event
/// (docs/choices-leg-purge.md LP7).
#[test]
fn rebooking_leaves_one_wake_a_region() {
    let (mut t, hs) = table(64);
    for q in 1..=50 {
        let at = Tick(q * Q + Q / 2);
        drain(&mut t, at);
        for &h in hs.iter().step_by(3) {
            if t.is_held(h, at) {
                assert!(t.take_held(h, at));
            }
            t.apply(h, at, |m, _| m.seen += 1);
        }
        assert_eq!(t.pending_wakes(), hs.len(), "after quantum {q}");
    }
    assert!(
        t.stats().stale_wakes > 0,
        "the superseded wakes are counted"
    );
}
