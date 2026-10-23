// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine halves of MOWZIE.md's slice M1 (§9): the world's difficulty (no war party leaves on
//! Peaceful, §4.12), a feud's notice kept until the plugin plays it (NOTICE_SHOWN, resent at each
//! session start, across a restart), and an envoy party wiped out with no player among its
//! killers withdrawing its demand (DEMAND_SETTLED outcome 5, ledger 14).

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, Signal, Target};
use rtsim_conflict::synth::{self, FakeConflict};
use rtsim_conflict::{ConflictTunables, Cultures, Step};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::ActorId;
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
/// every session start, a restart included; NOTICE_SHOWN for another step changes nothing, for
/// its own step it ends the resending.
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
    r.flush(r.c.now());
    assert_eq!(r.notices_since(mark), vec![n], "again after the restart");
    // Shown for another step: kept.
    let at = r.c.now() + 5;
    let shown = |step| {
        Obs::NoticeShown(boundary::NoticeShown {
            faction: synth::CAMP,
            step,
            player: synth::PLAYER,
        })
    };
    r.ingest(shown(Step::Envoy as u32), at);
    assert_eq!(r.c.kept_notices(), vec![n]);
    // A stranger's NOTICE_SHOWN, or one for a camp that is not there: nothing.
    r.ingest(
        Obs::NoticeShown(boundary::NoticeShown {
            faction: synth::CAMP,
            step: n.step,
            player: (7, 7),
        }),
        at,
    );
    r.ingest(
        Obs::NoticeShown(boundary::NoticeShown {
            faction: 0xDEAD,
            step: n.step,
            player: synth::PLAYER,
        }),
        at,
    );
    assert_eq!(r.c.kept_notices(), vec![n]);
    // Shown: never again.
    r.ingest(shown(n.step), at);
    assert!(r.c.kept_notices().is_empty());
    assert_eq!(r.c.stats.notices_shown, 1);
    let mark = r.cmds.len();
    r.c.session_start();
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
        t = t + 600;
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
    // Past the old deadline: nothing came due, nothing climbed, no second demand.
    r.watch = false;
    r.run(deadline + TICKS_PER_DAY);
    assert_eq!(r.c.stats.expired, 0, "no refusal");
    assert_eq!(r.c.stats.demands, 1);
    assert!(r.step() <= Step::Envoy, "no climb: {:?}", r.step());
    assert_eq!(r.c.stats.escalations[Step::Scouts.index()], 0);
    assert_eq!(settled(&r), vec![boundary::SETTLED_WITHDRAWN]);
}

/// The same envoy struck down by the player: an insult, as before. The demand stays open, the
/// killings add to the grievance, and the demand runs out unmet at its deadline.
#[test]
fn an_envoy_a_player_kills_leaves_the_demand_to_run_out() {
    let mut r = Run::new(36, quiet_goblins());
    let e = envoy_met(&mut r);
    let g0 = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    let deadline = r.c.feud(r.f, r.p).unwrap().demand.unwrap().deadline;
    wipe(&mut r, &e, boundary::BY_PLAYER);
    assert_eq!(r.c.stats.withdrawn, 0);
    assert!(r.c.feud(r.f, r.p).unwrap().demand.is_some(), "still open");
    assert!(settled(&r).is_empty());
    let g1 = r.c.feud_view(synth::CAMP, r.p).unwrap().grievance;
    assert!(g1 > g0 + 10.0, "the killings count: {g0} to {g1}");
    r.watch = false;
    r.run(deadline + TICKS_PER_DAY);
    assert_eq!(r.c.stats.expired, 1, "unmet at its deadline");
    assert_eq!(settled(&r), vec![boundary::SETTLED_EXPIRED]);
}
