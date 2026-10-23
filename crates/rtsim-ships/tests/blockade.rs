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
use rtsim_ships::engine::{LaneThreat, PortState, ShipId, Status};
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
        changes += p.step(&heats, full, min_closed, t).len();
        let above = heats.iter().all(|&h| h >= centre);
        if naive_above.is_some_and(|a| a != above) {
            naive_flips += 1;
        }
        naive_above = Some(above);
        t += check;
    }
    (changes, naive_flips, p)
}

/// The hysteresis, pure and on the coast. Pure ([`hover`]): heats hovering on each threshold
/// flip a one-threshold rule at most checks but change the port at most once. On the blockade
/// coast, seeds 1 to 4 for 400 days with the pirates' own comings and goings (their applied
/// share jumps between 0 and three times full within days): the tape shows only the legal steps
/// (open, blockaded, closed, reopened, no longer blockaded), each closed spell lasts at least
/// `blockade_min_closed_days`, no two changes come closer than a check, and the port changes
/// state far less often than the raw share crosses half.
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

/// PC23 with the fake plugin: a ship in view when its port closes keeps its timetable, is never
/// despawned, and lays up only once it has left view. Seed 1: a player rides a merchant of dock
/// 3's services (stands on it, step by step) from a day before the closure to three days after;
/// the other ships of the port's services lay up at the closure, this one sails on, its hold
/// empty from the first voyage after the closure. The player steps ashore far away: the body
/// leaves view, the plugin reports it dematerialized, and the ship is laid up at that report.
#[test]
fn a_ship_in_view_lays_up_only_after_it_leaves_view() {
    let tun = ShipTunables::default();
    let mut judged = 0;
    for seed in 1..=3u64 {
        let at = closing_tick(seed, &tun);
        let (mut c, mut fake) = blockade_coast(seed, &tun);
        let from = Tick((at.0 - TICKS_PER_DAY) / STEP * STEP);
        run_until(&mut c, &mut fake, from, |_, _| false);
        let ships = fleet(&c.ships, &calling(&c.ships, TARGET));
        let Some(m) = ships.iter().copied().find(|&id| {
            c.ships
                .ship(id)
                .is_some_and(|x| x.is_active() && x.promoted.is_none())
        }) else {
            continue;
        };
        let agent = rtsim_ships::Ship::agent(m);
        let mut t = from;
        let mut in_view_at_closure = false;
        while t < at + 3 * TICKS_PER_DAY {
            ride(&mut c, &mut fake, m, t);
            t += 50;
            go(&mut c, &mut fake, t);
            let x = c.ships.ship(m).expect("a ship");
            if c.ships.port_closed(TARGET) {
                if !in_view_at_closure {
                    in_view_at_closure = fake.bodies.contains_key(&agent);
                    if !in_view_at_closure {
                        break;
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
        if !in_view_at_closure {
            continue;
        }
        assert!(c.ships.waiting_to_lay_up(m));
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
        // Ashore, far away.
        let laid = c.ships.stats.ships_laid_up_by_closure;
        fake.players.clear();
        let p = c.ships.player(&mut c.world, PLAYER);
        c.ships.player_left(&mut c.world, p);
        let before = fake.dematerialized;
        t += 50;
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

/// Persistence (rule 6): a restart in each blockade state, compared by `state_hash` with the run
/// that was never saved, then both carried on 120 days. Three states: blockaded and open; closed
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
        assert!(s.lane_threats().iter().all(|(_, t)| t.heat_nano == 0));
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
