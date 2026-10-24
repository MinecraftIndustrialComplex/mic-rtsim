// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the modules that live in the phase 1 world share: the dragons (`dragons.rs`), the ships
//! (`ships.rs`), rtsim-ecology's herds (`herds.rs`) and the conflict (`conflict.rs`). Two
//! services, each in one place:
//!
//! - **One interest area per player** (overnight choices U2 to U6). PLAYER records go in once per
//!   hand-over ([`observe_state`], latest per player); at the simulate point each player's area
//!   is kept or booked anew in the phase 1 world's leg index (`World::observe_area`), and a new
//!   area is routed to the modules that run (`World::route_area`): each books what the area needs
//!   (the ships, the voyages it could see), the legs sharing its cells are looked up once, and
//!   each crossing goes to the module that owns the traveller. A player not reported for
//!   [`PLAYER_STALE`] game ticks leaves; a session start clears every area (puppets and players'
//!   presence never outlive a session). Before this, the world, the dragons and the ships each
//!   booked their own leg for every player (R31, T17).
//! - **One clock** (U7). The dragons', the ships', the herds' and the conflict's events run
//!   interleaved in time order ([`run_to`]), so none ever meets a world that another already ran
//!   past its time. A phase 1 input or a module's record runs them to its time first
//!   ([`catch_up`]).
//!
//! **Herds** joined (Y1) by `rtsim_ecology::herds::Herds` implementing
//! `rtsim_world::interest::Viewer` (legs under owner bit 60), `herds::running` and
//! `herds::run_engine`, and one line each in [`with_viewers`] and [`run_to`]. **The conflict**
//! joined the same way (C2-5: `rtsim_conflict::Conflict` is a `Viewer` with legs under owner bit
//! 59, so war bands and envoys that enter a player's view materialize). A module that starts
//! running in the middle of a session hears of the areas already booked through
//! [`introduce_herds`] and [`introduce_conflict`].
//!
//! **Seams** (TP4, TP8): players' positions are canonicalized here, and the area the world books
//! for them covers canonical regions across every seam (the leg index folds it), so a module's
//! traveller on the far side of an edge crosses into the area like any other.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_world::interest::Viewer;

use super::{State, conflict, dragons, herds, log_info, phase1, ships};
use crate::wire::{self, Rec, obs};

/// A player not reported for this many game ticks has left (phase 1's own rule, V8's
/// PLACEHOLDER). PLAYER comes every 20 ticks per player.
pub const PLAYER_STALE: i64 = 200;

type Uuid = (u64, u64);

/// The shared services' state the step keeps (`State::shared`). Per session: nothing here is
/// saved (the areas themselves are the world's, saved with it).
#[derive(Default)]
pub(super) struct Link {
    /// This hand-over's PLAYER records, the latest per player: (frame, x, z).
    /// (frame, x, z, y): the height for the dragons' field flights (OILGATE.md OG-3).
    pending: BTreeMap<Uuid, (u16, i32, i32, i32)>,
    /// Players with an area this session: their actor and the game time last reported.
    present: BTreeMap<Uuid, (ActorId, i64)>,
    /// Areas booked or replanned and routed, this session.
    pub(super) routed: u64,
}

/// Whether any module that sees players runs (and so needs their areas).
fn any_viewer(st: &State) -> bool {
    dragons::running(st) || ships::running(st) || herds::running(st) || conflict::running(st)
}

/// The radius of the shared area: the widest view of the modules (each is 160 blocks today).
fn radius(st: &State) -> u32 {
    st.phase1
        .world
        .cfg
        .view_blocks
        .max(st.dragons.engine.tun.view_radius)
        .max(st.ships.engine.tun.view_radius)
        .max(st.herds.engine.tun.view_radius)
        .max(st.conflict.engine.tun.view_radius)
}

/// Runs `f` with the phase 1 world and the modules an area is routed to: those that run, in a
/// fixed order (the dragons, the ships, the herds, the conflict).
fn with_viewers<R>(
    st: &mut State,
    f: impl FnOnce(&mut rtsim_world::World, &mut [&mut dyn Viewer]) -> R,
) -> R {
    let (d_on, s_on, h_on) = (dragons::running(st), ships::running(st), herds::running(st));
    let c_on = conflict::running(st);
    let w = &mut st.phase1.world;
    let mut v: Vec<&mut dyn Viewer> = Vec::with_capacity(4);
    if d_on {
        v.push(&mut st.dragons.engine);
    }
    if s_on {
        v.push(&mut st.ships.engine);
    }
    if h_on {
        v.push(&mut st.herds.engine);
    }
    if c_on {
        v.push(&mut st.conflict.engine);
    }
    let r = f(w, &mut v);
    if h_on {
        // A crossing or a player leaving can make the herds decide (a despawn).
        herds::take_commands(st);
    }
    if c_on {
        // A player leaving recalls the bands it watched.
        conflict::take_commands(st);
    }
    r
}

