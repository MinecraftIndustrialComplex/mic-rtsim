// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b (EQUILIBRIUM.md §2.4.6a; docs/choices-ports.md PC1 to PC8, PC30 on): havens that rise and
//! fall. An unwatched hunt pays the square law's toll in crew, a haven with no hunt for 30 days
//! starves and lays its ships up, a starving haven with no ship shuts, a navy breaks a weak haven
//! that has robbed its patrolled lanes for two years and lately, and a shut haven's site takes a
//! new band out of every player's view once a fresh band would find prey.
//!
//! The rules are judged on the synthetic coast (`synth::coastal`: one haven, a sea and a sky
//! pirate) and, for the lifecycle over many worlds, on `synth::sea_havens` (3 to 5 havens, 1 or 2
//! navies, 12 to 20 ports, laid out by the seed).

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
use rtsim_ships::boundary::{self, Obs};
use rtsim_ships::engine::{PState, Ship, ShipId};
use rtsim_ships::synth::{self, Coastal, FakeShips, SeaHavens};
use rtsim_ships::{ShipTunables, Ships, fight};
use rtsim_travel::Medium;

const SEEDS: std::ops::RangeInclusive<u64> = 1..=30;
/// The model's year (four 24-day seasons), as in `equilibrium.rs`.
const YEAR: u64 = 96;

/// `f` on each seed, a few seeds at a time on their own threads, in seed order.
fn per_seed<T: Send>(seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    per_seed_on(6, seeds, f)
}

/// [`per_seed`] on up to `threads` threads (the long runs' many jobs take more), each thread
/// taking the next job as it finishes one; the results in seed order.
fn per_seed_on<T: Send>(threads: usize, seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, threads));
    let next = AtomicUsize::new(0);
    let mut out: Vec<(usize, T)> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads.min(seeds.len()))
            .map(|_| {
                let (f, next) = (&f, &next);
                sc.spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&s) = seeds.get(i) else { break done };
                        done.push((i, f(s)));
                    }
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("a seed's run panicked"))
            .collect()
    });
    out.sort_by_key(|&(i, _)| i);
    out.into_iter().map(|(_, x)| x).collect()
}

fn seeds() -> Vec<u64> {
    SEEDS.collect()
}

fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// Runs the coast with nobody watching to `to`, in steps of `dt` ticks through the fake plugin.
fn idle_by(c: &mut Coastal, fake: &mut FakeShips, to: Tick, dt: u64) {
    synth::drive(
        &mut c.world,
        &mut c.ships,
        fake,
        &mut NoInfrastructure,
        &c.mask,
        to,
        dt,
    );
}

fn idle(c: &mut Coastal, fake: &mut FakeShips, to: Tick) {
    idle_by(c, fake, to, TICKS_PER_DAY / 4);
}

/// Haven `h`'s ships: (active, all).
fn manned(s: &Ships, h: u32) -> (usize, usize) {
    let slots = &s.havens[h as usize].ships;
    let up = slots
        .iter()
        .filter(|&&p| s.ship(p).is_some_and(Ship::is_active))
        .count();
    (up, slots.len())
}

fn tape_events(s: &Ships, kind: rtsim_archive::Kind) -> Vec<rtsim_archive::Event> {
    rtsim_archive::record::rows_on(&s.tape)
        .into_iter()
        .filter_map(|(k, _, r)| match r {
            rtsim_archive::Row::Event(e) if k == kind => Some(e),
            _ => None,
        })
        .collect()
}

/// Escorts every lane in reach of the coast's haven with `strength` (0 takes them away).
fn escort_all(c: &mut Coastal, strength: f64, at: Tick) {
    let lanes = c.ships.havens[c.haven as usize].lanes.clone();
    assert!(!lanes.is_empty());
    for l in lanes {
        c.ships.set_escort(&mut c.world, l, 7, strength, at);
    }
}

/// The toll §2.4.6a rule 1 sets for pirate `s` on lane `l` now: `crew x (1 - square_law(A, D))`,
/// thousandths, A its strength and D a merchant of its medium plus the lane's patrols and
/// escorts.
fn toll_of(s: &Ships, p: ShipId, l: u32) -> u32 {
    let t = &s.tun;
    let x = s.ship(p).expect("the pirate");
    let a = f64::from(x.crew) * t.pirate_strength * f64::from(x.hull_milli.max(100)) / 1000.0;
    let defence = s
        .threat(l)
        .map_or(0, |th| th.patrol_milli + th.escorts_milli()) as f64
        / 1000.0;
    let d = f64::from(t.medium(x.medium).merchant_crew) * t.merchant_strength + defence;
    (f64::from(x.crew) * 1000.0 * (1.0 - fight::square_law(a, d, t))).round() as u32
}

/// The lane pirate `p` is outbound to or hunting, if either.
fn lane_of(s: &Ships, p: ShipId) -> Option<u32> {
    match s.ship(p)?.pirate.as_ref()?.state {
        PState::Outbound { lane, .. } | PState::Hunting { lane, .. } => Some(lane),
        _ => None,
    }
}

/// S8b `a_hunt_pays_its_toll` (§2.4.6a rule 1, PC1). With hires off, so the pool moves only by
/// the toll: the first hunts cost exactly the square law's share in thousandths (a sea pirate on
/// an unpatrolled lane 343, the design's 0.34 men), paid from the pool; with the pool empty the
/// toll comes off the crew in whole men, rounded up; and a crew paid to its last man loses its
/// ship before the hunt begins, with no pressure (no player did it) and its slot's refit booked.
#[test]
fn a_hunt_pays_its_toll() {
    let tun = ShipTunables {
        hires_per_day: 0.0,
        ..ShipTunables::default()
    };
    let step = TICKS_PER_DAY / 96;
    per_seed(&seeds(), |seed| {
        // The pool pays.
        let mut c = synth::coastal(seed, tun.clone());
        let h = c.haven;
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let slots = c.ships.havens[h as usize].ships.clone();
        let pool0 = c.ships.havens[h as usize].pool_milli;
        let mut t = Tick::ZERO;
        // Each pirate's toll, read while it is outbound (the lane's defence then), paid when it
        // reaches the lane.
        let mut due = vec![None; slots.len()];
        while c.ships.stats.hunts == 0 {
            t += step;
            assert!(t < Tick::from_days(60), "seed {seed}: no hunt");
            for (i, &p) in slots.iter().enumerate() {
                if let Some(PState::Outbound { lane, .. }) = c
                    .ships
                    .ship(p)
                    .and_then(|x| x.pirate.as_ref())
                    .map(|x| x.state)
                {
                    due[i] = Some(toll_of(&c.ships, p, lane));
                }
            }
            idle_by(&mut c, &mut fake, t, step);
        }
        let mut paid = 0;
        let mut began = None::<Tick>;
        for (i, &p) in slots.iter().enumerate() {
            let Some(PState::Hunting { since, .. }) = c
                .ships
                .ship(p)
                .and_then(|x| x.pirate.as_ref())
                .map(|x| x.state)
            else {
                continue;
            };
            began = began.max(Some(since));
            let toll = due[i].expect("it was outbound first");
            let x = c.ships.ship(p).unwrap();
            let lane = lane_of(&c.ships, p).unwrap();
            let th = c.ships.threat(lane).unwrap();
            if x.medium == Medium::Sea && th.patrol_milli + th.escorts_milli() == 0 {
                assert_eq!(toll, 343, "seed {seed}: the design's 0.34 men");
            }
            assert!(toll > 0 && toll < 6_000, "seed {seed}: {toll}");
            assert_eq!(x.crew, tun.medium(x.medium).pirate_crew, "the pool paid");
            paid += toll;
        }
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.counts.toll_milli, u64::from(paid), "seed {seed}");
        assert_eq!(hv.pool_milli, pool0 - paid, "seed {seed}");
        assert_eq!(
            Some(hv.fed_until),
            began.map(|b| b + Tick::from_days(30).0),
            "seed {seed}: a hunt start feeds the haven for exactly 30 days"
        );
        c.ships.check(&c.world).expect("check");

        // The pool is empty: the crew pays, in whole men rounded up.
        let hunts = c.ships.stats.hunts;
        let at = c.ships.now();
        let hv = &mut c.ships.havens[h as usize];
        hv.pool_milli = 0;
        hv.pool_at = at;
        let toll0 = hv.counts.toll_milli;
        let mut due = vec![None; slots.len()];
        let mut crew = vec![0; slots.len()];
        while c.ships.stats.hunts == hunts {
            t += step;
            assert!(t < Tick::from_days(120), "seed {seed}: no second hunt");
            for (i, &p) in slots.iter().enumerate() {
                if let Some(PState::Outbound { lane, .. }) = c
                    .ships
                    .ship(p)
                    .and_then(|x| x.pirate.as_ref())
                    .map(|x| x.state)
                {
                    due[i] = Some(toll_of(&c.ships, p, lane));
                    crew[i] = c.ships.ship(p).unwrap().crew;
                }
            }
            idle_by(&mut c, &mut fake, t, step);
        }
        let mut paid = 0;
        for (i, &p) in slots.iter().enumerate() {
            let x = c.ships.ship(p).unwrap();
            let Some(PState::Hunting { since, .. }) = x.pirate.as_ref().map(|x| x.state) else {
                continue;
            };
            if since <= at {
                continue;
            }
            let toll = due[i].expect("outbound first");
            assert_eq!(
                x.crew,
                crew[i] - toll.div_ceil(1000),
                "seed {seed}: whole men"
            );
            paid += u64::from(toll);
        }
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.pool_milli, 0);
        assert_eq!(hv.counts.toll_milli, toll0 + paid, "seed {seed}");
        c.ships.check(&c.world).expect("check");

        // A crew paid to its last man: one man aboard, nobody ashore.
        let mut c = synth::coastal(seed, tun.clone());
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        let p = loop {
            t += step;
            assert!(t < Tick::from_days(60), "seed {seed}: nobody sailed");
            idle_by(&mut c, &mut fake, t, step);
            if let Some(&p) = slots.iter().find(|&&p| {
                matches!(
                    c.ships
                        .ship(p)
                        .and_then(|x| x.pirate.as_ref())
                        .map(|x| x.state),
                    Some(PState::Outbound { .. })
                )
            }) {
                break p;
            }
        };
        c.ships.ships.get_mut(p).unwrap().crew = 1;
        let lane = lane_of(&c.ships, p).unwrap();
        assert_eq!(
            toll_of(&c.ships, p, lane),
            1_000,
            "one man against a merchant: all of him"
        );
        c.ships.log_enabled = true;
        let hv = &mut c.ships.havens[h as usize];
        hv.pool_milli = 0;
        hv.pool_at = t;
        let fed = hv.fed_until;
        let (hunts, sinkings) = (c.ships.stats.hunts, c.ships.stats.sinkings);
        while c.ships.ship(p).unwrap().is_active() {
            t += step;
            assert!(t < Tick::from_days(90), "seed {seed}: still afloat");
            idle_by(&mut c, &mut fake, t, step);
        }
        let x = c.ships.ship(p).unwrap();
        assert_eq!(x.crew, 0);
        assert!(!x.is_laid_up(), "sunk by the toll, not laid up");
        assert_eq!(c.ships.stats.sinkings, sinkings + 1);
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.pressure, Pressure::default(), "no player, no pressure");
        let name = c.ships.ship_name(p);
        let tolls: Vec<&str> = c
            .ships
            .log
            .iter()
            .filter(|e| e.kind == "toll" && e.ship.as_deref() == Some(name.as_str()))
            .map(|e| e.what.as_str())
            .collect();
        assert_eq!(tolls.len(), 1, "seed {seed}: {tolls:?}");
        assert!(
            tolls[0].contains("costs 1.00 men: 0.00 from"),
            "{}",
            tolls[0]
        );
        assert!(
            hv.shut.is_none(),
            "the other slot is afloat, and the haven fed"
        );
        // The toll comes first: the lost ship began no hunt and fed nothing.
        if c.ships.stats.hunts == hunts {
            assert_eq!(hv.fed_until, fed);
        }
        assert!(c.ships.pending_by_kind().get("refit").copied().unwrap_or(0) >= 1);
        c.ships.check(&c.world).expect("check");
    });
}

