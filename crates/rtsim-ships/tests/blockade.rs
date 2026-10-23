// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b-4 (EQUILIBRIUM.md §2.4.6a rule 7; docs/choices-ports.md PC20 to PC25, PC80 to PC100): a
//! blockaded trade port closes and reopens.
//!
//! The rules are judged on the blockade coast: [`synth::coastal_with`]'s coast with three pirates
//! of each medium, no navy, and every sea lane but dock 3's two escorted out of the pirates'
//! reach, so they hunt dock 3's lanes. The many-worlds measurement runs on `synth::sea_havens`
//! and `synth::coastal`.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

use rtsim_archive::kinds;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::fnv1a64;
use rtsim_ships::boundary::Cmd;
use rtsim_ships::engine::{LaneThreat, PState, PortState, ShipId, Status};
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
            ev.iter().map(|e| (e.tick, e.magnitude)).collect::<Vec<_>>(),
            vec![(blockaded.0, 1), (closed.0, 2)],
            "seed {seed}"
        );
        // The pirates move on; the port reopens after its minimum. From ten days after the
        // closure until then, the lanes carry nothing.
        let quiet = closed + 10 * TICKS_PER_DAY;
        let mut carried = false;
        assert!(
            run_until(&mut c, &mut fake, closed + 150 * TICKS_PER_DAY, |c, _| {
                if c.ships.stats.reopenings == 1 {
                    return true;
                }
                carried |= c.ships.now() >= quiet && lanes.iter().any(|&l| volume(c, l) > 0);
                false
            }),
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
                assert!(
                    back >= reopened,
                    "seed {seed}: relaunched before the reopening"
                );
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

/// Each change of a port that rule 7 tells is news in the world (§2.4.6a rule 8, PC10, PC100):
/// Blockade (17, severity 1), PortClosed (18, severity 2) and PortReopened (19, severity 1), one
/// report per `ships.port` tape row at the same tick (the lift, magnitude 4, is not news), placed
/// at the dock, by the haven whose pirates hold the lanes' heat against the port's village, and
/// known to that village; nothing goes to chat. Seeds 1 to 3 on the blockade coast over 300 days.
/// A port's news lives 6 days, so it is looked at each day and counted once by `(id, at)`.
#[test]
fn a_port_change_is_news() {
    use rtsim_knowledge::ReportKind;
    let tun = ShipTunables::default();
    for seed in 1..=3u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        c.ships.tape.set_on(true);
        let havens: Vec<_> = c
            .ships
            .havens
            .iter()
            .map(|h| c.world.nodes[h.node].actor)
            .collect();
        let mut seen = BTreeSet::new();
        let mut news = Vec::new();
        run_until(&mut c, &mut fake, Tick::from_days(300), |c, _| {
            if !c.ships.now().0.is_multiple_of(TICKS_PER_DAY) {
                return false;
            }
            let k = &c.world.knowledge;
            for (id, _) in k.reports().iter() {
                let r = k.report(id).expect("a known kind");
                let port = matches!(
                    r.kind,
                    ReportKind::Blockade | ReportKind::PortClosed | ReportKind::PortReopened
                );
                if port && seen.insert((id, r.at)) {
                    assert!(k.knows(r.whom, id), "seed {seed}: the village knows {r:?}");
                    news.push(r);
                }
            }
            false
        });
        let mut told = Vec::new();
        for r in &news {
            let d = (0..c.ships.docks.len() as u32)
                .find(|&d| {
                    let k = &c.ships.docks[d as usize];
                    (k.frame, k.x, k.z) == (r.place.frame, r.place.x, r.place.z)
                })
                .unwrap_or_else(|| panic!("seed {seed}: at a dock: {r:?}"));
            assert!(c.ships.is_trade_port(&c.world, d), "seed {seed}: {r:?}");
            let node = c.ships.docks[d as usize].node;
            assert_eq!(r.whom, c.world.nodes[node].actor, "seed {seed}: the port");
            assert!(havens.contains(&r.who), "seed {seed}: by a haven: {r:?}");
            let (magnitude, severity) = match r.kind {
                ReportKind::Blockade => (1, 1),
                ReportKind::PortClosed => (2, 2),
                _ => (3, 1),
            };
            assert_eq!((r.severity, r.flags), (severity, 0), "seed {seed}: {r:?}");
            told.push((d, r.at.0, magnitude));
        }
        let mut taped: Vec<_> = (0..c.ships.docks.len() as u32)
            .flat_map(|d| {
                port_events(&c.ships, d)
                    .into_iter()
                    .filter(|e| e.magnitude != 4)
                    .map(move |e| (d, e.tick, e.magnitude))
            })
            .collect();
        told.sort();
        taped.sort();
        assert_eq!(told, taped, "seed {seed}: one report per change");
        let st = &c.ships.stats;
        assert!(st.closures > 0 && st.reopenings > 0, "seed {seed}: {st:?}");
        assert_eq!(
            told.len() as u64,
            st.blockades + st.closures + st.reopenings,
            "seed {seed}"
        );
    }
}

/// The rule's bands, pure (PC21, PC82): two lanes whose share is on for six days and off for six,
/// with the peak chosen so the 30-day heat hovers on each threshold in turn (an eighth, a
/// quarter, half of full), checked every six days on the share's edges, where the heat's ripple
/// puts it above the threshold at one check and below it at the next. A rule with one threshold
/// per state would change at nearly every check; [`PortState::step`] changes at most once per
/// threshold, because the threshold that ends each state lies below the one that began it.
fn hover(full: i64, centre: i64, tun: &ShipTunables) -> (usize, usize, PortState) {
    let tau = tun.blockade_days * TICKS_PER_DAY as f64;
    let check = days(tun.blockade_check_days);
    let min_closed = days(tun.blockade_min_closed_days);
    let mut lanes = [LaneThreat::default(), LaneThreat::default()];
    let mut p = PortState::default();
    let (mut changes, mut naive_flips) = (0, 0);
    let mut naive_above: Option<bool> = None;
    let mut t = Tick::ZERO;
    while t < Tick::from_days(600) {
        let on = (t.0 / check).is_multiple_of(2);
        for th in &mut lanes {
            let share = if on { 2 * centre } else { 0 };
            if share != th.share {
                th.heat_nano = th.heat_nano_at(t, tau);
                th.heat_at = t;
                th.share = share;
            }
        }
        let heats: Vec<i64> = lanes.iter().map(|th| th.heat(t, tau)).collect();
        changes += p.step(&heats, full, min_closed, 0, t).len();
        let above = heats.iter().all(|&h| h >= centre);
        if naive_above.is_some_and(|a| a != above) {
            naive_flips += 1;
        }
        naive_above = Some(above);
        t += check;
    }
    (changes, naive_flips, p)
}

