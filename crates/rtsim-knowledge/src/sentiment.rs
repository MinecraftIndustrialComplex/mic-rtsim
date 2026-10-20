// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sentiment (PLAN.md §5.6, SCALE.md §9): what a holder feels about an actor, an i8 in -127..=127,
//! in 8-byte entries (6-byte handle, i8 value, 1 flag byte), capped per holder.
//!
//! # Decay: stochastic, slower when strong, and lazy
//!
//! A feeling of magnitude m loses one step after a waiting time with mean
//! [`Tunables::mean_step_ticks`]`(m)`, which grows with m: a slight is forgotten in hours, a
//! grudge lasts weeks (the shape of Veloren's `sentiment.rs:53-75`, with our own law). Waiting
//! times are exponential, drawn from the counter RNG keyed by (world seed, `knowledge.sentiment`,
//! holder) with the slot naming (target, anchor), and the k-th step using the k-th draw.
//!
//! Each holder has one anchor tick. Events (a report learned, an adjustment, an inheritance)
//! commit every entry up to their time and move the anchor; reads project from the anchor with
//! the same draws and change nothing. So a read never changes what a later read or event sees
//! (the composition rule, SCALE.md §4), and a projection costs at most 127 draws per entry,
//! whatever the time elapsed. Restarting the waits at each commit is exact in distribution,
//! because exponential waits are memoryless.
//!
//! # Caps
//!
//! When a holder is full, the weakest entry goes, and entries about non-players go before
//! entries about players. A new feeling weaker than everything held is not stored. Veloren's
//! cleanup drains a max-heap of absolute values and so deletes the strongest feelings first
//! (`sentiment.rs:78-96`); that is the bug this order avoids.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::Writer;
use rtsim_kernel::handle::Handle;
use rtsim_kernel::rng::{StreamKey, SystemId};

use crate::actor::ActorId;
use crate::tunables::Tunables;

/// Flag bit 0 of an entry: the target is a player.
pub const SENT_FLAG_PLAYER: u8 = 1;

pub const SENTIMENT_SYSTEM: SystemId = SystemId::named("knowledge.sentiment");

/// One 8-byte entry: target handle (6), value (i8), flags (u8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SentimentEntry(pub [u8; 8]);

const _: () = assert!(std::mem::size_of::<SentimentEntry>() == 8);

impl SentimentEntry {
    pub fn new(target: ActorId, value: i8, flags: u8) -> Self {
        let mut b = [0u8; 8];
        b[0..6].copy_from_slice(&target.to_bytes());
        b[6] = value as u8;
        b[7] = flags;
        SentimentEntry(b)
    }

    pub fn target(&self) -> ActorId {
        Handle::from_bytes(self.0[0..6].try_into().unwrap())
    }

    pub fn value(&self) -> i8 {
        self.0[6] as i8
    }

    pub fn flags(&self) -> u8 {
        self.0[7]
    }

    pub fn is_player(&self) -> bool {
        self.flags() & SENT_FLAG_PLAYER != 0
    }

    fn set_value(&mut self, v: i8) {
        self.0[6] = v as u8;
    }
}

/// SplitMix64's finalizer (Steele, Lea and Flood, "Fast splittable pseudorandom number
/// generators", OOPSLA 2014): mixes (target, anchor) into one slot number.
const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn decay_slot(target: ActorId, anchor: Tick) -> u64 {
    mix64(target.to_bits() ^ mix64(anchor.0.wrapping_add(0x9e37_79b9_7f4a_7c15)))
}

/// `v` decayed from `from` to `to` with the holder's draws.
fn project(v: i8, from: Tick, to: Tick, key: StreamKey, target: ActorId, t: &Tunables) -> i8 {
    if v == 0 || to <= from {
        return v;
    }
    let elapsed = to.0 - from.0;
    let mut rng = key.slot(decay_slot(target, from));
    let mut m = v.unsigned_abs().min(127);
    let mut acc: u64 = 0;
    while m > 0 {
        // -ln(u) > 0 for u in (0, 1), so every wait is at least one tick.
        let w = (-rng.next_f64_open().ln() * t.mean_step_ticks(m)).ceil();
        let w = if w.is_finite() && w < u64::MAX as f64 {
            (w as u64).max(1)
        } else {
            u64::MAX
        };
        acc = acc.saturating_add(w);
        if acc > elapsed {
            break;
        }
        m -= 1;
    }
    let m = m as i8;
    if v < 0 { -m } else { m }
}

