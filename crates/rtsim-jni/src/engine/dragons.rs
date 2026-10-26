// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons behind the boundary (docs/boundary.md §6; choices S8 to S14 in
//! docs/overnight-choices-persist.md): `rtsim-ecology`'s engine fed by the Ice and Fire plugin's
//! records, its commands written into the command buffers, its pages in the store.
//!
//! The step calls this module at its observe, simulate, write and save points, beside the industry
//! link and phase 1 (`engine/industry.rs`, `engine/phase1.rs`):
//!
//! - **Observations** ([`observe_state`], [`journaled`]): DRAGON_STATE is latest state; the
//!   journaled kinds 0x0500 to 0x05FF go through the kernel's `Replay` (HOARD_ROBBED by its op
//!   id too) and into `Dragons::ingest`. Players' areas are the engine's shared ones
//!   (`engine/shared.rs`, U2): PLAYER positions go in there once per hand-over, and the dragons
//!   see each area through `rtsim_world::interest::Viewer` and feed their home counter. Java's
//!   dictionary names the species, lairs and dimensions.
//! - **Simulate** ([`simulate`], [`run_engine`]): the dragons run to the hand-over's game time on
//!   the phase 1 world's clock, before the world does, interleaved with the ships' events (the
//!   shared clock, U7), so each dragon event sees the world at its own time; a phase 1 input or a
//!   record runs them to its time first (`shared::catch_up`).
//! - **Commands** ([`write_queue`]): DRAGON_MATERIALIZE, STEER, DEPART, ADOPT and DISCARD, with
//!   sequences from the core's counter, after the core's, the industry's and phase 1's. All are per
//!   session (S11): puppets never outlive a session, and a lost adoption or discard is decided
//!   again when Java reports the body again.
//! - **Pages** ([`Link::load`], [`save`]): the dragons page through the engine's store
//!   (`engine/paging.rs`, PF12): their meta page at table 0x4400 id 0, their row and index tables
//!   at 0x4410 to 0x4416, a save writing the pages changed since the last one. The
//!   dragons share the phase 1 world, so while there is any dragon or lair the phase 1 link takes a
//!   snapshot at every save (S4), and both are written in one commit.
//! - **The switch** (C25, U10): `Module.ECOLOGY` (bit 4). Off, the dragons ignore their records,
//!   run nothing, write nothing and keep the page they had; on again, the bodies of before are
//!   gone (their session reset) and they run on from where they stopped.
//!
//! **The Ice and Fire plugin** (`mic_rtsim_iceandfire`, `neoforge/iceandfire`, docs/boundary.md
//! §6.5 and §6.6) sends these records and applies these commands; its GameTests order dragon
//! commands through [`order_command`] (TEST mode). The Rust fake Java side in `tests/common` plays
//! it for the protocol test.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use rtsim_ecology::boundary::{self, Cmd, Obs, SpeciesDict};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::far::{self, FarLeg, FarState};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_ecology::prey::Item;
use rtsim_ecology::species::Species;
use rtsim_ecology::{DragonStats, DragonTunables, Dragons, DragonsTickets, Placing};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;
use rtsim_kernel::page::{PageKey, SaveBatch};
use rtsim_kernel::rows::PageSource;

use super::paging::{self, Live, Pager};
use super::{State, TransferCmd, log_error, log_info, shared};
use crate::wire::{self, Rec, RecordWriter, cmd, obs};

/// The dragons' pages: table 0x4400 ('D'), next to the industry engine's 0x4900 and phase 1's
/// 0x5000.
pub const TABLE: u16 = 0x4400;
/// Their meta page (PF12); their own tables are 0x4410 and up (`rtsim_ecology` TABLES).
pub const KEY_DRAGONS: PageKey = PageKey::new(TABLE, 0);

/// Java's module bit the dragons run under (`config/Module.java`: ECOLOGY is ordinal 4). With it
/// off the dragons are frozen (U10): no records, no runs, no commands, their page kept.
pub const MODULE_ECOLOGY: u32 = 1 << 4;

/// The dragons' seed. PLACEHOLDER: the bridge does not send the Minecraft world's (as phase 1's).
pub const DRAGON_SEED: u64 = 0x6d69_635f_6472_6167;

/// The dragon kinds of docs/boundary.md §6.1.
fn journaled_kind(k: u16) -> bool {
    (0x0500..=0x05FF).contains(&k)
}

const STATE_KINDS: std::ops::RangeInclusive<u16> = 0x0060..=0x006F;

