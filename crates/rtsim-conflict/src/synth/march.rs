// SPDX-License-Identifier: AGPL-3.0-or-later
//! The march audit (`rtsim-cli sim mowzie` run 13, MZB-498): Java's map of marches as the engine's
//! records build it, held against the engine's own positions, step by step.
//!
//! A run that wants it turns it [`on`]; the places that step an engine then call [`begin`], take
//! the commands ([`took`]) and call [`end`], or call [`advance`], which does all three in steps
//! of [`STEP`] ticks. Off, [`advance`] is `run_until` and `take_commands` and nothing else, so
//! the runs that carry the hooks are what they were. The state is the thread's: a sim run is one
//! thread, and the tests that run in parallel each have their own.
//!
//! What a step checks (MOWZIE.md §4.16, MZB-454):
//!
//! - A party unchanged across the step (its `plan` and `march_rev`, no mark pending) has its
//!   record's `chart_pos(t)` equal the engine's `party_chart_pos` at every tick of the step. The
//!   record is the latest one Java holds, taken before the step; the engine has walked its
//!   segments meanwhile, which is the point: the shared position rule reads the same.
//! - A party that changed (a new party, a record built, a mark made) is compared at the step's
//!   end, and its record must say what it is then: its phase, its units' sum, the end of its
//!   route.
//! - A change of route, phase, count or chief flag in the step has a record taken in it.
//! - A party that left the engine has no record left in Java's map.
//! - While a replay runs no record is built (MZB-484), so no party is compared then.

use std::cell::RefCell;
use std::collections::BTreeMap;

use rtsim_ecology::infra::Infrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_world::World;

use crate::boundary::{self, BandMarch, Cmd};
use crate::engine::{Conflict, Party, Phase};

/// The audit's step in ticks: how far an engine is run between two looks.
pub const STEP: u64 = 20;

/// The most failures kept as text; the rest are counted.
const KEPT: usize = 12;

/// What an audited run found.
#[derive(Clone, Debug, Default)]
pub struct Totals {
    /// Steps looked at.
    pub steps: u64,
    /// Party-steps unchanged across their step, and the ticks compared in them.
    pub still: u64,
    pub ticks: u64,
    /// Party-steps that changed, compared at their end.
    pub changed: u64,
    /// Party-steps skipped: a mark pending while a replay runs.
    pub pending: u64,
    /// Party-steps of a party with no one alive and no chief (its record ended; the engine keeps
    /// the shell until it is reaped).
    pub shells: u64,
    /// The records taken, and the ENDED ones among them.
    pub records: u64,
    pub ended: u64,
    /// Changes of route, phase, count or chief flag, each of which had a record.
    pub changes: u64,
    /// Changes of route, phase, count or chief flag that no record followed (a failure each).
    pub unrecorded: u64,
    /// Failures: the first few as text, and the count.
    pub failures: Vec<String>,
    pub failed: u64,
}

impl Totals {
    fn fail(&mut self, tick: Tick, what: String) {
        self.failed += 1;
        if self.failures.len() < KEPT {
            self.failures.push(format!("tick {}: {what}", tick.0));
        }
    }

    /// Adds another run's totals.
    pub fn add(&mut self, o: &Totals) {
        self.steps += o.steps;
        self.still += o.still;
        self.ticks += o.ticks;
        self.changed += o.changed;
        self.pending += o.pending;
        self.shells += o.shells;
        self.records += o.records;
        self.ended += o.ended;
        self.changes += o.changes;
        self.unrecorded += o.unrecorded;
        self.failed += o.failed;
        for f in &o.failures {
            if self.failures.len() < KEPT {
                self.failures.push(f.clone());
            }
        }
    }
}

/// A party as a step began.
struct Snap {
    plan: u32,
    rev: u32,
    due: bool,
    phase: Phase,
    route: Vec<(i32, i32)>,
    alive: u32,
    chief: bool,
}

impl Snap {
    fn of(x: &Party) -> Snap {
        Snap {
            plan: x.plan,
            rev: x.march_rev,
            due: x.march_due,
            phase: x.phase,
            route: x.route.clone(),
            alive: x.alive(),
            chief: x.chief,
        }
    }

