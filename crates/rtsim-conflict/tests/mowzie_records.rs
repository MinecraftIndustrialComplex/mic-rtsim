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
use rtsim_settle::{NodeKind, Pos};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::{Command, World};

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
    r.c.begin_replay(true);
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
    r.c.begin_replay(true);
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
    r.c.begin_replay(true);
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

/// When the plugin's watched fight in these tests begins and the war chief falls.
fn fight_at() -> Tick {
    Tick::from_days(1) + 100
}

/// A raid on `aim` with its war chief, serving the player's feud (the player killed 20 of the
/// camp's members), fighting the player at [`fight_at`] as the plugin's watched fight would have
/// it. A fighting band stands where its march ends, at its target (`party_chart_pos`), wherever
/// it was on the road when the plugin said the fight began.
fn band_in_a_fight(seed: u64, aim: impl FnOnce(&Run) -> Target) -> (Run, Handle<Party>) {
    band_in_a_fight_of(Run::new(seed, quiet_goblins()), aim)
}

/// [`band_in_a_fight`] in a run set up by the caller (a player base marked, say).
fn band_in_a_fight_of(mut r: Run, aim: impl FnOnce(&Run) -> Target) -> (Run, Handle<Party>) {
    r.kill(20.0, Tick(10));
    let target = aim(&r);
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            target,
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    assert!(r.c.parties[band].chief);
    r.c.run_until(&mut r.w, &mut NoInfrastructure, fight_at());
    r.c.parties[band].phase = Phase::Fighting;
    (r, band)
}

/// A raid on a place of the player's, as `chief_in_a_fight`'s, but `at` blocks away: the player's
/// refinery is 164 blocks from Ashby, just outside the 160 a rout is seen from.
fn raid_in_a_fight_at(seed: u64, at: Pos) -> (Run, Handle<Party>) {
    band_in_a_fight(seed, |_| Target::District(synth::REFINERY, at))
}

/// A raid on the refinery with its war chief, fighting the player a day after the player killed
/// 20 of the camp's members.
fn chief_in_a_fight(seed: u64) -> (Run, Handle<Party>) {
    raid_in_a_fight_at(seed, synth::REFINERY_POS)
}

/// A raid on a clearing 86 blocks from Ashby, in its view.
const BY_ASHBY: Pos = Pos::new(0, 650, 720);

/// A raid on a clearing 50 blocks from Mirewater, a ribbit village goblins do not hate.
const BY_MIREWATER: Pos = Pos::new(0, -480, -560);

/// A village of the synthetic world by its key, with where it is.
fn village(r: &Run, key: u64) -> (ActorId, Pos) {
    let n = &r.w.nodes[r.w.node_of_key(key).expect("a village of that key")];
    assert!(matches!(n.kind, NodeKind::Village(_)));
    (n.actor, n.pos)
}

const OAKFORD: u64 = 2;
const ASHBY: u64 = 4;
const MIREWATER: u64 = 7;

fn blocks_between(a: Pos, b: (i32, i32)) -> f64 {
    (f64::from(a.x - b.0)).hypot(f64::from(a.z - b.1))
}

/// Every village of the world: its actor, its place, and whether goblins hate its culture.
fn villages(r: &Run) -> Vec<(ActorId, Pos, bool)> {
    r.w.nodes
        .iter()
        .filter_map(|n| match n.kind {
            NodeKind::Village(cu) => {
                Some((n.actor, n.pos, ["human", "bandit"].contains(&cu.name())))
            }
            _ => None,
        })
        .collect()
}

/// The rescue reports the knowledge layer holds.
fn rescue_reports(r: &Run) -> Vec<rtsim_knowledge::ReportId> {
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
}

/// MZB-36: the heir who saw the player kill the war chief feels it at the killing's base delta
/// (-125), so the grudge it takes over is about 60 grievance (`heir_grudge_factor` 0.5 a point),
/// not 2.5. And the village that hates the camp and sees the rout (Ashby, as the band passes it)
/// hears of the rescue at the rescue's +25.
#[test]
fn a_chief_killed_counts_at_the_base_delta() {
    let (mut r, band) = raid_in_a_fight_at(9, BY_ASHBY);
    let at = fight_at();
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
    // The villages that hate goblins and see the rout, and what they feel before.
    let (ashby, ashby_at) = village(&r, ASHBY);
    assert!(
        blocks_between(ashby_at, r.c.party_pos(band, at)) <= f64::from(r.c.tun.view_radius),
        "the band breaks in view of Ashby"
    );
    let haters: Vec<(ActorId, i8)> = vec![(ashby, r.w.knowledge.sentiment(ashby, r.p, at))];
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

/// CL-D60 (K2-25: the villages witness "a rescue report") and CL-D61 (only real witnesses, PLAN.md
/// §8.7): one rout is one rescue report, however many villages hate the camp. The villages that
/// hate it and are in view of the rout (within `view_radius` of where the band broke) witness it
/// first-hand at the full +25; every other village, a hating one 1,000 blocks off included, does
/// not know it yet and hears it later, from travellers. A report per witness made every village
/// count one rout once per hating village (CL-D60), and the range rule gave every hating village
/// within 2,000 blocks of the camp the rout first-hand (CL-D61).
#[test]
fn a_rout_is_one_rescue_report_that_only_the_villages_in_view_witness() {
    let (mut r, band) = raid_in_a_fight_at(9, BY_ASHBY);
    let at = fight_at();
    assert!(rescue_reports(&r).is_empty(), "no rescue before the rout");
    let view = f64::from(r.c.tun.view_radius);
    let broke = r.c.party_pos(band, at);
    let (mut seen, mut unseen) = (Vec::new(), Vec::new());
    for (a, pos, hates) in villages(&r) {
        let d = blocks_between(pos, broke);
        if hates && d <= view {
            seen.push((a, r.w.knowledge.sentiment(a, r.p, at)));
        } else {
            unseen.push((a, hates, d));
        }
    }
    assert!(
        !seen.is_empty(),
        "a village that hates goblins sees the rout"
    );
    assert!(
        unseen.iter().any(|&(_, hates, d)| hates && d >= 1_000.0),
        "a village that hates goblins 1,000 blocks from the rout"
    );
    assert!(
        unseen.iter().any(|&(_, hates, _)| !hates),
        "a village that does not hate goblins"
    );
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    let reps = rescue_reports(&r);
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
    assert_eq!(
        (rep.place.frame, rep.place.x, rep.place.z),
        (0, broke.0, broke.1),
        "where the band broke"
    );
    for (a, was) in seen {
        assert!(
            r.w.knowledge.knows(a, reps[0]),
            "a hating village in view witnesses it"
        );
        let now = r.w.knowledge.sentiment(a, r.p, at);
        assert!(
            i32::from(now) >= i32::from(was) + 20,
            "first-hand, not hearsay: {was} to {now}"
        );
    }
    for &(a, hates, d) in &unseen {
        assert!(
            !r.w.knowledge.knows(a, reps[0]),
            "a village {d:.0} blocks off (hating the camp: {hates}) does not witness it"
        );
    }
    // The rest hear it later, from travellers: some village that did not see it learns it.
    r.w.run_until(at + 5 * TICKS_PER_DAY);
    assert!(
        unseen
            .iter()
            .any(|&(a, _, _)| r.w.knowledge.knows(a, reps[0])),
        "travellers carry the rescue to a village that did not see it"
    );
}

/// CL-D61: the village a band marched on witnesses its rescue, whatever its culture. Goblins do
/// not hate ribbits, and a raid on Mirewater was after Mirewater all the same: the band breaks at
/// its gate, Mirewater has the rescue first-hand at +25, and the report names it. (A hating
/// village in view would witness too: this is the case where only the rule for the village the
/// band marched on can put Mirewater among the witnesses.)
#[test]
fn a_rescued_village_witnesses_even_when_the_band_does_not_hate_its_culture() {
    let (mut r, band) = band_in_a_fight(9, |r| Target::Node(r.w.node_of_key(MIREWATER).unwrap()));
    let at = fight_at();
    let (mirewater, _) = village(&r, MIREWATER);
    assert!(
        villages(&r)
            .iter()
            .any(|&(a, _, hates)| a == mirewater && !hates),
        "goblins do not hate Mirewater's culture"
    );
    let was = r.w.knowledge.sentiment(mirewater, r.p, at);
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    assert_eq!(
        r.w.knowledge.report(reps[0]).unwrap().whom,
        mirewater,
        "the report names the village the band marched on"
    );
    assert!(
        r.w.knowledge.knows(mirewater, reps[0]),
        "Mirewater witnesses its own rescue"
    );
    let now = r.w.knowledge.sentiment(mirewater, r.p, at);
    assert!(
        i32::from(now) >= i32::from(was) + 20,
        "first-hand, not hearsay: {was} to {now}"
    );
}

/// CL-D61: the rescued village has no distance rule of its own. The engine cannot reach this
/// state today (a fighting band stands at its target, so the village it marched on is at the
/// rout), so the target is set by hand: a band that fights 800 blocks from Oakford with Oakford
/// its target. Oakford witnesses at +25 and no village that hates the camp sees the rout.
#[test]
fn the_rescued_village_witnesses_wherever_the_rout_is() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = fight_at();
    let (oakford, oakford_at) = village(&r, OAKFORD);
    r.c.parties[band].target = Target::Node(r.w.node_of_key(OAKFORD).unwrap());
    let broke = r.c.party_pos(band, at);
    let view = f64::from(r.c.tun.view_radius);
    assert!(
        blocks_between(oakford_at, broke) >= 700.0,
        "the rout is far from Oakford"
    );
    assert!(
        villages(&r)
            .iter()
            .all(|&(_, pos, _)| blocks_between(pos, broke) > view),
        "no village sees the rout"
    );
    let was = r.w.knowledge.sentiment(oakford, r.p, at);
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    assert!(
        r.w.knowledge.knows(oakford, reps[0]),
        "Oakford witnesses it"
    );
    let now = r.w.knowledge.sentiment(oakford, r.p, at);
    assert!(
        i32::from(now) >= i32::from(was) + 20,
        "first-hand, not hearsay: {was} to {now}"
    );
    for (a, _, hates) in villages(&r) {
        if a != oakford {
            assert!(
                !r.w.knowledge.knows(a, reps[0]),
                "no other village (hating the camp: {hates}) witnesses it"
            );
        }
    }
}

