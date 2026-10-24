//! A war party lying in wait by a track (M8, MZB-350 to MZB-358): the AMBUSH_SPOT it sends when it
//! gets there, the one removal every end sends, the resend a session start needs, and the trains
//! that pass it (MOWZIE.md §4.4, ledger 8).

use rtsim_conflict::boundary::{self, AMBUSH_REMOVED, AmbushSpot, Cmd};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::rail::AMBUSH_REACH;
use rtsim_conflict::engine::{Conflict, Party, Phase, Target, body_id};
use rtsim_conflict::synth::{self, CAMP, CAMP_POS, PLAYER};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::handle::Handle;
use rtsim_settle::Pos;
use rtsim_world::World;

/// The track point, a hundred blocks east of the den.
fn rail() -> Pos {
    Pos::new(0, CAMP_POS.x + 100, CAMP_POS.z)
}

/// The track as the engine targets it: the spot five blocks toward the den, the rails at y 66.
fn track_target() -> Target {
    let track = rail();
    Target::Track {
        track,
        spot: Pos::new(0, track.x - 5, track.z),
        y: 66,
    }
}

/// A Frostmaw den, with a war party of `kind` ordered to the track and nothing said yet. Every
/// command the engine says goes into `said`, in order.
struct Lurk {
    w: World,
    c: Conflict,
    p: Handle<Party>,
    said: Vec<Cmd>,
}

impl Lurk {
    fn new(seed: u64, kind: PartyKind, target: Target) -> Lurk {
        let (mut w, _) = synth::world(seed);
        let mut c = synth::conflict(seed, &w);
        c.log_enabled = true;
        c.add_faction(&mut w, CAMP, "frostmaw", CAMP_POS, Tick::ZERO)
            .unwrap();
        c.player(&mut w, PLAYER);
        let p = c
            .order_party(&mut w, CAMP, kind, target, None, Tick(10))
            .expect("a band");
        Lurk {
            w,
            c,
            p,
            said: Vec::new(),
        }
    }

    /// A Raid bound for the track.
    fn raid(seed: u64) -> Lurk {
        Lurk::new(seed, PartyKind::Raid, track_target())
    }

    fn run(&mut self, to: Tick) {
        self.c.run_until(&mut self.w, &mut NoInfrastructure, to);
        self.said.extend(self.c.take_commands());
    }

    /// Runs hour by hour until the band lurks, and returns the tick. Nothing was said about a
    /// lurk on the way there.
    fn until_lurking(&mut self) -> Tick {
        let mut t = Tick(10);
        while !self.c.lurking(self.p) {
            t = Tick(t.0 + 1_000);
            assert!(t < Tick::from_days(20), "the band never got there");
            self.run(t);
            if !self.c.lurking(self.p) {
                assert!(self.spots().is_empty(), "a lurk before the arrival");
            }
        }
        t
    }

    /// Runs on, a day at a time, until the band's lurk has ended.
    fn until_over(&mut self) {
        let mut day = 1;
        while self.c.stats.lurks_ended == 0 {
            assert!(day < 80, "the lurk never ended");
            self.run(Tick::from_days(day));
            day += 1;
        }
    }

    fn spots(&self) -> Vec<AmbushSpot> {
        self.said
            .iter()
            .filter_map(|c| match c {
                Cmd::AmbushSpot(a) => Some(*a),
                _ => None,
            })
            .collect()
    }

    fn removals(&self) -> usize {
        self.spots()
            .iter()
            .filter(|a| a.flags & AMBUSH_REMOVED != 0)
            .count()
    }

    /// The AMBUSH_SPOT the band's target gives, at the band's epoch.
    fn want(&self, flags: u32) -> AmbushSpot {
        let Target::Track { track, spot, y } = self.c.parties[self.p].target else {
            panic!("not a track");
        };
        let at = |q: Pos| (f64::from(q.x) + 0.5, f64::from(y), f64::from(q.z) + 0.5);
        AmbushSpot {
            agent: Party::agent(self.p),
            epoch: self.c.parties[self.p].epoch,
            flags,
            dimension: 0,
            faction: CAMP,
            track: at(track),
            spot: at(spot),
            reach: AMBUSH_REACH,
        }
    }

    /// Marks the band as in Minecraft, with every member spawned, as a watched one is.
    fn embody(&mut self) {
        let player = self.c.player(&mut self.w, PLAYER);
        let x = &mut self.c.parties[self.p];
        x.embodied = true;
        x.seen_by = Some(player);
        x.spawned = x.units.clone();
    }
}

