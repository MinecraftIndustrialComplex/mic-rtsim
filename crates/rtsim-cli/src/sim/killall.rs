// SPDX-License-Identifier: AGPL-3.0-or-later
//! The kill-alls in one place (EQUILIBRIUM.md §4's S11 row; docs/overnight-choices-equilibrium.md
//! EQC425 to EQC430): a row per kind of the census, each naming its alone harness (a function of
//! the seed), the band its alone test asserts and the record the band came from. Every harness's
//! result is read into one shape, [`KindRefill`]; `rtsim-cli sim killall` prints a fixed-width line
//! a row and exits 1 on any FAIL, and the slices' `_30_seeds` tests are one-line calls of their
//! row ([`assert_row`]), so each check lives once.
//!
//! The table adds no number: a row's checks are its alone test's, word for word.

use std::process::ExitCode;

use super::census::Ecology;
use super::equilibrium::{self as eq, Grid, Wipe, YEAR, band, par_map, threads};

/// A kill-all on one seed, read into the table's shape (EQC425). Days are `None` for never in the
/// run; they are fractional where the harness reads them so (a haven's slots, a den's chief).
#[derive(Clone, Debug, PartialEq)]
pub struct KindRefill {
    /// The row's name and its census kind.
    pub row: &'static str,
    pub kind: &'static str,
    pub seed: u64,
    /// Members the kill-all killed, and the level it is read against (the year or the day before,
    /// as the harness reads it).
    pub killed: u64,
    pub pre: f64,
    /// Days from the kill to half the level before, to 90% of the same seed's control, and to
    /// every site whole again.
    pub half: Option<f64>,
    pub ninety_ctrl: Option<f64>,
    pub whole: Option<f64>,
    /// The end over the control (1.0 where the harness has no control).
    pub end_over_ctrl: f64,
    /// Breaks (a camp's), shuts (a haven's): what the band says must not happen.
    pub breaks: u64,
    /// The row's checks on this seed alone (its alone test's per-seed asserts).
    pub checks: Result<(), String>,
    /// What else the row counts across seeds (herds: ranges refounded by neighbours' dispersers).
    pub aux: u64,
}

impl KindRefill {
    fn new(row: &Row, seed: u64) -> KindRefill {
        KindRefill {
            row: row.name,
            kind: row.kind,
            seed,
            killed: 0,
            pre: 0.0,
            half: None,
            ninety_ctrl: None,
            whole: None,
            end_over_ctrl: 1.0,
            breaks: 0,
            checks: Ok(()),
            aux: 0,
        }
    }
}

/// One row of the table (EQC426).
#[derive(Clone, Copy)]
pub struct Row {
    /// The row's name (`--kind`), its census kind and the test that runs it on seeds 1 to 30.
    pub name: &'static str,
    pub kind: &'static str,
    pub test: &'static str,
    /// The band as the alone test asserts it, and the record it came from.
    pub band: &'static str,
    pub record: &'static str,
    /// The alone harness on one seed.
    pub run: fn(&Row, u64) -> KindRefill,
    /// The checks across seeds (medians, sums); the per-seed ones are in [`KindRefill::checks`].
    pub judge: fn(&[KindRefill]) -> Result<(), String>,
}

/// Census kinds with no kill-all, and why (EQC428).
pub const NO_ROW: [(&str, &str); 1] = [("ship.port", "not a population (§2.4.6)")];

/// `Ok` if `ok`, else `Err(why)`.
fn need(ok: bool, why: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(why()) }
}

fn days(x: Option<u64>) -> Option<f64> {
    x.map(|d| d as f64)
}

/// The median over seeds of `f`, never counted as infinity.
fn median(rs: &[KindRefill], f: impl Fn(&KindRefill) -> Option<f64>) -> f64 {
    band(
        &rs.iter()
            .map(|r| f(r).unwrap_or(f64::INFINITY))
            .collect::<Vec<_>>(),
    )
    .0
}