/// CL-D61: a village in view of the rout that does not hate the band did not witness a rescue
/// (one friendly to the band would be crediting the player for killing its friends). A raid
/// broken 50 blocks from Mirewater, marching on no village, has no witness: no report, and
/// Mirewater does not think better of the player.
#[test]
fn a_village_in_view_that_does_not_hate_the_band_does_not_witness_the_rout() {
    let (mut r, band) = raid_in_a_fight_at(9, BY_MIREWATER);
    let at = fight_at();
    let view = f64::from(r.c.tun.view_radius);
    let broke = r.c.party_pos(band, at);
    let (mirewater, mirewater_at) = village(&r, MIREWATER);
    assert!(
        blocks_between(mirewater_at, broke) <= view,
        "Mirewater sees the rout"
    );
    assert!(
        villages(&r)
            .iter()
            .all(|&(a, pos, hates)| a == mirewater || !hates || blocks_between(pos, broke) > view),
        "no village that hates goblins sees it"
    );
    let was = r.w.knowledge.sentiment(mirewater, r.p, at);
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    assert!(rescue_reports(&r).is_empty(), "no witness, no report");
    assert_eq!(r.w.knowledge.sentiment(mirewater, r.p, at), was);
}

/// CL-D61: the edge of a rout's view is `view_radius` of the raw distance, as
/// [`Conflict::watcher`] reads a player's: a hating village exactly that far from where the band
/// broke witnesses it, one block farther does not. Along an axis (160 yes, 161 no) and on a
/// diagonal, where a rounded distance would let 160.08 in: (100, 124) is 159.3 away, (100, 125)
/// is 160.08.
#[test]
fn a_rout_is_seen_from_exactly_the_view_radius() {
    let (_, ashby_at) = village(&Run::new(9, quiet_goblins()), ASHBY);
    let view = Run::new(9, quiet_goblins()).c.tun.view_radius as i32;
    for (dx, dz, seen) in [
        (view, 0, true),
        (view + 1, 0, false),
        (100, 124, true),
        (100, 125, false),
    ] {
        let (mut r, band) = raid_in_a_fight_at(9, Pos::new(0, ashby_at.x + dx, ashby_at.z + dz));
        let at = fight_at();
        // Where the band broke, before the distance is measured from it.
        let broke = r.c.party_pos(band, at);
        assert_eq!(
            broke,
            (ashby_at.x + dx, ashby_at.z + dz),
            "a fighting band stands at its target"
        );
        let d = blocks_between(ashby_at, broke);
        assert_eq!(d <= f64::from(view), seen, "{d:.2} blocks from Ashby");
        r.c.chief_killed(&mut r.w, band, Some(r.p), at);
        assert_eq!(
            rescue_reports(&r).len(),
            usize::from(seen),
            "a rout {d:.2} blocks from Ashby is seen from there: {seen}"
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
    answer_from(r, e.agent, e.epoch, answer, synth::PLAYER, at);
}

/// ENVOY_ANSWER from `player` at the party `agent`, `epoch`.
fn answer_from(r: &mut Run, agent: u64, epoch: u32, answer: u32, player: boundary::Uuid, at: Tick) {
    r.ingest(
        Obs::EnvoyAnswer(boundary::EnvoyAnswer {
            agent,
            epoch,
            answer,
            player,
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

/// MZB-144 (re-review P3-1): ENVOY_ANSWER is an envoy's. For a war band that has a feud (a stale or
/// misrouted record) it used to book the insult and refuse the feud's demand.
#[test]
fn an_attacked_answer_for_a_war_band_does_nothing() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    let (agent, epoch) = (Party::agent(band), r.c.parties[band].epoch);
    assert_eq!(
        r.c.parties[band].feud,
        Some(r.p),
        "it serves the player's feud"
    );
    let lambda = r.c.cultures.get(0).lambda;
    let at = r.c.now() + 5;
    let g = |r: &Run| r.c.feud(r.f, r.p).unwrap().g.total_at(at + 100, lambda);
    let before = g(&r);
    answer_from(
        &mut r,
        agent,
        epoch,
        boundary::ANSWER_ATTACKED,
        synth::PLAYER,
        at,
    );
    assert_eq!(g(&r), before, "no insult booked");
    assert!(settled(&r).is_empty(), "nothing refused");
}

/// MZB-145 (re-review P3-2): a second player's blow on an envoy that is not theirs is their own
/// insult, at the envoy's place, on their own feud and against no district. It refuses nothing:
/// the demand is still to its player, who can still pay it, and whose own blow refuses it as ever.
/// The old rule used the envoy's feud as the actor, so a stranger's blow refused the player's demand
/// and booked the insult at the player's district.
#[test]
fn a_blow_from_another_player_is_their_insult_and_leaves_the_demand_alone() {
    let stranger: boundary::Uuid = (0x2222_0000_0000_0002, 0x9999_0000_0000_0009);
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.answer = Answer::Ignore;
    let e = envoy_met(&mut r);
    let q = r.c.player(&mut r.w, stranger);
    assert!(
        r.c.feud(r.f, q).is_none(),
        "the stranger has no quarrel yet"
    );
    let lambda = r.c.cultures.get(0).lambda;
    let at = r.c.now() + 5;
    let owner = |r: &Run| r.c.feud(r.f, r.p).unwrap().g.total_at(at + 100, lambda);
    let before = owner(&r);
    answer_from(
        &mut r,
        e.agent,
        e.epoch,
        boundary::ANSWER_ATTACKED,
        stranger,
        at,
    );
    assert!(settled(&r).is_empty(), "the player's demand is not refused");
    let d = r.c.feud(r.f, r.p).unwrap().demand.expect("still open");
    assert_eq!(d.paid, 0);
    assert_eq!(owner(&r), before, "nor does the insult land on the player");
    let theirs = r.c.feud(r.f, q).expect("the stranger's own feud");
    let g = theirs.g.total_at(at + 100, lambda);
    assert!(g > 0.0, "the insult is held against the stranger: {g}");
    let focus = theirs.focus.expect("a focus at the envoy's place");
    assert_eq!(focus.district, None, "against no district");
    assert_ne!(focus.pos, synth::CAMP_POS, "and not at the camp");
    // The player's own blow refuses the demand, as it always did.
    answer_from(
        &mut r,
        e.agent,
        e.epoch,
        boundary::ANSWER_ATTACKED,
        synth::PLAYER,
        at + 5,
    );
    assert_eq!(settled(&r), vec![boundary::SETTLED_REFUSED]);
    assert_eq!(
        r.c.feud(r.f, r.p).unwrap().focus.unwrap().district,
        Some(synth::REFINERY),
        "at the demand's district"
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
    c.begin_replay(true);
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
    assert!(back.replaying());
    back.resend_notices();
    assert!(!back.replaying());
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
/// the bodies the pages held in the world stay embodied at their epoch, with their stack, for
/// the replayed records (docs/replay-gate.md §5.1, RP-5; no fold into a recalled stack, RP-10).
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
    assert!(
        back.parties[band].embodied,
        "the bodies in the world are kept for the replay"
    );
    assert_eq!(back.parties[band].epoch, epoch);
    assert_eq!(back.parties[band].spawned, r.c.parties[band].spawned);
    assert!(back.parties[band].recalled.is_empty(), "nothing folded");
    // One death reported, then one more that died unreported before the chunks unloaded.
    let despawn = despawn_with(&r, band, epoch, 2);
    for c in [&mut same, &mut back] {
        c.ingest(&mut r.w, &mut NoInfrastructure, &casualty, r.c.now() + 1);
        c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, r.c.now() + 2);
        assert!(!c.parties[band].embodied, "the despawn ends the body");
        assert!(c.parties[band].spawned.is_empty());
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

/// docs/replay-gate.md §5.1 and §5.6 (RP-5, RP-10): the replay's start changes no party; its end
/// makes every party abstract with what the replay left. A band watched and fighting at the
/// crash, with a stack still walking off from an older epoch: kept as the pages had it while
/// replaying, then not embodied, nobody watching, nothing in Minecraft, no stack walking off,
/// and the fight goes on abstractly (Waiting). Its members are all alive throughout.
#[test]
fn the_replay_keeps_the_bodies_and_its_end_makes_them_abstract() {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.recalled = vec![(x.epoch, x.units.clone())];
        x.epoch += 1;
        x.embodied = true;
        x.seen_by = Some(r.p);
        x.spawned = x.units.clone();
        x.phase = Phase::Fighting;
    }
    let kept = r.c.parties[band].clone();
    let alive = kept.alive();
    let mut back = restarted(&r);
    assert!(back.replaying());
    assert_eq!(
        back.parties[band], kept,
        "the replay's start changes no party"
    );
    back.replay_done();
    assert!(!back.replaying());
    let x = &back.parties[band];
    assert!(!x.embodied);
    assert_eq!(x.seen_by, None);
    assert!(x.spawned.is_empty());
    assert!(
        x.recalled.is_empty(),
        "nothing of an ended session reports again"
    );
    assert_eq!(x.phase, Phase::Waiting);
    assert_eq!(x.epoch, kept.epoch);
    assert_eq!(x.alive(), alive);
    // A second end changes nothing (the host's resend_notices calls it again).
    let after = back.parties[band].clone();
    back.resend_notices();
    assert_eq!(back.parties[band], after);
}

/// A replayed BAND_MATERIALIZED for `band` at `epoch`, `wave`, with `spawned` placed at once.
fn materialized(band: Handle<Party>, epoch: u32, wave: u32, spawned: u32) -> Obs {
    Obs::Materialized(boundary::BandMaterialized {
        agent: Party::agent(band),
        epoch,
        dimension: 0,
        wave,
        spawned,
    })
}

/// The first BAND_MATERIALIZE of `band` the engine sends while the player watches its target:
/// runs the engine and the plugin up to four days.
fn first_materialize(r: &mut Run, band: Handle<Party>) -> Option<boundary::BandMaterialize> {
    r.watch = true;
    let agent = Party::agent(band);
    let from = r.cmds.len();
    let limit = r.c.now() + 4 * TICKS_PER_DAY;
    while r.c.now() < limit {
        r.run(r.c.now() + 600);
        let got = r.cmds[from..].iter().find_map(|(_, c)| match c {
            Cmd::Materialize(m) if m.agent == agent => Some(m.clone()),
            _ => None,
        });
        if got.is_some() {
            return got;
        }
    }
    None
}

/// docs/replay-gate.md §5.3 (RP-7, RP-30): a fresh engine (not from its pages, RP-31) replays a
/// BAND_MATERIALIZED at epoch 4 of a party at epoch 0. It does not embody, nor raise the epoch
/// while replaying (a body at an older epoch would lose its records); the floor holds 4. At the
/// replay end the floor becomes the epoch, and the next body takes 5. Control: no such record,
/// the next body takes 1.
#[test]
fn a_fresh_engines_replayed_materialize_raises_the_epoch_floor_only() {
    let next_body = |record: bool| {
        let mut r = Run::new(36, quiet_goblins());
        let band = raid_band(&mut r);
        r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
        assert_eq!(r.c.parties[band].epoch, 0);
        r.c.begin_replay(false);
        if record {
            let at = r.c.now() + 1;
            r.ingest(materialized(band, 4, 0, 6), at);
            let x = &r.c.parties[band];
            assert!(!x.embodied, "a fresh engine's record embodies nothing");
            assert!(x.spawned.is_empty());
            assert_eq!(x.epoch, 0, "the floor, not the epoch");
        }
        r.c.replay_done();
        let after = r.c.parties[band].epoch;
        let m = first_materialize(&mut r, band).expect("the band came into view");
        (after, m.epoch)
    };
    assert_eq!(next_body(false), (0, 1), "control");
    assert_eq!(next_body(true), (4, 5));
}

/// A raid band of 26 members of its first unit's body, abstract at epoch 0.
fn band_of_26(r: &mut Run) -> (Handle<Party>, u32) {
    let band = raid_band(r);
    let x = &mut r.c.parties[band];
    let u = x.units[0].0;
    x.units = vec![(u, 26)];
    x.start_units = 26;
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(u).body)
    };
    (band, body)
}

fn placed(c: &Conflict, band: Handle<Party>) -> u32 {
    c.parties[band].spawned.iter().map(|s| s.1).sum()
}

/// docs/replay-gate.md §5.2 (RP-6, RP-22): a replayed BAND_MATERIALIZED on an engine from its
/// pages embodies the party at the record's epoch with the quota's members (16 of 26), not every
/// alive member; a live one changes nothing. At a newer epoch an older embodied epoch's bodies
/// walk off as its recalled stack; a record below the party's wave, or of an older epoch,
/// changes nothing.
#[test]
fn a_replayed_materialize_embodies_with_what_java_was_commanded() {
    let mut r = Run::new(36, quiet_goblins());
    let (band, _) = band_of_26(&mut r);
    // Live: nothing.
    let mut live = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    let at = r.c.now() + 1;
    live.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 1, 0, 16),
        at,
    );
    assert!(
        !live.parties[band].embodied,
        "a live record embodies nothing"
    );
    assert_eq!(live.parties[band].epoch, 0);
    // Replayed, from the pages.
    let mut back = restarted(&r);
    back.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 1, 0, 16),
        at,
    );
    let x = &back.parties[band];
    assert!(x.embodied && x.epoch == 1 && x.wave == 1);
    assert_eq!(x.seen_by, None);
    assert_eq!(placed(&back, band), 16, "the quota, not all 26");
    // Below its wave, and an older epoch: nothing.
    back.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 1, 0, 16),
        at + 1,
    );
    back.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 0, 3, 16),
        at + 2,
    );
    assert_eq!((back.parties[band].epoch, back.parties[band].wave), (1, 1));
    assert_eq!(placed(&back, band), 16);
    // A newer epoch: epoch 1's bodies walk off as its stack, epoch 2 has the quota.
    back.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 2, 0, 16),
        at + 3,
    );
    let x = &back.parties[band];
    assert_eq!((x.epoch, x.wave), (2, 1));
    assert_eq!(x.recalled.len(), 1);
    assert_eq!(x.recalled[0].0, 1);
    assert_eq!(x.recalled[0].1.iter().map(|s| s.1).sum::<u32>(), 16);
    assert_eq!(placed(&back, band), 16);
}

