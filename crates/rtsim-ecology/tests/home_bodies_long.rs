// SPDX-License-Identifier: AGPL-3.0-or-later
//! Home bodies over years (EQUILIBRIUM.md S5, EQC141): the scenario's five lairs with the fake
//! plugin keeping home bodies, lairs coming in and going out as chunks load, and a scripted player
//! killing the dragons of a lair now and then. The engine's books must hold at every step
//! (`Dragons::check`), nothing may vanish unaccounted (no body written off), and the lairs must
//! be held about as often as the same run without bodies (S4's bands, which a body must not move).

use proptest::prelude::*;
use rtsim_ecology::boundary::{self, Obs};
use rtsim_ecology::dragon::agent_of;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth::{self, FakeIaf};
use rtsim_ecology::{DragonTunables, Dragons};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_world::World;

const PLAYER: (u64, u64) = (5, 6);
/// A game year, in days.
const YEAR: u64 = 96;
const STEP: u64 = TICKS_PER_DAY / 4;

struct Sim {
    w: World,
    d: Dragons,
    iaf: FakeIaf,
    keys: Vec<u64>,
}

impl Sim {
    fn new(seed: u64, bodies: bool) -> Sim {
        let (mut w, _) = synth::world(seed);
        let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
        let specs = synth::lairs();
        synth::populate(&mut w, &mut d, &specs, Tick::ZERO);
        let mut iaf = FakeIaf::new(PLAYER);
        iaf.home.on = bodies;
        for s in &specs {
            iaf.home.lairs.insert(s.key, (s.x, s.z));
        }
        Sim {
            w,
            d,
            iaf,
            keys: specs.iter().map(|s| s.key).collect(),
        }
    }

    /// The engine's commands to the fake plugin and its answers due by `at`, until quiet.
    fn pump(&mut self, at: Tick) {
        loop {
            let cmds = self.d.take_commands();
            let quiet = cmds.is_empty();
            self.iaf.apply(cmds, at);
            let due = self.iaf.due(at);
            if quiet && due.is_empty() {
                break;
            }
            for (t, o) in due {
                let t = t.max(self.d.now());
                self.d.ingest(&mut self.w, &mut NoInfrastructure, &o, t);
            }
        }
    }

    fn run_to(&mut self, to: Tick) {
        let mut t = self.d.now();
        while t < to {
            t = (t + STEP).min(to);
            self.d.run_until(&mut self.w, &mut NoInfrastructure, t);
            self.pump(t);
        }
    }

    /// The scripted player kills every dragon of lair number `i`: a home body by its death record
    /// (loaded or saved), anything else by the engine's own `slay`.
    fn kill_lair(&mut self, i: usize, at: Tick) {
        let player = self.d.player(&mut self.w, PLAYER);
        let l = self.d.lair_of_key(self.keys[i]).expect("the lair");
        for x in self.d.lairs[l as usize].occupants.clone() {
            let agent = agent_of(x);
            let body = self
                .iaf
                .home
                .bodies
                .iter()
                .find(|(_, b)| b.agent == agent)
                .map(|(&e, _)| e);
            match body {
                Some(e) => self.iaf.kill_home_body(e, PLAYER, at),
                None => self.d.slay(&mut self.w, x, Some(player), at),
            }
        }
        self.pump(at);
    }
}

#[derive(Default, Debug)]
struct Run {
    held: f64,
    censuses: usize,
    losses: u64,
}

/// `years` of game time: lair `i` is in for three days in every eighteen, lair `(day / 5) % 5` is
/// emptied by the player every fifth day, and a census reads the lairs held every four days.
fn run(seed: u64, bodies: bool, years: u64) -> (Run, Sim) {
    let mut s = Sim::new(seed, bodies);
    let mut r = Run::default();
    for day in 1..=years * YEAR {
        let at = Tick(day * TICKS_PER_DAY);
        s.run_to(at);
        for i in 0..s.keys.len() {
            let want_in = (day / 3 + i as u64).is_multiple_of(6);
            let key = s.keys[i];
            let is_in = s.iaf.home.in_epoch.contains_key(&key);
            if want_in && !is_in {
                s.iaf.set_lair_in(key, false, at);
            } else if !want_in && is_in {
                s.iaf.set_lair_out(key, at);
            }
        }
        s.pump(at);
        if day.is_multiple_of(5) && day > YEAR {
            s.kill_lair(((day / 5) % 5) as usize, at);
        }
        if bodies {
            s.d.check(&s.w, None)
                .unwrap_or_else(|e| panic!("day {day}: {e}"));
        }
        if day.is_multiple_of(4) && day > YEAR {
            r.held += s.d.lairs.iter().filter(|l| !l.occupants.is_empty()).count() as f64;
            r.censuses += 1;
        }
    }
    r.held /= (r.censuses * s.keys.len()).max(1) as f64;
    r.losses = s.d.stats.player_losses;
    (r, s)
}

