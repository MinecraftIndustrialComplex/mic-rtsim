// SPDX-License-Identifier: AGPL-3.0-or-later
//! Season and climate for herds, through an abstract provider (PLAN.md §10.2: "forage fields
//! regrowing from climate").
//!
//! **The calendar** is the ecology's [`Calendar`] (P2: Serene Seasons' or mic-climate's, not
//! decided), split as Serene Seasons splits it: twelve sub-seasons a year, three to a season, early
//! spring first. Herds decide once per sub-season, and the forage field's dynamics are constant
//! within one.
//!
//! **The provider** ([`Climate`]) answers per forage cell and sub-season: how forage grows and
//! decays there, the temperature, and how hard the ground is to cross. A synthetic provider serves
//! the tests, the CLI and the bench (`herds::synth`); Serene Seasons and Deep Time come later as
//! plugins that fill the same trait. **Climate is periodic in the year**: the same cell and
//! sub-season give the same numbers every year, which is what lets forage catch up over any number
//! of years in closed form (`forage`). A provider whose years differ would re-anchor at each year.

use rtsim_kernel::clock::Tick;

use crate::calendar::Calendar;

/// Sub-seasons in a year (Serene Seasons: early, mid and late of each season).
pub const SUB_SEASONS: u32 = 12;

/// Forage dynamics in one cell for one sub-season: logistic growth toward `capacity` at
/// `growth_per_day`, and decay at `decay_per_day` (frost, drought, trampling). Units: forage units
/// (one feeds a sheep for a day) per forage cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForageRates {
    pub growth_per_day: f64,
    pub capacity: f64,
    pub decay_per_day: f64,
}

/// What the herds read about the world's climate and ground, per forage cell (frame, cell x,
/// cell z) and sub-season of the year (0 to 11). Implementations must be pure: the same
/// arguments always give the same answer.
pub trait Climate: Send + Sync {
    fn forage(&self, frame: u16, cx: i32, cz: i32, sub: u32) -> ForageRates;
    /// Mean temperature in °C.
    fn temperature(&self, frame: u16, cx: i32, cz: i32, sub: u32) -> f32;
    /// Cost of crossing the cell, relative to open grassland (1.0). Infinite or NaN: impassable
    /// (deep water, cliffs).
    fn terrain(&self, frame: u16, cx: i32, cz: i32) -> f32;
}

/// The same everywhere and always: grassland at 15 °C. For an engine built before the caller
/// gives it a provider.
#[derive(Clone, Copy, Debug, Default)]
pub struct Uniform;

impl Climate for Uniform {
    fn forage(&self, _: u16, _: i32, _: i32, _: u32) -> ForageRates {
        ForageRates {
            growth_per_day: 0.1,
            capacity: 2_000.0,
            decay_per_day: 0.0,
        }
    }
    fn temperature(&self, _: u16, _: i32, _: i32, _: u32) -> f32 {
        15.0
    }
    fn terrain(&self, _: u16, _: i32, _: i32) -> f32 {
        1.0
    }
}

/// Sub-season arithmetic over a [`Calendar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Subs {
    pub origin: i64,
    /// Ticks per sub-season.
    pub len: u64,
}

impl Subs {
    pub fn of(cal: &Calendar) -> Subs {
        Subs {
            origin: cal.origin,
            len: (cal.season_ticks / 3).max(1),
        }
    }

    /// The absolute sub-season number containing `t` (0 is early spring of year 0).
    pub fn index(&self, t: Tick) -> i64 {
        (t.0 as i64 - self.origin).div_euclid(self.len as i64)
    }

    /// The sub-season of the year (0 early spring to 11 late winter) of absolute sub-season `n`.
    pub fn in_year(n: i64) -> u32 {
        n.rem_euclid(i64::from(SUB_SEASONS)) as u32
    }

    /// The year of absolute sub-season `n`.
    pub fn year(n: i64) -> i64 {
        n.div_euclid(i64::from(SUB_SEASONS))
    }

    /// The first tick of absolute sub-season `n` (clamped at 0).
    pub fn start(&self, n: i64) -> Tick {
        Tick((self.origin + n * self.len as i64).max(0) as u64)
    }

    /// The first tick after `t` at which a sub-season begins.
    pub fn next_start(&self, t: Tick) -> Tick {
        self.start(self.index(t) + 1)
    }

    pub fn year_ticks(&self) -> u64 {
        self.len * u64::from(SUB_SEASONS)
    }

    /// Days in a year (at least 1).
    pub fn year_days(&self) -> u32 {
        (self.year_ticks() / rtsim_kernel::clock::TICKS_PER_DAY).max(1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::Season;

    #[test]
    fn sub_seasons_split_seasons_in_three() {
        let s = Subs::of(&Calendar::starting_in(Season::Summer));
        assert_eq!(s.len, 8 * 24_000);
        assert_eq!(Subs::in_year(s.index(Tick(0))), 3);
        assert_eq!(Subs::in_year(s.index(Tick::from_days(8))), 4);
        assert_eq!(Subs::in_year(s.index(Tick::from_days(72))), 0);
        assert_eq!(s.next_start(Tick(5)), Tick::from_days(8));
        assert_eq!(s.year_days(), 96);
    }
}
