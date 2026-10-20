// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim conflict`: PLAN.md §10.3's walkthrough, headless (rtsim-conflict).
//!
//! **The world** is the phase 1 synthetic world with a goblin camp, Gloomhollow, joined as a
//! haven in the Thornwood. **The player** builds a refinery at the forest's edge (an industry
//! district in `rtsim-industry`'s fake world; its smog is the settlers', P14) and logs the forest
//! for five days (the player's own act). It is at the refinery until day 8, offline until day 13,
//! then back. **The plugin** is `rtsim_conflict::synth::FakeConflict`: every record goes through
//! the boundary codec (docs/boundary.md §9). At home the player defends the refinery against the
//! director's waves and kills the war chief.
//!
//! Three runs: **ignore** (the envoy's demand goes unmet: scouts, then a war band that meets the
//! refinery while the player is offline, resolved by the industry engine and applied on load,
//! capped), **tribute** (the player pays each envoy), **converter** (a catalytic converter half a
//! day after the demand). A fourth run of **ignore** is saved and restored in the middle and must
//! end with the same state hashes.
//!
//! Exit 1 if a check fails.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::{ConflictStats, FeudView, LogEntry};
use rtsim_conflict::scenario::{self, BACK_DAY, OFFLINE_DAY, Options, Outcome, Rung, Variant};
use rtsim_conflict::{Step, synth};
use rtsim_industry::district::AttackReport;
use rtsim_kernel::clock::TICKS_PER_DAY;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;
use serde::Serialize;

struct Opts {
    days: u64,
    seed: u64,
    out: Option<PathBuf>,
    quiet: bool,
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let mut o = Opts {
        days: 24,
        seed: 7,
        out: Some(PathBuf::from("scenario/results/sim-conflict.json")),
        quiet: false,
    };
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--days" => match num(it.next()) {
                Some(n) if n >= 16 => o.days = n,
                _ => return bad("--days needs a number of at least 16"),
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
        "{why}\nusage: rtsim-cli sim conflict [--days N] [--seed S] [--out FILE | --no-write] [--quiet]"
    );
    ExitCode::from(64)
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct RunOut {
    variant: Variant,
    wall_ms: f64,
    checks: Vec<Check>,
    rungs: Vec<Rung>,
    feuds: Vec<FeudView>,
    /// Attacks on the refinery as the industry engine resolved them.
    refinery_attacks: Vec<AttackReport>,
    blocks_broken_on_load: usize,
    reloaded_day: Option<f64>,
    converter_day: Option<f64>,
    /// Per good name, items: stolen from villages, from players' districts, landed at the haven,
    /// tribute and gifts stocked there.
    ledger: BTreeMap<&'static str, BTreeMap<String, f64>>,
    unmapped_industry_goods: BTreeMap<String, i64>,
    haven_stock: BTreeMap<String, f64>,
    stats: ConflictStats,
    log: Vec<LogEntry>,
    plugin_commands: Vec<(f64, String)>,
    wire_bytes: u64,
    state_hashes: (String, String),
}

#[derive(Serialize)]
struct Results {
    seed: u64,
    days: u64,
    offline_day: f64,
    back_day: f64,
    runs: Vec<RunOut>,
    restore: Check,
}

fn by_name(w: &World, m: &BTreeMap<GoodId, i128>) -> BTreeMap<String, f64> {
    m.iter()
        .filter(|e| *e.1 != 0)
        .map(|(g, a)| (w.goods.get(*g).name.clone(), *a as f64 / MILLI as f64))
        .collect()
}