/// The hysteresis, pure and on the coast. The pure half is the real test of it ([`hover`]):
/// heats hovering on each threshold flip a one-threshold rule at most checks but change the port
/// at most once; and a reopened port whose heat comes straight back cannot close again before
/// `min_open` has passed (PC99), with 0 closing it at the next check. The coast half (seeds 1 to
/// 4 for 400 days with the pirates' own comings and goings, their applied share jumping between
/// 0 and three times full within days) checks the engine plays the rule as written: the tape
/// shows only the legal steps (open, blockaded, closed, reopened, no longer blockaded), each
/// closed spell lasts at least `blockade_min_closed_days`, each open spell after a reopening at
/// least `blockade_min_open_days`, and no two changes come closer than a check.
#[test]
fn a_blockade_cannot_flap() {
    let tun = ShipTunables::default();
    let full = synth::coastal(1, tun.clone()).ships.full_share(Medium::Sea);
    for (centre, want) in [
        (full / 8, None),
        (full / 4, Some((true, false))),
        (full / 2, Some((true, true))),
    ] {
        let (changes, naive, p) = hover(full, centre, &tun);
        assert!(
            naive >= 40,
            "centre {centre}: the heat crossed it only {naive} times"
        );
        let state = p
            .blockaded
            .map(|_| (p.blockaded.is_some(), p.closed.is_some()));
        assert!(
            changes <= 2 && (want.is_none() || state == want),
            "centre {centre}: {changes} changes, {p:?}"
        );
        eprintln!(
            "heat hovering on {centre}: a one-threshold rule flips {naive} times, the port changes {changes} times"
        );
    }

    // A reopened port whose heat is back at full the next check: blockaded at once, closed only
    // once its nerve is back (min_open after the reopening).
    let check = days(tun.blockade_check_days);
    let min_closed = days(tun.blockade_min_closed_days);
    for min_open in [0, 10 * TICKS_PER_DAY, 48 * TICKS_PER_DAY] {
        let mut p = PortState::default();
        let mut t = Tick::ZERO;
        p.step(&[full, full], full, min_closed, min_open, t);
        assert!(p.closed == Some(t));
        t = Tick(min_closed);
        p.step(&[0, 0], full, min_closed, min_open, t);
        assert!(p.closed.is_none() && p.blockaded.is_none(), "{p:?}");
        let reopened = t;
        loop {
            t += check;
            p.step(&[full, full], full, min_closed, min_open, t);
            assert!(p.blockaded.is_some(), "{p:?}");
            if p.closed.is_some() {
                break;
            }
        }
        assert!(
            t.0 >= reopened.0 + min_open && t.0 < reopened.0 + min_open.max(check) + check,
            "min_open {min_open}: closed again {} ticks after the reopening",
            t.0 - reopened.0
        );
        assert!(p.reopened.is_none(), "{p:?}");
    }

    let min_open = (tun.blockade_min_open_days * TICKS_PER_DAY as f64).round() as u64;
    let mut closures = 0;
    for seed in 1..=4u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        c.ships.tape.set_on(true);
        let half = c.ships.full_share(Medium::Sea) / 2;
        let lanes = c.ships.dock_lanes(TARGET).to_vec();
        let mut crossings = 0;
        let mut above = false;
        run_until(&mut c, &mut fake, Tick::from_days(400), |c, _| {
            let now = lanes.iter().all(|&l| c.ships.lane_share(l) >= half);
            if now != above {
                crossings += 1;
                above = now;
            }
            false
        });
        let ev = port_events(&c.ships, TARGET);
        // Replay the tape: blockaded, and closed since.
        let (mut blockaded, mut closed): (bool, Option<u64>) = (false, None);
        let (mut last_change, mut reopened): (Option<u64>, Option<u64>) = (None, None);
        for e in &ev {
            match e.magnitude {
                1 => {
                    assert!(!blockaded, "seed {seed}: blockaded twice: {ev:?}");
                    blockaded = true;
                }
                2 => {
                    assert!(blockaded && closed.is_none(), "seed {seed}: {ev:?}");
                    assert!(
                        reopened.is_none_or(|r| e.tick >= r + min_open),
                        "seed {seed}: closed again {} ticks after reopening",
                        e.tick - reopened.unwrap_or(0)
                    );
                    closed = Some(e.tick);
                    closures += 1;
                }
                3 => {
                    let since = closed.take().expect("closed before reopening");
                    reopened = Some(e.tick);
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
        let shut = ev.iter().filter(|e| e.magnitude == 2).count() as u64;
        assert!(
            changes <= crossings,
            "seed {seed}: the share crossed half {crossings} times, the port changed {changes}"
        );
        assert!(
            shut <= 1 + 400 / (min_closed / TICKS_PER_DAY + 6),
            "seed {seed}: {shut} closures"
        );
        eprintln!(
            "seed {seed}: the raw share crossed half {crossings} times; the port changed {changes} \
             times ({} closures)",
            ev.iter().filter(|e| e.magnitude == 2).count()
        );
    }
    assert!(closures > 0, "no closure to judge");
}

/// The world and ships read back from their saves, as a restarted engine would.
fn restarted(c: &Coastal, seed: u64, tun: &ShipTunables) -> Coastal {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    Coastal {
        world: rtsim_world::World::load(synth::world_config(seed), goods, book, &c.world.save())
            .expect("world loads"),
        ships: Ships::load(tun.clone(), &c.ships.save()).expect("ships load"),
        mask: synth::coast(),
        haven: c.haven,
        navy: c.navy,
    }
}

/// When dock 3 first closes on seed `seed`'s blockade coast (nobody watching).
fn closing_tick(seed: u64, tun: &ShipTunables) -> Tick {
    let (mut c, mut fake) = blockade_coast(seed, tun);
    assert!(run_until(&mut c, &mut fake, Tick::from_days(90), |c, _| c
        .ships
        .port_closed(TARGET)));
    c.ships.port(TARGET).and_then(|p| p.closed).expect("closed")
}

/// The blockade coast run to the step before its closure, then every active merchant of a
/// service calling at dock 3 promoted (its voyage's consignments out of the flow, as a loot or a
/// raid in view does). Returns the coast and the promoted ships holding cargo whose voyage ends
/// after the closure.
fn promoted_before_closure(
    seed: u64,
    tun: &ShipTunables,
) -> (Coastal, FakeShips, Vec<ShipId>, Tick) {
    let at = closing_tick(seed, tun);
    let (mut c, mut fake) = blockade_coast(seed, tun);
    let before = Tick((at.0 - 1) / STEP * STEP);
    run_until(&mut c, &mut fake, before, |_, _| false);
    let ships = fleet(&c.ships, &calling(&c.ships, TARGET));
    let mut held = Vec::new();
    for id in ships {
        if c.ships.ship(id).is_some_and(|x| x.is_active())
            && c.ships.promote(&mut c.world, id, before)
        {
            let x = c.ships.ship(id).expect("a ship");
            if !x.cargo.is_empty() && x.promoted.is_some_and(|p| p.arrive > at) {
                held.push(id);
            }
        }
    }
    (c, fake, held, at)
}

/// PC23 and PC50: a ship holding promoted cargo when its port closes sails on and lands the
/// cargo, and lays up only then. Seeds 1 to 3: every merchant of dock 3's services is promoted
/// just before the closure; at the closure those whose voyage is still under way sail on (the
/// rest lay up at once); each lands its whole hold (or loses what a port cannot stock) at its
/// voyage's end, and is laid up at that tick, holding nothing. Goods are conserved (the daily
/// checks).
#[test]
fn a_promoted_cargo_lands_before_its_ship_lays_up() {
    let tun = ShipTunables::default();
    let mut landed_any = 0;
    for seed in 1..=3u64 {
        let (mut c, mut fake, held, _) = promoted_before_closure(seed, &tun);
        assert!(
            !held.is_empty(),
            "seed {seed}: no promoted ship holds cargo"
        );
        let limit = c.ships.now() + 20 * TICKS_PER_DAY;
        assert!(
            run_until(&mut c, &mut fake, limit, |c, _| c.ships.port_closed(TARGET)),
            "seed {seed}"
        );
        let closed = c.ships.port(TARGET).and_then(|p| p.closed).expect("closed");
        let mut waiting: Vec<(ShipId, Tick, i128)> = held
            .iter()
            .filter_map(|&id| {
                let x = c.ships.ship(id)?;
                let p = x.promoted?;
                (x.is_active() && p.arrive > closed)
                    .then(|| (id, p.arrive, x.cargo.values().map(|&a| i128::from(a)).sum()))
            })
            .collect();
        assert!(
            !waiting.is_empty(),
            "seed {seed}: no promoted ship sails past the closure"
        );
        waiting.sort_by_key(|w| (w.1, w.0));
        for &(id, _, _) in &waiting {
            assert!(c.ships.waiting_to_lay_up(id));
            assert!(!c.ships.ship(id).expect("a ship").is_laid_up());
        }
        let gone = |s: &Ships| {
            let l = &s.ledger;
            [
                &l.landed_at_ports,
                &l.sunk,
                &l.taken_by_players,
                &l.looted_by_pirates,
            ]
            .iter()
            .flat_map(|m| m.values())
            .sum::<i128>()
        };
        let before = gone(&c.ships);
        for &(id, arrive, _) in &waiting {
            // Just before its arrival it still sails, holding its cargo.
            if c.ships.now() < arrive {
                run_until(&mut c, &mut fake, Tick(arrive.0 - 1), |_, _| false);
                let x = c.ships.ship(id).expect("a ship");
                assert!(x.is_active() && !x.cargo.is_empty(), "seed {seed}: {x:?}");
            }
            run_until(&mut c, &mut fake, arrive + STEP, |_, _| false);
            let x = c.ships.ship(id).expect("a ship");
            assert!(
                x.cargo.is_empty() && x.promoted.is_none(),
                "seed {seed}: {x:?}"
            );
            assert!(
                matches!(x.status, Status::Lost { since, .. } if since == arrive) && x.is_laid_up(),
                "seed {seed}: not laid up at its landing: {x:?}"
            );
            landed_any += 1;
        }
        let held: i128 = waiting.iter().map(|w| w.2).sum();
        assert!(
            gone(&c.ships) - before >= held,
            "seed {seed}: the holds went nowhere"
        );
        c.ships.check(&c.world).expect("checks");
    }
    assert!(landed_any > 0);
}

const PLAYER: (u64, u64) = (0xB10C, 0xADE);

/// Puts the player on ship `s` (where the engine has it now), for the plugin and the engine.
fn ride(c: &mut Coastal, fake: &mut FakeShips, s: ShipId, t: Tick) {
    let (x, z) = c.ships.position(&c.world, s, t);
    fake.set_player(PLAYER, f64::from(x), f64::from(z));
    c.ships
        .observe_player(&mut c.world, PLAYER, rtsim_settle::Pos::new(0, x, z), t);
}

/// Seed `seed`'s blockade coast with a player riding a merchant of dock 3's services (the first
/// active one holding nothing promoted a day before the closure; he stands on it, observed every
/// `step` ticks) until `until` (`None`: three days after the closure, the closure's tick found
/// first). Once the port is closed the ridden ship must stay active and embodied. Returns the
/// coast, the plugin, the ship and the closure, or `None` when the ship was out of view at the
/// closure.
fn ride_through_closure(
    seed: u64,
    tun: &ShipTunables,
    until: impl Fn(&Coastal, Tick) -> bool,
    step: u64,
) -> Option<(Coastal, FakeShips, ShipId, Tick)> {
    let at = closing_tick(seed, tun);
    let (mut c, mut fake) = blockade_coast(seed, tun);
    let from = Tick((at.0 - TICKS_PER_DAY) / STEP * STEP);
    run_until(&mut c, &mut fake, from, |_, _| false);
    let ships = fleet(&c.ships, &calling(&c.ships, TARGET));
    let m = ships.iter().copied().find(|&id| {
        c.ships
            .ship(id)
            .is_some_and(|x| x.is_active() && x.promoted.is_none())
    })?;
    let agent = rtsim_ships::Ship::agent(m);
    let mut t = from;
    let mut in_view_at_closure = false;
    while t < at || !until(&c, at) {
        ride(&mut c, &mut fake, m, t);
        t += step;
        go(&mut c, &mut fake, t);
        let x = c.ships.ship(m).expect("a ship");
        if c.ships.port(TARGET).is_some_and(|p| p.closed == Some(at)) {
            if !in_view_at_closure {
                in_view_at_closure = fake.bodies.contains_key(&agent);
                if !in_view_at_closure {
                    return None;
                }
            }
            assert!(
                x.is_active() && x.embodied,
                "seed {seed}: the ridden ship stopped: {x:?}"
            );
        }
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            c.ships.check(&c.world).expect("checks");
        }
    }
    in_view_at_closure.then_some((c, fake, m, at))
}

/// The player steps ashore far away: his interest area goes, and the plugin drops what he saw.
fn ashore(c: &mut Coastal, fake: &mut FakeShips) {
    fake.players.clear();
    let p = c.ships.player(&mut c.world, PLAYER);
    c.ships.player_left(&mut c.world, p);
}

/// PC23 with the fake plugin: a ship in view when its port closes keeps its timetable, is never
/// despawned, and lays up only once it has left view. Seeds 1 to 3: a player rides a merchant of
/// dock 3's services (stands on it, step by step) from a day before the closure to three days
/// after; the other ships of the port's services lay up at the closure, this one sails on, its
/// hold empty from the first voyage after the closure. The player steps ashore far away: the
/// body leaves view, the plugin reports it dematerialized, and the ship is laid up at that
/// report.
#[test]
fn a_ship_in_view_lays_up_only_after_it_leaves_view() {
    let tun = ShipTunables::default();
    let mut judged = 0;
    for seed in 1..=3u64 {
        let Some((mut c, mut fake, m, _)) = ride_through_closure(
            seed,
            &tun,
            |c, at| c.ships.now() >= at + 3 * TICKS_PER_DAY,
            50,
        ) else {
            continue;
        };
        let agent = rtsim_ships::Ship::agent(m);
        assert!(c.ships.waiting_to_lay_up(m));
        let ships = fleet(&c.ships, &calling(&c.ships, TARGET));
        let others = ships
            .iter()
            .filter(|&&id| id != m && c.ships.ship(id).is_some_and(|x| x.is_laid_up()))
            .count();
        assert!(others > 0, "seed {seed}: no other ship laid up");
        assert!(
            fake.bodies[&agent].cargo.is_empty(),
            "seed {seed}: a laid-up service's body holds cargo"
        );
        assert_eq!(
            fake.commands.get(&rtsim_ships::boundary::SHIP_DESPAWN),
            None,
            "seed {seed}: a despawn was sent"
        );
        let laid = c.ships.stats.ships_laid_up_by_closure;
        ashore(&mut c, &mut fake);
        let before = fake.dematerialized;
        let t = c.ships.now() + 50;
        go(&mut c, &mut fake, t);
        assert!(fake.dematerialized > before);
        assert!(!fake.bodies.contains_key(&agent));
        let x = c.ships.ship(m).expect("a ship");
        assert!(x.is_laid_up(), "seed {seed}: {x:?}");
        assert_eq!(c.ships.stats.ships_laid_up_by_closure, laid + 1);
        assert_eq!(
            fake.commands.get(&rtsim_ships::boundary::SHIP_DESPAWN),
            None
        );
        c.ships.check(&c.world).expect("checks");
        judged += 1;
    }
    assert!(
        judged > 0,
        "no seed had the ridden ship in view at the closure"
    );
}

/// A ship waiting to lay up (in view, its service laid up) that the player sinks is lost as any
/// sunk merchant is: its hold goes down, and its slot's respawn falls due a few days later. The
/// port is still closed then, so the slot lays up in port instead of sailing; the reopening
/// relaunches it with the rest. Seeds 1 to 3, the engine checked daily throughout.
#[test]
fn a_ship_sunk_while_it_waits_to_lay_up_lays_up_at_its_respawn() {
    let tun = ShipTunables::default();
    let mut judged = 0;
    for seed in 1..=3u64 {
        let Some((mut c, mut fake, m, closed)) =
            ride_through_closure(seed, &tun, |c, at| c.ships.now() >= at + TICKS_PER_DAY, 50)
        else {
            continue;
        };
        assert!(c.ships.waiting_to_lay_up(m));
        let t = c.ships.now();
        let sunk = c.ships.stats.sinkings;
        let o = fake
            .player_sinks(rtsim_ships::Ship::agent(m), PLAYER)
            .expect("a body to sink");
        c.ships.ingest(&mut c.world, &o, &c.mask, t);
        assert_eq!(c.ships.stats.sinkings, sunk + 1);
        let x = c.ships.ship(m).expect("a ship");
        let Status::Lost { back, .. } = x.status else {
            panic!("seed {seed}: not lost: {x:?}");
        };
        assert!(!x.is_laid_up() && x.cargo.is_empty(), "seed {seed}: {x:?}");
        c.ships.check(&c.world).expect("checks after the sinking");
        ashore(&mut c, &mut fake);
        let laid = c.ships.stats.ships_laid_up_by_closure;
        run_until(&mut c, &mut fake, back + STEP, |_, _| false);
        assert!(
            c.ships.port_closed(TARGET),
            "seed {seed}: reopened before the respawn"
        );
        let x = c.ships.ship(m).expect("a ship");
        assert!(
            matches!(x.status, Status::Lost { since, .. } if since == back) && x.is_laid_up(),
            "seed {seed}: not laid up at its respawn: {x:?}"
        );
        assert_eq!(c.ships.stats.ships_laid_up_by_closure, laid + 1);
        assert!(run_until(
            &mut c,
            &mut fake,
            closed + 150 * TICKS_PER_DAY,
            |c, _| !c.ships.port_closed(TARGET)
        ));
        let x = c.ships.ship(m).expect("a ship");
        assert!(!x.is_laid_up(), "seed {seed}: not relaunched: {x:?}");
        judged += 1;
    }
    assert!(
        judged > 0,
        "no seed had the ridden ship in view at the closure"
    );
}

/// A ship still in view when its port reopens simply trades again (PC23, PC81): its service is
/// no longer laid up, so nothing marks its voyages laid up, no relaunch is booked for it, and
/// when the player leaves it sails on (it is never laid up, despawned or relaunched). Seeds 1
/// to 3: the player rides it from a day before the closure until two days after the reopening.
#[test]
fn a_ship_in_view_at_the_reopening_sails_on() {
    let tun = ShipTunables::default();
    let mut judged = 0;
    for seed in 1..=3u64 {
        let Some((mut c, mut fake, m, _)) =
            ride_through_closure(seed, &tun, |c, _| c.ships.stats.reopenings > 0, 100)
        else {
            continue;
        };
        let reopened = c.ships.now();
        let v = c.ships.ship(m).and_then(|x| x.service).expect("a service");
        assert!(!c.ships.service_laid_up(v) && c.ships.laid_up_since(v).is_none());
        assert!(!c.ships.waiting_to_lay_up(m));
        let x = c.ships.ship(m).expect("a ship");
        assert!(x.is_active() && x.embodied, "seed {seed}: {x:?}");
        assert!(
            !c.ships.pending_by_kind().is_empty()
                && fleet(&c.ships, &[v])
                    .iter()
                    .any(|&id| id != m && c.ships.ship(id).is_some_and(|x| !x.is_active())),
            "seed {seed}: the other slots lie in port until their relaunch"
        );
        let laid = c.ships.stats.ships_laid_up_by_closure;
        let end = reopened + 2 * TICKS_PER_DAY;
        let mut t = reopened;
        while t < end {
            ride(&mut c, &mut fake, m, t);
            t += 100;
            go(&mut c, &mut fake, t);
        }
        let x = c.ships.ship(m).expect("a ship");
        assert!(x.is_active() && x.embodied, "seed {seed}: {x:?}");
        ashore(&mut c, &mut fake);
        run_until(&mut c, &mut fake, t + 2 * TICKS_PER_DAY, |_, _| false);
        let x = c.ships.ship(m).expect("a ship");
        assert!(x.is_active() && !x.embodied, "seed {seed}: {x:?}");
        assert_eq!(c.ships.stats.ships_laid_up_by_closure, laid);
        assert_eq!(
            fake.commands.get(&rtsim_ships::boundary::SHIP_DESPAWN),
            None
        );
        judged += 1;
    }
    assert!(
        judged > 0,
        "no seed had the ridden ship in view at the closure"
    );
}

/// A shuttle built to a port while it is closed ([`Ships::connect`], a port joining late) is
/// laid up from the start: its services laid up since the joining, their lane shut and closed in
/// the world, their ships in port; the engine's checks hold. When the port reopens the shuttle
/// opens with it and its ships relaunch. Seed 1 (the closure found first); the shuttle is the one
/// `connect` builds from the closed port to its nearest dock.
#[test]
fn a_shuttle_to_a_closed_port_starts_laid_up() {
    let tun = ShipTunables::default();
    let seed = 1;
    let at = closing_tick(seed, &tun);
    let (mut c, mut fake) = blockade_coast(seed, &tun);
    run_until(&mut c, &mut fake, at + TICKS_PER_DAY, |_, _| false);
    assert!(c.ships.port_closed(TARGET));
    let now = c.ships.now();
    let first = c.ships.services.len() as u32;
    let l = c
        .ships
        .connect(&mut c.world, TARGET, &c.mask, now)
        .expect("a shuttle");
    let shuttle: Vec<u32> = (first..c.ships.services.len() as u32).collect();
    assert!(!shuttle.is_empty());
    assert!(c.ships.lane_shut(l));
    assert!(c.world.lane_closed(c.ships.lanes[l as usize].edge));
    for &v in &shuttle {
        assert!(c.ships.service_laid_up(v));
        assert_eq!(c.ships.laid_up_since(v), Some(now), "service {v}");
    }
    let ships = fleet(&c.ships, &shuttle);
    assert!(!ships.is_empty());
    for &id in &ships {
        let x = c.ships.ship(id).expect("a ship");
        assert!(x.is_laid_up(), "{x:?}");
    }
    c.ships.check(&c.world).expect("checks after the shuttle");
    assert!(run_until(
        &mut c,
        &mut fake,
        at + 150 * TICKS_PER_DAY,
        |c, _| !c.ships.port_closed(TARGET)
    ));
    assert!(!c.ships.lane_shut(l));
    assert!(!c.world.lane_closed(c.ships.lanes[l as usize].edge));
    let t = c.ships.now() + 6 * TICKS_PER_DAY;
    run_until(&mut c, &mut fake, t, |_, _| false);
    for &id in &ships {
        let x = c.ships.ship(id).expect("a ship");
        assert!(x.is_active(), "not relaunched: {x:?}");
    }
}

/// Persistence (rule 6): a restart in each blockade state, compared by `state_hash` with the run
/// that was never saved (the one-blob form, and the paged form's meta page), then both carried
/// on 120 days. Three states: blockaded and open; closed
/// with a promoted ship still sailing (waiting to lay up); closed past its minimum. Each state is
/// left the way the rule says within the 120 days, so the comparison covers it.
#[test]
fn closures_survive_save_and_restore() {
    let tun = ShipTunables::default();
    let mut hits = [0; 3];
    for seed in 1..=3u64 {
        for (state, hit) in hits.iter_mut().enumerate() {
            let (mut a, mut fa, waiting) = if state == 1 {
                let (mut c, mut f, held, _) = promoted_before_closure(seed, &tun);
                let limit = c.ships.now() + 20 * TICKS_PER_DAY;
                assert!(run_until(&mut c, &mut f, limit, |c, _| c
                    .ships
                    .port_closed(TARGET)));
                let w: Vec<ShipId> = held
                    .into_iter()
                    .filter(|&id| c.ships.waiting_to_lay_up(id))
                    .collect();
                (c, f, w)
            } else {
                let (c, f) = blockade_coast(seed, &tun);
                (c, f, Vec::new())
            };
            let min = days(tun.blockade_min_closed_days);
            let reached = match state {
                0 => run_until(&mut a, &mut fa, Tick::from_days(90), |c, _| {
                    c.ships
                        .port(TARGET)
                        .is_some_and(|p| p.blockaded.is_some() && p.closed.is_none())
                }),
                1 => !waiting.is_empty(),
                _ => run_until(&mut a, &mut fa, Tick::from_days(150), |c, _| {
                    c.ships
                        .port(TARGET)
                        .and_then(|p| p.closed)
                        .is_some_and(|s| c.ships.now() > s + min)
                }),
            };
            if !reached {
                continue;
            }
            let before = *a.ships.port(TARGET).expect("a state");
            let stats = a.ships.stats;
            let mut b = restarted(&a, seed, &tun);
            assert_eq!(b.ships.ports, a.ships.ports, "seed {seed} state {state}");
            assert_eq!(b.ships.state_hash(), a.ships.state_hash());
            assert_eq!(b.world.state_hash(), a.world.state_hash());
            assert_eq!(b.ships.pending_by_kind(), a.ships.pending_by_kind());
            assert_eq!(b.ships.stats.closures, stats.closures);
            b.ships.check(&b.world).expect("restored checks");
            // The paged form too: the meta page carries the same trailing blockades section.
            {
                let mut q = restarted(&a, seed, &tun);
                let dir = std::env::temp_dir().join(format!(
                    "rtsim-ships-blockade-{}-{seed}-{state}",
                    std::process::id()
                ));
                let _ = std::fs::remove_dir_all(&dir);
                let store = Arc::new(RwLock::new(
                    PageStore::open(&dir, Durability::NoSync).expect("a store"),
                ));
                let meta = PageKey::new(0x5300, 1);
                q.ships.attach_store(store.clone(), 1 << 20);
                let mut batch = SaveBatch::new();
                let t = q.ships.save_paged(&mut batch, meta);
                store.write().unwrap().commit(batch, 1).expect("commit");
                q.ships.saved(t);
                let opened =
                    Ships::open_paged(store, 1 << 20, meta, tun.clone()).expect("opens paged");
                assert_eq!(
                    opened.ports, a.ships.ports,
                    "seed {seed} state {state}: paged"
                );
                assert_eq!(opened.state_hash(), a.ships.state_hash(), "paged");
                let _ = std::fs::remove_dir_all(&dir);
            }
            let mut fb = FakeShips::new(f64::from(tun.view_radius));
            fb.bodies = fa.bodies.clone();
            fb.seq = fa.seq;
            let end = a.ships.now() + 120 * TICKS_PER_DAY;
            run_until(&mut a, &mut fa, end, |_, _| false);
            run_until(&mut b, &mut fb, end, |_, _| false);
            assert_eq!(
                b.ships.state_hash(),
                a.ships.state_hash(),
                "seed {seed} state {state}: the restored run went its own way"
            );
            assert_eq!(b.world.state_hash(), a.world.state_hash());
            assert_eq!(b.ships.ports, a.ships.ports);
            // The state's way out happened.
            let st = a.ships.stats;
            match state {
                0 => assert!(
                    st.closures > stats.closures
                        || a.ships.stats.blockades > stats.blockades
                        || a.ships
                            .port(TARGET)
                            .is_some_and(|p| p.blockaded != before.blockaded),
                    "seed {seed}: still blockaded"
                ),
                1 => assert!(
                    waiting
                        .iter()
                        .all(|&id| a.ships.ship(id).is_some_and(|x| x.promoted.is_none())),
                    "seed {seed}"
                ),
                _ => assert!(
                    st.reopenings > stats.reopenings,
                    "seed {seed}: never reopened"
                ),
            }
            *hit += 1;
        }
    }
    assert!(hits.iter().all(|&h| h > 0), "states reached: {hits:?}");
    eprintln!("restarts judged per state: {hits:?}");
}

/// The hashes of the parent for three runs: the coast's scripted player for 30 days (seed 7)
/// and two seas unattended for 96 days, each over the ships' and the world's save bytes after
/// the format's magic (so a format bump alone changes nothing here). The parent is main at
/// 18356257 (S8b-2, S8b-3, RTS3, TALK slice 2a and the ports' news landed; both sea runs'
/// hashes differ from 257d2715's, the coast's do not) with this branch's world half (S8b-4a,
/// `crates/rtsim-world`
/// and `crates/rtsim-travel`, which main does not have: its edge record has no closed flag), so
/// the world hashes compare the same layout. Printed by the same scenarios built on that tree
/// (`git archive` of main, the world half patched in).
const PARENT: [(&str, u64, u64); 3] = [
    (
        "coastal 7 script 30 days",
        0xf921_e4e7_d97f_73b0,
        0xced9_aa1e_7c85_48da,
    ),
    (
        "sea_havens 1 unattended 96 days",
        0xb717_8258_8a21_df9f,
        0xaed4_77ab_b6c2_a2a5,
    ),
    (
        "sea_havens 2 unattended 96 days",
        0x6f8d_aca5_c777_938c,
        0x3ce5_5f9c_732b_0d30,
    ),
];

/// `blockade_days = 0` turns closures off and changes nothing: the ships' and the world's hashes
/// (of the save bytes after the magic) equal the parent's for the same runs, no port has a state, no lane has heat and no
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
        assert!(s.lane_threats().iter().all(|(_, t)| t.heat_nano == 0));
        assert_eq!(s.stats.port_checks, 0);
        assert!(!s.pending_by_kind().contains_key("dock_check"));
    };
    quiet(&c.ships);
    got.push((
        "coastal 7 script 30 days".into(),
        fnv1a64(&c.ships.save()[4..]),
        fnv1a64(&c.world.save()[4..]),
    ));
    for seed in [1u64, 2] {
        let mut sh = synth::sea_havens(seed, tun.clone());
        sh.ships
            .run_until(&mut sh.world, &mut NoInfrastructure, Tick::from_days(96));
        quiet(&sh.ships);
        got.push((
            format!("sea_havens {seed} unattended 96 days"),
            fnv1a64(&sh.ships.save()[4..]),
            fnv1a64(&sh.world.save()[4..]),
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

/// The pirates' hunts on the coast that began after `after` on one of `lanes`: (pirate, start).
fn hunts_begun(c: &Coastal, lanes: &[u32], after: Tick) -> Vec<(ShipId, Tick, u32)> {
    let mut out = Vec::new();
    for hv in &c.ships.havens {
        for &p in &hv.ships {
            if let Some(PState::Hunting { lane, since, .. }) = c
                .ships
                .ship(p)
                .filter(|x| x.is_active())
                .and_then(|x| x.pirate.as_ref())
                .map(|x| x.state)
                && since > after
                && lanes.contains(&lane)
            {
                out.push((p, since, lane));
            }
        }
    }
    out
}

/// PC91: a closed lane is not worth hunting. On the blockade coast, seeds 1 to 4, from the
/// closure to the reopening: no pirate begins a hunt on either of dock 3's lanes, although their
/// payments keep flowing for a while (which once made the lane look worth it).
#[test]
fn pirates_begin_no_hunt_on_a_closed_lane() {
    let tun = ShipTunables::default();
    let mut paid = 0;
    for seed in 1..=4u64 {
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        assert!(run_until(&mut c, &mut fake, Tick::from_days(90), |c, _| c
            .ships
            .port_closed(TARGET)));
        let closed = c.ships.port(TARGET).and_then(|p| p.closed).expect("closed");
        let lanes = c.ships.dock_lanes(TARGET).to_vec();
        let mut begun = Vec::new();
        let mut pay_seen = false;
        let reopened = run_until(&mut c, &mut fake, closed + 90 * TICKS_PER_DAY, |c, _| {
            if !c.ships.port_closed(TARGET) {
                return true;
            }
            for &l in &lanes {
                let e = &c.world.edges[c.ships.lanes[l as usize].edge as usize];
                pay_seen |= e
                    .dirs
                    .iter()
                    .any(|d| d.pay.values().any(|p| p.current_rate() > 0));
            }
            begun.extend(hunts_begun(c, &lanes, closed));
            false
        });
        assert!(reopened, "seed {seed}: never reopened");
        begun.sort();
        begun.dedup();
        assert!(
            begun.is_empty(),
            "seed {seed}: hunts begun on a closed lane after the closure (day {:.1}): {:?}",
            day(closed),
            begun
                .iter()
                .map(|&(p, t, l)| (p, day(t), l))
                .collect::<Vec<_>>()
        );
        paid += usize::from(pay_seen);
    }
    eprintln!("seeds whose closed lanes still carried payments: {paid} of 4");
}

/// Steps the coast by 50 ticks to `to` with a player standing at `at` (observed every step, as
/// the plugin's PLAYER record), checking the engine daily. Each `Materialize` of a ship in
/// `watch` after `from` passes to `seen` with the step's end.
#[allow(clippy::too_many_arguments)]
fn stand(
    c: &mut Coastal,
    fake: &mut FakeShips,
    at: (i32, i32),
    to: Tick,
    mut seen: impl FnMut(&Coastal, ShipId, (f64, f64), Tick),
    watch: &[ShipId],
) {
    fake.set_player(PLAYER, f64::from(at.0), f64::from(at.1));
    let mut t = c.ships.now();
    while t < to {
        c.ships.observe_player(
            &mut c.world,
            PLAYER,
            rtsim_settle::Pos::new(0, at.0, at.1),
            t,
        );
        t = (t + 50).min(to);
        c.ships.run_until(&mut c.world, &mut NoInfrastructure, t);
        let cmds = c.ships.take_commands();
        for cmd in &cmds {
            if let Cmd::Materialize(m) = cmd
                && let Some(id) = rtsim_ships::Ship::id_of_agent(m.agent)
                && watch.contains(&id)
            {
                seen(c, id, (m.pos.0, m.pos.2), t);
            }
        }
        let mut obs = fake.apply(&cmds, t);
        obs.extend(fake.step(t));
        for o in obs {
            c.ships.ingest(&mut c.world, &o, &c.mask, t);
        }
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            c.ships.check(&c.world).expect("checks");
        }
    }
}

/// PC90: a relaunch never happens in a player's view. Seeds 1 to 3; both of dock 3's services
/// start their loops at dock 0. A player stands at dock 0 from two days before the reopening to
/// four days after it: every slot's relaunch falls due while he watches, and every one is put
/// off to the slot's next stay there, so none of the laid-up ships appears. He then walks to
/// the middle of the services' first lane, out of sight of the dock, for six days: the ships
/// relaunch at dock 0 unseen and first materialize at the edge of his view as they sail into
/// it, never closer.
#[test]
fn a_relaunch_never_happens_in_view() {
    let tun = ShipTunables::default();
    let view = f64::from(tun.view_radius);
    let mut entered = 0;
    for seed in 1..=3u64 {
        let (mut c0, mut f0) = blockade_coast(seed, &tun);
        assert!(run_until(&mut c0, &mut f0, Tick::from_days(200), |c, _| c
            .ships
            .stats
            .reopenings
            > 0));
        let reopen = c0.ships.now();
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        let from = Tick((reopen.0 - 2 * TICKS_PER_DAY) / STEP * STEP);
        run_until(&mut c, &mut fake, from, |_, _| false);
        assert!(c.ships.port_closed(TARGET), "seed {seed}");
        let services = calling(&c.ships, TARGET);
        let laid: Vec<ShipId> = fleet(&c.ships, &services)
            .into_iter()
            .filter(|&id| c.ships.ship(id).is_some_and(|x| x.is_laid_up()))
            .collect();
        assert!(!laid.is_empty(), "seed {seed}");
        let home = c.ships.services[services[0] as usize].legs[0].from_dock;
        assert!(
            services
                .iter()
                .all(|&v| c.ships.services[v as usize].legs[0].from_dock == home),
            "seed {seed}: the services start their loops at different docks"
        );
        let dock = (
            c.ships.docks[home as usize].x,
            c.ships.docks[home as usize].z,
        );
        // At the home dock: nothing relaunches.
        let mut near = Vec::new();
        stand(
            &mut c,
            &mut fake,
            dock,
            reopen + 4 * TICKS_PER_DAY,
            |_, id, _, t| near.push((id, day(t))),
            &laid,
        );
        assert_eq!(c.ships.stats.reopenings, 1, "seed {seed}");
        let put_off = c.ships.stats.relaunches_put_off;
        assert!(
            put_off >= laid.len() as u64,
            "seed {seed}: {put_off} relaunches put off for {} slots",
            laid.len()
        );
        assert!(
            laid.iter()
                .all(|&id| c.ships.ship(id).is_some_and(|x| !x.is_active())),
            "seed {seed}: a slot relaunched at the dock he watched"
        );
        assert!(
            near.is_empty(),
            "seed {seed}: laid-up ships appeared: {near:?}"
        );
        // Mid-lane, out of sight of the dock: they relaunch, and sail into view.
        let ln = &c.ships.lanes[c.ships.services[services[0] as usize].legs[0].hop.lane as usize];
        let mid = ln.path.at(ln.path.length() / 2);
        let gap = f64::from(mid.0 - dock.0).hypot(f64::from(mid.1 - dock.1));
        assert!(
            gap > 2.0 * view,
            "seed {seed}: the lane's middle is {gap:.0} from the dock"
        );
        let mut first: Vec<(ShipId, f64, f64)> = Vec::new();
        stand(
            &mut c,
            &mut fake,
            mid,
            reopen + 10 * TICKS_PER_DAY,
            |_, id, pos, t| {
                if !first.iter().any(|f| f.0 == id) {
                    let d = (pos.0 - f64::from(mid.0)).hypot(pos.1 - f64::from(mid.1));
                    first.push((id, d, day(t)));
                }
            },
            &laid,
        );
        for &(id, d, at) in &first {
            assert!(
                d >= view,
                "seed {seed}: {id:?} first seen {d:.0} blocks from the player on day {at:.2}"
            );
        }
        let active = laid
            .iter()
            .filter(|&&id| c.ships.ship(id).is_some_and(|x| x.is_active()))
            .count();
        assert!(
            active * 4 >= laid.len() * 3,
            "seed {seed}: {active} of {} relaunched",
            laid.len()
        );
        entered += first.len();
        eprintln!(
            "seed {seed}: {} laid up, {put_off} relaunches put off at the watched dock, {active} \
             relaunched once he left it; first seen mid-lane (blocks from him, day): {:?}",
            laid.len(),
            first.iter().map(|f| (f.1.round(), f.2)).collect::<Vec<_>>()
        );
    }
    assert!(entered > 0, "no relaunched ship came into view");
}

// ---- many seas, nobody watching ------------------------------------------------------------------

/// The model's year (four 24-day seasons), as in `equilibrium.rs`.
const YEAR: u64 = 96;

/// `f` on each job, up to `threads` at a time on their own threads, each thread taking the next
/// job when it is free (the seas' runs differ a lot in length); the results in job order.
fn per_job<J: Sync, T: Send>(threads: usize, jobs: &[J], f: impl Fn(&J) -> T + Sync) -> Vec<T> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, threads));
    let next = AtomicUsize::new(0);
    let mut out: Vec<(usize, T)> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads)
            .map(|_| {
                let (f, next) = (&f, &next);
                sc.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(j) = jobs.get(i) else { break };
                        mine.push((i, f(j)));
                    }
                    mine
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("a sea's run panicked"))
            .collect()
    });
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, t)| t).collect()
}

