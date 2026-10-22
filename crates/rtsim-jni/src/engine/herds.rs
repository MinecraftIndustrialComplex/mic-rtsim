// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herds behind the boundary (docs/boundary.md §8; choices Y1 to Y# in
//! docs/overnight-choices-herdsmc.md): `rtsim-ecology`'s herds fed by the vanilla herd adapter's
//! records (the core's `herd` package: sheep, cows, pigs, horses and goats, D4), their commands
//! written into the command buffers, their page in the store.
//!
//! The step calls this module at its observe, simulate, write and save points, beside the dragons
//! and the ships (`engine/dragons.rs`, `engine/ships.rs`), whom it mirrors:
//!
//! - **Observations** ([`observe_state`], [`journaled`]): HERD_GROUP_STATE is latest state; the
//!   journaled kinds 0x0700 to 0x07FF go through the kernel's `Replay` (HERD_KILLED and
//!   HERD_TAKEN by their op ids too, Java's) into `Herds::ingest`, at their own time with the
//!   other modules and the world run to it first (the shared clock, `engine/shared.rs`).
//! - **Simulate** ([`simulate`], [`run_engine`]): the herds run to the hand-over's time on the
//!   phase 1 world's clock (their legs are the world's leg index); players' areas are the
//!   engine's shared ones, the herds being a `rtsim_world::interest::Viewer` (owner bit 60).
//!   With the dragons running too, the dragons run with the herds as prey
//!   (`Dragons::run_until_with_herds`, Y7).
//! - **Commands** ([`write_queue`]): HERD_MATERIALIZE, STEER and DESPAWN, with sequences from the
//!   core's counter after every other module's: all per session (puppets never outlive one, as
//!   the dragons' S11 and the ships' U4), so nothing is kept or resent. None carries an op id.
//!   Species and the dimension go through the engine's dictionary.
//! - **Pages** ([`Link::load`], [`save`]): the herds page through the engine's store
//!   (`engine/paging.rs`, PF12): their meta page at table 0x4800 ('H') id 0, their row and index
//!   tables at 0x4810 to 0x4815, a save writing the pages changed since the last one; in TEST
//!   mode their state hash beside the meta page, checked on load. The herds book legs in the
//!   phase 1 world, so while they run the phase 1 link snapshots at every save (S4), in the same
//!   commit; pages without the world they were saved with are dropped loudly.
//! - **The switch** (C25, U10): `Module.ECOLOGY` (bit 4), as the dragons. Off, the herds ignore
//!   their records, run nothing, write nothing and keep the page they had; on again, the bodies
//!   of before are gone (their session reset) and they run on from where they stopped.
//!
//! **Where herds come from** (X2-11): the world model (`engine/worldmodel.rs`) founds one herd
//! per suitable region when the core's habitat sampler has reported enough of it ([`found`]), and
//! the herds' climate is the world model's (`HabitatClimate`: real biomes, water and unknown
//! ground closed). A frame the world model knows nothing of still answers as the uniform
//! grassland of Y3 (X2-6), where a test's HERD_FOUND order ([`order_found`], Y4) founds herds.

use std::collections::VecDeque;
use std::sync::Arc;

use rtsim_ecology::herds::HerdsTicket;
use rtsim_ecology::herds::boundary::{self, Cmd, HerdDict, Obs, SpawnRejected};
use rtsim_ecology::herds::habitat::HabitatClimate;
use rtsim_ecology::herds::herd::herd_of;
use rtsim_ecology::herds::{HerdSpecies, HerdStats, HerdTunables, Herds, NewHerd};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{PageKey, SaveBatch};
use rtsim_kernel::rows::PageSource;
use rtsim_space::WorldHandle;

use super::paging::{self, Live, Pager};
use super::{State, log_error, log_info, log_warn, phase1, shared};
use crate::wire::{self, Rec, RecordWriter, obs};

/// The herds' pages: table 0x4800 ('H'), next to the dragons' 0x4400 and the ships' 0x5300.
/// Their meta page is id 0 (PF12); their own tables are 0x4810 and up (`Herds` TABLES).
pub const TABLE: u16 = 0x4800;
pub const KEY_HERDS: PageKey = PageKey::new(TABLE, 0);

/// Java's module bit the herds run under: `Module.ECOLOGY` (ordinal 4), as the dragons (Y2).
pub const MODULE_HERDS: u32 = 1 << 4;