fn no_judge(_: &[KindRefill]) -> Result<(), String> {
    Ok(())
}

// ---- the rows' harnesses ------------------------------------------------------------------------

fn herds(row: &Row, seed: u64, which: Wipe) -> KindRefill {
    let r = eq::wipe_run(
        &Grid::world(),
        seed,
        Ecology::herd_tunables(),
        which,
        10 * YEAR,
        40 * YEAR,
    );
    KindRefill {
        killed: r.killed,
        pre: r.pre,
        half: days(r.half),
        ninety_ctrl: days(r.ninety_ctrl),
        whole: days(r.all_alive),
        end_over_ctrl: r.end_over_ctrl,
        aux: r.recolonised,
        checks: need(r.half.is_some_and(|d| d <= 25 * YEAR), || {
            format!("not half back within 25 years: {r:?}")
        })
        .and(need(r.alive_end && r.all_alive.is_some(), || {
            format!("a range empty: {r:?}")
        })),
        ..KindRefill::new(row, seed)
    }
}

fn herds_world(row: &Row, seed: u64) -> KindRefill {
    herds(row, seed, Wipe::World)
}

fn herds_local(row: &Row, seed: u64) -> KindRefill {
    herds(row, seed, Wipe::Local)
}

fn herds_local_judge(rs: &[KindRefill]) -> Result<(), String> {
    need(rs.iter().map(|r| r.aux).sum::<u64>() > 0, || {
        "no range refilled from a neighbour".into()
    })
}

fn dragons(row: &Row, seed: u64) -> KindRefill {
    let r = eq::dragon_kill_all(&Grid::world(), seed, 20 * YEAR, 6 * YEAR);
    let cap = 192 + eq::EVERY;
    KindRefill {
        killed: r.killed,
        pre: r.pre,
        half: days(r.half),
        ninety_ctrl: days(r.ninety_ctrl),
        end_over_ctrl: r.end_over_ctrl,
        checks: need(r.killed > 0, || format!("nothing killed: {r:?}")).and(need(
            r.ninety_ctrl.is_some_and(|d| d <= cap),
            || format!("not 90% of the control within {cap} days: {r:?}"),
        )),
        ..KindRefill::new(row, seed)
    }
}

fn dragons_judge(rs: &[KindRefill]) -> Result<(), String> {
    let ninety = median(rs, |r| r.ninety_ctrl);
    let end = median(rs, |r| Some(r.end_over_ctrl));
    need(ninety <= 72.0, || {
        format!("median days to 90% of the control {ninety}")
    })
    .and(need((0.9..=1.1).contains(&end), || {
        format!("median end over the control {end}")
    }))
}

/// The nests' cells (chunks), as S10's `kill_all_nests` places them.
pub const NEST_CELLS: [(i32, i32); 4] = [(100, 100), (120, 100), (100, 120), (140, 140)];

fn nests(row: &Row, seed: u64) -> KindRefill {
    let r = rtsim_ecology::synth::nests::kill_all(seed, &NEST_CELLS);
    let k = r.k;
    let checks = need(r.nests == NEST_CELLS.len(), || format!("{} nests", r.nests))
        .and(need(r.hatched && r.cleared, || "not hatched or not cleared".into()))
        .and(need(r.killed == r.nests as u32 * k, || {
            format!("killed {}", r.killed)
        }))
        .and(need(r.whole.is_some_and(|d| d <= 72), || {
            format!("drilled, back after {:?} days", r.whole)
        }))
        .and(need(r.half.is_some_and(|h| r.whole.is_some_and(|w| h <= w)), || {
            format!("half {:?} whole {:?}", r.half, r.whole)
        }))
        .and(need(r.one_at_a_time, || "a renewal not from a wait".into()))
        .and(need(
            r.lost_player.iter().zip(&r.renewed).all(|(l, n)| *l == k && *n == k),
            || format!("lost {:?} renewed {:?}", r.lost_player, r.renewed),
        ))
        .and(need(r.undrilled_renewals == 0 && r.undrilled_worms == 0, || {
            format!(
                "undrilled: {} renewals, {} worms",
                r.undrilled_renewals, r.undrilled_worms
            )
        }))
        .and(need(r.redrilled.is_some_and(|d| d <= 150), || {
            format!("drilled again: {:?}", r.redrilled)
        }))
        .and(need(r.plugin_agrees, || "the plugin's counts differ".into()));
    KindRefill {
        killed: u64::from(r.killed),
        pre: f64::from(r.nests as u32 * k),
        half: days(r.half),
        whole: days(r.whole),
        checks,
        ..KindRefill::new(row, seed)
    }
}

