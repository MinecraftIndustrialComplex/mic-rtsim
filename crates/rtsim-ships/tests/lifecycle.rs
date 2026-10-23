// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8b (EQUILIBRIUM.md §2.4.6a, rules 1, 2, 4 and 5; docs/choices-ports.md PC1 to PC6): havens
//! that rise and fall. An unwatched hunt pays the square law's toll in crew, a haven with no hunt
//! for 30 days starves and lays its ships up, a starving haven with no ship shuts, and its site
//! takes a new band out of every player's view once a fresh band would find prey.
//!
//! The rules are judged on the synthetic coast (`synth::coastal`: one haven, a sea and a sky
//! pirate) and, for the lifecycle over many worlds, on `synth::sea_havens` (3 to 5 havens, 1 or 2
//! navies, 12 to 20 ports, laid out by the seed).

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
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
        for (i, &p) in slots.iter().enumerate() {
            if !matches!(
                c.ships
                    .ship(p)
                    .and_then(|x| x.pirate.as_ref())
                    .map(|x| x.state),
                Some(PState::Hunting { .. })
            ) {
                continue;
            }
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
        assert!(hv.fed_until >= t, "a hunt start feeds the haven");
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
            let stays = s.havens[h as usize].ships.iter().all(|&p| {
                next_change(s, p).is_none_or(|t| t > due)
            });
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
        assert!(c.ships.ship(sea).unwrap().is_active(), "the ship at sea sails on");
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
/// pirates have hunted a lane the navy patrols for a year, a player stands 100 blocks off the haven's dock, and at
/// each look for 40 days the haven has nobody ashore (weak whatever is in port: 11 men at most
/// against the gate's 12.73). No look breaks it. Once the player leaves, a look with the pool
/// empty breaks it.
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
            assert!(due < Tick::from_days(300), "seed {seed}: no target in 300 days");
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
    /// Hunts in each window (a century, or a decade in the everyday cut).
    window_hunts: Vec<u64>,
    shuts: u64,
    refounds: u64,
    laid_up: u64,
    hunts: u64,
    toll_men: f64,
    sinkings: u64,
}

/// `windows` windows of `years` each of [`synth::sea_havens`] with nobody watching, censused every
/// 24 days and checked at each window's end.
fn unattended(seed: u64, lifecycle: bool, windows: u64, years: u64) -> Life {
    let tun = ShipTunables {
        lifecycle,
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
        assert!(open <= life.sites && up <= slots, "seed {seed}");
        life.open += open as f64 / life.sites as f64;
        life.manned += up as f64 / slots as f64;
        life.min_open = life.min_open.min(open);
        samples += 1;
        if t.0.is_multiple_of(years * YEAR * TICKS_PER_DAY) {
            life.window_hunts.push(ships.stats.hunts - hunts0);
            hunts0 = ships.stats.hunts;
            ships.check(&world).unwrap_or_else(|e| {
                panic!("seed {seed} lifecycle {lifecycle} day {}: {e}", day(t))
            });
        }
    }
    life.open /= samples as f64;
    life.manned /= samples as f64;
    let counts = ships.haven_counts();
    life.shuts = counts.shuts;
    life.refounds = counts.refounds;
    life.laid_up = counts.laid_up;
    life.toll_men = counts.toll_milli as f64 / 1000.0;
    life.hunts = ships.stats.hunts;
    life.sinkings = ships.stats.sinkings;
    life
}

fn report(runs: &[Life]) {
    for l in runs {
        eprintln!(
            "seed {:>2} lifecycle {:<5} sites {} slots {} navies {}: open {:.3} (min {}), manned \
             {:.3}, shuts {}, refounds {}, laid up {}, sinkings {}, hunts/yr {:.1}, toll {:.1} \
             men ({:.2}/yr), fewest hunts in a window {}",
            l.seed,
            l.lifecycle,
            l.sites,
            l.slots,
            l.navies,
            l.open,
            l.min_open,
            l.manned,
            l.shuts,
            l.refounds,
            l.laid_up,
            l.sinkings,
            l.hunts as f64 / l.years as f64,
            l.toll_men,
            l.toll_men / l.years as f64,
            l.window_hunts.iter().min().copied().unwrap_or(0),
        );
    }
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
        assert_eq!(l.toll_men, 0.0);
        assert!(l.window_hunts.iter().all(|&n| n > 0), "{l:?}");
    }
}

/// Every seed's unattended run with the lifecycle on, then off, `threads` runs at a time.
fn lives(threads: usize, seeds: &[u64], windows: u64, years: u64) -> (Vec<Life>, Vec<Life>) {
    let jobs: Vec<u64> = seeds.iter().flat_map(|&s| [2 * s, 2 * s + 1]).collect();
    let all = per_seed_on(threads, &jobs, |j| {
        unattended(j / 2, j % 2 == 0, windows, years)
    });
    all.into_iter().partition(|l| l.lifecycle)
}

/// S8b `unattended_havens_rise_and_fall`, the everyday cut: three decades of three seas, the
/// lifecycle on and off (seed 2's sea has havens that shut and refound within them).
#[test]
fn unattended_havens_rise_and_fall() {
    let (on, off) = lives(6, &[1, 2, 3], 3, 10);
    report(&on);
    report(&off);
    check_lives(&on, &off);
}

/// S8b `unattended_havens_rise_and_fall` in full: ten centuries (96,000 game days) of seeds 1 to
/// 30, the lifecycle on and off. Release only, for its length (`cargo test --release -p
/// rtsim-ships --test lifecycle -- --ignored`).
#[test]
#[ignore = "long: run in release with --ignored"]
fn unattended_havens_rise_and_fall_1000_years() {
    let (on, off) = lives(12, &seeds(), 10, 100);
    report(&on);
    report(&off);
    check_lives(&on, &off);
}
