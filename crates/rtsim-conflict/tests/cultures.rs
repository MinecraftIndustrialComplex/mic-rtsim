// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cultures as data (MOWZIE.md slice M2): rungs a culture has no party for are skipped both ways
//! (ledger 2), a war rung repeats (ledger 3), the threat source and class come from the culture
//! and the chief's crystal (ledger 9), loot that is eaten leaves the books (ledger 10), tribute is
//! priced by value (ledger 12), a camp with no haven joins as a lair (ledger 13), and the Frostmaw
//! and the Umvuthana are data (ledger 15).

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{
    CultureDef, FROSTMAW_JSON, GOBLIN_JSON, PartyKind, SignalKind, UMVUTHANA_JSON,
};
use rtsim_conflict::engine::{
    Conflict, Demand, Party, Phase, Signal, TRIBUTE_PRICE_CAP, Target, item_of,
};
use rtsim_conflict::{ConflictTunables, Cultures, Step, synth};
use rtsim_ecology::infra::{
    DistrictIndex, DistrictSite, IndustryLink, Infrastructure, NoInfrastructure, RaidOutcome,
    TrainSighting,
};
use rtsim_industry::fake::Visit;
use rtsim_industry::ids::{Good, GoodVec, TrainId};
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::{ActorId, Temperament, Trait};
use rtsim_settle::{Culture as MarketCulture, NodeKind};
use rtsim_trade::{GoodId, MILLI, PRICE_ONE, Price};
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

/// The feud at the end of an hour: its rung, whether a party is out for it, since when, and how
/// many of the faction's bands are about.
#[derive(Clone, Copy, Debug)]
struct Sample {
    step: Step,
    out: bool,
    since: Tick,
    bands: usize,
}

/// Kills enough each day from `from` to `to` that the grievance settles near `peak` (half-life 6
/// days), running the engine hour by hour; returns each hour's feud.
fn feed(
    c: &mut Conflict,
    w: &mut World,
    f: u32,
    a: ActorId,
    peak: f64,
    from: Tick,
    to: Tick,
) -> Vec<Sample> {
    let amount = peak * (1.0 - 0.5f64.powf(1.0 / 6.0)) / 8.0;
    let mut seen = Vec::new();
    let mut t = from;
    while t < to {
        if (t.0 - from.0).is_multiple_of(TICKS_PER_DAY) && amount > 0.0 {
            kill(c, w, a, amount, t);
        }
        t = Tick(t.0 + HOUR);
        c.run_until(w, &mut NoInfrastructure, t);
        let bands = c.parties.iter().filter(|(_, x)| x.faction == f).count();
        seen.push(c.feud(f, a).map_or(
            Sample {
                step: Step::None,
                out: false,
                since: t,
                bands,
            },
            |x| Sample {
                step: x.step,
                out: x.party.is_some(),
                since: x.since,
                bands,
            },
        ));
    }
    seen
}

/// The distinct rungs in order.
fn rungs(seen: &[Sample]) -> Vec<Step> {
    let mut v: Vec<Step> = Vec::new();
    for x in seen {
        if v.last() != Some(&x.step) {
            v.push(x.step);
        }
    }
    v
}

/// The days the feud's parties turned for home (`since` once the feud has no party out).
fn turns(seen: &[Sample]) -> Vec<f64> {
    seen.windows(2)
        .filter(|w| w[0].out && !w[1].out)
        .map(|w| w[1].since.0 as f64 / TICKS_PER_DAY as f64)
        .collect()
}

/// Each repeat's day and the days since the last band came home before it (MZB-51).
fn repeat_gaps(c: &Conflict) -> Vec<(f64, f64, String)> {
    let mut home = None;
    let mut out = Vec::new();
    for e in &c.log {
        match e.kind {
            "home" => home = Some(e.day),
            "repeat" => out.push((e.day, e.day - home.unwrap_or(f64::NAN), e.what.clone())),
            _ => {}
        }
    }
    out
}

