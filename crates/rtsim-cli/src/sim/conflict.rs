// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim conflict`: PLAN.md §10.3's walkthrough, headless (rtsim-conflict).
//!
//! **The world** is the phase 1 synthetic world with a goblin camp, Gloomhollow, joined as a
//! haven in the Thornwood. **The player** builds a refinery at the forest's edge (an industry
//! district in `rtsim-industry`'s fake world; its smog counts against the player who runs it, P14
//! as changed) and logs the forest
//! for five days (the player's own act). It is at the refinery until day 8, offline until day 13,
//! then back. **The plugin** is `rtsim_conflict::synth::FakeConflict`: every record goes through
//! the boundary codec (docs/boundary.md §9). At home the player defends the refinery against the
//! director's waves and kills the war chief, when a band comes while he is home: a calm chief's
//! feud can stay on Raid after the raid while he is offline (CONFLICT-LIFE.md CL-D59, open).
//!
//! Three runs: **ignore** (the envoy's demand goes unmet: scouts, then a war band that meets the
//! refinery while the player is offline, resolved by the industry engine and applied on load,
//! capped), **tribute** (the player pays each envoy he meets; one that reaches the refinery while he
//! is offline runs out unmet, and the feud climbs), **converter** (a catalytic converter half a
//! day after the demand). A fourth run of **ignore** is saved and restored in the middle and must
//! end with the same state hashes.
//!
//! It also counts the event archive's rows (R, VIEWER.md §10.4) in each of the three variants
//! (every tape of the run on, see `Options::measure`); the saved and restored fourth run is a
//! comparison and records nothing. The summary R is the largest variant's.
//!
//! Exit 1 if a check fails.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_conflict::culture::PartyKind;
use rtsim_conflict::engine::{ConflictStats, FeudView, LogEntry};
use rtsim_conflict::scenario::{self, BACK_DAY, OFFLINE_DAY, Options, Outcome, Rung, Variant};
use rtsim_conflict::{Step, synth};
use rtsim_industry::district::AttackReport;
use rtsim_kernel::clock::TICKS_PER_DAY;
use rtsim_settle::NodeKind;
use rtsim_trade::{GoodId, MILLI};
use rtsim_world::World;
use serde::Serialize;

use super::archive::Rate;

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
    /// What each village feels at the end, toward the camp and toward the players.
    feelings: Vec<Feeling>,
    stats: ConflictStats,
    log: Vec<LogEntry>,
    plugin_commands: Vec<(f64, String)>,
    wire_bytes: u64,
    state_hashes: (String, String),
    /// The archive rows every tape of the run took per game day.
    archive: Rate,
}

/// A village's sentiment at the run's end (rtsim-knowledge's scale, -127 to 127; it fades).
#[derive(Serialize)]
struct Feeling {
    village: String,
    /// Toward the camp: a robbery counts -50 first-hand and half that heard (MZB-36).
    toward_camp: i8,
    /// Toward each player it feels anything about: a rescue counts +25 first-hand and half that
    /// heard, and only a rout a village or a base saw is a rescue report at all (CL-D61).
    toward_players: BTreeMap<String, i8>,
}

