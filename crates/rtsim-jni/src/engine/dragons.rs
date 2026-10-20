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
//!   id too) and into `Dragons::ingest`; PLAYER positions feed the dragons' interest areas once
//!   there is a dragon or a lair. Java's dictionary names the species, lairs and dimensions.
//! - **Simulate** ([`simulate`], [`catch_up`]): the dragons run to the hand-over's game time on
//!   the phase 1 world's clock, before the world does, so each dragon event sees the world at its
//!   own time; a phase 1 input runs the dragons to its time first.
//! - **Commands** ([`write_queue`]): DRAGON_MATERIALIZE, STEER, DEPART, ADOPT and DISCARD, with
//!   sequences from the core's counter, after the core's, the industry's and phase 1's. All are per
//!   session (S11): puppets never outlive a session, and a lost adoption or discard is decided
//!   again when Java reports the body again.
//! - **Pages** ([`Link::load`], [`snapshot`]): table 0x4400, one page with `Dragons::save`. The
//!   dragons share the phase 1 world, so while there is any dragon or lair the phase 1 link takes a
//!   snapshot at every save (S4), and both are written in one commit.
//!
//! **The Ice and Fire plugin** (`mic_rtsim_iceandfire`, docs/boundary.md §6.5 and §6.6) is not
//! written yet: nothing in the Java bridge sends these records today. The Rust fake Java side in
//! `tests/common` plays it for the protocol test.

use std::collections::{BTreeSet, VecDeque};

use rtsim_ecology::boundary::{self, Cmd, Obs, SpeciesDict};
use rtsim_ecology::dragon::LairKind;
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_ecology::prey::Item;
use rtsim_ecology::species::Species;
use rtsim_ecology::{DragonStats, DragonTunables, Dragons};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Writer};
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_settle::Pos;

use super::{State, log_error, log_info};
use crate::wire::{self, Rec, RecordWriter, obs};

/// The dragons' pages: table 0x4400 ('D'), next to the industry engine's 0x4900 and phase 1's
/// 0x5000.
pub const TABLE: u16 = 0x4400;
pub const KEY_DRAGONS: PageKey = PageKey::new(TABLE, 0);
const MAGIC: u32 = u32::from_le_bytes(*b"RTDL");
const VERSION: u16 = 1;

/// Java's module bit the dragons run under (`config/Module.java`: ECOLOGY is ordinal 4). With it
/// off the dragons decide nothing new; bodies already out are still captured.
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
    index: DistrictIndex,
    /// Commands not written yet (per session).
    queue: VecDeque<Cmd>,
    enabled: bool,
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
    pub(super) commands_written: u64,
}

fn new_engine() -> Dragons {
    Dragons::new(DRAGON_SEED, DragonTunables::default(), Tick::ZERO)
}

/// **The policy for dragon pages this build cannot read** (S12, as phase 1's
/// `when_pages_unreadable` and industry's W23): dropped with a loud log, the dragons start
/// afresh (the plugin reports every lair and dragon body again as it loads), and the next save
/// replaces them. Returning `Err` here would stop the engine instead.
fn when_pages_unreadable(why: &str) -> Result<Link, String> {
    log_error(&format!(
        "DRAGON PAGES DROPPED: {why}. The dragons start afresh (lairs and bodies are reported again as their chunks load) and the next save replaces them"
    ));
    let mut l = Link::empty();
    l.dropped_pages = true;
    Ok(l)
}