/// `sim mowzie` run 1's M2 part (MOWZIE.md §7 run 1: notice, raid, repeat, invasion): a Frostmaw
/// den, angered by kills in its range, climbs from Notice straight to Raid (it has no Envoy or
/// Scouts), comes again 4 days after each band is home, climbs to Invasion once the kills grow
/// past its threshold after Raid's dwell, comes again at the top of its ladder, and calms back to
/// Raid and to Notice (not to Scouts) when it fades.
#[test]
fn a_frostmaw_raids_invades_repeats_and_calms() {
    let (mut w, mut c, f) = setup_with(11, Cultures::builtin(), "frostmaw");
    let node = c.factions[f as usize].node;
    assert_eq!(w.nodes[node].kind, NodeKind::Lair, "a den is a lair");
    let p = c.player(&mut w, synth::PLAYER);
    let (raid, inv) = (theta(&c, f, Step::Raid), theta(&c, f, Step::Invasion));
    let dwell = c
        .cultures
        .get(c.factions[f as usize].culture)
        .def
        .dwell_days[Step::Raid.index()];
    // Between Raid and Invasion until Raid's dwell is over.
    let mut seen = Vec::new();
    let mut t = Tick(10);
    let mut on_raid = None;
    while on_raid
        .is_none_or(|d: Tick| t < Tick(d.0 + ((dwell + 1.0) * TICKS_PER_DAY as f64) as u64))
    {
        let to = Tick(t.0 + TICKS_PER_DAY);
        seen.extend(feed(&mut c, &mut w, f, p, 1.5 * raid, t, to));
        t = to;
        if on_raid.is_none() && seen.iter().any(|x| x.step == Step::Raid) {
            on_raid = Some(t);
        }
        assert!(t < Tick::from_days(60), "on Raid by now");
    }
    assert!(seen.iter().all(|x| x.step <= Step::Raid));
    let calm_from = Tick(t.0 + 40 * TICKS_PER_DAY);
    seen.extend(feed(&mut c, &mut w, f, p, 1.5 * inv, t, calm_from));
    seen.extend(feed(
        &mut c,
        &mut w,
        f,
        p,
        0.0,
        calm_from,
        Tick(calm_from.0 + 140 * TICKS_PER_DAY),
    ));
    let r = rungs(&seen);
    assert_eq!(
        r,
        vec![
            Step::Notice,
            Step::Raid,
            Step::Invasion,
            Step::Raid,
            Step::Notice,
            Step::None
        ],
    );
    let s = &c.stats;
    assert_eq!(s.escalations[Step::Envoy.index()], 0);
    assert_eq!(s.escalations[Step::Scouts.index()], 0);
    assert_eq!(s.escalations[Step::Raid.index()], 1);
    assert_eq!(s.escalations[Step::Invasion.index()], 1);
    assert_eq!(
        c.cultures
            .get(c.factions[f as usize].culture)
            .next_up(Step::Invasion),
        None
    );
    let gaps = repeat_gaps(&c);
    let invasions = gaps.iter().filter(|g| g.2.contains("invasion")).count();
    assert!(invasions >= 2, "the top rung comes again: {gaps:?}");
    assert_eq!(s.repeats as usize, gaps.len());
    assert_eq!(
        s.parties[PartyKind::Raid.index()] + s.parties[PartyKind::Invasion.index()],
        2 + s.repeats
    );
    assert_eq!(s.demands, 0, "a beast makes no demands");
    assert!(
        c.log
            .iter()
            .filter(|e| e.kind == "calm")
            .all(|e| !e.what.contains("scouts") && !e.what.contains("envoy")),
        "calms skip the empty rungs"
    );
    // Every repeat comes 4 days after the last band came home, to the tick while the kills go on
    // (MZB-51); never two bands about.
    let fed = calm_from.0 as f64 / TICKS_PER_DAY as f64;
    for (day, gap, what) in &gaps {
        assert!(*gap >= 4.0 - 1e-6, "day {day}: {gap} after home ({what})");
        if *day < fed {
            assert!(*gap <= 4.0 + 1e-3, "day {day}: {gap} after home ({what})");
        }
    }
    assert!(seen.iter().all(|x| x.bands <= 1));
    assert!(s.feud_wakes < 2_000, "{s:?}");
    c.check(&w).expect("books");
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
    assert_eq!(back.state_hash(), c.state_hash());
}

