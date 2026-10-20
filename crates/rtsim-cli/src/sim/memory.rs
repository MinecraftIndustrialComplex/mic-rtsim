// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim memory`: where a phase 1 world's memory goes, per market (overnight choice
//! E58). Builds a grid of villages (the bench's world), runs it, then prints the engine's estimate
//! by component next to what the counting allocator says is live.

use rtsim_kernel::clock::Tick;
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

use crate::sys;

pub fn run(markets: u64, days: u64) -> i32 {
    let before = sys::heap_live();
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    let mut w = World::build(cfg, goods, book, synth::grid(markets as usize, 42));
    w.run_until(Tick::from_days(days));
    let live = sys::heap_live().saturating_sub(before);
    let m = w.memory();
    let total: usize = m.values().sum();
    let n = w.nodes.len() as f64;
    println!(
        "a grid of {} villages, {} edges, after {} days: {:.1} KB live per market (counting allocator), {:.1} KB estimated",
        w.nodes.len(),
        w.edges.len(),
        days,
        live as f64 / n / 1024.0,
        total as f64 / n / 1024.0
    );
    let mut rows: Vec<_> = m.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (k, v) in rows {
        println!(
            "  {:<38} {:>9.0} B per market  {:>5.1}%",
            k,
            v as f64 / n,
            100.0 * v as f64 / total.max(1) as f64
        );
    }
    0
}
