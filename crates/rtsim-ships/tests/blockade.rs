// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b-4 (EQUILIBRIUM.md §2.4.6a rule 7; docs/choices-ports.md PC20 to PC25, PC80 to PC89): a
//! blockaded trade port closes and reopens.
//!
//! The rules are judged on the blockade coast: [`synth::coastal_with`]'s coast with three pirates
//! of each medium, no navy, and every sea lane but dock 3's two escorted out of the pirates'
//! reach, so they hunt dock 3's lanes. The many-worlds measurement runs on `synth::sea_havens`
//! and `synth::coastal`.

use rtsim_archive::kinds;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::fnv1a64;
use rtsim_ships::engine::{ShipId, Status};
use rtsim_ships::synth::{self, Coastal, FakeShips, Script};
use rtsim_ships::timetable::ServiceKind;
use rtsim_ships::{ShipTunables, Ships};
use rtsim_travel::Medium;

/// The port the pirates blockade.
const TARGET: u32 = 3;
const STEP: u64 = TICKS_PER_DAY / 4;

fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

fn days(d: f64) -> u64 {
    (d * TICKS_PER_DAY as f64).round() as u64
}

/// The blockade coast for `seed` (see the module's comment) and its fake plugin.
fn blockade_coast(seed: u64, tun: &ShipTunables) -> (Coastal, FakeShips) {
    let mut c = synth::coastal_with(seed, tun.clone(), 3, false);
    let keep = c.ships.dock_lanes(TARGET).to_vec();
    assert_eq!(keep.len(), 2);
    for l in 0..c.ships.lanes.len() as u32 {
        if c.ships.lanes[l as usize].medium == Medium::Sea && !keep.contains(&l) {
            c.ships.set_escort(&mut c.world, l, 9, 20.0, Tick::ZERO);
        }
    }
    assert!(c.ships.is_trade_port(&c.world, TARGET));
    let fake = FakeShips::new(f64::from(tun.view_radius));
    (c, fake)
}

fn go(c: &mut Coastal, fake: &mut FakeShips, to: Tick) {
    synth::drive(
        &mut c.world,
        &mut c.ships,
        fake,
        &mut NoInfrastructure,
        &c.mask,
        to,
        STEP,
    );
}

/// Steps the coast until `done` holds after a step, or `limit`; checks the engine every day.
/// Whether `done` held.
fn run_until(
    c: &mut Coastal,
    fake: &mut FakeShips,
    limit: Tick,
    mut done: impl FnMut(&Coastal, &FakeShips) -> bool,
) -> bool {
    let mut t = c.ships.now();
    while t < limit {
        t = (t + STEP).min(limit);
        go(c, fake, t);
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            c.ships
                .check(&c.world)
                .unwrap_or_else(|e| panic!("day {:.2}: {e}", day(t)));
        }
        if done(c, fake) {
            return true;
        }
    }
    false
}

/// The merchant services calling at dock `d`.
fn calling(s: &Ships, d: u32) -> Vec<u32> {
    (0..s.services.len() as u32)
        .filter(|&v| {
            let sv = &s.services[v as usize];
            sv.kind == ServiceKind::Merchant
                && sv.legs.iter().any(|g| g.from_dock == d || g.to_dock == d)
        })
        .collect()
}

fn fleet(s: &Ships, services: &[u32]) -> Vec<ShipId> {
    services.iter().flat_map(|&v| s.service_ships(v)).collect()
}

/// Dock `d`'s `ships.port` rows on the tape (each row stands at its dock).
fn port_events(s: &Ships, d: u32) -> Vec<rtsim_archive::Event> {
    let k = &s.docks[d as usize];
    rtsim_archive::record::rows_on(&s.tape)
        .into_iter()
        .filter_map(|(kind, _, r)| match r {
            rtsim_archive::Row::Event(e)
                if kind == kinds::SHIPS_PORT && (e.x, e.z) == (k.x, k.z) =>
            {
                Some(e)
            }
            _ => None,
        })
        .collect()
}

/// The flow on lane `l`'s world edge, both directions, thousandths of a slot per quantum.
fn volume(c: &Coastal, l: u32) -> i64 {
    let e = &c.world.edges[c.ships.lanes[l as usize].edge as usize];
    e.dirs
        .iter()
        .map(|d| d.volume_slot_milli(&c.world.goods))
        .sum()
}

