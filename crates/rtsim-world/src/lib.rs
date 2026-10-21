// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world (PLAN.md §8): settlements, storehouse markets, flows and caravans, and
//! knowledge, composed, with the step loop and the calls the bridge will make.
//!
//! This is scaffolding in one sense: PLAN.md §5.4 has generic modules register tables, systems and
//! handlers with a scheduler, and that registry does not exist yet. Until it does, this crate is
//! the composition, and the JNI crate drives it (overnight choice E1).
//!
//! - [`World::build`] and [`World::join`] (C22): markets from data, edges by the rule.
//! - [`World::run_until`]: market breakpoints, wakes, news deliveries and caravan arrivals, in
//!   time order. Nothing is ticked: a quiet world costs nothing.
//! - [`World::trade`], [`World::offers`], [`World::base_trade`]: players and the treasury (C14,
//!   C23, C26).
//! - [`World::travellers`], [`World::promote`], [`World::rob`], [`World::rescue`],
//!   [`World::take_member`], [`World::kill_member`]: caravans near players (C21, C24, C25).
//! - [`World::check_conserved`], [`World::state_hash`]: the ledger and the determinism hash.
//! - [`World::trade_exact`], [`World::caravan_trade_exact`], [`World::rob_goods`] and the rest of
//!   [`bridge`]: what the bridge reports, applied exactly as it happened in Minecraft.
//! - [`World::take_stock`], [`World::prey_on_traveller`]: predators with no haven (PLAN.md §9).

pub mod archive;
pub mod bases;
pub mod bridge;
pub mod caravans;
pub mod catalogue;
pub mod check;
pub mod config;
pub mod flows;
pub mod inputs;
pub mod interest;
pub mod lanes;
pub mod memory;
pub mod observe;
pub mod persist;
pub mod prey;
pub mod raids;
pub mod ribbits;
pub mod synth;
pub mod topology;
pub mod works;
pub mod world;

pub use caravans::{Attack, CaravanOutcome};
pub use config::WorldConfig;
pub use flows::PipeKind;
pub use interest::{PlayerState, Viewer, Visible};
pub use lanes::{Cut, LaneSpec};
pub use observe::Side;
pub use ribbits::{BandView, PlaceHouse, SentimentSnapshot, SpawnResident, identity_op};
pub use world::{CaravanId, Cause, Command, NodeData, Outside, Stats, TraceRow, Via, World};
