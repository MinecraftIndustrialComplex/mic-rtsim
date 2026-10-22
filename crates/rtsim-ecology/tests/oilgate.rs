// SPDX-License-Identifier: AGPL-3.0-or-later
//! The oil gate, the dragon half (OILGATE.md §4.1 to §4.5; docs/choices-oilgate.md).
//!
//! - **Refuel and theft** (OG-5, OG-6): a low fire dragon flies to the nearest field, raids the
//!   tanks of the district beside it through the industry engine, drinks the fuel and rests; the
//!   fuel it drank is burnt in its ledger and stolen in the industry's books, and the two agree.
//! - **Sensing** (OG-8): drinking beside a district that drills is an Encroachment against it,
//!   and the grudge sends a District raid; a watched district's tanks draw the dragon in person.
//! - **The dry fight** (OG-13): a dragon with no oil raids at `dry_strength_milli` of its
//!   strength.
//! - **Field flights** (OG-3): a player near a field and high enough draws hostile fire dragons
//!   under the caps; they breathe (OG-4), and leave the sim when done.
//! - **Lairs near oil** (OG-7, OG-2): a field with no fire lair in reach gets one placed, once;
//!   the new lair asks for a survey, and the survey's chunks join the index.
//! - **Paging and saves**: on the bench's world with oil, field flights, tanks and a walking
//!   player, the paged engine equals the resident one, and an engine restored mid-run from its
//!   pages (and one from its resident save) continues exactly.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rtsim_ecology::boundary::{self, Cmd, Obs, OilChunkRec, OilChunks};
use rtsim_ecology::dragon::{LairKind, dragon_of};
use rtsim_ecology::engine::{Dragons, NewDragon};
use rtsim_ecology::infra::{
    DistrictIndex, DistrictSignals, DistrictSite, IndustryLink, Infrastructure, NoInfrastructure,
    RaidOutcome,
};
use rtsim_ecology::prey::Item;
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::synth::{self, FakeIaf, Reaction};
use rtsim_ecology::tunables::DragonTunables;
use rtsim_industry::ids::{DistrictId, Good, GoodVec};
use rtsim_industry::threat::{Threat, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

const PLAYER: (u64, u64) = (5, 6);
const BEEF: Good = Good(1);
const DIESEL: Good = Good(4);

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

/// The reserve falls a point every `tick_rate` ticks: 300 makes a full dragon low (40) in 18,000.
fn quick_oil(tick_rate: u64) -> DragonTunables {
    let mut tun = DragonTunables::default();
    tun.oil.tick_rate = tick_rate;
    tun
}

/// A square field of `side` by `side` chunks from chunk (cx, cz), reported as Deep Time's traps.
fn field(cx: i32, cz: i32, side: i32) -> Obs {
    Obs::OilChunks(OilChunks {
        dimension: 0,
        source: boundary::OIL_SOURCE_DEEP_TIME,
        chunks: (0..side * side)
            .map(|k| OilChunkRec {
                cx: cx + k % side,
                cz: cz + k / side,
                amount: 1,
                flags: 0,
            })
            .collect(),
    })
}

fn fire_lair(w: &mut World, d: &mut Dragons, key: u64, x: i32, z: i32) -> u32 {
    d.add_lair(
        w,
        key,
        "L",
        Species::Fire,
        LairKind::Roost,
        Pos::new(0, x, z),
        100,
        Tick(0),
    )
}

fn materializations(cmds: &[Cmd]) -> Vec<boundary::Materialize> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Materialize(m) => Some(*m),
            _ => None,
        })
        .collect()
}

/// Districts with tanks, as a test infrastructure: fixed sites, fixed signals, and a raid that
/// takes 20 beef and drinks up to the room from `fuel_mb` (a bottomless tank: every raid is a
/// pure function of its threat). Records each raid's target, strength and room.
#[derive(Clone, Default)]
struct Tanks {
    sites: Vec<DistrictSite>,
    signals: DistrictSignals,
    raids: Vec<(DistrictId, f64, i64)>,
}

impl Tanks {
    fn at(sites: &[(u64, i32, i32, bool)], signals: DistrictSignals) -> Tanks {
        Tanks {
            sites: sites
                .iter()
                .map(|&(id, x, z, watched)| DistrictSite {
                    id: DistrictId(id),
                    pos: Pos::new(0, x, z),
                    watched,
                    loaded: watched,
                })
                .collect(),
            signals,
            raids: Vec::new(),
        }
    }
}

