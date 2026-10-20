// SPDX-License-Identifier: AGPL-3.0-or-later
//! The hierarchical timing wheel (PLAN.md §4.3): O(1) schedule, O(1) amortized pop, and a
//! deterministic order for equal times.
//!
//! **Layout.** Eleven levels of 64 slots each cover the whole `u64` tick range (6 bits per level,
//! as in Varghese and Lauck's hierarchical wheels and the Linux and tokio timers). An entry for time
//! `at` lives on the level of the highest 6-bit digit in which `at` differs from `now`, in the slot
//! named by that digit of `at`. A 64-bit occupancy mask per level finds the next non-empty slot with
//! one `trailing_zeros`, so skipping a quiet month costs the same as skipping a tick.
//!
//! **Order.** Entries due at the same tick pop in the order they were scheduled. This holds without
//! any sorting: a slot only ever receives entries in schedule order, because the entries of a
//! higher-level slot move down (cascade) at the moment `now` enters its range, before anything else
//! can be scheduled into that range on a lower level. So every lower slot the cascade fills is empty
//! when it starts.
//!
//! **Cancelling** is lazy, as PLAN.md §4.3 books meetings: an entry carries the generations it
//! depends on, and the consumer drops it when it fires stale. The wheel has no cancel operation, so
//! it needs no back-pointers and its entries can move freely.

use std::collections::VecDeque;

use crate::clock::Tick;
use crate::codec::{self, CodecError, Reader, Writer};

const LEVEL_BITS: u32 = 6;
const SLOTS: usize = 1 << LEVEL_BITS;
const LEVELS: usize = 64usize.div_ceil(LEVEL_BITS as usize);

struct Entry<T> {
    at: u64,
    item: T,
}

pub struct TimingWheel<T> {
    now: u64,
    len: usize,
    occupied: [u64; LEVELS],
    slots: Vec<Vec<Entry<T>>>,
    /// Entries due at `now`, in schedule order.
    ready: VecDeque<Entry<T>>,
    /// Emptied slot buffers, reused so a steady state allocates nothing.
    spare: Vec<Vec<Entry<T>>>,
}

impl<T> Default for TimingWheel<T> {
    fn default() -> Self {
        Self::new(Tick::ZERO)
    }
}

impl<T> TimingWheel<T> {
    pub fn new(now: Tick) -> Self {
        Self {
            now: now.0,
            len: 0,
            occupied: [0; LEVELS],
            slots: (0..LEVELS * SLOTS).map(|_| Vec::new()).collect(),
            ready: VecDeque::new(),
            spare: Vec::new(),
        }
    }

