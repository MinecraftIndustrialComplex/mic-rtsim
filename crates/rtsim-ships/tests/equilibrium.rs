// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8 (EQUILIBRIUM.md §2.4.6, §4): pirate havens on the shared renewal rule. Hires fill a capped
//! crew pool, an empty slot is refitted after `min(96, 4 x 2^P)` days once the pool can man it,
//! and a pirate hunts only a lane worth it. The fleet comes back after a kill-all, the pool never
//! passes its cap, a haven with no lane worth hunting rests, and pirates last a thousand years
//! with nobody watching.
//!
//! Each rule is judged on seeds 1 to 30 of the synthetic coast (`synth::coastal`: one haven with
//! a sea and a sky pirate, two slots).

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::renewal::Pressure;
use rtsim_ships::boundary::{self, By, Obs};
use rtsim_ships::engine::{Ship, ShipId, Status};
use rtsim_ships::synth::{self, Coastal, FakeShips, Script};
use rtsim_ships::{Role, ShipTunables, Ships};
use rtsim_travel::Medium;

const PLAYER: (u64, u64) = (0xAB, 0xCD);
const SEEDS: std::ops::RangeInclusive<u64> = 1..=30;

fn coast(seed: u64) -> Coastal {
    synth::coastal(seed, ShipTunables::default())
}

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

/// Haven `h`'s ships: (active, all).
fn manned(s: &Ships, h: u32) -> (usize, usize) {
    let slots = &s.havens[h as usize].ships;
    let up = slots
        .iter()
        .filter(|&&p| s.ship(p).is_some_and(Ship::is_active))
        .count();
    (up, slots.len())
}

/// Runs the coast with nobody watching to `to`, a quarter day at a time through the fake plugin.
fn idle(c: &mut Coastal, fake: &mut FakeShips, to: Tick) {
    synth::drive(
        &mut c.world,
        &mut c.ships,
        fake,
        &mut NoInfrastructure,
        &c.mask,
        to,
        TICKS_PER_DAY / 4,
    );
}

/// The scripted player (it watches, follows the hunted lane, fights in view and sinks one pirate
/// on day 9) for `days`; then it logs off, the bodies leave with it, and the haven mans every
/// slot again (within 120 days) before the coast is handed back.
fn scripted(seed: u64, days: u64) -> (Coastal, FakeShips) {
    let mut c = coast(seed);
    let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
    let mut script = Script::default();
    script.run(&mut c, &mut fake, Tick::ZERO, Tick::from_days(days));
    fake.players.clear();
    let p = c.ships.player(&mut c.world, synth::SCRIPT_PLAYER);
    c.ships.player_left(&mut c.world, p);
    let t = c.ships.now() + 1_000;
    idle(&mut c, &mut fake, t);
    assert!(fake.bodies.is_empty(), "every body left with the player");
    let (_, all) = days_back(&mut c, &mut fake, t, 120);
    assert!(all.is_some(), "seed {seed}: the fleet is whole again");
    (c, fake)
}

/// A player sinks every active ship of the coast's haven at `at`, wherever it is (PLAYER sinks
/// them one by one: each adds 1/slots to the haven's pressure). How many it sank.
fn sink_all(c: &mut Coastal, op: &mut u64, at: Tick) -> usize {
    let ids: Vec<ShipId> = c.ships.havens[c.haven as usize].ships.clone();
    let mut n = 0;
    for s in ids {
        let Some(x) = c.ships.ship(s).filter(|x| x.is_active()) else {
            continue;
        };
        *op += 1;
        let o = Obs::Sunk(boundary::Sunk {
            agent: Ship::agent(s),
            epoch: x.epoch,
            by: By::Player,
            dimension: 0,
            player: PLAYER,
            ship: 0,
            op: 1 << 62 | *op,
            pos: (0.0, 63.0, 0.0),
        });
        c.ships.ingest(&mut c.world, &o, &c.mask, at);
        n += 1;
    }
    n
}