#[derive(Serialize)]
struct Results {
    seed: u64,
    days: u64,
    offline_day: f64,
    back_day: f64,
    runs: Vec<RunOut>,
    restore: Check,
    /// The largest variant's archive rate, the scenario's R.
    archive: Rate,
    archive_variant: Variant,
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
    // The feud the refinery's smog feeds: the player who runs it (P14, changed).
    let owner = o.smog_owner.clone();
    let owned: Vec<&Rung> = o.rungs.iter().filter(|r| r.actor == owner).collect();
    let settlers: Vec<Step> = owned.iter().map(|r| r.step).collect();
    let st = &c.stats;
    let f = c.faction_of_key(synth::CAMP).expect("camp");
    let feud = c
        .feuds_of(f)
        .iter()
        .find(|(a, _)| c.actor_name(&o.world, *a) == owner)
        .and_then(|(a, _)| c.feud_view(synth::CAMP, *a));
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
                format!("{owner}'s rungs (the smog's owner): {settlers:?}"),
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
            // A band that arrives where the player watches comes in the director's waves and he
            // kills its chief. None comes while he is home when the feud parks on Raid: each climb
            // sends its rung's party once and only Siege repeats (K2-8, MOWZIE.md §3.3), and a calm
            // chief's Invasion threshold (bent up to 40 percent up) can sit above the grievance
            // this script's smog raises (CONFLICT-LIFE.md CL-D59, open).
            let invasion = c.rung_threshold(f, Step::Invasion);
            let parked = st.parties[PartyKind::Invasion.index()] == 0
                && feud.as_ref().is_some_and(|v| {
                    v.step == Step::Raid && v.party.is_none() && v.grievance < invasion
                });
            check(
                "the director's waves and the war chief",
                if st.raids_watched > 0 {
                    st.chiefs_killed > 0 && st.successions > 0
                } else {
                    parked
                },
                if st.raids_watched > 0 {
                    format!(
                        "{} watched raid(s), {} wave(s), {} chief(s) killed, {} heir(s) took over",
                        st.raids_watched, st.waves, st.chiefs_killed, st.successions
                    )
                } else {
                    format!(
                        "no war band came while the player was home: the feud stays on Raid at \
                         grievance {:.1}, under this chief's Invasion threshold {invasion:.1}, and a \
                         Raid is sent once (CL-D59, open); {owner}'s rungs: {settlers:?}",
                        feud.as_ref().map_or(0.0, |v| v.grievance)
                    )
                },
            );
        }
        Variant::Tribute => {
            // The scripted player pays every envoy he meets (one materializes only where he
            // watches). A paid demand settles at once, back to notice, and holds the ladder there
            // for the respite (K2-10, K2-11). An envoy that reaches the refinery while he is
            // offline waits there, but its deadline is game time (raids do not wait for the
            // owner, PLAN.md §10.3): the demand runs out unmet and the feud climbs as the ignore
            // run's does. So no band marches before such a demand has run out.
            let met: BTreeSet<&str> = o
                .plugin
                .commands
                .iter()
                .filter_map(|(_, s)| s.strip_prefix("ENVOY_MATERIALIZE: envoy "))
                .filter_map(|s| s.split(':').next())
                .collect();
            let days = |kind: &str, what: &str| -> Vec<f64> {
                c.log
                    .iter()
                    .filter(|l| l.kind == kind && l.what.contains(what))
                    .map(|l| l.day)
                    .collect()
            };
            let paid = days("settled", "tribute paid");
            let unmet = days("refused", "goes unmet");
            let open = u64::from(feud.as_ref().is_some_and(|v| v.demand.is_some()));
            let respite = c
                .cultures
                .get(c.factions[f as usize].culture)
                .def
                .respite_days;
            let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
            let held = paid.iter().all(|d| {
                owned
                    .iter()
                    .position(|r| near(r.day, *d) && r.step == Step::Notice)
                    .is_some_and(|i| {
                        owned
                            .get(i + 1)
                            .is_none_or(|n| n.step <= Step::Notice || n.day + 1e-6 >= d + respite)
                    })
            });
            let climb = owned.iter().find(|r| r.step > Step::Envoy).map(|r| r.day);
            let climbed_after_unmet = climb
                .is_none_or(|d| d >= OFFLINE_DAY && unmet.first().is_some_and(|u| *u <= d + 1e-6));
            let list = |v: &[f64]| {
                v.iter()
                    .map(|d| format!("{d:.2}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            check(
                "tribute de-escalates",
                st.tributes > 0
                    && paid.len() as u64 == st.tributes
                    && met.len() as u64 == st.tributes
                    && st.demands == st.tributes + st.expired + open
                    && held
                    && climbed_after_unmet,
                format!(
                    "{} envoy(s) met and {} tribute(s) paid (days {}), each back to notice for the \
                     {respite}-day respite; {}; {owner}'s rungs (the smog's owner): {settlers:?}",
                    met.len(),
                    st.tributes,
                    list(&paid),
                    match climb {
                        None if unmet.is_empty() => "no demand unmet, no band".to_string(),
                        None => format!("demand(s) unmet on day(s) {}, no band", list(&unmet)),
                        Some(d) => format!(
                            "{} demand(s) unmet while the player was offline (day(s) {}), the \
                             first climb past envoy on day {d:.2}",
                            st.expired,
                            list(&unmet)
                        ),
                    },
                ),
            );
        }
        Variant::Converter => {
            check(
                "a catalytic converter de-escalates",
                // P14 (changed): the smog is the refinery player's, whose own logging (a direct
                // act) shares the feud, so it ends at notice, not at nothing; no band marches.
                st.cuts > 0
                    && st.parties[PartyKind::Raid.index()] == 0
                    && st.parties[PartyKind::Scouts.index()] == 0
                    && settlers.last().is_some_and(|s| *s <= Step::Notice),
                format!(
                    "the demand met by cutting the signal on day {:.2}; {owner}'s rungs (the smog's owner): {settlers:?}",
                    o.converter_day.unwrap_or(0.0)
                ),
            );
        }
    }
    out
}

