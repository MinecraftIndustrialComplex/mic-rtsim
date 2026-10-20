// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine (`rtsim-industry`, INDUSTRY.md, C15 to C19) behind the boundary: the
//! records of docs/industry-boundary.md in and out of the bridge's buffers. Choices are W1 to W23
//! in docs/overnight-choices-industryjni.md.
//!
//! The step calls in here at five points and nowhere else:
//! - **observe** ([`observe_state`], [`session_start`], [`journaled`]): Java's DICT entries for
//!   the dictionaries industry records use, the MODULES switch, the latest-wins industry kinds
//!   (0x0040 to 0x005F) and, once `Replay` admitted them, the journaled ones (0x0400 to 0x04FF)
//!   and COMMAND_RESULTs for industry op ids, all through `rtsim_industry::wire`;
//! - **simulate** ([`simulate`]): the engine advances to the hand-over's game time and its
//!   commands move to the link's queue;
//! - **write** ([`write_resends`], [`write_queue`]): commands into the command buffer, with the
//!   bridge's sequences, the DICT records they need first, and what does not fit left for later;
//! - **save** ([`snapshot`], [`deletions`]): the engine's own pages (tables 0x4900 to 0x4902,
//!   F36) and the link's page, [`KEY_LINK`];
//! - **orders** ([`threat`]): a threat from another system (or a test) for the engine.
//!
//! **What is kept for resend.** IND_CARRIAGE_DELTA, IND_CHANNEL_DELTA, IND_DEPOSIT_DELTA and
//! IND_WRITE_PACKAGE stay in the link's outbox from the moment they are written until Java's
//! answer is admitted through `Replay` (IND_CARRIAGE_RESULT, or COMMAND_RESULT applied, duplicate
//! or failed), so a snapshot taken at observation N holds exactly the ones the world save at N
//! lacks; after a restart they are resent first. IND_CHUNK_RECONCILE is not: the industry engine
//! keeps every pending delta until its result and resends it at the chunk's next IND_CHUNK_LOADED
//! (F27). IND_LEDGER is a reply and is never kept.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::RangeInclusive;

use rtsim_industry::cmd::{Cmd, IndustryOps};
use rtsim_industry::engine::Engine as Industry;
use rtsim_industry::ids::{Good, GoodInfo};
use rtsim_industry::obs::{Obs, Timed};
use rtsim_industry::persist::{CLUSTER_TABLE, DISTRICT_TABLE, GLOBAL_TABLE};
use rtsim_industry::threat::Threat;
use rtsim_industry::tunables::Tunables;
use rtsim_industry::wire as iw;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Writer};
use rtsim_kernel::journal::OpId;
use rtsim_kernel::page::{PageKey, PageStore};

use super::{Config, Engine, State, lock, log_error, log_info, log_warn};
use crate::wire::{self, Rec, RecordWriter, cmd, obs};

/// The link's own page: its command queue and outbox. Table 1 is rtsim-jni's (J9); id 0x49 keeps
/// clear of the core's 1:0 and 1:1 and of whatever the core adds next (W6).
pub const KEY_LINK: PageKey = PageKey::new(1, 0x49);
const LINK_MAGIC: u32 = u32::from_le_bytes(*b"RTJI");
const LINK_VERSION: u16 = 1;

/// The industry engine's own page tables (rtsim-industry `persist.rs`, F36).
pub const TABLES: [u16; 3] = [GLOBAL_TABLE, DISTRICT_TABLE, CLUSTER_TABLE];

/// docs/industry-boundary.md §2.2.
pub const DICT_GOOD: u32 = 0x10;
pub const DICT_NAME: u32 = 0x11;
pub const DICT_BLOCK_STATE: u32 = 0x12;

/// The bridge's `Module.INDUSTRY` bit in MODULES and SESSION_START (its ordinal, 7).
pub const MODULE_INDUSTRY: u32 = 1 << 7;

/// Kind ranges (docs/industry-boundary.md §1, F1).
pub const LATEST: RangeInclusive<u16> = 0x0040..=0x005F;
pub const JOURNALED: RangeInclusive<u16> = 0x0400..=0x04FF;
pub const COMMANDS: RangeInclusive<u16> = 0x8400..=0x84FF;

/// IND_PORT and IND_PACKAGE carry Java's op id at 48.
const PORT_OP: usize = 48;
const PACKAGE_OP: usize = 48;
/// Where an industry command holds its dimension (the engine's dictionary id).
const RECONCILE_DIM: usize = 36;
const DEPOSIT_DIM: usize = 24;

