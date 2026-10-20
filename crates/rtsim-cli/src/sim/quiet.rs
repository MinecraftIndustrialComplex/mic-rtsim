// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim quiet`: does a closed world go quiet? (economy research failure table: "event
//! rate reaches zero in a closed world (quiescence)"; overnight choices E43 to E49.)
//!
//! Runs a world with no player at all and prints market events per market per day, window by
//! window, with conservation checked at the end of each window. The world is either the phase 1
//! scenario's eleven nodes or a grid of `n` villages (`rtsim_world::synth::grid`, the bench's).

use rtsim_kernel::clock::Tick;
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

pub struct Options {
    /// 0: the phase 1 scenario's world; otherwise a grid of this many villages.
    pub markets: u64,
    pub days: u64,
    pub every: u64,
    pub seed: u64,
    /// Genesis stocks in thousandths of target (1000: at target).
    pub genesis: i64,
    pub rules: Option<rtsim_trade::Rules>,
    /// Days at the end whose events are broken down by cause (0: none).
    pub trace_days: u64,
    /// Grow the phase 1 world by joins instead of building it at once: the first two nodes, then
    /// one more each day (C22).
    pub grow: bool,
    /// Ribbit villages grow houses (P17).
    pub growth: bool,
    /// Ribbit villages have touring bands (P16).
    pub bands: bool,
}

/// Builds the world and returns events per market per day for each window, the conservation
/// check, and the world.
fn series_world(o: &Options) -> (Vec<f64>, Result<(), String>, World) {
    let mut cfg = WorldConfig::new(o.seed);
    cfg.check_every_event = false;
    cfg.genesis = if o.genesis >= 1000 {
        Genesis::AtTarget
    } else {
        Genesis::Share(o.genesis)
    };
    if let Some(r) = o.rules {
        cfg.trade.rules = r;
    }
    cfg.growth = o.growth;
    if !o.bands {
        cfg.band_min_pop = u32::MAX;
    }
    let mut w: World = if o.markets == 0 && o.grow {
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        let all = synth::nodes();
        let mut w = World::build(cfg, goods, book, all[..2].to_vec());
        for (i, d) in all.into_iter().enumerate().skip(2) {
            w.join(d, Tick::from_days(i as u64 - 1));
        }
        w
    } else if o.markets == 0 {
        synth::build(cfg, true).0
    } else {
        let (goods, ids) = synth::goods();
        let book = synth::recipes(&ids);
        World::build(cfg, goods, book, synth::grid(o.markets as usize, o.seed))
    };
    if o.trace_days > 0 {
        w.enable_trace();
    }
    let n = w.nodes.len() as f64;
    let mut out = Vec::new();
    let mut day = 0;
    let mut check = Ok(());
    while day < o.days {
        let before = w.stats.market_events;
        day += o.every;
        w.run_until(Tick::from_days(day));
        out.push((w.stats.market_events - before) as f64 / o.every as f64 / n);
        if let Err(e) = w.check_conserved(w.now()) {
            check = Err(format!("day {day}: {e}"));
        }
    }
    (out, check, w)
}

pub fn run(o: Options) -> i32 {
    let (s, check, w) = series_world(&o);
    let label = if o.markets == 0 {
        "the phase 1 world (11 nodes)".to_string()
    } else {
        format!("a grid of {} villages", o.markets)
    };
    println!(
        "{label}, genesis {} of target, seed {}: market events per market per day, every {} days",
        o.genesis as f64 / 1000.0,
        o.seed,
        o.every
    );
    let cells: Vec<String> = s.iter().map(|x| format!("{x:.2}")).collect();
    println!("{}", cells.join(" "));
    if o.trace_days > 0 {
        let names = if o.markets == 0 {
            synth::NAMES.iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        };
        super::trace::print_trace(&w, o.trace_days, &|i| {
            names.get(i).cloned().unwrap_or_else(|| {
                let n = &w.nodes[i];
                format!(
                    "{}#{i}@{},{}",
                    if n.kind.culture() == Some(rtsim_settle::Culture::Ribbit) {
                        "R"
                    } else {
                        "H"
                    },
                    n.pos.x,
                    n.pos.z
                )
            })
        });
    }
    match check {
        Ok(()) => {
            println!("conservation: ok");
            0
        }
        Err(e) => {
            println!("conservation: {e}");
            1
        }
    }
}
