// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herd forage probe (docs/overnight-choices-equilibrium.md EQC479, EQC500 to EQC509), every
//! test `#[ignore]`d: herds founded as the world model founds them, run alone (no predator, no
//! player) for `HERDK_DAYS` (2,000) days, each range's K, N, starvation and forage yield over its
//! need printed.
//!
//! - `painted_world`: S11's every-kind paint (plains, savanna, forest, sea channels) and
//!   single-biome variants; `HERDK_ONLY` picks variants by name, `HERDK_NO_CHANNELS` drops the sea,
//!   `HERDK_SHARE` sets `FoundingTunables::forage_share`.
//! - `s2_ranges_grid`, `s2_ranges_grid_fenced`: S2's valley grid, open and closed outside its 56
//!   regions.
//! - `real_worlds`, `real_world_climates`: saved worlds (`HERDK_WORLDS=dir1:dir2`), their herds and
//!   their cells' temperatures by biome; `savanna_trace`: one savanna herd, sub-season by
//!   sub-season.
//!
//! `cargo test --release -p rtsim-view --test herdk_probe -- --ignored --nocapture --exact <name>`

use std::collections::BTreeMap;
use std::path::Path;

use rtsim_ecology::herds::climate::{Climate, SUB_SEASONS};
use rtsim_ecology::herds::forage::CELL_BLOCKS;
use rtsim_ecology::herds::habitat::{FoundingTunables, HabitatClimate, RegionPlan, plan_region};
use rtsim_ecology::herds::synth::{self, Valley};
use rtsim_ecology::herds::{HerdSpecies, HerdTunables, Herds, NewHerd};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_space::habitat::REGION_BLOCKS;
use rtsim_space::synth::{Painter, land, sea};
use rtsim_space::{RegionKey, SampledWorld, WorldHandle};
use rtsim_travel::legs::LegIndex;

const FOUNDING_SEED: u64 = 0x6d69_635f_666f_756e;

/// Mean over the year of the logistic's best yield, summed over the region's open forage cells:
/// the most a herd that stays in the region can eat a day at steady state.
fn region_msy(c: &dyn Climate, frame: u16, rx: i32, rz: i32) -> (f64, f64, u32) {
    let per = REGION_BLOCKS / CELL_BLOCKS;
    let (mut msy, mut cap, mut open) = (0.0, 0.0, 0u32);
    for dz in 0..per {
        for dx in 0..per {
            let (cx, cz) = (rx * per + dx, rz * per + dz);
            if !c.terrain(frame, cx, cz).is_finite() {
                continue;
            }
            open += 1;
            for s in 0..SUB_SEASONS {
                let r = c.forage(frame, cx, cz, s);
                cap += r.capacity / f64::from(SUB_SEASONS);
                if r.growth_per_day > 0.0 && r.capacity > 0.0 {
                    let g = r.growth_per_day - r.decay_per_day;
                    let h = r.growth_per_day / r.capacity;
                    if g > 0.0 {
                        msy += g * g / (4.0 * h) / f64::from(SUB_SEASONS);
                    }
                }
            }
        }
    }
    (msy, cap, open)
}

/// Sum K, sum N, starved, born and ranges, by habitat and species.
type Agg = (u64, u64, u64, u64, usize);

struct Row {
    name: String,
    label: String,
    species: HerdSpecies,
    k: u64,
    msy: f64,
    cap: f64,
    open: u32,
    temp: f32,
}