/// The herds' seed. PLACEHOLDER: the bridge does not send the Minecraft world's (as phase 1's,
/// the dragons' and the ships').
pub const HERD_SEED: u64 = 0x6d69_635f_6865_7264;

/// The herd kinds of docs/boundary.md §8.1.
fn journaled_kind(k: u16) -> bool {
    (0x0700..=0x07FF).contains(&k)
}

const STATE_KINDS: std::ops::RangeInclusive<u16> = 0x0080..=0x008F;

/// The herds' state the step keeps (`State::herds`).
pub(super) struct Link {
    pub(super) engine: Herds,
    /// Commands not written yet (per session).
    queue: VecDeque<Cmd>,
    pub(super) enabled: bool,
    /// The herds' session reset (bodies of an earlier session or of before a switch-off are gone)
    /// is due the next time they run.
    reset_due: bool,
    /// What the saves owe the herds' pages (a save while switched off, a purge at start).
    pub(super) pager: Pager,
    /// The last load dropped pages it could not read ([`when_pages_unreadable`]).
    pub(super) dropped_pages: bool,
    /// The last load read back herds whose state hash is not the one saved: (saved, read).
    pub(super) restore_diverged: Option<(u64, u64)>,
    pub(super) records: u64,
    /// Records ignored: the herds were switched off.
    pub(super) ignored: u64,
    pub(super) commands_written: u64,
    /// Latest-wins HERD_GROUP_STATE records seen.
    pub(super) group_states: u64,
}

/// The herds' climate: the world model's (X2-6: a frame it knows nothing of still answers as
/// the uniform grassland of Y3).
fn climate(world: &WorldHandle) -> Box<HabitatClimate> {
    Box::new(HabitatClimate::new(world.clone()))
}

fn new_engine(world: &WorldHandle) -> Herds {
    Herds::new(
        HERD_SEED,
        HerdTunables::default(),
        climate(world),
        Tick::ZERO,
    )
}

/// **The policy for herd pages this build cannot read** (as the dragons' S12 and the ships' U1):
/// dropped with a loud log, the herds start afresh, and the next save replaces them.
/// Their pages are deleted at start (`Pager::purge`), before a fresh table could read them.
fn when_pages_unreadable(
    why: &str,
    source: &Arc<dyn PageSource>,
    budget: usize,
    world: &WorldHandle,
) -> Result<Link, String> {
    log_error(&format!(
        "HERD PAGES DROPPED: {why}. The herds start afresh and the next save replaces them"
    ));
    let mut l = Link::fresh(source, budget, world);
    l.dropped_pages = true;
    l.pager.purge = true;
    Ok(l)
}

