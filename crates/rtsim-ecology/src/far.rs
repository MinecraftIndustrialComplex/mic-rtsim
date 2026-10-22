// SPDX-License-Identifier: AGPL-3.0-or-later
//! The far view's dragon legs (research/distant-horizons-2026-10-21.md §4.2, §6.3; docs/boundary.md
//! §12; choices DHT25 and up in docs/choices-dh-feed.md): what each dragon is doing, as one
//! FAR_LEG command to Java whenever that changes, so the core's far feed can draw dragons out to a
//! Distant Horizons client's view without asking the engine anything per tick.
//!
//! - **Event driven.** Dragon rows page (A16), so nothing here passes over every dragon on a step:
//!   [`Dragons::track`], `book_leg`, `cancel_leg`, a new dragon and a removed one mark the dragon in
//!   a set, and [`Dragons::take_far`] turns the marked ones into records. A changed dragon's page is
//!   pinned until the next save, so reading it back costs no load.
//! - **Per session.** A session start resends every dragon (one bounded pass over the rows, the
//!   only whole-table pass): Java's far table is per session, as HABITAT_KNOWN's resend is.
//! - **Off until asked.** An engine nobody drains (the synthetic world, the headless host's tools)
//!   marks nothing: [`Dragons::set_far_feed`] turns it on, and rtsim-jni does so at every session
//!   start.
//!
//! A record carries the leg as the engine booked it, in chart coordinates (TP7: an end may lie
//! past a seam), and its times in engine ticks; the link converts them to game time when it writes
//! the record.

use std::collections::BTreeSet;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::rows::PagedArena;
use rtsim_space::topology::Topologies;

use crate::dragon::{Dragon, DragonId, Lairs, State, agent_of};
use crate::engine::Dragons;
use crate::species::Species;
use crate::tunables::DragonTunables;

/// The far view's command range, 0x8C00 to 0x8CFF (docs/boundary.md §12.1), with 0x00D0 to 0x00DF
/// latest-wins and 0x0C00 to 0x0CFF journaled kept for it. 0x8A00 to 0x8BFF stay free for the
/// next simulation families (DHT26).
pub const FAR_LEG: u16 = 0x8C00;
/// FAR_LEG's size in bytes.
pub const FAR_LEG_SIZE: usize = 80;

/// What a far object is (FAR_LEG's byte at 32): the core's `FarKind` reads the same numbers.
pub const FAR_KIND_DRAGON: u8 = 1;

/// What a dragon is doing, as FAR_LEG's `aux`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FarState {
    /// It no longer exists (died, or its row was removed): forget it.
    Gone = 0,
    /// Flying a straight leg from (x0, z0) at t0 to (x1, z1) at t1, at height y.
    Fly = 1,
    /// Hunting around (x0, z0) at height y until t1: drawn circling at `radius`.
    Hunt = 2,
    /// At rest at (x0, y, z0) (its lair, or where it settled): drawn with its wings folded.
    Roost = 3,
    /// A body in the game (embodied or leaving): Java's own entity is the truth; (x0, y, z0) is
    /// where the engine last knew it.
    Body = 4,
}

impl FarState {
    pub fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::Gone,
            1 => Self::Fly,
            2 => Self::Hunt,
            3 => Self::Roost,
            4 => Self::Body,
            _ => return None,
        })
    }
}

/// One FAR_LEG, before the link names its species and dimension and moves its times to game
/// time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FarLeg {
    pub agent: u64,
    pub state: FarState,
    pub species: Species,
    /// Colour index 0 to 3 within the species.
    pub variant: u8,
    /// Ice and Fire's stage, 1 to 5, at the record's time.
    pub stage: u8,
    /// The engine's frame (Java's dimension id).
    pub frame: u16,
    pub y: i32,
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    /// Engine ticks.
    pub t0: Tick,
    pub t1: Tick,
    /// A hunt's circle, blocks; 0 otherwise.
    pub radius: u32,
}

/// The dragons' far feed state (not saved: per session).
#[derive(Clone, Debug, Default)]
pub(crate) struct FarFeed {
    pub(crate) on: bool,
    /// Every dragon is due (a session start).
    pub(crate) all: bool,
    pub(crate) dirty: BTreeSet<DragonId>,
    /// Session-start passes a page that would not load cut short.
    pub(crate) pass_failed: u64,
}

impl FarFeed {
    pub(crate) fn mark(&mut self, d: DragonId) {
        if self.on && !self.all {
            self.dirty.insert(d);
        }
    }
}

/// A hunt is drawn circling at least this far out, blocks: the booked stay radius is the
/// dragon's body (16), too tight to read as a circle from a kilometre. PLACEHOLDER 24.
pub const HUNT_CIRCLE_MIN: u32 = 24;

