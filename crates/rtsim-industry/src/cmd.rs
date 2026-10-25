// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands the industry engine sends the Java side (docs/industry-boundary.md §4). Every
//! cross-store change carries an operation id (A9), allocated once when the change is decided and
//! resent unchanged, so Java applies it once however often it arrives.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;
use serde::Serialize;

use crate::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodVec, StoreId, TrainId};
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

/// Packages replays sent toward a package store's frogport, written into it while it is loaded
/// (IND_WRITE_CHAIN, SECTIONS.md §2.5, slice S4b): Java writes what fits, addressed to the
/// frogport's filter, and journals one `CHAIN_IN` with this op for what it wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WriteChain {
    #[serde(skip)]
    pub op: OpId,
    pub store: StoreId,
    pub district: DistrictId,
    pub inventory: u32,
    /// The frogport's index in the store's last IND_PACKAGE_STORE.
    pub frogport: u32,
    pub pos: BlockPos,
    pub count: u32,
    /// The contents of one package.
    pub goods: GoodVec,
}

/// What an IND_HELD_DRAW asks of the frontier's keys (slice S4c2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DrawAction {
    /// Mark them drawn and answer each with a `CHAIN_DRAWN` (choice SC141).
    Draw,
    /// Take their packages off the chain, at once or at their conveyor's next first tick, and
    /// answer each with a `CHAIN_RELEASED` (choice SC142).
    Remove,
    /// Java holds them drawn but the engine only held: they are held again (choice SC143).
    Return,
}

/// The frontier's held keys of one store and epoch that the engine draws, has removed or gives
/// back (IND_HELD_DRAW, SECTIONS.md §2.5, slice S4c2). No op id: Java answers each key with a
/// record, and a key of another epoch or one Java does not hold is skipped or released.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HeldDraw {
    pub store: StoreId,
    pub epoch: u32,
    pub action: DrawAction,
    /// Ascending.
    pub serials: Vec<u64>,
}

/// The serials one IND_HELD_DRAW carries at most: a record's size is a u16.
pub const HELD_DRAW_MAX_SERIALS: usize = (0xFFF8 - 32) / 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Cmd {
    ChunkReconcile(ChunkReconcile),
    CarriageDelta(CarriageDelta),
    DepositDelta(DepositDelta),
    WritePackage(WritePackage),
    /// IND_PROBE_SET (GLASSBOX.md §5.3, G-B14): buffers to add to a district's sweeps.
    ProbeSet(crate::glass::records::ProbeSet),
    /// IND_COUNT_SET (SECTIONS.md §4.2): the movers to count at a district's shared stores.
    CountSet(crate::glass::records::CountSet),
    WriteChain(WriteChain),
    HeldDraw(HeldDraw),
}

impl Cmd {
    pub fn op(&self) -> Option<OpId> {
        match self {
            Cmd::ChunkReconcile(c) => Some(c.op),
            Cmd::CarriageDelta(c) => Some(c.op),
            Cmd::DepositDelta(c) => Some(c.op),
            Cmd::WritePackage(c) => Some(c.op),
            Cmd::ProbeSet(_) | Cmd::CountSet(_) | Cmd::HeldDraw(_) => None,
            Cmd::WriteChain(c) => Some(c.op),
        }
    }
}

/// The industry engine's op ids: the kernel's engine ids with namespace 0x49 ("I") in bits 56 to
/// 62, so they never collide with another engine allocator's (overnight choice F4).
///
/// The low 56 bits are **derived from the command's identity** (docs/boundary.md §2.2, overnight
/// choice W3): a hash of the world seed, the tick of the event that decided the change, and the
/// command itself (kind, target, epoch, every delta), never a counter. A counter restored from
/// the pages hands out again, after a crash, the ids it gave since the last save, to *different*
/// commands, which the bridge (still holding the first one's COMMAND_RESULT, or a chunk still
/// carrying its IND_RECONCILE_RESULT) answers DUPLICATE and drops. Re-deriving the *same* decision
/// gives the same id, so the bridge recognises it. Two identical commands of one tick get the
/// second id from an ordinal, as does a hash that meets another recent id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndustryOps {
    /// The ids derived lately, each with the tick that decided it (a lazily replayed cluster can
    /// decide at a tick older than another's last decision, so this is a window, not one tick):
    /// the last [`RECENT`], and every one of the tick being derived ([`IndustryOps::remember`]).
    /// A key tag ([`KEY_TAG`] set, which no tick reaches) has a budget of its own. The same bytes
    /// as the `(Tick, u64)` pairs it held before: no format change.
    recent: std::collections::VecDeque<(u64, u64)>,
}

