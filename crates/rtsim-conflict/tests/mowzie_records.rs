// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M1 (§9): the world's difficulty (no war party leaves on
//! Peaceful, §4.12), a feud's notice kept until the plugin plays it (NOTICE_SHOWN, resent at each
//! session start, across a restart), and an envoy party wiped out with no player among its
//! killers withdrawing its demand (DEMAND_SETTLED outcome 5, ledger 14). And the reports the
//! conflict records on the knowledge layer's severity unit (MZB-36).

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{
    Conflict, Demand, Party, Phase, REPLAY_HOLD_SLACK_TICKS, Signal, Target, body_id, item_of,
};
use rtsim_conflict::synth::{self, Answer, FakeConflict};
use rtsim_conflict::{ConflictTunables, Cultures, Step};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::{ActorId, ReportKind, SEVERITY_UNIT};
use rtsim_settle::{NodeKind, Pos};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::{Command, World};

/// A fresh op id for a hand-made BAND_CASUALTIES (MZB-589): each record is its own death, so none
/// repeats another's op.
fn next_op() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    (1 << 63) | (N.fetch_add(1, Ordering::Relaxed) + 1)
}

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
    /// With [`Run::watch`], the player's area is routed too (a band is met on its march, as
    /// `Conflict::observe_player` does). Clear, only the player's report is made: a band that
    /// arrives finds a watcher it was never met by, so its fight starts from an abstract band.
    sight: bool,
    /// Java refuses this band's fights: in [`run_journaled`], each BAND_MATERIALIZED the plugin
    /// answers at the epoch the band fights at is a BAND_SPAWN_REJECTED (1, not ticking).
    refuse: Option<Handle<Party>>,
    /// With [`Run::refuse`], Java refuses only each fight's first wave (0).
    refuse_wave0_only: bool,
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
            sight: true,
            refuse: None,
            refuse_wave0_only: false,
        }
    }

    /// `o`, or Java's refusal of it when [`Run::refuse`] names its band, fighting at its epoch.
    fn refused(&self, o: Obs) -> Obs {
        match (&o, self.refuse) {
            (Obs::Materialized(m), Some(band))
                if m.agent == Party::agent(band)
                    && self
                        .c
                        .parties
                        .get(band)
                        .is_some_and(|x| x.phase == Phase::Fighting && x.epoch == m.epoch)
                    && (!self.refuse_wave0_only || m.wave == 0) =>
            {
                Obs::SpawnRejected(boundary::BandSpawnRejected {
                    agent: m.agent,
                    epoch: m.epoch,
                    wave: m.wave,
                    reason: 1,
                    cmd_seq: 0,
                })
            }
            _ => o,
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

    /// The player's report from the refinery at `at` (and, with [`Run::sight`], the sightings
    /// it routes).
    fn observe(&mut self, at: Tick) {
        if self.sight {
            self.c
                .observe_player(&mut self.w, synth::PLAYER, synth::REFINERY_POS, at);
        } else {
            self.c
                .observe_presence(&mut self.w, synth::PLAYER, synth::REFINERY_POS, at);
            self.w.observe_area(
                synth::PLAYER,
                synth::REFINERY_POS,
                at,
                self.c.tun.view_radius,
            );
        }
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
                self.observe(next);
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
                op: next_op(),
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

/// MZB-23 (review): a record's op id is spent as rtsim-jni spends it, a deferred one too. The
/// camp records got their engine half in M4 (MZB-150) and the grove's tribute records in M9, so
/// what is left deferred is the rail's, which carry no op id.
#[test]
fn a_booked_record_spends_its_op_id() {
    let mut r = Run::new(42, quiet_goblins());
    let op = 0xABCD;
    let paid = || {
        Obs::ChiefPaid(boundary::ChiefPaid {
            faction: synth::CAMP,
            slot: 0,
            generation: 0,
            flags: 0,
            player: (0, 0),
            op,
            items: Vec::new(),
        })
    };
    assert!(!Conflict::later(&paid()));
    assert!(paid().op().is_some_and(|o| o == op));
    r.ingest(paid(), Tick(5));
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
    let focus = r.c.feud(r.f, r.p).unwrap().focus.clone().unwrap();
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
            .as_ref()
            .unwrap()
            .district
            .is_none()
    );
    answer_to(&mut r, &e, boundary::ANSWER_ATTACKED, at + 5);
    let focus = r.c.feud(r.f, r.p).unwrap().focus.clone().unwrap();
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
    let focus = theirs.focus.clone().expect("a focus at the envoy's place");
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
        r.c.feud(r.f, r.p).unwrap().focus.as_ref().unwrap().district,
        Some(synth::REFINERY),
        "at the demand's district"
    );
}

/// A raid band on the way, ordered at `Tick(20)` after two killings. The run's player is not
/// routed ([`Run::sight`]): its raid is never met on its march, and starts its fight from an
/// abstract band at the arrival, as the replay-gate tests below read it.
fn raid_band(r: &mut Run) -> Handle<Party> {
    r.sight = false;
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

/// A raid band's watched fight whose first wave Java refused, live: every other command goes to
/// the plugin, which kills a quarter of each wave sent at the base 1,500 ticks after it spawns.
/// Returns the deaths the plugin reported for the band's fight epoch, the deaths the engine
/// counted, the band's members alive before and after, and the epochs it recalled.
fn a_fight_with_its_first_wave_refused() -> (u32, u64, u32, u32, Vec<u32>) {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.fight = Some(synth::Fight {
        kill_share: 0.25,
        after: 1_500,
        kill_chief: false,
        only_at_base: true,
    });
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let agent = Party::agent(band);
    let arrival = *r.c.parties[band].times.last().unwrap();
    r.run(Tick(arrival.0 - 3_000));
    let (alive, counted) = (r.c.parties[band].alive(), r.c.stats.casualties);
    r.watch = true;
    r.next_obs = r.c.now();
    let fight_end = arrival + r.c.tun.watched_fight_ticks + 600;
    // The band walks home in its bodies after the fight (MZB-456) and is recalled when it gets
    // there: run until its row is gone.
    let end = Tick(fight_end.0 + day(2.0).0);
    let (mut fight, mut reported, mut recalled) = (None, 0, Vec::new());
    let (mut counted_at_end, mut alive_at_end) = (None, None);
    let mut t = r.c.now();
    while t < end && r.c.parties.contains(band) {
        let next = [
            r.c.next_due(),
            r.plugin.next_due(),
            r.watch.then_some(r.next_obs),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(end)
        .clamp(t + 1, end);
        for (at, o) in r.plugin.due(next) {
            if let Obs::Casualties(x) = &o
                && x.agent == agent
                && Some(x.epoch) == fight
            {
                reported += x.count;
            }
            r.c.ingest(&mut r.w, &mut NoInfrastructure, &o, at);
        }
        r.c.run_until(&mut r.w, &mut NoInfrastructure, next);
        if r.watch && next >= r.next_obs {
            r.observe(next);
            r.next_obs = next + 1_200;
        }
        let mut to_plugin = Vec::new();
        for cmd in r.c.take_commands() {
            match &cmd {
                Cmd::Materialize(m) if m.agent == agent && m.wave == 0 && m.arc_radius > 0 => {
                    fight = Some(m.epoch);
                    let o = Obs::SpawnRejected(boundary::BandSpawnRejected {
                        agent,
                        epoch: m.epoch,
                        wave: 0,
                        reason: 1,
                        cmd_seq: 0,
                    });
                    r.c.ingest(&mut r.w, &mut NoInfrastructure, &o, next);
                }
                Cmd::Recall(x) if x.agent == agent => {
                    recalled.push(x.epoch);
                    to_plugin.push(cmd);
                }
                _ => to_plugin.push(cmd),
            }
        }
        r.plugin.apply(to_plugin, next);
        t = next;
        if counted_at_end.is_none() && t >= fight_end {
            counted_at_end = Some(r.c.stats.casualties - counted);
            alive_at_end = r.c.parties.get(band).map(|x| x.alive());
        }
    }
    assert!(fight.is_some(), "the band's fight began");
    assert!(!r.c.parties.contains(band), "the band walked home");
    (
        reported,
        counted_at_end.unwrap(),
        alive,
        alive_at_end.unwrap(),
        recalled,
    )
}

/// After a refused first wave, a watched fight's later waves are the band's in Minecraft: their
/// deaths count, and the fight's end recalls them. The refusal leaves the band unembodied with
/// nothing placed; the next wave commanded embodies it again.
#[test]
fn a_fight_whose_first_wave_was_refused_counts_its_later_waves_deaths() {
    let (reported, counted, before, after, recalled) = a_fight_with_its_first_wave_refused();
    assert!(reported > 0, "the plugin killed members of a later wave");
    assert_eq!(
        (u64::from(reported), before - after),
        (counted, reported),
        "every death the plugin reported is counted against the band"
    );
    // The band is not met on its march ([`raid_band`]), so the fight is its first epoch.
    assert_eq!(
        recalled,
        vec![1],
        "the fight's end recalls the later waves' bodies"
    );
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
        op: next_op(),
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

/// A replayed BAND_MATERIALIZED for `band` at `epoch`, `wave`, with `spawned` placed at once: a
/// sighting's (no [`boundary::BAND_FIGHT`]).
fn materialized(band: Handle<Party>, epoch: u32, wave: u32, spawned: u32) -> Obs {
    Obs::Materialized(boundary::BandMaterialized {
        agent: Party::agent(band),
        epoch,
        dimension: 0,
        wave,
        spawned,
        flags: 0,
    })
}

/// [`materialized`] for a fight's wave ([`boundary::BAND_FIGHT`], RP-56).
fn fought(band: Handle<Party>, epoch: u32, wave: u32, spawned: u32) -> Obs {
    Obs::Materialized(boundary::BandMaterialized {
        agent: Party::agent(band),
        epoch,
        dimension: 0,
        wave,
        spawned,
        flags: u32::from(boundary::BAND_HOSTILE | boundary::BAND_FIGHT),
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

/// fixes5 item 1 (RP-57): a replayed BAND_SPAWN_REJECTED of epoch 1 (the last session's sighting
/// of the band on its road, which Java refused) for a raid at epoch 0 that is not held, on an
/// engine from its pages. No epoch is used twice (§5.3): after the replay end the band's next
/// body takes 2. The replay used to leave no floor, so it took 1 again. Control: no record, 1.
#[test]
fn a_replayed_refusal_for_a_party_not_held_raises_its_epoch_floor() {
    let next_body = |record: bool| {
        let mut r = Run::new(36, quiet_goblins());
        let band = raid_band(&mut r);
        r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
        while r.c.parties[band].phase != Phase::Out {
            assert!(r.c.now() < day(4.0), "the raid sets out");
            r.run(r.c.now() + 600);
        }
        r.c = restarted(&r);
        let x = &r.c.parties[band];
        assert!(x.phase == Phase::Out && !x.embodied && x.epoch == 0);
        assert!(
            r.c.now() + 1 < *x.times.last().unwrap(),
            "on its road: not held"
        );
        if record {
            let at = r.c.now() + 1;
            r.ingest(refusal(band, 1, 0), at);
            assert_eq!(
                r.c.parties[band].epoch, 0,
                "not held: the floor, not the epoch"
            );
        }
        r.c.replay_done();
        first_materialize(&mut r, band)
            .expect("the band came into view")
            .epoch
    };
    assert_eq!(next_body(false), 1, "control");
    assert_eq!(next_body(true), 2);
}

/// BAND_SPAWN_REJECTED for `band` at `epoch`, `wave` (1, not ticking).
fn refusal(band: Handle<Party>, epoch: u32, wave: u32) -> Obs {
    Obs::SpawnRejected(boundary::BandSpawnRejected {
        agent: Party::agent(band),
        epoch,
        wave,
        reason: 1,
        cmd_seq: 0,
    })
}

/// How a record reaches the engine in [`every_record_naming_an_epoch_raises_its_floor`].
#[derive(Clone, Copy, Debug)]
enum Reached {
    /// Replayed on an engine from its pages.
    Pages,
    /// Replayed on a fresh engine (RP-31).
    Fresh,
    /// A copy (REASSERT) replayed on an engine from its pages.
    Copy,
}

/// fixes5 item 1's sweep (RP-57): every record kind that names a party's epoch, at epoch 4 for a
/// raid at epoch 0 that is not held, replayed on an engine from its pages, on a fresh engine and
/// as a copy. After the replay end the band holds epoch 4 at least, so its next body takes 5 or
/// more (`next_epoch`): none of them can be used again. The camp records (CAMP_CENSUS,
/// CAMP_BODIES, CAMP_OUT) carry a camp's epoch, Java's op id, which the engine never allocates.
/// A BAND_CASUALTIES whose op the party counted already (MZB-589's dedupe, the M6 merge) raises
/// it too: the fold comes before the dedupe.
#[test]
fn every_record_naming_an_epoch_raises_its_floor() {
    let player = (1, 2);
    let counted = next_op();
    let records = |band: Handle<Party>| {
        let agent = Party::agent(band);
        vec![
            Obs::Casualties(boundary::BandCasualties {
                agent,
                epoch: 4,
                by: boundary::BY_MOB,
                body: 0,
                count: 1,
                killer: (0, 0),
                op: counted,
            }),
            Obs::State(boundary::BandState {
                agent,
                epoch: 4,
                flags: 0,
                dimension: 0,
                present: 0,
                pos: (0.0, 0.0, 0.0),
            }),
            materialized(band, 4, 0, 6),
            fought(band, 4, 0, 6),
            refusal(band, 4, 0),
            refusal(band, 4, 1),
            Obs::Dematerialized(boundary::BandDematerialized {
                agent,
                epoch: 4,
                reason: 1,
                survivors: Vec::new(),
            }),
            Obs::Casualties(boundary::BandCasualties {
                agent,
                epoch: 4,
                by: boundary::BY_MOB,
                body: 0,
                count: 1,
                killer: (0, 0),
                op: next_op(),
            }),
            Obs::ChiefKilled(boundary::ChiefKilled {
                agent,
                epoch: 4,
                by: boundary::BY_MOB,
                killer: (0, 0),
                op: 901,
            }),
            Obs::EnvoyAnswer(boundary::EnvoyAnswer {
                agent,
                epoch: 4,
                answer: boundary::ANSWER_REFUSED,
                player,
            }),
            Obs::TributePaid(boundary::TributePaid {
                faction: synth::CAMP,
                agent,
                epoch: 4,
                player,
                op: 902,
                items: Vec::new(),
            }),
            Obs::Looted(boundary::BandLooted {
                agent,
                epoch: 4,
                op: 903,
                items: Vec::new(),
            }),
            Obs::TrainPassed(boundary::TrainPassed {
                agent,
                epoch: 4,
                flags: 0,
                carriage: 0,
                train: 7,
            }),
        ]
    };
    let mut missed = Vec::new();
    for reached in [Reached::Pages, Reached::Fresh, Reached::Copy] {
        let mut r = Run::new(36, quiet_goblins());
        let band = raid_band(&mut r);
        r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
        r.c.parties[band].casualty_ops.push(counted);
        for o in records(band) {
            assert_eq!(o.party_epoch(), Some((Party::agent(band), 4)), "{o:?}");
            let mut c = match reached {
                Reached::Fresh => {
                    let mut c =
                        Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &r.c.save()).unwrap();
                    c.begin_replay(false);
                    c
                }
                Reached::Pages | Reached::Copy => restarted(&r),
            };
            let mut w = world_from(36, &r.w.save());
            let at = c.now() + 1;
            match reached {
                Reached::Copy => c.ingest_copy(&mut w, &mut NoInfrastructure, &o, at),
                Reached::Pages | Reached::Fresh => c.ingest(&mut w, &mut NoInfrastructure, &o, at),
            }
            c.replay_done();
            let epoch = c.parties.get(band).map(|x| x.epoch);
            if epoch < Some(4) {
                missed.push(format!("{reached:?} {:#06x}: {epoch:?}", o.kind()));
            }
        }
    }
    assert!(missed.is_empty(), "{missed:#?}");
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
/// leave the books, as live `start_fight` drops them (RP-46): no recalled stack. A record below
/// the party's wave, or of an older epoch, changes nothing.
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
    // A newer epoch: epoch 1's bodies leave the books unfolded, epoch 2 has the quota.
    back.ingest(
        &mut r.w,
        &mut NoInfrastructure,
        &materialized(band, 2, 0, 16),
        at + 3,
    );
    let x = &back.parties[band];
    assert_eq!((x.epoch, x.wave), (2, 1));
    assert!(x.recalled.is_empty(), "no recalled stack: {:?}", x.recalled);
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
                    op: next_op(),
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
        op: next_op(),
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

/// A band in a fight, in Minecraft at a new epoch, with the epoch, its first unit's body and its
/// size: the setting of the MZB-589 tests (a goblin raid of 8 and 4, routed at 6 losses).
fn embodied_in_a_fight(r_band: (Run, Handle<Party>)) -> (Run, Handle<Party>, u32, u32) {
    let (mut r, band) = r_band;
    let epoch = {
        let x = &mut r.c.parties[band];
        x.epoch += 1;
        x.embodied = true;
        x.spawned = x.units.clone();
        // The fight goes on for days: the band the helper made Fighting has the muster's end as
        // its `until`, in the past, so the first loss would bring the fight's end due.
        x.until = fight_at() + 3 * TICKS_PER_DAY;
        x.epoch
    };
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(r.c.parties[band].units[0].0).body)
    };
    (r, band, epoch, body)
}

/// BAND_CASUALTIES of `count` members of `body`, killed by the player, as the entity's death
/// listener journals it, carrying its Java op id.
fn killed(
    band: Handle<Party>,
    epoch: u32,
    body: u32,
    count: u32,
    op: u64,
) -> boundary::BandCasualties {
    boundary::BandCasualties {
        agent: Party::agent(band),
        epoch,
        by: boundary::BY_PLAYER,
        body,
        count,
        killer: synth::PLAYER,
        op,
    }
}

/// MZB-589: the engine's reading of a band, to compare a copy's effect with a control's.
#[derive(Debug, PartialEq)]
struct Count {
    units: Vec<(u8, u32)>,
    spawned: Vec<(u8, u32)>,
    recalled: Vec<(u32, Vec<(u8, u32)>)>,
    casualties: u64,
    grievance: i64,
    breaker: Option<ActorId>,
    routs: u64,
    phase: Phase,
    ops: usize,
}

fn counted(r: &Run, c: &Conflict, band: Handle<Party>) -> Count {
    let x = &c.parties[band];
    Count {
        units: x.units.clone(),
        spawned: x.spawned.clone(),
        recalled: x.recalled.clone(),
        casualties: c.stats.casualties,
        grievance: (c.feud_view(synth::CAMP, r.p).unwrap().grievance * 1e6).round() as i64,
        breaker: x.breaker,
        routs: c.stats.routs,
        phase: x.phase,
        ops: x.casualty_ops.len(),
    }
}

/// MZB-589: the engine of `r` after the pages are saved and loaded again, in the same session.
fn reopened(r: &Run, c: &Conflict) -> Conflict {
    Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &c.save()).unwrap()
}

/// MZB-589: the death listener's record is committed again under a new sequence when the engine's
/// replay forgot its op (the reassert of an entity-journaled record, after a world save). The same
/// record twice in one epoch is one death: one unit, one count, one Killing signal and one move
/// of the grudge, the same as the engine that saw it once.
#[test]
fn a_casualty_copy_in_one_epoch_counts_once() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    let alive = r.c.parties[band].alive();
    let op = next_op();
    let cs = Obs::Casualties(killed(band, epoch, body, 1, op));
    let mut once = reopened(&r, &r.c);
    let grievance = counted(&r, &r.c, band).grievance;
    let at = r.c.now() + 1;
    r.ingest(cs.clone(), at);
    once.ingest(&mut r.w, &mut NoInfrastructure, &cs, at);
    let first = counted(&r, &r.c, band);
    assert_eq!(first.units[0].1, 8 - 1, "one member is dead");
    assert_eq!(r.c.parties[band].alive(), alive - 1);
    assert_eq!(first.casualties, 1);
    assert_eq!(first.ops, 1, "and the party knows the record");
    assert_eq!(first.breaker, Some(r.p), "the killer is known");
    assert!(
        first.grievance > grievance,
        "and the killing is held against them"
    );
    assert_eq!(first.spawned[0].1, 7, "the stack of bodies too");
    r.ingest(cs.clone(), at);
    // The copy moves nothing: not the units, the count, the grudge (one Killing signal, the
    // first's) nor the breaker, and the engine is the one that saw the record once.
    assert_eq!(counted(&r, &r.c, band), first, "the copy changes nothing");
    assert_eq!(
        counted(&r, &once, band),
        first,
        "the same as seeing it once"
    );
}

/// MZB-589: with the copy counted twice the band would rout early (the loss is a third, twice it
/// is two thirds and the goblins rout at a half).
#[test]
fn a_casualty_copy_does_not_rout_the_band_early() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    let alive = r.c.parties[band].alive();
    let routs = r.c.stats.routs;
    let cs = Obs::Casualties(killed(band, epoch, body, 4, next_op()));
    let at = r.c.now() + 1;
    for k in 0..3 {
        r.ingest(cs.clone(), at);
        assert_eq!(r.c.parties[band].alive(), alive - 4, "after copy {k}");
        assert_eq!(r.c.parties[band].phase, Phase::Fighting, "still fighting");
        assert_eq!(r.c.stats.routs, routs, "no rout");
        assert_eq!(r.c.stats.casualties, 4);
    }
}

/// MZB-589: two deaths of the same body in one epoch are two records with two ops, and both
/// count (the engine skips a copy, not a death).
#[test]
fn two_casualties_of_one_body_with_two_ops_both_count() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    let alive = r.c.parties[band].alive();
    let at = r.c.now() + 1;
    let parties = r.c.memory()["parties"];
    let (a, b) = (next_op(), next_op());
    r.ingest(Obs::Casualties(killed(band, epoch, body, 1, a)), at);
    r.ingest(Obs::Casualties(killed(band, epoch, body, 1, b)), at);
    assert_eq!(r.c.parties[band].alive(), alive - 2);
    assert_eq!(r.c.stats.casualties, 2);
    assert_eq!(r.c.parties[band].casualty_ops, vec![a, b]);
    assert_eq!(
        r.c.memory()["parties"],
        parties + 16,
        "the memory estimate counts the list"
    );
    // And a copy of either is still one death each.
    r.ingest(Obs::Casualties(killed(band, epoch, body, 1, a)), at);
    r.ingest(Obs::Casualties(killed(band, epoch, body, 1, b)), at);
    assert_eq!(r.c.parties[band].alive(), alive - 2);
    assert_eq!(r.c.stats.casualties, 2);
}

/// MZB-589: the list holds the records that took a unit, so it cannot outgrow the band. A record
/// for a stack that was gone already takes nothing and is not kept.
#[test]
fn the_list_keeps_only_the_casualties_that_took_a_unit() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    let at = r.c.now() + 1;
    // The second stack has four, and four of twelve is below the rout at a half.
    let c = r.c.cultures.get(0).clone();
    let other = body_id(&r.c.cultures, &c.unit(r.c.parties[band].units[1].0).body);
    assert_ne!(other, body);
    let (a, b) = (next_op(), next_op());
    r.ingest(Obs::Casualties(killed(band, epoch, other, 2, a)), at);
    r.ingest(Obs::Casualties(killed(band, epoch, other, 2, b)), at);
    let held = r.c.parties[band].units[1].1;
    assert_eq!(held, 0, "the second stack is gone");
    // A third record for the same stack takes nothing.
    let extra = next_op();
    r.ingest(Obs::Casualties(killed(band, epoch, other, 2, extra)), at);
    assert!(
        !r.c.parties[band].casualty_ops.contains(&extra),
        "a record that took no unit is not kept"
    );
    assert!(r.c.parties[band].casualty_ops.len() as u32 <= r.c.parties[band].start_units);
    assert_eq!(r.c.stats.casualties, 4);
}