impl Infrastructure for Tanks {
    fn districts_near(&mut self, frame: u16, x: i64, z: i64, r: i64) -> Vec<DistrictSite> {
        self.sites
            .iter()
            .copied()
            .filter(|s| {
                let (dx, dz) = (i64::from(s.pos.x) - x, i64::from(s.pos.z) - z);
                s.pos.frame == frame && dx * dx + dz * dz <= r * r
            })
            .collect()
    }

    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome> {
        self.raid_drinking(t, &[], 0)
    }

    fn raid_drinking(&mut self, t: &Threat, fuels: &[String], room: i64) -> Option<RaidOutcome> {
        let ThreatTarget::District(id) = t.target else {
            return None;
        };
        self.raids.push((id, t.strength, room));
        let mut stolen = vec![(BEEF, 20)];
        let fuel = self.signals.fuel_mb.min(room);
        if fuel > 0 && !fuels.is_empty() {
            stolen.push((DIESEL, fuel));
        }
        Some(RaidOutcome {
            survived: 1.0,
            floor: None,
            untested: true,
            stolen: GoodVec::from_pairs(stolen),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }

    fn signals(&mut self, _: DistrictId, _: &[String]) -> DistrictSignals {
        self.signals
    }

    fn good_name(&self, g: Good) -> Option<String> {
        match g {
            BEEF => Some("minecraft:beef".into()),
            DIESEL => Some("createdieselgenerators:diesel".into()),
            _ => None,
        }
    }
}

// ---- OG-5, OG-6: refuel and theft through the industry engine ----------------------------------

/// A player at the farm for its first 2,000 ticks (the engine learns its stock), gone after.
fn visited() -> Vec<rtsim_industry::fake::Visit> {
    use rtsim_industry::fake::Visit;
    vec![
        Visit {
            from: Tick(0),
            at: vec![0],
        },
        Visit {
            from: Tick(2_000),
            at: vec![],
        },
    ]
}

/// A fire dragon goes low 18,000 ticks in, flies to the field beside the farm, raids its tank
/// (unwatched) and drinks; the diesel it drank is burnt in its ledger and stolen in the industry
/// engine's books, and they agree with everything else it took.
#[test]
fn a_low_dragon_drinks_from_the_tanks_and_the_books_balance() {
    let tank_mb = 40_000;
    let mut fake = synth::oil_farm(13, [0.0; 4], visited(), tank_mb);
    fake.run_until(Tick(1));
    let (mut w, _) = synth::world(13);
    let mut d = Dragons::new(13, quick_oil(300), Tick::ZERO);
    let l = fire_lair(&mut w, &mut d, 4, 1800, 2500);
    let x = d.add_dragon(&mut w, adult("x", l, 100), Tick(0));
    // A field of nine chunks just east of the farm (300, 1300): blocks 336 to 383, 1296 to 1343.
    d.ingest(&mut w, &mut NoInfrastructure, &field(21, 81, 3), Tick(0));
    assert!(d.oil_gated(x));
    assert_eq!(d.oil.chunks(), 9);
    assert_eq!(d.oil.fields(), 1);
    let mut index = DistrictIndex::default();
    let mut t = Tick(0);
    let mut lowest = i32::MAX;
    while t.0 < 60_000 {
        t += 250;
        fake.run_until(t);
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index: &mut index,
        };
        d.run_until(&mut w, &mut link, t);
        lowest = lowest.min(d.oil_level(x, t).unwrap());
    }
    let s = d.stats;
    assert!(s.oil_raids >= 1, "it drank from the tank: {s:?}");
    assert!(s.refuels >= 1 && s.encroachments >= 1, "{s:?}");
    assert!(
        lowest <= d.tun.oil.refuel_below && lowest > 0,
        "it went low ({lowest}) and drank before it ran dry"
    );
    let stolen = &fake.engine.books.stolen;
    let drunk = stolen.get(DIESEL);
    assert!(drunk > 0 && drunk <= tank_mb, "{drunk} mB");
    assert_eq!(drunk as u64, s.oil_drunk_mb);
    assert_eq!(
        d.ledger.burnt.get(&Item::Industry(DIESEL)).copied(),
        Some(i128::from(drunk)),
        "the fuel it drank is burnt in its books"
    );
    assert_eq!(d.ledger.eaten.get(&Item::Industry(DIESEL)), None);
    assert_eq!(d.ledger.hoarded.get(&Item::Industry(DIESEL)), None);
    d.check(&w, Some(stolen)).expect("the books balance");
    // The oil in the ground is nobody's (OG-6): drinking left the index as it was.
    assert_eq!(d.oil.chunks(), 9);
    assert!(d.oil_level(x, t).unwrap() > d.tun.oil.refuel_below);
}

// ---- OG-8: sensing, Encroachment and the District grudge ---------------------------------------

/// A dragon that drinks beside a district with an oil sample and pollution senses it: an
/// Encroachment report it witnesses, and a grudge against the district that sends it back on a
/// District raid when it next hunts.
#[test]
fn a_drilling_district_draws_a_grudge_raid() {
    let (mut w, _) = synth::world(13);
    let mut d = Dragons::new(13, quick_oil(300), Tick::ZERO);
    let l = fire_lair(&mut w, &mut d, 4, 1800, 2500);
    let x = d.add_dragon(&mut w, adult("x", l, 100), Tick(0));
    d.ingest(&mut w, &mut NoInfrastructure, &field(21, 81, 3), Tick(0));
    let mut tanks = Tanks::at(
        &[(7, 310, 1300, false)],
        DistrictSignals {
            oil_sample: true,
            polluted: true,
            fuel_mb: 0,
        },
    );
    // Hungry about five days in (fullness 100 down to the hunt threshold).
    let mut t = Tick(0);
    while t.0 < 8 * TICKS_PER_DAY {
        t += 500;
        d.run_until(&mut w, &mut tanks, t);
    }
    let s = d.stats;
    assert!(s.encroachments >= 1, "{s:?}");
    assert!(
        s.district_target_raids >= 1,
        "the grudge sent a District raid: {s:?}"
    );
    assert!(tanks.raids.iter().all(|r| r.0 == DistrictId(7)));
    assert!(!tanks.raids.is_empty());
    // An empty tank is nothing to drink: no fuel was taken.
    assert_eq!(s.oil_drunk_mb, 0);
    d.check(&w, None).unwrap();
    let me = d.dragon(x).unwrap().actor;
    let worst = w
        .knowledge
        .sentiment_snapshot(me, t)
        .into_iter()
        .map(|(_, v)| v)
        .min()
        .unwrap();
    assert!(worst <= d.tun.grudge_below, "sentiment {worst}");
}

/// A watched district's tanks draw a low dragon that arrives unseen in person, carrying its
/// reserve, the day it is first sensed. The player stands south-west of the district: within two
/// view radii (320 blocks) of it, but beyond view (160) of the field's north-east chunk (376,
/// 1336), where the dragon from the north-east lands. The player lives there (P26's home
/// counter), so the Encroachment is theirs too: the dragon comes hostile, at them.
#[test]
fn a_watched_tank_draws_the_dragon_in_person() {
    let (mut w, _) = synth::world(13);
    let mut d = Dragons::new(13, quick_oil(300), Tick::ZERO);
    let l = fire_lair(&mut w, &mut d, 4, 1800, 2500);
    let x = d.add_dragon(&mut w, adult("x", l, 100), Tick(0));
    d.ingest(&mut w, &mut NoInfrastructure, &field(21, 81, 3), Tick(0));
    let mut tanks = Tanks::at(
        &[(7, 310, 1300, true)],
        DistrictSignals {
            oil_sample: false,
            polluted: false,
            fuel_mb: 50_000,
        },
    );
    let mut cmds = Vec::new();
    let mut t = Tick(0);
    while t.0 < 24_000 {
        t += 200;
        d.observe_player(&mut w, PLAYER, Pos::new(0, 100, 1150), t);
        d.run_until(&mut w, &mut tanks, t);
        cmds.extend(d.take_commands());
    }
    let ms: Vec<_> = materializations(&cmds)
        .into_iter()
        .filter(|m| dragon_of(m.agent) == Some(x))
        .collect();
    let m = ms.first().expect("it came to the tanks in person");
    assert!(m.flags & boundary::MAT_OIL != 0, "{m:?}");
    assert!(m.oil <= d.tun.oil.refuel_below, "low: {}", m.oil);
    assert!(
        (m.pos.0 - 310.0).abs() < 1.0 && (m.pos.2 - 1300.0).abs() < 1.0,
        "at the tanks: {:?}",
        m.pos
    );
    assert_eq!(d.stats.encroachments, 1);
    assert!(m.flags & boundary::MAT_HOSTILE != 0, "{m:?}");
    assert_eq!(m.target_player, PLAYER);
    // In person, nothing was raided abstractly.
    assert!(tanks.raids.is_empty(), "{:?}", tanks.raids);
}

// ---- OG-13: the dry fight ----------------------------------------------------------------------

/// An aggressive dragon that hates the player raids the district at the player's home once it
/// is hungry. Returns the raids' strengths and the dry raids counted.
fn grudge_strengths(oil_far_away: bool) -> (Vec<f64>, u64) {
    let (mut w, _) = synth::world(13);
    // A point a tick: dry 100 ticks in.
    let mut d = Dragons::new(13, quick_oil(1), Tick::ZERO);
    if oil_far_away {
        // Oil known, but out of reach of the lair (6,000 blocks): the gate applies, no refuel.
        d.ingest(&mut w, &mut NoInfrastructure, &field(400, 400, 2), Tick(0));
    }
    let l = fire_lair(&mut w, &mut d, 4, 1800, 2500);
    let x = d.add_dragon(&mut w, adult("x", l, 70), Tick(0));
    assert_eq!(d.oil_gated(x), oil_far_away);
    let player = d.player(&mut w, PLAYER);
    let xa = d.dragon(x).unwrap().actor;
    w.knowledge.adjust(xa, player, -127, Tick(0));
    let mut tanks = Tanks::at(&[(7, 310, 1300, false)], DistrictSignals::default());
    let farm = Pos::new(0, 302, 1301);
    let mut t = Tick(0);
    while t.0 < 30_000 {
        d.observe_player(&mut w, PLAYER, farm, t);
        d.run_until(&mut w, &mut tanks, t);
        t += 1_200;
    }
    d.player_left(&mut w, player);
    while t.0 < 80_000 {
        t += 1_000;
        d.run_until(&mut w, &mut tanks, t);
    }
    d.check(&w, None).unwrap();
    (tanks.raids.iter().map(|r| r.1).collect(), d.stats.dry_raids)
}

#[test]
fn a_dry_dragon_raids_with_bite_and_claws_only() {
    let (wet, wet_dry) = grudge_strengths(false);
    let (dry, dry_dry) = grudge_strengths(true);
    assert!(!wet.is_empty() && !dry.is_empty(), "both raided");
    assert_eq!(wet_dry, 0);
    assert!(dry_dry >= 1);
    let share = f64::from(DragonTunables::default().oil.dry_strength_milli) / 1000.0;
    assert!(
        (dry[0] - wet[0] * share).abs() < 1e-9,
        "dry {} against {} at {share}",
        dry[0],
        wet[0]
    );
}

// ---- OG-3, OG-4: field flights -----------------------------------------------------------------

struct Flights {
    w: World,
    d: Dragons,
    iaf: FakeIaf,
}

fn flights(seed: u64, flags: u32) -> Flights {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::OilConfig(boundary::OilConfig { flags }),
        Tick(0),
    );
    // Near Ashpeak Roost (1000, 1100): a fire lair in reach, so nothing is placed.
    d.ingest(&mut w, &mut NoInfrastructure, &field(100, 100, 3), Tick(0));
    let mut iaf = FakeIaf::new(PLAYER);
    iaf.breaths = 7;
    iaf.reaction = Reaction::Ignore;
    Flights { w, d, iaf }
}

