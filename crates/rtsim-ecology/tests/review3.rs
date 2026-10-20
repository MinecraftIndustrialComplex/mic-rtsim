// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review 3 (2026-10-20, docs/review3-2026-10-20.md): each test failed before its fix.
//!
//! - D1: a roost whose claimant dies on the way never draws another dragon.
//! - D2: a dragon with no lair that settles down jumps to (0, 0).
//! - D3: a grudge raid looks for districts in frame 0, and weighs homes in other dimensions.
//! - D4: a grudge raid that reaches a watched district forgets what it ate at unwatched ones.
//! - D5: a hunger from the plugin above 100 overflows the next meal.

use rtsim_ecology::boundary::{self, AttackerKind, Cmd, Obs, SpawnKind};
use rtsim_ecology::dragon::{DragonId, LairKind, State, dragon_of};
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome};
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::threat::{Threat, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::World;

const PLAYER: (u64, u64) = (5, 6);

fn world(seed: u64) -> (World, Dragons) {
    let (w, _) = synth::world(seed);
    (w, Dragons::new(seed, DragonTunables::default(), Tick::ZERO))
}

fn aggressive_seed() -> u64 {
    (0..)
        .find(|&s| rtsim_knowledge::Temperament::from_seed(s).aggressive >= 200)
        .unwrap()
}

fn adult(name: &str, lair: u32, fullness: i32) -> NewDragon {
    NewDragon {
        name: name.into(),
        species: Species::Fire,
        gender: Gender::Male,
        variant: 0,
        age_ticks: 110 * TICKS_PER_DAY,
        lair: Some(lair),
        fullness,
        health: None,
        seed: Some(aggressive_seed()),
        at: None,
    }
}

/// A spawn-egg dragon at `pos`, adopted as an embodied wanderer; returns it and its epoch.
fn adopt_wanderer(
    w: &mut World,
    d: &mut Dragons,
    pos: (f64, f64, f64),
    hunger: i32,
    at: Tick,
) -> (DragonId, u32) {
    d.ingest(
        w,
        &mut NoInfrastructure,
        &Obs::Untagged(boundary::Untagged {
            entity: (0xE66, 0x1),
            spawn: SpawnKind::SpawnEgg,
            species: Species::Fire,
            dimension: 0,
            flags: 0,
            age_ticks: (110 * TICKS_PER_DAY) as i32,
            hunger,
            gender: Gender::Female,
            variant: 0,
            home: (0, 0, 0),
            lair: 0,
            pos,
        }),
        at,
    );
    let adopt = d
        .take_commands()
        .into_iter()
        .find_map(|c| match c {
            Cmd::Adopt(a) => Some(a),
            _ => None,
        })
        .expect("adopted");
    (dragon_of(adopt.agent).unwrap(), adopt.epoch)
}

// ---- D1 ----------------------------------------------------------------------------------------

/// The respawn rule (P13's recommendation) books a newcomer for an empty roost and marks the roost
/// claimed. When the newcomer dies on the way, the roost must stand empty again and draw another
/// after the delay; before the fix nothing ever came.
#[test]
fn a_roost_whose_claimant_dies_on_the_way_draws_another() {
    let (mut w, mut d) = world(21);
    // The newcomer flies 20,000 blocks (40,000 ticks) with a day to live.
    d.tun.respawn_from_blocks = 20_000;
    d.tun.species[Species::Fire.index()].lifespan_days = d.tun.respawn_age_days;
    d.add_lair(
        &mut w,
        9,
        "Far",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 60_000, 60_000),
        100,
        Tick(0),
    );
    let delay = d.tun.respawn_delay_days * TICKS_PER_DAY;
    d.run_until(&mut w, &mut NoInfrastructure, Tick(delay + 1));
    assert_eq!(d.stats.respawns, 1);
    d.run_until(
        &mut w,
        &mut NoInfrastructure,
        Tick(delay + 2 * TICKS_PER_DAY),
    );
    assert_eq!(d.stats.deaths_age, 1, "it died of age on the way");
    assert_eq!(d.dragons.len(), 0);
    assert_eq!(d.stats.claims, 0);
    d.run_until(
        &mut w,
        &mut NoInfrastructure,
        Tick(2 * delay + 4 * TICKS_PER_DAY),
    );
    assert_eq!(
        d.stats.respawns, 2,
        "the roost stood empty for another delay and drew a second newcomer"
    );
}

