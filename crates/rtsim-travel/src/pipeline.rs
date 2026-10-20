// SPDX-License-Identifier: AGPL-3.0-or-later
//! A pipeline: one good (or one currency) flowing one way along one edge (A13, SCALE.md §3).
//!
//! **Consignments.** At every quantum boundary `q` the origin books a departure of the pipeline's
//! rate at `q` (a *consignment*), and the destination books its arrival at `q + delay`. The rate is
//! a list of segments, each starting at the first boundary it applies to, so departures, arrivals
//! and what is in transit are closed forms over the segments: a road carrying a thousand caravans a
//! day costs what a road carrying one does.
//!
//! **Holes.** A consignment that leaves the flow (its caravan became an individual row because it
//! was robbed, or rescued) is cut out of the pipeline: it no longer counts in transit and never
//! arrives, so totals stay exact (SCALE.md §3: "that departure slot is removed from the flow").
//!
//! **Quotes.** Each segment carries the price the exporter quoted for it, in the payer's currency
//! (PLAN.md §8.4: money flows back at the quoted price), so the destination pays what was agreed
//! when the goods left, whatever its price is when they arrive.

use std::collections::VecDeque;

use rtsim_trade::{Amount, Price};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seg {
    /// The first departure boundary at this rate.
    pub start: u64,
    /// Thousandths per boundary.
    pub rate: Amount,
    pub quote: Price,
}

/// Consignments `from..=to` (departure boundaries) cut out of the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hole {
    pub from: u64,
    pub to: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pipeline {
    /// Quanta from departure to arrival, at least 1.
    pub delay: u64,
    segs: VecDeque<Seg>,
    /// Sorted, disjoint.
    holes: Vec<Hole>,
}

impl Pipeline {
    pub fn new(delay: u64) -> Self {
        assert!(
            delay >= 1,
            "a pipeline needs a delay of at least one quantum"
        );
        Pipeline {
            delay,
            segs: VecDeque::new(),
            holes: Vec::new(),
        }
    }

    pub fn segs(&self) -> impl Iterator<Item = &Seg> {
        self.segs.iter()
    }

    pub fn holes(&self) -> &[Hole] {
        &self.holes
    }

    fn seg_at(&self, q: u64) -> Option<&Seg> {
        // Segments are few (those still in transit), so a reverse scan beats a search.
        self.segs.iter().rev().find(|s| s.start <= q)
    }

    /// The departure rate booked at boundary `q`, holes ignored.
    pub fn rate_at(&self, q: u64) -> Amount {
        self.seg_at(q).map_or(0, |s| s.rate)
    }

    pub fn quote_at(&self, q: u64) -> Price {
        self.seg_at(q).map_or(0, |s| s.quote)
    }

    /// The rate the origin is sending now and from here on (the last segment).
    pub fn current_rate(&self) -> Amount {
        self.segs.back().map_or(0, |s| s.rate)
    }

    pub fn current_quote(&self) -> Price {
        self.segs.back().map_or(0, |s| s.quote)
    }

    pub fn in_hole(&self, q: u64) -> bool {
        self.holes.iter().any(|h| h.from <= q && q <= h.to)
    }

    /// What departs at boundary `q`.
    pub fn departure_at(&self, q: u64) -> Amount {
        if self.in_hole(q) { 0 } else { self.rate_at(q) }
    }

    /// What arrives at boundary `b`.
    pub fn arrival_at(&self, b: u64) -> Amount {
        if b < self.delay {
            return 0;
        }
        self.departure_at(b - self.delay)
    }

    /// The quote on what arrives at boundary `b`.
    pub fn arrival_quote(&self, b: u64) -> Price {
        if b < self.delay {
            return 0;
        }
        self.quote_at(b - self.delay)
    }

