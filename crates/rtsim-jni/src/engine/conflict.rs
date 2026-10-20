// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine behind the boundary (docs/boundary.md §9; choices C2-1 to C2-17 in
//! docs/overnight-choices-conflictjni.md): `rtsim-conflict`'s factions, feuds and war bands fed by
//! a conflict plugin's records and by the other modules inside the engine, its commands written
//! into the command buffers, its page in the store. No plugin exists yet (P8): the Rust fake Java
//! side in `tests/common` (`Java::conflict_obs`, `conflict_command`) plays one, after
//! `rtsim_conflict::synth::FakeConflict`.
//!
//! The step calls this module at its observe, simulate, write and save points, beside the dragons,
//! the ships and the herds (`engine/dragons.rs`, `engine/ships.rs`, `engine/herds.rs`), whom it
//! mirrors:
//!
//! - **Observations** ([`observe_state`], [`journaled`]): BAND_STATE is latest state (counted);
//!   the journaled kinds 0x0800 to 0x08FF go through the kernel's `Replay` (CHIEF_KILLED,
//!   TRIBUTE_PAID, GIFT_GIVEN and BAND_LOOTED by their op ids too) into `Conflict::ingest`, at
//!   their own time with the other modules and the world run to it first (the shared clock,
//!   `engine/shared.rs`). Java's ids are translated to the conflict's own first (C2-4): items to
//!   the phase 1 catalogue's goods, bodies to the culture data's, structures and pollution types
//!   to the conflict's own dictionary; dimensions are Java's ids (the conflict's frames).
//! - **Signals inside the engine** ([`simulate`], C2-7 to C2-9): each district's learned pollution
//!   (the Destroy plugin's attribution, I14, written as deposit goods) and, while unloaded, the
//!   deposits it drains, become the settlers' continuous sources; raids on districts go through
//!   the industry engine's threat interface carrying the band's own loads (C2-1); raids on
//!   villages and havens move stock in the phase 1 world (`World::stock_to_held`).
//! - **Simulate** ([`simulate`], [`run_engine`]): the conflict runs to the hand-over's time on the
//!   phase 1 world's clock (its marches are legs in the world's leg index, its chiefs notables in
//!   its knowledge); players' areas are the engine's shared ones, the conflict being a
//!   `rtsim_world::interest::Viewer` (owner bit 59), so war bands and envoys that enter a player's
//!   view materialize.
//! - **Commands** ([`write_queue`]): TERRITORY, FACTION_NOTICE, ENVOY_MATERIALIZE,
//!   BAND_MATERIALIZE, BAND_STEER, BAND_RECALL and DEMAND_SETTLED, with sequences from the core's
//!   counter after every other module's: all per session (K2-27, as the dragons' S11), so nothing
//!   is kept or resent and none carries an op id; TERRITORY goes again for every faction at a
//!   session start. Body kinds, the dimension and the tribute item go through the engine's
//!   dictionary; ticks (an envoy's deadline, a steer's arrival) are game time.
//! - **Pages** ([`Link::load`], [`snapshot`]): table 0x4300 ('C'), one page with `Conflict::save`
//!   and its state hash, checked on load. The conflict lives in the phase 1 world (havens are its
//!   nodes, marches its legs), so while it is active the phase 1 link snapshots at every save, in
//!   the same commit; a page without the world it was saved with is dropped loudly.
//! - **The switch** (C25, U10): `Module.CONFLICT` (bit 5). Off, the conflict ignores its records,
//!   runs nothing, writes nothing and keeps the page it had; on again, its bodies of before are
//!   gone (its session reset) and it runs on from where it stopped.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_conflict::boundary::{self, Cmd, Item, Obs};
use rtsim_conflict::engine::{body_id, body_names, item_of};
use rtsim_conflict::{Conflict, ConflictStats, ConflictTunables, Cultures, SignalKind};
use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_industry::cluster::Run;
use rtsim_industry::district::{Phase as DistrictPhase, Presence};
use rtsim_industry::ids::{DistrictId, GoodClass};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::{CodecError, Reader, Writer};
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_settle::Pos;
use rtsim_trade::GoodId;

use super::{State, log_error, log_info, log_warn, phase1, shared};
use crate::wire::{self, Rec, RecordWriter, obs};

/// The conflict's pages: table 0x4300 ('C', docs/boundary.md §9.1), next to the dragons' 0x4400.
pub const TABLE: u16 = 0x4300;
pub const KEY_CONFLICT: PageKey = PageKey::new(TABLE, 0);
const MAGIC: u32 = u32::from_le_bytes(*b"RTCF");
const VERSION: u16 = 1;

/// Java's module bit the conflict runs under (`config/Module.java`: CONFLICT is ordinal 5).
pub const MODULE_CONFLICT: u32 = 1 << 5;