/// One sea's unattended run with closures on or off, sampled every day.
#[derive(Clone, Debug, Default)]
struct Sea {
    world: &'static str,
    seed: u64,
    on: bool,
    years: u64,
    /// `blockade_min_open_days` the run used.
    min_open: f64,
    /// Trade ports with at least one lane (only these can close).
    ports: usize,
    navy_ships: usize,
    blockades: u64,
    closures: u64,
    reopenings: u64,
    laid_up: u64,
    /// Port-days closed: counted at the daily samples, and credited by the engine at reopening.
    closed_days: u64,
    closed_dock_days: u64,
    /// Each closed spell: (days closed, days from the closure to the last sample with a lane of
    /// the port above a quarter of full). A spell still running at the end counts as it stands.
    spells: Vec<(f64, f64)>,
    ever_closed: usize,
    /// Per medium (sea, sky): the trade ports, the days on which every one was closed, and of
    /// those the days a navy ship was sailing.
    media_ports: [usize; 2],
    all_closed: [u64; 2],
    all_closed_navy: [u64; 2],
    /// The largest share of the days one port spent closed.
    worst_port: f64,
    hunts: u64,
    decade_hunts: Vec<u64>,
    /// The mean share of haven sites open.
    open: f64,
}

/// `years` of `world` (`sea_havens` or `coastal`) for `seed` with nobody watching. Asserts as it
/// goes: the engine's checks every 24 days, no navy ship ever laid up, and no port closed longer
/// than its lanes stayed above a quarter plus the 30-day minimum, one check interval and the
/// day's sampling.
fn unattended(world: &'static str, seed: u64, on: bool, years: u64) -> Sea {
    unattended_with(world, seed, on, years, None)
}

