// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons on the shared renewal rule (EQUILIBRIUM.md §2.4.1, slice S4): a farmed lair settles
//! at the closed form's yield, and a lair whose ground is eaten out loses its dragon and waits for
//! the herds before it takes another. The world-level checks (kill all, the 60-year roamer) are in
//! `rtsim-cli`'s `sim::equilibrium`; the oil visitors' wait is `oil.rs`'s
//! `visitor_claims_only_a_due_lair`.
//!
//! The site ledger's hand-over (EQUILIBRIUM.md §2.3, slice S3, MZB-1012): a lair new to the engine
//! takes its row's pressure and wait from the found tail, waits the cap when found late with no
//! row (and an adoption into it resets its pressure), and the rows the engine sends are marked in a
//! set that is never saved and that no decision reads.

use rtsim_ecology::boundary::{self, Obs};
use rtsim_ecology::herds::climate::Uniform;
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::{DragonTunables, Dragons, Gender, LairId, LairKind, NewDragon, Species, synth};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::sites::{FoundRow, SiteKind, state_flags, tail_flags};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

/// Days in a game year (the dragons' calendar).
const YEAR: u64 = 96;

/// The scenario world with every village and road moved 100,000 blocks west: nothing near the
/// valley feeds a dragon but the herds a test puts there.
fn world_without_villages(seed: u64) -> World {
    let (g, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let nodes = synth::nodes()
        .into_iter()
        .map(|mut n| {
            n.pos.x -= 100_000;
            n
        })
        .collect();
    World::build(WorldConfig::new(seed), g, book, nodes)
}

/// §3.3: a lair whose every newcomer a player kills within two days of its claim (the model's
/// farmer looks every 2 days) settles near `P* = 2.12` and the closed form's 1.08 arrivals a
/// year; the plan's bound is 1.2. Each kill is the player's, so each raises the lair's pressure,
/// and the next wait reads it.
#[test]
fn farmed_lair_yields_closed_form() {
    // The newcomers' flights are the same length whatever the seed: one seed says it all.
    farmed_lair(11);
}

fn farmed_lair(seed: u64) {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let l = d.add_lair(
        &mut w,
        1,
        "Farmed Roost",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 20_000, 20_000),
        100,
        Tick::ZERO,
    );
    let player = d.player(&mut w, (1, 2));
    let mut kills: Vec<u64> = Vec::new();
    let mut p_after: Vec<f32> = Vec::new();
    let years = 60;
    let mut day = 0;
    while day < years * YEAR {
        day += 2;
        let at = Tick(day * TICKS_PER_DAY);
        d.run_until(&mut w, &mut NoInfrastructure, at);
        for o in d.lairs[l as usize].occupants.clone() {
            d.slay(&mut w, o, Some(player), at);
            kills.push(day);
            let rule = d.tun.renewal();
            p_after.push(
                d.lairs[l as usize]
                    .pressure
                    .now(day as u32, rule.half_life_days),
            );
        }
    }
    assert_eq!(d.stats.player_losses, kills.len() as u64);
    // The farmed state, over the last 30 years.
    let late: Vec<usize> = (0..kills.len())
        .filter(|&i| kills[i] >= 30 * YEAR)
        .collect();
    let rate = late.len() as f64 / 30.0;
    let p_mean = late.iter().map(|&i| f64::from(p_after[i])).sum::<f64>() / late.len() as f64;
    let (p_star, gap) = d.tun.renewal().farmed(1, 2.0);
    let closed = YEAR as f64 / f64::from(gap);
    eprintln!(
        "farmed lair, seed {seed}: {} kills in 60 years, {rate:.3} a year over the last 30 (closed form {closed:.3}), \
         P after a kill {p_mean:.3} (closed form {p_star:.3})",
        kills.len()
    );
    assert!(rate <= 1.2, "{rate:.3} arrivals a year");
    assert!(
        (rate - closed).abs() / closed <= 0.06,
        "{rate:.3} a year against the closed form's {closed:.3}"
    );
    assert!(
        (p_mean - f64::from(p_star)).abs() / f64::from(p_star) <= 0.06,
        "P {p_mean:.3} against {p_star:.3}"
    );
    // Every wait was the rule's: none shorter than the base, none longer than the cap.
    for k in kills.windows(2) {
        let gap = k[1] - k[0];
        assert!(
            (20..=192 + 4).contains(&gap),
            "{gap} days between kills {k:?}"
        );
    }
}