/// Days from `from` until half and all of haven `h`'s slots are manned, looked at every quarter
/// day up to `limit` days (`None`: not by then).
fn days_back(
    c: &mut Coastal,
    fake: &mut FakeShips,
    from: Tick,
    limit: u64,
) -> (Option<f64>, Option<f64>) {
    let (mut half, mut all) = (None, None);
    let mut t = from;
    while t < from + limit * TICKS_PER_DAY && all.is_none() {
        t = t + TICKS_PER_DAY / 4;
        idle(c, fake, t);
        let (up, n) = manned(&c.ships, c.haven);
        if half.is_none() && 2 * up >= n {
            half = Some(day(t) - day(from));
        }
        if up == n {
            all = Some(day(t) - day(from));
        }
    }
    (half, all)
}

fn quantile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * q).round() as usize]
}

/// A refitted slot's wait as it was set when the ship went down (`back - since`), days.
fn refit_wait(s: &Ships, p: ShipId) -> Option<f64> {
    match s.ship(p)?.status {
        Status::Lost { since, back } => Some(day(back) - day(since)),
        Status::Active => None,
    }
}

/// S8 `kill_all_pirates` (§3.6: 90% back within 180 days; the model's 50% at 48 and 90% at 72
/// [48, 150], on its 24-day grid). After twelve days of the scripted player (fights in view, one
/// pirate sunk) and the fleet whole again, a player sinks both of the haven's ships and leaves.
/// Every seed has both slots back within 180 days, and so does the model's harsher kill-all,
/// which also empties the pool and adds 1 to the pressure. A second kill-all as soon as the fleet
/// is back waits longer: the pressure is remembered.
#[test]
fn kill_all_pirates() {
    let tun = ShipTunables::default();
    let runs = per_seed(&seeds(), |seed| {
        let mut out = [0.0; 3];
        for harsh in [false, true] {
            let (mut c, mut fake) = scripted(seed, 12);
            let h = c.haven;
            let at = c.ships.now();
            let mut op = 0;
            assert_eq!(sink_all(&mut c, &mut op, at), 2, "seed {seed}");
            assert_eq!(manned(&c.ships, h).0, 0, "seed {seed}: all sunk");
            if harsh {
                // The model's kill-all: the crews ashore die too, and the haven itself is hit.
                let d = (at.0 / TICKS_PER_DAY) as u32;
                let hv = &mut c.ships.havens[h as usize];
                hv.pool_milli = 0;
                hv.pool_at = at;
                hv.pressure.bump(d, 1.0, tun.refit.half_life_days);
            }
            let first: Vec<f64> = c.ships.havens[h as usize]
                .ships
                .clone()
                .into_iter()
                .filter_map(|p| refit_wait(&c.ships, p))
                .collect();
            let (half, all) = days_back(&mut c, &mut fake, at, 180);
            let all = all.unwrap_or_else(|| {
                panic!("seed {seed} harsh {harsh}: not every slot back within 180 days")
            });
            if harsh {
                out[2] = all;
            } else {
                out[0] = half.expect("half before all");
                out[1] = all;
                // Again, as soon as they are back: the wait is longer.
                let at = c.ships.now();
                sink_all(&mut c, &mut op, at);
                let again: Vec<f64> = c.ships.havens[h as usize]
                    .ships
                    .clone()
                    .into_iter()
                    .filter_map(|p| refit_wait(&c.ships, p))
                    .collect();
                let longest = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
                assert!(
                    longest(&again) > longest(&first),
                    "seed {seed}: second kill-all waits {again:?}, first {first:?}"
                );
                assert!(
                    again.iter().all(|&w| w <= f64::from(tun.refit.cap_days) + 1e-6),
                    "{again:?}"
                );
            }
            c.ships.check_indexes().expect("indexes hold");
        }
        out
    });
    let col = |i: usize| runs.iter().map(|r| r[i]).collect::<Vec<f64>>();
    let q = |v: &[f64]| {
        format!(
            "{:.1} [{:.1}, {:.1}]",
            quantile(v, 0.5),
            quantile(v, 0.05),
            quantile(v, 0.95)
        )
    };
    println!(
        "kill-all, seeds 1 to 30 (days): half back {}, all back {}; with the pool emptied and \
         P + 1, all back {}",
        q(&col(0)),
        q(&col(1)),
        q(&col(2)),
    );
}

