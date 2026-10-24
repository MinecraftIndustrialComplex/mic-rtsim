// SPDX-License-Identifier: AGPL-3.0-or-later
//! The camp reconciler's engine half (MOWZIE.md §4.9, ledgers 6 and 7; slice M4).
//!
//! UR-1 lets Ben remove rtsim at any time, so a camp's members at home are the jar's own vanilla
//! bodies, saved by vanilla, and the engine only ever *keeps books* on them. A camp whose bodies a
//! census counts ([`crate::culture::CultureDef::camp_bodies`]) keeps a ledger: one [`Slot`] for each
//! member of its pool and of its war parties away, each with a generation and one of five states
//! ([`SlotState`]), under the invariants I1 to I6 (§4.9). Java reports what it found or did
//! (CAMP_CENSUS, CAMP_BODIES, CAMP_DEATH, CAMP_OUT) and the engine books only that; it orders
//! (CAMP_RECONCILE) only to a camp that is in, at most one line in flight per slot, and never again
//! after a restart: the camp's next census derives what is still needed.
//!
//! - **Epochs.** A camp is *in* from a CAMP_CENSUS (its Java op id is the epoch) to the CAMP_OUT
//!   that ends it, or the session's end. The engine orders only while it is in.
//! - **The pool follows the ledger** (I2): `floor(pool_at)` slots are at home (body or owed). The
//!   closed form adds no slot by itself, so every read of the pool first adds the slots it gained
//!   since the last (`camp_sync`), each with a generation from the tick it became whole, and a
//!   camp that is in books a [`Ev::Refill`] for the next whole member.
//! - **Draws and losses.** A war party draws slots at home (`camp_draw`: found bodies first, then
//!   missed, then owed, never one with a line in flight); its losses fall first on slots whose
//!   bodies are gone (`party_lost`); it ends only through its own `home`.
//! - **Answers are booked by the world fact they report**, by the slot's state (`camp_bodies`), so
//!   a replayed answer is a no-op by its engine op id, and one for a generation the slot no longer
//!   holds books nothing.

use rtsim_kernel::renewal::{LossCause, Pressure};

use super::*;
use crate::boundary::{
    ACTION_ADOPT, ACTION_HAND_BACK, ACTION_REMOVE, ACTION_SET_OUT, ACTION_SPAWN, ACTION_TAKE,
    BODY_ELSEWHERE, BODY_STRAY, CENSUS_CHIEF, CENSUS_CRYSTAL, CENSUS_THRONE, CampBodies,
    CampCensus, CampDeath, CampOut, CampReconcile, CensusBody, DEATH_STRAY, LINE_CHIEF,
    LINE_CRYSTAL, LINE_DONE, LINE_NOTHING, ReconcileLine,
};

/// Whole days after a body's first miss before the third miss writes its member off
/// (MOWZIE.md §4.9; PLACEHOLDER).
pub const WRITE_OFF_DAYS: u64 = 2;
/// Misses before a member is written off: a body found nowhere in three censuses.
pub const WRITE_OFF_MISSES: u8 = 3;

/// A slot's state (MOWZIE.md §4.9's table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    /// Home, body: the member is in the pool and its body is in the camp, tagged.
    Body = 0,
    /// Home, owed: in the pool, no body yet.
    Owed = 1,
    /// Away, left: in a war party, its body still in the camp.
    Left = 2,
    /// Away, gone: in a war party, no body (taken, handed to its puppet, or owed when drawn).
    Gone = 3,
    /// Dead, left: it died away, its body still in the camp to be removed.
    Dead = 4,
}

impl SlotState {
    pub fn from_u8(v: u8) -> Option<SlotState> {
        Some(match v {
            0 => SlotState::Body,
            1 => SlotState::Owed,
            2 => SlotState::Left,
            3 => SlotState::Gone,
            4 => SlotState::Dead,
            _ => return None,
        })
    }

    /// In the pool.
    pub fn home(self) -> bool {
        matches!(self, SlotState::Body | SlotState::Owed)
    }

    /// In a war party.
    pub fn away(self) -> bool {
        matches!(self, SlotState::Left | SlotState::Gone)
    }
}

/// The order in flight for a slot: its action (`boundary::ACTION_*`) and the op id of the
/// CAMP_RECONCILE that carried it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub action: u8,
    pub op: u64,
}

/// One member of the camp.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// The slot's number, from 0, reused when a slot is freed.
    pub id: u32,
    /// Derived from the slot and the tick its member was recruited or adopted (never 0: slot 0 with
    /// generation 0 is how a stray is named).
    pub generation: u32,
    /// The engine's body id of the member's body.
    pub body: u32,
    pub state: SlotState,
    /// The war party it is in (`Left`, `Gone`); null otherwise.
    pub party: Handle<Party>,
    /// Censuses that missed a home body since one found it.
    pub misses: u8,
    /// The first of those.
    pub missed_at: Tick,
    pub line: Option<Line>,
}

/// A generation a slot last retired, and whether at a death (its body, if found later, is
/// removed) or at a write-off (it is a stray like any other).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retired {
    pub slot: u32,
    pub generation: u32,
    pub died: bool,
}

/// An answer line booked in a camp's latest epoch: its (op, line), and the journal sequence of the
/// latest record that carried it (a holder's copy included). The host's world save makes the
/// sequence durable, and the answer is forgotten then (MZB-177). Sequences start at 1: 0 is a host
/// with none (the simulations), and its answers are kept to the end of their epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answered {
    pub op: u64,
    pub line: u32,
    pub seq: u64,
}

/// The Umvuthi's throne spot, kept from the grove's first census (MOWZIE.md ledger 7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Throne {
    pub spot: (f64, f64, f64),
    pub direction: f32,
}

/// A camp's ledger.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Camp {
    /// The epoch the camp is in at (a Java op id); 0 while it is out.
    pub epoch: u64,
    /// The latest epoch a census named, in or out (and replayed ones too): the only epoch whose
    /// answers are still booked, an older one's being superseded by that census.
    pub seen: u64,
    /// The (op, line) answers booked in `seen`'s epoch, in order: the records that may come twice
    /// (a holder's, journaled again) are skipped by it. Cleared at each census of a new epoch, and
    /// an answer is forgotten once the world save that makes its record durable has been reported
    /// (`Conflict::durable_obs_seq`), as `Replay` forgets op ids; a copy that comes later is booked
    /// by its slot's state and the next census (MZB-177).
    pub answered: Vec<Answered>,
    /// CAMP_RECONCILEs sent in this epoch (the decision in their op ids).
    pub seq: u32,
    /// The stamp of the booked refill event, and its tick.
    pub refill: u32,
    pub refill_at: Option<Tick>,
    /// By slot number.
    pub slots: Vec<Slot>,
    pub retired: Vec<Retired>,
    pub throne: Option<Throne>,
    /// Recent player-caused losses of a renewal culture's camp (EQUILIBRIUM.md §2.2, MZB-254).
    pub pressure: Pressure,
    /// The stamp of the booked arrival event, and its tick: a renewal culture's wait for its next
    /// member, booked whether the camp is in or out.
    pub arrival: u32,
    pub arrival_at: Option<Tick>,
}

