// SPDX-License-Identifier: AGPL-3.0-or-later
//! A first run of the synthetic world with conservation checked after every event.

use rtsim_kernel::clock::Tick;
use rtsim_world::WorldConfig;
use rtsim_world::synth;

#[test]
fn synthetic_world_runs_and_conserves() {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = true;
    cfg.genesis = rtsim_trade::build::Genesis::Share(500);
    let (mut w, ids) = synth::build(cfg, true);
    eprintln!("edges {} lints {:?}", w.edges.len(), w.lints);
    for day in 1..=30u64 {
        w.run_until(Tick::from_days(day));
        if day % 5 == 0 {
            eprintln!("day {day}: {:?}", w.stats);
            for i in 0..w.nodes.len() {
                let m = w.market(i);
                eprintln!(
                    "  {:<14} bread {:>8} target {:>8} price {:>9}  treasury {:>8}",
                    synth::NAMES[i],
                    m.stock(ids.bread),
                    m.line(ids.bread).target,
                    m.price(ids.bread),
                    m.treasury()
                );
            }
        }
    }
    w.check_conserved(w.now()).unwrap();
}