fn villages(row: &Row, seed: u64, culture: rtsim_settle::Culture, after: u64) -> KindRefill {
    let r = eq::village_wipe(seed, culture, 100, after, |_| {});
    KindRefill {
        killed: u64::from(r.pre),
        pre: f64::from(r.pre),
        ninety_ctrl: days(r.ninety_ctrl),
        end_over_ctrl: r.end_over_ctrl,
        aux: r.settlers,
        checks: need(r.paired == r.wiped_villages && r.wiped_villages > 0, || {
            format!("a wiped village never settled: {r:?}")
        })
        .and(need(
            culture != rtsim_settle::Culture::Human || (r.pair.0 >= 32.0 && r.pair.1 <= 33.0),
            || format!("settlers not on day 32 to 33: {r:?}"),
        )),
        ..KindRefill::new(row, seed)
    }
}

fn villagers(row: &Row, seed: u64) -> KindRefill {
    villages(row, seed, rtsim_settle::Culture::Human, 900)
}

fn villagers_judge(rs: &[KindRefill]) -> Result<(), String> {
    let b = median(rs, |r| r.ninety_ctrl);
    need(b <= 600.0, || format!("median days to 90% of the control {b}"))
}

fn ribbits(row: &Row, seed: u64) -> KindRefill {
    villages(row, seed, rtsim_settle::Culture::Ribbit, 700)
}

fn ribbits_judge(rs: &[KindRefill]) -> Result<(), String> {
    let b = median(rs, |r| r.ninety_ctrl);
    need(b <= 300.0, || format!("median days to 90% of the control {b}"))
}

fn camps_of(row: &Row, seed: u64, names: &[&str]) -> KindRefill {
    let r = eq::camp_wipe_of(seed, names, 100, 240);
    KindRefill {
        killed: r.killed,
        half: days(r.half_ctrl),
        ninety_ctrl: days(r.ninety_ctrl),
        end_over_ctrl: r.end_over_ctrl,
        breaks: r.breaks,
        checks: need(r.breaks == 0, || format!("a camp broke: {r:?}")).and(need(
            r.ninety_ctrl.is_some_and(|d| d <= 120),
            || format!("not 90% of the control within 120 days: {r:?}"),
        )),
        ..KindRefill::new(row, seed)
    }
}

fn camps(row: &Row, seed: u64) -> KindRefill {
    camps_of(row, seed, &eq::POOL_CAMPS)
}

fn outposts(row: &Row, seed: u64) -> KindRefill {
    camps_of(row, seed, &["raider"])
}

fn camps_judge(rs: &[KindRefill]) -> Result<(), String> {
    let b = median(rs, |r| r.ninety_ctrl);
    need(b <= 96.0, || format!("median days to 90% of the control {b}"))
}

