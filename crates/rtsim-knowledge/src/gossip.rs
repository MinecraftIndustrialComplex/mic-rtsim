// SPDX-License-Identifier: AGPL-3.0-or-later
//! Writing into vanilla gossip (C20, PLAN.md §8.7). When a report about a player reaches a vanilla
//! village, by witness or by news, the engine queues a [`VanillaGossip`] command; the bridge writes
//! it into the village's villagers through `GossipContainer.add(UUID, GossipType, int)` as the
//! player's entry, so vanilla's own reputation pricing and iron golems carry the consequence.
//!
//! Vanilla already writes gossip for what its villagers see themselves (a villager hurt or killed,
//! a trade with that villager), so a first-hand resident assault, resident killing or trade at a
//! vanilla village writes nothing: nothing counts twice. News of those from elsewhere is what
//! vanilla cannot know, so it is written.

use crate::actor::ActorId;
use crate::report::{ReportId, ReportKind};
use rtsim_kernel::clock::Tick;

/// Vanilla's `GossipType`, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum GossipType {
    MajorNegative = 1,
    MinorNegative = 2,
    MinorPositive = 3,
    MajorPositive = 4,
    Trading = 5,
}

/// One write into a vanilla village's gossip, for the bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VanillaGossip {
    pub village: ActorId,
    pub player: ActorId,
    pub gossip: GossipType,
    pub amount: i32,
    pub report: ReportId,
    /// What the report was and when it happened, kept with the write: by the time the bridge
    /// writes it the report itself may have expired from the table.
    pub kind: ReportKind,
    pub at: Tick,
    /// The village witnessed it (first-hand), not heard it as news.
    pub first_hand: bool,
}

/// Which gossip a report kind writes, and how much at the base severity, first-hand. `None` writes
/// no vanilla gossip at all: the TALK news kinds (TALK.md §9.4) are news, not a grudge.
/// PLACEHOLDER: P18 lists "which `GossipType` and amount each report kind writes" as a value to
/// set at the design gate. The amounts reuse the vanilla amounts P18 cites (a villager hurt adds 25
/// minor negative, killed 25 major negative, a trade 2 trading).
pub fn vanilla_mapping(kind: ReportKind) -> Option<(GossipType, i32)> {
    Some(match kind {
        ReportKind::Assault | ReportKind::ResidentAssault => (GossipType::MinorNegative, 25),
        ReportKind::Killing | ReportKind::ResidentKilling => (GossipType::MajorNegative, 25),
        // PLACEHOLDER: taking cargo or a llama counts as a killing would (major negative).
        ReportKind::Robbery | ReportKind::Theft => (GossipType::MajorNegative, 25),
        ReportKind::Trade => (GossipType::Trading, 2),
        // PLACEHOLDER: vanilla keeps major positive for curing a zombie villager.
        ReportKind::Rescue => (GossipType::MinorPositive, 25),
        // PLACEHOLDER: only dragons witness encroachment, so no village writes it; minor negative
        // keeps the mapping total (choices-oilgate.md OGC3).
        ReportKind::Encroachment => (GossipType::MinorNegative, 25),
        // News only (TALK.md §9.4): villagers say it, vanilla's gossip never hears of it.
        ReportKind::Feud
        | ReportKind::PactSworn
        | ReportKind::PactBroken
        | ReportKind::Raid
        | ReportKind::Fell
        | ReportKind::PortRaided
        | ReportKind::Blockade
        | ReportKind::PortClosed
        | ReportKind::PortReopened
        | ReportKind::HavenFell => return None,
    })
}

/// Whether vanilla already records this kind when its own villagers witness it. False for the TALK
/// news kinds, which have no vanilla gossip at all.
pub fn vanilla_sees_first_hand(kind: ReportKind) -> bool {
    matches!(
        kind,
        ReportKind::ResidentAssault | ReportKind::ResidentKilling | ReportKind::Trade
    )
}