/// S8b `a_starving_haven_shuts_and_refounds` (§2.4.6a rules 2, 4 and 5; PC2 to PC5). Every lane
/// in reach of the coast's haven is escorted past a hunt's worth from day 0, so nobody hunts.
/// Fed for its first 30 days, the haven then starves: each ship's next fruitless decision lays it
/// up (crew ashore, no sinking, no pressure), and with the second the haven shuts, counted in the
/// census's broken column, its site due to look again after 48 days (P = 0). The pool deserts
/// from there. With the escorts still on the look finds no prey and looks again 48 days on; the
/// state reads back across a restart. Once the escorts leave, the next look refounds the site:
/// 4.5 men ashore, fed for 30 days, both slots relaunched (taped back at the dock), and they hunt
/// again.
#[test]
fn a_starving_haven_shuts_and_refounds() {
    let tun = ShipTunables::default();
    let starve = Tick::from_days(30).0;
    let cap = tun.crew_pool_cap * 1000;
    let k = tun.hires_per_day / f64::from(tun.crew_pool_cap);
    per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.tape.set_on(true);
        let h = c.haven;
        escort_all(&mut c, 2.5, Tick::ZERO);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        idle(&mut c, &mut fake, Tick::from_days(29));
        assert_eq!(c.ships.stats.hunts, 0, "seed {seed}");
        assert_eq!(manned(&c.ships, h), (2, 2), "fed: they rest in port");
        assert!(!c.ships.starving(h, c.ships.now()));
        assert_eq!(c.ships.havens[h as usize].fed_until, Tick(starve));
        // The pool's one piecewise closed form (PC2): untouched since the founding (no hunt, no
        // top-up), it hires toward the cap until the turn on day 30, then deserts at the same
        // rate, `p_turn e^(-r t / cap)`.
        let hv = &c.ships.havens[h as usize];
        assert_eq!((hv.pool_milli, hv.pool_at), (6_000, Tick::ZERO));
        let fed = |d: f64| f64::from(cap) - f64::from(cap - 6_000) * (-k * d).exp();
        let read = |d: f64| {
            f64::from(
                c.ships
                    .crew_pool_milli(h, Tick((d * TICKS_PER_DAY as f64) as u64)),
            )
        };
        for d in [29.0, 30.0] {
            assert!((read(d) - fed(d)).abs() <= 1.0, "seed {seed} day {d}");
        }
        let turn = read(30.0);
        for d in [30.5, 33.0, 40.0] {
            let want = turn * (-k * (d - 30.0)).exp();
            assert!(
                (read(d) - want).abs() <= 1.0,
                "seed {seed} day {d}: {} against {want}",
                read(d)
            );
        }

        // Starving: laid up, then shut.
        let mut t = c.ships.now();
        while c.ships.havens[h as usize].shut.is_none() {
            t += TICKS_PER_DAY / 4;
            assert!(t < Tick::from_days(40), "seed {seed}: not shut by day 40");
            idle(&mut c, &mut fake, t);
        }
        let hv = &c.ships.havens[h as usize];
        let sh = hv.shut.unwrap();
        assert!(
            sh.since >= Tick(starve) && !sh.broken,
            "seed {seed}: {sh:?}"
        );
        for &p in &hv.ships {
            let x = c.ships.ship(p).unwrap();
            assert!(x.is_laid_up() && x.crew == 0, "seed {seed}: {x:?}");
        }
        assert_eq!((hv.counts.laid_up, hv.counts.shuts), (2, 1));
        assert_eq!(c.ships.stats.sinkings, 0, "laid up, not sunk");
        assert_eq!(hv.pressure, Pressure::default());
        assert!(tape_events(&c.ships, rtsim_archive::kinds::SHIPS_LOST).is_empty());
        let wait = tun.refound.delay_ticks(Pressure::default(), 0);
        assert_eq!(wait, Tick::from_days(48).0);
        assert_eq!(sh.back, sh.since + wait, "seed {seed}");
        let census = &c.ships.census()[0];
        assert_eq!((census.broken, census.members), (1, 0));
        // Both crews went ashore (capped), and the pool deserts from there.
        let full = c.ships.crew_pool_milli(h, sh.since);
        assert!(full > 10_000 && full <= cap, "seed {seed}: {full}");
        let mut last = full;
        for d in [1u64, 10, 30, 47] {
            let p = c.ships.crew_pool_milli(h, sh.since + d * TICKS_PER_DAY);
            let want = f64::from(full) * (-k * d as f64).exp();
            assert!(
                (f64::from(p) - want).abs() <= 1.0,
                "seed {seed} day {d}: {p} against {want}"
            );
            assert!(p < last, "seed {seed} day {d}: {p} after {last}");
            last = p;
        }
        c.ships.check(&c.world).expect("check");

        // The look finds no prey: it looks again 48 days on.
        idle(&mut c, &mut fake, sh.back + TICKS_PER_DAY);
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.counts.refounds, 0, "seed {seed}: escorted, no prey");
        let back = hv.shut.expect("still shut").back;
        assert_eq!(back, sh.back + wait, "seed {seed}");
        assert_eq!(c.ships.pending_by_kind().get("refound"), Some(&1));
        // A restart reads the shut haven and its look back.
        let s2 = Ships::load(tun.clone(), &c.ships.save()).expect("ships load");
        assert_eq!(s2.havens, c.ships.havens);
        assert_eq!(s2.state_hash(), c.ships.state_hash());
        c.ships.check(&c.world).expect("check");

        // The escorts leave: the next look refounds the site.
        let now = c.ships.now();
        escort_all(&mut c, 0.0, now);
        let mut t = c.ships.now();
        while c.ships.havens[h as usize].shut.is_some() {
            t += TICKS_PER_DAY / 4;
            assert!(
                t <= back + TICKS_PER_DAY,
                "seed {seed}: no refound at {}",
                day(back)
            );
            idle(&mut c, &mut fake, t);
        }
        let backs = tape_events(&c.ships, rtsim_archive::kinds::SHIPS_BACK);
        assert_eq!(backs.len(), 2, "seed {seed}: {backs:?}");
        let r = Tick(backs[0].tick);
        assert_eq!(r, back, "seed {seed}: refounded at the look");
        let hv = &c.ships.havens[h as usize];
        for e in &backs {
            assert_eq!(e.tick, r.0);
            assert_eq!((e.x, e.z), (hv.dock.0, hv.dock.2));
        }
        assert_eq!((hv.pool_milli, hv.pool_at), (cap / 4, r), "4.5 men ashore");
        assert_eq!(hv.fed_until, r + starve);
        assert_eq!(hv.counts.refounds, 1);
        assert_eq!(manned(&c.ships, h), (2, 2));
        for &p in &hv.ships {
            let x = c.ships.ship(p).unwrap();
            assert_eq!(
                (x.crew, x.hull_milli),
                (tun.medium(x.medium).pirate_crew, 1000)
            );
        }
        let census = &c.ships.census()[0];
        assert_eq!((census.broken, census.members), (0, 2));
        assert_eq!(census.renewals, c.ships.stats.rebuilds + 1);
        // And they hunt again.
        idle(&mut c, &mut fake, r + 20 * TICKS_PER_DAY);
        assert!(c.ships.stats.hunts > 0, "seed {seed}: {:?}", c.ships.stats);
        c.ships.check(&c.world).expect("check");
    });
}

/// S8b `a_laid_up_slot_wakes_when_its_haven_is_fed_again` (§2.4.6a rule 2, PC11): a starving
/// haven lays one ship up while its other ship is still afloat; when the escorts leave, that
/// other ship hunts, which feeds the haven, and the laid-up slot (its refit never due) looks for
/// its refit again and sails. Escorted from day 0 so nothing hunts before the haven starves.
#[test]
fn a_laid_up_slot_wakes_when_its_haven_is_fed_again() {
    let tun = ShipTunables::default();
    let step = TICKS_PER_DAY / 96;
    let results = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let h = c.haven;
        escort_all(&mut c, 2.5, Tick::ZERO);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let slots = c.ships.havens[h as usize].ships.clone();
        let mut t = Tick::from_days(29);
        idle_by(&mut c, &mut fake, t, step);
        // Step until exactly one slot is laid up and another is still afloat.
        let laid = loop {
            t += step;
            if t > Tick::from_days(40) || c.ships.havens[h as usize].shut.is_some() {
                break None;
            }
            idle_by(&mut c, &mut fake, t, step);
            let up: Vec<ShipId> = slots
                .iter()
                .copied()
                .filter(|&p| c.ships.ship(p).is_some_and(Ship::is_laid_up))
                .collect();
            if up.len() == 1 && manned(&c.ships, h).0 >= 1 {
                break Some(up[0]);
            }
        };
        let p = laid?;
        c.ships.check(&c.world).expect("check while one is laid up");
        // The escorts leave before the other ship decides: it hunts and feeds the haven.
        escort_all(&mut c, 0.0, t);
        let hunts = c.ships.stats.hunts;
        idle_by(&mut c, &mut fake, t + 60 * TICKS_PER_DAY, step);
        c.ships.check(&c.world).expect("check");
        let hv = &c.ships.havens[h as usize];
        let fed_again = c.ships.stats.hunts > hunts && hv.shut.is_none() && hv.counts.refounds == 0;
        Some((
            seed,
            fed_again,
            c.ships.ship(p).is_some_and(Ship::is_laid_up),
        ))
    });
    let fed: Vec<_> = results.iter().flatten().filter(|r| r.1).collect();
    let stuck: Vec<_> = fed.iter().filter(|r| r.2).map(|r| r.0).collect();
    eprintln!(
        "fed again beside a laid-up slot: {}; still laid up: {stuck:?}",
        fed.len()
    );
    assert!(
        !fed.is_empty(),
        "no seed had a slot laid up beside an active ship"
    );
    assert!(
        stuck.is_empty(),
        "PC11: laid-up slots never woke, seeds {stuck:?}"
    );
}

/// A late or replayed plugin report for a ship that is no longer active changes nothing
/// (§2.4.6a, PC3): a FIGHT or DEMATERIALIZED record for a laid-up slot must not put men aboard
/// a ship whose crew went ashore, and `check()` still holds.
#[test]
fn a_late_report_for_a_laid_up_ship_changes_nothing() {
    let tun = ShipTunables::default();
    let step = TICKS_PER_DAY / 96;
    per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let h = c.haven;
        escort_all(&mut c, 2.5, Tick::ZERO);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let slots = c.ships.havens[h as usize].ships.clone();
        let mut t = Tick::from_days(29);
        idle_by(&mut c, &mut fake, t, step);
        let p = loop {
            t += step;
            assert!(t < Tick::from_days(40), "seed {seed}: nothing laid up");
            idle_by(&mut c, &mut fake, t, step);
            if let Some(&p) = slots
                .iter()
                .find(|&&p| c.ships.ship(p).is_some_and(Ship::is_laid_up))
            {
                break p;
            }
        };
        let merchant = c
            .ships
            .ships
            .iter()
            .find(|(_, x)| matches!(x.role, rtsim_ships::Role::Merchant))
            .map(|(id, _)| id)
            .expect("a merchant");
        let (before, hash) = (c.ships.ship(p).cloned().unwrap(), c.ships.state_hash());
        let observed = c.ships.stats.fights_observed;
        let fight = Obs::Fight(boundary::Fight {
            attacker: Ship::agent(p),
            defender: Ship::agent(merchant),
            outcome: 1,
            attacker_crew: (6, 5),
            defender_crew: (4, 3),
            attacker_hull_milli: 700,
            defender_hull_milli: 900,
        });
        let demat = Obs::Dematerialized(boundary::Dematerialized {
            agent: Ship::agent(p),
            epoch: before.epoch,
            reason: 1,
            dimension: 0,
            hull_milli: 600,
            crew: 6,
            pos: (0.0, 64.0, 0.0),
        });
        for o in [&fight, &demat] {
            c.ships.ingest(&mut c.world, o, &c.mask, t);
        }
        let x = c.ships.ship(p).unwrap();
        assert!(x.is_laid_up(), "seed {seed}");
        assert_eq!(
            (x.crew, x.hull_milli),
            (before.crew, before.hull_milli),
            "seed {seed}"
        );
        assert_eq!(c.ships.stats.fights_observed, observed);
        assert_eq!(c.ships.state_hash(), hash, "seed {seed}: the state moved");
        c.ships.check(&c.world).expect("check");
    });
}

/// The world and ships read back from their saves, as a restarted engine would, in a `Coastal`.
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