/// The record for dragon `d`'s row `x` at `at`.
pub(crate) fn leg_of(
    d: DragonId,
    x: &Dragon,
    tun: &DragonTunables,
    topo: &Topologies,
    lairs: &Lairs,
    at: Tick,
) -> FarLeg {
    let stage = x.stage(tun.species(x.species), at).0;
    let mut r = FarLeg {
        agent: agent_of(d),
        state: FarState::Roost,
        species: x.species,
        variant: x.variant,
        stage,
        frame: 0,
        y: 0,
        x0: 0,
        z0: 0,
        x1: 0,
        z1: 0,
        t0: at,
        t1: at,
        radius: 0,
    };
    match (x.state, x.leg) {
        (State::Flying(_), Some(l)) => {
            r.state = FarState::Fly;
            r.frame = l.path.frame;
            r.y = l.y;
            (r.x0, r.z0, r.x1, r.z1) = (l.path.x0, l.path.z0, l.path.x1, l.path.z1);
            (r.t0, r.t1) = (l.path.t0, l.path.t1);
        }
        (State::Hunting { until, .. }, Some(l)) => {
            r.state = FarState::Hunt;
            r.frame = l.path.frame;
            r.y = l.y;
            (r.x0, r.z0, r.x1, r.z1) = (l.path.x0, l.path.z0, l.path.x1, l.path.z1);
            (r.t0, r.t1) = (l.path.t0, until.max(l.path.t1));
            r.radius = l.path.radius.max(HUNT_CIRCLE_MIN);
        }
        _ => {
            let (p, y) = x.position(topo, lairs, at);
            r.state = if x.is_embodied() {
                FarState::Body
            } else {
                FarState::Roost
            };
            r.frame = p.frame;
            r.y = y;
            (r.x0, r.z0, r.x1, r.z1) = (p.x, p.z, p.x, p.z);
        }
    }
    r
}

fn gone(d: DragonId, at: Tick) -> FarLeg {
    FarLeg {
        agent: agent_of(d),
        state: FarState::Gone,
        species: Species::Fire,
        variant: 0,
        stage: 0,
        frame: 0,
        y: 0,
        x0: 0,
        z0: 0,
        x1: 0,
        z1: 0,
        t0: at,
        t1: at,
        radius: 0,
    }
}

impl Dragons {
    /// Turns the far feed on or off. On, a session's first [`Dragons::take_far`] returns every
    /// dragon; off, nothing is marked and what was marked is dropped.
    pub fn set_far_feed(&mut self, on: bool) {
        self.far.on = on;
        self.far.all = on;
        self.far.dirty.clear();
    }

    pub fn far_feed(&self) -> bool {
        self.far.on
    }

    /// Session-start passes a page that would not load cut short.
    pub fn far_pass_failed(&self) -> u64 {
        self.far.pass_failed
    }

    /// Dragons marked since the last [`Dragons::take_far`].
    pub fn far_pending(&self) -> usize {
        self.far.dirty.len()
    }

    /// The far records due at `at`: every dragon after a session start (one bounded pass over
    /// the rows), else the dragons whose state, leg or existence changed since the last call.
    pub fn take_far(&mut self, at: Tick) -> Vec<FarLeg> {
        if !self.far.on {
            return Vec::new();
        }
        let mut out = Vec::new();
        if std::mem::take(&mut self.far.all) {
            self.far.dirty.clear();
            let (tun, topo, lairs) = (&self.tun, &*self.topo, &self.lairs);
            let pass = self.dragons.rows_mut().for_each_bounded(|k, x| {
                let d = PagedArena::<Dragon>::handle(k as u32);
                out.push(leg_of(d, x, tun, topo, lairs, at));
            });
            if let Err(e) = pass {
                // A page that will not load holds dragons Java then draws only once they next
                // change; the far view is a picture, so it carries on.
                self.far.pass_failed += 1;
                let _ = e;
            }
            return out;
        }
        for d in std::mem::take(&mut self.far.dirty) {
            match self.dragons.get(d) {
                Some(x) => out.push(leg_of(d, x, &self.tun, &self.topo, &self.lairs, at)),
                None => out.push(gone(d, at)),
            }
        }
        out
    }
}