fn checks_for(o: &Outcome, v: Variant) -> Vec<Check> {
    let mut out = Vec::new();
    let mut check = |name: &'static str, ok: bool, detail: String| {
        out.push(Check { name, ok, detail });
    };
    let c = &o.conflict;
    let books = c.check(&o.world);
    check(
        "conservation",
        books.is_ok(),
        books.err().unwrap_or_else(|| {
            "villages' stock, players' goods, havens and the world's books agree".into()
        }),
    );
    let fw = o.fake.conservation();
    check(
        "industry world totals",
        fw.is_ok(),
        fw.err()
            .unwrap_or_else(|| "the refinery's physical totals match the engine's books".into()),
    );
    let settlers: Vec<Step> = o
        .rungs
        .iter()
        .filter(|r| r.actor == "the settlers")
        .map(|r| r.step)
        .collect();
    let st = &c.stats;
    match v {
        Variant::Ignore => {
            let pos = |s: Step| settlers.iter().position(|x| *x == s);
            let ladder = matches!(
                (pos(Step::Notice), pos(Step::Envoy), pos(Step::Scouts), pos(Step::Raid)),
                (Some(a), Some(b), Some(c), Some(d)) if a < b && b < c && c < d
            );
            check(
                "notice, envoy and demand, scouts, a war band",
                ladder && st.demands > 0 && st.expired > 0,
                format!("the settlers' rungs: {settlers:?}"),
            );
            let offline: Vec<&AttackReport> = o
                .attacks
                .iter()
                .filter(|a| {
                    let d = a.at.0 as f64 / TICKS_PER_DAY as f64;
                    (OFFLINE_DAY..BACK_DAY).contains(&d) && a.skipped.is_none()
                })
                .collect();
            let destroyed: u32 = offline.iter().map(|a| a.destroyed).sum();
            let stolen: i128 = offline.iter().map(|a| a.stolen.total()).sum();
            check(
                "a raid while the player is offline, resolved abstractly",
                !offline.is_empty() && offline.iter().all(|a| a.outcome.is_some()),
                format!(
                    "{} attack(s) by class while offline: {stolen} items stolen, {destroyed} blocks",
                    offline.len()
                ),
            );
            let cap = rtsim_industry::tunables::Tunables::default()
                .threat
                .destroy_cap_blocks;
            check(
                "applied on load, capped",
                o.blocks_broken as u32 == destroyed
                    && destroyed > 0
                    && offline.iter().all(|a| a.destroyed <= cap)
                    && o.reloaded_day.is_some(),
                format!(
                    "{} blocks broken when the refinery loaded on day {:.2} (cap {cap} per attack)",
                    o.blocks_broken,
                    o.reloaded_day.unwrap_or(0.0)
                ),
            );
            check(
                "the director's waves and the war chief",
                st.raids_watched > 0 && st.chiefs_killed > 0 && st.successions > 0,
                format!(
                    "{} watched raid(s), {} wave(s), {} chief(s) killed, {} heir(s) took over",
                    st.raids_watched, st.waves, st.chiefs_killed, st.successions
                ),
            );
        }
        Variant::Tribute => {
            check(
                "tribute de-escalates",
                st.tributes > 0
                    && st.parties[PartyKind::Raid.index()] == 0
                    && st.parties[PartyKind::Invasion.index()] == 0
                    && settlers.iter().all(|s| *s <= Step::Envoy),
                format!(
                    "{} tribute(s) paid, no war band; the settlers' rungs: {settlers:?}",
                    st.tributes
                ),
            );
        }
        Variant::Converter => {
            check(
                "a catalytic converter de-escalates",
                st.cuts > 0
                    && st.parties[PartyKind::Raid.index()] == 0
                    && settlers.last() == Some(&Step::None),
                format!(
                    "the demand met by cutting the signal on day {:.2}; the settlers' rungs: {settlers:?}",
                    o.converter_day.unwrap_or(0.0)
                ),
            );
        }
    }
    out
}

