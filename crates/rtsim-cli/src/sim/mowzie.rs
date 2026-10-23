// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim mowzie`: MOWZIE.md §7's runs for Mowzie's Mobs' two factions, headless.
//!
//! **The scaffold** (slice M2). Each run is numbered as §7 numbers it. A run whose pieces are not
//! built yet prints the slice that builds them. M2 builds the parts of two runs that need only the
//! cultures as data, on the phase 1 synthetic world with the camp joined at Gloomhollow's place
//! (`rtsim_conflict::synth::CAMP_POS`):
//! - **Run 1, the den's ladder:** a Frostmaw den (a lair, MOWZIE.md ledger 13) angered by kills
//!   in its range, standing in for the rail meter (M7), until its grievance sits between Raid and
//!   Invasion; then nothing. It goes from Notice straight to Raid (it has no Envoy or Scouts,
//!   ledger 2), comes again `repeat_days` after each band turns for home (ledger 3), and calms back
//!   to Notice.
//! - **Run 5, the grove's ladder:** an Umvuthana grove pushed past Invasion. It goes Notice, Envoy
//!   (a demand in gold nuggets at the fallback of 9 an emerald, as no market prices gold yet,
//!   ledger 12), Scouts, Raid and Invasion, and never Siege (it has no siege party).
//!
//! Exit 1 if a check fails, 64 on bad arguments.

use std::path::PathBuf;
use std::process::ExitCode;

use rtsim_conflict::culture::{PartyKind, SignalKind};
use rtsim_conflict::engine::{Conflict, ConflictStats, LogEntry, Signal};
use rtsim_conflict::{Step, synth};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::{ActorId, Temperament, Trait};
use rtsim_settle::NodeKind;
use rtsim_world::World;
use serde::Serialize;

const HOUR: u64 = TICKS_PER_DAY / 24;

/// §7's runs: number, what it shows, what is built of it so far, and the slices that build the
/// rest.
const RUNS: [(u32, &str, &str, &str); 13] = [
    (
        1,
        "a line through a range: notice, raid, repeat, invasion; ambushes; a kill and the heir",
        "M2, the ladder on kills",
        "the meter M7, ambushes M8, the kill and the body M6",
    ),
    (2, "cut service and stopped service", "", "M7"),
    (3, "reroute", "", "M7"),
    (4, "two dens on one line", "", "M7"),
    (
        5,
        "the tribe: envoy, raids, invasion, plunder, the grove in and out",
        "M2, the ladder and the priced demand",
        "the rest M9",
    ),
    (6, "renewal of dens and groves", "", "M6 (den), M9 (grove)"),
    (7, "kills and which feud steps down", "", "M6"),
    (
        8,
        "save and restore mid-lurk and mid-reconcile",
        "",
        "M8, M4",
    ),
    (9, "a trunk of 14 lines", "", "M7"),
    (10, "the camp reconciler", "", "M4"),
    (11, "an Invasion with its chief", "", "M10, M11"),
    (12, "the heir farmed for 60 years", "", "M9"),
    (13, "the march record", "", "M10"),
];