/// The dragons' state the step keeps (`State::dragons`).
pub(super) struct Link {
    pub(super) engine: Dragons,
    /// Districts by cell for the industry engine's threat interface (rebuilt on its own when the
    /// district count changes; not saved).
    pub(super) index: DistrictIndex,
    /// Commands not written yet (per session).
    queue: VecDeque<Cmd>,
    pub(super) enabled: bool,
    /// The dragons' session reset (bodies of an earlier session or of before a switch-off are
    /// gone) is due the next time they run, or a record reaches them.
    reset_due: bool,
    /// The engine came back from its pages ([`Link::load`]), not fresh: only such an engine
    /// honours a replayed DRAGON_MATERIALIZED (docs/replay-gate.md §5.2, RP-31). Not saved.
    from_pages: bool,
    /// The dragons' replay end (`replay_done`) is due: set by their reset (`begin_replay`), run by
    /// [`replay_end`] at the session's replay end, or at once when no replay runs
    /// (docs/replay-gate.md §3.2). Not saved.
    replay_end_due: bool,
    /// The replay gate's "never" count last logged ([`take_commands`], docs/replay-gate.md
    /// §4.3). Not saved.
    never_logged: u64,
    /// What the saves owe the dragons' pages (a save while switched off, a purge at start).
    pub(super) pager: Pager,
    /// The Java ids of the species bodies and lair structures, from Java's dictionary (built when
    /// first needed after a DICT).
    java_species: Option<SpeciesDict>,
    /// Item ids already mapped to ledger items (`Dragons::define_item`).
    items_defined: BTreeSet<u32>,
    /// The last load dropped pages it could not read ([`when_pages_unreadable`]).
    pub(super) dropped_pages: bool,
    /// The last load read back dragons whose state hash is not the one saved: (saved, read).
    pub(super) restore_diverged: Option<(u64, u64)>,
    pub(super) records: u64,
    /// Records ignored while the dragons were switched off.
    pub(super) ignored: u64,
    pub(super) commands_written: u64,
    /// FAR_LEG records not written yet (per session; docs/boundary.md §12).
    far: VecDeque<FarLeg>,
    pub(super) far_written: u64,
}

fn new_engine() -> Dragons {
    Dragons::new(DRAGON_SEED, DragonTunables::default(), Tick::ZERO)
}

/// **The policy for dragon pages this build cannot read** (S12, as phase 1's
/// `when_pages_unreadable` and industry's W23): dropped with a loud log, the dragons start
/// afresh (the plugin reports every lair and dragon body again as it loads), and the next save
/// replaces them; they are deleted at start (`Pager::purge`), before a fresh table could read
/// them. Returning `Err` here would stop the engine instead.
fn when_pages_unreadable(
    why: &str,
    source: &Arc<dyn PageSource>,
    budget: usize,
) -> Result<Link, String> {
    log_error(&format!(
        "DRAGON PAGES DROPPED: {why}. The dragons start afresh (lairs and bodies are reported again as their chunks load) and the next save replaces them"
    ));
    let mut l = Link::fresh(source, budget);
    l.dropped_pages = true;
    l.pager.purge = true;
    Ok(l)
}

