// SPDX-License-Identifier: AGPL-3.0-or-later
//! TALK behind the boundary (docs/boundary.md §13, TALK.md §9; choices TK32 to TK46 in
//! docs/choices-talk.md): what a talker tells a player who crouches and clicks on it.
//!
//! - **TALK_ASK** (0x00C0, latest wins): Java's open questions, sent each tick until answered.
//!   [`observe_state`] queues each askId once; one answered, or still waiting, is ignored when it
//!   comes again; a malformed record or an ask that names nothing is refused and counted.
//! - **TALK_FACTS** (0x8A00, a command): the answer, written by [`write_queue`] after the step's
//!   simulate, so it reads the world as it stands at this hand-over. Per session: it takes a
//!   sequence from `session_seq`, has no op id, is never kept and never resent after a restart. A
//!   full buffer keeps the ask for the next one.
//! - **What it reads**: the holder's noticeboard (phase 1's knowledge), its feeling toward the
//!   asker, its market's scarcest and fullest good, and the actors the news names, each described by
//!   the module that owns it (phase 1: villages, bases, caravans and players; ecology: lairs and
//!   dragons; conflict: camps and chiefs). It changes nothing in the world; the only writer is the
//!   TEST order [`post`].
//! - **A camp** (holder kind 3, slice 4, TK71 to TK84): the camp's knowledge is its chief's board,
//!   else its heir's (a haven is not a holder), and the conflict is read through
//!   `rtsim_conflict::view::faction_facts` and nothing else. An ask with the FACTION flag also gets
//!   the 48-byte faction block after the news: the camp's feud against the asker, and its moods.
//!
//! The ranking, the refs and the port state are pure functions of the board and the world, so they
//! are tested apart from the protocol.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_conflict::engine::SETTLERS_KEY;
use rtsim_conflict::view::{FactionFacts, faction_facts};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::report::{COORD_LIMIT, FLAG_PLAYER, ReportId, rung_weight};
use rtsim_knowledge::{ActorId, ActorKind, Place, Report, ReportKind};
use rtsim_settle::{NodeKind, Pos};
use rtsim_space::topology::Topologies;
use rtsim_world::interest::uuid_hash;

use super::{State, log_warn};
use crate::orders::{TalkActor, TalkReportCmd};
use crate::talk_wire::{
    Ask, Faction, Facts, News, Ref, TALK_ASK, ask, decode_asks, facts, news,
    reference::{self, class},
};
use crate::wire::{self, Rec, RecordWriter};

/// Asks waiting for their answer at most; a later one is dropped (Java sends it again next tick).
pub const MAX_PENDING: usize = 256;
/// Answered askIds remembered, the oldest forgotten first (TK39).
pub const MAX_ANSWERED: usize = 1_024;
/// Blocks within which a report's place counts as seen by the holder, not heard (TK14).
pub const SEEN_BLOCKS: i64 = 96;
/// How far from a report's place the settlement it names as "near" may be, blocks: three market
/// cells (TK35).
pub const NEAR_BLOCKS: i64 = 768;
/// News items in one answer (TK20).
pub const MAX_NEWS: usize = facts::MAX_NEWS;
/// How long after a succession a camp's chief counts as new, ticks: ten game days (TK75).
/// TUNABLE (PLACEHOLDER).
pub const NEW_CHIEF_TICKS: u64 = 10 * TICKS_PER_DAY;

/// The tag rtsim-ecology puts on a lair's key to register its knowledge actor (`LAIR_TAG` in its
/// `engine.rs`, private there). A test registers a real lair and checks the actor's key against
/// it (TK41).
const LAIR_TAG: u64 = 0x4C41_4952 << 32;
/// The upper half of an oil district's actor key (`DIST_TAG ^ site id` in rtsim-ecology's
/// `oil.rs`): such a group is not a caravan.
const DIST_PREFIX: u64 = 0x4449_5354;
/// The upper half of a lair's actor key (`LAIR_TAG`): a lair the ecology does not hold any more is
/// still not a caravan.
const LAIR_PREFIX: u64 = LAIR_TAG >> 32;

/// A severity under this is read as a rung, whatever the kind, when ranking news (TK33). Only a
/// guard now: the conflict once recorded rungs on today's kinds (a robbery of 2, a rescue of 2, a
/// killing of 4), and since MZB-36 (b053f0f5) it records them on `SEVERITY_UNIT` like every other
/// module, so no record of today's kinds should fall under it.
pub const RUNG_LIMIT: u8 = 25;

/// What TALK counts (`Engine::talk_stats`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// TALK_ASK records read.
    pub records: u64,
    /// Asks accepted (distinct askIds, this session and before).
    pub asks: u64,
    /// Answers written.
    pub answered: u64,
    /// Of those, answers that carried HOLDER.
    pub with_holder: u64,
    /// Of those, answers that carried a faction block (a camp, with the FACTION ask flag).
    pub with_faction: u64,
    /// Asks and records refused: undecodable, an askId of 0, a dimension no frame can be.
    pub refused: u64,
    /// Asks dropped because [`MAX_PENDING`] were waiting.
    pub dropped: u64,
    /// TEST reports posted, and refused.
    pub posted: u64,
    pub post_refused: u64,
    /// TALKED records applied: a village credited a talk (TK2).
    pub talked: u64,
    /// TALKED records refused: undecodable, an unknown village, a nil player.
    pub talked_refused: u64,
}

/// TALK's state the step keeps (`State::talk`). Nothing here is saved: an ask lives for a few
/// ticks and its answer is per session.
#[derive(Default)]
pub(super) struct Link {
    pending: VecDeque<Ask>,
    answered: BTreeSet<u64>,
    /// `answered` in the order the ids were added, to forget the oldest.
    order: VecDeque<u64>,
    pub(super) stats: Stats,
}

impl Link {
    fn remember(&mut self, id: u64) {
        if self.answered.insert(id) {
            self.order.push_back(id);
            while self.order.len() > MAX_ANSWERED {
                if let Some(old) = self.order.pop_front() {
                    self.answered.remove(&old);
                }
            }
        }
    }

    /// Whether an ask with this id is waiting or was answered.
    fn knows(&self, id: u64) -> bool {
        self.answered.contains(&id) || self.pending.iter().any(|a| a.id == id)
    }
}

// ---- hooks --------------------------------------------------------------------------------------

/// SESSION_START: Java's askIds are per session; whatever was waiting is moot.
pub(super) fn session_start(st: &mut State) {
    let l = &mut st.talk;
    l.pending.clear();
    l.answered.clear();
    l.order.clear();
}

/// A latest-wins record: TALK_ASK.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != TALK_ASK {
        return;
    }
    st.talk.stats.records += 1;
    let asks = match decode_asks(rec.bytes) {
        Ok(a) => a,
        Err(e) => {
            refuse(st, &format!("a TALK_ASK does not decode ({e})"));
            return;
        }
    };
    for a in asks {
        if a.id == 0 {
            refuse(st, "a TALK_ASK names askId 0");
            continue;
        }
        if a.dimension > u32::from(u16::MAX) {
            refuse(
                st,
                &format!("a TALK_ASK names dimension {} (no frame)", a.dimension),
            );
            continue;
        }
        let l = &mut st.talk;
        if l.knows(a.id) {
            continue; // Java sends an open ask every tick
        }
        if l.pending.len() >= MAX_PENDING {
            l.stats.dropped += 1;
            continue;
        }
        l.stats.asks += 1;
        l.pending.push_back(a);
    }
}

/// Counts a refusal and logs it when the count reaches a power of two (Java would send the same
/// bad record every tick).
fn refuse(st: &mut State, why: &str) {
    st.talk.stats.refused += 1;
    let n = st.talk.stats.refused;
    if n.is_power_of_two() {
        log_warn(&format!("{why}; skipped ({n} refused so far)"));
    }
}

/// Counts a refused TALKED and logs it when the count reaches a power of two.
pub(super) fn refuse_talked(st: &mut State, why: &str) {
    st.talk.stats.talked_refused += 1;
    let n = st.talk.stats.talked_refused;
    if n.is_power_of_two() {
        log_warn(&format!("{why}; skipped ({n} TALKED refused so far)"));
    }
}

/// Whether every dictionary name fits a record of its own: a longer one could never be written,
/// whatever the buffer (the same test as a resent transfer's, TK44).
fn names_fit(names: &[(u32, &str)]) -> bool {
    names
        .iter()
        .all(|(_, n)| wire::cmd::dict::size(n.len()) <= wire::MAX_RECORD)
}

/// Writes the answers that fit. Returns the highest sequence written and whether some wait for the
/// next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    while let Some(q) = st.talk.pending.front().copied() {
        let mut ans = answer(st, &q);
        let names: Vec<(u32, &str)> = ans.whats.iter().map(|(_, k, n)| (*k, n.as_str())).collect();
        let need = st.dict_bytes(&names) + ans.facts.size();
        if !names_fit(&names) || need > w.capacity() {
            // Cannot happen with four news items and real names; a stuck ask would hold the others
            // back (TK44).
            super::log_error(&format!(
                "a TALK_FACTS of {need} bytes can never fit a command buffer; dropped"
            ));
            st.talk.pending.pop_front();
            continue;
        }
        if need > w.remaining() {
            return (highest, true);
        }
        for (slot, kind, name) in std::mem::take(&mut ans.whats) {
            let id = st.dict_id(w, kind, &name);
            ans.patch(slot, id);
        }
        let seq = st.session_seq();
        let rec = ans.facts.encode(seq);
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.talk.pending.pop_front();
        st.talk.remember(q.id);
        st.talk.stats.answered += 1;
        if ans.facts.flags & facts::HOLDER != 0 {
            st.talk.stats.with_holder += 1;
        }
        if ans.facts.flags & facts::FACTION != 0 {
            st.talk.stats.with_faction += 1;
        }
    }
    (highest, false)
}

// ---- the answer ---------------------------------------------------------------------------------

/// Where a dictionary name goes in an answer, once the dictionary has given it an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Scarcest,
    Glutted,
    /// News item, ref (0 who, 1 whom, 2 near).
    Ref(usize, usize),
}

/// An answer before its dictionary names have ids.
struct Answer {
    facts: Facts,
    whats: Vec<(Slot, u32, String)>,
}

impl Answer {
    fn patch(&mut self, slot: Slot, id: u32) {
        match slot {
            Slot::Scarcest => self.facts.scarcest = id,
            Slot::Glutted => self.facts.glutted = id,
            Slot::Ref(i, j) => {
                let n = &mut self.facts.news[i];
                match j {
                    0 => n.who.what = id,
                    1 => n.whom.what = id,
                    _ => n.near.what = id,
                }
            }
        }
    }
}

/// The holder an ask names, as phase 1 knows it.
struct Holder {
    /// Whose board, feelings and port state answer. A camp has none when its chief and its heir
    /// are both gone: the camp is known and has nothing to tell (TK71).
    actor: Option<ActorId>,
    /// Actors news about which is about the holder too: a caravan's home village (TK36); a camp's
    /// own actor, chief, heir and haven (TK73). No nulls, none twice, never `actor`.
    also: Vec<ActorId>,
    /// Its market node, for a village.
    node: Option<usize>,
    /// Where it is now, when it is in the ask's frame.
    pos: Option<(i64, i64)>,
    /// The camp, when the holder is one: what the conflict says of it, with no player.
    camp: Option<FactionFacts>,
}

/// Whose answer: the holder the ask names, or none (TK32, TK38).
fn holder_of(st: &State, a: &Ask) -> Option<Holder> {
    let l = &st.phase1;
    if !l.is_on() {
        return None;
    }
    let frame = a.dimension as u16;
    match a.holder_kind {
        ask::holder::VILLAGE => {
            let node = l.node_of(a.holder)?;
            let n = l.world.nodes.get(node)?;
            Some(Holder {
                actor: Some(n.actor),
                also: Vec::new(),
                node: Some(node),
                pos: (n.pos.frame == frame).then_some((i64::from(n.pos.x), i64::from(n.pos.z))),
                camp: None,
            })
        }
        ask::holder::CARAVAN => {
            let bits = l.caravan_bits(a.holder)?;
            let car = l.world.caravans.get(Handle::from_bits(bits))?;
            let p = l.world.caravan_place(car, l.world.now());
            Some(Holder {
                actor: Some(car.holder),
                also: l
                    .world
                    .nodes
                    .get(car.home)
                    .map(|n| n.actor)
                    .into_iter()
                    .collect(),
                node: None,
                pos: (p.frame == frame).then_some((i64::from(p.x), i64::from(p.z))),
                camp: None,
            })
        }
        ask::holder::FACTION => camp_holder(st, a),
        // A ship (kind 4) answers from the ships' table, a later slice; until then it answers
        // without HOLDER, like any holder this build does not know (TK38).
        _ => None,
    }
}

/// A camp as a holder (TK71, TK73, TK74): known when the conflict runs and has a faction of this key.
/// Its knowledge is the chief's board, else the heir's; a haven is not a holder in phase 1's
/// knowledge, so the camp has no board of its own. `us` for its news is the chief, the heir, the
/// faction's own actor and the haven's.
fn camp_holder(st: &State, a: &Ask) -> Option<Holder> {
    if !st.conflict.enabled {
        return None;
    }
    let ff = faction_facts(&st.conflict.engine, a.holder, None)?;
    let w = &st.phase1.world;
    let k = &w.knowledge;
    let actor = [ff.chief, ff.heir]
        .into_iter()
        .find(|c| !c.is_null() && k.is_holder(*c));
    let mut also = Vec::new();
    let haven = w.nodes.get(ff.node).map(|n| n.actor);
    for x in [Some(ff.actor), Some(ff.chief), Some(ff.heir), haven]
        .into_iter()
        .flatten()
    {
        if !x.is_null() && Some(x) != actor && !also.contains(&x) {
            also.push(x);
        }
    }
    let pos =
        (ff.pos.frame == a.dimension as u16).then_some((i64::from(ff.pos.x), i64::from(ff.pos.z)));
    Some(Holder {
        actor,
        also,
        node: None,
        pos,
        camp: Some(ff),
    })
}

