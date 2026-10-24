// SPDX-License-Identifier: AGPL-3.0-or-later
//! `sim mowzie`'s rail runs (M7): MOWZIE.md 4.3's two tables against the engine, with synthetic
//! trains on synthetic lines (`rtsim_conflict::synth::rail_events`).
//!
//! Every figure is the median of [`SEEDS`] runs whose trains' headways are jittered uniformly by
//! up to 0.4 of themselves (MZB-330), for a Frostmaw den whose chief is [`MIDDLING`] (a
//! temperament of 128 in every trait, so its thresholds are the culture's own). A settled
//! grievance is judged within 2%, a crossing within 0.6 day and the pushes of the first 20 days
//! within 4. The den stands at Gloomhollow's place, the track 150 blocks east of it (a raid of
//! 1 day's muster, 150 blocks each way, a 2 days' lurk is 3.6 days out), the line 2,600 blocks
//! long with its stations 1,200 and 1,400 blocks from the den's latitude.
//!
//! - **Run 1, the meter:** a line at 4, 12, 24 and 50 legs a day, and a trunk of 14 lines at 1.7.
//! - **Run 2, cut and stopped service:** 12 legs a day, 24 from day 2, then 4 or none from day 12.
//! - **Run 3, reroute:** the track moves off the circle, or within it, on day 12.
//! - **Run 4, two dens on one line,** and a third found on day 10.
//! - **Run 9, the trunk:** 14 lines against one; the busy line's point; two owners on one line.

use std::collections::BTreeMap;

use rtsim_conflict::Step;
use rtsim_conflict::boundary::{Obs, Uuid};
use rtsim_conflict::culture::{PartyKind, SignalKind};
use rtsim_conflict::engine::rail::rail_key;
use rtsim_conflict::engine::{Conflict, Party, StationEnd, Target};
use rtsim_conflict::synth::{self, MIDDLING, RailDen, RailEvent, RailLine, Service};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_industry::ids::DistrictId;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;

use super::{Check, HOUR, check};

/// Runs a figure is the median of (MZB-330).
const SEEDS: u64 = 8;
/// A train's headway is jittered by up to this share of itself (MZB-330).
const JITTER: f64 = 0.4;
/// Crossing and settling tolerances (MZB-330).
const DAYS_TOL: f64 = 0.6;
const SETTLE_TOL: f64 = 0.02;
const PUSHES_TOL: u64 = 4;
const DAY: f64 = TICKS_PER_DAY as f64;

/// The track's x: 150 blocks east of the den.
const TRUNK_X: f64 = 1050.0;
const SECOND: Uuid = (0x5EED, 0xB0C);

/// One of 4.3's rows.
struct Row {
    legs: &'static str,
    settles: f64,
    notice: Option<f64>,
    raid: Option<f64>,
    g140: Option<f64>,
    invasion: Option<f64>,
    pushes: u64,
}

const R4: Row = Row {
    legs: "4 legs a day",
    settles: 17.3,
    notice: Some(11.8),
    raid: None,
    g140: None,
    invasion: None,
    pushes: 19,
};
const R12: Row = Row {
    legs: "12 legs a day",
    settles: 86.6,
    notice: Some(3.0),
    raid: Some(17.1),
    g140: None,
    invasion: None,
    pushes: 24,
};
const R24: Row = Row {
    legs: "24 legs a day",
    settles: 190.4,
    notice: Some(1.8),
    raid: Some(6.4),
    g140: Some(14.1),
    invasion: Some(17.6),
    pushes: 30,
};
const TRUNK: Row = Row {
    legs: "14 lines at 1.7 legs a day on one trunk (23.8)",
    settles: 188.7,
    notice: Some(1.8),
    raid: Some(6.3),
    g140: Some(14.2),
    invasion: Some(17.5),
    pushes: 33,
};
const R50: Row = Row {
    legs: "50 legs a day",
    settles: 415.5,
    notice: Some(1.1),
    raid: Some(3.5),
    g140: Some(5.8),
    invasion: Some(7.5),
    pushes: 37,
};

fn end(id: u64, x: f64, z: f64) -> StationEnd {
    StationEnd {
        id,
        dim: 0,
        x: x as i32,
        z: z as i32,
        district: Some(id + 1000),
    }
}

/// A straight north-south path at `x`, 2,600 blocks, passing the den's latitude in the middle.
fn straight(x: f64) -> Vec<(f64, f64)> {
    vec![(x, -200.0), (x, 2400.0)]
}

/// A line along `x` between stations `id` (north) and `id + 1` (south), owned by `owner`.
fn line_at(id: u64, x: f64, owner: Uuid, service: Vec<Service>) -> RailLine {
    let mut l = RailLine::steady(
        end(id, x, -200.0),
        end(id + 1, x, 2400.0),
        straight(x),
        owner,
        0.0,
    );
    l.service = service;
    l
}

