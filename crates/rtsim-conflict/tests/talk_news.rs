// SPDX-License-Identifier: AGPL-3.0-or-later
//! The news the conflict records for TALK (TALK.md §9.4, TK26): Feud when a feud reaches a rung,
//! Raid when a raid or a watched fight ends, and Fell when a chief dies, each made by the camp and
//! witnessed by its chief and heir (and by the village a raid marched on), so that a camp's
//! talkers tell what it did and what it lost.

use std::collections::BTreeMap;

use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, Party, Phase, Signal, Target};
use rtsim_conflict::synth;
use rtsim_conflict::{ConflictTunables, Cultures, Step};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::report::{fell, raid};
use rtsim_knowledge::{ActorId, Report, ReportId, ReportKind};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// Goblins that never plunder on their own, so what happens is what the test does.
fn quiet_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

struct Run {
    w: World,
    c: Conflict,
    f: u32,
    p: ActorId,
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
        let f = c
            .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
            .unwrap();
        let p = c.player(&mut w, synth::PLAYER);
        Run { w, c, f, p }
    }

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

    fn run(&mut self, to: Tick) {
        self.c.run_until(&mut self.w, &mut NoInfrastructure, to);
        let _ = self.c.take_commands();
    }

    fn camp(&self) -> (ActorId, ActorId, ActorId) {
        let x = &self.c.factions[self.f as usize];
        (x.actor, x.chief, x.heir)
    }

    /// Every report of `kind` in the table, oldest first.
    fn news(&self, kind: ReportKind) -> Vec<(ReportId, Report)> {
        let mut v: Vec<(ReportId, Report)> = self
            .w
            .knowledge
            .reports()
            .iter()
            .map(|(id, _)| id)
            .filter_map(|id| {
                self.w
                    .knowledge
                    .report(id)
                    .filter(|x| x.kind == kind)
                    .map(|x| (id, x))
            })
            .collect();
        v.sort_by_key(|e| e.1.at);
        v
    }
}

/// A feud that climbs is a Feud report at each rung, the rung its severity, against the player;
/// the camp's chief and heir know it (feud.by_us), so the camp tells whom it has sworn against.
#[test]
fn each_rung_a_feud_reaches_is_feud_news_the_camp_knows() {
    let mut r = Run::new(5);
    assert!(r.news(ReportKind::Feud).is_empty(), "no feud yet");
    let mut d = 0.0;
    while r.c.feud(r.f, r.p).is_none_or(|x| x.step < Step::Envoy) {
        assert!(d < 40.0, "the feud never reached the envoy's rung");
        let at = r.c.now().max(day(d));
        r.kill(20.0, at);
        d += 1.0;
        r.run(day(d));
    }
    let (fa, chief, heir) = r.camp();
    let feuds = r.news(ReportKind::Feud);
    let rungs: Vec<u8> = feuds.iter().map(|e| e.1.severity).collect();
    assert_eq!(
        &rungs[..2],
        &[Step::Notice as u8, Step::Envoy as u8],
        "one report a rung, in order: {rungs:?}"
    );
    assert!(
        rungs.windows(2).all(|w| w[0] < w[1]),
        "each rung once, climbing: {rungs:?}"
    );
    for (id, rep) in &feuds {
        assert_eq!(
            (rep.who, rep.whom),
            (fa, r.p),
            "the camp against the player"
        );
        assert!(r.w.knowledge.knows(chief, *id), "the chief knows it");
        assert!(r.w.knowledge.knows(heir, *id), "the heir knows it");
    }
}

/// An unwatched plunder that takes something is a Raid report, PLUNDERED, against the village it
/// marched on, which witnesses it as the camp does.
#[test]
fn a_plunder_is_raid_news_the_village_and_the_camp_know() {
    let mut r = Run::new(6);
    let ashby = r.w.node_of_key(4).expect("Ashby");
    let va = r.w.nodes[ashby].actor;
    // Something over Ashby's targets to take (a raid takes only the surplus, CL-D58).
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
        r.run(t);
    }
    let (fa, chief, heir) = r.camp();
    let raids = r.news(ReportKind::Raid);
    assert_eq!(raids.len(), 1, "one raid, one report");
    let (id, rep) = raids[0];
    assert_eq!((rep.who, rep.whom), (fa, va), "the camp raided Ashby");
    assert_eq!(rep.severity, raid::PLUNDERED);
    for (h, who) in [(va, "Ashby"), (chief, "the chief"), (heir, "the heir")] {
        assert!(r.w.knowledge.knows(h, id), "{who} knows it");
    }
}

