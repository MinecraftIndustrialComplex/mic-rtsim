// SPDX-License-Identifier: AGPL-3.0-or-later
//! Diagnostics, not tests: `cargo test -p rtsim-world --test diag -- --ignored --nocapture`.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_trade::build::Genesis;
use rtsim_world::synth;
use rtsim_world::WorldConfig;

#[test]
#[ignore]
fn node_detail() {
    let node: usize = std::env::var("NODE").ok().and_then(|s| s.parse().ok()).unwrap_or(9);
    let from_day: u64 = std::env::var("FROM").ok().and_then(|s| s.parse().ok()).unwrap_or(110);
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(
        std::env::var("GENESIS").ok().and_then(|s| s.parse().ok()).unwrap_or(500),
    );
    if let Ok(r) = std::env::var("RULES") {
        cfg.trade.rules = rtsim_trade::Rules::parse(&r).unwrap();
    }
    let markets: usize = std::env::var("MARKETS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let (mut w, ids) = if markets == 0 {
        synth::build(cfg, true)
    } else {
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        (rtsim_world::World::build(cfg, goods, book, synth::grid(markets, 42)), ids)
    };
    w.run_until(Tick::from_days(from_day));
    w.enable_trace();
    let names = std::env::var("GOODS").unwrap_or("wheat,bread,emerald,amethyst_shard".into());
    let goods: Vec<_> = names.split(',').map(|n| w.goods.find(n).expect("good")).collect();
    let _ = ids;
    let span: u64 = std::env::var("SPAN").ok().and_then(|s| s.parse().ok()).unwrap_or(2);
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
                    if let Some(p) = din.goods.get(&g) && p.current_rate() > 0 {
                        line += &format!(" [from {} {}]", din.from, p.current_rate());
                    }
                    let dout = &e.dirs[d.dir as usize];
                    if let Some(p) = dout.goods.get(&g) && p.current_rate() > 0 {
                        line += &format!(" [to {} {}]", dout.to, p.current_rate());
                    }
                }
            }
            println!("{line}");
        }
    }
}