fn run_out(o: Outcome, v: Variant, wall_ms: f64) -> RunOut {
    let checks = checks_for(&o, v);
    let c = &o.conflict;
    let w = &o.world;
    let mut ledger = BTreeMap::new();
    ledger.insert("from_villages", by_name(w, &c.ledger.from_villages));
    ledger.insert("from_players", by_name(w, &c.ledger.from_players));
    ledger.insert("landed_at_haven", by_name(w, &c.ledger.landed));
    ledger.insert(
        "players_goods_landed_at_haven",
        by_name(w, &c.ledger.landed_from_players),
    );
    ledger.insert("tribute", by_name(w, &c.ledger.tribute));
    ledger.insert("gifts", by_name(w, &c.ledger.gifts));
    let f = c.faction_of_key(synth::CAMP).expect("camp");
    let haven = w.market(c.factions[f as usize].node);
    let haven_stock = w
        .goods
        .ids()
        .filter(|g| haven.stock(*g) > 0)
        .map(|g| {
            (
                w.goods.get(g).name.clone(),
                haven.stock(g) as f64 / MILLI as f64,
            )
        })
        .collect();
    let feuds = c
        .feuds
        .keys()
        .filter_map(|&(_, a)| c.feud_view(synth::CAMP, a))
        .collect();
    RunOut {
        variant: v,
        wall_ms,
        checks,
        rungs: o.rungs.clone(),
        feuds,
        refinery_attacks: o.attacks.clone(),
        blocks_broken_on_load: o.blocks_broken,
        reloaded_day: o.reloaded_day,
        converter_day: o.converter_day,
        ledger,
        unmapped_industry_goods: c.ledger.unmapped.clone(),
        haven_stock,
        stats: c.stats,
        log: c.log.clone(),
        plugin_commands: o
            .plugin
            .commands
            .iter()
            .map(|(t, s)| (*t as f64 / TICKS_PER_DAY as f64, s.clone()))
            .collect(),
        wire_bytes: o.plugin.wire_bytes,
        state_hashes: (
            format!("{:016x}", o.hashes.0),
            format!("{:016x}", o.hashes.1),
        ),
    }
}

fn main(o: Opts) -> i32 {
    let mut runs = Vec::new();
    let mut ignore_hashes = (0, 0);
    for v in [Variant::Ignore, Variant::Tribute, Variant::Converter] {
        let t0 = Instant::now();
        let out = scenario::run(&Options {
            seed: o.seed,
            days: o.days,
            variant: v,
            restart_day: None,
        });
        if v == Variant::Ignore {
            ignore_hashes = out.hashes;
        }
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        runs.push(run_out(out, v, ms));
    }
    let day = OFFLINE_DAY + 1.3;
    let restarted = scenario::run(&Options {
        seed: o.seed,
        days: o.days,
        variant: Variant::Ignore,
        restart_day: Some(day),
    });
    let restore = Check {
        name: "a restore continues with the same state",
        ok: restarted.hashes == ignore_hashes,
        detail: format!(
            "saved and loaded on day {day:.1}: conflict {:016x}, world {:016x} (uninterrupted {:016x}, {:016x})",
            restarted.hashes.0, restarted.hashes.1, ignore_hashes.0, ignore_hashes.1
        ),
    };
    let res = Results {
        seed: o.seed,
        days: o.days,
        offline_day: OFFLINE_DAY,
        back_day: BACK_DAY,
        runs,
        restore,
    };
    let failed = res.runs.iter().flat_map(|r| &r.checks).any(|c| !c.ok) || !res.restore.ok;
    if !o.quiet {
        print_report(&res);
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

fn print_report(r: &Results) {
    println!(
        "sim conflict: seed {}, {} days; the player is offline from day {} to day {}",
        r.seed, r.days, r.offline_day, r.back_day
    );
    for run in &r.runs {
        println!(
            "\n== {:?} ({:.0} ms, {} events)",
            run.variant, run.wall_ms, run.stats.events
        );
        for l in &run.log {
            if matches!(l.kind, "faction" | "muster" | "home") && run.variant != Variant::Ignore {
                continue;
            }
            println!("{:7.2}  {:<9} {}", l.day, l.kind, l.what);
        }
        for c in &run.checks {
            println!(
                "  [{}] {}: {}",
                if c.ok { "ok" } else { "FAIL" },
                c.name,
                c.detail
            );
        }
    }
    println!(
        "\n  [{}] {}: {}",
        if r.restore.ok { "ok" } else { "FAIL" },
        r.restore.name,
        r.restore.detail
    );
}
