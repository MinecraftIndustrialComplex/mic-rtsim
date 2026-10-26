// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands the industry engine sends the Java side (docs/industry-boundary.md §4). Every
//! cross-store change carries an operation id (A9), allocated once when the change is decided and
//! resent unchanged, so Java applies it once however often it arrives.

use std::collections::{BTreeMap, VecDeque};

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
#[derive(Clone, Debug, Default)]
pub struct IndustryOps {
    /// The ids derived lately, each with the tick that decided it (a lazily replayed cluster can
    /// decide at a tick older than another's last decision, so this is a window, not one tick),
    /// keyed by that tick and the order they came in, so the oldest tick's go first: the last
    /// [`RECENT`], and every one at or above the step's oldest tick, [`CAP`] more at most
    /// ([`IndustryOps::remember`]).
    derived: BTreeMap<(u64, u64), u64>,
    /// The key tags ([`KEY_TAG`] set, which no tick reaches) and their ids, oldest first: a
    /// budget of their own.
    keys: VecDeque<(u64, u64)>,
    /// How many of the window's ids have each low 56 bits: the probe, one lookup a derivation
    /// however many the window holds (the slice R re-check's step 3b).
    lows: BTreeMap<u64, u32>,
    /// The order the next derived id comes in. Not saved: a restored window numbers its ids
    /// again in their page order, which is that order.
    next: u64,
    /// The oldest tick derived since the step began ([`IndustryOps::end_step`]), None before the
    /// step's first derivation. Not saved, and not part of the state: a restored engine starts a
    /// step. Which ids a window forgets decides only whether a later identical decision of their
    /// tick takes a fresh ordinal; ordinals are taken lowest free first, so the ops of one tick's
    /// identical decisions are a prefix live and replayed alike, and two windows can differ by a
    /// loss, never a double, barring a 56-bit collision (an unrelated id in one window alone
    /// with an ordinal's low bits moves a decision to the next ordinal, past the prefix).
    floor: Option<u64>,
    /// The newest tick derived since the step began: [`IndustryOps::end_step`] keeps its ids.
    newest: Option<u64>,
}

/// Two windows are equal when their pages are: the step's ticks and the order numbers are not
/// part of the state.
impl PartialEq for IndustryOps {
    fn eq(&self, other: &Self) -> bool {
        self.pairs().eq(other.pairs())
    }
}
impl Eq for IndustryOps {}