/// S8b persistence (§2.4.6a; the haven rows' `fed_until`, `shut`, `epoch` and `counts` and a
/// slot's laid-up status): a restart in the middle of each lifecycle state, then both runs
/// carried on. Three states: a slot laid up beside an active ship; a shut haven with its look
/// pending; the same after one look found no prey. After the restart the escorts leave in both,
/// the runs go on for 120 days, and the saved-and-restored run ends in the very state of the
/// run that was never saved. At the defaults, navy looks included (PC38): each run ends inside
/// `assault_cause_days` of its start, so no navy breaks the haven, which would hide the first
/// state's way out (the wake).
#[test]
fn a_restart_in_each_lifecycle_state_changes_nothing() {
    let tun = ShipTunables::default();
    let step = TICKS_PER_DAY / 96;
    let seeds: Vec<u64> = (1..=8).collect();
    let hits = per_seed(&seeds, |seed| {
        let mut states = 0;
        for state in 0..3 {
            let mut a = synth::coastal(seed, tun.clone());
            let h = a.haven;
            escort_all(&mut a, 2.5, Tick::ZERO);
            let mut fa = FakeShips::new(f64::from(tun.view_radius));
            let slots = a.ships.havens[h as usize].ships.clone();
            let mut t = Tick::from_days(29);
            idle_by(&mut a, &mut fa, t, step);
            let reached = loop {
                t += step;
                if t > Tick::from_days(40) {
                    break false;
                }
                idle_by(&mut a, &mut fa, t, step);
                let hv = &a.ships.havens[h as usize];
                let up = slots
                    .iter()
                    .filter(|&&p| a.ships.ship(p).is_some_and(Ship::is_laid_up))
                    .count();
                if match state {
                    0 => up == 1 && manned(&a.ships, h).0 >= 1,
                    _ => hv.shut.is_some(),
                } {
                    break true;
                }
            };
            if !reached {
                continue;
            }
            if state == 2 {
                // One look finds no prey (the escorts are still on) and the next is booked.
                let back = a.ships.havens[h as usize].shut.unwrap().back;
                idle_by(&mut a, &mut fa, back + TICKS_PER_DAY, step);
                assert!(a.ships.havens[h as usize].shut.is_some(), "seed {seed}");
            }
            let mut b = restarted(&a, seed, &tun);
            assert_eq!(b.ships.havens, a.ships.havens, "seed {seed} state {state}");
            assert_eq!(b.ships.state_hash(), a.ships.state_hash());
            assert_eq!(b.world.state_hash(), a.world.state_hash());
            assert_eq!(b.ships.pending_by_kind(), a.ships.pending_by_kind());
            let mut fb = FakeShips::new(f64::from(tun.view_radius));
            fb.bodies = fa.bodies.clone();
            fb.seq = fa.seq;
            let now = a.ships.now();
            escort_all(&mut a, 0.0, now);
            escort_all(&mut b, 0.0, now);
            let end = now + 120 * TICKS_PER_DAY;
            idle_by(&mut a, &mut fa, end, step);
            idle_by(&mut b, &mut fb, end, step);
            assert_eq!(
                b.ships.state_hash(),
                a.ships.state_hash(),
                "seed {seed} state {state}: the restored run went its own way"
            );
            assert_eq!(b.world.state_hash(), a.world.state_hash());
            assert_eq!(b.ships.havens, a.ships.havens);
            // The run went through the state's way out, so the comparison covered it.
            let hv = &a.ships.havens[h as usize];
            let up = slots
                .iter()
                .filter(|&&p| a.ships.ship(p).is_some_and(Ship::is_laid_up))
                .count();
            if state == 0 {
                assert_eq!(up, 0, "seed {seed}: the laid-up slot woke");
            } else {
                assert!(hv.counts.refounds >= 1, "seed {seed} state {state}");
            }
            assert_eq!(a.ships.navy_counts().assaults, 0, "seed {seed}: broken");
            a.ships.check(&a.world).expect("check");
            b.ships.check(&b.world).expect("check restored");
            states += 1 << state;
        }
        states
    });
    for state in 0..3 {
        assert!(
            hits.iter().any(|h| h & (1 << state) != 0),
            "no seed reached state {state}"
        );
    }
}

/// S8b `no_refound_in_view` (§2.4.6a rule 5, PC5): nothing appears within a player's view. A shut
/// haven's look while a player stands 100 blocks off its dock does not refound the site, though
/// it has prey again: it looks again 48 days later and nothing is taped. Once the player leaves,
/// the next look refounds it.
#[test]
fn no_refound_in_view() {
    let tun = ShipTunables::default();
    per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.tape.set_on(true);
        let h = c.haven;
        escort_all(&mut c, 2.5, Tick::ZERO);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut t = Tick::ZERO;
        while c.ships.havens[h as usize].shut.is_none() {
            t += TICKS_PER_DAY / 4;
            assert!(t < Tick::from_days(40), "seed {seed}: not shut");
            idle(&mut c, &mut fake, t);
        }
        let back = c.ships.havens[h as usize].shut.unwrap().back;
        escort_all(&mut c, 0.0, t);
        let dock = c.ships.havens[h as usize].dock;
        let pos = rtsim_settle::Pos::new(c.ships.havens[h as usize].frame, dock.0 + 100, dock.2);
        let player = c.ships.player_stays(
            &mut c.world,
            synth::SCRIPT_PLAYER,
            pos,
            t,
            back + 200 * TICKS_PER_DAY,
        );
        idle(&mut c, &mut fake, back + TICKS_PER_DAY);
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.counts.refounds, 0, "seed {seed}: refounded in view");
        assert_eq!(
            hv.shut.expect("still shut").back,
            back + tun.refound.delay_ticks(Pressure::default(), 0),
            "seed {seed}: it looks again"
        );
        assert!(tape_events(&c.ships, rtsim_archive::kinds::SHIPS_BACK).is_empty());
        // The player leaves: the next look refounds.
        let again = hv.shut.unwrap().back;
        c.ships.player_left(&mut c.world, player);
        idle(&mut c, &mut fake, again + TICKS_PER_DAY);
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.counts.refounds, 1, "seed {seed}");
        assert!(hv.shut.is_none());
        c.ships.check(&c.world).expect("check");
    });
}

// ---- navies (§2.4.6a rule 3) -----------------------------------------------------------------

/// The tunables for the navy tests: S8b's lifecycle, with the navy looking every 2 days so a
/// test meets the moment it wants within a few weeks.
fn looks_often() -> ShipTunables {
    ShipTunables {
        assault_every_days: 2,
        ..ShipTunables::default()
    }
}

/// Escorts every lane of `medium` in reach of the coast's haven with `strength`.
fn escort_medium(c: &mut Coastal, medium: Medium, strength: f64, at: Tick) {
    let lanes: Vec<u32> = c.ships.havens[c.haven as usize]
        .lanes
        .iter()
        .copied()
        .filter(|&l| c.ships.lanes[l as usize].medium == medium)
        .collect();
    assert!(!lanes.is_empty());
    for l in lanes {
        c.ships.set_escort(&mut c.world, l, 7, strength, at);
    }
}

/// Drives the coast to the tick before navy `nv`'s next look; that look's tick.
fn to_look(c: &mut Coastal, fake: &mut FakeShips, nv: u32) -> Tick {
    let due = c.ships.assault_due(nv).expect("a navy looks");
    idle_by(c, fake, Tick(due.0 - 1), TICKS_PER_DAY / 96);
    due
}

/// When pirate `p` next changes state (its rest, voyage or hunt ends), if it is active.
fn next_change(s: &Ships, p: ShipId) -> Option<Tick> {
    let x = s.ship(p).filter(|x| x.is_active())?;
    Some(match x.pirate.as_ref()?.state {
        PState::Resting { until } | PState::Hunting { until, .. } => until,
        PState::Outbound { arrive, .. } | PState::Homebound { arrive } => arrive,
    })
}

fn resting(s: &Ships, p: ShipId) -> bool {
    s.ship(p).is_some_and(Ship::is_active)
        && matches!(
            s.ship(p).and_then(|x| x.pirate.as_ref()).map(|x| x.state),
            Some(PState::Resting { .. })
        )
}

/// What haven `h`'s ships in port add to its defence (PC8): each active ship resting there, at
/// its crew, pirate strength and hull.
fn in_port(s: &Ships, h: u32) -> f64 {
    s.havens[h as usize]
        .ships
        .iter()
        .filter(|&&p| resting(s, p))
        .map(|&p| {
            let x = s.ship(p).unwrap();
            f64::from(x.crew) * s.tun.pirate_strength * f64::from(x.hull_milli.max(100)) / 1000.0
        })
        .sum()
}

/// Whether one of haven `h`'s pirates began a hunt on a lane navy `nv`'s patrol sails within
/// `starve_days` of `at` (rule 3's targets, PC35).
fn hunts_the_patrol(s: &Ships, nv: u32, h: u32, at: Tick) -> bool {
    let starve = (s.tun.starve_days * TICKS_PER_DAY as f64).round() as u64;
    s.havens[h as usize]
        .navy_hunts
        .iter()
        .any(|&(n, t)| n == nv && at.0.saturating_sub(t.0) <= starve)
}

/// Whether navy `nv`'s patrol sails lane `l`.
fn patrols(s: &Ships, nv: u32, l: u32) -> bool {
    let sv = s.navies[nv as usize].service;
    s.services[sv as usize].legs.iter().any(|g| g.hop.lane == l)
}

/// Whether haven `h` has robbed patrolled lanes for `assault_cause_days` at `at` (PC31).
fn cause_served(s: &Ships, h: u32, at: Tick) -> bool {
    let cause = u64::from(s.tun.assault_cause_days) * TICKS_PER_DAY;
    s.havens[h as usize]
        .holds_navy_lane_since
        .is_some_and(|t0| at.0 >= t0.0 + cause)
}

/// Whether navy `nv` may assault haven `h` at `at`, out of view: its pirates hunted the patrol's
/// lanes lately and the cause has run its course.
fn a_target(s: &Ships, nv: u32, h: u32, at: Tick) -> bool {
    hunts_the_patrol(s, nv, h, at) && cause_served(s, h, at)
}

/// Sets haven `h`'s men ashore to `milli` thousandths now.
fn set_pool(c: &mut Coastal, h: u32, milli: u32) {
    let at = c.ships.now();
    let hv = &mut c.ships.havens[h as usize];
    hv.pool_milli = milli;
    hv.pool_at = at;
}

/// The coast's haven's sea and sky slots.
fn sea_and_sky(c: &Coastal) -> (ShipId, ShipId) {
    let slots = &c.ships.havens[c.haven as usize].ships;
    let of = |m| {
        *slots
            .iter()
            .find(|&&p| c.ships.ship(p).is_some_and(|x| x.medium == m))
            .expect("a slot of the medium")
    };
    (of(Medium::Sea), of(Medium::Sky))
}