/// One second: the player's record with its height, the engine, the plugin's answers.
fn second(f: &mut Flights, pos: Option<Pos>, t: Tick) -> Vec<Cmd> {
    if let Some(p) = pos {
        // The host books the player's interest area and reports where it is, with its height.
        f.d.observe_player(&mut f.w, PLAYER, p, t);
        f.d.player_at(&mut f.w, PLAYER, p, 70, t);
    }
    f.d.run_until(&mut f.w, &mut NoInfrastructure, t);
    let mut cmds = f.d.take_commands();
    f.iaf.apply(cmds.clone(), t);
    for (at, o) in f.iaf.due(t) {
        let at = at.max(f.d.now());
        f.d.ingest(&mut f.w, &mut NoInfrastructure, &o, at);
    }
    let more = f.d.take_commands();
    f.iaf.apply(more.clone(), t);
    cmds.extend(more);
    cmds
}

#[test]
fn a_player_near_a_field_draws_hostile_visitors_under_the_caps() {
    let mut f = flights(5, boundary::OIL_FIELD_FLIGHTS | boundary::OIL_PLACE_LAIRS);
    let cap = f.d.tun.oil.flight_cap_field;
    let dragons0 = f.d.dragons.len();
    let here = Pos::new(0, 1620, 1620);
    let mut cmds = Vec::new();
    let mut t = Tick(0);
    let mut most = 0;
    while t.0 < 6_000 {
        t += 20;
        cmds.extend(second(&mut f, Some(here), t));
        let n = f.d.field_visitors(0);
        assert!(n <= cap, "{n} visitors at {t:?}");
        most = most.max(n);
    }
    let s = f.d.stats;
    assert!(s.field_rolls > 0 && s.field_draws > 0, "{s:?}");
    assert_eq!(most, cap, "the field filled to its cap");
    assert_eq!(s.lairs_placed, 0, "Ashpeak Roost is in reach");
    let visitors: Vec<_> = materializations(&cmds)
        .into_iter()
        .filter(|m| {
            dragon_of(m.agent)
                .and_then(|x| f.d.dragon(x))
                .is_some_and(|x| x.visitor.is_some())
        })
        .collect();
    assert!(!visitors.is_empty(), "visitors came into view");
    for m in &visitors {
        assert!(m.flags & boundary::MAT_HOSTILE != 0 && m.flags & boundary::MAT_OIL != 0);
        assert_eq!(m.target_player, PLAYER);
        assert_eq!(m.species, Species::Fire);
    }
    // Breathing cost them oil (OG-4).
    assert!(s.breaths > 0);
    let burnt = visitors.iter().any(|m| {
        dragon_of(m.agent)
            .and_then(|x| f.d.oil_level(x, t))
            .is_some_and(|o| o <= f.d.tun.oil.capacity - 7)
    });
    assert!(burnt, "a visitor's reserve shows its breaths");
    // The player goes: the rolls stop, the bodies are captured, and the visitors finish and leave
    // the sim (they carry nothing).
    let p = f.d.player(&mut f.w, PLAYER);
    f.d.player_left(&mut f.w, p);
    f.iaf.player_leaves(t);
    while t.0 < 8_000 {
        t += 20;
        second(&mut f, None, t);
    }
    // Its presence went stale: the process stopped.
    let rolls = f.d.stats.field_rolls;
    while t.0 < 8_000 + 3 * TICKS_PER_DAY {
        t += 500;
        second(&mut f, None, t);
    }
    let s = f.d.stats;
    assert_eq!(s.field_rolls, rolls, "the process stopped: {s:?}");
    assert_eq!(f.d.field_visitors(0), 0);
    assert!(s.visitors_left > 0, "{s:?}");
    assert!(
        f.d.dragons.iter().all(|(_, x)| x.visitor.is_none()),
        "every visitor is done"
    );
    assert!(f.d.dragons.len() >= dragons0 - 1);
    f.d.check(&f.w, None).unwrap();
}