#[test]
fn a_band_says_where_it_lies_in_wait_when_it_arrives() {
    for (seed, kind) in [(61, PartyKind::Raid), (62, PartyKind::Invasion)] {
        let mut l = Lurk::new(seed, kind, track_target());
        assert!(!l.c.lurking(l.p), "mustering is not lurking: {kind:?}");
        l.until_lurking();
        assert_eq!(l.c.parties[l.p].phase, Phase::Waiting);
        assert_eq!(
            l.spots(),
            vec![l.want(0)],
            "one AMBUSH_SPOT, flags 0, the track point and the spot as whole blocks' centres, the \
             band's agent and epoch: {kind:?}"
        );
        assert_eq!(l.c.stats.lurks, 1);
        assert_eq!(l.c.stats.lurks_ended, 0);
        assert!(
            l.c.log
                .iter()
                .any(|e| e.kind == "track" && e.what.contains("lies in wait by the track")),
            "{kind:?}"
        );
        // Waiting on, it says nothing more.
        l.said.clear();
        l.run(l.c.now() + 5_000);
        assert!(l.spots().is_empty());
    }
}

/// A raid at a bare position walks out, finds nothing to raid and goes home: no lurk at any step.
#[test]
fn a_band_bound_for_a_bare_position_never_lurks() {
    let mut l = Lurk::new(63, PartyKind::Raid, Target::Pos(rail()));
    let mut t = Tick(10);
    while l.c.parties.contains(l.p) {
        t = Tick(t.0 + 1_000);
        assert!(t < Tick::from_days(20), "the band never came home");
        l.run(t);
        assert!(!l.c.lurking(l.p));
    }
    assert!(l.spots().is_empty(), "no lurk, no record, either way");
    assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (0, 0));
}

#[test]
fn a_band_that_gets_bored_takes_its_lurk_down_once() {
    let mut l = Lurk::raid(64);
    l.until_lurking();
    let first = l.want(0);
    l.until_over();
    // Run on past the walk home: nothing more is said.
    l.run(l.c.now() + Tick::from_days(5).0);
    assert_eq!(l.c.stats.bored, 1, "it waited out its patience");
    assert_eq!(
        l.spots(),
        vec![
            first,
            AmbushSpot {
                flags: AMBUSH_REMOVED,
                ..first
            }
        ],
        "a start and one removal of the same lurk"
    );
    assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (1, 1));
    assert!(
        !l.c.parties.contains(l.p),
        "the band is home and its row gone"
    );
    assert!(!l.c.lurking(l.p), "a handle that is gone is no lurk");
}

/// A band in Minecraft: its removal goes before its recall, so the plugin forgets the spot
/// before it hears the band go.
#[test]
fn the_removal_goes_before_the_recall() {
    let mut l = Lurk::raid(65);
    l.until_lurking();
    l.embody();
    l.said.clear();
    l.until_over();
    let at_remove = l
        .said
        .iter()
        .position(|c| matches!(c, Cmd::AmbushSpot(a) if a.flags & AMBUSH_REMOVED != 0))
        .expect("a removal");
    let at_recall = l
        .said
        .iter()
        .position(|c| matches!(c, Cmd::Recall(r) if r.agent == Party::agent(l.p)))
        .expect("a recall");
    assert!(at_remove < at_recall, "{:?}", l.said);
    assert_eq!(l.removals(), 1);
}

#[test]
fn peace_takes_a_lurk_down_once() {
    let mut l = Lurk::raid(66);
    l.until_lurking();
    let first = l.want(0);
    let t = l.c.now() + 100;
    l.run(t);
    l.c.set_difficulty(&mut l.w, boundary::DIFFICULTY_PEACEFUL);
    l.said.extend(l.c.take_commands());
    assert!(!l.c.lurking(l.p), "going home is no lurk");
    assert_eq!(l.removals(), 1);
    // Its walk home, and the patience it had left, end nothing a second time.
    l.run(Tick::from_days(40));
    assert_eq!(
        l.spots(),
        vec![
            first,
            AmbushSpot {
                flags: AMBUSH_REMOVED,
                ..first
            }
        ]
    );
    assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (1, 1));
}

/// Every member killed while it waits (CL-D61 routs it): the wipe-out goes home through the same
/// door, so the lurk ends there, once, and before the recall.
#[test]
fn a_band_wiped_out_at_its_track_takes_its_lurk_down_once() {
    let mut l = Lurk::raid(67);
    l.until_lurking();
    l.embody();
    let first = l.want(0);
    l.said.clear();
    let at = l.c.now() + 100;
    let goblins =
        l.c.cultures
            .get(l.c.cultures.index_of("frostmaw").unwrap())
            .clone();
    for (u, count) in l.c.parties[l.p].units.clone() {
        let body = body_id(&l.c.cultures, &goblins.unit(u).body);
        l.c.ingest(
            &mut l.w,
            &mut NoInfrastructure,
            &rtsim_conflict::boundary::Obs::Casualties(boundary::BandCasualties {
                agent: Party::agent(l.p),
                epoch: first.epoch,
                by: boundary::BY_MOB,
                body,
                count,
                killer: (0, 0),
            }),
            at,
        );
    }
    l.said.extend(l.c.take_commands());
    assert_eq!(l.c.parties[l.p].alive(), 0);
    assert_eq!(l.c.parties[l.p].phase, Phase::Home);
    assert_eq!(
        l.spots(),
        vec![AmbushSpot {
            flags: AMBUSH_REMOVED,
            ..first
        }]
    );
    let at_recall = l
        .said
        .iter()
        .position(|c| matches!(c, Cmd::Recall(_)))
        .expect("a recall");
    let at_remove = l
        .said
        .iter()
        .position(|c| matches!(c, Cmd::AmbushSpot(_)))
        .unwrap();
    assert!(at_remove < at_recall);
    // The walk home and the end of its patience (the wipe-out's boredom event) say nothing more.
    l.run(Tick::from_days(40));
    assert_eq!(l.removals(), 1);
    assert_eq!(l.c.stats.lurks_ended, 1);
}