struct Opts {
    runs: Vec<u32>,
    seed: u64,
    out: Option<PathBuf>,
    quiet: bool,
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let mut o = Opts {
        runs: Vec::new(),
        seed: 7,
        out: Some(PathBuf::from("scenario/results/sim-mowzie.json")),
        quiet: false,
    };
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "list" => {
                list();
                return ExitCode::SUCCESS;
            }
            "run" => match num(it.next()) {
                Some(n) if (1..=RUNS.len() as u64).contains(&n) => o.runs.push(n as u32),
                _ => return bad(&format!("run needs a number from 1 to {}", RUNS.len())),
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
    if o.runs.is_empty() {
        o.runs = RUNS.iter().map(|r| r.0).collect();
    }
    ExitCode::from(main(o) as u8)
}

fn bad(why: &str) -> ExitCode {
    eprintln!(
        "{why}\nusage: rtsim-cli sim mowzie [list | run N ...] [--seed S] [--out FILE | --no-write] [--quiet]"
    );
    ExitCode::from(64)
}

fn list() {
    for (n, what, built, rest) in RUNS {
        if built.is_empty() {
            println!("{n:>2}  {what}\n    built by: {rest}");
        } else {
            println!("{n:>2}  {what}\n    built: {built}; still to build: {rest}");
        }
    }
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct RunOut {
    run: u32,
    /// The whole run is built: no slice is left to build.
    built: bool,
    /// The slices that build what is not built yet.
    slice: &'static str,
    checks: Vec<Check>,
    /// The feud's rungs in order, with the day each began.
    rungs: Vec<(f64, Step)>,
    stats: Option<ConflictStats>,
    log: Vec<LogEntry>,
}

#[derive(Serialize)]
struct Results {
    seed: u64,
    runs: Vec<RunOut>,
}

fn main(o: Opts) -> i32 {
    let mut runs = Vec::new();
    for n in &o.runs {
        let (_, _, _, slice) = RUNS[*n as usize - 1];
        let out = match n {
            1 => den_ladder(o.seed, slice),
            5 => grove_ladder(o.seed, slice),
            _ => RunOut {
                run: *n,
                built: false,
                slice,
                checks: Vec::new(),
                rungs: Vec::new(),
                stats: None,
                log: Vec::new(),
            },
        };
        runs.push(out);
    }
    let res = Results { seed: o.seed, runs };
    print_report(&res, o.quiet);
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
    let failed = res.runs.iter().flat_map(|r| &r.checks).any(|c| !c.ok);
    i32::from(failed)
}

fn print_report(r: &Results, quiet: bool) {
    for run in &r.runs {
        let (_, what, _, _) = RUNS[run.run as usize - 1];
        println!("\nrun {}: {what}", run.run);
        if !run.built && run.checks.is_empty() {
            println!("  not built yet: {}", run.slice);
            continue;
        }
        if !quiet {
            for l in &run.log {
                println!("  {:7.2}  {:<9} {}", l.day, l.kind, l.what);
            }
            let rungs: Vec<String> = run
                .rungs
                .iter()
                .map(|(d, s)| format!("{} (day {d:.2})", s.name()))
                .collect();
            println!("  rungs: {}", rungs.join(", "));
        }
        for c in &run.checks {
            println!(
                "  [{}] {}: {}",
                if c.ok { "ok" } else { "FAIL" },
                c.name,
                c.detail
            );
        }
        println!("  still to build: {}", run.slice);
    }
}

/// A conflict engine on the built-in cultures with one camp of `culture` at Gloomhollow's place.
fn setup(seed: u64, culture: &str) -> (World, Conflict, u32, ActorId) {
    let (mut w, _) = synth::world(seed);
    let mut c = synth::conflict(seed, &w);
    c.log_enabled = true;
    let f = c
        .add_faction(&mut w, synth::CAMP, culture, synth::CAMP_POS, Tick::ZERO)
        .expect("a built-in culture");
    let p = c.player(&mut w, synth::PLAYER);
    (w, c, f, p)
}

/// Rung `s`'s threshold for faction `f`'s chief, as the engine bends it by temperament.
fn theta(c: &Conflict, f: u32, s: Step) -> f64 {
    let x = &c.factions[f as usize];
    let bend = Temperament::from_seed(x.chief_seed)
        .bend(Trait::Aggressive, 1000, 40)
        .max(1);
    c.cultures.get(x.culture).threshold(s) * 1000.0 / f64::from(bend)
}

/// What a feed saw of the feud, hour by hour.
#[derive(Default)]
struct Trace {
    /// Each new rung with the day it was seen.
    rungs: Vec<(f64, Step)>,
    /// The days its parties turned for home (the feud's `since` once no party is out for it).
    turns: Vec<f64>,
    out: bool,
}

/// Kills in the range each day from `from` to `to`, enough that the grievance settles near
/// `peak` (half-life 6 days; nothing for 0), the engine run an hour at a time.
#[allow(clippy::too_many_arguments)]
fn feed(
    c: &mut Conflict,
    w: &mut World,
    f: u32,
    a: ActorId,
    peak: f64,
    from: Tick,
    to: Tick,
    tr: &mut Trace,
) {
    let amount = peak * (1.0 - 0.5f64.powf(1.0 / 6.0)) / 8.0;
    let mut t = from;
    while t < to {
        if (t.0 - from.0).is_multiple_of(TICKS_PER_DAY) && amount > 0.0 {
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
                t,
            );
        }
        t = Tick(t.0 + HOUR);
        c.run_until(w, &mut NoInfrastructure, t);
        let _ = c.take_commands();
        let x = c.feud(f, a);
        let s = x.map_or(Step::None, |x| x.step);
        if tr.rungs.last().map(|r| r.1) != Some(s) {
            tr.rungs.push((t.0 as f64 / TICKS_PER_DAY as f64, s));
        }
        let out = x.is_some_and(|x| x.party.is_some());
        if tr.out
            && !out
            && let Some(x) = x
        {
            tr.turns.push(x.since.0 as f64 / TICKS_PER_DAY as f64);
        }
        tr.out = out;
    }
}

fn check(out: &mut Vec<Check>, name: &'static str, ok: bool, detail: String) {
    out.push(Check { name, ok, detail });
}

fn days(c: &Conflict, kind: &str) -> Vec<f64> {
    c.log
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.day)
        .collect()
}

