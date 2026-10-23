// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M1 (§9): the world's difficulty (no war party leaves on
//! Peaceful, §4.12), a feud's notice kept until the plugin plays it (NOTICE_SHOWN, resent at each
//! session start, across a restart), and an envoy party wiped out with no player among its
//! killers withdrawing its demand (DEMAND_SETTLED outcome 5, ledger 14). And the reports the
//! conflict records on the knowledge layer's severity unit (MZB-36).

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, Demand, Party, Phase, Signal, Target, body_id, item_of};
use rtsim_conflict::synth::{self, Answer, FakeConflict};
use rtsim_conflict::{ConflictTunables, Cultures, Step};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::{ActorId, ReportKind, SEVERITY_UNIT};
use rtsim_settle::NodeKind;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// Goblins that never plunder, so a quiet feud is all there is.
fn quiet_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// The goblin camp, the player, the fake plugin and every command the engine sent, with when.
struct Run {
    w: World,
    c: Conflict,
    f: u32,
    p: ActorId,
    plugin: FakeConflict,
    cmds: Vec<(Tick, Cmd)>,
    /// The player stands at the refinery (observed every 1,200 ticks) while this is set.
    watch: bool,
    next_obs: Tick,
}

impl Run {
    fn new(seed: u64, cultures: Cultures) -> Run {
        let (mut w, _) = synth::world(seed);
        let mut c = Conflict::new(seed, ConflictTunables::default(), cultures, Tick::ZERO);
        synth::define_names(&mut c, &w);
        let f = c
            .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
            .unwrap();
        let p = c.player(&mut w, synth::PLAYER);
        Run {
            w,
            c,
            f,
            p,
            plugin: FakeConflict::new(synth::PLAYER),
            cmds: Vec::new(),
            watch: false,
            next_obs: Tick::ZERO,
        }
    }

    /// A killing by the player at the refinery, `n` members.
    fn kill(&mut self, n: f64, at: Tick) {
        let sig = Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount: n,
            actor: self.p,
            pos: synth::REFINERY_POS,
            district: Some(synth::REFINERY),
        };
        self.c.signal(&mut self.w, sig, at);
    }

    fn ingest(&mut self, o: Obs, at: Tick) {
        self.c.ingest(&mut self.w, &mut NoInfrastructure, &o, at);
    }

    /// The engine, the plugin and the player to `to`.
    fn run(&mut self, to: Tick) {
        let mut t = self.c.now();
        while t < to {
            let next = [
                self.c.next_due(),
                self.plugin.next_due(),
                self.watch.then_some(self.next_obs),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(to)
            .clamp(t + 1, to);
            for (at, o) in self.plugin.due(next) {
                self.c.ingest(&mut self.w, &mut NoInfrastructure, &o, at);
            }
            self.c.run_until(&mut self.w, &mut NoInfrastructure, next);
            if self.watch && next >= self.next_obs {
                self.c
                    .observe_player(&mut self.w, synth::PLAYER, synth::REFINERY_POS, next);
                self.next_obs = next + 1_200;
            }
            self.flush(next);
            t = next;
        }
    }

    /// What the engine decided, to the plugin.
    fn flush(&mut self, at: Tick) {
        let cmds = self.c.take_commands();
        self.cmds.extend(cmds.iter().cloned().map(|c| (at, c)));
        self.plugin.apply(cmds, at);
    }

    fn step(&self) -> Step {
        self.c.feud(self.f, self.p).unwrap().step
    }

    fn war_parties(&self) -> u64 {
        [
            PartyKind::Raid,
            PartyKind::Invasion,
            PartyKind::Siege,
            PartyKind::Plunder,
        ]
        .iter()
        .map(|k| self.c.stats.parties[k.index()])
        .sum()
    }

    fn notices_since(&self, from: usize) -> Vec<boundary::Notice> {
        self.cmds[from..]
            .iter()
            .filter_map(|(_, c)| match c {
                Cmd::Notice(n) => Some(*n),
                _ => None,
            })
            .collect()
    }
}

fn difficulty(d: u32) -> Obs {
    Obs::Difficulty(boundary::Difficulty { difficulty: d })
}

/// Twenty killings every two days from `from` to `to`, the run going on between.
fn keep_killing(r: &mut Run, from: f64, to: f64) {
    let mut d = from;
    while d < to {
        let at = r.c.now().max(day(d));
        r.kill(20.0, at);
        d += 2.0;
        r.run(day(d.min(to)));
    }
}

/// MOWZIE.md §4.12: on Peaceful no war party and no envoy leaves (MZB-132). A feud kept far past
/// the raid rung for twenty days holds at its notice, its grievance kept; once the world is not
/// Peaceful the feud climbs on (every feud planned again): an envoy goes, and a raid sets out.
#[test]
fn on_peaceful_no_war_party_leaves_and_the_climb_resumes_after() {
    let mut r = Run::new(31, quiet_goblins());
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(1));
    assert!(r.c.peaceful);
    keep_killing(&mut r, 0.0, 20.0);
    assert_eq!(r.war_parties(), 0, "no war party on Peaceful");
    assert_eq!(r.step(), Step::Notice, "held at its notice");
    assert_eq!(r.c.stats.demands, 0, "no envoy on Peaceful");
    assert_eq!(r.c.stats.parties[PartyKind::Envoy.index()], 0);
    assert!(r.c.feud(r.f, r.p).unwrap().demand.is_none());
    let g = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert!(
        g > 70.0,
        "the grievance is kept, and past the raid's 70: {g}"
    );
    // A save keeps the flag.
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert!(back.peaceful);
    assert_eq!(back.state_hash(), r.c.state_hash());
    // Normal again: the held feud climbs, the envoy first.
    let at = r.c.now();
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), at);
    assert!(!r.c.peaceful);
    keep_killing(&mut r, 20.0, 34.0);
    assert!(
        r.c.stats.demands >= 1,
        "the envoy goes once it is not Peaceful"
    );
    assert!(
        r.war_parties() >= 1,
        "a raid leaves once it is not Peaceful"
    );
    assert!(r.step() >= Step::Raid, "{:?}", r.step());
}

/// A war party whose muster ends on Peaceful stands down: it never leaves, its feud steps back
/// to the rung below, and the party is gone. Plunder does not leave either.
#[test]
fn a_war_party_mustering_when_peaceful_begins_stands_down() {
    let mut r = Run::new(32, quiet_goblins());
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(30));
    r.run(day(2.0));
    assert!(!r.c.parties.contains(band), "the raid stood down");
    assert_eq!(r.c.stats.peace_held, 1);
    assert_eq!(r.step(), Step::Scouts, "a rung below the raid it served");
    // The built-in goblins plunder; on Peaceful they do not.
    let mut r = Run::new(33, Cultures::builtin());
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(1));
    r.run(day(40.0));
    assert_eq!(r.c.stats.parties[PartyKind::Plunder.index()], 0);
    assert!(r.c.stats.peace_held > 0, "the plunder drive tried and held");
}

/// MOWZIE.md §4.12: a notice is never lost. Unplayed, it is kept with its feud and sent again at
/// every session start (after the replay, `resend_notices`), a restart included; NOTICE_SHOWN for
/// another serial changes nothing, for its own it ends the resending.
#[test]
fn a_notice_is_kept_until_shown_and_resent_at_each_session_start() {
    let mut r = Run::new(34, quiet_goblins());
    r.plugin.play_notices = false;
    r.kill(2.0, Tick(10));
    r.run(day(0.25));
    assert_eq!(r.step(), Step::Notice);
    let sent = r.notices_since(0);
    assert_eq!(sent.len(), 1, "one notice");
    let n = sent[0];
    assert_eq!(n.player, synth::PLAYER);
    assert_eq!(r.c.kept_notices(), vec![n], "kept, unplayed");
    // A session start sends it again, after the faction's territory.
    let mark = r.cmds.len();
    r.c.session_start();
    r.c.resend_notices();
    r.flush(r.c.now());
    assert_eq!(r.notices_since(mark), vec![n]);
    let territory = r.cmds[mark..]
        .iter()
        .position(|(_, c)| matches!(c, Cmd::Territory(_)))
        .unwrap();
    let notice = r.cmds[mark..]
        .iter()
        .position(|(_, c)| matches!(c, Cmd::Notice(_)))
        .unwrap();
    assert!(territory < notice);
    // A restart keeps it.
    r.c = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(r.c.kept_notices(), vec![n]);
    let mark = r.cmds.len();
    r.c.session_start();
    assert!(r.notices_since(mark).is_empty(), "not before the replay");
    r.c.resend_notices();
    r.flush(r.c.now());
    assert_eq!(r.notices_since(mark), vec![n], "again after the restart");
    assert_eq!(n.serial, 1, "the feud's first notice");
    // Shown for another serial: kept.
    let at = r.c.now() + 5;
    let shown = |serial| {
        Obs::NoticeShown(boundary::NoticeShown {
            faction: synth::CAMP,
            serial,
            player: synth::PLAYER,
        })
    };
    r.ingest(shown(2), at);
    assert_eq!(r.c.kept_notices(), vec![n]);
    // A stranger's NOTICE_SHOWN, or one for a camp that is not there: nothing.
    r.ingest(
        Obs::NoticeShown(boundary::NoticeShown {
            faction: synth::CAMP,
            serial: n.serial,
            player: (7, 7),
        }),
        at,
    );
    r.ingest(
        Obs::NoticeShown(boundary::NoticeShown {
            faction: 0xDEAD,
            serial: n.serial,
            player: synth::PLAYER,
        }),
        at,
    );
    assert_eq!(r.c.kept_notices(), vec![n]);
    // Shown: never again.
    r.ingest(shown(n.serial), at);
    assert!(r.c.kept_notices().is_empty());
    assert_eq!(r.c.stats.notices_shown, 1);
    let mark = r.cmds.len();
    r.c.session_start();
    r.c.resend_notices();
    r.flush(r.c.now());
    assert!(r.notices_since(mark).is_empty(), "not resent once shown");
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert!(back.kept_notices().is_empty());
}

