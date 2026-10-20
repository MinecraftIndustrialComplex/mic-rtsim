// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim herds`: migrating herds (PLAN.md §10.2) in the synthetic valley, headless, for a
//! little over two sim years.
//!
//! **The world:** the valley (`rtsim_ecology::herds::synth::Valley`): cold north, warm south,
//! forage that is wet in the north in summer and in the south in winter, and an east-west ridge
//! with one pass a cell wide. Six herds, about 33,000 sheep, cows, horses, goats and pigs, start
//! in their spring pastures south of the ridge. The phase 1 dragon world's villages sit in the
//! middle (`rtsim_ecology::synth::world`), with two lairs: Ashpeak Roost south of the ridge and
//! Highmoor Roost in the northern summer pastures.
//!
//! **The plugin** is `rtsim_ecology::herds::synth::FakeHerds`: every command and observation goes
//! through the boundary codec (docs/boundary.md §8).
//!
//! **The scripted player:**
//! 1. In early summer of year 0 it walks to the Northdown flock: a group materializes; it kills
//!    six, shears three and leads one away (A10).
//! 2. It walks two kilometres away for two hours (the group is captured and folds back), then
//!    returns: the same animals come back, with the same colours, and none it killed.
//! 3. In autumn it fences the pass and waits there: the flock's puppets fail to pass, the plugin
//!    reports the obstacle, the cost of the pass rises, and the flock goes round the ridge.
//! 4. Year 1: the flock's spring and autumn routes avoid the pass.
//!
//! **Dragons** hunt the herds through the prey interface (`Dragons::run_until_with_herds`), and
//! follow a herd they ate from.
//!
//! **A restart** in the middle: world, dragons and herds saved and loaded, with the herds' state
//! hash compared.
//!
//! **Checks** (exit 1 on a failure): conservation (integer ledgers against the classes) after
//! every step; the kill count drops the flock by exactly six; identity on return; no killed or
//! taken animal reappears; the obstacle and the detour; dragons eating from herds; the restart.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::engine::{Dragons, HuntRecord, NewDragon};
use rtsim_ecology::herds::engine::{Decision, HerdLog};
use rtsim_ecology::herds::forage::cell_of;
use rtsim_ecology::herds::herd::agent_of;
use rtsim_ecology::herds::synth::{FakeHerds, Valley};
use rtsim_ecology::herds::{HerdId, HerdLedger, HerdTunables, Herds};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::species::{Gender, Species};
use rtsim_ecology::tunables::{DragonTunables, PreyKind};
use rtsim_ecology::{calendar::Calendar, synth};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};
use serde::Serialize;

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub out: Option<PathBuf>,
    pub quiet: bool,
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let mut o = Options {
        days: 200,
        seed: 7,
        out: Some(PathBuf::from("scenario/results/sim-herds.json")),
        quiet: false,
    };
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--days" => match num(it.next()) {
                Some(n) if n >= 150 => o.days = n,
                _ => return bad("--days needs a number of at least 150"),
            },
            "--seed" => match num(it.next()) {
                Some(n) => o.seed = n,
                None => return bad("--seed needs a number"),
            },
            "--out" => match it.next() {
                Some(f) => o.out = Some(PathBuf::from(f)),
                None => return bad("--out needs a file"),
            },
            "--no-write" => o.out = None,
            "--quiet" => o.quiet = true,
            other => return bad(&format!("unknown option {other}")),
        }
    }
    ExitCode::from(main(o) as u8)
}

fn bad(why: &str) -> ExitCode {
    eprintln!(
        "{why}\nusage: rtsim-cli sim herds [--days N] [--seed S] [--out FILE | --no-write] [--quiet]"
    );
    ExitCode::from(64)
}

const PLAYER: (u64, u64) = (0xBE4, 0x5EE9);
const STEP: u64 = 500;
const FLOCK: &str = "Northdown flock";

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Serialize, Clone)]
struct HerdRow {
    name: String,
    species: &'static str,
    count: u64,
    young: u64,
    adult: u64,
    old: u64,
    x: i64,
    z: i64,
    condition: [u16; 3],
    satisfaction: u16,
}

#[derive(Serialize)]
struct Snapshot {
    day: f64,
    year: i64,
    sub_season: u32,
    total: u64,
    by_species: BTreeMap<&'static str, u64>,
    herds: Vec<HerdRow>,
}

