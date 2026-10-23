// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b (EQUILIBRIUM.md §2.4.6a; docs/choices-ports.md PC1 to PC8, PC30 on): havens that rise and
//! fall. An unwatched hunt pays the square law's toll in crew, a haven with no hunt for 30 days
//! starves and lays its ships up, a starving haven with no ship shuts, a navy breaks a weak haven
//! that has robbed its patrolled lanes for a year, and a shut haven's site takes a new band out of
//! every player's view once a fresh band would find prey.
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

/// [`per_seed`] on up to `threads` threads (the long run's sixty jobs take more).
fn per_seed_on<T: Send>(threads: usize, seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, threads));
    let chunk = seeds.len().div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        let hs: Vec<_> = seeds
            .chunks(chunk)
            .map(|c| {
                let f = &f;
                sc.spawn(move || c.iter().map(|&s| f(s)).collect::<Vec<T>>())
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("a seed's run panicked"))
            .collect()
    })
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
/// run that was never saved. No navy assaults (PC34): the way out of the first state is the
/// wake, and a navy breaking the haven within the 120 days would hide it.
#[test]
fn a_restart_in_each_lifecycle_state_changes_nothing() {
    let tun = ShipTunables {
        assault_every_days: 0,
        ..ShipTunables::default()
    };
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

/// Whether haven `h`'s pirates hunt a lane navy `nv`'s patrol sails (rule 3's targets).
fn hunts_the_patrol(s: &Ships, nv: u32, h: u32) -> bool {
    let sv = s.navies[nv as usize].service;
    s.services[sv as usize]
        .legs
        .iter()
        .any(|g| s.threat(g.hop.lane).and_then(|t| t.haven) == Some(h))
}

/// Whether haven `h` has held a patrolled lane for `assault_cause_days` at `at` (PC31).
fn cause_served(s: &Ships, h: u32, at: Tick) -> bool {
    let cause = u64::from(s.tun.assault_cause_days) * TICKS_PER_DAY;
    s.havens[h as usize]
        .holds_navy_lane_since
        .is_some_and(|t0| at.0 >= t0.0 + cause)
}

/// Whether navy `nv` may assault haven `h` at `at`, out of view: its pirates hunt the patrol's
/// lanes and the cause has run its year.
fn a_target(s: &Ships, nv: u32, h: u32, at: Tick) -> bool {
    hunts_the_patrol(s, nv, h) && cause_served(s, h, at)
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

/// S8b `a_navy_breaks_a_weak_haven` (§2.4.6a rule 3, PC7, PC8, PC31). The coast's sky lanes are
/// escorted, so its sky pirate rests in port while its sea pirate hunts. Every look from the
/// haven's first hunt on a lane Saltmere's navy patrols finds its defence just below the gate:
/// 12.6 men (`A / sqrt 2` is 12.73 for the navy's 2 x 6 x 1.5), its ships in port plus the rest
/// ashore; other looks find the pool at its cap. For the first 96 days no look breaks it (a year
/// of robbery first). The first look after, with the sky ship in port and the sea ship at sea,
/// breaks the haven: the sky ship is sunk at once, by the navy, with no
/// pressure; the men ashore are gone; nothing is refitted. The sea ship finishes its hunt and is
/// laid up as it comes home, and the haven shuts then, counted broken, its site due to refound
/// 48 days later (P = 0), which it does. The broken state reads back across a restart.
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
            assert!(looks <= 200, "seed {seed}: never a weak moment in 400 days");
            let due = to_look(&mut c, &mut fake, nv);
            let s = &c.ships;
            let held = hunts_the_patrol(s, nv, h);
            let stays = s.havens[h as usize]
                .ships
                .iter()
                .all(|&p| next_change(s, p).is_none_or(|t| t > due));
            if held && !cause_served(s, h, due) && stays && in_port(s, h) <= 12.6 {
                // Weak, but the year is not out.
                early += 1;
                let pool = ((12.6 - in_port(s, h)) * 1000.0).round() as u32;
                set_pool(&mut c, h, pool);
                idle_by(&mut c, &mut fake, due + 1, 1);
                assert!(
                    c.ships.havens[h as usize].broken_at.is_none(),
                    "seed {seed}: broken before a year of robbery"
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
        assert!(early > 0, "seed {seed}: no look within the year");
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

        // The sea ship comes home and is laid up; the haven shuts, broken.
        let mut t = c.ships.now();
        while c.ships.ship(sea).unwrap().is_active() {
            t += TICKS_PER_DAY / 96;
            assert!(t < due + 40 * TICKS_PER_DAY, "seed {seed}: never home");
            idle_by(&mut c, &mut fake, t, TICKS_PER_DAY / 96);
        }
        let x = c.ships.ship(sea).unwrap();
        assert!(x.is_laid_up() && x.crew == 0, "seed {seed}: {x:?}");
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

/// S8b `a_navy_leaves_a_strong_haven_alone` (§2.4.6a rule 3, PC7, PC8). At every look of 240
/// days, once the haven has held a patrolled lane for a year, where the coast's ships hold still
/// until it, its defence is set just above the gate:
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
        for _ in 0..120 {
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
        assert_eq!((n.looked, n.assaults), (120, 0), "seed {seed}");
        c.ships.check(&c.world).expect("check");
    });
}

/// S8b `no_assault_in_view` (§2.4.6a rule 3: nothing happens in a player's view). Once the coast's
/// pirates have hunted a lane the navy patrols for a year, a player stands 100 blocks off the
/// haven's dock, and at each look for 40 days the haven has nobody ashore (weak whatever is in
/// port: 11 men at most against the gate's 12.73). No look breaks it. Once the player leaves, a
/// look with the pool empty breaks it.
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
                due < Tick::from_days(300),
                "seed {seed}: no target in 300 days"
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
            now + 400 * TICKS_PER_DAY,
        );
        let looked = c.ships.navy_counts().looked;
        for _ in 0..20 {
            let due = to_look(&mut c, &mut fake, nv);
            set_pool(&mut c, h, 0);
            idle_by(&mut c, &mut fake, due + 1, 1);
            assert!(
                c.ships.havens[h as usize].broken_at.is_none(),
                "seed {seed}: broken in view"
            );
        }
        let n = c.ships.navy_counts();
        assert_eq!((n.looked, n.assaults), (looked + 20, 0), "seed {seed}");
        // The player leaves: a look breaks the haven.
        c.ships.player_left(&mut c.world, player);
        let mut looks = 0;
        while c.ships.havens[h as usize].broken_at.is_none() {
            looks += 1;
            assert!(looks <= 20, "seed {seed}: not broken out of view");
            let due = to_look(&mut c, &mut fake, nv);
            set_pool(&mut c, h, 0);
            idle_by(&mut c, &mut fake, due + 1, 1);
        }
        assert_eq!(c.ships.navy_counts().assaults, 1);
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
/// and its one site is never open twice.
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
        let open = usize::from(hv.shut.is_none());
        assert!(open <= 1, "seed {seed}: more open than sites");
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

/// S8b `coast_havens_cycle_with_assaults` (§2.4.6a rule 3, PC31): 30 years of the coast with
/// nobody watching and Saltmere's navy assaulting, a year of robbery first and with no such wait
/// (`assault_cause_days` 96 and 0), on seeds 1, 7 and 30. It reports the share of slots manned,
/// breaks per decade, the mean days from opening to a break and the mean days shut; it asserts
/// only what holds by construction: a hunt in every 5-year window, no break sooner than the
/// cause after the haven opened, and never more open havens than sites.
#[test]
fn coast_havens_cycle_with_assaults() {
    let jobs: Vec<u64> = [1u64, 7, 30]
        .iter()
        .flat_map(|&s| [2 * s, 2 * s + 1])
        .collect();
    let runs = per_seed(&jobs, |j| {
        coast_cycle(j / 2, if j % 2 == 0 { 96 } else { 0 }, 30)
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
    }
}

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
    while t < end {
        t += 24 * TICKS_PER_DAY;
        synth::drive(
            &mut world,
            &mut ships,
            &mut fake,
            &mut NoInfrastructure,
            &mask,
            t,
            TICKS_PER_DAY / 4,
        );
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
             fewest hunts in a window {}, longest shut {:.0} days",
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
         refounds {:.4}, laid up {:.4}, hunts {:.2}, toll {:.3} men; longest shut spell {:.0} days",
        on.len(),
        on.first().map_or(0, |l| l.years),
        on.iter().map(|l| l.min_open).min().unwrap_or(0),
        shuts / years,
        refounds / years,
        laid_up / years,
        hunts_on / years,
        sum(on, &|l| l.toll_men) / years,
        on.iter().map(|l| l.longest_shut).fold(0.0, f64::max),
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

/// The lifecycle's bars on many worlds (EQUILIBRIUM.md S8b): no world ever has every site shut
/// at once; across the worlds, havens do shut and refound (they neither all die nor all hold
/// every site for good); every world hunts in every window; and open havens never exceed the
/// sites found. With the lifecycle off nothing shuts and no toll is paid (S8's havens).
fn check_lives(on: &[Life], off: &[Life]) {
    for l in on {
        assert!(
            l.min_open >= 1,
            "seed {}: every site shut at once: {l:?}",
            l.seed
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
    /// The lifecycle on, as built (a year of robbery before an assault, PC31).
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
/// no year of robbery before an assault (reported, not judged).
#[test]
fn unattended_havens_rise_and_fall() {
    report_and_check(&lives(6, &[1, 2, 3], 3, 10));
}

/// S8b `unattended_havens_rise_and_fall` in full: ten centuries (96,000 game days) of seeds 1 to
/// 30, the lifecycle on and off, and on with no year of robbery before an assault. Release only,
/// for its length (`cargo test --release -p rtsim-ships --test lifecycle -- --ignored`).
#[test]
#[ignore = "long: run in release with --ignored"]
fn unattended_havens_rise_and_fall_1000_years() {
    report_and_check(&lives(12, &seeds(), 10, 100));
}