/// Rule 7 on the blockade coast, seeds 1 to 3. The pirates hunt dock 3's two lanes: the port is
/// blockaded once both lanes' heats reach a quarter of full, and closes at the first check that
/// finds both at half. Closed, its lanes carry nothing new, its services lie in port (nobody
/// watches, so every ship lays up at once), and the pirates, finding no prey there, move on. The
/// heat cools and, no sooner than 30 days after the closure, a check finds no lane above a
/// quarter: the port reopens, the lanes open, every laid-up ship relaunches at its home dock on
/// its timetable and the lanes carry goods again. Goods are conserved throughout (the daily
/// checks).
#[test]
fn a_blockaded_port_closes_and_reopens() {
    let tun = ShipTunables::default();
    let min_closed = days(tun.blockade_min_closed_days);
    for seed in 1..=3u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        c.ships.tape.set_on(true);
        let lanes = c.ships.dock_lanes(TARGET).to_vec();
        let services = calling(&c.ships, TARGET);
        assert!(!services.is_empty());
        let ships = fleet(&c.ships, &services);
        assert!(
            run_until(&mut c, &mut fake, Tick::from_days(90), |c, _| c
                .ships
                .stats
                .closures
                == 1),
            "seed {seed}: the port never closed"
        );
        let p = *c.ships.port(TARGET).expect("a state");
        let closed = p.closed.expect("closed");
        let blockaded = p.blockaded.expect("blockaded before it closed");
        assert!(blockaded <= closed, "seed {seed}: {p:?}");
        assert_eq!(c.ships.stats.blockades, 1);
        for &l in &lanes {
            assert!(
                c.world.lane_closed(c.ships.lanes[l as usize].edge),
                "seed {seed}: lane {l} open at a closed port"
            );
        }
        for &s in &services {
            assert_eq!(c.ships.laid_up_since(s), Some(closed), "seed {seed}");
        }
        assert!(
            ships
                .iter()
                .all(|&id| !c.ships.ship(id).expect("a ship").is_active()),
            "seed {seed}: a ship still sails for a closed port with nobody watching"
        );
        let laid = c.ships.stats.ships_laid_up_by_closure;
        assert!(laid > 0);
        let ev = port_events(&c.ships, TARGET);
        assert_eq!(
            ev.iter()
                .map(|e| (e.tick, e.magnitude))
                .collect::<Vec<_>>(),
            vec![(blockaded.0, 1), (closed.0, 2)],
            "seed {seed}"
        );
        // The pirates move on; the port reopens after its minimum. From ten days after the
        // closure until then, the lanes carry nothing.
        let quiet = closed + 10 * TICKS_PER_DAY;
        let mut carried = false;
        assert!(
            run_until(
                &mut c,
                &mut fake,
                closed + 150 * TICKS_PER_DAY,
                |c, _| {
                    if c.ships.stats.reopenings == 1 {
                        return true;
                    }
                    carried |= c.ships.now() >= quiet && lanes.iter().any(|&l| volume(c, l) > 0);
                    false
                }
            ),
            "seed {seed}: the port never reopened"
        );
        assert!(!carried, "seed {seed}: a closed lane carried goods");
        let ev = port_events(&c.ships, TARGET);
        let reopened = ev
            .iter()
            .find(|e| e.magnitude == 3)
            .map(|e| Tick(e.tick))
            .expect("a reopening on the tape");
        assert!(
            reopened >= closed + min_closed,
            "seed {seed}: closed only {:.1} days",
            day(reopened) - day(closed)
        );
        assert!(!c.ships.port_closed(TARGET));
        assert_eq!(
            c.ships.stats.closed_dock_days,
            (reopened.0 - closed.0) / TICKS_PER_DAY
        );
        for &l in &lanes {
            assert!(!c.world.lane_closed(c.ships.lanes[l as usize].edge));
        }
        let mut period = 0;
        for &s in &services {
            assert!(!c.ships.service_laid_up(s));
            period = period.max(c.ships.services[s as usize].period);
        }
        for &id in &ships {
            let x = c.ships.ship(id).expect("a ship");
            assert!(!x.is_laid_up(), "seed {seed}: {x:?}");
            if let Status::Lost { back, .. } = x.status {
                assert!(back >= reopened, "seed {seed}: relaunched before the reopening");
            }
        }
        // One period on, the ships laid up by the closure sail again (bar any the pirates, back
        // on the lanes, have sunk since), and within 60 days the port's lanes have carried goods
        // again (a hunted lane's flow comes and goes, and seed 2's second lane carries nothing
        // for months with closures off too).
        let back = reopened + period + TICKS_PER_DAY;
        let end = back.max(reopened + 60 * TICKS_PER_DAY);
        let mut active = None;
        let mut flowed = vec![false; lanes.len()];
        run_until(&mut c, &mut fake, end, |c, _| {
            for (f, &l) in flowed.iter_mut().zip(&lanes) {
                *f |= volume(c, l) > 0;
            }
            if active.is_none() && c.ships.now() >= back {
                active = Some(
                    ships
                        .iter()
                        .filter(|&&id| c.ships.ship(id).is_some_and(|x| x.is_active()))
                        .count(),
                );
            }
            false
        });
        let active = active.expect("sampled");
        assert!(
            active * 4 >= ships.len() * 3,
            "seed {seed}: {active} of {} sail again",
            ships.len()
        );
        assert!(
            flowed.iter().any(|&f| f),
            "seed {seed}: a lane carried nothing after the reopening: {flowed:?}"
        );
        eprintln!(
            "seed {seed}: blockaded day {:.1}, closed day {:.1}, reopened day {:.1}, {laid} ships \
             laid up, {active} of {} active {:.0} days on",
            day(blockaded),
            day(closed),
            day(reopened),
            ships.len(),
            day(back) - day(reopened)
        );
    }
}

