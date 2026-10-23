// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M1 (§9): the world's difficulty (no war party leaves on
//! Peaceful, §4.12), a feud's notice kept until the plugin plays it (NOTICE_SHOWN, resent at each
//! session start, across a restart), and an envoy party wiped out with no player among its
//! killers withdrawing its demand (DEMAND_SETTLED outcome 5, ledger 14). And the reports the
//! conflict records on the knowledge layer's severity unit (MZB-36).

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, Party, Phase, Signal, Target};
use rtsim_conflict::synth::{self, FakeConflict};
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

/// MOWZIE.md §4.12: on Peaceful no war party leaves. A feud kept far past the raid rung for
/// twenty days climbs to scouts and no further, its grievance kept; once the world is not
/// Peaceful the feud climbs on (every feud planned again), and a raid sets out.
#[test]
fn on_peaceful_no_war_party_leaves_and_the_climb_resumes_after() {
    let mut r = Run::new(31, quiet_goblins());
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(1));
    assert!(r.c.peaceful);
    keep_killing(&mut r, 0.0, 20.0);
    assert_eq!(r.war_parties(), 0, "no war party on Peaceful");
    assert!(
        r.step() <= Step::Scouts,
        "held below the raid: {:?}",
        r.step()
    );
    assert!(r.c.stats.demands >= 1, "envoys still go");
    let g = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert!(
        g > 70.0,
        "the grievance is kept, and past the raid's 70: {g}"
    );
    // A save keeps the flag.
    let back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
    assert!(back.peaceful);
    assert_eq!(back.state_hash(), r.c.state_hash());
    // Normal again: the held feud climbs.
    let at = r.c.now();
    r.ingest(difficulty(boundary::DIFFICULTY_NORMAL), at);
    assert!(!r.c.peaceful);
    keep_killing(&mut r, 20.0, 26.0);
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

/// MZB-16 (review): the end of Peaceful sets off no burst. Several players' feuds are held below
/// the raid rung across eight days of Peaceful; once the world is Easy, each raids after its own
/// dwell, paused over the Peaceful span, and a jitter of its own: no two on the same tick, and
/// none before its dwell.
#[test]
fn feuds_held_over_peaceful_raid_one_by_one_after() {
    let mut r = Run::new(39, quiet_goblins());
    r.ingest(difficulty(boundary::DIFFICULTY_PEACEFUL), Tick(1));
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
    let mut d = 0.0;
    while d < 8.0 {
        let at = r.c.now().max(day(d));
        killings(&mut r, at);
        d += 2.0;
        r.run(day(d));
    }
    assert_eq!(r.war_parties(), 0, "none on Peaceful");
    let scouts = r.c.cultures.get(0).dwell_ticks[Step::Scouts.index()];
    let mut held = Vec::new();
    for &p in &players {
        let x = r.c.feud(r.f, p).unwrap();
        assert_eq!(x.step, Step::Scouts, "held below the raid");
        assert!(x.party.is_none() && x.demand.is_none());
        held.push((p, x.since));
    }
    let easy = r.c.now();
    r.ingest(difficulty(boundary::DIFFICULTY_EASY), easy);
    assert!(!r.c.peaceful);
    // To each raid, event by event, so each tick is exact.
    let mut raided: Vec<(ActorId, Tick)> = Vec::new();
    let end = easy + 3 * scouts + TICKS_PER_DAY;
    while raided.len() < players.len() && r.c.now() < end {
        let next = r.c.next_due().unwrap_or(end).min(end).max(r.c.now() + 1);
        r.c.run_until(&mut r.w, &mut NoInfrastructure, next);
        r.flush(next);
        for &p in &players {
            let x = r.c.feud(r.f, p).unwrap();
            if x.step == Step::Raid && !raided.iter().any(|e| e.0 == p) {
                raided.push((p, r.c.now()));
            }
        }
    }
    assert_eq!(
        raided.len(),
        players.len(),
        "every held feud raids: {raided:?}"
    );
    let mut ticks: Vec<Tick> = raided.iter().map(|e| e.1).collect();
    ticks.sort();
    ticks.dedup();
    assert_eq!(ticks.len(), players.len(), "no two on one tick: {raided:?}");
    for (p, at) in &raided {
        let since = held.iter().find(|h| h.0 == *p).unwrap().1;
        // Reached during Peaceful, so the whole dwell comes after it.
        assert!(since > Tick(1));
        assert!(
            *at >= easy + scouts,
            "{p:?} raided at {at:?}, before its dwell"
        );
        assert!(*at < easy + 2 * scouts + TICKS_PER_DAY, "{at:?}");
    }
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