/// MZB-589: the list is saved with the party, so a copy that comes after the pages were saved and
/// loaded again (in the session that went on, or in a restart's replay, where the bodies the pages
/// held stay embodied at their epoch, docs/replay-gate.md §5.1, RP-5) still counts once.
#[test]
fn a_casualty_copy_after_a_save_and_reopen_counts_once() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    let at = r.c.now() + 1;
    let cs = Obs::Casualties(killed(band, epoch, body, 1, next_op()));
    r.ingest(cs.clone(), at);
    let first = counted(&r, &r.c, band);
    assert_eq!(first.ops, 1);
    // The same session, the pages loaded again.
    let mut same = reopened(&r, &r.c);
    assert_eq!(counted(&r, &same, band), first, "the list is saved");
    same.ingest(&mut r.w, &mut NoInfrastructure, &cs, at);
    assert_eq!(counted(&r, &same, band), first, "a copy counts nothing");
    // The restart: the bodies of the pages stay embodied for the replay, which re-delivers the
    // record. The pages hold the dead already, and so the copy must not take them again.
    let mut back = restarted(&r);
    assert!(back.parties[band].embodied, "kept for the replay (RP-5)");
    assert_eq!(back.parties[band].epoch, epoch);
    assert_eq!(back.parties[band].spawned, r.c.parties[band].spawned);
    let before = counted(&r, &back, band);
    // As the host delivers a copy: through `ingest_copy`, which takes the record at any epoch
    // (`ingest` is a live commit, a different path).
    back.ingest_copy(&mut r.w, &mut NoInfrastructure, &cs, at);
    assert_eq!(counted(&r, &back, band), before, "the replay's copy too");
    assert_eq!(back.parties[band].alive(), r.c.parties[band].alive());
}

/// MZB-589: a raid on the refinery, the pages saved while it marches there unseen, a restart
/// that replays: the band's first copy of a casualty counts, the replay holds the arrival
/// (RP-23), and the record `second` arrives while the arrival is held, through `ingest_copy`
/// (`copy`) or as a plain recommit. Returns the raids decided while replaying right after it
/// (the hold ended: the arrival was decided unwatched), and the casualties counted.
fn a_casualty_while_an_arrival_is_held(second_is_new: bool, copy: bool) -> (usize, u64) {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    assert!(r.c.parties[band].phase == Phase::Out && !r.c.parties[band].embodied);
    let (pages, world) = (r.c.save(), r.w.save());
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    let c = back.cultures.get(0).clone();
    let body = body_id(&back.cultures, &c.unit(back.parties[band].units[0].0).body);
    let epoch = back.parties[band].epoch;
    let (a, b) = (next_op(), next_op());
    let first = Obs::Casualties(killed(band, epoch, body, 1, a));
    let at = back.now() + 1;
    back.ingest_copy(&mut w, &mut walls, &first, at);
    assert_eq!(
        back.parties[band].casualty_ops,
        vec![a],
        "the party counted it"
    );
    let counted = back.stats.casualties;
    // The arrival comes inside the replay and is held.
    back.run_until(&mut w, &mut walls, arrival + 10);
    let _ = back.take_commands();
    assert!(back.replaying());
    assert_eq!(back.parties[band].phase, Phase::Out, "held");
    assert!(walls.raids.is_empty(), "not decided yet");
    let second = Obs::Casualties(killed(
        band,
        epoch,
        body,
        1,
        if second_is_new { b } else { a },
    ));
    if copy {
        back.ingest_copy(&mut w, &mut walls, &second, arrival + 10);
    } else {
        back.ingest(&mut w, &mut walls, &second, arrival + 10);
    }
    (walls.raids.len(), back.stats.casualties - counted)
}