/// Appends one FAR_LEG with sequence `seq`. `body` and `dimension` are the session's dictionary
/// ids for the species' entity type and the frame's dimension; `offset` is the engine tick at game
/// time 0 (the link's), so a time goes out as `tick - offset` (game time).
pub fn encode(seq: u64, r: &FarLeg, body: u32, dimension: u32, offset: u64, buf: &mut Vec<u8>) {
    let start = buf.len();
    buf.resize(start + FAR_LEG_SIZE, 0);
    let b = &mut buf[start..];
    let game = |t: Tick| t.0 as i64 - offset as i64;
    b[0..2].copy_from_slice(&FAR_LEG.to_le_bytes());
    b[2..4].copy_from_slice(&(FAR_LEG_SIZE as u16).to_le_bytes());
    b[4..8].copy_from_slice(&(r.state as u32).to_le_bytes());
    b[8..16].copy_from_slice(&seq.to_le_bytes());
    b[16..24].copy_from_slice(&r.agent.to_le_bytes());
    b[24..28].copy_from_slice(&body.to_le_bytes());
    b[28..32].copy_from_slice(&dimension.to_le_bytes());
    b[32] = FAR_KIND_DRAGON;
    b[33] = r.variant;
    b[34] = r.stage;
    b[35] = 0;
    b[36..40].copy_from_slice(&r.y.to_le_bytes());
    b[40..44].copy_from_slice(&r.x0.to_le_bytes());
    b[44..48].copy_from_slice(&r.z0.to_le_bytes());
    b[48..52].copy_from_slice(&r.x1.to_le_bytes());
    b[52..56].copy_from_slice(&r.z1.to_le_bytes());
    b[56..64].copy_from_slice(&game(r.t0).to_le_bytes());
    b[64..72].copy_from_slice(&game(r.t1).to_le_bytes());
    b[72..76].copy_from_slice(&r.radius.to_le_bytes());
}

/// One FAR_LEG as Java reads it (tests and tools): every field raw, times in game time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub seq: u64,
    pub state: FarState,
    pub agent: u64,
    pub body: u32,
    pub dimension: u32,
    pub kind: u8,
    pub variant: u8,
    pub stage: u8,
    pub y: i32,
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub t0: i64,
    pub t1: i64,
    pub radius: u32,
}

/// Reads one FAR_LEG at the start of `b`; `None` for another kind, a short buffer or a state
/// this build does not know.
pub fn decode(b: &[u8]) -> Option<Decoded> {
    if b.len() < FAR_LEG_SIZE {
        return None;
    }
    let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"));
    let i32_at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().expect("4 bytes"));
    let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8 bytes"));
    if u16_at(0) != FAR_LEG || usize::from(u16_at(2)) != FAR_LEG_SIZE {
        return None;
    }
    Some(Decoded {
        seq: u64_at(8),
        state: FarState::from_u32(u32_at(4))?,
        agent: u64_at(16),
        body: u32_at(24),
        dimension: u32_at(28),
        kind: b[32],
        variant: b[33],
        stage: b[34],
        y: i32_at(36),
        x0: i32_at(40),
        z0: i32_at(44),
        x1: i32_at(48),
        z1: i32_at(52),
        t0: u64_at(56) as i64,
        t1: u64_at(64) as i64,
        radius: u32_at(72),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FarLeg {
        FarLeg {
            agent: 0x44 << 56 | 7,
            state: FarState::Fly,
            species: Species::Ice,
            variant: 2,
            stage: 4,
            frame: 0,
            y: 180,
            x0: -1200,
            z0: 300,
            x1: 4096,
            z1: -77,
            t0: Tick(10_000),
            t1: Tick(12_500),
            radius: 0,
        }
    }

    #[test]
    fn round_trips_with_game_time() {
        let mut buf = Vec::new();
        encode(42, &sample(), 9, 3, 4_000, &mut buf);
        assert_eq!(buf.len(), FAR_LEG_SIZE);
        assert_eq!(buf.len() % 8, 0, "records are 8-aligned");
        let d = decode(&buf).expect("decodes");
        assert_eq!(d.seq, 42);
        assert_eq!(d.state, FarState::Fly);
        assert_eq!(d.agent, 0x44 << 56 | 7);
        assert_eq!((d.body, d.dimension, d.kind), (9, 3, FAR_KIND_DRAGON));
        assert_eq!((d.variant, d.stage, d.y), (2, 4, 180));
        assert_eq!((d.x0, d.z0, d.x1, d.z1), (-1200, 300, 4096, -77));
        assert_eq!((d.t0, d.t1), (6_000, 8_500), "engine ticks less the offset");
        assert_eq!(&buf[76..80], &[0; 4], "reserved bytes stay zero");
    }

    #[test]
    fn a_time_before_game_time_zero_stays_signed() {
        let mut buf = Vec::new();
        let r = FarLeg {
            t0: Tick(10),
            t1: Tick(20),
            ..sample()
        };
        encode(1, &r, 1, 1, 100, &mut buf);
        let d = decode(&buf).expect("decodes");
        assert_eq!((d.t0, d.t1), (-90, -80));
    }

    #[test]
    fn other_kinds_and_states_are_not_read() {
        let mut buf = Vec::new();
        encode(1, &sample(), 1, 1, 0, &mut buf);
        let mut other = buf.clone();
        other[0] = 0x01;
        assert!(decode(&other).is_none());
        let mut bad_state = buf.clone();
        bad_state[4] = 9;
        assert!(decode(&bad_state).is_none());
        assert!(decode(&buf[..40]).is_none());
    }
}