/// The conflict's seed. PLACEHOLDER: the bridge does not send the Minecraft world's (as phase 1's,
/// the dragons', the ships' and the herds').
pub const CONFLICT_SEED: u64 = 0x6d69_635f_636f_6e66;

/// Dictionary kind 0x20 (Destroy pollution types, docs/boundary.md §9.1).
const DICT_POLLUTION: u32 = boundary::DICT_POLLUTION as u32;

/// Deposits that are not counted one mining signal per unit (C2-8): their units are millibuckets
/// (UNVERIFIED), so a bucket counts as one. Every other deposit a district drains (an Ore
/// Excavation vein's extraction cycles) counts one per unit. PLACEHOLDER.
const DEPOSIT_MINING_SCALE: [(&str, f64); 2] = [
    ("destroy:crude_oil", 0.001),
    ("createdieselgenerators:oil", 0.001),
];

/// The signals a district's sources carry (C2-7, C2-8).
const FED_KINDS: [SignalKind; 3] = [SignalKind::Smog, SignalKind::Radiation, SignalKind::Mining];

/// The conflict kinds of docs/boundary.md §9.1.
fn journaled_kind(k: u16) -> bool {
    (0x0800..=0x08FF).contains(&k)
}

const STATE_KINDS: std::ops::RangeInclusive<u16> = 0x0090..=0x009F;

/// A command waiting for the buffer, with the names of the ids it carries when they are not the
/// conflict's own (a test's order names Java's).
#[derive(Clone, Debug)]
struct Queued {
    cmd: Cmd,
    /// (dictionary kind, id) to name; `None`: the culture data's bodies and the catalogue's goods.
    names: Option<BTreeMap<(u32, u32), String>>,
}

/// The conflict's state the step keeps (`State::conflict`).
pub(super) struct Link {
    pub(super) engine: Conflict,
    /// Districts by cell for the industry engine's threat interface (not saved).
    index: DistrictIndex,
    /// Commands not written yet (per session).
    queue: VecDeque<Queued>,
    enabled: bool,
    /// The conflict's session reset (bodies of an earlier session or of before a switch-off are
    /// gone) is due the next time it runs.
    reset_due: bool,
    /// The page as last written or read: what a save keeps while the conflict is switched off.
    last_page: Option<Vec<u8>>,
    /// The conflict's own dictionary ids of structures (kind 4) and pollution types (kind 0x20),
    /// by name (C2-4).
    structures: BTreeMap<String, u32>,
    pollution: BTreeMap<String, u32>,
    /// Districts a conflict plugin reported POLLUTION for this session: while loaded, their
    /// pollution is the plugin's, not the industry engine's learned rate (C2-7).
    reported: BTreeSet<u64>,
    /// The last load dropped pages it could not read ([`when_pages_unreadable`]).
    pub(super) dropped_pages: bool,
    /// The last load read back a conflict whose state hash is not the one saved: (saved, read).
    pub(super) restore_diverged: Option<(u64, u64)>,
    pub(super) records: u64,
    /// Records ignored: the conflict was switched off.
    pub(super) ignored: u64,
    pub(super) commands_written: u64,
    /// Latest-wins BAND_STATE records seen (read for nothing yet).
    pub(super) band_states: u64,
    /// Sources the industry engine's districts set or changed (C2-7, C2-8).
    pub(super) fed: u64,
}

/// The conflict's own dictionary (C2-4): the catalogue's goods as items (`item_of`), the culture
/// data's bodies (`body_id`), every culture's camp structures and the tunables' pollution types,
/// ids from 1 in a fixed order.
fn own_names(
    c: &Cultures,
    tun: &ConflictTunables,
) -> (BTreeMap<String, u32>, BTreeMap<String, u32>) {
    let mut structures = BTreeMap::new();
    for cu in &c.list {
        for s in &cu.def.structures {
            let next = structures.len() as u32 + 1;
            structures.entry(s.clone()).or_insert(next);
        }
    }
    let mut pollution = BTreeMap::new();
    for (name, _, _) in &tun.pollution {
        let next = pollution.len() as u32 + 1;
        pollution.entry(name.clone()).or_insert(next);
    }
    (structures, pollution)
}

fn define_own(
    e: &mut Conflict,
    structures: &BTreeMap<String, u32>,
    pollution: &BTreeMap<String, u32>,
) {
    let (goods, _) = rtsim_world::synth::goods();
    for (g, d) in goods.iter() {
        e.define(2, item_of(g), &d.name);
    }
    for (i, n) in body_names(&e.cultures).iter().enumerate() {
        e.define(3, i as u32 + 1, n);
    }
    for (n, id) in structures {
        e.define(4, *id, n);
    }
    for (n, id) in pollution {
        e.define(boundary::DICT_POLLUTION, *id, n);
    }
}