#[derive(Serialize)]
struct Approach {
    day: f64,
    epoch: u32,
    /// (member index, variant: a sheep's dye id).
    members: Vec<(u64, u32)>,
}

#[derive(Serialize)]
struct PlayerStory {
    approaches: Vec<Approach>,
    killed: Vec<u64>,
    sheared: Vec<u64>,
    led_away: Option<u64>,
    count_before_kill: u64,
    count_after_kill: u64,
    returned_same: usize,
    returned_new: usize,
    killed_seen_again: usize,
}

#[derive(Serialize)]
struct FenceStory {
    fenced_cells: Vec<(i32, i32)>,
    obstacles: u64,
    pass_cost: f64,
    /// Days on which the flock's planned route went through the pass, by year.
    through_pass: BTreeMap<i64, Vec<f64>>,
    /// The flock's crossings of the ridge line: (day, x where it crossed), by year.
    crossings: BTreeMap<i64, Vec<(f64, i64)>>,
}

#[derive(Serialize)]
struct DragonStory {
    hunts: Vec<HuntRecord>,
    /// Hunts that found food, per season and prey kind.
    by_season: BTreeMap<&'static str, BTreeMap<&'static str, u64>>,
    /// Herd hunts: (day, dragon, herd, animals, distance from the lair).
    herd_hunts: Vec<(f64, String, String, i64, f64)>,
}

#[derive(Serialize)]
struct Results {
    seed: u64,
    days: u64,
    wall_ms: f64,
    founded: BTreeMap<String, u64>,
    snapshots: Vec<Snapshot>,
    ledgers: BTreeMap<String, HerdLedger>,
    player: PlayerStory,
    fence: FenceStory,
    dragons: DragonStory,
    restart: (u64, u64),
    stats: rtsim_ecology::herds::HerdStats,
    decisions: Vec<Decision>,
    log: Vec<HerdLog>,
    checks: Vec<Check>,
}

struct Sim {
    w: World,
    d: Dragons,
    h: Herds,
    fake: FakeHerds,
    lairs: BTreeMap<String, Pos>,
    seed: u64,
}

fn tunables() -> HerdTunables {
    HerdTunables {
        calendar: Calendar::default(),
        ..HerdTunables::default()
    }
}

fn dragon_tunables() -> DragonTunables {
    DragonTunables {
        calendar: Calendar::default(),
        ..DragonTunables::default()
    }
}

fn build(seed: u64) -> Sim {
    let (mut w, _) = synth::world(seed);
    let mut d = Dragons::new(seed, dragon_tunables(), Tick::ZERO);
    let mut lairs = BTreeMap::new();
    for (key, name, x, z, dragon, age) in [
        (201u64, "Highmoor Roost", 300, -3_400, "Ashwing", 110u64),
        (202, "Ashpeak Roost", 1_000, 1_100, "Cinderjaw", 105),
    ] {
        let l = d.add_lair(
            &mut w,
            key,
            name,
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, x, z),
            110,
            Tick::ZERO,
        );
        lairs.insert(dragon.to_string(), Pos::new(0, x, z));
        d.add_dragon(
            &mut w,
            NewDragon {
                name: dragon.to_string(),
                species: Species::Fire,
                gender: Gender::Female,
                variant: 0,
                age_ticks: age * TICKS_PER_DAY,
                lair: Some(l),
                fullness: 70,
                health: None,
                seed: None,
                at: None,
            },
            Tick::ZERO,
        );
    }
    let mut h = rtsim_ecology::herds::synth::valley(seed, tunables(), Tick::ZERO);
    h.log_enabled = true;
    h.trace_enabled = true;
    d.log_enabled = true;
    Sim {
        w,
        d,
        h,
        fake: FakeHerds::default(),
        lairs,
        seed,
    }
}

impl Sim {
    /// Runs to `to` in steps, the player standing at `at` (or absent), with the plugin answering.
    fn run(&mut self, to: Tick, player: Option<Pos>, checks: &mut Vec<String>) {
        let mut t = self.h.now();
        while t < to {
            let next = (t + STEP).min(to);
            if let Some(p) = player {
                self.h.observe_player(&mut self.w.legs, PLAYER, p, t);
            }
            self.d
                .run_until_with_herds(&mut self.w, &mut NoInfrastructure, &mut self.h, next);
            self.pump(next);
            if let Err(e) = self.h.check(next) {
                checks.push(format!(
                    "day {:.2}: {e}",
                    next.0 as f64 / TICKS_PER_DAY as f64
                ));
            }
            t = next;
        }
    }