/// The switch is the plugin's (OIL_CONFIG): off, a player near a field draws nothing; a player
/// below `flight_min_y` (deep in a mine) draws nothing either.
#[test]
fn field_flights_follow_the_switch_and_the_height() {
    let mut off = flights(5, 0);
    let mut low = flights(5, boundary::OIL_FIELD_FLIGHTS);
    let here = Pos::new(0, 1620, 1620);
    let mut t = Tick(0);
    while t.0 < 3_000 {
        t += 20;
        second(&mut off, Some(here), t);
        low.d.player_at(&mut low.w, PLAYER, here, 10, t);
        low.d.run_until(&mut low.w, &mut NoInfrastructure, t);
    }
    assert_eq!(off.d.stats.field_draws, 0);
    assert_eq!(off.d.stats.field_rolls, 0);
    assert_eq!(low.d.stats.field_draws, 0);
}

/// Field flights are deterministic: the same seed draws the same visitors and commands.
#[test]
fn field_flights_are_deterministic() {
    let run = |seed| {
        let mut f = flights(seed, boundary::OIL_FIELD_FLIGHTS);
        let mut cmds = Vec::new();
        let mut t = Tick(0);
        while t.0 < 4_000 {
            t += 20;
            cmds.extend(second(&mut f, Some(Pos::new(0, 1620, 1620)), t));
        }
        (cmds, f.d.state_hash(), f.d.stats)
    };
    let (a, b) = (run(5), run(5));
    assert_eq!(a.0, b.0);
    assert_eq!(a.1, b.1);
    assert_eq!(a.2, b.2);
    assert!(a.2.field_draws > 0);
    assert_ne!(a.1, run(6).1, "another seed, another draw");
}

