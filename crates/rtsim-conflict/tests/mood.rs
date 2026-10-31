// SPDX-License-Identifier: AGPL-3.0-or-later
//! S7, the raiders' plunder mood (EQUILIBRIUM.md §2.4.5, docs/overnight-choices-equilibrium.md
//! EQC285 to EQC289; Ben's FB-8: "it should depend on the raiders, with some RNG and on the
//! history of the raiders comprising the camp, their experiences, temperaments"): the wait to a
//! camp's next plunder is drawn from a mean of 48 days that its chief's temperament bends (the
//! aggression the engine already reads) and its recent history bends again: members lost lengthen
//! the wait, raids that brought loot home shorten it, and both fade. A new band has no history.
//! The raider carries the block and, since S9 (Ben's FB-S9-2, 2026-10-30), the Umvuthana grove carries
//! the same block unchanged; the other cultures keep their clocks exactly.

use std::collections::BTreeMap;

use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::culture::{
    BANDIT_JSON, Culture, CultureDef, FROSTMAW_JSON, GOBLIN_JSON, RAIDER_JSON, UMVUTHANA_JSON,
};
use rtsim_conflict::engine::Target;
use rtsim_conflict::{Conflict, ConflictTunables, Cultures, synth};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

const DAY: u64 = TICKS_PER_DAY;

fn def(json: &str) -> CultureDef {
    serde_json::from_str(json).unwrap()
}

fn refused(json: &str, edit: impl FnOnce(&mut CultureDef)) -> Option<String> {
    let mut d = def(json);
    edit(&mut d);
    Culture::resolve(d).err()
}

/// One camp of `culture` in the synthetic world (Ashby, the refinery and the camp at `synth::CAMP`).
fn camp(seed: u64, culture: &str) -> (World, Conflict, u32) {
    let (mut w, _) = synth::world(seed);
    let mut c = Conflict::new(
        seed,
        ConflictTunables::default(),
        Cultures::builtin(),
        Tick::ZERO,
    );
    synth::define_names(&mut c, &w);
    let f = c
        .add_faction(&mut w, synth::CAMP, culture, synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    (w, c, f)
}

/// `n` raider camps spread over a quiet world, with the faction index of each.
fn raiders(n: usize, seed: u64) -> (World, Conflict, Vec<u32>) {
    let names = vec!["raider"; n];
    let (w, c) = synth::camps(4, &names, seed, Cultures::builtin());
    let fs = (0..n as u64)
        .map(|k| c.faction_of_key(synth::CAMP_KEY + k).expect("a camp"))
        .collect();
    (w, c, fs)
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

// ---------------------------------------------------------------- the data

/// The raider carries the block, with the numbers EQC286 decided, and the grove carries the same
/// block (FB-S9-2: Ben said yes on 2026-10-30, no new code); no other culture does, and each way the
/// block can be wrong is its own error.
#[test]
fn the_raider_and_the_grove_have_a_plunder_mood_and_no_other_culture_does() {
    let c = Cultures::builtin();
    for n in ["goblin", "bandit", "frostmaw"] {
        assert!(c.get(c.index_of(n).unwrap()).plunder_mood.is_none(), "{n}");
    }
    let grove = c.get(c.index_of("umvuthana").unwrap());
    let grove_mood = grove.plunder_mood.as_ref().expect("the grove has a mood");
    let raider_mood = c
        .get(c.index_of("raider").unwrap())
        .plunder_mood
        .as_ref()
        .expect("the raider has a mood");
    assert_eq!(grove_mood.def, raider_mood.def, "the raider's block, reused");
    let cu = c.get(c.index_of("raider").unwrap());
    let m = cu.plunder_mood.as_ref().expect("the raider has a mood");
    assert_eq!(m.def.half_life_days, 96.0);
    assert_eq!((m.def.min_factor, m.def.max_factor), (0.5, 3.0));
    assert!(m.def.loss_gain > 0.0 && m.def.spoils_gain > 0.0 && m.def.chief_loss_share > 0.0);
    // The centre of the temperament's bend over the 256 aggressions, so that the mean over camps
    // is the culture's own: 1000 / bend, 500 to 1,500, averages a little over one.
    assert!(
        (1.05..1.15).contains(&m.temper_mean),
        "temper_mean {}",
        m.temper_mean
    );
    // The den's file and the other pool cultures' carry no such key; the grove's does.
    for json in [FROSTMAW_JSON, GOBLIN_JSON, BANDIT_JSON] {
        assert!(!json.contains("plunder_mood"));
    }
    assert!(UMVUTHANA_JSON.contains("plunder_mood"));

    let e = refused(GOBLIN_JSON, |d| {
        d.plunder_mood = def(RAIDER_JSON).plunder_mood
    })
    .map(|e| e.contains("plunder_mood"));
    assert_eq!(e, None, "a culture that plunders may carry one");
    let e = refused(FROSTMAW_JSON, |d| {
        d.plunder_mood = def(RAIDER_JSON).plunder_mood
    })
    .expect("no plunder, no mood");
    assert!(
        e.contains("plunder_mood") && e.contains("plunder_mean_days"),
        "{e}"
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let e = refused(RAIDER_JSON, |d| {
            d.plunder_mood.as_mut().unwrap().loss_gain = bad
        })
        .expect("not finite");
        assert!(e.contains("finite"), "{e}");
    }
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().half_life_days = 0.0
    })
    .expect("a half-life above 0");
    assert!(e.contains("half_life_days"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().loss_gain = -1.0
    })
    .expect("a gain is not negative");
    assert!(e.contains("loss_gain"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().spoils_gain = -0.1
    })
    .expect("a gain is not negative");
    assert!(e.contains("spoils_gain"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().chief_loss_share = -0.1
    })
    .expect("a share is not negative");
    assert!(e.contains("chief_loss_share"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().min_factor = 1.5
    })
    .expect("min_factor above 1");
    assert!(e.contains("min_factor"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().max_factor = 0.9
    })
    .expect("max_factor under 1");
    assert!(e.contains("max_factor"), "{e}");
    let e = refused(RAIDER_JSON, |d| {
        d.plunder_mood.as_mut().unwrap().min_factor = 0.0
    })
    .expect("min_factor above 0");
    assert!(e.contains("min_factor"), "{e}");
}

