// SPDX-License-Identifier: AGPL-3.0-or-later
//! Scenario runners: headless worlds that show what a phase does.

pub mod archive;
pub mod census;
pub mod conflict;
pub mod dragons;
// The kill-all table (`sim killall`, slice S11) runs its harnesses; most of the module is its tests'.
#[cfg_attr(not(test), allow(dead_code))]
pub mod equilibrium;
pub mod herds;
pub mod industry;
pub mod killall;
pub mod memory;
pub mod mowzie;
pub mod phase1;
pub mod quiet;
pub mod ships;
pub mod trace;
