// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M10 (§4.16, ledgers 23 and 25): the march record that
//! keeps Java's map of every band (BAND_MARCH, MZB-451 to MZB-455), the walk home in bodies, the
//! re-binding of a watcher that walks off, and a band that walks into its fight.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind};
use rtsim_conflict::engine::{Conflict, Party, Target, body_id};
use rtsim_conflict::synth::{self, FakeConflict};
use rtsim_conflict::{ConflictTunables, Cultures};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::ActorId;
use rtsim_world::World;

/// A fresh op id for a hand-made BAND_CASUALTIES (MZB-589).
fn next_op() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    (1 << 63) | (N.fetch_add(1, Ordering::Relaxed) + 1)
}

/// Goblins that never plunder, so the only parties are the ones a test orders.
fn quiet_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// The goblin camp, the player, the fake plugin and every command the engine sent, with when.
struct Run {
    w: World,
    c: Conflict,
    p: ActorId,
    plugin: FakeConflict,
    cmds: Vec<(Tick, Cmd)>,
}

impl Run {
    fn new(seed: u64) -> Run {
        let (mut w, _) = synth::world(seed);
        let mut c = Conflict::new(
            seed,
            ConflictTunables::default(),
            quiet_goblins(),
            Tick::ZERO,
        );
        synth::define_names(&mut c, &w);
        c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
            .unwrap();
        let p = c.player(&mut w, synth::PLAYER);
        Run {
            w,
            c,
            p,
            plugin: FakeConflict::new(synth::PLAYER),
            cmds: Vec::new(),
        }
    }

    /// A raid band on the refinery, ordered at tick 20 (it starts mustering).
    fn raid(&mut self) -> Handle<Party> {
        self.c
            .order_party(
                &mut self.w,
                synth::CAMP,
                PartyKind::Raid,
                Target::District(synth::REFINERY, synth::REFINERY_POS),
                Some(self.p),
                Tick(20),
            )
            .unwrap()
    }

    /// The engine to `to`, the commands taken after every step.
    fn run(&mut self, to: Tick) {
        let mut t = self.c.now();
        while t < to {
            let next = [self.c.next_due(), self.plugin.next_due()]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(to)
                .clamp(t + 1, to);
            for (at, o) in self.plugin.due(next) {
                self.c.ingest(&mut self.w, &mut NoInfrastructure, &o, at);
            }
            self.c.run_until(&mut self.w, &mut NoInfrastructure, next);
            self.flush(next);
            t = next;
        }
    }

    /// What the engine decided, to the plugin; returns the march records among it.
    fn flush(&mut self, at: Tick) -> Vec<boundary::BandMarch> {
        let cmds = self.c.take_commands();
        let marches = marches_of(&cmds);
        self.cmds.extend(cmds.iter().cloned().map(|c| (at, c)));
        self.plugin.apply(cmds, at);
        marches
    }

    fn ingest(&mut self, o: Obs, at: Tick) {
        self.c.ingest(&mut self.w, &mut NoInfrastructure, &o, at);
    }

