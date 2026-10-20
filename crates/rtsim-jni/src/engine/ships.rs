// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships behind the boundary (docs/boundary.md §7; choices U1 to U18 in
//! docs/overnight-choices-integrate2.md): `rtsim-ships`' engine fed by a ships plugin's records,
//! its commands written into the command buffers, its pages in the store. No plugin exists yet
//! (P8): the Rust fake Java side in `tests/common` (`Java::ship_obs`, `ship_command`) plays one,
//! after `rtsim_ships::synth::FakeShips`.
//!
//! The step calls this module at its observe, simulate, write and save points, beside the dragons
//! (`engine/dragons.rs`), whom it mirrors:
//!
//! - **Observations** ([`observe_state`], [`journaled`]): SHIP_STATE is latest state; the
//!   journaled kinds 0x0600 to 0x06FF go through the kernel's `Replay` (CARGO_LOOTED, SHIP_SUNK
//!   and SHIP_CAPTURED by their op ids too) into `Ships::ingest`, at their own time with the
//!   dragons and the world run to it first (the shared clock, `engine/shared.rs`). Items travel
//!   in Java's dictionary and are the phase 1 catalogue's goods here; dimensions are Java's ids
//!   (the ships' frames). PORT_FOUND names an adopted village; HAVEN_FOUND joins its haven to
//!   the world as a haven node when the world has none of that key.
//! - **Simulate** ([`simulate`]): the ships run to the hand-over's time on the shared clock, and
//!   the network is built once two docks of one medium share a dimension (U12); players' areas
//!   are the engine's shared ones (`engine/shared.rs`).
//! - **Commands** ([`write_queue`]): SHIP_MATERIALIZE, TIMETABLE, STEER, DEPART, CARGO and
//!   DESPAWN, with sequences from the core's counter after every other module's: all per session
//!   (U4, as the dragons' S11), so nothing is kept or resent, and they always follow the resends
//!   in sequence order. Body kinds, the dimension and cargo items go through the engine's
//!   dictionary. A SHIP_CARGO's op id comes from its identity: the engine's own from the ship,
//!   epoch and voyage (U18), a test order's from the ship, epoch and hold (U5); allocator 0x53.
//! - **Pages** ([`Link::load`], [`snapshot`]): table 0x5300, one page with `Ships::save` and its
//!   state hash. The ships live in the phase 1 world (lanes are its edges, voyages its legs), so
//!   while they are active the phase 1 link snapshots at every save, in the same commit.
//! - **The switch** (C25, U10): `Module.SHIPS` (bit 6). Off, the ships ignore their records,
//!   run nothing, write nothing and keep the page they had.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_ecology::infra::{DistrictIndex, IndustryLink, NoInfrastructure};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Writer};
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_ships::boundary::{self, Cmd, Item, Obs};
use rtsim_ships::engine::{body_names, good_of, item_of};
use rtsim_ships::water::HabitatWater;
use rtsim_ships::{ShipStats, ShipTunables, Ships};
use rtsim_world::NodeData;

use super::{State, log_error, log_info, log_warn, phase1, shared};
use crate::wire::{self, Rec, RecordWriter, obs};

/// The ships' pages: table 0x5300 ('S'), next to the dragons' 0x4400 and phase 1's 0x5000.
pub const TABLE: u16 = 0x5300;
pub const KEY_SHIPS: PageKey = PageKey::new(TABLE, 0);
const MAGIC: u32 = u32::from_le_bytes(*b"RTSH");
const VERSION: u16 = 1;

/// Java's module bit the ships run under (`config/Module.java`: SHIPS is ordinal 6).
pub const MODULE_SHIPS: u32 = 1 << 6;

/// The ships' seed. PLACEHOLDER: the bridge does not send the Minecraft world's (as phase 1's).
pub const SHIP_SEED: u64 = 0x6d69_635f_7368_6970;

/// A haven's population when it joins the world as a node (PLACEHOLDER, the ship sweep's haven).
const HAVEN_POP: u32 = 4;

