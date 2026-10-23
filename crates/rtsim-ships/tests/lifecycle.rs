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
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(1, 6));
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
            ships.check(&world).expect("check");
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

/// S8b `unattended_havens_rise_and_fall`, the everyday cut: three decades of four seas, the
/// lifecycle on and off.
#[test]
fn unattended_havens_rise_and_fall() {
    let seeds = [1, 2, 3, 4];
    let on = per_seed(&seeds, |seed| unattended(seed, true, 3, 10));
    let off = per_seed(&seeds, |seed| unattended(seed, false, 3, 10));
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
    let on = per_seed(&seeds(), |seed| unattended(seed, true, 10, 100));
    let off = per_seed(&seeds(), |seed| unattended(seed, false, 10, 100));
    report(&on);
    report(&off);
    check_lives(&on, &off);
}