fn new_engine() -> Conflict {
    Conflict::new(
        CONFLICT_SEED,
        ConflictTunables::default(),
        Cultures::builtin(),
        Tick::ZERO,
    )
}

/// **The policy for conflict pages this build cannot read** (as the dragons' S12, the ships' U1
/// and the herds' Y6): dropped with a loud log, the conflict starts afresh (the plugin reports its
/// camps again as their chunks load), and the next save replaces them.
fn when_pages_unreadable(why: &str) -> Result<Link, String> {
    log_error(&format!(
        "CONFLICT PAGES DROPPED: {why}. The conflict starts afresh (camps are reported again as their chunks load) and the next save replaces them"
    ));
    let mut l = Link::empty();
    l.dropped_pages = true;
    Ok(l)
}

impl Link {
    pub(super) fn empty() -> Link {
        let mut engine = new_engine();
        let (structures, pollution) = own_names(&engine.cultures, &engine.tun);
        define_own(&mut engine, &structures, &pollution);
        Link::with_engine(engine, structures, pollution)
    }

    fn with_engine(
        engine: Conflict,
        structures: BTreeMap<String, u32>,
        pollution: BTreeMap<String, u32>,
    ) -> Link {
        Link {
            engine,
            index: DistrictIndex::default(),
            queue: VecDeque::new(),
            enabled: true,
            reset_due: false,
            last_page: None,
            structures,
            pollution,
            reported: BTreeSet::new(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            ignored: 0,
            commands_written: 0,
            band_states: 0,
            fed: 0,
        }
    }

    /// Loads the conflict's page. `world_restored` says the phase 1 world came back from its
    /// pages: the conflict's havens, legs and notables are that world's, so without it the page
    /// is dropped too.
    pub(super) fn load(store: &mut PageStore, world_restored: bool) -> Result<Link, String> {
        let page = match store.get(KEY_CONFLICT) {
            Ok(p) => p,
            Err(e) => {
                return when_pages_unreadable(&format!("cannot read the conflict's page: {e}"));
            }
        };
        let Some(b) = page else {
            return Ok(Link::empty());
        };
        if !world_restored {
            return when_pages_unreadable(
                "the phase 1 world it lives in did not come back from its pages",
            );
        }
        let mut r = Reader::new(&b);
        let head = (|| -> Result<(), CodecError> {
            if r.u32()? != MAGIC {
                return Err(CodecError::Invalid("not a conflict page".into()));
            }
            let v = r.u16()?;
            if v != VERSION {
                return Err(CodecError::Invalid(format!(
                    "conflict page version {v}, this build reads {VERSION}"
                )));
            }
            Ok(())
        })();
        if let Err(e) = head {
            return when_pages_unreadable(&format!("the conflict's page: {e}"));
        }
        let Ok(saved_hash) = r.u64() else {
            return when_pages_unreadable("the conflict's page is cut short");
        };
        let rest = r.take(r.remaining()).unwrap_or_default();
        match Conflict::load(ConflictTunables::default(), Cultures::builtin(), rest) {
            Ok(engine) => {
                let hash = engine.state_hash();
                let diverged = (hash != saved_hash).then_some((saved_hash, hash));
                if diverged.is_some() {
                    log_warn(&format!(
                        "the conflict read back to state {hash:016x}, the save had {saved_hash:016x}"
                    ));
                }
                log_info(&format!(
                    "conflict restored: {} factions, {} feuds, {} parties, {} events booked, state {:016x}",
                    engine.factions.len(),
                    engine.feuds.len(),
                    engine.parties.len(),
                    engine.pending_events(),
                    hash
                ));
                let (structures, pollution) = own_names(&engine.cultures, &engine.tun);
                let mut l = Link::with_engine(engine, structures, pollution);
                l.restore_diverged = diverged;
                l.last_page = Some(b);
                Ok(l)
            }
            Err(e) => when_pages_unreadable(&format!("the conflict's page: {e}")),
        }
    }

    /// Whether the conflict is in the world at all: no faction, source or party means nothing to
    /// run and nothing that touches the world.
    fn active(&self) -> bool {
        !self.engine.factions.is_empty()
            || !self.engine.sources.is_empty()
            || !self.engine.parties.is_empty()
    }
}

/// Whether the conflict runs: switched on and in the world.
pub(super) fn running(st: &State) -> bool {
    st.conflict.enabled && st.conflict.active()
}

// ---- hooks: session and latest-wins records -----------------------------------------------------

/// SESSION_START: every party the conflict thought embodied is abstract again and the plugin
/// hears every territory again (the reset runs as soon as the conflict runs, now or when switched
/// on), and Java forgot the engine's dictionary.
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    st.conflict.queue.clear();
    st.conflict.reported.clear();
    // The first record of the engine's only session: what it holds is its pages as read, so a
    // switch that is off from the start keeps exactly those (`last_page`).
    st.conflict.enabled = module_mask & MODULE_CONFLICT != 0;
    st.conflict.reset_due = true;
    reset_if_due(st);
}