fn pirates(row: &Row, seed: u64) -> KindRefill {
    let r = rtsim_ships::synth::equilibrium::kill_all_pirates(seed);
    let longest = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    let checks = need(r.sunk > 0 && r.active_after == 0, || {
        format!("sunk {} of {}, {} still active", r.sunk, r.slots, r.active_after)
    })
    .and(need(r.whole.is_some_and(|d| d <= 180.0), || {
        format!("not every slot back within 180 days: {:?}", r.whole)
    }))
    .and(need(r.shuts == 0, || {
        format!("{} shuts: a kill-all is answered by refits (PC4)", r.shuts)
    }))
    .and(need(
        longest(&r.again_waits) > longest(&r.first_waits),
        || {
            format!(
                "a second kill-all waits {:?}, the first {:?}",
                r.again_waits, r.first_waits
            )
        },
    ))
    .and(need(
        r.again_waits.iter().all(|&w| w <= r.cap_days + 1e-6),
        || format!("a wait past the cap: {:?}", r.again_waits),
    ))
    .and(r.indexes.clone());
    KindRefill {
        killed: r.sunk as u64,
        pre: r.slots as f64,
        half: r.half,
        whole: r.whole,
        breaks: r.shuts,
        checks,
        ..KindRefill::new(row, seed)
    }
}

/// The table (EQC426, EQC428), in the census's order.
pub const ROWS: &[Row] = &[
    Row {
        name: "herds.world",
        kind: "herd.range",
        test: "kill_all_herds_30_seeds",
        band: "each seed half back within 25 years, every range alive within 40",
        record: "EQC21; S2",
        run: herds_world,
        judge: no_judge,
    },
    Row {
        name: "herds.local",
        kind: "herd.range",
        test: "local_wipe_refills_from_neighbours_30_seeds",
        band: "as herds.world; some range refounded by a neighbour",
        record: "EQC21",
        run: herds_local,
        judge: herds_local_judge,
    },
    Row {
        name: "dragons",
        kind: "dragon.lair",
        test: "kill_all_dragons_30_seeds",
        band: "median 90% of the control within 72 days, each seed within 216; end over the \
               control median 0.9 to 1.1",
        record: "EQC32; S4",
        run: dragons,
        judge: dragons_judge,
    },
    Row {
        name: "nests",
        kind: "oil.nest",
        test: "kill_all_nests_30_seeds",
        band: "drilled back within 72 days, one worm a renewal; undrilled nothing in 400 days; \
               drilled again back within 150",
        record: "S10, OGC76",
        run: nests,
        judge: no_judge,
    },
    Row {
        name: "villagers",
        kind: "village.human",
        test: "kill_all_villagers_30_seeds",
        band: "every village's settlers on day 32 to 33; median 90% of the control within 600 days",
        record: "EQC185",
        run: villagers,
        judge: villagers_judge,
    },
    Row {
        name: "ribbits",
        kind: "village.ribbit",
        test: "kill_all_ribbits_30_seeds",
        band: "every village settled; median 90% of the control within 300 days",
        record: "EQC184",
        run: ribbits,
        judge: ribbits_judge,
    },
    Row {
        name: "camps",
        kind: "conflict.camp",
        test: "kill_all_camps_30_seeds",
        band: "median 90% of the control within 96 days, each seed within 120, no break",
        record: "EQC270",
        run: camps,
        judge: camps_judge,
    },
    Row {
        name: "outposts",
        kind: "conflict.camp",
        test: "kill_all_outposts_30_seeds",
        band: "as camps",
        record: "FB-5 (b), EQC428",
        run: outposts,
        judge: camps_judge,
    },
    Row {
        name: "pirates",
        kind: "ship.haven",
        test: "kill_all_pirates_30_seeds",
        band: "every slot back within 180 days; no haven shut",
        record: "S8 (T32 to T39), PC38",
        run: pirates,
        judge: no_judge,
    },
];

/// The row named `name`.
pub fn row(name: &str) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.name == name)
}

/// The row run on `seeds`, on [`threads`].
pub fn run_row(row: &'static Row, seeds: &[u64]) -> Vec<KindRefill> {
    par_map(seeds, threads(), |&s| (row.run)(row, s))
}

/// The row's verdict on its seeds: every seed's checks, then the judge's.
pub fn verdict(row: &Row, rs: &[KindRefill]) -> Result<(), String> {
    let bad: Vec<String> = rs
        .iter()
        .filter_map(|r| r.checks.as_ref().err().map(|e| format!("seed {}: {e}", r.seed)))
        .collect();
    need(bad.is_empty(), || bad.join("; ")).and((row.judge)(rs))
}

