// SPDX-License-Identifier: AGPL-3.0-or-later
//! A quick look at the synthetic world running (printed with --nocapture).

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::Tick;

#[test]
fn a_season_runs_and_balances() {
    let (mut w, _) = synth::world(7);
    let mut tun = DragonTunables::default();
    tun.calendar = rtsim_ecology::calendar::Calendar::starting_in(rtsim_ecology::calendar::Season::Autumn);
    let mut d = synth::dragons(7, tun);
    d.log_enabled = true;
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    let mut infra = NoInfrastructure;
    d.run_until(&mut w, &mut infra, Tick::from_days(3));
    let c = d.by_name("Cinderjaw").unwrap();
    let mut v = d.weigh(&w, c, w.now());
    v.sort_by(|a, b| b.2.total_cmp(&a.2));
    for (t, k, u) in v.iter().take(12) {
        println!("{k:?} {u:.2} {t:?}");
    }
    for (i, e) in w.edges.iter().enumerate() {
        for dir in &e.dirs {
            let rates: Vec<(u16, i64)> = dir.goods.iter().map(|(g, p)| (g.0, p.current_rate())).collect();
            println!("edge {i} {}->{} period {} d {} rates {:?}", w.nodes[dir.from].key, w.nodes[dir.to].key, dir.lattice.current_period(), e.d, rates);
        }
    }
    d.run_until(&mut w, &mut infra, Tick::from_days(48));
    for l in &d.log {
        println!("{:7.2} {:12} {:10} {}", l.day, l.kind, l.dragon.clone().unwrap_or_default(), l.what);
    }
    println!("{:?}", d.stats);
    println!("prey by season {:?}", d.prey_by_season);
    d.check(&w, None).unwrap();
    assert!(d.stats.hunts > 0);
}
