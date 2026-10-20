// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine's side of the journal (A9, PLAN.md §4.9): observations arrive numbered, pages
//! record the number they include, and a replay applies only what the pages lack. Cross-store
//! transfers carry operation ids, so the same transfer seen twice applies once.
//!
//! The Java side (the `mic_rtsim:journal` SavedData) is not here; this is what it talks to.
//!
//! # The protocol, engine side
//!
//! 1. Every observation the bridge sends has the next [`ObsSeq`]. Transfers (loot from a caravan,
//!    coins, packages, vault deltas, trades) also carry an [`OpId`].
//! 2. [`Replay::admit`] decides: apply it, or drop it as already applied (by sequence, or by
//!    operation id).
//! 3. A save records [`Replay::applied_through`] in the page store's manifest. Once that save is
//!    durable, the engine acknowledges it and the bridge trims its journal.
//! 4. After a restart, the bridge resends its journal; [`Replay::admit`] drops what the pages
//!    already include and applies the rest, in order.
//!
//! Commands the engine sends to Minecraft go through an [`Outbox`]: numbered, each with an
//! operation id, kept until the bridge reports them applied, and resent after a restart.
//!
//! What to do when a crash leaves the engine's pages *ahead* of the world save is Proposal P29,
//! still open. [`Replay::reconcile`] only detects it.

use std::collections::{HashMap, VecDeque};

use crate::codec::{self, Decode, Encode, Reader, Writer};

/// An observation's number. The bridge counts from 1; 0 means "none yet".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObsSeq(pub u64);

/// A cross-store operation's identity. The top bit says who allocated it, so the engine and the
/// bridge never collide without coordinating: set for ids made on the Java side, clear for the
/// engine's, as the bridge's wire records define it (`Obs.JAVA_OP_BIT` in `neoforge/core`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpId(pub u64);

/// The bridge's `Obs.JAVA_OP_BIT`.
pub const JAVA_OP_BIT: u64 = 1 << 63;

impl OpId {
    /// The engine's `n`th id. `n` must leave the top bit clear.
    pub const fn engine(n: u64) -> OpId {
        assert!(
            n & JAVA_OP_BIT == 0,
            "engine op id counter overflowed into the Java bit"
        );
        OpId(n)
    }

    /// An id made on the Java side, from its low 63 bits. The bridge draws them at random, not
    /// from the observation's sequence: a crash can roll the journal back while a chunk saved on
    /// unload keeps a record from the lost stretch, and the lost sequences are then handed out
    /// again (docs/boundary.md §2.2). `n` must leave the top bit clear.
    pub const fn bridge(n: u64) -> OpId {
        assert!(n & JAVA_OP_BIT == 0, "bridge op id number uses the top bit");
        OpId(JAVA_OP_BIT | n)
    }

    pub const fn is_engine(self) -> bool {
        self.0 & JAVA_OP_BIT == 0
    }
}

/// What to do with an incoming observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// New: apply it. It is now recorded as applied.
    Apply,
    /// Its sequence is at or below what the pages already include.
    AlreadyApplied,
    /// Its operation id was already applied under another sequence. Nothing to do, but the
    /// sequence is consumed.
    DuplicateOp,
    /// Observations are missing between what was applied and this one. Nothing was recorded.
    Gap { expected: ObsSeq, got: ObsSeq },
}

/// How the bridge's journal relates to the engine's pages after a load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconcile {
    /// The bridge's next sequence follows the pages, or the journal covers the difference.
    InSync,
    /// The pages include observations the world save does not know about (P29).
    EngineAhead { pages: ObsSeq, world_next: ObsSeq },
    /// The world save's journal was trimmed past what the pages include: observations are lost.
    JournalTrimmedPastPages {
        pages: ObsSeq,
        journal_first: ObsSeq,
    },
}

/// Replay state: what has been applied. Saved with the pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    applied_through: ObsSeq,
    /// Operation ids applied, with the sequence that applied them, oldest first.
    ops: VecDeque<(ObsSeq, OpId)>,
    op_index: HashMap<OpId, ObsSeq>,
}

impl Replay {
    pub fn new(applied_through: ObsSeq) -> Self {
        Self {
            applied_through,
            ..Self::default()
        }
    }

    pub fn applied_through(&self) -> ObsSeq {
        self.applied_through
    }

    pub fn remembered_ops(&self) -> usize {
        self.ops.len()
    }

    pub fn admit(&mut self, seq: ObsSeq, op: Option<OpId>) -> Verdict {
        if seq <= self.applied_through {
            return Verdict::AlreadyApplied;
        }
        let expected = ObsSeq(self.applied_through.0 + 1);
        if seq != expected {
            return Verdict::Gap { expected, got: seq };
        }
        self.applied_through = seq;
        if let Some(op) = op {
            if self.op_index.contains_key(&op) {
                return Verdict::DuplicateOp;
            }
            self.op_index.insert(op, seq);
            self.ops.push_back((seq, op));
        }
        Verdict::Apply
    }