/// Counters, for tests and `/rtsim`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndustryStats {
    /// Observations handed to the industry engine.
    pub ingested: u64,
    /// Industry records that did not decode (skipped).
    pub bad_records: u64,
    /// Kinds in the industry ranges this build does not read (skipped).
    pub unknown_kinds: u64,
    /// Industry commands written (resends included).
    pub commands_written: u64,
    /// Outbox commands written again after a restart.
    pub resent: u64,
    /// Commands that could never be written (too big, or a dimension Java never named): dropped.
    pub dropped: u64,
    /// COMMAND_RESULT failed for an industry op id.
    pub results_failed: u64,
    /// Kept commands waiting for Java's answer (a gauge).
    pub outbox: u64,
    /// Commands decided but not written yet (a gauge).
    pub queued: u64,
    /// Threats handed to the engine.
    pub threats: u64,
    /// Industry records ignored while the module was switched off (U10).
    pub ignored: u64,
}

/// The industry side of one session.
pub(super) struct Link {
    pub(super) engine: Industry,
    /// Java's names for the dictionaries industry records use, per world: dimension, GOOD, NAME
    /// and BLOCK_STATE (Java resends them at every session start, before any record using them).
    java_names: HashMap<(u32, u32), String>,
    /// The GOOD, NAME and BLOCK_STATE ids sent as DICT commands this session. The engine's ids for
    /// these kinds are Java's own (F2), so they need no table of their own.
    dict_sent: HashSet<(u32, u32)>,
    /// Commands the engine decided, not written yet. Saved.
    queue: VecDeque<Cmd>,
    /// Commands written and kept until Java's answer: (cmdSeq, command). Saved.
    outbox: VecDeque<(u64, Cmd)>,
    /// Outbox commands to write again at the start of this session, oldest first.
    resend: VecDeque<(u64, Cmd)>,
    /// Latest-wins industry records of this hand-over not handed to the engine yet: they go in
    /// game-time order with the journaled ones, journaled first on a tie (W9).
    deferred: VecDeque<Timed>,
    /// The bridge's industry module switch (C25, U10): off, the engine applies no record, runs
    /// nothing and sends nothing; on again, it catches up.
    enabled: bool,
    /// SESSION_START's game time, until the engine's own session start runs: after the replayed
    /// journal, before this session's first record (W19).
    session_start_at: Option<Tick>,
    /// Threats ordered this step, applied at the simulate point (W12).
    threats: Vec<Threat>,
    /// Steps the front command has waited for a Java dictionary name (W21).
    name_waits: u32,
    /// The engine was made new at this start (no pages): its seed is committed at once (W20).
    pub(super) fresh: bool,
    pub(super) stats: IndustryStats,
}

/// Steps a command waits for a Java dictionary name it uses before it goes without (W21).
/// PLACEHOLDER 200 (ten seconds of steps at one a tick): Java sends its whole dictionary at a
/// session start, over as many hand-overs as it needs.
const NAME_WAIT_STEPS: u32 = 200;

/// District ids must fit a page key (48 bits, F36).
const DISTRICT_ID_LIMIT: u64 = 1 << 48;

fn tick(game_time: i64) -> Tick {
    Tick(game_time.max(0) as u64)
}

/// A world seed for a new industry engine: random, then kept in its pages (W5).
fn new_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

fn kept(c: &Cmd) -> bool {
    matches!(
        c,
        Cmd::CarriageDelta(_) | Cmd::DepositDelta(_) | Cmd::WritePackage(_)
    )
}

impl Link {
    /// A link with an empty engine (the core's `State::new`; [`Link::load`] replaces it at start).
    /// Whether the industry module runs (MODULES): the dragons raid districts only then.
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(super) fn empty() -> Link {
        Link {
            engine: Industry::new(0, Tunables::default(), Tick::ZERO),
            java_names: HashMap::new(),
            dict_sent: HashSet::new(),
            queue: VecDeque::new(),
            outbox: VecDeque::new(),
            resend: VecDeque::new(),
            deferred: VecDeque::new(),
            enabled: false,
            session_start_at: None,
            threats: Vec::new(),
            name_waits: 0,
            fresh: false,
            stats: IndustryStats::default(),
        }
    }

    /// Every page of the link and its engine (W6).
    pub(super) fn pages(&self) -> Vec<(PageKey, Vec<u8>)> {
        let mut pages = self.engine.snapshot();
        pages.push((KEY_LINK, encode_link(self)));
        pages
    }

    /// The engine's own session start, once: after the replayed journal (whose records keep their
    /// game times), before anything of this session (W19).
    fn begin_session(&mut self) {
        if let Some(t) = self.session_start_at.take() {
            self.engine.session_start(t);
        }
    }

