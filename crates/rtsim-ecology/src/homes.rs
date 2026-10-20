// SPDX-License-Identifier: AGPL-3.0-or-later
//! A player's home, for a grudge to find (Proposal P26, its recommendation): the region where the
//! player spent the most time recently, from a decaying counter fed by position observations.
//!
//! Each (player, region) entry holds time spent and the time-weighted sum of positions, all
//! decaying with one half-life. Decay is lazy: an entry stores its values at its own anchor and
//! is decayed when read, so nothing is stepped. A player keeps a few regions; the weakest goes
//! when a new one needs room.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_travel::legs::REGION_BLOCKS;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct Entry {
    frame: u16,
    rx: i64,
    rz: i64,
    /// Ticks spent, decayed to `at`.
    v: f64,
    sx: f64,
    sz: f64,
    #[serde(skip)]
    at: Tick,
}

impl Entry {
    fn decayed(&self, at: Tick, half_life: u64) -> (f64, f64, f64) {
        let dt = at.0.saturating_sub(self.at.0) as f64;
        let f = (-dt / half_life.max(1) as f64 * std::f64::consts::LN_2).exp();
        (self.v * f, self.sx * f, self.sz * f)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PlayerHome {
    last: Option<(Pos, Tick)>,
    regions: Vec<Entry>,
}

/// A home: where, and how many (decayed) ticks the player spent in its region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Home {
    pub pos: Pos,
    pub weight_ticks: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Homes {
    players: BTreeMap<ActorId, PlayerHome>,
    half_life: u64,
    cap: usize,
    /// The most a single gap between two observations counts (a player who logged off is not at
    /// home). PLACEHOLDER: 1,200 ticks, a minute.
    pub max_gap: u64,
}

impl Homes {
    pub fn new(half_life: u64, cap: usize) -> Self {
        Homes {
            players: BTreeMap::new(),
            half_life,
            cap: cap.max(1),
            max_gap: 1_200,
        }
    }

    /// The bridge saw `player` at `pos` at `at` (a PLAYER record).
    pub fn observe(&mut self, player: ActorId, pos: Pos, at: Tick) {
        let (half_life, cap, max_gap) = (self.half_life, self.cap, self.max_gap);
        let p = self.players.entry(player).or_default();
        let dt = match p.last {
            Some((_, t)) if at.0 > t.0 => (at.0 - t.0).min(max_gap),
            _ => 0,
        };
        p.last = Some((pos, at));
        if dt == 0 {
            return;
        }
        let (rx, rz) = (
            i64::from(pos.x).div_euclid(REGION_BLOCKS),
            i64::from(pos.z).div_euclid(REGION_BLOCKS),
        );
        let w = dt as f64;
        if let Some(e) = p
            .regions
            .iter_mut()
            .find(|e| e.frame == pos.frame && e.rx == rx && e.rz == rz)
        {
            let (v, sx, sz) = e.decayed(at, half_life);
            *e = Entry {
                v: v + w,
                sx: sx + w * f64::from(pos.x),
                sz: sz + w * f64::from(pos.z),
                at,
                ..*e
            };
            return;
        }
        if p.regions.len() >= cap {
            let weakest = p
                .regions
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1.decayed(at, half_life)
                        .0
                        .total_cmp(&b.1.decayed(at, half_life).0)
                })
                .map(|(i, _)| i)
                .expect("full means non-empty");
            p.regions.remove(weakest);
        }
        p.regions.push(Entry {
            frame: pos.frame,
            rx,
            rz,
            v: w,
            sx: w * f64::from(pos.x),
            sz: w * f64::from(pos.z),
            at,
        });
    }

    /// A stay the caller knows in one piece (a scripted player, a replayed journal): `until -
    /// from` ticks at `pos`, as if observed throughout.
    pub fn add_stay(&mut self, player: ActorId, pos: Pos, from: Tick, until: Tick) {
        let gap = self.max_gap;
        self.max_gap = u64::MAX;
        let p = self.players.entry(player).or_default();
        p.last = Some((pos, from));
        self.observe(player, pos, until);
        self.max_gap = gap;
    }

    /// The player's home at `at`: the region with the most decayed time, at the time-weighted
    /// mean position spent there. `None` for a player never seen staying.
    pub fn home(&self, player: ActorId, at: Tick) -> Option<Home> {
        let p = self.players.get(&player)?;
        let best = p.regions.iter().max_by(|a, b| {
            a.decayed(at, self.half_life)
                .0
                .total_cmp(&b.decayed(at, self.half_life).0)
        })?;
        let (v, sx, sz) = best.decayed(at, self.half_life);
        if v <= 0.0 {
            return None;
        }
        Some(Home {
            pos: Pos::new(best.frame, (sx / v).round() as i32, (sz / v).round() as i32),
            weight_ticks: v,
        })
    }

    /// Where the player was last seen, and when.
    pub fn last_seen(&self, player: ActorId) -> Option<(Pos, Tick)> {
        self.players.get(&player).and_then(|p| p.last)
    }

    pub fn players(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.players.keys().copied()
    }

    pub fn heap_bytes(&self) -> usize {
        self.players.len() * 96
            + self
                .players
                .values()
                .map(|p| p.regions.capacity() * std::mem::size_of::<Entry>())
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::handle::Handle;

    #[test]
    fn home_is_where_the_time_goes_and_it_moves() {
        let mut h = Homes::new(72_000, 4);
        let p: ActorId = Handle::from_parts(1, 1);
        // Two days at the farm, sampled every minute.
        let mut t = 0;
        while t < 48_000 {
            h.observe(p, Pos::new(0, 300, 300), Tick(t));
            t += 1_200;
        }
        // An hour on the road far away.
        let end = t + 1_000;
        while t < end {
            h.observe(p, Pos::new(0, 5_000, 0), Tick(t));
            t += 100;
        }
        let home = h.home(p, Tick(t)).unwrap();
        assert_eq!((home.pos.x, home.pos.z), (300, 300));
        // A long stay elsewhere moves it.
        let end = t + 400_000;
        while t < end {
            h.observe(p, Pos::new(0, 5_000, 0), Tick(t));
            t += 1_200;
        }
        assert_eq!(h.home(p, Tick(t)).unwrap().pos.x, 5_000);
    }
}