/// Only the raider's and the grove's camps have a mood.
#[test]
fn only_a_raider_or_grove_camp_has_a_mood() {
    for (culture, has) in [
        ("raider", true),
        ("umvuthana", true),
        ("goblin", false),
        ("bandit", false),
    ] {
        let (_, c, f) = camp(3, culture);
        assert_eq!(c.mood_now(f, Tick::ZERO).is_some(), has, "{culture}");
        assert!(
            c.plunder_mean_days(f, Tick::ZERO).is_some(),
            "{culture} plunders"
        );
    }
}

// ---------------------------------------------------------------- temperament

/// The mean over many camps with no history is the culture's 48 days, and the temperament still
/// spreads it: a bold band goes at a bit over two thirds of it, a meek one waits nearly twice.
#[test]
fn a_neutral_band_waits_the_cultures_mean_and_the_temperament_spreads_it() {
    let mut means = Vec::new();
    for seed in 0..16u64 {
        let (_, c, fs) = raiders(200, 100 + seed);
        for f in fs {
            means.push(c.plunder_mean_days(f, Tick::ZERO).unwrap());
        }
    }
    let m = mean(&means);
    let (lo, hi) = means
        .iter()
        .fold((f64::MAX, 0.0f64), |(l, h), &x| (l.min(x), h.max(x)));
    assert!(
        (47.5..=48.5).contains(&m),
        "the mean of {} camps is {m}",
        means.len()
    );
    assert!(lo < 0.7 * 48.0 && hi > 1.6 * 48.0, "spread {lo} to {hi}");
    // And bounded by the temperament's bend alone: within (0.55, 1.85) of the mean.
    assert!(lo > 0.55 * 48.0 && hi < 1.85 * 48.0, "spread {lo} to {hi}");
}

// ---------------------------------------------------------------- history

