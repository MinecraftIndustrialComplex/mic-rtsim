// SPDX-License-Identifier: AGPL-3.0-or-later
//! The synthetic world from autumn into winter: winter shifts what dragons prey on (the season
//! weights of P12's prey kinds), and the books balance all the way.

use rtsim_ecology::calendar::{Calendar, Season};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth;
use rtsim_ecology::tunables::{DragonTunables, PreyKind};
use rtsim_kernel::clock::Tick;

#[test]
fn winter_shifts_the_prey_from_roads_to_villages() {
    let (mut w, _) = synth::world(7);
    let tun = DragonTunables {
        calendar: Calendar::starting_in(Season::Autumn),
        ..Default::default()
    };
    let mut d = synth::dragons(7, tun);
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    d.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(48));
    d.check(&w, None).unwrap();
    let share = |s: Season| {
        let v = d.prey_by_season[s.index()];
        let (r, vi) = (v[PreyKind::Road.index()], v[PreyKind::Village.index()]);
        assert!(r + vi > 5, "{s:?}: too few hunts to tell");
        vi as f64 / (r + vi) as f64
    };
    let (autumn, winter) = (share(Season::Autumn), share(Season::Winter));
    assert!(
        winter > autumn + 0.25,
        "villages' share: autumn {autumn:.2}, winter {winter:.2}"
    );
}