/// The water mask: the world model's (X2-17, replacing U11's all-water mask): a 32-block cell is
/// water when every sample point in it is known open water; a frame the world model knows
/// nothing of is all water, as before (X2-6).
fn mask(st: &State) -> HabitatWater {
    HabitatWater::new(st.worldmodel.world.clone())
}

/// Op ids of the engine's SHIP_CARGO: allocator 0x53 in bits 56 to 62 (docs/boundary.md §2.2).
const OP_ALLOCATOR: u64 = 0x53 << 56;
const SYS_OP: SystemId = SystemId::named("ships.op");

/// The ship kinds of docs/boundary.md §7.1.
fn journaled_kind(k: u16) -> bool {
    (0x0600..=0x06FF).contains(&k)
}

const STATE_KINDS: std::ops::RangeInclusive<u16> = 0x0070..=0x007F;

/// A command waiting for the buffer, with the names of the ids it carries when they are not the
/// ships' own (a test's order names Java's).
#[derive(Clone, Debug)]
struct Queued {
    cmd: Cmd,
    /// (dictionary kind, id) to name; `None`: the ships' bodies and the catalogue's goods.
    names: Option<BTreeMap<(u32, u32), String>>,
}

/// The ships' state the step keeps (`State::ships`).
pub(super) struct Link {
    pub(super) engine: Ships,
    /// Districts by cell for the industry engine's threat interface (not saved).
    pub(super) index: DistrictIndex,
    /// Commands not written yet (per session).
    queue: VecDeque<Queued>,
    enabled: bool,
    /// The ships' session reset (bodies of an earlier session or of before a switch-off are
    /// gone) is due the next time they run.
    reset_due: bool,
    /// The page as last written or read: what a save keeps while the ships are switched off.
    last_page: Option<Vec<u8>>,
    /// Java item ids already mapped to world goods (`None`: not a good the catalogue trades).
    java_items: BTreeMap<u32, Option<u32>>,
    /// The last load dropped pages it could not read ([`when_pages_unreadable`]).
    pub(super) dropped_pages: bool,
    /// The last load read back ships whose state hash is not the one saved: (saved, read).
    pub(super) restore_diverged: Option<(u64, u64)>,
    pub(super) records: u64,
    /// Records ignored: the ships were switched off, or the record named what the world lacks.
    pub(super) ignored: u64,
    pub(super) commands_written: u64,
}

fn new_engine() -> Ships {
    Ships::new(SHIP_SEED, ShipTunables::default(), Tick::ZERO)
}

