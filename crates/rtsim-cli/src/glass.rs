// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli glass <world> [district] [--json | --graph]`: the glass box's paged state of a saved
//! world (SECTIONS.md §8.2, slice S1), read from the confirmed save through the read-only open, as
//! `rtsim-cli lines` reads train lines. Per district: its compile (records and counts), what
//! replays it, its last and its passed window, its ticking chunks and its compile's sections
//! (§5, slice S2: each section's nodes, stores, box and fingerprint, the stores they share and
//! their judging groups), and its judging units (slice S2b, with two or more: each unit's sections,
//! stores and chunks, its open window, its last and passed windows, strikes, edit and whether it may
//! replay; the extra rows they hold and a row cap's cut, §7.2; slice S2c: whether each replays or a
//! fallback froze it, the freezes and the nodes gated off). `--graph` writes the compile's
//! records for offline checks (§9.4), with
//! each node's section. A page the engine would drop at its restore (the format before sections,
//! or sections its compile no longer cuts into, with the default `glass.sectionsMax`) is listed
//! as dropped, with why.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use rtsim_industry::glass::check::{Compiled, DistrictGlass, WindowResult};
use rtsim_industry::glass::page::{PageRead, read_district_page};
use rtsim_industry::glass::report::replay_model;
use rtsim_industry::glass::sections::{NONE, Sections};
use rtsim_industry::ids::Good;
use rtsim_industry::persist::GLASS_TABLE;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::page::PageKey;
use rtsim_kernel::rows::PageSource;
use rtsim_view::industry::IndustryView;
use rtsim_view::journal::Names;
use rtsim_view::keys::DICT_NAME;
use rtsim_view::{Limits, load_world};
use serde_json::{Value, json};

const USAGE: &str = "usage: rtsim-cli glass <world> [district] [--json | --graph]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Out {
    Text,
    Json,
    Graph,
}

pub fn cli(args: &[&str]) -> ExitCode {
    let mut world = None;
    let mut district = None;
    let mut out = Out::Text;
    for a in args {
        match *a {
            "--json" if out != Out::Graph => out = Out::Json,
            "--graph" if out != Out::Json => out = Out::Graph,
            "--json" | "--graph" => {
                eprintln!("--json and --graph are one or the other\n{USAGE}");
                return ExitCode::from(64);
            }
            a if a.starts_with("--") => {
                eprintln!("unknown option {a}\n{USAGE}");
                return ExitCode::from(64);
            }
            a if world.is_none() => world = Some(a),
            a if district.is_none() => match a.parse::<u64>() {
                Ok(d) => district = Some(d),
                Err(_) => {
                    eprintln!("a district is its id (a number), not {a}\n{USAGE}");
                    return ExitCode::from(64);
                }
            },
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            }
        }
    }
    let Some(world) = world else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    match report(Path::new(world), district, out == Out::Graph) {
        Ok(v) => {
            if out == Out::Text {
                print_text(&v);
            } else {
                println!("{}", serde_json::to_string_pretty(&v).expect("serialize"));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rtsim-cli glass: {e}");
            ExitCode::FAILURE
        }
    }
}

fn report(world: &Path, only: Option<u64>, graph: bool) -> Result<Value, String> {
    let saved = load_world(world, Limits::default())?;
    let Some(iv) = saved.industry.as_ref() else {
        return Err(format!("{} has no industry pages", world.display()));
    };
    let ids = saved
        .source
        .table_ids(GLASS_TABLE)
        .map_err(|e| format!("cannot list the glass pages: {e}"))?;
    if let Some(d) = only
        && !ids.contains(&d)
    {
        return Err(format!("district {d} has no glass page"));
    }
    let mut districts = Vec::new();
    for id in ids {
        if only.is_some_and(|d| d != id) {
            continue;
        }
        let b = saved
            .source
            .read(PageKey::new(GLASS_TABLE, id))
            .map_err(|e| format!("glass page {id}: {e}"))?
            .ok_or_else(|| format!("glass page {id} is listed and missing"))?;
        let tun = Tunables::default().glass;
        match read_district_page(&b, tun.sections_max, tun.counted_stores_max)
            .map_err(|e| format!("glass page {id}: {e}"))?
        {
            PageRead::Kept(dg) => districts.push(district(iv, &saved.names, id, &dg, graph)),
            PageRead::Dropped(why) => districts.push(json!({
                "district": id,
                "name": name_of(iv, &saved.names, id),
                "dropped": why,
            })),
        }
    }
    Ok(json!({
        "world": saved.scrub(&world.display().to_string()),
        "now": iv.now,
        "districts": districts,
    }))
}