    /// The engine and the link as the pages hold them, or new ones.
    pub(super) fn load(store: &mut PageStore, cfg: &Config) -> Result<Link, String> {
        let test = cfg.test;
        if cfg.industry_read_only {
            // Phase I-A's read-only mode existed only to show players what a district would have
            // made; I18 (Ben's correction) removed that, and the mode with it. The flag is ignored.
            log_warn("the industry read-only flag is retired (I18); the industry engine applies");
        }
        let keys: Vec<PageKey> = store
            .keys()
            .filter(|k| TABLES.contains(&k.table()))
            .collect();
        let mut pages = Vec::with_capacity(keys.len());
        for k in keys {
            if let Some(b) = store
                .get(k)
                .map_err(|e| format!("cannot read industry page {:#x}: {e}", k.0))?
            {
                pages.push((k, b));
            }
        }
        let has_globals = pages.iter().any(|(k, _)| k.table() == GLOBAL_TABLE);
        let count = pages.len();
        // Pages another build wrote in another format do not decode. Before release there is no
        // backward compatibility (W23): they are dropped, loudly, and the industry engine starts
        // afresh, rather than keeping the whole engine, core included, from starting.
        let restored = if has_globals {
            match Industry::restore(pages, Tunables::default()) {
                Ok(e) => Some(e),
                Err(e) => {
                    log_error(&format!(
                        "the industry engine's {count} pages do not decode ({e}): another build's format; the industry engine starts afresh and they are replaced"
                    ));
                    None
                }
            }
        } else {
            if count > 0 {
                log_warn(&format!(
                    "{count} industry pages without their globals page are ignored"
                ));
            }
            None
        };
        let fresh = restored.is_none();
        let mut engine = restored.unwrap_or_else(|| {
            let seed = cfg.industry_seed.unwrap_or_else(new_seed);
            Industry::new(seed, Tunables::default(), Tick::ZERO)
        });
        engine.log_enabled = test;
        let link_page = store
            .get(KEY_LINK)
            .map_err(|e| format!("cannot read the industry link page: {e}"))?;
        let (queue, outbox) = match link_page {
            Some(_) if fresh => (VecDeque::new(), VecDeque::new()),
            Some(b) => match decode_link(&b) {
                Ok(page) => page,
                Err(e) => {
                    log_error(&format!(
                        "the industry link page does not decode ({e}): another build's format; its queue and outbox are dropped"
                    ));
                    (VecDeque::new(), VecDeque::new())
                }
            },
            None => (VecDeque::new(), VecDeque::new()),
        };
        let mut link = Link::empty();
        link.engine = engine;
        link.queue = queue;
        link.outbox = outbox;
        link.fresh = fresh;
        Ok(link)
    }

    /// At start: what the world save applied (`lastAppliedCmdSeq`) leaves the outbox; the rest is
    /// written again first, with its original sequences.
    pub(super) fn resume(&mut self, last_applied_cmd: u64) {
        self.outbox.retain(|(s, _)| *s > last_applied_cmd);
        self.resend = self.outbox.iter().cloned().collect();
        self.gauges();
        if !self.outbox.is_empty()
            || !self.queue.is_empty()
            || self.engine.districts().next().is_some()
        {
            log_info(&format!(
                "industry engine at tick {}: {} districts, {} commands to resend, {} queued",
                self.engine.now().0,
                self.engine.districts().count(),
                self.resend.len(),
                self.queue.len()
            ));
        }
    }

    fn gauges(&mut self) {
        self.stats.outbox = self.outbox.len() as u64;
        self.stats.queued = self.queue.len() as u64;
    }

    fn ingest(&mut self, t: &Timed) {
        self.engine.ingest(t);
        self.stats.ingested += 1;
    }

    /// Hands the engine the deferred latest-wins records from before `until` (all of them for
    /// None). A hand-over holds its latest-wins records first and its journaled ones after (the
    /// bridge copies the journal at the hand-over), while within a tick Java journals loads,
    /// ports and results before it samples stock: merged by game time, a sweep taken in a load
    /// tick reaches the engine after the load, as it happened.
    fn flush(&mut self, until: Option<Tick>) {
        while let Some(t) = self.deferred.front() {
            if until.is_some_and(|u| t.at >= u) {
                break;
            }
            let t = self.deferred.pop_front().expect("front");
            if self.enabled {
                self.begin_session();
                self.ingest(&t);
            } else {
                self.stats.ignored += 1;
            }
        }
    }

    /// One industry record, decoded. Returns whether it was one this build reads.
    fn decode(&mut self, rec: &Rec<'_>) -> Option<Timed> {
        match iw::decode_obs(rec.bytes) {
            Ok(Some((_, t))) if !districts_fit(&t.obs) => {
                self.stats.bad_records += 1;
                log_warn(&format!(
                    "an industry record of kind {:#06x} names a district id past 48 bits; skipped",
                    rec.kind
                ));
                None
            }
            Ok(Some((_, t))) => Some(t),
            Ok(None) => {
                self.stats.unknown_kinds += 1;
                None
            }
            Err(e) => {
                self.stats.bad_records += 1;
                log_warn(&format!(
                    "an industry record of kind {:#06x} does not decode ({e}); skipped",
                    rec.kind
                ));
                None
            }
        }
    }