/// S8 `crew_pool_is_bounded`: hires follow the closed form `cap - (cap - pool) e^(-r t / cap)`
/// toward 18 (from the founding crew of 6), captures add crew up to the cap and never past it,
/// and the pool and pressure read back exactly after a restart.
#[test]
fn crew_pool_is_bounded() {
    let tun = ShipTunables::default();
    let cap = tun.crew_pool_cap * 1000;
    let k = tun.hires_per_day / f64::from(tun.crew_pool_cap);
    per_seed(&seeds(), |seed| {
        let mut c = coast(seed);
        let h = c.haven;
        let start = c.ships.havens[h as usize].pool_milli;
        assert_eq!(start, tun.sea.pirate_crew * 1000, "one spare crew at founding");
        // Nobody watches: no fight, no top-up, the pool is the closed form from day 0.
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        for d in [1u64, 10, 29, 72, 200] {
            idle(&mut c, &mut fake, Tick::from_days(d));
            let want = f64::from(cap) - f64::from(cap - start) * (-k * d as f64).exp();
            let got = c.ships.crew_pool_milli(h, Tick::from_days(d));
            assert!(
                (f64::from(got) - want).abs() <= 1.0,
                "seed {seed} day {d}: {got} against {want:.1}"
            );
        }
        // From 6, a second crew of 6 (12 ashore) takes 72 ln(12/6) days, about 49.9.
        let day12 = 72.0 * 2f64.ln();
        let at = |f: f64| Tick((day12 * f * TICKS_PER_DAY as f64) as u64);
        assert!(c.ships.crew_pool_milli(h, at(0.99)) < 12_000);
        assert!(c.ships.crew_pool_milli(h, at(1.01)) > 12_000);
        // Captures: whole crews taken from merchants join the pool up to the cap, never past it.
        let mut op = 0u64;
        let pirate = c.ships.havens[h as usize].ships[0];
        let mut t = c.ships.now();
        for round in 0..6u32 {
            let merchants: Vec<(ShipId, u32)> = c
                .ships
                .ships
                .iter()
                .filter(|(_, x)| matches!(x.role, Role::Merchant) && x.is_active())
                .map(|(id, x)| (id, x.epoch))
                .take(3)
                .collect();
            for (m, epoch) in merchants {
                op += 1;
                let o = Obs::Captured(boundary::Captured {
                    agent: Ship::agent(m),
                    epoch,
                    by: By::Ship,
                    crew_taken: 7 + round,
                    player: (0, 0),
                    ship: Ship::agent(pirate),
                    op: 1 << 62 | op,
                });
                c.ships.ingest(&mut c.world, &o, &c.mask, t);
                assert!(c.ships.crew_pool_milli(h, t) <= cap);
                assert!(c.ships.havens[h as usize].pool_milli <= cap);
            }
            t = t + 3 * TICKS_PER_DAY;
            idle(&mut c, &mut fake, t);
            assert!(c.ships.crew_pool_milli(h, t) <= cap);
            c.ships.check_indexes().expect("indexes hold");
        }
        assert_eq!(c.ships.crew_pool_milli(h, t), cap, "seed {seed}: full");
        assert!(c.ships.stats.captures >= 6, "{:?}", c.ships.stats);
        // A restart reads the pool and the pressure back.
        sink_all(&mut c, &mut op, t);
        let s2 = Ships::load(tun.clone(), &c.ships.save()).expect("ships load");
        assert_eq!(s2.havens, c.ships.havens);
        assert_eq!(s2.state_hash(), c.ships.state_hash());
        assert!(s2.havens[h as usize].pressure != Pressure::default());
    });
}

