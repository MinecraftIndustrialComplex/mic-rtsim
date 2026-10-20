// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim dragons`: phase 2's dragons (PLAN.md §9) on the synthetic world, headless, from
//! autumn into winter.
//!
//! **The world** (`rtsim_ecology::synth`): eight villages whose herds (cattle and sheep) are market
//! stock and travel the roads in caravans; five lairs, four with dragons and one empty roost; a
//! player's farm, which is both a phase 1 base and an industry district (`rtsim-industry`'s fake
//! world), roofless and undefended.
//!
//! **The plugin** is `rtsim_ecology::synth::FakeIaf`: every command and observation goes through
//! the boundary codec (docs/boundary.md §6).
//!
//! **The scripted player** moves between places on a timetable; its interest area is booked in the
//! leg index at each move, and its position feeds the home counter (P26). It reacts to dragons by
//! name when they materialize near it: it wounds Cinderjaw on the road, later kills it at home, and
//! watches the rest. It robs a hoard once.
//!
//! **What it checks** (exit 1 on a failure): conservation with predation (the world, the ledger,
//! the hoards, the industry books and the fake world's own totals), and the story's beats: a wound
//! that becomes a grudge, a grudge raid on the unwatched farm resolved by the industry engine, the
//! kill and the heir's grudge, the heir hunting the player, a bored departure, and winter shifting
//! the prey from roads to villages.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_ecology::boundary::{self, Cmd, Obs};
use rtsim_ecology::calendar::{Calendar, Season};
use rtsim_ecology::dragon::{LairKind, dragon_of};
use rtsim_ecology::engine::{DragonStats, Dragons, HuntRecord, LogEntry};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink};
use rtsim_ecology::prey::Item;
use rtsim_ecology::synth::{self, FakeIaf, Reaction};
use rtsim_ecology::tunables::{DragonTunables, PreyKind};
use rtsim_industry::district::AttackReport;
use rtsim_industry::fake::{FakeWorld, Visit};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::Pos;
use rtsim_world::World;
use serde::Serialize;

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub out: Option<PathBuf>,
    pub quiet: bool,
}

/// Parses `sim dragons` options.
pub fn cli(rest: &[&str]) -> ExitCode {
    let mut o = Options {
        days: 48,
        seed: 7,
        out: Some(PathBuf::from("scenario/results/sim-dragons.json")),
        quiet: false,
    };
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--days" => match num(it.next()) {
                Some(n) if n >= 30 => o.days = n,
                _ => return bad("--days needs a number of at least 30"),
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
        "{why}\nusage: rtsim-cli sim dragons [--days N] [--seed S] [--out FILE | --no-write] [--quiet]"
    );
    ExitCode::from(64)
}

/// The player's UUID halves.
const PLAYER: (u64, u64) = (0x1111, 0x2222);
use synth::FARM;

/// Where the player is, from a day on, and what it does to dragons it meets.
struct Phase {
    from_day: f64,
    place: &'static str,
    x: i32,
    z: i32,
    farm_loaded: bool,
    react: &'static [(&'static str, Reaction)],
    /// Robs this lair's hoard half a day in.
    rob: Option<u64>,
}

fn script() -> Vec<Phase> {
    vec![
        Phase {
            from_day: 0.0,
            place: "home (the farm)",
            x: 300,
            z: 1300,
            farm_loaded: true,
            react: &[],
            rob: None,
        },
        Phase {
            from_day: 3.2,
            place: "the Ashby to Fernwick road",
            x: 490,
            z: 655,
            farm_loaded: false,
            react: &[(
                "Cinderjaw",
                Reaction::Wound {
                    share: 0.3,
                    after: 400,
                },
            )],
            rob: None,
        },
        Phase {
            from_day: 4.5,
            place: "Brindle",
            x: 0,
            z: 0,
            farm_loaded: false,
            react: &[],
            rob: None,
        },
        Phase {
            from_day: 7.0,
            place: "home (the farm)",
            x: 300,
            z: 1300,
            farm_loaded: true,
            react: &[("Cinderjaw", Reaction::Kill { after: 800 })],
            rob: None,
        },
        Phase {
            from_day: 18.0,
            place: "Highmoor",
            x: 1400,
            z: -400,
            farm_loaded: false,
            react: &[],
            rob: None,
        },
        Phase {
            from_day: 21.0,
            place: "Marrowdale",
            x: -600,
            z: -700,
            farm_loaded: false,
            react: &[],
            rob: None,
        },
        Phase {
            from_day: 24.0,
            place: "Rimefang Roost",
            x: -500,
            z: -1500,
            farm_loaded: false,
            react: &[],
            rob: Some(102),
        },
        Phase {
            from_day: 25.0,
            place: "home (the farm)",
            x: 300,
            z: 1300,
            farm_loaded: true,
            react: &[],
            rob: None,
        },
    ]
}

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// The farm (`synth::farm`), loaded while the player is home.
fn farm(seed: u64, phases: &[Phase]) -> FakeWorld {
    let visits = phases
        .iter()
        .map(|p| Visit {
            from: day(p.from_day),
            at: if p.farm_loaded { vec![0] } else { Vec::new() },
        })
        .collect();
    synth::farm(seed, [0.0; 4], visits)
}