fn run_alone(title: &str, climate: Box<dyn Climate>, rows: &[Row], news: Vec<NewHerd>, days: u64) {
    let tun = HerdTunables::default();
    let mut h = Herds::new(0x6865_7264, tun.clone(), climate, Tick::ZERO);
    for n in news {
        h.add_herd(n, Tick::ZERO);
    }
    let mut legs = LegIndex::new(1_000);
    let mut marks: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for d in 1..=days {
        let t = Tick(d * TICKS_PER_DAY);
        h.run_until(&mut legs, t);
        let _ = h.take_commands();
        if d % 500 == 0 {
            for (id, x) in h.herds.iter() {
                marks
                    .entry(x.name.clone())
                    .or_default()
                    .push(h.count(id, t));
            }
        }
    }
    let t = Tick(days * TICKS_PER_DAY);
    println!("\n=== {title}: herds alone, no predators, {days} days ===");
    println!(
        "{:<26} {:<34} {:>6} {:>6} {:>6} {:>5} {:>8} {:>6} {:>5} {:>7} {:>7} {:>7}  N at 500d steps",
        "herd", "habitat", "sp", "K", "N", "N/K", "MSY/dmd", "cap", "open", "born", "starve", "sat"
    );
    let mut agg: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut far: Vec<f64> = Vec::new();
    for (id, x) in h.herds.iter() {
        let Some(r) = rows.iter().find(|r| r.name == x.name) else {
            continue;
        };
        let sp = tun.species(x.species);
        let n = h.count(id, t);
        let p = h.position(id, t);
        let away = (((p.0 - i64::from(x.home.0)).pow(2) + (p.1 - i64::from(x.home.1)).pow(2))
            as f64)
            .sqrt();
        far.push(away);
        let dmd = r.k as f64 * sp.intake_per_day;
        println!(
            "{:<26} {:<34} {:>6} {:>6} {:>6} {:>5.2} {:>8.2} {:>6.0} {:>5} {:>7} {:>7} {:>7.3}  {:?} ({:.0}C)",
            x.name,
            r.label,
            format!("{:?}", r.species),
            x.k,
            n,
            n as f64 / x.k.max(1) as f64,
            r.msy / dmd.max(1e-9),
            r.cap,
            r.open,
            x.ledger.born,
            x.ledger.starved,
            f64::from(x.satisfaction) / 1000.0,
            marks.get(&x.name).cloned().unwrap_or_default(),
            r.temp,
        );
        let e = agg
            .entry((r.label.clone(), format!("{:?}", x.species)))
            .or_default();
        e.0 += x.k;
        e.1 += n;
        e.2 += x.ledger.starved;
        e.3 += x.ledger.born;
        e.4 += 1;
    }
    println!("--- by habitat and species: ranges, sum K, sum N, N/K, starved/born");
    for ((l, s), (k, n, st, b, c)) in &agg {
        println!(
            "{l:<34} {s:<6} {c:>3} K {k:>6} N {n:>6} N/K {:.2} starved/born {:.2}",
            *n as f64 / (*k).max(1) as f64,
            *st as f64 / (*b).max(1) as f64
        );
    }
    let (mut k, mut n) = (0u64, 0u64);
    let mut by_sp: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for (id, x) in h.herds.iter() {
        k += x.k;
        n += h.count(id, t);
        let e = by_sp.entry(format!("{:?}", x.species)).or_default();
        e.0 += x.k;
        e.1 += h.count(id, t);
    }
    for (s, (k, n)) in &by_sp {
        println!(
            "species {s:<6} K {k:>6} N {n:>6} N/K {:.2}",
            *n as f64 / (*k).max(1) as f64
        );
    }
    far.sort_by(|a, b| a.total_cmp(b));
    if !far.is_empty() {
        println!(
            "blocks from home at the end: median {:.0}, max {:.0}",
            far[far.len() / 2],
            far[far.len() - 1]
        );
    }
    println!("ALL K {k} N {n} N/K {:.2}", n as f64 / k.max(1) as f64);
}