const NAMESPACE: u64 = 0x49 << 56;
const LOW_BITS: u64 = (1 << 56) - 1;
/// Ids remembered for ordinals and collisions. TUNABLE, PLACEHOLDER 256: identical commands come
/// from one cluster's burst of events.
const RECENT: usize = 256;
/// The bit that marks a key tag in [`IndustryOps::recent`] (slice T's keyed ids, RP-180): a tick
/// is a game time, below 2^63, so a tag and a tick never meet.
const KEY_TAG: u64 = 1 << 63;

impl IndustryOps {
    /// The op id of `cmd` (its own op field is ignored), decided at `at` in the world `seed`.
    pub fn derive(&mut self, seed: u64, at: Tick, cmd: &Cmd) -> OpId {
        self.derive_with(seed, at, cmd, &[])
    }

    /// [`IndustryOps::derive`] with `extra`, identity the command does not carry, hashed after it:
    /// a deposit delta's district (two districts on one oil chunk write back at one tick, RP-275).
    /// Empty, it is `derive`'s id.
    pub fn derive_with(&mut self, seed: u64, at: Tick, cmd: &Cmd, extra: &[u8]) -> OpId {
        self.derive_from(seed, at.0, at, cmd, extra)
    }

    /// The op id of the `n`th change of `cmd`'s kind and target, counted by the engine and saved
    /// with its pages, whatever tick decided it (`at` only places it in the window): a package
    /// written into a postbox's offline buffer (docs/choices-mowzie-build.md RP-259, P1-4 of the
    /// review). Decided again after a crash, the `n`th package meets the op the world applied
    /// whenever the replay decides it, and carries the same content.
    pub fn derive_numbered(&mut self, seed: u64, at: Tick, cmd: &Cmd, n: u64) -> OpId {
        self.derive_from(seed, NUMBERED, at, cmd, &n.to_le_bytes())
    }