/// MZB-589 (the merge check's P3): a casualty the party counted already moves nothing, so it does
/// not end a held arrival either. `release_before` ran ahead of the dedupe in `casualties`, and
/// BAND_CASUALTIES has no op for `first_time`: the hold was removed, the arrival decided
/// unwatched, the count unchanged, and the hold's own fight record found no hold.
#[test]
fn a_casualty_the_party_counted_already_does_not_end_a_held_arrival() {
    for copy in [true, false] {
        assert_eq!(
            a_casualty_while_an_arrival_is_held(false, copy),
            (0, 0),
            "a known op, copy {copy}: the hold stays, nothing counts"
        );
    }
}

/// The control: a new op still releases the hold (a wipe-out or a rout sends the band home) and
/// counts.
#[test]
fn a_new_casualty_still_ends_a_held_arrival() {
    let (raids, counted) = a_casualty_while_an_arrival_is_held(true, true);
    assert_eq!(
        (raids, counted),
        (1, 1),
        "the hold ended, the death counted"
    );
}

/// MZB-589: a casualty of bodies kept as a recalled stack (the walk-off's dead) counts once as
/// well: a copy finds the op.
#[test]
fn a_casualty_copy_on_a_recalled_stack_counts_once() {
    let (mut r, band, epoch, body) = embodied_in_a_fight(chief_in_a_fight(36));
    {
        let x = &mut r.c.parties[band];
        x.recalled = vec![(epoch, std::mem::take(&mut x.spawned))];
        x.embodied = false;
    }
    let alive = r.c.parties[band].alive();
    let at = r.c.now() + 1;
    let cs = Obs::Casualties(killed(band, epoch, body, 1, next_op()));
    r.ingest(cs.clone(), at);
    let first = counted(&r, &r.c, band);
    assert_eq!(r.c.parties[band].alive(), alive - 1);
    assert_eq!(first.recalled[0].1[0].1, 7, "the stack lost its member");
    assert_eq!(first.casualties, 1);
    r.ingest(cs.clone(), at);
    assert_eq!(counted(&r, &r.c, band), first, "the copy counts nothing");
    // After a save and reopen too.
    let mut same = reopened(&r, &r.c);
    same.ingest(&mut r.w, &mut NoInfrastructure, &cs, at);
    assert_eq!(counted(&r, &same, band), first);
}

/// MZB-589: a copy that comes after the walk-off has settled (the despawn took the epoch's stack)
/// is no death, as before the list; the copy changes nothing: units, count, grudge and breaker.
#[test]
fn a_casualty_copy_after_the_recall_settled_changes_nothing() {
    let (mut r, band, epoch, body) =
        embodied_in_a_fight(raid_in_a_fight_at(36, synth::REFINERY_POS));
    {
        let x = &mut r.c.parties[band];
        x.recalled = vec![(epoch, std::mem::take(&mut x.spawned))];
        x.embodied = false;
    }
    let at = r.c.now() + 1;
    let cs = Obs::Casualties(killed(band, epoch, body, 1, next_op()));
    r.ingest(cs.clone(), at);
    let despawn = despawn_with(&r, band, epoch, 0);
    r.ingest(despawn, at);
    assert!(r.c.parties[band].recalled.is_empty(), "the stack is spent");
    let settled = counted(&r, &r.c, band);
    r.ingest(cs.clone(), at);
    assert_eq!(counted(&r, &r.c, band), settled, "the copy changes nothing");
    let mut same = reopened(&r, &r.c);
    same.ingest(&mut r.w, &mut NoInfrastructure, &cs, at);
    assert_eq!(counted(&r, &same, band), settled, "nor after a reopen");
}

/// MZB-589: a party's slot is reused when it has ended, under a new generation, and the list went
/// with the party. A copy of the old band's record carries the old agent id: it is no record of
/// the new party (`party_of` checks the handle's generation), whatever its epoch (the new party
/// starts at 0, as does an old one's first). Handles retire a slot before its u16 generation would
/// wrap (`rtsim-kernel` `handle.rs`), so no old agent id comes back to a live party.
#[test]
fn a_casualty_copy_of_an_ended_party_does_nothing_to_the_party_in_its_slot() {
    let mut r = Run::new(36, quiet_goblins());
    let old = raid_band(&mut r);
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(r.c.parties[old].units[0].0).body)
    };
    // The old band lost a member at epoch 0, which it counted.
    {
        let x = &mut r.c.parties[old];
        x.embodied = true;
        x.spawned = x.units.clone();
    }
    let op = next_op();
    let cs = Obs::Casualties(killed(old, 0, body, 1, op));
    r.ingest(cs.clone(), Tick(40));
    assert_eq!(r.c.parties[old].casualty_ops, vec![op]);
    // It ends (as it does when it is home), and the next band takes its slot.
    r.c.parties.remove(old);
    let new =
        r.c.order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(40),
        )
        .unwrap();
    assert_eq!(new.index(), old.index(), "the slot is reused");
    assert_ne!(Party::agent(new), Party::agent(old));
    assert_eq!(r.c.parties[new].epoch, 0);
    {
        let x = &mut r.c.parties[new];
        x.embodied = true;
        x.spawned = x.units.clone();
    }
    assert!(r.c.parties[new].casualty_ops.is_empty());
    let before = counted(&r, &r.c, new);
    r.ingest(cs.clone(), Tick(40));
    assert_eq!(counted(&r, &r.c, new), before, "live: the old agent's copy");
    let mut same = reopened(&r, &r.c);
    same.ingest(&mut r.w, &mut NoInfrastructure, &cs, Tick(40));
    assert_eq!(counted(&r, &same, new), before, "after a reopen");
    let mut back = restarted(&r);
    back.ingest(&mut r.w, &mut NoInfrastructure, &cs, Tick(40));
    assert_eq!(counted(&r, &back, new).units, before.units, "and a restart");
    assert_eq!(back.stats.casualties, before.casualties);
    // The new band's own record, with the old op, is a death: the new party has not counted it.
    r.ingest(Obs::Casualties(killed(new, 0, body, 1, op)), Tick(40));
    assert_eq!(r.c.stats.casualties, before.casualties + 1);
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
        op: next_op(),
    })
}

/// RP-27 (MZB-131, docs/replay-gate.md §5.9): a casualty finds its stack by the record's epoch. A
/// death on the walk-off of epoch `e` while the band is embodied again at `e + 1` takes from the
/// recalled stack of `e` and caps `spawned` at the units left alive (RP-25's cap); one of `e + 1`
/// takes from `spawned`. Before it the death of `e` was dropped (`party_of` wants the current
/// epoch), and the despawn of `e` then counted that member as a survivor.
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
        x.spawned[0],
        (first.0, first.1 - 1),
        "and the newer epoch's bodies are capped at the alive"
    );
    assert_eq!(r.c.stats.casualties, 1);
    r.ingest(casualty_of(band, e + 1, body), at + 1);
    let x = &r.c.parties[band];
    assert_eq!(x.alive(), alive - 2);
    assert_eq!(
        x.spawned[0],
        (first.0, first.1 - 2),
        "a death of e + 1 takes from spawned"
    );
    assert_eq!(
        x.recalled[0].1[0],
        (first.0, first.1 - 1),
        "and not from e's stack"
    );
    assert_eq!(r.c.stats.casualties, 2);
}