/// The switch changed (MODULES, U10). Off: what the conflict holds is written once more as it is
/// now and then kept; queued commands are dropped (all per session). On: the reset is due, and
/// the areas booked meanwhile are introduced to it.
fn set_enabled(st: &mut State, on: bool) {
    let was = st.conflict.enabled;
    st.conflict.enabled = on;
    if was && !on {
        let _ = st.conflict.engine.take_commands();
        st.conflict.queue.clear();
        if st.conflict.active() {
            st.conflict.last_page = Some(encode_page(&st.conflict.engine));
        }
    }
    if on && !was {
        st.conflict.reset_due = true;
    }
    reset_if_due(st);
    if on && !was && running(st) {
        shared::introduce_conflict(st);
    }
}

/// Bodies of an earlier session (or of before a switch-off) are gone: every party is abstract
/// again, and TERRITORY goes out again for every faction.
fn reset_if_due(st: &mut State) {
    if st.conflict.reset_due && running(st) {
        st.conflict.reset_due = false;
        st.conflict.engine.session_start();
        st.phase1.taint();
        take_commands(st);
    }
}

/// A record that is not journaled: MODULES and BAND_STATE. PLAYER positions go to the shared
/// interest areas (`engine/shared.rs`).
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            let on = wire::u32_at(b, obs::modules::MASK) & MODULE_CONFLICT != 0;
            set_enabled(st, on);
        }
        k if STATE_KINDS.contains(&k) => {
            if !st.conflict.enabled {
                st.conflict.ignored += 1;
                return;
            }
            // The engine reads nothing from BAND_STATE yet (a party's place is its march's
            // closed form); it is counted.
            if boundary::decode_obs(b).is_ok() {
                st.conflict.band_states += 1;
            }
        }
        _ => {}
    }
}

// ---- hooks: journaled records -------------------------------------------------------------------

/// The journaled kinds that are the conflict's (docs/boundary.md §9.1).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    journaled_kind(rec.kind)
}

/// Op ids `Replay` applies once: a chief's death and what the plugin moved (Java's, §9.2).
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    let (at, size) = match rec.kind {
        boundary::CHIEF_KILLED => (56, 64),
        boundary::TRIBUTE_PAID => (64, 72),
        boundary::GIFT_GIVEN => (48, 56),
        boundary::BAND_LOOTED => (40, 48),
        _ => return 0,
    };
    if rec.size() >= size {
        wire::u64_at(rec.bytes, at)
    } else {
        0
    }
}

/// A Java item id as the conflict's (the catalogue's good, `item_of`), if the catalogue trades
/// it (U16's rule: an item no good stands for is dropped).
fn own_item(st: &State, java: u32) -> Option<u32> {
    let name = st.world.java_dict.get(&(wire::DICT_ITEM, java))?;
    phase1::catalog_good(st, name).map(item_of)
}

/// A Java body id as the conflict's (the culture data's, `body_id`); 0 for a body no culture has.
fn own_body(st: &State, java: u32) -> u32 {
    st.world
        .java_dict
        .get(&(wire::DICT_BODY, java))
        .map_or(0, |n| body_id(&st.conflict.engine.cultures, n))
}

fn own_items(st: &State, items: &[Item]) -> Vec<Item> {
    items
        .iter()
        .filter_map(|it| own_item(st, it.item).map(|item| Item { item, ..*it }))
        .collect()
}

/// Java's ids in an observation as the conflict's own (C2-4). Dimensions stay Java's (frames).
fn translate(st: &State, o: &mut Obs) {
    match o {
        Obs::FactionFound(f) => {
            f.structure = st
                .world
                .java_dict
                .get(&(wire::DICT_STRUCTURE, f.structure))
                .and_then(|n| st.conflict.structures.get(n))
                .copied()
                .unwrap_or(0);
        }
        Obs::Pollution(p) => {
            p.ptype = st
                .world
                .java_dict
                .get(&(DICT_POLLUTION, p.ptype))
                .and_then(|n| st.conflict.pollution.get(n))
                .copied()
                .unwrap_or(0);
        }
        Obs::Dematerialized(d) => {
            for s in &mut d.survivors {
                s.0 = own_body(st, s.0);
            }
        }
        Obs::Casualties(c) => c.body = own_body(st, c.body),
        Obs::TributePaid(t) => t.items = own_items(st, &t.items),
        Obs::GiftGiven(g) => g.items = own_items(st, &g.items),
        Obs::Looted(l) => l.items = own_items(st, &l.items),
        _ => {}
    }
}