impl Link {
    pub(super) fn empty() -> Link {
        Link {
            engine: new_engine(),
            index: DistrictIndex::default(),
            queue: VecDeque::new(),
            enabled: true,
            reset_due: false,
            from_pages: false,
            replay_end_due: false,
            never_logged: 0,
            pager: Pager::default(),
            java_species: None,
            items_defined: BTreeSet::new(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            ignored: 0,
            commands_written: 0,
            far: VecDeque::new(),
            far_written: 0,
        }
    }

    /// New dragons paging through the engine's store under `budget` row bytes.
    fn fresh(source: &Arc<dyn PageSource>, budget: usize) -> Link {
        let mut l = Link::empty();
        l.engine.attach_store(source.clone(), budget);
        l
    }

    /// Opens the dragons' pages (their meta page; the rows are read as they are needed), paging
    /// under `budget` row bytes. `world_restored` says the phase 1 world came back from its
    /// pages: the dragons live in that world (its knowledge and legs), so without it their pages
    /// are dropped too. `test`: check the state hash the save wrote (PF16).
    pub(super) fn load(
        source: &Arc<dyn PageSource>,
        budget: usize,
        test: bool,
        world_restored: bool,
    ) -> Result<Link, String> {
        let unreadable = |why: &str| when_pages_unreadable(why, source, budget);
        match source.read(KEY_DRAGONS) {
            Err(e) => return unreadable(&format!("cannot read the dragons' meta page: {e}")),
            Ok(None) if paging::holds_pages(&**source, paging::tables(TABLE)) => {
                return unreadable("dragon pages without their meta page");
            }
            Ok(None) => return Ok(Link::fresh(source, budget)),
            Ok(Some(_)) => {}
        }
        if !world_restored {
            return unreadable("the phase 1 world they live in did not come back from its pages");
        }
        let engine = match Dragons::open_paged(
            source.clone(),
            budget,
            KEY_DRAGONS,
            DragonTunables::default(),
        ) {
            Ok(e) => e,
            Err(e) => return unreadable(&format!("the dragons' meta page: {e}")),
        };
        let saved = test
            .then(|| paging::saved_hash(&**source, KEY_DRAGONS))
            .flatten();
        let hash = saved.map(|_| engine.state_hash());
        let diverged = saved.zip(hash).filter(|(s, h)| s != h);
        if let Some((s, h)) = diverged {
            super::log_warn(&format!(
                "the dragons read back to state {h:016x}, the save had {s:016x}"
            ));
        }
        log_info(&format!(
            "dragons restored: {} dragons, {} lairs, {} events booked{}",
            engine.dragons.len(),
            engine.lairs.len(),
            engine.pending_events(),
            hash.map_or(String::new(), |h| format!(", state {h:016x}"))
        ));
        let mut l = Link::empty();
        l.engine = engine;
        l.restore_diverged = diverged;
        l.from_pages = true;
        Ok(l)
    }

    /// Whether the dragons are in the world at all: no dragon, no lair and no oil known means
    /// nothing to run and nothing that touches the world. Oil alone is enough: a field draws field
    /// flights and asks for a lair before any dragon or lair exists (OILGATE.md §4.2, §4.4).
    fn active(&self) -> bool {
        !self.engine.dragons.is_empty()
            || !self.engine.lairs.is_empty()
            || !self.engine.oil.is_empty()
    }
}

/// Whether the dragons run: switched on and in the world.
pub(super) fn running(st: &State) -> bool {
    st.dragons.enabled && st.dragons.active()
}

// ---- dictionaries -----------------------------------------------------------------------------------

const SPECIES: [Species; 3] = [Species::Fire, Species::Ice, Species::Lightning];
const KINDS: [LairKind; 2] = [LairKind::Roost, LairKind::Cave];

/// Java's ids for the three species bodies and six lair structures (0 for a name Java has not
/// defined: Java's ids start at 1, so 0 matches nothing).
fn java_species(st: &mut State) -> SpeciesDict {
    if let Some(d) = &st.dragons.java_species {
        return d.clone();
    }
    let mut d = SpeciesDict {
        body: [0; 3],
        structure: [[0; 2]; 3],
    };
    for (&(kind, id), name) in &st.world.java_dict {
        for (i, s) in SPECIES.iter().enumerate() {
            if kind == wire::DICT_BODY && name == s.entity_id() {
                d.body[i] = id;
            }
            for (j, k) in KINDS.iter().enumerate() {
                if kind == wire::DICT_STRUCTURE && *name == SpeciesDict::structure_name(*s, *k) {
                    d.structure[i][j] = id;
                }
            }
        }
    }
    st.dragons.java_species = Some(d.clone());
    d
}

/// HOARD_ROBBED names items by Java's dictionary: map the ones the phase 1 world trades to its
/// goods (the dragons' hoards hold what they took from it). Others are not in any hoard's ledger.
fn define_items(st: &mut State, ids: &[u32]) {
    for &id in ids {
        if !st.dragons.items_defined.insert(id) {
            continue;
        }
        let Some(name) = st.world.java_dict.get(&(wire::DICT_ITEM, id)) else {
            st.dragons.items_defined.remove(&id);
            continue;
        };
        if let Some(g) = super::phase1::catalog_good(st, name) {
            st.dragons.engine.define_item(id, Item::World(g));
        }
    }
}

// ---- hooks: session and latest-wins records -----------------------------------------------------

/// The dragons' replay end (docs/replay-gate.md §3.2, §5.6), when due and the dragons are
/// switched on: every dragon the engine thought embodied is abstract again where it was last
/// seen, at the replay clock (§3.6, RP-24). Switched off, it stays due; the reset on switch-on
/// runs it.
pub(super) fn replay_end(st: &mut State) {
    end_replay(st, st.replay_clock);
}

/// The replay gate's counts for the replay end's log line (docs/replay-gate.md §4.3).
pub(super) fn gate_counts(st: &State) -> rtsim_ecology::GateCounts {
    st.dragons.engine.replay_gate_counts()
}

/// SESSION_START: the dragons' reset is due (their engine's replay begins as soon as they run or
/// a record reaches them; every dragon the engine thought embodied is abstract again at the
/// replay end, since puppets never outlive a session), and Java forgot the engine's dictionary.
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    st.dragons.queue.clear();
    st.dragons.far.clear();
    st.dragons.java_species = None;
    st.dragons.items_defined.clear();
    // The first record of the engine's only session: what it holds is its pages as read, so a
    // switch that is off from the start keeps exactly those (no save writes them).
    st.dragons.enabled = module_mask & MODULE_ECOLOGY != 0;
    st.dragons.reset_due = true;
    reset_if_due(st);
}

/// The switch changed (MODULES, U10). Off: what the dragons hold is written once more as it is now and then kept;
/// queued commands are dropped (all per session, S11). On: the reset is due.
fn set_enabled(st: &mut State, on: bool) {
    let was = st.dragons.enabled;
    st.dragons.enabled = on;
    if was && !on {
        // Nothing decided before the switch goes out after it: bodies of before are gone when it
        // comes back on.
        let _ = st.dragons.engine.take_commands();
        st.dragons.queue.clear();
        st.dragons.far.clear();
        st.dragons.engine.set_far_feed(false);
        st.dragons.pager.save_due = true;
    }
    if on && !was {
        st.dragons.reset_due = true;
    }
    reset_if_due(st);
}