/// RP-27 (the re-review's item 7): a walk-off death of `e` while the band is embodied again at
/// `e + 1`, and a plugin that despawns `e + 1` first, holding every body it placed. The cap on
/// `spawned` keeps the death: `alive - spawned + min(spawned, survivors)` with `spawned` capped
/// at the alive gives the alive, where the whole `spawned` gave the death back. `e`'s despawn
/// then counts no more deaths. Control: the bridge's order (ConflictBridge ends `e` before it
/// materializes `e + 1`), `e` despawned first, whose despawn clamps `spawned` itself.
#[test]
fn a_walk_off_death_is_not_undone_when_the_newer_epoch_despawns_first() {
    for newer_first in [true, false] {
        let mut r = Run::new(36, quiet_goblins());
        let (band, e, body) = walking_off_and_embodied_again(&mut r);
        let alive = r.c.parties[band].alive();
        let at = r.c.now() + 1;
        // Java holds every body of `e + 1` it placed, and those of `e` but the one that died.
        let newer = despawn_with(&r, band, e + 1, 0);
        let older = despawn_with(&r, band, e, 1);
        r.ingest(casualty_of(band, e, body), at);
        assert_eq!(r.c.parties[band].alive(), alive - 1);
        let (one, two) = if newer_first {
            (newer, older)
        } else {
            (older, newer)
        };
        r.ingest(one, at + 1);
        assert_eq!(
            r.c.parties[band].alive(),
            alive - 1,
            "the first despawn keeps the death, newer first {newer_first}"
        );
        r.ingest(two, at + 2);
        let x = &r.c.parties[band];
        assert!(!x.embodied && x.recalled.is_empty() && x.spawned.is_empty());
        assert_eq!(
            x.alive(),
            alive - 1,
            "and so does the second, newer first {newer_first}"
        );
        assert_eq!(r.c.stats.casualties, 1);
    }
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
/// straggler both times (the band's epoch is not the record's and no recalled stack has it),
/// whose epoch still reaches the floor (RP-57).
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
        assert_eq!(x.epoch, 3, "the record's floor, copy {copy}");
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
/// tick per raid, whatever its classes, and the members each sent, its classes summed. With
/// `rout`, every raider is held at the walls (none survive); otherwise the raid has no outcome.
#[derive(Default)]
struct Walls {
    raids: std::collections::BTreeSet<Tick>,
    members: BTreeMap<Tick, u32>,
    rout: bool,
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
        *self.members.entry(t.at).or_insert(0) += t.count;
        self.rout.then(|| rtsim_ecology::infra::RaidOutcome {
            survived: 0.0,
            floor: None,
            untested: false,
            stolen: rtsim_industry::ids::GoodVec::new(),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
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
            let o = r.refused(o);
            r.c.ingest(&mut r.w, infra, &o, at);
            journal.push((at, o));
        }
        r.c.run_until(&mut r.w, infra, next);
        if r.watch && next >= r.next_obs {
            r.observe(next);
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
/// and a restart from the pages, the world's and the engine's, replays the plugin's records (the
/// player's position forgotten, as the arm forgets it), ends its replay and runs on two days
/// with nobody near. `restart`: also a second restart, from both pages saved once the replay
/// holds the arrival, whose clock then runs that many ticks before the next record (past the
/// poll: it arrives again first). Since RP-42 the host takes no page while replaying, so this
/// restart happens only here, at engine level: it keeps `held_since`'s inference from the pages
/// tested as a defence.
/// `straggler`: a body of the band's older epoch dies in its fight ([`straggler`], RP-46).
fn a_raid_arriving_after_the_save(
    watched: bool,
    crash: bool,
    restart: Option<u64>,
    straggler: bool,
) -> Arrival {
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
    let (pages, world) = (r.c.save(), r.w.save());
    let (casualties, raids_watched) = (r.c.stats.casualties, r.c.stats.raids_watched);
    journal.clear();
    r.watch = watched;
    r.next_obs = r.c.now();
    if straggler {
        // The band is met on its march (epoch 1), and the player logs out: its bodies are
        // recalled, and the player back at the arrival, the band fights at the next epoch, with
        // the bodies of the first as the older epoch's.
        let from = r.c.now();
        r.sight = true;
        run_journaled(&mut r, &mut walls, from + 600, &mut journal);
        assert!(r.c.parties[band].embodied, "met on its march");
        r.watch = false;
        r.w.forget_player_pos(r.p);
        r.c.player_left(&mut r.w, r.p);
        r.flush(from + 600);
        assert!(!r.c.parties[band].embodied, "recalled");
        r.sight = false;
        r.watch = true;
        r.next_obs = from + 1_200;
    }
    let mut end = arrival;
    let mut straggled = !straggler;
    while r.c.parties.get(band).is_some() {
        end += 600;
        assert!(end < day(8.0), "the band comes home");
        run_journaled(&mut r, &mut walls, end, &mut journal);
        if !straggled && fight_materialized(&r, band, &journal) {
            straggled = true;
            self::straggler(&mut r, &mut walls, band, &mut journal);
        }
    }
    assert!(straggled, "the band fought");
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
    // The restart: the world and the engine from the pages (the world the live session went on
    // with is not the one Java restarts with).
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let tun = r.c.tun.clone();
    let cultures = r.c.cultures.clone();
    let mut back = Conflict::load(tun.clone(), cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    let mut restarted_once = restart.is_none();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
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
            w = world_from(36, &w.save());
            back.begin_replay(true);
            let to = back.now() + restart.unwrap_or(0);
            back.run_until(&mut w, &mut walls, to);
            assert_eq!(back.parties[band].phase, Phase::Out, "held again");
        }
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(restarted_once);
    back.resend_notices();
    assert!(!back.replaying());
    back.run_until(&mut w, &mut walls, end + 2 * TICKS_PER_DAY);
    let mut a = arrival_of(&back, &walls);
    if !watched {
        // The journal goes on past the hold's slack: no fight answered it, and it resolves
        // there, while replaying (RP-48).
        let slack = arrival + REPLAY_HOLD_SLACK_TICKS;
        assert!(
            a.raids.iter().all(|&t| t == slack),
            "at the hold's slack ({slack:?}): {a:?}"
        );
        // Resolved at the slack, the band comes home later than in the session that went on:
        // its pool is not compared.
        a.pool = f64::NAN;
    }
    a
}

/// fixes4 item 5 (RP-54): a raid on the refinery, the pages saved while it marches there
/// unseen, its arrival in view of the player, and Java refusing every wave of its fight
/// ([`Run::refuse`]). The session that went on fights unembodied to the fight's end, then walks
/// home. `crash`: a restart from the pages replays the records, which hold the arrival. Read
/// 600 ticks past the fight's end, the replay still running: the band's phase, whether embodied
/// before its walk home (a live band walks home in the bodies its books hold, MZB-456; a
/// replayed one is recalled, as no body of the session stands yet), epoch, phase end, fights
/// watched since the save, and abstract raids since the save.
fn a_refused_fight_after_the_save(crash: bool) -> (Phase, bool, u32, Tick, u64, Vec<Tick>) {
    let mut r = Run::new(36, quiet_goblins());
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.refuse = Some(band);
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    let x = &r.c.parties[band];
    assert!(x.phase == Phase::Out && !x.embodied, "on its road, unseen");
    let (pages, world) = (r.c.save(), r.w.save());
    let (watched, saved_at) = (r.c.stats.raids_watched, r.c.now());
    journal.clear();
    r.watch = true;
    r.next_obs = r.c.now();
    let end = arrival + r.c.tun.watched_fight_ticks + 600;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let refused = journal.iter().any(
        |(_, o)| matches!(o, Obs::SpawnRejected(x) if x.agent == Party::agent(band) && x.wave == 0),
    );
    assert!(refused, "its fight's first wave refused");
    journal.push((end, fillers_casualty()));
    let read = |c: &Conflict, walls: &Walls| {
        let x = &c.parties[band];
        let raids = walls.raids.range(saved_at + 1..).copied().collect();
        (
            x.phase,
            x.embodied && x.phase != Phase::Home,
            x.epoch,
            x.until,
            c.stats.raids_watched - watched,
            raids,
        )
    };
    if !crash {
        return read(&r.c, &walls);
    }
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(back.replaying());
    read(&back, &walls)
}

/// fixes4 item 5 (RP-54): a held arrival whose fight Java refused (BAND_SPAWN_REJECTED of its
/// first wave, at the fight's epoch) is the last session's fight, as an honoured
/// BAND_MATERIALIZED is: the refusal ends the hold. The replay used to read the refusal against
/// the party's older epoch, keep the hold and resolve the arrival unwatched at the slack, where
/// the session that went on fought.
#[test]
fn a_held_arrivals_refused_fight_is_fought_when_replayed() {
    let live = a_refused_fight_after_the_save(false);
    assert_eq!((live.1, live.4, live.5.len()), (false, 1, 0), "{live:?}");
    assert_eq!(a_refused_fight_after_the_save(true), live);
}

/// How a held arrival's fight records come in [`a_held_arrivals_fight`]'s replay.
#[derive(Clone, Copy, Debug)]
enum Records {
    /// Java's journal stalls at the fight's first record: it and every record after it come
    /// later than the session that went on had them, the fight's at the held arrival + this.
    FightAt(u64),
    /// Java refuses the fight's first wave (`every_wave`: and every later one; else they are
    /// placed), and its journal stalls at that refusal: it and every record after it come later
    /// than the session that went on had them, the refusal at the held arrival + `at`.
    Refused { at: u64, every_wave: bool },
}

/// What [`a_held_arrivals_fight`] came to: the session that went on's arrival and the replay's
/// (the replay's pool not compared), the items each took from players, the replay's fight
/// records that found no hold, and the arrival's tick.
#[derive(Debug)]
struct HeldFight {
    live: Arrival,
    back: Arrival,
    live_loot: i128,
    back_loot: i128,
    unheld: u64,
    arrival: Tick,
}

/// fixes4 item 2 (RP-56): a raid on the refinery, the pages saved while it marches there unseen,
/// the player watching from `watch` ticks before its arrival, so the band is seen on its road
/// and fought at its arrival; once its fight's first record is in, its bodies carry 5 items off
/// a player's container (BAND_LOOTED at the fight's epoch). The session goes on until the band
/// is home. A restart from the pages replays the records as `records` has them, ends its replay
/// and runs on two days.
fn a_held_arrivals_fight(watch: u64, records: Records) -> HeldFight {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.fight = Some(synth::Fight {
        kill_share: 0.25,
        after: 1_500,
        kill_chief: false,
        only_at_base: true,
    });
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    // Java places one of the fight's waves, whose bodies loot.
    let placed = !matches!(
        records,
        Records::Refused {
            every_wave: true,
            ..
        }
    );
    if let Records::Refused { every_wave, .. } = records {
        r.refuse = Some(band);
        r.refuse_wave0_only = !every_wave;
    }
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    let x = &r.c.parties[band];
    assert!(x.phase == Phase::Out && !x.embodied, "on its road, unseen");
    let (pages, world) = (r.c.save(), r.w.save());
    let (casualties, watched) = (r.c.stats.casualties, r.c.stats.raids_watched);
    journal.clear();
    run_journaled(
        &mut r,
        &mut walls,
        Tick(arrival.0 - watch - 1),
        &mut journal,
    );
    r.watch = true;
    r.next_obs = Tick(arrival.0 - watch);
    let mut end = arrival;
    let mut looted = false;
    while r.c.parties.get(band).is_some() {
        end += 100;
        assert!(end < day(8.0), "the band comes home");
        run_journaled(&mut r, &mut walls, end, &mut journal);
        if !looted && r.c.parties.contains(band) && fight_materialized(&r, band, &journal) {
            looted = true;
            let iron =
                r.w.goods
                    .find("iron")
                    .or_else(|| r.w.goods.ids().next())
                    .unwrap();
            let o = Obs::Looted(boundary::BandLooted {
                agent: Party::agent(band),
                epoch: r.c.parties[band].epoch,
                op: 77,
                items: vec![boundary::Item {
                    item: item_of(iron),
                    count: 5,
                }],
            });
            r.c.ingest(&mut r.w, &mut walls, &o, end);
            journal.push((end, o));
        }
    }
    assert_eq!(looted, placed, "the band fought, its bodies placed");
    let loot = |c: &Conflict| c.ledger.from_players.values().sum::<i128>();
    let live = Arrival {
        raids: walls.raids.iter().copied().collect(),
        casualties: r.c.stats.casualties - casualties,
        watched: r.c.stats.raids_watched - watched,
        pool: f64::NAN,
    };
    let live_loot = loot(&r.c);
    let fight = journal
        .iter()
        .position(|(_, o)| {
            matches!(o, Obs::Materialized(m)
                if m.agent == Party::agent(band) && m.flags & u32::from(boundary::BAND_FIGHT) != 0)
        })
        .unwrap_or(journal.len());
    assert_eq!(fight < journal.len(), placed, "the fight's record");
    let late;
    match records {
        Records::FightAt(at) => {
            late = (arrival.0 + at)
                .checked_sub(journal[fight].0.0)
                .expect("recorded after its decision");
            for (t, _) in &mut journal[fight..] {
                *t += late;
            }
        }
        Records::Refused { at, .. } => {
            let refused = journal
                .iter()
                .position(|(_, o)| {
                    matches!(o, Obs::SpawnRejected(x) if x.agent == Party::agent(band) && x.wave == 0)
                })
                .expect("the first wave refused");
            assert!(
                refused < fight,
                "refused before a later wave's record, if any"
            );
            late = (arrival.0 + at)
                .checked_sub(journal[refused].0.0)
                .expect("recorded after its decision");
            for (t, _) in &mut journal[refused..] {
                *t += late;
            }
        }
    }
    journal.push((end + late, fillers_casualty()));
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let (casualties, watched) = (back.stats.casualties, back.stats.raids_watched);
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    back.resend_notices();
    assert!(!back.replaying());
    back.run_until(&mut w, &mut walls, end + late + 2 * TICKS_PER_DAY);
    HeldFight {
        live,
        back: Arrival {
            raids: walls.raids.iter().copied().collect(),
            casualties: back.stats.casualties - casualties,
            watched: back.stats.raids_watched - watched,
            pool: f64::NAN,
        },
        live_loot,
        back_loot: loot(&back),
        unheld: back.stats.fights_unheld,
        arrival,
    }
}

impl HeldFight {
    /// The replay's fight as the session that went on had it, every record held.
    fn assert_as_live(&self) {
        let of = |a: &Arrival, loot: i128| (a.raids.clone(), a.casualties, a.watched, loot);
        assert_eq!(
            (of(&self.back, self.back_loot), self.unheld),
            (of(&self.live, self.live_loot), 0),
            "{self:?}"
        );
    }
}

/// [`a_held_arrivals_fight`] past the hold's slack: the replay resolved the arrival unwatched
/// there, once, and the fight's records count against nothing: no casualty, no item, no fight
/// watched, the record counted as unheld.
fn assert_resolved_at_the_slack(h: &HeldFight) {
    let slack = h.arrival + REPLAY_HOLD_SLACK_TICKS;
    assert_eq!(
        (
            h.back.raids.as_slice(),
            h.back.casualties,
            h.back.watched,
            h.back_loot,
            h.unheld
        ),
        ([slack].as_slice(), 0, 0, 0, 1),
        "{h:?}"
    );
}

/// fixes4 item 2 (RP-56): a held arrival's fight recorded past the hold's slack is no double.
/// The replay resolves the arrival unwatched at the slack (one abstract raid), and the late
/// record, a fight's by its echoed flag, finds no hold: counted, the epoch floor raised, the
/// band not embodied, so the fight's casualties and loot count against nothing. The replay used
/// to embody the band at the record's epoch and count them on top of the raid.
#[test]
fn a_held_arrivals_fight_recorded_after_the_slack_is_no_double() {
    let h = a_held_arrivals_fight(3_000, Records::FightAt(REPLAY_HOLD_SLACK_TICKS + 600));
    assert!(h.live.raids.is_empty() && h.live.watched == 1, "{h:?}");
    assert_resolved_at_the_slack(&h);
}

/// fixes4 item 2 (RP-56): the slack's edge. A fight recorded one tick inside it is the held
/// arrival's fight, as the session that went on had it (watched, its casualties and loot
/// counted once, no abstract raid); one exactly at `since` + the slack is past it: the poll
/// there resolved the arrival unwatched, and the record finds no hold, as one after the slack.
#[test]
fn a_held_arrivals_fight_recorded_exactly_at_the_slack_finds_no_hold() {
    let h = a_held_arrivals_fight(3_000, Records::FightAt(REPLAY_HOLD_SLACK_TICKS - 1));
    assert!(h.live.casualties > 0 && h.live_loot > 0, "{h:?}");
    h.assert_as_live();
    assert_resolved_at_the_slack(&a_held_arrivals_fight(
        3_000,
        Records::FightAt(REPLAY_HOLD_SLACK_TICKS),
    ));
}

/// fixes6 item 4 (RP-63): a held arrival's fight whose first wave alone Java refused, the
/// refusal recorded past the hold's slack. The replay resolves the arrival unwatched at the slack
/// (one abstract raid), and the late refusal finds no hold: it marks the fight's epoch as unheld,
/// as a late fight record does (RP-56), so that the later waves' casualties and loot count
/// against nothing. The replay used to mark nothing at the refusal.
#[test]
fn a_held_arrivals_refused_first_wave_recorded_after_the_slack_is_no_double() {
    let h = a_held_arrivals_fight(
        3_000,
        Records::Refused {
            at: REPLAY_HOLD_SLACK_TICKS + 600,
            every_wave: false,
        },
    );
    assert!(
        h.live.raids.is_empty() && h.live.watched == 1 && h.live.casualties > 0 && h.live_loot > 0,
        "{h:?}"
    );
    assert_resolved_at_the_slack(&h);
}

/// fixes6 item 4 (RP-63): as above, every wave refused, so the late refusal is the fight's only
/// record. It is counted as a fight record that found no hold, once.
#[test]
fn a_held_arrivals_fight_refused_whole_and_recorded_after_the_slack_counts_as_unheld() {
    let h = a_held_arrivals_fight(
        3_000,
        Records::Refused {
            at: REPLAY_HOLD_SLACK_TICKS + 600,
            every_wave: true,
        },
    );
    assert!(h.live.watched == 1 && h.live_loot == 0, "{h:?}");
    assert_resolved_at_the_slack(&h);
}

/// Whether `band` fights, its fight's BAND_MATERIALIZED journaled.
fn fight_materialized(r: &Run, band: Handle<Party>, journal: &[(Tick, Obs)]) -> bool {
    let x = &r.c.parties[band];
    x.phase == Phase::Fighting
        && journal.iter().any(|(_, o)| {
            matches!(o, Obs::Materialized(m) if m.agent == Party::agent(band) && m.epoch == x.epoch)
        })
}

/// RP-46 (the fix round's item 6): a body of `band`'s epoch before its fight's dies, after the
/// fight's BAND_MATERIALIZED: a straggler Java still had. The band was seen on its road at that
/// epoch, and `start_fight` took the next without folding its bodies into a recalled stack, so
/// live the death counts against no stack.
fn straggler(r: &mut Run, walls: &mut Walls, band: Handle<Party>, journal: &mut Vec<(Tick, Obs)>) {
    let x = &r.c.parties[band];
    let older = x.epoch - 1;
    let seen = journal.iter().any(|(_, o)| {
        matches!(o, Obs::Materialized(m)
            if m.agent == Party::agent(band) && m.epoch == older && m.spawned > 0)
    });
    assert!(seen, "seen on its road at epoch {older}");
    let c = r.c.cultures.get(0).clone();
    let o = Obs::Casualties(boundary::BandCasualties {
        agent: Party::agent(band),
        epoch: older,
        by: boundary::BY_MOB,
        body: body_id(&r.c.cultures, &c.unit(x.units[0].0).body),
        count: 1,
        killer: (0, 0),
        op: next_op(),
    });
    let at = r.c.now();
    let casualties = r.c.stats.casualties;
    r.c.ingest(&mut r.w, walls, &o, at);
    assert_eq!(r.c.stats.casualties, casualties, "live, no stack");
    journal.push((at, o));
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
        op: next_op(),
    })
}

/// fixes4 item 1 (RP-51): the largest gap between a live watched fight's decision
/// (`start_fight`) and its first BAND_MATERIALIZED, the data RP-48's slack is set from
/// (`ConflictStats::fight_record_gap_max`). A raid on the refinery, the player there from 3,000
/// ticks before its arrival: the band is fought at its arrival, and the plugin answers each
/// command a tick after it. A later record of the fight's epoch measures nothing: only the first
/// answers the decision.
#[test]
fn a_watched_fights_record_gap_is_measured_from_its_decision() {
    let mut r = Run::new(36, quiet_goblins());
    r.c.log_enabled = true;
    r.plugin.fight = Some(synth::Fight {
        kill_share: 0.25,
        after: 1_500,
        kill_chief: false,
        only_at_base: true,
    });
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    r.watch = true;
    r.next_obs = r.c.now();
    let mut end = r.c.now();
    while !fight_materialized(&r, band, &journal) {
        end += 100;
        assert!(end < arrival + 6_000, "the band fought");
        run_journaled(&mut r, &mut walls, end, &mut journal);
    }
    let epoch = r.c.parties[band].epoch;
    let decided =
        r.c.log
            .iter()
            .find(|l| l.what.contains("arrives where a player watches"))
            .map(|l| l.tick)
            .expect("the fight's decision");
    let records: Vec<(u32, u64)> = journal
        .iter()
        .filter_map(|(at, o)| match o {
            Obs::Materialized(m) if m.agent == Party::agent(band) => Some((m.epoch, at.0)),
            _ => None,
        })
        .collect();
    // Not met on its march ([`raid_band`]): the fight's record is the band's first, and it
    // answers the decision.
    assert_eq!(
        records[0].0, epoch,
        "the fight is the first record: {records:?}"
    );
    let fought = records[0].1;
    assert!(fought > decided, "{records:?}, decided at {decided}");
    assert_eq!(r.c.stats.fight_record_gap_max, fought - decided);
    r.ingest(materialized(band, epoch, 1, 0), Tick(decided + 5_000));
    assert_eq!(
        r.c.stats.fight_record_gap_max,
        fought - decided,
        "a later wave's record measures nothing"
    );
}

/// RP-23 (amendment b): a raid reaching a watched target in the replayed window, its
/// BAND_MATERIALIZED and casualties replayed: resolved once, by the fight (no abstract raid), its
/// casualties counted, and its pool as the session that went on left it.
#[test]
fn a_watched_arrival_in_the_replayed_window_is_resolved_once() {
    let live = a_raid_arriving_after_the_save(true, false, None, false);
    assert!(live.raids.is_empty(), "fought, not raided: {live:?}");
    assert_eq!(live.watched, 1);
    assert!(live.casualties >= 2, "{live:?}");
    assert_eq!(
        a_raid_arriving_after_the_save(true, true, None, false),
        live
    );
}

/// RP-23: the same with a restart from pages saved while the replay holds the arrival: its
/// fight's record comes before the poll arrives again (found by the pages' `seg` and route), and
/// after it (the map again). The host takes no such page since RP-42 (rtsim-jni's
/// `a_held_arrival_across_a_clean_close_before_the_replay_end_is_resolved_once`); this restart,
/// at engine level, keeps `held_since`'s inference, the defence, tested.
#[test]
fn a_held_arrival_survives_a_restart_from_pages_saved_mid_replay() {
    let live = a_raid_arriving_after_the_save(true, false, None, false);
    assert_eq!(
        a_raid_arriving_after_the_save(true, true, Some(0), false),
        live
    );
    assert_eq!(
        a_raid_arriving_after_the_save(true, true, Some(50), false),
        live
    );
}

/// RP-46 (the fix round's item 6): a straggler of the band's older epoch dies in its fight: live
/// it counts against no stack, and so replayed. The replay's honoured fight used to fold the
/// older epoch's bodies into its recalled stack, which counted the death.
#[test]
fn a_straggler_of_an_older_epoch_counts_as_live_when_replayed() {
    let live = a_raid_arriving_after_the_save(true, false, None, true);
    assert_eq!(
        live,
        a_raid_arriving_after_the_save(true, false, None, false)
    );
    assert_eq!(a_raid_arriving_after_the_save(true, true, None, true), live);
}

/// RP-23's control: an unwatched arrival in the window resolves abstractly once (and once in the
/// session that went on), at the hold's slack (RP-48): the journal goes on past it.
#[test]
fn an_unwatched_arrival_in_the_replayed_window_is_resolved_once_at_the_holds_slack() {
    let live = a_raid_arriving_after_the_save(false, false, None, false);
    assert_eq!(live.raids.len(), 1, "{live:?}");
    assert_eq!((live.watched, live.casualties), (0, 0));
    let back = a_raid_arriving_after_the_save(false, true, None, false);
    assert_eq!(back.raids.len(), 1, "{back:?}");
    assert_eq!((back.watched, back.casualties), (0, 0));
}

/// RP-23 (§6.1): a siege's next round is held as an arrival is. A siege on the refinery, nobody
/// near, waiting after its first round when the pages are saved; its next round comes in the
/// replayed window, the journal's last record `past` ticks after it. The session that went on
/// raids at the round. The replay holds it: with the last record inside the hold's slack it
/// raids nothing while replaying, and the round comes once, after the replay end; past it, the
/// round resolves once at the slack (RP-48). Returns the raids after the save, those while
/// replaying, and the replay end (the round's tick without a crash).
fn a_siege_round_after_the_save(crash: bool, past: u64) -> (Vec<Tick>, Vec<Tick>, Tick) {
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
    let (pages, world) = (r.c.save(), r.w.save());
    let end = round + past;
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
        return (after(&walls), Vec::new(), round);
    }
    journal.push((end, fillers_casualty()));
    let mut w = world_from(36, &world);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    for (at, o) in journal.iter().filter(|(at, _)| *at > saved_at) {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
    }
    assert_eq!(back.parties[band].phase, Phase::Waiting, "held or waiting");
    let replaying = after(&walls);
    let replay_end = back.now();
    back.resend_notices();
    back.run_until(&mut w, &mut walls, end + 600);
    (after(&walls), replaying, replay_end)
}

#[test]
fn a_siege_round_in_the_replayed_window_waits_for_the_replay_end() {
    let past = REPLAY_HOLD_SLACK_TICKS / 2;
    let (live, _, round) = a_siege_round_after_the_save(false, past);
    assert_eq!(live, vec![round], "the session that went on");
    let (back, replaying, replay_end) = a_siege_round_after_the_save(true, past);
    assert!(replaying.is_empty(), "nothing raided while replaying");
    assert_eq!(back.len(), 1, "once: {back:?}");
    assert!(back[0] >= replay_end, "after the replay end: {back:?}");
}

/// RP-48: the same with the journal going on past the hold's slack: no fight answered the hold,
/// so the round resolves there, once, while replaying.
#[test]
fn a_siege_round_in_the_replayed_window_resolves_at_the_holds_slack() {
    let past = REPLAY_HOLD_SLACK_TICKS + 600;
    let (live, _, round) = a_siege_round_after_the_save(false, past);
    assert_eq!(live, vec![round], "the session that went on");
    let (back, replaying, _) = a_siege_round_after_the_save(true, past);
    let slack = round + REPLAY_HOLD_SLACK_TICKS;
    assert_eq!(replaying, vec![slack], "at the slack, while replaying");
    assert_eq!(back, vec![slack], "once");
}

// ---- a held raid's hunt (RP-133) ----------------------------------------------------------------

/// Goblins whose every unit leaves an animal owed where it raided and nobody watched, as the
/// raptor does (MZB-424), capped at the raptor's 8.
fn hunting_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    d.hunt_cap = 8;
    for u in &mut d.units {
        u.hunts = 1;
    }
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// A district that holds no one back: every raid on it is survived whole, so its survivors owe
/// their hunts.
#[derive(Default)]
struct Open {
    raids: Vec<Tick>,
}

impl rtsim_ecology::infra::Infrastructure for Open {
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
        self.raids.push(t.at);
        Some(rtsim_ecology::infra::RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: false,
            stolen: rtsim_industry::ids::GoodVec::new(),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }
    fn good_name(&self, _: rtsim_industry::ids::Good) -> Option<String> {
        None
    }
}

/// Where the first session of [`a_held_raids_hunt`] ends.
#[derive(Clone, Copy)]
enum HuntEnd {
    /// It goes on: no crash.
    Live,
    /// It crashes once every band is home.
    Home,
    /// It crashes this many ticks after the first band's arrival.
    After(u64),
}

/// What the raids' hunts came to across the sessions: the animals killed in the world, the
/// OWED_HUNTs (op, count) each session's plugin was sent, and the hunts the engine still owes.
#[derive(Debug)]
struct HuntRun {
    killed: u32,
    live: Vec<(u64, u32)>,
    next: Vec<(u64, u32)>,
    owed: usize,
}

/// RP-133 (P2-1 of the M9 merge's check): raids on the refinery that nobody watches, one band
/// ordered at tick 20 and one more `gaps[i]` ticks after it for each gap, the pages saved while the
/// first marches there unseen, or (`save_after`) 20 ticks past the first hunt's answer. The plugin
/// applies each OWED_HUNT as it comes and answers `answer` ticks later (`None`: it never applies
/// one). The session goes on as `end` says; at a crash, the restart from both pages replays the
/// journal (the arrivals are held, §6.1), ends its replay and runs two days on, with a plugin that
/// forgot what it held and applied (MZB-426) and answers each hunt it is sent at once. The world
/// keeps the animals the first plugin killed.
fn a_held_raids_hunt(gaps: &[u64], answer: Option<u64>, save_after: bool, end: HuntEnd) -> HuntRun {
    let mut r = Run::new(36, hunting_goblins());
    let mut bands = vec![raid_band(&mut r)];
    for &g in gaps {
        let band =
            r.c.order_party(
                &mut r.w,
                synth::CAMP,
                PartyKind::Raid,
                Target::District(synth::REFINERY, synth::REFINERY_POS),
                None,
                Tick(20 + g),
            )
            .expect("another band");
        bands.push(band);
    }
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.plugin.hunt_kills = answer.map(|_| 99);
    r.plugin.answer_ticks = answer.unwrap_or(1);
    let arrival = *r.c.parties[bands[0]].times.last().unwrap();
    let mut walls = Open::default();
    let mut journal = Vec::new();
    let save_at = match (save_after, answer) {
        (true, Some(a)) => arrival + a + 20,
        _ => Tick(arrival.0 - 3_000),
    };
    run_journaled(&mut r, &mut walls, save_at, &mut journal);
    if save_after {
        assert!(r.c.stats.hunts_killed > 0, "saved after the hunt's answer");
    } else {
        assert!(
            bands.iter().all(|&b| r.c.parties[b].phase == Phase::Out),
            "saved while the bands march"
        );
    }
    let (pages, world) = (r.c.save(), r.w.save());
    journal.clear();
    match end {
        HuntEnd::After(k) => run_journaled(&mut r, &mut walls, arrival + k, &mut journal),
        HuntEnd::Live | HuntEnd::Home => {
            let mut to = arrival;
            while bands.iter().any(|&b| r.c.parties.get(b).is_some()) {
                to += 600;
                assert!(to < day(8.0), "the bands come home");
                run_journaled(&mut r, &mut walls, to, &mut journal);
            }
        }
    }
    let sent = |p: &FakeConflict| -> Vec<(u64, u32)> {
        p.hunts_sent.iter().map(|h| (h.op, h.count)).collect()
    };
    let live = sent(&r.plugin);
    if matches!(end, HuntEnd::Live) {
        return HuntRun {
            killed: r.plugin.animals_killed,
            live,
            next: Vec::new(),
            owed: r.c.factions[r.f as usize].hunts().len(),
        };
    }
    // Java journals other records all along: one past the bands'.
    let last = r.c.now();
    journal.push((last, fillers_casualty()));
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Open::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    back.resend_notices();
    let mut plugin = FakeConflict::new(synth::PLAYER);
    plugin.hunt_kills = Some(99);
    let mut next = Run {
        w,
        c: back,
        f: r.f,
        p: r.p,
        plugin,
        cmds: Vec::new(),
        watch: false,
        next_obs: Tick::ZERO,
        sight: r.sight,
        refuse: None,
        refuse_wave0_only: false,
    };
    let now = next.c.now();
    next.flush(now);
    run_journaled(
        &mut next,
        &mut walls,
        now + 2 * TICKS_PER_DAY,
        &mut Vec::new(),
    );
    HuntRun {
        killed: r.plugin.animals_killed + next.plugin.animals_killed,
        live,
        next: sent(&next.plugin),
        owed: next.c.factions[next.f as usize].hunts().len(),
    }
}

/// P2-1: the session that went on owes the raid's hunt at its arrival and the plugin answers it 5
/// ticks later, inside the hold's slack; a crash, and the replay holds the arrival. The replayed
/// HUNT_APPLIED ends the hold first (RP-59's rule, RP-133), so the hunt is owed under its live op
/// (hashed from the hold's tick) and spent by its own answer: the next session's plugin is sent no
/// hunt and the animals die once. Before RP-133 the answer found no hunt, the hold ended at the
/// slack with a new op, and the next session's plugin killed them again.
#[test]
fn a_held_raids_hunt_answered_inside_the_slack_kills_once() {
    let live = a_held_raids_hunt(&[], Some(5), false, HuntEnd::Live);
    assert_eq!(live.live.len(), 1, "{live:?}");
    assert!(live.killed > 0 && live.owed == 0, "{live:?}");
    let back = a_held_raids_hunt(&[], Some(5), false, HuntEnd::Home);
    assert_eq!(back.live, live.live, "the first session as the live one");
    assert_eq!(back.next, [], "no hunt to the next session: {back:?}");
    assert_eq!((back.killed, back.owed), (live.killed, 0), "{back:?}");
}

/// The answer after the hold's own end (its slack's poll): the hold resolves there, owing the hunt
/// under the op hashed from the arrival, which the late answer then spends.
#[test]
fn a_held_raids_hunt_answered_after_the_slack_kills_once() {
    let a = REPLAY_HOLD_SLACK_TICKS + 300;
    let live = a_held_raids_hunt(&[], Some(a), false, HuntEnd::Live);
    assert!(live.killed > 0 && live.owed == 0, "{live:?}");
    let back = a_held_raids_hunt(&[], Some(a), false, HuntEnd::Home);
    assert_eq!(back.next, [], "no hunt to the next session: {back:?}");
    assert_eq!((back.killed, back.owed), (live.killed, 0), "{back:?}");
}

/// Two raids on the district, the second arriving inside the first's slack and before its answer
/// (500 ticks after each hunt): live the second hunt replaces the first, and the first's late
/// answer leaves the second's own count (MZB-449). The first answer ends both holds, in their
/// order, so the replay owes the same two ops as live, and each answer books its own.
#[test]
fn two_held_raids_on_a_district_kill_what_the_live_session_killed() {
    let gaps = [380];
    let live = a_held_raids_hunt(&gaps, Some(500), false, HuntEnd::Live);
    assert!(live.live.len() >= 2, "{live:?}");
    assert!(live.killed > 0 && live.owed == 0, "{live:?}");
    let back = a_held_raids_hunt(&gaps, Some(500), false, HuntEnd::Home);
    assert_eq!(back.next, [], "no hunt to the next session: {back:?}");
    assert_eq!((back.killed, back.owed), (live.killed, 0), "{back:?}");
    // Answered at once, the second raid comes after the first hunt is spent.
    let live = a_held_raids_hunt(&gaps, Some(5), false, HuntEnd::Live);
    let back = a_held_raids_hunt(&gaps, Some(5), false, HuntEnd::Home);
    assert_eq!(back.next, [], "{back:?}");
    assert_eq!((back.killed, back.owed), (live.killed, 0), "{back:?}");
}

/// The crash before the plugin answered (it never applies one): the journal goes past the slack,
/// so the hold resolves while replaying, under the live op, and the next session's plugin is sent
/// that hunt once and kills once. With the journal ending inside the slack the hold is cleared at
/// the replay end and decided after it, live: its op is the poll's, which no plugin has seen, and
/// it kills once too.
#[test]
fn a_held_raids_hunt_never_answered_goes_once_to_the_next_session() {
    let live = a_held_raids_hunt(&[], None, false, HuntEnd::Live);
    assert_eq!((live.killed, live.owed), (0, 1), "{live:?}");
    let [(op, count)] = live.live[..] else {
        panic!("{live:?}");
    };
    let past = REPLAY_HOLD_SLACK_TICKS + 300;
    let back = a_held_raids_hunt(&[], None, false, HuntEnd::After(past));
    assert_eq!(back.next, [(op, count)], "the live hunt, once: {back:?}");
    assert_eq!((back.killed, back.owed), (count, 0), "{back:?}");
    let inside = a_held_raids_hunt(&[], None, false, HuntEnd::After(100));
    let [(_, n)] = inside.next[..] else {
        panic!("{inside:?}");
    };
    assert_eq!((inside.killed, inside.owed), (n, 0), "{inside:?}");
}

/// The control: the pages saved after the hunt's answer. The replay has no arrival to hold, and
/// nothing is owed or sent again.
#[test]
fn a_raids_hunt_answered_before_the_save_is_not_sent_again() {
    let live = a_held_raids_hunt(&[], Some(5), true, HuntEnd::Live);
    assert!(live.killed > 0 && live.owed == 0, "{live:?}");
    let back = a_held_raids_hunt(&[], Some(5), true, HuntEnd::Home);
    assert_eq!(back.next, [], "{back:?}");
    assert_eq!((back.killed, back.owed), (live.killed, 0), "{back:?}");
}

/// The world as `pages` (its `save`) left it, for a restart (`Run::new`'s world, seed `seed`).
fn world_from(seed: u64, pages: &[u8]) -> World {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    World::load(synth::world_config(seed), goods, book, pages).unwrap()
}

/// RP-45 (the fix round's item 5): a siege on the refinery, nobody near, the pages saved while it
/// marches there unseen. Its arrival comes in the replayed window and is held, the journal's last
/// record inside the hold's slack (RP-48); its poll arrives again after the replay end. `crash`:
/// the restart, the world and the engine from the pages.
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
    let end = arrival + REPLAY_HOLD_SLACK_TICKS / 2;
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

/// [`quiet_goblins`] with five days' patience, so that a siege waits through two rounds after
/// its arrival's.
fn patient_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    d.patience_days = 5.0;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// A siege as [`a_siege_held_then`] reads it: its phase, whether it is embodied (before its walk
/// home: a live band walks home in the bodies its books hold, a replayed one is recalled, as no
/// body of the session stands yet, MZB-456) and at which epoch, when its phase ends, the fights
/// watched since the save, and the abstract raids since the save (by when).
#[derive(Debug)]
struct SiegeAt {
    phase: Phase,
    embodied: bool,
    epoch: u32,
    until: Tick,
    watched: u64,
    raids: Vec<Tick>,
}

impl SiegeAt {
    fn of(c: &Conflict, band: Handle<Party>, walls: &Walls, watched: u64, saved_at: Tick) -> Self {
        let x = &c.parties[band];
        SiegeAt {
            phase: x.phase,
            embodied: x.embodied && x.phase != Phase::Home,
            epoch: x.epoch,
            until: x.until,
            watched: c.stats.raids_watched - watched,
            raids: walls
                .raids
                .iter()
                .copied()
                .filter(|&t| t > saved_at)
                .collect(),
        }
    }

    /// As the session that went on left it, its raids up to the hold's slack later
    /// ([`REPLAY_HOLD_SLACK_TICKS`], RP-48): a replay resolves an unwatched decision there.
    fn assert_as(&self, live: &SiegeAt) {
        let late = |b: &Tick, l: &Tick| l <= b && b.0 <= l.0 + REPLAY_HOLD_SLACK_TICKS;
        assert!(
            self.raids.len() == live.raids.len()
                && self.raids.iter().zip(&live.raids).all(|(b, l)| late(b, l)),
            "replayed {self:?}, the session that went on {live:?}"
        );
        let state = |s: &SiegeAt| (s.phase, s.embodied, s.epoch, s.until, s.watched);
        assert_eq!(
            state(self),
            state(live),
            "replayed {self:?}, the session that went on {live:?}"
        );
    }
}

/// What follows a held, unwatched siege decision in [`a_siege_held_then`].
#[derive(Clone, Copy, Debug)]
enum Then {
    /// A player comes into view of the waiting band 3,000 ticks after the decision, once (seen,
    /// no fight), and goes stale.
    Sighting,
    /// The same 300 ticks after the decision: its record inside the hold's slack (RP-56).
    SightingInSlack,
    /// A player stands at the refinery from 600 ticks before the band's next round on, not
    /// routed ([`Run::sight`]): the round is fought from the waiting band.
    Fight,
}

/// RP-48 (the re-review's P1, docs/replay-gate.md §6.1): a siege on the refinery, nobody near at
/// a decision that asks for a watcher: its arrival (`round` false: the pages saved while it
/// marches there unseen) or its next round after it (`round` true: saved while it waits). The
/// decision resolves unwatched (the abstract raid) and the band waits for its next round; then
/// `then`. `crash`: a restart from the pages, the world's and the engine's, replays the plugin's
/// records, which hold the decision. The siege is read at the last record, the replay still
/// running. Returns it, and the decision's tick.
fn a_siege_held_then(round: bool, then: Then, crash: bool) -> (SiegeAt, Tick) {
    let mut r = Run::new(36, patient_goblins());
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
    let decision = if round {
        let mut to = r.c.now();
        while r.c.parties[band].phase != Phase::Waiting {
            to += 600;
            assert!(to < day(6.0), "the siege waits at its target");
            run_journaled(&mut r, &mut walls, to, &mut journal);
        }
        r.c.parties[band].until
    } else {
        let arrival = *r.c.parties[band].times.last().unwrap();
        run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
        arrival
    };
    let x = &r.c.parties[band];
    assert!(!x.embodied && decision < x.bored_at, "unseen, not bored");
    let saved_at = r.c.now();
    let (pages, world) = (r.c.save(), r.w.save());
    let watched = r.c.stats.raids_watched;
    journal.clear();
    // Past the decision: up to the sighting's, inside the slack, for that one.
    let past = match then {
        Then::SightingInSlack => 1,
        _ => 600,
    };
    run_journaled(&mut r, &mut walls, decision + past, &mut journal);
    let x = &r.c.parties[band];
    assert_eq!(
        (x.phase, x.embodied),
        (Phase::Waiting, false),
        "resolved unwatched"
    );
    assert!(walls.raids.contains(&decision), "{:?}", walls.raids);
    let next = x.until;
    let end = match then {
        Then::Sighting | Then::SightingInSlack => {
            let seen = decision
                + match then {
                    Then::Sighting => 3_000,
                    _ => 300,
                };
            r.watch = true;
            r.next_obs = seen;
            run_journaled(&mut r, &mut walls, seen + 1, &mut journal);
            r.watch = false;
            run_journaled(&mut r, &mut walls, seen + 600, &mut journal);
            let x = &r.c.parties[band];
            assert_eq!(
                (x.phase, x.embodied),
                (Phase::Waiting, true),
                "seen waiting"
            );
            seen + 2_000
        }
        Then::Fight => {
            assert!(next + 6_000 < r.c.parties[band].bored_at, "a next round");
            // Not routed: the band waits at its quota, so a sighting would place all of it and
            // the fight would be a walk-in's (MZB-461, MZB-492), its record an empty one
            // (MZB-700), not the arrival of an abstract band that this case replays.
            r.sight = false;
            r.watch = true;
            r.next_obs = Tick(next.0 - 600);
            run_journaled(&mut r, &mut walls, next + 600, &mut journal);
            assert_eq!(r.c.parties[band].phase, Phase::Fighting, "the round fought");
            next + 3_000
        }
    };
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let live = SiegeAt::of(&r.c, band, &walls, watched, saved_at);
    if !crash {
        return (live, decision);
    }
    // Java journals other records all along: one at the end.
    journal.push((end, fillers_casualty()));
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let watched = back.stats.raids_watched;
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        // Rule U: no recall reaches Java while replaying (§4.4), so no body vanishes in view.
        let sent = back.take_commands();
        assert!(
            !sent.iter().any(|c| matches!(c, Cmd::Recall(_))),
            "{sent:?}"
        );
    }
    assert!(back.replaying());
    (
        SiegeAt::of(&back, band, &walls, watched, saved_at),
        decision,
    )
}

