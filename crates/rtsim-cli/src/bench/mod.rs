// SPDX-License-Identifier: AGPL-3.0-or-later
//! Benches. `scale` is SCALE.md §10's: growth slopes against declared contracts. `bsp` is the
//! parallel step and its determinism gate (SCALE.md §5, PLAN.md §4.7).

pub mod bsp;
pub mod o5;
pub mod paged;
pub mod scale;
pub mod world;