/// [`unattended`] with `blockade_min_open_days` set to `min_open` (the default when `None`).
fn unattended_with(
    world: &'static str,
    seed: u64,
    on: bool,
    years: u64,
    min_open: Option<f64>,
) -> Sea {
    let base = ShipTunables::default();
    let tun = ShipTunables {
        blockade_days: if on { base.blockade_days } else { 0.0 },
        blockade_min_open_days: min_open.unwrap_or(base.blockade_min_open_days),
        ..base
    };
    let (mut w, mut ships, mask, havens): (_, _, Box<dyn rtsim_ships::water::WaterMask>, _) =
        match world {
            "sea_havens" => {
                let s = synth::sea_havens(seed, tun.clone());
                (s.world, s.ships, Box::new(s.mask), s.havens)
            }
            _ => {
                let c = synth::coastal(seed, tun.clone());
                (c.world, c.ships, Box::new(c.mask), vec![c.haven])
            }
        };
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let ports: Vec<(u32, Vec<u32>)> = (0..ships.docks.len() as u32)
        .filter(|&d| ships.is_trade_port(&w, d))
        .map(|d| {
            let lanes: Vec<u32> = ships
                .dock_lanes(d)
                .iter()
                .copied()
                .filter(|&l| ships.lanes[l as usize].edge != u32::MAX)
                .collect();
            (d, lanes)
        })
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let navy: Vec<ShipId> = (0..ships.services.len() as u32)
        .filter(|&v| ships.services[v as usize].kind == ServiceKind::Patrol)
        .flat_map(|v| ships.service_ships(v))
        .collect();
    let bound = tun.blockade_min_closed_days + tun.blockade_check_days + 1.0;
    // Each port's medium: 0 the sea, 1 the sky.
    let kinds: Vec<usize> = ports
        .iter()
        .map(|(d, _)| usize::from(ships.docks[*d as usize].medium != Medium::Sea))
        .collect();
    let mut sea = Sea {
        world,
        seed,
        on,
        years,
        ports: ports.len(),
        navy_ships: navy.len(),
        min_open: tun.blockade_min_open_days,
        ..Sea::default()
    };
    for &k in &kinds {
        sea.media_ports[k] += 1;
    }
    let mut port_days = vec![0u64; ports.len()];
    let mut spell: Vec<Option<(Tick, Tick)>> = vec![None; ports.len()];
    let mut ever = vec![false; ports.len()];
    let mut hunts0 = 0;
    let end = Tick::from_days(years * YEAR);
    let mut t = Tick::ZERO;
    let ended = |sea: &mut Sea, since: Tick, hot: Tick, t: Tick, d: u32| {
        let (len, hot) = (day(t) - day(since), day(hot) - day(since));
        assert!(
            len <= hot + bound,
            "{world} {seed}: port {d} closed {len:.1} days, its lanes hot {hot:.1}"
        );
        sea.spells.push((len, hot));
    };
    while t < end {
        t += TICKS_PER_DAY;
        synth::drive(
            &mut w,
            &mut ships,
            &mut fake,
            &mut NoInfrastructure,
            &*mask,
            t,
            STEP,
        );
        let navy_sails = navy
            .iter()
            .any(|&id| ships.ship(id).is_some_and(|x| x.is_active()));
        for &id in &navy {
            assert!(
                !ships.ship(id).is_some_and(|x| x.is_laid_up()),
                "{world} {seed}: navy ship {id:?} laid up"
            );
        }
        let mut medium_open = [false; 2];
        for (i, (d, lanes)) in ports.iter().enumerate() {
            let closed = ships.port(*d).and_then(|p| p.closed);
            let m = ships.docks[*d as usize].medium;
            medium_open[kinds[i]] |= closed.is_none();
            match (closed, spell[i]) {
                (Some(since), sp) => {
                    let (s0, mut hot) = match sp {
                        Some((s0, h)) if s0 == since => (s0, h),
                        Some((s0, h)) => {
                            ended(&mut sea, s0, h, t, *d);
                            (since, since)
                        }
                        None => (since, since),
                    };
                    let quarter = ships.full_share(m) / 4;
                    if lanes.iter().any(|&l| ships.heat_at(l, t) > quarter) {
                        hot = t;
                    }
                    spell[i] = Some((s0, hot));
                    ever[i] = true;
                    port_days[i] += 1;
                    sea.closed_days += 1;
                }
                (None, Some((s0, hot))) => {
                    ended(&mut sea, s0, hot, t, *d);
                    spell[i] = None;
                }
                (None, None) => {}
            }
        }
        for (k, open) in medium_open.into_iter().enumerate() {
            if sea.media_ports[k] > 0 && !open {
                sea.all_closed[k] += 1;
                sea.all_closed_navy[k] += u64::from(navy_sails);
            }
        }
        let open = havens
            .iter()
            .filter(|&&h| ships.havens[h as usize].shut.is_none())
            .count();
        sea.open += open as f64 / havens.len() as f64;
        if t.0.is_multiple_of(24 * TICKS_PER_DAY) {
            ships
                .check(&w)
                .unwrap_or_else(|e| panic!("{world} {seed} closures {on} day {}: {e}", day(t)));
        }
        if t.0.is_multiple_of(10 * YEAR * TICKS_PER_DAY) {
            sea.decade_hunts.push(ships.stats.hunts - hunts0);
            hunts0 = ships.stats.hunts;
        }
    }
    for (i, sp) in spell.iter().enumerate() {
        if let Some((s0, hot)) = *sp {
            ended(&mut sea, s0, hot, t, ports[i].0);
        }
    }
    sea.open /= (years * YEAR) as f64;
    sea.ever_closed = ever.iter().filter(|&&e| e).count();
    sea.worst_port = port_days.iter().copied().max().unwrap_or(0) as f64 / (years * YEAR) as f64;
    let st = &ships.stats;
    sea.blockades = st.blockades;
    sea.closures = st.closures;
    sea.reopenings = st.reopenings;
    sea.laid_up = st.ships_laid_up_by_closure;
    sea.closed_dock_days = st.closed_dock_days;
    sea.hunts = st.hunts;
    sea
}

