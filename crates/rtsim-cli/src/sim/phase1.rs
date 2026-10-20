// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim phase1`: the phase 1 scenario, headless (PLAN.md §8).
//!
//! Builds the synthetic world (`rtsim_world::synth`: six human villages, three ribbit villages
//! beside a geode, a pillager haven raiding one road, a player base), runs it for months, and
//! prints and records prices, stocks, flows, treasuries, exchange rates and how far news has
//! spread. A scripted player dumps goods at one village, robs a caravan and rescues another
//! (walkthroughs B, D and I). Every number in the world is a PLACEHOLDER (P9, P18): the scenario
//! shows behaviour, not tuned values.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_knowledge::{ActorId, ReportId};
use rtsim_trade::{GoodId, MILLI, PRICE_ONE};
use rtsim_world::synth::{self, NAMES, SynthGoods};
use rtsim_world::{Command, Side, World, WorldConfig};
use serde::Serialize;

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub script: bool,
    pub check: bool,
    pub out: Option<PathBuf>,
    pub every: u64,
}

#[derive(Serialize)]
struct GoodRow {
    price: f64,
    stock: f64,
    target: f64,
}

#[derive(Serialize)]
struct MarketRow {
    name: &'static str,
    currency: Option<String>,
    treasury: f64,
    goods: BTreeMap<String, GoodRow>,
    /// Price of each foreign currency in home coins, at border markets.
    fx: BTreeMap<String, f64>,
}

#[derive(Serialize)]
struct FlowRow {
    from: &'static str,
    to: &'static str,
    good: String,
    per_day: f64,
}

#[derive(Serialize)]
struct DayRow {
    day: u64,
    market_events: u64,
    deliveries: u64,
    markets: Vec<MarketRow>,
    flows: Vec<FlowRow>,
}