/// A conflict record `Replay` admitted. Switched off, the conflict ignores it (U10).
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    if !st.conflict.enabled {
        st.conflict.ignored += 1;
        return;
    }
    let mut o = match boundary::decode_obs(rec.bytes) {
        Ok((_, o, _)) => o,
        Err(e) => {
            log_warn(&format!(
                "a conflict record of kind {:#06x} does not decode ({e:?}); skipped",
                rec.kind
            ));
            return;
        }
    };
    translate(st, &mut o);
    if let Obs::Pollution(p) = &o
        && p.district != 0
    {
        st.conflict.reported.insert(p.district);
    }
    // Everything before it happens first: the other modules, the conflict and the world, in time
    // order.
    shared::catch_up(st, rec);
    let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
    let at = Tick(phase1::world_tick(st, gt));
    ingest(st, &o, at);
}

/// Applies one record at `at` (the modules were run to its time by the caller: the shared clock).
/// A first faction makes the conflict run in the middle of a session: the areas already booked
/// are introduced to it.
fn ingest(st: &mut State, o: &Obs, at: Tick) {
    st.conflict.records += 1;
    let was_running = running(st);
    let at = at.max(st.conflict.engine.now()).max(st.phase1.world.now());
    with_infra(st, |c, w, infra| c.ingest(w, infra, o, at));
    st.phase1.taint();
    reset_if_due(st);
    take_commands(st);
    if !was_running && running(st) {
        shared::introduce_conflict(st);
    }
}

/// Runs `f` on the conflict with the phase 1 world and the industry engine's threat interface.
fn with_infra<R>(
    st: &mut State,
    f: impl FnOnce(
        &mut Conflict,
        &mut rtsim_world::World,
        &mut dyn rtsim_ecology::infra::Infrastructure,
    ) -> R,
) -> R {
    let (c, w, index) = (
        &mut st.conflict.engine,
        &mut st.phase1.world,
        &mut st.conflict.index,
    );
    if st.industry.enabled() {
        let mut infra = IndustryLink {
            engine: &mut st.industry.engine,
            index,
        };
        f(c, w, &mut infra)
    } else {
        f(c, w, &mut NoInfrastructure)
    }
}

// ---- simulate -----------------------------------------------------------------------------------

/// One round of the shared clock (`shared::run_to`): the conflict's events up to world tick `t`.
pub(super) fn run_engine(st: &mut State, t: Tick) {
    if !running(st) {
        return;
    }
    // Never behind its own present: an event due before it (none should be) runs now rather than
    // holding the shared clock's rounds at its time.
    let t = t.max(st.conflict.engine.now()).max(st.phase1.world.now());
    let before = st.conflict.engine.stats.events;
    with_infra(st, |c, w, infra| c.run_until(w, infra, t));
    if st.conflict.engine.stats.events != before {
        st.phase1.taint();
    }
    take_commands(st);
}

/// The step's simulate point, before phase 1's: the conflict runs to the hand-over's game time
/// with the other modules on the shared clock, then hears what the industry engine's districts
/// emit and drain now (C2-7, C2-8).
pub(super) fn simulate(st: &mut State) {
    if !running(st) {
        return;
    }
    reset_if_due(st);
    let at = Tick(phase1::world_tick(st, st.game_time));
    shared::run_to(st, at);
    feed_districts(st, at);
}