    /// The id of `cmd` with `word` hashed where a derived id has its tick.
    fn derive_from(&mut self, seed: u64, word: u64, at: Tick, cmd: &Cmd, extra: &[u8]) -> OpId {
        let mut bytes = Vec::with_capacity(64 + extra.len());
        bytes.extend_from_slice(&seed.to_le_bytes());
        bytes.extend_from_slice(&word.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let mut w = rtsim_kernel::codec::Writer::new();
        crate::persist::Persist::put(&cmd.without_op(), &mut w);
        bytes.extend_from_slice(&w.into_bytes());
        bytes.extend_from_slice(extra);
        for ordinal in 0u64.. {
            bytes[16..24].copy_from_slice(&ordinal.to_le_bytes());
            let low = mix(rtsim_kernel::rng::fnv1a64(&bytes)) & LOW_BITS;
            if low != 0 && !self.recent.iter().any(|&(_, l)| l == low) {
                self.remember(at.0, low);
                return OpId::engine(NAMESPACE | low);
            }
        }
        unreachable!("an ordinal is always free")
    }

    /// Remembers `low`, derived at tick `by` (or for a key tag), and forgets past the window
    /// (docs/choices-mowzie-build.md RP-275). The oldest derived id goes once more than [`RECENT`]
    /// are kept, but never one of `by`'s own tick: two identical decisions of one tick more than
    /// [`RECENT`] derivations apart would otherwise both take ordinal 0, one op for two changes,
    /// which Java answers DUPLICATE (a loss in a live run). So the derived ids kept are at most
    /// [`RECENT`] or the derivations of the busiest tick, whichever is more: one tick's decisions,
    /// a round per chunk and a write per pool and good, are finite, and the next tick's first
    /// derivation trims the window back to [`RECENT`]. Key tags ([`KEY_TAG`]) are counted and
    /// forgotten apart, [`RECENT`] of them: a burst of derived ids never pushes a key out, so
    /// `knows_key` still knows a change another engine decided (a raid on a train, RP-183) when
    /// that engine's replay decides it again.
    fn remember(&mut self, by: u64, low: u64) {
        self.recent.push_back((by, low));
        let keyed = by & KEY_TAG != 0;
        let class = |b: u64| (b & KEY_TAG != 0) == keyed;
        let kept = self.recent.iter().filter(|&&(b, _)| class(b)).count();
        let mut over = kept.saturating_sub(RECENT);
        let mut i = 0;
        while over > 0 && i < self.recent.len() {
            let b = self.recent[i].0;
            if class(b) && (keyed || b != by) {
                self.recent.remove(i);
                over -= 1;
            } else {
                i += 1;
            }
        }
    }

    pub fn is_industry(op: OpId) -> bool {
        op.is_engine() && op.0 >> 56 == 0x49
    }
}

/// Where a derived id's hash has its tick, a numbered one's ([`IndustryOps::derive_numbered`]) has
/// this, which no game time reaches (slice T's keyed ids use `u64::MAX`, RP-180).
const NUMBERED: u64 = u64::MAX - 1;

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
            Cmd::DepositDelta(x) => x.op = OpId(0),
            Cmd::WritePackage(x) => x.op = OpId(0),
            Cmd::ProbeSet(_) | Cmd::CountSet(_) | Cmd::HeldDraw(_) => {}
            Cmd::WriteChain(x) => x.op = OpId(0),
        }
        c
    }

    /// Sets the command's op id.
    pub fn set_op(&mut self, op: OpId) {
        match self {
            Cmd::ChunkReconcile(x) => x.op = op,
            Cmd::CarriageDelta(x) => x.op = op,
            Cmd::DepositDelta(x) => x.op = op,
            Cmd::WritePackage(x) => x.op = op,
            Cmd::ProbeSet(_) | Cmd::CountSet(_) | Cmd::HeldDraw(_) => {}
            Cmd::WriteChain(x) => x.op = op,
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
    persist_struct!(WriteChain {
        op,
        store,
        district,
        inventory,
        frogport,
        pos,
        count,
        goods
    });
    persist_unit_enum!(
        DrawAction,
        [DrawAction::Draw, DrawAction::Remove, DrawAction::Return]
    );
    persist_struct!(HeldDraw {
        store,
        epoch,
        action,
        serials
    });
    persist_struct!(IndustryOps { recent });
    persist_struct!(crate::glass::records::ProbeSet {
        district,
        compile,
        nodes
    });
    persist_struct!(crate::glass::records::CountSet {
        district,
        compile,
        pairs
    });

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
                Cmd::DepositDelta(c) => {
                    w.u8(3);
                    c.put(w);
                }
                Cmd::WritePackage(c) => {
                    w.u8(4);
                    c.put(w);
                }
                Cmd::ProbeSet(c) => {
                    w.u8(6);
                    c.put(w);
                }
                Cmd::CountSet(c) => {
                    w.u8(7);
                    c.put(w);
                }
                // 8 left free.
                Cmd::WriteChain(c) => {
                    w.u8(9);
                    c.put(w);
                }
                Cmd::HeldDraw(c) => {
                    w.u8(10);
                    c.put(w);
                }
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Cmd::ChunkReconcile(Persist::get(r)?),
                1 => Cmd::CarriageDelta(Persist::get(r)?),
                3 => Cmd::DepositDelta(Persist::get(r)?),
                4 => Cmd::WritePackage(Persist::get(r)?),
                6 => Cmd::ProbeSet(Persist::get(r)?),
                7 => Cmd::CountSet(Persist::get(r)?),
                9 => Cmd::WriteChain(Persist::get(r)?),
                10 => Cmd::HeldDraw(Persist::get(r)?),
                _ => return invalid("command tag"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(n: i64) -> Cmd {
        Cmd::DepositDelta(DepositDelta {
            op: OpId(0),
            plugin: crate::obs::DepositPluginId::DieselOil,
            chunk: ChunkPos { dim: 0, x: 7, z: 0 },
            good: Good(1),
            delta: n,
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
        // No extra identity is `derive`'s id; some is another.
        assert_eq!(
            IndustryOps::default().derive_with(5, Tick(100), &channel(3), &[]),
            x
        );
        assert_ne!(
            IndustryOps::default().derive_with(5, Tick(100), &channel(3), &7u64.to_le_bytes()),
            x
        );
    }

    /// The slice R review's P2-3 (RP-275): two identical decisions of one tick more than
    /// [`RECENT`] derivations apart (a busy tick: a round per chunk and a write per pool). On
    /// 2a975cc3 the window had forgotten the first by the second, which took ordinal 0 again: one
    /// op for two changes, the second answered DUPLICATE in a live run.
    #[test]
    fn identical_decisions_of_one_tick_never_share_an_op_however_far_apart() {
        let mut a = IndustryOps::default();
        let first = a.derive(5, Tick(100), &channel(3));
        for n in 0..RECENT as i64 + 10 {
            a.derive(5, Tick(100), &channel(1_000 + n));
        }
        let second = a.derive(5, Tick(100), &channel(3));
        assert_ne!(second, first, "the second change has an op of its own");
        // The same tick's ids are all kept while it derives; the next tick trims back.
        assert_eq!(a.recent.len(), RECENT + 12);
        a.derive(5, Tick(101), &channel(3));
        assert_eq!(a.recent.len(), RECENT);
        assert_eq!(a.recent.back().map(|&(by, _)| by), Some(101));
        // Older ticks still go oldest first.
        let mut b = IndustryOps::default();
        for n in 0..RECENT as u64 + 5 {
            b.derive(5, Tick(n), &channel(3));
        }
        assert_eq!(b.recent.len(), RECENT);
        assert_eq!(b.recent.front().map(|&(by, _)| by), Some(5));
    }

    /// Key tags (slice T's keyed ids, RP-180) have a window of their own: a burst of derived ids
    /// never pushes one out (RP-275), so a change another engine decided is still known when its
    /// replay decides it again.
    #[test]
    fn a_burst_of_derived_ids_never_pushes_a_key_tag_out() {
        let mut a = IndustryOps::default();
        a.remember(KEY_TAG | 0x77, 0x1234);
        for n in 0..RECENT as u64 * 2 {
            a.derive(5, Tick(n), &channel(3));
        }
        assert!(a.recent.contains(&(KEY_TAG | 0x77, 0x1234)));
        assert_eq!(a.recent.len(), RECENT + 1);
        // Only other tags push it out, oldest first.
        for n in 0..RECENT as u64 {
            a.remember(KEY_TAG | (0x100 + n), 0x5000 + n);
        }
        assert!(!a.recent.contains(&(KEY_TAG | 0x77, 0x1234)));
        assert_eq!(a.recent.len(), RECENT * 2);
    }

    /// The window's page bytes are the `(Tick, u64)` pairs it held before: no format change.
    #[test]
    fn the_window_keeps_its_page_bytes() {
        use crate::persist::Persist;
        use rtsim_kernel::codec::{Reader, Writer};
        let mut ops = IndustryOps::default();
        ops.derive(5, Tick(100), &channel(3));
        ops.derive(5, Tick(101), &channel(4));
        let before: std::collections::VecDeque<(Tick, u64)> = ops
            .recent
            .iter()
            .map(|&(at, low)| (Tick(at), low))
            .collect();
        let mut old = Writer::new();
        before.put(&mut old);
        let mut new = Writer::new();
        ops.put(&mut new);
        assert_eq!(new.into_bytes(), old.into_bytes());
        let mut w = Writer::new();
        ops.put(&mut w);
        let bytes = w.into_bytes();
        let back = IndustryOps::get(&mut Reader::new(&bytes)).expect("decodes");
        assert_eq!(back, ops);
    }
}
