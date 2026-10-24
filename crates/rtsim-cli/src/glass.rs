// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli glass <world> [district] [--json | --graph]`: the glass box's paged state of a saved
//! world (SECTIONS.md §8.2, slice S1), read from the confirmed save through the read-only open, as
//! `rtsim-cli lines` reads train lines. Per district: its compile (records and counts), what
//! replays it, its last and its passed window, and its ticking chunks. `--graph` writes the
//! compile's records for offline checks (§9.4); slice S2 adds each node's section.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use rtsim_industry::glass::check::{DistrictGlass, WindowResult};
use rtsim_industry::glass::page::read_district_page;
use rtsim_industry::glass::report::replay_model;
use rtsim_industry::ids::Good;
use rtsim_industry::persist::GLASS_TABLE;
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
        let dg = read_district_page(&b).map_err(|e| format!("glass page {id}: {e}"))?;
        districts.push(district(iv, &saved.names, id, &dg, graph));
    }
    Ok(json!({
        "world": saved.scrub(&world.display().to_string()),
        "now": iv.now,
        "districts": districts,
    }))
}

fn district(iv: &IndustryView, names: &Names, id: u64, dg: &DistrictGlass, graph: bool) -> Value {
    let name = iv
        .districts
        .iter()
        .find(|d| d.id == id)
        .and_then(|d| names.get(DICT_NAME, d.name));
    let mut v = json!({
        "district": id,
        "name": name,
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
    });
    if graph {
        let mut goods = BTreeSet::new();
        for e in &g.edges {
            goods.extend(e.filter.iter().copied());
        }
        for r in &g.recipes {
            goods.extend(r.inputs.iter().chain(&r.outputs).map(|t| t.good));
        }
        compile["end"] = serde_json::to_value(&g.end).expect("serialize");
        compile["graph_nodes"] = serde_json::to_value(&g.nodes).expect("serialize");
        compile["graph_edges"] = serde_json::to_value(&g.edges).expect("serialize");
        compile["graph_recipes"] = serde_json::to_value(&g.recipes).expect("serialize");
        compile["graph_domains"] = serde_json::to_value(&g.domains).expect("serialize");
        compile["goods"] = goods
            .into_iter()
            .map(|gd: Good| (gd.0.to_string(), json!(iv.good_name(gd))))
            .collect::<serde_json::Map<_, _>>()
            .into();
    }
    v["compile"] = compile;
    v
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
    }
}