/// The settlers' continuous sources from the industry engine's districts (P14: a district's
/// emissions are unowned), keyed by the district's id as a POLLUTION record's are (K2-9):
///
/// - **Pollution** (C2-7): the learned rate of each deposit good whose registry id is a Destroy
///   pollution type (the Destroy plugin's attribution, I14, written as `d/0/destroy:smog/<hash>`
///   and learned as an output), scaled by the tunables' table, while the district runs: loaded
///   (unless a conflict plugin reports its POLLUTION itself), or unloaded with its replay running.
///   A Damaged district (a machine lost to a raid) emits nothing.
/// - **Mining** (C2-8): what an unloaded district's replay drains of every other deposit (an Ore
///   Excavation vein, chunk oil), as docs/boundary.md §9.3 says ore drained while unloaded reaches
///   the conflict; while loaded the plugin counts mining itself (TERRITORY_SIGNAL).
///
/// A source changes only when its rate does (compared exactly with the rate the conflict keeps),
/// so a steady district costs no event, and a restart derives the same sources from the same
/// pages.
fn feed_districts(st: &mut State, at: Tick) {
    if st.conflict.engine.factions.is_empty() || !st.industry.enabled() {
        return;
    }
    let mut changes: Vec<(u64, SignalKind, Pos, f64)> = Vec::new();
    {
        let ie = &st.industry.engine;
        let tun = &st.conflict.engine.tun;
        let season = ie.season_at(ie.now());
        for d in ie.districts() {
            if d.chunks.is_empty() {
                continue;
            }
            let live = d.presence == Presence::Live;
            let runs = d.phase != DistrictPhase::Damaged
                && (live || ie.sim_run(d.id) == Some(Run::Running));
            let mut per_day = [0.0f64; FED_KINDS.len()];
            if runs && let Some(m) = d.models.get(&season) {
                for &(g, rate) in &m.learned.rates {
                    let Some(info) = ie.goods().get(&g) else {
                        continue;
                    };
                    if info.class != GoodClass::Deposit {
                        continue;
                    }
                    let kind = info.name.split('/').next().unwrap_or("");
                    if let Some((signal, scale)) = tun.pollution_signal(kind) {
                        let plugin_owns = live && st.conflict.reported.contains(&d.id.0);
                        if rate > 0.0 && !plugin_owns {
                            let k = FED_KINDS.iter().position(|f| *f == signal).unwrap_or(0);
                            per_day[k] += rate * scale * TICKS_PER_DAY as f64;
                        }
                    } else if rate < 0.0 && !live {
                        let scale = DEPOSIT_MINING_SCALE
                            .iter()
                            .find(|(n, _)| *n == kind)
                            .map_or(1.0, |e| e.1);
                        per_day[2] += -rate * scale * TICKS_PER_DAY as f64;
                    }
                }
            }
            let (sx, sz) = d.chunks.iter().fold((0i64, 0i64), |a, c| {
                (a.0 + i64::from(c.0), a.1 + i64::from(c.1))
            });
            let k = d.chunks.len() as i64;
            let pos = Pos::new(
                d.dim as u16,
                (sx * 16 / k + 8) as i32,
                (sz * 16 / k + 8) as i32,
            );
            for (i, kind) in FED_KINDS.iter().enumerate() {
                // The rate as `Conflict::set_emission` keeps it, so equal means unchanged.
                let want = per_day[i].max(0.0) / TICKS_PER_DAY as f64;
                let have = st
                    .conflict
                    .engine
                    .sources
                    .get(&(d.id.0, *kind))
                    .map_or(0.0, |s| s.rate);
                if want != have {
                    changes.push((d.id.0, *kind, pos, per_day[i]));
                }
            }
        }
    }
    if changes.is_empty() {
        return;
    }
    for (district, kind, pos, per_day) in changes {
        st.conflict.fed += 1;
        let (c, w) = (&mut st.conflict.engine, &mut st.phase1.world);
        let at = at.max(c.now());
        c.set_emission(
            w,
            district,
            kind,
            pos,
            Some(DistrictId(district)),
            per_day,
            at,
        );
    }
    st.phase1.taint();
    take_commands(st);
}

/// Takes what the conflict decided into the link's queue.
pub(super) fn take_commands(st: &mut State) {
    let cmds = st.conflict.engine.take_commands();
    st.conflict
        .queue
        .extend(cmds.into_iter().map(|cmd| Queued { cmd, names: None }));
}

// ---- orders (TEST) ------------------------------------------------------------------------------

/// A conflict command a test ordered (`Order::ConflictCommand`, a future plugin's GameTests): it
/// joins the queue as if the conflict had decided it, so it takes its sequences and DICT records.
/// Its body and item ids are Java's, named by `names`; its dimension is Java's id, the conflict's
/// own frame; its ticks are game time. The conflict itself knows nothing of it: records Java sends
/// back about an agent it does not have are ignored.
pub(super) fn order_command(st: &mut State, names: &[(u32, u32, String)], record: &[u8]) {
    match boundary::decode_cmd(record) {
        Ok((_, cmd, _)) => {
            let names = names
                .iter()
                .map(|(k, id, n)| ((*k, *id), n.clone()))
                .collect();
            st.conflict.queue.push_back(Queued {
                cmd,
                names: Some(names),
            });
        }
        Err(e) => log_warn(&format!(
            "a conflict command order does not decode ({e:?}); dropped"
        )),
    }
}

// ---- commands -----------------------------------------------------------------------------------

/// The name of an id a queued command carries: a test's own names first, else the culture data's
/// bodies and the catalogue's goods. Dimensions are Java's ids, named by Java's dictionary.
fn name_of(st: &State, q: &Queued, kind: u32, id: u32) -> Option<String> {
    if kind == wire::DICT_DIMENSION {
        return st.world.java_dict.get(&(kind, id)).cloned();
    }
    if let Some(n) = &q.names {
        return n.get(&(kind, id)).cloned();
    }
    match kind {
        wire::DICT_BODY => body_names(&st.conflict.engine.cultures)
            .get((id as usize).wrapping_sub(1))
            .cloned(),
        wire::DICT_ITEM => id
            .checked_sub(1)
            .and_then(|g| u16::try_from(g).ok())
            .and_then(|g| phase1::catalog_item(st, GoodId(g)))
            .map(str::to_string),
        _ => None,
    }
}

