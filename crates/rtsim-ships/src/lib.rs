// SPDX-License-Identifier: AGPL-3.0-or-later
//! mic-rtsim's ships (PLAN.md §10.1; C5, C24, C25; A13, A15; M2's recommendation): sky ships
//! between villages, sky pirates, sea ships between villages with ports, navies and patrols, and
//! predator havens for stolen cargo. Headless: the body mods are open (P8), so the engine works on
//! abstract body kinds and speaks [`boundary`] records to a future plugin.
//!
//! - [`tunables`]: every number, each a TUNABLE PLACEHOLDER unless it cites a fact.
//! - [`geom`], [`water`]: Delaunay triangulation, polylines, the abstract water mask and paths over
//!   it (with a synthetic coast for tests and the CLI).
//! - [`network`]: docks, lanes and closed loops (Veloren's Eulerian loops, credited there), and
//!   patrol walks.
//! - [`timetable`]: services whose ships' positions are a closed form of the clock, with saved
//!   clock offsets, forced cruise altitude and platform assignment.
//! - [`fight`]: Lanchester's square law floored by observed fights, and lane hazards.
//! - [`engine`]: [`engine::Ships`], every ship, pirate and navy as events on one timing wheel,
//!   composed with `rtsim-world`'s [`rtsim_world::World`] (markets, lanes as flow edges, the leg
//!   index, knowledge) and the industry threat interface (`rtsim_ecology::infra`).
//! - [`boundary`]: the records and their reference codec (docs/boundary.md §7).
//! - [`synth`]: a coastal world and a fake plugin for tests, the CLI and the bench. Scaffolding.
//!
//! # Composition
//!
//! As the dragons do (`rtsim-ecology`), this crate composes with the phase 1 world through its
//! public API: lanes are world edges added by [`rtsim_world::World::add_lane`], so merchant ships
//! carry the same flows caravans do (A13), and pirates' raids are the world's predation pipelines
//! to a haven's market (C24). The caller owns the world and passes it in.

pub mod boundary;
pub mod engine;
pub mod fight;
pub mod geom;
pub mod network;
mod persist;
pub mod synth;
pub mod timetable;
pub mod tunables;
pub mod water;

pub use engine::{Role, Ship, ShipId, ShipStats, Ships, Status};
pub use tunables::{MediumTunables, ShipTunables};