impl Camp {
    pub fn is_in(&self) -> bool {
        self.epoch != 0
    }

    pub fn index(&self, id: u32, generation: u32) -> Option<usize> {
        let i = self.slots.binary_search_by_key(&id, |s| s.id).ok()?;
        (self.slots[i].generation == generation).then_some(i)
    }

    /// Slots in the pool: at home, body or owed.
    pub fn home_count(&self) -> u32 {
        self.slots.iter().filter(|s| s.state.home()).count() as u32
    }

    /// Slots in war parties.
    pub fn away_count(&self) -> u32 {
        self.slots.iter().filter(|s| s.state.away()).count() as u32
    }

    /// Slots a draw may take: at home with no line in flight.
    pub fn drawable(&self) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.state.home() && s.line.is_none())
            .count() as u32
    }

    /// Lines in flight.
    pub fn in_flight(&self) -> u32 {
        self.slots.iter().filter(|s| s.line.is_some()).count() as u32
    }

    /// Notes a census of `epoch` (replayed or not): a new epoch ends the old one's answers.
    fn see(&mut self, epoch: u64) {
        if self.seen != epoch {
            self.seen = epoch;
            self.answered.clear();
        }
    }

    /// Records the answer to line `line` of reconcile `op`, carried by the record with journal
    /// sequence `seq`; false if it was booked already (the copy's sequence then keeps the answer
    /// until that is durable too).
    fn answer(&mut self, op: u64, line: u32, seq: u64) -> bool {
        match self
            .answered
            .binary_search_by_key(&(op, line), |a| (a.op, a.line))
        {
            Ok(i) => {
                let a = &mut self.answered[i];
                a.seq = a.seq.max(seq);
                false
            }
            Err(at) => {
                self.answered.insert(at, Answered { op, line, seq });
                true
            }
        }
    }

    /// Forgets the answers whose records a world save has made durable (`seq` and before).
    fn forget_answers_through(&mut self, seq: u64) {
        self.answered.retain(|a| a.seq == 0 || a.seq > seq);
        // The paging budget counts capacity: a long epoch's burst is not kept after it.
        if self.answered.capacity() > 2 * self.answered.len() + 16 {
            self.answered.shrink_to_fit();
        }
    }

    fn retired_gen(&self, id: u32) -> Option<u32> {
        self.retired
            .iter()
            .find(|r| r.slot == id)
            .map(|r| r.generation)
    }

    /// Frees slot `i`, remembering the generation it retired and whether at a death.
    fn free(&mut self, i: usize, died: bool) {
        let s = self.slots.remove(i);
        match self.retired.iter_mut().find(|r| r.slot == s.id) {
            Some(r) => {
                r.generation = s.generation;
                r.died = died;
            }
            None => self.retired.push(Retired {
                slot: s.id,
                generation: s.generation,
                died,
            }),
        }
    }

    /// Voids every line in flight; how many there were.
    fn void(&mut self) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            n += usize::from(s.line.take().is_some());
        }
        n
    }

    /// Bytes on the heap (the paging budget).
    pub fn heap_bytes(&self) -> usize {
        std::mem::size_of::<Camp>()
            + self.slots.capacity() * std::mem::size_of::<Slot>()
            + self.retired.capacity() * std::mem::size_of::<Retired>()
            + self.answered.capacity() * std::mem::size_of::<Answered>()
    }
}

/// A slot's generation: from the camp, the slot and the tick its member was recruited or adopted
/// (docs/boundary.md's op id rule: derived from the transfer's own identity, so a replay after a
/// crash gives the same one), never 0 and never the one the slot last retired.
fn derive_gen(key: u64, id: u32, t: Tick, avoid: Option<u32>) -> u32 {
    let mut salt = 0u64;
    loop {
        let h = fnv1a64(
            &[
                key.to_le_bytes(),
                u64::from(id).to_le_bytes(),
                t.0.to_le_bytes(),
                salt.to_le_bytes(),
            ]
            .concat(),
        );
        let g = (h ^ (h >> 32)) as u32;
        if g != 0 && Some(g) != avoid {
            return g;
        }
        salt += 1;
    }
}

/// A CAMP_RECONCILE's op id: the engine's allocator byte over 56 bits derived from the camp, the
/// epoch and the decision (docs/boundary.md §9.2).
fn derive_op(key: u64, epoch: u64, seq: u32) -> u64 {
    let h = fnv1a64(
        &[
            key.to_le_bytes(),
            epoch.to_le_bytes(),
            u64::from(seq).to_le_bytes(),
        ]
        .concat(),
    );
    (boundary::AGENT_NAMESPACE << 56) | (h & ((1 << 56) - 1)).max(1)
}

impl Conflict {
    /// A faction's ledger, if its camp keeps one.
    pub fn camp(&self, f: u32) -> Option<&Camp> {
        self.factions[f as usize].camp.as_deref()
    }

    fn camp_mut(&mut self, f: u32) -> Option<&mut Camp> {
        self.factions[f as usize].camp.as_deref_mut()
    }

    /// Whether a faction's camp is in (the plugin has its census and no CAMP_OUT since).
    pub fn camp_is_in(&self, f: u32) -> bool {
        self.camp(f).is_some_and(Camp::is_in)
    }

    /// The unit a camp's pool member is: the first of its first war party's make-up.
    fn home_unit(&self, f: u32) -> u8 {
        let c = self.culture(f);
        [
            PartyKind::Raid,
            PartyKind::Invasion,
            PartyKind::Siege,
            PartyKind::Plunder,
        ]
        .into_iter()
        .find_map(|k| c.parties[k.index()].first().map(|u| u.0))
        .unwrap_or(0)
    }

    /// The engine's body id of the camp's pool members.
    fn home_body(&self, f: u32) -> u32 {
        let u = self.home_unit(f);
        body_id(&self.cultures, &self.culture(f).unit(u).body)
    }

    /// A line's flags: the chief's slot (a unit with a row of its own without its crystal is the
    /// camp's chief, ledger 9) and whether its crystal is held.
    fn line_flags(&self, f: u32) -> u8 {
        let u = self.home_unit(f);
        let chief = self.culture(f).bare[usize::from(u)] != u;
        let crystal = self.factions[f as usize].crystal;
        (if chief { LINE_CHIEF } else { 0 }) | (if crystal { LINE_CRYSTAL } else { 0 })
    }

    fn line(&self, f: u32, s: &Slot, action: u8, a: u64, b: u64) -> ReconcileLine {
        ReconcileLine {
            slot: s.id,
            generation: s.generation,
            body: s.body,
            action,
            flags: self.line_flags(f),
            a,
            b,
        }
    }

    // ---- creation --------------------------------------------------------------------------