fn name_of(iv: &IndustryView, names: &Names, id: u64) -> Option<String> {
    iv.districts
        .iter()
        .find(|d| d.id == id)
        .and_then(|d| names.get(DICT_NAME, d.name))
        .map(str::to_string)
}

fn pos(p: rtsim_industry::ids::BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}

/// A compile's sections (SECTIONS.md §5).
fn sections(s: &Sections) -> Value {
    json!({
        "count": s.len(),
        "merged": s.merged,
        "groups": s.groups(),
        "shared_stores": s.shared.iter().map(|(n, by)| json!({"store": n, "sections": by})).collect::<Vec<_>>(),
        "list": s.list.iter().map(|x| json!({
            "nodes": x.nodes,
            "stores": x.stores,
            "working": x.working,
            "low": x.low.map(pos),
            "box": x.bbox.map(|(a, b)| [pos(a), pos(b)]),
            "domains": x.domains,
            "fingerprint": format!("{:x}", x.fingerprint),
        })).collect::<Vec<_>>(),
    })
}

fn district(iv: &IndustryView, names: &Names, id: u64, dg: &DistrictGlass, graph: bool) -> Value {
    let mut v = json!({
        "district": id,
        "name": name_of(iv, names, id),
        "replay": match dg.replay_model {
            replay_model::NONE => "none",
            replay_model::BLACK_BOX => "black box",
            replay_model::GLASS => "glass",
            _ => "?",
        },
        "replay_since": dg.replay_since.map(|t| t.0),
        "fallback": dg.fallback.map(|f| f.words()),
        "not_compilable": dg.not_compilable.map(|(c, r)| json!({"compile": c, "reason": r})),
        "ticking_chunks": dg.ticking.iter().map(|c| [c.x, c.z]).collect::<Vec<_>>(),
        "demoted_models": dg.demoted.iter().collect::<Vec<_>>(),
    });
    let Some(c) = dg.compiled.as_ref() else {
        v["compile"] = Value::Null;
        return v;
    };
    let g = &c.graph;
    let bounded = g
        .nodes
        .iter()
        .filter(|n| n.flags & rtsim_industry::glass::records::node_flags::BOUNDED != 0)
        .count();
    let mut compile = json!({
        "compile": g.end.compile,
        "fingerprint": format!("{:x}", g.end.fingerprint),
        "revision": g.end.revision,
        "frame": g.end.frame,
        "nodes": g.nodes.len(),
        "bounded_nodes": bounded,
        "edges": g.edges.len(),
        "recipes": g.recipes.len(),
        "domains": g.domains.len(),
        "accepted_at": c.accepted_at.0,
        "ready_at": c.ready_at.0,
        "windows_completed": c.windows_completed,
        "strikes": c.strikes,
        "edited": c.edited,
        "last_window": c.last.as_ref().map(|w| window(iv, w)),
        "passed_window": c.passed.as_ref().map(|w| window(iv, w)),
        "sections": sections(&c.sections),
        "sections_cap": dg.sections_cap,
        "away": dg.away,
        "veto": c.district_failing().iter().map(|g| iv.good_name(*g)).collect::<Vec<_>>(),
        "extra_rows": c.extra_rows(),
        "unit_rows": c.rows_len(),
        "units": units(iv, dg, c),
        "replay_units": dg.replay.as_ref().filter(|_| c.units.active()).map(|rp| json!({
            "replaying": rp.units_on().iter().filter(|&&x| x).count(),
            "frozen": rp.frozen().iter().filter(|&&x| x).count(),
            "freezes": rp.freezes(),
            "gated_nodes": rp.gated().iter().filter(|&&x| x).count(),
        })),
    });
    if graph {
        let mut goods = BTreeSet::new();
        for e in &g.edges {
            goods.extend(e.filter.iter().copied());
        }
        for r in &g.recipes {
            goods.extend(r.inputs.iter().chain(&r.outputs).map(|t| t.good));
        }
        compile["end"] = serde_json::to_value(g.end).expect("serialize");
        compile["graph_nodes"] = serde_json::to_value(&g.nodes).expect("serialize");
        compile["graph_edges"] = serde_json::to_value(&g.edges).expect("serialize");
        compile["graph_recipes"] = serde_json::to_value(&g.recipes).expect("serialize");
        compile["graph_domains"] = serde_json::to_value(&g.domains).expect("serialize");
        // each node's section by index, null for none (an idle or virtual node, a shared store, or a
        // store no section touches); an interior store has its section's
        compile["section_of"] = c
            .sections
            .section_of
            .iter()
            .map(|&s| if s == NONE { Value::Null } else { json!(s) })
            .collect::<Vec<_>>()
            .into();
        compile["goods"] = goods
            .into_iter()
            .map(|gd: Good| (gd.0.to_string(), json!(iv.good_name(gd))))
            .collect::<serde_json::Map<_, _>>()
            .into();
    }
    v["compile"] = compile;
    v
}