#[derive(Serialize)]
struct DragonOut {
    name: String,
    species: &'static str,
    alive: bool,
    age_days: f64,
    stage: u8,
    lair: Option<String>,
    fullness: i32,
    health: f32,
    feeling_for_player: i8,
    holds_grudge: bool,
}

#[derive(Serialize)]
struct LairOut {
    name: String,
    species: &'static str,
    kind: &'static str,
    occupants: Vec<String>,
    hoard: BTreeMap<String, i64>,
}

#[derive(Serialize)]
struct VillageOut {
    name: String,
    feeling_for_player: i8,
    cattle_stock_start: f64,
    cattle_stock_end: f64,
    cattle_price_start: f64,
    cattle_price_end: f64,
}

#[derive(Serialize)]
struct SeasonOut {
    season: Season,
    hunts: BTreeMap<&'static str, u64>,
    fullness: BTreeMap<&'static str, i64>,
    village_share: f64,
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct Out {
    seed: u64,
    days: u64,
    starts_in: Season,
    wall_ms: f64,
    checks: Vec<Check>,
    seasons: Vec<SeasonOut>,
    dragons: Vec<DragonOut>,
    lairs: Vec<LairOut>,
    villages: Vec<VillageOut>,
    farm_attacks: Vec<AttackReport>,
    farm_destroyed_blocks_applied: usize,
    ledger: BTreeMap<String, BTreeMap<String, i128>>,
    stats: DragonStats,
    hunts: Vec<HuntRecord>,
    log: Vec<LogEntry>,
    commands: Vec<(u64, String)>,
    wire_bytes: u64,
    tunables: DragonTunables,
}

pub fn main(o: Options) -> i32 {
    let started = Instant::now();
    let phases = script();
    let (mut w, ids) = synth::world(o.seed);
    let tun = DragonTunables {
        calendar: Calendar::starting_in(Season::Autumn),
        ..Default::default()
    };
    let mut dragons = Dragons::new(o.seed, tun, Tick::ZERO);
    dragons.log_enabled = true;
    // The plugin's item dictionary for hoard records.
    dragons.define_item(1, Item::World(ids.emerald));
    dragons.define_item(2, Item::World(ids.gold));
    // The plugin's body dictionary: entity type 40 is `minecraft:cow`, a village herd's cattle.
    dragons.define_herd_body(40, ids.cattle);
    synth::populate(&mut w, &mut dragons, &synth::lairs(), Tick::ZERO);
    let mut fake = farm(o.seed, &phases);
    let mut index = DistrictIndex::default();
    let mut iaf = FakeIaf::new(PLAYER);
    let player = dragons.player(&mut w, PLAYER);
    let village_nodes: Vec<usize> = (0..w.nodes.len())
        .filter(|&i| w.nodes[i].kind.is_storehouse())
        .collect();
    let start_markets: Vec<(f64, f64)> = village_nodes
        .iter()
        .map(|&i| {
            let m = w.market(i);
            (
                m.stock(ids.cattle) as f64 / 1000.0,
                m.price(ids.cattle) as f64 / 1e6,
            )
        })
        .collect();

    let end = Tick::from_days(o.days);
    let mut t = Tick::ZERO;
    let mut phase = 0usize;
    let mut next_obs = Tick::ZERO;
    let mut robbed = false;
    let start = &phases[0];
    dragons.player_stays(
        &mut w,
        PLAYER,
        Pos::new(0, start.x, start.z),
        Tick::ZERO,
        day(phases[1].from_day),
    );
    loop {
        let phase_end = phases.get(phase + 1).map_or(end, |p| day(p.from_day));
        let rob_at = phases[phase]
            .rob
            .filter(|_| !robbed)
            .map(|_| day(phases[phase].from_day + 0.5));
        let mut next = [
            Some(end),
            dragons.next_due(),
            iaf.next_due(),
            Some(phase_end),
            Some(next_obs),
            rob_at,
        ]
        .into_iter()
        .flatten()
        .min()
        .expect("end");
        if next <= t && t > Tick::ZERO {
            next = t + 1;
        }
        if next > end {
            break;
        }
        fake.run_until(next);
        {
            let mut link = IndustryLink {
                engine: &mut fake.engine,
                index: &mut index,
            };
            for (at, ob) in iaf.due(next) {
                dragons.ingest(&mut w, &mut link, &ob, at);
            }
            dragons.run_until(&mut w, &mut link, next);
        }
        for c in dragons.take_commands() {
            if let Cmd::Materialize(m) = &c {
                let name = dragon_of(m.agent)
                    .and_then(|d| dragons.dragon(d))
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                iaf.reaction = phases[phase]
                    .react
                    .iter()
                    .find(|r| r.0 == name)
                    .map_or(Reaction::Ignore, |r| r.1);
            }
            iaf.apply(vec![c], next);
        }
        let here = Pos::new(0, phases[phase].x, phases[phase].z);
        if next >= next_obs {
            dragons.observe_player(&mut w, PLAYER, here, next);
            next_obs = next + 1_200;
        }
        if rob_at == Some(next) {
            robbed = true;
            let lair = phases[phase].rob.expect("rob");
            iaf.push(
                next,
                Obs::HoardRobbed(boundary::HoardRobbed {
                    lair,
                    op: 1 << 63 | 77,
                    player: PLAYER,
                    items: vec![(1, 20), (2, 10)],
                }),
            );
        }
        if next >= phase_end && phase + 1 < phases.len() {
            iaf.player_leaves(next);
            dragons.player_left(&mut w, player);
            phase += 1;
            let p = &phases[phase];
            iaf.village_here = synth::VILLAGES
                .iter()
                .find(|v| p.place == v.1)
                .map(|v| (v.0, 40, 2));
            let until = phases.get(phase + 1).map_or(end, |q| day(q.from_day));
            dragons.player_stays(&mut w, PLAYER, Pos::new(0, p.x, p.z), next, until);
            if !o.quiet {
                println!(
                    "{:7.2}  the player goes to {}",
                    next.0 as f64 / TICKS_PER_DAY as f64,
                    p.place
                );
            }
        }
        t = next;
        if t >= end {
            break;
        }
    }
    // Let the last commands land and the farm reconcile, with the player at home.
    let settle = end + 400;
    fake.run_until(settle);
    {
        let mut link = IndustryLink {
            engine: &mut fake.engine,
            index: &mut index,
        };
        for (at, ob) in iaf.due(settle) {
            dragons.ingest(&mut w, &mut link, &ob, at);
        }
        dragons.run_until(&mut w, &mut link, settle);
    }
    let now = w.now();

    // ---- checks ----
    let mut checks = Vec::new();
    let mut check = |name: &'static str, ok: bool, detail: String| {
        checks.push(Check { name, ok, detail });
    };
    let cons = dragons.check(&w, Some(&fake.engine.books.stolen));
    check(
        "conservation with predation",
        cons.is_ok(),
        cons.err()
            .unwrap_or_else(|| "world, ledger, hoards and industry books agree".into()),
    );
    let fc = fake.conservation();
    check(
        "industry world totals",
        fc.is_ok(),
        fc.err()
            .unwrap_or_else(|| "the farm's physical totals match the engine's books".into()),
    );
    let log = &dragons.log;
    let has = |dragon: &str, kind: &str, contains: &str| {
        log.iter().any(|l| {
            l.dragon.as_deref() == Some(dragon) && l.kind == kind && l.what.contains(contains)
        })
    };
    check(
        "a wound becomes a grudge",
        has("Cinderjaw", "hurt", "wounded by player"),
        "Cinderjaw is wounded on the road and remembers it".into(),
    );
    let farm_attacks: Vec<AttackReport> = fake
        .engine
        .district(FARM)
        .map(|d| d.attacks.iter().cloned().collect())
        .unwrap_or_default();
    let unwatched_raids = farm_attacks
        .iter()
        .filter(|a| a.skipped.is_none() && a.outcome.is_some())
        .count();
    check(
        "a grudge raid on the unwatched farm",
        unwatched_raids > 0 && log.iter().any(|l| l.kind == "raid"),
        format!("{unwatched_raids} raid(s) resolved by the industry engine, capped (I23)"),
    );
    check(
        "the slayer and the heir",
        has("Cinderjaw", "death", "killed by player") && has("Cinderjaw", "heir", "Emberling"),
        "Cinderjaw dies at the player's hands; its grudges pass to Emberling".into(),
    );
    let emberling_raid = log.iter().any(|l| {
        l.dragon.as_deref() == Some("Emberling")
            && (l.kind == "raid" || (l.kind == "hunt" && l.what.contains("grudge")))
    });
    check(
        "the heir hunts the player",
        emberling_raid,
        "Emberling, holding its parent's grudge, goes for the player's home".into(),
    );
    check(
        "threats get bored",
        dragons.stats.bored > 0,
        format!("{} bored departure(s)", dragons.stats.bored),
    );
    let share = |s: Season| {
        let v = dragons.prey_by_season[s.index()];
        let total = v[PreyKind::Road.index()] + v[PreyKind::Village.index()];
        if total == 0 {
            0.0
        } else {
            v[PreyKind::Village.index()] as f64 / total as f64
        }
    };
    let (autumn, winter) = (share(Season::Autumn), share(Season::Winter));
    check(
        "winter shifts the prey",
        winter > autumn + 0.2,
        format!(
            "villages' share of hunts: autumn {:.0}%, winter {:.0}%",
            autumn * 100.0,
            winter * 100.0
        ),
    );

