// SPDX-License-Identifier: AGPL-3.0-or-later
//! Benches. `scale` is SCALE.md §10's: growth slopes against declared contracts. `industry` is the
//! industry engine's (INDUSTRY.md §15). `bsp` is the parallel step and its determinism gate
//! (SCALE.md §5, PLAN.md §4.7); `o5` compares slot commits with dyadic counts.

pub mod bsp;
pub mod conflict;
pub mod dragons;
pub mod glass;
pub mod glass_replay;
pub mod herds;
pub mod industry;
pub mod market;
pub mod o5;
pub mod paged;
pub mod restore;
pub mod scale;
pub mod ships;
pub mod world;