    /// Takes a kept command out of the outbox (and the resend queue): Java answered it.
    fn complete(&mut self, op: OpId) -> Option<Cmd> {
        let i = self.outbox.iter().position(|(_, c)| c.op() == Some(op))?;
        let (_, c) = self.outbox.remove(i)?;
        self.resend.retain(|(_, c)| c.op() != Some(op));
        self.stats.outbox = self.outbox.len() as u64;
        Some(c)
    }

    fn name(&self, kind: u32, id: u32) -> Option<String> {
        if let Some(n) = self.java_names.get(&(kind, id)) {
            return Some(n.clone());
        }
        // The engine's goods are paged with it: their names survive a dictionary Java lost.
        (kind == DICT_GOOD)
            .then(|| {
                self.engine
                    .goods()
                    .get(&Good(id))
                    .map(GoodInfo::dictionary_name)
            })
            .flatten()
    }
}

// ---- the link page --------------------------------------------------------------------------

fn encode_link(l: &Link) -> Vec<u8> {
    let mut w = Writer::with_capacity(256);
    w.u32(LINK_MAGIC);
    w.u16(LINK_VERSION);
    rtsim_industry::persist::Persist::put(&l.queue, &mut w);
    rtsim_industry::persist::Persist::put(&l.outbox, &mut w);
    w.into_bytes()
}

type LinkPage = (VecDeque<Cmd>, VecDeque<(u64, Cmd)>);

fn decode_link(b: &[u8]) -> rtsim_kernel::codec::Result<LinkPage> {
    let mut r = Reader::new(b);
    if r.u32()? != LINK_MAGIC {
        return Err(CodecError::Invalid("not an industry link page".into()));
    }
    let v = r.u16()?;
    if v != LINK_VERSION {
        return Err(CodecError::Invalid(format!(
            "industry link page version {v}, this build reads {LINK_VERSION}"
        )));
    }
    let queue = rtsim_industry::persist::Persist::get(&mut r)?;
    let outbox = rtsim_industry::persist::Persist::get(&mut r)?;
    r.finish()?;
    Ok((queue, outbox))
}

// ---- observations ---------------------------------------------------------------------------

/// SESSION_START: the switch, and the engine's own session start (F6: open learning windows are
/// discarded, every district is unloaded until its chunks report in). That one waits until the
/// journal has replayed (W19): run first, it would advance the engine to this session's game time
/// and every replayed record would be taken as happening now, so a decision the pages had not
/// reached would come out different from the one Java already applied.
pub(super) fn session_start(st: &mut State, game_time: i64, module_mask: u32) {
    let l = &mut st.industry;
    l.enabled = module_mask & MODULE_INDUSTRY != 0;
    // Switched off, the engine's own session start waits for the switch (U10): run now, it would
    // advance the engine while it is meant to be frozen.
    l.session_start_at = Some(tick(game_time));
}

/// The switch changed in a session (MODULES, U10). Off: the engine is frozen where it stands (its
/// queue and outbox wait, its pages stay what they are, every record is ignored). On: its own
/// session start runs at once, at this game time, so it catches up to now (every breakpoint and
/// wake of the time it was off fires) and takes every district as unloaded until Java reports
/// them again; Java announces its districts and load states again when the module turns on, as
/// at a session start.
fn set_enabled(st: &mut State, on: bool) {
    let gt = tick(st.game_time);
    let l = &mut st.industry;
    let was = l.enabled;
    l.enabled = on;
    if on && !was {
        l.session_start_at = Some(gt.max(l.engine.now()));
        // An answer that came while off was ignored: every kept command goes again, under a new
        // sequence (Java skips any at or below the last it applied), and Java answers it by op id
        // (DUPLICATE if it had applied it), so none waits in the outbox for an answer already gone.
        let kept: Vec<Cmd> = l.outbox.drain(..).map(|(_, c)| c).collect();
        l.resend.clear();
        for c in kept.into_iter().rev() {
            l.queue.push_front(c);
        }
        l.gauges();
    }
}

/// A record that is not journaled: Java's DICT entries, MODULES, and the latest-wins industry
/// kinds. Anything else is not the industry's.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    let b = rec.bytes;
    let l = &mut st.industry;
    match rec.kind {
        obs::DICT if rec.size() >= obs::dict::NAME => {
            let kind = rec.aux;
            if ![wire::DICT_DIMENSION, DICT_GOOD, DICT_NAME, DICT_BLOCK_STATE].contains(&kind) {
                return;
            }
            let id = wire::u32_at(b, obs::dict::ID);
            let len = wire::u32_at(b, obs::dict::NAME_LEN) as usize;
            if obs::dict::NAME + len > b.len() {
                return;
            }
            let name =
                String::from_utf8_lossy(&b[obs::dict::NAME..obs::dict::NAME + len]).into_owned();
            java_name(l, kind, id, name);
        }
        obs::MODULES if rec.size() >= obs::modules::SIZE => {
            let on = wire::u32_at(b, obs::modules::MASK) & MODULE_INDUSTRY != 0;
            set_enabled(st, on);
        }
        k if LATEST.contains(&k) => {
            if !l.enabled {
                l.stats.ignored += 1;
                return;
            }
            if let Some(t) = l.decode(rec) {
                l.deferred.push_back(t);
            }
        }
        _ => {}
    }
}