/// §2.4.1: a dragon whose lair's ground feeds it nothing starves out (no player loss: the lair's
/// pressure stays 0) and, with no vacant lair in reach, leaves the sim. Its lair's wait ends after
/// the base, but the ground would not feed a newcomer, so the lair checks again every 24 days and
/// stays empty; once herds graze near it, the next check brings a newcomer, which stays.
#[test]
fn starving_dragon_leaves_and_returns_when_fed() {
    for seed in 17..=24 {
        starving_dragon(seed);
    }
}

fn starving_dragon(seed: u64) {
    let mut w = world_without_villages(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    d.log_enabled = true;
    let lair_pos = Pos::new(0, 0, 2_500);
    let l = d.add_lair(
        &mut w,
        7,
        "Bare Roost",
        Species::Fire,
        LairKind::Roost,
        lair_pos,
        100,
        Tick::ZERO,
    );
    let old = d.add_dragon(
        &mut w,
        NewDragon {
            name: "Gaunt".into(),
            species: Species::Fire,
            gender: Gender::Female,
            variant: 0,
            age_ticks: 110 * TICKS_PER_DAY,
            lair: Some(l),
            fullness: 30,
            health: None,
            seed: None,
            at: None,
        },
        Tick::ZERO,
    );
    let mut h = Herds::new(seed, HerdTunables::default(), Box::new(Uniform), Tick::ZERO);
    let tun = d.tun.clone();
    // Starving: the fed share falls by e^(-1/30) a day from 1, below 0.35 after 32 days.
    let mut left = None;
    for day in 1..=60u64 {
        d.run_until_with_herds(
            &mut w,
            &mut NoInfrastructure,
            &mut h,
            Tick(day * TICKS_PER_DAY),
        );
        if d.dragon(old).is_none() {
            left = Some(day);
            break;
        }
    }
    let left = left.expect("the starving dragon left");
    assert!((30..=40).contains(&left), "left on day {left}");
    assert_eq!(
        (
            d.stats.starved_away,
            d.stats.migrations,
            d.stats.player_losses
        ),
        (1, 0, 0)
    );
    let lair = &d.lairs[l as usize];
    assert!(lair.occupants.is_empty());
    assert!(lair.fed < tun.starve_leave);
    assert_eq!(lair.pressure.now(left as u32, 96.0), 0.0);
    // It left during the day before `left`: the wait is the base from then.
    let base = tun.respawn_delay_days * TICKS_PER_DAY;
    assert!(
        lair.due > Tick((left - 1) * TICKS_PER_DAY) + base
            && lair.due <= Tick(left * TICKS_PER_DAY) + base,
        "due {:?}, left on day {left}",
        lair.due
    );
    // The wait ends after the base, but nothing would feed a newcomer: checked, put off, and
    // checked again 24 days later.
    let due = lair.due;
    let gated_until = due + 2 * tun.arrive_recheck_days * TICKS_PER_DAY + TICKS_PER_DAY;
    d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, gated_until);
    assert_eq!(
        d.stats.arrivals_gated, 3,
        "at the wait's end and twice after"
    );
    assert_eq!((d.stats.respawns, d.stats.heirs_sent), (0, 0));
    assert!(d.lairs[l as usize].occupants.is_empty());
    // Herds come to graze by the lair (on uniform grassland, so they stay near it): the next
    // check brings a newcomer.
    for (i, (dx, dz)) in [(-250, -150), (200, 100), (0, 300)].into_iter().enumerate() {
        h.add_herd(
            NewHerd {
                name: format!("Returning flock {i}"),
                species: HerdSpecies::Sheep,
                frame: 0,
                pos: (lair_pos.x + dx, lair_pos.z + dz),
                count: 2_000,
                seed: Some(40 + i as u64),
            },
            gated_until,
        );
    }
    let next_check = due + 3 * tun.arrive_recheck_days * TICKS_PER_DAY;
    d.run_until_with_herds(
        &mut w,
        &mut NoInfrastructure,
        &mut h,
        next_check + 2 * TICKS_PER_DAY,
    );
    assert_eq!(d.stats.arrivals_gated, 3, "fed ground passes the gate");
    assert_eq!(d.stats.respawns, 1);
    let newcomer = *d.lairs[l as usize]
        .occupants
        .first()
        .expect("a newcomer claimed the lair");
    // It hunts the herds and stays: two seasons on, it still holds the lair, well fed.
    let later = next_check + 2 * 24 * TICKS_PER_DAY * 2;
    d.run_until_with_herds(&mut w, &mut NoInfrastructure, &mut h, later);
    assert_eq!(d.stats.starved_away, 1);
    assert_eq!(d.lairs[l as usize].occupants, vec![newcomer]);
    assert!(d.stats.herd_hunts > 0);
    let fed = d.lairs[l as usize].fed;
    assert!(fed >= tun.arrive_food, "fed share {fed}");
}