const NAMESPACE: u64 = 0x49 << 56;
const LOW_BITS: u64 = (1 << 56) - 1;
/// Ids remembered for ordinals and collisions. TUNABLE, PLACEHOLDER 256: identical commands come
/// from one cluster's burst of events.
const RECENT: usize = 256;
/// The derived ids at or above the step's oldest tick kept past [`RECENT`] at most (the slice R
/// re-check's step 3b). TUNABLE, PLACEHOLDER 4,096: a step that decides more (a jni replay since
/// the last durable pages, a catch-up over a long time off) forgets its oldest ticks' ids first,
/// and an identical decision of such a tick later in the step takes an op of its tick again, one
/// op for two changes, a loss (docs/choices-mowzie-build.md RP-259).
const CAP: usize = 4_096;
/// The bit that marks a key tag in [`IndustryOps::keys`] (slice T's keyed ids, RP-180): a tick
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
            if low != 0 && !self.lows.contains_key(&low) {
                self.remember(at.0, low);
                return OpId::engine(NAMESPACE | low);
            }
        }
        unreachable!("an ordinal is always free")
    }

    /// Remembers `low`, derived at tick `by` (or for a key tag), and forgets past the window
    /// (docs/choices-mowzie-build.md RP-275, RP-259). The oldest tick's derived id goes once more
    /// than [`RECENT`] are kept, but never one at or above the oldest tick derived in the step
    /// until [`CAP`] more are kept: two identical decisions of one tick more than [`RECENT`]
    /// derivations apart would otherwise both take ordinal 0, one op for two changes, which Java
    /// answers DUPLICATE (a loss in a live run), and a step decides at older ticks than its newest
    /// (a stale record, a cluster behind another), so keeping `by`'s own tick alone is not enough
    /// (the slice R re-check). Past the cap the step forgets its own oldest ticks' ids, a loss at
    /// worst (step 3b). The step ends at [`IndustryOps::end_step`], which trims the window back
    /// to [`RECENT`] plus the ids at the step's newest tick. A derivation costs a few map
    /// operations, whatever the window holds. Key tags ([`KEY_TAG`]) are counted and forgotten
    /// apart, [`RECENT`] of them: a burst of derived ids never pushes a key out, so `knows_key`
    /// still knows a change another engine decided (a raid on a train, RP-183) when that engine's
    /// replay decides it again.
    fn remember(&mut self, by: u64, low: u64) {
        self.restore(by, low);
        if by & KEY_TAG != 0 {
            if self.keys.len() > RECENT
                && let Some((_, gone)) = self.keys.pop_front()
            {
                self.forget(gone);
            }
            return;
        }
        let floor = self.floor.map_or(by, |f| f.min(by));
        self.floor = Some(floor);
        self.newest = Some(self.newest.map_or(by, |t| t.max(by)));
        self.trim(floor);
    }

    /// Puts back an id of a saved window, in its page order, forgetting nothing.
    fn restore(&mut self, by: u64, low: u64) {
        *self.lows.entry(low).or_default() += 1;
        if by & KEY_TAG != 0 {
            self.keys.push_back((by, low));
        } else {
            self.derived.insert((by, self.next), low);
            self.next += 1;
        }
    }

    /// Forgets the oldest ticks' derived ids past [`RECENT`] below `keep`, and past [`RECENT`]
    /// and [`CAP`] whatever their tick.
    fn trim(&mut self, keep: u64) {
        while self.derived.len() > RECENT {
            let over_cap = self.derived.len() > RECENT + CAP;
            let Some(oldest) = self.derived.first_entry() else {
                break;
            };
            if oldest.key().0 >= keep && !over_cap {
                break;
            }
            let low = oldest.remove();
            self.forget(low);
        }
    }

    /// One id of `low` leaves the window.
    fn forget(&mut self, low: u64) {
        if let Some(n) = self.lows.get_mut(&low) {
            *n -= 1;
            if *n == 0 {
                self.lows.remove(&low);
            }
        }
    }

    /// The window as its page holds it: the derived ids oldest tick first, then the key tags.
    fn pairs(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.derived
            .iter()
            .map(|(&(by, _), &low)| (by, low))
            .chain(self.keys.iter().copied())
    }

    /// The step is over (the engine's `advance_to`): the window keeps the last [`RECENT`] and the
    /// ids at or above the step's newest tick (the next step may decide at it again), so a page
    /// saved at the step's end holds no more, and the next step's derivations may forget the
    /// step's ids again, oldest tick first.
    pub fn end_step(&mut self) {
        if let Some(newest) = self.newest {
            self.trim(newest);
        }
        self.floor = None;
        self.newest = None;
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
    /// The window alone, as the `(Tick, u64)` pairs it held before ([`IndustryOps::pairs`]):
    /// the step's ticks are not saved ([`IndustryOps::floor`]).
    impl Persist for IndustryOps {
        fn put(&self, w: &mut Writer) {
            self.pairs().collect::<Vec<_>>().put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            let mut ops = IndustryOps::default();
            for (by, low) in Vec::<(u64, u64)>::get(r)? {
                ops.restore(by, low);
            }
            Ok(ops)
        }
    }
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
        // A numbered id (a package, RP-259) is its number's at any tick, and two numbers at one
        // tick (the review's table: several packages of one pool at one tick) are two ids.
        let n1 = IndustryOps::default().derive_numbered(5, Tick(100), &channel(3), 1);
        assert_eq!(
            IndustryOps::default().derive_numbered(5, Tick(9_000), &channel(3), 1),
            n1
        );
        let mut e = IndustryOps::default();
        assert_eq!(e.derive_numbered(5, Tick(100), &channel(3), 1), n1);
        assert_ne!(e.derive_numbered(5, Tick(100), &channel(3), 2), n1);
        assert_ne!(n1, x);
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
        // The step's ids are all kept while it derives, and at its end, all at its newest tick;
        // the next step trims back.
        assert_eq!(a.pairs().count(), RECENT + 12);
        a.end_step();
        assert_eq!(a.pairs().count(), RECENT + 12);
        a.derive(5, Tick(101), &channel(3));
        assert_eq!(a.pairs().count(), RECENT);
        assert_eq!(a.pairs().last().map(|(by, _)| by), Some(101));
        // Older steps' ticks still go oldest first.
        let mut b = IndustryOps::default();
        for n in 0..RECENT as u64 + 5 {
            b.derive(5, Tick(n), &channel(3));
            b.end_step();
        }
        assert_eq!(b.pairs().count(), RECENT);
        assert_eq!(b.pairs().next().map(|(by, _)| by), Some(5));
    }

    /// The slice R re-check's P3: a derivation at an older tick in the same step (a stale record,
    /// a cluster that decides behind another) once forgot the step's newer ids past [`RECENT`],
    /// and the next identical decision at the newer tick took ordinal 0 again: one op for two
    /// changes, the second answered DUPLICATE. Every tick at or above the step's oldest is kept.
    #[test]
    fn an_older_ticks_decision_never_makes_the_step_forget_a_newer_ticks_ids() {
        let mut a = IndustryOps::default();
        let first = a.derive(5, Tick(100), &channel(3));
        let stale = a.derive(5, Tick(90), &channel(4));
        for n in 0..RECENT as i64 + 10 {
            a.derive(5, Tick(100), &channel(1_000 + n));
        }
        // A stale record of the same step decides at 90.
        a.derive(5, Tick(90), &channel(5));
        let second = a.derive(5, Tick(100), &channel(3));
        let stale_again = a.derive(5, Tick(90), &channel(4));
        assert_eq!(
            (second == first, stale_again == stale),
            (false, false),
            "each change has an op of its own: first {:#x} second {:#x}; stale {:#x} again {:#x}",
            first.0,
            second.0,
            stale.0,
            stale_again.0
        );
        // The step ends: the window keeps RECENT plus the ids at the step's newest tick, and the
        // next step's derivations trim back to RECENT plus the ids at or above its oldest tick.
        assert_eq!(a.pairs().count(), RECENT + 15);
        a.end_step();
        assert_eq!(
            a.pairs().count(),
            RECENT + 12,
            "the 100s kept, the 90s gone"
        );
        assert!(a.pairs().all(|(by, _)| by == 100));
        a.derive(5, Tick(100), &channel(6));
        assert_eq!(a.pairs().count(), RECENT + 13);
        a.end_step();
        a.derive(5, Tick(200), &channel(6));
        assert_eq!(a.pairs().count(), RECENT);
        // A restored window starts a step.
        let mut w = rtsim_kernel::codec::Writer::new();
        crate::persist::Persist::put(&a, &mut w);
        let bytes = w.into_bytes();
        let mut back: IndustryOps =
            crate::persist::Persist::get(&mut rtsim_kernel::codec::Reader::new(&bytes))
                .expect("decodes");
        assert_eq!(back.floor, None);
        assert_eq!(back, a);
        back.derive(5, Tick(150), &channel(7));
        assert_eq!(back.pairs().count(), RECENT);
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
            a.end_step();
        }
        assert!(a.pairs().any(|p| p == (KEY_TAG | 0x77, 0x1234)));
        assert_eq!(a.pairs().count(), RECENT + 1);
        // Only other tags push it out, oldest first.
        for n in 0..RECENT as u64 {
            a.remember(KEY_TAG | (0x100 + n), 0x5000 + n);
        }
        assert!(!a.pairs().any(|p| p == (KEY_TAG | 0x77, 0x1234)));
        assert_eq!(a.pairs().count(), RECENT * 2);
    }

    /// The window's page bytes are the `(Tick, u64)` pairs it held before, the derived ids
    /// oldest tick first and the key tags last: no format change.
    #[test]
    fn the_window_keeps_its_page_bytes() {
        use crate::persist::Persist;
        use rtsim_kernel::codec::{Reader, Writer};
        let mut ops = IndustryOps::default();
        let newer = ops.derive(5, Tick(101), &channel(4));
        ops.remember(KEY_TAG | 0x77, 0x1234);
        // A stale record of the same step.
        let older = ops.derive(5, Tick(100), &channel(3));
        let before: std::collections::VecDeque<(Tick, u64)> = [
            (Tick(100), older.0 & LOW_BITS),
            (Tick(101), newer.0 & LOW_BITS),
            (Tick(KEY_TAG | 0x77), 0x1234),
        ]
        .into();
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

    /// The slice R re-check's step 3b: one step of 100,000 derivations (a jni replay since the
    /// last durable pages, the switch-on catch-up, `glass_replay_offline`) kept every id on
    /// 2bd1d828, and each derivation scanned them all. However long the step, the window holds at
    /// most [`CAP`] and [`RECENT`] derived ids and its key tags, and the page saved at the step's
    /// end holds the trimmed window: the last [`RECENT`] and the ids at the step's newest tick.
    #[test]
    fn a_step_of_100000_derivations_keeps_a_bounded_window_and_pages_the_trimmed_one() {
        use crate::persist::Persist;
        use rtsim_kernel::codec::{Reader, Writer};
        // The window as a page holds it: (tick or key tag, low) pairs.
        fn paged(ops: &IndustryOps) -> Vec<(u64, u64)> {
            let mut w = Writer::new();
            ops.put(&mut w);
            let bytes = w.into_bytes();
            let pairs: std::collections::VecDeque<(u64, u64)> =
                Persist::get(&mut Reader::new(&bytes)).expect("decodes");
            pairs.into_iter().collect()
        }
        let derived = |p: &[(u64, u64)]| p.iter().filter(|&&(by, _)| by & KEY_TAG == 0).count();
        let mut a = IndustryOps::default();
        for k in 0..3 {
            a.remember(KEY_TAG | k, 0x9000 + k);
        }
        let mut busy = std::collections::HashSet::new();
        for n in 0..100_000u64 {
            // A catch-up: ten decisions a tick, a stale record every 1,000, and a busy last tick
            // of 1,000 (twenty identical decisions of each of fifty commands).
            let at = if n >= 99_000 {
                20_000
            } else if n % 1_000 == 999 {
                500
            } else {
                1_000 + n / 10
            };
            let op = a.derive(5, Tick(at), &channel((n % 50) as i64));
            if at == 20_000 {
                assert!(
                    busy.insert(op),
                    "derivation {n} took an op of its tick again"
                );
            }
            if n % 5_000 == 4_999 {
                let p = paged(&a);
                assert!(
                    derived(&p) <= CAP + RECENT,
                    "after {} derivations the window holds {} derived ids",
                    n + 1,
                    derived(&p)
                );
                assert_eq!(p.len() - derived(&p), 3, "the key tags stay");
            }
        }
        a.end_step();
        let p = paged(&a);
        assert_eq!(
            derived(&p),
            1_000,
            "the busy last tick's ids, nothing older"
        );
        assert!(p.iter().all(|&(by, _)| by & KEY_TAG != 0 || by == 20_000));
        assert_eq!(p.len(), 1_003);
        // The page derives on as the window would: the busy tick's next identical decision
        // takes an op of its own.
        let mut w = Writer::new();
        a.put(&mut w);
        let bytes = w.into_bytes();
        let mut back = IndustryOps::get(&mut Reader::new(&bytes)).expect("decodes");
        assert_eq!(back, a);
        let next = back.derive(5, Tick(20_000), &channel(0));
        assert!(!busy.contains(&next));
        assert_eq!(next, a.derive(5, Tick(20_000), &channel(0)));
    }

    /// One step of many derivations, timed (the re-check's bench: 17.2 s for 100,000 on
    /// 2bd1d828). Not a test: `cargo test -p rtsim-industry --release --lib -- --ignored
    /// one_step_of_derivations_timed --nocapture`.
    #[test]
    #[ignore = "a timing bench"]
    fn one_step_of_derivations_timed() {
        for n in [1_000u64, 10_000, 30_000, 100_000] {
            let mut a = IndustryOps::default();
            let t = std::time::Instant::now();
            for i in 0..n {
                a.derive(5, Tick(1_000 + i / 10), &channel((i % 50) as i64));
            }
            a.end_step();
            eprintln!("{n} derivations in one step: {:?}", t.elapsed());
        }
    }
}