/// RP-48: an unwatched siege round, then a sighting of the band waiting for its next round, the
/// pages from before the round. The replay used to read the sighting's BAND_MATERIALIZED as the
/// held round's fight: Fighting, a fight watched, and the round's raid lost.
#[test]
fn a_sighting_of_a_siege_waiting_after_a_held_round_is_no_fight() {
    let (live, round) = a_siege_held_then(true, Then::Sighting, false);
    assert_eq!(live.raids, vec![round], "{live:?}");
    assert_eq!(
        (live.phase, live.embodied, live.watched),
        (Phase::Waiting, true, 0)
    );
    a_siege_held_then(true, Then::Sighting, true)
        .0
        .assert_as(&live);
}

/// RP-48: an unwatched siege round, then the next round fought (the player seen there first),
/// the pages from before the round. The replay used to read the next round's records against the
/// first round's stale hold: the band out of its fight early, and the first round's raid lost.
#[test]
fn a_siege_round_fought_after_a_held_unwatched_round_is_fought_at_its_own_round() {
    let (live, round) = a_siege_held_then(true, Then::Fight, false);
    assert_eq!(live.raids, vec![round], "{live:?}");
    assert_eq!((live.phase, live.watched), (Phase::Fighting, 1));
    a_siege_held_then(true, Then::Fight, true)
        .0
        .assert_as(&live);
}