    /// Sets the rate from departure boundary `from` on. `from` must not precede the last segment.
    /// Returns whether anything changed (an unchanged rate books no new segment and no event).
    pub fn set(&mut self, from: u64, rate: Amount, quote: Price) -> bool {
        debug_assert!(rate >= 0);
        if let Some(last) = self.segs.back_mut() {
            assert!(
                from >= last.start,
                "pipeline rate set at {from} before its last segment at {}",
                last.start
            );
            if last.rate == rate && (last.quote == quote || rate == 0) {
                return false;
            }
            if last.start == from {
                last.rate = rate;
                last.quote = quote;
                // Merge with the one before if they now agree.
                let n = self.segs.len();
                if n >= 2 {
                    let (p, l) = (self.segs[n - 2], self.segs[n - 1]);
                    if p.rate == l.rate && p.quote == l.quote {
                        self.segs.pop_back();
                    }
                }
                return true;
            }
        } else if rate == 0 {
            return false;
        }
        self.segs.push_back(Seg {
            start: from,
            rate,
            quote,
        });
        true
    }

    /// Sum of departures (holes ignored) over boundaries `lo..=hi`.
    fn sum_rates(&self, lo: u64, hi: u64) -> i128 {
        if hi < lo {
            return 0;
        }
        let mut total = 0i128;
        let n = self.segs.len();
        for k in 0..n {
            let s = self.segs[k];
            let end = if k + 1 < n {
                self.segs[k + 1].start - 1
            } else {
                u64::MAX
            };
            let (a, b) = (s.start.max(lo), end.min(hi));
            if a <= b {
                total += i128::from(s.rate) * i128::from(b - a + 1);
            }
        }
        total
    }

    /// Departures cut out by holes over `lo..=hi`.
    fn sum_holes(&self, lo: u64, hi: u64) -> i128 {
        self.holes
            .iter()
            .map(|h| {
                let (a, b) = (h.from.max(lo), h.to.min(hi));
                if a <= b { self.sum_rates(a, b) } else { 0 }
            })
            .sum()
    }

    /// Departed (holes excluded) over boundaries `lo..=hi`.
    pub fn departed_between(&self, lo: u64, hi: u64) -> i128 {
        self.sum_rates(lo, hi) - self.sum_holes(lo, hi)
    }

    /// What is on the road once boundary `b` is booked: consignments departed at `b - delay + 1`
    /// to `b`, holes excluded.
    pub fn in_transit(&self, b: u64) -> i128 {
        let lo = (b + 1).saturating_sub(self.delay);
        self.departed_between(lo, b)
    }

    /// Cuts consignments `from..=to` out of the flow and returns how much they held. The range
    /// must not overlap an earlier hole.
    pub fn cut(&mut self, from: u64, to: u64) -> Amount {
        if to < from {
            return 0;
        }
        debug_assert!(
            !self.holes.iter().any(|h| h.from <= to && from <= h.to),
            "consignments {from}..={to} cut twice"
        );
        let held = self.sum_rates(from, to);
        if held == 0 {
            return 0;
        }
        let at = self.holes.partition_point(|h| h.from < from);
        self.holes.insert(at, Hole { from, to });
        Amount::try_from(held).expect("cut overflow")
    }

    /// The boundaries after `after` where the arrival rate changes, in order: segment starts and
    /// hole edges, shifted by the delay. The destination must be woken at each.
    pub fn arrival_changes_after(&self, after: u64) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .segs
            .iter()
            .map(|s| s.start + self.delay)
            .chain(
                self.holes
                    .iter()
                    .flat_map(|h| [h.from + self.delay, h.to + 1 + self.delay]),
            )
            .filter(|&b| b > after)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Forgets what has fully arrived by boundary `b`: segments that ended before `b - delay` and
    /// holes before it. Keeps one segment covering `b - delay`, so arrivals stay defined.
    pub fn trim(&mut self, b: u64) {
        let oldest_open = (b + 1).saturating_sub(self.delay);
        while self.segs.len() >= 2 && self.segs[1].start <= oldest_open {
            self.segs.pop_front();
        }
        if self.segs.len() == 1 && self.segs[0].rate == 0 && self.segs[0].start <= oldest_open {
            self.segs.clear();
        }
        self.holes.retain(|h| h.to >= oldest_open);
    }