/// S8b `a_navy_breaks_a_weak_haven` (§2.4.6a rule 3, PC7, PC8, PC31, PC35). The coast's sky lanes
/// are escorted, so its sky pirate rests in port while its sea pirate hunts. Every look from the
/// haven's first hunt on a lane Saltmere's navy patrols finds its defence just below the gate:
/// 12.6 men (`A / sqrt 2` is 12.73 for the navy's 2 x 6 x 1.5), its ships in port plus the rest
/// ashore; other looks find the pool at its cap. Until its cause clock has run
/// `assault_cause_days` no look breaks it. The first look after, with the sky ship in port and
/// the sea ship at sea, breaks the haven: the sky ship is sunk at once, by the navy, with no
/// pressure; the men ashore are gone; nothing is refitted. The sea ship finishes its hunt and is
/// laid up on the tick it comes home, and the haven shuts then, counted broken, its site due to
/// refound 48 days later (P = 0), which it does. The broken state reads back across a restart.
#[test]
fn a_navy_breaks_a_weak_haven() {
    let tun = looks_often();
    let cap = tun.crew_pool_cap * 1000;
    per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.tape.set_on(true);
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let (sea, sky) = sea_and_sky(&c);
        escort_medium(&mut c, Medium::Sky, 2.5, Tick::ZERO);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let (mut looks, mut early) = (0, 0);
        let due = loop {
            looks += 1;
            assert!(
                looks <= 1000,
                "seed {seed}: never a weak moment in 2,000 days"
            );
            let due = to_look(&mut c, &mut fake, nv);
            let s = &c.ships;
            let held = hunts_the_patrol(s, nv, h, due);
            let stays = s.havens[h as usize]
                .ships
                .iter()
                .all(|&p| next_change(s, p).is_none_or(|t| t > due));
            if held && !cause_served(s, h, due) && stays && in_port(s, h) <= 12.6 {
                // Weak, but the cause has not run.
                early += 1;
                let pool = ((12.6 - in_port(s, h)) * 1000.0).round() as u32;
                set_pool(&mut c, h, pool);
                idle_by(&mut c, &mut fake, due + 1, 1);
                assert!(
                    c.ships.havens[h as usize].broken_at.is_none(),
                    "seed {seed}: broken before its cause has run"
                );
                continue;
            }
            let ready = a_target(s, nv, h, due)
                && resting(s, sky)
                && next_change(s, sky).is_some_and(|t| t > due)
                && !resting(s, sea)
                && next_change(s, sea).is_some_and(|t| t > due)
                && !s.starving(h, due);
            let port = in_port(s, h);
            let pool = if ready {
                ((12.6 - port) * 1000.0).round() as u32
            } else {
                cap
            };
            set_pool(&mut c, h, pool);
            let sinkings = c.ships.stats.sinkings;
            idle_by(&mut c, &mut fake, due + 1, 1);
            if ready {
                break due;
            }
            assert!(
                c.ships.havens[h as usize].broken_at.is_none(),
                "seed {seed}: broken with a full pool"
            );
            assert_eq!(c.ships.stats.sinkings, sinkings);
        };
        assert!(early > 0, "seed {seed}: no look before the cause had run");
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.broken_at, Some(due), "seed {seed}: broken at the look");
        assert!(hv.shut.is_none(), "its sea ship is out");
        assert_eq!(hv.pressure, Pressure::default(), "no player, no pressure");
        assert!(hv.pool_at >= due && c.ships.crew_pool_milli(h, c.ships.now()) <= 1);
        assert_eq!(
            c.ships.navy_counts().assaults,
            1,
            "seed {seed}: {:?}",
            c.ships.navy_counts()
        );
        assert!(c.ships.navy_counts().looked >= looks);
        let x = c.ships.ship(sky).unwrap();
        assert!(
            matches!(x.status, rtsim_ships::engine::Status::Lost { since, .. } if since == due)
                && !x.is_laid_up(),
            "seed {seed}: the ship in port is sunk: {x:?}"
        );
        assert!(
            c.ships.ship(sea).unwrap().is_active(),
            "the ship at sea sails on"
        );
        let lost: Vec<_> = tape_events(&c.ships, rtsim_archive::kinds::SHIPS_LOST)
            .into_iter()
            .filter(|e| e.tick == due.0)
            .collect();
        assert_eq!(lost.len(), 1, "seed {seed}: {lost:?}");
        let census = &c.ships.census()[0];
        assert_eq!((census.broken, census.members), (1, 1));
        let s2 = Ships::load(tun.clone(), &c.ships.save()).expect("ships load");
        assert_eq!(s2.havens, c.ships.havens);
        assert_eq!(s2.navies, c.ships.navies);
        assert_eq!(s2.assault_due(nv), c.ships.assault_due(nv));
        assert_eq!(s2.state_hash(), c.ships.state_hash());
        c.ships.check(&c.world).expect("check");
        let sinkings = c.ships.stats.sinkings;

        // The sea ship comes home and is laid up on the tick it docks; the haven shuts, broken.
        let mut t = c.ships.now();
        let mut home = None;
        while c.ships.ship(sea).unwrap().is_active() {
            if let Some(PState::Homebound { arrive }) = state(&c.ships, sea) {
                home = Some(arrive);
            }
            t += TICKS_PER_DAY / 96;
            assert!(t < due + 40 * TICKS_PER_DAY, "seed {seed}: never home");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
        }
        let x = c.ships.ship(sea).unwrap();
        assert!(x.is_laid_up() && x.crew == 0, "seed {seed}: {x:?}");
        let home = home.expect("it sailed home");
        assert!(
            matches!(x.status, rtsim_ships::engine::Status::Lost { since, .. } if since == home),
            "seed {seed}: laid up at {:?}, home at {home:?}",
            x.status
        );
        assert_eq!(c.ships.stats.sinkings, sinkings, "laid up, not sunk");
        let hv = &c.ships.havens[h as usize];
        let sh = hv.shut.expect("shut with its last ship");
        assert!(sh.broken && sh.since > due, "seed {seed}: {sh:?}");
        assert_eq!(
            (hv.counts.shuts, hv.counts.broken_shuts, hv.counts.laid_up),
            (1, 1, 1)
        );
        assert_eq!(sh.back, sh.since + Tick::from_days(48).0, "seed {seed}");
        assert_eq!(c.ships.census()[0].broken, 1);
        c.ships.check(&c.world).expect("check");

        // The site refounds 48 days on.
        idle(&mut c, &mut fake, sh.back + TICKS_PER_DAY / 4);
        let hv = &c.ships.havens[h as usize];
        assert_eq!(hv.counts.refounds, 1, "seed {seed}");
        assert!(hv.shut.is_none() && hv.broken_at.is_none());
        assert_eq!(manned(&c.ships, h), (2, 2));
        c.ships.check(&c.world).expect("check");
    });
}

/// S8b `a_navy_leaves_a_strong_haven_alone` (§2.4.6a rule 3, PC7, PC8). At every look of 2,000
/// days at which the haven is a target (PC31, PC35: it has robbed patrolled lanes long enough and
/// lately) and the coast's ships hold still until it, its defence is set just above the gate:
/// 12.8 men, its ships in port plus the rest ashore (the navy then believes it wins with p =
/// 0.664, under 2/3). Other looks find the pool at its cap. No look breaks the haven.
#[test]
fn a_navy_leaves_a_strong_haven_alone() {
    let tun = looks_often();
    let cap = tun.crew_pool_cap * 1000;
    per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let slots = c.ships.havens[h as usize].ships.clone();
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut close = 0;
        for _ in 0..1000 {
            let due = to_look(&mut c, &mut fake, nv);
            let s = &c.ships;
            let still = slots
                .iter()
                .all(|&p| next_change(s, p).is_none_or(|t| t > due));
            let port = in_port(s, h);
            let pool = if still && port <= 12.8 && a_target(s, nv, h, due) {
                close += 1;
                ((12.8 - port) * 1000.0).round() as u32
            } else {
                cap
            };
            set_pool(&mut c, h, pool);
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert!(
                c.ships.havens[h as usize].broken_at.is_none(),
                "seed {seed}: a strong haven broken"
            );
        }
        assert!(close > 0, "seed {seed}: no look at the gate");
        let n = c.ships.navy_counts();
        assert_eq!((n.looked, n.assaults), (1000, 0), "seed {seed}");
        c.ships.check(&c.world).expect("check");
    });
}

/// S8b `no_assault_in_view` (§2.4.6a rule 3: nothing happens in a player's view). Once the coast's
/// haven is a target (PC31, PC35), a player stands 100 blocks off the haven's dock, and at each
/// look the haven has nobody ashore (weak whatever is in port: 11 men at most against the gate's
/// 12.73) until ten looks have found it a target. No look breaks it. Once the player leaves, a
/// look at which it is a target, with the pool empty, breaks it.
#[test]
fn no_assault_in_view() {
    let tun = looks_often();
    per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let cap = tun.crew_pool_cap * 1000;
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        // Strong until it is a target.
        while !a_target(&c.ships, nv, h, c.ships.now()) {
            let due = to_look(&mut c, &mut fake, nv);
            assert!(
                due < Tick::from_days(800),
                "seed {seed}: no target in 800 days"
            );
            set_pool(&mut c, h, cap);
            idle_by(&mut c, &mut fake, due + 1, 1);
        }
        assert!(c.ships.havens[h as usize].broken_at.is_none());
        let now = c.ships.now();
        let dock = c.ships.havens[h as usize].dock;
        let pos = rtsim_settle::Pos::new(c.ships.havens[h as usize].frame, dock.0 + 100, dock.2);
        let player = c.ships.player_stays(
            &mut c.world,
            synth::SCRIPT_PLAYER,
            pos,
            now,
            now + 900 * TICKS_PER_DAY,
        );
        let looked = c.ships.navy_counts().looked;
        let (mut looks, mut targets) = (0, 0);
        while targets < 10 {
            looks += 1;
            assert!(
                looks <= 400,
                "seed {seed}: {targets} target looks in 800 days"
            );
            let due = to_look(&mut c, &mut fake, nv);
            targets += usize::from(a_target(&c.ships, nv, h, due));
            set_pool(&mut c, h, 0);
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert!(
                c.ships.havens[h as usize].broken_at.is_none(),
                "seed {seed}: broken in view"
            );
        }
        let n = c.ships.navy_counts();
        assert_eq!((n.looked, n.assaults), (looked + looks, 0), "seed {seed}");
        // The player leaves: a look at a target breaks the haven.
        c.ships.player_left(&mut c.world, player);
        let mut looks = 0;
        while c.ships.havens[h as usize].broken_at.is_none() {
            looks += 1;
            assert!(looks <= 400, "seed {seed}: not broken out of view");
            let due = to_look(&mut c, &mut fake, nv);
            let weak = a_target(&c.ships, nv, h, due);
            set_pool(&mut c, h, if weak { 0 } else { cap });
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert_eq!(
                c.ships.havens[h as usize].broken_at.is_some(),
                weak,
                "seed {seed}: broken exactly when a target, out of view"
            );
        }
        assert_eq!(c.ships.navy_counts().assaults, 1);
        c.ships.check(&c.world).expect("check");
    });
}

/// Drives the coast look by look until navy `nv` breaks the haven at a look where it is a target,
/// fed, and `ready` holds: the pool is emptied at those looks and filled to its cap at the
/// others. The tick of the break, or `None` within `looks` looks.
fn break_when(
    c: &mut Coastal,
    fake: &mut FakeShips,
    nv: u32,
    looks: usize,
    ready: impl Fn(&Ships, Tick) -> bool,
) -> Option<Tick> {
    let h = c.haven;
    let cap = c.ships.tun.crew_pool_cap * 1000;
    for _ in 0..looks {
        let due = to_look(c, fake, nv);
        let s = &c.ships;
        let go = a_target(s, nv, h, due) && !s.starving(h, due) && ready(s, due);
        set_pool(c, h, if go { 0 } else { cap });
        idle_by(c, fake, due + 1, 1);
        if c.ships.havens[h as usize].broken_at.is_some() {
            assert!(go, "broken at a look that was not ready");
            return Some(due);
        }
    }
    None
}

/// Whether pirate `p` is at sea past `due + 1`: active, not resting, its state holding on.
fn at_sea(s: &Ships, p: ShipId, due: Tick) -> bool {
    s.ship(p).is_some_and(Ship::is_active)
        && !resting(s, p)
        && next_change(s, p).is_some_and(|t| t > due + 1)
}

/// Pirate `p`'s state, if it has one.
fn state(s: &Ships, p: ShipId) -> Option<PState> {
    s.ship(p).and_then(|x| x.pirate.as_ref()).map(|x| x.state)
}