impl Link {
    pub(super) fn empty(world: &WorldHandle) -> Link {
        Link {
            engine: new_engine(world),
            queue: VecDeque::new(),
            enabled: true,
            reset_due: false,
            pager: Pager::default(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            ignored: 0,
            commands_written: 0,
            group_states: 0,
        }
    }

    /// New herds paging through the engine's store under `budget` row bytes.
    fn fresh(source: &Arc<dyn PageSource>, budget: usize, world: &WorldHandle) -> Link {
        let mut l = Link::empty(world);
        l.engine.attach_store(source.clone(), budget);
        l
    }

    /// Opens the herds' pages (their meta page; the rows are read as they are needed), paging
    /// under `budget` row bytes. `world_restored` says the phase 1 world came back from its
    /// pages: the herds' legs and the players' areas they hold are that world's, so without it
    /// their pages are dropped too. `test`: check the state hash the save wrote (PF16).
    pub(super) fn load(
        source: &Arc<dyn PageSource>,
        budget: usize,
        test: bool,
        world_restored: bool,
        world: &WorldHandle,
    ) -> Result<Link, String> {
        let unreadable = |why: &str| when_pages_unreadable(why, source, budget, world);
        match source.read(KEY_HERDS) {
            Err(e) => return unreadable(&format!("cannot read the herds' meta page: {e}")),
            Ok(None) if paging::holds_pages(&**source, paging::tables(TABLE)) => {
                return unreadable("herd pages without their meta page");
            }
            Ok(None) => return Ok(Link::fresh(source, budget, world)),
            Ok(Some(_)) => {}
        }
        if !world_restored {
            return unreadable("the phase 1 world they live in did not come back from its pages");
        }
        let tun = HerdTunables::default();
        let engine = match Herds::open_paged(source.clone(), budget, KEY_HERDS, tun, climate(world))
        {
            Ok(e) => e,
            Err(e) => return unreadable(&format!("the herds' meta page: {e}")),
        };
        let saved = test
            .then(|| paging::saved_hash(&**source, KEY_HERDS))
            .flatten();
        let hash = saved.map(|_| engine.state_hash());
        let diverged = saved.zip(hash).filter(|(s, h)| s != h);
        if let Some((s, h)) = diverged {
            log_warn(&format!(
                "the herds read back to state {h:016x}, the save had {s:016x}"
            ));
        }
        log_info(&format!(
            "herds restored: {} herds, {} events booked{}",
            engine.herds.len(),
            engine.pending_events(),
            hash.map_or(String::new(), |h| format!(", state {h:016x}"))
        ));
        let mut l = Link::empty(world);
        l.engine = engine;
        l.restore_diverged = diverged;
        Ok(l)
    }

    /// Whether the herds are in the world at all: no herd means nothing to run and nothing that
    /// touches the world.
    fn active(&self) -> bool {
        !self.engine.herds.is_empty()
    }
}

/// Whether the herds' switch is on (founding needs no herd yet).
pub(super) fn enabled(st: &State) -> bool {
    st.herds.enabled
}

/// Whether region `k` is a herd range already, alive or empty (founding's guard against a
/// second herd for a region; EQUILIBRIUM.md §2.4.2: the herds renew a range they hold, so the
/// world model founds each one once). One region index read, not a scan of the herds.
pub(super) fn has_range(st: &State, k: rtsim_space::RegionKey) -> bool {
    !st.herds
        .engine
        .range_herds((k.frame, k.rx, k.rz))
        .is_empty()
}

/// A herd founded from the world model (`worldmodel::simulate`, X2-11): added at the herds'
/// present, its first decision at once, and the areas already booked introduced to it, as a
/// test's HERD_FOUND (Y4) is.
pub(super) fn found(st: &mut State, n: NewHerd, at: Tick) {
    let at = at.max(st.herds.engine.now()).max(st.phase1.world.now());
    shared::run_to(st, at);
    let (name, species, count, pos) = (n.name.clone(), n.species, n.count, n.pos);
    let h = st.herds.engine.add_herd(n, at);
    log_info(&format!(
        "herd founded from the world model: {count} {} as {name:?} at ({}, {}): agent {:#x}",
        species.name(),
        pos.0,
        pos.1,
        rtsim_ecology::herds::herd::agent_of(h)
    ));
    st.phase1.taint();
    if !st.herds.enabled {
        return;
    }
    reset_if_due(st);
    let (w, e) = (&mut st.phase1.world, &mut st.herds.engine);
    e.run_until(&mut w.legs, at);
    take_commands(st);
    shared::introduce_herds(st);
}

/// Whether the herds run: switched on and in the world.
pub(super) fn running(st: &State) -> bool {
    st.herds.enabled && st.herds.active()
}

// ---- hooks: session and latest-wins records -----------------------------------------------------

/// SESSION_START: every group the herds thought embodied is abstract again (puppets never
/// outlive a session; the reset runs as soon as the herds run, now or when switched on), and
/// Java forgot the engine's dictionary.
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    st.herds.queue.clear();
    // The first record of the engine's only session: what it holds is its pages as read, so a
    // switch that is off from the start keeps exactly those (no save writes them).
    st.herds.enabled = module_mask & MODULE_HERDS != 0;
    st.herds.reset_due = true;
    reset_if_due(st);
}

/// The switch changed (MODULES, U10). Off: what the herds hold is written once more as it is now
/// and then kept; queued commands are dropped (all per session). On: the reset is due, and the
/// areas booked meanwhile are introduced to them.
fn set_enabled(st: &mut State, on: bool) {
    let was = st.herds.enabled;
    st.herds.enabled = on;
    if was && !on {
        let _ = st.herds.engine.take_commands();
        st.herds.queue.clear();
        st.herds.pager.save_due = true;
    }
    if on && !was {
        st.herds.reset_due = true;
    }
    reset_if_due(st);
    if on && !was && running(st) {
        shared::introduce_herds(st);
    }
}

