// SPDX-License-Identifier: AGPL-3.0-or-later
//! When caravans leave: the departure lattice of one direction of one edge.
//!
//! Flows are rates; caravans are how a player sees them (SCALE.md §3). A caravan departing at
//! boundary `D` carries every consignment of its direction since the previous departure, in every
//! pipeline (goods and money alike), so the caravans on a road partition what is in transit and
//! robbing one cuts exactly its consignments out of the flow.
//!
//! **The period.** Caravans per quantum are the direction's volume in chest slots over a caravan's
//! slots (PLAN.md §8.4: caravans per day = goods rate ÷ capacity), so the period is `slots ÷
//! volume`, at least 1. It is at most the travel time, so a moving flow always has a caravan on
//! the road and every caravan carries a full window while it walks (overnight choice E10). An idle
//! direction keeps scouts at a fixed PLACEHOLDER period (P18): they carry no cargo, only news, so
//! news crosses an idle road at the speed of the occasional traveller, not instantly.
//!
//! **Segments.** The period changes only when the volume does, at an event. Each change starts a
//! segment; the last window of the old period departs as a partial caravan just before it.
//! Departures are a pure function of the segments, so a caravan's identity (edge seed, direction,
//! departure boundary) comes back the same every time it is sampled.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Period {
    pub start: u64,
    pub period: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Lattice {
    segs: VecDeque<Period>,
}

impl Lattice {
    /// A lattice with `period` from boundary `start`.
    pub fn new(start: u64, period: u64) -> Self {
        let mut segs = VecDeque::new();
        segs.push_back(Period {
            start,
            period: period.max(1),
        });
        Lattice { segs }
    }

    pub fn current_period(&self) -> u64 {
        self.segs.back().map_or(1, |p| p.period)
    }

    /// Changes the period from boundary `from` on. Returns whether anything changed.
    pub fn set(&mut self, from: u64, period: u64) -> bool {
        let period = period.max(1);
        let last = self
            .segs
            .back_mut()
            .expect("a lattice always has a segment");
        assert!(
            from >= last.start,
            "lattice set at {from} before {}",
            last.start
        );
        if last.period == period {
            return false;
        }
        if last.start == from {
            last.period = period;
        } else {
            self.segs.push_back(Period {
                start: from,
                period,
            });
        }
        true
    }

    fn seg_of(&self, q: u64) -> Option<usize> {
        (0..self.segs.len())
            .rev()
            .find(|&k| self.segs[k].start <= q)
    }

    fn end_of(&self, k: usize) -> Option<u64> {
        self.segs.get(k + 1).map(|p| p.start)
    }

    /// Whether a caravan departs at boundary `d`.
    pub fn is_departure(&self, d: u64) -> bool {
        let Some(k) = self.seg_of(d) else {
            return false;
        };
        let Period { start, period } = self.segs[k];
        if (d + 1 - start).is_multiple_of(period) {
            return true;
        }
        // The partial caravan just before a change of period.
        self.end_of(k) == Some(d + 1)
    }

    /// The first departure at or after `b`.
    pub fn next_departure(&self, b: u64) -> u64 {
        let k = self.seg_of(b).unwrap_or(0);
        let Period { start, period } = self.segs[k];
        let b = b.max(start);
        // Smallest m >= 1 with start + m·period - 1 >= b.
        let m = (b + 1 - start).div_ceil(period).max(1);
        let regular = start + m * period - 1;
        match self.end_of(k) {
            Some(end) if regular >= end => {
                // The flush at end - 1 is at or after b, because b < end.
                end - 1
            }
            _ => regular,
        }
    }

    /// The consignments the caravan departing at `d` carries: `first..=d`.
    pub fn window(&self, d: u64) -> (u64, u64) {
        debug_assert!(self.is_departure(d), "no caravan departs at {d}");
        let k = self.seg_of(d).unwrap_or(0);
        let Period { start, period } = self.segs[k];
        let into = d + 1 - start;
        if into.is_multiple_of(period) {
            (d + 1 - period, d)
        } else {
            // A flush: from the last regular departure of the segment.
            (start + (into / period) * period, d)
        }
    }

    /// Departures in `lo..=hi`, in order.
    pub fn departures(&self, lo: u64, hi: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut b = lo;
        while b <= hi {
            let d = self.next_departure(b);
            if d > hi {
                break;
            }
            out.push(d);
            b = d + 1;
        }
        out
    }

    pub fn heap_bytes(&self) -> usize {
        self.segs.capacity() * std::mem::size_of::<Period>()
    }

    /// Forgets segments that ended before `b`.
    pub fn trim(&mut self, b: u64) {
        while self.segs.len() >= 2 && self.segs[1].start <= b {
            self.segs.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn regular_and_flush_departures() {
        let mut l = Lattice::new(0, 4);
        assert_eq!(l.departures(0, 12), vec![3, 7, 11]);
        l.set(10, 2);
        // The window 8..=9 leaves as a partial caravan at 9, then every 2 from 10.
        assert_eq!(l.departures(0, 15), vec![3, 7, 9, 11, 13, 15]);
        assert_eq!(l.window(9), (8, 9));
        assert_eq!(l.window(11), (10, 11));
        assert_eq!(l.window(7), (4, 7));
        assert_eq!(l.next_departure(8), 9);
    }

    proptest! {
        /// Windows partition the boundaries: every consignment from the start rides exactly one
        /// caravan.
        #[test]
        fn windows_partition(sets in prop::collection::vec((1u64..10, 1u64..7), 0..6), p0 in 1u64..7) {
            let mut l = Lattice::new(0, p0);
            let mut at = 0;
            for (gap, p) in sets {
                at += gap;
                l.set(at, p);
            }
            let deps = l.departures(0, 80);
            let mut next = 0u64;
            for d in deps {
                prop_assert!(l.is_departure(d));
                let (a, b) = l.window(d);
                prop_assert_eq!(a, next);
                prop_assert_eq!(b, d);
                next = d + 1;
            }
            for b in 0..80 {
                prop_assert_eq!(l.is_departure(b), l.next_departure(b) == b);
            }
        }
    }
}