    /// A faction found with its bodies counted (FACTION_FOUND, MOWZIE.md §4.9): the pool is what
    /// the census at discovery found, the camp's cap that count (one at least) bounded by the
    /// culture's, and every slot starts owed, for the first census's strays to fill. Only for a
    /// culture whose bodies a census counts.
    ///
    /// `census` is the record's byte (`CENSUS_CHIEF`, `CENSUS_CRYSTAL`): with the chief present it
    /// sets the faction's crystal (MZB-257). A renewal culture's camp found with no bodies starts
    /// at the cap wait, as if farmed to it, since no site ledger says otherwise (EQUILIBRIUM item 6),
    /// and books its arrival; one found with its member books nothing.
    pub(super) fn camp_found(&mut self, f: u32, bodies: u16, census: u8, at: Tick) {
        if !self.culture(f).def.camp_bodies {
            return;
        }
        let ceiling = self.culture(f).def.pool_cap;
        let n = u32::from(bodies).min(ceiling);
        let x = &mut self.factions[f as usize];
        x.cap = n.max(1).min(ceiling);
        x.pool = f64::from(n);
        x.pool_at = at;
        if u32::from(census) & CENSUS_CHIEF != 0 {
            x.crystal = u32::from(census) & CENSUS_CRYSTAL != 0;
        }
        self.camp_create(f, at);
        if let Some(r) = self.culture(f).renewal
            && bodies == 0
        {
            let day = (at.0 / TICKS_PER_DAY) as u32;
            self.camp_mut(f).expect("created").pressure = Pressure::at_cap(&r.rule, day);
        }
        self.plan_arrival(f, at);
    }

    /// The ledger of a camp that has none, from the books as they are: a slot owed for each whole
    /// member of the pool, and a slot left for each unit of a war party away.
    fn camp_create(&mut self, f: u32, at: Tick) {
        if self.factions[f as usize].camp.is_some() {
            return;
        }
        self.factions[f as usize].camp = Some(Box::default());
        let body = self.home_body(f);
        let whole = self.pool_at(f, at).floor() as u32;
        for _ in 0..whole {
            self.camp_alloc(f, at, body, SlotState::Owed, Handle::NULL);
        }
        let away: Vec<(Handle<Party>, u32)> = self
            .parties
            .iter()
            .filter(|(_, x)| x.faction == f && x.kind.is_war())
            .map(|(h, x)| (h, x.alive()))
            .collect();
        for (p, n) in away {
            for _ in 0..n {
                self.camp_alloc(f, at, body, SlotState::Left, p);
            }
        }
    }

    /// A new slot in the lowest free number, its generation from `t`.
    fn camp_alloc(
        &mut self,
        f: u32,
        t: Tick,
        body: u32,
        state: SlotState,
        party: Handle<Party>,
    ) -> u32 {
        let key = self.factions[f as usize].key;
        let camp = self.camp_mut(f).expect("a ledger");
        let id = (0..)
            .find(|id| camp.slots.binary_search_by_key(id, |s| s.id).is_err())
            .expect("a free number");
        let generation = derive_gen(key, id, t, camp.retired_gen(id));
        let at = camp.slots.partition_point(|s| s.id < id);
        camp.slots.insert(
            at,
            Slot {
                id,
                generation,
                body,
                state,
                party,
                misses: 0,
                missed_at: Tick::ZERO,
                line: None,
            },
        );
        id
    }

    // ---- the pool follows the ledger (I2) ---------------------------------------------------

    /// When the pool reached `k` whole warriors, by the closed form (never after `at`).
    fn whole_at(&self, f: u32, k: u32, at: Tick) -> Tick {
        let x = &self.factions[f as usize];
        let rate = self.culture(f).def.recruits_per_day / TICKS_PER_DAY as f64;
        if x.pool >= f64::from(k) || rate <= 0.0 {
            return x.pool_at.min(at);
        }
        later(x.pool_at, ((f64::from(k) - x.pool) / rate).ceil() as u64).min(at)
    }

    /// Adds the slots the pool gained since it was last read: each whole member it gained is a
    /// slot owed a body, with its generation from the tick it became whole (I2).
    pub(super) fn camp_sync(&mut self, f: u32, at: Tick) {
        let Some(have) = self.camp(f).map(Camp::home_count) else {
            return;
        };
        let whole = self.pool_at(f, at).floor() as u32;
        if whole <= have {
            return;
        }
        let body = self.home_body(f);
        for k in have + 1..=whole {
            let t = self.whole_at(f, k, at);
            self.camp_alloc(f, t, body, SlotState::Owed, Handle::NULL);
        }
    }

    /// The pool returns `n` survivors at `at`: they leave `away` as they join the pool, so the
    /// room they came back to is theirs (MOWZIE.md ledger 6).
    pub(super) fn pool_return(&mut self, f: u32, n: u32, at: Tick) {
        self.camp_sync(f, at);
        let v = self.pool_at(f, at);
        let x = &mut self.factions[f as usize];
        x.away = x.away.saturating_sub(n);
        let room = f64::from(x.cap.saturating_sub(x.away));
        x.pool = (v + f64::from(n)).clamp(0.0, room);
        x.pool_at = at;
    }

    /// Moves the pool by `n` with no regard to the ledger (the caller moves its slots).
    fn pool_shift(&mut self, f: u32, n: f64, at: Tick) {
        let v = self.pool_at(f, at);
        let x = &mut self.factions[f as usize];
        let room = f64::from(x.cap.saturating_sub(x.away));
        x.pool = (v + n).clamp(0.0, room);
        x.pool_at = at;
    }

    /// The slots a draw may take, if the camp keeps a ledger.
    pub(super) fn camp_drawable(&self, f: u32) -> Option<u32> {
        self.camp(f).map(Camp::drawable)
    }

    /// When a feud that needs `need` whole bodies may try again: when the pool holds them
    /// (`pool_ready`), and while lines in flight hold slots back, not before one is answered or
    /// void ([`BLOCKED`], `unblock`).
    pub(super) fn bodies_ready(&self, f: u32, need: f64, at: Tick) -> Tick {
        let t = self.pool_ready(f, need, at);
        if t > at {
            return t;
        }
        match self.camp(f) {
            Some(c) if f64::from(c.drawable()) < need => BLOCKED,
            _ => t,
        }
    }

    /// A change of a camp's room or lines: every feud waiting on it is planned again, and a camp
    /// that is in books its next refill.
    pub(super) fn unblock(&mut self, f: u32, at: Tick) {
        for a in self.feud_actors(f) {
            if self.feud(f, a).is_some_and(|x| x.wait_until >= BLOCKED) {
                self.feud_mut(f, a).expect("listed").wait_until = Tick::ZERO;
                self.plan_feud(f, a, at, false);
            }
        }
        self.plan_refill(f, at);
        self.plan_arrival(f, at);
    }

    // ---- refills ---------------------------------------------------------------------------