/// The herds started running in the middle of a session (a first herd founded, the switch
/// turned on): every area booked meanwhile is routed to them alone, as if it had just been
/// booked, so a player who stands still sees a herd that is near.
pub(super) fn introduce_herds(st: &mut State) {
    if !herds::running(st) {
        return;
    }
    let areas: Vec<(ActorId, rtsim_travel::legs::Stamp)> = st
        .shared
        .present
        .values()
        .filter_map(|(p, _)| st.phase1.world.interest_area(*p).map(|a| (*p, a)))
        .collect();
    for (p, a) in areas {
        let (w, h) = (&mut st.phase1.world, &mut st.herds.engine);
        let mut v: [&mut dyn Viewer; 1] = [h];
        w.route_area(p, a, &mut v);
        st.phase1.taint();
    }
    herds::take_commands(st);
}

/// The conflict started running in the middle of a session (a first camp reported, the switch
/// turned on): every area booked meanwhile is routed to it alone, as [`introduce_herds`] does, so
/// a player standing still sees an envoy or a band that is near.
pub(super) fn introduce_conflict(st: &mut State) {
    if !conflict::running(st) {
        return;
    }
    let areas: Vec<(ActorId, rtsim_travel::legs::Stamp)> = st
        .shared
        .present
        .values()
        .filter_map(|(p, _)| st.phase1.world.interest_area(*p).map(|a| (*p, a)))
        .collect();
    for (p, a) in areas {
        let (w, c) = (&mut st.phase1.world, &mut st.conflict.engine);
        let mut v: [&mut dyn Viewer; 1] = [c];
        w.route_area(p, a, &mut v);
        st.phase1.taint();
    }
    conflict::take_commands(st);
}

// ---- the interest areas ---------------------------------------------------------------------------

/// SESSION_START: every player's area goes (the world forgets where players were), and every
/// module hears it.
pub(super) fn session_start(st: &mut State) {
    st.shared.pending.clear();
    st.shared.present.clear();
    let with_area: Vec<ActorId> = st
        .phase1
        .world
        .players
        .iter()
        .filter(|(p, _)| st.phase1.world.interest_area(**p).is_some())
        .map(|(p, _)| *p)
        .collect();
    if with_area.is_empty() {
        return;
    }
    for p in with_area {
        leave(st, p);
    }
    st.phase1.taint();
}

fn leave(st: &mut State, p: ActorId) {
    with_viewers(st, |w, v| w.leave_area(p, v));
}

/// A PLAYER record: kept until the simulate point, the latest per player.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != obs::PLAYER || rec.size() < obs::player::SIZE {
        return;
    }
    let b = rec.bytes;
    let uuid = (
        wire::u64_at(b, obs::player::UUID_MOST),
        wire::u64_at(b, obs::player::UUID_LEAST),
    );
    let frame = rec.aux as u16;
    // Canonical at the door (TP4): a player just past an edge (the frame before Toroidal World
    // or the wrap engine folds it) is where it is on the world, not a jump.
    let (x, z) = st.topology.shared.of(frame).canonical_i32(
        wire::f64_at(b, obs::player::X).floor() as i32,
        wire::f64_at(b, obs::player::Z).floor() as i32,
    );
    let y = wire::f64_at(b, obs::player::Y).floor() as i32;
    st.shared.pending.insert(uuid, (frame, x, z, y));
}