/// The importance used for eviction: lower is evicted first.
fn rank(is_player: bool, value: i8, target: ActorId) -> (bool, u8, u64) {
    (is_player, value.unsigned_abs(), target.to_bits())
}

/// One holder's sentiment table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sentiments {
    pub(crate) holder: ActorId,
    pub(crate) anchor: Tick,
    pub(crate) cap: u16,
    /// Sorted by target handle, so lookups are binary searches and the order is deterministic.
    pub(crate) entries: Vec<SentimentEntry>,
}

impl Sentiments {
    pub fn new(holder: ActorId, cap: u16, at: Tick) -> Self {
        Self {
            holder,
            anchor: at,
            cap,
            entries: Vec::new(),
        }
    }

    pub fn anchor(&self) -> Tick {
        self.anchor
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn cap(&self) -> u16 {
        self.cap
    }

    /// The stored entries as of [`Sentiments::anchor`]; for saving and inspection.
    pub fn entries(&self) -> &[SentimentEntry] {
        &self.entries
    }

    fn key(&self, world_seed: u64) -> StreamKey {
        StreamKey::new(world_seed, SENTIMENT_SYSTEM, self.holder.to_bits())
    }

    fn find(&self, target: ActorId) -> Result<usize, usize> {
        self.entries.binary_search_by(|e| e.target().cmp(&target))
    }

    /// The feeling toward `target` at `at`, projected. Commits nothing.
    pub fn get(&self, target: ActorId, at: Tick, world_seed: u64, t: &Tunables) -> i8 {
        match self.find(target) {
            Ok(i) => project(
                self.entries[i].value(),
                self.anchor,
                at,
                self.key(world_seed),
                target,
                t,
            ),
            Err(_) => 0,
        }
    }

    /// Every entry projected to `at`, in target order, zeros left out. Commits nothing. For the
    /// bridge's snapshot (the sorcerer filter, PLAN.md §8.8).
    pub fn snapshot(&self, at: Tick, world_seed: u64, t: &Tunables) -> Vec<(ActorId, i8)> {
        let key = self.key(world_seed);
        self.entries
            .iter()
            .filter_map(|e| {
                let v = project(e.value(), self.anchor, at, key, e.target(), t);
                (v != 0).then_some((e.target(), v))
            })
            .collect()
    }

    /// The `n` strongest entries at `at` (by magnitude, then handle), projected. Commits nothing.
    pub fn strongest(
        &self,
        n: usize,
        at: Tick,
        world_seed: u64,
        t: &Tunables,
    ) -> Vec<(ActorId, i8, bool)> {
        let key = self.key(world_seed);
        let mut all: Vec<(ActorId, i8, bool)> = self
            .entries
            .iter()
            .map(|e| {
                (
                    e.target(),
                    project(e.value(), self.anchor, at, key, e.target(), t),
                    e.is_player(),
                )
            })
            .filter(|e| e.1 != 0)
            .collect();
        all.sort_by(|a, b| {
            b.1.unsigned_abs()
                .cmp(&a.1.unsigned_abs())
                .then(a.0.cmp(&b.0))
        });
        all.truncate(n);
        all
    }

    /// Decays every entry up to `at` and moves the anchor there. Called by events only.
    pub fn commit(&mut self, at: Tick, world_seed: u64, t: &Tunables) {
        if at <= self.anchor {
            return;
        }
        let key = self.key(world_seed);
        let anchor = self.anchor;
        for e in &mut self.entries {
            let v = project(e.value(), anchor, at, key, e.target(), t);
            e.set_value(v);
        }
        self.entries.retain(|e| e.value() != 0);
        self.anchor = at;
    }

    /// Adds `delta` toward `target` at `at` (an event: commits decay first), saturating at
    /// ±127. A new target enters only if the table has room or it outranks the weakest entry.
    pub fn add(
        &mut self,
        at: Tick,
        target: ActorId,
        delta: i16,
        is_player: bool,
        world_seed: u64,
        t: &Tunables,
    ) {
        self.commit(at, world_seed, t);
        match self.find(target) {
            Ok(i) => {
                let v = (i16::from(self.entries[i].value()) + delta).clamp(-127, 127) as i8;
                if v == 0 {
                    self.entries.remove(i);
                } else {
                    self.entries[i].set_value(v);
                }
            }
            Err(_) => self.insert_new(target, delta.clamp(-127, 127) as i8, is_player),
        }
    }

    /// An inherited feeling (C24): same sign keeps the stronger, opposite signs add. So a home
    /// market that already heard of the robbery its trader suffered does not count it twice.
    pub fn merge(
        &mut self,
        at: Tick,
        target: ActorId,
        value: i8,
        is_player: bool,
        world_seed: u64,
        t: &Tunables,
    ) {
        self.commit(at, world_seed, t);
        let value = value.clamp(-127, 127);
        match self.find(target) {
            Ok(i) => {
                let old = self.entries[i].value();
                let v = if (old < 0) == (value < 0) {
                    if value.unsigned_abs() > old.unsigned_abs() {
                        value
                    } else {
                        old
                    }
                } else {
                    (i16::from(old) + i16::from(value)).clamp(-127, 127) as i8
                };
                if v == 0 {
                    self.entries.remove(i);
                } else {
                    self.entries[i].set_value(v);
                }
            }
            Err(_) => self.insert_new(target, value, is_player),
        }
    }

    fn insert_new(&mut self, target: ActorId, value: i8, is_player: bool) {
        if value == 0 || self.cap == 0 {
            return;
        }
        let flags = if is_player { SENT_FLAG_PLAYER } else { 0 };
        if self.entries.len() >= usize::from(self.cap) {
            let (weakest, w_rank) = self
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| (i, rank(e.is_player(), e.value(), e.target())))
                .min_by_key(|x| x.1)
                .expect("a full table is not empty");
            // Compare without the handle tie-break: a tie keeps the resident.
            if (is_player, value.unsigned_abs()) <= (w_rank.0, w_rank.1) {
                return;
            }
            self.entries.remove(weakest);
        }
        let i = self.find(target).unwrap_err();
        self.entries
            .insert(i, SentimentEntry::new(target, value, flags));
    }