/// RP-48: a siege's unwatched arrival, held while replaying, then a sighting of the band waiting
/// for its next round (and, second, that round fought), the pages from before the arrival.
#[test]
fn a_sighting_of_a_siege_waiting_after_a_held_arrival_is_no_fight() {
    for then in [Then::Sighting, Then::Fight] {
        let (live, arrival) = a_siege_held_then(false, then, false);
        assert_eq!(live.raids, vec![arrival], "{then:?}: {live:?}");
        a_siege_held_then(false, then, true).0.assert_as(&live);
    }
}

/// fixes4 item 2 (RP-56): an unwatched siege decision (its arrival, then its next round), then a
/// sighting of the band waiting, its record inside the hold's slack, the pages from before the
/// decision. The record is a sighting's by its echoed flag, so it embodies the band, keeps the
/// hold and fights nothing; the decision resolves unwatched at the slack's poll. The replay used
/// to read it as the held decision's fight: Fighting, a fight watched, the decision's raid lost.
///
/// Case A's cost (decided by the lead): the arrival's poll is `arrive`'s unwatched branch, which
/// recalls the band the sighting embodied, where the session that went on kept it embodied.
/// The recall reaches no one while replaying (Rule U, checked in [`a_siege_held_then`]), the
/// crashed session's bodies are gone, and the replay end makes every band abstract.
#[test]
fn a_sighting_inside_a_held_decisions_slack_is_no_fight() {
    for round in [true, false] {
        let (live, decision) = a_siege_held_then(round, Then::SightingInSlack, false);
        assert_eq!(live.raids, vec![decision], "{live:?}");
        assert_eq!(
            (live.phase, live.embodied, live.watched),
            (Phase::Waiting, true, 0),
            "round {round}"
        );
        let back = a_siege_held_then(round, Then::SightingInSlack, true).0;
        if round {
            back.assert_as(&live);
        } else {
            assert!(!back.embodied, "recalled at the slack's poll: {back:?}");
            back.assert_as(&SiegeAt {
                embodied: false,
                ..live
            });
        }
    }
}