/// **The policy for ship pages this build cannot read** (as the dragons' S12 and phase 1's S5):
/// dropped with a loud log, the ships start afresh (the plugin reports ports and havens again as
/// their chunks load), and the next save replaces them.
fn when_pages_unreadable(why: &str) -> Result<Link, String> {
    log_error(&format!(
        "SHIP PAGES DROPPED: {why}. The ships start afresh (ports and havens are reported again as their chunks load) and the next save replaces them"
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
            reset_due: false,
            last_page: None,
            java_items: BTreeMap::new(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            ignored: 0,
            commands_written: 0,
        }
    }

    /// Loads the ships' page. `world_restored` says the phase 1 world came back from its pages:
    /// the ships live in that world (its lane edges, legs and knowledge), so without it their
    /// page is dropped too.
    pub(super) fn load(store: &mut PageStore, world_restored: bool) -> Result<Link, String> {
        let page = match store.get(KEY_SHIPS) {
            Ok(p) => p,
            Err(e) => return when_pages_unreadable(&format!("cannot read the ships' page: {e}")),
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
                return Err(CodecError::Invalid("not a ships page".into()));
            }
            let v = r.u16()?;
            if v != VERSION {
                return Err(CodecError::Invalid(format!(
                    "ships page version {v}, this build reads {VERSION}"
                )));
            }
            Ok(())
        })();
        if let Err(e) = head {
            return when_pages_unreadable(&format!("the ships' page: {e}"));
        }
        let Ok(saved_hash) = r.u64() else {
            return when_pages_unreadable("the ships' page is cut short");
        };
        let rest = r.take(r.remaining()).unwrap_or_default();
        match Ships::load(ShipTunables::default(), rest) {
            Ok(engine) => {
                let hash = engine.state_hash();
                let diverged = (hash != saved_hash).then_some((saved_hash, hash));
                if diverged.is_some() {
                    log_warn(&format!(
                        "the ships read back to state {hash:016x}, the save had {saved_hash:016x}"
                    ));
                }
                log_info(&format!(
                    "ships restored: {} docks, {} lanes, {} ships, {} havens, {} events booked, state {:016x}",
                    engine.docks.len(),
                    engine.lanes.len(),
                    engine.ships.len(),
                    engine.havens.len(),
                    engine.pending_events(),
                    hash
                ));
                let mut l = Link::empty();
                l.engine = engine;
                l.restore_diverged = diverged;
                l.last_page = Some(b);
                Ok(l)
            }
            Err(e) => when_pages_unreadable(&format!("the ships' page: {e}")),
        }
    }

    /// Whether the ships are in the world at all: no dock, no haven and no ship means nothing to
    /// run and nothing that touches the world.
    fn active(&self) -> bool {
        !self.engine.docks.is_empty()
            || !self.engine.havens.is_empty()
            || !self.engine.ships.is_empty()
    }
}

/// Whether the ships run: switched on and in the world.
pub(super) fn running(st: &State) -> bool {
    st.ships.enabled && st.ships.active()
}

// ---- dictionaries ---------------------------------------------------------------------------------

/// A Java item id as the ships' item (the world good's index plus 1), if the catalogue trades it.
fn ship_item(st: &mut State, java: u32) -> Option<u32> {
    if let Some(v) = st.ships.java_items.get(&java) {
        return *v;
    }
    let mapped = st
        .world
        .java_dict
        .get(&(wire::DICT_ITEM, java))
        .and_then(|name| phase1::catalog_good(st, name))
        .map(item_of);
    if st.world.java_dict.contains_key(&(wire::DICT_ITEM, java)) {
        st.ships.java_items.insert(java, mapped);
    }
    mapped
}

/// The name of an id a queued command carries: a test's own names first, else the ships' bodies
/// and the catalogue's goods.
fn name_of(st: &State, q: &Queued, kind: u32, id: u32) -> Option<String> {
    if let Some(n) = &q.names {
        return n.get(&(kind, id)).cloned();
    }
    match kind {
        wire::DICT_BODY => body_names(&st.ships.engine.tun)
            .get((id as usize).wrapping_sub(1))
            .map(|s| s.to_string()),
        wire::DICT_ITEM => good_of(id)
            .and_then(|g| phase1::catalog_item(st, g))
            .map(str::to_string),
        _ => None,
    }
}

// ---- hooks: session and latest-wins records -----------------------------------------------------

/// SESSION_START: Java forgot the engine's dictionary; the ships' bodies are gone (their reset
/// runs as soon as the ships run, now or when switched on).
pub(super) fn session_start(st: &mut State, module_mask: u32) {
    st.ships.queue.clear();
    st.ships.java_items.clear();
    // The first record of the engine's only session: what it holds is its pages as read, so a
    // switch that is off from the start keeps exactly those (`last_page`).
    st.ships.enabled = module_mask & MODULE_SHIPS != 0;
    st.ships.reset_due = true;
    reset_if_due(st);
}

/// The switch changed (MODULES, U10). Off: what the ships hold is written once more as it is now and then kept;
/// queued commands are dropped (all per session). On: the reset is due.
fn set_enabled(st: &mut State, on: bool) {
    let was = st.ships.enabled;
    st.ships.enabled = on;
    if was && !on {
        // Nothing decided before the switch goes out after it: bodies of before are gone when it
        // comes back on.
        let _ = st.ships.engine.take_commands();
        st.ships.queue.clear();
        if st.ships.active() {
            st.ships.last_page = Some(encode_page(&st.ships.engine));
        }
    }
    if on && !was {
        st.ships.reset_due = true;
    }
    reset_if_due(st);
}