fn report_seas(runs: &[Sea]) {
    for s in runs {
        let longest = s.spells.iter().map(|x| x.0).fold(0.0, f64::max);
        let over = s.spells.iter().map(|x| x.0 - x.1).fold(0.0, f64::max);
        eprintln!(
            "{} {:>2} closures {:<5} min-open {} ports {:>2} navy ships {}: blockades {}, closures {}, \
             reopenings {}, laid up {}, port-days closed {} (engine {}; most closed port \
             {:.3}), longest {longest:.0} days (at most {over:.0} past its hot lanes), ports \
             ever closed {}, days every port closed: sea {} of {} ports (navy sailing {}), sky \
             {} of {} ports (navy sailing {}), hunts/yr {:.1}, fewest hunts in a decade {}, \
             havens open {:.3}",
            s.world,
            s.seed,
            s.on,
            s.min_open,
            s.ports,
            s.navy_ships,
            s.blockades,
            s.closures,
            s.reopenings,
            s.laid_up,
            s.closed_days,
            s.closed_dock_days,
            s.worst_port,
            s.ever_closed,
            s.all_closed[0],
            s.media_ports[0],
            s.all_closed_navy[0],
            s.all_closed[1],
            s.media_ports[1],
            s.all_closed_navy[1],
            s.hunts as f64 / s.years as f64,
            s.decade_hunts.iter().min().copied().unwrap_or(0),
            s.open,
        );
    }
}

