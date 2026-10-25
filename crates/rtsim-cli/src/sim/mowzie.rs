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
    Conflict, ConflictStats, Demand, LogEntry, Party, Phase, Signal, SlotState, TRIBUTE_PRICE_CAP,
};
use rtsim_conflict::synth::CampRun;
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
        "M4, a reconcile in flight: saved and loaded, voided by a restart and ordered again",
        "the lurk, a cowed time and a meter's decay M8",
    ),
    (9, "a trunk of 14 lines", "", "M7"),
    (
        10,
        "the camp reconciler",
        "M4, both cultures' camps against the emulated plugin: the scripted cases and chaos",
        "the Umvuthi's marches M10, held lines M5 and M9, heirs M9 and M11",
    ),
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
            8 => reconcile_in_flight(slice),
            10 => camp_reconciler(o.seed, slice),
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

// ---- runs 10 and 8: the camp reconciler (slice M4) ---------------------------------------------

fn at_day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

/// A grove of nine Umvuthana, found with the jar's own nine bodies standing in it.
fn grove_run(seed: u64, found: u16) -> CampRun {
    CampRun::new(seed, "umvuthana", found)
}

/// A Frostmaw den.
fn den_run(seed: u64, found: u16) -> CampRun {
    CampRun::new(seed, "frostmaw", found)
}

/// The camp's slots by state: body, owed, left, gone, dead.
fn slot_counts(r: &CampRun) -> [u32; 5] {
    let mut n = [0; 5];
    if let Some(c) = r.c.camp(r.f) {
        for s in &c.slots {
            n[s.state as usize] += 1;
        }
    }
    n
}

/// Bodies of the camp's members standing in the emulated world.
fn world_bodies(r: &CampRun) -> usize {
    r.plugin.camps.get(&r.key).map_or(0, |c| c.bodies.len())
}

/// Lines of `action` the plugin applied (any outcome).
fn applied(r: &CampRun, action: u8) -> usize {
    r.plugin
        .applied
        .iter()
        .filter(|a| a.line.action == action)
        .count()
}

/// The camp comes in and its first lines are answered.
fn come_in(r: &mut CampRun) {
    r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
}

/// The camp goes out, two ticks pass, it comes in again and its lines are answered.
fn out_and_in(r: &mut CampRun) {
    r.camp_out();
    let to = r.c.now() + 2;
    r.run(to);
    come_in(r);
}

/// The war parties still about.
fn war_parties(r: &CampRun) -> usize {
    r.c.parties.iter().filter(|(_, p)| p.kind.is_war()).count()
}

/// The books hold against the world (I1 to I6) and no check failed on the way.
fn holds(r: &mut CampRun) -> bool {
    r.check().is_empty() && r.failures.is_empty()
}

/// Kills the camp's `n`th body, as a player would.
fn kill_body(r: &mut CampRun, n: usize) {
    let u = r.plugin.camps[&r.key].bodies[n].uuid;
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, at);
}

/// Each CAMP_RECONCILE the engine sent.
fn reconciles(r: &CampRun) -> usize {
    r.cmds
        .iter()
        .filter(|(_, c)| matches!(c, Cmd::Reconcile(_)))
        .count()
}

/// Lines of `action` in commands the engine sent.
fn lines_in(cmds: &[Cmd], action: u8) -> usize {
    cmds.iter()
        .map(|c| match c {
            Cmd::Reconcile(x) => x.lines.iter().filter(|l| l.action == action).count(),
            _ => 0,
        })
        .sum()
}