/// Bodies of an earlier session (or of before a switch-off) are gone: every ship is abstract.
fn reset_if_due(st: &mut State) {
    if st.ships.reset_due && running(st) {
        st.ships.reset_due = false;
        st.ships.engine.session_start();
        st.phase1.taint();
    }
}

/// A record that is not journaled: Java's DICT entries, MODULES and SHIP_STATE.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    match rec.kind {
        obs::DICT => st.ships.java_items.clear(),
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            let on = wire::u32_at(b, obs::modules::MASK) & MODULE_SHIPS != 0;
            set_enabled(st, on);
        }
        k if STATE_KINDS.contains(&k) => {
            if !st.ships.enabled {
                st.ships.ignored += 1;
                return;
            }
            if let Ok((_, _, _, o)) = boundary::decode_obs(b) {
                let at = Tick(phase1::world_tick(st, st.game_time));
                ingest(st, &o, at);
            }
        }
        _ => {}
    }
}

// ---- hooks: journaled records -----------------------------------------------------------------------

/// The journaled kinds that are the ships' (docs/boundary.md §7.1).
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    journaled_kind(rec.kind)
}

/// Op ids `Replay` applies once: what the plugin moved or destroyed (§7.3).
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    match rec.kind {
        boundary::SHIP_SUNK | boundary::SHIP_CAPTURED | boundary::CARGO_LOOTED
            if rec.size() >= 72 =>
        {
            wire::u64_at(rec.bytes, 64)
        }
        _ => 0,
    }
}

/// A ship record `Replay` admitted.
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    if !st.ships.enabled {
        st.ships.ignored += 1;
        return;
    }
    let mut o = match boundary::decode_obs(rec.bytes) {
        Ok((_, _, _, o)) => o,
        Err(e) => {
            log_warn(&format!(
                "a ship record of kind {:#06x} does not decode ({e:?}); skipped",
                rec.kind
            ));
            return;
        }
    };
    // Java's items as the ships' (the world's goods); an item no good stands for was never in a
    // hold the sim keeps.
    if let Obs::CargoLooted(c) = &mut o {
        let items = std::mem::take(&mut c.items);
        c.items = items
            .into_iter()
            .filter_map(|it| ship_item(st, it.item).map(|item| Item { item, ..it }))
            .collect();
    }
    let gt = wire::i64_at(rec.bytes, obs::GAME_TIME);
    let at = Tick(phase1::world_tick(st, gt));
    // Everything before it happens first: the dragons, the ships and the world, in time order.
    shared::run_to(st, at);
    match &o {
        Obs::PortFound(p) if st.phase1.world.node_of_key(p.village).is_none() => {
            // A port names an adopted village (VILLAGE_ADOPTED comes at its structure start,
            // before any piece of it exists): one the world lacks is not a port of the sim (U13).
            log_warn(&format!(
                "a port of village {:#x}, which the world does not have; ignored",
                p.village
            ));
            st.ships.ignored += 1;
            return;
        }
        Obs::HavenFound(h) if st.phase1.world.node_of_key(h.haven).is_none() => {
            // A haven is a world node whose market sells what its pirates take (C24): it joins
            // the world here (U13), outside the phase 1 log, so the world is tainted below.
            let at = at.max(st.phase1.world.now());
            st.phase1.world.join(
                NodeData {
                    key: h.haven,
                    kind: NodeKind::Haven(Culture::Human),
                    pos: Pos::new(h.dimension as u16, h.dock.0, h.dock.2),
                    pop: HAVEN_POP,
                    resources: BTreeMap::new(),
                },
                at,
            );
        }
        _ => {}
    }
    ingest(st, &o, at);
}