/// One world's seas together, closures on against off.
fn summary_seas(world: &str, on: &[Sea], off: &[Sea]) {
    let n = on.len() as f64;
    let years = on.first().map_or(0, |s| s.years);
    let sum = |v: &[Sea], f: &dyn Fn(&Sea) -> f64| v.iter().map(f).sum::<f64>();
    let spells: Vec<(f64, f64)> = on.iter().flat_map(|s| s.spells.iter().copied()).collect();
    let mean = if spells.is_empty() {
        0.0
    } else {
        spells.iter().map(|x| x.0).sum::<f64>() / spells.len() as f64
    };
    let longest = spells.iter().map(|x| x.0).fold(0.0, f64::max);
    let over = spells.iter().map(|x| x.0 - x.1).fold(0.0, f64::max);
    let port_days = sum(on, &|s| (s.ports as u64 * s.years * YEAR) as f64);
    eprintln!(
        "ALL {world}: {} seas, {years} years each, closures on: per sea per decade blockades \
         {:.3}, closures {:.3}, reopenings {:.3}, ships laid up {:.2}; days closed mean \
         {mean:.1}, longest {longest:.0} (at most {over:.1} past its hot lanes); port-days \
         closed {:.5} (most closed port {:.3}); ports ever closed {} of {} (seas with a \
         closure {}); days every sea port closed {} (navy sailing {}), every sky port {} (navy \
         sailing {}); hunts/yr {:.2} (off {:.2}); havens open {:.3} (off {:.3})",
        on.len(),
        sum(on, &|s| s.blockades as f64) * 10.0 / (n * years as f64),
        sum(on, &|s| s.closures as f64) * 10.0 / (n * years as f64),
        sum(on, &|s| s.reopenings as f64) * 10.0 / (n * years as f64),
        sum(on, &|s| s.laid_up as f64) * 10.0 / (n * years as f64),
        sum(on, &|s| s.closed_days as f64) / port_days,
        on.iter().map(|s| s.worst_port).fold(0.0, f64::max),
        sum(on, &|s| s.ever_closed as f64),
        sum(on, &|s| s.ports as f64),
        on.iter().filter(|s| s.closures > 0).count(),
        sum(on, &|s| s.all_closed[0] as f64),
        sum(on, &|s| s.all_closed_navy[0] as f64),
        sum(on, &|s| s.all_closed[1] as f64),
        sum(on, &|s| s.all_closed_navy[1] as f64),
        sum(on, &|s| s.hunts as f64) / (n * years as f64),
        sum(off, &|s| s.hunts as f64) / (n * years as f64),
        sum(on, &|s| s.open) / n,
        sum(off, &|s| s.open) / n,
    );
}