    /// Books the event at which the camp's pool next gains a whole member, while the camp is in
    /// and the pool and its members away are under the cap. A refill that was booked for the same
    /// tick stays.
    fn plan_refill(&mut self, f: u32, at: Tick) {
        if !self.camp_is_in(f) {
            return;
        }
        let x = &self.factions[f as usize];
        let next = (self.pool_at(f, at).floor() + 1.0).max(1.0);
        let t = (next <= f64::from(x.cap.saturating_sub(x.away)))
            .then(|| self.pool_ready(f, next, at))
            .filter(|t| *t < BLOCKED);
        let camp = self.camp_mut(f).expect("in");
        if camp.refill_at == t {
            return;
        }
        camp.refill = camp.refill.wrapping_add(1);
        camp.refill_at = t;
        let stamp = camp.refill;
        if let Some(t) = t {
            self.schedule(t, Ev::Refill { faction: f, stamp });
        }
    }

    /// Why a member of a renewal culture's camp was lost, by who killed it (MZB-258): a player's
    /// kill and a body carried off are a player's loss, an unclaimed death (`BY_NONE`, `BY_OTHER`, a
    /// kind this build does not know) is a player's too when the culture's renewal is a boss's
    /// (EQUILIBRIUM.md §2.2: an unclaimed death of a boss kind) and no one's otherwise, and a
    /// mob's kill is a predator's.
    pub(super) fn renewal_cause(&self, f: u32, by: u32) -> LossCause {
        let boss = self.culture(f).renewal.is_some_and(|r| r.boss);
        match by {
            boundary::BY_PLAYER | boundary::BY_TAKEN => LossCause::Player,
            boundary::BY_MOB => LossCause::Predator,
            _ if boss => LossCause::Player,
            _ => LossCause::Other,
        }
    }

    /// `n` members of a renewal culture's camp were lost at `at` from `cause`: only a player's loss
    /// (`LossCause::Player`) adds `n / cap` to the camp's pressure, as of the loss's day (MZB-258).
    /// It goes before whatever plans the next wait (`unblock`), which reads it. A camp with no
    /// ledger has nowhere to keep it.
    pub(super) fn renewal_bump(&mut self, f: u32, n: u32, cause: LossCause, at: Tick) {
        let Some(r) = self.culture(f).renewal else {
            return;
        };
        if cause != LossCause::Player || n == 0 {
            return;
        }
        let cap = self.factions[f as usize].cap.max(1);
        let day = (at.0 / TICKS_PER_DAY) as u32;
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        camp.pressure
            .bump(day, n as f32 / cap as f32, r.rule.half_life_days);
        self.stats.renewal_losses += u64::from(n);
    }

    /// Books the wait for a renewal culture's next member (MZB-255), whether the camp is in or
    /// out: short (the pool and the members away under the cap) with no wait booked, an arrival
    /// after `delay_ticks` of the pressure as it stands now; no longer short with one booked, the
    /// wait is cancelled. A wait running is never read again, so later losses do not push it out
    /// (the kernel's rule). `plan_refill` books nothing for such a camp, since its rate is 0. A
    /// faction with no ledger never refills.
    pub(super) fn plan_arrival(&mut self, f: u32, at: Tick) {
        let Some(r) = self.culture(f).renewal else {
            return;
        };
        let x = &self.factions[f as usize];
        let short = self.pool_at(f, at).floor() as u32 + x.away < x.cap;
        let day = (at.0 / TICKS_PER_DAY) as u32;
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        match (short, camp.arrival_at) {
            (true, None) => {
                let t = at + r.rule.delay_ticks(camp.pressure, day).max(1);
                camp.arrival = camp.arrival.wrapping_add(1);
                camp.arrival_at = Some(t);
                let stamp = camp.arrival;
                self.schedule(t, Ev::Arrival { faction: f, stamp });
            }
            (false, Some(_)) => {
                camp.arrival = camp.arrival.wrapping_add(1);
                camp.arrival_at = None;
            }
            _ => {}
        }
    }

    /// A renewal culture's wait ran out (MZB-256): its camp gains a member, owed a body, and with
    /// it the crystal a new Frostmaw holds (MZ-4 (a)). A camp that is in is ordered to spawn it at
    /// once, so one that never unloads gets its arrival when the wait ends (EQUILIBRIUM item 8); one
    /// that is out has it spawned by its next census, as any owed slot is.
    pub(super) fn camp_arrival(&mut self, f: u32, stamp: u32, at: Tick) {
        match self.camp_mut(f) {
            Some(c) if c.arrival == stamp && c.arrival_at.is_some() => c.arrival_at = None,
            _ => {
                self.stats.stale += 1;
                return;
            }
        }
        // `pool_add` syncs first, so the slot the arrival adds is the one the sync after it makes.
        self.pool_add(f, 1.0, at);
        self.camp_sync(f, at);
        self.factions[f as usize].crystal = true;
        self.stats.arrivals += 1;
        self.note(
            at,
            Some(f),
            "arrival",
            "a member arrives at the camp".to_string(),
        );
        if self.camp_is_in(f) {
            let lines = self.spawn_lines(f, &[]);
            self.send_lines(f, lines, at);
        }
        self.unblock(f, at);
    }

    pub(super) fn camp_refill(&mut self, f: u32, stamp: u32, at: Tick) {
        match self.camp_mut(f) {
            Some(c) if c.refill == stamp && c.is_in() => c.refill_at = None,
            _ => {
                self.stats.stale += 1;
                return;
            }
        }
        self.camp_sync(f, at);
        let lines = self.spawn_lines(f, &[]);
        self.send_lines(f, lines, at);
        self.plan_refill(f, at);
    }

    /// A spawn line for each slot owed a body that has no line in flight and is not in `but`.
    fn spawn_lines(&self, f: u32, but: &[ReconcileLine]) -> Vec<ReconcileLine> {
        let Some(c) = self.camp(f) else {
            return Vec::new();
        };
        c.slots
            .iter()
            .filter(|s| s.state == SlotState::Owed && s.line.is_none())
            .filter(|s| !but.iter().any(|l| l.slot == s.id && l.generation != 0))
            .map(|s| self.line(f, s, ACTION_SPAWN, 0, 0))
            .collect()
    }

    // ---- orders ----------------------------------------------------------------------------