/// Bodies of an earlier session (or of before a switch-off) are gone: every group is abstract
/// again, and the legs the herds booked for players go.
fn reset_if_due(st: &mut State) {
    if st.herds.reset_due && running(st) {
        st.herds.reset_due = false;
        let at = Tick(phase1::world_tick(st, st.game_time)).max(st.herds.engine.now());
        let (w, h) = (&mut st.phase1.world, &mut st.herds.engine);
        h.session_start(&mut w.legs, at);
        let _ = h.take_commands();
        st.phase1.taint();
    }
}

/// A record that is not journaled: MODULES and HERD_GROUP_STATE. PLAYER positions go to the
/// shared interest areas (`engine/shared.rs`).
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            let on = wire::u32_at(b, obs::modules::MASK) & MODULE_HERDS != 0;
            set_enabled(st, on);
        }
        k if STATE_KINDS.contains(&k) => {
            if !st.herds.enabled {
                st.herds.ignored += 1;
                return;
            }
            // The engine reads nothing from HERD_GROUP_STATE yet (the herd's position is its
            // closed form); it is counted.
            if boundary::decode_obs(b).is_ok() {
                st.herds.group_states += 1;
            }
        }
        _ => {}
    }
}

// ---- hooks: journaled records -------------------------------------------------------------------

/// The journaled kinds that are the herds' (docs/boundary.md §8.1).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    journaled_kind(rec.kind)
}

/// Op ids `Replay` applies once: a kill's or a taking's (Java's, docs/boundary.md §8.2).
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    match rec.kind {
        boundary::HERD_KILLED | boundary::HERD_TAKEN if rec.size() >= 72 => {
            wire::u64_at(rec.bytes, 64)
        }
        _ => 0,
    }
}

/// A herd record `Replay` admitted. Switched off, the herds ignore it (U10).
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    if !st.herds.enabled {
        st.herds.ignored += 1;
        return;
    }
    let o = match boundary::decode_obs(rec.bytes) {
        Ok((_, _, _, o)) => o,
        Err(e) => {
            log_warn(&format!(
                "a herd record of kind {:#06x} does not decode ({e:?}); skipped",
                rec.kind
            ));
            return;
        }
    };
    // Everything before it happens first: the dragons, the ships, the herds and the world, in
    // time order.
    shared::catch_up(st, rec);
    let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
    let at = Tick(phase1::world_tick(st, gt));
    ingest(st, &o, at);
}

/// Applies one record at `at` (the herds were run to its time by the caller: the shared clock).
fn ingest(st: &mut State, o: &Obs, at: Tick) {
    st.herds.records += 1;
    reset_if_due(st);
    let at = at.max(st.herds.engine.now());
    let (w, h) = (&mut st.phase1.world, &mut st.herds.engine);
    h.ingest(&mut w.legs, o, at);
    st.phase1.taint();
    take_commands(st);
}

// ---- simulate -----------------------------------------------------------------------------------

/// One round of the shared clock (`shared::run_to`): the herds' events up to world tick `t`.
pub(super) fn run_engine(st: &mut State, t: Tick) {
    if !running(st) || t < st.herds.engine.now() {
        return;
    }
    let (w, h) = (&mut st.phase1.world, &mut st.herds.engine);
    let before = h.stats.events;
    h.run_until(&mut w.legs, t);
    if h.stats.events != before {
        st.phase1.taint();
    }
    take_commands(st);
}

/// The step's simulate point, before phase 1's: the herds run to the hand-over's game time,
/// with the dragons and the ships, on the shared clock.
pub(super) fn simulate(st: &mut State) {
    if !running(st) {
        return;
    }
    reset_if_due(st);
    let at = Tick(phase1::world_tick(st, st.game_time));
    shared::run_to(st, at);
}

/// Takes what the herds decided into the link's queue.
pub(super) fn take_commands(st: &mut State) {
    let cmds = st.herds.engine.take_commands();
    st.herds.queue.extend(cmds);
}

// ---- orders (TEST) ------------------------------------------------------------------------------

