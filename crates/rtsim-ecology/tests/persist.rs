// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons' save (`Dragons::save`, `Dragons::load`) with the world's (`World::save`,
//! `World::load`): health, memories and grudges survive a restart, and an engine restored at any
//! point continues with the state hashes of one that never stopped.

use proptest::prelude::*;
use rtsim_ecology::boundary::{self, AttackerKind, Cmd, Obs};
use rtsim_ecology::dragon::{LairKind, agent_of};
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

const PLAYER: (u64, u64) = (5, 6);

/// The world and the dragons read back from their saves, as a restarted engine would.
fn restart(w: &World, d: &Dragons, seed: u64) -> (World, Dragons) {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let w2 = World::load(WorldConfig::new(seed), goods, book, &w.save()).expect("the world loads");
    let d2 = Dragons::load(DragonTunables::default(), &d.save()).expect("the dragons load");
    (w2, d2)
}

fn adult(name: &str, lair: u32) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender: Gender::Female,
        variant: 1,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(lair),
        fullness: 100,
        health: None,
        seed: None,
        at: None,
    }
}

/// A wounded dragon's health, its memories of the wound and its grudge against the player are
/// the same after a restart, and it goes on to the same departure, the same hunts and the same
/// grudge as one that never stopped.
#[test]
fn health_memory_and_grudges_survive_a_restart() {
    let seed = 5;
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let lp = Pos::new(0, 4000, 4000);
    let l = d.add_lair(
        &mut w,
        9,
        "Lair",
        Species::Fire,
        LairKind::Roost,
        lp,
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("Scorch", l), Tick(0));
    d.player_stays(&mut w, PLAYER, lp, Tick(100), Tick(150_000));
    d.run_until(&mut w, &mut NoInfrastructure, Tick(110));
    let m = d
        .take_commands()
        .into_iter()
        .find_map(|c| match c {
            Cmd::Materialize(m) if m.agent == agent_of(x) => Some(m),
            _ => None,
        })
        .expect("it materializes at home");
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Hurt(boundary::Hurt {
            agent: m.agent,
            epoch: m.epoch,
            attacker_kind: AttackerKind::Player,
            attacker: PLAYER,
            hits: 4,
            amount: 120.0,
            health_after: 180.0,
        }),
        Tick(500),
    );
    let player = d.player(&mut w, PLAYER);
    let at = Tick(600);
    d.run_until(&mut w, &mut NoInfrastructure, at);
    let xa = d.dragon(x).unwrap().actor;
    let grudge = w.knowledge.sentiment(xa, player, at);
    assert!(
        grudge <= d.tun.grudge_below,
        "the wound is a grudge ({grudge})"
    );
    let memories = w.knowledge.notable(xa).unwrap().memories().to_vec();
    assert!(!memories.is_empty(), "it remembers the wound");
    let health = d.dragon(x).unwrap().health;
    assert!(health < 200.0);

    let (mut w2, mut d2) = restart(&w, &d, seed);
    assert_eq!(d2.state_hash(), d.state_hash());
    assert_eq!(w2.state_hash(), w.state_hash());
    assert_eq!(d2.save(), d.save(), "the dragons read back byte for byte");
    let y = d2.dragon(x).expect("the same handle");
    assert_eq!(y.health.to_bits(), health.to_bits(), "health survives");
    assert_eq!(y.name, "Scorch");
    assert_eq!(
        w2.knowledge.sentiment(xa, player, at),
        grudge,
        "the grudge survives"
    );
    assert_eq!(
        w2.knowledge.notable(xa).unwrap().memories(),
        &memories[..],
        "memories survive"
    );

    // Both go on for 40 days, the plugin capturing a departing dragon at once: the departure,
    // the hunts, the grudge's pull, hour by hour.
    let capture = |w: &mut World, d: &mut Dragons, cmds: &[Cmd], t: Tick| {
        for c in cmds {
            if let Cmd::Depart(dep) = c {
                d.ingest(
                    w,
                    &mut NoInfrastructure,
                    &Obs::Dematerialized(boundary::Dematerialized {
                        agent: dep.agent,
                        epoch: dep.epoch,
                        reason: 1,
                        dimension: 0,
                        health: 180.0,
                        max_health: 500.0,
                        hunger: 60,
                        age_ticks: 0,
                        pos: dep.target,
                    }),
                    t,
                );
            }
        }
    };
    for hour in 1..=(40 * 24) {
        let t = Tick(hour * 1_000);
        if hour == 200 {
            // The player goes home and the lair's chunks unload: the plugin captures the dragon,
            // which is free to hunt again.
            let gone = Obs::Dematerialized(boundary::Dematerialized {
                agent: m.agent,
                epoch: m.epoch,
                reason: 3,
                dimension: 0,
                health: 180.0,
                max_health: 500.0,
                hunger: 40,
                age_ticks: 0,
                pos: (f64::from(lp.x), 100.0, f64::from(lp.z)),
            });
            d.player_left(&mut w, player);
            d2.player_left(&mut w2, player);
            d.ingest(&mut w, &mut NoInfrastructure, &gone, t);
            d2.ingest(&mut w2, &mut NoInfrastructure, &gone, t);
        }
        d.run_until(&mut w, &mut NoInfrastructure, t);
        d2.run_until(&mut w2, &mut NoInfrastructure, t);
        let (c1, c2) = (d.take_commands(), d2.take_commands());
        assert_eq!(c1, c2, "hour {hour}");
        capture(&mut w, &mut d, &c1, t);
        capture(&mut w2, &mut d2, &c2, t);
        assert_eq!(d.state_hash(), d2.state_hash(), "hour {hour}");
        assert_eq!(w.state_hash(), w2.state_hash(), "hour {hour}");
    }
    assert_eq!(d.stats, d2.stats);
    assert!(d.stats.hunts > 0, "it hunted after the restart");
    assert_eq!(d.save(), d2.save());
}