// ---- the site ledger's hand-over (S3) ----------------------------------------------------------

const ROOST: Pos = Pos {
    frame: 0,
    x: 20_000,
    z: 20_000,
};

fn day_of(t: Tick) -> u32 {
    (t.0 / TICKS_PER_DAY) as u32
}

/// A row as the found tail hands it over: K 1, `count` dragons, P as of the stamp, the wait left
/// as of the stamp when `wait_left` is set, `elapsed` game ticks since.
fn row(p: f32, count: u16, wait_left: Option<u64>, elapsed: u64) -> FoundRow {
    let mut flags = tail_flags::ROW;
    if wait_left.is_some() {
        flags |= tail_flags::WAITING;
    }
    FoundRow {
        flags,
        k: 1,
        count,
        p,
        p2: 0.0,
        away: 0,
        wait_left: wait_left.unwrap_or(0),
        elapsed,
        bodies: Default::default(),
    }
}

fn found(d: &mut Dragons, w: &mut World, key: u64, late: bool, row: &FoundRow, at: Tick) -> LairId {
    let name = format!("Roost {key}");
    let (sp, kind) = (Species::Fire, LairKind::Roost);
    d.found_lair(w, key, &name, sp, kind, ROOST, 100, late, row, at)
}

/// The plugin's LAIR_FOUND for lair `key` at the roost.
fn lair_found(key: u64, late: bool, placed: bool) -> Obs {
    Obs::LairFound(boundary::LairFound {
        lair: key,
        late,
        placed,
        dimension: 0,
        species: Species::Fire,
        kind: LairKind::Roost,
        min: (ROOST.x - 20, 90, ROOST.z - 20),
        max: (ROOST.x + 20, 120, ROOST.z + 20),
        home: (ROOST.x, 100, ROOST.z),
        row: FoundRow::none(),
    })
}

/// The lair's own world-generated dragon, found standing in its footprint by the lair's census
/// (LAIR_CENSUS, EQUILIBRIUM.md S5): the plugin no longer reports a lair's dragons untagged.
fn its_dragon(key: u64, entity: u64) -> Obs {
    Obs::LairCensus(boundary::LairCensus {
        lair: key,
        epoch: 1,
        dimension: 0,
        flags: 0,
        bodies: vec![boundary::CensusBody {
            entity: (entity, entity),
            agent: 0,
            species: Species::Fire,
            flags: boundary::BODY_SLEEPING,
            age_ticks: 110 * 24_000,
            hunger: 60,
            health: 400.0,
            gender: Gender::Female,
            variant: 0,
            pos: (f64::from(ROOST.x), 100.0, f64::from(ROOST.z)),
        }],
    })
}