    /// Commands to the plugin, its walk, and its answers back.
    fn pump(&mut self, at: Tick) {
        let cmds = self.h.take_commands();
        self.fake.apply(cmds, at);
        self.fake.walk(&self.h, at);
        for o in self.fake.take_obs() {
            self.h.ingest(&mut self.w.legs, &o, at);
        }
        // The replan an obstacle causes can steer the group again at once.
        let cmds = self.h.take_commands();
        self.fake.apply(cmds, at);
        for o in self.fake.take_obs() {
            self.h.ingest(&mut self.w.legs, &o, at);
        }
    }

    fn leave(&mut self, at: Tick) {
        self.h.player_left(&mut self.w.legs, PLAYER, at);
        self.pump(at);
    }

    fn flock(&self) -> HerdId {
        self.h.by_name(FLOCK).expect("the flock")
    }
}

fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn snapshot(h: &Herds, t: Tick) -> Snapshot {
    let subs = h.subs();
    let n = subs.index(t);
    let mut by_species: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut rows = Vec::new();
    for (id, x) in h.herds.iter() {
        let c = h.cohorts(id, t);
        let p = h.position(id, t);
        *by_species.entry(x.species.name()).or_default() += c.iter().sum::<u64>();
        rows.push(HerdRow {
            name: x.name.clone(),
            species: x.species.name(),
            count: c.iter().sum(),
            young: c[0],
            adult: c[1],
            old: c[2],
            x: p.0,
            z: p.1,
            condition: x.condition,
            satisfaction: x.satisfaction,
        });
    }
    Snapshot {
        day: day(t),
        year: rtsim_ecology::herds::climate::Subs::year(n),
        sub_season: rtsim_ecology::herds::climate::Subs::in_year(n),
        total: by_species.values().sum(),
        by_species,
        herds: rows,
    }
}