    /// Forgets operation ids applied at or before `seq`. Call it when no world save that could
    /// resend them remains: the bridge says so once a world save past `seq` is durable.
    pub fn forget_ops_through(&mut self, seq: ObsSeq) {
        while let Some(&(s, op)) = self.ops.front() {
            if s > seq {
                break;
            }
            self.ops.pop_front();
            self.op_index.remove(&op);
        }
    }

    /// Compares the pages with what the bridge reports after loading the world: the first
    /// sequence still in its journal, and the next sequence it will assign.
    pub fn reconcile(&self, journal_first: ObsSeq, world_next: ObsSeq) -> Reconcile {
        if world_next.0 <= self.applied_through.0 {
            return Reconcile::EngineAhead {
                pages: self.applied_through,
                world_next,
            };
        }
        // An empty journal reports journal_first == world_next; a gap above the pages is lost
        // data either way.
        if journal_first.0 > self.applied_through.0 + 1 {
            return Reconcile::JournalTrimmedPastPages {
                pages: self.applied_through,
                journal_first,
            };
        }
        Reconcile::InSync
    }
}

impl Encode for Replay {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.applied_through.0);
        w.var_u64(self.ops.len() as u64);
        for (s, op) in &self.ops {
            w.var_u64(s.0);
            w.u64(op.0);
        }
    }
}

impl Decode for Replay {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let mut out = Replay::new(ObsSeq(r.u64()?));
        let n = r.var_u64()?;
        for _ in 0..n {
            let s = ObsSeq(r.var_u64()?);
            let op = OpId(r.u64()?);
            out.op_index.insert(op, s);
            out.ops.push_back((s, op));
        }
        Ok(out)
    }
}

/// Engine-allocated operation ids. Saved with the pages, so ids never repeat across restarts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpIds {
    next: u64,
}

impl OpIds {
    pub fn allocate(&mut self) -> OpId {
        let id = OpId::engine(self.next);
        self.next += 1;
        id
    }
}

/// Commands to Minecraft, kept until the bridge reports them applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbox<C> {
    next_seq: u64,
    ids: OpIds,
    pending: VecDeque<(u64, OpId, C)>,
}

impl<C> Default for Outbox<C> {
    fn default() -> Self {
        Self {
            next_seq: 1,
            ids: OpIds::default(),
            pending: VecDeque::new(),
        }
    }
}

impl<C> Outbox<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues a command; returns its sequence and operation id.
    pub fn push(&mut self, cmd: C) -> (u64, OpId) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let op = self.ids.allocate();
        self.pending.push_back((seq, op, cmd));
        (seq, op)
    }

    /// The bridge applied every command up to `seq`.
    pub fn ack_through(&mut self, seq: u64) {
        while self.pending.front().is_some_and(|(s, _, _)| *s <= seq) {
            self.pending.pop_front();
        }
    }

    /// Commands after `seq`, to resend after a restart. The bridge drops any whose operation id it
    /// already applied.
    pub fn after(&self, seq: u64) -> impl Iterator<Item = &(u64, OpId, C)> {
        self.pending.iter().filter(move |(s, _, _)| *s > seq)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

impl<C: Encode> Encode for Outbox<C> {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.next_seq);
        w.u64(self.ids.next);
        w.var_u64(self.pending.len() as u64);
        for (s, op, c) in &self.pending {
            w.var_u64(*s);
            w.u64(op.0);
            c.encode(w);
        }
    }
}