/// A band whose walk home is longer than `repeat_days` (a Frostmaw sent far, MZB-51): the pool
/// has its warrior back long before the band is home, and still the repeat waits until 4 days
/// after the arrival, so there is never a second Frostmaw.
#[test]
fn a_repeat_counts_from_the_arrival_home() {
    let cultures = edited(FROSTMAW_JSON, |d| {
        d.parties.insert("invasion".into(), Vec::new());
        d.march_blocks_per_tick = 0.0005;
    });
    let (mut w, mut c, f) = setup_with(22, cultures, "frostmaw");
    let p = c.player(&mut w, synth::PLAYER);
    let raid = theta(&c, f, Step::Raid);
    let seen = feed(
        &mut c,
        &mut w,
        f,
        p,
        2.0 * raid,
        Tick(10),
        Tick::from_days(90),
    );
    let homes: Vec<f64> = c
        .log
        .iter()
        .filter(|e| e.kind == "home")
        .map(|e| e.day)
        .collect();
    let walks: Vec<f64> = turns(&seen)
        .iter()
        .zip(&homes)
        .map(|(t, h)| h - t)
        .collect();
    assert!(
        walks.iter().all(|d| *d > 4.0),
        "each walk home outlasts repeat_days: {walks:?}"
    );
    let gaps = repeat_gaps(&c);
    assert!(!gaps.is_empty(), "{:?}", c.stats);
    for (day, gap, _) in &gaps {
        assert!(
            (4.0 - 1e-6..4.0 + 1e-3).contains(gap),
            "day {day}: {gap} after home"
        );
    }
    assert!(seen.iter().all(|x| x.bands <= 1), "never a second Frostmaw");
    c.check(&w).expect("books");
}