/// RP-22: the same epoch's next wave, replayed. The pages hold the band embodied at epoch 1,
/// wave 0 placed, 20 alive and 10 of them in Minecraft (6 died before the save). Its wave 1,
/// decided after the save, is filled from the room the quota leaves: 6. With 2 more deaths in
/// view journaled before the record, the live engine may have placed 6 or 8 (the deaths read or
/// not); the honour counts them as placed and fills 6, never above what Java was commanded.
#[test]
fn a_same_epoch_wave_is_filled_never_above_what_java_was_commanded() {
    let fill = |deaths: u32| {
        let mut r = Run::new(36, quiet_goblins());
        let (band, body) = band_of_26(&mut r);
        {
            let x = &mut r.c.parties[band];
            let u = x.units[0].0;
            x.units = vec![(u, 20)];
            x.epoch = 1;
            x.embodied = true;
            x.spawned = vec![(u, 10)];
            x.wave = 1;
        }
        let mut back = restarted(&r);
        let at = r.c.now() + 1;
        if deaths > 0 {
            back.ingest(
                &mut r.w,
                &mut NoInfrastructure,
                &Obs::Casualties(boundary::BandCasualties {
                    agent: Party::agent(band),
                    epoch: 1,
                    by: boundary::BY_MOB,
                    body,
                    count: deaths,
                    killer: (0, 0),
                }),
                at,
            );
        }
        let before = placed(&back, band);
        back.ingest(
            &mut r.w,
            &mut NoInfrastructure,
            &materialized(band, 1, 1, 6),
            at + 1,
        );
        let x = &back.parties[band];
        assert_eq!((x.epoch, x.wave), (1, 2));
        (before, placed(&back, band), x.alive())
    };
    assert_eq!(fill(0), (10, 16, 20), "the room: 16 less 10 in Minecraft");
    assert_eq!(fill(2), (8, 14, 18), "6 more, not 8");
}

/// MZB-142 (re-review P2-2): a player's killing of a band on its walk-off, after it was recalled
/// and before its despawn, counts the same live as in a replay. The live run used to return at
/// once (the band is not embodied), so the replay of the same record counted a death, a signal
/// and a breaker the session itself never did.
#[test]
fn a_death_on_the_walk_off_counts_the_same_live_and_replayed() {
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
    let killing = Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_PLAYER,
        body,
        count: 2,
        killer: synth::PLAYER,
    });
    // The restart that replays it: the bodies the pages held stay embodied (RP-5).
    let mut replayed = restarted(&r);
    assert!(replayed.replaying());
    // The session that went on: the player left, the band was recalled, its bodies walk off.
    let mut live = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    {
        let x = &mut live.parties[band];
        x.recalled = vec![(epoch, std::mem::take(&mut x.spawned))];
        x.embodied = false;
    }
    assert!(!live.replaying());
    let at = r.c.now() + 1;
    for c in [&mut live, &mut replayed] {
        c.ingest(&mut r.w, &mut NoInfrastructure, &killing, at);
    }
    let grievance = |c: &Conflict| c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert_eq!(live.stats.casualties, 2, "the walk-off's dead are counted");
    assert_eq!(replayed.stats.casualties, live.stats.casualties);
    assert_eq!(live.parties[band].alive(), alive - 2);
    assert_eq!(replayed.parties[band].alive(), live.parties[band].alive());
    assert_eq!(live.parties[band].breaker, Some(r.p), "the killer is known");
    assert_eq!(replayed.parties[band].breaker, live.parties[band].breaker);
    assert!(
        grievance(&live) > grievance(&r.c),
        "and the killing is held against them"
    );
    assert_eq!(grievance(&replayed), grievance(&live));
    // The despawn then counts only the dead nobody reported: no double count, live or replayed.
    let despawn = despawn_with(&r, band, epoch, 3);
    for c in [&mut live, &mut replayed] {
        c.ingest(&mut r.w, &mut NoInfrastructure, &despawn, at + 1);
        assert_eq!(c.parties[band].alive(), alive - 3);
        assert!(c.parties[band].recalled.is_empty());
    }
    // A straggler after the despawn is no death.
    for c in [&mut live, &mut replayed] {
        c.ingest(&mut r.w, &mut NoInfrastructure, &killing, at + 2);
        assert_eq!(c.parties[band].alive(), alive - 3);
    }
}

/// A band recalled at epoch `e`, its bodies walking off, and seen again and embodied at `e + 1`
/// before that despawn (MZB-131): both stacks hold every alive member. Returns the band, `e` and
/// the body id of its first unit.
fn walking_off_and_embodied_again(r: &mut Run) -> (Handle<Party>, u32, u32) {
    let band = raid_band(r);
    let e = {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.recalled = vec![(x.epoch, x.units.clone())];
        x.epoch += 1;
        x.embodied = true;
        x.spawned = x.units.clone();
        x.epoch - 1
    };
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(r.c.parties[band].units[0].0).body)
    };
    (band, e, body)
}

fn casualty_of(band: Handle<Party>, epoch: u32, body: u32) -> Obs {
    Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_MOB,
        body,
        count: 1,
        killer: (0, 0),
    })
}

/// RP-27 (MZB-131, docs/replay-gate.md §5.9): a casualty finds its stack by the record's epoch. A
/// death on the walk-off of epoch `e` while the band is embodied again at `e + 1` takes from the
/// recalled stack of `e` and leaves `spawned` whole; one of `e + 1` takes from `spawned`. Before
/// it the death of `e` was dropped (`party_of` wants the current epoch), and the despawn of `e`
/// then counted that member as a survivor.
#[test]
fn a_walk_off_death_of_an_older_epoch_counts_against_its_own_stack() {
    let mut r = Run::new(36, quiet_goblins());
    let (band, e, body) = walking_off_and_embodied_again(&mut r);
    let alive = r.c.parties[band].alive();
    let first = r.c.parties[band].units[0];
    let at = r.c.now() + 1;
    r.ingest(casualty_of(band, e, body), at);
    let x = &r.c.parties[band];
    assert_eq!(x.alive(), alive - 1, "the walk-off's death is counted");
    assert_eq!(
        x.recalled,
        vec![(e, {
            let mut s = x.units.clone();
            s[0].1 = first.1 - 1;
            s
        })],
        "against the recalled stack of its epoch"
    );
    assert_eq!(
        x.spawned[0], first,
        "and the newer epoch's bodies are whole"
    );
    assert_eq!(r.c.stats.casualties, 1);
    r.ingest(casualty_of(band, e + 1, body), at + 1);
    let x = &r.c.parties[band];
    assert_eq!(x.alive(), alive - 2);
    assert_eq!(
        x.spawned[0],
        (first.0, first.1 - 1),
        "a death of e + 1 takes from spawned"
    );
    assert_eq!(
        x.recalled[0].1[0],
        (first.0, first.1 - 1),
        "and not from e's stack"
    );
    assert_eq!(r.c.stats.casualties, 2);
}

/// RP-27's control: deaths of the newer epoch only take from `spawned`, as they did before.
#[test]
fn a_death_of_the_newer_epoch_counts_against_spawned_alone() {
    let mut r = Run::new(36, quiet_goblins());
    let (band, e, body) = walking_off_and_embodied_again(&mut r);
    let alive = r.c.parties[band].alive();
    let first = r.c.parties[band].units[0];
    let at = r.c.now() + 1;
    r.ingest(casualty_of(band, e + 1, body), at);
    r.ingest(casualty_of(band, e + 1, body), at + 1);
    let x = &r.c.parties[band];
    assert_eq!(x.alive(), alive - 2);
    assert_eq!(x.spawned[0], (first.0, first.1 - 2));
    assert_eq!(
        x.recalled,
        vec![(e, {
            let mut s = x.units.clone();
            s[0].1 = first.1;
            s
        })],
        "the walk-off's stack is untouched"
    );
    assert_eq!(r.c.stats.casualties, 2);
}

/// A raid band of 6 members of its first unit's body, embodied at epoch 2 with all 6 placed.
fn six_placed(r: &mut Run) -> (Handle<Party>, u32) {
    let band = raid_band(r);
    let x = &mut r.c.parties[band];
    let u = x.units[0].0;
    x.units = vec![(u, 6)];
    x.start_units = 6;
    x.epoch = 2;
    x.embodied = true;
    x.spawned = vec![(u, 6)];
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(u).body)
    };
    (band, body)
}