/// One entry of Java's dictionary (a DICT observation, or an order's names). A GOOD defines the
/// engine's good of the same id.
fn java_name(l: &mut Link, kind: u32, id: u32, name: String) {
    if kind == DICT_GOOD {
        match GoodInfo::parse(&name) {
            Some(info) => l.engine.define_good(Good(id), info),
            None => log_warn(&format!(
                "GOOD {id} '{name}' is not a good name; the engine counts it as a 64-stack item"
            )),
        }
    }
    l.java_names.insert((kind, id), name);
}

/// The journaled records that are the industry's: its kinds, and COMMAND_RESULTs for its op ids.
pub(super) fn claims(rec: &Rec<'_>) -> bool {
    JOURNALED.contains(&rec.kind)
        || (rec.kind == obs::COMMAND_RESULT
            && rec.size() >= obs::command_result::SIZE
            && IndustryOps::is_industry(OpId(wire::u64_at(rec.bytes, obs::command_result::OP_ID))))
}

/// The op id `Replay` applies an industry record under at most once (0: none). Java's own op ids
/// only (ports and packages): IND_RECONCILE_RESULT and IND_CARRIAGE_RESULT carry the engine's, and
/// the engine takes each once by state (its pending reconciles and exchanges), so they need no op
/// in `Replay` and never meet the COMMAND_RESULT Java journals under the same op id (W11).
pub(super) fn op_of(rec: &Rec<'_>) -> u64 {
    let b = rec.bytes;
    let at = match rec.kind {
        iw::IND_PORT => PORT_OP,
        iw::IND_PACKAGE => PACKAGE_OP,
        _ => return 0,
    };
    if b.len() >= at + 8 {
        wire::u64_at(b, at)
    } else {
        0
    }
}

/// A journaled record `Replay` admitted (see [`claims`]). Switched off, the industry ignores it,
/// as the dragons and ships do (U10, replacing W8's "journaled records are still applied"): it is
/// admitted for its sequence and op id, counted, and dropped; a COMMAND_RESULT too, so a kept
/// command stays in the outbox and goes again once the module is on.
pub(super) fn journaled(st: &mut State, rec: &Rec<'_>) {
    let this_session = wire::u64_at(rec.bytes, obs::OBS_SEQ) >= st.session_first;
    let l = &mut st.industry;
    if !l.enabled {
        l.stats.ignored += 1;
        return;
    }
    if this_session {
        l.begin_session();
    }
    let at = tick(wire::i64_at(rec.bytes, obs::GAME_TIME));
    l.flush(Some(at));
    if rec.kind == obs::COMMAND_RESULT {
        let op = OpId(wire::u64_at(rec.bytes, obs::command_result::OP_ID));
        command_result(l, op, rec.aux, at);
        return;
    }
    let Some(t) = l.decode(rec) else { return };
    if let Obs::CarriageResult(r) = &t.obs {
        l.complete(r.op);
    }
    l.ingest(&t);
}

/// A journaled record `Replay` refused by op id: an answer for an op it already heard. A kept
/// command written under that op id again (a test's order, or the same decision resent) is
/// answered all the same, so it leaves the outbox.
pub(super) fn answered_again(st: &mut State, rec: &Rec<'_>) {
    if rec.kind == obs::COMMAND_RESULT {
        let op = OpId(wire::u64_at(rec.bytes, obs::command_result::OP_ID));
        st.industry.complete(op);
    }
}

/// An industry command a test ordered (`Order::IndustryCommand`, W17): it joins the queue as if
/// the industry engine had decided it, so it takes the same sequences, DICT records and outbox.
/// The Java dictionary entries it names come with it, so it never waits for Java's DICT records.
pub(super) fn order_command(st: &mut State, names: &[(u32, u32, String)], record: &[u8]) {
    for (kind, id, name) in names {
        java_name(&mut st.industry, *kind, *id, name.clone());
    }
    match iw::decode_cmd(record) {
        Ok(Some((_, c))) => {
            st.industry.queue.push_back(c);
            st.industry.gauges();
        }
        Ok(None) => log_warn("an industry command order holds no industry command; dropped"),
        Err(e) => log_warn(&format!(
            "an industry command order does not decode ({e}); dropped"
        )),
    }
}

