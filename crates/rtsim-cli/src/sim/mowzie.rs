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
use rtsim_conflict::culture::{Cultures, PartyKind, SignalKind};
use rtsim_conflict::engine::{
    CHIEF_SLOT, Conflict, ConflictStats, Demand, LogEntry, Party, Phase, Signal, SlotState,
    TRIBUTE_PRICE_CAP, Target, item_of,
};
use rtsim_conflict::synth::CampRun;
use rtsim_conflict::{Step, synth};
use rtsim_ecology::infra::{DistrictSite, Infrastructure, NoInfrastructure, RaidOutcome};
use rtsim_industry::ids::{Good, GoodVec};
use rtsim_industry::threat::Threat;
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
        "the meter M7, ambushes M8",
    ),
    (2, "cut service and stopped service", "", "M7"),
    (3, "reroute", "", "M7"),
    (4, "two dens on one line", "", "M7"),
    (
        5,
        "the tribe: envoy, raids, invasion, plunder, the grove in and out",
        "M2, the ladder and the priced demand, paid and refused; M9a, the grove: plunder, an \
         Invasion with its chief, in and out, hunts owed, gifts in nuggets",
        "",
    ),
    (
        6,
        "renewal of dens and groves",
        "M6, the den: the wait, the pressure, the farmed rate, the migrant, the steal; M9a, the \
         grove: the heir's wait, the pressure, the farmed rate",
        "",
    ),
    (
        7,
        "kills and which feud steps down",
        "M6: the raid, the camp, a band left at home, a district's walls, the kill and the body",
        "",
    ),
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
        "M4, both cultures' camps against the emulated plugin: the scripted cases and chaos; M9a, \
         a grove's chaos with heirs and Invasions",
        "the Umvuthi's marches M10, held lines M5 and M9, the chief's casualties on a march M11",
    ),
    (11, "an Invasion with its chief", "", "M10, M11"),
    (
        12,
        "the heir farmed for 60 years",
        "M9a, the Umvuthi killed as its heir stands: the wait and the rate at base 6 and 24",
        "",
    ),
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
        } else if rest.is_empty() {
            println!("{n:>2}  {what}\n    built: {built}");
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
            6 => {
                let mut o = den_renewal(slice);
                grove_renewal(&mut o.checks);
                o.built = true;
                o
            }
            7 => den_kills(slice),
            8 => reconcile_in_flight(slice),
            10 => camp_reconciler(o.seed, slice),
            12 => heir_farmed(slice),
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
        if !run.slice.is_empty() {
            println!("  still to build: {}", run.slice);
        }
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
    grove_rest(&mut out);
    RunOut {
        run: 5,
        built: true,
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

/// A Frostmaw den found with no member in it waits its renewal's cap for the migrant (MZB-257):
/// 192 days, where it once refilled by a rate in six.
const DEN_WAIT: f64 = 192.0;

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

    // A member drawn owed carries its debt: a den out for a week, once its migrant has come.
    let mut r = den_run(25, 0);
    r.run(at_day(DEN_WAIT + 0.5));
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
    let mut d = DEN_WAIT + 1.0;
    while war_parties(&r) > 0 && d < DEN_WAIT + 8.0 {
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
    // The den has no member for the migrant's whole wait; the kills begin the day before it comes.
    let mut d = DEN_WAIT - 1.0;
    r.run(at_day(d));
    while d < DEN_WAIT + 15.0 {
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
    r.run(at_day(DEN_WAIT + 0.5));
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

    chief_chaos(seed, &mut out);

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
            r.run(at_day(DEN_WAIT + 0.5));
            assert_eq!(r.c.camp(r.f).map_or(0, |c| c.in_flight()), 1);
            if save {
                r.round_trip();
            }
            r.run(at_day(DEN_WAIT + 8.0));
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
    r.run(at_day(DEN_WAIT + 0.5));
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

// ---- runs 6 and 7: the den's renewal and the chief's kills (slice M6) ---------------------------

/// The tick the den's next Frostmaw is booked to come, if one is.
fn arrival_at(r: &CampRun) -> Option<Tick> {
    r.c.camp(r.f).and_then(|c| c.arrival_at)
}

/// The den's recent losses a player caused, `P`.
fn pressure_of(r: &CampRun) -> f32 {
    r.c.camp(r.f).map_or(0.0, |c| c.pressure.parts().0)
}

/// The den's first body killed, `by`, a tick from now and the books run on a few ticks: the tick it
/// died at.
fn kill_first(r: &mut CampRun, by: u32) -> Tick {
    let u = r.plugin.camps[&r.key].bodies[0].uuid;
    let at = r.c.now() + 1;
    r.plugin.camp_kill(r.key, u, by, at);
    let to = r.c.now() + 5;
    r.run(to);
    at
}

/// Days from `from` to the booked arrival.
fn wait_days(r: &CampRun, from: Tick) -> Option<f64> {
    arrival_at(r).map(|t| (t.0 - from.0) as f64 / TICKS_PER_DAY as f64)
}

/// The flags of the lines of every reconcile the engine sent, in order.
fn line_flags(r: &CampRun) -> Vec<u8> {
    r.cmds
        .iter()
        .filter_map(|(_, c)| match c {
            Cmd::Reconcile(x) => Some(x.lines.iter().map(|l| l.flags)),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A den's feud against `who`, at `step` with grievance behind it (the feud stands there after a
/// replan).
fn feud_at(r: &mut CampRun, who: ActorId, step: Step) {
    let at = r.c.now() + 1;
    r.c.signal(
        &mut r.w,
        Signal {
            faction: Some(r.key),
            kind: SignalKind::Killing,
            amount: 10.0,
            actor: who,
            pos: synth::CAMP_POS,
            district: None,
        },
        at,
    );
    r.c.feud_mut(r.f, who).expect("a feud").step = step;
}

fn step_of(r: &CampRun, who: ActorId) -> Option<Step> {
    r.c.feud(r.f, who).map(|f| f.step)
}

/// Run 6 (the den's half): a Frostmaw den renews by a wait, `min(192, 6 x 2^P)` days, `P` being the
/// recent losses a player caused (EQUILIBRIUM.md §2.2, MOWZIE.md ledger 17, MZB-255 to 259, 284).
/// Played against the emulated plugin on fixed seeds.
fn den_renewal(slice: &'static str) -> RunOut {
    use rtsim_kernel::renewal::Pressure;
    let mut out = Vec::new();
    let rule = {
        let r = den_run(600, 1);
        r.c.cultures
            .get(r.c.factions[r.f as usize].culture)
            .renewal
            .expect("the Frostmaw renews by a wait")
            .rule
    };
    let day = |t: Tick| (t.0 / TICKS_PER_DAY) as u32;

    // The wait after a death, by what killed it.
    let mut rows = Vec::new();
    let mut books = true;
    for (k, by) in [
        boundary::BY_MOB,
        boundary::BY_PLAYER,
        boundary::BY_TAKEN,
        boundary::BY_NONE,
        boundary::BY_OTHER,
    ]
    .into_iter()
    .enumerate()
    {
        let mut r = den_run(610 + k as u64, 1);
        come_in(&mut r);
        let died = kill_first(&mut r, by);
        rows.push((
            by,
            wait_days(&r, died),
            pressure_of(&r),
            r.c.stats.renewal_losses,
        ));
        books &= holds(&mut r);
    }
    let near = near_days;
    check(
        &mut out,
        "killed by a mob the next wait is 6.00 days, by a player the first time 12.00",
        near(rows[0].1, 6.0) && near(rows[1].1, 12.0) && books,
        format!(
            "a mob's kill: {:?} days; a player's: {:?} days",
            rows[0].1, rows[1].1
        ),
    );
    check(
        &mut out,
        "a body taken and a death no one claims bump the pressure, a mob's kill does not",
        rows.iter().all(|r| {
            let bumps = r.0 != boundary::BY_MOB;
            near(r.1, if bumps { 12.0 } else { 6.0 })
                && r.2 == f32::from(u8::from(bumps))
                && r.3 == u64::from(bumps)
        }),
        format!(
            "(by, wait, P, losses): {:?}",
            rows.iter()
                .map(|r| (r.0, r.1.map(|w| (w * 100.0).round() / 100.0), r.2, r.3))
                .collect::<Vec<_>>()
        ),
    );

    // Farmed: each Frostmaw killed by a player 2 days after it comes. A wait is the kernel's own on
    // the days it ran (exact), and near the whole-day closed form.
    let mut r = den_run(620, 1);
    come_in(&mut r);
    let mut model = Pressure::default();
    let mut waits = Vec::new();
    let (mut exact, mut books) = (true, true);
    let mut at = r.c.now();
    for _ in 0..8 {
        let died = at + 1;
        let u = r.plugin.camps[&r.key].bodies[0].uuid;
        r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, died);
        r.run(died + 5);
        model.bump(day(died), 1.0, rule.half_life_days);
        let Some(booked) = arrival_at(&r) else {
            exact = false;
            break;
        };
        exact &= booked.0 - died.0 == rule.delay_ticks(model, day(died));
        waits.push((booked.0 - died.0) as f64 / TICKS_PER_DAY as f64);
        r.run(booked + 10);
        exact &= slot_counts(&r) == [1, 0, 0, 0, 0];
        books &= holds(&mut r);
        at = booked + at_day(2.0).0;
        r.run(at);
    }
    let want = [12.0, 22.45, 36.4, 47.2, 50.6];
    let close = waits.len() >= 5
        && waits
            .iter()
            .zip(want)
            .all(|(g, w)| (g - w).abs() <= 0.03 * w);
    check(
        &mut out,
        "a farmed den waits as the kernel's rule says, and near the closed form's 12.0, 22.45, 36.4, \
         47.2, 50.6",
        exact && close && books && r.c.stats.arrivals == 8,
        format!(
            "waits {:?} days ({} arrivals); every one the kernel's wait on its day: {exact}",
            waits
                .iter()
                .map(|w| (w * 100.0).round() / 100.0)
                .collect::<Vec<_>>(),
            r.c.stats.arrivals
        ),
    );

    // Farmed for 60 game years (96 days each): the rate settles at 96 / the steady state's gap.
    let (p_star, gap) = rule.farmed(1, 2.0);
    let want_rate = 96.0 / f64::from(gap);
    let years = 60.0;
    let end = at_day(years * 96.0);
    let mut r = den_run(621, 1);
    come_in(&mut r);
    let mut at = r.c.now();
    let (mut books, mut ended) = (true, false);
    for _ in 0..400 {
        let died = at + 1;
        let u = r.plugin.camps[&r.key].bodies[0].uuid;
        r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, died);
        r.run(died + 5);
        books &= holds(&mut r);
        let Some(booked) = arrival_at(&r) else { break };
        if booked > end {
            r.run(end);
            ended = true;
            break;
        }
        r.run(booked + 10);
        books &= holds(&mut r);
        at = booked + at_day(2.0).0;
        r.run(at);
    }
    let rate = r.c.stats.arrivals as f64 / years;
    check(
        &mut out,
        "farmed for 60 game years the den's arrivals per 96 days are within 5% of the closed form's",
        ended && books && (rate - want_rate).abs() <= 0.05 * want_rate,
        format!(
            "{} arrivals, {rate:.3} per 96 days against {want_rate:.3} (P settles at {p_star:.3}, a \
             wait of {:.2} days and a kill every {gap:.2})",
            r.c.stats.arrivals,
            f64::from(gap) - 2.0
        ),
    );

    // A den found empty gets a migrant after the cap.
    let mut r = den_run(630, 0);
    let wait = wait_days(&r, Tick(1));
    r.run(at_day(191.0));
    let before = r.c.stats.arrivals;
    r.run(at_day(193.0));
    let owed = slot_counts(&r);
    come_in(&mut r);
    let spawn = r
        .plugin
        .applied
        .iter()
        .find(|a| a.line.action == boundary::ACTION_SPAWN)
        .map(|a| a.line.flags);
    check(
        &mut out,
        "a den found empty gets its migrant after 192.00 days, owed, then spawned with its crystal",
        near(wait, 192.0)
            && before == 0
            && r.c.stats.arrivals == 1
            && owed == [0, 1, 0, 0, 0]
            && slot_counts(&r) == [1, 0, 0, 0, 0]
            && spawn
                .is_some_and(|f| f & boundary::LINE_CRYSTAL != 0 && f & boundary::LINE_CHIEF != 0)
            && holds(&mut r),
        format!(
            "booked {wait:?} days; none by day 191, {} by 193, {owed:?} (slots) while out; then {:?}",
            r.c.stats.arrivals,
            slot_counts(&r)
        ),
    );

    // A stolen crystal is no loss: no wait, no pressure; the next arrival comes with its crystal.
    let mut r = den_run(640, 1);
    come_in(&mut r);
    let at = r.c.now() + 1;
    let stole = r.plugin.steal(r.key, at);
    let to = r.c.now() + 5;
    r.run(to);
    let (waits, p, losses) = (arrival_at(&r), pressure_of(&r), r.c.stats.renewal_losses);
    let robbed = !r.c.factions[r.f as usize].crystal;
    let stands = slot_counts(&r) == [1, 0, 0, 0, 0];
    let died = kill_first(&mut r, boundary::BY_PLAYER);
    r.run(died + at_day(12.0).0 + 10);
    check(
        &mut out,
        "a stolen crystal starts no wait, and the next arrival's line has the crystal",
        stole
            && waits.is_none()
            && p == 0.0
            && losses == 0
            && robbed
            && stands
            && r.c.stats.arrivals == 1
            && r.c.factions[r.f as usize].crystal
            && line_flags(&r)
                .last()
                .is_some_and(|f| f & boundary::LINE_CRYSTAL != 0)
            && holds(&mut r),
        format!(
            "wait {waits:?}, P {p}, {losses} losses, robbed {robbed}, the Frostmaw stands {stands}; \
             after its death {} arrival, crystal back {}",
            r.c.stats.arrivals, r.c.factions[r.f as usize].crystal
        ),
    );

    // A feud with a busy line, its grievance fed every day as the meter will feed it (M7), wakes a
    // handful of times over the wait, never once a tick.
    let mut r = den_run(650, 1);
    come_in(&mut r);
    let raid = theta(&r.c, r.f, Step::Raid);
    let wakes = r.c.stats.feud_wakes;
    let died = kill_first(&mut r, boundary::BY_PLAYER);
    let booked = arrival_at(&r).expect("a wait");
    let mut d = r.c.now().0 as f64 / TICKS_PER_DAY as f64;
    while at_day(d) < booked {
        r.kill(1.5 * raid / 8.0);
        d += 1.0;
        r.run(at_day(d).min(booked));
    }
    let grew = r.c.stats.feud_wakes - wakes;
    let step = step_of(&r, r.player);
    r.run(booked + 10);
    check(
        &mut out,
        "a feud with a busy line wakes a handful of times over the wait, never once a tick",
        grew < 10 && r.c.stats.arrivals == 1 && holds(&mut r),
        format!(
            "{grew} feud wakes over the {:.1} days from the death to the arrival (the feud at \
             {step:?}); {} arrival",
            booked.0 as f64 / TICKS_PER_DAY as f64 - died.0 as f64 / TICKS_PER_DAY as f64,
            r.c.stats.arrivals
        ),
    );

    // A save and load in the middle of the wait: the arrival comes on the same tick.
    let end = |save: bool| {
        let mut r = den_run(660, 1);
        come_in(&mut r);
        let died = kill_first(&mut r, boundary::BY_PLAYER);
        r.run(died + at_day(5.0).0);
        let mut same = true;
        if save {
            r.round_trip();
            same &= r.failures.is_empty();
            let c = r.c.camp(r.f).expect("a ledger");
            same &= c.pressure.parts().0 == 1.0 && c.arrival_at == Some(died + at_day(12.0).0);
        }
        r.run(Tick((died + at_day(12.0).0).0 - 1));
        let before = r.c.stats.arrivals;
        r.run(died + at_day(12.0).0);
        let on_the_tick = r.c.stats.arrivals == before + 1;
        r.run(died + at_day(12.0).0 + 10);
        (
            r.c.state_hash(),
            slot_counts(&r),
            same && on_the_tick && holds(&mut r),
        )
    };
    let (kept, loaded) = (end(false), end(true));
    check(
        &mut out,
        "a save and load mid-wait arrives on the same tick, to the same books",
        kept == loaded && kept.2,
        format!(
            "state {:016x} against {:016x}, then {:?}",
            kept.0, loaded.0, loaded.1
        ),
    );

    RunOut {
        run: 6,
        built: false,
        slice,
        checks: out,
        rungs: Vec::new(),
        stats: None,
        log: Vec::new(),
    }
}

/// Walls that let `survived` of each band through (0: every raid on a district is lost to the last
/// unit), and a record of the bands they met: (count, strength).
struct Walls {
    survived: f64,
    seen: std::sync::Arc<std::sync::Mutex<Vec<(u32, f64)>>>,
}

type Met = std::sync::Arc<std::sync::Mutex<Vec<(u32, f64)>>>;

impl Walls {
    fn new(survived: f64) -> (Walls, Met) {
        let seen = Met::default();
        (
            Walls {
                survived,
                seen: seen.clone(),
            },
            seen,
        )
    }
}

impl Infrastructure for Walls {
    fn districts_near(&mut self, _: u16, _: i64, _: i64, _: i64) -> Vec<DistrictSite> {
        Vec::new()
    }

    fn raid(&mut self, t: &Threat) -> Option<RaidOutcome> {
        self.seen
            .lock()
            .expect("the record")
            .push((t.count, t.strength));
        Some(RaidOutcome {
            survived: self.survived,
            floor: None,
            untested: false,
            stolen: GoodVec::new(),
            destroyed_blocks: 0,
            machine_lost: false,
            skipped: None,
        })
    }

    fn good_name(&self, _: Good) -> Option<String> {
        None
    }
}

/// A den whose player's feud has sent the Frostmaw on a raid at the refinery: grievance one and a
/// half times Raid's threshold (below Invasion's, which is twice it), the camp out so the band
/// goes abstractly, and either `Some(killer)` kills the chief in the fight a watching player picks
/// (the emulated player is `killer`), or `None`, the district's walls kill the whole band.
fn feud_raid(seed: u64, killer: Option<boundary::Uuid>) -> CampRun {
    let mut r = den_run(seed, 1);
    match killer {
        Some(k) => {
            r.plugin.player = k;
            r.plugin.fight = Some(synth::Fight {
                kill_share: 0.0,
                after: 200,
                kill_chief: true,
                only_at_base: false,
            });
            r.watch = true;
        }
        None => r.infra = Box::new(Walls::new(0.0).0),
    }
    come_in(&mut r);
    let raid = theta(&r.c, r.f, Step::Raid);
    r.kill(1.5 * raid / 8.0);
    r.camp_out();
    let mut t = r.c.now();
    while r.c.stats.chiefs_killed == 0 && t < at_day(30.0) {
        t += 400;
        r.run(t);
    }
    let to = r.c.now() + 100;
    r.run(to);
    r
}

/// The war parties about walk home, for up to 12 days: whether none is left.
fn bands_home(r: &mut CampRun) -> bool {
    let limit = r.c.now() + at_day(12.0).0;
    let mut t = r.c.now();
    while war_parties(r) > 0 && t < limit {
        t += 400;
        r.run(t);
    }
    war_parties(r) == 0
}

/// Runs on past the cowed window an hour at a time, watching one player's feud: the highest rung
/// it reaches, the days after the window ends that it takes to fall back to nothing (`None` if it
/// has not within 30 days), and the rungs the faction climbed meanwhile (`stats.escalations`, any
/// feud: a roar is a Notice, a raid goes out at Raid).
fn feud_past_the_cowed_window(
    r: &mut CampRun,
    who: ActorId,
) -> (Step, Option<f64>, [u64; Step::COUNT]) {
    let end = r.c.factions[r.f as usize].cowed_until;
    let before = r.c.stats.escalations;
    let mut t = r.c.now();
    let mut most = step_of(r, who).unwrap_or(Step::None);
    let limit = end.0 + at_day(30.0).0;
    let mut gone = None;
    while t.0 < limit {
        t += HOUR;
        r.run(t);
        let s = step_of(r, who).unwrap_or(Step::None);
        most = most.max(s);
        if t > end && s == Step::None && most > Step::None {
            gone = Some((t.0 - end.0) as f64 / TICKS_PER_DAY as f64);
            break;
        }
    }
    let mut climbed = [0; Step::COUNT];
    for (i, c) in climbed.iter_mut().enumerate() {
        *c = r.c.stats.escalations[i] - before[i];
    }
    (most, gone, climbed)
}

/// Run 7: which feud steps down when the Frostmaw, a camp's chief, falls (MOWZIE.md §4.2 *Kills*,
/// MZB-262 to 266, 285), and run 1's tail: cowed, the heir, the body 12 days after a first kill.
fn den_kills(slice: &'static str) -> RunOut {
    use boundary::Uuid;
    const B: Uuid = (0x0713, 0xE2);
    const C: Uuid = (0x1D1E, 0x01);
    let mut out = Vec::new();

    // (a) The feud's player kills the chief on its raid: the feud steps down, from Raid to Notice.
    let mut r = feud_raid(710, Some(synth::PLAYER));
    let a = step_of(&r, r.player);
    let home = bands_home(&mut r);
    check(
        &mut out,
        "the feud's player kills the chief on a raid: Raid steps down to Notice, the band routs home",
        r.c.stats.chiefs_killed == 1
            && r.c.stats.successions == 1
            && a == Some(Step::Notice)
            && home
            && pressure_of(&r) == 1.0
            && holds(&mut r),
        format!(
            "{} fall, the feud at {a:?}, the band home {home}, P {}",
            r.c.stats.chiefs_killed,
            pressure_of(&r)
        ),
    );

    // (b) A third player kills it on A's raid: A's feud steps down, B is reported and the heir
    // witnesses it. The heir takes over a grudge of the report's -125 at half a point of grievance
    // each, about 62 (MZB-36; MOWZIE.md's "about 2.5" is a misreading of the factor, MZB-299), more
    // than Notice's rung, but the faction is cowed, so B's feud is not raised yet.
    let mut r = feud_raid(711, Some(B));
    let b = r.c.player(&mut r.w, B);
    let a = step_of(&r, r.player);
    let heir_of_the_fall = r.c.factions[r.f as usize].chief;
    let reported =
        r.w.knowledge
            .sentiment_snapshot(heir_of_the_fall, r.c.now())
            .iter()
            .any(|&(who, v)| who == b && v < 0);
    let view = r.c.feud_view(r.key, b);
    let grievance = view.as_ref().map_or(0.0, |v| v.grievance);
    let theta_notice = theta(&r.c, r.f, Step::Notice);
    let cowed = r.c.factions[r.f as usize].cowed_until > r.c.now();
    check(
        &mut out,
        "a third player kills it on A's raid: A's feud steps to Notice, B is reported and the heir \
         witnesses it, holding about 62 grievance, which the cowed faction does not act on yet",
        r.c.stats.chiefs_killed == 1
            && a == Some(Step::Notice)
            && reported
            && cowed
            && view.as_ref().is_some_and(|v| v.step < Step::Notice)
            && (grievance - 62.5).abs() < 2.0
            && grievance > theta_notice
            && holds(&mut r),
        format!(
            "A's feud at {a:?}; the new chief feels against B: {reported}; B's feud at {:?} with \
             grievance {grievance:.1} (-125 at 0.5 a point; Notice at {theta_notice:.1}), cowed \
             {cowed}",
            view.map(|v| v.step)
        ),
    );
    // The window ends: B's grievance stands above Notice and below Envoy, so the heir roars once
    // (a Notice, no raid), and the grudge then decays away on its own.
    let (most, gone, climbed) = feud_past_the_cowed_window(&mut r, b);
    check(
        &mut out,
        "once the cowed window ends B's feud reaches Notice and no higher, and decays away",
        most == Step::Notice
            && gone.is_some()
            && climbed[Step::Notice.index()] == 1
            && climbed[Step::Envoy.index()..].iter().all(|&n| n == 0)
            && holds(&mut r),
        format!(
            "B's feud peaked at {most:?}, back to nothing {gone:?} days after the window; rungs \
             climbed meanwhile {climbed:?}"
        ),
    );

    // (c) A player with no feud kills it at the camp: A's feud stays, and C is raised to nothing.
    let mut r = den_run(712, 1);
    come_in(&mut r);
    let me = r.player;
    feud_at(&mut r, me, Step::Raid);
    r.plugin.player = C;
    let c = r.c.player(&mut r.w, C);
    kill_first(&mut r, boundary::BY_PLAYER);
    let (a, cs) = (step_of(&r, me), step_of(&r, c));
    check(
        &mut out,
        "a player with no feud kills it at the camp: A's feud stands, C is raised to nothing",
        r.c.stats.chiefs_killed == 1
            && a == Some(Step::Raid)
            && cs.is_none_or(|s| s < Step::Notice)
            && holds(&mut r),
        format!("A's feud at {a:?}, C's at {cs:?}"),
    );
    // The window ends: C, reported by the same kill, gets one Notice at most, and it decays away.
    let (most, gone, climbed) = feud_past_the_cowed_window(&mut r, c);
    check(
        &mut out,
        "once the cowed window ends C's feud reaches Notice and no higher, and decays away",
        most == Step::Notice
            && gone.is_some()
            && climbed[Step::Envoy.index()..].iter().all(|&n| n == 0)
            && holds(&mut r),
        format!(
            "C's feud peaked at {most:?}, back to nothing {gone:?} days after the window; rungs \
             climbed meanwhile {climbed:?}"
        ),
    );

    // (d) The feud's player kills it at the camp: A's feud steps down.
    let mut r = den_run(713, 1);
    come_in(&mut r);
    let me = r.player;
    feud_at(&mut r, me, Step::Raid);
    kill_first(&mut r, boundary::BY_PLAYER);
    let a = step_of(&r, me);
    check(
        &mut out,
        "the feud's player kills it at the camp: A's feud steps down to Notice",
        r.c.stats.chiefs_killed == 1 && a == Some(Step::Notice) && holds(&mut r),
        format!("A's feud at {a:?}"),
    );

    // (e) A chief body left at home by its mustering band is killed there: the band routs and the
    // heir succeeds.
    let mut r = den_run(714, 1);
    come_in(&mut r);
    let p = r.raid().expect("a raid goes");
    let left = slot_counts(&r);
    let led = r.c.parties[p].chief;
    kill_first(&mut r, boundary::BY_PLAYER);
    let to = r.c.now() + at_day(2.0).0;
    r.run(to);
    check(
        &mut out,
        "a chief body left at home is killed there: its band routs and the heir succeeds",
        led && left == [0, 0, 1, 0, 0]
            && r.c.stats.chiefs_killed == 1
            && r.c.stats.successions == 1
            && war_parties(&r) == 0
            && r.c.factions[r.f as usize].away == 0
            && pressure_of(&r) == 1.0
            && holds(&mut r),
        format!(
            "the band led by the Frostmaw {led}, its slot {left:?}; then {} fall, {} succession, \
             {} bands about, P {}",
            r.c.stats.chiefs_killed,
            r.c.stats.successions,
            war_parties(&r),
            pressure_of(&r)
        ),
    );

    // (f) An abstract wipe-out on a district: the chief falls with no killer, the band's feud steps
    // down and the pressure is bumped.
    let mut r = feud_raid(715, None);
    let a = step_of(&r, r.player);
    let heard = r.c.stats.raids_district;
    let home = bands_home(&mut r);
    check(
        &mut out,
        "a band lost to a district's walls loses its chief with no killer: its feud steps down, the \
         pressure is bumped",
        heard == 1
            && r.c.stats.chiefs_killed == 1
            && a == Some(Step::Notice)
            && pressure_of(&r) == 1.0
            && home
            && holds(&mut r),
        format!(
            "{heard} raid met the walls, {} fall, the feud at {a:?}, P {}",
            r.c.stats.chiefs_killed,
            pressure_of(&r)
        ),
    );

    // (g) Run 1's tail: a first kill at the den. The faction is cowed, the heir is chief and the
    // body comes 12 days after, the feud waking a handful of times, not once a tick.
    let mut r = den_run(716, 1);
    come_in(&mut r);
    let chief = r.c.factions[r.f as usize].chief;
    let wakes = r.c.stats.feud_wakes;
    let died = kill_first(&mut r, boundary::BY_PLAYER);
    let cowed = r.c.factions[r.f as usize].cowed_until > died;
    let heir = r.c.factions[r.f as usize].chief != chief;
    let wait = wait_days(&r, died);
    let end = died + at_day(14.0).0;
    r.run(end);
    let grew = r.c.stats.feud_wakes - wakes;
    check(
        &mut out,
        "a first kill at the den: cowed, the heir chief, the body 12 days after, the feud wakes a \
         handful of times",
        cowed
            && heir
            && near_days(wait, 12.0)
            && r.c.stats.arrivals == 1
            && slot_counts(&r) == [1, 0, 0, 0, 0]
            && grew < 10
            && holds(&mut r),
        format!(
            "cowed {cowed}, the heir chief {heir}, the next Frostmaw booked {wait:?} days after, \
             {} arrival; {grew} feud wakes over the 14 days",
            r.c.stats.arrivals
        ),
    );

    RunOut {
        run: 7,
        built: true,
        slice,
        checks: out,
        rungs: Vec::new(),
        stats: None,
        log: Vec::new(),
    }
}

// ---- the grove (slice M9a): the rest of runs 5, 6 and 10, and run 12 ----------------------------

/// The Umvuthi's slot: its state and generation.
fn chief_of(r: &CampRun) -> Option<(SlotState, u32)> {
    r.c.camp(r.f)
        .and_then(|c| c.chief())
        .map(|s| (s.state, s.generation))
}

/// The Umvuthi bodies standing in the camp's world.
fn chief_bodies(r: &CampRun) -> Vec<synth::CampBody> {
    r.plugin.camps.get(&r.key).map_or_else(Vec::new, |c| {
        c.bodies
            .iter()
            .filter(|b| b.slot == CHIEF_SLOT && b.generation != 0)
            .copied()
            .collect()
    })
}

/// The warrior bodies standing in the camp's world.
fn warrior_bodies(r: &CampRun) -> Vec<synth::CampBody> {
    r.plugin.camps.get(&r.key).map_or_else(Vec::new, |c| {
        c.bodies
            .iter()
            .filter(|b| b.slot != CHIEF_SLOT || b.generation == 0)
            .copied()
            .collect()
    })
}

/// Where the next heir is due, if one is awaited.
fn heir_due(r: &CampRun) -> Option<Tick> {
    r.c.camp(r.f).and_then(|c| c.heir).map(|h| h.due)
}

/// The shipped cultures with the grove's chief wait changed to `min(cap, base x 2^P)` days, `P`
/// decaying with `half_life` (all days).
fn grove_cultures(base: f32, cap: f32, half_life: f32) -> Cultures {
    let mut cultures = Cultures::builtin();
    let i = usize::from(cultures.index_of("umvuthana").expect("the grove"));
    let cu = &mut cultures.list[i];
    let def = cu.def.chief_renewal.as_mut().expect("a chief renewal def");
    def.base_days = base;
    def.cap_days = cap;
    def.half_life_days = half_life;
    let rule = def
        .rule("umvuthana", "chief_renewal")
        .expect("a resolvable rule");
    cu.chief_renewal.as_mut().expect("a chief renewal").rule = rule;
    cultures
}

/// A grove of nine whose plunder drive is off, so the bands a run orders are the only ones about.
fn quiet_grove(seed: u64) -> CampRun {
    let mut cultures = Cultures::builtin();
    for k in &mut cultures.list {
        k.def.plunder_mean_days = None;
    }
    CampRun::with_cultures(seed, cultures, "umvuthana", 9)
}

/// The camp comes in and waits out its first heir: the generation of the Umvuthi seated, if one.
fn seat_first(r: &mut CampRun) -> Option<u32> {
    come_in(r);
    let due = heir_due(r)?;
    r.run(due + 100);
    chief_of(r).filter(|c| c.0 == SlotState::Body).map(|c| c.1)
}

/// A band of the grove's order ordered at the refinery now, and the camp's answers delivered.
fn send(r: &mut CampRun, kind: PartyKind) -> Option<rtsim_kernel::handle::Handle<Party>> {
    let at = r.c.now();
    let target = Target::District(synth::REFINERY, synth::REFINERY_POS);
    let p = r.c.order_party(&mut r.w, r.key, kind, target, None, at);
    r.flush(at);
    p
}

/// Run 5, the rest (slice M9a): what the grove adds to the ladder. Each is a check.
fn grove_rest(out: &mut Vec<Check>) {
    plunder_livestock(out);
    invasion_with_chief(out);
    grove_in_and_out(out);
    hunts_capped(out);
    gifts_in_nuggets(out);
}

/// MZB-427: a grove's plunder band takes its `plunder_goods` (leather and wool) and nothing else,
/// whatever it carries (a raptor carries nothing), and eats what it takes where it stands: the
/// goods are booked destroyed, none lands at the haven and the band brings nothing home. The
/// villages put up no defence, so the band lives to take what it can; eight worlds.
fn plunder_livestock(out: &mut Vec<Check>) {
    let (mut raided, mut eaten, mut bad) = (0u32, 0i128, Vec::<String>::new());
    for seed in 1..=8u64 {
        let (mut w, _) = synth::world(seed);
        let mut c = synth::conflict(seed, &w);
        c.tun.village_defence_per_pop = 0.0;
        for k in &mut c.cultures.list {
            k.def.plunder_mean_days = None;
        }
        c.add_faction(
            &mut w,
            synth::CAMP,
            "umvuthana",
            synth::CAMP_POS,
            Tick::ZERO,
        )
        .expect("the grove");
        let culture = c
            .cultures
            .get(c.cultures.index_of("umvuthana").expect("the grove"))
            .clone();
        let mut prey: Vec<usize> = (0..w.nodes.len())
            .filter(
                |&i| matches!(w.nodes[i].kind, NodeKind::Village(cu) if culture.hates(cu.name())),
            )
            .collect();
        prey.sort_by_key(|&i| {
            let p = w.nodes[i].pos;
            let dx = i64::from(p.x - synth::CAMP_POS.x);
            let dz = i64::from(p.z - synth::CAMP_POS.z);
            (dx * dx + dz * dz, i)
        });
        let Some(&i) = prey.first() else { continue };
        let Some(p) = c.order_party(
            &mut w,
            synth::CAMP,
            PartyKind::Plunder,
            Target::Node(i),
            None,
            Tick::from_days(1),
        ) else {
            bad.push(format!("seed {seed}: no band"));
            continue;
        };
        // Run to the band's arrival, and the world with it.
        let at = loop {
            let x = &c.parties[p];
            if x.phase == Phase::Out && x.seg as usize + 2 >= x.route.len() {
                let at = x.times[x.seg as usize + 1];
                w.run_until(at);
                break at;
            }
            let Some(next) = c.next_due() else {
                bad.push(format!("seed {seed}: the band never arrives"));
                break Tick::ZERO;
            };
            c.run_until(&mut w, &mut NoInfrastructure, next);
        };
        if at == Tick::ZERO {
            continue;
        }
        // Every good stands 100 items over its target.
        let goods: Vec<rtsim_trade::GoodId> = w.goods.ids().collect();
        let m = w.market(i);
        let rich: BTreeMap<rtsim_trade::GoodId, rtsim_trade::Amount> = goods
            .iter()
            .map(|&g| (g, m.line(g).target + 100 * MILLI - m.stock(g)))
            .filter(|e| e.1 > 0)
            .collect();
        w.stock_from_players(i, &rich, at);
        let (leather, wool) = (
            w.goods.find("leather").expect("leather"),
            w.goods.find("wool").expect("wool"),
        );
        let m = w.market(i);
        let reach: i128 = [leather, wool]
            .iter()
            .map(|&g| {
                i128::from(
                    (m.stock(g) - m.line(g).target).max(0) * culture.def.loot_share_milli / 1000,
                )
            })
            .sum();
        let taken = |c: &Conflict, g| c.ledger.from_villages.get(&g).copied().unwrap_or(0);
        let before: Vec<i128> = goods.iter().map(|&g| taken(&c, g)).collect();
        let landed = c.ledger.landed.clone();
        let raids = c.stats.raids_village;
        c.run_until(&mut w, &mut NoInfrastructure, at);
        if c.stats.raids_village == raids {
            continue;
        }
        raided += 1;
        let mut items = 0;
        for (k, &g) in goods.iter().enumerate() {
            let got = taken(&c, g) - before[k];
            if g == leather || g == wool {
                items += got;
            } else if got != 0 {
                bad.push(format!("seed {seed}: took {got} of good {k}, no livestock"));
            }
        }
        eaten += items;
        let strange = c
            .ledger
            .destroyed
            .keys()
            .filter(|&&g| g != leather && g != wool)
            .count();
        if items <= 0 || items > reach {
            bad.push(format!(
                "seed {seed}: took {} milli-items, reach {reach}",
                items
            ));
        }
        if strange != 0 || c.ledger.landed != landed || !c.parties[p].loot.is_empty() {
            bad.push(format!(
                "seed {seed}: livestock was not eaten where it stood"
            ));
        }
        while c.parties.get(p).is_some() {
            let Some(next) = c.next_due() else { break };
            c.run_until(&mut w, &mut NoInfrastructure, next);
        }
        w.run_until(c.now());
        if let Err(e) = c.check(&w) {
            bad.push(format!("seed {seed}: books: {e:?}"));
        }
    }
    check(
        out,
        "plunder takes leather and wool only, up to the band's reach whatever it carries, and eats \
         it where it stands",
        bad.is_empty() && raided >= 4,
        if bad.is_empty() {
            format!(
                "{raided} raids of 8 worlds ate {} items; nothing else taken, none landed",
                eaten / i128::from(MILLI)
            )
        } else {
            format!(
                "{} findings, first: {:?}",
                bad.len(),
                &bad[..bad.len().min(3)]
            )
        },
    );
}

/// MZB-413, MOWZIE.md ledger 4: the Umvuthi leads an Invasion and no raid. The Invasion's band
/// (six warriors, two raptors) with its chief meets the district's walls at 6 x 0.4 + 2 x 2.6 + 5 =
/// 12.6; a raid of three warriors and a raptor at 3.8, the chief left on its throne. Home again,
/// the chief stands in its slot as before.
fn invasion_with_chief(out: &mut Vec<Check>) {
    let mut r = quiet_grove(8);
    let (walls, met) = Walls::new(1.0);
    r.infra = Box::new(walls);
    let Some(seat) = seat_first(&mut r) else {
        check(
            out,
            "an Invasion draws the Umvuthi and fights at 12.6",
            false,
            "no heir".into(),
        );
        return;
    };
    let raid = send(&mut r, PartyKind::Raid).is_some();
    let stays = chief_of(&r) == Some((SlotState::Body, seat));
    r.settle();
    let invasion = send(&mut r, PartyKind::Invasion);
    let leaves = chief_of(&r).map(|c| c.0) == Some(SlotState::Left)
        && invasion.is_some_and(|p| r.c.parties[p].chief);
    r.settle();
    let home = chief_of(&r) == Some((SlotState::Body, seat)) && chief_bodies(&r).len() == 1;
    let fights: Vec<(u32, f64)> = met.lock().expect("the record").clone();
    let held = holds(&mut r);
    check(
        out,
        "an Invasion draws the Umvuthi and fights at 12.6; a raid leaves it on its throne at 3.8",
        raid && stays
            && leaves
            && home
            && held
            && fights.len() == 2
            && (fights[0].1 - 3.8).abs() < 1e-9
            && (fights[1].1 - 12.6).abs() < 1e-9,
        format!(
            "fights (count, strength) {fights:?}; the chief stayed for the raid: {stays}, left with \
             the Invasion: {leaves}, home in its generation: {home}; books hold: {held}"
        ),
    );
}

/// The grove in and out with bands abroad (the plugin's census answering, MOWZIE.md §4.9), the
/// Umvuthi leading the Invasions: the camp goes out while a band is away and comes in again. No
/// census books a living survivor dead, the pool refills to its cap while a band is out, and
/// every survivor's slot is kept (I1 to I6 after every round).
fn grove_in_and_out(out: &mut Vec<Check>) {
    let mut r = quiet_grove(9);
    let (walls, _) = Walls::new(0.5);
    r.infra = Box::new(walls);
    let Some(seat) = seat_first(&mut r) else {
        check(out, "the grove in and out", false, "no heir".into());
        return;
    };
    let (mut ok, mut why) = (true, String::new());
    let mut away_refills = 0;
    for round in 0..6u32 {
        let band = send(
            &mut r,
            if round % 2 == 0 {
                PartyKind::Invasion
            } else {
                PartyKind::Raid
            },
        );
        ok &= band.is_some();
        // The camp is unloaded while the band is out, and a day later loaded again.
        let at = r.c.now() + 50;
        r.run(at);
        r.camp_out();
        let at = r.c.now() + at_day(1.0).0;
        r.run(at);
        r.camp_in();
        let at = r.c.now() + 20;
        r.run(at);
        if band.is_some_and(|p| r.c.parties.get(p).is_some()) {
            // The band is still out: the pool refilled to its cap meanwhile, the survivors' slots
            // kept (the audit says the books match the world).
            let away = r.c.camp(r.f).map_or(0, |c| c.away_count());
            away_refills += u32::from(away > 0);
        }
        r.settle();
        let at = r.c.now() + at_day(6.0).0;
        r.run(at);
        let books = holds(&mut r);
        let counts = slot_counts(&r);
        let full = counts[SlotState::Body as usize] == 10 && warrior_bodies(&r).len() == 9;
        if !books || !full || chief_of(&r).map(|c| c.0) != Some(SlotState::Body) {
            ok = false;
            why = format!("round {round}: books {books}, slots {counts:?}");
            break;
        }
    }
    let lost = r.c.stats.camp_written_off;
    check(
        out,
        "the grove in and out: bands leave and come home, the pool refills to its cap, no census \
         books a living survivor dead",
        ok && lost == 0 && r.ghosts.is_empty() && chief_of(&r).map(|c| c.1) == Some(seat),
        format!(
            "6 rounds of Invasion and Raid with the camp out and in; {} deaths booked, {lost} \
             written off, {} ghosts, {away_refills} rounds with a band away at the census; {why}",
            r.c.stats.camp_deaths,
            r.ghosts.len()
        ),
    );
}

/// MZB-424, MZB-449: hunts are never over the culture's cap. A grove's raids on one district, each
/// survived whole, leave two animals a raptor; they add up to the cap (8) and no further, under a
/// new op each time; the plugin's answer, whatever it killed, spends them.
fn hunts_capped(out: &mut Vec<Check>) {
    let mut r = quiet_grove(11);
    let (walls, _) = Walls::new(1.0);
    r.infra = Box::new(walls);
    come_in(&mut r);
    let cap =
        r.c.cultures
            .get(r.c.factions[r.f as usize].culture)
            .def
            .hunt_cap;
    let (mut most, mut counts, mut ops) = (0, Vec::new(), std::collections::BTreeSet::new());
    for _ in 0..8 {
        send(&mut r, PartyKind::Raid);
        r.settle();
        let owed: Vec<_> = r.c.factions[r.f as usize]
            .hunts
            .iter()
            .map(|h| (h.count, h.op))
            .collect();
        for &(n, op) in &owed {
            most = most.max(n);
            ops.insert(op);
        }
        counts.push(owed.iter().map(|e| e.0).sum::<u32>());
    }
    let sent = r
        .cmds
        .iter()
        .filter(|(_, c)| matches!(c, Cmd::OwedHunt(_)))
        .count();
    let at = r.c.now() + 1;
    r.plugin.apply_hunts(3, at);
    r.run(at + 5);
    let spent = r.c.factions[r.f as usize].hunts.is_empty() && r.c.stats.hunts_killed == 3;
    check(
        out,
        "hunts are never over the culture's cap, and an answer spends them whatever it killed",
        most == cap
            && counts.iter().all(|&n| n <= cap)
            && ops.len() == sent
            && spent
            && holds(&mut r),
        format!(
            "owed after each of 8 raids {counts:?} (cap {cap}); {} ops over {sent} commands; the \
             plugin killed 3 and the hunt is spent: {spent}",
            ops.len()
        ),
    );
}

/// The relief a player's payment brings (the grievance it takes off a killing's), in a fresh grove.
fn gift_relief(pay: impl FnOnce(&mut CampRun)) -> f64 {
    let mut r = grove_run(10, 9);
    r.kill(40.0);
    let g0 = r.c.feud_view(r.key, r.player).map_or(0.0, |v| v.grievance);
    pay(&mut r);
    g0 - r.c.feud_view(r.key, r.player).map_or(0.0, |v| v.grievance)
}

/// A payment of `n` of the good `name` to a stray Umvuthi (a gift), as the plugin reports it.
fn gift(r: &mut CampRun, name: &str, n: i32) {
    let g = r.w.goods.find(name).expect("a good of the synthetic world");
    let id = item_of(g);
    r.c.define(2, id, name);
    let at = r.c.now();
    r.c.ingest(
        &mut r.w,
        &mut *r.infra,
        &Obs::ChiefPaid(boundary::ChiefPaid {
            faction: r.key,
            slot: 0,
            generation: 0,
            flags: boundary::PAID_STRAY,
            player: synth::PLAYER,
            op: 0x31,
            items: vec![boundary::Item { item: id, count: n }],
        }),
        at,
    );
}

/// MZB-416, MZB-447, MOWZIE.md ledger 12: no market prices gold, so a gold gift is worth its
/// nuggets over the grove's 9 a coin: 81 nuggets soften a feud as 9 emeralds do, and a trade's 810
/// at a tenth of that.
fn gifts_in_nuggets(out: &mut Vec<Check>) {
    let nuggets = gift_relief(|r| gift(r, "gold_nugget", 81));
    let coins = gift_relief(|r| gift(r, "emerald", 9));
    let traded = gift_relief(|r| {
        let at = r.c.now();
        let t = Obs::Traded(boundary::Traded {
            faction: r.key,
            player: synth::PLAYER,
            nuggets: 810,
            op: 0x33,
        });
        r.c.ingest(&mut r.w, &mut *r.infra, &t, at);
    });
    check(
        out,
        "gold gifts are worth nuggets over 9 a coin, a trade a tenth of that",
        nuggets > 1.0 && (nuggets - coins).abs() < 1e-9 && (traded - coins).abs() < 1e-9,
        format!(
            "relief of 81 nuggets {nuggets:.4}, of 9 emeralds {coins:.4}, of a trade of 810 nuggets {traded:.4}"
        ),
    );
}

/// Run 6, the grove (slice M9a): the warriors refill by their rate with the chief's slot
/// untouched; a grove found without its Umvuthi seats its heir at the cap's 192 days, or at once
/// when a stray turns up.
fn grove_renewal(out: &mut Vec<Check>) {
    // An emptied grove refills to its cap of 9 at 2 a day.
    let mut r = grove_run(650, 9);
    let seat = seat_first(&mut r);
    let throne = chief_bodies(&r).first().map(|b| b.uuid);
    let died = r.c.now() + 1;
    for b in warrior_bodies(&r) {
        r.plugin.camp_kill(r.key, b.uuid, boundary::BY_PLAYER, died);
    }
    r.run(died + 5);
    let emptied = warrior_bodies(&r).len();
    let (mut full, mut day_one, mut over) = (None, 0, false);
    let mut t = died;
    while t < died + at_day(10.0).0 {
        t += HOUR;
        r.run(t);
        let n = warrior_bodies(&r).len();
        if (t.0 - died.0) / HOUR == 24 {
            day_one = n;
        }
        over |= n > 9;
        if full.is_none() && n == 9 {
            full = Some((t.0 - died.0) as f64 / TICKS_PER_DAY as f64);
        }
    }
    let same = chief_of(&r) == seat.map(|g| (SlotState::Body, g))
        && chief_bodies(&r).first().map(|b| b.uuid) == throne;
    check(
        out,
        "an emptied grove refills to its cap of 9 at 2 a day, its chief untouched",
        seat.is_some()
            && emptied == 0
            && !over
            && same
            && full.is_some_and(|d| (4.0..5.0).contains(&d))
            && (1..=3).contains(&day_one)
            && holds(&mut r),
        format!(
            "{emptied} left, {day_one} after one day, full after {full:?} days; the Umvuthi stands in \
             its own slot and body: {same}"
        ),
    );

    // A grove found without its Umvuthi: its heir at the cap's 192 days, nothing before.
    let mut r = grove_run(651, 9);
    let wait = heir_due(&r).map(|t| (t.0 - 1) as f64 / TICKS_PER_DAY as f64);
    come_in(&mut r);
    r.run(at_day(191.0));
    let before = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_MASK)
        .count();
    let seat = seat_first(&mut r);
    let masked = r
        .plugin
        .applied
        .iter()
        .filter(|a| a.line.action == boundary::ACTION_MASK && a.outcome == boundary::LINE_DONE)
        .count();
    check(
        out,
        "a grove found without its Umvuthi seats its heir at 192.00 days, none before",
        near_days(wait, 192.0) && before == 0 && seat.is_some() && masked == 1 && holds(&mut r),
        format!(
            "booked at {wait:?} days; {before} masks by day 191; seated {seat:?} with {masked} mask; \
             {} warriors and one Umvuthi stand",
            warrior_bodies(&r).len()
        ),
    );

    // Or at once, when its census finds an untagged Umvuthi.
    let mut r = grove_run(652, 9);
    let umvuthi =
        rtsim_conflict::engine::body_id(&r.c.cultures, "mic_rtsim:conflict/umvuthana/umvuthi");
    if let Some(b) = r.plugin.camp_world(r.key).bodies.first_mut() {
        b.body = umvuthi;
        b.chief = true;
    }
    come_in(&mut r);
    check(
        out,
        "a grove whose census finds an untagged Umvuthi adopts it at once, its wait over",
        heir_due(&r).is_none()
            && matches!(chief_of(&r), Some((SlotState::Body, g)) if g != 0)
            && chief_bodies(&r).len() == 1
            && warrior_bodies(&r).len() == 9
            && r.c.now() < at_day(1.0)
            && holds(&mut r),
        format!(
            "chief {:?} at tick {}, {} warriors",
            chief_of(&r),
            r.c.now().0,
            warrior_bodies(&r).len()
        ),
    );
}

/// Run 10's grove arm (slice M9a): chaos on groves whose chief waits hours, not months, so heirs
/// are seated again and again between the camp's comings and goings, Invasions that take the chief,
/// kills, answers lost, late and replayed, and session restarts. After every event: no grove holds
/// two live chief bodies, the chief is in no pool count, and each MASK that was done has its
/// SEAT_HEIR done in the same reconcile.
fn chief_chaos(seed: u64, out: &mut Vec<Check>) {
    use rtsim_kernel::rng::{StreamKey, SystemId};
    const SYS: SystemId = SystemId::named("sim.mowzie.chief_chaos");
    let (mut bad, mut events) = (Vec::<String>::new(), 0u32);
    let (mut seated, mut led, mut orders) = (0usize, 0u32, 0u32);
    for k in 0..32u64 {
        let cultures = grove_cultures(0.2, 0.8, 2.0);
        let mut r = CampRun::with_cultures(seed + 300 + k, cultures, "umvuthana", 9);
        r.plugin.fight = Some(synth::Fight {
            kill_share: 0.4,
            after: 200,
            kill_chief: false,
            only_at_base: false,
        });
        r.watch = k % 2 == 0;
        let cs = seed * 1000 + k;
        r.plugin.holders_again = !cs.is_multiple_of(3);
        r.plugin.split_answers = [0, 0, 1][(cs % 3) as usize];
        let mut rng = StreamKey::new(cs, SYS, r.key).slot(0);
        // (op: a MASK done, a SEAT_HEIR done) of every reconcile applied so far.
        let mut pairs: BTreeMap<u64, (u32, u32)> = BTreeMap::new();
        let mut seen = 0;
        for e in 0..220u32 {
            if rng.below(8) == 0 && r.c.camp(r.f).is_some_and(|c| c.is_in()) {
                orders += 1;
                if send(&mut r, PartyKind::Invasion).is_some() {
                    led += u32::from(chief_of(&r).map(|c| c.0) == Some(SlotState::Left));
                }
            }
            r.chaos_step(&mut rng);
            events += 1;
            for a in &r.plugin.applied[seen..] {
                if a.outcome == boundary::LINE_DONE {
                    let e = pairs.entry(a.op).or_default();
                    e.0 += u32::from(a.line.action == boundary::ACTION_MASK);
                    e.1 += u32::from(a.line.action == boundary::ACTION_SEAT_HEIR);
                }
            }
            seen = r.plugin.applied.len();
            let tag = format!("seed {k} event {e}");
            if chief_bodies(&r).len() > 1 {
                bad.push(format!(
                    "{tag}: {} live chief bodies",
                    chief_bodies(&r).len()
                ));
            }
            if let Some(c) = r.c.camp(r.f) {
                let members: Vec<_> = c.slots.iter().filter(|s| s.id != CHIEF_SLOT).collect();
                let home = members.iter().filter(|s| s.state.home()).count() as u32;
                let away = members.iter().filter(|s| s.state.away()).count() as u32;
                // The pool is the members only, at most the cap, a dead or gone slot in none of it.
                if home != c.home_count() || away != c.away_count() || home + away > 9 {
                    bad.push(format!(
                        "{tag}: the pool counts {}+{} of {} member slots",
                        c.home_count(),
                        c.away_count(),
                        members.len()
                    ));
                }
            }
            for (op, &(mask, seat)) in &pairs {
                if mask != seat {
                    bad.push(format!(
                        "{tag}: reconcile {op:#x}: {mask} MASK done, {seat} SEAT_HEIR done"
                    ));
                }
            }
            pairs.retain(|_, v| v.0 != v.1);
        }
        // The totals were dropped as pairs balanced; count the seatings from the plugin's record.
        seated += r
            .plugin
            .applied
            .iter()
            .filter(|a| {
                a.line.action == boundary::ACTION_SEAT_HEIR && a.outcome == boundary::LINE_DONE
            })
            .count();
        r.settle();
        for f in std::mem::take(&mut r.failures) {
            bad.push(format!("seed {k}: {f}"));
        }
        for f in r.check() {
            bad.push(format!("seed {k}, at the end: {f}"));
        }
        if !r.ghosts.is_empty()
            || r.c.stats.camp_written_off != u64::from(r.ghosts_gone)
            || (r.plugin.holders_again && r.c.stats.camp_written_off != 0)
        {
            bad.push(format!(
                "seed {k}: {} members written off, {} ghosts gone, {} left",
                r.c.stats.camp_written_off,
                r.ghosts_gone,
                r.ghosts.len()
            ));
        }
    }
    check(
        out,
        "grove chaos with heirs: no two live chief bodies, the chief in no pool count, every MASK \
         done with its SEAT_HEIR done, after every event",
        bad.is_empty() && seated >= 32 && led > 0,
        if bad.is_empty() {
            format!(
                "32 groves of 220 events ({events} in all); {seated} heirs seated, {led} of {orders} \
                 Invasions took the Umvuthi"
            )
        } else {
            format!(
                "{} findings, first: {:?}",
                bad.len(),
                &bad[..bad.len().min(3)]
            )
        },
    );
}

/// Run 12: an Umvuthi killed by a player as soon as its heir's body stands, for 60 game years
/// (96 days each). Each wait is the kernel's own `min(192, base x 2^P)` on its day, and the
/// Umvuthi comes back about 1.8 times a game year at base 6 and about 1.0 at base 24, within 10%
/// (PLACEHOLDER) of EQUILIBRIUM.md's model (MZ-14).
fn heir_farmed(slice: &'static str) -> RunOut {
    use rtsim_kernel::renewal::Pressure;
    let mut out = Vec::new();
    let years = 60.0;
    let end = at_day(years * 96.0);
    let day = |t: Tick| (t.0 / TICKS_PER_DAY) as u32;
    for (base, doc) in [(6.0f32, 1.8), (24.0, 1.0)] {
        let mut r = CampRun::with_cultures(
            700 + u64::from(base as u32),
            grove_cultures(base, 192.0, 96.0),
            "umvuthana",
            9,
        );
        let ci = r.c.factions[r.f as usize].culture;
        let rule =
            r.c.cultures
                .get(ci)
                .chief_renewal
                .expect("a chief renewal")
                .rule;
        // The kill comes a few ticks after the seat: the closed form's check is under a minute.
        let (_, gap) = rule.farmed(1, 0.0);
        let want = 96.0 / f64::from(gap);
        let muster = at_day(r.c.cultures.get(ci).def.muster_days).0;
        come_in(&mut r);
        let mut model = Pressure::at_cap(&rule, 0);
        let (mut exact, mut books, mut ended) = (true, true, false);
        let (mut seats, mut waits) = (Vec::new(), Vec::new());
        for _ in 0..4000 {
            let Some(due) = heir_due(&r) else { break };
            if due > end {
                r.run(end);
                ended = true;
                break;
            }
            r.run(due + 10);
            // A band of the camp that is mustering holds the pair back to its muster's end (MZB-612):
            // the seat comes within a muster of its due tick, and the next wait is the kernel's from
            // the death all the same.
            let mut waited = 0;
            while chief_of(&r).map(|c| c.0) != Some(SlotState::Body) && waited < 96 {
                waited += 1;
                r.run(r.c.now() + muster / 24);
            }
            if chief_of(&r).map(|c| c.0) != Some(SlotState::Body) {
                exact = false;
                break;
            }
            seats.push(due);
            books &= holds(&mut r);
            let died = r.c.now() + 1;
            let u = chief_bodies(&r)[0].uuid;
            r.plugin.camp_kill(r.key, u, boundary::BY_PLAYER, died);
            r.run(died + 5);
            model.bump(day(died), 1.0, rule.half_life_days);
            let Some(next) = heir_due(&r) else {
                exact = false;
                break;
            };
            exact &= next.0 - died.0 == rule.delay_ticks(model, day(died));
            waits.push((next.0 - died.0) as f64 / TICKS_PER_DAY as f64);
        }
        books &= holds(&mut r);
        let n = seats.len();
        // The steady rate, from the first seat to the last (the first wait is the cap's).
        let rate = if n > 1 {
            (n - 1) as f64 * 96.0 / ((seats[n - 1].0 - seats[0].0) as f64 / TICKS_PER_DAY as f64)
        } else {
            0.0
        };
        let steady = &waits[waits.len().saturating_sub(10)..];
        check(
            &mut out,
            if base == 6.0 {
                "killed as fast as it comes back for 60 game years at base 6, the Umvuthi returns about 1.8 times a year"
            } else {
                "killed as fast as it comes back for 60 game years at base 24, the Umvuthi returns about 1.0 times a year"
            },
            ended
                && exact
                && books
                && (rate - want).abs() <= 0.10 * want
                && (rate - doc).abs() <= 0.10 * doc,
            format!(
                "{n} seats, {rate:.3} a game year against the closed form's {want:.3} (a wait of {:.2} \
                 days) and the doc's {doc}; every wait the kernel's on its day: {exact}; the last \
                 waits {:?} days",
                gap,
                steady
                    .iter()
                    .map(|w| (w * 100.0).round() / 100.0)
                    .collect::<Vec<_>>()
            ),
        );
    }
    RunOut {
        run: 12,
        built: true,
        slice,
        checks: out,
        rungs: Vec::new(),
        stats: None,
        log: Vec::new(),
    }
}

/// Whether a wait in days is `want` to a millionth.
fn near_days(w: Option<f64>, want: f64) -> bool {
    w.is_some_and(|w| (w - want).abs() < 1e-6)
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