pub fn main(o: Options) -> i32 {
    let wall = Instant::now();
    let mut sim = build(o.seed);
    let mut errors: Vec<String> = Vec::new();
    let mut checks: Vec<Check> = Vec::new();
    let founded: BTreeMap<String, u64> = sim
        .h
        .herds
        .iter()
        .map(|(_, x)| (x.name.clone(), x.ledger.founded))
        .collect();
    let valley = Valley::default();
    let pass_cells: Vec<(i32, i32)> = (valley.ridge.0..=valley.ridge.1)
        .map(|cz| (valley.pass, cz))
        .collect();
    let mut snapshots = Vec::new();
    let subs = sim.h.subs();
    let snap_every = subs.len;
    let mut next_snap = Tick::ZERO;
    let mut advance = |sim: &mut Sim,
                       to: Tick,
                       p: Option<Pos>,
                       errors: &mut Vec<String>,
                       snaps: &mut Vec<Snapshot>| {
        while next_snap <= to {
            sim.run(next_snap, p, errors);
            snaps.push(snapshot(&sim.h, next_snap));
            next_snap += snap_every;
        }
        sim.run(to, p, errors);
    };

    // 1. Early summer of year 0: the player meets the flock.
    let t1 = Tick::from_days(26);
    advance(&mut sim, t1, None, &mut errors, &mut snapshots);
    let flock = sim.flock();
    let agent = agent_of(flock);
    let at_flock = |sim: &Sim, t: Tick| {
        let p = sim.h.position(sim.flock(), t);
        // Stand just off the herd's centre, inside its footprint.
        Pos::new(0, p.0 as i32 + 40, p.1 as i32)
    };
    let mut approaches = Vec::new();
    let p1 = at_flock(&sim, t1);
    advance(&mut sim, t1 + 2_000, Some(p1), &mut errors, &mut snapshots);
    let first = sim.fake.group_of(agent).cloned();
    let count_before_kill = sim.h.count(flock, sim.h.now());
    let (mut killed, mut sheared, mut led_away) = (Vec::new(), Vec::new(), None);
    if let Some(g) = &first {
        approaches.push(Approach {
            day: day(g.since),
            epoch: g.epoch,
            members: g
                .bodies
                .iter()
                .map(|b| (b.spawn.member, b.spawn.variant))
                .collect(),
        });
        killed = sim.fake.kill(agent, 6, PLAYER);
        sheared = sim.fake.shear(agent, 3, PLAYER);
        led_away = sim.fake.lead(agent, PLAYER);
        let now = sim.h.now();
        sim.pump(now);
    }
    let count_after_kill = sim.h.count(flock, sim.h.now());
    checks.push(Check {
        name: "a group materializes when the player meets the flock",
        ok: first.as_ref().is_some_and(|g| !g.bodies.is_empty()),
        detail: format!(
            "{} animals of {}",
            first.as_ref().map_or(0, |g| g.bodies.len()),
            count_before_kill
        ),
    });
    checks.push(Check {
        name: "killing six and leading one away drops the flock by exactly seven",
        ok: count_before_kill - count_after_kill == 7 && killed.len() == 6 && led_away.is_some(),
        detail: format!("{count_before_kill} before, {count_after_kill} after"),
    });

    // 2. Walk away for two hours, then come back.
    let t2 = sim.h.now() + 1_000;
    let away = Pos::new(0, p1.x + 2_000, p1.z);
    advance(
        &mut sim,
        t2 + 2_000,
        Some(away),
        &mut errors,
        &mut snapshots,
    );
    let gone = sim.fake.group_of(agent).is_none();
    let t3 = sim.h.now();
    let p3 = at_flock(&sim, t3);
    advance(&mut sim, t3 + 2_000, Some(p3), &mut errors, &mut snapshots);
    let second = sim.fake.group_of(agent).cloned();
    let mut returned_same = 0;
    let mut returned_new = 0;
    let mut killed_seen_again = 0;
    let mut colours_ok = true;
    if let (Some(a), Some(b)) = (&first, &second) {
        approaches.push(Approach {
            day: day(b.since),
            epoch: b.epoch,
            members: b
                .bodies
                .iter()
                .map(|x| (x.spawn.member, x.spawn.variant))
                .collect(),
        });
        let before: BTreeMap<u64, u32> = a
            .bodies
            .iter()
            .map(|x| (x.spawn.member, x.spawn.variant))
            .collect();
        for x in &b.bodies {
            match before.get(&x.spawn.member) {
                Some(&v) => {
                    returned_same += 1;
                    colours_ok &= v == x.spawn.variant;
                }
                None => returned_new += 1,
            }
            if killed.contains(&x.spawn.member) || led_away == Some(x.spawn.member) {
                killed_seen_again += 1;
            }
        }
    }
    let expected_back = first
        .as_ref()
        .map_or(0, |g| g.bodies.len().saturating_sub(7));
    checks.push(Check {
        name: "walking away captures the group",
        ok: gone,
        detail: format!("embodied after walking away: {}", !gone),
    });
    checks.push(Check {
        name: "the same animals come back, with the same colours, and none killed or led away",
        ok: second.is_some() && returned_same >= expected_back && colours_ok && killed_seen_again == 0,
        detail: format!(
            "{returned_same} of the {expected_back} survivors back ({returned_new} new to fill the group), colours {}, {killed_seen_again} killed seen again",
            if colours_ok { "identical" } else { "CHANGED" }
        ),
    });
    sim.leave(sim.h.now());

    // 3. Autumn of year 0: fence the pass and wait there until the flock has gone round.
    let autumn = subs.start(6);
    advance(&mut sim, autumn, None, &mut errors, &mut snapshots);
    for c in &pass_cells {
        sim.fake.fences.insert(*c);
    }
    let pass_pos = Pos::new(0, valley.pass * 128 + 64, valley.ridge.1 * 128 + 200);
    let winter = subs.start(9) + TICKS_PER_DAY * 4;
    // The player stays at the pass through autumn (the scripted vigil), its interest booked.
    advance(
        &mut sim,
        winter,
        Some(pass_pos),
        &mut errors,
        &mut snapshots,
    );
    sim.leave(sim.h.now());
    let obstacles = sim.h.stats.obstacles;
    let pass_cost: f64 = pass_cells
        .iter()
        .map(|c| {
            sim.h.obstacles.extra(
                (0, c.0, c.1),
                sim.h.now(),
                sim.h.tun.obstacle_half_life_ticks,
            )
        })
        .fold(0.0, f64::max);

    // 4. Halfway through: a restart (world, dragons, herds saved and loaded).
    let mid = Tick::from_days(o.days / 2);
    advance(&mut sim, mid, None, &mut errors, &mut snapshots);
    let before = sim.h.state_hash();
    let (goods, _) = synth::goods();
    let book = synth::recipes(&synth::goods().1);
    let w2 = World::load(WorldConfig::new(sim.seed), goods, book, &sim.w.save());
    let d2 = Dragons::load(dragon_tunables(), &sim.d.save());
    let h2 = Herds::load(tunables(), Box::new(Valley::default()), &sim.h.save());
    let restart = match (w2, d2, h2) {
        (Ok(w2), Ok(mut d2), Ok(mut h2)) => {
            h2.log = std::mem::take(&mut sim.h.log);
            h2.trace = std::mem::take(&mut sim.h.trace);
            h2.predations = std::mem::take(&mut sim.h.predations);
            h2.log_enabled = true;
            h2.trace_enabled = true;
            d2.log_enabled = true;
            d2.hunts = std::mem::take(&mut sim.d.hunts);
            d2.log = std::mem::take(&mut sim.d.log);
            d2.prey_by_season = sim.d.prey_by_season;
            sim.w = w2;
            sim.d = d2;
            sim.h = h2;
            (before, sim.h.state_hash())
        }
        (a, b, c) => {
            errors.push(format!(
                "restart failed: world {:?}, dragons {:?}, herds {:?}",
                a.err(),
                b.err(),
                c.err()
            ));
            (before, 0)
        }
    };
    checks.push(Check {
        name: "a restart restores the herds' state hash",
        ok: restart.0 == restart.1,
        detail: format!("{:#x} saved, {:#x} restored", restart.0, restart.1),
    });

    // 5. On to the end.
    advance(
        &mut sim,
        Tick::from_days(o.days),
        None,
        &mut errors,
        &mut snapshots,
    );
    let end = sim.h.now();

    // The fence: where the flock's routes crossed the ridge line, by year.
    let ridge_z = (valley.ridge.0 * 128, (valley.ridge.1 + 1) * 128);
    let mut through_pass: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
    let mut crossings: BTreeMap<i64, Vec<(f64, i64)>> = BTreeMap::new();
    for dcs in sim.h.trace.iter().filter(|d| d.herd == FLOCK) {
        let year = (dcs.tick / subs.year_ticks()) as i64;
        let mut cells = BTreeSet::new();
        let pts = &dcs.route;
        for w2 in pts.windows(2) {
            let steps = 32;
            for k in 0..=steps {
                let x = i64::from(w2[0].0) + (i64::from(w2[1].0 - w2[0].0) * k) / steps;
                let z = i64::from(w2[0].1) + (i64::from(w2[1].1 - w2[0].1) * k) / steps;
                cells.insert(cell_of(x, z));
            }
            let (z0, z1) = (w2[0].1, w2[1].1);
            let mid_z = (ridge_z.0 + ridge_z.1) / 2;
            if (z0 - mid_z).signum() != (z1 - mid_z).signum() && z0 != z1 {
                let f = f64::from(mid_z - z0) / f64::from(z1 - z0);
                let x = f64::from(w2[0].0) + f * f64::from(w2[1].0 - w2[0].0);
                crossings.entry(year).or_default().push((dcs.day, x as i64));
            }
        }
        if pass_cells.iter().any(|c| cells.contains(c)) {
            through_pass.entry(year).or_default().push(dcs.day);
        }
    }
    let fence_day = day(autumn);
    // The flock's first obstacle report: no route it planned after it goes through the pass.
    let struck = sim
        .h
        .log
        .iter()
        .find(|e| e.kind == "obstacle" && e.herd.as_deref() == Some(FLOCK))
        .map(|e| e.day);
    let pass_after_fence: Vec<f64> = through_pass
        .values()
        .flatten()
        .copied()
        .filter(|&d| struck.is_some_and(|s| d >= s))
        .collect();
    let crossed_year1 = crossings.get(&1).cloned().unwrap_or_default();
    checks.push(Check {
        name: "the fenced pass stops the flock's puppets and the plugin reports it",
        ok: struck.is_some() && pass_cost > 0.0,
        detail: format!(
            "{obstacles} obstacle reports, the flock's first on day {}; the pass costs {:.1}x more",
            struck.map_or("none".to_string(), |d| format!("{d:.2}")),
            1.0 + pass_cost
        ),
    });
    checks.push(Check {
        name: "the flock used the pass before the fence",
        ok: through_pass
            .get(&0)
            .is_some_and(|v| v.iter().any(|&d| d < fence_day)),
        detail: format!(
            "planned through the pass on days {:?}",
            through_pass.get(&0)
        ),
    });
    checks.push(Check {
        name: "after the fence the flock takes the long way round (year 1 crossings away from the pass)",
        ok: struck.is_some()
            && pass_after_fence.is_empty()
            && !crossed_year1.is_empty()
            && crossed_year1
                .iter()
                .all(|&(_, x)| (x - i64::from(valley.pass * 128 + 64)).abs() > 256),
        detail: format!(
            "routes through the pass after the obstacle: {pass_after_fence:?}; year 1 crossings (day, x): {crossed_year1:?}"
        ),
    });

    // Dragons.
    let mut by_season: BTreeMap<&'static str, BTreeMap<&'static str, u64>> = BTreeMap::new();
    for s in rtsim_ecology::calendar::Season::ALL {
        let m = by_season.entry(s.name()).or_default();
        for k in PreyKind::ALL {
            m.insert(k.name(), sim.d.prey_by_season[s.index()][k.index()]);
        }
    }
    // Herd hunts, with the herd's position at the moment it was taken from (the herds' own
    // predation records) against the dragon's lair.
    let names: BTreeMap<u64, String> = sim
        .d
        .dragons
        .iter()
        .map(|(id, x)| (id.to_bits(), x.name.clone()))
        .collect();
    let herd_hunts: Vec<(f64, String, String, i64, f64)> = sim
        .h
        .predations
        .iter()
        .map(|p| {
            let dragon = names.get(&p.predator).cloned().unwrap_or_default();
            let lair = sim.lairs.get(&dragon).copied().unwrap_or(Pos::new(0, 0, 0));
            let d = (((p.herd_pos.0 - i64::from(lair.x)) as f64).powi(2)
                + ((p.herd_pos.1 - i64::from(lair.z)) as f64).powi(2))
            .sqrt();
            (p.day, dragon, p.herd.clone(), p.animals as i64, d)
        })
        .collect();
    let eaten: u64 = sim.h.herds.iter().map(|(_, x)| x.ledger.eaten).sum();
    let eaten_by_dragons: i64 = herd_hunts.iter().map(|h| h.3).sum();
    checks.push(Check {
        name: "dragons hunt the herds, and every animal they took left a herd's ledger as eaten",
        ok: !herd_hunts.is_empty() && eaten as i64 == eaten_by_dragons,
        detail: format!(
            "{} herd hunts took {eaten_by_dragons} animals; herds booked {eaten} eaten",
            herd_hunts.len()
        ),
    });
    let far = herd_hunts
        .iter()
        .filter(|h| h.1 == "Ashwing")
        .map(|h| h.4)
        .fold(0.0, f64::max);
    checks.push(Check {
        name: "a dragon follows a herd it ate from beyond its usual range",
        ok: far > 2_500.0,
        detail: format!("Ashwing's farthest herd hunt: {far:.0} blocks from Highmoor Roost (stage 5 range 2,500)"),
    });
    let totals = sim.h.totals(end);
    let total: u64 = totals.iter().sum();
    checks.push(Check {
        name: "tens of thousands of animals, conserved as integers after every step",
        ok: errors.is_empty() && total >= 10_000,
        detail: if errors.is_empty() {
            format!("{total} animals at the end; ledgers balance at every step")
        } else {
            errors.join("; ")
        },
    });

    let ledgers: BTreeMap<String, HerdLedger> = sim
        .h
        .herds
        .iter()
        .map(|(_, x)| (x.name.clone(), x.ledger))
        .collect();
    let results = Results {
        seed: o.seed,
        days: o.days,
        wall_ms: wall.elapsed().as_secs_f64() * 1e3,
        founded,
        snapshots,
        ledgers,
        player: PlayerStory {
            approaches,
            killed,
            sheared,
            led_away,
            count_before_kill,
            count_after_kill,
            returned_same,
            returned_new,
            killed_seen_again,
        },
        fence: FenceStory {
            fenced_cells: pass_cells.clone(),
            obstacles,
            pass_cost,
            through_pass,
            crossings,
        },
        dragons: DragonStory {
            hunts: sim.d.hunts.clone(),
            by_season,
            herd_hunts,
        },
        restart,
        stats: sim.h.stats,
        decisions: std::mem::take(&mut sim.h.trace),
        log: std::mem::take(&mut sim.h.log),
        checks,
    };
    if !o.quiet {
        print(&results);
    }
    if let Some(path) = &o.out {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(path, serde_json::to_string_pretty(&results).expect("json")) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(e) => eprintln!("could not write {}: {e}", path.display()),
        }
    }
    i32::from(results.checks.iter().any(|c| !c.ok))
}