    /// Sends the lines to the camp as CAMP_RECONCILEs (at most [`boundary::MAX_ENTRIES`] each),
    /// each slot named in one marked in flight. Only a camp that is in is ordered (§4.9).
    fn send_lines(&mut self, f: u32, lines: Vec<ReconcileLine>, at: Tick) {
        if lines.is_empty() {
            return;
        }
        let key = self.factions[f as usize].key;
        let Some(camp) = self.camp_mut(f).filter(|c| c.is_in()) else {
            return;
        };
        let epoch = camp.epoch;
        // At most one line in flight per slot (I4): a second for a slot is a bug, and not sent.
        let lines: Vec<ReconcileLine> = lines
            .into_iter()
            .filter(|l| {
                let busy = l.generation != 0
                    && camp
                        .index(l.slot, l.generation)
                        .is_some_and(|i| camp.slots[i].line.is_some());
                debug_assert!(!busy, "a second line for slot {} in flight", l.slot);
                !busy
            })
            .collect();
        if lines.is_empty() {
            return;
        }
        let mut out = Vec::new();
        for chunk in lines.chunks(boundary::MAX_ENTRIES) {
            let op = derive_op(key, epoch, camp.seq);
            camp.seq += 1;
            for l in chunk.iter().filter(|l| l.generation != 0) {
                if let Some(i) = camp.index(l.slot, l.generation) {
                    camp.slots[i].line = Some(Line {
                        action: l.action,
                        op,
                    });
                }
            }
            out.push(Cmd::Reconcile(CampReconcile {
                faction: key,
                epoch,
                op,
                flags: 0,
                throne: (0.0, 0.0, 0.0),
                direction: 0.0,
                lines: chunk.to_vec(),
            }));
        }
        self.stats.camp_orders += out.len() as u64;
        self.stats.camp_lines += lines.len() as u64;
        let n = lines.len();
        self.outbox.extend(out);
        self.note(
            at,
            Some(f),
            "camp",
            format!("{n} line(s) ordered at epoch {epoch:#x}"),
        );
    }

    // ---- draws, muster ends, homecomings and losses ---------------------------------------

    /// A war party's units are drawn from the camp's slots at home (MOWZIE.md §4.9): first those
    /// whose bodies the last census found, then those whose bodies it missed, then those owed,
    /// never one with a line in flight. A slot drawn with a body is away, left; one drawn owed is
    /// away, gone, carrying its debt: nothing is taken and nothing spawned for it while away.
    pub(super) fn camp_draw(&mut self, f: u32, p: Handle<Party>, n: u32, _at: Tick) {
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        let class = |s: &Slot| match (s.state, s.misses) {
            (SlotState::Body, 0) => 0,
            (SlotState::Body, _) => 1,
            _ => 2,
        };
        let mut order: Vec<usize> = (0..camp.slots.len())
            .filter(|&i| camp.slots[i].state.home() && camp.slots[i].line.is_none())
            .collect();
        order.sort_by_key(|&i| (class(&camp.slots[i]), camp.slots[i].id));
        // `send_party` draws no more than the slots at home with no line in flight; a draw past
        // them takes the slots there are, never one a line holds.
        debug_assert!(order.len() >= n as usize, "a draw past its slots");
        for i in order.into_iter().take(n as usize) {
            let s = &mut camp.slots[i];
            s.state = if s.state == SlotState::Body {
                SlotState::Left
            } else {
                SlotState::Gone
            };
            s.party = p;
            // A body the last census missed stays unverified while its member is away: `misses`
            // is kept, so a member that comes home is written off on the clock it was on.
        }
    }

    /// A war party's muster ends at a camp that is in: a set-out line, with the party's agent and
    /// epoch, for each member's body left in the camp. Java hands them off to the party's puppets
    /// when the party is materialized in the same hand-over, else takes them. A camp that is out
    /// orders nothing: its next census takes them.
    pub(super) fn camp_set_out(&mut self, p: Handle<Party>, at: Tick) {
        let x = &self.parties[p];
        let (f, epoch, war) = (x.faction, x.epoch, x.kind.is_war());
        if !war || !self.camp_is_in(f) {
            return;
        }
        let (agent, epoch) = (Party::agent(p), u64::from(epoch));
        let lines: Vec<ReconcileLine> = self
            .camp(f)
            .expect("in")
            .slots
            .iter()
            .filter(|s| s.state == SlotState::Left && s.party == p && s.line.is_none())
            .map(|s| self.line(f, s, ACTION_SET_OUT, agent, epoch))
            .collect();
        self.send_lines(f, lines, at);
    }

    /// A war party's units fell (a casualty, a death by slot): its members' slots follow, those
    /// whose bodies are gone first, so fewer bodies are removed later; a slot left behind that
    /// dies is dead, left. The room they leave refills (`unblock`).
    pub(super) fn party_lost(&mut self, p: Handle<Party>, lost: u32, at: Tick) {
        if lost == 0 || !self.parties[p].kind.is_war() {
            return;
        }
        let f = self.parties[p].faction;
        // The pool grew to the room it had, up to now.
        self.pool_add(f, 0.0, at);
        let x = &mut self.factions[f as usize];
        x.away = x.away.saturating_sub(lost);
        if let Some(camp) = self.camp_mut(f) {
            let mut left = lost;
            for want in [SlotState::Gone, SlotState::Left] {
                let mut i = 0;
                while left > 0 && i < camp.slots.len() {
                    let s = &mut camp.slots[i];
                    if s.state == want && s.party == p {
                        left -= 1;
                        if want == SlotState::Gone {
                            camp.free(i, true);
                            continue;
                        }
                        s.state = SlotState::Dead;
                        s.party = Handle::NULL;
                    }
                    i += 1;
                }
            }
        }
        self.unblock(f, at);
    }

    /// A war party reaches home (`home`): a survivor whose body is still in the camp is home, body
    /// again; one whose body is gone is owed again, and at a camp that is in gets a hand-back line
    /// if its puppet is in the world, else a spawn line; at a camp that is out its puppets are
    /// recalled as they always were and the next census spawns the bodies.
    pub(super) fn camp_home(&mut self, p: Handle<Party>, at: Tick) {
        let f = self.parties[p].faction;
        if self.camp(f).is_none() {
            return;
        }
        self.camp_sync(f, at);
        let x = &self.parties[p];
        let (embodied, epoch) = (x.embodied, x.epoch);
        let mut puppets: u32 = if embodied {
            x.spawned.iter().map(|s| s.1).sum()
        } else {
            0
        };
        let is_in = self.camp_is_in(f);
        let (agent, epoch64) = (Party::agent(p), u64::from(epoch));
        let mut hand: Vec<(u32, u32)> = Vec::new();
        let camp = self.camp_mut(f).expect("a ledger");
        for s in camp.slots.iter_mut().filter(|s| s.party == p) {
            s.party = Handle::NULL;
            match s.state {
                SlotState::Left => s.state = SlotState::Body,
                SlotState::Gone => {
                    s.state = SlotState::Owed;
                    if is_in && s.line.is_none() {
                        hand.push((s.id, s.generation));
                    }
                }
                _ => {}
            }
        }
        let mut lines = Vec::new();
        for (id, generation) in hand {
            let s = self
                .camp(f)
                .and_then(|c| c.index(id, generation).map(|i| &c.slots[i]));
            let Some(s) = s else {
                continue;
            };
            let l = if puppets > 0 {
                puppets -= 1;
                self.line(f, s, ACTION_HAND_BACK, agent, epoch64)
            } else {
                self.line(f, s, ACTION_SPAWN, 0, 0)
            };
            lines.push(l);
        }
        let more = self.spawn_lines(f, &lines);
        lines.extend(more);
        if is_in {
            self.send_lines(f, lines, at);
        } else if embodied {
            let camp = self.factions[f as usize].pos;
            self.recall(p, epoch, boundary::RECALL_DONE, (camp.x, camp.z));
        }
    }

    // ---- CAMP_CENSUS -----------------------------------------------------------------------