/// A run's name and counts, without its spells.
fn brief(s: &Sea) -> String {
    format!(
        "{} {} closures {}: closures {}, reopenings {}, every sea port closed {} days (navy \
         sailing {}), decade hunts {:?}",
        s.world,
        s.seed,
        s.on,
        s.closures,
        s.reopenings,
        s.all_closed[0],
        s.all_closed_navy[0],
        s.decade_hunts
    )
}

/// The bars (rule 7 on many seas): the per-run bounds asserted in [`unattended`]; never every
/// sea port closed while a navy sails (a navy patrols the sea, so the sky's ports are reported,
/// not bounded); pirates hunt in every century; every closure but those still running at the
/// end reopened; and with closures off no port ever closes.
fn check_seas(on: &[Sea], off: &[Sea]) {
    for s in on {
        assert_eq!(s.all_closed_navy[0], 0, "{}", brief(s));
        for c in s.decade_hunts.chunks(10) {
            assert!(
                c.iter().sum::<u64>() > 0,
                "a century with no hunt: {}",
                brief(s)
            );
        }
        assert!(
            s.reopenings <= s.closures && s.closures <= s.reopenings + s.ports as u64,
            "{}",
            brief(s)
        );
    }
    for s in off {
        assert_eq!(
            (s.blockades, s.closures, s.closed_days),
            (0, 0, 0),
            "{}",
            brief(s)
        );
        for c in s.decade_hunts.chunks(10) {
            assert!(
                c.iter().sum::<u64>() > 0,
                "a century with no hunt: {}",
                brief(s)
            );
        }
    }
}