    /// What a record says, if it differs from `o`.
    fn differs(&self, o: &Snap) -> bool {
        self.phase != o.phase
            || self.route != o.route
            || self.alive != o.alive
            || self.chief != o.chief
    }
}

#[derive(Default)]
struct State {
    /// The latest record of each party that has not ended, as Java's `MarchPath` keeps it.
    held: BTreeMap<u64, BandMarch>,
    before: BTreeMap<u64, Snap>,
    /// The records taken since the step began, by agent.
    taken: BTreeMap<u64, u32>,
    /// The tick the last step ended at: an engine that begins before it is a new one (a sim run
    /// builds many), and Java's map starts empty for it.
    last: Tick,
    totals: Totals,
}

thread_local! {
    static AUDIT: RefCell<Option<State>> = const { RefCell::new(None) };
}

impl State {
    /// An engine whose clock is behind the last step's is a new one (a sim run builds many): Java's
    /// map starts empty for it.
    fn sync(&mut self, c: &Conflict) {
        if c.now() < self.last {
            self.held.clear();
            self.before.clear();
            self.taken.clear();
        }
        self.last = c.now();
    }
}

fn with(f: impl FnOnce(&mut State)) {
    AUDIT.with(|a| {
        if let Some(s) = a.borrow_mut().as_mut() {
            f(s);
        }
    });
}

/// Starts auditing on this thread, from nothing.
pub fn on() {
    AUDIT.with(|a| *a.borrow_mut() = Some(State::default()));
}

/// Stops auditing and returns what it found (empty if it was not on).
pub fn off() -> Totals {
    AUDIT.with(|a| a.borrow_mut().take().map(|s| s.totals).unwrap_or_default())
}

pub fn active() -> bool {
    AUDIT.with(|a| a.borrow().is_some())
}

/// A session starts: Java's map of marches is empty (MZB-451).
pub fn session() {
    with(|s| {
        s.held.clear();
        s.before.clear();
        s.taken.clear();
    });
}

/// A step begins: the parties as they are.
pub fn begin(c: &Conflict) {
    with(|s| {
        s.sync(c);
        s.taken.clear();
        s.before = c
            .parties
            .iter()
            .map(|(p, x)| (Party::agent(p), Snap::of(x)))
            .collect();
    });
}

/// The engine's commands, taken: what Java would hold of them.
pub fn took(c: &Conflict, cmds: &[Cmd]) {
    with(|s| {
        s.sync(c);
        for cmd in cmds {
            let Cmd::March(m) = cmd else { continue };
            s.totals.records += 1;
            *s.taken.entry(m.agent).or_default() += 1;
            if m.flags & boundary::MARCH_ENDED != 0 {
                s.totals.ended += 1;
                s.held.remove(&m.agent);
            } else if let Some(old) = s.held.get(&m.agent) {
                if old.revision < m.revision {
                    s.held.insert(m.agent, m.clone());
                } else {
                    let what = format!(
                        "party {:x}: a record of revision {} after one of {}",
                        m.agent, m.revision, old.revision
                    );
                    s.totals.fail(Tick::ZERO, what);
                }
            } else {
                s.held.insert(m.agent, m.clone());
            }
        }
    });
}

fn phase_code(p: Phase) -> u8 {
    match p {
        Phase::Mustering => boundary::MARCH_MUSTERING,
        Phase::Out => boundary::MARCH_OUT,
        Phase::Waiting => boundary::MARCH_WAITING,
        Phase::Fighting => boundary::MARCH_FIGHTING,
        Phase::Home => boundary::MARCH_HOME,
    }
}