/// A compile's judging units (SECTIONS.md §3.4, slice S2b): none listed with fewer than two, as the
/// district's window is then the one unit's. With the glass replay on, whether each replays or a
/// fallback froze it (§3.5, slice S2c).
fn units(iv: &IndustryView, dg: &DistrictGlass, c: &Compiled) -> Value {
    let u = &c.units;
    let on = dg
        .replay
        .as_ref()
        .map(|rp| rp.units_on())
        .unwrap_or_default();
    let frozen = dg
        .replay
        .as_ref()
        .map(|rp| rp.frozen().to_vec())
        .unwrap_or_default();
    (0..c.unit_state.len())
        .map(|i| {
            let st = &c.unit_state[i];
            json!({
                "sections": u.sections[i],
                "stores": u.stores[i],
                "chunks": u.chunks[i],
                "unbounded": u.unbounded[i],
                "ready_at": st.ready_at.0,
                "window": c.unit_window(i as u16).map(|(s, n)| json!({"start": s.0, "samples": n})),
                "last_window": st.last.as_ref().map(|w| window(iv, w)),
                "passed": st.passed.map(|(s, t)| json!({"start": s.0, "ticks": t})),
                "windows_completed": st.windows_completed,
                "strikes": st.strikes,
                "edited": st.edited,
                "derated_at": st.derated_at.map(|t| t.0),
                "touched": st.touched.iter().map(|g| iv.good_name(*g)).collect::<Vec<_>>(),
                "may_replay": c.unit_may_replay(i as u16),
                "replaying": on.get(i).copied().unwrap_or(false),
                "frozen": frozen.get(i).copied().unwrap_or(false),
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn window(iv: &IndustryView, w: &WindowResult) -> Value {
    json!({
        "start": w.start.0,
        "ticks": w.ticks,
        "verdict": match w.verdict {
            1 => "pass",
            2 => "fail",
            _ => "none",
        },
        "goods": w.goods.iter().map(|r| json!({
            "good": iv.good_name(r.good),
            "class": r.class,
            "predicted": r.predicted,
            "lo": r.lo,
            "hi": r.hi,
            "measured": r.measured,
        })).collect::<Vec<_>>(),
    })
}

fn print_text(v: &Value) {
    let ds = v["districts"].as_array().cloned().unwrap_or_default();
    println!(
        "{}: industry at tick {}; {} districts with glass state",
        v["world"].as_str().unwrap_or("?"),
        v["now"],
        ds.len()
    );
    for d in &ds {
        let name = d["name"].as_str().unwrap_or("?");
        if let Some(why) = d["dropped"].as_str() {
            println!(
                "  district {} '{name}': page dropped at a restore: {why}",
                d["district"]
            );
            continue;
        }
        let c = &d["compile"];
        let ticking = d["ticking_chunks"].as_array().map_or(0, Vec::len);
        if c.is_null() {
            println!(
                "  district {} '{name}': no compile; replay {}; {ticking} ticking chunks{}",
                d["district"],
                d["replay"].as_str().unwrap_or("?"),
                if d["not_compilable"].is_null() {
                    String::new()
                } else {
                    format!("; not compilable {}", d["not_compilable"])
                }
            );
            continue;
        }
        let verdict = |w: &Value| {
            if w.is_null() {
                "none".to_string()
            } else {
                format!(
                    "{} at {} ({} ticks)",
                    w["verdict"].as_str().unwrap_or("?"),
                    w["start"],
                    w["ticks"]
                )
            }
        };
        println!(
            "  district {} '{name}': compile {} ({} nodes, {} bounded, {} edges, {} recipes, {} domains), {} windows, last {}, passed {}; replay {}{}; {ticking} ticking chunks",
            d["district"],
            c["compile"],
            c["nodes"],
            c["bounded_nodes"],
            c["edges"],
            c["recipes"],
            c["domains"],
            c["windows_completed"],
            verdict(&c["last_window"]),
            verdict(&c["passed_window"]),
            d["replay"].as_str().unwrap_or("?"),
            d["fallback"]
                .as_str()
                .map_or_else(String::new, |f| format!(" (fell back: {f})")),
        );
        let sc = &c["sections"];
        // judging groups of more than one section (each other section is judged alone)
        let groups = sc["groups"].as_array().map_or(0, |g| {
            g.iter()
                .filter(|x| x.as_array().is_some_and(|m| m.len() > 1))
                .count()
        });
        let shared = sc["shared_stores"].as_array().map_or(0, Vec::len);
        println!(
            "    {} sections ({} merged by the cap), {groups} shared judging groups, {shared} shared stores",
            sc["count"], sc["merged"]
        );
        for (i, x) in sc["list"].as_array().into_iter().flatten().enumerate() {
            println!(
                "    section {i}: {} nodes, {} stores{}, low {}, box {}, fingerprint {}",
                x["nodes"].as_array().map_or(0, Vec::len),
                x["stores"].as_array().map_or(0, Vec::len),
                if x["working"].as_bool() == Some(true) {
                    ", working"
                } else {
                    ", storage"
                },
                x["low"],
                x["box"],
                x["fingerprint"].as_str().unwrap_or("?"),
            );
        }
        let us = c["units"].as_array().cloned().unwrap_or_default();
        if !us.is_empty() || !c["sections_cap"].is_null() {
            println!(
                "    {} judging units, {} extra rows ({} open), cut to {} sections by the row caps",
                us.len(),
                c["extra_rows"],
                c["unit_rows"],
                if c["sections_cap"].is_null() {
                    "no fewer".to_string()
                } else {
                    c["sections_cap"].to_string()
                }
            );
            let ru = &c["replay_units"];
            if !ru.is_null() {
                println!(
                    "    replay by unit: {} of {} units replay, {} frozen by fallbacks ({} freezes), {} nodes gated off",
                    ru["replaying"],
                    us.len(),
                    ru["frozen"],
                    ru["freezes"],
                    ru["gated_nodes"]
                );
            }
            let veto = c["veto"].as_array().cloned().unwrap_or_default();
            if !veto.is_empty() {
                let names: Vec<&str> = veto.iter().filter_map(|g| g.as_str()).collect();
                println!(
                    "    the district's last window failed on {}: no unit touching them replays",
                    names.join(", ")
                );
            }
        }
        for (i, u) in us.iter().enumerate() {
            let open = &u["window"];
            println!(
                "    unit {i}: sections {}, {} stores; window {}; {} windows, last {}, passed {}; {} strikes{}{}{}{}",
                u["sections"],
                u["stores"].as_array().map_or(0, Vec::len),
                if open.is_null() {
                    format!("none (ready at {})", u["ready_at"])
                } else {
                    format!("from {} ({} samples)", open["start"], open["samples"])
                },
                u["windows_completed"],
                verdict(&u["last_window"]),
                if u["passed"].is_null() {
                    "none".to_string()
                } else {
                    format!(
                        "at {} ({} ticks)",
                        u["passed"]["start"], u["passed"]["ticks"]
                    )
                },
                u["strikes"],
                if u["edited"].as_bool() == Some(true) {
                    ", stale (edited)"
                } else {
                    ""
                },
                match u["derated_at"].as_u64() {
                    Some(t) => format!(", derated at {t}"),
                    None => String::new(),
                },
                if u["may_replay"].as_bool() == Some(true) {
                    ", may replay"
                } else {
                    ""
                },
                if u["frozen"].as_bool() == Some(true) {
                    ", frozen"
                } else if u["replaying"].as_bool() == Some(true) {
                    ", replaying"
                } else {
                    ""
                },
            );
        }
    }
}