/// The faction block of an answer (TK72, TK75 to TK77): the camp's feud against the asker, and its moods
/// at the view's own time (`ff.now`, so the block and the feud's projection share one). `heir_born` is
/// when the heir came to be: a succession names a new heir, so it is when the chief last changed.
/// `chief_gone` is a chief who has died and an heir who answers in its place: the conflict only
/// succeeds lazily (a feud wake or a plunder), so until then `chiefs` and `heir_born` trail the death
/// and the camp's chief is new all the same (TK75).
fn faction_block(ff: &FactionFacts, heir_born: Option<Tick>, chief_gone: bool) -> Faction {
    use facts::faction::{NO_SIGNAL, mood};
    let now = ff.now;
    let left = ff.cowed_until.0.saturating_sub(now.0);
    let mut flags = 0;
    if left > 0 {
        flags |= mood::COWED;
    }
    if ff.chief_out {
        flags |= mood::CHIEF_OUT;
    }
    if chief_gone
        || (ff.chiefs > 2 && heir_born.is_some_and(|b| now.0.saturating_sub(b.0) < NEW_CHIEF_TICKS))
    {
        flags |= mood::NEW_CHIEF;
    }
    // Never both: with a cap under a warrior and a half the two ranges meet, and lean wins (TK76).
    if ff.pool_cap > 0.0 {
        if ff.pool < ff.pool_cap / 3.0 {
            flags |= mood::LEAN;
        } else if ff.pool >= ff.pool_cap - 0.5 {
            flags |= mood::STRONG;
        }
    }
    Faction {
        rung: ff.step as u8,
        mood: flags,
        signal: ff.signal.map_or(NO_SIGNAL, |s| u16::from(s as u8)),
        grievance: ff.grievance as f32,
        cowed_ticks: u32::try_from(left).unwrap_or(u32::MAX),
        plunders: ff.plunders,
        // Reserved for the Mowzie plugin's name line (M9): the chief's ref, nothing yet.
        chief: Ref::default(),
    }
}

fn answer(st: &State, a: &Ask) -> Answer {
    let mut ans = Answer {
        facts: Facts {
            ask_id: a.id,
            ..Facts::default()
        },
        whats: Vec::new(),
    };
    let Some(holder) = holder_of(st, a) else {
        return ans;
    };
    ans.facts.flags |= facts::HOLDER;
    let w = &st.phase1.world;
    let k = &w.knowledge;
    let now = w.now();
    let player = k
        .actors()
        .find(ActorKind::Player, uuid_hash(a.player.0, a.player.1));
    ans.facts.sentiment = match (holder.actor, player) {
        (Some(h), Some(p)) => i32::from(k.sentiment(h, p, now)),
        _ => 0,
    };
    if let Some(ex) = holder.node.and_then(|n| st.phase1.market_extremes(n)) {
        if let Some((item, ratio)) = ex.scarcest {
            ans.facts.scarcest_ratio = ratio;
            ans.whats.push((Slot::Scarcest, wire::DICT_ITEM, item));
        }
        if let Some((item, ratio)) = ex.glutted {
            ans.facts.glutted_ratio = ratio;
            ans.whats.push((Slot::Glutted, wire::DICT_ITEM, item));
        }
    }
    let board: Vec<(ReportId, Report)> =
        holder
            .actor
            .and_then(|h| k.board(h))
            .map_or_else(Vec::new, |b| {
                b.iter()
                    .filter_map(|id| k.report(id).map(|r| (id, r)))
                    .collect()
            });
    // The port's state reads the whole board; only the news is thinned (TK84).
    let all: Vec<Report> = board.iter().map(|(_, r)| *r).collect();
    ans.facts.flags |= holder.actor.map_or(0, |h| port_flags(&all, h));
    if a.flags & ask::FACTION != 0 && holder.camp.is_some() {
        // Again with the asker, for the feud: the camp itself cannot have gone since.
        if let Some(ff) = faction_facts(&st.conflict.engine, a.holder, player) {
            let heir_born = k.notable(ff.heir).map(|n| n.born);
            let chief_gone = k.notable(ff.chief).is_none() && k.notable(ff.heir).is_some();
            ans.facts.faction = Some(faction_block(&ff, heir_born, chief_gone));
            ans.facts.flags |= facts::FACTION;
        }
    }
    if a.flags & ask::NEWS == 0 {
        return ans;
    }
    let ctx = Ctx {
        st,
        frame: a.dimension as u16,
        from: (i64::from(a.x), i64::from(a.z)),
        topo: &st.topology.shared,
        now,
    };
    let us: Vec<ActorId> = holder.actor.into_iter().chain(holder.also).collect();
    let tun = k.tunables();
    let mut cands: Vec<Candidate> = newest_per_pair(board)
        .into_iter()
        .map(|r| {
            let flags = talk_flags(&r, player, &us)
                | match holder.pos {
                    Some(h) if ctx.seen_from(h, r.place) => news::SEEN,
                    _ => 0,
                };
            candidate(r, now, tun.lifetime(r.kind), flags)
        })
        .collect();
    rank(&mut cands);
    for (i, c) in cands.iter().enumerate() {
        let r = &c.report;
        let (who, who_what) = ctx.describe(r.who);
        let (whom, whom_what) = ctx.describe(r.whom);
        let (near, near_what) = ctx.near(r.place);
        for (j, w) in [who_what, whom_what, near_what].into_iter().enumerate() {
            if let Some((kind, name)) = w {
                ans.whats.push((Slot::Ref(i, j), kind, name));
            }
        }
        let (dx, dz) = ctx.offset_of(r.place).unwrap_or((0, 0));
        ans.facts.news.push(News {
            kind: r.kind as u16,
            severity: r.severity,
            report_flags: r.flags,
            talk_flags: c.flags,
            ago: u32::try_from(now.0.saturating_sub(r.at.0)).unwrap_or(u32::MAX),
            dx,
            dz,
            who,
            whom,
            near,
        });
    }
    ans
}

// ---- ranking (TK33) -----------------------------------------------------------------------------

/// The board's reports with only the newest of each (kind, who, whom) kept (TK84): a siege records a
/// Raid per wave and a feud a Feud per rung, and without this the four news slots fill with
/// repeats of one story. The newest is the latest `at`; a tie goes to the higher report id, so the
/// choice never depends on the order the board is read in. Every holder, every kind. In
/// (kind, who, whom) order, which the ranking does not depend on.
fn newest_per_pair(board: Vec<(ReportId, Report)>) -> Vec<Report> {
    let mut newest: BTreeMap<(u16, ActorId, ActorId), (Tick, ReportId, Report)> = BTreeMap::new();
    for (id, r) in board {
        let key = (r.kind as u16, r.who, r.whom);
        match newest.get(&key) {
            Some(&(at, held, _)) if (at, held) >= (r.at, id) => {}
            _ => {
                newest.insert(key, (r.at, id, r));
            }
        }
    }
    newest.into_values().map(|(_, _, r)| r).collect()
}

/// One report on the board, scored.
#[derive(Clone, Copy, Debug)]
struct Candidate {
    score: u64,
    flags: u32,
    report: Report,
}

/// A report scored for an answer at `now`.
fn candidate(report: Report, now: Tick, life: u64, flags: u32) -> Candidate {
    let age = now.0.saturating_sub(report.at.0);
    Candidate {
        score: score(&report, age, life, flags),
        flags,
        report,
    }
}

/// How much a kind weighs, in tenths: a fair trade least, a death, a raid, a feud or a port raided
/// most, the rest between. TUNABLE (PLACEHOLDER), in this one place.
pub const fn kind_weight(kind: ReportKind) -> u64 {
    match kind {
        ReportKind::Trade => 3,
        ReportKind::Killing
        | ReportKind::ResidentKilling
        | ReportKind::Raid
        | ReportKind::Fell
        | ReportKind::Feud
        | ReportKind::PortRaided
        | ReportKind::HavenFell => 15,
        _ => 10,
    }
}

/// The severity's part of a score. A severity on `SEVERITY_UNIT` = 100 (their base delta) counts
/// as it is. A small code (a rung, an outcome, 1 or 2 for a port) counts as
/// [`rtsim_knowledge::report::rung_weight`]: 100 plus 25 a step to four, so a rung 4 feud outweighs
/// a rung 1 by 25 percent and a rung 1 outweighs a fair trade. A code is the news kinds' severity
/// always, and any kind's under [`RUNG_LIMIT`] (a guard, TK33). A robbery, a rescue or a killing the
/// conflict records is on the unit scale (MZB-36), so it counts as it is, beside a villager's own:
/// the ranking compares a camp's robbery and a player's like for like. Nothing weighs less than 25.
pub fn severity_term(kind: ReportKind, severity: u8) -> u64 {
    u64::from(if kind.is_news() || severity < RUNG_LIMIT {
        rung_weight(severity)
    } else {
        severity
    })
}

/// Freshness in thousandths: 1,000 at age 0, falling in a straight line to 250 at the end of the
/// kind's life, and 250 after it.
pub fn freshness(age: u64, life: u64) -> u64 {
    if life == 0 {
        return 250;
    }
    1_000 - 750 * age.min(life) / life
}

/// Who the news is about, for the multiplier: 3 about the asking player, 2 about the holder, else 1.
pub const fn about(talk_flags: u32) -> u64 {
    if talk_flags & (news::YOU_DID | news::TO_YOU) != 0 {
        3
    } else if talk_flags & (news::TO_US | news::BY_US) != 0 {
        2
    } else {
        1
    }
}

/// `kind weight x severity x freshness x about`, in whole numbers so a tie is exact.
fn score(r: &Report, age: u64, life: u64, talk_flags: u32) -> u64 {
    kind_weight(r.kind)
        * severity_term(r.kind, r.severity)
        * freshness(age, life)
        * about(talk_flags)
}

/// Best first, at most [`MAX_NEWS`]; a tie goes to the newer report, then to the lower row bytes,
/// so the order never depends on the board's.
fn rank(c: &mut Vec<Candidate>) {
    c.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.report.at.cmp(&a.report.at))
            .then(a.report.pack().0.cmp(&b.report.pack().0))
    });
    c.truncate(MAX_NEWS);
}

/// A news item's talk flags (who and whom, against the asker and `us`: the holder, and for a
/// caravan its home village too, TK36).
fn talk_flags(r: &Report, player: Option<ActorId>, us: &[ActorId]) -> u32 {
    let mut f = 0;
    if player.is_some_and(|p| r.who == p) {
        f |= news::YOU_DID;
    }
    if player.is_some_and(|p| r.whom == p) {
        f |= news::TO_YOU;
    }
    if us.contains(&r.whom) {
        f |= news::TO_US;
    }
    if us.contains(&r.who) {
        f |= news::BY_US;
    }
    f
}

/// PORT_CLOSED and BLOCKADED from the reports about the holder (TALK.md §9.3, TK42): closed when
/// the latest of PortClosed and PortReopened about it is a closure; blockaded when a Blockade about
/// it is newer than both. "About" is `whom`, the port.
fn port_flags(reports: &[Report], holder: ActorId) -> u32 {
    let latest = |kind: ReportKind| {
        reports
            .iter()
            .filter(|r| r.kind == kind && r.whom == holder)
            .map(|r| r.at)
            .max()
    };
    let (closed, reopened, blockade) = (
        latest(ReportKind::PortClosed),
        latest(ReportKind::PortReopened),
        latest(ReportKind::Blockade),
    );
    let mut f = 0;
    if closed.is_some_and(|c| reopened.is_none_or(|r| c > r)) {
        f |= facts::PORT_CLOSED;
    }
    if blockade.is_some_and(|b| closed.is_none_or(|c| b > c) && reopened.is_none_or(|r| b > r)) {
        f |= facts::BLOCKADED;
    }
    f
}

// ---- refs ---------------------------------------------------------------------------------------

/// A ref and the dictionary name its `what` waits for.
type Described = (Ref, Option<(u32, String)>);

fn plain(class: u8, a: u64, b: u64) -> Described {
    (
        Ref {
            class,
            a,
            b,
            ..Ref::default()
        },
        None,
    )
}

fn unknown() -> Described {
    plain(class::UNKNOWN, 0, 0)
}

/// What describing needs: the world, the ask's frame and the talker's block.
struct Ctx<'a> {
    st: &'a State,
    frame: u16,
    from: (i64, i64),
    topo: &'a Topologies,
    now: Tick,
}

