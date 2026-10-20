// SPDX-License-Identifier: AGPL-3.0-or-later
//! Diagnostics, not tests: `cargo test -p rtsim-world --test diag -- --ignored --nocapture`.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::WorldConfig;
use rtsim_world::synth;

#[test]
#[ignore]
fn node_detail() {
    let node: usize = std::env::var("NODE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9);
    let from_day: u64 = std::env::var("FROM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(110);
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(
        std::env::var("GENESIS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(500),
    );
    if let Ok(r) = std::env::var("RULES") {
        cfg.trade.rules = rtsim_trade::Rules::parse(&r).unwrap();
    }
    if std::env::var("NOGROWTH").is_ok() {
        cfg.growth = false;
    }
    let markets: usize = std::env::var("MARKETS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let (mut w, ids) = if markets == 0 {
        synth::build(cfg, true)
    } else {
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        (
            rtsim_world::World::build(cfg, goods, book, synth::grid(markets, 42)),
            ids,
        )
    };
    w.run_until(Tick::from_days(from_day));
    w.enable_trace();
    let names = std::env::var("GOODS").unwrap_or("wheat,bread,emerald,amethyst_shard".into());
    let goods: Vec<_> = names
        .split(',')
        .map(|n| w.goods.find(n).expect("good"))
        .collect();
    let _ = ids;
    let span: u64 = std::env::var("SPAN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let mut t = Tick::from_days(from_day).0;
    let mut seen = 0;
    while t < Tick::from_days(from_day + span).0 {
        t += 50;
        w.run_until(Tick(t));
        let tr = w.trace.as_ref().unwrap();
        while seen < tr.len() {
            let r = tr[seen].clone();
            seen += 1;
            if r.node != node {
                continue;
            }
            let m = w.market(node);
            let mut line = format!(
                "{:>8.3} {:?} ch{} news{} diffs{:?}",
                r.at.0 as f64 / TICKS_PER_DAY as f64,
                r.cause,
                r.changed,
                r.news as u8,
                r.diffs
                    .iter()
                    .map(|d| (w.goods.get(d.0).name.clone(), d.1, d.2, d.3, d.4))
                    .collect::<Vec<_>>()
            );
            for &g in &goods {
                let l = m.line(g);
                line += &format!(
                    "\n           {:<10} S {:>7} T {:>7} cap {:>5} cons {:>5} in {:>5} out {:>5} slack {:>5} adv {:>5} exp {:>5}",
                    w.goods.get(g).name,
                    l.stock,
                    l.target,
                    l.rates.cap,
                    l.rates.cons,
                    l.rates.inflow,
                    l.rates.outflow,
                    l.rates.slack.min(99999),
                    m.absorbable(g),
                    m.exportable(g)
                );
                // Who sends it here, and at what rate.
                for d in &w.nodes[node].out {
                    let e = &w.edges[d.edge as usize];
                    let din = &e.dirs[1 - d.dir as usize];
                    if let Some(p) = din.goods.get(&g)
                        && p.current_rate() > 0
                    {
                        line += &format!(" [from {} {}]", din.from, p.current_rate());
                    }
                    let dout = &e.dirs[d.dir as usize];
                    if let Some(p) = dout.goods.get(&g)
                        && p.current_rate() > 0
                    {
                        line += &format!(" [to {} {}]", dout.to, p.current_rate());
                    }
                }
            }
            println!("{line}");
        }
    }
}

#[test]
#[ignore]
fn robbery_spread() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let (mut w, _) = synth::build(cfg, true);
    let player = w.player(0xB0B);
    w.run_until(Tick::from_days(20));
    let mut key = None;
    let mut t = w.now();
    while key.is_none() {
        t += 1000;
        w.run_until(t);
        for d in w.nodes[1].out.clone() {
            for tr in w.travellers(d.edge, d.dir) {
                if tr.cargo.values().sum::<i64>() > 0 && tr.progress_milli < 700 {
                    key = Some(tr.key);
                }
            }
        }
    }
    let c = w.promote(key.unwrap()).unwrap();
    let a = w.rob(c, player, 1000).unwrap();
    println!("robbed at {:?}: {:?}", w.now(), a.outcome);
    for day in 0..10u64 {
        w.run_until(Tick(t.0 + day * TICKS_PER_DAY));
        let knows: Vec<usize> = (0..w.nodes.len())
            .filter(|&i| w.knowledge.knows(w.nodes[i].actor, a.report))
            .collect();
        let pend: Vec<String> = w.nodes[1]
            .out
            .iter()
            .map(|d| {
                let dir = &w.edges[d.edge as usize].dirs[d.dir as usize];
                format!(
                    "to {} pending {:?} period {}",
                    dir.to,
                    dir.pending,
                    dir.lattice.current_period()
                )
            })
            .collect();
        println!("day +{day}: knows {knows:?}; Oakford's roads: {pend:?}");
    }
}

#[test]
#[ignore]
fn sizes() {
    use std::mem::size_of;
    println!("Market {}", size_of::<rtsim_trade::Market>());
    println!("Line {}", size_of::<rtsim_trade::Line>());
    println!("Book {}", size_of::<rtsim_trade::line::Book>());
    println!("GoodState {}", size_of::<rtsim_trade::market::GoodState>());
    println!("Edge {}", size_of::<rtsim_travel::Edge>());
    println!("Dir {}", size_of::<rtsim_travel::Dir>());
    println!("Pipeline {}", size_of::<rtsim_travel::Pipeline>());
    println!("Node {}", size_of::<rtsim_world::world::Node>());
    println!("Snapshot {}", size_of::<rtsim_travel::Snapshot>());
    println!("GoodNews {}", size_of::<rtsim_travel::news::GoodNews>());
    println!(
        "Region<Market> {}",
        size_of::<rtsim_kernel::lazy::Region<rtsim_trade::Market>>()
    );
    println!("PacketData {}", size_of::<rtsim_world::world::PacketData>());
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut w = rtsim_world::World::build(cfg, goods, book, synth::grid(1024, 42));
    w.run_until(Tick::from_days(20));
    println!(
        "markets heap {} (table), edges cap {} len {}, nodes cap {} len {}",
        w.markets.heap_bytes(),
        w.edges.capacity(),
        w.edges.len(),
        w.nodes.capacity(),
        w.nodes.len()
    );
    let mut idle = 0;
    let mut total = 0;
    let b = w.cfg.trade.quantum.index(w.now());
    for e in &w.edges {
        for d in &e.dirs {
            for (_, p) in d.pipelines() {
                total += 1;
                if p.is_idle(b) {
                    idle += 1;
                }
            }
        }
    }
    println!(
        "pipelines {total}, idle {idle}; packets {}",
        w.packets.len()
    );
    println!(
        "table wakes pending {} heap {}; world wheel {}",
        w.markets.pending_wakes(),
        w.markets.heap_bytes(),
        w.world_wheel_len()
    );
}