/// The session's reset, once the dragons run (docs/replay-gate.md §3.2): their engine's replay
/// begins (`begin_replay`, told whether it came back from its pages, RP-31) and its end is due.
/// With no replay running (a switch-on) that end runs at once, at the hand-over's time.
fn reset_if_due(st: &mut State) {
    if st.dragons.reset_due && running(st) {
        reset(st);
    }
}

/// [`reset_if_due`] for the dragons as they are: a record reaches an engine nothing has made
/// active yet, and the gate must hold from the record that does (docs/replay-gate.md §4.1).
fn reset(st: &mut State) {
    st.dragons.reset_due = false;
    let from_pages = st.dragons.from_pages;
    st.dragons.engine.begin_replay(from_pages);
    st.dragons.replay_end_due = true;
    st.phase1.taint();
    take_commands(st);
    if !st.replaying {
        let at = shared::hand_over_target(st);
        end_replay(st, at);
    }
}

/// The dragons' replay end at `clock`: bodies of an earlier session (or of before a switch-off)
/// are gone, so every dragon is abstract again where it was last seen, and Java's far table,
/// which is per session, hears every dragon again (far.rs).
fn end_replay(st: &mut State, clock: Tick) {
    if !st.dragons.replay_end_due || !st.dragons.enabled {
        return;
    }
    st.dragons.replay_end_due = false;
    let (w, d) = (&mut st.phase1.world, &mut st.dragons.engine);
    let at = clock.max(d.now()).max(w.now());
    d.replay_done(w, at);
    d.set_far_feed(true);
    st.phase1.taint();
    take_commands(st);
}

/// A record that is not journaled: Java's DICT entries (they may rename species and lairs),
/// MODULES and DRAGON_STATE. PLAYER positions go to the shared interest areas
/// (`engine/shared.rs`).
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::DICT => {
            st.dragons.java_species = None;
            st.dragons.items_defined.clear();
        }
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            let on = wire::u32_at(b, obs::modules::MASK) & MODULE_ECOLOGY != 0;
            set_enabled(st, on);
        }
        k if STATE_KINDS.contains(&k) => {
            if !st.dragons.enabled {
                st.dragons.ignored += 1;
                return;
            }
            let dict = java_species(st);
            if let Ok((_, _, _, o)) = boundary::decode_obs(b, &dict) {
                // At the hand-over's time, held while replaying (docs/replay-gate.md §3.6).
                let at = shared::hand_over_target(st);
                ingest(st, &o, at);
            }
        }
        _ => {}
    }
}

// ---- hooks: journaled records -------------------------------------------------------------------

/// The journaled kinds that are the dragons' (docs/boundary.md §6.1).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    journaled_kind(rec.kind)
}

/// Op ids `Replay` applies once: a hoard robbery's transfer.
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    if rec.kind == boundary::HOARD_ROBBED && rec.size() >= 40 {
        wire::u64_at(rec.bytes, 32)
    } else {
        0
    }
}

/// A dragon record `Replay` admitted. Switched off, the dragons ignore it (U10).
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    if !st.dragons.enabled {
        st.dragons.ignored += 1;
        // Ignored, but the epoch it names is taken all the same (docs/replay-gate.md §5.3, RP-57,
        // RP-75): the dragons' next body, once they run again, is above it.
        let dict = java_species(st);
        if let Ok((_, _, _, o)) = boundary::decode_obs(rec.bytes, &dict) {
            st.dragons.engine.fold_epoch(&o);
        }
        return;
    }
    // Everything before it happens first: the ships, the dragons and the world, in time order.
    shared::catch_up(st, rec);
    let dict = java_species(st);
    let o = match boundary::decode_obs(rec.bytes, &dict) {
        Ok((_, _, _, o)) => o,
        Err(e) => {
            super::log_warn(&format!(
                "a dragon record of kind {:#06x} does not decode ({e:?}); skipped",
                rec.kind
            ));
            return;
        }
    };
    if let Obs::HoardRobbed(h) = &o {
        let ids: Vec<u32> = h.items.iter().map(|(id, _)| *id).collect();
        define_items(st, &ids);
    }
    let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
    let at = Tick(super::phase1::world_tick(st, gt));
    ingest(st, &o, at);
}

/// A COMMAND_RESULT `Replay` admitted (the core's records, after phase 1's): the answer to a
/// stack of tribute coin sent to a lair's chest (SINK.md §4.6). Applied or a duplicate, the
/// coin is in the chest; failed, it waits for the chest to be reported again; not loaded, the
/// stack waits under its op id for its lair's next chest report (RP-210).
pub(super) fn after_core(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != obs::COMMAND_RESULT || rec.size() < obs::command_result::SIZE {
        return;
    }
    if !st.dragons.enabled {
        // U10: ignored. The stack left the outbox, so it goes again once they are back on and
        // Java answers it DUPLICATE_OP if it is already in the chest (`answered_again`).
        return;
    }
    let op = wire::u64_at(rec.bytes, obs::command_result::OP_ID);
    match rec.aux {
        obs::command_result::APPLIED | obs::command_result::DUPLICATE_OP => {
            st.dragons.engine.tribute_placed(op);
        }
        obs::command_result::FAILED => {
            st.dragons.engine.tribute_failed(op);
        }
        obs::command_result::NOT_LOADED => {
            st.dragons.engine.tribute_unloaded(op);
        }
        _ => {}
    }
}

