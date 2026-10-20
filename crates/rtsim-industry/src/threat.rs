// SPDX-License-Identifier: AGPL-3.0-or-later
//! Threats (C16 to C18). Work in progress.

use serde::Serialize;

/// The four threat classes of INDUSTRY.md §13.2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ThreatClass {
    GroundMelee,
    GroundRanged,
    Flying,
    Siege,
}

impl ThreatClass {
    pub const COUNT: usize = 4;
    pub const ALL: [ThreatClass; 4] = [
        ThreatClass::GroundMelee,
        ThreatClass::GroundRanged,
        ThreatClass::Flying,
        ThreatClass::Siege,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}