/// The heat's hysteresis on the blockade coast, seeds 1 to 4 for 400 days: an escort joins dock
/// 3's lanes for two days in every four, so the pirates' applied share crosses half of full on
/// and off every two days. The port's state follows the 30-day heat, not the share: it changes
/// only at checks, by the legal steps (open, blockaded, closed, reopened, lifted), each closed
/// spell lasts at least `blockade_min_closed_days`, and it changes state far less often than the
/// share crosses half.
#[test]
fn a_blockade_cannot_flap() {
    let tun = ShipTunables::default();
    let min_closed = days(tun.blockade_min_closed_days);
    let check = days(tun.blockade_check_days);
    let mut closures = 0;
    for seed in 1..=4u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        c.ships.tape.set_on(true);
        let half = c.ships.full_share(Medium::Sea) / 2;
        let lanes = c.ships.dock_lanes(TARGET).to_vec();
        let mut crossings = 0;
        let mut above = false;
        let mut t = Tick::ZERO;
        while t < Tick::from_days(400) {
            t += STEP;
            go(&mut c, &mut fake, t);
            let escorted = (t.0 / (2 * TICKS_PER_DAY)) % 2 == 1;
            for &l in &lanes {
                c.ships
                    .set_escort(&mut c.world, l, 5, if escorted { 20.0 } else { 0.0 }, t);
            }
            let now = lanes.iter().all(|&l| c.ships.lane_share(l) >= half);
            if now != above {
                crossings += 1;
                above = now;
            }
            if t.0.is_multiple_of(TICKS_PER_DAY) {
                c.ships.check(&c.world).expect("checks");
            }
        }
        let ev = port_events(&c.ships, TARGET);
        // Replay the tape: (blockaded, closed since).
        let (mut blockaded, mut closed): (bool, Option<u64>) = (false, None);
        let mut last_change: Option<u64> = None;
        for e in &ev {
            match e.magnitude {
                1 => {
                    assert!(!blockaded, "seed {seed}: blockaded twice: {ev:?}");
                    blockaded = true;
                }
                2 => {
                    assert!(blockaded && closed.is_none(), "seed {seed}: {ev:?}");
                    closed = Some(e.tick);
                    closures += 1;
                }
                3 => {
                    let since = closed.take().expect("closed before reopening");
                    assert!(
                        e.tick >= since + min_closed,
                        "seed {seed}: closed {:.1} days",
                        (e.tick - since) as f64 / TICKS_PER_DAY as f64
                    );
                }
                4 => {
                    assert!(blockaded && closed.is_none(), "seed {seed}: {ev:?}");
                    blockaded = false;
                }
                m => panic!("magnitude {m}"),
            }
            if let Some(l) = last_change
                && l != e.tick
            {
                assert!(
                    e.tick >= l + check,
                    "seed {seed}: two changes {} ticks apart",
                    e.tick - l
                );
            }
            last_change = Some(e.tick);
        }
        let changes = ev.len();
        assert!(
            crossings >= 4 * changes.max(1),
            "seed {seed}: the share crossed half {crossings} times, the port changed {changes}"
        );
        eprintln!(
            "seed {seed}: share crossed half {crossings} times; port changes {changes} ({} \
             closures)",
            ev.iter().filter(|e| e.magnitude == 2).count()
        );
    }
    assert!(closures > 0, "no closure to judge");
}

