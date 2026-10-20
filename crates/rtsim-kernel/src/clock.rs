// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sim time. One tick is one Minecraft game tick; a day is 24,000 ticks, as in Minecraft's own
//! day cycle. Ticks are absolute from world creation, so a tick number means the same moment in
//! every region and every save.

use std::num::NonZeroU64;
use std::ops::{Add, AddAssign, Sub};

/// Ticks in one Minecraft day.
pub const TICKS_PER_DAY: u64 = 24_000;
/// Ticks in one real second at the normal tick rate.
pub const TICKS_PER_SECOND: u64 = 20;

/// An absolute sim time in ticks.
///
/// Reports store times as 48-bit ticks (SCALE.md §9); [`Tick::MAX_48`] is the last one that fits,
/// about 11.7 billion days.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

impl Tick {
    pub const ZERO: Tick = Tick(0);
    pub const MAX_48: Tick = Tick((1 << 48) - 1);

    pub const fn from_days(days: u64) -> Tick {
        Tick(days * TICKS_PER_DAY)
    }

    /// Whole days since world creation.
    pub const fn day(self) -> u64 {
        self.0 / TICKS_PER_DAY
    }

    pub const fn tick_of_day(self) -> u64 {
        self.0 % TICKS_PER_DAY
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// Ticks from `earlier` to `self`, or 0 if `earlier` is later.
    pub const fn since(self, earlier: Tick) -> u64 {
        self.0.saturating_sub(earlier.0)
    }

    /// The 48-bit form used in packed rows, if it fits.
    pub fn to_u48(self) -> Option<[u8; 6]> {
        if self > Self::MAX_48 {
            return None;
        }
        let b = self.0.to_le_bytes();
        Some([b[0], b[1], b[2], b[3], b[4], b[5]])
    }

    pub fn from_u48(b: [u8; 6]) -> Tick {
        Tick(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], 0, 0,
        ]))
    }
}

impl Add<u64> for Tick {
    type Output = Tick;
    fn add(self, dt: u64) -> Tick {
        Tick(self.0 + dt)
    }
}

impl AddAssign<u64> for Tick {
    fn add_assign(&mut self, dt: u64) {
        self.0 += dt;
    }
}

impl Sub for Tick {
    type Output = u64;
    /// Panics in debug builds if `rhs` is later; use [`Tick::since`] when that can happen.
    fn sub(self, rhs: Tick) -> u64 {
        self.0 - rhs.0
    }
}

/// Fixed slots aligned to tick 0, on which random processes commit (SCALE.md §4).
///
/// Alignment to tick 0, not to whenever a region was created or last read, is what makes a slot
/// boundary the same moment for every reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotPeriod(NonZeroU64);

impl SlotPeriod {
    pub const fn new(ticks: NonZeroU64) -> Self {
        Self(ticks)
    }

    /// Panics on zero; for constants.
    pub const fn ticks(ticks: u64) -> Self {
        match NonZeroU64::new(ticks) {
            Some(t) => Self(t),
            None => panic!("a slot period must be at least one tick"),
        }
    }

    pub const DAY: SlotPeriod = SlotPeriod::ticks(TICKS_PER_DAY);

    pub const fn ticks_per_slot(self) -> u64 {
        self.0.get()
    }

    /// The slot containing `t`.
    pub const fn slot_of(self, t: Tick) -> u64 {
        t.0 / self.0.get()
    }

    pub const fn start(self, slot: u64) -> Tick {
        Tick(slot * self.0.get())
    }

    /// The boundary at which `slot` commits: the start of the next slot.
    pub const fn end(self, slot: u64) -> Tick {
        Tick((slot + 1) * self.0.get())
    }

    /// How many slot boundaries lie in `(from, to]`: the commits a catch-up from `from` to `to`
    /// makes.
    pub const fn boundaries_between(self, from: Tick, to: Tick) -> u64 {
        if to.0 <= from.0 {
            return 0;
        }
        to.0 / self.0.get() - from.0 / self.0.get()
    }
}

/// The engine's clock: the tick the last completed step reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    now: Tick,
}

impl Clock {
    pub const fn starting_at(now: Tick) -> Self {
        Self { now }
    }

    pub const fn now(&self) -> Tick {
        self.now
    }

    /// Moves to `to`. Time never runs backwards.
    pub fn advance_to(&mut self, to: Tick) {
        assert!(
            to >= self.now,
            "the clock cannot go back from {:?} to {to:?}",
            self.now
        );
        self.now = to;
    }

    pub fn advance(&mut self, dt: u64) {
        self.now += dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_and_slots() {
        let t = Tick::from_days(3) + 5;
        assert_eq!(t.day(), 3);
        assert_eq!(t.tick_of_day(), 5);
        let p = SlotPeriod::DAY;
        assert_eq!(p.slot_of(t), 3);
        assert_eq!(p.end(3), Tick::from_days(4));
        assert_eq!(p.boundaries_between(Tick(0), Tick::from_days(2)), 2);
        assert_eq!(
            p.boundaries_between(Tick(1), Tick(Tick::from_days(2).0 - 1)),
            1
        );
        assert_eq!(p.boundaries_between(Tick(5), Tick(5)), 0);
    }

    #[test]
    fn u48_round_trip() {
        let t = Tick(0x0000_1234_5678_9abc);
        assert_eq!(Tick::from_u48(t.to_u48().unwrap()), t);
        assert!(Tick(1 << 48).to_u48().is_none());
    }
}
