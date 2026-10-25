// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons' commands to the plugin, gated while the session's replay runs
//! (docs/replay-gate.md §4.5, RP-3, RP-4).
//!
//! The replay's records are Java's answers to decisions of the last session: what the engine
//! decides while it applies them is decided again from a world Java's new session no longer has.
//! So every command goes in through [`Outbox::push`], which asks [`gate`]: one arm per [`Cmd`]
//! kind and no `_` arm, so that a new kind does not compile until it has both verdicts. The
//! commands are a private `Vec`: a push around the gate does not compile either.
//!
//! - **Queued** (applied by [`Outbox::begin_replay`] to what is still queued: restored from the
//!   pages, or left from before): keep or drop.
//! - **Replaying** (applied by `push` between `begin_replay` and `replay_done`): send (queued as
//!   usual), drop (counted), or never. A "never" kind embodies; the engine's own early returns
//!   (§6) make it impossible by construction, so one reaching the gate is a bug: counted, its
//!   kind kept for the log, not queued, and a `debug_assert!` (no release panic: the replay end
//!   makes every dragon abstract, which heals the skip).
//! - **Deferred**: the engine's early returns from a decision that embodies while replaying
//!   (§6) are counted here too ([`Outbox::defer`]), so that the replay end's log shows them.
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

/// A kind's two verdicts (docs/replay-gate.md §4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) queued: Queued,
    pub(crate) replaying: Replaying,
}

const fn verdicts(queued: Queued, replaying: Replaying) -> Gate {
    Gate { queued, replaying }
}

