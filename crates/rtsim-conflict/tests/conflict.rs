// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine's contracts: P14 attribution, grievance catch-up flat in elapsed time,
//! composition (reads never change the trajectory), never better than observed, damage caps,
//! conservation of stolen stock, the de-escalation paths.

use std::collections::BTreeMap;

use proptest::prelude::*;
use rtsim_conflict::boundary::{self, Obs};
use rtsim_conflict::culture::{CultureDef, GOBLIN_JSON, PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, Signal, Target, village_outcome};
use rtsim_conflict::{ConflictTunables, Cultures, Step, synth};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_industry::fake::Visit;
use rtsim_industry::obs::{FightObserved, Obs as IObs, Timed};
use rtsim_industry::threat::ThreatClass;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;

fn setup(seed: u64) -> (World, Conflict, u32) {
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    let f = c
        .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
        .unwrap();
    (w, c, f)
}

/// Goblins that never plunder, so a quiet feud is all there is.
fn quiet_goblins() -> Cultures {
    let mut d: CultureDef = serde_json::from_str(GOBLIN_JSON).unwrap();
    d.plunder_mean_days = None;
    Cultures::from_json(&[&serde_json::to_string(&d).unwrap()]).unwrap()
}

/// P14 as Ben changed it: a signal with no direct actor goes to the player who has spent the most
/// time in its chunk. The refinery's smog is the settlers' while nobody has stood there, becomes
/// the player's who works there, and moves to another player who then out-stays them; the
/// grievance moves with it, from feud to feud, with nothing left behind. A district's source
/// counts the time in all its chunks.
#[test]
fn p14_pollution_goes_to_the_player_who_spends_the_most_time_in_its_chunk() {
    use rtsim_conflict::presence::chunk_of;
    let (mut w, mut c, f) = setup(2);
    let s = c.settlers(&mut w);
    let smog = SignalKind::Smog.index();
    c.set_emission(
        &mut w,
        1,
        SignalKind::Smog,
        synth::REFINERY_POS,
        Some(synth::REFINERY),
        30.0,
        Tick(10),
    );
    assert!(
        c.feud(f, s).unwrap().g.rate[smog] > 0.0,
        "nobody there: the settlers'"
    );
    // The player works at the refinery for two hours, reported every minute.
    let mut t = 20u64;
    while t <= 20 + 144_000 {
        c.observe_presence(&mut w, synth::PLAYER, synth::REFINERY_POS, Tick(t));
        t += 1_200;
    }
    let p = c.player(&mut w, synth::PLAYER);
    assert_eq!(
        c.sources[&(1, SignalKind::Smog)].actor,
        p,
        "the player's now"
    );
    assert!(c.feud(f, p).unwrap().g.rate[smog] > 0.0);
    assert_eq!(
        c.feud(f, s).unwrap().g.rate[smog],
        0.0,
        "the settlers' feud lost it"
    );
    assert_eq!(c.stats.reattributed, 1);
    // Another player out-stays the first (who has left) over the next days.
    let q_uuid = (9, 9);
    while t <= 20 + 144_000 + 600_000 {
        c.observe_presence(&mut w, q_uuid, synth::REFINERY_POS, Tick(t));
        t += 1_200;
    }
    let q = c.player(&mut w, q_uuid);
    assert_eq!(c.sources[&(1, SignalKind::Smog)].actor, q);
    assert!(c.feud(f, q).unwrap().g.rate[smog] > 0.0);
    assert_eq!(c.feud(f, p).unwrap().g.rate[smog], 0.0);
    // A district's source spans its chunks: time anywhere in them counts.
    let far = Pos::new(0, synth::REFINERY_POS.x + 64, synth::REFINERY_POS.z);
    let chunks = [chunk_of(synth::REFINERY_POS), chunk_of(far)];
    let r = (5, 5);
    let mut u = t;
    while u <= t + 1_200_000 {
        c.observe_presence(&mut w, r, far, Tick(u));
        u += 1_200;
    }
    c.set_emission_in(
        &mut w,
        2,
        SignalKind::Smog,
        synth::REFINERY_POS,
        None,
        &chunks,
        10.0,
        Tick(u),
    );
    let rr = c.player(&mut w, r);
    assert_eq!(
        c.sources[&(2, SignalKind::Smog)].actor,
        rr,
        "time in the district's other chunk"
    );
    // A save keeps who owns what and who spent time where.
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
    assert_eq!(back.state_hash(), c.state_hash());
}

