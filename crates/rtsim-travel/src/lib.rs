// SPDX-License-Identifier: AGPL-3.0-or-later
//! Travel (SCALE.md §3, PLAN.md §8.6): how goods, money and news move between markets.
//!
//! - [`pipeline`]: one good flowing one way along one edge, as rate segments with closed-form
//!   arrivals and transit, and holes for consignments that left the flow.
//! - [`lattice`]: when caravans leave, so the caravans on a road partition what is in transit.
//! - [`edge`]: an edge's two directions: pipelines for goods, payments and raids, the caravan
//!   lattice, news cursors; predator havens (C24).
//! - [`news`]: market snapshots, shared and immutable, and each market's capped news ring.
//! - [`alloc`]: allocation of a market's exportable rate by news margin, best first.
//! - [`caravan`]: travellers sampled from flows, individual caravan rows, groups (C25), cargo in
//!   the chests (C21).
//! - [`legs`]: the region-and-time leg index with generation stamps and cascade cancellation
//!   (A15).
//!
//! What happens at a market when a flow arrives is `rtsim-world`'s: this crate holds the state
//! and the closed forms, not the step loop.

pub mod alloc;
pub mod caravan;
pub mod edge;
pub mod lattice;
pub mod legs;
pub mod news;
pub mod pipeline;

pub use caravan::{Body, Caravan, CaravanKey, GroupKind, Leg, Member, Stage, Traveller};
pub use edge::{Dir, DirRef, Edge, Predation};
pub use lattice::Lattice;
pub use legs::{LegIndex, Meeting, Path, Stamp};
pub use news::{NewsRing, Snapshot};
pub use pipeline::Pipeline;
