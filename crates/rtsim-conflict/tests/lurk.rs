//! A war party lying in wait by a track (M8, MZB-350 to MZB-358): the AMBUSH_SPOT it sends when it
//! gets there, the one removal every end sends, the resend a session start needs, and the trains
//! that pass it (MOWZIE.md §4.4, ledger 8).

use rtsim_conflict::boundary::{
    self, AMBUSH_REMOVED, AmbushSpot, Cmd, Obs, PASSED_WATCHED, TrainPassed, TrainRobbed,
};
use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::rail::AMBUSH_REACH;
use rtsim_conflict::engine::{Conflict, Party, Phase, Target, body_id};
use rtsim_conflict::synth::{self, CAMP, CAMP_POS, PLAYER};
use rtsim_ecology::infra::{
    DistrictIndex, DistrictSite, IndustryLink, Infrastructure, NoInfrastructure, RaidOutcome,
    TrainSighting,
};
use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
use rtsim_industry::obs::{Carriage, Obs as IndObs, StationDecl, Timed, TrainDeparture};
use rtsim_industry::threat::{Threat, ThreatTarget};
use rtsim_industry::tunables::Tunables;
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

// ---- the lurk's end by the chief's fall or by Peaceful, in a fight at the track (MZB-352) --------

/// The chief falls while the band waits at its track: the rout goes home through the same door.
#[test]
fn the_chiefs_fall_takes_a_lurk_down_once() {
    let mut l = Lurk::raid(72);
    l.until_lurking();
    let first = l.want(0);
    assert!(l.c.parties[l.p].chief, "a Frostmaw band has its chief");
    l.said.clear();
    let at = l.c.now() + 50;
    l.c.chief_killed(&mut l.w, l.p, None, at);
    l.said.extend(l.c.take_commands());
    assert_eq!(l.c.parties[l.p].phase, Phase::Home);
    assert_eq!(
        l.spots(),
        vec![AmbushSpot {
            flags: AMBUSH_REMOVED,
            ..first
        }]
    );
    l.run(Tick::from_days(40));
    assert_eq!(l.removals(), 1);
    assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (1, 1));
}

/// The two ends of a fight at the track (`end_fight`, which `go_home` follows): the chief's fall
/// and Peaceful. The engine reaches no Fighting at a track by itself (MZB-351), so the phase is
/// set; the lurk is still a lurk while it is set, and ends once.
#[test]
fn a_fight_at_the_track_ends_its_lurk_once_whatever_ends_it() {
    for (seed, peace) in [(73, false), (74, true)] {
        let mut l = Lurk::raid(seed);
        l.until_lurking();
        let first = l.want(0);
        l.said.clear();
        l.c.parties[l.p].phase = Phase::Fighting;
        l.embody();
        let at = l.c.now() + 50;
        if peace {
            l.c.set_difficulty(&mut l.w, boundary::DIFFICULTY_PEACEFUL);
        } else {
            l.c.chief_killed(&mut l.w, l.p, None, at);
        }
        l.said.extend(l.c.take_commands());
        assert_eq!(l.c.parties[l.p].phase, Phase::Home, "peace: {peace}");
        assert_eq!(
            l.spots(),
            vec![AmbushSpot {
                flags: AMBUSH_REMOVED,
                ..first
            }],
            "peace: {peace}"
        );
        l.run(Tick::from_days(40));
        assert_eq!(l.removals(), 1, "peace: {peace}");
        assert_eq!((l.c.stats.lurks, l.c.stats.lurks_ended), (1, 1));
    }
}

// ---- trains that pass a lurk (M8, MZB-355, MZB-356) ----------------------------------------------

/// An infrastructure for ambushes: an attack on a train loses `loot` of good 1 (`None` is an
/// attack the industry skips), named for a good the world knows; every leg a band walks crosses
/// the train `crossing`, if any. Records each threat.
struct Rails {
    name: String,
    loot: Option<i64>,
    crossing: Option<u64>,
    threats: Vec<Threat>,
}

impl Rails {
    fn new(w: &World, loot: Option<i64>) -> Rails {
        let (_, def) = w.goods.iter().next().expect("a good");
        Rails {
            name: def.name.clone(),
            loot,
            crossing: None,
            threats: Vec::new(),
        }
    }
}

