// SPDX-License-Identifier: AGPL-3.0-or-later
//! mic-rtsim's ecology module (PLAN.md §1.2): hunger, prey and predators. Phase 2 is its first
//! user: dragons on Ice and Fire (PLAN.md §9; C3, C16 to C18, C24, C25; A13, A15).
//!
//! - [`species`], [`tunables`], [`calendar`], [`prey`]: data. Every number not cited as an Ice
//!   and Fire fact, a pack config value or a decision is a TUNABLE PLACEHOLDER.
//! - [`dragon`]: the dragon row (a knowledge notable) and its lair; closed-form hunger and health.
//! - [`flight`]: flights as legs in the shared leg index, at a forced altitude while unloaded.
//! - [`homes`]: a player's home from a decaying counter (P26's recommendation).
//! - [`index`]: a static spatial index of prey (villages and roads).
//! - [`infra`]: player infrastructure as prey, through the industry threat interface.
//! - [`boundary`]: the records the Ice and Fire plugin speaks (docs/boundary.md §6) and their
//!   codec.
//! - [`engine`]: [`engine::Dragons`], every dragon's life as events on one timing wheel.
//! - [`synth`]: a synthetic world for tests, the CLI and the bench. Scaffolding.
//! - [`herds`]: migrating herds of vanilla animals (PLAN.md §10.2): forage, migration, latent
//!   members with derived identity, materialization, and the prey dragons and players hunt.
//!
//! # Composition
//!
//! PLAN.md §5.4's module registry does not exist yet, so, as `rtsim-world` composes phase 1, this
//! crate composes the dragons with `rtsim-world`'s [`rtsim_world::World`] (markets, flows, the leg
//! index, knowledge) and `rtsim-industry`'s threat interface, through their public APIs. The
//! caller owns the world and passes it in; [`engine::Dragons::run_until`] interleaves the two in
//! time order.

pub mod boundary;
pub mod calendar;
pub mod dragon;
pub mod engine;
pub mod flight;
pub mod herds;
pub mod homes;
pub mod index;
pub mod infra;
mod persist;
pub mod prey;
pub mod species;
pub mod synth;
pub mod tunables;

pub use dragon::{Dragon, DragonId, Lair, LairId, LairKind};
pub use engine::{DragonStats, Dragons, NewDragon};
pub use species::{Gender, Species};
pub use tunables::{DragonTunables, PreyKind};