/// Java's answer to an industry command. A kept command leaves the outbox. A failed carriage
/// delta moved nothing: the engine takes it back from its own exchange record (it holds one until
/// the result, so this works whether or not the outbox still had the command: a restart trims the
/// outbox by the world save's `lastAppliedCmdSeq`, which already counts a failed command). W10.
fn command_result(l: &mut Link, op: OpId, status: u32, at: Tick) {
    let kept = l.complete(op);
    if status != obs::command_result::FAILED {
        return;
    }
    l.stats.results_failed += 1;
    if l.engine.command_failed(at, op) {
        return;
    }
    match kept {
        Some(c) => log_warn(&format!(
            "Minecraft could not apply industry command {:#x} ({}); the engine keeps its books as if it had (W10)",
            op.0,
            kind_name(&c)
        )),
        None => log_warn(&format!(
            "Minecraft could not apply industry command {:#x}",
            op.0
        )),
    }
}

/// A kept command the link drops (it can never be written): for a carriage delta, as if Java had
/// answered it failed, so the exchange is taken back.
fn dropped(l: &mut Link, c: &Cmd) {
    l.stats.dropped += 1;
    if let Some(op) = c.op() {
        l.complete(op);
        let now = l.engine.now();
        l.engine.command_failed(now, op);
    }
}

/// Every district id an observation names fits 48 bits (F36: the page key); a longer one would
/// stop the engine at its next save.
fn districts_fit(o: &Obs) -> bool {
    let d = match o {
        Obs::StockSample { district, .. }
        | Obs::LedgerRequest { district }
        | Obs::Watched { district, .. }
        | Obs::Approach { district, .. }
        | Obs::DistrictChunks { district, .. }
        | Obs::DistrictRemoved { district }
        | Obs::Edit { district, .. }
        | Obs::Discard { district, .. }
        | Obs::LoadState { district, .. }
        | Obs::ChunkLoaded { district, .. }
        | Obs::ChunkUnloaded { district, .. }
        | Obs::SurveyBlocks { district, .. } => *district,
        Obs::SweepEnd(e) => e.district,
        Obs::DepositSample(s) => s.district,
        Obs::District(x) => x.district,
        Obs::Inventory(x) => x.district,
        Obs::Port(p) => p.district,
        Obs::TrainArrived(c) => c.district,
        Obs::TrainDeparted(c) => c.district,
        Obs::Package(p) => p.district,
        Obs::ReconcileResult(r) => r.district,
        Obs::SpawnSample(s) => s.district,
        Obs::Fight(f) => f.district,
        Obs::Survey(s) => s.district,
        Obs::Station(s) => s.district.unwrap_or_default(),
        Obs::PowerLink { sink, source } => {
            return sink.0 < DISTRICT_ID_LIMIT && source.is_none_or(|s| s.0 < DISTRICT_ID_LIMIT);
        }
        Obs::Conditions(_) | Obs::CarriageResult(_) => {
            return true;
        }
    };
    d.0 < DISTRICT_ID_LIMIT
}

fn kind_name(c: &Cmd) -> &'static str {
    match c {
        Cmd::ChunkReconcile(_) => "IND_CHUNK_RECONCILE",
        Cmd::CarriageDelta(_) => "IND_CARRIAGE_DELTA",
        Cmd::DepositDelta(_) => "IND_DEPOSIT_DELTA",
        Cmd::WritePackage(_) => "IND_WRITE_PACKAGE",
        Cmd::Ledger(_) => "IND_LEDGER",
    }
}

/// A threat from another system (I19; the dragons and conflict modules, or a test's order). It
/// waits for the simulate point, after this hand-over's records: applied at once it would carry
/// the engine past records of earlier ticks in the same hand-over (W12).
pub(super) fn threat(st: &mut State, t: &Threat) {
    st.industry.threats.push(*t);
}

// ---- the step's simulation ------------------------------------------------------------------

/// "2. Simulate": every breakpoint and wake due by the hand-over's game time fires, and the
/// engine's commands join the queue.
pub(super) fn simulate(st: &mut State) {
    let game_time = st.game_time;
    let l = &mut st.industry;
    l.begin_session();
    l.flush(None);
    let threats = std::mem::take(&mut l.threats);
    if !l.enabled {
        if !threats.is_empty() {
            log_warn("threats for the industry engine while its module is off; dropped");
        }
        return;
    }
    for t in &threats {
        l.engine.threat(t);
        l.stats.threats += 1;
    }
    l.engine.advance_to(tick(game_time));
    l.queue.extend(l.engine.drain_commands());
    l.gauges();
}

// ---- commands -------------------------------------------------------------------------------

enum One {
    Written(u64),
    /// Not enough room left in this buffer, or a Java name it uses has not come yet (W21).
    Full,
    /// Can never be written.
    Never,
}

/// A command's dimension: (its offset in the record, Java's dimension id).
type DimAt = Option<(usize, u32)>;