/// A herd command a test ordered (`Order::HerdCommand`, the vanilla herd adapter's GameTests): it
/// joins the queue as if the herds had decided it, so it takes their sequences and DICT records.
/// Its species ids are Java's, named by `names`; its dimension is Java's id, the herds' own
/// frame. The herds themselves know nothing of it unless its agent is one of theirs: records Java
/// sends back about an agent they do not have are counted late and ignored.
pub(super) fn order_command(st: &mut State, names: &[(u32, u32, String)], record: &[u8]) {
    let Some(dict) = crate::orders::herd_species_dict(names) else {
        log_warn("a herd command order names no herd species; dropped");
        return;
    };
    match boundary::decode_cmd(record, &dict) {
        Ok((_, _, c)) => st.herds.queue.push_back(c),
        Err(e) => log_warn(&format!(
            "a herd command order does not decode ({e:?}); dropped"
        )),
    }
}

/// HERD_FOUND (TEST, Y4): a herd of `count` founded at (x, z) in `dimension` (Java's id, the
/// herds' frame) at the engine's present, its first decision at once. The herds then hear of
/// every player's area already booked, so a player standing there sees it without moving.
#[allow(clippy::too_many_arguments)]
pub(super) fn order_found(
    st: &mut State,
    name: &str,
    species: &str,
    dimension: u32,
    x: i32,
    z: i32,
    count: u64,
    seed: u64,
) {
    let Some(sp) = crate::orders::herd_species(species) else {
        log_warn(&format!("HERD_FOUND of {species}: not a herd species"));
        return;
    };
    let at = Tick(phase1::world_tick(st, st.game_time))
        .max(st.herds.engine.now())
        .max(st.phase1.world.now());
    // Everything before it happens first.
    shared::run_to(st, at);
    let h = st.herds.engine.add_herd(
        NewHerd {
            name: name.to_string(),
            species: sp,
            frame: dimension as u16,
            pos: (x, z),
            count,
            seed: (seed != 0).then_some(seed),
        },
        at,
    );
    log_info(&format!(
        "HERD_FOUND: {count} {} as {name:?} at ({x}, {z}) in dimension {dimension}: agent {:#x}",
        sp.name(),
        rtsim_ecology::herds::herd::agent_of(h)
    ));
    st.phase1.taint();
    if !st.herds.enabled {
        return;
    }
    reset_if_due(st);
    // Its first decision now (its route and region index), then the areas already booked.
    let (w, e) = (&mut st.phase1.world, &mut st.herds.engine);
    e.run_until(&mut w.legs, at);
    take_commands(st);
    shared::introduce_herds(st);
}

// ---- commands -----------------------------------------------------------------------------------

/// What Java would have answered a materialization the engine cannot write: the herd must not
/// wait for a group that never comes.
fn reject(st: &mut State, agent: u64, epoch: u32, reason: u32) {
    let o = Obs::SpawnRejected(SpawnRejected {
        agent,
        epoch,
        reason,
        cmd_seq: 0,
    });
    let at = Tick(phase1::world_tick(st, st.game_time));
    ingest(st, &o, at);
}

fn cmd_size(c: &Cmd) -> usize {
    match c {
        Cmd::Materialize(m) => 64 + 40 * m.members.len(),
        Cmd::Steer(_) => 48,
        Cmd::Despawn(_) => 32,
    }
}