/// The same for a claim an admin's relocation abandons: the first roost is free again.
#[test]
fn a_relocated_claimant_frees_the_roost_it_was_flying_to() {
    let (mut w, mut d) = world(23);
    let home = d.add_lair(
        &mut w,
        1,
        "Home",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 40_000, 40_000),
        100,
        Tick(0),
    );
    let a = d.add_lair(
        &mut w,
        2,
        "A",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 50_000, 40_000),
        100,
        Tick(0),
    );
    let b = d.add_lair(
        &mut w,
        3,
        "B",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, 40_000, 50_000),
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("x", home, 100), Tick(0));
    d.run_until(&mut w, &mut NoInfrastructure, Tick(100));
    d.relocate(&mut w, x, a, Tick(100));
    d.run_until(&mut w, &mut NoInfrastructure, Tick(200));
    d.relocate(&mut w, x, b, Tick(200));
    assert!(
        d.lairs[a as usize].vacant_since.is_some(),
        "the roost it no longer flies to stands empty"
    );
    let delay = d.tun.respawn_delay_days * TICKS_PER_DAY;
    d.run_until(
        &mut w,
        &mut NoInfrastructure,
        Tick(200 + delay + TICKS_PER_DAY),
    );
    assert!(
        d.by_name("A newcomer").is_some(),
        "and draws a newcomer in time"
    );
}

// ---- D2 ----------------------------------------------------------------------------------------

/// A dragon with no lair (a spawn egg's, adopted as a wanderer) is captured where it is, flies
/// "home" to where it rests, and settles there. Before the fix settling dropped its resting
/// place, so it stood at (0, 0) of frame 0 and hunted around the world's origin.
#[test]
fn a_lairless_dragon_settles_where_it_landed() {
    let (mut w, mut d) = world(22);
    let (x, epoch) = adopt_wanderer(&mut w, &mut d, (7_000.0, 90.0, -7_000.0), 100, Tick(10));
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Dematerialized(boundary::Dematerialized {
            agent: rtsim_ecology::dragon::agent_of(x),
            epoch,
            reason: 3,
            dimension: 0,
            health: 400.0,
            max_health: 500.0,
            hunger: 100,
            age_ticks: (110 * TICKS_PER_DAY) as i32,
            pos: (7_000.0, 90.0, -7_000.0),
        }),
        Tick(100),
    );
    d.run_until(&mut w, &mut NoInfrastructure, Tick(1_000));
    let dr = d.dragon(x).unwrap();
    assert!(matches!(dr.state, State::Roosting), "{:?}", dr.state);
    let (p, _) = dr.position(&d.lairs, Tick(1_000));
    assert_eq!((p.frame, p.x, p.z), (0, 7_000, -7_000));
}

// ---- D3, D4 ------------------------------------------------------------------------------------

/// Infrastructure that records the frames it is asked about and hands out fixed sites.
#[derive(Default)]
struct Sites {
    frames: Vec<u16>,
    sites: Vec<DistrictSite>,
    raided: Vec<DistrictId>,
}