/// The fake plugin plays its notices: the engine keeps nothing once NOTICE_SHOWN is in.
#[test]
fn a_played_notice_is_not_kept() {
    let mut r = Run::new(35, quiet_goblins());
    r.kill(2.0, Tick(10));
    r.run(day(0.25));
    assert_eq!(r.notices_since(0).len(), 1);
    assert!(r.c.kept_notices().is_empty());
    assert_eq!(r.c.stats.notices_shown, 1);
}

/// Runs a feud to its envoy, embodied near the watching player; returns the envoy command.
fn envoy_met(r: &mut Run) -> boundary::EnvoyMaterialize {
    r.watch = true;
    // Seven killings: past the envoy's rung, short of the scouts'.
    r.kill(7.0, Tick(10));
    let mut t = r.c.now();
    while t < day(4.0) {
        t += 600;
        r.run(t);
        if let Some(e) = r.cmds.iter().find_map(|(_, c)| match c {
            Cmd::Envoy(e) => Some(e.clone()),
            _ => None,
        }) {
            return e;
        }
    }
    panic!("no envoy met the player: {:?}", r.step());
}

fn wipe(r: &mut Run, e: &boundary::EnvoyMaterialize, by: u32) {
    let at = r.c.now() + 40;
    for &(body, count) in &e.members {
        r.ingest(
            Obs::Casualties(boundary::BandCasualties {
                agent: e.agent,
                epoch: e.epoch,
                by,
                body,
                count,
                killer: if by == boundary::BY_PLAYER {
                    synth::PLAYER
                } else {
                    (0, 0)
                },
            }),
            at,
        );
    }
    r.flush(at);
}

fn settled(r: &Run) -> Vec<u32> {
    r.cmds
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::Settled(s) => Some(s.outcome),
            _ => None,
        })
        .collect()
}

/// Ledger 14: an envoy party wiped out with no player among its killers (a mob, `/kill`)
/// withdraws its demand: DEMAND_SETTLED outcome 5, no relief, no refusal, no climb; the feud
/// stays on Envoy (and calms as usual later) and its demand never comes due.
#[test]
fn an_envoy_wiped_out_by_no_player_withdraws_its_demand() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    assert_eq!(r.step(), Step::Envoy);
    let deadline = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let g0 = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    wipe(&mut r, &e, boundary::BY_MOB);
    assert!(r.c.feud(r.f, r.p).unwrap().demand.is_none(), "withdrawn");
    assert_eq!(r.c.stats.withdrawn, 1);
    assert_eq!(settled(&r), vec![boundary::SETTLED_WITHDRAWN]);
    assert_eq!(r.step(), Step::Envoy, "the feud stays on Envoy");
    let g1 = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert!(
        g1 > g0 * 0.99 && g1 <= g0,
        "no relief and no insult: {g0} to {g1}"
    );
    // No recall: nobody is left in Minecraft (MZB-32).
    assert!(
        !r.cmds
            .iter()
            .any(|(_, c)| matches!(c, Cmd::Recall(x) if x.agent == e.agent)),
        "nothing to recall"
    );
    // To the old deadline: nothing came due, nothing climbed, no second demand. At it a fresh
    // envoy goes with a new demand (MZB-22).
    r.watch = false;
    r.run(Tick(deadline.0 - 1));
    assert_eq!(r.c.stats.demands, 1);
    r.run(deadline + TICKS_PER_DAY);
    assert_eq!(r.c.stats.expired, 0, "no refusal");
    assert_eq!(r.c.stats.demands, 2, "a fresh envoy at the old deadline");
    assert!(r.step() <= Step::Envoy, "no climb: {:?}", r.step());
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 0);
    assert_eq!(settled(&r), vec![boundary::SETTLED_WITHDRAWN]);
}

/// MZB-22 (review): a withdrawn demand never makes a faster war. The feud is far past the
/// scouts' rung when its envoy falls to a mob; it holds on Envoy, climbing nowhere, until the
/// old deadline, and then sends a fresh envoy with a new demand instead of climbing.
#[test]
fn a_withdrawn_demand_past_the_scouts_rung_holds_to_its_deadline() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let deadline = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let at = r.c.now() + 20;
    r.kill(20.0, at);
    let g = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert!(g > 100.0, "far past the scouts' 50: {g}");
    wipe(&mut r, &e, boundary::BY_MOB);
    assert_eq!(r.c.stats.withdrawn, 1);
    r.watch = false;
    // The Envoy rung's dwell is half a day; the deadline is days off.
    assert!(deadline > r.c.now() + TICKS_PER_DAY, "{deadline:?}");
    r.run(Tick(deadline.0 - 1));
    assert_eq!(r.step(), Step::Envoy, "held on Envoy");
    assert_eq!(
        r.c.stats.escalations[Step::Scouts.index()],
        0,
        "no climb before the old deadline"
    );
    assert_eq!(r.c.stats.demands, 1);
    assert_eq!(r.c.stats.parties[PartyKind::Envoy.index()], 1);
    r.run(deadline + 1);
    assert_eq!(r.c.stats.demands, 2, "a fresh envoy with a new demand");
    assert_eq!(r.c.stats.parties[PartyKind::Envoy.index()], 2);
    assert_eq!(r.step(), Step::Envoy);
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 0);
    let fresh = r.c.feud(r.f, r.p).unwrap().demand.unwrap();
    assert!(fresh.deadline > deadline, "{fresh:?}");
    // A save keeps the hold's end (the hold itself ends with the fresh envoy).
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(back.state_hash(), r.c.state_hash());
}

/// The hold is saved: a restart between the withdrawal and the old deadline still holds, and
/// still sends the fresh envoy at the deadline.
#[test]
fn a_withdrawn_demand_holds_across_a_restart() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let deadline = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let at = r.c.now() + 20;
    r.kill(20.0, at);
    wipe(&mut r, &e, boundary::BY_MOB);
    r.watch = false;
    r.run(r.c.now() + TICKS_PER_DAY);
    r.c = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert!(r.c.feud(r.f, r.p).unwrap().envoy_again, "the hold is saved");
    r.run(deadline + 1);
    assert_eq!(r.c.stats.demands, 2);
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 0);
}

fn shown(n: &boundary::Notice) -> Obs {
    Obs::NoticeShown(boundary::NoticeShown {
        faction: n.faction,
        serial: n.serial,
        player: n.player,
    })
}

/// MZB-10 (review): NOTICE_SHOWN names its notice by serial, not by step. A feud's second notice
/// (the feud calmed below Notice and climbed to it again) is not cleared by a late answer to its
/// first, both on the Notice rung.
#[test]
fn a_late_answer_to_an_older_notice_leaves_the_newer_kept() {
    let mut r = Run::new(37, quiet_goblins());
    r.plugin.play_notices = false;
    r.kill(2.0, Tick(10));
    r.run(day(0.25));
    let first = r.notices_since(0);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].serial, 1);
    // Back to None (the notice stays kept), then a killing climbs to Notice again.
    let mut d = 0.25;
    while r.step() > Step::None {
        d += 0.5;
        assert!(d < 200.0, "never calmed");
        r.run(day(d));
    }
    assert_eq!(r.c.kept_notices(), first, "kept while it calms");
    let mark = r.cmds.len();
    let at = r.c.now() + 10;
    r.kill(2.0, at);
    r.run(day(d + 0.25));
    let second = r.notices_since(mark);
    assert_eq!(second.len(), 1, "{second:?}");
    assert_eq!(second[0].step, first[0].step, "the same rung");
    assert_eq!(second[0].serial, 2);
    assert_eq!(
        r.c.kept_notices(),
        second,
        "the later notice replaces the first"
    );
    // The plugin answers the first late: the second stays kept.
    let at = r.c.now() + 5;
    r.ingest(shown(&first[0]), at);
    assert_eq!(r.c.kept_notices(), second);
    assert_eq!(r.c.stats.notices_shown, 0);
    r.ingest(shown(&second[0]), at);
    assert!(r.c.kept_notices().is_empty());
    assert_eq!(r.c.stats.notices_shown, 1);
    // The count is saved: the next notice after a restart is the third.
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(back.notice_serial, 2);
}

/// MZB-10 (review): a notice to a player the engine knows only from the world's own records (a
/// signal another module booked, never `Conflict::player`) is matched by its NOTICE_SHOWN too.
#[test]
fn a_notice_to_a_player_known_only_to_the_world_is_cleared_when_shown() {
    let mut r = Run::new(38, quiet_goblins());
    r.plugin.play_notices = false;
    let stranger = (0x51, 0x52);
    let a = r.w.player_uuid(stranger.0, stranger.1);
    let sig = Signal {
        faction: Some(synth::CAMP),
        kind: SignalKind::Killing,
        amount: 2.0,
        actor: a,
        pos: synth::REFINERY_POS,
        district: Some(synth::REFINERY),
    };
    r.c.signal(&mut r.w, sig, Tick(10));
    r.run(day(0.25));
    let sent = r.notices_since(0);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].player, stranger, "named from the world's records");
    assert_eq!(r.c.kept_notices(), sent);
    let at = r.c.now() + 5;
    r.ingest(shown(&sent[0]), at);
    assert!(r.c.kept_notices().is_empty(), "matched on the kept notice");
    assert_eq!(r.c.stats.notices_shown, 1);
}