// ---- OG-7, OG-2: lairs near oil and surveys ----------------------------------------------------

#[test]
fn a_field_with_no_fire_lair_gets_one_and_its_survey_grows_the_index() {
    let mut f = flights(5, boundary::OIL_PLACE_LAIRS);
    // A field far from every lair, with more oil around it that only CDG's sampler would find.
    f.d.ingest(
        &mut f.w,
        &mut NoInfrastructure,
        &field(-250, 250, 2),
        Tick(0),
    );
    for (cx, cz) in [(-240, 250), (-240, 251), (-260, 262), (-200, 300)] {
        f.iaf.ground_oil.insert((0, cx, cz), cx == -200);
    }
    let chunks0 = f.d.oil.chunks();
    let lairs0 = f.d.lairs.len();
    let here = Pos::new(0, -3990, 4010);
    let mut cmds = Vec::new();
    let mut t = Tick(0);
    while t.0 < 2_000 {
        t += 20;
        cmds.extend(second(&mut f, Some(here), t));
    }
    let placed: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::PlaceLair(p) => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), 1, "once per field: {placed:?}");
    assert_eq!(placed[0].species, Species::Fire);
    assert_eq!(f.d.stats.lairs_placed, 1);
    assert_eq!(f.d.lairs.len(), lairs0 + 1, "the placed lair was found");
    let surveys: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::OilSurvey(s) => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(surveys.len(), 1);
    // The survey radius (1,024) around the new lair reaches the three near chunks, not (-200, 300).
    assert_eq!(f.d.oil.chunks(), chunks0 + 3);
    assert!(f.d.oil.chunk_at(0, -200, 300).is_none());
    // The player near the oil chunks the survey found draws nothing more: the field was checked.
    let mut t2 = t;
    while t2.0 < t.0 + 1_000 {
        t2 += 20;
        cmds.extend(second(&mut f, Some(Pos::new(0, -3830, 4030)), t2));
    }
    assert_eq!(f.d.stats.lairs_placed, 1);
    // Off, nothing is placed.
    let mut off = flights(5, 0);
    off.d.ingest(
        &mut off.w,
        &mut NoInfrastructure,
        &field(-250, 250, 2),
        Tick(0),
    );
    let mut t = Tick(0);
    while t.0 < 2_000 {
        t += 20;
        second(&mut off, Some(here), t);
    }
    assert_eq!(off.d.stats.lairs_placed, 0);
}

