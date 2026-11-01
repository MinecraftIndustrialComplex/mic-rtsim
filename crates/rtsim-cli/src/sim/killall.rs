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

use rtsim_host::every::Together;

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
    /// The most pressure a site of the row held just after the kill, where the run reads it (the
    /// together runs; the Umvuthi's row).
    pub p_after: Option<f32>,
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
            p_after: None,
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
    /// The band's per-seed checks a together run can read (EQC429): on the shape's own fields,
    /// so they hold for the alone run and the together one alike. `None`: the row has no wipe in
    /// the every-kind world.
    pub together: Option<SeedCheck>,
    /// Whether [`Row::judge`] holds together too: the medians do; the groves' one day for one K
    /// holds only where every seed's grove starts at P 0, as alone (EQC492).
    pub judge_together: bool,
}

/// A row's checks on one seed's [`KindRefill`].
pub type SeedCheck = fn(&KindRefill) -> Result<(), String>;

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

/// S4's world as main's `kill_all_dragons_30_seeds` runs it since the food gate (EQC539): the
/// jar's lairs at the real density, the herds' K capped by forage.
fn dragons(row: &Row, seed: u64) -> KindRefill {
    let (grid, lairs) = eq::s4_world(seed);
    let tun = (Ecology::herd_tunables(), Ecology::dragon_tunables());
    let r = eq::dragon_kill_all_on(&grid, seed, &lairs, tun, 20 * YEAR, 6 * YEAR);
    KindRefill {
        killed: r.killed,
        pre: r.pre,
        half: days(r.half),
        ninety_ctrl: days(r.ninety_ctrl),
        end_over_ctrl: r.end_over_ctrl,
        checks: need(r.killed > 0, || format!("nothing killed: {r:?}")).and(need(
            r.ninety_ctrl.is_some(),
            || format!("not 90% of the control within the run: {r:?}"),
        )),
        ..KindRefill::new(row, seed)
    }
}

fn dragons_judge(rs: &[KindRefill]) -> Result<(), String> {
    let ninety = median(rs, |r| r.ninety_ctrl);
    let ninety_95 = band(
        &rs.iter()
            .map(|r| r.ninety_ctrl.unwrap_or(f64::INFINITY))
            .collect::<Vec<_>>(),
    )
    .2;
    let end = median(rs, |r| Some(r.end_over_ctrl));
    let cap = (240 + eq::EVERY) as f64;
    need(ninety <= 72.0, || {
        format!("median days to 90% of the control {ninety}")
    })
    .and(need(ninety_95 <= cap, || {
        format!("95th days to 90% of the control {ninety_95}, over {cap}")
    }))
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
        .and(need(r.hatched && r.cleared, || {
            "not hatched or not cleared".into()
        }))
        .and(need(r.killed == r.nests as u32 * k, || {
            format!("killed {}", r.killed)
        }))
        .and(need(r.whole.is_some_and(|d| d <= 72), || {
            format!("drilled, back after {:?} days", r.whole)
        }))
        .and(need(
            r.half.is_some_and(|h| r.whole.is_some_and(|w| h <= w)),
            || format!("half {:?} whole {:?}", r.half, r.whole),
        ))
        .and(need(r.one_at_a_time, || "a renewal not from a wait".into()))
        .and(need(
            r.lost_player
                .iter()
                .zip(&r.renewed)
                .all(|(l, n)| *l == k && *n == k),
            || format!("lost {:?} renewed {:?}", r.lost_player, r.renewed),
        ))
        .and(need(
            r.undrilled_renewals == 0 && r.undrilled_worms == 0,
            || {
                format!(
                    "undrilled: {} renewals, {} worms",
                    r.undrilled_renewals, r.undrilled_worms
                )
            },
        ))
        .and(need(r.redrilled.is_some_and(|d| d <= 150), || {
            format!("drilled again: {:?}", r.redrilled)
        }))
        .and(need(r.plugin_agrees, || {
            "the plugin's counts differ".into()
        }));
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
    need(b <= 600.0, || {
        format!("median days to 90% of the control {b}")
    })
}

