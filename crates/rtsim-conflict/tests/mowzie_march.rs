// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M10 (§4.16, ledgers 23 and 25): the march record that
//! keeps Java's map of every band (BAND_MARCH, MZB-451 to MZB-455), the walk home in bodies, the
//! re-binding of a watcher that walks off, and a band that walks into its fight.

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind};
use rtsim_conflict::engine::{Conflict, Party, Phase, Target, body_id};
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
            t = t + 200;
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
        t = t + 600;
        r.run(t);
        assert!(t < Tick(20 * 24_000), "the siege never reached its target");
    }
    let agent = Party::agent(band);
    let epoch = embody(&mut r, band);
    let mark = r.cmds.len();
    let bored = r.c.parties[band].bored_at;
    while r.c.parties.contains(band) && r.c.parties[band].phase == Phase::Waiting {
        t = t + 100;
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