/// OGC12: a world with no oil known keeps the old rules: no reserve falls, nothing is gated.
#[test]
fn no_oil_known_means_no_gate() {
    let (mut w, _) = synth::world(13);
    let mut d = Dragons::new(13, quick_oil(1), Tick::ZERO);
    let l = fire_lair(&mut w, &mut d, 4, 1800, 2500);
    let x = d.add_dragon(&mut w, adult("x", l, 100), Tick(0));
    d.run_until(&mut w, &mut NoInfrastructure, Tick(10_000));
    assert!(!d.oil_gated(x));
    assert_eq!(d.stats.refuels + d.stats.dry_raids + d.stats.oil_raids, 0);
}

// ---- paging, saves and determinism with oil ----------------------------------------------------

const META: PageKey = PageKey::new(0x4400, 0);
const VILLAGES: usize = 64;
const LAIRS: usize = 48;
const STEP: u64 = 500;
const SAVE_EVERY: u64 = TICKS_PER_DAY / 2;
const BUDGET: usize = 24 << 10;
const DAYS: u64 = 8;

struct Run {
    w: World,
    d: Dragons,
    iaf: FakeIaf,
    tanks: Tanks,
    store: Option<Arc<RwLock<PageStore>>>,
}

fn paged_tun() -> DragonTunables {
    // Low a day and a half in, so every fire dragon refuels a few times in the run.
    quick_oil(600)
}