/// Run 10: the camp reconciler (MOWZIE.md §4.9, ledgers 6 and 7) against the emulated plugin, a
/// world of tagged bodies. First the scripted cases, each a check, on worlds of fixed seeds (they
/// check exact slot counts, which a raid's follow-up parties change from world to world); then
/// chaos over many seeds for both cultures, from `seed`, with the invariants checked after every
/// event.
fn camp_reconciler(seed: u64, slice: &'static str) -> RunOut {
    use rtsim_conflict::boundary::{
        ACTION_ADOPT, ACTION_HAND_BACK, ACTION_REMOVE, ACTION_SET_OUT, ACTION_SPAWN, ACTION_TAKE,
    };
    let mut out = Vec::new();

    // The first census: the jar's bodies fill the owed slots, no spawn beside them.
    let mut r = grove_run(11, 9);
    let found = slot_counts(&r);
    come_in(&mut r);
    check(
        &mut out,
        "a first census adopts the jar's bodies and spawns none",
        found == [0, 9, 0, 0, 0]
            && slot_counts(&r) == [9, 0, 0, 0, 0]
            && applied(&r, ACTION_ADOPT) == 9
            && applied(&r, ACTION_SPAWN) == 0
            && world_bodies(&r) == 9
            && holds(&mut r),
        format!(
            "found {found:?}, then {:?}: {} adopts, {} spawns, {} bodies",
            slot_counts(&r),
            applied(&r, ACTION_ADOPT),
            applied(&r, ACTION_SPAWN),
            world_bodies(&r)
        ),
    );

    // Strays: over the cap removed, under it adopted.
    let mut r = grove_run(15, 3);
    let body = r.body;
    r.plugin.camp_stray(r.key, body);
    r.plugin.camp_stray(r.key, body);
    come_in(&mut r);
    let at_cap = (
        slot_counts(&r),
        applied(&r, ACTION_REMOVE),
        world_bodies(&r),
    );
    kill_body(&mut r, 0);
    r.camp_out();
    r.plugin.camp_stray(r.key, body);
    r.plugin.camp_stray(r.key, body);
    let to = r.c.now() + 3;
    r.run(to);
    come_in(&mut r);
    check(
        &mut out,
        "strays over the cap are removed, those under it adopted",
        at_cap == ([3, 0, 0, 0, 0], 2, 3)
            && slot_counts(&r) == [3, 0, 0, 0, 0]
            && world_bodies(&r) == 3
            && holds(&mut r),
        format!(
            "cap 3 with two strays: {at_cap:?} (slots, removes, bodies); a death and two more \
             strays: {:?}, {} bodies",
            slot_counts(&r),
            world_bodies(&r)
        ),
    );

    // A raid: drawn with their bodies, set out at the muster's end, gone while away, owed at
    // home and spawned once.
    let mut r = grove_run(21, 9);
    come_in(&mut r);
    let p = r.raid().expect("nine members make a raid");
    let drawn = (slot_counts(&r), r.c.factions[r.f as usize].away);
    let mut t = r.c.now();
    let mut away = None;
    let mut out_for = 0;
    while r.c.parties.get(p).is_some_and(|x| x.phase != Phase::Home) && t < at_day(20.0) {
        t += 200;
        r.run(t);
        if r.c.parties.get(p).is_some_and(|x| x.phase == Phase::Out) {
            out_for += 1;
            if out_for == 3 {
                away = Some((slot_counts(&r), world_bodies(&r)));
            }
        }
    }
    let end = r.c.now() + at_day(2.0).0;
    r.run(end);
    check(
        &mut out,
        "a raid leaves in its members' bodies and comes home owed, spawned once",
        drawn == ([5, 0, 4, 0, 0], 4)
            && away == Some(([5, 0, 0, 4, 0], 5))
            && war_parties(&r) == 0
            && applied(&r, ACTION_SET_OUT) == 4
            && applied(&r, ACTION_SPAWN) == 4
            && slot_counts(&r) == [9, 0, 0, 0, 0]
            && world_bodies(&r) == 9
            && holds(&mut r),
        format!(
            "drawn {drawn:?}; out {away:?} (slots, bodies); home {:?}, {} set-outs, {} spawns, \
             {} bodies",
            slot_counts(&r),
            applied(&r, ACTION_SET_OUT),
            applied(&r, ACTION_SPAWN),
            world_bodies(&r)
        ),
    );

    // A party home with its puppets has them handed back, not spawned. The walk home in bodies
    // is M10's, so the party's embodied state is forged here.
    let mut r = grove_run(127, 9);
    come_in(&mut r);
    let p = r.raid().expect("a raid");
    let mut t = r.c.now();
    while r.c.parties[p].phase != Phase::Home && t < at_day(20.0) {
        t += 200;
        r.run(t);
    }
    let units = r.c.parties[p].units.clone();
    r.c.parties[p].embodied = true;
    r.c.parties[p].spawned = units;
    let end = r.c.parties[p].until.max(t) + at_day(2.0).0;
    r.run(end);
    check(
        &mut out,
        "a party home in its puppets has each handed back (forged state, M10 walks them home)",
        war_parties(&r) == 0
            && applied(&r, ACTION_HAND_BACK) == 4
            && applied(&r, ACTION_SPAWN) == 0
            && slot_counts(&r) == [9, 0, 0, 0, 0]
            && world_bodies(&r) == 9
            && holds(&mut r),
        format!(
            "{} hand-backs, {} spawns, {:?}, {} bodies",
            applied(&r, ACTION_HAND_BACK),
            applied(&r, ACTION_SPAWN),
            slot_counts(&r),
            world_bodies(&r)
        ),
    );

    // A member drawn owed carries its debt: a den out for a week.
    let mut r = den_run(25, 0);
    r.run(at_day(6.5));
    // The books add the pool's slots when they are read (a census, a draw, a death), not at an
    // audit: the member recruited while the den was out is the pool's, an owed slot once read.
    let owed = r.c.pool_at(r.f, r.c.now()).floor() as u32;
    let p = r.raid().expect("one Frostmaw makes the raid");
    let drawn = slot_counts(&r);
    r.run(r.c.parties[p].until + 100);
    out_and_in(&mut r);
    out_and_in(&mut r);
    let while_away = (slot_counts(&r), reconciles(&r), world_bodies(&r));
    let before = applied(&r, ACTION_SPAWN);
    let mut d = 7.0;
    while war_parties(&r) > 0 && d < 14.0 {
        d += 0.25;
        r.run(at_day(d));
    }
    let to = r.c.now() + 100;
    r.run(to);
    check(
        &mut out,
        "a member drawn owed is spawned once at home and never taken or booked dead while away",
        owed == 1
            && drawn == [0, 0, 0, 1, 0]
            && while_away == ([0, 0, 0, 1, 0], 0, 0)
            && r.c.stats.camp_written_off == 0
            && applied(&r, ACTION_SPAWN) == before + 1
            && slot_counts(&r) == [1, 0, 0, 0, 0]
            && world_bodies(&r) == 1
            && holds(&mut r),
        format!(
            "pool {owed}, drawn {drawn:?}, two censuses while away {while_away:?} (slots, \
             orders, bodies), home {:?} with {} Frostmaw",
            slot_counts(&r),
            world_bodies(&r)
        ),
    );

    // Refills: members killed with the grove in come back at the culture's rate, as owed slots
    // while it is out, spawned at the next census.
    let mut r = grove_run(14, 9);
    come_in(&mut r);
    for _ in 0..4 {
        kill_body(&mut r, 0);
        let to = r.c.now() + 2;
        r.run(to);
    }
    r.camp_out();
    let to = r.c.now() + 5;
    r.run(to);
    let dead = slot_counts(&r);
    let sent = reconciles(&r);
    r.run(at_day(1.0) + 100);
    let quiet = reconciles(&r) == sent;
    let r_ok = holds(&mut r);
    // Two a day: the pool is back at 7, its two new members slots the books add when next read.
    let whole = r.c.pool_at(r.f, r.c.now()).floor() as u32;
    let owed = whole.saturating_sub(r.c.camp(r.f).map_or(0, |c| c.home_count()));
    come_in(&mut r);
    check(
        &mut out,
        "a grove's refill is spawned at its next census, none ordered while it is out",
        dead == [5, 0, 0, 0, 0]
            && quiet
            && owed == 2
            && r_ok
            && applied(&r, ACTION_SPAWN) == 2
            && slot_counts(&r) == [7, 0, 0, 0, 0]
            && holds(&mut r),
        format!(
            "four died, {dead:?}; none ordered over the day: {quiet}; {owed} slots the books have not added; \
             then {:?} with {} spawns",
            slot_counts(&r),
            applied(&r, ACTION_SPAWN)
        ),
    );

    // Deaths: at home, in a muster, and left behind by a party that is out.
    let mut r = grove_run(18, 9);
    come_in(&mut r);
    kill_body(&mut r, 0);
    let to = r.c.now() + 5;
    r.run(to);
    check(
        &mut out,
        "a death at home costs the pool a member, one CAMP_DEATH booked",
        slot_counts(&r) == [8, 0, 0, 0, 0] && r.c.stats.camp_deaths == 1 && holds(&mut r),
        format!(
            "{:?}, {} deaths booked",
            slot_counts(&r),
            r.c.stats.camp_deaths
        ),
    );

    let mut r = grove_run(20, 4);
    come_in(&mut r);
    let p = r.raid().expect("four members make a raid");
    kill_body(&mut r, 0);
    let to = r.c.now() + 5;
    r.run(to);
    let alive = r.c.parties[p].alive();
    let after = (slot_counts(&r), r.c.factions[r.f as usize].away);
    let rest: Vec<_> = r.plugin.camps[&r.key].bodies.clone();
    for b in &rest {
        let at = r.c.now() + 1;
        r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
        let to = r.c.now() + 3;
        r.run(to);
    }
    let to = r.c.now() + at_day(2.0).0;
    r.run(to);
    check(
        &mut out,
        "a body killed in a muster is its member's unit lost, and an emptied party ends",
        alive == 3
            && after == ([0, 0, 3, 0, 0], 3)
            && war_parties(&r) == 0
            && r.c.factions[r.f as usize].away == 0
            && holds(&mut r),
        format!(
            "{alive} alive after one, {after:?}; every body killed: {} parties, {} away",
            war_parties(&r),
            r.c.factions[r.f as usize].away
        ),
    );

    let mut r = grove_run(22, 9);
    come_in(&mut r);
    let p = r.raid().expect("a raid");
    r.camp_out();
    let end = r.c.parties[p].until;
    r.run(end + 100);
    let left = slot_counts(&r)[2];
    let cu = r.c.camp(r.f).expect("a ledger").clone();
    let b = r.plugin.camps[&r.key]
        .bodies
        .iter()
        .find(|b| {
            b.generation != 0
                && cu
                    .slots
                    .iter()
                    .any(|s| s.generation == b.generation && s.state == SlotState::Left)
        })
        .copied()
        .expect("a body left behind");
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, at);
    let to = r.c.now() + 3;
    r.run(to);
    check(
        &mut out,
        "a body left behind by a party that is out dies with its unit",
        left == 4
            && r.c.parties[p].alive() == 3
            && slot_counts(&r)[2] == 3
            && r.c.camp(r.f).is_some_and(|c| c.slots.len() == 8)
            && holds(&mut r),
        format!(
            "{left} left behind; one died: {} units, {:?}",
            r.c.parties[p].alive(),
            slot_counts(&r)
        ),
    );

    // Waiting for whole bodies: a feud with every slot's line in flight waits, books no event,
    // and goes when the answer lands.
    let mut r = den_run(27, 0);
    r.plugin.answer_ticks = 5 * TICKS_PER_DAY;
    r.camp_in();
    let mut blocked = 0;
    let mut sent_blocked = false;
    let mut d = 0.0;
    while d < 16.0 {
        r.kill(20.0);
        d += 1.0;
        r.run(at_day(d));
        let wait = r.c.feud(r.f, r.player).map(|x| x.wait_until);
        if wait.is_some_and(|w| w >= rtsim_conflict::engine::BLOCKED) {
            blocked += 1;
            sent_blocked |= !r.c.parties.is_empty();
        }
    }
    let raids: u64 = r.c.stats.parties.iter().sum();
    check(
        &mut out,
        "a feud waits for the lines in flight, books no wake for it, and goes when they land",
        blocked > 0 && !sent_blocked && raids >= 1 && r.c.stats.feud_wakes < 2_000 && holds(&mut r),
        format!(
            "blocked on {blocked} of 16 days, {raids} parties after the answer, {} feud wakes",
            r.c.stats.feud_wakes
        ),
    );

    // Answers: lost, replayed, stale, late.
    let mut r = den_run(31, 0);
    r.camp_in();
    r.plugin.lose_answers = 1;
    r.run(at_day(6.5));
    let flying = r.c.camp(r.f).map_or(0, |c| c.in_flight());
    out_and_in(&mut r);
    check(
        &mut out,
        "a lost answer is repaired by the next census without a second body",
        flying == 1
            && applied(&r, ACTION_SPAWN) == 1
            && world_bodies(&r) == 1
            && slot_counts(&r) == [1, 0, 0, 0, 0]
            && holds(&mut r),
        format!(
            "{flying} line still in flight; then {:?}, {} spawn, {} body",
            slot_counts(&r),
            applied(&r, ACTION_SPAWN),
            world_bodies(&r)
        ),
    );

    let mut r = grove_run(32, 9);
    r.plugin.replay_answers = true;
    come_in(&mut r);
    check(
        &mut out,
        "a replayed answer is booked once",
        r.c.stats.camp_answers == 9 && slot_counts(&r) == [9, 0, 0, 0, 0] && holds(&mut r),
        format!(
            "{} answers booked for nine lines sent twice",
            r.c.stats.camp_answers
        ),
    );

    let mut r = grove_run(28, 9);
    come_in(&mut r);
    let p = r.raid().expect("a raid");
    r.camp_out();
    r.run(r.c.parties[p].until + 100);
    let first = r.camp_in();
    r.camp_out();
    let second = r.camp_in();
    let to = r.c.now() + 10;
    r.run(to);
    let takes = applied(&r, ACTION_TAKE);
    let stale = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == ACTION_TAKE && a.outcome == boundary::LINE_STALE)
        .count();
    check(
        &mut out,
        "a census before the last one's lines has them answered stale, each body taken once",
        first != second
            && takes == 8
            && stale == 4
            && slot_counts(&r) == [5, 0, 0, 4, 0]
            && world_bodies(&r) == 5
            && holds(&mut r),
        format!(
            "{takes} takes, {stale} stale; {:?}, {} bodies",
            slot_counts(&r),
            world_bodies(&r)
        ),
    );

    let mut r = grove_run(33, 3);
    r.plugin.answer_ticks = 2_000;
    r.camp_in();
    r.run(Tick(100));
    kill_body(&mut r, 0);
    r.run(Tick(300));
    let freed = r.c.camp(r.f).map_or(0, |c| c.slots.len());
    r.run(Tick(5_000));
    check(
        &mut out,
        "a late answer for a generation the slot no longer holds books nothing",
        freed == 2
            && r.c
                .camp(r.f)
                .is_some_and(|c| c.slots.iter().take(2).all(|s| s.state == SlotState::Body))
            && holds(&mut r),
        format!(
            "the slot freed by a death while its adopt was on the way ({freed} slots left); \
             {:?} once every answer landed",
            slot_counts(&r)
        ),
    );

    // A session restart voids the lines; the next census orders them again, once.
    let mut r = grove_run(29, 9);
    r.plugin.answer_ticks = 5_000;
    r.camp_in();
    r.run(Tick(100));
    let flying = r.c.camp(r.f).map_or(0, |c| c.in_flight());
    r.restart();
    let voided = (
        r.c.camp(r.f).map_or(9, |c| c.in_flight()),
        r.c.camp_is_in(r.f),
    );
    let sent = reconciles(&r);
    r.run(Tick(20_000));
    let quiet = reconciles(&r) == sent;
    out_and_in(&mut r);
    check(
        &mut out,
        "a session start voids the lines and the next census orders them again, once",
        flying == 9
            && voided == (0, false)
            && quiet
            && slot_counts(&r) == [9, 0, 0, 0, 0]
            && applied(&r, ACTION_ADOPT) == 9
            && applied(&r, ACTION_SPAWN) == 0
            && holds(&mut r),
        format!(
            "{flying} adopts in flight, then void and the camp out ({voided:?}), nothing resent \
             ({quiet}); the next census: {:?}, {} adopts",
            slot_counts(&r),
            applied(&r, ACTION_ADOPT)
        ),
    );

    // The crash table's gap: a raid's four set-out answers are in the journal's tail when the
    // session ends, and the party is home before the holders' records reach the engine (their
    // chunks load days later). Booked by the action each carries (not by the state the slots have
    // by then, nor by a line in flight, which the restart voided), the four slots are owed and
    // spawned, and no member is lost; whole or one line to a record (booked by (op, line), not
    // by op).
    for split in [0usize, 1] {
        let mut r = grove_run(70, 9);
        r.plugin.holders_again = true;
        r.plugin.split_answers = split;
        come_in(&mut r);
        r.plugin.answer_ticks = at_day(30.0).0;
        // A camp whose answers were not booked cannot field the raid: the check then fails.
        let raid = r.raid();
        let end = raid.map_or(r.c.now(), |p| r.c.parties[p].until);
        r.run(end + 10);
        let (set_outs, records) = (applied(&r, ACTION_SET_OUT), r.plugin.holders.len());
        r.plugin.answer_ticks = at_day(12.0).0;
        r.restart();
        r.plugin.answer_ticks = 1;
        r.run(at_day(8.0));
        let home = war_parties(&r) == 0 && slot_counts(&r) == [9, 0, 0, 0, 0];
        r.run(at_day(14.0));
        let owed = slot_counts(&r)[1];
        r.settle();
        check(
            &mut out,
            if split == 0 {
                "a raid's lost set-out answers are booked from the holders, no member lost"
            } else {
                "the same, journaled again one line to a record, each booked, no member lost"
            },
            set_outs == 4
                && records == 4
                && home
                && owed == 4
                && r.c.stats.camp_written_off == 0
                && r.ghosts_gone == 0
                && r.ghosts.is_empty()
                && r.plugin.holders.is_empty()
                && slot_counts(&r).iter().sum::<u32>() == 9
                && world_bodies(&r) == slot_counts(&r)[0] as usize
                && holds(&mut r),
            format!(
                "{set_outs} set-outs, {records} records on the markers; party home with its \
                 slots body: {home}; the records land: {owed} owed; {} written off, {:?}, {} \
                 bodies, {} war parties out",
                r.c.stats.camp_written_off,
                slot_counts(&r),
                world_bodies(&r),
                war_parties(&r)
            ),
        );
    }

    // The census waits for the footprint's holders: a set-out answer lost with no restart is
    // still on the markers, journaled again as the camp comes in, before its census, and booked
    // by its action: the four slots are owed and spawned at that census.
    let mut r = grove_run(74, 9);
    r.plugin.holders_again = true;
    come_in(&mut r);
    r.plugin.lose_answers = 1;
    let raid = r.raid();
    let end = raid.map_or(r.c.now(), |p| r.c.parties[p].until);
    r.run(end + 10);
    let (set_outs, records) = (applied(&r, ACTION_SET_OUT), r.plugin.holders.len());
    r.run(at_day(8.0));
    let home = war_parties(&r) == 0 && slot_counts(&r) == [9, 0, 0, 0, 0] && world_bodies(&r) == 5;
    out_and_in(&mut r);
    let spawned = world_bodies(&r);
    r.settle();
    check(
        &mut out,
        "holders journal their records as the camp comes in, before its census: no member lost",
        set_outs == 4
            && records == 4
            && home
            && spawned == 9
            && r.c.stats.camp_written_off == 0
            && r.ghosts_gone == 0
            && r.ghosts.is_empty()
            && r.plugin.holders.is_empty()
            && holds(&mut r),
        format!(
            "{set_outs} set-outs, {records} records on the markers; party home, books never told, \
             5 bodies: {home}; {spawned} bodies after the census; {} written off",
            r.c.stats.camp_written_off
        ),
    );

    // Chaos: every event kind at random times, both cultures, many seeds.
    let mut tally: BTreeMap<&'static str, u32> = BTreeMap::new();
    let (mut runs, mut bad) = (0u32, Vec::new());
    let mut stats = [0u64; 5];
    for culture in ["umvuthana", "frostmaw"] {
        for k in 0..32u64 {
            let found = if culture == "frostmaw" { 1 } else { 9 };
            let mut r = CampRun::new(seed + 100 + k, culture, found);
            r.plugin.fight = Some(synth::Fight {
                kill_share: 0.4,
                after: 200,
                kill_chief: false,
                only_at_base: false,
            });
            r.watch = k % 2 == 0;
            r.chaos(seed * 1000 + k, 220);
            // The answers stop being lost and the parties get home: every ghost has had its
            // three censuses.
            r.settle();
            runs += 1;
            for f in std::mem::take(&mut r.failures) {
                bad.push(format!("{culture} seed {k}: {f}"));
            }
            for f in r.check() {
                bad.push(format!("{culture} seed {k}, at the end: {f}"));
            }
            // The write-offs are exactly the ghosts: members the world lost with nothing left to
            // tell the books; and none at all when holders journal their records again (at
            // camp-in and at a session start, MZB-174), whatever the lines' order and split.
            if !r.ghosts.is_empty()
                || r.c.stats.camp_written_off != u64::from(r.ghosts_gone)
                || (r.plugin.holders_again && r.c.stats.camp_written_off != 0)
            {
                bad.push(format!(
                    "{culture} seed {k}: {} members written off, {} ghosts gone, {} left, \
                     holders journaling again: {}",
                    r.c.stats.camp_written_off,
                    r.ghosts_gone,
                    r.ghosts.len(),
                    r.plugin.holders_again
                ));
            }
            for (_, e) in &r.events {
                let kind = [
                    "camp in",
                    "camp out",
                    "raid true",
                    "kill slot",
                    "stray",
                    "lose the next",
                    "replay answers",
                    "restart",
                    "save and load",
                ]
                .into_iter()
                .find(|k| e.starts_with(k));
                if let Some(k) = kind {
                    *tally.entry(k).or_default() += 1;
                }
            }
            let s = &r.c.stats;
            stats[0] += s.camp_censuses;
            stats[1] += s.camp_lines;
            stats[2] += s.camp_answers;
            stats[3] += s.camp_deaths;
            stats[4] += s.camp_written_off;
        }
    }
    check(
        &mut out,
        "chaos holds I1 to I5 after every event (I6 is by construction), every member lost \
         unseen is written off and no other, none at all where holders journal again, no draw \
         passes its bound",
        bad.is_empty(),
        if bad.is_empty() {
            format!(
                "{runs} camps of 220 events each; {}; {} censuses, {} lines, {} answers, {} \
                 deaths, {} written off, each a ghost",
                tally
                    .iter()
                    .map(|(k, n)| format!("{n} {k}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                stats[0],
                stats[1],
                stats[2],
                stats[3],
                stats[4]
            )
        } else {
            format!(
                "{} findings, first: {:?}",
                bad.len(),
                &bad[..bad.len().min(3)]
            )
        },
    );

    RunOut {
        run: 10,
        built: false,
        slice,
        checks: out,
        rungs: Vec::new(),
        stats: None,
        log: Vec::new(),
    }
}

/// Run 8's reconcile in flight: the engine saved and loaded with lines in flight goes on exactly as
/// the run that kept going; a session restart voids the lines and the next census orders what is
/// still needed again, once.
fn reconcile_in_flight(slice: &'static str) -> RunOut {
    use rtsim_conflict::boundary::{ACTION_ADOPT, ACTION_SET_OUT, ACTION_SPAWN, ACTION_TAKE};
    let mut out = Vec::new();

    // Saved with lines in flight, three ways: nine adopts, a spawn, a muster's set-outs. Loaded
    // in place; the loaded run and the run that kept going end in one state.
    type Scene = fn(u64, bool) -> CampRun;
    let scenes: [(&str, Scene); 3] = [
        ("nine adopts in flight", |seed, save| {
            let mut r = grove_run(seed, 9);
            r.plugin.answer_ticks = 5_000;
            r.camp_in();
            r.run(Tick(100));
            assert_eq!(r.c.camp(r.f).map_or(0, |c| c.in_flight()), 9);
            if save {
                r.round_trip();
            }
            r.run(Tick(30_000));
            r
        }),
        ("a spawn in flight", |seed, save| {
            let mut r = den_run(seed, 0);
            r.plugin.answer_ticks = 3 * TICKS_PER_DAY;
            r.camp_in();
            r.run(at_day(6.5));
            assert_eq!(r.c.camp(r.f).map_or(0, |c| c.in_flight()), 1);
            if save {
                r.round_trip();
            }
            r.run(at_day(14.0));
            r
        }),
        ("a muster's set-outs in flight", |seed, save| {
            let mut r = grove_run(seed, 9);
            come_in(&mut r);
            r.plugin.answer_ticks = 5_000;
            let p = r.raid().expect("a raid");
            let end = r.c.parties[p].until;
            r.run(end + 50);
            assert_eq!(r.c.camp(r.f).map_or(0, |c| c.in_flight()), 4);
            if save {
                r.round_trip();
            }
            r.run(end + at_day(12.0).0);
            r
        }),
    ];
    for (k, (what, scene)) in scenes.iter().enumerate() {
        let mut kept = scene(300 + k as u64, false);
        let mut loaded = scene(300 + k as u64, true);
        let (a, b) = (kept.c.state_hash(), loaded.c.state_hash());
        let same = a == b
            && kept.plugin.camps[&kept.key].bodies.len()
                == loaded.plugin.camps[&loaded.key].bodies.len()
            && kept.c.stats.camp_lines == loaded.c.stats.camp_lines
            && kept.c.stats.camp_answers == loaded.c.stats.camp_answers;
        let name: &'static str = [
            "a save with nine adopts in flight loads and ends as the run that kept going",
            "a save with a spawn in flight loads and ends as the run that kept going",
            "a save with a muster's set-outs in flight loads and ends as the run that kept going",
        ][k];
        check(
            &mut out,
            name,
            same && holds(&mut kept) && holds(&mut loaded),
            format!(
                "{what}: state {a:016x} against {b:016x}, {} lines and {} answers booked",
                loaded.c.stats.camp_lines, loaded.c.stats.camp_answers
            ),
        );
    }

    // A restart in the middle of each: the lines are void, nothing is resent, and the next census
    // orders what is still needed, once.
    let mut r = grove_run(310, 9);
    let held = r.census_held();
    let adopts = lines_in(&held, ACTION_ADOPT);
    let flying = r.c.camp(r.f).map_or(0, |c| c.in_flight());
    r.restart();
    let void = r.c.camp(r.f).map_or(9, |c| c.in_flight());
    come_in(&mut r);
    check(
        &mut out,
        "a restart with adopts in flight voids them, and the next census adopts each once",
        adopts == 9
            && flying == 9
            && void == 0
            && applied(&r, ACTION_ADOPT) == 9
            && applied(&r, ACTION_SPAWN) == 0
            && slot_counts(&r) == [9, 0, 0, 0, 0]
            && holds(&mut r),
        format!(
            "{adopts} adopts held, {flying} in flight, {void} after the restart; then {} \
             applied, {:?}",
            applied(&r, ACTION_ADOPT),
            slot_counts(&r)
        ),
    );

    let mut r = den_run(311, 0);
    r.run(at_day(6.5));
    let held = r.census_held();
    let spawns = lines_in(&held, ACTION_SPAWN);
    r.restart();
    let nothing = world_bodies(&r) == 0;
    come_in(&mut r);
    check(
        &mut out,
        "a restart with a spawn in flight voids it, and the next census spawns once",
        spawns == 1
            && nothing
            && applied(&r, ACTION_SPAWN) == 1
            && world_bodies(&r) == 1
            && slot_counts(&r) == [1, 0, 0, 0, 0]
            && holds(&mut r),
        format!(
            "{spawns} spawn held, none applied before the restart: {nothing}; then {} applied, {} \
             body",
            applied(&r, ACTION_SPAWN),
            world_bodies(&r)
        ),
    );

    let mut r = grove_run(312, 9);
    come_in(&mut r);
    let p = r.raid().expect("a raid");
    let end = r.c.parties[p].until;
    let held = r.decide(end + 1);
    let sets = lines_in(&held, ACTION_SET_OUT);
    r.restart();
    let untouched = world_bodies(&r) == 9;
    come_in(&mut r);
    check(
        &mut out,
        "a restart with a muster's set-outs in flight voids them, and the next census takes the \
         bodies left behind",
        sets == 4
            && untouched
            && applied(&r, ACTION_SET_OUT) == 0
            && applied(&r, ACTION_TAKE) == 4
            && slot_counts(&r) == [5, 0, 0, 4, 0]
            && world_bodies(&r) == 5
            && holds(&mut r),
        format!(
            "{sets} set-outs held, nine bodies stood: {untouched}; then {} takes, {:?}, {} bodies",
            applied(&r, ACTION_TAKE),
            slot_counts(&r),
            world_bodies(&r)
        ),
    );

    RunOut {
        run: 8,
        built: false,
        slice,
        checks: out,
        rungs: Vec::new(),
        stats: None,
        log: Vec::new(),
    }
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