/// With bodies on, a body written off as missing would be a loss nobody caused: none is. The
/// lairs are held about as often as without bodies, and the player's losses are of the same order.
#[test]
fn bodies_do_not_move_the_s4_bands() {
    let (off, _) = run(1, false, 3);
    let (on, s) = run(1, true, 3);
    println!("bodies off {off:?}, on {on:?}; {:?}", s.d.stats);
    assert!(s.d.stats.lair_censuses > 20, "the lairs came in: {:?}", s.d.stats);
    assert!(s.d.stats.home_spawns > 0, "owed bodies were spawned");
    assert!(s.d.stats.home_saved > 0, "bodies were saved when their lair went out");
    assert!(s.d.stats.home_adopts > 0, "saved bodies were adopted again");
    assert_eq!(s.d.stats.home_missing, 0, "no body vanished without cause");
    assert_eq!(s.d.stats.strays_adopted, 0, "no stray: every body is the engine's");
    assert!(
        (on.held - off.held).abs() <= 0.15,
        "lairs held {:.3} with bodies, {:.3} without",
        on.held,
        off.held
    );
    assert!(on.losses > 0 && off.losses > 0);
    let ratio = on.losses as f64 / off.losses as f64;
    assert!((0.5..=2.0).contains(&ratio), "losses {} against {}", on.losses, off.losses);
}

/// The same over 30 seeds and 20 years; the medians must stay in the bands.
#[test]
#[ignore]
fn bodies_do_not_move_the_s4_bands_30_seeds() {
    let mut diffs = Vec::new();
    for seed in 1..=30 {
        let (off, _) = run(seed, false, 20);
        let (on, s) = run(seed, true, 20);
        assert_eq!(s.d.stats.home_missing, 0, "seed {seed}");
        diffs.push(on.held - off.held);
    }
    diffs.sort_by(f64::total_cmp);
    let median = diffs[diffs.len() / 2];
    println!("median difference in lairs held {median:+.3}: {diffs:?}");
    assert!(median.abs() <= 0.1, "{median}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// H1 and H2 over random histories (EQC105): lairs coming in and out, bodies killed, vanished
    /// without a trace, led past the leash, dragons moving house, time passing. The books hold at
    /// every step; and once every lair has been counted again, no dragon has two bodies standing
    /// and no lair has more than two.
    #[test]
    fn no_lair_ever_holds_two_bodies_for_one_dragon(
        ops in prop::collection::vec((0u8..9, 0u8..5), 30..70),
        seed in 1u64..40,
    ) {
        let mut s = Sim::new(seed, true);
        let mut day = 0u64;
        for (op, arg) in ops {
            let key = s.keys[usize::from(arg)];
            let at = Tick(day * TICKS_PER_DAY + 7);
            match op {
                0 | 1 | 7 => {
                    day += if op == 7 { 5 } else { 1 };
                    s.run_to(Tick(day * TICKS_PER_DAY + 7));
                }
                2 => s.iaf.set_lair_in(key, arg % 2 == 0, at),
                3 => s.iaf.set_lair_out(key, at),
                4 => s.kill_lair(usize::from(arg), at),
                5 => {
                    // A body gone without a word: another mod, an admin, a corrupted chunk.
                    if let Some(e) = s.iaf.home.bodies.iter().find(|(_, b)| b.lair == key).map(|(&e, _)| e) {
                        s.iaf.home.bodies.remove(&e);
                    }
                }
                6 => {
                    let l = s.d.lair_of_key(key).expect("the lair");
                    let other = s.d.lair_of_key(s.keys[(usize::from(arg) + 1) % 5]).expect("the lair");
                    if let Some(&x) = s.d.lairs[l as usize].occupants.first() {
                        s.d.relocate(&mut s.w, x, other, at);
                    }
                }
                _ => {
                    // The leash: a loaded body goes past it and is a puppet.
                    let found = s.iaf.home.bodies.iter().find(|(_, b)| b.lair == key && b.loaded).map(|(&e, b)| (e, b.agent, b.epoch, b.pos));
                    if let Some((e, agent, epoch, pos)) = found {
                        s.iaf.home.bodies.remove(&e);
                        s.iaf.push(at, Obs::DragonLeftHome(boundary::DragonLeftHome { agent, epoch, dimension: 0, entity: e, pos }));
                    }
                }
            }
            s.pump(at);
            s.d.check(&s.w, None).map_err(|e| TestCaseError::fail(format!("after op {op}/{arg}: {e}")))?;
        }
        // Everything is counted again, twice.
        for round in 0..2 {
            let t = Tick((day + 1 + round) * TICKS_PER_DAY + 9);
            for key in s.keys.clone() {
                s.iaf.set_lair_out(key, t);
            }
            s.pump(t);
            s.run_to(t + 5);
            for key in s.keys.clone() {
                s.iaf.set_lair_in(key, false, t + 6);
            }
            s.pump(t + 6);
            s.run_to(t + 600);
            s.d.check(&s.w, None).map_err(TestCaseError::fail)?;
        }
        let mut agents = std::collections::BTreeSet::new();
        let mut per_lair: std::collections::BTreeMap<u64, usize> = Default::default();
        for b in s.iaf.home.bodies.values() {
            // One body per dragon (H1), at most two per lair (H2), once all are counted.
            prop_assert!(agents.insert(b.agent), "two bodies of agent {:x}", b.agent);
            *per_lair.entry(b.lair).or_default() += 1;
        }
        prop_assert!(per_lair.values().all(|&n| n <= 2), "{per_lair:?}");
    }
}