impl Infrastructure for Rails {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }

    fn good_name(&self, _: Good) -> Option<String> {
        Some(self.name.clone())
    }

    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainSighting> {
        self.crossing
            .map(|t| TrainSighting {
                train: TrainId(t),
                from,
                to,
            })
            .into_iter()
            .collect()
    }

    fn raid_train(&mut self, t: &Threat) -> Option<GoodVec> {
        self.threats.push(*t);
        self.loot.map(|n| GoodVec::from_pairs([(Good(1), n)]))
    }
}

impl Lurk {
    /// A TRAIN_PASSED of `train` for the band, at its epoch, `after` ticks from now.
    fn pass(&mut self, infra: &mut dyn Infrastructure, train: u64, flags: u32, after: u64) {
        let tp = TrainPassed {
            agent: Party::agent(self.p),
            epoch: self.c.parties[self.p].epoch,
            flags,
            carriage: 0,
            train,
        };
        self.pass_of(infra, tp, after);
    }

    fn pass_of(&mut self, infra: &mut dyn Infrastructure, tp: TrainPassed, after: u64) {
        let at = self.c.now() + after;
        self.c.ingest(&mut self.w, infra, &Obs::TrainPassed(tp), at);
        self.said.extend(self.c.take_commands());
    }

    /// The trains the plugin was told were robbed, in order.
    fn robbed(&self) -> Vec<u64> {
        self.said
            .iter()
            .filter_map(|c| match c {
                Cmd::TrainRobbed(t) => {
                    assert_eq!(t.faction, CAMP);
                    Some(t.train)
                }
                _ => None,
            })
            .collect()
    }

    fn reports(&self) -> usize {
        self.w.knowledge.reports().iter().count()
    }
}

#[test]
fn a_train_passed_is_no_longer_deferred() {
    let o = Obs::TrainPassed(TrainPassed {
        agent: 1,
        epoch: 0,
        flags: 0,
        carriage: 0,
        train: 1,
    });
    assert!(!Conflict::later(&o));
}