    /// A camp's entities have all come in: it is in at the census's epoch, every line of an
    /// earlier epoch is void, and each slot is compared with the bodies found (MOWZIE.md §4.9's
    /// table). A census replayed from an earlier session only marks its camp out: its bodies are
    /// not the world as it is now.
    pub(super) fn camp_census(&mut self, c: &CampCensus, at: Tick) {
        let Some(&f) = self.by_key.get(&c.faction) else {
            return;
        };
        if !self.culture(f).def.camp_bodies {
            return;
        }
        if self.replaying {
            if let Some(camp) = self.camp_mut(f) {
                camp.epoch = 0;
                camp.refill_at = None;
                camp.see(c.epoch);
            }
            return;
        }
        self.camp_create(f, at);
        if self.camp(f).is_some_and(|k| k.epoch == c.epoch) {
            return;
        }
        self.stats.camp_censuses += 1;
        self.camp_sync(f, at);
        let mut camp = self.factions[f as usize].camp.take().expect("a ledger");
        camp.epoch = c.epoch;
        camp.see(c.epoch);
        camp.seq = 0;
        camp.refill = camp.refill.wrapping_add(1);
        camp.refill_at = None;
        camp.void();
        if c.flags & CENSUS_THRONE != 0 && camp.throne.is_none() {
            camp.throne = Some(Throne {
                spot: c.throne,
                direction: c.direction,
            });
        }
        let cap = self.factions[f as usize].cap;
        let body = self.home_body(f);
        let mut written_off = 0;
        let mut lines: Vec<ReconcileLine> = Vec::new();
        let flags = self.line_flags(f);
        let line = |s: &Slot, action: u8, a: u64, b: u64| ReconcileLine {
            slot: s.id,
            generation: s.generation,
            body: s.body,
            action,
            flags,
            a,
            b,
        };
        let by_uuid = |b: &CensusBody, action: u8| ReconcileLine {
            slot: 0,
            generation: 0,
            body: b.body,
            action,
            flags: 0,
            a: b.uuid.0,
            b: b.uuid.1,
        };

        // The bodies by slot and generation: of two with one, the one in the footprint stays (the
        // census names no distances), else the first; the others are removed by UUID.
        let mut held: BTreeMap<(u32, u32), &CensusBody> = BTreeMap::new();
        let mut unmatched: Vec<&CensusBody> = Vec::new();
        for b in &c.bodies {
            if b.flags & BODY_STRAY != 0 || b.generation == 0 {
                unmatched.push(b);
                continue;
            }
            let key = (b.slot, b.generation);
            match held.get(&key) {
                None => {
                    held.insert(key, b);
                }
                Some(k) if k.flags & BODY_ELSEWHERE != 0 && b.flags & BODY_ELSEWHERE == 0 => {
                    lines.push(by_uuid(k, ACTION_REMOVE));
                    held.insert(key, b);
                }
                Some(_) => lines.push(by_uuid(b, ACTION_REMOVE)),
            }
        }
        // Each slot against its body.
        let ids: Vec<(u32, u32)> = camp.slots.iter().map(|s| (s.id, s.generation)).collect();
        let mut matched: BTreeSet<(u32, u32)> = BTreeSet::new();
        for (id, generation) in ids {
            let Some(i) = camp.index(id, generation) else {
                continue;
            };
            let found = held.contains_key(&(id, generation));
            if found {
                matched.insert((id, generation));
            }
            let s = &mut camp.slots[i];
            match s.state {
                SlotState::Body => {
                    if found {
                        s.misses = 0;
                    } else {
                        if s.misses == 0 {
                            s.missed_at = at;
                        }
                        s.misses = (s.misses + 1).min(WRITE_OFF_MISSES);
                        if s.misses >= WRITE_OFF_MISSES
                            && at.0 >= s.missed_at.0 + WRITE_OFF_DAYS * TICKS_PER_DAY
                        {
                            // The member died unseen: the pool loses it and its generation is
                            // retired (not at a death: a body found later is a stray).
                            camp.free(i, false);
                            self.pool_shift(f, -1.0, at);
                            self.stats.camp_written_off += 1;
                            written_off += 1;
                        }
                    }
                }
                SlotState::Owed => {
                    if found {
                        s.state = SlotState::Body;
                        s.misses = 0;
                    }
                }
                SlotState::Left | SlotState::Gone => {
                    let phase = self.parties.get(s.party).map(|x| x.phase);
                    // A party leaves the books through `home`, which settles its slots first.
                    debug_assert!(
                        phase.is_some(),
                        "slot {} {:?} is in a party that is gone",
                        s.id,
                        s.state
                    );
                    if s.state == SlotState::Gone && found {
                        s.state = SlotState::Left;
                    }
                    // Found or gone, the slot's body is no longer in doubt.
                    s.misses = 0;
                    match (s.state, found, phase) {
                        (SlotState::Left, false, _) => s.state = SlotState::Gone,
                        // Its party is gone from the books: a survivor home, body.
                        (SlotState::Left, true, None) => {
                            s.state = SlotState::Body;
                            s.party = Handle::NULL;
                        }
                        (SlotState::Left, true, Some(Phase::Mustering)) => {}
                        (SlotState::Left, true, Some(_)) => lines.push(line(s, ACTION_TAKE, 0, 0)),
                        _ => {}
                    }
                }
                SlotState::Dead => {
                    if found {
                        lines.push(line(s, ACTION_REMOVE, 0, 0));
                    } else {
                        camp.free(i, true);
                    }
                }
            }
        }
        // Tagged bodies no slot holds, and strays: a body whose generation the books retired at
        // a death is removed; the rest fill slots owed, then new slots while the pool and the
        // members away are under the cap, and are removed above it (ledger 6).
        let mut rest: Vec<&CensusBody> = Vec::new();
        for b in &c.bodies {
            let key = (b.slot, b.generation);
            let is_stray = b.flags & BODY_STRAY != 0 || b.generation == 0;
            if !is_stray
                && (matched.contains(&key) || held.get(&key).is_none_or(|k| k.uuid != b.uuid))
            {
                continue;
            }
            if !is_stray
                && camp
                    .retired
                    .iter()
                    .any(|r| r.slot == b.slot && r.generation == b.generation && r.died)
            {
                lines.push(by_uuid(b, ACTION_REMOVE));
                continue;
            }
            rest.push(b);
        }
        let mut owed: std::collections::VecDeque<(u32, u32)> = camp
            .slots
            .iter()
            .filter(|s| s.state == SlotState::Owed)
            .map(|s| (s.id, s.generation))
            .collect();
        for b in rest {
            if let Some((id, generation)) = owed.pop_front() {
                let i = camp.index(id, generation).expect("owed");
                if b.body != 0 {
                    camp.slots[i].body = b.body;
                }
                lines.push(line(&camp.slots[i], ACTION_ADOPT, b.uuid.0, b.uuid.1));
            } else if camp.home_count() + camp.away_count() < cap {
                self.factions[f as usize].camp = Some(camp);
                self.pool_shift(f, 1.0, at);
                let id = self.camp_alloc(
                    f,
                    at,
                    if b.body != 0 { b.body } else { body },
                    SlotState::Owed,
                    Handle::NULL,
                );
                camp = self.factions[f as usize].camp.take().expect("put back");
                let i = camp.slots.binary_search_by_key(&id, |s| s.id).expect("new");
                lines.push(line(&camp.slots[i], ACTION_ADOPT, b.uuid.0, b.uuid.1));
            } else {
                lines.push(by_uuid(b, ACTION_REMOVE));
            }
        }
        // Slots still owed: a spawn line each.
        for (id, generation) in owed {
            let i = camp.index(id, generation).expect("owed");
            lines.push(line(&camp.slots[i], ACTION_SPAWN, 0, 0));
        }
        self.factions[f as usize].camp = Some(camp);
        // A body missing from the camp is a player's loss (EQUILIBRIUM item 11; rtsim cannot tell
        // an absence, MZB-260), booked once the camp is put back and before the wait is planned.
        self.renewal_bump(f, written_off, LossCause::Player, at);
        self.send_lines(f, lines, at);
        self.unblock(f, at);
    }