/// Oil clusters over the grid: the first half reported at the start, all of them in the ground
/// (a survey finds the rest), and a district with tanks beside every third village.
fn start(seed: u64) -> Run {
    let mut w = synth::grid(VILLAGES, seed);
    let mut d = Dragons::new(seed, paged_tun(), Tick::ZERO);
    let lairs = synth::bench_lairs(&w, LAIRS, 2, seed);
    synth::populate(&mut w, &mut d, &lairs, Tick::ZERO);
    let mut iaf = FakeIaf::new(PLAYER);
    iaf.reaction = Reaction::Wound {
        share: 0.25,
        after: 150,
    };
    iaf.breaths = 5;
    let key = StreamKey::new(seed, SystemId::named("test.oil"), 0);
    let mut reported = Vec::new();
    for k in 0..16u64 {
        let mut r = key.slot(k);
        let (cx, cz) = (r.below(240) as i32, r.below(240) as i32);
        for j in 0..9 {
            let (x, z) = (cx + j % 3, cz + j / 3);
            iaf.ground_oil.insert((0, x, z), k % 5 == 0);
            if k < 8 {
                reported.push(OilChunkRec {
                    cx: x,
                    cz: z,
                    amount: 1,
                    flags: if k % 5 == 0 {
                        boundary::OIL_BOTTOMLESS
                    } else {
                        0
                    },
                });
            }
        }
    }
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::OilConfig(boundary::OilConfig {
            flags: boundary::OIL_FIELD_FLIGHTS | boundary::OIL_PLACE_LAIRS,
        }),
        Tick(0),
    );
    d.ingest(
        &mut w,
        &mut NoInfrastructure,
        &Obs::OilChunks(OilChunks {
            dimension: 0,
            source: boundary::OIL_SOURCE_DEEP_TIME,
            chunks: reported,
        }),
        Tick(0),
    );
    let sites: Vec<(u64, i32, i32, bool)> = w
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 3 == 0)
        .map(|(i, n)| (100 + i as u64, n.pos.x + 40, n.pos.z, false))
        .collect();
    let tanks = Tanks::at(
        &sites,
        DistrictSignals {
            oil_sample: true,
            polluted: false,
            fuel_mb: 30_000,
        },
    );
    Run {
        w,
        d,
        iaf,
        tanks,
        store: None,
    }
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rtsim-oilgate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn page(run: &mut Run, dir: &Path) {
    let store = Arc::new(RwLock::new(
        PageStore::open(dir, Durability::NoSync).expect("the store opens"),
    ));
    run.d.attach_store(store.clone(), BUDGET);
    run.store = Some(store);
}

/// The bench's zigzag over the grid, at the height of a player on the surface.
fn walk(t: Tick) -> Pos {
    let k = (t.0 / STEP) as i64;
    let span = 3_800;
    let tri = |v: i64| {
        let m = v.rem_euclid(2 * span);
        if m < span { m } else { 2 * span - m }
    };
    Pos::new(0, tri(k * 40) as i32, tri(k * 17 + 600) as i32)
}

fn step(r: &mut Run, t: Tick) -> Vec<Cmd> {
    r.d.observe_player(&mut r.w, PLAYER, walk(t), t);
    r.d.player_at(&mut r.w, PLAYER, walk(t), 70, t);
    r.d.run_until(&mut r.w, &mut r.tanks, t);
    let mut cmds = r.d.take_commands();
    r.iaf.apply(cmds.clone(), t);
    for (at, o) in r.iaf.due(t) {
        let at = at.max(r.d.now());
        r.d.ingest(&mut r.w, &mut r.tanks, &o, at);
    }
    let more = r.d.take_commands();
    r.iaf.apply(more.clone(), t);
    cmds.extend(more);
    r.d.enforce_budget();
    cmds
}

fn save(r: &mut Run) {
    let store = r.store.as_ref().expect("paged");
    let mut b = SaveBatch::new();
    let tickets = r.d.save_paged(&mut b, META);
    store
        .write()
        .unwrap()
        .commit(b, 0)
        .expect("the save commits");
    r.d.saved(tickets);
    r.d.enforce_budget();
}