/// The header of [`line`].
pub const HEADER: &str = "row            kind            seeds  half: median [5th, 95th]   \
                          90% of ctrl: median [5th, 95th]  whole: median [5th, 95th]  \
                          end/ctrl  verdict";

fn spread(rs: &[KindRefill], f: impl Fn(&KindRefill) -> Option<f64>) -> String {
    if rs.iter().all(|r| f(r).is_none()) {
        return format!("{:>25}", "-");
    }
    let (m, lo, hi) = band(
        &rs.iter()
            .map(|r| f(r).unwrap_or(f64::INFINITY))
            .collect::<Vec<_>>(),
    );
    let never = rs.iter().filter(|r| f(r).is_none()).count();
    let n = |x: f64| {
        if x.is_finite() {
            format!("{x:.1}")
        } else {
            "never".into()
        }
    };
    let s = format!("{} [{}, {}]", n(m), n(lo), n(hi));
    if never > 0 {
        format!("{s:>19} ({never} never)")
    } else {
        format!("{s:>25}")
    }
}

/// One fixed-width line for a row's seeds (EQC425).
pub fn line(row: &Row, rs: &[KindRefill], v: &Result<(), String>) -> String {
    format!(
        "{:<14} {:<15} {:>5}  {}  {:>31}  {:>25}  {:>8.3}  {}",
        row.name,
        row.kind,
        rs.len(),
        spread(rs, |r| r.half),
        spread(rs, |r| r.ninety_ctrl),
        spread(rs, |r| r.whole),
        median(rs, |r| Some(r.end_over_ctrl)),
        match v {
            Ok(()) => "PASS".to_string(),
            Err(e) => format!("FAIL {e}"),
        }
    )
}

/// Prints every seed of a row (the slices' 30-seed tests print theirs so).
pub fn print_seeds(rs: &[KindRefill]) {
    for r in rs {
        println!(
            "  seed {:>2}: killed {:>6} pre {:>9.2} half {:?} 90% {:?} whole {:?} end/ctrl {:.3} \
             breaks {} aux {}{}",
            r.seed,
            r.killed,
            r.pre,
            r.half,
            r.ninety_ctrl,
            r.whole,
            r.end_over_ctrl,
            r.breaks,
            r.aux,
            r.checks
                .as_ref()
                .err()
                .map_or(String::new(), |e| format!(" FAIL {e}"))
        );
    }
}

/// Runs row `name` on `seeds`, prints it, and panics unless it passes: the body of the slices'
/// `_30_seeds` tests.
pub fn assert_row(name: &str, seeds: &[u64]) -> Vec<KindRefill> {
    let row = row(name).unwrap_or_else(|| panic!("no kill-all row {name}"));
    let rs = run_row(row, seeds);
    let v = verdict(row, &rs);
    println!("{} ({}; {})", row.test, row.band, row.record);
    println!("{HEADER}");
    println!("{}", line(row, &rs, &v));
    print_seeds(&rs);
    if let Err(e) = v {
        panic!("{}: {e}", row.name);
    }
    rs
}

fn bad(why: &str) -> ExitCode {
    eprintln!("{why}\nusage: rtsim-cli sim killall [--kind K[,K...]] [--seeds A..B] [--verbose]");
    ExitCode::from(64)
}