#[test]
fn p14_direct_acts_count_per_player_and_unowned_signals_go_to_the_settlers() {
    let (mut w, mut c, f) = setup(1);
    let p = c.player(&mut w, synth::PLAYER);
    let q = c.player(&mut w, (9, 9));
    let s = c.settlers(&mut w);
    c.set_emission(
        &mut w,
        1,
        SignalKind::Smog,
        synth::REFINERY_POS,
        Some(synth::REFINERY),
        30.0,
        Tick(10),
    );
    let sf = &c.feud(f, s).unwrap();
    assert!(
        sf.g.rate[SignalKind::Smog.index()] > 0.0,
        "pollution is the settlers'"
    );
    assert!(c.feud(f, p).is_none());
    let log = |actor, kind, pos| Signal {
        faction: None,
        kind,
        amount: 64.0,
        actor,
        pos,
        district: None,
    };
    c.signal(
        &mut w,
        log(p, SignalKind::Logging, synth::FOREST_POS),
        Tick(20),
    );
    assert!(c.feud(f, p).unwrap().g.level[SignalKind::Logging.index()] > 0.0);
    assert_eq!(
        c.feud(f, s).unwrap().g.level[SignalKind::Logging.index()],
        0.0
    );
    c.signal(
        &mut w,
        log(q, SignalKind::Building, synth::FOREST_POS),
        Tick(25),
    );
    assert!(c.feud(f, q).unwrap().g.level[SignalKind::Building.index()] > 0.0);
    assert_eq!(
        c.feud(f, p).unwrap().g.level[SignalKind::Building.index()],
        0.0
    );
    // An unowned act reported by the plugin, (0, 0): the settlers'.
    let o = Obs::Signal(boundary::TerritorySignal {
        faction: synth::CAMP,
        kind: SignalKind::Mining as u32,
        dimension: 0,
        count: 10,
        player: (0, 0),
        pos: (880, 64, 900),
    });
    c.ingest(&mut w, &mut NoInfrastructure, &o, Tick(30));
    assert!(c.feud(f, s).unwrap().g.level[SignalKind::Mining.index()] > 0.0);
    assert_eq!(
        c.feud(f, p).unwrap().g.level[SignalKind::Mining.index()],
        0.0
    );
    // Far from any territory nothing is held against anyone.
    c.signal(
        &mut w,
        log(p, SignalKind::Mining, Pos::new(0, 9_000, 9_000)),
        Tick(40),
    );
    assert_eq!(
        c.feud(f, p).unwrap().g.level[SignalKind::Mining.index()],
        0.0
    );
}

#[test]
fn grievance_catch_up_is_flat_in_elapsed_time() {
    let mut events = Vec::new();
    for days in [10u64, 1_000, 100_000] {
        let (mut w, _) = synth::world(2);
        let mut c = Conflict::new(2, ConflictTunables::default(), quiet_goblins(), Tick::ZERO);
        synth::define_names(&mut c, &w);
        let f = c
            .add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO)
            .unwrap();
        // Half a unit a day over the tolerance: an equilibrium under any chief's notice.
        c.set_emission(
            &mut w,
            1,
            SignalKind::Smog,
            synth::REFINERY_POS,
            Some(synth::REFINERY),
            4.5,
            Tick::ZERO,
        );
        c.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(days));
        events.push(c.stats.events);
        let s = c.settlers(&mut w);
        let v = c.feud_view(synth::CAMP, s).unwrap();
        assert_eq!(v.step, Step::None);
        if days == 100_000 {
            let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
            let eq = c.feud(f, s).unwrap().g.equilibrium(lambda);
            assert!((v.grievance - eq).abs() < 1e-6, "{} vs {eq}", v.grievance);
        }
    }
    assert!(
        events.windows(2).all(|e| e[0] == e[1]),
        "events per run {events:?} grow with elapsed time"
    );
}

#[test]
fn reads_never_change_the_trajectory() {
    let script = |c: &mut Conflict, w: &mut World, reads: bool| {
        let p = c.player(w, synth::PLAYER);
        let end = Tick::from_days(12);
        let mut t = Tick::ZERO;
        let mut k = 0;
        while t < end {
            let next = t + 9_973;
            if reads {
                for d in 1..4 {
                    c.run_until(w, &mut NoInfrastructure, t + d * 2_000);
                    let s = c.settlers(w);
                    let _ = c.feud_view(synth::CAMP, s);
                    let _ = c.feud_view(synth::CAMP, p);
                }
            }
            c.run_until(w, &mut NoInfrastructure, next);
            if k % 5 == 0 {
                let sig = Signal {
                    faction: None,
                    kind: SignalKind::Logging,
                    amount: 40.0,
                    actor: p,
                    pos: synth::FOREST_POS,
                    district: None,
                };
                c.signal(w, sig, next);
            }
            if k == 3 {
                c.set_emission(
                    w,
                    1,
                    SignalKind::Smog,
                    synth::REFINERY_POS,
                    Some(synth::REFINERY),
                    25.0,
                    next,
                );
            }
            let _ = c.take_commands();
            k += 1;
            t = next;
        }
        c.run_until(w, &mut NoInfrastructure, end);
    };
    let (mut wa, mut a, _) = setup(4);
    let (mut wb, mut b, _) = setup(4);
    script(&mut a, &mut wa, false);
    script(&mut b, &mut wb, true);
    assert!(
        a.stats.escalations.iter().sum::<u64>() > 0,
        "the script climbs the ladder"
    );
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(wa.state_hash(), wb.state_hash());
}