/// Writes the queued herd commands while they fit; each takes the next sequence. Returns the
/// highest sequence written and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    if !st.herds.enabled {
        st.herds.queue.clear();
        return (0, false);
    }
    let mut highest = 0;
    while let Some(c) = st.herds.queue.front().cloned() {
        // The engine's dictionary: the five species, and a materialization's dimension (the
        // herd's frame is Java's dimension id).
        let dim_name = match &c {
            Cmd::Materialize(m) => {
                match st.world.java_dict.get(&(wire::DICT_DIMENSION, m.dimension)) {
                    Some(n) => Some(n.clone()),
                    None => {
                        log_error(&format!(
                            "a herd materialization in dimension {} that Java never named; rejected",
                            m.dimension
                        ));
                        st.herds.queue.pop_front();
                        reject(st, m.agent, m.epoch, 3);
                        continue;
                    }
                }
            }
            _ => None,
        };
        let size = cmd_size(&c);
        if size > wire::MAX_RECORD || size > w.capacity() {
            log_error(&format!(
                "a herd command of {size} bytes can never fit a command buffer; dropped"
            ));
            st.herds.queue.pop_front();
            if let Cmd::Materialize(m) = &c {
                reject(st, m.agent, m.epoch, 6);
            }
            continue;
        }
        let mut names: Vec<(u32, &str)> = HerdSpecies::ALL
            .iter()
            .map(|s| (wire::DICT_BODY, s.entity_id()))
            .collect();
        if let Some(n) = &dim_name {
            names.push((wire::DICT_DIMENSION, n.as_str()));
        }
        let need = st.dict_bytes(&names) + size;
        if need > w.remaining() {
            return (highest, true);
        }
        let body = HerdSpecies::ALL.map(|s| st.dict_id(w, wire::DICT_BODY, s.entity_id()));
        let dict = HerdDict { body };
        let dim = dim_name.map(|n| st.dict_id(w, wire::DICT_DIMENSION, &n));
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(size);
        boundary::encode_cmd(seq, &c, &dict, &mut rec);
        if let Some(d) = dim {
            wire::put_u32(&mut rec, 32, d);
        }
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.herds.commands_written += 1;
        st.herds.queue.pop_front();
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

/// The herds' part of a save at this step boundary (PF13): their meta page and the pages
/// changed since the last save, into `batch`. Commands the herds decided and the link has not
/// taken go to the queue first. Switched off, nothing unless a save is due (U10: the pages stay
/// as they were).
pub(super) fn save(
    st: &mut State,
    batch: &mut SaveBatch,
    test: bool,
) -> (Option<HerdsTicket>, Live) {
    if st.herds.enabled {
        take_commands(st);
    }
    let l = &mut st.herds;
    let out = paging::save(
        &mut l.engine,
        &mut l.pager,
        l.enabled,
        KEY_HERDS,
        batch,
        test,
    );
    if out.0.is_some() {
        l.dropped_pages = false;
    }
    out
}

// ---- inspection ---------------------------------------------------------------------------------

/// [`super::Engine::herd_info`]'s fields, in order.
pub const HERD_INFO: usize = 9;

impl super::Engine {
    /// Runs `f` on the herds and the phase 1 world they live in, under the state lock. For tests
    /// and debugging, between steps only: a step holds the lock while it runs.
    pub fn with_herds<R>(&self, f: impl FnOnce(&Herds, &rtsim_world::World) -> R) -> R {
        let st = super::lock(&self.state);
        f(&st.herds.engine, &st.phase1.world)
    }

    /// Whether the last start read the herds back to the state hash they were saved with
    /// (`None`), or (saved, read); and whether it dropped their pages.
    pub fn herds_restore_check(&self) -> (Option<(u64, u64)>, bool) {
        let st = super::lock(&self.state);
        (st.herds.restore_diverged, st.herds.dropped_pages)
    }

    /// The herds' counters, and how many herd records went through the link, how many it
    /// ignored and how many commands it wrote.
    pub fn herd_stats(&self) -> (HerdStats, u64, u64, u64) {
        let st = super::lock(&self.state);
        (
            st.herds.engine.stats,
            st.herds.records,
            st.herds.ignored,
            st.herds.commands_written,
        )
    }

    /// One herd by its group's agent id, as of the herds' present ([`HERD_INFO`] values): 1 when
    /// the herd exists, animals alive, killed by players, taken, lost to other deaths, the
    /// embodied group's epoch (0 none), its members, the dead-index set's size, and the epoch of
    /// the last materialization. `None` for an agent that is no herd's.
    pub fn herd_info(&self, agent: u64) -> Option<[u64; HERD_INFO]> {
        let st = super::lock(&self.state);
        let h = &st.herds.engine;
        let id = herd_of(agent).filter(|id| h.herd(*id).is_some())?;
        let x = h.herd(id).expect("checked");
        let (epoch, members) = x
            .embodied
            .as_ref()
            .map_or((0, 0), |e| (u64::from(e.epoch), e.members.len() as u64));
        Some([
            1,
            h.count(id, h.now()),
            x.ledger.killed,
            x.ledger.taken,
            x.ledger.lost,
            epoch,
            members,
            x.dead.len() as u64,
            u64::from(x.epoch),
        ])
    }

    /// The agent ids of every herd, in handle order (tests).
    pub fn herd_agents(&self) -> Vec<u64> {
        let st = super::lock(&self.state);
        st.herds
            .engine
            .herds
            .iter()
            .map(|(id, _)| rtsim_ecology::herds::herd::agent_of(id))
            .collect()
    }
}
