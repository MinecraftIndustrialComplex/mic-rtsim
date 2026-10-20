// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands the industry engine sends the Java side (docs/industry-boundary.md §4). Every
//! cross-store change carries an operation id (A9), allocated once when the change is decided and
//! resent unchanged, so Java applies it once however often it arrives.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;
use serde::Serialize;

use crate::ids::{BlockPos, ChannelId, ChunkPos, DistrictId, Good, GoodVec, TrainId};
use crate::obs::DepositPluginId;

/// One boundary inventory's change for one good.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct InvDelta {
    pub inventory: u32,
    pub pos: BlockPos,
    pub good: Good,
    /// Negative: remove. Java applies every removal of the command before any addition (I8).
    pub delta: i64,
}

/// A block to break with `level.destroyBlock(pos, true)` if it still has the expected state
/// (I23). Its contents drop as recoverable items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct BlockDestroy {
    pub pos: BlockPos,
    pub expected_state: u32,
}

/// Everything one reconcile epoch changes in one chunk (I8). Applied when the chunk next loads,
/// only if its `mic_rtsim:reconcile` attachment holds an older epoch for this district.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChunkReconcile {
    #[serde(skip)]
    pub op: OpId,
    pub district: DistrictId,
    pub epoch: u32,
    pub chunk: ChunkPos,
    /// Removals first, then additions; within each, by inventory then good.
    pub deltas: Vec<InvDelta>,
    pub destroy: Vec<BlockDestroy>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CarriageReason {
    /// The emulated station exchange (I15).
    Exchange,
    /// Cargo looted from a train leg (I24).
    Loot,
}

/// Changes to a train's `Carriage.storage`, applied while the carriage has no entity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CarriageDelta {
    #[serde(skip)]
    pub op: OpId,
    pub train: TrainId,
    pub reason: CarriageReason,
    /// (carriage index, good, delta into the carriage).
    pub entries: Vec<(u32, Good, i64)>,
}

/// A change to an Ender Transmission channel buffer (`MatterWorldSavedData`), applied at once: no
/// chunk is needed (I13).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChannelDelta {
    #[serde(skip)]
    pub op: OpId,
    pub channel: ChannelId,
    pub goods: GoodVec,
}

/// A change to a deposit that lives in level SavedData (Diesel's `OilChunksSavedData`), applied
/// at once. Chunk-bound deposits (Ore Excavation, Destroy crude) travel in [`ChunkReconcile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct DepositDelta {
    #[serde(skip)]
    pub op: OpId,
    pub plugin: DepositPluginId,
    pub chunk: ChunkPos,
    pub good: Good,
    pub delta: i64,
}

/// Packages written into a postbox's offline buffer (`PackageItem.containing`, `addAddress`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WritePackage {
    #[serde(skip)]
    pub op: OpId,
    pub district: DistrictId,
    pub inventory: u32,
    pub pos: BlockPos,
    pub address: u32,
    pub count: u32,
    /// The contents of one package.
    pub goods: GoodVec,
}

/// One line of the read-only ledger, for the Display Source (I18).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LedgerLine {
    pub district: DistrictId,
    pub state: u32,
    pub freeze: u32,
    #[serde(serialize_with = "crate::ser::opt_tick")]
    pub next_breakpoint: Option<Tick>,
    pub next_kind: u32,
    pub next_good: Option<Good>,
    #[serde(serialize_with = "crate::ser::tick")]
    pub since: Tick,
    /// (good, units per hour, what it made or used since `since`).
    pub goods: Vec<(Good, i64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Cmd {
    ChunkReconcile(ChunkReconcile),
    CarriageDelta(CarriageDelta),
    ChannelDelta(ChannelDelta),
    DepositDelta(DepositDelta),
    WritePackage(WritePackage),
    Ledger(LedgerLine),
}

impl Cmd {
    pub fn op(&self) -> Option<OpId> {
        match self {
            Cmd::ChunkReconcile(c) => Some(c.op),
            Cmd::CarriageDelta(c) => Some(c.op),
            Cmd::ChannelDelta(c) => Some(c.op),
            Cmd::DepositDelta(c) => Some(c.op),
            Cmd::WritePackage(c) => Some(c.op),
            Cmd::Ledger(_) => None,
        }
    }
}

/// The industry engine's op ids: the kernel's engine ids with namespace 0x49 ("I") in bits 56 to
/// 62, so they never collide with another engine allocator's (overnight choice F4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndustryOps {
    next: u64,
}

const NAMESPACE: u64 = 0x49 << 56;

impl IndustryOps {
    pub fn with_next(next: u64) -> Self {
        Self { next }
    }

    pub fn next_value(&self) -> u64 {
        self.next
    }

    pub fn allocate(&mut self) -> OpId {
        let n = self.next;
        self.next += 1;
        assert!(n < 1 << 56, "industry op ids exhausted");
        OpId::engine(NAMESPACE | n)
    }

    pub fn is_industry(op: OpId) -> bool {
        op.is_engine() && op.0 >> 56 == 0x49
    }
}