    // ---- CAMP_OUT --------------------------------------------------------------------------

    /// A footprint chunk dropped below FULL: the camp is out, and the lines of the epoch that ended
    /// still in flight are void (Java answers them stale). An epoch that is not the camp's is an
    /// older one's: nothing.
    pub(super) fn camp_out(&mut self, o: &CampOut, at: Tick) {
        let Some(&f) = self.by_key.get(&o.faction) else {
            return;
        };
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        if camp.epoch != o.epoch {
            return;
        }
        camp.epoch = 0;
        camp.refill = camp.refill.wrapping_add(1);
        camp.refill_at = None;
        camp.void();
        self.unblock(f, at);
    }

    /// A session starts: every camp is out and every line in flight is void, as every conflict
    /// command is per session (docs/boundary.md §9.4); the camp's next census is reconciled
    /// afresh. A feud waiting on lines is planned again.
    pub(super) fn camp_session_start(&mut self, f: u32) {
        if let Some(camp) = self.camp_mut(f) {
            camp.epoch = 0;
            camp.refill = camp.refill.wrapping_add(1);
            camp.refill_at = None;
            camp.void();
        }
        let now = self.now;
        self.unblock(f, now);
    }

    // ---- CAMP_BODIES -----------------------------------------------------------------------

    /// Java's answer to a CAMP_RECONCILE: the engine books the world fact each line reports, by
    /// the action the line carries (docs/boundary.md §9.3) and the slot's state, as it books TAKEN
    /// and DIED. *Body gone* (a take, a set-out or a removal, done or with nothing to do): home,
    /// body becomes owed; away, left becomes gone; dead, left is freed. *Body there* (a spawn, a
    /// hand-back or an adoption, done or with nothing to do): home, owed becomes home, body; away,
    /// gone becomes left, reconciled as at a census. A stale line books nothing. An answer for a
    /// generation its slot no longer holds books nothing.
    ///
    /// The action is the answer's own, not the slot's line in flight: a line voided at CAMP_OUT or
    /// at a session start is answered all the same (Java applied it before), and the slot's state
    /// may have moved on since (a party home abstractly turns left into body), so only the action
    /// says which fact the answer is. An answer is booked once, by its (op, line): one op has
    /// several records (a holder's, journaled again, and held lines answered later). Only the
    /// latest census's epoch is answered: an older epoch's answers are superseded by that census,
    /// which read the world after the lines were applied.
    pub(super) fn camp_bodies(&mut self, b: &CampBodies, at: Tick) {
        let Some(&f) = self.by_key.get(&b.faction) else {
            return;
        };
        if self.camp(f).is_none_or(|k| k.seen != b.epoch) {
            return;
        }
        // Every change of the slots follows a sync, as the lowest free number a new slot takes
        // would else depend on whether an audit (or any read) synced before a freed one.
        self.camp_sync(f, at);
        let mut camp = self.factions[f as usize].camp.take().expect("a ledger");
        // The answers a durable world save covers are forgotten as the camp next books one: a
        // camp that books many is the one that would grow (MZB-177).
        camp.forget_answers_through(self.durable_obs_seq);
        let mut lines: Vec<ReconcileLine> = Vec::new();
        let mut cleared = 0;
        let is_in = camp.is_in();
        let flags = self.line_flags(f);
        for a in &b.lines {
            if a.generation == 0 {
                // A stray, named by its UUID: nothing is booked for any slot.
                continue;
            }
            let Some(i) = camp.index(a.slot, a.generation) else {
                continue;
            };
            if !camp.answer(b.op, a.line, self.obs_seq) {
                continue;
            }
            let s = &mut camp.slots[i];
            if s.line.is_some_and(|l| l.op == b.op) {
                s.line = None;
                cleared += 1;
            }
            // A stale line books nothing (nor does an outcome this build does not know).
            if a.outcome != LINE_DONE && a.outcome != LINE_NOTHING {
                continue;
            }
            let action = a.action;
            let gone = matches!(action, ACTION_TAKE | ACTION_SET_OUT | ACTION_REMOVE);
            // An adoption with nothing to do found no stray to adopt: no body is there.
            let there = match action {
                ACTION_SPAWN | ACTION_HAND_BACK => true,
                ACTION_ADOPT => a.outcome == LINE_DONE,
                _ => false,
            };
            self.stats.camp_answers += 1;
            match (s.state, gone, there) {
                (SlotState::Body, true, _) => {
                    s.state = SlotState::Owed;
                    s.misses = 0;
                    if is_in && s.line.is_none() {
                        lines.push(ReconcileLine {
                            slot: s.id,
                            generation: s.generation,
                            body: s.body,
                            action: ACTION_SPAWN,
                            flags,
                            a: 0,
                            b: 0,
                        });
                    }
                }
                (SlotState::Left, true, _) => {
                    s.state = SlotState::Gone;
                    s.misses = 0;
                }
                (SlotState::Dead, true, _) => camp.free(i, true),
                (SlotState::Owed, _, true) => {
                    s.state = SlotState::Body;
                    s.misses = 0;
                }
                // The stray an adoption was to take is gone: the slot is still owed its body, and
                // at a camp that is in it is spawned, as a body found gone is (MOWZIE.md ledger 7).
                (SlotState::Owed, false, false) if action == ACTION_ADOPT => {
                    if is_in && s.line.is_none() {
                        lines.push(ReconcileLine {
                            slot: s.id,
                            generation: s.generation,
                            body: s.body,
                            action: ACTION_SPAWN,
                            flags,
                            a: 0,
                            b: 0,
                        });
                    }
                }
                (SlotState::Gone, _, true) => {
                    s.state = SlotState::Left;
                    s.misses = 0;
                    let phase = self.parties.get(s.party).map(|x| x.phase);
                    debug_assert!(
                        phase.is_some(),
                        "slot {} was gone in a party that is gone",
                        s.id
                    );
                    match phase {
                        Some(Phase::Mustering) => {}
                        Some(_) => {
                            if is_in && s.line.is_none() {
                                lines.push(ReconcileLine {
                                    slot: s.id,
                                    generation: s.generation,
                                    body: s.body,
                                    action: ACTION_TAKE,
                                    flags,
                                    a: 0,
                                    b: 0,
                                });
                            }
                        }
                        None => {
                            s.state = SlotState::Body;
                            s.party = Handle::NULL;
                        }
                    }
                }
                _ => {}
            }
        }
        self.factions[f as usize].camp = Some(camp);
        self.send_lines(f, lines, at);
        if cleared > 0 {
            self.unblock(f, at);
        }
    }