    /// Every march record sent so far for `band`, in order.
    fn sent_for(&self, band: Handle<Party>) -> Vec<boundary::BandMarch> {
        self.cmds
            .iter()
            .filter_map(|(_, c)| match c {
                Cmd::March(m) if m.agent == Party::agent(band) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }
}

fn marches_of(cmds: &[Cmd]) -> Vec<boundary::BandMarch> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::March(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// A band the engine holds embodied at a new epoch with every member in the world, as a watched
/// fight leaves it (the casualty reports below count against that epoch).
fn embody(r: &mut Run, band: Handle<Party>) -> u32 {
    let x = &mut r.c.parties[band];
    x.epoch += 1;
    x.embodied = true;
    x.spawned = x.units.clone();
    x.epoch
}

fn casualty(r: &Run, band: Handle<Party>, epoch: u32) -> Obs {
    let c = r.c.cultures.get(0).clone();
    let body = body_id(&r.c.cultures, &c.unit(r.c.parties[band].units[0].0).body);
    Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_MOB,
        body,
        count: 1,
        killer: (0, 0),
        op: next_op(),
    })
}

// ---- the march record (MZB-451, MZB-452, MZB-455) ----------------------------------------------

/// MZB-451, MZB-452: a party's BAND_MARCH goes when what it says changes. A new band is one record
/// (Mustering, the whole route with its times, the muster's end); a quiet take sends nothing; the
/// muster ending is the next (Out, the route from the segment the band is on, revision 2). The
/// records carry the units by body id, summed and sorted.
#[test]
fn a_march_record_goes_when_what_it_says_changes() {
    let mut r = Run::new(41);
    let band = r.raid();
    let first = r.flush(Tick(21));
    assert_eq!(first.len(), 1, "one record for the new band");
    let m = &first[0];
    let x = &r.c.parties[band];
    assert_eq!(m.agent, Party::agent(band));
    assert_eq!(m.revision, 1);
    assert_eq!(m.phase, boundary::MARCH_MUSTERING);
    assert_eq!(m.kind, PartyKind::Raid as u8);
    assert_eq!(m.dimension, u32::from(x.frame));
    assert_eq!(m.muster_end, x.until.0 as i64);
    assert_eq!(m.flags & boundary::MARCH_ENDED, 0);
    assert_eq!(m.flags & boundary::MARCH_JOINT, 0);
    assert_eq!(m.partner, 0);
    assert_eq!(m.waypoints.len(), x.route.len(), "the whole route");
    for (i, w) in m.waypoints.iter().enumerate() {
        assert_eq!(*w, (x.route[i].0, x.route[i].1, x.times[i].0 as i64));
    }
    let bodies: Vec<u32> = m.units.iter().map(|u| u.0).collect();
    let mut sorted = bodies.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(bodies, sorted, "sorted by body id, one entry for a body");
    let total: u32 = m.units.iter().map(|u| u.1).sum();
    assert_eq!(total, x.alive(), "the counts are the band's alive units");
    // Nothing changed: nothing is sent.
    assert!(r.flush(Tick(22)).is_empty());
    // The muster ends: the band is Out, revision 2.
    let muster = r.c.parties[band].until;
    r.run(muster);
    let sent = r.sent_for(band);
    assert_eq!(sent.len(), 2, "the muster's end is the second record");
    let out = &sent[1];
    assert_eq!(out.revision, 2);
    assert_eq!(out.phase, boundary::MARCH_OUT);
    assert_eq!(out.muster_end, 0);
    let x = &r.c.parties[band];
    assert_eq!(out.waypoints.len(), x.route.len() - x.seg as usize);
    assert_eq!(out.waypoints[0].0, x.route[x.seg as usize].0);
}

/// MZB-451: a segment's advance sends nothing (the waypoints carry their times), several changes
/// between two takes make one record, and a unit loss sends the counts as they then stand.
#[test]
fn a_segment_sends_nothing_and_changes_between_takes_make_one_record() {
    let mut r = Run::new(42);
    let band = r.raid();
    r.flush(Tick(21));
    let muster = r.c.parties[band].until;
    r.run(muster);
    assert!(r.c.parties[band].route.len() > 2, "a route with a middle");
    let sent = r.sent_for(band).len();
    // To the next segment's start and a little past it: no record.
    let second = r.c.parties[band].times[1];
    r.run(second + 5);
    assert_eq!(
        r.c.parties[band].seg, 1,
        "the band is on its second segment"
    );
    assert_eq!(
        r.sent_for(band).len(),
        sent,
        "a segment's advance sends nothing"
    );
    // Two deaths reported in one step: one record, with both gone.
    let epoch = embody(&mut r, band);
    let alive = r.c.parties[band].alive();
    let at = r.c.now() + 1;
    for _ in 0..2 {
        let o = casualty(&r, band, epoch);
        r.ingest(o, at);
    }
    assert_eq!(r.c.parties[band].alive(), alive - 2);
    let marches = r.flush(at);
    assert_eq!(marches.len(), 1, "one record for the two deaths");
    let total: u32 = marches[0].units.iter().map(|u| u.1).sum();
    assert_eq!(total, alive - 2);
    assert_eq!(marches[0].revision, 3);
    assert!(r.flush(at + 1).is_empty());
}

/// MZB-451: the march records come first in a take, one per party in handle order, and the rest
/// of the outbox follows unchanged.
#[test]
fn the_records_come_first_in_handle_order_and_the_rest_follows() {
    let mut r = Run::new(43);
    let a = r.raid();
    let b = r.raid();
    assert!(a < b);
    let cmds = r.c.take_commands();
    let n = cmds
        .iter()
        .take_while(|c| matches!(c, Cmd::March(_)))
        .count();
    assert_eq!(n, 2, "both records first");
    assert!(
        cmds[n..].iter().all(|c| !matches!(c, Cmd::March(_))),
        "none after them"
    );
    assert!(cmds.len() > n, "the rest of the outbox follows");
    let agents: Vec<u64> = marches_of(&cmds).iter().map(|m| m.agent).collect();
    assert_eq!(agents, vec![Party::agent(a), Party::agent(b)]);
}

/// MZB-451: a chief that falls changes what the record says: `chief_killed_by` clears the band's
/// MARCH_CHIEF flag and turns it home, and the record names both.
#[test]
fn a_fallen_chief_marks_the_band_it_led() {
    let mut r = Run::new(44);
    let band = r.raid();
    assert!(r.c.parties[band].chief, "a raid rides with the chief");
    let first = r.flush(Tick(21));
    assert_eq!(
        first[0].flags & boundary::MARCH_CHIEF,
        boundary::MARCH_CHIEF
    );
    let muster = r.c.parties[band].until;
    r.run(muster);
    let at = r.c.now() + 10;
    r.c.chief_killed(&mut r.w, band, None, at);
    assert!(!r.c.parties[band].chief);
    let marches = r.flush(at);
    assert_eq!(marches.len(), 1);
    let m = &marches[0];
    assert_eq!(m.agent, Party::agent(band));
    assert_eq!(m.flags & boundary::MARCH_CHIEF, 0, "no longer led");
    assert_eq!(m.phase, boundary::MARCH_HOME, "and the band routs home");
}

// ---- the band's end and the replay gate (MZB-451, MZB-484, MZB-486, RP-170) --------------------

fn peaceful() -> Obs {
    Obs::Difficulty(boundary::Difficulty {
        difficulty: boundary::DIFFICULTY_PEACEFUL,
    })
}

/// The engine as a restart brings it back: loaded from its save, the session's replay begun.
fn restart(r: &mut Run) {
    r.c = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    r.c.begin_replay(true);
}

/// MZB-451: `home` ends the band with an ENDED record, one revision past the last, and the row
/// goes; the take after sends nothing for it.
#[test]
fn a_band_that_ends_sends_one_ended_record() {
    let mut r = Run::new(45);
    let band = r.raid();
    r.flush(Tick(21));
    r.ingest(peaceful(), Tick(30));
    let muster = r.c.parties[band].until;
    r.run(muster);
    assert!(!r.c.parties.contains(band), "the raid stood down");
    let sent = r.sent_for(band);
    assert_eq!(sent.len(), 2, "mustering, then its end");
    let end = &sent[1];
    assert_eq!(end.revision, 2);
    assert_ne!(end.flags & boundary::MARCH_ENDED, 0);
    assert_eq!(end.agent, Party::agent(band));
    // Nothing more for it, whatever the next takes hold.
    r.run(muster + 2_000);
    assert_eq!(r.sent_for(band).len(), 2);
}

/// MZB-484, RP-170: while the session's replay runs no march record is built and no flag is
/// cleared; the first take after `replay_done` sends each party once, with what the replay left,
/// one revision past what the restart's pages held.
#[test]
fn no_march_record_is_built_while_replaying_and_the_first_take_after_sends_each_party_once() {
    let mut r = Run::new(46);
    let a = r.raid();
    let b = r.raid();
    r.flush(Tick(21));
    let muster = r.c.parties[a].until.max(r.c.parties[b].until);
    r.run(muster);
    assert_eq!(r.sent_for(a).len(), 2);
    restart(&mut r);
    assert!(r.c.replaying());
    let before = [r.c.parties[a].march_rev, r.c.parties[b].march_rev];
    assert!(r.c.parties[a].march_due && r.c.parties[b].march_due);
    for _ in 0..2 {
        let cmds = r.c.take_commands();
        assert!(marches_of(&cmds).is_empty(), "none built while replaying");
    }
    assert!(
        r.c.parties[a].march_due && r.c.parties[b].march_due,
        "flags kept"
    );
    assert_eq!(
        [r.c.parties[a].march_rev, r.c.parties[b].march_rev],
        before,
        "no revision moved"
    );
    r.c.replay_done();
    let got = marches_of(&r.c.take_commands());
    assert_eq!(got.len(), 2, "each party once");
    assert_eq!(
        got.iter().map(|m| m.agent).collect::<Vec<_>>(),
        vec![Party::agent(a), Party::agent(b)]
    );
    assert_eq!(got[0].revision, before[0] + 1);
    assert_eq!(got[1].revision, before[1] + 1);
    assert!(marches_of(&r.c.take_commands()).is_empty(), "and no more");
}

/// MZB-484: a band that ends while the replay runs sends no ENDED record: Java's map never held it.
/// The replay end sends the others, and nothing for the one that is gone.
#[test]
fn a_band_that_ends_while_replaying_sends_no_end() {
    let mut r = Run::new(47);
    let a = r.raid();
    let b = r.raid();
    r.flush(Tick(21));
    restart(&mut r);
    // `a` stands down at its muster's end, as Peaceful ends every mustering war band.
    r.c.ingest(&mut r.w, &mut NoInfrastructure, &peaceful(), Tick(30));
    let muster = r.c.parties[a].until;
    r.c.run_until(&mut r.w, &mut NoInfrastructure, muster);
    assert!(!r.c.parties.contains(a), "the band ended in the replay");
    assert!(!r.c.parties.contains(b), "and so did the other");
    let dropped = r.c.replay_gate_counts();
    assert!(
        marches_of(&r.c.take_commands()).is_empty(),
        "no ENDED while replaying"
    );
    assert_eq!(
        r.c.replay_gate_counts(),
        dropped,
        "none was pushed to be dropped"
    );
    r.c.replay_done();
    assert!(
        marches_of(&r.c.take_commands()).is_empty(),
        "and nothing for what is gone"
    );
}