/// Members a raid lost lengthen the wait by `exp(loss_gain x lost / cap)`; the loss is any cause.
#[test]
fn members_lost_in_a_raid_lengthen_the_wait() {
    let (mut w, mut c, f) = camp(5, "raider");
    let ashby = w.node_of_key(4).expect("Ashby");
    let before = c.plunder_mean_days(f, Tick(10)).unwrap();
    assert_eq!(c.mood_now(f, Tick(10)), Some((0.0, 0.0)));
    c.order_party(
        &mut w,
        synth::CAMP,
        PartyKind::Plunder,
        Target::Node(ashby),
        None,
        Tick(10),
    )
    .expect("plunderers");
    // Take the pool away so the kills fall on the band: 4 of its 8 are lost, by anyone.
    c.factions[f as usize].pool = 0.0;
    c.factions[f as usize].pool_at = Tick(10);
    let fell = c.slay(&mut w, f, 4, Tick(10));
    assert_eq!(fell, 4);
    let (g, s) = c.mood_now(f, Tick(10)).unwrap();
    let cap = c.factions[f as usize].cap as f32;
    assert!((g - 4.0 / cap).abs() < 1e-6, "grief {g}");
    assert_eq!(s, 0.0);
    let gain = c
        .cultures
        .get(c.factions[f as usize].culture)
        .plunder_mood
        .unwrap()
        .def
        .loss_gain;
    let after = c.plunder_mean_days(f, Tick(10)).unwrap();
    let want = before * f64::from(gain * 4.0 / cap).exp();
    assert!((after - want).abs() < 1e-6 * want, "{after} against {want}");
    assert!(after > 1.2 * before, "{after} against {before}");
}

/// A raid that brings loot home shortens the next wait; the loot may be a small price in members.
#[test]
fn a_raid_that_brings_loot_home_shortens_the_wait() {
    let (mut w, mut c, f) = camp(6, "raider");
    let ashby = w.node_of_key(4).expect("Ashby");
    let m = w.market(ashby);
    let rich: BTreeMap<GoodId, i64> = w
        .goods
        .ids()
        .map(|g| (g, m.line(g).target + 20 * MILLI - m.stock(g)))
        .filter(|e| e.1 > 0)
        .collect();
    w.stock_from_players(ashby, &rich, Tick(10));
    c.order_party(
        &mut w,
        synth::CAMP,
        PartyKind::Plunder,
        Target::Node(ashby),
        None,
        Tick(10),
    )
    .expect("plunderers");
    let mut t = Tick(10);
    while c.ledger.from_villages.is_empty() || !c.parties.is_empty() {
        t = c.next_due().unwrap_or(t + DAY);
        assert!(t < Tick(30 * DAY), "never raided and came home");
        c.run_until(&mut w, &mut NoInfrastructure, t);
    }
    let (g, s) = c.mood_now(f, t).unwrap();
    assert!(
        (0.9..=1.0).contains(&s),
        "one raid with loot is one spoil: {s}"
    );
    assert!(g >= 0.0);
    // The gain is the data's, and the same temperament reads shorter than it would with none.
    let cu = c
        .cultures
        .get(c.factions[f as usize].culture)
        .plunder_mood
        .unwrap();
    let neutral = {
        let (_, c2, f2) = camp(6, "raider");
        c2.plunder_mean_days(f2, t).unwrap()
    };
    let now = c.plunder_mean_days(f, t).unwrap();
    let want = neutral * f64::from(cu.def.loss_gain * g - cu.def.spoils_gain * s).exp();
    assert!((now - want).abs() < 1e-3 * want, "{now} against {want}");
    assert!(
        now < neutral || g > 0.05,
        "{now} against the neutral {neutral}"
    );
}

/// A war chief's fall counts as a share of the cap lost.
#[test]
fn a_war_chief_lost_counts_as_a_share_of_the_cap() {
    let (mut w, mut c, f) = camp(7, "raider");
    let p = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            None,
            Tick(10),
        )
        .expect("a raid");
    assert!(c.parties[p].chief, "the chief leads the raid");
    c.chief_killed_by(&mut w, p, None, 0, None, Tick(10));
    let (g, _) = c.mood_now(f, Tick(10)).unwrap();
    let share = c
        .cultures
        .get(c.factions[f as usize].culture)
        .plunder_mood
        .unwrap()
        .def
        .chief_loss_share;
    assert!(g >= share - 1e-6, "grief {g} for a chief share {share}");
}