/// MZB-16 (review), MZB-132: the end of Peaceful sets off no burst. Several players' feuds are
/// held across eight days of Peaceful, below the Envoy (at the notice: no envoy leaves on
/// Peaceful) or below the raid (at the scouts, reached before it began); once the world is Easy,
/// each climbs after its own dwell, paused over the Peaceful span, and a jitter of its own: no
/// two on the same tick, and none before its dwell.
fn held_feuds_climb_one_by_one(seed: u64, held: Step, next: Step) {
    let mut r = Run::new(seed, quiet_goblins());
    let players: Vec<ActorId> = (0..5u64)
        .map(|i| r.c.player(&mut r.w, (0x100 + i, 0x200 + i)))
        .collect();
    let killings = |r: &mut Run, at: Tick| {
        for &p in &players {
            let sig = Signal {
                faction: Some(synth::CAMP),
                kind: SignalKind::Killing,
                amount: 20.0,
                actor: p,
                pos: synth::REFINERY_POS,
                district: Some(synth::REFINERY),
            };
            r.c.signal(&mut r.w, sig, at);
        }
    };
    if held == Step::Scouts {
        // Already at the scouts when Peaceful begins, none of them yet planned past it.
        killings(&mut r, Tick(5));
        for &p in &players {
            let x = r.c.feud_mut(r.f, p).unwrap();
            x.step = Step::Scouts;
            x.since = Tick(10);
        }
        r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(10));
    } else {
        r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(1));
    }
    assert!(r.c.peaceful);
    let mut d = 0.0;
    while d < 8.0 {
        let at = r.c.now().max(day(d));
        killings(&mut r, at);
        d += 2.0;
        r.run(day(d));
    }
    assert_eq!(r.war_parties(), 0, "none on Peaceful");
    assert_eq!(r.c.stats.demands, 0, "no envoy on Peaceful");
    let dwell = r.c.cultures.get(0).dwell_ticks[held.index()];
    for &p in &players {
        let x = r.c.feud(r.f, p).unwrap();
        assert_eq!(x.step, held, "held below {next:?}");
        assert!(x.party.is_none() && x.demand.is_none());
    }
    let easy = r.c.now();
    r.ingest(difficulty(boundary::DIFFICULTY_EASY), easy);
    assert!(!r.c.peaceful);
    // To each climb, event by event, so each tick is exact.
    let mut climbed: Vec<(ActorId, Tick)> = Vec::new();
    let end = easy + 3 * dwell + TICKS_PER_DAY;
    while climbed.len() < players.len() && r.c.now() < end {
        let next_at = r.c.next_due().unwrap_or(end).min(end).max(r.c.now() + 1);
        r.c.run_until(&mut r.w, &mut NoInfrastructure, next_at);
        r.flush(next_at);
        for &p in &players {
            let x = r.c.feud(r.f, p).unwrap();
            if x.step == next && !climbed.iter().any(|e| e.0 == p) {
                climbed.push((p, r.c.now()));
            }
        }
    }
    assert_eq!(
        climbed.len(),
        players.len(),
        "every held feud climbs: {climbed:?}"
    );
    let mut ticks: Vec<Tick> = climbed.iter().map(|e| e.1).collect();
    ticks.sort();
    ticks.dedup();
    assert_eq!(
        ticks.len(),
        players.len(),
        "no two on one tick: {climbed:?}"
    );
    for (p, at) in &climbed {
        // Reached before or during Peaceful, so the whole dwell comes after it.
        assert!(
            *at >= easy + dwell,
            "{p:?} climbed at {at:?}, before its dwell"
        );
        assert!(*at < easy + 2 * dwell + TICKS_PER_DAY, "{at:?}");
    }
}

#[test]
fn feuds_held_at_the_notice_over_peaceful_send_their_envoys_one_by_one_after() {
    held_feuds_climb_one_by_one(39, Step::Notice, Step::Envoy);
}

#[test]
fn feuds_held_at_the_scouts_over_peaceful_raid_one_by_one_after() {
    held_feuds_climb_one_by_one(39, Step::Scouts, Step::Raid);
}

/// MZB-15 (review): Peaceful stops what is already out. A raid on its way goes home at once
/// (RECALL_SETTLED if it was embodied), its feud a rung back; a watched fight ends; no war body
/// spawns while it is Peaceful.
#[test]
fn entering_peaceful_turns_back_a_raid_already_out() {
    let mut r = Run::new(40, quiet_goblins());
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.c.feud_mut(r.f, r.p).unwrap().party = Some(band);
    // Past its muster: out on the road.
    let mut t = Tick(20);
    while r.c.parties[band].phase != rtsim_conflict::engine::Phase::Out {
        t += 600;
        assert!(t < day(3.0), "never left");
        r.run(t);
    }
    let at = r.c.now() + 1;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), at);
    assert_eq!(
        r.c.parties[band].phase,
        rtsim_conflict::engine::Phase::Home,
        "turned back"
    );
    assert_eq!(
        r.step(),
        Step::Scouts,
        "a rung below the raid it never made"
    );
    assert!(r.c.feud(r.f, r.p).unwrap().party.is_none());
    assert_eq!(r.c.stats.peace_held, 1);
    // The player watches its road home: nothing spawns.
    r.watch = true;
    let before = r.cmds.len();
    r.run(r.c.now() + 2 * TICKS_PER_DAY);
    assert!(
        !r.cmds[before..]
            .iter()
            .any(|(_, c)| matches!(c, Cmd::Materialize(m) if m.agent == rtsim_conflict::engine::Party::agent(band))),
        "no war body spawns on Peaceful"
    );
    assert_eq!(r.war_parties(), 1, "only the one ordered");
}

/// The feud's envoy party, if one is out, with its phase.
fn envoy_of(r: &Run) -> Option<(Handle<Party>, Phase)> {
    r.c.parties
        .iter()
        .find(|(_, x)| x.kind == PartyKind::Envoy)
        .map(|(h, x)| (h, x.phase))
}

/// MZB-132: Peaceful begins with an envoy out (`Out` on its road) or waiting for its answer at
/// the target, none of it in view. It stays where it is, abstract: no deadline comes due however
/// long Peaceful lasts, nothing expires, no second envoy goes, and a save in the middle keeps
/// that. When Peaceful ends the demand's deadline has moved by the span, and it comes due then
/// and not before.
fn envoy_held_over_peaceful(seed: u64, phase: Phase) {
    let mut r = Run::new(seed, quiet_goblins());
    r.kill(7.0, Tick(10));
    let mut t = r.c.now();
    let band = loop {
        t += 100;
        assert!(t < day(4.0), "no envoy reached {phase:?}: {:?}", r.step());
        r.run(t);
        if let Some((h, ph)) = envoy_of(&r)
            && ph == phase
        {
            break h;
        }
    };
    let d0 =
        r.c.feud(r.f, r.p)
            .unwrap()
            .demand
            .expect("a demand")
            .deadline;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), r.c.now() + 1);
    let since = r.c.peaceful_since;
    // The killing goes on (a signal that stopped would be cut, not expired, at the deadline).
    r.kill(1.0, r.c.now() + 5);
    // Two days past the deadline it never reached.
    r.run(d0 + 2 * TICKS_PER_DAY);
    assert!(r.c.parties.contains(band), "the envoy waits where it is");
    assert_eq!(r.c.parties[band].phase, Phase::Waiting, "at its target");
    assert!(!r.c.parties[band].embodied);
    assert!(r.c.stats.peace_held >= 1, "the hold was counted");
    assert_eq!(r.c.stats.expired, 0, "no demand came due on Peaceful");
    assert!(settled(&r).is_empty(), "nothing settled: {:?}", settled(&r));
    assert_eq!(r.c.stats.demands, 1, "no second envoy");
    assert_eq!(r.step(), Step::Envoy);
    let x = r.c.feud(r.f, r.p).unwrap();
    assert_eq!(x.demand.unwrap().deadline, d0, "unmoved while Peaceful");
    // A save in the middle of it keeps the hold.
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(back.state_hash(), r.c.state_hash());
    // Normal: the deadline moves by the span; the envoy waits it out.
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), r.c.now() + 10);
    let span = r.c.now().since(since);
    assert!(span > 2 * TICKS_PER_DAY, "{span}");
    let d1 = d0 + span;
    assert_eq!(r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline, d1);
    r.run(Tick(d1.0 - 1));
    assert_eq!(r.c.stats.expired, 0, "not before the moved deadline");
    assert!(settled(&r).is_empty());
    r.run(d1 + TICKS_PER_DAY);
    assert_eq!(r.c.stats.expired, 1, "at the moved deadline");
    assert_eq!(settled(&r), vec![boundary::SETTLED_EXPIRED]);
}

#[test]
fn an_envoy_on_its_road_when_peaceful_begins_waits_out_the_span() {
    envoy_held_over_peaceful(42, Phase::Out);
}

#[test]
fn an_envoy_waiting_at_its_target_when_peaceful_begins_waits_out_the_span() {
    envoy_held_over_peaceful(43, Phase::Waiting);
}

/// MZB-132: an envoy whose muster ends on Peaceful is held in it (nothing booked, the hold
/// counted); the end of Peaceful books its muster again and it goes out.
#[test]
fn an_envoy_mustering_when_peaceful_begins_is_held_and_goes_after() {
    let mut r = Run::new(44, quiet_goblins());
    r.kill(2.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Envoy,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    assert_eq!(r.c.parties[band].phase, Phase::Mustering);
    r.c.set_difficulty(&mut r.w, boundary::DIFFICULTY_PEACEFUL);
    r.run(day(2.0));
    assert_eq!(
        r.c.parties[band].phase,
        Phase::Mustering,
        "held in its muster"
    );
    assert_eq!(r.c.stats.peace_held, 1);
    assert!(!r.c.parties[band].embodied);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), day(2.0) + 10);
    r.run(day(3.0));
    assert_ne!(
        r.c.parties.get(band).map(|x| x.phase),
        Some(Phase::Mustering),
        "it went out once the world was not Peaceful"
    );
}

