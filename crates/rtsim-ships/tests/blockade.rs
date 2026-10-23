// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b-4 (EQUILIBRIUM.md §2.4.6a rule 7; docs/choices-ports.md PC20 to PC25, PC60 to PC69): a
//! blockaded trade port closes and reopens.

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_ships::ShipTunables;
use rtsim_ships::synth::{self, FakeShips};
use rtsim_travel::Medium;

fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

#[test]
#[ignore]
fn explore() {
    for seed in 1..=4u64 {
        let mut c = synth::coastal(seed, ShipTunables::default());
        let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
        if seed == 1 {
            for (d, k) in c.ships.docks.iter().enumerate() {
                println!(
                    "dock {d} node {} {:?} lanes {:?}",
                    k.node,
                    k.medium,
                    c.ships.dock_lanes(d as u32)
                );
            }
        }
        let mut t = Tick::ZERO;
        let mut last = (0, 0);
        while t < Tick::from_days(960) {
            t = t + TICKS_PER_DAY;
            synth::drive(
                &mut c.world,
                &mut c.ships,
                &mut fake,
                &mut NoInfrastructure,
                &c.mask,
                t,
                TICKS_PER_DAY / 4,
            );
            let st = c.ships.stats;
            if (st.closures, st.reopenings) != last {
                last = (st.closures, st.reopenings);
                let closed: Vec<u32> = c
                    .ships
                    .ports
                    .iter()
                    .filter(|(_, p)| p.closed.is_some())
                    .map(|(d, _)| *d)
                    .collect();
                println!(
                    "seed {seed} day {:.0}: closures {} reopenings {} closed {closed:?}",
                    day(t),
                    st.closures,
                    st.reopenings
                );
            }
            if t.0 % (96 * TICKS_PER_DAY) == 0 {
                c.ships.check(&c.world).expect("checks");
                let heats: Vec<(u32, i64)> = c
                    .ships
                    .lane_threats()
                    .iter()
                    .map(|(l, _)| (*l, c.ships.heat_at(*l, t)))
                    .filter(|x| x.1 > 0)
                    .collect();
                println!(
                    "seed {seed} day {:.0}: blockades {} hunts {} heats {heats:?}",
                    day(t),
                    st.blockades,
                    st.hunts
                );
            }
        }
        let _ = Medium::Sea;
    }
}

#[test]
#[ignore]
fn explore_forced() {
    for seed in 1..=4u64 {
        let mut c = synth::coastal_with(seed, ShipTunables::default(), 3, false);
        let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
        let target = 3u32;
        let keep: Vec<u32> = c.ships.dock_lanes(target).to_vec();
        for l in 0..c.ships.lanes.len() as u32 {
            if c.ships.lanes[l as usize].medium == Medium::Sea && !keep.contains(&l) {
                c.ships.set_escort(&mut c.world, l, 9, 20.0, Tick::ZERO);
            }
        }
        let mut t = Tick::ZERO;
        while t < Tick::from_days(200) {
            t = t + TICKS_PER_DAY;
            synth::drive(
                &mut c.world,
                &mut c.ships,
                &mut fake,
                &mut NoInfrastructure,
                &c.mask,
                t,
                TICKS_PER_DAY / 4,
            );
            if t.0 % (4 * TICKS_PER_DAY) == 0 {
                c.ships.check(&c.world).expect("checks");
                let heats: Vec<i64> = keep.iter().map(|&l| c.ships.heat_at(l, t)).collect();
                let st = c.ships.stats;
                println!(
                    "seed {seed} day {:.0}: port {:?} heats {heats:?} closures {} reopen {} laid {} hunts {}",
                    day(t),
                    c.ships.port(target),
                    st.closures,
                    st.reopenings,
                    st.ships_laid_up_by_closure,
                    st.hunts
                );
            }
        }
    }
}
