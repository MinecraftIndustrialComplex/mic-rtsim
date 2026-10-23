// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim mowzie`: MOWZIE.md §7's runs for Mowzie's Mobs' two factions, headless.
//!
//! **The scaffold** (slice M2). Each run is numbered as §7 numbers it. A run whose pieces are not
//! built yet prints the slice that builds them. M2 builds the parts of two runs that need only the
//! cultures as data, on the phase 1 synthetic world with the camp joined at Gloomhollow's place
//! (`rtsim_conflict::synth::CAMP_POS`):
//! - **Run 1, the den's ladder** (notice, raid, repeat, invasion): a Frostmaw den (a lair, MOWZIE.md
//!   ledger 13) angered by kills in its range, standing in for the rail meter (M7): first enough
//!   to sit between Raid and Invasion until Raid's dwell is over, then 1.5 times Invasion's
//!   threshold for 40 days, then nothing. It goes from Notice straight to Raid (it has no Envoy or
//!   Scouts, ledger 2), comes again `repeat_days` after each band is home (ledger 3), climbs to
//!   Invasion, comes again at the top of its ladder, and calms back to Raid and to Notice.
//! - **Run 5, the grove's ladder:** an Umvuthana grove pushed past Invasion. It goes Notice, Envoy
//!   (a demand in gold nuggets at the fallback of 9 an emerald, as no market prices gold yet,
//!   ledger 12), Scouts, Raid and Invasion, and never Siege (it has no siege party). Then two
//!   groves whose haven markets price gold: one demand at 4 nuggets a coin paid in full, one at 9
//!   refused at the envoy.
//!
//! Exit 1 if a check fails, 64 on bad arguments.

use std::path::PathBuf;
use std::process::ExitCode;

use std::collections::BTreeMap;

use rtsim_conflict::boundary::{self, Cmd, Obs};
use rtsim_conflict::culture::{PartyKind, SignalKind};
use rtsim_conflict::engine::{
    Conflict, ConflictStats, Demand, LogEntry, Party, Signal, TRIBUTE_PRICE_CAP,
};
use rtsim_conflict::{Step, synth};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::{ActorId, Temperament, Trait};
use rtsim_settle::NodeKind;
use rtsim_trade::{MILLI, PRICE_ONE, Price};
use rtsim_world::World;
use serde::Serialize;

const HOUR: u64 = TICKS_PER_DAY / 24;

/// §7's runs: number, what it shows, what is built of it so far, and the slices that build the
/// rest.
const RUNS: [(u32, &str, &str, &str); 13] = [
    (
        1,
        "a line through a range: notice, raid, repeat, invasion; ambushes; a kill and the heir",
        "M2, the ladder on kills: notice, raid, repeat, invasion",
        "the meter M7, ambushes M8, the kill and the body M6",
    ),
    (2, "cut service and stopped service", "", "M7"),
    (3, "reroute", "", "M7"),
    (4, "two dens on one line", "", "M7"),
    (
        5,
        "the tribe: envoy, raids, invasion, plunder, the grove in and out",
        "M2, the ladder and the priced demand, paid and refused",
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
    /// The most of the faction's bands about at once.
    most_bands: usize,
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
        let bands = c.parties.iter().filter(|(_, x)| x.faction == f).count();
        tr.most_bands = tr.most_bands.max(bands);
    }
}

fn check(out: &mut Vec<Check>, name: &'static str, ok: bool, detail: String) {
    out.push(Check { name, ok, detail });
}

/// Each repeat: its day, the days since the last band came home before it (MZB-51), and whether
/// it was an Invasion's.
fn repeat_gaps(c: &Conflict) -> Vec<(f64, f64, bool)> {
    let mut home = None;
    let mut out = Vec::new();
    for e in &c.log {
        match e.kind {
            "home" => home = Some(e.day),
            "repeat" => out.push((
                e.day,
                e.day - home.unwrap_or(f64::NAN),
                e.what.ends_with("the invasion comes again"),
            )),
            _ => {}
        }
    }
    out
}