/// (dictionary kind, the id a command carries, its name).
type Named = (u32, u32, String);

/// The names a queued command needs in the engine's dictionary, or why it cannot be written and
/// the SPAWN_REJECTED reason Java would have given.
fn names_needed(st: &State, q: &Queued) -> Result<Vec<Named>, (String, u32)> {
    let mut out = Vec::new();
    let mut need = |kind: u32, id: u32| -> Result<(), (String, u32)> {
        match name_of(st, q, kind, id) {
            Some(n) => {
                out.push((kind, id, n));
                Ok(())
            }
            None if kind == wire::DICT_DIMENSION => Err((
                format!("dimension {id}, which Java never named"),
                boundary_reason::DIMENSION,
            )),
            None => Err((
                format!("no name for id {id} of dictionary {kind}"),
                boundary_reason::BODY,
            )),
        }
    };
    match &q.cmd {
        Cmd::Territory(t) => need(wire::DICT_DIMENSION, t.dimension)?,
        Cmd::Notice(n) => need(wire::DICT_DIMENSION, n.dimension)?,
        Cmd::Envoy(e) => {
            need(wire::DICT_DIMENSION, e.dimension)?;
            for (body, _) in &e.members {
                need(wire::DICT_BODY, *body)?;
            }
            if e.tribute_item != 0 {
                need(wire::DICT_ITEM, e.tribute_item)?;
            }
        }
        Cmd::Materialize(m) => {
            need(wire::DICT_DIMENSION, m.dimension)?;
            for s in &m.members {
                need(wire::DICT_BODY, s.body)?;
            }
        }
        Cmd::Steer(_) | Cmd::Recall(_) | Cmd::Settled(_) => {}
    }
    Ok(out)
}

/// docs/boundary.md §2.3's SPAWN_REJECTED codes the engine answers for Java.
mod boundary_reason {
    /// The dimension is not loaded (here: Java never named it).
    pub const DIMENSION: u32 = 3;
    /// The record cannot fit a command buffer.
    pub const TOO_BIG: u32 = 6;
    /// An unknown body kind.
    pub const BODY: u32 = 9;
}

/// What Java would have answered a materialization the engine cannot write: the party must not
/// wait for a body that never comes.
fn reject(st: &mut State, cmd: &Cmd, reason: u32) {
    let (agent, epoch, wave) = match cmd {
        Cmd::Envoy(e) => (e.agent, e.epoch, 0),
        Cmd::Materialize(m) => (m.agent, m.epoch, u32::from(m.wave)),
        _ => return,
    };
    let o = Obs::SpawnRejected(boundary::BandSpawnRejected {
        agent,
        epoch,
        wave,
        reason,
        cmd_seq: 0,
    });
    let at = Tick(phase1::world_tick(st, st.game_time));
    ingest(st, &o, at);
}

