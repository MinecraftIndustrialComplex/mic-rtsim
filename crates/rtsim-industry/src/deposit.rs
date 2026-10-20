// SPDX-License-Identifier: AGPL-3.0-or-later
//! Finite deposits (INDUSTRY.md §6 case 4, I11): the interface the Ore Excavation, Diesel oil and
//! Destroy crude-oil plugins implement on the Java side, and what the engine needs to know about
//! each.
//!
//! **In the engine a deposit is a good of class `Deposit`.** The sampler reads its remaining amount
//! in every sweep (IND_DEPOSIT_SAMPLE), so it is part of the district's stock, and learning sees
//! it fall: it becomes a consumed input of the box with no special code. Its exhaustion is an
//! input-empty breakpoint, which the ledger names "deposit exhausted". Infinite veins (Diesel) are
//! left out of the stock, so they never run out.
//!
//! **Write-back** happens at the district's reconcile, with its epoch. Chunk-bound stores (Ore
//! Excavation's `OreDataAttachment`, Destroy's `chunk_crude_oil` attachment) travel in the chunk's
//! IND_CHUNK_RECONCILE, so the depletion and the chunk's contents reach disk together. Level
//! SavedData (Diesel's `OilChunksSavedData`) is written at once with IND_DEPOSIT_DELTA, loaded or
//! not (overnight choice F15).

use serde::Serialize;

use crate::obs::{DepositPluginId, DepositSample};

/// When a deposit's depletion is written back into the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Writeback {
    /// In the chunk's reconcile, with the epoch (the store lives in chunk NBT).
    WithChunk,
    /// At once, by command (the store is level SavedData).
    Immediate,
}

/// What a deposit plugin is, engine side.
pub trait DepositPlugin: Sync {
    fn id(&self) -> DepositPluginId;
    fn writeback(&self) -> Writeback;
    /// The Java call that reads the remaining amount.
    fn read_hook(&self) -> &'static str;
    /// The Java call that applies a depletion.
    fn write_hook(&self) -> &'static str;
    /// Whether this sample means the deposit never runs out.
    fn infinite(&self, s: &DepositSample) -> bool {
        s.infinite
    }
}

/// Create Ore Excavation 1.6.8: veins in a chunk attachment.
pub struct OreExcavation;

impl DepositPlugin for OreExcavation {
    fn id(&self) -> DepositPluginId {
        DepositPluginId::OreExcavation
    }
    fn writeback(&self) -> Writeback {
        Writeback::WithChunk
    }
    fn read_hook(&self) -> &'static str {
        "OreData.getResourcesRemaining"
    }
    fn write_hook(&self) -> &'static str {
        "OreData.extract"
    }
}

/// Create Diesel Generators 1.3.15: oil per chunk in level SavedData; some veins are infinite.
pub struct DieselOil;

impl DepositPlugin for DieselOil {
    fn id(&self) -> DepositPluginId {
        DepositPluginId::DieselOil
    }
    fn writeback(&self) -> Writeback {
        Writeback::Immediate
    }
    fn read_hook(&self) -> &'static str {
        "OilChunksSavedData.getChunkOilAmount"
    }
    fn write_hook(&self) -> &'static str {
        "OilChunksSavedData.setChunkOilAmount"
    }
}

/// Destroy 0.4.1: crude oil in the `chunk_crude_oil` chunk attachment.
pub struct DestroyCrude;

impl DepositPlugin for DestroyCrude {
    fn id(&self) -> DepositPluginId {
        DepositPluginId::DestroyCrude
    }
    fn writeback(&self) -> Writeback {
        Writeback::WithChunk
    }
    fn read_hook(&self) -> &'static str {
        "chunk_crude_oil attachment (read)"
    }
    fn write_hook(&self) -> &'static str {
        "chunk_crude_oil attachment (write)"
    }
}

/// The plugin for an id.
pub fn plugin(id: DepositPluginId) -> &'static dyn DepositPlugin {
    match id {
        DepositPluginId::OreExcavation => &OreExcavation,
        DepositPluginId::DieselOil => &DieselOil,
        DepositPluginId::DestroyCrude => &DestroyCrude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writeback_routes() {
        assert_eq!(plugin(DepositPluginId::OreExcavation).writeback(), Writeback::WithChunk);
        assert_eq!(plugin(DepositPluginId::DestroyCrude).writeback(), Writeback::WithChunk);
        assert_eq!(plugin(DepositPluginId::DieselOil).writeback(), Writeback::Immediate);
    }
}
