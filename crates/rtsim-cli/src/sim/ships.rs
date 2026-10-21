// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim ships`: the ships engine (PLAN.md §10.1; C5, C24, C25; M2) on a synthetic coast,
//! headless.
//!
//! **The world** (`rtsim_ships::synth::coastal`): six fishing villages along a wavy shore and one
//! on an island have sea ports; a mountain village, an inland town and three of the ports have
//! sky docks; a pirate haven on the island's far side keeps a sea and a sky pirate ship; a navy
//! patrols out of Saltmere. Lanes are world edges, so merchant ships carry the phase 1 economy's
//! flows (A13).
//!
//! **The plugin** is `rtsim_ships::synth::FakeShips`: every command goes through the boundary
//! codec (docs/boundary.md §7); it moves bodies by the timetable it is given, releases them when
//! they leave every player's view, and plays fights.
//!
//! **The scripted player** watches a sea lane for two days (ships enter its view), then follows
//! the lane the pirates hunt (a raid in view, loot sailing to the haven), loots a merchant's hold
//! on day 7, and sinks the first pirate it meets after day 9.
//!
//! **What it checks** (exit 1 on a failure): conservation every hour (the world's books and the
//! ships' holds against the world's held goods), a ship seen by the player, a raid in view, stolen
//! goods reaching the haven's market, a patrol lowering a lane's hazard, and a restart in the
//! middle continuing with the same state hashes as the run that never stopped.
//!
//! It also counts the event archive's rows (R, VIEWER.md §10.4) of the main run: the world's two
//! tapes and the ships' are on. The restored twin beside it records nothing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_archive::record::Meter;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_ships::engine::{LogEntry, ShipStats};
use rtsim_ships::synth::{self, Coastal, FakeShips, Script};
use rtsim_ships::timetable::ServiceKind;
use rtsim_ships::{Role, ShipTunables, Ships, fight};
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;
use serde::Serialize;

use super::archive::{Rate, drain_world, world_on};

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub out: Option<PathBuf>,
    pub quiet: bool,
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let mut o = Options {
        days: 12,
        seed: 7,
        out: Some(PathBuf::from("scenario/results/sim-ships.json")),
        quiet: false,
    };
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--days" => match num(it.next()) {
                Some(n) if n >= 10 => o.days = n,
                _ => return bad("--days needs a number of at least 10"),
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
        "{why}\nusage: rtsim-cli sim ships [--days N] [--seed S] [--out FILE | --no-write] [--quiet]"
    );
    ExitCode::from(64)
}

#[derive(Serialize)]
struct DockRow {
    id: u32,
    port: String,
    medium: &'static str,
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Serialize)]
struct LaneRow {
    id: u32,
    medium: &'static str,
    from: String,
    to: String,
    blocks: u64,
    corners: usize,
    world_edge: u32,
    /// The lane's hold capacity, chest slots a day each way.
    capacity_slots_per_day: f64,
}

#[derive(Serialize)]
struct ServiceRow {
    id: u32,
    kind: String,
    medium: &'static str,
    stops: Vec<String>,
    period_ticks: u64,
    period_minutes: f64,
    ships: u32,
    cruise_y: i32,
}

#[derive(Serialize)]
struct TimetableRow {
    service: u32,
    ship: String,
    clock_offset: u64,
    /// Next departures: (from, day).
    departures: Vec<(String, f64)>,
}

#[derive(Serialize)]
struct PatrolRow {
    lane: u32,
    patrol_strength: f64,
    /// The hazard one of the haven's sea pirate ships would put on the lane, thousandths: with
    /// the patrol, and with none.
    hazard_with_patrol: i64,
    hazard_without: i64,
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct Results {
    seed: u64,
    days: u64,
    wall_ms: f64,
    docks: Vec<DockRow>,
    lanes: Vec<LaneRow>,
    services: Vec<ServiceRow>,
    lints: String,
    timetables: Vec<TimetableRow>,
    patrols: Vec<PatrolRow>,
    log: Vec<LogEntry>,
    plugin_log: Vec<String>,
    stats: ShipStats,
    looted_by_pirates: BTreeMap<String, f64>,
    landed_at_havens: BTreeMap<String, f64>,
    landed_at_ports: BTreeMap<String, f64>,
    taken_by_players: BTreeMap<String, f64>,
    sunk: BTreeMap<String, f64>,
    /// The haven's market: (good, stock at the start, stock at the end, received by raid
    /// pipelines and landings), whole items.
    haven_market: Vec<(String, f64, f64, f64)>,
    plugin_commands: BTreeMap<String, u64>,
    checks: Vec<Check>,
    /// The archive rows the main run's engines took per game day.
    archive: Rate,
}

fn names(w: &World) -> Vec<&'static str> {
    let n = synth::names();
    (0..w.nodes.len())
        .map(|i| n.get(i).copied().unwrap_or("?"))
        .collect()
}

