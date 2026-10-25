// SPDX-License-Identifier: AGPL-3.0-or-later
//! The far view's dragon legs (`far.rs`, docs/boundary.md §12): a session start sends every
//! dragon, then only the ones that changed, each record matching the dragon's row.

use rtsim_ecology::dragon::{State, dragon_of};
use rtsim_ecology::far::{FarLeg, FarState};
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth;
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

fn matches_row(d: &rtsim_ecology::Dragons, r: &FarLeg, at: Tick) {
    let id = dragon_of(r.agent).expect("a dragon's agent");
    let Some(x) = d.dragon(id) else {
        assert_eq!(r.state, FarState::Gone, "{r:?}: no row, so gone");
        return;
    };
    assert_eq!(r.species, x.species);
    assert_eq!(r.variant, x.variant);
    assert!((1..=5).contains(&r.stage), "{r:?}");
    assert_eq!(u64::from(r.age_days), x.age(at) / TICKS_PER_DAY, "{r:?}: the age in whole days, as Ice and Fire's");
    match (r.state, x.state, x.leg) {
        (FarState::Fly, State::Flying(_), Some(l)) | (FarState::Hunt, State::Hunting { .. }, Some(l)) => {
            assert_eq!((r.x0, r.z0, r.x1, r.z1), (l.path.x0, l.path.z0, l.path.x1, l.path.z1));
            assert_eq!(r.t0, l.path.t0);
            assert!(r.t1 >= l.path.t1);
            assert_eq!(r.y, l.y);
        }
        (FarState::Roost, s, _) => assert!(!x.is_embodied(), "{s:?}"),
        (FarState::Body, _, _) => assert!(x.is_embodied()),
        other => panic!("record {r:?} does not match the row: {other:?}"),
    }
}

#[test]
fn a_session_sends_every_dragon_then_only_changes() {
    let (mut w, _) = synth::world(11);
    let mut d = synth::dragons(11, DragonTunables::default());
    synth::populate(&mut w, &mut d, &synth::lairs(), Tick::ZERO);
    assert!(d.take_far(Tick::ZERO).is_empty(), "off until asked");
    assert_eq!(d.far_pending(), 0, "nothing marked while off");

    d.set_far_feed(true);
    let all = d.take_far(Tick::ZERO);
    assert_eq!(all.len(), d.dragons.len(), "a session start sends every dragon");
    for r in &all {
        matches_row(&d, r, Tick::ZERO);
    }
    assert!(all.iter().any(|r| r.age_days > 0), "some dragon is a day old or more");
    assert!(d.take_far(Tick::ZERO).is_empty(), "nothing changed since");

    let mut flights = 0;
    let mut steps = 0;
    for h in 1..=96u64 {
        let at = Tick(h * 1_000);
        d.run_until(&mut w, &mut NoInfrastructure, at);
        let due = d.take_far(at);
        steps += usize::from(!due.is_empty());
        for r in &due {
            matches_row(&d, r, at);
            flights += usize::from(r.state == FarState::Fly);
        }
    }
    assert!(flights > 0, "dragons flew in four days");
    assert!(steps < 96, "a quiet hour sends nothing ({steps} of 96 sent)");

    // A new session sends everything again.
    d.set_far_feed(true);
    assert_eq!(d.take_far(Tick(96_000)).len(), d.dragons.len());
    d.set_far_feed(false);
    d.run_until(&mut w, &mut NoInfrastructure, Tick(200_000));
    assert_eq!(d.far_pending(), 0);
}