/// The dictionary entries a command uses: (kind, Java's id), and where its dimension is.
fn names_of(c: &Cmd) -> (DimAt, Vec<(u32, u32)>) {
    let mut names = Vec::new();
    let goods = |g: Good, names: &mut Vec<(u32, u32)>| names.push((DICT_GOOD, g.0));
    let dim = match c {
        Cmd::ChunkReconcile(r) => {
            r.deltas.iter().for_each(|d| goods(d.good, &mut names));
            r.destroy
                .iter()
                .for_each(|d| names.push((DICT_BLOCK_STATE, d.expected_state)));
            Some((RECONCILE_DIM, r.chunk.dim))
        }
        Cmd::CarriageDelta(d) => {
            d.entries.iter().for_each(|e| goods(e.1, &mut names));
            None
        }
        Cmd::DepositDelta(d) => {
            goods(d.good, &mut names);
            Some((DEPOSIT_DIM, d.chunk.dim))
        }
        Cmd::WritePackage(p) => {
            names.push((DICT_NAME, p.address));
            p.goods.goods().for_each(|g| goods(g, &mut names));
            None
        }
        Cmd::Ledger(l) => {
            l.goods.iter().for_each(|e| goods(e.0, &mut names));
            if let Some(g) = l.next_good {
                goods(g, &mut names);
            }
            None
        }
    };
    names.sort_unstable();
    names.dedup();
    (dim, names)
}

/// Writes one command, its DICT records first: `seq` is its resent sequence, or None for a new
/// one taken now (so the buffer's sequences rise in buffer order).
fn write_one(st: &mut State, w: &mut RecordWriter<'_>, c: &Cmd, seq: Option<u64>) -> One {
    let mut rec = Vec::new();
    if let Err(e) = iw::encode_cmd(0, c, &mut rec) {
        log_error(&format!(
            "an industry command ({}) does not encode: {e}; dropped",
            kind_name(c)
        ));
        return One::Never;
    }
    debug_assert!(COMMANDS.contains(&wire::u16_at(&rec, wire::R_KIND)));
    let (dim, names) = names_of(c);
    // Java sends its whole dictionary at a session start, over as many hand-overs as it takes:
    // a name not here yet is waited for, a while (W21).
    let missing = dim.is_some_and(|(_, d)| {
        !st.industry
            .java_names
            .contains_key(&(wire::DICT_DIMENSION, d))
    }) || names
        .iter()
        .any(|&(kind, id)| st.industry.name(kind, id).is_none());
    if missing && st.industry.name_waits < NAME_WAIT_STEPS {
        st.industry.name_waits += 1;
        return One::Full;
    }
    let dim = match dim {
        Some((at, java_dim)) => match st
            .industry
            .java_names
            .get(&(wire::DICT_DIMENSION, java_dim))
        {
            Some(name) => Some((at, name.clone())),
            None => {
                log_error(&format!(
                    "an industry command ({}) names dimension {java_dim}, which Java never named; dropped",
                    kind_name(c)
                ));
                return One::Never;
            }
        },
        None => None,
    };
    // Mirrored entries this session has not sent; a name Java never gave goes without (Java then
    // skips what uses it, as the protocol says for an unknown id).
    let mirror: Vec<(u32, u32, String)> = names
        .into_iter()
        .filter(|k| !st.industry.dict_sent.contains(k))
        .filter_map(|(kind, id)| st.industry.name(kind, id).map(|n| (kind, id, n)))
        .collect();
    let mut need = rec.len();
    need += mirror
        .iter()
        .map(|(_, _, n)| cmd::dict::size(n.len()))
        .sum::<usize>();
    if let Some((_, name)) = &dim {
        need += st.dict_bytes(&[(wire::DICT_DIMENSION, name.as_str())]);
    }
    let too_big = rec.len() > wire::MAX_RECORD
        || mirror
            .iter()
            .any(|(_, _, n)| cmd::dict::size(n.len()) > wire::MAX_RECORD)
        || need > w.capacity();
    if too_big {
        log_error(&format!(
            "an industry command ({}, {need} bytes with its DICT records) can never fit a command buffer; dropped",
            kind_name(c)
        ));
        return One::Never;
    }
    if need > w.remaining() {
        return One::Full;
    }
    if let Some((at, name)) = dim {
        let id = st.dict_id(w, wire::DICT_DIMENSION, &name);
        wire::put_u32(&mut rec, at, id);
    }
    for (kind, id, name) in mirror {
        let bytes = name.as_bytes();
        let r = w
            .reserve(cmd::DICT, cmd::dict::size(bytes.len()), kind)
            .expect("checked to fit");
        wire::put_u32(r, cmd::dict::ID, id);
        wire::put_u32(r, cmd::dict::NAME_LEN, bytes.len() as u32);
        r[cmd::dict::NAME..cmd::dict::NAME + bytes.len()].copy_from_slice(bytes);
        st.industry.dict_sent.insert((kind, id));
    }
    let seq = seq.unwrap_or_else(|| st.session_seq());
    wire::put_u64(&mut rec, cmd::CMD_SEQ, seq);
    assert!(w.copy(&rec), "checked to fit");
    st.industry.stats.commands_written += 1;
    st.industry.name_waits = 0;
    One::Written(seq)
}