/// The simulate point, after the modules ran to the hand-over's time: each reported player's
/// area kept or booked anew and routed; players gone stale leave.
pub(super) fn simulate(st: &mut State) {
    let pending = std::mem::take(&mut st.shared.pending);
    let gt = st.game_time;
    if !any_viewer(st) {
        // Nobody needs areas: a phase 1 world alone keeps its cheap saves (S13). Areas booked
        // while a module ran expire with their window.
        st.shared.present.clear();
        return;
    }
    let at = Tick(phase1::world_tick(st, gt));
    let r = radius(st);
    for (uuid, (frame, x, z, y)) in pending {
        let pos = Pos::new(frame, x, z);
        if dragons::running(st) {
            // The dragons' home counter (P26) learns from every report, area or not, and the oil
            // gate where players are (field flights, lairs near oil).
            let (w, d) = (&mut st.phase1.world, &mut st.dragons.engine);
            d.player_at(w, uuid, pos, y, at.max(d.now()));
        }
        if conflict::running(st) {
            // The conflict's time-spent counter per chunk (P14, changed): an unowned source
            // counts against the player who spent the most time in its chunks.
            let (w, c) = (&mut st.phase1.world, &mut st.conflict.engine);
            c.observe_presence(w, uuid, pos, at.max(c.now()));
        }
        let (p, area) = st.phase1.world.observe_area(uuid, pos, at, r);
        st.shared.present.insert(uuid, (p, gt));
        st.phase1.taint();
        if let Some(a) = area {
            st.shared.routed += 1;
            with_viewers(st, |w, v| w.route_area(p, a, v));
        }
    }
    let stale: Vec<(Uuid, ActorId)> = st
        .shared
        .present
        .iter()
        .filter(|(_, (_, t))| gt - *t > PLAYER_STALE)
        .map(|(u, (p, _))| (*u, *p))
        .collect();
    for (u, p) in stale {
        st.shared.present.remove(&u);
        leave(st, p);
        st.phase1.taint();
    }
}

// ---- the clock --------------------------------------------------------------------------------------

/// Runs the dragons, the ships, the herds and the conflict to world tick `at`, their events
/// interleaved in time order: each round runs all four to the earliest next event any has (a
/// lower bound from their wheels), so an event of one never meets a world another ran past it. Each round pops
/// everything due up to its time and no module schedules another's events, so the rounds' times
/// rise and the loop ends. (With the dragons running, the herds run inside the dragons' rounds,
/// as their prey, Y7; the herds' own round then finds nothing left.)
pub(super) fn run_to(st: &mut State, at: Tick) {
    loop {
        let mut t = at;
        if dragons::running(st)
            && let Some(n) = st.dragons.engine.next_due()
        {
            t = t.min(n);
        }
        if ships::running(st)
            && let Some(n) = st.ships.engine.next_due()
        {
            t = t.min(n);
        }
        if herds::running(st)
            && let Some(n) = st.herds.engine.next_due()
        {
            t = t.min(n);
        }
        if conflict::running(st)
            && let Some(n) = st.conflict.engine.next_due()
        {
            t = t.min(n);
        }
        dragons::run_engine(st, t);
        ships::run_engine(st, t);
        herds::run_engine(st, t);
        conflict::run_engine(st, t);
        if t >= at {
            return;
        }
    }
}

/// Before a phase 1 input or a module's record at its game time: the modules' events up to it
/// happen first (S13), so the input meets the world they left.
pub(super) fn catch_up(st: &mut State, rec: &Rec<'_>) {
    if rec.size() >= obs::BODY {
        let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
        let at = Tick(phase1::world_tick(st, gt));
        run_to(st, at);
    }
}

// ---- the replay end ---------------------------------------------------------------------------------

/// The step's top, after the observations are read (docs/replay-gate.md §2.2, trigger 2): a
/// replay whose last record was the hand-over's last ends here.
pub(super) fn replay_end_if_due(st: &mut State) {
    if st.replaying && st.replay.applied_through().0 + 1 >= st.session_first {
        replay_end(st);
    }
}

/// The SESSION_START arm's end (§2.1, trigger 3): with nothing to replay the replay ends at once.
/// Java's journal counts too (RP-29): one that starts at or past `session_first`
/// (`journal_first`, 0 read as `world_next` by the caller) sends nothing below it.
pub(super) fn replay_end_at_session_start(st: &mut State, journal_first: u64) {
    if st.replaying && (st.replay.applied_through().0 + 1).max(journal_first) >= st.session_first {
        replay_end(st);
    }
}

/// The top of `journaled()` (§2.2, trigger 1): the first record at or past `session_first` ends
/// the replay before it is admitted, whatever its kind.
pub(super) fn replay_end_before(st: &mut State, seq: u64) {
    if st.replaying && seq >= st.session_first {
        replay_end(st);
    }
}

/// The replay is over (§3.2), once a session: logged with its counts, then each module's own end
/// in the step's order. Each module's end runs at the replay clock; the held part of the
/// hand-over's time runs at the next run of the clock.
fn replay_end(st: &mut State) {
    st.replaying = false;
    st.replays_ended += 1;
    log_info(&format!(
        "the session's replay is over: {} records replayed over {} hand-overs",
        st.replay_records, st.replay_handovers
    ));
    dragons::replay_end(st);
    ships::replay_end(st);
    herds::replay_end(st);
    conflict::replay_end(st);
}