/// A truncated or foreign page is an error, never a panic.
#[test]
fn a_bad_dragons_page_is_an_error() {
    let (mut w, _) = synth::world(3);
    let mut d = Dragons::new(3, DragonTunables::default(), Tick::ZERO);
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    d.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(2));
    let b = d.save();
    for cut in [0, 3, b.len() / 4, b.len() / 2, b.len() - 1] {
        assert!(Dragons::load(DragonTunables::default(), &b[..cut]).is_err());
    }
    let mut more = b.clone();
    more.push(1);
    assert!(Dragons::load(DragonTunables::default(), &more).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// The scenario's dragons restored at any hour of a month continue exactly: the same dragons'
    /// and world's state hashes every day after, and the same commands.
    #[test]
    fn dragons_restored_anywhere_continue_exactly(seed in 0u64..1_000, cut_hour in 1u64..(30 * 24)) {
        let make = || {
            let (mut w, _) = synth::world(seed);
            let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
            synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
            (w, d)
        };
        let (mut wa, mut da) = make();
        let (mut wb, mut db) = make();
        let cut = Tick(cut_hour * 1_000);
        // A player walks a road between two lairs, so dragons materialize and leave.
        let walk = |w: &mut World, d: &mut Dragons, t: Tick| {
            let k = t.0 / 1_000;
            let pos = Pos::new(0, 1000 - (k % 400) as i32 * 5, 1100 - (k % 300) as i32 * 4);
            d.observe_player(w, PLAYER, pos, t);
        };
        let mut t = Tick(0);
        let mut restored = false;
        while t < Tick::from_days(30) {
            t += 1_000;
            if !restored && t >= cut {
                let (w2, d2) = restart(&wb, &db, seed);
                prop_assert_eq!(d2.state_hash(), db.state_hash());
                prop_assert_eq!(w2.state_hash(), wb.state_hash());
                wb = w2;
                db = d2;
                restored = true;
            }
            walk(&mut wa, &mut da, t);
            walk(&mut wb, &mut db, t);
            da.run_until(&mut wa, &mut NoInfrastructure, t);
            db.run_until(&mut wb, &mut NoInfrastructure, t);
            prop_assert_eq!(da.take_commands(), db.take_commands(), "at {:?}", t);
            if t.0.is_multiple_of(TICKS_PER_DAY) {
                prop_assert_eq!(da.state_hash(), db.state_hash(), "at {:?}", t);
                prop_assert_eq!(wa.state_hash(), wb.state_hash(), "at {:?}", t);
            }
        }
        prop_assert_eq!(da.save(), db.save());
        prop_assert_eq!(wa.save(), wb.save());
    }
}

/// Review 3: an untagged spawn-egg dragon reported again after a crash (its DRAGON_ADOPT never
/// reached a world save, so the body comes back untagged) is the dragon the engine already
/// adopted, under a new epoch, never a second one.
#[test]
fn an_untagged_dragon_reported_again_after_a_restart_is_adopted_once() {
    let seed = 8;
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let u = Obs::Untagged(boundary::Untagged {
        entity: (0xE66, 0x2),
        spawn: boundary::SpawnKind::SpawnEgg,
        species: Species::Ice,
        dimension: 0,
        flags: 0,
        age_ticks: (60 * TICKS_PER_DAY) as i32,
        hunger: 70,
        gender: Gender::Male,
        variant: 2,
        home: (0, 0, 0),
        lair: 0,
        pos: (120.0, 80.0, -40.0),
    });
    let adopt = |cmds: Vec<Cmd>| {
        cmds.into_iter()
            .find_map(|c| match c {
                Cmd::Adopt(a) => Some(a),
                _ => None,
            })
            .expect("an adoption")
    };
    d.ingest(&mut w, &mut NoInfrastructure, &u, Tick(100));
    let first = adopt(d.take_commands());
    assert_eq!(d.dragons.len(), 1);

    let (mut w2, mut d2) = restart(&w, &d, seed);
    d2.begin_replay(true);
    d2.replay_done(&mut w2, Tick(5_000));
    d2.ingest(&mut w2, &mut NoInfrastructure, &u, Tick(5_100));
    let again = adopt(d2.take_commands());
    assert_eq!(d2.dragons.len(), 1, "no second dragon");
    assert_eq!(again.agent, first.agent, "the same dragon");
    assert!(again.epoch > first.epoch, "under a new epoch");
    // Reported a third time while embodied in this session: nothing more.
    d2.ingest(&mut w2, &mut NoInfrastructure, &u, Tick(5_200));
    assert!(
        d2.take_commands()
            .iter()
            .all(|c| !matches!(c, Cmd::Adopt(_)))
    );
}