/// A lair found with a waiting row resumes its wait: the pressure aged by the game time since the
/// stamp, the wait left less that time, and no dragon comes before it ends. A wait that ran out
/// while the engine was gone is due at once.
#[test]
fn a_lair_found_with_a_row_resumes_its_wait() {
    let (mut w, _) = synth::world(3);
    let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
    let h = d.tun.renewal().half_life_days;
    let at = Tick(50 * TICKS_PER_DAY);
    let (left, elapsed) = (30 * TICKS_PER_DAY, 4 * TICKS_PER_DAY + 7);
    let l = found(
        &mut d,
        &mut w,
        1,
        true,
        &row(1.0, 0, Some(left), elapsed),
        at,
    );
    let lair = &d.lairs[l as usize];
    assert_eq!(lair.due, at + (left - elapsed));
    assert_eq!(lair.vacant_since, Some(at));
    assert!(!lair.found_unknown, "a row is its history");
    let want = Pressure::handed_over(1.0, elapsed, day_of(at), h).now(day_of(at), h);
    assert_eq!(lair.pressure.now(day_of(at), h), want);
    assert!((want - (-(elapsed as f32) / 24_000.0 / h).exp2()).abs() < 1e-6);
    let due = lair.due;
    d.run_until(&mut w, &mut NoInfrastructure, Tick(due.0 - 1));
    assert_eq!(
        (d.stats.respawns, d.stats.heirs_sent),
        (0, 0),
        "nothing before the wait ends"
    );
    d.run_until(&mut w, &mut NoInfrastructure, due + 1);
    assert_eq!(d.stats.respawns, 1, "a newcomer once it does");
    // A wait that ran out while the engine was gone: due at once.
    let l = found(
        &mut d,
        &mut w,
        2,
        true,
        &row(0.5, 0, Some(left), left + 99),
        due + 1,
    );
    assert_eq!(d.lairs[l as usize].due, due + 1);
}

/// A lair found late with no row waits the cap, to the tick, with the pressure that gives (§2.3:
/// a site found empty with no row is taken as farmed). Its own dragon adopted during that wait
/// resets the pressure: its history was lost, not its dragon. A lair found with a row keeps its
/// pressure through an adoption.
#[test]
fn a_late_lair_with_no_row_waits_the_cap_and_its_adoption_resets_it() {
    let (mut w, _) = synth::world(4);
    let mut d = Dragons::new(4, DragonTunables::default(), Tick::ZERO);
    let rule = d.tun.renewal();
    let h = rule.half_life_days;
    let at = Tick(10 * TICKS_PER_DAY);
    let mut infra = NoInfrastructure;
    d.ingest(&mut w, &mut infra, &lair_found(1, true, false), at);
    let l = d.lair_of_key(1).expect("found");
    let lair = &d.lairs[l as usize];
    assert!(lair.found_unknown);
    assert_eq!(lair.due, at + rule.cap_ticks());
    let at_cap = Pressure::at_cap(&rule, day_of(at)).now(day_of(at), h);
    assert_eq!(lair.pressure.now(day_of(at), h), at_cap);
    assert!((at_cap - (rule.cap_days / rule.base_days).log2()).abs() < 1e-6);
    // Its dragon joins: adopted into it, no pressure left.
    let later = at + 5 * TICKS_PER_DAY;
    d.ingest(&mut w, &mut infra, &its_dragon(1, 0xD1), later);
    let lair = &d.lairs[l as usize];
    assert_eq!(lair.occupants.len(), 1, "adopted into its lair");
    assert!(!lair.found_unknown);
    assert_eq!(lair.pressure.now(day_of(later), h), 0.0);
    // A lair with a row keeps the row's pressure when its dragon is adopted.
    let rowed = row(1.0, 0, Some(40 * TICKS_PER_DAY), 0);
    let l2 = found(&mut d, &mut w, 2, true, &rowed, later);
    d.ingest(&mut w, &mut infra, &its_dragon(2, 0xD2), later + 1);
    let lair = &d.lairs[l2 as usize];
    assert_eq!(lair.occupants.len(), 1);
    assert!(lair.pressure.now(day_of(later), h) > 0.99);
}