/// What comes to a held siege decision's band inside the hold's slack, after a sighting there,
/// in [`a_held_decision_then`]: every member killed at the sighting's epoch, its chief killed,
/// the world made Peaceful, one member killed, or its bodies despawned with two of the first
/// placed stack's members gone (deaths Java never reported).
#[derive(Clone, Copy, Debug)]
enum Inside {
    WipedOut,
    ChiefKilled,
    Peaceful,
    Wounded,
    Despawned,
}

/// fixes6 items 1 and 2 (RP-59, RP-60; the checker's case): a siege's held decision (its round with
/// `round`, else its arrival), unwatched, the band with its chief; a sighting 300 ticks after the
/// decision embodies it inside the hold's slack (RP-56, the hold kept); then `inside`, 50 ticks
/// later, still inside the slack. Returns the siege as [`SiegeAt`] reads it, the decision, and
/// the members each abstract raid since the save sent: the session that went on, or, with
/// `crash`, a replay of its journal on an engine from the pages saved before the decision.
fn a_held_decision_then(round: bool, inside: Inside, crash: bool) -> (SiegeAt, Tick, Vec<u32>) {
    let mut r = Run::new(36, patient_goblins());
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
    r.c.parties[band].chief = true;
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Siege;
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    let decision = if round {
        let mut to = r.c.now();
        while r.c.parties[band].phase != Phase::Waiting {
            to += 600;
            assert!(to < day(6.0), "the siege waits at its target");
            run_journaled(&mut r, &mut walls, to, &mut journal);
        }
        r.c.parties[band].until
    } else {
        let arrival = *r.c.parties[band].times.last().unwrap();
        run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
        arrival
    };
    let x = &r.c.parties[band];
    assert!(!x.embodied && decision < x.bored_at, "unseen, not bored");
    let saved_at = r.c.now();
    let (pages, world) = (r.c.save(), r.w.save());
    let watched = r.c.stats.raids_watched;
    journal.clear();
    run_journaled(&mut r, &mut walls, decision + 1, &mut journal);
    assert!(walls.raids.contains(&decision), "resolved unwatched");
    let seen = decision + 300;
    r.watch = true;
    r.next_obs = seen;
    run_journaled(&mut r, &mut walls, seen + 1, &mut journal);
    r.watch = false;
    run_journaled(&mut r, &mut walls, seen + 50, &mut journal);
    let x = &r.c.parties[band];
    assert!(x.embodied && x.chief, "seen, with its chief");
    let at = r.c.now();
    let c = r.c.cultures.get(0).clone();
    let wounds = |n: Option<u32>| -> Vec<Obs> {
        x.units
            .iter()
            .take(if n.is_some() { 1 } else { x.units.len() })
            .map(|&(u, all)| {
                Obs::Casualties(boundary::BandCasualties {
                    agent: Party::agent(band),
                    epoch: x.epoch,
                    by: boundary::BY_MOB,
                    body: body_id(&r.c.cultures, &c.unit(u).body),
                    count: n.unwrap_or(all),
                    killer: (0, 0),
                    op: next_op(),
                })
            })
            .collect()
    };
    let records = match inside {
        Inside::WipedOut => wounds(None),
        Inside::Wounded => wounds(Some(1)),
        Inside::ChiefKilled => vec![chief_killed_by_mob(band, x.epoch, 7_001)],
        Inside::Peaceful => vec![difficulty(boundary::DIFFICULTY_PEACEFUL)],
        Inside::Despawned => {
            let survivors = x
                .spawned
                .iter()
                .enumerate()
                .map(|(i, &(u, n))| {
                    let n = if i == 0 { n.saturating_sub(2) } else { n };
                    (body_id(&r.c.cultures, &c.unit(u).body), n)
                })
                .collect();
            vec![Obs::Dematerialized(boundary::BandDematerialized {
                agent: Party::agent(band),
                epoch: x.epoch,
                reason: boundary::RECALL_PASSING,
                survivors,
            })]
        }
    };
    for o in records {
        r.c.ingest(&mut r.w, &mut walls, &o, at);
        journal.push((at, o));
    }
    let end = seen + 2_000;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let members = |walls: &Walls| walls.members.range(saved_at + 1..).map(|e| *e.1).collect();
    let live = SiegeAt::of(&r.c, band, &walls, watched, saved_at);
    if !crash {
        return (live, decision, members(&walls));
    }
    journal.push((end, fillers_casualty()));
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let watched = back.stats.raids_watched;
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    back.run_until(&mut w, &mut walls, end + 1_000);
    (
        SiegeAt::of(&back, band, &walls, watched, saved_at),
        decision,
        members(&walls),
    )
}

/// fixes6 item 1 (RP-59): a held siege decision, seen inside the hold's slack, then its band
/// wiped out, its chief killed or the world made Peaceful, still inside the slack, the pages
/// from before the decision. The session that went on raided at the decision, then sent the
/// band home. The replay used to send it home with its hold's poll left stale: the raid lost.
#[test]
fn a_held_decision_whose_band_is_sent_home_inside_the_slack_keeps_its_raid() {
    for inside in [Inside::WipedOut, Inside::ChiefKilled, Inside::Peaceful] {
        for round in [true, false] {
            let (live, decision, sent) = a_held_decision_then(round, inside, false);
            assert_eq!(
                live.raids,
                vec![decision],
                "{inside:?} round {round}: {live:?}"
            );
            assert_eq!(
                live.phase,
                Phase::Home,
                "{inside:?} round {round}: {live:?}"
            );
            let (back, _, back_sent) = a_held_decision_then(round, inside, true);
            back.assert_as(&live);
            assert_eq!(
                back_sent, sent,
                "{inside:?} round {round}: the raid's members"
            );
        }
    }
}

/// fixes6 item 2 (RP-59): a held siege decision, seen inside the hold's slack, then one of its
/// members killed, or its bodies despawned with two members gone, still inside the slack. The
/// session that went on raided at the decision with every member; the replay resolves the
/// decision with the members it had then, not the fewer left at the slack's poll.
#[test]
fn a_held_decision_raids_with_the_members_it_had_at_its_tick() {
    for inside in [Inside::Wounded, Inside::Despawned] {
        for round in [true, false] {
            let (live, decision, sent) = a_held_decision_then(round, inside, false);
            assert_eq!(
                live.raids,
                vec![decision],
                "{inside:?} round {round}: {live:?}"
            );
            let (back, _, back_sent) = a_held_decision_then(round, inside, true);
            assert_eq!(
                (back.raids.len(), back_sent),
                (1, sent),
                "{inside:?} round {round}: the raid's members"
            );
        }
    }
}

/// What a watched raid's arrival whose first wave alone Java refused came to: the band's phase
/// and epoch, the fights watched, the deaths counted, the band's members alive, and the
/// abstract raids (by when), since the save.
type FirstWave = (Phase, u32, u64, u64, u32, Vec<Tick>);

/// A watched raid's arrival whose first wave alone Java refused (the checker's case; its later
/// waves placed), and `fight_record_gap_max`: the session that went on, the plugin killing a
/// quarter of each wave at the base 1,500 ticks after it spawns; or, with `crash`, a replay of
/// its journal on an engine from the pages saved 3,000 ticks before the arrival.
fn a_first_wave_refused(crash: bool) -> (FirstWave, u64) {
    let mut r = Run::new(36, quiet_goblins());
    r.plugin.fight = Some(synth::Fight {
        kill_share: 0.25,
        after: 1_500,
        kill_chief: false,
        only_at_base: true,
    });
    let band = raid_band(&mut r);
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    r.refuse = Some(band);
    r.refuse_wave0_only = true;
    let arrival = *r.c.parties[band].times.last().unwrap();
    let mut walls = Walls::default();
    let mut journal = Vec::new();
    run_journaled(&mut r, &mut walls, Tick(arrival.0 - 3_000), &mut journal);
    let (pages, world) = (r.c.save(), r.w.save());
    let (watched, saved_at, cas) = (r.c.stats.raids_watched, r.c.now(), r.c.stats.casualties);
    journal.clear();
    r.watch = true;
    r.next_obs = r.c.now();
    let end = arrival + r.c.tun.watched_fight_ticks + 600;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let agent = Party::agent(band);
    assert!(
        journal
            .iter()
            .any(|(_, o)| matches!(o, Obs::SpawnRejected(x) if x.agent == agent && x.wave == 0)),
        "the first wave refused"
    );
    assert!(
        journal
            .iter()
            .any(|(_, o)| matches!(o, Obs::Materialized(x) if x.agent == agent && x.wave > 0)),
        "a later wave placed"
    );
    journal.push((end, fillers_casualty()));
    let read = |c: &Conflict, walls: &Walls, watched: u64, cas: u64| {
        let x = &c.parties[band];
        let books = (
            x.phase,
            x.epoch,
            c.stats.raids_watched - watched,
            c.stats.casualties - cas,
            x.alive(),
            walls.raids.range(saved_at + 1..).copied().collect(),
        );
        (books, c.stats.fight_record_gap_max)
    };
    if !crash {
        return read(&r.c, &walls, watched, cas);
    }
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let (watched, cas) = (back.stats.raids_watched, back.stats.casualties);
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    read(&back, &walls, watched, cas)
}

/// fixes6 item 3 (RP-62): a live watched fight's first wave refused. Java's refusal is the
/// fight's first record, the one a replay's hold reads (RP-54), and the gap is measured to it;
/// it used to be measured to the next wave's BAND_MATERIALIZED, a wave interval later, past the
/// hold's slack.
#[test]
fn a_refused_first_wave_is_its_fights_first_record() {
    let (_, gap) = a_first_wave_refused(false);
    assert!(
        gap < REPLAY_HOLD_SLACK_TICKS,
        "the gap to the refusal: {gap}"
    );
    assert_eq!(gap, 1, "the plugin answers a command a tick later");
}

/// fixes6 (RP-61, item 5's replay half): a watched arrival whose first wave alone Java refused,
/// replayed on an engine from the pages saved before it. The refusal ends the hold as the fight
/// (RP-54), unembodied; the fight's later waves' BAND_MATERIALIZED embodies the band again, as
/// live `wave` did when it commanded them (RP-58), so their deaths count as live. The replay
/// used to honour no record of an unembodied band's epoch: its deaths counted against nothing.
#[test]
fn a_held_arrival_whose_first_wave_alone_is_refused_counts_its_later_waves() {
    let (live, _) = a_first_wave_refused(false);
    assert_eq!(
        (live.0, live.2, live.5.len()),
        (Phase::Home, 1, 0),
        "fought, no abstract raid: {live:?}"
    );
    assert!(live.3 > 0, "the later waves' deaths count: {live:?}");
    let (back, _) = a_first_wave_refused(true);
    assert_eq!(back, live);
}