/// A COMMAND_RESULT for an op `Replay` already admitted: a tribute stack sent again after the
/// pages lost its first answer (the dragons were off, or a crash came before a save). Java
/// answered it DUPLICATE_OP: it is in the chest.
pub(super) fn answered_again(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != obs::COMMAND_RESULT
        || rec.size() < obs::command_result::SIZE
        || !st.dragons.enabled
    {
        return;
    }
    let op = wire::u64_at(rec.bytes, obs::command_result::OP_ID);
    if st.dragons.engine.tribute_placed(op) {
        st.outbox.complete(OpId(op));
    }
}

/// TRIBUTE_START (a test's order, applied at step start): village `village` becomes the
/// tributary of lair `lair` (keys), as if a dragon of the lair had raided it now.
pub(super) fn tribute_start(st: &mut State, village: u64, lair: u64) {
    if !running(st) {
        return; // U10: switched off, the order waits for nothing
    }
    let at = Tick(super::phase1::world_tick(st, st.game_time));
    let at = at.max(st.dragons.engine.now()).max(st.phase1.world.now());
    let started = with_infra(st, |d, w, infra| {
        d.tribute_start(w, infra, village, lair, at)
    });
    if !started {
        super::log_warn(&format!(
            "TRIBUTE_START: village {village} and lair {lair:#x} started no tribute (unknown, or no purse to pay from)"
        ));
    }
    st.phase1.taint();
    take_commands(st);
}

/// NEST_DAY (a test's order, applied at step start): the oil nests watch every `ticks` from now
/// (0 restores the tunable's day).
pub(super) fn nest_day(st: &mut State, ticks: u64) {
    st.dragons.engine.set_nest_day(ticks);
    st.phase1.taint();
}

/// Applies one record at world tick `at` (a journaled record's own time, or the hand-over's as
/// the clock may reach it, `shared::hand_over_target`).
fn ingest(st: &mut State, o: &Obs, at: Tick) {
    st.dragons.records += 1;
    if st.dragons.reset_due && st.dragons.enabled {
        reset(st);
    }
    let at = at.max(st.dragons.engine.now()).max(st.phase1.world.now());
    // A copy (REASSERT) of an ended session's record is applied by identity, never against a
    // body (docs/replay-gate.md §2.3, §5.7, RP-8, RP-9).
    let copy = st.applying_copy.is_some();
    with_infra(st, |d, w, infra| {
        if copy {
            d.ingest_copy(w, infra, o, at)
        } else {
            d.ingest(w, infra, o, at)
        }
    });
    st.phase1.taint();
    reset_if_due(st);
    take_commands(st);
}

/// Runs `f` on the dragons with the phase 1 world and the industry engine's threat interface.
fn with_infra<R>(
    st: &mut State,
    f: impl FnOnce(
        &mut Dragons,
        &mut rtsim_world::World,
        &mut dyn rtsim_ecology::infra::Infrastructure,
    ) -> R,
) -> R {
    let (d, w, index) = (
        &mut st.dragons.engine,
        &mut st.phase1.world,
        &mut st.dragons.index,
    );
    if st.industry.enabled() {
        let mut infra = IndustryLink {
            engine: &mut st.industry.engine,
            index,
        };
        f(d, w, &mut infra)
    } else {
        f(d, w, &mut NoInfrastructure)
    }
}

// ---- simulate -----------------------------------------------------------------------------------

/// One round of the shared clock (`shared::run_to`): the dragons (and with them the world) run
/// to world tick `t`, if they run. A round at their own time pops what is due then.
pub(super) fn run_engine(st: &mut State, t: Tick) {
    if !running(st) || t < st.dragons.engine.now() {
        return;
    }
    let t = t.max(st.phase1.world.now());
    if super::herds::running(st) && t >= st.herds.engine.now() {
        // The herds are prey (PLAN.md §10.2): their events run in the dragons' time order, and
        // hungry dragons weigh and hunt them (`Dragons::run_until_with_herds`, Y7).
        let (d, w, index, h) = (
            &mut st.dragons.engine,
            &mut st.phase1.world,
            &mut st.dragons.index,
            &mut st.herds.engine,
        );
        if st.industry.enabled() {
            let mut infra = IndustryLink {
                engine: &mut st.industry.engine,
                index,
            };
            d.run_until_with_herds(w, &mut infra, h, t);
        } else {
            d.run_until_with_herds(w, &mut NoInfrastructure, h, t);
        }
        super::herds::take_commands(st);
    } else {
        with_infra(st, |d, w, infra| d.run_until(w, infra, t));
    }
    st.phase1.taint();
    take_commands(st);
}