    pub fn encode(&self, w: &mut Writer) {
        w.u64(self.holder.to_bits());
        w.u64(self.anchor.0);
        w.u16(self.cap);
        w.var_u64(self.entries.len() as u64);
        for e in &self.entries {
            w.raw(&e.0);
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<SentimentEntry>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(i: u32) -> ActorId {
        Handle::from_parts(i, 1)
    }

    const SEED: u64 = 0xfeed;

    #[test]
    fn caps_evict_the_weakest_non_player_first() {
        let t = Tunables::default();
        let mut s = Sentiments::new(h(100), 3, Tick(0));
        s.add(Tick(0), h(1), -100, false, SEED, &t);
        s.add(Tick(0), h(2), 5, false, SEED, &t);
        s.add(Tick(0), h(3), 3, true, SEED, &t);
        assert_eq!(s.len(), 3);
        // Full. A new non-player entry of 10 evicts the weakest non-player one (5), not the
        // player's 3 and never the strongest (-100).
        s.add(Tick(0), h(4), 10, false, SEED, &t);
        assert_eq!(s.len(), 3);
        assert_eq!(s.get(h(2), Tick(0), SEED, &t), 0);
        assert_eq!(s.get(h(1), Tick(0), SEED, &t), -100);
        assert_eq!(s.get(h(3), Tick(0), SEED, &t), 3);
        assert_eq!(s.get(h(4), Tick(0), SEED, &t), 10);
        // A new non-player feeling weaker than every non-player one is dropped.
        s.add(Tick(0), h(5), 2, false, SEED, &t);
        assert_eq!(s.get(h(5), Tick(0), SEED, &t), 0);
        // A new player feeling outranks every non-player entry: the weakest of those goes.
        s.add(Tick(0), h(6), 1, true, SEED, &t);
        assert_eq!(s.get(h(6), Tick(0), SEED, &t), 1);
        assert_eq!(s.get(h(4), Tick(0), SEED, &t), 0);
        assert_eq!(
            s.get(h(1), Tick(0), SEED, &t),
            -100,
            "the strongest survives"
        );
        assert!(s.len() <= 3);
    }

    #[test]
    fn values_saturate_and_cancel() {
        let t = Tunables::default();
        let mut s = Sentiments::new(h(100), 8, Tick(0));
        s.add(Tick(0), h(1), -200, true, SEED, &t);
        assert_eq!(s.get(h(1), Tick(0), SEED, &t), -127);
        s.add(Tick(0), h(1), 127, true, SEED, &t);
        assert_eq!(s.get(h(1), Tick(0), SEED, &t), 0);
        assert!(s.is_empty(), "a zero feeling is not stored");
    }

    #[test]
    fn strong_feelings_decay_slower_than_weak_ones() {
        let t = Tunables::default();
        // Over many holders, after the same time a weak feeling has lost more steps than a
        // strong one. Deterministic: fixed seeds.
        let after = Tick(24_000);
        let (mut weak_lost, mut strong_lost) = (0u64, 0u64);
        for holder in 0..200 {
            let mut s = Sentiments::new(h(10_000 + holder), 4, Tick(0));
            s.add(Tick(0), h(1), 20, false, SEED, &t);
            s.add(Tick(0), h(2), 120, false, SEED, &t);
            weak_lost += u64::from(20 - s.get(h(1), after, SEED, &t).unsigned_abs());
            strong_lost += u64::from(120 - s.get(h(2), after, SEED, &t).unsigned_abs());
        }
        assert!(
            weak_lost > 4 * strong_lost,
            "weak lost {weak_lost} steps, strong {strong_lost}"
        );
        assert!(strong_lost > 0, "strong feelings still decay");
        // And everything weak is eventually forgotten.
        let mut s = Sentiments::new(h(9), 4, Tick(0));
        s.add(Tick(0), h(1), 3, false, SEED, &t);
        assert_eq!(s.get(h(1), Tick(1_000_000_000), SEED, &t), 0);
    }

    #[test]
    fn decay_is_read_order_independent() {
        let t = Tunables::default();
        let mut s = Sentiments::new(h(7), 8, Tick(0));
        s.add(Tick(0), h(1), -60, true, SEED, &t);
        s.add(Tick(0), h(2), 30, false, SEED, &t);
        let before = s.clone();
        // Reads at any times see a consistent, monotone trajectory and change nothing.
        let mut last = 60u8;
        for at in (0..200_000u64).step_by(7_919) {
            let v = s.get(h(1), Tick(at), SEED, &t).unsigned_abs();
            assert!(v <= last, "decay never strengthens a feeling");
            last = v;
        }
        assert_eq!(s, before, "reads commit nothing");
        // An event at t1 then a read at t2 is the same whether or not reads happened between.
        let mut a = before.clone();
        let mut b = before.clone();
        let _ = b.get(h(1), Tick(10_000), SEED, &t);
        let _ = b.snapshot(Tick(20_000), SEED, &t);
        a.add(Tick(30_000), h(3), 5, false, SEED, &t);
        b.add(Tick(30_000), h(3), 5, false, SEED, &t);
        assert_eq!(a, b);
        assert_eq!(
            a.get(h(1), Tick(90_000), SEED, &t),
            b.get(h(1), Tick(90_000), SEED, &t)
        );
        // A commit at t1 followed by a read at t2 is a valid trajectory: never stronger than the
        // value at t1.
        let at_t1 = before.get(h(1), Tick(30_000), SEED, &t);
        assert!(a.get(h(1), Tick(90_000), SEED, &t).unsigned_abs() <= at_t1.unsigned_abs());
    }

    #[test]
    fn merge_keeps_the_stronger_of_the_same_sign() {
        let t = Tunables::default();
        let mut s = Sentiments::new(h(1), 8, Tick(0));
        s.add(Tick(0), h(2), -40, true, SEED, &t);
        s.merge(Tick(0), h(2), -30, true, SEED, &t);
        assert_eq!(s.get(h(2), Tick(0), SEED, &t), -40);
        s.merge(Tick(0), h(2), -90, true, SEED, &t);
        assert_eq!(s.get(h(2), Tick(0), SEED, &t), -90);
        s.merge(Tick(0), h(2), 20, true, SEED, &t);
        assert_eq!(s.get(h(2), Tick(0), SEED, &t), -70);
    }
}