/// MZB-132: a withdrawn demand's fresh envoy (MZB-22) waits out Peaceful too: its old deadline
/// moves by the span, so no envoy goes on Peaceful and none goes in a burst at its end.
#[test]
fn a_withdrawn_demands_fresh_envoy_waits_out_peaceful() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let deadline = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let at = r.c.now() + 20;
    r.kill(20.0, at);
    wipe(&mut r, &e, boundary::BY_MOB);
    r.watch = false;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), r.c.now() + 1);
    let since = r.c.peaceful_since;
    r.run(deadline + 2 * TICKS_PER_DAY);
    assert_eq!(r.c.stats.demands, 1, "no fresh envoy on Peaceful");
    assert_eq!(r.step(), Step::Envoy);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), r.c.now() + 10);
    let span = r.c.now().since(since);
    r.run(Tick((deadline + span).0 - 1));
    assert_eq!(
        r.c.stats.demands, 1,
        "not before the old deadline plus the span"
    );
    r.run(deadline + span + TICKS_PER_DAY);
    assert_eq!(r.c.stats.demands, 2, "a fresh envoy at the moved deadline");
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 0, "no climb");
}

/// MZB-17 (review): DIFFICULTY is journaled; the engine applies it at its game time and an
/// unchanged value changes nothing.
#[test]
fn an_unchanged_difficulty_changes_nothing() {
    let mut r = Run::new(41, quiet_goblins());
    r.c.log_enabled = true;
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), Tick(5));
    assert!(!r.c.peaceful);
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(10));
    assert!(r.c.peaceful);
    assert_eq!(r.c.peaceful_since, Tick(10));
    let h = r.c.state_hash();
    let notes = r.c.log.len();
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(10));
    assert_eq!(r.c.state_hash(), h);
    assert_eq!(r.c.log.len(), notes);
    assert_eq!(r.c.peaceful_since, Tick(10), "the span starts at the first");
}

/// MZB-23 (review): a record whose engine half comes later spends its op id, as rtsim-jni does.
#[test]
fn a_deferred_record_spends_its_op_id() {
    let mut r = Run::new(42, quiet_goblins());
    let op = 0xABCD;
    assert!(Conflict::later(&Obs::CampDeath(boundary::CampDeath {
        faction: synth::CAMP,
        by: boundary::BY_MOB,
        body: 1,
        flags: 0,
        slot: 0,
        generation: 0,
        killer: (0, 0),
        op,
    })));
    r.ingest(
        Obs::CampDeath(boundary::CampDeath {
            faction: synth::CAMP,
            by: boundary::BY_MOB,
            body: 1,
            flags: 0,
            slot: 0,
            generation: 0,
            killer: (0, 0),
            op,
        }),
        Tick(5),
    );
    assert!(r.c.op_applied(op), "spent");
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert!(back.op_applied(op), "and saved");
}

/// A rail line under every band's road: each segment a band walks out crosses a train halfway.
#[derive(Default)]
struct Rails {
    raided: u32,
}

impl rtsim_ecology::infra::Infrastructure for Rails {
    fn districts_near(
        &mut self,
        _: u16,
        _: i64,
        _: i64,
        _: i64,
    ) -> Vec<rtsim_ecology::infra::DistrictSite> {
        Vec::new()
    }
    fn raid(
        &mut self,
        _: &rtsim_industry::threat::Threat,
    ) -> Option<rtsim_ecology::infra::RaidOutcome> {
        None
    }
    fn good_name(&self, _: rtsim_industry::ids::Good) -> Option<String> {
        None
    }
    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<rtsim_ecology::infra::TrainSighting> {
        vec![rtsim_ecology::infra::TrainSighting {
            train: rtsim_industry::ids::TrainId(7),
            from: from + to.since(from) / 2,
            to,
        }]
    }
    fn raid_train(
        &mut self,
        _: &rtsim_industry::threat::Threat,
    ) -> Option<rtsim_industry::ids::GoodVec> {
        self.raided += 1;
        None
    }
}

/// A raid ordered to the refinery, run on `rails` until it is out on its road with an ambush
/// booked; `peaceful` then turns the world Peaceful. Returns the trains raided by day 3.
fn raid_over_rails(seed: u64, peaceful: bool) -> (u32, Run) {
    let mut r = Run::new(seed, quiet_goblins());
    r.c.tun.ambush_milli = 1_000;
    let mut rails = Rails::default();
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let mut t = Tick(20);
    while r.c.parties[band].phase != rtsim_conflict::engine::Phase::Out {
        t += 100;
        assert!(t < day(3.0), "never left");
        r.c.run_until(&mut r.w, &mut rails, t);
    }
    assert!(
        r.c.pending_by_kind().get("ambush").copied().unwrap_or(0) > 0,
        "an ambush booked on its road"
    );
    if peaceful {
        let at = r.c.now() + 1;
        let o = difficulty(boundary::DIFFICULTY_PEACEFUL);
        r.c.ingest(&mut r.w, &mut rails, &o, at);
    }
    r.c.run_until(&mut r.w, &mut rails, day(3.0));
    (rails.raided, r)
}

/// MZB-15 (review): an ambush booked on a raid's road never falls once the world is Peaceful:
/// the raid turns back and its booking is stale. Without Peaceful the same raid falls on the
/// train (the control). The rtsim-jni rig has no train under a band's road (MZB-34).
#[test]
fn an_ambush_booked_before_peaceful_never_falls() {
    let (raided, _) = raid_over_rails(43, false);
    assert!(raided >= 1, "the control: the train is raided");
    let (raided, r) = raid_over_rails(43, true);
    assert_eq!(raided, 0, "no ambush on Peaceful");
    assert!(r.c.peaceful);
}

// ---- severity units (MZB-36) ----------------------------------------------------------------

/// MZB-36: a camp's robbery of a village counts at the kind's base delta, as every module's
/// robberies do (severity on `SEVERITY_UNIT`): the village thinks about 50 points worse of the
/// camp, not 1.
#[test]
fn a_robbery_counts_at_the_base_delta() {
    let mut r = Run::new(6, quiet_goblins());
    let ashby = r.w.node_of_key(4).expect("Ashby");
    let (va, fa) = (r.w.nodes[ashby].actor, r.c.factions[r.f as usize].actor);
    // Ashby's lines stand at their targets on day 0, and a raid takes only the surplus (CL-D58):
    // 20 items over each, so there is something to take.
    let m = r.w.market(ashby);
    let rich: BTreeMap<GoodId, i64> =
        r.w.goods
            .ids()
            .map(|g| (g, m.line(g).target + 20 * MILLI - m.stock(g)))
            .filter(|e| e.1 > 0)
            .collect();
    r.w.stock_from_players(ashby, &rich, Tick(10));
    r.c.order_party(
        &mut r.w,
        synth::CAMP,
        PartyKind::Plunder,
        Target::Node(ashby),
        None,
        Tick(10),
    )
    .expect("plunderers");
    let mut t = Tick(10);
    while r.c.ledger.from_villages.is_empty() {
        t = r.c.next_due().unwrap_or(t + TICKS_PER_DAY);
        assert!(t < day(10.0), "never raided");
        r.c.run_until(&mut r.w, &mut NoInfrastructure, t);
    }
    let base = i32::from(
        r.w.knowledge
            .tunables()
            .delta(ReportKind::Robbery, SEVERITY_UNIT, false),
    );
    assert_eq!(base, -50);
    let felt = i32::from(r.w.knowledge.sentiment(va, fa, t));
    assert!(
        felt <= base + 5,
        "Ashby feels {felt} toward the camp that robbed it (base delta {base})"
    );
}

/// A raid with its war chief, fighting the player (as the plugin's watched fight would have it),
/// a day after the player killed 20 of the camp's members.
fn chief_in_a_fight(seed: u64) -> (Run, Handle<Party>) {
    let mut r = Run::new(seed, quiet_goblins());
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    assert!(r.c.parties[band].chief);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, Tick::from_days(1) + 100);
    r.c.parties[band].phase = Phase::Fighting;
    (r, band)
}

/// The villages that hate goblins (human and bandit) within the rescue news range of the camp.
fn haters_in_range(r: &Run) -> Vec<ActorId> {
    let camp = synth::CAMP_POS;
    let range = f64::from(r.c.tun.rescue_news_blocks);
    r.w.nodes
        .iter()
        .filter(|n| match n.kind {
            NodeKind::Village(cu) => ["human", "bandit"].contains(&cu.name()),
            _ => false,
        })
        .filter(|n| {
            let (dx, dz) = (f64::from(n.pos.x - camp.x), f64::from(n.pos.z - camp.z));
            n.pos.frame == camp.frame && (dx * dx + dz * dz).sqrt() <= range - 1.0
        })
        .map(|n| n.actor)
        .collect()
}

/// MZB-36: the heir who saw the player kill the war chief feels it at the killing's base delta
/// (-125), so the grudge it takes over is about 60 grievance (`heir_grudge_factor` 0.5 a point),
/// not 2.5. And the villages that hate the camp hear of the rescue at the rescue's +25.
#[test]
fn a_chief_killed_counts_at_the_base_delta() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = Tick::from_days(1) + 100;
    let heir = r.c.factions[r.f as usize].heir;
    let killing = |r: &Run| {
        r.c.feud_view(synth::CAMP, r.p)
            .unwrap()
            .by_kind
            .get("killing")
            .copied()
            .unwrap_or(0.0)
    };
    let before = killing(&r);
    // The villages that hate goblins within the news range, and what they feel before.
    let haters: Vec<(ActorId, i8)> = haters_in_range(&r)
        .into_iter()
        .map(|a| (a, r.w.knowledge.sentiment(a, r.p, at)))
        .collect();
    assert!(
        !haters.is_empty(),
        "a village that hates goblins is in range"
    );
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(
        r.c.factions[r.f as usize].chief, heir,
        "the heir takes over"
    );
    let felt = i32::from(r.w.knowledge.sentiment(heir, r.p, at));
    assert!(
        felt <= -100,
        "the heir feels {felt} toward the chief's killer"
    );
    let grudge = killing(&r) - before;
    assert!(
        grudge >= 0.5 * 100.0 - 1.0,
        "the heir's grudge adds {grudge:.1} killing grievance ({before:.1} before)"
    );
    for (va, was) in haters {
        let now = r.w.knowledge.sentiment(va, r.p, at);
        assert!(
            i32::from(now) >= i32::from(was) + 20,
            "a village that hates goblins thinks better of the rescuer: {was} to {now}"
        );
    }
    assert_eq!(r.c.stats.routs, 1);
}