/// docs/replay-gate.md §5.7 (RP-9, RP-25): a copied BAND_CASUALTIES (an ended session's record,
/// S4p's REASSERT) of a band embodied again with all 6 placed counts the death against its alive
/// units and caps `spawned` at them. Java's despawn of the 6 it holds then leaves 5: without the
/// cap `spawned` stays 6 and the despawn gives `5 - 6 + min(6, 6)` = 6, the death undone. Control:
/// a live death in view and a despawn of 5 survivors, 5.
#[test]
fn a_copied_casualty_is_not_undone_by_the_next_despawn() {
    let mut r = Run::new(36, quiet_goblins());
    let (band, body) = six_placed(&mut r);
    let at = r.c.now() + 1;
    let copy = casualty_of(band, 1, body);
    r.c.ingest_copy(&mut r.w, &mut NoInfrastructure, &copy, at);
    assert_eq!(r.c.parties[band].alive(), 5, "the copy counts");
    assert_eq!(r.c.stats.casualties, 1);
    let despawn = despawn_with(&r, band, 2, 0);
    let Obs::Dematerialized(d) = &despawn else {
        unreachable!()
    };
    assert_eq!(d.survivors[0].1, 5);
    // Java holds 6 bodies of epoch 2: all 6 come back.
    let mut six = d.clone();
    six.survivors[0].1 = 6;
    r.ingest(Obs::Dematerialized(six), at + 1);
    let x = &r.c.parties[band];
    assert!(!x.embodied);
    assert_eq!(x.alive(), 5, "the copied death is not undone");

    // Control: the death in view, live, and 5 survivors.
    let mut r = Run::new(36, quiet_goblins());
    let (band, body) = six_placed(&mut r);
    let at = r.c.now() + 1;
    r.ingest(casualty_of(band, 2, body), at);
    assert_eq!(r.c.parties[band].alive(), 5);
    let despawn = despawn_with(&r, band, 2, 0);
    r.ingest(despawn, at + 1);
    assert_eq!(r.c.parties[band].alive(), 5, "control");
}

/// docs/replay-gate.md §2.3 (the conflict copy gap, RP-8, RP-9): a copied BAND_CASUALTIES names
/// a body of an ended session. Replayed on an engine from its pages, it counts once against a
/// band that is abstract (no stack to take from, and its epoch floor rises to the record's, so the
/// replay end leaves the band at epoch 3), and against one embodied again at a newer epoch, whose
/// stack is untouched (3 placed of 5 alive). Control: the same record through `ingest` is a
/// straggler both times (the band's epoch is not the record's and no recalled stack has it).
#[test]
fn a_copied_casualty_counts_against_an_abstract_band_once() {
    let mut r = Run::new(36, quiet_goblins());
    let (band, body) = six_placed(&mut r);
    let u = r.c.parties[band].units[0].0;
    {
        let x = &mut r.c.parties[band];
        x.epoch = 0;
        x.embodied = false;
        x.spawned.clear();
    }
    let at = r.c.now() + 1;
    let record = casualty_of(band, 3, body);
    for copy in [true, false] {
        let mut back = restarted(&r);
        if copy {
            back.ingest_copy(&mut r.w, &mut NoInfrastructure, &record, at);
        } else {
            back.ingest(&mut r.w, &mut NoInfrastructure, &record, at);
        }
        let x = &back.parties[band];
        assert_eq!(x.alive(), if copy { 5 } else { 6 }, "abstract, copy {copy}");
        assert!(x.spawned.is_empty() && x.recalled.is_empty());
        assert_eq!(back.stats.casualties, u64::from(copy));
        back.replay_done();
        let x = &back.parties[band];
        assert_eq!(x.alive(), if copy { 5 } else { 6 }, "counted once");
        assert_eq!(x.epoch, if copy { 3 } else { 0 }, "the copy's floor");
    }
    {
        let x = &mut r.c.parties[band];
        x.epoch = 4;
        x.embodied = true;
        x.spawned = vec![(u, 3)];
    }
    for copy in [true, false] {
        let mut back = restarted(&r);
        if copy {
            back.ingest_copy(&mut r.w, &mut NoInfrastructure, &record, at);
        } else {
            back.ingest(&mut r.w, &mut NoInfrastructure, &record, at);
        }
        let x = &back.parties[band];
        assert_eq!(x.alive(), if copy { 5 } else { 6 }, "embodied, copy {copy}");
        assert!(x.embodied && x.epoch == 4);
        assert_eq!(
            x.spawned,
            vec![(u, 3)],
            "the newer epoch's stack is untouched"
        );
        assert_eq!(back.stats.casualties, u64::from(copy));
    }
}

/// The abstract district raids an infrastructure was asked for (`Infrastructure::raid`): one
/// tick per raid, whatever its classes.
#[derive(Default)]
struct Walls {
    raids: std::collections::BTreeSet<Tick>,
}

impl rtsim_ecology::infra::Infrastructure for Walls {
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
        t: &rtsim_industry::threat::Threat,
    ) -> Option<rtsim_ecology::infra::RaidOutcome> {
        self.raids.insert(t.at);
        None
    }
    fn good_name(&self, _: rtsim_industry::ids::Good) -> Option<String> {
        None
    }
    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        _: Tick,
        _: Tick,
    ) -> Vec<rtsim_ecology::infra::TrainSighting> {
        Vec::new()
    }
}