/// Writes the queued conflict commands while they fit; each takes the next sequence. Returns the
/// highest sequence written and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    if !st.conflict.enabled {
        st.conflict.queue.clear();
        return (0, false);
    }
    let mut highest = 0;
    while let Some(q) = st.conflict.queue.front().cloned() {
        let names = match names_needed(st, &q) {
            Ok(n) => n,
            Err((e, reason)) => {
                log_error(&format!(
                    "a conflict {} names what the engine cannot: {e}; dropped",
                    q.cmd.name()
                ));
                st.conflict.queue.pop_front();
                reject(st, &q.cmd, reason);
                continue;
            }
        };
        let mut cmd = q.cmd.clone();
        // The conflict's ticks are the phase 1 world's (game time plus a test's P1_ADVANCE);
        // Java's are game time. A test's order is in game time already.
        if q.names.is_none() {
            let off = phase1::world_tick(st, 0) as i64;
            match &mut cmd {
                Cmd::Envoy(e) => e.deadline = (e.deadline - off).max(0),
                Cmd::Steer(s) => s.arrive = (s.arrive - off).max(0),
                _ => {}
            }
        }
        let mut probe = Vec::new();
        boundary::encode_cmd(0, &cmd, &mut probe);
        let refs: Vec<(u32, &str)> = names.iter().map(|(k, _, n)| (*k, n.as_str())).collect();
        let need = st.dict_bytes(&refs) + probe.len();
        if need > w.capacity() || probe.len() > wire::MAX_RECORD {
            log_error(&format!(
                "a conflict command of {need} bytes can never fit a command buffer; dropped"
            ));
            st.conflict.queue.pop_front();
            reject(st, &cmd, boundary_reason::TOO_BIG);
            continue;
        }
        if need > w.remaining() {
            return (highest, true);
        }
        // The engine's ids, DICT records first (each name once per session).
        let mut ids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for (kind, id, n) in &names {
            let e = st.dict_id(w, *kind, n);
            ids.insert((*kind, *id), e);
        }
        let id = |kind: u32, v: u32| ids.get(&(kind, v)).copied().unwrap_or(0);
        match &mut cmd {
            Cmd::Territory(t) => t.dimension = id(wire::DICT_DIMENSION, t.dimension),
            Cmd::Notice(n) => n.dimension = id(wire::DICT_DIMENSION, n.dimension),
            Cmd::Envoy(e) => {
                e.dimension = id(wire::DICT_DIMENSION, e.dimension);
                for (body, _) in &mut e.members {
                    *body = id(wire::DICT_BODY, *body);
                }
                if e.tribute_item != 0 {
                    e.tribute_item = id(wire::DICT_ITEM, e.tribute_item);
                }
            }
            Cmd::Materialize(m) => {
                m.dimension = id(wire::DICT_DIMENSION, m.dimension);
                for s in &mut m.members {
                    s.body = id(wire::DICT_BODY, s.body);
                }
            }
            Cmd::Steer(_) | Cmd::Recall(_) | Cmd::Settled(_) => {}
        }
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(probe.len());
        boundary::encode_cmd(seq, &cmd, &mut rec);
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.conflict.commands_written += 1;
        st.conflict.queue.pop_front();
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

fn encode_page(c: &Conflict) -> Vec<u8> {
    let body = c.save();
    let mut w = Writer::with_capacity(body.len() + 16);
    w.u32(MAGIC);
    w.u16(VERSION);
    w.u64(c.state_hash());
    w.raw(&body);
    w.into_bytes()
}

/// The conflict's page, taken at this step boundary with the phase 1 world's snapshot. Commands
/// the conflict decided and the link has not taken go to the queue first. Switched off, the page
/// it had (U10).
pub(super) fn snapshot(st: &mut State) -> Vec<(PageKey, Vec<u8>)> {
    if !st.conflict.enabled {
        return st
            .conflict
            .last_page
            .clone()
            .map(|b| vec![(KEY_CONFLICT, b)])
            .unwrap_or_default();
    }
    take_commands(st);
    if !st.conflict.active() && !st.conflict.dropped_pages {
        st.conflict.last_page = None;
        return Vec::new();
    }
    st.conflict.dropped_pages = false;
    let page = encode_page(&st.conflict.engine);
    st.conflict.last_page = Some(page.clone());
    vec![(KEY_CONFLICT, page)]
}

/// Conflict pages a save no longer writes (dropped pages, or no conflict any more).
pub(super) fn deletions(store: &PageStore, pages: &[(PageKey, Vec<u8>)]) -> Vec<PageKey> {
    store
        .keys()
        .filter(|k| k.table() == TABLE && !pages.iter().any(|(p, _)| p == k))
        .collect()
}

// ---- inspection ---------------------------------------------------------------------------------

/// The link's counters ([`super::Engine::conflict_stats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConflictLinkStats {
    pub records: u64,
    pub ignored: u64,
    pub commands_written: u64,
    pub band_states: u64,
    /// Sources the industry engine's districts set or changed.
    pub fed: u64,
}

impl super::Engine {
    /// Runs `f` on the conflict and the phase 1 world it lives in, under the state lock. For
    /// tests and debugging, between steps only: a step holds the lock while it runs.
    pub fn with_conflict<R>(&self, f: impl FnOnce(&Conflict, &rtsim_world::World) -> R) -> R {
        let st = super::lock(&self.state);
        f(&st.conflict.engine, &st.phase1.world)
    }

    /// Keeps (or stops keeping) the conflict's log of what it decided, in memory and unsaved
    /// (tests and `/rtsim`); returns the lines kept so far, oldest first, and clears them.
    pub fn conflict_log(&self, keep: bool) -> Vec<rtsim_conflict::engine::LogEntry> {
        let mut st = super::lock(&self.state);
        st.conflict.engine.log_enabled = keep;
        std::mem::take(&mut st.conflict.engine.log)
    }

    /// Whether the last start read the conflict back to the state hash it was saved with
    /// (`None`), or (saved, read); and whether it dropped its pages.
    pub fn conflict_restore_check(&self) -> (Option<(u64, u64)>, bool) {
        let st = super::lock(&self.state);
        (st.conflict.restore_diverged, st.conflict.dropped_pages)
    }

    /// The conflict's counters and the link's.
    pub fn conflict_stats(&self) -> (ConflictStats, ConflictLinkStats) {
        let st = super::lock(&self.state);
        let l = &st.conflict;
        (
            l.engine.stats,
            ConflictLinkStats {
                records: l.records,
                ignored: l.ignored,
                commands_written: l.commands_written,
                band_states: l.band_states,
                fed: l.fed,
            },
        )
    }
}