/// CL-D60 (K2-25: the villages witness "a rescue report"): one rout is one rescue report, however
/// many hating villages are in range. Each of them witnesses it first-hand at the full +25; a
/// village that does not hate goblins does not know it yet (it hears it later, as news). A report
/// per witness made every village count one rout once per hating village.
#[test]
fn a_rout_is_one_rescue_report_that_every_hater_in_range_witnesses() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = Tick::from_days(1) + 100;
    let rescues = |r: &Run| -> Vec<rtsim_knowledge::ReportId> {
        r.w.knowledge
            .reports()
            .iter()
            .map(|(id, _)| id)
            .filter(|id| {
                r.w.knowledge
                    .report(*id)
                    .is_some_and(|x| x.kind == ReportKind::Rescue)
            })
            .collect()
    };
    assert!(rescues(&r).is_empty(), "no rescue before the rout");
    let haters = haters_in_range(&r);
    assert!(
        haters.len() >= 2,
        "several hating villages in range ({})",
        haters.len()
    );
    let others: Vec<ActorId> =
        r.w.nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Village(_)) && !haters.contains(&n.actor))
            .map(|n| n.actor)
            .collect();
    assert!(!others.is_empty(), "a village that does not hate goblins");
    let was: Vec<i8> = haters
        .iter()
        .map(|a| r.w.knowledge.sentiment(*a, r.p, at))
        .collect();
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    let reps = rescues(&r);
    assert_eq!(
        reps.len(),
        1,
        "one rout, one rescue report ({} recorded)",
        reps.len()
    );
    let rep = r.w.knowledge.report(reps[0]).unwrap();
    assert_eq!(rep.who, r.p, "the rescuer");
    assert_eq!(
        rep.whom,
        ActorId::NULL,
        "the band was after the rescuer himself, so it names no one rescued"
    );
    for (a, was) in haters.iter().zip(was) {
        assert!(
            r.w.knowledge.knows(*a, reps[0]),
            "a hating village in range witnesses it"
        );
        let now = r.w.knowledge.sentiment(*a, r.p, at);
        assert!(
            i32::from(now) >= i32::from(was) + 20,
            "first-hand, not hearsay: {was} to {now}"
        );
    }
    for a in others {
        assert!(
            !r.w.knowledge.knows(a, reps[0]),
            "a village that does not hate goblins only hears it later"
        );
    }
}

// ---- the review of M3 items 12 and 13 (MZB-133 to MZB-138) -------------------------------------

/// TRIBUTE_PAID: `count` of `good` paid by the player to `agent` (0: at the camp).
fn pay_to(r: &mut Run, agent: u64, epoch: u32, count: i32, good: GoodId, op: u64, at: Tick) {
    r.ingest(
        Obs::TributePaid(boundary::TributePaid {
            faction: synth::CAMP,
            agent,
            epoch,
            player: synth::PLAYER,
            op,
            items: vec![boundary::Item {
                item: item_of(good),
                count,
            }],
        }),
        at,
    );
    r.flush(at);
}

/// ENVOY_ANSWER from the player at `e`.
fn answer_to(r: &mut Run, e: &boundary::EnvoyMaterialize, answer: u32, at: Tick) {
    r.ingest(
        Obs::EnvoyAnswer(boundary::EnvoyAnswer {
            agent: e.agent,
            epoch: e.epoch,
            answer,
            player: synth::PLAYER,
        }),
        at,
    );
    r.flush(at);
}

/// The targets of the scouts the feud has sent.
fn scout_targets(r: &Run) -> Vec<Target> {
    r.c.parties
        .iter()
        .filter(|(_, x)| x.kind == PartyKind::Scouts)
        .map(|(_, x)| x.target)
        .collect()
}

/// MZB-133 (review item 1): an insult answer (the player attacks the envoy) is a killing at
/// the envoy's own place, against its target's district, as the killing of one of its members
/// is. It used to be at the faction's own camp with no district, which a feud whose focus had no
/// district took for its new focus: the refusal's scouts then went to their own camp.
#[test]
fn an_insult_answer_keeps_the_scouts_aimed_at_the_demands_target() {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.answer = Answer::Ignore;
    let e = envoy_met(&mut r);
    // The killing goes on, so the refusal's grievance reaches the scouts' rung.
    let at = r.c.now() + 1;
    r.kill(60.0, at);
    answer_to(&mut r, &e, boundary::ANSWER_ATTACKED, at + 5);
    assert_eq!(settled(&r), vec![boundary::SETTLED_REFUSED]);
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 1, "it climbed");
    let focus = r.c.feud(r.f, r.p).unwrap().focus.unwrap();
    assert_eq!(
        focus.district,
        Some(synth::REFINERY),
        "the demand's district"
    );
    assert_ne!(focus.pos, synth::CAMP_POS, "not the camp's own place");
    let targets = scout_targets(&r);
    assert!(!targets.is_empty(), "scouts were sent");
    for t in targets {
        assert!(
            matches!(t, Target::District(d, _) if d == synth::REFINERY),
            "the scouts go where the demand pointed, not to their camp: {t:?}"
        );
    }
}

/// The same for a feud whose focus has no district (the killings were in the wild): the insult
/// is where the envoy stood, so the focus does not move to the camp and the scouts do not go
/// there either.
#[test]
fn an_insult_answer_on_a_districtless_focus_does_not_point_at_the_camp() {
    let mut r = Run::new(36, quiet_goblins());
    r.watch = true;
    let wild = |n: f64, a: ActorId| Signal {
        faction: Some(synth::CAMP),
        kind: SignalKind::Killing,
        amount: n,
        actor: a,
        pos: synth::REFINERY_POS,
        district: None,
    };
    let p = r.p;
    r.c.signal(&mut r.w, wild(7.0, p), Tick(10));
    let mut t = r.c.now();
    let e = loop {
        t += 600;
        assert!(t < day(4.0), "no envoy met the player");
        r.run(t);
        if let Some(e) = r.cmds.iter().find_map(|(_, c)| match c {
            Cmd::Envoy(e) => Some(e.clone()),
            _ => None,
        }) {
            break e;
        }
    };
    let at = r.c.now() + 5;
    r.c.signal(&mut r.w, wild(20.0, p), at);
    assert!(
        r.c.feud(r.f, r.p)
            .unwrap()
            .focus
            .unwrap()
            .district
            .is_none()
    );
    answer_to(&mut r, &e, boundary::ANSWER_ATTACKED, at + 5);
    let focus = r.c.feud(r.f, r.p).unwrap().focus.unwrap();
    assert_ne!(
        focus.pos,
        synth::CAMP_POS,
        "the focus did not move to the camp"
    );
    let targets = scout_targets(&r);
    assert!(!targets.is_empty(), "scouts were sent");
    assert!(
        !targets.contains(&Target::Pos(synth::CAMP_POS)),
        "the scouts do not go to their own camp: {targets:?}"
    );
}

/// An attack is still a refusal with an insult on top of it: two killings' worth more grievance
/// than a plain refusal, and no other difference in the outcome.
#[test]
fn an_insult_answer_adds_the_insult_to_a_refusal() {
    let run = |answer: Answer| {
        let mut r = Run::new(36, quiet_goblins());
        r.plugin.answer = answer;
        let _ = envoy_met(&mut r);
        let t = r.c.now() + 400;
        r.run(t);
        let lambda = r.c.cultures.get(0).lambda;
        let g = r.c.feud(r.f, r.p).unwrap().g.total_at(t, lambda);
        (settled(&r), g)
    };
    let (s_ref, g_ref) = run(Answer::Refuse);
    let (s_att, g_att) = run(Answer::Attack);
    assert_eq!(s_ref, vec![boundary::SETTLED_REFUSED]);
    assert_eq!(s_att, vec![boundary::SETTLED_REFUSED]);
    let weight = quiet_goblins().get(0).signals[SignalKind::Killing.index()].weight;
    let insult = g_att - g_ref;
    assert!(
        insult > 1.9 * weight && insult <= 2.0 * weight + 1e-9,
        "the insult is envoy_insult_kills killings: {g_ref} to {g_att} (weight {weight})"
    );
}

/// A raid band on the way, ordered at `Tick(20)` after two killings.
fn raid_band(r: &mut Run) -> Handle<Party> {
    r.kill(2.0, Tick(10));
    r.c.order_party(
        &mut r.w,
        synth::CAMP,
        PartyKind::Raid,
        Target::District(synth::REFINERY, synth::REFINERY_POS),
        Some(r.p),
        Tick(20),
    )
    .unwrap()
}

/// BAND_DEMATERIALIZED for `band` at `epoch`: every member survives but `dead` of the first
/// stack (the walk-off's deaths).
fn despawn_with(r: &Run, band: Handle<Party>, epoch: u32, dead: u32) -> Obs {
    let c = r.c.cultures.get(0).clone();
    let survivors = r.c.parties[band]
        .units
        .iter()
        .enumerate()
        .map(|(i, &(u, n))| {
            let n = if i == 0 { n.saturating_sub(dead) } else { n };
            (body_id(&r.c.cultures, &c.unit(u).body), n)
        })
        .collect();
    Obs::Dematerialized(boundary::BandDematerialized {
        agent: Party::agent(band),
        epoch,
        reason: boundary::RECALL_PASSING,
        survivors,
    })
}

/// A restart: the engine as the pages saved it, loaded again, in its next session.
fn restarted(r: &Run) -> Conflict {
    let mut c = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    c.session_start();
    c
}