/// S8b review R1 (§2.4.6a rule 3, PC7): a haven broken with both its ships at sea, one of them
/// then sunk at sea by a player. A broken haven refits nothing, so the loss books no refit
/// (`check()` holds no live refit for a broken haven's ship): the slot stays lost, not laid up,
/// and the player's loss presses the haven (S0). The other ship comes home and is laid up, the
/// haven shuts broken, and its site refounds after `min(384, 48 x 2^P)` days with both slots.
#[test]
fn a_loss_at_sea_after_a_break_books_no_refit() {
    let tun = looks_often();
    let done = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.tape.set_on(true);
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let (sea, sky) = sea_and_sky(&c);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let both = |s: &Ships, due: Tick| at_sea(s, sea, due) && at_sea(s, sky, due);
        break_when(&mut c, &mut fake, nv, 300, both)?;
        assert_eq!(manned(&c.ships, h), (2, 2), "seed {seed}");
        c.ships.check(&c.world).expect("check after the break");
        let x = c.ships.ship(sea).unwrap().clone();
        let at = c.ships.now();
        let o = Obs::Sunk(boundary::Sunk {
            agent: Ship::agent(sea),
            epoch: x.epoch,
            by: boundary::By::Player,
            dimension: 0,
            player: (0xAB, 0xCD),
            ship: 0,
            op: 1 << 62 | 77,
            pos: (0.0, 63.0, 0.0),
        });
        c.ships.ingest(&mut c.world, &o, &c.mask, at);
        c.ships
            .check(&c.world)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let x = c.ships.ship(sea).unwrap();
        assert!(
            matches!(x.status, rtsim_ships::engine::Status::Lost { since, .. } if since == at)
                && !x.is_laid_up(),
            "seed {seed}: sunk, not laid up: {x:?}"
        );
        let hv = &c.ships.havens[h as usize];
        assert!(
            hv.pressure != Pressure::default(),
            "a player's loss presses"
        );
        assert!(hv.shut.is_none(), "seed {seed}: its sky ship is out");
        let text = rtsim_archive::record::rows_on(&c.ships.tape)
            .into_iter()
            .filter_map(|(_, _, r)| match r {
                rtsim_archive::Row::Event(e) if e.tick == at.0 => Some(format!("{e:?}")),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!text.is_empty(), "seed {seed}: the loss is taped");
        // The sky ship comes home and is laid up; the haven shuts, broken.
        let mut t = c.ships.now();
        while c.ships.havens[h as usize].shut.is_none() {
            t += TICKS_PER_DAY / 8;
            assert!(t < at + 40 * TICKS_PER_DAY, "seed {seed}: never shut");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
            c.ships
                .check(&c.world)
                .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        }
        assert!(c.ships.ship(sky).unwrap().is_laid_up(), "seed {seed}");
        let hv = &c.ships.havens[h as usize];
        let sh = hv.shut.unwrap();
        assert!(sh.broken);
        let wait = tun
            .refound
            .delay_ticks(hv.pressure, (sh.since.0 / TICKS_PER_DAY) as u32);
        assert_eq!(sh.back, sh.since + wait.max(1), "seed {seed}");
        assert!(
            wait > Tick::from_days(48).0,
            "seed {seed}: P > 0 waits longer"
        );
        idle(&mut c, &mut fake, sh.back + TICKS_PER_DAY / 4);
        assert_eq!(c.ships.havens[h as usize].counts.refounds, 1, "seed {seed}");
        assert_eq!(manned(&c.ships, h), (2, 2), "seed {seed}");
        c.ships.check(&c.world).expect("check");
        Some(seed)
    });
    let n = done.iter().flatten().count();
    assert!(
        n >= 5,
        "only {n} seeds broke with both ships at sea: {done:?}"
    );
}

/// S8b review R2 (§2.4.6a rule 3; the persist pattern): a restart while the haven is broken and
/// open (both its ships at sea) continues exactly: both runs carried on for 150 days end in the
/// same state.
#[test]
fn a_restart_while_broken_continues_exactly() {
    let tun = looks_often();
    let done = per_seed(&[1, 2, 3, 4, 5, 6, 7, 8], |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let nv = c.navy.expect("the coast's navy");
        let (sea, sky) = sea_and_sky(&c);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let both = |s: &Ships, due: Tick| at_sea(s, sea, due) && at_sea(s, sky, due);
        break_when(&mut c, &mut fake, nv, 300, both)?;
        let mut d = restarted(&c, seed, &tun);
        assert_eq!(d.ships.state_hash(), c.ships.state_hash(), "seed {seed}");
        let end = c.ships.now() + 150 * TICKS_PER_DAY;
        let mut fc = FakeShips::new(f64::from(tun.view_radius));
        let mut fd = FakeShips::new(f64::from(tun.view_radius));
        idle(&mut c, &mut fc, end);
        idle(&mut d, &mut fd, end);
        c.ships.check(&c.world).expect("check");
        d.ships.check(&d.world).expect("check restored");
        assert_eq!(c.ships.state_hash(), d.ships.state_hash(), "seed {seed}");
        assert_eq!(c.ships.havens, d.ships.havens, "seed {seed}");
        Some(c.ships.haven_counts().broken_shuts)
    });
    let n = done.iter().flatten().count();
    assert!(
        n >= 3,
        "only {n} seeds broke with both ships at sea: {done:?}"
    );
}

/// S8b review R4 (PC39): the tunables are the caller's, not the save's, so a load matches each
/// navy's looks to them. A save made with the looks off, read with them on, has a look booked
/// within `assault_every_days` of the restart; a save made with them on, read with them off, has
/// its look voided (it fires stale and books no other). `check()` passes on both.
#[test]
fn a_restart_matches_the_navy_looks_to_the_tunables() {
    let on = ShipTunables::default();
    let off = ShipTunables {
        assault_every_days: 0,
        ..ShipTunables::default()
    };
    let every = u64::from(on.assault_every_days) * TICKS_PER_DAY;
    for seed in [1, 2, 3] {
        // Off, then on.
        let mut c = synth::coastal(seed, off.clone());
        let nv = c.navy.expect("the coast's navy");
        let mut fake = FakeShips::new(f64::from(off.view_radius));
        idle(&mut c, &mut fake, Tick::from_days(10));
        assert_eq!(c.ships.assault_due(nv), None);
        let mut d = restarted(&c, seed, &on);
        let now = d.ships.now();
        let due = d.ships.assault_due(nv).expect("a look booked on load");
        assert!(due > now && due.0 <= now.0 + every, "seed {seed}: {due:?}");
        d.ships.check(&d.world).expect("check, looks on");
        idle(&mut d, &mut fake, now + 2 * every);
        assert!(d.ships.navy_counts().looked >= 2, "seed {seed}");
        d.ships.check(&d.world).expect("check, looks on");
        // On, then off.
        let mut c = synth::coastal(seed, on.clone());
        let mut fake = FakeShips::new(f64::from(on.view_radius));
        idle(&mut c, &mut fake, Tick::from_days(10));
        assert!(c.ships.assault_due(nv).is_some());
        let mut d = restarted(&c, seed, &off);
        assert_eq!(
            d.ships.assault_due(nv),
            None,
            "seed {seed}: the look is void"
        );
        d.ships.check(&d.world).expect("check, looks off");
        let (looked, stale) = (d.ships.navy_counts().looked, d.ships.stats.stale);
        let now = d.ships.now();
        idle(&mut d, &mut fake, now + 2 * every);
        assert_eq!(d.ships.navy_counts().looked, looked, "seed {seed}");
        assert!(
            d.ships.stats.stale > stale,
            "seed {seed}: the old look fired stale"
        );
        d.ships.check(&d.world).expect("check, looks off");
        // The same tunables change nothing.
        let e = restarted(&c, seed, &on);
        assert_eq!(e.ships.state_hash(), c.ships.state_hash(), "seed {seed}");
    }
}

/// The cause clock of a haven, rebuilt from the engine's log (PC31, PC35): it starts at the
/// first hunt one of the haven's pirates begins on a lane the navy patrols, and restarts at one
/// that comes more than `starve_days` after the last; a refound clears it. The coast's one haven
/// owns every pirate hunt in the log.
#[derive(Debug, Default)]
struct CauseModel {
    read: usize,
    since: Option<Tick>,
    last: Option<Tick>,
    /// The clock as PC31 first ran it, from the first patrolled hunt since the haven opened with
    /// no restart.
    first: Option<Tick>,
    patrolled: u64,
    /// Hunts on lanes no navy patrols, and those of them while the clock had not started.
    unpatrolled: u64,
    unpatrolled_before: u64,
    restarts: u64,
}

impl CauseModel {
    fn catch_up(&mut self, s: &Ships, nv: u32) {
        let starve = (s.tun.starve_days * TICKS_PER_DAY as f64).round() as u64;
        for e in &s.log[self.read..] {
            match e.kind {
                "hunt" => {
                    let lane: u32 = e
                        .what
                        .strip_prefix("hunts lane ")
                        .and_then(|r| r.split(' ').next())
                        .and_then(|l| l.parse().ok())
                        .expect("a hunt names its lane");
                    let at = Tick(e.tick);
                    if !patrols(s, nv, lane) {
                        self.unpatrolled += 1;
                        self.unpatrolled_before += u64::from(self.since.is_none());
                        continue;
                    }
                    self.patrolled += 1;
                    self.first.get_or_insert(at);
                    let gap = self.last.is_none_or(|t| at.0 - t.0 > starve);
                    if self.since.is_none() || gap {
                        self.restarts += u64::from(self.since.is_some());
                        self.since = Some(at);
                    }
                    self.last = Some(at);
                }
                "refound" if e.what.contains("is refounded") => {
                    (self.since, self.last, self.first) = (None, None, None);
                }
                _ => {}
            }
        }
        self.read = s.log.len();
    }
}

/// S8b `the_cause_clock_runs_on_patrolled_hunts_only` (§2.4.6a rule 3, PC31, PC35). The coast's
/// haven hunts lanes Saltmere's navy patrols and, more often, lanes it does not. Over 400 days
/// with the looks off, read every day, the engine's cause clock and its record of the navy's
/// lanes hunted equal a clock rebuilt from the log of hunts: no clock starts on a hunt off the
/// patrol, and one restarts after a month with no patrolled hunt. Over the seeds, hunts off the
/// patrol come before the clock starts, and clocks restart.
#[test]
fn the_cause_clock_runs_on_patrolled_hunts_only() {
    let tun = ShipTunables {
        assault_every_days: 0,
        ..ShipTunables::default()
    };
    let models = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.log_enabled = true;
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut m = CauseModel::default();
        for d in 1..=400 {
            idle(&mut c, &mut fake, Tick::from_days(d));
            m.catch_up(&c.ships, nv);
            let hv = &c.ships.havens[h as usize];
            assert_eq!(
                hv.holds_navy_lane_since, m.since,
                "seed {seed} day {d}: the cause clock"
            );
            let last = hv.navy_hunts.iter().find(|e| e.0 == nv).map(|e| e.1);
            if m.last
                .is_some_and(|t| Tick::from_days(d).0 - t.0 <= 30 * TICKS_PER_DAY)
            {
                assert_eq!(last, m.last, "seed {seed} day {d}: the navy's last hunt");
            }
        }
        c.ships.check(&c.world).expect("check");
        m
    });
    let before: u64 = models.iter().map(|m| m.unpatrolled_before).sum();
    let restarts: u64 = models.iter().map(|m| m.restarts).sum();
    eprintln!(
        "cause clock: {} hunts on the patrol, {} off it ({before} before a clock started), \
         {restarts} restarts over {} seeds",
        models.iter().map(|m| m.patrolled).sum::<u64>(),
        models.iter().map(|m| m.unpatrolled).sum::<u64>(),
        models.len()
    );
    assert!(before > 0, "no hunt off the patrol before a clock started");
    assert!(restarts > 0, "no clock restarted");
}

/// S8b `a_haven_back_on_the_patrol_waits_out_its_cause_again` (§2.4.6a rule 3, PC31, PC35). The
/// coast over 1,600 days with the navy looking every 2 days and the haven's pool at its cap at
/// every look, so it is never broken; at each look a restored copy of the engine is asked whether an empty pool
/// would break the haven then. It would exactly when the haven hunted a patrolled lane within
/// `starve_days` and its clock, rebuilt from the log, has run `assault_cause_days`; so a haven
/// that left the patrolled lanes for more than a month is not a target, nor on its return until
/// the cause has run again, though its first clock would have served. Over the seeds such looks
/// occur, and each seed has looks that would break it.
#[test]
fn a_haven_back_on_the_patrol_waits_out_its_cause_again() {
    let tun = looks_often();
    let cap = tun.crew_pool_cap * 1000;
    let cause = u64::from(tun.assault_cause_days) * TICKS_PER_DAY;
    let starve = (tun.starve_days * TICKS_PER_DAY as f64).round() as u64;
    let out = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        c.ships.log_enabled = true;
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let mut m = CauseModel::default();
        let (mut would, mut away, mut back) = (0u64, 0u64, 0u64);
        for _ in 0..800 {
            let due = to_look(&mut c, &mut fake, nv);
            m.catch_up(&c.ships, nv);
            let hv = &c.ships.havens[h as usize];
            let lately = m.last.is_some_and(|t| due.0 - t.0 <= starve);
            let served = |clock: Option<Tick>| clock.is_some_and(|t| due.0 >= t.0 + cause);
            let expect = hv.shut.is_none() && lately && served(m.since);
            // Looks PC31's first clock (no restart) would have passed: off the patrol for over
            // a month, or back on it with the clock started again and not yet run.
            if hv.shut.is_none() && served(m.first) && !expect {
                away += u64::from(!lately);
                back += u64::from(lately);
            }
            let mut d = restarted(&c, seed, &tun);
            set_pool(&mut d, h, 0);
            let mut fd = FakeShips::new(f64::from(tun.view_radius));
            idle_by(&mut d, &mut fd, due + 1, 1);
            let broke = d.ships.havens[h as usize].broken_at.is_some();
            assert_eq!(
                broke,
                expect,
                "seed {seed} day {:.2}: clock {:?}, last patrolled hunt {:?}",
                day(due),
                m.since.map(day),
                m.last.map(day)
            );
            would += u64::from(broke);
            set_pool(&mut c, h, cap);
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert!(c.ships.havens[h as usize].broken_at.is_none());
        }
        c.ships.check(&c.world).expect("check");
        (seed, would, away, back)
    });
    eprintln!("(seed, looks that would break, spared off the patrol, spared on return): {out:?}");
    assert!(
        out.iter().all(|&(_, w, _, _)| w > 0),
        "a seed never a target"
    );
    assert!(
        out.iter().any(|&(_, _, a, _)| a > 0),
        "no look off the patrol"
    );
    assert!(out.iter().any(|&(_, _, _, b)| b > 0), "no look on a return");
}

