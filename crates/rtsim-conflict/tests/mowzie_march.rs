// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M10 (§4.16, ledgers 23 and 25): the march record that
//! keeps Java's map of every band (BAND_MARCH, MZB-451 to MZB-455), the walk home in bodies, the
//! re-binding of a watcher that walks off, and a band that walks into its fight.

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind};
use rtsim_conflict::engine::{Conflict, Party, Phase, Target, body_id};
use rtsim_conflict::synth::{self, FakeConflict};
use rtsim_conflict::{ConflictTunables, Cultures};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_space::topology::{Topologies, Topology};
use rtsim_trade::{GoodId, MILLI};
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

// ---- the saved state (MZB-453, RTC11 / META 10) ------------------------------------------------

/// MZB-453: a party's march revision, its due flag and its unseen tick survive a save and a paged
/// save, and the load builds the list of due parties from the flags, so the first take after a
/// restart sends exactly the parties whose record was due, one revision on.
#[test]
fn the_march_fields_survive_a_save_and_a_paged_save() {
    use rtsim_conflict::scenario::META_KEY;
    use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
    use std::sync::{Arc, RwLock};

    let mut r = Run::new(48);
    let a = r.raid();
    let b = r.raid();
    r.flush(Tick(21));
    let muster = r.c.parties[a].until.max(r.c.parties[b].until);
    r.run(muster);
    // `a` has a record due (a unit lost in a step that was not taken); `b`'s was sent.
    let epoch = embody(&mut r, a);
    let o = casualty(&r, a, epoch);
    r.ingest(o, muster + 1);
    r.c.parties[a].unseen_due = Tick(777);
    assert!(r.c.parties[a].march_due && !r.c.parties[b].march_due);
    assert_eq!(r.c.parties[a].march_rev, 2, "mustering, then out");
    assert_eq!(r.c.parties[b].march_rev, 2);
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(back.state_hash(), r.c.state_hash());
    for p in [a, b] {
        assert_eq!(back.parties[p].march_rev, r.c.parties[p].march_rev);
        assert_eq!(back.parties[p].march_due, r.c.parties[p].march_due);
        assert_eq!(back.parties[p].unseen_due, r.c.parties[p].unseen_due);
    }
    assert_eq!(back.parties[a].unseen_due, Tick(777));
    // The paged save reads back the same, one dir, nothing resident but the parties.
    let dir = std::env::temp_dir().join(format!("rtsim-march-persist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    r.c.attach_store(store.clone(), 8 << 10);
    let mut batch = SaveBatch::new();
    let tickets = r.c.save_paged(&mut batch, META_KEY);
    store.write().unwrap().commit(batch, 0).unwrap();
    r.c.saved(tickets);
    let mut paged = Conflict::open_paged(
        store.clone(),
        8 << 10,
        META_KEY,
        r.c.tun.clone(),
        r.c.cultures.clone(),
    )
    .unwrap()
    .expect("a meta page");
    for p in [a, b] {
        assert_eq!(paged.parties[p].march_rev, r.c.parties[p].march_rev);
        assert_eq!(paged.parties[p].march_due, r.c.parties[p].march_due);
    }
    // Both loads send only what was due, each one revision on.
    let rev = r.c.parties[a].march_rev;
    for c in [&mut paged, &mut { back }] {
        let got = marches_of(&c.take_commands());
        assert_eq!(got.len(), 1, "only the party whose record was due");
        assert_eq!(got[0].agent, Party::agent(a));
        assert_eq!(got[0].revision, rev + 1);
        assert!(marches_of(&c.take_commands()).is_empty());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the end at home and the books (MZB-462, MZB-463) ------------------------------------------

/// MZB-463: a band that comes home with its puppets in Minecraft and nothing to hand them back
/// (a faction with no camp ledger) is recalled toward its camp with RECALL_DONE at its epoch,
/// before its ENDED record; the same band with no puppets is not recalled at all.
#[test]
fn a_band_home_in_minecraft_with_no_hand_back_is_recalled_before_its_end() {
    for embodied in [true, false] {
        let mut r = Run::new(49);
        let band = r.raid();
        r.flush(Tick(21));
        let mut t = r.c.now();
        while r.c.parties[band].phase != Phase::Home {
            t += 200;
            r.run(t);
            assert!(t < Tick(20 * 24_000), "the band never walked home");
        }
        let agent = Party::agent(band);
        let epoch = if embodied { embody(&mut r, band) } else { 0 };
        let end = r.c.parties[band].times.last().copied().unwrap();
        r.run(end + 100);
        assert!(!r.c.parties.contains(band), "home");
        let recalls: Vec<(usize, u32, u32)> = r
            .cmds
            .iter()
            .enumerate()
            .filter_map(|(i, (_, c))| match c {
                Cmd::Recall(x) if x.agent == agent => Some((i, x.epoch, x.reason)),
                _ => None,
            })
            .collect();
        let ended = r
            .cmds
            .iter()
            .position(|(_, c)| {
                matches!(c, Cmd::March(m) if m.agent == agent
                && m.flags & boundary::MARCH_ENDED != 0)
            })
            .expect("the band's end");
        if embodied {
            assert_eq!(recalls.len(), 1, "one recall: {recalls:?}");
            assert_eq!(recalls[0].1, epoch);
            assert_eq!(recalls[0].2, boundary::RECALL_DONE);
            assert!(recalls[0].0 < ended, "the recall goes before the end");
        } else {
            assert!(recalls.is_empty(), "no puppet, no recall");
        }
    }
}

// ---- the walk home in bodies (MZB-456, MZB-489) ------------------------------------------------

/// A raid band on its way out, a tick past its muster, and the engine's phase there.
fn out_on_the_road(r: &mut Run) -> Handle<Party> {
    let band = r.raid();
    r.flush(Tick(21));
    let muster = r.c.parties[band].until;
    r.run(muster + 1);
    assert_eq!(r.c.parties[band].phase, Phase::Out, "on the road");
    band
}

/// The steers and recalls sent for `agent` from command `mark` on, in order: (index, steer's
/// epoch, target, flags) and (index, recall's epoch, reason).
type Orders = (Vec<(usize, u32, (f64, f64), u32)>, Vec<(usize, u32, u32)>);

fn orders_since(r: &Run, agent: u64, mark: usize) -> Orders {
    let (mut steers, mut recalls) = (Vec::new(), Vec::new());
    for (i, (_, c)) in r.cmds.iter().enumerate().skip(mark) {
        match c {
            Cmd::Steer(s) if s.agent == agent => {
                steers.push((i, s.epoch, (s.pos.0, s.pos.2), s.flags));
            }
            Cmd::Recall(x) if x.agent == agent => recalls.push((i, x.epoch, x.reason)),
            _ => {}
        }
    }
    (steers, recalls)
}

/// MZB-456: a band in bodies that is routed (its chief killed) walks home where its watcher can
/// see it: no recall at the order, one steer toward each home segment's end (never the war
/// band's attack flag), and the recall fallback only at its door.
#[test]
fn a_routed_band_in_bodies_walks_home_a_segment_at_a_time() {
    let mut r = Run::new(44);
    let band = out_on_the_road(&mut r);
    let agent = Party::agent(band);
    let epoch = embody(&mut r, band);
    let mark = r.cmds.len();
    let at = r.c.now() + 10;
    r.c.chief_killed(&mut r.w, band, None, at);
    r.flush(at);
    let x = &r.c.parties[band];
    assert!(
        x.phase == Phase::Home && x.embodied,
        "walking home in its bodies"
    );
    assert_eq!(x.epoch, epoch, "the same bodies");
    let (route, end) = (x.route.clone(), x.times.last().copied().unwrap());
    let (steers, recalls) = orders_since(&r, agent, mark);
    assert_eq!(steers.len(), 1, "the first segment is steered at the order");
    assert!(
        recalls.is_empty(),
        "nothing is recalled at the order: {recalls:?}"
    );
    r.run(end + 100);
    assert!(!r.c.parties.contains(band), "home");
    let (steers, recalls) = orders_since(&r, agent, mark);
    assert_eq!(steers.len(), route.len() - 1, "one steer per segment home");
    for (s, w) in steers.iter().zip(route.iter().skip(1)) {
        assert_eq!(s.1, epoch);
        assert_eq!(
            s.2,
            (f64::from(w.0), f64::from(w.1)),
            "toward the segment's end"
        );
        assert_eq!(s.3, 0, "walking home attacks nothing");
    }
    assert_eq!(
        recalls.len(),
        1,
        "only the fallback at the door: {recalls:?}"
    );
    assert!(recalls[0].0 > steers.last().unwrap().0);
    assert_eq!((recalls[0].1, recalls[0].2), (epoch, boundary::RECALL_DONE));
}

/// MZB-456: the rest are recalled toward the camp as before: on Peaceful, with nothing placed
/// in Minecraft, and while the session's replay runs (no body of this session stands yet).
#[test]
fn a_band_that_cannot_walk_home_is_recalled_where_it_stands() {
    for case in ["peaceful", "nothing placed", "replaying"] {
        let mut r = Run::new(44);
        let band = out_on_the_road(&mut r);
        let agent = Party::agent(band);
        let epoch = embody(&mut r, band);
        let mark = r.cmds.len();
        let at = r.c.now() + 10;
        match case {
            "peaceful" => r.c.set_difficulty(&mut r.w, boundary::DIFFICULTY_PEACEFUL),
            "nothing placed" => {
                r.c.parties[band].spawned.clear();
                r.c.chief_killed(&mut r.w, band, None, at);
            }
            _ => {
                restart(&mut r);
                assert!(r.c.parties[band].embodied, "the pages hold it embodied");
                r.c.chief_killed(&mut r.w, band, None, at);
            }
        }
        r.flush(at);
        let x = &r.c.parties[band];
        assert!(
            x.phase == Phase::Home && !x.embodied,
            "{case}: sent home recalled"
        );
        let (steers, recalls) = orders_since(&r, agent, mark);
        assert!(steers.is_empty(), "{case}: nothing steered: {steers:?}");
        if case != "replaying" {
            assert_eq!(recalls.len(), 1, "{case}: {recalls:?}");
            assert_eq!(recalls[0].1, epoch);
        }
    }
}

/// MZB-456: a siege in bodies that grows bored at its target walks home too, and the walk's
/// first steer is sent at the order.
#[test]
fn a_bored_siege_in_bodies_walks_home() {
    let mut r = Run::new(45);
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Siege,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.flush(Tick(21));
    let mut t = r.c.now();
    while r.c.parties[band].phase != Phase::Waiting {
        t += 600;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the siege never reached its target");
    }
    let agent = Party::agent(band);
    let epoch = embody(&mut r, band);
    let mark = r.cmds.len();
    let bored = r.c.parties[band].bored_at;
    while r.c.parties.contains(band) && r.c.parties[band].phase == Phase::Waiting {
        t += 100;
        r.run(t);
        assert!(t < bored + 20_000, "the siege never grew bored");
    }
    let (steers, recalls) = orders_since(&r, agent, mark);
    let x = &r.c.parties[band];
    assert!(
        x.phase == Phase::Home && x.embodied,
        "walking home in its bodies"
    );
    assert!(!steers.is_empty() && steers.iter().all(|s| s.1 == epoch));
    assert!(recalls.is_empty(), "{recalls:?}");
}

// ---- met on the road, and the way it faces (MZB-457) -------------------------------------------

/// The route and segment of `band` and the way the engine should face it there, in degrees from
/// +x toward +z: toward the segment's end.
fn heading(r: &Run, band: Handle<Party>) -> f32 {
    let x = &r.c.parties[band];
    let s = (x.seg as usize).min(x.route.len() - 2);
    let (a, b) = (x.route[s], x.route[s + 1]);
    ((b.1 - a.1) as f64).atan2((b.0 - a.0) as f64).to_degrees() as f32
}

/// The band's materialize command (the first sent at its epoch) and the steer after it.
fn met_orders(r: &Run, band: Handle<Party>) -> (boundary::BandMaterialize, boundary::BandSteer) {
    let agent = Party::agent(band);
    let epoch = r.c.parties[band].epoch;
    let m = r
        .cmds
        .iter()
        .find_map(|(_, c)| match c {
            Cmd::Materialize(m) if m.agent == agent && m.epoch == epoch => Some(m.clone()),
            _ => None,
        })
        .expect("materialized");
    let s = r
        .cmds
        .iter()
        .find_map(|(_, c)| match c {
            Cmd::Steer(s) if s.agent == agent && s.epoch == epoch => Some(*s),
            _ => None,
        })
        .expect("steered");
    (m, s)
}

/// MZB-457: a band met on its way out or on its way home is materialized in one group (no arc)
/// facing its march, and steered along the segment it is on; a home band is no longer skipped.
#[test]
fn a_band_met_on_the_road_either_way_is_placed_facing_its_march_and_steered() {
    for home in [false, true] {
        let mut r = Run::new(46);
        let band = r.raid();
        r.flush(Tick(21));
        let mut t = r.c.now();
        let want = if home { Phase::Home } else { Phase::Out };
        while r.c.parties[band].phase != want {
            t += 200;
            r.run(t);
            assert!(t < Tick(20 * 24_000), "the band never reached {want:?}");
        }
        // The player stands on the end of the segment the band walks.
        let x = &r.c.parties[band];
        let s = (x.seg as usize).min(x.route.len() - 2);
        let ahead = x.route[s + 1];
        let epoch = x.epoch;
        let at_the_end = Pos::new(0, ahead.0, ahead.1);
        while r.c.stats.seen == 0 {
            r.c.observe_player(&mut r.w, synth::PLAYER, at_the_end, t);
            t += 600;
            r.run(t);
            assert!(
                r.c.parties.contains(band),
                "home {home}: the band was never met"
            );
        }
        let x = &r.c.parties[band];
        assert!(x.embodied && x.phase == want, "home {home}");
        assert_eq!(x.epoch, epoch + 1);
        let face = heading(&r, band);
        let (m, steer) = met_orders(&r, band);
        assert_eq!(m.arc_radius, 0, "home {home}: one group, no arc");
        assert!(
            (m.bearing_deg - face).abs() < 0.5,
            "home {home}: it faces {} and its march is {face}",
            m.bearing_deg
        );
        let x = &r.c.parties[band];
        let s = (x.seg as usize).min(x.route.len() - 2);
        let end = x.route[s + 1];
        assert_eq!(
            (steer.pos.0, steer.pos.2),
            (f64::from(end.0), f64::from(end.1))
        );
        assert_eq!(
            steer.flags, 0,
            "home {home}: met on the road, attacking nothing"
        );
    }
}

/// MZB-457: a band met waiting at its target faces the way it came, from the last waypoint
/// before the target.
#[test]
fn a_band_met_waiting_faces_the_way_it_came() {
    let mut r = Run::new(47);
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Siege,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.flush(Tick(21));
    let mut t = r.c.now();
    while r.c.parties[band].phase != Phase::Waiting {
        t += 600;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the siege never reached its target");
    }
    let x = &r.c.parties[band];
    let (before, target) = (x.route[x.route.len() - 2], *x.route.last().unwrap());
    assert!(r.c.stats.seen == 0 && !x.embodied);
    let there = Pos::new(0, target.0, target.1);
    while r.c.stats.seen == 0 {
        r.c.observe_player(&mut r.w, synth::PLAYER, there, t);
        t += 600;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the waiting band was never met");
    }
    assert!(r.c.parties[band].embodied, "met at its target");
    let (epoch, agent) = (r.c.parties[band].epoch, Party::agent(band));
    let m = r
        .cmds
        .iter()
        .find_map(|(_, c)| match c {
            Cmd::Materialize(m) if m.agent == agent && m.epoch == epoch => Some(m.clone()),
            _ => None,
        })
        .expect("materialized");
    let centre = (m.arc_centre.0 as i32, m.arc_centre.1 as i32);
    let came = ((centre.1 - before.1) as f64)
        .atan2((centre.0 - before.0) as f64)
        .to_degrees() as f32;
    assert_eq!(m.arc_radius, 0);
    assert!(
        (m.bearing_deg - came).abs() < 0.5,
        "it faces {} and came by {came}",
        m.bearing_deg
    );
}

/// MZB-490: a band that walks home with none of its members alive (routed after its wipe-out) is
/// not met on the road: there is nobody to place.
#[test]
fn a_band_that_walks_home_with_none_alive_is_not_met() {
    let mut r = Run::new(46);
    let band = r.raid();
    r.flush(Tick(21));
    let mut t = r.c.now();
    while r.c.parties[band].phase != Phase::Home {
        t += 200;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the band never walked home");
    }
    for e in &mut r.c.parties[band].units {
        e.1 = 0;
    }
    let x = &r.c.parties[band];
    let ahead = x.route[(x.seg as usize).min(x.route.len() - 2) + 1];
    let there = Pos::new(0, ahead.0, ahead.1);
    while r.c.parties.contains(band) {
        r.c.observe_player(&mut r.w, synth::PLAYER, there, t);
        t += 600;
        r.run(t);
        assert_eq!(r.c.stats.seen, 0, "nobody to meet");
        assert!(t < Tick(20 * 24_000));
    }
}

// ---- re-binding, and a band that walks into its fight (MZB-458 to MZB-461) ---------------------

/// A second player, who is never the scripted one.
const OTHER: (u64, u64) = (0x0B0B, 0xCAFE);

fn at_pos(p: (i32, i32)) -> Pos {
    Pos::new(0, p.0, p.1)
}

/// A raid on the road, met by the player at its camp the tick it sets out: embodied, seen by
/// `r.p`, its exit from the player's view booked. Returns the band and that exit.
fn met_leaving_its_camp(r: &mut Run) -> (Handle<Party>, Tick) {
    let band = out_on_the_road(r);
    let mut t = r.c.now();
    while r.c.stats.seen == 0 {
        r.c.observe_player(&mut r.w, synth::PLAYER, synth::CAMP_POS, t);
        t += 100;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the band was never met");
    }
    let x = &r.c.parties[band];
    assert!(x.embodied && x.seen_by == Some(r.p), "met by the player");
    let exit = x.unseen_due;
    assert!(
        exit > r.c.now() + 600,
        "it walks out of view later: {exit:?}"
    );
    (band, exit)
}

/// MZB-458: where the watcher's circle (192 blocks) lets the band go, a player near it takes it
/// over: another player 180 blocks off, or the same one who followed at that distance. With
/// nobody there it is recalled where it leaves. Either way the next exit is booked a tick on at
/// the soonest, and the booking at hand is the only one that acts.
#[test]
fn a_band_walking_out_of_its_watchers_view_passes_to_a_player_near_it() {
    for case in ["another", "the same", "nobody"] {
        let mut r = Run::new(48);
        let (band, exit) = met_leaving_its_camp(&mut r);
        let agent = Party::agent(band);
        let epoch = r.c.parties[band].epoch;
        let here = r.c.party_pos(band, exit);
        let near = at_pos((here.0 + 180, here.1));
        r.run(Tick(exit.0 - 60));
        let other = match case {
            "another" => Some(r.c.observe_player(&mut r.w, OTHER, near, Tick(exit.0 - 60))),
            "the same" => {
                Some(r.c.observe_player(&mut r.w, synth::PLAYER, near, Tick(exit.0 - 60)))
            }
            _ => None,
        };
        let mark = r.cmds.len();
        r.run(exit + 3);
        let x = &r.c.parties[band];
        let (_, recalls) = orders_since(&r, agent, mark);
        match other {
            Some(player) => {
                assert!(
                    x.embodied && recalls.is_empty(),
                    "{case}: kept: {recalls:?}"
                );
                assert_eq!(x.seen_by, Some(player), "{case}");
                assert!(x.unseen_due > exit, "{case}: booked a tick on");
            }
            None => {
                assert!(!x.embodied, "{case}: recalled");
                assert_eq!(recalls.len(), 1, "{case}: {recalls:?}");
                assert_eq!(
                    (recalls[0].1, recalls[0].2),
                    (epoch, boundary::RECALL_PASSING)
                );
            }
        }
    }
}

/// MZB-458: only the latest booking acts. A booking the band's `unseen_due` has moved past does
/// nothing when its tick comes.
#[test]
fn an_unseen_superseded_by_a_later_booking_does_nothing() {
    let mut r = Run::new(48);
    let (band, exit) = met_leaving_its_camp(&mut r);
    r.c.parties[band].unseen_due = exit + 100_000;
    let mark = r.cmds.len();
    r.run(exit + 600);
    let (_, recalls) = orders_since(&r, Party::agent(band), mark);
    assert!(
        r.c.parties[band].embodied && recalls.is_empty(),
        "the old booking found the band moved on"
    );
}

/// `r.c`'s party as a watched fight leaves it: embodied, every member placed, `seen_by` set to the
/// scripted player (never reported to the world).
fn watched_in_bodies(r: &mut Run, band: Handle<Party>) -> u32 {
    let epoch = embody(r, band);
    r.c.parties[band].seen_by = Some(r.p);
    epoch
}

/// MZB-459: the watcher logs out of a fight. With a second player 100 blocks off the band keeps
/// its bodies and turns to waiting for that player; with nobody it is recalled; a siege is
/// recalled either way.
#[test]
fn a_watcher_logging_out_leaves_a_fight_to_a_player_near_it() {
    for (kind, near) in [
        (PartyKind::Raid, true),
        (PartyKind::Raid, false),
        (PartyKind::Siege, true),
    ] {
        let mut r = Run::new(50);
        let band =
            r.c.order_party(
                &mut r.w,
                synth::CAMP,
                kind,
                Target::District(synth::REFINERY, synth::REFINERY_POS),
                Some(r.p),
                Tick(20),
            )
            .unwrap();
        r.flush(Tick(21));
        let mut t = r.c.now();
        while r.c.parties[band].phase != Phase::Waiting && r.c.parties[band].phase != Phase::Out {
            t += 600;
            r.run(t);
        }
        let agent = Party::agent(band);
        let epoch = watched_in_bodies(&mut r, band);
        r.c.parties[band].phase = Phase::Fighting;
        let other = near.then(|| {
            let here = r.c.party_pos(band, t);
            r.c.observe_player(&mut r.w, OTHER, at_pos((here.0 + 100, here.1)), t)
        });
        let mark = r.cmds.len();
        r.c.player_left(&mut r.w, r.p);
        r.flush(t);
        let x = &r.c.parties[band];
        let (_, recalls) = orders_since(&r, agent, mark);
        let label = format!("{kind:?}, a second player near: {near}");
        assert_eq!(x.phase, Phase::Waiting, "{label}: a fight turns to waiting");
        if kind == PartyKind::Raid && near {
            assert!(x.embodied && recalls.is_empty(), "{label}: bodies kept");
            assert_eq!(x.seen_by, other, "{label}");
        } else {
            assert!(!x.embodied, "{label}: recalled");
            assert_eq!(recalls.len(), 1, "{label}: {recalls:?}");
            assert_eq!(
                (recalls[0].1, recalls[0].2),
                (epoch, boundary::RECALL_PLAYER_LEFT)
            );
        }
    }
}

/// A raid in bodies that arrives at the refinery a tick from now, with the only player `off`
/// blocks east of it (reported fresh). Returns the band, its epoch and the player's actor.
fn arriving_with_a_player(r: &mut Run, off: i32) -> (Handle<Party>, u32, ActorId) {
    let band = out_on_the_road(r);
    // Nobody is booked as its watcher: it walks to its target in bodies no one is tracking.
    let epoch = embody(r, band);
    let arrive = *r.c.parties[band].times.last().unwrap();
    r.run(Tick(arrive.0 - 200));
    let player = r.c.observe_player(
        &mut r.w,
        OTHER,
        at_pos((synth::REFINERY_POS.x + off, synth::REFINERY_POS.z)),
        Tick(arrive.0 - 200),
    );
    (band, epoch, player)
}

/// MZB-460: a band that walked to its target in bodies and finds no one within the view radius
/// (160) but a player within the circle (192), at 170 blocks, passes to that player: resolved
/// abstractly, its losses taken off the placed members, and walked home in the bodies. At 150
/// blocks it is a fight.
#[test]
fn an_arrival_with_a_player_170_blocks_off_is_abstract_and_150_is_a_fight() {
    // 170: re-bound, abstract.
    let mut r = Run::new(51);
    let (band, epoch, player) = arriving_with_a_player(&mut r, 170);
    let agent = Party::agent(band);
    let mark = r.cmds.len();
    let arrive = *r.c.parties[band].times.last().unwrap();
    r.run(arrive + 10);
    let x = &r.c.parties[band];
    let (steers, recalls) = orders_since(&r, agent, mark);
    assert_eq!(r.c.stats.raids_watched, 0, "no fight");
    assert!(recalls.is_empty(), "not recalled: {recalls:?}");
    assert!(x.embodied && x.epoch == epoch && x.seen_by == Some(player));
    assert_eq!(x.phase, Phase::Home, "walking home");
    assert!(!steers.is_empty(), "steered home in its bodies");
    for s in &x.spawned {
        let alive = x.units.iter().find(|e| e.0 == s.0).map_or(0, |e| e.1);
        assert!(s.1 <= alive, "placed {} of {alive}", s.1);
    }
    // 150: a fight, in the same bodies.
    let mut r = Run::new(51);
    let (band, epoch, player) = arriving_with_a_player(&mut r, 150);
    let agent = Party::agent(band);
    let mark = r.cmds.len();
    let arrive = *r.c.parties[band].times.last().unwrap();
    r.run(arrive + 10);
    let x = &r.c.parties[band];
    assert_eq!(r.c.stats.raids_watched, 1, "a fight");
    assert_eq!(x.phase, Phase::Fighting);
    assert!(x.embodied && x.epoch == epoch && x.seen_by == Some(player));
    let (steers, recalls) = orders_since(&r, agent, mark);
    assert!(recalls.is_empty(), "{recalls:?}");
    assert!(
        steers
            .iter()
            .any(|s| s.1 == epoch && s.3 == boundary::STEER_ATTACK),
        "told to attack where it stands: {steers:?}"
    );
}

/// MZB-461: the walked-in fight's arc sends only the members not placed yet, at the band's own
/// epoch, flagged as a fight's wave. A band with every member placed (up to the quota) sends the
/// same record with no members (MZB-700, MZB-492): the fight's one flagged record, which Java
/// echoes and spawns nothing for, and which a restart's replay needs to release the hold of the
/// arrival (the gap the first version of this test pinned). It is not counted in `stats.waves` or
/// `stats.materialized`, which count bodies sent.
#[test]
fn a_band_walking_into_its_fight_sends_only_the_members_not_placed() {
    for placed in [false, true] {
        let mut r = Run::new(51);
        let (band, epoch, _) = arriving_with_a_player(&mut r, 100);
        if !placed {
            // Three members of the first unit are in Minecraft: 13 more fit under the quota.
            let first = r.c.parties[band].units[0].0;
            r.c.parties[band].spawned = vec![(first, 3)];
        }
        let agent = Party::agent(band);
        let alive = r.c.parties[band].alive();
        let mark = r.cmds.len();
        let (waves_before, materialized_before) = (r.c.stats.waves, r.c.stats.materialized);
        let arrive = *r.c.parties[band].times.last().unwrap();
        r.run(arrive + 10);
        let waves: Vec<boundary::BandMaterialize> = r.cmds[mark..]
            .iter()
            .filter_map(|(_, c)| match c {
                Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            r.c.parties[band].epoch, epoch,
            "placed {placed}: same epoch"
        );
        assert_eq!(waves.len(), 1, "placed {placed}: {waves:?}");
        let w = &waves[0];
        assert_eq!(w.epoch, epoch);
        assert!(w.arc_radius > 0 && w.flags & boundary::BAND_FIGHT != 0);
        let sent = w.members.iter().map(|s| s.count).sum::<u32>();
        if placed {
            assert!(w.members.is_empty(), "everything is there already: {w:?}");
            assert_eq!(
                (r.c.stats.waves, r.c.stats.materialized),
                (waves_before, materialized_before),
                "no bodies sent, no wave counted"
            );
        } else {
            assert_eq!(
                sent,
                alive.min(r.c.tun.spawn_quota) - 3,
                "the members left, within the room under the quota"
            );
            assert_eq!(
                (r.c.stats.waves, r.c.stats.materialized),
                (waves_before + 1, materialized_before + 1)
            );
        }
    }
}

/// MZB-476: the fake plugin keeps each party's latest march record and trims its held body to the
/// record's counts. A death it never saw is gone from the survivors it names when the band is
/// recalled.
#[test]
fn the_fake_plugin_keeps_the_latest_march_and_trims_its_held_body() {
    let mut r = Run::new(52);
    let (band, exit) = met_leaving_its_camp(&mut r);
    let agent = Party::agent(band);
    assert_eq!(
        r.plugin.marches[&agent].revision,
        r.sent_for(band).last().unwrap().revision,
        "the latest of each"
    );
    let epoch = r.c.parties[band].epoch;
    let o = casualty(&r, band, epoch);
    let now = r.c.now();
    r.ingest(o, now + 1);
    r.run(now + 100);
    let alive = r.c.parties[band].alive();
    let held: u32 = r.plugin.marches[&agent].units.iter().map(|u| u.1).sum();
    assert_eq!(
        held, alive,
        "the record the plugin holds says what is alive"
    );
    r.run(Tick(exit.0 + 3));
    assert!(!r.c.parties[band].embodied, "recalled at its exit");
    let survivors: u32 = r
        .plugin
        .due(Tick(exit.0 + 2_000))
        .into_iter()
        .find_map(|(_, o)| match o {
            Obs::Dematerialized(d) if d.agent == agent => {
                Some(d.survivors.iter().map(|s| s.1).sum())
            }
            _ => None,
        })
        .expect("the recall's answer");
    assert_eq!(survivors, alive, "the body was trimmed by the record");
}

// ---- the books of a band resolved abstractly while it stands in bodies (MZB-462, MZB-466) ------

/// The survivors a despawn or a recall would count for `band`: its units alive by body id, as
/// Java would hold them once it has trimmed the puppets to the members left (MZB-473).
fn alive_by_body(r: &Run, band: Handle<Party>) -> Vec<(u32, u32)> {
    let c = r.c.cultures.get(0).clone();
    r.c.parties[band]
        .units
        .iter()
        .filter(|e| e.1 > 0)
        .map(|&(u, n)| (body_id(&r.c.cultures, &c.unit(u).body), n))
        .collect()
}

/// MZB-462 (the design's `lose_units` test): a band that walked to a village in bodies and is
/// resolved abstractly there (its watcher is 170 blocks off, so it is re-bound, MZB-460) loses
/// members to the village's defence; the bodies the books hold follow the members left. When
/// that watcher leaves, the band is recalled with those bodies, and Java's despawn counts only
/// the one that died on the walk off: the band keeps the members the village left it less that
/// one, not none (the recalled stack was the band that went out, and its survivors would all
/// have counted dead). The same when the engine is saved after the recall and the despawn is
/// replayed from the pages (the replay keeps the recalled stack, MZB-138).
#[test]
fn the_dead_of_an_abstract_fight_in_bodies_are_counted_once_at_the_recall_and_across_a_restart() {
    for crash in [false, true] {
        let mut r = Run::new(63);
        let ashby = r.w.node_of_key(4).expect("Ashby");
        let there = r.w.nodes[ashby].pos;
        // Something to take at Ashby: 20 items over each line's target (CL-D58).
        let m = r.w.market(ashby);
        let rich: BTreeMap<GoodId, i64> =
            r.w.goods
                .ids()
                .map(|g| (g, m.line(g).target + 20 * MILLI - m.stock(g)))
                .filter(|e| e.1 > 0)
                .collect();
        r.w.stock_from_players(ashby, &rich, Tick(10));
        let band =
            r.c.order_party(
                &mut r.w,
                synth::CAMP,
                PartyKind::Raid,
                Target::Node(ashby),
                Some(r.p),
                Tick(20),
            )
            .unwrap();
        r.flush(Tick(21));
        let muster = r.c.parties[band].until;
        r.run(muster + 1);
        assert_eq!(r.c.parties[band].phase, Phase::Out);
        let epoch = embody(&mut r, band);
        let n = r.c.parties[band].alive();
        let arrive = *r.c.parties[band].times.last().unwrap();
        r.run(Tick(arrive.0 - 200));
        let watcher = r.c.observe_player(
            &mut r.w,
            OTHER,
            at_pos((there.x + 170, there.z)),
            Tick(arrive.0 - 200),
        );
        r.run(arrive + 10);
        let x = &r.c.parties[band];
        assert_eq!(
            r.c.stats.raids_village, 1,
            "the village was raided abstractly"
        );
        let left = x.alive();
        assert!(
            left >= 2 && left < n,
            "the village took some: {n} -> {left}"
        );
        assert!(
            x.embodied && x.epoch == epoch,
            "still in bodies, walking home"
        );
        assert_eq!(
            x.spawned.iter().map(|s| s.1).sum::<u32>(),
            left,
            "the bodies the books hold follow the members left"
        );
        // The watcher leaves: nobody else is near, so the band is recalled with its bodies.
        let mark = r.cmds.len();
        let t = r.c.now();
        r.c.player_left(&mut r.w, watcher);
        r.flush(t);
        let (_, recalls) = orders_since(&r, Party::agent(band), mark);
        assert_eq!(recalls.len(), 1, "{recalls:?}");
        let x = &r.c.parties[band];
        assert!(!x.embodied && x.spawned.is_empty(), "recalled");
        assert_eq!(
            x.recalled,
            vec![(epoch, x.units.clone())],
            "the recalled stack is what the members left, not the band that went out"
        );
        if crash {
            // The pages hold the recall; the despawn comes from the journal.
            restart(&mut r);
        }
        // One of the bodies dies on the walk off: Java's despawn counts the rest.
        let mut survivors = alive_by_body(&r, band);
        survivors[0].1 -= 1;
        let at = r.c.now() + 1;
        r.ingest(
            Obs::Dematerialized(boundary::BandDematerialized {
                agent: Party::agent(band),
                epoch,
                reason: boundary::RECALL_PLAYER_LEFT,
                survivors,
            }),
            at,
        );
        if crash {
            r.c.replay_done();
        }
        let x = &r.c.parties[band];
        assert_eq!(
            x.alive(),
            left - 1,
            "crash {crash}: the one that died, no more"
        );
        assert!(
            x.recalled.is_empty(),
            "crash {crash}: the despawn is answered"
        );
    }
}

/// MZB-701: the band of `met_leaving_its_camp`, its road at the camp's exit replaced by a nearly
/// axis-aligned one (400 blocks along +x and 1 along +z, as integer waypoints make it), and its
/// watcher moved, before that exit fires, to where the road passes nearly tangent to the 192-block
/// circle, on the road's lower-z side (the side a positive z truncates toward). `exit_time` solves
/// the continuous path, but `inside_now` reads the truncated chart position, which is still
/// inside then (its z is 0.85 short): `unseen` re-bound the same player and booked a tick on,
/// every tick, until the truncated position left (645 ticks in a row here before the fix, each
/// with a wheel event, a scan of the players and an exit solve). The exit is now booked at the
/// first tick the truncated position is outside, so the one watcher is never re-bound for it.
#[test]
fn a_near_tangent_exit_is_not_rebooked_tick_after_tick() {
    let mut r = Run::new(48);
    let (band, exit) = met_leaving_its_camp(&mut r);
    let here = r.c.party_chart_pos(band, exit);
    // The road: A to B, 400 blocks east and 1 south, at the goblins' 0.02 blocks a tick; at the
    // exit the band is 340 blocks along it (z 0.85 past A's).
    let a = (here.0 - 340, here.1);
    let b = (a.0 + 400, a.1 + 1);
    let t0 = Tick(exit.0 - 17_000);
    let t1 = Tick(t0.0 + 20_000);
    {
        let x = &mut r.c.parties[band];
        x.route = vec![a, b];
        x.times = vec![t0, t1];
        x.seg = 0;
    }
    assert_eq!(r.c.party_chart_pos(band, exit), (a.0 + 340, a.1));
    // The watcher 191.85 blocks south of the road's point at the exit (the road at z + 0.85).
    let c = (a.0 + 340, a.1 - 191);
    r.run(Tick(exit.0 - 60));
    r.c.observe_player(&mut r.w, synth::PLAYER, at_pos(c), Tick(exit.0 - 60));
    let (mut run, mut longest, mut rebinds) = (0u64, 0u64, 0u64);
    let events = r.c.stats.events;
    let mut t = exit;
    while t < exit + 4_000 && r.c.parties[band].embodied {
        r.run(t + 1);
        let due = r.c.parties[band].unseen_due;
        if due == t + 2 {
            run += 1;
            rebinds += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
        t += 1;
    }
    let q = r.c.party_chart_pos(band, t);
    let events = r.c.stats.events - events;
    eprintln!(
        "watcher at {c:?}, longest run of tick-on bookings {longest}, {rebinds} in all, {events} events, embodied {} at {} ({q:?})",
        r.c.parties[band].embodied, t.0
    );
    // The bound: the re-bind at the booked exit books a tick on once, and the stepped exit is
    // then the first tick the watcher is outside, where nobody is left to take the band over.
    assert!(
        longest <= 1 && rebinds <= 1,
        "the re-bind re-booked a tick on for {longest} ticks in a row ({rebinds} in all)"
    );
    assert!(!r.c.parties[band].embodied, "recalled once outside");
    assert!(events <= 10, "{events} events from the exit to the recall");
    // The stepping's worst case: the truncated position is at most sqrt 2 blocks off the path, so
    // it is outside once the path is that far beyond the circle (radius R, 192 here). On a chord
    // that is at most sqrt(2 sqrt 2 R + 2) blocks of walk past the continuous exit (a tangent),
    // at the goblins' 0.02 blocks a tick.
    let r_circle = 192.0_f64;
    let (ax, az) = (f64::from(a.0 - c.0), f64::from(a.1 - c.1));
    let (qa, qb, qc) = (
        400.0_f64 * 400.0 + 1.0,
        2.0 * (ax * 400.0 + az),
        ax * ax + az * az - r_circle * r_circle,
    );
    let f = (-qb + (qb * qb - 4.0 * qa * qc).max(0.0).sqrt()) / (2.0 * qa);
    let root = t0.0 as f64 + (t1.0 - t0.0) as f64 * f;
    let worst = (2.0 * 2.0_f64.sqrt() * r_circle + 2.0).sqrt() / 0.02;
    let stepped = t.0 as f64 - root;
    eprintln!(
        "the continuous exit at {root:.1}, the stepped one {stepped:.1} ticks on (worst {worst:.0})"
    );
    assert!(
        (0.0..=worst + 1.0).contains(&stepped),
        "{stepped} ticks past the root, the bound is {worst}"
    );
}

/// MZB-456 (ledger 23's seam lap, MZB-751): a band that has crossed an x seam and routs walks home
/// from its position in the route's chart, not from its canonical cell. The canonical cell is a
/// lap away from the waypoints (x wraps over [0, 1200), the band stands on the far side of the
/// seam), so a route that starts there has a first segment as long as the world, and its first
/// steer would send the band across it.
#[test]
fn a_band_routed_after_the_seam_walks_home_from_its_chart_position_not_a_lap_away() {
    let seed = 21;
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    let mut topos = Topologies::new();
    topos.set(0, Topology::x_wrap(0, 1200));
    c.set_topologies(topos.shared());
    c.add_faction(&mut w, synth::CAMP, "frostmaw", synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    let who = c.player(&mut w, synth::PLAYER);
    // The camp is at x 900; the target at x 100 is 400 blocks east across the seam.
    let band = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::Pos(Pos::new(0, 100, 1000)),
            Some(who),
            Tick(20),
        )
        .expect("a raid");
    let mut t = c.parties[band].until;
    let end = *c.parties[band].times.last().expect("a route");
    // Walk it out until it stands past the seam: its chart cell and its canonical cell differ.
    let mut over = false;
    while t < end {
        t += 20;
        c.run_until(&mut w, &mut NoInfrastructure, t);
        let _ = c.take_commands();
        if !c.parties.contains(band) {
            break;
        }
        if c.party_chart_pos(band, t) != c.party_pos(band, t) {
            over = true;
            break;
        }
    }
    assert!(over, "the band never crossed the seam");
    let here = c.party_chart_pos(band, t);
    let canonical = c.party_pos(band, t);
    assert_ne!(here, canonical, "past the seam the two cells differ");
    let (route, seg) = (c.parties[band].route.clone(), c.parties[band].seg as usize);
    c.chief_killed(&mut w, band, None, t);
    let x = &c.parties[band];
    assert_eq!(x.phase, Phase::Home);
    // The way home is `here`, then the road it came by, back to front.
    let mut want: Vec<(i32, i32)> = route[..=seg].to_vec();
    want.reverse();
    if want.first() != Some(&here) {
        want.insert(0, here);
    }
    assert_eq!(
        x.route, want,
        "the route home starts at the chart cell {here:?}, not the canonical {canonical:?}"
    );
    // Its first segment is as long as the walk it has made, within one block's rounding.
    let first =
        f64::from(x.route[1].0 - x.route[0].0).hypot(f64::from(x.route[1].1 - x.route[0].1));
    let walked = f64::from(here.0 - route[seg].0).hypot(f64::from(here.1 - route[seg].1));
    assert!(
        (first - walked).abs() < 2.0,
        "the first segment is {first:.1} blocks and the walk made is {walked:.1}"
    );
}