/// `rtsim-cli sim killall`: each row on the seeds (1 to 30 by default), one line a row; exits 1
/// on any FAIL.
pub fn cli(rest: &[&str]) -> ExitCode {
    let mut rows: Vec<&'static Row> = ROWS.iter().collect();
    let (mut from, mut to) = (1u64, 30u64);
    let mut verbose = false;
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        match a {
            "--kind" => {
                let Some(v) = it.next() else {
                    return bad("--kind needs a row");
                };
                let mut rs = Vec::new();
                for n in v.split(',') {
                    match row(n) {
                        Some(r) => rs.push(r),
                        None => {
                            let names: Vec<&str> = ROWS.iter().map(|r| r.name).collect();
                            return bad(&format!("unknown row {n} (rows: {})", names.join(", ")));
                        }
                    }
                }
                rows = rs;
            }
            "--seeds" => {
                let parsed = it.next().and_then(|v| {
                    let (a, b) = v.split_once("..")?;
                    Some((a.parse().ok()?, b.parse().ok()?))
                });
                match parsed {
                    Some((a, b)) if a <= b => (from, to) = (a, b),
                    _ => return bad("--seeds needs A..B"),
                }
            }
            "--verbose" => verbose = true,
            other => return bad(&format!("unknown option {other}")),
        }
    }
    let seeds: Vec<u64> = (from..=to).collect();
    println!(
        "# rtsim-cli sim killall: seeds {from} to {to}, {} threads; each row's alone harness",
        threads()
    );
    for (k, why) in NO_ROW {
        println!("# {k}: no row, {why}");
    }
    println!("{HEADER}");
    let mut failed = false;
    for row in rows {
        let rs = run_row(row, &seeds);
        let v = verdict(row, &rs);
        failed |= v.is_err();
        println!("{}", line(row, &rs, &v));
        if verbose {
            print_seeds(&rs);
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EQC430: every kind of the every-kind world's census has a row or a recorded "no row", and
    /// every row names a kind of that census.
    #[test]
    fn the_kill_all_table_names_every_kind() {
        let mut w = rtsim_host::every::Every::open(
            "killall-names",
            1,
            false,
            rtsim_kernel::page::Durability::NoSync,
        );
        w.run_to(1);
        let cs = w.census();
        w.close();
        let kinds: Vec<&str> = cs.iter().map(|c| c.kind).collect();
        for k in &kinds {
            assert!(
                ROWS.iter().any(|r| r.kind == *k) || NO_ROW.iter().any(|(n, _)| n == k),
                "{k} has no kill-all row and no recorded reason"
            );
        }
        for r in ROWS {
            assert!(
                kinds.contains(&r.kind),
                "row {} names {}, which the census has not",
                r.name,
                r.kind
            );
        }
        for k in rtsim_host::every::KINDS {
            assert!(kinds.contains(&k), "the census has no {k}");
        }
    }

    /// EQC430: every row on seeds 1 to 30, printed as the table; fails if any row fails.
    #[test]
    #[ignore = "hours: run in release with --ignored --nocapture"]
    fn kill_all_every_kind_30_seeds() {
        let seeds: Vec<u64> = (1..=30).collect();
        println!("{HEADER}");
        let mut failed = Vec::new();
        for row in ROWS {
            let rs = run_row(row, &seeds);
            let v = verdict(row, &rs);
            println!("{}", line(row, &rs, &v));
            if v.is_err() {
                failed.push(row.name);
            }
        }
        assert!(failed.is_empty(), "failing rows: {failed:?}");
    }

    /// S10's nests on seeds 1 to 30 (it ran one), the outposts' row (EQC428) and S8's pirates
    /// (its own test runs them across the lifecycle variants); the other rows' twins are in
    /// `sim::equilibrium`.
    #[test]
    #[ignore = "minutes: run in release with --ignored --nocapture"]
    fn kill_all_nests_30_seeds() {
        assert_row("nests", &(1..=30).collect::<Vec<_>>());
    }

    #[test]
    #[ignore = "minutes: run in release with --ignored --nocapture"]
    fn kill_all_outposts_30_seeds() {
        assert_row("outposts", &(1..=30).collect::<Vec<_>>());
    }

    #[test]
    #[ignore = "minutes: run in release with --ignored --nocapture"]
    fn kill_all_pirates_30_seeds() {
        assert_row("pirates", &(1..=30).collect::<Vec<_>>());
    }
}
