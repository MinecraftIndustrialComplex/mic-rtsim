// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cultures as data (MOWZIE.md slice M2): rungs a culture has no party for are skipped both ways
//! (ledger 2), a war rung repeats (ledger 3), the threat source and class come from the culture
//! and the chief's crystal (ledger 9), loot that is eaten leaves the books (ledger 10), tribute is
//! priced by value (ledger 12), a camp with no haven joins as a lair (ledger 13), and the Frostmaw
//! and the Umvuthana are data (ledger 15).

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Obs};
use rtsim_conflict::culture::{
    CultureDef, FROSTMAW_JSON, GOBLIN_JSON, PartyKind, SignalKind, UMVUTHANA_JSON,
};
use rtsim_conflict::engine::{Conflict, Demand, Party, Signal, Target, item_of};
use rtsim_conflict::{ConflictTunables, Cultures, Step, synth};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_industry::fake::Visit;
use rtsim_industry::threat::{ThreatClass, ThreatSource};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::{ActorId, Temperament, Trait};
use rtsim_settle::{Culture as MarketCulture, NodeKind};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

const HOUR: u64 = TICKS_PER_DAY / 24;

fn setup_with(seed: u64, cultures: Cultures, culture: &str) -> (World, Conflict, u32) {
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(seed, ConflictTunables::default(), cultures, Tick::ZERO);
    synth::define_names(&mut c, &w);
    c.log_enabled = true;
    let f = c
        .add_faction(&mut w, synth::CAMP, culture, synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    (w, c, f)
}

/// One culture from `json`, edited.
fn edited(json: &str, edit: impl FnOnce(&mut CultureDef)) -> Cultures {
    let mut d: CultureDef = serde_json::from_str(json).unwrap();
    edit(&mut d);
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// Goblins that never plunder, with only the named parties.
fn goblins_with(keep: &[&str], edit: impl FnOnce(&mut CultureDef)) -> Cultures {
    edited(GOBLIN_JSON, |d| {
        d.plunder_mean_days = None;
        d.parties.retain(|k, _| keep.contains(&k.as_str()));
        edit(d);
    })
}

/// The threshold of rung `s` for faction `f`'s chief, as the engine bends it.
fn theta(c: &Conflict, f: u32, s: Step) -> f64 {
    let x = &c.factions[f as usize];
    let bend = Temperament::from_seed(x.chief_seed)
        .bend(Trait::Aggressive, 1000, 40)
        .max(1);
    c.cultures.get(x.culture).threshold(s) * 1000.0 / f64::from(bend)
}

fn kill(c: &mut Conflict, w: &mut World, a: ActorId, amount: f64, at: Tick) {
    c.signal(
        w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount,
            actor: a,
            pos: synth::FOREST_POS,
            district: None,
        },
        at,
    );
}

/// Kills enough each day from `from` to `to` that the grievance settles near `peak` (half-life 6
/// days), running the engine hour by hour; returns each hour's rung.
fn feed(
    c: &mut Conflict,
    w: &mut World,
    f: u32,
    a: ActorId,
    peak: f64,
    from: Tick,
    to: Tick,
) -> Vec<(Tick, Step)> {
    let amount = peak * (1.0 - 0.5f64.powf(1.0 / 6.0)) / 8.0;
    let mut seen = Vec::new();
    let mut t = from;
    while t < to {
        if (t.0 - from.0) % TICKS_PER_DAY == 0 && amount > 0.0 {
            kill(c, w, a, amount, t);
        }
        t = Tick(t.0 + HOUR);
        c.run_until(w, &mut NoInfrastructure, t);
        seen.push((t, c.feud(f, a).map_or(Step::None, |x| x.step)));
    }
    seen
}

/// The distinct rungs in order.
fn rungs(seen: &[(Tick, Step)]) -> Vec<Step> {
    let mut v: Vec<Step> = Vec::new();
    for &(_, s) in seen {
        if v.last() != Some(&s) {
            v.push(s);
        }
    }
    v
}

/// `sim mowzie` run 1's M2 part: a Frostmaw den, angered by kills in its range, climbs from
/// Notice straight to Raid (it has no Envoy or Scouts), comes again every 4 days after each
/// return while the grievance holds, and calms back to Notice (not to Scouts) when it fades.
#[test]
fn a_frostmaw_climbs_to_raid_repeats_and_calms_to_notice() {
    let (mut w, mut c, f) = setup_with(11, Cultures::builtin(), "frostmaw");
    let node = c.factions[f as usize].node;
    assert_eq!(w.nodes[node].kind, NodeKind::Lair, "a den is a lair");
    let p = c.player(&mut w, synth::PLAYER);
    let raid = theta(&c, f, Step::Raid);
    // Between Raid and Invasion (twice Raid's threshold): it raids and repeats, never invades.
    let mut seen = feed(&mut c, &mut w, f, p,
        1.5 * raid,
        Tick(10),
        Tick::from_days(60),
    );
    seen.extend(feed(&mut c, &mut w, f, p,
        0.0,
        Tick::from_days(60),
        Tick::from_days(160),
    ));
    let r = rungs(&seen);
    assert_eq!(
        &r[..4],
        &[Step::Notice, Step::Raid, Step::Notice, Step::None],
        "{r:?}"
    );
    let s = &c.stats;
    assert_eq!(s.escalations[Step::Envoy.index()], 0);
    assert_eq!(s.escalations[Step::Scouts.index()], 0);
    assert_eq!(s.escalations[Step::Raid.index()], 1);
    assert_eq!(s.escalations[Step::Invasion.index()], 0);
    assert!(s.repeats >= 3, "repeats every 4 days after a return: {s:?}");
    assert_eq!(s.parties[PartyKind::Raid.index()], 1 + s.repeats);
    assert_eq!(s.demands, 0, "a beast makes no demands");
    assert!(c.log.iter().any(|e| e.kind == "repeat"));
    assert!(
        c.log
            .iter()
            .filter(|e| e.kind == "calm")
            .all(|e| !e.what.contains("scouts") && !e.what.contains("envoy")),
        "calms skip the empty rungs"
    );
    // Each repeat came 4 days after the band turned for home (`since`, MZB-51), the den's own
    // walk back being part of them.
    let homes: Vec<f64> = c
        .log
        .iter()
        .filter(|e| e.kind == "bored")
        .map(|e| e.day)
        .collect();
    let repeats: Vec<f64> = c
        .log
        .iter()
        .filter(|e| e.kind == "repeat")
        .map(|e| e.day)
        .collect();
    for (h, r) in homes.iter().zip(&repeats) {
        assert!((4.0 - 1e-6..4.0 + 1e-3).contains(&(r - h)), "home {h}, again {r}");
    }
    c.check(&w).expect("books");
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
    assert_eq!(back.state_hash(), c.state_hash());
}

/// `sim mowzie` run 5's M2 part: the grove's ladder goes Notice, Envoy (a demand in gold
/// nuggets), Scouts, Raid, Invasion, and never Siege (it has no siege party).
#[test]
fn an_umvuthana_grove_climbs_to_invasion_and_never_besieges() {
    let (mut w, mut c, f) = setup_with(12, Cultures::builtin(), "umvuthana");
    let node = c.factions[f as usize].node;
    assert_eq!(w.nodes[node].kind, NodeKind::Haven(MarketCulture::Human));
    let p = c.player(&mut w, synth::PLAYER);
    let inv = theta(&c, f, Step::Invasion);
    let seen = feed(&mut c, &mut w, f, p,
        3.0 * inv,
        Tick(10),
        Tick::from_days(80),
    );
    let r = rungs(&seen);
    assert_eq!(
        r,
        vec![
            Step::Notice,
            Step::Envoy,
            Step::Scouts,
            Step::Raid,
            Step::Invasion
        ],
        "never Siege"
    );
    let s = &c.stats;
    assert_eq!(s.escalations[Step::Siege.index()], 0);
    assert_eq!(s.parties[PartyKind::Siege.index()], 0);
    assert!(s.escalations[Step::Invasion.index()] >= 1);
    assert_eq!(s.repeats, 0, "the grove does not repeat");
    assert!(s.feud_wakes < 2_000, "no wake loop at the top: {s:?}");
    c.check(&w).expect("books");
}

/// Tribute priced by value with no market price for the good (ledger 12): 9 gold nuggets an
/// emerald's worth, the culture's fallback, on the demand's grievance at the climb.
#[test]
fn tribute_with_no_market_price_uses_the_fallback() {
    let (mut w, mut c, f) = setup_with(13, Cultures::builtin(), "umvuthana");
    let nugget = w.goods.find("gold_nugget").expect("the nugget good");
    let node = c.factions[f as usize].node;
    assert_eq!(w.market(node).price(nugget), 0, "no market prices gold yet");
    let p = c.player(&mut w, synth::PLAYER);
    // One kill puts it past Envoy; it climbs at Notice's dwell, with no signal since.
    let envoy = theta(&c, f, Step::Envoy);
    kill(&mut c, &mut w, p, envoy * 1.2 / 8.0, Tick(10));
    let mut t = Tick(10);
    let d = loop {
        t = Tick(t.0 + HOUR);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        if let Some(d) = c.feud(f, p).and_then(|x| x.demand) {
            break d;
        }
        assert!(t < Tick::from_days(5), "a demand by now");
    };
    assert_eq!(d.good, nugget);
    let x = c.feud(f, p).unwrap();
    assert_eq!(x.g.anchor, d.made);
    let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
    let g = x.g.total_at(d.made, lambda);
    let bend = Temperament::from_seed(c.factions[f as usize].chief_seed)
        .bend(Trait::Aggressive, 1000, 50);
    let want = (0.5 * g * f64::from(bend) / 1000.0 * 9.0).ceil() as i64;
    assert_eq!(d.tribute, want, "grievance {g}, bend {bend}");
    // About 9 times what goblins would ask in emeralds.
    let emeralds = (0.5 * g * f64::from(bend) / 1000.0).ceil() as i64;
    assert!(d.tribute <= 9 * emeralds && d.tribute > 9 * (emeralds - 1));
}

/// An empty top rung: goblins with no siege party top out at Invasion, and the ladder books no
/// climb from there (no wake loop).
#[test]
fn an_empty_top_rung_ends_the_climb() {
    let cultures = goblins_with(&["envoy", "scouts", "raid", "invasion"], |_| {});
    let (mut w, mut c, f) = setup_with(14, cultures, "goblin");
    let p = c.player(&mut w, synth::PLAYER);
    let siege = theta(&c, f, Step::Siege);
    let seen = feed(&mut c, &mut w, f, p,
        2.0 * siege,
        Tick(10),
        Tick::from_days(80),
    );
    let r = rungs(&seen);
    assert_eq!(r.last(), Some(&Step::Invasion), "{r:?}");
    assert_eq!(c.stats.escalations[Step::Siege.index()], 0);
    assert!(c.stats.feud_wakes < 2_000, "{:?}", c.stats);
    c.check(&w).expect("books");
}

/// All war rungs empty but Raid: the feud sits on Raid. Without `repeat_days` one band goes and
/// none follows (today's behaviour); with it, the band comes again.
#[test]
fn only_raid_among_the_war_rungs() {
    for repeat in [None, Some(3.0)] {
        let cultures = goblins_with(&["envoy", "scouts", "raid"], |d| d.repeat_days = repeat);
        let (mut w, mut c, f) = setup_with(15, cultures, "goblin");
        let p = c.player(&mut w, synth::PLAYER);
        let inv = theta(&c, f, Step::Invasion);
        let seen = feed(&mut c, &mut w, f, p, 2.0 * inv, Tick(10), Tick::from_days(60));
        let r = rungs(&seen);
        assert_eq!(
            r,
            vec![Step::Notice, Step::Envoy, Step::Scouts, Step::Raid],
            "{repeat:?}"
        );
        assert_eq!(c.stats.escalations[Step::Invasion.index()], 0);
        match repeat {
            None => {
                assert_eq!(c.stats.repeats, 0);
                assert_eq!(c.stats.parties[PartyKind::Raid.index()], 1);
            }
            Some(_) => {
                assert!(c.stats.repeats >= 2, "{:?}", c.stats);
                assert_eq!(
                    c.stats.parties[PartyKind::Raid.index()],
                    1 + c.stats.repeats
                );
            }
        }
        assert!(c.stats.feud_wakes < 2_000, "{:?}", c.stats);
        c.check(&w).expect("books");
    }
}

/// A Frostmaw that can only raid, on Raid with its band home.
fn frostmaw_home_on_raid(seed: u64) -> (World, Conflict, u32, ActorId, Tick) {
    let cultures = edited(FROSTMAW_JSON, |d| {
        d.parties.insert("invasion".into(), Vec::new());
    });
    let (mut w, mut c, f) = setup_with(seed, cultures, "frostmaw");
    let p = c.player(&mut w, synth::PLAYER);
    let raid = theta(&c, f, Step::Raid);
    let mut t = Tick(10);
    loop {
        let to = Tick(t.0 + TICKS_PER_DAY);
        feed(&mut c, &mut w, f, p, 2.0 * raid, t, to);
        t = to;
        let x = c.feud(f, p).unwrap();
        if x.step == Step::Raid && x.party.is_none() {
            return (w, c, f, p, t);
        }
        assert!(t < Tick::from_days(60), "on Raid and home by now");
    }
}

/// A repeat waits out the cowed time (ledger 3: max of the repeat, the crossing, cowed and the
/// pool's wait).
#[test]
fn a_repeat_waits_while_cowed() {
    let (mut w, mut c, f, p, t) = frostmaw_home_on_raid(16);
    let since = c.feud(f, p).unwrap().since;
    let cowed = Tick(since.0 + 9 * TICKS_PER_DAY);
    c.factions[f as usize].cowed_until = cowed;
    let raid = theta(&c, f, Step::Raid);
    let before = c.stats.repeats;
    feed(&mut c, &mut w, f, p, 2.0 * raid, t, Tick(cowed.0 - HOUR));
    assert_eq!(c.stats.repeats, before, "none while cowed");
    feed(&mut c, &mut w, f, p,
        2.0 * raid,
        Tick(cowed.0 - HOUR),
        Tick(cowed.0 + 2 * HOUR),
    );
    assert_eq!(c.stats.repeats, before + 1, "once the cowed time is over");
    c.check(&w).expect("books");
}

/// A feud with a demand open books its deadline only: no repeat while it stands.
#[test]
fn no_repeat_with_a_demand_pending() {
    let (mut w, mut c, f, p, t) = frostmaw_home_on_raid(17);
    let deadline = Tick(t.0 + 20 * TICKS_PER_DAY);
    c.feud_mut(f, p).unwrap().demand = Some(Demand {
        kind: SignalKind::Killing,
        good: GoodId(0),
        tribute: 1,
        paid: 0,
        made: t,
        deadline,
        after: 0.0,
        envoy: None,
    });
    let raid = theta(&c, f, Step::Raid);
    let before = (c.stats.repeats, c.stats.parties);
    feed(&mut c, &mut w, f, p, 2.0 * raid, t, Tick(deadline.0 - HOUR));
    assert_eq!((c.stats.repeats, c.stats.parties), before);
}

/// A step down from Siege with Invasion and Raid empty crosses both: the chief's fall (two steps)
/// lands on Envoy, a calm on Scouts.
#[test]
fn a_step_down_from_siege_crosses_two_empty_rungs() {
    let cultures = goblins_with(&["envoy", "scouts", "siege"], |_| {});
    let (mut w, mut c, f) = setup_with(18, cultures.clone(), "goblin");
    let p = c.player(&mut w, synth::PLAYER);
    kill(&mut c, &mut w, p, 50.0, Tick(10));
    let band = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Siege,
            Target::Pos(synth::FOREST_POS),
            Some(p),
            Tick(20),
        )
        .expect("a siege");
    c.feud_mut(f, p).unwrap().step = Step::Siege;
    assert!(c.parties[band].chief);
    c.run_until(&mut w, &mut NoInfrastructure, Tick(100));
    c.chief_killed(&mut w, band, Some(p), Tick(100));
    assert_eq!(c.feud(f, p).unwrap().step, Step::Envoy, "Siege, Scouts, Envoy");

    // A calm from Siege lands on Scouts.
    let (mut w, mut c, f) = setup_with(18, cultures, "goblin");
    let p = c.player(&mut w, synth::PLAYER);
    kill(&mut c, &mut w, p, 0.5, Tick(10));
    c.feud_mut(f, p).unwrap().step = Step::Siege;
    kill(&mut c, &mut w, p, 0.01, Tick(20));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(21));
    let calm = c.log.iter().find(|e| e.kind == "calm").expect("a calm");
    assert!(calm.what.ends_with("back to scouts"), "{}", calm.what);
}

