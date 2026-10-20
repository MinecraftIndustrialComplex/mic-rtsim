// SPDX-License-Identifier: AGPL-3.0-or-later
//! Migrating herds (PLAN.md §10.2; SCALE.md §2 to §4; A10, A13, A15, C24, C25, D3, D4; O5).
//!
//! Herds of tens of thousands of vanilla animals (sheep, cows, pigs, horses, goats: D4, bodies
//! from vanilla until a plugin brings Naturalist's, P8) that graze a forage field, migrate with
//! the seasons within their comfort band, breed and die without being stepped, come to life as a
//! few puppets near a player with the same identities every time, and are the prey of dragons and
//! players.
//!
//! - [`species`]: data per species (vanilla facts cited, every other number TUNABLE) and the
//!   vanilla look rolls (a sheep's colour, a horse's coat).
//! - [`climate`]: sub-seasons over the ecology's calendar and the abstract climate provider.
//! - [`forage`]: the forage field on a coarse grid, regrowing in closed form (affine maps in
//!   1/F), lazy per cell.
//! - [`cohort`]: year-classes whose deaths are dyadic counts (O5's recommendation).
//! - [`herd`]: the group row, its route (a closed form of time) and its ledger.
//! - [`members`]: derived identity, sampling and the dead-index set (PLAN.md §4.4).
//! - [`route`]: the forage flow field, a decision's target, and obstacle costs.
//! - [`engine`]: [`Herds`], every herd's life as events on one timing wheel; the players'
//!   interest, materialization, records, and the prey interface dragons use.
//! - [`boundary`]: the records a herd plugin speaks (docs/boundary.md §8) and their codec.
//! - [`persist`]: save and restore.
//! - [`synth`]: a synthetic climate and world, and a fake plugin. Scaffolding.
//!
//! **Composition.** The herds share the phase 1 world's leg index (the caller passes it); the
//! dragons reach them through [`crate::engine::Dragons::run_until_with_herds`], which interleaves
//! the two engines in time order.

pub mod boundary;
pub mod climate;
pub mod cohort;
pub mod engine;
pub mod forage;
pub mod habitat;
pub mod herd;
pub mod members;
mod persist;
pub mod route;
pub mod species;
pub mod synth;
pub mod tunables;

pub use engine::{HerdOffer, HerdStats, Herds, NewHerd, Predation};
pub use herd::{Herd, HerdId, HerdLedger};
pub use species::{Cohort, HerdSpecies, HerdSpeciesData};
pub use tunables::HerdTunables;