/// The step's simulate point, before phase 1's: the dragons run to the hand-over's game time,
/// with the ships, on the shared clock.
pub(super) fn simulate(st: &mut State) {
    if !running(st) {
        return;
    }
    reset_if_due(st);
    let at = shared::hand_over_target(st);
    shared::run_to(st, at);
}

/// Takes what the dragons decided into the link's queue. A rise in the gate's "never" count is
/// logged at once (docs/replay-gate.md §4.3): a kind that embodies decided while replaying.
fn take_commands(st: &mut State) {
    let counts = st.dragons.engine.replay_gate_counts();
    if counts.never > st.dragons.never_logged {
        let (code, name) = counts.last_never.unwrap_or((0, "?"));
        log_error(&format!(
            "the dragons decided a {name} ({code:#06x}) while replaying ({} since the last take): \
             skipped, a bug (docs/replay-gate.md §4)",
            counts.never - st.dragons.never_logged
        ));
        st.dragons.never_logged = counts.never;
    }
    let cmds = st.dragons.engine.take_commands();
    st.dragons.queue.extend(cmds);
    let now = st.dragons.engine.now();
    let legs = st.dragons.engine.take_far(now);
    st.dragons.far.extend(legs);
}

// ---- commands -----------------------------------------------------------------------------------

/// A dragon command a test ordered (`Order::DragonCommand`, the Ice and Fire plugin's GameTests):
/// it joins the queue as if the dragons had decided it, so it takes their sequences and DICT
/// records. Its species ids are Java's, named by `names` (so it never waits for Java's DICT
/// records); its dimension is Java's id, the dragons' own frame. The dragons themselves know
/// nothing of it: records Java sends back about an agent they do not have are ignored.
pub(super) fn order_command(st: &mut State, names: &[(u32, u32, String)], record: &[u8]) {
    let mut dict = SpeciesDict {
        body: [0; 3],
        structure: [[0; 2]; 3],
    };
    for (kind, id, name) in names {
        for (i, s) in SPECIES.iter().enumerate() {
            if *kind == wire::DICT_BODY && name == s.entity_id() {
                dict.body[i] = *id;
            }
        }
    }
    match boundary::decode_cmd(record, &dict) {
        Ok((_, _, c)) => st.dragons.queue.push_back(c),
        Err(e) => super::log_warn(&format!(
            "a dragon command order does not decode ({e:?}); dropped"
        )),
    }
}

/// Writes the queued dragon commands while they fit; each takes the next sequence. Returns the
/// highest sequence written and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    if !st.dragons.enabled {
        st.dragons.queue.clear();
        return (0, false);
    }
    let (mut highest, more) = write_tribute(st, w);
    if more {
        return (highest, true);
    }
    while let Some(c) = st.dragons.queue.front().cloned() {
        // The engine's dictionary: the species bodies, a placed lair's structure, and the
        // dimension a record names (the engine's frame is Java's dimension id).
        let dim_name = match c.dimension() {
            Some((_, dim)) => match st.world.java_dict.get(&(wire::DICT_DIMENSION, dim)) {
                Some(n) => Some(n.clone()),
                None => {
                    st.dragons.queue.pop_front();
                    if let Cmd::Materialize(m) = &c {
                        // Never written: the dragon must not wait for a body that never comes,
                        // so the engine hears what Java would have answered (unknown dimension).
                        log_error(&format!(
                            "a dragon materialization in dimension {dim} that Java never named; rejected"
                        ));
                        let rejected = Obs::SpawnRejected(boundary::SpawnRejected {
                            agent: m.agent,
                            epoch: m.epoch,
                            reason: 3,
                            cmd_seq: 0,
                        });
                        let at = shared::hand_over_target(st);
                        ingest(st, &rejected, at);
                    } else {
                        // An oil gate ask (a lair placed, a survey) is only an ask: dropped.
                        super::log_warn(&format!(
                            "a dragon command {:#06x} in dimension {dim} that Java never named; dropped",
                            c.kind()
                        ));
                    }
                    continue;
                }
            },
            None => None,
        };
        let structure = match &c {
            Cmd::PlaceLair(p) => Some((
                p.species,
                p.kind,
                SpeciesDict::structure_name(p.species, p.kind),
            )),
            _ => None,
        };
        let mut names: Vec<(u32, &str)> = SPECIES
            .iter()
            .map(|s| (wire::DICT_BODY, s.entity_id()))
            .collect();
        if let Some(n) = &dim_name {
            names.push((wire::DICT_DIMENSION, n.as_str()));
        }
        if let Some((_, _, n)) = &structure {
            names.push((wire::DICT_STRUCTURE, n.as_str()));
        }
        let need = st.dict_bytes(&names) + c.size();
        if need > w.remaining() {
            return (highest, true);
        }
        let body = SPECIES.map(|s| st.dict_id(w, wire::DICT_BODY, s.entity_id()));
        let mut dict = SpeciesDict {
            body,
            structure: [[0; 2]; 3],
        };
        if let Some((sp, kind, n)) = &structure {
            dict.structure[sp.index()][matches!(kind, LairKind::Cave) as usize] =
                st.dict_id(w, wire::DICT_STRUCTURE, n);
        }
        let dim = dim_name.map(|n| st.dict_id(w, wire::DICT_DIMENSION, &n));
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(c.size());
        boundary::encode_cmd(seq, &c, &dict, &mut rec);
        if let (Some(d), Some((at, _))) = (dim, c.dimension()) {
            wire::put_u32(&mut rec, at, d);
        }
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.dragons.commands_written += 1;
        st.dragons.queue.pop_front();
    }
    let (h, more) = write_far(st, w);
    (highest.max(h), more)
}