#[derive(Serialize)]
struct ScriptRow {
    day: f64,
    what: String,
    detail: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Spread {
    report: String,
    happened_day: f64,
    /// Day each holder first knew it.
    learned: BTreeMap<String, f64>,
}

#[derive(Serialize)]
struct Report {
    schema: u32,
    seed: u64,
    days: u64,
    quantum_ticks: u64,
    nodes: Vec<String>,
    goods: Vec<String>,
    edges: Vec<(String, String, u64)>,
    lints: Vec<String>,
    series: Vec<DayRow>,
    script: Vec<ScriptRow>,
    spread: Vec<Spread>,
    vanilla_gossip: Vec<String>,
    stats: String,
    conservation: String,
    state_hash: String,
    final_events_per_day: f64,
}

fn coins(a: i64) -> f64 {
    a as f64 / MILLI as f64
}

fn price(p: i64) -> f64 {
    p as f64 / PRICE_ONE as f64
}

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn snapshot(w: &World, day: u64) -> DayRow {
    let mut markets = Vec::new();
    for i in 0..w.nodes.len() {
        let m = w.market(i);
        let mut goods = BTreeMap::new();
        let mut fx = BTreeMap::new();
        for (g, def) in w.goods.iter() {
            if w.goods.is_currency(g) {
                if Some(g) != m.currency && m.line(g).target > 0 {
                    fx.insert(def.name.clone(), price(m.price(g)));
                }
                continue;
            }
            goods.insert(
                def.name.clone(),
                GoodRow {
                    price: price(m.price(g)),
                    stock: coins(m.stock(g)),
                    target: coins(m.line(g).target),
                },
            );
        }
        markets.push(MarketRow {
            name: NAMES[i],
            currency: m.currency.map(|c| w.goods.get(c).name.clone()),
            treasury: coins(m.treasury()),
            goods,
            fx,
        });
    }
    let q = w.cfg.trade.quantum;
    let per_day = q.per_day_count() as f64;
    let mut flows = Vec::new();
    for e in &w.edges {
        for d in &e.dirs {
            for (g, p) in d.goods.iter().chain(d.pay.iter()) {
                let r = p.current_rate();
                if r > 0 {
                    flows.push(FlowRow {
                        from: NAMES[d.from],
                        to: NAMES[d.to],
                        good: w.goods.get(*g).name.clone(),
                        per_day: coins(r) * per_day,
                    });
                }
            }
            for (g, p) in &d.raid {
                let r = p.current_rate();
                if r > 0 {
                    flows.push(FlowRow {
                        from: NAMES[d.from],
                        to: "Pillager camp (raid)",
                        good: w.goods.get(*g).name.clone(),
                        per_day: coins(r) * per_day,
                    });
                }
            }
        }
    }
    DayRow {
        day,
        market_events: w.stats.market_events,
        deliveries: w.stats.deliveries,
        markets,
        flows,
    }
}

fn print_day(w: &World, ids: &SynthGoods, row: &DayRow, prev_events: u64) {
    println!(
        "\n== day {} == events so far {} ({} since last print), deliveries {}",
        row.day,
        row.market_events,
        row.market_events - prev_events,
        row.deliveries
    );
    println!(
        "{:<14} {:>9} {:>8} | {:>14} {:>14} {:>14} {:>14} | fx",
        "market", "treasury", "cur", "bread", "wheat", "iron", "leather"
    );
    let fmt = |m: &MarketRow, name: &str| {
        m.goods.get(name).map_or("-".to_string(), |g| {
            format!("{:>5.1}/{:<4.0}@{:.3}", g.stock, g.target, g.price)
        })
    };
    for m in &row.markets {
        let fx: Vec<String> = m.fx.iter().map(|(k, v)| format!("{k} {v:.4}")).collect();
        println!(
            "{:<14} {:>9.2} {:>8} | {:>14} {:>14} {:>14} {:>14} | {}",
            m.name,
            m.treasury,
            m.currency.as_deref().unwrap_or("-"),
            fmt(m, &w.goods.get(ids.bread).name),
            fmt(m, &w.goods.get(ids.wheat).name),
            fmt(m, &w.goods.get(ids.iron).name),
            fmt(m, &w.goods.get(ids.leather).name),
            fx.join(", ")
        );
    }
    let mut top: Vec<&FlowRow> = row.flows.iter().collect();
    top.sort_by(|a, b| b.per_day.total_cmp(&a.per_day));
    let shown: Vec<String> = top
        .iter()
        .take(10)
        .map(|f| format!("{}->{} {} {:.1}/d", f.from, f.to, f.good, f.per_day))
        .collect();
    println!("top flows: {}", shown.join("; "));
}

struct Tracked {
    name: String,
    report: ReportId,
    happened: Tick,
    learned: BTreeMap<String, f64>,
}

fn track(w: &World, t: &mut Tracked) {
    for (i, n) in w.nodes.iter().enumerate() {
        if !t.learned.contains_key(NAMES[i]) && w.knowledge.knows(n.actor, t.report) {
            t.learned.insert(NAMES[i].to_string(), day_of(w.now()));
        }
    }
}

/// The busiest road out of `from` right now, with the traveller on it carrying the most.
fn pick_caravan(w: &World, from: usize) -> Option<rtsim_travel::CaravanKey> {
    let mut best: Option<(i64, rtsim_travel::CaravanKey)> = None;
    for d in &w.nodes[from].out {
        for t in w.travellers(d.edge, d.dir) {
            let load: i64 = t
                .cargo
                .iter()
                .filter(|(g, _)| !w.goods.is_currency(**g))
                .map(|(_, a)| *a)
                .sum();
            if load > 0 && best.is_none_or(|b| load > b.0) && t.progress_milli < 700 {
                best = Some((load, t.key));
            }
        }
    }
    best.map(|b| b.1)
}

fn cargo_text(w: &World, cargo: &BTreeMap<GoodId, i64>) -> String {
    cargo
        .iter()
        .map(|(g, a)| format!("{} {:.2}", w.goods.get(*g).name, coins(*a)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn run(o: Options) -> i32 {
    let mut cfg = WorldConfig::new(o.seed);
    cfg.check_every_event = o.check;
    cfg.genesis = rtsim_trade::build::Genesis::Share(500);
    let (mut w, ids) = synth::build(cfg, true);
    let player: ActorId = w.player(0xB0B);
    let mut series = Vec::new();
    let mut script = Vec::new();
    let mut tracked: Vec<Tracked> = Vec::new();
    let mut gossip = Vec::new();
    let mut prev_events = 0;
    println!(
        "phase 1 scenario: {} nodes, {} edges, {} goods, seed {}, {} days{}",
        w.nodes.len(),
        w.edges.len(),
        w.goods.len(),
        o.seed,
        o.days,
        if o.script { ", scripted player" } else { "" }
    );
    for (a, b, len) in w
        .edges
        .iter()
        .map(|e| (NAMES[e.a], NAMES[e.b], e.length_blocks))
    {
        println!("  edge {a} - {b}: {len} blocks");
    }
    if !w.lints.is_empty() {
        println!("lints: {:?}", w.lints);
    }
    let brindle = 0;
    let oakford = 1;
    let fernwick = 4;
    for day in 0..=o.days {
        w.run_until(Tick::from_days(day));
        if o.script {
            match day {
                10 => {
                    // Walkthrough B: dump wheat at Brindle until its treasury refuses.
                    let before = w.market(brindle);
                    let (t0, p0) = (before.treasury(), before.price(ids.wheat));
                    let mut sold = 0;
                    let mut got = 0;
                    let mut refusal = None;
                    for _ in 0..100 {
                        match w.trade(brindle, player, ids.wheat, 32, Side::Sell, w.now()) {
                            Ok(q) => {
                                sold += q.items;
                                got += q.coins;
                            }
                            Err(r) => {
                                refusal = Some(r);
                                break;
                            }
                        }
                    }
                    let after = w.market(brindle);
                    let mut d = BTreeMap::new();
                    d.insert("market".into(), "Brindle".into());
                    d.insert("wheat sold".into(), sold.to_string());
                    d.insert("emeralds paid".into(), got.to_string());
                    d.insert("treasury before".into(), format!("{:.3}", coins(t0)));
                    d.insert("treasury after".into(), format!("{:.3}", coins(after.treasury())));
                    d.insert("wheat price before".into(), format!("{:.4}", price(p0)));
                    d.insert(
                        "wheat price after".into(),
                        format!("{:.4}", price(after.price(ids.wheat))),
                    );
                    d.insert("stopped by".into(), format!("{refusal:?}"));
                    println!("\n>> day 10: player dumps wheat at Brindle: {d:?}");
                    script.push(ScriptRow {
                        day: day_of(w.now()),
                        what: "dump wheat".into(),
                        detail: d,
                    });
                }
                20 => {
                    // Walkthrough D: rob a caravan leaving Oakford.
                    if let Some(key) = pick_caravan(&w, oakford)
                        && let Some(c) = w.promote(key)
                    {
                        let car = w.caravans.get(c).expect("promoted");
                        let (from, to) = (car.leg.from, car.leg.to);
                        let cargo = car.cargo();
                        let attack = w.rob(c, player, 1000).expect("robbable");
                        let mut d = BTreeMap::new();
                        d.insert("road".into(), format!("{} -> {}", NAMES[from], NAMES[to]));
                        d.insert("cargo".into(), cargo_text(&w, &cargo));
                        d.insert("taken".into(), cargo_text(&w, &attack.taken));
                        d.insert("trader".into(), format!("{:?}", attack.outcome));
                        println!("\n>> day 20: player robs a caravan: {d:?}");
                        script.push(ScriptRow {
                            day: day_of(w.now()),
                            what: "rob caravan".into(),
                            detail: d,
                        });
                        tracked.push(Tracked {
                            name: format!("robbery on {} -> {}", NAMES[from], NAMES[to]),
                            report: attack.report,
                            happened: w.now(),
                            learned: BTreeMap::new(),
                        });
                    } else {
                        println!("\n>> day 20: no caravan on the road out of Oakford to rob");
                    }
                }
                25 => {
                    // Walkthrough I: rescue a caravan leaving Fernwick.
                    if let Some(key) = pick_caravan(&w, fernwick).or_else(|| pick_caravan(&w, brindle))
                        && let Some(c) = w.promote(key)
                    {
                        let car = w.caravans.get(c).expect("promoted");
                        let (from, to) = (car.leg.from, car.leg.to);
                        let home = w.nodes[car.home].actor;
                        let before = w.knowledge.sentiment(home, player, w.now());
                        let report = w.rescue(c, player).expect("rescuable");
                        let mut d = BTreeMap::new();
                        d.insert("road".into(), format!("{} -> {}", NAMES[from], NAMES[to]));
                        d.insert("home sentiment before".into(), before.to_string());
                        println!("\n>> day 25: player rescues a caravan: {d:?}");
                        script.push(ScriptRow {
                            day: day_of(w.now()),
                            what: "rescue caravan".into(),
                            detail: d,
                        });
                        tracked.push(Tracked {
                            name: format!("rescue on {} -> {}", NAMES[from], NAMES[to]),
                            report,
                            happened: w.now(),
                            learned: BTreeMap::new(),
                        });
                    }
                }
                _ => {}
            }
        }
        for t in &mut tracked {
            track(&w, t);
        }
        for c in w.take_commands() {
            match c {
                Command::VanillaGossip(g) => {
                    let village = w
                        .nodes
                        .iter()
                        .position(|n| n.actor == g.village)
                        .map_or("?", |i| NAMES[i]);
                    gossip.push(format!(
                        "day {:.2}: {village}: {:?} {} for the player",
                        day_of(w.now()),
                        g.gossip,
                        g.amount
                    ));
                }
            }
        }
        let row = snapshot(&w, day);
        if day % o.every == 0 || day == o.days {
            print_day(&w, &ids, &row, prev_events);
            prev_events = row.market_events;
        }
        series.push(row);
    }
    // News spread, as of the end.
    println!("\n== news spread ==");
    let mut spread = Vec::new();
    for t in &tracked {
        let mut by_day: Vec<(&String, &f64)> = t.learned.iter().collect();
        by_day.sort_by(|a, b| a.1.total_cmp(b.1));
        println!(
            "{} (day {:.2}): {}",
            t.name,
            day_of(t.happened),
            by_day
                .iter()
                .map(|(n, d)| format!("{n} day {d:.2}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        spread.push(Spread {
            report: t.name.clone(),
            happened_day: day_of(t.happened),
            learned: t.learned.clone(),
        });
    }
    if o.script {
        let oak = w.nodes[oakford].actor;
        println!(
            "Oakford's sentiment toward the player at the end: {}",
            w.knowledge.sentiment(oak, player, w.now())
        );
    }
    println!("vanilla gossip writes: {}", gossip.len());
    for g in gossip.iter().take(12) {
        println!("  {g}");
    }
    let conservation = match w.check_conserved(w.now()) {
        Ok(()) => "ok".to_string(),
        Err(e) => e.to_string(),
    };
    let n = series.len();
    let last10 = if n > 10 {
        (series[n - 1].market_events - series[n - 11].market_events) as f64 / 10.0
    } else {
        0.0
    };
    println!("stats: {:?}", w.stats);
    println!("conservation: {conservation}; events per day over the last 10 days: {last10:.1}");
    let report = Report {
        schema: 1,
        seed: o.seed,
        days: o.days,
        quantum_ticks: w.cfg.trade.quantum.len(),
        nodes: NAMES.iter().map(|s| s.to_string()).collect(),
        goods: w.goods.iter().map(|(_, d)| d.name.clone()).collect(),
        edges: w
            .edges
            .iter()
            .map(|e| (NAMES[e.a].to_string(), NAMES[e.b].to_string(), e.length_blocks))
            .collect(),
        lints: w.lints.iter().map(|l| format!("{l:?}")).collect(),
        series,
        script,
        spread,
        vanilla_gossip: gossip,
        stats: format!("{:?}", w.stats),
        conservation: conservation.clone(),
        state_hash: format!("{:016x}", w.state_hash()),
        final_events_per_day: last10,
    };
    println!("state hash {}", report.state_hash);
    if let Some(dir) = &o.out {
        match write(dir, &report) {
            Ok(p) => println!("wrote {}", p.display()),
            Err(e) => {
                eprintln!("could not write the report: {e}");
                return 2;
            }
        }
    }
    if conservation == "ok" { 0 } else { 1 }
}

fn write(dir: &Path, r: &Report) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("phase1-seed{}-{}d.json", r.seed, r.days));
    let mut f = std::fs::File::create(&path)?;
    serde_json::to_writer_pretty(&mut f, r)?;
    writeln!(f)?;
    Ok(path)
}