fn ribbits(row: &Row, seed: u64) -> KindRefill {
    villages(row, seed, rtsim_settle::Culture::Ribbit, 700)
}

fn ribbits_judge(rs: &[KindRefill]) -> Result<(), String> {
    let b = median(rs, |r| r.ninety_ctrl);
    need(b <= 300.0, || {
        format!("median days to 90% of the control {b}")
    })
}

fn camps_of(row: &Row, seed: u64, names: &[&str]) -> KindRefill {
    let r = eq::camp_wipe_of(seed, names, 100, 240);
    KindRefill {
        killed: r.killed,
        half: days(r.half_ctrl),
        ninety_ctrl: days(r.ninety_ctrl),
        end_over_ctrl: r.end_over_ctrl,
        breaks: r.breaks,
        checks: need(r.breaks == 0, || format!("a camp broke: {r:?}"))
            .and(need(r.ninety_ctrl.is_some_and(|d| d <= 120), || {
                format!("not 90% of the control within 120 days: {r:?}")
            })),
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
    need(b <= 96.0, || {
        format!("median days to 90% of the control {b}")
    })
}

/// The day S9's den and grove kill-alls kill on, and the days the den and the groves run.
const MOWZIE_KILL_DAY: u64 = 30;

/// A den's or grove's [`eq::mowzie_run`] killed on [`MOWZIE_KILL_DAY`], read into the table's
/// shape (no control: the run is the rule alone). Its checks: K before the kill and none after.
fn mowzie(
    row: &Row,
    seed: u64,
    culture: &'static str,
    k: u16,
    to: u64,
) -> (KindRefill, eq::MowzieRun) {
    let d = MOWZIE_KILL_DAY;
    let r = eq::mowzie_run(
        seed,
        eq::mowzie_quiet_cultures(),
        culture,
        k,
        eq::MowziePresser::KillAll(d),
        to,
    );
    let pre = r.members[d as usize - 1];
    let from = |n: u32| {
        (d + 1..=to)
            .find(|&x| r.members[x as usize] >= n)
            .map(|x| (x - d) as f64)
    };
    let checks = need(pre == u32::from(k), || {
        format!("{pre} of {k} before the kill")
    })
    .and(need(r.members[d as usize] == 0, || {
        format!("{} left on the kill's day", r.members[d as usize])
    }));
    let out = KindRefill {
        killed: r.kills.iter().map(|x| u64::from(x.1)).sum(),
        pre: f64::from(pre),
        half: from(pre.div_ceil(2)),
        whole: r.whole_from(d + 1).map(|x| (x - d) as f64),
        checks,
        ..KindRefill::new(row, seed)
    };
    (out, r)
}

fn dens(row: &Row, seed: u64) -> KindRefill {
    let (mut out, r) = mowzie(row, seed, "frostmaw", 1, 60);
    let back = (MOWZIE_KILL_DAY + eq::EVERY) as usize;
    out.checks = out
        .checks
        .and(within_of("whole", out.whole, 11.5, 13.0))
        .and(need(r.members[back] == 1, || {
            "not whole at the census 24 days on".into()
        }))
        .and(need(r.arrivals == 1, || format!("{} arrivals", r.arrivals)));
    out
}

fn groves(row: &Row, seed: u64, k: u16) -> KindRefill {
    let (mut out, _) = mowzie(row, seed, "umvuthana", k, 160);
    out.checks = out.checks.and(within_of("whole", out.whole, 45.0, 100.0));
    out
}

fn groves_k5(row: &Row, seed: u64) -> KindRefill {
    groves(row, seed, 5)
}

fn groves_k7(row: &Row, seed: u64) -> KindRefill {
    groves(row, seed, 7)
}

fn groves_k9(row: &Row, seed: u64) -> KindRefill {
    groves(row, seed, 9)
}

/// No chance in the grove's rule: every seed's grove of one K is whole on the same day.
fn groves_judge(rs: &[KindRefill]) -> Result<(), String> {
    let first = rs.first().and_then(|r| r.whole);
    need(rs.iter().all(|r| r.whole == first), || {
        let ds: Vec<Option<f64>> = rs.iter().map(|r| r.whole).collect();
        format!("not one day for one K: {ds:?}")
    })
}

/// The grove the Umvuthi row kills in: S9's K 7.
const UMVUTHI_GROVE: u16 = 7;

fn umvuthi(row: &Row, seed: u64) -> KindRefill {
    let d = MOWZIE_KILL_DAY;
    let r = eq::umvuthi_kill(seed, UMVUTHI_GROVE, d, d + 60);
    let checks = need(r.held_before && !r.held_after, || {
        format!("held before {}, after {}", r.held_before, r.held_after)
    })
    .and(need((r.p_after - 1.0).abs() < 2e-3, || {
        format!("P {} after one Umvuthi of one", r.p_after)
    }))
    .and(within_of("the heir", r.whole, r.rule, r.rule + 1.0))
    .and(need(r.renewals == 1, || format!("{} heirs", r.renewals)))
    .and(need(r.warriors.0 == r.warriors.1, || {
        format!("warriors {:?}: the Umvuthi is no warrior", r.warriors)
    }));
    KindRefill {
        killed: u64::from(r.held_before && !r.held_after),
        pre: 1.0,
        half: r.whole,
        whole: r.whole,
        aux: r.renewals,
        p_after: Some(r.p_after),
        checks,
        ..KindRefill::new(row, seed)
    }
}

fn pirates(row: &Row, seed: u64) -> KindRefill {
    let r = rtsim_ships::synth::equilibrium::kill_all_pirates(seed);
    let longest = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    let checks = need(r.sunk > 0 && r.active_after == 0, || {
        format!(
            "sunk {} of {}, {} still active",
            r.sunk, r.slots, r.active_after
        )
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

/// `x` in `least..=most` days.
fn within_of(what: &str, x: Option<f64>, least: f64, most: f64) -> Result<(), String> {
    need(x.is_some_and(|d| (least..=most).contains(&d)), || {
        format!("{what} after {x:?} days, the band {least} to {most}")
    })
}

fn within(what: &str, x: Option<f64>, most: f64) -> Result<(), String> {
    need(x.is_some_and(|d| d <= most), || {
        format!("{what} after {x:?} days, the band's most {most}")
    })
}

fn herds_together(r: &KindRefill) -> Result<(), String> {
    within("half back", r.half, (25 * YEAR) as f64).and(need(r.whole.is_some(), || {
        "a range never alive again".into()
    }))
}

/// With pairs a Frontier dragon goes as a mate before it refills an empty lair (EQC576): the
/// alone row's 95th, 240 days, is each together seed's bound.
fn dragons_together(r: &KindRefill) -> Result<(), String> {
    within(
        "90% of the control",
        r.ninety_ctrl,
        (240 + eq::EVERY) as f64,
    )
}

fn nests_together(r: &KindRefill) -> Result<(), String> {
    within("whole", r.whole, 72.0)
}

fn any_seed(_: &KindRefill) -> Result<(), String> {
    Ok(())
}

fn camps_together(r: &KindRefill) -> Result<(), String> {
    within("90% of the control", r.ninety_ctrl, 120.0)
}

/// The wait a boss's rule sets at pressure `p` (`min(cap, base x 2^P)`), days: a den's Frostmaw
/// (`chief` false) or a grove's Umvuthi.
fn boss_wait(culture: &str, chief: bool, p: f32) -> f64 {
    let cs = rtsim_conflict::Cultures::builtin();
    let c = cs.get(cs.index_of(culture).expect("a built-in culture"));
    let rule = if chief { c.chief_renewal } else { c.renewal }
        .expect("a renewal")
        .rule;
    f64::from((rule.base_days * p.exp2()).min(rule.cap_days))
}

/// The alone band at the pressure the kill left (EQC492): the alone row's 12 days is the wait at
/// P 1, and a together site the roamer pressed before the shock is past 1 after it. The census
/// day is the read's grain.
fn boss_together(r: &KindRefill, culture: &str, chief: bool) -> Result<(), String> {
    let p = r.p_after.unwrap_or(1.0);
    within("whole", r.whole, boss_wait(culture, chief, p).ceil() + 1.0)
}

fn dens_together(r: &KindRefill) -> Result<(), String> {
    boss_together(r, "frostmaw", false)
}

fn groves_together(r: &KindRefill) -> Result<(), String> {
    within("whole", r.whole, 100.0)
}

fn umvuthi_together(r: &KindRefill) -> Result<(), String> {
    boss_together(r, "umvuthana", true)
}

fn pirates_together(r: &KindRefill) -> Result<(), String> {
    within("every slot", r.whole, 180.0)
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
        together: Some(herds_together),
        judge_together: true,
    },
    Row {
        name: "herds.local",
        kind: "herd.range",
        test: "local_wipe_refills_from_neighbours_30_seeds",
        band: "as herds.world; some range refounded by a neighbour",
        record: "EQC21",
        run: herds_local,
        judge: herds_local_judge,
        together: None,
        judge_together: true,
    },
    Row {
        name: "dragons",
        kind: "dragon.lair",
        test: "kill_all_dragons_30_seeds",
        band: "S4's world at the real lair density: median 90% of the control within 72 days, \
               95th within 240, every seed back within the run; end over the control median 0.9 \
               to 1.1",
        record: "EQC32; S4; EQC539, EQC576",
        run: dragons,
        judge: dragons_judge,
        together: Some(dragons_together),
        judge_together: true,
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
        together: Some(nests_together),
        judge_together: true,
    },
    Row {
        name: "villagers",
        kind: "village.human",
        test: "kill_all_villagers_30_seeds",
        band: "every village's settlers on day 32 to 33; median 90% of the control within 600 days",
        record: "EQC185",
        run: villagers,
        judge: villagers_judge,
        together: Some(any_seed),
        judge_together: true,
    },
    Row {
        name: "ribbits",
        kind: "village.ribbit",
        test: "kill_all_ribbits_30_seeds",
        band: "every village settled; median 90% of the control within 300 days",
        record: "EQC184",
        run: ribbits,
        judge: ribbits_judge,
        together: Some(any_seed),
        judge_together: true,
    },
    Row {
        name: "camps",
        kind: "conflict.camp",
        test: "kill_all_camps_30_seeds",
        band: "median 90% of the control within 96 days, each seed within 120, no break",
        record: "EQC270",
        run: camps,
        judge: camps_judge,
        together: Some(camps_together),
        judge_together: true,
    },
    Row {
        name: "outposts",
        kind: "conflict.camp",
        test: "kill_all_outposts_30_seeds",
        band: "as camps",
        record: "FB-5 (b), EQC428",
        run: outposts,
        judge: camps_judge,
        together: Some(camps_together),
        judge_together: true,
    },
    Row {
        name: "dens",
        kind: "conflict.den",
        test: "kill_all_dens_30_seeds",
        band: "each Frostmaw back in 11.5 to 13 days (the wait at P 1, 12), whole at the census 24 \
               days on, one arrival",
        record: "S9 (MZB-1262)",
        run: dens,
        judge: no_judge,
        together: Some(dens_together),
        judge_together: true,
    },
    Row {
        name: "groves.k5",
        kind: "conflict.grove",
        test: "kill_all_groves_30_seeds",
        band: "whole in 45 to 100 days, every seed on one day; K 5 before K 7 before K 9",
        record: "S9 (MZB-1210)",
        run: groves_k5,
        judge: groves_judge,
        together: Some(groves_together),
        judge_together: false,
    },
    Row {
        name: "groves.k7",
        kind: "conflict.grove",
        test: "kill_all_groves_30_seeds",
        band: "as groves.k5",
        record: "S9 (MZB-1210)",
        run: groves_k7,
        judge: groves_judge,
        together: Some(groves_together),
        judge_together: false,
    },
    Row {
        name: "groves.k9",
        kind: "conflict.grove",
        test: "kill_all_groves_30_seeds",
        band: "as groves.k5",
        record: "S9 (MZB-1210)",
        run: groves_k9,
        judge: groves_judge,
        together: Some(groves_together),
        judge_together: false,
    },
    Row {
        name: "umvuthi",
        kind: "conflict.umvuthi",
        test: "kill_all_umvuthi_30_seeds",
        band: "the heir within a day of the later of the cowed time and min(192, 6 x 2^P), 12 days \
               at P 1; one heir; no warrior lost",
        record: "EQC428; UNVERIFIED 8",
        run: umvuthi,
        judge: no_judge,
        together: Some(umvuthi_together),
        judge_together: true,
    },
    Row {
        name: "pirates",
        kind: "ship.haven",
        test: "kill_all_pirates_30_seeds",
        band: "every slot back within 180 days; no haven shut",
        record: "S8 (T32 to T39), PC38",
        run: pirates,
        judge: no_judge,
        together: Some(pirates_together),
        judge_together: true,
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
        .filter_map(|r| {
            r.checks
                .as_ref()
                .err()
                .map(|e| format!("seed {}: {e}", r.seed))
        })
        .collect();
    need(bad.is_empty(), || bad.join("; ")).and((row.judge)(rs))
}

/// The row's verdict on its together seeds: every seed's checks, then the judge's where it holds
/// together ([`Row::judge_together`]).
pub fn verdict_together(row: &Row, rs: &[KindRefill]) -> Result<(), String> {
    if row.judge_together {
        verdict(row, rs)
    } else {
        verdict(
            &Row {
                judge: no_judge,
                ..*row
            },
            rs,
        )
    }
}

/// The header of [`line`], in its widths.
pub fn header() -> String {
    format!(
        "{:<14} {:<16} {:>5}  {:>25}  {:>31}  {:>25}  {:>8}  verdict",
        "row",
        "kind",
        "seeds",
        "half: median [5th, 95th]",
        "90% of ctrl: median [5th, 95th]",
        "whole: median [5th, 95th]",
        "end/ctrl"
    )
}

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
        "{:<14} {:<16} {:>5}  {}  {:>31}  {:>25}  {:>8.3}  {}",
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
             breaks {} aux {}{}{}",
            r.seed,
            r.killed,
            r.pre,
            r.half,
            r.ninety_ctrl,
            r.whole,
            r.end_over_ctrl,
            r.breaks,
            r.aux,
            r.p_after
                .map_or(String::new(), |p| format!(" P after {p:.2}")),
            r.checks
                .as_ref()
                .err()
                .map_or(String::new(), |e| format!(" FAIL {e}"))
        );
    }
}

/// Runs row `name` on `seeds`, prints it, and panics unless it passes: the body of the slices'
/// `_30_seeds` tests.
#[cfg(test)]
pub fn assert_row(name: &str, seeds: &[u64]) -> Vec<KindRefill> {
    let row = row(name).unwrap_or_else(|| panic!("no kill-all row {name}"));
    let rs = run_row(row, seeds);
    let v = verdict(row, &rs);
    println!("{} ({}; {})", row.test, row.band, row.record);
    println!("{}", header());
    println!("{}", line(row, &rs, &v));
    print_seeds(&rs);
    if let Err(e) = v {
        panic!("{}: {e}", row.name);
    }
    rs
}

// ---- together (EQC429) ----------------------------------------------------------------------------

/// The together kill-all's shock day and the days read after it: four years and 600 days, the
/// herds' ten years and 40 (their alone row's).
pub fn shock_of(row: &str) -> (u64, u64) {
    if row == "herds.world" {
        (10 * YEAR, 40 * YEAR)
    } else {
        (4 * YEAR, 600)
    }
}

/// A together wipe read into the table's shape against its control (the same seed, the roamer
/// stopping on the same day, nothing killed).
pub fn together_refill(row: &Row, w: &Together, c: &Together) -> KindRefill {
    let shock = w.shock_day;
    let reads = |t: &Together| t.reads.get(row.name).cloned().unwrap_or_default();
    let (a, b) = (reads(w), reads(c));
    let before: Vec<f64> = a
        .iter()
        .filter(|x| x.0 + YEAR >= shock && x.0 < shock)
        .map(|x| x.1 as f64)
        .collect();
    let pre = before.iter().sum::<f64>() / before.len().max(1) as f64;
    let after = a.last().map_or(0, |x| x.0) - shock;
    let mut r = KindRefill {
        killed: w.killed,
        pre,
        p_after: w.p_after.get(row.name).copied(),
        ..KindRefill::new(row, w.seed)
    };
    let (mut tail, mut tail_c) = (0.0, 0.0);
    for (x, xc) in a.iter().zip(&b) {
        if x.0 <= shock {
            continue;
        }
        let since = (x.0 - shock) as f64;
        let (n, nc) = (x.1 as f64, xc.1 as f64);
        if r.half.is_none() && n >= 0.5 * pre {
            r.half = Some(since);
        }
        if r.ninety_ctrl.is_none() && n >= 0.9 * nc {
            r.ninety_ctrl = Some(since);
        }
        if r.whole.is_none() && x.3 {
            r.whole = Some(since);
        }
        if 2 * (x.0 - shock) >= after {
            tail += n;
            tail_c += nc;
        }
    }
    r.end_over_ctrl = tail / tail_c.max(1.0);
    let wrong = &w.teeth.wrong;
    let teeth = need(wrong.is_empty(), || format!("booked wrong: {wrong:?}"))
        .and(need(w.killed > 0, || "the wipe killed nothing".into()));
    r.checks = teeth.and(row.together.map_or(Ok(()), |f| f(&r)));
    r
}

/// The rows with a wipe in the every-kind world.
pub fn together_rows() -> Vec<&'static Row> {
    ROWS.iter()
        .filter(|r| r.together.is_some() && rtsim_host::every::READS.contains(&r.name))
        .collect()
}

