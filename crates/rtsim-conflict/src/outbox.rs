// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict's commands to the plugin, gated while the session's replay runs
//! (docs/replay-gate.md §4, RP-3, RP-4).
//!
//! The replay's records are Java's answers to decisions of the last session: what the engine
//! decides while it applies them is decided again from a world Java's new session no longer has.
//! So every command goes in through [`Outbox::push`], which asks [`gate`]: one arm per [`Cmd`]
//! kind and no `_` arm, so that a new kind does not compile until it has both verdicts. The
//! commands are a private `Vec`: a push around the gate does not compile either.
//!
//! - **Queued** (applied by [`Outbox::begin_replay`] to what is still queued: restored from the
//!   pages, or left from before): keep or drop. Exactly the filter the conflict's session start
//!   had (the agent kinds, TERRITORY, FACTION_NOTICE and CAMP_RECONCILE dropped).
//! - **Replaying** (applied by `push` between `begin_replay` and `replay_done`): send (queued as
//!   usual), drop (counted), or never. A "never" kind embodies (or answers a body of this
//!   session); the engine's own early returns (§6) make it impossible by construction, so one
//!   reaching the gate is a bug: counted, its kind kept for the log, not queued, and a
//!   `debug_assert!` (no release panic: the replay end makes every party abstract, which heals
//!   the skip).
//!
//! Not saved: `replaying` and the counts are a session's (the pages hold the commands alone).

use crate::boundary::Cmd;

/// What [`Outbox::begin_replay`] does to a command still queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Queued {
    Keep,
    Drop,
}

/// What [`Outbox::push`] does while replaying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Replaying {
    Send,
    Drop,
    Never,
}

/// A kind's two verdicts (docs/replay-gate.md §4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) queued: Queued,
    pub(crate) replaying: Replaying,
}

const fn verdicts(queued: Queued, replaying: Replaying) -> Gate {
    Gate { queued, replaying }
}

/// Each command kind's verdicts, the table of docs/replay-gate.md §4.4.
pub(crate) fn gate(c: &Cmd) -> Gate {
    use Queued as Q;
    use Replaying as R;
    match c {
        // Latest wins; the replay's start pushes every faction's afresh.
        Cmd::Territory(_) => verdicts(Q::Drop, R::Send),
        // The kept notices go at the replay end (MZB-31, RP-12).
        Cmd::Notice(_) => verdicts(Q::Drop, R::Drop),
        // Embody.
        Cmd::Envoy(_) => verdicts(Q::Drop, R::Never),
        Cmd::Materialize(_) => verdicts(Q::Drop, R::Never),
        // A body's orders and lurk: no body of this session yet.
        Cmd::Steer(_) => verdicts(Q::Drop, R::Drop),
        Cmd::Recall(_) => verdicts(Q::Drop, R::Drop),
        Cmd::AmbushSpot(_) => verdicts(Q::Drop, R::Drop),
        // News only; the envoy that would name it is gone (RP-12).
        Cmd::Settled(_) => verdicts(Q::Keep, R::Drop),
        // Answers a CAMP_CENSUS of this session with spawn, adopt and take lines (MOWZIE.md
        // §4.9).
        Cmd::Reconcile(_) => verdicts(Q::Drop, R::Never),
        // A roar (RP-12).
        Cmd::TrainRobbed(_) => verdicts(Q::Keep, R::Drop),
        // Goes only ahead of a stack, a spawn line or a hand-back, none made while replaying.
        Cmd::ChiefBody(_) => verdicts(Q::Keep, R::Never),
        // Latest wins per agent (MOWZIE.md §4.16): the abstract party's march.
        Cmd::March(_) => verdicts(Q::Drop, R::Send),
    }
}

/// The conflict's commands not yet taken by the host, behind the gate.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outbox {
    cmds: Vec<Cmd>,
    replaying: bool,
    /// Commands the gate dropped while replaying (verdict drop).
    dropped: u64,
    /// Commands of a "never" kind decided while replaying (a bug, counted and not queued).
    never: u64,
    /// The last such command's kind and name (`Cmd::kind`, `Cmd::name`), for the host's log.
    last_never: Option<(u16, &'static str)>,
}

impl Outbox {
    /// The only way in: the gate's verdict while replaying, else queued.
    pub(crate) fn push(&mut self, cmd: Cmd) {
        if self.replaying {
            match gate(&cmd).replaying {
                Replaying::Send => {}
                Replaying::Drop => {
                    self.dropped += 1;
                    return;
                }
                Replaying::Never => {
                    self.never += 1;
                    self.last_never = Some((cmd.kind(), cmd.name()));
                    debug_assert!(
                        false,
                        "{} ({:#06x}) decided while replaying",
                        cmd.name(),
                        cmd.kind()
                    );
                    return;
                }
            }
        }
        self.cmds.push(cmd);
    }

    /// A command the pages held, at load and before any session: not gated (`begin_replay`
    /// applies the queued verdicts to it).
    pub(crate) fn restore(&mut self, cmd: Cmd) {
        self.cmds.push(cmd);
    }