    /// Nothing in transit and nothing coming.
    pub fn is_idle(&self, b: u64) -> bool {
        self.current_rate() == 0 && self.in_transit(b) == 0
    }

    pub fn heap_bytes(&self) -> usize {
        self.segs.capacity() * std::mem::size_of::<Seg>()
            + self.holes.capacity() * std::mem::size_of::<Hole>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn departures_arrive_after_the_delay() {
        let mut p = Pipeline::new(3);
        assert!(p.set(10, 100, 7));
        assert!(!p.set(10, 100, 7));
        assert_eq!(p.arrival_at(12), 0);
        assert_eq!(p.arrival_at(13), 100);
        assert_eq!(p.in_transit(10), 100);
        assert_eq!(p.in_transit(12), 300);
        assert!(p.set(20, 40, 7));
        assert_eq!(p.in_transit(21), 100 + 2 * 40);
        assert_eq!(p.arrival_changes_after(0), vec![13, 23]);
        // A robbed caravan's consignments 15..=16 never arrive.
        assert_eq!(p.cut(15, 16), 200);
        assert_eq!(p.arrival_at(18), 0);
        assert_eq!(p.arrival_at(20), 100);
        assert_eq!(p.in_transit(16), 100);
        assert_eq!(p.arrival_changes_after(0), vec![13, 18, 20, 23]);
    }

    proptest! {
        /// Booking every boundary one at a time, departed = arrived + in transit + cut, always.
        #[test]
        fn conserved_through_any_schedule(delay in 1u64..8, sets in prop::collection::vec((0u64..5, 0i64..1000), 1..12), cuts in prop::collection::vec((0u64..60, 0u64..4), 0..4)) {
            let mut p = Pipeline::new(delay);
            let mut at = 0;
            for (gap, rate) in sets {
                at += gap;
                p.set(at, rate, 1);
            }
            let mut cut_total = 0i128;
            for (from, len) in cuts {
                let to = from + len;
                if !p.holes.iter().any(|h| h.from <= to && from <= h.to) {
                    cut_total += i128::from(p.cut(from, to));
                }
            }
            let mut departed = 0i128;
            let mut arrived = 0i128;
            for b in 0..120u64 {
                departed += i128::from(p.rate_at(b));
                arrived += i128::from(p.arrival_at(b));
                // Every hole is in the past or present relative to the cut-before-depart check
                // only for this identity's purpose: the cut amount left the flow.
                let cut_so_far: i128 = p.holes.iter().map(|h| p.sum_rates(h.from, h.to.min(b))).sum();
                prop_assert_eq!(departed, arrived + p.in_transit(b) + cut_so_far);
            }
            prop_assert!(cut_total >= 0);
        }

        /// Trimming forgets only what has arrived.
        #[test]
        fn trim_keeps_the_future(delay in 1u64..8, sets in prop::collection::vec((1u64..6, 0i64..1000), 1..12), at in 0u64..80) {
            let mut p = Pipeline::new(delay);
            let mut s = 0;
            for (gap, rate) in sets {
                s += gap;
                p.set(s, rate, 1);
            }
            // Arrivals at `at` itself are already booked; everything after must survive.
            let before: Vec<Amount> = (at + 1..at + 40).map(|b| p.arrival_at(b)).collect();
            let transit = p.in_transit(at);
            p.trim(at);
            let after: Vec<Amount> = (at + 1..at + 40).map(|b| p.arrival_at(b)).collect();
            prop_assert_eq!(before, after);
            prop_assert_eq!(transit, p.in_transit(at));
        }
    }
}