fn seas(threads: usize, seeds: std::ops::RangeInclusive<u64>, years: u64) {
    let jobs: Vec<(&'static str, u64, bool)> = ["sea_havens", "coastal"]
        .into_iter()
        .flat_map(|w| {
            seeds
                .clone()
                .flat_map(move |s| [(w, s, true), (w, s, false)])
        })
        .collect();
    let all = per_job(threads, &jobs, |&(w, s, on)| {
        let t0 = std::time::Instant::now();
        let r = unattended(w, s, on, years);
        eprintln!("done {w} {s} {on} in {:.0?}", t0.elapsed());
        r
    });
    report_seas(&all);
    for w in ["sea_havens", "coastal"] {
        let on: Vec<Sea> = all
            .iter()
            .filter(|s| s.world == w && s.on)
            .cloned()
            .collect();
        let off: Vec<Sea> = all
            .iter()
            .filter(|s| s.world == w && !s.on)
            .cloned()
            .collect();
        summary_seas(w, &on, &off);
        check_seas(&on, &off);
    }
}

/// One `blockade_min_open_days` value's seas together (PC99): closures per sea per decade, the
/// closed spells' mean and longest, the share of port-days closed and the most closed single
/// port's share, the sea-days with every sea port or every sky port of a sea closed at once,
/// the havens' open share (mean and lowest sea) and hunts per sea-year. Returns the most closed
/// port's share.
fn summary_min_open(min_open: f64, on: &[Sea]) -> f64 {
    let n = on.len() as f64;
    let years = on.first().map_or(0, |s| s.years);
    let sum = |f: &dyn Fn(&Sea) -> f64| on.iter().map(f).sum::<f64>();
    let spells: Vec<f64> = on
        .iter()
        .flat_map(|s| s.spells.iter().map(|x| x.0))
        .collect();
    let mean = if spells.is_empty() {
        0.0
    } else {
        spells.iter().sum::<f64>() / spells.len() as f64
    };
    let longest = spells.iter().copied().fold(0.0, f64::max);
    let port_days = sum(&|s| (s.ports as u64 * s.years * YEAR) as f64);
    let worst = on.iter().map(|s| s.worst_port).fold(0.0, f64::max);
    let lowest_open = on.iter().map(|s| s.open).fold(f64::INFINITY, f64::min);
    eprintln!(
        "MINOPEN {min_open}: {} seas, {years} years each: closures per sea per decade {:.3}, \
         closed spell mean {mean:.1} days, longest {longest:.0}; port-days closed {:.5}, most \
         closed port {worst:.3}; sea-days every sea port closed {} (seas {}), every sky port \
         closed {} (seas {}); havens open {:.3} (lowest sea {lowest_open:.3}); hunts per \
         sea-year {:.2}",
        on.len(),
        sum(&|s| s.closures as f64) * 10.0 / (n * years as f64),
        sum(&|s| s.closed_days as f64) / port_days,
        sum(&|s| s.all_closed[0] as f64),
        on.iter().filter(|s| s.all_closed[0] > 0).count(),
        sum(&|s| s.all_closed[1] as f64),
        on.iter().filter(|s| s.all_closed[1] > 0).count(),
        sum(&|s| s.open) / n,
        sum(&|s| s.hunts as f64) / (n * years as f64),
    );
    worst
}

/// The `blockade_min_open_days` sweep (PC99): `synth::sea_havens`, seeds 1 to 30, ten centuries
/// with nobody watching, the defaults (navy assaults and port raids on) with min-open 0, 48, 96
/// and 192 days. Prints each run, each value's summary and the rule's pick: the smallest value
/// whose most closed port spends at most a quarter of the days closed, else 192. Release only
/// (`nice -n 10 cargo test --release -p rtsim-ships --test blockade -- --ignored --exact
/// blockade_min_open_sweep --nocapture`); `MIN_OPEN_THREADS` sets the threads (default 10).
#[test]
#[ignore = "long: run in release with --ignored"]
fn blockade_min_open_sweep() {
    let values = [0.0, 48.0, 96.0, 192.0];
    let threads = std::env::var("MIN_OPEN_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let years = std::env::var("MIN_OPEN_YEARS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let jobs: Vec<(u64, f64)> = (1..=30u64)
        .flat_map(|s| values.iter().map(move |&m| (s, m)))
        .collect();
    let all = per_job(threads, &jobs, |&(s, m)| {
        let t0 = std::time::Instant::now();
        let r = unattended_with("sea_havens", s, true, years, Some(m));
        eprintln!("done sea_havens {s} min-open {m} in {:.0?}", t0.elapsed());
        r
    });
    report_seas(&all);
    let mut pick = None;
    for m in values {
        let on: Vec<Sea> = all.iter().filter(|s| s.min_open == m).cloned().collect();
        let worst = summary_min_open(m, &on);
        if pick.is_none() && worst <= 0.25 {
            pick = Some(m);
        }
    }
    for seed in [30, 21, 17, 14] {
        for s in all.iter().filter(|s| s.seed == seed) {
            eprintln!(
                "SEED {seed} min-open {}: closures {}, port-days closed {} of {}, most closed \
                 port {:.3}, every sky port closed {} days, havens open {:.3}, hunts/yr {:.1}",
                s.min_open,
                s.closures,
                s.closed_days,
                s.ports as u64 * s.years * YEAR,
                s.worst_port,
                s.all_closed[1],
                s.open,
                s.hunts as f64 / s.years as f64,
            );
        }
    }
    eprintln!(
        "PICK: {}",
        pick.map_or("none at or below 0.25: 192".into(), |m| format!("{m}"))
    );
}

/// Rule 7 on many seas: `synth::sea_havens` and `synth::coastal`, seeds 1 to 30, a century with
/// nobody watching, closures on and off. Release only, for its length (`nice -n 10 cargo test
/// --release -p rtsim-ships --test blockade -- --ignored --exact
/// blockades_on_many_seas_100_years --nocapture`; without `--exact` the name also matches the
/// ten-century run).
#[test]
#[ignore = "long: run in release with --ignored"]
fn blockades_on_many_seas_100_years() {
    seas(10, 1..=30, 100);
}

/// [`blockades_on_many_seas_100_years`] over ten centuries.
#[test]
#[ignore = "long: run in release with --ignored"]
fn blockades_on_many_seas_1000_years() {
    seas(10, 1..=30, 1000);
}