/// A step ends: `from` to `to`, with the commands taken in it already given to [`took`].
pub fn end(c: &Conflict, from: Tick, to: Tick) {
    with(|s| {
        s.totals.steps += 1;
        s.last = to;
        let replaying = c.replaying();
        let before = std::mem::take(&mut s.before);
        for (p, x) in c.parties.iter() {
            let agent = Party::agent(p);
            let now = Snap::of(x);
            let held = s.held.get(&agent).cloned();
            if x.alive() == 0 && !x.chief {
                // Every unit dead: its record said ENDED and Java dropped it, though the engine
                // keeps the shell until it is reaped.
                if held.is_some() {
                    s.totals.fail(
                        to,
                        format!("party {agent:x} has no one alive and Java holds it"),
                    );
                }
                s.totals.shells += 1;
                continue;
            }
            let still = before
                .get(&agent)
                .is_some_and(|b| !b.due && b.plan == now.plan && b.rev == now.rev && !now.due);
            if still {
                let Some(m) = held else {
                    s.totals.fail(
                        to,
                        format!("party {agent:x} is unchanged and has no record"),
                    );
                    continue;
                };
                s.totals.still += 1;
                for t in (from.0 + 1)..=to.0 {
                    let (rec, eng) = (m.chart_pos(t as i64), c.party_chart_pos(p, Tick(t)));
                    s.totals.ticks += 1;
                    if rec != Some(eng) {
                        s.totals.fail(
                            Tick(t),
                            format!(
                                "party {agent:x} {:?}: the record puts it at {rec:?}, the engine \
                                 at {eng:?} (revision {}, plan {})",
                                x.phase, m.revision, now.plan
                            ),
                        );
                        break;
                    }
                }
                continue;
            }
            if replaying || now.due {
                s.totals.pending += 1;
                continue;
            }
            s.totals.changed += 1;
            let Some(m) = held else {
                s.totals
                    .fail(to, format!("party {agent:x} changed and has no record"));
                continue;
            };
            let eng = c.party_chart_pos(p, to);
            if m.chart_pos(to.0 as i64) != Some(eng) {
                s.totals.fail(
                    to,
                    format!(
                        "party {agent:x} {:?}: the record puts it at {:?}, the engine at {eng:?}",
                        x.phase,
                        m.chart_pos(to.0 as i64)
                    ),
                );
            }
            let units: u32 = m.units.iter().map(|u| u.1).sum();
            let tail = m.waypoints.last().map(|w| (w.0, w.1));
            if m.phase != phase_code(x.phase)
                || units != x.alive()
                || tail != x.route.last().copied()
            {
                s.totals.fail(
                    to,
                    format!(
                        "party {agent:x} {:?} of {}: its record says phase {}, {units} units, \
                         ends at {tail:?}",
                        x.phase,
                        x.alive(),
                        m.phase
                    ),
                );
            }
            if before.get(&agent).is_some_and(|b| b.differs(&now)) {
                s.totals.changes += 1;
                if s.taken.get(&agent).is_none_or(|n| *n == 0) {
                    s.totals.unrecorded += 1;
                    s.totals.fail(
                        to,
                        format!("party {agent:x} changed its route, phase or count with no record"),
                    );
                }
            }
        }
        // A party that left the engine leaves no record in Java's map (its ENDED went with it).
        if !replaying {
            let gone: Vec<u64> = s
                .held
                .keys()
                .copied()
                .filter(|a| Party::of_agent(*a).is_none_or(|p| !c.parties.contains(p)))
                .collect();
            for a in gone {
                s.totals.fail(
                    to,
                    format!("party {a:x} is gone and Java's map still holds it"),
                );
                s.held.remove(&a);
            }
        }
    });
}

/// The engine to `to` and the commands it decided: in steps of [`STEP`] ticks, each audited, while
/// the audit is on; plainly otherwise.
pub fn advance(
    c: &mut Conflict,
    w: &mut World,
    infra: &mut dyn Infrastructure,
    to: Tick,
) -> Vec<Cmd> {
    if !active() {
        c.run_until(w, infra, to);
        return c.take_commands();
    }
    let mut out = Vec::new();
    loop {
        let from = c.now();
        let next = Tick(to.0.min(from.0 + STEP).max(from.0));
        begin(c);
        c.run_until(w, infra, next);
        let cmds = c.take_commands();
        took(c, &cmds);
        end(c, from, next);
        out.extend(cmds);
        if c.now() >= to {
            break;
        }
    }
    out
}