/// Each command kind's verdicts, the table of docs/replay-gate.md §4.5.
pub(crate) fn gate(c: &Cmd) -> Gate {
    use Queued as Q;
    use Replaying as R;
    match c {
        // Embodies.
        Cmd::Materialize(_) => verdicts(Q::Drop, R::Never),
        // A body's orders: no body of this session yet (Java answers a DEPART for none with
        // NOT_EMBODIED, which the engine reads as a no-op: §5.8).
        Cmd::Steer(_) => verdicts(Q::Drop, R::Drop),
        Cmd::Depart(_) => verdicts(Q::Drop, R::Drop),
        // Decided from a replayed UNTAGGED; the engine keeps the adoption and Java reports the
        // entity untagged again this session, which adopts it at a new epoch (RP-13).
        Cmd::Adopt(_) => verdicts(Q::Drop, R::Drop),
        // The same: the entity reports again and the live record decides.
        Cmd::Discard(_) => verdicts(Q::Keep, R::Drop),
        // Java places once per field (LairPlacer's saved `fields`).
        Cmd::PlaceLair(_) => verdicts(Q::Keep, R::Send),
        // A query, answered with OIL_CHUNKS (idempotent: a chunk sent twice is one chunk).
        Cmd::OilSurvey(_) => verdicts(Q::Keep, R::Send),
        // Idempotent (boundary.rs).
        Cmd::PlaceNest(_) => verdicts(Q::Keep, R::Send),
        // Java adds `owed += count`, so a renewal decided again is owed twice: dropped until the
        // renewal carries its serial (slice N, §7.1, RP-14). Loses a renewal Java's save lost.
        Cmd::NestRenew(_) => verdicts(Q::Keep, R::Drop),
        // A watched district's fight, re-derived: the raid's effect on the district is the
        // engine's and stands (§7.2, RP-15).
        Cmd::NestSortie(_) => verdicts(Q::Drop, R::Drop),
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

/// The dragons' commands not yet taken by the host, behind the gate.
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
    use crate::boundary;
    use crate::species::Species;

    /// One command of each kind.
    fn cmds() -> Vec<Cmd> {
        vec![
            Cmd::Materialize(boundary::Materialize {
                agent: 1,
                epoch: 1,
                flags: 0,
                species: Species::Fire,
                dimension: 0,
                age_ticks: 0,
                pos: (0.0, 0.0, 0.0),
                hunger: 0,
                gender: crate::species::Gender::Female,
                variant: 0,
                health: 0.0,
                home: (0, 0, 0),
                target: (0.0, 0.0, 0.0),
                target_player: (0, 0),
                oil: 0,
            }),
            Cmd::Steer(boundary::Steer {
                agent: 1,
                epoch: 1,
                flags: 0,
                target: (0.0, 0.0, 0.0),
            }),
            Cmd::Depart(boundary::Depart {
                agent: 1,
                epoch: 1,
                reason: 1,
                timeout_ticks: 1,
                target: (0.0, 0.0, 0.0),
            }),
            Cmd::Adopt(boundary::Adopt {
                entity: (1, 2),
                agent: 1,
                epoch: 1,
                flags: 0,
                home: (0, 0, 0),
            }),
            Cmd::Discard(boundary::Discard {
                entity: (1, 2),
                reason: 1,
            }),
            Cmd::PlaceLair(boundary::PlaceLair {
                field: 1,
                dimension: 0,
                species: Species::Fire,
                kind: crate::dragon::LairKind::Roost,
                x: 0,
                z: 0,
                radius: 1,
            }),
            Cmd::OilSurvey(boundary::OilSurvey {
                dimension: 0,
                x: 0,
                z: 0,
                radius: 1,
            }),
            Cmd::PlaceNest(boundary::PlaceNest {
                nest: 1,
                dimension: 0,
                cx: 0,
                cz: 0,
                depth: 1,
            }),
            Cmd::NestRenew(boundary::NestRenew {
                nest: 1,
                dimension: 0,
                count: 1,
                stage: 1,
            }),
            Cmd::NestSortie(boundary::NestSortie {
                nest: 1,
                dimension: 0,
                count: 1,
                x: 0,
                z: 0,
                timeout_ticks: 1,
            }),
        ]
    }

    /// docs/replay-gate.md §4.5, one row per kind: (kind, queued, while replaying).
    const TABLE: [(u16, Queued, Replaying); 10] = [
        (boundary::DRAGON_MATERIALIZE, Queued::Drop, Replaying::Never),
        (boundary::DRAGON_STEER, Queued::Drop, Replaying::Drop),
        (boundary::DRAGON_DEPART, Queued::Drop, Replaying::Drop),
        (boundary::DRAGON_ADOPT, Queued::Drop, Replaying::Drop),
        (boundary::DRAGON_DISCARD, Queued::Keep, Replaying::Drop),
        (boundary::PLACE_LAIR, Queued::Keep, Replaying::Send),
        (boundary::OIL_SURVEY, Queued::Keep, Replaying::Send),
        (boundary::PLACE_NEST, Queued::Keep, Replaying::Send),
        (boundary::NEST_RENEW, Queued::Keep, Replaying::Drop),
        (boundary::NEST_SORTIE, Queued::Drop, Replaying::Drop),
    ];

    #[test]
    fn every_command_kind_has_its_replay_verdicts() {
        let cmds = cmds();
        let kinds: std::collections::BTreeSet<u16> = cmds.iter().map(Cmd::kind).collect();
        assert_eq!(kinds.len(), TABLE.len(), "one value of each kind");
        for c in &cmds {
            let row = TABLE.iter().find(|r| r.0 == c.kind());
            let Some(&(_, queued, replaying)) = row else {
                panic!("{} has no row in §4.5", c.name());
            };
            assert_eq!(gate(c), verdicts(queued, replaying), "{}", c.name());
        }
        // The outbox applies them: queued at the start, then pushed while replaying.
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
        assert_eq!(
            kept,
            vec![
                boundary::DRAGON_DISCARD,
                boundary::PLACE_LAIR,
                boundary::OIL_SURVEY,
                boundary::PLACE_NEST,
                boundary::NEST_RENEW,
            ]
        );
        for c in cmds
            .iter()
            .filter(|c| gate(c).replaying != Replaying::Never)
        {
            o.push(c.clone());
        }
        let sent: Vec<u16> = o.take().iter().map(Cmd::kind).collect();
        assert_eq!(
            sent,
            vec![
                boundary::PLACE_LAIR,
                boundary::OIL_SURVEY,
                boundary::PLACE_NEST
            ]
        );
        let want = GateCounts {
            dropped: 6,
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
    /// release it is counted, its kind kept, and nothing is queued. `cargo test --release` runs
    /// the release half (docs/replay-gate.md §4.2).
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "while replaying"))]
    fn a_never_kind_decided_while_replaying_is_counted_and_not_queued() {
        let m = cmds()
            .into_iter()
            .find(|c| matches!(c, Cmd::Materialize(_)))
            .unwrap();
        let mut o = Outbox::default();
        o.begin_replay();
        o.push(m);
        let want = GateCounts {
            never: 1,
            last_never: Some((boundary::DRAGON_MATERIALIZE, "DRAGON_MATERIALIZE")),
            ..GateCounts::default()
        };
        assert_eq!(o.counts(), want);
        assert_eq!(o.len(), 0, "not queued");
    }
}