/// Loot a culture destroys (ledger 10): a village's stock leaves the world as destroyed, goods
/// a band takes from players are booked and never carried, and the books still balance.
#[test]
fn destroyed_loot_leaves_the_books() {
    let cultures = goblins_with(&["raid", "plunder"], |d| d.loot_destroyed = true);
    let (mut w, mut c, _) = setup_with(19, cultures, "goblin");
    let ashby = w.node_of_key(4).expect("Ashby");
    let p = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Plunder,
            Target::Node(ashby),
            None,
            Tick(10),
        )
        .expect("plunderers");
    // A watched band carries items off from a player's containers.
    let iron = w.goods.find("iron").or_else(|| w.goods.ids().next()).unwrap();
    let looted = Obs::Looted(boundary::BandLooted {
        agent: Party::agent(p),
        epoch: 0,
        op: 77,
        items: vec![boundary::Item {
            item: item_of(iron),
            count: 5,
        }],
    });
    c.ingest(&mut w, &mut NoInfrastructure, &looted, Tick(20));
    assert_eq!(c.parties[p].from_players.get(&iron), None, "nothing carried");
    let mut t = Tick(20);
    while !c.parties.is_empty() {
        t = c.next_due().unwrap_or(Tick(t.0 + TICKS_PER_DAY));
        c.run_until(&mut w, &mut NoInfrastructure, t);
        w.check_conserved(t).expect("world conserved");
        c.check(&w).expect("books");
        assert!(c.held().is_empty(), "eaten, never held");
    }
    let l = &c.ledger;
    assert!(!l.from_villages.is_empty(), "the plunderers took something");
    assert_eq!(l.from_villages, l.destroyed);
    assert!(l.landed.is_empty());
    assert_eq!(l.from_players.get(&iron), Some(&i128::from(5 * MILLI)));
    assert_eq!(l.from_players, l.destroyed_from_players);
    assert!(l.landed_from_players.is_empty());
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
    assert_eq!(back.ledger, c.ledger);
}