/// The land biomes of a region's known cells, most common first.
fn biome_mix(w: &SampledWorld, k: RegionKey) -> String {
    let per = REGION_BLOCKS / 64;
    let mut by: BTreeMap<String, u32> = BTreeMap::new();
    for cz in k.rz * per..(k.rz + 1) * per {
        for cx in k.rx * per..(k.rx + 1) * per {
            if let Some(h) = w.cell(k.frame, cx, cz) {
                let n = w
                    .biome_name(h.land_biome)
                    .trim_start_matches("minecraft:")
                    .to_string();
                *by.entry(n).or_default() += 1;
            }
        }
    }
    let mut v: Vec<(String, u32)> = by.into_iter().collect();
    v.sort_by_key(|e| std::cmp::Reverse(e.1));
    v.iter()
        .take(3)
        .map(|(n, c)| format!("{n}:{c}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Founds every judged region of `world` as the shipped engine does, and reports each.
fn found_world(world: &WorldHandle, label_by_biome: bool) -> (Vec<Row>, Vec<NewHerd>) {
    let climate = HabitatClimate::new(world.clone());
    let w = world.read();
    let s = w.sampled_ref().expect("a sampled world");
    let keys: Vec<RegionKey> = s.regions().map(|(k, _)| *k).collect();
    let (mut rows, mut news) = (Vec::new(), Vec::new());
    let (mut barren, mut notyet) = (0, 0);
    for k in keys {
        let mut ft = FoundingTunables::default();
        if let Some(v) = std::env::var("HERDK_SHARE")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            ft.forage_share = v;
        }
        match plan_region(&**w, &climate, k, FOUNDING_SEED, &ft) {
            RegionPlan::Found(n) => {
                let (msy, cap, open) = region_msy(&climate, k.frame, k.rx, k.rz);
                let (x0, z0, x1, z1) = k.rect();
                let a = w.area(k.frame, x0, z0, x1, z1);
                let mix = biome_mix(s, k);
                let label = if label_by_biome {
                    mix.split(':').next().unwrap_or("").to_string()
                } else {
                    format!(
                        "{mix} kn{:.2} ld{:.2} v{:.2}",
                        a.known_share(),
                        a.land_share(),
                        a.vegetation
                    )
                };
                rows.push(Row {
                    name: n.name.clone(),
                    label,
                    species: n.species,
                    k: n.count,
                    msy,
                    cap,
                    open,
                    temp: a.base_celsius,
                });
                news.push(n);
            }
            RegionPlan::Barren => barren += 1,
            RegionPlan::NotYet => notyet += 1,
        }
    }
    println!(
        "founded {} regions, barren {barren}, not yet {notyet}",
        news.len()
    );
    (rows, news)
}

fn is_channel(x: i32, z: i32) -> bool {
    let band = |v: i32, lo: i32| (lo..lo + 32).contains(&v);
    let across = (64..576).contains(&x) && [64, 544, 1_056].iter().any(|&lo| band(z, lo));
    let down = (64..1_088).contains(&z) && [64, 544].iter().any(|&lo| band(x, lo));
    across || down
}

/// S11's every-kind world's herd ranges (crates/rtsim-host/src/every.rs on feat/eq-s11).
fn painted(biomes: &[&'static str]) -> WorldHandle {
    painted_with(biomes, true)
}

fn painted_with(biomes: &[&'static str], channels: bool) -> WorldHandle {
    let mut w = SampledWorld::new();
    let mut p = Painter::default();
    let per = REGION_BLOCKS / 64;
    for rz in -3i32..=3 {
        for rx in -3i32..=4 {
            let biome = biomes[(rx + 3 + 2 * (rz + 3)).rem_euclid(biomes.len() as i32) as usize];
            for cz in rz * per..(rz + 1) * per {
                for cx in rx * per..(rx + 1) * per {
                    let r = p.record(
                        0,
                        cx,
                        cz,
                        &|x, z| {
                            if channels && is_channel(x, z) {
                                sea()
                            } else {
                                land(biome)
                            }
                        },
                        &[],
                    );
                    p.feed(&mut w, &r);
                }
            }
        }
    }
    WorldHandle::new(Box::new(w))
}

fn days() -> u64 {
    std::env::var("HERDK_DAYS")
        .ok()
        .and_then(|d| d.parse().ok())
        .unwrap_or(2_000)
}

#[test]
#[ignore]
fn painted_world() {
    let variants: Vec<(&str, Vec<&'static str>)> = vec![
        (
            "S11 painted: plains, savanna, forest",
            vec!["minecraft:plains", "minecraft:savanna", "minecraft:forest"],
        ),
        ("painted: plains only", vec!["minecraft:plains"]),
        (
            "painted: plains and savanna",
            vec!["minecraft:plains", "minecraft:savanna"],
        ),
        (
            "painted: plains and forest",
            vec!["minecraft:plains", "minecraft:forest"],
        ),
        ("painted: savanna only", vec!["minecraft:savanna"]),
        ("painted: forest only", vec!["minecraft:forest"]),
        ("painted: snowy slopes only", vec!["minecraft:snowy_slopes"]),
    ];
    for (title, b) in variants {
        if std::env::var("HERDK_ONLY").is_ok_and(|o| !title.contains(o.as_str())) {
            continue;
        }
        let world = painted_with(&b, std::env::var("HERDK_NO_CHANNELS").is_err());
        let (rows, news) = found_world(&world, true);
        run_alone(
            title,
            Box::new(HabitatClimate::new(world)),
            &rows,
            news,
            days(),
        );
    }
}

/// The valley closed outside the grid's 56 regions: the herds keep only their ranges' forage.
struct Fenced(Valley);

impl Climate for Fenced {
    fn forage(
        &self,
        f: u16,
        cx: i32,
        cz: i32,
        sub: u32,
    ) -> rtsim_ecology::herds::climate::ForageRates {
        self.0.forage(f, cx, cz, sub)
    }
    fn temperature(&self, f: u16, cx: i32, cz: i32, sub: u32) -> f32 {
        self.0.temperature(f, cx, cz, sub)
    }
    fn terrain(&self, f: u16, cx: i32, cz: i32) -> f32 {
        if (-12..20).contains(&cx) && (-12..16).contains(&cz) {
            self.0.terrain(f, cx, cz)
        } else {
            f32::INFINITY
        }
    }
}

#[test]
#[ignore]
fn s2_ranges_grid_fenced() {
    s2_grid(true);
}

#[test]
#[ignore]
fn s2_ranges_grid() {
    s2_grid(false);
}

fn s2_grid(fenced: bool) {
    let h = synth::ranges(7, HerdTunables::default(), Tick::ZERO, -3..=4, -3..=3);
    let v = Valley::default();
    let mut rows = Vec::new();
    let mut news = Vec::new();
    for (_, x) in h.herds.iter() {
        let (msy, cap, open) = region_msy(&v, 0, x.range.1, x.range.2);
        rows.push(Row {
            name: x.name.clone(),
            label: format!("valley {}:{}", x.range.1, x.range.2),
            species: x.species,
            k: x.k,
            msy,
            cap,
            open,
            temp: 0.0,
        });
        news.push(NewHerd {
            name: x.name.clone(),
            species: x.species,
            frame: 0,
            pos: x.home,
            count: x.k,
            seed: None,
        });
    }
    if fenced {
        run_alone(
            "S2 ranges grid, valley closed outside the grid",
            Box::new(Fenced(v)),
            &rows,
            news,
            days(),
        );
    } else {
        run_alone("S2 ranges grid (valley)", Box::new(v), &rows, news, days());
    }
}

#[test]
#[ignore]
fn real_worlds() {
    let list = std::env::var("HERDK_WORLDS").unwrap_or_default();
    for dir in list.split(':').filter(|d| !d.is_empty()) {
        println!("\n##### {dir}");
        let saved = match rtsim_view::load::load_world(Path::new(dir), Default::default()) {
            Ok(s) => s,
            Err(e) => {
                println!("unreadable: {e}");
                continue;
            }
        };
        for m in &saved.modules {
            if m.name == "herds" || m.name == "world_model" {
                println!("module {} {:?} {}", m.name, m.status, m.detail);
            }
        }
        if let Some(h) = &saved.herds {
            let now = h.now();
            println!(
                "saved herds at day {:.1}: {}",
                now.0 as f64 / TICKS_PER_DAY as f64,
                h.herds.len()
            );
            for (id, x) in h.herds.iter() {
                println!(
                    "  saved {:<28} {:?} K {} N {} born {} starved {} eaten {} sat {:.3} range {:?}",
                    x.name,
                    x.species,
                    x.k,
                    h.count(id, now),
                    x.ledger.born,
                    x.ledger.starved,
                    x.ledger.eaten,
                    f64::from(x.satisfaction) / 1000.0,
                    x.range
                );
            }
        }
        let Some(world) = saved.world_model.clone() else {
            println!("no world model");
            continue;
        };
        let (rows, news) = found_world(&world, false);
        if news.is_empty() {
            continue;
        }
        run_alone(
            dir,
            Box::new(HabitatClimate::new(world)),
            &rows,
            news,
            days(),
        );
    }
}

#[test]
#[ignore]
fn real_world_climates() {
    let list = std::env::var("HERDK_WORLDS").unwrap_or_default();
    for dir in list.split(':').filter(|d| !d.is_empty()) {
        let Ok(saved) = rtsim_view::load::load_world(Path::new(dir), Default::default()) else {
            continue;
        };
        let Some(world) = saved.world_model.clone() else {
            continue;
        };
        let w = world.read();
        let Some(s) = w.sampled_ref() else { continue };
        let mut by: BTreeMap<String, (u32, f64, f64, u8, u8)> = BTreeMap::new();
        for (_, h) in s.regions().flat_map(|(_, r)| r.cells()) {
            let e = by
                .entry(s.biome_name(h.land_biome).to_string())
                .or_insert((0, 0.0, 0.0, 0, 0));
            e.0 += 1;
            e.1 += f64::from(rtsim_space::sampled::base_celsius(h));
            e.2 += f64::from(h.temperature);
            e.3 = h.season;
            e.4 = h.season_count;
        }
        println!("\n##### {dir}");
        for (b, (n, c, t, se, sc)) in by {
            println!(
                "  {b:<40} cells {n:>4} base {:>6.1} C  vanilla t {:.2}  season {se}/{sc}",
                c / f64::from(n),
                t / f64::from(n)
            );
        }
    }
}

#[test]
#[ignore]
fn savanna_trace() {
    let world = painted(&["minecraft:savanna"]);
    let climate = HabitatClimate::new(world.clone());
    let k = RegionKey {
        frame: 0,
        rx: 0,
        rz: 0,
    };
    let RegionPlan::Found(mut n) = plan_region(
        &**world.read(),
        &climate,
        k,
        3,
        &FoundingTunables::default(),
    ) else {
        panic!()
    };
    n.count = std::env::var("HERDK_COUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(n.count);
    let c = HabitatClimate::new(world.clone());
    println!(
        "growth at 56: {} ; yield {:.1}",
        c.growth_share(56.0),
        rtsim_ecology::herds::habitat::range_yield(&c, 0, 0, 0)
    );
    let mut h = Herds::new(1, HerdTunables::default(), Box::new(c), Tick::ZERO);
    let id = h.add_herd(n.clone(), Tick::ZERO);
    let mut legs = LegIndex::new(1_000);
    let subs = h.subs();
    for s in 1..=36u64 {
        let t = Tick(s * subs.len);
        h.run_until(&mut legs, t);
        let _ = h.take_commands();
        let x = h.herd(id).unwrap();
        let p = h.position(id, t);
        let cc = rtsim_ecology::herds::forage::cell_of(p.0, p.1);
        let f = h.forage.at(&*h.climate, &subs, (0, cc.0, cc.1), t);
        println!(
            "sub {s:>2}: {:?} N {} sat {:.3} pos {:?} forage here {:.0} footprint {}",
            n.species,
            h.count(id, t),
            f64::from(x.satisfaction) / 1000.0,
            p,
            f,
            h.footprint(id, t)
        );
    }
}