/// Run 1's M2 part: the Frostmaw den's ladder on kills.
fn den_ladder(seed: u64, slice: &'static str) -> RunOut {
    let (mut w, mut c, f, p) = setup(seed, "frostmaw");
    let node = c.factions[f as usize].node;
    let raid = theta(&c, f, Step::Raid);
    let mut tr = Trace::default();
    feed(
        &mut c,
        &mut w,
        f,
        p,
        1.5 * raid,
        Tick(10),
        Tick::from_days(60),
        &mut tr,
    );
    feed(
        &mut c,
        &mut w,
        f,
        p,
        0.0,
        Tick::from_days(60),
        Tick::from_days(160),
        &mut tr,
    );
    let st = c.stats;
    let mut out = Vec::new();
    check(
        &mut out,
        "the den is a lair",
        w.nodes[node].kind == NodeKind::Lair && !w.market(node).is_storehouse(),
        format!("node {node}: {:?}", w.nodes[node].kind),
    );
    let order: Vec<Step> = tr.rungs.iter().map(|r| r.1).collect();
    check(
        &mut out,
        "Notice straight to Raid, back to Notice, then nothing",
        order.starts_with(&[Step::Notice, Step::Raid, Step::Notice, Step::None])
            && st.escalations[Step::Envoy.index()] == 0
            && st.escalations[Step::Scouts.index()] == 0,
        format!(
            "{order:?}; Raid's threshold {raid:.1}, the grievance held near {:.1}",
            1.5 * raid
        ),
    );
    let again = days(&c, "repeat");
    let turns = &tr.turns;
    let gaps: Vec<f64> = turns.iter().zip(&again).map(|(h, r)| r - h).collect();
    check(
        &mut out,
        "the band comes again 4 days after it turns for home",
        st.repeats >= 3
            && turns.len() > again.len()
            && st.parties[PartyKind::Raid.index()] == 1 + st.repeats
            && gaps.iter().all(|g| (4.0 - 1e-6..4.0 + 1e-3).contains(g)),
        format!(
            "{} repeat(s), {} raid(s); days between turning home and coming again: {:?}",
            st.repeats,
            st.parties[PartyKind::Raid.index()],
            gaps.iter().map(|g| format!("{g:.2}")).collect::<Vec<_>>()
        ),
    );
    check(
        &mut out,
        "never an Invasion below its threshold, never a demand",
        st.escalations[Step::Invasion.index()] == 0 && st.demands == 0,
        format!("Invasion's threshold {:.1}", 2.0 * raid),
    );
    check(
        &mut out,
        "the feud wakes a handful of times, never once a tick",
        st.feud_wakes < 2_000,
        format!("{} feud wakes over 160 days", st.feud_wakes),
    );
    books(&mut out, &c, &w);
    RunOut {
        run: 1,
        built: false,
        slice,
        checks: out,
        rungs: tr.rungs,
        stats: Some(st),
        log: std::mem::take(&mut c.log),
    }
}

