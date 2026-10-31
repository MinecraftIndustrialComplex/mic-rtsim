// SPDX-License-Identifier: AGPL-3.0-or-later
//! S8's pirate harness (EQUILIBRIUM.md §2.4.6, §4), lifted out of `tests/equilibrium.rs` so that
//! the kill-all table (`rtsim-cli sim killall`, slice S11 EQC427) can call it: the synthetic coast
//! run with nobody watching, the scripted player, a kill-all of a haven's fleet, and the days until
//! its slots are manned again. `tests/equilibrium.rs` runs its tests on the same code.
//!
//! **Scaffolding**, as the rest of [`crate::synth`].

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

use super::{Coastal, FakeShips, Script};
use crate::boundary::{self, By, Obs};
use crate::engine::{Ship, ShipId, Status};
use crate::tunables::ShipTunables;
use crate::{Ships, synth};

/// The player whose kills the kill-all books.
pub const PLAYER: (u64, u64) = (0xAB, 0xCD);

/// `t` in days.
pub fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// Haven `h`'s ships: (active, all).
pub fn manned(s: &Ships, h: u32) -> (usize, usize) {
    let slots = &s.havens[h as usize].ships;
    let up = slots
        .iter()
        .filter(|&&p| s.ship(p).is_some_and(Ship::is_active))
        .count();
    (up, slots.len())
}

/// Runs the coast with nobody watching to `to`, a quarter day at a time through the fake plugin.
pub fn idle(c: &mut Coastal, fake: &mut FakeShips, to: Tick) {
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
pub fn scripted(seed: u64, days: u64, tun: &ShipTunables) -> (Coastal, FakeShips) {
    let mut c = synth::coastal(seed, tun.clone());
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

/// A player sinks every active ship of the coast's haven at `at`, wherever it is ([`PLAYER`] sinks
/// them one by one: each adds 1/slots to the haven's pressure). How many it sank.
pub fn sink_all(c: &mut Coastal, op: &mut u64, at: Tick) -> usize {
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

/// Days from `from` until half and all of the coast's haven's slots are manned, looked at every
/// quarter day up to `limit` days (`None`: not by then).
pub fn days_back(
    c: &mut Coastal,
    fake: &mut FakeShips,
    from: Tick,
    limit: u64,
) -> (Option<f64>, Option<f64>) {
    let (mut half, mut all) = (None, None);
    let mut t = from;
    while t < from + limit * TICKS_PER_DAY && all.is_none() {
        t += TICKS_PER_DAY / 4;
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

/// A refitted slot's wait as it was set when the ship went down (`back - since`), days.
pub fn refit_wait(s: &Ships, p: ShipId) -> Option<f64> {
    match s.ship(p)?.status {
        Status::Lost { since, back } => Some(day(back) - day(since)),
        Status::Active => None,
    }
}

/// What S8's `kill_all_pirates` measures on one seed and one variant (EQC427).
#[derive(Clone, Debug, PartialEq)]
pub struct PirateRefill {
    pub seed: u64,
    /// Ships the player sank, the haven's slots, and the ships still active right after.
    pub sunk: usize,
    pub slots: usize,
    pub active_after: usize,
    /// Days from the kill-all until half and then every slot was manned (looked at up to 180).
    pub half: Option<f64>,
    pub whole: Option<f64>,
    /// The haven's shuts by then (a kill-all is answered by refits, PC4).
    pub shuts: u64,
    /// The refit waits set by the kill-all; for a plain kill-all also those set by a second one
    /// as soon as the fleet was back (empty for the harsh one), and the refit cap they keep under.
    pub first_waits: Vec<f64>,
    pub again_waits: Vec<f64>,
    pub cap_days: f64,
    /// The engine's indexes after the run.
    pub indexes: Result<(), String>,
}

/// S8's kill-all on seed `seed` with `tun`: twelve days of the scripted player and the fleet whole
/// again, then [`PLAYER`] sinks every ship of the haven and leaves. `harsh` is the model's
/// kill-all: the crews ashore die too and the haven's pressure takes 1 more.
pub fn kill_all(seed: u64, tun: &ShipTunables, harsh: bool) -> PirateRefill {
    let (mut c, mut fake) = scripted(seed, 12, tun);
    let h = c.haven;
    let at = c.ships.now();
    let mut op = 0;
    let sunk = sink_all(&mut c, &mut op, at);
    let (active_after, slots) = manned(&c.ships, h);
    if harsh {
        let d = (at.0 / TICKS_PER_DAY) as u32;
        let hv = &mut c.ships.havens[h as usize];
        hv.pool_milli = 0;
        hv.pool_at = at;
        hv.pressure.bump(d, 1.0, tun.refit.half_life_days);
    }
    let waits = |c: &Coastal| -> Vec<f64> {
        c.ships.havens[h as usize]
            .ships
            .clone()
            .into_iter()
            .filter_map(|p| refit_wait(&c.ships, p))
            .collect()
    };
    let first_waits = waits(&c);
    let (half, whole) = days_back(&mut c, &mut fake, at, 180);
    let shuts = c.ships.havens[h as usize].counts.shuts;
    let mut again_waits = Vec::new();
    if !harsh && whole.is_some() {
        // Again, as soon as they are back: the wait is longer.
        let at = c.ships.now();
        sink_all(&mut c, &mut op, at);
        again_waits = waits(&c);
    }
    PirateRefill {
        seed,
        sunk,
        slots,
        active_after,
        half,
        whole,
        shuts,
        first_waits,
        again_waits,
        cap_days: f64::from(tun.refit.cap_days),
        indexes: c.ships.check_indexes(),
    }
}

/// The table's row (EQC426): the plain kill-all at the defaults.
pub fn kill_all_pirates(seed: u64) -> PirateRefill {
    kill_all(seed, &ShipTunables::default(), false)
}