/// A pass rolls: a hit raids the train (one threat a class) and sends TRAIN_ROBBED; a miss sends
/// nothing.
#[test]
fn a_train_passed_rolls_and_robs() {
    let mut l = Lurk::raid(75);
    let mut rails = Rails::new(&l.w, Some(3));
    l.until_lurking();
    l.said.clear();
    l.c.tun.ambush_milli = 1000;
    l.pass(&mut rails, 7, 0, 10);
    assert_eq!((l.c.stats.passes, l.c.stats.passes_watched), (1, 0));
    assert_eq!(l.c.stats.ambushes, 1);
    assert_eq!(
        l.said
            .iter()
            .filter_map(|c| match c {
                Cmd::TrainRobbed(t) => Some(*t),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![TrainRobbed {
            faction: CAMP,
            train: 7
        }]
    );
    assert!(!rails.threats.is_empty());
    for t in &rails.threats {
        assert_eq!(t.target, ThreatTarget::Train(TrainId(7)));
        assert_eq!(t.at, l.c.now());
    }
    let items = l.c.stats.items_from_districts;
    assert!(items > 0);
    assert!(
        l.c.log.iter().any(|e| e.kind == "ambush"
            && e.what == format!("falls on train 7 by the track: {items} items")),
        "the note says by the track"
    );
    l.c.check(&l.w).expect("books");

    // A miss: the roll is the only thing between a pass and a threat.
    let asked = rails.threats.len();
    l.c.tun.ambush_milli = 0;
    l.pass(&mut rails, 8, 0, 10);
    assert_eq!(l.c.stats.passes, 2);
    assert_eq!(l.c.stats.ambushes, 1);
    assert_eq!(rails.threats.len(), asked, "a miss asks nothing");
    assert_eq!(l.robbed(), vec![7]);
}

/// A robbery of nothing sends no TRAIN_ROBBED: the industry skipped the attack (its cooldown), the
/// attack found no cargo, or what it found is not a good of the world's.
#[test]
fn a_robbery_of_nothing_sends_no_train_robbed() {
    // Skipped: no loot at all. Asked, not counted as an ambush.
    let mut l = Lurk::raid(76);
    let mut rails = Rails::new(&l.w, None);
    l.until_lurking();
    l.c.tun.ambush_milli = 1000;
    l.pass(&mut rails, 7, 0, 10);
    assert!(!rails.threats.is_empty(), "the industry was asked");
    assert_eq!((l.c.stats.passes, l.c.stats.ambushes), (1, 0));
    assert!(l.robbed().is_empty());

    // A hit that found an empty cargo.
    let mut l = Lurk::raid(77);
    let mut rails = Rails::new(&l.w, Some(0));
    l.until_lurking();
    l.c.tun.ambush_milli = 1000;
    l.pass(&mut rails, 7, 0, 10);
    assert_eq!(l.c.stats.ambushes, 1, "an attack that found nothing is one");
    assert!(l.robbed().is_empty());

    // A good the world has no name for goes to the unmapped ledger and counts for nothing.
    let mut l = Lurk::raid(78);
    let mut rails = Rails::new(&l.w, Some(5));
    rails.name = "mic_rtsim:not_a_good".to_string();
    l.until_lurking();
    l.c.tun.ambush_milli = 1000;
    l.pass(&mut rails, 7, 0, 10);
    assert_eq!(l.c.stats.ambushes, 1);
    assert!(l.robbed().is_empty(), "nothing the world knows was taken");
    assert!(l.c.ledger.unmapped.contains_key("mic_rtsim:not_a_good"));
}

/// A TRAIN_PASSED is counted stale when no party answers it: a pass for an agent that is not a
/// party, one for a band still on its way, one from an epoch later than the band's, and one for a
/// band that has gone home. One from an earlier epoch (a watch bumped the band's) still counts.
#[test]
fn a_stale_train_passed_is_counted() {
    let mut rails = Rails::new(&synth::world(79).0, Some(3));
    let mut l = Lurk::raid(79);
    l.c.tun.ambush_milli = 1000;
    let stale = |l: &Lurk| l.c.stats.stale;
    let tp = |l: &Lurk| TrainPassed {
        agent: Party::agent(l.p),
        epoch: l.c.parties[l.p].epoch,
        flags: 0,
        carriage: 0,
        train: 7,
    };

    // On its way: not lurking yet.
    l.run(Tick(1_000));
    assert!(!l.c.lurking(l.p));
    let before = stale(&l);
    let on_way = tp(&l);
    l.pass_of(&mut rails, on_way, 5);
    assert_eq!(stale(&l), before + 1);

    l.until_lurking();
    // An agent that is not a party.
    let before = stale(&l);
    let nobody = TrainPassed {
        agent: u64::MAX,
        ..tp(&l)
    };
    l.pass_of(&mut rails, nobody, 5);
    assert_eq!(stale(&l), before + 1);
    // An epoch later than the band's.
    let before = stale(&l);
    let future = TrainPassed {
        epoch: l.c.parties[l.p].epoch + 1,
        ..tp(&l)
    };
    l.pass_of(&mut rails, future, 5);
    assert_eq!(stale(&l), before + 1);
    assert_eq!(l.c.stats.passes, 0);
    assert!(rails.threats.is_empty(), "no stale pass is rolled");

    // Earlier than the band's epoch: a watch bumped it since the spot was sent. Counted.
    let old = tp(&l);
    l.c.parties[l.p].epoch += 1;
    let before = stale(&l);
    l.pass_of(&mut rails, old, 5);
    assert_eq!(stale(&l), before);
    assert_eq!(l.c.stats.passes, 1);
    assert!(!rails.threats.is_empty());

    // Gone home.
    l.until_over();
    l.run(l.c.now() + Tick::from_days(5).0);
    assert!(!l.c.parties.contains(l.p));
    let before = stale(&l);
    l.pass_of(&mut rails, old, 5);
    assert_eq!(stale(&l), before + 1);
    assert_eq!(l.c.stats.passes, 1);
}

/// Peaceful sends the band home; a pass after that finds no lurk.
#[test]
fn a_pass_after_peace_is_stale() {
    let mut l = Lurk::raid(80);
    let mut rails = Rails::new(&l.w, Some(3));
    l.until_lurking();
    let old = TrainPassed {
        agent: Party::agent(l.p),
        epoch: l.c.parties[l.p].epoch,
        flags: 0,
        carriage: 0,
        train: 7,
    };
    l.c.set_difficulty(&mut l.w, boundary::DIFFICULTY_PEACEFUL);
    let before = l.c.stats.stale;
    l.pass_of(&mut rails, old, 5);
    assert_eq!(l.c.stats.stale, before + 1);
    assert_eq!(l.c.stats.passes, 0);
    assert!(l.robbed().is_empty());
}

/// A watched pass rolls as an unwatched one does (P28): the same world, the same trains, the same
/// times, with and without the flag, lose the same trains; and the roll is a real mix at 500.
#[test]
fn a_watched_pass_rolls_as_an_unwatched_one() {
    let run = |flags: u32| {
        let mut l = Lurk::raid(81);
        let mut rails = Rails::new(&l.w, Some(3));
        l.until_lurking();
        for train in 0..60 {
            l.pass(&mut rails, 1_000 + train, flags, 10);
        }
        (l.robbed(), l.c.stats.passes, l.c.stats.passes_watched)
    };
    let (quiet, passes, watched) = run(0);
    assert_eq!((passes, watched), (60, 0));
    let (seen, passes, watched) = run(PASSED_WATCHED);
    assert_eq!((passes, watched), (60, 60));
    assert_eq!(quiet, seen, "the flag changes no roll");
    assert!(
        (15..=45).contains(&quiet.len()),
        "about half of 60 at 500: {}",
        quiet.len()
    );
}

/// The industry's cooldown (I25: 24,000 ticks per train per class) is what holds a train to one
/// robbery a day, and a lurk asks it each time: the same industry engine, two trains.
#[test]
fn the_cooldown_bounds_a_train_to_one_robbery_a_day() {
    const WEST: StationId = StationId(10);
    const EAST: StationId = StationId(11);
    let mut l = Lurk::raid(82);
    let name = l.w.goods.iter().next().expect("a good").1.name.clone();
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(Good(1), GoodInfo::item(&name, 64));
    for (station, district, x) in [(WEST, DistrictId(1), 0), (EAST, DistrictId(2), 4000)] {
        e.ingest(&Timed {
            at: Tick(0),
            obs: IndObs::Station(StationDecl {
                station,
                district: Some(district),
                dim: 0,
                pos: BlockPos::new(x, 64, 0),
                name: 0,
            }),
        });
    }
    // Two trains on long legs from the west station, 640 of good 1 each.
    for train in [7, 8] {
        e.ingest(&Timed {
            at: Tick(100),
            obs: IndObs::TrainDeparted(TrainDeparture {
                train: TrainId(train),
                station: WEST,
                district: DistrictId(1),
                destination: Some(EAST),
                eta_ticks: 400_000,
                carriages: vec![Carriage {
                    index: 0,
                    item_slots: 36,
                    fluid_capacity: 0,
                    goods: GoodVec::from_pairs([(Good(1), 640)]),
                }],
            }),
        });
    }
    let mut index = DistrictIndex::default();
    l.until_lurking();
    l.c.tun.ambush_milli = 1000;
    l.said.clear();
    let mut pass = |l: &mut Lurk, e: &mut Engine, train: u64, after: u64| {
        let mut link = IndustryLink {
            engine: e,
            index: &mut index,
        };
        l.pass(&mut link, train, 0, after);
    };

    pass(&mut l, &mut e, 7, 10);
    let first = l.c.now();
    assert_eq!(l.robbed(), vec![7], "the first pass of train 7 robs it");
    let taken = l.c.stats.items_from_districts;
    assert!(
        (1..=64).contains(&taken),
        "at most a tenth of 640, at most 64 carried: {taken}"
    );
    // Train 8 has a cooldown of its own.
    pass(&mut l, &mut e, 8, 10);
    assert_eq!(l.robbed(), vec![7, 8]);
    // Train 7 again, inside the day: the industry skips the attack.
    let skipped = e.stats.attacks_skipped;
    for after in [1_000, 5_000, 10_000] {
        pass(&mut l, &mut e, 7, after);
    }
    assert!(l.c.now().0 < first.0 + 24_000, "still inside the day");
    assert!(e.stats.attacks_skipped > skipped, "the industry said no");
    assert_eq!(l.robbed(), vec![7, 8], "and nothing was robbed");
    assert_eq!(l.c.stats.ambushes, 2);
    // A day after the first robbery of train 7 it can be robbed again.
    let wait = first.0 + 24_000 - l.c.now().0;
    pass(&mut l, &mut e, 7, wait + 10);
    assert_eq!(l.robbed(), vec![7, 8, 7]);
    assert_eq!(l.c.stats.ambushes, 3);
    l.c.check(&l.w).expect("books");
}

/// An ambush makes no news (MZB-358): nobody saw an unwatched one, and a watched one is seen in
/// the world by its watcher; the engine books no report for either.
#[test]
fn an_ambush_makes_no_news() {
    let mut l = Lurk::raid(83);
    let mut rails = Rails::new(&l.w, Some(3));
    l.until_lurking();
    l.c.tun.ambush_milli = 1000;
    let before = l.reports();
    l.pass(&mut rails, 7, 0, 10);
    l.pass(&mut rails, 8, PASSED_WATCHED, 10);
    assert_eq!(l.robbed(), vec![7, 8]);
    assert_eq!(l.reports(), before, "no report from either");
}

/// An ambush on the march keeps its old wording; one at a track says where it was.
#[test]
fn the_on_the_way_note_keeps_its_wording() {
    let mut l = Lurk::new(84, PartyKind::Raid, Target::Pos(rail()));
    let mut rails = Rails::new(&l.w, Some(3));
    rails.crossing = Some(9);
    l.c.tun.ambush_milli = 1000;
    let mut t = Tick(10);
    while l.c.stats.ambushes == 0 {
        t = Tick(t.0 + 1_000);
        assert!(t < Tick::from_days(20), "no ambush on the march");
        l.c.run_until(&mut l.w, &mut rails, t);
    }
    let items = l.c.stats.items_from_districts;
    assert!(
        l.c.log.iter().any(|e| e.kind == "ambush"
            && e.what == format!("falls on train 9 on the way: {items} items")),
        "{:?}",
        l.c.log
            .iter()
            .filter(|e| e.kind == "ambush")
            .collect::<Vec<_>>()
    );
    assert!(
        l.c.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::TrainRobbed(_))),
        "a band on the march tells no roar to anyone"
    );
}