fn ingest(st: &mut State, o: &Obs, at: Tick) {
    st.ships.records += 1;
    let at = at.max(st.ships.engine.now()).max(st.phase1.world.now());
    run_engine_at(st, at);
    let m = mask(st);
    let (w, s) = (&mut st.phase1.world, &mut st.ships.engine);
    s.ingest(w, o, &m, at);
    st.phase1.taint();
    reset_if_due(st);
    take_commands(st);
}

/// Runs `f` on the ships with the phase 1 world and the industry engine's threat interface.
fn with_infra<R>(
    st: &mut State,
    f: impl FnOnce(
        &mut Ships,
        &mut rtsim_world::World,
        &mut dyn rtsim_ecology::infra::Infrastructure,
    ) -> R,
) -> R {
    let (s, w, index) = (
        &mut st.ships.engine,
        &mut st.phase1.world,
        &mut st.ships.index,
    );
    if st.industry.enabled() {
        let mut infra = IndustryLink {
            engine: &mut st.industry.engine,
            index,
        };
        f(s, w, &mut infra)
    } else {
        f(s, w, &mut NoInfrastructure)
    }
}

// ---- simulate -----------------------------------------------------------------------------------

/// One round of the shared clock (`shared::run_to`): the ships' events up to world tick `t`.
pub(super) fn run_engine(st: &mut State, t: Tick) {
    if !running(st) || t < st.ships.engine.now() {
        return;
    }
    run_engine_at(st, t);
}

fn run_engine_at(st: &mut State, t: Tick) {
    let t = t.max(st.phase1.world.now());
    with_infra(st, |s, w, infra| s.run_until(w, infra, t));
    st.phase1.taint();
    take_commands(st);
}

/// The step's simulate point, before phase 1's: the ships (with the dragons) run to the
/// hand-over's time, and the network is built once it can have a lane (U12).
pub(super) fn simulate(st: &mut State) {
    if !running(st) {
        return;
    }
    reset_if_due(st);
    let at = Tick(phase1::world_tick(st, st.game_time));
    shared::run_to(st, at);
    if !st.ships.engine.is_built() {
        let mut groups: BTreeSet<(u16, rtsim_travel::Medium)> = BTreeSet::new();
        let mut buildable = false;
        for d in &st.ships.engine.docks {
            buildable |= !groups.insert((d.frame, d.medium));
        }
        if buildable {
            let at = at.max(st.phase1.world.now());
            let m = mask(st);
            let (w, s) = (&mut st.phase1.world, &mut st.ships.engine);
            s.build(w, &m, at);
            log_info(&format!(
                "ships: the network is built from {} docks: {} lanes, {} services, {} ships",
                s.docks.len(),
                s.lanes.len(),
                s.services.len(),
                s.ships.len()
            ));
            st.phase1.taint();
            take_commands(st);
        }
    }
}

fn take_commands(st: &mut State) {
    let cmds = st.ships.engine.take_commands();
    st.ships
        .queue
        .extend(cmds.into_iter().map(|cmd| Queued { cmd, names: None }));
}

// ---- commands -----------------------------------------------------------------------------------

/// A ship command a test ordered (`Order::ShipCommand`, a future plugin's GameTests): it joins the
/// queue as if the ships had decided it, so it takes their sequences and DICT records. Its body
/// and item ids are Java's, named by `names` (so it never waits for Java's DICT records); its
/// dimension is Java's id, the ships' own frame. The ships themselves know nothing of it: records
/// Java sends back about an agent they do not have are ignored.
pub(super) fn order_command(st: &mut State, names: &[(u32, u32, String)], record: &[u8]) {
    match boundary::decode_cmd(record) {
        Ok((_, _, cmd)) => {
            let names = names
                .iter()
                .map(|(k, id, n)| ((*k, *id), n.clone()))
                .collect();
            st.ships.queue.push_back(Queued {
                cmd,
                names: Some(names),
            });
        }
        Err(e) => log_warn(&format!(
            "a ship command order does not decode ({e:?}); dropped"
        )),
    }
}

