// SPDX-License-Identifier: AGPL-3.0-or-later
//! mic-rtsim's conflict engine (PLAN.md §10.3; C16 to C18, C24, C25; P14's recommendation):
//! factions (goblin tribes, bandit clans, raiders) that hold grievances against players, climb an
//! escalation ladder, and send war bands down the roads to raid player bases, supply lines and
//! villages, whether or not anyone is online. Headless: the body mods are open (P8: Goblin's
//! Tyranny, Illager Invasion; D7: bodies look like mobs), so the engine works on abstract body
//! kinds and speaks [`boundary`] records to a future conflict plugin.
//!
//! - [`culture`]: cultures as data (JSON in `data/cultures/`): tolerances per signal, thresholds,
//!   units, war band make-ups, havens. Every number there is a TUNABLE PLACEHOLDER.
//! - [`tunables`]: the culture-independent numbers (spawn arcs, quotas, the village defence).
//! - [`grievance`]: the closed-form ledger: exponential forgiveness plus a constant inflow
//!   between events, so a read costs the same after a day as after ten years, and the next rung of
//!   the ladder is one booked event (the time the curve crosses its threshold).
//! - [`engine`]: [`engine::Conflict`], every faction, feud and party as events on one timing
//!   wheel, composed with `rtsim-world`'s [`rtsim_world::World`] (havens are its haven nodes,
//!   marches are legs in its leg index, chiefs are its knowledge's notables, village raids are
//!   transfers between its markets) and the industry threat interface (`rtsim_ecology::infra`,
//!   which every raid on a player district goes through: never better than observed, capped,
//!   applied on load, C16 to C18, I22, I23).
//! - [`boundary`]: the records and their reference codec (docs/boundary.md §9).
//! - [`tables`]: the row tables (factions with their feuds, sources, the place index) on the
//!   kernel's paged rows: resident without a page store, paged under a budget with one
//!   ([`engine::Conflict::attach_store`], [`engine::Conflict::open_paged`]).
//! - [`synth`]: a forest, a goblin camp and a fake plugin for tests, the CLI and the bench.
//!   Scaffolding.
//!
//! # Attribution (P14's recommendation)
//!
//! A direct act (a block broken in a territory, ore drained, a member killed, a tribute refused)
//! counts against the player who did it. Unowned signals (pollution, a district's emissions while
//! unloaded) count against one shared **settlers** actor ([`engine::Conflict::settlers`]). Each
//! (faction, actor) pair is a **feud** with its own grievance and its own rung on the ladder.
//!
//! # Composition
//!
//! As the dragons and the ships do, this crate composes with the phase 1 world through its public
//! API; the caller owns the world and passes it in, and [`engine::Conflict::run_until`]
//! interleaves the conflict's events with the world's in time order.

pub mod boundary;
pub mod culture;
pub mod engine;
pub mod grievance;
mod outbox;
mod persist;
pub mod presence;
pub mod scenario;
pub mod synth;
pub mod tables;
pub mod tunables;
pub mod view;

pub use culture::{CultureDef, Cultures, SignalKind, Step, UnitClass};
pub use engine::{Conflict, ConflictStats, FeudView, Purpose};
pub use grievance::Grievance;
pub use outbox::GateCounts;
pub use persist::ConflictTickets;
pub use tunables::ConflictTunables;
