// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine (INDUSTRY.md; DECISIONS.md C15 to C19), headless. Work in progress.

pub mod cluster;
pub mod cmd;
pub mod ids;
pub mod learn;
pub mod obs;
pub(crate) mod ser;
pub mod threat;
pub mod tunables;
pub mod wire;