proptest! {
    /// I22 for village raids: the attackers' surviving share is never below the best (lowest)
    /// comparable real outcome, whatever the defence.
    #[test]
    fn village_raids_are_never_better_than_observed(
        strength in 0.5f64..200.0,
        defence in 0.0f64..300.0,
        fights in prop::collection::vec((0.2f64..5.0, 0.0f64..1.0), 0..6),
    ) {
        let observed: Vec<(f64, f64)> = fights.iter().map(|&(k, s)| (strength * k, s)).collect();
        let (survived, floor) = village_outcome(strength, defence, &observed, 2.0);
        prop_assert!((0.0..=1.0).contains(&survived));
        for o in &observed {
            if o.0 >= strength / 2.0 && o.0 <= strength * 2.0 {
                prop_assert!(survived + 1e-12 >= floor.unwrap());
                prop_assert!(floor.unwrap() <= o.1);
            }
        }
    }
}

/// Runs the engine and the industry world together to `to`.
fn run_with(
    c: &mut Conflict,
    w: &mut World,
    fake: &mut rtsim_industry::fake::FakeWorld,
    index: &mut DistrictIndex,
    to: Tick,
) {
    let mut t = c.now();
    while t < to {
        let next = c.next_due().unwrap_or(to).clamp(t + 1, to);
        fake.run_until(next);
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index,
        };
        c.run_until(w, &mut link, next);
        let _ = c.take_commands();
        t = next;
    }
}

#[test]
fn district_raids_are_never_better_than_observed_and_capped() {
    let (mut w, mut c, _) = setup(5);
    let mut fake = synth::refinery(
        5,
        vec![Visit {
            from: Tick::ZERO,
            at: Vec::new(),
        }],
    );
    fake.run_until(Tick(5));
    // A real fight seen against goblin-strength melee: eight of ten got through alive.
    fake.engine.ingest(&Timed {
        at: Tick(6),
        obs: IObs::Fight(FightObserved {
            district: synth::REFINERY,
            class: ThreatClass::GroundMelee,
            attackers: 10,
            attacker_strength: 8.0,
            attackers_killed: 2,
            blocks_destroyed: 0,
            machines_destroyed: 0,
            guards_lost: 0,
            items_stolen: 0,
            duration_ticks: 200,
        }),
    });
    let mut index = DistrictIndex::default();
    c.order_party(
        &mut w,
        synth::CAMP,
        PartyKind::Raid,
        Target::District(synth::REFINERY, synth::REFINERY_POS),
        None,
        Tick(10),
    )
    .expect("a band");
    run_with(&mut c, &mut w, &mut fake, &mut index, Tick::from_days(6));
    let attacks = &fake.engine.district(synth::REFINERY).unwrap().attacks;
    let melee = attacks
        .iter()
        .find(|a| a.class == ThreatClass::GroundMelee && a.skipped.is_none())
        .expect("a melee attack");
    let o = melee.outcome.unwrap();
    assert_eq!(o.floor, Some(0.8));
    assert!(o.survived >= 0.8, "never better than observed: {o:?}");
    // Caps (I23): at most a tenth of each good, the survivors' carry, eight blocks.
    let tun = rtsim_industry::tunables::Tunables::default().threat;
    let start = [
        (synth::IRON, 512),
        (synth::BREAD, 256),
        (synth::GASOLINE, 32),
    ];
    for a in attacks {
        assert!(a.destroyed <= tun.destroy_cap_blocks);
        for (g, n) in start {
            assert!(a.stolen.get(g) * 10 <= n, "{:?}", a.stolen);
        }
        assert!(a.stolen.total() <= i128::from(a.survivors) * 8);
    }
    c.check(&w).expect("books");
    // The band came home: whatever it took is stocked at the haven (a district never loaded has
    // no stock the engine knows of, so here it may be nothing).
    assert!(c.parties.iter().all(|(_, x)| x.kind != PartyKind::Raid));
    assert_eq!(c.ledger.from_players, c.ledger.landed_from_players);
}

