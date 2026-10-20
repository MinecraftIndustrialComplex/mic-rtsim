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

pub mod caravans;
pub mod check;
pub mod config;
pub mod flows;
pub mod observe;
pub mod synth;
pub mod world;

pub use caravans::{Attack, CaravanOutcome};
pub use config::WorldConfig;
pub use flows::PipeKind;
pub use observe::Side;
pub use world::{CaravanId, Cause, Command, NodeData, Outside, Stats, TraceRow, World};