/// Both memories halve in `half_life_days`, and are gone in a few years: the wait comes back to
/// what the temperament says.
#[test]
fn the_memory_fades() {
    let (mut w, mut c, f) = camp(8, "raider");
    let ashby = w.node_of_key(4).expect("Ashby");
    c.order_party(
        &mut w,
        synth::CAMP,
        PartyKind::Plunder,
        Target::Node(ashby),
        None,
        Tick(10),
    )
    .expect("plunderers");
    c.factions[f as usize].pool = 0.0;
    c.factions[f as usize].pool_at = Tick(10);
    c.slay(&mut w, f, 6, Tick(10));
    let (g0, _) = c.mood_now(f, Tick(10)).unwrap();
    let (g1, _) = c.mood_now(f, Tick(10 + 96 * DAY)).unwrap();
    assert!((g1 - g0 / 2.0).abs() < 1e-5, "{g1} against half of {g0}");
    let neutral = {
        let (_, c2, f2) = camp(8, "raider");
        c2.plunder_mean_days(f2, Tick(10 + 2000 * DAY)).unwrap()
    };
    let later = c.plunder_mean_days(f, Tick(10 + 2000 * DAY)).unwrap();
    assert!(
        (later - neutral).abs() < 1e-3 * neutral,
        "{later} against {neutral}"
    );
}

/// No history, however long, takes the mean past the data's bounds.
#[test]
fn the_factor_is_clamped() {
    let (mut w, mut c, f) = camp(9, "raider");
    let neutral = c.plunder_mean_days(f, Tick(10)).unwrap();
    // A great grief: the camp lost a hundred times its cap.
    c.grieve(f, 100.0, Tick(10));
    let high = c.plunder_mean_days(f, Tick(10)).unwrap();
    assert!(
        (high - 3.0 * neutral).abs() < 1e-9 * high,
        "{high} against 3 x {neutral}"
    );
    // And a great run of luck: a hundred raids with loot.
    let (mut w2, mut c2, f2) = camp(9, "raider");
    for _ in 0..100 {
        c2.gloat(f2, Tick(10));
    }
    let low = c2.plunder_mean_days(f2, Tick(10)).unwrap();
    assert!(
        (low - 0.5 * neutral).abs() < 1e-9 * low,
        "{low} against half of {neutral}"
    );
    let _ = (&mut w, &mut w2);
}

// ---------------------------------------------------------------- a new band, and saves

/// A refounded camp is a new band: a new chief and temperament, and no history.
#[test]
fn a_new_band_has_no_history_and_its_own_temperament() {
    let (mut w, mut c, f) = camp(10, "raider");
    c.grieve(f, 0.4, Tick(10));
    c.gloat(f, Tick(10));
    let (g, s) = c.mood_now(f, Tick(10)).unwrap();
    assert!(g > 0.3 && s > 0.9);
    let seed = c.factions[f as usize].chief_seed;
    // Farmed to a break, then left alone for its wait: refounded.
    c.slay(&mut w, f, u32::MAX, Tick(20));
    c.pool_press(f, 3.0, Tick(20));
    assert!(!c.camp_active(f), "the camp broke");
    let due = c.factions[f as usize]
        .strain
        .as_ref()
        .unwrap()
        .broken
        .unwrap()
        .due;
    c.run_until(&mut w, &mut NoInfrastructure, due + 1);
    assert!(c.camp_active(f), "the camp was refounded");
    assert_ne!(c.factions[f as usize].chief_seed, seed, "a new chief");
    let (g, s) = c.mood_now(f, due + 1).unwrap();
    assert_eq!((g, s), (0.0, 0.0), "no memory of the old band's raids");
}

/// The history is in the pages: a saved and loaded engine reads the same mood and the same hash.
#[test]
fn a_camps_mood_survives_a_save() {
    let (mut w, mut c, f) = camp(11, "raider");
    c.grieve(f, 0.25, Tick(10));
    c.gloat(f, Tick(10));
    let want = c.mood_now(f, Tick(40 * DAY)).unwrap();
    let mean_before = c.plunder_mean_days(f, Tick(40 * DAY)).unwrap();
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).expect("loads");
    assert_eq!(back.state_hash(), c.state_hash());
    let f2 = back.faction_of_key(synth::CAMP).unwrap();
    assert_eq!(back.mood_now(f2, Tick(40 * DAY)).unwrap(), want);
    assert_eq!(
        back.plunder_mean_days(f2, Tick(40 * DAY)).unwrap(),
        mean_before
    );
    let _ = &mut w;
}

// ---------------------------------------------------------------- the whole engine, many camps