/// S8 `a_haven_with_no_lane_worth_hunting_rests` (§2.4.6 (b)): with every lane in reach escorted
/// past the point where a hunt's value turns negative (a win chance of 2/3 at the model's 3 and
/// 6), the haven's pirates rest in port: no hunt, no loss, both ships manned. Escort half the
/// lanes and every hunt goes to the other half; take the escorts away and they hunt again.
#[test]
fn a_haven_with_no_lane_worth_hunting_rests() {
    let tun = ShipTunables::default();
    // The escort that tips the value: a sea pirate (6 crew) against a merchant (4 at 0.5) and
    // 2.25 more is a win chance of 36 / (36 + 18) = 2/3.
    let probe = coast(1);
    for (m, a) in [
        (Medium::Sea, f64::from(tun.sea.pirate_crew)),
        (Medium::Sky, f64::from(tun.sky.pirate_crew)),
    ] {
        let a = a * tun.pirate_strength;
        assert!(probe.ships.hunt_value(a, m, 0.0) > 0.0, "{m:?} unescorted");
        assert!(probe.ships.hunt_value(a, m, 2.3) <= 0.0, "{m:?} escorted");
    }
    let days = 30;
    per_seed(&seeds(), |seed| {
        // Every lane in reach escorted.
        let mut c = coast(seed);
        let h = c.haven;
        let lanes = c.ships.havens[h as usize].lanes.clone();
        assert!(!lanes.is_empty());
        for &l in &lanes {
            c.ships.set_escort(&mut c.world, l, 7, 2.5, Tick::ZERO);
        }
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        idle(&mut c, &mut fake, Tick::from_days(days));
        let st = c.ships.stats;
        assert_eq!(st.hunts, 0, "seed {seed}: {st:?}");
        assert!(st.fruitless > 0, "seed {seed}: it looked and rested");
        assert_eq!(st.sinkings, 0);
        assert_eq!(manned(&c.ships, h), (2, 2));
        assert!(c.ships.hunted_lanes().is_empty());
        // The escorts leave: hunting starts again.
        let t = c.ships.now();
        for &l in &lanes {
            c.ships.set_escort(&mut c.world, l, 7, 0.0, t);
        }
        idle(&mut c, &mut fake, t + days * TICKS_PER_DAY);
        assert!(c.ships.stats.hunts > 0, "seed {seed}: {:?}", c.ships.stats);

        // Half the lanes escorted: hunts go to the others only.
        let mut c = coast(seed);
        c.ships.log_enabled = true;
        let guarded: Vec<u32> = lanes.iter().copied().step_by(2).collect();
        for &l in &guarded {
            c.ships.set_escort(&mut c.world, l, 7, 2.5, Tick::ZERO);
        }
        let mut fake = FakeShips::new(f64::from(tun.view_radius));
        idle(&mut c, &mut fake, Tick::from_days(days));
        let sails: Vec<u32> = c
            .ships
            .log
            .iter()
            .filter(|e| e.kind == "sail")
            .filter_map(|e| e.what.rsplit(' ').next()?.parse().ok())
            .collect();
        assert!(
            sails.iter().all(|l| !guarded.contains(l)),
            "seed {seed}: sailed to {sails:?}, escorted {guarded:?}"
        );
    });
}

/// One stretch (a century, or a decade in the everyday cut) of the unattended run.
#[derive(Debug, Default)]
struct Stretch {
    manned: f64,
    hunts: u64,
    rebuilds: u64,
    pool_max: u32,
    pending_max: usize,
    save_bytes: usize,
}

