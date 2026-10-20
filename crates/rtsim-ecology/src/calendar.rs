// SPDX-License-Identifier: AGPL-3.0-or-later
//! The sim calendar's seasons. Which calendar the sim follows is Proposal P2 (Ben): Serene
//! Seasons' (8-day sub-seasons by default, three to a season, PLAN.md §17 P2) or mic-climate's
//! `YearClock` in Deep Time worlds. Until P2 is decided this is a plain calendar with the same
//! shape: four seasons of a fixed length from an origin tick, which the bridge can set from either
//! source (the industry engine reads the same four numbers from `IND_CONDITIONS`).

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub const ALL: [Season; 4] = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Autumn => "autumn",
            Season::Winter => "winter",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Calendar {
    /// The tick spring of year 0 begins (may be "before" tick 0: it is taken modulo the year).
    pub origin: i64,
    /// Ticks per season: 24 days, three Serene Seasons sub-seasons of 8 days (P2's default).
    pub season_ticks: u64,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            origin: 0,
            season_ticks: 24 * TICKS_PER_DAY,
        }
    }
}

impl Calendar {
    /// A calendar whose tick 0 falls at the start of `season`.
    pub fn starting_in(season: Season) -> Calendar {
        let c = Calendar::default();
        Calendar {
            origin: -(season.index() as i64) * c.season_ticks as i64,
            ..c
        }
    }

    pub fn year_ticks(&self) -> u64 {
        self.season_ticks * 4
    }

    pub fn season_at(&self, t: Tick) -> Season {
        let since = (t.0 as i64 - self.origin).rem_euclid(self.year_ticks() as i64) as u64;
        Season::ALL[(since / self.season_ticks.max(1)) as usize % 4]
    }

    /// The first tick after `t` at which the season changes.
    pub fn next_change(&self, t: Tick) -> Tick {
        let since = (t.0 as i64 - self.origin).rem_euclid(self.season_ticks as i64) as u64;
        Tick(t.0 + (self.season_ticks - since))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_cycle() {
        let c = Calendar::starting_in(Season::Autumn);
        assert_eq!(c.season_at(Tick(0)), Season::Autumn);
        assert_eq!(c.season_at(Tick::from_days(23)), Season::Autumn);
        assert_eq!(c.season_at(Tick::from_days(24)), Season::Winter);
        assert_eq!(c.season_at(Tick::from_days(48)), Season::Spring);
        assert_eq!(c.next_change(Tick(5)), Tick::from_days(24));
        assert_eq!(c.next_change(Tick::from_days(24)), Tick::from_days(48));
    }
}