/// What `camps` raiders did over `days` game days in a world of 24 camps, `rich` villages
/// refilled every ten days or not: the mean of the gaps their plunder clock drew (the draws the
/// camps had made, less the last of each, which had not run out), the mean grief and spoils at
/// the end, the lowest and highest mean wait any camp ended with, and the raids and casualties.
struct Years {
    gap: f64,
    grief: f64,
    spoils: f64,
    lo: f64,
    hi: f64,
    raids: u64,
    casualties: u64,
}

fn years(seeds: u64, days: u64, rich: bool) -> Years {
    let (mut draws, mut n, mut raids, mut casualties) = (0u64, 0u64, 0u64, 0u64);
    let (mut grief, mut spoils) = (0.0f64, 0.0f64);
    let (mut lo, mut hi) = (f64::MAX, 0.0f64);
    for seed in 0..seeds {
        let (mut w, mut c, fs) = raiders(24, 700 + seed);
        let mut d = 0;
        while d < days {
            if rich {
                // Every village has something over its targets to take (a raid takes the surplus).
                for i in 0..w.nodes.len() {
                    let m = w.market(i);
                    let top: BTreeMap<GoodId, i64> = w
                        .goods
                        .ids()
                        .map(|g| (g, m.line(g).target + 20 * MILLI - m.stock(g)))
                        .filter(|e| e.1 > 0)
                        .collect();
                    w.stock_from_players(i, &top, c.now());
                }
            }
            d += 10;
            c.run_until(&mut w, &mut NoInfrastructure, Tick(d * DAY));
        }
        for &f in &fs {
            draws += u64::from(c.factions[f as usize].plunders);
            let (g, s) = c.mood_now(f, c.now()).unwrap();
            grief += f64::from(g);
            spoils += f64::from(s);
            let m = c.plunder_mean_days(f, c.now()).unwrap();
            (lo, hi) = (lo.min(m), hi.max(m));
            n += 1;
        }
        raids += c.stats.raids_village;
        casualties += c.stats.casualties;
    }
    Years {
        gap: (n * days) as f64 / (draws - n) as f64,
        grief: grief / n as f64,
        spoils: spoils / n as f64,
        lo,
        hi,
        raids,
        casualties,
    }
}

/// FB-8: "~48 days is a good average". In a world where no village has anything to take the
/// camps' clocks run on their temperaments alone, and the gaps they draw average 48 days.
#[test]
fn a_quiet_world_of_raiders_plunders_about_every_48_days() {
    let y = years(6, 960, false);
    println!(
        "quiet: a plunder every {:.1} days, {} raids, grief {:.3}, spoils {:.3}",
        y.gap, y.raids, y.grief, y.spoils
    );
    assert!((45.0..=51.0).contains(&y.gap), "{}", y.gap);
    // Almost no history: the means are the temperament's, 0.6 to 1.8 of 48, give or take a raid.
    assert!(y.grief < 0.02 && y.spoils < 0.2, "{} {}", y.grief, y.spoils);
}

/// ... and where every village is rich, so that the camps raid, lose members and bring loot home
/// all the time, the history is live (they mourn and they gloat) and the mean is still about 48:
/// the data's gains are set so that the loot and the losses of a typical camp balance.
#[test]
fn a_world_of_rich_villages_keeps_the_mean_near_48_with_the_history_live() {
    let y = years(6, 960, true);
    println!(
        "rich: a plunder every {:.1} days, {} raids, {} casualties, grief {:.3}, spoils {:.3}, means {:.1} to {:.1}",
        y.gap, y.raids, y.casualties, y.grief, y.spoils, y.lo, y.hi
    );
    assert!(y.raids > 1000 && y.casualties > 1000, "{} raids", y.raids);
    assert!(y.grief > 0.05, "they mourn: {}", y.grief);
    assert!(y.spoils > 1.0, "they gloat: {}", y.spoils);
    assert!((42.0..=54.0).contains(&y.gap), "{}", y.gap);
    // The history spreads the camps beyond what the temperament does (0.6 to 1.8 of 48), within
    // the data's bounds (0.5 to 3 times the temperament's own mean, which is under 2 times 48).
    assert!(
        y.hi > 1.85 * 48.0 && y.lo < 0.6 * 48.0,
        "{} to {}",
        y.lo,
        y.hi
    );
    assert!(
        y.lo >= 0.5 * 0.6 * 48.0 - 1e-9 && y.hi <= 3.0 * 1.82 * 48.0,
        "bounded: {} to {}",
        y.lo,
        y.hi
    );
}