    /// The session's replay begins: what is still queued takes its queued verdict, and every
    /// push is gated until [`Outbox::replay_done`].
    pub(crate) fn begin_replay(&mut self) {
        self.cmds.retain(|c| gate(c).queued == Queued::Keep);
        self.replaying = true;
    }

    /// The session's replay is over: pushes are queued as usual.
    pub(crate) fn replay_done(&mut self) {
        self.replaying = false;
    }

    pub(crate) fn replaying(&self) -> bool {
        self.replaying
    }

    pub(crate) fn take(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.cmds)
    }

    /// Removal only: what is kept stays in its order.
    pub(crate) fn retain(&mut self, f: impl FnMut(&Cmd) -> bool) {
        self.cmds.retain(f);
    }

    pub(crate) fn len(&self) -> usize {
        self.cmds.len()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Cmd> {
        self.cmds.iter()
    }

    /// (dropped, never, the last never kind): this engine's, since it was built or loaded.
    pub(crate) fn counts(&self) -> (u64, u64, Option<(u16, &'static str)>) {
        (self.dropped, self.never, self.last_never)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary;

    /// docs/replay-gate.md §4.4, one row per kind: (kind, queued, while replaying).
    const TABLE: [(u16, Queued, Replaying); 12] = [
        (boundary::TERRITORY, Queued::Drop, Replaying::Send),
        (boundary::FACTION_NOTICE, Queued::Drop, Replaying::Drop),
        (boundary::ENVOY_MATERIALIZE, Queued::Drop, Replaying::Never),
        (boundary::BAND_MATERIALIZE, Queued::Drop, Replaying::Never),
        (boundary::BAND_STEER, Queued::Drop, Replaying::Drop),
        (boundary::BAND_RECALL, Queued::Drop, Replaying::Drop),
        (boundary::DEMAND_SETTLED, Queued::Keep, Replaying::Drop),
        (boundary::AMBUSH_SPOT, Queued::Drop, Replaying::Drop),
        (boundary::CAMP_RECONCILE, Queued::Drop, Replaying::Never),
        (boundary::TRAIN_ROBBED, Queued::Keep, Replaying::Drop),
        (boundary::CHIEF_BODY, Queued::Keep, Replaying::Never),
        (boundary::BAND_MARCH, Queued::Drop, Replaying::Send),
    ];

    #[test]
    fn every_command_kind_has_its_replay_verdicts() {
        let cmds = boundary::tests::cmds();
        let kinds: std::collections::BTreeSet<u16> = cmds.iter().map(Cmd::kind).collect();
        assert_eq!(kinds.len(), TABLE.len(), "one value of each kind");
        for c in &cmds {
            let row = TABLE.iter().find(|r| r.0 == c.kind());
            let Some(&(_, queued, replaying)) = row else {
                panic!("{} has no row in §4.4", c.name());
            };
            assert_eq!(gate(c), verdicts(queued, replaying), "{}", c.name());
        }
        // The queued column is the conflict's old session-start filter exactly.
        for c in &cmds {
            let old = c.agent().is_none()
                && !matches!(c, Cmd::Territory(_) | Cmd::Notice(_) | Cmd::Reconcile(_));
            assert_eq!(gate(c).queued == Queued::Keep, old, "{}", c.name());
        }
        // And the outbox applies them: queued at the start, then pushed while replaying.
        let mut o = Outbox::default();
        for c in &cmds {
            o.restore(c.clone());
        }
        o.begin_replay();
        let kept: Vec<u16> = o.take().iter().map(Cmd::kind).collect();
        let want: Vec<u16> = cmds
            .iter()
            .filter(|c| gate(c).queued == Queued::Keep)
            .map(Cmd::kind)
            .collect();
        assert_eq!(kept, want);
        for c in cmds
            .iter()
            .filter(|c| gate(c).replaying != Replaying::Never)
        {
            o.push(c.clone());
        }
        let sent: Vec<u16> = o.take().iter().map(Cmd::kind).collect();
        assert_eq!(sent, vec![boundary::TERRITORY, boundary::BAND_MARCH]);
        assert_eq!(o.counts(), (6, 0, None), "the drops are counted");
        o.replay_done();
        for c in &cmds {
            o.push(c.clone());
        }
        assert_eq!(o.len(), cmds.len(), "after the replay every kind is queued");
    }

    /// A "never" kind decided while replaying is a bug (RP-4): in a debug build it panics; in
    /// release it is counted, its kind kept, and nothing is queued.
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "while replaying"))]
    fn a_never_kind_decided_while_replaying_is_counted_and_not_queued() {
        let cmds = boundary::tests::cmds();
        let m = cmds
            .iter()
            .find(|c| matches!(c, Cmd::Materialize(_)))
            .unwrap()
            .clone();
        let mut o = Outbox::default();
        o.begin_replay();
        o.push(m);
        assert_eq!(
            o.counts(),
            (0, 1, Some((boundary::BAND_MATERIALIZE, "BAND_MATERIALIZE")))
        );
        assert_eq!(o.len(), 0, "not queued");
    }
}