fn print(r: &Results) {
    println!(
        "herds over {} days (seed {}), {:.0} ms",
        r.days, r.seed, r.wall_ms
    );
    println!("\nfounded: {:?}", r.founded);
    println!("\nby sub-season (total, then each herd's count and centre):");
    for s in &r.snapshots {
        let herds: Vec<String> = s
            .herds
            .iter()
            .map(|h| {
                format!(
                    "{} {} ({}, {})",
                    h.name.split(' ').next().unwrap_or(""),
                    h.count,
                    h.x,
                    h.z
                )
            })
            .collect();
        println!(
            "  day {:>6.1} y{} s{:>2}: {:>6} | {}",
            s.day,
            s.year,
            s.sub_season,
            s.total,
            herds.join(", ")
        );
    }
    println!("\nledgers:");
    for (n, l) in &r.ledgers {
        println!(
            "  {n:<16} founded {:>6} born {:>6} died {:>6} starved {:>5} killed {:>3} eaten {:>4} taken {:>2} lost {:>2}",
            l.founded, l.born, l.died, l.starved, l.killed, l.eaten, l.taken, l.lost
        );
    }
    let p = &r.player;
    println!(
        "\nthe player: {} approaches; killed {:?}, sheared {:?}, led away {:?}; flock {} -> {}; on return {} the same and {} new, {} killed seen again",
        p.approaches.len(),
        p.killed,
        p.sheared,
        p.led_away,
        p.count_before_kill,
        p.count_after_kill,
        p.returned_same,
        p.returned_new,
        p.killed_seen_again
    );
    for a in &p.approaches {
        let m: Vec<String> = a
            .members
            .iter()
            .take(12)
            .map(|(k, v)| format!("{k}:{v}"))
            .collect();
        println!(
            "  day {:.2} epoch {}: {} members, first {}",
            a.day,
            a.epoch,
            a.members.len(),
            m.join(" ")
        );
    }
    let f = &r.fence;
    println!(
        "\nthe fence at {:?}: {} obstacle reports, pass cost +{:.1}x; routes through the pass by year {:?}; ridge crossings {:?}",
        f.fenced_cells, f.obstacles, f.pass_cost, f.through_pass, f.crossings
    );
    println!(
        "\ndragon hunts that found food, by season and kind: {:?}",
        r.dragons.by_season
    );
    for h in r.dragons.herd_hunts.iter().take(40) {
        println!(
            "  day {:>6.2} {:<10} {:<16} {:>3} animals, {:>5.0} blocks from its lair",
            h.0, h.1, h.2, h.3, h.4
        );
    }
    println!("\nwhat happened (selected):");
    for e in &r.log {
        if matches!(
            e.kind,
            "materialize" | "capture" | "obstacle" | "killed" | "taken" | "despawn" | "extinct"
        ) {
            println!(
                "  day {:>6.2} {:<12} {:<16} {}",
                e.day,
                e.kind,
                e.herd.clone().unwrap_or_default(),
                e.what
            );
        }
    }
    println!("\nstats: {:?}", r.stats);
    println!("restart: {:#x} -> {:#x}", r.restart.0, r.restart.1);
    println!("\nchecks:");
    for c in &r.checks {
        println!(
            "  [{}] {}: {}",
            if c.ok { "ok" } else { "FAIL" },
            c.name,
            c.detail
        );
    }
}