    pub fn now(&self) -> Tick {
        Tick(self.now)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Schedules `item` at `at`. A time in the past is due now: it pops on the next
    /// [`TimingWheel::pop_until`], after everything already due now.
    pub fn schedule(&mut self, at: Tick, item: T) {
        debug_assert!(
            at.0 >= self.now,
            "scheduled in the past: {at:?} < {}",
            self.now
        );
        let e = Entry {
            at: at.0.max(self.now),
            item,
        };
        self.len += 1;
        if e.at == self.now {
            self.ready.push_back(e);
        } else {
            self.place(e);
        }
    }

    #[inline]
    fn place(&mut self, e: Entry<T>) {
        let diff = e.at ^ self.now;
        debug_assert!(diff != 0);
        let level = ((63 - diff.leading_zeros()) / LEVEL_BITS) as usize;
        let slot = ((e.at >> (level as u32 * LEVEL_BITS)) as usize) & (SLOTS - 1);
        let v = &mut self.slots[level * SLOTS + slot];
        if v.capacity() == 0
            && let Some(spare) = self.spare.pop()
        {
            *v = spare;
        }
        v.push(e);
        self.occupied[level] |= 1 << slot;
    }

    /// The earliest non-empty slot: (level, slot, the tick its range starts at). Every entry on a
    /// lower level is due before any entry on a higher one, so the first level with a set bit
    /// above `now`'s digit holds the earliest entry.
    fn earliest_slot(&self) -> Option<(usize, usize, u64)> {
        for level in 0..LEVELS {
            let shift = level as u32 * LEVEL_BITS;
            let digit = (self.now >> shift) & (SLOTS as u64 - 1);
            let above = if digit == SLOTS as u64 - 1 {
                0
            } else {
                !0u64 << (digit + 1)
            };
            debug_assert_eq!(
                self.occupied[level] & !above,
                0,
                "level {level} holds a slot at or behind now's digit"
            );
            let occ = self.occupied[level] & above;
            if occ != 0 {
                let slot = occ.trailing_zeros() as usize;
                let high_shift = shift + LEVEL_BITS;
                let high = if high_shift >= 64 {
                    0
                } else {
                    (self.now >> high_shift) << high_shift
                };
                return Some((level, slot, high | (slot as u64) << shift));
            }
        }
        None
    }

    /// A lower bound on the next due time, in O(levels): exact when something is due now or on
    /// level 0, otherwise the start of the slot that holds the next entry. Enough for a scheduler
    /// deciding whether to wake; [`TimingWheel::next_due`] gives the exact time.
    pub fn next_due_lower_bound(&self) -> Option<Tick> {
        if !self.ready.is_empty() {
            return Some(Tick(self.now));
        }
        self.earliest_slot().map(|(_, _, start)| Tick(start))
    }

    /// The exact next due time. Scans one slot, so it costs the entries in that slot.
    pub fn next_due(&self) -> Option<Tick> {
        if !self.ready.is_empty() {
            return Some(Tick(self.now));
        }
        let (level, slot, start) = self.earliest_slot()?;
        if level == 0 {
            return Some(Tick(start));
        }
        self.slots[level * SLOTS + slot]
            .iter()
            .map(|e| Tick(e.at))
            .min()
    }

    /// Pops the next entry due at or before `limit`, in (time, schedule order). When nothing more
    /// is due, `now` moves to `limit` (it never moves back) and this returns `None`. A `limit`
    /// before `now` pops nothing: everything left is due at `now` or later.
    pub fn pop_until(&mut self, limit: Tick) -> Option<(Tick, T)> {
        if limit.0 < self.now {
            return None;
        }
        loop {
            if let Some(e) = self.ready.pop_front() {
                self.len -= 1;
                return Some((Tick(e.at), e.item));
            }
            match self.earliest_slot() {
                Some((level, slot, start)) if start <= limit.0 => {
                    debug_assert!(start > self.now);
                    self.now = start;
                    self.occupied[level] &= !(1 << slot);
                    let mut v = std::mem::take(&mut self.slots[level * SLOTS + slot]);
                    for e in v.drain(..) {
                        if e.at == self.now {
                            self.ready.push_back(e);
                        } else {
                            self.place(e);
                        }
                    }
                    self.spare.push(v);
                }
                _ => {
                    self.now = self.now.max(limit.0);
                    return None;
                }
            }
        }
    }

    /// Every pending entry with the time it is due, in no particular order. For saving: a caller
    /// that needs a canonical order sorts what this returns.
    pub fn iter(&self) -> impl Iterator<Item = (Tick, &T)> {
        self.ready
            .iter()
            .chain(self.slots.iter().flatten())
            .map(|e| (Tick(e.at), &e.item))
    }

    /// Bytes held on the heap, for the memory budget.
    pub fn heap_bytes(&self) -> usize {
        let entry = std::mem::size_of::<Entry<T>>();
        self.slots
            .iter()
            .map(|v| v.capacity() * entry)
            .sum::<usize>()
            + self
                .spare
                .iter()
                .map(|v| v.capacity() * entry)
                .sum::<usize>()
            + self.slots.capacity() * std::mem::size_of::<Vec<Entry<T>>>()
            + self.ready.capacity() * entry
    }

    /// Every pending entry in the order it will pop: by time, then schedule order. Entries due at
    /// one tick all sit in one container (`ready` or one slot) in schedule order, so a stable
    /// sort of [`TimingWheel::iter`] by time is the pop order.
    pub fn in_pop_order(&self) -> Vec<(Tick, &T)> {
        let mut v: Vec<(Tick, &T)> = self.iter().collect();
        v.sort_by_key(|e| e.0);
        v
    }

    /// Writes `now` and the pending entries in pop order; [`TimingWheel::decode_with`] schedules
    /// them again in that order, so a restored wheel pops exactly as this one would.
    pub fn encode_with(&self, w: &mut Writer, mut f: impl FnMut(&T, &mut Writer)) {
        w.u64(self.now);
        let v = self.in_pop_order();
        w.var_u64(v.len() as u64);
        for (at, item) in v {
            w.var_u64(at.0 - self.now);
            f(item, w);
        }
    }

    pub fn decode_with(
        r: &mut Reader<'_>,
        mut f: impl FnMut(&mut Reader<'_>) -> codec::Result<T>,
    ) -> codec::Result<Self> {
        let now = r.u64()?;
        let mut wheel = TimingWheel::new(Tick(now));
        let n = r.len_prefix(1)?;
        let mut last = now;
        for _ in 0..n {
            let at = now
                .checked_add(r.var_u64()?)
                .ok_or_else(|| CodecError::Invalid("wheel entry past the end of time".into()))?;
            if at < last {
                return Err(CodecError::Invalid("wheel entries out of order".into()));
            }
            last = at;
            let item = f(r)?;
            wheel.schedule(Tick(at), item);
        }
        Ok(wheel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    #[test]
    fn pops_in_time_then_schedule_order() {
        let mut w = TimingWheel::new(Tick(0));
        w.schedule(Tick(5000), 'c');
        w.schedule(Tick(70), 'a');
        w.schedule(Tick(5000), 'd');
        w.schedule(Tick(70), 'b');
        w.schedule(Tick(1 << 40), 'e');
        assert_eq!(w.next_due(), Some(Tick(70)));
        let got: Vec<_> = std::iter::from_fn(|| w.pop_until(Tick(u64::MAX))).collect();
        assert_eq!(
            got,
            vec![
                (Tick(70), 'a'),
                (Tick(70), 'b'),
                (Tick(5000), 'c'),
                (Tick(5000), 'd'),
                (Tick(1 << 40), 'e')
            ]
        );
        assert!(w.is_empty());
    }

    #[test]
    fn equal_times_keep_order_across_cascades() {
        // 'x' is booked far ahead (a high level), 'y' for the same tick once that tick is near.
        let mut w = TimingWheel::new(Tick(0));
        w.schedule(Tick(100_000), 'x');
        assert_eq!(w.pop_until(Tick(99_990)), None);
        w.schedule(Tick(100_000), 'y');
        w.schedule(Tick(99_995), 'w');
        assert_eq!(w.pop_until(Tick(100_000)), Some((Tick(99_995), 'w')));
        assert_eq!(w.pop_until(Tick(100_000)), Some((Tick(100_000), 'x')));
        assert_eq!(w.pop_until(Tick(100_000)), Some((Tick(100_000), 'y')));
        assert_eq!(w.now(), Tick(100_000));
    }

    #[test]
    fn scheduling_now_pops_after_what_is_already_due() {
        let mut w = TimingWheel::new(Tick(10));
        w.schedule(Tick(10), 1);
        w.schedule(Tick(10), 2);
        assert_eq!(w.pop_until(Tick(10)), Some((Tick(10), 1)));
        w.schedule(Tick(10), 3);
        assert_eq!(w.pop_until(Tick(10)), Some((Tick(10), 2)));
        assert_eq!(w.pop_until(Tick(10)), Some((Tick(10), 3)));
        assert_eq!(w.pop_until(Tick(10)), None);
    }

    #[test]
    fn a_limit_in_the_past_pops_nothing() {
        let mut w = TimingWheel::new(Tick(100));
        w.schedule(Tick(100), 'a');
        assert_eq!(w.pop_until(Tick(50)), None);
        assert_eq!(w.now(), Tick(100));
        assert_eq!(w.pop_until(Tick(100)), Some((Tick(100), 'a')));
    }

    #[test]
    fn idle_advance_is_cheap_and_exact() {
        let mut w = TimingWheel::new(Tick(0));
        w.schedule(Tick(u64::MAX - 1), ());
        assert_eq!(w.pop_until(Tick(u64::MAX - 2)), None);
        assert_eq!(w.next_due(), Some(Tick(u64::MAX - 1)));
        assert_eq!(w.pop_until(Tick(u64::MAX)), Some((Tick(u64::MAX - 1), ())));
    }

    #[derive(Clone, Debug)]
    enum Op {
        /// Schedule at now + delta.
        Schedule(u64),
        /// Pop everything due up to now + delta.
        Drain(u64),
    }

    fn delta() -> impl Strategy<Value = u64> {
        prop_oneof![
            0u64..4,
            0u64..200,
            0u64..100_000,
            0u64..(1 << 40),
            Just(64),
            Just(4096),
        ]
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => delta().prop_map(Op::Schedule),
            1 => delta().prop_map(Op::Drain),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]
        /// The wheel pops exactly what a sorted map keyed by (time, schedule sequence) pops.
        #[test]
        fn matches_a_sorted_reference(ops in prop::collection::vec(op(), 1..300), start in 0u64..1_000_000) {
            let mut w = TimingWheel::new(Tick(start));
            let mut model: BTreeMap<(u64, u64), u64> = BTreeMap::new();
            let mut seq = 0u64;
            let mut now = start;
            for op in ops {
                match op {
                    Op::Schedule(d) => {
                        let at = now + d;
                        w.schedule(Tick(at), seq);
                        model.insert((at, seq), seq);
                        seq += 1;
                    }
                    Op::Drain(d) => {
                        let limit = now + d;
                        while let Some((t, item)) = w.pop_until(Tick(limit)) {
                            let ((mt, ms), _) = model.pop_first().expect("wheel popped more than the model");
                            prop_assert_eq!((t.0, item), (mt, ms));
                            prop_assert!(t.0 <= limit);
                        }
                        if let Some((&(mt, _), _)) = model.first_key_value() {
                            prop_assert!(mt > limit);
                        }
                        now = limit;
                        prop_assert_eq!(w.now(), Tick(now));
                    }
                }
                prop_assert_eq!(w.len(), model.len());
                prop_assert_eq!(w.next_due(), model.first_key_value().map(|(&(t, _), _)| Tick(t)));
            }
        }

        /// A wheel saved at any point and read back pops exactly what the original pops, equal
        /// times in schedule order included.
        #[test]
        fn a_saved_wheel_pops_as_the_original(ops in prop::collection::vec(op(), 1..200), cut in 0usize..200, start in 0u64..1_000_000) {
            let mut a = TimingWheel::new(Tick(start));
            let mut now = start;
            let mut seq = 0u64;
            let mut b: Option<TimingWheel<u64>> = None;
            for (i, op) in ops.iter().enumerate() {
                if i == cut {
                    let mut w = Writer::new();
                    a.encode_with(&mut w, |v, w| w.u64(*v));
                    let bytes = w.into_bytes();
                    let mut r = Reader::new(&bytes);
                    b = Some(TimingWheel::decode_with(&mut r, |r| r.u64()).unwrap());
                    r.finish().unwrap();
                }
                for w in std::iter::once(&mut a).chain(b.as_mut()) {
                    match op {
                        Op::Schedule(d) => w.schedule(Tick(now + d), seq),
                        Op::Drain(_) => {}
                    }
                }
                match op {
                    Op::Schedule(_) => seq += 1,
                    Op::Drain(d) => {
                        let limit = now + d;
                        let pa: Vec<_> = std::iter::from_fn(|| a.pop_until(Tick(limit))).collect();
                        if let Some(b) = b.as_mut() {
                            let pb: Vec<_> = std::iter::from_fn(|| b.pop_until(Tick(limit))).collect();
                            prop_assert_eq!(pa, pb);
                        }
                        now = limit;
                    }
                }
            }
            if let Some(mut b) = b {
                let pa: Vec<_> = std::iter::from_fn(|| a.pop_until(Tick(u64::MAX))).collect();
                let pb: Vec<_> = std::iter::from_fn(|| b.pop_until(Tick(u64::MAX))).collect();
                prop_assert_eq!(pa, pb);
            }
        }
    }
}