fn steady(per_day: f64) -> Vec<Service> {
    vec![Service {
        from: Tick::ZERO,
        per_day,
        path: 0,
    }]
}

/// What a run needs: lines, dens with the tick each is found, and the days to run.
struct Spec {
    lines: Vec<RailLine>,
    dens: Vec<(u64, Pos, Tick)>,
    days: u64,
}

fn den_at(i: u64, x: i32, z: i32, found: Tick) -> (u64, Pos, Tick) {
    (synth::CAMP + i, Pos::new(0, x, z), found)
}

/// One hour's reading of a feud.
#[derive(Clone, Copy)]
struct Pt {
    day: f64,
    step: Step,
    g: f64,
}

/// A band the engine sent, seen when first about.
#[derive(Clone, Copy)]
struct Seen {
    day: f64,
    kind: PartyKind,
    den: usize,
    target: Target,
    handle: Handle<Party>,
    /// The first hour it was no longer about: home.
    gone: Option<f64>,
}

struct Trace {
    actors: Vec<ActorId>,
    /// Hour by hour, by (den, owner), from the den's founding: the grievance is 0 and the rung
    /// None where there is no feud (a forgotten one is 0, not an empty window).
    feuds: BTreeMap<(usize, usize), Vec<Pt>>,
    seen: Vec<Seen>,
    /// The first hour each (den, owner) had a feud.
    first_feud: BTreeMap<(usize, usize), f64>,
    /// Per den: the day of its first pass and its meter's count `n` right after it.
    first_pass: Vec<Option<(f64, f64)>>,
    /// `stats.rail_pushes` at the first hour of day 20.
    pushes_20: u64,
    c: Conflict,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum At {
    Found(usize),
    Event(usize),
    Hour,
}

/// Runs the spec on the engine with the jitter of `seed`, the clock an hour at a time and every
/// pass and arrival at its own tick.
fn drive(seed: u64, spec: &Spec) -> Trace {
    let (mut w, _) = synth::world(7);
    let mut c = synth::conflict(7, &w);
    let mut owners: Vec<Uuid> = Vec::new();
    for l in &spec.lines {
        if !owners.contains(&l.owner) {
            owners.push(l.owner);
        }
    }
    let actors: Vec<ActorId> = owners.iter().map(|o| c.player(&mut w, *o)).collect();
    let dens: Vec<RailDen> = spec
        .dens
        .iter()
        .map(|&(key, p, since)| RailDen {
            key,
            x: p.x,
            z: p.z,
            radius: 200,
            since,
        })
        .collect();
    let end_t = Tick::from_days(spec.days);
    let events = synth::rail_events(&spec.lines, &dens, seed, JITTER, end_t);
    let mut timeline: Vec<(Tick, At)> = Vec::new();
    for (i, d) in spec.dens.iter().enumerate() {
        timeline.push((d.2, At::Found(i)));
    }
    for (i, e) in events.iter().enumerate() {
        timeline.push((e.0, At::Event(i)));
    }
    let mut t = HOUR;
    while t <= end_t.0 {
        timeline.push((Tick(t), At::Hour));
        t += HOUR;
    }
    timeline.sort();
    let mut fid: Vec<Option<u32>> = vec![None; spec.dens.len()];
    let mut feuds: BTreeMap<(usize, usize), Vec<Pt>> = BTreeMap::new();
    let mut known: Vec<Handle<Party>> = Vec::new();
    let mut seen = Vec::new();
    let mut first_feud = BTreeMap::new();
    let mut first_pass: Vec<Option<(f64, f64)>> = vec![None; spec.dens.len()];
    let mut pushes_20 = None;
    let infra = &mut NoInfrastructure;
    for (t, what) in timeline {
        c.run_until(&mut w, infra, t);
        let _ = c.take_commands();
        match what {
            At::Found(i) => {
                let (key, pos, _) = spec.dens[i];
                let f = c
                    .add_faction(&mut w, key, "frostmaw", pos, t)
                    .expect("the Frostmaw");
                c.factions[f as usize].chief_seed = MIDDLING;
                fid[i] = Some(f);
            }
            At::Event(i) => match events[i].1 {
                RailEvent::Pass(p) => {
                    c.ingest(&mut w, infra, &Obs::RailPass(p), t);
                    // The count a den's meter stands at right after the den's first pass.
                    let den = spec.dens.iter().position(|d| d.0 == p.faction);
                    let owner = owners.iter().position(|o| *o == p.owner);
                    if let (Some(i), Some(j)) = (den, owner)
                        && first_pass[i].is_none()
                        && let Some(f) = fid[i]
                        && let Some(m) = c.factions.row(f).rail_meter(actors[j])
                    {
                        first_pass[i] = Some((t.0 as f64 / DAY, m.n));
                    }
                }
                RailEvent::Arrive(call) => c.rail_call(&mut w, &call, t),
            },
            At::Hour => {
                let day = t.0 as f64 / DAY;
                for (i, f) in fid.iter().enumerate() {
                    let Some(f) = f else { continue };
                    for (j, a) in actors.iter().enumerate() {
                        let v = c.feud_view(spec.dens[i].0, *a);
                        if v.is_some() {
                            first_feud.entry((i, j)).or_insert(day);
                        }
                        feuds.entry((i, j)).or_default().push(Pt {
                            day,
                            step: v.as_ref().map_or(Step::None, |v| v.step),
                            g: v.as_ref().map_or(0.0, |v| v.grievance),
                        });
                    }
                    for (h, p) in c.parties.iter() {
                        if p.faction == *f && !known.contains(&h) {
                            known.push(h);
                            seen.push(Seen {
                                day,
                                kind: p.kind,
                                den: i,
                                target: p.target,
                                handle: h,
                                gone: None,
                            });
                        }
                    }
                }
                for s in seen.iter_mut().filter(|s: &&mut Seen| s.gone.is_none()) {
                    if c.parties.get(s.handle).is_none() {
                        s.gone = Some(day);
                    }
                }
                if pushes_20.is_none() && day >= 20.0 {
                    pushes_20 = Some(c.stats.rail_pushes);
                }
            }
        }
    }
    Trace {
        actors,
        feuds,
        seen,
        first_feud,
        first_pass,
        pushes_20: pushes_20.unwrap_or(c.stats.rail_pushes),
        c,
    }
}

/// What a feud's hours say, in days.
#[derive(Clone, Copy, Debug)]
struct Metrics {
    settles: f64,
    notice: Option<f64>,
    raid: Option<f64>,
    g140: Option<f64>,
    invasion: Option<f64>,
    peak: f64,
    peak_day: f64,
}

fn first_day(s: &[Pt], from: f64, f: impl Fn(&Pt) -> bool) -> Option<f64> {
    s.iter().find(|p| p.day >= from && f(p)).map(|p| p.day)
}

fn metrics(s: &[Pt], settle: (f64, f64)) -> Metrics {
    let window: Vec<f64> = s
        .iter()
        .filter(|p| p.day >= settle.0 && p.day <= settle.1)
        .map(|p| p.g)
        .collect();
    let (peak_day, peak) = s
        .iter()
        .fold((0.0, 0.0), |m, p| if p.g > m.1 { (p.day, p.g) } else { m });
    Metrics {
        settles: window.iter().sum::<f64>() / window.len().max(1) as f64,
        notice: first_day(s, 0.0, |p| p.step.index() >= Step::Notice.index()),
        raid: first_day(s, 0.0, |p| p.step.index() >= Step::Raid.index()),
        g140: first_day(s, 0.0, |p| p.g >= 140.0),
        invasion: first_day(s, 0.0, |p| p.step.index() >= Step::Invasion.index()),
        peak,
        peak_day,
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    (v[(v.len() - 1) / 2] + v[v.len() / 2]) / 2.0
}

/// The median of days where `None` is never: never when the middle two are.
fn median_day(v: &[Option<f64>]) -> Option<f64> {
    let mut v: Vec<f64> = v.iter().map(|x| x.unwrap_or(f64::INFINITY)).collect();
    v.sort_by(f64::total_cmp);
    let m = (v[(v.len() - 1) / 2] + v[v.len() / 2]) / 2.0;
    m.is_finite().then_some(m)
}

/// Whether the Invasion's day follows the engine's rule: the Raid's dwell restarts when a band
/// comes home (`feud.since`, MZB-51), so the Invasion comes a dwell after the *last* Raid's
/// return before it (the Raid rung's start if none has come home), or when the grievance reaches
/// the rung's threshold if that is later (MZB-341). An earlier return does not excuse it.
fn invasion_by_rule(tr: &Trace, den: usize, m: &Metrics) -> bool {
    let Some(inv) = m.invasion else {
        return true;
    };
    let dwell = {
        let x = &tr.c.factions[0];
        tr.c.cultures.get(x.culture).def.dwell_days[Step::Raid.index()]
    };
    let last_home = tr
        .seen
        .iter()
        .filter(|s| s.den == den && s.kind == PartyKind::Raid)
        .filter_map(|s| s.gone)
        .filter(|home| *home <= inv + 0.05)
        .fold(None, |m: Option<f64>, home| {
            Some(m.map_or(home, |m| m.max(home)))
        });
    let from = last_home.or(m.raid).map_or(0.0, |d| d + dwell);
    let want = from.max(m.g140.unwrap_or(0.0));
    (inv - want).abs() <= 0.25
}

/// Medians over the seeds, and the first seed's whole run.
struct Sweep {
    m: Metrics,
    /// Every seed's Invasion follows the engine's dwell rule.
    inv_rule: bool,
    pushes: u64,
    /// Every rail event popped, live and stale, a day.
    wakes_per_day: f64,
    first: Trace,
}

/// `spec` over [`SEEDS`] seeds, judged on the feud of `den` against `owner`.
fn sweep(spec: &Spec, den: usize, owner: usize, settle: (f64, f64)) -> Sweep {
    let mut all = Vec::new();
    let mut pushes = Vec::new();
    let mut wakes = Vec::new();
    let mut first = None;
    let mut inv_rule = true;
    for k in 0..SEEDS {
        let tr = drive(100 + k, spec);
        let s = tr.feuds.get(&(den, owner)).map_or(&[][..], |v| &v[..]);
        let m = metrics(s, settle);
        inv_rule &= invasion_by_rule(&tr, den, &m);
        all.push(m);
        pushes.push(tr.pushes_20 as f64);
        wakes.push((tr.c.stats.rail_wakes + tr.c.stats.rail_stale) as f64 / spec.days as f64);
        if first.is_none() {
            first = Some(tr);
        }
    }
    let day = |f: fn(&Metrics) -> Option<f64>| median_day(&all.iter().map(f).collect::<Vec<_>>());
    let num = |f: fn(&Metrics) -> f64| median(all.iter().map(f).collect());
    Sweep {
        m: Metrics {
            settles: num(|m| m.settles),
            notice: day(|m| m.notice),
            raid: day(|m| m.raid),
            g140: day(|m| m.g140),
            invasion: day(|m| m.invasion),
            peak: num(|m| m.peak),
            peak_day: num(|m| m.peak_day),
        },
        inv_rule,
        pushes: median(pushes) as u64,
        wakes_per_day: median(wakes),
        first: first.expect("a seed"),
    }
}

fn day_text(d: Option<f64>) -> String {
    d.map_or("never".into(), |d| format!("{d:.1}"))
}

/// Whether `got` is within `tol` days of `want` (both never counts).
fn day_ok(got: Option<f64>, want: Option<f64>, tol: f64) -> bool {
    match (got, want) {
        (None, None) => true,
        (Some(g), Some(w)) => (g - w).abs() <= tol,
        _ => false,
    }
}

fn settle_ok(got: f64, want: f64) -> bool {
    (got - want).abs() <= want * SETTLE_TOL + 1e-9
}

/// One row's check: every column of the measured medians against 4.3's.
fn judge(out: &mut Vec<Check>, name: &'static str, s: &Sweep, want: &Row, pushes_scale: u64) {
    let m = &s.m;
    let cols = [
        (
            "settles",
            settle_ok(m.settles, want.settles),
            format!("{:.1}", m.settles),
            format!("{:.1}", want.settles),
        ),
        (
            "Notice",
            day_ok(m.notice, want.notice, DAYS_TOL),
            day_text(m.notice),
            day_text(want.notice),
        ),
        (
            "Raid",
            day_ok(m.raid, want.raid, DAYS_TOL),
            day_text(m.raid),
            day_text(want.raid),
        ),
        (
            "140",
            day_ok(m.g140, want.g140, DAYS_TOL),
            day_text(m.g140),
            day_text(want.g140),
        ),
        (
            "Invasion",
            // The engine's dwell rule (MZB-341): never where the design says never, else by rule.
            match want.invasion {
                None => m.invasion.is_none(),
                Some(_) => m.invasion.is_some() && s.inv_rule,
            },
            day_text(m.invasion),
            format!("by the dwell rule, design {}", day_text(want.invasion)),
        ),
        (
            "pushes",
            s.pushes.abs_diff(want.pushes * pushes_scale) <= PUSHES_TOL * pushes_scale,
            s.pushes.to_string(),
            (want.pushes * pushes_scale).to_string(),
        ),
    ];
    let ok = cols.iter().all(|c| c.1);
    let detail = cols
        .iter()
        .map(|(n, ok, got, want)| {
            if *ok {
                format!("{n} {got}")
            } else {
                format!("{n} {got} (WANT {want})")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    check(
        out,
        name,
        ok,
        format!(
            "{}: {detail}; wakes {:.2} a day; first seed state {:016x}",
            want.legs,
            s.wakes_per_day,
            s.first.c.state_hash()
        ),
    );
}

/// The first seed's Raids all aim at the track and its Invasions at `station`'s district.
fn aims(out: &mut Vec<Check>, name: &'static str, tr: &Trace, den: usize, station: u64) {
    let raids: Vec<&Seen> = tr
        .seen
        .iter()
        .filter(|s| s.den == den && s.kind == PartyKind::Raid)
        .collect();
    let invasions: Vec<&Seen> = tr
        .seen
        .iter()
        .filter(|s| s.den == den && s.kind == PartyKind::Invasion)
        .collect();
    let raids_ok = raids
        .iter()
        .all(|s| matches!(s.target, Target::Track { .. }));
    let inv_ok = invasions
        .iter()
        .all(|s| matches!(s.target, Target::District(d, _) if d == DistrictId(station + 1000)));
    check(
        out,
        name,
        raids_ok && inv_ok,
        format!(
            "{} raids on the track, {} invasions on station {station}'s district {}",
            raids.len(),
            invasions.len(),
            station + 1000
        ),
    );
}

fn trunk_lines(owner: Uuid, n: u64, each: f64, spread: f64) -> Vec<RailLine> {
    (0..n)
        .map(|i| line_at(10 + 2 * i, TRUNK_X + spread * i as f64, owner, steady(each)))
        .collect()
}

fn single(per_day: f64) -> Spec {
    Spec {
        lines: vec![line_at(1, TRUNK_X, synth::PLAYER, steady(per_day))],
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: 70,
    }
}

/// Run 1's M7 part: the meter on one line at 4, 12, 24 and 50 legs a day, and the trunk.
pub fn meter(out: &mut Vec<Check>) {
    let mut pops = Vec::new();
    for (per_day, row) in [(4.0, &R4), (12.0, &R12), (24.0, &R24), (50.0, &R50)] {
        let s = sweep(&single(per_day), 0, 0, (50.0, 70.0));
        judge(out, row.legs, &s, row, 1);
        aims(
            out,
            "the raids aim at the track and the invasion at the line's station",
            &s.first,
            0,
            1,
        );
        check(
            out,
            WAKES_CHECK,
            s.wakes_per_day <= WAKES_MAX,
            format!(
                "{} at {per_day} legs a day: {:.2} pops a day",
                row.legs, s.wakes_per_day
            ),
        );
        pops.push(s.wakes_per_day);
    }
    // The pops do not grow with the rate: the busiest line's figure against the next busiest's.
    let (p24, p50) = (pops[2], pops[3]);
    check(
        out,
        "a meter's pops do not grow with the rate",
        p50 <= WAKES_GROWTH * p24,
        format!(
            "50 legs a day {p50:.2} pops a day, 24 legs a day {p24:.2}: x{:.2}",
            p50 / p24
        ),
    );
    let trunk = Spec {
        lines: trunk_lines(synth::PLAYER, 14, 1.7, 0.0),
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: 70,
    };
    let s = sweep(&trunk, 0, 0, (50.0, 70.0));
    judge(out, TRUNK.legs, &s, &TRUNK, 1);
    check(
        out,
        WAKES_CHECK,
        s.wakes_per_day <= WAKES_MAX,
        format!("{}: {:.2} pops a day", TRUNK.legs, s.wakes_per_day),
    );
}

/// The most rail events (every `Ev::Rail` pop, live and stale) a meter takes a day, at every rate
/// and on the trunk (MZB-385). A meter in steady flow wakes on its band's lower edge,
/// `tau ln(1/0.9)` = 0.21 days later, so about 4.8 a day whatever the rate (measured 2.09 to
/// 5.79); the design's "under 3 a day" counted the live wakes before the booked wake existed.
const WAKES_MAX: f64 = 6.5;
/// The busiest line's pops against the next busiest's: no growth with the rate (MZB-385).
const WAKES_GROWTH: f64 = 1.1;
const WAKES_CHECK: &str = "a meter takes at most 6.5 rail pops a day, stale pops counted";

/// The first day at or after `from` the feud's grievance is under `g`.
fn under(s: &[Pt], from: f64, g: f64) -> Option<f64> {
    first_day(s, from, |p| p.g < g)
}

/// Runs 2 and 3 run long enough for the grievance to come down from its peak of about 122 on
/// day 13 to its settled level, which is read over the last stretch.
const CHANGED_DAYS: u64 = 90;
/// The level a cut to 4 legs a day settles at is read over a long stretch (a seed's own reading
/// wanders by about 1.5); what is stopped has come to rest by the last fortnight.
const CHANGED_SETTLE: (f64, f64) = (60.0, 90.0);
const STOPPED_SETTLE: (f64, f64) = (75.0, 90.0);

/// A line at 12 legs a day, 24 from day 2, then `after` from day 12 on path `path`.
fn changed(after: f64, path: usize, paths: Vec<Vec<(f64, f64)>>) -> Spec {
    let mut l = line_at(1, TRUNK_X, synth::PLAYER, steady(12.0));
    l.paths = paths;
    l.service.push(Service {
        from: Tick::from_days(2),
        per_day: 24.0,
        path: 0,
    });
    l.service.push(Service {
        from: Tick::from_days(12),
        per_day: after,
        path,
    });
    Spec {
        lines: vec![l],
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: CHANGED_DAYS,
    }
}

/// Run 2: 12 legs a day, 24 from day 2; from day 12 cut to 4, or stopped.
pub fn cut(out: &mut Vec<Check>) {
    for (after, name, under70, under42, settles) in [
        (4.0, "cut to 4 legs a day", 21.4, 28.1, 17.3),
        (0.0, "stopped", 19.2, 23.6, 0.0),
    ] {
        let spec = changed(after, 0, vec![straight(TRUNK_X)]);
        let mut u70 = Vec::new();
        let mut u42 = Vec::new();
        let mut set = Vec::new();
        let mut gone = Vec::new();
        let mut peaks = Vec::new();
        let mut peak_days = Vec::new();
        let mut repeat = Vec::new();
        let mut late_raids = 0;
        let mut hash = 0;
        for k in 0..SEEDS {
            let tr = drive(100 + k, &spec);
            if k == 0 {
                hash = tr.c.state_hash();
            }
            let s = &tr.feuds[&(0, 0)];
            let m = metrics(
                s,
                if after > 0.0 {
                    CHANGED_SETTLE
                } else {
                    STOPPED_SETTLE
                },
            );
            peaks.push(m.peak);
            peak_days.push(m.peak_day);
            u70.push(under(s, 12.0, 70.0));
            u42.push(under(s, 12.0, 42.0));
            set.push(m.settles);
            gone.push(first_day(s, 13.0, |p| p.step == Step::None));
            repeat.push(
                tr.seen
                    .iter()
                    .find(|x| x.kind == PartyKind::Raid && x.day > 12.0)
                    .map(|x| x.day),
            );
            // No raid sets out once the grievance is under 70.
            if let Some(d) = under(s, 12.0, 70.0) {
                late_raids += tr
                    .seen
                    .iter()
                    .filter(|x| x.kind == PartyKind::Raid && x.day > d + 0.05)
                    .count();
            }
        }
        let (u70, u42) = (median_day(&u70), median_day(&u42));
        let settled = median(set);
        let settle_ok = if settles == 0.0 {
            // Still decaying to the floor on day 75 to 90, from a peak of 122.
            settled < 0.5
        } else {
            settle_ok(settled, settles)
        };
        check(
            out,
            if after > 0.0 {
                "cut service: under 70, under 42, settles"
            } else {
                "stopped service: under 70, under 42, settles"
            },
            day_ok(u70, Some(under70), DAYS_TOL)
                && day_ok(u42, Some(under42), DAYS_TOL)
                && settle_ok,
            format!(
                "{name}: under 70 on day {} (want {under70}), under 42 on day {} (want {under42}), \
                 settles at {settled:.1} (want {settles}); peak {:.1} on day {:.1} (about 122 on 13); \
                 first seed state {hash:016x}",
                day_text(u70),
                day_text(u42),
                median(peaks.clone()),
                median(peak_days.clone())
            ),
        );
        check(
            out,
            "the repeat raid due on day 15.1 still comes",
            day_ok(median_day(&repeat), Some(15.1), DAYS_TOL),
            format!("{name}: on day {}", day_text(median_day(&repeat))),
        );
        check(
            out,
            "no raid sets out once the grievance is under 70",
            late_raids == 0,
            format!("{name}: {late_raids} in {SEEDS} runs"),
        );
        if after == 0.0 {
            let g = median_day(&gone);
            check(
                out,
                "stopped: the Notice rung is left on day 40.5",
                day_ok(g, Some(40.5), DAYS_TOL),
                format!("left on day {}", day_text(g)),
            );
        }
        check(
            out,
            "the peak is about 122 on day 13",
            (median(peaks) - 122.0).abs() <= 122.0 * SETTLE_TOL
                && (median(peak_days) - 13.0).abs() <= DAYS_TOL + 0.4,
            format!("{name}: see above"),
        );
    }
}

/// Run 3: the track moves on day 12, off the circle or within it.
pub fn reroute(out: &mut Vec<Check>) {
    // (a) Off the circle: the passes stop and the grievance follows the stopped row.
    let off = changed(24.0, 1, vec![straight(TRUNK_X), straight(1400.0)]);
    let mut u70 = Vec::new();
    let mut u42 = Vec::new();
    let mut set = Vec::new();
    let mut hash = 0;
    for k in 0..SEEDS {
        let tr = drive(100 + k, &off);
        if k == 0 {
            hash = tr.c.state_hash();
        }
        let s = &tr.feuds[&(0, 0)];
        u70.push(under(s, 12.0, 70.0));
        u42.push(under(s, 12.0, 42.0));
        set.push(metrics(s, STOPPED_SETTLE).settles);
    }
    let (u70, u42, settled) = (median_day(&u70), median_day(&u42), median(set));
    check(
        out,
        "the track moved off the circle: the grievance follows the stopped row",
        day_ok(u70, Some(19.2), DAYS_TOL) && day_ok(u42, Some(23.6), DAYS_TOL) && settled < 0.5,
        format!(
            "under 70 on day {} (want 19.2), under 42 on day {} (want 23.6), settles at {settled:.1}; \
             first seed state {hash:016x}",
            day_text(u70),
            day_text(u42)
        ),
    );
    // (b) Within it: the line's point follows the new passes, and the next Raid's target is
    // there.
    let within = changed(24.0, 1, vec![straight(TRUNK_X), straight(1000.0)]);
    let tr = drive(100, &within);
    let tracks: Vec<(f64, i32)> = tr
        .seen
        .iter()
        .filter(|s| s.kind == PartyKind::Raid)
        .filter_map(|s| match s.target {
            Target::Track { track, .. } => Some((s.day, track.x)),
            _ => None,
        })
        .collect();
    let before = tracks.iter().filter(|t| t.0 < 12.0).all(|t| t.1 == 1050);
    let after = tracks
        .iter()
        .filter(|t| t.0 > 12.0)
        .map(|t| t.1)
        .collect::<Vec<_>>();
    let f = tr.c.faction_of_key(synth::CAMP).expect("the den");
    let meter =
        tr.c.factions
            .row(f)
            .rail_meter(tr.actors[0])
            .expect("a meter");
    let line_x = meter.lines.first().map(|l| l.point.x);
    check(
        out,
        "the track moved within the circle: the next Raid aims at the new point",
        before && !after.is_empty() && after.iter().all(|x| *x == 1000) && line_x == Some(1000),
        format!(
            "raids on the track at x {tracks:?} (days, x); the line's point at x {line_x:?} (was 1050); \
             first seed state {:016x}",
            tr.c.state_hash()
        ),
    );
}

/// Run 4: two dens on one line, and a third found on day 10.
pub fn two_dens(out: &mut Vec<Check>) {
    let spec = Spec {
        lines: vec![line_at(1, TRUNK_X, synth::PLAYER, steady(12.0))],
        dens: vec![
            den_at(0, 900, 1000, Tick::ZERO),
            den_at(1, 1200, 1000, Tick::ZERO),
        ],
        days: 70,
    };
    let a = sweep(&spec, 0, 0, (50.0, 70.0));
    let b = sweep(&spec, 1, 0, (50.0, 70.0));
    judge(
        out,
        "den A (west of the line) follows the 12-a-day row",
        &a,
        &R12,
        2,
    );
    judge(
        out,
        "den B (east of the line) follows the 12-a-day row",
        &b,
        &R12,
        2,
    );
    // A third den, found on day 10, gets its first pass from the next train.
    let third = Spec {
        lines: spec.lines.clone(),
        dens: vec![
            den_at(0, 900, 1000, Tick::ZERO),
            den_at(1, 1200, 1000, Tick::ZERO),
            den_at(2, 1200, 1300, Tick::from_days(10)),
        ],
        days: 40,
    };
    let tr = drive(100, &third);
    let c = &tr.c;
    let key = synth::CAMP + 2;
    let f = c.faction_of_key(key).expect("the third den");
    let meter = c.factions.row(f).rail_meter(tr.actors[0]);
    let first = tr.first_feud.get(&(2, 0)).copied();
    // Its meter starts at nothing: right after the first pass it counts that one leg, whatever the
    // other dens' meters had run up before the den was found.
    let n1 = tr.first_pass[2];
    check(
        out,
        "a den found on day 10 gets its first pass from the next train and its own meter",
        meter.is_some_and(|m| !m.lines.is_empty())
            && first.is_some_and(|d| d > 10.0 && d < 10.5)
            && n1.is_some_and(|(d, n)| d > 10.0 && d < 10.5 && (n - 1.0).abs() < 1e-9),
        format!(
            "its feud first seen on day {}; its meter counts n = {} right after its first pass \
             (day {}); a meter with {} line(s); {} of its passes unknown (the other dens')",
            day_text(first),
            n1.map_or("none".into(), |x| format!("{:.6}", x.1)),
            day_text(n1.map(|x| x.0)),
            meter.map_or(0, |m| m.lines.len()),
            c.stats.rail_unknown
        ),
    );
}

/// Run 9: the trunk.
pub fn trunk(out: &mut Vec<Check>) {
    // The 14 lines at 1.7 against the one line of 23.8 (run 1's trunk row).
    let one = sweep(&single(23.8), 0, 0, (50.0, 70.0));
    let many = Spec {
        lines: trunk_lines(synth::PLAYER, 14, 1.7, 0.0),
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: 70,
    };
    let m = sweep(&many, 0, 0, (50.0, 70.0));
    let same = settle_ok(m.m.settles, one.m.settles)
        && day_ok(m.m.notice, one.m.notice, DAYS_TOL)
        && day_ok(m.m.raid, one.m.raid, DAYS_TOL)
        && day_ok(m.m.g140, one.m.g140, DAYS_TOL)
        && day_ok(m.m.invasion, one.m.invasion, DAYS_TOL)
        && m.pushes.abs_diff(one.pushes) <= PUSHES_TOL;
    check(
        out,
        "14 lines at 1.7 match one line of 23.8 within the tolerances",
        same,
        format!(
            "14 lines: settles {:.1}, Notice {}, Raid {}, 140 {}, Invasion {}, pushes {}; one line: \
             settles {:.1}, Notice {}, Raid {}, 140 {}, Invasion {}, pushes {}",
            m.m.settles,
            day_text(m.m.notice),
            day_text(m.m.raid),
            day_text(m.m.g140),
            day_text(m.m.invasion),
            m.pushes,
            one.m.settles,
            day_text(one.m.notice),
            day_text(one.m.raid),
            day_text(one.m.g140),
            day_text(one.m.invasion),
            one.pushes
        ),
    );

    // One line at 6 and the others at 1.2: the focus is the busy line's point.
    let mut lines = trunk_lines(synth::PLAYER, 14, 1.2, 3.0);
    lines[0] = line_at(10, 1010.0, synth::PLAYER, steady(6.0));
    let busy_x = 1010;
    let spec = Spec {
        lines,
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: 40,
    };
    let tr = drive(100, &spec);
    let f = tr.c.faction_of_key(synth::CAMP).expect("the den");
    let src =
        tr.c.sources
            .get(&(rail_key(synth::CAMP, tr.actors[0]), SignalKind::Rail))
            .cloned();
    let raids: Vec<i32> = tr
        .seen
        .iter()
        .filter(|s| s.kind == PartyKind::Raid)
        .filter_map(|s| match s.target {
            Target::Track { track, .. } => Some(track.x),
            _ => None,
        })
        .collect();
    let meter = tr.c.factions.row(f).rail_meter(tr.actors[0]);
    let busiest = meter.and_then(|m| {
        m.lines
            .iter()
            .max_by(|a, b| a.n_at(tr.c.now()).total_cmp(&b.n_at(tr.c.now())))
            .map(|l| l.point.x)
    });
    check(
        out,
        "one line at 6 and thirteen at 1.2: the focus is the busy line's point",
        src.as_ref().is_some_and(|s| s.pos.x == busy_x)
            && busiest == Some(busy_x)
            && !raids.is_empty()
            && raids.iter().all(|x| *x == busy_x),
        format!(
            "the busy line's point at x {busy_x}: the source at {:?}, the busiest line's at {busiest:?}, \
             the raids' tracks at {raids:?}",
            src.as_ref().map(|s| s.pos.x)
        ),
    );

    // Two owners' trains on one line: two meters and two feuds, each at its own share.
    let spec = Spec {
        lines: vec![
            line_at(1, TRUNK_X, synth::PLAYER, steady(12.0)),
            line_at(1, TRUNK_X, SECOND, steady(4.0)),
        ],
        dens: vec![den_at(0, 900, 1000, Tick::ZERO)],
        days: 70,
    };
    let first = sweep(&spec, 0, 0, (50.0, 70.0));
    let second = sweep(&spec, 0, 1, (50.0, 70.0));
    let f = first.first.c.faction_of_key(synth::CAMP).expect("the den");
    let meters = first.first.c.factions.row(f).rail().len();
    let feuds = first.first.c.feuds_of(f).len();
    let own = |s: &Sweep, want: &Row| {
        settle_ok(s.m.settles, want.settles) && day_ok(s.m.notice, want.notice, DAYS_TOL)
    };
    check(
        out,
        "two owners on one line make two meters and two feuds, each at its own share",
        meters == 2 && feuds == 2 && own(&first, &R12) && own(&second, &R4),
        format!(
            "{meters} meters, {feuds} feuds; owner 1 (12 a day) settles {:.1} (want 86.6), Notice {}; \
             owner 2 (4 a day) settles {:.1} (want 17.3), Notice {}",
            first.m.settles,
            day_text(first.m.notice),
            second.m.settles,
            day_text(second.m.notice)
        ),
    );
}
