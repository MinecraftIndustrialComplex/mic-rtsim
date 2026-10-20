// SPDX-License-Identifier: AGPL-3.0-or-later
//! mic-rtsim's world model (PLAN.md §5.1: "world-model provider trait (sampled vanilla, Deep Time
//! reader)"; SCALE.md §2, §4, §12).
//!
//! Until this crate the engine knew no terrain: herds walked a uniform grassland (choice Y3),
//! ships sailed an all-water mask (U11), and nothing founded a herd in a real world (Y4). Now:
//!
//! - [`model`]: the [`WorldModel`] trait every module queries (ground, water, climate,
//!   creatures), by block rectangle, never by storage (SCALE.md §2). Unknown ground is reported as
//!   such and modules avoid it (X2-5).
//! - [`habitat`]: one sampled 64-block cell, and the boundary records that carry it (HABITAT,
//!   HABITAT_KNOWN; docs/boundary.md §10).
//! - [`sampled`]: [`SampledWorld`], the vanilla provider fed by the core's habitat sampler.
//! - `persist`: its pages (a meta page and one page per region), a state hash checked on load.
//! - [`synth`]: a fake sampler that paints test worlds. Scaffolding.
//! - [`topology`]: how each frame's edges are joined (flat, an x wrap, Deep Time's glued
//!   layout) and the geometry every module measures through: canonical keys, shortest
//!   displacements, legs and headings across seams (DECISIONS.md C31).
//!
//! **Deep Time later** (PLAN.md §11, SCALE.md §12): a `mic_rtsim_deeptime` provider reads Deep
//! Time's baked tiles (`WorldData`, `DeepTimeClimate`) and implements [`WorldModel`] itself; the
//! engine hands it to the modules through the same [`WorldHandle`], and HABITAT records then
//! only fill what the bake lacks (or are ignored: [`WorldModel::sampled`] answers `None`).

pub mod habitat;
pub mod model;
mod persist;
pub mod sampled;
pub mod synth;
pub mod topology;

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub use habitat::{Habitat, HabitatRecord, KnownRecord, Surface};
pub use model::{Area, Creatures, WorldModel};
pub use persist::region_page_id;
pub use sampled::{Ingest, RegionKey, SampledWorld};
pub use topology::{SharedTopologies, Topologies, Topology};

/// The world model the engine's modules share: one provider behind a lock. The step is the only
/// writer (HABITAT records at its observe point) and readers run inside the same step, so the
/// lock is never contended; it exists so the herds' climate and the ships' water mask can hold
/// the model while the engine feeds it.
#[derive(Clone)]
pub struct WorldHandle(Arc<RwLock<Box<dyn WorldModel>>>);

impl WorldHandle {
    pub fn new(model: Box<dyn WorldModel>) -> WorldHandle {
        WorldHandle(Arc::new(RwLock::new(model)))
    }

    /// A handle on an empty vanilla sampler.
    pub fn sampled() -> WorldHandle {
        WorldHandle::new(Box::new(SampledWorld::new()))
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Box<dyn WorldModel>> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, Box<dyn WorldModel>> {
        self.0.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the provider (a load, or a Deep Time provider taking over).
    pub fn replace(&self, model: Box<dyn WorldModel>) {
        *self.write() = model;
    }
}

impl std::fmt::Debug for WorldHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorldHandle({})", self.read().name())
    }
}