/// The sequence of the next kept command to resend, if any (the step merges every resend queue in
/// sequence order).
pub(super) fn next_resend(st: &State) -> Option<u64> {
    st.industry.resend.front().map(|r| r.0)
}

/// Writes the resend queue's commands whose sequences are below `before` (the core's next resend,
/// or None for all): Java skips a command at or below the last one it applied, so the two resend
/// queues go out merged in sequence order. Returns the highest sequence written and whether the
/// buffer filled.
pub(super) fn write_resends(
    st: &mut State,
    w: &mut RecordWriter<'_>,
    before: Option<u64>,
) -> (u64, bool) {
    let mut highest = 0;
    if !st.industry.enabled && !st.industry.resend.is_empty() {
        // The module is off: these wait in the queue and take new sequences when it is on again
        // (Java skips by op id what it already applied).
        let resend: Vec<(u64, Cmd)> = st.industry.resend.drain(..).collect();
        for (_, c) in resend.into_iter().rev() {
            if let Some(op) = c.op() {
                st.industry.complete(op);
            }
            st.industry.queue.push_front(c);
        }
        st.industry.gauges();
        return (0, false);
    }
    while let Some((seq, c)) = st.industry.resend.front().cloned() {
        if before.is_some_and(|b| seq > b) {
            break;
        }
        match write_one(st, w, &c, Some(seq)) {
            One::Written(s) => {
                highest = highest.max(s);
                st.industry.resend.pop_front();
                st.industry.stats.resent += 1;
            }
            One::Full => return (highest, true),
            One::Never => {
                st.industry.resend.pop_front();
                dropped(&mut st.industry, &c);
            }
        }
    }
    (highest, false)
}

/// Writes queued commands while they fit; kept ones join the outbox under the sequence they got.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    if !st.industry.enabled {
        return (0, false);
    }
    while let Some(c) = st.industry.queue.front().cloned() {
        match write_one(st, w, &c, None) {
            One::Written(seq) => {
                highest = highest.max(seq);
                st.industry.queue.pop_front();
                if kept(&c) {
                    st.industry.outbox.push_back((seq, c));
                }
            }
            One::Full => {
                st.industry.gauges();
                return (highest, true);
            }
            One::Never => {
                st.industry.queue.pop_front();
                dropped(&mut st.industry, &c);
            }
        }
    }
    st.industry.gauges();
    (highest, false)
}

// ---- saves ----------------------------------------------------------------------------------

/// Every industry page, taken at this step boundary: the engine's (F36) and the link's.
pub(super) fn snapshot(st: &State) -> Vec<(PageKey, Vec<u8>)> {
    st.industry.pages()
}

/// The industry pages the store holds that a snapshot no longer has (a district removed, a replay
/// cluster merged or dissolved): deleted in the same commit, or a restore would bring them back.
pub(super) fn deletions(store: &PageStore, snapshot: &[(PageKey, Vec<u8>)]) -> Vec<PageKey> {
    let keep: HashSet<PageKey> = snapshot.iter().map(|(k, _)| *k).collect();
    store
        .keys()
        .filter(|k| TABLES.contains(&k.table()) && !keep.contains(k))
        .collect()
}

// ---- tests ----------------------------------------------------------------------------------

impl Engine {
    /// Runs `f` on the industry engine and the link's counters, under the state lock. For tests
    /// and debugging, between steps only: a step holds the lock while it runs.
    pub fn with_industry<R>(&self, f: impl FnOnce(&Industry, &IndustryStats) -> R) -> R {
        let st = lock(&self.state);
        f(&st.industry.engine, &st.industry.stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_industry::cmd::{CarriageDelta, CarriageReason, DepositDelta};
    use rtsim_industry::ids::{ChunkPos, TrainId};
    use rtsim_industry::obs::DepositPluginId;

    #[test]
    fn the_link_page_round_trips() {
        let mut l = Link::empty();
        l.queue.push_back(Cmd::DepositDelta(DepositDelta {
            op: OpId((0x49 << 56) | 5),
            plugin: DepositPluginId::DieselOil,
            chunk: ChunkPos {
                dim: 1,
                x: 3,
                z: -2,
            },
            good: Good(2),
            delta: -7,
        }));
        l.outbox.push_back((
            41,
            Cmd::CarriageDelta(CarriageDelta {
                op: OpId((0x49 << 56) | 6),
                train: TrainId(9),
                reason: CarriageReason::Exchange,
                entries: vec![(0, Good(1), 12)],
            }),
        ));
        let (q, o) = decode_link(&encode_link(&l)).unwrap();
        assert_eq!(q, l.queue);
        assert_eq!(o, l.outbox);
        assert!(decode_link(&encode_link(&l)[..5]).is_err());
    }
}