    // ---- output ----
    let seasons = [Season::Autumn, Season::Winter]
        .into_iter()
        .map(|s| SeasonOut {
            season: s,
            hunts: PreyKind::ALL
                .iter()
                .map(|k| (k.name(), dragons.prey_by_season[s.index()][k.index()]))
                .collect(),
            fullness: PreyKind::ALL
                .iter()
                .map(|k| (k.name(), dragons.food_by_season[s.index()][k.index()]))
                .collect(),
            village_share: share(s),
        })
        .collect();
    let dragon_out = dragons
        .dragons
        .iter()
        .map(|(_, x)| {
            let sp = dragons.tun.species(x.species);
            let feel = w.knowledge.sentiment(x.actor, player, now);
            DragonOut {
                name: x.name.clone(),
                species: x.species.name(),
                alive: true,
                age_days: x.age(now) as f64 / TICKS_PER_DAY as f64,
                stage: x.stage(sp, now).0,
                lair: x.lair.map(|l| dragons.lairs[l as usize].name.clone()),
                fullness: x.fullness_at(&dragons.tun, now),
                health: x.health_at(sp, now),
                feeling_for_player: feel,
                holds_grudge: feel <= dragons.tun.grudge_below,
            }
        })
        .collect();
    let item_name = |i: &Item| match i {
        Item::World(g) => w.goods.get(*g).name.clone(),
        Item::Industry(g) => fake
            .goods
            .get(g)
            .map_or(format!("good {}", g.0), |x| x.name.clone()),
    };
    let lairs = dragons
        .lairs
        .iter()
        .map(|l| LairOut {
            name: l.name.clone(),
            species: l.species.name(),
            kind: match l.kind {
                LairKind::Roost => "roost",
                LairKind::Cave => "cave",
            },
            occupants: l
                .occupants
                .iter()
                .filter_map(|d| dragons.dragon(*d).map(|x| x.name.clone()))
                .collect(),
            hoard: l.hoard.iter().map(|(k, v)| (item_name(k), *v)).collect(),
        })
        .collect();
    let villages = village_nodes
        .iter()
        .zip(&start_markets)
        .map(|(&i, &(s0, p0))| {
            let m = w.market(i);
            VillageOut {
                name: synth::village_name(w.nodes[i].key).to_string(),
                feeling_for_player: w.knowledge.sentiment(w.nodes[i].actor, player, now),
                cattle_stock_start: s0,
                cattle_stock_end: m.stock(ids.cattle) as f64 / 1000.0,
                cattle_price_start: p0,
                cattle_price_end: m.price(ids.cattle) as f64 / 1e6,
            }
        })
        .collect();
    let mut ledger = BTreeMap::new();
    for (name, m) in [
        ("taken", &dragons.ledger.taken),
        ("eaten", &dragons.ledger.eaten),
        ("burnt", &dragons.ledger.burnt),
        ("hoarded", &dragons.ledger.hoarded),
        ("robbed", &dragons.ledger.robbed),
    ] {
        ledger.insert(
            name.to_string(),
            m.iter()
                .map(|(k, v)| (item_name(k), *v))
                .collect::<BTreeMap<_, _>>(),
        );
    }
    let destroyed_applied = fake.districts[0].broken.len();
    let ok = checks.iter().all(|c| c.ok);
    let out = Out {
        seed: o.seed,
        days: o.days,
        starts_in: Season::Autumn,
        wall_ms: started.elapsed().as_secs_f64() * 1000.0,
        checks,
        seasons,
        dragons: dragon_out,
        lairs,
        villages,
        farm_attacks,
        farm_destroyed_blocks_applied: destroyed_applied,
        ledger,
        stats: dragons.stats,
        hunts: dragons.hunts.clone(),
        log: dragons.log.clone(),
        commands: iaf.commands.clone(),
        wire_bytes: iaf.wire_bytes,
        tunables: dragons.tun.clone(),
    };
    if !o.quiet {
        print_report(&out, &w);
    }
    if let Some(path) = &o.out {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(path, serde_json::to_string_pretty(&out).expect("serialize")) {
            Ok(()) => println!("wrote {}", path.display()),
            Err(e) => {
                eprintln!("could not write {}: {e}", path.display());
                return 1;
            }
        }
    }
    if ok { 0 } else { 1 }
}