/// S8b `a_broken_havens_pirate_turns_back_at_its_lane` (§2.4.6a rule 3, PC36). The haven is broken
/// while one of its pirates sails out to a lane. At the lane it does not hunt: it turns for home
/// at once, pays no toll, feeds nothing and robs nothing (the haven's hunts, fed-until, tolls,
/// coast and port raids and the ship's crew and hold are as they were, a port raid called
/// straight included, and the lane has no hunter of it), and is laid up on the tick it comes
/// home.
#[test]
fn a_broken_havens_pirate_turns_back_at_its_lane() {
    let tun = looks_often();
    let done = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let (sea, sky) = sea_and_sky(&c);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let outbound = |s: &Ships, p: ShipId, due: Tick| {
            s.ship(p).is_some_and(Ship::is_active)
                && matches!(state(s, p), Some(PState::Outbound { arrive, .. }) if arrive > due + 1)
        };
        let ready = |s: &Ships, due: Tick| outbound(s, sea, due) || outbound(s, sky, due);
        break_when(&mut c, &mut fake, nv, 400, ready)?;
        let due = c.ships.havens[h as usize].broken_at.unwrap();
        let p = if outbound(&c.ships, sea, due) {
            sea
        } else {
            sky
        };
        let Some(PState::Outbound { lane, arrive }) = state(&c.ships, p) else {
            unreachable!()
        };
        let hv = &c.ships.havens[h as usize];
        let (fed, toll) = (hv.fed_until, hv.counts.toll_milli);
        let st = c.ships.stats;
        let x = c.ships.ship(p).unwrap();
        let (crew, cargo) = (x.crew, x.cargo.clone());
        idle_by(&mut c, &mut fake, arrive + 1, 1);
        let home = match state(&c.ships, p) {
            Some(PState::Homebound { arrive: home }) => home,
            other => panic!("seed {seed}: at the lane it is {other:?}"),
        };
        // Nor does it raid a port, from a hunt or called straight (S8b-3's port raids, PC36). The
        // coast's villages are too strong for its pirates, so the straight call is made with the
        // village undefended: only the break stops it.
        let def = c.ships.tun.port_village_defence_per_pop;
        c.ships.tun.port_village_defence_per_pop = 0.0;
        let now = c.ships.now();
        c.ships.port_raid(&mut c.world, p, lane, now);
        c.ships.tun.port_village_defence_per_pop = def;
        let hv = &c.ships.havens[h as usize];
        let after = c.ships.stats;
        assert_eq!(
            (after.hunts, after.coast_raids, after.port_raids),
            (st.hunts, st.coast_raids, st.port_raids),
            "seed {seed}: no hunt, no raid"
        );
        assert_eq!(
            (hv.fed_until, hv.counts.toll_milli),
            (fed, toll),
            "seed {seed}"
        );
        let x = c.ships.ship(p).unwrap();
        assert_eq!(
            (x.crew, &x.cargo),
            (crew, &cargo),
            "seed {seed}: no toll, no loot"
        );
        assert!(
            c.ships.threat(lane).is_none_or(|t| !t.hunters.contains(&p)),
            "seed {seed}: not a hunter of lane {lane}"
        );
        idle_by(&mut c, &mut fake, home + 1, 1);
        let x = c.ships.ship(p).unwrap();
        assert!(
            x.is_laid_up()
                && matches!(x.status, rtsim_ships::engine::Status::Lost { since, .. } if since == home),
            "seed {seed}: laid up on coming home: {x:?}"
        );
        c.ships.check(&c.world).expect("check");
        Some(seed)
    });
    let n = done.iter().flatten().count();
    assert!(
        n >= 5,
        "only {n} seeds broke with a pirate outbound: {done:?}"
    );
}

/// S8b `a_ship_home_in_view_rests_until_the_player_leaves` (§2.4.6a rule 3, PC33: nothing happens
/// in a player's view). The haven is broken while its sea ship is at sea; a player then stands
/// 100 blocks off its dock. The ship comes home in view, embodied, and rests rather than being
/// laid up; for 30 days it rests on, active, and the haven stays open. The player leaves, the
/// plugin releases the body, and the ship is laid up at its next decision; the haven shuts,
/// broken, when its other ship is laid up too.
#[test]
fn a_ship_home_in_view_rests_until_the_player_leaves() {
    let tun = looks_often();
    let done = per_seed(&seeds(), |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let (sea, _) = sea_and_sky(&c);
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        let ready = |s: &Ships, due: Tick| {
            at_sea(s, sea, due)
                && matches!(
                    state(s, sea),
                    Some(PState::Hunting { until, .. }) if until > due + TICKS_PER_DAY / 4
                )
        };
        break_when(&mut c, &mut fake, nv, 400, ready)?;
        let now = c.ships.now();
        let hv = &c.ships.havens[h as usize];
        let (frame, dock) = (hv.frame, hv.dock);
        let pos = rtsim_settle::Pos::new(frame, dock.0 + 100, dock.2);
        let player = c.ships.player_stays(
            &mut c.world,
            synth::SCRIPT_PLAYER,
            pos,
            now,
            now + 200 * TICKS_PER_DAY,
        );
        fake.set_player(
            synth::SCRIPT_PLAYER,
            f64::from(dock.0 + 100),
            f64::from(dock.2),
        );
        // Home in view: it rests, embodied.
        let mut t = now;
        while !resting(&c.ships, sea) {
            t += TICKS_PER_DAY / 96;
            assert!(t < now + 40 * TICKS_PER_DAY, "seed {seed}: never home");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
            assert!(
                !c.ships.ship(sea).unwrap().is_laid_up(),
                "seed {seed}: laid up in view"
            );
        }
        assert!(
            c.ships.ship(sea).unwrap().embodied,
            "seed {seed}: home in view"
        );
        let home = c.ships.now();
        idle_by(
            &mut c,
            &mut fake,
            home + 30 * TICKS_PER_DAY,
            TICKS_PER_DAY / 8,
        );
        let x = c.ships.ship(sea).unwrap();
        assert!(
            x.is_active() && x.embodied,
            "seed {seed}: rests on in view: {x:?}"
        );
        assert!(c.ships.havens[h as usize].shut.is_none(), "seed {seed}");
        // The player leaves: laid up at its next decision out of view.
        let left = c.ships.now();
        c.ships.player_left(&mut c.world, player);
        fake.set_player(synth::SCRIPT_PLAYER, 1.0e9, 1.0e9);
        let mut t = left;
        while !c.ships.ship(sea).unwrap().is_laid_up() {
            t += TICKS_PER_DAY / 96;
            assert!(t < left + 10 * TICKS_PER_DAY, "seed {seed}: never laid up");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
        }
        let x = c.ships.ship(sea).unwrap();
        assert!(!x.embodied && x.crew == 0, "seed {seed}: {x:?}");
        // The sky ship may be out yet: the haven shuts, broken, with its last ship.
        while c.ships.havens[h as usize].shut.is_none() {
            t += TICKS_PER_DAY / 8;
            assert!(t < left + 40 * TICKS_PER_DAY, "seed {seed}: never shut");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
        }
        let sh = c.ships.havens[h as usize].shut.unwrap();
        assert!(sh.broken && sh.since > left, "seed {seed}: {sh:?}");
        c.ships.check(&c.world).expect("check");
        Some(seed)
    });
    let n = done.iter().flatten().count();
    assert!(n >= 5, "only {n} seeds: {done:?}");
}

/// S8b `no_assault_while_a_haven_ship_is_embodied` (§2.4.6a rule 3: nothing happens in a player's
/// view). Once the coast's haven is a target, a player follows its sea pirate onto each lane it
/// hunts more than `view_radius` from the haven's dock, so the pirate is embodied while the dock
/// is out of every view. At each look with a haven ship embodied the pool is emptied (weak at
/// any defence in port); at the others it is at its cap. No look breaks it until ten looks have
/// found it a target with a ship embodied; then, the player gone, a look at a target with the
/// pool empty breaks it.
#[test]
fn no_assault_while_a_haven_ship_is_embodied() {
    let tun = looks_often();
    let cap = tun.crew_pool_cap * 1000;
    per_seed(&[1, 2, 3, 4, 5, 6], |seed| {
        let mut c = synth::coastal(seed, tun.clone());
        let (h, nv) = (c.haven, c.navy.expect("the coast's navy"));
        let (sea, _) = sea_and_sky(&c);
        let slots = c.ships.havens[h as usize].ships.clone();
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        while !a_target(&c.ships, nv, h, c.ships.now()) {
            let due = to_look(&mut c, &mut fake, nv);
            assert!(due < Tick::from_days(600), "seed {seed}: no target");
            set_pool(&mut c, h, cap);
            idle_by(&mut c, &mut fake, due + 1, 1);
        }
        let hv = &c.ships.havens[h as usize];
        let (frame, dock) = (hv.frame, (hv.dock.0, hv.dock.2));
        let far = f64::from(tun.view_radius) + 64.0;
        let mut player = None;
        let (mut looks, mut seen) = (0, 0);
        while seen < 10 {
            looks += 1;
            assert!(
                looks <= 400,
                "seed {seed}: {seen} looks with a ship embodied"
            );
            let due = c.ships.assault_due(nv).expect("a navy looks");
            while c.ships.now() < Tick(due.0 - 1) {
                let t = (c.ships.now() + TICKS_PER_DAY / 8).min(Tick(due.0 - 1));
                idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
                let now = c.ships.now();
                match (player, state(&c.ships, sea)) {
                    (None, Some(PState::Hunting { until, .. }))
                        if c.ships.ship(sea).is_some_and(Ship::is_active) =>
                    {
                        let at = c.ships.position(&c.world, sea, now);
                        let (dx, dz) = (f64::from(at.0 - dock.0), f64::from(at.1 - dock.1));
                        if (dx * dx + dz * dz).sqrt() > far {
                            let pos = rtsim_settle::Pos::new(frame, at.0, at.1);
                            player = Some(c.ships.player_stays(
                                &mut c.world,
                                synth::SCRIPT_PLAYER,
                                pos,
                                now,
                                until,
                            ));
                            fake.set_player(synth::SCRIPT_PLAYER, f64::from(at.0), f64::from(at.1));
                        }
                    }
                    (Some(p), st) if !matches!(st, Some(PState::Hunting { .. })) => {
                        c.ships.player_left(&mut c.world, p);
                        fake.set_player(synth::SCRIPT_PLAYER, 1.0e9, 1.0e9);
                        player = None;
                    }
                    _ => {}
                }
            }
            let s = &c.ships;
            let embodied = slots.iter().any(|&p| s.ship(p).is_some_and(|x| x.embodied));
            let target = a_target(s, nv, h, due);
            set_pool(&mut c, h, if embodied { 0 } else { cap });
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert!(
                c.ships.havens[h as usize].broken_at.is_none(),
                "seed {seed}: broken with a ship embodied"
            );
            seen += usize::from(embodied && target);
        }
        if let Some(p) = player {
            c.ships.player_left(&mut c.world, p);
        }
        fake.set_player(synth::SCRIPT_PLAYER, 1.0e9, 1.0e9);
        let mut looks = 0;
        while c.ships.havens[h as usize].broken_at.is_none() {
            looks += 1;
            assert!(looks <= 400, "seed {seed}: not broken once the player left");
            let due = to_look(&mut c, &mut fake, nv);
            let s = &c.ships;
            let weak = a_target(s, nv, h, due)
                && slots.iter().all(|&p| s.ship(p).is_none_or(|x| !x.embodied));
            set_pool(&mut c, h, if weak { 0 } else { cap });
            idle_by(&mut c, &mut fake, due + 1, 1);
        }
        c.ships.check(&c.world).expect("check");
    });
}