/// The op id of a test's SHIP_CARGO ordered with op 0, from what it is (U5; the engine's own carry
/// theirs, `Ships::cargo_op`, U18): the ship's agent and epoch and the
/// hold it sets, so the same command derived again after a crash has the same id and another
/// command never has an id an earlier one had.
fn cargo_op(agent: u64, epoch: u32, items: &[Item]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for it in items {
        for v in [u64::from(it.item), it.count as u32 as u64] {
            h = (h ^ v).wrapping_mul(0x0100_0000_01b3);
        }
    }
    let x = StreamKey::new(SHIP_SEED, SYS_OP, agent).block(u64::from(epoch), h)[0];
    let low = x & ((1 << 56) - 1);
    OP_ALLOCATOR | if low == 0 { 1 } else { low }
}

/// The names a queued command needs in the engine's dictionary: (kind, id it carries, name).
fn names_needed(st: &State, q: &Queued) -> Result<Vec<(u32, u32, String)>, String> {
    let mut out = Vec::new();
    let mut need = |kind: u32, id: u32| -> Result<(), String> {
        match name_of(st, q, kind, id) {
            Some(n) => {
                out.push((kind, id, n));
                Ok(())
            }
            None => Err(format!("no name for id {id} of dictionary {kind}")),
        }
    };
    match &q.cmd {
        Cmd::Materialize(m) => {
            need(wire::DICT_BODY, m.body)?;
            for mb in &m.members {
                need(wire::DICT_BODY, mb.body)?;
            }
            for it in &m.cargo {
                need(wire::DICT_ITEM, it.item)?;
            }
        }
        Cmd::SetCargo(c) => {
            for it in &c.items {
                need(wire::DICT_ITEM, it.item)?;
            }
        }
        _ => {}
    }
    Ok(out)
}

/// What Java would have answered a materialization the engine cannot write: the ship must not
/// wait for a body that never comes.
fn reject(st: &mut State, agent: u64, epoch: u32, reason: u32) {
    let o = Obs::SpawnRejected(boundary::SpawnRejected {
        agent,
        epoch,
        reason,
        cmd_seq: 0,
    });
    let at = Tick(phase1::world_tick(st, st.game_time));
    ingest(st, &o, at);
}