/// A pool short of a whole warrior (MZB-52): a Frostmaw's pool at 0.6 when it climbs to Raid
/// waits until it holds one, waking its feud a few times, not once a tick (about 72,000 times in
/// these 3 days with a wait for half a warrior).
#[test]
fn a_pool_short_of_a_whole_warrior_waits_for_it() {
    let (mut w, mut c, f) = setup_with(23, Cultures::builtin(), "frostmaw");
    let p = c.player(&mut w, synth::PLAYER);
    let raid = theta(&c, f, Step::Raid);
    kill(&mut c, &mut w, p, 2.0 * raid / 8.0, Tick(10));
    c.run_until(&mut w, &mut NoInfrastructure, Tick(20));
    assert_eq!(c.feud(f, p).unwrap().step, Step::Notice);
    c.factions[f as usize].pool = 0.6;
    c.factions[f as usize].pool_at = Tick(20);
    let wakes = c.stats.feud_wakes;
    let to = Tick(20 + 3 * TICKS_PER_DAY);
    let mut t = Tick(20);
    while t < to {
        t = Tick(t.0 + HOUR);
        c.run_until(&mut w, &mut NoInfrastructure, t);
    }
    let x = c.feud(f, p).unwrap();
    assert_eq!(x.step, Step::Raid, "it climbed once the warrior was there");
    assert_eq!(c.stats.parties[PartyKind::Raid.index()], 1);
    assert!(
        c.stats.feud_wakes - wakes < 10,
        "{} wakes",
        c.stats.feud_wakes - wakes
    );
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
    let seen = feed(
        &mut c,
        &mut w,
        f,
        p,
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
    let bend =
        Temperament::from_seed(c.factions[f as usize].chief_seed).bend(Trait::Aggressive, 1000, 50);
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
    let seen = feed(
        &mut c,
        &mut w,
        f,
        p,
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
        let seen = feed(
            &mut c,
            &mut w,
            f,
            p,
            2.0 * inv,
            Tick(10),
            Tick::from_days(60),
        );
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
    frostmaw_home_on_raid_with(seed, cultures)
}

/// A Frostmaw of `cultures` on Raid with its band home.
fn frostmaw_home_on_raid_with(
    seed: u64,
    cultures: Cultures,
) -> (World, Conflict, u32, ActorId, Tick) {
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
    feed(
        &mut c,
        &mut w,
        f,
        p,
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
    assert_eq!(
        c.feud(f, p).unwrap().step,
        Step::Envoy,
        "Siege, Scouts, Envoy"
    );

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
    let iron = w
        .goods
        .find("iron")
        .or_else(|| w.goods.ids().next())
        .unwrap();
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
    assert_eq!(
        c.parties[p].from_players.get(&iron),
        None,
        "nothing carried"
    );
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

/// An Umvuthana grove's first demand, its haven's market pricing gold nuggets at `price` (none:
/// no price), after a kill worth `times` Envoy's threshold. Returns the demand and what it should
/// be: `tribute_per_grievance` times the grievance at the climb, bent, times the nuggets a coin
/// buys (MOWZIE.md ledger 12, MZB-57).
fn grove_demand(
    seed: u64,
    price: Option<Price>,
    times: f64,
) -> (World, Conflict, u32, ActorId, Demand, i64) {
    let (mut w, mut c, f) = setup_with(seed, Cultures::builtin(), "umvuthana");
    let nugget = w.goods.find("gold_nugget").expect("the nugget good");
    let node = c.factions[f as usize].node;
    if let Some(pr) = price {
        synth::set_price(&mut w, node, nugget, pr, Tick(5));
    }
    let p = c.player(&mut w, synth::PLAYER);
    let envoy = theta(&c, f, Step::Envoy);
    kill(&mut c, &mut w, p, envoy * times / 8.0, Tick(10));
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
    assert_eq!(w.market(node).price(nugget), price.unwrap_or(0));
    let x = c.feud(f, p).unwrap();
    assert_eq!(x.g.anchor, d.made);
    let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
    let g = x.g.total_at(d.made, lambda);
    let bend =
        Temperament::from_seed(c.factions[f as usize].chief_seed).bend(Trait::Aggressive, 1000, 50);
    let per_coin = price.map_or(9.0, |pr| {
        (PRICE_ONE as f64 / pr as f64).min(TRIBUTE_PRICE_CAP * 9.0)
    });
    let want = (0.5 * g * f64::from(bend) / 1000.0 * per_coin).ceil() as i64;
    (w, c, f, p, d, want)
}

/// Tribute at the haven market's price (ledger 12): `ceil(base × PRICE_ONE / price)` nuggets, a
/// price near nothing held to 4 times the fallback (MZB-57).
#[test]
fn tribute_at_a_market_price_is_its_worth_in_nuggets() {
    let mut asked = Vec::new();
    for price in [
        PRICE_ONE / 4,
        PRICE_ONE / 9,
        PRICE_ONE / 20,
        PRICE_ONE / 1000,
    ] {
        let (w, c, _, _, d, want) = grove_demand(24, Some(price), 1.2);
        assert_eq!(d.tribute, want, "at {price}");
        asked.push(d.tribute);
        c.check(&w).expect("books");
    }
    // The same grievance each time: 4, 9, 20 and (capped) 36 nuggets a coin.
    let per = |k: usize| asked[k] as f64 / asked[0] as f64;
    assert!((per(1) - 9.0 / 4.0).abs() < 0.1, "{asked:?}");
    assert!((per(2) - 5.0).abs() < 0.1, "{asked:?}");
    assert!((per(3) - 9.0).abs() < 0.1, "capped at 36 a coin: {asked:?}");
}

/// A tribute paid in full at the camp settles the demand (part of it first does not): back to
/// Notice with the respite, the nuggets stocked at the haven, and the plugin told it was paid.
#[test]
fn a_priced_tribute_paid_in_full_settles() {
    let (mut w, mut c, f, p, d, want) = grove_demand(25, Some(PRICE_ONE / 4), 1.2);
    assert_eq!(d.tribute, want);
    let nugget = d.good;
    let at = Tick(d.made.0 + HOUR);
    let _ = c.take_commands();
    let part = BTreeMap::from([(nugget, (d.tribute - 1) * MILLI)]);
    assert!(!c.pay_tribute(&mut w, synth::CAMP, p, None, &part, at));
    assert_eq!(c.feud(f, p).unwrap().demand.unwrap().paid, d.tribute - 1);
    let rest = BTreeMap::from([(nugget, MILLI)]);
    assert!(c.pay_tribute(&mut w, synth::CAMP, p, None, &rest, at));
    let x = c.feud(f, p).unwrap();
    assert!(x.demand.is_none());
    assert_eq!(x.step, Step::Notice);
    let respite = c
        .cultures
        .get(c.factions[f as usize].culture)
        .def
        .respite_days;
    assert_eq!(
        x.wait_until,
        Tick(at.0 + (respite * TICKS_PER_DAY as f64) as u64)
    );
    assert_eq!(c.stats.tributes, 1);
    assert_eq!(
        c.ledger.tribute.get(&nugget),
        Some(&i128::from(d.tribute * MILLI))
    );
    assert!(
        c.take_commands()
            .iter()
            .any(|m| matches!(m, Cmd::Settled(s) if s.outcome == boundary::SETTLED_PAID))
    );
    c.check(&w).expect("books");
}

/// An envoy refused: the demand goes unmet and the grove climbs to Scouts, the next rung up it
/// has (ledger 2), the plugin told it was refused.
#[test]
fn a_refused_envoy_climbs_to_scouts() {
    let (mut w, mut c, f, p, d, want) = grove_demand(26, Some(PRICE_ONE / 9), 2.0);
    assert_eq!(d.tribute, want);
    let envoy = d.envoy.expect("an envoy carries the demand");
    let epoch = c.parties[envoy].epoch;
    let _ = c.take_commands();
    let answer = Obs::EnvoyAnswer(boundary::EnvoyAnswer {
        agent: Party::agent(envoy),
        epoch,
        answer: boundary::ANSWER_REFUSED,
        player: synth::PLAYER,
    });
    let at = Tick(d.made.0 + HOUR);
    c.ingest(&mut w, &mut NoInfrastructure, &answer, at);
    let x = c.feud(f, p).unwrap();
    assert!(x.demand.is_none());
    assert_eq!(x.step, Step::Scouts);
    assert_eq!(c.stats.expired, 1);
    assert_eq!(c.stats.tributes, 0);
    assert_eq!(c.stats.parties[PartyKind::Scouts.index()], 1);
    assert!(c.log.iter().any(|e| e.kind == "refused"));
    assert!(
        c.take_commands()
            .iter()
            .any(|m| matches!(m, Cmd::Settled(s) if s.outcome == boundary::SETTLED_REFUSED))
    );
    c.check(&w).expect("books");
}

/// A chief killed while its band walks home (reachable once M6 reports kills): the feud's step
/// falls two rungs and the feud is booked again (a new stamp), though neither `go_home` nor
/// `end_fight` runs; the band walks on home.
#[test]
fn a_chief_killed_on_the_way_home_replans_the_feud() {
    let cultures = edited(FROSTMAW_JSON, |d| {
        d.parties.insert("invasion".into(), Vec::new());
        d.march_blocks_per_tick = 0.0005;
    });
    let (mut w, mut c, f) = setup_with(27, cultures, "frostmaw");
    let p = c.player(&mut w, synth::PLAYER);
    let raid = theta(&c, f, Step::Raid);
    let mut t = Tick(10);
    let band = loop {
        let to = Tick(t.0 + HOUR);
        feed(&mut c, &mut w, f, p, 2.0 * raid, t, to);
        t = to;
        if let Some((h, _)) = c
            .parties
            .iter()
            .find(|(_, x)| x.faction == f && x.phase == Phase::Home)
        {
            break h;
        }
        assert!(t < Tick::from_days(60), "walking home by now");
    };
    assert!(c.parties[band].chief, "the Frostmaw is its own chief");
    let x = c.feud(f, p).unwrap();
    assert_eq!((x.step, x.party), (Step::Raid, None));
    let stamp = x.stamp;
    c.chief_killed(&mut w, band, Some(p), t);
    let x = c.feud(f, p).unwrap();
    assert_eq!(
        x.step,
        Step::Notice,
        "Raid, Notice (no rung below it but none)"
    );
    assert_ne!(x.stamp, stamp, "booked again");
    assert_eq!(c.parties[band].phase, Phase::Home, "it walks on home");
    let arrive = *c.parties[band].times.last().unwrap();
    let wakes = c.stats.feud_wakes;
    feed(&mut c, &mut w, f, p, 0.0, t, Tick(t.0 + 20 * TICKS_PER_DAY));
    assert!(c.stats.feud_wakes - wakes < 50, "{:?}", c.stats);
    assert!(
        c.log
            .iter()
            .any(|e| e.kind == "home" && Tick(e.tick) == arrive),
        "the band came home when it was to"
    );
    c.check(&w).expect("books");
}

/// A feud on a rung whose party is empty (MZB-64; a save made with other culture data does not
/// load, so only a culture edit in place gets here): no repeat of it and no wake loop; the climb
/// to Invasion still comes with the grievance.
#[test]
fn an_emptied_rung_neither_repeats_nor_loops() {
    let cultures = edited(FROSTMAW_JSON, |d| {
        d.parties.insert("raid".into(), Vec::new());
    });
    let (mut w, mut c, f) = setup_with(28, cultures, "frostmaw");
    let p = c.player(&mut w, synth::PLAYER);
    let (raid, inv) = (theta(&c, f, Step::Raid), theta(&c, f, Step::Invasion));
    kill(&mut c, &mut w, p, 2.0 * raid / 8.0, Tick(10));
    let x = c.feud_mut(f, p).unwrap();
    x.step = Step::Raid;
    x.since = Tick(10);
    let t = Tick(20);
    let before = c.stats;
    let mid = Tick(t.0 + 10 * TICKS_PER_DAY);
    feed(&mut c, &mut w, f, p, 1.5 * raid, t, mid);
    assert_eq!(
        c.stats.repeats, before.repeats,
        "no repeat of an empty rung"
    );
    assert_eq!(c.stats.parties, before.parties);
    assert!(c.stats.feud_wakes - before.feud_wakes < 50, "{:?}", c.stats);
    feed(
        &mut c,
        &mut w,
        f,
        p,
        1.5 * inv,
        mid,
        Tick(mid.0 + 40 * TICKS_PER_DAY),
    );
    assert_eq!(c.stats.escalations[Step::Invasion.index()], 1);
    assert!(c.stats.repeats >= 1, "the Invasion repeats: {:?}", c.stats);
    assert!(
        c.stats.feud_wakes - before.feud_wakes < 500,
        "{:?}",
        c.stats
    );
    c.check(&w).expect("books");
}

/// Rails for ambushes: every leg a band walks crosses train 9, and every attack on it loses three
/// of good 1. Records each threat.
struct Rails {
    threats: Vec<Threat>,
}

impl Infrastructure for Rails {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, _: &Threat) -> Option<RaidOutcome> {
        None
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }

    fn trains_crossed(
        &mut self,
        _: u16,
        _: &[(i32, i32)],
        from: Tick,
        to: Tick,
    ) -> Vec<TrainSighting> {
        vec![TrainSighting {
            train: TrainId(9),
            from,
            to,
        }]
    }

    fn raid_train(&mut self, t: &Threat) -> Option<GoodVec> {
        self.threats.push(*t);
        let mut g = GoodVec::new();
        g.add(Good(1), 3);
        Some(g)
    }
}

/// An ambush is one threat per class of unit (ledger 9, MZB-53), as a district raid is: a goblin
/// raid with more archers' strength than warriors' still sends its warriors as ground melee and
/// its archers as ground ranged, each with its own count, strength and carry; one ambush counted.
#[test]
fn an_ambush_is_one_threat_per_class() {
    let cultures = goblins_with(&["raid"], |_| {});
    let (mut w, mut c, f) = setup_with(29, cultures, "goblin");
    c.tun.ambush_milli = 1000;
    let band = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::Pos(synth::FOREST_POS),
            None,
            Tick(10),
        )
        .expect("a raid");
    // Damaged: 2 warriors (strength 2.0) and 4 archers (3.2).
    c.parties[band].units = vec![(0, 2), (1, 4)];
    let mut rails = Rails {
        threats: Vec::new(),
    };
    c.run_until(&mut w, &mut rails, Tick::from_days(3));
    assert!(c.stats.ambushes >= 1, "{:?}", c.stats);
    let first = rails.threats[0].at;
    let at_first: Vec<&Threat> = rails.threats.iter().filter(|t| t.at == first).collect();
    assert_eq!(at_first.len(), 2, "{at_first:?}");
    let by = |k: ThreatClass| *at_first.iter().find(|t| t.class == k).expect("each class");
    let (melee, ranged) = (by(ThreatClass::GroundMelee), by(ThreatClass::GroundRanged));
    assert_eq!((melee.count, ranged.count), (2, 4));
    assert!((melee.strength - 2.0).abs() < 1e-9 && (ranged.strength - 3.2).abs() < 1e-9);
    assert_eq!(
        (melee.loads.unwrap().carry, ranged.loads.unwrap().carry),
        (16, 16)
    );
    assert_ne!(melee.id, ranged.id);
    for t in &at_first {
        assert_eq!(t.source, ThreatSource::Bandit);
        assert_eq!(t.target, ThreatTarget::Train(TrainId(9)));
    }
    let times: std::collections::BTreeSet<u64> = rails.threats.iter().map(|t| t.at.0).collect();
    assert_eq!(
        c.stats.ambushes,
        times.len() as u64,
        "one ambush, two threats"
    );
    let _ = f;
    c.check(&w).expect("books");
}
