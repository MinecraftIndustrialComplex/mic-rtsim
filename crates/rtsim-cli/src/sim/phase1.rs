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

use super::trace::print_trace;
use serde::Serialize;

pub struct Options {
    pub days: u64,
    pub seed: u64,
    pub script: bool,
    pub check: bool,
    pub out: Option<PathBuf>,
    pub every: u64,
    /// Days at the end whose market events are broken down by cause (0: none).
    pub trace_days: u64,
    /// The markets' decision rules (`Rules::parse`); `None` for the defaults.
    pub rules: Option<rtsim_trade::Rules>,
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
    caravan_sizes: SizeStats,
    features: Features,
}

/// What the round 2 features did in the run: bands, sorcerers, growth, the base (P16, P17, P30).
#[derive(Serialize, Default)]
struct Features {
    bands: Vec<String>,
    band_packets: u64,
    sentiment_snapshots: usize,
    last_sentiment: BTreeMap<String, Vec<i8>>,
    sorcerers: BTreeMap<String, String>,
    houses: Vec<String>,
    residents: usize,
    populations: BTreeMap<String, u32>,
    base_stops: Vec<String>,
    visible_at_base: usize,
    base_trade: Option<String>,
}

/// The cargo of every traveller on the road, sampled once a day after day 20 (so the first fill
/// is past), in chest slots (items over stack size; coin counts too). Scouts (travellers with no
/// cargo, carrying only news) are counted apart.
#[derive(Serialize, Default)]
struct SizeStats {
    samples: usize,
    scouts: usize,
    p10_slots: f64,
    p50_slots: f64,
    p90_slots: f64,
    max_slots: f64,
    /// Share of caravans carrying less than one slot.
    under_one_slot: f64,
    /// Share of caravans carrying less than one item of anything.
    under_one_item: f64,
    /// Median of the biggest single good a caravan carries, in items.
    p50_largest_items: f64,
}

fn sizes(w: &World, out: &mut Vec<(f64, f64)>) {
    for (k, e) in w.edges.iter().enumerate() {
        for dir in 0..2u8 {
            let _ = e;
            for t in w.travellers(k as u32, dir) {
                let slots: f64 = t
                    .cargo
                    .iter()
                    .map(|(g, a)| {
                        *a as f64 / MILLI as f64 / f64::from(w.goods.get(*g).stack.max(1))
                    })
                    .sum();
                let largest = t.cargo.values().copied().max().unwrap_or(0) as f64 / MILLI as f64;
                out.push((slots, largest));
            }
        }
    }
}