/// `session_start` drops the agent commands and Java's table lives for one session: the lurk
/// goes again from `resend_notices`, once however often it is called, and at the band's epoch of
/// now. A queued one (a replayed record made it again) is replaced, not doubled.
#[test]
fn a_session_forgets_the_lurk_and_the_resend_says_it_once() {
    let mut l = Lurk::raid(68);
    l.until_lurking();
    l.said.clear();
    l.c.session_start();
    l.c.resend_notices();
    let again = l.c.take_commands();
    let spots: Vec<AmbushSpot> = again
        .iter()
        .filter_map(|c| match c {
            Cmd::AmbushSpot(a) => Some(*a),
            _ => None,
        })
        .collect();
    assert_eq!(spots, vec![l.want(0)], "one, flags 0");
    // The resend of the resend: a queued lurk is replaced.
    l.c.resend_notices();
    l.c.resend_lurks();
    let twice = l.c.take_commands();
    assert_eq!(
        twice
            .iter()
            .filter(|c| matches!(c, Cmd::AmbushSpot(_)))
            .count(),
        1
    );
    // A session start between a start and a resend drops the queued one first.
    l.c.resend_lurks();
    l.c.session_start();
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::AmbushSpot(_)))
    );
    // The stats count a lurk once, however often it is said.
    assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (1, 0));
}

/// A band that is not lurking is not resent: not one out on the road, nor one home.
#[test]
fn only_a_lurking_band_is_resent() {
    let mut l = Lurk::raid(69);
    // Out on the road.
    l.run(Tick(1_000));
    assert!(!l.c.lurking(l.p));
    l.said.clear();
    l.c.session_start();
    l.c.resend_notices();
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::AmbushSpot(_)))
    );
    // Gone home.
    l.until_lurking();
    l.until_over();
    l.run(Tick::from_days(40));
    l.c.session_start();
    l.c.resend_notices();
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::AmbushSpot(_)))
    );
}

/// A watched lurk fights on at its track: it is still a lurk, and a session that ends the watch
/// turns the fight back to waiting without ending the lurk. (The engine reaches no Fighting with
/// a Track target by itself yet, so the phase is set here: `lurking` is written for it all the
/// same, MZB-351.)
#[test]
fn a_fight_at_the_track_is_still_a_lurk_and_a_session_turns_it_back_to_waiting() {
    let mut l = Lurk::raid(70);
    l.until_lurking();
    l.said.clear();
    l.c.parties[l.p].phase = Phase::Fighting;
    assert!(l.c.lurking(l.p));
    l.c.session_start();
    assert_eq!(l.c.parties[l.p].phase, Phase::Waiting);
    assert!(l.c.lurking(l.p), "a recall does not end a lurk");
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::AmbushSpot(_))),
        "and says no removal"
    );
    assert_eq!(l.c.stats.lurks_ended, 0);
    // Resent as it was.
    l.c.resend_lurks();
    assert_eq!(
        l.c.take_commands()
            .iter()
            .filter(|c| matches!(c, Cmd::AmbushSpot(a) if a.flags == 0))
            .count(),
        1
    );
}

/// A band home with loot its haven cannot take keeps its row (never, by construction: the haven
/// is a storehouse). The row's target is the track point as a bare position, so a kept row never
/// reads as a lurk and nothing resends it (MZB-354). Its removal went at the turn.
#[test]
fn a_kept_row_never_reads_as_a_lurk() {
    let mut l = Lurk::raid(71);
    l.until_lurking();
    // Loot the world holds nothing of: the haven stocks none of it, so the row stays.
    let g = l.w.goods.iter().next().expect("a good").0;
    l.c.parties[l.p].loot.insert(g, 1_000);
    l.until_over();
    l.run(Tick::from_days(40));
    assert!(l.c.parties.contains(l.p), "the row is kept");
    let x = &l.c.parties[l.p];
    assert_eq!(x.target, Target::Pos(rail()));
    assert_eq!(x.phase, Phase::Waiting);
    assert!(!l.c.lurking(l.p));
    assert_eq!(l.removals(), 1, "the removal went at the turn, once");
    l.said.clear();
    l.c.session_start();
    l.c.resend_notices();
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::AmbushSpot(_)))
    );
}