fn name_villages(s: &str) -> String {
    let mut s = s.to_string();
    for (k, n) in synth::VILLAGES {
        s = s.replace(&format!("village {k}"), n);
    }
    let mut out = String::new();
    // "road a-b" with village keys.
    let mut rest = s.as_str();
    while let Some(i) = rest.find("road ") {
        out.push_str(&rest[..i + 5]);
        rest = &rest[i + 5..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '-'))
            .unwrap_or(rest.len());
        let names: Vec<String> = rest[..end]
            .split('-')
            .filter_map(|k| k.parse::<u64>().ok())
            .map(|k| synth::village_name(k).to_string())
            .collect();
        if names.len() == 2 {
            out.push_str(&format!("{} to {}", names[0], names[1]));
        } else {
            out.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn print_report(o: &Out, _w: &World) {
    println!("\n== what happened ==");
    for l in &o.log {
        if l.kind == "hunt" {
            continue;
        }
        println!(
            "{:7.2}  {:<12} {:<11} {}",
            l.day,
            l.kind,
            l.dragon
                .clone()
                .unwrap_or_default()
                .chars()
                .take(11)
                .collect::<String>(),
            name_villages(&l.what)
        );
    }
    println!("\n== prey by season (hunts that fed, and fullness gained) ==");
    for s in &o.seasons {
        println!(
            "{:<7} road {:>3} ({:>4}), village {:>3} ({:>4}), grudge {:>3} ({:>4}), train {:>3} ({:>4}); villages {:.0}% of hunts",
            s.season.name(),
            s.hunts["road"],
            s.fullness["road"],
            s.hunts["village"],
            s.fullness["village"],
            s.hunts["grudge"],
            s.fullness["grudge"],
            s.hunts["train"],
            s.fullness["train"],
            s.village_share * 100.0
        );
    }
    println!("\n== dragons ==");
    for d in &o.dragons {
        println!(
            "{:<38} {:<9} {:5.1} days stage {} fullness {:>3} health {:>5.0} feeling for the player {:>4}{}",
            d.name,
            d.species,
            d.age_days,
            d.stage,
            d.fullness,
            d.health,
            d.feeling_for_player,
            if d.holds_grudge { " (grudge)" } else { "" }
        );
    }
    println!("\n== villages (cattle stock and price, and their feeling for the player) ==");
    for v in &o.villages {
        println!(
            "{:<11} cattle {:>6.1} -> {:>6.1}, price {:>7.3} -> {:>7.3}, feeling {:>4}",
            v.name,
            v.cattle_stock_start,
            v.cattle_stock_end,
            v.cattle_price_start,
            v.cattle_price_end,
            v.feeling_for_player
        );
    }
    println!("\n== the farm (industry district) ==");
    for a in &o.farm_attacks {
        println!(
            "{:7.2}  {:?} {:?} x{}: survived {:.2}, stole {:?}, destroyed {} blocks{}{}",
            a.at.0 as f64 / TICKS_PER_DAY as f64,
            a.source,
            a.class,
            a.count,
            a.outcome.map_or(0.0, |x| x.survived),
            a.stolen.iter().collect::<Vec<_>>(),
            a.destroyed,
            if a.machine_lost { ", machine lost" } else { "" },
            a.skipped
                .map_or(String::new(), |s| format!(", skipped: {s}"))
        );
    }
    println!("blocks broken on load: {}", o.farm_destroyed_blocks_applied);
    println!("\n== ledger ==");
    for (k, v) in &o.ledger {
        println!("{k:<8} {v:?}");
    }
    println!("\n{:?}", o.stats);
    println!("\n== checks ==");
    for c in &o.checks {
        println!(
            "{} {}: {}",
            if c.ok { "ok  " } else { "FAIL" },
            c.name,
            c.detail
        );
    }
    println!("wall {:.0} ms, {} wire bytes", o.wall_ms, o.wire_bytes);
}