impl Ctx<'_> {
    /// Where a block is from the talker, the short way round, in the talker's own chart: the image
    /// of the block nearest the talker (`Topology::toward`), less the talker. Never plain
    /// subtraction (a seam is crossed), and not `displacement` either: that vector is in the
    /// canonical chart, so a talker standing in a turned tile would be sent the wrong way (TK47).
    /// `None` in another frame.
    fn offset_of(&self, p: Place) -> Option<(i32, i32)> {
        self.offset(p.frame, i64::from(p.x), i64::from(p.z))
    }

    fn offset(&self, frame: u16, x: i64, z: i64) -> Option<(i32, i32)> {
        if frame != self.frame {
            return None;
        }
        let (tx, tz) = self.topo.of(frame).toward(self.from, (x, z));
        Some((sat(tx - self.from.0), sat(tz - self.from.1)))
    }

    /// Whether a place is within [`SEEN_BLOCKS`] of a holder at `h`, the short way round.
    fn seen_from(&self, h: (i64, i64), p: Place) -> bool {
        p.frame == self.frame
            && self
                .topo
                .of(p.frame)
                .dist_sq(h, (i64::from(p.x), i64::from(p.z)))
                <= (SEEN_BLOCKS * SEEN_BLOCKS) as u64
    }

    /// Gives a ref its place, when it has one in the ask's frame.
    fn placed(&self, mut d: Described, pos: Pos) -> Described {
        if let Some((dx, dz)) = self.offset(pos.frame, i64::from(pos.x), i64::from(pos.z)) {
            d.0.flags |= reference::POS;
            d.0.dx = dx;
            d.0.dz = dz;
        }
        d
    }

    /// An actor as the module that owns it describes it (TK36, TK37).
    fn describe(&self, a: ActorId) -> Described {
        if a == ActorId::NULL {
            return unknown();
        }
        let w = &self.st.phase1.world;
        let Some(actor) = w.knowledge.actors().get(a) else {
            // The handle no longer validates: the notable it named has died and its actor with it.
            // Nothing says who it was.
            let mut d = unknown();
            d.0.flags |= reference::DEAD;
            return d;
        };
        match actor.kind {
            ActorKind::Player => {
                let (most, least) = w.player_uuid_of(a);
                plain(class::PLAYER, most, least)
            }
            ActorKind::Settlement => self.settlement(actor.external),
            ActorKind::Notable => self.notable(a),
            ActorKind::Group => self.group(actor.external),
        }
    }

    fn culture_of_node(&self, kind: NodeKind) -> Option<(u32, String)> {
        kind.culture()
            .map(|c| (wire::DICT_CULTURE, c.name().to_string()))
    }

    /// A conflict camp by its faction key.
    fn camp(&self, key: u64) -> Option<Described> {
        let c = &self.st.conflict.engine;
        let x = c.factions.get(c.faction_of_key(key)?)?;
        let mut d = plain(class::CAMP, key, 0);
        d.1 = Some((
            wire::DICT_CULTURE,
            c.cultures.get(x.culture).name().to_string(),
        ));
        Some(self.placed(d, x.pos))
    }

    /// A lair by its key (the ecology's, not the actor's).
    fn lair(&self, key: u64) -> Option<(&rtsim_ecology::dragon::Lair, Described)> {
        let e = &self.st.dragons.engine;
        let l = e.lairs.get(e.lair_of_key(key)?)?;
        let mut d = plain(class::LAIR, l.key, 0);
        d.1 = Some((wire::DICT_BODY, l.species.entity_id().to_string()));
        let d = self.placed(d, l.pos);
        Some((l, d))
    }

    /// The live caravan with this seed.
    fn caravan(&self, seed: u64) -> Option<&rtsim_travel::caravan::Caravan> {
        let w = &self.st.phase1.world;
        w.caravans.iter().map(|(_, c)| c).find(|c| c.seed == seed)
    }

    fn caravan_ref(&self, seed: u64) -> Described {
        let w = &self.st.phase1.world;
        match self.caravan(seed) {
            Some(c) => {
                let home = w.nodes.get(c.home).map_or(0, |n| n.key);
                let p = w.caravan_place(c, self.now);
                self.placed(
                    plain(class::CARAVAN, seed, home),
                    Pos::new(p.frame, p.x, p.z),
                )
            }
            None => {
                // The caravan reached home, or was lost: gone.
                let mut d = plain(class::CARAVAN, seed, 0);
                d.0.flags |= reference::DEAD;
                d
            }
        }
    }

    /// A settlement actor: a village, a player base, a haven (a camp's, or a port's).
    fn settlement(&self, key: u64) -> Described {
        if let Some(d) = self.camp(key) {
            return d;
        }
        let l = &self.st.phase1;
        let node = l.node_of(key).or_else(|| l.world.node_of_key(key));
        let Some(n) = node.and_then(|i| l.world.nodes.get(i)) else {
            return unknown();
        };
        let mut d = match n.kind {
            NodeKind::Village(_) => plain(class::VILLAGE, key, 0),
            NodeKind::PlayerBase { owner } => plain(class::BASE, key, owner),
            NodeKind::Haven(_) => plain(class::HAVEN, key, 0),
            // A Mowzie culture's lair has no class yet: LAIR is a dragon lair by the ecology's key,
            // and the lair's Java side waits for its slice (MZB-65).
            NodeKind::Lair => return unknown(),
        };
        if matches!(n.kind, NodeKind::Village(_)) {
            d.1 = self.culture_of_node(n.kind);
        }
        self.placed(d, n.pos)
    }

    /// A notable by what it belongs to: a camp's chief, a caravan's trader, a port's pirate
    /// captain, a lair's dragon.
    fn notable(&self, a: ActorId) -> Described {
        let k = &self.st.phase1.world.knowledge;
        let Some(n) = k.notable(a) else {
            return unknown();
        };
        match k.actors().get(n.home) {
            Some(h) if h.kind == ActorKind::Settlement => match self.camp(h.external) {
                Some((mut r, what)) => {
                    // The chief of a camp: its seed, and the camp's key.
                    r.class = class::CHIEF;
                    r.a = n.seed;
                    r.b = h.external;
                    (r, what)
                }
                None => self.homed_at(n.seed, h.external),
            },
            Some(h) if h.kind == ActorKind::Group => match self.lair(h.external ^ LAIR_TAG) {
                Some((l, (mut r, _))) => {
                    r.class = class::DRAGON;
                    r.a = n.seed;
                    r.b = l.key;
                    (
                        r,
                        Some((wire::DICT_BODY, l.species.entity_id().to_string())),
                    )
                }
                None => unknown(),
            },
            // A dragon with no lair has no home to name it by.
            _ => unknown(),
        }
    }

    /// A notable at home in a settlement that is not a camp, by the settlement's kind: a village's
    /// notable is a caravan's trader; a haven's or a base's is told as the haven or the base, so
    /// "the pirates of X robbed us" and not a caravan that is gone (TK36).
    fn homed_at(&self, seed: u64, key: u64) -> Described {
        let l = &self.st.phase1;
        let node = l.node_of(key).or_else(|| l.world.node_of_key(key));
        match node.and_then(|i| l.world.nodes.get(i)).map(|n| n.kind) {
            Some(NodeKind::Village(_)) => self.caravan_ref(seed),
            Some(_) => self.settlement(key),
            None => unknown(),
        }
    }

    /// A group actor: a camp, a lair, a caravan; the settlers and the oil districts are not
    /// described. A caravan that is gone looks like any other group nobody owns, so it is only
    /// told as a gone caravan while the conflict runs and has had its say about camps; with the
    /// conflict off a camp's group would be told as one (TK36).
    fn group(&self, key: u64) -> Described {
        if key == SETTLERS_KEY {
            return unknown();
        }
        if let Some(d) = self.camp(key) {
            return d;
        }
        if let Some((_, d)) = self.lair(key ^ LAIR_TAG) {
            return d;
        }
        if matches!(key >> 32, DIST_PREFIX | LAIR_PREFIX) {
            return unknown();
        }
        if self.caravan(key).is_some() || self.st.conflict.enabled {
            self.caravan_ref(key)
        } else {
            unknown()
        }
    }

    /// The village nearest a place, within [`NEAR_BLOCKS`], by phase 1's market cells (TK35).
    fn near(&self, p: Place) -> Described {
        let l = &self.st.phase1;
        let keys = l.keys_around(
            self.topo,
            p.frame,
            i64::from(p.x),
            i64::from(p.z),
            NEAR_BLOCKS,
        );
        let t = self.topo.of(p.frame);
        let best = keys
            .into_iter()
            .filter_map(|key| {
                let n = l.world.nodes.get(l.node_of(key)?)?;
                matches!(n.kind, NodeKind::Village(_)).then_some((n, key))
            })
            .map(|(n, key)| {
                let d = t.dist_sq(
                    (i64::from(p.x), i64::from(p.z)),
                    (i64::from(n.pos.x), i64::from(n.pos.z)),
                );
                (d, key, n)
            })
            .filter(|(d, ..)| *d <= (NEAR_BLOCKS * NEAR_BLOCKS) as u64)
            .min_by_key(|(d, key, _)| (*d, *key));
        let Some((_, key, n)) = best else {
            return unknown();
        };
        let mut d = plain(class::VILLAGE, key, 0);
        d.1 = self.culture_of_node(n.kind);
        self.placed(d, n.pos)
    }
}