/// MZB-138 (review item 2, older than item 12): a band was recalled with RECALL_PASSING before
/// the last save, its despawn (BAND_DEMATERIALIZED) came after it, in the journal. The restart
/// replays that record, and the members who died on the walk-off are still dead: the session
/// start used to clear `recalled`, so the replayed despawn was stale and they came back.
#[test]
fn a_replayed_despawn_after_a_restart_takes_the_walk_off_dead() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    let epoch = {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.recalled = vec![(x.epoch, x.units.clone())];
        x.epoch
    };
    let alive = r.c.parties[band].alive();
    let o = despawn_with(&r, band, epoch, 2);
    let mut same = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    let mut back = restarted(&r);
    assert_eq!(same.parties[band].recalled.len(), 1, "the entry is saved");
    assert_eq!(
        back.parties[band].recalled.len(),
        1,
        "and a restart keeps it"
    );
    let at = r.c.now() + 1;
    same.ingest(&mut r.w, &mut NoInfrastructure, &o, at);
    back.ingest(&mut r.w, &mut NoInfrastructure, &o, at);
    assert_eq!(
        same.parties[band].alive(),
        alive - 2,
        "the session that went on"
    );
    assert_eq!(
        back.parties[band].alive(),
        alive - 2,
        "the restart's replay"
    );
    assert!(back.parties[band].recalled.is_empty(), "the entry is spent");
}

/// MZB-138: what no replayed record counted is dropped when the replay has caught up (the host's
/// `resend_notices`): those bodies are gone with the last session and their members count as
/// alive, as they always did; a despawn that comes after that is stale.
#[test]
fn the_stacks_a_restart_kept_are_dropped_when_the_replay_has_caught_up() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    let epoch = {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.recalled = vec![(x.epoch, x.units.clone())];
        x.epoch
    };
    let alive = r.c.parties[band].alive();
    let mut back = restarted(&r);
    assert!(back.replaying);
    back.resend_notices();
    assert!(!back.replaying);
    assert!(
        back.parties[band].recalled.is_empty(),
        "nothing left to count"
    );
    assert_eq!(back.parties[band].alive(), alive);
    let at = r.c.now() + 1;
    let despawn = despawn_with(&r, band, epoch, 2);
    back.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at);
    assert_eq!(
        back.parties[band].alive(),
        alive,
        "a despawn after that is stale"
    );
}

/// MZB-138: a band in Minecraft when the pages were saved, its casualties and then its despawn
/// journaled after. The restart's replay counts them as the session that went on counts them:
/// the bodies the pages held in the world are a recalled stack of their epoch for the replayed
/// records.
#[test]
fn a_band_embodied_at_the_crash_has_its_replayed_casualties_and_despawn_counted() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    let epoch = {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.embodied = true;
        x.spawned = x.units.clone();
        x.epoch
    };
    let alive = r.c.parties[band].alive();
    assert!(alive >= 4, "a band to lose members from ({alive})");
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(r.c.parties[band].units[0].0).body)
    };
    let casualty = Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_MOB,
        body,
        count: 1,
        killer: (0, 0),
    });
    let mut same = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    let mut back = restarted(&r);
    assert_eq!(
        back.parties[band].recalled,
        vec![(epoch, r.c.parties[band].spawned.clone())],
        "the bodies in the world are kept for the replay"
    );
    assert!(!back.parties[band].embodied);
    assert!(back.parties[band].spawned.is_empty());
    // One death reported, then one more that died unreported before the chunks unloaded.
    let despawn = despawn_with(&r, band, epoch, 2);
    for c in [&mut same, &mut back] {
        c.ingest(&mut r.w, &mut NoInfrastructure, &casualty, r.c.now() + 1);
        c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, r.c.now() + 2);
    }
    assert_eq!(
        same.parties[band].alive(),
        alive - 2,
        "the session that went on"
    );
    assert_eq!(
        back.parties[band].alive(),
        alive - 2,
        "the restart's replay"
    );
    assert_eq!(same.stats.casualties, 1);
    assert_eq!(back.stats.casualties, 1, "counted once");
    assert!(back.parties[band].recalled.is_empty());
    back.resend_notices();
    assert_eq!(
        back.parties[band].alive(),
        alive - 2,
        "kept after the replay"
    );
}

/// MZB-134 (review item 3): a refusal that reaches the engine on Peaceful (an answer in flight
/// when the world changed, a dialogue still open) is recorded, but a climb to a rung Peaceful
/// holds waits for Normal. The culture here skips the scouts, so the rung above the Envoy is the
/// raid; it used to climb on Peaceful, counted its escalation and mustered a band.
#[test]
fn a_refusal_on_peaceful_is_recorded_and_its_climb_to_war_waits_for_normal() {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    d.parties.insert("scouts".into(), vec![]);
    let cultures = Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap();
    let mut r = Run::new(36, cultures);
    r.plugin.answer = Answer::Ignore;
    let e = envoy_met(&mut r);
    r.kill(40.0, r.c.now() + 20);
    r.watch = false;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), r.c.now() + 1);
    let at = r.c.now() + 5;
    answer_to(&mut r, &e, boundary::ANSWER_REFUSED, at);
    assert_eq!(settled(&r), vec![boundary::SETTLED_REFUSED], "recorded");
    assert_eq!(r.step(), Step::Envoy, "the feud stays where it is");
    assert_eq!(r.c.stats.escalations[Step::Raid.index()], 0, "no war climb");
    assert_eq!(r.war_parties(), 0, "no band mustered on Peaceful");
    // Peaceful goes on, the killing too: nothing leaves.
    let more = r.c.now() + 4 * TICKS_PER_DAY;
    while r.c.now() < more {
        let t = r.c.now() + TICKS_PER_DAY / 2;
        r.kill(10.0, t);
        r.run(t + 10);
    }
    assert_eq!(r.step(), Step::Envoy);
    assert_eq!(r.war_parties(), 0);
    // Normal: the held climb comes, after its dwell.
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), r.c.now() + 10);
    let end = r.c.now() + 12 * TICKS_PER_DAY;
    while r.c.now() < end && r.step() == Step::Envoy {
        let t = r.c.now() + TICKS_PER_DAY / 2;
        r.kill(10.0, t);
        r.run(t + 10);
    }
    assert_eq!(r.step(), Step::Raid, "the climb comes with Normal");
    assert_eq!(r.c.stats.escalations[Step::Raid.index()], 1);
}

/// The scouts are not war (MZB-132): a refusal on Peaceful still climbs to them, as a feud at the
/// notice does. A fork, kept: only a war rung or the Envoy is held.
#[test]
fn a_refusal_on_peaceful_still_climbs_to_the_scouts() {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.answer = Answer::Ignore;
    let e = envoy_met(&mut r);
    r.kill(20.0, r.c.now() + 20);
    r.watch = false;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), r.c.now() + 1);
    let at = r.c.now() + 5;
    answer_to(&mut r, &e, boundary::ANSWER_REFUSED, at);
    assert_eq!(settled(&r), vec![boundary::SETTLED_REFUSED]);
    assert_eq!(r.step(), Step::Scouts);
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 1);
}

/// MZB-135 (review item 4): Peaceful entered and left on the same tick pauses nothing and
/// jitters nothing: a feud held at its notice keeps its dwell clock. (A span of any length
/// does move it, the control here.) The walk-off of an envoy in view on entering stays.
#[test]
fn peaceful_on_and_off_in_one_tick_leaves_a_held_feuds_dwell_alone() {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(2.0, Tick(10));
    r.run(day(3.0));
    assert_eq!(
        r.step(),
        Step::Notice,
        "held below the envoy by its grievance or dwell"
    );
    // Let the clock sit well past its dwell, so a jitter would show.
    let since = r.c.feud(r.f, r.p).unwrap().since;
    let at = r.c.now() + 1;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), at);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), at);
    assert_eq!(r.c.peaceful_since, at);
    assert_eq!(
        r.c.feud(r.f, r.p).unwrap().since,
        since,
        "a span of no time moves no dwell"
    );
    // The control: the same feud over a span of 1,000 ticks does move.
    let on = at + 100;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), on);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), on + 1_000);
    assert_ne!(
        r.c.feud(r.f, r.p).unwrap().since,
        since,
        "a span of some time does (the test is not vacuous)"
    );
}

/// The envoy in view on a zero-tick Peaceful still walks off (RECALL_PASSING), and its deadline
/// does not move.
#[test]
fn an_envoy_in_view_still_walks_off_for_a_peaceful_of_no_time() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let d0 = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let from = r.cmds.len();
    let at = r.c.now() + 1;
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), at);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), at);
    r.flush(at);
    assert_eq!(r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline, d0);
    let recalls: Vec<u32> = r.cmds[from..]
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::Recall(x) if x.agent == e.agent => Some(x.reason),
            _ => None,
        })
        .collect();
    assert_eq!(recalls, vec![boundary::RECALL_PASSING]);
}

/// MZB-136 (review item 5): leaving Peaceful moves a waiting envoy's patience by the part of the
/// span it spent waiting. One waiting when Peaceful began waited all of it; one on its road
/// arrived part way through (its patience started then) and waited from its arrival.
fn bored_over_peaceful(seed: u64, phase: Phase) {
    let mut r = Run::new(seed, quiet_goblins());
    r.kill(7.0, Tick(10));
    let mut t = r.c.now();
    let band = loop {
        t += 100;
        assert!(t < day(4.0), "no envoy reached {phase:?}");
        r.run(t);
        if let Some((h, ph)) = envoy_of(&r)
            && ph == phase
        {
            break h;
        }
    };
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), r.c.now() + 1);
    let since = r.c.peaceful_since;
    r.kill(1.0, r.c.now() + 5);
    // An envoy on its road arrives, still on Peaceful, and waits.
    let limit = r.c.now() + 20 * TICKS_PER_DAY;
    while r.c.parties[band].phase != Phase::Waiting {
        assert!(r.c.now() < limit, "the envoy never arrived");
        r.run(r.c.now() + TICKS_PER_DAY / 4);
    }
    let x = &r.c.parties[band];
    assert_eq!(x.phase, Phase::Waiting);
    let arrived = x.times.last().copied().unwrap();
    let bored = x.bored_at;
    let waited_from = arrived.max(since);
    if phase == Phase::Out {
        assert!(
            arrived > since,
            "it arrived on Peaceful ({arrived:?} after {since:?})"
        );
    }
    r.run(r.c.now() + TICKS_PER_DAY);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), r.c.now() + 10);
    let left = r.c.now();
    assert_eq!(
        r.c.parties[band].bored_at,
        bored + left.since(waited_from),
        "patience moves by the time spent waiting on Peaceful"
    );
}