impl Infrastructure for Sites {
    fn districts_near(&mut self, frame: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        self.frames.push(frame);
        self.sites
            .iter()
            .copied()
            .filter(|s| s.pos.frame == frame)
            .collect()
    }

    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome> {
        let ThreatTarget::District(id) = t.target else {
            return None;
        };
        self.raided.push(id);
        Some(RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: true,
            stolen: GoodVec::from_pairs([(Good(1), 20)]),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }

    fn good_name(&self, g: Good) -> Option<String> {
        (g == Good(1)).then(|| "minecraft:beef".to_string())
    }
}

/// An aggressive dragon in a lair of `lair_frame` hates the player, who lives in `home_frame` for
/// most of a day and leaves; the dragon gets hungry after that. Runs to tick 80,000.
fn grudge(lair_frame: u16, home_frame: u16, infra: &mut Sites) -> (Dragons, DragonId) {
    let (mut w, mut d) = world(13);
    let l = d.add_lair(
        &mut w,
        4,
        "L",
        Species::Fire,
        LairKind::Roost,
        Pos::new(lair_frame, 1800, 2500),
        100,
        Tick(0),
    );
    let x = d.add_dragon(&mut w, adult("x", l, 70), Tick(0));
    let player = d.player(&mut w, PLAYER);
    let xa = d.dragon(x).unwrap().actor;
    w.knowledge.adjust(xa, player, -127, Tick(0));
    let farm = Pos::new(home_frame, 302, 1301);
    let mut t = Tick(0);
    while t.0 < 30_000 {
        d.observe_player(&mut w, PLAYER, farm, t);
        d.run_until(&mut w, infra, t);
        t += 1_200;
    }
    d.player_left(&mut w, player);
    while t.0 < 80_000 {
        t += 1_000;
        d.run_until(&mut w, infra, t);
    }
    (d, x)
}

/// The home's dimension travels with the grudge: the raid asks for districts in it.
#[test]
fn a_grudge_raid_looks_for_districts_in_the_homes_dimension() {
    let mut infra = Sites {
        sites: vec![DistrictSite {
            id: DistrictId(7),
            pos: Pos::new(3, 310, 1300),
            watched: false,
            loaded: false,
        }],
        ..Sites::default()
    };
    let (d, _) = grudge(3, 3, &mut infra);
    assert!(!infra.frames.is_empty(), "a grudge raid went out");
    assert!(
        infra.frames.iter().all(|&f| f == 3),
        "districts looked up in frame 3 only: {:?}",
        infra.frames
    );
    assert_eq!(infra.raided, vec![DistrictId(7)]);
    assert_eq!(d.stats.district_raids, 1);
}

/// A home in another dimension is out of reach, however close its coordinates.
#[test]
fn a_home_in_another_dimension_is_not_raided() {
    let mut infra = Sites::default();
    let (d, _) = grudge(3, 4, &mut infra);
    assert!(
        infra.frames.is_empty(),
        "no raid on a home in another dimension: {:?}",
        infra.frames
    );
    assert_eq!(d.stats.district_raids, 0);
}

/// Raided at an unwatched district, then meeting a watched one, the dragon comes in person; what
/// it ate at the first stays eaten (the ledger has it) and fills it.
#[test]
fn a_raid_that_ends_in_a_real_fight_keeps_what_it_ate() {
    let mut infra = Sites {
        sites: vec![
            DistrictSite {
                id: DistrictId(1),
                pos: Pos::new(0, 310, 1300),
                watched: false,
                loaded: false,
            },
            DistrictSite {
                id: DistrictId(2),
                pos: Pos::new(0, 320, 1310),
                watched: true,
                loaded: true,
            },
        ],
        ..Sites::default()
    };
    let (mut d, x) = grudge(0, 0, &mut infra);
    assert_eq!(infra.raided, vec![DistrictId(1)]);
    let mats: Vec<boundary::Materialize> = d
        .take_commands()
        .into_iter()
        .filter_map(|c| match c {
            Cmd::Materialize(m) => Some(m),
            _ => None,
        })
        .collect();
    let m = mats
        .last()
        .expect("it came in person to the watched district");
    // 20 beef at 2 points each went into the ledger as eaten.
    let eaten: i128 = d.ledger.eaten.values().sum();
    assert_eq!(eaten, 20);
    // It was hungry (below 60) when it left; 40 points of beef put it well above.
    assert!(
        m.hunger >= 80,
        "the meal fills it: hunger {} at materialization",
        m.hunger
    );
    let _ = x;
}

// ---- D5 ----------------------------------------------------------------------------------------

/// The plugin reports Ice and Fire's `Hunger` (0 to 100). A value out of range must not break
/// the engine: before the fix the next meal overflowed (a panic with overflow checks).
#[test]
fn hunger_out_of_range_from_the_plugin_is_clamped() {
    let (mut w, mut d) = world(24);
    let ids = synth::goods().1;
    d.define_herd_body(40, ids.cattle);
    let (x, epoch) = adopt_wanderer(&mut w, &mut d, (700.0, 70.0, 0.0), 50, Tick(10));
    let agent = rtsim_ecology::dragon::agent_of(x);
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::State(boundary::State {
            agent,
            epoch,
            flags: 8,
            dimension: 0,
            health: 400.0,
            max_health: 500.0,
            hunger: i32::MAX,
            age_ticks: (110 * TICKS_PER_DAY) as i32,
            pos: (700.0, 70.0, 0.0),
        }),
        Tick(20),
    );
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::Ate(boundary::Ate {
            agent,
            epoch,
            prey: 40,
            count: 2,
            village: 2,
            pos: (700.0, 70.0, 0.0),
        }),
        Tick(30),
    );
    let f = d.dragon(x).unwrap().fullness_at(&d.tun, Tick(30));
    assert!((0..=d.tun.full).contains(&f), "fullness {f}");
    let _ = AttackerKind::Player;
}
