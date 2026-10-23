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
    let camp = synth::CAMP_POS;
    let range = f64::from(r.c.tun.rescue_news_blocks);
    let haters: Vec<(ActorId, i8)> =
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
            .map(|n| (n.actor, r.w.knowledge.sentiment(n.actor, r.p, at)))
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