fn run_out(o: Outcome, v: Variant, wall_ms: f64, days: u64) -> RunOut {
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
    let (camp, now) = (c.factions[f as usize].actor, w.now());
    let feelings = w
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, NodeKind::Village(_)))
        .map(|(i, n)| Feeling {
            village: rtsim_world::synth::NAMES
                .get(i)
                .map_or_else(|| format!("node {}", n.key), |s| s.to_string()),
            toward_camp: w.knowledge.sentiment(n.actor, camp, now),
            toward_players: w
                .knowledge
                .sentiment_snapshot(n.actor, now)
                .into_iter()
                .filter(|(a, _)| w.knowledge.actors().is_player(*a))
                .map(|(a, v)| (c.actor_name(w, a), v))
                .collect(),
        })
        .collect();
    let feuds = c
        .feuds_of(f)
        .iter()
        .filter_map(|(a, _)| c.feud_view(synth::CAMP, *a))
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
        feelings,
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
        archive: Rate::new(&o.meter, days as f64),
    }
}

fn main(o: Opts) -> i32 {
    let mut runs = Vec::new();
    let mut ignore_hashes = (0, 0);
    for v in [Variant::Ignore, Variant::Tribute, Variant::Converter] {
        let t0 = Instant::now();
        let out = scenario::run(&Options {
            measure: true,
            ..Options::new(o.seed, o.days, v)
        });
        if v == Variant::Ignore {
            ignore_hashes = out.hashes;
        }
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        runs.push(run_out(out, v, ms, o.days));
    }
    let day = OFFLINE_DAY + 1.3;
    let restarted = scenario::run(&Options {
        restart_day: Some(day),
        ..Options::new(o.seed, o.days, Variant::Ignore)
    });
    let restore = Check {
        name: "a restore continues with the same state",
        ok: restarted.hashes == ignore_hashes,
        detail: format!(
            "saved and loaded on day {day:.1}: conflict {:016x}, world {:016x} (uninterrupted {:016x}, {:016x})",
            restarted.hashes.0, restarted.hashes.1, ignore_hashes.0, ignore_hashes.1
        ),
    };
    let largest = runs
        .iter()
        .max_by(|a, b| a.archive.rows_per_day.total_cmp(&b.archive.rows_per_day))
        .expect("three runs");
    let (archive, archive_variant) = (largest.archive.clone(), largest.variant);
    let res = Results {
        seed: o.seed,
        days: o.days,
        offline_day: OFFLINE_DAY,
        back_day: BACK_DAY,
        runs,
        restore,
        archive,
        archive_variant,
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
        let camp: Vec<String> = run
            .feelings
            .iter()
            .filter(|f| f.toward_camp != 0)
            .map(|f| format!("{} {}", f.village, f.toward_camp))
            .collect();
        println!("  villages toward the camp at the end: {}", camp.join(", "));
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
    for run in &r.runs {
        run.archive
            .print(Some(&format!("{:?}", run.variant).to_lowercase()));
    }
    r.archive.print(Some(&format!(
        "largest, {}",
        format!("{:?}", r.archive_variant).to_lowercase()
    )));
}