fn by_name(w: &World, m: &BTreeMap<GoodId, i128>) -> BTreeMap<String, f64> {
    m.iter()
        .filter(|(_, a)| **a != 0)
        .map(|(g, a)| (w.goods.get(*g).name.clone(), *a as f64 / MILLI as f64))
        .collect()
}

/// Counts and empties the main run's tapes. The script runs each half in one call (a run cut
/// off the hour changes its steps once the player has logged off, and the restored twin must match
/// it), so this runs at the half and at the end, not every game day; the ships make a handful of
/// rows a day, far under a tape's cap, and `dropped` would show it if not.
fn drain(m: &mut Meter, c: &mut Coastal) {
    drain_world(m, &mut c.world);
    m.drain(&mut c.ships.tape);
}

pub fn main(o: Options) -> i32 {
    let started = Instant::now();
    let tun = ShipTunables::default();
    let mut c = synth::coastal(o.seed, tun.clone());
    c.ships.log_enabled = true;
    world_on(&mut c.world);
    c.ships.tape.set_on(true);
    let mut meter = Meter::new();
    let nm = names(&c.world);
    let haven_node = c.ships.havens[c.haven as usize].node;
    let start_haven = c.world.market(haven_node);
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let end = Tick::from_days(o.days);
    let mid = Tick::from_days(o.days / 2);
    let mut script = Script::default();
    script.run(&mut c, &mut fake, Tick::ZERO, mid);
    drain(&mut meter, &mut c);
    // A restart in the middle: a copy restored from the saves runs the rest of the script beside
    // the engine that never stopped.
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let restored = World::load(synth::world_config(o.seed), goods, book, &c.world.save())
        .ok()
        .zip(Ships::load(tun.clone(), &c.ships.save()).ok());
    let mut twin = restored.map(|(w, s)| {
        (
            Coastal {
                world: w,
                ships: s,
                mask: synth::coast(),
                haven: c.haven,
                navy: c.navy,
            },
            {
                let mut f = FakeShips::new(fake.view);
                f.bodies = fake.bodies.clone();
                f.seq = fake.seq;
                f
            },
            Script {
                conserved: Vec::new(),
                ..script.clone()
            },
        )
    });
    script.run(&mut c, &mut fake, mid, end);
    drain(&mut meter, &mut c);
    let conserved = script.conserved;
    let mut twin_conserved = Vec::new();
    let restart_ok = match twin.as_mut() {
        Some((tc, tf, ts)) => {
            tc.ships.log_enabled = true;
            ts.run(tc, tf, mid, end);
            twin_conserved = std::mem::take(&mut ts.conserved);
            tc.ships.state_hash() == c.ships.state_hash()
                && tc.world.state_hash() == c.world.state_hash()
        }
        None => false,
    };

    let s = &c.ships;
    let w = &c.world;
    let q = w.cfg.trade.quantum;
    let docks: Vec<DockRow> = s
        .docks
        .iter()
        .enumerate()
        .map(|(i, d)| DockRow {
            id: i as u32,
            port: nm[d.node].to_string(),
            medium: d.medium.name(),
            x: d.x,
            y: d.y,
            z: d.z,
        })
        .collect();
    let lanes: Vec<LaneRow> = s
        .lanes
        .iter()
        .enumerate()
        .map(|(i, l)| LaneRow {
            id: i as u32,
            medium: l.medium.name(),
            from: nm[s.docks[l.a as usize].node].to_string(),
            to: nm[s.docks[l.b as usize].node].to_string(),
            blocks: l.path.length(),
            corners: l.path.pts.len().saturating_sub(2),
            world_edge: l.edge,
            capacity_slots_per_day: w.edges[l.edge as usize].cap_slot_milli as f64 / 1000.0
                * q.per_day_count() as f64,
        })
        .collect();
    let services: Vec<ServiceRow> = s
        .services
        .iter()
        .enumerate()
        .map(|(i, sv)| ServiceRow {
            id: i as u32,
            kind: format!("{:?}", sv.kind).to_lowercase(),
            medium: sv.medium.name(),
            stops: sv
                .legs
                .iter()
                .map(|g| nm[s.docks[g.from_dock as usize].node].to_string())
                .collect(),
            period_ticks: sv.period,
            period_minutes: sv.period as f64 / 1200.0,
            ships: sv.ships,
            cruise_y: sv.cruise_y,
        })
        .collect();
    let mut timetables = Vec::new();
    for (i, sv) in s.services.iter().enumerate() {
        if sv.kind != ServiceKind::Merchant {
            continue;
        }
        for (id, deps) in s.departures(i as u32, end, 3) {
            timetables.push(TimetableRow {
                service: i as u32,
                ship: s.ship_name(id),
                clock_offset: s.ship(id).map_or(0, |x| x.offset),
                departures: deps
                    .into_iter()
                    .map(|(_, node, t)| (nm[node].to_string(), t as f64 / TICKS_PER_DAY as f64))
                    .collect(),
            });
        }
    }
    let sea_pirate = tun.sea.pirate_crew as f64 * tun.pirate_strength;
    let mut patrols = Vec::new();
    for (l, th) in s.lane_threats() {
        if th.patrol_milli > 0 {
            let n = th.patrol_milli as f64 / 1000.0;
            patrols.push(PatrolRow {
                lane: l,
                patrol_strength: n,
                hazard_with_patrol: fight::hazard_milli(sea_pirate, n, &tun),
                hazard_without: fight::hazard_milli(sea_pirate, 0.0, &tun),
            });
        }
    }
    let end_haven = w.market(haven_node);
    let haven_market: Vec<(String, f64, f64, f64)> = (0..w.goods.len())
        .map(|gi| {
            let g = GoodId(gi as u16);
            let (a, b) = (start_haven.line(g), end_haven.line(g));
            (
                w.goods.get(g).name.clone(),
                a.stock as f64 / 1000.0,
                b.stock as f64 / 1000.0,
                (b.book.arrived + b.book.lump_in - a.book.arrived - a.book.lump_in) as f64 / 1000.0,
            )
        })
        .filter(|r| r.3 != 0.0 || r.1 != r.2)
        .collect();
    let received: f64 = haven_market.iter().map(|r| r.3).sum();
    let st = s.stats;
    let patrol_lowers = patrols
        .iter()
        .any(|p| p.hazard_with_patrol < p.hazard_without);
    // A live hazard lowered: the escorted lane's share while escorted, against before.
    let escorted = s
        .log
        .iter()
        .filter(|e| e.kind == "hazard" && !e.what.ends_with("(patrols and escorts 0.0)"))
        .count();
    let checks = vec![
        Check {
            name: "conservation every hour",
            ok: conserved.is_empty() && twin_conserved.is_empty(),
            detail: conserved
                .first()
                .or(twin_conserved.first())
                .cloned()
                .unwrap_or_else(|| "world books and ships' holds balance".into()),
        },
        Check {
            name: "a ship entered the player's view",
            ok: st.seen > 0,
            detail: format!("{} entered view, {} materialized", st.seen, st.materialized),
        },
        Check {
            name: "a raid in view",
            ok: st.raids_observed > 0,
            detail: format!(
                "{} raids in view, {} unobserved, {} fights the plugin played",
                st.raids_observed, st.raids_abstract, st.fights_observed
            ),
        },
        Check {
            name: "an unobserved fight resolved by Lanchester",
            ok: st.raids_abstract > 0,
            detail: format!(
                "{} raids resolved with nobody watching (the player logged off before one, after day 7)",
                st.raids_abstract
            ),
        },
        Check {
            name: "stolen goods reached the haven's market",
            ok: received > 0.0,
            detail: format!("{received:.1} items received by the haven's market"),
        },
        Check {
            name: "a patrol lowers a lane's hazard",
            ok: patrol_lowers && escorted > 0,
            detail: format!(
                "{} patrolled lanes, each cutting a sea pirate's hazard; {escorted} live hazard changes with a patrol or escort present (the player's escort on day 5)",
                patrols.len(),
            ),
        },
        Check {
            name: "a restart in the middle continues exactly",
            ok: restart_ok,
            detail: format!(
                "restored at day {} and run to day {}: ships {:016x}, world {:016x}",
                o.days / 2,
                o.days,
                s.state_hash(),
                w.state_hash()
            ),
        },
    ];
    let failed = checks.iter().any(|c| !c.ok);
    let res = Results {
        seed: o.seed,
        days: o.days,
        wall_ms: started.elapsed().as_secs_f64() * 1e3,
        docks,
        lanes,
        services,
        lints: format!("{:?}; over berths {:?}", s.lints, s.over_berths),
        timetables,
        patrols,
        log: s.log.clone(),
        plugin_log: fake.log.clone(),
        stats: st,
        looted_by_pirates: by_name(w, &s.ledger.looted_by_pirates),
        landed_at_havens: by_name(w, &s.ledger.landed_at_havens),
        landed_at_ports: by_name(w, &s.ledger.landed_at_ports),
        taken_by_players: by_name(w, &s.ledger.taken_by_players),
        sunk: by_name(w, &s.ledger.sunk),
        haven_market,
        plugin_commands: fake
            .commands
            .iter()
            .map(|(k, v)| (format!("{k:#06x}"), *v))
            .collect(),
        checks,
        archive: Rate::new(&meter, o.days as f64),
    };
    if !o.quiet {
        print_report(&res, s);
        res.archive.print(None);
    }
    if let Some(path) = &o.out {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(path, serde_json::to_string_pretty(&res).expect("serialize")) {
            Ok(()) => {
                if !o.quiet {
                    println!("\nwrote {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("cannot write {}: {e}", path.display());
                return 1;
            }
        }
    }
    i32::from(failed)
}

fn print_report(r: &Results, s: &Ships) {
    println!(
        "sim ships: seed {}, {} days, {:.0} ms\n",
        r.seed, r.days, r.wall_ms
    );
    println!("docks:");
    for d in &r.docks {
        println!(
            "  {:>2} {:<12} {:<4} ({}, {}, {})",
            d.id, d.port, d.medium, d.x, d.y, d.z
        );
    }
    println!("\nlanes (world edges carrying the ports' flows):");
    for l in &r.lanes {
        println!(
            "  {:>2} {:<4} {:<12} -> {:<12} {:>5} blocks, {} corners, edge {}, hold capacity {:.0} slots/day",
            l.id,
            l.medium,
            l.from,
            l.to,
            l.blocks,
            l.corners,
            l.world_edge,
            l.capacity_slots_per_day
        );
    }
    println!("\nservices (closed loops; each ship's position is a function of the clock):");
    for sv in &r.services {
        println!(
            "  {:>2} {:<8} {:<4} {} ships, period {:.1} min, cruise y {}: {}",
            sv.id,
            sv.kind,
            sv.medium,
            sv.ships,
            sv.period_minutes,
            sv.cruise_y,
            sv.stops.join(" -> ")
        );
    }
    println!("  lints: {}", r.lints);
    println!("\ntimetables at the end (saved clock offsets; next departures, day):");
    for t in r
        .timetables
        .iter()
        .filter(|t| t.service < 2 || t.service == 4)
    {
        let deps: Vec<String> = t
            .departures
            .iter()
            .map(|(p, d)| format!("{p} {d:.3}"))
            .collect();
        println!(
            "  {:<22} offset {:>6}: {}",
            t.ship,
            t.clock_offset,
            deps.join(", ")
        );
    }
    println!("\npatrols (M2: a patrol lowers the hazard a sea pirate ship puts on a lane):");
    for p in &r.patrols {
        println!(
            "  lane {:>2}: patrol strength {:.2}: hazard {}‰ with it, {}‰ without",
            p.lane, p.patrol_strength, p.hazard_with_patrol, p.hazard_without
        );
    }
    println!("\nwhat happened:");
    for e in &r.log {
        if e.kind == "seen" && !e.what.contains("pirate") && !e.what.contains("attacking") {
            continue;
        }
        println!(
            "  day {:>6.2} {:<10} {:<22} {}",
            e.day,
            e.kind,
            e.ship.clone().unwrap_or_default(),
            e.what
        );
    }
    let seen = r.log.iter().filter(|e| e.kind == "seen").count();
    println!("  ({seen} ships entered the player's view; the first five:)");
    for e in r.log.iter().filter(|e| e.kind == "seen").take(5) {
        println!(
            "  day {:>6.2} {:<22} {}",
            e.day,
            e.ship.clone().unwrap_or_default(),
            e.what
        );
    }
    println!("\nthe plugin:");
    for l in &r.plugin_log {
        println!("  {l}");
    }
    println!("  commands by kind: {:?}", r.plugin_commands);
    println!("\nstolen goods (whole items):");
    println!("  taken from holds by pirates: {:?}", r.looted_by_pirates);
    println!("  landed at the haven's market: {:?}", r.landed_at_havens);
    println!("  taken by the player: {:?}", r.taken_by_players);
    println!("  lost with sunk ships: {:?}", r.sunk);
    println!("  the haven's market (good, stock start, stock end, received):");
    for h in &r.haven_market {
        println!("    {:<14} {:>8.1} {:>8.1} {:>8.1}", h.0, h.1, h.2, h.3);
    }
    let pirates: Vec<String> = s
        .havens
        .iter()
        .flat_map(|h| h.ships.iter())
        .filter_map(|&p| {
            let x = s.ship(p)?;
            matches!(x.role, Role::Pirate { .. }).then(|| {
                format!(
                    "{} crew {} hull {}‰ {:?}",
                    s.ship_name(p),
                    x.crew,
                    x.hull_milli,
                    x.status
                )
            })
        })
        .collect();
    println!("\npirates: {}", pirates.join("; "));
    println!("stats: {:?}", r.stats);
    println!("pending events by kind: {:?}", s.pending_by_kind());
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