/// Writes the queued FAR_LEG records (docs/boundary.md §12) while they fit, after the dragons'
/// own commands: each takes the next sequence, its species and dimension named in the session's
/// dictionary and its times moved to game time.
fn write_far(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    while let Some(r) = st.dragons.far.front().copied() {
        let dim_name = if r.state == FarState::Gone {
            None
        } else {
            match st.world.java_dict.get(&(wire::DICT_DIMENSION, u32::from(r.frame))) {
                Some(n) => Some(n.clone()),
                None => {
                    // A dimension Java never named has no players to draw it for.
                    st.dragons.far.pop_front();
                    continue;
                }
            }
        };
        let mut names: Vec<(u32, &str)> = Vec::with_capacity(2);
        if r.state != FarState::Gone {
            names.push((wire::DICT_BODY, r.species.entity_id()));
        }
        if let Some(n) = &dim_name {
            names.push((wire::DICT_DIMENSION, n.as_str()));
        }
        let need = st.dict_bytes(&names) + far::FAR_LEG_SIZE;
        if need > w.remaining() {
            return (highest, true);
        }
        let body = if r.state == FarState::Gone {
            0
        } else {
            st.dict_id(w, wire::DICT_BODY, r.species.entity_id())
        };
        let dim = dim_name.map_or(0, |n| st.dict_id(w, wire::DICT_DIMENSION, &n));
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(far::FAR_LEG_SIZE);
        far::encode(seq, &r, body, dim, st.phase1.offset, &mut rec);
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.dragons.far_written += 1;
        st.dragons.far.pop_front();
    }
    (highest, false)
}