/// The industry engine hears a Frostmaw raid as a boss (ledger 9), ranged with its crystal and
/// melee without it.
#[test]
fn a_frostmaw_raid_is_a_boss_and_its_class_follows_its_crystal() {
    for crystal in [true, false] {
        let (mut w, mut c, f) = setup_with(20, Cultures::builtin(), "frostmaw");
        c.factions[f as usize].crystal = crystal;
        let mut fake = synth::refinery(
            20,
            vec![Visit {
                from: Tick::ZERO,
                at: Vec::new(),
            }],
        );
        fake.run_until(Tick(5));
        c.order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            None,
            Tick(10),
        )
        .expect("the Frostmaw");
        let mut index = DistrictIndex::default();
        let to = Tick::from_days(6);
        let mut t = c.now();
        while t < to {
            let next = c.next_due().unwrap_or(to).clamp(Tick(t.0 + 1), to);
            fake.run_until(next);
            let mut link = IndustryLink {
                engine: &mut fake.engine,
                index: &mut index,
            };
            c.run_until(&mut w, &mut link, next);
            let _ = c.take_commands();
            t = next;
        }
        let attacks = &fake.engine.district(synth::REFINERY).unwrap().attacks;
        assert!(!attacks.is_empty(), "the raid reached the refinery");
        let class = if crystal {
            ThreatClass::GroundRanged
        } else {
            ThreatClass::GroundMelee
        };
        for a in attacks {
            assert_eq!(a.source, ThreatSource::Boss);
            assert_eq!(a.class, class, "crystal {crystal}");
        }
        c.check(&w).expect("books");
    }
}