/// A lair the plugin placed (PLACE_LAIR's answer: LATE, its chunk is old, and PLACED) is new to
/// the world: it starts fresh, as one just generated. So does a lair found in a new chunk.
#[test]
fn a_placed_lair_starts_fresh() {
    let (mut w, _) = synth::world(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    let rule = d.tun.renewal();
    let at = Tick(3 * TICKS_PER_DAY);
    let mut infra = NoInfrastructure;
    for (key, late, placed) in [(1, true, true), (2, false, false)] {
        d.ingest(&mut w, &mut infra, &lair_found(key, late, placed), at);
        let lair = &d.lairs[d.lair_of_key(key).expect("found") as usize];
        assert!(!lair.found_unknown, "{key}");
        assert_eq!(lair.pressure, Pressure::default(), "{key}");
        assert_eq!(
            lair.due,
            at + rule.delay_ticks(Pressure::default(), 0),
            "{key}"
        );
    }
}

/// F12: a row whose lair was held (its dragons the old engine's puppets, lost with it) waits the
/// cap, its pressure handed over: a loss no player made, whose history is gone. A row with nobody
/// home and no wait (its claimer was in flight) waits from its pressure.
#[test]
fn a_held_rows_lost_dragon_waits_the_cap() {
    let (mut w, _) = synth::world(6);
    let mut d = Dragons::new(6, DragonTunables::default(), Tick::ZERO);
    let rule = d.tun.renewal();
    let h = rule.half_life_days;
    let at = Tick(20 * TICKS_PER_DAY);
    let l = found(&mut d, &mut w, 1, true, &row(0.5, 1, None, 48_000), at);
    let lair = &d.lairs[l as usize];
    assert_eq!(lair.due, at + rule.cap_ticks());
    assert!(!lair.found_unknown);
    let want = Pressure::handed_over(0.5, 48_000, day_of(at), h);
    assert_eq!(lair.pressure, want);
    let l = found(&mut d, &mut w, 2, true, &row(0.5, 0, None, 48_000), at);
    assert_eq!(
        d.lairs[l as usize].due,
        at + rule.delay_ticks(want, day_of(at))
    );
}

/// The rows the engine sends (SITE_STATE): every lair marked at a change of its occupants, its
/// vacancy or its pressure, and when it is found again; taken once, as it stands; kept while the
/// replay runs and taken at its end. A known lair found again changes nothing else, whatever its
/// row says.
#[test]
fn the_lairs_rows_are_marked_and_taken_outside_the_replay() {
    let (mut w, _) = synth::world(7);
    let mut d = Dragons::new(7, DragonTunables::default(), Tick::ZERO);
    let rule = d.tun.renewal();
    let h = rule.half_life_days;
    let l = found(&mut d, &mut w, 9, false, &FoundRow::none(), Tick::ZERO);
    let st = d.take_site_states();
    assert_eq!(st.len(), 1);
    let s = &st[0];
    assert_eq!(
        (s.key, s.kind, s.k, s.count, s.away),
        (9, SiteKind::Lair, 2, 0, 0)
    );
    let flags = state_flags::SHORT | state_flags::EMPTY | state_flags::WAITING;
    assert_eq!(s.flags, flags);
    assert_eq!(s.wait_left, rule.delay_ticks(Pressure::default(), 0));
    assert!(s.bodies.is_empty());
    assert!(d.take_site_states().is_empty(), "taken once");
    // A dragon moves in: home, no wait.
    let dragon = d.add_dragon(&mut w, settler(l), Tick(100));
    let st = d.take_site_states();
    assert_eq!(
        (st.len(), st[0].count, st[0].flags, st[0].wait_left),
        (1, 1, 0, 0)
    );
    // A player kills it: pressure 1, the wait from it.
    let player = d.player(&mut w, (1, 2));
    let at = Tick(2 * TICKS_PER_DAY);
    d.slay(&mut w, dragon, Some(player), at);
    let st = d.take_site_states();
    assert_eq!(st.len(), 1);
    assert!((st[0].p - 1.0).abs() < 1e-6);
    assert_eq!(st[0].wait_left, d.lairs[l as usize].due.0 - d.now().0);
    // Found again with a row that says otherwise: only marked.
    let before = d.lairs[l as usize].clone();
    let other = row(3.0, 0, Some(5), 0);
    assert_eq!(found(&mut d, &mut w, 9, true, &other, at + 10), l);
    assert_eq!(d.lairs[l as usize], before);
    assert!(d.lair_site_dirty(l));
    // While the replay runs the mark is kept; its end takes it.
    d.begin_replay(true);
    assert!(d.take_site_states().is_empty());
    assert!(d.lair_site_dirty(l));
    d.replay_done(&mut w, at + 20);
    let st = d.take_site_states();
    assert_eq!(st.len(), 1);
    assert!((st[0].p - before.pressure.now(day_of(at), h)).abs() < 1e-6);
}

fn settler(l: LairId) -> NewDragon {
    NewDragon {
        name: "Settler".into(),
        species: Species::Fire,
        gender: Gender::Male,
        variant: 0,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(l),
        fullness: 80,
        health: None,
        seed: None,
        at: None,
    }
}

/// Note 2 of the build (MZB-1012): the marks are never saved and no decision reads them. An engine
/// restarted from its save with the marks lost runs exactly as the one that kept them (the same
/// commands and state, to the byte), and the found refresh (each lair's LAIR_FOUND, every session)
/// heals the lost marks with the very rows the other engine sends.
#[test]
fn a_restart_that_loses_the_marks_changes_no_decision() {
    let seed = 8;
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let l = found(&mut d, &mut w, 1, false, &FoundRow::none(), Tick::ZERO);
    let l2 = found(
        &mut d,
        &mut w,
        2,
        true,
        &row(0.8, 0, Some(9 * TICKS_PER_DAY), 0),
        Tick(10),
    );
    let dragon = d.add_dragon(&mut w, settler(l), Tick(100));
    let player = d.player(&mut w, (1, 2));
    d.run_until(&mut w, &mut NoInfrastructure, Tick(TICKS_PER_DAY));
    d.slay(&mut w, dragon, Some(player), Tick(TICKS_PER_DAY));
    assert!(d.lair_site_dirty(l) && d.lair_site_dirty(l2));
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut w2 = World::load(WorldConfig::new(seed), goods, book, &w.save()).expect("world");
    let mut d2 = Dragons::load(DragonTunables::default(), &d.save()).expect("dragons");
    assert!(
        !d2.lair_site_dirty(l) && !d2.lair_site_dirty(l2),
        "never saved"
    );
    for day in 2..=120u64 {
        let t = Tick(day * TICKS_PER_DAY);
        d.run_until(&mut w, &mut NoInfrastructure, t);
        d2.run_until(&mut w2, &mut NoInfrastructure, t);
        assert_eq!(d.take_commands(), d2.take_commands(), "day {day}");
        assert_eq!(d.state_hash(), d2.state_hash(), "day {day}");
    }
    assert!(
        d.stats.respawns >= 2,
        "both lairs refilled: {:?}",
        d.stats.respawns
    );
    assert_eq!(d.stats, d2.stats);
    assert_eq!(d.save(), d2.save());
    // The refresh: both lairs found again in the restarted engine's next session.
    let t = d.now();
    for key in [1, 2] {
        found(&mut d2, &mut w2, key, true, &FoundRow::none(), t);
    }
    let mut kept = d.take_site_states();
    kept.retain(|s| s.key == 1 || s.key == 2);
    assert_eq!(d2.take_site_states(), kept);
    assert_eq!(kept.len(), 2);
}

/// MZB-1401 (P3 6): a lair's row whose wait left is past anything an engine sends (a corrupt
/// ledger row) waits the cap, the longest a renewal waits; the tick it ends never wraps.
#[test]
fn a_rows_wait_past_the_cap_is_held_to_the_cap() {
    let (mut w, _) = synth::world(5);
    let mut d = Dragons::new(5, DragonTunables::default(), Tick::ZERO);
    let cap = d.tun.renewal().cap_ticks();
    let at = Tick(50 * TICKS_PER_DAY);
    let l = found(
        &mut d,
        &mut w,
        1,
        true,
        &row(1.0, 0, Some(u64::MAX - 7), 3),
        at,
    );
    assert_eq!(d.lairs[l as usize].due, at + cap);
}