/// Writes the tribute stacks the dragons send to lairs' chests (SINK.md §4.6) that are not in
/// the core outbox yet and not parked (their chunk was not loaded, RP-210). Each joins it under
/// its own op id, so a restart sends it again and Java answers a stack it already put
/// DUPLICATE_OP. Returns the highest sequence written and whether some wait for the next buffer.
fn write_tribute(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let sent: BTreeSet<u64> = st.outbox.after(0).map(|(_, op, _)| op.0).collect();
    let todo: Vec<(u64, Placing)> = st
        .dragons
        .engine
        .placing_to_send()
        .filter(|(op, _)| !sent.contains(op))
        .map(|(op, p)| (op, *p))
        .collect();
    let mut highest = 0;
    for (op, p) in todo {
        let dimension = st
            .world
            .java_dict
            .get(&(wire::DICT_DIMENSION, p.chest.dimension))
            .cloned();
        let item = match p.item {
            Item::World(g) => super::phase1::catalog_item(st, g),
            Item::Industry(_) => None,
        };
        let (Some(dimension), Some(item)) = (dimension, item) else {
            // Never written: the coin waits for the chest to be reported again.
            log_error(&format!(
                "a tribute stack (op {op:#x}) to a chest in dimension {} Java never named, or of a good no item stands for; back to its lair",
                p.chest.dimension
            ));
            st.dragons.engine.tribute_failed(op);
            continue;
        };
        let t = TransferCmd {
            dimension,
            item: item.to_string(),
            pos: p.chest.pos,
            count: p.count as i32,
        };
        let need = st.dict_bytes(&[
            (wire::DICT_DIMENSION, &t.dimension),
            (wire::DICT_ITEM, &t.item),
        ]) + cmd::transfer_to_container::SIZE;
        if need > w.remaining() {
            return (highest, true);
        }
        // The DICT records go first, and the sequence is taken only now, so the buffer's
        // sequences rise in buffer order.
        st.dict_id(w, wire::DICT_DIMENSION, &t.dimension);
        st.dict_id(w, wire::DICT_ITEM, &t.item);
        let seq = st.outbox.push_with_op(t.clone(), OpId(op));
        super::write_transfer(st, w, seq, OpId(op), &t);
        highest = highest.max(seq);
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

/// The dragons' part of a save at this step boundary, with the phase 1 world's snapshot (S4,
/// PF13): their meta page and the pages changed since the last save, into `batch`. Commands the
/// dragons decided and the link has not taken go to the queue first, as phase 1's. Switched off,
/// nothing unless a save is due (U10: the pages stay as they were).
pub(super) fn save(
    st: &mut State,
    batch: &mut SaveBatch,
    test: bool,
) -> (Option<DragonsTickets>, Live) {
    if st.dragons.enabled {
        take_commands(st);
    }
    let l = &mut st.dragons;
    let out = paging::save(
        &mut l.engine,
        &mut l.pager,
        l.enabled,
        KEY_DRAGONS,
        batch,
        test,
    );
    if out.0.is_some() {
        l.dropped_pages = false;
    }
    out
}

// ---- inspection ---------------------------------------------------------------------------------

impl super::Engine {
    /// Runs `f` on the dragons and the phase 1 world they live in, under the state lock. For tests
    /// and debugging, between steps only: a step holds the lock while it runs.
    pub fn with_dragons<R>(&self, f: impl FnOnce(&Dragons, &rtsim_world::World) -> R) -> R {
        let st = super::lock(&self.state);
        f(&st.dragons.engine, &st.phase1.world)
    }

    /// Whether the last start read the dragons back to the state hash they were saved with
    /// (`None`), or (saved, read); and whether it dropped their pages.
    pub fn dragons_restore_check(&self) -> (Option<(u64, u64)>, bool) {
        let st = super::lock(&self.state);
        (st.dragons.restore_diverged, st.dragons.dropped_pages)
    }

    /// The dragons' counters, and how many dragon records and commands went through the link.
    pub fn dragon_stats(&self) -> (DragonStats, u64, u64) {
        let st = super::lock(&self.state);
        (
            st.dragons.engine.stats,
            st.dragons.records,
            st.dragons.commands_written,
        )
    }

    /// How many dragon records the link ignored while the dragons were switched off (U10).
    pub fn dragons_ignored(&self) -> u64 {
        super::lock(&self.state).dragons.ignored
    }

    /// Phase 1 village `village`'s fear to the dragons before a dragon's boldness bends it
    /// (fullness points, its iron golems' included: SINK.md §4.6), or -1 for a village the world
    /// does not know.
    pub fn dragons_village_risk(&self, village: u64) -> i64 {
        let st = super::lock(&self.state);
        match st.phase1.world.node_of_key(village) {
            Some(n) => st.dragons.engine.village_risk(&st.phase1.world, n),
            None => -1,
        }
    }

    /// The dragons' tribute (SINK.md §4.6): seasons paid, whole coins put in lairs' chests,
    /// whole coins waiting for a chest, and stacks sent and not yet answered.
    pub fn dragons_tribute(&self) -> [i64; 4] {
        let st = super::lock(&self.state);
        let d = &st.dragons.engine;
        let placed = d.lairs.iter().map(|l| l.placed).sum();
        let unplaced = d.lairs.iter().flat_map(|l| l.unplaced.values()).sum();
        [
            d.stats.tributes as i64,
            placed,
            unplaced,
            d.placing().count() as i64,
        ]
    }

    /// Oil nest `nest` (PLACE_NEST's id, OILGATE.md §4.6) as of the engine's last step:
    /// `rtsim_ecology::nest::NEST_INFO` values; None for a nest the engine does not keep.
    pub fn dragons_nest(&self, nest: u64) -> Option<[i64; rtsim_ecology::nest::NEST_INFO]> {
        let st = super::lock(&self.state);
        st.dragons.engine.nest_info(nest)
    }

    /// The oil gate's counters (OILGATE.md §4, `DragonStats`): oil chunks indexed, fields,
    /// field-flight rolls and draws, refuels, drinking raids, mB drunk, breaths, dry raids,
    /// Encroachment reports, district raids, lairs placed, visitors gone, surveys; then `agent`'s
    /// oil reserve now (-1 for no such dragon) and the OIL_CONFIG switches; then the engine's
    /// clock and the field flights' probe (`Dragons::flight_probe`). Between steps; tests.
    pub fn dragons_oil(&self, agent: u64) -> [i64; 26] {
        let st = super::lock(&self.state);
        let d = &st.dragons.engine;
        let s = &d.stats;
        let level = rtsim_ecology::dragon::dragon_of(agent)
            .and_then(|x| d.oil_level(x, d.now()))
            .map_or(-1, i64::from);
        let p = d.flight_probe(d.now());
        [
            s.oil_chunks as i64,
            s.fields as i64,
            s.field_rolls as i64,
            s.field_draws as i64,
            s.refuels as i64,
            s.oil_raids as i64,
            s.oil_drunk_mb as i64,
            s.breaths as i64,
            s.dry_raids as i64,
            s.encroachments as i64,
            s.district_target_raids as i64,
            s.lairs_placed as i64,
            s.visitors_left as i64,
            s.surveys as i64,
            level,
            i64::from(d.oil_flags),
            d.now().0 as i64,
            p[0],
            p[1],
            p[2],
            p[3],
            p[4],
            p[5],
            p[6],
            p[7],
            p[8],
        ]
    }
}