fn size_stats(v: Vec<(f64, f64)>) -> SizeStats {
    let scouts = v.iter().filter(|x| x.0 <= 0.0).count();
    let mut v: Vec<(f64, f64)> = v.into_iter().filter(|x| x.0 > 0.0).collect();
    if v.is_empty() {
        return SizeStats {
            scouts,
            ..SizeStats::default()
        };
    }
    let n = v.len();
    let pick = |s: &Vec<f64>, q: f64| s[((s.len() - 1) as f64 * q).round() as usize];
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let slots: Vec<f64> = v.iter().map(|x| x.0).collect();
    let mut largest: Vec<f64> = v.iter().map(|x| x.1).collect();
    largest.sort_by(|a, b| a.total_cmp(b));
    SizeStats {
        samples: n,
        scouts,
        p10_slots: pick(&slots, 0.1),
        p50_slots: pick(&slots, 0.5),
        p90_slots: pick(&slots, 0.9),
        max_slots: slots[n - 1],
        under_one_slot: slots.iter().filter(|&&s| s < 1.0).count() as f64 / n as f64,
        under_one_item: largest.iter().filter(|&&s| s < 1.0).count() as f64 / n as f64,
        p50_largest_items: pick(&largest, 0.5),
    }
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

#[allow(clippy::needless_range_loop)]
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

/// Walkthrough J, steps 2 and 3: a caravan whose road stops at the player's farm, near enough to
/// see; the player sells it what it buys, or buys from it, and the price becomes the farm's news.
fn base_trade(w: &mut World, player: ActorId, farm: usize, features: &mut Features) {
    let pos = w.nodes[farm].pos;
    w.set_player_pos(player, pos, w.now());
    let seen = w.visible_to(player);
    features.visible_at_base = features.visible_at_base.max(seen.len());
    for v in seen {
        let rtsim_world::Visible::Traveller { traveller, .. } = v else {
            continue;
        };
        if !traveller.stops.contains(&farm) {
            continue;
        }
        let r = rtsim_world::bases::CaravanRef::Sampled(traveller.key);
        let offers = w.caravan_offers(r, player);
        let pick = offers
            .iter()
            .find(|o| o.side == rtsim_trade::OfferSide::MarketBuys)
            .map(|o| (o.good, Side::Sell))
            .or_else(|| offers.first().map(|o| (o.good, Side::Buy)));
        if let Some((g, side)) = pick
            && let Ok((_, q)) = w.caravan_trade(r, Some(farm), player, g, side, 1)
        {
            let text = format!(
                "day {:.2}: the player {} {} {} for {} coins with a caravan {} -> {}; the farm's price is now news",
                day_of(w.now()),
                if side == Side::Sell { "sells" } else { "buys" },
                q.items,
                w.goods.get(g).name,
                q.coins,
                NAMES[traveller.from],
                NAMES[traveller.to]
            );
            println!("\n>> {text}");
            features.base_trade = Some(text);
            return;
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
    if let Some(r) = o.rules {
        cfg.trade.rules = r;
    }
    let (mut w, ids) = synth::build(cfg, true);
    if o.trace_days > 0 {
        w.enable_trace();
    }
    let player: ActorId = w.player(0xB0B);
    let mut series = Vec::new();
    let mut script = Vec::new();
    let mut tracked: Vec<Tracked> = Vec::new();
    let mut gossip = Vec::new();
    let mut prev_events = 0;
    let mut caravans: Vec<(f64, f64)> = Vec::new();
    let mut features = Features::default();
    for b in 0..w.bands.len() {
        let band = &w.bands[b];
        features.bands.push(format!(
            "{}'s band: {} (circuit of {:.1} days)",
            NAMES[band.home],
            band.route
                .iter()
                .map(|&i| NAMES[i])
                .collect::<Vec<_>>()
                .join(" -> "),
            band.period() as f64 / w.cfg.trade.quantum.per_day_count() as f64
        ));
    }
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
    let farm = 10;
    let fernwick = 4;
    let q = w.cfg.trade.quantum.len();
    for day in 0..=o.days {
        // Step a quantum at a time so news spread is timed to the hour.
        let start = Tick::from_days(day.saturating_sub(1)).0;
        let mut t = if day == 0 { 0 } else { start + q };
        while t <= Tick::from_days(day).0 {
            w.run_until(Tick(t));
            for tr in &mut tracked {
                track(&w, tr);
            }
            if o.script && day > 30 && features.base_trade.is_none() {
                base_trade(&mut w, player, farm, &mut features);
            }
            t += q;
        }
        if o.script {
            match day {
                10 => {
                    // Walkthrough B: dump wheat at Brindle until its treasury refuses.
                    let before = w.market(brindle);
                    let (t0, p0) = (before.treasury(), before.price(ids.wheat));
                    // As a player would in game: take the farmer's offer, a bundle of wheat
                    // for one emerald (PLAN.md §8.5), again and again until it shows crossed out.
                    let mut sold = 0;
                    let mut got = 0;
                    let mut refusal = None;
                    let mut bundles = Vec::new();
                    for _ in 0..200 {
                        let offer = w.offers(brindle, player).into_iter().find(|o| {
                            o.0 == ids.wheat && o.1 == rtsim_trade::OfferSide::MarketBuys
                        });
                        let Some((_, _, o)) = offer else {
                            refusal = Some("offer crossed out".to_string());
                            break;
                        };
                        match w.trade(brindle, player, ids.wheat, o.items, Side::Sell, w.now()) {
                            Ok(q) => {
                                sold += q.items;
                                got += q.coins;
                                if bundles.last() != Some(&q.items) {
                                    bundles.push(q.items);
                                }
                            }
                            Err(r) => {
                                refusal = Some(format!("{r:?}"));
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
                    d.insert(
                        "treasury after".into(),
                        format!("{:.3}", coins(after.treasury())),
                    );
                    d.insert("wheat price before".into(), format!("{:.4}", price(p0)));
                    d.insert(
                        "wheat price after".into(),
                        format!("{:.4}", price(after.price(ids.wheat))),
                    );
                    d.insert("stopped by".into(), refusal.unwrap_or_default());
                    d.insert(
                        "wheat per emerald, bundle by bundle".into(),
                        format!("{bundles:?}"),
                    );
                    println!("\n>> day 10: player dumps wheat at Brindle: {d:?}");
                    script.push(ScriptRow {
                        day: day_of(w.now()),
                        what: "dump wheat".into(),
                        detail: d,
                    });
                }
                20 => {
                    // Walkthrough D: rob a caravan leaving Oakford. Caravans wait at the dock for
                    // a full load or two days (E53), so wait for the next one out of town.
                    let mut key = pick_caravan(&w, oakford);
                    let mut t = w.now();
                    while key.is_none() && t < Tick::from_days(24) {
                        t += q;
                        w.run_until(t);
                        key = pick_caravan(&w, oakford);
                    }
                    if let Some(key) = key
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
                30 => {
                    // Walkthrough J: the player's farm becomes a base (P30's recommendation): the
                    // bridge's dwell counter marked it.
                    let pos = w.nodes[farm].pos;
                    w.mark_base(w.nodes[farm].key, pos, player, w.now());
                    let stops: Vec<String> = w
                        .edges
                        .iter()
                        .filter(|e| e.stops.contains(&farm))
                        .map(|e| format!("{} - {}", NAMES[e.a], NAMES[e.b]))
                        .collect();
                    println!(
                        "\n>> day 30: the player's farm is a base; caravans on {stops:?} stop there"
                    );
                    features.base_stops = stops;
                }
                35 => {
                    // Walkthrough F, step 2: the player robs a ribbit caravan out of Mirewater, and
                    // the news reaches the ribbit villages with their caravans and bands.
                    let mirewater = 6;
                    let mut key = pick_caravan(&w, mirewater);
                    let mut t = w.now();
                    while key.is_none() && t < Tick::from_days(39) {
                        t += q;
                        w.run_until(t);
                        key = pick_caravan(&w, mirewater);
                    }
                    if let Some(key) = key
                        && let Some(c) = w.promote(key)
                    {
                        let car = w.caravans.get(c).expect("promoted");
                        let (from, to) = (car.leg.from, car.leg.to);
                        let attack = w.rob(c, player, 1000).expect("robbable");
                        let mut d = BTreeMap::new();
                        d.insert("road".into(), format!("{} -> {}", NAMES[from], NAMES[to]));
                        d.insert("taken".into(), cargo_text(&w, &attack.taken));
                        println!("\n>> day 35: player robs a ribbit caravan: {d:?}");
                        script.push(ScriptRow {
                            day: day_of(w.now()),
                            what: "rob ribbit caravan".into(),
                            detail: d,
                        });
                        tracked.push(Tracked {
                            name: format!("robbery on {} -> {}", NAMES[from], NAMES[to]),
                            report: attack.report,
                            happened: w.now(),
                            learned: BTreeMap::new(),
                        });
                    }
                }
                45 => {
                    // Walkthrough F: the sorcerers of the ribbit villages decide whether to buff
                    // the player.
                    for (i, n) in w.nodes.iter().enumerate() {
                        if matches!(
                            n.kind,
                            rtsim_settle::NodeKind::Village(rtsim_settle::Culture::Ribbit)
                        ) {
                            let s = w.knowledge.sentiment(n.actor, player, w.now());
                            let buffs = w.sorcerer_includes(i, player);
                            features.sorcerers.insert(
                                NAMES[i].to_string(),
                                format!("sentiment {s}, buffs the player: {buffs}"),
                            );
                        }
                    }
                    println!("\n>> day 45: sorcerers: {:?}", features.sorcerers);
                }
                25 => {
                    // Walkthrough I: rescue a caravan leaving Fernwick.
                    if let Some(key) =
                        pick_caravan(&w, fernwick).or_else(|| pick_caravan(&w, brindle))
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
                Command::Sentiment(snap) => {
                    features.sentiment_snapshots += 1;
                    if let Some(i) = w.node_of_key(snap.village) {
                        features.last_sentiment.insert(
                            NAMES[i].to_string(),
                            snap.entries.iter().map(|e| e.2).collect(),
                        );
                    }
                }
                Command::PlaceHouse(h) => {
                    // The bridge would place it when its chunk next loads; here it stands at once.
                    let village = w.node_of_key(h.village).map_or("?", |i| NAMES[i]);
                    features.houses.push(format!(
                        "day {:.2}: {village} books house template {} (op {})",
                        day_of(w.now()),
                        h.template,
                        h.op
                    ));
                    features.residents += w.house_placed(h.op);
                }
                Command::SpawnResident(_) => {}
            }
        }
        if day > 20 {
            sizes(&w, &mut caravans);
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
    println!(
        "conservation: {conservation}; events per day over the last 10 days: {last10:.1} ({:.2} per market)",
        last10 / w.nodes.len() as f64
    );
    features.band_packets = w.stats.band_packets;
    for (i, n) in w.nodes.iter().enumerate() {
        features
            .populations
            .insert(NAMES[i].to_string(), n.local.pop);
    }
    println!("\n== round 2 features ==");
    for b in &features.bands {
        println!("{b}");
    }
    println!(
        "base stops {:?}; most travellers a player at the base saw at once: {}; {}",
        features.base_stops,
        features.visible_at_base,
        features
            .base_trade
            .as_deref()
            .unwrap_or("no caravan traded at the base")
    );
    println!(
        "news packets carried by bands: {}; sentiment snapshots sent: {}; houses booked: {} ({} residents)",
        features.band_packets,
        features.sentiment_snapshots,
        features.houses.len(),
        features.residents
    );
    for h in &features.houses {
        println!("  {h}");
    }
    let caravan_sizes = size_stats(caravans);
    println!(
        "caravans on the road (daily samples after day 20): {} with cargo, {} scouts; slots p10 {:.3}, median {:.3}, p90 {:.3}, max {:.2}; under one slot {:.0}%, nothing over one item {:.0}%; median biggest good {:.2} items",
        caravan_sizes.samples,
        caravan_sizes.scouts,
        caravan_sizes.p10_slots,
        caravan_sizes.p50_slots,
        caravan_sizes.p90_slots,
        caravan_sizes.max_slots,
        caravan_sizes.under_one_slot * 100.0,
        caravan_sizes.under_one_item * 100.0,
        caravan_sizes.p50_largest_items
    );
    if o.trace_days > 0 {
        print_trace(&w, o.trace_days, &|i| NAMES[i].to_string());
    }
    let report = Report {
        schema: 2,
        seed: o.seed,
        days: o.days,
        quantum_ticks: w.cfg.trade.quantum.len(),
        nodes: NAMES.iter().map(|s| s.to_string()).collect(),
        goods: w.goods.iter().map(|(_, d)| d.name.clone()).collect(),
        edges: w
            .edges
            .iter()
            .map(|e| {
                (
                    NAMES[e.a].to_string(),
                    NAMES[e.b].to_string(),
                    e.length_blocks,
                )
            })
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
        caravan_sizes,
        features,
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
