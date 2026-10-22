// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli lines <world> [--json]`: what the industry engine learned about trains in a saved
//! world. Its line edges (INDUSTRY.md §8.8: trips, travel time and cargo per station pair) and each
//! district's station exchange rules (I15: what a station unloads and loads), read from the
//! confirmed save through the read-only open, as `rtsim-cli view` reads it. For the synthetic
//! stress world (docs/stress-worlds-results.md section 10, choice SG10), where no in-game command
//! lists them.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use rtsim_industry::district::District;
use rtsim_industry::ids::GoodVec;
use rtsim_industry::persist::{self, DISTRICT_TABLE};
use rtsim_kernel::rows::PageSource;
use rtsim_view::industry::IndustryView;
use rtsim_view::journal::Names;
use rtsim_view::keys::DICT_NAME;
use rtsim_view::{Limits, load_world};
use serde_json::{Value, json};

pub fn cli(args: &[&str]) -> ExitCode {
    let (world, as_json) = match args {
        [w] => (*w, false),
        [w, "--json"] | ["--json", w] => (*w, true),
        _ => {
            eprintln!("usage: rtsim-cli lines <world> [--json]");
            return ExitCode::from(64);
        }
    };
    match report(Path::new(world)) {
        Ok(v) => {
            if as_json {
                println!("{}", serde_json::to_string_pretty(&v).expect("serialize"));
            } else {
                print_text(&v);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rtsim-cli lines: {e}");
            ExitCode::FAILURE
        }
    }
}

fn report(world: &Path) -> Result<Value, String> {
    let saved = load_world(world, Limits::default())?;
    let Some(iv) = saved.industry.as_ref() else {
        return Err(format!("{} has no industry pages", world.display()));
    };
    let names = &saved.names;
    let station = |id: u64| -> Value {
        let decl = iv.stations.iter().find(|s| s.station.0 == id);
        json!({
            "id": id,
            "name": station_name(iv, names, id),
            "district": decl.and_then(|d| d.district).map(|d| d.0),
            "pos": decl.map(|d| [d.pos.x, d.pos.y, d.pos.z]),
        })
    };
    let goods = |v: &GoodVec, per: u64| -> Value {
        let mut m = serde_json::Map::new();
        for (g, n) in v.iter() {
            m.insert(iv.good_name(g), json!(n as f64 / per.max(1) as f64));
        }
        Value::Object(m)
    };
    let mut lines = Vec::new();
    for ((a, b), e) in &iv.lines {
        lines.push(json!({
            "from": station(*a),
            "to": station(*b),
            "trips": e.trips,
            "travel_ticks_mean": e.travel_ticks as f64 / e.trips.max(1) as f64,
            "cargo_total": goods(&e.cargo, 1),
            "cargo_per_trip": goods(&e.cargo, e.trips),
            "first": e.first.0,
            "last": e.last.0,
        }));
    }
    let mut exchange = Vec::new();
    let ids = saved
        .source
        .table_ids(DISTRICT_TABLE)
        .map_err(|e| format!("cannot list the district pages: {e}"))?;
    for id in ids {
        let b = saved
            .source
            .read(rtsim_kernel::page::PageKey::new(DISTRICT_TABLE, id))
            .map_err(|e| format!("district page {id}: {e}"))?
            .ok_or_else(|| format!("district page {id} is listed and missing"))?;
        let d: District =
            persist::from_bytes(&b).map_err(|e| format!("district page {id}: {e}"))?;
        for (s, rule) in &d.exchange {
            let names_of =
                |set: &std::collections::BTreeSet<rtsim_industry::ids::Good>| -> Vec<String> {
                    set.iter().map(|g| iv.good_name(*g)).collect()
                };
            exchange.push(json!({
                "district": d.id.0,
                "station": station(s.0),
                "unload": names_of(&rule.unload),
                "load": names_of(&rule.load),
                "observed": rule.observed,
            }));
        }
    }
    let with_trips = iv.lines.iter().filter(|(_, e)| e.trips > 0).count();
    let with_cargo = iv
        .lines
        .iter()
        .filter(|(_, e)| e.cargo.iter().next().is_some())
        .count();
    let both = exchange
        .iter()
        .filter(|r| {
            r["unload"].as_array().is_some_and(|a| !a.is_empty())
                && r["load"].as_array().is_some_and(|a| !a.is_empty())
        })
        .count();
    let mut per_district: BTreeMap<u64, usize> = BTreeMap::new();
    for s in &iv.stations {
        if let Some(d) = s.district {
            *per_district.entry(d.0).or_default() += 1;
        }
    }
    Ok(json!({
        "world": saved.scrub(&world.display().to_string()),
        "now": iv.now,
        "districts": iv.districts.len(),
        "stations": iv.stations.len(),
        "stations_in_a_district": per_district.values().sum::<usize>(),
        "summary": {
            "lines": iv.lines.len(),
            "lines_with_trips": with_trips,
            "lines_with_cargo": with_cargo,
            "exchange_rules": exchange.len(),
            "exchange_rules_loading_and_unloading": both,
        },
        "lines": lines,
        "exchange": exchange,
    }))
}

fn station_name(iv: &IndustryView, names: &Names, id: u64) -> String {
    iv.stations
        .iter()
        .find(|s| s.station.0 == id)
        .and_then(|s| names.get(DICT_NAME, s.name))
        .map_or_else(|| format!("station {id}"), str::to_owned)
}

fn print_text(v: &Value) {
    let s = &v["summary"];
    println!(
        "{}: industry at tick {}; {} districts, {} stations ({} in a district); {} lines ({} with trips, {} with cargo); {} exchange rules ({} load and unload)",
        v["world"].as_str().unwrap_or("?"),
        v["now"],
        v["districts"],
        v["stations"],
        v["stations_in_a_district"],
        s["lines"],
        s["lines_with_trips"],
        s["lines_with_cargo"],
        s["exchange_rules"],
        s["exchange_rules_loading_and_unloading"],
    );
    let name = |st: &Value| {
        format!(
            "{} (district {})",
            st["name"].as_str().unwrap_or("?"),
            st["district"]
                .as_u64()
                .map_or_else(|| "-".to_string(), |d| d.to_string())
        )
    };
    let goods = |g: &Value| {
        g.as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, n)| format!("{:.1} {k}", n.as_f64().unwrap_or(0.0)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    println!("lines (from -> to: trips, mean travel ticks, cargo per trip):");
    for l in v["lines"].as_array().into_iter().flatten() {
        println!(
            "  {} -> {}: {} trips, {:.0} ticks, {}",
            name(&l["from"]),
            name(&l["to"]),
            l["trips"],
            l["travel_ticks_mean"].as_f64().unwrap_or(0.0),
            goods(&l["cargo_per_trip"])
        );
    }
    println!("exchange rules (district, station: unload; load; observed stops):");
    for r in v["exchange"].as_array().into_iter().flatten() {
        let list = |k: &str| {
            r[k].as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        };
        println!(
            "  district {}, {}: unload {}; load {}; observed {}",
            r["district"],
            r["station"]["name"].as_str().unwrap_or("?"),
            list("unload"),
            list("load"),
            r["observed"]
        );
    }
}