/// [`Run::run`] on `infra`, the plugin's records also kept in `journal` with when Java wrote
/// them.
fn run_journaled(
    r: &mut Run,
    infra: &mut dyn rtsim_ecology::infra::Infrastructure,
    to: Tick,
    journal: &mut Vec<(Tick, Obs)>,
) {
    let mut t = r.c.now();
    while t < to {
        let next = [
            r.c.next_due(),
            r.plugin.next_due(),
            r.watch.then_some(r.next_obs),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(to)
        .clamp(t + 1, to);
        for (at, o) in r.plugin.due(next) {
            r.c.ingest(&mut r.w, infra, &o, at);
            journal.push((at, o));
        }
        r.c.run_until(&mut r.w, infra, next);
        if r.watch && next >= r.next_obs {
            r.c.observe_player(&mut r.w, synth::PLAYER, synth::REFINERY_POS, next);
            r.next_obs = next + 1_200;
        }
        r.flush(next);
        t = next;
    }
}

/// What a raid's arrival came to: the abstract raids (by when), the casualties counted, the
/// raids fought in view, and the camp's pool on day 9 (its closed form, so that a band pooled
/// earlier or later, or with other members, shows).
#[derive(Debug, PartialEq)]
struct Arrival {
    raids: Vec<Tick>,
    casualties: u64,
    watched: u64,
    pool: f64,
}

/// RP-23 (the review's amendment b, docs/replay-gate.md §6.1): a raid on the refinery, the pages
/// saved while it marches there unseen. `watched`: the player stands at the refinery from the
/// save on, so the band is fought there (a quarter of each wave at the base falls) and walks
/// home. The session goes on until the band is home. `crash`: the session ends there instead,
/// and a restart from the pages replays the plugin's records (the player's position forgotten,
/// as the arm forgets it), ends its replay and runs on two days with nobody near. `restart`:
/// also a second restart, from pages saved once the replay holds the arrival, whose clock then
/// runs that many ticks before the next record (past the poll: it arrives again first).
fn a_raid_arriving_after_the_save(watched: bool, crash: bool, restart: Option<u64>) -> Arrival {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.fight = Some(synth::Fight {
        kill_share: 0.25,
        after: 1_500,
        kill_chief: false,
        only_at_base: true,
    });
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let f = r.f;
    let now = r.c.now();
    r.c.factions[f as usize].pool = 10.0;
    r.c.factions[f as usize].pool_at = now;
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    let x = &r.c.parties[band];
    assert!(x.phase == Phase::Out && !x.embodied, "on its road, unseen");
    let pages = r.c.save();
    let (casualties, raids_watched) = (r.c.stats.casualties, r.c.stats.raids_watched);
    journal.clear();
    r.watch = watched;
    r.next_obs = r.c.now();
    let mut end = arrival;
    while r.c.parties.get(band).is_some() {
        end += 600;
        assert!(end < day(8.0), "the band comes home");
        run_journaled(&mut r, &mut walls, end, &mut journal);
    }
    // Java journals other records all along: one past the band's, so that the replayed window
    // holds its arrival whatever the band's records are.
    journal.push((end, fillers_casualty()));
    let arrival_of = |c: &Conflict, walls: &Walls| Arrival {
        raids: walls.raids.iter().copied().collect(),
        casualties: c.stats.casualties - casualties,
        watched: c.stats.raids_watched - raids_watched,
        pool: c.pool_at(f, day(9.0)),
    };
    if !crash {
        return arrival_of(&r.c, &walls);
    }
    r.w.forget_player_pos(r.p);
    r.watch = false;
    let tun = r.c.tun.clone();
    let cultures = r.c.cultures.clone();
    let mut back = Conflict::load(tun.clone(), cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    let mut restarted_once = restart.is_none();
    for (at, o) in &journal {
        back.run_until(&mut r.w, &mut walls, *at);
        if let Some(x) = back.parties.get(band) {
            assert!(
                (x.seg as usize) < x.route.len(),
                "seg past the route: {} of {}",
                x.seg,
                x.route.len()
            );
        }
        if !restarted_once && back.now() > arrival {
            // Pages saved while the replay holds the arrival, and a restart from them.
            restarted_once = true;
            let phase = back.parties.get(band).map(|x| x.phase);
            assert_eq!(phase, Some(Phase::Out), "held, not resolved");
            assert!(walls.raids.is_empty());
            let x = &back.parties[band];
            assert_eq!(x.seg as usize + 2, x.route.len(), "on its last segment");
            let target = *x.route.last().unwrap();
            assert_eq!(back.party_pos(band, back.now()), target, "where it arrived");
            back = Conflict::load(tun.clone(), cultures.clone(), &back.save()).unwrap();
            back.begin_replay(true);
            let to = back.now() + restart.unwrap_or(0);
            back.run_until(&mut r.w, &mut walls, to);
            assert_eq!(back.parties[band].phase, Phase::Out, "held again");
        }
        back.ingest(&mut r.w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(restarted_once);
    let replay_end = back.now();
    back.resend_notices();
    assert!(!back.replaying());
    back.run_until(&mut r.w, &mut walls, end + 2 * TICKS_PER_DAY);
    let mut a = arrival_of(&back, &walls);
    if !watched {
        assert!(
            a.raids.iter().all(|&t| t >= replay_end),
            "after the replay end ({replay_end:?}): {a:?}"
        );
        // Resolved after the replay end, the band comes home later than in the session that
        // went on: its pool is not compared.
        a.pool = f64::NAN;
    }
    a
}

/// A filler record: a casualty of a band the engine never had.
fn fillers_casualty() -> Obs {
    Obs::Casualties(boundary::BandCasualties {
        agent: 0,
        epoch: 0,
        by: boundary::BY_MOB,
        body: 0,
        count: 1,
        killer: (0, 0),
    })
}

/// RP-23 (amendment b): a raid reaching a watched target in the replayed window, its
/// BAND_MATERIALIZED and casualties replayed: resolved once, by the fight (no abstract raid), its
/// casualties counted, and its pool as the session that went on left it.
#[test]
fn a_watched_arrival_in_the_replayed_window_is_resolved_once() {
    let live = a_raid_arriving_after_the_save(true, false, None);
    assert!(live.raids.is_empty(), "fought, not raided: {live:?}");
    assert_eq!(live.watched, 1);
    assert!(live.casualties >= 2, "{live:?}");
    assert_eq!(a_raid_arriving_after_the_save(true, true, None), live);
}

/// RP-23: the same with a restart from pages saved while the replay holds the arrival: its
/// fight's record comes before the poll arrives again (found by the pages' `seg` and route), and
/// after it (the map again).
#[test]
fn a_held_arrival_survives_a_restart_from_pages_saved_mid_replay() {
    let live = a_raid_arriving_after_the_save(true, false, None);
    assert_eq!(a_raid_arriving_after_the_save(true, true, Some(0)), live);
    assert_eq!(a_raid_arriving_after_the_save(true, true, Some(50)), live);
}

/// RP-23's control: an unwatched arrival in the window resolves abstractly once, after the
/// replay end (and once in the session that went on).
#[test]
fn an_unwatched_arrival_in_the_replayed_window_is_resolved_once_after_the_replay_end() {
    let live = a_raid_arriving_after_the_save(false, false, None);
    assert_eq!(live.raids.len(), 1, "{live:?}");
    assert_eq!((live.watched, live.casualties), (0, 0));
    let back = a_raid_arriving_after_the_save(false, true, None);
    assert_eq!(back.raids.len(), 1, "{back:?}");
    assert_eq!((back.watched, back.casualties), (0, 0));
}

/// RP-23 (§6.1): a siege's next round is held as an arrival is. A siege on the refinery, nobody
/// near, waiting after its first round when the pages are saved; its next round comes in the
/// replayed window. The session that went on raids at the round; the replay raids nothing, and
/// the round comes once, after the replay end. Returns the raids after the save, and the replay
/// end (the round's tick without a crash).
fn a_siege_round_after_the_save(crash: bool) -> (Vec<Tick>, Tick) {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(2.0, Tick(10));
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
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Siege;
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    let mut to = r.c.now();
    while r.c.parties[band].phase != Phase::Waiting {
        to += 600;
        assert!(to < day(6.0), "the siege waits at its target");
        run_journaled(&mut r, &mut walls, to, &mut journal);
    }
    assert_eq!(walls.raids.len(), 1, "its first round");
    let x = &r.c.parties[band];
    let round = x.until;
    assert!(round < x.bored_at, "a next round, not bored");
    let saved_at = r.c.now();
    let pages = r.c.save();
    let end = round + 600;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let after = |walls: &Walls| -> Vec<Tick> {
        walls
            .raids
            .iter()
            .copied()
            .filter(|&t| t > saved_at)
            .collect()
    };
    if !crash {
        return (after(&walls), round);
    }
    journal.push((end, fillers_casualty()));
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    for (at, o) in journal.iter().filter(|(at, _)| *at > saved_at) {
        back.run_until(&mut r.w, &mut walls, *at);
        back.ingest(&mut r.w, &mut walls, o, *at);
    }
    assert!(walls.raids.is_empty(), "nothing raided while replaying");
    assert_eq!(back.parties[band].phase, Phase::Waiting, "held");
    let replay_end = back.now();
    back.resend_notices();
    back.run_until(&mut r.w, &mut walls, end + 600);
    (after(&walls), replay_end)
}

#[test]
fn a_siege_round_in_the_replayed_window_waits_for_the_replay_end() {
    let (live, round) = a_siege_round_after_the_save(false);
    assert_eq!(live, vec![round], "the session that went on");
    let (back, replay_end) = a_siege_round_after_the_save(true);
    assert_eq!(back.len(), 1, "once: {back:?}");
    assert!(back[0] >= replay_end, "after the replay end: {back:?}");
}

/// The world as `pages` (its `save`) left it, for a restart (`Run::new`'s world, seed `seed`).
fn world_from(seed: u64, pages: &[u8]) -> World {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    World::load(synth::world_config(seed), goods, book, pages).unwrap()
}

/// RP-45 (the fix round's item 5): a siege on the refinery, nobody near, the pages saved while it
/// marches there unseen. Its arrival comes in the replayed window and is held; its poll arrives
/// again after the replay end. `crash`: the restart, the world and the engine from the pages.
/// Returns the siege's `bored_at` once it waits at its target, and the tick it turns for home.
fn a_siege_arriving_after_the_save(crash: bool) -> (Tick, Tick) {
    let mut r = Run::new(36, quiet_goblins());
    r.kill(2.0, Tick(10));
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
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Siege;
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    assert_eq!(r.c.parties[band].phase, Phase::Out, "on its road");
    let (pages, world) = (r.c.save(), r.w.save());
    journal.clear();
    let end = arrival + 600;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    if !crash {
        return siege_patience(&mut r.c, &mut r.w, &mut walls, band);
    }
    // Java journals other records all along: one past the arrival.
    journal.push((end, fillers_casualty()));
    let mut w = world_from(36, &world);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(walls.raids.is_empty(), "nothing raided while replaying");
    assert_eq!(back.parties[band].phase, Phase::Out, "held");
    back.resend_notices();
    assert!(!back.replaying());
    siege_patience(&mut back, &mut w, &mut walls, band)
}

/// A siege's `bored_at` once it waits at its target, and the tick it turns for home (its road
/// home's first time).
fn siege_patience(
    c: &mut Conflict,
    w: &mut World,
    walls: &mut Walls,
    band: Handle<Party>,
) -> (Tick, Tick) {
    let mut bored_at = None;
    let mut t = c.now();
    loop {
        let x = c.parties.get(band).expect("the siege is out");
        match x.phase {
            Phase::Waiting => {
                bored_at.get_or_insert(x.bored_at);
            }
            Phase::Home => return (bored_at.expect("it waited"), x.times[0]),
            _ => {}
        }
        t += 600;
        assert!(t < day(8.0), "the siege turns for home");
        c.run_until(w, walls, t);
        let _ = c.take_commands();
    }
}

/// RP-45: a held siege's patience runs from its arrival, as the session that went on has it:
/// bored on the same tick, home from there on the same tick. It used to run from the poll that
/// arrived again after the replay end.
#[test]
fn a_held_siege_arrival_is_bored_when_the_session_that_went_on_was() {
    let live = a_siege_arriving_after_the_save(false);
    assert_eq!(a_siege_arriving_after_the_save(true), live);
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

/// Goblins of a short patience that never plunder: an envoy gets bored a day after it arrives.
fn impatient_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    d.patience_days = 1.0;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// MZB-143 (re-review P2-3): a tribute paid to a bored envoy on its way home, its demand still
/// open, settles that demand. `go_home` clears the envoy's `feud`, and the lookup MZB-137 added
/// went through it, so the payment was a gift and the demand ran out and climbed.
#[test]
fn a_tribute_to_a_bored_envoy_on_its_way_home_settles_its_open_demand() {
    let mut r = Run::new(36, impatient_goblins());
    let e = envoy_met(&mut r);
    let d = r.c.feud(r.f, r.p).unwrap().demand.unwrap();
    let band = Party::of_agent(e.agent).unwrap();
    // The player does not answer: the envoy gets bored and starts home with the demand open.
    r.watch = false;
    let limit = d.deadline;
    while r.c.parties[band].phase != Phase::Home {
        assert!(r.c.now() < limit, "the envoy never left");
        r.run(r.c.now() + 600);
    }
    assert_eq!(r.c.parties[band].feud, None, "going home clears its feud");
    let open =
        r.c.feud(r.f, r.p)
            .unwrap()
            .demand
            .expect("the demand is open");
    assert_eq!(open.envoy, Some(band));
    let before = d.deadline.since(r.c.now());
    assert!(before > 1_000, "well before the deadline ({before} ticks)");
    let (tributes, gifts) = (r.c.stats.tributes, r.c.stats.gifts);
    let at = r.c.now() + 10;
    pay_to(&mut r, e.agent, e.epoch, d.tribute as i32, d.good, 91, at);
    assert_eq!(settled(&r), vec![boundary::SETTLED_PAID], "paid");
    assert_eq!(r.c.stats.tributes, tributes + 1, "a tribute");
    assert_eq!(r.c.stats.gifts, gifts, "not a gift");
    assert!(r.c.feud(r.f, r.p).unwrap().demand.is_none());
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
    // Nothing to replay: what follows is the new session's.
    c.begin_replay(true);
    c.replay_done();
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

/// MZB-146 (re-review P3-3): the chief died of age out with the band and nothing has asked for the
/// chief since (no feud wake, no plunder), so the lazy succession has not run and the band still
/// has the flag. A CHIEF_KILLED for it ran as a killing of the dead chief: a rout, a count and
/// an heir who took over holding a grudge. The succession runs first, the flag goes, and the
/// record is dropped.
#[test]
fn a_chief_killed_after_dying_of_age_before_any_wake_is_dropped() {
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
    assert!(r.c.parties[band].chief, "the chief rides with the band");
    // Past the events already due (the feud's wakes ask for the chief): the engine is quiet.
    r.run(Tick(600));
    assert!(
        r.c.parties[band].chief,
        "the chief still rides with the band"
    );
    let old = r.c.factions[r.f as usize].chief;
    let at = r.c.now() + 1;
    assert!(
        r.c.next_due().is_none_or(|d| d > at),
        "nothing due before the record"
    );
    r.w.knowledge.die(old, r.c.now());
    let successions = r.c.stats.successions;
    assert!(
        r.c.parties[band].chief,
        "nothing has found out yet: the band still has the flag"
    );
    r.ingest(chief_killed_by_mob(band, r.c.parties[band].epoch, 92), at);
    assert_eq!(
        r.c.stats.successions,
        successions + 1,
        "the heir took over, once"
    );
    assert_ne!(r.c.factions[r.f as usize].chief, old);
    assert_eq!(r.c.stats.chiefs_killed, 0, "no chief fell");
    assert_eq!(r.c.stats.routs, 0, "no rout");
    assert!(!r.c.parties[band].chief, "the band's flag is gone");
    let new = r.c.factions[r.f as usize].chief;
    assert!(r.w.knowledge.notable(new).is_some(), "the new chief lives");
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

/// CL-D61 (the Opus review): a watched fight is centred on the watching player, who may stand up
/// to `view_radius` from the band's target, so the rout is seen from where the breaker stands, not
/// from where the band was headed. Ashby, a village that hates goblins, is the one witness here:
/// the player 150 blocks from the target on Ashby's side (Ashby 200 from the target, 50 from the
/// player) puts Ashby in view; the player 150 past the target (Ashby 110 from the target, 260
/// from the player) puts it out of view. Without a position for the player (never reported, a
/// report too old for [`Conflict::watcher`] to believe, or another frame) the band's target is
/// the break point. The report says where the band broke.
#[test]
fn a_fight_breaks_where_the_breaker_stands_not_where_the_band_was_headed() {
    let (_, ashby_at) = village(&Run::new(9, quiet_goblins()), ASHBY);
    let stale = Run::new(9, quiet_goblins()).c.tun.player_stale_ticks;
    let off = |dz: i32| Pos::new(0, ashby_at.x, ashby_at.z + dz);
    // (target, where the player stands, how long ago that was reported, Ashby sees the rout)
    let cases = [
        (off(200), Some(off(50)), 0, true),
        (off(110), Some(off(260)), 0, false),
        (off(110), None, 0, true),
        (off(200), None, 0, false),
        // The same player, reported just inside the stale limit and just outside it.
        (off(200), Some(off(50)), stale, true),
        (off(200), Some(off(50)), stale + 1, false),
        // The same place in another frame is not near the band at all.
        (
            off(200),
            Some(Pos::new(1, ashby_at.x, ashby_at.z + 50)),
            0,
            false,
        ),
    ];
    for (target, player, age, seen) in cases {
        let (mut r, band) = raid_in_a_fight_at(9, target);
        let at = fight_at();
        if let Some(pp) = player {
            assert!(
                blocks_between(pp, (target.x, target.z)) <= 150.0,
                "the player stands within 150 blocks of the target"
            );
            // Observed in the world's frame, then reported in the case's (the world knows one).
            r.w.observe_area(synth::PLAYER, Pos::new(0, pp.x, pp.z), at, 160);
            let st =
                r.w.players
                    .get_mut(&r.p)
                    .expect("the player is known to the world");
            st.pos = Some((pp, Tick(at.0 - age)));
        }
        let broke = match player {
            Some(pp) if age <= stale && pp.frame == 0 => (pp.x, pp.z),
            _ => (target.x, target.z),
        };
        r.c.chief_killed(&mut r.w, band, Some(r.p), at);
        let reps = rescue_reports(&r);
        assert_eq!(
            reps.len(),
            usize::from(seen),
            "player {player:?} ({age} ticks ago), target {target:?}: Ashby sees: {seen}"
        );
        if seen {
            let rep = r.w.knowledge.report(reps[0]).unwrap();
            assert_eq!(
                (rep.place.frame, rep.place.x, rep.place.z),
                (0, broke.0, broke.1),
                "where the band broke"
            );
            let (ashby, _) = village(&r, ASHBY);
            assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby witnesses it");
        }
    }
}

/// CL-D61: a rout nobody witnessed records no report (it had six witnesses, every hating village
/// within 2,000 blocks, before). The band breaks at the refinery, 164 blocks from Ashby, the
/// nearest village, and it was marching on the player's refinery, not on a village.
#[test]
fn a_rout_nobody_witnessed_records_no_report() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = fight_at();
    let view = f64::from(r.c.tun.view_radius);
    let broke = r.c.party_pos(band, at);
    assert!(
        villages(&r)
            .iter()
            .all(|&(_, pos, _)| blocks_between(pos, broke) > view),
        "no village sees the rout"
    );
    let was: Vec<i8> = villages(&r)
        .iter()
        .map(|&(a, _, _)| r.w.knowledge.sentiment(a, r.p, at))
        .collect();
    r.c.log_enabled = true;
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    assert!(rescue_reports(&r).is_empty(), "no witness, no report");
    assert_eq!(
        rescue_notes(&r),
        ["nobody saw the goblin band broken: no rescue report"]
    );
    for ((a, _, _), was) in villages(&r).into_iter().zip(was) {
        assert_eq!(r.w.knowledge.sentiment(a, r.p, at), was);
    }
}

/// The key of the player base the CL-D61 follow-up tests mark.
const BASE: u64 = 0xBA5E_0001;

/// A raid on the player's refinery with its war chief, fighting the player at [`fight_at`], in a
/// world where the player's dwell counter has marked a base `base_at`: the base's node and actor.
/// A fighting band stands at its target, so the base is as far from the rout as `base_at` is from
/// the refinery.
fn chief_with_a_base_at(seed: u64, base_at: Pos) -> (Run, Handle<Party>, usize, ActorId) {
    let mut r = Run::new(seed, quiet_goblins());
    let node = r.w.mark_base(BASE, base_at, r.p, Tick::ZERO);
    assert!(
        matches!(r.w.nodes[node].kind, NodeKind::PlayerBase { .. }),
        "a player base"
    );
    let actor = r.w.nodes[node].actor;
    let (mut r, band) = band_in_a_fight_of(r, |_| {
        Target::District(synth::REFINERY, synth::REFINERY_POS)
    });
    r.c.log_enabled = true;
    (r, band, node, actor)
}

/// The note log's rescue lines.
fn rescue_notes(r: &Run) -> Vec<String> {
    r.c.log
        .iter()
        .filter(|e| e.kind == "rescue")
        .map(|e| e.what.clone())
        .collect()
}

/// CL-D61 ("Base carries the news", Ben, 2026-10-23): a band broken at a player's base has that
/// base as its witness, as a caravan has a rescue it saw (PLAN.md §8.7: "and its owner once it
/// gets home"). One rout at the refinery, 164 blocks from the nearest village, with the player's
/// base marked at the refinery: one rescue report, the base has it first-hand at the full +25,
/// and no village does, though the report is now recorded.
#[test]
fn a_rout_at_a_players_base_is_witnessed_by_the_base_and_by_no_village() {
    let (mut r, band, _, base) = chief_with_a_base_at(9, synth::REFINERY_POS);
    let at = fight_at();
    let view = f64::from(r.c.tun.view_radius);
    let broke = r.c.party_pos(band, at);
    assert!(
        villages(&r)
            .iter()
            .all(|&(_, pos, _)| blocks_between(pos, broke) > view),
        "no village sees the rout"
    );
    let was = r.w.knowledge.sentiment(base, r.p, at);
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    let rep = r.w.knowledge.report(reps[0]).unwrap();
    assert_eq!(rep.who, r.p, "the rescuer");
    assert_eq!(
        (rep.place.frame, rep.place.x, rep.place.z),
        (0, broke.0, broke.1),
        "where the band broke"
    );
    assert!(r.w.knowledge.knows(base, reps[0]), "the base saw it");
    let now = r.w.knowledge.sentiment(base, r.p, at);
    assert!(
        i32::from(now) >= i32::from(was) + 20,
        "first-hand, not hearsay: {was} to {now}"
    );
    for (a, _, hates) in villages(&r) {
        assert!(
            !r.w.knowledge.knows(a, reps[0]),
            "no village (hating the camp: {hates}) witnesses it"
        );
    }
    assert_eq!(
        rescue_notes(&r),
        [
            "0 village(s) and 1 base(s) saw the goblin band broken (the rest hear it from travellers)"
        ]
    );
}

/// CL-D61 ("Base carries the news"): the villages hear a rout at a base only second-hand. The
/// base's news goes out with the next messenger on each of its roads (the real travel path, the
/// world's packets and deliveries), so some village learns it days later, at hearsay weight
/// (+12, half the first-hand +25), and no village ever has it first-hand: the base is the only
/// witness, and nothing a traveller carries is first-hand to the one who hears it.
#[test]
fn a_village_hears_a_rout_at_a_base_later_from_travellers_at_hearsay_weight() {
    let (mut r, band, node, base) = chief_with_a_base_at(9, synth::REFINERY_POS);
    let at = fight_at();
    assert!(
        !r.w.nodes[node].out.is_empty(),
        "the base is on the road network, so travellers leave it"
    );
    let delivered = r.w.stats.deliveries;
    let was: Vec<(ActorId, i8)> = villages(&r)
        .iter()
        .map(|&(a, _, _)| (a, r.w.knowledge.sentiment(a, r.p, at)))
        .collect();
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1);
    assert!(r.w.knowledge.knows(base, reps[0]));
    assert_eq!(
        was.iter()
            .filter(|(a, _)| r.w.knowledge.knows(*a, reps[0]))
            .count(),
        0,
        "no village knows it when the band breaks"
    );
    // Watch the news go out, an hour of game time (1,000 ticks) at a time: a village is read the
    // moment it first knows the report, before its feeling fades.
    let mut heard: Vec<(ActorId, i8, i8, u64)> = Vec::new();
    let mut t = at;
    while t < at + 10 * TICKS_PER_DAY {
        t += 1_000;
        r.w.run_until(t);
        for &(a, was) in &was {
            if r.w.knowledge.knows(a, reps[0]) && !heard.iter().any(|h| h.0 == a) {
                heard.push((a, was, r.w.knowledge.sentiment(a, r.p, t), t.0 - at.0));
            }
        }
    }
    assert!(
        r.w.stats.deliveries > delivered,
        "messengers arrived in the meantime"
    );
    assert!(
        !heard.is_empty(),
        "travellers carry the rescue from the base to a village"
    );
    for (_, was, now, after) in heard {
        assert!(after > 0, "later, not at once");
        assert!(
            i32::from(now) >= i32::from(was) + 8 && i32::from(now) < i32::from(was) + 20,
            "a village that heard it from a traveller thinks a little better, not as one who \
             saw it: {was} to {now}"
        );
    }
}

/// CL-D61 ("Base carries the news"): the base sees the rout from `view_radius`, as a village
/// does: a base exactly that far from where the band broke witnesses it, one block farther does
/// not (and a rout no village and no base sees records nothing). The base is not the band's
/// target, so this is the view rule alone and not the target rule: the band was after the
/// refinery, the player stands 150 blocks east of it (where the band broke), and the base is
/// 160 or 161 blocks east of the player, 310 and 311 from the refinery.
#[test]
fn a_base_sees_a_rout_from_exactly_the_view_radius() {
    let view = Run::new(9, quiet_goblins()).c.tun.view_radius as i32;
    let target = synth::REFINERY_POS;
    let player_at = Pos::new(0, target.x + 150, target.z);
    for (dx, seen) in [(view, true), (view + 1, false)] {
        let base_at = Pos::new(0, player_at.x + dx, player_at.z);
        let (mut r, band, base) = base_and_a_player_off_the_target(base_at, player_at);
        assert!(
            blocks_between(base_at, (target.x, target.z)) > f64::from(view),
            "the base is not the band's target"
        );
        r.c.chief_killed(&mut r.w, band, Some(r.p), fight_at());
        let reps = rescue_reports(&r);
        assert_eq!(
            reps.len(),
            usize::from(seen),
            "a base {dx} blocks from the rout sees it: {seen}"
        );
        assert_eq!(reps.iter().any(|id| r.w.knowledge.knows(base, *id)), seen);
        if seen {
            let rep = r.w.knowledge.report(reps[0]).unwrap();
            assert_eq!(
                (rep.place.x, rep.place.z),
                (player_at.x, player_at.z),
                "the rout is where the player stands"
            );
        }
    }
}

/// CL-D61 ("Base carries the news"): a rout the villages and a base both see is still one report:
/// the village that hates the camp and the base each have it first-hand, the rest hear it later.
/// The base is 86 blocks from the rout, and Ashby is too.
#[test]
fn a_base_and_a_village_in_view_of_one_rout_are_both_witnesses_of_one_report() {
    let mut r = Run::new(9, quiet_goblins());
    let node = r.w.mark_base(BASE, BY_ASHBY, r.p, Tick::ZERO);
    let base = r.w.nodes[node].actor;
    let (mut r, band) = band_in_a_fight_of(r, |_| Target::District(synth::REFINERY, BY_ASHBY));
    r.c.log_enabled = true;
    let (ashby, _) = village(&r, ASHBY);
    r.c.chief_killed(&mut r.w, band, Some(r.p), fight_at());
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    assert!(r.w.knowledge.knows(base, reps[0]), "the base saw it");
    assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby saw it");
    for (a, _, _) in villages(&r) {
        if a != ashby {
            assert!(!r.w.knowledge.knows(a, reps[0]), "the rest have not heard");
        }
    }
    assert_eq!(
        rescue_notes(&r),
        [
            "1 village(s) and 1 base(s) saw the goblin band broken (the rest hear it from travellers)"
        ]
    );
}

/// A base on the road network at `base_at`, a raid on the refinery serving the player's feud, and
/// the player standing at `player_at`, reported at [`fight_at`]: the band fights the player there
/// (`start_fight` centres the fight on the watching player) while its target is the refinery.
fn base_and_a_player_off_the_target(base_at: Pos, player_at: Pos) -> (Run, Handle<Party>, ActorId) {
    let (mut r, band, _, base) = chief_with_a_base_at(9, base_at);
    r.w.observe_area(synth::PLAYER, player_at, fight_at(), 160);
    (r, band, base)
}

/// CL-D61 (the Opus review): where the rout happens and what the band marched on both bring a
/// base in. The refinery is the band's target and the player stands 150 blocks east of it, so the
/// rout is the player's place. A base 100 blocks west of the refinery is the base the band was
/// after (250 blocks from the rout): it witnesses with the player off the target. A base 300
/// blocks east of the refinery is neither the band's target nor within reach of it, but it is 150
/// from where the player stands: the rout happened at it. A base 400 blocks north of the
/// refinery is neither, and sees nothing.
#[test]
fn a_base_witnesses_the_rout_it_was_marched_on_or_that_happened_at_it() {
    let target = synth::REFINERY_POS;
    let player_at = Pos::new(0, target.x + 150, target.z);
    let cases = [
        (
            Pos::new(0, target.x - 100, target.z),
            true,
            "the base the band marched on",
        ),
        (
            Pos::new(0, target.x + 300, target.z),
            true,
            "the base the rout happened at",
        ),
        (
            Pos::new(0, target.x, target.z - 400),
            false,
            "a base that is neither",
        ),
    ];
    for (base_at, seen, what) in cases {
        let (mut r, band, base) = base_and_a_player_off_the_target(base_at, player_at);
        let at = fight_at();
        assert_eq!(r.c.party_pos(band, at), (target.x, target.z));
        assert!(
            villages(&r)
                .iter()
                .all(|&(_, pos, _)| blocks_between(pos, (player_at.x, player_at.z)) > 160.0),
            "no village sees the rout where the player stands"
        );
        r.c.chief_killed(&mut r.w, band, Some(r.p), at);
        let reps = rescue_reports(&r);
        assert_eq!(reps.len(), usize::from(seen), "{what}: {seen}");
        assert_eq!(
            reps.iter().any(|id| r.w.knowledge.knows(base, *id)),
            seen,
            "{what}: the base has the report first-hand: {seen}"
        );
        if let Some(&id) = reps.first() {
            let rep = r.w.knowledge.report(id).unwrap();
            assert_eq!(
                (rep.place.x, rep.place.z),
                (player_at.x, player_at.z),
                "the rout is where the player stands"
            );
        }
    }
}

/// A raid on Ashby (a village that hates goblins) with its war chief, serving the player's feud,
/// still on the road at [`fight_at`] (it is a day's march and more from the camp). Returns it with
/// the village.
fn band_on_the_road_to_ashby(seed: u64) -> (Run, Handle<Party>, ActorId) {
    let mut r = Run::new(seed, quiet_goblins());
    r.kill(20.0, Tick(10));
    let ashby_node = r.w.node_of_key(ASHBY).unwrap();
    let band =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::Node(ashby_node),
            Some(r.p),
            Tick(20),
        )
        .unwrap();
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.c.run_until(&mut r.w, &mut NoInfrastructure, fight_at());
    let x = &r.c.parties[band];
    assert!(
        x.chief && x.phase == Phase::Out,
        "on the road: {:?}",
        x.phase
    );
    r.c.log_enabled = true;
    let (ashby, _) = village(&r, ASHBY);
    (r, band, ashby)
}

/// CL-D61 (the Opus review): a rout on the road is a rescue like a rout in a fight (C24: rescues
/// count). The war chief falls to the player while the band is on the march: one rout counted,
/// one rescue report where the band was on the road, the village it marched on witnesses it at
/// the full +25.
#[test]
fn a_chief_killed_on_the_road_records_the_rescue() {
    let (mut r, band, ashby) = band_on_the_road_to_ashby(9);
    let at = fight_at();
    let broke = r.c.party_pos(band, at);
    // The player stands 100 blocks off, reported just now: a band on the march is seen where it
    // walks, and the player's place does not move where it broke (only a watched fight is
    // centred on the player).
    let near = Pos::new(0, broke.0 + 100, broke.1);
    r.w.observe_area(synth::PLAYER, near, at, 160);
    let was = r.w.knowledge.sentiment(ashby, r.p, at);
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert_eq!(r.c.stats.routs, 1);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    let rep = r.w.knowledge.report(reps[0]).unwrap();
    assert_eq!(rep.who, r.p, "the rescuer");
    assert_eq!(rep.whom, ashby, "the village the band marched on");
    assert_eq!(
        (rep.place.x, rep.place.z),
        broke,
        "where the band was on the road"
    );
    assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby witnesses it");
    let now = r.w.knowledge.sentiment(ashby, r.p, at);
    assert!(
        i32::from(now) >= i32::from(was) + 20,
        "first-hand, not hearsay: {was} to {now}"
    );
    assert_eq!(
        rescue_notes(&r),
        [
            "1 village(s) and 0 base(s) saw the goblin band broken (the rest hear it from travellers)"
        ]
    );
}

/// CL-D61 (the Opus review): a chief felled by nobody a player is no rescue, on the road as in a
/// fight: the rout is counted, no report is made.
#[test]
fn a_chief_killed_on_the_road_by_no_player_is_no_rescue() {
    let (mut r, band, _) = band_on_the_road_to_ashby(9);
    r.c.chief_killed(&mut r.w, band, None, fight_at());
    assert_eq!(r.c.stats.routs, 1);
    assert!(rescue_reports(&r).is_empty(), "no player broke it");
    assert!(rescue_notes(&r).is_empty());
}

/// CL-D61 (the Opus review): a war band the player wipes out on the road, unit by unit, is routed
/// as the chief's fall routs it: counted in `routs` (it was counted nowhere) and one rescue report
/// for the player who broke it, witnessed by the village it marched on.
#[test]
fn a_war_band_wiped_out_on_the_road_records_the_rescue() {
    let (mut r, band, ashby) = band_on_the_road_to_ashby(9);
    let at = fight_at();
    let broke = r.c.party_pos(band, at);
    // The player stands 100 blocks off, as in the chief's fall on the road: not the place.
    r.w.observe_area(synth::PLAYER, Pos::new(0, broke.0 + 100, broke.1), at, 160);
    // The plugin has it in view: embodied, near a player, and the player kills every member.
    r.c.parties[band].embodied = true;
    let epoch = r.c.parties[band].epoch;
    let units = r.c.parties[band].units.clone();
    let c =
        r.c.cultures
            .get(r.c.cultures.index_of("goblin").unwrap())
            .clone();
    assert_eq!(r.c.stats.routs, 0);
    for (u, count) in units {
        r.ingest(
            Obs::Casualties(boundary::BandCasualties {
                agent: Party::agent(band),
                epoch,
                by: boundary::BY_PLAYER,
                body: rtsim_conflict::engine::body_id(&r.c.cultures, &c.unit(u).body),
                count,
                killer: synth::PLAYER,
            }),
            at,
        );
    }
    assert_eq!(r.c.parties[band].alive(), 0);
    assert_eq!(r.c.parties[band].phase, Phase::Home, "it goes home");
    assert_eq!(r.c.stats.routs, 1, "a rout");
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    let rep = r.w.knowledge.report(reps[0]).unwrap();
    assert_eq!((rep.who, rep.whom), (r.p, ashby));
    assert_eq!((rep.place.x, rep.place.z), broke);
    assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby witnesses it");
}

/// A raid on `target` in a watched fight, then the casualties the plugin would report: `by` kills
/// one goblin of the band's first unit (`all` false) or every member still standing (`all`).
fn strike(r: &mut Run, band: Handle<Party>, by: u32, all: bool, at: Tick) {
    r.c.parties[band].embodied = true;
    let epoch = r.c.parties[band].epoch;
    let units = r.c.parties[band].units.clone();
    let goblins = r.c.cultures.index_of("goblin").unwrap();
    let c = r.c.cultures.get(goblins).clone();
    let killer = if by == boundary::BY_PLAYER {
        synth::PLAYER
    } else {
        (0, 0)
    };
    for (u, count) in units {
        let count = if all { count } else { 1 };
        r.ingest(
            Obs::Casualties(boundary::BandCasualties {
                agent: Party::agent(band),
                epoch,
                by,
                body: rtsim_conflict::engine::body_id(&r.c.cultures, &c.unit(u).body),
                count,
                killer,
            }),
            at,
        );
        if !all {
            break;
        }
    }
}

/// CL-D61 (the second review): the breaker is the last player who killed any member, and the rout
/// can come later, from a mob's kills, with the breaker walked off. A player who stands a long way
/// from the band when it breaks did not see it break, so the band's own place is where it broke,
/// and a village near the player does not witness it (it took the rout first-hand from 1,386
/// blocks away). The breaker's position counts only within `view_radius` of the band, the raw
/// distance `Conflict::watcher` reads, the edge included.
#[test]
fn a_breaker_who_has_walked_off_did_not_see_the_band_break() {
    let (_, ashby_at) = village(&Run::new(9, quiet_goblins()), ASHBY);
    let (_, oakford_at) = village(&Run::new(9, quiet_goblins()), OAKFORD);
    let view = Run::new(9, quiet_goblins()).c.tun.view_radius as i32;
    let off = |dz: i32| Pos::new(0, ashby_at.x, ashby_at.z + dz);
    // (band's target, where the breaker stands, which village witnesses: none or the one at the
    // player's feet)
    let cases = [
        // Near: Ashby is 50 from the player, 200 from the band; the player's place is the rout's.
        (off(200), off(50), true),
        // The edge: the player at Ashby's gate, exactly 160 from the band, and one block more.
        (off(view), off(0), true),
        (off(view + 1), off(0), false),
        // Far: the player at Oakford, nearly 800 blocks from the band, and nothing else in sight.
        (synth::REFINERY_POS, oakford_at, false),
    ];
    for (target, player_at, seen) in cases {
        let (mut r, band) = raid_in_a_fight_at(9, target);
        let at = fight_at();
        r.c.log_enabled = true;
        // A fight the plugin began: it runs for days yet (a hand-set phase has no end booked, and
        // the first loss would book the end at once).
        r.c.parties[band].until = at + 5 * TICKS_PER_DAY;
        r.c.parties[band].bored_at = at + 5 * TICKS_PER_DAY;
        // The breaker kills one member; the player is where they stand, reported just now.
        strike(&mut r, band, boundary::BY_PLAYER, false, at);
        assert_eq!(r.c.parties[band].breaker, Some(r.p));
        r.w.observe_area(synth::PLAYER, player_at, at, 160);
        let d = blocks_between(player_at, (target.x, target.z));
        // A mob finishes the band: no player struck the blow that broke it.
        strike(&mut r, band, boundary::BY_MOB, true, at);
        assert_eq!(r.c.stats.routs, 1, "the band routs ({d:.0} blocks off)");
        let reps = rescue_reports(&r);
        assert_eq!(
            reps.len(),
            usize::from(seen),
            "the player {d:.0} blocks from the band: the rout is seen: {seen}"
        );
        if seen {
            let rep = r.w.knowledge.report(reps[0]).unwrap();
            assert_eq!(
                (rep.place.x, rep.place.z),
                (player_at.x, player_at.z),
                "the rout is where the player stands"
            );
        }
        for (a, pos, _) in villages(&r) {
            let at_player = blocks_between(pos, (player_at.x, player_at.z)) <= f64::from(view);
            assert_eq!(
                reps.iter().any(|id| r.w.knowledge.knows(a, *id)),
                seen && at_player,
                "a village {:.0} blocks from the player ({d:.0} from the band)",
                blocks_between(pos, (player_at.x, player_at.z))
            );
        }
    }
}

/// The band of [`band_on_the_road_to_ashby`], on the road at [`fight_at`], with a player base
/// marked `offset` blocks east of where it is, and, when `aimed`, the base the band's target (a
/// district at the base, as a raid on the player's works there would be). Returns the run, the
/// band, the base's actor and where the band is.
fn road_band_near_a_base(offset: i32, aimed: bool) -> (Run, Handle<Party>, ActorId, (i32, i32)) {
    let (mut r, band, _) = band_on_the_road_to_ashby(9);
    let at = fight_at();
    let broke = r.c.party_pos(band, at);
    let base_at = Pos::new(0, broke.0 + offset, broke.1);
    let node = r.w.mark_base(BASE, base_at, r.p, at);
    if aimed {
        r.c.parties[band].target = Target::District(synth::REFINERY, base_at);
    }
    let base = r.w.nodes[node].actor;
    (r, band, base, broke)
}

/// CL-D61 (the second review): a band routed on the road was not at its target, so the base it
/// marched on does not witness from any distance: "a band broken at a player's base" is a band
/// that was fighting at it. On the road a base witnesses only if it is within `view_radius` of
/// where the band broke, the raw distance, the edge included, whatever the band was after. A road
/// rout 297 blocks from the base the band marched on is seen by no one: no report, so no village
/// ever hears it, from travellers or otherwise. The same rout within view of the base is a
/// report that base has first-hand.
#[test]
fn a_road_rout_is_seen_only_by_a_base_in_view_not_by_the_one_the_band_marched_on() {
    let view = f64::from(Run::new(9, quiet_goblins()).c.tun.view_radius);
    // (blocks from the band to the base, the base is the band's target, the base saw it). A band
    // that is not aimed at the base is still after Ashby, so Ashby, the rescued village, always
    // witnesses and there is a report whether or not the base saw it.
    let cases = [
        (297, true, false),
        (297, false, false),
        (100, true, true),
        (100, false, true),
        (160, true, true),
        (161, true, false),
    ];
    for (offset, aimed, seen) in cases {
        let (mut r, band, base, broke) = road_band_near_a_base(offset, aimed);
        let at = fight_at();
        assert!(
            villages(&r)
                .iter()
                .all(|&(_, pos, _)| blocks_between(pos, broke) > view),
            "no village is in view of the band on the road"
        );
        assert_eq!(r.c.parties[band].phase, Phase::Out);
        r.c.chief_killed(&mut r.w, band, Some(r.p), at);
        assert_eq!(r.c.stats.routs, 1);
        let reps = rescue_reports(&r);
        assert_eq!(
            reps.len(),
            usize::from(seen || !aimed),
            "a road rout {offset} blocks from the base (the band's target: {aimed}): recorded"
        );
        assert_eq!(
            reps.iter().any(|id| r.w.knowledge.knows(base, *id)),
            seen,
            "the base {offset} blocks off (the band's target: {aimed}) has it first-hand: {seen}"
        );
        // A rout nobody saw is heard of by nobody, at once or later.
        r.w.run_until(at + 5 * TICKS_PER_DAY);
        if reps.is_empty() {
            for (a, _, _) in villages(&r) {
                assert!(
                    !rescue_reports(&r)
                        .iter()
                        .any(|id| r.w.knowledge.knows(a, *id)),
                    "a village hears a rout nobody saw"
                );
            }
        }
    }
}

/// CL-D61 (the second review): a village that saw the rout tells at once, as a base does, rather
/// than with its next market event. Ashby (human) witnesses a rout in its view: its vanilla
/// gossip is flushed with the commands and a packet is made for the next messenger on its roads.
/// Mirewater (a ribbit village, the village the band marched on) has its feelings out with the
/// next commands: its sorcerers read them to leave out or favour players.
#[test]
fn a_village_that_saw_the_rout_tells_the_travellers_and_the_bridge_at_once() {
    // Ashby, in view of the rout.
    let (mut r, band) = raid_in_a_fight_at(9, BY_ASHBY);
    let at = fight_at();
    let (ashby, _) = village(&r, ASHBY);
    r.w.take_commands();
    let sent = r.w.stats.packets_sent;
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1);
    assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby witnessed it");
    assert!(
        r.w.stats.packets_sent > sent,
        "Ashby's news is in a packet for the next messengers ({sent} before)"
    );
    let cmds = r.w.take_commands();
    assert!(
        cmds.iter().any(|c| matches!(c,
            Command::VanillaGossip(g) if g.village == ashby && g.report == reps[0] && g.first_hand)),
        "Ashby's vanilla gossip is written with the commands: {cmds:?}"
    );

    // Mirewater, the village the band marched on.
    let (mut r, band) = band_in_a_fight(9, |r| Target::Node(r.w.node_of_key(MIREWATER).unwrap()));
    let at = fight_at();
    let mirewater = r.w.node_of_key(MIREWATER).unwrap();
    let key = r.w.nodes[mirewater].key;
    r.w.take_commands();
    let sent = r.w.stats.packets_sent;
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    assert!(
        r.w.stats.packets_sent > sent,
        "Mirewater's news is in a packet"
    );
    let cmds = r.w.take_commands();
    let mine = cmds.iter().find_map(|c| match c {
        Command::Sentiment(s) if s.village == key => Some(s.clone()),
        _ => None,
    });
    let mine = mine.expect("Mirewater's feelings go out with the commands");
    assert!(
        mine.entries
            .iter()
            .any(|&(m, l, v)| (m, l) == synth::PLAYER && v > 0),
        "it thinks well of the player who saved it: {:?}",
        mine.entries
    );
}

/// CL-D61 (the second review): a base no player marks any more keeps its node and roads (news on
/// them may still be arriving) but nobody is there: it neither witnesses nor tells, as
/// `band_may_visit` and the road stops already read it. The same rout at the same base is seen
/// while a player marks it and not once the mark is gone, and it was the only candidate.
#[test]
fn an_abandoned_base_is_no_witness() {
    for marked in [true, false] {
        let (mut r, band, node, base) = chief_with_a_base_at(9, synth::REFINERY_POS);
        if !marked {
            r.w.unmark_base(BASE, r.p);
        }
        assert_eq!(r.w.base_active(node), marked);
        r.c.chief_killed(&mut r.w, band, Some(r.p), fight_at());
        let reps = rescue_reports(&r);
        assert_eq!(
            reps.len(),
            usize::from(marked),
            "a rout at the refinery base (marked: {marked}) is seen: {marked}"
        );
        assert_eq!(reps.iter().any(|id| r.w.knowledge.knows(base, *id)), marked);
        assert_eq!(
            rescue_notes(&r),
            [if marked {
                "0 village(s) and 1 base(s) saw the goblin band broken (the rest hear it from travellers)"
            } else {
                "nobody saw the goblin band broken: no rescue report"
            }]
        );
    }
}

/// CL-D61 (the second review): the breaker is the last player who killed any member, and a rout
/// that comes later by no player's hand keeps him. A player strikes one goblin of a band on the
/// road and walks off; the war chief then dies to a mob (`killer` None). The rule gives the
/// player the rescue, as it does for a fight a mob finishes after his kills
/// (`a_breaker_who_has_walked_off_did_not_see_the_band_break`): he did the breaking that counts,
/// he is credited, and the village the band marched on witnesses. With no earlier strike by a
/// player there is no breaker and no rescue (`a_chief_killed_on_the_road_by_no_player_is_no_rescue`).
#[test]
fn a_chief_felled_by_no_player_after_a_player_struck_the_band_is_still_that_players_rescue() {
    let (mut r, band, ashby) = band_on_the_road_to_ashby(9);
    let at = fight_at();
    strike(&mut r, band, boundary::BY_PLAYER, false, at);
    assert_eq!(r.c.parties[band].breaker, Some(r.p));
    r.c.parties[band].embodied = false;
    r.c.chief_killed(&mut r.w, band, None, at);
    assert_eq!(r.c.stats.routs, 1);
    let reps = rescue_reports(&r);
    assert_eq!(reps.len(), 1, "one rout, one rescue report");
    let rep = r.w.knowledge.report(reps[0]).unwrap();
    assert_eq!((rep.who, rep.whom), (r.p, ashby), "the striker's rescue");
    assert!(r.w.knowledge.knows(ashby, reps[0]), "Ashby witnesses it");
}
