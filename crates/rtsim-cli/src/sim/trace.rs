// SPDX-License-Identifier: AGPL-3.0-or-later
//! What caused a world's market events: the trace breakdown shared by the scenario runners.

use std::collections::BTreeMap;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_world::World;

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// What caused the market events of the last `days` days: per node and cause, and which goods'
/// news changed most often.
pub fn print_trace(w: &World, days: u64, name: &dyn Fn(usize) -> String) {
    let Some(trace) = &w.trace else { return };
    // What broke each long silence: the first events after a quiet day or more.
    let mut last = Tick::ZERO;
    let mut shown = 0;
    for (k, r) in trace.iter().enumerate() {
        if r.at.0 >= last.0 + TICKS_PER_DAY && k > 0 && shown < 6 {
            shown += 1;
            println!(
                "  silence of {:.1} days broken at day {:.2} by:",
                (r.at.0 - last.0) as f64 / TICKS_PER_DAY as f64,
                day_of(r.at)
            );
            for x in trace[k..].iter().take(4) {
                println!(
                    "    {:<14} {:?} diffs {:?} flows {:?}",
                    name(x.node),
                    x.cause,
                    x.diffs
                        .iter()
                        .map(|d| (w.goods.get(d.0).name.as_str(), d.1, d.2, d.3, d.4))
                        .collect::<Vec<_>>(),
                    x.flows
                        .iter()
                        .map(|f| (name(f.0), w.goods.get(f.1).name.as_str(), f.2, f.3, f.4))
                        .collect::<Vec<_>>()
                );
            }
        }
        last = r.at;
    }
    let from = Tick(w.now().0.saturating_sub(days * TICKS_PER_DAY));
    let rows: Vec<_> = trace.iter().filter(|r| r.at > from).collect();
    println!(
        "\n== trace: {} market events in the last {days} days ==",
        rows.len()
    );
    let mut by: BTreeMap<(usize, String), u64> = BTreeMap::new();
    let mut diffs: BTreeMap<(usize, String, String), u64> = BTreeMap::new();
    for r in &rows {
        *by.entry((r.node, format!("{:?}", r.cause))).or_default() += 1;
        for d in &r.diffs {
            let what = if d.1 != d.2 {
                format!("{:?}->{:?}", d.1, d.2)
            } else {
                format!("adv {}->{}", d.3, d.4)
            };
            *diffs
                .entry((r.node, w.goods.get(d.0).name.clone(), what))
                .or_default() += 1;
        }
        if r.cash_changed {
            *diffs
                .entry((r.node, "cash".into(), "can_pay/refuses".into()))
                .or_default() += 1;
        }
    }
    for i in 0..w.nodes.len() {
        let causes: Vec<String> = by
            .range((i, String::new())..(i + 1, String::new()))
            .map(|((_, c), n)| format!("{c} {n}"))
            .collect();
        println!("  {:<14} {}", name(i), causes.join(", "));
    }
    let mut pipes: BTreeMap<(usize, usize, String, String), (u64, i64)> = BTreeMap::new();
    for r in &rows {
        for f in &r.flows {
            let e = pipes
                .entry((
                    r.node,
                    f.0,
                    w.goods.get(f.1).name.clone(),
                    format!("{:?}", f.2),
                ))
                .or_default();
            e.0 += 1;
            e.1 += (f.4 - f.3).abs();
        }
    }
    let mut ptop: Vec<_> = pipes.into_iter().collect();
    ptop.sort_by_key(|a| std::cmp::Reverse(a.1.0));
    println!("  most frequent pipeline changes (count, sum of |change|):");
    for ((i, j, g, k), (n, d)) in ptop.into_iter().take(20) {
        println!(
            "    {n:>5} {d:>8}  {:<14} -> {:<14} {g:<14} {k}",
            name(i),
            name(j)
        );
    }
    let mut top: Vec<_> = diffs.into_iter().collect();
    top.sort_by_key(|a| std::cmp::Reverse(a.1));
    println!("  most frequent news changes:");
    for ((i, g, what), n) in top.into_iter().take(25) {
        println!("    {n:>5}  {:<14} {g:<14} {what}", name(i));
    }
}