/// fixes4 item 3 (RP-52): a siege round, nobody near, that kills the last of the band (every
/// raider held at the walls, all of them falling), the round before the band's boredom and the
/// hold's slack after it; the pages from the band waiting for it. The session that went on
/// sends the band home done, as of the round. The replay holds the round and resolves it at the
/// slack, and used to label the walk home by the slack's poll, past the boredom: bored.
#[test]
fn a_held_round_that_kills_the_band_before_its_boredom_sends_it_home_done() {
    let mut r = Run::new(36, patient_goblins());
    r.c.tun.abstract_loss_share = 1.0;
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
    let round = r.c.parties[band].until;
    r.c.parties[band].bored_at = round + REPLAY_HOLD_SLACK_TICKS / 2;
    let saved_at = r.c.now();
    let (pages, world) = (r.c.save(), r.w.save());
    let bored = r.c.stats.bored;
    let read = |c: &Conflict, walls: &Walls| {
        let x = &c.parties[band];
        let raids: Vec<Tick> = walls.raids.range(saved_at + 1..).copied().collect();
        (x.phase, x.alive(), c.stats.bored - bored, raids)
    };
    walls.rout = true;
    journal.clear();
    let end = round + 2 * REPLAY_HOLD_SLACK_TICKS;
    run_journaled(&mut r, &mut walls, end, &mut journal);
    let live = read(&r.c, &walls);
    assert_eq!(
        live,
        (Phase::Home, 0, 0, vec![round]),
        "done, as of the round"
    );
    journal.push((end, fillers_casualty()));
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(r.c.tun.clone(), r.c.cultures.clone(), &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls {
        rout: true,
        ..Walls::default()
    };
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(back.replaying());
    let (phase, alive, bored, raids) = read(&back, &walls);
    assert_eq!(
        raids,
        vec![round + REPLAY_HOLD_SLACK_TICKS],
        "resolved at the slack"
    );
    assert_eq!((phase, alive, bored), (live.0, live.1, live.2), "as live");
}

/// fixes4 item 4 (RP-53): a siege's round, nobody near, held while replaying from pages saved
/// while the band waits for it; its boredom 110 ticks after the round, and its fight's
/// BAND_MATERIALIZED 115 ticks after it, past the boredom but inside the hold's slack.
/// `restart`: pages saved 105 ticks after the round (the hold's map lost) and a restart from
/// them before the record. Returns the band's phase, whether embodied, epoch, phase end, fights
/// watched since the save, and abstract raids since the save, at the replay's last record.
fn a_round_fought_past_its_boredom_replayed(
    restart: bool,
) -> (Phase, bool, u32, Tick, u64, Vec<Tick>) {
    let mut r = Run::new(36, patient_goblins());
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
    let round = r.c.parties[band].until;
    r.c.parties[band].bored_at = round + 110;
    let (epoch, watched, saved_at) = (r.c.parties[band].epoch, r.c.stats.raids_watched, r.c.now());
    let (tun, cultures) = (r.c.tun.clone(), r.c.cultures.clone());
    let mut w = world_from(36, &r.w.save());
    w.forget_player_pos(r.p);
    let mut back = Conflict::load(tun.clone(), cultures.clone(), &r.c.save()).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    back.run_until(&mut w, &mut walls, round + 105);
    let x = &back.parties[band];
    assert_eq!((x.phase, x.embodied), (Phase::Waiting, false), "held");
    if restart {
        back = Conflict::load(tun, cultures, &back.save()).unwrap();
        w = world_from(36, &w.save());
        back.begin_replay(true);
    }
    let records = [
        (round + 115, fought(band, epoch + 1, 0, 4)),
        (round + 1_000, fillers_casualty()),
    ];
    for (at, o) in &records {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    assert!(back.replaying());
    let x = &back.parties[band];
    let raids = walls.raids.range(saved_at + 1..).copied().collect();
    (
        x.phase,
        x.embodied,
        x.epoch,
        x.until,
        back.stats.raids_watched - watched,
        raids,
    )
}

/// fixes4 item 4 (RP-53): the hold's map honours a held round's fight record past the band's
/// boredom, inside the slack; so does `held_since`'s inference after a restart that lost the
/// map. The inference used to gate on the record's tick against the boredom, not the round's,
/// so it refused the record: the round resolved unwatched at the slack.
#[test]
fn a_held_rounds_fight_past_its_boredom_is_honoured_after_a_restart() {
    let map = a_round_fought_past_its_boredom_replayed(false);
    assert_eq!((map.4, map.5.len()), (1, 0), "fought, not raided: {map:?}");
    assert_eq!(a_round_fought_past_its_boredom_replayed(true), map);
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
        sight: a.sight,
        refuse: None,
        refuse_wave0_only: false,
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
                op: next_op(),
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
                op: next_op(),
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

// ---- MZB-589 across a paged save (the M6 re-review's probe) ----

fn paged_store(tag: &str) -> std::sync::Arc<std::sync::RwLock<rtsim_kernel::page::PageStore>> {
    let d = std::env::temp_dir().join(format!("rtsim-mzb589-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::sync::Arc::new(std::sync::RwLock::new(
        rtsim_kernel::page::PageStore::open(&d, rtsim_kernel::page::Durability::NoSync).unwrap(),
    ))
}

fn paged_save(
    c: &mut Conflict,
    s: &std::sync::Arc<std::sync::RwLock<rtsim_kernel::page::PageStore>>,
) {
    let mut b = rtsim_kernel::page::SaveBatch::new();
    let t = c.save_paged(&mut b, rtsim_conflict::scenario::META_KEY);
    s.write().unwrap().commit(b, 0).unwrap();
    c.saved(t);
}

fn paged_open(
    r: &Run,
    s: &std::sync::Arc<std::sync::RwLock<rtsim_kernel::page::PageStore>>,
) -> Conflict {
    Conflict::open_paged(
        s.clone(),
        1 << 20,
        rtsim_conflict::scenario::META_KEY,
        r.c.tun.clone(),
        r.c.cultures.clone(),
    )
    .unwrap()
    .unwrap()
}

/// MZB-589: the slot is free at a paged save (the old party ended); after open_paged the next
/// party takes it under the bumped generation, and the old agent's copy does nothing.
#[test]
fn a_casualty_copy_of_an_ended_party_does_nothing_across_a_paged_save() {
    let mut r = Run::new(36, quiet_goblins());
    let s = paged_store("slot");
    paged_save(&mut r.c, &s); // the first paged save, full
    let old = raid_band(&mut r);
    let body = {
        let c = r.c.cultures.get(0).clone();
        body_id(&r.c.cultures, &c.unit(r.c.parties[old].units[0].0).body)
    };
    {
        let x = &mut r.c.parties[old];
        x.embodied = true;
        x.spawned = x.units.clone();
    }
    let op = next_op();
    let cs = Obs::Casualties(killed(old, 0, body, 1, op));
    r.ingest(cs.clone(), Tick(40));
    paged_save(&mut r.c, &s); // incremental: the party page with the op
    let mut mid = paged_open(&r, &s);
    assert_eq!(
        mid.parties[old].casualty_ops,
        vec![op],
        "the op survives an incremental paged save"
    );
    mid.ingest(&mut r.w, &mut NoInfrastructure, &cs, Tick(40));
    assert_eq!(mid.stats.casualties, 1);
    r.c.parties.remove(old);
    paged_save(&mut r.c, &s); // incremental: the slot freed, the free list grown
    let mut c = paged_open(&r, &s);
    assert!(!c.parties.contains(old));
    let new = c
        .order_party(
            &mut r.w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(r.p),
            Tick(40),
        )
        .unwrap();
    assert_eq!(
        new.index(),
        old.index(),
        "the slot is reused after the reopen"
    );
    assert_eq!(new.generation(), old.generation() + 1);
    {
        let x = &mut c.parties[new];
        x.embodied = true;
        x.spawned = x.units.clone();
    }
    let units = c.parties[new].units.clone();
    let cas = c.stats.casualties;
    c.ingest(&mut r.w, &mut NoInfrastructure, &cs, Tick(40));
    assert_eq!(
        c.parties[new].units, units,
        "the old agent's copy does nothing"
    );
    assert_eq!(c.stats.casualties, cas);
}

/// The raid of `a_raid_arriving_after_the_save`, but the player's area is routed from the save
/// on, so the band is met on its road and walks into its fight in its bodies (MZB-461, MZB-492).
/// Every member is placed by then, so the fight's one record is the walk-in's empty BAND_FIGHT
/// one (MZB-700): the held arrival of a restart's replay finds it and the fight is not resolved
/// again (MZB-491). `crash`: a restart from the pages replays the journal.
fn a_raid_met_on_its_road_after_the_save(crash: bool) -> Arrival {
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
    let (pages, world) = (r.c.save(), r.w.save());
    let (casualties, raids_watched) = (r.c.stats.casualties, r.c.stats.raids_watched);
    journal.clear();
    r.watch = true;
    r.sight = true;
    r.next_obs = r.c.now();
    let mut end = arrival;
    while r.c.parties.get(band).is_some() {
        end += 600;
        assert!(end < day(8.0), "the band comes home");
        run_journaled(&mut r, &mut walls, end, &mut journal);
    }
    // Met on its road: the band's first record after the pages is the sighting's (unflagged),
    // and the walk-in's is the fight's: flagged, with no member to place (MZB-700).
    let flags: Vec<u32> = journal
        .iter()
        .filter_map(|(_, o)| match o {
            Obs::Materialized(m) if m.agent == Party::agent(band) => Some(m.flags),
            _ => None,
        })
        .collect();
    assert_eq!(
        flags[0] & u32::from(boundary::BAND_FIGHT),
        0,
        "a sighting: {flags:?}"
    );
    let fights: Vec<usize> = r
        .cmds
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::Materialize(m)
                if m.agent == Party::agent(band) && m.flags & boundary::BAND_FIGHT != 0 =>
            {
                Some(m.members.len())
            }
            _ => None,
        })
        .collect();
    assert_eq!(fights, [0], "the walk-in's empty fight record");
    journal.push((end, fillers_casualty()));
    let arrival_of = |c: &Conflict, walls: &Walls| Arrival {
        raids: walls.raids.iter().copied().collect(),
        casualties: c.stats.casualties - casualties,
        watched: c.stats.raids_watched - raids_watched,
        pool: f64::NAN,
    };
    if !crash {
        let a = arrival_of(&r.c, &walls);
        eprintln!("live: {a:?}");
        return a;
    }
    let mut w = world_from(36, &world);
    w.forget_player_pos(r.p);
    let tun = r.c.tun.clone();
    let cultures = r.c.cultures.clone();
    let mut back = Conflict::load(tun, cultures, &pages).unwrap();
    back.begin_replay(true);
    let mut walls = Walls::default();
    for (at, o) in &journal {
        back.run_until(&mut w, &mut walls, *at);
        back.ingest(&mut w, &mut walls, o, *at);
        let _ = back.take_commands();
    }
    back.resend_notices();
    assert!(!back.replaying());
    back.run_until(&mut w, &mut walls, end + 2 * TICKS_PER_DAY);
    let a = arrival_of(&back, &walls);
    eprintln!("crash: {a:?}");
    a
}

#[test]
fn a_walked_in_fight_in_the_replayed_window_is_resolved_once() {
    let live = a_raid_met_on_its_road_after_the_save(false);
    assert!(
        live.raids.is_empty(),
        "fought in its bodies, not raided: {live:?}"
    );
    assert_eq!(live.watched, 1, "{live:?}");
    let crash = a_raid_met_on_its_road_after_the_save(true);
    assert_eq!(
        crash.raids, live.raids,
        "no abstract raid on top of the fight"
    );
    assert_eq!(crash.watched, live.watched);
    assert_eq!(crash.casualties, live.casualties);
}

/// MZB-702 (MZB-457 on envoys): an envoy refused in view of its player goes home recalled
/// (MZB-456: an envoy never walks home in bodies), and the same player, still watching, met it
/// again on its way home: `seen` no longer skipped Home, and `materialize_envoy` then sent a
/// demand of nothing to nobody (the feud was cleared by `go_home`), which Java reads as a
/// settlers' demand. An envoy on its way home is not met.
#[test]
fn a_refused_envoy_is_not_met_again_on_its_way_home() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let at = r.c.now() + 5;
    answer_from(
        &mut r,
        e.agent,
        e.epoch,
        boundary::ANSWER_REFUSED,
        synth::PLAYER,
        at,
    );
    let from = r.cmds.len();
    r.run(at + TICKS_PER_DAY / 4);
    let after: Vec<String> = r.cmds[from..]
        .iter()
        .filter_map(|(t, c)| match c {
            Cmd::Envoy(x) if x.agent == e.agent => Some(format!(
                "{}: envoy epoch {} player {:?} flags {} count {}",
                t.0, x.epoch, x.player, x.demand_flags, x.tribute_count
            )),
            Cmd::Recall(x) if x.agent == e.agent => Some(format!(
                "{}: recall epoch {} reason {}",
                t.0, x.epoch, x.reason
            )),
            _ => None,
        })
        .collect();
    eprintln!("{after:#?}");
    assert!(
        !after.iter().any(|l| l.contains(": envoy ")),
        "met again on its way home: {after:#?}"
    );
}