#[test]
fn an_envoy_waiting_when_peaceful_begins_has_its_patience_moved_by_the_whole_span() {
    bored_over_peaceful(43, Phase::Waiting);
}

#[test]
fn an_envoy_that_arrived_on_peaceful_has_its_patience_moved_from_its_arrival() {
    bored_over_peaceful(42, Phase::Out);
}

/// MZB-137 (review item 6): a tribute paid to an envoy whose demand is over (settled, expired or
/// withdrawn) is a gift. It used to fall through to the payer's, then the settlers' open demand:
/// `envoy_home` clears the envoy's feud, so the settlers' demand (made on behalf of the
/// villages) was paid, and settled, by a payment to a goblin who no longer asked for anything.
/// A payment at the camp (no envoy) still goes to the payer's demand, then the settlers'.
#[test]
fn a_late_tribute_to_an_envoy_does_not_settle_the_settlers_demand() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let d = r.c.feud(r.f, r.p).unwrap().demand.unwrap();
    // The killing goes on, so the player's demand expires.
    r.kill(1.0, r.c.now() + 5);
    r.watch = false;
    r.run(d.deadline + 1);
    assert_eq!(settled(&r), vec![boundary::SETTLED_EXPIRED]);
    assert!(r.c.feud(r.f, r.p).unwrap().demand.is_none());
    // The settlers have a demand of their own, open.
    let s = r.c.settlers(&mut r.w);
    let now = r.c.now();
    r.c.signal(
        &mut r.w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount: 1.0,
            actor: s,
            pos: synth::REFINERY_POS,
            district: Some(synth::REFINERY),
        },
        now + 1,
    );
    let tribute = 3;
    r.c.feud_mut(r.f, s).expect("the settlers' feud").demand = Some(Demand {
        kind: SignalKind::Killing,
        good: d.good,
        tribute,
        paid: 0,
        made: now,
        deadline: now + TICKS_PER_DAY,
        after: 1.0,
        envoy: None,
    });
    let gifts = r.c.stats.gifts;
    let at = r.c.now() + 10;
    pay_to(&mut r, e.agent, e.epoch, tribute as i32, d.good, 77, at);
    assert_eq!(r.c.stats.tributes, 0, "not a tribute");
    assert_eq!(r.c.stats.gifts, gifts + 1, "a gift");
    assert_eq!(
        settled(&r),
        vec![boundary::SETTLED_EXPIRED],
        "nothing settled"
    );
    let open = r.c.feud(r.f, s).unwrap().demand.expect("still open");
    assert_eq!(open.paid, 0, "and nothing counted on it");
    // The same payment at the camp is for the settlers' demand, as ever.
    let at = r.c.now() + 10;
    pay_to(&mut r, 0, 0, tribute as i32, d.good, 78, at);
    assert_eq!(r.c.stats.tributes, 1, "at the camp it is a tribute");
    assert_eq!(settled(&r).last(), Some(&boundary::SETTLED_PAID));
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rtsim-conflict-mowzie-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// MZB-131 (review): `recalled` is persisted per epoch, through both save paths (the whole-engine
/// `save` and the paged `save_paged`): every epoch keeps its own stacks, in order, and a
/// stack that is empty keeps its entry.
#[test]
fn a_band_recalled_at_several_epochs_keeps_every_stack_through_both_saves() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    let want = {
        let x = &mut r.c.parties[band];
        let full = x.units.clone();
        let less: Vec<(u8, u32)> = full
            .iter()
            .map(|&(u, n)| (u, n.saturating_sub(1)))
            .collect();
        let empty: Vec<(u8, u32)> = full.iter().map(|&(u, _)| (u, 0)).collect();
        x.epoch = 9;
        x.recalled = vec![(2, full), (5, less), (8, empty)];
        x.recalled.clone()
    };
    assert_eq!(want.len(), 3);
    // The whole-engine save.
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert_eq!(back.parties[band].recalled, want);
    assert_eq!(back.parties[band].epoch, 9);
    assert_eq!(back.state_hash(), r.c.state_hash());
    // The paged save, opened again from the store.
    let dir = tmpdir("recalled");
    let store = std::sync::Arc::new(std::sync::RwLock::new(
        rtsim_kernel::page::PageStore::open(&dir, rtsim_kernel::page::Durability::NoSync).unwrap(),
    ));
    r.c.attach_store(store.clone(), 64 << 20);
    let mut b = rtsim_kernel::page::SaveBatch::new();
    let t = r.c.save_paged(&mut b, rtsim_conflict::scenario::META_KEY);
    store.write().unwrap().commit(b, 0).unwrap();
    r.c.saved(t);
    let paged = Conflict::open_paged(
        store.clone(),
        64 << 20,
        rtsim_conflict::scenario::META_KEY,
        r.c.tun.clone(),
        r.c.cultures.clone(),
    )
    .unwrap()
    .expect("a meta page");
    assert_eq!(paged.parties[band].recalled, want);
    assert_eq!(paged.parties[band].epoch, 9);
    assert_eq!(paged.state_hash(), r.c.state_hash());
    drop(paged);
    drop(r);
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

/// Everything the plugin was told after `from`, as the fake plugin logs it, but for the
/// territories (a session start sends them again, docs/boundary.md §9.4).
fn told_since(r: &Run, from: Tick) -> Vec<(u64, String)> {
    r.plugin
        .commands
        .iter()
        .filter(|(t, c)| *t >= from.0 && !c.starts_with("TERRITORY"))
        .cloned()
        .collect()
}

/// MZB-132 (review): a restart in the middle of Peaceful, the world then leaving Peaceful from
/// the loaded engine, behaves as the engine that kept running: the same deadline, the same
/// expiry on the same tick, the same commands, not just the same hashes. The envoy held on its
/// road arrives on Peaceful, so its patience and its wait are in the save.
#[test]
fn a_restart_in_the_middle_of_peaceful_then_leaving_it_goes_on_as_the_run_that_kept_going() {
    let seed = 42;
    let mut a = Run::new(seed, quiet_goblins());
    a.kill(7.0, Tick(10));
    let mut t = a.c.now();
    loop {
        t += 100;
        assert!(t < day(4.0), "no envoy on its road");
        a.run(t);
        if envoy_of(&a).is_some_and(|(_, ph)| ph == Phase::Out) {
            break;
        }
    }
    let d0 = a.c.feud(a.f, a.p).unwrap().demand.unwrap().deadline;
    a.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), a.c.now() + 1);
    a.kill(1.0, a.c.now() + 5);
    a.run(a.c.now() + 2 * TICKS_PER_DAY);
    a.kill(1.0, a.c.now() + 5);
    a.run(a.c.now() + TICKS_PER_DAY);
    // The restart: the engine from its save, in its next session, over a copy of the world.
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let w = World::load(synth::world_config(seed), goods, book, &a.w.save()).unwrap();
    let mut c = Conflict::load(a.c.tun.clone(), a.c.cultures.clone(), &a.c.save()).unwrap();
    synth::define_names(&mut c, &w);
    c.session_start();
    let mut b = Run {
        w,
        c,
        f: a.f,
        p: a.p,
        plugin: FakeConflict::new(synth::PLAYER),
        cmds: Vec::new(),
        watch: false,
        next_obs: Tick::ZERO,
    };
    assert!(b.c.peaceful, "still Peaceful after the restart");
    assert_eq!(b.c.peaceful_since, a.c.peaceful_since);
    assert_eq!(b.c.now(), a.c.now());
    assert_eq!(envoy_of(&b).map(|e| e.1), envoy_of(&a).map(|e| e.1));
    // Both leave Peaceful on the same tick and are run past the moved deadline.
    let left = a.c.now() + 10;
    let span = left.since(a.c.peaceful_since);
    for r in [&mut a, &mut b] {
        r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), left);
        r.kill(1.0, left + 5);
    }
    let d1 = d0 + span;
    for r in [&mut a, &mut b] {
        assert_eq!(r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline, d1);
        r.run(Tick(d1.0 - 1));
        assert_eq!(r.c.stats.expired, 0, "not before the moved deadline");
        r.run(d1 + 4 * TICKS_PER_DAY);
        assert_eq!(r.c.stats.expired, 1, "at it");
    }
    assert_eq!(settled(&a), settled(&b));
    assert_eq!(settled(&b), vec![boundary::SETTLED_EXPIRED]);
    assert_eq!(a.step(), b.step());
    assert_eq!(a.c.stats.demands, b.c.stats.demands);
    assert_eq!(a.c.stats.peace_held, b.c.stats.peace_held);
    assert_eq!(
        told_since(&a, left),
        told_since(&b, left),
        "the same commands, tick for tick"
    );
    assert_eq!(a.c.state_hash(), b.c.state_hash());
}