/// Run 5's M2 part: the Umvuthana grove's ladder and its priced demand.
fn grove_ladder(seed: u64, slice: &'static str) -> RunOut {
    let (mut w, mut c, f, p) = setup(seed, "umvuthana");
    let inv = theta(&c, f, Step::Invasion);
    let nugget = w.goods.find("gold_nugget").expect("the nugget good");
    let node = c.factions[f as usize].node;
    let price = w.market(node).price(nugget);
    let mut tr = Trace::default();
    // A day at a time (kills at each day's start), reading the demand on the day it is made.
    let mut t = Tick(10);
    let mut demand = None;
    while t < Tick::from_days(80) {
        let to = Tick(t.0 + TICKS_PER_DAY);
        feed(&mut c, &mut w, f, p, 3.0 * inv, t, to, &mut tr);
        t = to;
        if demand.is_none()
            && let Some(x) = c.feud(f, p)
            && let Some(d) = x.demand
        {
            let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
            let bend = Temperament::from_seed(c.factions[f as usize].chief_seed).bend(
                Trait::Aggressive,
                1000,
                50,
            );
            // The grievance the demand was priced on: the climb committed it, and no kill came
            // later that day.
            let g = x.g.total_at(d.made, lambda);
            demand = Some((d, x.g.anchor == d.made, g, bend));
        }
    }
    let st = c.stats;
    let mut out = Vec::new();
    let order: Vec<Step> = tr.rungs.iter().map(|r| r.1).collect();
    check(
        &mut out,
        "Notice, Envoy, Scouts, Raid, Invasion, and never Siege",
        order
            == [
                Step::Notice,
                Step::Envoy,
                Step::Scouts,
                Step::Raid,
                Step::Invasion,
            ]
            && st.escalations[Step::Siege.index()] == 0
            && st.parties[PartyKind::Siege.index()] == 0,
        format!("{order:?}"),
    );
    match demand {
        Some((d, exact, g, bend)) => {
            let want = (0.5 * g * f64::from(bend) / 1000.0 * 9.0).ceil() as i64;
            check(
                &mut out,
                "the demand is gold nuggets at 9 an emerald's worth (no market price)",
                d.good == nugget && price == 0 && exact && d.tribute == want,
                format!(
                    "pay {} gold_nugget by day {:.2} (grievance {g:.1}, aggression bend {bend}; at \
                     Envoy's base threshold 30 and a middling chief it is 135)",
                    d.tribute,
                    d.deadline.0 as f64 / TICKS_PER_DAY as f64
                ),
            );
        }
        None => check(&mut out, "the demand", false, "no demand was made".into()),
    }
    check(
        &mut out,
        "the grove never repeats a rung",
        st.repeats == 0,
        format!("{} repeat(s)", st.repeats),
    );
    books(&mut out, &c, &w);
    RunOut {
        run: 5,
        built: false,
        slice,
        checks: out,
        rungs: tr.rungs,
        stats: Some(st),
        log: std::mem::take(&mut c.log),
    }
}

/// The engine's books and a save that loads to the same state.
fn books(out: &mut Vec<Check>, c: &Conflict, w: &World) {
    let ok = c.check(w);
    check(out, "books balance", ok.is_ok(), format!("{ok:?}"));
    let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &c.save());
    let same = back
        .as_ref()
        .is_ok_and(|b| b.state_hash() == c.state_hash());
    check(
        out,
        "a save loads to the same state",
        same,
        format!("state hash {:016x}", c.state_hash()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every built run passes its checks, so a change elsewhere that breaks one fails here, not
    /// unseen (the dragons' scenario once failed for days because it ran nowhere else).
    #[test]
    fn the_built_runs_pass_every_check() {
        let o = Opts {
            runs: RUNS.iter().map(|r| r.0).collect(),
            seed: 7,
            out: None,
            quiet: true,
        };
        assert_eq!(main(o), 0, "a check failed: run `rtsim-cli sim mowzie`");
    }
}