/// Writes the queued ship commands while they fit; each takes the next sequence. Returns the
/// highest sequence written and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    if !st.ships.enabled {
        st.ships.queue.clear();
        return (0, false);
    }
    let mut highest = 0;
    while let Some(q) = st.ships.queue.front().cloned() {
        let mut names = match names_needed(st, &q) {
            Ok(n) => n,
            Err(e) => {
                log_error(&format!(
                    "a ship command names what the engine cannot: {e}; dropped"
                ));
                st.ships.queue.pop_front();
                if let Cmd::Materialize(m) = &q.cmd {
                    reject(st, m.agent, m.epoch, 9);
                }
                continue;
            }
        };
        // The dimension: Java's id is the ships' frame; Java must have named it.
        let dim_name = match &q.cmd {
            Cmd::Materialize(m) => {
                match st.world.java_dict.get(&(wire::DICT_DIMENSION, m.dimension)) {
                    Some(n) => Some(n.clone()),
                    None => {
                        log_error(&format!(
                            "a ship materialization in dimension {} that Java never named; rejected",
                            m.dimension
                        ));
                        st.ships.queue.pop_front();
                        reject(st, m.agent, m.epoch, 3);
                        continue;
                    }
                }
            }
            _ => None,
        };
        if let Some(n) = &dim_name {
            names.push((wire::DICT_DIMENSION, 0, n.clone()));
        }
        let mut cmd = q.cmd.clone();
        if let Cmd::SetCargo(c) = &mut cmd
            && c.op == 0
        {
            c.op = cargo_op(c.agent, c.epoch, &c.items);
        }
        let mut probe = Vec::new();
        boundary::encode_cmd(0, &cmd, &mut probe);
        let refs: Vec<(u32, &str)> = names.iter().map(|(k, _, n)| (*k, n.as_str())).collect();
        let need = st.dict_bytes(&refs) + probe.len();
        if need > w.capacity() || probe.len() > wire::MAX_RECORD {
            log_error(&format!(
                "a ship command of {need} bytes can never fit a command buffer; dropped"
            ));
            st.ships.queue.pop_front();
            if let Cmd::Materialize(m) = &cmd {
                reject(st, m.agent, m.epoch, 6);
            }
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
            Cmd::Materialize(m) => {
                m.body = id(wire::DICT_BODY, m.body);
                for mb in &mut m.members {
                    mb.body = id(wire::DICT_BODY, mb.body);
                }
                for it in &mut m.cargo {
                    it.item = id(wire::DICT_ITEM, it.item);
                }
                m.dimension = id(wire::DICT_DIMENSION, 0);
            }
            Cmd::SetCargo(c) => {
                for it in &mut c.items {
                    it.item = id(wire::DICT_ITEM, it.item);
                }
            }
            _ => {}
        }
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(probe.len());
        boundary::encode_cmd(seq, &cmd, &mut rec);
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.ships.commands_written += 1;
        st.ships.queue.pop_front();
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

fn encode_page(s: &Ships) -> Vec<u8> {
    let body = s.save();
    let mut w = Writer::with_capacity(body.len() + 16);
    w.u32(MAGIC);
    w.u16(VERSION);
    w.u64(s.state_hash());
    w.raw(&body);
    w.into_bytes()
}

/// The ships' page, taken at this step boundary with the phase 1 world's snapshot. Commands the
/// ships decided and the link has not taken go to the queue first. Switched off, the page they
/// had (U10).
pub(super) fn snapshot(st: &mut State) -> Vec<(PageKey, Vec<u8>)> {
    if !st.ships.enabled {
        return st
            .ships
            .last_page
            .clone()
            .map(|b| vec![(KEY_SHIPS, b)])
            .unwrap_or_default();
    }
    take_commands(st);
    if !st.ships.active() && !st.ships.dropped_pages {
        st.ships.last_page = None;
        return Vec::new();
    }
    st.ships.dropped_pages = false;
    let page = encode_page(&st.ships.engine);
    st.ships.last_page = Some(page.clone());
    vec![(KEY_SHIPS, page)]
}

/// Ship pages a save no longer writes (dropped pages, or no ships any more).
pub(super) fn deletions(store: &PageStore, pages: &[(PageKey, Vec<u8>)]) -> Vec<PageKey> {
    store
        .keys()
        .filter(|k| k.table() == TABLE && !pages.iter().any(|(p, _)| p == k))
        .collect()
}

// ---- inspection ---------------------------------------------------------------------------------

impl super::Engine {
    /// Runs `f` on the ships and the phase 1 world they live in, under the state lock. For tests
    /// and debugging, between steps only: a step holds the lock while it runs.
    pub fn with_ships<R>(&self, f: impl FnOnce(&Ships, &rtsim_world::World) -> R) -> R {
        let st = super::lock(&self.state);
        f(&st.ships.engine, &st.phase1.world)
    }

    /// Whether the last start read the ships back to the state hash they were saved with
    /// (`None`), or (saved, read); and whether it dropped their pages.
    pub fn ships_restore_check(&self) -> (Option<(u64, u64)>, bool) {
        let st = super::lock(&self.state);
        (st.ships.restore_diverged, st.ships.dropped_pages)
    }

    /// The ships' counters, and how many ship records went through the link, how many it ignored
    /// and how many commands it wrote.
    pub fn ship_stats(&self) -> (ShipStats, u64, u64, u64) {
        let st = super::lock(&self.state);
        (
            st.ships.engine.stats,
            st.ships.records,
            st.ships.ignored,
            st.ships.commands_written,
        )
    }
}