/// One run of the coast with navy assaults on and nobody watching (§2.4.6a rule 3).
#[derive(Debug, Default)]
struct Cycle {
    seed: u64,
    cause_days: u32,
    years: u64,
    /// The mean share of slots manned, censused every 24 days.
    manned: f64,
    breaks: u64,
    starved: u64,
    /// Days from the haven's opening (its founding or refound) to each break.
    open_to_break: Vec<f64>,
    /// Days each shut lasted, to its refound.
    closed: Vec<f64>,
    /// Hunts in each 5-year window.
    window_hunts: Vec<u64>,
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// `years` of the coast with assaults on and `assault_cause_days` at `cause_days`, nobody
/// watching, read every quarter day: no break comes sooner than the cause after the haven opened,
/// every break is one assault, and `check()` passes every 5 years.
fn coast_cycle(seed: u64, cause_days: u32, years: u64) -> Cycle {
    let tun = ShipTunables {
        assault_cause_days: cause_days,
        ..ShipTunables::default()
    };
    let mut c = synth::coastal(seed, tun.clone());
    let h = c.haven;
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let mut cy = Cycle {
        seed,
        cause_days,
        years,
        ..Cycle::default()
    };
    let cause = u64::from(cause_days) * TICKS_PER_DAY;
    let (mut opened, mut was) = (Tick::ZERO, None::<rtsim_ships::engine::Shut>);
    let mut seen = None;
    let (mut samples, mut hunts0) = (0u64, 0u64);
    let window = 5 * YEAR * TICKS_PER_DAY;
    let mut t = Tick::ZERO;
    while t < Tick::from_days(years * YEAR) {
        t += TICKS_PER_DAY / 4;
        idle(&mut c, &mut fake, t);
        let hv = &c.ships.havens[h as usize];
        if let Some(b) = hv.broken_at
            && seen != Some(b)
        {
            seen = Some(b);
            cy.breaks += 1;
            assert!(
                b.0 >= opened.0 + cause,
                "seed {seed}: broken on day {} after opening on day {}",
                day(b),
                day(opened)
            );
            cy.open_to_break.push(day(b) - day(opened));
        }
        match (was, hv.shut) {
            (Some(sh), None) => {
                // Refounded at its look (the look's tick, read before it).
                opened = sh.back;
                cy.closed.push(day(sh.back) - day(sh.since));
            }
            (_, Some(sh)) if was.is_none() && !sh.broken => cy.starved += 1,
            _ => {}
        }
        was = hv.shut;
        if t.0.is_multiple_of(24 * TICKS_PER_DAY) {
            let (up, n) = manned(&c.ships, h);
            cy.manned += up as f64 / n as f64;
            samples += 1;
        }
        if t.0.is_multiple_of(window) {
            cy.window_hunts.push(c.ships.stats.hunts - hunts0);
            hunts0 = c.ships.stats.hunts;
            c.ships
                .check(&c.world)
                .unwrap_or_else(|e| panic!("seed {seed} cause {cause_days} day {}: {e}", day(t)));
        }
    }
    cy.manned /= samples.max(1) as f64;
    assert_eq!(cy.breaks, c.ships.navy_counts().assaults, "seed {seed}");
    cy
}

/// S8b `coast_havens_cycle_with_assaults` (§2.4.6a rule 3, PC31, PC35, PC38): 30 years of the
/// coast with nobody watching and Saltmere's navy assaulting, at the default
/// `assault_cause_days` and with no such wait (0), on seeds 1, 7 and 30. It reports the share of
/// slots manned, breaks per decade, the mean days from opening to a break and the mean days shut.
/// It asserts a hunt in every 5-year window and no break sooner than the cause after the haven
/// opened; at the default, that the navy breaks the haven on every seed and the share manned
/// stays at least `COAST_MANNED_FLOOR`.
#[test]
fn coast_havens_cycle_with_assaults() {
    let jobs: Vec<u64> = [1u64, 7, 30]
        .iter()
        .flat_map(|&s| [2 * s, 2 * s + 1])
        .collect();
    let cause = ShipTunables::default().assault_cause_days;
    let runs = per_seed(&jobs, |j| {
        coast_cycle(j / 2, if j % 2 == 0 { cause } else { 0 }, 30)
    });
    for cy in &runs {
        eprintln!(
            "coast seed {:>2} cause {:>2} days: manned {:.3}, breaks {} ({:.2}/decade), starved \
             shuts {}, opening to break {:.1} days, shut {:.1} days, fewest hunts in 5 years {}",
            cy.seed,
            cy.cause_days,
            cy.manned,
            cy.breaks,
            cy.breaks as f64 * 10.0 / cy.years as f64,
            cy.starved,
            mean(&cy.open_to_break),
            mean(&cy.closed),
            cy.window_hunts.iter().min().copied().unwrap_or(0),
        );
        assert!(
            cy.window_hunts.iter().all(|&n| n > 0),
            "seed {} cause {}: a 5-year window with no hunt: {cy:?}",
            cy.seed,
            cy.cause_days
        );
        if cy.cause_days == cause {
            assert!(
                cy.breaks > 0,
                "seed {}: the navy never broke the haven",
                cy.seed
            );
            assert!(
                cy.manned >= COAST_MANNED_FLOOR,
                "seed {}: manned {:.3} under {COAST_MANNED_FLOOR}",
                cy.seed,
                cy.manned
            );
        }
    }
}

/// The least share of the coast's slots manned over 30 years at the default
/// `assault_cause_days` (PC38): the sweep's bar, 0.83. Measured at 192: 0.867, 0.900 and 0.850 on
/// seeds 1, 7 and 30.
const COAST_MANNED_FLOOR: f64 = 0.83;

// ---- many worlds, nobody watching ----------------------------------------------------------------

/// One world's unattended run, summed (§2.4.6a): the lifecycle on or off.
#[derive(Clone, Debug, Default)]
struct Life {
    seed: u64,
    lifecycle: bool,
    sites: usize,
    slots: usize,
    navies: usize,
    years: u64,
    /// The mean share of sites open and of slots manned, censused every 24 days.
    open: f64,
    manned: f64,
    /// The fewest sites open at a census.
    min_open: usize,
    /// The longest a site stood shut at a census, in days (a spell still running counts).
    longest_shut: f64,
    /// Days with every site shut at once, read every quarter day, and the longest such spell.
    all_shut_days: f64,
    longest_all_shut: f64,
    /// Spells of every site shut longer than the refound base plus one retry, with the looks
    /// that found no prey during each (the bar's exception).
    long_all_shut: Vec<(f64, u32)>,
    /// Hunts in each window (a century, or a decade in the everyday cut).
    window_hunts: Vec<u64>,
    /// `assault_cause_days` (PC31; meaningless with the lifecycle off).
    cause_days: u32,
    shuts: u64,
    /// Of `shuts`, those of a haven a navy broke (rule 3); the rest starved (rule 2).
    broken_shuts: u64,
    refounds: u64,
    laid_up: u64,
    hunts: u64,
    toll_men: f64,
    sinkings: u64,
    /// The navies' looks for a haven to assault, and the assaults made (each broke a haven).
    looked: u64,
    assaults: u64,
}

/// `windows` windows of `years` each of [`synth::sea_havens`] with nobody watching, censused every
/// 24 days and checked at each window's end.
fn unattended(seed: u64, lifecycle: bool, cause_days: u32, windows: u64, years: u64) -> Life {
    let tun = ShipTunables {
        lifecycle,
        assault_cause_days: cause_days,
        ..ShipTunables::default()
    };
    let SeaHavens {
        mut world,
        mut ships,
        mask,
        havens,
        navies,
        ..
    } = synth::sea_havens(seed, tun.clone());
    let mut fake = FakeShips::new(f64::from(tun.view_radius));
    let slots: usize = havens
        .iter()
        .map(|&h| ships.havens[h as usize].ships.len())
        .sum();
    let mut life = Life {
        seed,
        lifecycle,
        cause_days,
        sites: havens.len(),
        slots,
        navies: navies.len(),
        years: windows * years,
        min_open: havens.len(),
        ..Life::default()
    };
    let (mut samples, mut hunts0) = (0u64, 0u64);
    let mut t = Tick::ZERO;
    let end = Tick::from_days(windows * years * YEAR);
    // A spell of every site shut: when it began and the looks that found no prey in it (a shut
    // haven's look booked again while its shut stands).
    let (mut spell, mut backs) = (None::<(Tick, u32)>, vec![None::<Tick>; havens.len()]);
    let bar = 2.0 * f64::from(tun.refound.base_days);
    while t < end {
        // Every quarter day, as `drive` steps: the run is the same, the spells exact to 6 hours.
        t += TICKS_PER_DAY / 4;
        synth::drive(
            &mut world,
            &mut ships,
            &mut fake,
            &mut NoInfrastructure,
            &mask,
            t,
            TICKS_PER_DAY / 4,
        );
        let mut failed = 0;
        for (i, &h) in havens.iter().enumerate() {
            let back = ships.havens[h as usize].shut.map(|sh| sh.back);
            if back.is_some() && backs[i].is_some() && back != backs[i] {
                failed += 1;
            }
            backs[i] = back;
        }
        let all_shut = havens
            .iter()
            .all(|&h| ships.havens[h as usize].shut.is_some());
        spell = match (spell, all_shut) {
            (None, true) => Some((t, 0)),
            (Some((t0, n)), true) => Some((t0, n + failed)),
            (Some((t0, n)), false) => {
                let len = day(t) - day(t0);
                life.longest_all_shut = life.longest_all_shut.max(len);
                if len > bar {
                    life.long_all_shut.push((len, n));
                }
                None
            }
            (None, false) => None,
        };
        if all_shut {
            life.all_shut_days += 0.25;
        }
        if !t.0.is_multiple_of(24 * TICKS_PER_DAY) {
            continue;
        }
        let open = havens
            .iter()
            .filter(|&&h| ships.havens[h as usize].shut.is_none())
            .count();
        let up: usize = havens.iter().map(|&h| manned(&ships, h).0).sum();
        // A shut haven is manned by nobody, and its look is never overdue (every look due by now
        // has run) nor further off than the renewal rule's cap: a look is booked at shut or
        // refound-failed time, the first at most `cap_days` on, a retry `base_days` on. Whether a
        // look finds prey is the world's; the longest shut spell is reported, not bounded.
        let cap = Tick::from_days(u64::from(tun.refound.cap_days as u32)).0;
        for &h in &havens {
            let hv = &ships.havens[h as usize];
            if let Some(sh) = hv.shut {
                assert_eq!(manned(&ships, h).0, 0, "seed {seed}: shut but manned");
                assert!(sh.back > t, "seed {seed}: haven {h}'s look is overdue");
                assert!(
                    sh.back <= t + cap,
                    "seed {seed}: haven {h}'s look is {:.0} days off",
                    day(sh.back) - day(t)
                );
                life.longest_shut = life.longest_shut.max(day(t) - day(sh.since));
            }
        }
        assert!(up <= slots, "seed {seed}");
        assert!(
            life.lifecycle || open == life.sites,
            "seed {seed}: shut with the lifecycle off"
        );
        life.open += open as f64 / life.sites as f64;
        life.manned += up as f64 / slots as f64;
        life.min_open = life.min_open.min(open);
        samples += 1;
        // The conservation and the indexes, every census (24 days).
        ships
            .check(&world)
            .unwrap_or_else(|e| panic!("seed {seed} lifecycle {lifecycle} day {}: {e}", day(t)));
        if t.0.is_multiple_of(years * YEAR * TICKS_PER_DAY) {
            life.window_hunts.push(ships.stats.hunts - hunts0);
            hunts0 = ships.stats.hunts;
        }
    }
    if let Some((t0, n)) = spell {
        let len = day(t) - day(t0);
        life.longest_all_shut = life.longest_all_shut.max(len);
        if len > bar {
            life.long_all_shut.push((len, n));
        }
    }
    life.open /= samples as f64;
    life.manned /= samples as f64;
    let counts = ships.haven_counts();
    life.shuts = counts.shuts;
    life.broken_shuts = counts.broken_shuts;
    life.refounds = counts.refounds;
    let navy = ships.navy_counts();
    life.looked = navy.looked;
    life.assaults = navy.assaults;
    life.laid_up = counts.laid_up;
    life.toll_men = counts.toll_milli as f64 / 1000.0;
    life.hunts = ships.stats.hunts;
    life.sinkings = ships.stats.sinkings;
    life
}

fn report(runs: &[Life]) {
    for l in runs {
        eprintln!(
            "seed {:>2} lifecycle {:<5} cause {:>2} sites {} slots {} navies {}: open {:.3} (min \
             {}), manned {:.3}, shuts {} (starved {}, broken {}), assaults {} of {} looks, \
             refounds {}, laid up {}, sinkings {}, hunts/yr {:.1}, toll {:.1} men ({:.2}/yr), \
             fewest hunts in a window {}, longest shut {:.0} days, every site shut {:.4} of the \
             time (longest {:.1} days)",
            l.seed,
            l.lifecycle,
            if l.lifecycle { l.cause_days } else { 0 },
            l.sites,
            l.slots,
            l.navies,
            l.open,
            l.min_open,
            l.manned,
            l.shuts,
            l.shuts - l.broken_shuts,
            l.broken_shuts,
            l.assaults,
            l.looked,
            l.refounds,
            l.laid_up,
            l.sinkings,
            l.hunts as f64 / l.years as f64,
            l.toll_men,
            l.toll_men / l.years as f64,
            l.window_hunts.iter().min().copied().unwrap_or(0),
            l.longest_shut,
            l.all_shut_days / (l.years * YEAR) as f64,
            l.longest_all_shut,
        );
    }
}

/// The seas together, one arm of the lifecycle (`label`) against it off: shares open, the events
/// a year, and the hunts the lifecycle costs.
fn summary(label: &str, on: &[Life], off: &[Life]) {
    let years: f64 = on.iter().map(|l| l.years as f64).sum();
    let sum = |v: &[Life], f: &dyn Fn(&Life) -> f64| v.iter().map(f).sum::<f64>();
    let seas = on.len() as f64;
    let open_mean = sum(on, &|l| l.open) / seas;
    let open_weighted = sum(on, &|l| l.open * l.sites as f64) / sum(on, &|l| l.sites as f64);
    let open_lowest = on.iter().map(|l| l.open).fold(1.0, f64::min);
    let manned_mean = sum(on, &|l| l.manned) / seas;
    let (shuts, refounds, laid_up) = (
        sum(on, &|l| l.shuts as f64),
        sum(on, &|l| l.refounds as f64),
        sum(on, &|l| l.laid_up as f64),
    );
    let shutting = on.iter().filter(|l| l.shuts > 0).count();
    let (hunts_on, hunts_off) = (sum(on, &|l| l.hunts as f64), sum(off, &|l| l.hunts as f64));
    let worst = on
        .iter()
        .filter_map(|l| {
            let o = off.iter().find(|o| o.seed == l.seed)?;
            Some((l.seed, 1.0 - l.hunts as f64 / o.hunts as f64, l.shuts))
        })
        .fold((0, f64::MIN, 0), |a, b| if b.1 > a.1 { b } else { a });
    eprintln!(
        "ALL {} seas, {} years each, {label}: sites open {open_mean:.3} (site-weighted \
         {open_weighted:.3}, lowest sea {open_lowest:.3}, fewest at a census {}), slots manned \
         {manned_mean:.3}; seas where a haven shut {shutting}; per sea-year: shuts {:.4}, \
         refounds {:.4}, laid up {:.4}, hunts {:.2}, toll {:.3} men; longest shut spell {:.0} \
         days; every site shut {:.5} of the sea-time, in {} seas (longest {:.1} days)",
        on.len(),
        on.first().map_or(0, |l| l.years),
        on.iter().map(|l| l.min_open).min().unwrap_or(0),
        shuts / years,
        refounds / years,
        laid_up / years,
        hunts_on / years,
        sum(on, &|l| l.toll_men) / years,
        on.iter().map(|l| l.longest_shut).fold(0.0, f64::max),
        sum(on, &|l| l.all_shut_days) / (years * YEAR as f64),
        on.iter().filter(|l| l.all_shut_days > 0.0).count(),
        on.iter().map(|l| l.longest_all_shut).fold(0.0, f64::max),
    );
    eprintln!(
        "ALL lifecycle off: hunts {:.2} per sea-year; {label} has {:.2}% fewer (largest drop \
         {:.1}% in seed {}, which shut {} times)",
        hunts_off / years,
        100.0 * (1.0 - hunts_on / hunts_off),
        100.0 * worst.1,
        worst.0,
        worst.2,
    );
}

/// The lifecycle's bars on many worlds (EQUILIBRIUM.md S8b): no world has every site shut for
/// longer than the refound base plus one retry (96 days) unless looks in that spell found no
/// prey (PC37: a sea the navies cleared for a season refills; one with no prey may not); across
/// the worlds, havens do shut and refound (they neither all die nor all hold every site for
/// good); every world hunts in every window; and open havens never exceed the sites found. With
/// the lifecycle off nothing shuts and no toll is paid (S8's havens).
fn check_lives(on: &[Life], off: &[Life]) {
    for l in on {
        assert!(
            l.long_all_shut.iter().all(|&(_, failed)| failed >= 2),
            "seed {}: every site shut past the refound base and a retry, with prey: {:?}",
            l.seed,
            l.long_all_shut
        );
        assert!(
            l.window_hunts.iter().all(|&n| n > 0),
            "seed {}: a window with no hunt: {l:?}",
            l.seed
        );
    }
    let shuts: u64 = on.iter().map(|l| l.shuts).sum();
    let refounds: u64 = on.iter().map(|l| l.refounds).sum();
    assert!(shuts > 0, "no haven ever shut: {on:?}");
    assert!(refounds > 0, "no site ever refounded: {on:?}");
    for l in off {
        assert_eq!((l.shuts, l.refounds, l.laid_up), (0, 0, 0), "{l:?}");
        assert_eq!(
            (l.looked, l.assaults),
            (0, 0),
            "no assault with the lifecycle off: {l:?}"
        );
        assert_eq!(l.toll_men, 0.0);
        assert!(l.window_hunts.iter().all(|&n| n > 0), "{l:?}");
    }
}

/// The three arms of the unattended runs.
struct Lives {
    /// The lifecycle on, as built (`assault_cause_days` of robbery before an assault, PC31, PC38).
    on: Vec<Life>,
    /// The lifecycle on with no wait before an assault (`assault_cause_days` 0), measured only.
    no_cause: Vec<Life>,
    off: Vec<Life>,
}

/// Every seed's unattended run with the lifecycle on, on with no cause gate, and off, `threads`
/// runs at a time.
fn lives(threads: usize, seeds: &[u64], windows: u64, years: u64) -> Lives {
    let cause = ShipTunables::default().assault_cause_days;
    let jobs: Vec<u64> = seeds
        .iter()
        .flat_map(|&s| [3 * s, 3 * s + 1, 3 * s + 2])
        .collect();
    let all = per_seed_on(threads, &jobs, |j| match j % 3 {
        0 => unattended(j / 3, true, cause, windows, years),
        1 => unattended(j / 3, true, 0, windows, years),
        _ => unattended(j / 3, false, cause, windows, years),
    });
    let mut out = Lives {
        on: Vec::new(),
        no_cause: Vec::new(),
        off: Vec::new(),
    };
    for l in all {
        match (l.lifecycle, l.cause_days) {
            (false, _) => out.off.push(l),
            (true, 0) => out.no_cause.push(l),
            (true, _) => out.on.push(l),
        }
    }
    out
}

/// Each arm's lines and sums, then the bars on the built arm and the off arm.
fn report_and_check(l: &Lives) {
    report(&l.on);
    report(&l.no_cause);
    report(&l.off);
    summary("lifecycle on", &l.on, &l.off);
    summary("on with no cause wait", &l.no_cause, &l.off);
    check_lives(&l.on, &l.off);
}

/// S8b `unattended_havens_rise_and_fall`, the everyday cut: three decades of three seas, the
/// lifecycle on and off (seed 2's sea has havens that shut and refound within them), and on with
/// no wait before an assault (reported, not judged).
#[test]
fn unattended_havens_rise_and_fall() {
    report_and_check(&lives(6, &[1, 2, 3], 3, 10));
}

/// S8b `unattended_havens_rise_and_fall` in full: ten centuries (96,000 game days) of seeds 1 to
/// 30, the lifecycle on and off, and on with no wait before an assault. Release only,
/// for its length (`cargo test --release -p rtsim-ships --test lifecycle -- --ignored`).
#[test]
#[ignore = "long: run in release with --ignored"]
fn unattended_havens_rise_and_fall_1000_years() {
    report_and_check(&lives(12, &seeds(), 10, 100));
}

/// The sweep that chose `assault_cause_days` (PC38): each gate in [`CAUSE_SWEEP`] on the coast
/// (30 years, seeds 1, 7 and 30: the share of slots manned and breaks per decade) and on
/// `sea_havens` (1,000 years, seeds 1 to 30: sites open, the lowest sea, the share of the time
/// with every site of a sea shut and the longest such spell, hunts a year). Reports only.
/// Release only (`cargo test --release -p rtsim-ships --test lifecycle -- --ignored
/// assault_cause_sweep`).
#[test]
#[ignore = "long: run in release with --ignored"]
fn assault_cause_sweep() {
    let gates = CAUSE_SWEEP.len() as u64;
    let gate = |j: u64| CAUSE_SWEEP[(j % gates) as usize];
    let coast_jobs: Vec<u64> = [1u64, 7, 30]
        .iter()
        .flat_map(|&s| (0..gates).map(move |g| s * gates + g))
        .collect();
    let sea_jobs: Vec<u64> = seeds()
        .iter()
        .flat_map(|&s| (0..gates).map(move |g| s * gates + g))
        .collect();
    let coast = per_seed_on(16, &coast_jobs, |j| coast_cycle(j / gates, gate(j), 30));
    let seas = per_seed_on(16, &sea_jobs, |j| {
        unattended(j / gates, true, gate(j), 10, 100)
    });
    for &g in &CAUSE_SWEEP {
        let c: Vec<&Cycle> = coast.iter().filter(|c| c.cause_days == g).collect();
        let v: Vec<&Life> = seas.iter().filter(|l| l.cause_days == g).collect();
        let n = v.len() as f64;
        let sea_time: f64 = v.iter().map(|l| (l.years * YEAR) as f64).sum();
        eprintln!(
            "SWEEP cause {g:>3}: coast manned {:.3} ({}; breaks a decade {}); seas open {:.3} (lowest \
             {:.3}, seed {}), every site shut {:.5} of the sea-time in {} seas (longest {:.1} \
             days), hunts a year {:.1}, shuts {} (starved {}, broken {})",
            c.iter().map(|c| c.manned).sum::<f64>() / c.len() as f64,
            c.iter()
                .map(|c| format!("{:.3}", c.manned))
                .collect::<Vec<_>>()
                .join("/"),
            c.iter()
                .map(|c| format!("{:.2}", c.breaks as f64 * 10.0 / c.years as f64))
                .collect::<Vec<_>>()
                .join("/"),
            v.iter().map(|l| l.open).sum::<f64>() / n,
            v.iter().map(|l| l.open).fold(1.0, f64::min),
            v.iter()
                .min_by(|a, b| a.open.total_cmp(&b.open))
                .map_or(0, |l| l.seed),
            v.iter().map(|l| l.all_shut_days).sum::<f64>() / sea_time,
            v.iter().filter(|l| l.all_shut_days > 0.0).count(),
            v.iter().map(|l| l.longest_all_shut).fold(0.0, f64::max),
            v.iter()
                .map(|l| l.hunts as f64 / l.years as f64)
                .sum::<f64>()
                / n,
            v.iter().map(|l| l.shuts).sum::<u64>(),
            v.iter().map(|l| l.shuts - l.broken_shuts).sum::<u64>(),
            v.iter().map(|l| l.broken_shuts).sum::<u64>(),
        );
    }
    report(&seas);
}

/// The gates [`assault_cause_sweep`] tries, in days (PC38).
const CAUSE_SWEEP: [u32; 4] = [96, 144, 192, 240];
