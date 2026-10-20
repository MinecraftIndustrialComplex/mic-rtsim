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
///
/// The low 56 bits are **derived from the command's identity** (docs/boundary.md §2.2, overnight
/// choice W3): a hash of the world seed, the engine's tick when the change was decided, and the
/// command itself (kind, target, epoch, every delta), never a counter. A counter restored from
/// the pages hands out again, after a crash, the ids it gave since the last save, to *different*
/// commands, which the bridge (still holding the first one's COMMAND_RESULT, or a chunk still
/// carrying its IND_RECONCILE_RESULT) answers DUPLICATE and drops. Re-deriving the *same* decision
/// gives the same id, so the bridge recognises it. Two identical commands decided in one tick get
/// the second id from an ordinal, as does a hash that meets another id of the same tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndustryOps {
    /// The tick of the last id derived.
    at: Tick,
    /// The low bits of the ids derived at `at`.
    issued: Vec<u64>,
}

const NAMESPACE: u64 = 0x49 << 56;
const LOW_BITS: u64 = (1 << 56) - 1;

impl IndustryOps {
    /// The op id of `cmd` (its own op field is ignored), decided at `at` in the world `seed`.
    pub fn derive(&mut self, seed: u64, at: Tick, cmd: &Cmd) -> OpId {
        if at != self.at {
            self.at = at;
            self.issued.clear();
        }
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(&seed.to_le_bytes());
        bytes.extend_from_slice(&at.0.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let mut w = rtsim_kernel::codec::Writer::new();
        crate::persist::Persist::put(&cmd.without_op(), &mut w);
        bytes.extend_from_slice(&w.into_bytes());
        for ordinal in 0u64.. {
            bytes[16..24].copy_from_slice(&ordinal.to_le_bytes());
            let low = mix(rtsim_kernel::rng::fnv1a64(&bytes)) & LOW_BITS;
            if low != 0 && !self.issued.contains(&low) {
                self.issued.push(low);
                return OpId::engine(NAMESPACE | low);
            }
        }
        unreachable!("an ordinal is always free")
    }

    pub fn is_industry(op: OpId) -> bool {
        op.is_engine() && op.0 >> 56 == 0x49
    }
}

/// SplitMix64's finalizer: spreads FNV's bits over the 56 kept.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Cmd {
    /// The command with its op id cleared: its identity for [`IndustryOps::derive`].
    pub fn without_op(&self) -> Cmd {
        let mut c = self.clone();
        match &mut c {
            Cmd::ChunkReconcile(x) => x.op = OpId(0),
            Cmd::CarriageDelta(x) => x.op = OpId(0),
            Cmd::ChannelDelta(x) => x.op = OpId(0),
            Cmd::DepositDelta(x) => x.op = OpId(0),
            Cmd::WritePackage(x) => x.op = OpId(0),
            Cmd::Ledger(_) => {}
        }
        c
    }

    /// Sets the command's op id (no-op for a ledger line).
    pub fn set_op(&mut self, op: OpId) {
        match self {
            Cmd::ChunkReconcile(x) => x.op = op,
            Cmd::CarriageDelta(x) => x.op = op,
            Cmd::ChannelDelta(x) => x.op = op,
            Cmd::DepositDelta(x) => x.op = op,
            Cmd::WritePackage(x) => x.op = op,
            Cmd::Ledger(_) => {}
        }
    }
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    persist_struct!(InvDelta {
        inventory,
        pos,
        good,
        delta
    });
    persist_struct!(BlockDestroy {
        pos,
        expected_state
    });
    persist_struct!(ChunkReconcile {
        op,
        district,
        epoch,
        chunk,
        deltas,
        destroy
    });
    persist_unit_enum!(
        CarriageReason,
        [CarriageReason::Exchange, CarriageReason::Loot]
    );
    persist_struct!(CarriageDelta {
        op,
        train,
        reason,
        entries
    });
    persist_struct!(ChannelDelta { op, channel, goods });
    persist_struct!(DepositDelta {
        op,
        plugin,
        chunk,
        good,
        delta
    });
    persist_struct!(WritePackage {
        op,
        district,
        inventory,
        pos,
        address,
        count,
        goods
    });
    persist_struct!(LedgerLine {
        district,
        state,
        freeze,
        next_breakpoint,
        next_kind,
        next_good,
        since,
        goods
    });
    persist_struct!(IndustryOps { at, issued });

    impl Persist for Cmd {
        fn put(&self, w: &mut Writer) {
            match self {
                Cmd::ChunkReconcile(c) => {
                    w.u8(0);
                    c.put(w);
                }
                Cmd::CarriageDelta(c) => {
                    w.u8(1);
                    c.put(w);
                }
                Cmd::ChannelDelta(c) => {
                    w.u8(2);
                    c.put(w);
                }
                Cmd::DepositDelta(c) => {
                    w.u8(3);
                    c.put(w);
                }
                Cmd::WritePackage(c) => {
                    w.u8(4);
                    c.put(w);
                }
                Cmd::Ledger(c) => {
                    w.u8(5);
                    c.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Cmd::ChunkReconcile(Persist::get(r)?),
                1 => Cmd::CarriageDelta(Persist::get(r)?),
                2 => Cmd::ChannelDelta(Persist::get(r)?),
                3 => Cmd::DepositDelta(Persist::get(r)?),
                4 => Cmd::WritePackage(Persist::get(r)?),
                5 => Cmd::Ledger(Persist::get(r)?),
                _ => return invalid("command tag"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(n: i64) -> Cmd {
        Cmd::ChannelDelta(ChannelDelta {
            op: OpId(0),
            channel: ChannelId(7),
            goods: GoodVec::from_pairs([(Good(1), n)]),
        })
    }

    #[test]
    fn op_ids_come_from_identity_not_a_counter() {
        let mut a = IndustryOps::default();
        let x = a.derive(5, Tick(100), &channel(3));
        assert!(IndustryOps::is_industry(x), "{:#x}", x.0);
        // Re-deriving the same decision (an engine restored from older pages) gives the same id.
        let mut b = IndustryOps::default();
        assert_eq!(b.derive(5, Tick(100), &channel(3)), x);
        // A different decision at the same tick (what a crash makes of the same game time) does
        // not, and neither does the same one at another tick or in another world.
        assert_ne!(b.derive(5, Tick(100), &channel(4)), x);
        assert_ne!(IndustryOps::default().derive(5, Tick(101), &channel(3)), x);
        assert_ne!(IndustryOps::default().derive(6, Tick(100), &channel(3)), x);
        // Two identical decisions in one tick are two transfers with two ids, in a fixed order.
        let mut c = IndustryOps::default();
        let first = c.derive(5, Tick(100), &channel(3));
        let second = c.derive(5, Tick(100), &channel(3));
        assert_eq!(first, x);
        assert_ne!(second, first);
        let mut d = IndustryOps::default();
        d.derive(5, Tick(100), &channel(3));
        assert_eq!(d.derive(5, Tick(100), &channel(3)), second);
        // The command's own op field is not part of its identity.
        let mut set = channel(3);
        set.set_op(OpId(99));
        assert_eq!(IndustryOps::default().derive(5, Tick(100), &set), x);
    }
}