/// A distance as an i32 (a world is far smaller than that; saturates anyway).
fn sat(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

// ---- the test order -----------------------------------------------------------------------------

/// TEST (opcode 40): a report put on a phase 1 village's noticeboard, as if the village had
/// witnessed it, so slice 2b's GameTest can hand a talker news. Refused (counted, logged) when
/// phase 1 is off, the village is unknown, the kind is not one, an actor does not resolve, or the
/// report would already be expired.
pub(super) fn post(st: &mut State, c: &TalkReportCmd) {
    match post_report(st, c) {
        Ok(()) => st.talk.stats.posted += 1,
        Err(why) => {
            st.talk.stats.post_refused += 1;
            log_warn(&format!("TALK_REPORT refused: {why}"));
        }
    }
}

fn post_report(st: &mut State, c: &TalkReportCmd) -> Result<(), String> {
    // Everything that can be refused without reading an actor comes first: a refused order must
    // not have registered any (TK40).
    if !st.phase1.is_on() {
        return Err("phase 1 is off".into());
    }
    let kind =
        ReportKind::from_u16(c.kind).ok_or_else(|| format!("{} is not a report kind", c.kind))?;
    let (frame, holder) = match st.phase1.node_of(c.village) {
        Some(node) => {
            let n = &st.phase1.world.nodes[node];
            (n.pos.frame, n.actor)
        }
        // A key that is no adopted village but is a camp's: the camp's board, its chief's (TK79).
        None => camp_board(st, c.village)
            .ok_or_else(|| format!("village {} is not adopted", c.village))?,
    };
    for (name, v) in [("x", c.x), ("z", c.z)] {
        if !(-COORD_LIMIT - 1..=COORD_LIMIT).contains(&v) {
            return Err(format!("{name} {v} is past what a report can hold"));
        }
    }
    let now = st.phase1.world.now();
    if u64::from(c.ago) >= st.phase1.world.knowledge.tunables().lifetime(kind) {
        return Err("the report would already be expired".into());
    }
    let at = Tick(now.0.saturating_sub(u64::from(c.ago)));
    let place = Place::new(frame, c.x, 64, c.z);
    let player = c.who.class == class::PLAYER || c.whom.class == class::PLAYER;
    let row = Report {
        who: ActorId::NULL,
        whom: ActorId::NULL,
        kind,
        severity: c.severity,
        flags: c.flags | if player { FLAG_PLAYER } else { 0 },
        at,
        place,
    }
    .pack();
    if !st.phase1.world.knowledge.reports().admits(&row) {
        return Err("the report table is full of reports that matter more".into());
    }
    let who = resolve(st, &c.who);
    let whom = resolve(st, &c.whom);
    // Resolving may have registered actors (a player, a caravan nobody has seen): the world
    // changed outside the input log whether or not the order goes on.
    st.phase1.taint();
    let who = who.ok_or("who does not resolve")?;
    let whom = whom.ok_or("whom does not resolve")?;
    let w = &mut st.phase1.world;
    let id = w.knowledge.record(Report {
        who,
        whom,
        kind,
        severity: c.severity,
        flags: c.flags,
        at,
        place,
    });
    if id == Handle::NULL {
        return Err("the report table is full".into());
    }
    w.knowledge.witness(holder, id, now);
    // A test posts news. If its kind also writes vanilla gossip, nothing is written: the order
    // puts a report on a board, it does not play the village's reaction out. Only this report's
    // writes go; another module's queued gossip stays (TK40).
    w.knowledge.drop_vanilla_gossip_of(id);
    Ok(())
}

/// The board of the camp with this faction key, and its frame: the chief's, else the heir's (the
/// same holder an ask for the camp answers from). `None` while the conflict is off, or for a key
/// that is no camp, or when neither is alive.
fn camp_board(st: &State, key: u64) -> Option<(u16, ActorId)> {
    if !st.conflict.enabled {
        return None;
    }
    let ff = faction_facts(&st.conflict.engine, key, None)?;
    let k = &st.phase1.world.knowledge;
    let holder = [ff.chief, ff.heir]
        .into_iter()
        .find(|c| !c.is_null() && k.is_holder(*c))?;
    Some((ff.pos.frame, holder))
}

/// The actor a test order names, as the engine's own tables know it (a player is registered).
fn resolve(st: &mut State, a: &TalkActor) -> Option<ActorId> {
    if a.class == class::UNKNOWN {
        return Some(ActorId::NULL);
    }
    let l = &mut st.phase1;
    match a.class {
        class::PLAYER => Some(l.world.player_uuid(a.a, a.b)),
        class::VILLAGE | class::BASE => {
            let i = l.node_of(a.a)?;
            Some(l.world.nodes[i].actor)
        }
        class::HAVEN => {
            let i = l.world.node_of_key(a.a)?;
            Some(l.world.nodes[i].actor)
        }
        class::CARAVAN => match l.world.caravans.iter().find(|(_, c)| c.seed == a.a) {
            Some((_, c)) => Some(c.holder),
            // A caravan that is gone: its actor, as a finished caravan leaves it.
            None => Some(
                l.world
                    .knowledge
                    .actors_mut()
                    .register(ActorKind::Group, a.a),
            ),
        },
        class::CAMP => {
            let c = &st.conflict.engine;
            Some(c.factions.get(c.faction_of_key(a.a)?)?.actor)
        }
        class::CHIEF | class::DRAGON => l.world.knowledge.actors().find(ActorKind::Notable, a.a),
        class::LAIR => {
            let e = &st.dragons.engine;
            Some(e.lairs.get(e.lair_of_key(a.a)?)?.actor)
        }
        _ => None,
    }
}

impl super::Engine {
    /// What TALK has counted so far.
    pub fn talk_stats(&self) -> Stats {
        super::lock(&self.state).talk.stats
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rtsim_ecology::dragon::LairKind;
    use rtsim_ecology::engine::NewDragon;
    use rtsim_ecology::species::{Gender, Species};
    use rtsim_knowledge::{Actors, Tunables};
    use rtsim_space::topology::Topology;

    use super::*;
    use crate::talk_wire::{
        TALKED, Talked, encode_asks, news::SIZE as NEWS_SIZE, talked as talked_at,
    };
    use crate::wire::{Records, put_u16, put_u32, put_u64};

    const NOW: u64 = 300_000;
    const DAY: u64 = 24_000;

    fn state() -> State {
        let (_tx, rx) = std::sync::mpsc::channel();
        State::new(rx, None)
    }

    /// A state with villages 7 (human) at (1000, 1000) and 8 (ribbit) at (1100, 1000) in frame 0,
    /// its world run to [`NOW`] so reports can have an age.
    fn world() -> State {
        let mut st = state();
        st.phase1.join_for_test(7, false, 0, 1_000, 1_000);
        st.phase1.join_for_test(8, true, 0, 1_100, 1_000);
        st.phase1.world.run_until(Tick(NOW));
        st
    }

    fn player(n: u64) -> TalkActor {
        TalkActor {
            class: class::PLAYER,
            a: n,
            b: n + 1,
        }
    }

    fn village(key: u64) -> TalkActor {
        TalkActor {
            class: class::VILLAGE,
            a: key,
            b: 0,
        }
    }

    fn cmd(
        kind: ReportKind,
        severity: u8,
        who: TalkActor,
        whom: TalkActor,
        ago: u64,
    ) -> TalkReportCmd {
        TalkReportCmd {
            village: 7,
            kind: kind as u16,
            severity,
            flags: 0,
            who,
            whom,
            x: 1_020,
            z: 1_000,
            ago: ago as u32,
        }
    }

    fn ask_of(id: u64, holder_kind: u32, holder: u64, p: u64, flags: u32) -> Ask {
        Ask {
            id,
            player: (p, p + 1),
            holder_kind,
            dimension: 0,
            holder,
            x: 1_000,
            z: 1_000,
            flags,
        }
    }

    /// Hands the engine an ask record the way a step reads one.
    fn send(st: &mut State, asks: &[Ask]) {
        let bytes = encode_asks(asks);
        let rec = Rec {
            kind: TALK_ASK,
            aux: asks.len() as u32,
            bytes: &bytes,
        };
        observe_state(st, &rec);
    }

    /// Runs the write point on a buffer of `cap` bytes; the TALK_FACTS records written, with their
    /// sequences, and whether the queue wants another buffer.
    fn write(st: &mut State, cap: usize) -> (Vec<(u64, Facts)>, bool) {
        let mut area = vec![0u8; cap];
        let mut w = RecordWriter::new(&mut area);
        let (_, more) = write_queue(st, &mut w);
        let used = w.used();
        let mut out = Vec::new();
        for rec in Records::new(&area[..used]) {
            if rec.kind == crate::talk_wire::TALK_FACTS {
                out.push(Facts::decode(rec.bytes).unwrap());
            }
        }
        (out, more)
    }

    fn answer_one(st: &mut State, a: Ask) -> Facts {
        send(st, &[a]);
        let (mut got, more) = write(st, 8_192);
        assert!(!more);
        assert_eq!(got.len(), 1, "one ask, one answer");
        got.pop().unwrap().1
    }

    // ---- ranking ---------------------------------------------------------------------------

    fn handles() -> [ActorId; 4] {
        let mut a = Actors::new();
        [
            a.register(ActorKind::Player, 1),
            a.register(ActorKind::Settlement, 2),
            a.register(ActorKind::Settlement, 3),
            a.register(ActorKind::Group, 4),
        ]
    }

    fn rep(who: ActorId, whom: ActorId, kind: ReportKind, severity: u8, at: u64) -> Report {
        Report {
            who,
            whom,
            kind,
            severity,
            flags: 0,
            at: Tick(at),
            place: Place::new(0, 0, 64, 0),
        }
    }

    fn id(n: u32) -> ReportId {
        ReportId::from_parts(n, 1)
    }

    #[test]
    fn only_the_newest_report_of_a_kind_between_two_actors_is_kept() {
        let [p, s, t, g] = handles();
        let kept = |board: Vec<(ReportId, Report)>| {
            let mut v: Vec<(u64, ReportKind, ActorId, ActorId)> = newest_per_pair(board)
                .into_iter()
                .map(|r| (r.at.0, r.kind, r.who, r.whom))
                .collect();
            v.sort_by_key(|e| (e.0, e.1 as u16));
            v
        };
        // Two raids by the same camp on the same village: only the newer is told.
        assert_eq!(
            kept(vec![
                (id(1), rep(g, s, ReportKind::Raid, 3, 100)),
                (id(2), rep(g, s, ReportKind::Raid, 2, 900)),
            ]),
            vec![(900, ReportKind::Raid, g, s)]
        );
        // And the newer wins whichever way the board is read.
        assert_eq!(
            kept(vec![
                (id(2), rep(g, s, ReportKind::Raid, 2, 900)),
                (id(1), rep(g, s, ReportKind::Raid, 3, 100)),
            ]),
            vec![(900, ReportKind::Raid, g, s)]
        );
        // The same raider on another village is another story: both are kept.
        assert_eq!(
            kept(vec![
                (id(1), rep(g, s, ReportKind::Raid, 3, 100)),
                (id(2), rep(g, t, ReportKind::Raid, 3, 900)),
            ]),
            vec![(100, ReportKind::Raid, g, s), (900, ReportKind::Raid, g, t)]
        );
        // The same pair under another kind is kept too, and so is the pair the other way round.
        assert_eq!(
            kept(vec![
                (id(1), rep(g, s, ReportKind::Raid, 3, 100)),
                (id(2), rep(g, s, ReportKind::Feud, 2, 200)),
                (id(3), rep(s, g, ReportKind::Raid, 3, 300)),
            ]),
            vec![
                (100, ReportKind::Raid, g, s),
                (200, ReportKind::Feud, g, s),
                (300, ReportKind::Raid, s, g),
            ]
        );
        // Three waves, the middle one learned last: the latest time still wins.
        assert_eq!(
            kept(vec![
                (id(1), rep(g, s, ReportKind::Raid, 3, 100)),
                (id(2), rep(g, s, ReportKind::Raid, 3, 300)),
                (id(3), rep(g, s, ReportKind::Raid, 1, 200)),
            ]),
            vec![(300, ReportKind::Raid, g, s)]
        );
        // The same time: the higher id, in either reading order.
        for board in [
            vec![
                (id(4), rep(g, s, ReportKind::Raid, 1, 500)),
                (id(9), rep(g, s, ReportKind::Raid, 2, 500)),
            ],
            vec![
                (id(9), rep(g, s, ReportKind::Raid, 2, 500)),
                (id(4), rep(g, s, ReportKind::Raid, 1, 500)),
            ],
        ] {
            let got = newest_per_pair(board);
            assert_eq!(got.len(), 1);
            assert_eq!(got[0].severity, 2, "the higher id breaks the tie");
        }
        assert!(newest_per_pair(Vec::new()).is_empty());
        // A player on either side is a pair like any other.
        assert_eq!(
            newest_per_pair(vec![
                (id(1), rep(p, s, ReportKind::Assault, 100, 10)),
                (id(2), rep(p, s, ReportKind::Assault, 100, 20)),
            ])
            .len(),
            1
        );
    }

    /// Through the real answer: a village's board with two raids by one village on it tells the
    /// newer only, and a raid by another stays.
    #[test]
    fn an_answer_tells_the_newest_of_a_repeated_story_only() {
        let mut st = world();
        st.phase1.join_for_test(9, false, 0, 1_200, 1_000);
        // Two waves by village 8 on village 7, the older first, and one by village 9.
        for (ago, severity, by) in [(3_000, 3, 8), (1_000, 2, 8), (2_000, 3, 9)] {
            post(
                &mut st,
                &cmd(ReportKind::Raid, severity, village(by), village(7), ago),
            );
        }
        let a = answer_one(&mut st, ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        let mut told: Vec<(u8, u32)> = a.news.iter().map(|n| (n.severity, n.ago)).collect();
        told.sort();
        assert_eq!(told, vec![(2, 1_000), (3, 2_000)], "{told:?}");
    }

    fn cand(r: Report, now: u64, flags: u32) -> Candidate {
        candidate(r, Tick(now), Tunables::default().lifetime(r.kind), flags)
    }

    fn kinds(c: &[Candidate]) -> Vec<ReportKind> {
        c.iter().map(|c| c.report.kind).collect()
    }

    #[test]
    fn the_four_best_of_more_than_four_are_sent_best_first() {
        let [p, s, ..] = handles();
        let at = 1_000;
        let mut c = vec![
            cand(rep(p, s, ReportKind::Trade, 100, at), at, 0),
            cand(rep(p, s, ReportKind::Assault, 100, at), at, 0),
            cand(rep(p, s, ReportKind::Killing, 100, at), at, 0),
            cand(rep(p, s, ReportKind::Raid, 4, at), at, 0),
            cand(rep(p, s, ReportKind::Feud, 1, at), at, 0),
            cand(rep(p, s, ReportKind::Rescue, 101, at), at, 0),
        ];
        rank(&mut c);
        assert_eq!(
            kinds(&c),
            vec![
                ReportKind::Raid,    // 15 x 200
                ReportKind::Feud,    // 15 x 125
                ReportKind::Killing, // 15 x 100
                ReportKind::Rescue,  // 10 x 101
            ]
        );
    }

    #[test]
    fn a_tie_is_broken_the_same_way_whatever_order_the_board_is_in() {
        let [p, s, t, g] = handles();
        // Past the end of their life every Theft is 250 thousandths fresh, so these score alike
        // and only age and row bytes tell them apart.
        let life = Tunables::default().lifetime(ReportKind::Theft);
        let now = 10 * life;
        let older = rep(p, s, ReportKind::Theft, 100, 1_000);
        let newer = rep(p, s, ReportKind::Theft, 100, 2_000);
        let twin_a = rep(p, t, ReportKind::Theft, 100, 2_000);
        let twin_b = rep(p, g, ReportKind::Theft, 100, 2_000);
        let rows = |v: &[Candidate]| v.iter().map(|c| c.report.pack().0).collect::<Vec<_>>();
        let mut one: Vec<Candidate> = [older, newer, twin_a, twin_b]
            .iter()
            .map(|r| cand(*r, now, 0))
            .collect();
        let mut two: Vec<Candidate> = [twin_b, older, twin_a, newer]
            .iter()
            .map(|r| cand(*r, now, 0))
            .collect();
        assert!(one.iter().all(|c| c.score == one[0].score), "an exact tie");
        rank(&mut one);
        rank(&mut two);
        assert_eq!(rows(&one), rows(&two));
        assert_eq!(one[3].report.at, Tick(1_000), "the oldest is last");
        assert!(
            one[0].report.pack().0 < one[1].report.pack().0
                && one[1].report.pack().0 < one[2].report.pack().0,
            "equals by row bytes, lowest first"
        );
    }

    #[test]
    fn about_the_player_counts_triple_and_about_us_double() {
        let [p, s, ..] = handles();
        let r = rep(p, s, ReportKind::Assault, 100, 0);
        let base = cand(r, 0, 0).score;
        assert_eq!(cand(r, 0, news::YOU_DID).score, 3 * base);
        assert_eq!(cand(r, 0, news::TO_YOU).score, 3 * base);
        assert_eq!(cand(r, 0, news::TO_US).score, 2 * base);
        assert_eq!(cand(r, 0, news::BY_US).score, 2 * base);
        assert_eq!(
            cand(r, 0, news::YOU_DID | news::TO_US).score,
            3 * base,
            "the player outranks us; the multipliers do not stack"
        );
        assert_eq!(
            cand(r, 0, news::SEEN).score,
            base,
            "seen is not about anyone"
        );
    }

    #[test]
    fn freshness_falls_from_one_to_a_quarter_over_a_life() {
        let life = 10 * DAY;
        assert_eq!(freshness(0, life), 1_000);
        assert_eq!(freshness(life / 2, life), 625);
        assert_eq!(freshness(life, life), 250);
        assert_eq!(
            freshness(5 * life, life),
            250,
            "after its life it stays at a quarter"
        );
        let [p, s, ..] = handles();
        let r = rep(p, s, ReportKind::Assault, 100, 0);
        let life = Tunables::default().lifetime(ReportKind::Assault);
        assert_eq!(cand(r, 0, 0).score, 10 * 100 * 1_000);
        assert_eq!(cand(r, life, 0).score, 10 * 100 * 250);
    }

    #[test]
    fn the_news_kinds_count_their_rung_and_todays_count_their_severity() {
        assert_eq!(severity_term(ReportKind::Feud, 1), 125);
        assert_eq!(severity_term(ReportKind::Feud, 4), 200);
        assert_eq!(severity_term(ReportKind::Feud, 9), 200, "past four is four");
        assert_eq!(severity_term(ReportKind::PortClosed, 0), 100);
        assert_eq!(severity_term(ReportKind::Assault, 100), 100);
        assert_eq!(severity_term(ReportKind::Assault, 101), 101);
        // Under 25 a severity of any kind is read as a rung: a guard since MZB-36, when the conflict
        // stopped recording Robbery 2, Rescue 2 and Killing 4 and moved to the unit scale.
        assert_eq!(severity_term(ReportKind::Robbery, 2), 150);
        assert_eq!(severity_term(ReportKind::Killing, 4), 200);
        assert_eq!(severity_term(ReportKind::Assault, 0), 100);
        assert_eq!(severity_term(ReportKind::Assault, 24), 200);
        assert_eq!(severity_term(ReportKind::Assault, 25), 25);
        assert_eq!(kind_weight(ReportKind::Trade), 3);
        assert_eq!(kind_weight(ReportKind::Theft), 10);
        for k in [
            ReportKind::Killing,
            ReportKind::ResidentKilling,
            ReportKind::Raid,
            ReportKind::Fell,
            ReportKind::Feud,
            ReportKind::PortRaided,
            ReportKind::HavenFell,
        ] {
            assert_eq!(kind_weight(k), 15, "{k:?}");
        }
    }

    #[test]
    fn a_rung_coded_robbery_outranks_a_fair_trade_between_strangers() {
        let [p, s, t, _] = handles();
        let at = 5_000;
        let life = Tunables::default().lifetime(ReportKind::Robbery);
        let now = at + life / 4;
        // What the conflict writes: severity 2 is a rung, not a base delta of 2.
        let robbery = cand(rep(p, s, ReportKind::Robbery, 2, at), now, news::TO_US);
        let trade = cand(rep(t, s, ReportKind::Trade, 100, now), now, 0);
        let mut c = vec![trade, robbery];
        rank(&mut c);
        assert_eq!(kinds(&c), vec![ReportKind::Robbery, ReportKind::Trade]);
        // Even a robbery of strangers, a day old, is worth more than a fresh fair trade.
        let robbery = cand(rep(p, s, ReportKind::Robbery, 2, at), now, 0);
        assert!(robbery.score > trade.score);
        // A killing at its rung 4 outweighs a feud at rung 1 of the same age, as a rung should.
        let killing = cand(rep(p, s, ReportKind::Killing, 4, at), now, 0);
        let feud = cand(rep(p, s, ReportKind::Feud, 1, at), now, 0);
        assert!(killing.score > feud.score);
    }

    #[test]
    fn talk_flags_name_who_and_whom() {
        let [p, s, t, _] = handles();
        let f = |who, whom| talk_flags(&rep(who, whom, ReportKind::Assault, 100, 0), Some(p), &[s]);
        assert_eq!(f(p, t), news::YOU_DID);
        assert_eq!(f(t, p), news::TO_YOU);
        assert_eq!(f(t, s), news::TO_US);
        assert_eq!(f(s, t), news::BY_US);
        assert_eq!(f(p, s), news::YOU_DID | news::TO_US);
        assert_eq!(f(s, p), news::BY_US | news::TO_YOU);
        assert_eq!(f(t, t), 0);
        assert_eq!(
            talk_flags(&rep(p, t, ReportKind::Assault, 100, 0), None, &[s]),
            0,
            "nobody who is not known is the player"
        );
    }

    #[test]
    fn a_caravan_and_its_home_village_are_both_us() {
        let [p, caravan, home, other] = handles();
        let us = [caravan, home];
        let f = |who, whom| talk_flags(&rep(who, whom, ReportKind::Robbery, 2, 0), Some(p), &us);
        assert_eq!(f(other, caravan), news::TO_US);
        assert_eq!(
            f(other, home),
            news::TO_US,
            "about the home is about the caravan"
        );
        assert_eq!(f(home, other), news::BY_US);
        assert_eq!(f(caravan, other), news::BY_US);
        assert_eq!(f(other, other), 0);
    }

    #[test]
    fn a_name_longer_than_a_record_is_never_written() {
        let long = "x".repeat(wire::MAX_RECORD);
        assert!(names_fit(&[
            (wire::DICT_ITEM, "oak_log"),
            (wire::DICT_CULTURE, "human")
        ]));
        assert!(names_fit(&[]));
        assert!(!names_fit(&[
            (wire::DICT_ITEM, "oak_log"),
            (wire::DICT_BODY, &long)
        ]));
    }

    #[test]
    fn a_closed_port_is_the_latest_of_closed_and_reopened_and_a_blockade_must_be_newer_than_both() {
        let [_, port, other, haven] = handles();
        let at = |kind, whom, t| rep(haven, whom, kind, 2, t);
        let closed = |t| at(ReportKind::PortClosed, port, t);
        let reopened = |t| at(ReportKind::PortReopened, port, t);
        let blockade = |t| at(ReportKind::Blockade, port, t);
        assert_eq!(port_flags(&[], port), 0);
        assert_eq!(port_flags(&[closed(10)], port), facts::PORT_CLOSED);
        assert_eq!(port_flags(&[closed(10), reopened(20)], port), 0);
        assert_eq!(
            port_flags(&[reopened(10), closed(20)], port),
            facts::PORT_CLOSED
        );
        assert_eq!(
            port_flags(&[closed(10), reopened(10)], port),
            0,
            "a tie is open"
        );
        assert_eq!(port_flags(&[blockade(10)], port), facts::BLOCKADED);
        assert_eq!(
            port_flags(&[closed(10), blockade(20)], port),
            facts::PORT_CLOSED | facts::BLOCKADED,
            "closed, and a blockade since"
        );
        assert_eq!(
            port_flags(&[blockade(10), closed(20)], port),
            facts::PORT_CLOSED
        );
        assert_eq!(port_flags(&[blockade(10), reopened(20)], port), 0);
        assert_eq!(
            port_flags(&[closed(10), reopened(20), blockade(30)], port),
            facts::BLOCKADED
        );
        let elsewhere = at(ReportKind::PortClosed, other, 10);
        assert_eq!(
            port_flags(&[elsewhere], port),
            0,
            "another port's closure is not ours"
        );
    }

    // ---- the protocol ----------------------------------------------------------------------

    #[test]
    fn a_village_holder_answers_with_its_news_end_to_end() {
        let mut st = world();
        st.conflict
            .engine
            .add_faction(
                &mut st.phase1.world,
                55,
                "goblin",
                Pos::new(0, 1_300, 1_000),
                Tick(NOW),
            )
            .expect("goblins are a culture");
        let camp = TalkActor {
            class: class::CAMP,
            a: 55,
            b: 0,
        };
        post(&mut st, &cmd(ReportKind::Raid, 3, camp, village(7), DAY));
        post(
            &mut st,
            &cmd(ReportKind::Assault, 100, player(5), village(7), DAY),
        );
        assert_eq!(st.talk.stats.posted, 2);
        assert_eq!(st.talk.stats.post_refused, 0);

        let f = answer_one(&mut st, ask_of(41, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        assert_eq!(f.ask_id, 41);
        assert_eq!(f.flags & facts::HOLDER, facts::HOLDER);
        assert_eq!(f.flags & (facts::PORT_CLOSED | facts::BLOCKADED), 0);
        assert!(
            f.sentiment < 0,
            "a village that saw the player assault it thinks the worse of them"
        );
        assert_eq!(f.news.len(), 2);

        // 15 x 175 x 2 (about us) beats 10 x 100 x 3 (about the player).
        let (raid, assault) = (&f.news[0], &f.news[1]);
        assert_eq!(raid.kind, ReportKind::Raid as u16);
        assert_eq!(raid.severity, 3);
        assert_eq!(raid.talk_flags, news::TO_US | news::SEEN);
        assert_eq!(raid.ago, DAY as u32);
        assert_eq!(
            (raid.dx, raid.dz),
            (20, 0),
            "from the talker at 1000 to the place at 1020"
        );
        assert_eq!(raid.who.class, class::CAMP);
        assert_eq!(raid.who.a, 55);
        assert!(raid.who.has_pos());
        assert_eq!((raid.who.dx, raid.who.dz), (300, 0));
        assert_ne!(raid.who.what, 0, "the camp's culture is named");
        assert_eq!(raid.whom.class, class::VILLAGE);
        assert_eq!(raid.whom.a, 7);
        assert_ne!(raid.whom.what, 0, "and the village's");
        assert_eq!((raid.whom.dx, raid.whom.dz), (0, 0));
        assert_eq!(raid.near.class, class::VILLAGE);
        assert_eq!(raid.near.a, 7, "20 blocks from village 7 and 80 from 8");

        assert_eq!(assault.kind, ReportKind::Assault as u16);
        assert_eq!(assault.talk_flags, news::YOU_DID | news::TO_US | news::SEEN);
        assert_eq!(assault.who.class, class::PLAYER);
        assert_eq!((assault.who.a, assault.who.b), (5, 6));

        // The two names went through the dictionary, and so did the scarcest good.
        assert!(
            st.cmd_dict
                .contains_key(&(wire::DICT_CULTURE, "goblin".into()))
        );
        assert!(
            st.cmd_dict
                .contains_key(&(wire::DICT_CULTURE, "human".into()))
        );
        // A village with no resources keeps one good with a target: that good is the scarcest, and
        // not also the fullest (TK46).
        assert_ne!(f.scarcest, 0);
        assert_eq!(f.glutted, 0);
        assert_eq!(f.glutted_ratio, 0);
        assert_eq!(st.talk.stats.answered, 1);
        assert_eq!(st.talk.stats.with_holder, 1);
    }

    #[test]
    fn an_ask_is_answered_once_even_when_it_is_sent_again() {
        let mut st = world();
        let a = ask_of(9, ask::holder::VILLAGE, 7, 5, 0);
        send(&mut st, &[a]);
        send(&mut st, &[a]); // the same tick's record twice
        let (first, _) = write(&mut st, 4_096);
        assert_eq!(first.len(), 1, "queued once");
        for _ in 0..3 {
            send(&mut st, &[a]); // Java does not know yet
            let (again, more) = write(&mut st, 4_096);
            assert!(again.is_empty() && !more);
        }
        assert_eq!(st.talk.stats.answered, 1);
        assert_eq!(st.talk.stats.asks, 1);
        // A new ask is answered; a new session forgets the old ids.
        send(&mut st, &[ask_of(10, ask::holder::VILLAGE, 7, 5, 0)]);
        assert_eq!(write(&mut st, 4_096).0.len(), 1);
        session_start(&mut st);
        send(&mut st, &[a]);
        assert_eq!(write(&mut st, 4_096).0.len(), 1, "askIds are per session");
    }

    #[test]
    fn a_full_buffer_keeps_the_ask_for_the_next_one() {
        let mut st = world();
        send(
            &mut st,
            &[
                ask_of(1, ask::holder::VILLAGE, 7, 5, 0),
                ask_of(2, ask::holder::VILLAGE, 7, 5, 0),
            ],
        );
        let one = facts::record_size(0, false) + 2 * wire::align8(wire::cmd::dict::size(16));
        let (got, more) = write(&mut st, one);
        assert!(more, "the second does not fit");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1.ask_id, 1);
        let (got, more) = write(&mut st, 4_096);
        assert!(!more);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1.ask_id, 2, "oldest first");
        assert!(got[0].0 > 0, "a command sequence");
    }

    #[test]
    fn facts_take_the_sessions_command_sequence() {
        let mut st = world();
        send(&mut st, &[ask_of(1, ask::holder::VILLAGE, 7, 5, 0)]);
        send(&mut st, &[ask_of(2, ask::holder::VILLAGE, 7, 5, 0)]);
        let (got, _) = write(&mut st, 8_192);
        assert_eq!(got.len(), 2);
        assert!(got[1].0 > got[0].0, "{} then {}", got[0].0, got[1].0);
    }

    #[test]
    fn malformed_asks_are_refused_and_counted() {
        let mut st = world();
        // askId 0.
        send(&mut st, &[ask_of(0, ask::holder::VILLAGE, 7, 5, 0)]);
        assert_eq!(st.talk.stats.refused, 1);
        // A dimension no frame can be.
        let mut far = ask_of(3, ask::holder::VILLAGE, 7, 5, 0);
        far.dimension = 70_000;
        send(&mut st, &[far]);
        assert_eq!(st.talk.stats.refused, 2);
        // n says two, one is there.
        let mut bytes = encode_asks(&[ask_of(4, ask::holder::VILLAGE, 7, 5, 0)]);
        put_u32(&mut bytes, wire::R_AUX, 2);
        observe_state(
            &mut st,
            &Rec {
                kind: TALK_ASK,
                aux: 2,
                bytes: &bytes,
            },
        );
        assert_eq!(st.talk.stats.refused, 3);
        // A header and nothing else, with a lying n.
        observe_state(
            &mut st,
            &Rec {
                kind: TALK_ASK,
                aux: u32::MAX,
                bytes: &bytes[..8],
            },
        );
        assert_eq!(st.talk.stats.refused, 4);
        let mut huge = bytes.clone();
        put_u32(&mut huge, wire::R_AUX, u32::MAX);
        observe_state(
            &mut st,
            &Rec {
                kind: TALK_ASK,
                aux: u32::MAX,
                bytes: &huge,
            },
        );
        assert_eq!(st.talk.stats.refused, 5, "n x 56 overflows nothing");
        // Nothing was queued, so nothing is answered, and a good ask after them still is.
        assert_eq!(write(&mut st, 4_096).0.len(), 0);
        send(&mut st, &[ask_of(5, ask::holder::VILLAGE, 7, 5, 0)]);
        assert_eq!(write(&mut st, 4_096).0.len(), 1);
        // Another kind is none of TALK's.
        observe_state(
            &mut st,
            &Rec {
                kind: 0x00C1,
                aux: 0,
                bytes: &bytes,
            },
        );
        assert_eq!(st.talk.stats.records, 6);
    }

    #[test]
    fn at_most_256_asks_wait_and_1024_answers_are_remembered() {
        let mut st = world();
        let asks: Vec<Ask> = (1..=300)
            .map(|i| ask_of(i, ask::holder::VILLAGE, 7, 5, 0))
            .collect();
        send(&mut st, &asks);
        assert_eq!(st.talk.pending.len(), MAX_PENDING);
        assert_eq!(st.talk.stats.dropped, 300 - MAX_PENDING as u64);
        for i in 0..(MAX_ANSWERED as u64 + 10) {
            st.talk.remember(1_000 + i);
        }
        assert_eq!(st.talk.answered.len(), MAX_ANSWERED);
        assert!(
            !st.talk.answered.contains(&1_000),
            "the oldest was forgotten"
        );
        assert!(
            st.talk
                .answered
                .contains(&(1_000 + MAX_ANSWERED as u64 + 9))
        );
    }

    #[test]
    fn news_comes_only_when_asked_for() {
        let mut st = world();
        post(
            &mut st,
            &cmd(ReportKind::PortClosed, 2, village(8), village(7), DAY),
        );
        let f = answer_one(&mut st, ask_of(1, ask::holder::VILLAGE, 7, 5, 0));
        assert!(f.news.is_empty());
        assert_eq!(
            f.flags,
            facts::HOLDER | facts::PORT_CLOSED,
            "the port's state is a fact, not news"
        );
        let f = answer_one(&mut st, ask_of(2, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        assert_eq!(f.news.len(), 1);
        assert_eq!(f.news[0].kind, ReportKind::PortClosed as u16);
        assert_eq!(f.news[0].talk_flags, news::TO_US | news::SEEN);
    }

    #[test]
    fn a_holder_the_engine_does_not_know_is_answered_without_holder() {
        let mut st = world();
        for (id, kind, holder) in [
            (1, ask::holder::VILLAGE, 999), // not adopted
            (2, ask::holder::FACTION, 55),  // slice 4
            (3, ask::holder::SHIP, 5),      // slice 4
            (4, 77, 7),                     // a kind this build has not heard of
            (5, ask::holder::CARAVAN, 12),  // no such puppet
        ] {
            let f = answer_one(
                &mut st,
                ask_of(id, kind, holder, 5, ask::NEWS | ask::FACTION),
            );
            assert_eq!(f.ask_id, id);
            assert_eq!(f.flags, 0, "kind {kind}");
            assert_eq!((f.sentiment, f.scarcest, f.glutted), (0, 0, 0));
            assert!(f.news.is_empty());
        }
        assert_eq!(st.talk.stats.answered, 5);
        assert_eq!(st.talk.stats.with_holder, 0);
    }

    #[test]
    fn with_phase_1_off_an_ask_is_answered_at_once_without_holder() {
        let mut st = world();
        st.phase1.set_on_for_test(false);
        let f = answer_one(&mut st, ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        assert_eq!(f.flags, 0);
        assert!(f.news.is_empty());
        st.phase1.set_on_for_test(true);
        let f = answer_one(&mut st, ask_of(2, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        assert_eq!(f.flags, facts::HOLDER);
    }

    #[test]
    fn the_short_way_round_crosses_a_seam() {
        let build = |wrapped: bool| {
            let mut st = state();
            st.phase1.join_for_test(7, false, 0, 9_990, 500);
            st.phase1.world.run_until(Tick(NOW));
            if wrapped {
                let mut t = Topologies::new();
                t.set(0, Topology::x_wrap(0, 10_000));
                st.topology.shared = Arc::new(t);
            }
            // The report's place is 20 blocks across the seam from the village and the talker.
            let mut c = cmd(ReportKind::Raid, 3, village(7), village(7), DAY);
            c.x = 10;
            c.z = 500;
            post(&mut st, &c);
            let mut a = ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS);
            a.x = 9_990;
            a.z = 500;
            answer_one(&mut st, a)
        };
        let across = build(true);
        let n = &across.news[0];
        assert_eq!((n.dx, n.dz), (20, 0), "not 9,980 blocks the other way");
        assert_ne!(
            n.talk_flags & news::SEEN,
            0,
            "20 blocks from the holder across the seam"
        );
        assert_eq!(n.near.a, 7, "the village is near through the seam");
        assert_eq!((n.near.dx, n.near.dz), (0, 0));
        let flat = build(false);
        let n = &flat.news[0];
        assert_eq!(
            (n.dx, n.dz),
            (-9_980, 0),
            "plain subtraction, on a flat world"
        );
        assert_eq!(n.talk_flags & news::SEEN, 0);
        assert_eq!(
            n.near.class,
            class::UNKNOWN,
            "no village within 768 blocks of it"
        );
    }

    #[test]
    fn a_talker_in_a_turned_tile_is_given_directions_in_its_own_chart() {
        // A Petroff-Guyou world of side 1,024: x in [-1024, 1024), z in [-512, 512). North of it
        // (z < -512) is the half-turned image of the tile to the south, and a talker standing there
        // reads its map in that turned chart, where the origin's image is at (-1, -1025).
        let talker = (-101, -1_011);
        let asked = |x: i32, z: i32| {
            let mut st = world();
            let mut t = Topologies::new();
            t.set(0, rtsim_space::topology::layout::guyou(1_024, false));
            st.topology.shared = Arc::new(t);
            let mut c = cmd(ReportKind::Raid, 3, village(7), village(7), DAY);
            c.x = x;
            c.z = z;
            post(&mut st, &c);
            assert_eq!(st.talk.stats.posted, 1);
            let mut a = ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS);
            a.x = talker.0;
            a.z = talker.1;
            let n = answer_one(&mut st, a).news[0];
            (n.dx, n.dz)
        };
        // A place written in the same turned chart is where it looks: 251 east, 21 south.
        assert_eq!(asked(150, -990), (251, 21));
        // The origin of the canonical tile, from the turned chart, is 100 east and 14 north: the
        // canonical vector (-100, 14) turned half a turn. `displacement` would say the reverse.
        assert_eq!(asked(0, 0), (100, -14));
    }

    #[test]
    fn news_from_another_frame_has_no_place_and_is_not_seen() {
        let mut st = world();
        let other = TalkActor {
            class: class::VILLAGE,
            a: 7,
            b: 0,
        };
        post(&mut st, &cmd(ReportKind::Raid, 3, other, other, DAY));
        // The ask is in frame 1 (the nether, say): the village is not there.
        let mut a = ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS);
        a.dimension = 1;
        let f = answer_one(&mut st, a);
        // The holder is a village of frame 0; asked from frame 1 it is still the holder, but
        // nothing it knows has a place in frame 1.
        let n = &f.news[0];
        assert_eq!((n.dx, n.dz), (0, 0));
        assert_eq!(n.talk_flags & news::SEEN, 0);
        assert!(!n.who.has_pos() && !n.whom.has_pos() && !n.near.has_pos());
        assert_eq!(n.who.class, class::VILLAGE);
    }

    // ---- refs ------------------------------------------------------------------------------

    fn ctx(st: &State) -> Ctx<'_> {
        Ctx {
            st,
            frame: 0,
            from: (1_000, 1_000),
            topo: &st.topology.shared,
            now: st.phase1.world.now(),
        }
    }

    #[test]
    fn the_lair_tag_is_the_one_the_ecology_registers_with() {
        let mut st = world();
        let key = 0x1234_5678;
        let id = st.dragons.engine.add_lair(
            &mut st.phase1.world,
            key,
            "Cinder Roost",
            Species::Fire,
            LairKind::Roost,
            Pos::new(0, 1_500, 1_200),
            70,
            Tick(NOW),
        );
        let lair = st.dragons.engine.lairs.get(id).unwrap();
        let actor = lair.actor;
        let external = st
            .phase1
            .world
            .knowledge
            .actors()
            .get(actor)
            .map(|a| (a.kind, a.external))
            .unwrap();
        assert_eq!(external, (ActorKind::Group, LAIR_TAG ^ key));
        let (r, what) = ctx(&st).describe(actor);
        assert_eq!((r.class, r.a), (class::LAIR, key));
        assert_eq!((r.dx, r.dz), (500, 200));
        assert!(r.has_pos());
        assert_eq!(
            what,
            Some((wire::DICT_BODY, "iceandfire:fire_dragon".into()))
        );
    }

    #[test]
    fn a_dragon_is_described_by_its_lair_and_a_chief_by_its_camp() {
        let mut st = world();
        let lair = st.dragons.engine.add_lair(
            &mut st.phase1.world,
            0x77,
            "Frost Cave",
            Species::Ice,
            LairKind::Cave,
            Pos::new(0, 1_000, 1_900),
            70,
            Tick(NOW),
        );
        let dragon = st.dragons.engine.add_dragon(
            &mut st.phase1.world,
            NewDragon {
                name: "Sleet".into(),
                species: Species::Ice,
                gender: Gender::Female,
                variant: 0,
                age_ticks: 30 * DAY,
                lair: Some(lair),
                fullness: 80,
                health: None,
                seed: Some(0xD7A6),
                at: None,
            },
            Tick(NOW),
        );
        let actor = st.dragons.engine.dragon(dragon).unwrap().actor;
        let (r, what) = ctx(&st).describe(actor);
        assert_eq!((r.class, r.a, r.b), (class::DRAGON, 0xD7A6, 0x77));
        assert_eq!((r.dx, r.dz), (0, 900), "at its lair");
        assert_eq!(
            what,
            Some((wire::DICT_BODY, "iceandfire:ice_dragon".into()))
        );

        let f = st
            .conflict
            .engine
            .add_faction(
                &mut st.phase1.world,
                55,
                "bandit",
                Pos::new(0, 600, 1_000),
                Tick(NOW),
            )
            .unwrap();
        let (chief, seed) = {
            let x = st.conflict.engine.factions.get(f).unwrap();
            (x.chief, x.chief_seed)
        };
        let (r, what) = ctx(&st).describe(chief);
        assert_eq!((r.class, r.a, r.b), (class::CHIEF, seed, 55));
        assert_eq!((r.dx, r.dz), (-400, 0));
        assert_eq!(what, Some((wire::DICT_CULTURE, "bandit".into())));
        // The camp's haven node is the camp, not a haven of a port.
        let node = st.phase1.world.node_of_key(55).unwrap();
        let haven = st.phase1.world.nodes[node].actor;
        let (r, _) = ctx(&st).describe(haven);
        assert_eq!((r.class, r.a), (class::CAMP, 55));
    }

    #[test]
    fn a_fallen_notable_is_gone_and_an_unknown_one_is_nobody() {
        let mut st = world();
        let f = st
            .conflict
            .engine
            .add_faction(
                &mut st.phase1.world,
                55,
                "goblin",
                Pos::new(0, 600, 1_000),
                Tick(NOW),
            )
            .unwrap();
        let chief = st.conflict.engine.factions.get(f).unwrap().chief;
        st.phase1.world.knowledge.die(chief, Tick(NOW));
        let (r, what) = ctx(&st).describe(chief);
        assert_eq!(r.class, class::UNKNOWN);
        assert_eq!(r.flags, reference::DEAD, "the handle no longer validates");
        assert_eq!(what, None);
        let (r, _) = ctx(&st).describe(ActorId::NULL);
        assert_eq!((r.class, r.flags), (class::UNKNOWN, 0));
    }

    #[test]
    fn a_caravan_that_is_gone_is_a_dead_caravan_and_players_are_their_uuid() {
        let mut st = world();
        let gone = TalkActor {
            class: class::CARAVAN,
            a: 0xCA7A,
            b: 0,
        };
        post(
            &mut st,
            &cmd(ReportKind::Robbery, 100, player(5), gone, DAY),
        );
        assert_eq!(st.talk.stats.posted, 1);
        let f = answer_one(&mut st, ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        let n = &f.news[0];
        assert_eq!(n.who.class, class::PLAYER);
        assert_eq!((n.who.a, n.who.b), (5, 6));
        assert_eq!((n.whom.class, n.whom.a), (class::CARAVAN, 0xCA7A));
        assert_eq!(n.whom.flags & reference::DEAD, reference::DEAD);
        assert_eq!(n.talk_flags, news::YOU_DID | news::SEEN);
    }

    #[test]
    fn a_player_base_and_a_port_haven_are_described_apart_from_a_village() {
        let mut st = world();
        st.phase1.world.join(
            rtsim_world::NodeData {
                key: 900,
                kind: NodeKind::Haven(rtsim_settle::Culture::Human),
                pos: Pos::new(0, 2_000, 1_000),
                pop: 10,
                resources: Default::default(),
            },
            Tick(NOW),
        );
        let haven = st.phase1.world.nodes[st.phase1.world.node_of_key(900).unwrap()].actor;
        let (r, _) = ctx(&st).describe(haven);
        assert_eq!((r.class, r.a), (class::HAVEN, 900));
        assert_eq!((r.dx, r.dz), (1_000, 0));
        let (r, what) =
            ctx(&st).describe(st.phase1.world.nodes[st.phase1.node_of(7).unwrap()].actor);
        assert_eq!((r.class, r.a), (class::VILLAGE, 7));
        assert_eq!(what, Some((wire::DICT_CULTURE, "human".into())));
        let (r, what) =
            ctx(&st).describe(st.phase1.world.nodes[st.phase1.node_of(8).unwrap()].actor);
        assert_eq!(r.a, 8);
        assert_eq!(what, Some((wire::DICT_CULTURE, "ribbit".into())));
        // A Mowzie culture's lair is nobody yet (MZB-65): LAIR is a dragon lair's class.
        st.phase1.world.join(
            rtsim_world::NodeData {
                key: 901,
                kind: NodeKind::Lair,
                pos: Pos::new(0, 2_400, 1_000),
                pop: 1,
                resources: Default::default(),
            },
            Tick(NOW),
        );
        let lair = st.phase1.world.nodes[st.phase1.world.node_of_key(901).unwrap()].actor;
        assert_eq!(ctx(&st).describe(lair), unknown());
    }

    #[test]
    fn a_pirate_captain_is_the_haven_it_sails_from_and_a_traders_home_is_a_village() {
        let mut st = world();
        st.phase1.world.join(
            rtsim_world::NodeData {
                key: 900,
                kind: NodeKind::Haven(rtsim_settle::Culture::Human),
                pos: Pos::new(0, 2_000, 1_000),
                pop: 10,
                resources: Default::default(),
            },
            Tick(NOW),
        );
        let haven = st.phase1.world.nodes[st.phase1.world.node_of_key(900).unwrap()].actor;
        let v7 = st.phase1.world.nodes[st.phase1.node_of(7).unwrap()].actor;
        let k = &mut st.phase1.world.knowledge;
        let captain = k.create_notable(0xC4, haven, ActorId::NULL, Tick(NOW), 10 * DAY);
        let trader = k.create_notable(0x7A, v7, ActorId::NULL, Tick(NOW), 10 * DAY);
        // Told as the haven, not as a caravan that is gone.
        let (r, _) = ctx(&st).describe(captain);
        assert_eq!((r.class, r.a), (class::HAVEN, 900));
        assert_eq!(r.flags & reference::DEAD, 0);
        assert!(r.has_pos());
        assert_eq!((r.dx, r.dz), (1_000, 0));
        // A village's trader is still a caravan (gone here: nothing travels).
        let (r, _) = ctx(&st).describe(trader);
        assert_eq!((r.class, r.a), (class::CARAVAN, 0x7A));
        assert_eq!(r.flags, reference::DEAD);
        // Through a robbery of the village, the way a player would hear it.
        let pirate = TalkActor {
            class: class::CHIEF,
            a: 0xC4,
            b: 0,
        };
        post(
            &mut st,
            &cmd(ReportKind::Robbery, 2, pirate, village(7), DAY),
        );
        assert_eq!(st.talk.stats.posted, 1);
        let f = answer_one(&mut st, ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS));
        let n = &f.news[0];
        assert_eq!((n.who.class, n.who.a), (class::HAVEN, 900));
        assert_eq!(n.who.flags & reference::DEAD, 0);
        assert!(n.who.has_pos());
    }

    #[test]
    fn a_group_nobody_owns_is_a_gone_caravan_only_while_the_conflict_runs() {
        let mut st = world();
        let k = st.phase1.world.knowledge.actors_mut();
        // A camp's group, as a save from a run with the conflict on leaves it.
        let camp = k.register(ActorKind::Group, 0x55AA);
        // A lair's group, and an oil district's, are never caravans.
        let lair = k.register(ActorKind::Group, LAIR_TAG ^ 77);
        let district = k.register(ActorKind::Group, DIST_PREFIX << 32 | 3);
        let gone = |st: &State, a| {
            let (r, _) = ctx(st).describe(a);
            (r.class, r.flags)
        };
        assert!(st.conflict.enabled, "on unless turned off");
        assert_eq!(gone(&st, camp), (class::CARAVAN, reference::DEAD));
        assert_eq!(gone(&st, lair), (class::UNKNOWN, 0));
        assert_eq!(gone(&st, district), (class::UNKNOWN, 0));
        st.conflict.enabled = false;
        assert_eq!(gone(&st, camp), (class::UNKNOWN, 0), "nobody to tell it by");
        assert_eq!(gone(&st, lair), (class::UNKNOWN, 0));
    }

    // ---- the test order --------------------------------------------------------------------

    #[test]
    fn the_test_order_refuses_what_it_cannot_resolve() {
        let mut st = world();
        let good = cmd(ReportKind::Raid, 3, village(8), village(7), DAY);
        let mut bad = vec![];
        let mut c = good;
        c.village = 999;
        bad.push(c);
        let mut c = good;
        c.kind = 15; // a gap
        bad.push(c);
        let mut c = good;
        c.kind = 0;
        bad.push(c);
        let mut c = good;
        c.who = village(999);
        bad.push(c);
        let mut c = good;
        c.whom = TalkActor {
            class: class::CAMP,
            a: 404,
            b: 0,
        };
        bad.push(c);
        let mut c = good;
        c.who = TalkActor {
            class: class::LAIR,
            a: 404,
            b: 0,
        };
        bad.push(c);
        let mut c = good;
        c.who = TalkActor {
            class: class::SHIP,
            a: 1,
            b: 0,
        };
        bad.push(c);
        let mut c = good;
        c.who = TalkActor {
            class: 99,
            a: 1,
            b: 0,
        };
        bad.push(c);
        let mut c = good;
        c.ago = (30 * DAY) as u32; // a raid's news lasts 20 days
        bad.push(c);
        for (i, c) in bad.iter().enumerate() {
            post(&mut st, c);
            assert_eq!(st.talk.stats.post_refused, i as u64 + 1, "order {i}: {c:?}");
        }
        assert_eq!(st.talk.stats.posted, 0);
        let k = &st.phase1.world.knowledge;
        let me = k.actors().find(ActorKind::Settlement, 7).unwrap();
        assert!(k.board(me).unwrap().is_empty(), "nothing reached the board");
        post(&mut st, &good);
        assert_eq!(st.talk.stats.posted, 1);
        let k = &st.phase1.world.knowledge;
        assert_eq!(k.board(me).unwrap().len(), 1);
        // Switched off, the order does nothing either.
        st.phase1.set_on_for_test(false);
        post(&mut st, &good);
        assert_eq!(st.talk.stats.post_refused, bad.len() as u64 + 1);
    }

    #[test]
    fn an_order_refused_before_it_reads_an_actor_changes_nothing() {
        let mut st = world();
        assert!(!st.phase1.tainted_for_test(), "a fresh world is clean");
        let actors = st.phase1.world.knowledge.actors().len();
        // Each names a player nobody has met; none gets as far as registering them.
        let good = cmd(ReportKind::Raid, 3, player(9), village(7), DAY);
        let edits: [fn(&mut TalkReportCmd); 8] = [
            |c| c.village = 999,
            |c| c.kind = 0,
            |c| c.kind = 15,
            |c| c.ago = (30 * DAY) as u32,
            |c| c.x = COORD_LIMIT + 1,
            |c| c.x = -COORD_LIMIT - 2,
            |c| c.z = COORD_LIMIT + 1,
            |c| c.z = i32::MIN,
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut c = good;
            edit(&mut c);
            post(&mut st, &c);
            assert_eq!(st.talk.stats.post_refused, i as u64 + 1, "order {i}: {c:?}");
            assert!(
                !st.phase1.tainted_for_test(),
                "order {i} left the world alone"
            );
            assert_eq!(
                st.phase1.world.knowledge.actors().len(),
                actors,
                "order {i}"
            );
        }
        // Phase 1 off is a refusal too, and just as clean.
        st.phase1.set_on_for_test(false);
        post(&mut st, &good);
        assert!(!st.phase1.tainted_for_test());
        st.phase1.set_on_for_test(true);
        // The far corners of the pack's range are fine.
        for (x, z) in [
            (COORD_LIMIT, COORD_LIMIT),
            (-COORD_LIMIT - 1, -COORD_LIMIT - 1),
        ] {
            let mut c = good;
            c.x = x;
            c.z = z;
            post(&mut st, &c);
        }
        assert_eq!(st.talk.stats.posted, 2);
    }

    #[test]
    fn an_order_that_registered_an_actor_taints_the_world_even_when_refused() {
        // The player is registered; the village that is `whom` is not there.
        let mut st = world();
        let actors = st.phase1.world.knowledge.actors().len();
        post(
            &mut st,
            &cmd(ReportKind::Raid, 3, player(9), village(999), DAY),
        );
        assert_eq!(st.talk.stats.post_refused, 1);
        assert_eq!(st.phase1.world.knowledge.actors().len(), actors + 1);
        assert!(
            st.phase1.tainted_for_test(),
            "the next save takes a snapshot"
        );
        // And an order that goes through taints it as well.
        let mut st = world();
        post(
            &mut st,
            &cmd(ReportKind::Raid, 3, player(9), village(7), DAY),
        );
        assert_eq!(st.talk.stats.posted, 1);
        assert!(st.phase1.tainted_for_test());
    }

    #[test]
    fn an_order_the_full_table_would_drop_is_refused_before_it_reads_an_actor() {
        let mut st = state();
        st.phase1.small_report_table_for_test(2);
        st.phase1.join_for_test(7, false, 0, 1_000, 1_000);
        st.phase1.world.run_until(Tick(NOW));
        // Two reports about players fill it.
        for ago in [1_000, 2_000] {
            post(
                &mut st,
                &cmd(ReportKind::Assault, 100, player(5), village(7), ago),
            );
        }
        assert_eq!(st.talk.stats.posted, 2);
        st.phase1.untaint_for_test();
        // Older than both, and about a player nobody has met: the table would drop it, so it
        // never reads the player.
        let actors = st.phase1.world.knowledge.actors().len();
        post(
            &mut st,
            &cmd(ReportKind::Raid, 3, player(9), village(7), 10 * DAY),
        );
        assert_eq!(st.talk.stats.post_refused, 1);
        assert_eq!(st.phase1.world.knowledge.actors().len(), actors);
        assert!(!st.phase1.tainted_for_test());
        // Newer than the least important, it takes its place.
        post(
            &mut st,
            &cmd(ReportKind::Raid, 3, player(9), village(7), 100),
        );
        assert_eq!(st.talk.stats.posted, 3);
        assert!(st.phase1.tainted_for_test());
    }

    #[test]
    fn a_news_kind_writes_no_sentiment_and_no_gossip() {
        let mut st = world();
        let me = st.phase1.world.player_uuid(5, 6);
        let v7 = st.phase1.world.nodes[st.phase1.node_of(7).unwrap()].actor;
        let now = st.phase1.world.now();
        let place = Place::new(0, 1_020, 64, 1_000);
        let make = |kind, severity| Report {
            who: me,
            whom: v7,
            kind,
            severity,
            flags: 0,
            at: now,
            place,
        };
        // The real path, not the test order (which drops gossip itself): the village sees the
        // player feud with it. The player is `who`, so a kind with a delta would sentiment.
        let k = &mut st.phase1.world.knowledge;
        let id = k.record(make(ReportKind::Feud, 2));
        assert_ne!(id, Handle::NULL);
        assert!(k.witness(v7, id, now));
        assert!(
            k.take_vanilla_gossip().is_empty(),
            "a news kind has no vanilla mapping"
        );
        let ask = |id| ask_of(id, ask::holder::VILLAGE, 7, 5, ask::NEWS);
        let f = answer_one(&mut st, ask(1));
        assert_eq!(f.sentiment, 0, "and no delta");
        assert_eq!(f.news[0].talk_flags & news::YOU_DID, news::YOU_DID);
        // The same path with an old kind does write both: the checks above can fail.
        let k = &mut st.phase1.world.knowledge;
        let id = k.record(make(ReportKind::Assault, 100));
        assert!(k.witness(v7, id, now));
        assert_eq!(k.take_vanilla_gossip().len(), 1, "an assault is gossip");
        let f = answer_one(&mut st, ask(2));
        assert!(f.sentiment < 0);
    }

    #[test]
    fn a_test_order_keeps_gossip_other_modules_queued() {
        let mut st = world();
        let v7 = st.phase1.world.nodes[st.phase1.node_of(7).unwrap()].actor;
        let now = st.phase1.world.now();
        let other = st.phase1.world.player_uuid(8, 9);
        let k = &mut st.phase1.world.knowledge;
        let theirs = k.record(Report {
            who: other,
            whom: v7,
            kind: ReportKind::Assault,
            severity: 100,
            flags: 0,
            at: now,
            place: Place::new(0, 1_000, 64, 1_000),
        });
        assert!(k.witness(v7, theirs, now));
        // The test order is an assault by another player, which would queue gossip of its own.
        post(
            &mut st,
            &cmd(ReportKind::Assault, 100, player(5), village(7), DAY),
        );
        assert_eq!(st.talk.stats.posted, 1);
        let queued = st.phase1.world.knowledge.take_vanilla_gossip();
        assert_eq!(queued.len(), 1, "only the other module's write is left");
        assert_eq!(queued[0].report, theirs);
        assert_eq!(queued[0].player, other);
    }

    #[test]
    fn four_news_items_make_a_528_byte_record() {
        let mut st = world();
        for (i, k) in [
            ReportKind::Assault,
            ReportKind::Killing,
            ReportKind::Raid,
            ReportKind::Feud,
            ReportKind::Fell,
            ReportKind::Theft,
        ]
        .into_iter()
        .enumerate()
        {
            post(
                &mut st,
                &cmd(k, 2, village(8), village(7), (i as u64 + 1) * 1_000),
            );
        }
        send(&mut st, &[ask_of(1, ask::holder::VILLAGE, 7, 5, ask::NEWS)]);
        let mut area = vec![0u8; 8_192];
        let mut w = RecordWriter::new(&mut area);
        write_queue(&mut st, &mut w);
        let used = w.used();
        let facts_len = Records::new(&area[..used])
            .find(|r| r.kind == crate::talk_wire::TALK_FACTS)
            .map(|r| r.bytes.len())
            .unwrap();
        assert_eq!(facts_len, facts::NEWS + MAX_NEWS * NEWS_SIZE);
        assert_eq!(facts_len, 528);
    }

    // ---- TALKED (docs/boundary.md §13.7) ---------------------------------------------------

    fn talked_bytes(village: u64, p: u64, op: u64) -> Vec<u8> {
        Talked {
            obs_seq: 1,
            game_time: NOW as i64,
            op_id: op,
            village,
            player: (p, p + 1),
            body: 0,
        }
        .encode()
    }

    /// Hands phase 1 a TALKED record `Replay` admitted, as the step's journaled point does.
    fn journal_talked(st: &mut State, bytes: &[u8]) {
        let rec = Rec {
            kind: TALKED,
            aux: 0,
            bytes,
        };
        assert!(super::super::phase1::claims(&rec), "phase 1 claims TALKED");
        super::super::phase1::journaled(st, &rec);
    }

    #[test]
    fn a_talked_adds_three_to_its_villages_feeling_and_the_next_ask_hears_it() {
        let mut st = world();
        let ask = |id| ask_of(id, ask::holder::VILLAGE, 8, 5, 0);
        assert_eq!(answer_one(&mut st, ask(1)).sentiment, 0, "strangers");
        assert!(
            st.phase1
                .world
                .knowledge
                .actors()
                .find(ActorKind::Player, uuid_hash(5, 6))
                .is_none(),
            "an ask never creates the player's actor (TK45)"
        );
        journal_talked(&mut st, &talked_bytes(8, 5, 0xAA));
        assert_eq!(st.talk.stats.talked, 1);
        assert_eq!(st.talk.stats.talked_refused, 0);
        let f = answer_one(&mut st, ask(2));
        assert_eq!(f.flags & facts::HOLDER, facts::HOLDER);
        assert_eq!(f.sentiment, 3, "TALK_CREDIT");
        assert_eq!(
            rtsim_world::bridge::TALK_CREDIT,
            3,
            "TK2: the tunable is the one the brief names"
        );
        // Another player's feeling is untouched, and so is another village's.
        let other = ask_of(3, ask::holder::VILLAGE, 8, 50, 0);
        assert_eq!(answer_one(&mut st, other).sentiment, 0);
        let elsewhere = ask_of(4, ask::holder::VILLAGE, 7, 5, 0);
        assert_eq!(answer_one(&mut st, elsewhere).sentiment, 0);
        // A second talk (another ribbit of the village, another op) adds again.
        journal_talked(&mut st, &talked_bytes(8, 5, 0xAB));
        assert_eq!(answer_one(&mut st, ask(5)).sentiment, 6);
    }

    #[test]
    fn a_talked_that_cannot_be_applied_is_refused_and_counted_never_applied() {
        let mut st = world();
        let ask = |id| ask_of(id, ask::holder::VILLAGE, 8, 5, 0);
        let good = talked_bytes(8, 5, 0xAA);
        // Truncated and oversized records.
        journal_talked(&mut st, &good[..56]);
        let mut big = good.clone();
        big.extend_from_slice(&[0; 8]);
        put_u16(&mut big, wire::R_SIZE, 72);
        journal_talked(&mut st, &big);
        assert_eq!(st.talk.stats.talked_refused, 2);
        // A village the world does not know, and a key that is a base, not a village.
        journal_talked(&mut st, &talked_bytes(99, 5, 0xAB));
        assert_eq!(st.talk.stats.talked_refused, 3);
        st.phase1
            .input_for_test(rtsim_world::inputs::Input::MarkBase {
                key: 1 << 62 | 9,
                frame: 0,
                x: 10,
                z: 10,
                player: (5, 6),
            });
        journal_talked(&mut st, &talked_bytes(1 << 62 | 9, 5, 0xAC));
        assert_eq!(st.talk.stats.talked_refused, 4, "a base has no ribbits");
        // The nil player.
        let mut nil = good.clone();
        put_u64(&mut nil, talked_at::PLAYER_MOST, 0);
        put_u64(&mut nil, talked_at::PLAYER_LEAST, 0);
        journal_talked(&mut st, &nil);
        assert_eq!(st.talk.stats.talked_refused, 5);
        assert_eq!(st.talk.stats.talked, 0, "none applied");
        assert_eq!(answer_one(&mut st, ask(1)).sentiment, 0);
        // The refused inputs left no trace in the log (a replay would apply nothing either).
        assert!(
            !st.phase1.logged_talked_for_test(),
            "a refused TALKED is not logged"
        );
        // And the same record, whole, is applied: the refusals were the records' fault.
        journal_talked(&mut st, &good);
        assert_eq!(st.talk.stats.talked, 1);
        assert_eq!(answer_one(&mut st, ask(2)).sentiment, 3);
    }

    #[test]
    fn a_talked_of_a_scripted_village_is_left_alone() {
        let mut st = world();
        st.phase1.script_for_test(8);
        journal_talked(&mut st, &talked_bytes(8, 5, 0xAA));
        assert_eq!(st.talk.stats.talked, 0);
        assert_eq!(st.talk.stats.talked_refused, 0, "skipped, not refused");
        let a = ask_of(1, ask::holder::VILLAGE, 8, 5, 0);
        assert_eq!(answer_one(&mut st, a).sentiment, 0);
    }
    // ---- a camp (slice 4, TK71 to TK84) ------------------------------------------------------

    /// A state with the two villages and the goblin camp 55 at (1300, 1000), found at [`NOW`];
    /// the camp's index.
    fn camp_world() -> (State, u32) {
        let mut st = world();
        let f = st
            .conflict
            .engine
            .add_faction(
                &mut st.phase1.world,
                55,
                "goblin",
                Pos::new(0, 1_300, 1_000),
                Tick(NOW),
            )
            .expect("goblins are a culture");
        (st, f)
    }

    fn camp_actor(key: u64) -> TalkActor {
        TalkActor {
            class: class::CAMP,
            a: key,
            b: 0,
        }
    }

    /// An order posting onto the board of the holder `key` names (a village or a camp).
    fn cmd_on(key: u64, c: TalkReportCmd) -> TalkReportCmd {
        TalkReportCmd { village: key, ..c }
    }

    const CAMP_ASK: u32 = ask::NEWS | ask::FACTION;

    /// A camp's answer reads its chief's board: news from the camp's side is by us, news of the
    /// camp's own losses to us, the player's own acts toward it are told as theirs, and the block
    /// follows the news.
    #[test]
    fn a_camp_answers_with_its_chiefs_news_and_the_faction_block() {
        let (mut st, f) = camp_world();
        post(
            &mut st,
            &cmd_on(
                55,
                cmd(ReportKind::Raid, 3, camp_actor(55), village(7), DAY),
            ),
        );
        post(
            &mut st,
            &cmd_on(
                55,
                cmd(ReportKind::Assault, 100, player(5), camp_actor(55), DAY),
            ),
        );
        assert_eq!(st.talk.stats.posted, 2, "a camp's board takes TEST posts");
        assert_eq!(st.talk.stats.post_refused, 0);

        let f_ = answer_one(&mut st, ask_of(41, ask::holder::FACTION, 55, 5, CAMP_ASK));
        assert_eq!(f_.ask_id, 41);
        assert_eq!(f_.flags, facts::HOLDER | facts::FACTION);
        assert!(
            f_.sentiment < 0,
            "the chief saw the player assault the camp: {}",
            f_.sentiment
        );
        assert_eq!((f_.scarcest, f_.glutted), (0, 0), "a camp talks no prices");
        assert_eq!(f_.news.len(), 2);
        let by_us = f_
            .news
            .iter()
            .find(|n| n.kind == ReportKind::Raid as u16)
            .unwrap();
        assert_eq!(by_us.talk_flags & (news::BY_US | news::TO_US), news::BY_US);
        assert_eq!((by_us.who.class, by_us.who.a), (class::CAMP, 55));
        assert_eq!((by_us.whom.class, by_us.whom.a), (class::VILLAGE, 7));
        let to_us = f_
            .news
            .iter()
            .find(|n| n.kind == ReportKind::Assault as u16)
            .unwrap();
        assert_eq!(
            to_us.talk_flags & (news::YOU_DID | news::TO_US | news::BY_US),
            news::YOU_DID | news::TO_US,
            "the camp itself is us, whatever the chief or the heir"
        );

        let b = f_.faction.expect("the ask wanted the block");
        let x = &st.conflict.engine.factions[f as usize];
        assert_eq!((b.rung, b.signal), (0, facts::faction::NO_SIGNAL));
        assert_eq!(b.grievance, 0.0);
        assert_eq!((b.cowed_ticks, b.plunders), (0, x.plunders));
        assert_eq!(b.chief, Ref::default(), "M9's, never set yet");
        assert_eq!(
            b.mood,
            facts::faction::mood::STRONG,
            "a new camp has all its warriors and nothing else to say"
        );
        assert_eq!(st.talk.stats.with_faction, 1);
        assert_eq!(st.talk.stats.with_holder, 1);
        // The record carries the block after the news, and nothing else changed.
        assert_eq!(f_.size(), facts::record_size(2, true), "48 + 2 x 120 + 48");
    }

    #[test]
    fn the_faction_block_comes_only_when_asked_for_and_only_for_a_camp() {
        let (mut st, _) = camp_world();
        let plain = answer_one(&mut st, ask_of(1, ask::holder::FACTION, 55, 5, ask::NEWS));
        assert_eq!(
            plain.flags,
            facts::HOLDER,
            "no FACTION without the ask flag"
        );
        assert_eq!(plain.faction, None);
        assert_eq!(plain.size(), facts::record_size(0, false));
        // The block alone, without news.
        let only = answer_one(
            &mut st,
            ask_of(2, ask::holder::FACTION, 55, 5, ask::FACTION),
        );
        assert_eq!(only.flags, facts::HOLDER | facts::FACTION);
        assert!(only.news.is_empty());
        assert!(only.faction.is_some());
        assert_eq!(only.size(), 96);
        // A village asked with the flag gets none: the flag is meant for camps.
        let v = answer_one(&mut st, ask_of(3, ask::holder::VILLAGE, 7, 5, CAMP_ASK));
        assert_eq!(v.flags, facts::HOLDER);
        assert_eq!(v.faction, None);
        // Nor does a caravan or a camp that is no camp.
        let nobody = answer_one(&mut st, ask_of(4, ask::holder::FACTION, 56, 5, CAMP_ASK));
        assert_eq!((nobody.flags, nobody.faction), (0, None));
        assert_eq!(st.talk.stats.with_faction, 1);
    }

    #[test]
    fn a_camp_is_unknown_while_the_conflict_is_off() {
        let (mut st, _) = camp_world();
        st.conflict.enabled = false;
        let f = answer_one(&mut st, ask_of(1, ask::holder::FACTION, 55, 5, CAMP_ASK));
        assert_eq!(f.flags, 0);
        assert_eq!(f.faction, None);
        assert!(f.news.is_empty());
        // And the TEST order has no camp's board to post on.
        post(
            &mut st,
            &cmd_on(
                55,
                cmd(ReportKind::Raid, 3, camp_actor(55), village(7), DAY),
            ),
        );
        assert_eq!(st.talk.stats.post_refused, 1);
        st.conflict.enabled = true;
        let f = answer_one(&mut st, ask_of(2, ask::holder::FACTION, 55, 5, CAMP_ASK));
        assert_eq!(f.flags, facts::HOLDER | facts::FACTION);
    }

    #[test]
    fn a_camp_with_no_chief_answers_from_its_heir_and_with_neither_from_nobody() {
        let (mut st, f) = camp_world();
        post(
            &mut st,
            &cmd_on(
                55,
                cmd(ReportKind::Raid, 3, camp_actor(55), village(7), DAY),
            ),
        );
        let (chief, heir) = {
            let x = &st.conflict.engine.factions[f as usize];
            (x.chief, x.heir)
        };
        // The chief falls, and what it held (the raid it told of) passes to the heir.
        st.phase1.world.knowledge.die(chief, Tick(NOW));
        let a = answer_one(&mut st, ask_of(1, ask::holder::FACTION, 55, 5, CAMP_ASK));
        assert_eq!(a.flags, facts::HOLDER | facts::FACTION);
        assert_eq!(a.news.len(), 1, "the heir inherited the chief's board");
        assert_eq!(a.news[0].talk_flags & news::BY_US, news::BY_US);
        // The heir too: the camp is still known, with nothing to tell.
        st.phase1.world.knowledge.die(heir, Tick(NOW));
        let n = answer_one(&mut st, ask_of(2, ask::holder::FACTION, 55, 5, CAMP_ASK));
        assert_eq!(n.flags, facts::HOLDER | facts::FACTION);
        assert_eq!(n.sentiment, 0);
        assert!(n.news.is_empty());
        assert!(n.faction.is_some());
    }

    /// A feud against the asker shows in the block; another player is a stranger to it.
    #[test]
    fn the_block_carries_the_feud_against_the_asker_and_not_against_another() {
        use rtsim_conflict::SignalKind;
        use rtsim_conflict::engine::Signal;
        let (mut st, _) = camp_world();
        // The player is known to phase 1 once something names them.
        post(
            &mut st,
            &cmd_on(
                55,
                cmd(ReportKind::Assault, 100, player(5), camp_actor(55), DAY),
            ),
        );
        let uuid = (5, 6);
        let p = st.phase1.world.player_uuid(uuid.0, uuid.1);
        let w = &mut st.phase1.world;
        st.conflict.engine.player(w, uuid);
        st.conflict.engine.signal(
            w,
            Signal {
                faction: Some(55),
                kind: SignalKind::Killing,
                amount: 3.0,
                actor: p,
                pos: Pos::new(0, 1_300, 1_000),
                district: None,
            },
            Tick(NOW),
        );
        let b = answer_one(
            &mut st,
            ask_of(1, ask::holder::FACTION, 55, 5, ask::FACTION),
        )
        .faction
        .unwrap();
        let view = st.conflict.engine.feud_view(55, p).unwrap();
        assert_eq!(b.rung, view.step as u8);
        assert_eq!(b.signal, SignalKind::Killing as u16);
        assert!(b.grievance > 0.0);
        assert!((f64::from(b.grievance) - view.grievance).abs() < 1e-3 * view.grievance);
        // A player the camp has no feud with.
        let q = answer_one(
            &mut st,
            ask_of(2, ask::holder::FACTION, 55, 9, ask::FACTION),
        )
        .faction
        .unwrap();
        assert_eq!((q.rung, q.signal, q.grievance), (0, 0xFFFF, 0.0));
    }

    /// The moods of a camp, read at the engine's time, through the real tables.
    #[test]
    fn the_block_gives_a_camps_moods_through_the_real_engine() {
        use facts::faction::mood;
        let (mut st, f) = camp_world();
        let ask = |id| ask_of(id, ask::holder::FACTION, 55, 5, ask::FACTION);
        let moods = |st: &mut State, id| answer_one(st, ask(id)).faction.unwrap();
        // Cowed for 5,000 more ticks, with the war chief out, the pool a quarter of its cap.
        {
            let x = &mut st.conflict.engine.factions[f as usize];
            x.cowed_until = Tick(NOW + 5_000);
            x.chief_out = true;
            x.pool = 10.0;
            x.pool_at = Tick(NOW);
        }
        let b = moods(&mut st, 1);
        assert_eq!(b.mood, mood::COWED | mood::CHIEF_OUT | mood::LEAN);
        assert_eq!(b.cowed_ticks, 5_000);
        // A succession in the last ten days: a new chief.
        st.conflict.engine.factions[f as usize].chiefs = 3;
        let b = moods(&mut st, 2);
        assert_eq!(
            b.mood,
            mood::COWED | mood::CHIEF_OUT | mood::LEAN | mood::NEW_CHIEF
        );
        // The cowing is over, the war chief is back and the pool has recruited back to its cap; the
        // ten days' edge of a new chief is `a_chief_is_new_for_ten_days...`'s (the block reads the
        // conflict's own time, which only the conflict's events move).
        {
            let x = &mut st.conflict.engine.factions[f as usize];
            x.cowed_until = Tick(NOW);
            x.chief_out = false;
            x.chiefs = 2;
            x.pool = 60.0;
            x.pool_at = Tick(NOW);
        }
        let b = moods(&mut st, 3);
        assert_eq!(b.mood, mood::STRONG);
        assert_eq!(b.cowed_ticks, 0);
    }

    // The moods themselves, at their edges.

    fn facts_of(chiefs: u32, pool: f64, cap: f64) -> FactionFacts {
        let (st, _) = camp_world();
        let mut ff = faction_facts(&st.conflict.engine, 55, None).unwrap();
        ff.chiefs = chiefs;
        ff.pool = pool;
        ff.pool_cap = cap;
        ff.now = Tick(NOW);
        ff
    }

    #[test]
    fn lean_is_under_a_third_of_the_cap_and_strong_is_the_cap_within_half_a_warrior() {
        use facts::faction::mood::{LEAN, STRONG};
        let m = |pool, cap| faction_block(&facts_of(2, pool, cap), None, false).mood;
        assert_eq!(m(19.9, 60.0), LEAN);
        assert_eq!(m(20.0, 60.0), 0, "exactly a third is not lean");
        assert_eq!(m(0.0, 60.0), LEAN);
        assert_eq!(m(59.5, 60.0), STRONG);
        assert_eq!(m(59.4, 60.0), 0);
        assert_eq!(m(60.0, 60.0), STRONG);
        // A cap under a warrior and a half: the ranges meet, and lean wins; never both.
        assert_eq!(m(0.0, 1.0), LEAN);
        assert_eq!(m(0.2, 1.0), LEAN);
        assert_eq!(m(0.5, 1.0), STRONG);
        // A culture with no cap has neither.
        assert_eq!(m(0.0, 0.0), 0);
    }

    #[test]
    fn a_chief_is_new_for_ten_days_after_a_succession_and_never_before_one() {
        use facts::faction::mood::NEW_CHIEF;
        let m = |chiefs, born: Option<u64>| {
            faction_block(&facts_of(chiefs, 30.0, 60.0), born.map(Tick), false).mood & NEW_CHIEF
        };
        assert_eq!(m(3, Some(NOW)), NEW_CHIEF);
        assert_eq!(
            m(3, Some(NOW + 1)),
            NEW_CHIEF,
            "a heir born in the future: not past"
        );
        assert_eq!(m(3, Some(NOW - NEW_CHIEF_TICKS + 1)), NEW_CHIEF);
        assert_eq!(m(3, Some(NOW - NEW_CHIEF_TICKS)), 0);
        assert_eq!(
            m(2, Some(NOW)),
            0,
            "the founding chief was never succeeded to"
        );
        assert_eq!(m(3, None), 0, "a heir that is gone says nothing");
        assert_eq!(NEW_CHIEF_TICKS, 10 * DAY);
        // A chief who has died with the heir answering: new, whatever `chiefs` and the heir's birth
        // still say (the succession is lazy).
        let gone = |chiefs, born: Option<u64>| {
            faction_block(&facts_of(chiefs, 30.0, 60.0), born.map(Tick), true).mood & NEW_CHIEF
        };
        assert_eq!(gone(2, Some(0)), NEW_CHIEF, "born ages ago");
        assert_eq!(gone(2, None), NEW_CHIEF);
    }

    /// An old-age death is not a succession until the conflict next wakes: the heir answers and the
    /// block already says the chief is new; with the chief alive it does not.
    #[test]
    fn a_dead_chief_with_the_heir_answering_is_a_new_chief_before_the_succession_runs() {
        use facts::faction::mood::NEW_CHIEF;
        let (mut st, f) = camp_world();
        let block = |st: &mut State, id| {
            answer_one(st, ask_of(id, ask::holder::FACTION, 55, 5, CAMP_ASK))
                .faction
                .expect("a camp's block")
        };
        assert_eq!(
            block(&mut st, 1).mood & NEW_CHIEF,
            0,
            "the founding chief lives"
        );
        let (chief, heir) = {
            let x = &st.conflict.engine.factions[f as usize];
            (x.chief, x.heir)
        };
        st.phase1.world.knowledge.die(chief, Tick(NOW));
        let x = &st.conflict.engine.factions[f as usize];
        assert_eq!(
            (x.chief, x.heir, x.chiefs),
            (chief, heir, 2),
            "no succession yet"
        );
        assert_eq!(block(&mut st, 2).mood & NEW_CHIEF, NEW_CHIEF);
        // With the heir gone too nobody answers, and a camp with no chief and no heir is not "new".
        st.phase1.world.knowledge.die(heir, Tick(NOW));
        assert_eq!(block(&mut st, 3).mood & NEW_CHIEF, 0);
    }

    #[test]
    fn the_block_codes_the_rung_the_signal_and_the_cowing_and_never_says_you_killed_the_chief() {
        use rtsim_conflict::{SignalKind, Step};
        let mut ff = facts_of(2, 30.0, 60.0);
        ff.step = Step::Siege;
        ff.signal = Some(SignalKind::Building);
        ff.grievance = 12.5;
        ff.plunders = 7;
        ff.cowed_until = Tick(u64::MAX);
        let b = faction_block(&ff, None, false);
        assert_eq!(b.rung, 6);
        assert_eq!(b.signal, 5);
        assert_eq!(b.grievance, 12.5);
        assert_eq!(b.plunders, 7);
        assert_eq!(b.cowed_ticks, u32::MAX, "saturates");
        assert_eq!(b.mood & facts::faction::mood::YOU_KILLED_CHIEF, 0);
        assert_eq!(
            b.mood & facts::faction::mood::COWED,
            facts::faction::mood::COWED
        );
        for (kind, code) in SignalKind::ALL.iter().zip(0u16..) {
            ff.signal = Some(*kind);
            assert_eq!(faction_block(&ff, None, false).signal, code);
        }
        ff.signal = None;
        ff.cowed_until = Tick(NOW);
        let b = faction_block(&ff, None, false);
        assert_eq!(b.signal, 0xFFFF);
        assert_eq!(b.cowed_ticks, 0, "cowed until now is not cowed");
        assert_eq!(b.mood & facts::faction::mood::COWED, 0);
    }
}