    // ---- CAMP_DEATH ------------------------------------------------------------------------

    /// A camp body died or was carried off (MOWZIE.md §4.9): the death of a body whose slot and
    /// generation the books hold kills that member wherever the books have it. At home the pool
    /// loses one; away its party loses that unit in any phase (a party left with no units ends
    /// through its own `home`); dead, left, the slot is freed. A stray's death books nothing.
    pub(super) fn camp_death(&mut self, w: &mut World, d: &CampDeath, at: Tick) {
        let Some(&f) = self.by_key.get(&d.faction) else {
            return;
        };
        if d.flags & DEATH_STRAY != 0 || d.generation == 0 || self.camp(f).is_none() {
            return;
        }
        self.camp_sync(f, at);
        let Some(i) = self.camp(f).and_then(|c| c.index(d.slot, d.generation)) else {
            return;
        };
        self.stats.camp_deaths += 1;
        let (state, party) = {
            let s = &self.camp(f).expect("a ledger").slots[i];
            (s.state, s.party)
        };
        // A member that died away was lost then: its body's death at the camp is no second loss.
        // The bump goes first: the `unblock` that follows plans the wait from it.
        let cause = self.renewal_cause(f, d.by);
        if state != SlotState::Dead {
            self.renewal_bump(f, 1, cause, at);
        }
        match state {
            SlotState::Body | SlotState::Owed => {
                self.camp_mut(f).expect("a ledger").free(i, true);
                self.pool_shift(f, -1.0, at);
            }
            SlotState::Dead => self.camp_mut(f).expect("a ledger").free(i, true),
            SlotState::Left | SlotState::Gone => {
                self.camp_mut(f).expect("a ledger").free(i, true);
                self.pool_shift(f, 0.0, at);
                self.member_died(w, party, d.body, at);
            }
        }
        self.unblock(f, at);
    }

    /// A member of a party died at its camp, away from it: one unit of the party falls, its slot
    /// already freed. A party with no one left ends through `home`, from its camp's muster, or
    /// walks home as `casualties` sends it.
    fn member_died(&mut self, w: &mut World, p: Handle<Party>, body: u32, at: Tick) {
        let Some(x) = self.parties.get(p) else {
            return;
        };
        let f = x.faction;
        let c = self.culture(f).clone();
        let k = x
            .units
            .iter()
            .position(|&(u, n)| n > 0 && body_id(&self.cultures, &c.unit(u).body) == body)
            .or_else(|| x.units.iter().position(|&(_, n)| n > 0));
        let Some(k) = k else {
            return;
        };
        let x = &mut self.parties[p];
        x.units[k].1 -= 1;
        let u = x.units[k].0;
        let left = x.units[k].1;
        if let Some(s) = x.spawned.iter_mut().find(|s| s.0 == u) {
            s.1 = s.1.min(left);
        }
        let x = &mut self.factions[f as usize];
        x.away = x.away.saturating_sub(1);
        self.stats.casualties += 1;
        let x = &self.parties[p];
        if x.alive() > 0 {
            return;
        }
        // As `casualties` ends a band with no one left: through its walk home and its own `home`.
        match x.phase {
            Phase::Home => {}
            Phase::Fighting => self.end_fight(w, p, boundary::RECALL_ROUTED, at),
            Phase::Mustering | Phase::Out | Phase::Waiting => {
                self.go_home(w, p, boundary::RECALL_ROUTED, at)
            }
        }
    }

    // ---- the audit -------------------------------------------------------------------------

    /// Every way a camp's ledger breaks the engine's side of §4.9's invariants at `at` (empty when
    /// it holds): I1 (each slot in one state, one line each, numbers and generations unique and
    /// nonzero), I2 (the slots at home are no more than the pool's whole warriors and no fewer
    /// than it held when it was last moved, the difference being owed slots the next sync adds;
    /// the slots away the war parties' units; the cap kept), and
    /// the books' own bookkeeping (`away`, the parties). A camp with no ledger has none to break.
    /// It changes nothing: a sync here would number the slots by when the audit ran, so an
    /// audited run would differ from an unaudited one.
    pub fn camp_audit(&self, f: u32, at: Tick) -> Vec<String> {
        let Some(c) = self.camp(f) else {
            return Vec::new();
        };
        let x = &self.factions[f as usize];
        let mut bad = Vec::new();
        let whole = self.pool_at(f, at).floor() as u32;
        if c.home_count() > whole {
            bad.push(format!(
                "I2: {} slots at home, the pool holds {whole}",
                c.home_count()
            ));
        }
        // The pool's stored value is what it held when it was last moved, and every move of it
        // follows a sync and moves the slots with it: at least that many are at home, the refill
        // since being the gain the next sync adds.
        let held = x.pool.floor() as u32;
        if c.home_count() < held {
            bad.push(format!(
                "I2: {} slots at home, the pool held {held} when it was last moved",
                c.home_count()
            ));
        }
        let parties: u32 = self
            .parties
            .iter()
            .filter(|(_, p)| p.faction == f && p.kind.is_war())
            .map(|(_, p)| p.alive())
            .sum();
        if c.away_count() != parties || x.away != parties {
            bad.push(format!(
                "I2: {} slots away, {} counted away, the parties hold {parties}",
                c.away_count(),
                x.away
            ));
        }
        if whole + x.away > x.cap {
            bad.push(format!(
                "ledger 6: pool {whole} and {} away over the cap {}",
                x.away, x.cap
            ));
        }
        for (k, s) in c.slots.iter().enumerate() {
            if s.generation == 0 {
                bad.push(format!("I1: slot {} has generation 0", s.id));
            }
            if k > 0 && c.slots[k - 1].id >= s.id {
                bad.push(format!("I1: slot {} twice or out of order", s.id));
            }
            if s.state.away() != !s.party.eq(&Handle::NULL) && s.state != SlotState::Dead {
                bad.push(format!(
                    "I1: slot {} {:?} with a wrong party",
                    s.id, s.state
                ));
            }
            if s.state.away() && !self.parties.contains(s.party) {
                bad.push(format!("I1: slot {} is in a party that is gone", s.id));
            }
            if s.line.is_some() && !c.is_in() {
                bad.push(format!(
                    "ledger 7: slot {} has a line in flight with its camp out",
                    s.id
                ));
            }
        }
        bad
    }
}