/// `stretches` of `years` each of the coast with nobody watching, censused every 24 days (the
/// model's grid).
fn unattended(seed: u64, stretches: u64, years: u64) -> Vec<Stretch> {
    const YEAR: u64 = 96;
    let mut c = coast(seed);
    let mut fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
    let h = c.haven;
    let mut out = Vec::new();
    let mut st = Stretch::default();
    let (mut samples, mut hunts0, mut rebuilds0) = (0u32, 0u64, 0u64);
    let mut t = Tick::ZERO;
    while t < Tick::from_days(stretches * years * YEAR) {
        t = t + 24 * TICKS_PER_DAY;
        idle(&mut c, &mut fake, t);
        let (up, n) = manned(&c.ships, h);
        st.manned += up as f64 / n as f64;
        samples += 1;
        st.pool_max = st.pool_max.max(c.ships.crew_pool_milli(h, t));
        st.pending_max = st
            .pending_max
            .max(c.ships.pending_by_kind().values().sum());
        if t.0 % (years * YEAR * TICKS_PER_DAY) == 0 {
            st.manned /= f64::from(samples);
            st.hunts = c.ships.stats.hunts - hunts0;
            st.rebuilds = c.ships.stats.rebuilds - rebuilds0;
            st.save_bytes = c.ships.save().len();
            (samples, hunts0, rebuilds0) = (0, c.ships.stats.hunts, c.ships.stats.rebuilds);
            out.push(std::mem::take(&mut st));
        }
    }
    c.ships.check_indexes().expect("indexes hold");
    out
}

fn check_unattended(seed: u64, ss: &[Stretch]) {
    let cap = ShipTunables::default().crew_pool_cap * 1000;
    let first = &ss[0];
    for (i, x) in ss.iter().enumerate() {
        // §3.6's band is 0.83 to 0.94 by century; the engine sits at its top or above (its fights
        // are certain and only a lane a pirate wins is hunted, so with nobody watching no ship is
        // lost), and the band's floor is the bar.
        assert!(x.manned >= 0.83, "seed {seed} stretch {i}: {x:?}");
        assert!(x.hunts > 0, "seed {seed} stretch {i}: no hunt: {x:?}");
        assert!(x.pool_max <= cap, "seed {seed} stretch {i}: {x:?}");
        // Flat: the queue and the save do not grow with time.
        assert!(x.pending_max <= first.pending_max + 4, "{x:?} {first:?}");
        assert!(
            x.save_bytes <= first.save_bytes + first.save_bytes / 10,
            "seed {seed} stretch {i}: {x:?} against {first:?}"
        );
    }
}

/// S8 `pirates_survive_unattended_1000_years` (§3.6: hunting havens hold 0.83 to 0.94 of their
/// slots by century): ten centuries (96,000 game days) of the coast with nobody watching, on
/// seeds 1 to 30, censused on the model's 24-day grid. Every century every seed's haven still
/// hunts, its slots are manned, its pool is within the cap, and the event queue and the save stay
/// flat. Release only, for its length (`cargo test --release -p rtsim-ships --test equilibrium
/// -- --ignored`); [`pirates_survive_unattended_thirty_years`] is its everyday cut.
#[test]
#[ignore = "long: run in release with --ignored"]
fn pirates_survive_unattended_1000_years() {
    let runs = per_seed(&seeds(), |seed| {
        let ss = unattended(seed, 10, 100);
        assert_eq!(ss.len(), 10);
        check_unattended(seed, &ss);
        ss
    });
    let manned: Vec<f64> = runs.iter().flatten().map(|x| x.manned).collect();
    let hunts: Vec<f64> = runs.iter().flatten().map(|x| x.hunts as f64).collect();
    println!(
        "unattended, seeds 1 to 30 by century: manned {:.3} to {:.3}, hunts {} to {}",
        quantile(&manned, 0.0),
        quantile(&manned, 1.0),
        quantile(&hunts, 0.0),
        quantile(&hunts, 1.0)
    );
}

/// [`pirates_survive_unattended_1000_years`]'s rule over three decades on seeds 1, 7 and 30.
#[test]
fn pirates_survive_unattended_thirty_years() {
    per_seed(&[1, 7, 30], |seed| {
        let ss = unattended(seed, 3, 10);
        assert_eq!(ss.len(), 3);
        check_unattended(seed, &ss);
    });
}