/// Run 1's M2 part: the Frostmaw den's ladder on kills, notice, raid, repeat, invasion.
fn den_ladder(seed: u64, slice: &'static str) -> RunOut {
    let (mut w, mut c, f, p) = setup(seed, "frostmaw");
    let node = c.factions[f as usize].node;
    let (raid, inv) = (theta(&c, f, Step::Raid), theta(&c, f, Step::Invasion));
    let dwell = c
        .cultures
        .get(c.factions[f as usize].culture)
        .def
        .dwell_days[Step::Raid.index()];
    let mut tr = Trace::default();
    // Between Raid and Invasion, a day at a time, until Raid's dwell is over.
    let mut t = Tick(10);
    let mut on_raid: Option<Tick> = None;
    let dwell_ticks = ((dwell + 1.0) * TICKS_PER_DAY as f64) as u64;
    while on_raid.is_none_or(|d| t < Tick(d.0 + dwell_ticks)) && t < Tick::from_days(60) {
        let to = Tick(t.0 + TICKS_PER_DAY);
        feed(&mut c, &mut w, f, p, 1.5 * raid, t, to, &mut tr);
        t = to;
        if on_raid.is_none() && tr.rungs.iter().any(|r| r.1 == Step::Raid) {
            on_raid = Some(t);
        }
    }
    let calm_from = Tick(t.0 + 40 * TICKS_PER_DAY);
    feed(&mut c, &mut w, f, p, 1.5 * inv, t, calm_from, &mut tr);
    let end = Tick(calm_from.0 + 140 * TICKS_PER_DAY);
    feed(&mut c, &mut w, f, p, 0.0, calm_from, end, &mut tr);
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
        "Notice straight to Raid, Invasion, back to Raid and Notice, then nothing",
        order
            == [
                Step::Notice,
                Step::Raid,
                Step::Invasion,
                Step::Raid,
                Step::Notice,
                Step::None,
            ]
            && st.escalations[Step::Envoy.index()] == 0
            && st.escalations[Step::Scouts.index()] == 0,
        format!(
            "{order:?}; the grievance held near {:.1} (Raid's threshold {raid:.1}) until day \
             {:.1}, then near {:.1} (Invasion's {inv:.1}) until day {:.1}",
            1.5 * raid,
            t.0 as f64 / TICKS_PER_DAY as f64,
            1.5 * inv,
            calm_from.0 as f64 / TICKS_PER_DAY as f64
        ),
    );
    let gaps = repeat_gaps(&c);
    let fed = calm_from.0 as f64 / TICKS_PER_DAY as f64;
    let invasions = gaps.iter().filter(|g| g.2).count();
    check(
        &mut out,
        "a band comes again 4 days after the last one is home",
        st.repeats >= 3
            && st.repeats as usize == gaps.len()
            && st.parties[PartyKind::Raid.index()] + st.parties[PartyKind::Invasion.index()]
                == 2 + st.repeats
            && gaps
                .iter()
                .all(|g| g.1 >= 4.0 - 1e-6 && (g.0 >= fed || g.1 <= 4.0 + 1e-3)),
        format!(
            "{} repeat(s); days between the last return and coming again: {:?}",
            st.repeats,
            gaps.iter()
                .map(|g| format!("{:.2}{}", g.1, if g.2 { " (invasion)" } else { "" }))
                .collect::<Vec<_>>()
        ),
    );
    check(
        &mut out,
        "the Invasion, the top of its ladder, comes again",
        invasions >= 2
            && c.cultures
                .get(c.factions[f as usize].culture)
                .next_up(Step::Invasion)
                .is_none(),
        format!("{invasions} invasion repeat(s)"),
    );
    check(
        &mut out,
        "never two Frostmaws about, never a demand",
        tr.most_bands <= 1 && st.demands == 0,
        format!(
            "at most {} band(s) at once, {} demand(s)",
            tr.most_bands, st.demands
        ),
    );
    check(
        &mut out,
        "the feud wakes a handful of times, never once a tick",
        st.feud_wakes < 2_000,
        format!(
            "{} feud wakes over {:.0} days",
            st.feud_wakes,
            end.0 as f64 / TICKS_PER_DAY as f64
        ),
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

/// Run 5's M2 part: the Umvuthana grove's ladder, its demand at the fallback price, and two at a
/// market's price, paid and refused.
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
    priced_demands(seed, &mut out);
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

/// A grove whose haven market prices gold at `price`, angered to `times` Envoy's threshold by one
/// day's kills, and the demand it makes with what ledger 12 prices it at: `ceil(base × PRICE_ONE /
/// price)` nuggets, at most `TRIBUTE_PRICE_CAP` times the fallback's 9 a coin (MZB-57).
type Priced = (World, Conflict, u32, ActorId, Option<(Demand, i64)>);

fn priced_demand(seed: u64, price: Price, times: f64) -> Priced {
    let (mut w, mut c, f, p) = setup(seed, "umvuthana");
    let nugget = w.goods.find("gold_nugget").expect("the nugget good");
    let node = c.factions[f as usize].node;
    synth::set_price(&mut w, node, nugget, price, Tick(5));
    c.signal(
        &mut w,
        Signal {
            faction: Some(synth::CAMP),
            kind: SignalKind::Killing,
            amount: theta(&c, f, Step::Envoy) * times / 8.0,
            actor: p,
            pos: synth::FOREST_POS,
            district: None,
        },
        Tick(10),
    );
    let mut t = Tick(10);
    let mut d = None;
    while d.is_none() && t < Tick::from_days(5) {
        t = Tick(t.0 + HOUR);
        c.run_until(&mut w, &mut NoInfrastructure, t);
        d = c.feud(f, p).and_then(|x| x.demand);
    }
    let asked = d.and_then(|d| {
        let x = c.feud(f, p)?;
        let lambda = c.cultures.get(c.factions[f as usize].culture).lambda;
        let bend = Temperament::from_seed(c.factions[f as usize].chief_seed).bend(
            Trait::Aggressive,
            1000,
            50,
        );
        let per_coin = (PRICE_ONE as f64 / price as f64).min(TRIBUTE_PRICE_CAP * 9.0);
        let g = x.g.total_at(d.made, lambda);
        let ok = d.good == nugget && x.g.anchor == d.made && w.market(node).price(nugget) == price;
        ok.then(|| {
            (
                d,
                (0.5 * g * f64::from(bend) / 1000.0 * per_coin).ceil() as i64,
            )
        })
    });
    (w, c, f, p, asked)
}

/// Run 5's priced demands: one at 4 nuggets a coin paid in full (part of it first does not
/// settle it), one at 9 refused at the envoy (the grove climbs to Scouts).
fn priced_demands(seed: u64, out: &mut Vec<Check>) {
    let (mut w, mut c, f, p, asked) = priced_demand(seed.wrapping_add(1), PRICE_ONE / 4, 1.2);
    match asked {
        Some((d, want)) => {
            let at = Tick(d.made.0 + HOUR);
            let _ = c.take_commands();
            let part = BTreeMap::from([(d.good, (d.tribute - 1) * MILLI)]);
            let short = !c.pay_tribute(&mut w, synth::CAMP, p, None, &part, at);
            let rest = BTreeMap::from([(d.good, MILLI)]);
            let paid = c.pay_tribute(&mut w, synth::CAMP, p, None, &rest, at);
            let calm = c
                .feud(f, p)
                .is_some_and(|x| x.demand.is_none() && x.step == Step::Notice);
            let told = c
                .take_commands()
                .iter()
                .any(|m| matches!(m, Cmd::Settled(s) if s.outcome == boundary::SETTLED_PAID));
            let booked = c.ledger.tribute.get(&d.good) == Some(&i128::from(d.tribute * MILLI));
            check(
                out,
                "a demand at 4 nuggets a coin, paid in full, settles it",
                d.tribute == want
                    && short
                    && paid
                    && calm
                    && told
                    && booked
                    && c.stats.tributes == 1
                    && c.check(&w).is_ok(),
                format!(
                    "asked {} gold_nugget (ceil of base x 4 is {want}); paying {} first settled \
                     nothing, the rest settled it: {paid}, back to Notice: {calm}, the plugin told \
                     it was paid: {told}",
                    d.tribute,
                    d.tribute - 1
                ),
            );
        }
        None => check(
            out,
            "a demand at 4 nuggets a coin",
            false,
            "no demand at the market's price".into(),
        ),
    }
    let (mut w, mut c, f, p, asked) = priced_demand(seed.wrapping_add(2), PRICE_ONE / 9, 2.0);
    match asked.and_then(|(d, want)| Some((d, want, d.envoy?))) {
        Some((d, want, envoy)) => {
            let _ = c.take_commands();
            let answer = Obs::EnvoyAnswer(boundary::EnvoyAnswer {
                agent: Party::agent(envoy),
                epoch: c.parties[envoy].epoch,
                answer: boundary::ANSWER_REFUSED,
                player: synth::PLAYER,
            });
            c.ingest(
                &mut w,
                &mut NoInfrastructure,
                &answer,
                Tick(d.made.0 + HOUR),
            );
            let step = c.feud(f, p).map(|x| (x.step, x.demand.is_none()));
            let told = c
                .take_commands()
                .iter()
                .any(|m| matches!(m, Cmd::Settled(s) if s.outcome == boundary::SETTLED_REFUSED));
            check(
                out,
                "a demand at 9 nuggets a coin, refused at the envoy, climbs to Scouts",
                d.tribute == want
                    && step == Some((Step::Scouts, true))
                    && told
                    && c.stats.expired == 1
                    && c.stats.tributes == 0
                    && c.stats.parties[PartyKind::Scouts.index()] == 1
                    && c.check(&w).is_ok(),
                format!(
                    "asked {} gold_nugget (ceil of base x 9 is {want}); then {step:?}, the \
                     plugin told it was refused: {told}",
                    d.tribute
                ),
            );
        }
        None => check(
            out,
            "a demand at 9 nuggets a coin carried by an envoy",
            false,
            "no demand at the market's price, or no envoy".into(),
        ),
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