/// A raid with its war chief, in a watched fight with the player, a day after the player killed
/// 20 of the camp's members.
fn chief_in_a_fight(seed: u64) -> (Run, Handle<Party>) {
    let mut r = Run::new(seed);
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

/// The player kills the war chief: a Fell report, KILLED, names the dead chief, and the band's
/// rout is a Raid report, BEATEN, against the player. The heir who took over and the new heir
/// know both, so the camp tells its own loss (fell.by_us), which the chief's Killing cannot give
/// it (TK82).
#[test]
fn a_chief_killed_in_a_fight_is_fell_news_and_the_rout_raid_news() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = Tick::from_days(1) + 100;
    let (fa, old_chief, old_heir) = r.camp();
    r.c.chief_killed(&mut r.w, band, Some(r.p), at);
    let (_, chief, heir) = r.camp();
    assert_eq!(chief, old_heir, "the heir takes over");
    let fells = r.news(ReportKind::Fell);
    assert_eq!(fells.len(), 1, "one chief, one report");
    let (id, rep) = fells[0];
    assert_eq!((rep.who, rep.whom), (fa, old_chief));
    assert_eq!(rep.severity, fell::KILLED);
    assert!(r.w.knowledge.knows(chief, id), "the new chief knows it");
    assert!(r.w.knowledge.knows(heir, id), "the new heir knows it");
    let raids = r.news(ReportKind::Raid);
    assert_eq!(raids.len(), 1, "the rout is one raid report");
    let (id, rep) = raids[0];
    assert_eq!((rep.who, rep.whom), (fa, r.p), "against the feud's player");
    assert_eq!(rep.severity, raid::BEATEN);
    assert!(r.w.knowledge.knows(chief, id));
}

/// A chief dead of age is Fell news, DIED, when the conflict next wakes and the heir succeeds
/// (succession is lazy).
#[test]
fn a_chief_dead_of_age_is_fell_news_at_the_succession() {
    let mut r = Run::new(7);
    r.run(day(1.0));
    let (fa, old_chief, _) = r.camp();
    r.w.knowledge.die(old_chief, day(1.0));
    assert!(
        r.news(ReportKind::Fell).is_empty(),
        "nothing until the succession"
    );
    let mut d = 1.0;
    while r.c.factions[r.f as usize].chief == old_chief {
        assert!(d < 20.0, "no succession");
        let at = r.c.now().max(day(d));
        r.kill(20.0, at);
        d += 1.0;
        r.run(day(d));
    }
    let (_, chief, heir) = r.camp();
    let fells = r.news(ReportKind::Fell);
    assert_eq!(fells.len(), 1);
    let (id, rep) = fells[0];
    assert_eq!((rep.who, rep.whom), (fa, old_chief));
    assert_eq!(rep.severity, fell::DIED);
    assert!(r.w.knowledge.knows(chief, id) && r.w.knowledge.knows(heir, id));
}

/// A chief killed by a mob (no player among the killers) is still KILLED, not dead of age.
#[test]
fn a_chief_killed_by_no_player_is_fell_news_killed() {
    let (mut r, band) = chief_in_a_fight(9);
    let at = Tick::from_days(1) + 100;
    let (fa, old_chief, _) = r.camp();
    r.c.chief_killed(&mut r.w, band, None, at);
    let fells = r.news(ReportKind::Fell);
    assert_eq!(fells.len(), 1);
    assert_eq!((fells[0].1.who, fells[0].1.whom), (fa, old_chief));
    assert_eq!(fells[0].1.severity, fell::KILLED);
}

/// A raid on a place with no district near (nothing the sim knows to raid) took nothing and lost
/// nobody: no Raid report, though the band did reach its target.
#[test]
fn a_raid_that_finds_nothing_is_no_news() {
    let mut r = Run::new(9);
    r.c.log_enabled = true;
    r.kill(20.0, Tick(10));
    r.c.order_party(
        &mut r.w,
        synth::CAMP,
        PartyKind::Raid,
        Target::Pos(synth::FOREST_POS),
        Some(r.p),
        Tick(20),
    )
    .expect("a war band");
    r.c.feud_mut(r.f, r.p).unwrap().step = Step::Raid;
    let mut d = 0.0;
    while !r.c.log.iter().any(|e| e.what == "finds nothing to raid") {
        assert!(d < 10.0, "the band never reached its target");
        d += 0.5;
        r.run(day(d));
    }
    assert!(
        r.news(ReportKind::Raid).is_empty(),
        "a raid on nothing is no news"
    );
}