/// Every row of `rows` together on `seeds`: per seed a control for each shock day and one run a
/// wipe (the three grove rows read one wipe's run, [`rtsim_host::every::wipe_of`]), on
/// [`threads`]. The rows' results in `rows`' order.
pub fn run_together(rows: &[&'static Row], seeds: &[u64]) -> Vec<Vec<KindRefill>> {
    use rtsim_host::every::wipe_of;
    let mut shocks: Vec<(u64, u64)> = rows.iter().map(|r| shock_of(r.name)).collect();
    shocks.sort_unstable();
    shocks.dedup();
    let mut wipes: Vec<&'static str> = rows.iter().map(|r| wipe_of(r.name)).collect();
    wipes.sort_unstable();
    wipes.dedup();
    let mut jobs: Vec<(u64, Option<&'static str>, u64, u64)> = Vec::new();
    for &s in seeds {
        for &(at, after) in &shocks {
            jobs.push((s, None, at, at + after));
        }
        for &x in &wipes {
            let (at, after) = shock_of(x);
            jobs.push((s, Some(x), at, at + after));
        }
    }
    let got = par_map(&jobs, threads(), |&(s, wipe, at, to)| {
        rtsim_host::every::together(s, wipe, at, to)
    });
    rows.iter()
        .map(|row| {
            let (at, _) = shock_of(row.name);
            seeds
                .iter()
                .map(|&s| {
                    let w = got
                        .iter()
                        .find(|t| t.seed == s && t.wipe == Some(wipe_of(row.name)))
                        .expect("the wipe ran");
                    let c = got
                        .iter()
                        .find(|t| t.seed == s && t.wipe.is_none() && t.shock_day == at)
                        .expect("its control ran");
                    together_refill(row, w, c)
                })
                .collect()
        })
        .collect()
}

fn bad(why: &str) -> ExitCode {
    eprintln!(
        "{why}\nusage: rtsim-cli sim killall [--kind K[,K...]] [--seeds A..B] [--together] [--verbose]"
    );
    ExitCode::from(64)
}

/// `rtsim-cli sim killall`: each row on the seeds (1 to 30 by default), one line a row; exits 1
/// on any FAIL.
pub fn cli(rest: &[&str]) -> ExitCode {
    let mut rows: Vec<&'static Row> = ROWS.iter().collect();
    let (mut from, mut to) = (1u64, 30u64);
    let (mut verbose, mut together) = (false, false);
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
            "--together" => together = true,
            other => return bad(&format!("unknown option {other}")),
        }
    }
    let seeds: Vec<u64> = (from..=to).collect();
    println!(
        "# rtsim-cli sim killall: seeds {from} to {to}, {} threads; {}",
        threads(),
        if together {
            "every kind together in the every-kind world (EQC429)"
        } else {
            "each row's alone harness"
        }
    );
    for (k, why) in NO_ROW {
        println!("# {k}: no row, {why}");
    }
    for r in &rows {
        println!("# {}: {} ({}; {})", r.name, r.band, r.record, r.test);
    }
    println!("{}", header());
    let mut failed = false;
    let results: Vec<(&'static Row, Vec<KindRefill>)> = if together {
        let can = together_rows();
        let rows: Vec<&'static Row> = rows
            .into_iter()
            .filter(|r| can.iter().any(|c| c.name == r.name))
            .collect();
        rows.iter()
            .copied()
            .zip(run_together(&rows, &seeds))
            .collect()
    } else {
        rows.into_iter().map(|r| (r, run_row(r, &seeds))).collect()
    };
    for (row, rs) in results {
        let v = if together {
            verdict_together(row, &rs)
        } else {
            verdict(row, &rs)
        };
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
        let founded = w.founded.clone();
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
            let c = cs.iter().find(|c| c.kind == k).expect("the row");
            assert_eq!(c.sites, founded.sites(k), "{k}");
        }
    }

    /// EQC430: every row on seeds 1 to 30, printed as the table; fails if any row fails.
    #[test]
    #[ignore = "hours: run in release with --ignored --nocapture"]
    fn kill_all_every_kind_30_seeds() {
        let seeds: Vec<u64> = (1..=30).collect();
        println!("{}", header());
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

    /// EQC429: every row's wipe in one engine with every other module running, seeds 1 to 8,
    /// each against the same seed's control; the band is the alone row's, unchanged.
    #[test]
    #[ignore = "hours: run in release with --ignored --nocapture"]
    fn kill_all_in_one_engine_8_seeds() {
        let seeds: Vec<u64> = (1..=8).collect();
        let rows = together_rows();
        let got = run_together(&rows, &seeds);
        println!("{}", header());
        let mut failed = Vec::new();
        for (row, rs) in rows.iter().zip(&got) {
            let v = verdict_together(row, rs);
            println!("{}", line(row, rs, &v));
            print_seeds(rs);
            if v.is_err() {
                failed.push(row.name);
            }
        }
        assert!(failed.is_empty(), "failing rows: {failed:?}");
    }

    /// The Umvuthi's row (EQC428): S9 had no kill-all for the one boss with a pressure of its own.
    #[test]
    fn kill_all_umvuthi() {
        assert_row("umvuthi", &[1, 2, 3]);
    }

    #[test]
    #[ignore = "minutes: run in release with --ignored --nocapture"]
    fn kill_all_umvuthi_30_seeds() {
        assert_row("umvuthi", &(1..=30).collect::<Vec<_>>());
    }

    #[test]
    #[ignore = "minutes: run in release with --ignored --nocapture"]
    fn kill_all_pirates_30_seeds() {
        assert_row("pirates", &(1..=30).collect::<Vec<_>>());
    }
}