// ---- a lurk across a save (M8, run 8) ------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Trip {
    Never,
    Blob,
    Paged,
}

/// A band lurks, trains pass it, the engine is saved and loaded (or not) in the middle of the
/// wait, and the lurk goes on to its end: what the plugin heard, the stats and the state hash.
fn lurk_run(seed: u64, trip: Trip) -> (Vec<AmbushSpot>, Vec<u64>, [u64; 6], u64) {
    use rtsim_conflict::scenario::META_KEY;
    use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
    use std::sync::{Arc, RwLock};
    let mut l = Lurk::raid(seed);
    let dir = std::env::temp_dir().join(format!("rtsim-lurk-{seed}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Arc::new(RwLock::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    if trip == Trip::Paged {
        l.c.attach_store(store.clone(), 8 << 10);
    }
    let mut rails = Rails::new(&l.w, Some(3));
    l.until_lurking();
    for train in 0..6 {
        l.pass(&mut rails, 2_000 + train, (train % 2) as u32, 20);
    }
    assert!(l.c.lurking(l.p) && l.c.stats.passes == 6);
    match trip {
        Trip::Never => {}
        Trip::Blob => {
            let bytes = l.c.save();
            l.c = Conflict::load(l.c.tun.clone(), l.c.cultures.clone(), &bytes).expect("loads");
            assert_eq!(l.c.save(), bytes, "the same bytes");
        }
        Trip::Paged => {
            let mut batch = SaveBatch::new();
            let t = l.c.save_paged(&mut batch, META_KEY);
            store.write().unwrap().commit(batch, 0).unwrap();
            l.c.saved(t);
            l.c.enforce_budget();
            let (tun, cultures, hash) = (l.c.tun.clone(), l.c.cultures.clone(), l.c.state_hash());
            let c = std::mem::replace(
                &mut l.c,
                Conflict::new(1, tun.clone(), cultures.clone(), Tick::ZERO),
            );
            drop(c);
            l.c = Conflict::open_paged(store.clone(), 8 << 10, META_KEY, tun, cultures)
                .unwrap()
                .expect("a meta page");
            assert_eq!(l.c.state_hash(), hash, "the same state at once");
        }
    }
    // A lurk is derived from the saved phase, kind and target.
    assert!(l.c.lurking(l.p), "{trip:?}");
    // Trains go on passing it, then it gets bored.
    for train in 6..12 {
        l.pass(&mut rails, 2_000 + train, (train % 2) as u32, 20);
    }
    l.until_over();
    l.run(l.c.now() + Tick::from_days(5).0);
    let s = &l.c.stats;
    let end = (
        l.spots(),
        l.robbed(),
        [
            s.lurks,
            s.lurks_ended,
            s.passes,
            s.passes_watched,
            s.ambushes,
            s.stale,
        ],
        l.c.state_hash(),
    );
    let _ = std::fs::remove_dir_all(&dir);
    end
}

#[test]
fn a_lurk_survives_a_save_mid_wait_blob_and_paged() {
    let whole = lurk_run(85, Trip::Never);
    assert_eq!(whole.0.len(), 2, "a start and a removal: {:?}", whole.0);
    assert_eq!(whole.2[..4], [1, 1, 12, 6]);
    assert!(!whole.1.is_empty(), "some of twelve passes robbed");
    assert_eq!(lurk_run(85, Trip::Blob), whole, "blob");
    assert_eq!(lurk_run(85, Trip::Paged), whole, "paged");
}