fn save_hash(d: &Dragons) -> u64 {
    fnv1a64(&d.save())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

fn world_copy(seed: u64, w: &World) -> World {
    let (goods, ids) = synth::goods();
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    World::load(cfg, goods, synth::recipes(&ids), &w.save()).expect("the world loads")
}

/// The run exercised the oil gate, and the paged engine paged.
fn assert_oil_exercised(r: &Run, paged: &Run) {
    let s = r.d.stats;
    assert!(
        s.refuels > 0
            && s.oil_raids > 0
            && s.encroachments > 0
            && s.field_draws > 0
            && s.breaths > 0,
        "{s:?}"
    );
    let p = paged.d.paging_stats();
    assert!(p.evictions() > 0 && p.loads() > 0, "{p:?}");
    r.d.check(&r.w, None).expect("the resident run balances");
    paged
        .d
        .check(&paged.w, None)
        .expect("the paged run balances");
}

#[test]
fn with_oil_paged_equals_resident_and_a_restore_continues_exactly() {
    let seed = 21;
    let (dir_b, dir_c) = (tmpdir("b"), tmpdir("c"));
    let mut a = start(seed);
    let mut b = start(seed);
    page(&mut b, &dir_b);
    let mid = Tick::from_days(DAYS / 2) + 3 * STEP;
    let mut t = Tick::ZERO;
    while t < mid {
        t += STEP;
        assert_eq!(step(&mut a, t), step(&mut b, t), "commands at {t:?}");
        if t.0.is_multiple_of(SAVE_EVERY) || b.d.needs_save() {
            save(&mut b);
        }
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            assert_eq!(a.d.state_hash(), b.d.state_hash(), "at {t:?}");
            b.d.enforce_budget();
        }
    }
    // The midpoint: a paged engine from a copy of the pages, and a resident one from the
    // resident save, each with the world, the plugin and the tanks as they were.
    save(&mut b);
    copy_dir(&dir_b, &dir_c);
    let store_c = Arc::new(RwLock::new(
        PageStore::open(&dir_c, Durability::NoSync).expect("the copy opens"),
    ));
    let dc = Dragons::open_paged(store_c.clone(), BUDGET, META, paged_tun())
        .expect("the dragons open from their pages");
    assert_eq!(dc.resident_row_bytes(), 0, "nothing resident on open");
    let mut c = Run {
        w: world_copy(seed, &b.w),
        d: dc,
        iaf: b.iaf.clone(),
        tanks: b.tanks.clone(),
        store: Some(store_c),
    };
    let mut e = Run {
        w: world_copy(seed, &a.w),
        d: Dragons::load(paged_tun(), &a.d.save()).expect("the resident save loads"),
        iaf: a.iaf.clone(),
        tanks: a.tanks.clone(),
        store: None,
    };
    assert_eq!(c.d.state_hash(), a.d.state_hash(), "restored from pages");
    assert_eq!(e.d.state_hash(), a.d.state_hash(), "restored from the save");
    assert_eq!(c.d.oil.chunks(), a.d.oil.chunks());
    assert_eq!(e.d.oil.fields(), a.d.oil.fields());
    c.d.enforce_budget();
    while t < Tick::from_days(DAYS) {
        t += STEP;
        let ca = step(&mut a, t);
        assert_eq!(ca, step(&mut b, t), "commands at {t:?}");
        assert_eq!(ca, step(&mut c, t), "the paged restore's commands at {t:?}");
        assert_eq!(
            ca,
            step(&mut e, t),
            "the resident restore's commands at {t:?}"
        );
        if t.0.is_multiple_of(SAVE_EVERY) || b.d.needs_save() || c.d.needs_save() {
            save(&mut b);
            save(&mut c);
        }
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            let h = a.d.state_hash();
            assert_eq!(h, b.d.state_hash(), "at {t:?}");
            assert_eq!(h, c.d.state_hash(), "the paged restore at {t:?}");
            assert_eq!(h, e.d.state_hash(), "the resident restore at {t:?}");
            assert_eq!(a.w.state_hash(), c.w.state_hash());
            b.d.enforce_budget();
            c.d.enforce_budget();
        }
    }
    assert_oil_exercised(&a, &c);
    assert_eq!(a.d.stats, b.d.stats);
    let h = save_hash(&a.d);
    assert_eq!(h, save_hash(&b.d));
    assert_eq!(h, save_hash(&c.d), "the paged restore saves the same");
    assert_eq!(h, save_hash(&e.d), "the resident restore saves the same");
    drop((b, c));
    std::fs::remove_dir_all(&dir_b).unwrap();
    std::fs::remove_dir_all(&dir_c).unwrap();
}