/// Two Peaceful spans before a deadline (review): it moves by both, and the demand comes due
/// there and not before. Nothing expires inside either span or in the Normal time between them
/// that the deadline has not reached.
#[test]
fn two_peaceful_spans_move_a_deadline_by_both_and_it_comes_due_there() {
    let mut r = Run::new(43, quiet_goblins());
    r.kill(7.0, Tick(10));
    let mut t = r.c.now();
    while r.c.feud(r.f, r.p).unwrap().demand.is_none() {
        t += 100;
        assert!(t < day(4.0), "no demand");
        r.run(t);
    }
    let d0 = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    let t1 = r.c.now() + 10;
    let (t2, t3, t4) = (t1 + 1_000, t1 + 1_500, t1 + 3_500);
    assert!(t4 < d0, "both spans end before the deadline");
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), t1);
    r.kill(1.0, t1 + 5);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), t2);
    r.kill(1.0, t2 + 5);
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), t3);
    r.kill(1.0, t3 + 5);
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), t4);
    let d1 = d0 + 3_000;
    assert_eq!(r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline, d1);
    // The killing goes on (a signal that stopped would be cut, not expired).
    let mut at = r.c.now();
    while at + 600 < d1 {
        at += 600;
        r.kill(1.0, at);
        r.run(at + 1);
    }
    r.run(Tick(d1.0 - 1));
    assert_eq!(r.c.stats.expired, 0, "not before the moved deadline");
    r.kill(1.0, r.c.now() + 1);
    r.run(d1 + TICKS_PER_DAY);
    assert_eq!(r.c.stats.expired, 1, "at it");
    assert_eq!(settled(&r), vec![boundary::SETTLED_EXPIRED]);
}

/// CHIEF_KILLED for `band`, by a mob (no player).
fn chief_killed_by_mob(band: Handle<Party>, epoch: u32, op: u64) -> Obs {
    Obs::ChiefKilled(boundary::ChiefKilled {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_MOB,
        killer: (0, 0),
        op,
    })
}

/// The old chief dies of age while out with `band`, and the engine finds out (the lazy
/// succession, on the next feud wake).
fn chief_dies_of_age_out(r: &mut Run, band: Handle<Party>) -> ActorId {
    assert!(r.c.parties[band].chief, "the chief rides with the band");
    assert!(r.c.factions[r.f as usize].chief_out);
    let old = r.c.factions[r.f as usize].chief;
    let at = r.c.now() + 10;
    r.w.knowledge.die(old, at);
    let before = r.c.stats.successions;
    // Another feud's wake (the settlers' climb) asks for the chief.
    let settlers = r.c.settlers(&mut r.w);
    r.c.signal(
        &mut r.w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount: 7.0,
            actor: settlers,
            pos: synth::REFINERY_POS,
            district: Some(synth::REFINERY),
        },
        at + 5,
    );
    let limit = at + 6 * TICKS_PER_DAY;
    while r.c.stats.successions == before && r.c.now() < limit {
        r.run(r.c.now() + 600);
    }
    assert_eq!(r.c.stats.successions, before + 1, "the heir took over");
    assert!(
        r.c.parties
            .get(band)
            .is_some_and(|x| x.phase != Phase::Home),
        "the band is still out"
    );
    old
}

/// MZB-139 (TALK review): a chief that dies of age while out with a band is replaced by the lazy
/// succession, which cleared only the faction's `chief_out`; the band kept the chief flag, and a
/// later CHIEF_KILLED for it killed the new chief, who is at home. The band now carries on
/// without a chief: the flag is gone, the CHIEF_KILLED is stale (nothing counted, no rout, no
/// second succession), and the new chief lives.
#[test]
fn a_chief_dead_of_age_while_out_leaves_no_chief_flag_on_the_band() {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let old = chief_dies_of_age_out(&mut r, band);
    let new = r.c.factions[r.f as usize].chief;
    assert_ne!(new, old);
    assert!(!r.c.parties[band].chief, "the band's chief is gone");
    assert!(!r.c.factions[r.f as usize].chief_out);
    assert!(r.c.parties.contains(band), "the band carries on");
    let phase = r.c.parties[band].phase;
    // The old chief's body, still in Minecraft, is killed: nothing happens to the new chief.
    let successions = r.c.stats.successions;
    let at = r.c.now() + 10;
    r.ingest(chief_killed_by_mob(band, r.c.parties[band].epoch, 91), at);
    assert_eq!(r.c.stats.chiefs_killed, 0, "no chief fell");
    assert_eq!(r.c.stats.routs, 0, "no rout");
    assert_eq!(r.c.stats.successions, successions, "no second succession");
    assert_eq!(
        r.c.factions[r.f as usize].chief, new,
        "the new chief is the chief"
    );
    assert!(
        r.w.knowledge.notable(new).is_some(),
        "the new chief at home lives"
    );
    assert_eq!(r.c.parties[band].phase, phase, "the band carries on");
}

/// Control: with the chief alive, the same CHIEF_KILLED does kill him and the heir succeeds.
#[test]
fn a_chief_killed_with_his_band_still_falls_with_it() {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(20.0, Tick(10));
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let old = r.c.factions[r.f as usize].chief;
    let at = r.c.now() + 10;
    r.ingest(chief_killed_by_mob(band, r.c.parties[band].epoch, 92), at);
    assert_eq!(r.c.stats.chiefs_killed, 1);
    assert_ne!(r.c.factions[r.f as usize].chief, old, "the heir is chief");
    assert!(!r.c.parties[band].chief);
}

/// MZB-139: a band whose chief died of age before it materialized spawns without a chief body
/// (no BAND_CHIEF, no MEMBER_CHIEF), where the control, the chief alive, spawns him (MZB-109).
#[test]
fn a_band_whose_chief_died_of_age_materializes_without_a_chief_body() {
    let spawned = |dies: bool| {
        let mut r = Run::new(37, quiet_goblins());
        r.watch = true;
        r.kill(20.0, Tick(10));
        let band =
            r.c.order_party(
                &mut r.w,
                synth::CAMP,
                PartyKind::Raid,
                Target::District(synth::REFINERY, synth::REFINERY_POS),
                Some(r.p),
                Tick(20),
            )
            .unwrap();
        r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
        if dies {
            chief_dies_of_age_out(&mut r, band);
        }
        let limit = r.c.now() + 4 * TICKS_PER_DAY;
        let agent = Party::agent(band);
        let mut got = None;
        while got.is_none() && r.c.now() < limit {
            r.kill(20.0, r.c.now() + 5);
            r.run(r.c.now() + 600);
            got = r.cmds.iter().find_map(|(_, c)| match c {
                Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
                _ => None,
            });
        }
        got.expect("the band came into view")
    };
    let alive = spawned(false);
    assert!(
        alive.flags & boundary::BAND_CHIEF != 0,
        "control: the chief"
    );
    assert!(
        alive
            .members
            .iter()
            .any(|s| s.flags & boundary::MEMBER_CHIEF != 0),
        "control: his body"
    );
    let dead = spawned(true);
    assert_eq!(dead.flags & boundary::BAND_CHIEF, 0, "no chief flag");
    assert!(
        dead.members
            .iter()
            .all(|s| s.flags & boundary::MEMBER_CHIEF == 0),
        "no chief body"
    );
}

/// MZB-139: the band that came home with the stale flag cleared `chief_out` while the new
/// chief's own band was out, so a second band could carry the new chief at once. With the flag
/// gone it cannot.
#[test]
fn an_old_band_coming_home_does_not_free_the_new_chiefs_place() {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(20.0, Tick(10));
    let old_band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    chief_dies_of_age_out(&mut r, old_band);
    // The new chief rides out with another band.
    let new_band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            r.c.now() + 1,
        )
        .unwrap();
    assert!(r.c.parties[new_band].chief, "the new chief rides with it");
    assert!(r.c.factions[r.f as usize].chief_out);
    // The old band comes home, and the new chief is still out with his: his place stays taken.
    let mut seen = 0;
    let limit = r.c.now() + 8 * TICKS_PER_DAY;
    while r.c.now() < limit && r.c.parties.contains(new_band) {
        r.kill(1.0, r.c.now() + 5);
        r.run(r.c.now() + 100);
        if !r.c.parties.contains(old_band) && r.c.parties.contains(new_band) {
            seen += 1;
            assert!(
                r.c.factions[r.f as usize].chief_out,
                "the old band is home and the new chief's band is not"
            );
        }
    }
    assert!(seen > 0, "the old band came home first");
}

/// MZB-140 (TALK review): the heir's tape text tells a chief dead of age from one killed with no
/// player killer (a mob, `/kill`), and from one a player killed.
#[test]
fn the_heirs_text_tells_a_death_of_age_from_a_killing() {
    let text = |how: u8| {
        let mut r = Run::new(36, quiet_goblins());
        r.c.log_enabled = true;
        r.kill(20.0, Tick(10));
        let band =
            r.c.order_party(
                &mut r.w,
                synth::CAMP,
                PartyKind::Raid,
                Target::District(synth::REFINERY, synth::REFINERY_POS),
                Some(r.p),
                Tick(20),
            )
            .unwrap();
        r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
        match how {
            0 => {
                chief_dies_of_age_out(&mut r, band);
            }
            1 => {
                let at = r.c.now() + 10;
                r.ingest(chief_killed_by_mob(band, 0, 93), at);
            }
            _ => {
                let at = r.c.now() + 10;
                r.ingest(
                    Obs::ChiefKilled(boundary::ChiefKilled {
                        agent: Party::agent(band),
                        epoch: 0,
                        by: boundary::BY_PLAYER,
                        killer: synth::PLAYER,
                        op: 94,
                    }),
                    at,
                );
            }
        }
        let heirs: Vec<String> =
            r.c.log
                .iter()
                .filter(|l| l.kind == "heir")
                .map(|l| l.what.clone())
                .collect();
        assert_eq!(heirs.len(), 1, "one succession: {heirs:?}");
        heirs[0].clone()
    };
    let age = text(0);
    assert!(age.contains("dies of age"), "{age}");
    let mob = text(1);
    assert!(mob.contains("killed"), "{mob}");
    assert!(!mob.contains("age"), "a mob's kill is not old age: {mob}");
    assert!(mob.contains("no player"), "{mob}");
    let player = text(2);
    assert!(
        player.contains("killed") && player.contains("holding the grudge"),
        "{player}"
    );
    assert!(!player.contains("age"), "{player}");
}