/// A den joins the world as a lair: no storehouse, no market culture, never plundered; the
/// faction's crystal bit and the lair survive a save.
#[test]
fn a_lair_and_the_crystal_survive_a_save() {
    let (mut w, mut c, f) = setup_with(21, Cultures::builtin(), "frostmaw");
    let node = c.factions[f as usize].node;
    assert_eq!(w.nodes[node].kind, NodeKind::Lair);
    assert!(!w.market(node).is_storehouse());
    assert_eq!(w.nodes[node].kind.culture(), None);
    c.factions[f as usize].crystal = false;
    // A goblin camp that hates everyone near never picks the lair to plunder.
    let goblin = c
        .add_faction(&mut w, 0xC0B1_0002, "goblin", synth::FOREST_POS, Tick(5))
        .unwrap();
    let _ = goblin;
    c.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(60));
    for (_, x) in c.parties.iter() {
        assert_ne!(x.target, Target::Node(node), "nobody plunders a lair");
    }
    c.check(&w).expect("books");
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
    assert_eq!(back.state_hash(), c.state_hash());
    assert!(!back.factions[f as usize].crystal);
    assert_eq!(back.factions[f as usize].node, node);
}

/// Both Mowzie cultures load from their JSON alone, as a plugin's data would.
#[test]
fn both_mowzie_cultures_resolve_from_json() {
    let c = Cultures::from_json(&[FROSTMAW_JSON, UMVUTHANA_JSON]).unwrap();
    assert_eq!(c.list.len(), 2);
    let names: BTreeMap<&str, u16> = c
        .list
        .iter()
        .enumerate()
        .map(|(i, x)| (x.name(), i as u16))
        .collect();
    assert!(names.contains_key("frostmaw") && names.contains_key("umvuthana"));
}
