// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herds' commands to the plugin, gated while the session's replay runs
//! (docs/replay-gate.md §4.6, RP-3, RP-4, RP-80).
//!
//! The replay's records are Java's answers to decisions of the last session: what the engine
//! decides while it applies them is decided again from a world Java's new session no longer has.
//! So every command goes in through [`Outbox::push`], which asks [`gate`]: one arm per [`Cmd`]
//! kind and no `_` arm, so that a new kind does not compile until it has both verdicts. The
//! commands are a private `Vec`: a push around the gate does not compile either.
//!
//! - **Queued** (applied by [`Outbox::begin_replay`] to what is still queued: restored from the
//!   pages, or left from before): keep or drop. Every kind is dropped: the herds' session start
//!   took the whole queue and threw it away, because a command of an earlier session names a
//!   group Java no longer has.
//! - **Replaying** (applied by `push` between `begin_replay` and `replay_done`): send (queued as
//!   usual), drop (counted), or never. A "never" kind embodies; the engine's own early return
//!   (`seen`) makes it impossible by construction, so one reaching the gate is a bug: counted,
//!   its kind kept for the log, not queued, and a `debug_assert!` (no release panic: the replay
//!   end makes every group abstract, which heals the skip).
//! - **Deferred**: the early return from a decision that embodies while replaying is counted
//!   here too ([`Outbox::defer`]), so that the replay end's log shows it.
//!
//! Not saved: `replaying` and the counts are a session's (the pages hold the commands alone).

use super::boundary::Cmd;

/// What [`Outbox::begin_replay`] does to a command still queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Queued {
    Keep,
    Drop,
}

/// What [`Outbox::push`] does while replaying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Replaying {
    /// No herd command is sent while replaying; a new kind may be (a query Java answers once).
    #[allow(dead_code)]
    Send,
    Drop,
    Never,
}

/// A kind's two verdicts (docs/replay-gate.md §4.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) queued: Queued,
    pub(crate) replaying: Replaying,
}

const fn verdicts(queued: Queued, replaying: Replaying) -> Gate {
    Gate { queued, replaying }
}

/// Each command kind's verdicts, the table of docs/replay-gate.md §4.6.
pub(crate) fn gate(c: &Cmd) -> Gate {
    use Queued as Q;
    use Replaying as R;
    match c {
        // Embodies.
        Cmd::Materialize(_) => verdicts(Q::Drop, R::Never),
        // A body's order: no body of this session yet.
        Cmd::Steer(_) => verdicts(Q::Drop, R::Drop),
        // The same: the group it would send home is not Java's.
        Cmd::Despawn(_) => verdicts(Q::Drop, R::Drop),
    }
}

/// The replay gate's counts (docs/replay-gate.md §4.3): an engine's, since it was built or
/// loaded; not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateCounts {
    /// Commands the gate dropped while replaying (verdict drop).
    pub dropped: u64,
    /// Commands of a "never" kind decided while replaying (a bug, counted and not queued).
    pub never: u64,
    /// Decisions that embody, returned from early while replaying (§6).
    pub deferred: u64,
    /// The last "never" command's kind and name (`Cmd::kind`, `Cmd::name`), for the host's log.
    pub last_never: Option<(u16, &'static str)>,
}

/// The herds' commands not yet taken by the host, behind the gate.
#[derive(Clone, Debug, Default)]
pub(crate) struct Outbox {
    cmds: Vec<Cmd>,
    replaying: bool,
    counts: GateCounts,
}

impl Outbox {
    /// The only way in: the gate's verdict while replaying, else queued.
    pub(crate) fn push(&mut self, cmd: Cmd) {
        if self.replaying {
            match gate(&cmd).replaying {
                Replaying::Send => {}
                Replaying::Drop => {
                    self.counts.dropped += 1;
                    return;
                }
                Replaying::Never => {
                    self.counts.never += 1;
                    self.counts.last_never = Some((cmd.kind(), cmd.name()));
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

    /// A decision that embodies returned early because the replay runs (§6). Returns whether
    /// it does: the caller's early return is `if self.outbox.defer() { return; }`.
    pub(crate) fn defer(&mut self) -> bool {
        if self.replaying {
            self.counts.deferred += 1;
        }
        self.replaying
    }

    pub(crate) fn take(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.cmds)
    }

    pub(crate) fn len(&self) -> usize {
        self.cmds.len()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Cmd> {
        self.cmds.iter()
    }

    pub(crate) fn counts(&self) -> GateCounts {
        self.counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herds::boundary;

    /// docs/replay-gate.md §4.6, one row per kind: (kind, queued, while replaying).
    const TABLE: [(u16, Queued, Replaying); 3] = [
        (boundary::HERD_MATERIALIZE, Queued::Drop, Replaying::Never),
        (boundary::HERD_STEER, Queued::Drop, Replaying::Drop),
        (boundary::HERD_DESPAWN, Queued::Drop, Replaying::Drop),
    ];

    #[test]
    fn every_command_kind_has_its_replay_verdicts() {
        let cmds = boundary::tests::cmds();
        let kinds: std::collections::BTreeSet<u16> = cmds.iter().map(Cmd::kind).collect();
        assert_eq!(kinds.len(), TABLE.len(), "one value of each kind");
        for c in &cmds {
            let row = TABLE.iter().find(|r| r.0 == c.kind());
            let Some(&(_, queued, replaying)) = row else {
                panic!("{} has no row in §4.6", c.name());
            };
            assert_eq!(gate(c), verdicts(queued, replaying), "{}", c.name());
        }
        // The queued column is the herds' old session-start filter exactly: everything went.
        for c in &cmds {
            assert_eq!(gate(c).queued, Queued::Drop, "{}", c.name());
        }
        // And the outbox applies them: queued at the start, then pushed while replaying.
        let mut o = Outbox::default();
        for c in &cmds {
            o.restore(c.clone());
        }
        assert_eq!(o.len(), cmds.len());
        o.begin_replay();
        assert!(o.replaying());
        assert_eq!(
            o.take().len(),
            0,
            "the session's start drops what the pages queued"
        );
        for c in cmds
            .iter()
            .filter(|c| gate(c).replaying != Replaying::Never)
        {
            o.push(c.clone());
        }
        assert_eq!(o.len(), 0, "a body's orders are dropped while replaying");
        let want = GateCounts {
            dropped: 2,
            ..GateCounts::default()
        };
        assert_eq!(o.counts(), want, "the drops are counted");
        assert!(o.defer(), "an early return while replaying");
        assert_eq!(o.counts().deferred, 1);
        o.replay_done();
        assert!(!o.defer(), "no early return after the replay");
        assert_eq!(o.counts().deferred, 1, "nor a count");
        for c in &cmds {
            o.push(c.clone());
        }
        assert_eq!(o.len(), cmds.len(), "after the replay every kind is queued");
    }

    /// A "never" kind decided while replaying is a bug (RP-4): in a debug build it panics; in
    /// release it is counted, its kind kept, and nothing is queued. (Run in release too:
    /// `cargo test --release -p rtsim-ecology a_never_kind`.)
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
        let want = GateCounts {
            never: 1,
            last_never: Some((boundary::HERD_MATERIALIZE, "HERD_MATERIALIZE")),
            ..GateCounts::default()
        };
        assert_eq!(o.counts(), want);
        assert_eq!(o.len(), 0, "not queued");
    }
}