#[test]
fn village_raids_move_stock_to_the_haven_exactly_and_capped() {
    let (mut w, mut c, f) = setup(6);
    let ashby = w.node_of_key(4).expect("Ashby");
    let before = w.market(ashby);
    let goods: Vec<GoodId> = w.goods.ids().collect();
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
    let units = c.parties[p].alive();
    let carry: i64 = 8 * MILLI * i64::from(units);
    let share = c
        .cultures
        .get(c.factions[f as usize].culture)
        .def
        .loot_share_milli;
    let mut t = Tick(10);
    let mut checked_held = false;
    while !c.parties.is_empty() {
        t = c.next_due().unwrap_or(t + TICKS_PER_DAY);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        w.check_conserved(t).expect("world conserved at every step");
        c.check(&w).expect("books at every step");
        if !c.held().is_empty() {
            checked_held = true;
        }
    }
    assert!(checked_held, "the band carried loot home");
    let taken = &c.ledger.from_villages;
    assert!(!taken.is_empty());
    let mut total = 0i128;
    for g in goods {
        let t = taken.get(&g).copied().unwrap_or(0);
        total += t;
        assert!(t <= i128::from(before.stock(g)) * i128::from(share) / 1000 + 1);
    }
    assert!(total <= i128::from(carry));
    assert_eq!(&c.ledger.landed, taken, "all of it landed at the haven");
}

#[test]
fn gifts_soften_and_tribute_needs_the_demand() {
    let (mut w, mut c, f) = setup(8);
    let p = c.player(&mut w, synth::PLAYER);
    let sig = Signal {
        faction: Some(synth::CAMP),
        kind: SignalKind::Killing,
        amount: 3.0,
        actor: p,
        pos: synth::FOREST_POS,
        district: None,
    };
    c.signal(&mut w, sig, Tick(100));
    let g0 = c.feud_view(synth::CAMP, p).unwrap().grievance;
    let emerald = w.goods.find("emerald").unwrap();
    let gift: BTreeMap<GoodId, i64> = [(emerald, 10 * MILLI)].into();
    c.gift(&mut w, synth::CAMP, p, &gift, Tick(200));
    let g1 = c.feud_view(synth::CAMP, p).unwrap().grievance;
    assert!(g1 < g0 - 4.0, "a gift of 10 emeralds softens {g0} to {g1}");
    // Without a demand open, a tribute counts as a gift.
    assert!(!c.pay_tribute(&mut w, synth::CAMP, p, None, &gift, Tick(300)));
    assert!(c.feud_view(synth::CAMP, p).unwrap().grievance < g1);
    let haven = c.factions[f as usize].node;
    assert!(w.market(haven).stock(emerald) > 0);
    c.check(&w).expect("books");
}

#[test]
fn killing_the_war_chief_routs_the_band_and_the_heir_takes_over() {
    let (mut w, mut c, f) = setup(9);
    let p = c.player(&mut w, synth::PLAYER);
    let sig = Signal {
        faction: Some(synth::CAMP),
        kind: SignalKind::Killing,
        amount: 20.0,
        actor: p,
        pos: synth::REFINERY_POS,
        district: Some(synth::REFINERY),
    };
    c.signal(&mut w, sig, Tick(10));
    let band = c
        .order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Raid,
            Target::District(synth::REFINERY, synth::REFINERY_POS),
            Some(p),
            Tick(20),
        )
        .unwrap();
    {
        let x = c.feud_mut(f, p).unwrap();
        x.step = Step::Raid;
    }
    assert!(c.parties[band].chief);
    let old = c.factions[f as usize].chief;
    let heir = c.factions[f as usize].heir;
    c.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(1) + 100);
    c.chief_killed(&mut w, band, Some(p), Tick::from_days(1) + 100);
    assert_ne!(c.factions[f as usize].chief, old);
    assert_eq!(c.factions[f as usize].chief, heir, "the heir takes over");
    assert!(w.knowledge.notable(old).is_none(), "the old chief is dead");
    assert!(
        w.knowledge.sentiment(heir, p, w.now()) < 0,
        "and the heir holds the grudge"
    );
    assert!(c.factions[f as usize].cowed_until > Tick::from_days(2));
    assert!(c.feud(f, p).unwrap().step <= Step::Envoy, "two rungs down");
    assert_eq!(c.parties[band].phase, rtsim_conflict::engine::Phase::Home);
    assert_eq!(c.stats.chiefs_killed, 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    /// Conservation over many factions plundering each other's hated neighbours for weeks.
    #[test]
    fn many_factions_conserve_every_good(seed in 0u64..1_000, factions in 2usize..10) {
        let (mut w, mut c) = synth::many(36, factions, seed);
        c.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(40));
        prop_assert!(c.check(&w).is_ok(), "{:?}", c.check(&w));
        let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save()).unwrap();
        prop_assert_eq!(back.state_hash(), c.state_hash());
    }
}