/// The hashes of the parent commit (03f9a3f6, S8b-1 merged, before S8b-4b) for three runs: the
/// coast's scripted player for 30 days (seed 7) and two seas unattended for 96 days. Printed by
/// the same scenarios built on that commit.
const PARENT: [(&str, u64, u64); 3] = [
    (
        "coastal 7 script 30 days",
        0x4ee1_ab87_3239_177a,
        0x8543_149d_9ec4_39c2,
    ),
    (
        "sea_havens 1 unattended 96 days",
        0x83df_6cad_c994_afcd,
        0x3c13_3644_f099_8bf7,
    ),
    (
        "sea_havens 2 unattended 96 days",
        0xd67f_506a_8c6a_61e0,
        0x7add_52d0_3b00_6e73,
    ),
];

/// `blockade_days = 0` turns closures off and changes nothing: the ships' and the world's hashes
/// equal the parent commit's for the same runs, no port has a state, no lane has heat and no
/// check was ever booked.
#[test]
fn blockade_off_changes_nothing() {
    let tun = ShipTunables {
        blockade_days: 0.0,
        ..ShipTunables::default()
    };
    let mut got: Vec<(String, u64, u64)> = Vec::new();
    let mut c = synth::coastal(7, tun.clone());
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let mut s = Script::default();
    s.run(&mut c, &mut fake, Tick::ZERO, Tick::from_days(30));
    assert!(s.conserved.is_empty(), "{:?}", s.conserved);
    let quiet = |s: &Ships| {
        assert!(s.ports.is_empty());
        assert!(s.lane_threats().iter().all(|(_, t)| t.heat_milli == 0));
        assert_eq!(s.stats.port_checks, 0);
        assert!(!s.pending_by_kind().contains_key("dock_check"));
    };
    quiet(&c.ships);
    got.push((
        "coastal 7 script 30 days".into(),
        c.ships.state_hash(),
        fnv1a64(&c.world.save()),
    ));
    for seed in [1u64, 2] {
        let mut sh = synth::sea_havens(seed, tun.clone());
        sh.ships
            .run_until(&mut sh.world, &mut NoInfrastructure, Tick::from_days(96));
        quiet(&sh.ships);
        got.push((
            format!("sea_havens {seed} unattended 96 days"),
            sh.ships.state_hash(),
            fnv1a64(&sh.world.save()),
        ));
    }
    for ((name, a, b), (pn, pa, pb)) in got.iter().zip(PARENT) {
        assert_eq!(name, pn);
        assert_eq!(
            (*a, *b),
            (pa, pb),
            "{name}: ships {a:#018x} world {b:#018x}, the parent's ships {pa:#018x} world \
             {pb:#018x}"
        );
    }
}

#[test]
#[ignore]
fn explore_volume() {
    let off = std::env::var("OFF").is_ok();
    let tun = ShipTunables { blockade_days: if off { 0.0 } else { 30.0 }, ..ShipTunables::default() };
    for seed in 2..=2u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        let lanes = c.ships.dock_lanes(TARGET).to_vec();
        let mut t = Tick::ZERO;
        while t < Tick::from_days(200) {
            t += 4 * TICKS_PER_DAY;
            go(&mut c, &mut fake, t);
            let v: Vec<i64> = lanes.iter().map(|&l| volume(&c, l)).collect();
            let sh: Vec<i64> = lanes.iter().map(|&l| c.ships.lane_share(l)).collect();
            let cl: Vec<bool> = lanes.iter().map(|&l| c.world.lane_closed(c.ships.lanes[l as usize].edge)).collect();
            eprintln!("seed {seed} day {:.0}: vol {v:?} share {sh:?} closed {cl:?} port {:?}", day(t), c.ships.port(TARGET).map(|p| (p.blockaded.is_some(), p.closed.is_some())));
        }
    }
}
