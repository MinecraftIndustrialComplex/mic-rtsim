// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli sim quiet`: does a closed world go quiet? (economy research failure table: "event
//! rate reaches zero in a closed world (quiescence)"; overnight choices E43 to E49.)
//!
//! Runs a world with no player at all and prints market events per market per day, window by
//! window, with conservation checked at the end of each window. The world is either the phase 1
//! scenario's eleven nodes or a grid of `n` villages (`rtsim_world::synth::grid`, the bench's).
//!
//! It also counts the event archive's rows (R, VIEWER.md §10.4): the world's two tapes are on and
//! drained every game day, so a window is run a day at a time.

use rtsim_archive::record::Meter;
use rtsim_kernel::clock::Tick;
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};

use super::archive::{Rate, drain_world, world_on};

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
    /// The synthetic works catalogue (SINK.md): villages spend the coin their treasuries sink
    /// above the ceiling. Markets run exactly as without it (MS-1).
    pub works: bool,
}

/// Works bought and home-coin thousandths spent on them in one window.
#[derive(Clone, Copy, Default)]
struct WorksWindow {
    bought: u64,
    spent: i128,
    purses: i128,
}

/// What a run measured: events per market per day for each window, the works bought, the
/// conservation check, the world, and the archive rows its tapes took over `days` game days.
struct Series {
    events: Vec<f64>,
    works: Vec<WorksWindow>,
    check: Result<(), String>,
    world: World,
    meter: Meter,
    days: u64,
}

/// Builds the world and runs it, window by window.
fn series_world(o: &Options) -> Series {
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
        // On before the joins, so their rows are counted.
        world_on(&mut w);
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
    world_on(&mut w);
    if o.trace_days > 0 {
        w.enable_trace();
    }
    if o.works {
        let cat = rtsim_world::works::synthetic_catalog(&w.goods);
        w.set_works(cat, w.now());
    }
    let n = w.nodes.len() as f64;
    let mut out = Vec::new();
    let mut works = Vec::new();
    let mut meter = Meter::new();
    let mut day = 0;
    let mut check = Ok(());
    let spent = |w: &World| -> i128 { w.works_totals().0.iter().map(|x| x.1).sum() };
    while day < o.days {
        let before = w.stats.market_events;
        let (bought0, spent0) = (w.stats.works, spent(&w));
        let first = day + 1;
        day += o.every;
        // A day at a time, so the tapes are drained before they fill.
        for d in first..=day {
            w.run_until(Tick::from_days(d));
            drain_world(&mut meter, &mut w);
        }
        out.push((w.stats.market_events - before) as f64 / o.every as f64 / n);
        let purses = (0..w.nodes.len())
            .flat_map(|i| w.works_purse(i, w.now()))
            .map(|x| i128::from(x.1))
            .sum();
        works.push(WorksWindow {
            bought: w.stats.works - bought0,
            spent: spent(&w) - spent0,
            purses,
        });
        // The bridge: every bought work placed at the window's end (a village buys its next
        // work once the last one is answered, MSC13); other commands are dropped.
        for c in w.take_commands() {
            if let Some(op) = c.work_op() {
                let now = w.now();
                w.work_result(op, true, now);
            }
        }
        if let Err(e) = w.check_conserved(w.now()) {
            check = Err(format!("day {day}: {e}"));
        }
    }
    drain_world(&mut meter, &mut w);
    Series {
        events: out,
        works,
        check,
        world: w,
        meter,
        days: day,
    }
}

pub fn run(o: Options) -> i32 {
    let Series {
        events: s,
        works,
        check,
        world: w,
        meter,
        days,
    } = series_world(&o);
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
    if o.works {
        println!("works bought / coin spent / coin in purses at the window's end, per window:");
        let cells: Vec<String> = works
            .iter()
            .map(|x| {
                format!(
                    "{}/{:.0}/{:.0}",
                    x.bought,
                    x.spent as f64 / 1000.0,
                    x.purses as f64 / 1000.0
                )
            })
            .collect();
        println!("{}", cells.join(" "));
    }
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
    let code = match check {
        Ok(()) => {
            println!("conservation: ok");
            0
        }
        Err(e) => {
            println!("conservation: {e}");
            1
        }
    };
    let rate = Rate::new(&meter, days as f64);
    rate.print(None);
    rate.print_kinds();
    code
}