impl Link {
    pub(super) fn empty() -> Link {
        Link {
            engine: new_engine(),
            index: DistrictIndex::default(),
            queue: VecDeque::new(),
            enabled: true,
            java_species: None,
            items_defined: BTreeSet::new(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            commands_written: 0,
        }
    }

    /// Loads the dragons' page. `world_restored` says the phase 1 world came back from its pages:
    /// the dragons live in that world (its knowledge and legs), so without it their page is
    /// dropped too.
    pub(super) fn load(store: &mut PageStore, world_restored: bool) -> Result<Link, String> {
        let page = match store.get(KEY_DRAGONS) {
            Ok(p) => p,
            Err(e) => return when_pages_unreadable(&format!("cannot read the dragons' page: {e}")),
        };
        let Some(b) = page else {
            return Ok(Link::empty());
        };
        if !world_restored {
            return when_pages_unreadable(
                "the phase 1 world they live in did not come back from its pages",
            );
        }
        let mut r = Reader::new(&b);
        let head = (|| -> Result<(), CodecError> {
            if r.u32()? != MAGIC {
                return Err(CodecError::Invalid("not a dragons page".into()));
            }
            let v = r.u16()?;
            if v != VERSION {
                return Err(CodecError::Invalid(format!(
                    "dragons page version {v}, this build reads {VERSION}"
                )));
            }
            Ok(())
        })();
        if let Err(e) = head {
            return when_pages_unreadable(&format!("the dragons' page: {e}"));
        }
        let Ok(saved_hash) = r.u64() else {
            return when_pages_unreadable("the dragons' page is cut short");
        };
        let rest = r.take(r.remaining()).unwrap_or_default();
        match Dragons::load(DragonTunables::default(), rest) {
            Ok(engine) => {
                let hash = engine.state_hash();
                let diverged = (hash != saved_hash).then_some((saved_hash, hash));
                if diverged.is_some() {
                    super::log_warn(&format!(
                        "the dragons read back to state {hash:016x}, the save had {saved_hash:016x}"
                    ));
                }
                log_info(&format!(
                    "dragons restored: {} dragons, {} lairs, {} events booked, state {:016x}",
                    engine.dragons.len(),
                    engine.lairs.len(),
                    engine.pending_events(),
                    engine.state_hash()
                ));
                let mut l = Link::empty();
                l.engine = engine;
                l.restore_diverged = diverged;
                Ok(l)
            }
            Err(e) => when_pages_unreadable(&format!("the dragons' page: {e}")),
        }
    }

    /// Whether the dragons are in the world at all: no dragon and no lair means nothing to run
    /// and nothing that touches the world.
    fn active(&self) -> bool {
        !self.engine.dragons.is_empty() || !self.engine.lairs.is_empty()
    }
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

/// SESSION_START: every dragon the engine thought embodied is abstract again where it was last
/// seen (puppets never outlive a session), and Java forgot the engine's dictionary.
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    st.dragons.enabled = module_mask & MODULE_ECOLOGY != 0;
    st.dragons.queue.clear();
    st.dragons.java_species = None;
    st.dragons.items_defined.clear();
    if st.dragons.active() {
        let at = Tick(super::phase1::world_tick(st, st.world.game_time));
        let (w, d) = (&mut st.phase1.world, &mut st.dragons.engine);
        d.session_start(w, at.max(d.now()));
        st.phase1.taint();
    }
}

/// A record that is not journaled: Java's DICT entries (they may rename species and lairs),
/// MODULES, PLAYER positions and DRAGON_STATE.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::DICT => {
            st.dragons.java_species = None;
            st.dragons.items_defined.clear();
        }
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            st.dragons.enabled = wire::u32_at(b, obs::modules::MASK) & MODULE_ECOLOGY != 0;
        }
        obs::PLAYER if rec.size() >= obs::player::SIZE => {
            if !st.dragons.active() || !st.dragons.enabled {
                return;
            }
            let uuid = (
                wire::u64_at(b, obs::player::UUID_MOST),
                wire::u64_at(b, obs::player::UUID_LEAST),
            );
            let pos = Pos::new(
                rec.aux as u16,
                wire::f64_at(b, obs::player::X).floor() as i32,
                wire::f64_at(b, obs::player::Z).floor() as i32,
            );
            let at = Tick(super::phase1::world_tick(st, st.game_time));
            catch_up_to(st, at);
            let at = at.max(st.dragons.engine.now());
            let (w, d) = (&mut st.phase1.world, &mut st.dragons.engine);
            d.observe_player(w, uuid, pos, at);
            st.phase1.taint();
        }
        k if STATE_KINDS.contains(&k) => {
            let dict = java_species(st);
            if let Ok((_, _, _, o)) = boundary::decode_obs(b, &dict) {
                ingest(st, &o, st.game_time);
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

/// A dragon record `Replay` admitted.
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
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
    ingest(st, &o, gt);
}

fn ingest(st: &mut State, o: &Obs, game_time: i64) {
    st.dragons.records += 1;
    let at = Tick(super::phase1::world_tick(st, game_time));
    let at = at.max(st.dragons.engine.now()).max(st.phase1.world.now());
    with_infra(st, |d, w, infra| d.ingest(w, infra, o, at));
    st.phase1.taint();
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

/// Runs the dragons (and with them the world) to a world tick, if there are any.
fn catch_up_to(st: &mut State, at: Tick) {
    if !st.dragons.active() || at <= st.dragons.engine.now() {
        return;
    }
    let at = at.max(st.phase1.world.now());
    with_infra(st, |d, w, infra| d.run_until(w, infra, at));
    st.phase1.taint();
    take_commands(st);
}

/// Before a phase 1 input at `game_time`: the dragons' events up to it happen first, so the input
/// meets the world they left (and the world never runs past a dragon event).
pub(super) fn catch_up(st: &mut State, rec: &Rec<'_>) {
    if rec.size() >= obs::BODY {
        let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
        let at = Tick(super::phase1::world_tick(st, gt));
        catch_up_to(st, at);
    }
}

/// The step's simulate point, before phase 1's: the dragons run to the hand-over's game time.
pub(super) fn simulate(st: &mut State) {
    if !st.dragons.enabled {
        return;
    }
    let at = Tick(super::phase1::world_tick(st, st.game_time));
    catch_up_to(st, at);
}

fn take_commands(st: &mut State) {
    let cmds = st.dragons.engine.take_commands();
    st.dragons.queue.extend(cmds);
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

fn cmd_size(c: &Cmd) -> usize {
    match c {
        Cmd::Materialize(_) => 136,
        Cmd::Steer(_) | Cmd::Depart(_) => 56,
        Cmd::Adopt(_) => 64,
        Cmd::Discard(_) => 32,
    }
}

/// Writes the queued dragon commands while they fit; each takes the next sequence. Returns the
/// highest sequence written and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    while let Some(c) = st.dragons.queue.front().cloned() {
        // The engine's dictionary: the species bodies, and a materialization's dimension (the
        // dragon's frame is Java's dimension id).
        let dim_name = match &c {
            Cmd::Materialize(m) => {
                match st.world.java_dict.get(&(wire::DICT_DIMENSION, m.dimension)) {
                    Some(n) => Some(n.clone()),
                    None => {
                        // Never written: the dragon must not wait for a body that never comes,
                        // so the engine hears what Java would have answered (unknown dimension).
                        log_error(&format!(
                            "a dragon materialization in dimension {} that Java never named; rejected",
                            m.dimension
                        ));
                        st.dragons.queue.pop_front();
                        let rejected = Obs::SpawnRejected(boundary::SpawnRejected {
                            agent: m.agent,
                            epoch: m.epoch,
                            reason: 3,
                            cmd_seq: 0,
                        });
                        let gt = st.game_time;
                        ingest(st, &rejected, gt);
                        continue;
                    }
                }
            }
            _ => None,
        };
        let mut names: Vec<(u32, &str)> = SPECIES
            .iter()
            .map(|s| (wire::DICT_BODY, s.entity_id()))
            .collect();
        if let Some(n) = &dim_name {
            names.push((wire::DICT_DIMENSION, n.as_str()));
        }
        let need = st.dict_bytes(&names) + cmd_size(&c);
        if need > w.remaining() {
            return (highest, true);
        }
        let body = SPECIES.map(|s| st.dict_id(w, wire::DICT_BODY, s.entity_id()));
        let dict = SpeciesDict {
            body,
            structure: [[0; 2]; 3],
        };
        let dim = dim_name.map(|n| st.dict_id(w, wire::DICT_DIMENSION, &n));
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(cmd_size(&c));
        boundary::encode_cmd(seq, &c, &dict, &mut rec);
        if let Some(d) = dim {
            wire::put_u32(&mut rec, 32, d);
        }
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.dragons.commands_written += 1;
        st.dragons.queue.pop_front();
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

/// The dragons' page, taken at this step boundary with the phase 1 world's snapshot (S4).
/// Commands the dragons decided and the link has not taken go to the queue first, as phase 1's.
pub(super) fn snapshot(st: &mut State) -> Vec<(PageKey, Vec<u8>)> {
    take_commands(st);
    if !st.dragons.active() && !st.dragons.dropped_pages {
        return Vec::new();
    }
    st.dragons.dropped_pages = false;
    let body = st.dragons.engine.save();
    let mut w = Writer::with_capacity(body.len() + 16);
    w.u32(MAGIC);
    w.u16(VERSION);
    w.u64(st.dragons.engine.state_hash());
    w.raw(&body);
    vec![(KEY_DRAGONS, w.into_bytes())]
}

/// Dragon pages a save no longer writes (dropped pages, or no dragons any more).
pub(super) fn deletions(store: &PageStore, pages: &[(PageKey, Vec<u8>)]) -> Vec<PageKey> {
    store
        .keys()
        .filter(|k| k.table() == TABLE && !pages.iter().any(|(p, _)| p == k))
        .collect()
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
}
