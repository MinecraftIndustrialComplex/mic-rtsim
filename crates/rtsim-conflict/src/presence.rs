// SPDX-License-Identifier: AGPL-3.0-or-later
//! Who spends time where, chunk by chunk (P14 as Ben changed it on 2026-10-20: "a signal with no
//! direct actor, such as pollution, goes to the player who has spent the most time in that
//! chunk").
//!
//! Each chunk keeps the few players who spent the most (decaying) time in it. A report of a
//! player's position credits the time since that player's last report to the chunk it was in
//! then, so a player standing in a refinery for an hour has an hour there. Decay is lazy, as P26's
//! home counter does it (`rtsim_ecology::homes`): an entry holds its value at its own anchor and
//! is decayed when read, so nothing is stepped.
//!
//! **Bounded, no scans.** A chunk keeps at most `top` players (the weakest goes when a new one
//! needs room), and the counter keeps at most `cap` chunks: the chunk touched longest ago goes,
//! found through an index by last touch. An observation costs O(log chunks); a query costs the
//! chunks asked about times `top`. No tick-driven work at all.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;

/// A chunk: (frame, chunk x, chunk z).
pub type ChunkKey = (u16, i32, i32);

/// The chunk a position is in.
pub fn chunk_of(p: Pos) -> ChunkKey {
    (p.frame, p.x.div_euclid(16), p.z.div_euclid(16))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stay {
    pub(crate) player: ActorId,
    /// Ticks spent, decayed to `at`.
    pub(crate) v: f64,
    pub(crate) at: Tick,
}

impl Stay {
    fn decayed(&self, at: Tick, half_life: u64) -> f64 {
        let dt = at.0.saturating_sub(self.at.0) as f64;
        self.v * (-dt / half_life.max(1) as f64 * std::f64::consts::LN_2).exp()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ChunkTime {
    pub(crate) stays: Vec<Stay>,
    pub(crate) touched: Tick,
}

/// The per-chunk time-spent counter (P14).
#[derive(Clone, Debug, PartialEq)]
pub struct Presence {
    pub(crate) chunks: BTreeMap<ChunkKey, ChunkTime>,
    /// (last touch, chunk), for evicting the stalest chunk at the cap.
    pub(crate) by_touch: BTreeSet<(Tick, ChunkKey)>,
    /// Each player's last report: where and when.
    pub(crate) last: BTreeMap<ActorId, (ChunkKey, Tick)>,
    pub half_life: u64,
    pub top: usize,
    pub cap: usize,
    /// The most one gap between two reports counts (a player who logged off is not there).
    pub max_gap: u64,
}

impl Presence {
    pub fn new(half_life: u64, top: usize, cap: usize, max_gap: u64) -> Self {
        Presence {
            chunks: BTreeMap::new(),
            by_touch: BTreeSet::new(),
            last: BTreeMap::new(),
            half_life: half_life.max(1),
            top: top.max(1),
            cap: cap.max(1),
            max_gap,
        }
    }

    /// `player` was seen at `pos` at `at`. Returns the chunk credited if its leading player changed
    /// (the sources there may need a new owner).
    pub fn observe(&mut self, player: ActorId, pos: Pos, at: Tick) -> Option<ChunkKey> {
        let here = chunk_of(pos);
        let prev = self.last.insert(player, (here, at));
        let (chunk, since) = prev?;
        let dt = at.0.saturating_sub(since.0).min(self.max_gap);
        if dt == 0 {
            return None;
        }
        self.credit(chunk, player, dt as f64, at)
    }

    /// The player left (logged off): its next report starts afresh.
    pub fn left(&mut self, player: ActorId) {
        self.last.remove(&player);
    }

    /// A stay the caller knows in one piece (a scripted player): `ticks` in `chunk` up to `at`.
    pub fn credit(
        &mut self,
        chunk: ChunkKey,
        player: ActorId,
        ticks: f64,
        at: Tick,
    ) -> Option<ChunkKey> {
        let (half_life, top) = (self.half_life, self.top);
        let before = self.leader(chunk, at);
        let entry = self.chunks.entry(chunk).or_default();
        self.by_touch.remove(&(entry.touched, chunk));
        entry.touched = at;
        self.by_touch.insert((at, chunk));
        let known = entry.stays.iter().position(|s| s.player == player);
        match known {
            Some(i) => {
                let s = &mut entry.stays[i];
                s.v = s.decayed(at, half_life) + ticks;
                s.at = at;
            }
            None if entry.stays.len() < top => entry.stays.push(Stay {
                player,
                v: ticks,
                at,
            }),
            None => {
                // The weakest makes room, if the newcomer outweighs it.
                let (i, weakest) = entry
                    .stays
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i, s.decayed(at, half_life)))
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .expect("full means non-empty");
                if ticks > weakest {
                    entry.stays[i] = Stay {
                        player,
                        v: ticks,
                        at,
                    };
                }
            }
        }
        while self.chunks.len() > self.cap {
            let Some((t, oldest)) = self.by_touch.pop_first() else {
                break;
            };
            debug_assert!(t <= at);
            self.chunks.remove(&oldest);
        }
        let after = self.leader(chunk, at);
        (after != before).then_some(chunk)
    }

    fn leader(&self, chunk: ChunkKey, at: Tick) -> Option<ActorId> {
        self.top_of(std::slice::from_ref(&chunk), at)
    }

    /// The player who spent the most (decayed) time in these chunks together at `at`, if anyone
    /// did. Ties go to the lower actor id, so the answer is deterministic.
    pub fn top_of(&self, chunks: &[ChunkKey], at: Tick) -> Option<ActorId> {
        let mut sums: Vec<(ActorId, f64)> = Vec::new();
        for c in chunks {
            for s in self
                .chunks
                .get(c)
                .map(|x| x.stays.as_slice())
                .unwrap_or(&[])
            {
                let v = s.decayed(at, self.half_life);
                match sums.iter_mut().find(|e| e.0 == s.player) {
                    Some(e) => e.1 += v,
                    None => sums.push((s.player, v)),
                }
            }
        }
        sums.into_iter()
            .filter(|e| e.1 > 0.0)
            .max_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then_with(|| b.0.to_bits().cmp(&a.0.to_bits()))
            })
            .map(|e| e.0)
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn heap_bytes(&self) -> usize {
        self.chunks.len() * (48 + self.top * std::mem::size_of::<Stay>()) + self.last.len() * 48
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::handle::Handle;

    fn p(n: u32) -> ActorId {
        Handle::from_parts(n, 1)
    }

    #[test]
    fn the_player_who_stays_owns_the_chunk() {
        let mut c = Presence::new(72_000, 3, 100, 1_200);
        let works = Pos::new(0, 40, 40);
        // Player 1 works there for an hour, reported every minute.
        let mut t = 0;
        while t <= 72_000 {
            c.observe(p(1), works, Tick(t));
            t += 1_200;
        }
        // Player 2 walks through for two minutes.
        c.observe(p(2), works, Tick(t));
        c.observe(p(2), works, Tick(t + 1_200));
        c.observe(p(2), Pos::new(0, 4_000, 0), Tick(t + 2_400));
        assert_eq!(c.top_of(&[chunk_of(works)], Tick(t + 2_400)), Some(p(1)));
        assert_eq!(c.top_of(&[(0, 900, 900)], Tick(t)), None, "nobody there");
    }

    #[test]
    fn a_gap_counts_at_most_max_gap_and_a_player_who_left_starts_afresh() {
        let mut c = Presence::new(1_000_000, 3, 100, 1_200);
        let here = Pos::new(0, 0, 0);
        c.observe(p(1), here, Tick(0));
        c.observe(p(1), here, Tick(100_000));
        let v = c.chunks[&chunk_of(here)].stays[0].v;
        assert_eq!(v, 1_200.0, "a day offline is not a day there");
        c.left(p(1));
        assert_eq!(c.observe(p(1), here, Tick(200_000)), None);
    }

    #[test]
    fn the_counter_is_bounded() {
        let mut c = Presence::new(72_000, 2, 10, 1_200);
        for i in 0..50 {
            c.credit((0, i, 0), p(1), 100.0, Tick(i as u64));
            c.credit((0, 0, 0), p(10 + i as u32), 50.0 + i as f64, Tick(i as u64));
        }
        assert!(c.len() <= 10, "at most the cap: {}", c.len());
        assert!(c.chunks.values().all(|x| x.stays.len() <= 2));
        assert!(c.chunks.contains_key(&(0, 49, 0)), "the latest stays");
        assert!(!c.chunks.contains_key(&(0, 1, 0)), "the stalest went");
    }

    #[test]
    fn the_leader_changing_is_reported() {
        let mut c = Presence::new(72_000, 3, 100, 10_000);
        let k = (0, 0, 0);
        assert_eq!(c.credit(k, p(1), 100.0, Tick(0)), Some(k));
        assert_eq!(c.credit(k, p(2), 50.0, Tick(0)), None);
        assert_eq!(
            c.credit(k, p(2), 100.0, Tick(0)),
            Some(k),
            "player 2 leads now"
        );
    }
}