impl<C: Decode> Decode for Outbox<C> {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let next_seq = r.u64()?;
        let next = r.u64()?;
        let n = r.var_u64()?;
        let mut pending = VecDeque::new();
        for _ in 0..n {
            let s = r.var_u64()?;
            let op = OpId(r.u64()?);
            pending.push_back((s, op, C::decode(r)?));
        }
        Ok(Outbox {
            next_seq,
            ids: OpIds { next },
            pending,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn op_id_origins_never_collide() {
        assert!(OpId::engine(5).is_engine());
        assert!(!OpId::bridge(5).is_engine());
        assert_ne!(OpId::engine(5), OpId::bridge(5));
        // The bridge's wire value for its op id 5.
        assert_eq!(OpId::bridge(5).0, 0x8000_0000_0000_0005);
    }

    #[test]
    fn admit_orders_and_dedups() {
        let mut r = Replay::new(ObsSeq(0));
        assert_eq!(r.admit(ObsSeq(1), None), Verdict::Apply);
        assert_eq!(r.admit(ObsSeq(1), None), Verdict::AlreadyApplied);
        assert_eq!(
            r.admit(ObsSeq(3), None),
            Verdict::Gap {
                expected: ObsSeq(2),
                got: ObsSeq(3)
            }
        );
        assert_eq!(r.admit(ObsSeq(2), Some(OpId::bridge(9))), Verdict::Apply);
        assert_eq!(
            r.admit(ObsSeq(3), Some(OpId::bridge(9))),
            Verdict::DuplicateOp
        );
        assert_eq!(r.applied_through(), ObsSeq(3));
        r.forget_ops_through(ObsSeq(2));
        assert_eq!(r.remembered_ops(), 0);
        assert_eq!(r.admit(ObsSeq(4), Some(OpId::bridge(9))), Verdict::Apply);
    }

    #[test]
    fn reconcile_cases() {
        let r = Replay::new(ObsSeq(10));
        assert_eq!(r.reconcile(ObsSeq(5), ObsSeq(14)), Reconcile::InSync);
        assert_eq!(r.reconcile(ObsSeq(11), ObsSeq(11)), Reconcile::InSync);
        assert!(matches!(
            r.reconcile(ObsSeq(5), ObsSeq(9)),
            Reconcile::EngineAhead { .. }
        ));
        assert!(matches!(
            r.reconcile(ObsSeq(13), ObsSeq(20)),
            Reconcile::JournalTrimmedPastPages { .. }
        ));
        // An empty journal trimmed past the pages: observations 11 to 13 are gone.
        assert!(matches!(
            r.reconcile(ObsSeq(14), ObsSeq(14)),
            Reconcile::JournalTrimmedPastPages { .. }
        ));
    }

    #[test]
    fn outbox_resends_unacknowledged() {
        let mut o: Outbox<u32> = Outbox::new();
        let (s1, op1) = o.push(10);
        let (s2, _) = o.push(20);
        o.push(30);
        assert_eq!((s1, s2), (1, 2));
        assert!(op1.is_engine());
        o.ack_through(1);
        assert_eq!(o.after(2).map(|(_, _, c)| *c).collect::<Vec<_>>(), vec![30]);
        assert_eq!(o.len(), 2);
    }

    impl Encode for u32 {
        fn encode(&self, w: &mut Writer) {
            w.u32(*self);
        }
    }
    impl Decode for u32 {
        fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
            r.u32()
        }
    }

    #[test]
    fn state_round_trips() {
        let mut r = Replay::new(ObsSeq(3));
        r.admit(ObsSeq(4), Some(OpId::bridge(1)));
        r.admit(ObsSeq(5), Some(OpId::engine(2)));
        assert_eq!(Replay::from_bytes(&r.to_bytes()).unwrap(), r);
        let mut o: Outbox<u32> = Outbox::new();
        o.push(7);
        o.push(8);
        o.ack_through(1);
        assert_eq!(Outbox::<u32>::from_bytes(&o.to_bytes()).unwrap(), o);
    }

    /// A ledger of transfers keyed by operation: the state a replay must reproduce exactly once.
    #[derive(Clone, Debug, Default, PartialEq)]
    struct Vault {
        balance: i64,
        replay: Replay,
    }

    impl Vault {
        fn observe(&mut self, seq: u64, op: u64, amount: i64) {
            if self.replay.admit(ObsSeq(seq), Some(OpId::bridge(op))) == Verdict::Apply {
                self.balance += amount;
            }
        }
    }

    proptest! {
        /// A crash at any point, followed by a replay of the bridge's whole journal, ends in the
        /// state an uninterrupted run reaches. The journal may hold the same transfer twice under
        /// different sequences (the bridge re-observed it after its own rollback); it applies once.
        #[test]
        fn replay_after_a_crash_is_exactly_once(
            transfers in prop::collection::vec((0u64..20, -50i64..50), 1..80),
            crash_at in any::<prop::sample::Index>(),
        ) {
            // The journal: sequence i+1 carries transfer i; repeated op ids are re-observations.
            let journal: Vec<(u64, u64, i64)> = transfers
                .iter()
                .enumerate()
                .map(|(i, &(op, amt))| (i as u64 + 1, op, amt))
                .collect();
            let amount_of = |op: u64| journal.iter().find(|j| j.1 == op).unwrap().2;

            let mut clean = Vault::default();
            for &(s, op, amt) in &journal {
                clean.observe(s, op, amt);
            }

            // Run up to the crash, save, then restart from the save and replay the whole journal.
            let cut = crash_at.index(journal.len());
            let mut before = Vault::default();
            for &(s, op, amt) in &journal[..cut] {
                before.observe(s, op, amt);
            }
            let saved = Replay::from_bytes(&before.replay.to_bytes()).unwrap();
            let mut after = Vault { balance: before.balance, replay: saved };
            for &(s, op, amt) in &journal {
                after.observe(s, op, amt);
            }
            prop_assert_eq!(after.balance, clean.balance);
            prop_assert_eq!(after.replay.applied_through(), clean.replay.applied_through());
            // Each distinct operation counted once.
            let mut seen = std::collections::BTreeSet::new();
            let expect: i64 = journal.iter().filter(|j| seen.insert(j.1)).map(|j| amount_of(j.1)).sum();
            prop_assert_eq!(clean.balance, expect);
        }
    }
}
