// SPDX-License-Identifier: AGPL-3.0-or-later
//! mic-rtsim's knowledge module (PLAN.md §5.6, §5.7, §8.7; SCALE.md §6, §9; C20, C24, C25):
//! what happened, who knows it, and how they feel about it.
//!
//! - **Reports** ([`report`]) are facts in packed 32-byte rows in one capped table, each kind with
//!   a lifetime, folding into counters when they expire.
//! - **Holders** (villages, player bases, caravans, notables) know reports by handle on a
//!   [`Noticeboard`] and keep **sentiment** ([`sentiment`]) toward actors: 8-byte entries, capped,
//!   decaying stochastically and lazily, slower when strong, never evicting the strongest first.
//! - **Witnesses** learn first-hand ([`Knowledge::witness`], [`witnesses`]). Everyone else learns
//!   only from a [`Packet`] a traveller carried ([`Knowledge::packet_since`],
//!   [`Knowledge::deliver`]): news spreads at the speed of trade, and a village nobody trades with
//!   learns nothing.
//! - **Rescues and fair trades** are positive reports (C24).
//! - **Notables** ([`notable`]) remember reports in 8-byte memory slots, die on one booked event,
//!   and pass their strongest memories and feelings to an heir or their home (C24).
//! - **Temperament** ([`Temperament`]) is derived from a seed and stored nowhere (C25).
//!
//! # Engine side only
//!
//! This crate decides; Minecraft acts through the bridge. The bridge writes the
//! [`VanillaGossip`] commands into villagers' `GossipContainer`, so vanilla's reputation pricing and
//! iron golems carry the consequence (C20); the ribbit sorcerer mixin reads a sentiment snapshot
//! ([`Knowledge::sentiment_snapshot`]) to leave out players a village dislikes (PLAN.md §8.8). No
//! report, holder or sentiment is ever a Minecraft object.
//!
//! # Determinism and laziness
//!
//! Reads ([`Knowledge::sentiment`], snapshots) commit nothing, and the decay draws depend only on
//! (world seed, holder, target, the anchor event's tick, step), so what a holder feels at a time
//! never depends on who looked before. Iteration is in arena or B-tree order. Every tunable is in
//! [`Tunables`], each a PLACEHOLDER waiting on P18 unless its comment cites a recommendation.

pub mod actor;
pub mod board;
pub mod gossip;
pub mod knowledge;
pub mod notable;
mod persist;
pub mod report;
pub mod sentiment;
pub mod temperament;
pub mod tunables;

pub use actor::{Actor, ActorId, ActorKind, Actors};
pub use board::{BoardCursor, Noticeboard, Packet};
pub use gossip::{GossipType, VanillaGossip};
pub use knowledge::{HolderKind, Knowledge, witnesses};
pub use notable::{MemorySlot, Notable};
pub use report::{FLAG_PLAYER, Place, Report, ReportId, ReportKind, ReportRow, ReportTable};
pub use sentiment::{SentimentEntry, Sentiments};
pub use temperament::{Temperament, Trait};
pub use tunables::{SEVERITY_UNIT, Tunables};
